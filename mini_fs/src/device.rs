use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

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

/// A disk stored in an ordinary file: block `n` is the bytes from
/// `n * BLOCK_SIZE` onwards.
#[derive(Debug)]
pub struct FileDisk {
    file: File,
    block_count: u32,
}

impl FileDisk {
    /// Create an image of `block_count` zeroed blocks, replacing any file
    /// already at `path`.
    pub fn create(path: impl AsRef<Path>, block_count: u32) -> FsResult<Self> {
        let mut options = OpenOptions::new();
        let file = options
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(u64::from(block_count) * BLOCK_SIZE as u64)?;
        Ok(Self { file, block_count })
    }

    /// Open an existing image.
    pub fn open(path: impl AsRef<Path>) -> FsResult<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let length = file.metadata()?.len();
        let blocks = length / BLOCK_SIZE as u64;
        if length % BLOCK_SIZE as u64 != 0 || blocks > u64::from(u32::MAX) {
            return Err(FsError::BadImage(format!(
                "its size of {length} bytes is not a whole number of blocks"
            )));
        }
        Ok(Self {
            file,
            block_count: blocks as u32,
        })
    }

    fn seek_to(&mut self, index: u32) -> FsResult<()> {
        if index >= self.block_count {
            return Err(FsError::OutOfRange(index));
        }
        let offset = u64::from(index) * BLOCK_SIZE as u64;
        self.file.seek(SeekFrom::Start(offset))?;
        Ok(())
    }
}

impl BlockDevice for FileDisk {
    fn block_count(&self) -> u32 {
        self.block_count
    }

    fn read_block(&mut self, index: u32) -> FsResult<Block> {
        self.seek_to(index)?;
        let mut block = [0; BLOCK_SIZE];
        self.file.read_exact(&mut block)?;
        Ok(block)
    }

    fn write_block(&mut self, index: u32, block: &Block) -> FsResult<()> {
        self.seek_to(index)?;
        self.file.write_all(block)?;
        Ok(())
    }

    /// Ask the operating system to put the data on the physical disk. Until
    /// this returns, a write may exist only in memory and be lost in a crash.
    fn flush(&mut self) -> FsResult<()> {
        self.file.sync_data()?;
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

    /// A path under the build directory that no other test uses.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-images");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(format!("{name}-{}.img", std::process::id()))
    }

    #[test]
    fn a_file_disk_keeps_its_blocks_across_reopening() {
        let path = scratch("reopen");
        let mut block = [0; BLOCK_SIZE];
        block[..5].copy_from_slice(b"hello");

        let mut disk = FileDisk::create(&path, 8).unwrap();
        disk.write_block(3, &block).unwrap();
        disk.flush().unwrap();
        drop(disk);

        let mut disk = FileDisk::open(&path).unwrap();
        assert_eq!(disk.block_count(), 8);
        assert_eq!(disk.read_block(3), Ok(block));
        assert_eq!(disk.read_block(4), Ok([0; BLOCK_SIZE]));
        assert_eq!(disk.read_block(8), Err(FsError::OutOfRange(8)));
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            8 * BLOCK_SIZE as u64
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_file_that_is_not_whole_blocks_is_rejected() {
        let path = scratch("ragged");
        std::fs::write(&path, [0; BLOCK_SIZE + 1]).unwrap();
        assert!(matches!(FileDisk::open(&path), Err(FsError::BadImage(_))));
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(FileDisk::open(&path), Err(FsError::Io(_))));
    }
}
