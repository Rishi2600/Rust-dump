use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use mini_fs::blockmap::MAX_FILE_SIZE;
use mini_fs::device::{BLOCK_SIZE, FileDisk};
use mini_fs::error::FsError;
use mini_fs::fs::{FileSystem, FormatOptions};
use mini_fs::inode::InodeKind;
use mini_fs::ops::Stat;

const USAGE: &str = "\
Usage: mini_fs <IMAGE> <COMMAND> [ARGS]

IMAGE is an ordinary file that holds the whole file system.

Commands:
  mkfs [--blocks N] [--inodes N] [--force]
                          create an empty file system in IMAGE
  info                    show the layout recorded in the superblock
  df                      show how much space is free
  ls [PATH]               list a directory
  tree [PATH]             list a directory and everything beneath it
  stat PATH               show the details of one file or directory
  mkdir PATH              create a directory
  write PATH TEXT...      store TEXT as the content of PATH
  put HOST_FILE PATH      copy a file from this computer into the image
  cat PATH                print the content of a file
  get PATH HOST_FILE      copy a file out of the image
  rm PATH                 remove a file
  rmdir PATH              remove an empty directory
  mv FROM TO              rename or move a file or directory
  ln EXISTING NEW         give a file a second name
  fsck                    check the image for inconsistencies";

type Volume = FileSystem<FileDisk>;

/// Why a run did not succeed.
enum Failure {
    /// The command line was wrong: show the usage text.
    Usage(String),
    /// The command was understood but could not be carried out.
    Failed(String),
}

impl From<FsError> for Failure {
    fn from(error: FsError) -> Self {
        Failure::Failed(error.to_string())
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Failure::Failed(error.to_string())
    }
}

/// Take exactly `N` arguments for `command`, or explain what was expected.
fn exactly<'a, const N: usize>(command: &str, args: &'a [String]) -> Result<[&'a str; N], Failure> {
    let given: Vec<&str> = args.iter().map(String::as_str).collect();
    given.try_into().map_err(|_| {
        Failure::Usage(format!(
            "`{command}` takes {N} argument(s), {} given",
            args.len()
        ))
    })
}

/// An optional path argument, defaulting to the root.
fn optional_path<'a>(command: &str, args: &'a [String]) -> Result<&'a str, Failure> {
    match args {
        [] => Ok("/"),
        [path] => Ok(path),
        _ => Err(Failure::Usage(format!(
            "`{command}` takes at most one path"
        ))),
    }
}

fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

fn mkfs(image: &str, args: &[String]) -> Result<(), Failure> {
    let (mut blocks, mut inodes, mut force) = (4096, 512, false);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut number = |flag: &str| -> Result<u32, Failure> {
            let raw = args
                .next()
                .ok_or_else(|| Failure::Usage(format!("{flag} needs a value")))?;
            raw.parse()
                .map_err(|_| Failure::Usage(format!("{flag}: `{raw}` is not a number")))
        };
        match arg.as_str() {
            "--blocks" => blocks = number("--blocks")?,
            "--inodes" => inodes = number("--inodes")?,
            "--force" => force = true,
            other => {
                return Err(Failure::Usage(format!(
                    "mkfs does not understand `{other}`"
                )));
            }
        }
    }
    // Formatting destroys whatever the file held, so an existing file is
    // only replaced when that is asked for explicitly.
    if Path::new(image).exists() && !force {
        return Err(Failure::Failed(format!(
            "{image} already exists; pass --force to replace it"
        )));
    }
    let options = FormatOptions {
        inodes,
        journal_blocks: 64,
    };
    FileSystem::format(FileDisk::create(image, blocks)?, options)?;
    println!("created {image}: {blocks} blocks of {BLOCK_SIZE} bytes, {inodes} inodes");
    Ok(())
}

fn info(volume: &Volume) {
    let sb = volume.superblock();
    let range = |start: u32, blocks: u32| format!("blocks {start}-{}", start + blocks - 1);
    println!("block size     {BLOCK_SIZE} bytes");
    println!("blocks         {}", sb.total_blocks);
    println!("superblock     block 0");
    println!(
        "journal        {}",
        range(sb.journal_start, sb.journal_blocks)
    );
    println!(
        "inode bitmap   {}",
        range(sb.inode_bitmap_start, sb.inode_bitmap_blocks)
    );
    println!(
        "block bitmap   {}",
        range(sb.block_bitmap_start, sb.block_bitmap_blocks)
    );
    println!(
        "inode table    {} ({} inodes)",
        range(sb.inode_table_start, sb.inode_table_blocks),
        sb.inode_count
    );
    println!("data           {}", range(sb.data_start, sb.data_blocks()));
    println!("largest file   {MAX_FILE_SIZE} bytes");
}

fn df(volume: &mut Volume) -> Result<(), Failure> {
    let usage = volume.usage()?;
    println!(
        "blocks   {} used, {} free, {} total ({BLOCK_SIZE} bytes each)",
        usage.data_blocks - usage.free_blocks,
        usage.free_blocks,
        usage.data_blocks
    );
    println!(
        "inodes   {} used, {} free, {} total",
        usage.inodes - usage.free_inodes,
        usage.free_inodes,
        usage.inodes
    );
    Ok(())
}

/// One line of `ls`: kind, link count, size and name.
fn describe(stat: &Stat, name: &str) -> String {
    let (mark, suffix) = match stat.kind {
        InodeKind::Dir => ('d', "/"),
        _ => ('-', ""),
    };
    format!("{mark} {:>2} {:>9}  {name}{suffix}", stat.links, stat.size)
}

fn ls(volume: &mut Volume, path: &str) -> Result<(), Failure> {
    let stat = volume.stat(path)?;
    if stat.kind != InodeKind::Dir {
        println!("{}", describe(&stat, path));
        return Ok(());
    }
    for entry in volume.list(path)? {
        let stat = volume.stat(&join(path, &entry.name))?;
        println!("{}", describe(&stat, &entry.name));
    }
    Ok(())
}

fn tree(volume: &mut Volume, path: &str, depth: usize) -> Result<(), Failure> {
    let indent = "  ".repeat(depth);
    for entry in volume.list(path)? {
        let child = join(path, &entry.name);
        let stat = volume.stat(&child)?;
        if stat.kind == InodeKind::Dir {
            println!("{indent}{}/", entry.name);
            tree(volume, &child, depth + 1)?;
        } else {
            println!("{indent}{} ({} bytes)", entry.name, stat.size);
        }
    }
    Ok(())
}

fn stat(volume: &mut Volume, path: &str) -> Result<(), Failure> {
    let stat = volume.stat(path)?;
    let kind = match stat.kind {
        InodeKind::Dir => "directory",
        InodeKind::File => "file",
        InodeKind::Free => "free",
    };
    println!("path     {path}");
    println!("inode    {}", stat.inode);
    println!("kind     {kind}");
    println!("links    {}", stat.links);
    println!("size     {} bytes", stat.size);
    println!("blocks   {}", stat.blocks);
    Ok(())
}

fn run(args: &[String]) -> Result<ExitCode, Failure> {
    let [image, command, rest @ ..] = args else {
        return Err(Failure::Usage("an image and a command are needed".into()));
    };
    let command = command.as_str();
    if command == "mkfs" {
        mkfs(image, rest)?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut volume = FileSystem::mount(FileDisk::open(image)?)?;
    match command {
        "info" => {
            let [] = exactly(command, rest)?;
            info(&volume);
        }
        "df" => {
            let [] = exactly(command, rest)?;
            df(&mut volume)?;
        }
        "ls" => ls(&mut volume, optional_path(command, rest)?)?,
        "tree" => {
            let path = optional_path(command, rest)?;
            println!("{path}");
            tree(&mut volume, path, 1)?;
        }
        "stat" => {
            let [path] = exactly(command, rest)?;
            stat(&mut volume, path)?;
        }
        "mkdir" => {
            let [path] = exactly(command, rest)?;
            volume.mkdir(path)?;
        }
        "write" => {
            let [path, words @ ..] = rest else {
                return Err(Failure::Usage("`write` needs a path and some text".into()));
            };
            let text = format!("{}\n", words.join(" "));
            volume.write_file(path, text.as_bytes())?;
        }
        "put" => {
            let [host_file, path] = exactly(command, rest)?;
            let data = std::fs::read(host_file)?;
            volume.write_file(path, &data)?;
        }
        "cat" => {
            let [path] = exactly(command, rest)?;
            io::stdout().write_all(&volume.read_file(path)?)?;
        }
        "get" => {
            let [path, host_file] = exactly(command, rest)?;
            std::fs::write(host_file, volume.read_file(path)?)?;
        }
        "rm" => {
            let [path] = exactly(command, rest)?;
            volume.unlink(path)?;
        }
        "rmdir" => {
            let [path] = exactly(command, rest)?;
            volume.rmdir(path)?;
        }
        "mv" => {
            let [from, to] = exactly(command, rest)?;
            volume.rename(from, to)?;
        }
        "ln" => {
            let [existing, new] = exactly(command, rest)?;
            volume.link(existing, new)?;
        }
        "fsck" => {
            let [] = exactly(command, rest)?;
            let problems = volume.check()?;
            if !problems.is_empty() {
                problems
                    .iter()
                    .for_each(|problem| println!("problem: {problem}"));
                return Ok(ExitCode::FAILURE);
            }
            println!("clean: no inconsistencies found");
        }
        other => return Err(Failure::Usage(format!("unknown command `{other}`"))),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(code) => code,
        Err(Failure::Usage(message)) => {
            eprintln!("error: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(message)) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
