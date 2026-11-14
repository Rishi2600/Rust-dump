const KERNEL_OFFSET: u64 = 0xFFFF_FFFF_8000_0000;

pub const fn virt_to_phys(virt: u64) -> u64 {
    virt - KERNEL_OFFSET
}

pub const fn phys_to_virt(phys: u64) -> u64 {
    phys + KERNEL_OFFSET
}