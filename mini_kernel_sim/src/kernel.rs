use crate::console::Console;
use crate::cpu::Step;
use crate::error::{KResult, KernelError};
use crate::frame::{FrameAllocator, PAGE_SIZE};
use crate::instr::Program;
use crate::memory::PhysicalMemory;
use crate::mmu::Mmu;
use crate::paging::AddressSpace;
use crate::scheduler::RunQueue;
use crate::syscall::Syscall;
use crate::task::{Task, TaskState};

/// Tunable parameters of the simulated machine.
#[derive(Debug, Clone)]
pub struct Config {
    /// Physical frames available to the whole system.
    pub frames: usize,
    /// Size of each task's virtual address space, in pages.
    pub pages_per_task: usize,
    /// Instructions a task may run before it is preempted.
    pub time_slice: u64,
    /// Hard limit on simulated time, so a runaway program cannot hang the host.
    pub max_ticks: u64,
    /// Mirror console output to stdout as it is written.
    pub echo: bool,
    /// Log every executed instruction to stderr.
    pub trace: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frames: 64,
            pages_per_task: 8,
            time_slice: 5,
            max_ticks: 1_000_000,
            echo: false,
            trace: false,
        }
    }
}

/// The kernel: owns physical memory, the task table and the run queue.
pub struct Kernel {
    config: Config,
    frames: FrameAllocator,
    memory: PhysicalMemory,
    tasks: Vec<Task>,
    run_queue: RunQueue,
    console: Console,
    ticks: u64,
    idle_ticks: u64,
}

impl Kernel {
    pub fn new(mut config: Config) -> Self {
        // A zero-length slice would never let any task make progress.
        config.time_slice = config.time_slice.max(1);
        Self {
            frames: FrameAllocator::new(config.frames),
            memory: PhysicalMemory::new(config.frames),
            tasks: Vec::new(),
            run_queue: RunQueue::new(),
            console: Console::new(config.echo),
            ticks: 0,
            idle_ticks: 0,
            config,
        }
    }

    /// Create a task for `program`, load its data segment and make it runnable.
    /// Pids start at 1 and are never reused.
    pub fn spawn(&mut self, name: &str, program: Program) -> KResult<usize> {
        let pid = self.tasks.len() + 1;
        let mut task = Task::new(pid, name, program, self.config.pages_per_task);
        if let Err(fault) = self.load_data(&mut task) {
            unmap_all(&mut self.frames, &mut task.space);
            return Err(fault);
        }
        self.tasks.push(task);
        self.run_queue.enqueue(pid);
        Ok(pid)
    }

    /// Copy the data segment to the bottom of the task's address space.
    fn load_data(&mut self, task: &mut Task) -> KResult<()> {
        for (addr, word) in task.program.data.iter().enumerate() {
            if addr % PAGE_SIZE == 0 {
                let page = addr / PAGE_SIZE;
                map_fresh_page(&mut self.frames, &mut self.memory, &mut task.space, page)?;
            }
            let (frame, offset) = task.space.translate(addr)?;
            self.memory.write(frame, offset, *word)?;
        }
        Ok(())
    }

    pub fn task(&self, pid: usize) -> KResult<&Task> {
        self.tasks
            .get(pid.wrapping_sub(1))
            .ok_or(KernelError::NoSuchTask(pid))
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn console(&self) -> &Console {
        &self.console
    }

    /// Timer ticks since boot.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Ticks during which no task was runnable.
    pub fn idle_ticks(&self) -> u64 {
        self.idle_ticks
    }

    pub fn free_frames(&self) -> usize {
        self.frames.available()
    }
}

impl Kernel {
    /// Execute one instruction of task `pid`, with its memory accesses routed
    /// through the MMU.
    pub fn step_task(&mut self, pid: usize) -> KResult<Step> {
        let Some(task) = self.tasks.get_mut(pid.wrapping_sub(1)) else {
            return Err(KernelError::NoSuchTask(pid));
        };
        let pc = task.context.pc;
        let Some(&instr) = task.program.instrs.get(pc) else {
            return Err(KernelError::InvalidPc(pc));
        };
        loop {
            let mut bus = Mmu::new(&task.space, &mut self.memory);
            match task.context.step(instr, &mut bus) {
                // Demand paging: back the page with a zeroed frame, then
                // restart the instruction that faulted.
                Err(KernelError::PageFault(addr)) => {
                    task.stats.page_faults += 1;
                    let page = addr / PAGE_SIZE;
                    map_fresh_page(&mut self.frames, &mut self.memory, &mut task.space, page)?;
                }
                outcome => return outcome,
            }
        }
    }
}

impl Kernel {
    fn task_mut(&mut self, pid: usize) -> KResult<&mut Task> {
        self.tasks
            .get_mut(pid.wrapping_sub(1))
            .ok_or(KernelError::NoSuchTask(pid))
    }

    /// Service the system call task `pid` just trapped with.
    /// The call number is in `r0` and its argument in `r1`.
    pub fn handle_syscall(&mut self, pid: usize) -> KResult<()> {
        let task = self.task_mut(pid)?;
        task.stats.syscalls += 1;
        let number = task.context.regs[0];
        let arg = task.context.regs[1];
        match Syscall::from_number(number)? {
            Syscall::Exit => self.finish(pid, TaskState::Exited(arg))?,
            Syscall::PrintInt => self.console.write_str(&arg.to_string()),
            Syscall::PrintChar => self.console.write_char(word_to_char(arg)),
            Syscall::PrintStr => {
                let text = self.read_user_string(pid, arg)?;
                self.console.write_str(&text);
            }
            Syscall::Yield => {
                self.task_mut(pid)?.state = TaskState::Ready;
                self.run_queue.enqueue(pid);
            }
            Syscall::GetPid => self.task_mut(pid)?.context.regs[0] = pid as i64,
            _ => return Err(KernelError::BadSyscall(number)),
        }
        Ok(())
    }

    /// Record how a task ended.
    fn finish(&mut self, pid: usize, state: TaskState) -> KResult<()> {
        self.task_mut(pid)?.state = state;
        Ok(())
    }
}

/// Longest string the kernel will copy out of a task in one call.
const MAX_USER_STRING: i64 = 4096;

impl Kernel {
    /// Read one word of a task's memory on the kernel's behalf.
    fn read_user(&self, pid: usize, addr: i64) -> KResult<i64> {
        let task = self.task(pid)?;
        let addr = usize::try_from(addr).map_err(|_| KernelError::SegFault(addr as usize))?;
        match task.space.translate(addr) {
            Ok((frame, offset)) => self.memory.read(frame, offset),
            // A page the task never touched is logically all zeroes.
            Err(KernelError::PageFault(_)) => Ok(0),
            Err(fault) => Err(fault),
        }
    }

    /// Copy a NUL-terminated string out of a task's address space.
    /// A pointer outside the address space is the task's fault, not ours.
    fn read_user_string(&self, pid: usize, addr: i64) -> KResult<String> {
        let mut text = String::new();
        for offset in 0..MAX_USER_STRING {
            let word = self.read_user(pid, addr.saturating_add(offset))?;
            if word == 0 {
                break;
            }
            text.push(word_to_char(word));
        }
        Ok(text)
    }
}

impl Kernel {
    /// Run tasks round-robin until none are runnable or the tick budget is spent.
    pub fn run(&mut self) {
        while self.ticks < self.config.max_ticks {
            match self.run_queue.pick_next() {
                Some(pid) => self.run_slice(pid),
                None => break,
            }
        }
    }

    /// Let one task run until it blocks, finishes or uses up its time slice.
    fn run_slice(&mut self, pid: usize) {
        let Ok(task) = self.task_mut(pid) else {
            return;
        };
        task.state = TaskState::Running;
        task.stats.dispatches += 1;
        for _ in 0..self.config.time_slice {
            self.tick();
            self.tasks[pid - 1].stats.cpu_ticks += 1;
            let outcome = match self.step_task(pid) {
                Ok(Step::Continue) => Ok(()),
                Ok(Step::Syscall) => self.handle_syscall(pid),
                Ok(Step::Halt) => self.finish(pid, TaskState::Exited(0)),
                Err(fault) => Err(fault),
            };
            if let Err(fault) = outcome {
                // A faulting task is killed; everything else keeps running.
                let _ = self.finish(pid, TaskState::Killed(fault));
            }
            if self.tasks[pid - 1].state != TaskState::Running {
                return;
            }
        }
        // Time slice used up: preempt the task and send it to the back of the queue.
        let task = &mut self.tasks[pid - 1];
        task.state = TaskState::Ready;
        task.stats.preemptions += 1;
        self.run_queue.enqueue(pid);
    }

    /// Advance the timer by one tick.
    fn tick(&mut self) {
        self.ticks += 1;
    }
}

/// Back `page` with a freshly zeroed frame.
fn map_fresh_page(
    frames: &mut FrameAllocator,
    memory: &mut PhysicalMemory,
    space: &mut AddressSpace,
    page: usize,
) -> KResult<()> {
    let frame = frames.alloc()?;
    memory.zero_frame(frame)?;
    space.map(page, frame).inspect_err(|_| {
        let _ = frames.free(frame);
    })
}

/// Return every frame mapped in `space` to the allocator.
fn unmap_all(frames: &mut FrameAllocator, space: &mut AddressSpace) {
    for page in 0..space.pages() {
        if let Some(frame) = space.unmap(page) {
            let _ = frames.free(frame);
        }
    }
}

/// Interpret a memory word as a character for the console.
fn word_to_char(word: i64) -> char {
    u32::try_from(word)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or(char::REPLACEMENT_CHARACTER)
}
