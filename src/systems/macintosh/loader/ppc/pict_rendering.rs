//! PowerPC QuickDraw picture rendering, raw quilt blits, and PixMap lifecycle.

use std::collections::HashMap;

use super::*;

pub(crate) fn ppc_rgb555_to_rgb16(pixel: u16) -> [u16; 3] {
    fn component(value: u16) -> u16 {
        (((u32::from(value) * 65_535) + 15) / 31) as u16
    }
    [
        component((pixel >> 10) & 0x1f),
        component((pixel >> 5) & 0x1f),
        component(pixel & 0x1f),
    ]
}

pub(crate) fn ppc_rgb555_to_clut_index(pixel: u16, clut: &[[u16; 3]; 256]) -> u8 {
    let [red, green, blue] = ppc_rgb555_to_rgb16(pixel & 0x7fff);
    pict::closest_clut_index(red, green, blue, clut)
}

pub(crate) fn ppc_draw_raw_quilt_frame(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    data: &[u8],
    pict_bounds: (i16, i16, i16, i16),
    dst_rect: (i16, i16, i16, i16),
    clut: &[[u16; 3]; 256],
    zero_is_opaque: bool,
) -> bool {
    if data.len() < 24 || data[10..24].iter().any(|byte| *byte != 0) {
        return false;
    }
    let (src_top, src_left, src_bottom, src_right) = pict_bounds;
    let src_width = i32::from(src_right) - i32::from(src_left);
    let src_height = i32::from(src_bottom) - i32::from(src_top);
    if src_width <= 0 || src_height <= 0 {
        return false;
    }
    let raw = &data[24..];
    let Ok(src_height_usize) = usize::try_from(src_height) else {
        return false;
    };
    if src_height_usize == 0 || raw.len() % src_height_usize != 0 {
        return false;
    }
    let src_row_bytes = raw.len() / src_height_usize;
    let Ok(src_width_usize) = usize::try_from(src_width) else {
        return false;
    };
    if src_row_bytes < src_width_usize {
        return false;
    }
    let (dst_top, dst_left, dst_bottom, dst_right) = dst_rect;
    let dst_width = i32::from(dst_right) - i32::from(dst_left);
    let dst_height = i32::from(dst_bottom) - i32::from(dst_top);
    if dst_width <= 0 || dst_height <= 0 {
        return false;
    }
    let copy_top = i32::from(dst_top).max(0).min(front_buffer.height as i32);
    let copy_bottom = i32::from(dst_bottom).max(0).min(front_buffer.height as i32);
    let copy_left = i32::from(dst_left).max(0).min(front_buffer.width as i32);
    let copy_right = i32::from(dst_right).max(0).min(front_buffer.width as i32);
    if copy_bottom <= copy_top || copy_right <= copy_left {
        return false;
    }

    for y in copy_top..copy_bottom {
        let src_y =
            ((y - i32::from(dst_top)) * src_height / dst_height).clamp(0, src_height - 1) as usize;
        for x in copy_left..copy_right {
            let src_x = ((x - i32::from(dst_left)) * src_width / dst_width).clamp(0, src_width - 1)
                as usize;
            let Some(mut color_index) = raw
                .get(src_y.saturating_mul(src_row_bytes).saturating_add(src_x))
                .copied()
            else {
                continue;
            };
            if zero_is_opaque && color_index == 0 {
                color_index = pict::closest_clut_index(0, 0, 0, clut);
            }
            if front_buffer.depth == 8 {
                let Some(dst_addr) = front_buffer
                    .base_addr
                    .checked_add((y as u32).saturating_mul(front_buffer.row_bytes))
                    .and_then(|row| row.checked_add(x as u32))
                else {
                    continue;
                };
                let _ = memory.write_u8(dst_addr, color_index);
            } else if front_buffer.depth == 16 {
                let [red, green, blue] = clut[color_index as usize];
                let pixel = ppc_q3_rgb555((
                    f32::from(red) / 65_535.0,
                    f32::from(green) / 65_535.0,
                    f32::from(blue) / 65_535.0,
                ));
                let _ = ppc_q3_write_software_pixel(memory, front_buffer, (x, y), pixel);
            } else {
                return false;
            }
        }
    }
    true
}

pub(crate) fn ppc_draw_pict_bytes_to_16bpp(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    data: &[u8],
    dst_rect: (i16, i16, i16, i16),
    clut: &[[u16; 3]; 256],
    device_ct_seed: u32,
    quilt_zero_is_opaque: bool,
) -> bool {
    if front_buffer.base_addr == 0 || front_buffer.width == 0 || front_buffer.height == 0 {
        return false;
    }
    let Some((pict_offset, bounds)) = ppc_qt_pict_record_offset_and_bounds(data) else {
        return false;
    };
    let (dst_top, dst_left, dst_bottom, dst_right) = dst_rect;
    if dst_bottom <= dst_top || dst_right <= dst_left {
        return false;
    }

    let Ok(width) = u16::try_from(front_buffer.width) else {
        return false;
    };
    let Ok(height) = u16::try_from(front_buffer.height) else {
        return false;
    };
    if pict_offset == 0
        && ppc_draw_raw_quilt_frame(
            memory,
            front_buffer,
            data,
            bounds,
            dst_rect,
            clut,
            quilt_zero_is_opaque,
        )
    {
        return true;
    }
    if matches!(front_buffer.depth, 1 | 2 | 4 | 8) {
        let Some(minimum_row_bytes) = front_buffer
            .width
            .checked_mul(front_buffer.depth)
            .and_then(|bits| bits.checked_add(7))
            .map(|bits| bits / 8)
        else {
            return false;
        };
        if front_buffer.row_bytes < minimum_row_bytes {
            return false;
        }
        let row_bytes = front_buffer.row_bytes;
        let Some(buffer_len) = row_bytes.checked_mul(front_buffer.height) else {
            return false;
        };
        let Some(buffer_len_usize) = usize::try_from(buffer_len).ok() else {
            return false;
        };
        let pict_base = 0x0001_0000u32;
        let Some(pict_end) = pict_base.checked_add(u32::try_from(data.len()).unwrap_or(u32::MAX))
        else {
            return false;
        };
        let screen_base = (pict_end.saturating_add(0x0fff)) & !0x0fffu32;
        let Some(screen_end) = screen_base.checked_add(buffer_len) else {
            return false;
        };
        let Some(ram_size) = usize::try_from(screen_end.saturating_add(0x1000)).ok() else {
            return false;
        };
        if ram_size > 128 * 1024 * 1024 {
            return false;
        }

        let mut indexed = vec![0u8; buffer_len_usize];
        for y in 0..front_buffer.height {
            let Some(src_addr) = front_buffer
                .base_addr
                .checked_add(y.saturating_mul(front_buffer.row_bytes))
            else {
                return false;
            };
            let Some(dst_offset) = usize::try_from(y.saturating_mul(row_bytes)).ok() else {
                return false;
            };
            let Some(row_len) = usize::try_from(row_bytes).ok() else {
                return false;
            };
            if memory
                .read_bytes_into(src_addr, &mut indexed[dst_offset..dst_offset + row_len])
                .is_none()
            {
                return false;
            }
        }

        // Color QuickDraw matches colors only against pixel values the
        // destination PixMap can represent. A short 1/2/4-bit CTable is
        // overlaid on the logical 8-bit table, so hide that inherited tail
        // from the PICT renderer; otherwise a color can match (for example)
        // index 42 and then be truncated to the low one or two packed bits.
        // This is the same depth-limiting rule used by the 68K DrawPicture
        // path. Imaging With QuickDraw (1994), pp. 4-81--4-83 and 7-44--7-45.
        let packed_clut = matches!(front_buffer.depth, 1 | 2 | 4).then(|| {
            let mut packed_clut = *clut;
            let entry_count = 1usize << front_buffer.depth;
            let terminal = packed_clut[entry_count - 1];
            packed_clut[entry_count..].fill(terminal);
            packed_clut
        });
        let draw_clut = packed_clut.as_ref().unwrap_or(clut);

        let mut bus = MacMemoryBus::new(ram_size);
        bus.write_bytes(pict_base, data);
        bus.write_bytes(screen_base, &indexed);
        bus.begin_uncapped_write_probe();
        let (rendered, _) = pict::draw_picture(
            &mut bus,
            pict_base + u32::try_from(pict_offset).unwrap_or(0),
            dst_top,
            dst_left,
            dst_bottom,
            dst_right,
            (
                screen_base,
                row_bytes,
                width,
                height,
                front_buffer.depth as u16,
            ),
            draw_clut,
            device_ct_seed,
            None,
        );
        if !rendered {
            return false;
        }

        return ppc_commit_picture_writes(memory, &mut bus, screen_base, front_buffer, buffer_len);
    }

    if !matches!(front_buffer.depth, 16 | 32)
        || front_buffer.row_bytes < front_buffer.width.saturating_mul(front_buffer.depth / 8) {
        return false;
    }
    let row_bytes = front_buffer.row_bytes;
    let Some(buffer_len) = row_bytes.checked_mul(front_buffer.height) else {
        return false;
    };
    let Some(buffer_len_usize) = usize::try_from(buffer_len).ok() else {
        return false;
    };
    let pict_base = 0x0001_0000u32;
    let Some(pict_end) = pict_base.checked_add(u32::try_from(data.len()).unwrap_or(u32::MAX))
    else {
        return false;
    };
    let screen_base = (pict_end.saturating_add(0x0fff)) & !0x0fffu32;
    let Some(screen_end) = screen_base.checked_add(buffer_len) else {
        return false;
    };
    let Some(ram_size) = usize::try_from(screen_end.saturating_add(0x1000)).ok() else {
        return false;
    };
    if ram_size > 128 * 1024 * 1024 {
        return false;
    }

    let mut direct = vec![0u8; buffer_len_usize];
    for y in 0..front_buffer.height {
        let Some(src_addr) = front_buffer
            .base_addr
            .checked_add(y.saturating_mul(front_buffer.row_bytes))
        else {
            return false;
        };
        let Some(dst_offset) = usize::try_from(y.saturating_mul(row_bytes)).ok() else {
            return false;
        };
        let Some(row_len) = usize::try_from(row_bytes).ok() else {
            return false;
        };
        if memory
            .read_bytes_into(src_addr, &mut direct[dst_offset..dst_offset + row_len])
            .is_none()
        {
            return false;
        }
    }

    let mut bus = MacMemoryBus::new(ram_size);
    bus.write_bytes(pict_base, data);
    bus.write_bytes(screen_base, &direct);
    bus.begin_uncapped_write_probe();
    let (rendered, _) = pict::draw_picture(
        &mut bus,
        pict_base + u32::try_from(pict_offset).unwrap_or(0),
        dst_top,
        dst_left,
        dst_bottom,
        dst_right,
        (screen_base, row_bytes, width, height, front_buffer.depth as u16),
        clut,
        device_ct_seed,
        None,
    );
    if !rendered {
        return false;
    }

    ppc_commit_picture_writes(memory, &mut bus, screen_base, front_buffer, buffer_len)
}

pub(crate) fn ppc_commit_picture_writes(
    memory: &mut PpcSectionMem,
    bus: &mut MacMemoryBus,
    screen_base: u32,
    front_buffer: PpcFrontBuffer,
    buffer_len: u32,
) -> bool {
    // DrawPicture scales the picture into dstRect; its drawing operations can
    // extend beyond the picture frame. Copy the actual writes, not a rectangle
    // or a logical-pixel diff. Imaging With QuickDraw (1994), pp. 7-44--7-45.
    for range in bus.finish_write_probe_ranges() {
        let start = range.start.max(screen_base);
        let end = range.end.min(screen_base + buffer_len);
        if start < end {
            let bytes = bus.read_bytes(start, (end - start) as usize);
            if memory
                .write_bytes(front_buffer.base_addr + start - screen_base, &bytes)
                .is_none()
            {
                return false;
            }
        }
    }
    true
}

pub(crate) fn minimal_pict_bytes() -> [u8; 12] {
    [
        0x00, 0x0c, // picSize
        0x00, 0x00, 0x00, 0x00, // top, left
        0x00, 0x01, 0x00, 0x01, // bottom, right
        0x00, 0xff, // opEndPic
    ]
}

pub(crate) fn ppc_get_pict_info(cpu: &mut PpcCpu, memory: &mut PpcSectionMem, handles: &[PpcHandleRecord]) -> i16 {
    let pict_info_ptr = cpu.gpr[4];
    if pict_info_ptr == 0 || !ppc_memory_can_write_bytes(memory, pict_info_ptr, PPC_PICT_INFO_SIZE)
    {
        return PPC_PARAM_ERR;
    }
    if cpu.gpr[8] as u16 != 0 {
        return -11000; // pictInfoVersionErr
    }
    let Some(bytes) = ppc_handle_bytes(memory, handles, cpu.gpr[3]) else {
        return -11005; // pictureDataErr
    };
    let Some(info) = pict::picture_basic_info(&bytes) else {
        return -11005;
    };
    for offset in 0..PPC_PICT_INFO_SIZE {
        if memory.write_u8(pict_info_ptr + offset, 0).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    // PictInfo uses two-byte Macintosh structure alignment. Optional color,
    // font and comment allocations remain unsupported and their handles nil.
    memory.write_u32_be(pict_info_ptr + 14, info.h_res);
    memory.write_u32_be(pict_info_ptr + 18, info.v_res);
    memory.write_u16_be(pict_info_ptr + 22, info.depth);
    for (index, value) in info.source_rect.into_iter().enumerate() {
        memory.write_u16_be(pict_info_ptr + 24 + index as u32 * 2, value as u16);
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_new_pixmap(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    current_gdevice: u32,
) -> u32 {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-85--4-86:
    // NewPixMap clones the current device PixMap except for its ColorTable.
    // It allocates that handle but deliberately leaves the table uninitialized;
    // the application must install a table that describes its pixels.
    let source_pixmap = memory
        .read_u32_be(current_gdevice)
        .filter(|device| *device != 0)
        .and_then(|device| memory.read_u32_be(device.checked_add(22)?))
        .filter(|handle| *handle != 0)
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|pixmap| *pixmap != 0)
        .unwrap_or(PPC_MAIN_PIXMAP);
    let Some(mut pixmap_bytes) = ppc_memory_read_bytes(memory, source_pixmap, PPC_PIXMAP_SIZE)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };
    let ctable_bytes = [0; 8];
    let ctable_size = u32::try_from(ctable_bytes.len()).unwrap_or(u32::MAX);
    if !ppc_heap_can_alloc_sequence(
        memory,
        *heap_cursor,
        heap_limit,
        &[4, ctable_size, 4, PPC_PIXMAP_SIZE],
    ) {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    let ctable_handle = ppc_process_alloc_handle_with_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        &ctable_bytes,
    );
    let pixmap_handle = ppc_process_alloc_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        PPC_PIXMAP_SIZE,
        true,
    );
    if ctable_handle == 0 || pixmap_handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    pixmap_bytes[0..4].copy_from_slice(&0u32.to_be_bytes());
    pixmap_bytes[22..26].copy_from_slice(&0x0048_0000u32.to_be_bytes());
    pixmap_bytes[26..30].copy_from_slice(&0x0048_0000u32.to_be_bytes());
    pixmap_bytes[42..46].copy_from_slice(&ctable_handle.to_be_bytes());
    let Some(pixmap) = memory.read_u32_be(pixmap_handle) else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };
    if memory.write_bytes(pixmap, &pixmap_bytes).is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    *last_mem_error = PPC_NO_ERR;
    pixmap_handle
}

pub(crate) fn ppc_dispose_pixmap(
    pixmap_handle: u32,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    indexed_screen_ctables: &mut HashMap<u32, u32>,
) {
    if pixmap_handle == 0 {
        return;
    }
    // The same reference specifies that DisposePixMap owns both the PixMap
    // record and its pmTable handle. Read pmTable before invalidating pm.
    let ctable_handle = memory
        .read_u32_be(pixmap_handle)
        .filter(|pixmap| *pixmap != 0)
        .and_then(|pixmap| memory.read_u32_be(pixmap + 42))
        .filter(|handle| *handle != 0)
        .or_else(|| indexed_screen_ctables.remove(&pixmap_handle))
        .unwrap_or(0);
    if ctable_handle != 0 {
        let _ = ppc_dispose_process_native_handle(
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            ctable_handle,
        );
        indexed_screen_ctables.retain(|_, handle| *handle != ctable_handle);
    }
    indexed_screen_ctables.remove(&pixmap_handle);
    let _ = ppc_dispose_process_native_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        pixmap_handle,
    );
}
