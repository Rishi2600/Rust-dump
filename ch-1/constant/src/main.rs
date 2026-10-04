use std::sync::Arc;
use std::thread;

fn main() {
    // Arc allows safe sharing across multiple threads
    let shared_data = Arc::new(vec![1, 2, 3]);

    for _ in 0..3 {
        let data_clone = Arc::clone(&shared_data);
        thread::spawn(move || {
            println!("Read data: {:?}", data_clone);
        });
    }
}