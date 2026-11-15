# mini_kernel_sim

A small operating system kernel simulated in user space, written in Rust with
no dependencies. It runs programs written in a tiny assembly language as
separate tasks, with the pieces a real kernel has:

- a bitmap **frame allocator** over simulated physical memory
- a **page table** per task, with **demand paging** on first touch
- a **CPU** that executes bytecode and can restart a faulting instruction
- a **round-robin scheduler** with preemptive time slices
- **system calls** for output, yielding, sleeping and exiting
- **fault isolation**: a task that crashes is killed, the rest keep running

## Running it

```sh
cargo run --release -- programs/hello.ksm programs/counter.ksm programs/counter.ksm \
    programs/sleeper.ksm programs/crash.ksm
```

Each file becomes one task. Output from the run above:

```text
Hello from user space!
crash: about to touch a bad address
task 2: count 1
task 3: count 1
task 2: count 2
task 3: count 2
task 2: count 3
task 3: count 3
task 2: count 4
task 3: count 4
task 2: count 5
task 3: count 5
sleeper: awake at tick 47
sleeper: awake at tick 187
sleeper: awake at tick 269

--- run summary ---
PID  NAME          TICKS SYSCALLS FAULTS  RUNS PREEMPTED  STATE
  1  hello             6        2      0     2         1  exited(0)
  2  counter          51        6      0    11        10  exited(0)
  3  counter          51        6      0    11        10  exited(0)
  4  sleeper         174        9      1    36        32  exited(0)
  5  crash             5        1      0     1         0  killed: segmentation fault at address 0x186a0
ticks: 327 total, 40 idle    frames in use: 0 of 64
```

The two counters alternate because each is preempted when its time slice runs
out. `crash` is killed by the kernel without disturbing anyone else, and every
frame is back in the pool by the end.

### Options

| Option            | Meaning                                   | Default |
| ----------------- | ----------------------------------------- | ------- |
| `--slice <N>`     | instructions per time slice               | 5       |
| `--frames <N>`    | physical frames in the machine            | 64      |
| `--pages <N>`     | pages in each task's address space        | 8       |
| `--max-ticks <N>` | stop after this many timer ticks          | 1000000 |
| `--trace`         | log every executed instruction to stderr  | off     |
| `--quiet`         | do not print the run summary              | off     |

`--trace` shows the scheduler at work, one line per instruction:

```text
[tick      1] pid 1 pc   0: mov r0, 3
[tick      2] pid 1 pc   1: mov r1, 0
[tick      3] pid 1 pc   2: syscall
```

## The machine

- Memory is measured in 64-bit words. A page is 64 words.
- Each task has eight registers, `r0` to `r7`, a program counter and a stack
  pointer. The stack starts at the top of the address space and grows down.
- A task's data segment is loaded at address 0. Every other page is mapped the
  first time the task touches it. Addresses past the end of the address space
  are a segmentation fault.
- One instruction takes one timer tick.

## Assembly language

One statement per line: `label: mnemonic operand, operand ; comment`.
Operands are registers, labels, or immediates (`42`, `-7`, `0x1F`, `'A'`).

| Instruction                                  | Effect                                   |
| -------------------------------------------- | ---------------------------------------- |
| `mov dst, src`                               | `dst = src`                              |
| `add` `sub` `mul` `div` `mod` `and` `or` `xor` | `dst = dst op src`                     |
| `cmp a, b`                                   | compare, for the next conditional jump   |
| `jmp` `jeq` `jne` `jlt` `jle` `jgt` `jge`    | jump to a label                          |
| `load dst, addr`                             | read a word of memory                    |
| `store src, addr`                            | write a word of memory                   |
| `push src` / `pop dst`                       | stack operations                         |
| `call label` / `ret`                         | subroutine call and return               |
| `syscall`                                    | trap into the kernel                     |
| `halt`                                       | exit with code 0                         |
| `nop`                                        | do nothing                               |

Data directives place words in the data segment; a label on one is its address.

| Directive            | Emits                                 |
| -------------------- | ------------------------------------- |
| `.word 1, 2, 3`      | the given words                       |
| `.zero N`            | `N` zero words                        |
| `.ascii "text"`      | one word per character                |
| `.string "text"`     | the characters, then a terminating 0  |

### System calls

Put the call number in `r0` and the argument in `r1`, then `syscall`.
Results come back in `r0`.

| No. | Name         | Behaviour                                        |
| --- | ------------ | ------------------------------------------------ |
| 0   | `exit`       | end the task with exit code `r1`                 |
| 1   | `print_int`  | print `r1` as a decimal number                   |
| 2   | `print_char` | print `r1` as a character                        |
| 3   | `print_str`  | print the NUL-terminated string at address `r1`  |
| 4   | `yield`      | give up the rest of the time slice               |
| 5   | `getpid`     | return this task's pid                           |
| 6   | `sleep`      | block for `r1` ticks                             |
| 7   | `uptime`     | return ticks since boot                          |

A task can be preempted between two system calls. `fib.ksm` prints each number
and its newline with separate calls, so its lines can be split when it shares
the CPU. `counter.ksm` and `sleeper.ksm` avoid that by building the whole line
in memory and writing it with one `print_str`.

## Source layout

| File               | Role                                                   |
| ------------------ | ------------------------------------------------------ |
| `src/frame.rs`     | bitmap allocator for physical frames                   |
| `src/memory.rs`    | simulated physical memory                              |
| `src/paging.rs`    | per-task page table and address translation            |
| `src/mmu.rs`       | connects the CPU to memory through a page table        |
| `src/instr.rs`     | instruction set and disassembly                        |
| `src/cpu.rs`       | register context and instruction execution             |
| `src/assembler.rs` | tokenizer, operand parser and two-pass assembler       |
| `src/task.rs`      | task control block and task states                     |
| `src/scheduler.rs` | round-robin run queue                                  |
| `src/syscall.rs`   | system call numbers                                    |
| `src/console.rs`   | console output device                                  |
| `src/kernel.rs`    | spawning, demand paging, system calls, scheduling loop |
| `src/main.rs`      | command-line interface                                 |
| `programs/`        | sample programs                                        |

## Tests

```sh
cargo test
```

Unit tests sit next to the code they cover. The files in `tests/` exercise the
CPU, the assembler and the kernel through the public API, and run every sample
program end to end.
