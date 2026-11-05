macro_rules! vec_with_capacity {
    ($cap:expr, $( $x:expr ),* $(,)?) => {
        {
            let mut v = Vec::with_capacity($cap);
            $( v.push($x); )*
            v
        }
    };
}