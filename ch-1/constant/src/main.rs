use std::cell::UnsafeCell;
use std::mem;

pub struct Arena {
    buffer: UnsafeCell<Vec<u8>>,
    offset: UnsafeCell<usize>,
}

impl Arena {
    pub fn new(capacity: usize) -> Self {
        Self { buffer: UnsafeCell::new(Vec::with_capacity(capacity)), offset: UnsafeCell::new(0) }
    }

    pub fn alloc<T>(&self, val: T) -> &mut T {
        unsafe {
            let buf = &mut *self.buffer.get();
            let off = &mut *self.offset.get();
            let align = mem::align_of::<T>();
            let padding = (align - (*off % align)) % align;
            let size = mem::size_of::<T>();
            let ptr = buf.as_mut_ptr().add(*off + padding) as *mut T;
            ptr.write(val);
            *off += padding + size;
            &mut *ptr
        }
    }
}