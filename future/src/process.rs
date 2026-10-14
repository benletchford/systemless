//! One process-owned application resource map and heap, shared by both ABIs.
//! Memory Manager contracts: Inside Macintosh: Memory (1992), pp. 2-29–2-40.
//! Resource lookup: More Macintosh Toolbox (1993), GetResource, pp. 1-69–1-70.
use crate::{
    load::resource::ResourceFork,
    memory::{GuestPtr, Memory, Permissions},
    Error, Result,
};
use std::collections::BTreeMap;
pub const HEAP_BASE: u32 = 0x1000_0000;
pub const HEAP_SIZE: u32 = 4 * 1024 * 1024;
const MEM_FULL: i16 = -108;
const NIL_HANDLE: i16 = -109;
const MEM_BAD: i16 = -111;
#[derive(Default)]
pub struct Process {
    pub resources: ResourceFork,
    pub(crate) fragments: crate::cfm::Connections,
    pub files: crate::files::Files,
    pub input: crate::events::Input,
    pub ports: Option<crate::quickdraw::Ports>,
    resource_ref: i16,
    pub mem_error: i16,
    pub res_error: i16,
    heap: Option<Heap>,
    handles: BTreeMap<u32, HandleBlock>,
    pointers: BTreeMap<u32, u32>,
    resource_handles: BTreeMap<([u8; 4], i16), u32>,
}
#[derive(Clone, Copy)]
struct HandleBlock {
    data: u32,
    size: u32,
    locked: bool,
}
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

    fn ensure_heap(&mut self, memory: &mut Memory) -> Result<()> {
        if self.heap.is_none() {
            memory.map(
                GuestPtr(HEAP_BASE),
                vec![0; HEAP_SIZE as usize],
                // Classic applications execute resource-loaded code and
                // generated thunks from heap RAM on both architectures.
                Permissions::RWX,
            )?;
            self.heap = Some(Heap {
                free: BTreeMap::from([(HEAP_BASE, HEAP_SIZE)]),
                allocated: BTreeMap::new(),
            });
        }
        Ok(())
    }
    /// Nonrelocatable allocations share extents with handles, but have distinct
    /// ownership: a handle master or data address is never a disposable Ptr.
    pub fn new_pointer(&mut self, memory: &mut Memory, size: u32, clear: bool) -> Result<u32> {
        self.ensure_heap(memory)?;
        self.mem_error = MEM_FULL;
        if size > HEAP_SIZE {
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
        if size > HEAP_SIZE || !self.heap.as_mut().unwrap().resize_in_place(pointer, size) {
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
        let heap = self.heap.as_mut().unwrap();
        self.mem_error = MEM_FULL;
        if size > HEAP_SIZE {
            return Ok(0);
        }
        let Some(data) = heap.allocate(size) else {
            return Ok(0);
        };
        let Some(master) = heap.allocate(4) else {
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
        if size > HEAP_SIZE {
            self.mem_error = MEM_FULL;
            return Ok(());
        }
        let HandleBlock { data, locked, .. } = self.handles[&handle];
        // The heap is one checked writable mapping. Validate it before changing
        // allocator metadata or master pointers, including the relocation case.
        memory.validate_write(GuestPtr(HEAP_BASE), HEAP_SIZE as usize)?;
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
    pub fn dispose_handle(&mut self, memory: &mut Memory, handle: u32) -> Result<()> {
        if !self.handles.contains_key(&handle) {
            self.mem_error = if handle == 0 { NIL_HANDLE } else { MEM_BAD };
            return Ok(());
        }
        self.handle_size(memory, handle)?;
        let HandleBlock { data, .. } = self.handles.remove(&handle).unwrap();
        let heap = self.heap.as_mut().unwrap();
        heap.release(data);
        heap.release(handle);
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
    pub fn get_named_resource(
        &mut self,
        memory: &mut Memory,
        kind: [u8; 4],
        name: GuestPtr,
        _current_only: bool,
    ) -> Result<u32> {
        // Both search modes currently visit the single application map. Names
        // are case-sensitive raw Macintosh bytes (IM: More Toolbox, pp. 1-75–77).
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
            self.res_error = if self.resources.entries().any(|r| r.kind == kind) {
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
        let Some(resource) = self.resources.get(kind, id) else {
            // GetResource distinguishes an absent type from a missing ID.
            if self.resources.entries().any(|r| r.kind == kind) {
                self.res_error = -192;
            }
            return Ok(0);
        };
        if resource.attributes & 1 != 0 {
            return Err(Error::Load(
                "compressed resource decoding not implemented".into(),
            ));
        }
        let locked = resource.attributes & 0x10 != 0; // resLocked
        let contents = resource.data.clone();
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
