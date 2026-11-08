macro_rules! define_bitfield {
    ($name:ident { $($field:ident : $shift:expr),* $(,)? }) => {
        pub struct $name(pub u32);
        impl $name {
            $(
                pub fn $field(&self) -> bool {
                    ((self.0 >> $shift) & 1) == 1
                }
            )*
        }
    };
}
define_bitfield!(PacketFlags { active: 0, error: 1 });