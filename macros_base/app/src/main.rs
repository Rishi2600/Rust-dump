pub unsafe fn init_apic_timer(apic_base: usize, divisor: u32, initial_count: u32) {
    let timer_div_reg = (apic_base + 0x3E0) as *mut u32;
    let timer_init_reg = (apic_base + 0x380) as *mut u32;
    let timer_lvt_reg = (apic_base + 0x320) as *mut u32;

    core::ptr::write_volatile(timer_div_reg, divisor);
    core::ptr::write_volatile(timer_lvt_reg, 32); // Vector 32 for timer IRQ
    core::ptr::write_volatile(timer_init_reg, initial_count);
}