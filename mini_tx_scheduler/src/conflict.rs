use std::collections::HashMap;

use crate::tx::Transaction;

/// Whether two transactions must not run at the same time.
///
/// They clash if one changes an account the other reads or changes. Two
/// transactions that only *read* the same account do not clash: neither can
/// see any effect of the other, so their order does not matter.
///
/// | first  | second | clash? |
/// |--------|--------|--------|
/// | read   | read   | no     |
/// | read   | write  | yes    |
/// | write  | read   | yes    |
/// | write  | write  | yes    |
pub fn conflicts(first: &Transaction, second: &Transaction) -> bool {
    let (first_writes, second_writes) = (first.writes(), second.writes());
    let (first_reads, second_reads) = (first.read_only(), second.read_only());
    let both_write = first_writes.intersection(&second_writes).next().is_some();
    let first_writes_what_second_reads = first_writes.intersection(&second_reads).next().is_some();
    let second_writes_what_first_reads = second_writes.intersection(&first_reads).next().is_some();
    both_write || first_writes_what_second_reads || second_writes_what_first_reads
}

/// Sorts transactions into batches that can each run in parallel.
///
/// Transactions are handed over one at a time, most important first. Each is
/// put in the earliest batch that comes after every earlier transaction it
/// clashes with:
///
/// ```text
/// batch 0     batch 1     batch 2
/// A  a->b     C  b->e     E  e->a
/// B  c->d
/// D  f->g
/// ```
///
/// C clashes with A (both touch `b`), so it waits for the next batch. E
/// clashes with C and with A, so it waits for the one after that. B and D
/// touch nothing the others do and run alongside A.
///
/// Two properties follow. Inside a batch nothing clashes, so the order of
/// execution within it does not matter. And of any two transactions that do
/// clash, the one handed over first is in the earlier batch, so the order
/// that priority decided is the order in which they take effect.
#[derive(Debug, Clone, Default)]
pub struct BatchPlanner {
    /// The latest batch that writes each account.
    last_write: HashMap<String, usize>,
    /// The latest batch that reads each account.
    last_read: HashMap<String, usize>,
    /// How many transactions each batch holds so far.
    widths: Vec<usize>,
    /// The most transactions a batch may hold, if there is a limit.
    max_width: Option<usize>,
}

impl BatchPlanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allow at most `width` transactions in a batch.
    ///
    /// A batch runs on a fixed number of threads. Putting more transactions
    /// in it than there are threads gains nothing, so the extras are moved
    /// to a later batch.
    pub fn with_width(mut self, width: usize) -> Self {
        self.max_width = Some(width.max(1));
        self
    }

    /// How many batches have been started.
    pub fn batch_count(&self) -> usize {
        self.widths.len()
    }

    /// How many transactions are in `batch`.
    pub fn width(&self, batch: usize) -> usize {
        self.widths.get(batch).copied().unwrap_or(0)
    }

    /// Choose the batch for `tx` and record that it is there.
    pub fn place(&mut self, tx: &Transaction) -> usize {
        let (writes, reads) = (tx.writes(), tx.read_only());
        let mut batch = 0;
        for account in &writes {
            // A writer waits for everyone who touched the account before it.
            let earlier = [self.last_write.get(*account), self.last_read.get(*account)];
            for used in earlier.into_iter().flatten() {
                batch = batch.max(used + 1);
            }
        }
        for account in &reads {
            // A reader waits only for writers. Other readers are company.
            if let Some(written) = self.last_write.get(*account) {
                batch = batch.max(written + 1);
            }
        }
        // A full batch pushes the transaction later. Later is always safe:
        // it only has to come after the ones it clashes with.
        if let Some(limit) = self.max_width {
            while self.width(batch) >= limit {
                batch += 1;
            }
        }

        for account in writes {
            self.last_write.insert(account.to_string(), batch);
        }
        for account in reads {
            let latest = self.last_read.entry(account.to_string()).or_insert(batch);
            *latest = (*latest).max(batch);
        }
        if self.widths.len() <= batch {
            self.widths.resize(batch + 1, 0);
        }
        self.widths[batch] += 1;
        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payment(id: u64, from: &str, to: &str) -> Transaction {
        Transaction::new(id, from, 0).pay(to, 1).with_fee(1)
    }

    #[test]
    fn transactions_on_unrelated_accounts_do_not_clash() {
        let first = payment(1, "alice", "bob");
        let second = payment(2, "carol", "dave");
        assert!(!conflicts(&first, &second));
        assert!(!conflicts(&second, &first));
    }

    #[test]
    fn two_writers_of_one_account_clash() {
        // Both pay bob: each would change his balance.
        assert!(conflicts(
            &payment(1, "alice", "bob"),
            &payment(2, "carol", "bob")
        ));
        // One's payee is the other's sender.
        assert!(conflicts(
            &payment(1, "alice", "bob"),
            &payment(2, "bob", "carol")
        ));
        // The same sender twice.
        assert!(conflicts(
            &payment(1, "alice", "bob"),
            &payment(2, "alice", "carol")
        ));
    }

    #[test]
    fn a_reader_clashes_with_a_writer_in_either_order() {
        let writer = payment(1, "alice", "oracle");
        let reader = payment(2, "carol", "dave").reading("oracle");
        assert!(conflicts(&writer, &reader));
        assert!(conflicts(&reader, &writer));
    }

    #[test]
    fn two_readers_of_one_account_do_not_clash() {
        let first = payment(1, "alice", "bob").reading("oracle");
        let second = payment(2, "carol", "dave").reading("oracle");
        assert!(!conflicts(&first, &second));
    }

    #[test]
    fn a_transaction_clashes_with_itself() {
        let tx = payment(1, "alice", "bob");
        assert!(conflicts(&tx, &tx));
    }

    fn batches_for(txs: &[Transaction]) -> Vec<usize> {
        let mut planner = BatchPlanner::new();
        txs.iter().map(|tx| planner.place(tx)).collect()
    }

    #[test]
    fn unrelated_transactions_all_share_the_first_batch() {
        let txs = [
            payment(1, "alice", "bob"),
            payment(2, "carol", "dave"),
            payment(3, "erin", "frank"),
        ];
        assert_eq!(batches_for(&txs), [0, 0, 0]);
    }

    #[test]
    fn a_chain_of_clashes_becomes_a_chain_of_batches() {
        // Each one's payee is the next one's sender.
        let txs = [
            payment(1, "alice", "bob"),
            payment(2, "bob", "carol"),
            payment(3, "carol", "dave"),
        ];
        assert_eq!(batches_for(&txs), [0, 1, 2]);
    }

    #[test]
    fn the_example_from_the_documentation() {
        let txs = [
            payment(1, "a", "b"),
            payment(2, "c", "d"),
            payment(3, "b", "e"),
            payment(4, "f", "g"),
            payment(5, "e", "a"),
        ];
        let mut planner = BatchPlanner::new();
        let placed: Vec<usize> = txs.iter().map(|tx| planner.place(tx)).collect();
        assert_eq!(placed, [0, 0, 1, 0, 2]);
        assert_eq!(planner.batch_count(), 3);
        assert_eq!(
            (planner.width(0), planner.width(1), planner.width(2)),
            (3, 1, 1)
        );
        assert_eq!(planner.width(9), 0);
    }

    #[test]
    fn readers_share_a_batch_but_take_turns_with_writers() {
        let read_a = payment(1, "alice", "bob").reading("oracle");
        let read_b = payment(2, "carol", "dave").reading("oracle");
        let write = payment(3, "erin", "oracle");
        let read_c = payment(4, "frank", "gina").reading("oracle");
        // Two readers together, the writer after both, a third reader after
        // the writer so that it sees what was written.
        assert_eq!(batches_for(&[read_a, read_b, write, read_c]), [0, 0, 1, 2]);
    }

    #[test]
    fn no_batch_ever_holds_two_transactions_that_clash() {
        let txs = [
            payment(1, "a", "b"),
            payment(2, "b", "c").reading("a"),
            payment(3, "d", "e").reading("c"),
            payment(4, "e", "a"),
            payment(5, "f", "g").reading("a"),
            payment(6, "g", "f"),
        ];
        let placed = batches_for(&txs);
        for first in 0..txs.len() {
            for second in first + 1..txs.len() {
                if conflicts(&txs[first], &txs[second]) {
                    assert!(
                        placed[first] < placed[second],
                        "{first} and {second}: {placed:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_batch_holds_no_more_than_the_threads_that_run_it() {
        let txs = [
            payment(1, "a", "b"),
            payment(2, "c", "d"),
            payment(3, "e", "f"),
            payment(4, "g", "h"),
            payment(5, "i", "j"),
        ];
        let mut planner = BatchPlanner::new().with_width(2);
        let placed: Vec<usize> = txs.iter().map(|tx| planner.place(tx)).collect();
        assert_eq!(placed, [0, 0, 1, 1, 2]);
        assert_eq!(
            (planner.width(0), planner.width(1), planner.width(2)),
            (2, 2, 1)
        );
    }

    #[test]
    fn a_transaction_moved_by_the_width_limit_still_orders_what_follows() {
        let txs = [
            payment(1, "a", "b"),
            payment(2, "c", "d"),
            // Clashes with nothing, but batch 0 is full: goes to batch 1.
            payment(3, "e", "f"),
            // Clashes with the one just moved, so it must come after batch 1.
            payment(4, "f", "g"),
        ];
        let mut planner = BatchPlanner::new().with_width(2);
        let placed: Vec<usize> = txs.iter().map(|tx| planner.place(tx)).collect();
        assert_eq!(placed, [0, 0, 1, 2]);
    }

    #[test]
    fn a_width_of_zero_is_treated_as_one() {
        let mut planner = BatchPlanner::new().with_width(0);
        assert_eq!(planner.place(&payment(1, "a", "b")), 0);
        assert_eq!(planner.place(&payment(2, "c", "d")), 1);
    }
}
