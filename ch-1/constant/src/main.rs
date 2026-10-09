macro_rules! create_flags {
    ($name:ident { $($flag:ident = $val:expr),* $(,)? }) => {
        pub struct $name(u8);
        impl $name {
            $(
                pub const $flag: u8 = $val;
            )*

            pub fn new() -> Self { $name(0) }
            pub fn set(&mut self, flag: u8) { self.0 |= flag; }
            pub fn is_set(&self, flag: u8) -> bool { (self.0 & flag) != 0 }
        }
    };
}

create_flags!(Permissions {
    READ = 0b0001,
    WRITE = 0b0010,
    EXECUTE = 0b0100,
});