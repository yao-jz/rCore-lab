use crate::sync::UPSafeCell;
use crate::task::{add_task, block_current_and_run_next, current_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc};

pub struct Semaphore {
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Semaphore {
    pub fn get_next_queue_id(&self) -> isize {
        let inner = self.inner.exclusive_access();
        if inner.wait_queue.len() < 1 {
            -1
        } else {
            if let Some(waking_task) = inner.wait_queue.front() {
                waking_task.inner_exclusive_access().res.as_ref().unwrap().tid as isize
            } else {
                -1
            }
        }
    }
    pub fn new(res_count: usize) -> Self {
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    pub fn up(&self) {
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                add_task(task);
            }
        }
    }

    pub fn down(&self) {
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
        }
    }
}
