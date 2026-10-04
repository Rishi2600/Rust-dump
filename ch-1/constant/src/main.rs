use std::num::ParseIntError;

fn multiply_str(val: &str, factor: i32) -> Result<i32, ParseIntError> {
    let number = val.parse::<i32>()?; // '?' returns early if it's an Err
    Ok(number * factor)
}

fn main() {
    match multiply_str("10", 3) {
        Ok(res) => println!("Result: {}", res),
        Err(e) => println!("Failed to parse: {}", e),
    }
}