#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct PageTableEntry(pub u64);

impl PageTableEntry {
    pub fn set_addr(&mut self, addr: u64, flags: u64) {
        self.0 = (addr & 0x000fffff_fffff000) | (flags & 0xfff);
    }

    pub fn is_present(&self) -> bool { (self.0 & 1) != 0 }
    pub fn is_writable(&self) -> bool { ((self.0 >> 1) & 1) != 0 }
}