use std::net::{Ipv4Addr, Ipv6Addr};

use crate::error::{DnsError, DnsResult};
use crate::name::Name;
use crate::rtype::{CLASS_IN, RecordType};
use crate::wire::Reader;

/// The payload of a record, interpreted according to its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RData {
    A(Ipv4Addr),
    Aaaa(Ipv6Addr),
    /// Data of a type this resolver does not interpret, kept as raw bytes.
    Other(Vec<u8>),
}

/// A resource record: one fact about one name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub name: Name,
    pub rtype: RecordType,
    pub class: u16,
    /// How many seconds the record may be cached.
    pub ttl: u32,
    pub data: RData,
}

impl Record {
    pub fn new(name: Name, rtype: RecordType, ttl: u32, data: RData) -> Self {
        Self {
            name,
            rtype,
            class: CLASS_IN,
            ttl,
            data,
        }
    }

    /// Layout: name, type, class, TTL, data length, then that many bytes of data.
    pub fn decode(reader: &mut Reader<'_>) -> DnsResult<Self> {
        let name = Name::decode(reader)?;
        let rtype = RecordType::from_code(reader.read_u16()?);
        let class = reader.read_u16()?;
        let ttl = reader.read_u32()?;
        let length = usize::from(reader.read_u16()?);
        if length > reader.remaining() {
            return Err(DnsError::UnexpectedEof);
        }
        let end = reader.position() + length;
        let data = RData::decode(rtype, length, reader)?;
        if reader.position() != end {
            return Err(DnsError::BadRecord(format!(
                "{rtype} data does not match its declared length of {length}"
            )));
        }
        Ok(Self {
            name,
            rtype,
            class,
            ttl,
            data,
        })
    }
}

impl RData {
    fn decode(rtype: RecordType, length: usize, reader: &mut Reader<'_>) -> DnsResult<Self> {
        Ok(match rtype {
            RecordType::A => RData::A(Ipv4Addr::from(fixed::<4>(rtype, length, reader)?)),
            RecordType::Aaaa => RData::Aaaa(Ipv6Addr::from(fixed::<16>(rtype, length, reader)?)),
            _ => RData::Other(reader.read_bytes(length)?.to_vec()),
        })
    }
}

/// Read data that must be exactly `N` bytes long.
fn fixed<const N: usize>(
    rtype: RecordType,
    length: usize,
    reader: &mut Reader<'_>,
) -> DnsResult<[u8; N]> {
    if length != N {
        return Err(DnsError::BadRecord(format!(
            "{rtype} data must be {N} bytes, found {length}"
        )));
    }
    let mut bytes = [0; N];
    bytes.copy_from_slice(reader.read_bytes(N)?);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> Name {
        Name::parse(text).expect("valid name")
    }

    /// The bytes of a record owned by `example.com.` with a TTL of 300.
    fn raw(type_code: u16, rdata: &[u8]) -> Vec<u8> {
        let mut bytes = b"\x07example\x03com\x00".to_vec();
        bytes.extend_from_slice(&type_code.to_be_bytes());
        bytes.extend_from_slice(&[0, 1, 0, 0, 1, 0x2C]);
        bytes.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        bytes.extend_from_slice(rdata);
        bytes
    }

    fn decode(type_code: u16, rdata: &[u8]) -> DnsResult<Record> {
        Record::decode(&mut Reader::new(&raw(type_code, rdata)))
    }

    #[test]
    fn decodes_address_records() {
        let a = decode(1, &[203, 0, 113, 10]).unwrap();
        assert_eq!(
            (a.name.clone(), a.rtype, a.class, a.ttl),
            (name("example.com"), RecordType::A, 1, 300)
        );
        assert_eq!(a.data, RData::A(Ipv4Addr::new(203, 0, 113, 10)));

        let mut v6 = [0; 16];
        v6[0] = 0x20;
        v6[1] = 0x01;
        v6[15] = 1;
        let aaaa = decode(28, &v6).unwrap();
        assert_eq!(aaaa.data, RData::Aaaa("2001::1".parse().unwrap()));
    }

    #[test]
    fn keeps_unknown_types_as_raw_bytes() {
        let record = decode(99, b"\x01\x02\x03").unwrap();
        assert_eq!(record.rtype, RecordType::Unknown(99));
        assert_eq!(record.data, RData::Other(vec![1, 2, 3]));
    }

    #[test]
    fn rejects_data_of_the_wrong_size() {
        assert!(matches!(decode(1, &[1, 2, 3]), Err(DnsError::BadRecord(_))));
        // A length that runs past the end of the message.
        let mut bytes = raw(1, &[1, 2, 3, 4]);
        bytes.truncate(bytes.len() - 1);
        let truncated = Record::decode(&mut Reader::new(&bytes));
        assert_eq!(truncated, Err(DnsError::UnexpectedEof));
    }
}
