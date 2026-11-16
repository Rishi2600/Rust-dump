use std::env;
use std::net::IpAddr;
use std::process::ExitCode;
use std::time::Duration;

use mini_dns_resolver::error::DnsError;
use mini_dns_resolver::name::Name;
use mini_dns_resolver::resolver::{Config, Resolver};
use mini_dns_resolver::rtype::RecordType;
use mini_dns_resolver::trace::Tracing;
use mini_dns_resolver::transport::NetTransport;

const USAGE: &str = "\
Usage: mini_dns_resolver [OPTIONS] <NAME>...

Resolves each name by walking the DNS hierarchy down from the root servers.

Options:
  -t, --type <TYPE>    record type: A, AAAA, NS, CNAME, MX, TXT, SOA, PTR (default A)
  -x, --reverse        treat each NAME as an IP address and look up its PTR record
  -s, --server <IP>    send the question to this recursive resolver instead
      --timeout <MS>   how long to wait for each reply (default 2000)
      --trace          show every query that is sent
  -h, --help           show this help";

struct Options {
    rtype: RecordType,
    reverse: bool,
    server: Option<IpAddr>,
    timeout: Duration,
    trace: bool,
    names: Vec<String>,
}

/// The value that must follow a flag such as `--type`.
fn value(flag: &str, raw: Option<String>) -> Result<String, String> {
    raw.ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        rtype: RecordType::A,
        reverse: false,
        server: None,
        timeout: Duration::from_millis(2000),
        trace: false,
        names: Vec::new(),
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-t" | "--type" => {
                let raw = value(&arg, args.next())?;
                options.rtype = raw
                    .parse()
                    .map_err(|_| format!("unknown record type `{raw}`"))?;
            }
            "-s" | "--server" => {
                let raw = value(&arg, args.next())?;
                let address = raw
                    .parse()
                    .map_err(|_| format!("`{raw}` is not an IP address"))?;
                options.server = Some(address);
            }
            "--timeout" => {
                let raw = value(&arg, args.next())?;
                let millis: u64 = raw
                    .parse()
                    .map_err(|_| format!("`{raw}` is not a number"))?;
                options.timeout = Duration::from_millis(millis.max(1));
            }
            "-x" | "--reverse" => options.reverse = true,
            "--trace" => options.trace = true,
            flag if flag.starts_with('-') && flag.len() > 1 => {
                return Err(format!("unknown option `{flag}`"));
            }
            _ => options.names.push(arg),
        }
    }
    if options.names.is_empty() {
        return Err("no names given".into());
    }
    Ok(options)
}

/// Turn one command-line argument into the question to ask.
fn question(options: &Options, text: &str) -> Result<(Name, RecordType), String> {
    if options.reverse {
        let address: IpAddr = text.parse().map_err(|_| "not an IP address".to_string())?;
        return Ok((Name::reverse(address), RecordType::Ptr));
    }
    let name = Name::parse(text).map_err(|error| error.to_string())?;
    Ok((name, options.rtype))
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match parse_args(args.into_iter()) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    // One resolver for every name, so later lookups reuse what earlier ones cached.
    let transport = Tracing::new(NetTransport::new(options.timeout));
    let mut resolver = Resolver::new(transport, Config::default());
    let mut failed = false;
    for text in &options.names {
        let (name, rtype) = match question(&options, text) {
            Ok(question) => question,
            Err(message) => {
                eprintln!("error: {text}: {message}");
                failed = true;
                continue;
            }
        };
        let already_traced = resolver.transport().entries().len();
        let result = match options.server {
            Some(server) => resolver.forward(server, &name, rtype),
            None => resolver.resolve(&name, rtype),
        };
        if options.trace {
            for entry in &resolver.transport().entries()[already_traced..] {
                println!("; {entry}");
            }
        }
        match result {
            Ok(records) if records.is_empty() => println!("; {name} has no {rtype} records"),
            Ok(records) => records.iter().for_each(|record| println!("{record}")),
            Err(error) => {
                // This message already names the name it is about.
                match error {
                    DnsError::NxDomain(_) => eprintln!("error: {error}"),
                    _ => eprintln!("error: {name}: {error}"),
                }
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
