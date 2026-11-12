#[repr(C)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

pub extern "C" fn default_interrupt_handler(frame: &InterruptStackFrame) {
    // Handle hardware interrupt or exception
}