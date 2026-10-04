// 'a specifies that the returned reference lives as long as both input references
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() {
        x
    } else {
        y
    }
}

fn main() {
    let str1 = String::from("long string");
    let str2 = "short";
    let result = longest(str1.as_str(), str2);
    println!("The longest string is '{}'", result);
}