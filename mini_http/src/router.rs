use crate::method::Method;
use crate::request::Request;
use crate::response::Response;
use crate::service::Service;
use crate::status::StatusCode;

/// Values a route's pattern captured from the path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Params {
    values: Vec<(String, String)>,
}

impl Params {
    pub fn get(&self, name: &str) -> Option<&str> {
        let found = self.values.iter().find(|(key, _)| key == name);
        found.map(|(_, value)| value.as_str())
    }
}

/// The code that answers requests for one route.
pub type Handler = Box<dyn Fn(&Request, &Params) -> Response + Send + Sync>;

struct Route {
    method: Method,
    pattern: String,
    handler: Handler,
}

impl Route {
    /// Whether this route's pattern fits `path`, and what it captured.
    fn matches(&self, path: &str) -> Option<Params> {
        (self.pattern == path).then(Params::default)
    }
}

/// Decides which handler answers a request, by its method and path.
#[derive(Default)]
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `handler` for requests with this method and path pattern.
    /// Routes are tried in the order they were added.
    pub fn add(
        mut self,
        method: Method,
        pattern: &str,
        handler: impl Fn(&Request, &Params) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.routes.push(Route {
            method,
            pattern: pattern.to_string(),
            handler: Box::new(handler),
        });
        self
    }

    pub fn get(
        self,
        pattern: &str,
        handler: impl Fn(&Request, &Params) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.add(Method::Get, pattern, handler)
    }

    pub fn post(
        self,
        pattern: &str,
        handler: impl Fn(&Request, &Params) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.add(Method::Post, pattern, handler)
    }

    /// Find the route for `request` and run its handler.
    pub fn respond(&self, request: &Request) -> Response {
        for route in &self.routes {
            if route.method != request.method {
                continue;
            }
            if let Some(params) = route.matches(request.path()) {
                return (route.handler)(request, &params);
            }
        }
        Response::error(StatusCode::NOT_FOUND)
    }
}

impl Service for Router {
    fn call(&self, request: &Request) -> Response {
        self.respond(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::parse_request;

    fn request(method: &str, path: &str) -> Request {
        let raw = format!("{method} {path} HTTP/1.1\r\nHost: x\r\n\r\n");
        parse_request(raw.as_bytes()).unwrap()
    }

    fn text(response: &Response) -> String {
        String::from_utf8(response.bytes().unwrap().to_vec()).unwrap()
    }

    fn fixed(body: &'static str) -> impl Fn(&Request, &Params) -> Response + Send + Sync {
        move |_, _| Response::text(StatusCode::OK, body)
    }

    #[test]
    fn a_request_goes_to_the_route_with_its_method_and_path() {
        let router = Router::new()
            .get("/", fixed("home"))
            .get("/about", fixed("about"))
            .post("/about", fixed("posted"));
        assert_eq!(text(&router.respond(&request("GET", "/"))), "home");
        assert_eq!(text(&router.respond(&request("GET", "/about"))), "about");
        assert_eq!(text(&router.respond(&request("POST", "/about"))), "posted");
        // The query string is not part of the path.
        assert_eq!(
            text(&router.respond(&request("GET", "/about?x=1"))),
            "about"
        );
    }

    #[test]
    fn a_path_nothing_claims_is_not_found() {
        let router = Router::new().get("/about", fixed("about"));
        let missing = router.respond(&request("GET", "/nope"));
        assert_eq!(missing.status, StatusCode::NOT_FOUND);
        // Paths are compared exactly: a trailing slash is a different path.
        assert_eq!(
            router.respond(&request("GET", "/about/")).status,
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn a_router_is_itself_a_service() {
        let router = Router::new().get("/", fixed("home"));
        let service: &dyn Service = &router;
        assert_eq!(text(&service.call(&request("GET", "/"))), "home");
    }
}
