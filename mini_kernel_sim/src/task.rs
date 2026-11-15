use std::fmt;

use crate::cpu::Context;
use crate::error::KernelError;
use crate::instr::Program;
use crate::paging::AddressSpace;
use crate::stats::TaskStats;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    /// Waiting in the run queue for a turn on the CPU.
    Ready,
    Running,
    /// Blocked until the timer reaches `wake_at`.
    Sleeping {
        wake_at: u64,
    },
    /// Finished normally with an exit code.
    Exited(i64),
    /// Terminated by the kernel after a fault.
    Killed(KernelError),
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskState::Ready => write!(f, "ready"),
            TaskState::Running => write!(f, "running"),
            TaskState::Sleeping { .. } => write!(f, "sleeping"),
            TaskState::Exited(code) => write!(f, "exited({code})"),
            TaskState::Killed(fault) => write!(f, "killed: {fault}"),
        }
    }
}

/// Task control block: everything the kernel knows about one task.
#[derive(Debug)]
pub struct Task {
    pub pid: usize,
    pub name: String,
    pub state: TaskState,
    pub context: Context,
    pub space: AddressSpace,
    pub program: Program,
    pub stats: TaskStats,
}

impl Task {
    /// Create a ready task with an empty address space of `pages` pages.
    /// The stack starts at the top of the address space and grows down.
    pub fn new(pid: usize, name: &str, program: Program, pages: usize) -> Self {
        let space = AddressSpace::new(pages);
        Self {
            pid,
            name: name.to_string(),
            state: TaskState::Ready,
            context: Context::new(space.size()),
            space,
            program,
            stats: TaskStats::default(),
        }
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.state, TaskState::Exited(_) | TaskState::Killed(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::PAGE_SIZE;

    #[test]
    fn new_task_is_ready_with_stack_at_top_of_address_space() {
        let task = Task::new(3, "init", Program::default(), 4);
        assert_eq!(task.state, TaskState::Ready);
        assert_eq!(task.context.sp, 4 * PAGE_SIZE);
        assert!(task.space.mapped_frames().is_empty());
        assert!(!task.is_finished());
    }

    #[test]
    fn states_describe_themselves() {
        let killed = TaskState::Killed(KernelError::DivideByZero);
        assert_eq!(killed.to_string(), "killed: division by zero");
        assert_eq!(TaskState::Exited(2).to_string(), "exited(2)");
    }
}
