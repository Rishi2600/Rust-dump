use core::arch::asm;

pub unsafe fn load_page_table(pml4_physical_addr: u64) {
    asm!(
        "mov cr3, {},",
        in(reg) pml4_physical_addr,
        options(nostack, preserves_flags)
    );
}

pub unsafe fn read_cr3() -> u64 {
    let cr3_val: u64;
    asm!("mov {}, cr3", out(reg) cr3_val, options(nostack, preserves_flags));
    cr3_val
}