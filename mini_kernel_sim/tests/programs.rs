//! End-to-end runs of the sample programs in `programs/`.

use mini_kernel_sim::assembler::assemble;
use mini_kernel_sim::error::KernelError;
use mini_kernel_sim::kernel::{Config, Kernel};
use mini_kernel_sim::task::TaskState;

const HELLO: &str = include_str!("../programs/hello.ksm");
const COUNTER: &str = include_str!("../programs/counter.ksm");
const FIB: &str = include_str!("../programs/fib.ksm");
const SLEEPER: &str = include_str!("../programs/sleeper.ksm");
const CRASH: &str = include_str!("../programs/crash.ksm");

/// Boot a kernel with the given programs and run it to completion.
fn run(config: Config, sources: &[&str]) -> Kernel {
    let mut kernel = Kernel::new(config);
    for (index, source) in sources.iter().enumerate() {
        let program = assemble(source).expect("sample program assembles");
        let name = format!("task{}", index + 1);
        kernel.spawn(&name, program).expect("task spawns");
    }
    kernel.run();
    kernel
}

#[test]
fn hello_prints_its_greeting() {
    let kernel = run(Config::default(), &[HELLO]);
    assert_eq!(kernel.console().output(), "Hello from user space!\n");
    assert_eq!(kernel.task(1).unwrap().state, TaskState::Exited(0));
}

#[test]
fn fib_prints_the_first_ten_numbers() {
    let kernel = run(Config::default(), &[FIB]);
    assert_eq!(
        kernel.console().output(),
        "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n"
    );
}

#[test]
fn counters_share_the_cpu_without_garbling_lines() {
    let config = Config {
        time_slice: 3,
        ..Config::default()
    };
    let kernel = run(config, &[COUNTER, COUNTER]);

    let mut lines: Vec<&str> = kernel.console().output().lines().collect();
    lines.sort_unstable();
    let mut expected = Vec::new();
    for pid in 1..=2 {
        for count in 1..=5 {
            expected.push(format!("task {pid}: count {count}"));
        }
    }
    assert_eq!(lines, expected);

    for task in kernel.tasks() {
        assert_eq!(task.state, TaskState::Exited(0));
        assert!(task.stats.preemptions > 0);
    }
}

#[test]
fn sleeper_wakes_up_three_times() {
    let kernel = run(Config::default(), &[SLEEPER]);
    let output = kernel.console().output();
    assert_eq!(output.lines().count(), 3);
    assert!(
        output
            .lines()
            .all(|line| line.starts_with("sleeper: awake at tick "))
    );
    // With nothing else to run, the machine idles through every sleep.
    assert!(kernel.idle_ticks() >= 60);
}

#[test]
fn crash_is_contained_to_the_faulting_task() {
    let kernel = run(Config::default(), &[CRASH, HELLO]);
    let killed = TaskState::Killed(KernelError::SegFault(100000));
    assert_eq!(kernel.task(1).unwrap().state, killed);
    assert_eq!(kernel.task(2).unwrap().state, TaskState::Exited(0));
    assert!(kernel.console().output().contains("Hello from user space!"));
}

#[test]
fn all_samples_run_together_and_release_their_memory() {
    let kernel = run(Config::default(), &[HELLO, COUNTER, FIB, SLEEPER, CRASH]);
    let finished = kernel.tasks().iter().filter(|task| task.is_finished());
    assert_eq!(finished.count(), 5);
    assert_eq!(kernel.free_frames(), Config::default().frames);
}
