//! Process-scoped state shared by classic and native CPU adapters.

use crate::event_queue::EventQueue;
use crate::guest_call::SharedGuestCallStack;
use crate::memory::bus::{SharedClassicHeapAllocator, SharedRamRegion};
use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};
use crate::menu_manager::{ProcessMenuTrackingState, SharedNativeMenuSelection};
use ppc::PpcMemory;
use std::cell::{RefCell, RefMut};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Debug)]
struct ProcessMemoryRegion {
    base: u32,
    bytes: SharedRamRegion,
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
    fn detached_clone(&self) -> Self {
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

/// Architecture-neutral Memory Manager metadata for one Macintosh process.
///
/// Guest addresses, rather than CPU adapter records, identify relocatable
/// blocks. Keeping the reverse master-pointer index and handle state here
/// gives 68K traps and native imports one canonical registry as allocation
/// itself moves behind this process-level boundary. Inside Macintosh: Memory
/// (1992), pp. 2-12, 2-40--2-41.
#[derive(Debug, Default)]
pub(crate) struct ProcessMemoryManager {
    classic_allocator: Option<SharedClassicHeapAllocator>,
    ptr_to_handle: SharedProcessMap<u32>,
    handle_state_bits: SharedProcessMap<u8>,
    handle_high_locked: SharedProcessMap<bool>,
    native_handle_ptrs: HashSet<u32>,
    native_handles: HashSet<u32>,
    native_allocations: Vec<ProcessHandleRecord>,
    native_allocator: Option<ProcessNativeAllocatorState>,
    native_allocator_dirty: bool,
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
    const NATIVE_HEAP_ALIGNMENT: u32 = 16;
    const MEM_FULL_ERR: i16 = -108;
    const NIL_HANDLE_ERR: i16 = -109;
    const MEM_WZ_ERR: i16 = -111;
    const MEM_PUR_ERR: i16 = -112;
    const NO_ERR: i16 = 0;
    const PARAM_ERR: i16 = -50;

    fn detached_clone(&self) -> Self {
        Self {
            classic_allocator: None,
            ptr_to_handle: self.ptr_to_handle.detached_clone(),
            handle_state_bits: self.handle_state_bits.detached_clone(),
            handle_high_locked: self.handle_high_locked.detached_clone(),
            native_handle_ptrs: self.native_handle_ptrs.clone(),
            native_handles: self.native_handles.clone(),
            native_allocations: self.native_allocations.clone(),
            native_allocator: self.native_allocator.clone(),
            native_allocator_dirty: self.native_allocator_dirty,
        }
    }

    pub(crate) fn has_native_allocator(&self) -> bool {
        self.native_allocator.is_some()
    }

    /// Adopt the classic heap used by the process's 68K memory-bus adapter.
    ///
    /// The first attached bus contributes its live launch-time allocator;
    /// later adapters attach to that same process-owned state. Inside
    /// Macintosh: Memory (1992), pp. 2-19--2-21.
    pub(crate) fn attach_classic_memory_bus(&mut self, bus: &mut MacMemoryBus) {
        if let Some(allocator) = &self.classic_allocator {
            bus.attach_classic_heap_allocator(allocator.clone());
        } else {
            self.classic_allocator = Some(bus.shared_classic_heap_allocator());
        }
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

    #[cfg(test)]
    pub(crate) fn classic_allocation_size(&self, address: u32) -> Option<u32> {
        self.classic_allocator
            .as_ref()
            .and_then(|allocator| allocator.allocation_size(address))
    }

    /// Allocate a classic nonrelocatable block for this process.
    ///
    /// `NewPtr` returns a fixed block in the current heap or `NIL` with
    /// `memFullErr`. Inside Macintosh: Memory (1992), pp. 2-36--2-37.
    pub(crate) fn new_classic_ptr(&mut self, bus: &mut MacMemoryBus, size: u32) -> u32 {
        self.assert_classic_memory_bus_attached(bus);
        bus.alloc(size)
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
            bus.free(ptr);
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
        let ptr = bus.alloc(size);
        if ptr == 0 && size > 0 {
            return Err(Self::MEM_FULL_ERR);
        }
        let handle = bus.alloc(4);
        if handle == 0 {
            bus.free(ptr);
            return Err(Self::MEM_FULL_ERR);
        }
        bus.write_long(handle, ptr);
        self.ptr_to_handle.insert(ptr, handle);
        Ok((handle, ptr))
    }

    /// Allocate a classic master pointer whose relocatable block is empty.
    ///
    /// `NewEmptyHandle` returns a handle containing `NIL`. Inside Macintosh:
    /// Memory (1992), pp. 2-33--2-34.
    pub(crate) fn new_empty_classic_handle(
        &mut self,
        bus: &mut MacMemoryBus,
    ) -> Result<u32, i16> {
        self.assert_classic_memory_bus_attached(bus);
        let handle = bus.alloc(4);
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
            bus.free(ptr);
        }
        bus.free(handle);
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
            .or_else(|| bus.get_alloc_size(ptr))
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
            return Self::NIL_HANDLE_ERR;
        }
        let native_index = self.native_allocator.as_ref().and_then(|allocator| {
            allocator.ptrs.iter().position(|record| record.ptr == ptr)
        });
        let old_size = native_index
            .and_then(|index| {
                self.native_allocator
                    .as_ref()
                    .and_then(|allocator| allocator.ptrs.get(index))
                    .map(|record| record.size)
            })
            .or_else(|| bus.get_alloc_size(ptr))
            .unwrap_or(0);
        if MacMemoryBus::allocation_bucket_size(new_size)
            > MacMemoryBus::allocation_bucket_size(old_size)
        {
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
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
        } else {
            bus.set_alloc_size(ptr, new_size);
        }
        Self::NO_ERR
    }

    /// Return the logical size of a native or classic relocatable block.
    /// Inside Macintosh: Memory (1992), pp. 2-39--2-40.
    pub(crate) fn process_handle_size(
        &self,
        bus: &MacMemoryBus,
        handle: u32,
    ) -> Option<u32> {
        self.assert_classic_memory_bus_attached(bus);
        self.native_allocations
            .iter()
            .find(|record| record.handle == handle)
            .map(|record| record.size)
            .or_else(|| {
                (handle != 0)
                    .then(|| bus.read_long(handle))
                    .and_then(|ptr| bus.get_alloc_size(ptr))
            })
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
        let old_size = bus.get_alloc_size(old_ptr).unwrap_or(0);
        if old_size == new_size
            || (old_ptr != 0
                && MacMemoryBus::allocation_bucket_size(new_size)
                    == MacMemoryBus::allocation_bucket_size(old_size))
        {
            if new_size < old_size {
                bus.fill_zeros(old_ptr.wrapping_add(new_size), old_size - new_size);
            }
            bus.set_alloc_size(old_ptr, new_size);
            return Self::NO_ERR;
        }

        let new_ptr = bus.alloc(new_size);
        if new_ptr == 0 && new_size > 0 {
            return Self::MEM_FULL_ERR;
        }
        let copy_len = old_size.min(new_size) as usize;
        if copy_len > 0 {
            let bytes = bus.read_bytes(old_ptr, copy_len);
            bus.write_bytes(new_ptr, &bytes);
        }
        bus.free(old_ptr);
        bus.write_long(handle, new_ptr);
        self.ptr_to_handle.remove(&old_ptr);
        self.ptr_to_handle.insert(new_ptr, handle);
        Self::NO_ERR
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
        if native_record.is_none() && (handle == 0 || bus.get_alloc_size(handle) != Some(4)) {
            return Err(Self::MEM_WZ_ERR);
        }

        let relocated = if let Some(record) = native_record {
            let Some(required) = Self::native_allocation_size(size) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Err(Self::MEM_FULL_ERR);
            };
            let Some(allocator) = self.native_allocator.as_ref() else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Err(Self::MEM_FULL_ERR);
            };
            let reusable = allocator.free_ptr_blocks.iter().any(|free| {
                free.ptr != record.ptr
                    && Self::native_allocation_size(free.size)
                        .is_some_and(|capacity| capacity >= required)
            });
            if !reusable
                && Self::native_allocation_bounds(
                    allocator.heap.heap_cursor,
                    allocator.heap.heap_limit,
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
            let new_ptr = bus.alloc(size);
            if new_ptr == 0 && size > 0 {
                return Err(Self::MEM_FULL_ERR);
            }
            bus.fill_bytes(new_ptr, size, 0xA5);
            let old_ptr = bus.read_long(handle);
            bus.free(old_ptr);
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

    /// Release a native relocatable block while retaining its master pointer.
    ///
    /// The handle becomes empty and may later be repopulated by
    /// `ReallocateHandle`. A locked block is preserved and reports
    /// `memPurErr`. Inside Macintosh: Memory (1992), pp. 2-51--2-52.
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

        if handle == 0 || bus.get_alloc_size(handle) != Some(4) {
            return Self::MEM_WZ_ERR;
        }
        if self.state_for_handle(handle).unwrap_or(0) & 0x80 != 0 {
            return Self::MEM_PUR_ERR;
        }
        let ptr = bus.read_long(handle);
        if ptr != 0 {
            bus.free(ptr);
            self.ptr_to_handle.remove(&ptr);
        }
        bus.write_long(handle, 0);
        Self::NO_ERR
    }

    pub(crate) fn register_native_handle_records(
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
        for (record, state) in handles {
            let ProcessHandleRecord { handle, ptr, .. } = record;
            if handle != 0 {
                if ptr != 0 {
                    self.ptr_to_handle.insert(ptr, handle);
                    self.native_handle_ptrs.insert(ptr);
                }
                self.handle_state_bits.insert(handle, state);
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
            size.checked_add(Self::NATIVE_HEAP_ALIGNMENT - 1)?
                & !(Self::NATIVE_HEAP_ALIGNMENT - 1),
        )
        .map(|size| size.max(Self::NATIVE_HEAP_ALIGNMENT))
    }

    fn native_allocation_bounds(
        heap_cursor: u32,
        heap_limit: u32,
        aligned_size: u32,
        mut readonly_overlap_end: impl FnMut(u32, u32) -> Option<u32>,
    ) -> Option<(u32, u32)> {
        let mut ptr = heap_cursor
            .checked_add(Self::NATIVE_HEAP_ALIGNMENT - 1)?
            & !(Self::NATIVE_HEAP_ALIGNMENT - 1);
        loop {
            let next = ptr.checked_add(aligned_size)?;
            if next >= heap_limit {
                return None;
            }
            let Some(reserved_end) = readonly_overlap_end(ptr, aligned_size) else {
                return Some((ptr, next));
            };
            ptr = reserved_end
                .checked_add(Self::NATIVE_HEAP_ALIGNMENT - 1)?
                & !(Self::NATIVE_HEAP_ALIGNMENT - 1);
        }
    }

    pub(crate) fn set_native_mem_error(&mut self, error: i16) {
        if let Some(allocator) = &mut self.native_allocator {
            allocator.heap.last_mem_error = error;
            self.native_allocator_dirty = true;
        }
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
        let fully_mapped = (0..required)
            .all(|offset| PpcMemory::read_u8(memory, ptr + offset).is_some());
        if !fully_mapped {
            let Ok(required) = usize::try_from(required) else {
                return false;
            };
            memory.add_region(ptr, vec![0; required]);
            return true;
        }
        !clear
            || (0..required)
                .all(|offset| PpcMemory::write_u8(memory, ptr + offset, 0).is_some())
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
        let reusable_index = allocator
            .free_ptr_blocks
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                let capacity = Self::native_allocation_size(record.size)?;
                (capacity >= required).then_some((index, capacity))
            })
            .min_by_key(|(_, capacity)| *capacity)
            .map(|(index, _)| index);
        let allocation = if let Some(index) = reusable_index {
            Some((allocator.free_ptr_blocks[index].ptr, None))
        } else {
            Self::native_allocation_bounds(
                allocator.heap.heap_cursor,
                allocator.heap.heap_limit,
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
        let Some((resize_ptr, new_end)) = Self::native_allocation_bounds(
            record.ptr,
            heap.heap_limit,
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
        if (old_end..new_end)
            .any(|address| PpcMemory::write_u8(memory, address, 0).is_none())
        {
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
    pub(crate) fn recover_handle(&self, ptr: u32) -> Option<u32> {
        self.ptr_to_handle.get(&ptr)
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
        let reusable_handle_index = allocator
            .free_handle_blocks
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                if record.ptr == 0 {
                    return None;
                }
                let capacity = Self::native_allocation_size(record.capacity)?;
                (capacity >= required).then_some((index, capacity))
            })
            .min_by_key(|(_, capacity)| *capacity)
            .map(|(index, _)| index)
            .or_else(|| {
                allocator
                    .free_handle_blocks
                    .iter()
                    .position(|record| record.ptr == 0)
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
                        (capacity >= required).then_some((index, capacity))
                    })
                    .min_by_key(|(_, capacity)| *capacity)
                    .map(|(index, _)| index);
                if let Some(index) = reusable_ptr_index {
                    record.ptr = allocator.free_ptr_blocks[index].ptr;
                } else {
                    let Some((ptr, next)) = Self::native_allocation_bounds(
                        allocator.heap.heap_cursor,
                        allocator.heap.heap_limit,
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
                allocator.heap.heap_limit,
                handle_required,
                |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
            ) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return 0;
            };
            let Some((ptr, after_ptr)) = Self::native_allocation_bounds(
                after_handle,
                allocator.heap.heap_limit,
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
        )
            || !Self::prepare_native_allocation(memory, record.ptr, required, clear)
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
        if bytes.iter().copied().enumerate().any(|(offset, byte)| {
            PpcMemory::write_u8(memory, record.ptr + offset as u32, byte).is_none()
        }) {
            let _ = self.dispose_native_handle(memory, handle);
            self.set_native_mem_error(Self::PARAM_ERR);
            return 0;
        }
        handle
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

    pub(crate) fn native_handle_size(&mut self, handle: u32) -> Option<u32> {
        let size = self
            .native_allocations
            .iter()
            .find(|record| record.handle == handle)
            .map(|record| record.size);
        self.set_native_mem_error(if size.is_some() {
            Self::NO_ERR
        } else {
            Self::NIL_HANDLE_ERR
        });
        size
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
        let can_extend_last = record.ptr.checked_add(old_aligned)
            == Some(allocator.heap.heap_cursor)
            && Self::native_allocation_bounds(
                record.ptr,
                allocator.heap.heap_limit,
                new_aligned,
                |ptr, len| memory.readonly_allocation_overlap_end(ptr, len),
            )
            .is_some_and(|(ptr, _)| ptr == record.ptr);
        let (new_ptr, next_cursor) = if can_extend_last {
            (record.ptr, record.ptr.checked_add(new_aligned))
        } else {
            let Some((ptr, next)) = Self::native_allocation_bounds(
                allocator.heap.heap_cursor,
                allocator.heap.heap_limit,
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
            || (new_ptr != record.ptr
                && PpcMemory::write_u32_be(memory, handle, new_ptr).is_none())
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
                    (free.ptr != current_ptr && capacity >= new_aligned)
                        .then_some((index, capacity))
                })
                .min_by_key(|(_, capacity)| *capacity)
                .map(|(index, _)| index);
            if let Some(index) = recycled_ptr_index {
                new_ptr = allocator.free_ptr_blocks[index].ptr;
            } else {
                let Some((ptr, next)) = Self::native_allocation_bounds(
                    allocator.heap.heap_cursor,
                    allocator.heap.heap_limit,
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
                    allocator.heap.heap_limit,
                    new_aligned,
                    |ptr, len| bus.foreign_readonly_allocation_overlap_end(ptr, len),
                )
                .is_some_and(|(ptr, _)| ptr == record.ptr);
            if can_extend_last {
                new_cursor = record.ptr + new_aligned;
            } else {
                let Some((ptr, next)) = Self::native_allocation_bounds(
                    allocator.heap.heap_cursor,
                    allocator.heap.heap_limit,
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

    pub(crate) fn synchronize_native_allocator(
        &mut self,
        heap_cursor: u32,
        heap_limit: u32,
        last_mem_error: i16,
        ptrs: &[ProcessPtrRecord],
        free_ptr_blocks: &[ProcessPtrRecord],
        free_handle_blocks: &[ProcessHandleRecord],
    ) {
        let Some(heap) = self
            .native_allocator
            .as_ref()
            .map(|allocator| allocator.heap)
        else {
            return;
        };
        self.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: heap.heap_base,
                heap_cursor,
                heap_limit,
                last_mem_error,
                heap_maximized: heap.heap_maximized,
                master_pointer_blocks_requested: heap.master_pointer_blocks_requested,
            },
            ptrs,
            free_ptr_blocks,
            free_handle_blocks,
        );
    }

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

    pub(crate) fn adopt_handle_metadata(&mut self, source: &mut Self) {
        if self.ptr_to_handle.ptr_eq(&source.ptr_to_handle)
            && self.handle_state_bits.ptr_eq(&source.handle_state_bits)
            && self.handle_high_locked.ptr_eq(&source.handle_high_locked)
        {
            return;
        }
        self.ptr_to_handle.extend(source.ptr_to_handle.take_entries());
        self.handle_state_bits
            .extend(source.handle_state_bits.take_entries());
        self.handle_high_locked
            .extend(source.handle_high_locked.take_entries());
        if self.native_allocations.is_empty() {
            self.native_allocations.append(&mut source.native_allocations);
            self.native_handle_ptrs
                .extend(source.native_handle_ptrs.drain());
            self.native_handles.extend(source.native_handles.drain());
        }
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
#[derive(Debug, Default)]
pub(crate) struct ProcessContext {
    memory: Vec<ProcessMemoryRegion>,
    memory_manager: SharedProcessMemoryManager,
    event_queue: EventQueue,
    menu_tracking: Option<ProcessMenuTrackingState>,
    pending_native_menu_selection: SharedNativeMenuSelection,
    guest_calls: SharedGuestCallStack,
}

impl ProcessContext {
    #[cfg(test)]
    pub(crate) fn memory_manager_mut(&self) -> RefMut<'_, ProcessMemoryManager> {
        self.memory_manager.borrow_mut()
    }

    pub(crate) fn attach_classic_memory_bus(&mut self, bus: &mut MacMemoryBus) {
        self.memory_manager.borrow_mut().attach_classic_memory_bus(bus);
    }

    #[cfg(test)]
    pub(crate) fn handle_for_ptr(&self, ptr: u32) -> Option<u32> {
        self.memory_manager.borrow().handle_for_ptr(ptr)
    }

    pub(crate) fn attach_memory_manager(
        &self,
        adapter: &mut Option<SharedProcessMemoryManager>,
    ) {
        if let Some(attached) = adapter {
            assert!(
                attached.ptr_eq(&self.memory_manager),
                "cannot attach two process Memory Managers"
            );
        } else {
            *adapter = Some(self.memory_manager.clone());
        }
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

    pub(crate) fn event_queue_mut(&mut self) -> &mut EventQueue {
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
        self.menu_tracking = state;
    }

    #[cfg(test)]
    pub(crate) fn menu_tracking_slot_mut(&mut self) -> &mut Option<ProcessMenuTrackingState> {
        &mut self.menu_tracking
    }

    /// Transfer detached adapter state into the canonical process owner.
    pub(crate) fn adopt_menu_tracking(&mut self, adapter: &mut Option<ProcessMenuTrackingState>) {
        assert!(
            adapter.is_none() || self.menu_tracking.is_none(),
            "cannot attach two active Menu Manager continuations"
        );
        if self.menu_tracking.is_none() {
            self.menu_tracking = adapter.take();
        }
    }

    /// Borrow the process state temporarily installed in an active CPU adapter.
    pub(crate) fn event_queue_and_menu_tracking_mut(
        &mut self,
    ) -> (&mut EventQueue, &mut Option<ProcessMenuTrackingState>) {
        (&mut self.event_queue, &mut self.menu_tracking)
    }

    pub(crate) fn event_queue_menu_tracking_and_memory_manager(
        &mut self,
    ) -> (
        &mut EventQueue,
        &mut Option<ProcessMenuTrackingState>,
        &SharedProcessMemoryManager,
    ) {
        (
            &mut self.event_queue,
            &mut self.menu_tracking,
            &self.memory_manager,
        )
    }

    pub(crate) fn attach_native_menu_selection(&self, adapter: &mut SharedNativeMenuSelection) {
        adapter.attach_to(&self.pending_native_menu_selection);
    }

    pub(crate) fn attach_guest_calls(&self, adapter: &mut SharedGuestCallStack) {
        adapter.attach_to(&self.guest_calls);
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

        let ptr = context.memory_manager_mut().new_classic_ptr(&mut primary, 37);
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
        assert_eq!(detached.heap_bump_ptr(), 0x20_0000 + 24);
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
        assert_eq!(
            context.memory_ranges(),
            vec![(0, 0x1000), (0x3000, 0x1000)]
        );

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

        let (queue, menu) = context.event_queue_and_menu_tracking_mut();
        queue.push_back(QueuedEvent {
            what: 2,
            message: 0x5678,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        *menu = Some(crate::menu_manager::test_process_menu_tracking(0x0065_4321));
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
    #[should_panic(expected = "cannot attach two active Menu Manager continuations")]
    fn adopting_two_active_menu_continuations_is_always_rejected() {
        let mut context = ProcessContext::default();
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(
            0x1000,
        )));
        let mut second = Some(crate::menu_manager::test_process_menu_tracking(0x2000));
        context.adopt_menu_tracking(&mut second);
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
    fn native_allocator_synchronization_preserves_process_heap_operations() {
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
        manager.synchronize_native_allocator(
            HEAP_BASE + 0x20,
            HEAP_BASE + 0x1000,
            ProcessMemoryManager::PARAM_ERR,
            &[],
            &[],
            &[],
        );

        let heap = manager.native_heap_state().unwrap();
        assert_eq!(heap.heap_cursor, HEAP_BASE + 0x20);
        assert_eq!(heap.last_mem_error, ProcessMemoryManager::PARAM_ERR);
        assert!(heap.heap_maximized);
        assert_eq!(heap.master_pointer_blocks_requested, 1);
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
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };
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
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };
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
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };
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
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };
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
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };
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
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };
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
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };

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
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };

        let handle = manager.copy_bytes_to_new_native_handle(&mut native, b"native");
        let original = manager.native_allocation(handle).unwrap();
        assert_eq!(bus.read_bytes(original.ptr, 6), b"native");

        let blocking_ptr = manager.new_native_ptr(&mut native, 16, false);
        assert_ne!(blocking_ptr, 0);
        assert_eq!(
            manager.append_bytes_to_native_handle(
                &mut native,
                handle,
                b" process memory manager",
            ),
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
        assert_eq!(
            native.read_u8(appended.ptr + appended.size - 1),
            Some(b'!')
        );
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
}
