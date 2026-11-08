use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct FastSpsc<T, const CAP: usize> {
    buffer: [UnsafeCell<Option<T>>; CAP],
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<T: Send, const CAP: usize> Sync for FastSpsc<T, CAP> {}

impl<T, const CAP: usize> FastSpsc<T, CAP> {
    pub fn new() -> Self {
        Self {
            buffer: std::array::from_fn(|_| UnsafeCell::new(None)),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
}