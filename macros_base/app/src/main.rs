use std::mem;

#[repr(C, packed)]
pub struct RawHeader {
    pub magic: u16,
    pub payload_size: u32,
}

pub fn parse_header(buffer: &[u8]) -> Option<&RawHeader> {
    if buffer.len() < mem::size_of::<RawHeader>() {
        return None;
    }
    unsafe {
        let ptr = buffer.as_ptr() as *const RawHeader;
        Some(&*ptr)
    }
}