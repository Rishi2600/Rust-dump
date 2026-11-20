use std::collections::BTreeMap;

use crate::error::{SchedError, SchedResult};

/// Who owns how much.
///
/// This is the state that transactions read and change. Everything the
/// scheduler does exists to change it safely and in a sensible order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    balances: BTreeMap<String, u64>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    /// A ledger with the given starting balances.
    pub fn with_balances<'a>(balances: impl IntoIterator<Item = (&'a str, u64)>) -> Self {
        let mut ledger = Self::new();
        for (account, amount) in balances {
            ledger.set_balance(account, amount);
        }
        ledger
    }

    /// What an account holds. An account never heard of holds nothing.
    pub fn balance(&self, account: &str) -> u64 {
        self.balances.get(account).copied().unwrap_or(0)
    }

    /// Set a balance outright. An empty account is not stored at all, so two
    /// ledgers that mean the same thing are always equal.
    pub fn set_balance(&mut self, account: &str, amount: u64) {
        if amount == 0 {
            self.balances.remove(account);
        } else {
            self.balances.insert(account.to_string(), amount);
        }
    }

    pub fn credit(&mut self, account: &str, amount: u64) -> SchedResult<()> {
        let raised = self.balance(account).checked_add(amount);
        let raised = raised.ok_or_else(|| SchedError::Overflow(account.to_string()))?;
        self.set_balance(account, raised);
        Ok(())
    }

    /// Take `amount` from an account. Money cannot be created by going
    /// below zero: the subtraction is checked, and fails without changing
    /// anything if there is not enough.
    pub fn debit(&mut self, account: &str, amount: u64) -> SchedResult<()> {
        let available = self.balance(account);
        let lowered =
            available
                .checked_sub(amount)
                .ok_or_else(|| SchedError::InsufficientFunds {
                    account: account.to_string(),
                    needed: amount,
                    available,
                })?;
        self.set_balance(account, lowered);
        Ok(())
    }

    /// Everything in existence, summed. Transfers must never change this.
    pub fn total(&self) -> u128 {
        self.balances
            .values()
            .map(|&amount| u128::from(amount))
            .sum()
    }

    /// Accounts and balances, in name order.
    pub fn accounts(&self) -> impl Iterator<Item = (&str, u64)> {
        self.balances
            .iter()
            .map(|(account, &amount)| (account.as_str(), amount))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balances_start_at_zero_and_follow_credits_and_debits() {
        let mut ledger = Ledger::new();
        assert_eq!(ledger.balance("alice"), 0);
        ledger.credit("alice", 100).unwrap();
        ledger.debit("alice", 30).unwrap();
        assert_eq!(ledger.balance("alice"), 70);
        assert_eq!(ledger.total(), 70);
    }

    #[test]
    fn an_account_cannot_spend_more_than_it_has() {
        let mut ledger = Ledger::with_balances([("alice", 50)]);
        let refused = ledger.debit("alice", 51);
        let expected = SchedError::InsufficientFunds {
            account: "alice".into(),
            needed: 51,
            available: 50,
        };
        assert_eq!(refused, Err(expected));
        // A refused debit changes nothing.
        assert_eq!(ledger.balance("alice"), 50);
        assert!(ledger.debit("nobody", 1).is_err());
    }

    #[test]
    fn a_balance_cannot_wrap_around() {
        let mut ledger = Ledger::with_balances([("whale", u64::MAX)]);
        assert_eq!(
            ledger.credit("whale", 1),
            Err(SchedError::Overflow("whale".into()))
        );
        assert_eq!(ledger.balance("whale"), u64::MAX);
    }

    #[test]
    fn empty_accounts_leave_no_trace() {
        let mut ledger = Ledger::with_balances([("alice", 10), ("bob", 5)]);
        ledger.debit("alice", 10).unwrap();
        assert_eq!(ledger, Ledger::with_balances([("bob", 5)]));
        assert_eq!(ledger.accounts().collect::<Vec<_>>(), [("bob", 5)]);
    }
}
