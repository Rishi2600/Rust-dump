use crate::error::{SchedError, SchedResult};

/// Compute units a transaction asks for when it does not say.
pub const DEFAULT_UNITS: u64 = 100;
/// The most compute units one transaction may ask for.
pub const MAX_UNITS: u64 = 1_000_000;
/// The most transfers one transaction may carry.
pub const MAX_TRANSFERS: usize = 8;
/// The most accounts one transaction may declare it only reads.
pub const MAX_READS: usize = 8;

/// One payment inside a transaction: from its sender to `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    pub to: String,
    pub amount: u64,
}

/// A request to change the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub id: u64,
    /// Who signed it. The sender pays the fee and every transfer.
    pub sender: String,
    /// Its place in the sender's own sequence: 0, then 1, then 2.
    pub nonce: u64,
    /// What the sender offers to have it included, whether or not it succeeds.
    pub fee: u64,
    /// How much work it is expected to take. This is its size in a block:
    /// blocks are limited by total units, not by a count of transactions.
    pub units: u64,
    pub transfers: Vec<Transfer>,
    /// Accounts it looks at but does not change.
    pub reads: Vec<String>,
}

impl Transaction {
    /// A transaction with no fee and nothing in it yet. The methods below
    /// fill it in.
    pub fn new(id: u64, sender: &str, nonce: u64) -> Self {
        Self {
            id,
            sender: sender.to_string(),
            nonce,
            fee: 0,
            units: DEFAULT_UNITS,
            transfers: Vec::new(),
            reads: Vec::new(),
        }
    }

    /// Add a payment from the sender to `to`.
    pub fn pay(mut self, to: &str, amount: u64) -> Self {
        self.transfers.push(Transfer {
            to: to.to_string(),
            amount,
        });
        self
    }

    pub fn with_fee(mut self, fee: u64) -> Self {
        self.fee = fee;
        self
    }

    pub fn with_units(mut self, units: u64) -> Self {
        self.units = units;
        self
    }

    /// Declare an account that is read but not changed.
    pub fn reading(mut self, account: &str) -> Self {
        self.reads.push(account.to_string());
        self
    }

    /// Everything the sender parts with if the transaction succeeds: the fee
    /// plus every transfer. `None` if that does not fit in a number.
    pub fn spend(&self) -> Option<u64> {
        let amounts = self.transfers.iter().map(|transfer| transfer.amount);
        amounts.fold(Some(self.fee), |sum, amount| sum?.checked_add(amount))
    }
}

/// Whether `text` can be an account name: letters, digits, `_` and `-`.
pub fn is_name(text: &str) -> bool {
    let allowed = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-';
    !text.is_empty() && text.len() <= 32 && text.bytes().all(allowed)
}

impl Transaction {
    /// Check that the transaction makes sense on its own, before anything is
    /// known about balances or what else is waiting.
    ///
    /// These checks cost almost nothing and need no state, so they come
    /// first. Every limit here is also a bound on how much work one
    /// transaction can cause later.
    pub fn validate(&self) -> SchedResult<()> {
        let invalid = |reason: String| {
            Err(SchedError::Invalid(format!(
                "transaction {}: {reason}",
                self.id
            )))
        };
        if !is_name(&self.sender) {
            return invalid(format!("`{}` is not an account name", self.sender));
        }
        if self.units == 0 || self.units > MAX_UNITS {
            return invalid(format!("units must be from 1 to {MAX_UNITS}"));
        }
        if self.transfers.is_empty() {
            return invalid("it transfers nothing".into());
        }
        if self.transfers.len() > MAX_TRANSFERS {
            return invalid(format!("more than {MAX_TRANSFERS} transfers"));
        }
        for transfer in &self.transfers {
            if !is_name(&transfer.to) {
                return invalid(format!("`{}` is not an account name", transfer.to));
            }
            if transfer.to == self.sender {
                return invalid("it pays its own sender".into());
            }
            if transfer.amount == 0 {
                return invalid("it transfers an amount of zero".into());
            }
        }
        if self.reads.len() > MAX_READS {
            return invalid(format!("more than {MAX_READS} accounts read"));
        }
        if let Some(bad) = self.reads.iter().find(|account| !is_name(account)) {
            return invalid(format!("`{bad}` is not an account name"));
        }
        if self.spend().is_none() {
            return invalid("its fee and transfers add up to more than can exist".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transaction_is_built_up_piece_by_piece() {
        let tx = Transaction::new(7, "alice", 3)
            .pay("bob", 100)
            .pay("carol", 25)
            .with_fee(10)
            .with_units(400)
            .reading("oracle");
        assert_eq!((tx.id, tx.sender.as_str(), tx.nonce), (7, "alice", 3));
        assert_eq!((tx.fee, tx.units), (10, 400));
        assert_eq!(tx.transfers.len(), 2);
        assert_eq!(
            tx.transfers[1],
            Transfer {
                to: "carol".into(),
                amount: 25
            }
        );
        assert_eq!(tx.reads, ["oracle"]);
    }

    #[test]
    fn spend_is_the_fee_plus_every_transfer() {
        let tx = Transaction::new(1, "alice", 0)
            .pay("bob", 100)
            .pay("carol", 25)
            .with_fee(10);
        assert_eq!(tx.spend(), Some(135));
        assert_eq!(Transaction::new(2, "alice", 1).with_fee(5).spend(), Some(5));
        let absurd = Transaction::new(3, "alice", 2)
            .pay("bob", u64::MAX)
            .with_fee(1);
        assert_eq!(absurd.spend(), None);
    }

    #[test]
    fn units_default_to_a_modest_amount() {
        assert_eq!(Transaction::new(1, "alice", 0).units, DEFAULT_UNITS);
    }

    fn invalid(tx: Transaction) -> bool {
        matches!(tx.validate(), Err(SchedError::Invalid(_)))
    }

    fn good() -> Transaction {
        Transaction::new(1, "alice", 0).pay("bob", 10).with_fee(1)
    }

    #[test]
    fn a_well_formed_transaction_passes() {
        assert_eq!(good().validate(), Ok(()));
        assert_eq!(
            good().reading("oracle").with_units(MAX_UNITS).validate(),
            Ok(())
        );
    }

    #[test]
    fn malformed_transactions_are_refused_with_a_reason() {
        assert!(
            invalid(Transaction::new(1, "alice", 0).with_fee(1)),
            "transfers nothing"
        );
        assert!(invalid(good().with_units(0)));
        assert!(invalid(good().with_units(MAX_UNITS + 1)));
        assert!(invalid(good().pay("alice", 5)), "pays itself");
        assert!(invalid(good().pay("carol", 0)), "zero amount");
        assert!(invalid(good().pay("carol", u64::MAX)), "overflowing total");
        assert!(invalid(Transaction::new(1, "", 0).pay("bob", 1)));
        assert!(invalid(Transaction::new(1, "al ice", 0).pay("bob", 1)));
        assert!(invalid(good().pay("bo/b", 1)));
        assert!(invalid(good().reading("not a name")));

        let message = good().with_units(0).validate().unwrap_err().to_string();
        assert_eq!(
            message,
            "invalid: transaction 1: units must be from 1 to 1000000"
        );
    }

    #[test]
    fn the_number_of_accounts_a_transaction_touches_is_bounded() {
        let mut many_transfers = good();
        let mut many_reads = good();
        for index in 0..MAX_TRANSFERS {
            many_transfers = many_transfers.pay(&format!("payee{index}"), 1);
            many_reads = many_reads.reading(&format!("feed{index}"));
        }
        assert!(invalid(many_transfers));
        assert_eq!(many_reads.clone().validate(), Ok(()));
        assert!(invalid(many_reads.reading("one-too-many")));
    }

    #[test]
    fn account_names_are_short_plain_words() {
        for name in ["alice", "pool_7", "fee-vault", "A1"] {
            assert!(is_name(name), "{name}");
        }
        let too_long = "n".repeat(33);
        for name in ["", "two words", "é", "a.b", too_long.as_str()] {
            assert!(!is_name(name), "{name:?}");
        }
    }
}
