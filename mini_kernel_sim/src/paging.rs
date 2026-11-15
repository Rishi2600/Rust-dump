use crate::error::{KResult, KernelError};
use crate::frame::PAGE_SIZE;

/// Single-level page table mapping a task's virtual pages to physical frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressSpace {
    table: Vec<Option<usize>>,
}

impl AddressSpace {
    pub fn new(pages: usize) -> Self {
        Self {
            table: vec![None; pages],
        }
    }

    pub fn pages(&self) -> usize {
        self.table.len()
    }

    pub fn map(&mut self, page: usize, frame: usize) -> KResult<()> {
        match self.table.get_mut(page) {
            None => Err(KernelError::SegFault(page * PAGE_SIZE)),
            Some(Some(_)) => Err(KernelError::AlreadyMapped(page)),
            Some(slot) => {
                *slot = Some(frame);
                Ok(())
            }
        }
    }

    /// Remove a mapping, returning the frame it pointed at.
    pub fn unmap(&mut self, page: usize) -> Option<usize> {
        self.table.get_mut(page).and_then(Option::take)
    }

    pub fn mapped_frames(&self) -> Vec<usize> {
        self.table.iter().flatten().copied().collect()
    }
}

impl AddressSpace {
    /// Size of the address space in words; valid addresses are below this.
    pub fn size(&self) -> usize {
        self.table.len() * PAGE_SIZE
    }

    /// Translate a virtual address into `(frame, offset)`.
    ///
    /// An unmapped page inside the address space is a page fault (the kernel
    /// may fix it up); anything beyond the address space is a segmentation fault.
    pub fn translate(&self, addr: usize) -> KResult<(usize, usize)> {
        match self.table.get(addr / PAGE_SIZE) {
            None => Err(KernelError::SegFault(addr)),
            Some(None) => Err(KernelError::PageFault(addr)),
            Some(Some(frame)) => Ok((*frame, addr % PAGE_SIZE)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_splits_address_into_frame_and_offset() {
        let mut space = AddressSpace::new(4);
        space.map(1, 9).unwrap();
        assert_eq!(space.translate(PAGE_SIZE + 5), Ok((9, 5)));
        assert_eq!(space.size(), 4 * PAGE_SIZE);
    }

    #[test]
    fn unmapped_and_out_of_range_addresses_fault_differently() {
        let space = AddressSpace::new(2);
        assert_eq!(space.translate(3), Err(KernelError::PageFault(3)));
        let beyond = 2 * PAGE_SIZE;
        assert_eq!(space.translate(beyond), Err(KernelError::SegFault(beyond)));
    }

    #[test]
    fn map_rejects_double_mapping_and_unmap_returns_frame() {
        let mut space = AddressSpace::new(2);
        space.map(0, 4).unwrap();
        assert_eq!(space.map(0, 5), Err(KernelError::AlreadyMapped(0)));
        assert_eq!(space.map(2, 5), Err(KernelError::SegFault(2 * PAGE_SIZE)));
        assert_eq!(space.mapped_frames(), [4]);
        assert_eq!(space.unmap(0), Some(4));
        assert_eq!(space.unmap(0), None);
    }
}
