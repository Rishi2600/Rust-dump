pub struct Interner {
    buffer: Vec<u8>,
}

impl Interner {
    pub fn new() -> Self { Self { buffer: Vec::new() } }
    pub fn intern(&mut self, s: &str) -> &[u8] {
        let start = self.buffer.len();
        self.buffer.extend_from_slice(s.as_bytes());
        &self.buffer[start..]
    }
}