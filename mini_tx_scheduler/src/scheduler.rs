use std::collections::HashMap;

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
    /// The compute units a block may add up to. This is what makes block
    /// space scarce, and so what makes fees mean anything.
    pub max_units: u64,
    /// The most transactions a block may hold.
    pub max_transactions: usize,
}

impl Default for BlockLimits {
    fn default() -> Self {
        Self {
            threads: 4,
            max_units: 10_000,
            max_transactions: 1_000,
        }
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
    // What each sender has already committed to spending in this block.
    let mut reserved: HashMap<&str, u64> = HashMap::new();
    while let Some(candidate) = heap.pop() {
        let Some(pending) = pool.get(&candidate.sender, candidate.nonce) else {
            continue;
        };
        let tx = &pending.tx;
        if schedule.picks.len() >= limits.max_transactions {
            break;
        }
        // A transaction too big for the room that is left is passed over.
        // So is the rest of its sender's queue, which could only run after
        // it. Smaller transactions further down may still fit, so the
        // search goes on.
        if tx.units > limits.max_units.saturating_sub(schedule.units) {
            continue;
        }
        // Every transaction in a block must be able to pay its fee when its
        // turn comes. The ledger shows what the sender has now, not what
        // will be left after their earlier transactions in this same block,
        // so that is kept track of here. Money the sender may receive during
        // the block is not counted: only they can spend from their account,
        // so this estimate can be too cautious but never too generous.
        let spent = reserved.entry(tx.sender.as_str()).or_insert(0);
        if ledger.balance(&tx.sender).saturating_sub(*spent) < tx.fee {
            continue;
        }
        *spent = spent.saturating_add(tx.spend().unwrap_or(u64::MAX));
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

    fn limited(pool: &Mempool, max_units: u64, max_transactions: usize) -> Schedule {
        let limits = BlockLimits {
            max_units,
            max_transactions,
            ..BlockLimits::default()
        };
        build_schedule(pool, &ledger(), Policy::FeePerUnit, &limits)
    }

    #[test]
    fn a_block_stops_filling_when_its_compute_budget_is_spent() {
        let pool = pool_of(vec![
            tx(1, "alice", 0, "zed", 400).with_units(400),
            tx(2, "bob", 0, "yan", 270).with_units(300),
            tx(3, "carol", 0, "xia", 240).with_units(300),
        ]);
        // Rates are 1.0, 0.9 and 0.8. The first two use exactly 700 units.
        let chosen = limited(&pool, 700, 100);
        assert_eq!((chosen.ids(), chosen.units), (vec![1, 2], 700));
    }

    #[test]
    fn a_transaction_too_big_for_the_space_left_does_not_block_smaller_ones() {
        let pool = pool_of(vec![
            tx(1, "alice", 0, "zed", 600).with_units(600),
            // Second best rate, but it no longer fits after the first.
            tx(2, "bob", 0, "yan", 540).with_units(600),
            tx(3, "carol", 0, "xia", 50).with_units(100),
        ]);
        let chosen = limited(&pool, 700, 100);
        assert_eq!((chosen.ids(), chosen.units), (vec![1, 3], 700));
    }

    #[test]
    fn when_a_transaction_does_not_fit_neither_does_what_is_queued_behind_it() {
        let pool = pool_of(vec![
            tx(1, "alice", 0, "zed", 900).with_units(900),
            // Small enough to fit, but it must come after alice's first.
            tx(2, "alice", 1, "zed", 50).with_units(10),
            tx(3, "bob", 0, "yan", 5).with_units(100),
        ]);
        let chosen = limited(&pool, 500, 100);
        assert_eq!(chosen.ids(), [3]);
    }

    #[test]
    fn a_block_holds_no_more_than_its_transaction_limit() {
        let pool = pool_of(vec![
            tx(1, "alice", 0, "zed", 10),
            tx(2, "bob", 0, "yan", 30),
            tx(3, "carol", 0, "xia", 20),
        ]);
        assert_eq!(limited(&pool, 10_000, 2).ids(), [2, 3]);
        assert!(limited(&pool, 10_000, 0).is_empty());
        assert!(limited(&pool, 0, 100).is_empty());
    }

    #[test]
    fn a_sender_is_not_scheduled_beyond_what_they_can_pay_for() {
        let ledger = Ledger::with_balances([("alice", 100), ("bob", 10_000)]);
        let mut pool = Mempool::new(PoolConfig::default());
        // Alice's first uses 70 of her 100: a payment of 60 and a fee of 10.
        pool.add(
            Transaction::new(1, "alice", 0).pay("zed", 60).with_fee(10),
            &ledger,
            0,
        )
        .unwrap();
        // Her second offers a fee of 50, which the 30 left cannot cover.
        pool.add(
            Transaction::new(2, "alice", 1).pay("zed", 1).with_fee(50),
            &ledger,
            0,
        )
        .unwrap();
        pool.add(tx(3, "bob", 0, "yan", 5), &ledger, 0).unwrap();

        let chosen = build_schedule(&pool, &ledger, Policy::FeePerUnit, &BlockLimits::default());
        assert_eq!(chosen.ids(), [1, 3]);
        // The second stays in the pool for a later block.
        assert!(pool.contains(2));
    }

    #[test]
    fn a_sender_who_has_since_run_out_of_money_is_skipped() {
        let mut pool = Mempool::new(PoolConfig::default());
        pool.add(tx(1, "alice", 0, "zed", 50), &ledger(), 0)
            .unwrap();
        pool.add(tx(2, "bob", 0, "yan", 5), &ledger(), 0).unwrap();
        // By the time the block is built, alice's balance has dropped.
        let later = Ledger::with_balances([("alice", 20), ("bob", 10_000)]);
        let chosen = build_schedule(&pool, &later, Policy::FeePerUnit, &BlockLimits::default());
        assert_eq!(chosen.ids(), [2]);
    }

    #[test]
    fn a_sender_can_have_several_transactions_in_a_block_if_they_can_afford_them() {
        let pool = pool_of(vec![
            tx(1, "alice", 0, "zed", 30),
            tx(2, "alice", 1, "zed", 20),
            tx(3, "alice", 2, "zed", 10),
        ]);
        let chosen = schedule(&pool, Policy::FeePerUnit);
        assert_eq!(chosen.ids(), [1, 2, 3]);
        // They all change alice's account, so they run one after another.
        let batches: Vec<usize> = chosen.picks.iter().map(|pick| pick.batch).collect();
        assert_eq!(batches, [0, 1, 2]);
    }
}
