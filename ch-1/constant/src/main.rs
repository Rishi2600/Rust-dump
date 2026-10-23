pub trait Middleware {
    fn handle<'a>(&self, req: &'a str) -> &'a str;
}

pub struct Chain {
    handlers: Vec<Box<dyn Middleware>>,
}