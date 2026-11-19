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
}
