use std::cell::UnsafeCell;

pub struct StringInterner {
    buffer: UnsafeCell<Vec<u8>>,
}