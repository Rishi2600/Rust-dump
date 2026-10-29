use std::marker::PhantomData;

pub struct Meter;
pub struct Second;

pub struct Value<Unit> {
    pub inner: f64,
    _marker: PhantomData<Unit>,
}