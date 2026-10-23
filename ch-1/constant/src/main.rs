use std::sync::atomic::{AtomicPtr, Ordering};
use std::ptr;

struct Node<T> {
    data: T,
    next: *mut Node<T>,
}

pub struct TreiberStack<T> {
    head: AtomicPtr<Node<T>>,
}

impl<T> TreiberStack<T> {
    pub fn new() -> Self {
        Self { head: AtomicPtr::new(ptr::null_mut()) }
    }

    pub fn push(&self, data: T) {
        let new_node = Box::into_raw(Box::new(Node { data, next: ptr::null_mut() }));
        let mut old_head = self.head.load(Ordering::Relaxed);
        loop {
            unsafe { (*new_node).next = old_head; }
            match self.head.compare_exchange_weak(
                old_head,
                new_node,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => old_head = current,
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        let mut old_head = self.head.load(Ordering::Acquire);
        loop {
            if old_head.is_null() { return None; }
            let next = unsafe { (*old_head).next };
            match self.head.compare_exchange_weak(
                old_head,
                next,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let boxed = unsafe { Box::from_raw(old_head) };
                    return Some(boxed.data);
                }
                Err(current) => old_head = current,
            }
        }
    }
}