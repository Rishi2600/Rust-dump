macro_rules! impl_getters_setters {
    ($struct_name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        impl $struct_name {
            $(
                pub fn $field(&self) -> &$ty { &self.$field }
                pub fn set_$field(&mut self, val: $ty) { self.$field = val; }
            )*
        }
    };
}