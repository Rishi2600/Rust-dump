macro_rules! mock_repo {
    ($trait_name:ident, $mock_name:ident, { $($fn_name:ident -> $ret:ty),* }) => {
        pub struct $mock_name;
        impl $trait_name for $mock_name {
            $(
                fn $fn_name(&self) -> $ret {
                    unimplemented!(stringify!($fn_name mocked))
                }
            )*
        }
    };
}