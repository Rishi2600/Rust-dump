const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

pub unsafe fn pic_remap(offset1: u8, offset2: u8) {
    outb(PIC1_CMD, 0x11); // ICW1: begin initialization
    outb(PIC2_CMD, 0x11);
    outb(PIC1_DATA, offset1); // ICW2: vector offsets
    outb(PIC2_DATA, offset2);
    outb(PIC1_DATA, 0x04); // ICW3: slave on IRQ2
    outb(PIC2_DATA, 0x02);
    outb(PIC1_DATA, 0x01); // ICW4: 8086 mode
    outb(PIC2_DATA, 0x01);
}