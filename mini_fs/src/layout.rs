use crate::device::BLOCK_SIZE;
use crate::error::{FsError, FsResult};

/// Size of one inode on disk, in bytes.
pub const INODE_SIZE: usize = 64;
pub const INODES_PER_BLOCK: usize = BLOCK_SIZE / INODE_SIZE;
/// How many items one block of a bitmap can track.
pub const BITS_PER_BLOCK: usize = BLOCK_SIZE * 8;

/// Inode number of the root directory. Number 0 means "no inode".
pub const ROOT_INODE: u32 = 1;

/// The journal needs a header block plus room for a useful transaction.
pub const MIN_JOURNAL_BLOCKS: u32 = 16;
/// The journal header lists the logged blocks as 4-byte numbers after a
/// 4-byte count, which caps how many it can describe.
pub const MAX_JOURNAL_BLOCKS: u32 = 1 + (BLOCK_SIZE as u32 - 4) / 4;
/// Journal slots kept spare, beyond the block bitmap, when freeing a file.
const JOURNAL_HEADROOM: u32 = 8;

/// Where everything is on the disk.
///
/// ```text
/// | super | journal | inode bitmap | block bitmap | inode table | data ... |
///   0       1
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Superblock {
    pub total_blocks: u32,
    pub inode_count: u32,
    pub journal_start: u32,
    pub journal_blocks: u32,
    pub inode_bitmap_start: u32,
    pub inode_bitmap_blocks: u32,
    pub block_bitmap_start: u32,
    pub block_bitmap_blocks: u32,
    pub inode_table_start: u32,
    pub inode_table_blocks: u32,
    /// First block that can hold file contents.
    pub data_start: u32,
}

impl Superblock {
    /// Work out the layout for a disk of `total_blocks` blocks.
    pub fn plan(total_blocks: u32, inode_count: u32, journal_blocks: u32) -> FsResult<Self> {
        if inode_count < 2 {
            return Err(FsError::InvalidOptions(
                "at least 2 inodes are needed, one of them for the root directory".into(),
            ));
        }
        if !(MIN_JOURNAL_BLOCKS..=MAX_JOURNAL_BLOCKS).contains(&journal_blocks) {
            return Err(FsError::InvalidOptions(format!(
                "the journal must be {MIN_JOURNAL_BLOCKS} to {MAX_JOURNAL_BLOCKS} blocks"
            )));
        }
        let blocks_for = |items: u32, per_block: usize| items.div_ceil(per_block as u32);
        let inode_bitmap_blocks = blocks_for(inode_count, BITS_PER_BLOCK);
        let block_bitmap_blocks = blocks_for(total_blocks, BITS_PER_BLOCK);
        let inode_table_blocks = blocks_for(inode_count, INODES_PER_BLOCK);

        // Each region starts where the one before it ends.
        let journal_start = 1;
        let inode_bitmap_start = journal_start + journal_blocks;
        let block_bitmap_start = inode_bitmap_start + inode_bitmap_blocks;
        let inode_table_start = block_bitmap_start + block_bitmap_blocks;
        let data_start = u64::from(inode_table_start) + u64::from(inode_table_blocks);
        if data_start >= u64::from(total_blocks) {
            return Err(FsError::InvalidOptions(format!(
                "{total_blocks} blocks is too small, the bookkeeping alone needs {data_start}"
            )));
        }
        // Deleting a file is one transaction, and it may touch every block
        // of the block bitmap. The journal has to be able to hold that.
        if journal_blocks - 1 < block_bitmap_blocks + JOURNAL_HEADROOM {
            return Err(FsError::InvalidOptions(
                "the journal is too small for a disk this large".into(),
            ));
        }
        Ok(Self {
            total_blocks,
            inode_count,
            journal_start,
            journal_blocks,
            inode_bitmap_start,
            inode_bitmap_blocks,
            block_bitmap_start,
            block_bitmap_blocks,
            inode_table_start,
            inode_table_blocks,
            data_start: data_start as u32,
        })
    }

    /// Blocks available for file contents.
    pub fn data_blocks(&self) -> u32 {
        self.total_blocks - self.data_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_follow_one_another_without_gaps() {
        let sb = Superblock::plan(512, 128, 32).unwrap();
        assert_eq!((sb.journal_start, sb.journal_blocks), (1, 32));
        assert_eq!((sb.inode_bitmap_start, sb.inode_bitmap_blocks), (33, 1));
        assert_eq!((sb.block_bitmap_start, sb.block_bitmap_blocks), (34, 1));
        // 128 inodes of 64 bytes fill 8 blocks of 1024 bytes.
        assert_eq!((sb.inode_table_start, sb.inode_table_blocks), (35, 8));
        assert_eq!(sb.data_start, 43);
        assert_eq!(sb.data_blocks(), 469);
    }

    #[test]
    fn bitmaps_grow_with_the_things_they_track() {
        // 20000 blocks need 3 bitmap blocks of 8192 bits each.
        let sb = Superblock::plan(20_000, 9000, 64).unwrap();
        assert_eq!(sb.block_bitmap_blocks, 3);
        assert_eq!(sb.inode_bitmap_blocks, 2);
        assert_eq!(sb.inode_table_blocks, 563);
    }

    #[test]
    fn impossible_layouts_are_refused() {
        let refused = |total, inodes, journal| {
            matches!(
                Superblock::plan(total, inodes, journal),
                Err(FsError::InvalidOptions(_))
            )
        };
        assert!(refused(512, 1, 32), "no room for a root directory");
        assert!(refused(512, 128, 4), "journal too short");
        assert!(
            refused(512, 128, 1000),
            "journal longer than its header can describe"
        );
        assert!(refused(40, 128, 32), "nothing left for data");
        assert!(
            refused(4_000_000, 128, 16),
            "journal too small to free a big file"
        );
        assert!(
            refused(512, u32::MAX, 32),
            "inode table larger than the disk"
        );
    }
}
