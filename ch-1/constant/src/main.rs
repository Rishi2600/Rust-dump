fn main() {
    let mut num = 5;

    // Creating raw pointers (safe)
    let r1 = &num as *const i32;
    let r2 = &mut num as *mut i32;

    // Dereferencing raw pointers requires an `unsafe` block
    unsafe {
        println!("r1 is: {}", *r1);
        *r2 = 10;
        println!("r2 is: {}", *r2);
    }
}