use crate::error::{KResult, KernelError};

/// Services a task can request with the `syscall` instruction.
///
/// The call number goes in `r0` and the argument in `r1`; calls that produce
/// a result leave it in `r0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syscall {
    /// Terminate the task with exit code `r1`.
    Exit = 0,
    /// Print `r1` as a decimal number.
    PrintInt = 1,
    /// Print `r1` as a character.
    PrintChar = 2,
    /// Print the NUL-terminated string at address `r1`.
    PrintStr = 3,
    /// Give up the rest of the time slice.
    Yield = 4,
    /// Return this task's pid.
    GetPid = 5,
    /// Block for `r1` timer ticks.
    Sleep = 6,
    /// Return the number of ticks since boot.
    Uptime = 7,
}

impl Syscall {
    pub const ALL: [Syscall; 8] = [
        Syscall::Exit,
        Syscall::PrintInt,
        Syscall::PrintChar,
        Syscall::PrintStr,
        Syscall::Yield,
        Syscall::GetPid,
        Syscall::Sleep,
        Syscall::Uptime,
    ];

    pub fn number(self) -> i64 {
        self as i64
    }

    pub fn from_number(number: i64) -> KResult<Self> {
        Self::ALL
            .into_iter()
            .find(|call| call.number() == number)
            .ok_or(KernelError::BadSyscall(number))
    }

    pub fn name(self) -> &'static str {
        match self {
            Syscall::Exit => "exit",
            Syscall::PrintInt => "print_int",
            Syscall::PrintChar => "print_char",
            Syscall::PrintStr => "print_str",
            Syscall::Yield => "yield",
            Syscall::GetPid => "getpid",
            Syscall::Sleep => "sleep",
            Syscall::Uptime => "uptime",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_round_trip() {
        for call in Syscall::ALL {
            assert_eq!(Syscall::from_number(call.number()), Ok(call));
        }
        assert_eq!(Syscall::Sleep.name(), "sleep");
    }

    #[test]
    fn unknown_numbers_are_rejected() {
        assert_eq!(Syscall::from_number(8), Err(KernelError::BadSyscall(8)));
        assert_eq!(Syscall::from_number(-1), Err(KernelError::BadSyscall(-1)));
    }
}
