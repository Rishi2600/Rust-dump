pub struct SlabHeader {
    pub next_slab: *mut SlabHeader,
    pub free_list: *mut u8,
    pub in_use: u16,
    pub capacity: u16,
}

pub struct SlabCache {
    pub object_size: usize,
    pub partial: *mut SlabHeader,
    pub full: *mut SlabHeader,
}