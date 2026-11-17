//! The file system used the way a caller would use it: by path.

use mini_fs::blockmap::MAX_FILE_SIZE;
use mini_fs::device::MemDisk;
use mini_fs::error::FsError;
use mini_fs::fs::{FileSystem, FormatOptions};

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
