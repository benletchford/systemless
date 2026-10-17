//! PowerPC adapter for the shared QuickDraw raster core
//! (`crate::quickdraw::raster`): resolves the port's pixel image, colours
//! and clip once per call, then lets the core draw. Whatever the core
//! declines keeps the PowerPC path it had before.

use super::graphics::*;
use super::regions::*;
use super::*;
use crate::quickdraw::raster::paint::{
    paint_rect, supports, ClipRows, Depth, DrawOutcome, Ink, PatternKind, Surface, TransferMode,
    EMPTY_RECT,
};
use crate::quickdraw::raster::region_rows::intersect_rows;
use crate::trap::types::Rect;

/// FillRect(r, pat): fills the rectangle with `pattern` in patCopy mode,
/// ignoring the port's pen pattern, pen mode and background pattern
/// (Imaging With QuickDraw (1994), pp. 3-60--3-61). Above 1 bit per pixel,
/// patCopy gives black pattern bits the foreground colour and white bits
/// the background colour (p. 4-32); patterns are aligned to the port's
/// coordinate system (p. 2-32). `fore` and `back` are an RGB colour and the
/// port's explicit index, if it has one.
///
/// The shared core draws 8- and 16-bit pixel images. Other depths, and
/// clip regions whose storage cannot be parsed, keep the earlier PowerPC
/// fill: solid foreground, whatever the pattern.
pub(crate) fn ppc_fill_rect(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    rect: (i16, i16, i16, i16),
    pattern: [u8; 8],
    fore: (PpcRgbColor, Option<u8>),
    back: (PpcRgbColor, Option<u8>),
) -> bool {
    match ppc_fill_rect_with_core(memory, gworlds, current_gworld, rect, pattern, fore, back) {
        Some(DrawOutcome::Drawn { .. }) => true,
        Some(DrawOutcome::Declined) => {
            ppc_paint_rect_bounds(memory, gworlds, current_gworld, rect, fore.0, fore.1)
        }
        None => false,
    }
}

/// `None` when nothing can be drawn at all (no surface, or a colour that
/// cannot be resolved); the earlier path draws nothing in those cases too.
fn ppc_fill_rect_with_core(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    (top, left, bottom, right): (i16, i16, i16, i16),
    pattern: [u8; 8],
    fore: (PpcRgbColor, Option<u8>),
    back: (PpcRgbColor, Option<u8>),
) -> Option<DrawOutcome> {
    let surface = ppc_live_quickdraw_surface(memory, gworlds, current_gworld)?;
    let front_buffer = surface.front_buffer;
    let Some(depth) = Depth::from_bits(front_buffer.depth)
        .filter(|depth| supports(*depth, TransferMode::PatCopy, PatternKind::Mono))
    else {
        return Some(DrawOutcome::Declined);
    };
    let raster_surface = Surface {
        base: front_buffer.base_addr,
        row_bytes: front_buffer.row_bytes,
        depth,
        bounds: ppc_surface_port_bounds(surface),
    };
    let Some(area) = raster_surface.clip(Rect {
        top,
        left,
        bottom,
        right,
    }) else {
        return Some(DrawOutcome::Drawn {
            touched: EMPTY_RECT,
        });
    };
    // Resolve only the colours the pattern uses: a solid pattern resolves
    // one, as the solid fill always has.
    let uses_fore = pattern.iter().any(|row| *row != 0);
    let uses_back = pattern.iter().any(|row| *row != 0xff);
    let (fg, bg) = ppc_quickdraw_surface_pattern_pixels(
        memory,
        surface,
        uses_fore.then_some(fore),
        uses_back.then_some(back),
    )?;
    let Some(clip) = ppc_port_clip_rows(memory, current_gworld, area.top, area.bottom) else {
        return Some(DrawOutcome::Declined);
    };
    let ink = Ink {
        fg: u32::from(fg),
        bg: u32::from(bg),
        pattern,
        mode: TransferMode::PatCopy,
        pat_origin: (0, 0),
    };
    if let Some(wrote) = ppc_gpu_pattern_rect(memory, surface, area, &clip, &ink) {
        return Some(DrawOutcome::Drawn {
            touched: if wrote { area } else { EMPTY_RECT },
        });
    }
    let mut scratch = Vec::new();
    Some(paint_rect(
        memory,
        &raster_surface,
        &clip,
        &ink,
        area,
        &mut scratch,
    ))
}

/// `None` leaves the original software path in charge, including GPU errors.
pub(super) fn ppc_gpu_pattern_rect(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    area: Rect,
    clip: &ClipRows,
    ink: &Ink,
) -> Option<bool> {
    use crate::quickdraw::gpu::{try_native_rectangle, QuickDrawGpuCommand, RasterTarget};
    if memory.quickdraw_gpu_renderer().is_none() || ink.mode != TransferMode::PatCopy {
        return None;
    }
    let rows = clip.clipped_rows(area)?;
    if rows.iter().all(Vec::is_empty) {
        return Some(false);
    }
    // The shader's pattern origin is (0,0); rotate the tile to retain the
    // caller's coordinate convention, including negative port origins.
    let pattern = std::array::from_fn(|y| {
        ink.pattern[(y as i32 - i32::from(ink.pat_origin.1)).rem_euclid(8) as usize]
            .rotate_left((-i32::from(ink.pat_origin.0)).rem_euclid(8) as u32)
    });
    let command = QuickDrawGpuCommand {
        mask: None,
        area: [
            i32::from(area.left),
            i32::from(area.top),
            i32::from(area.right),
            i32::from(area.bottom),
        ],
        rect: [
            i32::from(area.left),
            i32::from(area.top),
            i32::from(area.right),
            i32::from(area.bottom),
        ],
        border: None,
        pattern,
        transfer: [[0, ink.bg], [0, ink.fg]],
        polygon_edges: None,
        clip_rows: Some(
            rows.into_iter()
                .map(|row| row.into_iter().map(i32::from).collect())
                .collect(),
        ),
    };
    try_native_rectangle(
        memory,
        RasterTarget {
            base: surface.front_buffer.base_addr,
            row_bytes: surface.front_buffer.row_bytes,
            depth: u16::try_from(surface.front_buffer.depth).ok()?,
            origin: [i32::from(surface.left), i32::from(surface.top)],
        },
        command,
    )
    .then_some(true)
}

/// The pixel image's extent in port coordinates. Its far edges are capped
/// at `i16::MAX`; no rectangle reaches past that.
fn ppc_surface_port_bounds(surface: PpcQuickDrawSurface) -> Rect {
    let far =
        |near: i16, len: u32| (i64::from(near) + i64::from(len)).min(i64::from(i16::MAX)) as i16;
    Rect {
        top: surface.top,
        left: surface.left,
        bottom: far(surface.top, surface.front_buffer.height),
        right: far(surface.left, surface.front_buffer.width),
    }
}

/// The port's visRgn ∩ clipRgn for scanlines `top..bottom`. A region
/// handle that cannot be read leaves drawing unclipped by it, as everywhere
/// on the PowerPC path. When every readable region is a rectangle, the clip
/// is their common rectangle and nothing is allocated; otherwise each
/// region is parsed once for the band. `None` when a region's storage is
/// malformed.
pub(super) fn ppc_port_clip_rows(
    memory: &mut PpcSectionMem,
    port: u32,
    top: i16,
    bottom: i16,
) -> Option<ClipRows> {
    let storages = [
        PPC_CGRAF_PORT_VIS_RGN_OFFSET,
        PPC_CGRAF_PORT_CLIP_RGN_OFFSET,
    ]
    .map(|offset| {
        memory
            .read_u32_be(port.wrapping_add(offset))
            .and_then(|rgn| ppc_region_storage(memory, rgn))
    });
    let regions = storages.iter().flatten();
    if regions.clone().all(|storage| storage.len() == 10) {
        let mut bbox = ClipRows::UNBOUNDED;
        for storage in regions {
            let Some((rgn_top, rgn_left, rgn_bottom, rgn_right)) = ppc_region_storage_bbox(storage)
            else {
                // An empty region clips every pixel.
                return Some(ClipRows::rectangular(EMPTY_RECT));
            };
            bbox = Rect {
                top: bbox.top.max(rgn_top),
                left: bbox.left.max(rgn_left),
                bottom: bbox.bottom.min(rgn_bottom),
                right: bbox.right.min(rgn_right),
            };
        }
        return Some(ClipRows::rectangular(bbox));
    }
    let mut rows: Option<Vec<Vec<i16>>> = None;
    for storage in regions {
        let band = ppc_region_rows_for_band(storage, top, bottom)?;
        rows = Some(match rows {
            None => band,
            Some(rows) => rows
                .iter()
                .zip(&band)
                .map(|(row, region_row)| intersect_rows(row, region_row))
                .collect(),
        });
    }
    Some(ClipRows::from_rows(top, rows.unwrap_or_default()))
}
