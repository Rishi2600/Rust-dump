pub type ParseResult<'a, T> = Result<(&'a [u8], T), &'static str>;

pub trait Parser<'a, T> {
    fn parse(&self, input: &'a [u8]) -> ParseResult<'a, T>;
}

struct TagParser<'a> {
    expected: &'a [u8],
}

impl<'a> Parser<'a, ()> for TagParser<'a> {
    fn parse(&self, input: &'a [u8]) -> ParseResult<'a, ()> {
        if input.starts_with(self.expected) {
            Ok((&input[self.expected.len()..], ()))
        } else {
            Err("Tag mismatch")
        }
    }
}