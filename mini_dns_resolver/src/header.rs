use crate::error::DnsResult;
use crate::wire::{Reader, Writer};

use std::fmt;

/// Response code: the server's verdict on a query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Rcode {
    /// The query was handled. This does not promise any answer records.
    #[default]
    NoError,
    /// The server could not parse the query.
    FormErr,
    /// The server hit an internal problem.
    ServFail,
    /// The name does not exist at all.
    NxDomain,
    /// The server does not support this kind of query.
    NotImp,
    /// The server will not answer this client or this question.
    Refused,
    Other(u8),
}

impl Rcode {
    pub fn code(self) -> u8 {
        match self {
            Rcode::NoError => 0,
            Rcode::FormErr => 1,
            Rcode::ServFail => 2,
            Rcode::NxDomain => 3,
            Rcode::NotImp => 4,
            Rcode::Refused => 5,
            Rcode::Other(code) => code,
        }
    }

    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Rcode::NoError,
            1 => Rcode::FormErr,
            2 => Rcode::ServFail,
            3 => Rcode::NxDomain,
            4 => Rcode::NotImp,
            5 => Rcode::Refused,
            other => Rcode::Other(other),
        }
    }
}

impl fmt::Display for Rcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rcode::NoError => write!(f, "NOERROR"),
            Rcode::FormErr => write!(f, "FORMERR"),
            Rcode::ServFail => write!(f, "SERVFAIL"),
            Rcode::NxDomain => write!(f, "NXDOMAIN"),
            Rcode::NotImp => write!(f, "NOTIMP"),
            Rcode::Refused => write!(f, "REFUSED"),
            Rcode::Other(code) => write!(f, "RCODE{code}"),
        }
    }
}

/// The fixed 12-byte start of every DNS message, minus the section counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Header {
    /// Chosen by the client and echoed by the server, to pair replies with queries.
    pub id: u16,
    /// QR: false for a query, true for a response.
    pub response: bool,
    pub opcode: u8,
    /// AA: the server owns this data rather than repeating what it heard.
    pub authoritative: bool,
    /// TC: the reply did not fit and was cut short.
    pub truncated: bool,
    /// RD: the client asks the server to chase the answer on its behalf.
    pub recursion_desired: bool,
    /// RA: the server is willing to do that.
    pub recursion_available: bool,
    pub rcode: Rcode,
}

/// How many entries each of the four sections holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub questions: u16,
    pub answers: u16,
    pub authorities: u16,
    pub additionals: u16,
}

impl Header {
    /// Size of the encoded header, counts included.
    pub const LEN: usize = 12;

    pub fn encode(&self, counts: Counts, writer: &mut Writer) {
        let flags = u16::from(self.response) << 15
            | u16::from(self.opcode & 0x0F) << 11
            | u16::from(self.authoritative) << 10
            | u16::from(self.truncated) << 9
            | u16::from(self.recursion_desired) << 8
            | u16::from(self.recursion_available) << 7
            | u16::from(self.rcode.code() & 0x0F);
        writer.write_u16(self.id);
        writer.write_u16(flags);
        writer.write_u16(counts.questions);
        writer.write_u16(counts.answers);
        writer.write_u16(counts.authorities);
        writer.write_u16(counts.additionals);
    }

    pub fn decode(reader: &mut Reader<'_>) -> DnsResult<(Self, Counts)> {
        let id = reader.read_u16()?;
        let flags = reader.read_u16()?;
        let bit = |n: u16| flags & (1 << n) != 0;
        let header = Self {
            id,
            response: bit(15),
            opcode: ((flags >> 11) & 0x0F) as u8,
            authoritative: bit(10),
            truncated: bit(9),
            recursion_desired: bit(8),
            recursion_available: bit(7),
            rcode: Rcode::from_code((flags & 0x0F) as u8),
        };
        let counts = Counts {
            questions: reader.read_u16()?,
            answers: reader.read_u16()?,
            authorities: reader.read_u16()?,
            additionals: reader.read_u16()?,
        };
        Ok((header, counts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_codes_round_trip() {
        for code in 0..16 {
            assert_eq!(Rcode::from_code(code).code(), code);
        }
        assert_eq!(Rcode::from_code(3), Rcode::NxDomain);
        assert_eq!(Rcode::NxDomain.to_string(), "NXDOMAIN");
        assert_eq!(Rcode::from_code(9).to_string(), "RCODE9");
    }

    #[test]
    fn flags_pack_into_the_second_word() {
        let header = Header {
            id: 0xBEEF,
            response: true,
            authoritative: true,
            rcode: Rcode::NxDomain,
            ..Header::default()
        };
        let counts = Counts {
            questions: 1,
            authorities: 2,
            ..Counts::default()
        };
        let mut writer = Writer::new();
        header.encode(counts, &mut writer);
        let bytes = writer.finish();
        assert_eq!(bytes.len(), Header::LEN);
        assert_eq!(bytes, [0xBE, 0xEF, 0x84, 0x03, 0, 1, 0, 0, 0, 2, 0, 0]);
        assert_eq!(
            Header::decode(&mut Reader::new(&bytes)),
            Ok((header, counts))
        );
    }

    #[test]
    fn decodes_the_flags_of_a_typical_recursive_answer() {
        // 0x8180: response, recursion desired, recursion available.
        let bytes = [0x12, 0x34, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        let (header, counts) = Header::decode(&mut Reader::new(&bytes)).unwrap();
        assert!(header.response && header.recursion_desired && header.recursion_available);
        assert!(!header.authoritative && !header.truncated);
        assert_eq!((header.id, header.rcode), (0x1234, Rcode::NoError));
        assert_eq!((counts.questions, counts.answers), (1, 1));
    }
}
