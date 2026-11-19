use std::fmt;
use std::io;

/// Everything that can go wrong while speaking HTTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    // A request the server cannot accept. Each has a status code to send back.
    BadRequest(String),
    HeadersTooLarge,
    BodyTooLarge,
    UnsupportedVersion(String),
    NotImplemented(String),
    Timeout,

    // The stream underneath.
    /// The other side closed the connection between messages. Not a fault.
    Closed,
    /// The other side closed the connection in the middle of a message.
    UnexpectedEof,
    /// More bytes arrived than the caller was prepared to hold.
    LimitExceeded,
    Io(String),

    // The client side.
    BadUrl(String),
    BadResponse(String),
    Resolve(String),
}

pub type HttpResult<T> = Result<T, HttpError>;

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest(reason) => write!(f, "malformed request: {reason}"),
            Self::HeadersTooLarge => write!(f, "the request headers are too large"),
            Self::BodyTooLarge => write!(f, "the request body is too large"),
            Self::UnsupportedVersion(version) => write!(f, "unsupported HTTP version `{version}`"),
            Self::NotImplemented(what) => write!(f, "{what} is not implemented"),
            Self::Timeout => write!(f, "timed out waiting for data"),
            Self::Closed => write!(f, "the connection was closed"),
            Self::UnexpectedEof => write!(f, "the connection closed in the middle of a message"),
            Self::LimitExceeded => write!(f, "more data arrived than is allowed"),
            Self::Io(reason) => write!(f, "network error: {reason}"),
            Self::BadUrl(reason) => write!(f, "invalid URL: {reason}"),
            Self::BadResponse(reason) => write!(f, "malformed response: {reason}"),
            Self::Resolve(reason) => write!(f, "cannot find the host: {reason}"),
        }
    }
}

impl std::error::Error for HttpError {}

impl From<io::Error> for HttpError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            // A read timeout surfaces as either of these, depending on the platform.
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => Self::Timeout,
            io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            _ => Self::Io(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_render_readable_messages() {
        let bad = HttpError::BadRequest("no colon in header line".into());
        assert_eq!(
            bad.to_string(),
            "malformed request: no colon in header line"
        );
        assert_eq!(HttpError::Timeout.to_string(), "timed out waiting for data");
    }

    #[test]
    fn io_errors_are_sorted_into_the_cases_callers_care_about() {
        let convert = |kind: io::ErrorKind| HttpError::from(io::Error::from(kind));
        assert_eq!(convert(io::ErrorKind::TimedOut), HttpError::Timeout);
        assert_eq!(convert(io::ErrorKind::WouldBlock), HttpError::Timeout);
        assert_eq!(
            convert(io::ErrorKind::UnexpectedEof),
            HttpError::UnexpectedEof
        );
        assert!(matches!(
            convert(io::ErrorKind::ConnectionReset),
            HttpError::Io(_)
        ));
    }
}
