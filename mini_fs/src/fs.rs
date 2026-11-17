use crate::device::{BLOCK_SIZE, BlockDevice};
use crate::error::{FsError, FsResult};
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
}
