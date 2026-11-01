pub struct WindowIter<'a, T> {
    slice: &'a [T],
    size: usize,
}

impl<'a, T> Iterator for WindowIter<'a, T> {
    type Item = &'a [T];
    fn next(&mut self) -> Option<Self::Item> {
        if self.slice.len() < self.size { return None; }
        let res = &self.slice[..self.size];
        self.slice = &self.slice[1..];
        Some(res)
    }
}