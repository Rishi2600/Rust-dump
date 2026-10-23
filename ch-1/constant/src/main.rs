use std::marker::PhantomData;

pub struct Quantity<const M: i32, const KG: i32, const S: i32> {
    value: f64,
    _marker: PhantomData<()>,
}

impl<const M: i32, const KG: i32, const S: i32> Quantity<M, KG, S> {
    pub fn new(val: f64) -> Self {
        Self { value: val, _marker: PhantomData }
    }
}