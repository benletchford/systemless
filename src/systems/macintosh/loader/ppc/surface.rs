//! PowerPC QuickDraw surface pixel rendering, color matching, and CLUT conversion.

use super::*;

pub(crate) fn ppc_live_gworld_ctable_handle(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    gworld: u32,
) -> Option<u32> {
    let record = gworlds.iter().find(|record| record.port == gworld)?;
    let live_pixmap = gworld
        .checked_add(6)
        .and_then(|row_bytes| memory.read_u16_be(row_bytes))
        .filter(|row_bytes| row_bytes & 0x8000 != 0)
        .and_then(|_| gworld.checked_add(2))
        .and_then(|port_bits| memory.read_u32_be(port_bits))
        .and_then(|pixmap_handle| memory.read_u32_be(pixmap_handle));
    live_pixmap
        .or_else(|| (record.pixmap != 0).then_some(record.pixmap))
        .and_then(|pixmap| pixmap.checked_add(42))
        .and_then(|pm_table| memory.read_u32_be(pm_table))
        .filter(|ctable_handle| *ctable_handle != 0)
}

pub(crate) fn ppc_live_gworld_clut(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    gworld: u32,
    _screen_clut: &[[u16; 3]; 256],
    color_manager_clut: &[[u16; 3]; 256],
) -> [[u16; 3]; 256] {
    let Some(ctable_handle) = ppc_live_gworld_ctable_handle(memory, gworlds, gworld) else {
        return *color_manager_clut;
    };
    // Imaging With QuickDraw (1994), "Color Tables": RGB-to-pixel mapping is
    // performed through the destination PixMap's logical ColorTable. A display
    // driver cscSetEntries request may temporarily fade the physical CLUT
    // without replacing that table, so never substitute the host's hardware
    // palette here—even for a PixMap whose baseAddr is the main screen.
    ppc_read_ctable_clut(memory, ctable_handle, color_manager_clut).unwrap_or(*color_manager_clut)
}

pub(crate) fn ppc_front_buffer_8bpp_pixel_addr(
    front_buffer: PpcFrontBuffer,
    (x, y): (i32, i32),
) -> Option<u32> {
    if x < 0 || y < 0 || front_buffer.depth != 8 {
        return None;
    }
    let (x, y) = (x as u32, y as u32);
    if x >= front_buffer.width || y >= front_buffer.height || x >= front_buffer.row_bytes {
        return None;
    }
    front_buffer
        .base_addr
        .checked_add(y.checked_mul(front_buffer.row_bytes)?)?
        .checked_add(x)
}

#[cfg(test)]
pub(crate) fn ppc_rgb_color_to_8bpp_index(color: PpcRgbColor) -> u8 {
    ppc_rgb_color_to_8bpp_index_in_clut(color, &TrapDispatcher::standard_mac_8bpp_clut())
}

#[cfg(test)]
pub(crate) fn ppc_rgb_color_to_8bpp_index_in_clut(
    color: PpcRgbColor,
    clut: &[[u16; 3]; 256],
) -> u8 {
    ppc_rgb555_to_clut_index(ppc_rgb_color_to_rgb555(color), clut)
}

pub(crate) fn ppc_indexed_depth_entry_count(depth: u32) -> Option<usize> {
    matches!(depth, 1 | 2 | 4 | 8).then(|| 1usize << depth)
}

pub(crate) fn ppc_rgb_color_to_index_in_clut(
    color: PpcRgbColor,
    clut: &[[u16; 3]; 256],
    entry_count: usize,
) -> u8 {
    ppc_rgb_color_to_valid_index_in_clut(color, clut, &[true; 256], entry_count).unwrap_or(0)
}

pub(crate) fn ppc_rgb_color_to_valid_index_in_clut(
    color: PpcRgbColor,
    clut: &[[u16; 3]; 256],
    valid: &[bool; 256],
    entry_count: usize,
) -> Option<u8> {
    let target = ppc_rgb_color_to_rgb555(color);
    let target_r = i64::from((target >> 10) & 0x1f);
    let target_g = i64::from((target >> 5) & 0x1f);
    let target_b = i64::from(target & 0x1f);
    clut.iter()
        .take(entry_count.min(clut.len()))
        .enumerate()
        .filter(|(index, _)| valid[*index])
        .min_by_key(|(_, [red, green, blue])| {
            let red = i64::from(*red >> 11);
            let green = i64::from(*green >> 11);
            let blue = i64::from(*blue >> 11);
            (red - target_r).pow(2) + (green - target_g).pow(2) + (blue - target_b).pow(2)
        })
        .map(|(index, _)| index as u8)
}

pub(crate) fn ppc_quickdraw_surface_color_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    color: PpcRgbColor,
) -> Option<u16> {
    match surface.front_buffer.depth {
        depth @ (1 | 2 | 4 | 8) => {
            // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-81--4-82:
            // Color2Index maps an RGBColor through the destination GDevice's
            // inverse table. For an offscreen PixMap, its own ColorTable is the
            // destination palette, so RGB QuickDraw primitives must not assume
            // the canonical system palette.
            // The same volume, pp. 4-14--4-16 and 4-46--4-47, defines indexed
            // pixel values at 1, 2, 4, and 8 bits. Match only the entries that
            // the destination pixel can represent; matching all 256 and then
            // truncating would select a different color and corrupt its index.
            let fallback = TrapDispatcher::standard_mac_indexed_clut(depth as u16)
                .map(|(clut, _)| clut)
                .unwrap_or_else(TrapDispatcher::standard_mac_8bpp_clut);
            let clut = surface
                .ctable_handle
                .and_then(|handle| ppc_read_ctable_clut(memory, handle, &fallback))
                .unwrap_or(fallback);
            Some(u16::from(ppc_rgb_color_to_index_in_clut(
                color,
                &clut,
                ppc_indexed_depth_entry_count(depth)?,
            )))
        }
        16 => Some(ppc_rgb_color_to_rgb555(color)),
        _ => None,
    }
}

pub(crate) fn ppc_quickdraw_surface_fore_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> Option<u16> {
    if let Some(entry_count) = ppc_indexed_depth_entry_count(surface.front_buffer.depth) {
        if let Some(index) = explicit_index {
            return Some(u16::from(index) & (entry_count as u16 - 1));
        }
    }
    ppc_quickdraw_surface_color_pixel(memory, surface, color)
}

pub(crate) fn ppc_quickdraw_write_pixel(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    point: (i32, i32),
    color: PpcRgbColor,
) -> bool {
    match front_buffer.depth {
        depth @ (1 | 2 | 4 | 8) => {
            let fallback = TrapDispatcher::standard_mac_indexed_clut(depth as u16)
                .map(|(clut, _)| clut)
                .unwrap_or_else(TrapDispatcher::standard_mac_8bpp_clut);
            let clut = if front_buffer.base_addr == PPC_MAIN_SCREEN_BASE {
                ppc_read_ctable_clut(memory, PPC_MAIN_CTABLE_HANDLE, &fallback).unwrap_or(fallback)
            } else {
                fallback
            };
            let pixel = ppc_rgb_color_to_index_in_clut(
                color,
                &clut,
                ppc_indexed_depth_entry_count(depth).unwrap_or(1),
            );
            ppc_quickdraw_write_raw_pixel(memory, front_buffer, point, u16::from(pixel))
        }
        16 => {
            ppc_q3_write_software_pixel(memory, front_buffer, point, ppc_rgb_color_to_rgb555(color))
        }
        _ => false,
    }
}

pub(crate) fn ppc_quickdraw_read_pixel(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    point: (i32, i32),
) -> Option<u16> {
    match front_buffer.depth {
        depth @ (1 | 2 | 4) => {
            let (x, y) = point;
            let x = u32::try_from(x).ok()?;
            let y = u32::try_from(y).ok()?;
            if x >= front_buffer.width || y >= front_buffer.height {
                return None;
            }
            let pixels_per_byte = 8 / depth;
            let byte_offset = x / pixels_per_byte;
            if byte_offset >= front_buffer.row_bytes {
                return None;
            }
            let byte = memory.read_u8(
                front_buffer
                    .base_addr
                    .checked_add(y.checked_mul(front_buffer.row_bytes)?)?
                    .checked_add(byte_offset)?,
            )?;
            let shift = 8 - depth - (x % pixels_per_byte) * depth;
            let mask = (1u8 << depth) - 1;
            Some(u16::from((byte >> shift) & mask))
        }
        8 => memory
            .read_u8(ppc_front_buffer_8bpp_pixel_addr(front_buffer, point)?)
            .map(u16::from),
        16 => ppc_q3_read_software_pixel(memory, front_buffer, point),
        _ => None,
    }
}

pub(crate) fn ppc_quickdraw_write_raw_pixel(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    point: (i32, i32),
    value: u16,
) -> bool {
    match front_buffer.depth {
        depth @ (1 | 2 | 4) => {
            let (x, y) = point;
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                return false;
            };
            if x >= front_buffer.width || y >= front_buffer.height {
                return false;
            }
            let Some(row_offset) = y.checked_mul(front_buffer.row_bytes) else {
                return false;
            };
            let pixels_per_byte = 8 / depth;
            let byte_offset = x / pixels_per_byte;
            if byte_offset >= front_buffer.row_bytes {
                return false;
            }
            let Some(addr) = front_buffer
                .base_addr
                .checked_add(row_offset)
                .and_then(|row| row.checked_add(byte_offset))
            else {
                return false;
            };
            let Some(byte) = memory.read_u8(addr) else {
                return false;
            };
            let shift = 8 - depth - (x % pixels_per_byte) * depth;
            let value_mask = (1u8 << depth) - 1;
            let pixel_mask = value_mask << shift;
            let packed = ((value as u8) & value_mask) << shift;
            memory
                .write_u8(addr, (byte & !pixel_mask) | packed)
                .is_some()
        }
        8 => {
            let Some(addr) = ppc_front_buffer_8bpp_pixel_addr(front_buffer, point) else {
                return false;
            };
            memory.write_u8(addr, value as u8).is_some()
        }
        16 => ppc_q3_write_software_pixel(memory, front_buffer, point, value),
        _ => false,
    }
}

pub(crate) fn ppc_zero_guest_bytes(memory: &mut PpcSectionMem, addr: u32, len: u32) -> bool {
    if addr == 0 || !ppc_memory_can_write_bytes(memory, addr, len) {
        return false;
    }
    for offset in 0..len {
        if memory.write_u8(addr + offset, 0).is_none() {
            return false;
        }
    }
    true
}
