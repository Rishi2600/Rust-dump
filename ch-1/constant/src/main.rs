use std::marker::PhantomData;
use std::mem;

#[repr(C, packed)]
pub struct WireHeader {
    pub magic: u16,
    pub payload_len: u32,
}

pub struct Packet<'a, T> {
    header: &'a WireHeader,
    payload: &'a [u8],
    _marker: PhantomData<T>,
}

impl<'a, T> Packet<'a, T> {
    pub fn parse(buffer: &'a [u8]) -> Result<Self, &'static str> {
        let header_size = mem::size_of::<WireHeader>();
        if buffer.len() < header_size {
            return Err("Buffer underflow");
        }

        let header = unsafe { &*(buffer.as_ptr() as *const WireHeader) };
        let payload_len = u32::from_be(header.payload_len) as usize;

        if buffer.len() < header_size + payload_len {
            return Err("Incomplete payload");
        }

        Ok(Self {
            header,
            payload: &buffer[header_size..header_size + payload_len],
            _marker: PhantomData,
        })
    }

    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }
}