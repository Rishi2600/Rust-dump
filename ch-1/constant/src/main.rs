use std::sync::atomic::{AtomicUsize, Ordering};

pub struct SpscRing<T, const CAP: usize> {
    head: AtomicUsize,
    tail: AtomicUsize,
}