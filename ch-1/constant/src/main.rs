use std::marker::PhantomPinned;
use std::pin::Pin;

pub struct SelfRef {
    data: String,
    _pin: PhantomPinned,
}