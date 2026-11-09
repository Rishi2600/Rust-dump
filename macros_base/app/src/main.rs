macro_rules! build_config {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        pub struct $name {
            pub $($field: $ty),*
        }
    };
}
build_config!(ServerConfig { host: String, port: u16 });