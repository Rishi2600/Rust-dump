//! Made-up transactions for testing and demonstration.
//!
//! The same seed always gives the same workload, so anything found with one
//! can be found again.

use std::collections::BTreeMap;

use crate::ledger::Ledger;
use crate::tx::Transaction;

/// A small pseudo-random number generator (xorshift64*).
///
/// It is not secure and does not need to be. What matters here is that it
/// is repeatable and has no dependencies.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // The generator must not start at zero, and nearby seeds should not
        // give nearby sequences, so the seed is scrambled first.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number from 0 up to, but not including, `bound`.
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound.max(1)
    }

    /// True about `percent` times in a hundred.
    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// A starting ledger and a list of transactions to throw at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workload {
    pub ledger: Ledger,
    /// In order of arrival. Each sender's nonces count up from 0.
    pub transactions: Vec<Transaction>,
}

/// Generate `count` transactions among `accounts` accounts.
///
/// The mix is chosen to exercise a scheduler, not to look realistic:
/// - a few "hot" accounts receive a large share of the payments, so many
///   transactions clash;
/// - some transactions read a shared `oracle` account and a few write it;
/// - fees and sizes vary widely, so priority order differs from arrival
///   order;
/// - some payments are larger than the sender's balance, so some fail.
pub fn random_workload(seed: u64, accounts: usize, count: usize) -> Workload {
    let mut rng = Rng::new(seed);
    let accounts = accounts.max(2);
    let names: Vec<String> = (0..accounts).map(|index| format!("acct{index}")).collect();
    let funded = names
        .iter()
        .map(|name| (name.as_str(), 2_000 + rng.below(50_000)));
    let ledger = Ledger::with_balances(funded.collect::<Vec<_>>());

    let mut nonces: BTreeMap<usize, u64> = BTreeMap::new();
    let mut transactions = Vec::with_capacity(count);
    for id in 0..count as u64 {
        let sender = rng.below(accounts as u64) as usize;
        let nonce = nonces.entry(sender).or_insert(0);
        let mut tx = Transaction::new(id, &names[sender], *nonce);
        *nonce += 1;

        // A third of the payments go to one of the first two accounts.
        let hot = rng.chance(33);
        let mut payee = if hot {
            rng.below(2)
        } else {
            rng.below(accounts as u64)
        } as usize;
        if payee == sender {
            payee = (payee + 1) % accounts;
        }
        let amount = 1 + rng.below(4_000);
        tx = if rng.chance(4) {
            tx.pay("oracle", amount)
        } else {
            tx.pay(&names[payee], amount)
        };
        if rng.chance(25) {
            tx = tx.reading("oracle");
        }
        let units = [50, 100, 200, 400, 800][rng.below(5) as usize];
        transactions.push(tx.with_fee(1 + rng.below(300)).with_units(units));
    }
    Workload {
        ledger,
        transactions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_workload() {
        assert_eq!(random_workload(7, 10, 200), random_workload(7, 10, 200));
        assert_ne!(random_workload(7, 10, 200), random_workload(8, 10, 200));
    }

    #[test]
    fn every_generated_transaction_is_well_formed() {
        let workload = random_workload(1, 8, 500);
        assert_eq!(workload.transactions.len(), 500);
        for tx in &workload.transactions {
            assert_eq!(tx.validate(), Ok(()), "{tx:?}");
        }
    }

    #[test]
    fn each_senders_nonces_count_up_from_zero() {
        let workload = random_workload(3, 6, 300);
        let mut expected: BTreeMap<&str, u64> = BTreeMap::new();
        for tx in &workload.transactions {
            let next = expected.entry(&tx.sender).or_insert(0);
            assert_eq!(tx.nonce, *next);
            *next += 1;
        }
    }

    #[test]
    fn the_workload_has_the_variety_it_promises() {
        let workload = random_workload(5, 10, 1000);
        let txs = &workload.transactions;
        let pays = |tx: &Transaction, account: &str| tx.transfers.iter().any(|t| t.to == account);
        let to_hot = txs
            .iter()
            .filter(|tx| pays(tx, "acct0") || pays(tx, "acct1"))
            .count();
        assert!(to_hot > 250, "only {to_hot} payments to the hot accounts");
        assert!(
            txs.iter()
                .any(|tx| tx.reads.contains(&"oracle".to_string()))
        );
        assert!(txs.iter().any(|tx| pays(tx, "oracle")));
        let sizes: std::collections::BTreeSet<u64> = txs.iter().map(|tx| tx.units).collect();
        assert_eq!(sizes.len(), 5);
    }

    #[test]
    fn the_generator_is_spread_out() {
        let mut rng = Rng::new(42);
        let mut buckets = [0u32; 10];
        for _ in 0..10_000 {
            buckets[rng.below(10) as usize] += 1;
        }
        // Each of ten buckets should get roughly a tenth.
        assert!(
            buckets.iter().all(|&count| (800..1200).contains(&count)),
            "{buckets:?}"
        );
        assert_eq!(Rng::new(0).below(0), 0);
    }
}
