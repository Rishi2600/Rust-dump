use std::ptr;

#[repr(transparent)]
pub struct VolatileReg<T> {
    value: T,
}

impl<T> VolatileReg<T> {
    pub unsafe fn read(&self) -> T
    where
        T: Copy,
    {
        ptr::read_volatile(&self.value)
    }

    pub unsafe fn write(&mut self, val: T)
    where
        T: Copy,
    {
        ptr::write_volatile(&mut self.value, val);
    }
}