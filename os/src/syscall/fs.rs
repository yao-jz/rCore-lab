//! File and filesystem-related syscalls

use core::panic;

use crate::fs::OSInode;
use crate::mm::translated_byte_buffer;
use crate::mm::translated_str;
use crate::mm::translated_refmut;
use crate::task::current_user_token;
use crate::task::current_task;
use crate::fs::open_file;
use crate::fs::OpenFlags;
use core::any::Any;
use crate::fs::{Stat,StatMode};
use crate::mm::UserBuffer;
use alloc::sync::Arc;
use crate::fs::ROOT_INODE;

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(
            UserBuffer::new(translated_byte_buffer(token, buf, len))
        ) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.read(
            UserBuffer::new(translated_byte_buffer(token, buf, len))
        ) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(
        path.as_str(),
        OpenFlags::from_bits(flags).unwrap()
    ) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

// YOUR JOB: 扩展 easy-fs 和内核以实现以下三个 syscall
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    let task = current_task().unwrap();
    // let mut inner = task.inner_exclusive_access();
    if _fd >= task.inner_exclusive_access().fd_table.len() {
        return -1;
    }
    if task.inner_exclusive_access().fd_table[_fd].is_none() {
        return -1;
    }
    let mut ino = 0 as u64;
    let mut nlink = 0 as u32;
    if let Some(inode) = &task.inner_exclusive_access().fd_table[_fd] {
        let it: &dyn Any = inode.as_any();
        let i = match it.downcast_ref::<OSInode>() {
            Some(i) => i,
            None => panic!(),
        };
        ino = i.get_inode_id();
        let inner = i.inner.exclusive_access();
        nlink = ROOT_INODE.get_link_num(inner.inode.block_id, inner.inode.block_offset) as u32;
    } else {
        return -1;
    }
    let status = &Stat{
        dev: 0,
        ino: ino,
        mode: StatMode::FILE,
        nlink: nlink,
        pad: [0 as u64; 7],
    };
    let st = translated_byte_buffer(current_user_token(), _st as *const u8, core::mem::size_of::<Stat>());
    let mut now_byte = 0;
    let t = (status as *const Stat) as usize;
    for i in st {
        let len = i.len();
        unsafe{
            i.copy_from_slice(core::slice::from_raw_parts_mut((t+now_byte)as *mut u8, len));
        }
        now_byte += len;
    }
    0
}

pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    let token = current_user_token();
    let old_name = translated_str(token, _old_name);
    let new_name = translated_str(token, _new_name);
    println!("link old name is {} new name is {}", old_name.as_str(), new_name.as_str());
    if old_name.as_str() != new_name.as_str() {
        if let Some(_) = ROOT_INODE.linknode(old_name.as_str(), new_name.as_str()) {
            for app in ROOT_INODE.ls() {
                println!("{}", app);
            }
            0
        } else {
            -1
        }
    } else {
        -1
    }
}

pub fn sys_unlinkat(_name: *const u8) -> isize {
    let token = current_user_token();
    let name = translated_str(token, _name);
    if let Some(inode) = ROOT_INODE.find(name.as_str()) {
        if ROOT_INODE.get_link_num(inode.block_id, inode.block_offset) > 1 {
            // 删除链接
            return ROOT_INODE.unlink(name.as_str());
        } else {
            // 删除文件
            inode.clear();
            return ROOT_INODE.unlink(name.as_str());
        }
    } else {
        -1 // 文件不存在
    }
}
