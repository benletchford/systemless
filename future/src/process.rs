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
    resource_ref: i16,
    pub mem_error: i16,
    pub res_error: i16,
    heap: Option<Heap>,
    handles: BTreeMap<u32, (u32, u32)>,
    resource_handles: BTreeMap<([u8; 4], i16), u32>,
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
    pub fn current_resource_file(&self) -> i16 {
        self.resource_ref
    }

    fn ensure_heap(&mut self, memory: &mut Memory) -> Result<()> {
        if self.heap.is_none() {
            memory.map(
                GuestPtr(HEAP_BASE),
                vec![0; HEAP_SIZE as usize],
                Permissions::RW,
            )?;
            self.heap = Some(Heap {
                free: BTreeMap::from([(HEAP_BASE, HEAP_SIZE)]),
                allocated: BTreeMap::new(),
            });
        }
        Ok(())
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
        self.handles.insert(master, (data, size));
        self.mem_error = 0;
        Ok(master)
    }
    pub fn handle_size(&mut self, memory: &Memory, handle: u32) -> Result<u32> {
        let Some(&(data, size)) = self.handles.get(&handle) else {
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
    pub fn dispose_handle(&mut self, memory: &mut Memory, handle: u32) -> Result<()> {
        if !self.handles.contains_key(&handle) {
            self.mem_error = if handle == 0 { NIL_HANDLE } else { MEM_BAD };
            return Ok(());
        }
        self.handle_size(memory, handle)?;
        let (data, _) = self.handles.remove(&handle).unwrap();
        let heap = self.heap.as_mut().unwrap();
        heap.release(data);
        heap.release(handle);
        memory.write(GuestPtr(handle), &0u32.to_be_bytes())?;
        self.resource_handles.retain(|_, h| *h != handle);
        self.mem_error = 0;
        Ok(())
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
        let contents = resource.data.clone();
        let handle = self.new_handle(memory, contents.len() as u32, false)?;
        if handle == 0 {
            self.res_error = self.mem_error;
            return Ok(0);
        }
        let data = memory.u32(GuestPtr(handle))?;
        memory.write(GuestPtr(data), &contents)?;
        self.resource_handles.insert((kind, id), handle);
        Ok(handle)
    }
}
