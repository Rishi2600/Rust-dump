macro_rules! bitfield {
    ($name:ident { $($field:ident : $shift:expr),* }) => {
        pub struct $name(pub u32);
        impl $name {
            $(
                pub fn $field(&self) -> u32 { (self.0 >> $shift) & 0x1 }
            )*
        }
    };
}
bitfield!(StatusFlags { ready: 0, error: 1 });