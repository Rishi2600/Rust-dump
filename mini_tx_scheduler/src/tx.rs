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
}
