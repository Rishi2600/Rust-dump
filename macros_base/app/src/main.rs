macro_rules! lazy_singleton {
    ($name:ident : $ty:ty = $init:expr) => {
        pub fn $name() -> &'static $ty {
            static INSTANCE: std::sync::OnceLock<$ty> = std::sync::OnceLock::new();
            INSTANCE.get_or_init(|| $init)
        }
    };
}