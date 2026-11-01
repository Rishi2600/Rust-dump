use std::marker::PhantomData;
use std::ops::Add;

pub struct Meters;
pub struct Seconds;

pub struct Quantity<Unit>(f64, PhantomData<Unit>);

impl<Unit> Quantity<Unit> {
    pub fn new(v: f64) -> Self { Self(v, PhantomData) }
}

impl<Unit> Add for Quantity<Unit> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output { Quantity(self.0 + rhs.0, PhantomData) }
}