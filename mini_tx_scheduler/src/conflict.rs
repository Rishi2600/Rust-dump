use crate::tx::Transaction;

/// Whether two transactions must not run at the same time.
///
/// They clash if one changes an account the other reads or changes. Two
/// transactions that only *read* the same account do not clash: neither can
/// see any effect of the other, so their order does not matter.
///
/// | first  | second | clash? |
/// |--------|--------|--------|
/// | read   | read   | no     |
/// | read   | write  | yes    |
/// | write  | read   | yes    |
/// | write  | write  | yes    |
pub fn conflicts(first: &Transaction, second: &Transaction) -> bool {
    let (first_writes, second_writes) = (first.writes(), second.writes());
    let (first_reads, second_reads) = (first.read_only(), second.read_only());
    let both_write = first_writes.intersection(&second_writes).next().is_some();
    let first_writes_what_second_reads = first_writes.intersection(&second_reads).next().is_some();
    let second_writes_what_first_reads = second_writes.intersection(&first_reads).next().is_some();
    both_write || first_writes_what_second_reads || second_writes_what_first_reads
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payment(id: u64, from: &str, to: &str) -> Transaction {
        Transaction::new(id, from, 0).pay(to, 1).with_fee(1)
    }

    #[test]
    fn transactions_on_unrelated_accounts_do_not_clash() {
        let first = payment(1, "alice", "bob");
        let second = payment(2, "carol", "dave");
        assert!(!conflicts(&first, &second));
        assert!(!conflicts(&second, &first));
    }

    #[test]
    fn two_writers_of_one_account_clash() {
        // Both pay bob: each would change his balance.
        assert!(conflicts(
            &payment(1, "alice", "bob"),
            &payment(2, "carol", "bob")
        ));
        // One's payee is the other's sender.
        assert!(conflicts(
            &payment(1, "alice", "bob"),
            &payment(2, "bob", "carol")
        ));
        // The same sender twice.
        assert!(conflicts(
            &payment(1, "alice", "bob"),
            &payment(2, "alice", "carol")
        ));
    }

    #[test]
    fn a_reader_clashes_with_a_writer_in_either_order() {
        let writer = payment(1, "alice", "oracle");
        let reader = payment(2, "carol", "dave").reading("oracle");
        assert!(conflicts(&writer, &reader));
        assert!(conflicts(&reader, &writer));
    }

    #[test]
    fn two_readers_of_one_account_do_not_clash() {
        let first = payment(1, "alice", "bob").reading("oracle");
        let second = payment(2, "carol", "dave").reading("oracle");
        assert!(!conflicts(&first, &second));
    }

    #[test]
    fn a_transaction_clashes_with_itself() {
        let tx = payment(1, "alice", "bob");
        assert!(conflicts(&tx, &tx));
    }
}
