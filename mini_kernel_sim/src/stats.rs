/// Per-task accounting, updated by the kernel as the task runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TaskStats {
    /// Timer ticks spent executing this task's instructions.
    pub cpu_ticks: u64,
    /// System calls made.
    pub syscalls: u64,
    /// Page faults resolved by demand paging.
    pub page_faults: u64,
    /// Times the scheduler put the task on the CPU.
    pub dispatches: u64,
    /// Times the task was taken off the CPU because its time slice ran out.
    pub preemptions: u64,
}
