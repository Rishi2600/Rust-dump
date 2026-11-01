use std::mem;

#[repr(C, packed)]
pub struct PacketHeader {
    pub magic: u16,
    pub length: u32,
}

pub fn parse_packet<'a>(buffer: &'a [u8]) -> Result<&'a [u8], &'static str> {
    let header_size = mem::size_of::<PacketHeader>();
    if buffer.len() < header_size { return Err("Underflow"); }
    let header = unsafe { &*(buffer.as_ptr() as *const PacketHeader) };
    let len = u32::from_be(header.length) as usize;
    if buffer.len() < header_size + len { return Err("Truncated payload"); }
    Ok(&buffer[header_size..header_size + len])
}