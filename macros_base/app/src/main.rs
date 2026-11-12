use core::arch::asm;

pub fn halt_cpu() -> ! {
    loop {
        unsafe {
            asm!("cli", "hlt", options(nostack, preserves_flags));
        }
    }
}