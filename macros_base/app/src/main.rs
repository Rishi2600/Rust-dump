macro_rules! dispatch_commands {
    ($cmd:expr, { $( $name:pat => $handler:expr ),* $(,)? }) => {
        match $cmd.as_str() {
            $( $name => $handler(), )*
            _ => println!("Unknown command"),
        }
    };
}