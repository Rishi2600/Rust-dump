pub struct FrameBitmap<const SIZE: usize> {
    bitmap: [u8; SIZE],
}

impl<const SIZE: usize> FrameBitmap<SIZE> {
    pub const fn new() -> Self {
        Self { bitmap: [0; SIZE] }
    }

    pub fn mark_allocated(&mut self, frame_index: usize) {
        let byte_idx = frame_index / 8;
        let bit_idx = frame_index % 8;
        self.bitmap[byte_idx] |= 1 << bit_idx;
    }
}