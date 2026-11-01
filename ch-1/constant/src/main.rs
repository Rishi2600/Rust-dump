use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};

pub struct RateLimiter {
    interval: Duration,
    last: Arc<Mutex<Instant>>,
}

impl RateLimiter {
    pub fn new(interval: Duration) -> Self {
        Self { interval, last: Arc::new(Mutex::new(Instant::now())) }
    }
    async fn acquire(&self) {
        let mut last = self.last.lock().await;
        let elapsed = last.elapsed();
        if elapsed < self.interval {
            sleep(self.interval - elapsed).await;
        }
        *last = Instant::now();
    }
}