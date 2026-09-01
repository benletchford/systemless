//! Process-scoped state shared by classic and native CPU adapters.

use crate::event_queue::EventQueue;
use crate::guest_call::SharedGuestCallStack;
use crate::memory::bus::SharedRamRegion;
use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};
use crate::menu_manager::{ProcessMenuTrackingState, SharedNativeMenuSelection};
use ppc::PpcMemory;
use std::collections::{HashMap, HashSet};

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

/// Architecture-neutral Memory Manager metadata for one Macintosh process.
///
/// Guest addresses, rather than CPU adapter records, identify relocatable
/// blocks. Keeping the reverse master-pointer index and handle state here
/// gives 68K traps and native imports one canonical registry as allocation
/// itself moves behind this process-level boundary. Inside Macintosh: Memory
/// (1992), pp. 2-12, 2-40--2-41.
#[derive(Debug, Default)]
pub(crate) struct ProcessMemoryManager {
    ptr_to_handle: HashMap<u32, u32>,
    handle_state_bits: HashMap<u32, u8>,
    native_handle_ptrs: HashSet<u32>,
    native_handles: HashSet<u32>,
    native_allocations: HashMap<u32, ProcessHandleRecord>,
    native_allocator: Option<ProcessNativeAllocatorState>,
    native_allocator_dirty: bool,
}

impl ProcessMemoryManager {
    const NATIVE_HEAP_ALIGNMENT: u32 = 16;
    const MEM_FULL_ERR: i16 = -108;
    const NIL_HANDLE_ERR: i16 = -109;
    const NO_ERR: i16 = 0;
    const PARAM_ERR: i16 = -50;

    pub(crate) fn merge_metadata(
        &mut self,
        ptr_to_handle: HashMap<u32, u32>,
        handle_state_bits: HashMap<u32, u8>,
    ) {
        self.ptr_to_handle.extend(ptr_to_handle);
        self.handle_state_bits.extend(handle_state_bits);
    }

    pub(crate) fn adopt_metadata(
        &mut self,
        ptr_to_handle: &mut HashMap<u32, u32>,
        handle_state_bits: &mut HashMap<u32, u8>,
    ) {
        assert!(
            self.ptr_to_handle.is_empty() || ptr_to_handle.is_empty(),
            "cannot attach two active pointer-to-handle registries"
        );
        assert!(
            self.handle_state_bits.is_empty() || handle_state_bits.is_empty(),
            "cannot attach two active handle-state registries"
        );
        if self.ptr_to_handle.is_empty() {
            std::mem::swap(&mut self.ptr_to_handle, ptr_to_handle);
        }
        if self.handle_state_bits.is_empty() {
            std::mem::swap(&mut self.handle_state_bits, handle_state_bits);
        }
    }

    #[cfg(test)]
    pub(crate) fn metadata_mut(
        &mut self,
    ) -> (&mut HashMap<u32, u32>, &mut HashMap<u32, u8>) {
        (&mut self.ptr_to_handle, &mut self.handle_state_bits)
    }

    pub(crate) fn take_metadata(&mut self) -> (HashMap<u32, u32>, HashMap<u32, u8>) {
        (
            std::mem::take(&mut self.ptr_to_handle),
            std::mem::take(&mut self.handle_state_bits),
        )
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
        }
        self.native_allocations.clear();
        for (record, state) in handles {
            let ProcessHandleRecord { handle, ptr, .. } = record;
            if handle != 0 && ptr != 0 {
                self.ptr_to_handle.insert(ptr, handle);
                self.native_handle_ptrs.insert(ptr);
                self.handle_state_bits.insert(handle, state);
                self.native_handles.insert(handle);
                self.native_allocations.insert(handle, record);
            }
        }
    }

    pub(crate) fn state_for_handle(&self, handle: u32) -> Option<u8> {
        self.handle_state_bits.get(&handle).copied()
    }

    pub(crate) fn native_allocation(&self, handle: u32) -> Option<ProcessHandleRecord> {
        self.native_allocations.get(&handle).copied()
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

    pub(crate) fn dispose_native_ptr(&mut self, ptr: u32) {
        if let Some(allocator) = &mut self.native_allocator {
            if let Some(index) = allocator.ptrs.iter().position(|record| record.ptr == ptr) {
                allocator.free_ptr_blocks.push(allocator.ptrs.remove(index));
            }
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
        }
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
        let reusable_index = allocator
            .free_handle_blocks
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                let capacity = Self::native_allocation_size(record.capacity)?;
                (capacity >= required).then_some((index, capacity))
            })
            .min_by_key(|(_, capacity)| *capacity)
            .map(|(index, _)| index);
        let (record, next_cursor) = if let Some(index) = reusable_index {
            let mut record = allocator.free_handle_blocks[index];
            record.size = size;
            (record, None)
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
        if let Some(index) = reusable_index {
            allocator.free_handle_blocks.swap_remove(index);
        }
        if let Some(next_cursor) = next_cursor {
            allocator.heap.heap_cursor = next_cursor;
        }
        allocator.heap.last_mem_error = Self::NO_ERR;
        self.native_allocations.insert(record.handle, record);
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
        let Some(record) = self.native_allocations.remove(&handle) else {
            self.set_native_mem_error(Self::NO_ERR);
            return None;
        };
        if PpcMemory::write_u32_be(memory, handle, 0).is_none() {
            self.native_allocations.insert(handle, record);
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return None;
        }
        self.ptr_to_handle.remove(&record.ptr);
        self.native_handle_ptrs.remove(&record.ptr);
        self.handle_state_bits.remove(&handle);
        self.native_handles.remove(&handle);
        if let Some(allocator) = &mut self.native_allocator {
            allocator.free_handle_blocks.push(record);
            allocator.heap.last_mem_error = Self::NO_ERR;
            self.native_allocator_dirty = true;
        }
        Some(record)
    }

    pub(crate) fn native_handle_size(&mut self, handle: u32) -> Option<u32> {
        let size = self
            .native_allocations
            .get(&handle)
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
        let Some(mut record) = self.native_allocations.get(&handle).copied() else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        };
        if PpcMemory::read_u32_be(memory, handle) != Some(record.ptr) {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Self::NIL_HANDLE_ERR;
        }
        if size <= record.capacity {
            record.size = size;
            self.native_allocations.insert(handle, record);
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
        self.native_allocations.insert(handle, record);
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
        let Some(record) = self.native_allocations.get(&handle).copied() else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        };
        let current_ptr = bus.read_long(handle);
        if current_ptr == 0 || current_ptr != expected_ptr || record.ptr != current_ptr {
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
        if size > record.capacity {
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
        self.native_allocations.insert(handle, updated);
        self.native_handle_ptrs.remove(&current_ptr);
        self.native_handle_ptrs.insert(new_ptr);
        let allocator = self
            .native_allocator
            .as_mut()
            .expect("native allocator remains registered");
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn synchronize_native_allocator(
        &mut self,
        heap_cursor: u32,
        heap_limit: u32,
        last_mem_error: i16,
        heap_maximized: bool,
        master_pointer_blocks_requested: u32,
        ptrs: &[ProcessPtrRecord],
        free_ptr_blocks: &[ProcessPtrRecord],
        free_handle_blocks: &[ProcessHandleRecord],
    ) {
        let Some(heap_base) = self
            .native_allocator
            .as_ref()
            .map(|allocator| allocator.heap.heap_base)
        else {
            return;
        };
        self.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base,
                heap_cursor,
                heap_limit,
                last_mem_error,
                heap_maximized,
                master_pointer_blocks_requested,
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

    #[cfg(test)]
    pub(crate) fn native_allocator(&self) -> Option<&ProcessNativeAllocatorState> {
        self.native_allocator.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn set_native_allocation(&mut self, record: ProcessHandleRecord) {
        self.native_allocations.insert(record.handle, record);
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
        self.ptr_to_handle.get(&ptr).copied()
    }

    #[cfg(test)]
    pub(crate) fn handle_state(&self, handle: u32) -> u8 {
        self.handle_state_bits.get(&handle).copied().unwrap_or(0)
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
    memory_manager: ProcessMemoryManager,
    event_queue: EventQueue,
    menu_tracking: Option<ProcessMenuTrackingState>,
    pending_native_menu_selection: SharedNativeMenuSelection,
    guest_calls: SharedGuestCallStack,
}

impl ProcessContext {
    pub(crate) fn memory_manager_mut(&mut self) -> &mut ProcessMemoryManager {
        &mut self.memory_manager
    }

    #[cfg(test)]
    pub(crate) fn handle_for_ptr(&self, ptr: u32) -> Option<u32> {
        self.memory_manager.handle_for_ptr(ptr)
    }

    pub(crate) fn adopt_memory_manager_metadata(
        &mut self,
        ptr_to_handle: &mut HashMap<u32, u32>,
        handle_state_bits: &mut HashMap<u32, u8>,
    ) {
        self.memory_manager
            .adopt_metadata(ptr_to_handle, handle_state_bits);
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

    pub(crate) fn event_queue_menu_tracking_and_memory_manager_mut(
        &mut self,
    ) -> (
        &mut EventQueue,
        &mut Option<ProcessMenuTrackingState>,
        &mut ProcessMemoryManager,
    ) {
        (
            &mut self.event_queue,
            &mut self.menu_tracking,
            &mut self.memory_manager,
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
        manager.dispose_native_ptr(ptr);
        let allocator = manager.native_allocator().unwrap();
        assert!(allocator.ptrs.is_empty());
        assert_eq!(
            allocator.free_ptr_blocks,
            vec![ProcessPtrRecord { ptr, size: 20 }]
        );
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
        manager.merge_metadata(HashMap::from([(0x2200, 0x1100)]), HashMap::new());

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
        ]);
        assert_eq!(manager.handle_for_ptr(0x2200), Some(0x1100));
        assert_eq!(manager.handle_for_ptr(0x4400), Some(0x3300));
        assert_eq!(manager.handle_for_ptr(0x6600), Some(0x5500));
        assert_eq!(manager.handle_state(0x3300), 0x80);
        assert_eq!(manager.handle_state(0x5500), 0x40);
        assert_eq!(manager.native_allocation(0x3300).unwrap().size, 16);

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
