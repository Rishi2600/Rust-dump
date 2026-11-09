use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

pub struct ConfigSnapshot<T> {
    current: AtomicPtr<T>,
}

impl<T> ConfigSnapshot<T> {
    pub fn new(val: T) -> Self {
        let boxed = Box::into_raw(Box::new(val));
        Self { current: AtomicPtr::new(boxed) }
    }

    pub fn update(&self, new_val: T) {
        let new_ptr = Box::into_raw(Box::new(new_val));
        let old_ptr = self.current.swap(new_ptr, Ordering::AcqRel);
        unsafe { let _ = Box::from_raw(old_ptr); }
    }
}