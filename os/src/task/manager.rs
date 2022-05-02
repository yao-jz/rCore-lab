//! Implementation of [`TaskManager`]
//!
//! It is only used to manage processes and schedule process based on ready queue.
//! Other CPU process monitoring functions are in Processor.


use super::{TaskControlBlock, TaskStatus};
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
use crate::config::BIG_STRIDE;

pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

// YOUR JOB: FIFO->Stride
/// A simple FIFO scheduler.
impl TaskManager {
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        let mut index = 0;
        let mut min_stride = 0x7FFFFFFF;
        for i in 0..self.ready_queue.len() {
            if let Some(task) = self.ready_queue.get_mut(i) {
                let inner = task.inner_exclusive_access();
                if inner.task_status == TaskStatus::Ready {
                    if inner.stride < min_stride {
                        index = i;
                        min_stride = inner.stride;
                    }
                } else {
                    continue;
                }
            }
        }
        if let Some(task) = self.ready_queue.get(index) {
            let mut inner = task.inner_exclusive_access();
            inner.stride += BIG_STRIDE / inner.priority;
        }
        // self.ready_queue.pop_front()
        self.ready_queue.remove(index)
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}
