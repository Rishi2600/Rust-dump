const LAPIC_ICR_LOW: usize = 0x300;
const LAPIC_ICR_HIGH: usize = 0x310;

pub unsafe fn start_ap(lapic_base: usize, apic_id: u8, trampoline_page: u8) {
    let icr_low = (lapic_base + LAPIC_ICR_LOW) as *mut u32;
    let icr_high = (lapic_base + LAPIC_ICR_HIGH) as *mut u32;

    icr_high.write_volatile((apic_id as u32) << 24);
    icr_low.write_volatile(0x0000_4500); // INIT IPI
    // Wait 10ms, then send the Startup IPI
    icr_high.write_volatile((apic_id as u32) << 24);
    icr_low.write_volatile(0x0000_4600 | trampoline_page as u32); // SIPI
}