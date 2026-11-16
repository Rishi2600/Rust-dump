use std::fmt;
use std::io;

/// Everything that can go wrong between a name and its records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsError {
    // Decoding and encoding messages.
    UnexpectedEof,
    LabelTooLong(usize),
    NameTooLong,
    BadLabel(u8),
    BadPointer(usize),
    CompressionLoop,
    InvalidName(String),
    BadRecord(String),

    // Talking to servers.
    Timeout,
    Io(String),
    BadResponse(String),

    // Resolving a name.
    NxDomain(String),
    ServerFailure(String),
    NoNameservers(String),
    TooManyReferrals,
    TooManyCnames,
    DepthExceeded,
}

pub type DnsResult<T> = Result<T, DnsError>;

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "message ended unexpectedly"),
            Self::LabelTooLong(len) => write!(f, "label is {len} bytes long, the limit is 63"),
            Self::NameTooLong => write!(f, "name is longer than 255 bytes"),
            Self::BadLabel(byte) => write!(f, "unsupported label type {byte:#04x}"),
            Self::BadPointer(offset) => {
                write!(
                    f,
                    "compression pointer to offset {offset} does not point backwards"
                )
            }
            Self::CompressionLoop => write!(f, "compression pointers form a loop"),
            Self::InvalidName(reason) => write!(f, "invalid name: {reason}"),
            Self::BadRecord(reason) => write!(f, "malformed record: {reason}"),
            Self::Timeout => write!(f, "timed out waiting for a reply"),
            Self::Io(reason) => write!(f, "network error: {reason}"),
            Self::BadResponse(reason) => write!(f, "unacceptable response: {reason}"),
            Self::NxDomain(name) => write!(f, "{name} does not exist"),
            Self::ServerFailure(reason) => write!(f, "server failure: {reason}"),
            Self::NoNameservers(zone) => write!(f, "no reachable nameserver for {zone}"),
            Self::TooManyReferrals => write!(f, "gave up after too many referrals"),
            Self::TooManyCnames => write!(f, "gave up after too many CNAME redirections"),
            Self::DepthExceeded => write!(f, "nameserver lookups nested too deeply"),
        }
    }
}

impl std::error::Error for DnsError {}

impl From<io::Error> for DnsError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            // A read timeout surfaces as either of these, depending on the platform.
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => Self::Timeout,
            _ => Self::Io(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_render_readable_messages() {
        let missing = DnsError::NxDomain("nope.example.com.".into());
        assert_eq!(missing.to_string(), "nope.example.com. does not exist");
        assert_eq!(
            DnsError::BadLabel(0x40).to_string(),
            "unsupported label type 0x40"
        );
    }

    #[test]
    fn io_timeouts_become_dns_timeouts() {
        let timed_out = io::Error::from(io::ErrorKind::TimedOut);
        assert_eq!(DnsError::from(timed_out), DnsError::Timeout);
        let refused = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert!(matches!(DnsError::from(refused), DnsError::Io(_)));
    }
}
