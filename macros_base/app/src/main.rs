use core::arch::asm;

pub unsafe fn flush_tlb(virtual_address: usize) {
    asm!(
        "invlpg [{0}]",
        in(reg) virtual_address,
        options(nostack, preserves_flags)
    );
}