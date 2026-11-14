const STACK_CANARY: u64 = 0xDEAD_C0DE_CAFE_BABE;

pub unsafe fn stack_guard_init(stack_bottom: *mut u64) {
    stack_bottom.write(STACK_CANARY);
}

pub unsafe fn stack_overflowed(stack_bottom: *const u64) -> bool {
    stack_bottom.read() != STACK_CANARY
}