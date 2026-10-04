trait Summary {
    fn summarize(&self) -> String;
}

struct Article {
    headline: String,
    author: String,
}

impl Summary for Article {
    fn summarize(&self) -> String {
        format!("'{}' by {}", self.headline, self.author)
    }
}

fn notify(item: &impl Summary) {
    println!("Breaking news: {}", item.summarize());
}