use std::io::{Read, Write};
use std::time::SystemTime;

use crate::error::HttpResult;
use crate::method::Method;
use crate::reader::{Limits, read_body, read_head};
use crate::request::{Request, Version};
use crate::response::{Response, WriteOptions, write_response};
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
            Err(error) => {
                // A client that simply went away is owed nothing. One that
                // sent something unintelligible is told so, and then the
                // connection is closed: once a message could not be parsed
                // there is no telling where the next one would start.
                if let Some(status) = error.status() {
                    let _ = write_response(output, Response::error(status), &closing());
                }
                break;
            }
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

/// Options for a final response, after which the connection is closed.
fn closing() -> WriteOptions {
    WriteOptions {
        version: Version::Http11,
        head_only: false,
        keep_alive: false,
        now: SystemTime::now(),
    }
}

fn next_request<R: Read>(stream: &mut Buffered<R>, limits: &Limits) -> HttpResult<Request> {
    let head = read_head(stream, limits)?;
    let body = read_body(stream, head.framing()?, limits)?;
    Ok(Request::new(head, body))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn status_of(input: &str, limits: &Limits) -> String {
        let (_, output) = exchange(input, &echo_path, limits);
        output.lines().next().unwrap_or("").to_string()
    }

    #[test]
    fn unintelligible_requests_get_an_error_response() {
        let limits = Limits::default();
        assert_eq!(
            status_of("complete nonsense\r\n\r\n", &limits),
            "HTTP/1.1 400 Bad Request"
        );
        assert_eq!(
            status_of("GET / HTTP/1.1\r\n\r\n", &limits),
            "HTTP/1.1 400 Bad Request"
        );
        assert_eq!(
            status_of("BREW / HTTP/1.1\r\nHost: x\r\n\r\n", &limits),
            "HTTP/1.1 501 Not Implemented"
        );
        assert_eq!(
            status_of("GET / HTTP/3.0\r\nHost: x\r\n\r\n", &limits),
            "HTTP/1.1 505 HTTP Version Not Supported"
        );
    }

    #[test]
    fn requests_over_the_limits_get_the_matching_status() {
        let small = Limits {
            max_head: 64,
            max_body: 8,
            ..Limits::default()
        };
        let long_head = format!(
            "GET / HTTP/1.1\r\nHost: x\r\nX-Pad: {}\r\n\r\n",
            "p".repeat(100)
        );
        assert_eq!(
            status_of(&long_head, &small),
            "HTTP/1.1 431 Request Header Fields Too Large"
        );
        let long_body = "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 9\r\n\r\n123456789";
        assert_eq!(
            status_of(long_body, &small),
            "HTTP/1.1 413 Content Too Large"
        );
    }

    #[test]
    fn an_error_closes_the_connection_even_if_more_follows() {
        let input = "GET /ok HTTP/1.1\r\nHost: x\r\n\r\nnonsense\r\n\r\nGET /never HTTP/1.1\r\nHost: x\r\n\r\n";
        let (served, output) = exchange(input, &echo_path, &Limits::default());
        assert_eq!((served, responses(&output)), (1, 2));
        assert!(output.contains("path=/ok") && !output.contains("/never"));
        let error = &output[output.rfind("HTTP/1.1 ").unwrap()..];
        assert!(error.starts_with("HTTP/1.1 400") && error.contains("Connection: close"));
        assert!(error.ends_with("400 Bad Request\n"));
    }

    #[test]
    fn a_client_that_just_leaves_is_sent_nothing() {
        let (served, output) = exchange("", &echo_path, &Limits::default());
        assert_eq!((served, output.as_str()), (0, ""));
    }
}
