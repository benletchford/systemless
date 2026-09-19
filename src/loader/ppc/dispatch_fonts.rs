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
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
}

pub(super) fn dispatch_font_import(
    context: PpcFontDispatchContext<'_>,
) -> Option<PpcImportAction> {
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
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InitFonts => {
            toolbox_startup.fonts_initialized = true;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MeasureText => {
            let count = cpu.gpr[3] as u16 as i16;
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_face = ppc_current_text_style(memory, current_gworld);
            ppc_measure_text(
                memory,
                count,
                cpu.gpr[4],
                cpu.gpr[5],
                text_font,
                *quickdraw_text_size,
                text_face,
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
            )))
        }
        PpcImportDispatcherTarget::TruncString => {
            let font = ppc_current_text_font(memory, current_gworld);
            let style = ppc_current_text_style(memory, current_gworld);
            Some(PpcImportAction::Return(ppc_i16_result(ppc_trunc_string(
                memory,
                cpu.gpr[4],
                cpu.gpr[3] as u16 as i16,
                cpu.gpr[5] as u16,
                font,
                *quickdraw_text_size,
                style,
            ))))
        }
        PpcImportDispatcherTarget::StringWidth => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_face = ppc_current_text_style(memory, current_gworld);
            let width = ppc_read_pascal_string(memory, cpu.gpr[3])
                .map(|bytes| {
                    ppc_text_width_bytes(text_font, *quickdraw_text_size, text_face, &bytes).max(0)
                        as u32
                })
                .unwrap_or(0);
            Some(PpcImportAction::Return(width))
        }
        PpcImportDispatcherTarget::CharWidth => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_face = ppc_current_text_style(memory, current_gworld);
            Some(PpcImportAction::Return(
                ppc_text_width_bytes(
                    text_font,
                    *quickdraw_text_size,
                    text_face,
                    &[(cpu.gpr[3] & 0xff) as u8],
                )
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
                    ppc_text_width_bytes(text_font, *quickdraw_text_size, text_style, &bytes)
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
