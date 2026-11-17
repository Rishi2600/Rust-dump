use crate::bytes::{get_u32, put_u32};
use crate::device::BlockDevice;
use crate::error::{FsError, FsResult};
use crate::fs::FileSystem;
use crate::inode::InodeKind;

/// Size of one directory entry on disk.
pub const DIRENT_SIZE: usize = 64;
/// Longest name, in bytes: what is left after the inode number and length.
pub const MAX_NAME: usize = DIRENT_SIZE - 5;

/// One name in a directory, and the inode it stands for.
///
/// A directory is an ordinary file whose content is a row of these:
///
/// ```text
/// bytes 0-3    inode number, 0 for an empty slot
/// byte  4      length of the name
/// bytes 5-63   the name
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub inode: u32,
    pub name: String,
}

impl DirEntry {
    pub fn encode(&self) -> FsResult<[u8; DIRENT_SIZE]> {
        validate_name(&self.name)?;
        let mut raw = [0; DIRENT_SIZE];
        put_u32(&mut raw, 0, self.inode);
        raw[4] = self.name.len() as u8;
        raw[5..5 + self.name.len()].copy_from_slice(self.name.as_bytes());
        Ok(raw)
    }

    /// Decode one slot. An empty slot gives `None`.
    pub fn decode(raw: &[u8]) -> FsResult<Option<Self>> {
        let inode = get_u32(raw, 0);
        if inode == 0 {
            return Ok(None);
        }
        let len = usize::from(raw[4]);
        let name = raw
            .get(5..5 + len)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .filter(|name| validate_name(name).is_ok())
            .ok_or_else(|| FsError::Corrupt("a directory entry has an unreadable name".into()))?;
        Ok(Some(Self {
            inode,
            name: name.to_string(),
        }))
    }
}

/// Check that `name` can be stored as a single path component.
pub fn validate_name(name: &str) -> FsResult<()> {
    let problem = if name.is_empty() {
        "a name cannot be empty".to_string()
    } else if name == "." || name == ".." {
        format!("`{name}` is reserved")
    } else if name.len() > MAX_NAME {
        format!("`{name}` is longer than {MAX_NAME} bytes")
    } else if name.contains(['/', '\0']) {
        format!("`{name}` contains a slash or a zero byte")
    } else {
        return Ok(());
    };
    Err(FsError::InvalidName(problem))
}

impl<D: BlockDevice> FileSystem<D> {
    /// Every slot of directory `dir`: its byte offset, and its entry if it
    /// holds one.
    fn dir_slots(&mut self, dir: u32) -> FsResult<Vec<(usize, Option<DirEntry>)>> {
        let inode = self.read_inode(dir)?;
        if inode.kind != InodeKind::Dir {
            return Err(FsError::NotADirectory(format!("inode {dir}")));
        }
        let size = inode.size as usize;
        if size % DIRENT_SIZE != 0 {
            return Err(FsError::Corrupt(format!(
                "directory {dir} has a ragged size of {size}"
            )));
        }
        let mut raw = vec![0; size];
        self.read_at(dir, 0, &mut raw)?;
        let slots = raw.chunks_exact(DIRENT_SIZE).enumerate();
        slots
            .map(|(index, slot)| Ok((index * DIRENT_SIZE, DirEntry::decode(slot)?)))
            .collect()
    }

    /// The entries of a directory, in the order they sit on disk.
    pub fn dir_list(&mut self, dir: u32) -> FsResult<Vec<DirEntry>> {
        let slots = self.dir_slots(dir)?;
        Ok(slots.into_iter().filter_map(|(_, entry)| entry).collect())
    }

    /// The inode that `name` refers to inside directory `dir`, if any.
    pub fn dir_lookup(&mut self, dir: u32, name: &str) -> FsResult<Option<u32>> {
        let entries = self.dir_list(dir)?;
        Ok(entries
            .into_iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.inode))
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Add the name `name` to directory `dir`, pointing at `inode`. One step
    /// of a transaction.
    pub fn dir_add(&mut self, dir: u32, name: &str, inode: u32) -> FsResult<()> {
        let entry = DirEntry {
            inode,
            name: name.to_string(),
        };
        let raw = entry.encode()?;
        let slots = self.dir_slots(dir)?;
        let taken = slots
            .iter()
            .any(|(_, slot)| slot.as_ref().is_some_and(|e| e.name == name));
        if taken {
            return Err(FsError::AlreadyExists(name.to_string()));
        }
        // Reuse a slot emptied by an earlier removal, or else grow the
        // directory by one entry.
        let empty = slots.iter().find(|(_, slot)| slot.is_none());
        let offset = empty.map_or(slots.len() * DIRENT_SIZE, |(offset, _)| *offset);
        self.write_span(dir, offset, &raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::MemDisk;
    use crate::fs::FormatOptions;
    use crate::layout::ROOT_INODE;

    fn fresh() -> FileSystem<MemDisk> {
        FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap()
    }

    fn entry(inode: u32, name: &str) -> DirEntry {
        DirEntry {
            inode,
            name: name.to_string(),
        }
    }

    #[test]
    fn an_entry_is_an_inode_number_and_a_name() {
        let raw = entry(7, "notes.txt").encode().unwrap();
        assert_eq!(raw[..5], [7, 0, 0, 0, 9]);
        assert_eq!(&raw[5..14], b"notes.txt");
        assert!(raw[14..].iter().all(|&byte| byte == 0));
        assert_eq!(DirEntry::decode(&raw), Ok(Some(entry(7, "notes.txt"))));
        // A slot whose inode number is 0 is empty.
        assert_eq!(DirEntry::decode(&[0; DIRENT_SIZE]), Ok(None));
    }

    #[test]
    fn names_must_be_single_path_components() {
        assert!(validate_name("report-2024.final.txt").is_ok());
        assert!(validate_name(&"n".repeat(MAX_NAME)).is_ok());
        for bad in ["", ".", "..", "a/b", "nul\0byte", &"n".repeat(MAX_NAME + 1)] {
            assert!(
                matches!(validate_name(bad), Err(FsError::InvalidName(_))),
                "{bad:?}"
            );
            assert!(entry(7, bad).encode().is_err());
        }
    }

    #[test]
    fn damaged_entries_are_reported() {
        let mut raw = entry(7, "ok").encode().unwrap();
        raw[4] = 200; // a length that runs past the slot
        assert!(matches!(DirEntry::decode(&raw), Err(FsError::Corrupt(_))));
        raw[4] = 2;
        raw[5] = 0xFF; // not valid UTF-8
        assert!(matches!(DirEntry::decode(&raw), Err(FsError::Corrupt(_))));
    }

    #[test]
    fn a_directory_is_a_file_full_of_entries() {
        let mut fs = fresh();
        assert_eq!(fs.dir_list(ROOT_INODE), Ok(Vec::new()));

        // Write two entries straight into the root directory's content.
        let (first, second) = (entry(5, "a.txt"), entry(9, "b.txt"));
        fs.atomic(|fs| {
            fs.write_span(ROOT_INODE, 0, &first.encode()?)?;
            fs.write_span(ROOT_INODE, DIRENT_SIZE, &second.encode()?)
        })
        .unwrap();

        assert_eq!(fs.dir_list(ROOT_INODE), Ok(vec![first, second]));
        assert_eq!(fs.dir_lookup(ROOT_INODE, "b.txt"), Ok(Some(9)));
        assert_eq!(fs.dir_lookup(ROOT_INODE, "c.txt"), Ok(None));
        // Names are compared exactly.
        assert_eq!(fs.dir_lookup(ROOT_INODE, "B.TXT"), Ok(None));
    }

    #[test]
    fn only_directories_have_entries() {
        let mut fs = fresh();
        let file = fs.atomic(|fs| fs.alloc_inode(InodeKind::File)).unwrap();
        assert!(matches!(fs.dir_list(file), Err(FsError::NotADirectory(_))));
        assert!(matches!(
            fs.dir_lookup(file, "x"),
            Err(FsError::NotADirectory(_))
        ));
    }

    #[test]
    fn adding_a_name_appends_an_entry() {
        let mut fs = fresh();
        fs.atomic(|fs| {
            fs.dir_add(ROOT_INODE, "a.txt", 5)?;
            fs.dir_add(ROOT_INODE, "b.txt", 9)
        })
        .unwrap();
        assert_eq!(
            fs.dir_list(ROOT_INODE),
            Ok(vec![entry(5, "a.txt"), entry(9, "b.txt")])
        );
        assert_eq!(
            fs.read_inode(ROOT_INODE).unwrap().size as usize,
            2 * DIRENT_SIZE
        );
    }

    #[test]
    fn a_name_can_appear_only_once_in_a_directory() {
        let mut fs = fresh();
        fs.atomic(|fs| fs.dir_add(ROOT_INODE, "a.txt", 5)).unwrap();
        let again = fs.atomic(|fs| fs.dir_add(ROOT_INODE, "a.txt", 6));
        assert_eq!(again, Err(FsError::AlreadyExists("a.txt".into())));
        let bad = fs.atomic(|fs| fs.dir_add(ROOT_INODE, "a/b", 6));
        assert!(matches!(bad, Err(FsError::InvalidName(_))));
        assert_eq!(fs.dir_list(ROOT_INODE).unwrap().len(), 1);
    }

    #[test]
    fn a_directory_grows_past_one_block() {
        let mut fs = fresh();
        // 16 entries fill a block, so 20 need a second one.
        for index in 0..20 {
            let name = format!("file{index}");
            fs.atomic(|fs| fs.dir_add(ROOT_INODE, &name, 10 + index))
                .unwrap();
        }
        let root = fs.read_inode(ROOT_INODE).unwrap();
        assert_eq!(root.size as usize, 20 * DIRENT_SIZE);
        assert!(root.direct[0] != 0 && root.direct[1] != 0);
        assert_eq!(fs.dir_lookup(ROOT_INODE, "file19"), Ok(Some(29)));
    }
}
