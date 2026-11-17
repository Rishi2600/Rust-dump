//! The consistency checker, against sound and deliberately damaged images.

use mini_fs::device::MemDisk;
use mini_fs::fs::{FileSystem, FormatOptions};
use mini_fs::inode::InodeKind;
use mini_fs::layout::ROOT_INODE;

/// A file system with a few files, directories and a hard link.
fn populated() -> FileSystem<MemDisk> {
    let mut fs = FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap();
    fs.mkdir("/docs").unwrap();
    fs.mkdir("/docs/old").unwrap();
    fs.write_file("/docs/a.txt", b"small").unwrap();
    fs.write_file("/big.bin", &vec![1; 60_000]).unwrap();
    fs.link("/big.bin", "/docs/big-again").unwrap();
    fs
}

/// Run the checker and insist that it reports `expected` somewhere.
fn assert_reports(fs: &mut FileSystem<MemDisk>, expected: &str) {
    let problems = fs.check().unwrap();
    assert!(
        problems.iter().any(|problem| problem.contains(expected)),
        "expected a problem containing {expected:?}, got {problems:#?}"
    );
}

#[test]
fn a_file_system_that_has_only_been_used_normally_is_sound() {
    let mut fs = populated();
    assert_eq!(fs.check(), Ok(Vec::new()));

    // Still sound after a round of removing, moving and rewriting.
    fs.unlink("/big.bin").unwrap();
    fs.rename("/docs/a.txt", "/moved.txt").unwrap();
    fs.rmdir("/docs/old").unwrap();
    fs.write_file("/moved.txt", &vec![2; 20_000]).unwrap();
    fs.unlink("/docs/big-again").unwrap();
    assert_eq!(fs.check(), Ok(Vec::new()));

    let fresh = FileSystem::format(MemDisk::new(512), FormatOptions::default());
    assert_eq!(fresh.unwrap().check(), Ok(Vec::new()));
}

#[test]
fn a_block_nobody_owns_is_a_leak() {
    let mut fs = populated();
    let leaked = fs.atomic(|fs| fs.alloc_block()).unwrap();
    assert_reports(
        &mut fs,
        &format!("block {leaked} is marked used but belongs to no file"),
    );
}

#[test]
fn a_block_in_use_but_marked_free_would_be_handed_out_twice() {
    let mut fs = populated();
    let number = fs.resolve("/docs/a.txt").unwrap();
    let block = fs.read_inode(number).unwrap().direct[0];
    fs.atomic(|fs| fs.free_block(block)).unwrap();
    assert_reports(
        &mut fs,
        &format!("block {block} is in use by inode {number}"),
    );
}

#[test]
fn two_files_claiming_one_block_are_caught() {
    let mut fs = populated();
    let first = fs.resolve("/docs/a.txt").unwrap();
    let second = fs.create("/thief").unwrap();
    let shared = fs.read_inode(first).unwrap().direct[0];
    let mut thief = fs.read_inode(second).unwrap();
    thief.direct[0] = shared;
    fs.atomic(|fs| fs.write_inode(second, &thief)).unwrap();
    assert_reports(&mut fs, &format!("block {shared} is claimed by both"));
}

#[test]
fn a_pointer_out_of_the_data_area_is_caught_without_following_it() {
    let mut fs = populated();
    let number = fs.resolve("/docs/a.txt").unwrap();
    let mut inode = fs.read_inode(number).unwrap();
    inode.indirect = 3; // a journal block
    inode.double_indirect = 99_999; // off the end of the disk
    fs.atomic(|fs| fs.write_inode(number, &inode)).unwrap();
    assert_reports(
        &mut fs,
        &format!("inode {number} points at block 3, outside the data area"),
    );
    assert_reports(&mut fs, "points at block 99999, outside the data area");
}

#[test]
fn a_wrong_link_count_is_caught() {
    let mut fs = populated();
    let number = fs.resolve("/big.bin").unwrap();
    let mut inode = fs.read_inode(number).unwrap();
    inode.nlink = 5;
    fs.atomic(|fs| fs.write_inode(number, &inode)).unwrap();
    assert_reports(
        &mut fs,
        &format!("inode {number} says 5 links but 2 names point at it"),
    );
}

#[test]
fn an_inode_with_no_name_is_an_orphan() {
    let mut fs = populated();
    let orphan = fs.atomic(|fs| fs.alloc_inode(InodeKind::File)).unwrap();
    assert_reports(
        &mut fs,
        &format!("inode {orphan} is marked used but no directory entry refers to it"),
    );
}

#[test]
fn names_that_lead_nowhere_are_caught() {
    let mut fs = populated();
    fs.atomic(|fs| {
        fs.dir_add(ROOT_INODE, "ghost", 90)?;
        fs.dir_add(ROOT_INODE, "beyond", 5000)
    })
    .unwrap();
    assert_reports(
        &mut fs,
        "entry `ghost` in directory 1 points at inode 90, which is free",
    );
    assert_reports(
        &mut fs,
        "entry `beyond` in directory 1 points at inode 5000, which does not exist",
    );
}

#[test]
fn a_directory_with_two_names_is_caught() {
    let mut fs = populated();
    let docs = fs.resolve("/docs").unwrap();
    fs.atomic(|fs| fs.dir_add(ROOT_INODE, "docs-again", docs))
        .unwrap();
    assert_reports(
        &mut fs,
        &format!("directory {docs} is reachable through more than one name"),
    );
}
