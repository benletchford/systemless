//! One process-owned application resource map and heap, shared by both ABIs.
//! Memory Manager contracts: Inside Macintosh: Memory (1992), pp. 2-29–2-40.
//! Resource lookup: More Macintosh Toolbox (1993), GetResource, pp. 1-69–1-70.
use crate::{
    load::resource::ResourceFork,
    memory::{GuestPtr, Memory, Permissions},
    Error, Result,
};
use std::collections::{BTreeMap, BTreeSet};
pub const HEAP_BASE: u32 = 0x1000_0000;
pub const HEAP_SIZE: u32 = 4 * 1024 * 1024;
const MEM_FULL: i16 = -108;
const NIL_HANDLE: i16 = -109;
const MEM_BAD: i16 = -111;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceInfo {
    pub handle: u32,
    pub id: GuestPtr,
    pub kind: GuestPtr,
    pub name: GuestPtr,
}
#[derive(Default)]
pub struct Process {
    pub resources: ResourceFork,
    pub(crate) fragments: crate::cfm::Connections,
    pub files: crate::files::Files,
    pub dialogs: crate::dialogs::Dialogs,
    pub sound: crate::sound::Manager,
    pub devices: crate::devices::Manager,
    pub cursor: crate::cursor::Cursor,
    pub input: crate::events::Input,
    pub menus: crate::menus::Menus,
    pub polygons: crate::polygons::Manager,
    pub textedit: crate::textedit::Manager,
    pub fonts: crate::font::Manager,
    pub windows: crate::windows::Windows,
    pub controls: crate::controls::Controls,
    pub ports: Option<crate::quickdraw::Ports>,
    resource_ref: i16,
    pub mem_error: i16,
    pub res_error: i16,
    heap: Option<Heap>,
    reservation: Option<(u32, u32)>,
    handles: BTreeMap<u32, HandleBlock>,
    free_masters: Vec<u32>,
    pooled_masters: BTreeSet<u32>,
    pointers: BTreeMap<u32, u32>,
    resource_handles: BTreeMap<([u8; 4], i16), u32>,
}
#[derive(Clone, Copy)]
struct HandleBlock {
    data: u32,
    size: u32,
    locked: bool,
}
#[derive(Clone)]
struct Heap {
    free: BTreeMap<u32, u32>,
    allocated: BTreeMap<u32, u32>,
}
impl Heap {
    fn allocate(&mut self, size: u32) -> Option<u32> {
        let extent = size.max(1).checked_add(3)? & !3;
        let (&base, &available) = self.free.iter().find(|(_, n)| **n >= extent)?;
        self.free.remove(&base);
        if available > extent {
            self.free.insert(base + extent, available - extent);
        }
        self.allocated.insert(base, extent);
        Some(base)
    }
    fn resize_in_place(&mut self, base: u32, size: u32) -> bool {
        let extent = (size.max(1) + 3) & !3;
        let old = self.allocated[&base];
        if extent == old {
            return true;
        }
        if extent < old {
            self.allocated.insert(base, extent);
            self.allocated.insert(base + extent, old - extent);
            self.release(base + extent);
            return true;
        }
        let end = base + old;
        let needed = extent - old;
        let Some(&available) = self.free.get(&end) else {
            return false;
        };
        if available < needed {
            return false;
        }
        self.free.remove(&end);
        if available > needed {
            self.free.insert(end + needed, available - needed);
        }
        self.allocated.insert(base, extent);
        true
    }
    fn release(&mut self, base: u32) -> bool {
        let Some(mut size) = self.allocated.remove(&base) else {
            return false;
        };
        let mut base = base;
        if let Some((&previous, &n)) = self.free.range(..base).next_back() {
            if previous + n == base {
                self.free.remove(&previous);
                base = previous;
                size += n;
            }
        }
        if let Some((&next, &n)) = self.free.range(base..).next() {
            if base + size == next {
                self.free.remove(&next);
                size += n;
            }
        }
        self.free.insert(base, size);
        true
    }
}
impl Process {
    /// The loader currently owns one application resource map per process.
    /// Reference numbers are process-local; additional files will receive distinct IDs.
    pub fn install_application_resources(&mut self, resources: ResourceFork) {
        self.resources = resources;
        self.resource_ref = 1;
    }
    pub(crate) fn install_files(&mut self, data: &[u8], resource: Option<&[u8]>) {
        self.files = crate::files::Files::application(data, resource);
        self.resource_ref = if resource.is_some() {
            self.files
                .open(crate::files::Fork::Resource)
                .expect("fresh application fork")
        } else {
            0
        };
    }
    pub fn current_resource_file(&self) -> i16 {
        self.resource_ref
    }

    fn heap_base(&self) -> u32 {
        self.reservation.map_or(HEAP_BASE, |r| r.0)
    }
    fn heap_capacity(&self) -> u32 {
        self.reservation.map_or(HEAP_SIZE, |r| r.1)
    }
    /// CODE applications place their heap below the A5 world. Preparation has
    /// not run guest code or allocated any handles, so no live pointer moves.
    pub(crate) fn initialize_code_heap(&mut self, memory: &mut Memory, end: u32) -> Result<()> {
        const BASE: u32 = 0x0001_0000;
        let size = end
            .checked_sub(BASE)
            .filter(|n| *n >= 256)
            .ok_or_else(|| Error::Load("CODE heap reservation is too small".into()))?;
        if self.heap.as_ref().is_some_and(|h| !h.allocated.is_empty()) {
            return Err(Error::Load(
                "cannot relocate a live application heap".into(),
            ));
        }
        memory.validate_write(GuestPtr(0x114), 4)?;
        memory.validate_write(GuestPtr(0x130), 4)?;
        memory.map(GuestPtr(BASE), vec![0; size as usize], Permissions::RWX)?;
        if self.heap.is_some() {
            memory.remove_mappings(&[self.heap_base()]);
        }
        self.reservation = Some((BASE, size));
        self.heap = Some(Heap {
            free: BTreeMap::from([(BASE, size)]),
            allocated: BTreeMap::new(),
        });
        memory.write(GuestPtr(0x114), &end.to_be_bytes())?;
        memory.write(GuestPtr(0x130), &end.to_be_bytes())?;
        Ok(())
    }
    fn ensure_heap(&mut self, memory: &mut Memory) -> Result<()> {
        if self.heap.is_none() {
            memory.map(
                GuestPtr(self.heap_base()),
                vec![0; self.heap_capacity() as usize],
                // Classic applications execute resource-loaded code and
                // generated thunks from heap RAM on both architectures.
                Permissions::RWX,
            )?;
            self.heap = Some(Heap {
                free: BTreeMap::from([(self.heap_base(), self.heap_capacity())]),
                allocated: BTreeMap::new(),
            });
        }
        Ok(())
    }
    /// Loaded applications reserve their complete fixed heap before execution.
    pub(crate) fn initialize_heap(&mut self, memory: &mut Memory) -> Result<()> {
        memory.validate_write(GuestPtr(0x114), 4)?;
        memory.validate_write(GuestPtr(0x130), 4)?;
        self.ensure_heap(memory)?;
        let end = self.heap_base() + self.heap_capacity();
        memory.write(GuestPtr(0x114), &end.to_be_bytes())?;
        memory.write(GuestPtr(0x130), &end.to_be_bytes())?;
        Ok(())
    }
    pub fn maximize_heap(&mut self, memory: &mut Memory) -> Result<()> {
        self.application_limit(memory)?;
        memory.validate_write(GuestPtr(0x114), 4)?;
        self.ensure_heap(memory)?;
        // The current allocator eagerly grows to its reservation. Lowering
        // ApplLimit cannot cut an already-grown zone back (IM: Memory 2-85).
        let end = self.heap_base() + self.heap_capacity();
        memory.write(GuestPtr(0x114), &end.to_be_bytes())?;
        self.mem_error = 0;
        Ok(())
    }
    pub fn application_limit(&self, memory: &Memory) -> Result<u32> {
        let limit = memory.u32(GuestPtr(0x130))?;
        if limit < self.heap_base() || limit > self.heap_base() + self.heap_capacity() {
            return Err(Error::Cpu(
                "application limit is outside its heap reservation".into(),
            ));
        }
        Ok(limit)
    }
    pub fn set_application_limit(&mut self, memory: &mut Memory, limit: u32) -> Result<()> {
        memory.validate_write(GuestPtr(0x130), 4)?;
        self.ensure_heap(memory)?;
        self.mem_error = MEM_FULL;
        if limit < self.heap_base() || limit > self.heap_base() + self.heap_capacity() {
            return Ok(());
        }
        // A lower growth limit does not invalidate live blocks or shrink an
        // already-expanded zone. The reservation remains mapped and owned.
        memory.write(GuestPtr(0x130), &limit.to_be_bytes())?;
        self.mem_error = 0;
        Ok(())
    }
    /// Reserve a nonrelocatable block of 64 master pointers (IM: Memory,
    /// MoreMasters). Slots remain reserved when their handles are disposed.
    pub fn more_masters(&mut self, memory: &mut Memory) -> Result<()> {
        self.ensure_heap(memory)?;
        self.mem_error = MEM_FULL;
        let Some(base) = self.heap.as_mut().unwrap().allocate(64 * 4) else {
            return Ok(());
        };
        memory.write(GuestPtr(base), &[0; 64 * 4])?;
        for index in (0..64).rev() {
            let slot = base + index * 4;
            self.free_masters.push(slot);
            self.pooled_masters.insert(slot);
        }
        self.mem_error = 0;
        Ok(())
    }
    /// Nonrelocatable allocations share extents with handles, but have distinct
    /// ownership: a handle master or data address is never a disposable Ptr.
    pub fn new_pointer(&mut self, memory: &mut Memory, size: u32, clear: bool) -> Result<u32> {
        self.ensure_heap(memory)?;
        self.mem_error = MEM_FULL;
        if size > self.heap_capacity() {
            return Ok(0);
        }
        let Some(pointer) = self.heap.as_mut().unwrap().allocate(size) else {
            return Ok(0);
        };
        if clear {
            memory.write(GuestPtr(pointer), &vec![0; size as usize])?;
        }
        self.pointers.insert(pointer, size);
        self.mem_error = 0;
        Ok(pointer)
    }
    pub fn pointer_size(&mut self, pointer: u32) -> u32 {
        match self.pointers.get(&pointer) {
            Some(size) => {
                self.mem_error = 0;
                *size
            }
            None => {
                self.mem_error = MEM_BAD;
                MEM_BAD as i32 as u32
            }
        }
    }
    pub fn set_pointer_size(&mut self, pointer: u32, size: u32) {
        self.pointer_size(pointer);
        if self.mem_error != 0 {
            return;
        }
        if size > self.heap_capacity()
            || !self.heap.as_mut().unwrap().resize_in_place(pointer, size)
        {
            self.mem_error = MEM_FULL;
            return;
        }
        self.pointers.insert(pointer, size);
    }
    pub fn dispose_pointer(&mut self, pointer: u32) {
        if self.pointers.remove(&pointer).is_none() {
            self.mem_error = MEM_BAD;
            return;
        }
        self.heap.as_mut().unwrap().release(pointer);
        self.mem_error = 0;
    }
    pub fn new_handle(&mut self, memory: &mut Memory, size: u32, clear: bool) -> Result<u32> {
        self.ensure_heap(memory)?;
        self.mem_error = MEM_FULL;
        if size > self.heap_capacity() {
            return Ok(0);
        }
        let heap = self.heap.as_mut().unwrap();
        let Some(data) = heap.allocate(size) else {
            return Ok(0);
        };
        let Some(master) = self.free_masters.pop().or_else(|| heap.allocate(4)) else {
            heap.release(data);
            return Ok(0);
        };
        if clear {
            memory.write(GuestPtr(data), &vec![0; size as usize])?;
        }
        memory.write(GuestPtr(master), &data.to_be_bytes())?;
        self.handles.insert(
            master,
            HandleBlock {
                data,
                size,
                locked: false,
            },
        );
        self.mem_error = 0;
        Ok(master)
    }
    pub fn handle_size(&mut self, memory: &Memory, handle: u32) -> Result<u32> {
        let Some(&HandleBlock { data, size, .. }) = self.handles.get(&handle) else {
            self.mem_error = if handle == 0 { NIL_HANDLE } else { MEM_BAD };
            return Ok(0);
        };
        if memory.u32(GuestPtr(handle))? != data {
            return Err(Error::Cpu(
                "guest-modified master pointer is not an owned heap block".into(),
            ));
        }
        self.mem_error = 0;
        Ok(size)
    }
    pub fn set_handle_lock(&mut self, memory: &Memory, handle: u32, locked: bool) -> Result<()> {
        self.handle_size(memory, handle)?;
        if self.mem_error == 0 {
            self.handles.get_mut(&handle).unwrap().locked = locked;
        }
        Ok(())
    }
    pub fn set_handle_size(&mut self, memory: &mut Memory, handle: u32, size: u32) -> Result<()> {
        let old_size = self.handle_size(memory, handle)?;
        if self.mem_error != 0 {
            return Ok(());
        }
        if size > self.heap_capacity() {
            self.mem_error = MEM_FULL;
            return Ok(());
        }
        let HandleBlock { data, locked, .. } = self.handles[&handle];
        // The heap is one checked writable mapping. Validate it before changing
        // allocator metadata or master pointers, including the relocation case.
        memory.validate_write(GuestPtr(self.heap_base()), self.heap_capacity() as usize)?;
        let heap = self.heap.as_mut().unwrap();
        if heap.resize_in_place(data, size) {
            self.handles.get_mut(&handle).unwrap().size = size;
            return Ok(());
        }
        if locked {
            self.mem_error = MEM_FULL;
            return Ok(());
        }
        let preserved = memory
            .read(GuestPtr(data), old_size.min(size) as usize)?
            .to_vec();
        let Some(replacement) = heap.allocate(size) else {
            self.mem_error = MEM_FULL;
            return Ok(());
        };
        memory.write(GuestPtr(replacement), &preserved)?;
        memory.write(GuestPtr(handle), &replacement.to_be_bytes())?;
        self.handles.insert(
            handle,
            HandleBlock {
                data: replacement,
                size,
                locked,
            },
        );
        heap.release(data);
        Ok(())
    }
    /// Validate a sequence of handle resizes against a private allocator copy.
    /// Callers can preflight compound graphics operations without publishing
    /// heap metadata or changing their guest error authority.
    pub(crate) fn can_resize_handles(
        &mut self,
        memory: &Memory,
        sizes: &[(u32, u32)],
    ) -> Result<bool> {
        if sizes.is_empty() {
            return Ok(true);
        }
        let saved = self.mem_error;
        let result = (|| {
            for &(h, _) in sizes {
                self.handle_size(memory, h)?;
                if self.mem_error != 0 {
                    return Ok(false);
                };
                memory.validate_write(GuestPtr(h), 4)?;
            }
            memory.validate_write(GuestPtr(self.heap_base()), self.heap_capacity() as usize)?;
            let Some(mut heap) = self.heap.clone() else {
                return Ok(sizes.is_empty());
            };
            let mut handles = self.handles.clone();
            for &(h, size) in sizes {
                if size > self.heap_capacity() {
                    return Ok(false);
                }
                let block = handles.get_mut(&h).unwrap();
                if !heap.resize_in_place(block.data, size) {
                    if block.locked {
                        return Ok(false);
                    }
                    let Some(data) = heap.allocate(size) else {
                        return Ok(false);
                    };
                    heap.release(block.data);
                    block.data = data;
                }
                block.size = size;
            }
            Ok(true)
        })();
        self.mem_error = saved;
        result
    }
    /// Stage a group of payload/size changes without changing live allocation
    /// metadata. Every master keeps its identity; locked blocks cannot move.
    pub(crate) fn replace_handles(
        &mut self,
        memory: &mut Memory,
        changes: Vec<(u32, Vec<u8>)>,
    ) -> Result<()> {
        if changes.is_empty() {
            self.mem_error = 0;
            return Ok(());
        }
        let mut unique = std::collections::BTreeSet::new();
        for (handle, bytes) in &changes {
            if !unique.insert(*handle) {
                return Err(Error::Load("duplicate staged handle".into()));
            }
            self.handle_size(memory, *handle)?;
            if self.mem_error != 0 {
                return Ok(());
            }
            if bytes.len() > self.heap_capacity() as usize {
                self.mem_error = MEM_FULL;
                return Ok(());
            }
            memory.validate_write(GuestPtr(*handle), 4)?;
        }
        memory.validate_write(GuestPtr(self.heap_base()), self.heap_capacity() as usize)?;
        let mut staged = self.heap.as_ref().unwrap().clone();
        let mut placements = Vec::new();
        for (handle, bytes) in changes {
            let block = self.handles[&handle];
            let size = bytes.len() as u32;
            let data = if staged.resize_in_place(block.data, size) {
                block.data
            } else {
                if block.locked {
                    self.mem_error = MEM_FULL;
                    return Ok(());
                }
                let Some(data) = staged.allocate(size) else {
                    self.mem_error = MEM_FULL;
                    return Ok(());
                };
                staged.release(block.data);
                data
            };
            placements.push((handle, data, bytes));
        }
        for (handle, data, bytes) in placements {
            memory.write(GuestPtr(data), &bytes)?;
            memory.write(GuestPtr(handle), &data.to_be_bytes())?;
            let block = self.handles.get_mut(&handle).unwrap();
            block.data = data;
            block.size = bytes.len() as u32;
        }
        self.heap = Some(staged);
        self.mem_error = 0;
        Ok(())
    }
    pub fn dispose_handle(&mut self, memory: &mut Memory, handle: u32) -> Result<()> {
        if !self.handles.contains_key(&handle) {
            self.mem_error = if handle == 0 { NIL_HANDLE } else { MEM_BAD };
            return Ok(());
        }
        self.handle_size(memory, handle)?;
        let HandleBlock { data, .. } = self.handles.remove(&handle).unwrap();
        let heap = self.heap.as_mut().unwrap();
        heap.release(data);
        if self.pooled_masters.contains(&handle) {
            self.free_masters.push(handle);
        } else {
            heap.release(handle);
        }
        memory.write(GuestPtr(handle), &0u32.to_be_bytes())?;
        self.resource_handles.retain(|_, h| *h != handle);
        self.mem_error = 0;
        Ok(())
    }
    /// IM: More Macintosh Toolbox, ReleaseResource. Released handles
    /// lose resource identity; a later lookup loads the original fork bytes.
    pub fn release_resource(&mut self, memory: &mut Memory, handle: u32) -> Result<()> {
        self.res_error = 0;
        let Some((&(kind, id), _)) = self.resource_handles.iter().find(|(_, h)| **h == handle)
        else {
            self.res_error = -192;
            return Ok(());
        };
        if self
            .resources
            .get(kind, id)
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
            .resources
            .get(key.0, key.1)
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
        Ok(())
    }
    /// Read metadata from resource identity, never from mutable payload bytes.
    pub fn resource_info(&mut self, memory: &mut Memory, request: ResourceInfo) -> Result<()> {
        let Some((&(kind, id), _)) = self
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
            .resources
            .get(kind, id)
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
    pub fn get_named_resource(
        &mut self,
        memory: &mut Memory,
        kind: [u8; 4],
        name: GuestPtr,
        current_only: bool,
    ) -> Result<u32> {
        // Named resources live in the application map; standard cursor fallbacks
        // are unnamed. Only full-chain searches see that fallback CURS type.
        // Names are case-sensitive raw Macintosh bytes (IM: More Toolbox, pp. 1-75–77).
        let length = memory.read(name, 1)?[0] as usize;
        let bytes = &memory.read(name, length + 1)?[1..];
        let id = self
            .resources
            .entries()
            .find(|r| r.kind == kind && r.name.as_deref() == Some(bytes))
            .map(|r| r.id);
        if let Some(id) = id {
            self.get_resource(memory, kind, id)
        } else {
            self.res_error = if (!current_only && kind == *b"CURS")
                || self.resources.entries().any(|r| r.kind == kind)
            {
                -192
            } else {
                0
            };
            Ok(0)
        }
    }
    pub fn get_resource(&mut self, memory: &mut Memory, kind: [u8; 4], id: i16) -> Result<u32> {
        self.res_error = 0;
        if let Some(handle) = self.resource_handles.get(&(kind, id)) {
            return Ok(*handle);
        }
        let (contents, locked) = if let Some(resource) = self.resources.get(kind, id) {
            if resource.attributes & 1 != 0 {
                return Err(Error::Load(
                    "compressed resource decoding not implemented".into(),
                ));
            }
            (resource.data.clone(), resource.attributes & 0x10 != 0)
        } else if let Some(cursor) = (kind == *b"CURS")
            .then(|| crate::cursor::standard(id))
            .flatten()
        {
            (cursor.to_vec(), false)
        } else {
            // Standard CURS records form a fallback provider after the application
            // map. Their type exists even if this particular ID does not.
            if kind == *b"CURS" || self.resources.entries().any(|r| r.kind == kind) {
                self.res_error = -192;
            }
            return Ok(0);
        };
        let handle = self.new_handle(memory, contents.len() as u32, false)?;
        if handle == 0 {
            self.res_error = self.mem_error;
            return Ok(0);
        }
        let data = memory.u32(GuestPtr(handle))?;
        memory.write(GuestPtr(data), &contents)?;
        self.handles.get_mut(&handle).unwrap().locked = locked;
        self.resource_handles.insert((kind, id), handle);
        Ok(handle)
    }
}

#[cfg(test)]
mod resize_preflight_tests {
    use super::*;
    #[test]
    fn sequential_resize_preflight_preserves_bytes_errors_locks_and_allocator() {
        fn setup() -> (Memory, Process, u32, u32) {
            let mut m = Memory::default();
            let mut p = Process::default();
            let h = p.new_handle(&mut m, 10, true).unwrap();
            let blocker = p.new_pointer(&mut m, 64, false).unwrap();
            (m, p, h, blocker)
        }
        let (m, mut p, h, _) = setup();
        let before = m
            .read(GuestPtr(HEAP_BASE), HEAP_SIZE as usize)
            .unwrap()
            .to_vec();
        p.set_handle_lock(&m, h, true).unwrap();
        p.mem_error = -111;
        assert!(!p.can_resize_handles(&m, &[(h, 100)]).unwrap());
        assert_eq!(p.mem_error, -111);
        assert!(p.can_resize_handles(&m, &[(h, 4), (h, 10)]).unwrap());
        assert_eq!(p.mem_error, -111);
        assert_eq!(
            m.read(GuestPtr(HEAP_BASE), HEAP_SIZE as usize).unwrap(),
            before
        );
        let (mut other_m, mut other_p, other_h, _) = setup();
        other_p.set_handle_lock(&other_m, other_h, true).unwrap();
        let next = other_p.new_pointer(&mut other_m, 32, false).unwrap();
        let mut m = m;
        assert_eq!(p.new_pointer(&mut m, 32, false).unwrap(), next);
    }
}
