//! Process-local directories/files, immutable loaded application forks and independent
//! access-path marks. Host paths never enter File Manager service implementations.
use crate::{
    memory::{GuestPtr, Memory},
    Result,
};
use std::collections::BTreeMap;
mod aliases;
mod browse;
pub use aliases::ResolveAliasFile;
mod io;
mod resource;
mod spec;
pub use spec::MakeSpec;
mod volume;
mod working;
pub use browse::{CatalogEntry, DirectoryView};
pub use io::Op;
pub use volume::IMAGE_LIMIT as VOLUME_IMAGE_LIMIT;
pub use working::WorkingOp;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fork {
    Data,
    Resource,
}
struct Directory {
    name: Vec<u8>,
    parent: u32,
    created: u32,
}
struct File {
    name: Vec<u8>,
    parent: u32,
    data: Vec<u8>,
    resource: Vec<u8>,
    created: u32,
    modified: u32,
    finder: [u8; 16],
}
struct FileView<'a> {
    name: &'a [u8],
    parent: u32,
    data: &'a [u8],
    resource: Option<&'a [u8]>,
    locked: bool,
    created: u32,
    modified: u32,
    finder: [u8; 16],
}
#[derive(Clone, Copy)]
struct OpenFork {
    resource_map: bool,
    node: u32,
    writable: bool,
    shared: bool,
    kind: Fork,
    mark: u32,
}
#[derive(Default)]
pub struct Files {
    name: Vec<u8>,
    data: Vec<u8>,
    resource: Option<Vec<u8>>,
    open: BTreeMap<i16, OpenFork>,
    next_ref: i16,
    files: BTreeMap<u32, File>,
    directories: BTreeMap<u32, Directory>,
    next_node: u32,
    revision: u64,
    working: BTreeMap<i16, (u32, u32)>,
    default_directory: working::DefaultDirectory,
}
impl Files {
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn application(data: &[u8], resource: Option<&[u8]>) -> Self {
        Self {
            name: b"Application".to_vec(),
            data: data.to_vec(),
            resource: resource.map(Vec::from),
            open: BTreeMap::new(),
            next_ref: 1,
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
            next_node: 17,
            revision: 0,
            working: BTreeMap::new(),
            default_directory: working::DefaultDirectory::default(),
        }
    }
    /// A classic HFS Str31, passed as raw Macintosh bytes by the host boundary.
    pub fn set_name(&mut self, name: &[u8]) -> std::result::Result<(), i16> {
        if self.name.is_empty() {
            return Err(-43);
        }
        if name.is_empty() || name.len() > 31 || name.contains(&b':') || name.contains(&0) {
            return Err(-37);
        }
        self.name = name.to_vec();
        Ok(())
    }
    pub fn fork_bytes(&self, kind: Fork) -> Option<&[u8]> {
        if self.name.is_empty() {
            return None;
        }
        match kind {
            Fork::Data => Some(&self.data),
            Fork::Resource => self.resource.as_deref(),
        }
    }
    pub fn open(&mut self, kind: Fork) -> std::result::Result<i16, i16> {
        if self.fork_bytes(kind).is_none() {
            return Err(-43);
        }
        self.open_node(16, kind, false, false)
    }
    fn open_node(
        &mut self,
        node: u32,
        kind: Fork,
        writable: bool,
        shared: bool,
    ) -> std::result::Result<i16, i16> {
        if self.open.len() >= 256 {
            return Err(-42);
        }
        while self.open.contains_key(&self.next_ref) {
            self.next_ref = if self.next_ref == i16::MAX {
                1
            } else {
                self.next_ref + 1
            };
        }
        let reference = self.next_ref;
        self.next_ref = if self.next_ref == i16::MAX {
            1
        } else {
            self.next_ref + 1
        };
        self.open.insert(
            reference,
            OpenFork {
                resource_map: false,
                node,
                kind,
                mark: 0,
                writable,
                shared,
            },
        );
        Ok(reference)
    }
    fn file(&self, id: u32) -> Option<FileView<'_>> {
        if id == 16 && !self.name.is_empty() {
            Some(FileView {
                name: &self.name,
                parent: 2,
                data: &self.data,
                resource: self.resource.as_deref(),
                locked: true,
                created: 0,
                modified: 0,
                finder: [0; 16],
            })
        } else {
            self.files.get(&id).map(|f| FileView {
                name: &f.name,
                parent: f.parent,
                data: &f.data,
                resource: Some(&f.resource),
                locked: false,
                created: f.created,
                modified: f.modified,
                finder: f.finder,
            })
        }
    }
    fn node_fork(&self, id: u32, kind: Fork) -> Option<&[u8]> {
        let file = self.file(id)?;
        match kind {
            Fork::Data => Some(file.data),
            Fork::Resource => file.resource,
        }
    }
    fn children(&self, parent: u32, files_only: bool) -> Vec<u32> {
        let mut result = Vec::new();
        if parent == 2 && !self.name.is_empty() {
            result.push(16);
        }
        result.extend(
            self.files
                .iter()
                .filter_map(|(&id, f)| (f.parent == parent).then_some(id)),
        );
        if !files_only {
            result.extend(
                self.directories
                    .iter()
                    .filter_map(|(&id, d)| (d.parent == parent).then_some(id)),
            );
        }
        result.sort_unstable();
        result
    }
    fn directory_exists(&self, id: u32) -> bool {
        id == 2 || self.directories.contains_key(&id)
    }
    fn child(&self, parent: u32, name: &[u8]) -> Option<u32> {
        if parent == 2 && name.eq_ignore_ascii_case(&self.name) {
            return Some(16);
        }
        if let Some((&id, _)) = self
            .files
            .iter()
            .find(|(_, f)| f.parent == parent && name.eq_ignore_ascii_case(&f.name))
        {
            return Some(id);
        }
        self.directories.iter().find_map(|(&id, d)| {
            (d.parent == parent && name.eq_ignore_ascii_case(&d.name)).then_some(id)
        })
    }
    fn path(&self, name: &[u8], parent: u32) -> std::result::Result<u32, i16> {
        if name.contains(&0) {
            return Err(-37);
        }
        let (mut node, parts) = if name.starts_with(b":") {
            (parent, &name[1..])
        } else if let Some(colon) = name.iter().position(|&c| c == b':') {
            if !name[..colon].eq_ignore_ascii_case(b"Application Volume") {
                return Err(-35);
            }
            (2, &name[colon + 1..])
        } else {
            (parent, name)
        };
        if !self.directory_exists(node) {
            return Err(-120);
        }
        if parts.is_empty() {
            return Ok(node);
        }
        let mut components = parts.split(|&c| c == b':').peekable();
        while let Some(component) = components.next() {
            if component.is_empty() {
                if components.peek().is_none() {
                    if !self.directory_exists(node) {
                        return Err(-120);
                    }
                    break;
                }
                node = self.directories.get(&node).map_or(1, |d| d.parent);
                if node == 1 {
                    return Err(-120);
                }
            } else {
                if component.len() > 31 {
                    return Err(-37);
                }
                if !self.directory_exists(node) {
                    return Err(-120);
                }
                node = self
                    .child(node, component)
                    .ok_or(if components.peek().is_some() {
                        -120i16
                    } else {
                        -43i16
                    })?;
            }
        }
        Ok(node)
    }
    fn name_input(memory: &Memory, pointer: GuestPtr) -> Result<Vec<u8>> {
        if pointer.0 == 0 {
            return Ok(Vec::new());
        }
        let length = usize::from(memory.read(pointer, 1)?[0]);
        let data = GuestPtr(
            pointer
                .0
                .checked_add(1)
                .ok_or_else(|| crate::Error::Cpu("catalog name overflow".into()))?,
        );
        Ok(memory.read(data, length)?.to_vec())
    }
    /// Synchronous PBDirCreate, entirely process-local; no host filesystem writes.
    pub fn create_directory(&mut self, memory: &mut Memory, pointer: GuestPtr) -> Result<i16> {
        let mut pb = memory.read(pointer, 52)?.to_vec();
        memory.validate_write(pointer, 52)?;
        let volume = i16::from_be_bytes(pb[22..24].try_into().unwrap());
        let parent = u32::from_be_bytes(pb[48..52].try_into().unwrap());
        let parent = self.parent(volume, parent);
        let mut name = Self::name_input(
            memory,
            GuestPtr(u32::from_be_bytes(pb[18..22].try_into().unwrap())),
        )?;
        if name.last() == Some(&b':') {
            name.pop();
        }
        let split = name.iter().rposition(|&c| c == b':');
        let leaf = split.map_or(name.as_slice(), |at| &name[at + 1..]);
        let target = if self.name.is_empty() {
            Err(-43)
        } else if let Err(error) = parent {
            Err(error)
        } else if leaf.is_empty() || leaf.len() > 31 || leaf.contains(&0) {
            Err(-37)
        } else {
            let parent = parent.unwrap();
            let resolved = match split {
                Some(0) => Ok(parent),
                Some(at) => self.path(&name[..=at], parent),
                None => Ok(parent),
            };
            resolved.and_then(|parent| {
                if !self.directory_exists(parent) {
                    Err(-120)
                } else if self.child(parent, leaf).is_some() {
                    Err(-48)
                } else if self.directories.len() + self.files.len() >= 1024
                    || self.next_node == u32::MAX
                {
                    Err(-33)
                } else {
                    Ok(parent)
                }
            })
        };
        let error: i16 = match target {
            Ok(parent) => {
                let id = self.next_node;
                let next = id + 1;
                let created = memory.u32(GuestPtr(0x20c))?; // shared Macintosh Time global.
                self.directories.insert(
                    id,
                    Directory {
                        name: leaf.to_vec(),
                        parent,
                        created,
                    },
                );
                self.next_node = next;
                self.changed();
                pb[48..52].copy_from_slice(&id.to_be_bytes());
                0
            }
            Err(error) => error,
        };
        pb[12..16].fill(0);
        pb[16..18].copy_from_slice(&error.to_be_bytes());
        memory.write(pointer, &pb)?;
        Ok(error)
    }
    fn fill_file_info(&self, pb: &mut [u8], node: u32) {
        let file = self.file(node).expect("checked file node");
        let reference = self
            .open
            .iter()
            .find_map(|(&r, f)| (f.node == node).then_some(r))
            .unwrap_or(0);
        pb[24..26].copy_from_slice(&reference.to_be_bytes());
        pb[30] = self.open.values().filter(|f| f.node == node).fold(
            u8::from(file.locked),
            |flags, fork| {
                flags
                    | 0x80
                    | match fork.kind {
                        Fork::Resource => 4,
                        Fork::Data => 8,
                    }
            },
        );
        pb[32..48].copy_from_slice(&file.finder);
        pb[48..52].copy_from_slice(&node.to_be_bytes());
        let data = file.data.len() as u32;
        let resource = file.resource.map_or(0, |bytes| bytes.len() as u32);
        pb[52..54].copy_from_slice(&(if data == 0 { 0u16 } else { 1 }).to_be_bytes());
        pb[62..64].copy_from_slice(
            &(if resource == 0 {
                0u16
            } else {
                1 + data.div_ceil(4096) as u16
            })
            .to_be_bytes(),
        );
        for (at, value) in [
            (54, data),
            (58, data.div_ceil(4096) * 4096),
            (64, resource),
            (68, resource.div_ceil(4096) * 4096),
        ] {
            pb[at..at + 4].copy_from_slice(&value.to_be_bytes());
        }
        pb[72..76].copy_from_slice(&file.created.to_be_bytes());
        pb[76..80].copy_from_slice(&file.modified.to_be_bytes());
    }
    /// PB[H]GetFInfo projection of the same virtual fork state.
    pub fn file_info(
        &self,
        memory: &mut Memory,
        pointer: GuestPtr,
        hierarchical: bool,
    ) -> Result<i16> {
        let header = memory.read(pointer, 52)?.to_vec();
        let volume = i16::from_be_bytes(header[22..24].try_into().unwrap());
        let index = i16::from_be_bytes(header[28..30].try_into().unwrap());
        let parent = if hierarchical {
            u32::from_be_bytes(header[48..52].try_into().unwrap())
        } else {
            0
        };
        let parent = if hierarchical {
            self.parent(volume, parent)
        } else {
            self.classic_parent(volume)
        };
        let name_ptr = GuestPtr(u32::from_be_bytes(header[18..22].try_into().unwrap()));
        let input_name = if parent.is_ok() && !self.name.is_empty() && index <= 0 {
            Self::name_input(memory, name_ptr)?
        } else {
            Vec::new()
        };
        let selected = parent.and_then(|parent| {
            if self.name.is_empty() {
                Err(-43)
            } else if index > 0 {
                if !self.directory_exists(parent) {
                    Err(-120)
                } else {
                    self.children(parent, true)
                        .get(index as usize - 1)
                        .copied()
                        .ok_or(-43)
                }
            } else {
                self.path(&input_name, parent).and_then(|id| {
                    if self.file(id).is_some() {
                        Ok(id)
                    } else {
                        Err(-43)
                    }
                })
            }
        });
        let error: i16 = selected.as_ref().err().copied().unwrap_or(0);
        memory.validate_write(GuestPtr(pointer.0 + 12), 6)?;
        if error != 0 {
            memory.write(GuestPtr(pointer.0 + 12), &[0; 4])?;
            memory.write(GuestPtr(pointer.0 + 16), &error.to_be_bytes())?;
            return Ok(error);
        }
        let mut pb = memory.read(pointer, 80)?.to_vec();
        memory.validate_write(pointer, 80)?;
        let node = selected.unwrap();
        let file = self.file(node).unwrap();
        let output_name =
            name_ptr.0 != 0 && (index > 0 || self.open.values().any(|f| f.node == node));
        let mut name = vec![file.name.len() as u8];
        name.extend(file.name);
        if output_name {
            memory.validate_write(name_ptr, name.len())?;
        }
        pb[12..18].fill(0);
        self.fill_file_info(&mut pb, node);
        if output_name {
            memory.write(name_ptr, &name)?;
        }
        memory.write(pointer, &pb)?;
        Ok(0)
    }
    /// Catalog queries for all process-local files and directories.
    pub fn catalog_info(&self, memory: &mut Memory, pointer: GuestPtr) -> Result<i16> {
        let header = memory.read(pointer, 52)?.to_vec();
        let volume = i16::from_be_bytes(header[22..24].try_into().unwrap());
        let index = i16::from_be_bytes(header[28..30].try_into().unwrap());
        let directory = u32::from_be_bytes(header[48..52].try_into().unwrap());
        let name_ptr = GuestPtr(u32::from_be_bytes(header[18..22].try_into().unwrap()));
        let parent = self.parent(
            volume,
            if directory == 0 && matches!(volume, -1 | 1) {
                2
            } else {
                directory
            },
        );
        let input_name = if parent.is_ok() && !self.name.is_empty() && index == 0 {
            Self::name_input(memory, name_ptr)?
        } else {
            Vec::new()
        };
        let selected = parent.and_then(|parent| {
            if self.name.is_empty() {
                Err(-43)
            } else if index < 0 {
                if self.directory_exists(parent) {
                    Ok(parent)
                } else {
                    Err(-120)
                }
            } else if index == 0 {
                self.path(&input_name, parent)
            } else if !self.directory_exists(parent) {
                Err(-120)
            } else {
                self.children(parent, false)
                    .get(index as usize - 1)
                    .copied()
                    .ok_or(-43)
            }
        });
        let error: i16 = selected.as_ref().err().copied().unwrap_or(0);
        let node = selected.unwrap_or(2);
        let root = self.file(node).is_none();
        memory.validate_write(GuestPtr(pointer.0 + 12), 6)?;
        if error != 0 {
            memory.write(GuestPtr(pointer.0 + 12), &[0; 4])?;
            memory.write(GuestPtr(pointer.0 + 16), &error.to_be_bytes())?;
            return Ok(error);
        }
        let size = if root { 104 } else { 108 };
        let mut pb = memory.read(pointer, size)?.to_vec();
        memory.validate_write(pointer, size)?;
        let name = if node == 2 {
            b"Application Volume".as_slice()
        } else if root {
            &self.directories[&node].name
        } else {
            self.file(node).unwrap().name
        };
        let mut output_name = vec![name.len() as u8];
        output_name.extend(name);
        if name_ptr.0 != 0 {
            memory.validate_write(name_ptr, output_name.len())?;
        }
        pb[12..18].fill(0);
        pb[22..24].copy_from_slice(&(-1i16).to_be_bytes());
        let reference = if root {
            0
        } else {
            self.open
                .iter()
                .find_map(|(&r, f)| (f.node == node).then_some(r))
                .unwrap_or(0)
        };
        pb[24..26].copy_from_slice(&reference.to_be_bytes());
        pb[30] = if root {
            0x10
        } else {
            self.open.values().filter(|f| f.node == node).fold(
                u8::from(self.file(node).unwrap().locked),
                |flags, fork| {
                    flags
                        | 0x80
                        | match fork.kind {
                            Fork::Resource => 4,
                            Fork::Data => 8,
                        }
                },
            )
        };
        pb[31] = 0;
        pb[32..48].fill(0); // Finder info: virtual files have no imported Finder metadata.
        pb[48..52].copy_from_slice(&node.to_be_bytes());
        if root {
            let children = self.children(node, false).len();
            pb[52..54].copy_from_slice(&(children as u16).to_be_bytes());
        } else {
            self.fill_file_info(&mut pb, node);
            pb[104..108].copy_from_slice(&4096u32.to_be_bytes());
        }
        pb[80..100].fill(0);
        if root {
            pb[72..80].fill(0);
        }
        let parent = if node == 2 {
            1
        } else if root {
            self.directories[&node].parent
        } else {
            self.file(node).unwrap().parent
        };
        pb[100..104].copy_from_slice(&parent.to_be_bytes());
        if root && node != 2 {
            let date = self.directories[&node].created.to_be_bytes();
            pb[72..76].copy_from_slice(&date);
            pb[76..80].copy_from_slice(&date);
        }
        // Every output validated before either guest destination changes.
        if name_ptr.0 != 0 {
            memory.write(name_ptr, &output_name)?;
        }
        memory.write(pointer, &pb)?;
        Ok(0)
    }
    /// PBGetFCBInfo synchronous contract, IM: Files (1992), pp. 2-237–2-238.
    pub fn fcb_info(&self, memory: &mut Memory, pointer: GuestPtr) -> Result<i16> {
        let mut pb: [u8; 62] = memory.read(pointer, 62)?.try_into().unwrap();
        memory.validate_write(pointer, 62)?;
        let word = |at| i16::from_be_bytes([pb[at], pb[at + 1]]);
        let index = word(28);
        let volume = word(22);
        let reference = word(24);
        let selected = if index == 0 {
            self.open.get_key_value(&reference)
        } else if index > 0 && self.parent(volume, 0).is_ok() {
            self.open.iter().nth(index as usize - 1)
        } else {
            None
        };
        let error = if index < 0 {
            -50
        } else if index > 0 && self.parent(volume, 0).is_err() {
            -35
        } else if selected.is_none() {
            -51
        } else {
            0
        };
        pb[16..18].copy_from_slice(&(error as i16).to_be_bytes());
        if let Some((&reference, file)) = selected.filter(|_| error == 0) {
            let view = self.file(file.node).unwrap();
            let length = self.node_fork(file.node, file.kind).unwrap().len() as u32;
            let physical = length.div_ceil(4096) * 4096;
            let name_ptr = u32::from_be_bytes(pb[18..22].try_into().unwrap());
            let mut name = vec![view.name.len() as u8];
            name.extend(view.name);
            if name_ptr != 0 {
                memory.validate_write(GuestPtr(name_ptr), name.len())?;
            }
            pb[24..26].copy_from_slice(&reference.to_be_bytes());
            pb[32..36].copy_from_slice(&file.node.to_be_bytes()); // virtual catalog node
            let flags = if view.locked { 0x2000u16 } else { 0 }
                | if file.writable { 0x0100 } else { 0 }
                | if file.kind == Fork::Resource {
                    0x200
                } else {
                    0
                };
            pb[36..38].copy_from_slice(&flags.to_be_bytes());
            let block = if length == 0 {
                0u16
            } else if file.kind == Fork::Data {
                1
            } else {
                1 + (view.data.len() as u32).div_ceil(4096) as u16
            };
            pb[38..40].copy_from_slice(&block.to_be_bytes());
            for (at, value) in [
                (40, length),
                (44, physical),
                (48, file.mark),
                (54, 4096),
                (58, view.parent),
            ] {
                pb[at..at + 4].copy_from_slice(&value.to_be_bytes());
            }
            pb[52..54].copy_from_slice(&(-1i16).to_be_bytes());
            // Validate both destinations before writing either one.
            if name_ptr != 0 {
                memory.write(GuestPtr(name_ptr), &name)?;
            }
        }
        memory.write(pointer, &pb)?;
        Ok(error as i16)
    }
}
