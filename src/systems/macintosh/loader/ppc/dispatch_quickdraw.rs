use super::*;
use crate::trap::{pict, TrapDispatcher};

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
                if let Some(commands) = ppc_open_picture_commands(toolbox_startup, current_gworld) {
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
                ppc_write_port_hilite_color(memory, current_gworld, color, quickdraw_hilite_colors);
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
                .map(|color| ppc_color_to_index(memory, current_gdevice, color_manager_clut, color))
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
        PpcImportDispatcherTarget::MapPt => {
            ppc_map_pt(memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]);
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
        PpcImportDispatcherTarget::MovePortTo => {
            // MovePortTo ($A877)
            // Changes the current port's position relative to its boundary rectangle.
            // PROCEDURE MovePortTo (leftGlobal,topGlobal: INTEGER);
            // Inside Macintosh: Imaging With QuickDraw (1994), pp. 2-46--2-47.
            let port_rect_ptr = current_gworld.wrapping_add(16);
            let Some((port_top, port_left, _, _)) = ppc_read_rect(memory, port_rect_ptr) else {
                return Some(PpcImportAction::ReturnPreserve);
            };
            let is_color = memory
                .read_u16_be(current_gworld.wrapping_add(6))
                .is_some_and(|version| version & 0xc000 != 0);
            let bits_rect_ptr = if is_color {
                let pixmap_handle = memory
                    .read_u32_be(current_gworld.wrapping_add(2))
                    .unwrap_or(0);
                if pixmap_handle == 0 {
                    return Some(PpcImportAction::ReturnPreserve);
                }
                let pixmap = memory.read_u32_be(pixmap_handle).unwrap_or(0);
                if pixmap == 0 {
                    return Some(PpcImportAction::ReturnPreserve);
                }
                pixmap.wrapping_add(6)
            } else {
                current_gworld.wrapping_add(8)
            };
            if let Some((bits_top, bits_left, bits_bottom, bits_right)) =
                ppc_read_rect(memory, bits_rect_ptr)
            {
                let new_top = port_top.wrapping_sub(cpu.gpr[4] as u16 as i16);
                let new_left = port_left.wrapping_sub(cpu.gpr[3] as u16 as i16);
                let _ = ppc_write_rect(
                    memory,
                    bits_rect_ptr,
                    new_top,
                    new_left,
                    new_top.wrapping_add(bits_bottom.wrapping_sub(bits_top)),
                    new_left.wrapping_add(bits_right.wrapping_sub(bits_left)),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PortSize => {
            // PortSize ($A876)
            // Resizes the current port rectangle without moving its upper-left corner.
            // PROCEDURE PortSize (width,height: INTEGER);
            // Inside Macintosh: Imaging With QuickDraw (1994), p. 2-46.
            let port_rect_ptr = current_gworld.wrapping_add(16);
            if let Some((top, left, _, _)) = ppc_read_rect(memory, port_rect_ptr) {
                let width = cpu.gpr[3] as u16 as i16;
                let height = cpu.gpr[4] as u16 as i16;
                let _ = ppc_write_rect(
                    memory,
                    port_rect_ptr,
                    top,
                    left,
                    top.wrapping_add(height),
                    left.wrapping_add(width),
                );
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
        PpcImportDispatcherTarget::FrameOval
        | PpcImportDispatcherTarget::PaintOval
        | PpcImportDispatcherTarget::EraseOval => {
            if toolbox_startup.open_region_port == current_gworld {
                if binding.dispatcher_target == PpcImportDispatcherTarget::FrameOval {
                    if let Some((top, left, bottom, right)) = ppc_read_rect(memory, cpu.gpr[3]) {
                        let rows = TrapDispatcher::compute_oval_spans(
                            right.saturating_sub(left),
                            bottom.saturating_sub(top),
                        )
                        .into_iter()
                        .map(|(row_left, row_right)| {
                            vec![
                                left.saturating_add(row_left),
                                left.saturating_add(row_right),
                            ]
                        })
                        .collect();
                        ppc_open_region_include_rows(toolbox_startup, top, rows);
                    }
                } else {
                    ppc_open_region_include_rect(toolbox_startup, memory, cpu.gpr[3]);
                }
            } else {
                let color = if matches!(
                    binding.dispatcher_target,
                    PpcImportDispatcherTarget::EraseOval
                ) {
                    *quickdraw_back_color
                } else {
                    *quickdraw_fore_color
                };
                let _ = ppc_draw_oval(
                    memory,
                    gworlds,
                    current_gworld,
                    cpu.gpr[3],
                    color,
                    (!matches!(
                        binding.dispatcher_target,
                        PpcImportDispatcherTarget::EraseOval
                    ))
                    .then(|| quickdraw_fore_indices.get(&current_gworld).copied())
                    .flatten(),
                    matches!(
                        binding.dispatcher_target,
                        PpcImportDispatcherTarget::FrameOval
                    ),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PaintArc => {
            if toolbox_startup.open_region_port == current_gworld {
                ppc_open_region_include_rect(toolbox_startup, memory, cpu.gpr[3]);
            } else {
                let _ = ppc_paint_arc(
                    memory,
                    gworlds,
                    current_gworld,
                    cpu.gpr[3],
                    cpu.gpr[4] as u16 as i16,
                    cpu.gpr[5] as u16 as i16,
                    *quickdraw_fore_color,
                    quickdraw_fore_indices.get(&current_gworld).copied(),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FrameRoundRect | PpcImportDispatcherTarget::PaintRoundRect => {
            let paint = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::PaintRoundRect
            );
            if let (Some(commands), Some(rect)) = (
                ppc_open_picture_commands(toolbox_startup, current_gworld),
                ppc_read_rect(memory, cpu.gpr[3]),
            ) {
                pict::recording_push_round_rect(
                    commands,
                    if paint { 0x0041 } else { 0x0040 },
                    rect,
                    cpu.gpr[4] as u16 as i16,
                    cpu.gpr[5] as u16 as i16,
                );
            } else if toolbox_startup.open_region_port == current_gworld {
                if let Some((top, left, bottom, right)) = ppc_read_rect(memory, cpu.gpr[3]) {
                    let rect = Rect {
                        top,
                        left,
                        bottom,
                        right,
                    };
                    let rows = TrapDispatcher::compute_rrect_spans(
                        &rect,
                        cpu.gpr[4] as u16 as i16,
                        cpu.gpr[5] as u16 as i16,
                    )
                    .into_iter()
                    .map(|(row_left, row_right)| vec![row_left, row_right])
                    .collect();
                    ppc_open_region_include_rows(toolbox_startup, top, rows);
                }
            } else {
                let explicit_index = quickdraw_fore_indices.get(&current_gworld).copied();
                if paint {
                    let _ = ppc_paint_round_rect(
                        cpu,
                        memory,
                        gworlds,
                        current_gworld,
                        *quickdraw_fore_color,
                        explicit_index,
                    );
                } else {
                    let _ = ppc_frame_round_rect(
                        cpu,
                        memory,
                        gworlds,
                        current_gworld,
                        *quickdraw_fore_color,
                        explicit_index,
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FrameRgn => {
            if toolbox_startup.open_region_port == current_gworld {
                ppc_open_region_include_region(toolbox_startup, memory, cpu.gpr[3]);
            } else {
                let _ = ppc_frame_region(
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
        PpcImportDispatcherTarget::PaintRgn | PpcImportDispatcherTarget::FillRgn => {
            if toolbox_startup.open_region_port == current_gworld {
                ppc_open_region_include_region(toolbox_startup, memory, cpu.gpr[3]);
            } else {
                let _ = ppc_paint_region(
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
        PpcImportDispatcherTarget::InvertRgn => {
            let _ = ppc_invert_region(memory, gworlds, current_gworld, cpu.gpr[3]);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcQuickDrawCompatibilityOperation {
    AnimateEntry,
    AnimatePalette,
    BackPat,
    BackPixPat,
    ClosePicture,
    CopyDeepMask,
    CopyMask,
    CopyPalette,
    Ctab2Palette,
    DisposeGDevice,
    DisposePalette,
    DisposePixPat,
    Exp1To3,
    Exp1To6,
    GetCPixel,
    GetEntryColor,
    GetEntryUsage,
    GetItemIcon,
    GetItemStyle,
    GetNewPalette,
    NewGDevice,
    NewPalette,
    NewPixPat,
    OpenPicture,
    Palette2Ctab,
    PenPat,
    PlotIcon,
    ScrollRect,
    SetCPixel,
    SetEntryColor,
    SetEntryUsage,
    SetItemIcon,
    SetItemStyle,
    SetStdCProcs,
    SetStdProcs,
}

pub(super) fn ppc_quickdraw_background_pattern(
    memory: &mut PpcSectionMem,
    current_gworld: u32,
    fallback: [u8; 8],
) -> [u8; 8] {
    // A color port's bkPixPat field is at the same offset as documented by
    // Color QuickDraw. Its pat1Data member remains the required monochrome
    // fallback for one-bit patterns and for destinations without a usable
    // pixel map (Inside Macintosh: Imaging With QuickDraw, pp. 4-80--4-82).
    let Some(pattern_handle) = memory
        .read_u32_be(current_gworld.wrapping_add(32))
        .filter(|handle| *handle != 0)
    else {
        return fallback;
    };
    let Some(pattern_ptr) = memory.read_u32_be(pattern_handle).filter(|ptr| *ptr != 0) else {
        return fallback;
    };
    let mut pattern = [0u8; 8];
    memory
        .read_bytes_into(pattern_ptr.wrapping_add(20), &mut pattern)
        .is_some()
        .then_some(pattern)
        .unwrap_or(fallback)
}

pub(super) fn ppc_quickdraw_surface_background_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    local_point: (i32, i32),
    fore_color: PpcRgbColor,
    back_color: PpcRgbColor,
    explicit_back_index: Option<u8>,
    pattern: [u8; 8],
) -> Option<u16> {
    let (x, y) = local_point;
    let global_x = i32::from(surface.left).checked_add(x)?;
    let global_y = i32::from(surface.top).checked_add(y)?;
    let row = pattern[global_y.rem_euclid(8) as usize];
    let is_foreground = row & (0x80 >> (global_x.rem_euclid(8) as u8)) != 0;
    if is_foreground {
        ppc_quickdraw_surface_fore_pixel(memory, surface, fore_color, None)
    } else {
        ppc_quickdraw_surface_fore_pixel(memory, surface, back_color, explicit_back_index)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_dispatch_quickdraw_compatibility(
    operation: PpcQuickDrawCompatibilityOperation,
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
    fore_color: PpcRgbColor,
    fore_index: Option<u8>,
    back_color: PpcRgbColor,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> PpcImportAction {
    match operation {
        PpcQuickDrawCompatibilityOperation::SetCPixel => {
            let color = ppc_read_rgb_color(memory, cpu.gpr[5]);
            let surface = ppc_live_quickdraw_surface(memory, gworlds, current_gworld);
            if let (Some(color), Some(surface)) = (color, surface) {
                let point = surface.local_point((
                    cpu.gpr[3] as u16 as i16 as i32,
                    cpu.gpr[4] as u16 as i16 as i32,
                ));
                if let Some(pixel) = ppc_quickdraw_surface_color_pixel(memory, surface, color) {
                    let _ =
                        ppc_quickdraw_write_raw_pixel(memory, surface.front_buffer, point, pixel);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::GetCPixel => {
            let color = ppc_live_quickdraw_surface(memory, gworlds, current_gworld)
                .and_then(|surface| {
                    let front = surface.front_buffer;
                    ppc_quickdraw_read_pixel(
                        memory,
                        front,
                        surface.local_point((
                            cpu.gpr[3] as u16 as i16 as i32,
                            cpu.gpr[4] as u16 as i16 as i32,
                        )),
                    )
                    .map(|pixel| match front.depth {
                        depth @ (1 | 2 | 4 | 8) => {
                            let fallback = TrapDispatcher::standard_mac_indexed_clut(depth as u16)
                                .map(|(clut, _)| clut)
                                .unwrap_or_else(TrapDispatcher::standard_mac_8bpp_clut);
                            let clut = surface
                                .ctable_handle
                                .and_then(|handle| ppc_read_ctable_clut(memory, handle, &fallback))
                                .unwrap_or(fallback);
                            let [red, green, blue] = clut[usize::from(pixel as u8)];
                            PpcRgbColor { red, green, blue }
                        }
                        16 => {
                            let [red, green, blue] = ppc_rgb555_to_rgb16(pixel);
                            PpcRgbColor { red, green, blue }
                        }
                        _ => PPC_RGB_BLACK,
                    })
                })
                .unwrap_or(PPC_RGB_BLACK);
            let _ = ppc_write_rgb_color(memory, cpu.gpr[5], color);
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::GetItemIcon => {
            // GetItemIcon reads the one-byte icon number stored after the
            // item's Pascal text. Macintosh Toolbox Essentials (1992),
            // pp. 3-132--3-133.
            let icon =
                ppc_menu_item_attribute_address(memory, cpu.gpr[3], cpu.gpr[4] as u16 as i16, 0)
                    .and_then(|address| memory.read_u8(address))
                    .unwrap_or(0);
            let _ = memory.write_u8(cpu.gpr[5], icon);
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::SetItemIcon => {
            // SetItemIcon stores only the icon-number byte; the standard
            // MDEF resolves resource ID icon+256 when it draws the item.
            // Macintosh Toolbox Essentials (1992), pp. 3-137--3-138.
            ppc_mutate_menu_items_in_place(memory, handles, cpu.gpr[3], |items| {
                items.set_icon(cpu.gpr[4] as u16 as i16, cpu.gpr[5] as u8)
            });
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::GetItemStyle => {
            // GetItemStyle returns the item's QuickDraw Style byte.
            // Macintosh Toolbox Essentials (1992), pp. 3-132--3-133.
            let style =
                ppc_menu_item_attribute_address(memory, cpu.gpr[3], cpu.gpr[4] as u16 as i16, 3)
                    .and_then(|address| memory.read_u8(address))
                    .unwrap_or(0);
            let _ = memory.write_u8(cpu.gpr[5], style);
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::SetItemStyle => {
            // SetItemStyle replaces the one-byte QuickDraw face bitset used
            // by the standard MDEF. Macintosh Toolbox Essentials (1992),
            // pp. 3-133--3-134.
            ppc_mutate_menu_items_in_place(memory, handles, cpu.gpr[3], |items| {
                items.set_style(cpu.gpr[4] as u16 as i16, cpu.gpr[5] as u8)
            });
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::OpenPicture => {
            let rect = ppc_read_rect(memory, cpu.gpr[3]).unwrap_or((0, 0, 1, 1));
            let mut bytes = Vec::from(minimal_pict_bytes());
            bytes[2..4].copy_from_slice(&rect.0.to_be_bytes());
            bytes[4..6].copy_from_slice(&rect.1.to_be_bytes());
            bytes[6..8].copy_from_slice(&rect.2.to_be_bytes());
            bytes[8..10].copy_from_slice(&rect.3.to_be_bytes());
            let handle = ppc_process_alloc_handle_with_bytes(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                &bytes,
            );
            *last_mem_error = if handle == 0 {
                PPC_MEM_FULL_ERR
            } else {
                toolbox_startup.open_picture = Some((handle, current_gworld, rect, Vec::new()));
                PPC_NO_ERR
            };
            PpcImportAction::Return(handle)
        }
        PpcQuickDrawCompatibilityOperation::ClosePicture => {
            if let Some((handle, _, frame, commands)) = toolbox_startup.open_picture.take() {
                let picture = pict::finish_recording(frame, commands);
                let size = u32::try_from(picture.len()).unwrap_or(u32::MAX);
                let result = process_memory_manager.set_native_handle_size(memory, handle, size);
                ppc_apply_process_native_resource_handle(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    handles,
                    handle,
                );
                if result == PPC_NO_ERR {
                    if let Some(ptr) = memory.read_u32_be(handle) {
                        *last_mem_error = if memory.write_bytes(ptr, &picture).is_some() {
                            PPC_NO_ERR
                        } else {
                            PPC_PARAM_ERR
                        };
                    }
                } else {
                    *last_mem_error = result;
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::PlotIcon => {
            let rect = ppc_read_rect(memory, cpu.gpr[3]);
            let icon = ppc_handle_bytes(memory, handles, cpu.gpr[4]);
            let surface = ppc_live_quickdraw_surface(memory, gworlds, current_gworld);
            if let (Some(rect), Some(icon), Some(surface)) = (rect, icon, surface) {
                let front = surface.front_buffer;
                let rect = surface.local_rect(rect);
                let width = (rect.3 - rect.1).max(0);
                let height = (rect.2 - rect.0).max(0);
                let fore_pixel =
                    ppc_quickdraw_surface_fore_pixel(memory, surface, fore_color, fore_index);
                if let (true, Some(fore_pixel)) =
                    (icon.len() >= 128 && width > 0 && height > 0, fore_pixel)
                {
                    for y in 0..height {
                        for x in 0..width {
                            let sx = (x * 32 / width).clamp(0, 31) as usize;
                            let sy = (y * 32 / height).clamp(0, 31) as usize;
                            let bit = icon[sy * 4 + sx / 8] & (0x80 >> (sx & 7)) != 0;
                            if bit {
                                let _ = ppc_quickdraw_write_raw_pixel(
                                    memory,
                                    front,
                                    (rect.1 + x, rect.0 + y),
                                    fore_pixel,
                                );
                            }
                        }
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::CopyPalette => {
            let src_palette = cpu.gpr[3];
            let dst_palette = cpu.gpr[4];
            let src_entry = cpu.gpr[5] as u16 as i16;
            let dst_entry = cpu.gpr[6] as u16 as i16;
            let dst_length = cpu.gpr[7] as u16 as i16;
            if src_palette == 0
                || dst_palette == 0
                || src_entry < 0
                || dst_entry < 0
                || dst_length <= 0
            {
                return PpcImportAction::ReturnPreserve;
            }

            let Some(src_ptr) = memory.read_u32_be(src_palette).filter(|ptr| *ptr != 0) else {
                return PpcImportAction::ReturnPreserve;
            };
            let src_count = u32::from(memory.read_u16_be(src_ptr).unwrap_or(0));
            let src_entry = u32::from(src_entry as u16);
            let dst_entry = u32::from(dst_entry as u16);
            let dst_length = u32::from(dst_length as u16);
            let mut entries = Vec::new();
            for offset in 0..dst_length {
                let source = src_entry.saturating_add(offset);
                if source >= src_count {
                    break;
                }
                let Some(info) = ppc_memory_read_bytes(memory, src_ptr + 16 + source * 16, 10)
                else {
                    break;
                };
                entries.push(info);
            }

            let required_entries = dst_entry.saturating_add(dst_length);
            let Some(mut dst_ptr) = memory.read_u32_be(dst_palette).filter(|ptr| *ptr != 0) else {
                return PpcImportAction::ReturnPreserve;
            };
            let dst_count = u32::from(memory.read_u16_be(dst_ptr).unwrap_or(0));
            if required_entries > dst_count && required_entries <= i16::MAX as u32 {
                let size = 16u32.saturating_add(required_entries.saturating_mul(16));
                let mut allocator = PpcProcessAllocatorView {
                    memory_manager: process_memory_manager,
                };
                let result = allocator.resize_handle(
                    memory,
                    heap_cursor,
                    last_mem_error,
                    handles,
                    dst_palette,
                    size,
                );
                *last_mem_error = result;
                if result != PPC_NO_ERR {
                    return PpcImportAction::ReturnPreserve;
                }
                let Some(resized_ptr) = memory.read_u32_be(dst_palette).filter(|ptr| *ptr != 0)
                else {
                    return PpcImportAction::ReturnPreserve;
                };
                dst_ptr = resized_ptr;
                let _ = memory.write_u16_be(dst_ptr, required_entries as u16);
            }

            let dst_count = u32::from(memory.read_u16_be(dst_ptr).unwrap_or(0));
            for (offset, info) in entries.into_iter().enumerate() {
                let destination = dst_entry.saturating_add(offset as u32);
                if destination >= dst_count {
                    break;
                }
                let _ = memory.write_bytes(dst_ptr + 16 + destination * 16, &info);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::DisposePixPat => {
            // DisposePixPat releases the PixPat's owned handles, including
            // the PixMap's color table. Imaging With QuickDraw (1994), p. 4-91.
            let pat = cpu.gpr[3];
            if let Some(record) = (pat != 0)
                .then(|| memory.read_u32_be(pat))
                .flatten()
                .filter(|ptr| *ptr != 0)
            {
                let pat_map = memory.read_u32_be(record + 2).unwrap_or(0);
                for offset in [6, 10, 16] {
                    let nested = memory.read_u32_be(record + offset).unwrap_or(0);
                    if nested != 0 {
                        let _ = ppc_dispose_process_native_handle(
                            process_memory_manager,
                            memory,
                            heap_cursor,
                            heap_limit,
                            last_mem_error,
                            handles,
                            nested,
                        );
                    }
                }
                ppc_dispose_pixmap(
                    pat_map,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    &mut toolbox_startup.indexed_screen_ctables,
                );
                let _ = ppc_dispose_process_native_handle(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    pat,
                );
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::NewPixPat => {
            // NewPixPat creates a PixPat with a device-matched PixMap,
            // three initially empty data handles, and a gray pat1Data.
            // Imaging With QuickDraw (1994), pp. 4-88--4-89.
            let pat_map = ppc_new_pixmap(
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                current_gdevice,
            );
            if pat_map == 0 {
                return PpcImportAction::Return(0);
            }
            let pat_data = ppc_process_alloc_handle(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                0,
                true,
            );
            let pat_xdata = ppc_process_alloc_handle(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                0,
                true,
            );
            let pat_xmap = ppc_process_alloc_handle(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                0,
                true,
            );
            let pat = ppc_process_alloc_handle(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                28,
                true,
            );
            if [pat_data, pat_xdata, pat_xmap, pat].contains(&0) {
                for nested in [pat_data, pat_xdata, pat_xmap, pat] {
                    if nested != 0 {
                        let _ = ppc_dispose_process_native_handle(
                            process_memory_manager,
                            memory,
                            heap_cursor,
                            heap_limit,
                            last_mem_error,
                            handles,
                            nested,
                        );
                    }
                }
                ppc_dispose_pixmap(
                    pat_map,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    &mut toolbox_startup.indexed_screen_ctables,
                );
                *last_mem_error = PPC_MEM_FULL_ERR;
                return PpcImportAction::Return(0);
            }
            let Some(record) = memory.read_u32_be(pat) else {
                *last_mem_error = PPC_PARAM_ERR;
                return PpcImportAction::Return(0);
            };
            let _ = memory.write_u16_be(record, 1);
            let _ = memory.write_u32_be(record + 2, pat_map);
            let _ = memory.write_u32_be(record + 6, pat_data);
            let _ = memory.write_u32_be(record + 10, pat_xdata);
            let _ = memory.write_u32_be(record + 16, pat_xmap);
            let _ = memory.write_bytes(
                record + 20,
                &[0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
            );
            *last_mem_error = PPC_NO_ERR;
            PpcImportAction::Return(pat)
        }
        PpcQuickDrawCompatibilityOperation::NewPalette => {
            let entry_count = u32::from(cpu.gpr[3] as u16);
            let byte_count = 16u32.saturating_add(entry_count.saturating_mul(16));
            let handle = ppc_process_alloc_handle(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                byte_count,
                true,
            );
            *last_mem_error = if handle == 0 {
                PPC_MEM_FULL_ERR
            } else {
                PPC_NO_ERR
            };
            if let Some(palette_ptr) = (handle != 0)
                .then(|| memory.read_u32_be(handle))
                .flatten()
                .filter(|ptr| *ptr != 0)
            {
                let _ = memory.write_u16_be(palette_ptr, entry_count as u16);
                let src_ctab = memory.read_u32_be(cpu.gpr[4]).filter(|ptr| *ptr != 0);
                let src_entries = src_ctab
                    .and_then(|ptr| memory.read_u16_be(ptr + 6))
                    .map_or(0, |last| u32::from(last) + 1);
                for entry in 0..entry_count {
                    let info_ptr = palette_ptr + 16 + entry * 16;
                    if let Some(ctab_ptr) = src_ctab.filter(|_| entry < src_entries) {
                        let spec_ptr = ctab_ptr + 8 + entry * 8;
                        for offset in [0, 2, 4] {
                            if let Some(component) = memory.read_u16_be(spec_ptr + 2 + offset) {
                                let _ = memory.write_u16_be(info_ptr + offset, component);
                            }
                        }
                    }
                    let _ = memory.write_u16_be(info_ptr + 6, cpu.gpr[5] as u16);
                    let _ = memory.write_u16_be(info_ptr + 8, cpu.gpr[6] as u16);
                }
            }
            PpcImportAction::Return(handle)
        }
        PpcQuickDrawCompatibilityOperation::NewGDevice
        | PpcQuickDrawCompatibilityOperation::DisposeGDevice => {
            unreachable!("graphics-device imports return through dispatch_graphics_device_import")
        }
        PpcQuickDrawCompatibilityOperation::DisposePalette => {
            // Inside Macintosh Volume VI 1991, p. 20-24: DisposePalette
            // releases the relocatable Palette record and its master pointer.
            ppc_release_palette_allocations_and_restore(
                memory,
                toolbox_startup,
                cpu.gpr[3],
                current_gdevice,
                screen_clut,
                color_manager_clut,
            );
            if toolbox_startup.application_palette == cpu.gpr[3] {
                toolbox_startup.application_palette = 0;
                toolbox_startup.application_palette_updates = 0;
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
                cpu.gpr[3],
            );
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::Palette2Ctab => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_palette_to_ctab(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                toolbox_startup,
                cpu.gpr[3],
                cpu.gpr[4],
            );
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::Ctab2Palette => {
            let ctable_handle = cpu.gpr[3];
            let palette_handle = cpu.gpr[4];
            let usage = cpu.gpr[5] as u16;
            let tolerance = cpu.gpr[6] as u16;
            let Some(ctable_ptr) = memory.read_u32_be(ctable_handle).filter(|ptr| *ptr != 0) else {
                return PpcImportAction::ReturnPreserve;
            };
            if palette_handle == 0 || memory.read_u32_be(palette_handle).unwrap_or(0) == 0 {
                return PpcImportAction::ReturnPreserve;
            }
            // Inside Macintosh Volume VI (1991), p. 20-24: replacing a
            // palette from a color table first relinquishes every animated
            // device index allocated to that palette.
            ppc_release_palette_allocations_and_restore(
                memory,
                toolbox_startup,
                palette_handle,
                current_gdevice,
                screen_clut,
                color_manager_clut,
            );
            let entry_count = memory
                .read_u16_be(ctable_ptr + 6)
                .map_or(0, |last| u32::from(last) + 1)
                .min(256);
            let size = 16u32.saturating_add(entry_count.saturating_mul(16));
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = allocator.resize_handle(
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                palette_handle,
                size,
            );
            *last_mem_error = result;
            if result != PPC_NO_ERR {
                return PpcImportAction::ReturnPreserve;
            }
            let Some(palette_ptr) = memory.read_u32_be(palette_handle).filter(|ptr| *ptr != 0)
            else {
                return PpcImportAction::ReturnPreserve;
            };
            let _ = memory.write_u16_be(palette_ptr, entry_count as u16);
            for entry in 0..entry_count {
                let spec_ptr = ctable_ptr + 8 + entry * 8;
                let info_ptr = palette_ptr + 16 + entry * 16;
                for offset in [0, 2, 4] {
                    if let Some(component) = memory.read_u16_be(spec_ptr + 2 + offset) {
                        let _ = memory.write_u16_be(info_ptr + offset, component);
                    }
                }
                let _ = memory.write_u16_be(info_ptr + 6, usage);
                let _ = memory.write_u16_be(info_ptr + 8, tolerance);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::GetNewPalette => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let palette = ppc_copy_palette_resource(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                current_resource_refnum,
                cpu.gpr[3] as u16 as i16,
            );
            if palette != 0 && current_gworld != 0 {
                let _ = memory.write_u32_be(
                    current_gworld + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                    palette,
                );
                let _ =
                    memory.write_u16_be(current_gworld + PPC_CGRAF_PORT_PALETTE_UPDATES_OFFSET, 1);
            }
            PpcImportAction::Return(palette)
        }
        PpcQuickDrawCompatibilityOperation::ScrollRect => {
            let rect = ppc_read_rect(memory, cpu.gpr[3]);
            let surface = ppc_live_quickdraw_surface(memory, gworlds, current_gworld);
            if let (Some(port_rect), Some(surface)) = (rect, surface) {
                let front = surface.front_buffer;
                if !matches!(front.depth, 1 | 2 | 4 | 8 | 16) {
                    return PpcImportAction::ReturnPreserve;
                }
                let requested_rect = surface.local_rect(port_rect);
                // ScrollRect operates on the intersection of the requested
                // rectangle and the live PixMap. Keep both the source
                // snapshot and every destination write inside that
                // intersection; otherwise a positive delta can overwrite
                // pixels outside r.
                let rect = (
                    requested_rect.0.clamp(0, front.height as i32),
                    requested_rect.1.clamp(0, front.width as i32),
                    requested_rect.2.clamp(0, front.height as i32),
                    requested_rect.3.clamp(0, front.width as i32),
                );
                let dh = cpu.gpr[4] as u16 as i16 as i32;
                let dv = cpu.gpr[5] as u16 as i16 as i32;
                if rect.0 < rect.2 && rect.1 < rect.3 {
                    let width = (rect.3 - rect.1) as usize;
                    let height = (rect.2 - rect.0) as usize;
                    let background_pattern = ppc_quickdraw_background_pattern(
                        memory,
                        current_gworld,
                        toolbox_startup.quickdraw_back_pattern,
                    );
                    let mut pixels = vec![0; width * height];
                    for y in rect.0..rect.2 {
                        for x in rect.1..rect.3 {
                            if let Some(pixel) = ppc_quickdraw_read_pixel(memory, front, (x, y)) {
                                pixels[((y - rect.0) as usize) * width + (x - rect.1) as usize] =
                                    pixel;
                            }
                        }
                    }
                    // Read all pixels before writing any destination pixel so
                    // overlapping scrolls use the original raster. Exposed
                    // cells retain the port's background color/pattern pixel.
                    for y in rect.0..rect.2 {
                        for x in rect.1..rect.3 {
                            let src_x = x - dh;
                            let src_y = y - dv;
                            let pixel = if src_x >= rect.1
                                && src_x < rect.3
                                && src_y >= rect.0
                                && src_y < rect.2
                            {
                                pixels[((src_y - rect.0) as usize) * width
                                    + (src_x - rect.1) as usize]
                            } else {
                                ppc_quickdraw_surface_background_pixel(
                                    memory,
                                    surface,
                                    (x, y),
                                    fore_color,
                                    back_color,
                                    toolbox_startup
                                        .quickdraw_back_indices
                                        .get(&current_gworld)
                                        .copied(),
                                    background_pattern,
                                )
                                .unwrap_or(0)
                            };
                            let _ = ppc_quickdraw_write_raw_pixel(memory, front, (x, y), pixel);
                        }
                    }
                }
                if cpu.gpr[6] != 0 {
                    let top = ppc_i32_to_i16_saturating(i32::from(surface.top) + rect.0);
                    let left = ppc_i32_to_i16_saturating(i32::from(surface.left) + rect.1);
                    let bottom = ppc_i32_to_i16_saturating(i32::from(surface.top) + rect.2);
                    let right = ppc_i32_to_i16_saturating(i32::from(surface.left) + rect.3);
                    let width = i32::from(right) - i32::from(left);
                    let height = i32::from(bottom) - i32::from(top);
                    let horizontal_exposed = dh.unsigned_abs().min(width.max(0) as u32) as i32;
                    let vertical_exposed = dv.unsigned_abs().min(height.max(0) as u32) as i32;
                    let mut rows = vec![Vec::new(); height.max(0) as usize];
                    for row in 0..height.max(0) {
                        let full_row = (dv > 0 && row < vertical_exposed)
                            || (dv < 0 && row >= height - vertical_exposed);
                        rows[row as usize] = if full_row {
                            vec![left, right]
                        } else if horizontal_exposed > 0 && dh > 0 {
                            vec![
                                left,
                                ppc_i32_to_i16_saturating(i32::from(left) + horizontal_exposed),
                            ]
                        } else if horizontal_exposed > 0 && dh < 0 {
                            vec![
                                ppc_i32_to_i16_saturating(i32::from(right) - horizontal_exposed),
                                right,
                            ]
                        } else {
                            Vec::new()
                        };
                    }
                    let storage = ppc_region_storage_from_rows(top, &rows)
                        .unwrap_or_else(|| vec![0, 10, 0, 0, 0, 0, 0, 0, 0, 0]);
                    let mut allocator = PpcProcessAllocatorView {
                        memory_manager: process_memory_manager,
                    };
                    let _ = ppc_write_region_storage(
                        Some(&mut allocator),
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        cpu.gpr[6],
                        &storage,
                    );
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::CopyMask
        | PpcQuickDrawCompatibilityOperation::CopyDeepMask => {
            unreachable!("bit-transfer imports return through dispatch_bit_transfer_import")
        }
        PpcQuickDrawCompatibilityOperation::GetEntryColor => {
            // GetEntryColor copies the palette entry's RGB components to the
            // caller's RGBColor. Inside Macintosh Volume VI (1991), p. 20-25.
            let palette_handle = cpu.gpr[3];
            let entry = cpu.gpr[4] as u16 as i16;
            let rgb_ptr = cpu.gpr[5];
            if entry >= 0 && ppc_memory_can_write_bytes(memory, rgb_ptr, 6) {
                if let Some(palette_ptr) = memory
                    .read_u32_be(palette_handle)
                    .filter(|palette_ptr| *palette_ptr != 0)
                {
                    let entry = entry as u32;
                    if memory
                        .read_u16_be(palette_ptr)
                        .is_some_and(|count| entry < u32::from(count))
                    {
                        let info_ptr = palette_ptr + 16 + entry * 16;
                        if let (Some(red), Some(green), Some(blue)) = (
                            memory.read_u16_be(info_ptr),
                            memory.read_u16_be(info_ptr + 2),
                            memory.read_u16_be(info_ptr + 4),
                        ) {
                            let _ = memory.write_u16_be(rgb_ptr, red);
                            let _ = memory.write_u16_be(rgb_ptr + 2, green);
                            let _ = memory.write_u16_be(rgb_ptr + 4, blue);
                        }
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::SetEntryColor => {
            // PaletteHandle, entry index, and RGBColor pointer. Inside Macintosh
            // Volume VI 1991, p. 20-25.
            let palette_handle = cpu.gpr[3];
            let entry = cpu.gpr[4] as u16 as i16;
            let rgb_ptr = cpu.gpr[5];
            if entry >= 0 {
                if let Some(palette_ptr) = memory
                    .read_u32_be(palette_handle)
                    .filter(|palette_ptr| *palette_ptr != 0)
                {
                    let entry = entry as u32;
                    if memory
                        .read_u16_be(palette_ptr)
                        .is_some_and(|count| entry < u32::from(count))
                    {
                        let info_ptr = palette_ptr + 16 + entry * 16;
                        if let (Some(red), Some(green), Some(blue)) = (
                            memory.read_u16_be(rgb_ptr),
                            memory.read_u16_be(rgb_ptr + 2),
                            memory.read_u16_be(rgb_ptr + 4),
                        ) {
                            let _ = memory.write_u16_be(info_ptr, red);
                            let _ = memory.write_u16_be(info_ptr + 2, green);
                            let _ = memory.write_u16_be(info_ptr + 4, blue);
                        }
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::GetEntryUsage => {
            let palette_handle = cpu.gpr[3];
            let entry = cpu.gpr[4] as u16 as i16;
            if entry >= 0 {
                if let Some(palette_ptr) = memory
                    .read_u32_be(palette_handle)
                    .filter(|palette_ptr| *palette_ptr != 0)
                {
                    let entry = entry as u32;
                    if memory
                        .read_u16_be(palette_ptr)
                        .is_some_and(|count| entry < u32::from(count))
                    {
                        let info_ptr = palette_ptr + 16 + entry * 16;
                        if let Some(usage) = memory.read_u16_be(info_ptr + 6) {
                            let _ = memory.write_u16_be(cpu.gpr[5], usage);
                        }
                        if let Some(tolerance) = memory.read_u16_be(info_ptr + 8) {
                            let _ = memory.write_u16_be(cpu.gpr[6], tolerance);
                        }
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::SetEntryUsage => {
            // Inside Macintosh Volume VI 1991, pp. 20-24--20-25.
            let palette_handle = cpu.gpr[3];
            let entry = cpu.gpr[4] as u16 as i16;
            if entry >= 0 {
                if let Some(palette_ptr) = memory
                    .read_u32_be(palette_handle)
                    .filter(|palette_ptr| *palette_ptr != 0)
                {
                    let entry = entry as u32;
                    if memory
                        .read_u16_be(palette_ptr)
                        .is_some_and(|count| entry < u32::from(count))
                    {
                        let info_ptr = palette_ptr + 16 + entry * 16;
                        // Inside Macintosh Volume VI (1991), p. 20-25:
                        // $FFFF independently preserves the corresponding
                        // usage or tolerance field.
                        if cpu.gpr[5] as u16 != u16::MAX {
                            let _ = memory.write_u16_be(info_ptr + 6, cpu.gpr[5] as u16);
                        }
                        if cpu.gpr[6] as u16 != u16::MAX {
                            let _ = memory.write_u16_be(info_ptr + 8, cpu.gpr[6] as u16);
                        }
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::AnimateEntry => {
            let window = cpu.gpr[3];
            let entry = usize::from(cpu.gpr[4] as u16);
            let assigned_palette = memory
                .read_u32_be(window.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
                .unwrap_or(0);
            let palette = if assigned_palette != 0 {
                assigned_palette
            } else {
                toolbox_startup.application_palette
            };
            let palette_ptr = memory.read_u32_be(palette).unwrap_or(0);
            let info = palette_ptr.wrapping_add(16 + entry as u32 * 16);
            let usage = memory.read_u16_be(info + 6).unwrap_or(0);
            if usage & 0x0004 != 0 {
                let allocations = toolbox_startup
                    .palette_allocations
                    .iter()
                    .filter(|allocation| allocation.palette == palette)
                    .filter_map(|allocation| {
                        allocation
                            .entry_mappings
                            .get(entry)
                            .copied()
                            .and_then(PpcPaletteEntryMapping::animated_index)
                            .map(|index| (allocation.gdevice, index))
                    })
                    .collect::<Vec<_>>();
                if let Some(color) =
                    ppc_read_rgb_color(memory, cpu.gpr[5]).filter(|_| !allocations.is_empty())
                {
                    let _ = ppc_write_rgb_color(memory, info, color);
                    for (gdevice, index) in allocations {
                        let slot = usize::from(index);
                        if gdevice == current_gdevice {
                            screen_clut[slot] = [color.red, color.green, color.blue];
                            color_manager_clut[slot] = [color.red, color.green, color.blue];
                        }
                        if let Some(table) = ppc_gdevice_ctable_handle(memory, gdevice)
                            .and_then(|handle| memory.read_u32_be(handle))
                            .filter(|ptr| *ptr != 0)
                        {
                            let _ = ppc_write_rgb_color(
                                memory,
                                table + 10 + u32::from(index) * 8,
                                color,
                            );
                        }
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::AnimatePalette => {
            // Inside Macintosh Volume VI (1991), pp. 20-22--20-23:
            // copy a source ColorTable range into animated destination
            // palette entries and immediately update their device indexes.
            let window = cpu.gpr[3];
            let ctable_handle = cpu.gpr[4];
            let src_index = u32::from(cpu.gpr[5] as u16);
            let dst_entry = u32::from(cpu.gpr[6] as u16);
            let dst_length = u32::from(cpu.gpr[7] as u16);
            let assigned_palette = memory
                .read_u32_be(window + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET)
                .unwrap_or(0);
            let palette_handle = if assigned_palette != 0 {
                assigned_palette
            } else {
                toolbox_startup.application_palette
            };
            let palette_ptr = memory.read_u32_be(palette_handle).unwrap_or(0);
            let ctable_ptr = memory.read_u32_be(ctable_handle).unwrap_or(0);
            let palette_count = u32::from(memory.read_u16_be(palette_ptr).unwrap_or(0));
            let source_count = memory
                .read_u16_be(ctable_ptr + 6)
                .map_or(0, |last| u32::from(last) + 1);
            for offset in 0..dst_length {
                let source = src_index + offset;
                let destination = dst_entry + offset;
                if source >= source_count || destination >= palette_count {
                    break;
                }
                let spec_ptr = ctable_ptr + 8 + source * 8;
                let info_ptr = palette_ptr + 16 + destination * 16;
                let usage = memory.read_u16_be(info_ptr + 6).unwrap_or(0);
                if usage & 0x0004 == 0 {
                    continue;
                }
                let destination = destination as usize;
                let allocations = toolbox_startup
                    .palette_allocations
                    .iter()
                    .filter(|allocation| allocation.palette == palette_handle)
                    .filter_map(|allocation| {
                        allocation
                            .entry_mappings
                            .get(destination)
                            .copied()
                            .and_then(PpcPaletteEntryMapping::animated_index)
                            .map(|index| (allocation.gdevice, index))
                    })
                    .collect::<Vec<_>>();
                if allocations.is_empty() {
                    continue;
                }
                let Some(color) = ppc_read_rgb_color(memory, spec_ptr + 2) else {
                    break;
                };
                let _ = ppc_write_rgb_color(memory, info_ptr, color);
                let rgb = [color.red, color.green, color.blue];
                // Inside Macintosh Volume VI (1991), pp. 20-22--20-23:
                // animation changes only the RGB at the reserved device
                // index. It is not a color-environment change, so preserve
                // the table seed, ColorSpec.value metadata, and all other
                // entries.
                for (gdevice, index) in allocations {
                    if let Some(device_ctable) = ppc_gdevice_ctable_handle(memory, gdevice)
                        .and_then(|handle| memory.read_u32_be(handle))
                        .filter(|ptr| *ptr != 0)
                    {
                        let device_spec = device_ctable + 8 + u32::from(index) * 8;
                        let _ = ppc_write_rgb_color(memory, device_spec + 2, color);
                    }
                    if gdevice == current_gdevice {
                        screen_clut[usize::from(index)] = rgb;
                        color_manager_clut[usize::from(index)] = rgb;
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::PenPat => {
            let mut pattern = [0; 8];
            if memory.read_bytes_into(cpu.gpr[3], &mut pattern).is_some() {
                // Imaging With QuickDraw (1994), pp. 3-38--3-40: PenPat
                // installs the 8-by-8 bit pattern used by Paint and Frame
                // shape operations. A set bit selects the foreground color;
                // a clear bit selects the background color.
                toolbox_startup.quickdraw_pen_pattern = pattern;
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::BackPat => {
            let mut pattern = [0u8; 8];
            if memory.read_bytes_into(cpu.gpr[3], &mut pattern).is_some() {
                // BackPat changes the legacy pattern used to fill exposed
                // ScrollRect pixels. A prior PixPat must not continue to
                // shadow the newly installed Pattern (IM:V V-72).
                toolbox_startup.quickdraw_back_pattern = pattern;
                let _ = memory.write_u32_be(
                    current_gworld.wrapping_add(PPC_CGRAF_PORT_BK_PIXPAT_OFFSET),
                    0,
                );
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::BackPixPat => {
            let pattern_handle = cpu.gpr[3];
            if pattern_handle != 0 {
                // Keep the handle in the live CGrafPort so subsequent
                // ScrollRect calls see exactly the PixPat selected by the
                // application. Its pat1Data shadow is used by the common
                // monochrome fallback path.
                let _ = memory.write_u32_be(
                    current_gworld.wrapping_add(PPC_CGRAF_PORT_BK_PIXPAT_OFFSET),
                    pattern_handle,
                );
                if let Some(pattern_ptr) =
                    memory.read_u32_be(pattern_handle).filter(|ptr| *ptr != 0)
                {
                    let mut pattern = [0u8; 8];
                    if memory
                        .read_bytes_into(pattern_ptr.wrapping_add(20), &mut pattern)
                        .is_some()
                    {
                        toolbox_startup.quickdraw_back_pattern = pattern;
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickDrawCompatibilityOperation::SetStdCProcs
        | PpcQuickDrawCompatibilityOperation::SetStdProcs
        | PpcQuickDrawCompatibilityOperation::Exp1To3
        | PpcQuickDrawCompatibilityOperation::Exp1To6 => PpcImportAction::ReturnPreserve,
    }
}
