use std::fmt;
use std::io;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum AppError {
    Io(io::Error),
    Parse(ParseIntError),
    Custom(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "IO Error: {}", e),
            AppError::Parse(e) => write!(f, "Parse Error: {}", e),
            AppError::Custom(msg) => write!(f, "Application Error: {}", msg),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self { AppError::Io(err) }
}

impl From<ParseIntError> for AppError {
    fn from(err: ParseIntError) -> Self { AppError::Parse(err) }
}