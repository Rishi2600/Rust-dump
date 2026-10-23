use std::cell::RefCell;

thread_local! {
    static TRANSACTION_CONTEXT: RefCell<Option<String>> = RefCell::new(None);
}