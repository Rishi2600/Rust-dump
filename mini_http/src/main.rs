use std::io::{self, Read, Write};
use std::net::IpAddr;
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use mini_http::client::Client;
use mini_http::date::http_date;
use mini_http::dns::DnsResolver;
use mini_http::files::StaticFiles;
use mini_http::log::Logged;
use mini_http::response::Response;
use mini_http::router::Router;
use mini_http::server::{Server, ServerConfig};
use mini_http::status::StatusCode;

const USAGE: &str = "\
Usage:
  mini_http serve [OPTIONS]
  mini_http fetch [OPTIONS] <URL>

serve: answer HTTP requests with the files under a directory, plus a few
built-in routes under /api/.
  --root <DIR>      directory to serve (default: public)
  --addr <IP>       address to listen on (default: 127.0.0.1)
  --port <N>        port to listen on (default: 8080)
  --threads <N>     worker threads (default: 8)
  --list            list directories that have no index.html
  --quiet           do not log requests

fetch: print what a URL returns. Host names are looked up by walking the DNS
hierarchy with mini_dns_resolver.
  -i, --include     print the status line and headers before the body
  -I, --head        ask for the headers only

  -h, --help        show this help";

/// The value that must follow a flag such as `--port`.
fn value<T: std::str::FromStr>(flag: &str, raw: Option<&String>) -> Result<T, String> {
    let raw = raw.ok_or_else(|| format!("{flag} needs a value"))?;
    raw.parse()
        .map_err(|_| format!("{flag}: `{raw}` is not valid"))
}

/// A body that is produced a line at a time, to show chunked encoding.
struct Countdown {
    left: u32,
}

impl Read for Countdown {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let line = format!("{}...\n", self.left);
        self.left -= 1;
        buf[..line.len()].copy_from_slice(line.as_bytes());
        Ok(line.len())
    }
}

/// The routes of the demo server. Earlier routes win, so the catch-all for
/// files comes last.
fn routes(root: String, listing: bool) -> Router {
    let files = StaticFiles::new(root).with_listing(listing);
    Router::new()
        .get("/api/hello/:name", |_, params| {
            let name = params.get("name").unwrap_or("stranger");
            Response::text(StatusCode::OK, format!("Hello, {name}!\n"))
        })
        .get("/api/time", |_, _| {
            Response::text(
                StatusCode::OK,
                format!("{}\n", http_date(SystemTime::now())),
            )
        })
        .get("/api/headers", |request, _| {
            let lines: Vec<String> = request
                .headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}\n"))
                .collect();
            Response::text(StatusCode::OK, lines.concat())
        })
        .post("/api/echo", |request, _| {
            let kind = request
                .header("Content-Type")
                .unwrap_or("application/octet-stream");
            Response::new(StatusCode::OK)
                .header("Content-Type", kind)
                .body(request.body.clone())
        })
        .get("/api/stream", |_, _| {
            Response::stream(StatusCode::OK, Countdown { left: 5 })
                .header("Content-Type", "text/plain; charset=utf-8")
        })
        .get("/*path", move |request, _| {
            files.serve(request, request.path())
        })
}

fn serve(args: &[String]) -> Result<(), String> {
    let (mut root, mut addr, mut port) = (
        "public".to_string(),
        "127.0.0.1".parse::<IpAddr>().unwrap(),
        8080u16,
    );
    let (mut listing, mut quiet) = (false, false);
    let mut config = ServerConfig::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = value(arg, args.next())?,
            "--addr" => addr = value(arg, args.next())?,
            "--port" => port = value(arg, args.next())?,
            "--threads" => config.threads = value(arg, args.next())?,
            "--list" => listing = true,
            "--quiet" => quiet = true,
            other => return Err(format!("serve does not understand `{other}`")),
        }
    }
    if !std::path::Path::new(&root).is_dir() {
        return Err(format!("`{root}` is not a directory"));
    }

    let service = Logged::with_sink(routes(root.clone(), listing), move |line| {
        if !quiet {
            println!("{line}");
        }
    });
    let server = Server::bind((addr, port), service, config).map_err(|error| error.to_string())?;
    let address = server.local_addr().map_err(|error| error.to_string())?;
    println!("serving {root} at http://{address}/  (Ctrl-C to stop)");
    server.run();
    Ok(())
}

fn fetch(args: &[String]) -> Result<(), String> {
    let (mut include, mut head, mut url) = (false, false, None);
    for arg in args {
        match arg.as_str() {
            "-i" | "--include" => include = true,
            "-I" | "--head" => head = true,
            flag if flag.starts_with('-') => {
                return Err(format!("fetch does not understand `{flag}`"));
            }
            other if url.is_none() => url = Some(other),
            _ => return Err("fetch takes one URL".into()),
        }
    }
    let url = url.ok_or("fetch needs a URL")?;

    let mut client = Client::new(DnsResolver::on_the_internet(Duration::from_secs(3)));
    let response = if head {
        client.head(url)
    } else {
        client.get(url)
    };
    let response = response.map_err(|error| error.to_string())?;

    let mut out = io::stdout().lock();
    let mut write = |bytes: &[u8]| out.write_all(bytes).map_err(|error| error.to_string());
    if include || head {
        write(format!("HTTP/1.1 {} {}\r\n", response.status.0, response.reason).as_bytes())?;
        for (name, value) in response.headers.iter() {
            write(format!("{name}: {value}\r\n").as_bytes())?;
        }
        write(b"\r\n")?;
    }
    write(&response.body)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let result = match args.split_first() {
        Some((command, rest)) if command == "serve" => serve(rest),
        Some((command, rest)) if command == "fetch" => fetch(rest),
        Some((other, _)) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
        None => Err(format!("a command is needed\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
