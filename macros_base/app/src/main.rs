pub fn page_table_indices(virt: u64) -> [usize; 4] {
    [
        ((virt >> 39) & 0x1FF) as usize, // PML4
        ((virt >> 30) & 0x1FF) as usize, // PDPT
        ((virt >> 21) & 0x1FF) as usize, // PD
        ((virt >> 12) & 0x1FF) as usize, // PT
    ]
}