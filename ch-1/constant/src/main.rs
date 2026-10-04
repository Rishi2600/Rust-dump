fn main() {
    let mut data = String::from("Rust");

    // Creating one mutable reference
    let ref1 = &mut data;
    ref1.push_str(" Language");

    // let ref2 = &mut data; // Error! Cannot borrow `data` as mutable more than once at a time
    
    println!("{}", ref1);
}