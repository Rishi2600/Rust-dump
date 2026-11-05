macro_rules! impl_bitflags {
    ($struct_name:ident) => {
        impl std::ops::BitOr for $struct_name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
        }
    };
}