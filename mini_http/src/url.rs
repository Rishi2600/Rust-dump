use crate::error::{HttpError, HttpResult};

/// The parts of an `http://` address a client needs.
///
/// ```text
/// http://example.com:8080/search?q=rust
///        \_________/ \__/\____________/
///           host     port     path
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    /// A name to look up, or an address.
    pub host: String,
    pub port: u16,
    /// The path and query, exactly as they are sent on the request line.
    pub path: String,
}

impl Url {
    pub fn parse(text: &str) -> HttpResult<Self> {
        let bad = |reason: &str| HttpError::BadUrl(format!("`{text}` {reason}"));
        let rest = match text.strip_prefix("http://") {
            Some(rest) => rest,
            None if text.starts_with("https://") => {
                return Err(bad("needs TLS, which this client does not have"));
            }
            None => return Err(bad("does not start with http://")),
        };
        // The fragment is for the reader of the page and is never sent.
        let rest = rest.split('#').next().unwrap_or(rest);
        let split = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, path) = rest.split_at(split);
        if authority.contains('@') {
            return Err(bad("has a user name, which is not supported"));
        }

        let (host, port) = match authority.strip_prefix('[') {
            // An IPv6 address is full of colons, so it is wrapped in brackets
            // to keep them apart from the one before the port.
            Some(bracketed) => {
                let (host, after) = bracketed
                    .split_once(']')
                    .ok_or_else(|| bad("has an unclosed ["))?;
                (host, after.strip_prefix(':'))
            }
            None => match authority.rsplit_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            },
        };
        let port = match port {
            Some(port) => port.parse().map_err(|_| bad("has an invalid port"))?,
            None => 80,
        };
        let valid_host = !host.is_empty()
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-.:_".contains(&byte));
        if !valid_host {
            return Err(bad("has no usable host name"));
        }
        let path = match path {
            "" => "/".to_string(),
            query if query.starts_with('?') => format!("/{query}"),
            path => path.to_string(),
        };
        Ok(Self {
            host: host.to_string(),
            port,
            path,
        })
    }

    /// The value for the `Host` header: the host, and the port unless it is
    /// the default one.
    pub fn authority(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == 80 {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(text: &str) -> (String, u16, String) {
        let url = Url::parse(text).unwrap();
        (url.host, url.port, url.path)
    }

    #[test]
    fn a_url_splits_into_host_port_and_path() {
        assert_eq!(
            parts("http://example.com/a/b?x=1"),
            ("example.com".into(), 80, "/a/b?x=1".into())
        );
        assert_eq!(
            parts("http://localhost:8080/"),
            ("localhost".into(), 8080, "/".into())
        );
        assert_eq!(
            parts("http://127.0.0.1:3000/x"),
            ("127.0.0.1".into(), 3000, "/x".into())
        );
    }

    #[test]
    fn missing_pieces_get_their_defaults() {
        assert_eq!(
            parts("http://example.com"),
            ("example.com".into(), 80, "/".into())
        );
        assert_eq!(
            parts("http://example.com?q=1"),
            ("example.com".into(), 80, "/?q=1".into())
        );
        assert_eq!(
            parts("http://example.com/page#part"),
            ("example.com".into(), 80, "/page".into())
        );
    }

    #[test]
    fn ipv6_addresses_are_written_in_brackets() {
        assert_eq!(
            parts("http://[::1]:8080/x"),
            ("::1".into(), 8080, "/x".into())
        );
        assert_eq!(
            parts("http://[2001:db8::1]/"),
            ("2001:db8::1".into(), 80, "/".into())
        );
        assert_eq!(
            Url::parse("http://[::1]:8080/").unwrap().authority(),
            "[::1]:8080"
        );
    }

    #[test]
    fn the_host_header_omits_the_default_port() {
        assert_eq!(
            Url::parse("http://example.com/").unwrap().authority(),
            "example.com"
        );
        assert_eq!(
            Url::parse("http://example.com:80/").unwrap().authority(),
            "example.com"
        );
        assert_eq!(
            Url::parse("http://example.com:8080/").unwrap().authority(),
            "example.com:8080"
        );
    }

    #[test]
    fn unusable_urls_are_rejected_with_a_reason() {
        for bad in [
            "example.com",
            "ftp://example.com/",
            "https://example.com/",
            "http://",
            "http:///path",
            "http://example.com:port/",
            "http://example.com:99999/",
            "http://user@example.com/",
            "http://exa mple.com/",
            "http://[::1/",
        ] {
            assert!(
                matches!(Url::parse(bad), Err(HttpError::BadUrl(_))),
                "{bad}"
            );
        }
        let https = Url::parse("https://example.com/").unwrap_err().to_string();
        assert!(https.contains("TLS"), "{https}");
    }
}
