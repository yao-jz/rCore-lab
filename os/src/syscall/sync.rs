use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}

// LAB5 HINT: you might need to maintain data structures used for deadlock detection
// during sys_mutex_* and sys_semaphore_* syscalls
pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();

    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        process_inner.mutex_available[id] = 1;
        for i in 0..process_inner.mutex_allocation.len() {
            process_inner.mutex_allocation[i][id] = 0;
            process_inner.mutex_need[i][id] = 0;
        }
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_available.push(1);
        for i in 0..process_inner.mutex_allocation.len() {
            process_inner.mutex_allocation[i].push(0);
            process_inner.mutex_need[i].push(0);
        }
        process_inner.mutex_list.len() as isize - 1
    }
    
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid as usize;
    if process_inner.mutex_available[mutex_id] == 1 {
        process_inner.mutex_available[mutex_id] = 0;
        process_inner.mutex_allocation[tid][mutex_id] = 1;
    } else {
        process_inner.mutex_need[tid][mutex_id] = 1;
        if process_inner.deadlock_detect == true {
            let res = process_inner.check_deadlock_mutex();
            if res != 0{
                drop(process_inner);
                drop(process);
                return res;
            }
        }
    }
    drop(process_inner);
    drop(process);
    mutex.lock();
    0
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid as usize;
    let next_queue_id = mutex.get_next_queue_id();
    if next_queue_id == -1 { // 队列内没有东西
        process_inner.mutex_available[mutex_id] += 1;
        process_inner.mutex_allocation[tid][mutex_id] = 0;
    } else { // 队列非空
        process_inner.mutex_allocation[tid][mutex_id] = 0;
        process_inner.mutex_allocation[next_queue_id as usize][mutex_id] = 1;
        process_inner.mutex_need[next_queue_id as usize][mutex_id] = 0;
    }
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        process_inner.semaphore_available[id] = res_count;
        for i in 0..process_inner.semaphore_allocation.len() {
            process_inner.semaphore_allocation[i][id] = 0;
            process_inner.semaphore_need[i][id] = 0;
        }
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_available.push(res_count);
        for i in 0..process_inner.semaphore_allocation.len() {
            process_inner.semaphore_allocation[i].push(0);
            process_inner.semaphore_need[i].push(0);
        }
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid as usize;
    let next_queue_id = sem.get_next_queue_id();
    if next_queue_id == -1 { // 队列内没有东西
        process_inner.semaphore_available[sem_id] += 1;
        process_inner.semaphore_allocation[tid][sem_id] -= 1;
    } else { // 队列非空
        process_inner.semaphore_allocation[tid][sem_id] -= 1;
        process_inner.semaphore_allocation[next_queue_id as usize][sem_id] += 1;
        process_inner.semaphore_need[next_queue_id as usize][sem_id] -= 1;
    }
    drop(process_inner);
    drop(process);
    sem.up();
    0
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid as usize;
    if process_inner.semaphore_available[sem_id] > 0 {
        process_inner.semaphore_available[sem_id] -= 1;
        process_inner.semaphore_allocation[tid][sem_id] += 1;
    } else {
        process_inner.semaphore_need[tid][sem_id] += 1;
        if process_inner.deadlock_detect == true {
            let res = process_inner.check_deadlock_semaphore();
            if res != 0{
                drop(process_inner);
                drop(process);
                return res;
            }
        }
    }
    drop(process_inner);
    drop(process);
    sem.down();
    0
}

pub fn sys_condvar_create(_arg: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}

// LAB5 YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    if _enabled == 1 {
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        process_inner.deadlock_detect = true;
        drop(process_inner);
        0
    } else if _enabled == 0 {
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        process_inner.deadlock_detect = false;
        drop(process_inner);
        0
    } else {
        -1
    }
}
