macro_rules! quick_vec {
    ( $( $x:expr ),* $(,)? ) => {
        {
            let mut temp_vec = Vec::new();
            $(
                temp_vec.push($x);
            )*
            temp_vec
        }
    };
}

fn main() {
    let numbers = quick_vec![1, 2, 3, 4];
    println!("{:?}", numbers);
}