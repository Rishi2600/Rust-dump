use std::cell::Cell;
use std::ptr::NonNull;

pub struct IntrusiveLink<T> {
    pub value: T,
    pub next: Cell<Option<NonNull<IntrusiveLink<T>>>>,
}

impl<T> IntrusiveLink<T> {
    pub fn new(value: T) -> Self {
        Self { value, next: Cell::new(None) }
    }
}