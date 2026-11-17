use crate::device::{BLOCK_SIZE, BlockDevice};
use crate::error::FsResult;
use crate::inode::{Inode, InodeKind};
use crate::journal::Journal;
use crate::layout::{ROOT_INODE, Superblock};

/// Choices made once, when an image is created.
#[derive(Debug, Clone, Copy)]
pub struct FormatOptions {
    /// How many files and directories the image can ever hold.
    pub inodes: u32,
    /// Blocks set aside for the journal.
    pub journal_blocks: u32,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            inodes: 128,
            journal_blocks: 32,
        }
    }
}

/// A mounted file system: a disk, and the layout read from its superblock.
pub struct FileSystem<D: BlockDevice> {
    pub(crate) disk: Journal<D>,
    pub(crate) sb: Superblock,
}

impl<D: BlockDevice> FileSystem<D> {
    /// Create an empty file system on `device`, destroying what was there.
    pub fn format(mut device: D, options: FormatOptions) -> FsResult<Self> {
        let sb = Superblock::plan(device.block_count(), options.inodes, options.journal_blocks)?;

        // Wipe the bookkeeping blocks: a disk that held something else must
        // not leave stale bitmaps, inodes or a half-finished journal behind.
        for block in 1..sb.data_start {
            device.write_block(block, &[0; BLOCK_SIZE])?;
        }

        // Inode 0 means "no inode", so it is marked used and never handed out.
        let inodes = sb.inode_bitmap();
        inodes.set(&mut device, 0, true)?;
        inodes.set(&mut device, ROOT_INODE, true)?;

        // The bookkeeping blocks themselves are not available for file data.
        let blocks = sb.block_bitmap();
        for block in 0..sb.data_start {
            blocks.set(&mut device, block, true)?;
        }

        // The root directory starts out empty.
        let (table_block, offset) = sb.inode_location(ROOT_INODE)?;
        let mut table = device.read_block(table_block)?;
        Inode::new(InodeKind::Dir).encode(&mut table[offset..]);
        device.write_block(table_block, &table)?;

        // The superblock goes last. Until it is written, nothing will mistake
        // a half-formatted disk for a file system.
        device.write_block(0, &sb.encode())?;
        device.flush()?;

        let disk = Journal::new(device, sb.journal_start, sb.journal_blocks);
        Ok(Self { disk, sb })
    }

    pub fn superblock(&self) -> &Superblock {
        &self.sb
    }

    /// Give the disk back, for example to mount it again.
    pub fn into_device(self) -> D {
        self.disk.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::MemDisk;
    use crate::error::FsError;

    #[test]
    fn format_lays_out_an_empty_file_system() {
        let fs = FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap();
        let sb = *fs.superblock();
        let mut disk = fs.into_device();
        assert_eq!(Superblock::decode(&disk.read_block(0).unwrap()), Ok(sb));

        // Only the reserved inode and the root directory are in use.
        assert_eq!(sb.inode_bitmap().count_set(&mut disk), Ok(2));
        // Exactly the bookkeeping blocks are in use; all data blocks are free.
        assert_eq!(sb.block_bitmap().count_set(&mut disk), Ok(sb.data_start));
        assert_eq!(sb.block_bitmap().get(&mut disk, sb.data_start), Ok(false));

        let (block, offset) = sb.inode_location(ROOT_INODE).unwrap();
        let root = Inode::decode(&disk.read_block(block).unwrap()[offset..]).unwrap();
        assert_eq!(root, Inode::new(InodeKind::Dir));
    }

    #[test]
    fn format_wipes_whatever_was_on_the_disk() {
        let mut dirty = MemDisk::new(512);
        for block in 0..512 {
            dirty.write_block(block, &[0xFF; BLOCK_SIZE]).unwrap();
        }
        let fs = FileSystem::format(dirty, FormatOptions::default()).unwrap();
        let sb = *fs.superblock();
        let mut disk = fs.into_device();
        assert_eq!(sb.inode_bitmap().count_set(&mut disk), Ok(2));
        assert_eq!(sb.block_bitmap().count_set(&mut disk), Ok(sb.data_start));
        assert_eq!(disk.read_block(sb.journal_start), Ok([0; BLOCK_SIZE]));
    }

    #[test]
    fn a_disk_too_small_to_format_is_refused() {
        let tiny = FileSystem::format(MemDisk::new(20), FormatOptions::default());
        assert!(matches!(tiny, Err(FsError::InvalidOptions(_))));
    }
}
