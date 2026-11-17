use crate::bytes::{get_u32, put_u32};
use crate::device::{BLOCK_SIZE, BlockDevice};
use crate::error::{FsError, FsResult};
use crate::fs::FileSystem;
use crate::inode::{DIRECT, Inode};

/// How many block numbers fit in one block.
pub const POINTERS_PER_BLOCK: usize = BLOCK_SIZE / 4;

/// The most blocks of content one file can have.
pub const MAX_FILE_BLOCKS: usize = DIRECT + POINTERS_PER_BLOCK;
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

        // Past the direct pointers, the inode points at a block that holds
        // nothing but more pointers.
        let index = index - DIRECT;
        if index < POINTERS_PER_BLOCK {
            let Some(table) = self.pointer_target(&mut inode.indirect, allocate)? else {
                return Ok(None);
            };
            return self.table_target(table, index, allocate);
        }
        Err(FsError::FileTooLarge)
    }

    /// Follow pointer number `slot` inside the pointer block `table`.
    fn table_target(&mut self, table: u32, slot: usize, allocate: bool) -> FsResult<Option<u32>> {
        let mut contents = self.disk.read_block(table)?;
        let mut pointer = get_u32(&contents, slot * 4);
        let was_empty = pointer == 0;
        let target = self.pointer_target(&mut pointer, allocate)?;
        if was_empty && target.is_some() {
            put_u32(&mut contents, slot * 4, pointer);
            self.disk.write_block(table, &contents)?;
        }
        Ok(target)
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

    #[test]
    fn the_thirteenth_block_goes_through_an_indirect_block() {
        let mut fs = fresh();
        let before = free_blocks(&mut fs);
        let mut inode = Inode::new(InodeKind::File);
        let block = fs
            .atomic(|fs| fs.bmap(&mut inode, DIRECT, true))
            .unwrap()
            .unwrap();
        // One block for the table of pointers, one for the data.
        assert_eq!(free_blocks(&mut fs), before - 2);
        assert_ne!(inode.indirect, 0);
        assert_ne!(inode.indirect, block);
        // The table's first slot holds the data block's number.
        let table = fs.disk.read_block(inode.indirect).unwrap();
        assert_eq!(get_u32(&table, 0), block);

        // A neighbour shares the table, so it costs only its own block.
        fs.atomic(|fs| fs.bmap(&mut inode, DIRECT + 1, true))
            .unwrap();
        assert_eq!(free_blocks(&mut fs), before - 3);
        // Looking without allocating finds blocks, and reports holes.
        assert_eq!(fs.bmap(&mut inode, DIRECT, false), Ok(Some(block)));
        assert_eq!(fs.bmap(&mut inode, DIRECT + 2, false), Ok(None));
    }
}
