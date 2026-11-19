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

/// One piece of a route pattern, between slashes.
enum Segment {
    /// Must appear exactly: the `users` in `/users/:id`.
    Literal(String),
    /// Matches any one segment and remembers it: the `:id`.
    Param(String),
    /// Matches everything that is left, slashes included: `*path`.
    Rest(String),
}

fn parse_pattern(pattern: &str) -> Vec<Segment> {
    let parts = pattern.trim_start_matches('/').split('/');
    let segments = parts.map(|part| {
        if let Some(name) = part.strip_prefix(':') {
            Segment::Param(name.to_string())
        } else if let Some(name) = part.strip_prefix('*') {
            Segment::Rest(name.to_string())
        } else {
            Segment::Literal(part.to_string())
        }
    });
    segments.collect()
}

struct Route {
    method: Method,
    segments: Vec<Segment>,
    handler: Handler,
}

impl Route {
    /// Whether this route's pattern fits `path`, and what it captured.
    fn matches(&self, path: &str) -> Option<Params> {
        let mut params = Params::default();
        let mut parts = path.trim_start_matches('/').split('/');
        for segment in &self.segments {
            match segment {
                Segment::Literal(expected) => {
                    if parts.next()? != expected {
                        return None;
                    }
                }
                Segment::Param(name) => {
                    let part = parts.next().filter(|part| !part.is_empty())?;
                    params.values.push((name.clone(), part.to_string()));
                }
                Segment::Rest(name) => {
                    let rest: Vec<&str> = parts.collect();
                    params.values.push((name.clone(), rest.join("/")));
                    return Some(params);
                }
            }
        }
        // The path must not have segments the pattern did not account for.
        parts.next().is_none().then_some(params)
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
            segments: parse_pattern(pattern),
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
    ///
    /// There are two ways to find nothing, and they mean different things.
    /// If no route knows the path, the answer is 404. If routes know the
    /// path but not with this method, the answer is 405, with an `Allow`
    /// header listing the methods that would work.
    pub fn respond(&self, request: &Request) -> Response {
        // A HEAD request is answered by the GET route. The body the handler
        // produces is dropped when the response is written.
        let wanted = match request.method {
            Method::Head => Method::Get,
            other => other,
        };
        let mut allowed = Vec::new();
        for route in &self.routes {
            let Some(params) = route.matches(request.path()) else {
                continue;
            };
            if route.method == wanted {
                return (route.handler)(request, &params);
            }
            allowed.push(route.method);
        }
        if allowed.is_empty() {
            return Response::error(StatusCode::NOT_FOUND);
        }
        if allowed.contains(&Method::Get) {
            allowed.push(Method::Head);
        }
        allowed.sort();
        allowed.dedup();
        let names: Vec<&str> = allowed.iter().map(|method| method.as_str()).collect();
        Response::error(StatusCode::METHOD_NOT_ALLOWED).header("Allow", &names.join(", "))
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

    #[test]
    fn a_named_segment_captures_part_of_the_path() {
        let router = Router::new().get("/users/:id/posts/:post", |_, params| {
            let (id, post) = (params.get("id").unwrap(), params.get("post").unwrap());
            Response::text(StatusCode::OK, format!("user {id}, post {post}"))
        });
        let found = router.respond(&request("GET", "/users/42/posts/hello-world"));
        assert_eq!(text(&found), "user 42, post hello-world");
        // The captured value arrives already decoded.
        let spaced = router.respond(&request("GET", "/users/a%20b/posts/1"));
        assert_eq!(text(&spaced), "user a b, post 1");

        for path in [
            "/users/42",
            "/users/42/posts",
            "/users/42/posts/1/extra",
            "/users//posts/1",
        ] {
            assert_eq!(
                router.respond(&request("GET", path)).status,
                StatusCode::NOT_FOUND,
                "{path}"
            );
        }
    }

    #[test]
    fn a_wildcard_captures_everything_that_is_left() {
        let router = Router::new().get("/files/*path", |_, params| {
            Response::text(StatusCode::OK, format!("[{}]", params.get("path").unwrap()))
        });
        assert_eq!(
            text(&router.respond(&request("GET", "/files/a.txt"))),
            "[a.txt]"
        );
        assert_eq!(
            text(&router.respond(&request("GET", "/files/css/site/main.css"))),
            "[css/site/main.css]"
        );
        assert_eq!(text(&router.respond(&request("GET", "/files/"))), "[]");
        assert_eq!(
            router.respond(&request("GET", "/other/a.txt")).status,
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn earlier_routes_win_over_later_ones() {
        let router = Router::new()
            .get("/users/me", fixed("that is you"))
            .get("/users/:id", |_, params| {
                Response::text(StatusCode::OK, params.get("id").unwrap())
            })
            .get("/*anything", fixed("fallback"));
        assert_eq!(
            text(&router.respond(&request("GET", "/users/me"))),
            "that is you"
        );
        assert_eq!(text(&router.respond(&request("GET", "/users/7"))), "7");
        assert_eq!(
            text(&router.respond(&request("GET", "/elsewhere/deep"))),
            "fallback"
        );
        assert_eq!(text(&router.respond(&request("GET", "/"))), "fallback");
    }

    #[test]
    fn a_known_path_with_the_wrong_method_is_405_not_404() {
        let router = Router::new()
            .get("/items/:id", fixed("item"))
            .add(Method::Delete, "/items/:id", fixed("deleted"))
            .post("/items", fixed("created"));

        let wrong = router.respond(&request("PUT", "/items/7"));
        assert_eq!(wrong.status, StatusCode::METHOD_NOT_ALLOWED);
        // HEAD is listed because GET is supported.
        assert_eq!(wrong.headers.get("Allow"), Some("GET, HEAD, DELETE"));

        let wrong = router.respond(&request("GET", "/items"));
        assert_eq!(wrong.headers.get("Allow"), Some("POST"));
        assert_eq!(
            router.respond(&request("PUT", "/nowhere")).status,
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn head_is_answered_by_the_get_route() {
        let router = Router::new()
            .get("/page", fixed("content"))
            .post("/form", fixed("ok"));
        let head = router.respond(&request("HEAD", "/page"));
        assert_eq!(head.status, StatusCode::OK);
        assert_eq!(text(&head), "content");
        // There is no GET for /form, so there is no HEAD for it either.
        assert_eq!(
            router.respond(&request("HEAD", "/form")).status,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
}
