//! Typed Font Manager and text-measurement dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcFontDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) current_gworld: u32,
    pub(super) quickdraw_text_mode: &'a mut i16,
    pub(super) quickdraw_text_size: &'a mut i16,
    pub(super) quickdraw_fore_color: &'a PpcRgbColor,
    pub(super) quickdraw_fore_indices: &'a HashMap<u32, u8>,
    pub(super) quickdraw_pen_h: &'a mut i16,
    pub(super) quickdraw_pen_v: &'a mut i16,
    pub(super) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) current_resource_refnum: i16,
    pub(super) last_resource_error: &'a mut i16,
}

pub(super) fn dispatch_font_import(context: PpcFontDispatchContext<'_>) -> Option<PpcImportAction> {
    let PpcFontDispatchContext {
        binding,
        cpu,
        memory,
        toolbox_startup,
        gworlds,
        current_gworld,
        quickdraw_text_mode,
        quickdraw_text_size,
        quickdraw_fore_color,
        quickdraw_fore_indices,
        quickdraw_pen_h,
        quickdraw_pen_v,
        vfs_resources,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        current_resource_refnum,
        last_resource_error,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::FMSwapFont => {
            let result = font_swap::swap_font(
                cpu.gpr[3],
                memory,
                process_memory_manager,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                current_resource_refnum,
                last_resource_error,
            );
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::SetPreserveGlyph => {
            // Inside Macintosh: Text (1993), 4-62 through 4-63.
            toolbox_startup.preserve_glyph = cpu.gpr[3] != 0;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetPreserveGlyph => {
            Some(PpcImportAction::Return(u32::from(toolbox_startup.preserve_glyph)))
        }
        PpcImportDispatcherTarget::IsMetric => {
            // The active Roman numeric-format resource has metricSys = 0.
            // Inside Macintosh: Operating System Utilities (1994), 4-48.
            Some(PpcImportAction::Return(0))
        }
        // Inside Macintosh: Text (1993), p. 4-53: GetSysFont returns the
        // system font ID. Systemless models the standard systemFont value.
        PpcImportDispatcherTarget::GetSysFont => Some(PpcImportAction::Return(0)),
        // Inside Macintosh: Text (1993), p. 4-54: GetAppFont returns ApFontID.
        // The standard Roman application font is Geneva (family ID 3).
        PpcImportDispatcherTarget::GetAppFont => Some(PpcImportAction::Return(3)),
        PpcImportDispatcherTarget::FontToScript => {
            // FontToScript maps a font family ID to an enabled script. This
            // Roman system has only script 0 enabled, so non-Roman families
            // also default to the system script.
            // Inside Macintosh: Text (1993), pp. 6-22 to 6-24, 6-82 to 6-83.
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::VisibleLength => {
            // VisibleLength excludes whitespace at the display end of a
            // Roman, left-to-right style run without changing its contents.
            // Inside Macintosh: Text (1993), pp. 3-36 to 3-38, 3-82.
            let mut visible = (cpu.gpr[4] as i32).max(0) as u32;
            if !ppc_memory_can_read_bytes(memory, cpu.gpr[3], visible) {
                return None;
            }
            while visible > 0 {
                let byte = memory.read_u8(cpu.gpr[3].checked_add(visible - 1)?)?;
                if !matches!(byte, b' ' | b'\t') {
                    break;
                }
                visible -= 1;
            }
            Some(PpcImportAction::Return(visible))
        }
        // The same reference specifies that GetDefFontSize returns
        // SysFontSize, using 12 points when that low-memory value is zero.
        // Systemless currently models the default system font at 12 points.
        PpcImportDispatcherTarget::GetDefFontSize => Some(PpcImportAction::Return(12)),
        PpcImportDispatcherTarget::GetFontName => {
            let font_id = cpu.gpr[3] as u16 as i16;
            let name = font_name_for_id(font_id)
                .map(str::to_owned)
                .or_else(|| ppc_vfs_font_name_for_id(vfs_resources, font_id))
                .unwrap_or_default();
            let _ = ppc_write_pstring_bytes(memory, cpu.gpr[4], name.as_bytes());
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InitFonts => {
            toolbox_startup.fonts_initialized = true;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MeasureText => {
            let count = cpu.gpr[3] as u16 as i16;
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_face = ppc_current_text_style(memory, current_gworld);
            let char_extra = ppc_port_char_extra_packed(memory, current_gworld);
            ppc_measure_text(
                memory,
                count,
                cpu.gpr[4],
                cpu.gpr[5],
                text_font,
                *quickdraw_text_size,
                text_face,
                char_extra,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RealFont => {
            let font = cpu.gpr[3] as u16 as i16;
            let size = cpu.gpr[4] as u16 as i16;
            Some(PpcImportAction::Return(u32::from(
                font != FONT_APPLICATION
                    && get_font_face(font, ppc_te_font_lookup_size(size)).is_some(),
            )))
        }
        PpcImportDispatcherTarget::TextWidth => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_face = ppc_current_text_style(memory, current_gworld);
            Some(PpcImportAction::Return(ppc_text_width(
                cpu,
                memory,
                text_font,
                *quickdraw_text_size,
                text_face,
                current_gworld,
            )))
        }
        PpcImportDispatcherTarget::TruncString => {
            let font = ppc_current_text_font(memory, current_gworld);
            let style = ppc_current_text_style(memory, current_gworld);
            let char_extra = ppc_port_char_extra_packed(memory, current_gworld);
            Some(PpcImportAction::Return(ppc_i16_result(ppc_trunc_string(
                memory,
                cpu.gpr[4],
                cpu.gpr[3] as u16 as i16,
                cpu.gpr[5] as u16,
                font,
                *quickdraw_text_size,
                style,
                char_extra,
            ))))
        }
        PpcImportDispatcherTarget::StringWidth => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_face = ppc_current_text_style(memory, current_gworld);
            let width = ppc_read_pascal_string(memory, cpu.gpr[3])
                .map(|bytes| {
                    (i32::from(ppc_text_width_bytes(
                        text_font,
                        *quickdraw_text_size,
                        text_face,
                        &bytes,
                    )) + ppc_char_extra_width(
                        ppc_port_char_extra_packed(memory, current_gworld),
                        *quickdraw_text_size,
                        &bytes,
                    ))
                    .max(0) as u32
                })
                .unwrap_or(0);
            Some(PpcImportAction::Return(width))
        }
        PpcImportDispatcherTarget::CharWidth => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_face = ppc_current_text_style(memory, current_gworld);
            let bytes = [(cpu.gpr[3] & 0xff) as u8];
            Some(PpcImportAction::Return(
                (i32::from(ppc_text_width_bytes(
                    text_font,
                    *quickdraw_text_size,
                    text_face,
                    &bytes,
                )) + ppc_char_extra_width(
                    ppc_port_char_extra_packed(memory, current_gworld),
                    *quickdraw_text_size,
                    &bytes,
                ))
                .max(0) as u32,
            ))
        }
        PpcImportDispatcherTarget::GetFontInfo => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            ppc_get_font_info(memory, cpu.gpr[3], text_font, *quickdraw_text_size);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FontMetrics => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            ppc_font_metrics(memory, cpu.gpr[3], text_font, *quickdraw_text_size);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DrawChar => {
            let ch = (cpu.gpr[3] & 0xff) as u8;
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_style = ppc_current_text_style(memory, current_gworld);
            let advance = ppc_draw_text_bytes_styled(
                memory,
                gworlds,
                current_gworld,
                (*quickdraw_pen_h, *quickdraw_pen_v),
                text_font,
                *quickdraw_text_size,
                *quickdraw_text_mode,
                *quickdraw_fore_color,
                quickdraw_fore_indices.get(&current_gworld).copied(),
                text_style,
                &[ch],
            );
            *quickdraw_pen_h = (*quickdraw_pen_h).saturating_add(advance);
            ppc_sync_gworld_pen(memory, current_gworld, *quickdraw_pen_h, *quickdraw_pen_v);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DrawText => {
            let text_ptr = cpu.gpr[3];
            let first_byte = cpu.gpr[4];
            let byte_count = cpu.gpr[5];
            let mut bytes = Vec::with_capacity(byte_count.min(i16::MAX as u32) as usize);
            for offset in 0..byte_count {
                let Some(addr) = text_ptr
                    .checked_add(first_byte)
                    .and_then(|base| base.checked_add(offset))
                else {
                    break;
                };
                let Some(byte) = memory.read_u8(addr) else {
                    break;
                };
                bytes.push(byte);
            }
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_style = ppc_current_text_style(memory, current_gworld);
            let advance = ppc_draw_text_bytes_styled(
                memory,
                gworlds,
                current_gworld,
                (*quickdraw_pen_h, *quickdraw_pen_v),
                text_font,
                *quickdraw_text_size,
                *quickdraw_text_mode,
                *quickdraw_fore_color,
                quickdraw_fore_indices.get(&current_gworld).copied(),
                text_style,
                &bytes,
            );
            *quickdraw_pen_h = (*quickdraw_pen_h).saturating_add(advance);
            ppc_sync_gworld_pen(memory, current_gworld, *quickdraw_pen_h, *quickdraw_pen_v);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DrawString => {
            if let Some(bytes) = ppc_read_pascal_string(memory, cpu.gpr[3]) {
                let text_font = ppc_current_text_font(memory, current_gworld);
                let text_style = ppc_current_text_style(memory, current_gworld);
                if std::env::var_os("SYSTEMLESS_TRACE_FONT_TRAPS").is_some() {
                    eprintln!(
                        "[FONT] PPC DrawString port=${:08X} font={} size={} text={:?}",
                        current_gworld,
                        text_font,
                        *quickdraw_text_size,
                        decode_mac_roman(&bytes),
                    );
                }
                let advance = if let Some(commands) =
                    ppc_open_picture_commands(toolbox_startup, current_gworld)
                {
                    pict::recording_push_long_text(
                        commands,
                        *quickdraw_pen_v,
                        *quickdraw_pen_h,
                        &bytes,
                    );
                    let base =
                        ppc_text_width_bytes(text_font, *quickdraw_text_size, text_style, &bytes);
                    (i32::from(base)
                        + ppc_char_extra_width(
                            ppc_port_char_extra_packed(memory, current_gworld),
                            *quickdraw_text_size,
                            &bytes,
                        ))
                    .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
                } else {
                    ppc_draw_text_bytes_styled(
                        memory,
                        gworlds,
                        current_gworld,
                        (*quickdraw_pen_h, *quickdraw_pen_v),
                        text_font,
                        *quickdraw_text_size,
                        *quickdraw_text_mode,
                        *quickdraw_fore_color,
                        quickdraw_fore_indices.get(&current_gworld).copied(),
                        text_style,
                        &bytes,
                    )
                };
                *quickdraw_pen_h = (*quickdraw_pen_h).saturating_add(advance);
                ppc_sync_gworld_pen(memory, current_gworld, *quickdraw_pen_h, *quickdraw_pen_v);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TextFont => {
            if std::env::var_os("SYSTEMLESS_TRACE_FONT_TRAPS").is_some() {
                eprintln!(
                    "[FONT] PPC TextFont port=${:08X} font={}",
                    current_gworld, cpu.gpr[3] as u16 as i16,
                );
            }
            if current_gworld != 0 {
                let _ = memory.write_u16_be(
                    current_gworld + PPC_CGRAF_PORT_TX_FONT_OFFSET,
                    cpu.gpr[3] as u16,
                );
            }
            if let Some(commands) = ppc_open_picture_commands(toolbox_startup, current_gworld) {
                pict::recording_push_word(commands, 0x0003);
                pict::recording_push_word(commands, cpu.gpr[3] as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TextFace => {
            if current_gworld != 0 {
                let _ = memory.write_u8(
                    current_gworld + PPC_CGRAF_PORT_TX_FACE_OFFSET,
                    cpu.gpr[3] as u8,
                );
            }
            if let Some(commands) = ppc_open_picture_commands(toolbox_startup, current_gworld) {
                pict::recording_push_word(commands, 0x0004);
                commands.extend_from_slice(&[cpu.gpr[3] as u8, 0]);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TextMode => {
            *quickdraw_text_mode = cpu.gpr[3] as u16 as i16;
            ppc_sync_gworld_text(
                memory,
                current_gworld,
                *quickdraw_text_mode,
                *quickdraw_text_size,
            );
            if let Some(commands) = ppc_open_picture_commands(toolbox_startup, current_gworld) {
                pict::recording_push_word(commands, 0x0005);
                pict::recording_push_word(commands, *quickdraw_text_mode as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TextSize => {
            if std::env::var_os("SYSTEMLESS_TRACE_FONT_TRAPS").is_some() {
                eprintln!(
                    "[FONT] PPC TextSize port=${:08X} size={}",
                    current_gworld, cpu.gpr[3] as u16 as i16,
                );
            }
            *quickdraw_text_size = cpu.gpr[3] as u16 as i16;
            ppc_sync_gworld_text(
                memory,
                current_gworld,
                *quickdraw_text_mode,
                *quickdraw_text_size,
            );
            if let Some(commands) = ppc_open_picture_commands(toolbox_startup, current_gworld) {
                pict::recording_push_word(commands, 0x000D);
                pict::recording_push_word(commands, *quickdraw_text_size as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CharExtra => {
            // Inside Macintosh: Text (1993), p. 3-23: CharExtra stores the
            // Fixed pixel amount as signed 4.12 per txSize in a CGrafPort.
            // A monochrome GrafPort has no chExtra field.
            if current_gworld != 0
                && memory
                    .read_u16_be(current_gworld + 6)
                    .is_some_and(|version| version & 0xc000 == 0xc000)
            {
                let text_size = if *quickdraw_text_size == PPC_QD_TEXT_SIZE_SYSTEM {
                    12
                } else {
                    i32::from((*quickdraw_text_size).max(1))
                };
                let packed = ((cpu.gpr[3] as i32) / (text_size * 16))
                    .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                    as i16;
                let _ = memory.write_u16_be(
                    current_gworld + PPC_CGRAF_PORT_CH_EXTRA_OFFSET,
                    packed as u16,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetFNum => {
            // Inside Macintosh: Text (1993), 4-52: GetFNum maps a Str255 font
            // family name to its ID and returns zero when no family matches.
            let font_id = ppc_read_pstring_bytes(memory, cpu.gpr[3])
                .map(|name| decode_mac_roman(&name))
                .and_then(|name| {
                    font_id_for_name(&name)
                        .or_else(|| ppc_vfs_font_id_for_name(vfs_resources, &name))
                })
                .unwrap_or(0);
            if cpu.gpr[4] != 0 {
                let _ = memory.write_u16_be(cpu.gpr[4], font_id as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}

fn ppc_text_width(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    text_font: i16,
    text_size: i16,
    text_face: u8,
    current_gworld: u32,
) -> u32 {
    let text_ptr = cpu.gpr[3];
    let first_byte = cpu.gpr[4];
    let byte_count = cpu.gpr[5];
    let mut bytes = Vec::with_capacity(byte_count.min(i16::MAX as u32) as usize);
    for offset in 0..byte_count {
        let Some(addr) = text_ptr
            .checked_add(first_byte)
            .and_then(|base| base.checked_add(offset))
        else {
            break;
        };
        bytes.push(memory.read_u8(addr).unwrap_or(0));
    }
    (i32::from(ppc_text_width_bytes(
        text_font, text_size, text_face, &bytes,
    )) + ppc_char_extra_width(
        ppc_port_char_extra_packed(memory, current_gworld),
        text_size,
        &bytes,
    ))
    .max(0) as u32
}

fn ppc_get_font_info(memory: &mut PpcSectionMem, info_ptr: u32, text_font: i16, text_size: i16) {
    if info_ptr == 0 || !ppc_memory_can_write_bytes(memory, info_ptr, 8) {
        return;
    }
    // Inside Macintosh Volume I (1985), p. I-173: FontInfo contains the
    // current port font's ascent, descent, maximum advance, and leading.
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    let metrics = face.metrics;
    let _ = memory.write_u16_be(
        info_ptr,
        ppc_scale_font_value(i32::from(metrics.ascent), numerator, denominator) as u16,
    );
    let _ = memory.write_u16_be(
        info_ptr + 2,
        ppc_scale_font_value(i32::from(metrics.descent), numerator, denominator) as u16,
    );
    let _ = memory.write_u16_be(
        info_ptr + 4,
        ppc_scale_font_value(i32::from(metrics.wid_max), numerator, denominator) as u16,
    );
    let _ = memory.write_u16_be(
        info_ptr + 6,
        ppc_scale_font_value(i32::from(metrics.leading), numerator, denominator) as u16,
    );
}

fn ppc_font_metrics(memory: &mut PpcSectionMem, metrics_ptr: u32, text_font: i16, text_size: i16) {
    if metrics_ptr == 0 || !ppc_memory_can_write_bytes(memory, metrics_ptr, 20) {
        return;
    }
    // Inside Macintosh: Text (1993), pp. 4-54--4-55: FMetricRec stores
    // ascent, descent, leading, and maximum width as Fixed values, followed
    // by a handle to the global width table. Systemless does not model that
    // table, matching the 68k HLE by returning NIL for its handle.
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    let metrics = face.metrics;
    let to_fixed = |value: i16| -> u32 { (i32::from(value) as u32) << 16 };
    let _ = memory.write_u32_be(
        metrics_ptr,
        to_fixed(ppc_scale_font_value(
            i32::from(metrics.ascent),
            numerator,
            denominator,
        )),
    );
    let _ = memory.write_u32_be(
        metrics_ptr + 4,
        to_fixed(ppc_scale_font_value(
            i32::from(metrics.descent),
            numerator,
            denominator,
        )),
    );
    let _ = memory.write_u32_be(
        metrics_ptr + 8,
        to_fixed(ppc_scale_font_value(
            i32::from(metrics.leading),
            numerator,
            denominator,
        )),
    );
    let _ = memory.write_u32_be(
        metrics_ptr + 12,
        to_fixed(ppc_scale_font_value(
            i32::from(metrics.wid_max),
            numerator,
            denominator,
        )),
    );
    let _ = memory.write_u32_be(metrics_ptr + 16, 0);
}
