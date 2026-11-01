use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

pub struct RcuCell<T> {
    ptr: AtomicPtr<T>,
}

impl<T> RcuCell<T> {
    pub fn new(val: T) -> Self {
        Self { ptr: AtomicPtr::new(Box::into_raw(Box::new(val))) }
    }
    pub fn update(&self, new_val: T) {
        let new_ptr = Box::into_raw(Box::new(new_val));
        let old_ptr = self.ptr.swap(new_ptr, Ordering::AcqRel);
        unsafe { let _ = Box::from_raw(old_ptr); }
    }
}