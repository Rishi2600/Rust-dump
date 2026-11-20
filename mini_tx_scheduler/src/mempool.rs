use std::collections::{BTreeMap, HashSet};

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
}
