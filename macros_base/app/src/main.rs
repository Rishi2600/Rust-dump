macro_rules! impl_enum_display {
    ($enum_name:ident { $($variant:ident),* $(,)? }) => {
        impl std::fmt::Display for $enum_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $( $enum_name::$variant => write!(f, stringify!($variant)), )*
                }
            }
        }
    };
}