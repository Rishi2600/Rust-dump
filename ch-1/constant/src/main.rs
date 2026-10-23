use std::sync::atomic::{AtomicBool, Ordering};
use std::cell::UnsafeCell;

pub struct LazyInit<T> {
    data: UnsafeCell<Option<T>>,
    initialized: AtomicBool,
}

unsafe impl<T: Send + Sync> Sync for LazyInit<T> {}

impl<T> LazyInit<T> {
    pub const fn new() -> Self {
        Self {
            data: UnsafeCell::new(None),
            initialized: AtomicBool::new(false),
        }
    }
}