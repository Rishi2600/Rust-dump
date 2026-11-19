use std::io::Read;

use crate::error::{HttpError, HttpResult};
use crate::request::Head;
use crate::stream::Buffered;

/// How much a client is allowed to make the server hold or do.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Bytes allowed for the request line and headers together.
    pub max_head: usize,
    /// Bytes allowed for a request body.
    pub max_body: usize,
    /// Requests served on one connection before it is closed.
    pub max_requests: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_head: 8 * 1024,
            max_body: 1024 * 1024,
            max_requests: 100,
        }
    }
}

/// Blank lines tolerated before a request line.
const MAX_LEADING_BLANK_LINES: usize = 4;

/// Read one request head from the stream: everything up to the empty line.
pub fn read_head<R: Read>(stream: &mut Buffered<R>, limits: &Limits) -> HttpResult<Head> {
    for _ in 0..=MAX_LEADING_BLANK_LINES {
        let raw = stream
            .read_until(b"\r\n\r\n", limits.max_head)
            .map_err(|error| match error {
                HttpError::LimitExceeded => HttpError::HeadersTooLarge,
                other => other,
            })?;
        let text = std::str::from_utf8(&raw)
            .map_err(|_| HttpError::BadRequest("the request head is not valid UTF-8".into()))?;
        // Some clients send a stray line break after a request body. The
        // specification asks servers to ignore it.
        let text = text.trim_start_matches("\r\n");
        if let Some(head) = text.strip_suffix("\r\n\r\n") {
            return Head::parse(head);
        }
    }
    Err(HttpError::BadRequest(
        "too many blank lines before the request".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::method::Method;

    fn stream(data: &str) -> Buffered<&[u8]> {
        Buffered::new(data.as_bytes())
    }

    #[test]
    fn a_head_is_read_up_to_the_empty_line_and_no_further() {
        let mut stream = stream("GET /a HTTP/1.1\r\nHost: x\r\n\r\nBODY");
        let head = read_head(&mut stream, &Limits::default()).unwrap();
        assert_eq!(
            (head.method, head.target.path.as_str()),
            (Method::Get, "/a")
        );
        assert_eq!(stream.buffered(), b"BODY");
    }

    #[test]
    fn a_closed_connection_and_a_truncated_request_are_different() {
        let limits = Limits::default();
        assert_eq!(read_head(&mut stream(""), &limits), Err(HttpError::Closed));
        let cut_off = read_head(&mut stream("GET /a HTTP/1.1\r\nHost"), &limits);
        assert_eq!(cut_off, Err(HttpError::UnexpectedEof));
    }

    #[test]
    fn oversized_heads_are_refused() {
        let limits = Limits {
            max_head: 64,
            ..Limits::default()
        };
        let padded = format!(
            "GET / HTTP/1.1\r\nHost: x\r\nX-Padding: {}\r\n\r\n",
            "p".repeat(100)
        );
        assert_eq!(
            read_head(&mut stream(&padded), &limits),
            Err(HttpError::HeadersTooLarge)
        );
    }

    #[test]
    fn stray_blank_lines_before_a_request_are_skipped() {
        let limits = Limits::default();
        let mut one = stream("\r\nGET /a HTTP/1.1\r\nHost: x\r\n\r\n");
        assert!(read_head(&mut one, &limits).is_ok());
        let mut several = stream("\r\n\r\n\r\n\r\nGET /a HTTP/1.1\r\nHost: x\r\n\r\n");
        assert!(read_head(&mut several, &limits).is_ok());
        // But not without end.
        let flood = "\r\n".repeat(100);
        assert!(matches!(
            read_head(&mut stream(&flood), &limits),
            Err(HttpError::BadRequest(_))
        ));
    }

    #[test]
    fn bytes_that_are_not_text_are_a_bad_request() {
        let mut binary = Buffered::new(&b"GET /\xFF HTTP/1.1\r\nHost: x\r\n\r\n"[..]);
        let result = read_head(&mut binary, &Limits::default());
        assert!(matches!(result, Err(HttpError::BadRequest(_))));
    }
}
