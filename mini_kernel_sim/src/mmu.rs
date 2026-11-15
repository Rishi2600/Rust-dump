use crate::cpu::Bus;
use crate::error::KResult;
use crate::memory::PhysicalMemory;
use crate::paging::AddressSpace;

/// Memory management unit: routes a task's virtual addresses through its page
/// table to physical memory.
pub struct Mmu<'a> {
    space: &'a AddressSpace,
    memory: &'a mut PhysicalMemory,
}

impl<'a> Mmu<'a> {
    pub fn new(space: &'a AddressSpace, memory: &'a mut PhysicalMemory) -> Self {
        Self { space, memory }
    }
}

impl Bus for Mmu<'_> {
    fn load(&mut self, addr: usize) -> KResult<i64> {
        let (frame, offset) = self.space.translate(addr)?;
        self.memory.read(frame, offset)
    }

    fn store(&mut self, addr: usize, value: i64) -> KResult<()> {
        let (frame, offset) = self.space.translate(addr)?;
        self.memory.write(frame, offset, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::KernelError;

    #[test]
    fn same_virtual_address_is_isolated_between_address_spaces() {
        let mut memory = PhysicalMemory::new(2);
        let (mut first, mut second) = (AddressSpace::new(1), AddressSpace::new(1));
        first.map(0, 0).unwrap();
        second.map(0, 1).unwrap();

        Mmu::new(&first, &mut memory).store(5, 111).unwrap();
        Mmu::new(&second, &mut memory).store(5, 222).unwrap();

        assert_eq!(Mmu::new(&first, &mut memory).load(5), Ok(111));
        assert_eq!(Mmu::new(&second, &mut memory).load(5), Ok(222));
    }

    #[test]
    fn unmapped_page_raises_a_page_fault() {
        let mut memory = PhysicalMemory::new(1);
        let space = AddressSpace::new(1);
        let fault = Mmu::new(&space, &mut memory).load(7);
        assert_eq!(fault, Err(KernelError::PageFault(7)));
    }
}
