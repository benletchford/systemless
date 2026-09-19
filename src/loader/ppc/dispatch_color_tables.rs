//! Typed Color QuickDraw color-table and PixMap dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcColorTableDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(super) current_resource_refnum: &'a i16,
    pub(super) current_gworld: &'a u32,
    pub(super) quickdraw_hilite_colors: &'a SharedProcessQuickDrawHiliteColors,
    pub(super) tick_count: &'a u32,
    pub(super) current_gdevice: &'a u32,
    pub(super) screen_clut: &'a mut [[u16; 3]; 256],
    pub(super) color_manager_clut: &'a mut [[u16; 3]; 256],
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_color_table_import(
    context: PpcColorTableDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcColorTableDispatchContext {
        binding,
        cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        current_resource_refnum,
        current_gworld,
        quickdraw_hilite_colors,
        tick_count,
        current_gdevice,
        screen_clut,
        color_manager_clut,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetCTable => {
            let handle = ppc_get_ctable(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                vfs_resources,
                *current_resource_refnum,
                *current_gworld,
                quickdraw_hilite_colors,
            );
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] GetCTable tick={} id={} lr=${:08X} -> handle=${handle:08X}",
                    *tick_count, cpu.gpr[3] as u16 as i16, cpu.lr
                );
            }
            Some(PpcImportAction::Return(handle))
        }
        PpcImportDispatcherTarget::GetCTSeed => {
            // Inside Macintosh Volume V (1986), p. V-143: application color
            // table seeds are unique and greater than minSeed (1023).
            Some(PpcImportAction::Return(ppc_next_ct_seed(toolbox_startup)))
        }
        PpcImportDispatcherTarget::MakeITable => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_make_itable(
                cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                *current_gdevice,
                color_manager_clut,
                toolbox_startup,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CTabChanged => {
            let color_table_handle = cpu.gpr[3];
            if let Some(color_table) = memory.read_u32_be(color_table_handle) {
                if color_table != 0 {
                    let seed = ppc_next_ct_seed(toolbox_startup);
                    let _ = memory.write_u32_be(color_table, seed);
                    if ppc_gdevice_ctable_handle(memory, *current_gdevice)
                        == Some(color_table_handle)
                    {
                        // CTabChanged invalidates cached structures after an
                        // application edits a ColorTable directly. The next
                        // indexed mapping rebuilds the current GDevice's
                        // inverse table from those edited RGB entries.
                        if let Some(updated) =
                            ppc_read_ctable_clut(memory, color_table_handle, color_manager_clut)
                        {
                            *color_manager_clut = updated;
                        }
                    }
                }
            }
            if ppc_hle_trace_enabled() {
                eprintln!("[PPC-TRACE] CTabChanged handle=${color_table_handle:08X}");
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ProtectEntry => {
            let index = cpu.gpr[3] as u16 as i16;
            let flags = ppc_device_clut_protected_mut(toolbox_startup, *current_gdevice);
            ppc_set_clut_entry_flag(flags, index, cpu.gpr[4] & 0xff != 0);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ReserveEntry => {
            let index = cpu.gpr[3] as u16 as i16;
            let flags = ppc_device_clut_reserved_mut(toolbox_startup, *current_gdevice);
            ppc_set_clut_entry_flag(flags, index, cpu.gpr[4] & 0xff != 0);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RestoreEntries => {
            ppc_restore_entries(cpu, memory, *current_gdevice, screen_clut);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetEntries => {
            ppc_set_entries(
                cpu,
                memory,
                *current_gdevice,
                screen_clut,
                color_manager_clut,
                *tick_count,
                toolbox_startup,
            );
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] SetEntries tick={} start={} count={} table=${:08X} clut0=({:04X},{:04X},{:04X}) clut42=({:04X},{:04X},{:04X}) clut128=({:04X},{:04X},{:04X}) clut245=({:04X},{:04X},{:04X}) clut255=({:04X},{:04X},{:04X})",
                    *tick_count,
                    cpu.gpr[3] as u16 as i16,
                    cpu.gpr[4] as u16 as i16,
                    cpu.gpr[5],
                    screen_clut[0][0],
                    screen_clut[0][1],
                    screen_clut[0][2],
                    screen_clut[42][0],
                    screen_clut[42][1],
                    screen_clut[42][2],
                    screen_clut[128][0],
                    screen_clut[128][1],
                    screen_clut[128][2],
                    screen_clut[245][0],
                    screen_clut[245][1],
                    screen_clut[245][2],
                    screen_clut[255][0],
                    screen_clut[255][1],
                    screen_clut[255][2],
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RestoreDeviceClut => {
            ppc_restore_device_clut(
                memory,
                cpu.gpr[3],
                *current_gdevice,
                screen_clut,
                color_manager_clut,
                toolbox_startup,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DisposeCTable => {
            let ctable_handle = cpu.gpr[3];
            let _ = ppc_dispose_process_native_handle(
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                ctable_handle,
            );
            toolbox_startup
                .indexed_screen_ctables
                .retain(|_, handle| *handle != ctable_handle);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::NewPixMap => Some(PpcImportAction::Return(ppc_new_pixmap(
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            *current_gdevice,
        ))),
        PpcImportDispatcherTarget::DisposePixMap => {
            ppc_dispose_pixmap(
                cpu.gpr[3],
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                &mut toolbox_startup.indexed_screen_ctables,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
