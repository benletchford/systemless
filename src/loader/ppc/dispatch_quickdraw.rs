use super::*;

pub(super) struct PpcQuickDrawDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) tick_count: u32,
    pub(super) current_gworld: u32,
    pub(super) current_gdevice: u32,
    pub(super) quickdraw_op_colors: &'a SharedProcessQuickDrawOpColors,
    pub(super) quickdraw_hilite_colors: &'a SharedProcessQuickDrawHiliteColors,
    pub(super) screen_clut: &'a [[u16; 3]; 256],
    pub(super) color_manager_clut: &'a [[u16; 3]; 256],
    pub(super) quickdraw_fore_color: &'a mut PpcRgbColor,
    pub(super) quickdraw_fore_indices: &'a mut HashMap<u32, u8>,
    pub(super) quickdraw_back_color: &'a mut PpcRgbColor,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_quickdraw_import(
    context: PpcQuickDrawDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQuickDrawDispatchContext {
        binding,
        cpu,
        memory,
        tick_count,
        current_gworld,
        current_gdevice,
        quickdraw_op_colors,
        quickdraw_hilite_colors,
        screen_clut,
        color_manager_clut,
        quickdraw_fore_color,
        quickdraw_fore_indices,
        quickdraw_back_color,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetForeColor => {
            let color_ptr = cpu.gpr[3];
            if color_ptr != 0 && ppc_memory_can_write_bytes(memory, color_ptr, 6) {
                let color = ppc_port_rgb_colors(memory, current_gworld)
                    .map_or(*quickdraw_fore_color, |colors| colors.0);
                let _ = ppc_write_rgb_color(memory, color_ptr, color);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetBackColor => {
            let color_ptr = cpu.gpr[3];
            if color_ptr != 0 && ppc_memory_can_write_bytes(memory, color_ptr, 6) {
                let color = ppc_port_rgb_colors(memory, current_gworld)
                    .map_or(*quickdraw_back_color, |colors| colors.1);
                let _ = ppc_write_rgb_color(memory, color_ptr, color);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ForeColor => {
            quickdraw_fore_indices.remove(&current_gworld);
            *quickdraw_fore_color = ppc_legacy_qd_color_to_rgb(cpu.gpr[3]);
            let _ = ppc_write_port_rgb_color(
                memory,
                current_gworld,
                PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
                *quickdraw_fore_color,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::BackColor => {
            *quickdraw_back_color = ppc_legacy_qd_color_to_rgb(cpu.gpr[3]);
            let _ = ppc_write_port_rgb_color(
                memory,
                current_gworld,
                PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET,
                *quickdraw_back_color,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RGBForeColor => {
            if let Some(color) = ppc_read_rgb_color(memory, cpu.gpr[3]) {
                *quickdraw_fore_color = color;
                let main_8bpp = current_gdevice == PPC_MAIN_GDEVICE
                    && ppc_current_gdevice_record(memory, current_gdevice)
                        .and_then(|gdevice| memory.read_u32_be(gdevice.checked_add(22)?))
                        .and_then(|handle| memory.read_u32_be(handle))
                        .and_then(|pixmap| memory.read_u16_be(pixmap.checked_add(32)?))
                        == Some(8);
                if main_8bpp {
                    quickdraw_fore_indices.insert(
                        current_gworld,
                        ppc_color_to_index(memory, current_gdevice, color_manager_clut, color)
                            as u8,
                    );
                } else {
                    quickdraw_fore_indices.remove(&current_gworld);
                }
                let _ = ppc_write_port_rgb_color(
                    memory,
                    current_gworld,
                    PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
                    color,
                );
                if let Some(commands) =
                    ppc_open_picture_commands(toolbox_startup, current_gworld)
                {
                    pict::recording_push_word(commands, 0x001A);
                    for component in [color.red, color.green, color.blue] {
                        pict::recording_push_word(commands, component);
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RGBBackColor => {
            if let Some(color) = ppc_read_rgb_color(memory, cpu.gpr[3]) {
                *quickdraw_back_color = color;
                let main_8bpp = current_gdevice == PPC_MAIN_GDEVICE
                    && ppc_current_gdevice_record(memory, current_gdevice)
                        .and_then(|gdevice| memory.read_u32_be(gdevice.checked_add(22)?))
                        .and_then(|handle| memory.read_u32_be(handle))
                        .and_then(|pixmap| memory.read_u16_be(pixmap.checked_add(32)?))
                        == Some(8);
                if main_8bpp {
                    toolbox_startup.quickdraw_back_indices.insert(
                        current_gworld,
                        ppc_color_to_index(memory, current_gdevice, color_manager_clut, color)
                            as u8,
                    );
                } else {
                    toolbox_startup
                        .quickdraw_back_indices
                        .remove(&current_gworld);
                }
                let _ = ppc_write_port_rgb_color(
                    memory,
                    current_gworld,
                    PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET,
                    color,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::OpColor => {
            if let Some(color) = ppc_read_rgb_color(memory, cpu.gpr[3]) {
                // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62
                // and 4-64: OpColor updates the current CGrafPort's
                // GrafVars.rgbOpColor. Static ports without a valid handle
                // use the process-owned per-port fallback.
                ppc_write_port_op_color(memory, current_gworld, color, quickdraw_op_colors);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HiliteColor => {
            if let Some(color) = ppc_read_rgb_color(memory, cpu.gpr[3]) {
                // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62
                // and 4-64: HiliteColor updates the current CGrafPort's
                // GrafVars.rgbHiliteColor. Static ports without a valid
                // handle use the process-owned per-port fallback.
                ppc_write_port_hilite_color(
                    memory,
                    current_gworld,
                    color,
                    quickdraw_hilite_colors,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PmForeColor => {
            // Inside Macintosh Volume VI 1991, p. 20-21: courteous and
            // tolerant entries select their palette RGB, while explicit
            // entries select the corresponding device-table index.
            let entry = cpu.gpr[3] as u16 as i16;
            let assigned_palette = memory
                .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
                .unwrap_or(0);
            let palette_handle = if assigned_palette != 0 {
                assigned_palette
            } else {
                toolbox_startup.application_palette
            };
            if entry >= 0 {
                if let Some((color, explicit)) =
                    ppc_palette_entry_color(memory, palette_handle, entry as u16)
                {
                    // Inside Macintosh Volume VI (1991), p. 20-21:
                    // PmForeColor preserves the palette RGB in the color
                    // port while explicit entries select the corresponding
                    // raw device index for indexed drawing.
                    *quickdraw_fore_color = color;
                    if let Some(allocated_override) = ppc_palette_indexed_override(
                        toolbox_startup,
                        palette_handle,
                        current_gdevice,
                        entry as usize,
                    ) {
                        if let Some(index) = allocated_override {
                            quickdraw_fore_indices.insert(current_gworld, index);
                        } else {
                            quickdraw_fore_indices.remove(&current_gworld);
                        }
                    } else if explicit {
                        quickdraw_fore_indices.insert(current_gworld, entry as u8);
                    } else {
                        quickdraw_fore_indices.remove(&current_gworld);
                    }
                    let _ = ppc_write_port_rgb_color(
                        memory,
                        current_gworld,
                        PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
                        *quickdraw_fore_color,
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PmBackColor => {
            let entry = cpu.gpr[3] as u16 as i16;
            let assigned = memory
                .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
                .unwrap_or(0);
            let palette = if assigned != 0 {
                assigned
            } else {
                toolbox_startup.application_palette
            };
            if entry >= 0 {
                if let Some((color, explicit)) =
                    ppc_palette_entry_color(memory, palette, entry as u16)
                {
                    *quickdraw_back_color = color;
                    if let Some(allocated_override) = ppc_palette_indexed_override(
                        toolbox_startup,
                        palette,
                        current_gdevice,
                        entry as usize,
                    ) {
                        if let Some(index) = allocated_override {
                            toolbox_startup
                                .quickdraw_back_indices
                                .insert(current_gworld, index);
                        } else {
                            toolbox_startup
                                .quickdraw_back_indices
                                .remove(&current_gworld);
                        }
                    } else if explicit {
                        toolbox_startup
                            .quickdraw_back_indices
                            .insert(current_gworld, entry as u8);
                    } else {
                        toolbox_startup
                            .quickdraw_back_indices
                            .remove(&current_gworld);
                    }
                    let _ = ppc_write_port_rgb_color(
                        memory,
                        current_gworld,
                        PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET,
                        color,
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::Color2Index => {
            let pixel = ppc_read_rgb_color(memory, cpu.gpr[3])
                .map(|color| {
                    ppc_color_to_index(memory, current_gdevice, color_manager_clut, color)
                })
                .unwrap_or(0);
            Some(PpcImportAction::Return(pixel))
        }
        PpcImportDispatcherTarget::Index2Color => {
            let color = ppc_index_to_color(memory, current_gdevice, screen_clut, cpu.gpr[3]);
            if cpu.gpr[4] != 0 {
                let _ = ppc_write_rgb_color(memory, cpu.gpr[4], color);
            }
            if ppc_hle_trace_enabled() && matches!(cpu.gpr[3], 0 | 42 | 128 | 245 | 255) {
                eprintln!(
                    "[PPC-TRACE] Index2Color tick={} index={} -> ({:04X},{:04X},{:04X}) out=${:08X}",
                    tick_count,
                    cpu.gpr[3],
                    color.red,
                    color.green,
                    color.blue,
                    cpu.gpr[4]
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RGB2HSL => {
            ppc_rgb2hsl(memory, cpu.gpr[3], cpu.gpr[4]);
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
