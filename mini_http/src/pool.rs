use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce() + Send + 'static>;

/// A fixed set of threads that take turns running submitted jobs.
///
/// Starting a thread for every connection is simple, but then a burst of
/// clients means a burst of threads, each with its own stack. A pool puts a
/// ceiling on that: jobs wait in a queue until one of the workers is free.
pub struct ThreadPool {
    sender: Option<Sender<Job>>,
    workers: Vec<JoinHandle<()>>,
}

impl ThreadPool {
    /// Start `size` worker threads (at least one).
    pub fn new(size: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        // A channel has one receiving end. Sharing it between the workers
        // takes a lock, so that each job is handed to exactly one of them.
        let receiver = Arc::new(Mutex::new(receiver));
        let workers = (0..size.max(1)).map(|_| {
            let receiver = Arc::clone(&receiver);
            thread::spawn(move || work(&receiver))
        });
        Self {
            sender: Some(sender),
            workers: workers.collect(),
        }
    }

    pub fn size(&self) -> usize {
        self.workers.len()
    }

    /// Queue a job. It runs as soon as a worker is free.
    pub fn execute(&self, job: impl FnOnce() + Send + 'static) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(Box::new(job));
        }
    }
}

/// What each worker does: wait for a job, run it, repeat.
fn work(receiver: &Mutex<Receiver<Job>>) {
    loop {
        // The lock is held while waiting for a job and released before the
        // job runs, so the other workers are not blocked by a slow one.
        let next = match receiver.lock() {
            Ok(queue) => queue.recv(),
            Err(_) => return,
        };
        match next {
            // A job that panics must not cost the pool a worker.
            Ok(job) => drop(panic::catch_unwind(AssertUnwindSafe(job))),
            // The sending end is gone: the pool is shutting down.
            Err(_) => return,
        }
    }
}

impl Drop for ThreadPool {
    /// Finish the queued jobs, then stop.
    fn drop(&mut self) {
        // Closing the channel is the signal. Each worker sees it once the
        // queue is empty and returns.
        drop(self.sender.take());
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn every_job_runs_before_the_pool_is_gone() {
        let done = Arc::new(AtomicUsize::new(0));
        let pool = ThreadPool::new(4);
        assert_eq!(pool.size(), 4);
        for _ in 0..100 {
            let done = Arc::clone(&done);
            pool.execute(move || {
                thread::sleep(Duration::from_millis(1));
                done.fetch_add(1, Ordering::SeqCst);
            });
        }
        // Dropping the pool waits for the queue to drain.
        drop(pool);
        assert_eq!(done.load(Ordering::SeqCst), 100);
    }

    #[test]
    fn jobs_really_run_at_the_same_time() {
        // Each job waits until three of them have arrived at the barrier.
        // With fewer than three threads running at once, none could pass.
        let barrier = Arc::new(Barrier::new(3));
        let passed = Arc::new(AtomicUsize::new(0));
        let pool = ThreadPool::new(3);
        for _ in 0..3 {
            let (barrier, passed) = (Arc::clone(&barrier), Arc::clone(&passed));
            pool.execute(move || {
                barrier.wait();
                passed.fetch_add(1, Ordering::SeqCst);
            });
        }
        drop(pool);
        assert_eq!(passed.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn a_panicking_job_does_not_cost_the_pool_a_worker() {
        let done = Arc::new(AtomicUsize::new(0));
        // One worker: if the panic killed it, the second job would never run.
        let pool = ThreadPool::new(1);
        pool.execute(|| panic!("job bug (this panic is expected by the test)"));
        let counter = Arc::clone(&done);
        pool.execute(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        drop(pool);
        assert_eq!(done.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_pool_always_has_at_least_one_worker() {
        assert_eq!(ThreadPool::new(0).size(), 1);
    }
}
