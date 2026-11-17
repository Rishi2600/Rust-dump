use crate::device::BlockDevice;
use crate::error::FsResult;
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
