use std::sync::atomic::{AtomicU64, Ordering};

pub struct SequenceGenerator {
    counter: AtomicU64,
}

impl SequenceGenerator {
    pub fn new() -> Self {
        Self { counter: AtomicU64::new(0) }
    }

    pub fn next_id(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }
}