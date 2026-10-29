#[repr(transparent)]
pub struct PeripheralRegister {
    bits: u32,
}

impl PeripheralRegister {
    pub unsafe fn modify_bit(&mut self, bit: u32, set: bool) {
        let mut val = std::ptr::read_volatile(&self.bits);
        if set { val |= 1 << bit; } else { val &= !(1 << bit); }
        std::ptr::write_volatile(&mut self.bits, val);
    }
}