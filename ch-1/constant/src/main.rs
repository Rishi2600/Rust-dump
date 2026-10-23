macro_rules! pipeline {
    ( $val:expr => $( $fn:ident ),* ) => {
        {
            let mut temp = $val;
            $(
                temp = $fn(temp);
            )*
            temp
        }
    };
}