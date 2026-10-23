pub struct PackedHeader(u16);

impl PackedHeader {
    pub fn get_flags(&self) -> u8 {
        (self.0 & 0x00FF) as u8
    }
    pub fn set_flags(&mut self, val: u8) {
        self.0 = (self.0 & 0xFF00) | (val as u16);
    }
}