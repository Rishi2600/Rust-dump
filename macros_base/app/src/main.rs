macro_rules! assert_with_context {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond {
            panic!("Assertion failed: {}\nContext: {}", stringify!($cond), format!($($arg)*));
        }
    };
}