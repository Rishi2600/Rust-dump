use std::sync::atomic::{AtomicBool, Ordering};
use std::cell::UnsafeCell;

pub struct LazyCell<T> {
    value: UnsafeCell<Option<T>>,
    init: AtomicBool,
}
unsafe impl<T: Send + Sync> Sync for LazyCell<T> {}