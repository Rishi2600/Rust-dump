pub fn process_data<F>(callback: F)
where
    F: for<'a> Fn(&'a str) -> usize,
{
    let local_data = String::from("runtime generated value");
    let len = callback(&local_data);
    println!("Callback computed length: {}", len);
}

fn main() {
    process_data(|s| s.len());
}