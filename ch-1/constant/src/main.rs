use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};

pub struct RateLimiter {
    interval: Duration,
    last_run: Arc<Mutex<Instant>>,
}

impl RateLimiter {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_run: Arc::new(Mutex::new(Instant::now() - interval)),
        }
    }

    pub async fn wait(&self) {
        let mut last = self.last_run.lock().await;
        let elapsed = last.elapsed();
        if elapsed < self.interval {
            sleep(self.interval - elapsed).await;
        }
        *last = Instant::now();
    }
}