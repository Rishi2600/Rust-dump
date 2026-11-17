use crate::error::{FsError, FsResult};

/// Size of one block in bytes. Every read and write is a whole block.
pub const BLOCK_SIZE: usize = 1024;

/// The contents of one block.
pub type Block = [u8; BLOCK_SIZE];

/// A disk, seen the way a file system sees it: a numbered row of
/// fixed-size blocks that can be read and written one at a time.
///
/// Everything above this trait is logic and everything below it is storage,
/// so tests can run the whole file system against a disk held in memory.
pub trait BlockDevice {
    fn block_count(&self) -> u32;

    fn read_block(&mut self, index: u32) -> FsResult<Block>;

    fn write_block(&mut self, index: u32, block: &Block) -> FsResult<()>;

    /// Push everything written so far onto stable storage.
    fn flush(&mut self) -> FsResult<()> {
        Ok(())
    }
}

/// A disk that lives in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemDisk {
    blocks: Vec<Block>,
}

impl MemDisk {
    /// A disk of `block_count` blocks, all zero.
    pub fn new(block_count: u32) -> Self {
        Self {
            blocks: vec![[0; BLOCK_SIZE]; block_count as usize],
        }
    }
}

impl BlockDevice for MemDisk {
    fn block_count(&self) -> u32 {
        self.blocks.len() as u32
    }

    fn read_block(&mut self, index: u32) -> FsResult<Block> {
        let block = self.blocks.get(index as usize);
        block.copied().ok_or(FsError::OutOfRange(index))
    }

    fn write_block(&mut self, index: u32, block: &Block) -> FsResult<()> {
        let slot = self.blocks.get_mut(index as usize);
        *slot.ok_or(FsError::OutOfRange(index))? = *block;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_reads_back_what_was_written() {
        let mut disk = MemDisk::new(4);
        assert_eq!(disk.block_count(), 4);
        assert_eq!(disk.read_block(2), Ok([0; BLOCK_SIZE]));

        let mut block = [0; BLOCK_SIZE];
        block[0] = 0xAB;
        block[BLOCK_SIZE - 1] = 0xCD;
        disk.write_block(2, &block).unwrap();
        assert_eq!(disk.read_block(2), Ok(block));
        assert_eq!(disk.read_block(1), Ok([0; BLOCK_SIZE]));
    }

    #[test]
    fn blocks_past_the_end_do_not_exist() {
        let mut disk = MemDisk::new(4);
        assert_eq!(disk.read_block(4), Err(FsError::OutOfRange(4)));
        let written = disk.write_block(9, &[0; BLOCK_SIZE]);
        assert_eq!(written, Err(FsError::OutOfRange(9)));
        assert_eq!(disk.flush(), Ok(()));
    }
}
