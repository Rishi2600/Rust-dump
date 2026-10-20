use std::future::Future;
use std::pin::Pin;

pub struct Unauthenticated;
pub struct Authenticated { token: String };

pub struct Request<State> {
    pub body: String,
    pub state: State,
}

impl Request<Unauthenticated> {
    pub fn new(body: &str) -> Self {
        Request { body: body.to_string(), state: Unauthenticated }
    }

    pub fn authenticate(self, token: &str) -> Request<Authenticated> {
        Request {
            body: self.body,
            state: Authenticated { token: token.to_string() },
        }
    }
}

pub type AsyncHandler = Box<
    dyn for<'a> Fn(&'a Request<Authenticated>) -> Pin<Box<dyn Future<Output = String> + 'a>>
>;

pub struct RequestPipeline {
    handler: AsyncHandler,
}

impl RequestPipeline {
    pub fn new<F>(handler: F) -> Self
    where
        F: for<'a> Fn(&'a Request<Authenticated>) -> Pin<Box<dyn Future<Output = String> + 'a>> + 'static,
    {
        Self { handler: Box::new(handler) }
    }

    pub async fn execute(&self, req: Request<Authenticated>) -> String {
        (self.handler)(&req).await
    }
}