trait Container {
    type Item; // Associated type
    fn add(&mut self, item: Self::Item);
}

struct IntStack(Vec<i32>);

impl Container for IntStack {
    type Item = i32;
    fn add(&mut self, item: Self::Item) {
        self.0.push(item);
    }
}