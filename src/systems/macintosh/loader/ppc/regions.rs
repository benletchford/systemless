//! PowerPC QuickDraw regions, polygons, and coordinate transformations.

use super::graphics::*;
use super::*;

pub(super) fn ppc_transform_port_point(
    memory: &mut PpcSectionMem,
    port: u32,
    point_ptr: u32,
    local_to_global: bool,
) -> Option<()> {
    if port == 0 || point_ptr == 0 {
        return None;
    }
    let pixmap_handle = memory.read_u32_be(port.checked_add(2)?)?;
    let pixmap = memory.read_u32_be(pixmap_handle)?;
    let (pixel_top, pixel_left, _, _) = ppc_read_rect(memory, pixmap.checked_add(6)?)?;
    let v = memory.read_u16_be(point_ptr)? as i16;
    let h = memory.read_u16_be(point_ptr.checked_add(2)?)? as i16;
    let (v, h) = if local_to_global {
        (v.wrapping_sub(pixel_top), h.wrapping_sub(pixel_left))
    } else {
        (v.wrapping_add(pixel_top), h.wrapping_add(pixel_left))
    };

    // Inside Macintosh: Imaging With QuickDraw (1994), Basic QuickDraw
    // pp. 2-9–2-10 and 2-51–2-52: the boundary rectangle's upper-left
    // coordinates are subtracted from a local point to convert it to global.
    memory.write_u16_be(point_ptr, v as u16)?;
    memory.write_u16_be(point_ptr + 2, h as u16)?;
    Some(())
}

pub(super) fn ppc_set_port_origin(memory: &mut PpcSectionMem, port: u32, h: i16, v: i16) -> Option<()> {
    if port == 0 {
        return None;
    }
    let pixmap_handle = memory.read_u32_be(port.checked_add(2)?)?;
    let pixmap = memory.read_u32_be(pixmap_handle)?;
    let (pixel_top, pixel_left, pixel_bottom, pixel_right) =
        ppc_read_rect(memory, pixmap.checked_add(6)?)?;
    let pixel_height = pixel_bottom.wrapping_sub(pixel_top);
    let pixel_width = pixel_right.wrapping_sub(pixel_left);
    ppc_write_rect(
        memory,
        pixmap + 6,
        v,
        h,
        v.wrapping_add(pixel_height),
        h.wrapping_add(pixel_width),
    )?;

    let (port_top, port_left, port_bottom, port_right) =
        ppc_read_rect(memory, port.checked_add(16)?)?;
    let port_height = port_bottom.wrapping_sub(port_top);
    let port_width = port_right.wrapping_sub(port_left);
    // Inside Macintosh: Imaging With QuickDraw (1994), Basic QuickDraw
    // p. 2-45: SetOrigin redefines the local coordinates of the portRect
    // without moving the underlying pixels or clipping region.
    ppc_write_rect(
        memory,
        port + 16,
        v,
        h,
        v.wrapping_add(port_height),
        h.wrapping_add(port_width),
    )
}

pub(super) fn ppc_open_region_include_point(startup: &mut PpcToolboxStartupState, h: i16, v: i16) {
    startup.open_region_bounds = Some(match startup.open_region_bounds {
        Some((top, left, bottom, right)) => (top.min(v), left.min(h), bottom.max(v), right.max(h)),
        None => (v, h, v, h),
    });
}

pub(super) fn ppc_open_picture_commands(
    startup: &mut PpcToolboxStartupState,
    current_gworld: u32,
) -> Option<&mut Vec<u8>> {
    startup
        .open_picture
        .as_mut()
        .filter(|(_, port, _, _)| *port == current_gworld)
        .map(|(_, _, _, commands)| commands)
}

pub(super) fn ppc_open_region_include_rows(
    startup: &mut PpcToolboxStartupState,
    rows_top: i16,
    rows: Vec<Vec<i16>>,
) {
    let Some(first) = rows.iter().position(|row| !row.is_empty()) else {
        return;
    };
    let Some(last) = rows.iter().rposition(|row| !row.is_empty()) else {
        return;
    };
    let shape_top = i32::from(rows_top) + first as i32;
    let shape_bottom = i32::from(rows_top) + last as i32 + 1;
    let (Ok(shape_top), Ok(shape_bottom)) = (i16::try_from(shape_top), i16::try_from(shape_bottom))
    else {
        return;
    };
    let shape_left = rows
        .iter()
        .filter_map(|row| row.first().copied())
        .min()
        .unwrap_or(0);
    let shape_right = rows
        .iter()
        .filter_map(|row| row.last().copied())
        .max()
        .unwrap_or(0);
    startup.open_region_bounds = Some(match startup.open_region_bounds {
        Some((top, left, bottom, right)) => (
            top.min(shape_top),
            left.min(shape_left),
            bottom.max(shape_bottom),
            right.max(shape_right),
        ),
        None => (shape_top, shape_left, shape_bottom, shape_right),
    });

    let (combined_top, combined_bottom) = startup
        .open_region_rows
        .as_ref()
        .map(|(top, existing)| {
            (
                (*top).min(rows_top),
                (i32::from(*top) + existing.len() as i32)
                    .max(i32::from(rows_top) + rows.len() as i32),
            )
        })
        .unwrap_or((rows_top, i32::from(rows_top) + rows.len() as i32));
    let Ok(combined_bottom) = i16::try_from(combined_bottom) else {
        return;
    };
    let Ok(combined_height) = usize::try_from(i32::from(combined_bottom) - i32::from(combined_top))
    else {
        return;
    };
    let previous = startup.open_region_rows.take();
    let mut combined = Vec::with_capacity(combined_height);
    for row_offset in 0..combined_height {
        let y = i32::from(combined_top) + row_offset as i32;
        let existing_row = previous
            .as_ref()
            .and_then(|(top, existing)| {
                usize::try_from(y - i32::from(*top))
                    .ok()
                    .and_then(|index| existing.get(index))
            })
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let new_row = usize::try_from(y - i32::from(rows_top))
            .ok()
            .and_then(|index| rows.get(index))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        combined.push(ppc_region_combine_rows(
            existing_row,
            new_row,
            PpcRegionBooleanOp::Xor,
        ));
    }
    startup.open_region_rows = Some((combined_top, combined));
}

pub(super) fn ppc_open_region_include_rect(
    startup: &mut PpcToolboxStartupState,
    memory: &mut PpcSectionMem,
    rect_ptr: u32,
) {
    let Some((rect_top, rect_left, rect_bottom, rect_right)) = ppc_read_rect(memory, rect_ptr)
    else {
        return;
    };
    let Ok(height) = usize::try_from(i32::from(rect_bottom) - i32::from(rect_top)) else {
        return;
    };
    ppc_open_region_include_rows(startup, rect_top, vec![vec![rect_left, rect_right]; height]);
}

pub(super) fn ppc_open_region_include_region(
    startup: &mut PpcToolboxStartupState,
    memory: &mut PpcSectionMem,
    region_handle: u32,
) {
    // Inside Macintosh: Imaging With QuickDraw, pp. 3-87–3-89: while a
    // region is open, drawing commands contribute to its collected outline.
    let Some(storage) = ppc_region_storage(memory, region_handle) else {
        return;
    };
    let Some((rect_top, _, rect_bottom, _)) = ppc_region_storage_bbox(&storage) else {
        return;
    };
    let Some(rows) = ppc_region_rows_for_band(&storage, rect_top, rect_bottom) else {
        return;
    };
    ppc_open_region_include_rows(startup, rect_top, rows);
}

pub(super) fn ppc_open_region_include_polygon(
    startup: &mut PpcToolboxStartupState,
    memory: &mut PpcSectionMem,
    polygon_handle: u32,
) {
    // Inside Macintosh: Imaging With QuickDraw, p. 3-82: framing a polygon
    // while a region is open adds the polygon outline to that region.
    let Some(ptr) = memory.read_u32_be(polygon_handle) else {
        return;
    };
    let Some((rect_top, rect_left, rect_bottom, rect_right)) = ppc_read_rect(memory, ptr + 2)
    else {
        return;
    };
    startup.open_region_bounds = Some(match startup.open_region_bounds {
        Some((top, left, bottom, right)) => (
            top.min(rect_top),
            left.min(rect_left),
            bottom.max(rect_bottom),
            right.max(rect_right),
        ),
        None => (rect_top, rect_left, rect_bottom, rect_right),
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_open_rgn(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    current_gworld: u32,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    mut legacy_handle_states: Option<&mut Vec<PpcHandleStateRecord>>,
    startup: &mut PpcToolboxStartupState,
) {
    if startup.open_region_save_handle != 0 {
        let _ = ppc_allocator_view_dispose_handle(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            legacy_handle_states.as_deref_mut(),
            startup.open_region_save_handle,
        );
    }
    let save_handle = ppc_allocator_view_new_rgn(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    if save_handle == 0 {
        startup.open_region_port = 0;
        startup.open_region_save_handle = 0;
        startup.open_region_bounds = None;
        startup.open_region_rows = None;
        return;
    }
    startup.open_region_port = current_gworld;
    startup.open_region_save_handle = save_handle;
    startup.open_region_bounds = None;
    startup.open_region_rows = None;

    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 3-88--3-89:
    // OpenRgn installs temporary rgnSave state in the current port and calls
    // HidePen so collected outlines are not also drawn to the screen.
    if current_gworld != 0 {
        let _ = memory.write_u32_be(
            current_gworld.wrapping_add(PPC_CGRAF_PORT_RGN_SAVE_OFFSET),
            save_handle,
        );
        if let Some(pn_vis) = memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_VIS_OFFSET))
            .map(|value| value as i16)
        {
            let _ = memory.write_u16_be(
                current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_VIS_OFFSET),
                pn_vis.saturating_sub(1) as u16,
            );
        }
    }
    *last_mem_error = PPC_NO_ERR;
}

pub(super) fn ppc_close_rgn(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    destination: u32,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    mut legacy_handle_states: Option<&mut Vec<PpcHandleStateRecord>>,
    startup: &mut PpcToolboxStartupState,
) {
    *last_mem_error = if let Some((top, rows)) = startup.open_region_rows.take() {
        ppc_region_storage_from_rows(top, &rows).map_or(PPC_PARAM_ERR, |storage| {
            ppc_write_region_storage(
                allocator.as_deref_mut(),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                destination,
                &storage,
            )
        })
    } else {
        match startup.open_region_bounds {
            Some((top, left, bottom, right)) if bottom > top && right > left => {
                if ppc_write_rgn_bbox(memory, destination, top, left, bottom, right).is_some() {
                    PPC_NO_ERR
                } else {
                    PPC_PARAM_ERR
                }
            }
            _ => {
                if ppc_set_empty_rgn(memory, destination).is_some() {
                    PPC_NO_ERR
                } else {
                    PPC_PARAM_ERR
                }
            }
        }
    };

    let port = startup.open_region_port;
    if port != 0 {
        let _ = memory.write_u32_be(port.wrapping_add(PPC_CGRAF_PORT_RGN_SAVE_OFFSET), 0);
        if let Some(pn_vis) = memory
            .read_u16_be(port.wrapping_add(PPC_CGRAF_PORT_PN_VIS_OFFSET))
            .map(|value| value as i16)
        {
            let _ = memory.write_u16_be(
                port.wrapping_add(PPC_CGRAF_PORT_PN_VIS_OFFSET),
                pn_vis.saturating_add(1) as u16,
            );
        }
    }
    if startup.open_region_save_handle != 0 {
        let _ = ppc_allocator_view_dispose_handle(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            legacy_handle_states.as_deref_mut(),
            startup.open_region_save_handle,
        );
    }
    startup.open_region_port = 0;
    startup.open_region_save_handle = 0;
    startup.open_region_bounds = None;
    startup.open_region_rows = None;
}

pub(super) fn ppc_bitmap_to_region(
    cpu: &PpcCpu,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> i16 {
    let region_handle = cpu.gpr[3];
    let bitmap = cpu.gpr[4];
    if ppc_rgn_ptr(memory, region_handle).is_none() || bitmap == 0 {
        return PPC_PARAM_ERR;
    }
    let (Some(base_addr), Some(raw_row_bytes), Some(bounds)) = (
        memory.read_u32_be(bitmap),
        memory.read_u16_be(bitmap + 4),
        ppc_read_rect(memory, bitmap + 6),
    ) else {
        return PPC_PARAM_ERR;
    };
    let row_bytes = u32::from(raw_row_bytes & 0x3fff);
    let pixel_size = if raw_row_bytes & 0x8000 != 0 {
        memory.read_u16_be(bitmap + 32).unwrap_or(0)
    } else {
        1
    };
    if pixel_size != 1 {
        return PPC_PIXMAP_TOO_DEEP_ERR;
    }
    let (top, left, bottom, right) = bounds;
    if bottom <= top || right <= left || row_bytes == 0 {
        return if ppc_set_empty_rgn(memory, region_handle).is_some() {
            PPC_NO_ERR
        } else {
            PPC_PARAM_ERR
        };
    }

    let mut rows = Vec::with_capacity((i32::from(bottom) - i32::from(top)) as usize);
    for v in top..bottom {
        let mut endpoints = Vec::new();
        let mut in_run = false;
        for h in left..right {
            let inked = ppc_read_packed_bitmap_index(memory, base_addr, row_bytes, 1, bounds, v, h)
                == Some(1);
            if inked != in_run {
                endpoints.push(h);
                in_run = inked;
            }
        }
        if in_run {
            endpoints.push(right);
        }
        rows.push(endpoints);
    }
    let Some(storage) = ppc_region_storage_from_rows(top, &rows) else {
        return PPC_RGN_TOO_BIG_ERR;
    };
    if storage.len() > usize::from(u16::MAX) {
        return PPC_RGN_TOO_BIG_ERR;
    }
    let result = ppc_write_region_storage(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        region_handle,
        &storage,
    );
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] BitMapToRegion bitmap=${:08X} base=${:08X} row_bytes={} bounds=({}, {}, {}, {}) region=${:08X} size={} result={}",
            bitmap,
            base_addr,
            row_bytes,
            top,
            left,
            bottom,
            right,
            region_handle,
            storage.len(),
            result
        );
    }
    result
}

#[cfg(test)]
pub(super) fn ppc_new_rgn(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let handle = ppc_alloc_handle(memory, heap_cursor, heap_limit, handles, 10, true);
    if handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    if ppc_set_empty_rgn(memory, handle).is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    *last_mem_error = PPC_NO_ERR;
    handle
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_process_new_rgn(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let handle = ppc_process_alloc_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        10,
        true,
    );
    if handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    if ppc_set_empty_rgn(memory, handle).is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    *last_mem_error = PPC_NO_ERR;
    handle
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_allocator_view_new_rgn(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let handle = ppc_allocator_view_allocate_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        10,
        true,
    );
    if handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    if ppc_set_empty_rgn(memory, handle).is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    *last_mem_error = PPC_NO_ERR;
    handle
}

pub(super) fn ppc_clip_rect(
    cpu: &PpcCpu,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    current_gworld: u32,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    // Inside Macintosh: Imaging With QuickDraw (1994), p. 2-42: ClipRect
    // replaces the current port's clipping-region contents while retaining
    // the region handle itself.
    let Some((top, left, bottom, right)) = ppc_read_rect(memory, cpu.gpr[3]) else {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    };
    let Some(clip_addr) = current_gworld.checked_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET) else {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    };
    let mut clip_rgn = memory.read_u32_be(clip_addr).unwrap_or(0);
    if ppc_rgn_ptr(memory, clip_rgn).is_none() {
        clip_rgn = ppc_allocator_view_new_rgn(
            allocator,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
        );
        if clip_rgn == 0 || memory.write_u32_be(clip_addr, clip_rgn).is_none() {
            *last_mem_error = PPC_PARAM_ERR;
            return;
        }
    }
    if ppc_write_rgn_bbox(memory, clip_rgn, top, left, bottom, right).is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    }
    *last_mem_error = PPC_NO_ERR;
}

pub(super) fn ppc_rgn_ptr(memory: &mut PpcSectionMem, rgn_handle: u32) -> Option<u32> {
    if rgn_handle == 0 {
        return None;
    }
    let ptr = memory.read_u32_be(rgn_handle)?;
    if ptr == 0 {
        None
    } else {
        Some(ptr)
    }
}

pub(super) fn ppc_write_rgn_bbox(
    memory: &mut PpcSectionMem,
    rgn_handle: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
) -> Option<()> {
    let ptr = ppc_rgn_ptr(memory, rgn_handle)?;
    memory.write_u16_be(ptr, 10)?;
    ppc_write_rect(memory, ptr + 2, top, left, bottom, right)
}

pub(super) fn ppc_read_rgn_bbox(memory: &mut PpcSectionMem, rgn_handle: u32) -> Option<(i16, i16, i16, i16)> {
    let ptr = ppc_rgn_ptr(memory, rgn_handle)?;
    ppc_read_rect(memory, ptr + 2)
}

pub(super) fn ppc_set_empty_rgn(memory: &mut PpcSectionMem, rgn_handle: u32) -> Option<()> {
    ppc_write_rgn_bbox(memory, rgn_handle, 0, 0, 0, 0)
}

pub(super) fn ppc_copy_rgn(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    src_rgn: u32,
    dst_rgn: u32,
) -> i16 {
    if src_rgn == dst_rgn {
        return if ppc_rgn_ptr(memory, src_rgn).is_some() {
            PPC_NO_ERR
        } else {
            PPC_PARAM_ERR
        };
    }
    let Some(src_ptr) = ppc_rgn_ptr(memory, src_rgn) else {
        return PPC_PARAM_ERR;
    };
    let Some(size) = memory.read_u16_be(src_ptr).map(u32::from) else {
        return PPC_PARAM_ERR;
    };
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 3-90–3-91:
    // CopyRgn duplicates the complete mathematical region into an existing
    // destination so later changes to either region remain independent.
    if size < 10 || !ppc_memory_can_read_bytes(memory, src_ptr, size) {
        return PPC_PARAM_ERR;
    }
    let bytes = (0..size)
        .map(|offset| memory.read_u8(src_ptr + offset))
        .collect::<Option<Vec<_>>>();
    let Some(bytes) = bytes else {
        return PPC_PARAM_ERR;
    };
    if !handles.iter().any(|record| record.handle == dst_rgn) {
        let Some(dst_ptr) = ppc_rgn_ptr(memory, dst_rgn) else {
            return PPC_PARAM_ERR;
        };
        let Some(dst_size) = memory.read_u16_be(dst_ptr).map(u32::from) else {
            return PPC_PARAM_ERR;
        };
        if dst_size < size || !ppc_memory_can_write_bytes(memory, dst_ptr, size) {
            return PPC_PARAM_ERR;
        }
        return if memory.write_bytes(dst_ptr, &bytes).is_some() {
            PPC_NO_ERR
        } else {
            PPC_PARAM_ERR
        };
    }
    let result = ppc_allocator_view_resize_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        dst_rgn,
        size,
    );
    if result != PPC_NO_ERR {
        return result;
    }
    let Some(dst_ptr) = ppc_rgn_ptr(memory, dst_rgn) else {
        return PPC_PARAM_ERR;
    };
    for (offset, byte) in bytes.into_iter().enumerate() {
        if memory.write_u8(dst_ptr + offset as u32, byte).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    PPC_NO_ERR
}

#[derive(Clone, Copy)]
pub(super) enum PpcRegionBooleanOp {
    Intersection,
    Union,
    Difference,
    Xor,
}

pub(super) fn ppc_region_storage(memory: &mut PpcSectionMem, rgn_handle: u32) -> Option<Vec<u8>> {
    let ptr = ppc_rgn_ptr(memory, rgn_handle)?;
    let size = u32::from(memory.read_u16_be(ptr)?);
    if size < 10 || !ppc_memory_can_read_bytes(memory, ptr, size) {
        return None;
    }
    (0..size)
        .map(|offset| memory.read_u8(ptr + offset))
        .collect()
}

/// The port's clipRgn storage, further intersected with `clip_rect` when the
/// caller imposes its own bound (TextEdit's viewRect). Imaging With QuickDraw
/// (1994), p. 3-94: an empty intersection is an empty region, which clips
/// every pixel.
pub(super) fn ppc_port_clip_storage(
    memory: &mut PpcSectionMem,
    port: u32,
    clip_rect: Option<(i16, i16, i16, i16)>,
) -> Option<Vec<u8>> {
    let clip_storage = memory
        .read_u32_be(port.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
        .and_then(|clip_rgn| ppc_region_storage(memory, clip_rgn));
    let Some((top, left, bottom, right)) = clip_rect else {
        return clip_storage;
    };
    const EMPTY_REGION: [u8; 10] = [0, 10, 0, 0, 0, 0, 0, 0, 0, 0];
    if bottom <= top || right <= left {
        return Some(EMPTY_REGION.to_vec());
    }
    let mut rect_storage = vec![0, 10];
    for word in [top, left, bottom, right] {
        rect_storage.extend_from_slice(&word.to_be_bytes());
    }
    let Some(clip_storage) = clip_storage else {
        return Some(rect_storage);
    };
    let Some(bbox) = ppc_region_storage_bbox(&clip_storage) else {
        return Some(EMPTY_REGION.to_vec());
    };
    let (band_top, band_bottom) = (bbox.0.max(top), bbox.2.min(bottom));
    if band_bottom <= band_top {
        return Some(EMPTY_REGION.to_vec());
    }
    let (Some(clip_rows), Some(rect_rows)) = (
        ppc_region_rows_for_band(&clip_storage, band_top, band_bottom),
        ppc_region_rows_for_band(&rect_storage, band_top, band_bottom),
    ) else {
        return Some(clip_storage);
    };
    let rows = clip_rows
        .iter()
        .zip(&rect_rows)
        .map(|(clip, rect)| ppc_region_intersect_rows(clip, rect))
        .collect::<Vec<_>>();
    Some(ppc_region_storage_from_rows(band_top, &rows).unwrap_or_else(|| EMPTY_REGION.to_vec()))
}

pub(super) fn ppc_region_storage_bbox(storage: &[u8]) -> Option<(i16, i16, i16, i16)> {
    if storage.len() < 10 {
        return None;
    }
    let word = |offset: usize| i16::from_be_bytes([storage[offset], storage[offset + 1]]);
    let bbox = (word(2), word(4), word(6), word(8));
    (bbox.2 > bbox.0 && bbox.3 > bbox.1).then_some(bbox)
}

pub(super) fn ppc_region_merge_endpoints(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
    let mut merged = Vec::with_capacity(lhs.len() + rhs.len());
    let mut lhs_index = 0usize;
    let mut rhs_index = 0usize;
    while lhs_index < lhs.len() || rhs_index < rhs.len() {
        match (lhs.get(lhs_index), rhs.get(rhs_index)) {
            (Some(&lhs_value), Some(&rhs_value)) if lhs_value < rhs_value => {
                merged.push(lhs_value);
                lhs_index += 1;
            }
            (Some(&lhs_value), Some(&rhs_value)) if rhs_value < lhs_value => {
                merged.push(rhs_value);
                rhs_index += 1;
            }
            (Some(_), Some(_)) => {
                lhs_index += 1;
                rhs_index += 1;
            }
            (Some(&lhs_value), None) => {
                merged.push(lhs_value);
                lhs_index += 1;
            }
            (None, Some(&rhs_value)) => {
                merged.push(rhs_value);
                rhs_index += 1;
            }
            (None, None) => break,
        }
    }
    merged
}

pub(super) fn ppc_region_rows_for_band(storage: &[u8], top: i16, bottom: i16) -> Option<Vec<Vec<i16>>> {
    let height = (i32::from(bottom) - i32::from(top)).max(0) as usize;
    let mut rows = vec![Vec::new(); height];
    let Some((rgn_top, rgn_left, rgn_bottom, rgn_right)) = ppc_region_storage_bbox(storage) else {
        return Some(rows);
    };
    let overlap_top = top.max(rgn_top);
    let overlap_bottom = bottom.min(rgn_bottom);
    if overlap_bottom <= overlap_top {
        return Some(rows);
    }
    if storage.len() == 10 {
        for y in overlap_top..overlap_bottom {
            rows[(i32::from(y) - i32::from(top)) as usize] = vec![rgn_left, rgn_right];
        }
        return Some(rows);
    }

    const REGION_STOP: i16 = i16::MAX;
    let mut cursor = 10usize;
    let read_word = |storage: &[u8], cursor: &mut usize| {
        let bytes = storage.get(*cursor..cursor.checked_add(2)?)?;
        *cursor += 2;
        Some(i16::from_be_bytes([bytes[0], bytes[1]]))
    };
    let mut next_change_y = read_word(storage, &mut cursor)?;
    let mut active = Vec::new();
    for y32 in i32::from(rgn_top)..i32::from(overlap_bottom) {
        let y = y32 as i16;
        while next_change_y != REGION_STOP && next_change_y <= y {
            let mut delta = Vec::new();
            loop {
                let value = read_word(storage, &mut cursor)?;
                if value == REGION_STOP {
                    break;
                }
                delta.push(value);
            }
            active = ppc_region_merge_endpoints(&active, &delta);
            next_change_y = read_word(storage, &mut cursor)?;
        }
        if y >= overlap_top {
            rows[(i32::from(y) - i32::from(top)) as usize] = active.clone();
        }
    }
    Some(rows)
}

pub(super) fn ppc_region_endpoints_to_intervals(endpoints: &[i16]) -> Vec<(i16, i16)> {
    endpoints
        .chunks_exact(2)
        .filter_map(|pair| (pair[0] < pair[1]).then_some((pair[0], pair[1])))
        .collect()
}

pub(super) fn ppc_region_intervals_to_endpoints(mut intervals: Vec<(i16, i16)>) -> Vec<i16> {
    intervals.sort_unstable();
    let mut merged: Vec<(i16, i16)> = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        if start >= end {
            continue;
        }
        if let Some((_, last_end)) = merged.last_mut() {
            if start <= *last_end {
                *last_end = (*last_end).max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
        .into_iter()
        .flat_map(|(start, end)| [start, end])
        .collect()
}

pub(super) fn ppc_region_intersect_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
    let lhs = ppc_region_endpoints_to_intervals(lhs);
    let rhs = ppc_region_endpoints_to_intervals(rhs);
    let mut out = Vec::new();
    for &(lhs_start, lhs_end) in &lhs {
        for &(rhs_start, rhs_end) in &rhs {
            let start = lhs_start.max(rhs_start);
            let end = lhs_end.min(rhs_end);
            if start < end {
                out.push((start, end));
            }
        }
    }
    ppc_region_intervals_to_endpoints(out)
}

pub(super) fn ppc_region_union_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
    let mut intervals = ppc_region_endpoints_to_intervals(lhs);
    intervals.extend(ppc_region_endpoints_to_intervals(rhs));
    ppc_region_intervals_to_endpoints(intervals)
}

pub(super) fn ppc_region_difference_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
    let lhs = ppc_region_endpoints_to_intervals(lhs);
    let rhs = ppc_region_endpoints_to_intervals(rhs);
    let mut out = Vec::new();
    for (lhs_start, lhs_end) in lhs {
        let mut start = lhs_start;
        for &(rhs_start, rhs_end) in &rhs {
            if rhs_end <= start {
                continue;
            }
            if rhs_start >= lhs_end {
                break;
            }
            if rhs_start > start {
                out.push((start, rhs_start.min(lhs_end)));
            }
            start = start.max(rhs_end);
            if start >= lhs_end {
                break;
            }
        }
        if start < lhs_end {
            out.push((start, lhs_end));
        }
    }
    ppc_region_intervals_to_endpoints(out)
}

pub(super) fn ppc_region_combine_rows(lhs: &[i16], rhs: &[i16], operation: PpcRegionBooleanOp) -> Vec<i16> {
    match operation {
        PpcRegionBooleanOp::Intersection => ppc_region_intersect_rows(lhs, rhs),
        PpcRegionBooleanOp::Union => ppc_region_union_rows(lhs, rhs),
        PpcRegionBooleanOp::Difference => ppc_region_difference_rows(lhs, rhs),
        PpcRegionBooleanOp::Xor => ppc_region_union_rows(
            &ppc_region_difference_rows(lhs, rhs),
            &ppc_region_difference_rows(rhs, lhs),
        ),
    }
}

pub(super) fn ppc_region_storage_from_rows(top: i16, rows: &[Vec<i16>]) -> Option<Vec<u8>> {
    const REGION_STOP: i16 = i16::MAX;
    let mut bbox: Option<(i16, i16, i16, i16)> = None;
    for (row_index, row) in rows.iter().enumerate() {
        if row.is_empty() {
            continue;
        }
        let y = i32::from(top).checked_add(i32::try_from(row_index).ok()?)?;
        let y = i16::try_from(y).ok()?;
        let bottom = y.checked_add(1)?;
        let left = row[0];
        let right = *row.last()?;
        bbox = Some(match bbox {
            Some((current_top, current_left, current_bottom, current_right)) => (
                current_top,
                current_left.min(left),
                current_bottom.max(bottom),
                current_right.max(right),
            ),
            None => (y, left, bottom, right),
        });
    }
    let Some((bbox_top, left, bottom, right)) = bbox else {
        return Some(vec![0, 10, 0, 0, 0, 0, 0, 0, 0, 0]);
    };
    let first = &rows[(i32::from(bbox_top) - i32::from(top)) as usize];
    let rectangular = first == &[left, right]
        && (i32::from(bbox_top)..i32::from(bottom))
            .all(|y| rows[(y - i32::from(top)) as usize] == *first);

    let mut words = Vec::new();
    if !rectangular {
        let mut previous = Vec::new();
        for y32 in i32::from(bbox_top)..=i32::from(bottom) {
            let current = if y32 < i32::from(bottom) {
                rows[(y32 - i32::from(top)) as usize].clone()
            } else {
                Vec::new()
            };
            let delta = ppc_region_merge_endpoints(&previous, &current);
            if !delta.is_empty() {
                words.push(y32 as i16);
                words.extend(delta);
                words.push(REGION_STOP);
            }
            previous = current;
        }
        words.push(REGION_STOP);
    }
    let size = 10usize.checked_add(words.len().checked_mul(2)?)?;
    let size = u16::try_from(size).ok()?;
    let mut storage = Vec::with_capacity(usize::from(size));
    storage.extend_from_slice(&size.to_be_bytes());
    for value in [bbox_top, left, bottom, right] {
        storage.extend_from_slice(&value.to_be_bytes());
    }
    for word in words {
        storage.extend_from_slice(&word.to_be_bytes());
    }
    Some(storage)
}

pub(super) fn ppc_write_region_storage(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    rgn_handle: u32,
    storage: &[u8],
) -> i16 {
    let result = ppc_allocator_view_resize_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        rgn_handle,
        u32::try_from(storage.len()).unwrap_or(u32::MAX),
    );
    if result != PPC_NO_ERR {
        return result;
    }
    let Some(ptr) = ppc_rgn_ptr(memory, rgn_handle) else {
        return PPC_PARAM_ERR;
    };
    for (offset, byte) in storage.iter().copied().enumerate() {
        if memory.write_u8(ptr + offset as u32, byte).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    PPC_NO_ERR
}

pub(super) fn ppc_region_boolean_op(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    src_a: u32,
    src_b: u32,
    dst: u32,
    operation: PpcRegionBooleanOp,
) -> i16 {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 3-93–3-96:
    // region Boolean operations preserve the complete mathematical shape,
    // allow a destination to alias either source, and write into an existing
    // destination handle.
    let Some(storage_a) = ppc_region_storage(memory, src_a) else {
        return PPC_PARAM_ERR;
    };
    let Some(storage_b) = ppc_region_storage(memory, src_b) else {
        return PPC_PARAM_ERR;
    };
    let bbox_a = ppc_region_storage_bbox(&storage_a);
    let bbox_b = ppc_region_storage_bbox(&storage_b);
    let band = match operation {
        PpcRegionBooleanOp::Intersection => match (bbox_a, bbox_b) {
            (Some(a), Some(b)) if a.2.min(b.2) > a.0.max(b.0) => Some((a.0.max(b.0), a.2.min(b.2))),
            _ => None,
        },
        PpcRegionBooleanOp::Difference => bbox_a.map(|a| (a.0, a.2)),
        PpcRegionBooleanOp::Union | PpcRegionBooleanOp::Xor => match (bbox_a, bbox_b) {
            (Some(a), Some(b)) => Some((a.0.min(b.0), a.2.max(b.2))),
            (Some(a), None) => Some((a.0, a.2)),
            (None, Some(b)) => Some((b.0, b.2)),
            (None, None) => None,
        },
    };
    let output = if let Some((top, bottom)) = band {
        let Some(rows_a) = ppc_region_rows_for_band(&storage_a, top, bottom) else {
            return PPC_PARAM_ERR;
        };
        let Some(rows_b) = ppc_region_rows_for_band(&storage_b, top, bottom) else {
            return PPC_PARAM_ERR;
        };
        let rows = rows_a
            .iter()
            .zip(&rows_b)
            .map(|(lhs, rhs)| ppc_region_combine_rows(lhs, rhs, operation))
            .collect::<Vec<_>>();
        let Some(output) = ppc_region_storage_from_rows(top, &rows) else {
            return PPC_MEM_FULL_ERR;
        };
        output
    } else {
        vec![0, 10, 0, 0, 0, 0, 0, 0, 0, 0]
    };
    ppc_write_region_storage(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        dst,
        &output,
    )
}

pub(super) fn ppc_set_rect_rgn(
    memory: &mut PpcSectionMem,
    rgn_handle: u32,
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
) -> Option<()> {
    ppc_write_rgn_bbox(memory, rgn_handle, top, left, bottom, right)
}

pub(super) fn ppc_rect_rgn(memory: &mut PpcSectionMem, rgn_handle: u32, rect_ptr: u32) -> Option<()> {
    let (top, left, bottom, right) = ppc_read_rect(memory, rect_ptr)?;
    ppc_write_rgn_bbox(memory, rgn_handle, top, left, bottom, right)
}

pub(super) fn ppc_offset_rgn(memory: &mut PpcSectionMem, rgn_handle: u32, dh: i16, dv: i16) -> i16 {
    let Some(ptr) = ppc_rgn_ptr(memory, rgn_handle) else {
        return PPC_PARAM_ERR;
    };
    let Some(size) = memory.read_u16_be(ptr).map(u32::from) else {
        return PPC_PARAM_ERR;
    };
    if size < 10 || !ppc_memory_can_write_bytes(memory, ptr, size) {
        return PPC_PARAM_ERR;
    }
    let Some((top, left, bottom, right)) = ppc_read_rect(memory, ptr + 2) else {
        return PPC_PARAM_ERR;
    };
    if bottom <= top || right <= left {
        return PPC_NO_ERR;
    }
    if ppc_write_rect(
        memory,
        ptr + 2,
        top.wrapping_add(dv),
        left.wrapping_add(dh),
        bottom.wrapping_add(dv),
        right.wrapping_add(dh),
    )
    .is_none()
    {
        return PPC_PARAM_ERR;
    }
    if size == 10 {
        return PPC_NO_ERR;
    }

    // Imaging With QuickDraw (1994), p. 3-93: the nonrectangular region
    // payload alternates vertical change coordinates with horizontal boundary
    // coordinates; $7FFF terminates each row and then the complete region.
    let mut offset = 10u32;
    while offset + 2 <= size {
        let Some(y) = memory.read_u16_be(ptr + offset) else {
            return PPC_PARAM_ERR;
        };
        if y == 0x7fff {
            return PPC_NO_ERR;
        }
        if memory
            .write_u16_be(ptr + offset, (y as i16).wrapping_add(dv) as u16)
            .is_none()
        {
            return PPC_PARAM_ERR;
        }
        offset += 2;
        loop {
            if offset + 2 > size {
                return PPC_PARAM_ERR;
            }
            let Some(x) = memory.read_u16_be(ptr + offset) else {
                return PPC_PARAM_ERR;
            };
            if x == 0x7fff {
                offset += 2;
                break;
            }
            if memory
                .write_u16_be(ptr + offset, (x as i16).wrapping_add(dh) as u16)
                .is_none()
            {
                return PPC_PARAM_ERR;
            }
            offset += 2;
        }
    }
    PPC_PARAM_ERR
}

pub(super) fn ppc_empty_rgn(memory: &mut PpcSectionMem, rgn_handle: u32) -> bool {
    let Some((top, left, bottom, right)) = ppc_read_rgn_bbox(memory, rgn_handle) else {
        return true;
    };
    bottom <= top || right <= left
}

pub(super) fn ppc_point_in_region(memory: &mut PpcSectionMem, rgn_handle: u32, v: i16, h: i16) -> bool {
    let Some(storage) = ppc_region_storage(memory, rgn_handle) else {
        return false;
    };
    ppc_point_in_region_storage(&storage, v, h)
}

pub(super) fn ppc_point_in_region_storage(storage: &[u8], v: i16, h: i16) -> bool {
    let Some((top, left, bottom, right)) = ppc_region_storage_bbox(&storage) else {
        return false;
    };
    if v < top || v >= bottom || h < left || h >= right {
        return false;
    }
    if storage.len() == 10 {
        return true;
    }
    ppc_region_rows_for_band(&storage, v, v.saturating_add(1)).is_some_and(|rows| {
        rows.first().is_some_and(|row| {
            row.chunks_exact(2)
                .any(|interval| h >= interval[0] && h < interval[1])
        })
    })
}

pub(super) fn ppc_rect_in_region(memory: &mut PpcSectionMem, rect_ptr: u32, rgn_handle: u32) -> bool {
    let Some((rect_top, rect_left, rect_bottom, rect_right)) = ppc_read_rect(memory, rect_ptr)
    else {
        return false;
    };
    let Some(storage) = ppc_region_storage(memory, rgn_handle) else {
        return false;
    };
    let Some((rgn_top, rgn_left, rgn_bottom, rgn_right)) = ppc_region_storage_bbox(&storage) else {
        return false;
    };
    let top = rect_top.max(rgn_top);
    let bottom = rect_bottom.min(rgn_bottom);
    if top >= bottom || rect_left.max(rgn_left) >= rect_right.min(rgn_right) {
        return false;
    }
    ppc_region_rows_for_band(&storage, top, bottom).is_some_and(|rows| {
        rows.iter().any(|row| {
            row.chunks_exact(2)
                .any(|interval| rect_left < interval[1] && rect_right > interval[0])
        })
    })
}

pub(super) fn ppc_open_polygon(memory: &mut PpcSectionMem, current_gworld: u32) -> Option<u32> {
    (current_gworld != 0)
        .then(|| memory.read_u32_be(current_gworld.wrapping_add(100)))
        .flatten()
        .filter(|handle| *handle != 0)
}

pub(super) fn ppc_open_poly(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    current_gworld: u32,
) -> u32 {
    if current_gworld == 0 || ppc_open_polygon(memory, current_gworld).is_some() {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    let handle = ppc_process_alloc_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        10,
        true,
    );
    let Some(ptr) = memory.read_u32_be(handle).filter(|ptr| *ptr != 0) else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    };
    if memory.write_u16_be(ptr, 10).is_none()
        || ppc_write_rect(memory, ptr + 2, 0, 0, 0, 0).is_none()
        || memory
            .write_u32_be(current_gworld.wrapping_add(100), handle)
            .is_none()
    {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    // Imaging With QuickDraw (1994), pp. 3-77--3-81: OpenPoly allocates a
    // PolyHandle, installs it in polySave, and records subsequent Move/Line
    // vertices until ClosePoly clears polySave.
    *last_mem_error = PPC_NO_ERR;
    handle
}

pub(super) fn ppc_record_polygon_point(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    handle: u32,
    h: i16,
    v: i16,
) -> i16 {
    let Some(ptr) = memory.read_u32_be(handle).filter(|ptr| *ptr != 0) else {
        return PPC_PARAM_ERR;
    };
    let Some(size) = memory.read_u16_be(ptr).map(u32::from) else {
        return PPC_PARAM_ERR;
    };
    if size < 10 || (size - 10) % 4 != 0 {
        return PPC_PARAM_ERR;
    }
    if size >= 14
        && memory.read_u16_be(ptr + size - 4) == Some(v as u16)
        && memory.read_u16_be(ptr + size - 2) == Some(h as u16)
    {
        return PPC_NO_ERR;
    }
    let Some(new_size) = size.checked_add(4) else {
        return PPC_MEM_FULL_ERR;
    };
    let result = process_memory_manager.set_native_handle_size(memory, handle, new_size);
    ppc_apply_process_native_resource_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        handle,
    );
    if result != PPC_NO_ERR {
        return result;
    }
    let Some(ptr) = memory.read_u32_be(handle).filter(|ptr| *ptr != 0) else {
        return PPC_PARAM_ERR;
    };
    if memory.write_u16_be(ptr + size, v as u16).is_none()
        || memory.write_u16_be(ptr + size + 2, h as u16).is_none()
        || memory.write_u16_be(ptr, new_size as u16).is_none()
    {
        return PPC_PARAM_ERR;
    }
    let bounds = if size == 10 {
        (v, h, v.saturating_add(1), h.saturating_add(1))
    } else {
        let Some((top, left, bottom, right)) = ppc_read_rect(memory, ptr + 2) else {
            return PPC_PARAM_ERR;
        };
        (
            top.min(v),
            left.min(h),
            bottom.max(v.saturating_add(1)),
            right.max(h.saturating_add(1)),
        )
    };
    if ppc_write_rect(memory, ptr + 2, bounds.0, bounds.1, bounds.2, bounds.3).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(super) fn ppc_polygon_points(memory: &mut PpcSectionMem, handle: u32) -> Option<Vec<(i16, i16)>> {
    let ptr = memory.read_u32_be(handle).filter(|ptr| *ptr != 0)?;
    let size = u32::from(memory.read_u16_be(ptr)?);
    if size < 10 || (size - 10) % 4 != 0 {
        return None;
    }
    (10..size)
        .step_by(4)
        .map(|offset| {
            Some((
                memory.read_u16_be(ptr + offset + 2)? as i16,
                memory.read_u16_be(ptr + offset)? as i16,
            ))
        })
        .collect()
}

pub(super) fn ppc_paint_polygon(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    handle: u32,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    let Some(points) = ppc_polygon_points(memory, handle).filter(|points| points.len() >= 3) else {
        return false;
    };
    let Some(ptr) = memory.read_u32_be(handle) else {
        return false;
    };
    let Some(bounds) = ppc_read_rect(memory, ptr + 2) else {
        return false;
    };
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return false;
    };
    let (top, left, bottom, right) = surface.local_rect(bounds);
    let points = points
        .into_iter()
        .map(|(h, v)| surface.local_point((i32::from(h), i32::from(v))))
        .collect::<Vec<_>>();
    let mut wrote = false;
    for y in top..bottom {
        for x in left..right {
            let mut crossings = 0u32;
            for index in 0..points.len() {
                let (x1, y1) = points[index];
                let (x2, y2) = points[(index + 1) % points.len()];
                let (x1, y1, x2, y2) = (f64::from(x1), f64::from(y1), f64::from(x2), f64::from(y2));
                let (y_min, y_max, x_at_min, x_at_max) = if y1 < y2 {
                    (y1, y2, x1, x2)
                } else {
                    (y2, y1, x2, x1)
                };
                if y1 != y2 && f64::from(y) >= y_min && f64::from(y) < y_max {
                    let intersection = x_at_min
                        + (f64::from(y) - y_min) * (x_at_max - x_at_min) / (y_max - y_min);
                    if intersection <= f64::from(x) {
                        crossings += 1;
                    }
                }
            }
            if crossings & 1 != 0 {
                wrote |= ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), color_pixel);
            }
        }
    }
    wrote
}

pub(super) fn ppc_frame_polygon(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    handle: u32,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    // Inside Macintosh: Imaging With QuickDraw, pp. 3-81–3-82: FramePoly
    // plays back the polygon's line-drawing commands with the current pen.
    let Some(points) = ppc_polygon_points(memory, handle).filter(|points| points.len() >= 2) else {
        return false;
    };
    let mut wrote = false;
    for i in 0..points.len() {
        let from = points[i];
        let to = points[(i + 1) % points.len()];
        wrote |= ppc_line_to(
            memory,
            gworlds,
            current_gworld,
            from,
            to,
            color,
            explicit_index,
        );
    }
    wrote
}

pub(super) fn ppc_offset_rect(memory: &mut PpcSectionMem, rect_ptr: u32, dh: i16, dv: i16) -> Option<()> {
    let (top, left, bottom, right) = ppc_read_rect(memory, rect_ptr)?;
    ppc_write_rect(
        memory,
        rect_ptr,
        top.wrapping_add(dv),
        left.wrapping_add(dh),
        bottom.wrapping_add(dv),
        right.wrapping_add(dh),
    )
}

pub(super) fn ppc_map_rect(memory: &mut PpcSectionMem, rect_ptr: u32, src_ptr: u32, dst_ptr: u32) {
    // Inside Macintosh Volume I (1985), p. I-197: MapRect maps all four
    // coordinates proportionally from srcRect's coordinate space into
    // dstRect's coordinate space.
    let (
        Some((top, left, bottom, right)),
        Some((src_top, src_left, src_bottom, src_right)),
        Some((dst_top, dst_left, dst_bottom, dst_right)),
    ) = (
        ppc_read_rect(memory, rect_ptr),
        ppc_read_rect(memory, src_ptr),
        ppc_read_rect(memory, dst_ptr),
    )
    else {
        return;
    };
    let src_width = i64::from(src_right) - i64::from(src_left);
    let src_height = i64::from(src_bottom) - i64::from(src_top);
    let dst_width = i64::from(dst_right) - i64::from(dst_left);
    let dst_height = i64::from(dst_bottom) - i64::from(dst_top);
    let map_h = |value: i16| {
        if src_width == 0 {
            value
        } else {
            (i64::from(dst_left) + (i64::from(value) - i64::from(src_left)) * dst_width / src_width)
                as i16
        }
    };
    let map_v = |value: i16| {
        if src_height == 0 {
            value
        } else {
            (i64::from(dst_top) + (i64::from(value) - i64::from(src_top)) * dst_height / src_height)
                as i16
        }
    };
    let _ = ppc_write_rect(
        memory,
        rect_ptr,
        map_v(top),
        map_h(left),
        map_v(bottom),
        map_h(right),
    );
}

