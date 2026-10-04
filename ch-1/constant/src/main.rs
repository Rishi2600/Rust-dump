struct Seconds(u32);

struct Minutes(u32);

impl From<Minutes> for Seconds {
    fn from(m: Minutes) -> Self {
        Seconds(m.0 * 60)
    }
}

fn main() {
    let mins = Minutes(5);
    let secs: Seconds = mins.into(); // Using Into automatically
    println!("Seconds: {}", secs.0);
}