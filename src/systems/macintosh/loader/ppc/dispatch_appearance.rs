//! Appearance Manager imports for PowerPC.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_appearance_import(
    binding: &PpcImportBinding,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &mut [PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> Option<PpcImportAction> {
    match binding.dispatcher_target {
        PpcImportDispatcherTarget::RegisterAppearanceClient
        | PpcImportDispatcherTarget::UnregisterAppearanceClient => {
            // The runtime selects one appearance for the process and its
            // existing UI adapters already draw using that appearance.
            // Registration does not change this policy.
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::SetControlFontStyle => {
            let control = cpu.gpr[3];
            let style = cpu.gpr[4];
            let valid_control = controls.iter().any(|record| record.handle == control)
                && memory
                    .read_u32_be(control)
                    .is_some_and(|control_ptr| control_ptr != 0);
            let Some(style_bytes) = ppc_memory_read_bytes(memory, style, 24) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            if !valid_control {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else {
                let style_value = crate::control_manager::ControlFontStyle {
                    flags: i16::from_be_bytes([style_bytes[0], style_bytes[1]]),
                    font: i16::from_be_bytes([style_bytes[2], style_bytes[3]]),
                    size: i16::from_be_bytes([style_bytes[4], style_bytes[5]]),
                    style: i16::from_be_bytes([style_bytes[6], style_bytes[7]]),
                    mode: i16::from_be_bytes([style_bytes[8], style_bytes[9]]),
                    justification: i16::from_be_bytes([style_bytes[10], style_bytes[11]]),
                    foreground: [
                        u16::from_be_bytes([style_bytes[12], style_bytes[13]]),
                        u16::from_be_bytes([style_bytes[14], style_bytes[15]]),
                        u16::from_be_bytes([style_bytes[16], style_bytes[17]]),
                    ],
                    background: [
                        u16::from_be_bytes([style_bytes[18], style_bytes[19]]),
                        u16::from_be_bytes([style_bytes[20], style_bytes[21]]),
                        u16::from_be_bytes([style_bytes[22], style_bytes[23]]),
                    ],
                };
                if let Some(record) = controls.iter_mut().find(|record| record.handle == control) {
                    let style = (style_value.flags != 0).then_some(style_value);
                    if record.font_style != style { record.paint.invalidate_presentation(); }
                    record.font_style = style;
                }
                // The painter applies the font, face, size and foreground
                // colour to the title; see ppc_control_title_style.
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
            }
        }
        PpcImportDispatcherTarget::ActivateControl | PpcImportDispatcherTarget::DeactivateControl => {
            // Activation is kept apart from contrlHilite: applications are
            // told to call IsControlActive instead of reading that field.
            // Mac OS 8 Control Manager Reference, IsControlActive.
            let handle = cpu.gpr[3];
            if !controls.iter().any(|record| record.handle == handle)
                || memory.read_u32_be(handle).filter(|control| *control != 0).is_none()
            {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            let active = binding.dispatcher_target == PpcImportDispatcherTarget::ActivateControl;
            if let Some(record) = controls.iter_mut().find(|record| record.handle == handle) {
                record.active = active;
                let _ = ppc_draw_control(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    handle,
                );
            }
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::IsControlActive => {
            let handle = cpu.gpr[3];
            let active = controls
                .iter()
                .find(|record| record.handle == handle)
                .is_some_and(|record| record.active);
            Some(PpcImportAction::Return(u32::from(active)))
        }
        PpcImportDispatcherTarget::CollapseWindow => {
            // The PPC window adapter has no collapsed-window representation.
            let window = cpu.gpr[3];
            let valid_window = window != 0
                && memory.read_u8(window + PPC_CWINDOW_VISIBLE_OFFSET).is_some();
            let collapse = cpu.gpr[4] != 0;
            let result = crate::window_manager::evaluate_collapse_window(valid_window, collapse);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::IsWindowCollapsed => {
            let window = cpu.gpr[3];
            let valid_window = window != 0
                && memory.read_u8(window + PPC_CWINDOW_VISIBLE_OFFSET).is_some();
            let collapsed = crate::window_manager::evaluate_is_window_collapsed(valid_window);
            Some(PpcImportAction::Return(u32::from(collapsed)))
        }
        _ => None,
    }
}
