pub struct CsvParser<'a> {
    data: &'a [u8],
}

impl<'a> CsvParser<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn rows(&self) -> impl Iterator<Item = &'a [u8]> {
        self.data.split(|&b| b == b'\n')
    }
}