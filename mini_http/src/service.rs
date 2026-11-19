use crate::request::Request;
use crate::response::Response;

/// Anything that can turn a request into a response.
///
/// The connection code only knows this trait. A plain function, a router, or
/// a wrapper around another service are all the same thing to it.
pub trait Service: Send + Sync {
    fn call(&self, request: &Request) -> Response;
}

impl<F> Service for F
where
    F: Fn(&Request) -> Response + Send + Sync,
{
    fn call(&self, request: &Request) -> Response {
        self(request)
    }
}
