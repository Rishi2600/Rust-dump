use crate::bytes::{get_u16, get_u32, put_u16, put_u32};
use crate::error::{FsError, FsResult};
use crate::layout::{INODE_SIZE, INODES_PER_BLOCK, Superblock};

/// How many block pointers an inode holds directly.
pub const DIRECT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeKind {
    /// Not in use.
    Free,
    File,
    Dir,
}

/// Everything the file system knows about one file or directory, except its
/// name. Names live in directories and merely point at an inode.
///
/// ```text
/// bytes 0-1    kind
/// bytes 2-3    link count
/// bytes 4-7    size in bytes
/// bytes 8-55   12 direct block pointers
/// bytes 56-59  indirect pointer
/// bytes 60-63  double indirect pointer
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inode {
    pub kind: InodeKind,
    /// How many directory entries point at this inode.
    pub nlink: u16,
    pub size: u32,
    /// The blocks holding the first `DIRECT` blocks of content. 0 means none.
    pub direct: [u32; DIRECT],
    /// A block full of further block pointers.
    pub indirect: u32,
    /// A block full of pointers to blocks full of block pointers.
    pub double_indirect: u32,
}

impl Inode {
    /// An empty file or directory with one name pointing at it.
    pub fn new(kind: InodeKind) -> Self {
        Self {
            kind,
            nlink: 1,
            size: 0,
            direct: [0; DIRECT],
            indirect: 0,
            double_indirect: 0,
        }
    }

    /// Write the 64-byte disk form into `raw`.
    pub fn encode(&self, raw: &mut [u8]) {
        let kind = match self.kind {
            InodeKind::Free => 0,
            InodeKind::File => 1,
            InodeKind::Dir => 2,
        };
        put_u16(raw, 0, kind);
        put_u16(raw, 2, self.nlink);
        put_u32(raw, 4, self.size);
        for (index, pointer) in self.direct.iter().enumerate() {
            put_u32(raw, 8 + index * 4, *pointer);
        }
        put_u32(raw, 56, self.indirect);
        put_u32(raw, 60, self.double_indirect);
    }

    pub fn decode(raw: &[u8]) -> FsResult<Self> {
        let kind = match get_u16(raw, 0) {
            0 => InodeKind::Free,
            1 => InodeKind::File,
            2 => InodeKind::Dir,
            other => return Err(FsError::Corrupt(format!("inode has unknown kind {other}"))),
        };
        let mut direct = [0; DIRECT];
        for (index, pointer) in direct.iter_mut().enumerate() {
            *pointer = get_u32(raw, 8 + index * 4);
        }
        Ok(Self {
            kind,
            nlink: get_u16(raw, 2),
            size: get_u32(raw, 4),
            direct,
            indirect: get_u32(raw, 56),
            double_indirect: get_u32(raw, 60),
        })
    }
}

impl Superblock {
    /// Which block of the inode table holds inode `number`, and at what
    /// byte offset inside that block.
    pub fn inode_location(&self, number: u32) -> FsResult<(u32, usize)> {
        if number == 0 || number >= self.inode_count {
            return Err(FsError::Corrupt(format!("there is no inode {number}")));
        }
        let block = self.inode_table_start + number / INODES_PER_BLOCK as u32;
        let offset = (number as usize % INODES_PER_BLOCK) * INODE_SIZE;
        Ok((block, offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inode_is_exactly_64_bytes_on_disk() {
        let mut inode = Inode::new(InodeKind::File);
        inode.nlink = 2;
        inode.size = 0x0001_0203;
        inode.direct[0] = 50;
        inode.direct[11] = 61;
        inode.indirect = 70;
        inode.double_indirect = 80;

        let mut raw = [0xEE; INODE_SIZE];
        inode.encode(&mut raw);
        assert_eq!(raw[..8], [1, 0, 2, 0, 0x03, 0x02, 0x01, 0x00]);
        assert_eq!(raw[8..12], [50, 0, 0, 0]);
        assert_eq!(raw[52..64], [61, 0, 0, 0, 70, 0, 0, 0, 80, 0, 0, 0]);
        assert_eq!(Inode::decode(&raw), Ok(inode));
    }

    #[test]
    fn a_zeroed_slot_is_a_free_inode() {
        let inode = Inode::decode(&[0; INODE_SIZE]).unwrap();
        assert_eq!(inode.kind, InodeKind::Free);
        assert_eq!((inode.nlink, inode.size), (0, 0));

        let mut garbage = [0; INODE_SIZE];
        garbage[0] = 9;
        assert!(matches!(Inode::decode(&garbage), Err(FsError::Corrupt(_))));
    }

    #[test]
    fn inodes_are_packed_sixteen_to_a_block() {
        let sb = Superblock::plan(512, 128, 32).unwrap();
        assert_eq!(sb.inode_location(1), Ok((35, 64)));
        assert_eq!(sb.inode_location(15), Ok((35, 960)));
        assert_eq!(sb.inode_location(16), Ok((36, 0)));
        assert_eq!(sb.inode_location(127), Ok((42, 960)));
        // Inode 0 means "none", and 128 is past the end of the table.
        assert!(sb.inode_location(0).is_err());
        assert!(sb.inode_location(128).is_err());
    }
}
