//! Shape computation and drawing helpers.

use super::types::{Rect, ShapeOp};
use crate::cpu::{CpuOps, Register};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::quickdraw::fonts::MONO_COVERAGE_THRESHOLD;
use crate::Result;

/// Linear-blend of fg → bg by `alpha` (0=bg, 255=fg). Channels are 16-bit
/// Mac RGB. Returned triple feeds `closest_clut_index` for antialiased
/// glyph edges onto 8bpp destinations.
///
/// `blend_rgb`, `lighten_stem_alpha`, and `fg_bg_low_contrast` serviced
/// the prior closest_clut_index blend path for partial glyph coverage;
/// that path was replaced by 4x4 Bayer dithering. Kept behind
/// `#[allow(dead_code)]` as primitives for a future smarter blend.
#[inline]
#[allow(dead_code)]
fn blend_rgb(fg: (u16, u16, u16), bg: (u16, u16, u16), alpha: u8) -> (u16, u16, u16) {
    let a = alpha as u32;
    let inv = 255 - a;
    let mix = |f: u16, b: u16| -> u16 {
        let v = (f as u32 * a + b as u32 * inv + 127) / 255;
        v.min(0xFFFF) as u16
    };
    (mix(fg.0, bg.0), mix(fg.1, bg.1), mix(fg.2, bg.2))
}

/// Stem-lightening curve for antialiased glyph edges. Maps the raw 8-bit
/// coverage value through `out = (in/255)^2 * 255` to fade partial-alpha
/// pixels more aggressively, perceptually thinning bold stems.
///
/// Only applied to the partial-coverage path (alpha 1..127) — fully-
/// covered pixels (alpha >= 128) take the boolean transfer-mode write
/// in draw_generic_shape, so srcOr/srcCopy/srcXor semantics are unchanged.
#[inline]
#[allow(dead_code)]
fn lighten_stem_alpha(alpha: u8) -> u8 {
    let a = alpha as u32;
    ((a * a + 127) / 255).min(255) as u8
}

/// Detect fg/bg colour pairs where CLUT-quantised antialiasing produces
/// visibly fuzzy text on the 8bpp framebuffer. Returns true when
/// antialiasing should be SKIPPED (use crisp 1-bit edges instead).
///
/// The only provably safe case for antialiasing on the standard Mac 8bpp
/// CLUT is grayscale blending — the CLUT has a dense gray ramp (entries
/// 245-254) that handles intermediate luminance values cleanly. For
/// chromatic colours, mid-hue blends land on whatever-the-nearest CLUT
/// entry happens to be, which can be visually wrong.
#[inline]
#[allow(dead_code)]
fn fg_bg_low_contrast(fg: (u16, u16, u16), bg: (u16, u16, u16)) -> bool {
    // A colour counts as "gray" when its R/G/B channels span less
    // than 12% of the 16-bit range — accommodates pure white/black
    // (span = 0) and light-tinted system grays like (EE, EE, EE).
    let is_gray = |c: (u16, u16, u16)| -> bool {
        let max = c.0.max(c.1).max(c.2);
        let min = c.0.min(c.1).min(c.2);
        (max - min) < 0x2000
    };
    // Both gray → antialias (CLUT gray ramp handles it cleanly).
    if is_gray(fg) && is_gray(bg) {
        return false;
    }
    // At least one colour is chromatic. Check whether they share the
    // same dominant channel — that's the "muddy mid-hue" failure mode
    // (EV HUD's light-green-on-dark-green; both have G dominant).
    let dominant = |c: (u16, u16, u16)| -> u8 {
        if c.0 >= c.1 && c.0 >= c.2 {
            0
        } else if c.1 >= c.2 {
            1
        } else {
            2
        }
    };
    dominant(fg) == dominant(bg)
}

use std::sync::OnceLock;
static TRACE_MENU_REDRAWS: OnceLock<bool> = OnceLock::new();
static TRACE_DIALOG_DRAW: OnceLock<bool> = OnceLock::new();
static TRACE_DIALOG_TEXT: OnceLock<bool> = OnceLock::new();
static TRACE_LARGE_SHAPES: OnceLock<bool> = OnceLock::new();
static TRACE_ALL_SHAPES: OnceLock<bool> = OnceLock::new();

fn trace_menu_redraw_enabled() -> bool {
    *TRACE_MENU_REDRAWS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_MENU_REDRAWS").is_some())
}

fn trace_dialog_draw_enabled() -> bool {
    *TRACE_DIALOG_DRAW.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_DRAW").is_some())
}

fn trace_dialog_text_enabled() -> bool {
    *TRACE_DIALOG_TEXT.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_TEXT").is_some())
}

fn trace_large_shapes_enabled() -> bool {
    *TRACE_LARGE_SHAPES.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_LARGE_SHAPES").is_some())
}

/// Log EVERY rect op (not just large ones). The large-shapes gate fires
/// only on width/height >= 200 or area >= 40k.
fn trace_all_shapes_enabled() -> bool {
    *TRACE_ALL_SHAPES.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SHAPES_ALL").is_some())
}

fn shape_palette_index_for_rgb(rgb: [u16; 3], pixel_size: u16, clut: &[[u16; 3]; 256]) -> u8 {
    // Color QuickDraw maps RGB colors through the destination GDevice's
    // inverse table. The default 4-bit NewGWorld table uses ROM propagation
    // and tie-breaking, which can differ from a fresh Euclidean CLUT search.
    //
    // Inside Macintosh: Imaging With QuickDraw 1994, pp. 4-82 and 6-30
    if pixel_size == 4 && super::dispatch::TrapDispatcher::uses_standard_mac_4bpp_gworld_clut(clut)
    {
        return super::dispatch::TrapDispatcher::standard_mac_4bpp_gworld_color2index(
            rgb[0], rgb[1], rgb[2],
        );
    }
    super::pict::closest_clut_index(rgb[0], rgb[1], rgb[2], clut)
}

fn indexed_shape_color_index(
    port_pixel: u32,
    effective_rgb: (u16, u16, u16),
    pixel_size: u16,
    clut: &[[u16; 3]; 256],
    port_pixel_is_resolved: bool,
    generated_rgb: bool,
) -> u8 {
    if generated_rgb || !port_pixel_is_resolved {
        let (r, g, b) = effective_rgb;
        shape_palette_index_for_rgb([r, g, b], pixel_size, clut)
    } else {
        (port_pixel as u8) & ((1u16 << pixel_size) - 1) as u8
    }
}

/// The RGB of the ColorSpec whose `value` field is `wanted_value`, if the
/// table has one: the entry at that ordinal is tried first (index-addressed
/// tables), then the table is scanned (device tables carry client IDs in
/// `value`). The table is fetched into a local buffer with one bulk read;
/// the scan used to issue a bus read per entry, and it runs for every shape.
#[allow(dead_code)]
fn ctab_rgb_for_value(bus: &MacMemoryBus, ctab_handle: u32, wanted_value: u8) -> Option<[u16; 3]> {
    let entries = ctab_entries(bus, ctab_handle)?;
    ctab_entries_rgb_for_value(&entries, wanted_value)
}

/// The ColorSpec entries of a color table (8 bytes each: value, r, g, b),
/// read in one bulk transfer.
fn ctab_entries(bus: &MacMemoryBus, ctab_handle: u32) -> Option<Vec<u8>> {
    if ctab_handle == 0 {
        return None;
    }
    let ctab = bus.read_long(ctab_handle);
    if ctab == 0 {
        return None;
    }
    let count = usize::from(bus.read_word(ctab + 6).min(255)) + 1;
    let mut entries = vec![0u8; count * 8];
    bus.read_bytes_into(ctab + 8, &mut entries);
    Some(entries)
}

fn ctab_entries_rgb_for_value(entries: &[u8], wanted_value: u8) -> Option<[u16; 3]> {
    let rgb = |entry: &[u8]| {
        [
            u16::from_be_bytes([entry[2], entry[3]]),
            u16::from_be_bytes([entry[4], entry[5]]),
            u16::from_be_bytes([entry[6], entry[7]]),
        ]
    };
    let matches =
        |entry: &[u8]| u16::from_be_bytes([entry[0], entry[1]]) == u16::from(wanted_value);
    let ordinal = usize::from(wanted_value);
    if let Some(entry) = entries.chunks_exact(8).nth(ordinal) {
        if matches(entry) {
            return Some(rgb(entry));
        }
    }
    entries
        .chunks_exact(8)
        .find(|entry| matches(entry))
        .map(rgb)
}

fn ctab_uses_noncanonical_black(bus: &MacMemoryBus, ctab_handle: u32) -> bool {
    let Some(entries) = ctab_entries(bus, ctab_handle) else {
        return false;
    };
    ctab_entries_rgb_for_value(&entries, 1) == Some([0, 0, 0])
        && ctab_entries_rgb_for_value(&entries, 255) != Some([0, 0, 0])
}

fn trace_menu_probe_points() -> [(&'static str, i16, i16); 2] {
    [("orb", 337, 220), ("enter_ship", 307, 500)]
}

fn trace_menu_rect_intersects(top: i16, left: i16, bottom: i16, right: i16) -> bool {
    const MENU_TOP: i16 = 260;
    const MENU_LEFT: i16 = 120;
    const MENU_BOTTOM: i16 = 390;
    const MENU_RIGHT: i16 = 680;

    top < MENU_BOTTOM && bottom > MENU_TOP && left < MENU_RIGHT && right > MENU_LEFT
}

fn trace_menu_rect_contains_point(
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
    y: i16,
    x: i16,
) -> bool {
    y >= top && y < bottom && x >= left && x < right
}

fn trace_dialog_rect_intersects(top: i16, left: i16, bottom: i16, right: i16) -> bool {
    const PANE_TOP: i16 = 8;
    const PANE_LEFT: i16 = 353;
    const PANE_BOTTOM: i16 = 148;
    const PANE_RIGHT: i16 = 603;

    top < PANE_BOTTOM && bottom > PANE_TOP && left < PANE_RIGHT && right > PANE_LEFT
}

fn normalize_boolean_transfer_mode(mode: i16) -> i16 {
    match mode {
        0..=3 => mode,
        8..=11 => mode - 8,
        _ => 0,
    }
}

/// What a whole-row 1bpp operation does to every bit of the clipped rect.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BitRowOp {
    Set,
    Clear,
    Toggle,
}

/// A solid all-black pattern sets every bit and a solid all-white pattern
/// clears every bit (`apply_boolean_transfer_1` in srcCopy: the result is
/// the source bit); anything else needs the per-pixel pattern lookup.
fn solid_bit_row_op(pattern: [u8; 8]) -> Option<BitRowOp> {
    if pattern == [0xFF; 8] {
        Some(BitRowOp::Set)
    } else if pattern == [0x00; 8] {
        Some(BitRowOp::Clear)
    } else {
        None
    }
}

/// Apply `op` to bits `first_bit..end_bit` (MSB-first, as QuickDraw packs
/// them) of the 1bpp row at `row_base`: masked read-modify-write of the two
/// edge bytes, one bulk write for the whole bytes between.
fn apply_bit_row_op(
    bus: &mut MacMemoryBus,
    row_base: u32,
    first_bit: u32,
    end_bit: u32,
    op: BitRowOp,
) {
    debug_assert!(first_bit < end_bit);
    let first_byte = first_bit / 8;
    let last_byte = (end_bit - 1) / 8;
    let head_mask = 0xFFu8 >> (first_bit % 8);
    let tail_mask = 0xFFu8 << (7 - ((end_bit - 1) % 8));
    let apply = |old: u8, mask: u8| match op {
        BitRowOp::Set => old | mask,
        BitRowOp::Clear => old & !mask,
        BitRowOp::Toggle => old ^ mask,
    };
    if first_byte == last_byte {
        let addr = row_base + first_byte;
        bus.write_byte(addr, apply(bus.read_byte(addr), head_mask & tail_mask));
        return;
    }
    let head_addr = row_base + first_byte;
    bus.write_byte(head_addr, apply(bus.read_byte(head_addr), head_mask));
    let middle = last_byte - first_byte - 1;
    if middle > 0 {
        let middle_addr = head_addr + 1;
        match op {
            BitRowOp::Set => bus.fill_bytes(middle_addr, middle, 0xFF),
            BitRowOp::Clear => bus.fill_bytes(middle_addr, middle, 0x00),
            BitRowOp::Toggle => {
                let mut row = vec![0u8; middle as usize];
                bus.read_bytes_into(middle_addr, &mut row);
                for byte in row.iter_mut() {
                    *byte = !*byte;
                }
                bus.write_bytes(middle_addr, &row);
            }
        }
    }
    let tail_addr = row_base + last_byte;
    bus.write_byte(tail_addr, apply(bus.read_byte(tail_addr), tail_mask));
}

fn invert_indexed_pixel(old: u8) -> u8 {
    255 - old
}

fn apply_boolean_transfer_1(old: bool, mode: i16, source_is_black: bool) -> bool {
    match normalize_boolean_transfer_mode(mode) {
        0 => source_is_black,
        1 => old || source_is_black,
        2 => old ^ source_is_black,
        3 => old && !source_is_black,
        _ => source_is_black,
    }
}

fn apply_boolean_transfer_8(
    old: u8,
    mode: i16,
    source_is_black: bool,
    fg_idx: u8,
    bg_idx: u8,
) -> u8 {
    match normalize_boolean_transfer_mode(mode) {
        0 => {
            if source_is_black {
                fg_idx
            } else {
                bg_idx
            }
        }
        1 => {
            if source_is_black {
                fg_idx
            } else {
                old
            }
        }
        2 => {
            if source_is_black {
                invert_indexed_pixel(old)
            } else {
                old
            }
        }
        3 => {
            if source_is_black {
                bg_idx
            } else {
                old
            }
        }
        _ => old,
    }
}

fn apply_boolean_transfer_32(
    old: u32,
    mode: i16,
    source_is_black: bool,
    fg_color: u32,
    bg_color: u32,
) -> u32 {
    match normalize_boolean_transfer_mode(mode) {
        0 => {
            if source_is_black {
                fg_color
            } else {
                bg_color
            }
        }
        1 => {
            if source_is_black {
                fg_color
            } else {
                old
            }
        }
        2 => {
            if source_is_black {
                !old & 0x00FF_FFFF
            } else {
                old
            }
        }
        3 => {
            if source_is_black {
                bg_color
            } else {
                old
            }
        }
        _ => old,
    }
}

impl super::TrapDispatcher {
    pub(super) fn draw_rect<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        r: &Rect,
        op: ShapeOp,
    ) {
        {
            let width = i32::from(r.right) - i32::from(r.left);
            let height = i32::from(r.bottom) - i32::from(r.top);
            let area = width.saturating_mul(height);
            let large = width >= 200 || height >= 200 || area >= 40_000;
            let should_trace =
                trace_all_shapes_enabled() || (trace_large_shapes_enabled() && large);
            if should_trace {
                let op_name = match &op {
                    ShapeOp::Paint => "paint",
                    ShapeOp::Frame => "frame",
                    ShapeOp::Erase => "erase",
                    ShapeOp::Invert => "invert",
                    ShapeOp::Fill(_) => "fill",
                    ShapeOp::Glyph(_) => "glyph",
                };
                eprintln!(
                    "[SHAPE] rect op={} rect=({},{}..{},{}) area={} port=${:08X} tick={}",
                    op_name,
                    r.top,
                    r.left,
                    r.bottom,
                    r.right,
                    area,
                    *self.current_port,
                    self.current_tick(),
                );
            }
        }
        let (pen_h, pen_w) = self.pn_size;
        self.draw_generic_shape(cpu, bus, r, op, true, |y, x| {
            if y < r.top || y >= r.bottom || x < r.left || x >= r.right {
                return 0;
            }
            let inside = if let ShapeOp::Frame = op {
                y < r.top + pen_h
                    || y >= r.bottom - pen_h
                    || x < r.left + pen_w
                    || x >= r.right - pen_w
            } else {
                true
            };
            if inside {
                255
            } else {
                0
            }
        });
    }

    /// Read polygon vertices from a guest PolyHandle.
    /// PolyRec layout: polySize(2) + polyBBox(8) + polyPoints(4*N)
    /// Inside Macintosh Volume I, I-189
    pub(super) fn read_poly_vertices(
        &self,
        bus: &MacMemoryBus,
        poly_handle: u32,
    ) -> Vec<(i16, i16)> {
        if poly_handle == 0 {
            return vec![];
        }
        let poly_ptr = bus.read_long(poly_handle);
        if poly_ptr == 0 {
            return vec![];
        }
        let size = bus.read_word(poly_ptr) as u32;
        let n_points = size.saturating_sub(10) / 4;
        let mut verts = Vec::with_capacity(n_points as usize);
        for i in 0..n_points {
            let base = poly_ptr + 10 + i * 4;
            let v = bus.read_word(base) as i16;
            let h = bus.read_word(base + 2) as i16;
            verts.push((v, h));
        }
        verts
    }

    /// Draw a filled polygon using scanline edge-intersection (even-odd rule).
    /// Inside Macintosh Volume I, I-190
    pub(super) fn draw_poly<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        poly_handle: u32,
        op: ShapeOp,
    ) {
        let verts = self.read_poly_vertices(bus, poly_handle);
        if verts.len() < 2 {
            return;
        }

        // Read bounding box from PolyRec via single deref of the PolyHandle.
        let poly_ptr = bus.read_long(poly_handle);
        let bbox = Rect {
            top: bus.read_word(poly_ptr + 2) as i16,
            left: bus.read_word(poly_ptr + 4) as i16,
            bottom: bus.read_word(poly_ptr + 6) as i16,
            right: bus.read_word(poly_ptr + 8) as i16,
        };

        if bbox.bottom <= bbox.top || bbox.right <= bbox.left {
            return;
        }

        // Build edge list from consecutive vertex pairs (including closing edge)
        let n = verts.len();
        let mut edges: Vec<(i16, i16, i16, i16)> = Vec::with_capacity(n);
        for i in 0..n {
            let (v0, h0) = verts[i];
            let (v1, h1) = verts[(i + 1) % n];
            if v0 != v1 {
                // Skip horizontal edges — they don't contribute intersections
                edges.push((v0, h0, v1, h1));
            }
        }

        // Precompute edge data for scanline intersection
        // Each edge: (y_min, y_max, x_at_ymin, dx_per_scanline as f32)
        struct Edge {
            y_min: i16,
            y_max: i16,
            x_at_ymin: f32,
            inv_slope: f32,
        }
        let edge_data: Vec<Edge> = edges
            .iter()
            .map(|&(v0, h0, v1, h1)| {
                let (y_min, y_max, x_start) = if v0 < v1 {
                    (v0, v1, h0 as f32)
                } else {
                    (v1, v0, h1 as f32)
                };
                let inv_slope = (h1 as f32 - h0 as f32) / (v1 as f32 - v0 as f32);
                Edge {
                    y_min,
                    y_max,
                    x_at_ymin: x_start,
                    inv_slope,
                }
            })
            .collect();

        // For each scanline, compute edge intersections and fill spans
        self.draw_generic_shape(cpu, bus, &bbox, op, false, |y, x| {
            // Count edge crossings to the left of (or at) x using even-odd rule
            let mut crossings = 0u32;
            for edge in &edge_data {
                if y < edge.y_min || y >= edge.y_max {
                    continue;
                }
                let x_intersect = edge.x_at_ymin + (y - edge.y_min) as f32 * edge.inv_slope;
                if x_intersect <= x as f32 {
                    crossings += 1;
                }
            }
            if crossings & 1 != 0 {
                255
            } else {
                0
            }
        });
    }

    /// Draw an arc (wedge/pie slice) within the bounding rect.
    /// Mac arcs: 0° = 12 o'clock (north), positive = clockwise.
    /// Inside Macintosh Volume I, I-184
    pub(super) fn draw_arc<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        r: &Rect,
        start_angle: i16,
        arc_angle: i16,
        op: ShapeOp,
    ) {
        let width = r.right - r.left;
        let height = r.bottom - r.top;
        if width <= 0 || height <= 0 || arc_angle == 0 {
            return;
        }

        // Normalize angles to 0..360 range
        let mut a_start = start_angle as f64;
        let mut a_extent = arc_angle as f64;
        // Handle negative arc angles (counter-clockwise)
        if a_extent < 0.0 {
            a_start += a_extent;
            a_extent = -a_extent;
        }
        // Clamp extent to 360
        if a_extent > 360.0 {
            a_extent = 360.0;
        }
        // Normalize start into [0, 360)
        a_start = a_start.rem_euclid(360.0);
        let a_end = a_start + a_extent;

        // Convert Mac angle convention (0°=north, CW) to math convention
        // (0°=east, CCW) for atan2-based testing:
        // math_angle = 90 - mac_angle
        // We'll test each pixel by computing its Mac-convention angle from center.

        let cx = (r.left as f64 + r.right as f64) / 2.0;
        let cy = (r.top as f64 + r.bottom as f64) / 2.0;
        let rx = width as f64 / 2.0;
        let ry = height as f64 / 2.0;

        let (pen_h, pen_w) = self.pn_size;

        self.draw_generic_shape(cpu, bus, r, op, false, |y, x| {
            // Check if point is inside the oval
            let dx = (x as f64 - cx + 0.5) / rx;
            let dy = (y as f64 - cy + 0.5) / ry;
            let dist_sq = dx * dx + dy * dy;

            if matches!(op, ShapeOp::Frame) {
                // For frame, check if inside outer oval but outside inner oval
                let inner_rx = rx - pen_w as f64;
                let inner_ry = ry - pen_h as f64;
                if inner_rx <= 0.0 || inner_ry <= 0.0 {
                    if dist_sq > 1.0 {
                        return 0;
                    }
                } else {
                    let idx = (x as f64 - cx + 0.5) / inner_rx;
                    let idy = (y as f64 - cy + 0.5) / inner_ry;
                    let inner_dist = idx * idx + idy * idy;
                    if dist_sq > 1.0 || inner_dist <= 1.0 {
                        return 0;
                    }
                }
            } else if dist_sq > 1.0 {
                return 0;
            }

            // Compute Mac-convention angle: 0°=north, CW
            // atan2 gives angle from east, CCW. Convert:
            // mac_angle = 90 - math_angle = 90 - atan2(dy, dx) in degrees
            // But we need to account for the oval aspect ratio
            let angle = (-(y as f64 - cy + 0.5)).atan2(x as f64 - cx + 0.5);
            let mut mac_angle = 90.0 - angle.to_degrees();
            if mac_angle < 0.0 {
                mac_angle += 360.0;
            }

            // Check if angle is within the arc range
            if mac_angle >= a_start && mac_angle < a_end {
                return 255;
            }
            // Handle wraparound (e.g., arc from 350° to 370° = 350..360 + 0..10)
            if a_end > 360.0 && mac_angle + 360.0 < a_end {
                return 255;
            }
            0
        });
    }

    /// Draw a region using its scanline-encoded data.
    /// Region format (Inside Macintosh Volume I, I-141):
    ///   rgnSize(2) + rgnBBox(8) + [scanline data...]
    /// If rgnSize == 10: rectangular region (just bbox).
    /// If rgnSize > 10: scanline data follows as:
    ///   y(2) x1(2) x2(2) ... 0x7FFF(2)  (pairs toggle inside/outside)
    ///   ...
    ///   0x7FFF(2) (region terminator)
    pub(super) fn draw_rgn<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        rgn_handle: u32,
        op: ShapeOp,
    ) {
        if rgn_handle == 0 {
            return;
        }
        let rgn_ptr = bus.read_long(rgn_handle);
        if rgn_ptr == 0 {
            return;
        }
        let rgn_size = bus.read_word(rgn_ptr) as u32;
        let bbox = Rect {
            top: bus.read_word(rgn_ptr + 2) as i16,
            left: bus.read_word(rgn_ptr + 4) as i16,
            bottom: bus.read_word(rgn_ptr + 6) as i16,
            right: bus.read_word(rgn_ptr + 8) as i16,
        };

        if bbox.bottom <= bbox.top || bbox.right <= bbox.left {
            return;
        }

        if rgn_size <= 10 {
            // Rectangular region — draw as a rect.
            // FrameRgn draws the frame of the bbox; all other verbs fill the
            // interior exactly as the corresponding Rect verb would.
            if matches!(op, ShapeOp::Frame) {
                self.draw_rect(cpu, bus, &bbox, ShapeOp::Frame);
            } else {
                self.draw_rect(cpu, bus, &bbox, op);
            }
            return;
        }

        // Complex (non-rectangular) region: decode the differential scanline
        // data using the same region-membership cache that region_contains_point
        // uses.  The QuickDraw region format is differential: each y-entry
        // specifies a delta that XOR-merges into the running active-span set
        // and persists until the next y-entry changes it.  See IM:I I-142 and
        // build_region_membership_cache in quickdraw.rs for the full parser.
        let cache = Self::build_region_membership_cache(bus, rgn_handle, bbox.top, bbox.bottom);

        // Imaging With QuickDraw (1994), pp. 3-100--3-101 specifies FrameRgn
        // as CopyRgn + InsetRgn(pnSize) + DiffRgn: paint the original region
        // minus its inset, never the whole interior.  InsetRgn's horizontal
        // erosion shrinks every span; its vertical erosion intersects the
        // neighbouring rows.  Computing that membership here avoids temporary
        // guest handles while retaining the documented geometry.
        if matches!(op, ShapeOp::Frame) {
            let Some(cache) = cache else {
                return;
            };
            let pen_h = self.pn_size.0.max(0) as i32;
            let pen_w = self.pn_size.1.max(0);
            let inset_rows = cache
                .rows
                .iter()
                .map(|row| Self::inset_region_row(row, pen_w))
                .collect::<Vec<_>>();
            let top = i32::from(bbox.top);
            let bottom = i32::from(bbox.bottom);

            self.draw_generic_shape(cpu, bus, &bbox, op, false, |y, x| {
                let row = i32::from(y) - top;
                if row < 0
                    || row >= cache.rows.len() as i32
                    || !Self::endpoints_contain_point(&cache.rows[row as usize], x)
                {
                    return 0;
                }

                // A zero-sized pen draws no frame, matching the rectangular
                // shape paths. Otherwise this is membership in the region
                // produced by InsetRgn(rgn, pnSize.h, pnSize.v).
                if pen_h == 0 && pen_w == 0 {
                    return 0;
                }
                let first_source_y = i32::from(y) - pen_h;
                let last_source_y = i32::from(y) + pen_h;
                let inside_inset = first_source_y >= top
                    && last_source_y < bottom
                    && (first_source_y..=last_source_y).all(|source_y| {
                        Self::endpoints_contain_point(&inset_rows[(source_y - top) as usize], x)
                    });
                if inside_inset {
                    0
                } else {
                    255
                }
            });
            return;
        }

        self.draw_generic_shape(cpu, bus, &bbox, op, false, |y, x| {
            if let Some(c) = &cache {
                let row = (y - c.top) as usize;
                if row < c.rows.len() && Self::endpoints_contain_point(&c.rows[row], x) {
                    return 255;
                }
                return 0;
            }
            // Cache build failed: fall back to filled bbox so the trap still
            // produces visible output rather than silently doing nothing.
            255
        });
    }

    pub(crate) fn compute_oval_spans(width: i16, height: i16) -> Vec<(i16, i16)> {
        if width <= 0 || height <= 0 {
            return vec![];
        }

        // Bill Atkinson's QuickDraw Oval Algorithm (from references/QuickDraw/DrawArc.a)
        // This is a 100% bit-accurate recreation using a 64-bit fixed-point difference engine.
        let mut spans = vec![(0, 0); height as usize];

        // InitOval logic (DrawArc.a:898)
        let mut oval_y = 1 - height;
        let mut rsq_ysq = 2 * (height as i32) - 1;
        let mut square = 0i64; // 32.32 FIXED

        let width_f = (width as i32) << 16;
        let half_width = width_f >> 1;

        let mut left_edge = half_width;
        let mut right_edge = (width_f - half_width) + 0x8000; // 0.5 bias for rounding

        // ODDNUM = (H/W)^2 as 32.32 Fixed.
        // Uses _FixRatio (16.16) then _LongMul (32.32).
        let ratio = ((height as i64) << 16) / (width as i64);
        let mut odd_num = ratio * ratio; // 16.16 * 16.16 -> 32.32
        let odd_bump = odd_num * 2;

        let half_f = 0x8000i32;

        for y_idx in 0..height {
            // BumpOval logic (DrawArc.a:1003) - Bumps BEFORE finalizing each scanline.
            // PutOval.a (line 143) shows that vertical N uses edges after N+1 bumps.

            // WHILE SQUARE < RSQYSQ DO MAKE OVAL BIGGER
            while (square >> 32) < (rsq_ysq as i64) {
                right_edge += half_f;
                left_edge -= half_f;
                square += odd_num;
                odd_num += odd_bump;
            }
            // WHILE SQUARE > RSQYSQ DO MAKE OVAL SMALLER
            while (square >> 32) > (rsq_ysq as i64) {
                right_edge -= half_f;
                left_edge += half_f;
                odd_num -= odd_bump;
                square -= odd_num;
            }

            let l = (left_edge >> 16) as i16;
            let r = (right_edge >> 16) as i16;
            spans[y_idx as usize] = (l.max(0), r.min(width));

            // Update RSQYSQ for next scanline: RSQYSQ := RSQYSQ - 4 * (OVALY + 1)
            rsq_ysq -= 4 * (oval_y as i32 + 1);
            oval_y += 2;
        }

        spans
    }

    pub(super) fn draw_oval<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        r: &Rect,
        op: ShapeOp,
    ) {
        let (pen_h, pen_w) = self.pn_size;
        let width = r.right - r.left;
        let height = r.bottom - r.top;
        if width <= 0 || height <= 0 {
            return;
        }

        let spans = Self::compute_oval_spans(width, height);
        let r_inset = Rect {
            top: r.top + pen_h,
            left: r.left + pen_w,
            bottom: r.bottom - pen_h,
            right: r.right - pen_w,
        };
        let spans_inset =
            Self::compute_oval_spans(r_inset.right - r_inset.left, r_inset.bottom - r_inset.top);

        self.draw_generic_shape(cpu, bus, r, op, false, |y, x| {
            let idx = (y - r.top) as usize;
            if idx >= spans.len() {
                return 0;
            }
            let (l_rel, r_rel) = spans[idx];
            let (l, r_edge) = (r.left + l_rel, r.left + r_rel);
            if x < l || x >= r_edge {
                return 0;
            }
            let inside = if let ShapeOp::Frame = op {
                let idx_in = (y - r_inset.top) as usize;
                if idx_in >= spans_inset.len() {
                    return 255;
                }
                let (li_rel, ri_rel) = spans_inset[idx_in];
                let (li, ri) = (r_inset.left + li_rel, r_inset.left + ri_rel);
                x < li || x >= ri
            } else {
                true
            };
            if inside {
                255
            } else {
                0
            }
        });
    }

    pub(crate) fn compute_rrect_spans(r: &Rect, ow: i16, oh: i16) -> Vec<(i16, i16)> {
        let width = r.right - r.left;
        let height = r.bottom - r.top;
        if width <= 0 || height <= 0 {
            return vec![];
        }

        let ow = ow.min(width).max(0);
        let oh = oh.min(height).max(0);

        if oh < 1 || ow < 1 {
            return vec![(r.left, r.right); height as usize];
        }

        let corner_spans = Self::compute_oval_spans(ow, oh);
        let mut spans = Vec::new();

        let mid_y = oh / 2;
        let insert_y = height - oh;
        let insert_x = width - ow;

        // Top curves
        for y in 0..mid_y {
            if (y as usize) < corner_spans.len() {
                let (l_rel, r_rel) = corner_spans[y as usize];
                spans.push((r.left + l_rel, r.left + r_rel + insert_x));
            }
        }

        // Stretched middle
        for _ in 0..insert_y {
            if (mid_y as usize) < corner_spans.len() {
                let (l_rel, r_rel) = corner_spans[mid_y as usize];
                spans.push((r.left + l_rel, r.left + r_rel + insert_x));
            } else {
                spans.push((r.left, r.right));
            }
        }

        // Bottom curves
        for y in mid_y..oh {
            if (y as usize) < corner_spans.len() {
                let (l_rel, r_rel) = corner_spans[y as usize];
                spans.push((r.left + l_rel, r.left + r_rel + insert_x));
            }
        }

        spans
    }

    pub(super) fn draw_round_rect<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        r: &Rect,
        ow: i16,
        oh: i16,
        op: ShapeOp,
    ) {
        let (pen_h, pen_w) = self.pn_size;

        let width = r.right - r.left;
        let height = r.bottom - r.top;
        if width <= 0 || height <= 0 {
            return;
        }

        let spans = Self::compute_rrect_spans(r, ow, oh);

        let r_inset = Rect {
            top: r.top + pen_h,
            left: r.left + pen_w,
            bottom: r.bottom - pen_h,
            right: r.right - pen_w,
        };
        let spans_inset = Self::compute_rrect_spans(&r_inset, ow - 2 * pen_w, oh - 2 * pen_h);

        self.draw_generic_shape(cpu, bus, r, op, false, |y, x| {
            let idx = (y - r.top) as usize;
            if idx >= spans.len() {
                return 0;
            }
            let (l, r_edge) = spans[idx];
            if x < l || x >= r_edge {
                return 0;
            }

            let inside = if let ShapeOp::Frame = op {
                let idx_in = (y - r_inset.top) as usize;
                if idx_in >= spans_inset.len() || y < r_inset.top || y >= r_inset.bottom {
                    return 255;
                }
                let (li, ri) = spans_inset[idx_in];
                x < li || x >= ri
            } else {
                true
            };
            if inside {
                255
            } else {
                0
            }
        });
    }

    /// QuickDraw-accurate line drawing.
    /// Based on original QuickDraw DrawLine.a source code.
    /// Uses fixed-point arithmetic matching FixRatio(dh,dv).
    /// Inside Macintosh Volume I, I-170 (LineTo)
    pub(super) fn draw_line<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        x1: i16,
        y1: i16,
        x2: i16,
        y2: i16,
    ) -> Result<()> {
        // Handle single point case
        if x1 == x2 && y1 == y2 {
            let r = Rect {
                top: y1,
                left: x1,
                bottom: y1 + self.pn_size.0,
                right: x1 + self.pn_size.1,
            };
            self.draw_rect(cpu, bus, &r, ShapeOp::Paint);
            return Ok(());
        }

        // Handle horizontal and vertical lines specially
        if x1 == x2 {
            // Vertical line
            let (top, bottom) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
            for y in top..=bottom {
                let r = Rect {
                    top: y,
                    left: x1,
                    bottom: y + self.pn_size.0,
                    right: x1 + self.pn_size.1,
                };
                self.draw_rect(cpu, bus, &r, ShapeOp::Paint);
            }
            return Ok(());
        }

        if y1 == y2 {
            // Horizontal line
            let (left, right) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
            for x in left..=right {
                let r = Rect {
                    top: y1,
                    left: x,
                    bottom: y1 + self.pn_size.0,
                    right: x + self.pn_size.1,
                };
                self.draw_rect(cpu, bus, &r, ShapeOp::Paint);
            }
            return Ok(());
        }

        // Dual-axis fixed-point DDA with slope/2 offset
        let (sx, sy, ex, ey) = if y1 <= y2 {
            (x1 as i32, y1 as i32, x2 as i32, y2 as i32)
        } else {
            (x2 as i32, y2 as i32, x1 as i32, y1 as i32)
        };

        let dh = (ex - sx).abs();
        let dv = ey - sy; // Always >= 0 after sort
        let x_dir: i32 = if ex > sx { 1 } else { -1 };

        if dv >= dh {
            // Primarily vertical - iterate over Y, step X with slope
            let slope: i32 = if dv != 0 {
                let num = ((ex - sx) as i64) << 16;
                let den = dv as i64;
                ((num + (den / 2)) / den) as i32
            } else {
                0
            };

            // Initialize with 0.5 fractional offset + slope/2 centering
            let mut x_fp: i32 = (sx << 16) | 0x8000;
            x_fp += slope >> 1;

            for y in sy..=ey {
                let x = x_fp >> 16;
                let r = Rect {
                    top: y as i16,
                    left: x as i16,
                    bottom: y as i16 + self.pn_size.0,
                    right: x as i16 + self.pn_size.1,
                };
                self.draw_rect(cpu, bus, &r, ShapeOp::Paint);
                x_fp += slope;
            }
        } else {
            // Primarily horizontal - iterate over X, step Y with slope
            let num = (dv as i64) << 16;
            let den = dh as i64;
            let slope: i32 = ((num + (den / 2)) / den) as i32;

            // Initialize with 0.5 fractional offset + slope/2 centering
            let mut y_fp: i32 = (sy << 16) | 0x8000;
            y_fp += slope >> 1;
            let mut x = sx;

            for _ in 0..=dh {
                let y = y_fp >> 16;
                let r = Rect {
                    top: y as i16,
                    left: x as i16,
                    bottom: y as i16 + self.pn_size.0,
                    right: x as i16 + self.pn_size.1,
                };
                self.draw_rect(cpu, bus, &r, ShapeOp::Paint);
                x += x_dir;
                y_fp += slope;
            }
        }

        Ok(())
    }

    /// Draw a clipped, transfer-mode-aware shape. The `coverage_at`
    /// closure returns 0..=255 alpha for each (y, x) in the shape's
    /// bounding rect:
    ///   - 0 → pixel is outside the shape (skipped)
    ///   - 255 → pixel is fully inside (written through the normal
    ///     boolean transfer-mode path for Paint/Frame/Erase/Fill/Invert)
    ///   - 1..=254 → ONLY meaningful for ShapeOp::Glyph on 8bpp
    ///     destinations, where it blends foreground → background and
    ///     writes the nearest CLUT index; other ops and 1bpp paths
    ///     threshold at >=128.
    ///
    /// Non-text shape callers should return 0 or 255 — their geometry
    /// is binary by nature. Only the text-render path exercises the
    /// full 8-bit gradient produced by fontdue's hinted rasteriser.
    pub(super) fn draw_generic_shape<C: CpuOps, F>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        r: &Rect,
        op: ShapeOp,
        full_rect_coverage: bool,
        coverage_at: F,
    ) where
        F: Fn(i16, i16) -> u8,
    {
        // HidePen decrements pn_vis below 0 to suppress ALL QuickDraw
        // drawing through StdRect / StdOval / StdRRect / StdLine /
        // StdText (frame/paint/erase/invert/fill + glyph) until the
        // matching ShowPen restores it. Inside Macintosh Volume I,
        // I-169 (HidePen).
        if self.pn_vis < 0
            && matches!(
                op,
                ShapeOp::Paint
                    | ShapeOp::Frame
                    | ShapeOp::Invert
                    | ShapeOp::Erase
                    | ShapeOp::Fill(_)
                    | ShapeOp::Glyph(_)
            )
        {
            return;
        }
        let a5 = cpu.read_reg(Register::A5);
        let global_ptr = bus.read_long(a5);
        let port = bus.read_long(global_ptr);

        if trace_dialog_text_enabled()
            && matches!(op, ShapeOp::Glyph(_))
            && port != *self.current_port
        {
            eprintln!(
                "[DIALOG-TEXT] Glyph port mismatch a5_port=${:08X} current_port=${:08X} rect=({},{}..{},{} )",
                port,
                *self.current_port,
                r.top,
                r.left,
                r.bottom,
                r.right,
            );
        }

        // Detect if this is a CGrafPort (portVersion at offset 6 has high bits set)
        let port_version = bus.read_word(port.wrapping_add(6));
        let is_color = (port_version & 0xC000) != 0;

        let (
            pix_base,
            pix_row_bytes,
            pixel_size,
            bounds_top,
            bounds_left,
            bounds_bottom,
            bounds_right,
        ) = if is_color {
            // In a CGrafPort, portPixMap is a handle at offset 2
            let pix_map_handle = bus.read_long(port.wrapping_add(2));
            if pix_map_handle == 0 {
                return;
            }
            let pix_map_ptr = bus.read_long(pix_map_handle);
            if pix_map_ptr == 0 {
                return;
            }

            let base = Self::offscreen_pixmap_base_ptr(bus, pix_map_ptr);
            let row_bytes = (bus.read_word(pix_map_ptr.wrapping_add(4)) & 0x3FFF) as u32;
            let top = bus.read_word(pix_map_ptr.wrapping_add(6)) as i16;
            let left = bus.read_word(pix_map_ptr.wrapping_add(8)) as i16;
            let bottom = bus.read_word(pix_map_ptr.wrapping_add(10)) as i16;
            let right = bus.read_word(pix_map_ptr.wrapping_add(12)) as i16;
            let size = bus.read_word(pix_map_ptr.wrapping_add(32));

            (base, row_bytes, size, top, left, bottom, right)
        } else {
            let base = bus.read_long(port.wrapping_add(2));
            let row_bytes = (bus.read_word(port.wrapping_add(6)) & 0x3FFF) as u32;
            let top = bus.read_word(port.wrapping_add(8)) as i16;
            let left = bus.read_word(port.wrapping_add(10)) as i16;
            let bottom = bus.read_word(port.wrapping_add(12)) as i16;
            let right = bus.read_word(port.wrapping_add(14)) as i16;
            // Generic safety net: a basic GrafPort (port_version & 0xC000
            // == 0) is implicitly 1bpp by Mac OS convention. But if `base`
            // happens to point at an indexed color screen, per-bit set/clear
            // into screen bytes corrupts packed pixels. Inside Macintosh V-122:
            // on a color screen, all GrafPorts displayed at the screen
            // base must use the screen's depth.
            let (screen_base_addr, screen_rb, _, _, screen_ps) = self.screen_mode;
            if matches!(screen_ps, 2 | 4 | 8) && base == screen_base_addr && row_bytes == screen_rb
            {
                (base, row_bytes, screen_ps, top, left, bottom, right)
            } else {
                (base, row_bytes, 1u16, top, left, bottom, right)
            }
        };

        // A MakeRGBPat-generated PixPat carries its requested color in
        // the installed pnPixPat/bkPixPat handle. Use that color for
        // color-port pen/background operations while retaining pat1Data
        // as the documented monochrome fallback on 1bpp destinations.
        let generated_pen_rgb = if is_color && matches!(op, ShapeOp::Paint | ShapeOp::Frame) {
            self.makergbpat_colors
                .get(&bus.read_long(port.wrapping_add(58)))
                .copied()
        } else {
            None
        };
        let generated_back_rgb = if is_color && matches!(op, ShapeOp::Erase) {
            self.makergbpat_colors
                .get(&bus.read_long(port.wrapping_add(32)))
                .copied()
        } else {
            None
        };
        let effective_pn_pat = if generated_pen_rgb.is_some() {
            [0xFF; 8]
        } else {
            self.pn_pat
        };
        let effective_bk_pat = if generated_back_rgb.is_some() {
            [0xFF; 8]
        } else {
            self.bk_pat
        };
        let effective_fg_color = generated_pen_rgb
            .or(generated_back_rgb)
            .unwrap_or(self.fg_color);
        let effective_bg_color = generated_pen_rgb
            .or(generated_back_rgb)
            .unwrap_or(self.bg_color);

        if trace_menu_redraw_enabled()
            && trace_menu_rect_intersects(r.top, r.left, r.bottom, r.right)
        {
            eprintln!(
                "[MENU-REDRAW] Shape {:?} port=${:08X} base=${:08X} rect=({},{}..{},{} )",
                op, port, pix_base, r.top, r.left, r.bottom, r.right,
            );
        }
        if trace_dialog_draw_enabled()
            && !matches!(op, ShapeOp::Glyph(_))
            && trace_dialog_rect_intersects(r.top, r.left, r.bottom, r.right)
        {
            eprintln!(
                "[DIALOG-DRAW] Shape {:?} port=${:08X} rect=({},{}..{},{} )",
                op, port, r.top, r.left, r.bottom, r.right,
            );
        }

        // Read visRgn bounds for clipping (GrafPort offset 24 = visRgn handle)
        let vis_rgn_handle = bus.read_long(port.wrapping_add(24));
        let (mut clip_top, mut clip_left, mut clip_bottom, mut clip_right) = if vis_rgn_handle != 0
        {
            let vis_rgn_ptr = bus.read_long(vis_rgn_handle);
            if vis_rgn_ptr != 0 {
                let vt = bus.read_word(vis_rgn_ptr + 2) as i16;
                let vl = bus.read_word(vis_rgn_ptr + 4) as i16;
                let vb = bus.read_word(vis_rgn_ptr + 6) as i16;
                let vr = bus.read_word(vis_rgn_ptr + 8) as i16;
                (
                    vt.max(bounds_top),
                    vl.max(bounds_left),
                    vb.min(bounds_bottom),
                    vr.min(bounds_right),
                )
            } else {
                (bounds_top, bounds_left, bounds_bottom, bounds_right)
            }
        } else {
            (bounds_top, bounds_left, bounds_bottom, bounds_right)
        };

        // Also intersect with clipRgn (GrafPort offset 28 = clipRgn handle).
        // Real QuickDraw clips to the intersection of portBits.bounds, visRgn, and clipRgn.
        let clip_rgn_handle = bus.read_long(port.wrapping_add(28));
        if clip_rgn_handle != 0 {
            let clip_rgn_ptr = bus.read_long(clip_rgn_handle);
            if clip_rgn_ptr != 0 {
                let ct = bus.read_word(clip_rgn_ptr + 2) as i16;
                let cl = bus.read_word(clip_rgn_ptr + 4) as i16;
                let cb = bus.read_word(clip_rgn_ptr + 6) as i16;
                let cr = bus.read_word(clip_rgn_ptr + 8) as i16;
                clip_top = clip_top.max(ct);
                clip_left = clip_left.max(cl);
                clip_bottom = clip_bottom.min(cb);
                clip_right = clip_right.min(cr);
            }
        }

        let touch_top = r.top.max(clip_top);
        let touch_left = r.left.max(clip_left);
        let touch_bottom = r.bottom.min(clip_bottom);
        let touch_right = r.right.min(clip_right);
        let touched_screen =
            pix_base == self.screen_mode.0 && touch_top < touch_bottom && touch_left < touch_right;
        if touched_screen {
            self.ensure_dialog_background_saved_for_screen_port(bus, port);

            // A large FrameRect is explicit guest-drawn presentation
            // geometry. Remember the outermost such frame so the frontend can
            // correlate it with a retained app-managed CPort instead of
            // assuming that buffer is centered. FrameRect draws its outline
            // "just inside" the supplied rectangle (Inside Macintosh:
            // Imaging With QuickDraw, 1994, p. 3-59), so the rectangle itself
            // is the visible extent; do not expand it by the pen dimensions.
            if matches!(op, ShapeOp::Frame) {
                let width = r.right.saturating_sub(r.left);
                let height = r.bottom.saturating_sub(r.top);
                let (_, _, screen_width, screen_height, _) = self.screen_mode;
                if width >= (screen_width as i16 / 2).max(1)
                    && height >= (screen_height as i16 / 2).max(1)
                    && width < screen_width as i16
                    && height < screen_height as i16
                {
                    let candidate = super::dispatch::ScreenCopyBitsRect {
                        src_top: r.top,
                        src_left: r.left,
                        src_bottom: r.bottom,
                        src_right: r.right,
                        dst_top: r.top,
                        dst_left: r.left,
                        dst_bottom: r.bottom,
                        dst_right: r.right,
                    };
                    let candidate_area = i64::from(width) * i64::from(height);
                    let current_area = if self.last_screen_frame_rect_tick == self.current_tick() {
                        self.last_screen_frame_rect
                            .map(|current| {
                                i64::from(current.dst_right.saturating_sub(current.dst_left))
                                    * i64::from(current.dst_bottom.saturating_sub(current.dst_top))
                            })
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    if self.last_screen_frame_rect_tick != self.current_tick()
                        || candidate_area > current_area
                    {
                        self.last_screen_frame_rect = Some(candidate);
                        self.last_screen_frame_rect_tick = self.current_tick();
                    }
                }
            }
        }

        let vis_region_complex =
            vis_rgn_handle != 0 && Self::region_is_complex(bus, vis_rgn_handle);
        let clip_region_complex =
            clip_rgn_handle != 0 && Self::region_is_complex(bus, clip_rgn_handle);
        let vis_region_cache = if vis_region_complex {
            Self::build_region_membership_cache(bus, vis_rgn_handle, touch_top, touch_bottom)
        } else {
            None
        };
        let clip_region_cache = if clip_region_complex {
            Self::build_region_membership_cache(bus, clip_rgn_handle, touch_top, touch_bottom)
        } else {
            None
        };
        let has_complex_port_clip = vis_region_complex || clip_region_complex;

        // For 8bpp, map fg/bg colors to the destination bitmap's effective CLUT.
        // QuickDraw's Color Manager maintains an inverse color table (ITable)
        // for mapping RGB colors to pixel indices. The ITable is derived
        // from the Color Manager's palette, NOT the live hardware CLUT.
        // Low-level video driver SetEntries (cscSetEntries) updates only
        // the hardware CLUT for palette animation/fading. The Color Manager
        // palette (and thus the ITable) is only updated by high-level
        // SetEntries ($AA3F) and ActivatePalette.
        // Imaging With QuickDraw 1994, p. 4-82
        // Designing Cards and Drivers 3rd Ed. 1992, p. 245-248
        let fg_idx;
        let bg_idx;
        let mut indexed_clut = None;
        if matches!(pixel_size, 2 | 4 | 8) && is_color {
            let pix_map_handle = bus.read_long(port.wrapping_add(2));
            let port_ctab_handle = if pix_map_handle != 0 {
                let pix_map_ptr = bus.read_long(pix_map_handle);
                if pix_map_ptr != 0 {
                    bus.read_long(pix_map_ptr + 42)
                } else {
                    0
                }
            } else {
                0
            };
            // For screen-backed ports, use the live device CLUT. Low-level
            // cscSetEntries updates the hardware palette immediately even when
            // the Color Manager table is stale. Offscreen ports continue to
            // use their own ColorTable.
            let is_screen_port = pix_base == self.screen_mode.0
                && pix_row_bytes == self.screen_mode.1
                && pixel_size == self.screen_mode.4;
            let current_ctab_handle = if port == *self.current_port {
                self.current_gdevice_ctab_handle(bus)
            } else {
                0
            };
            let use_raw_current_ctab =
                current_ctab_handle != 0 && ctab_uses_noncanonical_black(bus, current_ctab_handle);
            let ctab_handle = if use_raw_current_ctab {
                current_ctab_handle
            } else {
                port_ctab_handle
            };
            let port_clut = if is_screen_port {
                *self.device_clut
            } else if use_raw_current_ctab {
                self.read_ctab_handle_clut(bus, ctab_handle)
            } else {
                self.read_port_clut(bus, ctab_handle)
            };
            indexed_clut = Some(port_clut);
            let resolved_color_fields = self
                .resolved_port_color_fields
                .get(&port)
                .copied()
                .unwrap_or(0);
            // A CGrafPort stores the already-resolved destination pixel in
            // fgColor/bkColor. Applications may edit those fields directly,
            // while rgbFgColor/rgbBkColor retain the logical colors. Re-running
            // the RGB inverse lookup here loses that distinction, especially
            // while an indexed palette is being animated. Generated RGB pixel
            // patterns are the exception because their color does not come
            // from the port fields.
            // Inside Macintosh Volume V, pp. V-48 and V-163
            fg_idx = indexed_shape_color_index(
                bus.read_long(port + 80),
                effective_fg_color,
                pixel_size,
                &port_clut,
                (resolved_color_fields & 0x01) != 0,
                generated_pen_rgb.is_some(),
            );
            bg_idx = indexed_shape_color_index(
                bus.read_long(port + 84),
                effective_bg_color,
                pixel_size,
                &port_clut,
                (resolved_color_fields & 0x02) != 0,
                generated_back_rgb.is_some(),
            );
            if trace_dialog_text_enabled() && matches!(op, ShapeOp::Glyph(_)) {
                eprintln!(
                    "[DIALOG-TEXT] Glyph colors port=${:08X} fgRGB=({:04X},{:04X},{:04X}) bgRGB=({:04X},{:04X},{:04X}) fgIdx={} bgIdx={}",
                    port,
                    self.fg_color.0,
                    self.fg_color.1,
                    self.fg_color.2,
                    self.bg_color.0,
                    self.bg_color.1,
                    self.bg_color.2,
                    fg_idx,
                    bg_idx,
                );
            }
        } else {
            fg_idx = 255;
            bg_idx = 0;
        }

        let installed_raw_pixpat = if pixel_size == 8 && is_color {
            let handle = match op {
                ShapeOp::Paint | ShapeOp::Frame => bus.read_long(port.wrapping_add(58)),
                ShapeOp::Erase => bus.read_long(port.wrapping_add(32)),
                _ => 0,
            };
            self.decode_raw_pixpat(bus, handle)
        } else {
            None
        };

        if pixel_size == 8
            && full_rect_coverage
            && !has_complex_port_clip
            && installed_raw_pixpat.is_none()
        {
            // Whole-row 8bpp paths: a solid srcCopy fill writes one prepared
            // row per scanline; InvertRect maps each row through `255 - x`
            // (exactly `invert_indexed_pixel`, applied per pixel below) --
            // both instead of one bus round trip per pixel.
            let solid_fill_idx = self.solid_src_copy_fill_index(
                &op,
                fg_idx,
                bg_idx,
                effective_pn_pat,
                effective_bk_pat,
            );
            let invert_rows = matches!(op, ShapeOp::Invert);
            if solid_fill_idx.is_some() || invert_rows {
                let top = r.top.max(clip_top);
                let left = r.left.max(clip_left);
                let bottom = r.bottom.min(clip_bottom);
                let right = r.right.min(clip_right);
                if top < bottom && left < right {
                    let dx = (left - bounds_left) as u32;
                    let width = (right - left) as u32;
                    if dx < pix_row_bytes && width <= pix_row_bytes.saturating_sub(dx) {
                        let mut row = vec![solid_fill_idx.unwrap_or(0); width as usize];
                        for y in top..bottom {
                            let dy = (y - bounds_top) as u32;
                            let addr = pix_base + dy * pix_row_bytes + dx;
                            if invert_rows {
                                bus.read_bytes_into(addr, &mut row);
                                for pixel in row.iter_mut() {
                                    *pixel = invert_indexed_pixel(*pixel);
                                }
                            }
                            bus.write_bytes(addr, &row);
                        }
                        let screen_rect = (
                            top.saturating_sub(bounds_top),
                            left.saturating_sub(bounds_left),
                            bottom.saturating_sub(bounds_top),
                            right.saturating_sub(bounds_left),
                        );
                        self.refresh_dialog_saved_pixels_after_screen_draw(bus, port, screen_rect);
                        self.refresh_visible_dialog_snapshot_region_for_port(
                            bus,
                            port,
                            screen_rect,
                        );
                        return;
                    }
                }
            }
        }

        // Whole-row 1bpp paths. On a monochrome port the per-pixel arm
        // below reads and writes a byte per PIXEL; the ops that reduce to
        // "set", "clear" or "toggle" every bit of the clipped rect --
        // Erase with a solid pattern, srcCopy Paint/Fill with a solid
        // pattern, and Invert (mode 2 with a black source, i.e. `!old`) --
        // become masked writes at the two edge bytes and one bulk write in
        // between, exactly what `apply_boolean_transfer_1` yields per pixel.
        // EV Override clears its 1-bit offscreen buffers at boot with an
        // EraseRect + InvertRect pair over 8.3 M pixels each.
        if pixel_size == 1 && full_rect_coverage && !has_complex_port_clip {
            let bit_op = match op {
                ShapeOp::Invert => Some(BitRowOp::Toggle),
                ShapeOp::Erase => solid_bit_row_op(self.bk_pat),
                ShapeOp::Fill(pattern) => solid_bit_row_op(pattern),
                ShapeOp::Paint if normalize_boolean_transfer_mode(self.pn_mode) == 0 => {
                    solid_bit_row_op(self.pn_pat)
                }
                _ => None,
            };
            if let Some(bit_op) = bit_op {
                let top = r.top.max(clip_top);
                let left = r.left.max(clip_left);
                let bottom = r.bottom.min(clip_bottom);
                let right = r.right.min(clip_right);
                if top < bottom && left < right {
                    let first_bit = (left - bounds_left) as u32;
                    // Pixels past the row's last byte are skipped, as the
                    // per-pixel arm does.
                    let end_bit = ((right - bounds_left) as u32).min(pix_row_bytes * 8);
                    if first_bit < end_bit {
                        for y in top..bottom {
                            let row_base = pix_base + (y - bounds_top) as u32 * pix_row_bytes;
                            apply_bit_row_op(bus, row_base, first_bit, end_bit, bit_op);
                        }
                    }
                    let screen_rect = (
                        top.saturating_sub(bounds_top),
                        left.saturating_sub(bounds_left),
                        bottom.saturating_sub(bounds_top),
                        right.saturating_sub(bounds_left),
                    );
                    self.refresh_dialog_saved_pixels_after_screen_draw(bus, port, screen_rect);
                    self.refresh_visible_dialog_snapshot_region_for_port(bus, port, screen_rect);
                }
                return;
            }
        }

        // Resolve the port's CLUT once. `glyph_clut` is currently unused
        // (Bayer-dither path superseded the blend) but kept for a future
        // smarter blend path.
        #[allow(unused_variables)]
        let glyph_clut =
            if matches!(pixel_size, 2 | 4 | 8) && is_color && matches!(op, ShapeOp::Glyph(_)) {
                let pix_map_handle = bus.read_long(port.wrapping_add(2));
                let port_ctab_handle = if pix_map_handle != 0 {
                    let pix_map_ptr = bus.read_long(pix_map_handle);
                    if pix_map_ptr != 0 {
                        bus.read_long(pix_map_ptr + 42)
                    } else {
                        0
                    }
                } else {
                    0
                };
                let is_screen_port = pix_base == self.screen_mode.0
                    && pix_row_bytes == self.screen_mode.1
                    && pixel_size == self.screen_mode.4;
                let current_ctab_handle = if port == *self.current_port {
                    self.current_gdevice_ctab_handle(bus)
                } else {
                    0
                };
                let use_raw_current_ctab = current_ctab_handle != 0
                    && ctab_uses_noncanonical_black(bus, current_ctab_handle);
                let ctab_handle = if use_raw_current_ctab {
                    current_ctab_handle
                } else {
                    port_ctab_handle
                };
                Some(if is_screen_port {
                    *self.device_clut
                } else if use_raw_current_ctab {
                    self.read_ctab_handle_clut(bus, ctab_handle)
                } else {
                    self.read_port_clut(bus, ctab_handle)
                })
            } else {
                None
            };

        for y in r.top..r.bottom {
            if y < clip_top || y >= clip_bottom {
                continue;
            }
            let dy = (y - bounds_top) as u32;
            for x in r.left..r.right {
                if x < clip_left || x >= clip_right {
                    continue;
                }
                if vis_region_complex
                    && !Self::region_contains_point_cached(
                        bus,
                        vis_rgn_handle,
                        vis_region_cache.as_ref(),
                        y,
                        x,
                    )
                {
                    continue;
                }
                if clip_region_complex
                    && !Self::region_contains_point_cached(
                        bus,
                        clip_rgn_handle,
                        clip_region_cache.as_ref(),
                        y,
                        x,
                    )
                {
                    continue;
                }
                // Inside Macintosh I, "The GrafPort" (QuickDraw): portBits.bounds
                // establishes local coordinates; visRgn and clipRgn limit drawing.
                // Feed only these already-clipped cells to the presentation plane.
                if pixel_size == 8 && matches!(op, ShapeOp::Glyph(0 | 1)) {
                    if let Some(p) = &mut bus.presentation {
                        let dx = (x - bounds_left) as u32;
                        if dx < pix_row_bytes {
                            p.glyph_pixel(pix_base + dy * pix_row_bytes + dx, x, y, fg_idx);
                        }
                    }
                }
                let alpha = coverage_at(y, x);
                if alpha == 0 {
                    continue;
                }
                // 1bpp and 32bpp targets can't represent partial coverage,
                // so reduce antialiased edges back to a binary decision at
                // the 50% threshold. Only 8bpp runs the full alpha path.
                if pixel_size != 8 && alpha < MONO_COVERAGE_THRESHOLD {
                    continue;
                }
                let dx = (x - bounds_left) as u32;

                if pixel_size == 8 {
                    let byte_offset = dy * pix_row_bytes + dx;
                    if dx >= pix_row_bytes {
                        continue;
                    }
                    let addr = pix_base + byte_offset;
                    if let (Some(pixpat), Some(dst_clut)) =
                        (installed_raw_pixpat.as_ref(), indexed_clut.as_ref())
                    {
                        if let Some(source_index) = Self::raw_pixpat_index_at(bus, pixpat, y, x) {
                            let rgb = pixpat.clut[usize::from(source_index)];
                            bus.write_byte(
                                addr,
                                shape_palette_index_for_rgb(rgb, pixel_size, dst_clut),
                            );
                        }
                        continue;
                    }
                    match op {
                        ShapeOp::Paint | ShapeOp::Frame => {
                            let source_is_black = effective_pn_pat[y.rem_euclid(8) as usize]
                                & (1 << (7 - x.rem_euclid(8)))
                                != 0;
                            let old = bus.read_byte(addr);
                            let new = apply_boolean_transfer_8(
                                old,
                                self.pn_mode,
                                source_is_black,
                                fg_idx,
                                bg_idx,
                            );
                            bus.write_byte(addr, new);
                        }
                        ShapeOp::Erase => {
                            let source_is_black = effective_bk_pat[y.rem_euclid(8) as usize]
                                & (1 << (7 - x.rem_euclid(8)))
                                != 0;
                            let old = bus.read_byte(addr);
                            let new =
                                apply_boolean_transfer_8(old, 0, source_is_black, fg_idx, bg_idx);
                            bus.write_byte(addr, new);
                        }
                        ShapeOp::Fill(ref p) => {
                            let source_is_black =
                                p[y.rem_euclid(8) as usize] & (1 << (7 - x.rem_euclid(8))) != 0;
                            let old = bus.read_byte(addr);
                            let new =
                                apply_boolean_transfer_8(old, 0, source_is_black, fg_idx, bg_idx);
                            bus.write_byte(addr, new);
                        }
                        ShapeOp::Glyph(mode) => {
                            // systemless bitmap glyphs emit exclusively
                            // {0, 255} coverage (binary mask from the
                            // const decoder). Any non-zero alpha is a fully-set
                            // pixel; route through the normal boolean
                            // transfer mode to keep srcCopy / srcOr /
                            // srcXor / srcBic semantics.
                            if alpha < MONO_COVERAGE_THRESHOLD {
                                continue;
                            }
                            let old = bus.read_byte(addr);
                            let new = apply_boolean_transfer_8(old, mode, true, fg_idx, bg_idx);
                            bus.write_byte(addr, new);
                        }
                        ShapeOp::Invert => {
                            bus.write_byte(addr, invert_indexed_pixel(bus.read_byte(addr)))
                        }
                    }
                } else if matches!(pixel_size, 2 | 4) {
                    let bits = u32::from(pixel_size);
                    let pixels_per_byte = 8 / bits;
                    let byte_col = dx / pixels_per_byte;
                    if byte_col >= pix_row_bytes {
                        continue;
                    }
                    let shift = 8 - bits - (dx % pixels_per_byte) * bits;
                    let index_mask = ((1u16 << pixel_size) - 1) as u8;
                    let addr = pix_base + dy * pix_row_bytes + byte_col;
                    let byte = bus.read_byte(addr);
                    let old = (byte >> shift) & index_mask;
                    let new = match op {
                        ShapeOp::Paint | ShapeOp::Frame => {
                            let source_is_black = effective_pn_pat[y.rem_euclid(8) as usize]
                                & (1 << (7 - x.rem_euclid(8)))
                                != 0;
                            apply_boolean_transfer_8(
                                old,
                                self.pn_mode,
                                source_is_black,
                                fg_idx,
                                bg_idx,
                            )
                        }
                        ShapeOp::Erase => {
                            let source_is_black = effective_bk_pat[y.rem_euclid(8) as usize]
                                & (1 << (7 - x.rem_euclid(8)))
                                != 0;
                            apply_boolean_transfer_8(old, 0, source_is_black, fg_idx, bg_idx)
                        }
                        ShapeOp::Fill(ref p) => {
                            let source_is_black =
                                p[y.rem_euclid(8) as usize] & (1 << (7 - x.rem_euclid(8))) != 0;
                            apply_boolean_transfer_8(old, 0, source_is_black, fg_idx, bg_idx)
                        }
                        ShapeOp::Glyph(mode) => {
                            apply_boolean_transfer_8(old, mode, true, fg_idx, bg_idx)
                        }
                        ShapeOp::Invert => !old,
                    } & index_mask;
                    let field_mask = index_mask << shift;
                    bus.write_byte(addr, (byte & !field_mask) | (new << shift));
                } else if pixel_size == 32 {
                    let byte_offset = dy * pix_row_bytes + dx * 4;
                    if byte_offset + 4 > (dy + 1) * pix_row_bytes {
                        continue;
                    }
                    let addr = pix_base + byte_offset;
                    let old = bus.read_long(addr) & 0x00FF_FFFF;
                    let (fg_r, fg_g, fg_b) = effective_fg_color;
                    let fg_color =
                        ((fg_r as u32 >> 8) << 16) | ((fg_g as u32 >> 8) << 8) | (fg_b as u32 >> 8);
                    let (bg_r, bg_g, bg_b) = effective_bg_color;
                    let bg_color =
                        ((bg_r as u32 >> 8) << 16) | ((bg_g as u32 >> 8) << 8) | (bg_b as u32 >> 8);
                    let color = match op {
                        ShapeOp::Paint | ShapeOp::Frame => {
                            let source_is_black = effective_pn_pat[y.rem_euclid(8) as usize]
                                & (1 << (7 - x.rem_euclid(8)))
                                != 0;
                            apply_boolean_transfer_32(
                                old,
                                self.pn_mode,
                                source_is_black,
                                fg_color,
                                bg_color,
                            )
                        }
                        ShapeOp::Erase => {
                            let source_is_black = effective_bk_pat[y.rem_euclid(8) as usize]
                                & (1 << (7 - x.rem_euclid(8)))
                                != 0;
                            apply_boolean_transfer_32(old, 0, source_is_black, fg_color, bg_color)
                        }
                        ShapeOp::Fill(ref p) => {
                            let source_is_black =
                                p[y.rem_euclid(8) as usize] & (1 << (7 - x.rem_euclid(8))) != 0;
                            apply_boolean_transfer_32(old, 0, source_is_black, fg_color, bg_color)
                        }
                        ShapeOp::Glyph(mode) => {
                            apply_boolean_transfer_32(old, mode, true, fg_color, bg_color)
                        }
                        ShapeOp::Invert => !old & 0x00FF_FFFF,
                    };
                    bus.write_long(addr, color);
                } else if pixel_size == 1 {
                    let byte_offset = dy * pix_row_bytes + (dx / 8);
                    if (dx / 8) >= pix_row_bytes {
                        continue;
                    }

                    let bit = 7 - (dx % 8);
                    let addr = pix_base + byte_offset;
                    let b = bus.read_byte(addr);
                    let old = b & (1 << bit) != 0;
                    let (mode, source_is_black) = match op {
                        ShapeOp::Paint | ShapeOp::Frame => (
                            self.pn_mode,
                            self.pn_pat[y.rem_euclid(8) as usize] & (1 << (7 - x.rem_euclid(8)))
                                != 0,
                        ),
                        ShapeOp::Erase => (
                            0,
                            self.bk_pat[y.rem_euclid(8) as usize] & (1 << (7 - x.rem_euclid(8)))
                                != 0,
                        ),
                        ShapeOp::Fill(ref p) => (
                            0,
                            p[y.rem_euclid(8) as usize] & (1 << (7 - x.rem_euclid(8))) != 0,
                        ),
                        ShapeOp::Glyph(mode) => (mode, true),
                        ShapeOp::Invert => (2, true),
                    };
                    let val = apply_boolean_transfer_1(old, mode, source_is_black);

                    if val {
                        bus.write_byte(addr, b | (1 << bit));
                    } else {
                        bus.write_byte(addr, b & !(1 << bit));
                    }
                }
            }
        }
        if touched_screen {
            let screen_rect = (
                touch_top.saturating_sub(bounds_top),
                touch_left.saturating_sub(bounds_left),
                touch_bottom.saturating_sub(bounds_top),
                touch_right.saturating_sub(bounds_left),
            );
            self.refresh_dialog_saved_pixels_after_screen_draw(bus, port, screen_rect);
            self.refresh_visible_dialog_snapshot_region_for_port(bus, port, screen_rect);
        }

        if trace_menu_redraw_enabled() {
            for (label, probe_y, probe_x) in trace_menu_probe_points() {
                if trace_menu_rect_contains_point(
                    r.top, r.left, r.bottom, r.right, probe_y, probe_x,
                ) && pixel_size == 8
                {
                    let row = (probe_y - bounds_top) as u32;
                    let col = (probe_x - bounds_left) as u32;
                    let pixel = bus.read_byte(pix_base + row * pix_row_bytes + col);
                    eprintln!("[MENU-REDRAW] Shape probe={} pixel={}", label, pixel);
                }
            }
        }
    }

    #[inline]
    fn solid_src_copy_fill_index(
        &self,
        op: &ShapeOp,
        fg_idx: u8,
        bg_idx: u8,
        pn_pat: [u8; 8],
        bk_pat: [u8; 8],
    ) -> Option<u8> {
        let pattern = match op {
            ShapeOp::Paint if normalize_boolean_transfer_mode(self.pn_mode) == 0 => pn_pat,
            ShapeOp::Erase => bk_pat,
            ShapeOp::Fill(pattern) => *pattern,
            _ => return None,
        };
        if pattern == [0xFF; 8] {
            Some(fg_idx)
        } else if pattern == [0x00; 8] {
            Some(bg_idx)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_boolean_transfer_1, apply_boolean_transfer_8, blend_rgb, ctab_rgb_for_value,
        ctab_uses_noncanonical_black, fg_bg_low_contrast, indexed_shape_color_index,
        lighten_stem_alpha, normalize_boolean_transfer_mode, shape_palette_index_for_rgb,
    };
    use crate::memory::{MacMemoryBus, MemoryBus};

    /// Write a ColorTable at `ctab` (handle at `handle`) with the given
    /// (value, rgb) ColorSpecs; `device` sets ctFlags' device bit.
    fn write_ctab(
        bus: &mut MacMemoryBus,
        handle: u32,
        ctab: u32,
        device: bool,
        specs: &[(u16, [u16; 3])],
    ) {
        bus.write_long(handle, ctab);
        bus.write_long(ctab, 0x1234_5678); // ctSeed
        bus.write_word(ctab + 4, if device { 0x8000 } else { 0 });
        bus.write_word(ctab + 6, (specs.len() - 1) as u16);
        for (i, (value, rgb)) in specs.iter().enumerate() {
            let entry = ctab + 8 + i as u32 * 8;
            bus.write_word(entry, *value);
            bus.write_word(entry + 2, rgb[0]);
            bus.write_word(entry + 4, rgb[1]);
            bus.write_word(entry + 6, rgb[2]);
        }
    }

    #[test]
    fn ctab_lookups_find_values_by_ordinal_and_by_scan_and_detect_noncanonical_black() {
        let mut bus = MacMemoryBus::new(64 * 1024);
        // Index-addressed table: value == position, black at 255 (canonical).
        let specs: Vec<(u16, [u16; 3])> = (0..=255u16)
            .map(|v| {
                (
                    v,
                    if v == 255 {
                        [0, 0, 0]
                    } else {
                        [v * 100, 0x1000, 0x2000]
                    },
                )
            })
            .collect();
        write_ctab(&mut bus, 0x1000, 0x2000, false, &specs);
        assert_eq!(
            ctab_rgb_for_value(&bus, 0x1000, 7),
            Some([700, 0x1000, 0x2000])
        );
        assert_eq!(ctab_rgb_for_value(&bus, 0x1000, 255), Some([0, 0, 0]));
        assert!(!ctab_uses_noncanonical_black(&bus, 0x1000));

        // Device table with client IDs in `value` (out of order): the ordinal
        // shortcut misses and the scan must find them; black lives at value 1.
        let specs = [
            (5u16, [0x0100u16, 0x0200, 0x0300]),
            (1, [0, 0, 0]),
            (255, [0xFFFF, 0xFFFF, 0xFFFF]),
            (2, [0x0400, 0x0500, 0x0600]),
        ];
        write_ctab(&mut bus, 0x3000, 0x4000, true, &specs);
        assert_eq!(
            ctab_rgb_for_value(&bus, 0x3000, 2),
            Some([0x0400, 0x0500, 0x0600])
        );
        assert_eq!(ctab_rgb_for_value(&bus, 0x3000, 1), Some([0, 0, 0]));
        assert_eq!(
            ctab_rgb_for_value(&bus, 0x3000, 255),
            Some([0xFFFF, 0xFFFF, 0xFFFF])
        );
        assert_eq!(ctab_rgb_for_value(&bus, 0x3000, 9), None);
        assert!(ctab_uses_noncanonical_black(&bus, 0x3000));

        // Null handle / null table pointer.
        assert_eq!(ctab_rgb_for_value(&bus, 0, 1), None);
        bus.write_long(0x5000, 0);
        assert_eq!(ctab_rgb_for_value(&bus, 0x5000, 1), None);
        assert!(!ctab_uses_noncanonical_black(&bus, 0x5000));
    }

    #[test]
    fn standard_4bit_gworld_shape_colors_use_the_rom_inverse_table() {
        let clut = crate::trap::TrapDispatcher::standard_mac_4bpp_gworld_clut();

        assert_eq!(
            shape_palette_index_for_rgb([0x6666, 0xFFFF, 0xFFFF], 4, &clut),
            0
        );
        assert_eq!(
            crate::trap::pict::closest_clut_index(0x6666, 0xFFFF, 0xFFFF, &clut),
            12
        );
    }

    #[test]
    fn non_4bit_shape_colors_keep_clut_nearest_matching() {
        let clut = crate::trap::TrapDispatcher::standard_mac_4bpp_gworld_clut();

        assert_eq!(
            shape_palette_index_for_rgb([0x6666, 0xFFFF, 0xFFFF], 8, &clut),
            12
        );
    }

    #[test]
    fn indexed_cgrafport_shapes_honor_the_resolved_port_pixel() {
        let mut clut = [[0, 0, 0]; 256];
        clut[95] = [0x0000, 0x7B60, 0x0000];
        clut[181] = [0xAB90, 0x7C50, 0x9570];

        assert_eq!(
            indexed_shape_color_index(95, (0xF2D7, 0x0856, 0x84EC), 8, &clut, true, false,),
            95
        );
        assert_eq!(
            indexed_shape_color_index(95, (0xF2D7, 0x0856, 0x84EC), 8, &clut, true, true,),
            181
        );
    }

    #[test]
    fn normalize_boolean_transfer_mode_accepts_pen_and_text_modes() {
        assert_eq!(normalize_boolean_transfer_mode(0), 0);
        assert_eq!(normalize_boolean_transfer_mode(3), 3);
        assert_eq!(normalize_boolean_transfer_mode(8), 0);
        assert_eq!(normalize_boolean_transfer_mode(11), 3);
    }

    #[test]
    fn apply_boolean_transfer_8_pat_bic_uses_background_and_preserves_white_source() {
        assert_eq!(apply_boolean_transfer_8(77, 11, true, 255, 0), 0);
        assert_eq!(apply_boolean_transfer_8(77, 11, false, 255, 0), 77);
    }

    #[test]
    fn apply_boolean_transfer_1_src_xor_inverts_only_black_source_pixels() {
        assert!(!apply_boolean_transfer_1(true, 2, true));
        assert!(apply_boolean_transfer_1(true, 2, false));
    }

    // Lock the anti-aliased glyph blend contract. Partial glyph coverage
    // blends fg → bg linearly in 16-bit Mac RGB space; the ShapeOp::Glyph
    // pixel-write then calls closest_clut_index on the result.

    #[test]
    fn blend_rgb_returns_fg_at_full_alpha() {
        let fg = (0xFFFFu16, 0x8000u16, 0x0000u16);
        let bg = (0x0000u16, 0x0000u16, 0xFFFFu16);
        assert_eq!(blend_rgb(fg, bg, 255), fg);
    }

    #[test]
    fn blend_rgb_returns_bg_at_zero_alpha() {
        let fg = (0xFFFFu16, 0x8000u16, 0x0000u16);
        let bg = (0x0000u16, 0x0000u16, 0xFFFFu16);
        assert_eq!(blend_rgb(fg, bg, 0), bg);
    }

    #[test]
    fn blend_rgb_mid_alpha_lerps_each_channel() {
        // alpha=128 is just over 50% (128/255 ≈ 0.502); each channel
        // should land within a few rounding units of the midpoint.
        let fg = (0xFFFFu16, 0xFFFFu16, 0xFFFFu16);
        let bg = (0x0000u16, 0x0000u16, 0x0000u16);
        let mid = blend_rgb(fg, bg, 128);
        // expected = round(0xFFFF * 128 / 255) ≈ 0x8080
        for channel in [mid.0, mid.1, mid.2] {
            assert!(
                (0x7F00..=0x8100).contains(&channel),
                "mid-alpha channel {channel:#06X} should be ~0x8080"
            );
        }
    }

    // Lock the stem-lightening curve. A straight pass-through (or any
    // other curve shape) would silently re-thicken antialiased edges.

    #[test]
    fn lighten_stem_alpha_endpoints_passthrough() {
        // Fully-on and fully-off pixels must not be modified —
        // the curve only fades the edges, not the stem cores.
        assert_eq!(lighten_stem_alpha(0), 0);
        assert_eq!(lighten_stem_alpha(255), 255);
    }

    #[test]
    fn lighten_stem_alpha_quadratic_shape() {
        // Curve is `out = in² / 255` with rounding. Worked examples:
        // in=64 → 16, in=128 → 64, in=192 → 144.
        assert_eq!(lighten_stem_alpha(64), 16);
        assert_eq!(lighten_stem_alpha(128), 64);
        assert_eq!(lighten_stem_alpha(192), 145);
    }

    // Lock the low-contrast detection contract.

    #[test]
    fn fg_bg_low_contrast_white_on_black_is_high_contrast() {
        // Body text case: white text on black background. MUST stay
        // antialiased.
        assert!(!fg_bg_low_contrast(
            (0xFFFF, 0xFFFF, 0xFFFF),
            (0x0000, 0x0000, 0x0000)
        ));
        assert!(!fg_bg_low_contrast(
            (0x0000, 0x0000, 0x0000),
            (0xFFFF, 0xFFFF, 0xFFFF)
        ));
    }

    #[test]
    fn fg_bg_low_contrast_light_green_on_dark_green_is_low_contrast() {
        // Light green text on dark green panel. MUST be detected as
        // low-contrast so antialiasing is skipped.
        assert!(fg_bg_low_contrast(
            (0x0000, 0xFFFF, 0x0000),
            (0x0000, 0x4000, 0x0000)
        ));
        // Brighter HUD text on darker panel — still same dominant
        // green channel, must trigger the skip.
        assert!(fg_bg_low_contrast(
            (0x4000, 0xFFFF, 0x4000),
            (0x0000, 0x6000, 0x0000)
        ));
    }

    #[test]
    fn fg_bg_low_contrast_two_grays_is_high_contrast() {
        // Gray-on-gray (e.g. dark gray text on lighter gray dialog
        // background) — CLUT gray ramp handles intermediate luminance
        // values cleanly, so antialiasing IS safe here. Must NOT
        // trigger the skip path.
        assert!(!fg_bg_low_contrast(
            (0x4000, 0x4000, 0x4000),
            (0x6000, 0x6000, 0x6000)
        ));
    }

    #[test]
    fn fg_bg_low_contrast_blue_on_yellow_is_high_contrast() {
        // Different-dominant-channel + high luminance separation:
        // should KEEP antialiasing (the canonical case it benefits).
        assert!(!fg_bg_low_contrast(
            (0x0000, 0x0000, 0xFFFF),
            (0xFFFF, 0xFFFF, 0x0000)
        ));
    }

    #[test]
    fn lighten_stem_alpha_monotone() {
        // Higher input must produce higher-or-equal output — the
        // curve must preserve relative coverage ordering so glyph
        // edges still grade smoothly from off to on.
        let mut prev = 0u8;
        for a in 0..=255u8 {
            let out = lighten_stem_alpha(a);
            assert!(
                out >= prev,
                "lighten_stem_alpha non-monotone at a={a}: prev={prev} out={out}"
            );
            prev = out;
        }
    }

    #[test]
    fn blend_rgb_per_channel_independence() {
        // A blend step on one channel must not leak into the others.
        // Uses asymmetric fg/bg where each channel has a different
        // gradient so a rogue "blend all channels together" bug
        // would produce visibly wrong results.
        let fg = (0xFFFFu16, 0x0000u16, 0x8000u16);
        let bg = (0x0000u16, 0xFFFFu16, 0x4000u16);
        let blended = blend_rgb(fg, bg, 64); // ~25% fg
                                             // R: 25% of FFFF ≈ 0x4000
                                             // G: 75% of FFFF ≈ 0xC000
                                             // B: 25% of 8000 + 75% of 4000 = 0x2000 + 0x3000 = 0x5000
        assert!((0x3F00..=0x4100).contains(&blended.0));
        assert!((0xBF00..=0xC100).contains(&blended.1));
        assert!((0x4F00..=0x5100).contains(&blended.2));
    }
}
