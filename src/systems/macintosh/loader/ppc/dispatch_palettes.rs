//! Typed Palette Manager association dispatch for PowerPC imports.

use super::*;

const PM_NO_UPDATES: u16 = 0x8000;
const PM_BACKGROUND_UPDATES: u16 = 0xa000;
const PM_FOREGROUND_UPDATES: u16 = 0xc000;
const PM_ALL_UPDATES: u16 = 0xe000;

fn palette_requests_update(mode: u16, foreground: bool) -> bool {
    match mode {
        PM_ALL_UPDATES | 1 => true,
        PM_FOREGROUND_UPDATES => foreground,
        PM_BACKGROUND_UPDATES => !foreground,
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn enqueue_palette_updates(
    memory: &mut PpcSectionMem,
    startup: &PpcToolboxStartupState,
    window_list: &SharedProcessWindowList,
    gworlds: &[PpcGWorldRecord],
    front: u32,
    gdevice: u32,
    fallback_front_update: bool,
    event_queue: &mut EventQueue,
    tick_count: u32,
    input: PpcInputSnapshot,
) {
    window_list.with_ref(|windows| {
        for &window in windows {
            if !ppc_window_is_visible(memory, window)
                || ppc_gworld_device(gworlds, window).is_some_and(|device| device != gdevice)
            {
                continue;
            }
            let associated = ppc_window_palette_handle(memory, startup, window);
            let palette = if associated == 0 {
                startup.application_palette
            } else {
                associated
            };
            let foreground = window == front;
            let requests_update = startup
                .palette_update_modes
                .get(&palette)
                .map_or(foreground && fallback_front_update, |&mode| {
                    palette_requests_update(mode, foreground)
                });
            if requests_update {
                ppc_enqueue_window_update_event(event_queue, window, tick_count, input);
            }
        }
    });
}

pub(super) struct PpcPaletteDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
    pub(super) current_resource_refnum: &'a i16,
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) window_list: &'a SharedProcessWindowList,
    pub(super) current_gdevice: u32,
    pub(super) screen_clut: &'a mut [[u16; 3]; 256],
    pub(super) color_manager_clut: &'a mut [[u16; 3]; 256],
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) event_queue: &'a mut EventQueue,
    pub(super) tick_count: u32,
    pub(super) input: PpcInputSnapshot,
}

pub(super) fn dispatch_palette_import(
    context: PpcPaletteDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcPaletteDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        current_resource_refnum,
        gworlds,
        window_list,
        current_gdevice,
        screen_clut,
        color_manager_clut,
        toolbox_startup,
        event_queue,
        tick_count,
        input,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InitPalettes | PpcImportDispatcherTarget::InitWindows => {
            ppc_initialize_application_palette(
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                *current_resource_refnum,
                toolbox_startup,
            );
            if binding.dispatcher_target == PpcImportDispatcherTarget::InitPalettes {
                Some(PpcImportAction::ReturnPreserve)
            } else {
                // InitWindows must still initialize the Window Manager below.
                None
            }
        }
        PpcImportDispatcherTarget::ActivatePalette => {
            let window_ptr = cpu.gpr[3];
            // Inside Macintosh Volume VI (1991), p. 20-19: an explicit
            // ActivatePalette affects only the visible frontmost window;
            // calling it for an offscreen port has no effect.
            if ppc_front_visible_process_window(memory, window_list) == Some(window_ptr) {
                if let Some(gdevice) = ppc_gworld_device(gworlds, window_ptr) {
                    let applied = ppc_activate_window_palette(
                        memory,
                        window_ptr,
                        gdevice,
                        current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                    if applied {
                        enqueue_palette_updates(
                            memory,
                            toolbox_startup,
                            window_list,
                            gworlds,
                            window_ptr,
                            gdevice,
                            true,
                            event_queue,
                            tick_count,
                            input,
                        );
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetPaletteUpdates => {
            let palette = cpu.gpr[3];
            if palette != 0 {
                // Inside Macintosh: Advanced Color Imaging, Palette Manager.
                // The update policy belongs to the palette, independent of
                // how many windows currently use it.
                toolbox_startup
                    .palette_update_modes
                    .insert(palette, cpu.gpr[4] as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetPaletteUpdates => {
            let palette = cpu.gpr[3];
            let updates = toolbox_startup
                .palette_update_modes
                .get(&palette)
                .copied()
                .unwrap_or_else(|| {
                    if palette != 0 && memory.read_u32_be(palette).is_some_and(|ptr| ptr != 0) {
                        PM_ALL_UPDATES
                    } else {
                        0
                    }
                });
            Some(PpcImportAction::Return(ppc_i16_result(updates as i16)))
        }
        PpcImportDispatcherTarget::NSetPalette => {
            let window_ptr = cpu.gpr[3];
            let palette_handle = cpu.gpr[4];
            let updates = cpu.gpr[5] as u16;
            if palette_handle != 0 && window_ptr != 0 {
                let mode = if binding.symbol_name == "SetPalette" {
                    if updates as u8 == 0 {
                        PM_NO_UPDATES
                    } else {
                        PM_ALL_UPDATES
                    }
                } else {
                    updates
                };
                toolbox_startup
                    .palette_update_modes
                    .insert(palette_handle, mode);
            }
            let previous_palette_handle = if window_ptr == u32::MAX {
                toolbox_startup.application_palette
            } else {
                ppc_window_palette_handle(memory, toolbox_startup, window_ptr)
            };
            if window_ptr == u32::MAX {
                // Inside Macintosh Volume VI (1991), p. 20-16: WindowPtr(-1)
                // selects the application's default palette and retains the
                // same update policy accepted for an ordinary window.
                toolbox_startup.application_palette = palette_handle;
                toolbox_startup.application_palette_updates = updates;
            } else if window_ptr != 0
                && memory.read_u16_be(window_ptr.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET))
                    == Some(crate::dialog_manager::DIALOG_WINDOW_KIND)
            {
                toolbox_startup
                    .dialog_palettes
                    .insert(window_ptr, (palette_handle, updates));
            } else if window_ptr != 0
                && ppc_memory_can_write_bytes(
                    memory,
                    window_ptr.wrapping_add(PPC_CGRAF_PORT_PALETTE_UPDATES_OFFSET),
                    2,
                )
            {
                let _ = memory.write_u32_be(
                    window_ptr.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET),
                    palette_handle,
                );
                let _ = memory.write_u16_be(
                    window_ptr.wrapping_add(PPC_CGRAF_PORT_PALETTE_UPDATES_OFFSET),
                    updates,
                );
            }
            if previous_palette_handle != 0 && previous_palette_handle != palette_handle {
                let still_associated = toolbox_startup.application_palette
                    == previous_palette_handle
                    || gworlds.iter().any(|record| {
                        ppc_window_palette_handle(memory, toolbox_startup, record.port)
                            == previous_palette_handle
                    });
                if !still_associated {
                    ppc_release_palette_allocations_and_restore(
                        memory,
                        toolbox_startup,
                        previous_palette_handle,
                        current_gdevice,
                        screen_clut,
                        color_manager_clut,
                    );
                }
            }
            let previous_device_colors = *screen_clut;
            let front_window = ppc_front_visible_process_window(memory, window_list);
            let activation_window = if window_ptr == u32::MAX {
                front_window.unwrap_or(PPC_MAIN_GWORLD)
            } else {
                window_ptr
            };
            let front_uses_default = front_window.map_or(true, |front| {
                ppc_window_palette_handle(memory, toolbox_startup, front) == 0
            });
            let applies_now = if window_ptr == u32::MAX {
                front_window.is_none() || front_uses_default
            } else {
                window_ptr == PPC_MAIN_GWORLD || front_window == Some(window_ptr)
            };
            let gdevice = ppc_gworld_device(gworlds, activation_window).unwrap_or(current_gdevice);
            let applied = activation_window != 0
                && applies_now
                && ppc_activate_window_palette(
                    memory,
                    activation_window,
                    gdevice,
                    current_gdevice,
                    screen_clut,
                    color_manager_clut,
                    toolbox_startup,
                );
            if applied
                && *screen_clut != previous_device_colors
                && previous_palette_handle != 0
                && previous_palette_handle != palette_handle
                && window_ptr != u32::MAX
            {
                // Inside Macintosh Volume VI (1991), pp. 20-20--20-21:
                // activating a changed color environment invalidates windows
                // whose SetPalette/NSetPalette update policy requests it.
                if window_ptr == PPC_MAIN_GWORLD {
                    if palette_requests_update(updates, true) {
                        ppc_enqueue_window_update_event(event_queue, window_ptr, tick_count, input);
                    }
                } else {
                    enqueue_palette_updates(
                        memory,
                        toolbox_startup,
                        window_list,
                        gworlds,
                        window_ptr,
                        gdevice,
                        updates != 0,
                        event_queue,
                        tick_count,
                        input,
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetPalette => {
            let window_ptr = cpu.gpr[3];
            let palette = if window_ptr == u32::MAX {
                toolbox_startup.application_palette
            } else if window_ptr != 0 {
                ppc_window_palette_handle(memory, toolbox_startup, window_ptr)
            } else {
                0
            };
            Some(PpcImportAction::Return(palette))
        }
        _ => None,
    }
}

/// Advanced Color Imaging, "Designating a Default Palette for Your Application":
/// InitWindows calls InitPalettes; pltt 0 is searched in the current resource file
/// only, then in the System file, finally falling back to black and white.
fn ppc_initialize_application_palette(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    resources: &[PpcVfsResourceRecord],
    current_refnum: i16,
    startup: &mut PpcToolboxStartupState,
) {
    // Repeated initialization must not replace an application-selected palette
    // or leak the palette installed by the implicit InitWindows call.
    if startup.application_palette != 0 {
        return;
    }
    let valid_palette = |bytes: &[u8]| {
        bytes
            .get(..2)
            .map(|count| u16::from_be_bytes([count[0], count[1]]))
            .filter(|count| bytes.len() >= 16 + usize::from(*count) * 16)
    };
    let resource = [current_refnum, 0].into_iter().find_map(|refnum| {
        resources.iter().find(|resource| {
            resource.ref_num == refnum
                && resource.res_type == u32::from_be_bytes(*b"pltt")
                && resource.res_id == 0
                && valid_palette(&resource.data).is_some()
        })
    });
    let mut bytes = if let Some(resource) = resource {
        let count = valid_palette(&resource.data).unwrap();
        resource.data[..16 + usize::from(count) * 16].to_vec()
    } else {
        let mut bytes = vec![0; 48];
        bytes[..2].copy_from_slice(&2u16.to_be_bytes());
        bytes[16..22].fill(0xff); // white, then black; courteous entries
        bytes
    };
    let count = u16::from_be_bytes([bytes[0], bytes[1]]);
    bytes[2..16].fill(0);
    for entry in 0..usize::from(count) {
        bytes[16 + entry * 16 + 10..16 + (entry + 1) * 16].fill(0);
    }
    let mut allocator = PpcProcessAllocatorView {
        memory_manager: process_memory_manager,
    };
    let handle = ppc_allocator_view_allocate_handle_with_bytes(
        Some(&mut allocator),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        &bytes,
    );
    if handle != 0 {
        startup.application_palette = handle;
        startup.application_palette_updates = 0;
    }
}
