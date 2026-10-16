//! One process-owned application resource map and heap, shared by both ABIs.
//! Memory Manager contracts: Inside Macintosh: Memory (1992), pp. 2-29–2-40.
//! Resource lookup: More Macintosh Toolbox (1993), GetResource, pp. 1-69–1-70.
use crate::{
    load::resource::ResourceFork,
    memory::{GuestPtr, Memory, Permissions},
    Error, Result,
};
use std::collections::{BTreeMap, BTreeSet};
mod compaction;
pub mod resources;
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
    pub numerics: crate::numerics::Numerics,
    pub timers: crate::timers::Timers,
    pub retrace: crate::retrace::Retrace,
    pub(crate) display: crate::display::Display,
    pub resources: ResourceFork,
    pub(crate) fragments: crate::cfm::Connections,
    pub files: crate::files::Files,
    pub dialogs: crate::dialogs::Dialogs,
    pub sound: crate::sound::Manager,
    pub devices: crate::devices::Manager,
    pub color_ports: crate::color_ports::Manager,
    pub color: crate::color::Manager,
    pub cursor: crate::cursor::Cursor,
    pub input: crate::events::Input,
    pub menus: crate::menus::Menus,
    pub polygons: crate::polygons::Manager,
    pub region_recording: crate::region_recording::Manager,
    pub textedit: crate::textedit::Manager,
    pub fonts: crate::font::Manager,
    pub windows: crate::windows::Windows,
    pub controls: crate::controls::Controls,
    pub ports: Option<crate::quickdraw::Ports>,
    resource_ref: i16,
    resource_files: resources::Maps,
    pub mem_error: i16,
    pub res_error: i16,
    heap: Option<Heap>,
    reservation: Option<(u32, u32)>,
    handles: BTreeMap<u32, HandleBlock>,
    free_masters: Vec<u32>,
    pooled_masters: BTreeSet<u32>,
    pointers: BTreeMap<u32, u32>,
    resource_handles: BTreeMap<(i16, [u8; 4], i16), u32>,
}
#[derive(Clone, Copy)]
struct HandleBlock {
    data: u32,
    size: u32,
    locked: bool,
    attributes: u8,
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
    fn allocate_at(&mut self, base: u32, size: u32) -> bool {
        let Some(extent) = size.max(1).checked_add(3).map(|v| v & !3) else {
            return false;
        };
        let Some((&start, &length)) = self.free.range(..=base).next_back() else {
            return false;
        };
        if u64::from(base) + u64::from(extent) > u64::from(start) + u64::from(length) {
            return false;
        }
        self.free.remove(&start);
        if base > start {
            self.free.insert(start, base - start);
        }
        let end = base + extent;
        let limit = start + length;
        if end < limit {
            self.free.insert(end, limit - end);
        }
        self.allocated.insert(base, extent);
        true
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
        self.resource_files = resources::Maps::default();
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
        if self.resource_ref != 0 {
            self.files.protect_resource_access(self.resource_ref);
        }
    }
    pub fn current_resource_file(&self) -> i16 {
        if self.resource_files.current == resources::APPLICATION {
            self.resource_ref
        } else {
            self.resource_files.current
        }
    }

    fn heap_base(&self) -> u32 {
        self.reservation.map_or(HEAP_BASE, |r| r.0)
    }
    /// Total unallocated extents in the eagerly grown application heap.
    /// This includes fragmented free blocks, not just the largest allocation.
    pub fn free_memory(&self) -> u32 {
        self.heap.as_ref().map_or_else(
            || self.heap_capacity(),
            |heap| heap.free.values().copied().sum(),
        )
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
                attributes: 0,
            },
        );
        self.mem_error = 0;
        Ok(master)
    }
    /// Copy the payload, not resource identity or handle attributes. The runtime
    /// currently owns one application heap; multiple heap zones remain unsupported.
    pub fn copy_handle(&mut self, memory: &mut Memory, handle: u32) -> Result<u32> {
        if !self.check_handle_flags(memory, handle)? {
            return Ok(0);
        }
        let source = self.handles[&handle];
        // Snapshot before allocation: a future compacting allocator must not
        // invalidate a borrowed guest payload while obtaining the new block.
        let bytes = memory
            .read(GuestPtr(source.data), source.size as usize)?
            .to_vec();
        let copy = self.new_handle(memory, source.size, false)?;
        if copy != 0 {
            let data = memory.u32(GuestPtr(copy))?;
            memory.write(GuestPtr(data), &bytes)?;
        }
        Ok(copy)
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
    fn check_handle_flags(&mut self, memory: &Memory, handle: u32) -> Result<bool> {
        if !self.handles.contains_key(&handle) {
            self.mem_error = if handle == 0 { NIL_HANDLE } else { MEM_BAD };
            return Ok(false);
        }
        if memory.u32(GuestPtr(handle))? == 0 {
            self.mem_error = NIL_HANDLE;
            return Ok(false);
        }
        self.handle_size(memory, handle)?;
        Ok(true)
    }
    pub fn handle_state(&mut self, memory: &Memory, handle: u32) -> Result<u8> {
        if !self.check_handle_flags(memory, handle)? {
            return Ok(self.mem_error as i8 as u8);
        }
        let block = self.handles[&handle];
        Ok(block.attributes | if block.locked { 0x80 } else { 0 })
    }
    pub fn set_handle_state(&mut self, memory: &Memory, handle: u32, flags: u8) -> Result<()> {
        if self.check_handle_flags(memory, handle)? {
            let block = self.handles.get_mut(&handle).unwrap();
            block.locked = flags & 0x80 != 0;
            block.attributes = flags & 0x7f;
        }
        Ok(())
    }
    pub fn set_handle_attribute(
        &mut self,
        memory: &Memory,
        handle: u32,
        resource: bool,
        enabled: bool,
    ) -> Result<()> {
        if self.check_handle_flags(memory, handle)? {
            let mask = if resource { 0x20 } else { 0x40 };
            let block = self.handles.get_mut(&handle).unwrap();
            block.attributes = if enabled {
                block.attributes | mask
            } else {
                block.attributes & !mask
            };
        }
        Ok(())
    }
    pub fn set_handle_lock(&mut self, memory: &Memory, handle: u32, locked: bool) -> Result<()> {
        if self.check_handle_flags(memory, handle)? {
            self.handles.get_mut(&handle).unwrap().locked = locked;
        }
        Ok(())
    }
    /// Move within the adjacent free interval. Other allocations are barriers;
    /// compaction of other unlocked handles is a separate unfinished operation.
    pub fn move_handle_high(&mut self, memory: &mut Memory, handle: u32) -> Result<()> {
        if !self.check_handle_flags(memory, handle)? {
            return Ok(());
        }
        let block = self.handles[&handle];
        if block.locked {
            self.mem_error = -117;
            return Ok(());
        }
        if self
            .resource_handles
            .iter()
            .any(|(&(owner, kind, id), &h)| {
                h == handle
                    && self
                        .resource_metadata(owner, kind, id)
                        .is_some_and(|r| r.attributes & 2 != 0)
            })
        {
            return Err(Error::Load(
                "MoveHHi of changed resource requires unimplemented WriteResource".into(),
            ));
        }
        let heap = self.heap.as_ref().unwrap();
        let extent = heap.allocated[&block.data];
        let end = block.data + extent;
        let Some(&available) = heap.free.get(&end) else {
            return Ok(());
        };
        let destination = ((end + available) & !3) - extent;
        if destination <= block.data {
            return Ok(());
        }
        memory.validate_write(GuestPtr(handle), 4)?;
        memory.validate_write(GuestPtr(destination), block.size as usize)?;
        let bytes = memory
            .read(GuestPtr(block.data), block.size as usize)?
            .to_vec();
        let mut staged = heap.clone();
        staged.release(block.data);
        if !staged.allocate_at(destination, block.size) {
            return Err(Error::Cpu("invalid staged high-handle relocation".into()));
        }
        memory.write(GuestPtr(destination), &bytes)?;
        memory.write(GuestPtr(handle), &destination.to_be_bytes())?;
        self.handles.get_mut(&handle).unwrap().data = destination;
        self.heap = Some(staged);
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
        let HandleBlock {
            data,
            locked,
            attributes,
            ..
        } = self.handles[&handle];
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
                attributes,
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
