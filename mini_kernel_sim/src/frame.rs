use crate::error::{KResult, KernelError};

/// Number of machine words in one page frame.
pub const PAGE_SIZE: usize = 64;

/// Bitmap allocator handing out physical frame numbers.
pub struct FrameAllocator {
    bitmap: Vec<u8>,
    total: usize,
    used: usize,
}

impl FrameAllocator {
    pub fn new(total: usize) -> Self {
        Self {
            bitmap: vec![0; total.div_ceil(8)],
            total,
            used: 0,
        }
    }

    /// Allocate the lowest free frame.
    pub fn alloc(&mut self) -> KResult<usize> {
        let frame = (0..self.total)
            .find(|&frame| !self.is_allocated(frame))
            .ok_or(KernelError::OutOfFrames)?;
        self.bitmap[frame / 8] |= 1 << (frame % 8);
        self.used += 1;
        Ok(frame)
    }

    pub fn is_allocated(&self, frame: usize) -> bool {
        frame < self.total && self.bitmap[frame / 8] & (1 << (frame % 8)) != 0
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn used(&self) -> usize {
        self.used
    }
}

impl FrameAllocator {
    /// Return a frame to the pool.
    pub fn free(&mut self, frame: usize) -> KResult<()> {
        if frame >= self.total {
            return Err(KernelError::InvalidFrame(frame));
        }
        if !self.is_allocated(frame) {
            return Err(KernelError::DoubleFree(frame));
        }
        self.bitmap[frame / 8] &= !(1 << (frame % 8));
        self.used -= 1;
        Ok(())
    }

    pub fn available(&self) -> usize {
        self.total - self.used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_lowest_free_frame_first() {
        let mut frames = FrameAllocator::new(3);
        assert_eq!(frames.alloc(), Ok(0));
        assert_eq!(frames.alloc(), Ok(1));
        frames.free(0).unwrap();
        assert_eq!(frames.alloc(), Ok(0));
        assert_eq!(
            (frames.used(), frames.available(), frames.total()),
            (2, 1, 3)
        );
    }

    #[test]
    fn reports_exhaustion_and_bad_frees() {
        let mut frames = FrameAllocator::new(1);
        frames.alloc().unwrap();
        assert_eq!(frames.alloc(), Err(KernelError::OutOfFrames));
        assert_eq!(frames.free(5), Err(KernelError::InvalidFrame(5)));
        frames.free(0).unwrap();
        assert_eq!(frames.free(0), Err(KernelError::DoubleFree(0)));
    }
}
