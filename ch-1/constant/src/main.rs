pub trait Middleware {
    fn process<'a>(&self, input: &'a str) -> &'a str;
}

pub struct Pipeline {
    handlers: Vec<Box<dyn Middleware>>,
}