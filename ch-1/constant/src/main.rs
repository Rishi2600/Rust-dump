use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Condvar};

pub struct TaskPool<T> {
    tasks: Mutex<VecDeque<T>>,
    condvar: Condvar,
}

impl<T> TaskPool<T> {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { tasks: Mutex::new(VecDeque::new()), condvar: Condvar::new() })
    }
    pub fn push(&self, task: T) {
        self.tasks.lock().unwrap().push_back(task);
        self.condvar.notify_one();
    }
}