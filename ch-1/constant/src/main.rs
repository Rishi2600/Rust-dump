use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

pub type TaskFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub struct TaskQueue {
    queue: Mutex<VecDeque<TaskFuture>>,
}

impl TaskQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { queue: Mutex::new(VecDeque::new()) })
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.queue.lock().unwrap().push_back(Box::pin(future));
    }

    pub fn run_next(self: &Arc<Self>) -> bool {
        let mut task = match self.queue.lock().unwrap().pop_front() {
            Some(t) => t,
            None => return false,
        };

        let waker = dummy_waker();
        let mut cx = Context::from_waker(&waker);
        if let Poll::Pending = task.as_mut().poll(&mut cx) {
            self.queue.lock().unwrap().push_back(task);
        }
        true
    }
}

fn dummy_waker() -> Waker {
    unsafe fn no_op(_: *const ()) {}
    unsafe fn clone(_: *const ()) -> RawWaker { raw_waker() }
    unsafe fn raw_waker() -> RawWaker {
        RawWaker::new(std::ptr::null(), &RawWakerVTable::new(clone, no_op, no_op, no_op))
    }
    unsafe { Waker::from_raw(raw_waker()) }
}