use std::fmt;
use std::str::FromStr;

use crate::error::{DnsError, DnsResult};
use crate::wire::{Reader, Writer};

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
    ///
    /// If the rest of the name already appears earlier in the message, a
    /// two-byte pointer to it is written instead of repeating the labels.
    pub fn encode(&self, writer: &mut Writer) {
        for (index, label) in self.labels.iter().enumerate() {
            let suffix = &self.labels[index..];
            if let Some(offset) = writer.name_offset(suffix) {
                writer.write_u16(0xC000 | offset);
                return;
            }
            writer.remember_name(suffix);
            writer.write_u8(label.len() as u8);
            writer.write_bytes(label);
        }
        writer.write_u8(0);
    }
}

/// More pointer hops than any honest message needs.
const MAX_POINTER_JUMPS: usize = 32;

impl Name {
    /// Read a name, following compression pointers.
    ///
    /// A length byte whose top two bits are set is not a length at all: with
    /// the next byte it forms a 14-bit offset to where the rest of the name
    /// was already written earlier in the message. The reader is left just
    /// after the name as it appears at the starting position.
    pub fn decode(reader: &mut Reader<'_>) -> DnsResult<Self> {
        let mut labels = Vec::new();
        let mut resume_at = None;
        let mut jumps = 0;
        loop {
            let at = reader.position();
            let len = reader.read_u8()?;
            match len & 0xC0 {
                0x00 if len == 0 => break,
                0x00 => labels.push(reader.read_bytes(usize::from(len))?.to_vec()),
                0xC0 => {
                    let low = reader.read_u8()?;
                    let target = usize::from(len & 0x3F) << 8 | usize::from(low);
                    // Pointers may only refer to earlier data. Together with the
                    // hop limit this makes a malicious loop impossible.
                    if target >= at {
                        return Err(DnsError::BadPointer(target));
                    }
                    jumps += 1;
                    if jumps > MAX_POINTER_JUMPS {
                        return Err(DnsError::CompressionLoop);
                    }
                    resume_at.get_or_insert(reader.position());
                    reader.seek(target)?;
                }
                _ => return Err(DnsError::BadLabel(len)),
            }
        }
        if let Some(pos) = resume_at {
            reader.seek(pos)?;
        }
        Self::from_labels(labels)
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

    #[test]
    fn decodes_what_it_encodes() {
        let bytes = encoded(&name("Mail.Example.COM"));
        let mut reader = Reader::new(&bytes);
        assert_eq!(Name::decode(&mut reader), Ok(name("mail.example.com")));
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn follows_a_compression_pointer_and_resumes_after_it() {
        // `example.com.` at offset 0, then `www` + a pointer back to offset 0.
        let mut bytes = b"\x07example\x03com\x00".to_vec();
        bytes.extend_from_slice(b"\x03www\xC0\x00\xAB");
        let mut reader = Reader::new(&bytes);
        reader.seek(13).unwrap();
        assert_eq!(Name::decode(&mut reader), Ok(name("www.example.com")));
        // The reader continues after the two pointer bytes, not after the target.
        assert_eq!(reader.read_u8(), Ok(0xAB));
    }

    #[test]
    fn rejects_hostile_or_broken_encodings() {
        let decode = |bytes: &[u8]| Name::decode(&mut Reader::new(bytes));
        // A pointer to itself or forwards.
        assert_eq!(decode(b"\xC0\x00"), Err(DnsError::BadPointer(0)));
        assert_eq!(decode(b"\xC0\x05"), Err(DnsError::BadPointer(5)));
        // A label followed by a pointer back to that label, forever.
        assert_eq!(decode(b"\x01a\xC0\x00"), Err(DnsError::CompressionLoop));
        // Reserved label types and truncated data.
        assert_eq!(decode(b"\x40"), Err(DnsError::BadLabel(0x40)));
        assert_eq!(decode(b"\x03ww"), Err(DnsError::UnexpectedEof));
    }

    #[test]
    fn repeated_suffixes_are_replaced_by_pointers() {
        let names = [
            "example.com.",
            "www.example.com.",
            "mail.example.com.",
            "com.",
        ];
        let mut writer = Writer::new();
        for text in names {
            name(text).encode(&mut writer);
        }
        let bytes = writer.finish();
        // `example.com.` takes 13 bytes; `com` starts at offset 8 inside it.
        assert_eq!(bytes[13..19], *b"\x03www\xC0\x00");
        assert_eq!(bytes[19..26], *b"\x04mail\xC0\x00");
        assert_eq!(bytes[26..], [0xC0, 8]);

        let mut reader = Reader::new(&bytes);
        for text in names {
            assert_eq!(Name::decode(&mut reader), Ok(name(text)));
        }
    }
}
