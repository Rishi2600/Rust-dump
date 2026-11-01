use std::ptr;

#[repr(transparent)]
pub struct VolatileReg<T> {
    val: T,
}

impl<T: Copy> VolatileReg<T> {
    pub unsafe fn read(&self) -> T { ptr::read_volatile(&self.val) }
    pub unsafe fn write(&mut self, val: T) { ptr::write_volatile(&mut self.val, val); }
}