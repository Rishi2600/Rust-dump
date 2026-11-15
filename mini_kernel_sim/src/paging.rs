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
