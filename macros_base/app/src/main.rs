use core::arch::asm;

pub fn rdtsc() -> u64 {
    let (low, high): (u32, u32);
    unsafe {
        asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}