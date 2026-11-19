use std::fmt;
use std::str::FromStr;

use crate::error::{HttpError, HttpResult};

/// What the client wants done to the resource it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Method {
    /// Fetch it.
    Get,
    /// Fetch only the headers it would be sent with.
    Head,
    /// Hand it some data to process.
    Post,
    /// Replace it with the data sent.
    Put,
    /// Remove it.
    Delete,
    /// Ask what can be done with it.
    Options,
    /// Change part of it.
    Patch,
}

impl Method {
    pub const ALL: [Method; 7] = [
        Method::Get,
        Method::Head,
        Method::Post,
        Method::Put,
        Method::Delete,
        Method::Options,
        Method::Patch,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
            Method::Patch => "PATCH",
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Method {
    type Err = HttpError;

    /// Method names are case-sensitive: `get` is not `GET`.
    fn from_str(text: &str) -> HttpResult<Self> {
        if let Some(method) = Self::ALL.into_iter().find(|method| method.as_str() == text) {
            return Ok(method);
        }
        let is_word = !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_alphabetic());
        if is_word {
            // A well-formed method this server simply does not know.
            Err(HttpError::NotImplemented(format!("the method `{text}`")))
        } else {
            Err(HttpError::BadRequest(format!("`{text}` is not a method")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_round_trip_through_their_names() {
        for method in Method::ALL {
            assert_eq!(method.as_str().parse(), Ok(method));
            assert_eq!(method.to_string(), method.as_str());
        }
    }

    #[test]
    fn unknown_and_malformed_methods_are_told_apart() {
        assert!(matches!(
            "get".parse::<Method>(),
            Err(HttpError::NotImplemented(_))
        ));
        assert!(matches!(
            "BREW".parse::<Method>(),
            Err(HttpError::NotImplemented(_))
        ));
        assert!(matches!(
            "G=T".parse::<Method>(),
            Err(HttpError::BadRequest(_))
        ));
        assert!(matches!(
            "".parse::<Method>(),
            Err(HttpError::BadRequest(_))
        ));
    }
}
