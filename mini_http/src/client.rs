use std::io::Read;

use crate::error::{HttpError, HttpResult};
use crate::headers::Headers;
use crate::reader::read_chunked;
use crate::request::parse_header_line;
use crate::status::StatusCode;
use crate::stream::Buffered;

/// Bytes allowed for a response's status line and headers.
const MAX_RESPONSE_HEAD: usize = 64 * 1024;

/// What came back from a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientResponse {
    pub status: StatusCode,
    /// The words the server put after the status number.
    pub reason: String,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl ClientResponse {
    /// The body as text, with anything that is not UTF-8 replaced.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Parse a status line such as `HTTP/1.1 404 Not Found`.
pub fn parse_status_line(line: &str) -> HttpResult<(StatusCode, String)> {
    let bad = || HttpError::BadResponse(format!("`{line}` is not a status line"));
    let mut parts = line.splitn(3, ' ');
    let (version, code) = (parts.next().ok_or_else(bad)?, parts.next().ok_or_else(bad)?);
    let three_digits = code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_digit());
    if !version.starts_with("HTTP/1.") || !three_digits {
        return Err(bad());
    }
    let code = code.parse().map_err(|_| bad())?;
    Ok((StatusCode(code), parts.next().unwrap_or("").to_string()))
}

/// Read one response from the stream.
///
/// This is the server's reading code seen in a mirror: a head ending in an
/// empty line, then a body whose end the headers describe. `head_only` says
/// the request was a HEAD, in which case no body follows whatever the
/// headers claim.
pub fn read_response<R: Read>(
    stream: &mut Buffered<R>,
    head_only: bool,
    max_body: usize,
) -> HttpResult<ClientResponse> {
    loop {
        let raw = stream
            .read_until(b"\r\n\r\n", MAX_RESPONSE_HEAD)
            .map_err(as_response_error)?;
        let text = std::str::from_utf8(&raw[..raw.len() - 4])
            .map_err(|_| HttpError::BadResponse("the response head is not text".into()))?;
        let mut lines = text.split("\r\n");
        let (status, reason) = parse_status_line(lines.next().unwrap_or(""))?;
        let mut headers = Headers::new();
        for line in lines {
            let (name, value) = parse_header_line(line).map_err(as_response_error)?;
            headers.append(name, value);
        }
        // An interim response such as `100 Continue` is not the answer.
        // The real one follows it.
        if status.0 / 100 == 1 {
            continue;
        }

        let body = if head_only || !status.allows_body() {
            Vec::new()
        } else if headers.has_token("Transfer-Encoding", "chunked") {
            read_chunked(stream, max_body).map_err(as_response_error)?
        } else if let Some(length) = headers.get("Content-Length") {
            let length: usize = length.parse().map_err(|_| {
                HttpError::BadResponse(format!("`{length}` is not a content length"))
            })?;
            if length > max_body {
                return Err(as_response_error(HttpError::BodyTooLarge));
            }
            stream.read_exact(length)?
        } else {
            // Neither a length nor chunks: the body is everything the server
            // sends before it closes the connection.
            stream.read_to_end(max_body).map_err(as_response_error)?
        };
        return Ok(ClientResponse {
            status,
            reason,
            headers,
            body,
        });
    }
}

/// The parsing code is shared with the server, where a malformed message is
/// the client's fault. Seen from this side it is the server's.
fn as_response_error(error: HttpError) -> HttpError {
    match error {
        HttpError::BadRequest(reason) => HttpError::BadResponse(reason),
        HttpError::Closed => HttpError::BadResponse("the server closed without answering".into()),
        HttpError::LimitExceeded | HttpError::BodyTooLarge => {
            HttpError::BadResponse("the response is larger than allowed".into())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(raw: &str) -> HttpResult<ClientResponse> {
        read_response(&mut Buffered::new(raw.as_bytes()), false, 1000)
    }

    #[test]
    fn a_status_line_has_a_version_a_code_and_a_reason() {
        assert_eq!(
            parse_status_line("HTTP/1.1 200 OK"),
            Ok((StatusCode::OK, "OK".into()))
        );
        let missing = parse_status_line("HTTP/1.0 404 Not Found At All");
        assert_eq!(
            missing,
            Ok((StatusCode::NOT_FOUND, "Not Found At All".into()))
        );
        // The reason is optional.
        assert_eq!(
            parse_status_line("HTTP/1.1 204"),
            Ok((StatusCode::NO_CONTENT, String::new()))
        );
        for bad in [
            "",
            "HTTP/1.1",
            "HTTP/2 200 OK",
            "FTP/1.1 200 OK",
            "HTTP/1.1 20 OK",
            "HTTP/1.1 2xx OK",
        ] {
            assert!(
                matches!(parse_status_line(bad), Err(HttpError::BadResponse(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_body_with_a_length_is_read_exactly() {
        let response = read(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhelloEXTRA",
        )
        .unwrap();
        assert_eq!(
            (response.status, response.reason.as_str()),
            (StatusCode::OK, "OK")
        );
        assert_eq!(response.headers.get("content-type"), Some("text/plain"));
        assert_eq!(response.text(), "hello");
    }

    #[test]
    fn a_chunked_body_is_reassembled() {
        let response = read("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n").unwrap();
        assert_eq!(response.text(), "hello world");
    }

    #[test]
    fn without_a_length_the_body_runs_to_the_end_of_the_connection() {
        let response = read("HTTP/1.0 200 OK\r\n\r\nall of this\nuntil the end").unwrap();
        assert_eq!(response.text(), "all of this\nuntil the end");
    }

    #[test]
    fn some_responses_have_no_body_whatever_the_headers_say() {
        let not_modified =
            read("HTTP/1.1 304 Not Modified\r\nContent-Length: 500\r\n\r\n").unwrap();
        assert!(not_modified.body.is_empty());

        let raw = "HTTP/1.1 200 OK\r\nContent-Length: 500\r\n\r\n";
        let to_head = read_response(&mut Buffered::new(raw.as_bytes()), true, 1000).unwrap();
        assert!(to_head.body.is_empty());
        assert_eq!(to_head.headers.get("Content-Length"), Some("500"));
    }

    #[test]
    fn an_interim_response_is_skipped() {
        let response = read(
            "HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\nok",
        )
        .unwrap();
        assert_eq!(
            (response.status, response.text().as_str()),
            (StatusCode::CREATED, "ok")
        );
    }

    #[test]
    fn broken_and_oversized_responses_are_reported_as_the_servers_fault() {
        let bad = |raw: &str| matches!(read(raw), Err(HttpError::BadResponse(_)));
        assert!(bad(""), "nothing at all");
        assert!(bad("garbage\r\n\r\n"));
        assert!(bad("HTTP/1.1 200 OK\r\nBroken Header\r\n\r\n"));
        assert!(bad("HTTP/1.1 200 OK\r\nContent-Length: lots\r\n\r\n"));
        assert!(
            bad("HTTP/1.1 200 OK\r\nContent-Length: 5000\r\n\r\n"),
            "over the limit"
        );
        assert!(bad(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n"
        ));
        let cut_short = read("HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\nshort");
        assert_eq!(cut_short, Err(HttpError::UnexpectedEof));
    }
}
