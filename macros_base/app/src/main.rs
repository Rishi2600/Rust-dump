use core::arch::asm;

pub unsafe fn write_gs_base(addr: u64) {
    const IA32_GS_BASE: u32 = 0xC0000101;
    let low = addr as u32;
    let high = (addr >> 32) as u32;
    asm!(
        "wrmsr",
        in("ecx") IA32_GS_BASE,
        in("eax") low,
        in("edx") high,
        options(nostack, preserves_flags)
    );
}