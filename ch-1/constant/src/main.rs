use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

pub struct RcuCell<T> {
    ptr: AtomicPtr<T>,
}

impl<T> RcuCell<T> {
    pub fn new(val: T) -> Self {
        let heap_val = Box::into_raw(Box::new(val));
        Self { ptr: AtomicPtr::new(heap_val) }
    }

    pub fn read(&self) -> Arc<T> {
        unsafe {
            let raw = self.ptr.load(Ordering::Acquire);
            // Re-create temporary Arc clone reference safely
            let temp = Arc::from_raw(raw);
            let clone = Arc::clone(&temp);
            let _ = Arc::into_raw(temp); // Keep original pointer valid
            clone
        }
    }

    pub fn update(&self, new_val: T) {
        let new_raw = Box::into_raw(Box::new(new_val));
        let old_raw = self.ptr.swap(new_raw, Ordering::AcqRel);
        unsafe {
            let _ = Box::from_raw(old_raw); // Free old allocation safely
        }
    }
}