pub struct UriPathTokenizer<'a> {
    path: &'a str,
}

impl<'a> UriPathTokenizer<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }

    pub fn segments(&self) -> impl Iterator<Item = &'a str> {
        self.path.split('/').filter(|s| !s.is_empty())
    }
}