//! PowerPC GWorlds, Graphics Devices, depth switching, port management, and pictures.

use super::graphics::*;
use super::quickdraw::*;
use super::regions::*;
use super::*;

/// True when both screen buffers fit in their spans at the active profile's
/// geometry. A false here would alias guest memory, so the loader refuses the
/// launch rather than silently corrupting the heap.
pub(crate) fn ppc_main_screen_fits() -> bool {
    let size = ppc_main_screen_buffer_size();
    PPC_MAIN_SCREEN_BASE
        .checked_add(size)
        .is_some_and(|end| end <= PPC_MAIN_GWORLD)
        && PPC_DSP_BACK_SCREEN_BASE
            .checked_add(size)
            .is_some_and(|end| end <= PPC_STACK_TOP.saturating_add(PPC_DSP_BACK_SCREEN_SPAN))
}

pub(crate) fn ppc_main_screen_buffer_size() -> u32 {
    ppc_row_bytes(ppc_main_screen_width(), PPC_MAIN_SCREEN_STORAGE_DEPTH).unwrap()
        * ppc_main_screen_height()
}

pub(crate) fn ppc_main_screen_row_bytes() -> u32 {
    ppc_row_bytes(ppc_main_screen_width(), PPC_MAIN_PIXEL_DEPTH).unwrap()
}

pub(crate) fn ppc_seed_main_gworld(memory: &mut PpcSectionMem) -> PpcGWorldRecord {
    let row_bytes = ppc_main_screen_row_bytes();
    let _ = ppc_seed_main_color_table(memory);
    let _ = memory.write_u32_be(PPC_GRAY_RGN_HANDLE, PPC_GRAY_RGN);
    let _ = memory.write_u32_be(PPC_GRAY_RGN_ADDR, PPC_GRAY_RGN_HANDLE);
    let _ = memory.write_u16_be(PPC_GRAY_RGN, 10);
    // Macintosh Toolbox Essentials (1992), pp. 3-112 and 4-16: GrayRgn is
    // the desktop area below the menu bar, whose live height is MBarHeight.
    let menu_bar_height = u32::from(memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20))
        .min(ppc_main_screen_height()) as i16;
    let _ = ppc_write_rect(
        memory,
        PPC_GRAY_RGN + 2,
        menu_bar_height,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    );
    let _ = memory.write_u32_be(PPC_MAIN_DCE_HANDLE, PPC_MAIN_DCE);
    let _ = memory.write_u16_be(PPC_MAIN_DCE + 24, 0);
    let _ = memory.write_u32_be(PPC_MAIN_DCE + 42, PPC_MAIN_SCREEN_BASE);
    let _ = memory.write_u32_be(PPC_MAIN_PIXMAP_HANDLE, PPC_MAIN_PIXMAP);
    let _ = ppc_write_pixmap(
        memory,
        PPC_MAIN_PIXMAP,
        PPC_MAIN_SCREEN_BASE,
        row_bytes,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
        PPC_MAIN_PIXEL_DEPTH,
    );
    let _ = memory.write_u32_be(PPC_MAIN_PIXMAP + 42, PPC_MAIN_CTABLE_HANDLE);
    let _ = ppc_write_gworld_port(
        memory,
        PPC_MAIN_GWORLD,
        PPC_MAIN_PIXMAP_HANDLE,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    );
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 2-38--2-42:
    // every open graphics port owns valid visible and clipping regions.
    let _ = memory.write_u32_be(PPC_MAIN_VIS_RGN_HANDLE, PPC_MAIN_VIS_RGN);
    let _ = memory.write_u32_be(PPC_MAIN_CLIP_RGN_HANDLE, PPC_MAIN_CLIP_RGN);
    let _ = memory.write_u32_be(
        PPC_MAIN_GWORLD + PPC_CGRAF_PORT_VIS_RGN_OFFSET,
        PPC_MAIN_VIS_RGN_HANDLE,
    );
    let _ = memory.write_u32_be(
        PPC_MAIN_GWORLD + PPC_CGRAF_PORT_CLIP_RGN_OFFSET,
        PPC_MAIN_CLIP_RGN_HANDLE,
    );
    let _ = ppc_write_rgn_bbox(
        memory,
        PPC_MAIN_VIS_RGN_HANDLE,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    );
    let _ = ppc_write_rgn_bbox(
        memory,
        PPC_MAIN_CLIP_RGN_HANDLE,
        i16::MIN,
        i16::MIN,
        i16::MAX,
        i16::MAX,
    );
    let _ = memory.write_u32_be(PPC_MAIN_GDEVICE, PPC_MAIN_GDEVICE_RECORD);
    let _ = ppc_write_gdevice(
        memory,
        PPC_MAIN_GDEVICE_RECORD,
        PPC_MAIN_PIXMAP_HANDLE,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    );
    let record = PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: PPC_MAIN_PIXMAP_HANDLE,
        pixmap: PPC_MAIN_PIXMAP,
        base_addr: PPC_MAIN_SCREEN_BASE,
        gdevice: PPC_MAIN_GDEVICE,
        width: ppc_main_screen_width(),
        height: ppc_main_screen_height(),
        depth: PPC_MAIN_PIXEL_DEPTH,
        row_bytes,
        pixels_locked: false,
        pixels_no_purge: true,
    };
    let front_buffer = PpcFrontBuffer {
        base_addr: record.base_addr,
        row_bytes: record.row_bytes,
        width: record.width,
        height: record.height,
        depth: record.depth,
    };
    let fallback = TrapDispatcher::standard_mac_indexed_clut(front_buffer.depth as u16)
        .map(|(clut, _)| clut)
        .unwrap_or_else(TrapDispatcher::standard_mac_8bpp_clut);
    let clut = ppc_read_ctable_clut(memory, PPC_MAIN_CTABLE_HANDLE, &fallback).unwrap_or(fallback);
    let entry_count = ppc_indexed_depth_entry_count(front_buffer.depth).unwrap_or(256);
    let black = u16::from(ppc_rgb_color_to_index_in_clut(
        ppc_standard_desktop_color(std::slice::from_ref(&record), 0, 0),
        &clut,
        entry_count,
    ));
    let white = u16::from(ppc_rgb_color_to_index_in_clut(
        ppc_standard_desktop_color(std::slice::from_ref(&record), 1, 0),
        &clut,
        entry_count,
    ));
    let screen_width = ppc_main_screen_width() as i32;
    let screen_height = ppc_main_screen_height() as i32;
    for v in i32::from(menu_bar_height)..screen_height {
        if front_buffer.depth == 8 {
            let row = (0..screen_width)
                .map(|h| {
                    if crate::window_manager::standard_desktop_pattern_is_ink(h, v) {
                        black as u8
                    } else {
                        white as u8
                    }
                })
                .collect::<Vec<_>>();
            let _ = memory.write_bytes(
                front_buffer.base_addr + v as u32 * front_buffer.row_bytes,
                &row,
            );
        } else {
            for h in 0..screen_width {
                let pixel = if crate::window_manager::standard_desktop_pattern_is_ink(h, v) {
                    black
                } else {
                    white
                };
                let _ = ppc_quickdraw_write_raw_pixel(memory, front_buffer, (h, v), pixel);
            }
        }
    }
    record
}

pub(crate) fn ppc_init_graf(memory: &mut PpcSectionMem, gworlds: &[PpcGWorldRecord], global_ptr: u32) -> bool {
    // InitGraf receives the address of QDGlobals.thePort. In the classic
    // reverse layout, randSeed begins 126 bytes before that pointer and the
    // 14-byte screenBits record begins 122 bytes before it.
    let Some(globals_base) = global_ptr.checked_sub(126) else {
        return false;
    };
    if !ppc_memory_can_write_bytes(memory, globals_base, 130) {
        return false;
    }
    let Some(front_buffer) = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD) else {
        return false;
    };

    let white = [0x00; 8];
    let black = [0xff; 8];
    let gray = [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55];
    let lt_gray = [0x88, 0x22, 0x88, 0x22, 0x88, 0x22, 0x88, 0x22];
    let dk_gray = [0x77, 0xdd, 0x77, 0xdd, 0x77, 0xdd, 0x77, 0xdd];
    let (arrow_data, arrow_mask, arrow_hot_v, arrow_hot_h) = TrapDispatcher::default_arrow_cursor();
    let arrow = global_ptr - 108;
    let screen_bits = global_ptr - 122;

    memory.write_u32_be(globals_base, 1).is_some()
        && memory
            .write_u32_be(screen_bits, front_buffer.base_addr)
            .is_some()
        && memory
            .write_u16_be(screen_bits + 4, front_buffer.row_bytes as u16 & 0x3fff)
            .is_some()
        && ppc_write_rect(
            memory,
            screen_bits + 6,
            0,
            0,
            ppc_u32_to_i16_saturating(front_buffer.height),
            ppc_u32_to_i16_saturating(front_buffer.width),
        )
        .is_some()
        && memory.write_bytes(arrow, &arrow_data).is_some()
        && memory.write_bytes(arrow + 32, &arrow_mask).is_some()
        && memory
            .write_u16_be(arrow + 64, arrow_hot_v as u16)
            .is_some()
        && memory
            .write_u16_be(arrow + 66, arrow_hot_h as u16)
            .is_some()
        && memory.write_bytes(global_ptr - 40, &dk_gray).is_some()
        && memory.write_bytes(global_ptr - 32, &lt_gray).is_some()
        && memory.write_bytes(global_ptr - 24, &gray).is_some()
        && memory.write_bytes(global_ptr - 16, &black).is_some()
        && memory.write_bytes(global_ptr - 8, &white).is_some()
        && memory.write_u32_be(global_ptr, PPC_MAIN_GWORLD).is_some()
}

pub(crate) fn ppc_seed_main_color_table(memory: &mut PpcSectionMem) -> Option<()> {
    memory.write_u32_be(PPC_MAIN_CTABLE_HANDLE, PPC_MAIN_CTABLE)?;
    memory.write_u32_be(PPC_MAIN_CTABLE, PPC_MAIN_PIXEL_DEPTH)?;
    memory.write_u16_be(PPC_MAIN_CTABLE + 4, 0x8000)?;
    memory.write_u16_be(PPC_MAIN_CTABLE + 6, 255)?;
    for (index, [red, green, blue]) in TrapDispatcher::standard_mac_8bpp_clut()
        .into_iter()
        .enumerate()
    {
        let entry = PPC_MAIN_CTABLE.checked_add(8 + index as u32 * 8)?;
        memory.write_u16_be(entry, index as u16)?;
        memory.write_u16_be(entry + 2, red)?;
        memory.write_u16_be(entry + 4, green)?;
        memory.write_u16_be(entry + 6, blue)?;
    }
    // Imaging With QuickDraw (1994), pp. 4-46 and 5-14: an indexed main
    // device PixMap owns a 256-entry ColorTable whose ctFlags device bit is
    // set and whose RGB entries mirror the hardware CLUT.
    Some(())
}

pub(crate) fn ppc_seed_dsp_back_gworld(memory: &mut PpcSectionMem) -> PpcGWorldRecord {
    let row_bytes = ppc_main_screen_row_bytes();
    let _ = memory.write_u32_be(PPC_DSP_BACK_PIXMAP_HANDLE, PPC_DSP_BACK_PIXMAP);
    let _ = ppc_write_pixmap(
        memory,
        PPC_DSP_BACK_PIXMAP,
        PPC_DSP_BACK_SCREEN_BASE,
        row_bytes,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
        PPC_MAIN_PIXEL_DEPTH,
    );
    let _ = memory.write_u32_be(PPC_DSP_BACK_PIXMAP + 42, PPC_MAIN_CTABLE_HANDLE);
    let _ = ppc_write_gworld_port(
        memory,
        PPC_DSP_BACK_GWORLD,
        PPC_DSP_BACK_PIXMAP_HANDLE,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    );
    let _ = memory.write_u32_be(PPC_DSP_BACK_VIS_RGN_HANDLE, PPC_DSP_BACK_VIS_RGN);
    let _ = memory.write_u32_be(PPC_DSP_BACK_CLIP_RGN_HANDLE, PPC_DSP_BACK_CLIP_RGN);
    let _ = memory.write_u32_be(
        PPC_DSP_BACK_GWORLD + PPC_CGRAF_PORT_VIS_RGN_OFFSET,
        PPC_DSP_BACK_VIS_RGN_HANDLE,
    );
    let _ = memory.write_u32_be(
        PPC_DSP_BACK_GWORLD + PPC_CGRAF_PORT_CLIP_RGN_OFFSET,
        PPC_DSP_BACK_CLIP_RGN_HANDLE,
    );
    let _ = ppc_write_rgn_bbox(
        memory,
        PPC_DSP_BACK_VIS_RGN_HANDLE,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    );
    let _ = ppc_write_rgn_bbox(
        memory,
        PPC_DSP_BACK_CLIP_RGN_HANDLE,
        i16::MIN,
        i16::MIN,
        i16::MAX,
        i16::MAX,
    );
    PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_DSP_BACK_GWORLD,
        pixmap_handle: PPC_DSP_BACK_PIXMAP_HANDLE,
        pixmap: PPC_DSP_BACK_PIXMAP,
        base_addr: PPC_DSP_BACK_SCREEN_BASE,
        gdevice: PPC_MAIN_GDEVICE,
        width: ppc_main_screen_width(),
        height: ppc_main_screen_height(),
        depth: PPC_MAIN_PIXEL_DEPTH,
        row_bytes,
        pixels_locked: false,
        pixels_no_purge: true,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_open_port(
    cpu: &PpcCpu,
    allocator: &mut PpcProcessAllocatorView<'_>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    current_gdevice: u32,
) -> bool {
    let port = cpu.gpr[3];
    if port == 0 || !ppc_memory_can_write_bytes(memory, port, PPC_GRAF_PORT_SIZE) {
        *last_mem_error = PPC_PARAM_ERR;
        return false;
    }
    let pixmap_handle = memory
        .read_u32_be(current_gdevice)
        .and_then(|gdevice| memory.read_u32_be(gdevice.checked_add(22)?))
        .unwrap_or(PPC_MAIN_PIXMAP_HANDLE);
    let bits = ppc_read_pixmap_handle_bits(memory, pixmap_handle).unwrap_or(PpcPixMapBits {
        base_addr: PPC_MAIN_SCREEN_BASE,
        row_bytes: ppc_main_screen_row_bytes(),
        top: 0,
        left: 0,
        bottom: ppc_main_screen_height() as i16,
        right: ppc_main_screen_width() as i16,
        width: ppc_main_screen_width(),
        height: ppc_main_screen_height(),
        depth: PPC_MAIN_PIXEL_DEPTH,
    });
    let vis_rgn = ppc_allocator_view_new_rgn(
        Some(allocator),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    let clip_rgn = ppc_allocator_view_new_rgn(
        Some(allocator),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    if vis_rgn == 0 || clip_rgn == 0 {
        return false;
    }

    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 2-38--2-40:
    // OpenPort copies screenBits into the caller-owned GrafPort, creates its
    // visible and clipping regions, and makes the new port current.
    let cleared = vec![0; PPC_GRAF_PORT_SIZE as usize];
    if memory.write_bytes(port, &cleared).is_none()
        || memory.write_u32_be(port + 2, bits.base_addr).is_none()
        || memory
            .write_u16_be(port + 6, bits.row_bytes as u16 & 0x3fff)
            .is_none()
        || ppc_write_rect(
            memory,
            port + 8,
            bits.top,
            bits.left,
            bits.bottom,
            bits.right,
        )
        .is_none()
        || ppc_write_rect(
            memory,
            port + 16,
            bits.top,
            bits.left,
            bits.bottom,
            bits.right,
        )
        .is_none()
        || memory.write_u32_be(port + 24, vis_rgn).is_none()
        || memory
            .write_u32_be(port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET, clip_rgn)
            .is_none()
        || ppc_write_rgn_bbox(
            memory,
            vis_rgn,
            bits.top,
            bits.left,
            bits.bottom,
            bits.right,
        )
        .is_none()
        || ppc_write_rgn_bbox(memory, clip_rgn, i16::MIN, i16::MIN, i16::MAX, i16::MAX).is_none()
        || memory.write_bytes(port + 32, &[0xff; 8]).is_none()
        || memory
            .write_u16_be(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 1)
            .is_none()
        || memory
            .write_u16_be(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, 1)
            .is_none()
        || memory
            .write_u16_be(
                port + PPC_CGRAF_PORT_PN_MODE_OFFSET,
                PPC_QD_PEN_MODE_PAT_COPY as u16,
            )
            .is_none()
        || memory
            .write_u16_be(
                port + PPC_CGRAF_PORT_TX_MODE_OFFSET,
                PPC_QD_TEXT_MODE_SRC_OR as u16,
            )
            .is_none()
    {
        *last_mem_error = PPC_PARAM_ERR;
        return false;
    }

    gworlds.retain(|gworld| gworld.port != port);
    gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle,
        pixmap: memory.read_u32_be(pixmap_handle).unwrap_or(PPC_MAIN_PIXMAP),
        base_addr: bits.base_addr,
        gdevice: current_gdevice,
        width: bits.width,
        height: bits.height,
        depth: bits.depth,
        row_bytes: bits.row_bytes,
        pixels_locked: false,
        pixels_no_purge: true,
    });
    *last_mem_error = PPC_NO_ERR;
    true
}

pub(crate) fn ppc_open_cport(
    cpu: &PpcCpu,
    allocator: &mut PpcProcessAllocatorView<'_>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    current_gdevice: u32,
) -> bool {
    let port = cpu.gpr[3];
    if port == 0 || !ppc_memory_can_write_bytes(memory, port, PPC_CGRAF_PORT_SIZE) {
        *last_mem_error = PPC_PARAM_ERR;
        return false;
    }

    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-64--4-66:
    // OpenCPort allocates its port PixMap and initializes it from the current
    // GDevice's PixMap, including its bounds, row bytes, depth, and base.
    let screen_pixmap_handle = memory
        .read_u32_be(current_gdevice)
        .and_then(|gdevice| memory.read_u32_be(gdevice.checked_add(22)?));
    let screen_color_table = screen_pixmap_handle
        .and_then(|pixmap_handle| memory.read_u32_be(pixmap_handle))
        .and_then(|pixmap| memory.read_u32_be(pixmap.checked_add(42)?))
        .filter(|handle| *handle != 0)
        .unwrap_or(PPC_MAIN_CTABLE_HANDLE);
    let screen_bits = screen_pixmap_handle
        .and_then(|pixmap_handle| ppc_read_pixmap_handle_bits(memory, pixmap_handle));
    let bits = screen_bits.unwrap_or(PpcPixMapBits {
        base_addr: PPC_MAIN_SCREEN_BASE,
        row_bytes: ppc_main_screen_row_bytes(),
        top: 0,
        left: 0,
        bottom: ppc_main_screen_height() as i16,
        right: ppc_main_screen_width() as i16,
        width: ppc_main_screen_width(),
        height: ppc_main_screen_height(),
        depth: PPC_MAIN_PIXEL_DEPTH,
    });
    if bits.row_bytes == 0 || !matches!(bits.depth, 1 | 2 | 4 | 8 | 16 | 32) {
        *last_mem_error = PPC_PARAM_ERR;
        return false;
    }
    if !ppc_heap_can_alloc_sequence(memory, *heap_cursor, heap_limit, &[PPC_PIXMAP_SIZE, 4]) {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return false;
    }
    let pixmap = allocator.reserve_bytes(memory, heap_cursor, PPC_PIXMAP_SIZE, true);
    let pixmap_handle = allocator.reserve_bytes(memory, heap_cursor, 4, true);
    let vis_rgn = ppc_allocator_view_new_rgn(
        Some(allocator),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    let clip_rgn = ppc_allocator_view_new_rgn(
        Some(allocator),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    if pixmap == 0
        || pixmap_handle == 0
        || vis_rgn == 0
        || clip_rgn == 0
        || memory.write_u32_be(pixmap_handle, pixmap).is_none()
        || ppc_write_pixmap(
            memory,
            pixmap,
            bits.base_addr,
            bits.row_bytes,
            bits.top,
            bits.left,
            bits.bottom,
            bits.right,
            bits.depth,
        )
        .is_none()
        || memory
            .write_u32_be(
                pixmap + 42,
                if bits.depth <= 8 {
                    screen_color_table
                } else {
                    0
                },
            )
            .is_none()
        || ppc_write_gworld_port(
            memory,
            port,
            pixmap_handle,
            bits.top,
            bits.left,
            bits.bottom,
            bits.right,
        )
        .is_none()
        || memory.write_u32_be(port + 24, vis_rgn).is_none()
        || memory
            .write_u32_be(port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET, clip_rgn)
            .is_none()
        || ppc_write_rgn_bbox(
            memory,
            vis_rgn,
            bits.top,
            bits.left,
            bits.bottom,
            bits.right,
        )
        .is_none()
        || ppc_write_rgn_bbox(memory, clip_rgn, i16::MIN, i16::MIN, i16::MAX, i16::MAX).is_none()
    {
        *last_mem_error = PPC_PARAM_ERR;
        return false;
    }

    let (width, height) = ppc_rect_dimensions(bits.top, bits.left, bits.bottom, bits.right);
    gworlds.retain(|gworld| gworld.port != port);
    gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle,
        pixmap,
        base_addr: bits.base_addr,
        gdevice: current_gdevice,
        width,
        height,
        depth: bits.depth,
        row_bytes: bits.row_bytes,
        pixels_locked: false,
        pixels_no_purge: true,
    });
    *last_mem_error = PPC_NO_ERR;
    true
}

pub(crate) fn ppc_close_cport(
    port: u32,
    gworlds: &mut Vec<PpcGWorldRecord>,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
) {
    // Inside Macintosh: Imaging With QuickDraw (1994), p. 4-67: CloseCPort
    // releases the structures owned by a CGrafPort while leaving caller-owned
    // port storage and the display device's color table intact.
    gworlds.retain(|gworld| {
        matches!(gworld.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD) || gworld.port != port
    });
    if *current_gworld == port {
        *current_gworld = PPC_MAIN_GWORLD;
        *current_gdevice = PPC_MAIN_GDEVICE;
    }
}

pub(crate) fn ppc_set_port_bits(
    memory: &mut PpcSectionMem,
    current_port: u32,
    bits_or_pixmap: u32,
    color: bool,
) {
    if current_port == 0 || bits_or_pixmap == 0 {
        return;
    }
    let is_color_port = memory
        .read_u16_be(current_port.wrapping_add(6))
        .is_some_and(|row_bytes| row_bytes & 0xc000 == 0xc000);
    if color {
        if is_color_port {
            let _ = memory.write_u32_be(current_port + 2, bits_or_pixmap);
        }
        return;
    }
    if is_color_port || !ppc_memory_can_write_bytes(memory, bits_or_pixmap, 14) {
        return;
    }
    if let Some(bitmap) = ppc_memory_read_bytes(memory, bits_or_pixmap, 14) {
        // Imaging With QuickDraw (1994), pp. 2-50 and 4-86--4-87:
        // SetPortBits copies the complete 14-byte BitMap into a basic port;
        // SetPortPix instead replaces the PixMap handle of a color port.
        let _ = memory.write_bytes(current_port + 2, &bitmap);
    }
}

pub(crate) fn ppc_color_table_bytes(seed: u32, colors: &[[u16; 3]]) -> Option<Vec<u8>> {
    let last_index = colors.len().checked_sub(1)?;
    let last_index = u16::try_from(last_index).ok()?;
    let mut bytes = Vec::with_capacity(8usize.checked_add(colors.len().checked_mul(8)?)?);
    bytes.extend_from_slice(&seed.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes());
    bytes.extend_from_slice(&last_index.to_be_bytes());
    for (index, [red, green, blue]) in colors.iter().copied().enumerate() {
        bytes.extend_from_slice(&u16::try_from(index).ok()?.to_be_bytes());
        bytes.extend_from_slice(&red.to_be_bytes());
        bytes.extend_from_slice(&green.to_be_bytes());
        bytes.extend_from_slice(&blue.to_be_bytes());
    }
    Some(bytes)
}

pub(crate) fn ppc_default_color_table_bytes(depth: u32) -> Option<Vec<u8>> {
    let colors = match depth {
        1 => vec![[0xffff, 0xffff, 0xffff], [0, 0, 0]],
        2 => vec![
            [0xffff, 0xffff, 0xffff],
            [0xaaaa, 0xaaaa, 0xaaaa],
            [0x5555, 0x5555, 0x5555],
            [0, 0, 0],
        ],
        4 => TrapDispatcher::standard_mac_4bpp_gworld_clut()[..16].to_vec(),
        8 => TrapDispatcher::standard_mac_8bpp_clut().to_vec(),
        _ => return None,
    };
    ppc_color_table_bytes(depth, &colors)
}

pub(crate) fn ppc_copy_color_table_bytes(memory: &mut PpcSectionMem, handle: u32) -> Option<Vec<u8>> {
    let table = memory.read_u32_be(handle).filter(|table| *table != 0)?;
    let last_index = usize::from(memory.read_u16_be(table.checked_add(6)?)?);
    if last_index >= 256 {
        return None;
    }
    let size = 8u32.checked_add(
        u32::try_from(last_index.checked_add(1)?)
            .ok()?
            .checked_mul(8)?,
    )?;
    ppc_memory_read_bytes(memory, table, size)
}

#[derive(Clone)]
pub(crate) struct PpcGWorldClut {
    pub(crate) colors: [[u16; 3]; 256],
    pub(crate) valid: [bool; 256],
}

pub(crate) fn ppc_color_table_clut_from_bytes(bytes: &[u8]) -> Option<PpcGWorldClut> {
    let flags = u16::from_be_bytes(bytes.get(4..6)?.try_into().ok()?);
    let last_entry = usize::from(u16::from_be_bytes(bytes.get(6..8)?.try_into().ok()?));
    let entry_count = last_entry.checked_add(1)?.min(256);
    let mut clut = [[0; 3]; 256];
    let mut valid = [false; 256];
    for slot in 0..entry_count {
        let offset = 8usize.checked_add(slot.checked_mul(8)?)?;
        let entry = bytes.get(offset..offset.checked_add(8)?)?;
        let value = usize::from(u16::from_be_bytes(entry.get(0..2)?.try_into().ok()?));
        let index = if flags & 0x8000 != 0 { slot } else { value };
        if index >= clut.len() {
            continue;
        }
        valid[index] = true;
        clut[index] = [
            u16::from_be_bytes(entry.get(2..4)?.try_into().ok()?),
            u16::from_be_bytes(entry.get(4..6)?.try_into().ok()?),
            u16::from_be_bytes(entry.get(6..8)?.try_into().ok()?),
        ];
    }
    Some(PpcGWorldClut {
        colors: clut,
        valid,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_new_gworld(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    gworld_allocations: &mut HashMap<u32, PpcGWorldAllocationRecord>,
    current_gdevice: u32,
) -> i16 {
    let gworld_out_ptr = cpu.gpr[3];
    let requested_depth = cpu.gpr[4] as u16 as i16;
    let bounds_ptr = cpu.gpr[5];
    let requested_ctab = cpu.gpr[6];
    let requested_gdevice = cpu.gpr[7];
    let flags = cpu.gpr[8];
    if gworld_out_ptr == 0 || bounds_ptr == 0 {
        *last_mem_error = PPC_PARAM_ERR;
        return PPC_PARAM_ERR;
    }

    let Some((requested_top, requested_left, requested_bottom, requested_right)) =
        ppc_read_rect(memory, bounds_ptr)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return PPC_PARAM_ERR;
    };
    let (width, height) = ppc_rect_dimensions(
        requested_top,
        requested_left,
        requested_bottom,
        requested_right,
    );
    const NO_NEW_DEVICE: u32 = 1 << 1;
    // Imaging With QuickDraw (1994), pp. 6-16--6-20: literal depths are
    // exactly 0, 1, 2, 4, 8, 16, and 32. A zero depth rescans screen devices
    // even when noNewDevice is present; this single-display runtime selects
    // the physical main device for both the source PixMap and attachment.
    let uses_explicit_gdevice =
        flags & NO_NEW_DEVICE != 0 && requested_gdevice != 0 && requested_depth != 0;
    if !matches!(requested_depth, 0 | 1 | 2 | 4 | 8 | 16 | 32) {
        *last_mem_error = PPC_C_DEPTH_ERR;
        return PPC_C_DEPTH_ERR;
    }
    let selected_gdevice = if uses_explicit_gdevice {
        requested_gdevice
    } else if requested_depth == 0 {
        PPC_MAIN_GDEVICE
    } else {
        current_gdevice
    };
    let device_pixmap = ppc_current_gdevice_record(memory, selected_gdevice)
        .and_then(|device| memory.read_u32_be(device.checked_add(22)?))
        .filter(|handle| *handle != 0)
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|pixmap| *pixmap != 0);
    if uses_explicit_gdevice && device_pixmap.is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return PPC_PARAM_ERR;
    }
    let device_depth = device_pixmap
        .and_then(|pixmap| memory.read_u16_be(pixmap + 32))
        .map(u32::from)
        .filter(|depth| *depth != 0);
    if (uses_explicit_gdevice || requested_depth == 0) && device_depth.is_none() {
        *last_mem_error = PPC_C_DEPTH_ERR;
        return PPC_C_DEPTH_ERR;
    }
    let depth = if uses_explicit_gdevice || requested_depth == 0 {
        device_depth.unwrap_or(PPC_MAIN_PIXEL_DEPTH)
    } else {
        requested_depth as u32
    };
    if !matches!(depth, 1 | 2 | 4 | 8 | 16 | 32) {
        *last_mem_error = PPC_C_DEPTH_ERR;
        return PPC_C_DEPTH_ERR;
    }
    let Some(row_bytes) = ppc_row_bytes(width, depth).filter(|row_bytes| *row_bytes <= 0x3fff)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return PPC_PARAM_ERR;
    };
    let Some(buffer_size) = row_bytes.checked_mul(height) else {
        return PPC_PARAM_ERR;
    };
    // QuickDraw owns a PixMap's pixel storage separately from the PixMap
    // record itself. Keep a trailing scanline in that storage allocation so
    // classic renderers that finish with aligned stores at the logical image
    // boundary cannot overwrite the following PixMap metadata.
    let Some(pixel_allocation_size) = buffer_size.checked_add(row_bytes.max(64)) else {
        return PPC_PARAM_ERR;
    };
    let ctable_bytes = if depth <= 8 {
        if requested_depth == 0 || uses_explicit_gdevice {
            ppc_gdevice_ctable_handle(memory, selected_gdevice)
                .and_then(|handle| ppc_copy_color_table_bytes(memory, handle))
        } else if requested_ctab != 0 {
            ppc_copy_color_table_bytes(memory, requested_ctab)
        } else {
            ppc_default_color_table_bytes(depth)
        }
    } else {
        None
    };
    if depth <= 8 && ctable_bytes.is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return PPC_PARAM_ERR;
    }

    let has_positive_bounds = requested_bottom > requested_top && requested_right > requested_left;
    let (top, left, bottom, right) = if !has_positive_bounds {
        (
            0,
            0,
            ppc_u32_to_i16_saturating(height),
            ppc_u32_to_i16_saturating(width),
        )
    } else {
        (
            requested_top,
            requested_left,
            requested_bottom,
            requested_right,
        )
    };
    if !ppc_memory_can_write_bytes(memory, gworld_out_ptr, 4) {
        *last_mem_error = PPC_PARAM_ERR;
        return PPC_PARAM_ERR;
    }

    let raw_allocation_size = [
        pixel_allocation_size,
        PPC_PIXMAP_SIZE,
        4,
        PPC_CGRAF_PORT_SIZE,
    ]
    .into_iter()
    .try_fold(0u32, |total, size| {
        total.checked_add(ppc_allocation_size(size)?)
    });
    let Some(raw_allocation_size) = raw_allocation_size else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return PPC_MEM_FULL_ERR;
    };
    // Imaging With QuickDraw (1994), pp. 6-12 and 6-16--6-20: an
    // offscreen world owns a PixMap and associated ColorTable. noNewDevice
    // reuses the requested GDevice record, not its mutable ColorTable handle;
    // snapshot that table so later screen palette changes do not reinterpret
    // pixels already rendered into the offscreen world.
    let heap_cursor_before = *heap_cursor;
    let native_manager_before = process_memory_manager.detached_clone();
    let ctable_handle = ctable_bytes
        .as_deref()
        .map(|bytes| process_memory_manager.copy_bytes_to_new_native_handle(memory, bytes))
        .unwrap_or(0);
    // Keep all nonrelocatable GWorld storage in one tracked pointer block so
    // DisposeGWorld can return a middle-of-heap allocation for later reuse.
    // Allocate it after the relocatable color table so an otherwise-current
    // world can still contract the bump heap when it is disposed.
    let base_addr = if depth <= 8 && ctable_handle == 0 {
        0
    } else {
        process_memory_manager.new_native_ptr(memory, raw_allocation_size, true)
    };
    let pixmap = base_addr.saturating_add(ppc_allocation_size(pixel_allocation_size).unwrap_or(0));
    let pixmap_handle = pixmap.saturating_add(ppc_allocation_size(PPC_PIXMAP_SIZE).unwrap_or(0));
    let port = pixmap_handle.saturating_add(ppc_allocation_size(4).unwrap_or(0));
    if base_addr == 0
        || pixmap == 0
        || pixmap_handle == 0
        || (depth <= 8 && ctable_handle == 0)
        || port == 0
    {
        if ctable_handle != 0 {
            let _ = memory.write_u32_be(ctable_handle, 0);
        }
        process_memory_manager.restore_native_snapshot(native_manager_before);
        process_memory_manager.set_native_mem_error(PPC_MEM_FULL_ERR);
        *heap_cursor = heap_cursor_before;
        ppc_update_zone_free_bytes(memory, heap_cursor_before, heap_limit);
        *last_mem_error = PPC_MEM_FULL_ERR;
        return PPC_MEM_FULL_ERR;
    }

    if memory.write_u32_be(pixmap_handle, pixmap).is_none()
        || ppc_write_pixmap(
            memory, pixmap, base_addr, row_bytes, top, left, bottom, right, depth,
        )
        .is_none()
        || memory.write_u32_be(pixmap + 42, ctable_handle).is_none()
        || ppc_write_gworld_port(memory, port, pixmap_handle, top, left, bottom, right).is_none()
        || memory.write_u32_be(gworld_out_ptr, port).is_none()
    {
        if ctable_handle != 0 {
            let _ = memory.write_u32_be(ctable_handle, 0);
        }
        process_memory_manager.restore_native_snapshot(native_manager_before);
        process_memory_manager.set_native_mem_error(PPC_PARAM_ERR);
        *heap_cursor = heap_cursor_before;
        ppc_update_zone_free_bytes(memory, heap_cursor_before, heap_limit);
        *last_mem_error = PPC_PARAM_ERR;
        return PPC_PARAM_ERR;
    }

    ppc_apply_process_native_allocator(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
    );
    if ctable_handle != 0 {
        ppc_apply_process_native_handle(
            process_memory_manager,
            handles,
            ctable_handle,
        );
    }
    process_memory_manager.set_native_mem_error(PPC_NO_ERR);
    *last_mem_error = PPC_NO_ERR;
    gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle,
        pixmap,
        base_addr,
        gdevice: if uses_explicit_gdevice {
            requested_gdevice
        } else {
            selected_gdevice
        },
        width,
        height,
        depth,
        row_bytes,
        pixels_locked: false,
        pixels_no_purge: true,
    });
    gworld_allocations.insert(
        port,
        PpcGWorldAllocationRecord {
            storage_ptr: base_addr,
            pixel_ptr: base_addr,
            origin_base: base_addr,
            pixel_capacity: ppc_allocation_size(pixel_allocation_size)
                .unwrap_or(pixel_allocation_size),
            ctable_handle,
            allocation_end: base_addr
                .checked_add(ppc_allocation_size(raw_allocation_size).unwrap_or(0))
                .filter(|end| *end == *heap_cursor)
                .unwrap_or(0),
        },
    );
    if ppc_gworld_trace_enabled() {
        eprintln!(
            "[PPC-GWORLD-TRACE] NewGWorld port=${:08X} out=${:08X} base=${:08X} pixmap=${:08X} handle=${:08X} ctable=${:08X} requested_depth={} requested_ctable=${:08X} requested_gdevice=${:08X} gdevice=${:08X} requested_bounds=({}, {}, {}, {}) bounds=({}, {}, {}, {}) size={}x{}x{} row_bytes={} flags=${:08X}",
            port,
            gworld_out_ptr,
            base_addr,
            pixmap,
            pixmap_handle,
            ctable_handle,
            requested_depth,
            requested_ctab,
            requested_gdevice,
            if uses_explicit_gdevice {
                requested_gdevice
            } else {
                selected_gdevice
            },
            requested_top,
            requested_left,
            requested_bottom,
            requested_right,
            top,
            left,
            bottom,
            right,
            width,
            height,
            depth,
            row_bytes,
            cpu.gpr[8]
        );
    }
    PPC_NO_ERR
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_update_gworld(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut [PpcGWorldRecord],
    gworld_allocations: &mut HashMap<u32, PpcGWorldAllocationRecord>,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
) -> u32 {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 6-23--6-26:
    // UpdateGWorld leaves the old world intact on error, reports structural
    // changes in its result flags, and preserves resized pixels only when the
    // caller requests clipPix or stretchPix.
    const MAP_PIX: u32 = 1 << 16;
    const NEW_DEPTH: u32 = 1 << 17;
    const ALIGN_PIX: u32 = 1 << 18;
    const NEW_ROW_BYTES: u32 = 1 << 19;
    const REALLOC_PIX: u32 = 1 << 20;
    const KEEP_LOCAL: u32 = 1 << 3;
    const CLIP_PIX: u32 = 1 << 28;
    const STRETCH_PIX: u32 = 1 << 29;
    const DITHER_PIX: u32 = 1 << 30;
    const GW_FLAG_ERR: u32 = 1 << 31;

    let gworld_ptr_ptr = cpu.gpr[3];
    let requested_depth = cpu.gpr[4] as u16 as i16;
    let bounds_ptr = cpu.gpr[5];
    let requested_ctab = cpu.gpr[6];
    let requested_gdevice = cpu.gpr[7];
    let requested_flags = cpu.gpr[8];
    let update_flags = requested_flags & (CLIP_PIX | STRETCH_PIX | DITHER_PIX);
    let clip_requested = update_flags & CLIP_PIX != 0;
    let stretch_requested = update_flags & STRETCH_PIX != 0;
    let dither_requested = update_flags & DITHER_PIX != 0;
    if requested_flags & !(KEEP_LOCAL | CLIP_PIX | STRETCH_PIX | DITHER_PIX) != 0
        || (clip_requested && stretch_requested)
        || (dither_requested && !clip_requested && !stretch_requested)
    {
        // Inside Macintosh Volume VI (1991), p. 21-16: the only legal pixel
        // update sets are none, clip, stretch, clip+dither, and stretch+dither.
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    }
    let updates_pixels = clip_requested || stretch_requested;

    let Some(port) = (gworld_ptr_ptr != 0 && bounds_ptr != 0)
        .then(|| memory.read_u32_be(gworld_ptr_ptr))
        .flatten()
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    };
    let Some(record_index) = gworlds
        .iter()
        .position(|record| record.port == port && port != PPC_MAIN_GWORLD)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    };
    let old = gworlds[record_index];
    let Some((requested_top, requested_left, requested_bottom, requested_right)) =
        ppc_read_rect(memory, bounds_ptr)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    };
    if requested_bottom <= requested_top || requested_right <= requested_left {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    }

    let device_pixmap_for = |memory: &mut PpcSectionMem, gdevice: u32| {
        memory
            .read_u32_be(gdevice)
            .filter(|device| *device != 0)
            .and_then(|device| memory.read_u32_be(device.checked_add(22)?))
            .filter(|pixmap_handle| *pixmap_handle != 0)
            .and_then(|pixmap_handle| memory.read_u32_be(pixmap_handle))
            .filter(|pixmap| *pixmap != 0)
    };
    if requested_gdevice == 0 && !matches!(requested_depth, 0 | 1 | 2 | 4 | 8 | 16 | 32) {
        *last_mem_error = PPC_C_DEPTH_ERR;
        return GW_FLAG_ERR;
    }
    let selected_screen_gdevice = if requested_gdevice != 0 {
        Some(requested_gdevice)
    } else if requested_depth == 0 {
        // Imaging With QuickDraw (1994), pp. 6-23--6-24: depth zero rescans
        // intersecting screen devices. This single-screen runtime selects the
        // physical main device for both the source PixMap and attachment.
        Some(PPC_MAIN_GDEVICE)
    } else {
        None
    };
    let selected_screen_pixmap =
        selected_screen_gdevice.and_then(|gdevice| device_pixmap_for(memory, gdevice));
    if requested_gdevice != 0 && selected_screen_pixmap.is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    }
    let device_depth = selected_screen_pixmap
        .and_then(|pixmap| memory.read_u16_be(pixmap + 32))
        .map(u32::from)
        .filter(|depth| *depth != 0);
    if (requested_gdevice != 0 || requested_depth == 0) && device_depth.is_none() {
        *last_mem_error = PPC_C_DEPTH_ERR;
        return GW_FLAG_ERR;
    }
    let depth = if requested_gdevice != 0 {
        device_depth.unwrap_or(PPC_MAIN_PIXEL_DEPTH)
    } else if requested_depth == 0 {
        device_depth.unwrap_or(PPC_MAIN_PIXEL_DEPTH)
    } else {
        requested_depth as u32
    };
    if !matches!(depth, 1 | 2 | 4 | 8 | 16 | 32) {
        *last_mem_error = PPC_C_DEPTH_ERR;
        return GW_FLAG_ERR;
    }

    let (top, left, bottom, right) = (
        requested_top,
        requested_left,
        requested_bottom,
        requested_right,
    );
    let (width, height) = ppc_rect_dimensions(top, left, bottom, right);
    let Some(row_bytes) = ppc_row_bytes(width, depth).filter(|row_bytes| *row_bytes <= 0x3fff)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    };
    let Some(buffer_size) = row_bytes.checked_mul(height) else {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    };
    let Some(pixel_allocation_size) = buffer_size.checked_add(row_bytes.max(64)) else {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    };
    if !ppc_memory_can_write_bytes(memory, old.pixmap, PPC_PIXMAP_SIZE)
        || !ppc_memory_can_write_bytes(memory, old.port + 16, 8)
    {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    }

    let old_bounds = ppc_read_rect(memory, old.pixmap + 6).unwrap_or((
        0,
        0,
        ppc_u32_to_i16_saturating(old.height),
        ppc_u32_to_i16_saturating(old.width),
    ));
    let old_ctab = memory.read_u32_be(old.pixmap + 42).unwrap_or(0);
    let desired_ctable_bytes = if depth <= 8 {
        if requested_gdevice != 0 || requested_depth == 0 {
            selected_screen_pixmap
                .and_then(|pixmap| memory.read_u32_be(pixmap + 42))
                .filter(|handle| *handle != 0)
                .and_then(|handle| ppc_copy_color_table_bytes(memory, handle))
        } else if requested_ctab != 0 {
            ppc_copy_color_table_bytes(memory, requested_ctab)
        } else {
            ppc_default_color_table_bytes(depth)
        }
    } else {
        None
    };
    if depth <= 8 && desired_ctable_bytes.is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    }
    let old_ctable_bytes = (old_ctab != 0)
        .then(|| ppc_copy_color_table_bytes(memory, old_ctab))
        .flatten();
    let ctable_changed = old_ctable_bytes != desired_ctable_bytes;
    let dimensions_changed = width != old.width || height != old.height;
    let maps_pixels = updates_pixels && depth <= 8 && ctable_changed;
    let translates_pixels = updates_pixels && (depth != old.depth || maps_pixels);
    let replaces_pixels = dimensions_changed
        || depth != old.depth
        || row_bytes != old.row_bytes
        || old.base_addr == 0
        || (ctable_changed && updates_pixels);
    let preserves_pixels = old.base_addr != 0 && updates_pixels;
    let dithers_pixels = dither_requested && preserves_pixels && translates_pixels && depth <= 8;
    let replaces_ctable = depth <= 8 && (ctable_changed || old_ctab == 0);

    let old_clut = if old.depth <= 8 && preserves_pixels && translates_pixels {
        let Some(bytes) = old_ctable_bytes.as_deref() else {
            *last_mem_error = PPC_PARAM_ERR;
            return GW_FLAG_ERR;
        };
        let Some(clut) = ppc_color_table_clut_from_bytes(bytes) else {
            *last_mem_error = PPC_PARAM_ERR;
            return GW_FLAG_ERR;
        };
        Some(clut)
    } else {
        None
    };
    let new_clut = if depth <= 8 && preserves_pixels && translates_pixels {
        let Some(bytes) = desired_ctable_bytes.as_deref() else {
            *last_mem_error = PPC_PARAM_ERR;
            return GW_FLAG_ERR;
        };
        let Some(clut) = ppc_color_table_clut_from_bytes(bytes) else {
            *last_mem_error = PPC_PARAM_ERR;
            return GW_FLAG_ERR;
        };
        Some(clut)
    } else {
        None
    };

    let allocation_before = gworld_allocations.get(&port).copied();
    let old_pixel_capacity = allocation_before.map_or_else(
        || {
            old.row_bytes
                .checked_mul(old.height)
                .and_then(ppc_allocation_size)
                .unwrap_or(0)
        },
        |allocation| allocation.pixel_capacity,
    );
    let Some(required_pixel_capacity) = ppc_allocation_size(pixel_allocation_size) else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return GW_FLAG_ERR;
    };
    let stable_ctable_handle = allocation_before
        .map(|allocation| allocation.ctable_handle)
        .filter(|handle| *handle != 0)
        .or_else(|| {
            handles
                .iter()
                .any(|record| record.handle == old_ctab && old_ctab != 0)
                .then_some(old_ctab)
        });
    let stable_ctable_record = stable_ctable_handle
        .and_then(|handle| handles.iter().find(|record| record.handle == handle))
        .copied();
    let desired_ctable_size = desired_ctable_bytes
        .as_ref()
        .and_then(|bytes| u32::try_from(bytes.len()).ok())
        .unwrap_or(0);
    let stable_ctable_resize_allocation = if replaces_ctable {
        if let Some(record) = stable_ctable_record {
            let Some(size) = ppc_handle_resize_allocation_size(
                memory,
                record,
                *heap_cursor,
                heap_limit,
                desired_ctable_size,
            ) else {
                *last_mem_error = PPC_MEM_FULL_ERR;
                return GW_FLAG_ERR;
            };
            size
        } else {
            0
        }
    } else {
        0
    };
    let allocates_ctable_handle = replaces_ctable && stable_ctable_record.is_none();
    let grows_pixel_storage_in_place = replaces_pixels
        && old.base_addr != 0
        && required_pixel_capacity > old_pixel_capacity
        && old.base_addr.checked_add(old_pixel_capacity) == Some(*heap_cursor)
        && ppc_heap_allocation_bounds(memory, old.base_addr, heap_limit, required_pixel_capacity)
            .is_some_and(|(base, _)| base == old.base_addr)
        && stable_ctable_resize_allocation == 0
        && !allocates_ctable_handle;
    let reuses_pixel_storage = replaces_pixels
        && old.base_addr != 0
        && (required_pixel_capacity <= old_pixel_capacity || grows_pixel_storage_in_place);
    let allocates_pixel_storage = replaces_pixels && !reuses_pixel_storage;

    let mut allocation_sizes = Vec::new();
    if stable_ctable_resize_allocation != 0 {
        allocation_sizes.push(stable_ctable_resize_allocation);
    } else if allocates_ctable_handle {
        allocation_sizes.push(4);
        allocation_sizes.push(desired_ctable_size);
    }
    if grows_pixel_storage_in_place {
        allocation_sizes.push(required_pixel_capacity - old_pixel_capacity);
    } else if allocates_pixel_storage {
        allocation_sizes.push(pixel_allocation_size);
    }
    if !allocation_sizes.is_empty()
        && !ppc_heap_can_alloc_sequence(memory, *heap_cursor, heap_limit, &allocation_sizes)
    {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return GW_FLAG_ERR;
    }

    let staged_pixels = if replaces_pixels {
        let Ok(buffer_len) = usize::try_from(buffer_size) else {
            *last_mem_error = PPC_MEM_FULL_ERR;
            return GW_FLAG_ERR;
        };
        let staged = if preserves_pixels {
            ppc_stage_update_gworld_pixels(
                memory,
                old,
                row_bytes,
                width,
                height,
                depth,
                stretch_requested,
                translates_pixels,
                dithers_pixels,
                old_clut.as_ref(),
                new_clut.as_ref(),
            )
        } else {
            let mut bytes = Vec::new();
            if bytes.try_reserve_exact(buffer_len).is_err() {
                None
            } else {
                bytes.resize(buffer_len, 0);
                Some(bytes)
            }
        };
        let Some(staged) = staged else {
            *last_mem_error = PPC_PARAM_ERR;
            return GW_FLAG_ERR;
        };
        Some(staged)
    } else {
        None
    };

    let Some(old_pixmap_snapshot) = ppc_memory_read_bytes(memory, old.pixmap, PPC_PIXMAP_SIZE)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    };
    let Some(old_port_rect_snapshot) = ppc_memory_read_bytes(memory, old.port + 16, 8) else {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    };
    if !ppc_memory_can_write_bytes(memory, old.pixmap, PPC_PIXMAP_SIZE)
        || !ppc_memory_can_write_bytes(memory, old.port + 16, 8)
    {
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    }

    let vis_region_snapshot = memory
        .read_u32_be(old.port + PPC_CGRAF_PORT_VIS_RGN_OFFSET)
        .and_then(|handle| ppc_rgn_ptr(memory, handle))
        .and_then(|ptr| ppc_memory_read_bytes(memory, ptr, 10).map(|bytes| (ptr, bytes)))
        .filter(|(ptr, _)| ppc_memory_can_write_bytes(memory, *ptr, 10));
    let new_gdevice = if requested_gdevice != 0 {
        requested_gdevice
    } else if requested_depth == 0 {
        selected_screen_gdevice.unwrap_or(PPC_MAIN_GDEVICE)
    } else if old.gdevice != 0 {
        old.gdevice
    } else {
        *current_gdevice
    };

    let reused_pixel_snapshot = if reuses_pixel_storage {
        let snapshot_size = if grows_pixel_storage_in_place {
            old_pixel_capacity
        } else {
            buffer_size
        };
        if !ppc_memory_can_write_bytes(memory, old.base_addr, snapshot_size) {
            *last_mem_error = PPC_PARAM_ERR;
            return GW_FLAG_ERR;
        }
        let Some(bytes) = ppc_memory_read_bytes(memory, old.base_addr, snapshot_size) else {
            *last_mem_error = PPC_PARAM_ERR;
            return GW_FLAG_ERR;
        };
        Some(bytes)
    } else {
        None
    };
    let stable_ctable_snapshot = if replaces_ctable {
        if let Some(record) = stable_ctable_record {
            let Some(master) = memory.read_u32_be(record.handle) else {
                *last_mem_error = PPC_PARAM_ERR;
                return GW_FLAG_ERR;
            };
            if master != record.ptr {
                *last_mem_error = PPC_PARAM_ERR;
                return GW_FLAG_ERR;
            }
            let Some(bytes) = ppc_memory_read_bytes(memory, record.ptr, record.size) else {
                *last_mem_error = PPC_PARAM_ERR;
                return GW_FLAG_ERR;
            };
            if desired_ctable_size <= record.capacity
                && !ppc_memory_can_write_bytes(memory, record.ptr, desired_ctable_size)
            {
                *last_mem_error = PPC_PARAM_ERR;
                return GW_FLAG_ERR;
            }
            Some((record, master, bytes))
        } else {
            None
        }
    } else {
        None
    };

    // Allocate only after every source pixel, destination byte, metadata
    // target, and allocation has been preflighted. Pixel bytes are staged on
    // the host, and the world's private ColorTable keeps one stable handle;
    // rollback restores both if an already-preflighted guest write fails.
    let heap_cursor_before = *heap_cursor;
    let native_manager_before = process_memory_manager.detached_clone();
    let mut allocation_error = PPC_NO_ERR;
    let mut owned_ctable_handle = stable_ctable_handle.unwrap_or(0);
    let new_ctab = if depth > 8 {
        0
    } else if replaces_ctable {
        if let Some(record) = stable_ctable_record {
            let result = process_memory_manager.set_native_handle_size(
                memory,
                record.handle,
                desired_ctable_size,
            );
            if result != PPC_NO_ERR {
                allocation_error = result;
                0
            } else if let Some(table) = memory
                .read_u32_be(record.handle)
                .filter(|table| *table != 0)
            {
                if memory
                    .write_bytes(table, desired_ctable_bytes.as_deref().unwrap_or_default())
                    .is_none()
                {
                    allocation_error = PPC_PARAM_ERR;
                    0
                } else {
                    owned_ctable_handle = record.handle;
                    record.handle
                }
            } else {
                allocation_error = PPC_PARAM_ERR;
                0
            }
        } else {
            let handle =
                process_memory_manager.new_native_handle(memory, desired_ctable_size, false);
            if handle == 0 {
                allocation_error = PPC_MEM_FULL_ERR;
            } else if process_memory_manager
                .native_allocation(handle)
                .and_then(|record| {
                    memory.write_bytes(
                        record.ptr,
                        desired_ctable_bytes.as_deref().unwrap_or_default(),
                    )
                })
                .is_none()
            {
                allocation_error = PPC_PARAM_ERR;
            } else {
                owned_ctable_handle = handle;
            }
            handle
        }
    } else {
        old_ctab
    };
    let new_base = if allocation_error != PPC_NO_ERR || !replaces_pixels {
        old.base_addr
    } else if reuses_pixel_storage {
        if grows_pixel_storage_in_place {
            allocation_error = process_memory_manager.set_native_ptr_size(
                memory,
                old.base_addr,
                pixel_allocation_size,
            );
        }
        if allocation_error == PPC_NO_ERR
            && memory
                .write_bytes(old.base_addr, staged_pixels.as_deref().unwrap_or_default())
                .is_none()
        {
            allocation_error = PPC_PARAM_ERR;
        }
        old.base_addr
    } else {
        let base = process_memory_manager.new_native_ptr(memory, pixel_allocation_size, true);
        if base == 0 {
            allocation_error = PPC_MEM_FULL_ERR;
        } else if memory
            .write_bytes(base, staged_pixels.as_deref().unwrap_or_default())
            .is_none()
        {
            allocation_error = PPC_PARAM_ERR;
        }
        base
    };
    if allocation_error != PPC_NO_ERR {
        if let Some(bytes) = reused_pixel_snapshot.as_ref() {
            let _ = memory.write_bytes(old.base_addr, bytes);
        }
        if let Some((record, master, bytes)) = stable_ctable_snapshot.as_ref() {
            let _ = memory.write_u32_be(record.handle, *master);
            let _ = memory.write_bytes(record.ptr, bytes);
        }
        process_memory_manager.restore_native_snapshot(native_manager_before);
        *heap_cursor = heap_cursor_before;
        ppc_update_zone_free_bytes(memory, heap_cursor_before, heap_limit);
        *last_mem_error = allocation_error;
        return GW_FLAG_ERR;
    }

    let mut result = 0;
    if maps_pixels {
        result |= MAP_PIX;
    }
    if translates_pixels && depth != old.depth {
        result |= NEW_DEPTH;
    }
    if updates_pixels
        && width == old.width
        && height == old.height
        && old_bounds != (top, left, bottom, right)
    {
        result |= ALIGN_PIX;
    }
    if row_bytes != old.row_bytes {
        result |= NEW_ROW_BYTES;
    }
    if allocates_pixel_storage {
        result |= REALLOC_PIX;
    }
    if dimensions_changed {
        if stretch_requested {
            result |= STRETCH_PIX;
        } else if clip_requested {
            result |= CLIP_PIX;
        }
    }
    if dithers_pixels {
        result |= DITHER_PIX;
    }

    let (pixel_type, component_count, component_size) = match depth {
        1 | 2 | 4 | 8 => (0, 1, depth as u16),
        16 => (16, 3, 5),
        32 => (16, 3, 8),
        _ => unreachable!("validated GWorld depth"),
    };
    let mut write_ok = memory.write_u32_be(old.pixmap, new_base).is_some()
        && memory
            .write_u16_be(old.pixmap + 4, (row_bytes as u16) | 0x8000)
            .is_some()
        && ppc_write_rect(memory, old.pixmap + 6, top, left, bottom, right).is_some()
        && memory.write_u16_be(old.pixmap + 30, pixel_type).is_some()
        && memory.write_u16_be(old.pixmap + 32, depth as u16).is_some()
        && memory
            .write_u16_be(old.pixmap + 34, component_count)
            .is_some()
        && memory
            .write_u16_be(old.pixmap + 36, component_size)
            .is_some()
        && memory.write_u32_be(old.pixmap + 38, 0).is_some()
        && memory.write_u32_be(old.pixmap + 42, new_ctab).is_some()
        && ppc_write_rect(memory, old.port + 16, top, left, bottom, right).is_some();
    if let Some((ptr, _)) = vis_region_snapshot.as_ref() {
        write_ok &= memory.write_u16_be(*ptr, 10).is_some()
            && ppc_write_rect(memory, *ptr + 2, top, left, bottom, right).is_some();
    }
    if !write_ok {
        let _ = memory.write_bytes(old.pixmap, &old_pixmap_snapshot);
        let _ = memory.write_bytes(old.port + 16, &old_port_rect_snapshot);
        if let Some((ptr, bytes)) = vis_region_snapshot.as_ref() {
            let _ = memory.write_bytes(*ptr, bytes);
        }
        if let Some(bytes) = reused_pixel_snapshot.as_ref() {
            let _ = memory.write_bytes(old.base_addr, bytes);
        }
        if let Some((record, master, bytes)) = stable_ctable_snapshot.as_ref() {
            let _ = memory.write_u32_be(record.handle, *master);
            let _ = memory.write_bytes(record.ptr, bytes);
        }
        process_memory_manager.restore_native_snapshot(native_manager_before);
        *heap_cursor = heap_cursor_before;
        ppc_update_zone_free_bytes(memory, heap_cursor_before, heap_limit);
        *last_mem_error = PPC_PARAM_ERR;
        return GW_FLAG_ERR;
    }

    if allocates_pixel_storage {
        if let Some(previous) = allocation_before.filter(|previous| {
            previous.pixel_ptr != 0 && previous.pixel_ptr != previous.storage_ptr
        }) {
            let _ = process_memory_manager.dispose_native_ptr(previous.pixel_ptr);
        }
    }
    ppc_apply_process_native_allocator(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
    );
    if owned_ctable_handle != 0 {
        ppc_apply_process_native_handle(
            process_memory_manager,
            handles,
            owned_ctable_handle,
        );
    }

    gworlds[record_index] = PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        base_addr: new_base,
        gdevice: new_gdevice,
        width,
        height,
        depth,
        row_bytes,
        ..old
    };
    let heap_grew = *heap_cursor != heap_cursor_before;
    let pixel_capacity = if allocates_pixel_storage || grows_pixel_storage_in_place {
        required_pixel_capacity
    } else {
        old_pixel_capacity
    };
    let allocation_after = if let Some(previous) = allocation_before {
        if heap_grew {
            PpcGWorldAllocationRecord {
                storage_ptr: previous.storage_ptr,
                pixel_ptr: new_base,
                origin_base: if previous.allocation_end == heap_cursor_before {
                    previous.origin_base
                } else {
                    heap_cursor_before
                },
                pixel_capacity,
                ctable_handle: owned_ctable_handle,
                allocation_end: *heap_cursor,
            }
        } else {
            PpcGWorldAllocationRecord {
                pixel_ptr: new_base,
                pixel_capacity,
                ctable_handle: owned_ctable_handle,
                ..previous
            }
        }
    } else {
        PpcGWorldAllocationRecord {
            storage_ptr: 0,
            pixel_ptr: new_base,
            origin_base: if heap_grew { heap_cursor_before } else { 0 },
            pixel_capacity,
            ctable_handle: owned_ctable_handle,
            allocation_end: if heap_grew { *heap_cursor } else { 0 },
        }
    };
    gworld_allocations.insert(port, allocation_after);
    if *current_gworld == port {
        *current_gworld = port;
        *current_gdevice = new_gdevice;
    }
    *last_mem_error = PPC_NO_ERR;
    if ppc_gworld_trace_enabled() {
        eprintln!(
            "[PPC-GWORLD-TRACE] UpdateGWorld port=${port:08X} base=${:08X}->${new_base:08X} bounds=({}, {}, {}, {})->({}, {}, {}, {}) size={}x{}x{} row_bytes={} flags=${requested_flags:08X} result=${result:08X}",
            old.base_addr,
            old_bounds.0,
            old_bounds.1,
            old_bounds.2,
            old_bounds.3,
            top,
            left,
            bottom,
            right,
            width,
            height,
            depth,
            row_bytes,
        );
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_stage_update_gworld_pixels(
    memory: &mut PpcSectionMem,
    old: PpcGWorldRecord,
    new_row_bytes: u32,
    new_width: u32,
    new_height: u32,
    new_depth: u32,
    stretch: bool,
    translate: bool,
    dither: bool,
    old_clut: Option<&PpcGWorldClut>,
    new_clut: Option<&PpcGWorldClut>,
) -> Option<Vec<u8>> {
    let buffer_size = new_row_bytes.checked_mul(new_height)?;
    let buffer_len = usize::try_from(buffer_size).ok()?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(buffer_len).ok()?;
    pixels.resize(buffer_len, 0);
    let error_width = usize::try_from(new_width).ok()?.checked_add(2)?;
    let mut current_errors = Vec::new();
    let mut next_errors = Vec::new();
    if dither {
        current_errors.try_reserve_exact(error_width).ok()?;
        next_errors.try_reserve_exact(error_width).ok()?;
        current_errors.resize(error_width, [0i32; 3]);
        next_errors.resize(error_width, [0i32; 3]);
    }
    for dst_y in 0..new_height {
        let src_y = if stretch {
            u64::from(dst_y)
                .checked_mul(u64::from(old.height))?
                .checked_div(u64::from(new_height))? as u32
        } else {
            if dst_y >= old.height {
                continue;
            }
            dst_y
        };
        for dst_x in 0..new_width {
            let src_x = if stretch {
                u64::from(dst_x)
                    .checked_mul(u64::from(old.width))?
                    .checked_div(u64::from(new_width))? as u32
            } else {
                if dst_x >= old.width {
                    continue;
                }
                dst_x
            };
            let raw = ppc_read_gworld_record_raw_pixel(memory, old, src_x, src_y)?;
            let pixel = if dither {
                let rgb = ppc_gworld_pixel_to_rgb(raw, old.depth, old_clut)?;
                let error_index = usize::try_from(dst_x).ok()?.checked_add(1)?;
                let error = *current_errors.get(error_index)?;
                let adjusted = PpcRgbColor {
                    red: u16::try_from((i32::from(rgb.red) + error[0]).clamp(0, 0xffff)).ok()?,
                    green: u16::try_from((i32::from(rgb.green) + error[1]).clamp(0, 0xffff))
                        .ok()?,
                    blue: u16::try_from((i32::from(rgb.blue) + error[2]).clamp(0, 0xffff)).ok()?,
                };
                let pixel = ppc_gworld_rgb_to_pixel(adjusted, new_depth, new_clut)?;
                let palette_index = usize::try_from(pixel).ok()?;
                let palette = new_clut?;
                let [red, green, blue] = *palette.colors.get(palette_index)?;
                let quantization_error = [
                    i32::from(adjusted.red) - i32::from(red),
                    i32::from(adjusted.green) - i32::from(green),
                    i32::from(adjusted.blue) - i32::from(blue),
                ];
                for component in 0..3 {
                    current_errors[error_index + 1][component] +=
                        quantization_error[component] * 7 / 16;
                    next_errors[error_index - 1][component] +=
                        quantization_error[component] * 3 / 16;
                    next_errors[error_index][component] += quantization_error[component] * 5 / 16;
                    next_errors[error_index + 1][component] += quantization_error[component] / 16;
                }
                pixel
            } else if translate {
                let rgb = ppc_gworld_pixel_to_rgb(raw, old.depth, old_clut)?;
                ppc_gworld_rgb_to_pixel(rgb, new_depth, new_clut)?
            } else {
                raw
            };
            ppc_write_gworld_staged_pixel(
                &mut pixels,
                new_row_bytes,
                new_width,
                new_height,
                new_depth,
                dst_x,
                dst_y,
                pixel,
            )?;
        }
        if dither {
            std::mem::swap(&mut current_errors, &mut next_errors);
            next_errors.fill([0; 3]);
        }
    }
    Some(pixels)
}

pub(crate) fn ppc_read_gworld_record_raw_pixel(
    memory: &mut PpcSectionMem,
    record: PpcGWorldRecord,
    x: u32,
    y: u32,
) -> Option<u32> {
    if x >= record.width || y >= record.height {
        return None;
    }
    let row = record
        .base_addr
        .checked_add(y.checked_mul(record.row_bytes)?)?;
    match record.depth {
        1 | 2 | 4 => {
            let bit_offset = x.checked_mul(record.depth)?;
            let byte_offset = bit_offset / 8;
            if byte_offset >= record.row_bytes {
                return None;
            }
            let shift = 8u32
                .checked_sub(record.depth)?
                .checked_sub(bit_offset & 7)?;
            let mask = (1u8 << record.depth) - 1;
            memory
                .read_u8(row.checked_add(byte_offset)?)
                .map(|byte| u32::from((byte >> shift) & mask))
        }
        8 => {
            if x >= record.row_bytes {
                return None;
            }
            memory.read_u8(row.checked_add(x)?).map(u32::from)
        }
        16 => {
            let byte_offset = x.checked_mul(2)?;
            if byte_offset.checked_add(2)? > record.row_bytes {
                return None;
            }
            memory
                .read_u16_be(row.checked_add(byte_offset)?)
                .map(u32::from)
        }
        32 => {
            let byte_offset = x.checked_mul(4)?;
            if byte_offset.checked_add(4)? > record.row_bytes {
                return None;
            }
            memory.read_u32_be(row.checked_add(byte_offset)?)
        }
        _ => None,
    }
}

pub(crate) fn ppc_gworld_pixel_to_rgb(
    pixel: u32,
    depth: u32,
    clut: Option<&PpcGWorldClut>,
) -> Option<PpcRgbColor> {
    // Imaging With QuickDraw (1994), pp. 4-13--4-16: indexed pixels name a
    // ColorTable entry; 16-bit direct pixels use 5:5:5 RGB; 32-bit direct
    // pixels store the high byte of each RGBColor component in 0x00RRGGBB.
    match depth {
        depth @ (1 | 2 | 4 | 8) => {
            let index = usize::try_from(pixel).ok()?;
            if index >= ppc_indexed_depth_entry_count(depth)? {
                return None;
            }
            let clut = clut?;
            if !clut.valid[index] {
                return None;
            }
            let [red, green, blue] = clut.colors[index];
            Some(PpcRgbColor { red, green, blue })
        }
        16 => {
            let [red, green, blue] = ppc_rgb555_to_rgb16(pixel as u16);
            Some(PpcRgbColor { red, green, blue })
        }
        32 => Some(PpcRgbColor {
            red: (((pixel >> 16) & 0xff) as u16) * 0x0101,
            green: (((pixel >> 8) & 0xff) as u16) * 0x0101,
            blue: ((pixel & 0xff) as u16) * 0x0101,
        }),
        _ => None,
    }
}

pub(crate) fn ppc_gworld_rgb_to_pixel(
    color: PpcRgbColor,
    depth: u32,
    clut: Option<&PpcGWorldClut>,
) -> Option<u32> {
    match depth {
        depth @ (1 | 2 | 4 | 8) => {
            let clut = clut?;
            ppc_rgb_color_to_valid_index_in_clut(
                color,
                &clut.colors,
                &clut.valid,
                ppc_indexed_depth_entry_count(depth)?,
            )
            .map(u32::from)
        }
        16 => Some(u32::from(
            ((color.red >> 11) << 10) | ((color.green >> 11) << 5) | (color.blue >> 11),
        )),
        32 => Some(
            (u32::from(color.red >> 8) << 16)
                | (u32::from(color.green >> 8) << 8)
                | u32::from(color.blue >> 8),
        ),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_write_gworld_staged_pixel(
    pixels: &mut [u8],
    row_bytes: u32,
    width: u32,
    height: u32,
    depth: u32,
    x: u32,
    y: u32,
    pixel: u32,
) -> Option<()> {
    if x >= width || y >= height {
        return None;
    }
    let row = usize::try_from(y.checked_mul(row_bytes)?).ok()?;
    match depth {
        1 | 2 | 4 => {
            let bit_offset = x.checked_mul(depth)?;
            let byte_offset = usize::try_from(bit_offset / 8).ok()?;
            if u32::try_from(byte_offset).ok()? >= row_bytes {
                return None;
            }
            let shift = 8u32.checked_sub(depth)?.checked_sub(bit_offset & 7)?;
            let value_mask = (1u8 << depth) - 1;
            let field_mask = value_mask << shift;
            let byte = pixels.get_mut(row.checked_add(byte_offset)?)?;
            *byte = (*byte & !field_mask) | (((pixel as u8) & value_mask) << shift);
        }
        8 => {
            if x >= row_bytes {
                return None;
            }
            *pixels.get_mut(row.checked_add(usize::try_from(x).ok()?)?)? = pixel as u8;
        }
        16 => {
            let byte_offset = x.checked_mul(2)?;
            if byte_offset.checked_add(2)? > row_bytes {
                return None;
            }
            let offset = row.checked_add(usize::try_from(byte_offset).ok()?)?;
            pixels
                .get_mut(offset..offset.checked_add(2)?)?
                .copy_from_slice(&(pixel as u16).to_be_bytes());
        }
        32 => {
            let byte_offset = x.checked_mul(4)?;
            if byte_offset.checked_add(4)? > row_bytes {
                return None;
            }
            let offset = row.checked_add(usize::try_from(byte_offset).ok()?)?;
            pixels
                .get_mut(offset..offset.checked_add(4)?)?
                .copy_from_slice(&pixel.to_be_bytes());
        }
        _ => return None,
    }
    Some(())
}

pub(crate) fn ppc_gworld_device(gworlds: &[PpcGWorldRecord], port: u32) -> Option<u32> {
    gworlds
        .iter()
        .find(|gworld| gworld.port == port)
        .map(|gworld| gworld.gdevice)
}

pub(crate) fn ppc_gworld_pixmap(memory: &mut PpcSectionMem, gworlds: &[PpcGWorldRecord], gworld: u32) -> u32 {
    if let Some(record) = gworlds.iter().find(|record| record.port == gworld) {
        return record.pixmap_handle;
    }
    if gworld == 0 {
        return 0;
    }
    memory.read_u32_be(gworld + 2).unwrap_or(0)
}

pub(crate) fn ppc_pix_base_addr(memory: &mut PpcSectionMem, pixmap_handle: u32) -> u32 {
    if pixmap_handle == 0 {
        return 0;
    }
    let Some(pixmap) = memory.read_u32_be(pixmap_handle) else {
        return 0;
    };
    if pixmap == 0 {
        return 0;
    }
    memory.read_u32_be(pixmap).unwrap_or(0)
}

pub(crate) fn ppc_legacy_qd_color_to_rgb(color: u32) -> PpcRgbColor {
    match color {
        30 => PPC_RGB_WHITE,
        33 => PPC_RGB_BLACK,
        69 => PpcRgbColor {
            red: 0xfc00,
            green: 0xf37d,
            blue: 0x052f,
        },
        137 => PpcRgbColor {
            red: 0xf2d7,
            green: 0x0856,
            blue: 0x84ec,
        },
        205 => PpcRgbColor {
            red: 0xdd6b,
            green: 0x08c2,
            blue: 0x06a2,
        },
        273 => PpcRgbColor {
            red: 0x0241,
            green: 0xab54,
            blue: 0xeaff,
        },
        341 => PpcRgbColor {
            red: 0,
            green: 0x8000,
            blue: 0x11b0,
        },
        409 => PpcRgbColor {
            red: 0,
            green: 0,
            blue: 0xd400,
        },
        _ => PPC_RGB_BLACK,
    }
}

pub(crate) fn ppc_rgb_color_to_rgb555(color: PpcRgbColor) -> u16 {
    fn component(value: u16) -> u16 {
        (((u32::from(value) * 31) + 32_767) / 65_535) as u16
    }
    (component(color.red) << 10) | (component(color.green) << 5) | component(color.blue)
}



pub(crate) fn ppc_write_gworld_port(
    memory: &mut PpcSectionMem,
    port: u32,
    pixmap_handle: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
) -> Option<()> {
    memory.write_u16_be(port, 0)?;
    memory.write_u32_be(port + 2, pixmap_handle)?;
    memory.write_u16_be(port + 6, 0xc000)?;
    ppc_write_rect(memory, port + 16, top, left, bottom, right)?;
    ppc_write_rgb_color(
        memory,
        port + PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
        PPC_RGB_BLACK,
    )?;
    ppc_write_rgb_color(
        memory,
        port + PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET,
        PPC_RGB_WHITE,
    )?;
    memory.write_u16_be(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 1)?;
    memory.write_u16_be(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, 1)?;
    ppc_write_rgb_color(
        memory,
        port + PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
        PPC_RGB_BLACK,
    )?;
    ppc_write_rgb_color(
        memory,
        port + PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET,
        PPC_RGB_WHITE,
    )?;
    memory.write_u16_be(
        port + PPC_CGRAF_PORT_PN_MODE_OFFSET,
        PPC_QD_PEN_MODE_PAT_COPY as u16,
    )?;
    memory.write_u16_be(
        port + PPC_CGRAF_PORT_TX_FONT_OFFSET,
        PPC_QD_TEXT_FONT_DEFAULT as u16,
    )?;
    memory.write_u16_be(port + PPC_CGRAF_PORT_TX_FACE_OFFSET, 0)?;
    memory.write_u16_be(
        port + PPC_CGRAF_PORT_TX_MODE_OFFSET,
        PPC_QD_TEXT_MODE_SRC_OR as u16,
    )?;
    memory.write_u16_be(
        port + PPC_CGRAF_PORT_TX_SIZE_OFFSET,
        PPC_QD_TEXT_SIZE_SYSTEM as u16,
    )?;
    Some(())
}

pub(crate) fn ppc_write_gdevice(
    memory: &mut PpcSectionMem,
    gdevice: u32,
    pixmap_handle: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
) -> Option<()> {
    let depth = memory
        .read_u32_be(pixmap_handle)
        .filter(|pixmap| *pixmap != 0)
        .and_then(|pixmap| memory.read_u16_be(pixmap + 32))
        .map(u32::from)
        .unwrap_or(PPC_MAIN_PIXEL_DEPTH);
    let gd_type = if depth <= 8 { 0 } else { 2 };
    let depth_mode = crate::display::classic_depth_mode(u16::try_from(depth).ok()?)?;
    memory.write_u16_be(gdevice, 0)?;
    memory.write_u16_be(gdevice + 2, 0)?;
    memory.write_u16_be(gdevice + 4, gd_type)?;
    memory.write_u16_be(gdevice + 10, 4)?;
    memory.write_u16_be(gdevice + 20, PPC_MAIN_GDEVICE_FLAGS)?;
    memory.write_u32_be(gdevice + 22, pixmap_handle)?;
    ppc_write_rect(memory, gdevice + 34, top, left, bottom, right)?;
    memory.write_u32_be(gdevice + 42, u32::from(depth_mode))?;
    Some(())
}

pub(crate) fn ppc_test_device_attribute(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    let gdevice_handle = cpu.gpr[3];
    let attribute = cpu.gpr[4] & 0xffff;
    let Some(bit) = 1u16.checked_shl(attribute) else {
        return 0;
    };
    let Some(gdevice) = memory.read_u32_be(gdevice_handle) else {
        return 0;
    };
    if gdevice == 0 {
        return 0;
    }
    let flags = memory.read_u16_be(gdevice + 20).unwrap_or(0);
    if flags & bit != 0 {
        0xffff
    } else {
        0
    }
}

pub(crate) fn ppc_set_device_attribute(cpu: &PpcCpu, memory: &mut PpcSectionMem) {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 5-22--5-23:
    // SetDeviceAttribute changes the requested gdFlags bit to the supplied
    // Boolean value on the GDevice record referenced by the handle.
    let gdevice_handle = cpu.gpr[3];
    let attribute = cpu.gpr[4] & 0xffff;
    let Some(bit) = 1u16.checked_shl(attribute) else {
        return;
    };
    let Some(gdevice) = memory
        .read_u32_be(gdevice_handle)
        .filter(|gdevice| *gdevice != 0)
    else {
        return;
    };
    let Some(flags) = memory.read_u16_be(gdevice + 20) else {
        return;
    };
    let flags = if cpu.gpr[5] & 0xff != 0 {
        flags | bit
    } else {
        flags & !bit
    };
    let _ = memory.write_u16_be(gdevice + 20, flags);
}

pub(crate) fn ppc_main_gdevice_record_for_depth(
    memory: &mut PpcSectionMem,
    requested_gdevice: u32,
) -> Option<u32> {
    // Classic applications commonly use NIL to mean the current/main screen.
    // Every non-NIL handle must be the actual single display handle; never
    // fall back to the main record after a failed guest dereference.
    let gdevice_handle = if requested_gdevice == 0 {
        PPC_MAIN_GDEVICE
    } else {
        requested_gdevice
    };
    if gdevice_handle != PPC_MAIN_GDEVICE {
        return None;
    }
    (memory.read_u32_be(gdevice_handle) == Some(PPC_MAIN_GDEVICE_RECORD))
        .then_some(PPC_MAIN_GDEVICE_RECORD)
}

pub(crate) fn ppc_supported_depth_mode(
    depth_or_mode: u32,
    which_flags: u32,
    flags: u32,
    current_flags: u16,
) -> Option<(u32, bool)> {
    let depth = u32::from(crate::display::classic_pixel_size(
        (depth_or_mode & 0xffff) as u16,
    )?);
    if !matches!(depth, 1 | 2 | 4 | 8 | 16) {
        return None;
    }
    // Imaging With QuickDraw (1994), pp. 5-33--5-35: bit gdDevType in
    // whichFlags selects the black-and-white/grayscale or color personality.
    // Indexed hardware exposes grayscale at 1/2/4/8 bits and color at 2/4/8
    // bits; the modeled 16-bit direct mode is color-only. With no gdDevType
    // constraint, preserve the current indexed personality and use the only
    // coherent personality at the two endpoint depths.
    let mode_is_color = if which_flags & 1 != 0 {
        flags & 1 != 0
    } else {
        match depth {
            1 => false,
            16 => true,
            _ => current_flags & 1 != 0,
        }
    };
    if (depth == 1 && mode_is_color) || (depth == 16 && !mode_is_color) {
        return None;
    }
    let fixed_mask = which_flags as u16 & !1;
    if current_flags & fixed_mask != flags as u16 & fixed_mask {
        return None;
    }
    Some((depth, mode_is_color))
}

pub(crate) fn ppc_has_depth(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    let Some(gdevice) = ppc_main_gdevice_record_for_depth(memory, cpu.gpr[3]) else {
        return 0;
    };
    let Some(current_flags) = memory.read_u16_be(gdevice + 20) else {
        return 0;
    };
    ppc_supported_depth_mode(cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], current_flags)
        .and_then(|(depth, _)| {
            crate::display::classic_depth_mode(u16::try_from(depth).ok()?).map(u32::from)
        })
        .unwrap_or(0)
}

pub(crate) fn ppc_standard_screen_clut(depth: u32, is_color: bool) -> Option<([[u16; 3]; 256], usize)> {
    let (mut clut, entry_count) = TrapDispatcher::standard_mac_indexed_clut(depth as u16)?;
    if !is_color {
        let last = u32::try_from(entry_count.checked_sub(1)?).ok()?;
        for (index, color) in clut.iter_mut().take(entry_count).enumerate() {
            let index = u32::try_from(index).ok()?;
            let component = ((last - index) * u32::from(u16::MAX) / last) as u16;
            *color = [component; 3];
        }
    } else if depth == 2 {
        // Inside Macintosh: Volume VI (1991), pp. 17-17--17-18: a color
        // 2-bit screen uses the enhanced standard table, whose spare entry
        // carries the current highlight color. PPC currently exposes the
        // System 7 default highlight green used by the shared 68k Toolbox.
        (clut, _) = TrapDispatcher::standard_mac_enhanced_clut(2, DEFAULT_QUICKDRAW_HILITE_COLOR)?;
    }
    Some((clut, entry_count))
}

pub(crate) fn ppc_update_pixmap_depth(
    memory: &mut PpcSectionMem,
    pixmap: u32,
    row_bytes: u32,
    depth: u32,
    indexed_ctable: u32,
) -> Option<()> {
    let (pixel_type, component_count, component_size, color_table) = match depth {
        1 | 2 | 4 | 8 => (0, 1, depth as u16, indexed_ctable),
        16 => (16, 3, 5, 0),
        _ => return None,
    };
    memory.write_u16_be(pixmap + 4, (row_bytes as u16) | 0x8000)?;
    memory.write_u16_be(pixmap + 30, pixel_type)?;
    memory.write_u16_be(pixmap + 32, depth as u16)?;
    memory.write_u16_be(pixmap + 34, component_count)?;
    memory.write_u16_be(pixmap + 36, component_size)?;
    memory.write_u32_be(pixmap + 42, color_table)?;
    Some(())
}

pub(crate) fn ppc_main_color_table_is_valid(memory: &mut PpcSectionMem) -> bool {
    let Some(table) = memory
        .read_u32_be(PPC_MAIN_CTABLE_HANDLE)
        .filter(|table| *table != 0)
    else {
        return false;
    };
    ppc_memory_can_write_bytes(memory, table, PPC_MAIN_CTABLE_SIZE)
}

pub(crate) fn ppc_install_standard_indexed_ctable(
    memory: &mut PpcSectionMem,
    ctable_handle: u32,
    seed: u32,
    clut: &[[u16; 3]; 256],
    entry_count: usize,
) -> Option<()> {
    let table = memory
        .read_u32_be(ctable_handle)
        .filter(|table| *table != 0)?;
    let byte_len = 8u32.checked_add(u32::try_from(entry_count).ok()?.checked_mul(8)?)?;
    if !ppc_memory_can_write_bytes(memory, table, byte_len) {
        return None;
    }
    let flags = memory.read_u16_be(table + 4)?;
    memory.write_u32_be(table, seed)?;
    memory.write_u16_be(
        table + 4,
        if ctable_handle == PPC_MAIN_CTABLE_HANDLE {
            flags | 0x8000
        } else {
            flags
        },
    )?;
    memory.write_u16_be(table + 6, u16::try_from(entry_count.checked_sub(1)?).ok()?)?;
    for (index, [red, green, blue]) in clut.iter().copied().take(entry_count).enumerate() {
        let entry = table.checked_add(8 + u32::try_from(index).ok()?.checked_mul(8)?)?;
        memory.write_u16_be(entry, index as u16)?;
        memory.write_u16_be(entry + 2, red)?;
        memory.write_u16_be(entry + 4, green)?;
        memory.write_u16_be(entry + 6, blue)?;
    }
    Some(())
}

pub(crate) fn ppc_preflight_ctable_growth(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    ctable_handles: &[u32],
    required_size: u32,
    heap_cursor: u32,
    heap_limit: u32,
) -> Result<(), i16> {
    let mut simulated_handles = handles.to_vec();
    let mut simulated_cursor = heap_cursor;
    for ctable_handle in ctable_handles {
        if *ctable_handle == PPC_MAIN_CTABLE_HANDLE {
            let table = memory
                .read_u32_be(*ctable_handle)
                .filter(|table| *table != 0)
                .ok_or(PPC_PARAM_ERR)?;
            if !ppc_memory_can_write_bytes(memory, table, required_size) {
                return Err(PPC_PARAM_ERR);
            }
            continue;
        }
        let table = memory
            .read_u32_be(*ctable_handle)
            .filter(|table| *table != 0)
            .ok_or(PPC_PARAM_ERR)?;
        let tracked = simulated_handles
            .iter_mut()
            .find(|record| record.handle == *ctable_handle);
        let Some(record) = tracked else {
            // A mapped address is not evidence that an untracked ColorTable
            // owns the following bytes: the synthetic heap is one contiguous
            // region and another object can begin immediately after ctTable.
            // Without a Memory Manager record we can only use the logical
            // size declared by ctSize, and cannot relocate the caller's
            // private Handle on its behalf. Imaging With QuickDraw (1994),
            // pp. 4-56--4-57.
            let last_index = memory.read_u16_be(table + 6).ok_or(PPC_PARAM_ERR)?;
            if (last_index as i16) < 0 || last_index >= 256 {
                return Err(PPC_PARAM_ERR);
            }
            let logical_size = u32::from(last_index)
                .checked_add(1)
                .and_then(|entries| entries.checked_mul(8))
                .and_then(|entries| entries.checked_add(8))
                .ok_or(PPC_PARAM_ERR)?;
            if logical_size < required_size
                || !ppc_memory_can_write_bytes(memory, table, required_size)
            {
                return Err(PPC_PARAM_ERR);
            }
            continue;
        };
        if record.ptr != table {
            return Err(PPC_PARAM_ERR);
        }
        if record.capacity >= required_size {
            continue;
        }
        let old_aligned = ppc_allocation_size(record.size).ok_or(PPC_PARAM_ERR)?;
        let new_aligned = ppc_allocation_size(required_size).ok_or(PPC_MEM_FULL_ERR)?;
        let old_end = record
            .ptr
            .checked_add(old_aligned)
            .ok_or(PPC_MEM_FULL_ERR)?;
        let in_place_end = (old_end == simulated_cursor)
            .then(|| {
                ppc_heap_allocation_bounds(memory, record.ptr, heap_limit, new_aligned)
                    .filter(|(new_ptr, _)| *new_ptr == record.ptr)
                    .map(|(_, new_end)| new_end)
            })
            .flatten();
        if let Some(new_end) = in_place_end {
            simulated_cursor = new_end;
        } else {
            let (new_ptr, new_end) =
                ppc_heap_allocation_bounds(memory, simulated_cursor, heap_limit, new_aligned)
                    .ok_or(PPC_MEM_FULL_ERR)?;
            record.ptr = new_ptr;
            simulated_cursor = new_end;
        }
        record.size = required_size;
        record.capacity = required_size;
    }
    Ok(())
}

pub(crate) fn ppc_grow_ctables(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    ctable_handles: &[u32],
    required_size: u32,
) -> Result<(), i16> {
    for ctable_handle in ctable_handles {
        if *ctable_handle == PPC_MAIN_CTABLE_HANDLE {
            continue;
        }
        let Some(record) = handles
            .iter()
            .find(|record| record.handle == *ctable_handle)
            .copied()
        else {
            continue;
        };
        if record.capacity >= required_size {
            continue;
        }
        let result = ppc_allocator_view_resize_handle(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            *ctable_handle,
            required_size,
        );
        if result != PPC_NO_ERR {
            return Err(result);
        }
    }
    Ok(())
}

pub(crate) fn ppc_set_depth(
    cpu: &PpcCpu,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut [PpcGWorldRecord],
    toolbox_startup: &mut PpcToolboxStartupState,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
) -> i16 {
    ppc_set_depth_with_geometry(
        cpu,
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        toolbox_startup,
        screen_clut,
        color_manager_clut,
        None,
    )
}

pub(crate) fn ppc_set_depth_with_geometry(
    cpu: &PpcCpu,
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut [PpcGWorldRecord],
    toolbox_startup: &mut PpcToolboxStartupState,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
    geometry: Option<(u32, u32)>,
) -> i16 {
    let Some(gdevice) = ppc_main_gdevice_record_for_depth(memory, cpu.gpr[3]) else {
        return PPC_PARAM_ERR;
    };
    let Some(current_gd_flags) = memory.read_u16_be(gdevice + 20) else {
        return PPC_PARAM_ERR;
    };
    let Some((depth, target_is_color)) =
        ppc_supported_depth_mode(cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], current_gd_flags)
    else {
        return PPC_PARAM_ERR;
    };

    let Some(mut main_record) = gworlds
        .iter()
        .find(|record| record.port == PPC_MAIN_GWORLD && record.gdevice == PPC_MAIN_GDEVICE)
        .copied()
    else {
        return PPC_PARAM_ERR;
    };
    if let Some((width, height)) = geometry {
        if width == 0
            || height == 0
            || width > ppc_main_screen_width()
            || height > ppc_main_screen_height()
        {
            return PPC_PARAM_ERR;
        }
        main_record.width = width;
        main_record.height = height;
    }
    let Some(row_bytes) = ppc_row_bytes(main_record.width, depth).filter(|row| *row <= 0x3fff)
    else {
        return PPC_PARAM_ERR;
    };
    let Some(clear_len) = row_bytes
        .checked_mul(main_record.height)
        .and_then(|len| usize::try_from(len).ok())
    else {
        return PPC_PARAM_ERR;
    };
    let Some(clear_len_u32) = u32::try_from(clear_len).ok() else {
        return PPC_PARAM_ERR;
    };

    // Discover screen-backed color ports through their live portPixMap
    // handles. A GWorld that merely uses the main GDevice but owns separate
    // pixel storage remains an offscreen world and must retain its format.
    let mut screen_records = Vec::new();
    let mut screen_pixmaps = Vec::new();
    for (index, record) in gworlds.iter().enumerate() {
        let Some(port_version_addr) = record.port.checked_add(6) else {
            return PPC_PARAM_ERR;
        };
        let Some(port_version) = memory.read_u16_be(port_version_addr) else {
            return PPC_PARAM_ERR;
        };
        if port_version & 0xc000 != 0xc000 {
            continue;
        }
        let Some(port_pixmap_addr) = record.port.checked_add(2) else {
            return PPC_PARAM_ERR;
        };
        let Some(pixmap_handle) = memory
            .read_u32_be(port_pixmap_addr)
            .filter(|handle| *handle != 0)
        else {
            return PPC_PARAM_ERR;
        };
        let Some(pixmap) = memory
            .read_u32_be(pixmap_handle)
            .filter(|pixmap| *pixmap != 0)
        else {
            return PPC_PARAM_ERR;
        };
        let Some(base_addr) = memory.read_u32_be(pixmap) else {
            return PPC_PARAM_ERR;
        };
        if base_addr != PPC_MAIN_SCREEN_BASE {
            continue;
        }
        if pixmap.checked_add(PPC_PIXMAP_SIZE - 1).is_none() {
            return PPC_PARAM_ERR;
        }
        if !ppc_memory_can_write_bytes(memory, pixmap + 4, 2)
            || !ppc_memory_can_write_bytes(memory, pixmap + 30, 8)
            || !ppc_memory_can_write_bytes(memory, pixmap + 42, 4)
        {
            return PPC_PARAM_ERR;
        }
        screen_records.push((index, pixmap_handle, pixmap));
        if !screen_pixmaps.contains(&pixmap) {
            screen_pixmaps.push(pixmap);
        }
    }
    let Some((_, main_pixmap_handle, main_pixmap)) = screen_records
        .iter()
        .find(|(index, _, _)| gworlds[*index].port == PPC_MAIN_GWORLD)
        .copied()
    else {
        return PPC_PARAM_ERR;
    };
    if memory.read_u32_be(gdevice + 22) != Some(main_pixmap_handle) {
        return PPC_PARAM_ERR;
    }
    let Some(current_screen_depth) = memory.read_u16_be(main_pixmap + 32).map(u32::from) else {
        return PPC_PARAM_ERR;
    };
    let current_is_color = current_gd_flags & 1 != 0;
    let preserved_indexed_mode = if current_screen_depth <= 8 {
        Some((current_screen_depth, current_is_color))
    } else {
        toolbox_startup.indexed_screen_mode
    };
    let preserve_indexed_tables =
        depth <= 8 && preserved_indexed_mode == Some((depth, target_is_color));

    let screen_bits = if toolbox_startup.init_graf_global_ptr == 0 {
        None
    } else {
        let Some(screen_bits) = toolbox_startup.init_graf_global_ptr.checked_sub(122) else {
            return PPC_PARAM_ERR;
        };
        if !ppc_memory_can_write_bytes(memory, screen_bits, 14) {
            return PPC_PARAM_ERR;
        }
        Some(screen_bits)
    };
    if !ppc_memory_can_write_bytes(memory, gdevice + 4, 2)
        || !ppc_memory_can_write_bytes(memory, gdevice + 20, 2)
        || !ppc_memory_can_write_bytes(memory, gdevice + 42, 4)
        || (depth <= 8 && !ppc_main_color_table_is_valid(memory))
    {
        return PPC_PARAM_ERR;
    }

    // Imaging With QuickDraw (1994), pp. 4-46--4-47 and 4-83: indexed
    // PixMaps carry a ColorTable Handle, while direct PixMaps expose NIL.
    // Retain each live Handle's indexed attachment in host state so a
    // caller-installed SetPortPix table can be restored after the direct leg.
    let mut indexed_screen_ctables = toolbox_startup.indexed_screen_ctables.clone();
    for (_, pixmap_handle, pixmap) in &screen_records {
        let Some(ctable_handle) = memory.read_u32_be(*pixmap + 42) else {
            return PPC_PARAM_ERR;
        };
        if ctable_handle != 0 {
            indexed_screen_ctables.insert(*pixmap_handle, ctable_handle);
        }
    }
    let ctable_fallback = TrapDispatcher::standard_mac_8bpp_clut();
    let mut screen_pixmap_updates = Vec::with_capacity(screen_pixmaps.len());
    for pixmap in screen_pixmaps {
        let indexed_ctable = if depth <= 8 {
            let mut selected = (pixmap == main_pixmap).then_some(PPC_MAIN_CTABLE_HANDLE);
            for (_, pixmap_handle, candidate_pixmap) in &screen_records {
                if *candidate_pixmap != pixmap {
                    continue;
                }
                let Some(candidate) = indexed_screen_ctables.get(pixmap_handle).copied() else {
                    continue;
                };
                if ppc_read_ctable_clut(memory, candidate, &ctable_fallback).is_none() {
                    indexed_screen_ctables.remove(pixmap_handle);
                    continue;
                }
                if selected.is_some_and(|current| current != candidate) {
                    return PPC_PARAM_ERR;
                }
                selected = Some(candidate);
            }
            let selected = selected.unwrap_or(PPC_MAIN_CTABLE_HANDLE);
            for (_, pixmap_handle, candidate_pixmap) in &screen_records {
                if *candidate_pixmap == pixmap {
                    indexed_screen_ctables.insert(*pixmap_handle, selected);
                }
            }
            selected
        } else {
            0
        };
        screen_pixmap_updates.push((pixmap, indexed_ctable));
    }
    let indexed_device_clut = if depth <= 8 {
        let Some((standard_clut, entry_count)) = ppc_standard_screen_clut(depth, target_is_color)
        else {
            return PPC_PARAM_ERR;
        };
        if preserve_indexed_tables {
            let Some(clut) = ppc_read_ctable_clut(memory, PPC_MAIN_CTABLE_HANDLE, &standard_clut)
            else {
                return PPC_PARAM_ERR;
            };
            Some((clut, entry_count, Vec::new(), false, 0))
        } else {
            let Some(byte_len) = u32::try_from(entry_count)
                .ok()
                .and_then(|count| count.checked_mul(8))
                .and_then(|entries| entries.checked_add(8))
            else {
                return PPC_PARAM_ERR;
            };
            let mut ctable_handles = screen_pixmap_updates
                .iter()
                .map(|(_, ctable_handle)| *ctable_handle)
                .filter(|ctable_handle| *ctable_handle != 0)
                .collect::<Vec<_>>();
            ctable_handles.sort_unstable();
            ctable_handles.dedup();
            if let Err(error) = ppc_preflight_ctable_growth(
                memory,
                handles,
                &ctable_handles,
                byte_len,
                *heap_cursor,
                heap_limit,
            ) {
                return error;
            }
            Some((standard_clut, entry_count, ctable_handles, true, byte_len))
        }
    } else {
        None
    };

    if !ppc_memory_can_write_bytes(memory, PPC_MAIN_SCREEN_BASE, clear_len_u32) {
        return PPC_PARAM_ERR;
    }

    if let Some((indexed_device_clut, entry_count, ctable_handles, install_tables, required_size)) =
        indexed_device_clut.as_ref()
    {
        if *install_tables {
            if let Err(error) = ppc_grow_ctables(
                allocator.as_deref_mut(),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                ctable_handles,
                *required_size,
            ) {
                return error;
            }
            // Canonical color tables use their depth as the traditional
            // seed. Grayscale and enhanced 2-bit color tables differ at the
            // same depth, so give them a fresh seed to keep inverse-table
            // caches from treating distinct personalities as identical.
            let seed = if (target_is_color && depth != 2) || depth == 1 {
                depth
            } else {
                ppc_next_ct_seed(toolbox_startup)
            };
            for ctable_handle in ctable_handles {
                if ppc_install_standard_indexed_ctable(
                    memory,
                    *ctable_handle,
                    seed,
                    indexed_device_clut,
                    *entry_count,
                )
                .is_none()
                {
                    return PPC_PARAM_ERR;
                }
            }
        }
    }

    // All guest addresses have been preflighted. Patch only the fields that
    // describe the selected format so window bounds, resolutions, private
    // metadata, ColorTable contents, and unrelated GDevice flags survive.
    for (pixmap, indexed_ctable) in screen_pixmap_updates {
        if ppc_update_pixmap_depth(memory, pixmap, row_bytes, depth, indexed_ctable).is_none()
            || (geometry.is_some()
                && ppc_write_rect(
                    memory,
                    pixmap + 6,
                    0,
                    0,
                    ppc_u32_to_i16_saturating(main_record.height),
                    ppc_u32_to_i16_saturating(main_record.width),
                )
                .is_none())
        {
            return PPC_PARAM_ERR;
        }
    }
    for (index, pixmap_handle, pixmap) in screen_records {
        let record = &mut gworlds[index];
        // SetPortPix replaces the live destination without rewriting the
        // creation-time GWorld cache. Keep that cache self-consistent when it
        // still describes an owned offscreen PixMap; live QuickDraw drawing
        // resolves the replacement through the port itself.
        if record.pixmap_handle == pixmap_handle && record.pixmap == pixmap {
            record.depth = depth;
            record.row_bytes = row_bytes;
            if geometry.is_some() {
                record.width = main_record.width;
                record.height = main_record.height;
            }
        }
    }

    let gd_type = if depth <= 8 { 0 } else { 2 };
    let gd_flags = if target_is_color {
        current_gd_flags | 1
    } else {
        current_gd_flags & !1
    };
    let Some(depth_mode) = u16::try_from(depth)
        .ok()
        .and_then(crate::display::classic_depth_mode)
    else {
        return PPC_PARAM_ERR;
    };
    if memory.write_u16_be(gdevice + 4, gd_type).is_none()
        || memory.write_u16_be(gdevice + 20, gd_flags).is_none()
        || memory
            .write_u32_be(gdevice + 42, u32::from(depth_mode))
            .is_none()
    {
        return PPC_PARAM_ERR;
    }
    if geometry.is_some() {
        if ppc_write_rect(
            memory,
            gdevice + 34,
            0,
            0,
            ppc_u32_to_i16_saturating(main_record.height),
            ppc_u32_to_i16_saturating(main_record.width),
        )
        .is_none()
            || ppc_write_rect(
                memory,
                PPC_MAIN_GWORLD + 16,
                0,
                0,
                ppc_u32_to_i16_saturating(main_record.height),
                ppc_u32_to_i16_saturating(main_record.width),
            )
            .is_none()
            || ppc_write_rgn_bbox(
                memory,
                PPC_MAIN_VIS_RGN_HANDLE,
                0,
                0,
                ppc_u32_to_i16_saturating(main_record.height),
                ppc_u32_to_i16_saturating(main_record.width),
            )
            .is_none()
        {
            return PPC_PARAM_ERR;
        }
        let menu_bar_height = i16::try_from(
            u32::from(memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20))
                .min(main_record.height),
        )
        .unwrap_or(20);
        if ppc_write_rgn_bbox(
            memory,
            PPC_GRAY_RGN_HANDLE,
            menu_bar_height,
            0,
            ppc_u32_to_i16_saturating(main_record.height),
            ppc_u32_to_i16_saturating(main_record.width),
        )
        .is_none()
        {
            return PPC_PARAM_ERR;
        }
    }
    if let Some(screen_bits) = screen_bits {
        if memory
            .write_u32_be(screen_bits, PPC_MAIN_SCREEN_BASE)
            .is_none()
            || memory
                .write_u16_be(screen_bits + 4, row_bytes as u16 & 0x3fff)
                .is_none()
            || ppc_write_rect(
                memory,
                screen_bits + 6,
                0,
                0,
                ppc_u32_to_i16_saturating(main_record.height),
                ppc_u32_to_i16_saturating(main_record.width),
            )
            .is_none()
        {
            return PPC_PARAM_ERR;
        }
    }

    let clear_byte = if depth == 8 { 0xff } else { 0x00 };
    let clear = vec![clear_byte; clear_len];
    if memory.write_bytes(PPC_MAIN_SCREEN_BASE, &clear).is_none() {
        return PPC_PARAM_ERR;
    }

    toolbox_startup.indexed_screen_ctables = indexed_screen_ctables;
    toolbox_startup.indexed_screen_mode = if depth <= 8 {
        Some((depth, target_is_color))
    } else {
        preserved_indexed_mode
    };
    if let Some((indexed_device_clut, _, _, _, _)) = indexed_device_clut {
        *screen_clut = indexed_device_clut;
        *color_manager_clut = indexed_device_clut;
    }

    // A depth switch invalidates the saved pixels and front-buffer metadata
    // owned by an active menu operation. Drop that operation only after the
    // mode switch succeeds so it cannot resume against the new PixMap. A
    // PopUpMenuSelect overlay never owns TheMenu, so preserve that low-memory
    // value when cancelling popup tracking.
    let tracking_owned_menu_bar = toolbox_startup
        .execution
        .menu()
        .as_ref()
        .is_some_and(|state| state.kind == MenuTrackingKind::MenuBar);
    toolbox_startup.execution.with_existing_menu_context_mut(|context| {
        context.tracking = None;
        context.clear_native_popup();
    });
    toolbox_startup.go_away_tracking = None;
    toolbox_startup.drag_window_tracking = None;
    toolbox_startup.grow_window_tracking = None;
    if tracking_owned_menu_bar {
        let _ = memory.write_u16_be(PPC_THE_MENU_ADDR, 0);
    }

    PPC_NO_ERR
}

pub(crate) fn ppc_port_rgb_colors(
    memory: &mut PpcSectionMem,
    port: u32,
) -> Option<(PpcRgbColor, PpcRgbColor)> {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 2-9--2-10:
    // rgbFgColor and rgbBkColor belong to each CGrafPort. Port switches must
    // therefore restore them instead of leaking the previous port's colors.
    if memory.read_u16_be(port.checked_add(6)?)? & 0xc000 != 0xc000 {
        return None;
    }
    Some((
        ppc_read_rgb_color(
            memory,
            port.checked_add(PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET)?,
        )?,
        ppc_read_rgb_color(
            memory,
            port.checked_add(PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET)?,
        )?,
    ))
}

pub(crate) fn ppc_port_op_color_from_graf_vars(
    memory: &mut PpcSectionMem,
    port: u32,
) -> Option<PpcRgbColor> {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64:
    // a CGrafPort's GrafVars record begins with rgbOpColor. Do not consult
    // the fallback map when a valid guest record is present; guest memory is
    // the authoritative representation for that port.
    if memory.read_u16_be(port.checked_add(6)?)? & 0xc000 != 0xc000 {
        return None;
    }
    let graf_vars_handle = memory.read_u32_be(port.checked_add(PPC_CGRAF_PORT_GRAF_VARS_OFFSET)?)?;
    if graf_vars_handle == 0 || !ppc_memory_can_write_bytes(memory, graf_vars_handle, 4) {
        return None;
    }
    let graf_vars = memory.read_u32_be(graf_vars_handle)?;
    if graf_vars == 0 || !ppc_memory_can_write_bytes(memory, graf_vars, 6) {
        return None;
    }
    ppc_read_rgb_color(memory, graf_vars)
}

pub(crate) fn ppc_current_op_color(
    memory: &mut PpcSectionMem,
    port: u32,
    quickdraw_op_colors: &SharedProcessQuickDrawOpColors,
) -> PpcRgbColor {
    ppc_port_op_color_from_graf_vars(memory, port)
        .or_else(|| {
            quickdraw_op_colors
                .quickdraw_op_color(port)
                .map(|(red, green, blue)| PpcRgbColor { red, green, blue })
        })
        .unwrap_or(PPC_RGB_BLACK)
}

pub(crate) fn ppc_write_port_op_color(
    memory: &mut PpcSectionMem,
    port: u32,
    color: PpcRgbColor,
    quickdraw_op_colors: &SharedProcessQuickDrawOpColors,
) {
    let Some(port_version) = port.checked_add(6).and_then(|address| memory.read_u16_be(address))
    else {
        return;
    };
    if port_version & 0xc000 != 0xc000 {
        return;
    }
    let graf_vars_handle = memory
        .read_u32_be(port.wrapping_add(PPC_CGRAF_PORT_GRAF_VARS_OFFSET))
        .unwrap_or(0);
    if let Some(graf_vars) = (graf_vars_handle != 0)
        .then(|| memory.read_u32_be(graf_vars_handle).unwrap_or(0))
        .filter(|graf_vars| {
            *graf_vars != 0 && ppc_memory_can_write_bytes(memory, *graf_vars, 6)
        })
    {
        let _ = ppc_write_rgb_color(memory, graf_vars, color);
    }
    // Static process ports (including the seeded main port) do not own an
    // allocator-managed GrafVars handle. Keep their per-port fallback live
    // across attached 68K and PowerPC adapters.
    quickdraw_op_colors.set_quickdraw_op_color(port, (color.red, color.green, color.blue));
}

pub(crate) fn ppc_port_hilite_color_from_graf_vars(
    memory: &mut PpcSectionMem,
    port: u32,
) -> Option<PpcRgbColor> {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-62 and 4-64:
    // a CGrafPort's GrafVars.rgbHiliteColor follows rgbOpColor at offset 6.
    // Do not consult the fallback map when a valid guest record is present;
    // guest memory is the authoritative representation for that port.
    if memory.read_u16_be(port.checked_add(6)?)? & 0xc000 != 0xc000 {
        return None;
    }
    let graf_vars_handle = memory.read_u32_be(port.checked_add(PPC_CGRAF_PORT_GRAF_VARS_OFFSET)?)?;
    if graf_vars_handle == 0 || !ppc_memory_can_write_bytes(memory, graf_vars_handle, 4) {
        return None;
    }
    let graf_vars = memory.read_u32_be(graf_vars_handle)?;
    let hilite_color = graf_vars.checked_add(6)?;
    if graf_vars == 0 || !ppc_memory_can_write_bytes(memory, hilite_color, 6) {
        return None;
    }
    ppc_read_rgb_color(memory, hilite_color)
}

pub(crate) fn ppc_current_hilite_color(
    memory: &mut PpcSectionMem,
    port: u32,
    quickdraw_hilite_colors: &SharedProcessQuickDrawHiliteColors,
) -> PpcRgbColor {
    ppc_port_hilite_color_from_graf_vars(memory, port)
        .or_else(|| {
            quickdraw_hilite_colors
                .quickdraw_hilite_color(port)
                .map(|(red, green, blue)| PpcRgbColor { red, green, blue })
        })
        .unwrap_or_else(|| {
            let (red, green, blue) = DEFAULT_QUICKDRAW_HILITE_COLOR;
            PpcRgbColor { red, green, blue }
        })
}

pub(crate) fn ppc_write_port_hilite_color(
    memory: &mut PpcSectionMem,
    port: u32,
    color: PpcRgbColor,
    quickdraw_hilite_colors: &SharedProcessQuickDrawHiliteColors,
) {
    let Some(port_version) = port.checked_add(6).and_then(|address| memory.read_u16_be(address))
    else {
        return;
    };
    if port_version & 0xc000 != 0xc000 {
        return;
    }
    let graf_vars_handle = memory
        .read_u32_be(port.wrapping_add(PPC_CGRAF_PORT_GRAF_VARS_OFFSET))
        .unwrap_or(0);
    if let Some(graf_vars) = (graf_vars_handle != 0)
        .then(|| memory.read_u32_be(graf_vars_handle).unwrap_or(0))
        .filter(|graf_vars| *graf_vars != 0)
        .and_then(|graf_vars| graf_vars.checked_add(6))
        .filter(|graf_vars| {
            ppc_memory_can_write_bytes(memory, *graf_vars, 6)
        })
    {
        let _ = ppc_write_rgb_color(memory, graf_vars, color);
    }
    // Static process ports (including the seeded main port) do not own an
    // allocator-managed GrafVars handle. Keep their per-port fallback live
    // across attached 68K and PowerPC adapters.
    quickdraw_hilite_colors.set_quickdraw_hilite_color(
        port,
        (color.red, color.green, color.blue),
    );
}

pub(crate) fn ppc_write_port_rgb_color(
    memory: &mut PpcSectionMem,
    port: u32,
    offset: u32,
    color: PpcRgbColor,
) -> Option<()> {
    // A monochrome GrafPort has legacy fgColor/bkColor fields at these byte
    // positions rather than RGBColor records. Do not overwrite them.
    if memory.read_u16_be(port.checked_add(6)?)? & 0xc000 != 0xc000 {
        return None;
    }
    ppc_write_rgb_color(memory, port.checked_add(offset)?, color)
}

pub(crate) fn ppc_restore_port_colors(
    memory: &mut PpcSectionMem,
    port: u32,
    fore_color: &mut PpcRgbColor,
    back_color: &mut PpcRgbColor,
) {
    if let Some((port_fore_color, port_back_color)) = ppc_port_rgb_colors(memory, port) {
        *fore_color = port_fore_color;
        *back_color = port_back_color;
    }
}

pub(crate) fn ppc_restore_process_port_draw_state(
    memory: &mut PpcSectionMem,
    port: u32,
    fore_color: &mut PpcRgbColor,
    back_color: &mut PpcRgbColor,
    pen_h: &mut i16,
    pen_v: &mut i16,
    text_mode: &mut i16,
    text_size: &mut i16,
) {
    let Some((port_fore_color, port_back_color)) = ppc_port_rgb_colors(memory, port) else {
        return;
    };

    // A graphics port is the complete drawing environment, and each port
    // owns its pen, colors, and text settings. Reload the documented
    // CGrafPort fields whenever attached execution crosses an HLE boundary so
    // a preceding 68K callback is visible without an adapter-to-adapter copy.
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 2-30--2-33,
    // 4-8--4-10, and 4-48.
    *fore_color = port_fore_color;
    *back_color = port_back_color;
    if let Some((h, v)) = ppc_gworld_pen(memory, port) {
        *pen_h = h;
        *pen_v = v;
    }
    if let Some(value) = memory.read_u16_be(port.wrapping_add(PPC_CGRAF_PORT_TX_MODE_OFFSET)) {
        *text_mode = value as i16;
    }
    if let Some(value) = memory.read_u16_be(port.wrapping_add(PPC_CGRAF_PORT_TX_SIZE_OFFSET)) {
        *text_size = value as i16;
    }
}

pub(crate) fn ppc_rgb2hsl(memory: &mut PpcSectionMem, rgb_ptr: u32, hsl_ptr: u32) -> bool {
    // Inside Macintosh Volume VI (1991), pp. 19-10--19-11: RGBColor and
    // HSLColor both carry unsigned 16-bit components; hue is a fraction of a
    // circle and lightness is midway between the greatest and least channel.
    let Some(rgb) = ppc_read_rgb_color(memory, rgb_ptr) else {
        return false;
    };
    if hsl_ptr == 0 || !ppc_memory_can_write_bytes(memory, hsl_ptr, 6) {
        return false;
    }
    let red = f64::from(rgb.red) / 65_535.0;
    let green = f64::from(rgb.green) / 65_535.0;
    let blue = f64::from(rgb.blue) / 65_535.0;
    let greatest = red.max(green).max(blue);
    let least = red.min(green).min(blue);
    let delta = greatest - least;
    let lightness = (greatest + least) / 2.0;
    let (hue, saturation) = if delta == 0.0 {
        (0.0, 0.0)
    } else {
        let saturation = if lightness <= 0.5 {
            delta / (greatest + least)
        } else {
            delta / (2.0 - greatest - least)
        };
        let mut hue = if greatest == red {
            (green - blue) / delta
        } else if greatest == green {
            2.0 + (blue - red) / delta
        } else {
            4.0 + (red - green) / delta
        };
        if hue < 0.0 {
            hue += 6.0;
        }
        (hue / 6.0, saturation)
    };
    let to_small_fract =
        |component: f64| -> u16 { (component.clamp(0.0, 1.0) * 65_535.0).round() as u16 };
    memory.write_u16_be(hsl_ptr, to_small_fract(hue)).is_some()
        && memory
            .write_u16_be(hsl_ptr + 2, to_small_fract(saturation))
            .is_some()
        && memory
            .write_u16_be(hsl_ptr + 4, to_small_fract(lightness))
            .is_some()
}

pub(crate) fn ppc_rgb2hsv(memory: &mut PpcSectionMem, rgb_ptr: u32, hsv_ptr: u32) -> bool {
    // Inside Macintosh Volume VI (1991), pp. 19-10--19-11: HSVColor uses
    // unsigned 16-bit fractions for hue, saturation, and value.
    let Some(rgb) = ppc_read_rgb_color(memory, rgb_ptr) else {
        return false;
    };
    if hsv_ptr == 0 || !ppc_memory_can_write_bytes(memory, hsv_ptr, 6) {
        return false;
    }
    let red = f64::from(rgb.red) / 65_535.0;
    let green = f64::from(rgb.green) / 65_535.0;
    let blue = f64::from(rgb.blue) / 65_535.0;
    let greatest = red.max(green).max(blue);
    let least = red.min(green).min(blue);
    let delta = greatest - least;
    let saturation = if greatest == 0.0 {
        0.0
    } else {
        delta / greatest
    };
    let hue = if delta == 0.0 {
        0.0
    } else {
        let mut sector = if greatest == red {
            (green - blue) / delta
        } else if greatest == green {
            2.0 + (blue - red) / delta
        } else {
            4.0 + (red - green) / delta
        };
        if sector < 0.0 {
            sector += 6.0;
        }
        sector / 6.0
    };
    let to_small_fract =
        |component: f64| -> u16 { (component.clamp(0.0, 1.0) * 65_535.0).round() as u16 };
    memory.write_u16_be(hsv_ptr, to_small_fract(hue)).is_some()
        && memory
            .write_u16_be(hsv_ptr + 2, to_small_fract(saturation))
            .is_some()
        && memory
            .write_u16_be(hsv_ptr + 4, to_small_fract(greatest))
            .is_some()
}

pub(crate) fn ppc_hsv2rgb(memory: &mut PpcSectionMem, hsv_ptr: u32, rgb_ptr: u32) -> bool {
    if hsv_ptr == 0
        || rgb_ptr == 0
        || !ppc_memory_can_write_bytes(memory, hsv_ptr, 6)
        || !ppc_memory_can_write_bytes(memory, rgb_ptr, 6)
    {
        return false;
    }
    let hue = f64::from(memory.read_u16_be(hsv_ptr).unwrap_or(0)) / 65_535.0;
    let saturation = f64::from(memory.read_u16_be(hsv_ptr + 2).unwrap_or(0)) / 65_535.0;
    let value = f64::from(memory.read_u16_be(hsv_ptr + 4).unwrap_or(0)) / 65_535.0;
    let (red, green, blue) = if saturation == 0.0 {
        (value, value, value)
    } else {
        let scaled_hue = hue * 6.0;
        let sector = scaled_hue.floor() as u32 % 6;
        let fraction = scaled_hue - scaled_hue.floor();
        let low = value * (1.0 - saturation);
        let falling = value * (1.0 - saturation * fraction);
        let rising = value * (1.0 - saturation * (1.0 - fraction));
        match sector {
            0 => (value, rising, low),
            1 => (falling, value, low),
            2 => (low, value, rising),
            3 => (low, falling, value),
            4 => (rising, low, value),
            _ => (value, low, falling),
        }
    };
    let to_word =
        |component: f64| -> u16 { (component.clamp(0.0, 1.0) * 65_535.0).round() as u16 };
    memory.write_u16_be(rgb_ptr, to_word(red)).is_some()
        && memory.write_u16_be(rgb_ptr + 2, to_word(green)).is_some()
        && memory.write_u16_be(rgb_ptr + 4, to_word(blue)).is_some()
}

pub(crate) fn ppc_rect_dimensions(top: i16, left: i16, bottom: i16, right: i16) -> (u32, u32) {
    let width = i32::from(right).saturating_sub(i32::from(left)).max(1) as u32;
    let height = i32::from(bottom).saturating_sub(i32::from(top)).max(1) as u32;
    (width, height)
}

pub(crate) fn ppc_row_bytes(width: u32, depth: u32) -> Option<u32> {
    let visible_row_bytes = width.checked_mul(depth)?.div_ceil(8);
    visible_row_bytes
        .checked_div(16)?
        .checked_add(1)?
        .checked_mul(16)
}

pub(crate) fn ppc_u32_to_i16_saturating(value: u32) -> i16 {
    i16::try_from(value).unwrap_or(i16::MAX)
}

pub(crate) fn ppc_i32_to_i16_saturating(value: i32) -> i16 {
    i16::try_from(value).unwrap_or(if value < 0 { i16::MIN } else { i16::MAX })
}

pub(crate) fn ppc_draw_picture(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    screen_clut: &[[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
) -> bool {
    let pic_handle = cpu.gpr[3];
    let dst_rect_ptr = cpu.gpr[4];
    if pic_handle == 0 || dst_rect_ptr == 0 {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] DrawPicture handle=${:08X} dstRect=${:08X} gworld=${:08X} depth=none drawn=false reason=missing-arg",
                pic_handle, dst_rect_ptr, current_gworld
            );
        }
        return false;
    }
    let Some(bytes) = ppc_handle_bytes(memory, handles, pic_handle) else {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] DrawPicture handle=${:08X} dstRect=${:08X} gworld=${:08X} depth=none drawn=false reason=missing-handle",
                pic_handle, dst_rect_ptr, current_gworld
            );
        }
        return false;
    };
    let Some(mut dst_rect) = ppc_read_rect(memory, dst_rect_ptr) else {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] DrawPicture handle=${:08X} dstRect=${:08X} gworld=${:08X} depth=none drawn=false reason=bad-rect",
                pic_handle, dst_rect_ptr, current_gworld
            );
        }
        return false;
    };
    let Some((_pict_offset, pict_bounds)) = ppc_qt_pict_record_offset_and_bounds(&bytes) else {
        if ppc_hle_trace_enabled() {
            let head = bytes
                .iter()
                .take(24)
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join("");
            eprintln!(
                "[PPC-TRACE] DrawPicture handle=${:08X} dstRect=${:08X} gworld=${:08X} depth=none drawn=false reason=bad-pict size={} head={}",
                pic_handle,
                dst_rect_ptr,
                current_gworld,
                bytes.len(),
                head
            );
        }
        return false;
    };
    if dst_rect == (0, 0, 0, 0) {
        dst_rect = pict_bounds;
    }
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] DrawPicture handle=${:08X} dstRect=${:08X} gworld=${:08X} depth=none drawn=false reason=missing-gworld",
                pic_handle, dst_rect_ptr, current_gworld
            );
        }
        return false;
    };
    let front_buffer = surface.front_buffer;
    dst_rect = surface.local_rect_i16(dst_rect);
    let draw_clut = ppc_live_gworld_clut(
        memory,
        gworlds,
        current_gworld,
        screen_clut,
        color_manager_clut,
    );
    let device_ct_seed = surface
        .ctable_handle
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|ctable| *ctable != 0)
        .and_then(|ctable| memory.read_u32_be(ctable))
        .unwrap_or(0);
    let quilt_zero_is_opaque = ppc_quilt_picture_zero_is_opaque(vfs_resources, pic_handle);
    let drawn = ppc_draw_pict_bytes_to_16bpp(
        memory,
        front_buffer,
        &bytes,
        dst_rect,
        &draw_clut,
        device_ct_seed,
        quilt_zero_is_opaque,
    );
    if ppc_hle_trace_enabled() {
        let (top, left, bottom, right) = dst_rect;
        eprintln!(
            "[PPC-TRACE] DrawPicture handle=${:08X} dstRect=${:08X} gworld=${:08X} depth={} rect=({}, {}, {}, {}) drawn={}",
            pic_handle,
            dst_rect_ptr,
            current_gworld,
            front_buffer.depth,
            top,
            left,
            bottom,
            right,
            drawn
        );
    }
    drawn
}

pub(crate) fn ppc_quilt_picture_zero_is_opaque(
    vfs_resources: &[PpcVfsResourceRecord],
    picture_handle: u32,
) -> bool {
    let pict_type = u32::from_be_bytes(*b"PICT");
    let img_type = u32::from_be_bytes(*b"#Img");
    let Some(picture) = vfs_resources
        .iter()
        .find(|resource| resource.res_type == pict_type && resource.handle == picture_handle)
    else {
        return false;
    };
    vfs_resources.iter().any(|resource| {
        // Quilt's #Img record distinguishes opaque-black frames (mode 3)
        // from frames that retain pixel index 0 as a key for later sprite
        // compositing. Converting every zero while loading destroys that key.
        resource.res_type == img_type
            && resource.path.eq_ignore_ascii_case(&picture.path)
            && resource.data.get(10..12) == Some(&3u16.to_be_bytes())
    })
}

pub(crate) fn ppc_kill_picture(
    cpu: &mut PpcCpu,
    allocator: &mut PpcProcessAllocatorView<'_>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    let pic_handle = cpu.gpr[3];
    let _ = allocator.dispose_handle(
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        pic_handle,
    );
}
