pub unsafe fn scan_rsdp(mut addr: usize, end: usize) -> Option<*const u8> {
    while addr < end {
        let signature = *(addr as *const [u8; 8]);
        if &signature == b"RSD PTR " {
            return Some(addr as *const u8);
        }
        addr += 16;
    }
    None
}