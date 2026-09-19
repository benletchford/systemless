//! Typed Palette Manager association dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcPaletteDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
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
        PpcImportDispatcherTarget::ActivatePalette => {
            let window_ptr = cpu.gpr[3];
            // Inside Macintosh Volume VI (1991), p. 20-19: an explicit
            // ActivatePalette affects only the visible frontmost window;
            // calling it for an offscreen port has no effect.
            let applied = ppc_front_visible_process_window(memory, window_list) == Some(window_ptr)
                && ppc_gworld_device(gworlds, window_ptr).is_some_and(|gdevice| {
                    ppc_activate_window_palette(
                        memory,
                        window_ptr,
                        gdevice,
                        current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    )
                });
            if applied {
                // Inside Macintosh Volume VI 1991, p. 20-20: changing the
                // active color environment generates update events for
                // windows that need to redraw under the new palette.
                ppc_enqueue_window_update_event(event_queue, window_ptr, tick_count, input);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::NSetPalette => {
            let window_ptr = cpu.gpr[3];
            let palette_handle = cpu.gpr[4];
            let updates = cpu.gpr[5] as u16;
            let previous_palette_handle = if window_ptr == u32::MAX {
                toolbox_startup.application_palette
            } else {
                memory
                    .read_u32_be(window_ptr.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
                    .unwrap_or(0)
            };
            if window_ptr == u32::MAX {
                // Inside Macintosh Volume VI (1991), p. 20-16: WindowPtr(-1)
                // selects the application's default palette and retains the
                // same update policy accepted for an ordinary window.
                toolbox_startup.application_palette = palette_handle;
                toolbox_startup.application_palette_updates = updates;
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
                        memory
                            .read_u32_be(
                                record
                                    .port
                                    .wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET),
                            )
                            .unwrap_or(0)
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
            let front_uses_default = front_window
                .and_then(|front| {
                    memory.read_u32_be(front.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
                })
                .unwrap_or(0)
                == 0;
            let applies_now = if window_ptr == u32::MAX {
                front_window.is_none() || front_uses_default
            } else {
                window_ptr == PPC_MAIN_GWORLD || front_window == Some(window_ptr)
            };
            let gdevice =
                ppc_gworld_device(gworlds, activation_window).unwrap_or(current_gdevice);
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
                && updates != 0
                && window_ptr != u32::MAX
            {
                // Inside Macintosh Volume VI (1991), pp. 20-20--20-21:
                // activating a changed color environment invalidates windows
                // whose SetPalette/NSetPalette update policy requests it.
                ppc_enqueue_window_update_event(event_queue, window_ptr, tick_count, input);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetPalette => {
            let window_ptr = cpu.gpr[3];
            let palette = if window_ptr == u32::MAX {
                toolbox_startup.application_palette
            } else if window_ptr != 0
                && ppc_memory_can_read_bytes(
                    memory,
                    window_ptr.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET),
                    4,
                )
            {
                memory
                    .read_u32_be(window_ptr.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
                    .unwrap_or(0)
            } else {
                0
            };
            Some(PpcImportAction::Return(palette))
        }
        _ => None,
    }
}
