//! Typed Scrap Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcScrapDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) scrap: &'a mut PpcScrapState,
}

pub(super) fn dispatch_scrap_import(
    context: PpcScrapDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcScrapDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        scrap,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetScrap => {
            // Inside Macintosh: More Macintosh Toolbox (1993), pp. 2-38--2-40:
            // return the first matching flavor, resize the destination handle,
            // and report its offset in the ordered desktop scrap.
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            Some(PpcImportAction::Return(ppc_get_scrap(
                cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                scrap,
            )))
        }
        PpcImportDispatcherTarget::PutScrap => {
            // More Macintosh Toolbox (1993), pp. 2-35--2-37: successive calls
            // add ordered flavors; a repeated type remains a later occurrence.
            let result = if !scrap.desktop.initialized {
                PPC_NO_SCRAP_ERR
            } else if (cpu.gpr[3] as i32) < 0 {
                PPC_PARAM_ERR
            } else if let Some(bytes) = ppc_memory_read_bytes(memory, cpu.gpr[5], cpu.gpr[3]) {
                scrap.desktop.append_entry(cpu.gpr[4].to_be_bytes(), bytes);
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return((i32::from(result)) as u32))
        }
        PpcImportDispatcherTarget::ZeroScrap => {
            scrap.desktop.zero();
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::LoadScrap => {
            // The process-owned desktop scrap remains resident in HLE, so an
            // explicit disk-to-memory synchronization is already satisfied.
            scrap.desktop.load();
            Some(PpcImportAction::Return(0))
        }
        _ => None,
    }
}

fn ppc_scrap_flavor_offset(scrap: &PpcScrapState, flavor_index: usize) -> u32 {
    scrap
        .desktop
        .entries
        .iter()
        .take(flavor_index)
        .fold(0u32, |offset, (_, bytes)| {
            offset.saturating_add(bytes.len() as u32)
        })
}

fn ppc_get_scrap(
    cpu: &PpcCpu,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    scrap: &PpcScrapState,
) -> u32 {
    let Some((index, (_, bytes))) = scrap
        .desktop
        .entries
        .iter()
        .enumerate()
        .find(|(_, (flavor, _))| *flavor == cpu.gpr[4].to_be_bytes())
    else {
        if cpu.gpr[5] != 0 {
            let _ = memory.write_u32_be(cpu.gpr[5], 0);
        }
        return (i32::from(PPC_NO_TYPE_ERR)) as u32;
    };
    if cpu.gpr[5] != 0 {
        let _ = memory.write_u32_be(cpu.gpr[5], ppc_scrap_flavor_offset(scrap, index));
    }
    if cpu.gpr[3] != 0 {
        let result = ppc_allocator_view_resize_handle(
            allocator,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            cpu.gpr[3],
            bytes.len() as u32,
        );
        if result != PPC_NO_ERR {
            return (i32::from(result)) as u32;
        }
        if !bytes.is_empty() {
            let Some(ptr) = memory.read_u32_be(cpu.gpr[3]).filter(|ptr| *ptr != 0) else {
                return (i32::from(PPC_PARAM_ERR)) as u32;
            };
            if memory.write_bytes(ptr, bytes).is_none() {
                return (i32::from(PPC_PARAM_ERR)) as u32;
            }
        }
    }
    bytes.len() as u32
}
