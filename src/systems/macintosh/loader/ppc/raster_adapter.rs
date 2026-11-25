//! PowerPC adapter for the shared QuickDraw raster core
//! (`crate::quickdraw::raster`): resolves the port's pixel image, colours
//! and clip once per call, then lets the core draw. Whatever the core
//! declines keeps the PowerPC path it had before.

use super::graphics::*;
use super::regions::*;
use super::*;
use crate::quickdraw::raster::memory::RasterMemory;
use crate::quickdraw::raster::paint::{
    paint_rect, paint_solid_1bpp_rect, supports, ClipRows, Depth, DrawOutcome, Ink, PatternKind,
    Surface, TransferMode, EMPTY_RECT,
};
use crate::quickdraw::raster::region_rows::intersect_rows;
use crate::quickdraw::raster::transfer::BitRowOp;
use crate::trap::types::Rect;

/// FillRect(r, pat): fills the rectangle with `pattern` in patCopy mode,
/// ignoring the port's pen pattern, pen mode and background pattern
/// (Imaging With QuickDraw (1994), pp. 3-60--3-61). Above 1 bit per pixel,
/// patCopy gives black pattern bits the foreground colour and white bits
/// the background colour (p. 4-32); patterns are aligned to the port's
/// coordinate system (p. 2-32). `fore` and `back` are an RGB colour and the
/// port's explicit index, if it has one.
///
/// The shared core draws 8- and 16-bit pixel images, and solid patterns on
/// 1-bit images while the pen is visible. Other depths and patterns, and clip
/// regions whose storage cannot be parsed, keep the earlier PowerPC fill:
/// solid foreground, whatever the pattern.
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
            // The core writes whole-byte pixels only. Packed pixels still take
            // the pattern: the background colour where its bits are clear
            // (Imaging With QuickDraw (1994), p. 3-67), so FillRect with the
            // white pattern clears a 1-bit image instead of blackening it.
            let packed = ppc_live_quickdraw_surface(memory, gworlds, current_gworld)
                .is_some_and(|surface| matches!(surface.front_buffer.depth, 1 | 2 | 4));
            let (top, left, bottom, right) = rect;
            if packed && top < bottom && left < right {
                let rows = vec![vec![left, right]; (i32::from(bottom) - i32::from(top)) as usize];
                ppc_fill_mono_rows(memory, gworlds, current_gworld, top, rows, pattern, fore, back)
            } else {
                ppc_paint_rect_bounds(memory, gworlds, current_gworld, rect, fore.0, fore.1)
            }
        }
        None => false,
    }
}

fn ppc_raster_surface(surface: PpcQuickDrawSurface, depth: Depth) -> Surface {
    Surface {
        base: surface.front_buffer.base_addr,
        row_bytes: surface.front_buffer.row_bytes,
        depth,
        bounds: ppc_surface_port_bounds(surface),
    }
}

/// How the shared core's row stores fail on PowerPC memory.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PpcRunWrites {
    /// One `write_bytes` per run; a run with any unwritable byte is lost
    /// whole, as the PowerPC FillRect and window-background row writers
    /// always have at `8` and `16` bits. `1`-bit surfaces, which those
    /// callers drew pixel by pixel, always take `PerPixelOnFailure`.
    Whole,
    /// A run that cannot be stored whole is stored pixel by pixel, so only
    /// the unreadable or unwritable pixels are lost, as in the per-pixel
    /// loops of PaintRect, EraseRect, InvertRect and the 1-bit fill.
    PerPixelOnFailure,
}

/// PowerPC memory that retries a failed run one pixel (`pixel_len` bytes) at a
/// time; see [`PpcRunWrites::PerPixelOnFailure`].
pub(super) struct PpcPixelFallback<'a> {
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) pixel_len: usize,
}

impl PpcPixelFallback<'_> {
    fn write_pixels(&mut self, addr: u32, bytes: &[u8], readable: Option<&[bool]>) -> bool {
        let mut wrote = false;
        for (index, pixel) in bytes.chunks(self.pixel_len).enumerate() {
            if readable.is_some_and(|readable| !readable[index]) {
                continue;
            }
            let pixel_addr = addr.wrapping_add((index * self.pixel_len) as u32);
            wrote |= self.memory.write_bytes(pixel_addr, pixel).is_some();
        }
        wrote
    }
}

impl RasterMemory for PpcPixelFallback<'_> {
    fn write_row(&mut self, addr: u32, bytes: &[u8]) -> Option<()> {
        if self.memory.write_bytes(addr, bytes).is_some() {
            return Some(());
        }
        self.write_pixels(addr, bytes, None).then_some(())
    }

    fn modify_row(&mut self, addr: u32, buf: &mut [u8], f: impl FnOnce(&mut [u8])) -> Option<()> {
        if self.memory.read_bytes_into(addr, buf).is_some() {
            f(buf);
            return self.write_row(addr, buf);
        }
        let pixel_len = self.pixel_len;
        let readable: Vec<bool> = buf
            .chunks_mut(pixel_len)
            .enumerate()
            .map(|(index, pixel)| {
                let pixel_addr = addr.wrapping_add((index * pixel_len) as u32);
                self.memory.read_bytes_into(pixel_addr, pixel).is_some()
            })
            .collect();
        f(buf);
        self.write_pixels(addr, buf, Some(&readable)).then_some(())
    }
}

/// Draws a clipped monochrome-pattern rectangle on `surface` via the native
/// GPU path or the shared `quickdraw::raster` core (`8`/`16`-bpp copy/RMW and
/// `1`-bpp solid fills). Returns `None` when the depth/pattern combination or
/// clip storage is declined so the caller can run its own fallback.
pub(super) fn ppc_draw_mono_rect(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    port: u32,
    area: Rect,
    respect_clip: bool,
    ink: &Ink,
    writes: PpcRunWrites,
) -> Option<bool> {
    let depth = Depth::from_bits(surface.front_buffer.depth)?;
    let clip = ppc_port_clip_rows_with_clip(memory, port, area.top, area.bottom, respect_clip)?;
    if let Some(wrote) = ppc_gpu_pattern_rect(memory, surface, area, &clip, ink) {
        return Some(wrote);
    }
    let raster_surface = ppc_raster_surface(surface, depth);
    match if depth == Depth::D1 {
        PpcRunWrites::PerPixelOnFailure
    } else {
        writes
    } {
        PpcRunWrites::Whole => ppc_core_mono_rect(memory, &raster_surface, &clip, area, ink),
        PpcRunWrites::PerPixelOnFailure => {
            let pixel_len = if depth == Depth::D16 { 2 } else { 1 };
            let mut memory = PpcPixelFallback { memory, pixel_len };
            ppc_core_mono_rect(&mut memory, &raster_surface, &clip, area, ink)
        }
    }
}

fn ppc_core_mono_rect<M: RasterMemory>(
    memory: &mut M,
    raster_surface: &Surface,
    clip: &ClipRows,
    area: Rect,
    ink: &Ink,
) -> Option<bool> {
    let depth = raster_surface.depth;
    if supports(depth, ink.mode, PatternKind::Mono) {
        let mut scratch = Vec::new();
        return match paint_rect(memory, raster_surface, clip, ink, area, &mut scratch) {
            DrawOutcome::Drawn { wrote, .. } => Some(wrote),
            DrawOutcome::Declined => None,
        };
    }
    if depth == Depth::D1
        && ink.mode.is_copy()
        && (ink.pattern == [0xFF; 8] || ink.pattern == [0x00; 8])
    {
        let solid_is_fg = (ink.pattern[0] == 0xFF) ^ ink.mode.inverts_source();
        let bit = if solid_is_fg { ink.fg } else { ink.bg } & 1 != 0;
        let op = if bit { BitRowOp::Set } else { BitRowOp::Clear };
        let mut scratch = Vec::new();
        return match paint_solid_1bpp_rect(memory, raster_surface, clip, area, op, &mut scratch) {
            DrawOutcome::Drawn { wrote, .. } => Some(wrote),
            DrawOutcome::Declined => None,
        };
    }
    None
}

/// Complements a `1`-bpp rectangle on `surface` without clip regions, matching
/// PowerPC `InvertRect`.
pub(super) fn ppc_invert_1bpp_rect(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    area: Rect,
) -> Option<bool> {
    if surface.front_buffer.depth != 1 {
        return None;
    }
    let raster_surface = ppc_raster_surface(surface, Depth::D1);
    let clip = ClipRows::rectangular(ClipRows::UNBOUNDED);
    let mut scratch = Vec::new();
    let mut memory = PpcPixelFallback {
        memory,
        pixel_len: 1,
    };
    match paint_solid_1bpp_rect(
        &mut memory,
        &raster_surface,
        &clip,
        area,
        BitRowOp::Toggle,
        &mut scratch,
    ) {
        DrawOutcome::Drawn { wrote, .. } => Some(wrote),
        DrawOutcome::Declined => None,
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
    // The earlier 1-bit fill draws nothing while pnVis is negative; keep
    // that by leaving hidden-pen fills to it.
    let solid_1bpp = front_buffer.depth == 1
        && (pattern == [0xFF; 8] || pattern == [0x00; 8])
        && !ppc_port_pen_hidden(memory, current_gworld);
    let Some(depth) = Depth::from_bits(front_buffer.depth)
        .filter(|depth| supports(*depth, TransferMode::PatCopy, PatternKind::Mono) || solid_1bpp)
    else {
        return Some(DrawOutcome::Declined);
    };
    let raster_surface = ppc_raster_surface(surface, depth);
    let Some(area) = raster_surface.clip(Rect {
        top,
        left,
        bottom,
        right,
    }) else {
        return Some(DrawOutcome::Drawn {
            touched: EMPTY_RECT,
            wrote: false,
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
    let ink = Ink {
        fg: u32::from(fg),
        bg: u32::from(bg),
        pattern,
        mode: TransferMode::PatCopy,
        pat_origin: (0, 0),
    };
    // Whole-byte fills have stored whole runs since they moved to the core.
    let writes = PpcRunWrites::Whole;
    let Some(wrote) = ppc_draw_mono_rect(memory, surface, current_gworld, area, true, &ink, writes)
    else {
        return Some(DrawOutcome::Declined);
    };
    Some(DrawOutcome::Drawn {
        touched: if wrote { area } else { EMPTY_RECT },
        wrote,
    })
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
    if !memory.quickdraw_gpu_renderer().prefers_precomputed_raster()
        || ink.mode != TransferMode::PatCopy
    {
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
pub(super) fn ppc_surface_port_bounds(surface: PpcQuickDrawSurface) -> Rect {
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
    ppc_port_clip_rows_with_clip(memory, port, top, bottom, true)
}

fn ppc_port_clip_rows_with_clip(
    memory: &mut PpcSectionMem,
    port: u32,
    top: i16,
    bottom: i16,
    respect_clip: bool,
) -> Option<ClipRows> {
    let storages = [
        memory
            .read_u32_be(port.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
            .and_then(|rgn| ppc_region_storage(memory, rgn)),
        respect_clip
            .then(|| {
                memory
                    .read_u32_be(port.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
                    .and_then(|rgn| ppc_region_storage(memory, rgn))
            })
            .flatten(),
    ];
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
