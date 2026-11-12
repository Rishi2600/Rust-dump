pub unsafe fn kernel_memcpy(dest: *mut u8, src: *const u8, count: usize) {
    let mut i = 0;
    while i < count {
        *dest.add(i) = *src.add(i);
        i += 1;
    }
}