use crate::device::{BLOCK_SIZE, BlockDevice};
use crate::error::FsResult;
use crate::fs::FileSystem;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::MemDisk;
    use crate::fs::FormatOptions;
    use crate::inode::InodeKind;

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
}
