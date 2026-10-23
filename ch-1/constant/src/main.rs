use std::sync::atomic::{AtomicIsize, Ordering};

pub struct ChaseLevDeque<T> {
    head: AtomicIsize,
    tail: AtomicIsize,
    _marker: std::marker::PhantomData<T>,
}

impl<T> ChaseLevDeque<T> {
    pub fn new() -> Self {
        Self {
            head: AtomicIsize::new(0),
            tail: AtomicIsize::new(0),
            _marker: std::marker::PhantomData,
        }
    }
}