//! Typed QuickDraw bit-transfer dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcBitTransferDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) window_list: &'a SharedProcessWindowList,
    pub(super) current_gworld: u32,
    pub(super) current_gdevice: u32,
    pub(super) quickdraw_op_colors: &'a SharedProcessQuickDrawOpColors,
    pub(super) color_manager_clut: &'a [[u16; 3]; 256],
    pub(super) quickdraw_fore_color: PpcRgbColor,
    pub(super) quickdraw_fore_index: Option<u8>,
    pub(super) quickdraw_back_color: PpcRgbColor,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_bit_transfer_import(
    context: PpcBitTransferDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcBitTransferDispatchContext {
        binding,
        cpu,
        memory,
        gworlds,
        window_list,
        current_gworld,
        current_gdevice,
        quickdraw_op_colors,
        color_manager_clut,
        quickdraw_fore_color,
        quickdraw_fore_index,
        quickdraw_back_color,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::CopyBits => {
            if let Some((_, picture_gworld, _, commands)) = toolbox_startup.open_picture.as_mut() {
                if *picture_gworld == current_gworld {
                    let _ = ppc_record_copy_bits(cpu, memory, gworlds, commands);
                }
            }
            // Screen blits must not paint through windows above the current
            // port. Preserve their structure pixels, including presentation
            // detail, just as the Window Manager clips rear-window chrome.
            let saved_front = if window_list.contains(&current_gworld)
                && ppc_resolve_pixmap_bits_with_provenance(memory, gworlds, cpu.gpr[4])
                    .zip(ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD))
                    .is_some_and(|(destination, front)| {
                        destination.bits.base_addr == front.base_addr
                    }) {
                ppc_front_window_occlusion_pixels(memory, gworlds, window_list, current_gworld)
            } else {
                None
            };
            let op_color = ppc_current_op_color(memory, current_gworld, quickdraw_op_colors);
            let quickdraw_back_index = toolbox_startup
                .quickdraw_back_indices
                .get(&current_gworld)
                .copied();
            let _ = ppc_copy_bits(
                cpu,
                memory,
                gworlds,
                current_gworld,
                current_gdevice,
                toolbox_startup,
                color_manager_clut,
                quickdraw_fore_color,
                quickdraw_fore_index,
                quickdraw_back_color,
                quickdraw_back_index,
                op_color,
            );
            if let Some(saved) = saved_front {
                for (index, (x, y, pixel)) in saved.pixels.iter().copied().enumerate() {
                    let _ =
                        ppc_quickdraw_write_raw_pixel(memory, saved.front_buffer, (x, y), pixel);
                    ppc_restore_saved_detail(
                        memory,
                        saved.front_buffer,
                        (x, y),
                        &saved.pixels,
                        index,
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::CopyMask,
        ) => {
            let _ = ppc_copy_mask(cpu, memory, gworlds, color_manager_clut);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::CopyDeepMask,
        ) => {
            let _ = ppc_copy_deep_mask(cpu, memory, gworlds, color_manager_clut);
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
