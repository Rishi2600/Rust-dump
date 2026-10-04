fn main() {
    let numbers = vec![1, 2, 3, 4, 5];

    // Iterator with closure to double even numbers
    let processed: Vec<i32> = numbers
        .into_iter()
        .filter(|&x| x % 2 == 0)
        .map(|x| x * 2)
        .collect();

    println!("{:?}", processed); // Output: [4, 8]
}