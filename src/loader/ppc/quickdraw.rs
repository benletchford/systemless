//! PowerPC QuickDraw 2D drawing primitives, text rendering, PixMap access, and blit transfers.

use super::graphics::*;
use super::regions::*;
use super::*;

pub(crate) fn ppc_sync_gworld_pen(memory: &mut PpcSectionMem, current_gworld: u32, h: i16, v: i16) {
    if current_gworld == 0
        || !ppc_memory_can_write_bytes(memory, current_gworld + PPC_CGRAF_PORT_PN_LOC_OFFSET, 4)
    {
        return;
    }
    let _ = memory.write_u16_be(current_gworld + PPC_CGRAF_PORT_PN_LOC_OFFSET, v as u16);
    let _ = memory.write_u16_be(current_gworld + PPC_CGRAF_PORT_PN_LOC_OFFSET + 2, h as u16);
}

pub(crate) fn ppc_gworld_pen(memory: &mut PpcSectionMem, current_gworld: u32) -> Option<(i16, i16)> {
    let v = memory.read_u16_be(current_gworld.checked_add(PPC_CGRAF_PORT_PN_LOC_OFFSET)?)? as i16;
    let h = memory.read_u16_be(
        current_gworld
            .checked_add(PPC_CGRAF_PORT_PN_LOC_OFFSET)?
            .checked_add(2)?,
    )? as i16;
    Some((h, v))
}

pub(crate) fn ppc_set_pen_size(cpu: &PpcCpu, memory: &mut PpcSectionMem, current_gworld: u32) {
    if current_gworld == 0
        || !ppc_memory_can_write_bytes(memory, current_gworld + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 4)
    {
        return;
    }
    // Inside Macintosh: Imaging With QuickDraw (1994), p. 3-20: PenSize's
    // parameters are horizontal width then vertical height, while Point fields
    // are stored vertical first.
    let _ = memory.write_u16_be(
        current_gworld + PPC_CGRAF_PORT_PN_SIZE_OFFSET,
        cpu.gpr[4] as u16,
    );
    let _ = memory.write_u16_be(
        current_gworld + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2,
        cpu.gpr[3] as u16,
    );
}

pub(crate) fn ppc_set_pen_normal(memory: &mut PpcSectionMem, current_gworld: u32) {
    if current_gworld == 0
        || !ppc_memory_can_write_bytes(memory, current_gworld + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 6)
    {
        return;
    }
    // Imaging With QuickDraw (1994), p. 3-23: PenNormal restores a 1x1
    // black pen in patCopy mode without changing its location.
    let _ = memory.write_u16_be(current_gworld + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 1);
    let _ = memory.write_u16_be(current_gworld + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, 1);
    let _ = memory.write_u16_be(
        current_gworld + PPC_CGRAF_PORT_PN_MODE_OFFSET,
        PPC_QD_PEN_MODE_PAT_COPY as u16,
    );
}

pub(crate) fn ppc_get_pen_state(memory: &mut PpcSectionMem, current_gworld: u32, state_ptr: u32) {
    const PEN_STATE_SIZE: u32 = 18;
    let Some(bytes) = ppc_memory_read_bytes(
        memory,
        current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_LOC_OFFSET),
        PEN_STATE_SIZE,
    ) else {
        return;
    };
    if ppc_memory_can_write_bytes(memory, state_ptr, PEN_STATE_SIZE) {
        let _ = memory.write_bytes(state_ptr, &bytes);
    }
}

pub(crate) fn ppc_set_pen_state(memory: &mut PpcSectionMem, current_gworld: u32, state_ptr: u32) {
    const PEN_STATE_SIZE: u32 = 18;
    let Some(bytes) = ppc_memory_read_bytes(memory, state_ptr, PEN_STATE_SIZE) else {
        return;
    };
    if ppc_memory_can_write_bytes(
        memory,
        current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_LOC_OFFSET),
        PEN_STATE_SIZE,
    ) {
        // Imaging With QuickDraw (1994), pp. 3-37 and 3-43--3-44: PenState
        // is the contiguous pnLoc, pnSize, pnMode, and 8-byte pnPat portion of
        // the current graphics port, totaling 18 bytes.
        let _ = memory.write_bytes(
            current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_LOC_OFFSET),
            &bytes,
        );
    }
}

pub(crate) fn ppc_sync_gworld_text(
    memory: &mut PpcSectionMem,
    current_gworld: u32,
    text_mode: i16,
    text_size: i16,
) {
    if current_gworld == 0
        || !ppc_memory_can_write_bytes(memory, current_gworld + PPC_CGRAF_PORT_TX_MODE_OFFSET, 4)
    {
        return;
    }
    let _ = memory.write_u16_be(
        current_gworld + PPC_CGRAF_PORT_TX_MODE_OFFSET,
        text_mode as u16,
    );
    let _ = memory.write_u16_be(
        current_gworld + PPC_CGRAF_PORT_TX_SIZE_OFFSET,
        text_size as u16,
    );
}

pub(crate) fn ppc_line_to(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    from: (i16, i16),
    to: (i16, i16),
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    if memory
        .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_VIS_OFFSET))
        .is_some_and(|visibility| (visibility as i16) < 0)
    {
        return false;
    }
    let front_buffer = surface.front_buffer;
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }
    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return false;
    };
    let clip_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
        .and_then(|clip_rgn| ppc_region_storage(memory, clip_rgn));
    let vis_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|vis_rgn| ppc_region_storage(memory, vis_rgn));

    let (x0, y0) = surface.local_point((i32::from(from.0), i32::from(from.1)));
    let (x1, y1) = surface.local_point((i32::from(to.0), i32::from(to.1)));
    let pen_height = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET))
            .unwrap_or(1),
    );
    let pen_width = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2))
            .unwrap_or(1),
    );
    let mut wrote = false;
    let mut paint_pen = |x: i32, y: i32| {
        for py in y..y.saturating_add(pen_height) {
            for px in x..x.saturating_add(pen_width) {
                if ppc_local_point_in_port_regions(
                    surface,
                    (px, py),
                    vis_storage.as_deref(),
                    clip_storage.as_deref(),
                ) {
                    wrote |= ppc_quickdraw_write_raw_pixel(
                        memory,
                        front_buffer,
                        (px, py),
                        color_pixel,
                    );
                }
            }
        }
    };

    if x0 == x1 {
        let (top, bottom) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        for y in top..=bottom {
            paint_pen(x0, y);
        }
    } else if y0 == y1 {
        let (left, right) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        for x in left..=right {
            paint_pen(x, y0);
        }
    } else {
        let (sx, sy, ex, ey) = if y0 <= y1 {
            (x0, y0, x1, y1)
        } else {
            (x1, y1, x0, y0)
        };
        let dh = (ex - sx).abs();
        let dv = ey - sy;
        let x_dir = if ex > sx { 1 } else { -1 };
        if dv >= dh {
            let slope = (((i64::from(ex - sx)) << 16) + i64::from(dv) / 2)
                / i64::from(dv);
            let mut x_fixed = ((i64::from(sx)) << 16) | 0x8000;
            x_fixed += slope >> 1;
            for y in sy..=ey {
                paint_pen((x_fixed >> 16) as i32, y);
                x_fixed += slope;
            }
        } else {
            let slope = (((i64::from(dv)) << 16) + i64::from(dh) / 2)
                / i64::from(dh);
            let mut y_fixed = ((i64::from(sy)) << 16) | 0x8000;
            y_fixed += slope >> 1;
            let mut x = sx;
            for _ in 0..=dh {
                paint_pen(x, (y_fixed >> 16) as i32);
                x += x_dir;
                y_fixed += slope;
            }
        }
    }

    wrote
}

pub(crate) fn ppc_local_point_in_port_regions(
    surface: PpcQuickDrawSurface,
    (x, y): (i32, i32),
    vis_storage: Option<&[u8]>,
    clip_storage: Option<&[u8]>,
) -> bool {
    let port_h = x + i32::from(surface.left);
    let port_v = y + i32::from(surface.top);
    let port_point = i16::try_from(port_h).ok().zip(i16::try_from(port_v).ok());
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 2-20--2-21 and
    // 2-47--2-49: every QuickDraw primitive is constrained by the
    // intersection of the current port's visRgn and clipRgn.
    port_point.is_some_and(|(h, v)| {
        vis_storage.is_none_or(|storage| ppc_point_in_region_storage(storage, v, h))
            && clip_storage.is_none_or(|storage| ppc_point_in_region_storage(storage, v, h))
    })
}

pub(crate) fn ppc_draw_oval(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    rect_ptr: u32,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
    frame_only: bool,
) -> bool {
    let Some(rect) = ppc_read_rect(memory, rect_ptr) else {
        return false;
    };
    ppc_draw_oval_bounds(
        memory,
        gworlds,
        current_gworld,
        rect,
        color,
        explicit_index,
        frame_only,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_draw_oval_bounds(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    rect: (i16, i16, i16, i16),
    color: PpcRgbColor,
    explicit_index: Option<u8>,
    frame_only: bool,
) -> bool {
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    let (top, left, bottom, right) = surface.local_rect(rect);
    let width = right - left;
    let height = bottom - top;
    if width <= 0 || height <= 0 || !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }
    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return false;
    };
    let pen_height = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET))
            .unwrap_or(1),
    )
    .max(1);
    let pen_width = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2))
            .unwrap_or(1),
    )
    .max(1);
    let spans = TrapDispatcher::compute_oval_spans(width as i16, height as i16);
    let inner_left = left + pen_width;
    let inner_top = top + pen_height;
    let inner_width = width - 2 * pen_width;
    let inner_height = height - 2 * pen_height;
    let inner_spans = TrapDispatcher::compute_oval_spans(
        inner_width.max(0) as i16,
        inner_height.max(0) as i16,
    );
    let clip_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
        .and_then(|clip_rgn| ppc_region_storage(memory, clip_rgn));
    let vis_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|vis_rgn| ppc_region_storage(memory, vis_rgn));
    let mut wrote = false;
    for y in top.max(0)..bottom.min(front_buffer.height as i32) {
        let row = (y - top) as usize;
        let Some(&(span_left, span_right)) = spans.get(row) else {
            continue;
        };
        let outer_left = left + i32::from(span_left);
        let outer_right = left + i32::from(span_right);
        for x in outer_left.max(0)..outer_right.min(front_buffer.width as i32) {
            let inner = if frame_only && y >= inner_top && y < inner_top + inner_height {
                let inner_row = (y - inner_top) as usize;
                inner_spans.get(inner_row).is_some_and(|&(span_left, span_right)| {
                    x >= inner_left + i32::from(span_left)
                        && x < inner_left + i32::from(span_right)
                })
            } else {
                false
            };
            if (!frame_only || !inner)
                && ppc_local_point_in_port_regions(
                    surface,
                    (x, y),
                    vis_storage.as_deref(),
                    clip_storage.as_deref(),
                )
            {
                wrote |= ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), color_pixel);
            }
        }
    }
    wrote
}

pub(crate) fn ppc_paint_arc(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    rect_ptr: u32,
    start_angle: i16,
    arc_angle: i16,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    // Inside Macintosh: Imaging With QuickDraw, pp. 3-73–3-74: PaintArc
    // paints the wedge inside the oval, with zero at 12 o'clock and positive
    // angles proceeding clockwise.
    let Some(rect) = ppc_read_rect(memory, rect_ptr) else {
        return false;
    };
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    let (top, left, bottom, right) = surface.local_rect(rect);
    let width = right - left;
    let height = bottom - top;
    if width <= 0
        || height <= 0
        || arc_angle == 0
        || !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16)
    {
        return false;
    }
    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return false;
    };

    let mut a_start = start_angle as f64;
    let mut a_extent = arc_angle as f64;
    if a_extent < 0.0 {
        a_start += a_extent;
        a_extent = -a_extent;
    }
    if a_extent > 360.0 {
        a_extent = 360.0;
    }
    a_start = a_start.rem_euclid(360.0);
    let a_end = a_start + a_extent;

    let cx = (left as f64 + right as f64) / 2.0;
    let cy = (top as f64 + bottom as f64) / 2.0;
    let rx = width as f64 / 2.0;
    let ry = height as f64 / 2.0;

    let mut wrote = false;
    for y in top.max(0)..bottom.min(front_buffer.height as i32) {
        for x in left.max(0)..right.min(front_buffer.width as i32) {
            let dx = (x as f64 - cx + 0.5) / rx;
            let dy = (y as f64 - cy + 0.5) / ry;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq > 1.0 {
                continue;
            }
            let angle = (-(y as f64 - cy + 0.5)).atan2(x as f64 - cx + 0.5);
            let mut mac_angle = 90.0 - angle.to_degrees();
            if mac_angle < 0.0 {
                mac_angle += 360.0;
            }
            let in_arc = (mac_angle >= a_start && mac_angle < a_end)
                || (a_end > 360.0 && mac_angle + 360.0 < a_end);
            if in_arc {
                wrote |= ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), color_pixel);
            }
        }
    }
    wrote
}

pub(crate) fn ppc_paint_region(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    region_handle: u32,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    let Some(storage) = ppc_region_storage(memory, region_handle) else {
        return false;
    };
    let Some((top, _, bottom, _)) = ppc_region_storage_bbox(&storage) else {
        return true;
    };
    let Some(rows) = ppc_region_rows_for_band(&storage, top, bottom) else {
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
    let mut wrote = false;
    for (row, endpoints) in rows.iter().enumerate() {
        let y = i32::from(top) + row as i32 - i32::from(surface.top);
        for interval in endpoints.chunks_exact(2) {
            for x in (i32::from(interval[0]) - i32::from(surface.left))
                ..(i32::from(interval[1]) - i32::from(surface.left))
            {
                wrote |= ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), color_pixel);
            }
        }
    }
    wrote
}

pub(crate) fn ppc_frame_region(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    region_handle: u32,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    // Inside Macintosh: Imaging With QuickDraw, pp. 3-100–3-101: FrameRgn
    // draws the outline just inside the region boundary using the pen size.
    let Some(storage) = ppc_region_storage(memory, region_handle) else {
        return false;
    };
    let Some((top, _, bottom, _)) = ppc_region_storage_bbox(&storage) else {
        return true;
    };
    let Some(rows) = ppc_region_rows_for_band(&storage, top, bottom) else {
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
    let pen_height = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET))
            .unwrap_or(1),
    )
    .max(1);
    let pen_width = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2))
            .unwrap_or(1),
    )
    .max(1);

    let row_contains = |row_idx: i32, x: i16| -> bool {
        if row_idx < 0 || (row_idx as usize) >= rows.len() {
            return false;
        }
        rows[row_idx as usize]
            .chunks_exact(2)
            .any(|interval| x >= interval[0] && x < interval[1])
    };

    let mut wrote = false;
    for (row_idx, endpoints) in rows.iter().enumerate() {
        let v = i32::from(top) + row_idx as i32;
        let y = v - i32::from(surface.top);
        for interval in endpoints.chunks_exact(2) {
            for h in interval[0]..interval[1] {
                let x = i32::from(h) - i32::from(surface.left);
                let inside_inset = (row_idx as i32 - pen_height
                    ..=row_idx as i32 + pen_height)
                    .all(|source_row| {
                        row_contains(source_row, h.saturating_sub(pen_width as i16))
                            && row_contains(source_row, h.saturating_add(pen_width as i16))
                    });
                if !inside_inset {
                    wrote |=
                        ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), color_pixel);
                }
            }
        }
    }
    wrote
}

pub(crate) fn ppc_invert_region(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    region_handle: u32,
) -> bool {
    let Some(storage) = ppc_region_storage(memory, region_handle) else {
        return false;
    };
    let Some((top, _, bottom, _)) = ppc_region_storage_bbox(&storage) else {
        return true;
    };
    let Some(rows) = ppc_region_rows_for_band(&storage, top, bottom) else {
        return false;
    };
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    let mask = match front_buffer.depth {
        depth @ (1 | 2 | 4 | 8) => (1u16 << depth) - 1,
        16 => 0x7fff,
        _ => return false,
    };
    let mut wrote = false;
    for (row, endpoints) in rows.iter().enumerate() {
        let y = i32::from(top) + row as i32 - i32::from(surface.top);
        for interval in endpoints.chunks_exact(2) {
            for x in (i32::from(interval[0]) - i32::from(surface.left))
                ..(i32::from(interval[1]) - i32::from(surface.left))
            {
                if let Some(pixel) = ppc_quickdraw_read_pixel(memory, front_buffer, (x, y)) {
                    wrote |= ppc_invert_pixel_detail(memory, front_buffer, (x, y), pixel, mask);
                }
            }
        }
    }
    wrote
}

pub(crate) fn ppc_current_text_font(memory: &mut PpcSectionMem, current_gworld: u32) -> i16 {
    if current_gworld == 0 {
        return PPC_QD_TEXT_FONT_DEFAULT;
    }
    memory
        .read_u16_be(current_gworld + PPC_CGRAF_PORT_TX_FONT_OFFSET)
        .map(|font| font as i16)
        .unwrap_or(PPC_QD_TEXT_FONT_DEFAULT)
}

pub(crate) fn ppc_current_text_style(memory: &mut PpcSectionMem, current_gworld: u32) -> u8 {
    if current_gworld == 0 {
        return 0;
    }
    memory
        .read_u8(current_gworld + PPC_CGRAF_PORT_TX_FACE_OFFSET)
        .unwrap_or(0)
}

pub(crate) fn ppc_text_byte_advance_for_font(ch: u8, text_font: i16, text_size: i16) -> i16 {
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    get_glyph(text_font, face.size, ch as char)
        .map(|(glyph, _)| ppc_scale_font_value(i32::from(glyph.advance), numerator, denominator))
        .unwrap_or(6)
}

#[cfg(test)]
pub(crate) fn ppc_text_byte_advance(ch: u8, text_size: i16) -> i16 {
    ppc_text_byte_advance_for_font(ch, PPC_QD_TEXT_FONT_DEFAULT, text_size)
}

pub(crate) fn ppc_text_bytes_advance_for_font(bytes: &[u8], text_font: i16, text_size: i16) -> i16 {
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    let base_advance = bytes.iter().fold(0i32, |advance, ch| {
        advance.saturating_add(
            get_glyph(text_font, face.size, *ch as char)
                .map(|(glyph, _)| i32::from(glyph.advance))
                .unwrap_or(6),
        )
    });
    ppc_scale_font_value(base_advance, numerator, denominator)
}

// Inside Macintosh: Text (1993), Text Utilities, TruncString. Use the
// current QuickDraw font and Roman ellipsis, retaining Pascal byte lengths.
pub(crate) fn ppc_trunc_string(
    memory: &mut PpcSectionMem,
    string: u32,
    width: i16,
    where_: u16,
    font: i16,
    size: i16,
    style: u8,
) -> i16 {
    let Some(bytes) = ppc_read_pstring_bytes(memory, string) else {
        return -1;
    };
    let measure = |text: &[u8]| ppc_text_width_bytes(font, size, style, text);
    if measure(&bytes) <= width {
        return 0;
    }
    const ELLIPSIS: u8 = 0xc9;
    if !matches!(where_, 0 | 0x4000) || measure(&[ELLIPSIS]) > width {
        return -1;
    }
    // A Pascal string has at most 255 bytes. Measure complete candidates so
    // style advances and font scaling agree with StringWidth at the boundary.
    for retained in (0..bytes.len()).rev() {
        let left = if where_ == 0x4000 {
            retained.div_ceil(2)
        } else {
            retained
        };
        let right = retained - left;
        let mut candidate = bytes[..left].to_vec();
        candidate.push(ELLIPSIS);
        candidate.extend_from_slice(&bytes[bytes.len() - right..]);
        if measure(&candidate) <= width {
            return if ppc_write_pstring_bytes(memory, string, &candidate) {
                1
            } else {
                -1
            };
        }
    }
    -1
}

pub(crate) fn ppc_text_width_bytes(text_font: i16, text_size: i16, style: u8, bytes: &[u8]) -> i16 {
    let style = QuickDrawTextStyle::from_bits(style);
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    let base_advance = bytes.iter().fold(0i32, |advance, ch| {
        advance.saturating_add(
            get_glyph(text_font, face.size, *ch as char)
                .map(|(glyph, _)| style.glyph_advance(i32::from(glyph.advance)))
                .unwrap_or_else(|| style.glyph_advance(6)),
        )
    });
    ppc_scale_font_value(base_advance, numerator, denominator)
}

pub(crate) fn ppc_scale_font_value(value: i32, numerator: i32, denominator: i32) -> i16 {
    let scaled = value
        .saturating_mul(numerator)
        .saturating_add(denominator / 2)
        / denominator;
    i16::try_from(scaled).unwrap_or(if scaled < 0 { i16::MIN } else { i16::MAX })
}

pub(crate) fn ppc_scale_font_floor(value: i32, numerator: i32, denominator: i32) -> i32 {
    let scaled = value.saturating_mul(numerator);
    if scaled >= 0 {
        scaled / denominator
    } else {
        -((-scaled).saturating_add(denominator - 1) / denominator)
    }
}

#[cfg(test)]
pub(crate) fn ppc_text_bytes_advance(bytes: &[u8], text_size: i16) -> i16 {
    ppc_text_bytes_advance_for_font(bytes, PPC_QD_TEXT_FONT_DEFAULT, text_size)
}

pub(crate) fn ppc_read_pascal_string(memory: &mut PpcSectionMem, string_ptr: u32) -> Option<Vec<u8>> {
    let len = memory.read_u8(string_ptr)? as u32;
    let mut bytes = Vec::with_capacity(len as usize);
    for offset in 0..len {
        bytes.push(memory.read_u8(string_ptr.checked_add(1 + offset)?)?);
    }
    Some(bytes)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_begin_outline_text_glyph(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    vis: Option<&[u8]>,
    clip: Option<&[u8]>,
    glyph: &crate::quickdraw::fonts::Glyph,
    data: &[u8],
    h: i32,
    v: i32,
    color: u16,
    style: QuickDrawTextStyle,
    italic: Option<i16>,
    underline: Option<(i16, i16)>,
) {
    let mut slot = memory.presentation();
    if slot.is_none() || !matches!(surface.front_buffer.depth, 8 | 16) {
        return;
    }
    let (Ok(h), Ok(v)) = (i16::try_from(h), i16::try_from(v)) else {
        return;
    };
    slot.begin_outline_glyph(glyph, data, h, v, style.bold(), italic, underline);
    slot.style_outline_glyph(style);
    let bounds = slot.as_ref().and_then(|p| p.glyph_bounds());
    let Some((top, left, bottom, right)) = bounds else {
        return;
    };
    let fb = surface.front_buffer;
    let lanes = fb.depth / 8;
    for y in top.max(0)..bottom.min(fb.height as i32) {
        for x in left.max(0)..right.min(fb.width as i32) {
            if !ppc_local_point_in_port_regions(surface, (x, y), vis, clip) {
                continue;
            }
            let address = fb.base_addr + y as u32 * fb.row_bytes + x as u32 * lanes;
            for lane in 0..lanes {
                let foreground = (color >> ((lanes - 1 - lane) * 8)) as u8;
                let Some(background) = memory.read_u8(address + lane) else {
                    continue;
                };
                if let Some(mut p) = slot.as_mut() {
                    p.glyph_pixel(address + lane, x as i16, y as i16, foreground, background);
                }
            }
        }
    }
}

pub(crate) fn ppc_apply_text_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    vis_storage: Option<&[u8]>,
    clip_storage: Option<&[u8]>,
    point: (i32, i32),
    color_pixel: u16,
    text_mode: i16,
) -> bool {
    if !ppc_local_point_in_port_regions(surface, point, vis_storage, clip_storage) {
        return false;
    }
    let front_buffer = surface.front_buffer;
    let mode = text_mode & 0x3f;
    match mode {
        2 => {
            let Some(dst) = ppc_quickdraw_read_pixel(memory, front_buffer, point) else {
                return false;
            };
            ppc_quickdraw_write_raw_pixel(memory, front_buffer, point, dst ^ color_pixel)
        }
        3 => {
            let Some(dst) = ppc_quickdraw_read_pixel(memory, front_buffer, point) else {
                return false;
            };
            ppc_quickdraw_write_raw_pixel(memory, front_buffer, point, dst & !color_pixel)
        }
        _ => ppc_quickdraw_write_raw_pixel(memory, front_buffer, point, color_pixel),
    }
}

pub(crate) fn ppc_draw_text_bytes(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    pen: (i16, i16),
    text_font: i16,
    text_size: i16,
    text_mode: i16,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
    bytes: &[u8],
) -> i16 {
    ppc_draw_text_bytes_clipped(
        memory,
        gworlds,
        current_gworld,
        pen,
        text_font,
        text_size,
        text_mode,
        color,
        explicit_index,
        None,
        bytes,
    )
}

/// [`ppc_draw_text_bytes`], additionally clipped to `clip_rect` in port
/// coordinates.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_draw_text_bytes_clipped(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    pen: (i16, i16),
    text_font: i16,
    text_size: i16,
    text_mode: i16,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
    clip_rect: Option<(i16, i16, i16, i16)>,
    bytes: &[u8],
) -> i16 {
    let advance = ppc_text_bytes_advance_for_font(bytes, text_font, text_size);
    ppc_draw_text_chars(
        memory,
        gworlds,
        current_gworld,
        pen,
        text_font,
        text_size,
        text_mode,
        color,
        explicit_index,
        clip_rect,
        bytes.iter().map(|byte| char::from(*byte)),
    );
    advance
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_draw_text_bytes_styled(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    pen: (i16, i16),
    text_font: i16,
    text_size: i16,
    text_mode: i16,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
    style: u8,
    bytes: &[u8],
) -> i16 {
    ppc_draw_text_bytes_styled_clipped(
        memory,
        gworlds,
        current_gworld,
        pen,
        text_font,
        text_size,
        text_mode,
        color,
        explicit_index,
        style,
        None,
        bytes,
    )
}

/// [`ppc_draw_text_bytes_styled`], additionally clipped to `clip_rect` in
/// port coordinates.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_draw_text_bytes_styled_clipped(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    pen: (i16, i16),
    text_font: i16,
    text_size: i16,
    text_mode: i16,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
    style: u8,
    clip_rect: Option<(i16, i16, i16, i16)>,
    bytes: &[u8],
) -> i16 {
    let style = QuickDrawTextStyle::from_bits(style);
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    let base_advance = bytes.iter().fold(0i32, |advance, ch| {
        advance.saturating_add(
            get_glyph(text_font, face.size, *ch as char)
                .map(|(glyph, _)| style.glyph_advance(i32::from(glyph.advance)))
                .unwrap_or_else(|| style.glyph_advance(6)),
        )
    });
    let advance = ppc_scale_font_value(base_advance, numerator, denominator);
    if style.is_plain() {
        ppc_draw_text_chars(
            memory,
            gworlds,
            current_gworld,
            pen,
            text_font,
            text_size,
            text_mode,
            color,
            explicit_index,
            clip_rect,
            bytes.iter().map(|byte| char::from(*byte)),
        );
    } else {
        ppc_draw_text_chars_styled(
            memory,
            gworlds,
            current_gworld,
            pen,
            text_font,
            text_size,
            text_mode,
            color,
            explicit_index,
            style,
            base_advance,
            clip_rect,
            bytes.iter().map(|byte| char::from(*byte)),
        );
    }
    advance
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_draw_text_chars(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    pen: (i16, i16),
    text_font: i16,
    text_size: i16,
    text_mode: i16,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
    clip_rect: Option<(i16, i16, i16, i16)>,
    chars: impl IntoIterator<Item = char>,
) {
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return;
    };
    let front_buffer = surface.front_buffer;
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return;
    }
    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return;
    };
    let clip_storage = ppc_port_clip_storage(memory, current_gworld, clip_rect);
    let vis_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|vis_rgn| ppc_region_storage(memory, vis_rgn));

    let (local_h, local_v) = surface.local_point((i32::from(pen.0), i32::from(pen.1)));
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    let mut base_advance = 0i32;
    for ch in chars {
        if let Some((glyph, data)) = get_glyph(text_font, face.size, ch) {
            if matches!(text_mode & 0x3f, 0 | 1) && numerator == denominator {
                ppc_begin_outline_text_glyph(
                    memory,
                    surface,
                    vis_storage.as_deref(),
                    clip_storage.as_deref(),
                    glyph,
                    data,
                    local_h + base_advance,
                    local_v,
                    color_pixel,
                    QuickDrawTextStyle::from_bits(0),
                    None,
                    None,
                );
            }
            let width = glyph.width as usize;
            let height = glyph.height as usize;
            for row in 0..height {
                for col in 0..width {
                    let index = glyph.data_offset + row * width + col;
                    if index < data.len() && data[index] >= 128 {
                        let source_x = base_advance + i32::from(glyph.origin_x) + col as i32;
                        let source_y = i32::from(glyph.origin_y) + row as i32;
                        let left = local_h + ppc_scale_font_floor(source_x, numerator, denominator);
                        let right = local_h
                            + ppc_scale_font_floor(source_x + 1, numerator, denominator)
                                .max(ppc_scale_font_floor(source_x, numerator, denominator) + 1);
                        let top = local_v + ppc_scale_font_floor(source_y, numerator, denominator);
                        let bottom = local_v
                            + ppc_scale_font_floor(source_y + 1, numerator, denominator)
                                .max(ppc_scale_font_floor(source_y, numerator, denominator) + 1);
                        for y in top..bottom {
                            for x in left..right {
                                let _ = ppc_apply_text_pixel(
                                    memory,
                                    surface,
                                    vis_storage.as_deref(),
                                    clip_storage.as_deref(),
                                    (x, y),
                                    color_pixel,
                                    text_mode,
                                );
                            }
                        }
                    }
                }
            }
            memory.presentation().end_outline_glyph();
            base_advance = base_advance.saturating_add(i32::from(glyph.advance));
        } else {
            base_advance = base_advance.saturating_add(6);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_apply_scaled_text_source_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    vis_storage: Option<&[u8]>,
    clip_storage: Option<&[u8]>,
    local_h: i32,
    local_v: i32,
    source_x: i32,
    source_y: i32,
    numerator: i32,
    denominator: i32,
    color_pixel: u16,
    text_mode: i16,
) {
    let scaled_x = ppc_scale_font_floor(source_x, numerator, denominator);
    let scaled_y = ppc_scale_font_floor(source_y, numerator, denominator);
    let left = local_h + scaled_x;
    let right =
        local_h + ppc_scale_font_floor(source_x + 1, numerator, denominator).max(scaled_x + 1);
    let top = local_v + scaled_y;
    let bottom =
        local_v + ppc_scale_font_floor(source_y + 1, numerator, denominator).max(scaled_y + 1);
    for y in top..bottom {
        for x in left..right {
            let _ = ppc_apply_text_pixel(
                memory,
                surface,
                vis_storage,
                clip_storage,
                (x, y),
                color_pixel,
                text_mode,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_draw_text_chars_styled(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    pen: (i16, i16),
    text_font: i16,
    text_size: i16,
    text_mode: i16,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
    style: QuickDrawTextStyle,
    line_advance: i32,
    clip_rect: Option<(i16, i16, i16, i16)>,
    chars: impl IntoIterator<Item = char>,
) {
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return;
    };
    let front_buffer = surface.front_buffer;
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return;
    }
    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return;
    };
    let clip_storage = ppc_port_clip_storage(memory, current_gworld, clip_rect);
    let vis_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|vis_rgn| ppc_region_storage(memory, vis_rgn));
    let (local_h, local_v) = surface.local_point((i32::from(pen.0), i32::from(pen.1)));
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    let metrics = get_font_metrics(text_font, face.size);
    let mut source_advance = 0i32;

    for ch in chars {
        let (glyph_hit, synthetic_italic) = if style.italic() {
            if let Some(hit) = get_glyph_italic(text_font, face.size, ch) {
                (Some(hit), false)
            } else {
                (get_glyph(text_font, face.size, ch), true)
            }
        } else {
            (get_glyph(text_font, face.size, ch), false)
        };
        let Some((glyph, data)) = glyph_hit else {
            source_advance = source_advance.saturating_add(6);
            continue;
        };
        if matches!(text_mode & 0x3f, 0 | 1) && numerator == denominator {
            ppc_begin_outline_text_glyph(
                memory,
                surface,
                vis_storage.as_deref(),
                clip_storage.as_deref(),
                glyph,
                data,
                local_h + source_advance,
                local_v,
                color_pixel,
                style,
                synthetic_italic.then_some(metrics.descent),
                style.underline().then_some((
                    glyph.advance as i16,
                    get_underline_thickness(text_font, face.size).max(1),
                )),
            );
        }
        let mut base_pixels = HashSet::new();
        for row in 0..glyph.height as usize {
            for col in 0..glyph.width as usize {
                let index = glyph.data_offset + row * glyph.width as usize + col;
                if index >= data.len() || data[index] < 128 {
                    continue;
                }
                let source_y = i32::from(glyph.origin_y) + row as i32;
                let slant = synthetic_italic.then(|| {
                    get_italic_slant(
                        text_font,
                        face.size,
                        &metrics,
                        0,
                        i16::try_from(source_y).unwrap_or(0),
                    )
                });
                let source_x = source_advance
                    + i32::from(glyph.origin_x)
                    + col as i32
                    + i32::from(slant.unwrap_or(0));
                base_pixels.insert((source_x, source_y));
                if style.bold() {
                    base_pixels.insert((source_x + 1, source_y));
                }
            }
        }

        if style.underline() && style.smear_max().is_some() && line_advance > 0 {
            let underline_offset: i32 = if style.shadow() { -1 } else { 0 };
            let synthetic_italic = style.italic()
                && get_glyph_italic(text_font, face.size, 'A').is_none();
            let underline_left = if synthetic_italic {
                get_italic_underline_extend_left(
                    text_font,
                    face.size,
                    style.bold(),
                    false,
                )
            } else {
                0
            };
            let underline_right = if synthetic_italic {
                get_italic_end_extend(text_font, face.size, &metrics)
            } else {
                0
            };
            let final_effect_advance = style.glyph_advance(0);
            for source_x in underline_offset.saturating_sub(i32::from(underline_left))
                ..line_advance
                    .saturating_sub(final_effect_advance)
                    .saturating_add(underline_offset)
                    .saturating_add(i32::from(underline_right))
            {
                base_pixels.insert((source_x, 1));
            }
        }

        if let Some(smear_max) = style.smear_max() {
            let min_x = base_pixels
                .iter()
                .map(|(x, _)| *x)
                .min()
                .unwrap_or(source_advance)
                - 1;
            let max_x = base_pixels
                .iter()
                .map(|(x, _)| *x)
                .max()
                .unwrap_or(source_advance)
                + smear_max;
            let min_y = base_pixels.iter().map(|(_, y)| *y).min().unwrap_or(0) - 1;
            let max_y = base_pixels.iter().map(|(_, y)| *y).max().unwrap_or(0) + smear_max;
            for source_y in min_y..=max_y {
                for source_x in min_x..=max_x {
                    if base_pixels.contains(&(source_x, source_y)) {
                        continue;
                    }
                    let smeared = (-1..=smear_max).any(|dy| {
                        (-1..=smear_max)
                            .any(|dx| base_pixels.contains(&(source_x - dx, source_y - dy)))
                    });
                    if smeared {
                        ppc_apply_scaled_text_source_pixel(
                            memory,
                            surface,
                            vis_storage.as_deref(),
                            clip_storage.as_deref(),
                            local_h,
                            local_v,
                            source_x,
                            source_y,
                            numerator,
                            denominator,
                            color_pixel,
                            text_mode,
                        );
                    }
                }
            }
        } else {
            for (source_x, source_y) in base_pixels.iter().copied() {
                ppc_apply_scaled_text_source_pixel(
                    memory,
                    surface,
                    vis_storage.as_deref(),
                    clip_storage.as_deref(),
                    local_h,
                    local_v,
                    source_x,
                    source_y,
                    numerator,
                    denominator,
                    color_pixel,
                    text_mode,
                );
            }
        }
        memory.presentation().end_outline_glyph();
        source_advance =
            source_advance.saturating_add(style.glyph_advance(i32::from(glyph.advance)));
    }

    if style.underline() && style.smear_max().is_none() && source_advance > 0 {
        let thickness = get_underline_thickness(text_font, face.size).max(1);
        for dy in 1..=i32::from(thickness) {
            for source_x in 0..source_advance {
                ppc_apply_scaled_text_source_pixel(
                    memory,
                    surface,
                    vis_storage.as_deref(),
                    clip_storage.as_deref(),
                    local_h,
                    local_v,
                    source_x,
                    dy,
                    numerator,
                    denominator,
                    color_pixel,
                    text_mode,
                );
            }
        }
    }
}

pub(crate) fn ppc_paint_rect(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    fore_color: PpcRgbColor,
    explicit_index: Option<u8>,
    back_color: PpcRgbColor,
    pattern: &[u8; 8],
) -> bool {
    let Some(rect) = ppc_read_rect(memory, cpu.gpr[3]) else {
        return false;
    };
    if pattern.iter().all(|row| *row == 0xff) {
        return ppc_paint_rect_bounds(
            memory,
            gworlds,
            current_gworld,
            rect,
            fore_color,
            explicit_index,
        );
    }
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }
    let Some(fore_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, fore_color, explicit_index)
    else {
        return false;
    };
    let Some(back_pixel) = ppc_quickdraw_surface_color_pixel(memory, surface, back_color) else {
        return false;
    };
    let (top, left, bottom, right) = surface.local_rect(rect);
    let left = left.max(0).min(front_buffer.width as i32);
    let top = top.max(0).min(front_buffer.height as i32);
    let right = right.max(0).min(front_buffer.width as i32);
    let bottom = bottom.max(0).min(front_buffer.height as i32);
    let clip_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
        .and_then(|clip_rgn| ppc_region_storage(memory, clip_rgn));
    let vis_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|vis_rgn| ppc_region_storage(memory, vis_rgn));
    let mut wrote = false;
    for y in top..bottom {
        let pattern_row = pattern[(y as usize) & 7];
        for x in left..right {
            if !ppc_local_point_in_port_regions(
                surface,
                (x, y),
                vis_storage.as_deref(),
                clip_storage.as_deref(),
            ) {
                continue;
            }
            let mask = 0x80 >> ((x as usize) & 7);
            let pixel = if pattern_row & mask != 0 {
                fore_pixel
            } else {
                back_pixel
            };
            wrote |= ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), pixel);
        }
    }
    wrote
}

pub(crate) fn ppc_paint_rect_bounds(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    rect: (i16, i16, i16, i16),
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }

    let (top, left, bottom, right) = surface.local_rect(rect);
    let left = left.max(0).min(front_buffer.width as i32);
    let top = top.max(0).min(front_buffer.height as i32);
    let right = right.max(0).min(front_buffer.width as i32);
    let bottom = bottom.max(0).min(front_buffer.height as i32);
    if left >= right || top >= bottom {
        return false;
    }

    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return false;
    };
    let clip_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
        .and_then(|clip_rgn| ppc_region_storage(memory, clip_rgn));
    let vis_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|vis_rgn| ppc_region_storage(memory, vis_rgn));
    // Imaging With QuickDraw (1994), pp. 2-20--2-21: every destination pixel
    // is constrained by visRgn ∩ clipRgn. Per-pixel writes also preserve the
    // neighboring fields of packed 1/2/4-bit PixMaps.
    if matches!(front_buffer.depth, 8 | 16)
        && [
            top + i32::from(surface.top),
            bottom + i32::from(surface.top),
            left + i32::from(surface.left),
            right + i32::from(surface.left),
        ]
        .into_iter()
        .all(|value| i16::try_from(value).is_ok())
    {
        let port_top = (top + i32::from(surface.top)) as i16;
        let port_bottom = (bottom + i32::from(surface.top)) as i16;
        let port_left = (left + i32::from(surface.left)) as i16;
        let port_right = (right + i32::from(surface.left)) as i16;
        let mut rows = vec![
            vec![port_left, port_right];
            (port_bottom as i32 - port_top as i32).max(0) as usize
        ];
        for storage in [vis_storage.as_deref(), clip_storage.as_deref()]
            .into_iter()
            .flatten()
        {
            let Some(clip_rows) = ppc_region_rows_for_band(storage, port_top, port_bottom) else {
                return false;
            };
            for (row, clip) in rows.iter_mut().zip(clip_rows) {
                *row = ppc_region_intersect_rows(row, &clip);
            }
        }
        let lanes = (front_buffer.depth / 8) as usize;
        let pixel = color_pixel.to_be_bytes();
        let row_bytes: Vec<_> = (left..right)
            .flat_map(|_| pixel[2 - lanes..].iter().copied())
            .collect();
        let mut wrote = false;
        for (dy, row) in rows.iter().enumerate() {
            let y = i32::from(port_top) + dy as i32 - i32::from(surface.top);
            for pair in row.chunks_exact(2) {
                let x = i32::from(pair[0]) - i32::from(surface.left);
                let len = (i32::from(pair[1]) - i32::from(pair[0])) as usize * lanes;
                let address = front_buffer.base_addr
                    + y as u32 * front_buffer.row_bytes
                    + x as u32 * lanes as u32;
                wrote |= memory.write_bytes(address, &row_bytes[..len]).is_some();
            }
        }
        return wrote;
    }
    let mut wrote = false;
    for y in top..bottom {
        for x in left..right {
            if ppc_local_point_in_port_regions(
                surface,
                (x, y),
                vis_storage.as_deref(),
                clip_storage.as_deref(),
            ) {
                wrote |= ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), color_pixel);
            }
        }
    }
    wrote
}

pub(crate) fn ppc_invert_rect(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
) -> bool {
    let Some(rect) = ppc_read_rect(memory, cpu.gpr[3]) else {
        return false;
    };
    ppc_invert_rect_bounds(memory, gworlds, current_gworld, rect)
}

pub(crate) fn ppc_invert_pixel_detail(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    point: (i32, i32),
    pixel: u16,
    mask: u16,
) -> bool {
    let mut detail = crate::memory::SavedPixels::<()>::default();
    ppc_capture_saved_detail(memory, front, point, &mut detail, 0);
    let wrote = ppc_quickdraw_write_raw_pixel(memory, front, point, pixel ^ mask);
    if wrote && matches!(front.depth, 8 | 16) {
        let lanes = (front.depth / 8) as usize;
        detail.transform_detail(|offset, value| value ^ (mask >> ((lanes - 1 - offset) * 8)) as u8);
        ppc_restore_saved_detail(memory, front, point, &detail, 0);
    }
    wrote
}

pub(crate) fn ppc_invert_rect_bounds(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    rect: (i16, i16, i16, i16),
) -> bool {
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    let mask = match front_buffer.depth {
        depth @ (1 | 2 | 4 | 8) => (1u16 << depth) - 1,
        16 => 0x7fff,
        _ => return false,
    };
    let (top, left, bottom, right) = surface.local_rect(rect);
    let left = left.max(0).min(front_buffer.width as i32);
    let top = top.max(0).min(front_buffer.height as i32);
    let right = right.max(0).min(front_buffer.width as i32);
    let bottom = bottom.max(0).min(front_buffer.height as i32);
    if left >= right || top >= bottom {
        return false;
    }

    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 3-42--3-43:
    // InvertRect complements every destination pixel in the rectangle and is
    // independent of the current foreground color and pen pattern. The same
    // volume, pp. 2-13--2-14, defines a basic BitMap as one bit per pixel, so
    // monochrome offscreen ports must participate in that complement too.
    let mut wrote = false;
    for y in top..bottom {
        for x in left..right {
            if let Some(pixel) = ppc_quickdraw_read_pixel(memory, front_buffer, (x, y)) {
                wrote |= ppc_invert_pixel_detail(memory, front_buffer, (x, y), pixel, mask);
            }
        }
    }
    wrote
}

pub(crate) fn ppc_frame_rect(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    let Some(rect) = ppc_read_rect(memory, cpu.gpr[3]) else {
        return false;
    };
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }

    let (top, left, bottom, right) = surface.local_rect(rect);
    let left = left.max(0).min(front_buffer.width as i32);
    let top = top.max(0).min(front_buffer.height as i32);
    let right = right.max(0).min(front_buffer.width as i32);
    let bottom = bottom.max(0).min(front_buffer.height as i32);
    if left >= right || top >= bottom {
        return false;
    }
    let pen_height = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET))
            .unwrap_or(1),
    )
    .min(bottom - top);
    let pen_width = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2))
            .unwrap_or(1),
    )
    .min(right - left);
    if pen_width == 0 || pen_height == 0 {
        return true;
    }

    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return false;
    };
    let mut wrote = false;
    for dy in 0..pen_height {
        for x in left..right {
            wrote |=
                ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, top + dy), color_pixel);
            wrote |= ppc_quickdraw_write_raw_pixel(
                memory,
                front_buffer,
                (x, bottom - 1 - dy),
                color_pixel,
            );
        }
    }
    for dx in 0..pen_width {
        for y in top..bottom {
            wrote |=
                ppc_quickdraw_write_raw_pixel(memory, front_buffer, (left + dx, y), color_pixel);
            wrote |= ppc_quickdraw_write_raw_pixel(
                memory,
                front_buffer,
                (right - 1 - dx, y),
                color_pixel,
            );
        }
    }
    wrote
}

pub(crate) fn ppc_frame_round_rect(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    let Some(rect) = ppc_read_rect(memory, cpu.gpr[3]) else {
        return false;
    };
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }
    let (top, left, bottom, right) = surface.local_rect(rect);
    let left = left.max(0).min(front_buffer.width as i32);
    let top = top.max(0).min(front_buffer.height as i32);
    let right = right.max(0).min(front_buffer.width as i32);
    let bottom = bottom.max(0).min(front_buffer.height as i32);
    if left >= right || top >= bottom {
        return false;
    }
    let pen_height = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET))
            .unwrap_or(1),
    )
    .min(bottom - top);
    let pen_width = i32::from(
        memory
            .read_u16_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2))
            .unwrap_or(1),
    )
    .min(right - left);
    if pen_width == 0 || pen_height == 0 {
        return true;
    }
    let oval_width = (cpu.gpr[4] as u16 as i16).max(0) as i32;
    let oval_height = (cpu.gpr[5] as u16 as i16).max(0) as i32;
    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return false;
    };
    let clip_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
        .and_then(|clip_rgn| ppc_region_storage(memory, clip_rgn));
    let vis_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|vis_rgn| ppc_region_storage(memory, vis_rgn));
    let inner_left = left + pen_width;
    let inner_top = top + pen_height;
    let inner_right = right - pen_width;
    let inner_bottom = bottom - pen_height;
    let mut wrote = false;
    for y in top..bottom {
        for x in left..right {
            let outer =
                ppc_point_in_rounded_rect(x, y, left, top, right, bottom, oval_width, oval_height);
            let inner = inner_left < inner_right
                && inner_top < inner_bottom
                && ppc_point_in_rounded_rect(
                    x,
                    y,
                    inner_left,
                    inner_top,
                    inner_right,
                    inner_bottom,
                    oval_width.saturating_sub(pen_width.saturating_mul(2)),
                    oval_height.saturating_sub(pen_height.saturating_mul(2)),
                );
            if outer
                && !inner
                && ppc_local_point_in_port_regions(
                    surface,
                    (x, y),
                    vis_storage.as_deref(),
                    clip_storage.as_deref(),
                )
            {
                wrote |= ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), color_pixel);
            }
        }
    }
    wrote
}

pub(crate) fn ppc_paint_round_rect(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> bool {
    let Some(rect) = ppc_read_rect(memory, cpu.gpr[3]) else {
        return false;
    };
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return false;
    };
    let front_buffer = surface.front_buffer;
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }
    let (top, left, bottom, right) = surface.local_rect(rect);
    let left = left.max(0).min(front_buffer.width as i32);
    let top = top.max(0).min(front_buffer.height as i32);
    let right = right.max(0).min(front_buffer.width as i32);
    let bottom = bottom.max(0).min(front_buffer.height as i32);
    if left >= right || top >= bottom {
        return false;
    }
    let oval_width = (cpu.gpr[4] as u16 as i16).max(0) as i32;
    let oval_height = (cpu.gpr[5] as u16 as i16).max(0) as i32;
    let Some(color_pixel) =
        ppc_quickdraw_surface_fore_pixel(memory, surface, color, explicit_index)
    else {
        return false;
    };
    let clip_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
        .and_then(|clip_rgn| ppc_region_storage(memory, clip_rgn));
    let vis_storage = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|vis_rgn| ppc_region_storage(memory, vis_rgn));
    let mut wrote = false;
    for y in top..bottom {
        for x in left..right {
            if ppc_point_in_rounded_rect(
                x,
                y,
                left,
                top,
                right,
                bottom,
                oval_width,
                oval_height,
            ) && ppc_local_point_in_port_regions(
                surface,
                (x, y),
                vis_storage.as_deref(),
                clip_storage.as_deref(),
            ) {
                wrote |= ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), color_pixel);
            }
        }
    }
    wrote
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_point_in_rounded_rect(
    x: i32,
    y: i32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    oval_width: i32,
    oval_height: i32,
) -> bool {
    let radius_x = (oval_width.min(right - left).max(0) as f64) / 2.0;
    let radius_y = (oval_height.min(bottom - top).max(0) as f64) / 2.0;
    if radius_x == 0.0 || radius_y == 0.0 {
        return (left..right).contains(&x) && (top..bottom).contains(&y);
    }
    let px = f64::from(x) + 0.5;
    let py = f64::from(y) + 0.5;
    let center_x = if px < f64::from(left) + radius_x {
        f64::from(left) + radius_x
    } else if px > f64::from(right) - radius_x {
        f64::from(right) - radius_x
    } else {
        px
    };
    let center_y = if py < f64::from(top) + radius_y {
        f64::from(top) + radius_y
    } else if py > f64::from(bottom) - radius_y {
        f64::from(bottom) - radius_y
    } else {
        py
    };
    let dx = (px - center_x) / radius_x;
    let dy = (py - center_y) / radius_y;
    dx * dx + dy * dy <= 1.0
}

pub(crate) const PPC_PIXELS_PURGEABLE: u32 = 1 << 6;
pub(crate) const PPC_PIXELS_LOCKED: u32 = 1 << 7;
pub(crate) const PPC_KEEP_LOCAL: u32 = 1 << 3;

pub(crate) fn ppc_gworld_pixel_state_from_mirror(record: &PpcGWorldRecord) -> u32 {
    (u32::from(!record.pixels_no_purge) * PPC_PIXELS_PURGEABLE)
        | (u32::from(record.pixels_locked) * PPC_PIXELS_LOCKED)
}

pub(crate) fn ppc_sync_gworld_pixel_state_mirrors(
    gworlds: &mut [PpcGWorldRecord],
    pixel_states: &SharedProcessQuickDrawPixelStates,
) {
    for record in gworlds {
        if record.pixmap_handle == 0 {
            continue;
        }
        if !pixel_states.has_quickdraw_pixel_state(record.pixmap_handle) {
            // Synthetic/native records created before the process registry
            // existed contribute their old booleans exactly once; after this
            // adoption the registry is canonical.
            pixel_states.set_quickdraw_pixel_state(
                record.pixmap_handle,
                ppc_gworld_pixel_state_from_mirror(record),
            );
        }
        let state = pixel_states.quickdraw_pixel_state(record.pixmap_handle);
        record.pixels_locked = state & PPC_PIXELS_LOCKED != 0;
        record.pixels_no_purge = state & PPC_PIXELS_PURGEABLE == 0;
    }
}

/// Return a process-owned state word, adopting a legacy native mirror only
/// for a record that predates the shared registry. This compatibility path is
/// intentionally one-way: every subsequent transition writes the registry.
pub(crate) fn ppc_ensure_gworld_pixel_state(
    gworlds: &[PpcGWorldRecord],
    pixel_states: &SharedProcessQuickDrawPixelStates,
    pixmap_handle: u32,
) -> Option<u32> {
    if pixmap_handle == 0 {
        return None;
    }
    if pixel_states.has_quickdraw_pixel_state(pixmap_handle) {
        return Some(pixel_states.quickdraw_pixel_state(pixmap_handle));
    }
    let state = gworlds
        .iter()
        .find(|record| record.pixmap_handle == pixmap_handle)
        .map(ppc_gworld_pixel_state_from_mirror)?;
    pixel_states.set_quickdraw_pixel_state(pixmap_handle, state);
    Some(state)
}

pub(crate) fn ppc_sync_gworld_pixel_state_mirror(
    gworlds: &mut [PpcGWorldRecord],
    pixmap_handle: u32,
    state: u32,
) {
    if let Some(record) = gworlds
        .iter_mut()
        .find(|record| record.pixmap_handle == pixmap_handle)
    {
        record.pixels_locked = state & PPC_PIXELS_LOCKED != 0;
        record.pixels_no_purge = state & PPC_PIXELS_PURGEABLE == 0;
    }
}

pub(crate) fn ppc_set_gworld_pixel_state(
    gworlds: &mut [PpcGWorldRecord],
    pixel_states: &SharedProcessQuickDrawPixelStates,
    pixmap_handle: u32,
    state: u32,
) {
    let Some(existing) = ppc_ensure_gworld_pixel_state(gworlds, pixel_states, pixmap_handle)
    else {
        return;
    };
    let masked = state & (PPC_KEEP_LOCAL | PPC_PIXELS_PURGEABLE | PPC_PIXELS_LOCKED);
    let preserved = existing & !(PPC_PIXELS_PURGEABLE | PPC_PIXELS_LOCKED);
    let next = preserved | masked;
    pixel_states.set_quickdraw_pixel_state(pixmap_handle, next);
    ppc_sync_gworld_pixel_state_mirror(gworlds, pixmap_handle, next);
}

pub(crate) fn ppc_lock_pixels(
    gworlds: &mut [PpcGWorldRecord],
    pixel_states: &SharedProcessQuickDrawPixelStates,
    pixmap_handle: u32,
) -> u32 {
    let Some(state) = ppc_ensure_gworld_pixel_state(gworlds, pixel_states, pixmap_handle) else {
        return 0;
    };
    let next = state | PPC_PIXELS_LOCKED;
    pixel_states.set_quickdraw_pixel_state(pixmap_handle, next);
    ppc_sync_gworld_pixel_state_mirror(gworlds, pixmap_handle, next);
    1
}

pub(crate) fn ppc_unlock_pixels(
    gworlds: &mut [PpcGWorldRecord],
    pixel_states: &SharedProcessQuickDrawPixelStates,
    pixmap_handle: u32,
) {
    let Some(state) = ppc_ensure_gworld_pixel_state(gworlds, pixel_states, pixmap_handle) else {
        return;
    };
    let next = state & !PPC_PIXELS_LOCKED;
    pixel_states.set_quickdraw_pixel_state(pixmap_handle, next);
    ppc_sync_gworld_pixel_state_mirror(gworlds, pixmap_handle, next);
}

pub(crate) fn ppc_allow_purge_pixels(
    gworlds: &mut [PpcGWorldRecord],
    pixel_states: &SharedProcessQuickDrawPixelStates,
    pixmap_handle: u32,
) {
    let Some(state) = ppc_ensure_gworld_pixel_state(gworlds, pixel_states, pixmap_handle) else {
        return;
    };
    let next = state | PPC_PIXELS_PURGEABLE;
    pixel_states.set_quickdraw_pixel_state(pixmap_handle, next);
    ppc_sync_gworld_pixel_state_mirror(gworlds, pixmap_handle, next);
}

pub(crate) fn ppc_no_purge_pixels(
    gworlds: &mut [PpcGWorldRecord],
    pixel_states: &SharedProcessQuickDrawPixelStates,
    pixmap_handle: u32,
) {
    let Some(state) = ppc_ensure_gworld_pixel_state(gworlds, pixel_states, pixmap_handle) else {
        return;
    };
    let next = state & !PPC_PIXELS_PURGEABLE;
    pixel_states.set_quickdraw_pixel_state(pixmap_handle, next);
    ppc_sync_gworld_pixel_state_mirror(gworlds, pixmap_handle, next);
}

pub(crate) fn ppc_read_pixmap_bits(memory: &mut PpcSectionMem, pixmap: u32) -> Option<PpcPixMapBits> {
    let base_addr = memory.read_u32_be(pixmap)?;
    let packed_row_bytes = memory.read_u16_be(pixmap + 4)?;
    let row_bytes = u32::from(packed_row_bytes & 0x3fff);
    let (top, left, bottom, right) = ppc_read_rect(memory, pixmap + 6)?;
    let width = u32::try_from(i32::from(right) - i32::from(left)).ok()?;
    let height = u32::try_from(i32::from(bottom) - i32::from(top)).ok()?;
    if base_addr == 0 || row_bytes == 0 || width == 0 || height == 0 {
        return None;
    }
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 3-12 and 4-46:
    // BitMap is the 14-byte prefix shared with PixMap, and a set high bit in
    // rowBytes distinguishes the extended PixMap record. Never read pmSize
    // from the bytes following a basic BitMap; inside a GrafPort those bytes
    // are portRect/visRgn fields and can look like a bogus depth such as -1.
    let depth = if packed_row_bytes & 0x8000 == 0 {
        1
    } else {
        memory
            .read_u16_be(pixmap + 32)
            .map(u32::from)
            .filter(|depth| *depth != 0)
            .unwrap_or_else(|| {
                if row_bytes >= width.saturating_mul(2) {
                    16
                } else {
                    0
                }
            })
    };
    if depth == 0 {
        return None;
    }
    if packed_row_bytes & 0x8000 != 0 && matches!(depth, 1 | 2 | 4 | 8 | 16 | 32) {
        let required_row_bytes = width.checked_mul(depth)?.checked_add(7)? / 8;
        if row_bytes < required_row_bytes {
            return None;
        }
    }
    Some(PpcPixMapBits {
        base_addr,
        row_bytes,
        top,
        left,
        bottom,
        right,
        width,
        height,
        depth,
    })
}

pub(crate) fn ppc_read_pixmap_handle_bits(
    memory: &mut PpcSectionMem,
    pixmap_handle: u32,
) -> Option<PpcPixMapBits> {
    let pixmap = memory.read_u32_be(pixmap_handle)?;
    ppc_read_pixmap_bits(memory, pixmap)
}

pub(crate) fn ppc_pixmap_bits_from_record(record: PpcGWorldRecord) -> Option<PpcPixMapBits> {
    let right = ppc_u32_to_i16_saturating(record.width);
    let bottom = ppc_u32_to_i16_saturating(record.height);
    Some(PpcPixMapBits {
        base_addr: record.base_addr,
        row_bytes: record.row_bytes,
        top: 0,
        left: 0,
        bottom,
        right,
        width: record.width,
        height: record.height,
        depth: record.depth,
    })
}

pub(crate) fn ppc_resolve_pixmap_bits(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    bits_ptr: u32,
) -> Option<PpcPixMapBits> {
    ppc_resolve_pixmap_bits_with_provenance(memory, gworlds, bits_ptr).map(|resolved| resolved.bits)
}

pub(crate) fn ppc_resolve_pixmap_bits_with_provenance(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    bits_ptr: u32,
) -> Option<PpcResolvedPixMapBits> {
    if bits_ptr == 0 {
        return None;
    }
    if let Some(record) = gworlds
        .iter()
        .find(|record| {
            record.pixmap == bits_ptr
                || record.pixmap_handle == bits_ptr
                || record.port == bits_ptr
                || record.port.checked_add(2) == Some(bits_ptr)
        })
        .copied()
    {
        if record.port == bits_ptr || record.port.checked_add(2) == Some(bits_ptr) {
            let surface = ppc_live_quickdraw_surface(memory, gworlds, record.port)?;
            let front = surface.front_buffer;
            let bits = PpcPixMapBits {
                base_addr: front.base_addr,
                row_bytes: front.row_bytes,
                top: surface.top,
                left: surface.left,
                bottom: ppc_i32_to_i16_saturating(
                    i32::from(surface.top).saturating_add(front.height as i32),
                ),
                right: ppc_i32_to_i16_saturating(
                    i32::from(surface.left).saturating_add(front.width as i32),
                ),
                width: front.width,
                height: front.height,
                depth: front.depth,
            };
            let exact_bottom = i64::from(surface.top) + i64::from(front.height);
            let exact_right = i64::from(surface.left) + i64::from(front.width);
            return Some(PpcResolvedPixMapBits {
                bits,
                guest_bounds: false,
                authoritative_bounds: i16::try_from(exact_bottom).is_ok()
                    && i16::try_from(exact_right).is_ok(),
            });
        }
        let bits = if record.pixmap_handle == bits_ptr {
            ppc_read_pixmap_handle_bits(memory, bits_ptr)
        } else {
            ppc_read_pixmap_bits(memory, record.pixmap)
        };
        return bits
            .map(|bits| PpcResolvedPixMapBits {
                bits,
                guest_bounds: true,
                authoritative_bounds: true,
            })
            .or_else(|| {
                ppc_pixmap_bits_from_record(record).map(|bits| PpcResolvedPixMapBits {
                    bits,
                    guest_bounds: false,
                    authoritative_bounds: i16::try_from(record.height).is_ok()
                        && i16::try_from(record.width).is_ok(),
                })
            });
    }
    if let Some(bits) = ppc_read_pixmap_bits(memory, bits_ptr) {
        return Some(PpcResolvedPixMapBits {
            bits,
            guest_bounds: true,
            authoritative_bounds: true,
        });
    }
    let pixmap_or_handle = memory.read_u32_be(bits_ptr)?;
    ppc_read_pixmap_bits(memory, pixmap_or_handle)
        .or_else(|| ppc_read_pixmap_handle_bits(memory, pixmap_or_handle))
        .map(|bits| PpcResolvedPixMapBits {
            bits,
            guest_bounds: true,
            authoritative_bounds: true,
        })
}

pub(crate) fn ppc_read_pixmap_raw_pixel(
    memory: &mut PpcSectionMem,
    bits: PpcPixMapBits,
    x: i32,
    y: i32,
) -> Option<u32> {
    if x < i32::from(bits.left)
        || x >= i32::from(bits.right)
        || y < i32::from(bits.top)
        || y >= i32::from(bits.bottom)
    {
        return None;
    }
    let x_offset = u32::try_from(x - i32::from(bits.left)).ok()?;
    let y_offset = u32::try_from(y - i32::from(bits.top)).ok()?;
    let row = bits
        .base_addr
        .checked_add(y_offset.checked_mul(bits.row_bytes)?)?;
    match bits.depth {
        1 | 2 | 4 => {
            let bit_offset = x_offset.checked_mul(bits.depth)?;
            let byte_offset = bit_offset / 8;
            if byte_offset >= bits.row_bytes {
                return None;
            }
            let shift = 8u32.checked_sub(bits.depth)?.checked_sub(bit_offset & 7)?;
            let mask = (1u8 << bits.depth) - 1;
            memory
                .read_u8(row.checked_add(byte_offset)?)
                .map(|byte| u32::from((byte >> shift) & mask))
        }
        8 => {
            if x_offset >= bits.row_bytes {
                return None;
            }
            memory.read_u8(row.checked_add(x_offset)?).map(u32::from)
        }
        16 => {
            let byte_offset = x_offset.checked_mul(2)?;
            if byte_offset.checked_add(2)? > bits.row_bytes {
                return None;
            }
            memory
                .read_u16_be(row.checked_add(byte_offset)?)
                .map(u32::from)
        }
        32 => {
            let byte_offset = x_offset.checked_mul(4)?;
            if byte_offset.checked_add(4)? > bits.row_bytes {
                return None;
            }
            memory.read_u32_be(row.checked_add(byte_offset)?)
        }
        _ => None,
    }
}

pub(crate) fn ppc_pixmap_pixel_rgb(
    bits: PpcPixMapBits,
    pixel: u32,
    clut: Option<&[[u16; 3]; 256]>,
) -> Option<[u16; 3]> {
    match bits.depth {
        1 => Some(if pixel == 0 {
            [u16::MAX; 3]
        } else {
            [0; 3]
        }),
        _depth @ (2 | 4 | 8) => clut?.get(pixel as usize).copied(),
        16 => Some(ppc_rgb555_to_rgb16(pixel as u16)),
        32 => Some([
            u16::from(((pixel >> 16) & 0xff) as u8) * 0x0101,
            u16::from(((pixel >> 8) & 0xff) as u8) * 0x0101,
            u16::from((pixel & 0xff) as u8) * 0x0101,
        ]),
        _ => None,
    }
}

pub(crate) fn ppc_rgb_to_pixmap_pixel(
    bits: PpcPixMapBits,
    rgb: [u16; 3],
    clut: Option<&[[u16; 3]; 256]>,
) -> Option<u32> {
    match bits.depth {
        1 => {
            let black_distance = u64::from(rgb[0]).pow(2)
                + u64::from(rgb[1]).pow(2)
                + u64::from(rgb[2]).pow(2);
            let white_distance = u64::from(u16::MAX - rgb[0]).pow(2)
                + u64::from(u16::MAX - rgb[1]).pow(2)
                + u64::from(u16::MAX - rgb[2]).pow(2);
            Some(u32::from(black_distance <= white_distance))
        }
        depth @ (2 | 4 | 8) => Some(u32::from(ppc_rgb_color_to_index_in_clut(
            PpcRgbColor {
                red: rgb[0],
                green: rgb[1],
                blue: rgb[2],
            },
            clut?,
            ppc_indexed_depth_entry_count(depth)?,
        ))),
        16 => Some(u32::from(ppc_rgb_color_to_rgb555(PpcRgbColor {
            red: rgb[0],
            green: rgb[1],
            blue: rgb[2],
        }))),
        32 => Some(
            (u32::from(rgb[0] >> 8) << 16)
                | (u32::from(rgb[1] >> 8) << 8)
                | u32::from(rgb[2] >> 8),
        ),
        _ => None,
    }
}

pub(crate) fn ppc_blend_deep_mask_rgb(source: [u16; 3], destination: [u16; 3], mask: [u16; 3]) -> [u16; 3] {
    std::array::from_fn(|component| {
        let source = u64::from(source[component]);
        let destination = u64::from(destination[component]);
        let mask = u64::from(mask[component]);
        ((source * (u64::from(u16::MAX) - mask) + destination * mask + 32_767)
            / u64::from(u16::MAX)) as u16
    })
}

pub(crate) fn ppc_deep_mask_boolean_pixel(source: u32, destination: u32, depth: u32, mode: u16) -> u32 {
    if depth == 1 {
        let source = u32::from(source != 0);
        let destination = u32::from(destination != 0);
        return match mode {
            0 => source,
            1 => source | destination,
            2 => source ^ destination,
            3 => (!source) & destination & 1,
            4 => 1 - source,
            5 => (1 - source) | destination,
            6 => (1 - source) ^ destination,
            7 => source & destination,
            _ => source,
        };
    }
    let value_mask = match depth {
        2 | 4 | 8 => (1u32 << depth) - 1,
        16 => u32::from(u16::MAX),
        32 => u32::MAX,
        _ => 0,
    };
    match mode {
        0 => source,
        1 => source | destination,
        2 => source ^ destination,
        3 => (!source) & destination & value_mask,
        4 => !source & value_mask,
        5 => (!source & value_mask) | destination,
        6 => (!source & value_mask) ^ destination,
        7 => source & destination,
        _ => source,
    }
}

pub(crate) fn ppc_write_pixmap_raw_pixel(
    memory: &mut PpcSectionMem,
    bits: PpcPixMapBits,
    x: i32,
    y: i32,
    pixel: u32,
) -> Option<()> {
    if x < i32::from(bits.left)
        || x >= i32::from(bits.right)
        || y < i32::from(bits.top)
        || y >= i32::from(bits.bottom)
    {
        return None;
    }
    let x_offset = u32::try_from(x - i32::from(bits.left)).ok()?;
    let y_offset = u32::try_from(y - i32::from(bits.top)).ok()?;
    let row = bits
        .base_addr
        .checked_add(y_offset.checked_mul(bits.row_bytes)?)?;
    match bits.depth {
        1 | 2 | 4 => {
            let bit_offset = x_offset.checked_mul(bits.depth)?;
            let byte_offset = bit_offset / 8;
            if byte_offset >= bits.row_bytes {
                return None;
            }
            let addr = row.checked_add(byte_offset)?;
            let shift = 8u32.checked_sub(bits.depth)?.checked_sub(bit_offset & 7)?;
            let value_mask = (1u8 << bits.depth) - 1;
            let field_mask = value_mask << shift;
            let byte = memory.read_u8(addr)?;
            memory.write_u8(
                addr,
                (byte & !field_mask) | (((pixel as u8) & value_mask) << shift),
            )?;
        }
        8 => {
            if x_offset >= bits.row_bytes {
                return None;
            }
            memory.write_u8(row.checked_add(x_offset)?, pixel as u8)?;
        }
        16 => {
            let byte_offset = x_offset.checked_mul(2)?;
            if byte_offset.checked_add(2)? > bits.row_bytes {
                return None;
            }
            memory.write_u16_be(row.checked_add(byte_offset)?, pixel as u16)?;
        }
        32 => {
            let byte_offset = x_offset.checked_mul(4)?;
            if byte_offset.checked_add(4)? > bits.row_bytes {
                return None;
            }
            memory.write_u32_be(row.checked_add(byte_offset)?, pixel)?;
        }
        _ => return None,
    }
    Some(())
}

pub(crate) fn ppc_resolve_pixmap_ctable_handle(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    bits_ptr: u32,
) -> Option<u32> {
    ppc_resolve_pixmap_ctable_handle_with_provenance(memory, gworlds, bits_ptr).handle
}

#[derive(Clone, Copy)]
pub(crate) struct PpcOptionalHandleResolution {
    pub(crate) handle: Option<u32>,
    pub(crate) known: bool,
}

pub(crate) fn ppc_resolve_pixmap_ctable_handle_with_provenance(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    bits_ptr: u32,
) -> PpcOptionalHandleResolution {
    let tracked_pixmap = gworlds.iter().find_map(|record| {
        if record.pixmap == bits_ptr {
            return Some(record.pixmap);
        }
        if record.pixmap_handle == bits_ptr {
            return memory.read_u32_be(bits_ptr);
        }
        if record.port == bits_ptr || record.port.checked_add(2) == Some(bits_ptr) {
            let port_bits = record.port.checked_add(2)?;
            return memory
                .read_u32_be(port_bits)
                .and_then(|pixmap_handle| memory.read_u32_be(pixmap_handle));
        }
        None
    });
    let pixmap = tracked_pixmap.or_else(|| {
        let direct_is_pixmap = bits_ptr
            .checked_add(4)
            .and_then(|row_bytes| memory.read_u16_be(row_bytes))
            .is_some_and(|row_bytes| row_bytes & 0x8000 != 0);
        if direct_is_pixmap {
            return Some(bits_ptr);
        }
        let indirect = memory.read_u32_be(bits_ptr)?;
        indirect
            .checked_add(4)
            .and_then(|row_bytes| memory.read_u16_be(row_bytes))
            .filter(|row_bytes| row_bytes & 0x8000 != 0)
            .map(|_| indirect)
    });
    let Some(pixmap) = pixmap else {
        return PpcOptionalHandleResolution {
            handle: None,
            known: false,
        };
    };
    let raw_handle = pixmap
        .checked_add(42)
        .and_then(|pm_table| memory.read_u32_be(pm_table));
    let Some(raw_handle) = raw_handle else {
        return PpcOptionalHandleResolution {
            handle: None,
            known: false,
        };
    };
    PpcOptionalHandleResolution {
        handle: (raw_handle != 0).then_some(raw_handle),
        known: true,
    }
}

pub(crate) fn ppc_color_tables_share_index_space(
    memory: &mut PpcSectionMem,
    src_ctable_handle: Option<u32>,
    dst_ctable_handle: Option<u32>,
) -> bool {
    let identity = |memory: &mut PpcSectionMem, handle: Option<u32>| {
        let table = handle
            .and_then(|handle| memory.read_u32_be(handle))
            .filter(|table| *table != 0)?;
        let seed = memory.read_u32_be(table)?;
        let flags = memory.read_u16_be(table.checked_add(4)?)?;
        if seed == 0 || flags & 0xc000 != 0 {
            return None;
        }
        let entry_count = usize::from(memory.read_u16_be(table.checked_add(6)?)?).checked_add(1)?;
        if entry_count > 256 {
            return None;
        }
        let mut colors = [None; 256];
        for slot in 0..entry_count {
            let entry = table.checked_add(8 + u32::try_from(slot).ok()?.checked_mul(8)?)?;
            let index = usize::from(memory.read_u16_be(entry)?);
            if index >= colors.len() || colors[index].is_some() {
                return None;
            }
            colors[index] = Some([
                memory.read_u16_be(entry.checked_add(2)?)?,
                memory.read_u16_be(entry.checked_add(4)?)?,
                memory.read_u16_be(entry.checked_add(6)?)?,
            ]);
        }
        Some((seed, colors))
    };
    let src_identity = identity(memory, src_ctable_handle);
    let dst_identity = identity(memory, dst_ctable_handle);
    matches!((src_identity, dst_identity), (Some(src), Some(dst)) if src == dst)
}

pub(crate) fn ppc_copy_bits_clut(
    memory: &mut PpcSectionMem,
    ctable_handle: u32,
    color_manager_clut: &[[u16; 3]; 256],
) -> [[u16; 3]; 256] {
    ppc_copy_bits_clut_with_provenance(memory, ctable_handle, color_manager_clut).0
}

pub(crate) fn ppc_copy_bits_clut_with_provenance(
    memory: &mut PpcSectionMem,
    ctable_handle: u32,
    color_manager_clut: &[[u16; 3]; 256],
) -> ([[u16; 3]; 256], bool) {
    if ctable_handle == 0 {
        return (*color_manager_clut, true);
    }
    // Inside Macintosh Volume V (1986), p. V-143: SetEntries updates the
    // current GDevice's ColorTable as well as its hardware lookup table.
    // Read even the main device table from guest memory so CopyBits sees
    // palette animation instead of the immutable startup palette.
    match ppc_read_ctable_clut(memory, ctable_handle, color_manager_clut) {
        Some(clut) => (clut, true),
        None => (*color_manager_clut, false),
    }
}

pub(crate) fn ppc_copy_bits_palette_index_map(
    memory: &mut PpcSectionMem,
    ctable_handle: u32,
    current_gworld: u32,
    current_gdevice: u32,
    toolbox_startup: &PpcToolboxStartupState,
    dst_clut: &[[u16; 3]; 256],
) -> Option<[u8; 256]> {
    let ctable = memory.read_u32_be(ctable_handle).filter(|ptr| *ptr != 0)?;
    let flags = memory.read_u16_be(ctable + 4)?;
    // Inside Macintosh Volume VI (1991), pp. 20-16--20-17: bit 14
    // links a source PixMap table to Palette Manager entries. The high bit
    // instead identifies a GDevice table, whose ColorSpec.value fields are
    // reserved and whose pixel values are implicit table positions.
    if flags & 0xc000 != 0x4000 {
        return None;
    }
    let entry_count = usize::from(memory.read_u16_be(ctable + 6)?)
        .saturating_add(1)
        .min(256);
    let assigned_palette = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
        .unwrap_or(0);
    let palette_handle = if assigned_palette != 0 {
        assigned_palette
    } else {
        toolbox_startup.application_palette
    };
    let palette = memory.read_u32_be(palette_handle).filter(|ptr| *ptr != 0);
    let palette_count = palette
        .and_then(|ptr| memory.read_u16_be(ptr))
        .map(usize::from)
        .unwrap_or(0);
    let mut map = std::array::from_fn(|index| index as u8);
    for (slot, mapped) in map.iter_mut().enumerate().take(entry_count) {
        let spec = ctable + 8 + slot as u32 * 8;
        let palette_entry = usize::from(memory.read_u16_be(spec)?);
        let fallback = [
            memory.read_u16_be(spec + 2)?,
            memory.read_u16_be(spec + 4)?,
            memory.read_u16_be(spec + 6)?,
        ];
        let Some(palette_info) = palette
            .filter(|_| palette_entry < palette_count)
            .and_then(|ptr| ptr.checked_add(16 + palette_entry as u32 * 16))
        else {
            // Inside Macintosh Volume VI (1991), pp. 20-16--20-17: the
            // ColorSpec RGB is the required fallback when no palette exists
            // or the linked entry lies beyond that palette.
            *mapped = pict::closest_clut_index(fallback[0], fallback[1], fallback[2], dst_clut);
            continue;
        };
        if let Some(index) = ppc_palette_allocated_index(
            toolbox_startup,
            palette_handle,
            current_gdevice,
            palette_entry,
        ) {
            *mapped = index;
            continue;
        }
        let usage = memory.read_u16_be(palette_info + 6)?;
        if usage & 0x0008 != 0 {
            // Explicit entries are defined as the palette entry modulo the
            // indexed device's table size.
            *mapped = palette_entry as u8;
        } else {
            let color = ppc_read_rgb_color(memory, palette_info)?;
            let rgb = [color.red, color.green, color.blue];
            *mapped = if usage & 0x0004 != 0 {
                // Animated colors use the device index reserved for the
                // palette entry. Prefer the aligned allocation used by this
                // HLE, then find the exact active-device entry; if the
                // reservation is gone, Palette Manager semantics degrade to
                // an ordinary courteous color match.
                dst_clut
                    .get(palette_entry)
                    .filter(|candidate| **candidate == rgb)
                    .map(|_| palette_entry)
                    .or_else(|| dst_clut.iter().position(|candidate| *candidate == rgb))
                    .map(|index| index as u8)
                    .unwrap_or_else(|| {
                        pict::closest_clut_index(color.red, color.green, color.blue, dst_clut)
                    })
            } else {
                pict::closest_clut_index(color.red, color.green, color.blue, dst_clut)
            };
        }
    }
    Some(map)
}

pub(crate) fn ppc_copy_bits_linked_palette_clut(
    memory: &mut PpcSectionMem,
    ctable_handle: u32,
    current_gworld: u32,
    _current_gdevice: u32,
    toolbox_startup: &PpcToolboxStartupState,
    fallback_clut: &[[u16; 3]; 256],
) -> Option<[[u16; 3]; 256]> {
    let ctable = memory.read_u32_be(ctable_handle).filter(|ptr| *ptr != 0)?;
    (memory.read_u16_be(ctable + 4)? & 0xc000 == 0x4000).then_some(())?;
    let entry_count = usize::from(memory.read_u16_be(ctable + 6)?)
        .saturating_add(1)
        .min(256);
    let assigned_palette = memory
        .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
        .unwrap_or(0);
    let palette_handle = if assigned_palette != 0 {
        assigned_palette
    } else {
        toolbox_startup.application_palette
    };
    let palette = memory.read_u32_be(palette_handle).filter(|ptr| *ptr != 0);
    let palette_count = palette
        .and_then(|ptr| memory.read_u16_be(ptr))
        .map(usize::from)
        .unwrap_or(0);
    let mut clut = *fallback_clut;
    for (slot, color) in clut.iter_mut().enumerate().take(entry_count) {
        let spec = ctable + 8 + slot as u32 * 8;
        let palette_entry = usize::from(memory.read_u16_be(spec)?);
        let color_ptr = palette
            .filter(|_| palette_entry < palette_count)
            .and_then(|ptr| ptr.checked_add(16 + palette_entry as u32 * 16))
            .unwrap_or(spec + 2);
        let rgb = ppc_read_rgb_color(memory, color_ptr)?;
        *color = [rgb.red, rgb.green, rgb.blue];
    }
    Some(clut)
}

pub(crate) fn ppc_record_copy_bits(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    commands: &mut Vec<u8>,
) -> bool {
    let src_bits_ptr = cpu.gpr[3];
    let Some(src_bits) = ppc_resolve_pixmap_bits(memory, gworlds, src_bits_ptr) else {
        return false;
    };
    if src_bits.depth != 8 || cpu.gpr[8] != 0 || cpu.gpr[7] as u16 & 0x3f != 0 {
        return false;
    }
    let Some(src_rect) = ppc_read_rect(memory, cpu.gpr[5]) else {
        return false;
    };
    let Some(dst_rect) = ppc_read_rect(memory, cpu.gpr[6]) else {
        return false;
    };
    let width = src_rect.3.saturating_sub(src_rect.1);
    let height = src_rect.2.saturating_sub(src_rect.0);
    if width <= 0 || height <= 0 || width > 0x3fff {
        return false;
    }
    let Some(ctable_handle) = ppc_resolve_pixmap_ctable_handle(memory, gworlds, src_bits_ptr) else {
        return false;
    };
    let Some(ctable) = memory.read_u32_be(ctable_handle).filter(|ptr| *ptr != 0) else {
        return false;
    };
    let Some(ct_seed) = memory.read_u32_be(ctable) else {
        return false;
    };
    let Some(ct_flags) = memory.read_u16_be(ctable + 4) else {
        return false;
    };
    let Some(ct_size) = memory.read_u16_be(ctable + 6) else {
        return false;
    };
    let entry_count = usize::from(ct_size).saturating_add(1).min(256);

    // Assemble the complete opcode before appending it so a malformed source
    // PixMap cannot leave a partial command in an otherwise valid picture.
    let mut recording = Vec::new();
    pict::recording_push_word(&mut recording, 0x0090); // BitsRect with indexed PixMap
    pict::recording_push_word(&mut recording, 0x8000 | width as u16);
    for value in [src_rect.0, src_rect.1, src_rect.2, src_rect.3] {
        pict::recording_push_word(&mut recording, value as u16);
    }
    pict::recording_push_word(&mut recording, 0); // pmVersion
    pict::recording_push_word(&mut recording, 1); // packType: unpacked
    recording.extend_from_slice(&0u32.to_be_bytes()); // packSize
    recording.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // hRes
    recording.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // vRes
    pict::recording_push_word(&mut recording, 0); // indexed pixelType
    pict::recording_push_word(&mut recording, 8); // pixelSize
    pict::recording_push_word(&mut recording, 1); // cmpCount
    pict::recording_push_word(&mut recording, 8); // cmpSize
    recording.extend_from_slice(&0u32.to_be_bytes()); // planeBytes
    recording.extend_from_slice(&0u32.to_be_bytes()); // pmTable is not serialized
    recording.extend_from_slice(&0u32.to_be_bytes()); // pmReserved
    recording.extend_from_slice(&ct_seed.to_be_bytes());
    pict::recording_push_word(&mut recording, ct_flags);
    pict::recording_push_word(&mut recording, entry_count.saturating_sub(1) as u16);
    for entry in 0..entry_count {
        let spec = ctable + 8 + entry as u32 * 8;
        let Some(bytes) = ppc_memory_read_bytes(memory, spec, 8) else {
            return false;
        };
        recording.extend_from_slice(&bytes);
    }
    for rect in [src_rect, dst_rect] {
        for value in [rect.0, rect.1, rect.2, rect.3] {
            pict::recording_push_word(&mut recording, value as u16);
        }
    }
    pict::recording_push_word(&mut recording, cpu.gpr[7] as u16);
    for y in src_rect.0..src_rect.2 {
        for x in src_rect.1..src_rect.3 {
            let Some(pixel) = ppc_read_pixmap_raw_pixel(memory, src_bits, i32::from(x), i32::from(y))
            else {
                return false;
            };
            recording.push(pixel as u8);
        }
    }
    if !recording.len().is_multiple_of(2) {
        recording.push(0);
    }
    commands.extend(recording);
    true
}

pub(crate) fn ppc_copy_bits(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    current_gdevice: u32,
    toolbox_startup: &PpcToolboxStartupState,
    color_manager_clut: &[[u16; 3]; 256],
    fore_color: PpcRgbColor,
    fore_index: Option<u8>,
    back_color: PpcRgbColor,
    back_index: Option<u8>,
    op_color: PpcRgbColor,
) -> bool {
    let src_bits_ptr = cpu.gpr[3];
    let dst_bits_ptr = cpu.gpr[4];
    let src_rect_ptr = cpu.gpr[5];
    let dst_rect_ptr = cpu.gpr[6];
    let transfer_mode = cpu.gpr[7] as u16;
    let mask_rgn = cpu.gpr[8];
    let mut reason = "ok";
    let mut trace_details = None;
    let copied = (|| {
        let mode = transfer_mode & 0x3f;
        if !matches!(mode, 0 | 32 | 36) {
            reason = "unsupported-mode";
            return None;
        }
        let Some(src_resolved) =
            ppc_resolve_pixmap_bits_with_provenance(memory, gworlds, src_bits_ptr)
        else {
            reason = "src-bits";
            return None;
        };
        let Some(dst_resolved) =
            ppc_resolve_pixmap_bits_with_provenance(memory, gworlds, dst_bits_ptr)
        else {
            reason = "dst-bits";
            return None;
        };
        let src_bits = src_resolved.bits;
        let dst_bits = dst_resolved.bits;
        let transparent = mode == 36;
        let supported_source = matches!(src_bits.depth, 1 | 2 | 4 | 8 | 16);
        let supported_destination = matches!(dst_bits.depth, 1 | 2 | 4 | 8 | 16);
        let compatible_depths = if transparent || mode == 0 {
            // Imaging With QuickDraw (1994), p. 3-117 and pp. 4-27--4-28:
            // CopyBits converts source colors into the destination device's
            // pixel format, and expands a BitMap through the foreground and
            // background colors. Source and destination depths need not match.
            supported_source && supported_destination
        } else {
            src_bits.depth == dst_bits.depth && supported_source
        };
        if !compatible_depths {
            reason = "depth";
            return None;
        }
        let src_ctable_resolution =
            ppc_resolve_pixmap_ctable_handle_with_provenance(memory, gworlds, src_bits_ptr);
        let dst_ctable_resolution =
            ppc_resolve_pixmap_ctable_handle_with_provenance(memory, gworlds, dst_bits_ptr);
        // Keep the legacy Option values below: unreadable lookup metadata has
        // always fallen back to the Color Manager table. The additional
        // provenance only prevents that fallback from selecting the new raw
        // indexed reducer as if a readable nil pmTable had been observed.
        let src_ctable = src_ctable_resolution.handle;
        let dst_ctable = dst_ctable_resolution.handle;
        let mut src_clut_resolution_known = false;
        let src_clut = ppc_indexed_depth_entry_count(src_bits.depth).map(|_| {
            let src_ctable = src_ctable.unwrap_or(0);
            if let Some(clut) = ppc_copy_bits_linked_palette_clut(
                memory,
                src_ctable,
                current_gworld,
                current_gdevice,
                toolbox_startup,
                color_manager_clut,
            ) {
                src_clut_resolution_known = src_ctable_resolution.known;
                clut
            } else {
                let (clut, known) =
                    ppc_copy_bits_clut_with_provenance(memory, src_ctable, color_manager_clut);
                src_clut_resolution_known = src_ctable_resolution.known && known;
                clut
            }
        });
        // Inside Macintosh Volume V (1986), p. V-69: CopyBits assumes that
        // an indexed destination uses the current GDevice's color table,
        // because that device's inverse table performs RGB-to-index mapping.
        // A destination PixMap's own ColorTable is therefore not the target
        // of the translation.
        // Imaging With QuickDraw (1994), pp. 4-27--4-28: CopyBits maps
        // colors into the current GDevice. The destination PixMap's table is
        // not the inverse-mapping target, even when it differs from that
        // device table.
        let dst_device_ctable_resolution =
            ppc_gdevice_ctable_handle_with_provenance(memory, current_gdevice);
        let mut dst_clut_resolution_known = false;
        let dst_clut = ppc_indexed_depth_entry_count(dst_bits.depth).map(|_| {
            if let Some(handle) = dst_device_ctable_resolution.handle {
                let (clut, known) =
                    ppc_copy_bits_clut_with_provenance(memory, handle, color_manager_clut);
                dst_clut_resolution_known = dst_device_ctable_resolution.known && known;
                clut
            } else {
                dst_clut_resolution_known = dst_device_ctable_resolution.known;
                *color_manager_clut
            }
        });
        // A shared nonzero ctSeed identifies a particular ColorTable instance.
        // When two same-depth ordinary image tables also describe the same
        // index-to-RGB space, their pixels already name the same colors and
        // CopyBits preserves the raw indexes even if the handles differ.
        // Imaging With QuickDraw (1994), pp. 4-56--4-57 and 4-97.
        let same_indexed_ctable_identity = src_bits.depth == dst_bits.depth
            && ppc_indexed_depth_entry_count(src_bits.depth).is_some()
            && src_ctable_resolution.known
            && dst_ctable_resolution.known
            && ppc_color_tables_share_index_space(memory, src_ctable, dst_ctable);
        let mut indexed_palette_identity_known = false;
        let palette_map = if src_bits.depth == 8 && dst_bits.depth == 8 {
            let src_ctable = src_ctable.unwrap_or(0);
            let src_clut = src_clut.as_ref().unwrap();
            let dst_clut = dst_clut.as_ref().unwrap();
            let source_uses_device_indices = memory
                .read_u32_be(src_ctable)
                .and_then(|table| memory.read_u16_be(table + 4))
                .is_some_and(|flags| flags & 0x8000 != 0);
            let linked_palette_map = ppc_copy_bits_palette_index_map(
                memory,
                src_ctable,
                current_gworld,
                current_gdevice,
                toolbox_startup,
                dst_clut,
            );
            if let Some(linked_palette_map) = linked_palette_map {
                Some(linked_palette_map)
            } else if same_indexed_ctable_identity {
                indexed_palette_identity_known =
                    src_clut_resolution_known && dst_clut_resolution_known;
                None
            } else {
                // Inside Macintosh Volume V (1986), p. V-135: the high
                // ctFlags bit distinguishes a GDevice table from a PixMap
                // image table. Its pixels are already device indexes, so an
                // indexed CopyBits preserves them.
                if source_uses_device_indices || src_clut == dst_clut {
                    indexed_palette_identity_known =
                        src_clut_resolution_known && dst_clut_resolution_known;
                    None
                } else {
                    Some(std::array::from_fn::<u8, 256, _>(|index| {
                        let [red, green, blue] = src_clut[index];
                        pict::closest_clut_index(red, green, blue, dst_clut)
                    }))
                }
            }
        } else if src_bits.depth == dst_bits.depth && matches!(src_bits.depth, 1 | 2 | 4) {
            let src_ctable = src_ctable.unwrap_or(0);
            let src_clut = src_clut.as_ref().unwrap();
            let dst_clut = dst_clut.as_ref().unwrap();
            let source_uses_device_indices = memory
                .read_u32_be(src_ctable)
                .and_then(|table| memory.read_u16_be(table + 4))
                .is_some_and(|flags| flags & 0x8000 != 0);
            (!same_indexed_ctable_identity && !source_uses_device_indices && src_clut != dst_clut)
                .then(|| {
                    let entry_count = ppc_indexed_depth_entry_count(src_bits.depth).unwrap_or(1);
                    std::array::from_fn::<u8, 256, _>(|index| {
                        let [red, green, blue] = src_clut[index];
                        // The inverse lookup is constrained to the active
                        // destination table entries representable at this
                        // packed depth. Looking up in an 8-bit table and then
                        // masking the result does not preserve the matched
                        // color. Imaging With QuickDraw (1994), pp. 4-27--4-28
                        // and 4-81--4-82.
                        ppc_rgb_color_to_index_in_clut(
                            PpcRgbColor { red, green, blue },
                            dst_clut,
                            entry_count,
                        )
                    })
                })
        } else {
            None
        };
        let (src_top, src_left, src_bottom, src_right) = if src_rect_ptr != 0 {
            ppc_read_rect(memory, src_rect_ptr).unwrap_or((
                src_bits.top,
                src_bits.left,
                src_bits.bottom,
                src_bits.right,
            ))
        } else {
            (src_bits.top, src_bits.left, src_bits.bottom, src_bits.right)
        };
        let (dst_top, dst_left, dst_bottom, dst_right) = if dst_rect_ptr != 0 {
            ppc_read_rect(memory, dst_rect_ptr).unwrap_or((
                dst_bits.top,
                dst_bits.left,
                dst_bits.bottom,
                dst_bits.right,
            ))
        } else {
            (dst_bits.top, dst_bits.left, dst_bits.bottom, dst_bits.right)
        };
        trace_details = Some((
            src_bits,
            dst_bits,
            (src_top, src_left, src_bottom, src_right),
            (dst_top, dst_left, dst_bottom, dst_right),
        ));

        let src_width = i64::from(src_right) - i64::from(src_left);
        let src_height = i64::from(src_bottom) - i64::from(src_top);
        let dst_width = i64::from(dst_right) - i64::from(dst_left);
        let dst_height = i64::from(dst_bottom) - i64::from(dst_top);
        if src_width <= 0 || src_height <= 0 || dst_width <= 0 || dst_height <= 0 {
            reason = "empty-rect";
            return None;
        }

        let copy_left = i32::from(dst_left).max(i32::from(dst_bits.left));
        let copy_top = i32::from(dst_top).max(i32::from(dst_bits.top));
        let copy_right = i32::from(dst_right).min(i32::from(dst_bits.right));
        let copy_bottom = i32::from(dst_bottom).min(i32::from(dst_bits.bottom));
        if copy_left >= copy_right || copy_top >= copy_bottom {
            reason = "clipped-empty";
            return None;
        }

        let mask_storage = if mask_rgn == 0 {
            None
        } else {
            let Some(storage) = ppc_region_storage(memory, mask_rgn) else {
                reason = "mask-rgn";
                return None;
            };
            Some(storage)
        };

        // Inside Macintosh: Imaging With QuickDraw (1994), pp. 3-112–3-116
        // and 4-27: CopyBits copies bitmap or PixMap pixels between graphics
        // ports and GWorlds, including indexed-color PixMaps. The import
        // resolves records and masks; the shared operation owns pure
        // byte-aligned or packed srcCopy format, mode, and geometry eligibility.
        if mask_storage.is_none() {
            use crate::copy_bits::{
                BytePixmap, Indexed8ScalingSelection, RowCopy, RowCopyOutcome,
            };

            let indexed8_scaling = Indexed8ScalingSelection::from_adapter_facts(
                mask_rgn == 0,
                src_resolved.guest_bounds,
                dst_resolved.authoritative_bounds,
                indexed_palette_identity_known,
                transfer_mode,
            );

            let outcome = RowCopy {
                mode,
                source: BytePixmap {
                    base: src_bits.base_addr,
                    row_bytes: src_bits.row_bytes,
                    depth: src_bits.depth,
                    bounds: [src_bits.top, src_bits.left, src_bits.bottom, src_bits.right]
                        .map(i32::from),
                },
                destination: BytePixmap {
                    base: dst_bits.base_addr,
                    row_bytes: dst_bits.row_bytes,
                    depth: dst_bits.depth,
                    bounds: [dst_bits.top, dst_bits.left, dst_bits.bottom, dst_bits.right]
                        .map(i32::from),
                },
                source_rect: [src_top, src_left, src_bottom, src_right].map(i32::from),
                destination_rect: [dst_top, dst_left, dst_bottom, dst_right].map(i32::from),
                clip: [copy_top, copy_left, copy_bottom, copy_right],
                palette: palette_map.as_ref(),
            }
            .execute_with_indexed8_scaling(memory, indexed8_scaling);
            match outcome {
                RowCopyOutcome::Completed => return Some(()),
                RowCopyOutcome::NoOp => {
                    reason = "row-copy-no-op";
                    return Some(());
                }
                RowCopyOutcome::Declined => {}
                RowCopyOutcome::ReadOrGeometryFailure => {
                    reason = "row-copy-read-or-geometry";
                    return None;
                }
                RowCopyOutcome::WriteFailure { .. } => {
                    reason = "row-copy-write";
                    return None;
                }
            }
        }

        let transparent_back_pixel = match src_bits.depth {
            1 => 0,
            8 if back_index.is_some() => u32::from(back_index.unwrap()),
            2 | 4 | 8 => {
                let clut = src_clut.as_ref()?;
                u32::from(ppc_rgb_color_to_index_in_clut(
                    back_color,
                    clut,
                    ppc_indexed_depth_entry_count(src_bits.depth)?,
                ))
            }
            16 => u32::from(ppc_rgb_color_to_rgb555(back_color)),
            _ => return None,
        };
        let bitmap_fore_pixel = match dst_bits.depth {
            1 | 2 | 4 | 8 if fore_index.is_some() => {
                let pixel_mask = ((1u16 << dst_bits.depth) - 1) as u8;
                u32::from(fore_index.unwrap() & pixel_mask)
            }
            1 | 2 | 4 | 8 => {
                let clut = dst_clut.as_ref()?;
                u32::from(ppc_rgb_color_to_index_in_clut(
                    fore_color,
                    clut,
                    ppc_indexed_depth_entry_count(dst_bits.depth)?,
                ))
            }
            16 => u32::from(ppc_rgb_color_to_rgb555(fore_color)),
            _ => return None,
        };
        let bitmap_back_pixel = match dst_bits.depth {
            1 | 2 | 4 | 8 if back_index.is_some() => {
                let pixel_mask = ((1u16 << dst_bits.depth) - 1) as u8;
                u32::from(back_index.unwrap() & pixel_mask)
            }
            1 | 2 | 4 | 8 => {
                let clut = dst_clut.as_ref()?;
                u32::from(ppc_rgb_color_to_index_in_clut(
                    back_color,
                    clut,
                    ppc_indexed_depth_entry_count(dst_bits.depth)?,
                ))
            }
            16 => u32::from(ppc_rgb_color_to_rgb555(back_color)),
            _ => return None,
        };
        let mut writes = Vec::new();
        let mut details = crate::memory::SavedPixels::<()>::default();
        for dst_y in copy_top..copy_bottom {
            let rel_y = i64::from(dst_y) - i64::from(dst_top);
            let src_y = i64::from(src_top) + (rel_y * src_height) / dst_height;
            let Ok(src_y) = i32::try_from(src_y) else {
                continue;
            };
            for dst_x in copy_left..copy_right {
                let rel_x = i64::from(dst_x) - i64::from(dst_left);
                let src_x = i64::from(src_left) + (rel_x * src_width) / dst_width;
                let Ok(src_x) = i32::try_from(src_x) else {
                    continue;
                };
                let Some(src_pixel) = ppc_read_pixmap_raw_pixel(memory, src_bits, src_x, src_y)
                else {
                    continue;
                };
                if mask_storage.as_ref().is_some_and(|storage| {
                    !ppc_point_in_region_storage(storage, dst_y as i16, dst_x as i16)
                }) {
                    continue;
                }
                if transparent && src_pixel == transparent_back_pixel {
                    continue;
                }
                let pixel = if mode == 32 {
                    let dst_pixel = ppc_read_pixmap_raw_pixel(memory, dst_bits, dst_x, dst_y)?;
                    let (source_rgb, destination_rgb) = match src_bits.depth {
                        1 | 2 | 4 | 8 => (
                            src_clut.as_ref()?[src_pixel as usize],
                            dst_clut.as_ref()?[dst_pixel as usize],
                        ),
                        16 => (
                            ppc_rgb555_to_rgb16(src_pixel as u16),
                            ppc_rgb555_to_rgb16(dst_pixel as u16),
                        ),
                        _ => return None,
                    };
                    let weights = [op_color.red, op_color.green, op_color.blue];
                    let blended: [u16; 3] = std::array::from_fn(|channel| {
                        let weight = u64::from(weights[channel]);
                        ((u64::from(source_rgb[channel]) * weight
                            + u64::from(destination_rgb[channel]) * (65_536 - weight))
                            >> 16) as u16
                    });
                    match dst_bits.depth {
                        depth @ (1 | 2 | 4 | 8) => u32::from(ppc_rgb_color_to_index_in_clut(
                            PpcRgbColor {
                                red: blended[0],
                                green: blended[1],
                                blue: blended[2],
                            },
                            dst_clut.as_ref()?,
                            ppc_indexed_depth_entry_count(depth)?,
                        )),
                        16 => u32::from(ppc_rgb_color_to_rgb555(PpcRgbColor {
                            red: blended[0],
                            green: blended[1],
                            blue: blended[2],
                        })),
                        _ => return None,
                    }
                } else if transparent && src_bits.depth == 1 {
                    bitmap_fore_pixel
                } else if src_bits.depth == 1 {
                    if src_pixel == 0 {
                        bitmap_back_pixel
                    } else {
                        bitmap_fore_pixel
                    }
                } else if ppc_indexed_depth_entry_count(src_bits.depth).is_some()
                    && src_bits.depth == dst_bits.depth
                {
                    u32::from(
                        palette_map
                            .as_ref()
                            .map(|palette_map| palette_map[src_pixel as usize])
                            .unwrap_or(src_pixel as u8),
                    )
                } else if ppc_indexed_depth_entry_count(src_bits.depth).is_some()
                    && dst_bits.depth == 16
                {
                    let [red, green, blue] = src_clut.as_ref()?[src_pixel as usize];
                    u32::from(ppc_rgb_color_to_rgb555(PpcRgbColor { red, green, blue }))
                } else if src_bits.depth == 16
                    && ppc_indexed_depth_entry_count(dst_bits.depth).is_some()
                {
                    let [red, green, blue] = ppc_rgb555_to_rgb16(src_pixel as u16);
                    u32::from(ppc_rgb_color_to_index_in_clut(
                        PpcRgbColor { red, green, blue },
                        dst_clut.as_ref()?,
                        ppc_indexed_depth_entry_count(dst_bits.depth)?,
                    ))
                } else if ppc_indexed_depth_entry_count(src_bits.depth).is_some()
                    && ppc_indexed_depth_entry_count(dst_bits.depth).is_some()
                {
                    let [red, green, blue] = src_clut.as_ref()?[src_pixel as usize];
                    u32::from(ppc_rgb_color_to_index_in_clut(
                        PpcRgbColor { red, green, blue },
                        dst_clut.as_ref()?,
                        ppc_indexed_depth_entry_count(dst_bits.depth)?,
                    ))
                } else {
                    src_pixel
                };
                if matches!(mode, 0 | 36)
                    && src_bits.depth == dst_bits.depth
                    && matches!(src_bits.depth, 8 | 16)
                    && src_pixel == pixel
                {
                    let lanes = src_bits.depth / 8;
                    let address = src_bits.base_addr
                        + (src_y - i32::from(src_bits.top)) as u32 * src_bits.row_bytes
                        + (src_x - i32::from(src_bits.left)) as u32 * lanes;
                    memory.presentation().capture_detail(
                        &mut details,
                        writes.len() * lanes as usize,
                        address,
                        lanes as usize,
                    );
                }
                writes.push((dst_x, dst_y, pixel));
            }
        }
        if writes.is_empty() {
            if transparent {
                return Some(());
            }
            reason = "no-pixels";
            return None;
        }
        for (index, (x, y, pixel)) in writes.into_iter().enumerate() {
            ppc_write_pixmap_raw_pixel(memory, dst_bits, x, y, pixel)?;
            if matches!(dst_bits.depth, 8 | 16) {
                let lanes = dst_bits.depth / 8;
                let address = dst_bits.base_addr
                    + (y - i32::from(dst_bits.top)) as u32 * dst_bits.row_bytes
                    + (x - i32::from(dst_bits.left)) as u32 * lanes;
                memory.presentation().restore_detail(
                    &details,
                    index * lanes as usize,
                    address,
                    lanes as usize,
                );
            }
        }
        Some(())
    })();
    let copied = copied.is_some();
    if ppc_hle_trace_enabled() {
        if let Some((src, dst, src_rect, dst_rect)) = trace_details {
            eprintln!(
                "[PPC-TRACE] CopyBits src=${:08X}[base=${:08X} rb={} depth={} bounds=({},{},{},{})] dst=${:08X}[base=${:08X} rb={} depth={} bounds=({},{},{},{})] srcRect=${:08X}({},{},{},{}) dstRect=${:08X}({},{},{},{}) mode=${:04X} maskRgn=${:08X} copied={} reason={}",
                src_bits_ptr,
                src.base_addr,
                src.row_bytes,
                src.depth,
                src.top,
                src.left,
                src.bottom,
                src.right,
                dst_bits_ptr,
                dst.base_addr,
                dst.row_bytes,
                dst.depth,
                dst.top,
                dst.left,
                dst.bottom,
                dst.right,
                src_rect_ptr,
                src_rect.0,
                src_rect.1,
                src_rect.2,
                src_rect.3,
                dst_rect_ptr,
                dst_rect.0,
                dst_rect.1,
                dst_rect.2,
                dst_rect.3,
                transfer_mode,
                mask_rgn,
                copied,
                reason,
            );
        } else {
            eprintln!(
                "[PPC-TRACE] CopyBits src=${:08X} dst=${:08X} srcRect=${:08X} dstRect=${:08X} mode=${:04X} maskRgn=${:08X} copied={} reason={}",
                src_bits_ptr,
                dst_bits_ptr,
                src_rect_ptr,
                dst_rect_ptr,
                transfer_mode,
                mask_rgn,
                copied,
                reason
            );
        }
    }
    copied
}

pub(crate) fn ppc_copy_mask(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    color_manager_clut: &[[u16; 3]; 256],
) -> bool {
    let src_bits_ptr = cpu.gpr[3];
    let mask_bits_ptr = cpu.gpr[4];
    let dst_bits_ptr = cpu.gpr[5];
    let src_rect_ptr = cpu.gpr[6];
    let mask_rect_ptr = cpu.gpr[7];
    let dst_rect_ptr = cpu.gpr[8];
    // Imaging With QuickDraw (1994), pp. 3-119--3-122: a pixel-map mask
    // averages source and destination weighted by the mask's color instead
    // of gating the transfer. That is CopyDeepMask's srcCopy transfer with
    // no mask region.
    if ppc_resolve_pixmap_bits(memory, gworlds, mask_bits_ptr).is_some_and(|mask| mask.depth != 1)
    {
        return ppc_copy_deep_mask_with(cpu, memory, gworlds, color_manager_clut, 0, 0, "CopyMask");
    }
    let mut reason = "ok";
    let mut trace_details = None;
    let copied = (|| {
        let Some(src_bits) = ppc_resolve_pixmap_bits(memory, gworlds, src_bits_ptr) else {
            reason = "src-bits";
            return None;
        };
        let Some(mask_bits) = ppc_resolve_pixmap_bits(memory, gworlds, mask_bits_ptr) else {
            reason = "mask-bits";
            return None;
        };
        let Some(dst_bits) = ppc_resolve_pixmap_bits(memory, gworlds, dst_bits_ptr) else {
            reason = "dst-bits";
            return None;
        };
        if src_bits.depth != dst_bits.depth || !matches!(src_bits.depth, 1 | 2 | 4 | 8 | 16) {
            reason = "depth";
            return None;
        }
        let src_rect = ppc_read_rect(memory, src_rect_ptr)?;
        let mask_rect = ppc_read_rect(memory, mask_rect_ptr)?;
        let dst_rect = ppc_read_rect(memory, dst_rect_ptr)?;
        trace_details = Some((src_bits, mask_bits, dst_bits, src_rect, mask_rect, dst_rect));

        let src_width = i64::from(src_rect.3) - i64::from(src_rect.1);
        let src_height = i64::from(src_rect.2) - i64::from(src_rect.0);
        let mask_width = i64::from(mask_rect.3) - i64::from(mask_rect.1);
        let mask_height = i64::from(mask_rect.2) - i64::from(mask_rect.0);
        let dst_width = i64::from(dst_rect.3) - i64::from(dst_rect.1);
        let dst_height = i64::from(dst_rect.2) - i64::from(dst_rect.0);
        if src_width <= 0
            || src_height <= 0
            || mask_width <= 0
            || mask_height <= 0
            || dst_width <= 0
            || dst_height <= 0
        {
            reason = "empty-rect";
            return None;
        }

        let palette_map = if let Some(entry_count) = ppc_indexed_depth_entry_count(src_bits.depth) {
            let src_ctable = ppc_resolve_pixmap_ctable_handle(memory, gworlds, src_bits_ptr);
            let dst_ctable = ppc_resolve_pixmap_ctable_handle(memory, gworlds, dst_bits_ptr);
            match (src_ctable, dst_ctable) {
                (Some(src_ctable), Some(dst_ctable)) if src_ctable != dst_ctable => {
                    let src_clut = ppc_copy_bits_clut(memory, src_ctable, color_manager_clut);
                    let dst_clut = ppc_copy_bits_clut(memory, dst_ctable, color_manager_clut);
                    (src_clut[..entry_count] != dst_clut[..entry_count]).then(|| {
                        std::array::from_fn::<u8, 256, _>(|index| {
                            if index >= entry_count {
                                return 0;
                            }
                            let [red, green, blue] = src_clut[index];
                            ppc_rgb_color_to_index_in_clut(
                                PpcRgbColor { red, green, blue },
                                &dst_clut,
                                entry_count,
                            )
                        })
                    })
                }
                _ => None,
            }
        } else {
            None
        };

        let copy_left = i32::from(dst_rect.1).max(i32::from(dst_bits.left));
        let copy_top = i32::from(dst_rect.0).max(i32::from(dst_bits.top));
        let copy_right = i32::from(dst_rect.3).min(i32::from(dst_bits.right));
        let copy_bottom = i32::from(dst_rect.2).min(i32::from(dst_bits.bottom));
        if copy_left >= copy_right || copy_top >= copy_bottom {
            reason = "clipped-empty";
            return None;
        }

        // Imaging With QuickDraw (1994), pp. 3-119--3-120: CopyMask scales
        // srcRect and maskRect to dstRect, transferring a source pixel only
        // where the corresponding one-bit mask pixel is set (black).
        let mut writes = Vec::new();
        for dst_y in copy_top..copy_bottom {
            let rel_y = i64::from(dst_y) - i64::from(dst_rect.0);
            let src_y = i64::from(src_rect.0) + rel_y * src_height / dst_height;
            let mask_y = i64::from(mask_rect.0) + rel_y * mask_height / dst_height;
            let (Ok(src_y), Ok(mask_y)) = (i32::try_from(src_y), i32::try_from(mask_y)) else {
                continue;
            };
            for dst_x in copy_left..copy_right {
                let rel_x = i64::from(dst_x) - i64::from(dst_rect.1);
                let src_x = i64::from(src_rect.1) + rel_x * src_width / dst_width;
                let mask_x = i64::from(mask_rect.1) + rel_x * mask_width / dst_width;
                let (Ok(src_x), Ok(mask_x)) = (i32::try_from(src_x), i32::try_from(mask_x)) else {
                    continue;
                };
                if ppc_read_pixmap_raw_pixel(memory, mask_bits, mask_x, mask_y) != Some(1) {
                    continue;
                }
                let Some(mut pixel) = ppc_read_pixmap_raw_pixel(memory, src_bits, src_x, src_y)
                else {
                    continue;
                };
                if let Some(palette_map) = palette_map.as_ref() {
                    pixel = u32::from(palette_map[pixel as usize]);
                }
                writes.push((dst_x, dst_y, pixel));
            }
        }
        if writes.is_empty() {
            reason = "no-pixels";
            return None;
        }
        for (x, y, pixel) in writes {
            ppc_write_pixmap_raw_pixel(memory, dst_bits, x, y, pixel)?;
        }
        Some(())
    })();
    let copied = copied.is_some();
    if ppc_hle_trace_enabled() {
        if let Some((src, mask, dst, src_rect, mask_rect, dst_rect)) = trace_details {
            eprintln!(
                "[PPC-TRACE] CopyMask src=${src_bits_ptr:08X}[rb={} depth={} bounds=({},{},{},{})] mask=${mask_bits_ptr:08X}[rb={} depth={} bounds=({},{},{},{})] dst=${dst_bits_ptr:08X}[rb={} depth={} bounds=({},{},{},{})] srcRect={src_rect:?} maskRect={mask_rect:?} dstRect={dst_rect:?} copied={copied} reason={reason}",
                src.row_bytes,
                src.depth,
                src.top,
                src.left,
                src.bottom,
                src.right,
                mask.row_bytes,
                mask.depth,
                mask.top,
                mask.left,
                mask.bottom,
                mask.right,
                dst.row_bytes,
                dst.depth,
                dst.top,
                dst.left,
                dst.bottom,
                dst.right,
            );
        } else {
            eprintln!(
                "[PPC-TRACE] CopyMask src=${src_bits_ptr:08X} mask=${mask_bits_ptr:08X} dst=${dst_bits_ptr:08X} copied={copied} reason={reason}"
            );
        }
    }
    copied
}

pub(crate) fn ppc_copy_deep_mask(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    color_manager_clut: &[[u16; 3]; 256],
) -> bool {
    let mode = cpu.gpr[9] as u16 & 0x3f;
    let mask_rgn = cpu.gpr[10];
    ppc_copy_deep_mask_with(cpu, memory, gworlds, color_manager_clut, mode, mask_rgn, "CopyDeepMask")
}

/// CopyDeepMask's transfer, reading the shared bitmap and rectangle arguments
/// from r3-r8; `mode` and `mask_rgn` come from the caller so CopyMask can
/// reuse it for deep masks. `name` labels the trace line.
pub(crate) fn ppc_copy_deep_mask_with(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    color_manager_clut: &[[u16; 3]; 256],
    mode: u16,
    mask_rgn: u32,
    name: &str,
) -> bool {
    let src_bits_ptr = cpu.gpr[3];
    let mask_bits_ptr = cpu.gpr[4];
    let dst_bits_ptr = cpu.gpr[5];
    let src_rect_ptr = cpu.gpr[6];
    let mask_rect_ptr = cpu.gpr[7];
    let dst_rect_ptr = cpu.gpr[8];
    let mut reason = "ok";
    let mut trace_details = None;
    let copied = (|| {
        let Some(src_bits) = ppc_resolve_pixmap_bits(memory, gworlds, src_bits_ptr) else {
            reason = "src-bits";
            return None;
        };
        let Some(mask_bits) = ppc_resolve_pixmap_bits(memory, gworlds, mask_bits_ptr) else {
            reason = "mask-bits";
            return None;
        };
        let Some(dst_bits) = ppc_resolve_pixmap_bits(memory, gworlds, dst_bits_ptr) else {
            reason = "dst-bits";
            return None;
        };
        if !matches!(src_bits.depth, 1 | 2 | 4 | 8 | 16 | 32)
            || !matches!(dst_bits.depth, 1 | 2 | 4 | 8 | 16 | 32)
            || !matches!(mask_bits.depth, 1 | 2 | 4 | 8 | 16 | 32)
        {
            reason = "depth";
            return None;
        }
        let src_rect = ppc_read_rect(memory, src_rect_ptr)?;
        let mask_rect = ppc_read_rect(memory, mask_rect_ptr)?;
        let dst_rect = ppc_read_rect(memory, dst_rect_ptr)?;
        trace_details = Some((src_bits, mask_bits, dst_bits, src_rect, mask_rect, dst_rect));

        let src_width = i64::from(src_rect.3) - i64::from(src_rect.1);
        let src_height = i64::from(src_rect.2) - i64::from(src_rect.0);
        let mask_width = i64::from(mask_rect.3) - i64::from(mask_rect.1);
        let mask_height = i64::from(mask_rect.2) - i64::from(mask_rect.0);
        let dst_width = i64::from(dst_rect.3) - i64::from(dst_rect.1);
        let dst_height = i64::from(dst_rect.2) - i64::from(dst_rect.0);
        if src_width <= 0
            || src_height <= 0
            || mask_width <= 0
            || mask_height <= 0
            || dst_width <= 0
            || dst_height <= 0
        {
            reason = "empty-rect";
            return None;
        }

        let mask_storage = if mask_rgn == 0 {
            None
        } else {
            let Some(storage) = ppc_region_storage(memory, mask_rgn) else {
                reason = "mask-rgn";
                return None;
            };
            Some(storage)
        };
        let src_clut = ppc_indexed_depth_entry_count(src_bits.depth).map(|_| {
            ppc_resolve_pixmap_ctable_handle(memory, gworlds, src_bits_ptr)
                .map(|handle| ppc_copy_bits_clut(memory, handle, color_manager_clut))
                .unwrap_or(*color_manager_clut)
        });
        let dst_clut = ppc_indexed_depth_entry_count(dst_bits.depth).map(|_| {
            ppc_resolve_pixmap_ctable_handle(memory, gworlds, dst_bits_ptr)
                .map(|handle| ppc_copy_bits_clut(memory, handle, color_manager_clut))
                .unwrap_or(*color_manager_clut)
        });
        let mask_clut = ppc_indexed_depth_entry_count(mask_bits.depth).map(|_| {
            ppc_resolve_pixmap_ctable_handle(memory, gworlds, mask_bits_ptr)
                .map(|handle| ppc_copy_bits_clut(memory, handle, color_manager_clut))
                .unwrap_or(*color_manager_clut)
        });

        let copy_left = i32::from(dst_rect.1).max(i32::from(dst_bits.left));
        let copy_top = i32::from(dst_rect.0).max(i32::from(dst_bits.top));
        let copy_right = i32::from(dst_rect.3).min(i32::from(dst_bits.right));
        let copy_bottom = i32::from(dst_rect.2).min(i32::from(dst_bits.bottom));
        if copy_left >= copy_right || copy_top >= copy_bottom {
            reason = "clipped-empty";
            return None;
        }

        // Imaging With QuickDraw (1994), pp. 3-120--3-122: CopyDeepMask
        // scales the source and deep mask into dstRect, then clips the
        // transfer to maskRgn. For a deep mask, black selects the source,
        // white preserves the destination, and intermediate RGB components
        // weight the source and destination independently.
        let mut writes = Vec::new();
        for dst_y in copy_top..copy_bottom {
            let rel_y = i64::from(dst_y) - i64::from(dst_rect.0);
            let src_y = i64::from(src_rect.0) + rel_y * src_height / dst_height;
            let mask_y = i64::from(mask_rect.0) + rel_y * mask_height / dst_height;
            let (Ok(src_y), Ok(mask_y)) = (i32::try_from(src_y), i32::try_from(mask_y)) else {
                continue;
            };
            for dst_x in copy_left..copy_right {
                if mask_storage.as_ref().is_some_and(|storage| {
                    !ppc_point_in_region_storage(storage, dst_y as i16, dst_x as i16)
                }) {
                    continue;
                }
                let rel_x = i64::from(dst_x) - i64::from(dst_rect.1);
                let src_x = i64::from(src_rect.1) + rel_x * src_width / dst_width;
                let mask_x = i64::from(mask_rect.1) + rel_x * mask_width / dst_width;
                let (Ok(src_x), Ok(mask_x)) = (i32::try_from(src_x), i32::try_from(mask_x)) else {
                    continue;
                };
                let Some(src_pixel) = ppc_read_pixmap_raw_pixel(memory, src_bits, src_x, src_y)
                else {
                    continue;
                };
                let Some(source_rgb) =
                    ppc_pixmap_pixel_rgb(src_bits, src_pixel, src_clut.as_ref())
                else {
                    continue;
                };
                let Some(destination_pixel) =
                    ppc_read_pixmap_raw_pixel(memory, dst_bits, dst_x, dst_y)
                else {
                    continue;
                };
                let Some(destination_rgb) = ppc_pixmap_pixel_rgb(
                    dst_bits,
                    destination_pixel,
                    dst_clut.as_ref(),
                ) else {
                    continue;
                };

                let Some(mask_pixel) = ppc_read_pixmap_raw_pixel(memory, mask_bits, mask_x, mask_y)
                else {
                    continue;
                };
                if mask_bits.depth == 1 {
                    // A one-bit mask uses the historical BitMap convention:
                    // a set (black) bit copies the source and a clear (white)
                    // bit leaves the destination unchanged.
                    if mask_pixel == 0 {
                        continue;
                    }
                    let source_for_destination = ppc_rgb_to_pixmap_pixel(
                        dst_bits,
                        source_rgb,
                        dst_clut.as_ref(),
                    )?;
                    let pixel = if mode <= 7 {
                        ppc_deep_mask_boolean_pixel(
                            source_for_destination,
                            destination_pixel,
                            dst_bits.depth,
                            mode,
                        )
                    } else {
                        source_for_destination
                    };
                    writes.push((dst_x, dst_y, pixel));
                    continue;
                }

                let Some(mask_rgb) =
                    ppc_pixmap_pixel_rgb(mask_bits, mask_pixel, mask_clut.as_ref())
                else {
                    continue;
                };
                let source_for_blend = if mode <= 7 && mode != 0 {
                    let source_for_destination = ppc_rgb_to_pixmap_pixel(
                        dst_bits,
                        source_rgb,
                        dst_clut.as_ref(),
                    )?;
                    let mode_pixel = ppc_deep_mask_boolean_pixel(
                        source_for_destination,
                        destination_pixel,
                        dst_bits.depth,
                        mode,
                    );
                    ppc_pixmap_pixel_rgb(dst_bits, mode_pixel, dst_clut.as_ref())?
                } else {
                    source_rgb
                };
                let blended_rgb = ppc_blend_deep_mask_rgb(
                    source_for_blend,
                    destination_rgb,
                    mask_rgb,
                );
                let pixel = ppc_rgb_to_pixmap_pixel(dst_bits, blended_rgb, dst_clut.as_ref())?;
                writes.push((dst_x, dst_y, pixel));
            }
        }
        if writes.is_empty() {
            reason = "no-pixels";
            return None;
        }
        for (x, y, pixel) in writes {
            ppc_write_pixmap_raw_pixel(memory, dst_bits, x, y, pixel)?;
        }
        Some(())
    })();
    let copied = copied.is_some();
    if ppc_hle_trace_enabled() {
        if let Some((src, mask, dst, src_rect, mask_rect, dst_rect)) = trace_details {
            eprintln!(
                "[PPC-TRACE] {name} src=${src_bits_ptr:08X}[rb={} depth={} bounds=({},{},{},{})] mask=${mask_bits_ptr:08X}[rb={} depth={} bounds=({},{},{},{})] dst=${dst_bits_ptr:08X}[rb={} depth={} bounds=({},{},{},{})] srcRect={src_rect:?} maskRect={mask_rect:?} dstRect={dst_rect:?} mode={mode} maskRgn=${mask_rgn:08X} copied={copied} reason={reason}",
                src.row_bytes,
                src.depth,
                src.top,
                src.left,
                src.bottom,
                src.right,
                mask.row_bytes,
                mask.depth,
                mask.top,
                mask.left,
                mask.bottom,
                mask.right,
                dst.row_bytes,
                dst.depth,
                dst.top,
                dst.left,
                dst.bottom,
                dst.right,
            );
        } else {
            eprintln!(
                "[PPC-TRACE] {name} src=${src_bits_ptr:08X} mask=${mask_bits_ptr:08X} dst=${dst_bits_ptr:08X} mode={mode} maskRgn=${mask_rgn:08X} copied={copied} reason={reason}"
            );
        }
    }
    copied
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_write_pixmap(
    memory: &mut PpcSectionMem,
    pixmap: u32,
    base_addr: u32,
    row_bytes: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
    depth: u32,
) -> Option<()> {
    memory.write_u32_be(pixmap, base_addr)?;
    memory.write_u16_be(pixmap + 4, (row_bytes as u16) | 0x8000)?;
    ppc_write_rect(memory, pixmap + 6, top, left, bottom, right)?;
    memory.write_u16_be(pixmap + 14, 0)?;
    memory.write_u16_be(pixmap + 16, 0)?;
    memory.write_u32_be(pixmap + 18, 0)?;
    memory.write_u32_be(pixmap + 22, 0x0048_0000)?;
    memory.write_u32_be(pixmap + 26, 0x0048_0000)?;
    let (pixel_type, component_count, component_size) = match depth {
        16 => (16, 3, 5),
        32 => (16, 3, 8),
        _ => (0, 1, depth),
    };
    memory.write_u16_be(pixmap + 30, pixel_type)?;
    memory.write_u16_be(pixmap + 32, depth as u16)?;
    memory.write_u16_be(pixmap + 34, component_count)?;
    memory.write_u16_be(pixmap + 36, component_size as u16)?;
    memory.write_u32_be(pixmap + 38, 0)?;
    memory.write_u32_be(pixmap + 42, 0)?;
    memory.write_u32_be(pixmap + 46, 0)?;
    Some(())
}
