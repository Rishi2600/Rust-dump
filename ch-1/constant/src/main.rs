use std::cell::RefCell;

thread_local! {
    static REQUEST_ID: RefCell<u64> = RefCell::new(0);
}

pub fn set_request_id(id: u64) {
    REQUEST_ID.with(|r| *r.borrow_mut() = id);
}