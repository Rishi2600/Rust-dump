fn main() {
    let mut stack = vec![1, 2, 3];

    // Keep popping as long as pop() returns Some(top)
    while let Some(top) = stack.pop() {
        println!("Popped: {}", top);
    }
}