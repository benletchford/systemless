//! Process-scoped state shared by classic and native CPU adapters.

use crate::callback_manager::{ProcessCallbackScheduling, ProcessTimerTask, ProcessVblTask};
use crate::control_manager::ProcessControlManagerState;
use crate::display::{
    default_arrow_cursor_image, default_display_gamma, standard_mac_8bpp_clut, CursorImage,
    DisplayGamma,
};
use crate::event_queue::EventQueue;
use crate::guest_call::SharedGuestCallStack;
use crate::guest_procedure::GuestProcedure;
use crate::list_manager::ProcessListManagerState;
use crate::memory::bus::SharedRamRegion;
use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};
use crate::menu_manager::{ProcessMenuTrackingState, SharedNativeMenuSelection};
use crate::sound::SoundManager;
use crate::text_edit::ProcessTextEditManagerState;
use ppc::PpcMemory;
use std::cell::{RefCell, RefMut, UnsafeCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::hash::Hash;
use std::rc::Rc;

#[derive(Debug)]
struct ProcessMemoryRegion {
    base: u32,
    bytes: SharedRamRegion,
}

/// A process-owned file fork whose attached views share bytes immediately.
///
/// Ordinary clones are detached snapshots. `shared_handle` is reserved for
/// installing another index over the same process fork, such as the native
/// File Manager record and the classic VFS path map.
pub struct ProcessForkBytes(Rc<UnsafeCell<Vec<u8>>>);

impl Default for ProcessForkBytes {
    fn default() -> Self {
        Self::from(Vec::new())
    }
}

impl Clone for ProcessForkBytes {
    fn clone(&self) -> Self {
        Self::from((**self).clone())
    }
}

impl fmt::Debug for ProcessForkBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ProcessForkBytes")
            .field(&**self)
            .finish()
    }
}

impl PartialEq for ProcessForkBytes {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl PartialEq<Vec<u8>> for ProcessForkBytes {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<const N: usize> PartialEq<[u8; N]> for ProcessForkBytes {
    fn eq(&self, other: &[u8; N]) -> bool {
        self.as_slice() == other
    }
}

impl<const N: usize> PartialEq<&[u8; N]> for ProcessForkBytes {
    fn eq(&self, other: &&[u8; N]) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for ProcessForkBytes {}

impl From<Vec<u8>> for ProcessForkBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self(Rc::new(UnsafeCell::new(bytes)))
    }
}

impl AsRef<[u8]> for ProcessForkBytes {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl std::ops::Deref for ProcessForkBytes {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: process adapters are serialized by the runner, and normal
        // clones detach instead of creating an alias.
        unsafe { &*self.0.get() }
    }
}

impl std::ops::DerefMut for ProcessForkBytes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: see `Deref`.
        unsafe { &mut *self.0.get() }
    }
}

impl ProcessForkBytes {
    pub(crate) fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    #[cfg(test)]
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Path index for process-owned fork bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessForkMap(HashMap<String, ProcessForkBytes>);

impl std::ops::Deref for ProcessForkMap {
    type Target = HashMap<String, ProcessForkBytes>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ProcessForkMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ProcessForkMap {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn insert(
        &mut self,
        path: String,
        bytes: impl Into<ProcessForkBytes>,
    ) -> Option<ProcessForkBytes> {
        self.0.insert(path, bytes.into())
    }

    pub fn get<Q>(&self, path: &Q) -> Option<&Vec<u8>>
    where
        String: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.0.get(path).map(|bytes| &**bytes)
    }

    pub fn get_mut<Q>(&mut self, path: &Q) -> Option<&mut Vec<u8>>
    where
        String: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.0.get_mut(path).map(|bytes| &mut **bytes)
    }

    pub(crate) fn get_shared<Q>(&self, path: &Q) -> Option<&ProcessForkBytes>
    where
        String: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.0.get(path)
    }

    pub(crate) fn insert_shared(
        &mut self,
        path: String,
        bytes: &ProcessForkBytes,
    ) -> Option<ProcessForkBytes> {
        self.0.insert(path, bytes.shared_handle())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessVfsFileRecord {
    pub path: String,
    pub data: ProcessForkBytes,
    pub creator: u32,
    pub file_type: u32,
    pub finder_flags: u16,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessVfsDirectory {
    pub dir_id: u32,
    pub parent_dir_id: u32,
    pub path: String,
    pub creator: u32,
    pub file_type: u32,
    pub finder_flags: u16,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessVfsVolumeRecord {
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProcessVfsMetadata {
    pub file_id: u32,
    pub parent_dir_id: u32,
    pub file_type: u32,
    pub creator: u32,
    pub finder_flags: u16,
    pub created_date: u32,
    pub modified_date: u32,
}

fn process_native_vfs_catalogue_is_pristine(
    volumes: &[ProcessVfsVolumeRecord],
    directories: &[ProcessVfsDirectory],
) -> bool {
    if !volumes.is_empty() {
        return false;
    }
    let expected = [
        ("", 2, 1),
        ("System Folder", 16, 2),
        ("System Folder/Preferences", 17, 16),
    ];
    directories.len() <= expected.len()
        && directories.iter().all(|directory| {
            !directory.dirty
                && expected.iter().any(|(path, dir_id, parent_dir_id)| {
                    directory.path == *path
                        && directory.dir_id == *dir_id
                        && directory.parent_dir_id == *parent_dir_id
                })
        })
}

fn process_vfs_directories_are_pristine(directories: &[ProcessVfsDirectory]) -> bool {
    if directories.is_empty() {
        return true;
    }
    let expected = [
        ("", 2, 1),
        ("System Folder", 16, 2),
        ("System Folder/Preferences", 17, 16),
    ];
    directories.len() <= expected.len()
        && directories.iter().all(|directory| {
            !directory.dirty
                && expected.iter().any(|(path, dir_id, parent_dir_id)| {
                    directory.path == *path
                        && directory.dir_id == *dir_id
                        && directory.parent_dir_id == *parent_dir_id
                })
        })
}

/// Native record index backed by the canonical process data-fork map.
#[derive(Debug, Default)]
pub(crate) struct ProcessVfsFileRecords {
    records: Vec<ProcessVfsFileRecord>,
    data_forks: SharedProcessValue<ProcessForkMap>,
}

impl Clone for ProcessVfsFileRecords {
    fn clone(&self) -> Self {
        Self::from(self.records.clone())
    }
}

impl From<Vec<ProcessVfsFileRecord>> for ProcessVfsFileRecords {
    fn from(records: Vec<ProcessVfsFileRecord>) -> Self {
        let mut result = Self::default();
        for record in records {
            result.push(record);
        }
        result
    }
}

impl std::ops::Deref for ProcessVfsFileRecords {
    type Target = Vec<ProcessVfsFileRecord>;

    fn deref(&self) -> &Self::Target {
        &self.records
    }
}

impl std::ops::DerefMut for ProcessVfsFileRecords {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.records
    }
}

impl ProcessVfsFileRecords {
    pub(crate) fn push(&mut self, record: ProcessVfsFileRecord) {
        if !record.path.is_empty() {
            self.data_forks
                .insert_shared(record.path.clone(), &record.data);
        }
        self.records.push(record);
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&ProcessVfsFileRecord) -> bool) {
        self.records.retain(|record| keep(record));
        self.data_forks.retain(|path, _| {
            self.records
                .iter()
                .any(|record| record.path.eq_ignore_ascii_case(path))
        });
    }

    pub(crate) fn replace(&mut self, records: Vec<ProcessVfsFileRecord>) {
        self.records.clear();
        self.data_forks.clear();
        for record in records {
            self.push(record);
        }
    }

    fn merge_from(&mut self, source: &mut Self) {
        for record in source.records.drain(..) {
            if self
                .records
                .iter()
                .any(|existing| existing.path.eq_ignore_ascii_case(&record.path))
            {
                continue;
            }
            self.push(record);
        }
        let forks = source.data_forks.drain().collect::<Vec<_>>();
        for (path, bytes) in forks {
            if self
                .data_forks
                .keys()
                .any(|existing| existing.eq_ignore_ascii_case(&path))
            {
                continue;
            }
            self.data_forks.insert_shared(path, &bytes);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOpenFileRecord {
    pub ref_num: i16,
    pub path: String,
    pub position: u32,
}

/// Detached-by-default open-file records for one process File Manager.
///
/// Native code uses the record slice directly. The classic adapter installs
/// map-shaped path and position views over the same allocation so both ABIs
/// observe one refnum and one file mark during Mixed Mode calls.
#[derive(Debug)]
pub(crate) struct SharedProcessOpenFiles(Rc<UnsafeCell<Vec<ProcessOpenFileRecord>>>);

impl Default for SharedProcessOpenFiles {
    fn default() -> Self {
        Self(Rc::new(UnsafeCell::new(Vec::new())))
    }
}

impl Clone for SharedProcessOpenFiles {
    fn clone(&self) -> Self {
        Self::from_records((**self).clone())
    }
}

impl std::ops::Deref for SharedProcessOpenFiles {
    type Target = Vec<ProcessOpenFileRecord>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: process adapters execute serially and ordinary clones detach.
        unsafe { &*self.0.get() }
    }
}

impl std::ops::DerefMut for SharedProcessOpenFiles {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: see `Deref`.
        unsafe { &mut *self.0.get() }
    }
}

impl SharedProcessOpenFiles {
    fn from_records(records: Vec<ProcessOpenFileRecord>) -> Self {
        Self(Rc::new(UnsafeCell::new(records)))
    }

    pub(crate) fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    pub(crate) fn positions(&self) -> SharedProcessOpenFilePositions {
        SharedProcessOpenFilePositions(Rc::clone(&self.0))
    }

    pub(crate) fn get(&self, ref_num: &u16) -> Option<&String> {
        let ref_num = i16::try_from(*ref_num).ok()?;
        self.iter()
            .find(|record| record.ref_num == ref_num)
            .map(|record| &record.path)
    }

    pub(crate) fn insert(&mut self, ref_num: u16, path: String) -> Option<String> {
        let Ok(ref_num) = i16::try_from(ref_num) else {
            return None;
        };
        if let Some(record) = self.iter_mut().find(|record| record.ref_num == ref_num) {
            return Some(std::mem::replace(&mut record.path, path));
        }
        self.push(ProcessOpenFileRecord {
            ref_num,
            path,
            position: 0,
        });
        None
    }

    pub(crate) fn remove(&mut self, ref_num: &u16) -> Option<String> {
        let ref_num = i16::try_from(*ref_num).ok()?;
        let index = self.iter().position(|record| record.ref_num == ref_num)?;
        Some(Vec::remove(self, index).path)
    }

    pub(crate) fn contains_key(&self, ref_num: &u16) -> bool {
        self.get(ref_num).is_some()
    }

    #[cfg(test)]
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Classic File Manager position-map view over process open-file records.
#[derive(Debug)]
pub(crate) struct SharedProcessOpenFilePositions(
    Rc<UnsafeCell<Vec<ProcessOpenFileRecord>>>,
);

impl SharedProcessOpenFilePositions {
    pub(crate) fn get(&self, ref_num: &u16) -> Option<&u32> {
        let ref_num = i16::try_from(*ref_num).ok()?;
        // SAFETY: this view shares the same serialized process allocation as
        // `SharedProcessOpenFiles`.
        unsafe { &*self.0.get() }
            .iter()
            .find(|record| record.ref_num == ref_num)
            .map(|record| &record.position)
    }

    pub(crate) fn get_mut(&mut self, ref_num: &u16) -> Option<&mut u32> {
        let ref_num = i16::try_from(*ref_num).ok()?;
        // SAFETY: see `get`; mutable adapter access is serialized.
        unsafe { &mut *self.0.get() }
            .iter_mut()
            .find(|record| record.ref_num == ref_num)
            .map(|record| &mut record.position)
    }

    pub(crate) fn insert(&mut self, ref_num: u16, position: usize) -> Option<usize> {
        let position = u32::try_from(position).unwrap_or(u32::MAX);
        if let Some(old) = self.get(&ref_num).copied() {
            *self
                .get_mut(&ref_num)
                .expect("open file disappeared while updating its position") = position;
            return usize::try_from(old).ok();
        }
        let Ok(ref_num) = i16::try_from(ref_num) else {
            return None;
        };
        // Some focused File Manager fixtures seed the mark before the path.
        // A later path insertion fills this same canonical record.
        unsafe { &mut *self.0.get() }.push(ProcessOpenFileRecord {
            ref_num,
            path: String::new(),
            position,
        });
        None
    }

    pub(crate) fn remove(&mut self, ref_num: &u16) -> Option<usize> {
        self.get(ref_num)
            .copied()
            .and_then(|position| usize::try_from(position).ok())
    }

    #[cfg(test)]
    pub(crate) fn contains_key(&self, ref_num: &u16) -> bool {
        self.get(ref_num).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessStdioStreamRecord {
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
pub struct ProcessResourceFileRecord {
    pub ref_num: i16,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessVfsResourceFileRecord {
    pub path: String,
    pub creator: u32,
    pub file_type: u32,
    pub finder_flags: u16,
    pub resource_len: u32,
    pub raw_data: Option<ProcessForkBytes>,
    pub map_attrs: u16,
    pub dirty: bool,
}

/// Native resource-file index backed by the canonical process fork map.
#[derive(Debug, Default)]
pub(crate) struct ProcessVfsResourceFileRecords {
    records: Vec<ProcessVfsResourceFileRecord>,
    resource_forks: SharedProcessValue<ProcessForkMap>,
}

impl Clone for ProcessVfsResourceFileRecords {
    fn clone(&self) -> Self {
        let mut result = Self::from(self.records.clone());
        for (path, bytes) in self.resource_forks.iter() {
            result.update_fork(path, bytes);
        }
        result
    }
}

impl From<Vec<ProcessVfsResourceFileRecord>> for ProcessVfsResourceFileRecords {
    fn from(records: Vec<ProcessVfsResourceFileRecord>) -> Self {
        let mut result = Self::default();
        for record in records {
            result.push(record);
        }
        result
    }
}

impl std::ops::Deref for ProcessVfsResourceFileRecords {
    type Target = Vec<ProcessVfsResourceFileRecord>;

    fn deref(&self) -> &Self::Target {
        &self.records
    }
}

impl std::ops::DerefMut for ProcessVfsResourceFileRecords {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.records
    }
}

impl ProcessVfsResourceFileRecords {
    pub(crate) fn push(&mut self, record: ProcessVfsResourceFileRecord) {
        if !record.path.is_empty() {
            if let Some(raw_data) = &record.raw_data {
                self.resource_forks
                    .insert_shared(record.path.clone(), raw_data);
            } else if !self.resource_forks.contains_key(&record.path) {
                self.resource_forks.insert(record.path.clone(), Vec::new());
            }
        }
        self.records.push(record);
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&ProcessVfsResourceFileRecord) -> bool) {
        self.records.retain(|record| keep(record));
        self.resource_forks.retain(|path, _| {
            self.records
                .iter()
                .any(|record| record.path.eq_ignore_ascii_case(path))
        });
    }

    pub(crate) fn replace(&mut self, records: Vec<ProcessVfsResourceFileRecord>) {
        self.records.clear();
        self.resource_forks.clear();
        for record in records {
            self.push(record);
        }
    }

    pub(crate) fn update_fork(&mut self, path: &str, bytes: &[u8]) {
        let key = self
            .resource_forks
            .keys()
            .find(|candidate| candidate.eq_ignore_ascii_case(path))
            .cloned()
            .unwrap_or_else(|| path.to_string());
        if let Some(target) = self.resource_forks.get_mut(&key) {
            target.clear();
            target.extend_from_slice(bytes);
        } else {
            self.resource_forks.insert(key, bytes.to_vec());
        }
    }

    pub(crate) fn fork(&self, path: &str) -> Option<&Vec<u8>> {
        self.resource_forks.get(path)
    }

    fn merge_from(&mut self, source: &mut Self) {
        for record in source.records.drain(..) {
            if self
                .records
                .iter()
                .any(|existing| existing.path.eq_ignore_ascii_case(&record.path))
            {
                continue;
            }
            self.push(record);
        }
        let forks = source.resource_forks.drain().collect::<Vec<_>>();
        for (path, bytes) in forks {
            if self
                .resource_forks
                .keys()
                .any(|existing| existing.eq_ignore_ascii_case(&path))
            {
                continue;
            }
            self.resource_forks.insert_shared(path, &bytes);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessVfsResourceRecord {
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

/// Guest-memory view of one open classic resource file.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProcessResourceFileMap {
    pub(crate) loaded: HashMap<([u8; 4], i16), u32>,
    pub(crate) named: HashMap<([u8; 4], String), (i16, u32)>,
    pub(crate) names_by_id: HashMap<([u8; 4], i16), String>,
    pub(crate) attrs: HashMap<([u8; 4], i16), u8>,
    pub(crate) map_attrs: u16,
}

/// Open classic resource-file chain for one process.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProcessLoadedResources {
    pub(crate) files: HashMap<u16, ProcessResourceFileMap>,
    pub(crate) names: HashMap<u16, String>,
    pub(crate) search_order: Vec<u16>,
    pub(crate) current_file: u16,
}

/// Process-owned Resource Manager bookkeeping used by CPU adapters.
#[derive(Clone, Debug, Default)]
pub struct ProcessResourceManagerState {
    /// Current resource file for the process, shared by every CPU adapter.
    /// `ProcessLoadedResources::current_file` remains the classic file-chain
    /// cursor, while this is the architecture-neutral `CurResFile` value.
    pub(crate) current_resource_file: SharedProcessValue<i16>,
    /// Resource loading and purge policy shared by every CPU gateway.
    pub(crate) policy: SharedProcessValue<ProcessResourcePolicyState>,
    pub(crate) loaded_handles: HashMap<u32, (u32, [u8; 4], i16)>,
    pub(crate) resource_handles_by_key: HashMap<(u16, [u8; 4], i16), u32>,
    pub(crate) detached_handles: HashMap<u32, ([u8; 4], i16)>,
    pub(crate) resource_handle_files: HashMap<u32, u16>,
    pub(crate) detached_handle_files: HashMap<u32, u16>,
    pub(crate) resources: Option<ProcessLoadedResources>,
    pub(crate) resource_file_order: HashMap<u16, Vec<([u8; 4], i16)>>,
    pub(crate) resource_backing_data: HashMap<(u16, [u8; 4], i16), Vec<u8>>,
    pub(crate) resident_resources: HashSet<(u16, [u8; 4], i16)>,
    pub(crate) resource_files: Vec<ProcessResourceFileRecord>,
    pub(crate) vfs_resource_files: ProcessVfsResourceFileRecords,
    pub(crate) vfs_resources: Vec<ProcessVfsResourceRecord>,
}

/// Process-wide Resource Manager switches that are not represented in a
/// resource map. Inside Macintosh Volume I (1985), pp. I-118 and I-126.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProcessResourcePolicyState {
    pub(crate) res_load: bool,
    pub(crate) res_purge: bool,
}

impl Default for ProcessResourcePolicyState {
    fn default() -> Self {
        Self {
            res_load: true,
            res_purge: false,
        }
    }
}

fn process_resource_manager_runtime_is_empty(manager: &ProcessResourceManagerState) -> bool {
    manager.loaded_handles.is_empty()
        && manager.resource_handles_by_key.is_empty()
        && manager.detached_handles.is_empty()
        && manager.resource_handle_files.is_empty()
        && manager.detached_handle_files.is_empty()
        && manager.resources.is_none()
        && manager.resource_file_order.is_empty()
        && manager.resource_backing_data.is_empty()
        && manager.resident_resources.is_empty()
        && manager.resource_files.is_empty()
}

impl ProcessResourceManagerState {
    fn publish_classic_current_file(&mut self) {
        if *self.current_resource_file != 0 {
            return;
        }
        let classic_selection = self
            .resources
            .as_ref()
            .map_or(0, |resources| resources.current_file as i16);
        if classic_selection != 0 {
            *self.current_resource_file = classic_selection;
        }
    }

    fn merge_from(&mut self, source: &mut Self) {
        let source_runtime_is_empty = process_resource_manager_runtime_is_empty(source);
        let target_runtime_is_empty = process_resource_manager_runtime_is_empty(self);
        assert!(
            source_runtime_is_empty || target_runtime_is_empty,
            "cannot attach two active process Resource Managers"
        );
        self.publish_classic_current_file();
        source.publish_classic_current_file();
        source
            .current_resource_file
            .attach_copy_to(&self.current_resource_file, |refnum| *refnum == 0);
        source.policy.attach_copy_to(&self.policy, |policy| {
            *policy == ProcessResourcePolicyState::default()
        });

        self.vfs_resource_files
            .merge_from(&mut source.vfs_resource_files);
        for resource in source.vfs_resources.drain(..) {
            if self.vfs_resources.iter().any(|existing| {
                existing.path.eq_ignore_ascii_case(&resource.path)
                    && existing.res_type == resource.res_type
                    && existing.res_id == resource.res_id
            }) {
                continue;
            }
            self.vfs_resources.push(resource);
        }

        if target_runtime_is_empty && !source_runtime_is_empty {
            self.loaded_handles = std::mem::take(&mut source.loaded_handles);
            self.resource_handles_by_key = std::mem::take(&mut source.resource_handles_by_key);
            self.detached_handles = std::mem::take(&mut source.detached_handles);
            self.resource_handle_files = std::mem::take(&mut source.resource_handle_files);
            self.detached_handle_files = std::mem::take(&mut source.detached_handle_files);
            self.resources = std::mem::take(&mut source.resources);
            self.resource_file_order = std::mem::take(&mut source.resource_file_order);
            self.resource_backing_data = std::mem::take(&mut source.resource_backing_data);
            self.resident_resources = std::mem::take(&mut source.resident_resources);
            self.resource_files = std::mem::take(&mut source.resource_files);
        }
    }
}

/// Canonical File Manager and Resource Manager storage for one process.
///
/// These managers belong to the process, not to the currently executing
/// instruction set. Keeping their records behind one ownership handle lets
/// native and classic adapters converge on the same mutations during nested
/// Mixed Mode calls. Inside Macintosh: Files (1992), pp. 1-7--1-9; Inside
/// Macintosh Volume I (1985), pp. I-109--I-110.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingFileCompletion {
    pub(crate) parameter_block: u32,
    pub(crate) completion_addr: u32,
    pub(crate) result: i16,
}

/// One open File Manager working directory for the process.
///
/// Working-directory reference numbers are process-wide access paths, not
/// CPU-adapter state. Inside Macintosh: Files (1992), pp. 2-173--2-183.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProcessWorkingDirectory {
    pub(crate) ref_num: i16,
    pub(crate) volume_ref_num: i16,
    pub(crate) dir_id: u32,
    pub(crate) proc_id: u32,
}

#[derive(Debug, Clone)]
pub struct ProcessFileSystemState {
    pub(crate) files: SharedProcessOpenFiles,
    /// Access paths granted write permission, shared by both CPU adapters.
    /// Inside Macintosh: Files (1992), pp. 2-7--2-8.
    pub(crate) writable_refnums: SharedProcessValue<HashSet<u16>>,
    /// Completed asynchronous requests awaiting `ioResult` publication and
    /// optional completion-procedure delivery. Inside Macintosh: Files
    /// (1992), p. 2-238.
    pub(crate) pending_completions: SharedProcessValue<VecDeque<PendingFileCompletion>>,
    pub(crate) working_directories:
        SharedProcessValue<HashMap<i16, ProcessWorkingDirectory>>,
    pub(crate) next_working_directory_ref_num: SharedProcessValue<i16>,
    pub(crate) application_working_directory_ref_num: SharedProcessValue<i16>,
    pub(crate) stdio_streams: HashMap<u32, ProcessStdioStreamRecord>,
    pub(crate) vfs_volumes: SharedProcessValue<Vec<ProcessVfsVolumeRecord>>,
    pub(crate) next_vfs_volume_ref_num: SharedProcessValue<i16>,
    pub(crate) vfs_directories: SharedProcessValue<Vec<ProcessVfsDirectory>>,
    pub(crate) next_vfs_dir_id: SharedProcessValue<u32>,
    pub(crate) default_dir_id: SharedProcessValue<u32>,
    pub(crate) classic_vfs_metadata: SharedProcessValue<HashMap<String, ProcessVfsMetadata>>,
    pub(crate) classic_locked_files: SharedProcessValue<HashSet<String>>,
    pub(crate) classic_next_vfs_file_id: SharedProcessValue<u32>,
    pub(crate) classic_next_vfs_timestamp: SharedProcessValue<u32>,
    pub(crate) vfs_files: ProcessVfsFileRecords,
    pub(crate) deleted_vfs_file_paths: Vec<String>,
    /// Normalized VFS path of the launched application, shared by every CPU
    /// adapter in the process. Inside Macintosh Volume II (1985), pp. II-57--
    /// II-58.
    pub(crate) launched_app_path: Option<String>,
    pub(crate) resource_manager: SharedProcessResourceManager,
    pub(crate) next_file_ref_num: i16,
}

impl Default for ProcessFileSystemState {
    fn default() -> Self {
        Self {
            files: SharedProcessOpenFiles::default(),
            writable_refnums: SharedProcessValue::default(),
            pending_completions: SharedProcessValue::default(),
            working_directories: SharedProcessValue::default(),
            next_working_directory_ref_num: SharedProcessValue::from_value(32),
            application_working_directory_ref_num: SharedProcessValue::from_value(-1),
            stdio_streams: HashMap::new(),
            vfs_volumes: SharedProcessValue::default(),
            next_vfs_volume_ref_num: SharedProcessValue::from_value(-2),
            vfs_directories: SharedProcessValue::default(),
            next_vfs_dir_id: SharedProcessValue::from_value(0),
            default_dir_id: SharedProcessValue::from_value(0),
            classic_vfs_metadata: SharedProcessValue::default(),
            classic_locked_files: SharedProcessValue::default(),
            classic_next_vfs_file_id: SharedProcessValue::from_value(32),
            classic_next_vfs_timestamp: SharedProcessValue::from_value(1),
            vfs_files: ProcessVfsFileRecords::default(),
            deleted_vfs_file_paths: Vec::new(),
            launched_app_path: None,
            resource_manager: SharedProcessResourceManager::default(),
            next_file_ref_num: 128,
        }
    }
}

impl ProcessFileSystemState {
    fn merge_from(&mut self, source: &mut Self) {
        assert!(
            self.files.is_empty() || source.files.is_empty(),
            "cannot attach two active native File Managers"
        );
        if self.files.is_empty() {
            self.files = std::mem::take(&mut source.files);
        }
        match (&self.launched_app_path, &source.launched_app_path) {
            (Some(target), Some(source)) => assert!(
                target.eq_ignore_ascii_case(source),
                "cannot attach two different launched application paths"
            ),
            (None, Some(_)) => {
                self.launched_app_path = source.launched_app_path.take();
            }
            _ => {}
        }
        if !Rc::ptr_eq(&self.writable_refnums.0, &source.writable_refnums.0) {
            self.writable_refnums
                .extend(std::mem::take(&mut *source.writable_refnums));
        }
        if !Rc::ptr_eq(&self.pending_completions.0, &source.pending_completions.0) {
            self.pending_completions
                .extend(std::mem::take(&mut *source.pending_completions));
        }
        if !Rc::ptr_eq(&self.working_directories.0, &source.working_directories.0) {
            assert!(
                self.working_directories.is_empty()
                    || source.working_directories.is_empty()
                    || *self.working_directories == *source.working_directories,
                "cannot attach two different working-directory registries"
            );
            if self.working_directories.is_empty() {
                *self.working_directories = std::mem::take(&mut *source.working_directories);
            }
        }
        *self.next_working_directory_ref_num = (*self.next_working_directory_ref_num)
            .max(*source.next_working_directory_ref_num);
        if *self.application_working_directory_ref_num == -1 {
            *self.application_working_directory_ref_num =
                *source.application_working_directory_ref_num;
        }
        for (stream, record) in std::mem::take(&mut source.stdio_streams) {
            self.stdio_streams.entry(stream).or_insert(record);
        }

        let target_catalogue_was_pristine =
            process_native_vfs_catalogue_is_pristine(&self.vfs_volumes, &self.vfs_directories);
        if !Rc::ptr_eq(&self.vfs_volumes.0, &source.vfs_volumes.0) {
            for volume in source.vfs_volumes.drain(..) {
                if self.vfs_volumes.iter().any(|existing| {
                    existing.ref_num == volume.ref_num
                        || existing.name.eq_ignore_ascii_case(&volume.name)
                }) {
                    continue;
                }
                self.vfs_volumes.push(volume);
            }
        }
        *self.next_vfs_volume_ref_num =
            (*self.next_vfs_volume_ref_num).min(*source.next_vfs_volume_ref_num);
        if !Rc::ptr_eq(&self.vfs_directories.0, &source.vfs_directories.0) {
            for directory in source.vfs_directories.drain(..) {
                if self.vfs_directories.iter().any(|existing| {
                    existing.dir_id == directory.dir_id
                        || existing.path.eq_ignore_ascii_case(&directory.path)
                }) {
                    continue;
                }
                self.vfs_directories.push(directory);
            }
        }
        *self.next_vfs_dir_id = (*self.next_vfs_dir_id).max(*source.next_vfs_dir_id);
        if *self.default_dir_id == 0
            || (target_catalogue_was_pristine && *source.default_dir_id != 0)
        {
            *self.default_dir_id = *source.default_dir_id;
        }

        self.vfs_files.merge_from(&mut source.vfs_files);
        for path in source.deleted_vfs_file_paths.drain(..) {
            if !self
                .deleted_vfs_file_paths
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&path))
            {
                self.deleted_vfs_file_paths.push(path);
            }
        }
        self.next_file_ref_num = self.next_file_ref_num.max(source.next_file_ref_num);

        if !Rc::ptr_eq(&self.classic_vfs_metadata.0, &source.classic_vfs_metadata.0) {
            for (path, metadata) in std::mem::take(&mut *source.classic_vfs_metadata) {
                self.classic_vfs_metadata.entry(path).or_insert(metadata);
            }
        }
        if !Rc::ptr_eq(&self.classic_locked_files.0, &source.classic_locked_files.0) {
            self.classic_locked_files
                .extend(std::mem::take(&mut *source.classic_locked_files));
        }
        *self.classic_next_vfs_file_id =
            (*self.classic_next_vfs_file_id).max(*source.classic_next_vfs_file_id);
        *self.classic_next_vfs_timestamp =
            (*self.classic_next_vfs_timestamp).max(*source.classic_next_vfs_timestamp);
        source
            .resource_manager
            .attach_resource_manager_to(&self.resource_manager);
    }

    fn detached_vfs_snapshot(&self) -> Self {
        let mut snapshot = Self::default();
        snapshot.vfs_volumes = self.vfs_volumes.clone();
        snapshot.next_vfs_volume_ref_num = self.next_vfs_volume_ref_num.clone();
        snapshot.vfs_directories = self.vfs_directories.clone();
        snapshot.next_vfs_dir_id = self.next_vfs_dir_id.clone();
        snapshot.default_dir_id = self.default_dir_id.clone();
        snapshot.classic_vfs_metadata = self.classic_vfs_metadata.clone();
        snapshot.classic_locked_files = self.classic_locked_files.clone();
        snapshot.classic_next_vfs_file_id = self.classic_next_vfs_file_id.clone();
        snapshot.classic_next_vfs_timestamp = self.classic_next_vfs_timestamp.clone();
        snapshot.vfs_files = self.vfs_files.clone();
        snapshot.launched_app_path = self.launched_app_path.clone();
        snapshot.resource_manager.vfs_resource_files = self.vfs_resource_files.clone();
        snapshot
    }

    #[cfg(test)]
    pub(crate) fn with_resources(
        mut self,
        resource_files: Vec<ProcessResourceFileRecord>,
        vfs_resource_files: Vec<ProcessVfsResourceFileRecord>,
        vfs_resources: Vec<ProcessVfsResourceRecord>,
    ) -> Self {
        self.resource_files = resource_files;
        self.vfs_resource_files.replace(vfs_resource_files);
        self.vfs_resources = vfs_resources;
        self
    }

    /// Mirror native file records into the classic path indexes.
    ///
    /// Directory and volume records are canonical process-owned records and
    /// therefore do not participate in this compatibility pass. Files (1992),
    /// pp. 2-27--2-29 and 2-85.
    pub(crate) fn publish_native_vfs_catalogue(&mut self) {
        let directories = (*self.vfs_directories).clone();
        let files = self.vfs_files.iter().cloned().collect::<Vec<_>>();
        let resource_files = self.vfs_resource_files.iter().cloned().collect::<Vec<_>>();
        let deleted_paths = self.deleted_vfs_file_paths.clone();

        for file in files {
            if file.path.is_empty() {
                continue;
            }
            let parent_dir_id = process_vfs_parent_dir_id(&directories, &file.path);
            publish_native_vfs_metadata(
                &mut self.classic_vfs_metadata,
                &mut self.classic_next_vfs_file_id,
                &mut self.classic_next_vfs_timestamp,
                &file.path,
                parent_dir_id,
                file.file_type,
                file.creator,
                file.finder_flags,
                file.dirty,
            );
        }
        for file in resource_files {
            if file.path.is_empty() {
                continue;
            }
            let parent_dir_id = process_vfs_parent_dir_id(&directories, &file.path);
            publish_native_vfs_metadata(
                &mut self.classic_vfs_metadata,
                &mut self.classic_next_vfs_file_id,
                &mut self.classic_next_vfs_timestamp,
                &file.path,
                parent_dir_id,
                file.file_type,
                file.creator,
                file.finder_flags,
                file.dirty,
            );
        }
        for path in deleted_paths {
            self.vfs_files.data_forks.remove(&path);
            self.vfs_resource_files.resource_forks.remove(&path);
            self.vfs_files
                .records
                .retain(|file| !file.path.eq_ignore_ascii_case(&path));
            self.vfs_resource_files
                .records
                .retain(|file| !file.path.eq_ignore_ascii_case(&path));
            self.classic_vfs_metadata.remove(&path);
            self.classic_locked_files.remove(&path);
        }
    }

    pub(crate) fn publish_classic_vfs_metadata(&mut self, path: &str) {
        let Some(metadata) = self.classic_vfs_metadata.get(path).copied() else {
            return;
        };
        if self
            .vfs_directories
            .iter()
            .any(|directory| directory.path.eq_ignore_ascii_case(path))
        {
            return;
        }
        if let Some(data) = self.vfs_files.data_forks.get_shared(path) {
            let data = data.shared_handle();
            if let Some(file) = self
                .vfs_files
                .iter_mut()
                .find(|file| file.path.eq_ignore_ascii_case(path))
            {
                file.creator = metadata.creator;
                file.file_type = metadata.file_type;
                file.finder_flags = metadata.finder_flags;
            } else {
                self.vfs_files.push(ProcessVfsFileRecord {
                    path: path.to_string(),
                    data,
                    creator: metadata.creator,
                    file_type: metadata.file_type,
                    finder_flags: metadata.finder_flags,
                    dirty: false,
                });
            }
        }
        if let Some(data) = self.vfs_resource_files.resource_forks.get_shared(path) {
            let data = data.shared_handle();
            if let Some(file) = self
                .vfs_resource_files
                .iter_mut()
                .find(|file| file.path.eq_ignore_ascii_case(path))
            {
                file.creator = metadata.creator;
                file.file_type = metadata.file_type;
                file.finder_flags = metadata.finder_flags;
            } else {
                self.vfs_resource_files.push(ProcessVfsResourceFileRecord {
                    path: path.to_string(),
                    creator: metadata.creator,
                    file_type: metadata.file_type,
                    finder_flags: metadata.finder_flags,
                    resource_len: data.len() as u32,
                    raw_data: Some(data),
                    map_attrs: 0,
                    dirty: false,
                });
            }
        }
    }

    pub(crate) fn remove_classic_vfs_path(&mut self, path: &str) {
        let prefix = format!("{path}/");
        self.vfs_files.retain(|file| {
            !file.path.eq_ignore_ascii_case(path)
                && !file
                    .path
                    .to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
        });
        self.vfs_resource_files.retain(|file| {
            !file.path.eq_ignore_ascii_case(path)
                && !file
                    .path
                    .to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
        });
        self.vfs_directories.retain(|directory| {
            !directory.path.eq_ignore_ascii_case(path)
                && !directory
                    .path
                    .to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
        });
    }
}

fn process_vfs_parent_dir_id(directories: &[ProcessVfsDirectory], path: &str) -> u32 {
    let parent_path = path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("");
    directories
        .iter()
        .find(|directory| directory.path.eq_ignore_ascii_case(parent_path))
        .map(|directory| directory.dir_id)
        .unwrap_or(2)
}

#[allow(clippy::too_many_arguments)]
fn publish_native_vfs_metadata(
    metadata: &mut HashMap<String, ProcessVfsMetadata>,
    next_file_id: &mut u32,
    next_timestamp: &mut u32,
    path: &str,
    parent_dir_id: u32,
    file_type: u32,
    creator: u32,
    finder_flags: u16,
    touch: bool,
) {
    let timestamp = *next_timestamp;
    let entry = metadata.entry(path.to_string()).or_insert_with(|| {
        let file_id = *next_file_id;
        *next_file_id = next_file_id.saturating_add(1);
        *next_timestamp = next_timestamp.saturating_add(1);
        ProcessVfsMetadata {
            file_id,
            parent_dir_id,
            file_type,
            creator,
            finder_flags,
            created_date: timestamp,
            modified_date: timestamp,
        }
    });
    entry.parent_dir_id = parent_dir_id;
    entry.file_type = file_type;
    entry.creator = creator;
    entry.finder_flags = finder_flags;
    if touch {
        entry.modified_date = *next_timestamp;
        *next_timestamp = next_timestamp.saturating_add(1);
    }
}

impl std::ops::Deref for ProcessFileSystemState {
    type Target = ProcessResourceManagerState;

    fn deref(&self) -> &Self::Target {
        &self.resource_manager
    }
}

impl std::ops::DerefMut for ProcessFileSystemState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.resource_manager
    }
}

/// Shared attachment handle for process-owned file and resource managers.
///
/// A normal clone is deliberately detached so cloning a loaded application
/// cannot couple two processes. `attach_to` is the only operation that shares
/// state, and the runner serializes every attached adapter access.
#[derive(Debug)]
pub(crate) struct SharedProcessFileSystem(Rc<UnsafeCell<ProcessFileSystemState>>);

/// Detached-by-default shared storage for one process manager collection.
///
/// Ordinary clones are snapshots so cloning a dispatcher cannot couple two
/// processes. Adapters share only through `attach_to`, under the same
/// serialized runner ownership used for guest RAM and the Memory Manager.
#[derive(Debug)]
pub struct SharedProcessValue<T>(Rc<UnsafeCell<T>>);

pub(crate) type SharedProcessResourceManager = SharedProcessValue<ProcessResourceManagerState>;
pub(crate) type SharedProcessSoundManager = SharedProcessValue<SoundManager>;
pub(crate) type SharedProcessCursorState = SharedProcessValue<ProcessCursorState>;
pub(crate) type SharedProcessEventQueue = SharedProcessValue<EventQueue>;
pub(crate) type SharedProcessMenuTracking = SharedProcessValue<Option<ProcessMenuTrackingState>>;
pub(crate) type SharedProcessWindowList = SharedProcessValue<Vec<u32>>;
pub(crate) type SharedProcessInputState = SharedProcessValue<ProcessInputState>;
pub(crate) type SharedProcessTimerTasks = SharedProcessValue<Vec<ProcessTimerTask>>;
pub(crate) type SharedProcessVblTasks = SharedProcessValue<Vec<ProcessVblTask>>;
pub(crate) type SharedProcessCallbackScheduling = SharedProcessValue<ProcessCallbackScheduling>;
pub(crate) type SharedProcessMixedModeM68kState = SharedProcessValue<ProcessMixedModeM68kState>;
pub(crate) type SharedProcessScrapState = SharedProcessValue<ProcessScrapState>;
pub(crate) type SharedProcessControlManager = SharedProcessValue<ProcessControlManagerState>;
pub(crate) type SharedProcessListManager = SharedProcessValue<ProcessListManagerState>;
pub(crate) type SharedProcessTextEditManager = SharedProcessValue<ProcessTextEditManagerState>;
/// Detached-by-default attachment handle for Dialog Manager `ParamText` slots.
pub type SharedProcessDialogText = SharedProcessValue<[Vec<u8>; 4]>;

/// Canonical per-color-port arithmetic transfer colors. The guest-visible
/// `CGrafPort.grafVars` record is the primary representation; this process
/// index keeps the value live across adapters when a port is one of the
/// static records that cannot own an allocator-managed GrafVars handle.
/// Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64.
pub(crate) type SharedProcessQuickDrawOpColors =
    SharedProcessValue<HashMap<u32, (u16, u16, u16)>>;

/// Default HiliteRGB used when a color graphics port has not received a
/// HiliteColor call. The selected-list green matches the System 7.5.3
/// BasiliskII reference used by the QuickDraw HLE.
/// Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64.
pub(crate) const DEFAULT_QUICKDRAW_HILITE_COLOR: (u16, u16, u16) = (0x0000, 0x8000, 0x0000);

/// Canonical per-color-port highlight colors. The guest-visible
/// `CGrafPort.grafVars.rgbHiliteColor` record remains authoritative when it
/// exists; this process index covers static ports without owned GrafVars.
/// Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64.
pub(crate) type SharedProcessQuickDrawHiliteColors =
    SharedProcessValue<HashMap<u32, (u16, u16, u16)>>;

/// Canonical desktop scrap for one Macintosh process.
///
/// The Scrap Manager exposes one ordered collection of typed flavors to every
/// execution architecture in the process. TextEdit's private scrap remains a
/// separate adapter detail. Inside Macintosh Volume I (1985), pp. I-453 and
/// I-457--I-459.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessScrapState {
    pub(crate) entries: Vec<([u8; 4], Vec<u8>)>,
    pub(crate) count: i16,
    pub(crate) initialized: bool,
    pub(crate) in_memory: bool,
    pub(crate) clipboard_writable: bool,
    pub(crate) handle: Option<u32>,
    pub(crate) handle_dirty: bool,
    pub(crate) stuff_ptr: Option<u32>,
}

impl Default for ProcessScrapState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            count: 0,
            initialized: false,
            in_memory: true,
            clipboard_writable: false,
            handle: None,
            handle_dirty: false,
            stuff_ptr: None,
        }
    }
}

impl ProcessScrapState {
    fn is_pristine(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProcessKeyRepeatState {
    pub(crate) key_code: u8,
    pub(crate) char_code: u8,
    pub(crate) next_tick: u32,
}

/// Process-owned storage for the generated 68K gateway and compatibility
/// stack used by native Mixed Mode callbacks. Both CPU adapters use one
/// logical bridge for a process. Inside Macintosh: PowerPC System Software
/// (1994), pp. 2-12--2-20.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessMixedModeM68kState {
    pub(crate) gateway: u32,
    pub(crate) stack_top: u32,
}

impl ProcessMixedModeM68kState {
    pub(crate) fn is_pristine(&self) -> bool {
        *self == Self::default()
    }
}

/// Canonical mouse and keyboard device state for one Macintosh process.
///
/// Event Manager calls, direct low-memory polling, and either ISA observe the
/// same mouse position, button state, and 128-key map. Inside Macintosh Volume
/// I (1985), pp. I-259--I-263 and I-273--I-275.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessInputState {
    pub(crate) mouse_pos: (i16, i16),
    pub(crate) mouse_button: bool,
    pub(crate) key_map: [u8; 16],
    pub(crate) caps_lock_physically_pressed: bool,
    pub(crate) key_repeat: Option<ProcessKeyRepeatState>,
}

impl ProcessInputState {
    pub(crate) fn is_pristine(&self) -> bool {
        self == &Self::default()
    }
}

/// Canonical QuickDraw cursor state for one Macintosh process.
///
/// InitCursor, SetCursor, HideCursor, and ShowCursor operate on one signed
/// visibility level and one installed image regardless of the executing ISA.
/// Inside Macintosh Volume I (1985), pp. I-167--I-168.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessCursorState {
    pub(crate) image: Option<CursorImage>,
    pub(crate) level: i16,
}

impl Default for ProcessCursorState {
    fn default() -> Self {
        Self {
            image: Some(default_arrow_cursor_image()),
            level: 0,
        }
    }
}

impl ProcessCursorState {
    pub(crate) fn is_pristine(&self) -> bool {
        self == &Self::default()
    }

    pub(crate) fn visible(&self) -> bool {
        self.level == 0
    }

    pub(crate) fn init(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn install(&mut self, image: CursorImage) {
        self.image = Some(image);
    }

    pub(crate) fn hide(&mut self) {
        self.level = self.level.saturating_sub(1);
    }

    pub(crate) fn show(&mut self) {
        if self.level < 0 {
            self.level += 1;
        }
    }
}

impl<T: Default> Default for SharedProcessValue<T> {
    fn default() -> Self {
        Self(Rc::new(UnsafeCell::new(T::default())))
    }
}

impl<T: Clone> Clone for SharedProcessValue<T> {
    fn clone(&self) -> Self {
        Self(Rc::new(UnsafeCell::new((**self).clone())))
    }
}

impl<T: PartialEq> PartialEq for SharedProcessValue<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: PartialEq> PartialEq<T> for SharedProcessValue<T> {
    fn eq(&self, other: &T) -> bool {
        **self == *other
    }
}

impl<T: Eq> Eq for SharedProcessValue<T> {}

#[cfg(test)]
impl<T: Clone> SharedProcessValue<T> {
    /// Copy a detached process snapshot for value-oriented assertions.
    pub(crate) fn snapshot(&self) -> T {
        (**self).clone()
    }
}

impl<T> std::ops::Deref for SharedProcessValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: attachment and access are serialized by the owning runner;
        // normal clones allocate detached snapshots.
        unsafe { &*self.0.get() }
    }
}

impl<T> std::ops::DerefMut for SharedProcessValue<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: see `Deref`.
        unsafe { &mut *self.0.get() }
    }
}

impl<T> SharedProcessValue<T> {
    pub(crate) fn from_value(value: T) -> Self {
        Self(Rc::new(UnsafeCell::new(value)))
    }

    pub(crate) fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    #[cfg(test)]
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl SharedProcessValue<HashMap<u32, (u16, u16, u16)>> {
    /// Read one Color QuickDraw operation color without exposing a reference
    /// into the process-owned map to adapter code. This keeps any mutable
    /// borrow inside the individual operation, so a Mixed Mode callback can
    /// safely enter the other attached adapter afterwards.
    /// Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64.
    pub(crate) fn quickdraw_op_color(&self, port: u32) -> Option<(u16, u16, u16)> {
        // SAFETY: process adapters are serialized by the runner. The map is
        // accessed only for the duration of this operation; no reference is
        // returned to the UnsafeCell-backed value.
        unsafe { (&*self.0.get()).get(&port).copied() }
    }

    /// Update one process-owned Color QuickDraw operation color while keeping
    /// the UnsafeCell borrow scoped to this statement.
    /// Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64.
    pub(crate) fn set_quickdraw_op_color(&self, port: u32, color: (u16, u16, u16)) {
        // SAFETY: see `quickdraw_op_color`.
        unsafe {
            (&mut *self.0.get()).insert(port, color);
        }
    }

    /// Drop a disposed port's fallback operation color.
    pub(crate) fn remove_quickdraw_op_color(&self, port: u32) {
        // SAFETY: see `quickdraw_op_color`.
        unsafe {
            (&mut *self.0.get()).remove(&port);
        }
    }

    /// Read one Color QuickDraw highlight color without exposing a reference
    /// into the process-owned map to adapter code.
    /// Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64.
    pub(crate) fn quickdraw_hilite_color(&self, port: u32) -> Option<(u16, u16, u16)> {
        // SAFETY: process adapters are serialized by the runner. The map is
        // accessed only for the duration of this operation; no reference is
        // returned to the UnsafeCell-backed value.
        unsafe { (&*self.0.get()).get(&port).copied() }
    }

    /// Update one process-owned Color QuickDraw highlight color.
    /// Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64.
    pub(crate) fn set_quickdraw_hilite_color(&self, port: u32, color: (u16, u16, u16)) {
        // SAFETY: see `quickdraw_hilite_color`.
        unsafe {
            (&mut *self.0.get()).insert(port, color);
        }
    }

    /// Drop a disposed port's fallback highlight color.
    pub(crate) fn remove_quickdraw_hilite_color(&self, port: u32) {
        // SAFETY: see `quickdraw_hilite_color`.
        unsafe {
            (&mut *self.0.get()).remove(&port);
        }
    }
}

impl<T: Default> SharedProcessValue<T> {
    pub(crate) fn attach_to(&mut self, process_value: &Self, is_empty: impl Fn(&T) -> bool) {
        if Rc::ptr_eq(&self.0, &process_value.0) {
            return;
        }
        assert!(
            is_empty(self) || is_empty(process_value),
            "cannot attach two populated process manager collections"
        );
        if is_empty(process_value) {
            // SAFETY: attachment occurs before the adapter is exposed through
            // the runner, so no references into either value exist.
            unsafe {
                *process_value.0.get() = std::mem::take(&mut **self);
            }
        }
        self.0 = Rc::clone(&process_value.0);
    }
}

impl<T: Copy + PartialEq> SharedProcessValue<T> {
    fn attach_copy_to(&mut self, process_value: &Self, is_pristine: impl Fn(&T) -> bool) {
        if Rc::ptr_eq(&self.0, &process_value.0) {
            return;
        }
        assert!(
            is_pristine(self) || is_pristine(process_value) || **self == **process_value,
            "cannot attach two populated process manager values"
        );
        if is_pristine(process_value) && !is_pristine(self) {
            // SAFETY: attachment occurs before either adapter is exposed.
            unsafe {
                *process_value.0.get() = **self;
            }
        }
        self.0 = Rc::clone(&process_value.0);
    }

    fn activate_copy_to(&mut self, process_value: &Self) {
        if Rc::ptr_eq(&self.0, &process_value.0) {
            return;
        }
        // SAFETY: application activation occurs while the runner exclusively
        // owns both adapters and before guest execution resumes.
        unsafe {
            *process_value.0.get() = **self;
        }
        self.0 = Rc::clone(&process_value.0);
    }
}

impl SharedProcessValue<ProcessResourceManagerState> {
    fn attach_resource_manager_to(&mut self, target: &Self) {
        if Rc::ptr_eq(&self.0, &target.0) {
            return;
        }
        // SAFETY: adapters attach before being exposed through the runner,
        // and the target allocation must stay stable because its nested fork
        // maps may already be shared with the classic dispatcher.
        unsafe {
            (&mut *target.0.get()).merge_from(&mut *self.0.get());
        }
        self.0 = Rc::clone(&target.0);
    }
}

impl Default for SharedProcessFileSystem {
    fn default() -> Self {
        Self(Rc::new(UnsafeCell::new(ProcessFileSystemState::default())))
    }
}

impl Clone for SharedProcessFileSystem {
    fn clone(&self) -> Self {
        Self(Rc::new(UnsafeCell::new((**self).clone())))
    }
}

impl std::ops::Deref for SharedProcessFileSystem {
    type Target = ProcessFileSystemState;

    fn deref(&self) -> &Self::Target {
        // SAFETY: attached CPU adapters are private children of one runner,
        // and every execution entry point requires an exclusive mutable
        // runner borrow. Detached clones receive an independent allocation.
        unsafe { &*self.0.get() }
    }
}

impl std::ops::DerefMut for SharedProcessFileSystem {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: see `Deref`; mutable access is serialized by the runner.
        unsafe { &mut *self.0.get() }
    }
}

impl SharedProcessFileSystem {
    /// Return another handle to this process file system without detaching
    /// its records. Execution adapters use this when a long-running call
    /// needs scoped mutable access to process-owned state while retaining
    /// access to their own adapter fields.
    pub(crate) fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    pub(crate) fn from_state(state: ProcessFileSystemState) -> Self {
        Self(Rc::new(UnsafeCell::new(state)))
    }

    pub(crate) fn detached_vfs_snapshot(&self) -> Self {
        Self::from_state((**self).detached_vfs_snapshot())
    }

    pub(crate) fn attach_to(&mut self, process_file_system: &Self) {
        if Rc::ptr_eq(&self.0, &process_file_system.0) {
            return;
        }
        // SAFETY: adapters attach before being exposed through the runner.
        // The process allocation must remain stable because the classic
        // dispatcher may already share its nested catalogue and fork handles.
        unsafe {
            (&mut *process_file_system.0.get()).merge_from(&mut *self.0.get());
        }
        self.0 = Rc::clone(&process_file_system.0);
    }

    #[cfg(test)]
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessHandleRecord {
    pub handle: u32,
    pub ptr: u32,
    pub size: u32,
    pub capacity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessPtrRecord {
    pub ptr: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessHandleStateRecord {
    pub handle: u32,
    pub locked: bool,
    pub high_locked: bool,
    pub no_purge: bool,
    pub resource: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProcessAppleEventHandler {
    pub(crate) procedure: GuestProcedure,
    pub(crate) refcon: u32,
}

/// One process's application and system AppleEvent dispatch tables.
///
/// Get and remove use the exact key supplied by the caller, while dispatch
/// searches the application table before the system table and considers the
/// exact, class-wildcard, ID-wildcard, and double-wildcard keys in that order.
/// Inside Macintosh: Interapplication Communication (1993), pp. 4-62--4-68.
#[derive(Debug, Default)]
pub(crate) struct SharedProcessAppleEventHandlers(
    Rc<RefCell<HashMap<(bool, u32, u32), ProcessAppleEventHandler>>>,
);

impl Clone for SharedProcessAppleEventHandlers {
    fn clone(&self) -> Self {
        Self(Rc::new(RefCell::new(self.0.borrow().clone())))
    }
}

impl PartialEq for SharedProcessAppleEventHandlers {
    fn eq(&self, other: &Self) -> bool {
        *self.0.borrow() == *other.0.borrow()
    }
}

impl Eq for SharedProcessAppleEventHandlers {}

impl SharedProcessAppleEventHandlers {
    pub(crate) fn attach_to(&mut self, process_handlers: &Self) {
        if Rc::ptr_eq(&self.0, &process_handlers.0) {
            return;
        }
        assert!(
            self.0.borrow().is_empty() || process_handlers.0.borrow().is_empty(),
            "cannot attach two populated AppleEvent dispatch tables"
        );
        let handlers = std::mem::take(&mut *self.0.borrow_mut());
        self.0 = Rc::clone(&process_handlers.0);
        self.0.borrow_mut().extend(handlers);
    }

    pub(crate) fn install(
        &self,
        is_system_handler: bool,
        event_class: u32,
        event_id: u32,
        handler: ProcessAppleEventHandler,
    ) {
        self.0
            .borrow_mut()
            .insert((is_system_handler, event_class, event_id), handler);
    }

    pub(crate) fn get(
        &self,
        is_system_handler: bool,
        event_class: u32,
        event_id: u32,
    ) -> Option<ProcessAppleEventHandler> {
        self.0
            .borrow()
            .get(&(is_system_handler, event_class, event_id))
            .copied()
    }

    pub(crate) fn remove(
        &self,
        is_system_handler: bool,
        event_class: u32,
        event_id: u32,
        procedure: u32,
    ) -> bool {
        let key = (is_system_handler, event_class, event_id);
        let mut handlers = self.0.borrow_mut();
        let matches = handlers.get(&key).is_some_and(|handler| {
            procedure == 0 || handler.procedure.original_pointer == procedure
        });
        if matches {
            handlers.remove(&key);
        }
        matches
    }

    pub(crate) fn handler_for(
        &self,
        event_class: u32,
        event_id: u32,
        wildcard: u32,
    ) -> Option<ProcessAppleEventHandler> {
        let handlers = self.0.borrow();
        for is_system_handler in [false, true] {
            for key in [
                (is_system_handler, event_class, event_id),
                (is_system_handler, event_class, wildcard),
                (is_system_handler, wildcard, event_id),
                (is_system_handler, wildcard, wildcard),
            ] {
                if let Some(handler) = handlers.get(&key) {
                    return Some(*handler);
                }
            }
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.borrow().len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProcessNativeHeapState {
    pub(crate) heap_base: u32,
    pub(crate) heap_cursor: u32,
    pub(crate) heap_limit: u32,
    pub(crate) last_mem_error: i16,
    pub(crate) heap_maximized: bool,
    pub(crate) master_pointer_blocks_requested: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessNativeAllocatorState {
    initial_heap: ProcessNativeHeapState,
    pub(crate) heap: ProcessNativeHeapState,
    pub(crate) ptrs: Vec<ProcessPtrRecord>,
    pub(crate) free_ptr_blocks: Vec<ProcessPtrRecord>,
    pub(crate) free_handle_blocks: Vec<ProcessHandleRecord>,
}

/// Shared process metadata indexed by a guest address.
///
/// CPU adapters retain clones of this handle, not copies of its map, so
/// Memory Manager mutations are visible before an execution slice returns.
#[derive(Debug, Clone)]
pub(crate) struct SharedProcessMap<V>(Rc<RefCell<HashMap<u32, V>>>);

impl<V> Default for SharedProcessMap<V> {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(HashMap::new())))
    }
}

impl<V: Copy> SharedProcessMap<V> {
    pub(crate) fn detached_clone(&self) -> Self {
        Self(Rc::new(RefCell::new(self.0.borrow().clone())))
    }

    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub(crate) fn insert(&self, key: u32, value: V) -> Option<V> {
        self.0.borrow_mut().insert(key, value)
    }

    pub(crate) fn remove(&self, key: &u32) -> Option<V> {
        self.0.borrow_mut().remove(key)
    }

    pub(crate) fn get(&self, key: &u32) -> Option<V> {
        self.0.borrow().get(key).copied()
    }

    #[cfg(test)]
    pub(crate) fn contains_key(&self, key: &u32) -> bool {
        self.0.borrow().contains_key(key)
    }

    pub(crate) fn extend(&self, entries: impl IntoIterator<Item = (u32, V)>) {
        self.0.borrow_mut().extend(entries);
    }

    pub(crate) fn take_entries(&self) -> Vec<(u32, V)> {
        self.0.borrow_mut().drain().collect()
    }

    fn replace_from(&self, source: &Self) {
        *self.0.borrow_mut() = source.0.borrow().clone();
    }

    pub(crate) fn update(&self, key: u32, update: impl FnOnce(Option<V>) -> Option<V>) {
        let mut entries = self.0.borrow_mut();
        let value = update(entries.get(&key).copied());
        if let Some(value) = value {
            entries.insert(key, value);
        } else {
            entries.remove(&key);
        }
    }
}

#[derive(Debug, Clone)]
struct ClassicHeapAllocatorState {
    /// Identity of the process Memory Manager that claimed this allocator.
    owner_id: Option<usize>,
    /// Heap allocation pointer (grows upward from 0x200000).
    heap_ptr: u32,
    /// Free list: maps aligned_size to recycled addresses.
    free_blocks: HashMap<u32, Vec<u32>>,
    /// Logical allocation sizes keyed by guest address.
    alloc_sizes: HashMap<u32, u32>,
    /// Direct-loaded application image spans that heap allocations must skip.
    reserved_heap_ranges: Vec<(u32, u32)>,
    /// Capacity retained when a best-fit recycled block exceeds its request.
    alloc_bucket_sizes: HashMap<u32, u32>,
}

#[derive(Debug, Clone, Copy)]
enum ClassicAllocationPlanSource {
    Free {
        bucket: u32,
        retains_capacity: bool,
    },
    Bump {
        previous_heap_ptr: u32,
        next_heap_ptr: u32,
    },
}

#[derive(Debug, Clone, Copy)]
struct ClassicAllocationPlan {
    address: u32,
    requested_size: u32,
    aligned_size: u32,
    alignment: u32,
    heap_limit: u32,
    source: ClassicAllocationPlanSource,
}

impl ClassicAllocationPlan {
    fn capacity(self) -> u32 {
        match self.source {
            ClassicAllocationPlanSource::Free { bucket, .. } => bucket,
            ClassicAllocationPlanSource::Bump { .. } => self.aligned_size,
        }
    }
}

impl Default for ClassicHeapAllocatorState {
    fn default() -> Self {
        Self {
            owner_id: None,
            heap_ptr: 0x20_0000,
            free_blocks: HashMap::new(),
            alloc_sizes: HashMap::new(),
            reserved_heap_ranges: Vec::new(),
            alloc_bucket_sizes: HashMap::new(),
        }
    }
}

/// Process-owned classic Memory Manager metadata shared by the 68K adapter
/// and native imports. `MacMemoryBus` only retains an adapter view so it can
/// materialize and access the guest bytes at the addresses selected here.
/// Detached process managers clone this state rather than sharing it. Inside
/// Macintosh: Memory (1992), pp. 2-19--2-21, 2-35--2-44.
#[derive(Debug, Clone, Default)]
pub(crate) struct SharedClassicHeapAllocator(Rc<RefCell<ClassicHeapAllocatorState>>);

impl SharedClassicHeapAllocator {
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub(crate) fn detached_clone_for_owner(&self, owner_id: usize) -> Self {
        let mut state = self.0.borrow().clone();
        state.owner_id = Some(owner_id);
        Self(Rc::new(RefCell::new(state)))
    }

    fn assert_can_claim_owner(&self, owner_id: usize) {
        if let Some(existing) = self.0.borrow().owner_id {
            assert_eq!(
                existing, owner_id,
                "cannot attach a classic heap owned by another process"
            );
        }
    }

    pub(crate) fn claim_owner(&self, owner_id: usize) {
        self.assert_can_claim_owner(owner_id);
        let mut state = self.0.borrow_mut();
        if state.owner_id.is_none() {
            state.owner_id = Some(owner_id);
        }
    }

    pub(crate) fn transfer_owner(&self, source_owner_id: usize, target_owner_id: usize) {
        let mut state = self.0.borrow_mut();
        assert_eq!(
            state.owner_id,
            Some(source_owner_id),
            "classic heap owner changed before process-manager handoff"
        );
        state.owner_id = Some(target_owner_id);
    }

    fn assert_owned_by(&self, owner_id: usize) {
        assert_eq!(
            self.0.borrow().owner_id,
            Some(owner_id),
            "classic heap owner changed before process-manager handoff"
        );
    }

    pub(crate) fn is_pristine(&self) -> bool {
        let state = self.0.borrow();
        state.heap_ptr == 0x20_0000
            && state.free_blocks.is_empty()
            && state.alloc_sizes.is_empty()
            && state.reserved_heap_ranges.is_empty()
            && state.alloc_bucket_sizes.is_empty()
    }

    pub(crate) fn replace_pristine_state_from(&self, source: &Self) {
        assert!(
            self.is_pristine(),
            "cannot replace active process-owned classic heap state"
        );
        self.replace_state_from(source);
    }

    pub(crate) fn replace_state_from(&self, source: &Self) {
        let owner_id = self.0.borrow().owner_id;
        let mut replacement = source.0.borrow().clone();
        replacement.owner_id = owner_id;
        *self.0.borrow_mut() = replacement;
    }

    pub(crate) fn allocation_size(&self, address: u32) -> Option<u32> {
        self.0.borrow().alloc_sizes.get(&address).copied()
    }

    pub(crate) fn allocation_capacity(&self, address: u32) -> Option<u32> {
        let state = self.0.borrow();
        let size = state.alloc_sizes.get(&address).copied()?;
        Some(
            state
                .alloc_bucket_sizes
                .get(&address)
                .copied()
                .unwrap_or_else(|| Self::allocation_bucket_size(size)),
        )
    }

    pub(crate) fn allocation_bucket_size(size: u32) -> u32 {
        ((size + 3) & !3).max(4)
    }

    fn checked_allocation_bucket_size(size: u32) -> Option<u32> {
        size.checked_add(3).map(|size| (size & !3).max(4))
    }

    fn can_reuse_bucket_for_request(bucket: u32, requested: u32) -> bool {
        let max_bucket = if requested <= 1024 {
            4096
        } else {
            requested.saturating_mul(2).saturating_add(4096)
        };
        bucket <= max_bucket
    }

    pub(crate) fn reserve_until(&self, end_addr: u32) {
        let aligned = (end_addr + 3) & !3;
        let mut state = self.0.borrow_mut();
        state.heap_ptr = state.heap_ptr.max(aligned);
    }

    pub(crate) fn reserve_range(&self, start_addr: u32, end_addr: u32) {
        let start = start_addr & !3;
        let end = (end_addr.saturating_add(3)) & !3;
        if start >= end {
            return;
        }
        let mut state = self.0.borrow_mut();
        state.reserved_heap_ranges.push((start, end));
        state.reserved_heap_ranges.sort_unstable();
    }

    fn bump_allocation_address(
        state: &ClassicHeapAllocatorState,
        size: u32,
        alignment: u32,
    ) -> Option<(u32, u32)> {
        let mut ptr = state
            .heap_ptr
            .checked_add(alignment - 1)?
            & !(alignment - 1);
        loop {
            let new_ptr = ptr.checked_add(size)?;
            let overlap = state
                .reserved_heap_ranges
                .iter()
                .find(|&&(start, end)| ptr < end && new_ptr > start);
            if let Some(&(_, end)) = overlap {
                ptr = end.checked_add(alignment - 1)? & !(alignment - 1);
                continue;
            }
            return Some((ptr, new_ptr));
        }
    }

    fn range_overlaps_reserved(
        state: &ClassicHeapAllocatorState,
        address: u32,
        len: u32,
    ) -> bool {
        let Some(end) = address.checked_add(len) else {
            return true;
        };
        state
            .reserved_heap_ranges
            .iter()
            .any(|&(start, reserved_end)| address < reserved_end && start < end)
    }

    fn allocation_plan(
        &self,
        size: u32,
        alignment: u32,
        heap_limit: u32,
    ) -> Option<ClassicAllocationPlan> {
        let alignment = if alignment > 4 && alignment.is_power_of_two() {
            alignment
        } else {
            4
        };
        let aligned_size = Self::checked_allocation_bucket_size(size)?;
        let state = self.0.borrow();

        if let Some(blocks) = state.free_blocks.get(&aligned_size) {
            let address = if alignment == 4 {
                blocks
                    .iter()
                    .rev()
                    .copied()
                    .find(|&address| !Self::range_overlaps_reserved(&state, address, aligned_size))
            } else {
                blocks.iter().copied().find(|&address| {
                    address % alignment == 0
                        && !Self::range_overlaps_reserved(&state, address, aligned_size)
                })
            };
            if let Some(address) = address {
                return Some(ClassicAllocationPlan {
                    address,
                    requested_size: size,
                    aligned_size,
                    alignment,
                    heap_limit,
                    source: ClassicAllocationPlanSource::Free {
                        bucket: aligned_size,
                        retains_capacity: false,
                    },
                });
            }
        }

        let best = state
            .free_blocks
            .iter()
            .filter_map(|(&bucket, blocks)| {
                if bucket <= aligned_size
                    || blocks.is_empty()
                    || !Self::can_reuse_bucket_for_request(bucket, aligned_size)
                {
                    return None;
                }
                let address = if alignment == 4 {
                    blocks
                        .iter()
                        .rev()
                        .copied()
                        .find(|&address| !Self::range_overlaps_reserved(&state, address, bucket))
                } else {
                    blocks.iter().copied().find(|&address| {
                        address % alignment == 0
                            && !Self::range_overlaps_reserved(&state, address, bucket)
                    })
                }?;
                Some((bucket, address))
            })
            .min_by_key(|(bucket, _)| *bucket);
        if let Some((bucket, address)) = best {
            return Some(ClassicAllocationPlan {
                address,
                requested_size: size,
                aligned_size,
                alignment,
                heap_limit,
                source: ClassicAllocationPlanSource::Free {
                    bucket,
                    retains_capacity: true,
                },
            });
        }

        let (address, next_heap_ptr) =
            Self::bump_allocation_address(&state, aligned_size, alignment)?;
        if next_heap_ptr >= heap_limit {
            return None;
        }
        Some(ClassicAllocationPlan {
            address,
            requested_size: size,
            aligned_size,
            alignment,
            heap_limit,
            source: ClassicAllocationPlanSource::Bump {
                previous_heap_ptr: state.heap_ptr,
                next_heap_ptr,
            },
        })
    }

    fn commit_allocation_plan(&self, plan: ClassicAllocationPlan) -> bool {
        let mut state = self.0.borrow_mut();
        if state.alloc_sizes.contains_key(&plan.address)
            || Self::range_overlaps_reserved(&state, plan.address, plan.capacity())
        {
            return false;
        }

        let event = match plan.source {
            ClassicAllocationPlanSource::Free {
                bucket,
                retains_capacity,
            } => {
                let Some(blocks) = state.free_blocks.get_mut(&bucket) else {
                    return false;
                };
                let Some(index) = blocks.iter().position(|&address| address == plan.address)
                else {
                    return false;
                };
                blocks.swap_remove(index);
                if retains_capacity {
                    state.alloc_bucket_sizes.insert(plan.address, bucket);
                }
                if plan.alignment == 4 {
                    if retains_capacity {
                        "reuse-best"
                    } else {
                        "reuse-exact"
                    }
                } else if retains_capacity {
                    "reuse-best-aligned"
                } else {
                    "reuse-exact-aligned"
                }
            }
            ClassicAllocationPlanSource::Bump {
                previous_heap_ptr,
                next_heap_ptr,
            } => {
                if state.heap_ptr != previous_heap_ptr
                    || Self::bump_allocation_address(
                        &state,
                        plan.aligned_size,
                        plan.alignment,
                    ) != Some((plan.address, next_heap_ptr))
                    || next_heap_ptr >= plan.heap_limit
                {
                    return false;
                }
                state.heap_ptr = next_heap_ptr;
                if plan.alignment == 4 {
                    "bump"
                } else {
                    "bump-aligned"
                }
            }
        };

        state.alloc_sizes.insert(plan.address, plan.requested_size);
        crate::memory::bus::trace_alloc_event(
            event,
            plan.address,
            plan.requested_size,
            plan.aligned_size,
        );
        true
    }

    pub(crate) fn allocate(&self, size: u32, alignment: u32, heap_limit: u32) -> u32 {
        let Some(plan) = self.allocation_plan(size, alignment, heap_limit) else {
            return 0;
        };
        if self.commit_allocation_plan(plan) {
            plan.address
        } else {
            0
        }
    }

    pub(crate) fn heap_bump_ptr(&self) -> u32 {
        self.0.borrow().heap_ptr
    }

    pub(crate) fn set_allocation_size(&self, address: u32, size: u32) {
        let mut state = self.0.borrow_mut();
        if let Some(old_size) = state.alloc_sizes.get(&address).copied() {
            let capacity = state
                .alloc_bucket_sizes
                .get(&address)
                .copied()
                .unwrap_or_else(|| Self::allocation_bucket_size(old_size));
            state.alloc_bucket_sizes.insert(address, capacity);
            state.alloc_sizes.insert(address, size);
        }
    }

    pub(crate) fn free(&self, address: u32) {
        if address == 0 {
            return;
        }
        let mut state = self.0.borrow_mut();
        if let Some(size) = state.alloc_sizes.remove(&address) {
            let bucket = state
                .alloc_bucket_sizes
                .remove(&address)
                .unwrap_or_else(|| Self::allocation_bucket_size(size));
            state.free_blocks.entry(bucket).or_default().push(address);
            crate::memory::bus::trace_alloc_event("free", address, size, bucket);
        }
    }
}

/// Architecture-neutral Memory Manager metadata for one Macintosh process.
///
/// Guest addresses, rather than CPU adapter records, identify relocatable
/// blocks. Keeping the reverse master-pointer index and handle state here
/// gives 68K traps and native imports one canonical registry as allocation
/// itself moves behind this process-level boundary. Inside Macintosh: Memory
/// (1992), pp. 2-12, 2-40--2-41.
#[derive(Debug, Default)]
pub(crate) struct ProcessMemoryManager {
    native: ProcessNativeMemoryManager,
}

/// Canonical architecture-neutral allocation state used directly by native
/// imports and by the classic Memory Manager bridge.
#[derive(Debug, Default)]
pub(crate) struct ProcessNativeMemoryManager {
    classic_owner: Rc<()>,
    classic_allocator: Option<SharedClassicHeapAllocator>,
    /// Canonical upper bound for classic allocations in this process. The
    /// adapter bus supplies it once at attachment; detached managers retain
    /// the same ceiling even when their byte adapter is recreated.
    classic_heap_limit: Option<u32>,
    ptr_to_handle: SharedProcessMap<u32>,
    handle_state_bits: SharedProcessMap<u8>,
    handle_high_locked: SharedProcessMap<bool>,
    native_handle_ptrs: HashSet<u32>,
    native_handles: HashSet<u32>,
    native_allocations: Vec<ProcessHandleRecord>,
    native_allocator: Option<ProcessNativeAllocatorState>,
    native_allocator_dirty: bool,
    /// Process-owned application heap limit exposed by GetApplLimit and
    /// SetApplLimit. This is deliberately separate from the native allocator
    /// ceiling: the PowerPC Memory Manager uses the latter to protect its
    /// fixed stack mapping, while the former is the guest-visible boundary
    /// between heap growth and stack space. Inside Macintosh: Memory (1992),
    /// pp. 2-83--2-85; PowerPC System Software (1994), pp. 1-60, 1-69--1-70.
    application_heap_limit: Option<u32>,
}

impl std::ops::Deref for ProcessMemoryManager {
    type Target = ProcessNativeMemoryManager;

    fn deref(&self) -> &Self::Target {
        &self.native
    }
}

impl std::ops::DerefMut for ProcessMemoryManager {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.native
    }
}

/// Shared ownership handle for one process's architecture-neutral Memory Manager.
///
/// CPU adapters retain this handle across execution slices. Allocator operations
/// take a short mutable manager borrow, while handle indexes remain independently
/// borrowable for reentrant cross-ISA callbacks. The runner serializes adapters.
#[derive(Debug, Clone)]
pub(crate) struct SharedProcessMemoryManager {
    manager: Rc<RefCell<ProcessMemoryManager>>,
    /// Reverse handle index used by RecoverHandle. Inside Macintosh Volume V
    /// (1986), p. V-579.
    ptr_to_handle: SharedProcessMap<u32>,
    /// Guest-visible lock, purge, and resource bits indexed by master pointer.
    /// Inside Macintosh: Memory (1992), pp. 2-46--2-49.
    handle_state_bits: SharedProcessMap<u8>,
    /// Native `HLockHi` placement state, kept separately from the master
    /// pointer's lock, purge, and resource bits. Inside Macintosh: Memory
    /// (1992), pp. 2-46--2-49, 2-58--2-59.
    handle_high_locked: SharedProcessMap<bool>,
}

impl Default for SharedProcessMemoryManager {
    fn default() -> Self {
        Self::from_manager(ProcessMemoryManager::default())
    }
}

impl ProcessNativeMemoryManager {
    const NATIVE_HEAP_ALIGNMENT: u32 = 16;
    const MEM_FULL_ERR: i16 = -108;
    const NIL_HANDLE_ERR: i16 = -109;
    const MEM_WZ_ERR: i16 = -111;
    const MEM_PUR_ERR: i16 = -112;
    const NO_ERR: i16 = 0;
    const PARAM_ERR: i16 = -50;

    pub(crate) fn detached_clone(&self) -> Self {
        let classic_owner = Rc::new(());
        let classic_owner_id = Rc::as_ptr(&classic_owner) as usize;
        Self {
            classic_owner,
            classic_allocator: self
                .classic_allocator
                .as_ref()
                .map(|allocator| allocator.detached_clone_for_owner(classic_owner_id)),
            classic_heap_limit: self.classic_heap_limit,
            ptr_to_handle: self.ptr_to_handle.detached_clone(),
            handle_state_bits: self.handle_state_bits.detached_clone(),
            handle_high_locked: self.handle_high_locked.detached_clone(),
            native_handle_ptrs: self.native_handle_ptrs.clone(),
            native_handles: self.native_handles.clone(),
            native_allocations: self.native_allocations.clone(),
            native_allocator: self.native_allocator.clone(),
            native_allocator_dirty: self.native_allocator_dirty,
            application_heap_limit: self.application_heap_limit,
        }
    }

    pub(crate) fn restore_native_snapshot(&mut self, snapshot: Self) {
        match (&self.classic_allocator, snapshot.classic_allocator) {
            (Some(current), Some(snapshot)) => current.replace_state_from(&snapshot),
            (None, Some(snapshot)) => {
                self.classic_allocator = Some(
                    snapshot.detached_clone_for_owner(Rc::as_ptr(&self.classic_owner) as usize),
                );
            }
            (Some(current), None) => {
                current.replace_state_from(&SharedClassicHeapAllocator::default());
                self.classic_allocator = None;
            }
            (None, None) => {}
        }
        self.classic_heap_limit = snapshot.classic_heap_limit;
        self.ptr_to_handle.replace_from(&snapshot.ptr_to_handle);
        self.handle_state_bits
            .replace_from(&snapshot.handle_state_bits);
        self.handle_high_locked
            .replace_from(&snapshot.handle_high_locked);
        self.native_handle_ptrs = snapshot.native_handle_ptrs;
        self.native_handles = snapshot.native_handles;
        self.native_allocations = snapshot.native_allocations;
        self.native_allocator = snapshot.native_allocator;
        self.native_allocator_dirty = snapshot.native_allocator_dirty;
        self.application_heap_limit = snapshot.application_heap_limit;
    }

    fn commit_empty_native_handle(&mut self, record: ProcessHandleRecord) {
        if record.ptr != 0 {
            self.ptr_to_handle.remove(&record.ptr);
            self.native_handle_ptrs.remove(&record.ptr);
        }
        self.set_native_allocation_record(ProcessHandleRecord {
            handle: record.handle,
            ptr: 0,
            size: 0,
            capacity: 0,
        });
        if let Some(allocator) = &mut self.native_allocator {
            if record.ptr != 0 {
                allocator.free_ptr_blocks.push(ProcessPtrRecord {
                    ptr: record.ptr,
                    size: record.capacity,
                });
            }
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
        }
    }

    pub(crate) fn empty_native_handle(
        &mut self,
        memory: &mut GuestAddressSpace,
        handle: u32,
    ) -> i16 {
        let Some(record) = self.native_allocation(handle) else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        };
        if self.state_for_handle(handle).unwrap_or(0) & 0x80 != 0 {
            self.set_native_mem_error(Self::MEM_PUR_ERR);
            return Self::MEM_PUR_ERR;
        }
        if PpcMemory::read_u32_be(memory, handle) != Some(record.ptr)
            || PpcMemory::write_u32_be(memory, handle, 0).is_none()
        {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }
        self.commit_empty_native_handle(record);
        Self::NO_ERR
    }
}

impl SharedProcessMemoryManager {
    fn from_manager(manager: ProcessMemoryManager) -> Self {
        let ptr_to_handle = manager.ptr_to_handle.clone();
        let handle_state_bits = manager.handle_state_bits.clone();
        let handle_high_locked = manager.handle_high_locked.clone();
        Self {
            manager: Rc::new(RefCell::new(manager)),
            ptr_to_handle,
            handle_state_bits,
            handle_high_locked,
        }
    }

    pub(crate) fn borrow(&self) -> std::cell::Ref<'_, ProcessMemoryManager> {
        self.manager.borrow()
    }

    pub(crate) fn borrow_mut(&self) -> RefMut<'_, ProcessMemoryManager> {
        self.manager.borrow_mut()
    }

    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.manager, &other.manager)
    }

    pub(crate) fn track_handle_ptr(&self, ptr: u32, handle: u32) -> Option<u32> {
        self.ptr_to_handle.insert(ptr, handle)
    }

    pub(crate) fn untrack_handle_ptr(&self, ptr: u32) -> Option<u32> {
        self.ptr_to_handle.remove(&ptr)
    }

    pub(crate) fn handle_for_ptr(&self, ptr: u32) -> Option<u32> {
        self.ptr_to_handle.get(&ptr)
    }

    #[cfg(test)]
    pub(crate) fn has_handle_ptr(&self, ptr: u32) -> bool {
        self.ptr_to_handle.contains_key(&ptr)
    }

    #[cfg(test)]
    pub(crate) fn set_handle_state(&self, handle: u32, state: u8) {
        if handle != 0 {
            self.handle_state_bits.insert(handle, state);
            if state & 0x80 == 0 {
                self.handle_high_locked.remove(&handle);
            }
        }
    }

    pub(crate) fn remove_handle_state(&self, handle: u32) -> Option<u8> {
        self.handle_high_locked.remove(&handle);
        self.handle_state_bits.remove(&handle)
    }

    pub(crate) fn handle_state(&self, handle: u32) -> Option<u8> {
        self.handle_state_bits.get(&handle)
    }

    pub(crate) fn update_handle_state(
        &self,
        handle: u32,
        update: impl FnOnce(Option<u8>) -> Option<u8>,
    ) {
        let mut updated = None;
        self.handle_state_bits.update(handle, |state| {
            updated = update(state);
            updated
        });
        if updated.is_none_or(|state| state & 0x80 == 0) {
            self.handle_high_locked.remove(&handle);
        }
    }

    #[cfg(test)]
    pub(crate) fn has_handle_state(&self, handle: u32) -> bool {
        self.handle_state_bits.contains_key(&handle)
    }

    /// Copy process Memory Manager metadata without retaining adapter sharing.
    ///
    /// A cloned CPU adapter represents a detached execution snapshot, so its
    /// allocation records and handle metadata must evolve independently.
    pub(crate) fn detached_clone(&self) -> Self {
        Self::from_manager(self.manager.borrow().detached_clone())
    }
}

impl ProcessMemoryManager {
    #[cfg(test)]
    const MEM_FULL_ERR: i16 = ProcessNativeMemoryManager::MEM_FULL_ERR;
    #[cfg(test)]
    const NIL_HANDLE_ERR: i16 = ProcessNativeMemoryManager::NIL_HANDLE_ERR;
    #[cfg(test)]
    const MEM_PUR_ERR: i16 = ProcessNativeMemoryManager::MEM_PUR_ERR;
    #[cfg(test)]
    const NO_ERR: i16 = ProcessNativeMemoryManager::NO_ERR;
    #[cfg(test)]
    const PARAM_ERR: i16 = ProcessNativeMemoryManager::PARAM_ERR;

    pub(crate) fn detached_clone(&self) -> Self {
        Self {
            native: self.native.detached_clone(),
        }
    }

    pub(crate) fn has_native_allocator(&self) -> bool {
        self.native_allocator.is_some()
    }

    pub(crate) fn native_mut(&mut self) -> &mut ProcessNativeMemoryManager {
        &mut self.native
    }

    #[cfg(test)]
    pub(crate) fn restore_native_snapshot(&mut self, snapshot: Self) {
        self.native.restore_native_snapshot(snapshot.native);
    }
}

impl ProcessNativeMemoryManager {
    /// Adopt the classic heap used by the process's 68K memory-bus adapter.
    ///
    /// The first attached bus contributes its live launch-time allocator;
    /// later adapters attach to that same process-owned state. A populated
    /// adapter must already expose this process's guest RAM; this handoff moves
    /// allocator metadata, not guest bytes. Inside Macintosh: Memory (1992),
    /// pp. 2-19--2-21.
    pub(crate) fn attach_classic_memory_bus(&mut self, bus: &mut MacMemoryBus) {
        let owner_id = Rc::as_ptr(&self.classic_owner) as usize;
        let bus_allocator = bus.shared_classic_heap_allocator();
        let bus_heap_limit = bus.classic_heap_limit();

        if let Some(heap_limit) = self.classic_heap_limit {
            assert_eq!(
                heap_limit, bus_heap_limit,
                "cannot attach a classic bus with a different heap ceiling"
            );
        }

        bus_allocator.assert_can_claim_owner(owner_id);
        let bus_is_pristine = bus_allocator.is_pristine();
        if let Some(allocator) = self.classic_allocator.as_ref() {
            if !allocator.ptr_eq(&bus_allocator) {
                assert!(
                    allocator.is_pristine() || bus_is_pristine,
                    "cannot attach two populated classic heap allocators"
                );
            }
        }
        bus_allocator.claim_owner(owner_id);

        if let Some(allocator) = &self.classic_allocator {
            if allocator.ptr_eq(&bus_allocator) {
                return;
            }
            if allocator.is_pristine() && !bus_is_pristine {
                allocator.replace_pristine_state_from(&bus_allocator);
                bus.replace_adopted_classic_heap_allocator(allocator.clone());
            } else {
                bus.attach_classic_heap_allocator(allocator.clone());
            }
        } else {
            self.classic_allocator = Some(bus_allocator);
        }
        self.classic_heap_limit = Some(bus_heap_limit);
    }

    fn assert_classic_memory_bus_attached(&self, bus: &MacMemoryBus) {
        let allocator = self
            .classic_allocator
            .as_ref()
            .expect("classic Memory Manager operation requires an attached bus");
        assert!(
            allocator.ptr_eq(&bus.shared_classic_heap_allocator()),
            "classic Memory Manager operation used a detached bus"
        );
    }

    fn classic_allocator(&self) -> &SharedClassicHeapAllocator {
        self.classic_allocator
            .as_ref()
            .expect("classic Memory Manager operation requires an attached bus")
    }

    fn classic_heap_ceiling(&self) -> u32 {
        self.classic_heap_limit
            .expect("classic Memory Manager operation requires an attached bus")
    }

    pub(crate) fn classic_heap_bump_ptr(&self) -> u32 {
        self.classic_allocator().heap_bump_ptr()
    }

    pub(crate) fn reserve_classic_heap(&mut self, size: u32) {
        self.classic_allocator()
            .reserve_until(0x20_0000 + ((size + 3) & !3));
    }

    pub(crate) fn reserve_classic_heap_range(&mut self, start_addr: u32, end_addr: u32) {
        self.classic_allocator().reserve_range(start_addr, end_addr);
    }

    pub(crate) fn classic_allocation_size(&self, address: u32) -> Option<u32> {
        self.classic_allocator().allocation_size(address)
    }

    /// Allocate a classic nonrelocatable block for this process.
    ///
    /// `NewPtr` returns a fixed block in the current heap or `NIL` with
    /// `memFullErr`. Inside Macintosh: Memory (1992), pp. 2-36--2-37.
    pub(crate) fn new_classic_ptr(&mut self, bus: &mut MacMemoryBus, size: u32) -> u32 {
        self.assert_classic_memory_bus_attached(bus);
        self.classic_allocator()
            .allocate(size, 4, self.classic_heap_ceiling())
    }

    /// Release a native or classic nonrelocatable block owned by this process.
    ///
    /// Native allocator metadata is updated immediately even when `DisposePtr`
    /// originates in an attached 68K callback. Inside Macintosh: Memory
    /// (1992), pp. 2-38--2-39.
    pub(crate) fn dispose_process_ptr(
        &mut self,
        bus: &mut MacMemoryBus,
        ptr: u32,
    ) -> Option<ProcessPtrRecord> {
        self.assert_classic_memory_bus_attached(bus);
        if self
            .native_allocator
            .as_ref()
            .is_some_and(|allocator| allocator.ptrs.iter().any(|record| record.ptr == ptr))
        {
            self.dispose_native_ptr(ptr)
        } else {
            self.classic_allocator().free(ptr);
            None
        }
    }

    /// Allocate a classic relocatable block and stable master pointer.
    ///
    /// `NewHandle` creates an unlocked, unpurgeable block and returns `NIL`
    /// if either allocation fails. Inside Macintosh: Memory (1992),
    /// pp. 2-29--2-31.
    pub(crate) fn new_classic_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        size: u32,
    ) -> Result<(u32, u32), i16> {
        self.assert_classic_memory_bus_attached(bus);
        let allocator = self.classic_allocator();
        let ptr = allocator.allocate(size, 4, self.classic_heap_ceiling());
        if ptr == 0 && size > 0 {
            return Err(Self::MEM_FULL_ERR);
        }
        let handle = allocator.allocate(4, 4, self.classic_heap_ceiling());
        if handle == 0 {
            allocator.free(ptr);
            return Err(Self::MEM_FULL_ERR);
        }
        bus.write_long(handle, ptr);
        self.ptr_to_handle.insert(ptr, handle);
        Ok((handle, ptr))
    }

    /// Allocate a current-heap handle containing a copy of `bytes`.
    ///
    /// `PtrToHand` creates a new relocatable block in the current heap and
    /// copies the requested bytes into it. Inside Macintosh: Memory (1992),
    /// pp. 2-60--2-61.
    pub(crate) fn copy_bytes_to_new_classic_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        bytes: &[u8],
    ) -> Result<(u32, u32), i16> {
        self.assert_classic_memory_bus_attached(bus);
        let size = u32::try_from(bytes.len()).map_err(|_| Self::MEM_FULL_ERR)?;
        let (handle, ptr) = self.new_classic_handle(bus, size)?;
        bus.write_bytes(ptr, bytes);
        Ok((handle, ptr))
    }

    fn process_handle_bytes(&self, bus: &MacMemoryBus, handle: u32) -> Result<Vec<u8>, i16> {
        self.assert_classic_memory_bus_attached(bus);
        if handle == 0 {
            return Err(Self::NIL_HANDLE_ERR);
        }
        let ptr = bus.read_long(handle);
        if ptr == 0 {
            return Err(Self::NIL_HANDLE_ERR);
        }
        if let Some(record) = self.native_allocation(handle) {
            if record.ptr != ptr {
                return Err(Self::NIL_HANDLE_ERR);
            }
            return Ok(bus.read_bytes(ptr, record.size as usize));
        }
        if self.classic_allocator().allocation_size(handle) != Some(4) {
            return Err(Self::MEM_WZ_ERR);
        }
        let Some(size) = self.classic_allocator().allocation_size(ptr) else {
            return Err(Self::MEM_WZ_ERR);
        };
        Ok(bus.read_bytes(ptr, size as usize))
    }

    /// Copy a relocatable block into a new handle in the source heap zone.
    ///
    /// The copy is unlocked, unpurgeable, and not a resource. Inside
    /// Macintosh: Memory (1992), pp. 2-62--2-63.
    pub(crate) fn copy_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
    ) -> Result<(u32, u32), i16> {
        let bytes = self.process_handle_bytes(bus, handle)?;
        if self.native_allocation(handle).is_some() {
            let copy = bus
                .with_foreign_address_space(|memory| {
                    self.copy_bytes_to_new_native_handle(memory, &bytes)
                })
                .ok_or(Self::PARAM_ERR)?;
            if copy == 0 {
                return Err(self
                    .native_heap_state()
                    .map(|heap| heap.last_mem_error)
                    .unwrap_or(Self::MEM_FULL_ERR));
            }
            return Ok((copy, bus.read_long(copy)));
        }
        self.copy_bytes_to_new_classic_handle(bus, &bytes)
    }

    /// Replace a native or classic relocatable block with copied bytes.
    ///
    /// `PtrToXHand` preserves the stable handle while changing its logical
    /// size and contents. Inside Macintosh: Memory (1992), pp. 2-61--2-62.
    pub(crate) fn replace_process_handle_bytes(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        bytes: &[u8],
    ) -> i16 {
        self.assert_classic_memory_bus_attached(bus);
        if handle == 0 {
            return Self::NIL_HANDLE_ERR;
        }
        if let Some(record) = self.native_allocation(handle) {
            return self
                .replace_native_handle_bytes(bus, handle, record.ptr, bytes)
                .map_or_else(|error| error, |_| Self::NO_ERR);
        }
        if self.classic_allocator().allocation_size(handle) != Some(4) {
            return Self::MEM_WZ_ERR;
        }
        let Ok(size) = u32::try_from(bytes.len()) else {
            return Self::MEM_FULL_ERR;
        };
        let result = self.set_process_handle_size(bus, handle, size);
        if result != Self::NO_ERR {
            return result;
        }
        let ptr = bus.read_long(handle);
        bus.write_bytes(ptr, bytes);
        Self::NO_ERR
    }

    /// Append bytes to a native or classic relocatable block.
    ///
    /// `HandAndHand` and `PtrAndHand` leave their source unchanged while the
    /// destination handle remains stable. Inside Macintosh: Memory (1992),
    /// pp. 2-64--2-66.
    pub(crate) fn append_bytes_to_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        bytes: &[u8],
    ) -> i16 {
        let mut combined = match self.process_handle_bytes(bus, handle) {
            Ok(bytes) => bytes,
            Err(error) => return error,
        };
        if combined.len().checked_add(bytes.len()).is_none() {
            return Self::MEM_FULL_ERR;
        }
        combined.extend_from_slice(bytes);
        self.replace_process_handle_bytes(bus, handle, &combined)
    }

    /// Append one relocatable block to another without changing the source.
    /// Inside Macintosh: Memory (1992), pp. 2-64--2-65.
    pub(crate) fn append_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        source: u32,
        destination: u32,
    ) -> i16 {
        let source_bytes = match self.process_handle_bytes(bus, source) {
            Ok(bytes) => bytes,
            Err(error) => return error,
        };
        self.append_bytes_to_process_handle(bus, destination, &source_bytes)
    }

    /// Allocate a classic master pointer whose relocatable block is empty.
    ///
    /// `NewEmptyHandle` returns a handle containing `NIL`. Inside Macintosh:
    /// Memory (1992), pp. 2-33--2-34.
    pub(crate) fn new_empty_classic_handle(&mut self, bus: &mut MacMemoryBus) -> Result<u32, i16> {
        self.assert_classic_memory_bus_attached(bus);
        let handle = self
            .classic_allocator()
            .allocate(4, 4, self.classic_heap_ceiling());
        if handle == 0 {
            return Err(Self::MEM_FULL_ERR);
        }
        bus.write_long(handle, 0);
        Ok(handle)
    }

    /// Release a classic relocatable block and its master pointer.
    ///
    /// The stale reverse entry is intentionally retained because disposed
    /// master-pointer contents are undefined and `RecoverHandle` scans those
    /// slots. Inside Macintosh: Memory (1992), pp. 2-34--2-35, and Inside
    /// Macintosh Volume V (1986), p. V-579.
    pub(crate) fn dispose_classic_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        dispose_data: bool,
    ) {
        self.assert_classic_memory_bus_attached(bus);
        if handle == 0 {
            return;
        }
        let ptr = bus.read_long(handle);
        if dispose_data {
            self.classic_allocator().free(ptr);
        }
        self.classic_allocator().free(handle);
        self.handle_state_bits.remove(&handle);
        self.handle_high_locked.remove(&handle);
    }

    fn commit_dispose_native_handle(&mut self, index: usize, record: ProcessHandleRecord) {
        self.native_allocations.remove(index);
        if record.ptr != 0 {
            self.ptr_to_handle.remove(&record.ptr);
            self.native_handle_ptrs.remove(&record.ptr);
        }
        self.handle_state_bits.remove(&record.handle);
        self.handle_high_locked.remove(&record.handle);
        self.native_handles.remove(&record.handle);
        if let Some(allocator) = &mut self.native_allocator {
            allocator.free_handle_blocks.push(record);
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
        }
    }

    /// Release a native or classic relocatable block and its master pointer.
    ///
    /// A native block is returned to the native allocator even when disposal
    /// originates in an attached 68K callback. Classic resource callers may
    /// retain their separately owned data block while still releasing the
    /// handle. Inside Macintosh: Memory (1992), pp. 2-34--2-35.
    pub(crate) fn dispose_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        dispose_classic_data: bool,
    ) -> Result<Option<ProcessHandleRecord>, i16> {
        self.assert_classic_memory_bus_attached(bus);
        let Some((index, record)) = self
            .native_allocations
            .iter()
            .copied()
            .enumerate()
            .find(|(_, record)| record.handle == handle)
        else {
            self.dispose_classic_handle(bus, handle, dispose_classic_data);
            return Ok(None);
        };
        if bus.read_long(handle) != record.ptr
            || bus
                .write_foreign_bytes(handle, &0u32.to_be_bytes())
                .is_none()
        {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        }
        self.commit_dispose_native_handle(index, record);
        Ok(Some(record))
    }

    /// Return the logical size of a native or classic nonrelocatable block.
    /// Inside Macintosh: Memory (1992), pp. 2-41--2-42.
    pub(crate) fn process_ptr_size(&self, bus: &MacMemoryBus, ptr: u32) -> Option<u32> {
        self.assert_classic_memory_bus_attached(bus);
        self.native_allocator
            .as_ref()
            .and_then(|allocator| allocator.ptrs.iter().find(|record| record.ptr == ptr))
            .map(|record| record.size)
            .or_else(|| self.classic_allocator().allocation_size(ptr))
    }

    /// Change a native or classic nonrelocatable block's logical size without
    /// moving its pointer. Inside Macintosh: Memory (1992), pp. 2-42--2-43.
    pub(crate) fn set_process_ptr_size(
        &mut self,
        bus: &mut MacMemoryBus,
        ptr: u32,
        new_size: u32,
    ) -> i16 {
        self.assert_classic_memory_bus_attached(bus);
        if ptr == 0 {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }
        let native_index = self
            .native_allocator
            .as_ref()
            .and_then(|allocator| allocator.ptrs.iter().position(|record| record.ptr == ptr));
        let (old_size, capacity) = if let Some(index) = native_index {
            let old_size = self
                .native_allocator
                .as_ref()
                .and_then(|allocator| allocator.ptrs.get(index))
                .map(|record| record.size)
                .unwrap_or(0);
            (
                old_size,
                SharedClassicHeapAllocator::allocation_bucket_size(old_size),
            )
        } else {
            let allocator = self.classic_allocator();
            let Some(old_size) = allocator.allocation_size(ptr) else {
                self.set_native_mem_error(Self::MEM_WZ_ERR);
                return Self::MEM_WZ_ERR;
            };
            (
                old_size,
                allocator
                    .allocation_capacity(ptr)
                    .expect("classic pointer retains its allocation capacity"),
            )
        };
        if SharedClassicHeapAllocator::allocation_bucket_size(new_size) > capacity {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        }
        if new_size < old_size {
            bus.fill_zeros(ptr.wrapping_add(new_size), old_size - new_size);
        }
        if let Some(index) = native_index {
            let allocator = self
                .native_allocator
                .as_mut()
                .expect("native pointer record retains its allocator");
            allocator.ptrs[index].size = new_size;
            self.native_allocator_dirty = true;
        } else {
            self.classic_allocator().set_allocation_size(ptr, new_size);
        }
        self.set_native_mem_error(Self::NO_ERR);
        Self::NO_ERR
    }

    /// Return the logical size of a native or classic relocatable block.
    /// Inside Macintosh: Memory (1992), pp. 2-39--2-40.
    pub(crate) fn process_handle_size(&self, bus: &MacMemoryBus, handle: u32) -> Option<u32> {
        self.assert_classic_memory_bus_attached(bus);
        self.native_allocations
            .iter()
            .find(|record| record.handle == handle)
            .map(|record| record.size)
            .or_else(|| {
                (handle != 0)
                    .then(|| bus.read_long(handle))
                    .and_then(|ptr| self.classic_allocator().allocation_size(ptr))
            })
    }

    /// Return a relocatable block's logical size from process-owned allocator
    /// metadata and its current master pointer.
    ///
    /// Native imports can therefore inspect classic allocations without an
    /// architecture-specific bus adapter. Inside Macintosh: Memory (1992),
    /// pp. 2-39--2-40.
    pub(crate) fn process_handle_size_from_master_pointer(
        &mut self,
        handle: u32,
        ptr: u32,
    ) -> Option<u32> {
        let size = if handle == 0 || ptr == 0 {
            None
        } else {
            self.native_allocations
                .iter()
                .find(|record| record.handle == handle && record.ptr == ptr)
                .map(|record| record.size)
                .or_else(|| {
                    self.classic_allocator.as_ref().and_then(|allocator| {
                        if allocator.allocation_size(handle) == Some(4) {
                            allocator.allocation_size(ptr)
                        } else {
                            None
                        }
                    })
                })
        };
        self.set_native_mem_error(if size.is_some() {
            Self::NO_ERR
        } else {
            Self::NIL_HANDLE_ERR
        });
        size
    }

    /// Change the logical size of a native or classic relocatable block.
    ///
    /// The handle remains stable while the Memory Manager may move its data
    /// block and update the master pointer. Inside Macintosh: Memory (1992),
    /// pp. 2-40--2-41.
    pub(crate) fn set_process_handle_size(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        new_size: u32,
    ) -> i16 {
        self.assert_classic_memory_bus_attached(bus);
        if handle == 0 {
            return Self::NIL_HANDLE_ERR;
        }

        if let Some(record) = self.native_allocation(handle) {
            let Ok(new_len) = usize::try_from(new_size) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Self::MEM_FULL_ERR;
            };
            let copy_len = record.size.min(new_size) as usize;
            let mut bytes = vec![0; new_len];
            if copy_len > 0 {
                bytes[..copy_len].copy_from_slice(&bus.read_bytes(record.ptr, copy_len));
            }
            return self
                .replace_native_handle_bytes(bus, handle, record.ptr, &bytes)
                .map_or_else(|error| error, |_| Self::NO_ERR);
        }

        let old_ptr = bus.read_long(handle);
        let old_size = self
            .classic_allocator()
            .allocation_size(old_ptr)
            .unwrap_or(0);
        if old_size == new_size
            || (old_ptr != 0
                && SharedClassicHeapAllocator::allocation_bucket_size(new_size)
                    == SharedClassicHeapAllocator::allocation_bucket_size(old_size))
        {
            if new_size < old_size {
                bus.fill_zeros(old_ptr.wrapping_add(new_size), old_size - new_size);
            }
            self.classic_allocator()
                .set_allocation_size(old_ptr, new_size);
            return Self::NO_ERR;
        }

        let new_ptr = self
            .classic_allocator()
            .allocate(new_size, 4, self.classic_heap_ceiling());
        if new_ptr == 0 && new_size > 0 {
            return Self::MEM_FULL_ERR;
        }
        let copy_len = old_size.min(new_size) as usize;
        if copy_len > 0 {
            let bytes = bus.read_bytes(old_ptr, copy_len);
            bus.write_bytes(new_ptr, &bytes);
        }
        self.classic_allocator().free(old_ptr);
        bus.write_long(handle, new_ptr);
        self.ptr_to_handle.remove(&old_ptr);
        self.ptr_to_handle.insert(new_ptr, handle);
        Self::NO_ERR
    }

    /// Resize a Resource Manager handle through the allocator that owns it.
    ///
    /// Resource metadata remains the Resource Manager's responsibility, but
    /// moving the relocatable block, updating the stable master pointer, and
    /// changing the reverse pointer index form one process Memory Manager
    /// transaction. This is especially important when 68K code resizes a
    /// resource handle allocated by the native PowerPC heap. Inside
    /// Macintosh: Memory (1992), pp. 2-40--2-41, and More Macintosh Toolbox
    /// (1993), pp. 1-84--1-85.
    pub(crate) fn resize_process_resource_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        backing_ptr: u32,
        new_size: u32,
    ) -> Result<(u32, u32), i16> {
        self.assert_classic_memory_bus_attached(bus);
        if handle == 0 {
            return Err(Self::NIL_HANDLE_ERR);
        }

        if let Some(record) = self.native_allocation(handle) {
            if record.ptr == 0 {
                if new_size == 0 {
                    self.set_native_mem_error(Self::NO_ERR);
                    return Ok((0, 0));
                }
                let Ok(len) = usize::try_from(new_size) else {
                    self.set_native_mem_error(Self::MEM_FULL_ERR);
                    return Err(Self::MEM_FULL_ERR);
                };
                return self.replace_native_handle_bytes_with_relocation(
                    bus,
                    handle,
                    0,
                    &vec![0; len],
                    true,
                );
            }
            if backing_ptr != 0 && backing_ptr != record.ptr {
                self.set_native_mem_error(Self::NIL_HANDLE_ERR);
                return Err(Self::NIL_HANDLE_ERR);
            }
            let old_ptr = record.ptr;
            let result = self.set_process_handle_size(bus, handle, new_size);
            if result != Self::NO_ERR {
                return Err(result);
            }
            let new_ptr = self
                .native_allocation(handle)
                .map(|record| record.ptr)
                .ok_or(Self::NIL_HANDLE_ERR)?;
            return Ok((old_ptr, new_ptr));
        }

        if self.classic_allocator().allocation_size(handle) != Some(4) {
            return Err(Self::MEM_WZ_ERR);
        }
        let live_ptr = bus.read_long(handle);
        let old_ptr = if live_ptr != 0 { live_ptr } else { backing_ptr };
        if old_ptr == 0 && new_size == 0 {
            return Ok((0, 0));
        }
        let old_size = self
            .classic_allocator()
            .allocation_size(old_ptr)
            .unwrap_or(0);
        let old_capacity = SharedClassicHeapAllocator::allocation_bucket_size(old_size);
        let new_capacity = SharedClassicHeapAllocator::allocation_bucket_size(new_size);
        if old_ptr != 0 && new_capacity <= old_capacity {
            if new_size < old_size {
                bus.fill_zeros(old_ptr.wrapping_add(new_size), old_size - new_size);
            }
            self.classic_allocator()
                .set_allocation_size(old_ptr, new_size);
            return Ok((old_ptr, old_ptr));
        }

        let new_ptr = self
            .classic_allocator()
            .allocate(new_size, 4, self.classic_heap_ceiling());
        if new_ptr == 0 && new_size > 0 {
            return Err(Self::MEM_FULL_ERR);
        }
        let copy_len = old_size.min(new_size) as usize;
        if copy_len > 0 {
            let bytes = bus.read_bytes(old_ptr, copy_len);
            bus.write_bytes(new_ptr, &bytes);
        }
        self.classic_allocator().free(old_ptr);
        bus.write_long(handle, new_ptr);
        if old_ptr != 0 {
            self.ptr_to_handle.remove(&old_ptr);
        }
        if new_ptr != 0 {
            self.ptr_to_handle.insert(new_ptr, handle);
        }
        Ok((old_ptr, new_ptr))
    }

    /// Replace a native or classic relocatable block without changing its handle.
    ///
    /// The replacement has undefined contents and is left unlocked and
    /// unpurgeable. If allocation fails, the prior block, master pointer, and
    /// handle state remain unchanged. Inside Macintosh: Memory (1992),
    /// pp. 2-52--2-53.
    pub(crate) fn reallocate_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        size: u32,
    ) -> Result<(u32, u32), i16> {
        self.assert_classic_memory_bus_attached(bus);
        if (size as i32) < 0 {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        }

        let native_record = self.native_allocation(handle);
        if native_record.is_none()
            && (handle == 0 || self.classic_allocator().allocation_size(handle) != Some(4))
        {
            return Err(Self::MEM_WZ_ERR);
        }

        let relocated = if let Some(record) = native_record {
            let Some(required) = ProcessNativeMemoryManager::native_allocation_size(size) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Err(Self::MEM_FULL_ERR);
            };
            let Some(allocator) = self.native_allocator.as_ref() else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Err(Self::MEM_FULL_ERR);
            };
            let allocation_limit = self.native_allocation_limit(allocator.heap.heap_limit);
            let reusable = allocator.free_ptr_blocks.iter().any(|free| {
                free.ptr != record.ptr
                    && ProcessNativeMemoryManager::native_allocation_size(free.size)
                        .is_some_and(|capacity| {
                            capacity >= required
                                && free
                                    .ptr
                                    .checked_add(capacity)
                                    .is_some_and(|end| end <= allocation_limit)
                        })
            });
            if !reusable
                && ProcessNativeMemoryManager::native_allocation_bounds(
                    allocator.heap.heap_cursor,
                    allocation_limit,
                    required,
                    |ptr, len| bus.foreign_readonly_allocation_overlap_end(ptr, len),
                )
                .is_none()
            {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Err(Self::MEM_FULL_ERR);
            }
            let replacement = usize::try_from(size)
                .ok()
                .map(|len| vec![0xA5; len])
                .ok_or(Self::MEM_FULL_ERR)?;
            self.replace_native_handle_bytes_with_relocation(
                bus,
                handle,
                record.ptr,
                &replacement,
                true,
            )?
        } else {
            let new_ptr = self
                .classic_allocator()
                .allocate(size, 4, self.classic_heap_ceiling());
            if new_ptr == 0 && size > 0 {
                return Err(Self::MEM_FULL_ERR);
            }
            bus.fill_bytes(new_ptr, size, 0xA5);
            let old_ptr = bus.read_long(handle);
            self.classic_allocator().free(old_ptr);
            bus.write_long(handle, new_ptr);
            self.ptr_to_handle.remove(&old_ptr);
            self.ptr_to_handle.insert(new_ptr, handle);
            (old_ptr, new_ptr)
        };

        self.handle_state_bits.update(handle, |state| {
            let state = state.unwrap_or(0) & !0xC0;
            (state != 0).then_some(state)
        });
        self.handle_high_locked.remove(&handle);
        Ok(relocated)
    }

    /// Empty a native or classic relocatable block through the attached 68K bus.
    ///
    /// Allocation ownership and the reverse master-pointer index change as one
    /// process transaction while the stable handle and its resource/purge bits
    /// remain live. Inside Macintosh: Memory (1992), pp. 2-51--2-52.
    pub(crate) fn empty_process_handle(&mut self, bus: &mut MacMemoryBus, handle: u32) -> i16 {
        self.assert_classic_memory_bus_attached(bus);
        if let Some(record) = self.native_allocation(handle) {
            if self.state_for_handle(handle).unwrap_or(0) & 0x80 != 0 {
                self.set_native_mem_error(Self::MEM_PUR_ERR);
                return Self::MEM_PUR_ERR;
            }
            if bus.read_long(handle) != record.ptr
                || bus
                    .write_foreign_bytes(handle, &0u32.to_be_bytes())
                    .is_none()
            {
                self.set_native_mem_error(Self::NIL_HANDLE_ERR);
                return Self::NIL_HANDLE_ERR;
            }
            self.commit_empty_native_handle(record);
            return Self::NO_ERR;
        }

        if handle == 0 || self.classic_allocator().allocation_size(handle) != Some(4) {
            return Self::MEM_WZ_ERR;
        }
        if self.state_for_handle(handle).unwrap_or(0) & 0x80 != 0 {
            return Self::MEM_PUR_ERR;
        }
        let ptr = bus.read_long(handle);
        if ptr != 0 {
            self.classic_allocator().free(ptr);
            self.ptr_to_handle.remove(&ptr);
        }
        bus.write_long(handle, 0);
        Self::NO_ERR
    }
}

impl ProcessNativeMemoryManager {
    #[cfg(test)]
    pub(crate) fn register_native_handle_records(
        &mut self,
        handles: impl IntoIterator<Item = (ProcessHandleRecord, u8)>,
    ) {
        self.replace_native_handle_records(handles);
    }

    #[cfg(test)]
    fn replace_native_handle_records(
        &mut self,
        handles: impl IntoIterator<Item = (ProcessHandleRecord, u8)>,
    ) {
        for ptr in self.native_handle_ptrs.drain() {
            self.ptr_to_handle.remove(&ptr);
        }
        for handle in self.native_handles.drain() {
            self.handle_state_bits.remove(&handle);
            self.handle_high_locked.remove(&handle);
        }
        self.native_allocations.clear();
        for (record, adapter_state) in handles {
            let ProcessHandleRecord { handle, ptr, .. } = record;
            if handle != 0 {
                if ptr != 0 {
                    self.ptr_to_handle.insert(ptr, handle);
                    self.native_handle_ptrs.insert(ptr);
                }
                self.handle_state_bits.insert(handle, adapter_state);
                self.native_handles.insert(handle);
                self.native_allocations.push(record);
            }
        }
    }

    pub(crate) fn state_for_handle(&self, handle: u32) -> Option<u8> {
        self.handle_state_bits
            .get(&handle)
            .or_else(|| self.native_handles.contains(&handle).then_some(0))
    }

    pub(crate) fn set_state_for_handle(&mut self, handle: u32, state: u8) {
        if handle != 0 {
            self.handle_state_bits.insert(handle, state);
            if state & 0x80 == 0 {
                self.handle_high_locked.remove(&handle);
            }
        }
    }

    /// Lock a relocatable block, optionally requesting high-heap placement.
    ///
    /// The master pointer remains stable; `HLockHi` records its placement
    /// request separately from the guest-visible state byte. Inside Macintosh:
    /// Memory (1992), pp. 2-46--2-49 and 2-58--2-59.
    pub(crate) fn lock_process_handle(&mut self, handle: u32, high: bool) {
        if handle == 0 {
            return;
        }
        let state = self.state_for_handle(handle).unwrap_or(0) | 0x80;
        self.set_state_for_handle(handle, state);
        if high {
            self.handle_high_locked.insert(handle, true);
        }
    }

    /// Unlock a relocatable block and clear any high-heap placement request.
    /// Inside Macintosh: Memory (1992), pp. 2-46--2-49.
    pub(crate) fn unlock_process_handle(&mut self, handle: u32) {
        if handle == 0 {
            return;
        }
        let state = self.state_for_handle(handle).unwrap_or(0) & !0x80;
        self.set_state_for_handle(handle, state);
    }

    /// Change whether a relocatable block may be purged while preserving its
    /// lock and resource properties. Inside Macintosh: Memory (1992),
    /// pp. 2-46--2-48.
    pub(crate) fn set_process_handle_purgeable(&mut self, handle: u32, purgeable: bool) {
        if handle == 0 {
            return;
        }
        let state = self.state_for_handle(handle).unwrap_or(0);
        let state = if purgeable {
            state | 0x40
        } else {
            state & !0x40
        };
        self.set_state_for_handle(handle, state);
    }

    /// Restore the lock and purge properties of a relocatable block without
    /// changing its resource bit. Inside Macintosh: Memory (1992), p. 2-49.
    pub(crate) fn restore_process_handle_state(&mut self, handle: u32, state: u8) {
        if handle == 0 {
            return;
        }
        let resource = self.state_for_handle(handle).unwrap_or(0) & 0x20;
        self.set_state_for_handle(handle, resource | (state & 0xC0));
    }

    /// Change the resource property of a relocatable block while preserving
    /// its lock and purge properties. Inside Macintosh: Memory (1992),
    /// pp. 2-49--2-51.
    pub(crate) fn set_process_handle_resource(&mut self, handle: u32, resource: bool) {
        if handle == 0 {
            return;
        }
        let state = self.state_for_handle(handle).unwrap_or(0);
        let state = if resource {
            state | 0x20
        } else {
            state & !0x20
        };
        self.set_state_for_handle(handle, state);
    }

    pub(crate) fn native_handle_state(&self, handle: u32) -> ProcessHandleStateRecord {
        let bits = self.state_for_handle(handle).unwrap_or(0x40);
        let locked = bits & 0x80 != 0;
        ProcessHandleStateRecord {
            handle,
            locked,
            high_locked: locked && self.handle_high_locked.get(&handle).unwrap_or(false),
            no_purge: bits & 0x40 == 0,
            resource: bits & 0x20 != 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_native_handle_state(&mut self, state: ProcessHandleStateRecord) {
        let mut bits = 0u8;
        if state.locked {
            bits |= 0x80;
        }
        if !state.no_purge {
            bits |= 0x40;
        }
        if state.resource {
            bits |= 0x20;
        }
        self.set_state_for_handle(state.handle, bits);
        if state.locked && state.high_locked {
            self.handle_high_locked.insert(state.handle, true);
        }
    }

    pub(crate) fn native_allocation(&self, handle: u32) -> Option<ProcessHandleRecord> {
        self.native_allocations
            .iter()
            .find(|record| record.handle == handle)
            .copied()
    }

    pub(crate) fn native_handle_records(&self) -> &[ProcessHandleRecord] {
        &self.native_allocations
    }

    fn set_native_allocation_record(&mut self, record: ProcessHandleRecord) {
        if let Some(existing) = self
            .native_allocations
            .iter_mut()
            .find(|existing| existing.handle == record.handle)
        {
            *existing = record;
        } else {
            self.native_allocations.push(record);
        }
    }

    fn native_allocation_size(size: u32) -> Option<u32> {
        Some(
            size.checked_add(Self::NATIVE_HEAP_ALIGNMENT - 1)? & !(Self::NATIVE_HEAP_ALIGNMENT - 1),
        )
        .map(|size| size.max(Self::NATIVE_HEAP_ALIGNMENT))
    }

    fn native_allocation_bounds(
        heap_cursor: u32,
        heap_limit: u32,
        aligned_size: u32,
        mut readonly_overlap_end: impl FnMut(u32, u32) -> Option<u32>,
    ) -> Option<(u32, u32)> {
        let mut ptr = heap_cursor.checked_add(Self::NATIVE_HEAP_ALIGNMENT - 1)?
            & !(Self::NATIVE_HEAP_ALIGNMENT - 1);
        loop {
            let next = ptr.checked_add(aligned_size)?;
            if next >= heap_limit {
                return None;
            }
            let Some(reserved_end) = readonly_overlap_end(ptr, aligned_size) else {
                return Some((ptr, next));
            };
            ptr = reserved_end.checked_add(Self::NATIVE_HEAP_ALIGNMENT - 1)?
                & !(Self::NATIVE_HEAP_ALIGNMENT - 1);
        }
    }

    pub(crate) fn set_native_mem_error(&mut self, error: i16) {
        if let Some(allocator) = &mut self.native_allocator {
            allocator.heap.last_mem_error = error;
            self.native_allocator_dirty = true;
        }
    }

    /// Set the expandable application-heap boundary for subsequent native
    /// allocations. The caller has already enforced the guest stack ceiling.
    #[cfg(test)]
    pub(crate) fn set_native_heap_limit(&mut self, heap_limit: u32) {
        if let Some(allocator) = &mut self.native_allocator {
            allocator.heap.heap_limit = heap_limit;
            self.native_allocator_dirty = true;
        }
    }

    /// Return the process-owned application heap limit used by native Memory
    /// Manager imports. A caller-provided fallback keeps standalone managers
    /// useful before a native application partition has been initialized.
    pub(crate) fn application_heap_limit(&self, fallback: u32) -> u32 {
        self.application_heap_limit.unwrap_or(fallback)
    }

    pub(crate) fn application_heap_limit_is_set(&self) -> bool {
        self.application_heap_limit.is_some()
    }

    /// Return the effective native allocation boundary for the current
    /// process. The native heap's limit describes the mapped address ceiling;
    /// `ApplLimit` is the guest-visible boundary within that mapping. Native
    /// Memory Manager allocations must honor both. Inside Macintosh: Memory
    /// (1992), pp. 2-42--2-44 and 2-83--2-85.
    pub(crate) fn native_allocation_limit(&self, native_heap_limit: u32) -> u32 {
        self.application_heap_limit(native_heap_limit)
            .min(native_heap_limit)
    }

    /// Set the process-owned application heap limit without changing the
    /// native allocator's physical ceiling. Inside Macintosh: Memory (1992),
    /// pp. 2-83--2-85.
    pub(crate) fn set_application_heap_limit(&mut self, heap_limit: u32) {
        self.application_heap_limit = Some(heap_limit);
    }

    /// Record that the process application heap has been expanded to its limit.
    ///
    /// `MaxApplZone` grows the application heap as far as possible. Inside
    /// Macintosh: Memory (1992), pp. 2-83--2-84.
    pub(crate) fn maximize_native_heap(&mut self) {
        if let Some(allocator) = &mut self.native_allocator {
            allocator.heap.heap_maximized = true;
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
        }
    }

    /// Record one process-level request for another block of master pointers.
    ///
    /// `MoreMasters` adds master pointers to the current heap zone. Inside
    /// Macintosh: Memory (1992), pp. 2-85--2-86.
    pub(crate) fn request_native_master_pointers(&mut self) {
        if let Some(allocator) = &mut self.native_allocator {
            allocator.heap.master_pointer_blocks_requested = allocator
                .heap
                .master_pointer_blocks_requested
                .saturating_add(1);
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
        }
    }

    fn prepare_native_allocation(
        memory: &mut GuestAddressSpace,
        ptr: u32,
        required: u32,
        clear: bool,
    ) -> bool {
        let fully_mapped =
            (0..required).all(|offset| PpcMemory::read_u8(memory, ptr + offset).is_some());
        if !fully_mapped {
            let Ok(required) = usize::try_from(required) else {
                return false;
            };
            memory.add_region(ptr, vec![0; required]);
            return true;
        }
        if !memory.preflight_writable_range(ptr, required) {
            return false;
        }
        if clear {
            for offset in 0..required {
                PpcMemory::write_u8(memory, ptr + offset, 0)
                    .expect("preflighted native allocation remains writable");
            }
        }
        true
    }

    /// Reserve process-owned native heap bytes for Toolbox records that are
    /// not exposed as caller-disposable pointers.
    pub(crate) fn reserve_native_bytes(
        &mut self,
        memory: &mut GuestAddressSpace,
        size: u32,
        clear: bool,
    ) -> u32 {
        let Some(required) = Self::native_allocation_size(size) else {
            return 0;
        };
        let Some(heap) = self.native_heap_state() else {
            return 0;
        };
        let allocation_limit = self.native_allocation_limit(heap.heap_limit);
        let Some((ptr, next)) = Self::native_allocation_bounds(
            heap.heap_cursor,
            allocation_limit,
            required,
            |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
        ) else {
            return 0;
        };
        if !Self::prepare_native_allocation(memory, ptr, required, clear) {
            return 0;
        }
        let allocator = self
            .native_allocator
            .as_mut()
            .expect("native allocator remains registered");
        allocator.heap.heap_cursor = next;
        self.native_allocator_dirty = true;
        ptr
    }

    /// Borrow unmapped tail space for a non-reentrant native import scratch buffer.
    ///
    /// The cursor is intentionally unchanged: the caller must consume the
    /// bytes before another process allocation occurs and must not publish the
    /// address to guest code.
    pub(crate) fn native_scratch_bytes(
        &mut self,
        memory: &mut GuestAddressSpace,
        size: u32,
        clear: bool,
    ) -> u32 {
        let Some(required) = Self::native_allocation_size(size) else {
            return 0;
        };
        let Some(heap) = self.native_heap_state() else {
            return 0;
        };
        let allocation_limit = self.native_allocation_limit(heap.heap_limit);
        let Some((ptr, _)) = Self::native_allocation_bounds(
            heap.heap_cursor,
            allocation_limit,
            required,
            |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
        ) else {
            return 0;
        };
        Self::prepare_native_allocation(memory, ptr, required, clear)
            .then_some(ptr)
            .unwrap_or(0)
    }

    /// Commit a validated CFM mapping layout to the process-owned native heap.
    ///
    /// Dynamic PEF sections choose their individual alignment before they are
    /// installed, so the loader validates the complete sparse layout first and
    /// advances the canonical cursor only after every section has been mapped.
    pub(crate) fn commit_native_heap_cursor(&mut self, heap_cursor: u32) -> bool {
        let Some(allocator) = self.native_allocator.as_mut() else {
            return false;
        };
        if heap_cursor < allocator.heap.heap_cursor || heap_cursor >= allocator.heap.heap_limit {
            return false;
        }
        allocator.heap.heap_cursor = heap_cursor;
        self.native_allocator_dirty = true;
        true
    }

    /// Allocate a native nonrelocatable block in the process heap.
    ///
    /// `NewPtr` reserves fixed storage and `DisposePtr` returns it to the
    /// application heap. Inside Macintosh: Memory (1992), pp. 2-42--2-44.
    pub(crate) fn new_native_ptr(
        &mut self,
        memory: &mut GuestAddressSpace,
        size: u32,
        clear: bool,
    ) -> u32 {
        let Some(required) = Self::native_allocation_size(size) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return 0;
        };
        let Some(allocator) = self.native_allocator.as_ref() else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return 0;
        };
        let allocation_limit = self.native_allocation_limit(allocator.heap.heap_limit);
        let reusable_index = allocator
            .free_ptr_blocks
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                let capacity = Self::native_allocation_size(record.size)?;
                (capacity >= required
                    && record
                        .ptr
                        .checked_add(capacity)
                        .is_some_and(|end| end <= allocation_limit))
                .then_some((index, capacity))
            })
            .min_by_key(|(_, capacity)| *capacity)
            .map(|(index, _)| index);
        let allocation = if let Some(index) = reusable_index {
            Some((allocator.free_ptr_blocks[index].ptr, None))
        } else {
            Self::native_allocation_bounds(
                allocator.heap.heap_cursor,
                allocation_limit,
                required,
                |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
            )
            .map(|(ptr, next)| (ptr, Some(next)))
        };
        let Some((ptr, next_cursor)) = allocation else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return 0;
        };

        if !Self::prepare_native_allocation(memory, ptr, required, clear) {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return 0;
        }

        let allocator = self
            .native_allocator
            .as_mut()
            .expect("native allocator remains registered");
        if let Some(index) = reusable_index {
            allocator.free_ptr_blocks.swap_remove(index);
        }
        if let Some(next_cursor) = next_cursor {
            allocator.heap.heap_cursor = next_cursor;
        }
        allocator.ptrs.push(ProcessPtrRecord { ptr, size });
        allocator.heap.last_mem_error = Self::NO_ERR;
        self.native_allocator_dirty = true;
        ptr
    }

    pub(crate) fn dispose_native_ptr(&mut self, ptr: u32) -> Option<ProcessPtrRecord> {
        let mut disposed = None;
        if let Some(allocator) = &mut self.native_allocator {
            if let Some(index) = allocator.ptrs.iter().position(|record| record.ptr == ptr) {
                let record = allocator.ptrs.remove(index);
                allocator.free_ptr_blocks.push(record);
                disposed = Some(record);
            }
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
        }
        disposed
    }

    /// Replace native fixed storage while preserving its existing bytes.
    ///
    /// StdCLib `realloc` may move a block, unlike the Memory Manager's
    /// in-place `SetPtrSize`. Keep the old allocation live until the new
    /// storage and byte copy both succeed so a failed replacement is atomic.
    pub(crate) fn reallocate_native_ptr(
        &mut self,
        memory: &mut GuestAddressSpace,
        ptr: u32,
        size: u32,
    ) -> u32 {
        if ptr == 0 {
            return self.new_native_ptr(memory, size, false);
        }
        let Some(record) = self.native_allocator.as_ref().and_then(|allocator| {
            allocator
                .ptrs
                .iter()
                .find(|record| record.ptr == ptr)
                .copied()
        }) else {
            self.set_native_mem_error(Self::PARAM_ERR);
            return 0;
        };
        if size == 0 {
            let _ = self.dispose_native_ptr(ptr);
            return 0;
        }
        let copy_size = record.size.min(size);
        let Some(bytes) = (0..copy_size)
            .map(|offset| PpcMemory::read_u8(memory, ptr + offset))
            .collect::<Option<Vec<_>>>()
        else {
            self.set_native_mem_error(Self::PARAM_ERR);
            return 0;
        };

        let snapshot = self.detached_clone();
        let replacement = self.new_native_ptr(memory, size, false);
        if replacement == 0 {
            return 0;
        }
        if memory.write_bytes(replacement, &bytes).is_none() {
            self.restore_native_snapshot(snapshot);
            self.set_native_mem_error(Self::PARAM_ERR);
            return 0;
        }
        let _ = self.dispose_native_ptr(ptr);
        replacement
    }

    /// Reclaim a contiguous tail allocation from the native process heap.
    ///
    /// Composite Toolbox objects can own both fixed blocks and relocatable
    /// blocks allocated immediately before them. Once their guest-visible
    /// records are disposed, returning the whole contiguous tail prevents
    /// adapter-local cursor and free-list surgery. `DisposeGWorld` uses this
    /// for its pixel image, PixMap, port, and owned color table. Imaging With
    /// QuickDraw (1994), p. 6-25.
    pub(crate) fn reclaim_native_heap_tail(
        &mut self,
        reclaim_base: u32,
        disposed_ptrs: &[u32],
        disposed_handle: Option<u32>,
    ) -> bool {
        let Some(allocator) = self.native_allocator.as_ref() else {
            return false;
        };
        let allocation_crosses_base = |ptr: u32, size: u32| {
            ptr < reclaim_base
                && Self::native_allocation_size(size)
                    .and_then(|size| ptr.checked_add(size))
                    .is_some_and(|end| end > reclaim_base)
        };
        if reclaim_base < allocator.heap.heap_base
            || reclaim_base > allocator.heap.heap_cursor
            || disposed_ptrs
                .iter()
                .any(|ptr| !allocator.ptrs.iter().any(|record| record.ptr == *ptr))
            || allocator.ptrs.iter().any(|record| {
                (record.ptr >= reclaim_base && !disposed_ptrs.contains(&record.ptr))
                    || allocation_crosses_base(record.ptr, record.size)
            })
            || allocator
                .free_ptr_blocks
                .iter()
                .any(|record| allocation_crosses_base(record.ptr, record.size))
            || self.native_allocations.iter().any(|record| {
                record.handle >= reclaim_base
                    || record.ptr >= reclaim_base
                    || allocation_crosses_base(record.handle, 4)
                    || allocation_crosses_base(record.ptr, record.capacity)
            })
            || disposed_handle.is_some_and(|handle| {
                !allocator
                    .free_handle_blocks
                    .iter()
                    .any(|record| record.handle == handle)
            })
        {
            return false;
        }
        let allocator = self
            .native_allocator
            .as_mut()
            .expect("native allocator remains registered");
        allocator
            .ptrs
            .retain(|record| !disposed_ptrs.contains(&record.ptr));
        allocator
            .free_ptr_blocks
            .retain(|record| record.ptr < reclaim_base);
        allocator.free_handle_blocks.retain_mut(|record| {
            if record.handle >= reclaim_base {
                false
            } else {
                if record.ptr >= reclaim_base {
                    record.ptr = 0;
                    record.size = 0;
                    record.capacity = 0;
                }
                true
            }
        });
        allocator.heap.heap_cursor = reclaim_base;
        allocator.heap.last_mem_error = Self::NO_ERR;
        self.native_allocator_dirty = true;
        true
    }

    pub(crate) fn native_ptr_size(&mut self, ptr: u32) -> u32 {
        let size = self
            .native_allocator
            .as_ref()
            .and_then(|allocator| allocator.ptrs.iter().find(|record| record.ptr == ptr))
            .map_or(0, |record| record.size);
        self.set_native_mem_error(if size == 0 {
            Self::PARAM_ERR
        } else {
            Self::NO_ERR
        });
        size
    }

    /// Return a fixed block's logical size for a native import, including a
    /// classic block owned by an attached 68K Memory Manager. Native imports
    /// have no `MacMemoryBus` parameter, so this lookup deliberately consults
    /// only process-owned metadata. Inside Macintosh: Memory (1992),
    /// pp. 2-41--2-44.
    pub(crate) fn process_ptr_size_for_native_import(&mut self, ptr: u32) -> u32 {
        if let Some(size) = self.native_allocator.as_ref().and_then(|allocator| {
            allocator
                .ptrs
                .iter()
                .find(|record| record.ptr == ptr)
                .map(|record| record.size)
        }) {
            self.set_native_mem_error(if size == 0 {
                Self::PARAM_ERR
            } else {
                Self::NO_ERR
            });
            return size;
        }
        if let Some(size) = self
            .classic_allocator
            .as_ref()
            .and_then(|allocator| allocator.allocation_size(ptr))
        {
            self.set_native_mem_error(Self::NO_ERR);
            return size;
        }
        self.set_native_mem_error(Self::PARAM_ERR);
        0
    }

    fn copy_bytes_to_new_classic_handle_from_native_import(
        &mut self,
        memory: &mut GuestAddressSpace,
        bytes: &[u8],
    ) -> Result<u32, i16> {
        let Ok(size) = u32::try_from(bytes.len()) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        };
        let Some(allocator) = self.classic_allocator.clone() else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        };
        let allocator_before = allocator.0.borrow().clone();
        let ptr = allocator.allocate(size, 4, self.classic_heap_ceiling());
        if ptr == 0 && size > 0 {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        }
        let handle = allocator.allocate(4, 4, self.classic_heap_ceiling());
        if handle == 0 {
            *allocator.0.borrow_mut() = allocator_before;
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        }

        if (size > 0 && !memory.preflight_writable_range(ptr, size))
            || !memory.preflight_writable_range(handle, 4)
        {
            *allocator.0.borrow_mut() = allocator_before;
            self.set_native_mem_error(Self::PARAM_ERR);
            return Err(Self::PARAM_ERR);
        }
        let mut data_before = vec![0; bytes.len()];
        let mut master_before = [0; 4];
        if (size > 0 && memory.read_bytes_into(ptr, &mut data_before).is_none())
            || memory
                .read_bytes_into(handle, &mut master_before)
                .is_none()
        {
            *allocator.0.borrow_mut() = allocator_before;
            self.set_native_mem_error(Self::PARAM_ERR);
            return Err(Self::PARAM_ERR);
        }
        if (size > 0 && memory.write_bytes(ptr, bytes).is_none())
            || memory.write_bytes(handle, &ptr.to_be_bytes()).is_none()
        {
            if size > 0 {
                let _ = memory.write_bytes(ptr, &data_before);
            }
            let _ = memory.write_bytes(handle, &master_before);
            *allocator.0.borrow_mut() = allocator_before;
            self.set_native_mem_error(Self::PARAM_ERR);
            return Err(Self::PARAM_ERR);
        }

        self.ptr_to_handle.insert(ptr, handle);
        self.set_state_for_handle(handle, 0);
        self.set_native_mem_error(Self::NO_ERR);
        Ok(handle)
    }

    /// Copy a native or classic relocatable block through the process address
    /// space used by native imports.
    ///
    /// The source is validated and read completely before allocating the new
    /// handle in the source allocator. The copy is unlocked, unpurgeable, and
    /// not a resource. Inside Macintosh: Memory (1992), pp. 2-62--2-64.
    pub(crate) fn copy_process_handle_from_native_import(
        &mut self,
        memory: &mut GuestAddressSpace,
        handle: u32,
    ) -> Result<u32, i16> {
        if handle == 0 {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        }

        let (ptr, size, capacity, source_is_native) =
            if let Some(record) = self.native_allocation(handle) {
                let Some(master_ptr) = PpcMemory::read_u32_be(memory, handle) else {
                    self.set_native_mem_error(Self::NIL_HANDLE_ERR);
                    return Err(Self::NIL_HANDLE_ERR);
                };
                if master_ptr == 0 {
                    self.set_native_mem_error(Self::NIL_HANDLE_ERR);
                    return Err(Self::NIL_HANDLE_ERR);
                }
                if master_ptr != record.ptr
                    || self.ptr_to_handle.get(&master_ptr) != Some(handle)
                    || record.size > record.capacity
                {
                    self.set_native_mem_error(Self::MEM_WZ_ERR);
                    return Err(Self::MEM_WZ_ERR);
                }
                (master_ptr, record.size, record.capacity, true)
            } else {
                let Some(allocator) = self.classic_allocator.clone() else {
                    self.set_native_mem_error(Self::NIL_HANDLE_ERR);
                    return Err(Self::NIL_HANDLE_ERR);
                };
                if allocator.allocation_size(handle) != Some(4) {
                    self.set_native_mem_error(Self::NIL_HANDLE_ERR);
                    return Err(Self::NIL_HANDLE_ERR);
                }
                let Some(master_ptr) = PpcMemory::read_u32_be(memory, handle) else {
                    self.set_native_mem_error(Self::NIL_HANDLE_ERR);
                    return Err(Self::NIL_HANDLE_ERR);
                };
                if master_ptr == 0 {
                    self.set_native_mem_error(Self::NIL_HANDLE_ERR);
                    return Err(Self::NIL_HANDLE_ERR);
                }
                let Some(size) = allocator.allocation_size(master_ptr) else {
                    self.set_native_mem_error(Self::MEM_WZ_ERR);
                    return Err(Self::MEM_WZ_ERR);
                };
                let Some(capacity) = allocator.allocation_capacity(master_ptr) else {
                    self.set_native_mem_error(Self::MEM_WZ_ERR);
                    return Err(Self::MEM_WZ_ERR);
                };
                if self.ptr_to_handle.get(&master_ptr) != Some(handle)
                    || size > capacity
                    || master_ptr == handle
                    || handle.checked_add(4).is_some_and(|master_end| {
                        master_ptr < master_end
                            && handle < master_ptr.saturating_add(capacity)
                    })
                {
                    self.set_native_mem_error(Self::MEM_WZ_ERR);
                    return Err(Self::MEM_WZ_ERR);
                }
                (master_ptr, size, capacity, false)
            };

        let Ok(byte_count) = usize::try_from(size) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        };
        let mut bytes = vec![0; byte_count];
        if size > capacity || memory.read_bytes_into(ptr, &mut bytes).is_none() {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Err(Self::PARAM_ERR);
        }

        let copy = if source_is_native {
            let copy = self.copy_bytes_to_new_native_handle(memory, &bytes);
            if copy == 0 {
                return Err(self
                    .native_heap_state()
                    .map(|heap| heap.last_mem_error)
                    .unwrap_or(Self::MEM_FULL_ERR));
            }
            copy
        } else {
            self.copy_bytes_to_new_classic_handle_from_native_import(memory, &bytes)?
        };
        self.set_native_mem_error(Self::NO_ERR);
        Ok(copy)
    }

    /// Resize a native or classic fixed block from a native import without
    /// moving its guest address. Classic allocations retain their physical
    /// bucket capacity even when their logical size shrinks. Inside
    /// Macintosh: Memory (1992), pp. 2-42--2-44.
    pub(crate) fn set_process_ptr_size_for_native_import(
        &mut self,
        memory: &mut GuestAddressSpace,
        ptr: u32,
        new_size: u32,
    ) -> i16 {
        if self
            .native_allocator
            .as_ref()
            .is_some_and(|allocator| allocator.ptrs.iter().any(|record| record.ptr == ptr))
        {
            return self.set_native_ptr_size(memory, ptr, new_size);
        }

        let Some(allocator) = self.classic_allocator.clone() else {
            self.set_native_mem_error(Self::MEM_WZ_ERR);
            return Self::MEM_WZ_ERR;
        };
        let Some(old_size) = allocator.allocation_size(ptr) else {
            self.set_native_mem_error(Self::MEM_WZ_ERR);
            return Self::MEM_WZ_ERR;
        };
        let capacity = allocator
            .allocation_capacity(ptr)
            .expect("classic pointer retains its allocation capacity");
        if SharedClassicHeapAllocator::allocation_bucket_size(new_size) > capacity {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        }

        if new_size < old_size {
            let Some(tail) = ptr.checked_add(new_size) else {
                self.set_native_mem_error(Self::PARAM_ERR);
                return Self::PARAM_ERR;
            };
            let Ok(tail_len) = usize::try_from(old_size - new_size) else {
                self.set_native_mem_error(Self::PARAM_ERR);
                return Self::PARAM_ERR;
            };
            if memory.write_bytes(tail, &vec![0; tail_len]).is_none() {
                self.set_native_mem_error(Self::PARAM_ERR);
                return Self::PARAM_ERR;
            }
        }

        allocator.set_allocation_size(ptr, new_size);
        self.set_native_mem_error(Self::NO_ERR);
        Self::NO_ERR
    }

    /// Dispose a classic fixed block from a native import when the process
    /// manager owns the classic allocator. The guest bytes remain in the bus
    /// mapping; only the process-owned allocation metadata moves to its free
    /// list, matching `DisposePtr`'s classic semantics. Inside Macintosh:
    /// Memory (1992), pp. 2-38--2-39.
    pub(crate) fn dispose_classic_ptr_from_native_import(&mut self, ptr: u32) -> bool {
        let Some(allocator) = self.classic_allocator.as_ref() else {
            return false;
        };
        if allocator.allocation_size(ptr).is_none() {
            return false;
        }
        allocator.free(ptr);
        self.set_native_mem_error(Self::NO_ERR);
        true
    }

    /// Empty a native or classic relocatable block from a native import.
    ///
    /// The live master pointer is read and cleared through the process address
    /// space before allocator metadata is committed, so a failed guest write
    /// leaves the allocation intact. The stable master-pointer block and its
    /// state bits remain allocated. Inside Macintosh: Memory (1992),
    /// pp. 2-51--2-52.
    pub(crate) fn empty_process_handle_from_native_import(
        &mut self,
        memory: &mut GuestAddressSpace,
        handle: u32,
    ) -> i16 {
        if self.native_allocation(handle).is_some() {
            return self.empty_native_handle(memory, handle);
        }

        let Some(allocator) = self.classic_allocator.clone() else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        };
        if handle == 0 || allocator.allocation_size(handle) != Some(4) {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }
        if self.state_for_handle(handle).unwrap_or(0) & 0x20 != 0 {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }
        if self.state_for_handle(handle).unwrap_or(0) & 0x80 != 0 {
            self.set_native_mem_error(Self::MEM_PUR_ERR);
            return Self::MEM_PUR_ERR;
        }
        let Some(ptr) = PpcMemory::read_u32_be(memory, handle) else {
            self.set_native_mem_error(Self::MEM_WZ_ERR);
            return Self::MEM_WZ_ERR;
        };
        if ptr != 0 && allocator.allocation_size(ptr).is_none() {
            self.set_native_mem_error(Self::MEM_WZ_ERR);
            return Self::MEM_WZ_ERR;
        }
        if PpcMemory::write_u32_be(memory, handle, 0).is_none() {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }

        if ptr != 0 {
            allocator.free(ptr);
            self.ptr_to_handle.remove(&ptr);
        }
        self.set_native_mem_error(Self::NO_ERR);
        Self::NO_ERR
    }

    /// Dispose a native or classic relocatable block from a native import.
    ///
    /// Classic disposal releases both allocator records without rewriting the
    /// freed master-pointer bytes. The stale reverse entry is intentionally
    /// retained until validated `RecoverHandle` observes reuse of that slot,
    /// preserving existing invalid-handle compatibility. Inside Macintosh:
    /// Memory (1992), pp. 2-34--2-35 and 2-53--2-54.
    pub(crate) fn dispose_process_handle_from_native_import(
        &mut self,
        memory: &mut GuestAddressSpace,
        handle: u32,
    ) -> bool {
        if self.native_allocation(handle).is_some() {
            return self.dispose_native_handle(memory, handle).is_some();
        }

        let Some(allocator) = self.classic_allocator.clone() else {
            self.set_native_mem_error(Self::NO_ERR);
            return false;
        };
        if handle == 0 || allocator.allocation_size(handle) != Some(4) {
            self.set_native_mem_error(Self::NO_ERR);
            return false;
        }
        if self.state_for_handle(handle).unwrap_or(0) & 0x20 != 0 {
            self.set_native_mem_error(Self::NO_ERR);
            return false;
        }
        let Some(ptr) = PpcMemory::read_u32_be(memory, handle) else {
            self.set_native_mem_error(Self::MEM_WZ_ERR);
            return false;
        };
        if ptr != 0 && allocator.allocation_size(ptr).is_none() {
            self.set_native_mem_error(Self::MEM_WZ_ERR);
            return false;
        }

        allocator.free(ptr);
        allocator.free(handle);
        self.handle_state_bits.remove(&handle);
        self.handle_high_locked.remove(&handle);
        self.set_native_mem_error(Self::NO_ERR);
        true
    }

    /// Change the logical size of a native nonrelocatable block in place.
    ///
    /// A nonrelocatable block cannot move, so growth can fail when another
    /// block occupies the following address range. Inside Macintosh: Memory
    /// (1992), pp. 2-42--2-43.
    pub(crate) fn set_native_ptr_size(
        &mut self,
        memory: &mut GuestAddressSpace,
        ptr: u32,
        size: u32,
    ) -> i16 {
        let Some(record) = self.native_allocator.as_ref().and_then(|allocator| {
            allocator
                .ptrs
                .iter()
                .find(|record| record.ptr == ptr)
                .copied()
        }) else {
            self.set_native_mem_error(Self::MEM_WZ_ERR);
            return Self::MEM_WZ_ERR;
        };
        if size <= record.size {
            let allocator = self
                .native_allocator
                .as_mut()
                .expect("native allocator remains registered");
            allocator
                .ptrs
                .iter_mut()
                .find(|record| record.ptr == ptr)
                .expect("native pointer remains registered")
                .size = size;
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
            return Self::NO_ERR;
        }

        let Some(old_capacity) = Self::native_allocation_size(record.size) else {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        };
        let Some(new_capacity) = Self::native_allocation_size(size) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let Some(old_end) = record.ptr.checked_add(old_capacity) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let Some(heap) = self.native_heap_state() else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        if old_end != heap.heap_cursor {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        }
        let allocation_limit = self.native_allocation_limit(heap.heap_limit);
        let Some((resize_ptr, new_end)) = Self::native_allocation_bounds(
            record.ptr,
            allocation_limit,
            new_capacity,
            |base, len| memory.readonly_allocation_overlap_end(base, len),
        ) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        if resize_ptr != record.ptr {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        }
        if new_end > old_end && PpcMemory::read_u8(memory, old_end).is_none() {
            let Ok(growth) = usize::try_from(new_end - old_end) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Self::MEM_FULL_ERR;
            };
            memory.add_region(old_end, vec![0; growth]);
        }
        if (old_end..new_end).any(|address| PpcMemory::write_u8(memory, address, 0).is_none()) {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        }

        let allocator = self
            .native_allocator
            .as_mut()
            .expect("native allocator remains registered");
        allocator
            .ptrs
            .iter_mut()
            .find(|record| record.ptr == ptr)
            .expect("native pointer remains registered")
            .size = size;
        allocator.heap.heap_cursor = new_end;
        allocator.heap.last_mem_error = Self::NO_ERR;
        self.native_allocator_dirty = true;
        Self::NO_ERR
    }

    /// Recover the stable handle whose relocatable block starts at `ptr`.
    /// Inside Macintosh: Memory (1992), pp. 2-54--2-55.
    #[cfg(test)]
    pub(crate) fn recover_handle(&self, ptr: u32) -> Option<u32> {
        self.ptr_to_handle.get(&ptr)
    }

    /// Recover a handle only while its live master-pointer slot still names
    /// `ptr`. The reverse map is an index for the Memory Manager's conceptual
    /// master-pointer-table scan, so a reused or guest-mutated slot invalidates
    /// its cached entry. Inside Macintosh Volume V (1986), p. V-579.
    pub(crate) fn recover_handle_from_master_pointer(
        &self,
        ptr: u32,
        read_master_pointer: impl FnOnce(u32) -> Option<u32>,
    ) -> Option<u32> {
        let handle = self.ptr_to_handle.get(&ptr)?;
        if read_master_pointer(handle) == Some(ptr) {
            return Some(handle);
        }
        if self.ptr_to_handle.get(&ptr) == Some(handle) {
            self.ptr_to_handle.remove(&ptr);
        }
        None
    }

    /// Allocate a native relocatable block and its stable master pointer.
    ///
    /// A handle addresses a nonrelocatable master pointer whose contents may
    /// change when the relocatable block moves. Inside Macintosh: Memory
    /// (1992), pp. 1-18--1-19 and 2-40--2-41.
    pub(crate) fn new_native_handle(
        &mut self,
        memory: &mut GuestAddressSpace,
        size: u32,
        clear: bool,
    ) -> u32 {
        let Some(required) = Self::native_allocation_size(size) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return 0;
        };
        let Some(allocator) = self.native_allocator.as_ref() else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return 0;
        };
        let allocation_limit = self.native_allocation_limit(allocator.heap.heap_limit);
        let reusable_handle_index = allocator
            .free_handle_blocks
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                let handle_capacity = Self::native_allocation_size(4)?;
                let handle_fits = record
                    .handle
                    .checked_add(handle_capacity)
                    .is_some_and(|end| end <= allocation_limit);
                if record.ptr == 0 {
                    return handle_fits.then_some((index, 0));
                }
                let capacity = Self::native_allocation_size(record.capacity)?;
                (handle_fits
                    && capacity >= required
                    && record
                        .ptr
                        .checked_add(capacity)
                        .is_some_and(|end| end <= allocation_limit))
                .then_some((index, capacity))
            })
            .filter(|(_, capacity)| *capacity != 0)
            .min_by_key(|(_, capacity)| *capacity)
            .map(|(index, _)| index)
            .or_else(|| {
                allocator
                    .free_handle_blocks
                    .iter()
                    .enumerate()
                    .find(|(_, record)| {
                        record.ptr == 0
                            && record
                                .handle
                                .checked_add(Self::native_allocation_size(4).unwrap_or(0))
                                .is_some_and(|end| end <= allocation_limit)
                    })
                    .map(|(index, _)| index)
            });
        let mut reusable_ptr_index = None;
        let (record, next_cursor) = if let Some(index) = reusable_handle_index {
            let mut record = allocator.free_handle_blocks[index];
            let mut next_cursor = None;
            if record.ptr == 0 {
                reusable_ptr_index = allocator
                    .free_ptr_blocks
                    .iter()
                    .enumerate()
                    .filter_map(|(index, record)| {
                        let capacity = Self::native_allocation_size(record.size)?;
                        (capacity >= required
                            && record
                                .ptr
                                .checked_add(capacity)
                                .is_some_and(|end| end <= allocation_limit))
                        .then_some((index, capacity))
                    })
                    .min_by_key(|(_, capacity)| *capacity)
                    .map(|(index, _)| index);
                if let Some(index) = reusable_ptr_index {
                    record.ptr = allocator.free_ptr_blocks[index].ptr;
                } else {
                    let Some((ptr, next)) = Self::native_allocation_bounds(
                        allocator.heap.heap_cursor,
                        allocation_limit,
                        required,
                        |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
                    ) else {
                        self.set_native_mem_error(Self::MEM_FULL_ERR);
                        return 0;
                    };
                    record.ptr = ptr;
                    next_cursor = Some(next);
                }
                record.capacity = size;
            }
            record.size = size;
            (record, next_cursor)
        } else {
            let Some(handle_required) = Self::native_allocation_size(4) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return 0;
            };
            let Some((handle, after_handle)) = Self::native_allocation_bounds(
                allocator.heap.heap_cursor,
                allocation_limit,
                handle_required,
                |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
            ) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return 0;
            };
            let Some((ptr, after_ptr)) = Self::native_allocation_bounds(
                after_handle,
                allocation_limit,
                required,
                |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
            ) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return 0;
            };
            (
                ProcessHandleRecord {
                    handle,
                    ptr,
                    size,
                    capacity: size,
                },
                Some(after_ptr),
            )
        };

        if !Self::prepare_native_allocation(
            memory,
            record.handle,
            Self::native_allocation_size(4).expect("four-byte master pointer fits"),
            true,
        ) || !Self::prepare_native_allocation(memory, record.ptr, required, clear)
            || PpcMemory::write_u32_be(memory, record.handle, record.ptr).is_none()
        {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return 0;
        }

        let allocator = self
            .native_allocator
            .as_mut()
            .expect("native allocator remains registered");
        if let Some(index) = reusable_handle_index {
            allocator.free_handle_blocks.swap_remove(index);
        }
        if let Some(index) = reusable_ptr_index {
            allocator.free_ptr_blocks.swap_remove(index);
        }
        if let Some(next_cursor) = next_cursor {
            allocator.heap.heap_cursor = next_cursor;
        }
        allocator.heap.last_mem_error = Self::NO_ERR;
        self.set_native_allocation_record(record);
        self.ptr_to_handle.insert(record.ptr, record.handle);
        self.native_handle_ptrs.insert(record.ptr);
        self.handle_state_bits.insert(record.handle, 0x40);
        self.native_handles.insert(record.handle);
        self.native_allocator_dirty = true;
        record.handle
    }

    /// Allocate a native relocatable block containing a copy of `bytes`.
    ///
    /// `PtrToHand` and `HandToHand` both create a new relocatable block and
    /// copy existing bytes into it. Inside Macintosh: Memory (1992),
    /// pp. 2-60--2-63.
    pub(crate) fn copy_bytes_to_new_native_handle(
        &mut self,
        memory: &mut GuestAddressSpace,
        bytes: &[u8],
    ) -> u32 {
        let Ok(size) = u32::try_from(bytes.len()) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return 0;
        };
        let handle = self.new_native_handle(memory, size, false);
        let Some(record) = self.native_allocation(handle) else {
            return 0;
        };
        memory
            .write_bytes(record.ptr, bytes)
            .expect("allocated native handle storage remains writable");
        self.set_state_for_handle(handle, 0);
        handle
    }

    /// Materialize a native Resource Manager handle in the process heap.
    ///
    /// When resource loading is disabled, the stable master pointer is
    /// allocated immediately while its relocatable block remains `NIL`.
    /// Resource handles are purgeable and carry the resource bit. Inside
    /// Macintosh Volume I (1985), pp. I-118--I-120, and Inside Macintosh:
    /// Memory (1992), pp. 2-46--2-51.
    pub(crate) fn new_native_resource_handle(
        &mut self,
        memory: &mut GuestAddressSpace,
        bytes: Option<&[u8]>,
    ) -> u32 {
        let handle = if let Some(bytes) = bytes {
            self.copy_bytes_to_new_native_handle(memory, bytes)
        } else {
            let handle = self.new_native_handle(memory, 0, true);
            if handle != 0 && self.empty_native_handle(memory, handle) != Self::NO_ERR {
                let _ = self.dispose_native_handle(memory, handle);
                return 0;
            }
            handle
        };
        if handle != 0 {
            self.set_process_handle_purgeable(handle, true);
            self.set_process_handle_resource(handle, true);
        }
        handle
    }

    /// Populate an empty native Resource Manager handle without changing its
    /// stable master pointer. The allocation record, reverse handle index, and
    /// guest bytes become visible as one process-owned transaction. Inside
    /// Macintosh Volume I (1985), pp. I-118--I-120.
    pub(crate) fn load_native_resource_handle(
        &mut self,
        memory: &mut GuestAddressSpace,
        handle: u32,
        bytes: &[u8],
    ) -> i16 {
        let Some(record) = self.native_allocation(handle) else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        };
        if record.ptr != 0 {
            self.set_process_handle_purgeable(handle, true);
            self.set_process_handle_resource(handle, true);
            self.set_native_mem_error(Self::NO_ERR);
            return Self::NO_ERR;
        }
        let Ok(size) = u32::try_from(bytes.len()) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let result = self.set_native_handle_size(memory, handle, size);
        if result != Self::NO_ERR {
            return result;
        }
        let Some(updated) = self.native_allocation(handle) else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        };
        if bytes.iter().copied().enumerate().any(|(offset, byte)| {
            PpcMemory::write_u8(memory, updated.ptr + offset as u32, byte).is_none()
        }) {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        }
        self.set_process_handle_purgeable(handle, true);
        self.set_process_handle_resource(handle, true);
        self.set_native_mem_error(Self::NO_ERR);
        Self::NO_ERR
    }

    /// Publish a resource block referenced by a master pointer in an ordinary
    /// PEF mapping.
    ///
    /// The relocatable data consumes process heap space and participates in
    /// `RecoverHandle`, but the caller-owned master-pointer address must never
    /// enter the native handle free list. This preserves canonical PEF mapping
    /// priority while making its resource state immediately cross-ISA visible.
    pub(crate) fn publish_external_native_resource_handle(
        &mut self,
        handle: u32,
        ptr: u32,
        heap_cursor: u32,
    ) {
        if handle == 0 {
            return;
        }
        if ptr != 0 {
            self.ptr_to_handle.insert(ptr, handle);
        }
        self.set_process_handle_purgeable(handle, true);
        self.set_process_handle_resource(handle, true);
        if let Some(allocator) = &mut self.native_allocator {
            allocator.heap.heap_cursor = heap_cursor;
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
        }
    }

    /// Append bytes to a native relocatable block through its stable handle.
    ///
    /// `HandAndHand` leaves the source unchanged and grows the destination
    /// before appending the source bytes. Inside Macintosh: Memory (1992),
    /// pp. 2-64--2-65.
    pub(crate) fn append_bytes_to_native_handle(
        &mut self,
        memory: &mut GuestAddressSpace,
        handle: u32,
        bytes: &[u8],
    ) -> i16 {
        let Some(record) = self.native_allocation(handle) else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        };
        let Ok(byte_count) = u32::try_from(bytes.len()) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let Some(new_size) = record.size.checked_add(byte_count) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let result = self.set_native_handle_size(memory, handle, new_size);
        if result != Self::NO_ERR {
            return result;
        }
        let destination = self
            .native_allocation(handle)
            .expect("successful native handle resize remains registered");
        if bytes.iter().copied().enumerate().any(|(offset, byte)| {
            PpcMemory::write_u8(memory, destination.ptr + record.size + offset as u32, byte)
                .is_none()
        }) {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        }
        Self::NO_ERR
    }

    pub(crate) fn dispose_native_handle(
        &mut self,
        memory: &mut GuestAddressSpace,
        handle: u32,
    ) -> Option<ProcessHandleRecord> {
        let Some((index, record)) = self
            .native_allocations
            .iter()
            .copied()
            .enumerate()
            .find(|(_, record)| record.handle == handle)
        else {
            self.set_native_mem_error(Self::NO_ERR);
            return None;
        };
        if PpcMemory::write_u32_be(memory, handle, 0).is_none() {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return None;
        }
        self.commit_dispose_native_handle(index, record);
        Some(record)
    }

    /// Change a native or classic relocatable block's logical size through
    /// the process-owned native address space.
    ///
    /// Native records retain their existing allocator policy. Classic
    /// handles use the shared four-byte bucket allocator, preserving larger
    /// recycled capacities and the logical prefix while making relocation a
    /// single process-manager transaction. A locked classic handle may grow
    /// only while its current bucket can hold the request. Resource and
    /// externally-owned handles are rejected because their backing metadata
    /// is not part of the generic Memory Manager record. Inside Macintosh:
    /// Memory (1992), pp. 2-40--2-43, 2-46--2-51.
    pub(crate) fn set_process_handle_size_from_native_import(
        &mut self,
        memory: &mut GuestAddressSpace,
        handle: u32,
        new_size: u32,
    ) -> i16 {
        if self.native_allocation(handle).is_some() {
            return self.set_native_handle_size(memory, handle, new_size);
        }

        let Some(allocator) = self.classic_allocator.clone() else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        };
        if handle == 0 || allocator.allocation_size(handle) != Some(4) {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }

        let state = self.state_for_handle(handle).unwrap_or(0);
        if state & 0x20 != 0 {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }

        let Some(master_ptr) = PpcMemory::read_u32_be(memory, handle) else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        };
        let (old_size, old_capacity) = if master_ptr == 0 {
            (0, 0)
        } else {
            let Some(old_size) = allocator.allocation_size(master_ptr) else {
                self.set_native_mem_error(Self::MEM_WZ_ERR);
                return Self::MEM_WZ_ERR;
            };
            let Some(old_capacity) = allocator.allocation_capacity(master_ptr) else {
                self.set_native_mem_error(Self::MEM_WZ_ERR);
                return Self::MEM_WZ_ERR;
            };
            if self.ptr_to_handle.get(&master_ptr) != Some(handle)
                || old_size > old_capacity
                || master_ptr == handle
                || master_ptr
                    .checked_add(old_capacity)
                    .is_none()
                || handle
                    .checked_add(4)
                    .is_some_and(|master_end| {
                        master_ptr < master_end
                            && handle < master_ptr.saturating_add(old_capacity)
                    })
            {
                self.set_native_mem_error(Self::MEM_WZ_ERR);
                return Self::MEM_WZ_ERR;
            }
            (old_size, old_capacity)
        };

        let Some(new_capacity) = SharedClassicHeapAllocator::checked_allocation_bucket_size(
            new_size,
        ) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        // The master slot is part of the commit contract even when the
        // pointer remains stable. Write-back of identical bytes verifies that
        // an ordinary/PEF read-only mapping cannot be mistaken for a classic
        // master pointer without changing any guest-visible value.
        let master_bytes = master_ptr.to_be_bytes();
        if !memory.preflight_writable_range(handle, 4) {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }

        if master_ptr == 0 && new_size == 0 {
            self.set_native_mem_error(Self::NO_ERR);
            return Self::NO_ERR;
        }

        if new_capacity <= old_capacity && master_ptr != 0 {
            if !memory.preflight_writable_range(master_ptr, new_capacity)
                || (new_size < old_size
                    && !memory.preflight_writable_range(
                        master_ptr
                            .checked_add(new_size)
                            .expect("new size fits in the validated allocation"),
                        old_size - new_size,
                    ))
            {
                self.set_native_mem_error(Self::PARAM_ERR);
                return Self::PARAM_ERR;
            }
            if new_size < old_size {
                let tail = vec![0; (old_size - new_size) as usize];
                memory
                    .write_bytes(
                        master_ptr
                            .checked_add(new_size)
                            .expect("new size fits in the validated allocation"),
                        &tail,
                    )
                    .expect("the writable classic tail was preflighted");
            }
            allocator.set_allocation_size(master_ptr, new_size);
            self.set_native_mem_error(Self::NO_ERR);
            return Self::NO_ERR;
        }

        if state & 0x80 != 0 {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        }

        let Some(plan) = allocator.allocation_plan(
            new_size,
            4,
            self.classic_heap_ceiling(),
        ) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let new_ptr = plan.address;
        let new_capacity = plan.capacity();
        if new_ptr == handle
            || master_ptr
                .checked_add(old_capacity)
                .is_some_and(|old_end| {
                    new_ptr < old_end
                        && master_ptr < new_ptr.saturating_add(new_capacity)
                })
            || allocator.allocation_size(new_ptr).is_some()
            || self
                .ptr_to_handle
                .get(&new_ptr)
                .is_some_and(|mapped_handle| mapped_handle != handle)
        {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        }

        let copy_len = old_size.min(new_size);
        let Ok(copy_len) = usize::try_from(copy_len) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let mut prefix = vec![0; copy_len];
        if copy_len > 0 && memory.read_bytes_into(master_ptr, &mut prefix).is_none() {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        }

        if !memory.preflight_writable_range(new_ptr, new_capacity) {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        }
        let mut destination_before = vec![0; copy_len];
        if copy_len > 0 && memory.read_bytes_into(new_ptr, &mut destination_before).is_none() {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        }
        if memory.write_bytes(new_ptr, &prefix).is_none() {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        }

        if memory.write_bytes(handle, &new_ptr.to_be_bytes()).is_none() {
            if copy_len > 0 {
                let _ = memory.write_bytes(new_ptr, &destination_before);
            }
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }

        if !allocator.commit_allocation_plan(plan) {
            let _ = memory.write_bytes(handle, &master_bytes);
            if copy_len > 0 {
                let _ = memory.write_bytes(new_ptr, &destination_before);
            }
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        }
        if master_ptr != 0 {
            allocator.free(master_ptr);
            if self.ptr_to_handle.get(&master_ptr) == Some(handle) {
                self.ptr_to_handle.remove(&master_ptr);
            }
        }
        self.ptr_to_handle.insert(new_ptr, handle);
        self.set_native_mem_error(Self::NO_ERR);
        Self::NO_ERR
    }

    pub(crate) fn set_native_handle_size(
        &mut self,
        memory: &mut GuestAddressSpace,
        handle: u32,
        size: u32,
    ) -> i16 {
        let Some(mut record) = self.native_allocation(handle) else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        };
        if PpcMemory::read_u32_be(memory, handle) != Some(record.ptr) {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }
        if size <= record.capacity {
            record.size = size;
            self.set_native_allocation_record(record);
            self.set_native_mem_error(Self::NO_ERR);
            return Self::NO_ERR;
        }
        if record.ptr == 0 {
            let Some(required) = Self::native_allocation_size(size) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Self::MEM_FULL_ERR;
            };
            let Some(allocator) = self.native_allocator.as_ref() else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Self::MEM_FULL_ERR;
            };
            let allocation_limit = self.native_allocation_limit(allocator.heap.heap_limit);
            let reusable_ptr_index = allocator
                .free_ptr_blocks
                .iter()
                .enumerate()
                .filter_map(|(index, free)| {
                    let capacity = Self::native_allocation_size(free.size)?;
                    (capacity >= required
                        && free
                            .ptr
                            .checked_add(capacity)
                            .is_some_and(|end| end <= allocation_limit))
                    .then_some((index, capacity))
                })
                .min_by_key(|(_, capacity)| *capacity)
                .map(|(index, _)| index);
            let (new_ptr, next_cursor) = if let Some(index) = reusable_ptr_index {
                (allocator.free_ptr_blocks[index].ptr, None)
            } else {
                let Some((ptr, next)) = Self::native_allocation_bounds(
                    allocator.heap.heap_cursor,
                    allocation_limit,
                    required,
                    |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
                ) else {
                    self.set_native_mem_error(Self::MEM_FULL_ERR);
                    return Self::MEM_FULL_ERR;
                };
                (ptr, Some(next))
            };
            if !Self::prepare_native_allocation(memory, new_ptr, required, true)
                || PpcMemory::write_u32_be(memory, handle, new_ptr).is_none()
            {
                self.set_native_mem_error(Self::PARAM_ERR);
                return Self::PARAM_ERR;
            }
            record.ptr = new_ptr;
            record.size = size;
            record.capacity = size;
            self.set_native_allocation_record(record);
            self.ptr_to_handle.insert(new_ptr, handle);
            self.native_handle_ptrs.insert(new_ptr);
            let allocator = self
                .native_allocator
                .as_mut()
                .expect("native allocator remains registered");
            if let Some(index) = reusable_ptr_index {
                allocator.free_ptr_blocks.swap_remove(index);
            }
            if let Some(next_cursor) = next_cursor {
                allocator.heap.heap_cursor = next_cursor;
            }
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
            return Self::NO_ERR;
        }
        let Some(old_aligned) = Self::native_allocation_size(record.size) else {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        };
        let Some(new_aligned) = Self::native_allocation_size(size) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let Some(allocator) = self.native_allocator.as_ref() else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let allocation_limit = self.native_allocation_limit(allocator.heap.heap_limit);
        let can_extend_last = record.ptr.checked_add(old_aligned)
            == Some(allocator.heap.heap_cursor)
            && Self::native_allocation_bounds(
                record.ptr,
                allocation_limit,
                new_aligned,
                |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
            )
            .is_some_and(|(ptr, _)| ptr == record.ptr);
        let (new_ptr, next_cursor) = if can_extend_last {
            (record.ptr, record.ptr.checked_add(new_aligned))
        } else {
            let Some((ptr, next)) = Self::native_allocation_bounds(
                allocator.heap.heap_cursor,
                allocation_limit,
                new_aligned,
                |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
            ) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Self::MEM_FULL_ERR;
            };
            (ptr, Some(next))
        };
        let Some(next_cursor) = next_cursor else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Self::MEM_FULL_ERR;
        };
        let mut bytes = Vec::with_capacity(record.size as usize);
        for offset in 0..record.size {
            let Some(byte) = PpcMemory::read_u8(memory, record.ptr + offset) else {
                self.set_native_mem_error(Self::PARAM_ERR);
                return Self::PARAM_ERR;
            };
            bytes.push(byte);
        }
        if !Self::prepare_native_allocation(memory, new_ptr, new_aligned, true)
            || bytes.iter().copied().enumerate().any(|(offset, byte)| {
                PpcMemory::write_u8(memory, new_ptr + offset as u32, byte).is_none()
            })
            || (new_ptr != record.ptr && PpcMemory::write_u32_be(memory, handle, new_ptr).is_none())
        {
            self.set_native_mem_error(Self::PARAM_ERR);
            return Self::PARAM_ERR;
        }
        self.ptr_to_handle.remove(&record.ptr);
        self.native_handle_ptrs.remove(&record.ptr);
        record.ptr = new_ptr;
        record.size = size;
        record.capacity = size;
        self.set_native_allocation_record(record);
        self.ptr_to_handle.insert(new_ptr, handle);
        self.native_handle_ptrs.insert(new_ptr);
        let allocator = self
            .native_allocator
            .as_mut()
            .expect("native allocator remains registered");
        allocator.heap.heap_cursor = next_cursor;
        allocator.heap.last_mem_error = Self::NO_ERR;
        self.native_allocator_dirty = true;
        Self::NO_ERR
    }

    /// Replace a native relocatable block while its process address space is
    /// attached to the serialized 68K adapter.
    ///
    /// A handle remains stable while its master pointer may change when the
    /// block grows. Inside Macintosh: Memory (1992), pp. 1-18--1-19 and
    /// 2-40--2-41.
    pub(crate) fn replace_native_handle_bytes(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        expected_ptr: u32,
        bytes: &[u8],
    ) -> Result<(u32, u32), i16> {
        self.replace_native_handle_bytes_with_relocation(bus, handle, expected_ptr, bytes, false)
    }

    fn replace_native_handle_bytes_with_relocation(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        expected_ptr: u32,
        bytes: &[u8],
        force_relocation: bool,
    ) -> Result<(u32, u32), i16> {
        let Some(record) = self.native_allocation(handle) else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        };
        let current_ptr = bus.read_long(handle);
        if current_ptr != expected_ptr
            || record.ptr != current_ptr
            || (current_ptr == 0 && !force_relocation)
        {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        }
        let Ok(size) = u32::try_from(bytes.len()) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        };
        let Some(new_aligned) = Self::native_allocation_size(size) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        };
        let Some(allocator) = self.native_allocator.as_ref() else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        };
        let allocation_limit = self.native_allocation_limit(allocator.heap.heap_limit);

        let mut new_ptr = record.ptr;
        let mut new_cursor = allocator.heap.heap_cursor;
        let mut new_capacity = record.capacity;
        let mut recycled_ptr_index = None;
        if force_relocation {
            recycled_ptr_index = allocator
                .free_ptr_blocks
                .iter()
                .enumerate()
                .filter_map(|(index, free)| {
                    let capacity = Self::native_allocation_size(free.size)?;
                    (free.ptr != current_ptr
                        && capacity >= new_aligned
                        && free
                            .ptr
                            .checked_add(capacity)
                            .is_some_and(|end| end <= allocation_limit))
                        .then_some((index, capacity))
                })
                .min_by_key(|(_, capacity)| *capacity)
                .map(|(index, _)| index);
            if let Some(index) = recycled_ptr_index {
                new_ptr = allocator.free_ptr_blocks[index].ptr;
            } else {
                let Some((ptr, next)) = Self::native_allocation_bounds(
                    allocator.heap.heap_cursor,
                    allocation_limit,
                    new_aligned,
                    |ptr, len| bus.foreign_readonly_allocation_overlap_end(ptr, len),
                ) else {
                    self.set_native_mem_error(Self::MEM_FULL_ERR);
                    return Err(Self::MEM_FULL_ERR);
                };
                new_ptr = ptr;
                new_cursor = next;
            }
            new_capacity = size;
        } else if size > record.capacity {
            let Some(old_aligned) = Self::native_allocation_size(record.capacity) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Err(Self::MEM_FULL_ERR);
            };
            let can_extend_last = record.ptr.checked_add(old_aligned)
                == Some(allocator.heap.heap_cursor)
                && Self::native_allocation_bounds(
                    record.ptr,
                    allocation_limit,
                    new_aligned,
                    |ptr, len| bus.foreign_readonly_allocation_overlap_end(ptr, len),
                )
                .is_some_and(|(ptr, _)| ptr == record.ptr);
            if can_extend_last {
                new_cursor = record.ptr + new_aligned;
            } else {
                let Some((ptr, next)) = Self::native_allocation_bounds(
                    allocator.heap.heap_cursor,
                    allocation_limit,
                    new_aligned,
                    |ptr, len| bus.foreign_readonly_allocation_overlap_end(ptr, len),
                ) else {
                    self.set_native_mem_error(Self::MEM_FULL_ERR);
                    return Err(Self::MEM_FULL_ERR);
                };
                new_ptr = ptr;
                new_cursor = next;
            }
            new_capacity = size;
        }

        if bus.write_foreign_bytes(new_ptr, bytes).is_none()
            || (new_ptr != current_ptr
                && bus
                    .write_foreign_bytes(handle, &new_ptr.to_be_bytes())
                    .is_none())
        {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        }

        let updated = ProcessHandleRecord {
            handle,
            ptr: new_ptr,
            size,
            capacity: new_capacity,
        };
        self.set_native_allocation_record(updated);
        self.ptr_to_handle.remove(&current_ptr);
        self.ptr_to_handle.insert(new_ptr, handle);
        self.native_handle_ptrs.remove(&current_ptr);
        self.native_handle_ptrs.insert(new_ptr);
        let allocator = self
            .native_allocator
            .as_mut()
            .expect("native allocator remains registered");
        if let Some(index) = recycled_ptr_index {
            allocator.free_ptr_blocks.swap_remove(index);
        }
        if new_ptr != current_ptr && current_ptr != 0 {
            allocator.free_ptr_blocks.push(ProcessPtrRecord {
                ptr: current_ptr,
                size: record.capacity,
            });
        }
        allocator.heap.heap_cursor = new_cursor;
        allocator.heap.last_mem_error = Self::NO_ERR;
        self.native_allocator_dirty = true;
        Ok((current_ptr, new_ptr))
    }

    pub(crate) fn publish_native_allocator(
        &mut self,
        heap: ProcessNativeHeapState,
        ptrs: &[ProcessPtrRecord],
        free_ptr_blocks: &[ProcessPtrRecord],
        free_handle_blocks: &[ProcessHandleRecord],
    ) {
        let allocator = self
            .native_allocator
            .get_or_insert_with(|| ProcessNativeAllocatorState {
                initial_heap: heap,
                heap,
                ptrs: Vec::new(),
                free_ptr_blocks: Vec::new(),
                free_handle_blocks: Vec::new(),
            });
        allocator.heap = heap;
        if allocator.ptrs != ptrs {
            allocator.ptrs.clear();
            allocator.ptrs.extend_from_slice(ptrs);
        }
        if allocator.free_ptr_blocks != free_ptr_blocks {
            allocator.free_ptr_blocks.clear();
            allocator.free_ptr_blocks.extend_from_slice(free_ptr_blocks);
        }
        if allocator.free_handle_blocks != free_handle_blocks {
            allocator.free_handle_blocks.clear();
            allocator
                .free_handle_blocks
                .extend_from_slice(free_handle_blocks);
        }
        self.native_allocator_dirty = false;
    }

    #[cfg(test)]
    pub(crate) fn native_allocator_update(&self) -> Option<ProcessNativeAllocatorState> {
        self.native_allocator_dirty
            .then(|| self.native_allocator.clone())
            .flatten()
    }

    pub(crate) fn native_allocator_snapshot(&self) -> Option<ProcessNativeAllocatorState> {
        self.native_allocator.clone()
    }

    pub(crate) fn native_heap_state(&self) -> Option<ProcessNativeHeapState> {
        self.native_allocator
            .as_ref()
            .map(|allocator| allocator.heap)
    }

    pub(crate) fn native_ptr_records(&self) -> &[ProcessPtrRecord] {
        self.native_allocator
            .as_ref()
            .map_or(&[], |allocator| allocator.ptrs.as_slice())
    }

    pub(crate) fn native_free_ptr_blocks(&self) -> &[ProcessPtrRecord] {
        self.native_allocator
            .as_ref()
            .map_or(&[], |allocator| allocator.free_ptr_blocks.as_slice())
    }

    #[cfg(test)]
    pub(crate) fn native_allocator(&self) -> Option<&ProcessNativeAllocatorState> {
        self.native_allocator.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn set_native_allocation(&mut self, record: ProcessHandleRecord) {
        self.set_native_allocation_record(record);
    }

    #[cfg(test)]
    pub(crate) fn mutate_native_allocator(
        &mut self,
        mutation: impl FnOnce(&mut ProcessNativeAllocatorState),
    ) {
        mutation(
            self.native_allocator
                .as_mut()
                .expect("native allocator registered"),
        );
        self.native_allocator_dirty = true;
    }

    #[cfg(test)]
    pub(crate) fn handle_for_ptr(&self, ptr: u32) -> Option<u32> {
        self.ptr_to_handle.get(&ptr)
    }

    #[cfg(test)]
    pub(crate) fn track_handle_ptr(&mut self, ptr: u32, handle: u32) -> Option<u32> {
        self.ptr_to_handle.insert(ptr, handle)
    }

    fn assert_can_adopt_handle_metadata(&self, source: &Self) {
        if let Some(source_allocator) = source.classic_allocator.as_ref() {
            assert!(
                self.classic_allocator.is_none(),
                "cannot adopt a standalone classic heap into an attached process allocator"
            );
            source_allocator.assert_owned_by(Rc::as_ptr(&source.classic_owner) as usize);
        }
        if let (Some(target_limit), Some(source_limit)) =
            (self.classic_heap_limit, source.classic_heap_limit)
        {
            assert_eq!(
                target_limit, source_limit,
                "cannot adopt process Memory Managers with different classic heap ceilings"
            );
        }
    }

    fn adopt_application_heap_limit(&mut self, source: &mut Self) {
        // The process owner is authoritative when an adapter is attached
        // after another ISA has already established a limit that is valid in
        // the source address space. A native adapter cannot use a classic
        // limit below its current native heap cursor (or above its mapped
        // ceiling), however; carry the native launch value in that case so
        // attaching a companion does not make every native allocation fail.
        let source_limit = source.application_heap_limit.take();
        let target_limit_is_compatible = self.application_heap_limit.is_none_or(|limit| {
            source.native_heap_state().is_none_or(|heap| {
                limit >= heap.heap_cursor && limit <= heap.heap_limit
            })
        });
        if target_limit_is_compatible {
            if self.application_heap_limit.is_none() {
                self.application_heap_limit = source_limit;
            }
        } else {
            self.application_heap_limit = source_limit;
        }
    }

    fn adopt_handle_metadata(&mut self, source: &mut Self) {
        self.assert_can_adopt_handle_metadata(source);
        if let Some(source_allocator) = source.classic_allocator.as_ref() {
            source_allocator.transfer_owner(
                Rc::as_ptr(&source.classic_owner) as usize,
                Rc::as_ptr(&self.classic_owner) as usize,
            );
            self.classic_allocator = source.classic_allocator.take();
            self.classic_heap_limit = source.classic_heap_limit.take();
        }
        if self.ptr_to_handle.ptr_eq(&source.ptr_to_handle)
            && self.handle_state_bits.ptr_eq(&source.handle_state_bits)
            && self.handle_high_locked.ptr_eq(&source.handle_high_locked)
        {
            return;
        }
        self.ptr_to_handle
            .extend(source.ptr_to_handle.take_entries());
        self.handle_state_bits
            .extend(source.handle_state_bits.take_entries());
        self.handle_high_locked
            .extend(source.handle_high_locked.take_entries());
        if self.native_allocations.is_empty() {
            self.native_allocations
                .append(&mut source.native_allocations);
            self.native_handle_ptrs
                .extend(source.native_handle_ptrs.drain());
            self.native_handles.extend(source.native_handles.drain());
        }
    }

    fn native_allocator_is_pristine(&self) -> bool {
        self.native_allocations.is_empty()
            && self.native_handle_ptrs.is_empty()
            && self.native_handles.is_empty()
            && self.native_allocator.as_ref().is_none_or(|allocator| {
                allocator.heap == allocator.initial_heap
                    && allocator.ptrs.is_empty()
                    && allocator.free_ptr_blocks.is_empty()
                    && allocator.free_handle_blocks.is_empty()
            })
    }

    fn assert_can_adopt_native_allocator(&self, source: &Self) {
        assert!(
            self.native_allocator_is_pristine() || source.native_allocator_is_pristine(),
            "cannot attach two populated native allocators"
        );
    }

    /// Transfer one standalone native allocator into the process owner.
    ///
    /// An allocator with no state beyond its launch-time heap baseline may be
    /// discarded. A populated allocator instead replaces a pristine target,
    /// leaving no second owner behind after attachment.
    fn adopt_native_allocator(&mut self, source: &mut Self) {
        self.assert_can_adopt_native_allocator(source);
        let target_is_pristine = self.native_allocator_is_pristine();
        let source_is_pristine = source.native_allocator_is_pristine();
        let source_supplies_allocator = !source_is_pristine
            || (self.native_allocator.is_none() && source.native_allocator.is_some());

        if target_is_pristine && source_supplies_allocator {
            self.native_allocator = source.native_allocator.take();
            self.native_allocator_dirty = source.native_allocator_dirty;
            self.native_allocations = std::mem::take(&mut source.native_allocations);
            self.native_handle_ptrs = std::mem::take(&mut source.native_handle_ptrs);
            self.native_handles = std::mem::take(&mut source.native_handles);
        }

        source.native_allocator = None;
        source.native_allocator_dirty = false;
        source.native_allocations.clear();
        source.native_handle_ptrs.clear();
        source.native_handles.clear();
    }

    /// Reject an incompatible Memory Manager handoff before either owner is
    /// modified.
    pub(crate) fn assert_can_adopt_process_memory_manager(&self, source: &Self) {
        self.assert_can_adopt_native_allocator(source);
        self.assert_can_adopt_handle_metadata(source);
    }

    /// Transfer every standalone allocator and handle index into one process
    /// owner after all compatibility checks have passed.
    pub(crate) fn adopt_process_memory_manager(&mut self, source: &mut Self) {
        self.assert_can_adopt_process_memory_manager(source);
        // Resolve the application boundary while the source still exposes its
        // native heap ceiling; native allocator adoption clears that source
        // projection as part of transferring ownership.
        self.adopt_application_heap_limit(source);
        self.adopt_native_allocator(source);
        self.adopt_handle_metadata(source);
    }

    #[cfg(test)]
    pub(crate) fn handle_state(&self, handle: u32) -> u8 {
        self.state_for_handle(handle).unwrap_or(0)
    }
}

/// Canonical owner for state that belongs to one emulated process rather than
/// to either of its CPU ABI adapters.
///
/// `FixtureRunner` owns this context and serializes all adapter access through
/// its mutable borrow.
#[derive(Debug)]
pub(crate) struct ProcessContext {
    memory: Vec<ProcessMemoryRegion>,
    memory_manager: SharedProcessMemoryManager,
    event_queue: SharedProcessEventQueue,
    input_state: SharedProcessInputState,
    menu_tracking: SharedProcessMenuTracking,
    window_list: SharedProcessWindowList,
    pending_native_menu_selection: SharedNativeMenuSelection,
    guest_calls: SharedGuestCallStack,
    apple_event_handlers: SharedProcessAppleEventHandlers,
    file_system: SharedProcessFileSystem,
    sound_manager: SharedProcessSoundManager,
    timer_tasks: SharedProcessTimerTasks,
    vbl_tasks: SharedProcessVblTasks,
    callback_scheduling: SharedProcessCallbackScheduling,
    mixed_mode_m68k: SharedProcessMixedModeM68kState,
    scrap_state: SharedProcessScrapState,
    control_manager: SharedProcessControlManager,
    list_manager: SharedProcessListManager,
    text_edit_manager: SharedProcessTextEditManager,
    dialog_text: SharedProcessDialogText,
    cursor_state: SharedProcessCursorState,
    quickdraw_op_colors: SharedProcessQuickDrawOpColors,
    quickdraw_hilite_colors: SharedProcessQuickDrawHiliteColors,
    current_graphics_port: SharedProcessValue<u32>,
    current_graphics_device: SharedProcessValue<u32>,
    quickdraw_error: SharedProcessValue<i16>,
    device_clut: SharedProcessValue<[[u16; 3]; 256]>,
    color_manager_clut: SharedProcessValue<[[u16; 3]; 256]>,
    device_gamma: SharedProcessValue<DisplayGamma>,
    device_gamma_explicit: SharedProcessValue<bool>,
}

impl Default for ProcessContext {
    fn default() -> Self {
        Self {
            memory: Vec::new(),
            memory_manager: SharedProcessMemoryManager::default(),
            event_queue: SharedProcessEventQueue::default(),
            input_state: SharedProcessInputState::default(),
            menu_tracking: SharedProcessMenuTracking::default(),
            window_list: SharedProcessWindowList::default(),
            pending_native_menu_selection: SharedNativeMenuSelection::default(),
            guest_calls: SharedGuestCallStack::default(),
            apple_event_handlers: SharedProcessAppleEventHandlers::default(),
            file_system: SharedProcessFileSystem::default(),
            sound_manager: SharedProcessSoundManager::default(),
            timer_tasks: SharedProcessTimerTasks::default(),
            vbl_tasks: SharedProcessVblTasks::default(),
            callback_scheduling: SharedProcessCallbackScheduling::default(),
            mixed_mode_m68k: SharedProcessMixedModeM68kState::default(),
            scrap_state: SharedProcessScrapState::default(),
            control_manager: SharedProcessControlManager::default(),
            list_manager: SharedProcessListManager::default(),
            text_edit_manager: SharedProcessTextEditManager::default(),
            dialog_text: SharedProcessDialogText::default(),
            cursor_state: SharedProcessCursorState::default(),
            quickdraw_op_colors: SharedProcessQuickDrawOpColors::default(),
            quickdraw_hilite_colors: SharedProcessQuickDrawHiliteColors::default(),
            current_graphics_port: SharedProcessValue::from_value(0),
            current_graphics_device: SharedProcessValue::from_value(0),
            quickdraw_error: SharedProcessValue::from_value(0),
            device_clut: SharedProcessValue::from_value(standard_mac_8bpp_clut()),
            color_manager_clut: SharedProcessValue::from_value(standard_mac_8bpp_clut()),
            device_gamma: SharedProcessValue::from_value(default_display_gamma()),
            device_gamma_explicit: SharedProcessValue::from_value(false),
        }
    }
}

impl ProcessContext {
    pub(crate) fn with_file_system(file_system: SharedProcessFileSystem) -> Self {
        Self {
            file_system,
            ..Self::default()
        }
    }

    pub(crate) fn detached_vfs_snapshot(&self) -> SharedProcessFileSystem {
        self.file_system.detached_vfs_snapshot()
    }

    #[cfg(test)]
    pub(crate) fn memory_manager_mut(&self) -> RefMut<'_, ProcessMemoryManager> {
        self.memory_manager.borrow_mut()
    }

    pub(crate) fn attach_classic_memory_bus(&mut self, bus: &mut MacMemoryBus) {
        self.memory_manager
            .borrow_mut()
            .attach_classic_memory_bus(bus);
    }

    /// Return the process-owned classic heap cursor used by launch-time
    /// partition setup. `MacMemoryBus` remains the byte adapter; the cursor
    /// belongs to the process Memory Manager so all attached adapters observe
    /// it immediately. Inside Macintosh: Memory (1992), pp. 2-19--2-21.
    pub(crate) fn classic_heap_bump_ptr(&self) -> u32 {
        self.memory_manager.borrow().classic_heap_bump_ptr()
    }

    /// Reserve the process-owned classic heap's initial zone header.
    pub(crate) fn reserve_classic_heap(&self, size: u32) {
        self.memory_manager.borrow_mut().reserve_classic_heap(size);
    }

    /// Keep a direct-loaded guest span out of future classic allocations while
    /// retaining any usable heap space before that span.
    pub(crate) fn reserve_classic_heap_range(&self, start_addr: u32, end_addr: u32) {
        self.memory_manager
            .borrow_mut()
            .reserve_classic_heap_range(start_addr, end_addr);
    }

    #[cfg(test)]
    pub(crate) fn handle_for_ptr(&self, ptr: u32) -> Option<u32> {
        self.memory_manager.borrow().handle_for_ptr(ptr)
    }

    pub(crate) fn attach_memory_manager(&self, adapter: &mut Option<SharedProcessMemoryManager>) {
        if let Some(attached) = adapter {
            assert!(
                attached.ptr_eq(&self.memory_manager),
                "cannot attach two process Memory Managers"
            );
        } else {
            *adapter = Some(self.memory_manager.clone());
        }
    }

    pub(crate) fn attach_file_system(&self, adapter: &mut SharedProcessFileSystem) {
        adapter.attach_to(&self.file_system);
    }

    #[cfg(test)]
    pub(crate) fn attach_resource_manager(&self, adapter: &mut SharedProcessResourceManager) {
        adapter.attach_resource_manager_to(&self.file_system.resource_manager);
    }

    pub(crate) fn attach_sound_manager(&self, adapter: &mut SharedProcessSoundManager) {
        adapter.attach_to(&self.sound_manager, SoundManager::is_pristine);
    }

    pub(crate) fn attach_callback_tasks(
        &self,
        timer_tasks: &mut SharedProcessTimerTasks,
        vbl_tasks: &mut SharedProcessVblTasks,
        scheduling: &mut SharedProcessCallbackScheduling,
    ) {
        timer_tasks.attach_to(&self.timer_tasks, Vec::is_empty);
        vbl_tasks.attach_to(&self.vbl_tasks, Vec::is_empty);
        scheduling.attach_to(&self.callback_scheduling, |state| {
            state == &Default::default()
        });
    }

    pub(crate) fn attach_mixed_mode_m68k_state(
        &self,
        adapter: &mut SharedProcessMixedModeM68kState,
    ) {
        adapter.attach_copy_to(
            &self.mixed_mode_m68k,
            ProcessMixedModeM68kState::is_pristine,
        );
    }

    pub(crate) fn attach_scrap_state(&self, adapter: &mut SharedProcessScrapState) {
        adapter.attach_to(&self.scrap_state, ProcessScrapState::is_pristine);
    }

    pub(crate) fn attach_control_manager(&self, adapter: &mut SharedProcessControlManager) {
        adapter.attach_to(&self.control_manager, ProcessControlManagerState::is_pristine);
    }

    pub(crate) fn attach_list_manager(&self, adapter: &mut SharedProcessListManager) {
        adapter.attach_to(&self.list_manager, ProcessListManagerState::is_pristine);
    }

    pub(crate) fn attach_text_edit_manager(&self, adapter: &mut SharedProcessTextEditManager) {
        adapter.attach_to(
            &self.text_edit_manager,
            ProcessTextEditManagerState::is_pristine,
        );
    }

    pub(crate) fn attach_dialog_text(&self, adapter: &mut SharedProcessDialogText) {
        adapter.attach_to(&self.dialog_text, |slots| slots.iter().all(Vec::is_empty));
    }

    pub(crate) fn attach_cursor_state(&self, adapter: &mut SharedProcessCursorState) {
        adapter.attach_to(&self.cursor_state, ProcessCursorState::is_pristine);
    }

    /// Attach Color QuickDraw's per-port `GrafVars.rgbOpColor` index. The
    /// ordinary clone path of `SharedProcessValue` remains detached, while
    /// attached 68K and PowerPC adapters observe updates immediately.
    /// Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64.
    pub(crate) fn attach_quickdraw_op_colors(
        &self,
        adapter: &mut SharedProcessQuickDrawOpColors,
    ) {
        adapter.attach_to(&self.quickdraw_op_colors, |colors| colors.is_empty());
    }

    /// Attach Color QuickDraw's per-port `GrafVars.rgbHiliteColor` index.
    /// Ordinary clones remain detached while attached 68K and PowerPC
    /// adapters observe updates immediately.
    /// Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64.
    pub(crate) fn attach_quickdraw_hilite_colors(
        &self,
        adapter: &mut SharedProcessQuickDrawHiliteColors,
    ) {
        adapter.attach_to(&self.quickdraw_hilite_colors, |colors| colors.is_empty());
    }

    /// Attach a CPU adapter to the process's current QuickDraw port and device.
    ///
    /// `GetPort`/`SetPort` expose one `thePort`, while `GetGWorld`/`SetGWorld`
    /// preserve the associated current graphics device. Imaging With
    /// QuickDraw (1994), pp. 2-41--2-42 and 6-29.
    pub(crate) fn attach_quickdraw_selection(
        &self,
        current_port: &mut SharedProcessValue<u32>,
        current_device: &mut SharedProcessValue<u32>,
    ) {
        current_port.attach_copy_to(&self.current_graphics_port, |address| *address == 0);
        current_device.attach_copy_to(&self.current_graphics_device, |address| *address == 0);
    }

    pub(crate) fn activate_quickdraw_selection(
        &self,
        current_port: &mut SharedProcessValue<u32>,
        current_device: &mut SharedProcessValue<u32>,
    ) {
        current_port.activate_copy_to(&self.current_graphics_port);
        current_device.activate_copy_to(&self.current_graphics_device);
    }

    /// Attach the error from the last applicable Color QuickDraw or Color
    /// Manager operation. QDError exposes one process result regardless of
    /// which CPU ABI performed the operation. Imaging With QuickDraw (1994),
    /// pp. 4-94--4-95.
    pub(crate) fn attach_quickdraw_error(&self, error: &mut SharedProcessValue<i16>) {
        error.attach_copy_to(&self.quickdraw_error, |value| *value == 0);
    }

    pub(crate) fn attach_display_color_state(
        &self,
        device_clut: &mut SharedProcessValue<[[u16; 3]; 256]>,
        color_manager_clut: &mut SharedProcessValue<[[u16; 3]; 256]>,
        device_gamma: &mut SharedProcessValue<DisplayGamma>,
        device_gamma_explicit: &mut SharedProcessValue<bool>,
    ) {
        let clut_is_pristine =
            |clut: &[[u16; 3]; 256]| *clut == [[0; 3]; 256] || *clut == standard_mac_8bpp_clut();
        let gamma_is_pristine = |gamma: &DisplayGamma| {
            gamma
                .iter()
                .all(|channel| channel.iter().all(|component| *component == 0))
                || *gamma == default_display_gamma()
        };
        device_clut.attach_copy_to(&self.device_clut, clut_is_pristine);
        color_manager_clut.attach_copy_to(&self.color_manager_clut, clut_is_pristine);
        device_gamma.attach_copy_to(&self.device_gamma, gamma_is_pristine);
        device_gamma_explicit.attach_copy_to(&self.device_gamma_explicit, |explicit| !*explicit);
    }

    pub(crate) fn attach_event_queue(&self, adapter: &mut SharedProcessEventQueue) {
        // The Operating System Event Manager maintains one FIFO queue for the
        // current process. GetNextEvent removes the first matching event while
        // EventAvail observes it in place. Inside Macintosh Volume I (1985),
        // pp. I-244--I-245 and I-257--I-259; Processes (1994), pp. 2-15--2-16.
        adapter.attach_to(&self.event_queue, EventQueue::is_pristine);
    }

    /// Attach the Window Manager's canonical front-to-back process list.
    ///
    /// WindowRecords remain guest-memory objects, while this list models the
    /// process-wide ordering used by FrontWindow, FindWindow, activation, and
    /// occlusion. Macintosh Toolbox Essentials (1992), pp. 4-64--4-65.
    pub(crate) fn attach_window_list(&self, adapter: &mut SharedProcessWindowList) {
        adapter.attach_to(&self.window_list, Vec::is_empty);
    }

    pub(crate) fn attach_input_state(&self, adapter: &mut SharedProcessInputState) {
        adapter.attach_to(&self.input_state, ProcessInputState::is_pristine);
    }

    pub(crate) fn attach_menu_tracking(&self, adapter: &mut SharedProcessMenuTracking) {
        // MenuSelect owns one retained selection and pane hierarchy until the
        // mouse is released and any MenuFlash phases complete. Both ISA
        // gateways therefore attach to the same process continuation. Inside
        // Macintosh Volume I (1985), pp. I-354--I-366; Macintosh Toolbox
        // Essentials (1992), pp. 3-87--3-92 and 3-140--3-142.
        if Rc::ptr_eq(&adapter.0, &self.menu_tracking.0) {
            return;
        }
        assert!(
            adapter.is_none() || self.menu_tracking.is_none(),
            "cannot attach two active Menu Manager continuations"
        );
        adapter.attach_to(&self.menu_tracking, Option::is_none);
    }

    pub(crate) fn attach_classic_file_system(
        &self,
        data_forks: &mut SharedProcessValue<ProcessForkMap>,
        resource_forks: &mut SharedProcessValue<ProcessForkMap>,
    ) {
        data_forks.attach_to(
            &self.file_system.vfs_files.data_forks,
            ProcessForkMap::is_empty,
        );
        resource_forks.attach_to(
            &self.file_system.vfs_resource_files.resource_forks,
            ProcessForkMap::is_empty,
        );
    }

    /// Attach the classic path indexes to the process File Manager.
    ///
    /// Volume records and their negative-reference allocator are attached by
    /// the adapter's process-file-system handle, so this compatibility-index
    /// method deliberately has no parallel volume arguments.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn attach_classic_vfs_catalogue(
        &self,
        directories: &mut SharedProcessValue<Vec<ProcessVfsDirectory>>,
        metadata: &mut SharedProcessValue<HashMap<String, ProcessVfsMetadata>>,
        locked_files: &mut SharedProcessValue<HashSet<String>>,
        next_dir_id: &mut SharedProcessValue<u32>,
        next_file_id: &mut SharedProcessValue<u32>,
        next_timestamp: &mut SharedProcessValue<u32>,
        default_dir_id: &mut SharedProcessValue<u32>,
    ) {
        directories.attach_to(&self.file_system.vfs_directories, |directories| {
            process_vfs_directories_are_pristine(directories)
        });
        metadata.attach_to(&self.file_system.classic_vfs_metadata, HashMap::is_empty);
        locked_files.attach_to(&self.file_system.classic_locked_files, HashSet::is_empty);
        next_dir_id.attach_to(&self.file_system.next_vfs_dir_id, |value| {
            matches!(*value, 0 | 16 | 18)
        });
        next_file_id.attach_to(&self.file_system.classic_next_vfs_file_id, |value| {
            *value == 32
        });
        next_timestamp.attach_to(&self.file_system.classic_next_vfs_timestamp, |value| {
            *value == 1
        });
        default_dir_id.attach_to(&self.file_system.default_dir_id, |value| {
            matches!(*value, 0 | 2)
        });
    }

    /// Install a canonical process-memory allocation and attach a CPU
    /// address-space adapter to it.
    ///
    /// Repeated attachment is allowed for another adapter (or a relaunched
    /// native fragment), but each range must either match an existing region
    /// exactly or remain disjoint from every region already owned here.
    pub(crate) fn attach_memory(
        &mut self,
        base: u32,
        bytes: SharedRamRegion,
        adapter: &mut GuestAddressSpace,
    ) {
        let len = bytes.len();
        let memory_index = self
            .memory
            .iter()
            .position(|memory| memory.base == base && memory.bytes.len() == len)
            .unwrap_or_else(|| {
                let start = u64::from(base);
                let end = start.saturating_add(len as u64);
                assert!(
                    self.memory.iter().all(|memory| {
                        let memory_start = u64::from(memory.base);
                        let memory_end = memory_start.saturating_add(memory.bytes.len() as u64);
                        end <= memory_start || memory_end <= start
                    }),
                    "cannot overlap process memory regions"
                );
                self.memory.push(ProcessMemoryRegion { base, bytes });
                self.memory.len() - 1
            });

        let memory = &self.memory[memory_index];
        // SAFETY: `ProcessContext` and all attached CPU adapters are private
        // children of one runner. Every execution entry point requires an
        // exclusive mutable runner borrow, so adapter access is serialized.
        unsafe {
            adapter.add_shared_region(memory.base, memory.bytes.clone());
        }
    }

    #[cfg(test)]
    pub(crate) fn memory_ranges(&self) -> Vec<(u32, usize)> {
        self.memory
            .iter()
            .map(|memory| (memory.base, memory.bytes.len()))
            .collect()
    }

    pub(crate) fn event_queue(&self) -> &EventQueue {
        &self.event_queue
    }

    pub(crate) fn event_queue_mut(&mut self) -> &mut SharedProcessEventQueue {
        &mut self.event_queue
    }

    pub(crate) fn menu_tracking(&self) -> Option<&ProcessMenuTrackingState> {
        self.menu_tracking.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn menu_tracking_mut(&mut self) -> Option<&mut ProcessMenuTrackingState> {
        self.menu_tracking.as_mut()
    }

    #[cfg(test)]
    pub(crate) fn take_menu_tracking(&mut self) -> Option<ProcessMenuTrackingState> {
        self.menu_tracking.take()
    }

    #[cfg(test)]
    pub(crate) fn set_menu_tracking(&mut self, state: Option<ProcessMenuTrackingState>) {
        *self.menu_tracking = state;
    }

    #[cfg(test)]
    pub(crate) fn memory_manager_handle(&self) -> &SharedProcessMemoryManager {
        &self.memory_manager
    }

    pub(crate) fn attach_native_menu_selection(&self, adapter: &mut SharedNativeMenuSelection) {
        adapter.attach_to(&self.pending_native_menu_selection);
    }

    pub(crate) fn attach_guest_calls(&self, adapter: &mut SharedGuestCallStack) {
        adapter.attach_to(&self.guest_calls);
    }

    pub(crate) fn attach_apple_event_handlers(
        &self,
        adapter: &mut SharedProcessAppleEventHandlers,
    ) {
        adapter.attach_to(&self.apple_event_handlers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_queue::QueuedEvent;
    use crate::guest_call::GuestCallTarget;
    use crate::guest_procedure::GuestIsa;
    use crate::memory::{MacMemoryBus, MemoryBus};
    use ppc::PpcMemory;

    fn native_heap_state(heap_cursor: u32, heap_limit: u32) -> ProcessNativeHeapState {
        ProcessNativeHeapState {
            heap_base: 0x0300_0000,
            heap_cursor,
            heap_limit,
            last_mem_error: 0,
            heap_maximized: false,
            master_pointer_blocks_requested: 0,
        }
    }

    fn classic_owner_id(allocator: &SharedClassicHeapAllocator) -> Option<usize> {
        allocator.0.borrow().owner_id
    }

    #[test]
    fn process_context_owns_the_memory_mapping_for_cpu_adapters() {
        let mut context = ProcessContext::default();
        let mut bus = MacMemoryBus::new(0x2000);
        bus.write_long(0x100, 0x1234_5678);
        let region = bus.shared_ram_region(0, 0x1000).unwrap();
        let mut native = GuestAddressSpace::new();

        context.attach_memory(0, region, &mut native);

        assert_eq!(context.memory_ranges(), vec![(0, 0x1000)]);
        assert_eq!(native.read_u32_be(0x100), Some(0x1234_5678));
        native.write_u32_be(0x100, 0x89ab_cdef).unwrap();
        assert_eq!(bus.read_long(0x100), 0x89ab_cdef);
    }

    #[test]
    fn process_context_owns_the_classic_heap_allocator() {
        let mut context = ProcessContext::default();
        let mut primary = MacMemoryBus::new(8 * 1024 * 1024);
        context.attach_classic_memory_bus(&mut primary);

        let ptr = context
            .memory_manager_mut()
            .new_classic_ptr(&mut primary, 37);
        assert_ne!(ptr, 0);
        assert_eq!(
            context.memory_manager.borrow().classic_allocation_size(ptr),
            Some(37)
        );

        let mut second_adapter = MacMemoryBus::new(8 * 1024 * 1024);
        context.attach_classic_memory_bus(&mut second_adapter);
        assert_eq!(second_adapter.get_alloc_size(ptr), Some(37));
        context
            .memory_manager_mut()
            .dispose_process_ptr(&mut second_adapter, ptr);
        assert_eq!(primary.get_alloc_size(ptr), None);
        assert_eq!(
            context.memory_manager.borrow().classic_allocation_size(ptr),
            None
        );

        let recycled = primary.alloc(21);
        assert_eq!(recycled, ptr);
        assert_eq!(second_adapter.get_alloc_size(recycled), Some(21));
    }

    #[test]
    fn detached_classic_heap_allocators_remain_independent() {
        let mut attached = MacMemoryBus::new(8 * 1024 * 1024);
        let mut detached = MacMemoryBus::new(8 * 1024 * 1024);
        let mut context = ProcessContext::default();
        context.attach_classic_memory_bus(&mut attached);

        let attached_ptr = attached.alloc(24);
        let detached_ptr = detached.alloc(24);
        assert_eq!(attached_ptr, detached_ptr);
        attached.free(attached_ptr);

        assert_eq!(
            context
                .memory_manager
                .borrow()
                .classic_allocation_size(attached_ptr),
            None
        );
        assert_eq!(detached.get_alloc_size(detached_ptr), Some(24));
        assert_eq!(attached.alloc(16), attached_ptr);
        assert_eq!(detached.alloc(16), detached_ptr + 24);
        assert_eq!(detached.heap_bump_ptr(), 0x20_0000 + 40);
    }

    #[test]
    fn detached_process_memory_manager_snapshots_classic_allocator_state() {
        let mut context = ProcessContext::default();
        let mut attached_bus = MacMemoryBus::new(8 * 1024 * 1024);
        context.attach_classic_memory_bus(&mut attached_bus);
        let manager = context.memory_manager_handle().clone();
        let original_ptr = manager.borrow_mut().new_classic_ptr(&mut attached_bus, 24);
        assert_ne!(original_ptr, 0);

        let detached_manager = manager.detached_clone();
        assert!(!manager.ptr_eq(&detached_manager));
        let mut detached_bus = MacMemoryBus::new(8 * 1024 * 1024);
        detached_manager
            .borrow_mut()
            .attach_classic_memory_bus(&mut detached_bus);
        assert_eq!(
            detached_manager
                .borrow()
                .classic_allocation_size(original_ptr),
            Some(24)
        );

        manager
            .borrow_mut()
            .dispose_process_ptr(&mut attached_bus, original_ptr);
        let attached_reuse = manager.borrow_mut().new_classic_ptr(&mut attached_bus, 16);
        let detached_next = detached_manager
            .borrow_mut()
            .new_classic_ptr(&mut detached_bus, 16);

        assert_eq!(attached_reuse, original_ptr);
        assert_eq!(detached_next, original_ptr + 24);
        assert_eq!(
            manager.borrow().classic_allocation_size(original_ptr),
            Some(16)
        );
        assert_eq!(
            detached_manager
                .borrow()
                .classic_allocation_size(original_ptr),
            Some(24)
        );
    }

    #[test]
    fn detached_process_memory_manager_snapshots_application_heap_limit() {
        const HEAP_BASE: u32 = 0x0300_0000;
        const NATIVE_HEAP_CEILING: u32 = HEAP_BASE + 0x20_000;
        const APPLICATION_HEAP_LIMIT: u32 = HEAP_BASE + 0x10_000;

        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE + 0x100,
                heap_limit: NATIVE_HEAP_CEILING,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.set_application_heap_limit(APPLICATION_HEAP_LIMIT);

        let detached = manager.detached_clone();
        assert_eq!(detached.application_heap_limit(0), APPLICATION_HEAP_LIMIT);
        assert_eq!(
            detached.native_heap_state().unwrap().heap_limit,
            NATIVE_HEAP_CEILING,
            "detached application-limit state must not rewrite the native allocator ceiling"
        );

        manager.set_application_heap_limit(HEAP_BASE + 0x18_000);
        manager.restore_native_snapshot(detached);
        assert_eq!(
            manager.application_heap_limit(0),
            APPLICATION_HEAP_LIMIT,
            "transaction restore must roll back the canonical application limit"
        );
        assert_eq!(
            manager.native_heap_state().unwrap().heap_limit,
            NATIVE_HEAP_CEILING
        );
    }

    #[test]
    fn native_attachment_replaces_an_incompatible_prior_application_limit() {
        const HEAP_BASE: u32 = 0x0300_0000;
        const NATIVE_CURSOR: u32 = HEAP_BASE + 0x100;
        const NATIVE_HEAP_CEILING: u32 = HEAP_BASE + 0x2000;
        const NATIVE_APPLICATION_LIMIT: u32 = HEAP_BASE + 0x1000;

        let mut target = ProcessMemoryManager::default();
        // A classic-only process may have established a low-memory limit in
        // its own address range before a native companion is attached.
        target.set_application_heap_limit(HEAP_BASE - 0x100);

        let mut source = ProcessMemoryManager::default();
        source.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: NATIVE_CURSOR,
                heap_limit: NATIVE_HEAP_CEILING,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        source.set_application_heap_limit(NATIVE_APPLICATION_LIMIT);

        target.adopt_process_memory_manager(&mut source);

        assert_eq!(
            target.application_heap_limit(0),
            NATIVE_APPLICATION_LIMIT,
            "native attachment must retain an allocation-valid process boundary"
        );
        assert_eq!(source.application_heap_limit(0), 0);
    }

    #[test]
    fn attaching_a_second_populated_classic_allocator_is_rejected_without_mutation() {
        let mut context = ProcessContext::default();
        let mut primary = MacMemoryBus::new(8 * 1024 * 1024);
        context.attach_classic_memory_bus(&mut primary);
        let primary_allocator = primary.shared_classic_heap_allocator();
        let primary_owner_before = classic_owner_id(&primary_allocator);
        let primary_ptr = context
            .memory_manager_mut()
            .new_classic_ptr(&mut primary, 24);
        let cursor_before = context.classic_heap_bump_ptr();

        let mut second = MacMemoryBus::new(8 * 1024 * 1024);
        let second_ptr = second.alloc(24);
        let second_allocator = second.shared_classic_heap_allocator();
        let second_owner_before = classic_owner_id(&second_allocator);
        assert_ne!(second_ptr, 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            context.attach_classic_memory_bus(&mut second);
        }));

        assert!(result.is_err());
        assert_eq!(context.classic_heap_bump_ptr(), cursor_before);
        assert_eq!(
            context
                .memory_manager
                .borrow()
                .classic_allocation_size(primary_ptr),
            Some(24)
        );
        assert_eq!(second.get_alloc_size(second_ptr), Some(24));
        assert_eq!(classic_owner_id(&primary_allocator), primary_owner_before);
        assert_eq!(classic_owner_id(&second_allocator), second_owner_before);
    }

    #[test]
    fn rejected_populated_classic_bus_can_attach_to_a_fresh_process() {
        let mut original_context = ProcessContext::default();
        let mut original_bus = MacMemoryBus::new(8 * 1024 * 1024);
        original_context.attach_classic_memory_bus(&mut original_bus);
        let original_ptr = original_context
            .memory_manager_mut()
            .new_classic_ptr(&mut original_bus, 24);
        let original_allocator = original_bus.shared_classic_heap_allocator();
        let original_owner = classic_owner_id(&original_allocator);

        let mut incoming_bus = MacMemoryBus::new(8 * 1024 * 1024);
        let incoming_ptr = incoming_bus.alloc(32);
        let incoming_allocator = incoming_bus.shared_classic_heap_allocator();
        let incoming_owner = classic_owner_id(&incoming_allocator);
        let incoming_cursor = incoming_bus.heap_bump_ptr();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            original_context.attach_classic_memory_bus(&mut incoming_bus);
        }));

        assert!(result.is_err());
        assert_eq!(classic_owner_id(&original_allocator), original_owner);
        assert_eq!(classic_owner_id(&incoming_allocator), incoming_owner);
        assert_eq!(incoming_bus.heap_bump_ptr(), incoming_cursor);
        assert_eq!(incoming_bus.get_alloc_size(incoming_ptr), Some(32));
        assert_eq!(
            original_context
                .memory_manager
                .borrow()
                .classic_allocation_size(original_ptr),
            Some(24)
        );

        let mut fresh_context = ProcessContext::default();
        fresh_context.attach_classic_memory_bus(&mut incoming_bus);

        assert_eq!(fresh_context.classic_heap_bump_ptr(), incoming_cursor);
        assert_eq!(
            fresh_context
                .memory_manager
                .borrow()
                .classic_allocation_size(incoming_ptr),
            Some(32)
        );
        assert_eq!(incoming_bus.get_alloc_size(incoming_ptr), Some(32));
        assert_ne!(classic_owner_id(&incoming_allocator), incoming_owner);
    }

    #[test]
    fn populated_process_allocator_attaches_a_pristine_bus() {
        let mut context = ProcessContext::default();
        let mut attached_bus = MacMemoryBus::new(8 * 1024 * 1024);
        context.attach_classic_memory_bus(&mut attached_bus);
        let ptr = context
            .memory_manager_mut()
            .new_classic_ptr(&mut attached_bus, 24);
        let process_allocator = attached_bus.shared_classic_heap_allocator();
        let process_owner = classic_owner_id(&process_allocator);

        let mut pristine_bus = MacMemoryBus::new(8 * 1024 * 1024);
        context.attach_classic_memory_bus(&mut pristine_bus);

        assert!(
            process_allocator.ptr_eq(&pristine_bus.shared_classic_heap_allocator()),
            "successful attachment must share the process allocator"
        );
        assert_eq!(
            classic_owner_id(&pristine_bus.shared_classic_heap_allocator()),
            process_owner
        );
        assert_eq!(pristine_bus.get_alloc_size(ptr), Some(24));
    }

    #[test]
    fn pristine_process_allocator_adopts_the_only_populated_bus() {
        let mut context = ProcessContext::default();
        let mut first_adapter = MacMemoryBus::new(8 * 1024 * 1024);
        context.attach_classic_memory_bus(&mut first_adapter);
        let process_allocator = first_adapter.shared_classic_heap_allocator();
        let process_owner = classic_owner_id(&process_allocator);

        let mut populated = MacMemoryBus::new(8 * 1024 * 1024);
        let ptr = populated.alloc(24);
        assert_ne!(ptr, 0);

        context.attach_classic_memory_bus(&mut populated);

        assert_eq!(
            context.memory_manager.borrow().classic_allocation_size(ptr),
            Some(24)
        );
        assert!(
            process_allocator.ptr_eq(&populated.shared_classic_heap_allocator()),
            "successful adoption must retain the process allocator identity"
        );
        assert_eq!(
            classic_owner_id(&populated.shared_classic_heap_allocator()),
            process_owner
        );
        assert_eq!(first_adapter.get_alloc_size(ptr), Some(24));
        context
            .memory_manager_mut()
            .dispose_process_ptr(&mut first_adapter, ptr);
        assert_eq!(populated.get_alloc_size(ptr), None);
    }

    #[test]
    fn process_owned_reservations_are_visible_to_every_attached_bus() {
        let mut context = ProcessContext::default();
        let mut first = MacMemoryBus::new(8 * 1024 * 1024);
        let mut second = MacMemoryBus::new(8 * 1024 * 1024);
        context.attach_classic_memory_bus(&mut first);
        context.attach_classic_memory_bus(&mut second);
        context.reserve_classic_heap_range(0x20_0040, 0x20_0080);

        assert_eq!(second.alloc(64), 0x20_0000);
        assert_eq!(first.alloc(4), 0x20_0080);
    }

    #[test]
    fn classic_ptr_resize_retains_reused_bucket_capacity() {
        let mut context = ProcessContext::default();
        let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
        context.attach_classic_memory_bus(&mut bus);

        let original = context.memory_manager_mut().new_classic_ptr(&mut bus, 64);
        context
            .memory_manager_mut()
            .dispose_process_ptr(&mut bus, original);
        let reused = context.memory_manager_mut().new_classic_ptr(&mut bus, 16);
        assert_eq!(reused, original);
        let cursor = context.classic_heap_bump_ptr();
        let detached = context.memory_manager_mut().detached_clone();

        assert_eq!(
            context
                .memory_manager_mut()
                .set_process_ptr_size(&mut bus, reused, 48),
            ProcessMemoryManager::NO_ERR
        );
        assert_eq!(context.classic_heap_bump_ptr(), cursor);
        assert_eq!(
            context
                .memory_manager
                .borrow()
                .classic_allocation_size(reused),
            Some(48)
        );
        assert_eq!(detached.classic_allocation_size(reused), Some(16));

        assert_eq!(
            context
                .memory_manager_mut()
                .set_process_ptr_size(&mut bus, reused, 65),
            ProcessMemoryManager::MEM_FULL_ERR
        );
        assert_eq!(context.classic_heap_bump_ptr(), cursor);
        assert_eq!(
            context
                .memory_manager
                .borrow()
                .classic_allocation_size(reused),
            Some(48)
        );
    }

    #[test]
    fn failed_classic_handle_master_pointer_allocation_rolls_back_data() {
        let mut context = ProcessContext::default();
        let mut bus = MacMemoryBus::new(0x29_0008);
        context.attach_classic_memory_bus(&mut bus);

        assert_eq!(
            context.memory_manager_mut().new_classic_handle(&mut bus, 4),
            Err(ProcessMemoryManager::MEM_FULL_ERR)
        );
        assert_eq!(context.classic_heap_bump_ptr(), 0x20_0004);
        assert_eq!(bus.get_alloc_size(0x20_0000), None);
        assert_eq!(
            context.memory_manager_mut().new_classic_ptr(&mut bus, 4),
            0x20_0000
        );
    }

    #[test]
    fn classic_allocator_cannot_be_owned_by_two_processes() {
        let mut first_context = ProcessContext::default();
        let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
        first_context.attach_classic_memory_bus(&mut bus);
        let ptr = first_context
            .memory_manager_mut()
            .new_classic_ptr(&mut bus, 24);

        let mut second_context = ProcessContext::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            second_context.attach_classic_memory_bus(&mut bus);
        }));

        assert!(result.is_err());
        assert_eq!(
            first_context
                .memory_manager
                .borrow()
                .classic_allocation_size(ptr),
            Some(24)
        );
        assert_eq!(bus.get_alloc_size(ptr), Some(24));
        assert!(second_context
            .memory_manager
            .borrow()
            .classic_allocator
            .is_none());
    }

    #[test]
    fn process_context_owns_multiple_regions_and_clones_detach_from_all_of_them() {
        let mut context = ProcessContext::default();
        let mut bus = MacMemoryBus::new(0x5000);
        bus.write_long(0x100, 0x1122_3344);
        bus.write_long(0x3100, 0x5566_7788);
        let low = bus.shared_ram_region(0, 0x1000).unwrap();
        let high = bus.shared_ram_region(0x3000, 0x1000).unwrap();
        let mut native = GuestAddressSpace::new();

        context.attach_memory(0, low, &mut native);
        context.attach_memory(0x3000, high, &mut native);
        assert_eq!(context.memory_ranges(), vec![(0, 0x1000), (0x3000, 0x1000)]);

        let mut detached = native.clone();
        native.write_u32_be(0x100, 0x99aa_bbcc).unwrap();
        native.write_u32_be(0x3100, 0xddee_ff00).unwrap();
        assert_eq!(bus.read_long(0x100), 0x99aa_bbcc);
        assert_eq!(bus.read_long(0x3100), 0xddee_ff00);
        assert_eq!(detached.read_u32_be(0x100), Some(0x1122_3344));
        assert_eq!(detached.read_u32_be(0x3100), Some(0x5566_7788));

        detached.write_u32_be(0x100, 0x0102_0304).unwrap();
        detached.write_u32_be(0x3100, 0x0506_0708).unwrap();
        assert_eq!(bus.read_long(0x100), 0x99aa_bbcc);
        assert_eq!(bus.read_long(0x3100), 0xddee_ff00);
    }

    #[test]
    fn process_context_owns_canonical_event_queue() {
        let mut context = ProcessContext::default();
        assert!(context.event_queue().is_empty());
        context.event_queue_mut().push_back(QueuedEvent {
            what: 1,
            message: 0x1234,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });
        assert_eq!(context.event_queue().len(), 1);
        assert_eq!(context.event_queue().front().unwrap().message, 0x1234);
    }

    #[test]
    fn process_context_owns_canonical_menu_tracking() {
        let mut context = ProcessContext::default();
        assert!(context.menu_tracking().is_none());

        let tracking = crate::menu_manager::test_process_menu_tracking(0x0012_3456);
        context.set_menu_tracking(Some(tracking));
        assert_eq!(
            context.menu_tracking().map(|t| t.menu_handle),
            Some(0x0012_3456)
        );

        if let Some(t) = context.menu_tracking_mut() {
            t.highlighted_item = 3;
        }
        assert_eq!(
            context
                .menu_tracking()
                .map(|t| (t.menu_handle, t.highlighted_item)),
            Some((0x0012_3456, 3))
        );

        let taken = context.take_menu_tracking();
        assert_eq!(taken.map(|t| t.menu_handle), Some(0x0012_3456));
        assert!(context.menu_tracking().is_none());

        context.event_queue_mut().push_back(QueuedEvent {
            what: 2,
            message: 0x5678,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(
            0x0065_4321,
        )));
        assert_eq!(context.event_queue().len(), 1);
        assert_eq!(
            context.menu_tracking().map(|t| t.menu_handle),
            Some(0x0065_4321)
        );
    }

    #[test]
    fn adapters_transfer_pending_state_and_share_one_process_owner() {
        let context = ProcessContext::default();

        let mut classic_selection = SharedNativeMenuSelection::default();
        assert!(classic_selection.stage((128, 2)));
        let mut native_selection = SharedNativeMenuSelection::default();
        context.attach_native_menu_selection(&mut classic_selection);
        context.attach_native_menu_selection(&mut native_selection);
        assert_eq!(native_selection.take(), Some((128, 2)));
        assert!(classic_selection.is_none());

        let mut classic_calls = SharedGuestCallStack::default();
        classic_calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000,
        );
        let mut native_calls = SharedGuestCallStack::default();
        context.attach_guest_calls(&mut classic_calls);
        context.attach_guest_calls(&mut native_calls);
        assert_eq!(native_calls.len(), 1);
        assert!(native_calls.complete_m68k(0x2002, 0x3000));
        assert!(classic_calls.is_empty());
    }

    #[test]
    fn mixed_mode_state_shares_as_one_pair_and_detaches_with_clone() {
        let context = ProcessContext::default();
        let mut first = SharedProcessMixedModeM68kState::default();
        let mut second = SharedProcessMixedModeM68kState::default();

        context.attach_mixed_mode_m68k_state(&mut first);
        first.gateway = 0x1000;
        first.stack_top = 0x20_0000;
        context.attach_mixed_mode_m68k_state(&mut second);

        assert!(first.ptr_eq(&second));
        assert_eq!(second.gateway, 0x1000);
        assert_eq!(second.stack_top, 0x20_0000);

        let mut detached = first.clone();
        detached.gateway = 0x3000;
        detached.stack_top = 0x30_0000;
        assert!(!first.ptr_eq(&detached));
        assert_eq!(first.gateway, 0x1000);
        assert_eq!(first.stack_top, 0x20_0000);
    }

    #[test]
    #[should_panic(expected = "cannot attach two active Menu Manager continuations")]
    fn adopting_two_active_menu_continuations_is_always_rejected() {
        let mut context = ProcessContext::default();
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(
            0x1000,
        )));
        let mut second = SharedProcessMenuTracking::default();
        *second = Some(crate::menu_manager::test_process_menu_tracking(0x2000));
        context.attach_menu_tracking(&mut second);
    }

    #[test]
    #[should_panic(expected = "cannot attach two pending native menu selections")]
    fn attaching_two_pending_native_selections_is_always_rejected() {
        let context = ProcessContext::default();
        let mut first = SharedNativeMenuSelection::default();
        let mut second = SharedNativeMenuSelection::default();
        first.stage((128, 1));
        second.stage((129, 2));
        context.attach_native_menu_selection(&mut first);
        context.attach_native_menu_selection(&mut second);
    }

    #[test]
    #[should_panic(expected = "cannot attach two active guest-procedure continuation stacks")]
    fn attaching_two_active_guest_call_stacks_is_always_rejected() {
        fn begin_call(calls: &SharedGuestCallStack, entry: u32) {
            calls.begin_m68k(
                GuestCallTarget {
                    isa: GuestIsa::M68k,
                    entry,
                    rtoc: 0,
                },
                entry + 2,
                0x3000,
            );
        }

        let context = ProcessContext::default();
        let mut first = SharedGuestCallStack::default();
        let mut second = SharedGuestCallStack::default();
        begin_call(&first, 0x1000);
        begin_call(&second, 0x2000);
        context.attach_guest_calls(&mut first);
        context.attach_guest_calls(&mut second);
    }

    #[test]
    fn native_heap_operations_update_canonical_state_directly() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: ProcessMemoryManager::NO_ERR,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );

        manager.maximize_native_heap();
        manager.request_native_master_pointers();
        manager.set_native_mem_error(ProcessMemoryManager::PARAM_ERR);
        let mut memory = GuestAddressSpace::new();
        memory.add_region(HEAP_BASE, vec![0; 0x1000]);
        assert_eq!(
            manager.reserve_native_bytes(&mut memory, 0x20, true),
            HEAP_BASE
        );

        let heap = manager.native_heap_state().unwrap();
        assert_eq!(heap.heap_cursor, HEAP_BASE + 0x20);
        assert_eq!(heap.last_mem_error, ProcessMemoryManager::PARAM_ERR);
        assert!(heap.heap_maximized);
        assert_eq!(heap.master_pointer_blocks_requested, 1);
    }

    #[test]
    fn native_allocator_attachment_transfers_the_populated_owner() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut target = ProcessMemoryManager::default();
        target.publish_native_allocator(
            native_heap_state(HEAP_BASE, HEAP_BASE + 0x1000),
            &[],
            &[],
            &[],
        );
        let mut source = ProcessMemoryManager::default();
        source.publish_native_allocator(
            native_heap_state(HEAP_BASE + 0x80, HEAP_BASE + 0x2000),
            &[],
            &[],
            &[],
        );
        source.mutate_native_allocator(|allocator| {
            allocator.heap.heap_cursor += 0x20;
            allocator.ptrs.push(ProcessPtrRecord {
                ptr: HEAP_BASE + 0x80,
                size: 0x20,
            });
        });
        let expected = source.native_allocator_snapshot();

        target.adopt_process_memory_manager(&mut source);

        assert_eq!(target.native_allocator_snapshot(), expected);
        assert!(!source.has_native_allocator());
        assert!(source.native_handle_records().is_empty());
    }

    #[test]
    fn native_allocator_attachment_retains_a_populated_target() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut target = ProcessMemoryManager::default();
        target.publish_native_allocator(
            native_heap_state(HEAP_BASE, HEAP_BASE + 0x2000),
            &[],
            &[],
            &[],
        );
        target.mutate_native_allocator(|allocator| {
            allocator.heap.heap_limit -= 0x100;
            allocator.heap.last_mem_error = ProcessMemoryManager::PARAM_ERR;
        });
        let expected = target.native_allocator_snapshot();
        let mut source = ProcessMemoryManager::default();
        source.publish_native_allocator(
            native_heap_state(HEAP_BASE + 0x80, HEAP_BASE + 0x3000),
            &[],
            &[],
            &[],
        );

        target.adopt_process_memory_manager(&mut source);

        assert_eq!(target.native_allocator_snapshot(), expected);
        assert!(!source.has_native_allocator());
    }

    #[test]
    fn native_allocator_attachment_moves_a_pristine_source_into_an_empty_target() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut target = ProcessMemoryManager::default();
        let mut source = ProcessMemoryManager::default();
        let heap = native_heap_state(HEAP_BASE + 0x80, HEAP_BASE + 0x2000);
        source.publish_native_allocator(heap, &[], &[], &[]);

        target.adopt_process_memory_manager(&mut source);

        assert_eq!(target.native_heap_state(), Some(heap));
        assert!(!source.has_native_allocator());
    }

    #[test]
    fn native_allocator_attachment_retains_the_target_when_both_are_pristine() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let target_heap = native_heap_state(HEAP_BASE, HEAP_BASE + 0x1000);
        let source_heap = native_heap_state(HEAP_BASE + 0x80, HEAP_BASE + 0x2000);
        let mut target = ProcessMemoryManager::default();
        target.publish_native_allocator(target_heap, &[], &[], &[]);
        let mut source = ProcessMemoryManager::default();
        source.publish_native_allocator(source_heap, &[], &[], &[]);

        target.adopt_process_memory_manager(&mut source);

        assert_eq!(target.native_heap_state(), Some(target_heap));
        assert!(!source.has_native_allocator());
    }

    #[test]
    fn native_allocator_attachment_rejects_two_populated_owners_atomically() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let first_record = ProcessHandleRecord {
            handle: HEAP_BASE,
            ptr: HEAP_BASE + 0x20,
            size: 16,
            capacity: 16,
        };
        let second_record = ProcessHandleRecord {
            handle: HEAP_BASE + 0x40,
            ptr: HEAP_BASE + 0x60,
            size: 32,
            capacity: 32,
        };
        let mut target = ProcessMemoryManager::default();
        target.publish_native_allocator(
            native_heap_state(HEAP_BASE, HEAP_BASE + 0x2000),
            &[],
            &[],
            &[],
        );
        target.register_native_handle_records([(first_record, 0x80)]);
        let mut source = ProcessMemoryManager::default();
        source.publish_native_allocator(
            native_heap_state(HEAP_BASE + 0x100, HEAP_BASE + 0x3000),
            &[],
            &[],
            &[],
        );
        source.register_native_handle_records([(second_record, 0x40)]);
        let target_allocator = target.native_allocator_snapshot();
        let source_allocator = source.native_allocator_snapshot();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            target.adopt_process_memory_manager(&mut source);
        }));

        assert!(result.is_err());
        assert_eq!(target.native_allocator_snapshot(), target_allocator);
        assert_eq!(source.native_allocator_snapshot(), source_allocator);
        assert_eq!(target.native_allocation(first_record.handle), Some(first_record));
        assert_eq!(source.native_allocation(second_record.handle), Some(second_record));
        assert_eq!(target.handle_for_ptr(first_record.ptr), Some(first_record.handle));
        assert_eq!(source.handle_for_ptr(second_record.ptr), Some(second_record.handle));
        assert_eq!(target.state_for_handle(first_record.handle), Some(0x80));
        assert_eq!(source.state_for_handle(second_record.handle), Some(0x40));
    }

    #[test]
    fn process_memory_manager_handoff_preflights_classic_metadata_before_native_transfer() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut target = ProcessMemoryManager::default();
        target.publish_native_allocator(
            native_heap_state(HEAP_BASE, HEAP_BASE + 0x2000),
            &[],
            &[],
            &[],
        );
        let mut target_bus = MacMemoryBus::new(8 * 1024 * 1024);
        target.attach_classic_memory_bus(&mut target_bus);
        let target_ptr = target.new_classic_ptr(&mut target_bus, 16);

        let mut source = ProcessMemoryManager::default();
        source.publish_native_allocator(
            native_heap_state(HEAP_BASE, HEAP_BASE + 0x2000),
            &[],
            &[],
            &[],
        );
        source.mutate_native_allocator(|allocator| allocator.heap.heap_cursor += 0x20);
        let mut source_bus = MacMemoryBus::new(8 * 1024 * 1024);
        source.attach_classic_memory_bus(&mut source_bus);
        let source_ptr = source.new_classic_ptr(&mut source_bus, 32);
        let target_allocator = target.native_allocator_snapshot();
        let source_allocator = source.native_allocator_snapshot();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            target.adopt_process_memory_manager(&mut source);
        }));

        assert!(result.is_err());
        assert_eq!(target.native_allocator_snapshot(), target_allocator);
        assert_eq!(source.native_allocator_snapshot(), source_allocator);
        assert_eq!(target.classic_allocation_size(target_ptr), Some(16));
        assert_eq!(source.classic_allocation_size(source_ptr), Some(32));
        assert_ne!(target.new_classic_ptr(&mut target_bus, 8), 0);
        assert_ne!(source.new_classic_ptr(&mut source_bus, 8), 0);
    }

    #[test]
    fn process_memory_manager_relocates_native_handle_immediately_through_68k_bus() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let old_ptr = HEAP_BASE + 0x10;
        let heap_cursor = HEAP_BASE + 0x40;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x1000]);
        native.write_u32_be(handle, old_ptr).unwrap();
        native.write_bytes(old_ptr, b"original").unwrap();

        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.register_native_handle_records([(
            ProcessHandleRecord {
                handle,
                ptr: old_ptr,
                size: 8,
                capacity: 16,
            },
            0,
        )]);

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);
        let replacement = vec![0x5a; 48];
        let relocated = manager
            .replace_native_handle_bytes(&mut bus, handle, old_ptr, &replacement)
            .unwrap();

        assert_eq!(relocated, (old_ptr, heap_cursor));
        assert_eq!(bus.read_long(handle), heap_cursor);
        assert_eq!(bus.read_bytes(heap_cursor, replacement.len()), replacement);
        assert_eq!(
            manager.native_allocation(handle),
            Some(ProcessHandleRecord {
                handle,
                ptr: heap_cursor,
                size: 48,
                capacity: 48,
            })
        );
        assert_eq!(
            manager
                .native_allocator_update()
                .map(|allocator| allocator.heap.heap_cursor),
            Some(heap_cursor + 48)
        );
    }

    #[test]
    fn process_handle_resize_updates_native_allocation_through_68k_bus() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let old_ptr = HEAP_BASE + 0x10;
        let heap_cursor = HEAP_BASE + 0x40;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x1000]);
        native.write_u32_be(handle, old_ptr).unwrap();
        native.write_bytes(old_ptr, b"original").unwrap();

        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.register_native_handle_records([(
            ProcessHandleRecord {
                handle,
                ptr: old_ptr,
                size: 8,
                capacity: 16,
            },
            0,
        )]);

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);
        manager.attach_classic_memory_bus(&mut bus);

        assert_eq!(
            manager.set_process_handle_size(&mut bus, handle, 48),
            ProcessMemoryManager::NO_ERR
        );
        assert_eq!(bus.read_long(handle), heap_cursor);
        assert_eq!(bus.read_bytes(heap_cursor, 8), b"original");
        assert_eq!(bus.read_bytes(heap_cursor + 8, 40), vec![0; 40]);
        assert_eq!(
            manager.native_allocation(handle),
            Some(ProcessHandleRecord {
                handle,
                ptr: heap_cursor,
                size: 48,
                capacity: 48,
            })
        );
        assert_eq!(manager.recover_handle(heap_cursor), Some(handle));
        assert_eq!(manager.recover_handle(old_ptr), None);
    }

    #[test]
    fn process_handle_disposal_is_atomic_when_native_master_pointer_is_readonly() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let ptr = HEAP_BASE + 0x20;
        let record = ProcessHandleRecord {
            handle,
            ptr,
            size: 8,
            capacity: 16,
        };
        let mut native = GuestAddressSpace::new();
        native.add_readonly_region(handle, ptr.to_be_bytes().to_vec());
        native.add_region(ptr, b"original".to_vec());

        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE + 0x100,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.register_native_handle_records([(record, 0xE0)]);

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);
        manager.attach_classic_memory_bus(&mut bus);

        assert_eq!(
            manager.dispose_process_handle(&mut bus, handle, true),
            Err(ProcessMemoryManager::NIL_HANDLE_ERR)
        );
        assert_eq!(bus.read_long(handle), ptr);
        assert_eq!(manager.native_allocation(handle), Some(record));
        assert_eq!(manager.recover_handle(ptr), Some(handle));
        assert_eq!(manager.state_for_handle(handle), Some(0xE0));
        assert!(manager
            .native_allocator()
            .is_some_and(|allocator| allocator.free_handle_blocks.is_empty()));
        assert_eq!(
            manager
                .native_allocator_update()
                .map(|allocator| allocator.heap.last_mem_error),
            Some(ProcessMemoryManager::NIL_HANDLE_ERR)
        );
    }

    #[test]
    fn process_memory_manager_preserves_native_handle_when_growth_exhausts_heap() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let old_ptr = HEAP_BASE + 0x10;
        let heap_cursor = HEAP_BASE + 0x40;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x100]);
        native.write_u32_be(handle, old_ptr).unwrap();
        native.write_bytes(old_ptr, b"original").unwrap();

        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor,
                heap_limit: heap_cursor,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let original = ProcessHandleRecord {
            handle,
            ptr: old_ptr,
            size: 8,
            capacity: 16,
        };
        manager.register_native_handle_records([(original, 0)]);

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);
        assert_eq!(
            manager.replace_native_handle_bytes(&mut bus, handle, old_ptr, &[0x5a; 48]),
            Err(ProcessMemoryManager::MEM_FULL_ERR)
        );
        assert_eq!(bus.read_long(handle), old_ptr);
        assert_eq!(bus.read_bytes(old_ptr, 8), b"original");
        assert_eq!(manager.native_allocation(handle), Some(original));
        assert_eq!(
            manager
                .native_allocator_update()
                .map(|allocator| allocator.heap.last_mem_error),
            Some(ProcessMemoryManager::MEM_FULL_ERR)
        );
    }

    #[test]
    fn process_handle_reallocation_failure_preserves_native_process_state() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let old_ptr = HEAP_BASE + 0x10;
        let heap_cursor = HEAP_BASE + 0x40;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x100]);
        native.write_u32_be(handle, old_ptr).unwrap();
        native.write_bytes(old_ptr, b"original").unwrap();

        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor,
                heap_limit: heap_cursor,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let original = ProcessHandleRecord {
            handle,
            ptr: old_ptr,
            size: 8,
            capacity: 16,
        };
        manager.register_native_handle_records([(original, 0xE0)]);

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);
        manager.attach_classic_memory_bus(&mut bus);

        assert_eq!(
            manager.reallocate_process_handle(&mut bus, handle, 32),
            Err(ProcessMemoryManager::MEM_FULL_ERR)
        );
        assert_eq!(bus.read_long(handle), old_ptr);
        assert_eq!(bus.read_bytes(old_ptr, 8), b"original");
        assert_eq!(manager.native_allocation(handle), Some(original));
        assert_eq!(manager.recover_handle(old_ptr), Some(handle));
        assert_eq!(manager.state_for_handle(handle), Some(0xE0));
        assert_eq!(
            manager
                .native_allocator_update()
                .map(|allocator| allocator.heap.last_mem_error),
            Some(ProcessMemoryManager::MEM_FULL_ERR)
        );
    }

    #[test]
    fn native_empty_handle_is_atomic_and_reallocates_through_classic_bus() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let old_ptr = HEAP_BASE + 0x20;
        let heap_cursor = HEAP_BASE + 0x100;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x1000]);
        native.write_u32_be(handle, old_ptr).unwrap();
        native.write_bytes(old_ptr, b"process-owned").unwrap();

        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let original = ProcessHandleRecord {
            handle,
            ptr: old_ptr,
            size: 13,
            capacity: 64,
        };
        manager.register_native_handle_records([(original, 0xE0)]);

        assert_eq!(
            manager.empty_native_handle(&mut native, handle),
            ProcessMemoryManager::MEM_PUR_ERR
        );
        assert_eq!(native.read_u32_be(handle), Some(old_ptr));
        assert_eq!(manager.native_allocation(handle), Some(original));
        assert_eq!(manager.state_for_handle(handle), Some(0xE0));

        manager.set_state_for_handle(handle, 0x60);
        assert_eq!(
            manager.empty_native_handle(&mut native, handle),
            ProcessMemoryManager::NO_ERR
        );
        assert_eq!(native.read_u32_be(handle), Some(0));
        assert_eq!(
            manager.native_allocation(handle),
            Some(ProcessHandleRecord {
                handle,
                ptr: 0,
                size: 0,
                capacity: 0,
            })
        );
        assert_eq!(manager.recover_handle(old_ptr), None);
        assert_eq!(manager.state_for_handle(handle), Some(0x60));
        assert_eq!(
            manager
                .native_allocator()
                .and_then(|allocator| allocator.free_ptr_blocks.last())
                .copied(),
            Some(ProcessPtrRecord {
                ptr: old_ptr,
                size: 64,
            })
        );

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);
        manager.attach_classic_memory_bus(&mut bus);
        assert_eq!(
            manager.reallocate_process_handle(&mut bus, handle, 17),
            Ok((0, old_ptr))
        );
        assert_eq!(bus.read_long(handle), old_ptr);
        assert_eq!(bus.read_bytes(old_ptr, 17), vec![0xA5; 17]);
        assert_eq!(manager.recover_handle(old_ptr), Some(handle));
        assert_eq!(manager.state_for_handle(handle), Some(0x20));
        assert!(manager
            .native_allocator()
            .is_some_and(|allocator| allocator.free_ptr_blocks.is_empty()));
    }

    #[test]
    fn process_memory_manager_allocates_native_ptrs_around_readonly_mappings() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut native = GuestAddressSpace::new();
        native.add_readonly_region(HEAP_BASE, vec![0xcc; 0x30]);
        native
            .add_readonly_allocation_exclusion(HEAP_BASE, 0x30)
            .unwrap();
        native.add_region(HEAP_BASE + 0x30, vec![0x5a; 0x100]);
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE,
                heap_limit: HEAP_BASE + 0x130,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );

        let ptr = manager.new_native_ptr(&mut native, 20, true);

        assert_eq!(ptr, HEAP_BASE + 0x30);
        assert_eq!(native.read_u8(HEAP_BASE), Some(0xcc));
        assert!((0..32).all(|offset| native.read_u8(ptr + offset) == Some(0)));
        assert_eq!(
            manager
                .native_allocator()
                .map(|allocator| allocator.ptrs.as_slice()),
            Some([ProcessPtrRecord { ptr, size: 20 }].as_slice())
        );
        assert_eq!(manager.native_ptr_size(ptr), 20);
        assert_eq!(
            manager.dispose_native_ptr(ptr),
            Some(ProcessPtrRecord { ptr, size: 20 })
        );
        let allocator = manager.native_allocator().unwrap();
        assert!(allocator.ptrs.is_empty());
        assert_eq!(
            allocator.free_ptr_blocks,
            vec![ProcessPtrRecord { ptr, size: 20 }]
        );
    }

    #[test]
    fn native_handle_copy_rejects_readonly_heap_storage_before_commit() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut native = GuestAddressSpace::new();
        native.add_readonly_region(HEAP_BASE, vec![0xcc; 0x100]);
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE,
                heap_limit: HEAP_BASE + 0x100,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );

        assert_eq!(
            manager.copy_bytes_to_new_native_handle(&mut native, b"copy"),
            0
        );
        assert_eq!(native.read_u8(HEAP_BASE), Some(0xcc));
        assert!(manager.native_handle_records().is_empty());
        let allocator = manager.native_allocator().unwrap();
        assert_eq!(allocator.heap.heap_cursor, HEAP_BASE);
        assert!(allocator.free_ptr_blocks.is_empty());
        assert!(allocator.free_handle_blocks.is_empty());
        assert_eq!(
            allocator.heap.last_mem_error,
            ProcessMemoryManager::MEM_FULL_ERR
        );
    }

    #[test]
    fn process_memory_manager_reallocates_native_ptrs_atomically() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x100]);
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE,
                heap_limit: HEAP_BASE + 0x100,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let original = manager.new_native_ptr(&mut native, 8, false);
        native.write_bytes(original, b"payload!").unwrap();
        let detached = manager.detached_clone();

        assert_eq!(
            manager.reallocate_native_ptr(&mut native, original, u32::MAX),
            0
        );
        assert_eq!(
            (0..8)
                .map(|offset| native.read_u8(original + offset))
                .collect::<Option<Vec<_>>>(),
            Some(b"payload!".to_vec())
        );
        assert!(manager
            .native_allocator()
            .unwrap()
            .ptrs
            .iter()
            .any(|record| record.ptr == original));

        let replacement = manager.reallocate_native_ptr(&mut native, original, 24);

        assert_ne!(replacement, 0);
        assert_ne!(replacement, original);
        assert_eq!(
            (0..8)
                .map(|offset| native.read_u8(replacement + offset))
                .collect::<Option<Vec<_>>>(),
            Some(b"payload!".to_vec())
        );
        let allocator = manager.native_allocator().unwrap();
        assert_eq!(
            allocator.ptrs,
            vec![ProcessPtrRecord {
                ptr: replacement,
                size: 24,
            }]
        );
        assert_eq!(
            allocator.free_ptr_blocks,
            vec![ProcessPtrRecord {
                ptr: original,
                size: 8,
            }]
        );
        assert_eq!(
            detached.native_allocator().unwrap().ptrs,
            vec![ProcessPtrRecord {
                ptr: original,
                size: 8,
            }]
        );
    }

    #[test]
    fn process_ptr_disposal_leaves_detached_allocator_independent() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let ptr = HEAP_BASE + 0x20;
        let record = ProcessPtrRecord { ptr, size: 24 };
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE + 0x100,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[record],
            &[],
            &[],
        );
        let detached = manager.detached_clone();
        let mut bus = MacMemoryBus::new(0x20_0000);
        manager.attach_classic_memory_bus(&mut bus);

        assert_eq!(manager.dispose_process_ptr(&mut bus, ptr), Some(record));
        assert_eq!(manager.process_ptr_size(&bus, ptr), None);
        assert_eq!(detached.native_allocator().unwrap().ptrs, vec![record]);
        assert!(detached
            .native_allocator()
            .unwrap()
            .free_ptr_blocks
            .is_empty());
    }

    #[test]
    fn process_heap_tail_reclamation_preserves_unrelated_and_detached_allocations() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let retained_ptr = ProcessPtrRecord {
            ptr: HEAP_BASE + 0x20,
            size: 16,
        };
        let reclaimed_handle = ProcessHandleRecord {
            handle: HEAP_BASE + 0x60,
            ptr: HEAP_BASE + 0x70,
            size: 8,
            capacity: 16,
        };
        let reclaimed_ptr = ProcessPtrRecord {
            ptr: HEAP_BASE + 0x80,
            size: 128,
        };
        let unrelated_free = ProcessPtrRecord {
            ptr: HEAP_BASE + 0x10,
            size: 8,
        };
        let reclaimed_free = ProcessPtrRecord {
            ptr: HEAP_BASE + 0xf0,
            size: 8,
        };
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE + 0x100,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: -108,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[retained_ptr, reclaimed_ptr],
            &[unrelated_free, reclaimed_free],
            &[reclaimed_handle],
        );
        let detached = manager.detached_clone();

        assert!(manager.reclaim_native_heap_tail(
            reclaimed_handle.handle,
            &[reclaimed_ptr.ptr],
            Some(reclaimed_handle.handle),
        ));

        let allocator = manager.native_allocator().unwrap();
        assert_eq!(allocator.heap.heap_cursor, reclaimed_handle.handle);
        assert_eq!(allocator.heap.last_mem_error, ProcessMemoryManager::NO_ERR);
        assert_eq!(allocator.ptrs, vec![retained_ptr]);
        assert_eq!(allocator.free_ptr_blocks, vec![unrelated_free]);
        assert!(allocator.free_handle_blocks.is_empty());

        let detached_allocator = detached.native_allocator().unwrap();
        assert_eq!(detached_allocator.heap.heap_cursor, HEAP_BASE + 0x100);
        assert_eq!(detached_allocator.ptrs, vec![retained_ptr, reclaimed_ptr]);
        assert_eq!(
            detached_allocator.free_ptr_blocks,
            vec![unrelated_free, reclaimed_free]
        );
        assert_eq!(
            detached_allocator.free_handle_blocks,
            vec![reclaimed_handle]
        );
    }

    #[test]
    fn native_transaction_restore_preserves_shared_indexes() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let ptr = ProcessPtrRecord {
            ptr: HEAP_BASE + 0x20,
            size: 24,
        };
        let handle = ProcessHandleRecord {
            handle: HEAP_BASE + 0x40,
            ptr: HEAP_BASE + 0x50,
            size: 16,
            capacity: 16,
        };
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE + 0x80,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[ptr],
            &[],
            &[],
        );
        manager.register_native_handle_records([(handle, 0x40)]);
        let shared_reverse_index = manager.ptr_to_handle.clone();
        let shared_state_index = manager.handle_state_bits.clone();
        let snapshot = manager.detached_clone();

        manager.dispose_native_ptr(ptr.ptr);
        manager.set_state_for_handle(handle.handle, 0x80);
        manager.restore_native_snapshot(snapshot);

        assert_eq!(shared_reverse_index.get(&handle.ptr), Some(handle.handle));
        assert_eq!(shared_state_index.get(&handle.handle), Some(0x40));
        assert_eq!(manager.native_allocator().unwrap().ptrs, vec![ptr]);
        assert!(manager
            .native_allocator()
            .unwrap()
            .free_ptr_blocks
            .is_empty());
    }

    #[test]
    fn transaction_restore_preserves_attached_classic_allocator_state() {
        let mut manager = ProcessMemoryManager::default();
        let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
        manager.attach_classic_memory_bus(&mut bus);
        let original = manager.new_classic_ptr(&mut bus, 24);
        let snapshot = manager.detached_clone();

        manager.dispose_process_ptr(&mut bus, original);
        let replacement = manager.new_classic_ptr(&mut bus, 16);
        assert_eq!(replacement, original);
        manager.restore_native_snapshot(snapshot);

        assert_eq!(manager.classic_allocation_size(original), Some(24));
        assert_eq!(bus.get_alloc_size(original), Some(24));
        let next = manager.new_classic_ptr(&mut bus, 16);
        assert_eq!(next, original + 24);
    }

    #[test]
    fn process_memory_manager_native_allocations_are_immediately_cross_isa_visible() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x1000]);
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);

        let handle = manager.new_native_handle(&mut native, 24, true);
        let record = manager.native_allocation(handle).unwrap();
        native.write_bytes(record.ptr, b"native").unwrap();

        assert_eq!(bus.read_long(handle), record.ptr);
        assert_eq!(bus.read_bytes(record.ptr, 6), b"native");
        bus.write_byte(record.ptr + 6, b'!');
        assert_eq!(native.read_u8(record.ptr + 6), Some(b'!'));
    }

    #[test]
    fn process_memory_manager_copies_and_appends_native_handle_bytes_cross_isa() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x1000]);
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);

        let handle = manager.copy_bytes_to_new_native_handle(&mut native, b"native");
        let original = manager.native_allocation(handle).unwrap();
        assert_eq!(bus.read_bytes(original.ptr, 6), b"native");

        let blocking_ptr = manager.new_native_ptr(&mut native, 16, false);
        assert_ne!(blocking_ptr, 0);
        assert_eq!(
            manager.append_bytes_to_native_handle(&mut native, handle, b" process memory manager",),
            ProcessMemoryManager::NO_ERR
        );

        let appended = manager.native_allocation(handle).unwrap();
        assert_ne!(appended.ptr, original.ptr);
        assert_eq!(bus.read_long(handle), appended.ptr);
        assert_eq!(
            bus.read_bytes(appended.ptr, appended.size as usize),
            b"native process memory manager"
        );
        bus.write_byte(appended.ptr + appended.size - 1, b'!');
        assert_eq!(native.read_u8(appended.ptr + appended.size - 1), Some(b'!'));
    }

    #[test]
    fn process_memory_manager_materializes_native_resources_immediately() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x1000]);
        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);

        let unloaded = manager.new_native_resource_handle(&mut native, None);
        assert_ne!(unloaded, 0);
        assert_eq!(bus.read_long(unloaded), 0);
        assert_eq!(manager.state_for_handle(unloaded), Some(0x60));
        let recycled_ptr = manager.native_allocator().unwrap().free_ptr_blocks[0].ptr;
        let cursor_before_load = manager.native_heap_state().unwrap().heap_cursor;
        let detached = manager.detached_clone();

        assert_eq!(
            manager.load_native_resource_handle(&mut native, unloaded, b"resource"),
            ProcessMemoryManager::NO_ERR
        );
        let loaded = manager.native_allocation(unloaded).unwrap();
        assert_eq!(loaded.ptr, recycled_ptr);
        assert_eq!(
            manager.native_heap_state().unwrap().heap_cursor,
            cursor_before_load
        );
        assert_eq!(manager.recover_handle(loaded.ptr), Some(unloaded));
        assert_eq!(
            bus.read_bytes(loaded.ptr, loaded.size as usize),
            b"resource"
        );
        assert_eq!(manager.state_for_handle(unloaded), Some(0x60));

        let second = manager.new_native_resource_handle(&mut native, Some(b"second"));
        assert_ne!(second, 0);
        assert_ne!(second, unloaded);
        assert_eq!(manager.state_for_handle(second), Some(0x60));
        assert_eq!(manager.native_handle_records().len(), 2);

        assert_eq!(
            detached.native_allocation(unloaded),
            Some(ProcessHandleRecord {
                handle: unloaded,
                ptr: 0,
                size: 0,
                capacity: 0,
            })
        );
        assert_eq!(detached.native_allocation(second), None);
        assert_eq!(detached.state_for_handle(unloaded), Some(0x60));
    }

    #[test]
    fn native_handle_registration_tracks_relocation_without_discarding_classic_handles() {
        let mut manager = ProcessMemoryManager::default();
        manager.track_handle_ptr(0x2200, 0x1100);

        manager.register_native_handle_records([
            (
                ProcessHandleRecord {
                    handle: 0x3300,
                    ptr: 0x4400,
                    size: 16,
                    capacity: 32,
                },
                0x80,
            ),
            (
                ProcessHandleRecord {
                    handle: 0x5500,
                    ptr: 0x6600,
                    size: 48,
                    capacity: 64,
                },
                0x40,
            ),
            (
                ProcessHandleRecord {
                    handle: 0x8800,
                    ptr: 0,
                    size: 0,
                    capacity: 0,
                },
                0x40,
            ),
        ]);
        assert_eq!(manager.handle_for_ptr(0x2200), Some(0x1100));
        assert_eq!(manager.handle_for_ptr(0x4400), Some(0x3300));
        assert_eq!(manager.handle_for_ptr(0x6600), Some(0x5500));
        assert_eq!(manager.handle_state(0x3300), 0x80);
        assert_eq!(manager.handle_state(0x5500), 0x40);
        assert_eq!(manager.native_allocation(0x3300).unwrap().size, 16);
        assert_eq!(manager.native_allocation(0x8800).unwrap().ptr, 0);
        assert_eq!(manager.handle_state(0x8800), 0x40);

        manager.register_native_handle_records([(
            ProcessHandleRecord {
                handle: 0x3300,
                ptr: 0x7700,
                size: 80,
                capacity: 96,
            },
            0xc0,
        )]);
        assert_eq!(manager.handle_for_ptr(0x2200), Some(0x1100));
        assert_eq!(manager.handle_for_ptr(0x4400), None);
        assert_eq!(manager.handle_for_ptr(0x6600), None);
        assert_eq!(manager.handle_for_ptr(0x7700), Some(0x3300));
        assert_eq!(manager.handle_state(0x3300), 0xc0);
        assert_eq!(manager.handle_state(0x5500), 0);
        assert_eq!(manager.native_allocation(0x3300).unwrap().size, 80);
        assert_eq!(manager.native_allocation(0x5500), None);
    }

    #[test]
    fn apple_event_dispatch_prefers_application_exact_and_wildcard_entries() {
        let handlers = SharedProcessAppleEventHandlers::default();
        let wildcard = u32::from_be_bytes(*b"****");
        let event_class = u32::from_be_bytes(*b"aevt");
        let event_id = u32::from_be_bytes(*b"oapp");
        for (is_system, class, id, pointer, refcon) in [
            (true, event_class, event_id, 0x1000, 1),
            (false, wildcard, wildcard, 0x2000, 2),
            (false, event_class, wildcard, 0x3000, 3),
            (false, event_class, event_id, 0x4000, 4),
        ] {
            handlers.install(
                is_system,
                class,
                id,
                ProcessAppleEventHandler {
                    procedure: GuestProcedure::raw_m68k(pointer),
                    refcon,
                },
            );
        }

        assert_eq!(
            handlers
                .handler_for(event_class, event_id, wildcard)
                .map(|handler| (handler.procedure.original_pointer, handler.refcon)),
            Some((0x4000, 4))
        );
        assert!(handlers.remove(false, event_class, event_id, 0));
        assert_eq!(
            handlers
                .handler_for(event_class, event_id, wildcard)
                .map(|handler| (handler.procedure.original_pointer, handler.refcon)),
            Some((0x3000, 3))
        );
        assert!(handlers.remove(false, event_class, wildcard, 0x3000));
        assert!(!handlers.remove(false, wildcard, wildcard, 0x9998));
        assert_eq!(
            handlers
                .handler_for(event_class, event_id, wildcard)
                .map(|handler| (handler.procedure.original_pointer, handler.refcon)),
            Some((0x2000, 2))
        );
    }

    #[test]
    fn attached_apple_event_tables_share_mutations_while_clones_detach() {
        let context = ProcessContext::default();
        let mut classic = SharedProcessAppleEventHandlers::default();
        let mut native = SharedProcessAppleEventHandlers::default();
        context.attach_apple_event_handlers(&mut classic);
        context.attach_apple_event_handlers(&mut native);
        let detached = native.clone();
        let event_class = u32::from_be_bytes(*b"misc");
        let event_id = u32::from_be_bytes(*b"slct");

        classic.install(
            false,
            event_class,
            event_id,
            ProcessAppleEventHandler {
                procedure: GuestProcedure::raw_m68k(0x4000),
                refcon: 0x1234_5678,
            },
        );

        assert_eq!(
            native.get(false, event_class, event_id),
            classic.get(false, event_class, event_id)
        );
        assert_eq!(detached.get(false, event_class, event_id), None);
        assert_eq!(classic.len(), 1);
        assert_eq!(native.len(), 1);
        assert_eq!(detached.len(), 0);
    }

    #[test]
    fn attached_classic_file_maps_share_mutations_while_clones_detach() {
        let context = ProcessContext::default();
        let mut native = SharedProcessFileSystem::default();
        let mut first_data = SharedProcessValue::<ProcessForkMap>::default();
        let mut first_resources = SharedProcessValue::<ProcessForkMap>::default();
        first_data.insert("Existing".to_string(), b"before".to_vec());
        let mut second_data = SharedProcessValue::<ProcessForkMap>::default();
        let mut second_resources = SharedProcessValue::<ProcessForkMap>::default();

        context.attach_classic_file_system(&mut first_data, &mut first_resources);
        context.attach_classic_file_system(&mut second_data, &mut second_resources);
        context.attach_file_system(&mut native);
        let detached_data = second_data.clone();
        let detached_resources = second_resources.clone();

        native.vfs_files.push(ProcessVfsFileRecord {
            path: "Created".to_string(),
            data: b"native".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: true,
        });
        native
            .vfs_resource_files
            .push(ProcessVfsResourceFileRecord {
                path: "Created".to_string(),
                creator: 0,
                file_type: 0,
                finder_flags: 0,
                resource_len: 8,
                raw_data: Some(b"resource".to_vec().into()),
                map_attrs: 0,
                dirty: true,
            });

        second_data
            .get_mut("Existing")
            .unwrap()
            .extend_from_slice(b"-after");
        first_resources.insert("Existing".to_string(), b"resource".to_vec());
        second_resources
            .get_mut("Created")
            .unwrap()
            .extend_from_slice(b"-classic");

        assert!(first_data.ptr_eq(&second_data));
        assert!(first_resources.ptr_eq(&second_resources));
        assert_eq!(first_data.get("Existing").unwrap(), b"before-after");
        assert_eq!(second_data.get("Created").unwrap(), b"native");
        assert_eq!(second_resources.get("Existing").unwrap(), b"resource");
        assert_eq!(
            native.vfs_resource_files[0]
                .raw_data
                .as_ref()
                .unwrap()
                .as_slice(),
            b"resource-classic"
        );
        assert_eq!(detached_data.get("Existing").unwrap(), b"before");
        assert!(!detached_data.contains_key("Created"));
        assert!(detached_resources.is_empty());
    }

    #[test]
    fn attached_classic_catalogues_share_mutations_while_clones_detach() {
        let context = ProcessContext::default();
        let mut first_directories = SharedProcessValue::from_value(vec![ProcessVfsDirectory {
            dir_id: 2,
            parent_dir_id: 1,
            path: String::new(),
            creator: u32::from_be_bytes(*b"MACS"),
            file_type: u32::from_be_bytes(*b"fold"),
            finder_flags: 0,
            dirty: false,
        }]);
        let mut first_metadata =
            SharedProcessValue::<HashMap<String, ProcessVfsMetadata>>::default();
        let mut first_locked_files = SharedProcessValue::<HashSet<String>>::default();
        let mut first_next_dir_id = SharedProcessValue::from_value(16);
        let mut first_next_file_id = SharedProcessValue::from_value(32);
        let mut first_next_timestamp = SharedProcessValue::from_value(1);
        let mut first_default_dir_id = SharedProcessValue::from_value(2);

        context.attach_classic_vfs_catalogue(
            &mut first_directories,
            &mut first_metadata,
            &mut first_locked_files,
            &mut first_next_dir_id,
            &mut first_next_file_id,
            &mut first_next_timestamp,
            &mut first_default_dir_id,
        );

        let mut second_metadata =
            SharedProcessValue::<HashMap<String, ProcessVfsMetadata>>::default();
        let mut second_directories = SharedProcessValue::<Vec<ProcessVfsDirectory>>::default();
        let mut second_locked_files = SharedProcessValue::<HashSet<String>>::default();
        let mut second_next_dir_id = SharedProcessValue::from_value(16);
        let mut second_next_file_id = SharedProcessValue::from_value(32);
        let mut second_next_timestamp = SharedProcessValue::from_value(1);
        let mut second_default_dir_id = SharedProcessValue::from_value(2);
        context.attach_classic_vfs_catalogue(
            &mut second_directories,
            &mut second_metadata,
            &mut second_locked_files,
            &mut second_next_dir_id,
            &mut second_next_file_id,
            &mut second_next_timestamp,
            &mut second_default_dir_id,
        );
        assert!(first_directories.ptr_eq(&second_directories));
        let detached_directories = second_directories.clone();
        let detached_default_dir_id = second_default_dir_id.clone();

        first_directories.push(ProcessVfsDirectory {
            dir_id: 16,
            parent_dir_id: 2,
            path: "Games".to_string(),
            creator: u32::from_be_bytes(*b"TEST"),
            file_type: u32::from_be_bytes(*b"fold"),
            finder_flags: 0x0400,
            dirty: true,
        });
        *first_next_dir_id = 17;
        *first_default_dir_id = 16;

        assert!(second_directories
            .iter()
            .any(|directory| directory.path == "Games"));
        assert_eq!(
            second_directories
                .iter()
                .find(|directory| directory.dir_id == 16)
                .map(|directory| directory.path.as_str()),
            Some("Games")
        );
        assert_eq!(*second_next_dir_id, 17);
        assert_eq!(*second_default_dir_id, 16);
        assert!(!detached_directories
            .iter()
            .any(|directory| directory.path == "Games"));
        assert_eq!(*detached_default_dir_id, 2);
    }

    #[test]
    fn attached_file_systems_share_catalogue_state_while_clones_detach() {
        let context = ProcessContext::default();
        let mut files = SharedProcessFileSystem::default();
        files.vfs_files.push(ProcessVfsFileRecord {
            path: "Existing".to_string(),
            data: b"data".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        let mut first = SharedProcessFileSystem::from_state(ProcessFileSystemState {
            vfs_volumes: SharedProcessValue::from_value(vec![ProcessVfsVolumeRecord {
                ref_num: -1,
                name: "Macintosh HD".to_string(),
                root_dir_id: 2,
                attributes: 0,
                file_count: 1,
                allocation_block_count: 100,
                allocation_block_size: 4096,
                clump_size: 4096,
                free_blocks: 50,
                bitmap_start: 3,
                allocation_pointer: 4,
                allocation_start: 5,
                next_catalog_id: 17,
                created_date: 1,
                modified_date: 2,
            }]),
            vfs_directories: SharedProcessValue::from_value(vec![ProcessVfsDirectory {
                dir_id: 2,
                parent_dir_id: 1,
                path: String::new(),
                creator: 0,
                file_type: 0,
                finder_flags: 0,
                dirty: false,
            }]),
            next_vfs_dir_id: SharedProcessValue::from_value(16),
            default_dir_id: SharedProcessValue::from_value(2),
            ..ProcessFileSystemState::default()
        });
        let mut second = SharedProcessFileSystem::default();

        context.attach_file_system(&mut files);
        context.attach_file_system(&mut first);
        context.attach_file_system(&mut second);
        let detached = second.clone();

        first.vfs_directories.push(ProcessVfsDirectory {
            dir_id: 16,
            parent_dir_id: 2,
            path: "Games".to_string(),
            creator: u32::from_be_bytes(*b"TEST"),
            file_type: u32::from_be_bytes(*b"fold"),
            finder_flags: 0x0400,
            dirty: true,
        });
        first.vfs_volumes[0].file_count = 2;
        *first.next_vfs_dir_id = 17;
        *first.default_dir_id = 16;

        assert!(files.ptr_eq(&first));
        assert!(first.ptr_eq(&second));
        assert_eq!(second.vfs_files[0].data, b"data");
        assert_eq!(second.vfs_directories[1].path, "Games");
        assert_eq!(second.vfs_volumes[0].file_count, 2);
        assert_eq!(second.next_vfs_dir_id, 17);
        assert_eq!(second.default_dir_id, 16);
        assert_eq!(detached.vfs_directories.len(), 1);
        assert_eq!(detached.vfs_volumes[0].file_count, 1);
        assert_eq!(detached.next_vfs_dir_id, 16);
        assert_eq!(detached.default_dir_id, 2);
    }

    #[test]
    fn attached_file_systems_share_launched_application_path_while_detaching_clones() {
        let context = ProcessContext::default();
        let mut source = SharedProcessFileSystem::from_state(ProcessFileSystemState {
            launched_app_path: Some("Apps/Main App".to_string()),
            ..ProcessFileSystemState::default()
        });
        let mut native = SharedProcessFileSystem::default();

        context.attach_file_system(&mut source);
        context.attach_file_system(&mut native);
        assert!(source.ptr_eq(&native));
        assert_eq!(native.launched_app_path.as_deref(), Some("Apps/Main App"));

        let detached_clone = native.clone();
        let detached_snapshot = native.detached_vfs_snapshot();
        native.launched_app_path = Some("Apps/Other App".to_string());

        assert_eq!(source.launched_app_path.as_deref(), Some("Apps/Other App"));
        assert_eq!(
            detached_clone.launched_app_path.as_deref(),
            Some("Apps/Main App")
        );
        assert_eq!(
            detached_snapshot.launched_app_path.as_deref(),
            Some("Apps/Main App")
        );
    }

    #[test]
    #[should_panic(expected = "cannot attach two different launched application paths")]
    fn attaching_file_systems_rejects_conflicting_launched_application_paths() {
        let context = ProcessContext::default();
        let mut first = SharedProcessFileSystem::from_state(ProcessFileSystemState {
            launched_app_path: Some("Apps/Main App".to_string()),
            ..ProcessFileSystemState::default()
        });
        let mut second = SharedProcessFileSystem::from_state(ProcessFileSystemState {
            launched_app_path: Some("Apps/Other App".to_string()),
            ..ProcessFileSystemState::default()
        });

        context.attach_file_system(&mut first);
        context.attach_file_system(&mut second);
    }

    #[test]
    fn attached_file_systems_share_open_stdio_vfs_and_deletion_records_while_clones_detach() {
        let context = ProcessContext::default();
        let mut classic = SharedProcessFileSystem::default();
        classic.files.push(ProcessOpenFileRecord {
            ref_num: 7,
            path: "Classic/first.bin".to_string(),
            position: 3,
        });
        classic.stdio_streams.insert(
            0x1000,
            ProcessStdioStreamRecord {
                ref_num: Some(7),
                path: Some("Classic/first.bin".to_string()),
                position: 3,
                standard: false,
                readable: true,
                writable: false,
                append: false,
                closed: false,
                eof: false,
                error: false,
            },
        );
        classic.vfs_files.push(ProcessVfsFileRecord {
            path: "Classic/first.bin".to_string(),
            data: b"classic".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        classic
            .deleted_vfs_file_paths
            .push("Classic/removed.bin".to_string());

        let mut native = SharedProcessFileSystem::default();
        context.attach_file_system(&mut classic);
        context.attach_file_system(&mut native);
        assert!(classic.ptr_eq(&native));
        let detached = native.clone();

        native.files.push(ProcessOpenFileRecord {
            ref_num: 8,
            path: "Native/second.bin".to_string(),
            position: 11,
        });
        native.stdio_streams.insert(
            0x2000,
            ProcessStdioStreamRecord {
                ref_num: Some(8),
                path: Some("Native/second.bin".to_string()),
                position: 11,
                standard: false,
                readable: true,
                writable: true,
                append: true,
                closed: false,
                eof: false,
                error: false,
            },
        );
        native.vfs_files.push(ProcessVfsFileRecord {
            path: "Native/second.bin".to_string(),
            data: b"native".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: true,
        });
        native
            .deleted_vfs_file_paths
            .push("Native/removed.bin".to_string());

        assert_eq!(classic.files[1].path, "Native/second.bin");
        assert_eq!(classic.stdio_streams[&0x2000].position, 11);
        assert_eq!(classic.vfs_files[1].data, b"native");
        assert_eq!(
            classic.deleted_vfs_file_paths,
            ["Classic/removed.bin", "Native/removed.bin"]
        );

        assert_eq!(detached.files.len(), 1);
        assert_eq!(detached.stdio_streams.len(), 1);
        assert_eq!(detached.vfs_files.len(), 1);
        assert_eq!(detached.deleted_vfs_file_paths, ["Classic/removed.bin"]);
    }

    #[test]
    fn attached_file_systems_share_completion_fifo_while_clones_detach() {
        let first_completion = PendingFileCompletion {
            parameter_block: 0x1000,
            completion_addr: 0x2000,
            result: 0,
        };
        let second_completion = PendingFileCompletion {
            parameter_block: 0x3000,
            completion_addr: 0x4000,
            result: -39,
        };
        let third_completion = PendingFileCompletion {
            parameter_block: 0x5000,
            completion_addr: 0x6000,
            result: -51,
        };

        let mut process_state = ProcessFileSystemState::default();
        process_state.pending_completions.push_back(first_completion);
        let context = ProcessContext::with_file_system(
            SharedProcessFileSystem::from_state(process_state),
        );

        let mut adapter_state = ProcessFileSystemState::default();
        adapter_state
            .pending_completions
            .push_back(second_completion);
        let mut first = SharedProcessFileSystem::from_state(adapter_state);
        let mut second = SharedProcessFileSystem::default();
        context.attach_file_system(&mut first);
        context.attach_file_system(&mut second);

        assert!(first
            .pending_completions
            .ptr_eq(&second.pending_completions));
        second.pending_completions.push_back(third_completion);
        let mut detached = second.clone();

        assert_eq!(first.pending_completions.pop_front(), Some(first_completion));
        assert_eq!(first.pending_completions.pop_front(), Some(second_completion));
        assert_eq!(first.pending_completions.pop_front(), Some(third_completion));
        assert!(second.pending_completions.is_empty());
        assert_eq!(
            detached.pending_completions.pop_front(),
            Some(first_completion)
        );
        assert_eq!(detached.pending_completions.len(), 2);
    }

    #[test]
    fn attaching_populated_file_systems_merges_persistent_catalogues() {
        let mut target_state = ProcessFileSystemState::default();
        target_state.vfs_files.push(ProcessVfsFileRecord {
            path: "Shared".to_string(),
            data: b"classic".to_vec().into(),
            creator: u32::from_be_bytes(*b"CLSC"),
            file_type: u32::from_be_bytes(*b"TEXT"),
            finder_flags: 0,
            dirty: false,
        });
        target_state
            .vfs_resource_files
            .push(ProcessVfsResourceFileRecord {
                path: "Shared".to_string(),
                creator: u32::from_be_bytes(*b"CLSC"),
                file_type: u32::from_be_bytes(*b"APPL"),
                finder_flags: 0,
                resource_len: 16,
                raw_data: Some(b"classic-resource".to_vec().into()),
                map_attrs: 0,
                dirty: false,
            });
        target_state.vfs_resources.push(ProcessVfsResourceRecord {
            ref_num: 2,
            path: "Shared".to_string(),
            res_type: u32::from_be_bytes(*b"TEST"),
            res_id: 128,
            name: b"Target".to_vec(),
            data: b"target".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        let context =
            ProcessContext::with_file_system(SharedProcessFileSystem::from_state(target_state));

        let mut source_state = ProcessFileSystemState::default();
        for (path, data) in [
            ("Shared", b"native".as_slice()),
            ("Native", b"new".as_slice()),
        ] {
            source_state.vfs_files.push(ProcessVfsFileRecord {
                path: path.to_string(),
                data: data.to_vec().into(),
                creator: u32::from_be_bytes(*b"NATV"),
                file_type: u32::from_be_bytes(*b"TEXT"),
                finder_flags: 0,
                dirty: false,
            });
        }
        for (res_id, data) in [(128, b"source".as_slice()), (129, b"new".as_slice())] {
            source_state.vfs_resources.push(ProcessVfsResourceRecord {
                ref_num: 2,
                path: "Shared".to_string(),
                res_type: u32::from_be_bytes(*b"TEST"),
                res_id,
                name: Vec::new(),
                data: data.to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
        }
        let mut native = SharedProcessFileSystem::from_state(source_state);
        context.attach_file_system(&mut native);

        assert_eq!(native.vfs_files.len(), 2);
        assert_eq!(native.vfs_files[0].data, b"classic");
        assert_eq!(native.vfs_files[1].path, "Native");
        assert_eq!(native.vfs_files[1].data, b"new");
        assert_eq!(native.vfs_resources.len(), 2);
        assert_eq!(native.vfs_resources[0].data, b"target");
        assert_eq!(native.vfs_resources[1].res_id, 129);
        assert_eq!(native.vfs_resources[1].data, b"new");
        assert_eq!(
            native.vfs_resource_files.fork("Shared").unwrap(),
            b"classic-resource"
        );
    }

    #[test]
    fn attached_resource_managers_share_state_while_clones_detach() {
        let context = ProcessContext::default();
        let mut native = SharedProcessFileSystem::default();
        let mut first = SharedProcessResourceManager::default();
        *first.current_resource_file = 7;
        first
            .resource_backing_data
            .insert((7, *b"TEST", 128), b"before".to_vec());
        let mut second = SharedProcessResourceManager::default();

        context.attach_resource_manager(&mut first);
        context.attach_resource_manager(&mut second);
        context.attach_file_system(&mut native);
        let detached = second.clone();
        assert_eq!(*second.current_resource_file, 7);
        *second.current_resource_file = 9;
        second
            .resource_backing_data
            .get_mut(&(7, *b"TEST", 128))
            .unwrap()
            .extend_from_slice(b"-after");
        second.resident_resources.insert((7, *b"TEST", 128));
        native.vfs_resources.push(ProcessVfsResourceRecord {
            ref_num: 7,
            path: "Shared".to_string(),
            res_type: u32::from_be_bytes(*b"TEST"),
            res_id: 128,
            name: b"Shared".to_vec(),
            data: b"native".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });

        assert!(first.ptr_eq(&second));
        assert_eq!(*first.current_resource_file, 9);
        assert_eq!(*native.current_resource_file, 9);
        assert_eq!(
            first
                .resource_backing_data
                .get(&(7, *b"TEST", 128))
                .unwrap(),
            b"before-after"
        );
        assert!(first.resident_resources.contains(&(7, *b"TEST", 128)));
        assert_eq!(first.vfs_resources[0].data, b"native");
        assert_eq!(
            detached
                .resource_backing_data
                .get(&(7, *b"TEST", 128))
                .unwrap(),
            b"before"
        );
        assert!(detached.resident_resources.is_empty());
        assert!(detached.vfs_resources.is_empty());
        assert_eq!(*detached.current_resource_file, 7);
    }

    #[test]
    fn attached_sound_managers_share_channels_while_clones_detach() {
        let context = ProcessContext::default();
        let mut classic = SharedProcessSoundManager::default();
        classic
            .channels
            .push(crate::sound::SndChannel::new(0x2000, false));
        let mut native = SharedProcessSoundManager::default();

        context.attach_sound_manager(&mut classic);
        context.attach_sound_manager(&mut native);
        let detached = native.clone();

        native.set_sys_beep_volume(0x0080_0040);
        classic
            .channels
            .push(crate::sound::SndChannel::new(0x3000, false));
        native.play_file_buffer(
            0x4000,
            vec![0x80],
            crate::sound::OUTPUT_RATE << 16,
            Some((
                crate::callback_manager::CallbackTaskArchitecture::PowerPc,
                0x5000,
            )),
        );
        assert_eq!(classic.toggle_file_paused(0x4000), Some(true));
        assert_eq!(native.file_playback_paused(0x4000), Some(true));
        assert_eq!(detached.file_playback_paused(0x4000), None);
        assert_eq!(native.toggle_file_paused(0x4000), Some(false));
        native.mix_frame(1);
        native.double_buffer_playbacks.push(
            crate::sound::ProcessSoundDoubleBufferPlayback {
                channel: 0x6000,
                header: 0x6100,
                buffers: [0x6200, 0x6300],
                callback: 0x6400,
                callback_architecture:
                    crate::callback_manager::CallbackTaskArchitecture::PowerPc,
                sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
                num_channels: 1,
                sample_size: 8,
                compression_id: 0,
                packet_size: 0,
                current_buffer_index: 0,
                callback_pending_mask: 1,
                active: true,
                host_initialized: true,
                host_buffer_loaded: true,
            },
        );
        native.pending_process_doublebacks.push(
            crate::sound::PendingProcessSoundDoubleBack {
                architecture: crate::callback_manager::CallbackTaskArchitecture::PowerPc,
                channel: 0x6000,
                header: 0x6100,
                exhausted_buffer: 0x6200,
                exhausted_buffer_index: 0,
                callback: 0x6400,
                tick: 12,
                instruction_count: 34,
            },
        );
        let playback_snapshot = native.clone();
        classic.quiet_channel(0x6000);

        assert!(classic.ptr_eq(&native));
        assert_eq!(native.channels.len(), 3);
        assert_eq!(native.channels[0].guest_ptr, 0x2000);
        assert_eq!(classic.sys_beep_volume(), 0x0080_0040);
        assert!(matches!(
            classic.pending_sound_callbacks.as_slice(),
            [crate::sound::PendingSoundCallback::FileCompletion {
                architecture: crate::callback_manager::CallbackTaskArchitecture::PowerPc,
                callback_addr: 0x5000,
                chan_ptr: 0x4000,
            }]
        ));
        assert_eq!(detached.channels.len(), 1);
        assert!(detached.pending_sound_callbacks.is_empty());
        assert_eq!(detached.sys_beep_volume(), 0x0100_0100);
        assert!(!native.double_buffer_playbacks[0].active);
        assert!(!native.double_buffer_playbacks[0].host_buffer_loaded);
        assert!(classic.pending_process_doublebacks.is_empty());
        assert!(playback_snapshot.double_buffer_playbacks[0].active);
        assert!(playback_snapshot.double_buffer_playbacks[0].host_buffer_loaded);
        assert_eq!(playback_snapshot.pending_process_doublebacks.len(), 1);
    }

    #[test]
    fn attached_event_queues_share_fifo_and_invalidation_while_clones_detach() {
        let context = ProcessContext::default();
        let mut classic = SharedProcessEventQueue::default();
        classic.push_back(QueuedEvent {
            what: 1,
            message: 0x1111,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });
        let mut native = SharedProcessEventQueue::default();

        context.attach_event_queue(&mut classic);
        context.attach_event_queue(&mut native);
        let detached = native.clone();

        native.push_back(QueuedEvent {
            what: 2,
            message: 0x2222,
            where_v: 30,
            where_h: 40,
            modifiers: 0,
        });
        classic.invalidate_menu_bar();

        assert!(classic.ptr_eq(&native));
        assert_eq!(classic.pop_front().unwrap().message, 0x1111);
        assert_eq!(native.front().unwrap().message, 0x2222);
        assert!(native.take_menu_bar_invalidation());
        assert_eq!(detached.len(), 1);
        assert_eq!(detached.front().unwrap().message, 0x1111);
        assert!(!detached.menu_bar_is_invalid());
    }

    #[test]
    fn attached_input_states_share_immediately_while_clones_detach() {
        let context = ProcessContext::default();
        let mut classic = SharedProcessInputState::default();
        classic.mouse_pos = (12, 34);
        classic.key_map[2] = 0x40;
        let mut native = SharedProcessInputState::default();

        context.attach_input_state(&mut classic);
        context.attach_input_state(&mut native);
        let detached = native.clone();

        native.mouse_button = true;
        native.mouse_pos = (56, 78);
        native.caps_lock_physically_pressed = true;
        native.key_repeat = Some(ProcessKeyRepeatState {
            key_code: 0x24,
            char_code: b'\r',
            next_tick: 90,
        });

        assert!(classic.ptr_eq(&native));
        assert_eq!(classic.mouse_pos, (56, 78));
        assert!(classic.mouse_button);
        assert_eq!(classic.key_map[2], 0x40);
        assert!(classic.caps_lock_physically_pressed);
        assert_eq!(classic.key_repeat.unwrap().next_tick, 90);
        assert_eq!(detached.mouse_pos, (12, 34));
        assert!(!detached.mouse_button);
        assert!(!detached.caps_lock_physically_pressed);
        assert!(detached.key_repeat.is_none());
    }

    #[test]
    fn attached_menu_tracking_is_immediate_while_clones_detach() {
        let context = ProcessContext::default();
        let mut classic = SharedProcessMenuTracking::default();
        *classic = Some(crate::menu_manager::test_process_menu_tracking(0x1234));
        let mut native = SharedProcessMenuTracking::default();

        context.attach_menu_tracking(&mut classic);
        context.attach_menu_tracking(&mut native);
        let detached = native.clone();

        classic.as_mut().unwrap().highlighted_item = 4;

        assert!(classic.ptr_eq(&native));
        assert_eq!(native.as_ref().unwrap().highlighted_item, 4);
        assert_eq!(detached.as_ref().unwrap().highlighted_item, 1);
        assert_eq!(native.take().unwrap().menu_handle, 0x1234);
        assert!(classic.is_none());
        assert_eq!(detached.as_ref().unwrap().menu_handle, 0x1234);
    }

    #[test]
    fn attached_window_lists_share_order_immediately_while_clones_detach() {
        let context = ProcessContext::default();
        let mut classic = SharedProcessWindowList::from_value(vec![0x1000, 0x2000]);
        let mut native = SharedProcessWindowList::default();

        context.attach_window_list(&mut classic);
        context.attach_window_list(&mut native);
        let detached = native.clone();

        native.remove(1);
        native.insert(0, 0x3000);
        classic.push(0x4000);

        assert!(classic.ptr_eq(&native));
        assert_eq!(&*classic, &[0x3000, 0x1000, 0x4000]);
        assert_eq!(&*native, &[0x3000, 0x1000, 0x4000]);
        assert_eq!(&*detached, &[0x1000, 0x2000]);
    }

    #[test]
    fn attached_cursor_states_share_immediately_while_clones_detach() {
        let context = ProcessContext::default();
        let mut classic = SharedProcessCursorState::default();
        let mut native = SharedProcessCursorState::default();
        context.attach_cursor_state(&mut classic);
        context.attach_cursor_state(&mut native);
        let detached = native.clone();
        let mut data = [0; 32];
        data[0] = 0x80;
        let mut mask = [0; 32];
        mask[0] = 0xc0;

        native.hide();
        classic.install(CursorImage::mono(data, mask, 3, 4));

        assert!(classic.ptr_eq(&native));
        assert_eq!(classic.level, -1);
        assert_eq!(
            native.image.as_ref().unwrap().mono_parts(),
            (data, mask, 3, 4)
        );
        assert_eq!(detached.level, 0);
        assert_eq!(
            detached.image.as_ref().unwrap().mono_parts(),
            crate::display::default_arrow_cursor()
        );
    }
}
