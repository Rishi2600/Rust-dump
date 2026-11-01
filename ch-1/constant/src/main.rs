use std::cell::RefCell;

thread_local! {
    static REQUEST_CONTEXT: RefCell<String> = RefCell::new(String::new());
}

pub fn set_trace_id(id: &str) {
    REQUEST_CONTEXT.with(|ctx| *ctx.borrow_mut() = id.to_string());
}