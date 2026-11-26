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

    // StdBits(srcBits, srcRect, dstRect, mode, maskRgn) is the CopyBits
    // bottleneck: CopyBits with the current port's bits as destination, as
    // the 68k $A8EB handler implements it. Shift its arguments into CopyBits'
    // register layout and restore the caller's argument registers after.
    let std_bits_arguments = matches!(
        binding.dispatcher_target,
        PpcImportDispatcherTarget::StdBits
    )
    .then(|| {
        let saved = [cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7], cpu.gpr[8]];
        cpu.gpr[8] = saved[3];
        cpu.gpr[7] = saved[2];
        cpu.gpr[6] = saved[1];
        cpu.gpr[5] = saved[0];
        cpu.gpr[4] = current_gworld.wrapping_add(2);
        saved
    });
    match binding.dispatcher_target {
        PpcImportDispatcherTarget::UnpackBits => {
            // UnpackBits ($A8D0)
            // Expands PackBits data and advances the source and destination pointer variables.
            // PROCEDURE UnpackBits (VAR srcPtr, dstPtr: Ptr; dstBytes: INTEGER);
            // Inside Macintosh Volume I (1985), I-470.
            let _ = ppc_unpack_bits(memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5] as i16);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CopyBits | PpcImportDispatcherTarget::StdBits => {
            if let Some((_, picture_gworld, _, commands)) = toolbox_startup.open_picture.as_mut() {
                if *picture_gworld == current_gworld {
                    let _ = ppc_record_copy_bits(cpu, memory, gworlds, commands);
                }
            }
            // Screen blits must not paint through windows above the current
            // port. Preserve their structure pixels, including presentation
            // detail, just as the Window Manager clips rear-window chrome.
            // CopyBits copies srcRect into dstRect, scaling it to fit, and a
            // copy into the current port is also clipped to its visRgn and
            // clipRgn (Imaging With QuickDraw, 1994, pp. 3-112--3-113 and
            // 3-116). Only pixels a copy can change are saved: none, when
            // the port's visRgn already excludes the windows in front.
            let screen_destination = window_list
                .contains(&current_gworld)
                .then(|| ppc_resolve_pixmap_bits_with_provenance(memory, gworlds, cpu.gpr[4]))
                .flatten()
                .zip(ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD))
                .filter(|(destination, front)| destination.bits.base_addr == front.base_addr);
            let saved_front = screen_destination.and_then(|(destination, _)| {
                // Local coordinates minus bounds.topLeft are global.
                let (dy, dx) = (destination.bits.top, destination.bits.left);
                let limit = ppc_read_rect(memory, cpu.gpr[6]).map(|(top, left, bottom, right)| {
                    (
                        top.saturating_sub(dy),
                        left.saturating_sub(dx),
                        bottom.saturating_sub(dy),
                        right.saturating_sub(dx),
                    )
                });
                // Reading the port clip is not free, so it waits for a
                // covered rectangle.
                let mut port_clip: Option<Option<Vec<u8>>> = None;
                let refine = |memory: &mut PpcSectionMem, rect| {
                    let clip = port_clip.get_or_insert_with(|| {
                        ppc_copy_bits_current_port_clip(
                            memory,
                            gworlds,
                            current_gworld,
                            destination.bits,
                        )
                    });
                    match clip.as_deref() {
                        Some(clip) => ppc_global_rect_within_port_clip(clip, (dy, dx), rect),
                        None => Some(rect),
                    }
                };
                ppc_front_window_occlusion_pixels_within(
                    memory,
                    gworlds,
                    window_list,
                    current_gworld,
                    limit,
                    refine,
                )
            });
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
            if let Some(saved) = std_bits_arguments {
                cpu.gpr[4..=8].copy_from_slice(&saved);
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

/// The bounding box of the part of `rect` (global) inside a port clip region
/// (port-local, where local minus `origin` is global), in global coordinates.
/// None when the clip leaves none of it; `rect` itself if the clip can't be
/// evaluated.
fn ppc_global_rect_within_port_clip(
    clip: &[u8],
    (dy, dx): (i16, i16),
    rect: (i16, i16, i16, i16),
) -> Option<(i16, i16, i16, i16)> {
    let mut local = vec![0, 10];
    for value in [
        rect.0.saturating_add(dy),
        rect.1.saturating_add(dx),
        rect.2.saturating_add(dy),
        rect.3.saturating_add(dx),
    ] {
        local.extend_from_slice(&value.to_be_bytes());
    }
    let Ok(inside) = ppc_region_storage_boolean(clip, &local, PpcRegionBooleanOp::Intersection)
    else {
        return Some(rect);
    };
    ppc_region_storage_bbox(&inside).map(|(top, left, bottom, right)| {
        (
            top.saturating_sub(dy),
            left.saturating_sub(dx),
            bottom.saturating_sub(dy),
            right.saturating_sub(dx),
        )
    })
}

fn ppc_unpack_bits(
    memory: &mut PpcSectionMem,
    source_variable: u32,
    destination_variable: u32,
    destination_bytes: i16,
) -> Option<()> {
    if source_variable == 0 || destination_variable == 0 || destination_bytes <= 0 {
        return Some(());
    }
    if !ppc_memory_can_write_bytes(memory, source_variable, 4)
        || !ppc_memory_can_write_bytes(memory, destination_variable, 4)
    {
        return None;
    }
    let mut source = memory.read_u32_be(source_variable)?;
    let destination = memory.read_u32_be(destination_variable)?;
    let output_len = destination_bytes as usize;
    let destination_end = destination.checked_add(output_len as u32)?;
    if !ppc_memory_can_write_bytes(memory, destination, output_len as u32) {
        return None;
    }
    let mut output = Vec::with_capacity(output_len);
    while output.len() < output_len {
        let flag = memory.read_u8(source)? as i8;
        source = source.checked_add(1)?;
        if flag >= 0 {
            for _ in 0..(flag as usize + 1).min(output_len - output.len()) {
                output.push(memory.read_u8(source)?);
                source = source.checked_add(1)?;
            }
        } else if flag != -128 {
            let value = memory.read_u8(source)?;
            source = source.checked_add(1)?;
            let count = ((1 - flag as i16) as usize).min(output_len - output.len());
            output.resize(output.len() + count, value);
        }
    }
    memory.write_bytes(destination, &output)?;
    memory.write_u32_be(source_variable, source)?;
    memory.write_u32_be(destination_variable, destination_end)?;
    Some(())
}
