use std::time::Instant;

use crate::request::Request;
use crate::response::Response;
use crate::service::Service;

/// Wraps a service and reports each request it handles.
///
/// The service inside does not know it is being watched: logging is added
/// from outside, by handing the server this in its place.
pub struct Logged<S> {
    inner: S,
    sink: Box<dyn Fn(&str) + Send + Sync>,
}

impl<S: Service> Logged<S> {
    /// Log to standard output.
    pub fn new(inner: S) -> Self {
        Self::with_sink(inner, |line| println!("{line}"))
    }

    /// Send each log line to `sink` instead.
    pub fn with_sink(inner: S, sink: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self {
            inner,
            sink: Box::new(sink),
        }
    }
}

impl<S: Service> Service for Logged<S> {
    fn call(&self, request: &Request) -> Response {
        let started = Instant::now();
        let response = self.inner.call(request);
        let millis = started.elapsed().as_secs_f64() * 1000.0;
        // A streamed body has no size until it has been sent.
        let size = response
            .bytes()
            .map_or("-".to_string(), |bytes| bytes.len().to_string());
        (self.sink)(&format!(
            "{} {} {} {size} {millis:.1}ms",
            request.method,
            request.path(),
            response.status.0
        ));
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::parse_request;
    use crate::status::StatusCode;
    use std::sync::{Arc, Mutex};

    fn request(raw: &str) -> Request {
        parse_request(raw.as_bytes()).unwrap()
    }

    #[test]
    fn each_request_produces_one_line() {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&lines);
        let service = Logged::with_sink(
            |request: &Request| match request.path() {
                "/missing" => Response::error(StatusCode::NOT_FOUND),
                "/stream" => Response::stream(StatusCode::OK, &b"abc"[..]),
                _ => Response::text(StatusCode::OK, "four"),
            },
            move |line| sink.lock().unwrap().push(line.to_string()),
        );

        let ok = service.call(&request("GET /page?x=1 HTTP/1.1\r\nHost: x\r\n\r\n"));
        // The response passes through unchanged.
        assert_eq!(ok.bytes(), Some(&b"four"[..]));
        service.call(&request(
            "POST /missing HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n",
        ));
        service.call(&request("GET /stream HTTP/1.1\r\nHost: x\r\n\r\n"));

        let lines = lines.lock().unwrap();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("GET /page 200 4 "), "{}", lines[0]);
        assert!(lines[0].ends_with("ms"));
        assert!(
            lines[1].starts_with("POST /missing 404 14 "),
            "{}",
            lines[1]
        );
        assert!(lines[2].starts_with("GET /stream 200 - "), "{}", lines[2]);
    }
}
