use crate::blockmap::MAX_FILE_SIZE;
use crate::device::BlockDevice;
use crate::dir::DirEntry;
use crate::error::{FsError, FsResult};
use crate::fs::FileSystem;
use crate::inode::InodeKind;

/// What is known about one file or directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub inode: u32,
    pub kind: InodeKind,
    /// How many names point at it.
    pub links: u16,
    /// Length of its content in bytes.
    pub size: u32,
    /// Disk blocks it occupies, pointer blocks included.
    pub blocks: u32,
}

/// How full the file system is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    /// Blocks that can hold file content.
    pub data_blocks: u32,
    pub free_blocks: u32,
    /// Inodes that can be used, which is also the most files and directories.
    pub inodes: u32,
    pub free_inodes: u32,
}

impl<D: BlockDevice> FileSystem<D> {
    pub fn stat(&mut self, path: &str) -> FsResult<Stat> {
        let number = self.resolve(path)?;
        let inode = self.read_inode(number)?;
        let blocks = self.block_list(&inode)?.len() as u32;
        Ok(Stat {
            inode: number,
            kind: inode.kind,
            links: inode.nlink,
            size: inode.size,
            blocks,
        })
    }

    pub fn usage(&mut self) -> FsResult<Usage> {
        let used_blocks = self.sb.block_bitmap().count_set(&mut self.disk)?;
        let used_inodes = self.sb.inode_bitmap().count_set(&mut self.disk)?;
        Ok(Usage {
            data_blocks: self.sb.data_blocks(),
            free_blocks: self.sb.total_blocks - used_blocks,
            // Inode 0 is reserved and never usable.
            inodes: self.sb.inode_count - 1,
            free_inodes: self.sb.inode_count - used_inodes,
        })
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// One step of a transaction: make an empty file or directory at `path`.
    fn make_node(&mut self, path: &str, kind: InodeKind) -> FsResult<u32> {
        let (chain, name) = self.walk_parent(path)?;
        let parent = chain[chain.len() - 1];
        if self.dir_lookup(parent, name)?.is_some() {
            return Err(FsError::AlreadyExists(path.to_string()));
        }
        // Two structures change together: an inode is claimed, and a name
        // for it appears in the parent. The transaction keeps them in step.
        let number = self.alloc_inode(kind)?;
        self.dir_add(parent, name, number)?;
        Ok(number)
    }

    /// Create an empty file. Returns its inode number.
    pub fn create(&mut self, path: &str) -> FsResult<u32> {
        self.atomic(|fs| fs.make_node(path, InodeKind::File))
    }

    /// Replace the content of the file at `path` with `data`, creating the
    /// file if it does not exist. Returns its inode number.
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> FsResult<u32> {
        // Checked first, so an impossible write does not destroy what is there.
        if data.len() > MAX_FILE_SIZE {
            return Err(FsError::FileTooLarge);
        }
        let number = self.atomic(|fs| match fs.resolve(path) {
            Ok(number) => {
                let mut inode = fs.read_inode(number)?;
                if inode.kind != InodeKind::File {
                    return Err(FsError::IsADirectory(path.to_string()));
                }
                fs.release_blocks(&mut inode)?;
                fs.write_inode(number, &inode)?;
                Ok(number)
            }
            Err(FsError::NotFound(_)) => fs.make_node(path, InodeKind::File),
            Err(other) => Err(other),
        })?;
        self.write_at(number, 0, data)?;
        Ok(number)
    }

    /// The whole content of the file at `path`.
    pub fn read_file(&mut self, path: &str) -> FsResult<Vec<u8>> {
        let number = self.resolve(path)?;
        let inode = self.read_inode(number)?;
        if inode.kind != InodeKind::File {
            return Err(FsError::IsADirectory(path.to_string()));
        }
        let mut contents = vec![0; inode.size as usize];
        self.read_at(number, 0, &mut contents)?;
        Ok(contents)
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Create an empty directory. Returns its inode number.
    pub fn mkdir(&mut self, path: &str) -> FsResult<u32> {
        self.atomic(|fs| fs.make_node(path, InodeKind::Dir))
    }

    /// The entries of the directory at `path`, sorted by name.
    pub fn list(&mut self, path: &str) -> FsResult<Vec<DirEntry>> {
        let number = self.resolve(path)?;
        if self.read_inode(number)?.kind != InodeKind::Dir {
            return Err(FsError::NotADirectory(path.to_string()));
        }
        let mut entries = self.dir_list(number)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Remove the name `path`. The file itself is deleted only when its
    /// last name goes.
    pub fn unlink(&mut self, path: &str) -> FsResult<()> {
        self.atomic(|fs| {
            let (chain, name) = fs.walk_parent(path)?;
            let parent = chain[chain.len() - 1];
            let found = fs.dir_lookup(parent, name)?;
            let number = found.ok_or_else(|| FsError::NotFound(path.to_string()))?;
            let mut inode = fs.read_inode(number)?;
            if inode.kind != InodeKind::File {
                return Err(FsError::IsADirectory(path.to_string()));
            }
            fs.dir_remove(parent, name)?;
            inode.nlink = inode.nlink.saturating_sub(1);
            if inode.nlink > 0 {
                // Other names still lead here, so the content stays.
                return fs.write_inode(number, &inode);
            }
            fs.release_blocks(&mut inode)?;
            fs.free_inode(number)
        })
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Remove an empty directory.
    pub fn rmdir(&mut self, path: &str) -> FsResult<()> {
        self.atomic(|fs| {
            let (chain, name) = fs.walk_parent(path)?;
            let parent = chain[chain.len() - 1];
            let found = fs.dir_lookup(parent, name)?;
            let number = found.ok_or_else(|| FsError::NotFound(path.to_string()))?;
            let mut inode = fs.read_inode(number)?;
            if inode.kind != InodeKind::Dir {
                return Err(FsError::NotADirectory(path.to_string()));
            }
            // Removing a directory that still has entries would strand
            // everything beneath it.
            if !fs.dir_list(number)?.is_empty() {
                return Err(FsError::DirectoryNotEmpty(path.to_string()));
            }
            fs.dir_remove(parent, name)?;
            // An empty directory can still own blocks: the slots of entries
            // that were removed from it.
            fs.release_blocks(&mut inode)?;
            fs.free_inode(number)
        })
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Give the file at `existing` a second name, `new`.
    ///
    /// Both names are equals: each is just a directory entry holding the
    /// same inode number, and the inode counts how many there are.
    pub fn link(&mut self, existing: &str, new: &str) -> FsResult<()> {
        self.atomic(|fs| {
            let number = fs.resolve(existing)?;
            let mut inode = fs.read_inode(number)?;
            // A second name for a directory could create a loop in the tree.
            if inode.kind != InodeKind::File {
                return Err(FsError::IsADirectory(existing.to_string()));
            }
            let (chain, name) = fs.walk_parent(new)?;
            let parent = chain[chain.len() - 1];
            if fs.dir_lookup(parent, name)?.is_some() {
                return Err(FsError::AlreadyExists(new.to_string()));
            }
            inode.nlink = inode.nlink.checked_add(1).ok_or(FsError::TooManyLinks)?;
            fs.write_inode(number, &inode)?;
            fs.dir_add(parent, name, number)
        })
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Move or rename the file or directory at `from` to `to`.
    ///
    /// Nothing is copied, however large the thing being moved. Its name is
    /// taken out of one directory and put into another, and the inode stays
    /// where it is.
    pub fn rename(&mut self, from: &str, to: &str) -> FsResult<()> {
        self.atomic(|fs| {
            let (from_chain, from_name) = fs.walk_parent(from)?;
            let from_parent = from_chain[from_chain.len() - 1];
            let found = fs.dir_lookup(from_parent, from_name)?;
            let number = found.ok_or_else(|| FsError::NotFound(from.to_string()))?;

            let (to_chain, to_name) = fs.walk_parent(to)?;
            let to_parent = to_chain[to_chain.len() - 1];
            // If the thing being moved is on the way to its destination, it
            // is a directory being moved into itself. That would detach it,
            // and everything in it, from the root.
            if to_chain.contains(&number) {
                return Err(FsError::InvalidPath(format!(
                    "`{from}` cannot be moved inside itself"
                )));
            }
            if fs.dir_lookup(to_parent, to_name)?.is_some() {
                return Err(FsError::AlreadyExists(to.to_string()));
            }
            fs.dir_remove(from_parent, from_name)?;
            fs.dir_add(to_parent, to_name, number)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{BLOCK_SIZE, MemDisk};
    use crate::fs::FormatOptions;
    use crate::layout::ROOT_INODE;

    fn fresh() -> FileSystem<MemDisk> {
        FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap()
    }

    #[test]
    fn an_empty_file_system_is_all_free() {
        let mut fs = fresh();
        let usage = fs.usage().unwrap();
        assert_eq!((usage.data_blocks, usage.free_blocks), (469, 469));
        // 127 usable inodes, one of them taken by the root directory.
        assert_eq!((usage.inodes, usage.free_inodes), (127, 126));
    }

    #[test]
    fn stat_reports_size_and_the_blocks_behind_it() {
        let mut fs = fresh();
        let file = fs
            .atomic(|fs| {
                let file = fs.alloc_inode(InodeKind::File)?;
                fs.dir_add(ROOT_INODE, "big", file)?;
                Ok(file)
            })
            .unwrap();
        // 20 blocks of content: 12 direct, 8 behind the indirect block.
        fs.write_at(file, 0, &vec![1; 20 * BLOCK_SIZE]).unwrap();

        let stat = fs.stat("/big").unwrap();
        assert_eq!(
            (stat.inode, stat.kind, stat.links),
            (file, InodeKind::File, 1)
        );
        assert_eq!(stat.size as usize, 20 * BLOCK_SIZE);
        assert_eq!(stat.blocks, 21);

        // The root holds one entry, in one block.
        let root = fs.stat("/").unwrap();
        assert_eq!((root.kind, root.size, root.blocks), (InodeKind::Dir, 64, 1));
        // 21 blocks for the file and 1 for the root directory are gone.
        let usage = fs.usage().unwrap();
        assert_eq!((usage.free_blocks, usage.free_inodes), (469 - 22, 125));
    }
}
