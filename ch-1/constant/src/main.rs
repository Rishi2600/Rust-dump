fn main() {
    let s1 = String::from("hello");
    
    // Ownership moves to s2
    let s2 = s1; 
    // println!("{}", s1); // Error! s1 is no longer valid.

    // Borrowing via a reference (does not take ownership)
    let len = calculate_length(&s2);
    println!("Length of '{}' is {}.", s2, len);
}

fn calculate_length(s: &String) -> usize {
    s.len()
}