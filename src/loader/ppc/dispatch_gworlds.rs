//! Typed QuickDraw GWorld and graphics-port dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcGWorldDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) gworlds: &'a mut Vec<PpcGWorldRecord>,
    pub(super) gworld_pixel_states: &'a SharedProcessQuickDrawPixelStates,
    pub(super) current_gworld: &'a mut u32,
    pub(super) current_gdevice: &'a mut u32,
    pub(super) quickdraw_op_colors: &'a SharedProcessQuickDrawOpColors,
    pub(super) quickdraw_hilite_colors: &'a SharedProcessQuickDrawHiliteColors,
    pub(super) quickdraw_fore_color: &'a mut PpcRgbColor,
    pub(super) quickdraw_fore_indices: &'a mut HashMap<u32, u8>,
    pub(super) quickdraw_back_color: &'a mut PpcRgbColor,
    pub(super) quickdraw_pen_h: &'a mut i16,
    pub(super) quickdraw_pen_v: &'a mut i16,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_gworld_import(
    context: PpcGWorldDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcGWorldDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        gworld_pixel_states,
        current_gworld,
        current_gdevice,
        quickdraw_op_colors,
        quickdraw_hilite_colors,
        quickdraw_fore_color,
        quickdraw_fore_indices,
        quickdraw_back_color,
        quickdraw_pen_h,
        quickdraw_pen_v,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetPort => {
            let port_ptr = cpu.gpr[3];
            if port_ptr != 0 && ppc_memory_can_write_bytes(memory, port_ptr, 4) {
                let _ = memory.write_u32_be(port_ptr, *current_gworld);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetPort => {
            *current_gworld = cpu.gpr[3];
            *current_gdevice =
                ppc_gworld_device(gworlds, *current_gworld).unwrap_or(*current_gdevice);
            ppc_register_gdevice(toolbox_startup, *current_gdevice);
            ppc_restore_port_colors(
                memory,
                *current_gworld,
                quickdraw_fore_color,
                quickdraw_back_color,
            );
            if ppc_gworld_trace_enabled() {
                eprintln!(
                    "[PPC-GWORLD-TRACE] SetPort port=${:08X} gdevice=${:08X}",
                    *current_gworld, *current_gdevice
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::NewGWorld => {
            let result = ppc_new_gworld(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                &mut toolbox_startup.gworld_allocations,
                *current_gdevice,
            );
            if result == PPC_NO_ERR {
                let out_ptr = cpu.gpr[3];
                let flags = cpu.gpr[8];
                let port = memory.read_u32_be(out_ptr).unwrap_or(0);
                let pixmap_handle = memory.read_u32_be(port.wrapping_add(2)).unwrap_or(0);
                if pixmap_handle != 0 {
                    let mut state = 0;
                    if flags & (1 << 0) != 0 {
                        state |= PPC_PIXELS_PURGEABLE;
                    }
                    if flags & PPC_KEEP_LOCAL != 0 {
                        state |= PPC_KEEP_LOCAL;
                    }
                    gworld_pixel_states.set_quickdraw_pixel_state(pixmap_handle, state);
                    ppc_sync_gworld_pixel_state_mirror(gworlds, pixmap_handle, state);
                }
            }
            // Imaging With QuickDraw (1994), pp. 6-20 and 6-24: QDError
            // reports NewGWorld and UpdateGWorld failures, and a successful
            // call clears the previous QuickDraw error.
            toolbox_startup.last_quickdraw_error.set(result);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::UpdateGWorld => {
            let old_port = memory.read_u32_be(cpu.gpr[3]).unwrap_or(0);
            let old_pixmap_handle = ppc_gworld_pixmap(memory, gworlds, old_port);
            if old_pixmap_handle != 0 {
                let _ = ppc_ensure_gworld_pixel_state(
                    gworlds,
                    gworld_pixel_states,
                    old_pixmap_handle,
                );
            }
            let result = ppc_update_gworld(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                &mut toolbox_startup.gworld_allocations,
                current_gworld,
                current_gdevice,
            );
            // Imaging With QuickDraw (1994), p. 6-24: after gwFlagErr the
            // caller uses QDError to obtain the reason UpdateGWorld failed.
            let quickdraw_error = if result & (1 << 31) != 0 {
                *last_mem_error
            } else {
                PPC_NO_ERR
            };
            toolbox_startup.last_quickdraw_error.set(quickdraw_error);
            if result & (1 << 31) == 0 {
                ppc_register_gdevice(toolbox_startup, *current_gdevice);
            }
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::DisposeGWorld => {
            // DisposeGWorld
            // Disposes every allocation owned by an offscreen graphics world.
            // void DisposeGWorld(GWorldPtr offscreenGWorld);
            // Imaging With QuickDraw (1994), p. 6-25.
            let port = cpu.gpr[3];
            let disposed = gworlds
                .iter()
                .find(|gworld| gworld.port != PPC_MAIN_GWORLD && gworld.port == port)
                .copied();
            gworlds.retain(|gworld| gworld.port == PPC_MAIN_GWORLD || gworld.port != port);
            if let Some(record) = disposed {
                quickdraw_fore_indices.remove(&port);
                quickdraw_op_colors.remove_quickdraw_op_color(port);
                quickdraw_hilite_colors.remove_quickdraw_hilite_color(port);
                let allocation = toolbox_startup.gworld_allocations.remove(&port);
                let saved_ctable = toolbox_startup
                    .indexed_screen_ctables
                    .remove(&record.pixmap_handle)
                    .unwrap_or(0);
                let live_ctable = memory
                    .read_u32_be(record.pixmap + 42)
                    .filter(|handle| *handle != 0)
                    .unwrap_or(saved_ctable);
                let owned_ctable = allocation.map_or(0, |allocation| allocation.ctable_handle);
                let ctable_handle = if owned_ctable != 0 {
                    owned_ctable
                } else {
                    live_ctable
                };
                let fallback_storage_ptr = process_memory_manager
                    .native_ptr_records()
                    .iter()
                    .find(|allocation| {
                        ppc_allocation_size(allocation.size)
                            .and_then(|size| allocation.ptr.checked_add(size))
                            .is_some_and(|end| allocation.ptr <= port && port < end)
                    })
                    .map(|allocation| allocation.ptr);
                let mut owned_ptrs = Vec::with_capacity(2);
                if let Some(ptr) = allocation
                    .map(|allocation| allocation.storage_ptr)
                    .filter(|ptr| *ptr != 0)
                    .or(fallback_storage_ptr)
                {
                    owned_ptrs.push(ptr);
                }
                if let Some(pixel_ptr) = allocation
                    .map(|allocation| allocation.pixel_ptr)
                    .filter(|ptr| *ptr != 0 && !owned_ptrs.contains(ptr))
                {
                    owned_ptrs.push(pixel_ptr);
                }
                let ctable_reclaim_base = allocation.and_then(|allocation| {
                    handles
                        .iter()
                        .find(|handle| handle.handle == ctable_handle && ctable_handle != 0)
                        .filter(|handle| {
                            allocation.allocation_end == *heap_cursor
                                && handle
                                    .handle
                                    .checked_add(ppc_allocation_size(4).unwrap_or(0))
                                    == Some(handle.ptr)
                                && handle
                                    .ptr
                                    .checked_add(ppc_allocation_size(handle.capacity).unwrap_or(0))
                                    == Some(allocation.origin_base)
                        })
                        .map(|handle| handle.handle)
                });
                if ctable_handle != 0 && ctable_handle != PPC_MAIN_CTABLE_HANDLE {
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
                }
                let reclaim_base = allocation
                    .filter(|allocation| allocation.allocation_end == *heap_cursor)
                    .map(|allocation| ctable_reclaim_base.unwrap_or(allocation.origin_base))
                    .or_else(|| {
                        (ppc_allocation_size(PPC_CGRAF_PORT_SIZE)
                            .and_then(|size| record.port.checked_add(size))
                            == Some(*heap_cursor))
                        .then_some(record.base_addr)
                    });
                if let Some(reclaim_base) = reclaim_base {
                    let reclaimed_ptrs = owned_ptrs
                        .iter()
                        .copied()
                        .filter(|ptr| *ptr >= reclaim_base)
                        .collect::<Vec<_>>();
                    if !process_memory_manager.reclaim_native_heap_tail(
                        reclaim_base,
                        &reclaimed_ptrs,
                        (ctable_handle != 0 && ctable_handle != PPC_MAIN_CTABLE_HANDLE)
                            .then_some(ctable_handle),
                    ) {
                        for ptr in owned_ptrs {
                            let _ = process_memory_manager.dispose_native_ptr(ptr);
                        }
                    } else {
                        for ptr in owned_ptrs
                            .into_iter()
                            .filter(|ptr| !reclaimed_ptrs.contains(ptr))
                        {
                            let _ = process_memory_manager.dispose_native_ptr(ptr);
                        }
                    }
                } else {
                    for ptr in owned_ptrs {
                        let _ = process_memory_manager.dispose_native_ptr(ptr);
                    }
                }
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
            }
            if *current_gworld == port {
                *current_gworld = PPC_MAIN_GWORLD;
                *current_gdevice = PPC_MAIN_GDEVICE;
                ppc_restore_port_colors(
                    memory,
                    *current_gworld,
                    quickdraw_fore_color,
                    quickdraw_back_color,
                );
            }
            if ppc_gworld_trace_enabled() {
                eprintln!(
                    "[PPC-GWORLD-TRACE] DisposeGWorld port=${:08X} current=${:08X} remaining_gworlds={}",
                    port,
                    *current_gworld,
                    gworlds.len()
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::QDError => Some(PpcImportAction::Return(ppc_i16_result(
            *toolbox_startup.last_quickdraw_error,
        ))),
        PpcImportDispatcherTarget::GetGWorld => {
            let port_ptr = cpu.gpr[3];
            let device_ptr = cpu.gpr[4];
            if ppc_optional_output_can_write(memory, port_ptr, 4)
                && ppc_optional_output_can_write(memory, device_ptr, 4)
            {
                if port_ptr != 0 {
                    let _ = memory.write_u32_be(port_ptr, *current_gworld);
                }
                if device_ptr != 0 {
                    let _ = memory.write_u32_be(device_ptr, *current_gdevice);
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetGWorld => {
            *current_gworld = cpu.gpr[3];
            *current_gdevice = if cpu.gpr[4] != 0 {
                cpu.gpr[4]
            } else {
                ppc_gworld_device(gworlds, *current_gworld).unwrap_or(PPC_MAIN_GDEVICE)
            };
            if let Some((h, v)) = ppc_gworld_pen(memory, *current_gworld) {
                *quickdraw_pen_h = h;
                *quickdraw_pen_v = v;
            }
            ppc_restore_port_colors(
                memory,
                *current_gworld,
                quickdraw_fore_color,
                quickdraw_back_color,
            );
            if ppc_gworld_trace_enabled() {
                eprintln!(
                    "[PPC-GWORLD-TRACE] SetGWorld port=${:08X} requested_gdevice=${:08X} gdevice=${:08X}",
                    *current_gworld, cpu.gpr[4], *current_gdevice
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetGWorldDevice => Some(PpcImportAction::Return(
            ppc_gworld_device(gworlds, cpu.gpr[3]).unwrap_or(0),
        )),
        PpcImportDispatcherTarget::GetGWorldPixMap => {
            let gworld = cpu.gpr[3];
            Some(PpcImportAction::Return(ppc_gworld_pixmap(
                memory, gworlds, gworld,
            )))
        }
        PpcImportDispatcherTarget::OpenPort => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            if ppc_open_port(
                cpu,
                &mut allocator,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                *current_gdevice,
            ) {
                let pixmap_handle = ppc_gworld_pixmap(memory, gworlds, cpu.gpr[3]);
                if pixmap_handle != 0 && !gworld_pixel_states.has_quickdraw_pixel_state(pixmap_handle)
                {
                    gworld_pixel_states.set_quickdraw_pixel_state(pixmap_handle, 0);
                }
                ppc_sync_gworld_pixel_state_mirror(
                    gworlds,
                    pixmap_handle,
                    gworld_pixel_states.quickdraw_pixel_state(pixmap_handle),
                );
                quickdraw_fore_indices.remove(&cpu.gpr[3]);
                *current_gworld = cpu.gpr[3];
                ppc_restore_port_colors(
                    memory,
                    *current_gworld,
                    quickdraw_fore_color,
                    quickdraw_back_color,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::OpenCPort => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            if ppc_open_cport(
                cpu,
                &mut allocator,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                *current_gdevice,
            ) {
                let pixmap_handle = ppc_gworld_pixmap(memory, gworlds, cpu.gpr[3]);
                if pixmap_handle != 0 {
                    // A newly opened CPort starts with non-purgeable pixels;
                    // this record is not a GWorld allocation owner. Inside
                    // Macintosh: Imaging With QuickDraw (1994), pp. 6-34--6-35.
                    gworld_pixel_states.set_quickdraw_pixel_state(pixmap_handle, 0);
                    ppc_sync_gworld_pixel_state_mirror(gworlds, pixmap_handle, 0);
                }
                quickdraw_fore_indices.remove(&cpu.gpr[3]);
                *current_gworld = cpu.gpr[3];
                ppc_restore_port_colors(
                    memory,
                    *current_gworld,
                    quickdraw_fore_color,
                    quickdraw_back_color,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CloseCPort => {
            let port = cpu.gpr[3];
            let closed_pixmap_handle = gworlds
                .iter()
                .find(|gworld| {
                    gworld.port == port
                        && !matches!(gworld.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
                })
                .map(|gworld| gworld.pixmap_handle);
            let was_current = *current_gworld == port;
            ppc_close_cport(port, gworlds, current_gworld, current_gdevice);
            if let Some(pixmap_handle) = closed_pixmap_handle {
                toolbox_startup.indexed_screen_ctables.remove(&pixmap_handle);
                quickdraw_fore_indices.remove(&port);
            }
            quickdraw_op_colors.remove_quickdraw_op_color(port);
            quickdraw_hilite_colors.remove_quickdraw_hilite_color(port);
            if was_current && *current_gworld != port {
                ppc_restore_port_colors(
                    memory,
                    *current_gworld,
                    quickdraw_fore_color,
                    quickdraw_back_color,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetPortBits { color } => {
            ppc_set_port_bits(memory, *current_gworld, cpu.gpr[3], color);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetPixBaseAddr => {
            let pixmap_handle = cpu.gpr[3];
            let base = ppc_pix_base_addr(memory, pixmap_handle);
            if ppc_hle_trace_enabled() {
                let pixmap = memory.read_u32_be(pixmap_handle).unwrap_or(0);
                eprintln!(
                    "[PPC-TRACE] GetPixBaseAddr pc=${:08X} lr=${:08X} handle=${:08X} pixmap=${:08X} base=${:08X}",
                    cpu.pc, cpu.lr, pixmap_handle, pixmap, base
                );
            }
            Some(PpcImportAction::Return(base))
        }
        PpcImportDispatcherTarget::LockPixels => Some(PpcImportAction::Return(ppc_lock_pixels(
            gworlds,
            gworld_pixel_states,
            cpu.gpr[3],
        ))),
        PpcImportDispatcherTarget::UnlockPixels => {
            ppc_unlock_pixels(gworlds, gworld_pixel_states, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetPixelsState => {
            let state = ppc_ensure_gworld_pixel_state(gworlds, gworld_pixel_states, cpu.gpr[3])
                .unwrap_or(0);
            Some(PpcImportAction::Return(state))
        }
        PpcImportDispatcherTarget::SetPixelsState => {
            ppc_set_gworld_pixel_state(gworlds, gworld_pixel_states, cpu.gpr[3], cpu.gpr[4]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::AllowPurgePixels => {
            // Inside Macintosh: Imaging With QuickDraw (1994), pp. 6-34--6-35:
            // AllowPurgePixels marks unlocked pixel storage purgeable. The
            // HLE models the state bit; actual Memory Manager purging remains
            // outside this process-state migration.
            ppc_allow_purge_pixels(gworlds, gworld_pixel_states, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::NoPurgePixels => {
            ppc_no_purge_pixels(gworlds, gworld_pixel_states, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetOrigin => {
            ppc_set_port_origin(
                memory,
                *current_gworld,
                cpu.gpr[3] as u16 as i16,
                cpu.gpr[4] as u16 as i16,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
