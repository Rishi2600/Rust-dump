use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::time::SystemTime;

use crate::date::http_date;
use crate::error::HttpResult;
use crate::headers::Headers;
use crate::request::Version;
use crate::status::StatusCode;

/// What a response carries after its headers.
pub enum Body {
    /// Content held in memory, whose length is therefore known.
    Bytes(Vec<u8>),
    /// Content produced as it is sent, whose length is not known up front.
    Stream(Box<dyn Read + Send>),
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Body::Bytes(bytes) => write!(f, "Bytes({} bytes)", bytes.len()),
            Body::Stream(_) => write!(f, "Stream"),
        }
    }
}

/// What a handler gives back: a status, some headers, and a body.
#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub headers: Headers,
    pub body: Body,
}

impl Response {
    /// An empty response with the given status.
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            headers: Headers::new(),
            body: Body::Bytes(Vec::new()),
        }
    }

    /// Set a header, replacing any earlier value.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.set(name, value);
        self
    }

    pub fn body(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.body = Body::Bytes(bytes.into());
        self
    }

    pub fn text(status: StatusCode, text: impl Into<String>) -> Self {
        Self::new(status)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(text.into())
    }

    pub fn html(status: StatusCode, html: impl Into<String>) -> Self {
        Self::new(status)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(html.into())
    }

    /// A response whose body just names the status, such as `404 Not Found`.
    pub fn error(status: StatusCode) -> Self {
        Self::text(status, format!("{status}\n"))
    }

    /// Send the client to another address.
    pub fn redirect(status: StatusCode, location: &str) -> Self {
        Self::text(status, format!("{status}: {location}\n")).header("Location", location)
    }

    /// A response whose body is read from `source` while it is being sent.
    pub fn stream(status: StatusCode, source: impl Read + Send + 'static) -> Self {
        let mut response = Self::new(status);
        response.body = Body::Stream(Box::new(source));
        response
    }

    /// The body, when it is held in memory.
    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.body {
            Body::Bytes(bytes) => Some(bytes),
            Body::Stream(_) => None,
        }
    }
}

/// What the writer needs to know about the request being answered.
#[derive(Debug, Clone, Copy)]
pub struct WriteOptions {
    /// The version the client spoke.
    pub version: Version,
    /// The request was a HEAD: send the headers a GET would get, and no body.
    pub head_only: bool,
    /// Whether the connection should stay open afterwards.
    pub keep_alive: bool,
    /// The time to put in the `Date` header.
    pub now: SystemTime,
}

/// Write a response onto the connection.
///
/// ```text
/// HTTP/1.1 200 OK                  <- status line
/// Content-Type: text/plain         <- header fields
/// Content-Length: 5
///                                  <- an empty line ends the head
/// hello                            <- body
/// ```
///
/// Returns whether the connection can carry another request afterwards.
pub fn write_response<W: Write>(
    out: &mut W,
    response: Response,
    options: &WriteOptions,
) -> HttpResult<bool> {
    let Response {
        status,
        mut headers,
        body,
    } = response;
    let mut keep_alive = options.keep_alive;
    let sends_body = status.allows_body();

    if !headers.contains("Date") {
        headers.set("Date", &http_date(options.now));
    }
    // The client must be able to find the end of this response, or it cannot
    // find the start of the next one on the same connection.
    let chunked = match &body {
        // A body in memory has a length. It is computed here, so a handler
        // cannot get it wrong.
        Body::Bytes(bytes) => {
            if sends_body {
                headers.set("Content-Length", &bytes.len().to_string());
            } else {
                headers.remove("Content-Length");
            }
            false
        }
        Body::Stream(_) => {
            headers.remove("Content-Length");
            if !sends_body {
                false
            } else if options.version == Version::Http11 {
                // No length is known, so the body goes out in sized chunks.
                headers.set("Transfer-Encoding", "chunked");
                true
            } else {
                // HTTP/1.0 has no chunks. The only way left to mark the end
                // of the body is to close the connection.
                keep_alive = false;
                false
            }
        }
    };
    headers.set(
        "Connection",
        if keep_alive { "keep-alive" } else { "close" },
    );

    let mut head = format!("HTTP/1.1 {status}\r\n");
    for (name, value) in headers.iter() {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    out.write_all(head.as_bytes())?;

    if sends_body && !options.head_only {
        match body {
            Body::Bytes(bytes) => out.write_all(&bytes)?,
            Body::Stream(mut source) => copy_stream(&mut source, out, chunked)?,
        }
    }
    out.flush()?;
    Ok(keep_alive)
}

/// Copy `source` to `out` until it runs dry, optionally as chunks: each one
/// its size in hexadecimal, a line break, the bytes, and a line break, with
/// a chunk of size zero to finish.
fn copy_stream<W: Write>(source: &mut dyn Read, out: &mut W, chunked: bool) -> HttpResult<()> {
    let mut buffer = [0; 8192];
    loop {
        let count = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        if chunked {
            write!(out, "{count:x}\r\n")?;
        }
        out.write_all(&buffer[..count])?;
        if chunked {
            out.write_all(b"\r\n")?;
        }
    }
    if chunked {
        out.write_all(b"0\r\n\r\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_response_is_built_up_piece_by_piece() {
        let response = Response::new(StatusCode::CREATED)
            .header("Location", "/items/7")
            .header("X-Flag", "a")
            .header("x-flag", "b")
            .body("made");
        assert_eq!(response.status, StatusCode::CREATED);
        assert_eq!(response.headers.get("Location"), Some("/items/7"));
        // Setting a header again replaces it.
        assert_eq!(
            response.headers.get_all("X-Flag").collect::<Vec<_>>(),
            ["b"]
        );
        assert_eq!(response.bytes(), Some(&b"made"[..]));
    }

    #[test]
    fn shortcuts_set_the_content_type() {
        let text = Response::text(StatusCode::OK, "hi");
        assert_eq!(
            text.headers.get("Content-Type"),
            Some("text/plain; charset=utf-8")
        );
        let html = Response::html(StatusCode::OK, "<p>hi</p>");
        assert_eq!(
            html.headers.get("Content-Type"),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(html.bytes(), Some(&b"<p>hi</p>"[..]));
    }

    #[test]
    fn errors_and_redirects_describe_themselves() {
        let missing = Response::error(StatusCode::NOT_FOUND);
        assert_eq!(missing.bytes(), Some(&b"404 Not Found\n"[..]));

        let moved = Response::redirect(StatusCode::MOVED_PERMANENTLY, "/docs/");
        assert_eq!(moved.status, StatusCode::MOVED_PERMANENTLY);
        assert_eq!(moved.headers.get("Location"), Some("/docs/"));
        assert_eq!(moved.bytes(), Some(&b"301 Moved Permanently: /docs/\n"[..]));
        assert_eq!(format!("{:?}", moved.body), "Bytes(30 bytes)");
    }

    use std::time::{Duration, UNIX_EPOCH};

    fn options() -> WriteOptions {
        WriteOptions {
            version: Version::Http11,
            head_only: false,
            keep_alive: true,
            now: UNIX_EPOCH + Duration::from_secs(784_111_777),
        }
    }

    fn written(response: Response, options: &WriteOptions) -> String {
        let mut out = Vec::new();
        write_response(&mut out, response, options).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_response_is_written_as_status_line_headers_blank_line_body() {
        let text = written(Response::text(StatusCode::OK, "hello"), &options());
        assert_eq!(
            text,
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Date: Sun, 06 Nov 1994 08:49:37 GMT\r\n\
             Content-Length: 5\r\n\
             Connection: keep-alive\r\n\
             \r\n\
             hello"
        );
    }

    #[test]
    fn the_length_is_computed_not_trusted() {
        let lying = Response::text(StatusCode::OK, "hello").header("Content-Length", "999");
        let text = written(lying, &options());
        assert!(text.contains("Content-Length: 5\r\n"));
        assert!(!text.contains("999"));
    }

    #[test]
    fn a_head_request_gets_the_headers_without_the_body() {
        let head_only = WriteOptions {
            head_only: true,
            ..options()
        };
        let text = written(Response::text(StatusCode::OK, "hello"), &head_only);
        // The length still describes what a GET would have received.
        assert!(text.contains("Content-Length: 5\r\n"));
        assert!(text.ends_with("\r\n\r\n"));
    }

    #[test]
    fn bodiless_statuses_send_neither_body_nor_length() {
        let not_modified = Response::new(StatusCode::NOT_MODIFIED).body("ignored");
        let text = written(not_modified, &options());
        assert!(text.starts_with("HTTP/1.1 304 Not Modified\r\n"));
        assert!(!text.contains("Content-Length"));
        assert!(text.ends_with("\r\n\r\n"));
    }

    #[test]
    fn the_connection_header_says_what_happens_next() {
        let closing = WriteOptions {
            keep_alive: false,
            ..options()
        };
        let mut out = Vec::new();
        let reusable = write_response(&mut out, Response::new(StatusCode::OK), &closing);
        assert_eq!(reusable, Ok(false));
        assert!(
            String::from_utf8(out)
                .unwrap()
                .contains("Connection: close\r\n")
        );

        let mut out = Vec::new();
        let reusable = write_response(&mut out, Response::new(StatusCode::OK), &options());
        assert_eq!(reusable, Ok(true));
    }

    #[test]
    fn a_handler_may_supply_its_own_date() {
        let dated = Response::new(StatusCode::OK).header("Date", "Thu, 01 Jan 1970 00:00:00 GMT");
        let text = written(dated, &options());
        assert!(text.contains("Date: Thu, 01 Jan 1970 00:00:00 GMT\r\n"));
        assert!(!text.contains("1994"));
    }

    /// A source that hands over one prepared piece per read.
    struct Pieces(Vec<&'static str>);

    impl Read for Pieces {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0.is_empty() {
                return Ok(0);
            }
            let piece = self.0.remove(0).as_bytes();
            buf[..piece.len()].copy_from_slice(piece);
            Ok(piece.len())
        }
    }

    fn streamed() -> Response {
        Response::stream(
            StatusCode::OK,
            Pieces(vec!["hello", " wor", "ld, this is longer"]),
        )
    }

    #[test]
    fn a_stream_of_unknown_length_is_sent_in_chunks() {
        let text = written(streamed(), &options());
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        assert!(head.contains("Transfer-Encoding: chunked"));
        assert!(!head.contains("Content-Length"));
        // 0x12 is 18, the length of the last piece.
        assert_eq!(
            body,
            "5\r\nhello\r\n4\r\n wor\r\n12\r\nld, this is longer\r\n0\r\n\r\n"
        );

        // Our own chunk decoder reads it back.
        let mut reader = crate::stream::Buffered::new(body.as_bytes());
        let decoded = crate::reader::read_chunked(&mut reader, 1000).unwrap();
        assert_eq!(decoded, b"hello world, this is longer");
    }

    #[test]
    fn an_http_1_0_client_gets_the_stream_ended_by_closing() {
        let old_client = WriteOptions {
            version: Version::Http10,
            ..options()
        };
        let mut out = Vec::new();
        let reusable = write_response(&mut out, streamed(), &old_client);
        // The connection cannot be reused, whatever the client asked for.
        assert_eq!(reusable, Ok(false));
        let text = String::from_utf8(out).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        assert!(head.contains("Connection: close") && !head.contains("chunked"));
        assert_eq!(body, "hello world, this is longer");
    }

    #[test]
    fn a_streamed_response_has_no_bytes_to_inspect() {
        let response = streamed();
        assert_eq!(response.bytes(), None);
        assert_eq!(format!("{:?}", response.body), "Stream");
    }
}
