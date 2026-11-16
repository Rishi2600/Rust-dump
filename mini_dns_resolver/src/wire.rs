use crate::error::{DnsError, DnsResult};

/// Cursor over the bytes of a message.
///
/// DNS sends every multi-byte integer most significant byte first
/// ("network byte order"), so the readers here are all big-endian.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// Jump to an absolute offset in the message.
    pub fn seek(&mut self, pos: usize) -> DnsResult<()> {
        if pos > self.data.len() {
            return Err(DnsError::UnexpectedEof);
        }
        self.pos = pos;
        Ok(())
    }

    /// Take the next `len` bytes. A failed read consumes nothing.
    pub fn read_bytes(&mut self, len: usize) -> DnsResult<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|&end| end <= self.data.len())
            .ok_or(DnsError::UnexpectedEof)?;
        let bytes = &self.data[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    pub fn read_u8(&mut self) -> DnsResult<u8> {
        Ok(self.read_bytes(1)?[0])
    }

    pub fn read_u16(&mut self) -> DnsResult<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_u32(&mut self) -> DnsResult<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_big_endian_integers_in_order() {
        let mut reader = Reader::new(&[0x12, 0x34, 0x00, 0x00, 0x0E, 0x10, 0xFF]);
        assert_eq!(reader.read_u16(), Ok(0x1234));
        assert_eq!(reader.read_u32(), Ok(3600));
        assert_eq!(reader.read_u8(), Ok(0xFF));
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn running_off_the_end_is_an_error_not_a_panic() {
        let mut reader = Reader::new(&[1, 2, 3]);
        assert_eq!(reader.read_u32(), Err(DnsError::UnexpectedEof));
        assert_eq!(reader.position(), 0);
        assert_eq!(reader.seek(4), Err(DnsError::UnexpectedEof));
        assert_eq!(reader.seek(1), Ok(()));
        assert_eq!(reader.read_u8(), Ok(2));
        assert_eq!(reader.read_bytes(1), Ok(&[3][..]));
    }
}
