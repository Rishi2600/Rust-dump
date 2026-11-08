use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct ConnectionTracker {
    active_count: Arc<AtomicUsize>,
}

impl ConnectionTracker {
    pub fn new() -> Self {
        Self { active_count: Arc::new(AtomicUsize::new(0)) }
    }

    pub fn register(&self) {
        self.active_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn unregister(&self) {
        self.active_count.fetch_sub(1, Ordering::Relaxed);
    }
}