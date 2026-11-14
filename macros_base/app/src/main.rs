const VGA_BUFFER: *mut u16 = 0xb8000 as *mut u16;
const VGA_WIDTH: usize = 80;

pub unsafe fn vga_put_char(row: usize, col: usize, ch: u8, color: u8) {
    let entry = (color as u16) << 8 | ch as u16;
    VGA_BUFFER.add(row * VGA_WIDTH + col).write_volatile(entry);
}