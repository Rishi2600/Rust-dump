pub struct ChunkWindows<'a, T> {
    slice: &'a [T],
    window_size: usize,
}

impl<'a, T> Iterator for ChunkWindows<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<Self::Item> {
        if self.slice.len() < self.window_size {
            None
        } else {
            let res = &self.slice[..self.window_size];
            self.slice = &self.slice[1..];
            Some(res)
        }
    }
}