use core::arch::x86_64::__cpuid;

pub fn has_apic() -> bool {
    let leaf = unsafe { __cpuid(1) };
    leaf.edx & (1 << 9) != 0
}