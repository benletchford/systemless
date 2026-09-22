use super::*;

pub(super) const PPC_DM_MODE_LIST_ENTRY_STRIDE: u32 = 0x320;
pub(super) const PPC_DM_MODE_LIST_SIZE: u32 = PPC_DM_MODE_LIST_ENTRY_STRIDE * 3;
pub(super) const PPC_DM_MODE_LIST_MAGIC: u32 = u32::from_be_bytes(*b"DML1");
pub(super) const PPC_DM_MODE_LIST_ENTRY_OFFSET: u32 = 0x10;
pub(super) const PPC_DM_MODE_LIST_SWITCH_INFO_OFFSET: u32 = 0x30;
pub(super) const PPC_DM_MODE_LIST_RESOLUTION_INFO_OFFSET: u32 = 0x80;
pub(super) const PPC_DM_MODE_LIST_TIMING_INFO_OFFSET: u32 = 0xa0;
pub(super) const PPC_DM_MODE_LIST_DEPTH_BLOCK_OFFSET: u32 = 0xb8;
pub(super) const PPC_DM_MODE_LIST_DEPTH_INFO_OFFSET: u32 = 0xd0;
pub(super) const PPC_DM_MODE_LIST_VP_BLOCK_OFFSET: u32 = 0x140;
pub(super) const PPC_DM_MODE_LIST_NAME_OFFSET: u32 = 0x220;
pub(super) const PPC_DM_640_480_MODE_ID: u32 = 0x80;
pub(super) const PPC_DM_512_342_MODE_ID: u32 = 0x81;
pub(super) const PPC_DM_NATIVE_MODE_ID: u32 = 0x85;
#[cfg(test)]
pub(super) const PPC_DM_CURRENT_DISPLAY_MODE_ID: u32 = PPC_DM_NATIVE_MODE_ID;
pub(super) const PPC_DM_NO_SWITCH_CONFIRM_MASK: u32 = 1;
pub(super) const PPC_DM_DEPTH_NOT_AVAILABLE_MASK: u32 = 1 << 1;
pub(super) const PPC_DM_MODE_NOT_FOUND_ERR: i16 = -330;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcDmLiveDisplayMode {
    pub(super) display_mode_id: u32,
    pub(super) depth_mode: u32,
    pub(super) base_addr: u32,
    pub(super) row_bytes: u16,
    pub(super) bounds: (i16, i16, i16, i16),
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) pm_version: u16,
    pub(super) pack_type: u16,
    pub(super) pack_size: u32,
    pub(super) h_res: u32,
    pub(super) v_res: u32,
    pub(super) pixel_type: u16,
    pub(super) pixel_size: u16,
    pub(super) component_count: u16,
    pub(super) component_size: u16,
    pub(super) plane_bytes: u32,
}

const PPC_DM_GEOMETRIES: [(u32, u32, u32); 3] = [
    (PPC_DM_512_342_MODE_ID, 512, 342),
    (PPC_DM_640_480_MODE_ID, 640, 480),
    (PPC_DM_NATIVE_MODE_ID, 0, 0),
];

pub(super) fn ppc_dm_geometry(display_mode_id: u32) -> Option<(u32, u32)> {
    match display_mode_id {
        PPC_DM_512_342_MODE_ID => Some((512, 342)),
        PPC_DM_640_480_MODE_ID => Some((640, 480)),
        PPC_DM_NATIVE_MODE_ID => Some((ppc_main_screen_width(), ppc_main_screen_height())),
        _ => None,
    }
}

fn ppc_dm_mode_id_for_geometry(width: u32, height: u32) -> u32 {
    PPC_DM_GEOMETRIES
        .into_iter()
        .find_map(|(mode, candidate_width, candidate_height)| {
            let (candidate_width, candidate_height) = if candidate_width == 0 {
                (ppc_main_screen_width(), ppc_main_screen_height())
            } else {
                (candidate_width, candidate_height)
            };
            (width == candidate_width && height == candidate_height).then_some(mode)
        })
        .unwrap_or(PPC_DM_NATIVE_MODE_ID)
}

pub(super) struct PpcDisplayDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) gworlds: &'a mut [PpcGWorldRecord],
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) screen_clut: &'a mut [[u16; 3]; 256],
    pub(super) color_manager_clut: &'a mut [[u16; 3]; 256],
}

pub(super) fn dispatch_display_import(
    context: PpcDisplayDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcDisplayDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        toolbox_startup,
        screen_clut,
        color_manager_clut,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::DMGetDisplayIDByGDevice => {
            let gdevice = cpu.gpr[3];
            let display_id_out_ptr = cpu.gpr[4];
            let fail_to_main = cpu.gpr[5] & 0xff != 0;
            // Apple's "Optimizing Display Modes and Window Buffers" (2007),
            // p. 26: DMGetDisplayIDByGDevice returns the long-lived display ID
            // associated with a video device. The failToMain Boolean requests
            // the main display when the supplied GDevice cannot be resolved.
            if display_id_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, display_id_out_ptr, 4)
            {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else if gdevice != PPC_MAIN_GDEVICE && !fail_to_main {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else {
                let _ = memory.write_u32_be(display_id_out_ptr, PPC_DSP_DISPLAY_ID);
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
            }
        }
        PpcImportDispatcherTarget::DMGetGDeviceByDisplayID => {
            let display_id = cpu.gpr[3];
            let gdevice_out_ptr = cpu.gpr[4];
            let fail_to_main = cpu.gpr[5] & 0xff != 0;
            if gdevice_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, gdevice_out_ptr, 4) {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else if display_id != PPC_DSP_DISPLAY_ID && !fail_to_main {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else {
                let _ = memory.write_u32_be(gdevice_out_ptr, PPC_MAIN_GDEVICE);
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
            }
        }
        PpcImportDispatcherTarget::DMGetFirstScreenDevice => {
            // Display Manager's device iterator exposes the single active
            // screen modeled by this runtime (the same policy as GetDeviceList).
            Some(PpcImportAction::Return(PPC_MAIN_GDEVICE))
        }
        PpcImportDispatcherTarget::DMGetNextScreenDevice => Some(PpcImportAction::Return(0)),
        PpcImportDispatcherTarget::DMGetDisplayMode => {
            let switch_info = cpu.gpr[4];
            let result = ppc_dm_live_display_mode(memory, cpu.gpr[3])
                .filter(|_| switch_info != 0 && ppc_memory_can_write_bytes(memory, switch_info, 16))
                .and_then(|mode| ppc_dm_write_switch_info(memory, switch_info, mode))
                .map_or(PPC_PARAM_ERR, |_| PPC_NO_ERR);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DMCheckDisplayMode => {
            let switch_flags = cpu.gpr[6];
            let reserved = cpu.gpr[7];
            let mode_ok = cpu.gpr[8];
            // Universal Interfaces `Displays.h` defines the intervening
            // UInt32 as reserved; callers must pass zero.
            let result = if reserved != 0
                || switch_flags == 0
                || mode_ok == 0
                || !ppc_memory_can_write_bytes(memory, switch_flags, 4)
                || !ppc_memory_can_write_bytes(memory, mode_ok, 1)
            {
                PPC_PARAM_ERR
            } else if ppc_main_gdevice_record_for_depth(memory, cpu.gpr[3]).is_some() {
                let supported = ppc_dm_geometry(cpu.gpr[4]).is_some()
                    && crate::display::classic_pixel_size(cpu.gpr[5] as u16).is_some();
                let flags = if supported {
                    PPC_DM_NO_SWITCH_CONFIRM_MASK
                } else {
                    PPC_DM_DEPTH_NOT_AVAILABLE_MASK
                };
                let _ = memory.write_u32_be(switch_flags, flags);
                let _ = memory.write_u8(mode_ok, u8::from(supported));
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DMSetDisplayMode => {
            let depth_mode = cpu.gpr[5];
            let reserved = cpu.gpr[7];
            // Display Manager 2.0 accepts an opaque display-state value in
            // r6. The single-display HLE keeps the restorable state in its
            // live GDevice records, so it need not dereference that value.
            let result = if reserved != 0
                || depth_mode == 0
                || !ppc_memory_can_write_bytes(memory, depth_mode, 4)
            {
                PPC_PARAM_ERR
            } else if ppc_main_gdevice_record_for_depth(memory, cpu.gpr[3]).is_none() {
                PPC_PARAM_ERR
            } else if ppc_dm_geometry(cpu.gpr[4]).is_none() {
                PPC_DM_MODE_NOT_FOUND_ERR
            } else if let Some(requested_depth_mode) = memory.read_u32_be(depth_mode) {
                if crate::display::classic_pixel_size(requested_depth_mode as u16).is_none() {
                    PPC_DM_MODE_NOT_FOUND_ERR
                } else {
                    let mut set_depth_cpu = cpu.clone();
                    set_depth_cpu.gpr[4] = requested_depth_mode;
                    set_depth_cpu.gpr[5] = 0;
                    set_depth_cpu.gpr[6] = 0;
                    let mut allocator = PpcProcessAllocatorView {
                        memory_manager: process_memory_manager,
                    };
                    let result = ppc_set_depth_with_geometry(
                        &set_depth_cpu,
                        Some(&mut allocator),
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        gworlds,
                        toolbox_startup,
                        screen_clut,
                        color_manager_clut,
                        ppc_dm_geometry(cpu.gpr[4]),
                    );
                    result
                }
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DMNewDisplayModeList => {
            let result = ppc_dm_new_display_mode_list(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DMGetIndexedDisplayModeFromList => {
            ppc_dm_get_indexed_display_mode(cpu, memory)
        }
        PpcImportDispatcherTarget::DMDisposeList => {
            let list = cpu.gpr[3];
            let result = if process_memory_manager.native_ptr_size(list) != 0
                && memory.read_u32_be(list) == Some(PPC_DM_MODE_LIST_MAGIC)
            {
                let _ = process_memory_manager.dispose_native_ptr(list);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DMBeginConfigureDisplays => {
            let display_state_out = cpu.gpr[3];
            let result = if display_state_out != 0
                && memory
                    .write_u32_be(display_state_out, PPC_MAIN_GDEVICE)
                    .is_some()
            {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DMEndConfigureDisplays => {
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        _ => None,
    }
}

pub(super) fn ppc_dm_live_display_mode(
    memory: &mut PpcSectionMem,
    requested_gdevice: u32,
) -> Option<PpcDmLiveDisplayMode> {
    let gdevice = ppc_main_gdevice_record_for_depth(memory, requested_gdevice)?;
    let pixmap_handle = memory
        .read_u32_be(gdevice + 22)
        .filter(|handle| *handle != 0)?;
    let pixmap = memory
        .read_u32_be(pixmap_handle)
        .filter(|pixmap| *pixmap != 0)?;
    let bounds = ppc_read_rect(memory, pixmap + 6)?;
    let width = u32::try_from(i32::from(bounds.3) - i32::from(bounds.1)).ok()?;
    let height = u32::try_from(i32::from(bounds.2) - i32::from(bounds.0)).ok()?;
    let row_bytes = memory.read_u16_be(pixmap + 4)? & 0x3fff;
    let pixel_type = memory.read_u16_be(pixmap + 30)?;
    let pixel_size = memory.read_u16_be(pixmap + 32)?;
    let component_count = memory.read_u16_be(pixmap + 34)?;
    let component_size = memory.read_u16_be(pixmap + 36)?;
    let format_is_supported = match pixel_size {
        1 | 2 | 4 | 8 => {
            pixel_type == 0
                && component_count == 1
                && component_size == pixel_size
                && memory
                    .read_u32_be(pixmap + 42)
                    .is_some_and(|table| table != 0)
        }
        16 => pixel_type == 16 && component_count == 3 && component_size == 5,
        _ => false,
    };
    if width == 0 || height == 0 || row_bytes == 0 || !format_is_supported {
        return None;
    }
    let depth_mode = u32::from(crate::display::classic_depth_mode(pixel_size)?);
    if memory.read_u32_be(gdevice + 42)? != depth_mode {
        return None;
    }

    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-46--4-47 and
    // 5-15--5-18: gdPMap describes the live screen buffer's dimensions,
    // storage format, and depth, while gdMode is the device's current depth
    // mode. Keep the Display Manager view derived from those live records.
    Some(PpcDmLiveDisplayMode {
        display_mode_id: ppc_dm_mode_id_for_geometry(width, height),
        depth_mode,
        base_addr: memory.read_u32_be(pixmap)?,
        row_bytes,
        bounds,
        width,
        height,
        pm_version: memory.read_u16_be(pixmap + 14)?,
        pack_type: memory.read_u16_be(pixmap + 16)?,
        pack_size: memory.read_u32_be(pixmap + 18)?,
        h_res: memory.read_u32_be(pixmap + 22)?,
        v_res: memory.read_u32_be(pixmap + 26)?,
        pixel_type,
        pixel_size,
        component_count,
        component_size,
        plane_bytes: memory.read_u32_be(pixmap + 38)?,
    })
}

pub(super) fn ppc_dm_write_switch_info(
    memory: &mut PpcSectionMem,
    switch_info: u32,
    mode: PpcDmLiveDisplayMode,
) -> Option<()> {
    // Universal Interfaces 3.4.1 Video.h defines VDSwitchInfoRec as the
    // depth mode, timing mode, page, base address, and a zero reserved field.
    memory.write_u16_be(switch_info, mode.depth_mode as u16)?;
    memory.write_u32_be(switch_info + 2, mode.display_mode_id)?;
    memory.write_u16_be(switch_info + 6, 0)?;
    memory.write_u32_be(switch_info + 8, mode.base_addr)?;
    memory.write_u32_be(switch_info + 12, 0)?;
    Some(())
}

pub(super) fn ppc_dm_mode_at_depth(
    mode: PpcDmLiveDisplayMode,
    depth: u16,
) -> Option<PpcDmLiveDisplayMode> {
    let (pixel_type, component_count, component_size) = match depth {
        1 | 2 | 4 | 8 => (0, 1, depth),
        16 => (16, 3, 5),
        _ => return None,
    };
    Some(PpcDmLiveDisplayMode {
        depth_mode: u32::from(crate::display::classic_depth_mode(depth)?),
        row_bytes: u16::try_from(ppc_row_bytes(mode.width, u32::from(depth))?).ok()?,
        pixel_type,
        pixel_size: depth,
        component_count,
        component_size,
        ..mode
    })
}

fn ppc_dm_mode_at_geometry(
    mode: PpcDmLiveDisplayMode,
    display_mode_id: u32,
    width: u32,
    height: u32,
) -> Option<PpcDmLiveDisplayMode> {
    let row_bytes = u16::try_from(ppc_row_bytes(width, u32::from(mode.pixel_size))?).ok()?;
    Some(PpcDmLiveDisplayMode {
        display_mode_id,
        row_bytes,
        bounds: (
            0,
            0,
            i16::try_from(height).ok()?,
            i16::try_from(width).ok()?,
        ),
        width,
        height,
        ..mode
    })
}

pub(super) fn ppc_dm_write_vp_block(
    memory: &mut PpcSectionMem,
    vp_block: u32,
    mode: PpcDmLiveDisplayMode,
) -> Option<()> {
    memory.write_u32_be(vp_block, 0)?;
    memory.write_u16_be(vp_block + 4, mode.row_bytes)?;
    ppc_write_rect(
        memory,
        vp_block + 6,
        mode.bounds.0,
        mode.bounds.1,
        mode.bounds.2,
        mode.bounds.3,
    )?;
    memory.write_u16_be(vp_block + 14, mode.pm_version)?;
    memory.write_u16_be(vp_block + 16, mode.pack_type)?;
    memory.write_u32_be(vp_block + 18, mode.pack_size)?;
    memory.write_u32_be(vp_block + 22, mode.h_res)?;
    memory.write_u32_be(vp_block + 26, mode.v_res)?;
    memory.write_u16_be(vp_block + 30, mode.pixel_type)?;
    memory.write_u16_be(vp_block + 32, mode.pixel_size)?;
    memory.write_u16_be(vp_block + 34, mode.component_count)?;
    memory.write_u16_be(vp_block + 36, mode.component_size)?;
    memory.write_u32_be(vp_block + 38, mode.plane_bytes)?;
    Some(())
}

pub(super) fn ppc_dm_new_display_mode_list(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> i16 {
    ppc_dm_new_display_mode_list_values(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        cpu.gpr[3],
        cpu.gpr[6],
        cpu.gpr[7],
    )
}

pub(super) fn ppc_dm_new_display_mode_list_values(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    display_id: u32,
    count_out: u32,
    list_out: u32,
) -> i16 {
    if count_out == 0
        || list_out == 0
        || display_id != PPC_DSP_DISPLAY_ID
        || !ppc_memory_can_write_bytes(memory, count_out, 4)
        || !ppc_memory_can_write_bytes(memory, list_out, 4)
    {
        return PPC_PARAM_ERR;
    }
    let Some(live_mode) = ppc_dm_live_display_mode(memory, PPC_MAIN_GDEVICE) else {
        return PPC_PARAM_ERR;
    };
    const DEPTHS: [u16; 5] = [1, 2, 4, 8, 16];
    let mut geometries = Vec::new();
    for (display_mode_id, width, height) in PPC_DM_GEOMETRIES {
        let (width, height) = if width == 0 {
            (ppc_main_screen_width(), ppc_main_screen_height())
        } else {
            (width, height)
        };
        if !geometries
            .iter()
            .any(|(_, candidate_width, candidate_height)| {
                *candidate_width == width && *candidate_height == height
            })
        {
            geometries.push((display_mode_id, width, height));
        }
    }
    let list = process_memory_manager.new_native_ptr(memory, PPC_DM_MODE_LIST_SIZE, true);
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
    if list == 0 {
        return PPC_MEM_FULL_ERR;
    }
    // Universal Interfaces 3.4.1 Displays.h defines one
    // DMDisplayModeListEntryRec per timing and one DMDepthInfoRec per
    // supported depth. Expose the compact Macintosh geometry, the common
    // 13-inch geometry, and the user-selected native geometry. Applications
    // can therefore choose a compatible logical screen without a title-
    // specific host heuristic, while the native/user override remains active
    // until a guest explicitly requests another advertised mode.
    let _ = memory.write_u32_be(list, PPC_DM_MODE_LIST_MAGIC);
    let _ = memory.write_u32_be(list + 4, display_id);
    let _ = memory.write_u32_be(list + 8, geometries.len() as u32);
    for (geometry_index, (display_mode_id, width, height)) in geometries.iter().copied().enumerate()
    {
        let entry_base = list + geometry_index as u32 * PPC_DM_MODE_LIST_ENTRY_STRIDE;
        let entry = entry_base + PPC_DM_MODE_LIST_ENTRY_OFFSET;
        let resolution = entry_base + PPC_DM_MODE_LIST_RESOLUTION_INFO_OFFSET;
        let timing = entry_base + PPC_DM_MODE_LIST_TIMING_INFO_OFFSET;
        let depth_block = entry_base + PPC_DM_MODE_LIST_DEPTH_BLOCK_OFFSET;
        let name = entry_base + PPC_DM_MODE_LIST_NAME_OFFSET;
        let Some(geometry_mode) =
            ppc_dm_mode_at_geometry(live_mode, display_mode_id, width, height)
        else {
            let _ = process_memory_manager.dispose_native_ptr(list);
            return PPC_PARAM_ERR;
        };
        let Some(modes) = DEPTHS
            .map(|depth| ppc_dm_mode_at_depth(geometry_mode, depth))
            .into_iter()
            .collect::<Option<Vec<_>>>()
        else {
            let _ = process_memory_manager.dispose_native_ptr(list);
            return PPC_PARAM_ERR;
        };
        let live_depth_index = DEPTHS
            .iter()
            .position(|depth| *depth == live_mode.pixel_size)
            .unwrap_or(0) as u32;
        let _ = memory.write_u32_be(entry, 0);
        let _ = memory.write_u32_be(
            entry + 4,
            entry_base + PPC_DM_MODE_LIST_SWITCH_INFO_OFFSET + live_depth_index * 16,
        );
        let _ = memory.write_u32_be(entry + 8, resolution);
        let _ = memory.write_u32_be(entry + 12, timing);
        let _ = memory.write_u32_be(entry + 16, depth_block);
        let _ = memory.write_u32_be(entry + 20, DEPTHS.len() as u32);
        let _ = memory.write_u32_be(entry + 24, name);
        let _ = memory.write_u32_be(entry + 28, 0);

        for (index, mode) in modes.iter().copied().enumerate() {
            let offset = index as u32;
            let switch_info = entry_base + PPC_DM_MODE_LIST_SWITCH_INFO_OFFSET + offset * 16;
            let depth_info = entry_base + PPC_DM_MODE_LIST_DEPTH_INFO_OFFSET + offset * 20;
            let vp_block = entry_base + PPC_DM_MODE_LIST_VP_BLOCK_OFFSET + offset * 42;
            let _ = ppc_dm_write_switch_info(memory, switch_info, mode);
            let _ = memory.write_u32_be(depth_info, switch_info);
            let _ = memory.write_u32_be(depth_info + 4, vp_block);
            let _ = memory.write_u32_be(depth_info + 8, 0);
            let _ = memory.write_u32_be(depth_info + 12, 0);
            let _ = memory.write_u32_be(depth_info + 16, 0);
            let _ = ppc_dm_write_vp_block(memory, vp_block, mode);
        }

        let _ = memory.write_u32_be(resolution, 0);
        let _ = memory.write_u32_be(resolution + 4, display_mode_id);
        let _ = memory.write_u32_be(resolution + 8, width);
        let _ = memory.write_u32_be(resolution + 12, height);
        let _ = memory.write_u32_be(resolution + 16, 60 << 16);
        let _ = memory.write_u32_be(resolution + 20, geometry_mode.depth_mode);
        let _ = memory.write_u32_be(resolution + 24, 0);
        let _ = memory.write_u32_be(resolution + 28, 0);

        let _ = memory.write_u32_be(timing, display_mode_id);
        let _ = memory.write_u32_be(timing + 4, 0);
        let _ = memory.write_u32_be(timing + 8, 0);
        let _ = memory.write_u32_be(timing + 12, 0);
        let _ = memory.write_u32_be(timing + 16, 0b111);

        let _ = memory.write_u32_be(depth_block, DEPTHS.len() as u32);
        let _ = memory.write_u32_be(
            depth_block + 4,
            entry_base + PPC_DM_MODE_LIST_DEPTH_INFO_OFFSET,
        );
        let _ = memory.write_u32_be(depth_block + 8, 0);
        let _ = memory.write_u32_be(depth_block + 12, 0);
        let _ = memory.write_u32_be(depth_block + 16, 0);
        // Video.h VPBlock is 68K-aligned even for CFM clients.
        let mode_name = format!("{} x {}", width, height);
        let _ = ppc_write_pstring_bytes(memory, name, mode_name.as_bytes());
    }
    let _ = memory.write_u32_be(count_out, geometries.len() as u32);
    let _ = memory.write_u32_be(list_out, list);
    PPC_NO_ERR
}

pub(super) fn ppc_dm_get_indexed_display_mode(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> Option<PpcImportAction> {
    ppc_dm_get_indexed_display_mode_values(
        cpu, memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[6], cpu.gpr[7],
    )
}

pub(super) fn ppc_dm_get_indexed_display_mode_values(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    list: u32,
    item_index: u32,
    callback: u32,
    user_data: u32,
) -> Option<PpcImportAction> {
    if memory.read_u32_be(list) != Some(PPC_DM_MODE_LIST_MAGIC)
        || memory
            .read_u32_be(list + 8)
            .is_none_or(|count| item_index >= count)
        || callback == 0
    {
        return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
    }
    let restore_rtoc = cpu.gpr[2];
    let final_pc = cpu.lr;
    let target = ppc_resolve_callback_target(memory, callback, restore_rtoc, None)?;
    install_powerpc_call_arguments(
        cpu,
        memory,
        &[
            user_data,
            item_index,
            list + item_index * PPC_DM_MODE_LIST_ENTRY_STRIDE + PPC_DM_MODE_LIST_ENTRY_OFFSET,
        ],
    )?;
    Some(
        GuestCallEffect::call_guest(
            GuestCallRequest::new(GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: target.entry,
                rtoc: target.rtoc,
            }),
            GuestCallContinuation::to_powerpc(
                PPC_GUEST_CALL_RETURN_PC,
                final_pc,
                restore_rtoc,
                PpcNativeReturnGpr3::Set(ppc_i16_result(PPC_NO_ERR)),
            ),
        )
        .into_ppc_import_action()?,
    )
}
