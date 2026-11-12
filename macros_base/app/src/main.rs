pub unsafe fn dump_stack_trace() {
    let mut rbp: usize;
    core::arch::asm!("mov {}, rbp", out(reg) rbp, options(nostack, preserves_flags));

    while rbp != 0 {
        let return_addr = *((rbp + 8) as *const usize);
        // Print return_addr to serial console
        rbp = *(rbp as *const usize);
    }
}

fn main() {
    unsafe {
        dump_stack_trace();
    }
}