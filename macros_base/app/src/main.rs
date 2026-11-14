const HPET_MAIN_COUNTER: usize = 0xF0;

pub unsafe fn hpet_read_counter(hpet_base: usize) -> u64 {
    ((hpet_base + HPET_MAIN_COUNTER) as *const u64).read_volatile()
}