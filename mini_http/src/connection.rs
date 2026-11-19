use std::io::{Read, Write};
use std::time::SystemTime;

use crate::error::HttpResult;
use crate::method::Method;
use crate::reader::{Limits, read_body, read_head};
use crate::request::Request;
use crate::response::{WriteOptions, write_response};
use crate::service::Service;
use crate::stream::Buffered;

/// Serve requests arriving on `input`, writing responses to `output`, until
/// the client is done or the connection has to be closed.
///
/// Opening a TCP connection takes a round trip before any data moves, so
/// HTTP/1.1 keeps one open and sends request after request down it. This
/// loop is that conversation. Returns how many requests were served.
pub fn serve_connection<R: Read, W: Write>(
    input: R,
    output: &mut W,
    service: &dyn Service,
    limits: &Limits,
) -> usize {
    let mut stream = Buffered::new(input);
    let mut served = 0;
    while served < limits.max_requests {
        let request = match next_request(&mut stream, limits) {
            Ok(request) => request,
            Err(_) => break,
        };
        served += 1;
        let response = service.call(&request);
        let options = WriteOptions {
            version: request.version,
            head_only: request.method == Method::Head,
            // The last request allowed on a connection is told it is the last.
            keep_alive: request.keep_alive() && served < limits.max_requests,
            now: SystemTime::now(),
        };
        if !matches!(write_response(output, response, &options), Ok(true)) {
            break;
        }
    }
    served
}

fn next_request<R: Read>(stream: &mut Buffered<R>, limits: &Limits) -> HttpResult<Request> {
    let head = read_head(stream, limits)?;
    let body = read_body(stream, head.framing()?, limits)?;
    Ok(Request::new(head, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::Response;
    use crate::status::StatusCode;

    fn echo_path(request: &Request) -> Response {
        Response::text(StatusCode::OK, format!("path={}", request.path()))
    }

    /// Feed `input` to a connection and collect what is written back.
    fn exchange(input: &str, service: &dyn Service, limits: &Limits) -> (usize, String) {
        let mut output = Vec::new();
        let served = serve_connection(input.as_bytes(), &mut output, service, limits);
        (served, String::from_utf8(output).unwrap())
    }

    fn responses(output: &str) -> usize {
        output.matches("HTTP/1.1 ").count()
    }

    #[test]
    fn several_requests_are_served_on_one_connection() {
        let input = "GET /one HTTP/1.1\r\nHost: x\r\n\r\nGET /two HTTP/1.1\r\nHost: x\r\n\r\n";
        let (served, output) = exchange(input, &echo_path, &Limits::default());
        assert_eq!((served, responses(&output)), (2, 2));
        assert!(output.contains("path=/one") && output.contains("path=/two"));
        assert!(output.contains("Connection: keep-alive"));
    }

    #[test]
    fn connection_close_ends_the_conversation() {
        let input = "GET /one HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\nGET /two HTTP/1.1\r\nHost: x\r\n\r\n";
        let (served, output) = exchange(input, &echo_path, &Limits::default());
        assert_eq!((served, responses(&output)), (1, 1));
        assert!(output.contains("Connection: close") && !output.contains("/two"));
    }

    #[test]
    fn http_1_0_closes_after_one_response_unless_asked_not_to() {
        let input = "GET /one HTTP/1.0\r\n\r\nGET /two HTTP/1.0\r\n\r\n";
        let (served, _) = exchange(input, &echo_path, &Limits::default());
        assert_eq!(served, 1);

        let input = "GET /one HTTP/1.0\r\nConnection: keep-alive\r\n\r\nGET /two HTTP/1.0\r\n\r\n";
        let (served, _) = exchange(input, &echo_path, &Limits::default());
        assert_eq!(served, 2);
    }

    #[test]
    fn a_connection_is_closed_after_its_quota_of_requests() {
        let limits = Limits {
            max_requests: 2,
            ..Limits::default()
        };
        let input = "GET /a HTTP/1.1\r\nHost: x\r\n\r\n".repeat(5);
        let (served, output) = exchange(&input, &echo_path, &limits);
        assert_eq!((served, responses(&output)), (2, 2));
        // The first response keeps the connection, the second announces the end.
        let (first, second) = output.split_at(output.rfind("HTTP/1.1 ").unwrap());
        assert!(first.contains("Connection: keep-alive"));
        assert!(second.contains("Connection: close"));
    }

    #[test]
    fn a_head_request_is_answered_without_a_body() {
        let input = "HEAD /thing HTTP/1.1\r\nHost: x\r\n\r\n";
        let (_, output) = exchange(input, &echo_path, &Limits::default());
        assert!(output.contains("Content-Length: 11\r\n"));
        assert!(output.ends_with("\r\n\r\n") && !output.contains("path="));
    }

    #[test]
    fn a_request_body_reaches_the_handler() {
        let shout = |request: &Request| {
            let text = String::from_utf8_lossy(&request.body).to_uppercase();
            Response::text(StatusCode::OK, text)
        };
        let input = "POST /shout HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\n\r\nhello";
        let (_, output) = exchange(input, &shout, &Limits::default());
        assert!(output.ends_with("\r\n\r\nHELLO"));
    }
}
