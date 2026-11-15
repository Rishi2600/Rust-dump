use std::fmt;

/// Everything that can go wrong inside the simulated machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelError {
    OutOfFrames,
    InvalidFrame(usize),
    DoubleFree(usize),
    AlreadyMapped(usize),
    PageFault(usize),
    SegFault(usize),
    StackOverflow,
    InvalidPc(usize),
    DivideByZero,
    BadSyscall(i64),
    NoSuchTask(usize),
    Assembly { line: usize, msg: String },
}

pub type KResult<T> = Result<T, KernelError>;

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfFrames => write!(f, "out of physical frames"),
            Self::InvalidFrame(frame) => write!(f, "invalid frame {frame}"),
            Self::DoubleFree(frame) => write!(f, "frame {frame} freed twice"),
            Self::AlreadyMapped(page) => write!(f, "page {page} is already mapped"),
            Self::PageFault(addr) => write!(f, "page fault at address {addr:#x}"),
            Self::SegFault(addr) => write!(f, "segmentation fault at address {addr:#x}"),
            Self::StackOverflow => write!(f, "stack overflow"),
            Self::InvalidPc(pc) => write!(f, "program counter {pc} is outside the program"),
            Self::DivideByZero => write!(f, "division by zero"),
            Self::BadSyscall(number) => write!(f, "unknown system call {number}"),
            Self::NoSuchTask(pid) => write!(f, "no task with pid {pid}"),
            Self::Assembly { line, msg } => write!(f, "line {line}: {msg}"),
        }
    }
}

impl std::error::Error for KernelError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_render_readable_messages() {
        assert_eq!(
            KernelError::PageFault(0x40).to_string(),
            "page fault at address 0x40"
        );
        let err = KernelError::Assembly {
            line: 3,
            msg: "unknown mnemonic `frob`".into(),
        };
        assert_eq!(err.to_string(), "line 3: unknown mnemonic `frob`");
    }
}
