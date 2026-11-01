use std::marker::PhantomPinned;
use std::pin::Pin;

pub struct SelfRef {
    data: String,
    ptr: *const String,
    _pin: PhantomPinned,
}

impl SelfRef {
    pub fn new(text: &str) -> Pin<Box<Self>> {
        let mut boxed = Box::pin(SelfRef { data: text.to_string(), ptr: std::ptr::null(), _pin: PhantomPinned });
        let p = &boxed.data as *const String;
        unsafe { Pin::as_mut(&mut boxed).ptr = p; }
        boxed
    }
}