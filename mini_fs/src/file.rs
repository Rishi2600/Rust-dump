use crate::blockmap::MAX_FILE_SIZE;
use crate::device::{BLOCK_SIZE, BlockDevice};
use crate::error::{FsError, FsResult};
use crate::fs::FileSystem;
use crate::inode::InodeKind;

impl<D: BlockDevice> FileSystem<D> {
    /// Read from the content of inode `number`, starting `offset` bytes in.
    ///
    /// Returns how many bytes were read. That is less than `buf.len()` when
    /// the end of the file comes first, and 0 at or past the end.
    pub fn read_at(&mut self, number: u32, offset: usize, buf: &mut [u8]) -> FsResult<usize> {
        let mut inode = self.read_inode(number)?;
        let size = inode.size as usize;
        if offset >= size {
            return Ok(0);
        }
        let len = buf.len().min(size - offset);
        let mut done = 0;
        while done < len {
            // Work out which block of the file this byte is in, and how much
            // of that block is wanted.
            let position = offset + done;
            let (index, within) = (position / BLOCK_SIZE, position % BLOCK_SIZE);
            let step = (BLOCK_SIZE - within).min(len - done);
            let target = &mut buf[done..done + step];
            match self.bmap(&mut inode, index, false)? {
                Some(block) => {
                    let contents = self.disk.read_block(block)?;
                    target.copy_from_slice(&contents[within..within + step]);
                }
                // A hole: no block was ever written here, so it reads as zeros.
                None => target.fill(0),
            }
            done += step;
        }
        Ok(len)
    }
}

/// Journal slots a write may need beyond its data blocks: two bitmap blocks,
/// the inode's block, up to three pointer blocks, and one more data block
/// when the write does not start on a block boundary.
const WRITE_OVERHEAD: usize = 8;

impl<D: BlockDevice> FileSystem<D> {
    /// Write `data` into the content of inode `number` at `offset`, growing
    /// the file as needed. This is one step of a transaction, so it belongs
    /// inside [`FileSystem::atomic`]; [`FileSystem::write_at`] does that.
    ///
    /// Writing past the current end leaves a hole: the skipped range takes
    /// no disk space and reads back as zeros.
    pub fn write_span(&mut self, number: u32, offset: usize, data: &[u8]) -> FsResult<()> {
        let end = offset
            .checked_add(data.len())
            .filter(|&end| end <= MAX_FILE_SIZE)
            .ok_or(FsError::FileTooLarge)?;
        let mut inode = self.read_inode(number)?;
        let mut done = 0;
        while done < data.len() {
            let position = offset + done;
            let (index, within) = (position / BLOCK_SIZE, position % BLOCK_SIZE);
            let step = (BLOCK_SIZE - within).min(data.len() - done);
            let block = self
                .bmap(&mut inode, index, true)?
                .ok_or(FsError::NoSpace)?;
            // A block that is only partly overwritten keeps its other bytes.
            let mut contents = if step == BLOCK_SIZE {
                [0; BLOCK_SIZE]
            } else {
                self.disk.read_block(block)?
            };
            contents[within..within + step].copy_from_slice(&data[done..done + step]);
            self.disk.write_block(block, &contents)?;
            done += step;
        }
        inode.size = inode.size.max(end as u32);
        self.write_inode(number, &inode)
    }

    /// Write `data` into a file at `offset`.
    ///
    /// A transaction has to fit in the journal, so a long write is split
    /// into several. Each piece is atomic; the write as a whole is not, and
    /// a crash or a full disk can leave the file with only its first pieces.
    pub fn write_at(&mut self, number: u32, offset: usize, data: &[u8]) -> FsResult<()> {
        match self.read_inode(number)?.kind {
            InodeKind::File => {}
            InodeKind::Dir => return Err(FsError::IsADirectory(format!("inode {number}"))),
            InodeKind::Free => return Err(FsError::NotFound(format!("inode {number}"))),
        }
        let blocks = self.disk.capacity().saturating_sub(WRITE_OVERHEAD).max(1);
        let chunk = blocks * BLOCK_SIZE;
        let mut done = 0;
        loop {
            let step = chunk.min(data.len() - done);
            let piece = &data[done..done + step];
            self.atomic(|fs| fs.write_span(number, offset + done, piece))?;
            done += step;
            if done == data.len() {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::MemDisk;
    use crate::fs::FormatOptions;

    fn fresh() -> FileSystem<MemDisk> {
        FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap()
    }

    /// Build a 2500-byte file by hand: block 0 full of 'a', block 1 a hole,
    /// and the first 452 bytes of block 2 full of 'c'.
    fn handmade(fs: &mut FileSystem<MemDisk>) -> u32 {
        fs.atomic(|fs| {
            let number = fs.alloc_inode(InodeKind::File)?;
            let mut inode = fs.read_inode(number)?;
            for (index, byte) in [(0, b'a'), (2, b'c')] {
                let block = fs.bmap(&mut inode, index, true)?.unwrap();
                fs.disk.write_block(block, &[byte; BLOCK_SIZE])?;
            }
            inode.size = 2500;
            fs.write_inode(number, &inode)?;
            Ok(number)
        })
        .unwrap()
    }

    #[test]
    fn reads_cross_block_boundaries_and_holes() {
        let mut fs = fresh();
        let file = handmade(&mut fs);
        let mut buf = [0xEE; 2500];
        assert_eq!(fs.read_at(file, 0, &mut buf), Ok(2500));
        assert!(buf[..1024].iter().all(|&byte| byte == b'a'));
        assert!(buf[1024..2048].iter().all(|&byte| byte == 0));
        assert!(buf[2048..].iter().all(|&byte| byte == b'c'));

        // A read that starts inside one block and ends inside the next.
        let mut middle = [0xEE; 4];
        assert_eq!(fs.read_at(file, 1022, &mut middle), Ok(4));
        assert_eq!(middle, [b'a', b'a', 0, 0]);
    }

    #[test]
    fn reads_stop_at_the_end_of_the_file() {
        let mut fs = fresh();
        let file = handmade(&mut fs);
        let mut buf = [0xEE; 100];
        // Only 10 bytes are left at offset 2490, though block 2 holds more.
        assert_eq!(fs.read_at(file, 2490, &mut buf), Ok(10));
        assert_eq!(buf[..10], [b'c'; 10]);
        assert_eq!(buf[10], 0xEE);
        assert_eq!(fs.read_at(file, 2500, &mut buf), Ok(0));
        assert_eq!(fs.read_at(file, 9999, &mut buf), Ok(0));
    }

    fn empty_file(fs: &mut FileSystem<MemDisk>) -> u32 {
        fs.atomic(|fs| fs.alloc_inode(InodeKind::File)).unwrap()
    }

    fn read_all(fs: &mut FileSystem<MemDisk>, file: u32) -> Vec<u8> {
        let mut buf = vec![0; fs.read_inode(file).unwrap().size as usize];
        let read = fs.read_at(file, 0, &mut buf).unwrap();
        assert_eq!(read, buf.len());
        buf
    }

    fn free_blocks(fs: &mut FileSystem<MemDisk>) -> u32 {
        fs.sb.total_blocks - fs.sb.block_bitmap().count_set(&mut fs.disk).unwrap()
    }

    /// Bytes that differ from block to block, so misplaced blocks show up.
    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|index| (index % 251) as u8).collect()
    }

    #[test]
    fn what_is_written_can_be_read_back() {
        let mut fs = fresh();
        let file = empty_file(&mut fs);
        fs.write_at(file, 0, b"hello, disk").unwrap();
        assert_eq!(read_all(&mut fs, file), b"hello, disk");

        // Overwriting the middle leaves both ends alone.
        fs.write_at(file, 7, b"DISK").unwrap();
        assert_eq!(read_all(&mut fs, file), b"hello, DISK");
        // Writing at the end makes the file longer.
        fs.write_at(file, 11, b"!").unwrap();
        assert_eq!(read_all(&mut fs, file), b"hello, DISK!");
    }

    #[test]
    fn a_write_can_straddle_blocks() {
        let mut fs = fresh();
        let file = empty_file(&mut fs);
        let data = pattern(3000);
        fs.write_at(file, 1000, &data).unwrap();
        let contents = read_all(&mut fs, file);
        assert_eq!(contents.len(), 4000);
        assert!(contents[..1000].iter().all(|&byte| byte == 0));
        assert_eq!(contents[1000..], data[..]);
    }

    #[test]
    fn writing_far_past_the_end_leaves_a_hole_that_costs_nothing() {
        let mut fs = fresh();
        let file = empty_file(&mut fs);
        let before = free_blocks(&mut fs);
        fs.write_at(file, 200_000, b"end").unwrap();
        assert_eq!(fs.read_inode(file).unwrap().size, 200_003);
        // One data block and one indirect block, not the 195 in between.
        assert_eq!(free_blocks(&mut fs), before - 2);
        let contents = read_all(&mut fs, file);
        assert!(contents[..200_000].iter().all(|&byte| byte == 0));
        assert_eq!(&contents[200_000..], b"end");
    }

    #[test]
    fn a_large_write_spans_many_transactions_and_pointer_blocks() {
        let mut fs = fresh();
        let file = empty_file(&mut fs);
        // 300 blocks: all 12 direct, all 256 indirect, 32 double indirect.
        let data = pattern(300 * BLOCK_SIZE);
        fs.write_at(file, 0, &data).unwrap();
        assert_eq!(read_all(&mut fs, file), data);
        let inode = fs.read_inode(file).unwrap();
        assert!(inode.indirect != 0 && inode.double_indirect != 0);

        // It is still all there after a remount.
        let mut fs = FileSystem::mount(fs.into_device()).unwrap();
        assert_eq!(read_all(&mut fs, file), data);
    }

    #[test]
    fn a_write_that_would_exceed_the_maximum_size_is_refused_whole() {
        let mut fs = fresh();
        let file = empty_file(&mut fs);
        let refused = fs.write_at(file, MAX_FILE_SIZE - 2, b"abc");
        assert_eq!(refused, Err(FsError::FileTooLarge));
        assert_eq!(fs.read_inode(file).unwrap().size, 0);
        assert_eq!(
            fs.write_at(file, usize::MAX, b"abc"),
            Err(FsError::FileTooLarge)
        );
    }

    #[test]
    fn a_full_disk_stops_a_write_between_pieces() {
        let mut fs = fresh();
        let file = empty_file(&mut fs);
        // 469 data blocks exist, so 480 blocks of content cannot fit.
        let data = pattern(480 * BLOCK_SIZE);
        assert_eq!(fs.write_at(file, 0, &data), Err(FsError::NoSpace));
        // The pieces that did fit are complete and correct.
        let kept = read_all(&mut fs, file);
        assert!(!kept.is_empty() && kept.len() < data.len());
        assert_eq!(kept.len() % BLOCK_SIZE, 0);
        assert_eq!(kept[..], data[..kept.len()]);
    }

    #[test]
    fn only_files_can_be_written_this_way() {
        let mut fs = fresh();
        let root = crate::layout::ROOT_INODE;
        assert!(matches!(
            fs.write_at(root, 0, b"x"),
            Err(FsError::IsADirectory(_))
        ));
        assert!(matches!(
            fs.write_at(40, 0, b"x"),
            Err(FsError::NotFound(_))
        ));
    }
}
