use std::fmt;
use std::io;

/// Everything that can go wrong between a path and the bytes on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    // The disk and the image on it.
    Io(String),
    OutOfRange(u32),
    BadImage(String),
    Corrupt(String),
    InvalidOptions(String),
    TransactionTooLarge,

    // Running out of room.
    NoSpace,
    NoInodes,
    FileTooLarge,
    TooManyLinks,

    // Names and paths.
    NotFound(String),
    AlreadyExists(String),
    NotADirectory(String),
    IsADirectory(String),
    DirectoryNotEmpty(String),
    InvalidName(String),
    InvalidPath(String),
}

pub type FsResult<T> = Result<T, FsError>;

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(reason) => write!(f, "disk error: {reason}"),
            Self::OutOfRange(block) => write!(f, "block {block} is outside the disk"),
            Self::BadImage(reason) => write!(f, "not a usable image: {reason}"),
            Self::Corrupt(reason) => write!(f, "file system is corrupt: {reason}"),
            Self::InvalidOptions(reason) => write!(f, "cannot format: {reason}"),
            Self::TransactionTooLarge => write!(f, "operation does not fit in the journal"),
            Self::NoSpace => write!(f, "no free blocks left"),
            Self::NoInodes => write!(f, "no free inodes left"),
            Self::FileTooLarge => write!(f, "file would exceed the maximum file size"),
            Self::TooManyLinks => write!(f, "too many links to one file"),
            Self::NotFound(path) => write!(f, "{path}: no such file or directory"),
            Self::AlreadyExists(path) => write!(f, "{path}: already exists"),
            Self::NotADirectory(path) => write!(f, "{path}: not a directory"),
            Self::IsADirectory(path) => write!(f, "{path}: is a directory"),
            Self::DirectoryNotEmpty(path) => write!(f, "{path}: directory is not empty"),
            Self::InvalidName(reason) => write!(f, "invalid name: {reason}"),
            Self::InvalidPath(reason) => write!(f, "invalid path: {reason}"),
        }
    }
}

impl std::error::Error for FsError {}

impl From<io::Error> for FsError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_render_readable_messages() {
        let missing = FsError::NotFound("/docs/a.txt".into());
        assert_eq!(
            missing.to_string(),
            "/docs/a.txt: no such file or directory"
        );
        assert_eq!(
            FsError::OutOfRange(99).to_string(),
            "block 99 is outside the disk"
        );
    }

    #[test]
    fn io_errors_convert_with_the_question_mark() {
        fn open_missing() -> FsResult<std::fs::File> {
            Ok(std::fs::File::open("/definitely/not/here")?)
        }
        assert!(matches!(open_missing(), Err(FsError::Io(_))));
    }
}
