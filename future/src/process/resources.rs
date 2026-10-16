//! Process-owned parsed resource maps, search order and file-scoped handle identity.
use super::{Process, ResourceInfo};
use crate::{
    load::resource::{Resource, ResourceFork},
    memory::{GuestPtr, Memory},
    Error, Result,
};
use std::collections::BTreeMap;
pub(super) const APPLICATION: i16 = i16::MIN;
pub(super) struct FileMap {
    node: u32,
    fork: ResourceFork,
    bytes: usize,
    writable: bool,
}
pub(super) struct Maps {
    pub(super) current: i16,
    chain: Vec<i16>,
    files: BTreeMap<i16, FileMap>,
}
impl Default for Maps {
    fn default() -> Self {
        Self {
            current: APPLICATION,
            chain: vec![APPLICATION],
            files: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Use,
    Close,
    Home,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    OpenSpec {
        spec: GuestPtr,
        permission: i8,
    },
    Open {
        name: GuestPtr,
        volume: i16,
        permission: i8,
    },
    Use(i16),
    Close(i16),
    Home(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddResource {
    pub handle: u32,
    pub kind: [u8; 4],
    pub id: i16,
    pub name: GuestPtr,
}
impl AddResource {
    pub fn validate(self, memory: &Memory) -> Result<()> {
        if self.name.0 != 0 {
            let n = memory.read(self.name, 1)?[0] as usize;
            memory.read(self.name, n + 1)?;
        }
        memory.validate_write(GuestPtr(0xa60), 2)
    }
}
impl Process {
    pub fn add_resource(&mut self, memory: &Memory, request: AddResource) -> Result<()> {
        request.validate(memory)?;
        if request.handle == 0
            || self.resource_handles.values().any(|&h| h == request.handle)
            || !self.handles.contains_key(&request.handle)
        {
            self.res_error = -194;
            return Ok(());
        }
        let owner = self.resource_files.current;
        let Some(map) = self.resource_files.files.get(&owner) else {
            self.res_error = if owner == APPLICATION { -54 } else { -193 };
            return Ok(());
        };
        if !map.writable || map.fork.attributes & 0x80 != 0 {
            self.res_error = -54;
            return Ok(());
        }
        let block = self.handles[&request.handle];
        if memory.u32(GuestPtr(request.handle))? != block.data {
            return Err(Error::Cpu("guest-modified resource master pointer".into()));
        }
        let data = if block.size == 0 {
            Vec::new()
        } else {
            memory
                .read(GuestPtr(block.data), block.size as usize)?
                .to_vec()
        };
        let name = if request.name.0 == 0 {
            None
        } else {
            let n = memory.read(request.name, 1)?[0] as usize;
            if n == 0 {
                None
            } else {
                Some(memory.read(request.name, n + 1)?[1..].to_vec())
            }
        };
        let mut candidate = map.fork.clone();
        candidate.insert_unique(Resource {
            kind: request.kind,
            id: request.id,
            attributes: 2,
            name,
            data,
        })?;
        candidate.attributes |= 0x20;
        let size = candidate.encode()?.len();
        if self
            .resource_files
            .files
            .values()
            .map(|m| m.bytes)
            .sum::<usize>()
            - map.bytes
            + size
            > crate::load::IMAGE_LIMIT
        {
            self.res_error = -108;
            return Ok(());
        }
        let node = map.node;
        if let Err(error) = self.files.reserve_resource_growth(node, size) {
            self.res_error = error;
            return Ok(());
        }
        let map = self.resource_files.files.get_mut(&owner).unwrap();
        map.fork = candidate;
        map.bytes = size;
        self.handles.get_mut(&request.handle).unwrap().attributes |= 0x20;
        self.resource_handles
            .insert((owner, request.kind, request.id), request.handle);
        self.res_error = 0;
        Ok(())
    }
    fn resource_map(&self, owner: i16) -> Option<&ResourceFork> {
        if owner == APPLICATION {
            self.resource_files
                .chain
                .contains(&APPLICATION)
                .then_some(&self.resources)
        } else {
            self.resource_files.files.get(&owner).map(|m| &m.fork)
        }
    }
    pub(super) fn resource_metadata(
        &self,
        owner: i16,
        kind: [u8; 4],
        id: i16,
    ) -> Option<&Resource> {
        self.resource_map(owner)?.get(kind, id)
    }
    fn resource_owners(&self, current_only: bool) -> Vec<i16> {
        let current = self.resource_files.current;
        if current_only {
            return vec![current];
        }
        self.resource_files
            .chain
            .iter()
            .position(|o| *o == current)
            .map_or_else(Vec::new, |at| self.resource_files.chain[at..].to_vec())
    }
    pub fn resource_entries(&self) -> Vec<&Resource> {
        self.resource_owners(false)
            .into_iter()
            .filter_map(|o| self.resource_map(o))
            .flat_map(|m| m.entries())
            .collect()
    }
    pub fn find_resource(&self, kind: [u8; 4], id: i16) -> Option<&Resource> {
        self.resource_owners(false)
            .into_iter()
            .find_map(|o| self.resource_metadata(o, kind, id))
    }
    pub fn get_named_resource(
        &mut self,
        memory: &mut Memory,
        kind: [u8; 4],
        name: GuestPtr,
        current_only: bool,
    ) -> Result<u32> {
        let n = memory.read(name, 1)?[0] as usize;
        let bytes = memory.read(name, n + 1)?[1..].to_vec();
        self.get_named_resource_bytes(memory, kind, &bytes, current_only)
    }
    pub(crate) fn get_named_resource_bytes(
        &mut self,
        memory: &mut Memory,
        kind: [u8; 4],
        bytes: &[u8],
        current_only: bool,
    ) -> Result<u32> {
        let found = self
            .resource_owners(current_only)
            .into_iter()
            .find_map(|o| {
                self.resource_map(o)?
                    .entries()
                    .find(|r| r.kind == kind && r.name.as_deref() == Some(bytes))
                    .map(|r| (o, r.id))
            });
        if let Some((o, id)) = found {
            self.get_owned_resource(memory, o, kind, id)
        } else {
            self.resource_miss(kind, current_only);
            Ok(0)
        }
    }
    fn resource_miss(&mut self, kind: [u8; 4], current_only: bool) {
        self.res_error = if (kind == *b"CURS"
            && (!current_only || self.resource_files.current == 0))
            || self.resource_owners(current_only).into_iter().any(|o| {
                self.resource_map(o)
                    .is_some_and(|m| m.entries().any(|r| r.kind == kind))
            }) {
            -192
        } else {
            0
        };
    }
    pub fn get_current_resource(
        &mut self,
        memory: &mut Memory,
        kind: [u8; 4],
        id: i16,
    ) -> Result<u32> {
        let o = self.resource_files.current;
        if self.resource_metadata(o, kind, id).is_some()
            || (o == 0 && kind == *b"CURS" && crate::cursor::standard(id).is_some())
        {
            self.get_owned_resource(memory, o, kind, id)
        } else {
            self.resource_miss(kind, true);
            Ok(0)
        }
    }
    pub fn get_resource(&mut self, memory: &mut Memory, kind: [u8; 4], id: i16) -> Result<u32> {
        if let Some(o) = self
            .resource_owners(false)
            .into_iter()
            .find(|o| self.resource_metadata(*o, kind, id).is_some())
        {
            return self.get_owned_resource(memory, o, kind, id);
        }
        if (kind == *b"CURS" && crate::cursor::standard(id).is_some())
            || (kind == *b"clut" && crate::color_tables::standard(id).is_some())
        {
            return self.get_owned_resource(memory, 0, kind, id);
        }
        self.resource_miss(kind, false);
        Ok(0)
    }
    fn get_owned_resource(
        &mut self,
        memory: &mut Memory,
        owner: i16,
        kind: [u8; 4],
        id: i16,
    ) -> Result<u32> {
        self.res_error = 0;
        if let Some(h) = self.resource_handles.get(&(owner, kind, id)) {
            return Ok(*h);
        }
        let (contents, attributes) = if let Some(r) = self.resource_metadata(owner, kind, id) {
            if r.attributes & 1 != 0 {
                return Err(Error::Load(
                    "compressed resource decoding not implemented".into(),
                ));
            }
            (r.data.clone(), r.attributes)
        } else if owner == 0 {
            let contents = if kind == *b"CURS" {
                crate::cursor::standard(id).map(|bytes| bytes.to_vec())
            } else if kind == *b"clut" {
                crate::color_tables::standard(id)
            } else {
                None
            };
            (
                contents.ok_or_else(|| Error::Load("missing builtin resource".into()))?,
                0,
            )
        } else {
            return Err(Error::Load("resource identity has no map entry".into()));
        };
        let h = self.new_handle(memory, contents.len() as u32, false)?;
        if h == 0 {
            self.res_error = self.mem_error;
            return Ok(0);
        }
        let data = memory.u32(GuestPtr(h))?;
        memory.write(GuestPtr(data), &contents)?;
        let block = self.handles.get_mut(&h).unwrap();
        block.locked = attributes & 0x10 != 0;
        block.attributes = 0x20 | if attributes & 0x20 != 0 { 0x40 } else { 0 };
        self.resource_handles.insert((owner, kind, id), h);
        Ok(h)
    }
    fn owner_reference(&self, owner: i16) -> i16 {
        if owner == APPLICATION {
            self.resource_ref
        } else {
            owner
        }
    }
    fn owner_for_reference(&self, reference: i16) -> Option<i16> {
        if reference == 0 {
            Some(0)
        } else if reference == self.resource_ref && self.resource_files.chain.contains(&APPLICATION)
        {
            Some(APPLICATION)
        } else {
            self.resource_files
                .files
                .contains_key(&reference)
                .then_some(reference)
        }
    }
    pub fn resource_file(&mut self, memory: &mut Memory, op: Op) -> Result<i16> {
        memory.validate_write(GuestPtr(0xa60), 2)?;
        if !matches!(op, Op::Home(_)) {
            memory.validate_write(GuestPtr(0xa5a), 2)?;
        }
        self.res_error = 0;
        let result = match op {
            Op::OpenSpec { spec, permission } => {
                match self.files.resource_spec_target(memory, spec)? {
                    Ok(node) => self.open_resource_node(memory, node, permission),
                    Err(error) => {
                        self.res_error = error;
                        Ok(-1)
                    }
                }
            }
            Op::Open {
                name,
                volume,
                permission,
            } => self.open_resource_file(memory, name, volume, permission),
            Op::Use(reference) => {
                if let Some(owner) = self.owner_for_reference(reference) {
                    self.resource_files.current = owner;
                } else {
                    self.res_error = -193;
                }
                Ok(0)
            }
            Op::Home(handle) => {
                let owner = self
                    .resource_handles
                    .iter()
                    .find(|(_, h)| **h == handle)
                    .map(|(k, _)| k.0);
                if let Some(o) = owner {
                    Ok(self.owner_reference(o))
                } else {
                    self.res_error = -192;
                    Ok(-1)
                }
            }
            Op::Close(reference) => {
                let old = self.mem_error;
                let result = self.close_resource_file(memory, reference);
                self.mem_error = old;
                result
            }
        };
        if result.is_ok() && !matches!(op, Op::Home(_)) {
            memory.write(GuestPtr(0xa5a), &self.current_resource_file().to_be_bytes())?;
        }
        result
    }
    fn open_resource_file(
        &mut self,
        memory: &mut Memory,
        name: GuestPtr,
        volume: i16,
        permission: i8,
    ) -> Result<i16> {
        let node = match self.files.resource_target(memory, name, volume) {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                self.res_error = e;
                return Ok(-1);
            }
            Err(e) => return Err(e),
        };
        self.open_resource_node(memory, node, permission)
    }
    fn open_resource_node(
        &mut self,
        memory: &mut Memory,
        node: u32,
        permission: i8,
    ) -> Result<i16> {
        if !(0..=4).contains(&permission) {
            self.res_error = -50;
            return Ok(-1);
        }
        let mut already_open = false;
        for &o in &self.resource_files.chain {
            let (n, writable) = if o == APPLICATION {
                (16, false)
            } else {
                let f = &self.resource_files.files[&o];
                (f.node, f.writable)
            };
            already_open |= n == node;
            if n == node && (permission == 0 || (permission >= 2 && writable)) {
                return Ok(self.owner_reference(o));
            }
        }
        let bytes = self.files.resource_bytes(node).unwrap_or(&[]);
        let size = bytes.len();
        if size < 16 {
            self.res_error = -39;
            return Ok(-1);
        }
        if self.resource_files.files.len() >= 64
            || self
                .resource_files
                .files
                .values()
                .map(|m| m.bytes)
                .sum::<usize>()
                .saturating_add(size)
                > crate::load::IMAGE_LIMIT
        {
            self.res_error = -108;
            return Ok(-1);
        }
        let fork = match ResourceFork::parse(bytes) {
            Ok(f) => f,
            Err(_) => {
                self.res_error = -199;
                return Ok(-1);
            }
        };
        let preloads = fork
            .entries()
            .filter(|r| r.attributes & 4 != 0)
            .map(|r| (r.kind, r.id))
            .collect::<Vec<_>>();
        let (reference, writable) = match self.files.open_resource_access(node, permission as u8) {
            Ok(v) => v,
            Err(e) => {
                self.res_error = if e == -45 { -54 } else { e };
                return Ok(-1);
            }
        };
        self.resource_files.files.insert(
            reference,
            FileMap {
                node,
                fork,
                bytes: size,
                writable,
            },
        );
        for (kind, id) in preloads {
            let loaded = self.get_owned_resource(memory, reference, kind, id);
            if !matches!(loaded,Ok(h) if h!=0) {
                let err = self.res_error;
                let mem = self.mem_error;
                self.remove_resource_map(memory, reference)?;
                self.res_error = err;
                self.mem_error = mem;
                return match loaded {
                    Err(e) => Err(e),
                    _ => Ok(-1),
                };
            }
        }
        self.resource_files.chain.insert(0, reference);
        if !already_open {
            self.resource_files.current = reference;
        }
        self.res_error = 0;
        Ok(reference)
    }
    fn remove_resource_map(&mut self, memory: &mut Memory, owner: i16) -> Result<()> {
        self.files
            .validate_resource_access(self.owner_reference(owner))?;
        let handles = self
            .resource_handles
            .iter()
            .filter(|(k, _)| k.0 == owner)
            .map(|(_, h)| *h)
            .collect::<Vec<_>>();
        // Preflight every attached allocation before disposing any of them.
        for h in &handles {
            self.handle_size(memory, *h)?;
            memory.validate_write(GuestPtr(*h), 4)?;
        }
        for h in handles {
            self.dispose_handle(memory, h)?;
        }
        self.files
            .close_resource_access(self.owner_reference(owner))?;
        if owner == APPLICATION {
            self.resources = ResourceFork::default();
        } else {
            self.resource_files.files.remove(&owner);
        }
        self.resource_files.chain.retain(|o| *o != owner);
        if self.resource_files.current == owner {
            self.resource_files.current = self.resource_files.chain.first().copied().unwrap_or(0);
        }
        Ok(())
    }
    fn close_resource_file(&mut self, memory: &mut Memory, reference: i16) -> Result<i16> {
        if reference == 0 {
            return Ok(0);
        }
        let Some(owner) = self.owner_for_reference(reference) else {
            self.res_error = -193;
            return Ok(0);
        };
        if self
            .resource_map(owner)
            .is_some_and(|m| m.attributes & 0x20 != 0 || m.entries().any(|r| r.attributes & 2 != 0))
        {
            return Err(Error::Load(
                "closing changed resource maps requires WriteResource/UpdateResFile".into(),
            ));
        }
        self.remove_resource_map(memory, owner)?;
        Ok(0)
    }
    /// IM: More Macintosh Toolbox, ReleaseResource. Released handles
    /// lose resource identity; a later lookup loads the original fork bytes.
    pub fn release_resource(&mut self, memory: &mut Memory, handle: u32) -> Result<()> {
        self.res_error = 0;
        let Some((&(owner, kind, id), _)) =
            self.resource_handles.iter().find(|(_, h)| **h == handle)
        else {
            self.res_error = -192;
            return Ok(());
        };
        if self
            .resource_metadata(owner, kind, id)
            .is_some_and(|r| r.attributes & 2 != 0)
        {
            return Ok(()); // resChanged resources must remain resident.
        }
        self.dispose_handle(memory, handle)
    }
    /// DetachResource retains the owned master/data blocks while severing
    /// resource identity. Changed resources remain attached with resAttrErr.
    pub fn detach_resource(&mut self, memory: &Memory, handle: u32) -> Result<()> {
        self.res_error = 0;
        let Some((&key, _)) = self.resource_handles.iter().find(|(_, h)| **h == handle) else {
            self.res_error = -192;
            return Ok(());
        };
        if self
            .resource_metadata(key.0, key.1, key.2)
            .is_some_and(|r| r.attributes & 2 != 0)
        {
            self.res_error = -198;
            return Ok(());
        }
        let old_error = self.mem_error;
        let checked = self.handle_size(memory, handle);
        self.mem_error = old_error;
        checked?;
        self.resource_handles.remove(&key);
        self.handles.get_mut(&handle).unwrap().attributes &= !0x20;
        Ok(())
    }
    /// Read metadata from resource identity, never from mutable payload bytes.
    pub fn resource_info(&mut self, memory: &mut Memory, request: ResourceInfo) -> Result<()> {
        let Some((&(owner, kind, id), _)) = self
            .resource_handles
            .iter()
            .find(|(_, h)| **h == request.handle)
        else {
            self.res_error = -192;
            return Ok(());
        };
        let old_error = self.mem_error;
        let checked = self.handle_size(memory, request.handle);
        self.mem_error = old_error;
        checked?;
        let name = self
            .resource_metadata(owner, kind, id)
            .and_then(|r| r.name.as_deref())
            .unwrap_or(&[]);
        let mut pascal = Vec::with_capacity(name.len() + 1);
        pascal.push(name.len() as u8);
        pascal.extend_from_slice(name);
        for (pointer, length) in [
            (request.id, 2),
            (request.kind, 4),
            (request.name, pascal.len()),
        ] {
            if pointer.0 != 0 {
                memory.validate_write(pointer, length)?;
            }
        }
        // Snapshot all metadata before outputs can overlap each other or a handle.
        if request.id.0 != 0 {
            memory.write(request.id, &id.to_be_bytes())?;
        }
        if request.kind.0 != 0 {
            memory.write(request.kind, &kind)?;
        }
        if request.name.0 != 0 {
            memory.write(request.name, &pascal)?;
        }
        self.res_error = 0;
        Ok(())
    }
}
