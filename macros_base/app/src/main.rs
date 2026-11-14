const IOAPIC_BASE: usize = 0xFEC0_0000;
const IOREGSEL: usize = 0x00;
const IOWIN: usize = 0x10;

pub unsafe fn ioapic_set_irq(irq: u8, vector: u8, apic_id: u8) {
    let reg = 0x10 + (irq as u32) * 2;
    let sel = (IOAPIC_BASE + IOREGSEL) as *mut u32;
    let win = (IOAPIC_BASE + IOWIN) as *mut u32;

    sel.write_volatile(reg);
    win.write_volatile(vector as u32);
    sel.write_volatile(reg + 1);
    win.write_volatile((apic_id as u32) << 24);
}