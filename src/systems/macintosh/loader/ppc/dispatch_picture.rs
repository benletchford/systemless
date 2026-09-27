//! Typed Picture Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcPictureDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) current_gworld: u32,
    pub(super) screen_clut: &'a [[u16; 3]; 256],
    pub(super) color_manager_clut: &'a mut [[u16; 3]; 256],
}

pub(super) fn dispatch_picture_import(
    context: PpcPictureDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcPictureDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        gworlds,
        current_gworld,
        screen_clut,
        color_manager_clut,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetPictInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_get_pict_info(cpu, memory),
        ))),
        PpcImportDispatcherTarget::DrawPicture => {
            let _ = ppc_draw_picture(
                cpu,
                memory,
                handles,
                vfs_resources,
                gworlds,
                current_gworld,
                screen_clut,
                color_manager_clut,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::KillPicture => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_kill_picture(
                cpu,
                &mut allocator,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
