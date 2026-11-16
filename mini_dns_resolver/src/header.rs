use std::fmt;

/// Response code: the server's verdict on a query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Rcode {
    /// The query was handled. This does not promise any answer records.
    #[default]
    NoError,
    /// The server could not parse the query.
    FormErr,
    /// The server hit an internal problem.
    ServFail,
    /// The name does not exist at all.
    NxDomain,
    /// The server does not support this kind of query.
    NotImp,
    /// The server will not answer this client or this question.
    Refused,
    Other(u8),
}

impl Rcode {
    pub fn code(self) -> u8 {
        match self {
            Rcode::NoError => 0,
            Rcode::FormErr => 1,
            Rcode::ServFail => 2,
            Rcode::NxDomain => 3,
            Rcode::NotImp => 4,
            Rcode::Refused => 5,
            Rcode::Other(code) => code,
        }
    }

    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Rcode::NoError,
            1 => Rcode::FormErr,
            2 => Rcode::ServFail,
            3 => Rcode::NxDomain,
            4 => Rcode::NotImp,
            5 => Rcode::Refused,
            other => Rcode::Other(other),
        }
    }
}

impl fmt::Display for Rcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rcode::NoError => write!(f, "NOERROR"),
            Rcode::FormErr => write!(f, "FORMERR"),
            Rcode::ServFail => write!(f, "SERVFAIL"),
            Rcode::NxDomain => write!(f, "NXDOMAIN"),
            Rcode::NotImp => write!(f, "NOTIMP"),
            Rcode::Refused => write!(f, "REFUSED"),
            Rcode::Other(code) => write!(f, "RCODE{code}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_codes_round_trip() {
        for code in 0..16 {
            assert_eq!(Rcode::from_code(code).code(), code);
        }
        assert_eq!(Rcode::from_code(3), Rcode::NxDomain);
        assert_eq!(Rcode::NxDomain.to_string(), "NXDOMAIN");
        assert_eq!(Rcode::from_code(9).to_string(), "RCODE9");
    }
}
