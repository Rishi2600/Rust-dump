/// The header fields of a message: names and values, in the order received.
///
/// Names are matched without regard to case, so `Content-Length`,
/// `content-length` and `CONTENT-LENGTH` are the same header. A name may
/// appear more than once, which is why this is a list and not a map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers {
    fields: Vec<(String, String)>,
}

impl Headers {
    pub fn new() -> Self {
        Self::default()
    }

    /// The first value of `name`.
    pub fn get(&self, name: &str) -> Option<&str> {
        let found = self
            .fields
            .iter()
            .find(|(field, _)| field.eq_ignore_ascii_case(name));
        found.map(|(_, value)| value.as_str())
    }

    /// Every value of `name`, in order.
    pub fn get_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        let matching = self
            .fields
            .iter()
            .filter(move |(field, _)| field.eq_ignore_ascii_case(name));
        matching.map(|(_, value)| value.as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Add a field, keeping any that already have this name.
    pub fn append(&mut self, name: &str, value: &str) {
        self.fields.push((name.to_string(), sanitize(value)));
    }

    /// Set a field, replacing any that already have this name.
    pub fn set(&mut self, name: &str, value: &str) {
        self.remove(name);
        self.append(name, value);
    }

    pub fn remove(&mut self, name: &str) {
        self.fields
            .retain(|(field, _)| !field.eq_ignore_ascii_case(name));
    }

    /// Whether `name` lists `token` among its comma-separated values, as in
    /// `Connection: keep-alive, Upgrade`.
    pub fn has_token(&self, name: &str, token: &str) -> bool {
        let mut listed = self.get_all(name).flat_map(|value| value.split(','));
        listed.any(|item| item.trim().eq_ignore_ascii_case(token))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.fields
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

/// Make a value safe to write on a single header line.
///
/// Headers end at a line break. If a value could contain one, whoever
/// controls the value could finish the header early and write headers, or a
/// whole second response, of their own. Line breaks are replaced with spaces
/// so that can never happen.
fn sanitize(value: &str) -> String {
    let flattened: String = value
        .chars()
        .map(|ch| {
            if matches!(ch, '\r' | '\n' | '\0') {
                ' '
            } else {
                ch
            }
        })
        .collect();
    flattened.trim().to_string()
}

/// Whether `text` is a valid header name: one or more "token" characters.
pub fn is_token(text: &str) -> bool {
    let allowed = |byte: u8| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte);
    !text.is_empty() && text.bytes().all(allowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_matched_without_regard_to_case() {
        let mut headers = Headers::new();
        headers.append("Content-Type", "text/html");
        assert_eq!(headers.get("content-type"), Some("text/html"));
        assert_eq!(headers.get("CONTENT-TYPE"), Some("text/html"));
        assert!(headers.contains("Content-type"));
        assert_eq!(headers.get("Content-Length"), None);
    }

    #[test]
    fn a_name_can_repeat_and_set_replaces_all_of_them() {
        let mut headers = Headers::new();
        headers.append("Set-Cookie", "a=1");
        headers.append("set-cookie", "b=2");
        assert_eq!(
            headers.get_all("Set-Cookie").collect::<Vec<_>>(),
            ["a=1", "b=2"]
        );
        assert_eq!(headers.get("Set-Cookie"), Some("a=1"));

        headers.set("SET-COOKIE", "c=3");
        assert_eq!(headers.get_all("Set-Cookie").collect::<Vec<_>>(), ["c=3"]);
        headers.remove("set-cookie");
        assert!(headers.is_empty());
    }

    #[test]
    fn comma_separated_tokens_can_be_searched() {
        let mut headers = Headers::new();
        headers.append("Connection", "keep-alive, Upgrade");
        headers.append("Connection", "close");
        assert!(headers.has_token("connection", "upgrade"));
        assert!(headers.has_token("Connection", "CLOSE"));
        assert!(!headers.has_token("Connection", "keep"));
    }

    #[test]
    fn a_value_cannot_smuggle_in_a_second_header_line() {
        let mut headers = Headers::new();
        headers.set("Location", "/home\r\nSet-Cookie: stolen=1");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers.get("Location"), Some("/home  Set-Cookie: stolen=1"));
        assert_eq!(headers.iter().collect::<Vec<_>>().len(), 1);
    }

    #[test]
    fn header_names_are_tokens() {
        assert!(is_token("Content-Type"));
        assert!(is_token("X-Custom_1"));
        for bad in [
            "",
            "Content Type",
            "Content-Type ",
            "Name:",
            "Na\tme",
            "Größe",
        ] {
            assert!(!is_token(bad), "{bad:?}");
        }
    }
}
