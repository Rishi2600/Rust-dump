macro_rules! simple_builder {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        pub struct $name { $($field: Option<$ty>,)* }
        impl $name {
            pub fn new() -> Self { Self { $($field: None,)* } }
            $(
                pub fn $field(mut self, val: $ty) -> Self {
                    self.$field = Some(val);
                    self
                }
            )*
        }
    };
}