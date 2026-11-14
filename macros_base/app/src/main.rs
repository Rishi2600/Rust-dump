const PIT_BASE_HZ: u32 = 1_193_182;

pub unsafe fn pit_set_frequency(hz: u32) {
    let divisor = (PIT_BASE_HZ / hz) as u16;
    outb(0x43, 0x36); // Channel 0, lobyte/hibyte, square wave
    outb(0x40, divisor as u8);
    outb(0x40, (divisor >> 8) as u8);
}