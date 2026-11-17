//! Reading and writing integers inside a block.
//!
//! Every multi-byte number on disk is little-endian: the least significant
//! byte comes first. It is this file system's own format, so the choice is
//! free, and little-endian is what the machines it runs on use natively.

pub fn get_u16(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

pub fn get_u32(buf: &[u8], offset: usize) -> u32 {
    let bytes = [
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ];
    u32::from_le_bytes(bytes)
}

pub fn put_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_stored_least_significant_byte_first() {
        let mut buf = [0; 8];
        put_u32(&mut buf, 0, 0x1234_5678);
        put_u16(&mut buf, 6, 0xBEEF);
        assert_eq!(buf, [0x78, 0x56, 0x34, 0x12, 0, 0, 0xEF, 0xBE]);
        assert_eq!(get_u32(&buf, 0), 0x1234_5678);
        assert_eq!(get_u16(&buf, 6), 0xBEEF);
    }
}
