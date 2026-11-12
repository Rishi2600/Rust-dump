use core::ptr::{read_volatile, write_volatile};

pub struct SerialPort {
    base_addr: usize,
}

impl SerialPort {
    pub unsafe fn write_data(&mut self, data: u8) {
        let data_reg = self.base_addr as *mut u8;
        write_volatile(data_reg, data);
    }

    pub unsafe fn read_data(&self) -> u8 {
        let data_reg = self.base_addr as *const u8;
        read_volatile(data_reg)
    }
}