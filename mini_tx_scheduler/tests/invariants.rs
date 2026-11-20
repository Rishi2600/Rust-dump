//! Properties every schedule must have, checked on many random workloads.
//!
//! The unit tests show the scheduler doing the right thing on examples
//! chosen by hand. These check that it never does the wrong thing on
//! thousands of transactions nobody chose.

use std::collections::BTreeMap;

use mini_tx_scheduler::conflict::conflicts;
use mini_tx_scheduler::mempool::{Mempool, PoolConfig};
use mini_tx_scheduler::priority::Policy;
use mini_tx_scheduler::scheduler::{BlockLimits, Schedule, build_schedule};
use mini_tx_scheduler::workload::{Workload, random_workload};

const SEEDS: u64 = 60;

/// A pool holding as much of the workload as it will accept.
fn pool_for(workload: &Workload, policy: Policy) -> Mempool {
    let mut pool = Mempool::new(PoolConfig {
        capacity: 100_000,
        max_nonce_gap: 100_000,
        policy,
        ..PoolConfig::default()
    });
    for tx in &workload.transactions {
        // Some are refused, for instance for a fee the sender cannot pay.
        let _ = pool.add(tx.clone(), &workload.ledger, 0);
    }
    pool
}

/// A different, valid set of limits for each seed.
fn limits_for(seed: u64) -> BlockLimits {
    BlockLimits {
        threads: 1 + (seed % 8) as usize,
        max_units: 2_000 + seed * 500,
        max_transactions: 10 + (seed % 5) as usize * 40,
    }
}

/// Run `check` on a schedule built from each seed, under each policy.
fn for_each_schedule(check: impl Fn(&Workload, &Mempool, &BlockLimits, Policy, &Schedule)) {
    let mut scheduled = 0;
    for seed in 0..SEEDS {
        let workload = random_workload(seed, 12, 150);
        for policy in Policy::ALL {
            let pool = pool_for(&workload, policy);
            let limits = limits_for(seed);
            let schedule = build_schedule(&pool, &workload.ledger, policy, &limits);
            scheduled += schedule.len();
            check(&workload, &pool, &limits, policy, &schedule);
        }
    }
    // Guard against a check that passes because nothing was ever scheduled.
    assert!(
        scheduled > 1000,
        "only {scheduled} transactions were scheduled"
    );
}

#[test]
fn no_batch_holds_two_transactions_that_clash() {
    for_each_schedule(|_, _, _, _, schedule| {
        for batch in schedule.batches() {
            for first in 0..batch.len() {
                for second in first + 1..batch.len() {
                    let (a, b) = (&batch[first], &batch[second]);
                    assert!(!conflicts(a, b), "{} and {} share a batch", a.id, b.id);
                }
            }
        }
    });
}

#[test]
fn of_two_clashing_transactions_the_one_chosen_first_runs_first() {
    for_each_schedule(|_, _, _, _, schedule| {
        let picks = &schedule.picks;
        for earlier in 0..picks.len() {
            for later in earlier + 1..picks.len() {
                if conflicts(&picks[earlier].tx, &picks[later].tx) {
                    assert!(
                        picks[earlier].batch < picks[later].batch,
                        "{} was chosen before {} but does not run before it",
                        picks[earlier].tx.id,
                        picks[later].tx.id
                    );
                }
            }
        }
    });
}

#[test]
fn each_senders_transactions_appear_in_nonce_order_without_gaps() {
    for_each_schedule(|workload, _, _, _, schedule| {
        let mut next: BTreeMap<&str, u64> = BTreeMap::new();
        for pick in &schedule.picks {
            let sender = pick.tx.sender.as_str();
            let expected = next
                .entry(sender)
                .or_insert_with(|| workload.ledger.nonce(sender));
            assert_eq!(pick.tx.nonce, *expected, "{sender} out of order");
            *expected += 1;
        }
    });
}

#[test]
fn a_schedule_stays_within_every_limit() {
    for_each_schedule(|_, _, limits, _, schedule| {
        let units: u64 = schedule.picks.iter().map(|pick| pick.tx.units).sum();
        assert_eq!(units, schedule.units);
        assert!(units <= limits.max_units);
        assert!(schedule.len() <= limits.max_transactions);
        for batch in schedule.batches() {
            assert!(batch.len() <= limits.threads);
            assert!(
                !batch.is_empty(),
                "an empty batch in the middle of a schedule"
            );
        }
    });
}

#[test]
fn a_better_transaction_is_only_chosen_later_when_its_own_queue_held_it_back() {
    for_each_schedule(|_, pool, _, policy, schedule| {
        let picks = &schedule.picks;
        let key = |index: usize| {
            let tx = &picks[index].tx;
            policy.key(tx, pool.get(&tx.sender, tx.nonce).unwrap().arrival)
        };
        for later in 0..picks.len() {
            for earlier in 0..later {
                if key(later) <= key(earlier) {
                    continue;
                }
                // `later` outranks `earlier` yet was chosen after it. The
                // only allowed reason: the transaction just before it in its
                // sender's queue had not been chosen yet at that point.
                let tx = &picks[later].tx;
                let held_back = picks[earlier..later]
                    .iter()
                    .any(|pick| pick.tx.sender == tx.sender && pick.tx.nonce + 1 == tx.nonce);
                assert!(
                    held_back,
                    "{} was passed over by {}",
                    tx.id, picks[earlier].tx.id
                );
            }
        }
    });
}

#[test]
fn every_scheduled_transaction_came_from_the_pool_exactly_once() {
    for_each_schedule(|_, pool, _, _, schedule| {
        let mut ids = schedule.ids();
        let chosen = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), chosen, "a transaction was scheduled twice");
        assert!(ids.iter().all(|id| pool.contains(*id)));
    });
}
