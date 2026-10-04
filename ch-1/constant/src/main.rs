macro_rules! create_vector {
    ( $($x:expr ),* ) => {
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
    let my_vec = create_vector!(10, 20, 30);
    println!("{:?}", my_vec);
}