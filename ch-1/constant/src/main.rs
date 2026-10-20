use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct RingBuffer<T, const CAP: usize> {
    buffer: [UnsafeCell<Option<T>>; CAP],
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<T: Send, const CAP: usize> Sync for RingBuffer<T, CAP> {}

impl<T, const CAP: usize> RingBuffer<T, CAP> {
    pub fn new() -> Arc<Self> {
        let buffer = std::array::from_fn(|_| UnsafeCell::new(None));
        Arc::new(Self {
            buffer,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        })
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head.wrapping_add(1) % CAP == tail % CAP {
            return Err(value); // Buffer full
        }

        unsafe {
            *self.buffer[head % CAP].get() = Some(value);
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        if tail % CAP == head % CAP {
            return None; // Buffer empty
        }

        let val = unsafe { (*self.buffer[tail % CAP].get()).take() };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        val
    }
}