use std::sync::{Mutex, Condvar};

pub struct Semaphore {
    permits: Mutex<usize>,
    condvar: Condvar,
}