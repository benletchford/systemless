//! Typed Font Manager and text-measurement dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcFontDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) current_gworld: u32,
    pub(super) quickdraw_text_size: i16,
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
        current_gworld,
        quickdraw_text_size,
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
                quickdraw_text_size,
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
                quickdraw_text_size,
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
                quickdraw_text_size,
                style,
            ))))
        }
        PpcImportDispatcherTarget::StringWidth => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            let text_face = ppc_current_text_style(memory, current_gworld);
            let width = ppc_read_pascal_string(memory, cpu.gpr[3])
                .map(|bytes| {
                    ppc_text_width_bytes(text_font, quickdraw_text_size, text_face, &bytes).max(0)
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
                    quickdraw_text_size,
                    text_face,
                    &[(cpu.gpr[3] & 0xff) as u8],
                )
                .max(0) as u32,
            ))
        }
        PpcImportDispatcherTarget::GetFontInfo => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            ppc_get_font_info(memory, cpu.gpr[3], text_font, quickdraw_text_size);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FontMetrics => {
            let text_font = ppc_current_text_font(memory, current_gworld);
            ppc_font_metrics(memory, cpu.gpr[3], text_font, quickdraw_text_size);
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
