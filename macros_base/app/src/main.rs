use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct RingBuffer<const N: usize> {
    buf: UnsafeCell<[u8; N]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<const N: usize> Sync for RingBuffer<N> {}

impl<const N: usize> RingBuffer<N> {
    pub fn push(&self, byte: u8) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next = (tail + 1) % N;
        if next == self.head.load(Ordering::Acquire) {
            return false; // Full
        }
        unsafe { (*self.buf.get())[tail] = byte };
        self.tail.store(next, Ordering::Release);
        true
    }
}