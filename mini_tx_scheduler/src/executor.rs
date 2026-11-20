use std::collections::BTreeMap;
use std::thread;

use crate::ledger::Ledger;
use crate::tx::Transaction;

/// How a transaction turned out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Every transfer was made.
    Success,
    /// The fee was charged and the nonce used, but no transfer was made.
    Failed(String),
    /// Not applied at all: no fee, no nonce, no trace.
    Rejected(String),
}

/// The record of one transaction's execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub id: u64,
    pub status: Status,
    /// The fee actually charged.
    pub fee: u64,
    /// The compute units charged against the block.
    pub units: u64,
}

/// What a transaction does to the ledger, worked out but not yet done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub receipt: Receipt,
    /// The account whose nonce moves on, if the transaction was applied.
    pub nonce_of: Option<String>,
    /// The new balance of every account the transaction changes.
    pub balances: Vec<(String, u64)>,
}

/// Work out what `tx` would do to `ledger`, without changing the ledger.
///
/// Splitting execution into "work it out" and "write it down" is what makes
/// running transactions in parallel safe. Many of these can look at the same
/// ledger at once, because none of them modifies it.
///
/// A transaction either happens completely or leaves only its fee behind.
/// There is no outcome where some of its transfers are made and others not.
pub fn run(tx: &Transaction, ledger: &Ledger) -> Effect {
    let receipt = |status: Status, fee: u64, units: u64| Receipt {
        id: tx.id,
        status,
        fee,
        units,
    };
    let rejected = |reason: &str| Effect {
        receipt: receipt(Status::Rejected(reason.to_string()), 0, 0),
        nonce_of: None,
        balances: Vec::new(),
    };
    if tx.nonce != ledger.nonce(&tx.sender) {
        return rejected("wrong nonce");
    }
    let Some(after_fee) = ledger.balance(&tx.sender).checked_sub(tx.fee) else {
        return rejected("cannot pay the fee");
    };

    // From here on the fee is owed whatever happens. The work of checking
    // and running the transaction was done, and somebody has to pay for it.
    // If failed transactions were free, flooding the system with ones
    // designed to fail would cost an attacker nothing.
    let charged = |status: Status, balances: Vec<(String, u64)>| Effect {
        receipt: receipt(status, tx.fee, tx.units),
        nonce_of: Some(tx.sender.clone()),
        balances,
    };
    let fee_only = |reason: &str| {
        charged(
            Status::Failed(reason.to_string()),
            vec![(tx.sender.clone(), after_fee)],
        )
    };

    let total = tx
        .transfers
        .iter()
        .try_fold(0u64, |sum, transfer| sum.checked_add(transfer.amount));
    let Some(remaining) = total.and_then(|total| after_fee.checked_sub(total)) else {
        return fee_only("insufficient funds");
    };
    // Several transfers may pay the same account, so the amounts are added
    // up per payee before anything is recorded.
    let mut payees: BTreeMap<&str, u64> = BTreeMap::new();
    for transfer in &tx.transfers {
        let current = payees
            .entry(&transfer.to)
            .or_insert_with(|| ledger.balance(&transfer.to));
        match current.checked_add(transfer.amount) {
            Some(raised) => *current = raised,
            None => return fee_only("a payee's balance would overflow"),
        }
    }
    let mut balances = vec![(tx.sender.clone(), remaining)];
    balances.extend(
        payees
            .into_iter()
            .map(|(account, balance)| (account.to_string(), balance)),
    );
    charged(Status::Success, balances)
}

/// Write an effect into the ledger.
pub fn apply(ledger: &mut Ledger, effect: &Effect) {
    for (account, balance) in &effect.balances {
        ledger.set_balance(account, *balance);
    }
    if let Some(account) = &effect.nonce_of {
        ledger.bump_nonce(account);
    }
}

/// Run transactions one after another, each seeing the result of the one
/// before. This is the plain, obviously correct way to execute a block, and
/// the standard that any cleverer way has to match.
pub fn execute_serial(ledger: &mut Ledger, txs: &[Transaction]) -> Vec<Receipt> {
    let mut receipts = Vec::with_capacity(txs.len());
    for tx in txs {
        let effect = run(tx, ledger);
        apply(ledger, &effect);
        receipts.push(effect.receipt);
    }
    receipts
}

/// Run a block's batches: the batches one after another, the transactions
/// inside each batch at the same time.
///
/// For every batch, each transaction's effect is worked out against the
/// ledger as it stood when the batch began, on up to `threads` threads.
/// Only then are the effects written, one by one. That is sound because the
/// planner guarantees no two transactions in a batch touch the same account
/// unless both only read it. None of them could have seen another's change,
/// and no two of them write the same balance.
pub fn execute_batches(
    ledger: &mut Ledger,
    batches: &[Vec<Transaction>],
    threads: usize,
) -> Vec<Receipt> {
    let mut receipts = Vec::new();
    for batch in batches {
        let effects = run_together(batch, ledger, threads);
        for effect in &effects {
            apply(ledger, effect);
        }
        receipts.extend(effects.into_iter().map(|effect| effect.receipt));
    }
    receipts
}

/// Work out the effects of a batch on several threads. The result is in the
/// same order as the batch, whichever thread finished first.
fn run_together(batch: &[Transaction], ledger: &Ledger, threads: usize) -> Vec<Effect> {
    let workers = threads.clamp(1, batch.len().max(1));
    if workers == 1 {
        return batch.iter().map(|tx| run(tx, ledger)).collect();
    }
    let share = batch.len().div_ceil(workers);
    // Scoped threads may borrow the batch and the ledger, because the scope
    // does not end until every one of them has finished.
    thread::scope(|scope| {
        let running: Vec<_> = batch
            .chunks(share)
            .map(|txs| {
                scope.spawn(move || txs.iter().map(|tx| run(tx, ledger)).collect::<Vec<_>>())
            })
            .collect();
        let finished = running
            .into_iter()
            .map(|worker| worker.join().expect("a worker thread panicked"));
        finished.flatten().collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> Ledger {
        Ledger::with_balances([("alice", 1000), ("bob", 50)])
    }

    fn balances(effect: &Effect) -> Vec<(&str, u64)> {
        effect
            .balances
            .iter()
            .map(|(account, balance)| (account.as_str(), *balance))
            .collect()
    }

    #[test]
    fn a_successful_transaction_moves_the_money_and_the_fee() {
        let tx = Transaction::new(1, "alice", 0)
            .pay("bob", 100)
            .with_fee(10)
            .with_units(300);
        let effect = run(&tx, &ledger());
        assert_eq!(
            effect.receipt,
            Receipt {
                id: 1,
                status: Status::Success,
                fee: 10,
                units: 300
            }
        );
        assert_eq!(balances(&effect), [("alice", 890), ("bob", 150)]);
        assert_eq!(effect.nonce_of.as_deref(), Some("alice"));
    }

    #[test]
    fn working_out_an_effect_does_not_touch_the_ledger() {
        let before = ledger();
        let tx = Transaction::new(1, "alice", 0).pay("bob", 100).with_fee(10);
        run(&tx, &before);
        assert_eq!(before, ledger());
    }

    #[test]
    fn a_transaction_that_cannot_be_completed_still_pays_its_fee() {
        // Alice has 1000. The fee is affordable, the payment is not.
        let tx = Transaction::new(1, "alice", 0)
            .pay("bob", 5000)
            .with_fee(10);
        let effect = run(&tx, &ledger());
        assert_eq!(
            effect.receipt.status,
            Status::Failed("insufficient funds".into())
        );
        assert_eq!(effect.receipt.fee, 10);
        // Only the fee left her account, and bob received nothing.
        assert_eq!(balances(&effect), [("alice", 990)]);
        // Her nonce is used all the same, so the failure cannot be replayed.
        assert_eq!(effect.nonce_of.as_deref(), Some("alice"));
    }

    #[test]
    fn transfers_are_all_or_nothing() {
        // The first payment alone would fit; the two together do not.
        let tx = Transaction::new(1, "alice", 0)
            .pay("bob", 600)
            .pay("carol", 600)
            .with_fee(10);
        let effect = run(&tx, &ledger());
        assert!(matches!(effect.receipt.status, Status::Failed(_)));
        assert_eq!(balances(&effect), [("alice", 990)]);
    }

    #[test]
    fn a_transaction_out_of_turn_or_unable_to_pay_is_rejected_without_trace() {
        let wrong_nonce = Transaction::new(1, "alice", 3).pay("bob", 1).with_fee(10);
        let effect = run(&wrong_nonce, &ledger());
        assert_eq!(
            effect.receipt.status,
            Status::Rejected("wrong nonce".into())
        );
        assert_eq!((effect.receipt.fee, effect.receipt.units), (0, 0));
        assert!(effect.balances.is_empty() && effect.nonce_of.is_none());

        let no_fee = Transaction::new(2, "bob", 0).pay("alice", 1).with_fee(51);
        let effect = run(&no_fee, &ledger());
        assert_eq!(
            effect.receipt.status,
            Status::Rejected("cannot pay the fee".into())
        );
        assert!(effect.balances.is_empty());
    }

    #[test]
    fn several_payments_to_one_account_add_up() {
        let tx = Transaction::new(1, "alice", 0)
            .pay("bob", 100)
            .pay("carol", 5)
            .pay("bob", 20)
            .with_fee(1);
        let effect = run(&tx, &ledger());
        assert_eq!(
            balances(&effect),
            [("alice", 874), ("bob", 170), ("carol", 5)]
        );
    }

    #[test]
    fn a_payment_that_would_overflow_the_payee_fails_cleanly() {
        let rich = Ledger::with_balances([("alice", 1000), ("whale", u64::MAX)]);
        let tx = Transaction::new(1, "alice", 0).pay("whale", 5).with_fee(1);
        let effect = run(&tx, &rich);
        assert!(matches!(effect.receipt.status, Status::Failed(_)));
        assert_eq!(balances(&effect), [("alice", 999)]);
    }

    fn statuses(receipts: &[Receipt]) -> Vec<&Status> {
        receipts.iter().map(|receipt| &receipt.status).collect()
    }

    #[test]
    fn applying_an_effect_changes_balances_and_uses_the_nonce() {
        let mut ledger = ledger();
        let tx = Transaction::new(1, "alice", 0).pay("bob", 100).with_fee(10);
        let effect = run(&tx, &ledger);
        apply(&mut ledger, &effect);
        assert_eq!((ledger.balance("alice"), ledger.balance("bob")), (890, 150));
        assert_eq!((ledger.nonce("alice"), ledger.nonce("bob")), (1, 0));
        // The fee has left the ledger. Where it goes is the caller's affair.
        assert_eq!(ledger.total(), 1050 - 10);
    }

    #[test]
    fn later_transactions_see_what_earlier_ones_did() {
        let mut ledger = ledger();
        let txs = [
            Transaction::new(1, "alice", 0).pay("bob", 500).with_fee(1),
            // Bob started with 50. This only works because of the payment above.
            Transaction::new(2, "bob", 0).pay("carol", 400).with_fee(1),
            Transaction::new(3, "alice", 1)
                .pay("carol", 100)
                .with_fee(1),
        ];
        let receipts = execute_serial(&mut ledger, &txs);
        assert!(
            receipts
                .iter()
                .all(|receipt| receipt.status == Status::Success)
        );
        assert_eq!(ledger.balance("alice"), 1000 - 500 - 100 - 2);
        assert_eq!(ledger.balance("bob"), 50 + 500 - 400 - 1);
        assert_eq!(ledger.balance("carol"), 500);
        assert_eq!(ledger.nonce("alice"), 2);
    }

    #[test]
    fn the_same_transaction_cannot_be_applied_twice() {
        let mut ledger = ledger();
        let tx = Transaction::new(1, "alice", 0).pay("bob", 100).with_fee(10);
        let receipts = execute_serial(&mut ledger, &[tx.clone(), tx]);
        assert_eq!(
            statuses(&receipts),
            [&Status::Success, &Status::Rejected("wrong nonce".into())]
        );
        // Alice paid once.
        assert_eq!(ledger.balance("alice"), 890);
    }

    #[test]
    fn a_failed_transaction_uses_its_nonce_so_the_next_one_can_run() {
        let mut ledger = ledger();
        let txs = [
            Transaction::new(1, "alice", 0)
                .pay("bob", 9999)
                .with_fee(10),
            Transaction::new(2, "alice", 1).pay("bob", 5).with_fee(10),
        ];
        let receipts = execute_serial(&mut ledger, &txs);
        assert!(matches!(receipts[0].status, Status::Failed(_)));
        assert_eq!(receipts[1].status, Status::Success);
        assert_eq!((ledger.balance("alice"), ledger.balance("bob")), (975, 55));
    }

    #[test]
    fn a_rejected_transaction_leaves_the_ledger_exactly_as_it_was() {
        let mut ledger = ledger();
        let out_of_turn = Transaction::new(1, "alice", 7).pay("bob", 100).with_fee(10);
        execute_serial(&mut ledger, &[out_of_turn]);
        assert_eq!(ledger, self::ledger());
    }

    /// Sixteen senders, each paying a payee nobody else touches.
    fn independent() -> (Ledger, Vec<Transaction>) {
        let names: Vec<String> = (0..16).map(|index| format!("sender{index}")).collect();
        let ledger = Ledger::with_balances(names.iter().map(|name| (name.as_str(), 1000)));
        let txs = names.iter().enumerate().map(|(index, name)| {
            Transaction::new(index as u64, name, 0)
                .pay(&format!("payee{index}"), 10 + index as u64)
                .with_fee(1)
        });
        (ledger, txs.collect())
    }

    #[test]
    fn a_batch_run_together_matches_running_it_one_by_one() {
        let (start, txs) = independent();
        let mut serial = start.clone();
        let expected = execute_serial(&mut serial, &txs);
        for threads in [1, 2, 4, 16, 64] {
            let mut parallel = start.clone();
            let receipts = execute_batches(&mut parallel, &[txs.clone()], threads);
            assert_eq!(parallel, serial, "{threads} threads");
            assert_eq!(receipts, expected, "{threads} threads");
        }
    }

    #[test]
    fn the_order_of_writing_within_a_batch_does_not_matter() {
        let (start, txs) = independent();
        let effects: Vec<Effect> = txs.iter().map(|tx| run(tx, &start)).collect();
        let (mut forwards, mut backwards) = (start.clone(), start.clone());
        effects
            .iter()
            .for_each(|effect| apply(&mut forwards, effect));
        effects
            .iter()
            .rev()
            .for_each(|effect| apply(&mut backwards, effect));
        assert_eq!(forwards, backwards);
    }

    #[test]
    fn batches_run_in_order_so_a_later_one_sees_the_earlier_ones_result() {
        let mut ledger = ledger();
        let first = vec![Transaction::new(1, "alice", 0).pay("bob", 500).with_fee(1)];
        // Bob can only afford this after the first batch has paid him.
        let second = vec![Transaction::new(2, "bob", 0).pay("carol", 400).with_fee(1)];
        let receipts = execute_batches(&mut ledger, &[first, second], 4);
        assert!(
            receipts
                .iter()
                .all(|receipt| receipt.status == Status::Success)
        );
        assert_eq!(ledger.balance("carol"), 400);
    }

    #[test]
    fn nothing_to_run_is_not_a_problem() {
        let mut ledger = ledger();
        assert!(execute_batches(&mut ledger, &[], 4).is_empty());
        assert!(execute_batches(&mut ledger, &[Vec::new()], 0).is_empty());
        assert_eq!(ledger, self::ledger());
    }
}
