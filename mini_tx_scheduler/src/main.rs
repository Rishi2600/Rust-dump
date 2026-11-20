use std::process::ExitCode;
use std::str::FromStr;

use mini_tx_scheduler::node::NodeConfig;
use mini_tx_scheduler::report::{describe_comparison, describe_run, describe_totals};
use mini_tx_scheduler::scenario::Scenario;
use mini_tx_scheduler::sim::{compare_policies, run_scenario};
use mini_tx_scheduler::workload::random_workload;

const USAGE: &str = "\
Usage: mini_tx_scheduler [OPTIONS] <SCENARIO>
       mini_tx_scheduler [OPTIONS] --random <SEED>

Runs the transactions in a scenario file through a pool and a block builder,
and prints the blocks that result.

Options:
  --policy <NAME>     how to rank transactions: fee-per-unit (default),
                      total-fee or fifo
  --compare           run under every policy and compare the fees collected
  --threads <N>       transactions that may run at once (default 4)
  --block-units <N>   compute units per block (default 10000)
  --block-txs <N>     transactions per block (default 1000)
  --ttl <SLOTS>       slots a transaction may wait before it is dropped (default 32)
  --random <SEED>     use a generated workload instead of a file
  --accounts <N>      accounts in a generated workload (default 10)
  --count <N>         transactions in a generated workload (default 60)
  --summary           print only the totals
  -h, --help          show this help";

struct Options {
    scenario: Scenario,
    config: NodeConfig,
    compare: bool,
    summary: bool,
}

/// The value that must follow a flag such as `--threads`.
fn value<T: FromStr>(flag: &str, raw: Option<&String>) -> Result<T, String> {
    let raw = raw.ok_or_else(|| format!("{flag} needs a value"))?;
    raw.parse()
        .map_err(|_| format!("{flag}: `{raw}` is not valid"))
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let (mut path, mut seed) = (None, None);
    let (mut accounts, mut count) = (10, 60);
    let (mut compare, mut summary) = (false, false);
    // Flags are collected first and applied after the scenario's own limits,
    // so that the command line has the last word.
    let mut overrides: Vec<Box<dyn Fn(&mut NodeConfig)>> = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--policy" => {
                let policy = value(arg, args.next())?;
                overrides.push(Box::new(move |config| config.pool.policy = policy));
            }
            "--threads" => {
                let threads = value(arg, args.next())?;
                overrides.push(Box::new(move |config| config.limits.threads = threads));
            }
            "--block-units" => {
                let units = value(arg, args.next())?;
                overrides.push(Box::new(move |config| config.limits.max_units = units));
            }
            "--block-txs" => {
                let txs = value(arg, args.next())?;
                overrides.push(Box::new(move |config| config.limits.max_transactions = txs));
            }
            "--ttl" => {
                let ttl = value(arg, args.next())?;
                overrides.push(Box::new(move |config| config.pool.ttl_slots = ttl));
            }
            "--random" => seed = Some(value::<u64>(arg, args.next())?),
            "--accounts" => accounts = value(arg, args.next())?,
            "--count" => count = value(arg, args.next())?,
            "--compare" => compare = true,
            "--summary" => summary = true,
            flag if flag.starts_with('-') => return Err(format!("unknown option `{flag}`")),
            file if path.is_none() => path = Some(file.to_string()),
            _ => return Err("only one scenario file can be given".into()),
        }
    }

    let scenario = match (path, seed) {
        (Some(_), Some(_)) => return Err("give a scenario file or --random, not both".into()),
        (Some(path), None) => Scenario::load(&path).map_err(|error| format!("{path}: {error}"))?,
        (None, Some(seed)) => Scenario::from_workload(&random_workload(seed, accounts, count)),
        (None, None) => return Err("a scenario file or --random is needed".into()),
    };
    let mut config = NodeConfig::default();
    // A generated workload gives each sender many transactions at once.
    config.pool.max_nonce_gap = 100_000;
    scenario.configure(&mut config);
    for apply in &overrides {
        apply(&mut config);
    }
    Ok(Options {
        scenario,
        config,
        compare,
        summary,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match parse_args(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let (scenario, config) = (&options.scenario, &options.config);
    let limits = &config.limits;
    println!(
        "{} transactions, blocks of {} units and {} threads, ranked by {}\n",
        scenario.transaction_count(),
        limits.max_units,
        limits.threads,
        if options.compare {
            "each policy in turn"
        } else {
            config.pool.policy.name()
        }
    );

    if options.compare {
        print!(
            "{}",
            describe_comparison(&compare_policies(scenario, config))
        );
        return ExitCode::SUCCESS;
    }
    let run = run_scenario(scenario, config);
    if options.summary {
        println!("totals: {}", describe_totals(&run));
    } else {
        print!("{}", describe_run(&run));
    }
    ExitCode::SUCCESS
}
