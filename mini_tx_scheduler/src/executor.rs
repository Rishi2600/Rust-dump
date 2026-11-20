use std::collections::BTreeMap;

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
}
