use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct SpmcBroadcast<T, const CAP: usize> {
    buffer: [UnsafeCell<MaybeUninit<T>>; CAP],
    write_seq: AtomicUsize,
}

use std::mem::MaybeUninit;
unsafe impl<T: Send, const CAP: usize> Sync for SpmcBroadcast<T, CAP> {}