use std::cell::RefCell;

thread_local! {
    static SESSION_ID: RefCell<u64> = RefCell::new(0);
}

pub fn set_session(id: u64) {
    SESSION_ID.with(|s| *s.borrow_mut() = id);
}