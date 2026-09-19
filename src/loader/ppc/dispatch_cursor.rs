use super::*;

pub(super) struct PpcCursorDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(super) current_resource_refnum: i16,
    pub(super) last_resource_error: &'a mut i16,
    pub(super) cursor_state: &'a SharedProcessCursorState,
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) current_gworld: u32,
    pub(super) screen_clut: &'a [[u16; 3]; 256],
}

pub(super) fn dispatch_cursor_import(
    context: PpcCursorDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcCursorDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        current_resource_refnum,
        last_resource_error,
        cursor_state,
        gworlds,
        current_gworld,
        screen_clut,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InitCursor => {
            cursor_state.init();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HideCursor => {
            cursor_state.hide();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ShowCursor => {
            cursor_state.show();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ShieldCursor => {
            // ShieldCursor decrements the cursor level and must be balanced by
            // ShowCursor. The mouse driver decides whether the shield rectangle
            // currently requires erasing the cursor.
            // Imaging With QuickDraw (1994), p. 8-29.
            cursor_state.hide();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CrsrDevNextDevice => {
            // The reference machine exposes a single aggregate mouse through
            // the Event Manager, not a Cursor Device Manager device chain.
            if cpu.gpr[3] != 0 && ppc_memory_can_write_bytes(memory, cpu.gpr[3], 4) {
                let _ = memory.write_u32_be(cpu.gpr[3], 0);
            }
            Some(PpcImportAction::Return(ppc_i16_result(-1)))
        }
        PpcImportDispatcherTarget::CrsrDevMoveTo => {
            Some(PpcImportAction::Return(ppc_i16_result(-1)))
        }
        PpcImportDispatcherTarget::GetCursor => Some(PpcImportAction::Return(ppc_get_cursor(
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
            last_resource_error,
        ))),
        PpcImportDispatcherTarget::SetCursor => {
            ppc_set_cursor(memory, cpu.gpr[3], cursor_state);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetCCursor => Some(PpcImportAction::Return(ppc_get_ccursor(
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
            last_resource_error,
        ))),
        PpcImportDispatcherTarget::GetCIcon => Some(PpcImportAction::Return(ppc_get_cicon(
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
            last_resource_error,
        ))),
        PpcImportDispatcherTarget::PlotCIcon => {
            let _ = ppc_plot_cicon(cpu, memory, gworlds, current_gworld, screen_clut);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DisposeCIcon => {
            ppc_dispose_cicon(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetCCursor | PpcImportDispatcherTarget::DisposeCCursor => {
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}

fn ppc_get_ccursor(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> u32 {
    let crsr_id = cpu.gpr[3] as u16 as i16;
    let res_type = u32::from_be_bytes(*b"crsr");
    let Some(index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        res_type,
        crsr_id,
        false,
    ) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    ppc_materialize_vfs_resource_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        index,
        true,
        last_resource_error,
    )
}

fn ppc_get_cicon(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> u32 {
    let icon_id = cpu.gpr[3] as u16 as i16;
    let icon_type = u32::from_be_bytes(*b"cicn");
    let Some(index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        icon_type,
        icon_id,
        false,
    ) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] GetCIcon({}) current_ref={} -> NULL err={}",
                icon_id, current_resource_refnum, PPC_RES_NOT_FOUND_ERR
            );
        }
        return 0;
    };
    let data = &vfs_resources[index].data;
    let read_u16 = |offset: usize| {
        let bytes: [u8; 2] = data.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
        Some(u16::from_be_bytes(bytes))
    };
    let Some(pm_top) = read_u16(6).map(|value| value as i16) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Some(pm_bottom) = read_u16(10).map(|value| value as i16) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let icon_height = i32::from(pm_bottom) - i32::from(pm_top);
    let Some(mask_row_bytes) = read_u16(54).map(|value| usize::from(value & 0x3fff)) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Some(bitmap_row_bytes) = read_u16(68).map(|value| usize::from(value & 0x3fff)) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Ok(icon_height) = usize::try_from(icon_height) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Some(mask_size) = mask_row_bytes.checked_mul(icon_height) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Some(bitmap_size) = bitmap_row_bytes.checked_mul(icon_height) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Some(color_table_offset) = 82usize
        .checked_add(mask_size)
        .and_then(|offset| offset.checked_add(bitmap_size))
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Some(color_table_size) = read_u16(color_table_offset + 6)
        .and_then(|size| usize::from(size).checked_add(1))
        .and_then(|count| count.checked_mul(8))
        .and_then(|entries| entries.checked_add(8))
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Some(pixel_data_offset) = color_table_offset.checked_add(color_table_size) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let (Some(color_table), Some(pixel_data)) = (
        data.get(color_table_offset..pixel_data_offset),
        data.get(pixel_data_offset..),
    ) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };

    // Imaging With QuickDraw (1994), pp. 4-105--4-106: the compiled cicn
    // stores its mask, monochrome bitmap, color table, and pixels inline.
    // GetCIcon returns a private live CIcon with address/Handle fields fixed
    // up to independently owned copies.
    let icon_handle = ppc_process_alloc_handle_with_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        data,
    );
    let color_table_handle = ppc_process_alloc_handle_with_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        color_table,
    );
    let pixel_data_handle = ppc_process_alloc_handle_with_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        pixel_data,
    );
    if icon_handle == 0 || color_table_handle == 0 || pixel_data_handle == 0 {
        let mut allocator = PpcProcessAllocatorView {
            memory_manager: process_memory_manager,
        };
        for handle in [icon_handle, color_table_handle, pixel_data_handle] {
            let _ = allocator.dispose_handle(
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                handle,
            );
        }
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    let Some(icon_ptr) = memory.read_u32_be(icon_handle) else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    };
    let Some(pixel_data_ptr) = memory.read_u32_be(pixel_data_handle) else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    };
    let mask_data_ptr = icon_ptr + 82;
    let bitmap_data_ptr = mask_data_ptr + mask_size as u32;
    let patched = memory.write_u32_be(icon_ptr, pixel_data_ptr).is_some()
        && memory
            .write_u32_be(icon_ptr + 42, color_table_handle)
            .is_some()
        && memory.write_u32_be(icon_ptr + 50, mask_data_ptr).is_some()
        && memory
            .write_u32_be(icon_ptr + 64, bitmap_data_ptr)
            .is_some()
        && memory
            .write_u32_be(icon_ptr + 78, pixel_data_handle)
            .is_some();
    if !patched {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    *last_mem_error = PPC_NO_ERR;
    *last_resource_error = PPC_NO_ERR;
    if ppc_hle_trace_enabled() {
        let record = &vfs_resources[index];
        eprintln!(
            "[PPC-TRACE] GetCIcon({}) current_ref={} -> handle=${:08X} home_ref={} path=\"{}\" size={} pixel_size={}",
            icon_id,
            current_resource_refnum,
            icon_handle,
            record.ref_num,
            record.path,
            record.data.len(),
            pixel_data.len()
        );
    }
    icon_handle
}

fn ppc_cicon_color(
    memory: &mut PpcSectionMem,
    color_table_handle: u32,
    index: u8,
) -> Option<PpcRgbColor> {
    let color_table = memory.read_u32_be(color_table_handle)?;
    let count = u32::from(memory.read_u16_be(color_table + 6)?).checked_add(1)?;
    let mut ordinal = None;
    for entry_index in 0..count.min(256) {
        let entry = color_table.checked_add(8 + entry_index * 8)?;
        let value = memory.read_u16_be(entry)?;
        if entry_index == u32::from(index) {
            ordinal = Some(entry);
        }
        if value == u16::from(index) {
            return Some(PpcRgbColor {
                red: memory.read_u16_be(entry + 2)?,
                green: memory.read_u16_be(entry + 4)?,
                blue: memory.read_u16_be(entry + 6)?,
            });
        }
    }
    let entry = ordinal?;
    Some(PpcRgbColor {
        red: memory.read_u16_be(entry + 2)?,
        green: memory.read_u16_be(entry + 4)?,
        blue: memory.read_u16_be(entry + 6)?,
    })
}

fn ppc_scale_icon_coordinate(
    dst_coordinate: i16,
    dst_start: i16,
    dst_end: i16,
    src_start: i16,
    src_end: i16,
) -> Option<i16> {
    let dst_size = i32::from(dst_end) - i32::from(dst_start);
    let src_size = i32::from(src_end) - i32::from(src_start);
    if dst_size <= 0 || src_size <= 0 {
        return None;
    }
    let dst_offset = i32::from(dst_coordinate) - i32::from(dst_start);
    i16::try_from(i32::from(src_start) + dst_offset * src_size / dst_size).ok()
}

fn ppc_plot_cicon(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    _screen_clut: &[[u16; 3]; 256],
) -> bool {
    let rect_ptr = cpu.gpr[3];
    let icon_handle = cpu.gpr[4];
    let (Some(port_dst_rect), Some(icon_ptr)) = (
        ppc_read_rect(memory, rect_ptr),
        memory.read_u32_be(icon_handle).filter(|ptr| *ptr != 0),
    ) else {
        return false;
    };
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    let (dst_top, dst_left, dst_bottom, dst_right) = surface.local_rect_i16(port_dst_rect);
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }
    let Some(pixel_base) = memory.read_u32_be(icon_ptr) else {
        return false;
    };
    let Some(pixel_row_bytes) = memory
        .read_u16_be(icon_ptr + 4)
        .map(|value| u32::from(value & 0x3fff))
    else {
        return false;
    };
    let pixel_bounds = (
        memory.read_u16_be(icon_ptr + 6).unwrap_or(0) as i16,
        memory.read_u16_be(icon_ptr + 8).unwrap_or(0) as i16,
        memory.read_u16_be(icon_ptr + 10).unwrap_or(0) as i16,
        memory.read_u16_be(icon_ptr + 12).unwrap_or(0) as i16,
    );
    let pixel_size = memory.read_u16_be(icon_ptr + 32).unwrap_or(0);
    let color_table_handle = memory.read_u32_be(icon_ptr + 42).unwrap_or(0);
    let mask_base = memory.read_u32_be(icon_ptr + 50).unwrap_or(0);
    let mask_row_bytes = u32::from(memory.read_u16_be(icon_ptr + 54).unwrap_or(0) & 0x3fff);
    let mask_bounds = (
        memory.read_u16_be(icon_ptr + 56).unwrap_or(0) as i16,
        memory.read_u16_be(icon_ptr + 58).unwrap_or(0) as i16,
        memory.read_u16_be(icon_ptr + 60).unwrap_or(0) as i16,
        memory.read_u16_be(icon_ptr + 62).unwrap_or(0) as i16,
    );
    if dst_bottom <= dst_top
        || dst_right <= dst_left
        || pixel_bounds.2 <= pixel_bounds.0
        || pixel_bounds.3 <= pixel_bounds.1
        || mask_bounds.2 <= mask_bounds.0
        || mask_bounds.3 <= mask_bounds.1
    {
        return false;
    }

    // More Macintosh Toolbox (1993), pp. 5-25--5-26: PlotCIcon scales the
    // color PixMap into the destination rectangle and applies the icon mask.
    let mut drew = false;
    for dst_v in dst_top..dst_bottom {
        let Some(src_v) =
            ppc_scale_icon_coordinate(dst_v, dst_top, dst_bottom, pixel_bounds.0, pixel_bounds.2)
        else {
            continue;
        };
        let Some(mask_v) =
            ppc_scale_icon_coordinate(dst_v, dst_top, dst_bottom, mask_bounds.0, mask_bounds.2)
        else {
            continue;
        };
        for dst_h in dst_left..dst_right {
            let Some(mask_h) =
                ppc_scale_icon_coordinate(dst_h, dst_left, dst_right, mask_bounds.1, mask_bounds.3)
            else {
                continue;
            };
            if ppc_read_packed_bitmap_index(
                memory,
                mask_base,
                mask_row_bytes,
                1,
                mask_bounds,
                mask_v,
                mask_h,
            ) != Some(1)
            {
                continue;
            }
            let Some(src_h) = ppc_scale_icon_coordinate(
                dst_h,
                dst_left,
                dst_right,
                pixel_bounds.1,
                pixel_bounds.3,
            ) else {
                continue;
            };
            let Some(index) = ppc_read_packed_bitmap_index(
                memory,
                pixel_base,
                pixel_row_bytes,
                pixel_size,
                pixel_bounds,
                src_v,
                src_h,
            ) else {
                continue;
            };
            let Some(color) = ppc_cicon_color(memory, color_table_handle, index) else {
                continue;
            };
            let Some(raw_pixel) = ppc_quickdraw_surface_color_pixel(memory, surface, color) else {
                continue;
            };
            drew |= ppc_quickdraw_write_raw_pixel(
                memory,
                front_buffer,
                (i32::from(dst_h), i32::from(dst_v)),
                raw_pixel,
            );
        }
    }
    if ppc_hle_trace_enabled() {
        let (port_top, port_left, port_bottom, port_right) = port_dst_rect;
        eprintln!(
            "[PPC-TRACE] PlotCIcon rect=({}, {}, {}, {}) local=({}, {}, {}, {}) icon=${:08X} depth={} drew={}",
            port_top,
            port_left,
            port_bottom,
            port_right,
            dst_top,
            dst_left,
            dst_bottom,
            dst_right,
            icon_handle,
            front_buffer.depth,
            drew
        );
    }
    drew
}

#[allow(clippy::too_many_arguments)]
fn ppc_dispose_cicon(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    let icon_handle = cpu.gpr[3];
    let icon_ptr = memory.read_u32_be(icon_handle).unwrap_or(0);
    let (color_table_handle, pixel_data_handle) = if icon_ptr == 0 {
        (0, 0)
    } else {
        (
            memory.read_u32_be(icon_ptr + 42).unwrap_or(0),
            memory.read_u32_be(icon_ptr + 78).unwrap_or(0),
        )
    };
    for handle in [color_table_handle, pixel_data_handle, icon_handle] {
        let _ = ppc_dispose_process_native_handle(
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            handle,
        );
    }
}

fn ppc_set_cursor(
    memory: &mut PpcSectionMem,
    cursor_ptr: u32,
    cursor_state: &SharedProcessCursorState,
) -> Option<()> {
    // SetCursor(crsr: Cursor) installs the 16-by-16 data and mask bitmaps plus
    // the Point hotspot stored at byte offset 64 in the 68-byte Cursor record.
    // Imaging With QuickDraw (1994), pp. 8-19 and 8-25.
    let mut data = [0; 32];
    let mut mask = [0; 32];
    memory.read_bytes_into(cursor_ptr, &mut data)?;
    memory.read_bytes_into(cursor_ptr.checked_add(32)?, &mut mask)?;
    let hot_v = memory.read_u16_be(cursor_ptr.checked_add(64)?)? as i16;
    let hot_h = memory.read_u16_be(cursor_ptr.checked_add(66)?)? as i16;

    cursor_state.install(crate::display::CursorImage::mono(data, mask, hot_v, hot_h));
    Some(())
}

fn ppc_get_cursor(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> u32 {
    let cursor_id = cpu.gpr[3] as u16 as i16;
    let res_type = u32::from_be_bytes(*b"CURS");
    let index = match ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        res_type,
        cursor_id,
        false,
    ) {
        Some(index) => index,
        None => {
            let Some((data, mask, hot_v, hot_h)) =
                crate::trap::TrapDispatcher::system_cursor(cursor_id)
            else {
                *last_resource_error = PPC_RES_NOT_FOUND_ERR;
                return 0;
            };
            let mut cursor = Vec::with_capacity(68);
            cursor.extend_from_slice(&data);
            cursor.extend_from_slice(&mask);
            cursor.extend_from_slice(&(hot_v as u16).to_be_bytes());
            cursor.extend_from_slice(&(hot_h as u16).to_be_bytes());
            vfs_resources.push(PpcVfsResourceRecord {
                ref_num: 0,
                path: "__system__/CURS".to_string(),
                res_type,
                res_id: cursor_id,
                name: Vec::new(),
                data: cursor,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
            vfs_resources.len() - 1
        }
    };
    ppc_materialize_vfs_resource_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        index,
        true,
        last_resource_error,
    )
}
