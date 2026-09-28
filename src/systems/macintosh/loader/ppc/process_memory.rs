//! PowerPC process memory manager and PixMap geometry descriptors.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcPixMapBits {
    pub(crate) base_addr: u32,
    pub(crate) row_bytes: u32,
    pub(crate) top: i16,
    pub(crate) left: i16,
    pub(crate) bottom: i16,
    pub(crate) right: i16,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcResolvedPixMapBits {
    pub(crate) bits: PpcPixMapBits,
    pub(crate) guest_bounds: bool,
    pub(crate) authoritative_bounds: bool,
}

#[derive(Debug, Default)]
pub(crate) struct PpcProcessMemoryManager(pub(crate) SharedProcessMemoryManager);

#[cfg(test)]
pub(crate) struct PpcTestHeapCursor {
    pub(crate) memory_manager: SharedProcessMemoryManager,
    pub(crate) value: u32,
}

#[cfg(test)]
pub(crate) struct PpcTestHandles {
    pub(crate) memory_manager: SharedProcessMemoryManager,
    pub(crate) value: Vec<PpcHandleRecord>,
}

#[cfg(test)]
impl std::ops::Deref for PpcTestHandles {
    type Target = Vec<PpcHandleRecord>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[cfg(test)]
impl std::ops::DerefMut for PpcTestHandles {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

#[cfg(test)]
impl Drop for PpcTestHandles {
    fn drop(&mut self) {
        let mut memory_manager = self.memory_manager.borrow_mut();
        let records: Vec<_> = self
            .value
            .iter()
            .map(|record| {
                (
                    *record,
                    memory_manager
                        .state_for_handle(record.handle)
                        .unwrap_or(0x40),
                )
            })
            .collect();
        memory_manager.register_native_handle_records(records);
    }
}

#[cfg(test)]
impl std::ops::Deref for PpcTestHeapCursor {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[cfg(test)]
impl std::ops::DerefMut for PpcTestHeapCursor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

#[cfg(test)]
impl Drop for PpcTestHeapCursor {
    fn drop(&mut self) {
        self.memory_manager
            .borrow_mut()
            .mutate_native_allocator(|allocator| {
                allocator.heap.heap_cursor = self.value;
            });
    }
}

impl Clone for PpcProcessMemoryManager {
    fn clone(&self) -> Self {
        Self(self.0.detached_clone())
    }
}

impl PpcProcessMemoryManager {
    pub(crate) fn with_heap(heap_cursor: u32, heap_limit: u32) -> Self {
        let memory_manager = Self::default();
        {
            let mut process_memory_manager = memory_manager.0.borrow_mut();
            let process_memory_manager = process_memory_manager.native_mut();
            process_memory_manager.publish_native_allocator(
                ProcessNativeHeapState {
                    heap_base: PPC_HEAP_BASE,
                    heap_cursor,
                    heap_limit,
                    last_mem_error: PPC_NO_ERR,
                    heap_maximized: false,
                    master_pointer_blocks_requested: 0,
                },
                &[],
                &[],
                &[],
            );
            // Inside Macintosh: Memory (1992), pp. 2-83--2-85: the
            // application limit is an API-visible heap/stack boundary, not
            // the allocator's physical mapping ceiling.
            process_memory_manager.set_application_heap_limit(heap_limit);
        }
        memory_manager
    }

    pub(crate) fn attach_to(&mut self, memory_manager: SharedProcessMemoryManager) {
        self.0 = memory_manager;
    }

    pub(crate) fn ptr_eq(&self, memory_manager: &SharedProcessMemoryManager) -> bool {
        self.0.ptr_eq(memory_manager)
    }

    pub(crate) fn heap_limit(&self, fallback: u32) -> u32 {
        self.0
            .borrow()
            .native_heap_state()
            .map_or(fallback, |heap| heap.heap_limit)
    }

    pub(crate) fn heap_cursor(&self, fallback: u32) -> u32 {
        self.0
            .borrow()
            .native_heap_state()
            .map_or(fallback, |heap| heap.heap_cursor)
    }

    pub(crate) fn last_mem_error(&self) -> i16 {
        self.0
            .borrow()
            .native_heap_state()
            .map_or(0, |heap| heap.last_mem_error)
    }

    pub(crate) fn application_heap_limit(&self, fallback: u32) -> u32 {
        self.0.borrow().application_heap_limit(fallback)
    }

    #[cfg(test)]
    pub(crate) fn heap_cursor_mut(&self) -> PpcTestHeapCursor {
        PpcTestHeapCursor {
            memory_manager: self.0.clone(),
            value: self.heap_cursor(PPC_HEAP_BASE),
        }
    }

    #[cfg(test)]
    pub(crate) fn handles_mut(&self) -> PpcTestHandles {
        PpcTestHandles {
            memory_manager: self.0.clone(),
            value: self.0.borrow().native_handle_records().to_vec(),
        }
    }

    pub(crate) fn handles(&self) -> Vec<PpcHandleRecord> {
        self.0.borrow().native_handle_records().to_vec()
    }
}
