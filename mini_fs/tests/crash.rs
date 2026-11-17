//! Cut the power at every possible moment of an operation, restart, and
//! check what is found.

use std::collections::BTreeMap;

use mini_fs::device::{FaultyDisk, MemDisk};
use mini_fs::error::FsResult;
use mini_fs::fs::{FileSystem, FormatOptions};
use mini_fs::inode::InodeKind;

/// Every path in a file system, with each file's content.
type Tree = BTreeMap<String, Option<Vec<u8>>>;

fn snapshot(fs: &mut FileSystem<MemDisk>) -> Tree {
    let mut tree = Tree::new();
    let mut to_visit = vec![String::from("/")];
    while let Some(dir) = to_visit.pop() {
        for entry in fs.list(&dir).unwrap() {
            let path = format!("{}/{}", dir.trim_end_matches('/'), entry.name);
            if fs.stat(&path).unwrap().kind == InodeKind::Dir {
                tree.insert(path.clone(), None);
                to_visit.push(path);
            } else {
                let content = fs.read_file(&path).unwrap();
                tree.insert(path, Some(content));
            }
        }
    }
    tree
}

/// The image every operation is tried against.
fn base() -> MemDisk {
    let mut fs = FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap();
    fs.mkdir("/docs").unwrap();
    fs.mkdir("/empty").unwrap();
    fs.write_file("/docs/a.txt", &vec![b'a'; 3000]).unwrap();
    fs.write_file("/big", &vec![b'b'; 40_000]).unwrap();
    fs.into_device()
}

/// Run `operation` once for every write it performs, cutting the power just
/// before that write. After each cut the image is mounted again and must be
/// sound. If `all_or_nothing`, it must also look exactly as it did before
/// the operation or exactly as it does after a complete one.
///
/// Returns how many crash points were tried.
fn crash_everywhere(
    operation: impl Fn(&mut FileSystem<FaultyDisk<MemDisk>>) -> FsResult<()>,
    all_or_nothing: bool,
) -> usize {
    let base = base();
    let before = snapshot(&mut FileSystem::mount(base.clone()).unwrap());

    // An uninterrupted run shows the end state and how many writes it takes.
    let mut whole = FileSystem::mount(FaultyDisk::failing_after(base.clone(), usize::MAX)).unwrap();
    operation(&mut whole).unwrap();
    let whole = whole.into_device();
    let writes = whole.writes();
    let after = snapshot(&mut FileSystem::mount(whole.into_inner()).unwrap());
    assert_ne!(before, after, "the operation should change something");

    for budget in 0..writes {
        let mut fs = FileSystem::mount(FaultyDisk::failing_after(base.clone(), budget)).unwrap();
        assert!(
            operation(&mut fs).is_err(),
            "{budget} writes should not be enough"
        );

        // The power comes back.
        let mut restarted = FileSystem::mount(fs.into_device().into_inner()).unwrap();
        let problems = restarted.check().unwrap();
        assert!(problems.is_empty(), "after {budget} writes: {problems:#?}");
        if all_or_nothing {
            let found = snapshot(&mut restarted);
            assert!(
                found == before || found == after,
                "after {budget} writes the operation was left half done"
            );
        }
    }
    writes
}

#[test]
fn creating_a_file_is_all_or_nothing() {
    let points = crash_everywhere(|fs| fs.create("/docs/new.txt").map(drop), true);
    assert!(points >= 6);
}

#[test]
fn creating_a_directory_is_all_or_nothing() {
    crash_everywhere(|fs| fs.mkdir("/docs/sub").map(drop), true);
}

#[test]
fn unlinking_a_large_file_is_all_or_nothing() {
    crash_everywhere(|fs| fs.unlink("/big"), true);
}

#[test]
fn removing_a_directory_is_all_or_nothing() {
    crash_everywhere(|fs| fs.rmdir("/empty"), true);
}

#[test]
fn moving_a_file_between_directories_is_all_or_nothing() {
    crash_everywhere(|fs| fs.rename("/docs/a.txt", "/empty/moved.txt"), true);
}

#[test]
fn adding_a_hard_link_is_all_or_nothing() {
    crash_everywhere(|fs| fs.link("/big", "/docs/alias"), true);
}

#[test]
fn a_write_that_fits_one_transaction_is_all_or_nothing() {
    let overwrite = |fs: &mut FileSystem<FaultyDisk<MemDisk>>| {
        let number = fs.resolve("/docs/a.txt")?;
        // Straddles the boundary between the file's first two blocks.
        fs.write_at(number, 1000, &[b'X'; 100])
    };
    crash_everywhere(overwrite, true);
}

#[test]
fn a_long_write_may_stop_part_way_but_never_corrupts() {
    // Several transactions: the file may end up short, never inconsistent.
    let long_write = |fs: &mut FileSystem<FaultyDisk<MemDisk>>| {
        fs.write_file("/docs/long", &vec![b'z'; 60_000]).map(drop)
    };
    let points = crash_everywhere(long_write, false);
    assert!(points > 100);
}
