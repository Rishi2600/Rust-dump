pub unsafe fn pci_config_read(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let address = 0x8000_0000
        | (bus as u32) << 16
        | (device as u32) << 11
        | (func as u32) << 8
        | (offset as u32 & 0xFC);
    outl(0xCF8, address);
    inl(0xCFC)
}