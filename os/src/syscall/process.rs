//! Process management syscalls

use crate::config::{MAX_SYSCALL_NUM, PAGE_SIZE};
use crate::task::{exit_current_and_run_next, suspend_current_and_run_next, TaskStatus, get_current_block_status,get_current_block_syscall_times,get_current_block_start_time, TASK_MANAGER};
use crate::timer::get_time_us;
use crate::mm::{VirtAddr,VirtPageNum,translated_byte_buffer,MapPermission,MapArea,MapType,VPNRange};
use crate::task::current_user_token;

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
    info!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
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

// CLUE: 从 ch4 开始不再对调度算法进行测试~
pub fn sys_set_priority(_prio: isize) -> isize {
    -1
}

// YOUR JOB: 扩展内核以实现 sys_mmap 和 sys_munmap
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    //_port 第 0 位表示是否可读，第 1 位表示是否可写，第 2 位表示是否可执行。其他位无效且必须为 0
    if _start % PAGE_SIZE != 0 || _port & !0x7 != 0 || _port & 0x7 == 0 {
        return -1;
    }
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    let mut this_permission = MapPermission::U | MapPermission::from_bits((_port << 1) as u8).unwrap();
    let vpn_range = VPNRange::new(VirtAddr::from(_start).floor(), VirtAddr::from(_start+_len).ceil());
    for vpn in vpn_range {
        if let Some(pte) = inner.tasks[current].memory_set.page_table.find_pte(vpn) {
            if pte.is_valid() {
                return -1;
            }
        } else {
        }
    }
    inner.tasks[current].memory_set.insert_framed_area(VirtAddr::from(_start), VirtAddr::from(_start+_len), this_permission);
    0
}

pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    if _start % PAGE_SIZE != 0 {
        return -1;
    }
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    let start_vpn: VirtPageNum = VirtAddr::from(_start).floor();
    let end_vpn: VirtPageNum = VirtAddr::from(_start+_len).ceil();
    let vpn_range = VPNRange::new(start_vpn, end_vpn);
    for vpn in vpn_range {
        if let Some(pte) = inner.tasks[current].memory_set.page_table.find_pte(vpn) {
            if !pte.is_valid() {
                return -1;
            }
        } else{
            return -1;
        }
    }
    for vpn in vpn_range {
        inner.tasks[current].memory_set.page_table.unmap(vpn);
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
