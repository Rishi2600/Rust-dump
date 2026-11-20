//! The claim the whole design rests on: running a block's batches in
//! parallel gives exactly the result of running its transactions one at a
//! time, in the order priority chose them.

use std::collections::BTreeMap;

use mini_tx_scheduler::executor::{Receipt, Status, execute_batches, execute_serial};
use mini_tx_scheduler::mempool::{Mempool, PoolConfig};
use mini_tx_scheduler::node::{Node, NodeConfig};
use mini_tx_scheduler::priority::Policy;
use mini_tx_scheduler::scheduler::{BlockLimits, build_schedule};
use mini_tx_scheduler::tx::Transaction;
use mini_tx_scheduler::workload::random_workload;

fn roomy_pool(policy: Policy) -> PoolConfig {
    PoolConfig {
        capacity: 100_000,
        max_nonce_gap: 100_000,
        policy,
        ..PoolConfig::default()
    }
}

fn by_id(mut receipts: Vec<Receipt>) -> Vec<Receipt> {
    receipts.sort_by_key(|receipt| receipt.id);
    receipts
}

#[test]
fn parallel_batches_give_the_same_ledger_as_serial_execution_in_priority_order() {
    let mut compared = 0;
    for seed in 0..80 {
        let workload = random_workload(seed, 10, 200);
        let policy = Policy::ALL[(seed % 3) as usize];
        let mut pool = Mempool::new(roomy_pool(policy));
        for tx in &workload.transactions {
            let _ = pool.add(tx.clone(), &workload.ledger, 0);
        }
        let limits = BlockLimits {
            threads: 1 + (seed % 6) as usize,
            max_units: 100_000,
            max_transactions: 10_000,
        };
        let schedule = build_schedule(&pool, &workload.ledger, policy, &limits);

        // The reference: one transaction at a time, most important first.
        let in_order: Vec<Transaction> =
            schedule.picks.iter().map(|pick| pick.tx.clone()).collect();
        let mut serial = workload.ledger.clone();
        let serial_receipts = execute_serial(&mut serial, &in_order);

        let mut parallel = workload.ledger.clone();
        let parallel_receipts = execute_batches(&mut parallel, &schedule.batches(), limits.threads);

        assert_eq!(parallel, serial, "seed {seed}: the ledgers differ");
        // The receipts come out in a different order, batch by batch, but
        // every transaction has the same outcome.
        assert_eq!(
            by_id(parallel_receipts),
            by_id(serial_receipts),
            "seed {seed}"
        );
        compared += in_order.len();
    }
    assert!(
        compared > 5000,
        "only {compared} transactions were compared"
    );
}

#[test]
fn the_number_of_threads_changes_the_speed_not_the_result() {
    for seed in [3, 17, 41] {
        let workload = random_workload(seed, 10, 200);
        let mut results = Vec::new();
        for threads in [1, 2, 3, 8, 32] {
            let config = NodeConfig {
                pool: roomy_pool(Policy::FeePerUnit),
                limits: BlockLimits {
                    threads,
                    ..BlockLimits::default()
                },
                ..NodeConfig::default()
            };
            let mut node = Node::new(workload.ledger.clone(), config);
            for tx in &workload.transactions {
                let _ = node.submit(tx.clone());
            }
            // Batches are packed differently for each thread count, so the
            // blocks differ. The outcome must not.
            while !node.pool().is_empty() {
                node.produce_block();
            }
            results.push(node.ledger().clone());
        }
        assert!(
            results.windows(2).all(|pair| pair[0] == pair[1]),
            "seed {seed}"
        );
    }
}

#[test]
fn a_block_never_contains_a_transaction_that_has_to_be_rejected() {
    // The builder reserves each sender's money as it goes, so everything it
    // schedules can pay its fee and has the right nonce when its turn comes.
    for seed in 0..40 {
        let workload = random_workload(seed, 8, 250);
        let mut node = Node::new(
            workload.ledger.clone(),
            NodeConfig {
                pool: roomy_pool(Policy::FeePerUnit),
                ..NodeConfig::default()
            },
        );
        for tx in &workload.transactions {
            let _ = node.submit(tx.clone());
        }
        for _ in 0..200 {
            let block = node.produce_block();
            for receipt in &block.receipts {
                assert!(
                    !matches!(receipt.status, Status::Rejected(_)),
                    "seed {seed}: {receipt:?}"
                );
            }
        }
    }
}

#[test]
fn across_many_blocks_money_is_conserved_and_nothing_runs_twice() {
    for seed in 0..40 {
        let workload = random_workload(seed, 10, 300);
        let mut node = Node::new(
            workload.ledger.clone(),
            NodeConfig {
                pool: roomy_pool(Policy::FeePerUnit),
                ..NodeConfig::default()
            },
        );
        let supply = node.ledger().total();
        let mut accepted = 0;
        for tx in &workload.transactions {
            accepted += usize::from(node.submit(tx.clone()).is_ok());
        }

        let mut seen: BTreeMap<u64, usize> = BTreeMap::new();
        let (mut fees, mut expired) = (0, 0);
        // Long enough for everything left waiting to reach its time limit.
        for _ in 0..200 {
            let block = node.produce_block();
            fees += block.fees;
            expired += block.expired.len();
            for receipt in &block.receipts {
                *seen.entry(receipt.id).or_insert(0) += 1;
            }
            assert_eq!(
                node.ledger().total(),
                supply,
                "seed {seed}, slot {}",
                block.slot
            );
        }
        assert!(
            seen.values().all(|&times| times == 1),
            "seed {seed}: a transaction ran twice"
        );
        // Every accepted transaction either ran or expired. None is lost.
        assert!(node.pool().is_empty());
        assert_eq!(seen.len() + expired, accepted, "seed {seed}");
        assert_eq!(node.ledger().balance("validator"), fees);
    }
}
