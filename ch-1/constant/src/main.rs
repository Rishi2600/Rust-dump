pub struct TransactionGuard<F: FnOnce()> {
    rollback: Option<F>,
}