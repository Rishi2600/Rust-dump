use std::sync::mpsc;
use std::thread;

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct SimpleThreadPool {
    workers: Vec<Option<thread::JoinHandle<()>>>,
    sender: Option<mpsc::Sender<Job>>,
}

impl Drop for SimpleThreadPool {
    fn drop(&mut self) {
        drop(self.sender.take()); // Close channel signaling threads to exit loop
        for worker in &mut self.workers {
            if let Some(thread) = worker.take() {
                thread.join().unwrap();
            }
        }
    }
}