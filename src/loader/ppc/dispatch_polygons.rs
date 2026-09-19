//! Typed QuickDraw Polygon Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcPolygonDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) current_gworld: u32,
    pub(super) quickdraw_fore_color: &'a PpcRgbColor,
    pub(super) quickdraw_fore_indices: &'a HashMap<u32, u8>,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_polygon_import(
    context: PpcPolygonDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcPolygonDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        current_gworld,
        quickdraw_fore_color,
        quickdraw_fore_indices,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::OpenPoly => Some(PpcImportAction::Return(ppc_open_poly(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
            handles,
            current_gworld,
        ))),
        PpcImportDispatcherTarget::ClosePoly => {
            if current_gworld != 0 {
                let _ = memory.write_u32_be(current_gworld.wrapping_add(100), 0);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::KillPoly => {
            let polygon = cpu.gpr[3];
            if ppc_open_polygon(memory, current_gworld) == Some(polygon) {
                let _ = memory.write_u32_be(current_gworld.wrapping_add(100), 0);
            }
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let _ = allocator.dispose_handle(
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                polygon,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FramePoly => {
            if toolbox_startup.open_region_port == current_gworld {
                ppc_open_region_include_polygon(toolbox_startup, memory, cpu.gpr[3]);
            } else {
                let _ = ppc_frame_polygon(
                    memory,
                    gworlds,
                    current_gworld,
                    cpu.gpr[3],
                    *quickdraw_fore_color,
                    quickdraw_fore_indices.get(&current_gworld).copied(),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PaintPoly | PpcImportDispatcherTarget::FillPoly => {
            if toolbox_startup.open_region_port == current_gworld {
                ppc_open_region_include_polygon(toolbox_startup, memory, cpu.gpr[3]);
            } else {
                let _ = ppc_paint_polygon(
                    memory,
                    gworlds,
                    current_gworld,
                    cpu.gpr[3],
                    *quickdraw_fore_color,
                    quickdraw_fore_indices.get(&current_gworld).copied(),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
