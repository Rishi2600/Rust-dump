use std::sync::atomic::{AtomicPtr, Ordering};
use std::ptr;

struct Node<T> { data: T, next: *mut Node<T> }

pub struct TreiberStack<T> {
    head: AtomicPtr<Node<T>>,
}

impl<T> TreiberStack<T> {
    pub fn new() -> Self { Self { head: AtomicPtr::new(ptr::null_mut()) } }
    pub fn push(&self, data: T) {
        let new_node = Box::into_raw(Box::new(Node { data, next: ptr::null_mut() }));
        let mut old = self.head.load(Ordering::Relaxed);
        loop {
            unsafe { (*new_node).next = old; }
            match self.head.compare_exchange_weak(old, new_node, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => break,
                Err(current) => old = current,
            }
        }
    }
}