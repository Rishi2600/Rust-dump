#[repr(C, packed)]
pub struct IdtEntry {
    pub offset_low: u16,
    pub selector: u16,
    pub ist: u8,
    pub type_attr: u8,
    pub offset_middle: u16,
    pub offset_high: u32,
    pub reserved: u32,
}