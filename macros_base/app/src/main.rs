use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct TokenBucket {
    capacity: usize,
    tokens: Mutex<(usize, Instant)>,
    refill_rate: Duration,
}

impl TokenBucket {
    pub fn new(capacity: usize, refill_rate: Duration) -> Self {
        Self {
            capacity,
            tokens: Mutex::new((capacity, Instant::now())),
            refill_rate,
        }
    }
}