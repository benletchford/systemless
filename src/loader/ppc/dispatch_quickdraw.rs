use super::*;

pub(super) struct PpcQuickDrawDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) gworlds: &'a [PpcGWorldRecord],
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
    pub(super) quickdraw_pen_h: &'a mut i16,
    pub(super) quickdraw_pen_v: &'a mut i16,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_quickdraw_import(
    context: PpcQuickDrawDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQuickDrawDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        handles,
        gworlds,
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
        quickdraw_pen_h,
        quickdraw_pen_v,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InitGraf => {
            toolbox_startup.init_graf_count = toolbox_startup.init_graf_count.saturating_add(1);
            toolbox_startup.init_graf_global_ptr = cpu.gpr[3];
            let _ = ppc_init_graf(memory, gworlds, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
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
        PpcImportDispatcherTarget::RGB2HSV => {
            ppc_rgb2hsv(memory, cpu.gpr[3], cpu.gpr[4]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HSV2RGB => {
            ppc_hsv2rgb(memory, cpu.gpr[3], cpu.gpr[4]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetRect => {
            let rect_ptr = cpu.gpr[3];
            let left = cpu.gpr[4] as u16 as i16;
            let top = cpu.gpr[5] as u16 as i16;
            let right = cpu.gpr[6] as u16 as i16;
            let bottom = cpu.gpr[7] as u16 as i16;
            if rect_ptr != 0 && ppc_memory_can_write_bytes(memory, rect_ptr, 8) {
                let _ = ppc_write_rect(memory, rect_ptr, top, left, bottom, right);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SectRect => {
            // Imaging With QuickDraw (1994), 3-55: SectRect writes the
            // non-empty intersection, otherwise the canonical empty Rect.
            let intersection = ppc_read_rect(memory, cpu.gpr[3])
                .zip(ppc_read_rect(memory, cpu.gpr[4]))
                .map(|(left, right)| {
                    (
                        left.0.max(right.0),
                        left.1.max(right.1),
                        left.2.min(right.2),
                        left.3.min(right.3),
                    )
                });
            let intersects =
                intersection.is_some_and(|(top, left, bottom, right)| top < bottom && left < right);
            let output = intersection.filter(|_| intersects).unwrap_or((0, 0, 0, 0));
            if cpu.gpr[5] != 0 {
                let _ = ppc_write_rect(memory, cpu.gpr[5], output.0, output.1, output.2, output.3);
            }
            Some(PpcImportAction::Return(u32::from(intersects)))
        }
        PpcImportDispatcherTarget::UnionRect => {
            let union = ppc_read_rect(memory, cpu.gpr[3])
                .zip(ppc_read_rect(memory, cpu.gpr[4]))
                .map(|(first, second)| {
                    (
                        first.0.min(second.0),
                        first.1.min(second.1),
                        first.2.max(second.2),
                        first.3.max(second.3),
                    )
                });
            if let Some((top, left, bottom, right)) = union {
                // Imaging With QuickDraw (1994), p. 3-55: UnionRect writes
                // the smallest Rect enclosing both inputs and permits either
                // input to alias the destination.
                let _ = ppc_write_rect(memory, cpu.gpr[5], top, left, bottom, right);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::EqualRect => {
            let equal = ppc_read_rect(memory, cpu.gpr[3])
                .zip(ppc_read_rect(memory, cpu.gpr[4]))
                .is_some_and(|(first, second)| first == second);
            Some(PpcImportAction::Return(u32::from(equal)))
        }
        PpcImportDispatcherTarget::EmptyRect => {
            let empty = ppc_read_rect(memory, cpu.gpr[3])
                .is_some_and(|(top, left, bottom, right)| top >= bottom || left >= right);
            Some(PpcImportAction::Return(u32::from(empty)))
        }
        PpcImportDispatcherTarget::SetPt => {
            let point_ptr = cpu.gpr[3];
            let h = cpu.gpr[4] as u16;
            let v = cpu.gpr[5] as u16;
            if point_ptr != 0 && ppc_memory_can_write_bytes(memory, point_ptr, 4) {
                let _ = memory.write_u16_be(point_ptr, v);
                let _ = memory.write_u16_be(point_ptr + 2, h);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::EqualPt => {
            // Imaging With QuickDraw (1994), p. 2-53: EqualPt returns true
            // only when both the vertical and horizontal coordinates match.
            // A Point occupies one PowerPC parameter word.
            Some(PpcImportAction::Return(u32::from(cpu.gpr[3] == cpu.gpr[4])))
        }
        PpcImportDispatcherTarget::AddPt | PpcImportDispatcherTarget::SubPt => {
            let src_v = (cpu.gpr[3] >> 16) as u16 as i16;
            let src_h = cpu.gpr[3] as u16 as i16;
            let dst = cpu.gpr[4];
            if let Some((dst_v, dst_h)) = memory
                .read_u16_be(dst)
                .zip(memory.read_u16_be(dst.wrapping_add(2)))
                .map(|(v, h)| (v as i16, h as i16))
            {
                // Imaging With QuickDraw (1994), pp. 2-52--2-53: AddPt and
                // SubPt update the destination Point component-by-component.
                let subtract =
                    matches!(binding.dispatcher_target, PpcImportDispatcherTarget::SubPt);
                let new_v = if subtract {
                    dst_v.wrapping_sub(src_v)
                } else {
                    dst_v.wrapping_add(src_v)
                };
                let new_h = if subtract {
                    dst_h.wrapping_sub(src_h)
                } else {
                    dst_h.wrapping_add(src_h)
                };
                let _ = memory.write_u16_be(dst, new_v as u16);
                let _ = memory.write_u16_be(dst.wrapping_add(2), new_h as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LocalToGlobal => {
            ppc_transform_port_point(memory, current_gworld, cpu.gpr[3], true);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GlobalToLocal => {
            ppc_transform_port_point(memory, current_gworld, cpu.gpr[3], false);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PtInRect => {
            let v = (cpu.gpr[3] >> 16) as u16 as i16;
            let h = cpu.gpr[3] as u16 as i16;
            let rect = ppc_read_rect(memory, cpu.gpr[4]);
            let inside = rect.is_some_and(|(top, left, bottom, right)| {
                v >= top && v < bottom && h >= left && h < right
            });
            if ppc_pt_in_rect_trace_enabled()
                && (120..=220).contains(&v)
                && (300..=390).contains(&h)
            {
                if let Some((top, left, bottom, right)) = rect {
                    eprintln!(
                        "[PPC-PTINRECT] point=({}, {}) rect_ptr=${:08X} rect=({}, {}, {}, {}) inside={}",
                        v, h, cpu.gpr[4], top, left, bottom, right, inside
                    );
                } else {
                    eprintln!(
                        "[PPC-PTINRECT] point=({}, {}) rect_ptr=${:08X} rect=<invalid> inside=false",
                        v, h, cpu.gpr[4]
                    );
                }
            }
            Some(PpcImportAction::Return(u32::from(inside)))
        }
        PpcImportDispatcherTarget::OffsetRect => {
            let rect_ptr = cpu.gpr[3];
            let dh = cpu.gpr[4] as u16 as i16;
            let dv = cpu.gpr[5] as u16 as i16;
            if rect_ptr != 0 && ppc_memory_can_write_bytes(memory, rect_ptr, 8) {
                let _ = ppc_offset_rect(memory, rect_ptr, dh, dv);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MapRect => {
            ppc_map_rect(memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InsetRect => {
            let rect_ptr = cpu.gpr[3];
            let dh = cpu.gpr[4] as u16 as i16;
            let dv = cpu.gpr[5] as u16 as i16;
            if rect_ptr != 0 && ppc_memory_can_write_bytes(memory, rect_ptr, 8) {
                if let Some((top, left, bottom, right)) = ppc_read_rect(memory, rect_ptr) {
                    let _ = ppc_write_rect(
                        memory,
                        rect_ptr,
                        top.wrapping_add(dv),
                        left.wrapping_add(dh),
                        bottom.wrapping_sub(dv),
                        right.wrapping_sub(dh),
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MoveTo => {
            *quickdraw_pen_h = cpu.gpr[3] as u16 as i16;
            *quickdraw_pen_v = cpu.gpr[4] as u16 as i16;
            if let Some(polygon) = ppc_open_polygon(memory, current_gworld) {
                let _ = ppc_record_polygon_point(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    handles,
                    polygon,
                    *quickdraw_pen_h,
                    *quickdraw_pen_v,
                );
            }
            ppc_sync_gworld_pen(memory, current_gworld, *quickdraw_pen_h, *quickdraw_pen_v);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::Move => {
            *quickdraw_pen_h = quickdraw_pen_h.wrapping_add(cpu.gpr[3] as u16 as i16);
            *quickdraw_pen_v = quickdraw_pen_v.wrapping_add(cpu.gpr[4] as u16 as i16);
            if let Some(polygon) = ppc_open_polygon(memory, current_gworld) {
                let _ = ppc_record_polygon_point(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    handles,
                    polygon,
                    *quickdraw_pen_h,
                    *quickdraw_pen_v,
                );
            }
            ppc_sync_gworld_pen(memory, current_gworld, *quickdraw_pen_h, *quickdraw_pen_v);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LineTo | PpcImportDispatcherTarget::Line => {
            let relative = matches!(binding.dispatcher_target, PpcImportDispatcherTarget::Line);
            let new_h = if relative {
                quickdraw_pen_h.wrapping_add(cpu.gpr[3] as u16 as i16)
            } else {
                cpu.gpr[3] as u16 as i16
            };
            let new_v = if relative {
                quickdraw_pen_v.wrapping_add(cpu.gpr[4] as u16 as i16)
            } else {
                cpu.gpr[4] as u16 as i16
            };
            if toolbox_startup.open_region_port == current_gworld {
                ppc_open_region_include_point(toolbox_startup, *quickdraw_pen_h, *quickdraw_pen_v);
                ppc_open_region_include_point(toolbox_startup, new_h, new_v);
            } else if let Some(polygon) = ppc_open_polygon(memory, current_gworld) {
                let _ = ppc_record_polygon_point(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    handles,
                    polygon,
                    *quickdraw_pen_h,
                    *quickdraw_pen_v,
                );
                let _ = ppc_record_polygon_point(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    handles,
                    polygon,
                    new_h,
                    new_v,
                );
            } else {
                let _ = ppc_line_to(
                    memory,
                    gworlds,
                    current_gworld,
                    (*quickdraw_pen_h, *quickdraw_pen_v),
                    (new_h, new_v),
                    *quickdraw_fore_color,
                    quickdraw_fore_indices.get(&current_gworld).copied(),
                );
            }
            *quickdraw_pen_h = new_h;
            *quickdraw_pen_v = new_v;
            ppc_sync_gworld_pen(memory, current_gworld, new_h, new_v);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PaintRect => {
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] PaintRect tick={} port=${:08X} rect=${:08X} bounds={:?} fore=({:04X},{:04X},{:04X})",
                    tick_count,
                    current_gworld,
                    cpu.gpr[3],
                    ppc_read_rect(memory, cpu.gpr[3]),
                    quickdraw_fore_color.red,
                    quickdraw_fore_color.green,
                    quickdraw_fore_color.blue,
                );
            }
            if let (Some(commands), Some(rect)) = (
                ppc_open_picture_commands(toolbox_startup, current_gworld),
                ppc_read_rect(memory, cpu.gpr[3]),
            ) {
                pict::recording_push_rect(commands, 0x0031, rect);
            } else {
                let _ = ppc_paint_rect(
                    cpu,
                    memory,
                    gworlds,
                    current_gworld,
                    *quickdraw_fore_color,
                    quickdraw_fore_indices.get(&current_gworld).copied(),
                    *quickdraw_back_color,
                    &toolbox_startup.quickdraw_pen_pattern,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::EraseRect => {
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] EraseRect tick={} port=${:08X} rect=${:08X} bounds={:?} back=({:04X},{:04X},{:04X})",
                    tick_count,
                    current_gworld,
                    cpu.gpr[3],
                    ppc_read_rect(memory, cpu.gpr[3]),
                    quickdraw_back_color.red,
                    quickdraw_back_color.green,
                    quickdraw_back_color.blue,
                );
            }
            if let Some(rect) = ppc_read_rect(memory, cpu.gpr[3]) {
                let _ = ppc_paint_rect_bounds(
                    memory,
                    gworlds,
                    current_gworld,
                    rect,
                    *quickdraw_back_color,
                    None,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InvertRect => {
            let _ = ppc_invert_rect(cpu, memory, gworlds, current_gworld);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FrameRect => {
            if toolbox_startup.open_region_port == current_gworld {
                ppc_open_region_include_rect(toolbox_startup, memory, cpu.gpr[3]);
            } else {
                let _ = ppc_frame_rect(
                    cpu,
                    memory,
                    gworlds,
                    current_gworld,
                    *quickdraw_fore_color,
                    quickdraw_fore_indices.get(&current_gworld).copied(),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FillRect | PpcImportDispatcherTarget::FillCRect => {
            if let Some(rect) = ppc_read_rect(memory, cpu.gpr[3]) {
                let _ = ppc_paint_rect_bounds(
                    memory,
                    gworlds,
                    current_gworld,
                    rect,
                    *quickdraw_fore_color,
                    quickdraw_fore_indices.get(&current_gworld).copied(),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetPen => {
            if cpu.gpr[3] != 0 {
                let _ = memory.write_u16_be(cpu.gpr[3], *quickdraw_pen_v as u16);
                let _ = memory.write_u16_be(cpu.gpr[3] + 2, *quickdraw_pen_h as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HidePen | PpcImportDispatcherTarget::ShowPen => {
            let address = current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_VIS_OFFSET);
            let visibility = memory.read_u16_be(address).unwrap_or(0) as i16;
            let visibility = if matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::HidePen
            ) {
                visibility.saturating_sub(1)
            } else {
                visibility.saturating_add(1).min(0)
            };
            let _ = memory.write_u16_be(address, visibility as u16);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PenSize => {
            ppc_set_pen_size(cpu, memory, current_gworld);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PenMode => {
            if ppc_memory_can_write_bytes(
                memory,
                current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_MODE_OFFSET),
                2,
            ) {
                let _ = memory.write_u16_be(
                    current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_MODE_OFFSET),
                    cpu.gpr[3] as u16,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PenNormal => {
            ppc_set_pen_normal(memory, current_gworld);
            toolbox_startup.quickdraw_pen_pattern = [0xff; 8];
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PenPixPat => {
            // Imaging With QuickDraw (1994), 4-67 through 4-68: the PixPat
            // handle is installed directly into the color port's pnPixPat.
            if cpu.gpr[3] != 0 && current_gworld != 0 {
                let _ = memory.write_u32_be(current_gworld.wrapping_add(58), cpu.gpr[3]);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetPenState => {
            ppc_get_pen_state(memory, current_gworld, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetPenState => {
            ppc_set_pen_state(memory, current_gworld, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
