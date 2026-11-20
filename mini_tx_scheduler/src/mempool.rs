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
        // A replacement reuses the place of what it replaced. Anything else
        // needs a free place, or has to win one.
        let evicted = match replaced {
            Some(_) => None,
            None => self.make_room(&tx)?,
        };
        self.insert(tx, slot);
        Ok(Admission { replaced, evicted })
    }

    /// Make sure there is room for `tx`, by pushing out the least valuable
    /// transaction if the pool is full.
    ///
    /// A full pool should hold the best-paying transactions it has seen, not
    /// merely the earliest. So a newcomer may displace something, but only
    /// something it outranks.
    fn make_room(&mut self, tx: &Transaction) -> SchedResult<Option<u64>> {
        if self.len() < self.config.capacity {
            return Ok(None);
        }
        let policy = self.config.policy;
        // Only the last transaction in a sender's queue can go. Removing one
        // from the middle would strand everything queued behind it. The
        // newcomer's own sender is left alone for the same reason.
        let tails = self
            .by_sender
            .iter()
            .filter(|(sender, _)| **sender != tx.sender);
        let tails = tails.filter_map(|(_, queue)| queue.values().next_back());
        let Some(victim) = tails.min_by_key(|pending| policy.key(&pending.tx, pending.arrival))
        else {
            return Err(SchedError::PoolFull);
        };
        if policy.key(tx, self.next_arrival) <= policy.key(&victim.tx, victim.arrival) {
            return Err(SchedError::PoolFull);
        }
        let (sender, nonce, id) = (victim.tx.sender.clone(), victim.tx.nonce, victim.tx.id);
        self.remove(&sender, nonce);
        Ok(Some(id))
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

impl Mempool {
    /// Drop transactions that have waited too long, and return them.
    ///
    /// A transaction that is never chosen would otherwise stay forever. A
    /// time limit bounds how long a sender can be left guessing, and gives
    /// back the room.
    pub fn expire(&mut self, slot: u64) -> Vec<Transaction> {
        let ttl = self.config.ttl_slots;
        let too_old = |pending: &Pending| slot.saturating_sub(pending.added_slot) >= ttl;
        let mut doomed = Vec::new();
        for (sender, queue) in &self.by_sender {
            // When one transaction goes, so does everything queued behind
            // it: without it they could never run.
            let Some(first) = queue.values().find(|pending| too_old(pending)) else {
                continue;
            };
            let from = first.tx.nonce;
            doomed.extend(
                queue
                    .range(from..)
                    .map(|(nonce, _)| (sender.clone(), *nonce)),
            );
        }
        let removed = doomed
            .into_iter()
            .filter_map(|(sender, nonce)| self.remove(&sender, nonce));
        removed.map(|pending| pending.tx).collect()
    }
}

impl Mempool {
    /// The transactions that could run right now: for each sender, the one
    /// whose nonce is exactly the next the ledger expects.
    ///
    /// Everything else in the pool is waiting on one of these.
    pub fn ready(&self, ledger: &Ledger) -> Vec<&Pending> {
        let heads = self
            .by_sender
            .iter()
            .filter_map(|(sender, queue)| queue.get(&ledger.nonce(sender)));
        heads.collect()
    }

    /// The transaction that becomes runnable once this sender's `nonce` has
    /// been used.
    pub fn next_after(&self, sender: &str, nonce: u64) -> Option<&Pending> {
        self.get(sender, nonce.checked_add(1)?)
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

    fn small_pool(capacity: usize) -> Mempool {
        Mempool::new(PoolConfig {
            capacity,
            ..PoolConfig::default()
        })
    }

    fn many_funded() -> Ledger {
        Ledger::with_balances([
            ("alice", 1000),
            ("bob", 1000),
            ("carol", 1000),
            ("dave", 1000),
        ])
    }

    #[test]
    fn a_full_pool_drops_its_worst_transaction_for_a_better_one() {
        let (mut pool, ledger) = (small_pool(3), many_funded());
        pool.add(tx(1, "alice", 0).with_fee(30), &ledger, 0)
            .unwrap();
        pool.add(tx(2, "bob", 0).with_fee(10), &ledger, 0).unwrap();
        pool.add(tx(3, "carol", 0).with_fee(20), &ledger, 0)
            .unwrap();

        let admission = pool.add(tx(4, "dave", 0).with_fee(25), &ledger, 0).unwrap();
        assert_eq!(admission.evicted, Some(2), "bob paid least");
        assert_eq!(pool.len(), 3);
        assert!(pool.contains(4) && !pool.contains(2));
    }

    #[test]
    fn a_full_pool_refuses_what_does_not_outbid_anything() {
        let (mut pool, ledger) = (small_pool(2), many_funded());
        pool.add(tx(1, "alice", 0).with_fee(30), &ledger, 0)
            .unwrap();
        pool.add(tx(2, "bob", 0).with_fee(20), &ledger, 0).unwrap();
        assert_eq!(
            pool.add(tx(3, "carol", 0).with_fee(15), &ledger, 0),
            Err(SchedError::PoolFull)
        );
        // Matching the lowest is not outbidding it: the earlier arrival stays.
        assert_eq!(
            pool.add(tx(4, "carol", 0).with_fee(20), &ledger, 0),
            Err(SchedError::PoolFull)
        );
        assert!(pool.contains(1) && pool.contains(2));
    }

    #[test]
    fn eviction_never_leaves_a_gap_in_a_senders_queue() {
        let (mut pool, ledger) = (small_pool(3), many_funded());
        // Alice's first transaction is the cheapest in the pool, but another
        // of hers is queued behind it.
        pool.add(tx(1, "alice", 0).with_fee(5), &ledger, 0).unwrap();
        pool.add(tx(2, "alice", 1).with_fee(50), &ledger, 0)
            .unwrap();
        pool.add(tx(3, "bob", 0).with_fee(40), &ledger, 0).unwrap();

        let admission = pool
            .add(tx(4, "carol", 0).with_fee(45), &ledger, 0)
            .unwrap();
        // Bob's goes, not alice's cheap one: hers cannot leave without
        // stranding the transaction that depends on it.
        assert_eq!(admission.evicted, Some(3));
        assert!(pool.contains(1) && pool.contains(2));
    }

    #[test]
    fn a_sender_does_not_evict_their_own_earlier_transactions() {
        let (mut pool, ledger) = (small_pool(2), many_funded());
        pool.add(tx(1, "alice", 0).with_fee(5), &ledger, 0).unwrap();
        pool.add(tx(2, "alice", 1).with_fee(5), &ledger, 0).unwrap();
        // Evicting nonce 1 to admit nonce 2 would leave nonce 2 unreachable.
        assert_eq!(
            pool.add(tx(3, "alice", 2).with_fee(500), &ledger, 0),
            Err(SchedError::PoolFull)
        );
    }

    #[test]
    fn a_replacement_needs_no_room() {
        let (mut pool, ledger) = (small_pool(1), many_funded());
        pool.add(tx(1, "alice", 0).with_fee(10), &ledger, 0)
            .unwrap();
        let admission = pool
            .add(tx(2, "alice", 0).with_fee(20), &ledger, 0)
            .unwrap();
        assert_eq!((admission.replaced, admission.evicted), (Some(1), None));
    }

    fn ids(txs: &[Transaction]) -> Vec<u64> {
        txs.iter().map(|tx| tx.id).collect()
    }

    #[test]
    fn a_transaction_expires_after_waiting_its_full_time() {
        let mut pool = Mempool::new(PoolConfig {
            ttl_slots: 3,
            ..PoolConfig::default()
        });
        pool.add(tx(1, "alice", 0), &funded(), 10).unwrap();
        assert!(pool.expire(11).is_empty());
        assert!(pool.expire(12).is_empty());
        // Three slots after it was added, its time is up.
        assert_eq!(ids(&pool.expire(13)), [1]);
        assert!(pool.is_empty());
    }

    #[test]
    fn expiry_takes_everything_queued_behind_the_expired_transaction() {
        let mut pool = Mempool::new(PoolConfig {
            ttl_slots: 3,
            ..PoolConfig::default()
        });
        pool.add(tx(1, "alice", 0), &funded(), 0).unwrap();
        // Added later, so not old enough by itself.
        pool.add(tx(2, "alice", 1), &funded(), 2).unwrap();
        pool.add(tx(3, "bob", 0), &funded(), 2).unwrap();

        assert_eq!(ids(&pool.expire(3)), [1, 2]);
        // Bob's is as young as alice's second, and unaffected.
        assert!(pool.contains(3) && pool.len() == 1);
    }

    #[test]
    fn a_replacement_starts_its_wait_afresh() {
        let mut pool = Mempool::new(PoolConfig {
            ttl_slots: 3,
            ..PoolConfig::default()
        });
        pool.add(tx(1, "alice", 0).with_fee(10), &funded(), 0)
            .unwrap();
        pool.add(tx(2, "alice", 0).with_fee(20), &funded(), 2)
            .unwrap();
        assert!(pool.expire(3).is_empty());
        assert_eq!(ids(&pool.expire(5)), [2]);
    }

    fn ready_ids(pool: &Mempool, ledger: &Ledger) -> Vec<u64> {
        pool.ready(ledger)
            .iter()
            .map(|pending| pending.tx.id)
            .collect()
    }

    #[test]
    fn only_the_first_of_each_senders_queue_is_ready() {
        let (mut pool, ledger) = (pool(), funded());
        pool.add(tx(1, "alice", 0), &ledger, 0).unwrap();
        pool.add(tx(2, "alice", 1), &ledger, 0).unwrap();
        pool.add(tx(3, "bob", 0), &ledger, 0).unwrap();
        assert_eq!(ready_ids(&pool, &ledger), [1, 3]);
        // What comes after each of them is found by nonce.
        assert_eq!(
            pool.next_after("alice", 0).map(|pending| pending.tx.id),
            Some(2)
        );
        assert!(pool.next_after("alice", 1).is_none());
        assert!(pool.next_after("bob", u64::MAX).is_none());
    }

    #[test]
    fn a_queue_with_a_gap_at_the_front_has_nothing_ready() {
        let (mut pool, ledger) = (pool(), funded());
        // Nonce 0 has not arrived, so nonce 1 cannot run yet.
        pool.add(tx(1, "alice", 1), &ledger, 0).unwrap();
        assert!(ready_ids(&pool, &ledger).is_empty());
        pool.add(tx(2, "alice", 0), &ledger, 0).unwrap();
        assert_eq!(ready_ids(&pool, &ledger), [2]);
    }

    #[test]
    fn readiness_follows_the_ledgers_nonce() {
        let (mut pool, mut ledger) = (pool(), funded());
        pool.add(tx(1, "alice", 0), &ledger, 0).unwrap();
        pool.add(tx(2, "alice", 1), &ledger, 0).unwrap();
        // Once nonce 0 has been applied and taken out, nonce 1 is next.
        pool.remove("alice", 0);
        ledger.bump_nonce("alice");
        assert_eq!(ready_ids(&pool, &ledger), [2]);
    }
}
