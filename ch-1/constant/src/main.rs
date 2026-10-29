use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;

pub struct Seqlock<T> {
    sequence: AtomicUsize,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Seqlock<T> {}

impl<T: Copy> Seqlock<T> {
    pub fn new(val: T) -> Self {
        Self { sequence: AtomicUsize::new(0), data: UnsafeCell::new(val) }
    }

    pub fn read(&self) -> T {
        loop {
            let s1 = self.sequence.load(Ordering::Acquire);
            if s1 & 1 != 0 { continue; } // Writer is currently updating
            let val = unsafe { *self.data.get() };
            let s2 = self.sequence.load(Ordering::Acquire);
            if s1 == s2 { return val; } // No concurrent write occurred
        }
    }

    pub fn write(&self, val: T) {
        let s = self.sequence.load(Ordering::Relaxed);
        self.sequence.store(s.wrapping_add(1), Ordering::Release); // Odd = writing
        unsafe { *self.data.get() = val; }
        self.sequence.store(s.wrapping_add(2), Ordering::Release); // Even = done
    }
}