extern "C" {
    static _kernel_start: u8;
    static _kernel_end: u8;
}

pub fn get_kernel_size() -> usize {
    unsafe {
        &_kernel_end as *const u8 as usize - &_kernel_start as *const u8 as usize
    }
}