use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;
use std::{env, fs};

use mini_kernel_sim::assembler::assemble;
use mini_kernel_sim::kernel::{Config, Kernel};

const USAGE: &str = "\
Usage: mini_kernel_sim [OPTIONS] <PROGRAM.ksm>...

Boots a simulated kernel and runs each program as its own task.

Options:
  --slice <N>       instructions per time slice (default 5)
  --frames <N>      physical frames in the machine (default 64)
  --pages <N>       pages in each task's address space (default 8)
  --max-ticks <N>   stop after this many timer ticks (default 1000000)
  --trace           log every executed instruction to stderr
  --quiet           do not print the run summary
  -h, --help        show this help";

struct Options {
    config: Config,
    programs: Vec<String>,
    summary: bool,
}

/// Parse the value that follows a flag such as `--slice`.
fn value<T: FromStr>(flag: &str, raw: Option<String>) -> Result<T, String> {
    let raw = raw.ok_or_else(|| format!("{flag} needs a value"))?;
    raw.parse()
        .map_err(|_| format!("{flag}: `{raw}` is not a valid number"))
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        config: Config {
            echo: true,
            ..Config::default()
        },
        programs: Vec::new(),
        summary: true,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--slice" => options.config.time_slice = value(&arg, args.next())?,
            "--frames" => options.config.frames = value(&arg, args.next())?,
            "--pages" => options.config.pages_per_task = value(&arg, args.next())?,
            "--max-ticks" => options.config.max_ticks = value(&arg, args.next())?,
            "--trace" => options.config.trace = true,
            "--quiet" => options.summary = false,
            flag if flag.starts_with('-') => return Err(format!("unknown option `{flag}`")),
            _ => options.programs.push(arg),
        }
    }
    if options.programs.is_empty() {
        return Err("no programs given".into());
    }
    Ok(options)
}

/// Read, assemble and spawn one program. The task is named after the file.
fn load(kernel: &mut Kernel, path: &str) -> Result<usize, String> {
    let source = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let program = assemble(&source).map_err(|err| err.to_string())?;
    let name = Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(path);
    kernel.spawn(name, program).map_err(|err| err.to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match parse_args(args.into_iter()) {
        Ok(options) => options,
        Err(msg) => {
            eprintln!("error: {msg}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let mut kernel = Kernel::new(options.config);
    for path in &options.programs {
        if let Err(msg) = load(&mut kernel, path) {
            eprintln!("error: {path}: {msg}");
            return ExitCode::FAILURE;
        }
    }
    kernel.run();

    if options.summary {
        print!("\n--- run summary ---\n{}", kernel.summary());
    }
    ExitCode::SUCCESS
}
