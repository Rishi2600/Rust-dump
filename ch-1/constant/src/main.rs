#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PacketHeader {
    pub magic: u16,
    pub length: u16,
}

pub fn parse_packet_header(bytes: &[u8]) -> Option<&PacketHeader> {
    if bytes.len() < std::mem::size_of::<PacketHeader>() {
        return None;
    }

    // Unsafe pointer casting for zero-copy read
    unsafe {
        let ptr = bytes.as_ptr() as *const PacketHeader;
        Some(&*ptr)
    }
}