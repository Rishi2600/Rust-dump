#[repr(C)]
pub struct VirtIoMmioHeader {
    pub magic_value: u32,
    pub version: u32,
    pub device_id: u32,
    pub vendor_id: u32,
    pub device_features: u32,
    pub driver_features: u32,
    pub queue_sel: u32,
    pub queue_num_max: u32,
    pub queue_num: u32,
    pub queue_ready: u32,
}