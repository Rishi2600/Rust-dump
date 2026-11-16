use std::fmt;
use std::str::FromStr;

use crate::error::{DnsError, DnsResult};
use crate::wire::Writer;

/// Longest single label, in bytes.
pub const MAX_LABEL_LEN: usize = 63;
/// Longest whole name in its wire form, in bytes.
pub const MAX_NAME_LEN: usize = 255;

/// A domain name: a list of labels read from the most specific to the root.
///
/// `www.example.com.` is the labels `www`, `example`, `com`. The root itself
/// is the empty list. Labels are stored lowercased because DNS compares names
/// without regard to ASCII case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name {
    labels: Vec<Vec<u8>>,
}

impl Name {
    pub fn root() -> Self {
        Self::default()
    }

    /// Build a name from raw labels, enforcing the protocol's length limits.
    pub fn from_labels(labels: Vec<Vec<u8>>) -> DnsResult<Self> {
        let mut wire_len = 1; // the terminating zero byte
        let mut lowered = Vec::with_capacity(labels.len());
        for mut label in labels {
            if label.is_empty() {
                return Err(DnsError::InvalidName("empty label".into()));
            }
            if label.len() > MAX_LABEL_LEN {
                return Err(DnsError::LabelTooLong(label.len()));
            }
            wire_len += label.len() + 1;
            label.make_ascii_lowercase();
            lowered.push(label);
        }
        if wire_len > MAX_NAME_LEN {
            return Err(DnsError::NameTooLong);
        }
        Ok(Self { labels: lowered })
    }

    /// Parse dotted text such as `example.com` or `example.com.`.
    pub fn parse(text: &str) -> DnsResult<Self> {
        let trimmed = text.strip_suffix('.').unwrap_or(text);
        if trimmed.is_empty() {
            return Ok(Self::root());
        }
        let mut labels = Vec::new();
        for label in trimmed.split('.') {
            if label.is_empty() {
                return Err(DnsError::InvalidName(format!(
                    "`{text}` has an empty label"
                )));
            }
            if !label.bytes().all(|byte| byte.is_ascii_graphic()) {
                return Err(DnsError::InvalidName(format!(
                    "`{text}` has characters other than printable ASCII"
                )));
            }
            labels.push(label.as_bytes().to_vec());
        }
        Self::from_labels(labels)
    }

    pub fn labels(&self) -> &[Vec<u8>] {
        &self.labels
    }

    pub fn is_root(&self) -> bool {
        self.labels.is_empty()
    }

    /// Size of the uncompressed wire form: a length byte per label, the
    /// labels themselves, and the final zero.
    pub fn wire_len(&self) -> usize {
        self.labels
            .iter()
            .map(|label| label.len() + 1)
            .sum::<usize>()
            + 1
    }
}

impl Name {
    /// Write the wire form: each label prefixed by its length, then a zero
    /// byte standing for the root.
    pub fn encode(&self, writer: &mut Writer) {
        for label in &self.labels {
            writer.write_u8(label.len() as u8);
            writer.write_bytes(label);
        }
        writer.write_u8(0);
    }
}

impl fmt::Display for Name {
    /// Prints the fully qualified form, with the trailing dot.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_root() {
            return write!(f, ".");
        }
        for label in &self.labels {
            for &byte in label {
                if byte.is_ascii_graphic() && byte != b'.' && byte != b'\\' {
                    write!(f, "{}", byte as char)?;
                } else {
                    // Zone-file escape for bytes that would be ambiguous.
                    write!(f, "\\{byte:03}")?;
                }
            }
            write!(f, ".")?;
        }
        Ok(())
    }
}

impl FromStr for Name {
    type Err = DnsError;

    fn from_str(text: &str) -> DnsResult<Self> {
        Self::parse(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(name: &Name) -> Vec<u8> {
        let mut writer = Writer::new();
        name.encode(&mut writer);
        writer.finish()
    }

    fn name(text: &str) -> Name {
        Name::parse(text).expect("valid name")
    }

    #[test]
    fn parses_and_prints_names_ignoring_case() {
        let www = name("WWW.Example.com");
        assert_eq!(www.to_string(), "www.example.com.");
        assert_eq!(www, name("www.example.com."));
        assert_eq!(www.labels().len(), 3);
        assert_eq!(www.wire_len(), 17);
        assert_eq!("example.com".parse::<Name>(), Ok(name("example.com.")));
    }

    #[test]
    fn the_root_is_the_empty_name() {
        assert_eq!(name("."), Name::root());
        assert_eq!(name(""), Name::root());
        assert!(Name::root().is_root());
        assert_eq!(Name::root().to_string(), ".");
        assert_eq!(Name::root().wire_len(), 1);
    }

    #[test]
    fn rejects_malformed_names() {
        assert!(matches!(Name::parse("a..b"), Err(DnsError::InvalidName(_))));
        assert!(matches!(
            Name::parse("has space.com"),
            Err(DnsError::InvalidName(_))
        ));
        let long_label = "a".repeat(64);
        assert_eq!(Name::parse(&long_label), Err(DnsError::LabelTooLong(64)));
        let long_name = vec!["a".repeat(63); 4].join(".");
        assert_eq!(Name::parse(&long_name), Err(DnsError::NameTooLong));
    }

    #[test]
    fn unusual_bytes_are_escaped_when_printed() {
        let odd = Name::from_labels(vec![b"a.b".to_vec(), vec![0x07]]).unwrap();
        assert_eq!(odd.to_string(), "a\\046b.\\007.");
    }

    #[test]
    fn encodes_labels_with_length_prefixes() {
        let bytes = encoded(&name("www.example.com"));
        assert_eq!(
            bytes, b"\x03www\x07example\x03com\x00",
            "three labels and the root"
        );
        assert_eq!(bytes.len(), name("www.example.com").wire_len());
        assert_eq!(encoded(&Name::root()), [0]);
    }
}
