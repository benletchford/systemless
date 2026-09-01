//! VFS file summary and inspection records.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsFileSummary {
    pub path: String,
    pub data_len: usize,
    pub resource_len: usize,
    pub data_hash: u64,
    pub resource_hash: u64,
    pub file_type: u32,
    pub creator: u32,
    pub finder_flags: u16,
    pub created_date: u32,
    pub modified_date: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsFileStat {
    pub path: String,
    pub data_len: usize,
    pub resource_len: usize,
    pub file_type: u32,
    pub creator: u32,
    pub finder_flags: u16,
    pub created_date: u32,
    pub modified_date: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsFileSnapshot {
    pub path: String,
    pub data_fork: Vec<u8>,
    pub resource_fork: Vec<u8>,
    pub file_type: u32,
    pub creator: u32,
    pub finder_flags: u16,
    pub created_date: u32,
    pub modified_date: u32,
}

pub(crate) fn vfs_fork_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
