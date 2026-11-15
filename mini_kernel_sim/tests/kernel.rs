use mini_kernel_sim::assembler::assemble;
use mini_kernel_sim::cpu::Step;
use mini_kernel_sim::error::KernelError;
use mini_kernel_sim::kernel::{Config, Kernel};
use mini_kernel_sim::task::TaskState;

/// Boot a kernel with one task per source program.
fn boot(config: Config, sources: &[&str]) -> Kernel {
    let mut kernel = Kernel::new(config);
    for (index, source) in sources.iter().enumerate() {
        let program = assemble(source).expect("program assembles");
        let name = format!("task{}", index + 1);
        kernel.spawn(&name, program).expect("task spawns");
    }
    kernel
}

#[test]
fn spawn_assigns_pids_and_loads_the_data_segment() {
    let kernel = boot(Config::default(), &["msg: .string \"hi\"\nhalt", "halt"]);
    assert_eq!(kernel.tasks().len(), 2);
    assert_eq!(kernel.task(2).unwrap().pid, 2);
    assert_eq!(kernel.task(1).unwrap().state, TaskState::Ready);
    // Only the first task has data, so exactly one frame is in use.
    assert_eq!(kernel.free_frames(), Config::default().frames - 1);
}

#[test]
fn spawn_fails_cleanly_when_memory_runs_out() {
    let two_pages_of_data = assemble("buf: .zero 100\nhalt").unwrap();

    let mut kernel = Kernel::new(Config {
        frames: 1,
        ..Config::default()
    });
    let spawned = kernel.spawn("big", two_pages_of_data.clone());
    assert_eq!(spawned, Err(KernelError::OutOfFrames));
    assert_eq!(kernel.free_frames(), 1);
    assert!(kernel.tasks().is_empty());

    // Data that does not fit in the address space is rejected too.
    let mut kernel = Kernel::new(Config {
        pages_per_task: 1,
        ..Config::default()
    });
    let spawned = kernel.spawn("big", two_pages_of_data);
    assert_eq!(spawned, Err(KernelError::SegFault(64)));
    assert_eq!(kernel.free_frames(), Config::default().frames);
}

#[test]
fn step_task_executes_one_instruction_at_a_time() {
    let mut kernel = boot(Config::default(), &["mov r3, 42\nhalt"]);
    assert_eq!(kernel.step_task(1), Ok(Step::Continue));
    let context = &kernel.task(1).unwrap().context;
    assert_eq!((context.regs[3], context.pc), (42, 1));
    assert_eq!(kernel.step_task(1), Ok(Step::Halt));
    // The program counter has now run off the end of the program.
    assert_eq!(kernel.step_task(1), Err(KernelError::InvalidPc(2)));
    assert_eq!(kernel.step_task(9), Err(KernelError::NoSuchTask(9)));
}
