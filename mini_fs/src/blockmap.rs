use crate::device::{BLOCK_SIZE, BlockDevice};
use crate::error::{FsError, FsResult};
use crate::fs::FileSystem;
use crate::inode::{DIRECT, Inode};

/// The most blocks of content one file can have.
pub const MAX_FILE_BLOCKS: usize = DIRECT;
pub const MAX_FILE_SIZE: usize = MAX_FILE_BLOCKS * BLOCK_SIZE;

impl<D: BlockDevice> FileSystem<D> {
    /// Translate a position in a file into a position on the disk.
    ///
    /// Returns the disk block holding block `index` of the file, or `None`
    /// if the file has a hole there. With `allocate` set, missing blocks are
    /// created on the way. The inode is updated in memory only: the caller
    /// writes it back.
    pub fn bmap(
        &mut self,
        inode: &mut Inode,
        index: usize,
        allocate: bool,
    ) -> FsResult<Option<u32>> {
        if index < DIRECT {
            return self.pointer_target(&mut inode.direct[index], allocate);
        }
        Err(FsError::FileTooLarge)
    }

    /// Follow a block pointer. An empty pointer (0) leads nowhere, unless
    /// `allocate` is set, in which case a new block is hung on it.
    fn pointer_target(&mut self, pointer: &mut u32, allocate: bool) -> FsResult<Option<u32>> {
        if *pointer == 0 {
            if !allocate {
                return Ok(None);
            }
            *pointer = self.alloc_block()?;
        }
        Ok(Some(*pointer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::MemDisk;
    use crate::fs::FormatOptions;
    use crate::inode::InodeKind;

    fn fresh() -> FileSystem<MemDisk> {
        FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap()
    }

    fn free_blocks(fs: &mut FileSystem<MemDisk>) -> u32 {
        fs.sb.total_blocks - fs.sb.block_bitmap().count_set(&mut fs.disk).unwrap()
    }

    #[test]
    fn the_first_blocks_are_pointed_at_directly() {
        let mut fs = fresh();
        let before = free_blocks(&mut fs);
        let mut inode = Inode::new(InodeKind::File);
        // Nothing is there until it is asked for.
        assert_eq!(fs.bmap(&mut inode, 3, false), Ok(None));

        let block = fs
            .atomic(|fs| fs.bmap(&mut inode, 3, true))
            .unwrap()
            .unwrap();
        assert!(block >= fs.sb.data_start);
        assert_eq!(inode.direct[3], block);
        assert_eq!(free_blocks(&mut fs), before - 1);

        // Asking again finds the same block instead of making another.
        assert_eq!(
            fs.atomic(|fs| fs.bmap(&mut inode, 3, true)),
            Ok(Some(block))
        );
        assert_eq!(free_blocks(&mut fs), before - 1);
    }

    #[test]
    fn a_position_past_the_largest_file_is_refused() {
        let mut fs = fresh();
        let mut inode = Inode::new(InodeKind::File);
        let beyond = fs.atomic(|fs| fs.bmap(&mut inode, MAX_FILE_BLOCKS, true));
        assert_eq!(beyond, Err(FsError::FileTooLarge));
    }
}
