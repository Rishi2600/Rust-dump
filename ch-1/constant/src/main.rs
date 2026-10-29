pub struct Lexer<'a> {
    input: &'a [u8],
    cursor: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input, cursor: 0 }
    }

    pub fn next_token(&mut self) -> Option<&'a [u8]> {
        while self.cursor < self.input.len() && self.input[self.cursor].is_ascii_whitespace() {
            self.cursor += 1;
        }
        if self.cursor >= self.input.len() { return None; }

        let start = self.cursor;
        while self.cursor < self.input.len() && !self.input[self.cursor].is_ascii_whitespace() {
            self.cursor += 1;
        }
        Some(&self.input[start..self.cursor])
    }
}