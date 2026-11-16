use crate::error::DnsResult;
use crate::header::{Counts, Header};
use crate::name::Name;
use crate::question::Question;
use crate::record::{RData, Record};
use crate::rtype::RecordType;
use crate::wire::{Reader, Writer};

/// A complete DNS message. Queries and responses share this one layout:
/// a header, the question, and three lists of records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message {
    pub header: Header,
    pub questions: Vec<Question>,
    /// Records that answer the question.
    pub answers: Vec<Record>,
    /// Records naming the servers that are authoritative for the answer.
    pub authorities: Vec<Record>,
    /// Extra records the server thought would save a follow-up query.
    pub additionals: Vec<Record>,
}

impl Message {
    pub fn decode(bytes: &[u8]) -> DnsResult<Self> {
        let mut reader = Reader::new(bytes);
        let (header, counts) = Header::decode(&mut reader)?;
        let mut message = Message {
            header,
            ..Message::default()
        };
        for _ in 0..counts.questions {
            message.questions.push(Question::decode(&mut reader)?);
        }
        for _ in 0..counts.answers {
            message.answers.push(Record::decode(&mut reader)?);
        }
        for _ in 0..counts.authorities {
            message.authorities.push(Record::decode(&mut reader)?);
        }
        for _ in 0..counts.additionals {
            message.additionals.push(Record::decode(&mut reader)?);
        }
        Ok(message)
    }

    pub fn encode(&self) -> Vec<u8> {
        let counts = Counts {
            questions: self.questions.len() as u16,
            answers: self.answers.len() as u16,
            authorities: self.authorities.len() as u16,
            additionals: self.additionals.len() as u16,
        };
        let mut writer = Writer::new();
        self.header.encode(counts, &mut writer);
        for question in &self.questions {
            question.encode(&mut writer);
        }
        let sections = [&self.answers, &self.authorities, &self.additionals];
        for record in sections.into_iter().flatten() {
            record.encode(&mut writer);
        }
        writer.finish()
    }
}

impl Message {
    /// A query for one name and type.
    ///
    /// Recursion is not requested: the caller is expected to follow referrals
    /// itself. Set `header.recursion_desired` to hand that work to the server.
    pub fn query(id: u16, name: Name, rtype: RecordType) -> Self {
        Message {
            header: Header {
                id,
                ..Header::default()
            },
            questions: vec![Question::new(name, rtype)],
            ..Message::default()
        }
    }
}

impl Message {
    /// Advertise EDNS: this client can take UDP replies of up to `payload`
    /// bytes, instead of the original limit of 512.
    pub fn with_edns(mut self, payload: u16) -> Self {
        // OPT is a pseudo-record owned by the root. Its class field is reused
        // to carry the payload size, and its TTL field carries flags.
        self.additionals.push(Record {
            name: Name::root(),
            rtype: RecordType::Opt,
            class: payload,
            ttl: 0,
            data: RData::Other(Vec::new()),
        });
        self
    }

    /// The UDP payload size the sender advertised, if it used EDNS.
    pub fn edns_payload(&self) -> Option<u16> {
        self.additionals
            .iter()
            .find(|record| record.rtype == RecordType::Opt)
            .map(|record| record.class)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DnsError;

    /// A real-shaped reply to "example.com A": the answer's owner name is a
    /// pointer back to the name in the question at offset 12.
    const REPLY: &[u8] = b"\x12\x34\x81\x80\x00\x01\x00\x01\x00\x00\x00\x00\
        \x07example\x03com\x00\x00\x01\x00\x01\
        \xC0\x0C\x00\x01\x00\x01\x00\x00\x0E\x10\x00\x04\x5D\xB8\xD8\x22";

    #[test]
    fn decodes_a_compressed_reply() {
        let message = Message::decode(REPLY).unwrap();
        let example = Name::parse("example.com").unwrap();
        assert!(message.header.response);
        assert_eq!(message.header.id, 0x1234);
        assert_eq!(
            message.questions,
            [Question::new(example.clone(), RecordType::A)]
        );
        let address = RData::A("93.184.216.34".parse().unwrap());
        assert_eq!(
            message.answers,
            [Record::new(example, RecordType::A, 3600, address)]
        );
        assert!(message.authorities.is_empty() && message.additionals.is_empty());
    }

    #[test]
    fn encoding_reproduces_the_original_bytes() {
        let message = Message::decode(REPLY).unwrap();
        assert_eq!(message.encode(), REPLY);
    }

    #[test]
    fn a_count_larger_than_the_message_is_an_error() {
        let mut bytes = REPLY.to_vec();
        bytes[7] = 2; // claim two answers
        assert_eq!(Message::decode(&bytes), Err(DnsError::UnexpectedEof));
        assert_eq!(Message::decode(&REPLY[..5]), Err(DnsError::UnexpectedEof));
    }

    #[test]
    fn builds_the_bytes_of_a_query() {
        let query = Message::query(0xABCD, Name::parse("example.com").unwrap(), RecordType::A);
        assert_eq!(
            query.encode(),
            b"\xAB\xCD\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\
              \x07example\x03com\x00\x00\x01\x00\x01"
        );
    }

    #[test]
    fn edns_adds_an_opt_record_carrying_the_payload_size() {
        let plain = Message::query(1, Name::parse("example.com").unwrap(), RecordType::A);
        assert_eq!(plain.edns_payload(), None);

        let bytes = plain.with_edns(1232).encode();
        assert_eq!(bytes[10..12], [0, 1], "one additional record");
        // Root name, type 41, "class" 1232, TTL 0, no data.
        assert!(bytes.ends_with(b"\x00\x00\x29\x04\xD0\x00\x00\x00\x00\x00\x00"));
        assert_eq!(Message::decode(&bytes).unwrap().edns_payload(), Some(1232));
    }
}
