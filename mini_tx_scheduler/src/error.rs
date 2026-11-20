use std::fmt;

/// Everything that can stop a transaction, or a scenario, from being used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedError {
    // A transaction that is malformed in itself.
    Invalid(String),

    // Reasons the pool will not take a transaction.
    Duplicate(u64),
    FeeTooLow {
        fee: u64,
        minimum: u64,
    },
    StaleNonce {
        sender: String,
        nonce: u64,
        expected: u64,
    },
    NonceTooFar {
        sender: String,
        nonce: u64,
        expected: u64,
    },
    CannotPayFee {
        sender: String,
        balance: u64,
        fee: u64,
    },
    ReplacementUnderpriced {
        fee: u64,
        needed: u64,
    },
    PoolFull,

    // The ledger.
    InsufficientFunds {
        account: String,
        needed: u64,
        available: u64,
    },
    Overflow(String),

    // Scenario files.
    Parse {
        line: usize,
        msg: String,
    },
    Io(String),
}

pub type SchedResult<T> = Result<T, SchedError>;

impl fmt::Display for SchedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => write!(f, "invalid: {reason}"),
            Self::Duplicate(id) => write!(f, "transaction {id} is already in the pool"),
            Self::FeeTooLow { fee, minimum } => {
                write!(f, "fee {fee} is below the minimum of {minimum}")
            }
            Self::StaleNonce {
                sender,
                nonce,
                expected,
            } => {
                write!(
                    f,
                    "{sender} already used nonce {nonce}; the next one is {expected}"
                )
            }
            Self::NonceTooFar {
                sender,
                nonce,
                expected,
            } => {
                write!(
                    f,
                    "nonce {nonce} is too far ahead of {sender}'s next nonce, {expected}"
                )
            }
            Self::CannotPayFee {
                sender,
                balance,
                fee,
            } => {
                write!(f, "{sender} has {balance}, not enough for a fee of {fee}")
            }
            Self::ReplacementUnderpriced { fee, needed } => {
                write!(
                    f,
                    "a replacement must pay at least {needed}, this one pays {fee}"
                )
            }
            Self::PoolFull => write!(
                f,
                "the pool is full and this transaction does not outbid anything in it"
            ),
            Self::InsufficientFunds {
                account,
                needed,
                available,
            } => {
                write!(f, "{account} has {available}, {needed} is needed")
            }
            Self::Overflow(account) => write!(f, "the balance of {account} would overflow"),
            Self::Parse { line, msg } => write!(f, "line {line}: {msg}"),
            Self::Io(reason) => write!(f, "cannot read the file: {reason}"),
        }
    }
}

impl std::error::Error for SchedError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_render_readable_messages() {
        let stale = SchedError::StaleNonce {
            sender: "alice".into(),
            nonce: 2,
            expected: 5,
        };
        assert_eq!(
            stale.to_string(),
            "alice already used nonce 2; the next one is 5"
        );
        let low = SchedError::FeeTooLow {
            fee: 1,
            minimum: 10,
        };
        assert_eq!(low.to_string(), "fee 1 is below the minimum of 10");
        let parse = SchedError::Parse {
            line: 7,
            msg: "expected a number".into(),
        };
        assert_eq!(parse.to_string(), "line 7: expected a number");
    }
}
