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

#[test]
fn first_touch_maps_a_zeroed_frame() {
    let source = "mov r0, 9\n store r0, 200\n load r1, 200\n load r2, 201\n halt";
    let mut kernel = boot(Config::default(), &[source]);
    let free_before = kernel.free_frames();
    for _ in 0..4 {
        kernel.step_task(1).unwrap();
    }
    let task = kernel.task(1).unwrap();
    assert_eq!((task.context.regs[1], task.context.regs[2]), (9, 0));
    assert_eq!(task.stats.page_faults, 1);
    assert_eq!(kernel.free_frames(), free_before - 1);
}

#[test]
fn stack_pages_are_mapped_on_demand() {
    let mut kernel = boot(Config::default(), &["push 5\n pop r3\n halt"]);
    kernel.step_task(1).unwrap();
    kernel.step_task(1).unwrap();
    let task = kernel.task(1).unwrap();
    assert_eq!(task.context.regs[3], 5);
    assert_eq!(task.stats.page_faults, 1);
}

#[test]
fn access_outside_the_address_space_is_a_segfault() {
    let mut kernel = boot(Config::default(), &["load r0, 100000"]);
    assert_eq!(kernel.step_task(1), Err(KernelError::SegFault(100000)));
    assert_eq!(kernel.task(1).unwrap().stats.page_faults, 0);
}

#[test]
fn demand_paging_fails_when_memory_is_exhausted() {
    let config = Config {
        frames: 1,
        ..Config::default()
    };
    let mut kernel = boot(config, &["store r0, 0\n store r0, 100"]);
    kernel.step_task(1).unwrap();
    assert_eq!(kernel.step_task(1), Err(KernelError::OutOfFrames));
}

/// Drive one task by hand until it finishes, without the scheduler.
fn drive(kernel: &mut Kernel, pid: usize) -> Result<(), KernelError> {
    while !kernel.task(pid)?.is_finished() {
        match kernel.step_task(pid)? {
            Step::Continue => {}
            Step::Syscall => kernel.handle_syscall(pid)?,
            Step::Halt => break,
        }
    }
    Ok(())
}

#[test]
fn exit_and_console_syscalls() {
    let source = "mov r0, 1\n mov r1, 42\n syscall\n mov r0, 2\n mov r1, '!'\n syscall\n mov r0, 0\n mov r1, 7\n syscall";
    let mut kernel = boot(Config::default(), &[source]);
    drive(&mut kernel, 1).unwrap();
    assert_eq!(kernel.console().output(), "42!");
    let task = kernel.task(1).unwrap();
    assert_eq!(task.state, TaskState::Exited(7));
    assert_eq!(task.stats.syscalls, 3);
}

#[test]
fn unknown_syscall_is_rejected() {
    let mut kernel = boot(Config::default(), &["mov r0, 99\n syscall"]);
    assert_eq!(drive(&mut kernel, 1), Err(KernelError::BadSyscall(99)));
}

#[test]
fn print_str_reads_from_user_memory() {
    let source = "msg: .string \"hi there\\n\"\n mov r0, 3\n mov r1, msg\n syscall\n halt";
    let mut kernel = boot(Config::default(), &[source]);
    drive(&mut kernel, 1).unwrap();
    assert_eq!(kernel.console().output(), "hi there\n");
}

#[test]
fn print_str_checks_the_pointer_it_is_given() {
    // Far outside the address space: the call fails with a segfault.
    let mut kernel = boot(Config::default(), &["mov r0, 3\n mov r1, 100000\n syscall"]);
    assert_eq!(drive(&mut kernel, 1), Err(KernelError::SegFault(100000)));

    // Inside the address space but never touched: reads as an empty string.
    let mut kernel = boot(
        Config::default(),
        &["mov r0, 3\n mov r1, 300\n syscall\n halt"],
    );
    assert_eq!(drive(&mut kernel, 1), Ok(()));
    assert_eq!(kernel.console().output(), "");
}

#[test]
fn run_interleaves_tasks_with_time_slices() {
    // Each task prints its letter three times; a 4-tick slice ends right
    // after each print, so the letters alternate.
    let source = |letter: char| {
        format!(
            "mov r5, 3\nloop: mov r0, 2\n mov r1, '{letter}'\n syscall\n sub r5, 1\n cmp r5, 0\n jgt loop\n halt"
        )
    };
    let (a, b) = (source('a'), source('b'));
    let config = Config {
        time_slice: 4,
        ..Config::default()
    };
    let mut kernel = boot(config, &[a.as_str(), b.as_str()]);
    kernel.run();
    assert_eq!(kernel.console().output(), "ababab");
    for task in kernel.tasks() {
        assert_eq!(task.state, TaskState::Exited(0));
        assert!(task.stats.preemptions >= 3);
        assert_eq!(task.stats.syscalls, 3);
    }
}

#[test]
fn faulting_task_is_killed_without_stopping_others() {
    let crasher = "mov r1, 0\n div r0, r1\n halt";
    let printer = "mov r0, 1\n mov r1, 7\n syscall\n halt";
    let mut kernel = boot(Config::default(), &[crasher, printer]);
    kernel.run();
    let killed = TaskState::Killed(KernelError::DivideByZero);
    assert_eq!(kernel.task(1).unwrap().state, killed);
    assert_eq!(kernel.task(2).unwrap().state, TaskState::Exited(0));
    assert_eq!(kernel.console().output(), "7");
}

#[test]
fn tick_budget_stops_runaway_tasks() {
    let config = Config {
        max_ticks: 100,
        ..Config::default()
    };
    let mut kernel = boot(config, &["spin: jmp spin"]);
    kernel.run();
    assert_eq!(kernel.ticks(), 100);
    assert_eq!(kernel.task(1).unwrap().state, TaskState::Ready);
}

#[test]
fn yield_hands_the_cpu_to_the_next_task() {
    // Print, yield, print again. The slice is long enough that without the
    // yield each task would finish in one go and the output would be "aabb".
    let source = |letter: char| {
        format!(
            "mov r1, '{letter}'\n mov r0, 2\n syscall\n mov r0, 4\n syscall\n mov r0, 2\n syscall\n halt"
        )
    };
    let (a, b) = (source('a'), source('b'));
    let config = Config {
        time_slice: 100,
        ..Config::default()
    };
    let mut kernel = boot(config, &[a.as_str(), b.as_str()]);
    kernel.run();
    assert_eq!(kernel.console().output(), "abab");
    assert_eq!(kernel.task(1).unwrap().stats.preemptions, 0);
}

#[test]
fn getpid_returns_the_callers_pid() {
    let source = "mov r0, 5\n syscall\n mov r1, r0\n mov r0, 1\n syscall\n halt";
    let mut kernel = boot(Config::default(), &[source, source]);
    kernel.run();
    assert_eq!(kernel.console().output(), "12");
}

#[test]
fn sleep_blocks_until_the_timer_wakes_the_task() {
    let sleeper = "mov r0, 6\n mov r1, 50\n syscall\n mov r0, 2\n mov r1, 's'\n syscall\n halt";
    let runner = "mov r0, 2\n mov r1, 'r'\n syscall\n halt";
    let mut kernel = boot(Config::default(), &[sleeper, runner]);
    kernel.run();
    // The sleeper was spawned first but prints last.
    assert_eq!(kernel.console().output(), "rs");
    assert!(kernel.ticks() >= 50);
    assert!(kernel.idle_ticks() > 0);
    for task in kernel.tasks() {
        assert_eq!(task.state, TaskState::Exited(0));
    }
}

#[test]
fn uptime_reports_ticks_since_boot() {
    // The syscall is the second instruction, so it runs on tick 2.
    let source = "mov r0, 7\n syscall\n mov r1, r0\n mov r0, 1\n syscall\n halt";
    let mut kernel = boot(Config::default(), &[source]);
    kernel.run();
    assert_eq!(kernel.console().output(), "2");
}

#[test]
fn finished_tasks_return_their_frames() {
    // One task exits normally holding data and stack pages; the other is
    // killed holding a demand-paged frame.
    let tidy = "buf: .zero 70\n push 1\n halt";
    let crasher = "store r0, 100\n load r0, 100000";
    let mut kernel = boot(Config::default(), &[tidy, crasher]);
    assert!(kernel.free_frames() < Config::default().frames);
    kernel.run();
    assert_eq!(kernel.task(1).unwrap().state, TaskState::Exited(0));
    let killed = TaskState::Killed(KernelError::SegFault(100000));
    assert_eq!(kernel.task(2).unwrap().state, killed);
    assert_eq!(kernel.free_frames(), Config::default().frames);
}
