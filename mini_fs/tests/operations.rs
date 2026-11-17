//! The file system used the way a caller would use it: by path.

use mini_fs::blockmap::MAX_FILE_SIZE;
use mini_fs::device::MemDisk;
use mini_fs::error::FsError;
use mini_fs::fs::{FileSystem, FormatOptions};
use mini_fs::inode::InodeKind;

fn fresh() -> FileSystem<MemDisk> {
    FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap()
}

fn free_blocks(fs: &mut FileSystem<MemDisk>) -> u32 {
    fs.usage().unwrap().free_blocks
}

#[test]
fn a_created_file_exists_and_is_empty() {
    let mut fs = fresh();
    let number = fs.create("/a.txt").unwrap();
    assert_eq!(fs.resolve("/a.txt"), Ok(number));
    assert_eq!(fs.read_file("/a.txt"), Ok(Vec::new()));
    assert_eq!(
        fs.create("/a.txt"),
        Err(FsError::AlreadyExists("/a.txt".into()))
    );
}

#[test]
fn a_file_can_be_written_and_read_back_by_path() {
    let mut fs = fresh();
    fs.write_file("/hello.txt", b"hello, world\n").unwrap();
    assert_eq!(fs.read_file("/hello.txt").unwrap(), b"hello, world\n");
    assert_eq!(fs.stat("/hello.txt").unwrap().size, 13);
}

#[test]
fn writing_a_file_again_replaces_its_content_and_frees_the_old_blocks() {
    let mut fs = fresh();
    let empty = free_blocks(&mut fs);
    let number = fs.write_file("/data", &vec![1; 50_000]).unwrap();
    let with_big = free_blocks(&mut fs);
    assert!(with_big < empty - 48);

    // Same name, same inode, new and much smaller content.
    assert_eq!(fs.write_file("/data", b"tiny"), Ok(number));
    assert_eq!(fs.read_file("/data").unwrap(), b"tiny");
    // Only the root directory's block and one data block remain in use.
    assert_eq!(free_blocks(&mut fs), empty - 2);
}

#[test]
fn files_survive_remounting() {
    let mut fs = fresh();
    fs.write_file("/keep.bin", &vec![9; 5000]).unwrap();
    let mut fs = FileSystem::mount(fs.into_device()).unwrap();
    assert_eq!(fs.read_file("/keep.bin").unwrap(), vec![9; 5000]);
}

#[test]
fn paths_that_cannot_work_are_reported() {
    let mut fs = fresh();
    fs.write_file("/a.txt", b"x").unwrap();
    assert_eq!(
        fs.read_file("/nope"),
        Err(FsError::NotFound("/nope".into()))
    );
    // The directory a new file would go in has to exist, and be a directory.
    assert!(matches!(
        fs.create("/nope/b.txt"),
        Err(FsError::NotFound(_))
    ));
    assert!(matches!(
        fs.create("/a.txt/b.txt"),
        Err(FsError::NotADirectory(_))
    ));
    assert!(matches!(
        fs.write_file("/a.txt/b.txt", b"x"),
        Err(FsError::NotADirectory(_))
    ));
    assert!(matches!(fs.create("/"), Err(FsError::InvalidPath(_))));
    assert!(matches!(fs.read_file("/"), Err(FsError::IsADirectory(_))));
    assert!(matches!(
        fs.write_file("/", b"x"),
        Err(FsError::IsADirectory(_))
    ));
}

#[test]
fn an_oversized_write_is_refused_before_anything_is_lost() {
    let mut fs = fresh();
    fs.write_file("/a.txt", b"precious").unwrap();
    let too_big = vec![0; MAX_FILE_SIZE + 1];
    assert_eq!(
        fs.write_file("/a.txt", &too_big),
        Err(FsError::FileTooLarge)
    );
    assert_eq!(fs.read_file("/a.txt").unwrap(), b"precious");
}

#[test]
fn a_failed_create_does_not_leak_an_inode() {
    let mut fs = fresh();
    let before = fs.usage().unwrap();
    assert!(fs.create("/nope/b.txt").is_err());
    assert_eq!(fs.usage().unwrap(), before);
}

fn names(fs: &mut FileSystem<MemDisk>, path: &str) -> Vec<String> {
    fs.list(path)
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

#[test]
fn directories_nest_and_list_their_entries_in_name_order() {
    let mut fs = fresh();
    fs.mkdir("/docs").unwrap();
    fs.mkdir("/docs/drafts").unwrap();
    fs.write_file("/docs/b.txt", b"b").unwrap();
    fs.write_file("/docs/a.txt", b"a").unwrap();
    fs.write_file("/docs/drafts/idea.txt", b"?").unwrap();

    assert_eq!(names(&mut fs, "/"), ["docs"]);
    assert_eq!(names(&mut fs, "/docs"), ["a.txt", "b.txt", "drafts"]);
    assert_eq!(names(&mut fs, "/docs/drafts"), ["idea.txt"]);
    assert_eq!(fs.stat("/docs").unwrap().kind, InodeKind::Dir);
    assert_eq!(fs.read_file("/docs/drafts/idea.txt").unwrap(), b"?");
}

#[test]
fn the_same_file_can_be_reached_by_differently_spelled_paths() {
    let mut fs = fresh();
    fs.mkdir("/docs").unwrap();
    let number = fs.write_file("/docs/a.txt", b"a").unwrap();
    for path in [
        "docs/a.txt",
        "/docs/./a.txt",
        "//docs/../docs/a.txt",
        "/../docs/a.txt",
    ] {
        assert_eq!(fs.resolve(path), Ok(number), "{path}");
    }
}

#[test]
fn directories_and_files_are_not_interchangeable() {
    let mut fs = fresh();
    fs.mkdir("/docs").unwrap();
    fs.write_file("/a.txt", b"a").unwrap();
    assert_eq!(
        fs.mkdir("/docs"),
        Err(FsError::AlreadyExists("/docs".into()))
    );
    assert_eq!(
        fs.mkdir("/a.txt"),
        Err(FsError::AlreadyExists("/a.txt".into()))
    );
    assert!(matches!(fs.list("/a.txt"), Err(FsError::NotADirectory(_))));
    assert!(matches!(
        fs.read_file("/docs"),
        Err(FsError::IsADirectory(_))
    ));
    assert!(matches!(
        fs.write_file("/docs", b"x"),
        Err(FsError::IsADirectory(_))
    ));
    assert!(matches!(
        fs.mkdir("/a.txt/sub"),
        Err(FsError::NotADirectory(_))
    ));
}

#[test]
fn unlinking_a_file_gives_back_its_blocks_and_its_inode() {
    let mut fs = fresh();
    fs.write_file("/keep.txt", b"keep").unwrap();
    let before = fs.usage().unwrap();
    let number = fs.write_file("/big.bin", &vec![3; 40_000]).unwrap();
    assert!(fs.usage().unwrap().free_blocks < before.free_blocks - 39);

    fs.unlink("/big.bin").unwrap();
    assert_eq!(fs.usage().unwrap(), before);
    assert_eq!(
        fs.read_file("/big.bin"),
        Err(FsError::NotFound("/big.bin".into()))
    );
    assert_eq!(names(&mut fs, "/"), ["keep.txt"]);
    // The next file gets the inode number that was just freed.
    assert_eq!(fs.create("/next"), Ok(number));
}

#[test]
fn only_existing_files_can_be_unlinked() {
    let mut fs = fresh();
    fs.mkdir("/docs").unwrap();
    assert_eq!(fs.unlink("/nope"), Err(FsError::NotFound("/nope".into())));
    assert_eq!(
        fs.unlink("/docs"),
        Err(FsError::IsADirectory("/docs".into()))
    );
    assert!(matches!(fs.unlink("/"), Err(FsError::InvalidPath(_))));
    assert_eq!(names(&mut fs, "/"), ["docs"]);
}

#[test]
fn only_an_empty_directory_can_be_removed() {
    let mut fs = fresh();
    fs.mkdir("/outer").unwrap();
    let before = fs.usage().unwrap();
    fs.mkdir("/outer/docs").unwrap();
    fs.write_file("/outer/docs/a.txt", b"a").unwrap();

    let refused = fs.rmdir("/outer/docs");
    assert_eq!(
        refused,
        Err(FsError::DirectoryNotEmpty("/outer/docs".into()))
    );
    fs.unlink("/outer/docs/a.txt").unwrap();
    assert_eq!(fs.rmdir("/outer/docs"), Ok(()));
    assert!(names(&mut fs, "/outer").is_empty());
    // The block that held its one-time entry came back with it. The parent
    // keeps the block its own entry slot lives in.
    let after = fs.usage().unwrap();
    assert_eq!(after.free_inodes, before.free_inodes);
    assert_eq!(after.free_blocks, before.free_blocks - 1);
}

#[test]
fn rmdir_rejects_files_the_root_and_missing_names() {
    let mut fs = fresh();
    fs.write_file("/a.txt", b"a").unwrap();
    assert_eq!(
        fs.rmdir("/a.txt"),
        Err(FsError::NotADirectory("/a.txt".into()))
    );
    assert_eq!(fs.rmdir("/nope"), Err(FsError::NotFound("/nope".into())));
    assert!(matches!(fs.rmdir("/"), Err(FsError::InvalidPath(_))));
}
