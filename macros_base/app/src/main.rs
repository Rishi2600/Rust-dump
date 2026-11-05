macro_rules! calculate {
    ($val:expr) => { $val };
    ($val:expr + $($rest:tt)*) => { $val + calculate!($($rest)*) };
    ($val:expr * $($rest:tt)*) => { $val * calculate!($($rest)*) };
}