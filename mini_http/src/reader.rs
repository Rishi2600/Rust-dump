use std::io::Read;

use crate::error::{HttpError, HttpResult};
use crate::request::{Framing, Head, Request};
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

/// Longest line accepted inside a chunked body.
const MAX_CHUNK_LINE: usize = 1024;
/// Most trailer lines accepted after the last chunk.
const MAX_TRAILERS: usize = 32;

/// Read the body that the head announced.
pub fn read_body<R: Read>(
    stream: &mut Buffered<R>,
    framing: Framing,
    limits: &Limits,
) -> HttpResult<Vec<u8>> {
    match framing {
        Framing::None => Ok(Vec::new()),
        // The length is checked before reading, so an oversized body is
        // refused without receiving it.
        Framing::Length(length) if length > limits.max_body => Err(HttpError::BodyTooLarge),
        Framing::Length(length) => stream.read_exact(length),
        Framing::Chunked => read_chunked(stream, limits.max_body),
    }
}

/// Read a body sent with chunked transfer encoding.
///
/// The sender does not know the total length in advance, so it sends pieces.
/// Each piece is its size in hexadecimal, a line break, the bytes, and
/// another line break. A piece of size 0 ends the body:
///
/// ```text
/// 5\r\nhello\r\n
/// 6\r\n world\r\n
/// 0\r\n\r\n
/// ```
pub fn read_chunked<R: Read>(stream: &mut Buffered<R>, max_body: usize) -> HttpResult<Vec<u8>> {
    let bad = |reason: &str| HttpError::BadRequest(format!("chunked body: {reason}"));
    let mut body = Vec::new();
    loop {
        let line = read_chunk_line(stream)?;
        // Anything after a semicolon is a chunk extension. Nothing uses them.
        let digits = line.split(';').next().unwrap_or("").trim();
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(bad("a chunk size is not a hexadecimal number"));
        }
        let size = usize::from_str_radix(digits, 16).map_err(|_| HttpError::BodyTooLarge)?;
        if size == 0 {
            break;
        }
        if size > max_body - body.len() {
            return Err(HttpError::BodyTooLarge);
        }
        body.extend(stream.read_exact(size)?);
        if stream.read_exact(2)? != b"\r\n" {
            return Err(bad("a chunk is not followed by a line break"));
        }
    }
    // Header lines may follow the last chunk. They are read and discarded.
    for _ in 0..MAX_TRAILERS {
        if read_chunk_line(stream)?.is_empty() {
            return Ok(body);
        }
    }
    Err(bad("too many trailer lines"))
}

fn read_chunk_line<R: Read>(stream: &mut Buffered<R>) -> HttpResult<String> {
    let raw = stream
        .read_until(b"\r\n", MAX_CHUNK_LINE)
        .map_err(|error| match error {
            HttpError::LimitExceeded => {
                HttpError::BadRequest("chunked body: a line is too long".into())
            }
            // Inside a body, the stream ending is never a clean close.
            HttpError::Closed => HttpError::UnexpectedEof,
            other => other,
        })?;
    String::from_utf8(raw[..raw.len() - 2].to_vec())
        .map_err(|_| HttpError::BadRequest("chunked body: a line is not text".into()))
}

/// Read one whole request: head, then body.
pub fn read_request<R: Read>(stream: &mut Buffered<R>, limits: &Limits) -> HttpResult<Request> {
    let head = read_head(stream, limits)?;
    let body = read_body(stream, head.framing()?, limits)?;
    Ok(Request::new(head, body))
}

/// Parse a request held entirely in memory, with the default limits.
pub fn parse_request(raw: &[u8]) -> HttpResult<Request> {
    read_request(&mut Buffered::new(raw), &Limits::default())
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

    #[test]
    fn a_body_of_known_length_is_read_exactly() {
        let mut stream =
            stream("POST /form HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\n\r\nhello worldNEXT");
        let request = read_request(&mut stream, &Limits::default()).unwrap();
        assert_eq!(request.body, b"hello world");
        assert_eq!((request.method, request.path()), (Method::Post, "/form"));
        // The bytes after the body belong to the next request.
        assert_eq!(stream.buffered(), b"NEXT");
    }

    #[test]
    fn a_chunked_body_is_reassembled() {
        let raw = "POST / HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n\
                   5\r\nhello\r\n6;ext=1\r\n world\r\nA\r\n0123456789\r\n0\r\nTrailer: x\r\n\r\nNEXT";
        let mut stream = stream(raw);
        let request = read_request(&mut stream, &Limits::default()).unwrap();
        assert_eq!(request.body, b"hello world0123456789");
        assert_eq!(stream.buffered(), b"NEXT");
    }

    #[test]
    fn two_requests_sent_back_to_back_are_read_one_after_the_other() {
        let mut stream = stream(
            "POST /one HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\n\r\nabcGET /two HTTP/1.1\r\nHost: x\r\n\r\n",
        );
        let limits = Limits::default();
        let first = read_request(&mut stream, &limits).unwrap();
        let second = read_request(&mut stream, &limits).unwrap();
        assert_eq!((first.path(), first.body.as_slice()), ("/one", &b"abc"[..]));
        assert_eq!((second.path(), second.body.len()), ("/two", 0));
        assert_eq!(read_request(&mut stream, &limits), Err(HttpError::Closed));
    }

    #[test]
    fn bodies_over_the_limit_are_refused() {
        let limits = Limits {
            max_body: 10,
            ..Limits::default()
        };
        let declared = "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\n\r\nhello world";
        assert_eq!(
            read_request(&mut stream(declared), &limits),
            Err(HttpError::BodyTooLarge)
        );
        // Chunks are counted as they arrive, since no total is announced.
        let chunked = "POST / HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n\
                       6\r\nhello \r\n6\r\nworld!\r\n0\r\n\r\n";
        assert_eq!(
            read_request(&mut stream(chunked), &limits),
            Err(HttpError::BodyTooLarge)
        );
    }

    #[test]
    fn malformed_and_truncated_bodies_are_errors() {
        let chunked = |body: &str| {
            let raw =
                format!("POST / HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n{body}");
            parse_request(raw.as_bytes())
        };
        assert!(matches!(
            chunked("zz\r\nhello\r\n0\r\n\r\n"),
            Err(HttpError::BadRequest(_))
        ));
        assert!(matches!(
            chunked("5\r\nhelloXX0\r\n\r\n"),
            Err(HttpError::BadRequest(_))
        ));
        assert!(matches!(
            chunked("-5\r\nhello\r\n0\r\n\r\n"),
            Err(HttpError::BadRequest(_))
        ));
        assert_eq!(chunked("5\r\nhel"), Err(HttpError::UnexpectedEof));
        assert_eq!(chunked("5\r\nhello\r\n"), Err(HttpError::UnexpectedEof));

        let short = parse_request(b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 9\r\n\r\nshort");
        assert_eq!(short, Err(HttpError::UnexpectedEof));
    }

    #[test]
    fn a_request_knows_whether_to_keep_the_connection() {
        let keep = |raw: &str| parse_request(raw.as_bytes()).unwrap().keep_alive();
        assert!(keep("GET / HTTP/1.1\r\nHost: x\r\n\r\n"));
        assert!(!keep(
            "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
        ));
        assert!(!keep("GET / HTTP/1.0\r\n\r\n"));
        assert!(keep("GET / HTTP/1.0\r\nConnection: Keep-Alive\r\n\r\n"));
    }

    #[test]
    fn a_request_exposes_its_parts() {
        let request =
            parse_request(b"GET /find?q=a+b HTTP/1.1\r\nHost: x\r\nAccept: */*\r\n\r\n").unwrap();
        assert_eq!(request.query("q"), Some("a b"));
        assert_eq!(request.header("accept"), Some("*/*"));
        assert_eq!(request.header("missing"), None);
    }
}
