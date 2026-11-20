use std::collections::{BTreeMap, HashSet};

use crate::error::{SchedError, SchedResult};
use crate::ledger::Ledger;
use crate::priority::Policy;
use crate::tx::Transaction;

/// The rules a pool applies to what it holds.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// The most transactions held at once.
    pub capacity: usize,
    /// The smallest fee worth holding.
    pub min_fee: u64,
    /// How far ahead of a sender's next nonce a transaction may be.
    pub max_nonce_gap: u64,
    /// How many slots a transaction may wait before it is dropped.
    pub ttl_slots: u64,
    /// How much more, in percent, a replacement must pay than what it replaces.
    pub replace_bump_percent: u64,
    /// The rule for ranking transactions against each other.
    pub policy: Policy,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            capacity: 1024,
            min_fee: 1,
            max_nonce_gap: 16,
            ttl_slots: 32,
            replace_bump_percent: 10,
            policy: Policy::FeePerUnit,
        }
    }
}

/// A transaction waiting in the pool, with what the pool knows about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub tx: Transaction,
    /// Its place in the order of arrival: the first accepted is 0.
    pub arrival: u64,
    /// The slot during which it was accepted.
    pub added_slot: u64,
}

/// Transactions that have been accepted and are waiting to be put in a block.
///
/// They are kept per sender, in nonce order. A sender's transactions can
/// only be applied in that order, so only the first of each queue is ever a
/// candidate to run next.
///
/// ```text
/// alice   nonce 4 -> nonce 5 -> nonce 6
/// bob     nonce 0
/// carol   nonce 9 -> nonce 10
/// ```
pub struct Mempool {
    config: PoolConfig,
    by_sender: BTreeMap<String, BTreeMap<u64, Pending>>,
    ids: HashSet<u64>,
    next_arrival: u64,
}

impl Mempool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            by_sender: BTreeMap::new(),
            ids: HashSet::new(),
            next_arrival: 0,
        }
    }

    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Whether a transaction with this id is waiting.
    pub fn contains(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }

    /// The transaction holding this sender's `nonce`, if any.
    pub fn get(&self, sender: &str, nonce: u64) -> Option<&Pending> {
        self.by_sender.get(sender)?.get(&nonce)
    }

    /// Store a transaction without checking anything about it. If the sender
    /// already has one waiting with the same nonce, this takes its place.
    /// Returns the arrival number it was given.
    pub fn insert(&mut self, tx: Transaction, slot: u64) -> u64 {
        self.remove(&tx.sender, tx.nonce);
        let arrival = self.next_arrival;
        self.next_arrival += 1;
        self.ids.insert(tx.id);
        let queue = self.by_sender.entry(tx.sender.clone()).or_default();
        queue.insert(
            tx.nonce,
            Pending {
                tx,
                arrival,
                added_slot: slot,
            },
        );
        arrival
    }

    /// Take a transaction out of the pool.
    pub fn remove(&mut self, sender: &str, nonce: u64) -> Option<Pending> {
        let queue = self.by_sender.get_mut(sender)?;
        let removed = queue.remove(&nonce)?;
        if queue.is_empty() {
            self.by_sender.remove(sender);
        }
        self.ids.remove(&removed.tx.id);
        Some(removed)
    }

    /// Everything waiting, sender by sender, each in nonce order.
    pub fn iter(&self) -> impl Iterator<Item = &Pending> {
        self.by_sender.values().flat_map(|queue| queue.values())
    }
}

/// What happened to the rest of the pool when a transaction was accepted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Admission {
    /// The id of a transaction this one took the place of.
    pub replaced: Option<u64>,
    /// The id of a transaction pushed out to make room.
    pub evicted: Option<u64>,
}

impl Mempool {
    /// Offer a transaction to the pool.
    ///
    /// The pool is the first line of defence. Anyone can send it anything,
    /// for free, so it must refuse whatever could never be worth including
    /// before spending memory on it.
    pub fn add(&mut self, tx: Transaction, ledger: &Ledger, slot: u64) -> SchedResult<Admission> {
        tx.validate()?;
        if self.ids.contains(&tx.id) {
            return Err(SchedError::Duplicate(tx.id));
        }
        self.check_admission(&tx, ledger)?;
        let replaced = self.replace_existing(&tx)?;
        self.insert(tx, slot);
        Ok(Admission {
            replaced,
            evicted: None,
        })
    }

    /// If the sender already has a transaction waiting with this nonce,
    /// decide whether the new one may take its place.
    ///
    /// Replacing is how a sender raises the fee on a transaction that is
    /// stuck, or cancels it by sending something else with the same nonce.
    /// It has to cost noticeably more each time. If it were free, one
    /// transaction could be swapped back and forth forever, making the pool
    /// do work for which nobody pays.
    fn replace_existing(&mut self, tx: &Transaction) -> SchedResult<Option<u64>> {
        let Some(existing) = self.get(&tx.sender, tx.nonce) else {
            return Ok(None);
        };
        let (old_id, old_fee) = (existing.tx.id, u128::from(existing.tx.fee));
        // The required increase is rounded up, so even a tiny fee must rise.
        let bump = (old_fee * u128::from(self.config.replace_bump_percent)).div_ceil(100);
        let needed = u64::try_from(old_fee + bump).unwrap_or(u64::MAX);
        if tx.fee < needed {
            return Err(SchedError::ReplacementUnderpriced {
                fee: tx.fee,
                needed,
            });
        }
        self.remove(&tx.sender, tx.nonce);
        Ok(Some(old_id))
    }

    /// The checks that depend on the ledger: is this transaction one that
    /// could run, now or soon?
    fn check_admission(&self, tx: &Transaction, ledger: &Ledger) -> SchedResult<()> {
        if tx.fee < self.config.min_fee {
            return Err(SchedError::FeeTooLow {
                fee: tx.fee,
                minimum: self.config.min_fee,
            });
        }
        let (sender, nonce) = (tx.sender.clone(), tx.nonce);
        let expected = ledger.nonce(&tx.sender);
        // A nonce already used can never run again.
        if nonce < expected {
            return Err(SchedError::StaleNonce {
                sender,
                nonce,
                expected,
            });
        }
        // A nonce slightly ahead is fine: it waits for the ones before it.
        // One far ahead may never become runnable, and would sit here
        // taking up room for nothing.
        if nonce - expected > self.config.max_nonce_gap {
            return Err(SchedError::NonceTooFar {
                sender,
                nonce,
                expected,
            });
        }
        // The fee is charged even when a transaction fails, so a sender who
        // cannot cover it has nothing to offer for the space.
        let balance = ledger.balance(&tx.sender);
        if balance < tx.fee {
            return Err(SchedError::CannotPayFee {
                sender,
                balance,
                fee: tx.fee,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(id: u64, sender: &str, nonce: u64) -> Transaction {
        Transaction::new(id, sender, nonce)
            .pay("zed", 1)
            .with_fee(10)
    }

    fn pool() -> Mempool {
        Mempool::new(PoolConfig::default())
    }

    /// (sender, nonce) of everything in the pool, in the pool's own order.
    fn contents(pool: &Mempool) -> Vec<(String, u64)> {
        pool.iter()
            .map(|pending| (pending.tx.sender.clone(), pending.tx.nonce))
            .collect()
    }

    #[test]
    fn transactions_are_queued_per_sender_in_nonce_order() {
        let mut pool = pool();
        // Arrival order is deliberately not nonce order.
        for (id, sender, nonce) in [
            (1, "bob", 0),
            (2, "alice", 2),
            (3, "alice", 0),
            (4, "alice", 1),
        ] {
            pool.insert(tx(id, sender, nonce), 0);
        }
        let expected = [("alice", 0), ("alice", 1), ("alice", 2), ("bob", 0)];
        let expected: Vec<(String, u64)> =
            expected.iter().map(|(s, n)| (s.to_string(), *n)).collect();
        assert_eq!(contents(&pool), expected);
        assert_eq!(pool.len(), 4);
    }

    #[test]
    fn each_transaction_remembers_when_it_arrived() {
        let mut pool = pool();
        assert_eq!(pool.insert(tx(1, "alice", 0), 5), 0);
        assert_eq!(pool.insert(tx(2, "bob", 0), 6), 1);
        let second = pool.get("bob", 0).unwrap();
        assert_eq!((second.arrival, second.added_slot, second.tx.id), (1, 6, 2));
        assert!(pool.get("bob", 1).is_none() && pool.get("nobody", 0).is_none());
    }

    #[test]
    fn removing_a_transaction_forgets_it_completely() {
        let mut pool = pool();
        pool.insert(tx(1, "alice", 0), 0);
        pool.insert(tx(2, "alice", 1), 0);
        assert!(pool.contains(1));
        let removed = pool.remove("alice", 0).unwrap();
        assert_eq!(removed.tx.id, 1);
        assert!(!pool.contains(1) && pool.contains(2));
        assert_eq!(pool.remove("alice", 0), None);
        pool.remove("alice", 1);
        assert!(pool.is_empty());
        assert_eq!(pool.config().capacity, 1024);
    }

    #[test]
    fn a_second_transaction_with_the_same_nonce_takes_the_first_ones_place() {
        let mut pool = pool();
        pool.insert(tx(1, "alice", 0), 0);
        pool.insert(tx(2, "alice", 0), 0);
        assert_eq!(pool.len(), 1);
        assert!(!pool.contains(1) && pool.contains(2));
    }

    fn funded() -> Ledger {
        Ledger::with_balances([("alice", 1000), ("bob", 1000), ("poor", 5)])
    }

    #[test]
    fn a_sound_transaction_is_accepted() {
        let mut pool = pool();
        assert_eq!(
            pool.add(tx(1, "alice", 0), &funded(), 0),
            Ok(Admission::default())
        );
        assert!(pool.contains(1));
    }

    #[test]
    fn a_transaction_slightly_ahead_of_its_turn_waits_in_the_queue() {
        let mut pool = pool();
        // Nonces 1 and 2 arrive before nonce 0.
        assert!(pool.add(tx(1, "alice", 2), &funded(), 0).is_ok());
        assert!(pool.add(tx(2, "alice", 1), &funded(), 0).is_ok());
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn each_reason_for_refusal_has_its_own_error() {
        let mut pool = Mempool::new(PoolConfig {
            min_fee: 5,
            max_nonce_gap: 3,
            ..PoolConfig::default()
        });
        let mut ledger = funded();
        ledger.bump_nonce("bob");
        ledger.bump_nonce("bob");

        let cheap = tx(1, "alice", 0).with_fee(4);
        assert_eq!(
            pool.add(cheap, &ledger, 0),
            Err(SchedError::FeeTooLow { fee: 4, minimum: 5 })
        );

        let stale = SchedError::StaleNonce {
            sender: "bob".into(),
            nonce: 1,
            expected: 2,
        };
        assert_eq!(pool.add(tx(2, "bob", 1), &ledger, 0), Err(stale));

        let far = SchedError::NonceTooFar {
            sender: "bob".into(),
            nonce: 6,
            expected: 2,
        };
        assert_eq!(pool.add(tx(3, "bob", 6), &ledger, 0), Err(far));
        // Three ahead is the limit, and is still allowed.
        assert!(pool.add(tx(4, "bob", 5), &ledger, 0).is_ok());

        let broke = SchedError::CannotPayFee {
            sender: "poor".into(),
            balance: 5,
            fee: 10,
        };
        assert_eq!(pool.add(tx(5, "poor", 0), &ledger, 0), Err(broke));
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn the_same_transaction_is_not_accepted_twice() {
        let mut pool = pool();
        pool.add(tx(1, "alice", 0), &funded(), 0).unwrap();
        assert_eq!(
            pool.add(tx(1, "alice", 0), &funded(), 0),
            Err(SchedError::Duplicate(1))
        );
        // The id is what counts, even on a different sender and nonce.
        assert_eq!(
            pool.add(tx(1, "bob", 0), &funded(), 0),
            Err(SchedError::Duplicate(1))
        );
    }

    #[test]
    fn a_malformed_transaction_never_reaches_the_ledger_checks() {
        let mut pool = pool();
        let empty = Transaction::new(1, "alice", 0).with_fee(10);
        assert!(matches!(
            pool.add(empty, &funded(), 0),
            Err(SchedError::Invalid(_))
        ));
        assert!(pool.is_empty());
    }

    #[test]
    fn a_better_paying_transaction_replaces_one_with_the_same_nonce() {
        let mut pool = pool();
        pool.add(tx(1, "alice", 0).with_fee(100), &funded(), 0)
            .unwrap();
        // 10% more is the least that will do.
        let admission = pool
            .add(tx(2, "alice", 0).with_fee(110), &funded(), 0)
            .unwrap();
        assert_eq!(admission.replaced, Some(1));
        assert_eq!(pool.len(), 1);
        assert!(!pool.contains(1));
        assert_eq!(pool.get("alice", 0).unwrap().tx.fee, 110);
    }

    #[test]
    fn a_replacement_that_does_not_pay_enough_more_is_refused() {
        let mut pool = pool();
        pool.add(tx(1, "alice", 0).with_fee(100), &funded(), 0)
            .unwrap();
        let short = SchedError::ReplacementUnderpriced {
            fee: 109,
            needed: 110,
        };
        assert_eq!(
            pool.add(tx(2, "alice", 0).with_fee(109), &funded(), 0),
            Err(short)
        );
        let same = pool.add(tx(3, "alice", 0).with_fee(100), &funded(), 0);
        assert!(matches!(
            same,
            Err(SchedError::ReplacementUnderpriced { .. })
        ));
        // The original is still there.
        assert!(pool.contains(1) && pool.len() == 1);
    }

    #[test]
    fn the_required_increase_is_rounded_up() {
        let mut pool = pool();
        pool.add(tx(1, "alice", 0).with_fee(5), &funded(), 0)
            .unwrap();
        // 10% of 5 is half a unit, which rounds up to a whole one.
        let short = SchedError::ReplacementUnderpriced { fee: 5, needed: 6 };
        assert_eq!(
            pool.add(tx(2, "alice", 0).with_fee(5), &funded(), 0),
            Err(short)
        );
        assert!(
            pool.add(tx(3, "alice", 0).with_fee(6), &funded(), 0)
                .is_ok()
        );
    }

    #[test]
    fn a_different_nonce_is_a_different_transaction_not_a_replacement() {
        let mut pool = pool();
        pool.add(tx(1, "alice", 0).with_fee(100), &funded(), 0)
            .unwrap();
        let admission = pool
            .add(tx(2, "alice", 1).with_fee(1), &funded(), 0)
            .unwrap();
        assert_eq!(admission.replaced, None);
        assert_eq!(pool.len(), 2);
    }
}
