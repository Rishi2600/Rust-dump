use core::arch::asm;

pub struct PageFaultError(pub u64);

impl PageFaultError {
    pub fn present(&self) -> bool {
        self.0 & 1 != 0
    }

    pub fn write(&self) -> bool {
        self.0 & (1 << 1) != 0
    }

    pub fn user(&self) -> bool {
        self.0 & (1 << 2) != 0
    }
}

pub unsafe fn faulting_address() -> u64 {
    let addr: u64;
    asm!("mov {}, cr2", out(reg) addr, options(nomem, nostack, preserves_flags));
    addr
}