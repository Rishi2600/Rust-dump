use crate::error::DnsResult;
use crate::name::Name;
use crate::rtype::{CLASS_IN, RecordType};
use crate::wire::{Reader, Writer};

/// What a query asks: records of one type for one name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub name: Name,
    pub rtype: RecordType,
    pub class: u16,
}

impl Question {
    pub fn new(name: Name, rtype: RecordType) -> Self {
        Self {
            name,
            rtype,
            class: CLASS_IN,
        }
    }

    pub fn encode(&self, writer: &mut Writer) {
        self.name.encode(writer);
        writer.write_u16(self.rtype.code());
        writer.write_u16(self.class);
    }

    pub fn decode(reader: &mut Reader<'_>) -> DnsResult<Self> {
        Ok(Self {
            name: Name::decode(reader)?,
            rtype: RecordType::from_code(reader.read_u16()?),
            class: reader.read_u16()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_round_trips_through_the_wire_format() {
        let question = Question::new(Name::parse("example.com").unwrap(), RecordType::Mx);
        let mut writer = Writer::new();
        question.encode(&mut writer);
        let bytes = writer.finish();
        assert_eq!(bytes, b"\x07example\x03com\x00\x00\x0F\x00\x01");
        assert_eq!(Question::decode(&mut Reader::new(&bytes)), Ok(question));
    }
}
