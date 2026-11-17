use crate::device::BlockDevice;
use crate::error::{FsError, FsResult};
use crate::layout::{BITS_PER_BLOCK, Superblock};

/// A run of blocks used as a bitmap: one bit per item, 1 meaning "in use".
///
/// This is how the file system knows which blocks and which inodes are free.
/// The struct only says where the bitmap is; the bits themselves stay on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bitmap {
    start: u32,
    bits: u32,
}

impl Bitmap {
    /// A bitmap of `bits` bits whose first block is `start`.
    pub fn new(start: u32, bits: u32) -> Self {
        Self { start, bits }
    }

    pub fn bits(&self) -> u32 {
        self.bits
    }

    /// Where a bit lives: its block, the byte inside that block, and the mask
    /// selecting it within the byte.
    fn locate(&self, bit: u32) -> FsResult<(u32, usize, u8)> {
        if bit >= self.bits {
            return Err(FsError::Corrupt(format!(
                "bit {bit} is beyond a bitmap of {} bits",
                self.bits
            )));
        }
        let block = self.start + bit / BITS_PER_BLOCK as u32;
        let within = bit as usize % BITS_PER_BLOCK;
        Ok((block, within / 8, 1 << (within % 8)))
    }

    pub fn get<D: BlockDevice>(&self, disk: &mut D, bit: u32) -> FsResult<bool> {
        let (block, byte, mask) = self.locate(bit)?;
        Ok(disk.read_block(block)?[byte] & mask != 0)
    }

    pub fn set<D: BlockDevice>(&self, disk: &mut D, bit: u32, value: bool) -> FsResult<()> {
        let (block, byte, mask) = self.locate(bit)?;
        let mut contents = disk.read_block(block)?;
        if value {
            contents[byte] |= mask;
        } else {
            contents[byte] &= !mask;
        }
        disk.write_block(block, &contents)
    }

    /// Claim the lowest free item, or return `None` when every one is taken.
    pub fn allocate<D: BlockDevice>(&self, disk: &mut D) -> FsResult<Option<u32>> {
        let blocks = self.bits.div_ceil(BITS_PER_BLOCK as u32);
        for index in 0..blocks {
            let mut contents = disk.read_block(self.start + index)?;
            // A byte of 0xFF is full, so whole bytes can be skipped at once.
            let Some(byte) = contents.iter().position(|&byte| byte != 0xFF) else {
                continue;
            };
            let shift = contents[byte].trailing_ones();
            let bit = index * BITS_PER_BLOCK as u32 + byte as u32 * 8 + shift;
            if bit >= self.bits {
                // Only the unused tail of the last block is left.
                return Ok(None);
            }
            contents[byte] |= 1 << shift;
            disk.write_block(self.start + index, &contents)?;
            return Ok(Some(bit));
        }
        Ok(None)
    }

    /// How many items are in use.
    pub fn count_set<D: BlockDevice>(&self, disk: &mut D) -> FsResult<u32> {
        let blocks = self.bits.div_ceil(BITS_PER_BLOCK as u32);
        let mut total = 0;
        for index in 0..blocks {
            let contents = disk.read_block(self.start + index)?;
            total += contents.iter().map(|byte| byte.count_ones()).sum::<u32>();
        }
        Ok(total)
    }
}

impl Superblock {
    /// One bit per inode number.
    pub fn inode_bitmap(&self) -> Bitmap {
        Bitmap::new(self.inode_bitmap_start, self.inode_count)
    }

    /// One bit per block of the disk, bookkeeping blocks included.
    pub fn block_bitmap(&self) -> Bitmap {
        Bitmap::new(self.block_bitmap_start, self.total_blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::MemDisk;

    #[test]
    fn bits_can_be_set_cleared_and_read() {
        let mut disk = MemDisk::new(4);
        let bitmap = Bitmap::new(1, 100);
        assert_eq!(bitmap.get(&mut disk, 9), Ok(false));
        bitmap.set(&mut disk, 9, true).unwrap();
        assert_eq!(bitmap.get(&mut disk, 9), Ok(true));
        assert_eq!(bitmap.get(&mut disk, 8), Ok(false));
        // Bit 9 is bit 1 of byte 1 of the bitmap's first block.
        assert_eq!(disk.read_block(1).unwrap()[1], 0b0000_0010);
        bitmap.set(&mut disk, 9, false).unwrap();
        assert_eq!(bitmap.count_set(&mut disk), Ok(0));
        assert!(matches!(
            bitmap.get(&mut disk, 100),
            Err(FsError::Corrupt(_))
        ));
    }

    #[test]
    fn allocation_hands_out_the_lowest_free_bit() {
        let mut disk = MemDisk::new(4);
        let bitmap = Bitmap::new(1, 20);
        assert_eq!(bitmap.allocate(&mut disk), Ok(Some(0)));
        assert_eq!(bitmap.allocate(&mut disk), Ok(Some(1)));
        bitmap.set(&mut disk, 0, false).unwrap();
        assert_eq!(bitmap.allocate(&mut disk), Ok(Some(0)));
        assert_eq!(bitmap.count_set(&mut disk), Ok(2));
    }

    #[test]
    fn allocation_stops_at_the_last_real_bit() {
        let mut disk = MemDisk::new(4);
        let bitmap = Bitmap::new(1, 10);
        for expected in 0..10 {
            assert_eq!(bitmap.allocate(&mut disk), Ok(Some(expected)));
        }
        // The block has thousands more bits, but they stand for nothing.
        assert_eq!(bitmap.allocate(&mut disk), Ok(None));
        assert_eq!(bitmap.count_set(&mut disk), Ok(10));
    }

    #[test]
    fn a_bitmap_can_span_several_blocks() {
        let mut disk = MemDisk::new(4);
        let bitmap = Bitmap::new(1, 2 * BITS_PER_BLOCK as u32);
        let far = BITS_PER_BLOCK as u32 + 3;
        bitmap.set(&mut disk, far, true).unwrap();
        assert_eq!(disk.read_block(2).unwrap()[0], 0b0000_1000);
        assert_eq!(bitmap.get(&mut disk, far), Ok(true));

        // Fill the whole first block; the search moves on to the second.
        disk.write_block(1, &[0xFF; 1024]).unwrap();
        assert_eq!(bitmap.allocate(&mut disk), Ok(Some(BITS_PER_BLOCK as u32)));
    }
}
