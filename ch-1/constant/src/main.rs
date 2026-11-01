pub trait Middleware {
    fn process<'a>(&self, input: &'a str) -> &'a str;
}

pub struct Engine {
    handlers: Vec<Box<dyn Middleware>>,
}