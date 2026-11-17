use crate::device::{BLOCK_SIZE, BlockDevice};
use crate::error::{FsError, FsResult};
use crate::inode::{Inode, InodeKind};
use crate::journal::Journal;
use crate::layout::{INODE_SIZE, ROOT_INODE, Superblock};

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

impl<D: BlockDevice> FileSystem<D> {
    /// Open the file system on `device`.
    ///
    /// Mounting also finishes any transaction that a crash interrupted after
    /// its commit point, so the rest of the code always sees a consistent disk.
    pub fn mount(mut device: D) -> FsResult<Self> {
        let sb = Superblock::decode(&device.read_block(0)?)?;
        if sb.total_blocks != device.block_count() {
            return Err(FsError::BadImage(format!(
                "it was made for {} blocks but the disk has {}",
                sb.total_blocks,
                device.block_count()
            )));
        }
        let mut disk = Journal::new(device, sb.journal_start, sb.journal_blocks);
        disk.recover()?;
        Ok(Self { disk, sb })
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Read inode `number` from the inode table.
    pub fn read_inode(&mut self, number: u32) -> FsResult<Inode> {
        let (block, offset) = self.sb.inode_location(number)?;
        Inode::decode(&self.disk.read_block(block)?[offset..])
    }

    /// Store inode `number`. Like every write, this belongs inside
    /// [`FileSystem::atomic`].
    pub fn write_inode(&mut self, number: u32, inode: &Inode) -> FsResult<()> {
        let (block, offset) = self.sb.inode_location(number)?;
        // Sixteen inodes share a block, so the others must be kept intact.
        let mut table = self.disk.read_block(block)?;
        inode.encode(&mut table[offset..]);
        self.disk.write_block(block, &table)
    }

    /// Run `operation` as one transaction: either every block it writes
    /// reaches the disk, or none of them do.
    ///
    /// If the operation fails part-way, its writes are simply dropped, so an
    /// error never leaves the file system half-changed.
    pub fn atomic<R>(&mut self, operation: impl FnOnce(&mut Self) -> FsResult<R>) -> FsResult<R> {
        let result = operation(self).and_then(|value| {
            self.disk.commit()?;
            Ok(value)
        });
        if result.is_err() {
            self.disk.abort();
        }
        result
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Claim a free inode and initialise it as an empty file or directory.
    pub fn alloc_inode(&mut self, kind: InodeKind) -> FsResult<u32> {
        let bitmap = self.sb.inode_bitmap();
        let number = bitmap.allocate(&mut self.disk)?.ok_or(FsError::NoInodes)?;
        self.write_inode(number, &Inode::new(kind))?;
        Ok(number)
    }

    /// Return an inode to the pool. Its blocks must already have been freed.
    pub fn free_inode(&mut self, number: u32) -> FsResult<()> {
        let bitmap = self.sb.inode_bitmap();
        if number == ROOT_INODE || !bitmap.get(&mut self.disk, number)? {
            return Err(FsError::Corrupt(format!("inode {number} cannot be freed")));
        }
        let (block, offset) = self.sb.inode_location(number)?;
        let mut table = self.disk.read_block(block)?;
        table[offset..offset + INODE_SIZE].fill(0);
        self.disk.write_block(block, &table)?;
        bitmap.set(&mut self.disk, number, false)
    }

    /// Claim a free data block. It is handed out full of zeros, so a new
    /// file can never show what a deleted one left behind.
    pub fn alloc_block(&mut self) -> FsResult<u32> {
        let bitmap = self.sb.block_bitmap();
        let block = bitmap.allocate(&mut self.disk)?.ok_or(FsError::NoSpace)?;
        self.disk.write_block(block, &[0; BLOCK_SIZE])?;
        Ok(block)
    }

    /// Return a data block to the pool.
    pub fn free_block(&mut self, block: u32) -> FsResult<()> {
        let bitmap = self.sb.block_bitmap();
        let is_data = block >= self.sb.data_start && block < self.sb.total_blocks;
        if !is_data || !bitmap.get(&mut self.disk, block)? {
            return Err(FsError::Corrupt(format!("block {block} cannot be freed")));
        }
        bitmap.set(&mut self.disk, block, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::MemDisk;

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

    #[test]
    fn a_formatted_disk_can_be_mounted_again() {
        let fs = FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap();
        let sb = *fs.superblock();
        let mounted = FileSystem::mount(fs.into_device()).unwrap();
        assert_eq!(*mounted.superblock(), sb);
    }

    #[test]
    fn mounting_rejects_disks_that_are_not_ours() {
        let blank = FileSystem::mount(MemDisk::new(512));
        assert!(matches!(blank, Err(FsError::BadImage(_))));

        // A superblock copied onto a disk of a different size.
        let mut formatted = FileSystem::format(MemDisk::new(512), FormatOptions::default())
            .unwrap()
            .into_device();
        let mut larger = MemDisk::new(600);
        larger
            .write_block(0, &formatted.read_block(0).unwrap())
            .unwrap();
        assert!(matches!(
            FileSystem::mount(larger),
            Err(FsError::BadImage(_))
        ));
    }

    #[test]
    fn mounting_replays_a_transaction_left_in_the_journal() {
        use crate::bytes::put_u32;
        let fs = FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap();
        let sb = *fs.superblock();
        let mut disk = fs.into_device();

        // Fake a crash just after a commit: one block logged, not installed.
        let target = sb.data_start + 5;
        disk.write_block(sb.journal_start + 1, &[0x5A; BLOCK_SIZE])
            .unwrap();
        let mut header = [0; BLOCK_SIZE];
        put_u32(&mut header, 0, 1);
        put_u32(&mut header, 4, target);
        disk.write_block(sb.journal_start, &header).unwrap();

        let mut disk = FileSystem::mount(disk).unwrap().into_device();
        assert_eq!(disk.read_block(target), Ok([0x5A; BLOCK_SIZE]));
        assert_eq!(disk.read_block(sb.journal_start), Ok([0; BLOCK_SIZE]));
    }

    fn fresh() -> FileSystem<MemDisk> {
        FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap()
    }

    #[test]
    fn an_inode_written_in_a_transaction_survives_remounting() {
        let mut fs = fresh();
        let mut file = Inode::new(InodeKind::File);
        file.size = 1234;
        fs.atomic(|fs| fs.write_inode(7, &file)).unwrap();

        let mut fs = FileSystem::mount(fs.into_device()).unwrap();
        assert_eq!(fs.read_inode(7), Ok(file));
        // Its neighbours in the same table block were left alone.
        assert_eq!(fs.read_inode(ROOT_INODE), Ok(Inode::new(InodeKind::Dir)));
        assert_eq!(fs.read_inode(6).unwrap().kind, InodeKind::Free);
        assert!(fs.read_inode(0).is_err());
    }

    #[test]
    fn a_failed_operation_leaves_no_trace() {
        let mut fs = fresh();
        let result: FsResult<()> = fs.atomic(|fs| {
            let mut root = fs.read_inode(ROOT_INODE)?;
            root.size = 999;
            fs.write_inode(ROOT_INODE, &root)?;
            // The operation can see its own change...
            assert_eq!(fs.read_inode(ROOT_INODE)?.size, 999);
            Err(FsError::NoSpace)
        });
        // ...but once it fails, the change is gone.
        assert_eq!(result, Err(FsError::NoSpace));
        assert_eq!(fs.read_inode(ROOT_INODE).unwrap().size, 0);
    }

    #[test]
    fn inodes_are_handed_out_lowest_first_and_reused() {
        let mut fs = fresh();
        let (first, second) = fs
            .atomic(|fs| {
                Ok((
                    fs.alloc_inode(InodeKind::File)?,
                    fs.alloc_inode(InodeKind::Dir)?,
                ))
            })
            .unwrap();
        assert_eq!((first, second), (2, 3));
        assert_eq!(fs.read_inode(3), Ok(Inode::new(InodeKind::Dir)));

        fs.atomic(|fs| fs.free_inode(2)).unwrap();
        assert_eq!(fs.read_inode(2).unwrap().kind, InodeKind::Free);
        assert_eq!(fs.atomic(|fs| fs.alloc_inode(InodeKind::File)), Ok(2));
        // The root and inodes that are not in use cannot be freed.
        assert!(fs.atomic(|fs| fs.free_inode(ROOT_INODE)).is_err());
        assert!(fs.atomic(|fs| fs.free_inode(50)).is_err());
    }

    #[test]
    fn running_out_of_inodes_is_an_error_that_changes_nothing() {
        let mut fs = fresh();
        // 128 inodes, minus the reserved one and the root.
        for _ in 0..126 {
            fs.atomic(|fs| fs.alloc_inode(InodeKind::File)).unwrap();
        }
        assert_eq!(
            fs.atomic(|fs| fs.alloc_inode(InodeKind::File)),
            Err(FsError::NoInodes)
        );
        assert_eq!(fs.sb.inode_bitmap().count_set(&mut fs.disk), Ok(128));
    }

    #[test]
    fn a_fresh_block_is_zeroed_even_on_a_dirty_disk() {
        let mut dirty = MemDisk::new(512);
        for block in 0..512 {
            dirty.write_block(block, &[0xFF; BLOCK_SIZE]).unwrap();
        }
        let mut fs = FileSystem::format(dirty, FormatOptions::default()).unwrap();
        let block = fs.atomic(|fs| fs.alloc_block()).unwrap();
        assert_eq!(block, fs.sb.data_start);
        assert_eq!(fs.disk.read_block(block), Ok([0; BLOCK_SIZE]));
    }

    #[test]
    fn only_data_blocks_that_are_in_use_can_be_freed() {
        let mut fs = fresh();
        let block = fs.atomic(|fs| fs.alloc_block()).unwrap();
        assert_eq!(fs.atomic(|fs| fs.free_block(block)), Ok(()));
        // Freeing twice, or freeing the superblock, would corrupt the disk.
        assert!(matches!(
            fs.atomic(|fs| fs.free_block(block)),
            Err(FsError::Corrupt(_))
        ));
        assert!(matches!(
            fs.atomic(|fs| fs.free_block(0)),
            Err(FsError::Corrupt(_))
        ));
        assert!(matches!(
            fs.atomic(|fs| fs.free_block(9999)),
            Err(FsError::Corrupt(_))
        ));
    }

    #[test]
    fn a_full_disk_reports_no_space() {
        let mut fs = fresh();
        let free = fs.sb.data_blocks();
        // Small transactions, so each one fits in the journal.
        for _ in 0..free {
            fs.atomic(|fs| fs.alloc_block()).unwrap();
        }
        assert_eq!(fs.atomic(|fs| fs.alloc_block()), Err(FsError::NoSpace));
    }
}
