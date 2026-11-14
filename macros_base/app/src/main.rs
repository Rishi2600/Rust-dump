use core::sync::atomic::{AtomicUsize, Ordering};

pub struct SeqLock {
    seq: AtomicUsize,
}

impl SeqLock {
    pub fn read<T>(&self, f: impl Fn() -> T) -> T {
        loop {
            let start = self.seq.load(Ordering::Acquire);
            if start & 1 != 0 {
                core::hint::spin_loop();
                continue;
            }
            let value = f();
            if self.seq.load(Ordering::Acquire) == start {
                return value;
            }
        }
    }
}