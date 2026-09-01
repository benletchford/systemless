//! PowerPC Virtual File System, Volume, Scrap, and List records.

use crate::process_context::{
    ProcessForkBytes, ProcessOpenFileRecord, ProcessResourceFileRecord, ProcessStdioStreamRecord,
    ProcessVfsDirectory, ProcessVfsFileRecord, ProcessVfsResourceFileRecord,
    ProcessVfsResourceRecord, ProcessVfsVolumeRecord, SharedProcessScrapState,
    SharedProcessValue,
};
use crate::list_manager::{ProcessListManagerState, ProcessListRecord};

pub type PpcVfsDirectory = ProcessVfsDirectory;
pub type PpcVfsVolumeRecord = ProcessVfsVolumeRecord;

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
    pub(crate) desktop: SharedProcessScrapState,
}

pub(crate) type PpcListRecord = ProcessListRecord;
pub type PpcListManagerState = SharedProcessValue<ProcessListManagerState>;
