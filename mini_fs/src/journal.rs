use std::collections::BTreeMap;

use crate::device::{Block, BlockDevice};
use crate::error::{FsError, FsResult};

/// A disk on which a group of writes either all happen or none do.
///
/// One file system operation changes several blocks: creating a file touches
/// a bitmap, an inode and a directory. If the power fails between those
/// writes the disk is left contradicting itself. The journal prevents that by
/// collecting the writes of one operation, a *transaction*, and applying
/// them as a unit.
pub struct Journal<D> {
    device: D,
    /// First block of the log region. It holds the header; slots follow it.
    start: u32,
    /// How many different blocks one transaction may change.
    slots: u32,
    /// Writes of the transaction in progress, keyed by the block they are for.
    pending: BTreeMap<u32, Block>,
}

impl<D: BlockDevice> Journal<D> {
    /// Wrap `device`, using the `blocks` blocks starting at `start` as the log.
    pub fn new(device: D, start: u32, blocks: u32) -> Self {
        Self {
            device,
            start,
            slots: blocks.saturating_sub(1),
            pending: BTreeMap::new(),
        }
    }

    /// The most blocks a single transaction can change.
    pub fn capacity(&self) -> usize {
        self.slots as usize
    }

    /// How many blocks the current transaction has changed so far.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Forget the current transaction. Nothing it wrote has touched the disk.
    pub fn abort(&mut self) {
        self.pending.clear();
    }

    pub fn device(&self) -> &D {
        &self.device
    }

    pub fn into_inner(self) -> D {
        self.device
    }

    fn is_log_block(&self, index: u32) -> bool {
        index >= self.start && index <= self.start + self.slots
    }
}

impl<D: BlockDevice> BlockDevice for Journal<D> {
    fn block_count(&self) -> u32 {
        self.device.block_count()
    }

    /// A transaction sees its own writes, although the disk does not yet.
    fn read_block(&mut self, index: u32) -> FsResult<Block> {
        match self.pending.get(&index) {
            Some(block) => Ok(*block),
            None => self.device.read_block(index),
        }
    }

    /// Writes are only remembered here. They reach the disk at commit.
    fn write_block(&mut self, index: u32, block: &Block) -> FsResult<()> {
        if index >= self.device.block_count() || self.is_log_block(index) {
            return Err(FsError::OutOfRange(index));
        }
        if !self.pending.contains_key(&index) && self.pending.len() == self.capacity() {
            return Err(FsError::TransactionTooLarge);
        }
        self.pending.insert(index, *block);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{BLOCK_SIZE, MemDisk};

    /// A 32-block disk whose blocks 1 to 8 are the log: one header, 7 slots.
    fn journal() -> Journal<MemDisk> {
        Journal::new(MemDisk::new(32), 1, 8)
    }

    fn filled(byte: u8) -> Block {
        [byte; BLOCK_SIZE]
    }

    #[test]
    fn a_transaction_reads_its_own_writes_before_the_disk_has_them() {
        let mut journal = journal();
        journal.write_block(20, &filled(7)).unwrap();
        assert_eq!(journal.read_block(20), Ok(filled(7)));
        assert_eq!(journal.pending(), 1);
        // The disk underneath is still untouched.
        assert_eq!(journal.device(), &MemDisk::new(32));
    }

    #[test]
    fn abort_throws_the_transaction_away() {
        let mut journal = journal();
        journal.write_block(20, &filled(7)).unwrap();
        journal.abort();
        assert_eq!(journal.pending(), 0);
        assert_eq!(journal.read_block(20), Ok(filled(0)));
    }

    #[test]
    fn a_transaction_cannot_outgrow_the_log() {
        let mut journal = journal();
        assert_eq!(journal.capacity(), 7);
        for block in 10..17 {
            journal.write_block(block, &filled(1)).unwrap();
        }
        // Rewriting a block already in the transaction costs nothing extra.
        assert_eq!(journal.write_block(10, &filled(2)), Ok(()));
        assert_eq!(
            journal.write_block(17, &filled(1)),
            Err(FsError::TransactionTooLarge)
        );
    }

    #[test]
    fn the_log_itself_and_blocks_off_the_disk_cannot_be_written() {
        let mut journal = journal();
        assert_eq!(
            journal.write_block(1, &filled(1)),
            Err(FsError::OutOfRange(1))
        );
        assert_eq!(
            journal.write_block(8, &filled(1)),
            Err(FsError::OutOfRange(8))
        );
        assert_eq!(
            journal.write_block(32, &filled(1)),
            Err(FsError::OutOfRange(32))
        );
        assert_eq!(journal.write_block(9, &filled(1)), Ok(()));
        assert_eq!(journal.block_count(), 32);
    }
}
