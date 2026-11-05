macro_rules! state_machine {
    ($curr:expr => { $($from:ident => $to:ident),* $(,)? }) => {
        match $curr {
            $( (stringify!($from), stringify!($to)) => true, )*
            _ => false,
        }
    };
}