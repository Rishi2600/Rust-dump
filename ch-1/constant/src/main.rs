use std::cell::RefCell;

thread_local! {
    static CONTEXT: RefCell<Option<String>> = RefCell::new(None);
}