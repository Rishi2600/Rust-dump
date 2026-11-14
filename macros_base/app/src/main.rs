const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;

pub unsafe fn syscall_init(entry: u64) {
    wrmsr(IA32_EFER, rdmsr(IA32_EFER) | 1); // SCE: enable syscall/sysret
    wrmsr(IA32_STAR, (0x10 << 48) | (0x08 << 32)); // User base / kernel CS
    wrmsr(IA32_LSTAR, entry);
    wrmsr(IA32_FMASK, 1 << 9); // Mask IF on entry
}