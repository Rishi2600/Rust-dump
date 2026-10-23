use std::cell::Cell;
use std::ptr::NonNull;

struct RcBox<T> {
    value: T,
    strong: Cell<usize>,
    weak: Cell<usize>,
}

pub struct CustomRc<T> {
    ptr: NonNull<RcBox<T>>,
}

impl<T> CustomRc<T> {
    pub fn new(value: T) -> Self {
        let boxed = Box::new(RcBox {
            value,
            strong: Cell::new(1),
            weak: Cell::new(1),
        });
        Self { ptr: NonNull::new(Box::into_raw(boxed)).unwrap() }
    }
}