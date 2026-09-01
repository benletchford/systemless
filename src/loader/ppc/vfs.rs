//! PowerPC Virtual File System, Volume, Scrap, and List records.

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcFileRecord {
    pub ref_num: i16,
    pub path: String,
    pub position: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcStdioStreamRecord {
    pub(crate) ref_num: Option<i16>,
    pub(crate) path: Option<String>,
    pub(crate) position: u32,
    pub(crate) standard: bool,
    pub(crate) readable: bool,
    pub(crate) writable: bool,
    pub(crate) append: bool,
    pub(crate) closed: bool,
    pub(crate) eof: bool,
    pub(crate) error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcVfsFileRecord {
    pub path: String,
    pub data: Vec<u8>,
    pub creator: u32,
    pub file_type: u32,
    pub finder_flags: u16,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcResourceFileRecord {
    pub ref_num: i16,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcVfsResourceFileRecord {
    pub path: String,
    pub creator: u32,
    pub file_type: u32,
    pub finder_flags: u16,
    pub resource_len: u32,
    pub raw_data: Option<Vec<u8>>,
    pub map_attrs: u16,
    pub dirty: bool,
}

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
    pub data: Vec<u8>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcVfsResourceRecord {
    pub ref_num: i16,
    pub path: String,
    pub res_type: u32,
    pub res_id: i16,
    pub name: Vec<u8>,
    pub data: Vec<u8>,
    pub raw_data: Option<Vec<u8>>,
    pub raw_attrs: Option<u16>,
    pub attrs: u16,
    pub handle: u32,
}

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
