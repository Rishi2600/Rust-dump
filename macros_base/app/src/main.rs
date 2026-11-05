macro_rules! safe_ffi_call {
    ($call:expr) => {
        std::panic::catch_unwind(|| {
            $call
        }).unwrap_or_else(|_| {
            eprintln!("FFI panic caught safely");
            -1
        })
    };
}