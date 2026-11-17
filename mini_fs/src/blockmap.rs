use crate::bytes::{get_u32, put_u32};
use crate::device::{BLOCK_SIZE, BlockDevice};
use crate::error::{FsError, FsResult};
use crate::fs::FileSystem;
use crate::inode::{DIRECT, Inode};

/// How many block numbers fit in one block.
pub const POINTERS_PER_BLOCK: usize = BLOCK_SIZE / 4;

/// The most blocks of content one file can have.
pub const MAX_FILE_BLOCKS: usize =
    DIRECT + POINTERS_PER_BLOCK + POINTERS_PER_BLOCK * POINTERS_PER_BLOCK;
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

        // Past that, one more level: a block of pointers to pointer blocks.
        // Two lookups pick first the inner table, then the slot inside it.
        let index = index - POINTERS_PER_BLOCK;
        if index < POINTERS_PER_BLOCK * POINTERS_PER_BLOCK {
            let Some(outer) = self.pointer_target(&mut inode.double_indirect, allocate)? else {
                return Ok(None);
            };
            let Some(inner) = self.table_target(outer, index / POINTERS_PER_BLOCK, allocate)?
            else {
                return Ok(None);
            };
            return self.table_target(inner, index % POINTERS_PER_BLOCK, allocate);
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

impl<D: BlockDevice> FileSystem<D> {
    /// Free every block of a file and leave it empty. The inode is updated
    /// in memory only: the caller writes it back or frees it.
    pub fn release_blocks(&mut self, inode: &mut Inode) -> FsResult<()> {
        for pointer in inode.direct {
            self.free_tree(pointer, 0)?;
        }
        self.free_tree(inode.indirect, 1)?;
        self.free_tree(inode.double_indirect, 2)?;
        inode.direct = [0; DIRECT];
        inode.indirect = 0;
        inode.double_indirect = 0;
        inode.size = 0;
        Ok(())
    }

    /// Free `block` and everything beneath it. `depth` is how many levels of
    /// pointer blocks lie between it and the data: 0 for a data block.
    fn free_tree(&mut self, block: u32, depth: u32) -> FsResult<()> {
        if block == 0 {
            return Ok(());
        }
        if depth > 0 {
            let table = self.disk.read_block(block)?;
            for slot in 0..POINTERS_PER_BLOCK {
                self.free_tree(get_u32(&table, slot * 4), depth - 1)?;
            }
        }
        self.free_block(block)
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Every disk block an inode uses: its data blocks and the pointer
    /// blocks that lead to them.
    pub fn block_list(&mut self, inode: &Inode) -> FsResult<Vec<u32>> {
        let mut blocks = Vec::new();
        for pointer in inode.direct {
            self.collect_tree(pointer, 0, &mut blocks)?;
        }
        self.collect_tree(inode.indirect, 1, &mut blocks)?;
        self.collect_tree(inode.double_indirect, 2, &mut blocks)?;
        Ok(blocks)
    }

    fn collect_tree(&mut self, block: u32, depth: u32, found: &mut Vec<u32>) -> FsResult<()> {
        if block == 0 {
            return Ok(());
        }
        found.push(block);
        // A pointer that leaves the data area is damage. It is listed so the
        // checker can report it, but it is not followed.
        let in_data_area = block >= self.sb.data_start && block < self.sb.total_blocks;
        if depth > 0 && in_data_area {
            let table = self.disk.read_block(block)?;
            for slot in 0..POINTERS_PER_BLOCK {
                self.collect_tree(get_u32(&table, slot * 4), depth - 1, found)?;
            }
        }
        Ok(())
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

    #[test]
    fn far_blocks_go_through_two_levels_of_pointer_blocks() {
        let mut fs = fresh();
        let before = free_blocks(&mut fs);
        let mut inode = Inode::new(InodeKind::File);
        let first = DIRECT + POINTERS_PER_BLOCK;
        fs.atomic(|fs| fs.bmap(&mut inode, first, true)).unwrap();
        // The outer table, one inner table, and the data block.
        assert_eq!(free_blocks(&mut fs), before - 3);

        // 256 blocks further on, a second inner table is needed.
        fs.atomic(|fs| fs.bmap(&mut inode, first + POINTERS_PER_BLOCK, true))
            .unwrap();
        assert_eq!(free_blocks(&mut fs), before - 5);
        let outer = fs.disk.read_block(inode.double_indirect).unwrap();
        assert_ne!(get_u32(&outer, 0), 0);
        assert_ne!(get_u32(&outer, 4), 0);
        assert_eq!(get_u32(&outer, 8), 0);
        // Neither the direct nor the single indirect pointers were touched.
        assert_eq!((inode.direct, inode.indirect), ([0; DIRECT], 0));
    }

    #[test]
    fn the_last_block_of_the_largest_possible_file_is_reachable() {
        let mut fs = fresh();
        let mut inode = Inode::new(InodeKind::File);
        let last = fs.atomic(|fs| fs.bmap(&mut inode, MAX_FILE_BLOCKS - 1, true));
        assert!(matches!(last, Ok(Some(_))));
        // 12 + 256 + 256 * 256 blocks of 1024 bytes: a little over 64 MB.
        assert_eq!(MAX_FILE_BLOCKS, 65_804);
        assert_eq!(MAX_FILE_SIZE, 67_383_296);
    }

    #[test]
    fn releasing_a_file_returns_every_block_including_pointer_blocks() {
        let mut fs = fresh();
        let before = free_blocks(&mut fs);
        let file = fs.atomic(|fs| fs.alloc_inode(InodeKind::File)).unwrap();
        // 300 blocks of content reach into the double indirect range.
        fs.write_at(file, 0, &vec![7; 300 * BLOCK_SIZE]).unwrap();
        // 300 data blocks, plus the indirect block, the outer table and one
        // inner table.
        assert_eq!(free_blocks(&mut fs), before - 303);

        let mut inode = fs.read_inode(file).unwrap();
        fs.atomic(|fs| {
            fs.release_blocks(&mut inode)?;
            fs.write_inode(file, &inode)
        })
        .unwrap();
        assert_eq!(free_blocks(&mut fs), before);
        assert_eq!(inode, Inode::new(InodeKind::File));
        // Releasing an empty file is harmless.
        assert_eq!(fs.atomic(|fs| fs.release_blocks(&mut inode)), Ok(()));
    }
}
