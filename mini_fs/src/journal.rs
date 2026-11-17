use std::collections::BTreeMap;

use crate::bytes::{get_u32, put_u32};
use crate::device::{BLOCK_SIZE, Block, BlockDevice};
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

impl<D: BlockDevice> Journal<D> {
    /// Apply the current transaction so that a crash at any moment leaves
    /// either all of it on the disk or none of it.
    pub fn commit(&mut self) -> FsResult<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        // 1. Copy every changed block into the log. The real locations are
        //    untouched, so a crash now loses the transaction and nothing else.
        for (slot, block) in self.pending.values().enumerate() {
            self.device
                .write_block(self.start + 1 + slot as u32, block)?;
        }
        self.device.flush()?;

        // 2. Write the header naming where each logged block belongs. This
        //    single block write is the commit point: before it the
        //    transaction did not happen, after it the transaction will
        //    happen even if the power fails.
        let mut header = [0; BLOCK_SIZE];
        put_u32(&mut header, 0, self.pending.len() as u32);
        for (slot, home) in self.pending.keys().enumerate() {
            put_u32(&mut header, 4 + slot * 4, *home);
        }
        self.device.write_block(self.start, &header)?;
        self.device.flush()?;

        // 3. Install: copy the blocks to where they really live.
        for (home, block) in &self.pending {
            self.device.write_block(*home, block)?;
        }
        self.device.flush()?;

        // 4. Erase the header, so the log is not replayed needlessly.
        self.device.write_block(self.start, &[0; BLOCK_SIZE])?;
        self.device.flush()?;
        self.pending.clear();
        Ok(())
    }
}

impl<D: BlockDevice> Journal<D> {
    /// Finish a transaction that was committed but not fully installed
    /// before a crash. Returns how many blocks were replayed.
    ///
    /// Replaying only copies blocks from the log to their homes, so doing it
    /// twice gives the same result. A crash during recovery is therefore
    /// harmless: recovery simply runs again.
    pub fn recover(&mut self) -> FsResult<usize> {
        let header = self.device.read_block(self.start)?;
        let count = get_u32(&header, 0) as usize;
        if count > self.capacity() {
            return Err(FsError::Corrupt(format!(
                "the journal claims {count} blocks but holds at most {}",
                self.capacity()
            )));
        }
        for slot in 0..count {
            let home = get_u32(&header, 4 + slot * 4);
            if home >= self.device.block_count() || self.is_log_block(home) {
                return Err(FsError::Corrupt(format!(
                    "the journal wants to replay onto block {home}"
                )));
            }
            let block = self.device.read_block(self.start + 1 + slot as u32)?;
            self.device.write_block(home, &block)?;
        }
        if count > 0 {
            self.device.flush()?;
            self.device.write_block(self.start, &[0; BLOCK_SIZE])?;
            self.device.flush()?;
        }
        Ok(count)
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
    use crate::device::{FaultyDisk, MemDisk};

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

    #[test]
    fn commit_puts_every_block_in_its_real_place() {
        let mut journal = journal();
        journal.write_block(20, &filled(1)).unwrap();
        journal.write_block(30, &filled(2)).unwrap();
        journal.commit().unwrap();
        assert_eq!(journal.pending(), 0);

        let mut disk = journal.into_inner();
        assert_eq!(disk.read_block(20), Ok(filled(1)));
        assert_eq!(disk.read_block(30), Ok(filled(2)));
        // The header was erased again once the blocks were installed.
        assert_eq!(disk.read_block(1), Ok(filled(0)));
        // The copies made on the way are still in the log slots.
        assert_eq!(disk.read_block(2), Ok(filled(1)));
        assert_eq!(disk.read_block(3), Ok(filled(2)));
    }

    #[test]
    fn committing_nothing_writes_nothing() {
        let mut journal = journal();
        journal.commit().unwrap();
        assert_eq!(journal.device(), &MemDisk::new(32));
    }

    /// A disk as a crash would leave it just after the commit point: two
    /// blocks logged and named in the header, neither installed yet.
    fn committed_but_not_installed() -> MemDisk {
        let mut disk = MemDisk::new(32);
        disk.write_block(2, &filled(1)).unwrap();
        disk.write_block(3, &filled(2)).unwrap();
        let mut header = filled(0);
        put_u32(&mut header, 0, 2);
        put_u32(&mut header, 4, 20);
        put_u32(&mut header, 8, 30);
        disk.write_block(1, &header).unwrap();
        disk
    }

    #[test]
    fn recovery_replays_a_committed_transaction() {
        let mut journal = Journal::new(committed_but_not_installed(), 1, 8);
        assert_eq!(journal.recover(), Ok(2));
        assert_eq!(journal.read_block(20), Ok(filled(1)));
        assert_eq!(journal.read_block(30), Ok(filled(2)));
        // The header is gone, so a second recovery finds nothing to do.
        assert_eq!(journal.recover(), Ok(0));
    }

    #[test]
    fn a_clean_log_needs_no_recovery() {
        let mut journal = journal();
        assert_eq!(journal.recover(), Ok(0));
        assert_eq!(journal.device(), &MemDisk::new(32));
    }

    #[test]
    fn a_nonsensical_header_is_reported_not_obeyed() {
        let mut too_many = MemDisk::new(32);
        let mut header = filled(0);
        put_u32(&mut header, 0, 100);
        too_many.write_block(1, &header).unwrap();
        let recovered = Journal::new(too_many, 1, 8).recover();
        assert!(matches!(recovered, Err(FsError::Corrupt(_))));

        // A header that would replay a block on top of the log itself.
        let mut into_log = committed_but_not_installed();
        put_u32(&mut header, 0, 1);
        put_u32(&mut header, 4, 3);
        into_log.write_block(1, &header).unwrap();
        let recovered = Journal::new(into_log, 1, 8).recover();
        assert!(matches!(recovered, Err(FsError::Corrupt(_))));
    }

    #[test]
    fn a_crash_at_any_write_leaves_all_of_a_transaction_or_none_of_it() {
        // Three blocks hold 'a'. One transaction turns them all into 'b'.
        let homes = [20, 21, 22];
        let mut base = MemDisk::new(32);
        for home in homes {
            base.write_block(home, &filled(b'a')).unwrap();
        }

        let (mut kept_old, mut got_new) = (0, 0);
        for budget in 0.. {
            // Cut the power after `budget` writes.
            let mut journal = Journal::new(FaultyDisk::failing_after(base.clone(), budget), 1, 8);
            for home in homes {
                journal.write_block(home, &filled(b'b')).unwrap();
            }
            let finished = journal.commit().is_ok();

            // The power comes back, and recovery runs on whatever survived.
            let mut restarted = Journal::new(journal.into_inner().into_inner(), 1, 8);
            restarted.recover().unwrap();
            let seen = homes.map(|home| restarted.read_block(home).unwrap()[0]);
            match seen {
                [b'a', b'a', b'a'] => kept_old += 1,
                [b'b', b'b', b'b'] => got_new += 1,
                torn => panic!("after {budget} writes the transaction was torn: {torn:?}"),
            }
            if finished {
                break;
            }
        }
        // 3 log writes and the header come before the commit point takes
        // effect; 3 installs, the header erase, and a clean run come after.
        assert_eq!((kept_old, got_new), (4, 5));
    }

    #[test]
    fn a_crash_during_recovery_is_repaired_by_recovering_again() {
        for budget in 0..4 {
            let dying = FaultyDisk::failing_after(committed_but_not_installed(), budget);
            let mut interrupted = Journal::new(dying, 1, 8);
            let _ = interrupted.recover();

            let mut restarted = Journal::new(interrupted.into_inner().into_inner(), 1, 8);
            restarted.recover().unwrap();
            assert_eq!(restarted.read_block(20), Ok(filled(1)));
            assert_eq!(restarted.read_block(30), Ok(filled(2)));
        }
    }
}
