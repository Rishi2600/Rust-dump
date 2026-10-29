use std::sync::atomic::{AtomicU32, Ordering};

pub struct BroadcastFlag {
    state: AtomicU32,
}

impl BroadcastFlag {
    pub fn new() -> Self {
        Self { state: AtomicU32::new(0) }
    }

    pub fn broadcast(&self, version: u32) {
        self.state.store(version, Ordering::Release);
    }

    pub fn check_update(&self, last_seen: &mut u32) -> bool {
        let current = self.state.load(Ordering::Acquire);
        if current != *last_seen {
            *last_seen = current;
            true
        } else {
            false
        }
    }
}