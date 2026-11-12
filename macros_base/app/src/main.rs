pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: usize,
}

impl BumpAllocator {
    pub const fn new(heap_start: usize, heap_end: usize) -> Self {
        Self { heap_start, heap_end, next: heap_start }
    }

    pub unsafe fn allocate(&mut self, size: usize, align: usize) -> Result<*mut u8, &'static str> {
        let alloc_addr = (self.next + (align - 1)) & !(align - 1);
        if alloc_addr + size > self.heap_end {
            return Err("Out of memory");
        }
        self.next = alloc_addr + size;
        Ok(alloc_addr as *mut u8)
    }
}