use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;

pub struct Seqlock<T> {
    seq: AtomicUsize,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Seqlock<T> {}

impl<T: Copy> Seqlock<T> {
    pub fn new(val: T) -> Self { Self { seq: AtomicUsize::new(0), data: UnsafeCell::new(val) } }
    pub fn read(&self) -> T {
        loop {
            let s1 = self.seq.load(Ordering::Acquire);
            if s1 & 1 != 0 { continue; }
            let val = unsafe { *self.data.get() };
            let s2 = self.seq.load(Ordering::Acquire);
            if s1 == s2 { return val; }
        }
    }
}