pub unsafe fn read_cmos(register: u8) -> u8 {
    outb(0x70, register);
    inb(0x71)
}