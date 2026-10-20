use std::sync::atomic::{AtomicPtr, Ordering};
use std::thread_local;

pub struct HazardPointer {
    ptr: AtomicPtr<()>,
}

thread_local! {
    static HAZARD_RESERVOIR: HazardPointer = HazardPointer {
        ptr: AtomicPtr::new(std::ptr::null_mut()),
    };
}

pub struct HpGuard<T> {
    ptr: *mut T,
}

impl<T> HpGuard<T> {
    pub fn protect(atomic: &AtomicPtr<T>) -> Option<Self> {
        let ptr = atomic.load(Ordering::Relaxed);
        HAZARD_RESERVOIR.with(|hp| {
            hp.ptr.store(ptr as *mut (), Ordering::Release);
        });

        // Double check against racing deletion
        if atomic.load(Ordering::Acquire) != ptr {
            None
        } else {
            Some(HpGuard { ptr })
        }
    }
}

impl<T> Drop for HpGuard<T> {
    fn drop(&mut self) {
        HAZARD_RESERVOIR.with(|hp| {
            hp.ptr.store(std::ptr::null_mut(), Ordering::Release);
        });
    }
}