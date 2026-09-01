//! PowerPC Virtual File System, Volume, Scrap, and List records.

use crate::process_context::{
    ProcessForkBytes, ProcessOpenFileRecord, ProcessResourceFileRecord,
    ProcessStdioStreamRecord, ProcessVfsFileRecord, ProcessVfsResourceFileRecord,
    ProcessVfsResourceRecord,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcVfsDirectory {
    pub dir_id: u32,
    pub parent_dir_id: u32,
    pub path: String,
    pub creator: u32,
    pub file_type: u32,
    pub finder_flags: u16,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcVfsVolumeRecord {
    pub ref_num: i16,
    pub name: String,
    pub root_dir_id: u32,
    pub attributes: u16,
    pub file_count: u16,
    pub allocation_block_count: u16,
    pub allocation_block_size: u32,
    pub clump_size: u32,
    pub free_blocks: u16,
    pub bitmap_start: u16,
    pub allocation_pointer: u16,
    pub allocation_start: u16,
    pub next_catalog_id: u32,
    pub created_date: u32,
    pub modified_date: u32,
}

pub type PpcFileRecord = ProcessOpenFileRecord;
pub(crate) type PpcStdioStreamRecord = ProcessStdioStreamRecord;
pub type PpcVfsFileRecord = ProcessVfsFileRecord;
pub type PpcResourceFileRecord = ProcessResourceFileRecord;
pub type PpcVfsResourceFileRecord = ProcessVfsResourceFileRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcVfsResourceForkExport {
    pub path: String,
    pub data: Vec<u8>,
    pub creator: u32,
    pub file_type: u32,
    pub finder_flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcVfsFileExport {
    pub path: String,
    pub data: ProcessForkBytes,
    pub creator: u32,
    pub file_type: u32,
    pub finder_flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcVfsDirectoryExport {
    pub path: String,
    pub creator: u32,
    pub file_type: u32,
    pub finder_flags: u16,
}

pub type PpcVfsResourceRecord = ProcessVfsResourceRecord;

#[derive(Debug, Clone, Default)]
pub struct PpcScrapState {
    pub(crate) private_text_handle: u32,
    pub(crate) desktop_initialized: bool,
    pub(crate) desktop_flavors: Vec<(u32, Vec<u8>)>,
}

#[derive(Debug, Clone)]
pub(crate) struct PpcListRecord {
    pub(crate) handle: u32,
    pub(crate) cells_handle: u32,
    pub(crate) data_bounds: (i16, i16, i16, i16),
    pub(crate) cells: Vec<Vec<u8>>,
    pub(crate) selected: Vec<bool>,
    pub(crate) draw_enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PpcListManagerState {
    pub(crate) lists: Vec<PpcListRecord>,
}
