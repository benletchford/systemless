//! Typed QuickDraw Graphics Device Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcGraphicsDeviceDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) gworlds: &'a mut [PpcGWorldRecord],
    pub(super) current_gdevice: &'a mut u32,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) screen_clut: &'a mut [[u16; 3]; 256],
    pub(super) color_manager_clut: &'a mut [[u16; 3]; 256],
}

pub(super) fn dispatch_graphics_device_import(
    context: PpcGraphicsDeviceDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcGraphicsDeviceDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        current_gdevice,
        toolbox_startup,
        screen_clut,
        color_manager_clut,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetGDevice => Some(PpcImportAction::Return(*current_gdevice)),
        PpcImportDispatcherTarget::SetGDevice => {
            if cpu.gpr[3] != 0 {
                *current_gdevice = cpu.gpr[3];
                ppc_register_gdevice(toolbox_startup, cpu.gpr[3]);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetDeviceList | PpcImportDispatcherTarget::GetMainDevice => {
            Some(PpcImportAction::Return(PPC_MAIN_GDEVICE))
        }
        PpcImportDispatcherTarget::GetMaxDevice => {
            let device = ppc_read_rect(memory, cpu.gpr[3])
                .filter(|&(top, left, bottom, right)| {
                    // Inside Macintosh: Imaging With QuickDraw (1994),
                    // pp. 5-27–5-28: GetMaxDevice considers only graphics
                    // devices intersecting the supplied global rectangle.
                    bottom > 0
                        && right > 0
                        && top < PPC_MAIN_SCREEN_HEIGHT as i16
                        && left < PPC_MAIN_SCREEN_WIDTH as i16
                })
                .map_or(0, |_| PPC_MAIN_GDEVICE);
            Some(PpcImportAction::Return(device))
        }
        PpcImportDispatcherTarget::GetNextDevice => Some(PpcImportAction::Return(0)),
        PpcImportDispatcherTarget::TestDeviceAttribute => Some(PpcImportAction::Return(
            ppc_test_device_attribute(cpu, memory),
        )),
        PpcImportDispatcherTarget::SetDeviceAttribute => {
            ppc_set_device_attribute(cpu, memory);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HasDepth => {
            // Imaging With QuickDraw (1994), pp. 5-33--5-34: return a
            // nonzero mode ID only when the requested device, pixel depth,
            // and color mode can be imposed by SetDepth.
            Some(PpcImportAction::Return(ppc_has_depth(cpu, memory)))
        }
        PpcImportDispatcherTarget::SetDepth => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            Some(PpcImportAction::Return(ppc_i16_result(ppc_set_depth(
                cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                toolbox_startup,
                screen_clut,
                color_manager_clut,
            ))))
        }
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::NewGDevice,
        ) => {
            let handle = ppc_process_alloc_handle(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                62,
                true,
            );
            *last_mem_error = if handle == 0 {
                PPC_MEM_FULL_ERR
            } else {
                PPC_NO_ERR
            };
            ppc_register_gdevice(toolbox_startup, handle);
            Some(PpcImportAction::Return(handle))
        }
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::DisposeGDevice,
        ) => {
            toolbox_startup.active_device_palettes.remove(&cpu.gpr[3]);
            toolbox_startup
                .known_gdevices
                .retain(|&gdevice| gdevice != cpu.gpr[3]);
            toolbox_startup.clut_protected_by_device.remove(&cpu.gpr[3]);
            toolbox_startup.clut_reserved_by_device.remove(&cpu.gpr[3]);
            toolbox_startup
                .palette_allocations
                .retain(|allocation| allocation.gdevice != cpu.gpr[3]);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let _ = allocator.dispose_handle(
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
