use std::marker::PhantomData;
use std::ops::Add;

#[derive(Debug, Clone, Copy)]
pub struct Meters;
#[derive(Debug, Clone, Copy)]
pub struct Seconds;

#[derive(Debug, Clone, Copy)]
pub struct Quantity<Unit>(f64, PhantomData<Unit>);

impl<Unit> Quantity<Unit> {
    pub fn new(value: f64) -> Self { Quantity(value, PhantomData) }
}

impl<Unit> Add for Quantity<Unit> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Quantity::new(self.0 + rhs.0)
    }
}