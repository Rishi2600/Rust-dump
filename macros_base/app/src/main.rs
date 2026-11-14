use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

pub struct KernelHeap;

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        null_mut() // Delegate to the slab / buddy allocator
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static HEAP: KernelHeap = KernelHeap;