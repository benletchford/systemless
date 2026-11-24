//! Painting prepared rows into a pixel image.
//!
//! The caller (one adapter per CPU route) resolves the port, the colours
//! and the clip, and asks [`supports`] before preparing anything. The core
//! clips, expands the pattern and writes rows through [`RasterMemory`]; it
//! never decodes guest structures itself.

use super::memory::RasterMemory;
use super::region_rows::intersect_rows;
use super::transfer::{apply_bit_row_op, apply_boolean_row_8, BitRowOp, PatCopyRows};
use crate::trap::types::Rect;

#[cfg(test)]
mod tests;

/// Bits per pixel of a pixel image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Depth {
    D1,
    D2,
    D4,
    D8,
    D16,
    D32,
}

impl Depth {
    pub(crate) fn from_bits(bits: u32) -> Option<Self> {
        Some(match bits {
            1 => Self::D1,
            2 => Self::D2,
            4 => Self::D4,
            8 => Self::D8,
            16 => Self::D16,
            32 => Self::D32,
            _ => return None,
        })
    }

    /// Bytes per pixel at the depths [`paint_rect`] writes whole bytes for.
    fn whole_byte_pixel_len(self) -> Option<usize> {
        match self {
            Self::D8 => Some(1),
            Self::D16 => Some(2),
            _ => None,
        }
    }
}

/// Pattern transfer modes (Imaging With QuickDraw (1994), pp. 3-9--3-10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferMode {
    PatCopy,
    PatOr,
    PatXor,
    PatBic,
    NotPatCopy,
    NotPatOr,
    NotPatXor,
    NotPatBic,
}

impl TransferMode {
    /// Normalizes a QuickDraw source (`0..=7`) or pattern (`8..=15`) boolean
    /// transfer mode; out-of-range values fall back to `PatCopy`.
    #[inline]
    pub(crate) fn from_quickdraw_mode(mode: i16) -> Self {
        let normalized = match mode {
            0..=7 => mode,
            8..=15 => mode - 8,
            _ => 0,
        };
        match normalized {
            0 => Self::PatCopy,
            1 => Self::PatOr,
            2 => Self::PatXor,
            3 => Self::PatBic,
            4 => Self::NotPatCopy,
            5 => Self::NotPatOr,
            6 => Self::NotPatXor,
            _ => Self::NotPatBic,
        }
    }

    /// Whether the mode inverts the source pattern bits before applying the
    /// boolean operation (`notPat*`).
    #[inline]
    pub(crate) fn inverts_source(self) -> bool {
        matches!(
            self,
            Self::NotPatCopy | Self::NotPatOr | Self::NotPatXor | Self::NotPatBic
        )
    }

    /// Whether the mode replaces every covered destination pixel without
    /// reading the old pixel value (`patCopy` / `notPatCopy`).
    #[inline]
    pub(crate) fn is_copy(self) -> bool {
        matches!(self, Self::PatCopy | Self::NotPatCopy)
    }
}

/// The kind of pattern a draw uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Pixel patterns are declined until a route can decode them.
pub(crate) enum PatternKind {
    /// An 8×8 bit pattern drawn with the foreground and background pixels.
    Mono,
    /// A PixPat's own pixel image.
    Pixel,
}

/// Whether the core can draw this combination. Adapters ask before building
/// clip rows or resolving colours, so a declined draw costs the legacy path
/// nothing extra.
#[inline]
pub(crate) fn supports(depth: Depth, mode: TransferMode, pattern: PatternKind) -> bool {
    pattern == PatternKind::Mono
        && match depth {
            Depth::D8 => true,
            Depth::D16 => mode.is_copy(),
            _ => false,
        }
}

/// A pixel image in port coordinates: the pixel at `(bounds.left,
/// bounds.top)` is the first one at `base`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Surface {
    pub(crate) base: u32,
    pub(crate) row_bytes: u32,
    pub(crate) depth: Depth,
    pub(crate) bounds: Rect,
}

impl Surface {
    /// `rect` clipped to the pixel image, or `None` when nothing is left.
    #[inline]
    pub(crate) fn clip(&self, rect: Rect) -> Option<Rect> {
        intersect(rect, self.bounds)
    }
}

/// The clip a draw is constrained to, prepared once per draw.
pub(crate) struct ClipRows {
    /// Bounds every row; outside it nothing is drawn.
    bbox: Rect,
    /// For a non-rectangular clip, the covered x endpoints of each row from
    /// `bbox.top`; `None` means the clip is the rectangle `bbox` itself and
    /// costs no allocation.
    rows: Option<Vec<Vec<i16>>>,
}

impl ClipRows {
    pub(crate) const UNBOUNDED: Rect = Rect {
        top: i16::MIN,
        left: i16::MIN,
        bottom: i16::MAX,
        right: i16::MAX,
    };

    pub(crate) fn rectangular(bbox: Rect) -> Self {
        Self { bbox, rows: None }
    }

    /// Covered intervals of `area`, including empty rows outside the clip.
    /// Malformed or oversized endpoint lists are declined without normalization.
    pub(crate) fn clipped_rows(&self, area: Rect) -> Option<Vec<Vec<i16>>> {
        let mut endpoints = 0usize;
        let mut work = 0usize;
        (i32::from(area.top)..i32::from(area.bottom))
            .map(|y| {
                let left = area.left.max(self.bbox.left);
                let right = area.right.min(self.bbox.right);
                if y < i32::from(self.bbox.top) || y >= i32::from(self.bbox.bottom) || left >= right
                {
                    return Some(vec![]);
                }
                let row = if let Some(rows) = &self.rows {
                    let Some(row) = rows.get(usize::try_from(y - i32::from(self.bbox.top)).ok()?)
                    else {
                        return Some(vec![]);
                    };
                    work = work.checked_add(row.len())?;
                    if work > 64_000_000 {
                        return None;
                    }
                    if row.len() % 2 != 0 || row.windows(2).any(|p| p[0] >= p[1]) {
                        return None;
                    }
                    super::region_rows::intersect_rows(&[left, right], row)
                } else {
                    vec![left, right]
                };
                endpoints = endpoints.checked_add(row.len())?;
                if endpoints > 1_048_576 {
                    return None;
                }
                Some(row)
            })
            .collect()
    }

    /// Row `i` of `rows` lists the covered x endpoints of scanline
    /// `top + i`, as `region_rows` reads them; scanlines past the last row
    /// are clipped away.
    pub(crate) fn from_rows(top: i16, rows: Vec<Vec<i16>>) -> Self {
        let bottom = i32::from(top) + rows.len() as i32;
        Self {
            bbox: Rect {
                top,
                left: i16::MIN,
                bottom: bottom.min(i32::from(i16::MAX)) as i16,
                right: i16::MAX,
            },
            rows: Some(rows),
        }
    }

    /// Visits each covered `(y, run_left, run_right)` interval of `area`.
    #[inline(always)]
    fn for_each_run(&self, area: Rect, mut f: impl FnMut(i32, i32, i32)) {
        let (top, bottom) = (i32::from(area.top), i32::from(area.bottom));
        match &self.rows {
            None => {
                let (left, right) = (i32::from(area.left), i32::from(area.right));
                for y in top..bottom {
                    f(y, left, right);
                }
            }
            Some(clip_rows) => {
                let clip_top = i32::from(self.bbox.top);
                for y in top..bottom {
                    let Some(clip_row) = clip_rows.get((y - clip_top) as usize) else {
                        continue;
                    };
                    for run in intersect_rows(&[area.left, area.right], clip_row).chunks_exact(2) {
                        f(y, i32::from(run[0]), i32::from(run[1]));
                    }
                }
            }
        }
    }
}

/// How to colour the covered pixels: already-resolved destination pixel
/// values, an 8×8 bit pattern and the transfer mode.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ink {
    pub(crate) fg: u32,
    pub(crate) bg: u32,
    pub(crate) pattern: [u8; 8],
    pub(crate) mode: TransferMode,
    /// The port point `(h, v)` the pattern's top-left bit is aligned to.
    pub(crate) pat_origin: (i16, i16),
}

pub(crate) enum DrawOutcome {
    /// Drawn; `touched` bounds every pixel written (it may be empty).
    Drawn { touched: Rect },
    /// Nothing was written; the caller draws with its own path.
    Declined,
}

/// A rectangle covering nothing.
pub(crate) const EMPTY_RECT: Rect = Rect {
    top: 0,
    left: 0,
    bottom: 0,
    right: 0,
};

#[inline]
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let out = Rect {
        top: a.top.max(b.top),
        left: a.left.max(b.left),
        bottom: a.bottom.min(b.bottom),
        right: a.right.min(b.right),
    };
    (out.top < out.bottom && out.left < out.right).then_some(out)
}

/// Fills `rect` (port coordinates) on `surface`, clipped to the image and to
/// `clip`, with `ink`. Each scanline writes one run per covered interval:
/// one run for a rectangular clip, one per clip interval otherwise.
pub(crate) fn paint_rect<M: RasterMemory>(
    memory: &mut M,
    surface: &Surface,
    clip: &ClipRows,
    ink: &Ink,
    rect: Rect,
    scratch: &mut Vec<u8>,
) -> DrawOutcome {
    if !supports(surface.depth, ink.mode, PatternKind::Mono) {
        return DrawOutcome::Declined;
    }
    let Some(pixel_len) = surface.depth.whole_byte_pixel_len() else {
        return DrawOutcome::Declined;
    };
    let Some(area) = surface
        .clip(rect)
        .and_then(|area| intersect(area, clip.bbox))
    else {
        return DrawOutcome::Drawn {
            touched: EMPTY_RECT,
        };
    };
    let left = i32::from(area.left);
    let width = (i32::from(area.right) - left) as usize;
    let origin_v = i32::from(ink.pat_origin.1);
    let image_top = i32::from(surface.bounds.top);
    let image_left = i32::from(surface.bounds.left);
    let pixel_address = |y: i32, x: i32| {
        surface
            .base
            .wrapping_add(((y - image_top) as u32).wrapping_mul(surface.row_bytes))
            .wrapping_add(((x - image_left) as u32).wrapping_mul(pixel_len as u32))
    };
    if ink.mode.is_copy() {
        let mut rows = PatCopyRows::new(scratch, ink, left, width, pixel_len);
        clip.for_each_run(area, |y, run_left, run_right| {
            let prepared = rows.row((y - origin_v).rem_euclid(8) as usize);
            let start = (run_left - left) as usize * pixel_len;
            let end = (run_right - left) as usize * pixel_len;
            let _ = memory.write_row(pixel_address(y, run_left), &prepared[start..end]);
        });
    } else {
        debug_assert_eq!(surface.depth, Depth::D8);
        let (fg, bg) = (ink.fg as u8, ink.bg as u8);
        let origin_h = i32::from(ink.pat_origin.0);
        scratch.clear();
        scratch.resize(width, 0);
        clip.for_each_run(area, |y, run_left, run_right| {
            let pat_row = (y - origin_v).rem_euclid(8) as usize;
            let rotate = (run_left - origin_h).rem_euclid(8) as u32;
            let bits = ink.pattern[pat_row].rotate_left(rotate);
            let run_len = (run_right - run_left) as usize;
            let _ = memory.modify_row(pixel_address(y, run_left), &mut scratch[..run_len], |row| {
                apply_boolean_row_8(row, bits, ink.mode, fg, bg);
            });
        });
    }
    DrawOutcome::Drawn { touched: area }
}

/// Applies a uniform 1-bpp whole-row operation (`Set`, `Clear`, or `Toggle`)
/// over `rect` (port coordinates) on a `Depth::D1` `surface`, clipped to the
/// image, `surface.row_bytes * 8`, and `clip`.
pub(crate) fn paint_solid_1bpp_rect<M: RasterMemory>(
    memory: &mut M,
    surface: &Surface,
    clip: &ClipRows,
    rect: Rect,
    op: BitRowOp,
    scratch: &mut Vec<u8>,
) -> DrawOutcome {
    if surface.depth != Depth::D1 {
        return DrawOutcome::Declined;
    }
    let Some(area) = surface
        .clip(rect)
        .and_then(|area| intersect(area, clip.bbox))
    else {
        return DrawOutcome::Drawn {
            touched: EMPTY_RECT,
        };
    };
    let image_top = i32::from(surface.bounds.top);
    let image_left = i32::from(surface.bounds.left);
    let max_bits = surface.row_bytes.saturating_mul(8);
    let first_bit = (i32::from(area.left) - image_left) as u32;
    let end_bit = ((i32::from(area.right) - image_left) as u32).min(max_bits);
    if first_bit >= end_bit {
        return DrawOutcome::Drawn {
            touched: EMPTY_RECT,
        };
    }
    let max_middle = ((end_bit - 1) / 8).saturating_sub(first_bit / 8 + 1) as usize;
    let fill = if op == BitRowOp::Set { 0xFF } else { 0x00 };
    scratch.clear();
    scratch.resize(max_middle, fill);
    clip.for_each_run(area, |y, run_left, run_right| {
        let run_first = (run_left - image_left) as u32;
        let run_end = ((run_right - image_left) as u32).min(max_bits);
        if run_first < run_end {
            let row_base = surface
                .base
                .wrapping_add(((y - image_top) as u32).wrapping_mul(surface.row_bytes));
            let _ = apply_bit_row_op(memory, row_base, run_first, run_end, op, scratch);
        }
    });
    DrawOutcome::Drawn { touched: area }
}
