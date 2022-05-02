//! Process management syscalls

use crate::mm::{translated_ref};
use crate::loader::get_app_data_by_name;
use crate::mm::{translated_refmut, translated_str,translated_byte_buffer,MapPermission};
use crate::mm::{VPNRange,VirtPageNum,MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE};
use crate::config::PAGE_SIZE;
use crate::task::{
    add_task, current_task, current_user_token, exit_current_and_run_next,TaskContext,take_current_task,
    suspend_current_and_run_next, TaskStatus, pid_alloc, PidHandle,TaskControlBlockInner,
    get_current_block_status,get_current_block_syscall_times,get_current_block_start_time, KernelStack, TaskControlBlock
};
use crate::fs::{open_file, OpenFlags};
use crate::config::{TRAP_CONTEXT,BIG_STRIDE};
use crate::sync::UPSafeCell;
use crate::timer::get_time_us;
use crate::trap::{trap_handler, TrapContext};
use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::config::MAX_SYSCALL_NUM;
use alloc::string::String;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[derive(Clone, Copy)]
pub struct TaskInfo {
    pub status: TaskStatus,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    pub time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    debug!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().pid.0 as isize
}

/// Syscall Fork which returns 0 for child process and child_pid for parent process
pub fn sys_fork() -> isize {
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

/// Syscall Exec which accepts the elf path
pub fn sys_exec(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}


/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    // find a child process

    // ---- access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB lock exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after removing from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child TCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB lock automatically
}

// YOUR JOB: 引入虚地址后重写 sys_get_time
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    let _us = get_time_us();
    let ts = translated_byte_buffer(current_user_token(), _ts as *const u8, core::mem::size_of::<TimeVal>());
    let mut now_byte = 0;
    let time_val = &TimeVal {
        sec: _us / 1_000_000,
        usec: _us % 1_000_000,
    };
    let t = (time_val as *const TimeVal) as usize;
    for i in ts {
        let len = i.len();
        unsafe {
            i.copy_from_slice(core::slice::from_raw_parts_mut((t+now_byte)as *mut u8, len));
        }
        now_byte += len;
    }
    0
}

// YOUR JOB: 引入虚地址后重写 sys_task_info
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    let this_time = get_time_us();
    let ti = translated_byte_buffer(current_user_token(), ti as *const u8, core::mem::size_of::<TaskInfo>());
    let task_info = &TaskInfo {
        status: get_current_block_status(),
        syscall_times: get_current_block_syscall_times(),
        time: (this_time - get_current_block_start_time())/1000 
    };
    let mut now_byte = 0;
    let t = (task_info as *const TaskInfo) as usize;
    for i in ti {
        let len = i.len();
        unsafe{
            i.copy_from_slice(core::slice::from_raw_parts_mut((t+now_byte)as *mut u8, len));
        }
        now_byte += len;
    }
    0
}

// YOUR JOB: 实现sys_set_priority，为任务添加优先级
pub fn sys_set_priority(_prio: isize) -> isize {
    if _prio >= 2 {
        let task = current_task().unwrap();
        task.set_priority(_prio);
        _prio
    } else {
        -1
    }
    // -1
}

// YOUR JOB: 扩展内核以实现 sys_mmap 和 sys_munmap
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    if _start % PAGE_SIZE != 0 || _port & !0x7 != 0 || _port & 0x7 == 0 {
        return -1;
    }
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    let this_permission = MapPermission::U | MapPermission::from_bits((_port << 1) as u8).unwrap();
    let vpn_range = VPNRange::new(VirtAddr::from(_start).floor(), VirtAddr::from(_start+_len).ceil());
    for vpn in vpn_range {
        if let Some(pte) = inner.memory_set.page_table.find_pte(vpn) {
            if pte.is_valid() {
                return -1;
            }
        } else {
        }
    }
    inner.memory_set.insert_framed_area(VirtAddr::from(_start), VirtAddr::from(_start+_len), this_permission);
    0
}

pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    if _start % PAGE_SIZE != 0 {
        return -1;
    }
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    let start_vpn: VirtPageNum = VirtAddr::from(_start).floor();
    let end_vpn: VirtPageNum = VirtAddr::from(_start+_len).ceil();
    let vpn_range = VPNRange::new(start_vpn, end_vpn);
    for vpn in vpn_range {
        if let Some(pte) = inner.memory_set.page_table.find_pte(vpn) {
            if !pte.is_valid() {
                return -1;
            }
        } else{
            return -1;
        }
    }
    for vpn in vpn_range {
        inner.memory_set.page_table.unmap(vpn);
    }
    0
}

//
// YOUR JOB: 实现 sys_spawn 系统调用
// ALERT: 注意在实现 SPAWN 时不需要复制父进程地址空间，SPAWN != FORK + EXEC 
pub fn sys_spawn(_path: *const u8) -> isize {
    let task = current_task().unwrap();
    // let mut parent_inner = task.inner_exclusive_access();
    let token = current_user_token();
    let path = translated_str(token, _path);
    if let Some(data) = get_app_data_by_name(path.as_str()){
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(data);
        let trap_cx_ppn = memory_set
                .translate(VirtAddr::from(TRAP_CONTEXT).into())
                .unwrap()
                .ppn();
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: task.inner_exclusive_access().base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(&task)),
                    children: Vec::new(),
                    exit_code: 0,
                    syscall_times: [0;MAX_SYSCALL_NUM],
                    start_time: 0,
                    stride: 0,
                    priority: 16
                })
            },
        });
        task.inner_exclusive_access().children.push(task_control_block.clone());
        
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();

        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        // trap_cx.x[10] = 0;
        let pid = task_control_block.pid.0;
        add_task(task_control_block);
        pid as isize
    } else {
        -1
    }
    // 1
}
