use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

pub struct WorkQueue<T> {
    queue: Mutex<VecDeque<T>>,
    cond: Condvar,
}

impl<T> WorkQueue<T> {
    pub fn new() -> Self {
        Self { queue: Mutex::new(VecDeque::new()), cond: Condvar::new() }
    }

    pub fn push(&self, item: T) {
        let mut q = self.queue.lock().unwrap();
        q.push_back(item);
        self.cond.notify_one();
    }

    pub fn pop(&self) -> T {
        let mut q = self.queue.lock().unwrap();
        while q.is_empty() {
            q = self.cond.wait(q).unwrap();
        }
        q.pop_front().unwrap()
    }
}