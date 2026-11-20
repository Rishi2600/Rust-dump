use crate::conflict::BatchPlanner;
use crate::heap::MaxHeap;
use crate::ledger::Ledger;
use crate::mempool::{Mempool, Pending};
use crate::priority::{Policy, Priority};
use crate::tx::Transaction;

/// What a block is allowed to contain.
#[derive(Debug, Clone)]
pub struct BlockLimits {
    /// How many transactions can run at the same time.
    pub threads: usize,
}

impl Default for BlockLimits {
    fn default() -> Self {
        Self { threads: 4 }
    }
}

/// One transaction chosen for a block, and the batch it runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub tx: Transaction,
    pub batch: usize,
}

/// The plan for a block: which transactions, in what order, in which batches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schedule {
    /// The chosen transactions, in the order they were chosen: most
    /// important first.
    pub picks: Vec<Pick>,
    /// The compute units they add up to.
    pub units: u64,
}

impl Schedule {
    pub fn len(&self) -> usize {
        self.picks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.picks.is_empty()
    }

    /// The ids of the chosen transactions, most important first.
    pub fn ids(&self) -> Vec<u64> {
        self.picks.iter().map(|pick| pick.tx.id).collect()
    }

    /// The fees offered by everything chosen.
    pub fn fees(&self) -> u64 {
        self.picks.iter().map(|pick| pick.tx.fee).sum()
    }

    /// The transactions grouped by batch. Batches run one after another;
    /// the transactions inside one can run together.
    pub fn batches(&self) -> Vec<Vec<Transaction>> {
        let count = self
            .picks
            .iter()
            .map(|pick| pick.batch + 1)
            .max()
            .unwrap_or(0);
        let mut batches = vec![Vec::new(); count];
        for pick in &self.picks {
            batches[pick.batch].push(pick.tx.clone());
        }
        batches
    }
}

/// A transaction waiting its turn in the heap. Ordered by priority alone;
/// the sender and nonce say where to find it in the pool.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Candidate {
    priority: Priority,
    sender: String,
    nonce: u64,
}

impl Candidate {
    fn of(pending: &Pending, policy: Policy) -> Self {
        Self {
            priority: policy.key(&pending.tx, pending.arrival),
            sender: pending.tx.sender.clone(),
            nonce: pending.tx.nonce,
        }
    }
}

/// Choose transactions from the pool for the next block.
///
/// The heap holds one candidate per sender: the transaction at the front of
/// that sender's queue. The best candidate is taken, and the transaction
/// queued behind it steps up as that sender's new candidate. So the block
/// fills in priority order, while each sender's transactions still appear
/// in nonce order.
pub fn build_schedule(
    pool: &Mempool,
    ledger: &Ledger,
    policy: Policy,
    limits: &BlockLimits,
) -> Schedule {
    let mut heap = MaxHeap::new();
    for pending in pool.ready(ledger) {
        heap.push(Candidate::of(pending, policy));
    }
    let mut planner = BatchPlanner::new().with_width(limits.threads);
    let mut schedule = Schedule::default();
    while let Some(candidate) = heap.pop() {
        let Some(pending) = pool.get(&candidate.sender, candidate.nonce) else {
            continue;
        };
        let tx = &pending.tx;
        let batch = planner.place(tx);
        schedule.units += tx.units;
        schedule.picks.push(Pick {
            tx: tx.clone(),
            batch,
        });
        if let Some(next) = pool.next_after(&tx.sender, tx.nonce) {
            heap.push(Candidate::of(next, policy));
        }
    }
    schedule
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mempool::PoolConfig;

    const PEOPLE: [&str; 6] = ["alice", "bob", "carol", "dave", "erin", "frank"];

    fn ledger() -> Ledger {
        Ledger::with_balances(PEOPLE.map(|name| (name, 10_000)))
    }

    fn tx(id: u64, sender: &str, nonce: u64, to: &str, fee: u64) -> Transaction {
        Transaction::new(id, sender, nonce).pay(to, 1).with_fee(fee)
    }

    fn pool_of(txs: Vec<Transaction>) -> Mempool {
        let mut pool = Mempool::new(PoolConfig::default());
        for tx in txs {
            pool.add(tx, &ledger(), 0).unwrap();
        }
        pool
    }

    fn schedule(pool: &Mempool, policy: Policy) -> Schedule {
        build_schedule(pool, &ledger(), policy, &BlockLimits::default())
    }

    #[test]
    fn the_best_paying_transactions_are_chosen_first() {
        // All 100 units, so the fee decides the rate.
        let pool = pool_of(vec![
            tx(1, "alice", 0, "zed", 20),
            tx(2, "bob", 0, "yan", 90),
            tx(3, "carol", 0, "xia", 50),
        ]);
        let chosen = schedule(&pool, Policy::FeePerUnit);
        assert_eq!(chosen.ids(), [2, 3, 1]);
        assert_eq!((chosen.fees(), chosen.units), (160, 300));
    }

    #[test]
    fn a_small_generous_transaction_beats_a_large_stingy_one() {
        let small = tx(1, "alice", 0, "zed", 40).with_units(20);
        let large = tx(2, "bob", 0, "yan", 90).with_units(900);
        let pool = pool_of(vec![large, small]);
        assert_eq!(schedule(&pool, Policy::FeePerUnit).ids(), [1, 2]);
        // Ranked by total fee, the order is the other way round.
        assert_eq!(schedule(&pool, Policy::TotalFee).ids(), [2, 1]);
    }

    #[test]
    fn a_senders_transactions_stay_in_nonce_order_whatever_they_pay() {
        let pool = pool_of(vec![
            tx(1, "alice", 0, "zed", 5),
            // Pays the most of all, but cannot run before alice's first.
            tx(2, "alice", 1, "zed", 500),
            tx(3, "bob", 0, "yan", 50),
        ]);
        assert_eq!(schedule(&pool, Policy::FeePerUnit).ids(), [3, 1, 2]);
    }

    #[test]
    fn fifo_ignores_fees_entirely() {
        let pool = pool_of(vec![
            tx(1, "alice", 0, "zed", 1),
            tx(2, "bob", 0, "yan", 900),
            tx(3, "carol", 0, "xia", 50),
        ]);
        assert_eq!(schedule(&pool, Policy::Fifo).ids(), [1, 2, 3]);
    }

    #[test]
    fn clashing_transactions_are_split_across_batches_in_priority_order() {
        let pool = pool_of(vec![
            // Both pay into the same account, so they clash.
            tx(1, "alice", 0, "shop", 10),
            tx(2, "bob", 0, "shop", 80),
            // Unrelated to both.
            tx(3, "carol", 0, "dave", 40),
        ]);
        let chosen = schedule(&pool, Policy::FeePerUnit);
        assert_eq!(chosen.ids(), [2, 3, 1]);
        let batch_of = |id: u64| {
            chosen
                .picks
                .iter()
                .find(|pick| pick.tx.id == id)
                .unwrap()
                .batch
        };
        // The better-paying of the clashing pair runs first.
        assert_eq!((batch_of(2), batch_of(3), batch_of(1)), (0, 0, 1));
        let batches: Vec<Vec<u64>> = chosen
            .batches()
            .iter()
            .map(|batch| batch.iter().map(|tx| tx.id).collect())
            .collect();
        assert_eq!(batches, [vec![2, 3], vec![1]]);
    }

    #[test]
    fn an_empty_pool_gives_an_empty_schedule() {
        let chosen = schedule(&pool_of(Vec::new()), Policy::FeePerUnit);
        assert!(chosen.is_empty());
        assert_eq!(
            (chosen.len(), chosen.units, chosen.batches().len()),
            (0, 0, 0)
        );
    }

    #[test]
    fn transactions_waiting_on_a_missing_nonce_are_left_in_the_pool() {
        // Alice's nonce 0 never arrived.
        let pool = pool_of(vec![
            tx(1, "alice", 1, "zed", 999),
            tx(2, "bob", 0, "yan", 1),
        ]);
        assert_eq!(schedule(&pool, Policy::FeePerUnit).ids(), [2]);
    }
}
