macro_rules! match_range {
    ($val:expr, $( $start:expr ..= $end:expr => $handler:expr ),* $(,)? ) => {
        match $val {
            $( x if x >= $start && x <= $end => $handler, )*
            _ => panic!("Value out of range"),
        }
    };
}