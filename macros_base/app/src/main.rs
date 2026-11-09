use std::cell::UnsafeCell;

pub struct StringPool {
    buffer: UnsafeCell<Vec<u8>>,
}

impl StringPool {
    pub fn new(capacity: usize) -> Self {
        Self { buffer: UnsafeCell::new(Vec::with_capacity(capacity)) }
    }

    pub fn push(&self, s: &str) -> &[u8] {
        unsafe {
            let buf = &mut *self.buffer.get();
            let start = buf.len();
            buf.extend_from_slice(s.as_bytes());
            &buf[start..]
        }
    }
}