use core::arch::asm;

pub struct IrqGuard {
    was_enabled: bool,
}

impl IrqGuard {
    pub fn new() -> Self {
        let rflags: u64;
        unsafe {
            asm!("pushfq", "pop {}", "cli", out(reg) rflags);
        }
        Self { was_enabled: rflags & (1 << 9) != 0 }
    }
}

impl Drop for IrqGuard {
    fn drop(&mut self) {
        if self.was_enabled {
            unsafe { asm!("sti", options(nomem, nostack)) };
        }
    }
}