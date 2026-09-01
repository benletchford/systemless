//! CFM imported symbol binding, trace, and probe records.

use super::sprockets::{PpcDrawSprocketTraceEntry, PpcInputSprocketSimpleStateTraceEntry};
use super::PpcImportDispatcherTarget;
use crate::trap::dispatch::key_map_key_is_down;
use ppc::{PpcFetchHistogram, PpcRunResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcImportBinding {
    pub library_index: u32,
    pub symbol_index: u32,
    pub library_name: String,
    pub symbol_name: String,
    pub class: u8,
    pub weak: bool,
    pub address: u32,
    pub tvector_address: Option<u32>,
    pub trap_pc: u32,
    pub dispatcher_target: PpcImportDispatcherTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcStartupProbe {
    pub result: PpcRunResult,
    pub first_import_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcHleImportTraceEntry {
    pub import_index: u32,
    pub library_name: String,
    pub symbol_name: String,
    pub pc: u32,
    pub lr: u32,
    pub rtoc: u32,
    pub sp: u32,
    pub dispatcher_target: PpcImportDispatcherTarget,
    pub repeat_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcHleRunProbe {
    pub result: PpcRunResult,
    pub handled_import_count: u32,
    pub last_import_index: Option<u32>,
    pub unsupported_import_index: Option<u32>,
    pub import_trace: Vec<PpcHleImportTraceEntry>,
    pub draw_sprocket_trace: Vec<PpcDrawSprocketTraceEntry>,
    pub input_sprocket_trace: Vec<PpcInputSprocketSimpleStateTraceEntry>,
    pub fetch_histogram: Option<PpcFetchHistogram>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcCfmConnection {
    pub id: u32,
    pub library_name: String,
    pub main_addr: u32,
    pub init_addr: u32,
    pub term_addr: u32,
    pub exports: Vec<PpcCfmExport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcCfmExport {
    pub name: String,
    pub class: u8,
    pub address: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcCfmLibraryFragment {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcPreparedMemFragment {
    pub(crate) main_addr: u32,
    pub(crate) init_addr: u32,
    pub(crate) term_addr: u32,
    pub(crate) exports: Vec<PpcCfmExport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PpcInputSnapshot {
    pub key_map: [u8; 16],
    pub mouse_button: bool,
    pub mouse_v: i16,
    pub mouse_h: i16,
}

impl PpcInputSnapshot {
    pub fn key_down(&self, key_code: u8) -> bool {
        key_map_key_is_down(&self.key_map, key_code)
    }

    pub fn any_key_down(&self, key_codes: &[u8]) -> bool {
        key_codes.iter().copied().any(|key| self.key_down(key))
    }

    pub fn is_idle(&self) -> bool {
        !self.mouse_button && self.key_map == [0; 16]
    }
}
