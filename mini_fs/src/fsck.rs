use crate::device::BlockDevice;
use crate::error::FsResult;
use crate::fs::FileSystem;
use crate::inode::InodeKind;
use crate::layout::ROOT_INODE;

impl<D: BlockDevice> FileSystem<D> {
    /// Check that the file system agrees with itself. Returns a description
    /// of every inconsistency found; an empty list means it is sound.
    ///
    /// The same facts are recorded in more than one place: a block is in use
    /// if a bitmap says so *and* if some inode points at it. The checker
    /// rebuilds each fact from the directory tree and compares.
    pub fn check(&mut self) -> FsResult<Vec<String>> {
        let sb = self.sb;
        let mut problems = Vec::new();

        // Pass 1: walk the tree from the root, counting the names that
        // point at each inode.
        let mut names = vec![0u32; sb.inode_count as usize];
        names[ROOT_INODE as usize] = 1;
        let mut to_visit = vec![ROOT_INODE];
        while let Some(dir) = to_visit.pop() {
            for entry in self.dir_list(dir)? {
                let target = entry.inode;
                let describe = format!("entry `{}` in directory {dir}", entry.name);
                if target >= sb.inode_count {
                    problems.push(format!(
                        "{describe} points at inode {target}, which does not exist"
                    ));
                    continue;
                }
                names[target as usize] += 1;
                match self.read_inode(target)?.kind {
                    InodeKind::Free => {
                        problems.push(format!(
                            "{describe} points at inode {target}, which is free"
                        ));
                    }
                    InodeKind::Dir if names[target as usize] == 1 => to_visit.push(target),
                    InodeKind::Dir => {
                        problems.push(format!(
                            "directory {target} is reachable through more than one name"
                        ));
                    }
                    InodeKind::File => {}
                }
            }
        }

        // Pass 2: every inode. Its bitmap bit and link count must match what
        // the walk found, and no two inodes may claim the same block.
        let mut owner = vec![0u32; sb.total_blocks as usize];
        for number in 1..sb.inode_count {
            let inode = self.read_inode(number)?;
            let marked = sb.inode_bitmap().get(&mut self.disk, number)?;
            let found = names[number as usize];
            if found > 0 && !marked {
                problems.push(format!(
                    "inode {number} is in use but its bitmap bit says free"
                ));
            }
            if found == 0 && marked {
                problems.push(format!(
                    "inode {number} is marked used but no directory entry refers to it"
                ));
            }
            if inode.kind == InodeKind::Free {
                continue;
            }
            if found > 0 && u32::from(inode.nlink) != found {
                problems.push(format!(
                    "inode {number} says {} links but {found} names point at it",
                    inode.nlink
                ));
            }
            for block in self.block_list(&inode)? {
                if block < sb.data_start || block >= sb.total_blocks {
                    problems.push(format!(
                        "inode {number} points at block {block}, outside the data area"
                    ));
                } else if owner[block as usize] != 0 {
                    let other = owner[block as usize];
                    problems.push(format!(
                        "block {block} is claimed by both inode {other} and inode {number}"
                    ));
                } else {
                    owner[block as usize] = number;
                }
            }
        }

        // Pass 3: the block bitmap must say exactly what pass 2 found.
        for block in sb.data_start..sb.total_blocks {
            let marked = sb.block_bitmap().get(&mut self.disk, block)?;
            let user = owner[block as usize];
            if marked && user == 0 {
                problems.push(format!(
                    "block {block} is marked used but belongs to no file"
                ));
            }
            if !marked && user != 0 {
                problems.push(format!(
                    "block {block} is in use by inode {user} but its bitmap bit says free"
                ));
            }
        }
        Ok(problems)
    }
}
