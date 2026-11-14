#[repr(C)]
pub struct Multiboot2Tag {
    pub typ: u32,
    pub size: u32,
}

pub unsafe fn find_tag(info_addr: usize, wanted: u32) -> Option<*const Multiboot2Tag> {
    let mut tag = (info_addr + 8) as *const Multiboot2Tag;
    while (*tag).typ != 0 {
        if (*tag).typ == wanted {
            return Some(tag);
        }
        let next = (tag as usize + (*tag).size as usize + 7) & !7;
        tag = next as *const Multiboot2Tag;
    }
    None
}