use std::cell::UnsafeCell;
use std::marker::PhantomPinned;
use std::pin::Pin;

pub struct SelfRefBuffer {
    data: String,
    slice: UnsafeCell<*const str>,
    _pin: PhantomPinned,
}

impl SelfRefBuffer {
    pub fn new(text: &str) -> Pin<Box<Self>> {
        let res = SelfRefBuffer {
            data: text.to_string(),
            slice: UnsafeCell::new(std::ptr::null()),
            _pin: PhantomPinned,
        };
        let mut boxed = Box::pin(res);
        
        let self_ptr: *const str = &boxed.data;
        unsafe {
            let mut_ref = Pin::as_mut(&mut boxed);
            *mut_ref.slice.get() = self_ptr;
        }
        boxed
    }

    pub fn get_borrowed<'a>(self: &'a Pin<Box<Self>>) -> &'a str {
        unsafe { &**self.slice.get() }
    }
}