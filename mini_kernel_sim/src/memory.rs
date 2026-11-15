use crate::error::{KResult, KernelError};
use crate::frame::PAGE_SIZE;

/// Simulated RAM: a flat array of words carved into page frames.
pub struct PhysicalMemory {
    words: Vec<i64>,
}

impl PhysicalMemory {
    pub fn new(frames: usize) -> Self {
        Self {
            words: vec![0; frames * PAGE_SIZE],
        }
    }

    fn index(&self, frame: usize, offset: usize) -> KResult<usize> {
        let index = frame * PAGE_SIZE + offset;
        if offset < PAGE_SIZE && index < self.words.len() {
            Ok(index)
        } else {
            Err(KernelError::InvalidFrame(frame))
        }
    }

    pub fn read(&self, frame: usize, offset: usize) -> KResult<i64> {
        self.index(frame, offset).map(|index| self.words[index])
    }

    pub fn write(&mut self, frame: usize, offset: usize, value: i64) -> KResult<()> {
        let index = self.index(frame, offset)?;
        self.words[index] = value;
        Ok(())
    }

    /// Clear a frame before it is handed to a task.
    pub fn zero_frame(&mut self, frame: usize) -> KResult<()> {
        let start = self.index(frame, 0)?;
        self.words[start..start + PAGE_SIZE].fill(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_written_words_per_frame() {
        let mut memory = PhysicalMemory::new(2);
        memory.write(1, 3, -7).unwrap();
        assert_eq!(memory.read(1, 3), Ok(-7));
        assert_eq!(memory.read(0, 3), Ok(0));
        memory.zero_frame(1).unwrap();
        assert_eq!(memory.read(1, 3), Ok(0));
    }

    #[test]
    fn rejects_out_of_range_access() {
        let mut memory = PhysicalMemory::new(1);
        assert_eq!(memory.read(1, 0), Err(KernelError::InvalidFrame(1)));
        assert_eq!(
            memory.write(0, PAGE_SIZE, 1),
            Err(KernelError::InvalidFrame(0))
        );
    }
}
