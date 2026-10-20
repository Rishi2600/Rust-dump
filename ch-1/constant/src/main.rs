use std::cell::UnsafeCell;
use std::mem;

pub struct RegionArena {
    buffer: UnsafeCell<Vec<u8>>,
    offset: UnsafeCell<usize>,
}

impl RegionArena {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buffer: UnsafeCell::new(Vec::with_capacity(cap)),
            offset: UnsafeCell::new(0),
        }
    }

    pub fn alloc<T>(&self, val: T) -> &mut T {
        unsafe {
            let buffer = &mut *self.buffer.get();
            let offset = &mut *self.offset.get();
            
            let align = mem::align_of::<T>();
            let padding = (align - (*offset % align)) % align;
            let size = mem::size_of::<T>();

            if *offset + padding + size > buffer.capacity() {
                buffer.reserve(size + padding);
            }

            let ptr = buffer.as_mut_ptr().add(*offset + padding) as *mut T;
            ptr.write(val);
            *offset += padding + size;
            &mut *ptr
        }
    }
}