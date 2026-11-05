macro_rules! munch_tokens {
    // Base case
    () => {};
    // Recursive case: consume first token and process the rest
    ($head:expr, $($tail:tt)*) => {
        println!("Munched token: {}", $head);
        munch_tokens!($($tail)*);
    };
}