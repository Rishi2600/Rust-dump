macro_rules! html {
    ($tag:ident [ $($child:tt)* ]) => {
        format!("<{0}>{1}</{0}>", stringify!($tag), html!(@inner $($child)*))
    };
    (@inner $text:expr) => { $text };
}