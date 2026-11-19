use std::fmt;

use crate::error::HttpError;

/// The three-digit result of a request. The first digit is its class:
/// 1 informational, 2 success, 3 redirection, 4 client error, 5 server error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusCode(pub u16);

impl StatusCode {
    pub const CONTINUE: Self = Self(100);
    pub const OK: Self = Self(200);
    pub const CREATED: Self = Self(201);
    pub const NO_CONTENT: Self = Self(204);
    pub const PARTIAL_CONTENT: Self = Self(206);
    pub const MOVED_PERMANENTLY: Self = Self(301);
    pub const FOUND: Self = Self(302);
    pub const NOT_MODIFIED: Self = Self(304);
    pub const BAD_REQUEST: Self = Self(400);
    pub const FORBIDDEN: Self = Self(403);
    pub const NOT_FOUND: Self = Self(404);
    pub const METHOD_NOT_ALLOWED: Self = Self(405);
    pub const REQUEST_TIMEOUT: Self = Self(408);
    pub const CONTENT_TOO_LARGE: Self = Self(413);
    pub const RANGE_NOT_SATISFIABLE: Self = Self(416);
    pub const HEADER_FIELDS_TOO_LARGE: Self = Self(431);
    pub const INTERNAL_SERVER_ERROR: Self = Self(500);
    pub const NOT_IMPLEMENTED: Self = Self(501);
    pub const VERSION_NOT_SUPPORTED: Self = Self(505);

    /// The words that follow the number on the status line. They are for
    /// people; programs go by the number.
    pub fn reason(self) -> &'static str {
        match self.0 {
            100 => "Continue",
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            206 => "Partial Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            400 => "Bad Request",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            408 => "Request Timeout",
            413 => "Content Too Large",
            416 => "Range Not Satisfiable",
            431 => "Request Header Fields Too Large",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            505 => "HTTP Version Not Supported",
            100..=199 => "Informational",
            200..=299 => "Success",
            300..=399 => "Redirection",
            400..=499 => "Client Error",
            _ => "Server Error",
        }
    }

    pub fn is_success(self) -> bool {
        (200..300).contains(&self.0)
    }

    /// Some responses never carry a body, whatever the handler produced:
    /// informational ones, 204 No Content and 304 Not Modified.
    pub fn allows_body(self) -> bool {
        !(self.0 < 200 || self.0 == 204 || self.0 == 304)
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.reason())
    }
}

impl HttpError {
    /// The status to answer with when a request fails this way, or `None`
    /// when there is nobody to answer or nothing sensible to say.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::BadRequest(_) | Self::UnexpectedEof | Self::LimitExceeded => {
                Some(StatusCode::BAD_REQUEST)
            }
            Self::HeadersTooLarge => Some(StatusCode::HEADER_FIELDS_TOO_LARGE),
            Self::BodyTooLarge => Some(StatusCode::CONTENT_TOO_LARGE),
            Self::UnsupportedVersion(_) => Some(StatusCode::VERSION_NOT_SUPPORTED),
            Self::NotImplemented(_) => Some(StatusCode::NOT_IMPLEMENTED),
            Self::Timeout => Some(StatusCode::REQUEST_TIMEOUT),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_prints_as_number_and_reason() {
        assert_eq!(StatusCode::NOT_FOUND.to_string(), "404 Not Found");
        assert_eq!(StatusCode::OK.to_string(), "200 OK");
        // A code without a phrase of its own falls back to its class.
        assert_eq!(StatusCode(418).to_string(), "418 Client Error");
        assert_eq!(StatusCode(599).reason(), "Server Error");
    }

    #[test]
    fn some_statuses_never_have_a_body() {
        assert!(StatusCode::OK.allows_body());
        assert!(StatusCode::NOT_FOUND.allows_body());
        for bodiless in [
            StatusCode::CONTINUE,
            StatusCode::NO_CONTENT,
            StatusCode::NOT_MODIFIED,
        ] {
            assert!(!bodiless.allows_body(), "{bodiless}");
        }
        assert!(StatusCode::CREATED.is_success() && !StatusCode::FOUND.is_success());
    }

    #[test]
    fn request_errors_know_which_status_to_send() {
        let too_big = HttpError::BodyTooLarge.status();
        assert_eq!(too_big, Some(StatusCode::CONTENT_TOO_LARGE));
        let bad = HttpError::BadRequest("x".into()).status();
        assert_eq!(bad, Some(StatusCode::BAD_REQUEST));
        // A client that simply went away is not sent anything.
        assert_eq!(HttpError::Closed.status(), None);
        assert_eq!(HttpError::Io("reset".into()).status(), None);
    }
}
