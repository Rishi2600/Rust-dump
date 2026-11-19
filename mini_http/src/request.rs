use crate::error::{HttpError, HttpResult};
use crate::headers::{Headers, is_token};
use crate::method::Method;
use crate::target::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    Http10,
    Http11,
}

impl Version {
    pub fn as_str(self) -> &'static str {
        match self {
            Version::Http10 => "HTTP/1.0",
            Version::Http11 => "HTTP/1.1",
        }
    }

    pub fn parse(text: &str) -> HttpResult<Self> {
        match text {
            "HTTP/1.1" => Ok(Version::Http11),
            "HTTP/1.0" => Ok(Version::Http10),
            other if other.starts_with("HTTP/") => {
                Err(HttpError::UnsupportedVersion(other.to_string()))
            }
            other => Err(HttpError::BadRequest(format!(
                "`{other}` is not an HTTP version"
            ))),
        }
    }
}

/// Everything in a request before its body.
///
/// ```text
/// GET /search?q=rust HTTP/1.1      <- request line
/// Host: example.com                <- header fields, one per line
/// Accept: text/html
///                                  <- an empty line ends the head
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub method: Method,
    pub target: Target,
    pub version: Version,
    pub headers: Headers,
}

/// Parse the first line: a method, a target and a version, separated by
/// single spaces.
pub fn parse_request_line(line: &str) -> HttpResult<(Method, Target, Version)> {
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(HttpError::BadRequest(
            "the request line is not `METHOD target VERSION`".into(),
        ));
    };
    Ok((
        method.parse()?,
        Target::parse(target)?,
        Version::parse(version)?,
    ))
}

/// Parse one `Name: value` line into its two halves.
pub fn parse_header_line(line: &str) -> HttpResult<(&str, &str)> {
    let Some((name, value)) = line.split_once(':') else {
        return Err(HttpError::BadRequest(format!(
            "header line `{line}` has no colon"
        )));
    };
    // No space is allowed between the name and the colon. Servers that
    // disagree about `Name : value` can be made to disagree about where a
    // request ends, so the strict reading is the safe one.
    if !is_token(name) {
        return Err(HttpError::BadRequest(format!(
            "`{name}` is not a header name"
        )));
    }
    let value = value.trim_matches([' ', '\t']);
    if value
        .bytes()
        .any(|byte| byte.is_ascii_control() && byte != b'\t')
    {
        return Err(HttpError::BadRequest(format!(
            "the value of `{name}` has control characters"
        )));
    }
    Ok((name, value))
}

impl Head {
    /// Parse the text of a request head: the lines before the empty line,
    /// separated by CRLF.
    pub fn parse(text: &str) -> HttpResult<Self> {
        let mut lines = text.split("\r\n");
        let (method, target, version) = parse_request_line(lines.next().unwrap_or(""))?;
        let mut headers = Headers::new();
        for line in lines {
            let (name, value) = parse_header_line(line)?;
            headers.append(name, value);
        }
        // One address can serve many sites, so HTTP/1.1 makes the client say
        // which one it wants. Two different answers would be ambiguous.
        let hosts = headers.get_all("Host").count();
        if hosts > 1 || (hosts == 0 && version == Version::Http11) {
            return Err(HttpError::BadRequest(
                "an HTTP/1.1 request needs exactly one Host header".into(),
            ));
        }
        Ok(Self {
            method,
            target,
            version,
            headers,
        })
    }
}

/// A complete request: the head, and the body that followed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub target: Target,
    pub version: Version,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl Request {
    pub fn new(head: Head, body: Vec<u8>) -> Self {
        Self {
            method: head.method,
            target: head.target,
            version: head.version,
            headers: head.headers,
            body,
        }
    }

    pub fn path(&self) -> &str {
        &self.target.path
    }

    /// The first value of a query string parameter.
    pub fn query(&self, name: &str) -> Option<&str> {
        self.target.query_value(name)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name)
    }

    /// Whether the client wants to send more requests on this connection.
    ///
    /// HTTP/1.1 keeps connections open unless told otherwise, because
    /// opening one is slow. HTTP/1.0 closed after every response unless the
    /// client asked to keep it.
    pub fn keep_alive(&self) -> bool {
        match self.version {
            Version::Http11 => !self.headers.has_token("Connection", "close"),
            Version::Http10 => self.headers.has_token("Connection", "keep-alive"),
        }
    }
}

/// How a message says where its body ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    /// There is no body.
    None,
    /// The body is exactly this many bytes.
    Length(usize),
    /// The body arrives in pieces, each announcing its own size.
    Chunked,
}

impl Head {
    /// Work out how the body is delimited.
    ///
    /// TCP is a stream with no message boundaries, so the headers are the
    /// only thing that says where one request stops and the next begins. If
    /// two servers in a chain read that differently, an attacker can hide a
    /// second request inside the first ("request smuggling"). Every
    /// ambiguous case is therefore an error, never a guess.
    pub fn framing(&self) -> HttpResult<Framing> {
        let lengths: Vec<&str> = self.headers.get_all("Content-Length").collect();
        if let Some(encoding) = self.headers.get("Transfer-Encoding") {
            if !lengths.is_empty() {
                return Err(HttpError::BadRequest(
                    "both Content-Length and Transfer-Encoding are present".into(),
                ));
            }
            if self.version == Version::Http10 {
                return Err(HttpError::BadRequest(
                    "HTTP/1.0 has no transfer encodings".into(),
                ));
            }
            let repeated = self.headers.get_all("Transfer-Encoding").count() > 1;
            if repeated || !encoding.eq_ignore_ascii_case("chunked") {
                return Err(HttpError::NotImplemented(format!(
                    "the transfer encoding `{encoding}`"
                )));
            }
            return Ok(Framing::Chunked);
        }
        let Some(first) = lengths.first() else {
            return Ok(Framing::None);
        };
        // A repeated length is harmless only if every copy says the same.
        if lengths.iter().any(|length| length != first) {
            return Err(HttpError::BadRequest(
                "conflicting Content-Length headers".into(),
            ));
        }
        if first.is_empty() || !first.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(HttpError::BadRequest(format!(
                "`{first}` is not a content length"
            )));
        }
        // All digits, yet too big for a number: certainly too big to accept.
        let length = first.parse().map_err(|_| HttpError::BodyTooLarge)?;
        Ok(Framing::Length(length))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bad_request<T: std::fmt::Debug>(result: HttpResult<T>) -> bool {
        matches!(result, Err(HttpError::BadRequest(_)))
    }

    #[test]
    fn a_request_line_has_three_parts() {
        let (method, target, version) = parse_request_line("GET /a/b?x=1 HTTP/1.1").unwrap();
        assert_eq!(method, Method::Get);
        assert_eq!(
            (target.path.as_str(), target.query_value("x")),
            ("/a/b", Some("1"))
        );
        assert_eq!(version, Version::Http11);

        assert!(bad_request(parse_request_line("GET /")));
        assert!(bad_request(parse_request_line("GET  / HTTP/1.1")));
        assert!(bad_request(parse_request_line("GET / HTTP/1.1 extra")));
        assert!(bad_request(parse_request_line("")));
    }

    #[test]
    fn each_part_of_the_request_line_fails_in_its_own_way() {
        let not_implemented = parse_request_line("BREW /pot HTTP/1.1");
        assert!(matches!(not_implemented, Err(HttpError::NotImplemented(_))));
        let version = parse_request_line("GET / HTTP/2.0");
        assert_eq!(
            version,
            Err(HttpError::UnsupportedVersion("HTTP/2.0".into()))
        );
        assert!(bad_request(parse_request_line("GET / FTP/1.1")));
        assert!(bad_request(parse_request_line("GET nowhere HTTP/1.1")));
        // The first bytes of a TLS handshake, sent to a plain HTTP port.
        assert!(bad_request(parse_request_line("\u{16}\u{3}\u{1} x y")));
    }

    #[test]
    fn a_header_line_is_a_name_a_colon_and_a_value() {
        assert_eq!(
            parse_header_line("Host: example.com"),
            Ok(("Host", "example.com"))
        );
        assert_eq!(parse_header_line("X-Empty:"), Ok(("X-Empty", "")));
        assert_eq!(
            parse_header_line("Accept:  text/html \t"),
            Ok(("Accept", "text/html"))
        );
        // Colons after the first belong to the value.
        assert_eq!(
            parse_header_line("Host: localhost:8080"),
            Ok(("Host", "localhost:8080"))
        );
    }

    #[test]
    fn sloppy_header_lines_are_refused() {
        for line in [
            "no colon here",
            "Host : example.com",
            " Host: x",
            ": value",
            "Na me: x",
            "X: a\u{0}b",
        ] {
            assert!(bad_request(parse_header_line(line)), "{line:?}");
        }
    }

    #[test]
    fn a_head_is_a_request_line_and_its_headers() {
        let head =
            Head::parse("POST /submit HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5").unwrap();
        assert_eq!((head.method, head.version), (Method::Post, Version::Http11));
        assert_eq!(head.target.path, "/submit");
        assert_eq!(head.headers.get("host"), Some("example.com"));
        assert_eq!(head.headers.get("content-length"), Some("5"));
        assert_eq!(head.headers.len(), 2);
    }

    #[test]
    fn http_1_1_requires_exactly_one_host() {
        assert!(bad_request(Head::parse("GET / HTTP/1.1")));
        assert!(bad_request(Head::parse(
            "GET / HTTP/1.1\r\nHost: a\r\nHost: b"
        )));
        // HTTP/1.0 predates the Host header.
        assert!(Head::parse("GET / HTTP/1.0").is_ok());
    }

    #[test]
    fn line_breaks_other_than_crlf_do_not_split_lines() {
        // A bare line feed must not be read as the end of a line.
        assert!(bad_request(Head::parse("GET / HTTP/1.1\nHost: x")));
        assert!(bad_request(Head::parse(
            "GET / HTTP/1.1\r\nHost: x\nEvil: y"
        )));
        // Nor is a continuation line accepted.
        assert!(bad_request(Head::parse(
            "GET / HTTP/1.1\r\nHost: x\r\n folded"
        )));
    }

    fn framing_of(headers: &str) -> HttpResult<Framing> {
        Head::parse(&format!("POST / HTTP/1.1\r\nHost: x{headers}"))
            .unwrap()
            .framing()
    }

    #[test]
    fn the_headers_say_where_the_body_ends() {
        assert_eq!(framing_of(""), Ok(Framing::None));
        assert_eq!(
            framing_of("\r\nContent-Length: 42"),
            Ok(Framing::Length(42))
        );
        assert_eq!(framing_of("\r\nContent-Length: 0"), Ok(Framing::Length(0)));
        assert_eq!(
            framing_of("\r\nTransfer-Encoding: chunked"),
            Ok(Framing::Chunked)
        );
        assert_eq!(
            framing_of("\r\nTransfer-Encoding: Chunked"),
            Ok(Framing::Chunked)
        );
    }

    #[test]
    fn ambiguous_framing_is_an_error_not_a_guess() {
        // The two classic request smuggling setups.
        let both = framing_of("\r\nContent-Length: 5\r\nTransfer-Encoding: chunked");
        assert!(bad_request(both));
        let differing = framing_of("\r\nContent-Length: 5\r\nContent-Length: 6");
        assert!(bad_request(differing));
        // Identical repeats agree, so they are allowed.
        let same = framing_of("\r\nContent-Length: 5\r\nContent-Length: 5");
        assert_eq!(same, Ok(Framing::Length(5)));
    }

    #[test]
    fn lengths_must_be_plain_decimal_numbers() {
        for bad in ["-1", "+5", "5 5", "0x10", "five", "", "1,1"] {
            assert!(
                bad_request(framing_of(&format!("\r\nContent-Length: {bad}"))),
                "{bad:?}"
            );
        }
        let huge = framing_of("\r\nContent-Length: 99999999999999999999999999");
        assert_eq!(huge, Err(HttpError::BodyTooLarge));
    }

    #[test]
    fn unknown_transfer_encodings_are_not_implemented() {
        let gzip = framing_of("\r\nTransfer-Encoding: gzip, chunked");
        assert!(matches!(gzip, Err(HttpError::NotImplemented(_))));
        let old = Head::parse("POST / HTTP/1.0\r\nTransfer-Encoding: chunked").unwrap();
        assert!(bad_request(old.framing()));
    }
}
