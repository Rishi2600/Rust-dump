use crate::error::{HttpError, HttpResult};
use crate::percent::{form_decode, percent_decode};

/// What a request is aimed at: a path, and the query string after `?`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Decoded and normalised: starts with `/` and contains no `.` or `..`.
    pub path: String,
    /// The `name=value` pairs of the query string, decoded, in order.
    pub query: Vec<(String, String)>,
}

impl Target {
    /// Parse the middle part of a request line, such as `/search?q=rust`.
    pub fn parse(raw: &str) -> HttpResult<Self> {
        // Clients talking to a proxy send a full URL. Keep the part after
        // the host, which is all a server needs.
        let raw = match raw.strip_prefix("http://") {
            Some(rest) => rest.find('/').map_or("/", |slash| &rest[slash..]),
            None => raw,
        };
        if !raw.starts_with('/') {
            return Err(HttpError::BadRequest(format!("`{raw}` is not a path")));
        }
        // A fragment is for the browser. It should never be sent, so drop it.
        let raw = raw.split('#').next().unwrap_or(raw);
        let (path, query) = raw.split_once('?').unwrap_or((raw, ""));
        Ok(Self {
            path: normalize_path(path)?,
            query: parse_query(query)?,
        })
    }

    /// The first value given for `name` in the query string.
    pub fn query_value(&self, name: &str) -> Option<&str> {
        let found = self.query.iter().find(|(key, _)| key == name);
        found.map(|(_, value)| value.as_str())
    }
}

/// Decode a path and resolve its `.` and `..` segments.
///
/// The result can be joined onto a directory without ever leaving it. That
/// matters more than tidiness: `/../../etc/passwd` is the oldest attack on a
/// file server, and `%2e%2e%2f` is the same attack in disguise.
pub fn normalize_path(raw: &str) -> HttpResult<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut ends_in_directory = false;
    for encoded in raw.split('/').skip(1) {
        // Each segment is decoded on its own, after splitting. An encoded
        // slash therefore stays inside its segment, where it is rejected,
        // and can never act as a separator.
        let segment = percent_decode(encoded)?;
        if segment.contains(['/', '\\', '\0']) {
            return Err(HttpError::BadRequest(
                "a path segment hides a separator".into(),
            ));
        }
        ends_in_directory = matches!(segment.as_str(), "" | "." | "..");
        match segment.as_str() {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(HttpError::BadRequest(
                        "the path climbs above the root".into(),
                    ));
                }
            }
            _ => segments.push(segment),
        }
    }
    let mut path = format!("/{}", segments.join("/"));
    if ends_in_directory && !segments.is_empty() {
        path.push('/');
    }
    Ok(path)
}

/// Split `a=1&b=two` into decoded pairs. A name without `=` gets an empty value.
pub fn parse_query(raw: &str) -> HttpResult<Vec<(String, String)>> {
    let pairs = raw.split('&').filter(|pair| !pair.is_empty());
    let decoded = pairs.map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        Ok((form_decode(name)?, form_decode(value)?))
    });
    decoded.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_of(raw: &str) -> HttpResult<String> {
        Target::parse(raw).map(|target| target.path)
    }

    #[test]
    fn a_target_splits_into_path_and_query() {
        let target = Target::parse("/search?q=rust+lang&page=2&debug").unwrap();
        assert_eq!(target.path, "/search");
        assert_eq!(target.query_value("q"), Some("rust lang"));
        assert_eq!(target.query_value("page"), Some("2"));
        assert_eq!(target.query_value("debug"), Some(""));
        assert_eq!(target.query_value("missing"), None);
        assert_eq!(target.query.len(), 3);
    }

    #[test]
    fn paths_are_decoded_and_tidied() {
        assert_eq!(path_of("/"), Ok("/".into()));
        assert_eq!(path_of("/my%20file.txt"), Ok("/my file.txt".into()));
        assert_eq!(path_of("/a//b/./c"), Ok("/a/b/c".into()));
        assert_eq!(path_of("/a/b/../c"), Ok("/a/c".into()));
        // A trailing slash means "the directory" and is kept.
        assert_eq!(path_of("/docs/"), Ok("/docs/".into()));
        assert_eq!(path_of("/docs/sub/.."), Ok("/docs/".into()));
        assert_eq!(path_of("/docs/.."), Ok("/".into()));
    }

    #[test]
    fn a_path_can_never_leave_the_root() {
        let climbing = [
            "/..",
            "/../etc/passwd",
            "/a/../../etc",
            "/%2e%2e/secret",
            "/a/%2E%2E/%2E%2E/b",
        ];
        for attack in climbing {
            assert!(
                matches!(path_of(attack), Err(HttpError::BadRequest(_))),
                "{attack}"
            );
        }
        // Separators hidden by encoding do not get to act as separators.
        for attack in ["/a%2Fb", "/..%2F..%2Fetc", "/a%5Cb", "/a%00b"] {
            assert!(
                matches!(path_of(attack), Err(HttpError::BadRequest(_))),
                "{attack}"
            );
        }
    }

    #[test]
    fn full_urls_and_fragments_are_reduced_to_a_path() {
        assert_eq!(path_of("http://example.com/a/b?x=1"), Ok("/a/b".into()));
        assert_eq!(path_of("http://example.com"), Ok("/".into()));
        assert_eq!(path_of("/page#section"), Ok("/page".into()));
        assert!(path_of("no-slash").is_err());
        assert!(path_of("").is_err());
    }

    #[test]
    fn query_values_are_decoded() {
        let pairs = parse_query("name=J%C3%BCrgen&msg=hi+there&&empty=").unwrap();
        let expected = [("name", "Jürgen"), ("msg", "hi there"), ("empty", "")];
        let expected: Vec<(String, String)> = expected
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        assert_eq!(pairs, expected);
        assert!(parse_query("bad=%zz").is_err());
        assert_eq!(parse_query(""), Ok(Vec::new()));
    }
}
