//! The node as a whole: submit transactions, produce blocks, check the ledger.

use mini_tx_scheduler::executor::Status;
use mini_tx_scheduler::ledger::Ledger;
use mini_tx_scheduler::mempool::PoolConfig;
use mini_tx_scheduler::node::{Node, NodeConfig};
use mini_tx_scheduler::scheduler::BlockLimits;
use mini_tx_scheduler::tx::Transaction;

fn ledger() -> Ledger {
    Ledger::with_balances([("alice", 1000), ("bob", 1000), ("carol", 1000)])
}

fn node() -> Node {
    Node::new(ledger(), NodeConfig::default())
}

fn tx(id: u64, sender: &str, nonce: u64, to: &str, amount: u64, fee: u64) -> Transaction {
    Transaction::new(id, sender, nonce)
        .pay(to, amount)
        .with_fee(fee)
}

#[test]
fn a_block_takes_transactions_from_the_pool_and_applies_them() {
    let mut node = node();
    node.submit(tx(1, "alice", 0, "dave", 100, 10)).unwrap();
    node.submit(tx(2, "bob", 0, "erin", 200, 30)).unwrap();

    let block = node.produce_block();
    assert_eq!(block.slot, 0);
    // The better-paying one was chosen first.
    assert_eq!(block.schedule.ids(), [2, 1]);
    assert_eq!(block.fees, 40);
    assert!(
        block
            .receipts
            .iter()
            .all(|receipt| receipt.status == Status::Success)
    );

    assert_eq!(node.ledger().balance("dave"), 100);
    assert_eq!(node.ledger().balance("erin"), 200);
    assert_eq!(node.ledger().balance("alice"), 890);
    // Once included, a transaction is gone from the pool.
    assert!(node.pool().is_empty());
    assert_eq!(node.slot(), 1);
}

#[test]
fn a_slot_with_nothing_to_do_produces_an_empty_block() {
    let mut node = node();
    let block = node.produce_block();
    assert!(block.schedule.is_empty() && block.receipts.is_empty());
    assert_eq!((block.fees, node.slot()), (0, 1));
    assert_eq!(node.ledger(), &ledger());
}

#[test]
fn what_does_not_fit_in_one_block_waits_for_the_next() {
    let config = NodeConfig {
        limits: BlockLimits {
            max_units: 250,
            ..BlockLimits::default()
        },
        ..NodeConfig::default()
    };
    let mut node = Node::new(ledger(), config);
    // Three transactions of 100 units each: only two fit per block.
    node.submit(tx(1, "alice", 0, "dave", 1, 10)).unwrap();
    node.submit(tx(2, "bob", 0, "dave", 1, 30)).unwrap();
    node.submit(tx(3, "carol", 0, "dave", 1, 20)).unwrap();

    assert_eq!(node.produce_block().schedule.ids(), [2, 3]);
    assert_eq!(node.pool().len(), 1);
    // The one that paid least gets in when there is room.
    assert_eq!(node.produce_block().schedule.ids(), [1]);
    assert!(node.pool().is_empty());
}

#[test]
fn a_transaction_waiting_on_an_earlier_nonce_runs_once_that_arrives() {
    let mut node = node();
    node.submit(tx(1, "alice", 1, "dave", 5, 50)).unwrap();
    assert!(
        node.produce_block().schedule.is_empty(),
        "nonce 0 is missing"
    );

    node.submit(tx(2, "alice", 0, "dave", 5, 10)).unwrap();
    // Both run in the same block, in nonce order.
    assert_eq!(node.produce_block().schedule.ids(), [2, 1]);
    assert_eq!(node.ledger().nonce("alice"), 2);
}

#[test]
fn a_transaction_that_fails_is_still_included_and_charged() {
    let mut node = node();
    // Alice has 1000 and tries to send 5000.
    node.submit(tx(1, "alice", 0, "dave", 5000, 10)).unwrap();
    let block = node.produce_block();
    assert!(matches!(block.receipts[0].status, Status::Failed(_)));
    assert_eq!(block.fees, 10);
    assert_eq!(
        (
            node.ledger().balance("alice"),
            node.ledger().balance("dave")
        ),
        (990, 0)
    );
    assert!(node.pool().is_empty());
}

#[test]
fn the_pool_judges_new_transactions_by_the_ledger_as_it_now_stands() {
    let mut node = node();
    node.submit(tx(1, "alice", 0, "dave", 5, 10)).unwrap();
    node.produce_block();
    // Nonce 0 has been used, so sending it again is refused.
    assert!(node.submit(tx(2, "alice", 0, "dave", 5, 10)).is_err());
    assert!(node.submit(tx(3, "alice", 1, "dave", 5, 10)).is_ok());
}

#[test]
fn the_fees_of_a_block_go_to_the_validator() {
    let mut node = node();
    node.submit(tx(1, "alice", 0, "dave", 100, 10)).unwrap();
    node.submit(tx(2, "bob", 0, "erin", 200, 30)).unwrap();
    // A failed transaction pays its fee like any other.
    node.submit(tx(3, "carol", 0, "frank", 99_999, 5)).unwrap();
    let block = node.produce_block();
    assert_eq!(block.fees, 45);
    assert_eq!(node.ledger().balance("validator"), 45);
}

#[test]
fn no_money_is_created_or_destroyed() {
    let mut node = node();
    let before = node.ledger().total();
    for (index, sender) in ["alice", "bob", "carol"].into_iter().enumerate() {
        for nonce in 0..4 {
            let id = (index * 10 + nonce) as u64;
            // Some of these succeed and some ask for more than there is.
            let amount = 150 * (nonce as u64 + 1);
            node.submit(tx(id, sender, nonce as u64, "dave", amount, 7))
                .unwrap();
        }
    }
    while !node.pool().is_empty() {
        node.produce_block();
    }
    // Every unit that left an account arrived in another: payees for the
    // transfers, the validator for the fees.
    assert_eq!(node.ledger().total(), before);
    assert_eq!(node.ledger().balance("validator"), 12 * 7);
}

#[test]
fn the_validator_account_can_be_chosen() {
    let config = NodeConfig {
        validator: "treasury".to_string(),
        ..NodeConfig::default()
    };
    let mut node = Node::new(ledger(), config);
    node.submit(tx(1, "alice", 0, "dave", 1, 9)).unwrap();
    node.produce_block();
    assert_eq!(node.ledger().balance("treasury"), 9);
    assert_eq!(node.ledger().balance("validator"), 0);
}

#[test]
fn a_transaction_that_never_becomes_runnable_is_eventually_dropped() {
    let config = NodeConfig {
        pool: PoolConfig {
            ttl_slots: 3,
            ..PoolConfig::default()
        },
        ..NodeConfig::default()
    };
    let mut node = Node::new(ledger(), config);
    // Nonce 0 never arrives, so this can never run.
    node.submit(tx(1, "alice", 1, "dave", 5, 50)).unwrap();

    for slot in 0..3 {
        let block = node.produce_block();
        assert!(
            block.expired.is_empty() && block.schedule.is_empty(),
            "slot {slot}"
        );
    }
    let block = node.produce_block();
    assert_eq!(block.expired, [1]);
    assert!(node.pool().is_empty());
    // Having expired, it can be sent again.
    assert!(node.submit(tx(1, "alice", 1, "dave", 5, 50)).is_ok());
}

#[test]
fn a_transaction_included_in_time_does_not_expire() {
    let mut node = node();
    node.submit(tx(1, "alice", 0, "dave", 5, 50)).unwrap();
    let block = node.produce_block();
    assert!(block.expired.is_empty());
    assert_eq!(block.schedule.ids(), [1]);
}
