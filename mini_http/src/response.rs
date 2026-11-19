use std::fmt;

use crate::headers::Headers;
use crate::status::StatusCode;

/// What a response carries after its headers.
pub enum Body {
    /// Content held in memory, whose length is therefore known.
    Bytes(Vec<u8>),
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Body::Bytes(bytes) => write!(f, "Bytes({} bytes)", bytes.len()),
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

    /// The body, when it is held in memory.
    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.body {
            Body::Bytes(bytes) => Some(bytes),
        }
    }
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
}
