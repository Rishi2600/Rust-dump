use std::collections::HashMap;

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

/// Builds the bytes of an outgoing message.
#[derive(Debug, Default)]
pub struct Writer {
    buf: Vec<u8>,
    /// Where each name suffix was first written, for compression.
    names: HashMap<Vec<Vec<u8>>, u16>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Offset at which the next byte will be written.
    pub fn position(&self) -> usize {
        self.buf.len()
    }

    pub fn write_u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    pub fn write_u16(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Overwrite a `u16` written earlier, to fill in a length once it is known.
    pub fn set_u16(&mut self, pos: usize, value: u16) {
        self.buf[pos..pos + 2].copy_from_slice(&value.to_be_bytes());
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

impl Writer {
    /// Offset of an earlier copy of this run of labels, if there is one.
    pub fn name_offset(&self, suffix: &[Vec<u8>]) -> Option<u16> {
        self.names.get(suffix).copied()
    }

    /// Record that this run of labels is about to be written here.
    pub fn remember_name(&mut self, suffix: &[Vec<u8>]) {
        // A pointer has only 14 bits, so later offsets cannot be referenced.
        if let Ok(offset) = u16::try_from(self.buf.len()) {
            if offset < 0x4000 {
                self.names.entry(suffix.to_vec()).or_insert(offset);
            }
        }
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

    #[test]
    fn writes_big_endian_integers() {
        let mut writer = Writer::new();
        writer.write_u16(0x1234);
        writer.write_u32(3600);
        writer.write_u8(0xFF);
        writer.write_bytes(b"ok");
        assert_eq!(writer.position(), 9);
        assert_eq!(
            writer.finish(),
            [0x12, 0x34, 0, 0, 0x0E, 0x10, 0xFF, b'o', b'k']
        );
    }

    #[test]
    fn a_length_can_be_filled_in_after_the_data() {
        let mut writer = Writer::new();
        let length_at = writer.position();
        writer.write_u16(0);
        writer.write_bytes(b"abc");
        writer.set_u16(length_at, 3);
        assert_eq!(writer.finish(), [0, 3, b'a', b'b', b'c']);
    }
}
