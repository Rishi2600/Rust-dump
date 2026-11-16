use std::fmt;
use std::str::FromStr;

use crate::error::{DnsError, DnsResult};

/// The Internet class. Other classes exist on paper but are not used.
pub const CLASS_IN: u16 = 1;

/// What kind of data a record holds, or a question asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RecordType {
    /// IPv4 address.
    A,
    /// Nameserver that is authoritative for a zone.
    Ns,
    /// Alias: this name is really another name.
    Cname,
    /// Start of authority: facts about the zone itself.
    Soa,
    /// Reverse mapping, from an address back to a name.
    Ptr,
    /// Mail exchanger.
    Mx,
    /// Free-form text.
    Txt,
    /// IPv6 address.
    Aaaa,
    /// EDNS pseudo-record: carries protocol options, not data.
    Opt,
    /// Any type this resolver has no name for.
    Unknown(u16),
}

impl RecordType {
    const KNOWN: [(RecordType, u16, &'static str); 9] = [
        (RecordType::A, 1, "A"),
        (RecordType::Ns, 2, "NS"),
        (RecordType::Cname, 5, "CNAME"),
        (RecordType::Soa, 6, "SOA"),
        (RecordType::Ptr, 12, "PTR"),
        (RecordType::Mx, 15, "MX"),
        (RecordType::Txt, 16, "TXT"),
        (RecordType::Aaaa, 28, "AAAA"),
        (RecordType::Opt, 41, "OPT"),
    ];

    /// The number used for this type on the wire.
    pub fn code(self) -> u16 {
        match self {
            RecordType::Unknown(code) => code,
            known => Self::KNOWN
                .iter()
                .find(|(rtype, _, _)| *rtype == known)
                .map_or(0, |(_, code, _)| *code),
        }
    }

    pub fn from_code(code: u16) -> Self {
        Self::KNOWN
            .iter()
            .find(|(_, known, _)| *known == code)
            .map_or(RecordType::Unknown(code), |(rtype, _, _)| *rtype)
    }
}

impl fmt::Display for RecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match Self::KNOWN.iter().find(|(rtype, _, _)| rtype == self) {
            Some((_, _, name)) => write!(f, "{name}"),
            // The standard spelling for a type without a mnemonic.
            None => write!(f, "TYPE{}", self.code()),
        }
    }
}

impl FromStr for RecordType {
    type Err = DnsError;

    fn from_str(text: &str) -> DnsResult<Self> {
        let upper = text.to_ascii_uppercase();
        if let Some((rtype, _, _)) = Self::KNOWN.iter().find(|(_, _, name)| *name == upper) {
            return Ok(*rtype);
        }
        upper
            .strip_prefix("TYPE")
            .and_then(|code| code.parse().ok())
            .map(Self::from_code)
            .ok_or_else(|| DnsError::BadRecord(format!("unknown record type `{text}`")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_names_round_trip() {
        for (rtype, code, name) in RecordType::KNOWN {
            assert_eq!(rtype.code(), code);
            assert_eq!(RecordType::from_code(code), rtype);
            assert_eq!(rtype.to_string(), name);
            assert_eq!(name.parse(), Ok(rtype));
        }
    }

    #[test]
    fn unknown_types_keep_their_number() {
        let https = RecordType::from_code(65);
        assert_eq!(https, RecordType::Unknown(65));
        assert_eq!(https.code(), 65);
        assert_eq!(https.to_string(), "TYPE65");
        assert_eq!("type65".parse(), Ok(https));
    }

    #[test]
    fn parsing_ignores_case_and_rejects_nonsense() {
        assert_eq!("aaaa".parse(), Ok(RecordType::Aaaa));
        assert_eq!("TYPE28".parse(), Ok(RecordType::Aaaa));
        assert!("bogus".parse::<RecordType>().is_err());
    }
}
