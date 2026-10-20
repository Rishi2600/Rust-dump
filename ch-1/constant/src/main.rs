use std::os::raw::c_void;
use std::panic::catch_unwind;

pub type NativeCallback = extern "C" fn(*const c_void, i32);

pub struct Context {
    pub callback: NativeCallback,
    pub user_data: *const c_void,
}

#[no_mangle]
pub unsafe extern "C" fn execute_rust_task(ctx: *mut Context) -> i32 {
    let result = catch_unwind(|| {
        if ctx.is_null() { return -1; }
        let safe_ctx = &*ctx;
        (safe_ctx.callback)(safe_ctx.user_data, 200);
        0
    });

    result.unwrap_or(-1) // Catch panics gracefully across FFI boundary
}