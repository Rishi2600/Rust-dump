use std::sync::atomic::{AtomicUsize, Ordering};

pub struct SpscQueue<T, const CAP: usize> {
    buffer: [std::cell::UnsafeCell<Option<T>>; CAP],
    head: AtomicUsize,
    tail: AtomicUsize,
}