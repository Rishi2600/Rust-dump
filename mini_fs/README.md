# mini_fs

A small Unix-style file system written in Rust with no dependencies. The whole
file system lives inside one ordinary file, the *image*, which the program
treats as a disk of 1024-byte blocks.

It implements the parts a real file system is made of:

- a **superblock** describing where everything is on the disk
- **bitmaps** tracking which blocks and which inodes are free
- **inodes** with 12 direct pointers, an indirect block and a double indirect
  block, for files up to about 64 MB
- **sparse files**: holes take no space and read back as zeros
- **directories** stored as files of fixed-size entries, nested to any depth
- **hard links** and link counts, so a file lives until its last name goes
- **rename** that moves a name without copying any data
- a **write-ahead journal**, so a power failure in the middle of an
  operation never leaves the disk half-changed
- a **consistency checker** (`fsck`)

## Running it

Every command takes the image file first.

```sh
cargo build --release
alias mfs=target/release/mini_fs

mfs disk.img mkfs --blocks 4096 --inodes 512
mfs disk.img mkdir /docs
mfs disk.img write /docs/hello.txt Hello from inside the image
mfs disk.img put Cargo.lock /docs/Cargo.lock
mfs disk.img ln /docs/hello.txt /greeting
mfs disk.img mkdir /docs/old
mfs disk.img mv /docs/Cargo.lock /docs/old/lockfile
mfs disk.img tree
```

```text
/
  docs/
    hello.txt (28 bytes)
    old/
      lockfile (151 bytes)
  greeting (28 bytes)
```

`ls` shows the kind, the link count and the size. `hello.txt` and `greeting`
are two names for one file, so both show 2 links. `docs` is 192 bytes, three
64-byte entries, although it now lists only two: the slot that held
`Cargo.lock` before it was moved is empty and waiting to be reused.

```text
$ mfs disk.img ls /
d  1       192  docs/
-  2        28  greeting

$ mfs disk.img cat /greeting
Hello from inside the image
```

`info` prints the layout that `mkfs` chose, and `df` the space in use:

```text
$ mfs disk.img info
block size     1024 bytes
blocks         4096
superblock     block 0
journal        blocks 1-64
inode bitmap   blocks 65-65
block bitmap   blocks 66-66
inode table    blocks 67-98 (512 inodes)
data           blocks 99-4095
largest file   67383296 bytes

$ mfs disk.img df
blocks   5 used, 3992 free, 3997 total (1024 bytes each)
inodes   5 used, 506 free, 511 total

$ mfs disk.img fsck
clean: no inconsistencies found
```

The image is a plain file, so ordinary tools can look inside it. It always
starts with the four bytes `MFS1`:

```sh
head -c 16 disk.img | od -c
```

### Commands

| Command | What it does |
| --- | --- |
| `mkfs [--blocks N] [--inodes N] [--force]` | create an empty file system (default 4096 blocks, 512 inodes) |
| `info` | show the layout recorded in the superblock |
| `df` | show how much space is free |
| `ls [PATH]` | list a directory |
| `tree [PATH]` | list a directory and everything beneath it |
| `stat PATH` | show one file's inode number, kind, links, size and blocks |
| `mkdir PATH` | create a directory |
| `write PATH TEXT...` | store text as the content of a file |
| `put HOST_FILE PATH` | copy a file from this computer into the image |
| `cat PATH` | print the content of a file |
| `get PATH HOST_FILE` | copy a file out of the image |
| `rm PATH` | remove a file |
| `rmdir PATH` | remove an empty directory |
| `mv FROM TO` | rename or move a file or directory |
| `ln EXISTING NEW` | give a file a second name |
| `fsck` | check the image for inconsistencies |

`mkfs` refuses to replace a file that already exists unless `--force` is
given. The exit code is 0 on success, 1 when a command fails or `fsck` finds
a problem, and 2 for a usage error.

## Layout of an image

```text
| super | journal | inode bitmap | block bitmap | inode table | data blocks ... |
  0       1
```

| Region | Holds |
| --- | --- |
| superblock | the magic number, the sizes, and where each region starts |
| journal | a header block and a copy of each block of the transaction in progress |
| inode bitmap | one bit per inode: in use or free |
| block bitmap | one bit per block of the disk: in use or free |
| inode table | 64 bytes per inode: kind, link count, size and block pointers |
| data | file contents, directory entries, and blocks of pointers |

## Source layout

| File | Role |
| --- | --- |
| `src/device.rs` | the `BlockDevice` trait, a disk in memory, a disk in a file, and a disk that fails on purpose |
| `src/bytes.rs` | little-endian integers inside a block |
| `src/layout.rs` | the superblock and the planning of the on-disk layout |
| `src/journal.rs` | transactions: buffering, commit through a log, recovery |
| `src/bitmap.rs` | bitmap allocation |
| `src/inode.rs` | the inode and its 64-byte disk form |
| `src/fs.rs` | format, mount, the inode table, allocation, atomic operations |
| `src/blockmap.rs` | from a position in a file to a block on disk; freeing a file's blocks |
| `src/file.rs` | reading and writing file content |
| `src/dir.rs` | directory entries: lookup, add, remove |
| `src/path.rs` | following a path from the root |
| `src/ops.rs` | create, write, read, mkdir, unlink, rmdir, link, rename, stat |
| `src/fsck.rs` | the consistency checker |
| `src/main.rs` | the command-line interface |

## Tests

```sh
cargo test
```

Almost every test runs against a disk held in memory. Three kinds are worth
knowing about:

- `tests/operations.rs` uses the file system by path, the way a caller would.
- `tests/fsck.rs` damages an image in a specific way and checks that the
  checker names the damage.
- `tests/crash.rs` runs each operation once for every block write it makes,
  cutting the power just before that write. After each cut it mounts the
  image again and requires the checker to find nothing wrong, and the
  operation to have happened completely or not at all.

## Limits

This is a learning project, not somewhere to keep files you care about.

- No permissions, owners or timestamps.
- No symbolic links.
- Directories never shrink, and are searched linearly.
- A file's content can be replaced or extended, but not cut to an arbitrary
  shorter length.
- A write larger than the journal is split into several transactions, so a
  crash can leave a long write partly done. Each part is still consistent.
- One program uses an image at a time. There is no locking.
