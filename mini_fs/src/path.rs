use crate::device::BlockDevice;
use crate::dir::validate_name;
use crate::error::{FsError, FsResult};
use crate::fs::FileSystem;
use crate::inode::InodeKind;
use crate::layout::ROOT_INODE;

impl<D: BlockDevice> FileSystem<D> {
    /// Follow `path` from the root, one component at a time.
    ///
    /// Returns the inode of every directory passed through, ending with the
    /// inode the path names. Every path starts at the root, so `.` is a step
    /// that goes nowhere and `..` simply drops the last step taken.
    pub fn walk(&mut self, path: &str) -> FsResult<Vec<u32>> {
        let mut chain = vec![ROOT_INODE];
        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    // The root is its own parent.
                    if chain.len() > 1 {
                        chain.pop();
                    }
                }
                name => {
                    let current = chain[chain.len() - 1];
                    if self.read_inode(current)?.kind != InodeKind::Dir {
                        return Err(FsError::NotADirectory(path.to_string()));
                    }
                    let next = self.dir_lookup(current, name)?;
                    chain.push(next.ok_or_else(|| FsError::NotFound(path.to_string()))?);
                }
            }
        }
        Ok(chain)
    }

    /// The inode that `path` names.
    pub fn resolve(&mut self, path: &str) -> FsResult<u32> {
        let chain = self.walk(path)?;
        Ok(chain[chain.len() - 1])
    }

    /// Split `path` into the directory that would contain it and its final
    /// name. The directory must exist; the name need not.
    ///
    /// This is what creating, removing and renaming all start with, since
    /// each of them edits the entries of the containing directory.
    pub fn walk_parent<'p>(&mut self, path: &'p str) -> FsResult<(Vec<u32>, &'p str)> {
        let trimmed = path.trim_end_matches('/');
        let (parent, name) = trimmed.rsplit_once('/').unwrap_or(("", trimmed));
        if name.is_empty() || name == "." || name == ".." {
            return Err(FsError::InvalidPath(format!(
                "`{path}` does not name an entry in a directory"
            )));
        }
        validate_name(name)?;
        let chain = self.walk(parent)?;
        if self.read_inode(chain[chain.len() - 1])?.kind != InodeKind::Dir {
            return Err(FsError::NotADirectory(parent.to_string()));
        }
        Ok((chain, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::MemDisk;
    use crate::fs::FormatOptions;

    /// A small tree built by hand:
    ///
    /// ```text
    /// /            inode 1
    ///   docs/      inode 2
    ///     deep/    inode 3
    ///     a.txt    inode 4
    /// ```
    fn tree() -> FileSystem<MemDisk> {
        let mut fs = FileSystem::format(MemDisk::new(512), FormatOptions::default()).unwrap();
        fs.atomic(|fs| {
            let docs = fs.alloc_inode(InodeKind::Dir)?;
            let deep = fs.alloc_inode(InodeKind::Dir)?;
            let file = fs.alloc_inode(InodeKind::File)?;
            fs.dir_add(ROOT_INODE, "docs", docs)?;
            fs.dir_add(docs, "deep", deep)?;
            fs.dir_add(docs, "a.txt", file)
        })
        .unwrap();
        fs
    }

    #[test]
    fn a_path_is_followed_one_directory_at_a_time() {
        let mut fs = tree();
        assert_eq!(fs.walk("/"), Ok(vec![1]));
        assert_eq!(fs.walk("/docs/deep"), Ok(vec![1, 2, 3]));
        assert_eq!(fs.resolve("/docs/a.txt"), Ok(4));
    }

    #[test]
    fn dots_and_extra_slashes_are_normalised_away() {
        let mut fs = tree();
        assert_eq!(fs.resolve("docs/a.txt"), Ok(4));
        assert_eq!(fs.resolve("//docs///a.txt"), Ok(4));
        assert_eq!(fs.resolve("/docs/./deep/"), Ok(3));
        assert_eq!(fs.resolve("/docs/deep/../a.txt"), Ok(4));
        // Going up from the root stays at the root.
        assert_eq!(fs.resolve("/../../docs"), Ok(2));
        assert_eq!(fs.resolve(""), Ok(1));
    }

    #[test]
    fn missing_names_and_files_used_as_directories_are_errors() {
        let mut fs = tree();
        assert_eq!(fs.resolve("/nope"), Err(FsError::NotFound("/nope".into())));
        assert_eq!(
            fs.resolve("/docs/nope/x"),
            Err(FsError::NotFound("/docs/nope/x".into()))
        );
        let through_file = fs.resolve("/docs/a.txt/x");
        assert_eq!(
            through_file,
            Err(FsError::NotADirectory("/docs/a.txt/x".into()))
        );
    }

    #[test]
    fn the_parent_is_resolved_and_the_last_name_is_handed_back() {
        let mut fs = tree();
        assert_eq!(fs.walk_parent("/docs/new.txt"), Ok((vec![1, 2], "new.txt")));
        assert_eq!(fs.walk_parent("top"), Ok((vec![1], "top")));
        assert_eq!(fs.walk_parent("/docs/deep/"), Ok((vec![1, 2], "deep")));

        assert!(matches!(fs.walk_parent("/"), Err(FsError::InvalidPath(_))));
        assert!(matches!(
            fs.walk_parent("/docs/.."),
            Err(FsError::InvalidPath(_))
        ));
        assert!(matches!(
            fs.walk_parent("/nope/x"),
            Err(FsError::NotFound(_))
        ));
        assert!(matches!(
            fs.walk_parent("/docs/a.txt/x"),
            Err(FsError::NotADirectory(_))
        ));
        let long = format!("/docs/{}", "n".repeat(60));
        assert!(matches!(
            fs.walk_parent(&long),
            Err(FsError::InvalidName(_))
        ));
    }
}
