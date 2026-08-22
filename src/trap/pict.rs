//! PICT v1/v2 picture parser and renderer.
//!
//! Handles DrawPicture by parsing the PICT opcode stream and rendering
//! bitmap data to the framebuffer. Supports PackBitsRect (0x0098) and
//! DirectBitsRect (0x009A) for game artwork.

use crate::memory::{MacMemoryBus, MemoryBus};

pub(crate) type DstClipRect = (i32, i32, i32, i32); // top, left, bottom, right in dst pixels

#[derive(Clone, Debug)]
pub(crate) struct DstClipRegion {
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
    rows: Option<Vec<Vec<i32>>>,
}

impl DstClipRegion {
    pub(crate) fn rectangular(top: i32, left: i32, bottom: i32, right: i32) -> Self {
        Self {
            top,
            left,
            bottom,
            right,
            rows: None,
        }
    }

    pub(crate) fn complex(
        top: i32,
        left: i32,
        bottom: i32,
        right: i32,
        rows: Vec<Vec<i32>>,
    ) -> Self {
        Self {
            top,
            left,
            bottom,
            right,
            rows: Some(rows),
        }
    }

    fn contains(&self, y: i32, x: i32) -> bool {
        if y < self.top || y >= self.bottom || x < self.left || x >= self.right {
            return false;
        }
        let Some(rows) = self.rows.as_ref() else {
            return true;
        };
        let row_index = (y - self.top) as usize;
        let Some(row) = rows.get(row_index) else {
            return false;
        };
        let mut in_region = false;
        for &edge in row {
            if edge > x {
                break;
            }
            in_region = !in_region;
        }
        in_region
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DstClip {
    rect: DstClipRect,
    regions: Vec<DstClipRegion>,
}

impl DstClip {
    pub(crate) fn new(rect: DstClipRect, regions: Vec<DstClipRegion>) -> Self {
        Self { rect, regions }
    }

    pub(crate) fn rect(&self) -> DstClipRect {
        self.rect
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        let (top, left, bottom, right) = self.rect;
        if y < top || y >= bottom || x < left || x >= right {
            return false;
        }
        self.regions.iter().all(|region| region.contains(y, x))
    }
}

use std::sync::{Mutex, OnceLock};
static TRACE_PICT: OnceLock<bool> = OnceLock::new();
static TRACE_PICT_PALETTE: OnceLock<bool> = OnceLock::new();
static TRACE_PICT_SAMPLES: OnceLock<bool> = OnceLock::new();
static CLUT_MATCH_ITABLE: OnceLock<bool> = OnceLock::new();
static CLUT_MATCH_LEGACY_GRAY: OnceLock<bool> = OnceLock::new();
static PICT_IDENTITY_REMAP: OnceLock<bool> = OnceLock::new();
static SRC_TO_DST_TABLE_CACHE: OnceLock<Mutex<Vec<SrcToDstTableCacheEntry>>> = OnceLock::new();
static STANDARD_MAC_8BPP_CLUT: OnceLock<[[u16; 3]; 256]> = OnceLock::new();

const SRC_TO_DST_TABLE_CACHE_LIMIT: usize = 16;

#[derive(Clone)]
struct SrcToDstTableCacheEntry {
    src_clut: Vec<[u16; 3]>,
    dst_clut: [[u16; 3]; 256],
    table: [u8; 256],
}

fn trace_pict_enabled() -> bool {
    *TRACE_PICT.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_PICT").is_some())
}

fn trace_pict_palette_enabled() -> bool {
    *TRACE_PICT_PALETTE.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_PICT_PALETTE").is_some())
}

fn trace_pict_samples_enabled() -> bool {
    *TRACE_PICT_SAMPLES.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_PICT_SAMPLES").is_some())
}

fn clut_match_itable_enabled() -> bool {
    *CLUT_MATCH_ITABLE.get_or_init(|| std::env::var_os("SYSTEMLESS_CLUT_MATCH_ITABLE").is_some())
}

fn clut_match_legacy_gray_enabled() -> bool {
    *CLUT_MATCH_LEGACY_GRAY
        .get_or_init(|| std::env::var_os("SYSTEMLESS_CLUT_MATCH_LEGACY_GRAY").is_some())
}

fn pict_identity_remap_enabled() -> bool {
    *PICT_IDENTITY_REMAP
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PICT_IDENTITY_REMAP").is_some())
}

fn align_pict_pos(pos: u32) -> u32 {
    if pos.is_multiple_of(2) {
        pos
    } else {
        pos + 1
    }
}

fn checked_skip_pict_data(pos: u32, length_field_size: u32, data_len: u32) -> Option<u32> {
    let next = pos.checked_add(length_field_size)?.checked_add(data_len)?;
    next.checked_add(next & 1)
}

fn skip_reserved_v1_opcode(bus: &MacMemoryBus, opcode: u16, pos: u32) -> Option<u32> {
    match opcode {
        0x35..=0x37 | 0x45..=0x47 | 0x55..=0x57 => Some(pos + 8),
        0x3D..=0x3F | 0x4D..=0x4F | 0x5D..=0x5F | 0x7D..=0x7F | 0x8D..=0x8F => Some(pos),
        0x65..=0x67 => Some(pos + 12),
        0x6D..=0x6F => Some(pos + 4),
        0x75..=0x77 | 0x85..=0x87 => Some(pos + u32::from(bus.read_word(pos))),
        _ => None,
    }
}

const REGION_HEADER_SIZE: u32 = 10;
const REGION_STOP: i16 = i16::MAX;

struct PictureRegion {
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
    rows: Vec<Vec<i16>>,
}

impl PictureRegion {
    fn contains(&self, y: i32, x: i32) -> bool {
        if y < i32::from(self.top)
            || y >= i32::from(self.bottom)
            || x < i32::from(self.left)
            || x >= i32::from(self.right)
        {
            return false;
        }

        if self.rows.is_empty() {
            return true;
        }

        let row_index = (y - i32::from(self.top)) as usize;
        let Some(row) = self.rows.get(row_index) else {
            return false;
        };

        let mut in_region = false;
        for &edge in row {
            if i32::from(edge) > x {
                break;
            }
            in_region = !in_region;
        }
        in_region
    }
}

fn merge_region_endpoints(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
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

fn parse_picture_region(bus: &MacMemoryBus, region_ptr: u32) -> Option<PictureRegion> {
    let region_size = u32::from(bus.read_word(region_ptr));
    if region_size < REGION_HEADER_SIZE {
        return None;
    }

    let top = bus.read_word(region_ptr + 2) as i16;
    let left = bus.read_word(region_ptr + 4) as i16;
    let bottom = bus.read_word(region_ptr + 6) as i16;
    let right = bus.read_word(region_ptr + 8) as i16;
    if bottom <= top || right <= left {
        return None;
    }

    if region_size == REGION_HEADER_SIZE {
        return Some(PictureRegion {
            top,
            left,
            bottom,
            right,
            rows: Vec::new(),
        });
    }

    let region_end = region_ptr + region_size;
    let mut cursor = region_ptr + REGION_HEADER_SIZE;
    if cursor + 2 > region_end {
        return None;
    }

    let mut next_change_y = bus.read_word(cursor) as i16;
    cursor += 2;
    let mut active = Vec::new();
    let mut rows = Vec::with_capacity((bottom - top) as usize);

    for y in top..bottom {
        while next_change_y != REGION_STOP && next_change_y <= y {
            let mut delta = Vec::new();
            loop {
                if cursor + 2 > region_end {
                    return None;
                }
                let value = bus.read_word(cursor) as i16;
                cursor += 2;
                if value == REGION_STOP {
                    break;
                }
                delta.push(value);
            }
            active = merge_region_endpoints(&active, &delta);
            if cursor + 2 > region_end {
                return None;
            }
            next_change_y = bus.read_word(cursor) as i16;
            cursor += 2;
        }
        rows.push(active.clone());
    }

    Some(PictureRegion {
        top,
        left,
        bottom,
        right,
        rows,
    })
}

/// Parse and render a PICT from guest memory.
/// `pic_ptr` points to the Picture record (picSize + picFrame + opcodes).
/// `dst_rect` is the destination rectangle from DrawPicture parameters.
/// Returns true if rendering was attempted.
pub fn draw_picture(
    bus: &mut MacMemoryBus,
    pic_ptr: u32,
    dst_top: i16,
    dst_left: i16,
    dst_bottom: i16,
    dst_right: i16,
    screen_mode: (u32, u32, u16, u16, u16), // (base, row_bytes, width, height, pixel_size)
    device_clut: &[[u16; 3]; 256],
    device_ct_seed: u32,
    dst_clip: Option<&DstClip>,
) -> (bool, Option<Vec<[u16; 3]>>) {
    if pic_ptr == 0 {
        return (false, None);
    }

    // Read Picture header
    let _pic_size = bus.read_word(pic_ptr) as u32;
    let frame_top = bus.read_word(pic_ptr + 2) as i16;
    let frame_left = bus.read_word(pic_ptr + 4) as i16;
    let frame_bottom = bus.read_word(pic_ptr + 6) as i16;
    let frame_right = bus.read_word(pic_ptr + 8) as i16;

    if trace_pict_enabled() {
        eprintln!(
            "[PICT] draw_picture picPtr=${:08X} picFrame=({},{}..{},{}) dst=({},{}..{},{}) dstBase=${:08X}",
            pic_ptr, frame_top, frame_left, frame_bottom, frame_right,
            dst_top, dst_left, dst_bottom, dst_right, screen_mode.0,
        );
    }

    let frame_w = (frame_right - frame_left) as f64;
    let frame_h = (frame_bottom - frame_top) as f64;
    let dst_w = (dst_right - dst_left) as f64;
    let dst_h = (dst_bottom - dst_top) as f64;

    if frame_w <= 0.0 || frame_h <= 0.0 {
        return (false, None);
    }

    let scale_x = dst_w / frame_w;
    let scale_y = dst_h / frame_h;

    // Start parsing opcodes after the 10-byte header
    let mut pos = pic_ptr + 10;
    let mut opcount = 0;
    let mut last_clut: Option<Vec<[u16; 3]>> = None;
    // Prefer a non-canonical-looking CTab (the "scene palette") over
    // canonical-looking ones. A multi-PICT scene with a custom-palette PICT
    // plus several canonical-palette decorative PICTs would otherwise lose
    // the scene palette since `last_clut` returns the canonical one
    // last-drawn.
    let mut preferred_clut: Option<Vec<[u16; 3]>> = None;
    let mut clip_region: Option<PictureRegion> = None;
    // Most recently seen shape rect for $38-$3C / $48-$4C / $58-$5C
    // (frame/paint/erase/invert/fillSameRect — 0-byte opcodes that reuse
    // the last rect). Imaging With QuickDraw 1994, Appendix A, A-7.
    let mut last_shape_rect: Option<(i16, i16, i16, i16)> = None;
    // Text state tracked through the picture — TxFont (0x03), TxSize
    // (0x0D), and pen position as updated by LongText / DHText / DVText /
    // DHDVText. Default to Geneva 12 like most classic Mac UI.
    let mut pict_font_id: i16 = 3;
    let mut pict_font_size: i16 = 12;
    // Imaging With QuickDraw (1994), Appendix A, pp. A-7--A-8, defines
    // relative text opcodes in terms of the current text position. Keep it
    // distinct from the graphics pen updated by LineFrom/ShortLineFrom;
    // recorded pictures routinely interleave framing lines and DHDVText.
    let mut line_pen_v: i16 = 0;
    let mut line_pen_h: i16 = 0;
    let mut text_pen_v: i16 = 0;
    let mut text_pen_h: i16 = 0;
    // PnSize(0x07) so frameRect / frameOval / frameArc / frame-variants
    // honor thick pens. Default (1, 1) matches QuickDraw initPort.
    let mut pen_size: (i16, i16) = (1, 1);
    // PnPat (0x09) + BkPat (0x02) so paintRect uses the pen pattern and
    // eraseRect uses the background pattern. Defaults patBlack / patWhite.
    let mut pn_pat: [u8; 8] = [0xFF; 8];
    let mut bk_pat: [u8; 8] = [0x00; 8];
    // FillPat (0x0A): PICT fill* use this pattern, NOT the pen pattern.
    // Imaging With QuickDraw 1994, Appendix A, A-7.
    let mut fill_pat: [u8; 8] = [0xFF; 8];
    // PICT FgColor (0x0E) and BkColor (0x0F) tracked as destination CLUT
    // indices. Monochrome BitMap/PixMap sources map 1-bits through the
    // foreground color and 0-bits through the background color; custom
    // application palettes can place logical white somewhere other than 0.
    let (black_idx, white_idx) = if screen_mode.4 == 1 {
        (255, 0)
    } else {
        clut_black_white_indices(device_clut)
    };
    let mut fg_idx: u8 = black_idx;
    let mut bg_idx: u8 = white_idx;
    // TxMode (PICT opcode 0x05). Default srcOr (1) per QuickDraw initPort
    // (IM:I I-171). Used by draw_picture_text to XOR glyph pixels when the
    // PICT sets mode srcXor (2).
    let mut tx_mode: i16 = 1;

    let mut is_v2 = false;

    loop {
        if opcount > 10000 {
            eprintln!("[PICT] Too many opcodes, stopping");
            break;
        }
        opcount += 1;

        // PICT v1: opcodes are single bytes.
        // PICT v2: opcodes are words, word-aligned.
        // Imaging With QuickDraw (1994), Appendix A, pp. A-3, A-18
        if is_v2 && !pos.is_multiple_of(2) {
            pos += 1;
        }

        let opcode: u16 = if is_v2 {
            let op = bus.read_word(pos);
            pos += 2;
            op
        } else {
            let op = bus.read_byte(pos) as u16;
            pos += 1;
            op
        };

        match opcode {
            0x00 => {
                // NOP
            }
            0x01 => {
                // ClipRgn
                let rgn_size = bus.read_word(pos) as u32;
                clip_region = parse_picture_region(bus, pos);
                pos += rgn_size;
            }
            0x02 => {
                // BkPat (8 bytes) — track for eraseRect pattern fill
                bus.read_bytes_into(pos, &mut bk_pat);
                pos += 8;
            }
            0x03 => {
                // TxFont (2 bytes) — track for subsequent text opcodes
                pict_font_id = bus.read_word(pos) as i16;
                pos += 2;
            }
            0x04 => {
                // TxFace (1 byte in v1, 2 in v2)
                pos += if is_v2 { 2 } else { 1 };
            }
            0x05 => {
                // TxMode (2 bytes) — track for draw_picture_text.
                tx_mode = bus.read_word(pos) as i16;
                pos += 2;
            }
            0x06 => {
                // SpExtra (4 bytes)
                pos += 4;
            }
            0x07 => {
                // PnSize(v:word, h:word) — track for frame opcodes
                pen_size = (bus.read_word(pos) as i16, bus.read_word(pos + 2) as i16);
                pos += 4;
            }
            0x08 => {
                // PnMode (2 bytes)
                pos += 2;
            }
            0x09 => {
                // PnPat (8 bytes) — track for paint/frame/fillRect
                bus.read_bytes_into(pos, &mut pn_pat);
                pos += 8;
            }
            0x0A => {
                // FillPat (8 bytes) — track for fill-variant shape opcodes
                // (kind=4: fillRect / fillOval / etc.). Imaging With
                // QuickDraw 1994, A-7.
                bus.read_bytes_into(pos, &mut fill_pat);
                pos += 8;
            }
            0x0B => {
                // OvSize (4 bytes)
                pos += 4;
            }
            0x0C => {
                // Origin (4 bytes)
                pos += 4;
            }
            0x0D => {
                // TxSize (2 bytes) — track for subsequent text opcodes
                pict_font_size = bus.read_word(pos) as i16;
                pos += 2;
            }
            0x0E => {
                // FgColor (4 bytes) — map legacy QD colors into the active
                // destination palette.
                let c = bus.read_long(pos);
                fg_idx = pict_qd_color_to_clut_index(c, fg_idx, black_idx, white_idx);
                pos += 4;
            }
            0x0F => {
                // BkColor (4 bytes) — same mapping as FgColor.
                let c = bus.read_long(pos);
                bg_idx = pict_qd_color_to_clut_index(c, bg_idx, black_idx, white_idx);
                pos += 4;
            }
            0x10 => {
                // TxRatio (8 bytes)
                pos += 8;
            }
            0x11 => {
                // picVersion / VersionOp
                let version = bus.read_byte(pos);
                pos += 1;
                if version == 0x02 {
                    // PICT v2: skip 0xFF padding byte, switch to word opcodes
                    pos += 1;
                    is_v2 = true;
                }
            }
            0x12..=0x14 => {
                // BkPixPat / PnPixPat / FillPixPat.
                pos = skip_pixpat(bus, pos);
            }
            0x15 | 0x16 => {
                // PnLocHFrac / ChExtra.
                pos += 2;
            }
            0x1A => {
                // RGBFgCol (6 bytes) - v2 only. Maps the 48-bit RGB to the
                // closest 8bpp CLUT index.
                let r = bus.read_word(pos);
                let g = bus.read_word(pos + 2);
                let b = bus.read_word(pos + 4);
                pos += 6;
                let clut = super::TrapDispatcher::standard_mac_8bpp_clut();
                fg_idx = closest_clut_index(r, g, b, &clut);
            }
            0x1B => {
                // RGBBkCol (6 bytes) - v2 only. Same mapping.
                let r = bus.read_word(pos);
                let g = bus.read_word(pos + 2);
                let b = bus.read_word(pos + 4);
                pos += 6;
                let clut = super::TrapDispatcher::standard_mac_8bpp_clut();
                bg_idx = closest_clut_index(r, g, b, &clut);
            }
            0x1C => {
                // HiliteMode (0 bytes).
            }
            0x1D => {
                // HiliteColor (6 bytes). Highlight transfer modes are not
                // modelled by this PICT renderer, but the stream must advance.
                pos += 6;
            }
            0x1E => {
                // DefHilite (0 bytes) - v2 only
            }
            0x1F => {
                // OpColor (6 bytes). Only arithmetic transfer modes consult it.
                pos += 6;
            }
            0x20 => {
                // Line: pnLoc(v:word, h:word) + newPt(v:word, h:word).
                // Render via draw_picture_line; pen position updates for
                // follow-on LineFrom / ShortLineFrom opcodes.
                let pn_v = bus.read_word(pos) as i16;
                let pn_h = bus.read_word(pos + 2) as i16;
                let new_v = bus.read_word(pos + 4) as i16;
                let new_h = bus.read_word(pos + 6) as i16;
                pos += 8;
                draw_picture_line(
                    bus,
                    screen_mode,
                    pn_v,
                    pn_h,
                    new_v,
                    new_h,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    clip_region.as_ref(),
                    dst_clip,
                    pen_size,
                    pn_pat,
                    fg_idx,
                );
                line_pen_v = new_v;
                line_pen_h = new_h;
            }
            0x21 => {
                // LineFrom: newPt(v:word, h:word). Draws from current
                // pen to newPt and updates the pen.
                let new_v = bus.read_word(pos) as i16;
                let new_h = bus.read_word(pos + 2) as i16;
                pos += 4;
                draw_picture_line(
                    bus,
                    screen_mode,
                    line_pen_v,
                    line_pen_h,
                    new_v,
                    new_h,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    clip_region.as_ref(),
                    dst_clip,
                    pen_size,
                    pn_pat,
                    fg_idx,
                );
                line_pen_v = new_v;
                line_pen_h = new_h;
            }
            0x22 => {
                // ShortLine: pnLoc(v:word, h:word) + dh(i8) + dv(i8).
                let pn_v = bus.read_word(pos) as i16;
                let pn_h = bus.read_word(pos + 2) as i16;
                let dh = bus.read_byte(pos + 4) as i8 as i16;
                let dv = bus.read_byte(pos + 5) as i8 as i16;
                pos += 6;
                let new_v = pn_v.saturating_add(dv);
                let new_h = pn_h.saturating_add(dh);
                draw_picture_line(
                    bus,
                    screen_mode,
                    pn_v,
                    pn_h,
                    new_v,
                    new_h,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    clip_region.as_ref(),
                    dst_clip,
                    pen_size,
                    pn_pat,
                    fg_idx,
                );
                line_pen_v = new_v;
                line_pen_h = new_h;
            }
            0x23 => {
                // ShortLineFrom: dh(i8) + dv(i8). Draws from current
                // pen + (dh, dv) and updates pen.
                let dh = bus.read_byte(pos) as i8 as i16;
                let dv = bus.read_byte(pos + 1) as i8 as i16;
                pos += 2;
                let new_v = line_pen_v.saturating_add(dv);
                let new_h = line_pen_h.saturating_add(dh);
                draw_picture_line(
                    bus,
                    screen_mode,
                    line_pen_v,
                    line_pen_h,
                    new_v,
                    new_h,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    clip_region.as_ref(),
                    dst_clip,
                    pen_size,
                    pn_pat,
                    fg_idx,
                );
                line_pen_v = new_v;
                line_pen_h = new_h;
            }
            0x28 => {
                // LongText: txLoc(v:word, h:word) + count(1) + text
                text_pen_v = bus.read_word(pos) as i16;
                text_pen_h = bus.read_word(pos + 2) as i16;
                pos += 4;
                let len = bus.read_byte(pos) as u32;
                pos += 1;
                let text_start = pos;
                pos += len;
                draw_picture_text(
                    bus,
                    screen_mode,
                    text_pen_v,
                    text_pen_h,
                    text_start,
                    len,
                    pict_font_id,
                    pict_font_size,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    clip_region.as_ref(),
                    dst_clip,
                    fg_idx,
                    bg_idx,
                    tx_mode,
                );
                text_pen_h = text_pen_h.saturating_add(text_advance(
                    bus,
                    text_start,
                    len,
                    pict_font_id,
                    pict_font_size,
                ));
                if is_v2 && !(1 + len).is_multiple_of(2) {
                    pos += 1;
                }
            }
            0x29 => {
                // DHText: dh(1) + count(1) + text
                let dh = bus.read_byte(pos) as i8 as i16;
                pos += 1;
                let len = bus.read_byte(pos) as u32;
                pos += 1;
                let text_start = pos;
                pos += len;
                text_pen_h = text_pen_h.saturating_add(dh);
                draw_picture_text(
                    bus,
                    screen_mode,
                    text_pen_v,
                    text_pen_h,
                    text_start,
                    len,
                    pict_font_id,
                    pict_font_size,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    clip_region.as_ref(),
                    dst_clip,
                    fg_idx,
                    bg_idx,
                    tx_mode,
                );
                text_pen_h = text_pen_h.saturating_add(text_advance(
                    bus,
                    text_start,
                    len,
                    pict_font_id,
                    pict_font_size,
                ));
                if is_v2 && !len.is_multiple_of(2) {
                    pos += 1;
                }
            }
            0x2A => {
                // DVText: dv(1) + count(1) + text
                let dv = bus.read_byte(pos) as i8 as i16;
                pos += 1;
                let len = bus.read_byte(pos) as u32;
                pos += 1;
                let text_start = pos;
                pos += len;
                text_pen_v = text_pen_v.saturating_add(dv);
                draw_picture_text(
                    bus,
                    screen_mode,
                    text_pen_v,
                    text_pen_h,
                    text_start,
                    len,
                    pict_font_id,
                    pict_font_size,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    clip_region.as_ref(),
                    dst_clip,
                    fg_idx,
                    bg_idx,
                    tx_mode,
                );
                text_pen_h = text_pen_h.saturating_add(text_advance(
                    bus,
                    text_start,
                    len,
                    pict_font_id,
                    pict_font_size,
                ));
                if is_v2 && !len.is_multiple_of(2) {
                    pos += 1;
                }
            }
            0x2B => {
                // DHDVText: dh(1) + dv(1) + count(1) + text
                let dh = bus.read_byte(pos) as i8 as i16;
                let dv = bus.read_byte(pos + 1) as i8 as i16;
                pos += 2;
                let len = bus.read_byte(pos) as u32;
                pos += 1;
                let text_start = pos;
                pos += len;
                text_pen_h = text_pen_h.saturating_add(dh);
                text_pen_v = text_pen_v.saturating_add(dv);
                draw_picture_text(
                    bus,
                    screen_mode,
                    text_pen_v,
                    text_pen_h,
                    text_start,
                    len,
                    pict_font_id,
                    pict_font_size,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    clip_region.as_ref(),
                    dst_clip,
                    fg_idx,
                    bg_idx,
                    tx_mode,
                );
                text_pen_h = text_pen_h.saturating_add(text_advance(
                    bus,
                    text_start,
                    len,
                    pict_font_id,
                    pict_font_size,
                ));
                if is_v2 && !(1 + len).is_multiple_of(2) {
                    pos += 1;
                }
            }
            0x2C => {
                // FontName (v2)
                let data_len = bus.read_word(pos) as u32;
                pos += 2 + data_len;
                pos = align_pict_pos(pos);
            }
            0x2D | 0x2E => {
                // lineJustify / glyphState: word data length followed by data.
                let data_len = bus.read_word(pos) as u32;
                pos += 2 + data_len;
                pos = align_pict_pos(pos);
            }
            0x24..=0x27 | 0x2F => {
                // Reserved data-bearing opcodes: word length + data.
                let data_len = bus.read_word(pos) as u32;
                pos += 2 + data_len;
                pos = align_pict_pos(pos);
            }
            // Rectangle drawing opcodes ($30-$34, $38-$3C)
            // Imaging With QuickDraw 1994, Appendix A, A-7
            0x30..=0x34 => {
                // frame/paint/erase/invert/fillRect: Rect(8)
                let (t, l, b, r) = read_shape_rect(bus, pos);
                pos += 8;
                last_shape_rect = Some((t, l, b, r));
                draw_shape_rect(
                    bus,
                    opcode as u8,
                    t,
                    l,
                    b,
                    r,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    screen_mode,
                    clip_region.as_ref(),
                    dst_clip,
                    pen_size,
                    pn_pat,
                    bk_pat,
                    fill_pat,
                    fg_idx,
                    bg_idx,
                );
            }
            0x38..=0x3C => {
                // frameSameRect etc: 0 bytes; reuse the most recent rect
                if let Some((t, l, b, r)) = last_shape_rect {
                    draw_shape_rect(
                        bus,
                        (opcode - 0x38 + 0x30) as u8,
                        t,
                        l,
                        b,
                        r,
                        dst_top,
                        dst_left,
                        frame_top,
                        frame_left,
                        scale_x,
                        scale_y,
                        screen_mode,
                        clip_region.as_ref(),
                        dst_clip,
                        pen_size,
                        pn_pat,
                        bk_pat,
                        fill_pat,
                        fg_idx,
                        bg_idx,
                    );
                }
            }
            // Rounded rectangle opcodes ($40-$44, $48-$4C)
            // Imaging With QuickDraw 1994, Appendix A, A-8
            // (ovSize state is tracked via opcode 0x0B but rounded
            // corners are approximated as plain rects here; fine for
            // games that use frame/paintRRect decoratively.)
            0x40..=0x44 => {
                let (t, l, b, r) = read_shape_rect(bus, pos);
                pos += 8;
                last_shape_rect = Some((t, l, b, r));
                draw_shape_rect(
                    bus,
                    (opcode - 0x40 + 0x30) as u8,
                    t,
                    l,
                    b,
                    r,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    screen_mode,
                    clip_region.as_ref(),
                    dst_clip,
                    pen_size,
                    pn_pat,
                    bk_pat,
                    fill_pat,
                    fg_idx,
                    bg_idx,
                );
            }
            0x48..=0x4C => {
                if let Some((t, l, b, r)) = last_shape_rect {
                    draw_shape_rect(
                        bus,
                        (opcode - 0x48 + 0x30) as u8,
                        t,
                        l,
                        b,
                        r,
                        dst_top,
                        dst_left,
                        frame_top,
                        frame_left,
                        scale_x,
                        scale_y,
                        screen_mode,
                        clip_region.as_ref(),
                        dst_clip,
                        pen_size,
                        pn_pat,
                        bk_pat,
                        fill_pat,
                        fg_idx,
                        bg_idx,
                    );
                }
            }
            // Oval opcodes ($50-$54, $58-$5C)
            // Imaging With QuickDraw 1994, Appendix A, A-9
            0x50..=0x54 => {
                let (t, l, b, r) = read_shape_rect(bus, pos);
                pos += 8;
                last_shape_rect = Some((t, l, b, r));
                draw_shape_oval(
                    bus,
                    (opcode - 0x50) as u8,
                    t,
                    l,
                    b,
                    r,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    screen_mode,
                    clip_region.as_ref(),
                    dst_clip,
                    pen_size,
                    pn_pat,
                    bk_pat,
                    fill_pat,
                    fg_idx,
                    bg_idx,
                );
            }
            0x58..=0x5C => {
                if let Some((t, l, b, r)) = last_shape_rect {
                    draw_shape_oval(
                        bus,
                        (opcode - 0x58) as u8,
                        t,
                        l,
                        b,
                        r,
                        dst_top,
                        dst_left,
                        frame_top,
                        frame_left,
                        scale_x,
                        scale_y,
                        screen_mode,
                        clip_region.as_ref(),
                        dst_clip,
                        pen_size,
                        pn_pat,
                        bk_pat,
                        fill_pat,
                        fg_idx,
                        bg_idx,
                    );
                }
            }
            // Arc opcodes ($60-$64: rect(8)+startAngle(2)+arcAngle(2)=12;
            // $68-$6C: same-rect variants with just the two angles = 4).
            // Mac convention: 0°=north, positive angle sweeps clockwise.
            // Same-rect variants ($68-$6C) reuse last_shape_rect just
            // like $48-$4C for round rects.
            0x60..=0x64 => {
                let (t, l, b, r) = read_shape_rect(bus, pos);
                let start_angle = bus.read_word(pos + 8) as i16;
                let arc_angle = bus.read_word(pos + 10) as i16;
                pos += 12;
                last_shape_rect = Some((t, l, b, r));
                draw_shape_oval_or_arc(
                    bus,
                    (opcode - 0x60) as u8,
                    t,
                    l,
                    b,
                    r,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    screen_mode,
                    clip_region.as_ref(),
                    dst_clip,
                    Some((start_angle, arc_angle)),
                    pen_size,
                    pn_pat,
                    bk_pat,
                    fill_pat,
                    fg_idx,
                    bg_idx,
                );
            }
            0x68..=0x6C => {
                let start_angle = bus.read_word(pos) as i16;
                let arc_angle = bus.read_word(pos + 2) as i16;
                pos += 4;
                if let Some((t, l, b, r)) = last_shape_rect {
                    draw_shape_oval_or_arc(
                        bus,
                        (opcode - 0x68) as u8,
                        t,
                        l,
                        b,
                        r,
                        dst_top,
                        dst_left,
                        frame_top,
                        frame_left,
                        scale_x,
                        scale_y,
                        screen_mode,
                        clip_region.as_ref(),
                        dst_clip,
                        Some((start_angle, arc_angle)),
                        pen_size,
                        pn_pat,
                        bk_pat,
                        fill_pat,
                        fg_idx,
                        bg_idx,
                    );
                }
            }
            // Polygon opcodes ($70-$74: frame/paint/erase/invert/fillPoly).
            // Each has a PolyRec inline: polySize(2) + polyBBox(8) +
            // N*(v,h) vertex pairs.
            0x70..=0x74 => {
                let poly_size = bus.read_word(pos) as u32;
                let poly_ptr = pos;
                pos += poly_size;
                // Pass kind so paint/erase/invert/fill get scanline-filled
                // interior pixels; framePoly (kind=0) uses the edge-draw
                // path. Pass pen_size + pn_pat so framePoly honors PnSize
                // and PnPat.
                render_pict_polygon(
                    bus,
                    poly_ptr,
                    (opcode - 0x70) as u8,
                    pen_size,
                    pn_pat,
                    bk_pat,
                    fill_pat,
                    screen_mode,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    clip_region.as_ref(),
                    dst_clip,
                    fg_idx,
                    bg_idx,
                );
            }
            0x78..=0x7C => {}
            // Region opcodes ($80-$84: frame/paint/erase/invert/fillRgn).
            // Region data = rgnSize(2) + rgnBBox(8) + optional scanlines.
            // Region storage is bbox-only, so the bbox fully describes what
            // we can draw; paint/fill collapse to a solid rect in that
            // bounding box.
            0x80..=0x84 => {
                let rgn_size = bus.read_word(pos) as u32;
                let rgn_top = bus.read_word(pos + 2) as i16;
                let rgn_left = bus.read_word(pos + 4) as i16;
                let rgn_bottom = bus.read_word(pos + 6) as i16;
                let rgn_right = bus.read_word(pos + 8) as i16;
                pos += rgn_size;
                let kind = (opcode - 0x80) as u8; // 0=frame .. 4=fill
                draw_shape_rect(
                    bus,
                    kind,
                    rgn_top,
                    rgn_left,
                    rgn_bottom,
                    rgn_right,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    screen_mode,
                    clip_region.as_ref(),
                    dst_clip,
                    pen_size,
                    pn_pat,
                    bk_pat,
                    fill_pat,
                    fg_idx,
                    bg_idx,
                );
            }
            0x88..=0x8C => {}
            0x90 | 0x91 => {
                // BitsRect / BitsRgn. The rowBytes high bit selects an
                // indexed PixMap instead of a 1-bit BitMap; unlike the
                // $0098/$0099 forms, its pixel data is unpacked.
                // Imaging With QuickDraw 1994, Appendix A, pp. A-13 and A-17.
                if bus.read_word(pos) & 0x8000 != 0 {
                    let (new_pos, clut16) = parse_indexed_bits_rect(
                        bus,
                        pos,
                        opcode == 0x91,
                        false,
                        dst_top,
                        dst_left,
                        frame_top,
                        frame_left,
                        scale_x,
                        scale_y,
                        screen_mode,
                        device_clut,
                        device_ct_seed,
                        fg_idx,
                        bg_idx,
                        clip_region.as_ref(),
                        dst_clip,
                    );
                    pos = new_pos;
                    if preferred_clut.is_none() {
                        preferred_clut = clut16.clone();
                    }
                    last_clut = clut16.or(last_clut);
                } else {
                    pos = parse_bits_rect(
                        bus,
                        pos,
                        opcode == 0x91,
                        dst_top,
                        dst_left,
                        frame_top,
                        frame_left,
                        scale_x,
                        scale_y,
                        screen_mode,
                        device_clut,
                        fg_idx,
                        bg_idx,
                        clip_region.as_ref(),
                        dst_clip,
                    );
                }
            }
            0x98 | 0x99 => {
                // PackBitsRect / PackBitsRgn - packed bitmap
                let (new_pos, clut16) = parse_indexed_bits_rect(
                    bus,
                    pos,
                    opcode == 0x99,
                    true,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    screen_mode,
                    device_clut,
                    device_ct_seed,
                    fg_idx,
                    bg_idx,
                    clip_region.as_ref(),
                    dst_clip,
                );
                pos = new_pos;
                if let Some(ct) = clut16 {
                    // Always track the most-recent CTab as a fallback. Also
                    // track the FIRST CTab seen — that's usually the
                    // scene-defining PICT drawn before any decorative overlay
                    // PICTs inside a multi-PICT DrawPicture stream.
                    if preferred_clut.is_none() {
                        preferred_clut = Some(ct.clone());
                    } else if !clut_resembles_canonical_8bpp(&ct) {
                        // If we see a non-canonical CTab later, prefer
                        // it over an earlier canonical one (covers
                        // PICT streams where canonical helper PICTs
                        // draw first).
                        if let Some(ref existing) = preferred_clut {
                            if clut_resembles_canonical_8bpp(existing) {
                                preferred_clut = Some(ct.clone());
                            }
                        }
                    }
                    last_clut = Some(ct);
                }
            }
            0x9A | 0x9B => {
                // DirectBitsRect / DirectBitsRgn - direct RGB
                pos = parse_direct_bits_rect(
                    bus,
                    pos,
                    opcode == 0x9B,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    screen_mode,
                    device_clut,
                    clip_region.as_ref(),
                    dst_clip,
                );
            }
            0xA0 => {
                // ShortComment (2 bytes)
                pos += 2;
            }
            0xA1 => {
                // LongComment: kind(2) + size(2) + data
                pos += 2;
                let data_len = bus.read_word(pos) as u32;
                pos += 2 + data_len;
                if is_v2 && !data_len.is_multiple_of(2) {
                    pos += 1;
                }
            }
            0xFF => {
                // EndOfPicture (v1: $FF, v2: $00FF)
                break;
            }
            // --- v2-only opcodes below ---
            0x0C00 => {
                // HeaderOp (extended v2) - 24 bytes
                pos += 24;
            }
            0x02FF => {
                // Version (v2, 2 bytes)
                pos += 2;
            }
            0x8200 => {
                // CompressedQuickTime. QuickTime 1993, Table 3-1:
                // fixed opcode header, optional matte/mask data, an
                // ImageDescription, then the compressed image bytes.
                let (new_pos, clut) = parse_compressed_quicktime(
                    bus,
                    pos,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    scale_x,
                    scale_y,
                    screen_mode,
                    device_clut,
                    clip_region.as_ref(),
                    dst_clip,
                );
                pos = new_pos;
                if let Some(ct) = clut {
                    if preferred_clut.is_none()
                        || preferred_clut
                            .as_ref()
                            .is_some_and(|existing| clut_resembles_canonical_8bpp(existing))
                    {
                        preferred_clut = Some(ct.clone());
                    }
                    last_clut = Some(ct);
                }
            }
            _ => {
                if is_v2 {
                    // v2 reserved opcode skip rules
                    if (0x00A2..=0x00AF).contains(&opcode) {
                        let data_len = bus.read_word(pos) as u32;
                        let Some(next) = checked_skip_pict_data(pos, 2, data_len) else {
                            eprintln!(
                                "[PICT] Reserved v2 opcode 0x{opcode:04X} length overflow - stopping"
                            );
                            break;
                        };
                        pos = next;
                    } else if (0x00B0..=0x00CF).contains(&opcode)
                        || (0x8000..=0x80FF).contains(&opcode)
                    {
                        // 0 bytes — both reserved-range blocks have no payload
                    } else if (0x00D0..=0x00FE).contains(&opcode) || opcode >= 0x8100 {
                        let data_len = bus.read_long(pos);
                        let Some(next) = checked_skip_pict_data(pos, 4, data_len) else {
                            eprintln!(
                                "[PICT] Reserved v2 opcode 0x{opcode:04X} length overflow - stopping"
                            );
                            break;
                        };
                        pos = next;
                    } else if (0x0100..=0x7FFF).contains(&opcode) {
                        pos += u32::from(opcode >> 8) * 2;
                    } else {
                        eprintln!(
                            "[PICT] Unknown v2 opcode 0x{:04X} at offset {} - stopping",
                            opcode,
                            pos - 2 - pic_ptr
                        );
                        break;
                    }
                } else {
                    if let Some(new_pos) = skip_reserved_v1_opcode(bus, opcode, pos) {
                        pos = new_pos;
                        continue;
                    }
                    eprintln!(
                        "[PICT] Unknown v1 opcode 0x{:02X} at offset {} - stopping",
                        opcode,
                        pos - 1 - pic_ptr
                    );
                    break;
                }
            }
        }
    }

    // Prefer the first non-canonical CTab if any PackBitsRect emitted
    // one (scene palette); fall back to the last CTab seen otherwise.
    let returned_clut = preferred_clut.or(last_clut);
    (true, returned_clut)
}

#[allow(clippy::too_many_arguments)]
fn parse_compressed_quicktime(
    bus: &mut MacMemoryBus,
    pos: u32,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    screen_mode: (u32, u32, u16, u16, u16),
    device_clut: &[[u16; 3]; 256],
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
) -> (u32, Option<Vec<[u16; 3]>>) {
    let payload_size = bus.read_long(pos);
    let end = align_pict_pos(pos.saturating_add(4).saturating_add(payload_size));
    let mut cursor = pos + 4;
    if payload_size < 68 {
        return (end, None);
    }

    let _version = bus.read_word(cursor);
    cursor += 2;
    cursor += 36; // MatrixRecord
    let matte_size = bus.read_long(cursor);
    cursor += 4;
    cursor += 8; // matteRect
    let _mode = bus.read_word(cursor);
    cursor += 2;
    let src_top = bus.read_word(cursor) as i16;
    let src_left = bus.read_word(cursor + 2) as i16;
    let src_bottom = bus.read_word(cursor + 4) as i16;
    let src_right = bus.read_word(cursor + 6) as i16;
    cursor += 8;
    cursor += 4; // accuracy
    let mask_size = bus.read_long(cursor);
    cursor += 4;

    if matte_size != 0 {
        let matte_description_size = bus.read_long(cursor);
        cursor = cursor
            .saturating_add(matte_description_size)
            .saturating_add(matte_size);
    }
    cursor = cursor.saturating_add(mask_size);
    if cursor.saturating_add(86) > end {
        return (end, None);
    }

    let description_size = bus.read_long(cursor);
    let codec = bus.read_long(cursor + 4);
    let width = usize::from(bus.read_word(cursor + 32));
    let height = usize::from(bus.read_word(cursor + 34));
    let data_size = bus.read_long(cursor + 44);
    let depth = bus.read_word(cursor + 82);

    let mut source_clut = vec![[0u16; 3]; 256];
    if depth <= 8 && description_size >= 94 {
        let mut table = cursor + 86;
        let _seed = bus.read_long(table);
        table += 4;
        let flags = bus.read_word(table);
        table += 2;
        let size = usize::from(bus.read_word(table));
        table += 2;
        for index in 0..=size.min(255) {
            let value = usize::from(bus.read_word(table));
            let destination = if flags & 0x8000 != 0 { index } else { value };
            if destination < source_clut.len() {
                source_clut[destination] = [
                    bus.read_word(table + 2),
                    bus.read_word(table + 4),
                    bus.read_word(table + 6),
                ];
            }
            table += 8;
        }
    }

    if trace_pict_enabled() {
        let codec_bytes = codec.to_be_bytes();
        eprintln!(
            "[PICT] CompressedQuickTime codec='{}' size={}x{} depth={} data={} src=({},{}..{},{})",
            String::from_utf8_lossy(&codec_bytes),
            width,
            height,
            depth,
            data_size,
            src_top,
            src_left,
            src_bottom,
            src_right,
        );
    }

    if codec != u32::from_be_bytes(*b"smc ") || width == 0 || height == 0 {
        return (end, Some(source_clut));
    }
    let data_start = cursor.saturating_add(description_size);
    if data_start.saturating_add(data_size) > end {
        return (end, Some(source_clut));
    }
    let compressed = bus.read_bytes(data_start, data_size as usize);
    let mut decoder = super::smc::SmcDecoder::new(width, height);
    let Ok(pixels) = decoder.decode(&compressed) else {
        return (end, Some(source_clut));
    };
    let color_map = build_src_to_dst_table(&source_clut, device_clut);
    let (screen_base, screen_rb, screen_w, screen_h, pixel_size) = (
        screen_mode.0,
        screen_mode.1,
        i32::from(screen_mode.2),
        i32::from(screen_mode.3),
        screen_mode.4,
    );

    let crop_top = i32::from(src_top).max(0).min(height as i32);
    let crop_left = i32::from(src_left).max(0).min(width as i32);
    let crop_bottom = i32::from(src_bottom).max(crop_top).min(height as i32);
    let crop_right = i32::from(src_right).max(crop_left).min(width as i32);
    for source_y in crop_top..crop_bottom {
        for source_x in crop_left..crop_right {
            if clip_region.is_some_and(|clip| !clip.contains(source_y, source_x)) {
                continue;
            }
            let x =
                ((source_x - i32::from(frame_left)) as f64 * scale_x) as i32 + i32::from(dst_left);
            let y =
                ((source_y - i32::from(frame_top)) as f64 * scale_y) as i32 + i32::from(dst_top);
            let source_index = pixels[source_y as usize * width + source_x as usize];
            write_pixel_clipped(
                bus,
                screen_base,
                screen_rb,
                x,
                y,
                color_map[usize::from(source_index)],
                screen_w,
                screen_h,
                pixel_size,
                dst_clip,
            );
        }
    }

    // QuickTime appends a QuickDraw-only warning after compressed image
    // data so systems without the Image Compression Manager can explain
    // why the picture was not drawn. The installed ICM suppresses that
    // fallback after a successful decode. Its stream begins with a
    // deliberately bogus PnSize opcode whose vertical value is $00AE and
    // whose horizontal value gives the byte count to skip.
    //
    // QuickTime 1993, pp. 3-25 to 3-26 documents the warning and the
    // $8200 payload. Apple DTS's August 1992 Macintosh Q&A additionally
    // confirms that QuickTime ignores the following fallback after it has
    // displayed the compressed image.
    let final_pos = if bus.read_long(end) == 0x0007_00AE {
        end.saturating_add(6)
            .saturating_add(u32::from(bus.read_word(end + 4)))
    } else {
        end
    };

    (final_pos, Some(source_clut))
}

pub(crate) fn peek_initial_packbits_clut(
    bus: &MacMemoryBus,
    pic_ptr: u32,
) -> Option<Vec<[u16; 3]>> {
    if pic_ptr == 0 {
        return None;
    }

    let frame_top = bus.read_word(pic_ptr + 2) as i16;
    let frame_left = bus.read_word(pic_ptr + 4) as i16;
    let frame_bottom = bus.read_word(pic_ptr + 6) as i16;
    let frame_right = bus.read_word(pic_ptr + 8) as i16;
    if frame_bottom <= frame_top || frame_right <= frame_left {
        return None;
    }

    let mut pos = pic_ptr + 10;
    let mut is_v2 = false;
    for _ in 0..10000 {
        if is_v2 && !pos.is_multiple_of(2) {
            pos += 1;
        }
        let opcode = if is_v2 {
            let opcode = bus.read_word(pos);
            pos += 2;
            opcode
        } else {
            let opcode = bus.read_byte(pos) as u16;
            pos += 1;
            opcode
        };

        match opcode {
            0x00 => {}
            0x01 => {
                let rgn_size = bus.read_word(pos) as u32;
                pos += rgn_size;
            }
            0x02 | 0x09 | 0x0A | 0x10 => pos += 8,
            0x03 | 0x05 | 0x08 | 0x0D | 0x15 | 0x16 | 0xA0 | 0x02FF => pos += 2,
            0x04 => pos += if is_v2 { 2 } else { 1 },
            0x06 | 0x07 | 0x0B | 0x0C | 0x0E | 0x0F => pos += 4,
            0x11 => {
                let version = bus.read_byte(pos);
                pos += 1;
                if version == 0x02 {
                    pos += 1;
                    is_v2 = true;
                }
            }
            0x12..=0x14 => pos = skip_pixpat(bus, pos),
            0x1A | 0x1B | 0x1D | 0x1F => pos += 6,
            0x1C | 0x1E => {}
            0x24..=0x27 | 0x2C..=0x2F => {
                let data_len = bus.read_word(pos) as u32;
                pos += 2 + data_len;
                pos = align_pict_pos(pos);
            }
            0x90 | 0x91 if bus.read_word(pos) & 0x8000 != 0 => {
                return peek_pack_bits_rect_clut(bus, pos);
            }
            0x98 | 0x99 => return peek_pack_bits_rect_clut(bus, pos),
            0xA1 => {
                pos += 2;
                let data_len = bus.read_word(pos) as u32;
                pos += 2 + data_len;
                if is_v2 && !data_len.is_multiple_of(2) {
                    pos += 1;
                }
            }
            0xFF => return None,
            0x0C00 => pos += 24,
            _ => {
                if is_v2 {
                    if (0x00A2..=0x00AF).contains(&opcode) {
                        let data_len = bus.read_word(pos) as u32;
                        pos = checked_skip_pict_data(pos, 2, data_len)?;
                    } else if (0x00B0..=0x00CF).contains(&opcode)
                        || (0x8000..=0x80FF).contains(&opcode)
                    {
                    } else if (0x00D0..=0x00FE).contains(&opcode) || opcode >= 0x8100 {
                        let data_len = bus.read_long(pos);
                        pos = checked_skip_pict_data(pos, 4, data_len)?;
                    } else if (0x0100..=0x7FFF).contains(&opcode) {
                        pos += u32::from(opcode >> 8) * 2;
                    } else {
                        return None;
                    }
                } else if let Some(new_pos) = skip_reserved_v1_opcode(bus, opcode, pos) {
                    pos = new_pos;
                } else {
                    return None;
                }
            }
        }
    }

    None
}

fn peek_pack_bits_rect_clut(bus: &MacMemoryBus, pos: u32) -> Option<Vec<[u16; 3]>> {
    let row_bytes_raw = bus.read_word(pos);
    if (row_bytes_raw & 0x8000) == 0 {
        return None;
    }
    let (pos, pm) = read_pixmap(bus, pos);
    let (pos, colors16, _ct_seed) = read_color_table(bus, pos);
    let mode = bus.read_word(pos + 16);
    if pm.pixel_size == 8 && pm.cmp_count == 1 && (mode & 0x003F) == 0 {
        Some(colors16)
    } else {
        None
    }
}

/// Read a PixMap structure from PICT data. Returns (pos, PixMapInfo).
struct PixMapInfo {
    row_bytes: u16,
    bounds_top: i16,
    bounds_left: i16,
    bounds_bottom: i16,
    bounds_right: i16,
    pixel_size: u16,
    cmp_count: u16,
    /// PixMap.packType: 0=default (no compression for >8bpp; PackBits for ≤8bpp),
    /// 1=no packing, 2=drop pad byte, 3=byte-PackBits on 16-bit pixels, 4=byte-PackBits
    /// on cmpCount component planes per row.
    /// Imaging With QuickDraw 1994, 4-29.
    pack_type: u16,
}

/// Read PixMap with baseAddr prefix (for DirectBitsRect).
fn read_pixmap_with_base(bus: &MacMemoryBus, mut pos: u32) -> (u32, PixMapInfo) {
    let _base_addr = bus.read_long(pos);
    pos += 4;
    read_pixmap(bus, pos)
}

/// Read PixMap starting from rowBytes (for PackBitsRect).
fn read_pixmap(bus: &MacMemoryBus, mut pos: u32) -> (u32, PixMapInfo) {
    let row_bytes_raw = bus.read_word(pos);
    pos += 2;
    let row_bytes = row_bytes_raw & 0x3FFF;
    let bounds_top = bus.read_word(pos) as i16;
    pos += 2;
    let bounds_left = bus.read_word(pos) as i16;
    pos += 2;
    let bounds_bottom = bus.read_word(pos) as i16;
    pos += 2;
    let bounds_right = bus.read_word(pos) as i16;
    pos += 2;
    let _version = bus.read_word(pos);
    pos += 2;
    let pack_type = bus.read_word(pos);
    pos += 2;
    let _pack_size = bus.read_long(pos);
    pos += 4;
    let _h_res = bus.read_long(pos);
    pos += 4;
    let _v_res = bus.read_long(pos);
    pos += 4;
    let _pixel_type = bus.read_word(pos);
    pos += 2;
    let pixel_size = bus.read_word(pos);
    pos += 2;
    let cmp_count = bus.read_word(pos);
    pos += 2;
    let _cmp_size = bus.read_word(pos);
    pos += 2;
    let _plane_bytes = bus.read_long(pos);
    pos += 4;
    let _pm_table = bus.read_long(pos);
    pos += 4;
    let _pm_reserved = bus.read_long(pos);
    pos += 4;

    (
        pos,
        PixMapInfo {
            row_bytes,
            bounds_top,
            bounds_left,
            bounds_bottom,
            bounds_right,
            pixel_size,
            cmp_count,
            pack_type,
        },
    )
}

fn skip_pixpat(bus: &MacMemoryBus, mut pos: u32) -> u32 {
    let pat_type = bus.read_word(pos);
    pos += 2;
    pos += 8; // Pat1Data.

    if pat_type == 2 {
        // ditherPat: RGBColor follows the old Pattern.
        return pos + 6;
    }

    if pat_type != 1 {
        return pos;
    }

    let (new_pos, pm) = read_pixmap(bus, pos);
    pos = new_pos;
    let (new_pos, _colors16, _ct_seed) = read_color_table(bus, pos);
    pos = new_pos;
    skip_pixdata(bus, pos, &pm)
}

fn skip_pixdata(bus: &MacMemoryBus, mut pos: u32, pm: &PixMapInfo) -> u32 {
    let height = (pm.bounds_bottom - pm.bounds_top).max(0) as u32;
    let row_bytes = u32::from(pm.row_bytes);

    if pm.pack_type == 1 || pm.row_bytes < 8 {
        return pos + row_bytes.saturating_mul(height);
    }

    if pm.pack_type == 2 {
        let data_bytes = if pm.pixel_size == 32 {
            row_bytes.saturating_mul(height).saturating_mul(3) / 4
        } else {
            row_bytes.saturating_mul(height)
        };
        return pos + data_bytes;
    }

    for _ in 0..height {
        let byte_count = if pm.row_bytes > 250 {
            let count = u32::from(bus.read_word(pos));
            pos += 2;
            count
        } else {
            let count = u32::from(bus.read_byte(pos));
            pos += 1;
            count
        };
        pos += byte_count;
    }

    pos
}

fn read_pict_u16(bytes: &[u8], pos: usize) -> Option<u16> {
    let hi = *bytes.get(pos)?;
    let lo = *bytes.get(pos + 1)?;
    Some(u16::from_be_bytes([hi, lo]))
}

fn read_pict_u32(bytes: &[u8], pos: usize) -> Option<u32> {
    let b0 = *bytes.get(pos)?;
    let b1 = *bytes.get(pos + 1)?;
    let b2 = *bytes.get(pos + 2)?;
    let b3 = *bytes.get(pos + 3)?;
    Some(u32::from_be_bytes([b0, b1, b2, b3]))
}

fn pict_add(pos: usize, amount: usize, len: usize) -> Option<usize> {
    let next = pos.checked_add(amount)?;
    if next <= len {
        Some(next)
    } else {
        None
    }
}

fn pict_align_index(pos: usize, len: usize) -> Option<usize> {
    pict_add(pos, usize::from(!pos.is_multiple_of(2)), len)
}

fn read_pixmap_bytes(bytes: &[u8], mut pos: usize) -> Option<(usize, PixMapInfo)> {
    let row_bytes_raw = read_pict_u16(bytes, pos)?;
    pos += 2;
    let row_bytes = row_bytes_raw & 0x3FFF;
    let bounds_top = read_pict_u16(bytes, pos)? as i16;
    pos += 2;
    let bounds_left = read_pict_u16(bytes, pos)? as i16;
    pos += 2;
    let bounds_bottom = read_pict_u16(bytes, pos)? as i16;
    pos += 2;
    let bounds_right = read_pict_u16(bytes, pos)? as i16;
    pos += 2;
    pos = pict_add(pos, 2, bytes.len())?; // version
    let pack_type = read_pict_u16(bytes, pos)?;
    pos += 2;
    pos = pict_add(pos, 4 + 4 + 4 + 2, bytes.len())?; // packSize, hRes, vRes, pixelType
    let pixel_size = read_pict_u16(bytes, pos)?;
    pos += 2;
    let cmp_count = read_pict_u16(bytes, pos)?;
    pos += 2;
    pos = pict_add(pos, 2 + 4 + 4 + 4, bytes.len())?; // cmpSize, planeBytes, pmTable, pmReserved

    Some((
        pos,
        PixMapInfo {
            row_bytes,
            bounds_top,
            bounds_left,
            bounds_bottom,
            bounds_right,
            pixel_size,
            cmp_count,
            pack_type,
        },
    ))
}

fn skip_color_table_bytes(bytes: &[u8], pos: usize) -> Option<usize> {
    let ct_size = usize::from(read_pict_u16(bytes, pos + 6)?);
    pict_add(
        pos,
        8usize.checked_add((ct_size + 1).checked_mul(8)?)?,
        bytes.len(),
    )
}

fn skip_pixpat_bytes(bytes: &[u8], mut pos: usize) -> Option<usize> {
    let pat_type = read_pict_u16(bytes, pos)?;
    pos = pict_add(pos, 2 + 8, bytes.len())?;

    if pat_type == 2 {
        return pict_add(pos, 6, bytes.len());
    }
    if pat_type != 1 {
        return Some(pos);
    }

    let (new_pos, pm) = read_pixmap_bytes(bytes, pos)?;
    let new_pos = skip_color_table_bytes(bytes, new_pos)?;
    skip_pixdata_bytes(bytes, new_pos, &pm)
}

fn skip_pixdata_bytes(bytes: &[u8], mut pos: usize, pm: &PixMapInfo) -> Option<usize> {
    let height = usize::try_from((pm.bounds_bottom - pm.bounds_top).max(0)).ok()?;
    let row_bytes = usize::from(pm.row_bytes);

    if pm.pack_type == 1 || pm.row_bytes < 8 {
        return pict_add(pos, row_bytes.checked_mul(height)?, bytes.len());
    }

    if pm.pack_type == 2 {
        let data_bytes = if pm.pixel_size == 32 {
            row_bytes.checked_mul(height)?.checked_mul(3)? / 4
        } else {
            row_bytes.checked_mul(height)?
        };
        return pict_add(pos, data_bytes, bytes.len());
    }

    for _ in 0..height {
        let byte_count = if pm.row_bytes > 250 {
            let count = usize::from(read_pict_u16(bytes, pos)?);
            pos = pict_add(pos, 2, bytes.len())?;
            count
        } else {
            let count = usize::from(*bytes.get(pos)?);
            pos = pict_add(pos, 1, bytes.len())?;
            count
        };
        pos = pict_add(pos, byte_count, bytes.len())?;
    }

    Some(pos)
}

fn skip_bits_rect_bytes(bytes: &[u8], mut pos: usize, has_rgn: bool) -> Option<usize> {
    if read_pict_u16(bytes, pos)? & 0x8000 != 0 {
        return skip_indexed_bits_rect_bytes(bytes, pos, has_rgn, false);
    }
    let row_bytes = usize::from(read_pict_u16(bytes, pos)? & 0x3FFF);
    let bounds_top = read_pict_u16(bytes, pos + 2)? as i16;
    let bounds_bottom = read_pict_u16(bytes, pos + 6)? as i16;
    pos = pict_add(pos, 10 + 18, bytes.len())?;
    if has_rgn {
        let rgn_size = usize::from(read_pict_u16(bytes, pos)?);
        pos = pict_add(pos, rgn_size, bytes.len())?;
    }
    let height = usize::try_from((bounds_bottom - bounds_top).max(0)).ok()?;
    pict_add(pos, row_bytes.checked_mul(height)?, bytes.len())
}

fn skip_pack_bits_rect_bytes(bytes: &[u8], pos: usize, has_rgn: bool) -> Option<usize> {
    skip_indexed_bits_rect_bytes(bytes, pos, has_rgn, true)
}

fn skip_indexed_bits_rect_bytes(
    bytes: &[u8],
    mut pos: usize,
    has_rgn: bool,
    packed: bool,
) -> Option<usize> {
    let row_bytes_raw = read_pict_u16(bytes, pos)?;
    let is_pixmap = (row_bytes_raw & 0x8000) != 0;
    let pm = if is_pixmap {
        let (new_pos, pm) = read_pixmap_bytes(bytes, pos)?;
        pos = skip_color_table_bytes(bytes, new_pos)?;
        pm
    } else {
        let row_bytes = row_bytes_raw & 0x3FFF;
        let bounds_top = read_pict_u16(bytes, pos + 2)? as i16;
        let bounds_left = read_pict_u16(bytes, pos + 4)? as i16;
        let bounds_bottom = read_pict_u16(bytes, pos + 6)? as i16;
        let bounds_right = read_pict_u16(bytes, pos + 8)? as i16;
        pos = pict_add(pos, 10, bytes.len())?;
        PixMapInfo {
            row_bytes,
            bounds_top,
            bounds_left,
            bounds_bottom,
            bounds_right,
            pixel_size: 1,
            cmp_count: 1,
            pack_type: 0,
        }
    };

    pos = pict_add(pos, 18, bytes.len())?;
    if has_rgn {
        let rgn_size = usize::from(read_pict_u16(bytes, pos)?);
        pos = pict_add(pos, rgn_size, bytes.len())?;
    }
    if packed {
        skip_pixdata_bytes(bytes, pos, &pm)
    } else {
        let height = usize::try_from((pm.bounds_bottom - pm.bounds_top).max(0)).ok()?;
        pict_add(
            pos,
            usize::from(pm.row_bytes).checked_mul(height)?,
            bytes.len(),
        )
    }
}

fn skip_direct_bits_rect_bytes(bytes: &[u8], mut pos: usize, has_rgn: bool) -> Option<usize> {
    pos = pict_add(pos, 4, bytes.len())?; // baseAddr
    let (new_pos, pm) = read_pixmap_bytes(bytes, pos)?;
    pos = skip_color_table_bytes(bytes, new_pos)?;
    pos = pict_add(pos, 18, bytes.len())?;
    if has_rgn {
        let rgn_size = usize::from(read_pict_u16(bytes, pos)?);
        pos = pict_add(pos, rgn_size, bytes.len())?;
    }
    skip_pixdata_bytes(bytes, pos, &pm)
}

fn skip_v1_reserved_bytes(bytes: &[u8], opcode: u16, pos: usize) -> Option<usize> {
    match opcode {
        0x35..=0x37 | 0x45..=0x47 | 0x55..=0x57 => pict_add(pos, 8, bytes.len()),
        0x3D..=0x3F | 0x4D..=0x4F | 0x5D..=0x5F | 0x7D..=0x7F | 0x8D..=0x8F => Some(pos),
        0x65..=0x67 => pict_add(pos, 12, bytes.len()),
        0x6D..=0x6F => pict_add(pos, 4, bytes.len()),
        0x75..=0x77 | 0x85..=0x87 => {
            let data_len = usize::from(read_pict_u16(bytes, pos)?);
            pict_add(pos, data_len, bytes.len())
        }
        _ => None,
    }
}

/// Return the byte length of a Picture record through EndOfPicture.
///
/// Color QuickDraw can ignore the 16-bit `picSize` field and read a picture
/// until the end-of-picture opcode, which is required for large spooled PICT
/// files. Inside Macintosh Volume V, V-92.
pub(crate) fn picture_stream_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 10 {
        return None;
    }

    let mut pos = 10usize;
    let mut opcount = 0usize;
    let mut is_v2 = false;

    while pos < bytes.len() {
        if opcount > 1_000_000 {
            return None;
        }
        opcount += 1;

        if is_v2 {
            pos = pict_align_index(pos, bytes.len())?;
        }

        let opcode = if is_v2 {
            let op = read_pict_u16(bytes, pos)?;
            pos = pict_add(pos, 2, bytes.len())?;
            op
        } else {
            let op = u16::from(*bytes.get(pos)?);
            pos = pict_add(pos, 1, bytes.len())?;
            op
        };

        pos = match opcode {
            0x00
            | 0x1C
            | 0x1E
            | 0x38..=0x3C
            | 0x48..=0x4C
            | 0x58..=0x5C
            | 0x78..=0x7C
            | 0x88..=0x8C => pos,
            0x01 | 0x70..=0x74 | 0x80..=0x84 => {
                let data_len = usize::from(read_pict_u16(bytes, pos)?);
                pict_add(pos, data_len, bytes.len())?
            }
            0x02 | 0x09 | 0x0A | 0x10 => pict_add(pos, 8, bytes.len())?,
            0x03 | 0x05 | 0x08 | 0x0D | 0x15 | 0x16 => pict_add(pos, 2, bytes.len())?,
            0x04 => pict_add(pos, if is_v2 { 2 } else { 1 }, bytes.len())?,
            0x06 | 0x07 | 0x0B | 0x0C | 0x0E | 0x0F | 0x21 | 0x68..=0x6C => {
                pict_add(pos, 4, bytes.len())?
            }
            0x11 => {
                let version = *bytes.get(pos)?;
                let next = pict_add(pos, 1, bytes.len())?;
                if version == 0x02 {
                    pos = pict_add(next, 1, bytes.len())?;
                    is_v2 = true;
                    pos
                } else {
                    next
                }
            }
            0x12..=0x14 => skip_pixpat_bytes(bytes, pos)?,
            0x1A | 0x1B | 0x1D | 0x1F | 0x22 => pict_add(pos, 6, bytes.len())?,
            0x20 | 0x30..=0x34 | 0x40..=0x44 | 0x50..=0x54 => pict_add(pos, 8, bytes.len())?,
            0x23 | 0xA0 => pict_add(pos, 2, bytes.len())?,
            0x28 => {
                let len = usize::from(*bytes.get(pos + 4)?);
                let mut next = pict_add(pos, 5usize.checked_add(len)?, bytes.len())?;
                if is_v2 && !(1 + len).is_multiple_of(2) {
                    next = pict_add(next, 1, bytes.len())?;
                }
                next
            }
            0x29 | 0x2A => {
                let len = usize::from(*bytes.get(pos + 1)?);
                let mut next = pict_add(pos, 2usize.checked_add(len)?, bytes.len())?;
                if is_v2 && !len.is_multiple_of(2) {
                    next = pict_add(next, 1, bytes.len())?;
                }
                next
            }
            0x2B => {
                let len = usize::from(*bytes.get(pos + 2)?);
                let mut next = pict_add(pos, 3usize.checked_add(len)?, bytes.len())?;
                if is_v2 && !(1 + len).is_multiple_of(2) {
                    next = pict_add(next, 1, bytes.len())?;
                }
                next
            }
            0x2C | 0x2D | 0x2E | 0x24..=0x27 | 0x2F => {
                let data_len = usize::from(read_pict_u16(bytes, pos)?);
                let next = pict_add(pos, 2usize.checked_add(data_len)?, bytes.len())?;
                pict_align_index(next, bytes.len())?
            }
            0x60..=0x64 => pict_add(pos, 12, bytes.len())?,
            0x90 => skip_bits_rect_bytes(bytes, pos, false)?,
            0x91 => skip_bits_rect_bytes(bytes, pos, true)?,
            0x98 => skip_pack_bits_rect_bytes(bytes, pos, false)?,
            0x99 => skip_pack_bits_rect_bytes(bytes, pos, true)?,
            0x9A => skip_direct_bits_rect_bytes(bytes, pos, false)?,
            0x9B => skip_direct_bits_rect_bytes(bytes, pos, true)?,
            0xA1 => {
                let data_len = usize::from(read_pict_u16(bytes, pos + 2)?);
                let mut next = pict_add(pos, 4usize.checked_add(data_len)?, bytes.len())?;
                if is_v2 && !data_len.is_multiple_of(2) {
                    next = pict_add(next, 1, bytes.len())?;
                }
                next
            }
            0xFF => return Some(pos),
            0x0C00 => pict_add(pos, 24, bytes.len())?,
            0x02FF => pict_add(pos, 2, bytes.len())?,
            _ if is_v2 => {
                if (0x00A2..=0x00AF).contains(&opcode) {
                    let data_len = usize::from(read_pict_u16(bytes, pos)?);
                    let next = pict_add(pos, 2usize.checked_add(data_len)?, bytes.len())?;
                    pict_align_index(next, bytes.len())?
                } else if (0x00B0..=0x00CF).contains(&opcode) || (0x8000..=0x80FF).contains(&opcode)
                {
                    pos
                } else if (0x00D0..=0x00FE).contains(&opcode) || opcode >= 0x8100 {
                    let data_len = usize::try_from(read_pict_u32(bytes, pos)?).ok()?;
                    let next = pict_add(pos, 4usize.checked_add(data_len)?, bytes.len())?;
                    pict_align_index(next, bytes.len())?
                } else if (0x0100..=0x7FFF).contains(&opcode) {
                    pict_add(pos, usize::from(opcode >> 8).checked_mul(2)?, bytes.len())?
                } else {
                    return None;
                }
            }
            _ => skip_v1_reserved_bytes(bytes, opcode, pos)?,
        };
    }

    None
}

/// Read a ColorTable from PICT data.
/// Returns (pos, color_table_16bit, ct_seed).
/// color_table_16bit: 256 entries of [r16, g16, b16].
/// ct_seed is the PICT CTab's ctSeed field; used by
/// `parse_pack_bits_rect` to skip remapping when the PICT CTab
/// and current GDevice CTab share a seed (matching CopyBits seed behavior).
fn read_color_table(bus: &MacMemoryBus, mut pos: u32) -> (u32, Vec<[u16; 3]>, u32) {
    let ct_seed = bus.read_long(pos);
    pos += 4;
    let ct_flags = bus.read_word(pos);
    pos += 2;
    let ct_size = bus.read_word(pos) as u32;
    pos += 2;

    if trace_pict_enabled() {
        eprintln!(
            "[PICT] ColorTable ctSeed=${:08X} ctFlags=0x{:04X} ctSize={} at ${:08X}",
            ct_seed,
            ct_flags,
            ct_size,
            pos - 8
        );
    }

    let mut colors16 = vec![[0u16; 3]; 256];
    for i in 0..=ct_size {
        let value = bus.read_word(pos) as usize;
        pos += 2;
        let r = bus.read_word(pos);
        pos += 2;
        let g = bus.read_word(pos);
        pos += 2;
        let b = bus.read_word(pos);
        pos += 2;
        let idx = if ct_flags & 0x8000 != 0 {
            i as usize
        } else {
            value
        };
        if idx < 256 {
            colors16[idx] = [r, g, b];
        }
    }

    if trace_pict_palette_enabled() {
        for index in [
            0usize, 1, 2, 15, 16, 17, 32, 43, 50, 93, 100, 150, 185, 220, 245,
        ] {
            if index < colors16.len() {
                let [r, g, b] = colors16[index];
                eprintln!("[PICT]   clut[{}]=({:04X},{:04X},{:04X})", index, r, g, b);
            }
        }
    }

    (pos, colors16, ct_seed)
}

/// Decompress 16-bit chunked PackBits for one scanline (PixMap.packType=3).
///
/// At 16bpp packType=3 the run-length encoding operates on 16-bit pixels
/// rather than bytes: a literal run of N+1 emits N+1 pixels (2 bytes each),
/// and a repeat run of -N+1 emits -N+1 copies of one 2-byte pixel.
/// The leading byte-count word/byte selects between word/byte length per
/// the same `rowBytes > 250` rule as byte PackBits.
/// Imaging With QuickDraw 1994, 4-30.
fn unpack_bits_chunk16_into(
    bus: &MacMemoryBus,
    mut pos: u32,
    row_bytes: u16,
    result: &mut Vec<u8>,
) -> u32 {
    let byte_count = if row_bytes > 250 {
        let bc = bus.read_word(pos) as u32;
        pos += 2;
        bc
    } else {
        let bc = bus.read_byte(pos) as u32;
        pos += 1;
        bc
    };

    let end_pos = pos + byte_count;
    result.clear();
    result.reserve(row_bytes as usize);
    if end_pos <= bus.ram_size() {
        unpack_bits_chunk16_data_into(bus.ram_slice(pos, byte_count), row_bytes, result);
        return end_pos;
    }

    while pos < end_pos && result.len() < row_bytes as usize {
        let flag = bus.read_byte(pos) as i8;
        pos += 1;

        if flag >= 0 {
            // Literal run: flag+1 pixels (each 2 bytes) follow.
            let count = (flag as usize) + 1;
            for _ in 0..count {
                if pos + 1 < end_pos {
                    result.push(bus.read_byte(pos));
                    result.push(bus.read_byte(pos + 1));
                    pos += 2;
                }
            }
        } else if flag != -128 {
            // Repeat run: -(flag)+1 copies of next pixel (2 bytes).
            let count = (-(flag as i16)) as usize + 1;
            let hi = bus.read_byte(pos);
            let lo = bus.read_byte(pos + 1);
            pos += 2;
            for _ in 0..count {
                result.push(hi);
                result.push(lo);
            }
        }
        // flag == -128 (0x80) is a NOP
    }

    end_pos
}

fn unpack_bits_chunk16_data_into(data: &[u8], row_bytes: u16, result: &mut Vec<u8>) {
    let mut pos = 0usize;
    while pos < data.len() && result.len() < row_bytes as usize {
        let flag = data[pos] as i8;
        pos += 1;

        if flag >= 0 {
            let count = (flag as usize) + 1;
            let byte_count = count.saturating_mul(2).min(data.len().saturating_sub(pos));
            result.extend_from_slice(&data[pos..pos + byte_count]);
            pos += byte_count;
        } else if flag != -128 {
            let count = (-(flag as i16)) as usize + 1;
            if pos + 1 >= data.len() {
                break;
            }
            let hi = data[pos];
            let lo = data[pos + 1];
            pos += 2;
            for _ in 0..count {
                result.push(hi);
                result.push(lo);
            }
        }
    }
}

fn unpack_bits_chunk16(bus: &MacMemoryBus, pos: u32, row_bytes: u16) -> (u32, Vec<u8>) {
    let mut result = Vec::with_capacity(row_bytes as usize);
    let end_pos = unpack_bits_chunk16_into(bus, pos, row_bytes, &mut result);
    (end_pos, result)
}

/// Decompress PackBits data when the decoded row length differs from the
/// PixMap rowBytes value used to size the scanline byte count.
fn unpack_bits_with_byte_count_row_bytes(
    bus: &MacMemoryBus,
    pos: u32,
    decoded_row_bytes: u16,
    byte_count_row_bytes: u16,
) -> (u32, Vec<u8>) {
    let mut result = Vec::with_capacity(decoded_row_bytes as usize);
    let end_pos = unpack_bits_with_byte_count_row_bytes_into(
        bus,
        pos,
        decoded_row_bytes,
        byte_count_row_bytes,
        &mut result,
    );
    (end_pos, result)
}

fn unpack_bits_with_byte_count_row_bytes_into(
    bus: &MacMemoryBus,
    mut pos: u32,
    decoded_row_bytes: u16,
    byte_count_row_bytes: u16,
    result: &mut Vec<u8>,
) -> u32 {
    // Read byte count for this scanline
    let byte_count = if byte_count_row_bytes > 250 {
        let bc = bus.read_word(pos) as u32;
        pos += 2;
        bc
    } else {
        let bc = bus.read_byte(pos) as u32;
        pos += 1;
        bc
    };

    let end_pos = pos + byte_count;
    result.clear();
    result.reserve(decoded_row_bytes as usize);
    if end_pos <= bus.ram_size() {
        unpack_bits_data_into(bus.ram_slice(pos, byte_count), decoded_row_bytes, result);
        return end_pos;
    }

    while pos < end_pos && result.len() < (decoded_row_bytes as usize) * 2 {
        let flag = bus.read_byte(pos) as i8;
        pos += 1;

        if flag >= 0 {
            // Literal run: flag+1 bytes follow
            let count = (flag as usize) + 1;
            for _ in 0..count {
                if pos < end_pos {
                    result.push(bus.read_byte(pos));
                    pos += 1;
                }
            }
        } else if flag != -128 {
            // Repeat run: -(flag)+1 copies of next byte
            let count = (-(flag as i16)) as usize + 1;
            let val = bus.read_byte(pos);
            pos += 1;
            for _ in 0..count {
                result.push(val);
            }
        }
        // flag == -128 (0x80) is a NOP
    }

    end_pos
}

fn unpack_bits_with_byte_count_row_bytes_mapped_into(
    bus: &MacMemoryBus,
    mut pos: u32,
    decoded_row_bytes: u16,
    byte_count_row_bytes: u16,
    src_to_dst: &[u8; 256],
    result: &mut Vec<u8>,
) -> u32 {
    let byte_count = if byte_count_row_bytes > 250 {
        let bc = bus.read_word(pos) as u32;
        pos += 2;
        bc
    } else {
        let bc = bus.read_byte(pos) as u32;
        pos += 1;
        bc
    };

    let end_pos = pos + byte_count;
    result.clear();
    result.reserve(decoded_row_bytes as usize);
    if end_pos <= bus.ram_size() {
        unpack_bits_data_mapped_into(
            bus.ram_slice(pos, byte_count),
            decoded_row_bytes,
            src_to_dst,
            result,
        );
        return end_pos;
    }

    while pos < end_pos && result.len() < (decoded_row_bytes as usize) * 2 {
        let flag = bus.read_byte(pos) as i8;
        pos += 1;

        if flag >= 0 {
            let count = (flag as usize) + 1;
            for _ in 0..count {
                if pos < end_pos {
                    result.push(src_to_dst[bus.read_byte(pos) as usize]);
                    pos += 1;
                }
            }
        } else if flag != -128 {
            let count = (-(flag as i16)) as usize + 1;
            let val = src_to_dst[bus.read_byte(pos) as usize];
            pos += 1;
            result.extend(std::iter::repeat(val).take(count));
        }
    }

    end_pos
}

fn unpack_bits_data_into(data: &[u8], decoded_row_bytes: u16, result: &mut Vec<u8>) {
    let mut pos = 0usize;
    while pos < data.len() && result.len() < (decoded_row_bytes as usize) * 2 {
        let flag = data[pos] as i8;
        pos += 1;

        if flag >= 0 {
            let count = (flag as usize) + 1;
            let end = pos.saturating_add(count).min(data.len());
            result.extend_from_slice(&data[pos..end]);
            pos = end;
        } else if flag != -128 {
            let count = (-(flag as i16)) as usize + 1;
            let Some(&val) = data.get(pos) else {
                break;
            };
            pos += 1;
            result.extend(std::iter::repeat(val).take(count));
        }
    }
}

fn unpack_bits_data_mapped_into(
    data: &[u8],
    decoded_row_bytes: u16,
    src_to_dst: &[u8; 256],
    result: &mut Vec<u8>,
) {
    let mut pos = 0usize;
    while pos < data.len() && result.len() < (decoded_row_bytes as usize) * 2 {
        let flag = data[pos] as i8;
        pos += 1;

        if flag >= 0 {
            let count = (flag as usize) + 1;
            let end = pos.saturating_add(count).min(data.len());
            result.extend(
                data[pos..end]
                    .iter()
                    .map(|&pixel| src_to_dst[pixel as usize]),
            );
            pos = end;
        } else if flag != -128 {
            let count = (-(flag as i16)) as usize + 1;
            let Some(&val) = data.get(pos) else {
                break;
            };
            pos += 1;
            result.extend(std::iter::repeat(src_to_dst[val as usize]).take(count));
        }
    }
}

/// Write a pixel to an indexed screen framebuffer.
fn write_pixel(
    bus: &mut MacMemoryBus,
    screen_base: u32,
    screen_rb: u32,
    x: i32,
    y: i32,
    color_index: u8,
    screen_w: i32,
    screen_h: i32,
    pixel_size: u16,
) {
    if x < 0 || y < 0 || x >= screen_w || y >= screen_h {
        return;
    }
    match pixel_size {
        1 => {
            // 1bpp: each byte holds 8 pixels, MSB = leftmost
            let byte_offset = (x as u32) / 8;
            let bit = 7 - ((x as u32) % 8);
            let addr = screen_base + (y as u32) * screen_rb + byte_offset;
            let byte = bus.read_byte(addr);
            // In 1bpp Mac, bit set = black (color_index 255), bit clear = white (0)
            if color_index != 0 {
                bus.write_byte(addr, byte | (1 << bit));
            } else {
                bus.write_byte(addr, byte & !(1 << bit));
            }
        }
        bits @ (2 | 4) => {
            let pixels_per_byte = 8 / u32::from(bits);
            let pixel = x as u32;
            let shift = 8 - u32::from(bits) - (pixel % pixels_per_byte) * u32::from(bits);
            let pixel_mask = (1u8 << bits) - 1;
            let shifted_mask = pixel_mask << shift;
            let addr = screen_base + (y as u32) * screen_rb + pixel / pixels_per_byte;
            let byte = bus.read_byte(addr);
            bus.write_byte(
                addr,
                (byte & !shifted_mask) | ((color_index & pixel_mask) << shift),
            );
        }
        16 => {
            let device_clut = STANDARD_MAC_8BPP_CLUT
                .get_or_init(crate::trap::dispatch::TrapDispatcher::standard_mac_8bpp_clut);
            let [red, green, blue] = device_clut[usize::from(color_index)];
            let pixel = ((red >> 11) << 10) | ((green >> 11) << 5) | (blue >> 11);
            let addr = screen_base + (y as u32) * screen_rb + (x as u32) * 2;
            bus.write_word(addr, pixel);
        }
        _ => {
            // 8bpp: one byte per pixel
            let addr = screen_base + (y as u32) * screen_rb + (x as u32);
            bus.write_byte(addr, color_index);
        }
    }
}

fn dst_clip_contains(clip: Option<&DstClip>, x: i32, y: i32) -> bool {
    clip.is_none_or(|clip| clip.contains(x, y))
}

#[allow(clippy::too_many_arguments)]
fn write_pixel_clipped(
    bus: &mut MacMemoryBus,
    screen_base: u32,
    screen_rb: u32,
    x: i32,
    y: i32,
    color_index: u8,
    screen_w: i32,
    screen_h: i32,
    pixel_size: u16,
    dst_clip: Option<&DstClip>,
) {
    if dst_clip_contains(dst_clip, x, y) {
        write_pixel(
            bus,
            screen_base,
            screen_rb,
            x,
            y,
            color_index,
            screen_w,
            screen_h,
            pixel_size,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn write_rgb555_pixel_clipped(
    bus: &mut MacMemoryBus,
    screen_base: u32,
    screen_rb: u32,
    x: i32,
    y: i32,
    pixel: u16,
    screen_w: i32,
    screen_h: i32,
    dst_clip: Option<&DstClip>,
) {
    if x < 0 || y < 0 || x >= screen_w || y >= screen_h || !dst_clip_contains(dst_clip, x, y) {
        return;
    }
    let addr = screen_base + (y as u32) * screen_rb + (x as u32) * 2;
    bus.write_word(addr, pixel & 0x7fff);
}

fn clut_black_white_indices(clut: &[[u16; 3]; 256]) -> (u8, u8) {
    let mut black_idx = 0u8;
    let mut black_luma = u64::MAX;
    let mut white_idx = 0u8;
    let mut white_luma = 0u64;

    for (idx, entry) in clut.iter().enumerate() {
        let luma = u64::from(entry[0]) + u64::from(entry[1]) + u64::from(entry[2]);
        if luma < black_luma || (luma == black_luma && idx as u8 > black_idx) {
            black_idx = idx as u8;
            black_luma = luma;
        }
        if luma > white_luma || (luma == white_luma && (idx as u8) < white_idx) {
            white_idx = idx as u8;
            white_luma = luma;
        }
    }

    (black_idx, white_idx)
}

fn one_bit_destination_clut() -> [[u16; 3]; 256] {
    let mut clut = [[0x0000u16, 0x0000, 0x0000]; 256];
    clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
    clut[255] = [0x0000, 0x0000, 0x0000];
    clut
}

fn indexed_destination_clut(device_clut: &[[u16; 3]; 256], pixel_size: u16) -> [[u16; 3]; 256] {
    if pixel_size == 1 {
        return one_bit_destination_clut();
    }
    let mut clut = *device_clut;
    if matches!(pixel_size, 2 | 4) {
        let entry_count = 1usize << pixel_size;
        let terminal = clut[entry_count - 1];
        clut[entry_count..].fill(terminal);
    }
    clut
}

/// Map a legacy Mac Pascal QuickDraw color constant to a destination CLUT
/// index. Inside Macintosh Volume I, I-172 defines the 8 canonical colors.
/// `default_idx` is returned for color = 0 (Pascal sentinel for "no color
/// change"). Unknown constants fall through to `color & 0xFF` so apps passing
/// custom CLUT indices via FgColor still address the intended slot.
fn pict_qd_color_to_clut_index(color: u32, default_idx: u8, black_idx: u8, white_idx: u8) -> u8 {
    match color {
        0 => default_idx,
        30 => white_idx,
        33 => black_idx,
        205 => 35,  // redColor → red slot on std Mac 8bpp CLUT
        341 => 173, // greenColor
        409 => 210, // blueColor
        69 => 17,   // yellowColor
        137 => 137, // magentaColor
        273 => 69,  // cyanColor
        _ => (color & 0xFF) as u8,
    }
}

fn read_shape_rect(bus: &MacMemoryBus, pos: u32) -> (i16, i16, i16, i16) {
    let t = bus.read_word(pos) as i16;
    let l = bus.read_word(pos + 2) as i16;
    let b = bus.read_word(pos + 4) as i16;
    let r = bus.read_word(pos + 6) as i16;
    (t, l, b, r)
}

/// Transform a PICT-space rect to dst-space pixel coordinates.
fn transform_shape_rect(
    src_top: i16,
    src_left: i16,
    src_bottom: i16,
    src_right: i16,
    frame_top: i16,
    frame_left: i16,
    dst_top: i16,
    dst_left: i16,
    scale_x: f64,
    scale_y: f64,
) -> (i32, i32, i32, i32) {
    let x1 = ((src_left as f64 - frame_left as f64) * scale_x + dst_left as f64).round() as i32;
    let y1 = ((src_top as f64 - frame_top as f64) * scale_y + dst_top as f64).round() as i32;
    let x2 = ((src_right as f64 - frame_left as f64) * scale_x + dst_left as f64).round() as i32;
    let y2 = ((src_bottom as f64 - frame_top as f64) * scale_y + dst_top as f64).round() as i32;
    (x1, y1, x2, y2)
}

/// Plot a dst-space pixel, honoring the PICT clip region (checked in
/// picture-space via a back-transform).
#[allow(clippy::too_many_arguments)]
fn plot_dst_pixel(
    bus: &mut MacMemoryBus,
    screen_base: u32,
    screen_rb: u32,
    screen_w: i32,
    screen_h: i32,
    pixel_size: u16,
    x: i32,
    y: i32,
    color_index: u8,
    frame_top: i16,
    frame_left: i16,
    dst_top: i16,
    dst_left: i16,
    scale_x: f64,
    scale_y: f64,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
) {
    if let Some(rgn) = clip_region {
        let inv_sx = if scale_x > 0.0 { 1.0 / scale_x } else { 1.0 };
        let inv_sy = if scale_y > 0.0 { 1.0 / scale_y } else { 1.0 };
        let pic_x = ((x - dst_left as i32) as f64 * inv_sx + frame_left as f64).floor() as i32;
        let pic_y = ((y - dst_top as i32) as f64 * inv_sy + frame_top as f64).floor() as i32;
        if !rgn.contains(pic_y, pic_x) {
            return;
        }
    }
    write_pixel_clipped(
        bus,
        screen_base,
        screen_rb,
        x,
        y,
        color_index,
        screen_w,
        screen_h,
        pixel_size,
        dst_clip,
    );
}

/// Fill a dst-space axis-aligned rectangle with `color_index`.
#[allow(clippy::too_many_arguments)]
fn fill_dst_rect(
    bus: &mut MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    frame_top: i16,
    frame_left: i16,
    dst_top: i16,
    dst_left: i16,
    scale_x: f64,
    scale_y: f64,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    color_index: u8,
) {
    let (sb, srb, sw, sh, ps) = screen_mode;
    for y in y1..y2 {
        for x in x1..x2 {
            plot_dst_pixel(
                bus,
                sb,
                srb,
                sw as i32,
                sh as i32,
                ps,
                x,
                y,
                color_index,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
            );
        }
    }
}

/// Fill a dst-space axis-aligned rectangle with an 8x8 QuickDraw pattern.
/// Pattern bit set → `on_color`; bit clear → `off_color`. Pattern is
/// sampled in dst-pixel coords modulo 8.
#[allow(clippy::too_many_arguments)]
fn fill_dst_rect_pat(
    bus: &mut MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    frame_top: i16,
    frame_left: i16,
    dst_top: i16,
    dst_left: i16,
    scale_x: f64,
    scale_y: f64,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    pattern: [u8; 8],
    on_color: u8,
    off_color: u8,
) {
    // Fast path for the two monochrome patterns (solid black/white)
    // so we don't do per-pixel bit lookups for the common case.
    if pattern == [0xFF; 8] {
        fill_dst_rect(
            bus,
            screen_mode,
            x1,
            y1,
            x2,
            y2,
            frame_top,
            frame_left,
            dst_top,
            dst_left,
            scale_x,
            scale_y,
            clip_region,
            dst_clip,
            on_color,
        );
        return;
    }
    if pattern == [0x00; 8] {
        fill_dst_rect(
            bus,
            screen_mode,
            x1,
            y1,
            x2,
            y2,
            frame_top,
            frame_left,
            dst_top,
            dst_left,
            scale_x,
            scale_y,
            clip_region,
            dst_clip,
            off_color,
        );
        return;
    }
    let (sb, srb, sw, sh, ps) = screen_mode;
    for y in y1..y2 {
        let row = pattern[y.rem_euclid(8) as usize];
        for x in x1..x2 {
            let bit = 1u8 << (7 - x.rem_euclid(8));
            let color = if row & bit != 0 { on_color } else { off_color };
            plot_dst_pixel(
                bus,
                sb,
                srb,
                sw as i32,
                sh as i32,
                ps,
                x,
                y,
                color,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
            );
        }
    }
}

/// Render a PICT shape-rect opcode ($30-$34 family).
/// `kind` = low 4 bits of opcode: 0=frame, 1=paint, 2=erase, 3=invert,
/// 4=fill. `pen_size` (h, w) controls frame thickness. `pn_pat` /
/// `bk_pat` are the 8x8 pen + background patterns from the PICT state
/// opcodes 0x09 / 0x02. Inside Macintosh Volume I, I-190 + Imaging With
/// QuickDraw 1994, Appendix A, A-7.
///
/// `fg_idx` / `bg_idx` are 8bpp CLUT indices tracked from FgColor (0x0E)
/// / BkColor (0x0F) state opcodes. paintRect / fillRect / frameRect draw
/// with fg_idx for pen bits; eraseRect uses bg_idx per IM:V V-66.
#[allow(clippy::too_many_arguments)]
fn draw_shape_rect(
    bus: &mut MacMemoryBus,
    kind: u8,
    src_top: i16,
    src_left: i16,
    src_bottom: i16,
    src_right: i16,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    screen_mode: (u32, u32, u16, u16, u16),
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    pen_size: (i16, i16),
    pn_pat: [u8; 8],
    bk_pat: [u8; 8],
    fill_pat: [u8; 8],
    fg_idx: u8,
    bg_idx: u8,
) {
    let kind = kind & 0x0F;
    if src_bottom <= src_top || src_right <= src_left {
        return;
    }
    let (x1, y1, x2, y2) = transform_shape_rect(
        src_top, src_left, src_bottom, src_right, frame_top, frame_left, dst_top, dst_left,
        scale_x, scale_y,
    );
    if x2 <= x1 || y2 <= y1 {
        return;
    }
    // fg/bg colors come from tracked FgColor/BkColor state (default
    // fg_idx=255 black, bg_idx=0 white per QD initPort). Invert (kind=3)
    // is still pure XOR and ignores fg/bg.
    match kind {
        0 => {
            // frameRect — 4 edges, thickness = PnSize (in picture-
            // space, scaled along with the rect).  Scale each PnSize
            // dim separately so 1.5× horizontal + 1.0× vertical draws
            // still produce distinguishable thicknesses.  Per IM:I
            // I-163, pen_h applies to top/bottom edges, pen_w to
            // left/right edges; PnSize(h, w) stored with pen_size.0 = h.
            let (pen_h, pen_w) = pen_size;
            let eh = ((pen_h as f64 * scale_y).round() as i32).max(1);
            let ew = ((pen_w as f64 * scale_x).round() as i32).max(1);
            fill_dst_rect(
                bus,
                screen_mode,
                x1,
                y1,
                x2,
                y1 + eh,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
                fg_idx,
            );
            fill_dst_rect(
                bus,
                screen_mode,
                x1,
                y2 - eh,
                x2,
                y2,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
                fg_idx,
            );
            fill_dst_rect(
                bus,
                screen_mode,
                x1,
                y1,
                x1 + ew,
                y2,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
                fg_idx,
            );
            fill_dst_rect(
                bus,
                screen_mode,
                x2 - ew,
                y1,
                x2,
                y2,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
                fg_idx,
            );
        }
        1 => {
            // paintRect — interior uses pn_pat with fg_idx / bg_idx for
            // set / clear bits.
            fill_dst_rect_pat(
                bus,
                screen_mode,
                x1,
                y1,
                x2,
                y2,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
                pn_pat,
                fg_idx,
                bg_idx,
            );
        }
        4 => {
            // fillRect — interior uses fill_pat (tracked via state opcode
            // 0x0A) instead of pn_pat. Per IM:I I-169 fillRect "paints the
            // area ... using pat as a pattern"; the pat arg on the 68k trap
            // comes from the FillPat state, distinct from the pen pattern
            // used by paintRect.
            fill_dst_rect_pat(
                bus,
                screen_mode,
                x1,
                y1,
                x2,
                y2,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
                fill_pat,
                fg_idx,
                bg_idx,
            );
        }
        2 => {
            // eraseRect — interior uses bk_pat with bg_idx for set bits
            // per IM:V V-66 (erase draws in background color).
            fill_dst_rect_pat(
                bus,
                screen_mode,
                x1,
                y1,
                x2,
                y2,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
                bk_pat,
                bg_idx,
                fg_idx,
            );
        }
        3 => {
            // invertRect — XOR each pixel in the interior.
            invert_dst_rect(
                bus,
                screen_mode,
                x1,
                y1,
                x2,
                y2,
                frame_top,
                frame_left,
                dst_top,
                dst_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
            );
        }
        _ => {}
    }
}

/// XOR-invert each pixel in a dst-space axis-aligned rectangle.
#[allow(clippy::too_many_arguments)]
fn invert_dst_rect(
    bus: &mut MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    frame_top: i16,
    frame_left: i16,
    dst_top: i16,
    dst_left: i16,
    scale_x: f64,
    scale_y: f64,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
) {
    let (sb, srb, sw, sh, ps) = screen_mode;
    let sw = sw as i32;
    let sh = sh as i32;
    for y in y1..y2 {
        if y < 0 || y >= sh {
            continue;
        }
        for x in x1..x2 {
            if x < 0 || x >= sw {
                continue;
            }
            if !dst_clip_contains(dst_clip, x, y) {
                continue;
            }
            if let Some(rgn) = clip_region {
                let inv_sx = if scale_x > 0.0 { 1.0 / scale_x } else { 1.0 };
                let inv_sy = if scale_y > 0.0 { 1.0 / scale_y } else { 1.0 };
                let pic_x =
                    ((x - dst_left as i32) as f64 * inv_sx + frame_left as f64).floor() as i32;
                let pic_y =
                    ((y - dst_top as i32) as f64 * inv_sy + frame_top as f64).floor() as i32;
                if !rgn.contains(pic_y, pic_x) {
                    continue;
                }
            }
            if ps == 1 {
                let addr = sb + (y as u32) * srb + (x as u32) / 8;
                let bit = 7 - ((x as u32) % 8);
                let byte = bus.read_byte(addr);
                bus.write_byte(addr, byte ^ (1 << bit));
            } else {
                let addr = sb + (y as u32) * srb + (x as u32);
                let byte = bus.read_byte(addr);
                bus.write_byte(addr, byte ^ 0xFF);
            }
        }
    }
}

/// Render a PICT shape-oval opcode ($50-$54 family).
/// `kind` = low 4 bits of opcode (0=frame, 1=paint, 2=erase, 3=invert,
/// 4=fill). Inside Macintosh Volume I, I-193 + Imaging With QuickDraw
/// 1994, Appendix A, A-9.
#[allow(clippy::too_many_arguments)]
fn draw_shape_oval(
    bus: &mut MacMemoryBus,
    kind: u8,
    src_top: i16,
    src_left: i16,
    src_bottom: i16,
    src_right: i16,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    screen_mode: (u32, u32, u16, u16, u16),
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    pen_size: (i16, i16),
    pn_pat: [u8; 8],
    bk_pat: [u8; 8],
    fill_pat: [u8; 8],
    fg_idx: u8,
    bg_idx: u8,
) {
    draw_shape_oval_or_arc(
        bus,
        kind,
        src_top,
        src_left,
        src_bottom,
        src_right,
        dst_top,
        dst_left,
        frame_top,
        frame_left,
        scale_x,
        scale_y,
        screen_mode,
        clip_region,
        dst_clip,
        None,
        pen_size,
        pn_pat,
        bk_pat,
        fill_pat,
        fg_idx,
        bg_idx,
    );
}

/// Shared renderer for shape oval + arc opcodes. When `arc_angles` is
/// Some((start, extent)) the pixel filter gates by Mac-convention
/// angle-from-center (0°=north, CW positive). Frame mode becomes
/// arc-outline; paint/erase/invert/fill become wedge fill. Full-oval
/// behavior is preserved when None.
#[allow(clippy::too_many_arguments)]
fn draw_shape_oval_or_arc(
    bus: &mut MacMemoryBus,
    kind: u8,
    src_top: i16,
    src_left: i16,
    src_bottom: i16,
    src_right: i16,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    screen_mode: (u32, u32, u16, u16, u16),
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    arc_angles: Option<(i16, i16)>,
    pen_size: (i16, i16),
    pn_pat: [u8; 8],
    bk_pat: [u8; 8],
    fill_pat: [u8; 8],
    fg_idx: u8,
    bg_idx: u8,
) {
    let kind = kind & 0x0F;
    if src_bottom <= src_top || src_right <= src_left {
        return;
    }
    let (x1, y1, x2, y2) = transform_shape_rect(
        src_top, src_left, src_bottom, src_right, frame_top, frame_left, dst_top, dst_left,
        scale_x, scale_y,
    );
    if x2 <= x1 || y2 <= y1 {
        return;
    }
    let cx = (x1 as f64 + x2 as f64) * 0.5;
    let cy = (y1 as f64 + y2 as f64) * 0.5;
    let rx = (x2 - x1) as f64 * 0.5;
    let ry = (y2 - y1) as f64 * 0.5;
    // Frame mode inset = pen_size, scaled by the picture→dst ratio so a
    // thick PnSize doesn't degenerate to a 1-pixel ring.
    let (pen_h, pen_w) = pen_size;
    let stamp_w = ((pen_w.max(1) as f64) * scale_x).max(1.0);
    let stamp_h = ((pen_h.max(1) as f64) * scale_y).max(1.0);
    let rx_in = (rx - stamp_w).max(0.0);
    let ry_in = (ry - stamp_h).max(0.0);
    let (sb, srb, sw, sh, ps) = screen_mode;
    let sw = sw as i32;
    let sh = sh as i32;
    // Prepare arc-angle gating state. Empty extent → skip everything
    // (matches IM: arcAngle=0 draws nothing).
    let arc_range = arc_angles.map(|(start_raw, extent_raw)| {
        if extent_raw == 0 {
            // sentinel: unreachable range so every pixel fails the test.
            (f64::INFINITY, f64::INFINITY)
        } else {
            let mut start = start_raw as f64;
            let mut extent = extent_raw as f64;
            if extent < 0.0 {
                start += extent;
                extent = -extent;
            }
            if extent > 360.0 {
                extent = 360.0;
            }
            start = start.rem_euclid(360.0);
            (start, start + extent)
        }
    });
    for y in y1..y2 {
        let ny = (y as f64 + 0.5 - cy) / ry;
        let ny2 = 1.0 - ny * ny;
        if ny2 < 0.0 {
            continue;
        }
        let hw_out = ny2.sqrt() * rx;
        let xl_out = (cx - hw_out).round() as i32;
        let xr_out = (cx + hw_out).round() as i32;
        let (xl_in, xr_in) = if rx_in <= 0.0 || ry_in <= 0.0 {
            (i32::MAX, i32::MIN)
        } else {
            let ny_in = (y as f64 + 0.5 - cy) / ry_in;
            let ny2_in = 1.0 - ny_in * ny_in;
            if ny2_in <= 0.0 {
                (i32::MAX, i32::MIN)
            } else {
                let hw_in = ny2_in.sqrt() * rx_in;
                ((cx - hw_in).round() as i32, (cx + hw_in).round() as i32)
            }
        };
        for x in xl_out..xr_out {
            let inside_inner = x >= xl_in && x < xr_in;
            let do_draw = match kind {
                0 => !inside_inner, // frame
                1..=4 => true,
                _ => false,
            };
            if !do_draw {
                continue;
            }
            // Arc-angle gate. Mac convention 0°=north CW; the atan2 below
            // converts screen-space (y down) → Mac angle.
            if let Some((a_start, a_end)) = arc_range {
                if !a_start.is_finite() {
                    continue;
                }
                let angle = (-(y as f64 + 0.5 - cy)).atan2(x as f64 + 0.5 - cx);
                let mut mac_angle = 90.0 - angle.to_degrees();
                if mac_angle < 0.0 {
                    mac_angle += 360.0;
                }
                let in_range = (mac_angle >= a_start && mac_angle < a_end)
                    || (a_end > 360.0 && mac_angle + 360.0 < a_end);
                if !in_range {
                    continue;
                }
            }
            // Pick color from tracked FgColor/BkColor state and sample the
            // appropriate 8×8 pattern per pixel so paint/erase/fill honor
            // PnPat / BkPat / FillPat. Frame (kind=0) keeps solid fg
            // (pen-thickness frame doesn't participate in fill patterning).
            // Invert ignores fg/bg (pure XOR).
            // Imaging With QuickDraw 1994, Appendix A, A-7.
            let pat_row_idx = (y.rem_euclid(8)) as usize;
            let pat_bit = 1u8 << (7 - x.rem_euclid(8) as u32);
            let color = match kind {
                0 => fg_idx,
                1 => {
                    // paint — pn_pat, set bit = fg.
                    if pn_pat[pat_row_idx] & pat_bit != 0 {
                        fg_idx
                    } else {
                        bg_idx
                    }
                }
                2 => {
                    // erase — bk_pat, set bit = bg (IM:V V-66).
                    if bk_pat[pat_row_idx] & pat_bit != 0 {
                        bg_idx
                    } else {
                        fg_idx
                    }
                }
                4 => {
                    // fill — fill_pat, set bit = fg.
                    if fill_pat[pat_row_idx] & pat_bit != 0 {
                        fg_idx
                    } else {
                        bg_idx
                    }
                }
                _ => fg_idx,
            };
            if kind == 3 {
                if x < 0 || x >= sw || y < 0 || y >= sh {
                    continue;
                }
                if !dst_clip_contains(dst_clip, x, y) {
                    continue;
                }
                if let Some(rgn) = clip_region {
                    let inv_sx = if scale_x > 0.0 { 1.0 / scale_x } else { 1.0 };
                    let inv_sy = if scale_y > 0.0 { 1.0 / scale_y } else { 1.0 };
                    let pic_x =
                        ((x - dst_left as i32) as f64 * inv_sx + frame_left as f64).floor() as i32;
                    let pic_y =
                        ((y - dst_top as i32) as f64 * inv_sy + frame_top as f64).floor() as i32;
                    if !rgn.contains(pic_y, pic_x) {
                        continue;
                    }
                }
                if ps == 1 {
                    let addr = sb + (y as u32) * srb + (x as u32) / 8;
                    let bit = 7 - ((x as u32) % 8);
                    let byte = bus.read_byte(addr);
                    bus.write_byte(addr, byte ^ (1 << bit));
                } else {
                    let addr = sb + (y as u32) * srb + (x as u32);
                    let byte = bus.read_byte(addr);
                    bus.write_byte(addr, byte ^ 0xFF);
                }
            } else {
                plot_dst_pixel(
                    bus,
                    sb,
                    srb,
                    sw,
                    sh,
                    ps,
                    x,
                    y,
                    color,
                    frame_top,
                    frame_left,
                    dst_top,
                    dst_left,
                    scale_x,
                    scale_y,
                    clip_region,
                    dst_clip,
                );
            }
        }
    }
}

/// Render a PICT Poly record. `poly_ptr` points to the polySize word; the
/// record is `polySize(2) + bbox(8) + N*(v,h)(4)`. Frame variants use the
/// edge-only Bresenham path; paint/erase/invert/fill use a scanline-fill
/// rasteriser.
///
/// kind: 0=frame, 1=paint, 2=erase, 3=invert, 4=fill.
/// The fill color maps as in draw_shape_rect: paint/fill → 255
/// (Mac CLUT black), erase → 0 (white), invert → pixel XOR.
#[allow(clippy::too_many_arguments)]
fn render_pict_polygon(
    bus: &mut MacMemoryBus,
    poly_ptr: u32,
    kind: u8,
    pen_size: (i16, i16),
    pn_pat: [u8; 8],
    bk_pat: [u8; 8],
    fill_pat: [u8; 8],
    screen_mode: (u32, u32, u16, u16, u16),
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    fg_idx: u8,
    bg_idx: u8,
) {
    let poly_size = bus.read_word(poly_ptr) as u32;
    if poly_size < 10 {
        return;
    }
    let n = (poly_size - 10) / 4;
    if n < 2 {
        return;
    }
    let verts_ptr = poly_ptr + 10;
    let mut verts: Vec<(i16, i16)> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let v = bus.read_word(verts_ptr + i * 4) as i16;
        let h = bus.read_word(verts_ptr + i * 4 + 2) as i16;
        verts.push((v, h));
    }
    if kind == 0 {
        // framePoly: edge-only outline via draw_picture_line.
        for i in 0..verts.len() {
            let (v0, h0) = verts[i];
            let (v1, h1) = verts[(i + 1) % verts.len()];
            if v0 == v1 && h0 == h1 {
                continue;
            }
            draw_picture_line(
                bus,
                screen_mode,
                v0,
                h0,
                v1,
                h1,
                dst_top,
                dst_left,
                frame_top,
                frame_left,
                scale_x,
                scale_y,
                clip_region,
                dst_clip,
                pen_size,
                pn_pat,
                fg_idx,
            );
        }
        return;
    }

    // Scanline fill (even-odd rule). Build edge list excluding
    // horizontal edges (they contribute no crossings).
    struct Edge {
        y_min: i16,
        y_max: i16,
        x_at_ymin: f32,
        inv_slope: f32,
    }
    let mut edges: Vec<Edge> = Vec::with_capacity(verts.len());
    for i in 0..verts.len() {
        let (v0, h0) = verts[i];
        let (v1, h1) = verts[(i + 1) % verts.len()];
        if v0 == v1 {
            continue;
        }
        let (y_min, y_max, x_at_ymin) = if v0 < v1 {
            (v0, v1, h0 as f32)
        } else {
            (v1, v0, h1 as f32)
        };
        let inv_slope = (h1 as f32 - h0 as f32) / (v1 as f32 - v0 as f32);
        edges.push(Edge {
            y_min,
            y_max,
            x_at_ymin,
            inv_slope,
        });
    }
    if edges.is_empty() {
        return;
    }

    // Read polyBBox for scanline bounds.
    let bbox_top = bus.read_word(poly_ptr + 2) as i16;
    let bbox_left = bus.read_word(poly_ptr + 4) as i16;
    let bbox_bottom = bus.read_word(poly_ptr + 6) as i16;
    let bbox_right = bus.read_word(poly_ptr + 8) as i16;

    let (screen_base, screen_rb, screen_w, screen_h, pixel_size) = screen_mode;
    let screen_w = screen_w as i32;
    let screen_h = screen_h as i32;

    for y in bbox_top..bbox_bottom {
        // Gather x-intersections for edges active at this scanline.
        let mut xs: Vec<f32> = Vec::with_capacity(edges.len());
        for edge in &edges {
            if y < edge.y_min || y >= edge.y_max {
                continue;
            }
            let x = edge.x_at_ymin + (i32::from(y) - i32::from(edge.y_min)) as f32 * edge.inv_slope;
            xs.push(x);
        }
        if xs.len() < 2 {
            continue;
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Walk spans in pairs (even-odd rule: fill between pair 0-1,
        // 2-3, etc).
        let mut i = 0;
        while i + 1 < xs.len() {
            let x_start = xs[i].ceil() as i16;
            let x_end = xs[i + 1].ceil() as i16;
            for x in x_start..x_end.min(bbox_right) {
                if x < bbox_left {
                    continue;
                }
                // Clip region check in picture-space.
                if let Some(rgn) = clip_region {
                    if !rgn.contains(y as i32, x as i32) {
                        continue;
                    }
                }
                // Map picture-space (y, x) to dst-space. Widen to i32
                // before arithmetic — POD ships PICTs whose poly bbox
                // and the surrounding frame straddle the i16 range, so
                // the literal i16 subtractions panic on overflow.
                let dx = ((i32::from(x) - i32::from(bbox_left)) as f64 * scale_x
                    + (i32::from(dst_left) + i32::from(bbox_left) - i32::from(frame_left)) as f64)
                    as i32;
                let dy = ((i32::from(y) - i32::from(bbox_top)) as f64 * scale_y
                    + (i32::from(dst_top) + i32::from(bbox_top) - i32::from(frame_top)) as f64)
                    as i32;
                // Sample the appropriate 8×8 pattern at (dx mod 8, dy mod 8)
                // so paint/erase/fill honor PnPat / BkPat / FillPat. Pattern
                // set bit → "on" color (fg for paint/fill, bg for erase);
                // clear bit → "off" color. Imaging With QuickDraw 1994, A-7.
                let pat_row_idx = (dy.rem_euclid(8)) as usize;
                let pat_bit = 1u8 << (7 - dx.rem_euclid(8) as u32);
                match kind {
                    1 => {
                        // paintPoly — pn_pat.
                        let bit_set = pn_pat[pat_row_idx] & pat_bit != 0;
                        let color = if bit_set { fg_idx } else { bg_idx };
                        write_pixel_clipped(
                            bus,
                            screen_base,
                            screen_rb,
                            dx,
                            dy,
                            color,
                            screen_w,
                            screen_h,
                            pixel_size,
                            dst_clip,
                        );
                    }
                    4 => {
                        // fillPoly — fill_pat.
                        let bit_set = fill_pat[pat_row_idx] & pat_bit != 0;
                        let color = if bit_set { fg_idx } else { bg_idx };
                        write_pixel_clipped(
                            bus,
                            screen_base,
                            screen_rb,
                            dx,
                            dy,
                            color,
                            screen_w,
                            screen_h,
                            pixel_size,
                            dst_clip,
                        );
                    }
                    2 => {
                        // erasePoly — bk_pat. Set bit → bg, clear → fg
                        // (per IM:V V-66 "erase paints in background color";
                        // bk_pat's set bits are the dominant erase color).
                        let bit_set = bk_pat[pat_row_idx] & pat_bit != 0;
                        let color = if bit_set { bg_idx } else { fg_idx };
                        write_pixel_clipped(
                            bus,
                            screen_base,
                            screen_rb,
                            dx,
                            dy,
                            color,
                            screen_w,
                            screen_h,
                            pixel_size,
                            dst_clip,
                        );
                    }
                    3 => {
                        // invert: XOR pixel value
                        if dx < 0 || dx >= screen_w || dy < 0 || dy >= screen_h {
                            continue;
                        }
                        if !dst_clip_contains(dst_clip, dx, dy) {
                            continue;
                        }
                        if pixel_size == 8 {
                            let addr = screen_base + (dy as u32) * screen_rb + (dx as u32);
                            let old = bus.read_byte(addr);
                            bus.write_byte(addr, old ^ 0xFF);
                        } else {
                            // 1bpp not commonly hit by PICT invert;
                            // skip for now.
                        }
                    }
                    _ => {}
                }
            }
            i += 2;
        }
    }
}

/// Draw a line between two picture-space points using Bresenham's
/// algorithm. Plots each pixel through the picture→dst coord transform
/// so scaling / clipping match the shape-opcode handlers.
///
/// Honors PICT PnSize (tracked via state opcode 0x07): each Bresenham
/// pixel stamps a pen_h × pen_w rectangle in dst-space, scaled by the
/// picture→dst ratio per IM:V V-102. Pen sizes of (1, 1) take the fast
/// single-pixel path; (0, 0) suppresses the draw entirely per IM:I I-170
/// ("if either component is 0 or negative, no drawing is performed").
#[allow(clippy::too_many_arguments)]
fn draw_picture_line(
    bus: &mut MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    v0: i16,
    h0: i16,
    v1: i16,
    h1: i16,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    pen_size: (i16, i16),
    pn_pat: [u8; 8],
    fg_idx: u8,
) {
    let (pen_h, pen_w) = pen_size;
    if pen_h <= 0 || pen_w <= 0 {
        return;
    }
    // All-zero PnPat means the pen is transparent per the same IM:I I-170
    // rule that gates pen size. Fast-skip.
    if pn_pat == [0u8; 8] {
        return;
    }
    // Scale pen dimensions per picture→dst ratio, floor at 1 pixel
    // so a pen of 1 stays visible at any scale.
    let stamp_w = ((pen_w as f64 * scale_x).round() as i32).max(1);
    let stamp_h = ((pen_h as f64 * scale_y).round() as i32).max(1);
    let (screen_base, screen_rb, screen_w, screen_h, pixel_size) = screen_mode;
    let screen_w = screen_w as i32;
    let screen_h = screen_h as i32;
    // Fast path for solid pen skips the per-pixel pn_pat lookup. Every
    // other pattern goes through the 8×8 mod-sampling plot branch below.
    let solid_black = pn_pat == [0xFFu8; 8];
    let plot = |bus: &mut MacMemoryBus, cx: i32, cy: i32| {
        if stamp_w == 1 && stamp_h == 1 {
            if solid_black {
                write_pixel_clipped(
                    bus,
                    screen_base,
                    screen_rb,
                    cx,
                    cy,
                    fg_idx,
                    screen_w,
                    screen_h,
                    pixel_size,
                    dst_clip,
                );
            } else {
                let row = pn_pat[cy.rem_euclid(8) as usize];
                let bit = 1u8 << (7 - cx.rem_euclid(8));
                if row & bit != 0 {
                    write_pixel_clipped(
                        bus,
                        screen_base,
                        screen_rb,
                        cx,
                        cy,
                        fg_idx,
                        screen_w,
                        screen_h,
                        pixel_size,
                        dst_clip,
                    );
                }
            }
        } else {
            for dy in 0..stamp_h {
                for dx in 0..stamp_w {
                    let ox = cx + dx;
                    let oy = cy + dy;
                    if solid_black {
                        write_pixel_clipped(
                            bus,
                            screen_base,
                            screen_rb,
                            ox,
                            oy,
                            fg_idx,
                            screen_w,
                            screen_h,
                            pixel_size,
                            dst_clip,
                        );
                    } else {
                        let row = pn_pat[oy.rem_euclid(8) as usize];
                        let bit = 1u8 << (7 - ox.rem_euclid(8));
                        if row & bit != 0 {
                            write_pixel_clipped(
                                bus,
                                screen_base,
                                screen_rb,
                                ox,
                                oy,
                                fg_idx,
                                screen_w,
                                screen_h,
                                pixel_size,
                                dst_clip,
                            );
                        }
                    }
                }
            }
        }
    };
    let mut x0 = h0 as i32;
    let mut y0 = v0 as i32;
    let x1 = h1 as i32;
    let y1 = v1 as i32;
    let dx = (x1 - x0).abs();
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy: i32 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        let plot_here = match clip_region {
            Some(rgn) => rgn.contains(y0, x0),
            None => true,
        };
        if plot_here {
            let x = ((x0 - i32::from(frame_left)) as f64 * scale_x + dst_left as f64) as i32;
            let y = ((y0 - i32::from(frame_top)) as f64 * scale_y + dst_top as f64) as i32;
            plot(bus, x, y);
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// Render PICT text at pen (v, h) in picture coordinates. Pixels are
/// plotted through `write_pixel` after the picture→dst coord transform so
/// scaling + clipping match the shape-opcode paths.
#[allow(clippy::too_many_arguments)]
fn draw_picture_text(
    bus: &mut MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    pen_v: i16,
    pen_h: i16,
    text_ptr: u32,
    len: u32,
    font_id: i16,
    font_size: i16,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    fg_idx: u8,
    bg_idx: u8,
    tx_mode: i16,
) {
    let (screen_base, screen_rb, screen_w, screen_h, pixel_size) = screen_mode;
    let screen_w = screen_w as i32;
    let screen_h = screen_h as i32;
    let mut cur_h: i32 = pen_h as i32;
    let inv_sx = if scale_x > 0.0 { 1.0 / scale_x } else { 1.0 };
    let inv_sy = if scale_y > 0.0 { 1.0 / scale_y } else { 1.0 };
    for i in 0..len {
        let ch = bus.read_byte(text_ptr + i) as char;
        if let Some((glyph, data)) = crate::quickdraw::text::get_glyph(font_id, font_size, ch) {
            let gx0 = cur_h + glyph.origin_x as i32;
            let gy0 = pen_v as i32 + glyph.origin_y as i32;
            let gw = glyph.width as usize;
            let gh = glyph.height as usize;
            for row in 0..gh {
                for col in 0..gw {
                    let idx = glyph.data_offset + row * gw + col;
                    if idx >= data.len() || data[idx] < 128 {
                        continue;
                    }
                    let pic_x = gx0 + col as i32;
                    let pic_y = gy0 + row as i32;
                    if let Some(rgn) = clip_region {
                        if !rgn.contains(pic_y, pic_x) {
                            continue;
                        }
                    }
                    let x =
                        ((pic_x - i32::from(frame_left)) as f64 * scale_x + dst_left as f64) as i32;
                    let y =
                        ((pic_y - i32::from(frame_top)) as f64 * scale_y + dst_top as f64) as i32;
                    let _ = inv_sx;
                    let _ = inv_sy;
                    // Honor TxMode. srcCopy (0) / srcOr (1) overwrite;
                    // srcXor (2) XORs the dst byte with fg_idx; srcBic (3)
                    // clears dst to bg_idx at glyph pixels. Modes >= 32 fall
                    // back to plain overwrite.
                    if x < 0 || x >= screen_w || y < 0 || y >= screen_h {
                        continue;
                    }
                    if !dst_clip_contains(dst_clip, x, y) {
                        continue;
                    }
                    if pixel_size == 8 {
                        let addr = screen_base + (y as u32) * screen_rb + (x as u32);
                        match tx_mode & 0x3F {
                            2 => {
                                let old = bus.read_byte(addr);
                                bus.write_byte(addr, old ^ fg_idx);
                            }
                            3 => {
                                bus.write_byte(addr, bg_idx);
                            }
                            _ => {
                                bus.write_byte(addr, fg_idx);
                            }
                        }
                    } else {
                        write_pixel_clipped(
                            bus,
                            screen_base,
                            screen_rb,
                            x,
                            y,
                            fg_idx,
                            screen_w,
                            screen_h,
                            pixel_size,
                            dst_clip,
                        );
                    }
                }
            }
            cur_h += glyph.advance as i32;
        } else {
            cur_h += 6;
        }
    }
}

/// Compute total glyph advance for a PICT text run without drawing.
fn text_advance(bus: &MacMemoryBus, text_ptr: u32, len: u32, font_id: i16, font_size: i16) -> i16 {
    let mut w: i32 = 0;
    for i in 0..len {
        let ch = bus.read_byte(text_ptr + i) as char;
        if let Some((g, _)) = crate::quickdraw::text::get_glyph(font_id, font_size, ch) {
            w += g.advance as i32;
        } else {
            w += 6;
        }
    }
    w.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Find the closest index in a CLUT for a given 16-bit RGB color.
/// Quantizes to 5-bit precision then compares in 8-bit space, matching
/// the Mac's MakeITable 32x32x32 inverse table approach.
/// Imaging With QuickDraw 1994, p. 4-82
pub(crate) fn closest_clut_index(r: u16, g: u16, b: u16, clut: &[[u16; 3]; 256]) -> u8 {
    // ITable-style 4-bit-per-channel quantized lookup, gated by env var
    // for initial rollout. The Mac ROM's MakeITable uses a default 4-bit
    // inverse table: each 16-bit input channel is quantized to its top
    // 4 bits, giving 16x16x16 = 4096 cube cells. Within each cell, the
    // first-encountered CLUT entry whose top-4-bits match is used.
    // This produces different results than full-precision Euclidean
    // closest-match when the input is numerically close to two different
    // CLUT entries but falls in the same 4-bit cube as one of them.
    //
    // Example: input (F0F0, F0F0, F0F0) has 4-bit cube (F, F, F);
    //   clut[0] = (FFFF, FFFF, FFFF) has cube (F, F, F)   -> MATCH, idx 0
    //   clut[245] = (EEEE, EEEE, EEEE) has cube (E, E, E) -> different cube
    // Full-precision Euclidean would pick 245 (exact-distance from F0F0).
    // ITable picks 0 (first-bin-match). The Mac ROM uses ITable.
    // Imaging With QuickDraw 1994, p. 4-82 (MakeITable, default res 4)
    if clut_match_itable_enabled() {
        // Opt-in path: cached ITable from the System 8bpp CLUT. Improves
        // splash fidelity but regresses some menu renders — the System
        // ITable picks dst indices assuming the System CLUT, while games
        // may have overwritten device_clut entries via SetEntries.
        //
        // Correct long-term fix: rebuild the ITable when the Palette
        // Manager changes the GDevice CTab (not on every SetEntries).
        let _ = clut;
        return crate::trap::TrapDispatcher::standard_itable_lookup(r, g, b);
    }

    // QuickDraw's inverse tables pin exact white/black to the first/last
    // entries when the CLUT endpoints are white/black, instead of picking an
    // arbitrary duplicate from the 8bpp color cube. During all-black fade
    // steps index 0 can temporarily collapse to black too; preserve canonical
    // black drawing by preferring index 255 in that case.
    // Imaging With QuickDraw 1994, p. 4-82
    // references/executor/src/quickdraw/qColorMgr.cpp (MakeITable)
    let rgb = [r, g, b];
    if rgb == [0, 0, 0] && clut[255] == [0, 0, 0] {
        return 255;
    }
    if rgb == [0xFFFF, 0xFFFF, 0xFFFF] && clut[0] == [0xFFFF, 0xFFFF, 0xFFFF] {
        return 0;
    }
    if rgb == clut[255] {
        return 255;
    }
    if rgb == clut[0] {
        return 0;
    }

    // Grayscale source colors should stay on the destination grayscale ramp
    // when one is available. A pure Euclidean search across the 8bpp system
    // palette can pick tinted cube entries that are numerically close but
    // visibly wrong for grayscale title art (notably EV's splash PICTs).
    if r == g && g == b {
        let mut best_gray_idx = None;
        let mut best_gray_dist = i64::MAX;
        for (idx, entry) in clut.iter().enumerate() {
            if entry[0] != entry[1] || entry[1] != entry[2] {
                continue;
            }
            let dr = i64::from(r) - i64::from(entry[0]);
            let d = dr * dr;
            if d < best_gray_dist {
                best_gray_dist = d;
                best_gray_idx = Some(idx as u8);
                if d == 0 {
                    break;
                }
            }
        }
        if let Some(idx) = best_gray_idx {
            return idx;
        }
    }

    // Use full 16-bit precision for the distance calculation.
    // The Mac Color Manager's MakeITable quantizes to 5-bit bins for the
    // lookup table index, but the actual nearest-match is computed from
    // full-precision color values.
    // Imaging With QuickDraw 1994, p. 4-82
    let mut best_idx = 0u8;
    let mut best_dist = i64::MAX;
    for (idx, entry) in clut.iter().enumerate() {
        let dr = i64::from(r) - i64::from(entry[0]);
        let dg = i64::from(g) - i64::from(entry[1]);
        let db = i64::from(b) - i64::from(entry[2]);
        let d = dr * dr + dg * dg + db * db;
        if d < best_dist {
            best_dist = d;
            best_idx = idx as u8;
            if d == 0 {
                break;
            }
        }
    }
    best_idx
}

/// Build a 256-entry mapping table from source CLUT indices to device CLUT indices.
/// For each source palette entry, finds the closest match in the device CLUT.
/// This is the core of CopyBits color translation for indexed pixmaps.
fn build_src_to_dst_table(src_clut: &[[u16; 3]], device_clut: &[[u16; 3]; 256]) -> [u8; 256] {
    let cache = SRC_TO_DST_TABLE_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut entries) = cache.lock() {
        if let Some(pos) = entries.iter().position(|entry| {
            entry.src_clut.as_slice() == src_clut && entry.dst_clut == *device_clut
        }) {
            let entry = entries.remove(pos);
            let table = entry.table;
            entries.push(entry);
            return table;
        }
    }

    let table = build_src_to_dst_table_uncached(src_clut, device_clut);
    if let Ok(mut entries) = cache.lock() {
        entries.push(SrcToDstTableCacheEntry {
            src_clut: src_clut.to_vec(),
            dst_clut: *device_clut,
            table,
        });
        if entries.len() > SRC_TO_DST_TABLE_CACHE_LIMIT {
            entries.remove(0);
        }
    }
    table
}

fn build_src_to_dst_table_uncached(
    src_clut: &[[u16; 3]],
    device_clut: &[[u16; 3]; 256],
) -> [u8; 256] {
    let mut table = [0u8; 256];
    if super::dispatch::TrapDispatcher::uses_standard_mac_4bpp_gworld_clut(device_clut)
        && device_clut[16..]
            .iter()
            .all(|entry| *entry == device_clut[15])
    {
        for (index, entry) in src_clut.iter().take(256).enumerate() {
            table[index] = super::dispatch::TrapDispatcher::standard_mac_4bpp_gworld_color2index(
                entry[0], entry[1], entry[2],
            );
        }
        return table;
    }
    if should_preserve_source_palette_indices(src_clut, device_clut) {
        for (i, slot) in table.iter_mut().enumerate() {
            *slot = i as u8;
        }
        return table;
    }
    // Color QuickDraw matches indexed PICT colors through the current
    // GDevice's inverse table. Build that 4-bit table once and consult it for
    // each source ColorSpec. Dense grayscale PICTs retain their diagnostic
    // legacy opt-out because their older luminance-only matcher predates the
    // general Color Manager path.
    let use_itable = !pict_clut_is_dense_grayscale(src_clut) || !clut_match_legacy_gray_enabled();
    if use_itable {
        let itable = build_device_itable(device_clut);
        for (i, entry) in src_clut.iter().enumerate() {
            if i >= 256 {
                break;
            }
            table[i] = if *entry == device_clut[i] {
                i as u8
            } else if let Some(index) = device_clut
                .iter()
                .position(|destination| destination == entry)
            {
                // Color2Index returns an exact CTable entry before consulting
                // the quantized inverse-table cell. This matters when source
                // and device palettes contain the same colors at different
                // indices: a lower-index color in the same 4-bit cell must not
                // replace an exact match.
                index as u8
            } else if let Some(index) = device_clut.iter().position(|destination| {
                destination[0] >> 8 == entry[0] >> 8
                    && destination[1] >> 8 == entry[1] >> 8
                    && destination[2] >> 8 == entry[2] >> 8
            }) {
                // Indexed 8-bit video exposes the most-significant byte of
                // each 16-bit RGB component. Some classic PICT generators
                // leave bookkeeping values in the low bytes, so colors that
                // are identical on the device must match before the coarser
                // inverse-table lookup. Imaging With QuickDraw 1994, p. 4-13.
                index as u8
            } else {
                let qr = (entry[0] >> 12) as u32;
                let qg = (entry[1] >> 12) as u32;
                let qb = (entry[2] >> 12) as u32;
                itable[((qr << 8) | (qg << 4) | qb) as usize]
            };
        }
        return table;
    }
    for (i, entry) in src_clut.iter().enumerate() {
        if i >= 256 {
            break;
        }
        table[i] = if *entry == device_clut[i] {
            // If a PICT's source table and the active port table share an
            // exact color at the same index, preserve the authored pixel
            // value. Games such as Prince of Destruction duplicate colors
            // in custom palettes; a nearest-color search can otherwise move
            // art to an earlier duplicate with a very different intended use.
            i as u8
        } else if pict_clut_is_dense_grayscale(src_clut)
            && entry[0] == entry[1]
            && entry[1] == entry[2]
        {
            closest_grayscale_luminance_index(entry[0], device_clut)
        } else {
            closest_clut_index(entry[0], entry[1], entry[2], device_clut)
        };
    }
    table
}

#[cfg(test)]
fn clear_src_to_dst_table_cache_for_tests() {
    if let Some(cache) = SRC_TO_DST_TABLE_CACHE.get() {
        cache.lock().expect("src-to-dst cache lock").clear();
    }
}

/// Build a 4-bit-per-channel inverse table (16 × 16 × 16 = 4096 cells)
/// against the given `device_clut`. QuickDraw seeds the quantized cube in
/// ascending ColorTable order, then performs a multi-source flood fill over
/// the six axis-adjacent cells. A nonblack shade displaces a black seed in the
/// darkest cell; exact black is resolved separately before ITable lookup.
/// The cell index is `qr<<8 | qg<<4 | qb`.
///
/// Cost is linear in the 4096 cells. Called once per
/// `build_src_to_dst_table` invocation (once per PICT parse), not per pixel.
/// Matches the Mac ROM MakeITable semantics for the device's active GDevice
/// at the moment of DrawPicture.
///
/// Imaging With QuickDraw 1994, p. 4-82 (MakeITable, default 4 bits)
fn build_device_itable(device_clut: &[[u16; 3]; 256]) -> [u8; 4096] {
    let mut table = [0u8; 4096];
    let mut filled = [false; 4096];
    let mut queue = std::collections::VecDeque::with_capacity(4096);

    for (index, color) in device_clut.iter().copied().enumerate() {
        let cell = (usize::from(color[0] >> 12) << 8)
            | (usize::from(color[1] >> 12) << 4)
            | usize::from(color[2] >> 12);
        if !filled[cell] {
            filled[cell] = true;
            table[cell] = index as u8;
            queue.push_back(cell);
        } else if cell == 0 && device_clut[table[cell] as usize] == [0; 3] && color != [0; 3] {
            table[cell] = index as u8;
        }
    }

    const DIRECTIONS: [[i8; 3]; 6] = [
        [0, 0, 1],
        [0, 0, -1],
        [0, 1, 0],
        [0, -1, 0],
        [1, 0, 0],
        [-1, 0, 0],
    ];

    while let Some(cell) = queue.pop_front() {
        let components = [
            ((cell >> 8) & 0x0F) as i8,
            ((cell >> 4) & 0x0F) as i8,
            (cell & 0x0F) as i8,
        ];
        for direction in DIRECTIONS {
            let next_components = [
                components[0] + direction[0],
                components[1] + direction[1],
                components[2] + direction[2],
            ];
            if next_components
                .iter()
                .any(|&component| !(0..16).contains(&component))
            {
                continue;
            }
            let next = ((next_components[0] as usize) << 8)
                | ((next_components[1] as usize) << 4)
                | next_components[2] as usize;
            if !filled[next] {
                filled[next] = true;
                table[next] = table[cell];
                queue.push_back(next);
            }
        }
    }
    table
}

fn closest_grayscale_luminance_index(luma: u16, clut: &[[u16; 3]; 256]) -> u8 {
    let grayscale_entries = clut
        .iter()
        .enumerate()
        .filter_map(|(idx, entry)| (entry[0] == entry[1] && entry[1] == entry[2]).then_some(idx))
        .collect::<Vec<_>>();
    let distinct_grays = grayscale_entries
        .iter()
        .map(|&idx| clut[idx][0])
        .collect::<std::collections::BTreeSet<_>>();
    let search_indices = if distinct_grays.len() >= 8 {
        grayscale_entries
    } else {
        (0..clut.len()).collect()
    };

    // Opt-in cached-ITable path for grayscale (matches the colour
    // path's SYSTEMLESS_CLUT_MATCH_ITABLE gate). This remains gated because
    // the direct luminance path better matches menu artwork in common apps.
    if clut_match_itable_enabled() {
        let _ = clut;
        let _ = search_indices;
        return crate::trap::TrapDispatcher::standard_itable_lookup(luma, luma, luma);
    }
    let match_target = luma;

    let mut best_idx = 0u8;
    let mut best_luma_diff = u64::MAX;
    let mut best_chroma = u64::MAX;
    for idx in search_indices {
        let entry = clut[idx];
        let entry_luma =
            (u64::from(entry[0]) * 30 + u64::from(entry[1]) * 59 + u64::from(entry[2]) * 11) / 100;
        let luma_diff = entry_luma.abs_diff(u64::from(match_target));
        let max_component = entry[0].max(entry[1]).max(entry[2]);
        let min_component = entry[0].min(entry[1]).min(entry[2]);
        let chroma = u64::from(max_component - min_component);
        if luma_diff < best_luma_diff || (luma_diff == best_luma_diff && chroma < best_chroma) {
            best_idx = idx as u8;
            best_luma_diff = luma_diff;
            best_chroma = chroma;
        }
    }
    best_idx
}

fn pict_clut_is_dense_grayscale(clut: &[[u16; 3]]) -> bool {
    let grayscale_entries = clut
        .iter()
        .take(192)
        .filter(|rgb| rgb[0] == rgb[1] && rgb[1] == rgb[2] && rgb[0] != 0)
        .count();
    grayscale_entries >= 96
}

/// True if `clut` looks like the canonical Mac 8bpp system palette.
/// Used by draw_picture to distinguish a scene-defining CTab from
/// canonical helper CTabs inside a multi-PICT stream. Checks a handful
/// of landmark entries — custom CTabs are statistically unlikely to
/// have all of them match.
fn clut_resembles_canonical_8bpp(clut: &[[u16; 3]]) -> bool {
    // Landmark entries from the 6x6x6 colour cube used across Mac OS:
    //   idx 0  = (FFFF, FFFF, FFFF) — white
    //   idx 1  = (FFFF, FFFF, CCCC)
    //   idx 16 = (FFFF, 9999, 3333)
    //   idx 42 = (CCCC, CCCC, FFFF)
    //   idx 255= (0000, 0000, 0000) — black
    if clut.len() < 256 {
        return false;
    }
    clut[0] == [0xFFFF, 0xFFFF, 0xFFFF]
        && clut[1] == [0xFFFF, 0xFFFF, 0xCCCC]
        && clut[16] == [0xFFFF, 0x9999, 0x3333]
        && clut[42] == [0xCCCC, 0xCCCC, 0xFFFF]
        && clut[255] == [0x0000, 0x0000, 0x0000]
}

fn should_preserve_source_palette_indices(
    src_clut: &[[u16; 3]],
    device_clut: &[[u16; 3]; 256],
) -> bool {
    // Diagnostic gate: SYSTEMLESS_PICT_IDENTITY_REMAP=1 forces identity
    // mapping (src idx = dst idx, no remap through device_clut) for every
    // 256-entry source CTab.
    if pict_identity_remap_enabled() && src_clut.len() == 256 {
        return true;
    }
    // Preserve identity only when source and destination really use the same
    // table. Grayscale PICTs drawn into the canonical 8bpp system palette
    // still need color translation; preserving raw source indices there turns
    // gray art into the orange/green system cube.
    if src_clut.len() == 256 {
        src_clut.iter().zip(device_clut.iter()).all(|(s, d)| s == d)
    } else {
        false
    }
}

/// Map a source bitmap coordinate from a CopyBits srcRect into its dstRect.
/// BitsRect, PackBitsRect, and DirectBitsRect use CopyBits-style rectangles,
/// so pixels outside srcRect must not be transferred.
/// Inside Macintosh Volume I, I-158; Imaging With QuickDraw 1994, A-17
fn map_src_coord(
    src_coord: i32,
    src_start: i16,
    src_end: i16,
    dst_start: i16,
    dst_end: i16,
) -> Option<i32> {
    let src_span = i32::from(src_end) - i32::from(src_start);
    let dst_span = i32::from(dst_end) - i32::from(dst_start);
    if src_span <= 0 || dst_span <= 0 {
        return None;
    }

    let rel = src_coord - i32::from(src_start);
    if rel < 0 || rel >= src_span {
        return None;
    }

    Some(i32::from(dst_start) + (rel * dst_span) / src_span)
}

fn map_src_pixel_span(
    src_coord: i32,
    src_start: i16,
    src_end: i16,
    pic_dst_start: i16,
    pic_dst_end: i16,
    frame_start: i16,
    dst_start: i16,
    scale: f64,
) -> Option<(i32, i32, i32)> {
    // CopyBits scales every source pixel to cover its corresponding area in
    // the destination rectangle; mapping only the pixel origin leaves holes
    // whenever an indexed PICT is enlarged.
    // Imaging With QuickDraw (1994), pp. 3-113, 3-116
    let src_span = i32::from(src_end) - i32::from(src_start);
    let pic_dst_span = i32::from(pic_dst_end) - i32::from(pic_dst_start);
    let rel = src_coord - i32::from(src_start);
    if src_span <= 0 || pic_dst_span <= 0 || rel < 0 || rel >= src_span {
        return None;
    }

    let centered_edge = |source_edge: i32| {
        ((source_edge as f64 * pic_dst_span as f64 / src_span as f64) - 0.5)
            .ceil()
            .clamp(0.0, pic_dst_span as f64) as i32
    };
    let pic_start = i32::from(pic_dst_start) + centered_edge(rel);
    let pic_end = i32::from(pic_dst_start) + centered_edge(rel + 1);
    let screen_start = (((pic_start - i32::from(frame_start)) as f64 * scale) - 0.5).ceil() as i32
        + i32::from(dst_start);
    let screen_end = (((pic_end - i32::from(frame_start)) as f64 * scale) - 0.5).ceil() as i32
        + i32::from(dst_start);
    Some((pic_start, screen_start, screen_end))
}

/// Parse BitsRect / BitsRgn (1bpp bitmap, opcode 0x0090/0x0091)
fn parse_bits_rect(
    bus: &mut MacMemoryBus,
    mut pos: u32,
    has_rgn: bool,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    screen_mode: (u32, u32, u16, u16, u16),
    _device_clut: &[[u16; 3]; 256],
    fg_idx: u8,
    bg_idx: u8,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
) -> u32 {
    // Read BitMap structure (not full PixMap)
    let row_bytes = bus.read_word(pos) & 0x3FFF;
    pos += 2;
    let bounds_top = bus.read_word(pos) as i16;
    pos += 2;
    let bounds_left = bus.read_word(pos) as i16;
    pos += 2;
    let bounds_bottom = bus.read_word(pos) as i16;
    pos += 2;
    let bounds_right = bus.read_word(pos) as i16;
    pos += 2;
    // srcRect
    let src_top = bus.read_word(pos) as i16;
    pos += 2;
    let src_left = bus.read_word(pos) as i16;
    pos += 2;
    let src_bottom = bus.read_word(pos) as i16;
    pos += 2;
    let src_right = bus.read_word(pos) as i16;
    pos += 2;
    // dstRect
    let pic_dst_top = bus.read_word(pos) as i16;
    pos += 2;
    let pic_dst_left = bus.read_word(pos) as i16;
    pos += 2;
    let pic_dst_bottom = bus.read_word(pos) as i16;
    pos += 2;
    let pic_dst_right = bus.read_word(pos) as i16;
    pos += 2;
    // mode
    let mode = bus.read_word(pos);
    pos += 2;

    if trace_pict_enabled() {
        eprintln!(
            "[PICT] Bits{} mode={} src=({},{}..{},{} ) dst=({},{}..{},{} )",
            if has_rgn { "Rgn" } else { "Rect" },
            mode,
            src_top,
            src_left,
            src_bottom,
            src_right,
            pic_dst_top,
            pic_dst_left,
            pic_dst_bottom,
            pic_dst_right,
        );
    }

    if has_rgn {
        let rgn_size = bus.read_word(pos) as u32;
        pos += rgn_size;
    }

    let width = (bounds_right - bounds_left).max(0) as u32;
    let height = (bounds_bottom - bounds_top).max(0) as u32;
    let (screen_base, screen_rb, screen_w, screen_h, scrn_ps) = (
        screen_mode.0,
        screen_mode.1,
        screen_mode.2 as i32,
        screen_mode.3 as i32,
        screen_mode.4,
    );

    // Read unpacked bitmap data (row_bytes * height bytes)
    for row in 0..height {
        let src_y = i32::from(bounds_top) + row as i32;
        let mapped_pic_y = map_src_coord(src_y, src_top, src_bottom, pic_dst_top, pic_dst_bottom);
        for col_byte in 0..row_bytes as u32 {
            let byte = bus.read_byte(pos);
            pos += 1;
            for bit in 0..8u32 {
                let px = col_byte * 8 + bit;
                if px >= width {
                    continue;
                }
                let is_set = (byte & (1 << (7 - bit))) != 0;
                let color_idx = if is_set {
                    fg_idx
                } else if mode == 0 {
                    bg_idx
                } else {
                    continue;
                };
                let Some(pic_y) = mapped_pic_y else {
                    continue;
                };
                let src_x = i32::from(bounds_left) + px as i32;
                let Some(pic_x) =
                    map_src_coord(src_x, src_left, src_right, pic_dst_left, pic_dst_right)
                else {
                    continue;
                };
                if clip_region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                    continue;
                }
                let x = ((pic_x - i32::from(frame_left)) as f64 * scale_x) as i32 + dst_left as i32;
                let y = ((pic_y - i32::from(frame_top)) as f64 * scale_y) as i32 + dst_top as i32;
                write_pixel_clipped(
                    bus,
                    screen_base,
                    screen_rb,
                    x,
                    y,
                    color_idx,
                    screen_w,
                    screen_h,
                    scrn_ps,
                    dst_clip,
                );
            }
        }
    }

    pos
}

/// Parse indexed PixMap forms of BitsRect/BitsRgn and PackBitsRect/PackBitsRgn.
fn parse_indexed_bits_rect(
    bus: &mut MacMemoryBus,
    mut pos: u32,
    has_rgn: bool,
    packed: bool,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    screen_mode: (u32, u32, u16, u16, u16),
    device_clut: &[[u16; 3]; 256],
    device_ct_seed: u32,
    fg_idx: u8,
    bg_idx: u8,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
) -> (u32, Option<Vec<[u16; 3]>>) {
    // These PICT opcodes start with rowBytes directly (no baseAddr).
    // Check if this is a PixMap (row_bytes high bit set) or BitMap
    let row_bytes_raw = bus.read_word(pos); // peek at rowBytes field (first word)
    let is_pixmap = (row_bytes_raw & 0x8000) != 0;

    let (new_pos, pm, colors16, src_ct_seed) = if is_pixmap {
        let (p, pm) = read_pixmap(bus, pos);
        let (p2, colors16, ct_seed) = read_color_table(bus, p);
        (p2, pm, Some(colors16), ct_seed)
    } else {
        // 1bpp BitMap: just rowBytes + bounds
        let row_bytes = bus.read_word(pos) & 0x3FFF;
        pos += 2;
        let bt = bus.read_word(pos) as i16;
        pos += 2;
        let bl = bus.read_word(pos) as i16;
        pos += 2;
        let bb = bus.read_word(pos) as i16;
        pos += 2;
        let br = bus.read_word(pos) as i16;
        pos += 2;
        let pm = PixMapInfo {
            row_bytes,
            bounds_top: bt,
            bounds_left: bl,
            bounds_bottom: bb,
            bounds_right: br,
            pixel_size: 1,
            cmp_count: 1,
            pack_type: 0,
        };
        (pos, pm, None, 0)
    };
    pos = new_pos;

    if trace_pict_enabled() {
        let kind = if packed { "PackBits" } else { "Bits" };
        // Include destination base + rowBytes so each PackBitsRect decode
        // can be correlated to the specific offscreen GWorld buffer it
        // writes into.
        eprintln!(
            "[PICT] {}{} pixelSize={} cmpCount={} rowBytes={} bounds=({}, {}, {}, {}) dstBase=${:08X} dstRowBytes={}",
            kind,
            if has_rgn { "Rgn" } else { "Rect" },
            pm.pixel_size,
            pm.cmp_count,
            pm.row_bytes,
            pm.bounds_top,
            pm.bounds_left,
            pm.bounds_bottom,
            pm.bounds_right,
            screen_mode.0,
            screen_mode.1,
        );
    }

    // srcRect
    let src_top = bus.read_word(pos) as i16;
    pos += 2;
    let src_left = bus.read_word(pos) as i16;
    pos += 2;
    let src_bottom = bus.read_word(pos) as i16;
    pos += 2;
    let src_right = bus.read_word(pos) as i16;
    pos += 2;
    // dstRect (within picture coordinates)
    let pic_dst_top = bus.read_word(pos) as i16;
    pos += 2;
    let pic_dst_left = bus.read_word(pos) as i16;
    pos += 2;
    let pic_dst_bottom = bus.read_word(pos) as i16;
    pos += 2;
    let pic_dst_right = bus.read_word(pos) as i16;
    pos += 2;
    // mode
    let mode = bus.read_word(pos);
    pos += 2;
    let mode_base = mode & 0x003F;

    if trace_pict_enabled() {
        let kind = if packed { "PackBits" } else { "Bits" };
        eprintln!(
            "[PICT] {}{} mode={} src=({},{}..{},{} ) dst=({},{}..{},{} )",
            kind,
            if has_rgn { "Rgn" } else { "Rect" },
            mode,
            src_top,
            src_left,
            src_bottom,
            src_right,
            pic_dst_top,
            pic_dst_left,
            pic_dst_bottom,
            pic_dst_right,
        );
    }

    if has_rgn {
        let rgn_size = bus.read_word(pos) as u32;
        pos += rgn_size;
    }

    let height = (pm.bounds_bottom - pm.bounds_top) as u32;
    let (screen_base, screen_rb, screen_w, screen_h, scrn_ps) = (
        screen_mode.0,
        screen_mode.1,
        screen_mode.2 as i32,
        screen_mode.3 as i32,
        screen_mode.4,
    );

    // Build source→destination CLUT mapping for indexed pixel formats.
    // Each PICT pixel index is remapped to the closest match in the
    // destination port's color table (passed as device_clut).
    //
    // Seed-match identity gate: ctSeed is a ColorTable identifier
    // (Inside Macintosh Volume V, V-48; Imaging With QuickDraw 1994,
    // 4-56). Some PICTs carry the depth-convention seed (for example
    // 8) even when their inline table differs from the active device
    // table, so a seed match alone is not enough to bypass RGB
    // translation. Preserve raw source indices only when the source and
    // destination tables actually match.
    let src_clut = colors16.as_deref().unwrap_or(&[][..]);
    // Black-and-white devices can display PixMaps in pictures, but color
    // pixels must be matched to the colors actually available in that
    // destination, not to the full 8bpp screen CLUT. Imaging With QuickDraw
    // 1994, pp. 4-13 and A-13; Inside Macintosh Volume V, V-57.
    let restricted_clut = indexed_destination_clut(device_clut, scrn_ps);
    let dst_clut = &restricted_clut;
    let seed_and_table_match = scrn_ps != 1
        && src_ct_seed != 0
        && src_ct_seed == device_ct_seed
        && should_preserve_source_palette_indices(src_clut, dst_clut);
    let src_to_dst = if seed_and_table_match {
        let mut t = [0u8; 256];
        for (i, slot) in t.iter_mut().enumerate() {
            *slot = i as u8;
        }
        t
    } else {
        build_src_to_dst_table(src_clut, dst_clut)
    };
    let src_to_dst_is_identity = src_to_dst
        .iter()
        .enumerate()
        .all(|(idx, &value)| value == idx as u8);
    let indexed_transfer = build_pict_indexed_transfer_table(
        mode_base,
        src_clut,
        &src_to_dst,
        dst_clut,
        fg_idx,
        bg_idx,
    );
    let can_direct_blit_8bpp_src_copy = !trace_pict_enabled()
        && pm.pixel_size == 8
        && can_blit_8bpp_src_copy_rows_fast(
            mode_base,
            &pm,
            src_top,
            src_left,
            src_bottom,
            src_right,
            pic_dst_top,
            pic_dst_left,
            pic_dst_bottom,
            pic_dst_right,
            scale_x,
            scale_y,
            scrn_ps,
            clip_region,
        );
    let mut traced_min_index = u8::MAX;
    let mut traced_max_index = 0u8;
    let mut traced_have_index = false;

    let mut update_trace_index_range = |row_data: &[u8]| {
        if !trace_pict_enabled() || pm.pixel_size != 8 {
            return;
        }
        for &pixel in row_data {
            traced_min_index = traced_min_index.min(pixel);
            traced_max_index = traced_max_index.max(pixel);
            traced_have_index = true;
        }
    };

    let mut row_data = Vec::with_capacity(pm.row_bytes as usize);
    let mut blit_scratch = Vec::new();

    if !packed || pm.row_bytes < 8 {
        // BitsRect/BitsRgn data is always unpacked. PackBits forms also
        // store small rows unpacked. Imaging With QuickDraw 1994, A-13.
        for row in 0..height {
            row_data.resize(pm.row_bytes as usize, 0);
            bus.read_bytes_into(pos, &mut row_data);
            pos += pm.row_bytes as u32;
            if can_direct_blit_8bpp_src_copy && row_data.len() >= pm.row_bytes as usize {
                let copied = try_blit_row_8bpp_src_copy_fast(
                    bus,
                    &row_data,
                    mode_base,
                    &pm,
                    &src_to_dst,
                    src_to_dst_is_identity,
                    row,
                    src_top,
                    src_left,
                    src_bottom,
                    src_right,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    pic_dst_top,
                    pic_dst_left,
                    pic_dst_bottom,
                    pic_dst_right,
                    scale_x,
                    scale_y,
                    screen_base,
                    screen_rb,
                    screen_w,
                    screen_h,
                    scrn_ps,
                    clip_region,
                    dst_clip,
                    &mut blit_scratch,
                );
                debug_assert!(copied);
                continue;
            }
            update_trace_index_range(&row_data);
            blit_row(
                bus,
                &row_data,
                mode_base,
                &pm,
                device_clut,
                &src_to_dst,
                src_to_dst_is_identity,
                &indexed_transfer,
                row,
                src_top,
                src_left,
                src_bottom,
                src_right,
                dst_top,
                dst_left,
                frame_top,
                frame_left,
                pic_dst_top,
                pic_dst_left,
                pic_dst_bottom,
                pic_dst_right,
                scale_x,
                scale_y,
                screen_base,
                screen_rb,
                screen_w,
                screen_h,
                scrn_ps,
                fg_idx,
                bg_idx,
                clip_region,
                dst_clip,
                &mut blit_scratch,
            );
        }
    } else if let Some(fast_pos) = try_blit_packbits_8bpp_src_copy_fast(
        bus,
        pos,
        height,
        &pm,
        mode_base,
        &src_to_dst,
        src_to_dst_is_identity,
        src_top,
        src_left,
        src_bottom,
        src_right,
        dst_top,
        dst_left,
        frame_top,
        frame_left,
        pic_dst_top,
        pic_dst_left,
        pic_dst_bottom,
        pic_dst_right,
        scale_x,
        scale_y,
        screen_base,
        screen_rb,
        screen_w,
        screen_h,
        scrn_ps,
        clip_region,
        dst_clip,
    ) {
        pos = fast_pos;
    } else {
        // PackBits compressed
        for row in 0..height {
            let map_during_unpack = can_direct_blit_8bpp_src_copy
                && pm.pixel_size == 8
                && !src_to_dst_is_identity
                && scrn_ps == 8;
            pos = if map_during_unpack {
                unpack_bits_with_byte_count_row_bytes_mapped_into(
                    bus,
                    pos,
                    pm.row_bytes,
                    pm.row_bytes,
                    &src_to_dst,
                    &mut row_data,
                )
            } else {
                unpack_bits_with_byte_count_row_bytes_into(
                    bus,
                    pos,
                    pm.row_bytes,
                    pm.row_bytes,
                    &mut row_data,
                )
            };
            if can_direct_blit_8bpp_src_copy && row_data.len() >= pm.row_bytes as usize {
                let copied = try_blit_row_8bpp_src_copy_fast(
                    bus,
                    &row_data,
                    mode_base,
                    &pm,
                    &src_to_dst,
                    src_to_dst_is_identity || map_during_unpack,
                    row,
                    src_top,
                    src_left,
                    src_bottom,
                    src_right,
                    dst_top,
                    dst_left,
                    frame_top,
                    frame_left,
                    pic_dst_top,
                    pic_dst_left,
                    pic_dst_bottom,
                    pic_dst_right,
                    scale_x,
                    scale_y,
                    screen_base,
                    screen_rb,
                    screen_w,
                    screen_h,
                    scrn_ps,
                    clip_region,
                    dst_clip,
                    &mut blit_scratch,
                );
                debug_assert!(copied);
                continue;
            }
            update_trace_index_range(&row_data);
            blit_row(
                bus,
                &row_data,
                mode_base,
                &pm,
                device_clut,
                &src_to_dst,
                src_to_dst_is_identity,
                &indexed_transfer,
                row,
                src_top,
                src_left,
                src_bottom,
                src_right,
                dst_top,
                dst_left,
                frame_top,
                frame_left,
                pic_dst_top,
                pic_dst_left,
                pic_dst_bottom,
                pic_dst_right,
                scale_x,
                scale_y,
                screen_base,
                screen_rb,
                screen_w,
                screen_h,
                scrn_ps,
                fg_idx,
                bg_idx,
                clip_region,
                dst_clip,
                &mut blit_scratch,
            );
        }
    }

    if trace_pict_enabled() && pm.pixel_size == 8 && traced_have_index {
        eprintln!(
            "[PICT] PackBits index range {}..={}",
            traced_min_index, traced_max_index
        );
    }

    // Return the 16-bit color table for 8bpp PICTs so the caller can
    // install it as the device CLUT if needed.
    (pos, if pm.pixel_size == 8 { colors16 } else { None })
}

/// Identity source-to-destination index map, for rows that already hold
/// destination indices.
static IDENTITY_INDEX_MAP: [u8; 256] = {
    let mut map = [0u8; 256];
    let mut index = 0;
    while index < 256 {
        map[index] = index as u8;
        index += 1;
    }
    map
};

/// Blit a decompressed row of pixel data to the screen.
fn blit_row(
    bus: &mut MacMemoryBus,
    row_data: &[u8],
    mode_base: u16,
    pm: &PixMapInfo,
    device_clut: &[[u16; 3]; 256],
    src_to_dst: &[u8; 256],
    src_to_dst_is_identity: bool,
    indexed_transfer: &[PictIndexedTransfer; 256],
    row: u32,
    src_top: i16,
    src_left: i16,
    src_bottom: i16,
    src_right: i16,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    pic_dst_top: i16,
    pic_dst_left: i16,
    pic_dst_bottom: i16,
    pic_dst_right: i16,
    scale_x: f64,
    scale_y: f64,
    screen_base: u32,
    screen_rb: u32,
    screen_w: i32,
    screen_h: i32,
    scrn_ps: u16,
    fg_idx: u8,
    bg_idx: u8,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    scratch: &mut Vec<u8>,
) {
    let width = (pm.bounds_right - pm.bounds_left).max(0) as u32;
    // A 1-bit srcCopy source onto an 8-bit screen is the 8-bit row path in
    // disguise: expand the row's bits to the destination indices they map to
    // (set -> fg, clear -> bg, exactly the per-pixel arm's choice) and let
    // that path place them, region and dst_clip spans included. EV
    // Override's intro draws a zoom sequence of region-masked 1-bit frames.
    if !trace_pict_enabled() && pm.pixel_size == 1 && mode_base == 0 && scrn_ps == 8 {
        let expanded: Vec<u8> = row_data
            .iter()
            .flat_map(|&byte| {
                (0..8).map(move |bit| {
                    if byte & (0x80 >> bit) != 0 {
                        fg_idx
                    } else {
                        bg_idx
                    }
                })
            })
            .take(width as usize)
            .collect();
        if try_blit_row_8bpp_src_copy_fast(
            bus,
            &expanded,
            mode_base,
            pm,
            &IDENTITY_INDEX_MAP,
            true,
            row,
            src_top,
            src_left,
            src_bottom,
            src_right,
            dst_top,
            dst_left,
            frame_top,
            frame_left,
            pic_dst_top,
            pic_dst_left,
            pic_dst_bottom,
            pic_dst_right,
            scale_x,
            scale_y,
            screen_base,
            screen_rb,
            screen_w,
            screen_h,
            scrn_ps,
            clip_region,
            dst_clip,
            scratch,
        ) {
            return;
        }
    }
    if !trace_pict_enabled()
        && pm.pixel_size == 8
        && try_blit_row_8bpp_src_copy_fast(
            bus,
            row_data,
            mode_base,
            pm,
            src_to_dst,
            src_to_dst_is_identity,
            row,
            src_top,
            src_left,
            src_bottom,
            src_right,
            dst_top,
            dst_left,
            frame_top,
            frame_left,
            pic_dst_top,
            pic_dst_left,
            pic_dst_bottom,
            pic_dst_right,
            scale_x,
            scale_y,
            screen_base,
            screen_rb,
            screen_w,
            screen_h,
            scrn_ps,
            clip_region,
            dst_clip,
            scratch,
        )
    {
        return;
    }

    let src_y = i32::from(pm.bounds_top) + row as i32;
    let Some(pic_y) = map_src_coord(src_y, src_top, src_bottom, pic_dst_top, pic_dst_bottom) else {
        return;
    };
    let base_y = ((pic_y - i32::from(frame_top)) as f64 * scale_y) as i32 + dst_top as i32;

    let map_x = |px: u32| {
        let src_x = i32::from(pm.bounds_left) + px as i32;
        map_src_coord(src_x, src_left, src_right, pic_dst_left, pic_dst_right)
    };

    match pm.pixel_size {
        1 => {
            for px in 0..width {
                let byte_idx = (px / 8) as usize;
                let bit = 7 - (px % 8);
                if byte_idx < row_data.len() {
                    let is_set = (row_data[byte_idx] & (1 << bit)) != 0;
                    let color_idx = if is_set {
                        fg_idx
                    } else if mode_base == 0 {
                        bg_idx
                    } else {
                        continue;
                    };
                    let Some(pic_x) = map_x(px) else {
                        continue;
                    };
                    if clip_region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                        continue;
                    }
                    let x =
                        ((pic_x - i32::from(frame_left)) as f64 * scale_x) as i32 + dst_left as i32;
                    write_pixel_clipped(
                        bus,
                        screen_base,
                        screen_rb,
                        x,
                        base_y,
                        color_idx,
                        screen_w,
                        screen_h,
                        scrn_ps,
                        dst_clip,
                    );
                }
            }
        }
        2 => {
            for px in 0..width {
                let byte_idx = (px / 4) as usize;
                let shift = (3 - (px % 4)) * 2;
                if byte_idx < row_data.len() {
                    let ci = ((row_data[byte_idx] >> shift) & 0x03) as usize;
                    if mode_base == 36 && ci == 0 {
                        continue;
                    }
                    let Some(pic_x) = map_x(px) else {
                        continue;
                    };
                    if clip_region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                        continue;
                    }
                    let x =
                        ((pic_x - i32::from(frame_left)) as f64 * scale_x) as i32 + dst_left as i32;
                    let Some(pixel) = pict_indexed_transfer_pixel(
                        bus,
                        ci,
                        indexed_transfer,
                        screen_base,
                        screen_rb,
                        x,
                        base_y,
                        screen_w,
                        screen_h,
                        scrn_ps,
                        device_clut,
                    ) else {
                        continue;
                    };
                    write_pixel_clipped(
                        bus,
                        screen_base,
                        screen_rb,
                        x,
                        base_y,
                        pixel,
                        screen_w,
                        screen_h,
                        scrn_ps,
                        dst_clip,
                    );
                }
            }
        }
        4 => {
            let Some((pic_y, y_start, y_end)) = map_src_pixel_span(
                src_y,
                src_top,
                src_bottom,
                pic_dst_top,
                pic_dst_bottom,
                frame_top,
                dst_top,
                scale_y,
            ) else {
                return;
            };
            for px in 0..width {
                let byte_idx = (px / 2) as usize;
                let shift = if px % 2 == 0 { 4 } else { 0 };
                if byte_idx < row_data.len() {
                    let ci = ((row_data[byte_idx] >> shift) & 0x0F) as usize;
                    if mode_base == 36 && ci == 0 {
                        continue;
                    }
                    let src_x = i32::from(pm.bounds_left) + px as i32;
                    let Some((pic_x, x_start, x_end)) = map_src_pixel_span(
                        src_x,
                        src_left,
                        src_right,
                        pic_dst_left,
                        pic_dst_right,
                        frame_left,
                        dst_left,
                        scale_x,
                    ) else {
                        continue;
                    };
                    if clip_region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                        continue;
                    }
                    for y in y_start..y_end {
                        for x in x_start..x_end {
                            let Some(pixel) = pict_indexed_transfer_pixel(
                                bus,
                                ci,
                                indexed_transfer,
                                screen_base,
                                screen_rb,
                                x,
                                y,
                                screen_w,
                                screen_h,
                                scrn_ps,
                                device_clut,
                            ) else {
                                continue;
                            };
                            write_pixel_clipped(
                                bus,
                                screen_base,
                                screen_rb,
                                x,
                                y,
                                pixel,
                                screen_w,
                                screen_h,
                                scrn_ps,
                                dst_clip,
                            );
                        }
                    }
                }
            }
        }
        8 => {
            let Some((pic_y, y_start, y_end)) = map_src_pixel_span(
                src_y,
                src_top,
                src_bottom,
                pic_dst_top,
                pic_dst_bottom,
                frame_top,
                dst_top,
                scale_y,
            ) else {
                return;
            };
            for px in 0..width {
                let byte_idx = px as usize;
                if byte_idx < row_data.len() {
                    let src_pixel = row_data[byte_idx] as usize;
                    if mode_base == 36 && src_pixel == 0 {
                        continue;
                    }
                    let src_x = i32::from(pm.bounds_left) + px as i32;
                    let Some((pic_x, x_start, x_end)) = map_src_pixel_span(
                        src_x,
                        src_left,
                        src_right,
                        pic_dst_left,
                        pic_dst_right,
                        frame_left,
                        dst_left,
                        scale_x,
                    ) else {
                        continue;
                    };
                    if clip_region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                        continue;
                    }
                    for y in y_start..y_end {
                        for x in x_start..x_end {
                            let Some(pixel) = pict_indexed_transfer_pixel(
                                bus,
                                src_pixel,
                                indexed_transfer,
                                screen_base,
                                screen_rb,
                                x,
                                y,
                                screen_w,
                                screen_h,
                                scrn_ps,
                                device_clut,
                            ) else {
                                continue;
                            };
                            if trace_pict_samples_enabled() {
                                for (label, sample_x, sample_y) in [
                                    ("center", 400i32, 300i32),
                                    ("title_right", 580, 350),
                                    ("title_low", 400, 430),
                                ] {
                                    if x == sample_x && y == sample_y {
                                        let src_rgb = device_clut[pixel as usize];
                                        eprintln!(
                                            "[PICT] sample {} dst=({}, {}) src_row={} src_px={} src_idx={} dst_idx={} dst_rgb=({:04X},{:04X},{:04X})",
                                            label,
                                            x,
                                            y,
                                            row,
                                            px,
                                            src_pixel,
                                            pixel,
                                            src_rgb[0],
                                            src_rgb[1],
                                            src_rgb[2],
                                        );
                                    }
                                }
                            }
                            write_pixel_clipped(
                                bus,
                                screen_base,
                                screen_rb,
                                x,
                                y,
                                pixel,
                                screen_w,
                                screen_h,
                                scrn_ps,
                                dst_clip,
                            );
                        }
                    }
                }
            }
        }
        16 => {
            for px in 0..width {
                let byte_idx = (px * 2) as usize;
                if byte_idx + 1 < row_data.len() {
                    let hi = row_data[byte_idx] as u16;
                    let lo = row_data[byte_idx + 1] as u16;
                    let pixel = (hi << 8) | lo;
                    // Mac 16-bit: xRRRRRGG GGGBBBBB
                    let r = (((pixel >> 10) & 0x1F) * 255 / 31) as u8;
                    let g = (((pixel >> 5) & 0x1F) * 255 / 31) as u8;
                    let b = ((pixel & 0x1F) * 255 / 31) as u8;
                    let idx = closest_clut_index(
                        r as u16 * 257,
                        g as u16 * 257,
                        b as u16 * 257,
                        device_clut,
                    );
                    let Some(pic_x) = map_x(px) else {
                        continue;
                    };
                    if clip_region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                        continue;
                    }
                    let x =
                        ((pic_x - i32::from(frame_left)) as f64 * scale_x) as i32 + dst_left as i32;
                    write_pixel_clipped(
                        bus,
                        screen_base,
                        screen_rb,
                        x,
                        base_y,
                        idx,
                        screen_w,
                        screen_h,
                        scrn_ps,
                        dst_clip,
                    );
                }
            }
        }
        _ => {
            // Unsupported pixel size
        }
    }
}

fn intersect_spans_with_range(spans: &mut Vec<(i32, i32)>, left: i32, right: i32) {
    spans.retain_mut(|(span_left, span_right)| {
        *span_left = (*span_left).max(left);
        *span_right = (*span_right).min(right);
        *span_right > *span_left
    });
}

fn intersect_spans_with_region_row(spans: &mut Vec<(i32, i32)>, region: &DstClipRegion, y: i32) {
    if y < region.top || y >= region.bottom {
        spans.clear();
        return;
    }
    let Some(rows) = region.rows.as_ref() else {
        intersect_spans_with_range(spans, region.left, region.right);
        return;
    };
    let Some(edges) = rows.get((y - region.top) as usize) else {
        spans.clear();
        return;
    };

    let mut row_spans = Vec::new();
    for pair in edges.chunks(2) {
        if pair.len() != 2 {
            break;
        }
        let left = pair[0].max(region.left);
        let right = pair[1].min(region.right);
        if right > left {
            row_spans.push((left, right));
        }
    }
    if row_spans.is_empty() {
        spans.clear();
        return;
    }

    let mut clipped = Vec::new();
    for &(span_left, span_right) in spans.iter() {
        for &(row_left, row_right) in &row_spans {
            let left = span_left.max(row_left);
            let right = span_right.min(row_right);
            if right > left {
                clipped.push((left, right));
            }
        }
    }
    *spans = clipped;
}

fn dst_clip_row_spans(
    dst_clip: Option<&DstClip>,
    y: i32,
    left: i32,
    right: i32,
) -> Vec<(i32, i32)> {
    if right <= left {
        return Vec::new();
    }
    let mut spans = vec![(left, right)];
    let Some(dst_clip) = dst_clip else {
        return spans;
    };

    let (clip_top, clip_left, clip_bottom, clip_right) = dst_clip.rect();
    if y < clip_top || y >= clip_bottom {
        return Vec::new();
    }
    intersect_spans_with_range(&mut spans, clip_left, clip_right);
    for region in &dst_clip.regions {
        intersect_spans_with_region_row(&mut spans, region, y);
        if spans.is_empty() {
            break;
        }
    }
    spans
}

enum RowClipSpan {
    Empty,
    Single(i32, i32),
    Complex,
}

fn dst_clip_simple_row_span(
    dst_clip: Option<&DstClip>,
    y: i32,
    mut left: i32,
    mut right: i32,
) -> RowClipSpan {
    if right <= left {
        return RowClipSpan::Empty;
    }
    let Some(dst_clip) = dst_clip else {
        return RowClipSpan::Single(left, right);
    };

    let (clip_top, clip_left, clip_bottom, clip_right) = dst_clip.rect();
    if y < clip_top || y >= clip_bottom {
        return RowClipSpan::Empty;
    }
    left = left.max(clip_left);
    right = right.min(clip_right);
    if right <= left {
        return RowClipSpan::Empty;
    }

    for region in &dst_clip.regions {
        if region.rows.is_some() {
            return RowClipSpan::Complex;
        }
        if y < region.top || y >= region.bottom {
            return RowClipSpan::Empty;
        }
        left = left.max(region.left);
        right = right.min(region.right);
        if right <= left {
            return RowClipSpan::Empty;
        }
    }
    RowClipSpan::Single(left, right)
}

#[allow(clippy::too_many_arguments)]
fn can_blit_8bpp_src_copy_rows_fast(
    mode_base: u16,
    pm: &PixMapInfo,
    src_top: i16,
    src_left: i16,
    src_bottom: i16,
    src_right: i16,
    pic_dst_top: i16,
    pic_dst_left: i16,
    pic_dst_bottom: i16,
    pic_dst_right: i16,
    scale_x: f64,
    scale_y: f64,
    scrn_ps: u16,
    clip_region: Option<&PictureRegion>,
) -> bool {
    // A PackBitsRgn / BitsRgn mask region does not disqualify the row path:
    // it is applied per row as span intersections, exactly like the port's
    // dst_clip regions. (Unused here beyond that; kept for the signature.)
    let _ = clip_region;
    if mode_base != 0
        || (scrn_ps != 8 && scrn_ps != 1)
        || pm.cmp_count != 1
        || scale_x != 1.0
        || scale_y != 1.0
    {
        return false;
    }

    let src_span_x = i32::from(src_right) - i32::from(src_left);
    let dst_span_x = i32::from(pic_dst_right) - i32::from(pic_dst_left);
    let src_span_y = i32::from(src_bottom) - i32::from(src_top);
    let dst_span_y = i32::from(pic_dst_bottom) - i32::from(pic_dst_top);
    src_span_x > 0
        && src_span_y > 0
        && src_span_x == dst_span_x
        && src_span_y == dst_span_y
        && pm.bounds_right > pm.bounds_left
        && pm.bounds_bottom > pm.bounds_top
}

#[allow(clippy::too_many_arguments)]
fn try_blit_row_8bpp_src_copy_fast(
    bus: &mut MacMemoryBus,
    row_data: &[u8],
    mode_base: u16,
    pm: &PixMapInfo,
    src_to_dst: &[u8; 256],
    src_to_dst_is_identity: bool,
    row: u32,
    src_top: i16,
    src_left: i16,
    src_bottom: i16,
    src_right: i16,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    pic_dst_top: i16,
    pic_dst_left: i16,
    pic_dst_bottom: i16,
    pic_dst_right: i16,
    scale_x: f64,
    scale_y: f64,
    screen_base: u32,
    screen_rb: u32,
    screen_w: i32,
    screen_h: i32,
    scrn_ps: u16,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    scratch: &mut Vec<u8>,
) -> bool {
    if !can_blit_8bpp_src_copy_rows_fast(
        mode_base,
        pm,
        src_top,
        src_left,
        src_bottom,
        src_right,
        pic_dst_top,
        pic_dst_left,
        pic_dst_bottom,
        pic_dst_right,
        scale_x,
        scale_y,
        scrn_ps,
        clip_region,
    ) {
        return false;
    }

    let src_span_x = i32::from(src_right) - i32::from(src_left);
    let dst_span_x = i32::from(pic_dst_right) - i32::from(pic_dst_left);
    let src_span_y = i32::from(src_bottom) - i32::from(src_top);
    let dst_span_y = i32::from(pic_dst_bottom) - i32::from(pic_dst_top);
    if src_span_x <= 0 || src_span_y <= 0 || src_span_x != dst_span_x || src_span_y != dst_span_y {
        return false;
    }

    let source_left = i32::from(pm.bounds_left).max(i32::from(src_left));
    let source_right = i32::from(pm.bounds_right).min(i32::from(src_right));
    let mut run_len = source_right - source_left;
    if run_len <= 0 {
        return true;
    }

    let src_y = i32::from(pm.bounds_top) + row as i32;
    if src_y < i32::from(src_top) || src_y >= i32::from(src_bottom) {
        return true;
    }

    let pic_y = i32::from(pic_dst_top) + src_y - i32::from(src_top);
    let y = i32::from(dst_top) + pic_y - i32::from(frame_top);
    if y < 0 || y >= screen_h {
        return true;
    }

    let mut src_offset = source_left - i32::from(pm.bounds_left);
    let mut dst_x = i32::from(dst_left) + source_left - i32::from(src_left)
        + i32::from(pic_dst_left)
        - i32::from(frame_left);

    if dst_x < 0 {
        src_offset -= dst_x;
        run_len += dst_x;
        dst_x = 0;
    }
    if dst_x + run_len > screen_w {
        run_len = screen_w - dst_x;
    }
    if run_len <= 0 {
        return true;
    }

    if scrn_ps == 1 {
        let write_span_1bpp = |bus: &mut MacMemoryBus, span_left: i32, span_right: i32| {
            let span_len = span_right - span_left;
            let src_start = (src_offset + span_left - dst_x) as usize;
            let src_end = src_start + span_len as usize;
            if src_end > row_data.len() {
                return false;
            }

            let mut x = span_left;
            while x < span_right {
                let byte_left = x & !7;
                let byte_right = byte_left + 8;
                let write_left = x.max(byte_left);
                let write_right = span_right.min(byte_right);
                let addr = screen_base + (y as u32) * screen_rb + (byte_left as u32 / 8);
                let mut byte = if write_left == byte_left && write_right == byte_right {
                    0
                } else {
                    bus.read_byte(addr)
                };

                for px in write_left..write_right {
                    let source_index = src_start + (px - span_left) as usize;
                    let bit = 7 - (px & 7);
                    let pixel = if src_to_dst_is_identity {
                        row_data[source_index]
                    } else {
                        src_to_dst[row_data[source_index] as usize]
                    };
                    if pixel != 0 {
                        byte |= 1 << bit;
                    } else {
                        byte &= !(1 << bit);
                    }
                }
                bus.write_byte(addr, byte);
                x = write_right;
            }
            true
        };

        if clip_region.is_none() {
            match dst_clip_simple_row_span(dst_clip, y, dst_x, dst_x + run_len) {
                RowClipSpan::Empty => return true,
                RowClipSpan::Single(span_left, span_right) => {
                    return write_span_1bpp(bus, span_left, span_right);
                }
                RowClipSpan::Complex => {}
            }
        }

        let mut spans = dst_clip_row_spans(dst_clip, y, dst_x, dst_x + run_len);
        if let Some(region) = clip_region {
            intersect_spans_with_picture_region_row(
                &mut spans,
                region,
                pic_y,
                i32::from(dst_left) - i32::from(frame_left),
            );
        }
        for (span_left, span_right) in spans {
            if !write_span_1bpp(bus, span_left, span_right) {
                return false;
            }
        }
        return true;
    }

    let mut write_span = |bus: &mut MacMemoryBus, span_left: i32, span_right: i32| {
        let span_len = span_right - span_left;
        let src_start = (src_offset + span_left - dst_x) as usize;
        let src_end = src_start + span_len as usize;
        if src_end > row_data.len() {
            return false;
        }

        let row_slice = &row_data[src_start..src_end];
        let dst_addr = screen_base + (y as u32) * screen_rb + span_left as u32;
        if src_to_dst_is_identity {
            bus.write_bytes(dst_addr, row_slice);
        } else {
            scratch.resize(row_slice.len(), 0);
            for (dst, &pixel) in scratch.iter_mut().zip(row_slice.iter()) {
                *dst = src_to_dst[pixel as usize];
            }
            bus.write_bytes(dst_addr, scratch);
        }
        true
    };

    if clip_region.is_none() {
        match dst_clip_simple_row_span(dst_clip, y, dst_x, dst_x + run_len) {
            RowClipSpan::Empty => return true,
            RowClipSpan::Single(span_left, span_right) => {
                return write_span(bus, span_left, span_right)
            }
            RowClipSpan::Complex => {}
        }
    }

    let mut spans = dst_clip_row_spans(dst_clip, y, dst_x, dst_x + run_len);
    if let Some(region) = clip_region {
        intersect_spans_with_picture_region_row(
            &mut spans,
            region,
            pic_y,
            i32::from(dst_left) - i32::from(frame_left),
        );
    }
    for (span_left, span_right) in spans {
        if !write_span(bus, span_left, span_right) {
            return false;
        }
    }
    true
}

/// Intersect destination-x `spans` on picture row `pic_y` with a PICT op's
/// own mask region (PackBitsRgn / BitsRgn), whose edges are in picture
/// coordinates: destination x = picture x + `dx`. Mirrors
/// `PictureRegion::contains`, which the per-pixel path consults: outside the
/// bounding box is out; an empty edge list is the whole box; otherwise the
/// row's edges toggle membership, a trailing unpaired edge running to the
/// box's right.
fn intersect_spans_with_picture_region_row(
    spans: &mut Vec<(i32, i32)>,
    region: &PictureRegion,
    pic_y: i32,
    dx: i32,
) {
    let (top, left, bottom, right) = (
        i32::from(region.top),
        i32::from(region.left),
        i32::from(region.bottom),
        i32::from(region.right),
    );
    if pic_y < top || pic_y >= bottom {
        spans.clear();
        return;
    }
    intersect_spans_with_range(spans, left + dx, right + dx);
    if region.rows.is_empty() || spans.is_empty() {
        return;
    }
    let Some(edges) = region.rows.get((pic_y - top) as usize) else {
        spans.clear();
        return;
    };
    let mut row_spans = Vec::with_capacity(edges.len() / 2 + 1);
    let mut index = 0;
    while index < edges.len() {
        let span_left = i32::from(edges[index]).max(left);
        let span_right = edges
            .get(index + 1)
            .map_or(right, |&edge| i32::from(edge).min(right));
        if span_right > span_left {
            row_spans.push((span_left + dx, span_right + dx));
        }
        index += 2;
    }
    let mut clipped = Vec::new();
    for &(span_left, span_right) in spans.iter() {
        for &(row_left, row_right) in &row_spans {
            let left = span_left.max(row_left);
            let right = span_right.min(row_right);
            if right > left {
                clipped.push((left, right));
            }
        }
    }
    *spans = clipped;
}

fn packbits_row_data_bounds(
    bus: &MacMemoryBus,
    pos: u32,
    byte_count_row_bytes: u16,
) -> Option<(u32, u32, u32)> {
    let (data_start, byte_count) = if byte_count_row_bytes > 250 {
        (pos.checked_add(2)?, u32::from(bus.read_word(pos)))
    } else {
        (pos.checked_add(1)?, u32::from(bus.read_byte(pos)))
    };
    let data_end = data_start.checked_add(byte_count)?;
    (data_end <= bus.ram_size()).then_some((data_start, data_end, data_end))
}

#[allow(clippy::too_many_arguments)]
fn try_blit_packbits_8bpp_src_copy_fast(
    bus: &mut MacMemoryBus,
    mut pos: u32,
    height: u32,
    pm: &PixMapInfo,
    mode_base: u16,
    src_to_dst: &[u8; 256],
    src_to_dst_is_identity: bool,
    src_top: i16,
    src_left: i16,
    src_bottom: i16,
    src_right: i16,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    pic_dst_top: i16,
    pic_dst_left: i16,
    pic_dst_bottom: i16,
    pic_dst_right: i16,
    scale_x: f64,
    scale_y: f64,
    screen_base: u32,
    screen_rb: u32,
    screen_w: i32,
    screen_h: i32,
    scrn_ps: u16,
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
) -> Option<u32> {
    if pm.pixel_size != 8
        || pm.row_bytes < 8
        || scrn_ps != 8
        || !can_blit_8bpp_src_copy_rows_fast(
            mode_base,
            pm,
            src_top,
            src_left,
            src_bottom,
            src_right,
            pic_dst_top,
            pic_dst_left,
            pic_dst_bottom,
            pic_dst_right,
            scale_x,
            scale_y,
            scrn_ps,
            clip_region,
        )
    {
        return None;
    }
    if clip_region.is_some()
        || dst_clip
            .map(|clip| clip.regions.iter().any(|region| region.rows.is_some()))
            .unwrap_or(false)
    {
        // Region-masked rows are multi-span; the per-row path handles them.
        return None;
    }

    let mut scan_pos = pos;
    for _ in 0..height {
        let (_, _, next_pos) = packbits_row_data_bounds(bus, scan_pos, pm.row_bytes)?;
        scan_pos = next_pos;
    }

    let source_left = i32::from(pm.bounds_left).max(i32::from(src_left));
    let source_right = i32::from(pm.bounds_right).min(i32::from(src_right));
    let source_run_len = source_right - source_left;
    if source_run_len <= 0 {
        return Some(scan_pos);
    }

    for row in 0..height {
        let (row_start, row_end, next_pos) = packbits_row_data_bounds(bus, pos, pm.row_bytes)?;
        pos = next_pos;

        let src_y = i32::from(pm.bounds_top) + row as i32;
        if src_y < i32::from(src_top) || src_y >= i32::from(src_bottom) {
            continue;
        }

        let pic_y = i32::from(pic_dst_top) + src_y - i32::from(src_top);
        let y = i32::from(dst_top) + pic_y - i32::from(frame_top);
        if y < 0 || y >= screen_h {
            continue;
        }

        let mut src_offset = source_left - i32::from(pm.bounds_left);
        let mut run_len = source_run_len;
        let mut dst_x = i32::from(dst_left) + source_left - i32::from(src_left)
            + i32::from(pic_dst_left)
            - i32::from(frame_left);

        if dst_x < 0 {
            src_offset -= dst_x;
            run_len += dst_x;
            dst_x = 0;
        }
        if dst_x + run_len > screen_w {
            run_len = screen_w - dst_x;
        }
        if run_len <= 0 {
            continue;
        }

        let (span_left, span_right) =
            match dst_clip_simple_row_span(dst_clip, y, dst_x, dst_x + run_len) {
                RowClipSpan::Empty => continue,
                RowClipSpan::Single(left, right) => (left, right),
                RowClipSpan::Complex => return None,
            };
        let span_len = span_right - span_left;
        if span_len <= 0 {
            continue;
        }
        let src_start = (src_offset + span_left - dst_x) as usize;
        let src_end = src_start + span_len as usize;
        if src_end > pm.row_bytes as usize {
            return None;
        }
        let dst_addr = screen_base + (y as u32) * screen_rb + span_left as u32;
        if !write_packbits_span_8bpp(
            bus,
            row_start,
            row_end,
            pm.row_bytes as usize,
            src_start,
            src_end,
            dst_addr,
            src_to_dst,
            src_to_dst_is_identity,
        ) {
            return None;
        }
    }

    Some(pos)
}

#[allow(clippy::too_many_arguments)]
fn write_packbits_span_8bpp(
    bus: &mut MacMemoryBus,
    mut pos: u32,
    end_pos: u32,
    decoded_row_bytes: usize,
    src_start: usize,
    src_end: usize,
    dst_addr: u32,
    src_to_dst: &[u8; 256],
    src_to_dst_is_identity: bool,
) -> bool {
    let mut out_pos = 0usize;
    while pos < end_pos && out_pos < decoded_row_bytes {
        let flag = bus.read_byte(pos) as i8;
        pos += 1;

        if flag >= 0 {
            let count = (flag as usize) + 1;
            let literal_start = pos;
            let available = (end_pos - pos) as usize;
            let actual_count = count.min(available);
            let run_start = out_pos;
            let run_end = out_pos.saturating_add(actual_count).min(decoded_row_bytes);
            if run_end > src_start && run_start < src_end {
                let copy_start = run_start.max(src_start);
                let copy_end = run_end.min(src_end);
                let copy_len = copy_end - copy_start;
                let input_addr = literal_start + (copy_start - run_start) as u32;
                let write_addr = dst_addr + (copy_start - src_start) as u32;
                let copied = if src_to_dst_is_identity {
                    bus.copy_ram_bytes(input_addr, write_addr, copy_len as u32)
                } else {
                    bus.copy_mapped_ram_bytes(input_addr, write_addr, copy_len as u32, src_to_dst)
                };
                if !copied {
                    return false;
                }
            }
            out_pos = out_pos.saturating_add(actual_count);
            pos += actual_count as u32;
        } else if flag != -128 {
            if pos >= end_pos {
                return false;
            }
            let count = (-(flag as i16)) as usize + 1;
            let pixel = if src_to_dst_is_identity {
                bus.read_byte(pos)
            } else {
                src_to_dst[bus.read_byte(pos) as usize]
            };
            pos += 1;
            let run_start = out_pos;
            let run_end = out_pos.saturating_add(count).min(decoded_row_bytes);
            if run_end > src_start && run_start < src_end {
                let copy_start = run_start.max(src_start);
                let copy_end = run_end.min(src_end);
                let copy_len = copy_end - copy_start;
                let write_addr = dst_addr + (copy_start - src_start) as u32;
                bus.fill_bytes(write_addr, copy_len as u32, pixel);
            }
            out_pos = out_pos.saturating_add(count);
        }
    }

    out_pos >= src_end
}

fn read_screen_pixel_index(
    bus: &MacMemoryBus,
    screen_base: u32,
    screen_rb: u32,
    x: i32,
    y: i32,
    screen_w: i32,
    screen_h: i32,
) -> Option<u8> {
    if x < 0 || y < 0 || x >= screen_w || y >= screen_h {
        return None;
    }
    Some(bus.read_byte(screen_base + (y as u32) * screen_rb + x as u32))
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PictIndexedTransfer {
    Write(u8),
    Skip,
    InvertDestination(u8),
}

fn build_pict_indexed_transfer_table(
    mode_base: u16,
    src_clut: &[[u16; 3]],
    src_to_dst: &[u8; 256],
    device_clut: &[[u16; 3]; 256],
    fg_idx: u8,
    bg_idx: u8,
) -> [PictIndexedTransfer; 256] {
    if mode_base == 0 {
        return std::array::from_fn(|source_index| {
            PictIndexedTransfer::Write(src_to_dst[source_index])
        });
    }

    std::array::from_fn(|source_index| {
        let translated_pixel = src_to_dst[source_index];
        pict_indexed_source_mode_transfer(
            mode_base,
            source_index,
            translated_pixel,
            fg_idx,
            bg_idx,
            src_clut,
            device_clut,
        )
    })
}

fn pict_indexed_transfer_pixel(
    bus: &MacMemoryBus,
    source_index: usize,
    transfer_table: &[PictIndexedTransfer; 256],
    screen_base: u32,
    screen_rb: u32,
    x: i32,
    y: i32,
    screen_w: i32,
    screen_h: i32,
    scrn_ps: u16,
    device_clut: &[[u16; 3]; 256],
) -> Option<u8> {
    let transfer = transfer_table[source_index];
    match transfer {
        PictIndexedTransfer::Write(pixel) => Some(pixel),
        PictIndexedTransfer::Skip => None,
        PictIndexedTransfer::InvertDestination(fallback) => {
            if scrn_ps != 8 {
                return Some(fallback);
            }
            let dst_pixel =
                read_screen_pixel_index(bus, screen_base, screen_rb, x, y, screen_w, screen_h)?;
            Some(pict_inverted_clut_index(device_clut, dst_pixel))
        }
    }
}

fn pict_source_rgb(
    source_index: usize,
    translated_pixel: u8,
    src_clut: &[[u16; 3]],
    device_clut: &[[u16; 3]; 256],
) -> [u16; 3] {
    src_clut
        .get(source_index)
        .copied()
        .unwrap_or(device_clut[translated_pixel as usize])
}

fn pict_colorize_src_copy_rgb(src_rgb: [u16; 3], fg_rgb: [u16; 3], bg_rgb: [u16; 3]) -> [u16; 3] {
    let mut out = [0u16; 3];
    for component in 0..3 {
        let src = u32::from(src_rgb[component]);
        let fg = u32::from(fg_rgb[component]);
        let bg = u32::from(bg_rgb[component]);
        out[component] = ((((0xFFFF - src) * fg) + (src * bg) + 0x7FFF) / 0xFFFF) as u16;
    }
    out
}

fn pict_colorize_src_or_rgb(src_rgb: [u16; 3], fg_rgb: [u16; 3]) -> [u16; 3] {
    let mut out = [0u16; 3];
    for component in 0..3 {
        let src = u32::from(src_rgb[component]);
        let fg = u32::from(fg_rgb[component]);
        out[component] = (((0xFFFF - src) * fg + 0x7FFF) / 0xFFFF) as u16;
    }
    out
}

fn pict_colorize_not_src_or_rgb(src_rgb: [u16; 3], fg_rgb: [u16; 3]) -> [u16; 3] {
    let mut out = [0u16; 3];
    for component in 0..3 {
        let src = u32::from(src_rgb[component]);
        let fg = u32::from(fg_rgb[component]);
        out[component] = ((src * fg + 0x7FFF) / 0xFFFF) as u16;
    }
    out
}

fn pict_inverted_clut_index(clut: &[[u16; 3]; 256], pixel: u8) -> u8 {
    let rgb = clut[pixel as usize];
    closest_clut_index(0xFFFF - rgb[0], 0xFFFF - rgb[1], 0xFFFF - rgb[2], clut)
}

fn pict_indexed_source_mode_transfer(
    mode_base: u16,
    source_index: usize,
    translated_pixel: u8,
    fg_idx: u8,
    bg_idx: u8,
    src_clut: &[[u16; 3]],
    device_clut: &[[u16; 3]; 256],
) -> PictIndexedTransfer {
    // Plain srcCopy copies indexed PixMap colors through the source and
    // destination ColorTables. Boolean source modes apply foreground/background
    // colors on colored pixels; white/black source pixels preserve or modify
    // the destination according to Table 4-1.
    // Imaging With QuickDraw 1994, p. 4-33.
    let src_rgb = pict_source_rgb(source_index, translated_pixel, src_clut, device_clut);
    let is_black = src_rgb == [0, 0, 0];
    let is_white = src_rgb == [0xFFFF, 0xFFFF, 0xFFFF];
    let fg_rgb = device_clut[fg_idx as usize];
    let bg_rgb = device_clut[bg_idx as usize];
    let map_rgb = |rgb: [u16; 3]| closest_clut_index(rgb[0], rgb[1], rgb[2], device_clut);

    match mode_base {
        0 => PictIndexedTransfer::Write(translated_pixel),
        1 => {
            if is_white {
                PictIndexedTransfer::Skip
            } else if is_black {
                PictIndexedTransfer::Write(fg_idx)
            } else {
                PictIndexedTransfer::Write(map_rgb(pict_colorize_src_or_rgb(src_rgb, fg_rgb)))
            }
        }
        2 => {
            if is_black {
                PictIndexedTransfer::InvertDestination(translated_pixel)
            } else {
                PictIndexedTransfer::Skip
            }
        }
        3 => {
            if is_white {
                PictIndexedTransfer::Skip
            } else if is_black {
                PictIndexedTransfer::Write(bg_idx)
            } else {
                PictIndexedTransfer::Write(map_rgb(pict_colorize_src_or_rgb(src_rgb, bg_rgb)))
            }
        }
        4 => PictIndexedTransfer::Write(if is_black {
            bg_idx
        } else if is_white {
            fg_idx
        } else {
            map_rgb(pict_colorize_src_copy_rgb(src_rgb, bg_rgb, fg_rgb))
        }),
        5 => {
            if is_black {
                PictIndexedTransfer::Skip
            } else if is_white {
                PictIndexedTransfer::Write(fg_idx)
            } else {
                PictIndexedTransfer::Write(map_rgb(pict_colorize_not_src_or_rgb(src_rgb, fg_rgb)))
            }
        }
        6 => {
            if is_white {
                PictIndexedTransfer::InvertDestination(translated_pixel)
            } else {
                PictIndexedTransfer::Skip
            }
        }
        7 => {
            if is_black {
                PictIndexedTransfer::Skip
            } else if is_white {
                PictIndexedTransfer::Write(bg_idx)
            } else {
                PictIndexedTransfer::Write(map_rgb(pict_colorize_not_src_or_rgb(src_rgb, bg_rgb)))
            }
        }
        36 => {
            if source_index == 0 {
                PictIndexedTransfer::Skip
            } else {
                PictIndexedTransfer::Write(translated_pixel)
            }
        }
        _ => PictIndexedTransfer::Write(translated_pixel),
    }
}

/// Parse DirectBitsRect / DirectBitsRgn (opcode 0x009A/0x009B)
fn parse_direct_bits_rect(
    bus: &mut MacMemoryBus,
    mut pos: u32,
    has_rgn: bool,
    dst_top: i16,
    dst_left: i16,
    frame_top: i16,
    frame_left: i16,
    scale_x: f64,
    scale_y: f64,
    screen_mode: (u32, u32, u16, u16, u16),
    device_clut: &[[u16; 3]; 256],
    clip_region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
) -> u32 {
    // DirectBitsRect has PixMap WITH baseAddr prefix (usually 0x000000FF)
    let (new_pos, pm) = read_pixmap_with_base(bus, pos);
    pos = new_pos;
    // No ColorTable for direct pixels

    if trace_pict_enabled() {
        eprintln!(
            "[PICT] DirectBits{} pixelSize={} cmpCount={} packType={} rowBytes={} bounds=({}, {}, {}, {})",
            if has_rgn { "Rgn" } else { "Rect" },
            pm.pixel_size,
            pm.cmp_count,
            pm.pack_type,
            pm.row_bytes,
            pm.bounds_top,
            pm.bounds_left,
            pm.bounds_bottom,
            pm.bounds_right
        );
    }

    // srcRect
    let src_top = bus.read_word(pos) as i16;
    pos += 2;
    let src_left = bus.read_word(pos) as i16;
    pos += 2;
    let src_bottom = bus.read_word(pos) as i16;
    pos += 2;
    let src_right = bus.read_word(pos) as i16;
    pos += 2;
    // dstRect
    let pic_dst_top = bus.read_word(pos) as i16;
    pos += 2;
    let pic_dst_left = bus.read_word(pos) as i16;
    pos += 2;
    let pic_dst_bottom = bus.read_word(pos) as i16;
    pos += 2;
    let pic_dst_right = bus.read_word(pos) as i16;
    pos += 2;
    // mode
    let mode = bus.read_word(pos);
    pos += 2;

    if trace_pict_enabled() {
        eprintln!(
            "[PICT] DirectBits{} mode={} src=({},{}..{},{} ) dst=({},{}..{},{} )",
            if has_rgn { "Rgn" } else { "Rect" },
            mode,
            src_top,
            src_left,
            src_bottom,
            src_right,
            pic_dst_top,
            pic_dst_left,
            pic_dst_bottom,
            pic_dst_right,
        );
    }

    if has_rgn {
        let rgn_size = bus.read_word(pos) as u32;
        pos += rgn_size;
    }

    let height = (pm.bounds_bottom - pm.bounds_top).max(0) as u32;
    let width = (pm.bounds_right - pm.bounds_left).max(0) as u32;
    let (screen_base, screen_rb, screen_w, screen_h, scrn_ps) = (
        screen_mode.0,
        screen_mode.1,
        screen_mode.2 as i32,
        screen_mode.3 as i32,
        screen_mode.4,
    );
    let dst_clut = indexed_destination_clut(device_clut, scrn_ps);

    for row in 0..height {
        // Per PixMap.packType (Imaging With QuickDraw 1994, 4-29):
        //   0 = default (no compression for >8bpp; PackBits for ≤8bpp)
        //   1 = unpacked
        //   2 = drop pad byte (32bpp only — 4-byte → 3-byte conversion)
        //   3 = byte-PackBits on 16-bit pixels (16bpp)
        //   4 = byte-PackBits on cmpCount component planes per row
        //       (16bpp uses RGB planes; 32bpp uses ARGB or RGB planes)
        let unpacked_len = match (pm.pixel_size, pm.pack_type) {
            (32, 4) => (pm.bounds_right - pm.bounds_left) as u16 * pm.cmp_count,
            _ => pm.row_bytes,
        };
        let (new_pos, row_data) = if pm.row_bytes < 8 || pm.pack_type == 0 || pm.pack_type == 1 {
            // Uncompressed: raw bytes equal to rowBytes.
            let data: Vec<u8> = (0..pm.row_bytes as u32)
                .map(|i| bus.read_byte(pos + i))
                .collect();
            (pos + pm.row_bytes as u32, data)
        } else if pm.pack_type == 3 && pm.pixel_size == 16 {
            unpack_bits_chunk16(bus, pos, unpacked_len)
        } else {
            unpack_bits_with_byte_count_row_bytes(bus, pos, unpacked_len, pm.row_bytes)
        };
        pos = new_pos;

        let src_y = i32::from(pm.bounds_top) + row as i32;
        let mapped_pic_y = map_src_coord(src_y, src_top, src_bottom, pic_dst_top, pic_dst_bottom);

        match pm.pixel_size {
            16 => {
                for px in 0..width {
                    let byte_idx = (px * 2) as usize;
                    if byte_idx + 1 < row_data.len() {
                        let pixel =
                            ((row_data[byte_idx] as u16) << 8) | (row_data[byte_idx + 1] as u16);
                        let Some(pic_y) = mapped_pic_y else {
                            continue;
                        };
                        let src_x = i32::from(pm.bounds_left) + px as i32;
                        let Some(pic_x) =
                            map_src_coord(src_x, src_left, src_right, pic_dst_left, pic_dst_right)
                        else {
                            continue;
                        };
                        if clip_region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                            continue;
                        }
                        let x = ((pic_x - i32::from(frame_left)) as f64 * scale_x) as i32
                            + dst_left as i32;
                        let y = ((pic_y - i32::from(frame_top)) as f64 * scale_y) as i32
                            + dst_top as i32;
                        if scrn_ps == 16 {
                            write_rgb555_pixel_clipped(
                                bus,
                                screen_base,
                                screen_rb,
                                x,
                                y,
                                pixel,
                                screen_w,
                                screen_h,
                                dst_clip,
                            );
                        } else {
                            let r = (((pixel >> 10) & 0x1F) * 255 / 31) as u8;
                            let g = (((pixel >> 5) & 0x1F) * 255 / 31) as u8;
                            let b = ((pixel & 0x1F) * 255 / 31) as u8;
                            let idx = closest_clut_index(
                                r as u16 * 257,
                                g as u16 * 257,
                                b as u16 * 257,
                                &dst_clut,
                            );
                            write_pixel_clipped(
                                bus,
                                screen_base,
                                screen_rb,
                                x,
                                y,
                                idx,
                                screen_w,
                                screen_h,
                                scrn_ps,
                                dst_clip,
                            );
                        }
                    }
                }
            }
            32 => {
                // 32-bit direct: data is arranged as component planes
                // cmpCount=3: R plane, G plane, B plane (each width bytes)
                // cmpCount=4: A, R, G, B (each width bytes)
                let skip = if pm.cmp_count == 4 { width as usize } else { 0 };
                let r_start = skip;
                let g_start = skip + width as usize;
                let b_start = skip + 2 * width as usize;
                for px in 0..width {
                    let ri = r_start + px as usize;
                    let gi = g_start + px as usize;
                    let bi = b_start + px as usize;
                    if bi < row_data.len() {
                        let Some(pic_y) = mapped_pic_y else {
                            continue;
                        };
                        let src_x = i32::from(pm.bounds_left) + px as i32;
                        let Some(pic_x) =
                            map_src_coord(src_x, src_left, src_right, pic_dst_left, pic_dst_right)
                        else {
                            continue;
                        };
                        if clip_region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                            continue;
                        }
                        let x = ((pic_x - i32::from(frame_left)) as f64 * scale_x) as i32
                            + dst_left as i32;
                        let y = ((pic_y - i32::from(frame_top)) as f64 * scale_y) as i32
                            + dst_top as i32;
                        if scrn_ps == 16 {
                            let red = u16::from(row_data[ri]) * 31 / 255;
                            let green = u16::from(row_data[gi]) * 31 / 255;
                            let blue = u16::from(row_data[bi]) * 31 / 255;
                            write_rgb555_pixel_clipped(
                                bus,
                                screen_base,
                                screen_rb,
                                x,
                                y,
                                (red << 10) | (green << 5) | blue,
                                screen_w,
                                screen_h,
                                dst_clip,
                            );
                        } else {
                            let ci = closest_clut_index(
                                row_data[ri] as u16 * 257,
                                row_data[gi] as u16 * 257,
                                row_data[bi] as u16 * 257,
                                &dst_clut,
                            );
                            write_pixel_clipped(
                                bus,
                                screen_base,
                                screen_rb,
                                x,
                                y,
                                ci,
                                screen_w,
                                screen_h,
                                scrn_ps,
                                dst_clip,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pos
}

#[cfg(test)]
mod tests {
    use super::{
        blit_row, build_device_itable, build_pict_indexed_transfer_table, build_src_to_dst_table,
        clear_src_to_dst_table_cache_for_tests, closest_grayscale_luminance_index, draw_picture,
        dst_clip_row_spans, peek_initial_packbits_clut, picture_stream_len, read_color_table,
        try_blit_packbits_8bpp_src_copy_fast, try_blit_row_8bpp_src_copy_fast, DstClip,
        DstClipRegion, PictIndexedTransfer, PictureRegion, PixMapInfo,
    };
    use crate::memory::{MacMemoryBus, MemoryBus};
    use crate::trap::dispatch::TrapDispatcher;

    #[test]
    fn device_itable_matches_rom_propagation_samples() {
        let table = build_device_itable(&TrapDispatcher::standard_mac_8bpp_clut());

        assert_eq!(table[0x564], 130);
        assert_eq!(table[0x631], 137);
        assert_eq!(table[0x431], 173);
        assert_eq!(table[0x666], 129);
        assert_eq!(table[0x333], 172);
        assert_eq!(table[0x555], 251);
    }

    #[test]
    fn device_itable_keeps_a_dark_shade_available_beside_exact_black() {
        let mut clut = [[0xFFFF; 3]; 256];
        clut[1] = [0x0000, 0x0000, 0x0000];
        clut[104] = [0x0F0F, 0x0A0A, 0x0F0F];

        let table = build_device_itable(&clut);

        assert_eq!(table[0x000], 104);
        assert_eq!(table[0x010], 104);
    }

    #[test]
    fn color_table_uses_sparse_colorspec_values_as_source_indexes() {
        let mut bus = MacMemoryBus::new(1024);
        let mut pos = 0x100;
        bus.write_long(pos, 0x1234_5678);
        pos += 4;
        bus.write_word(pos, 0); // explicit ColorSpec.value indexes
        pos += 2;
        bus.write_word(pos, 2); // three entries
        pos += 2;
        for (value, rgb) in [
            (200u16, [0x1111, 0x2222, 0x3333]),
            (3u16, [0x4444, 0x5555, 0x6666]),
            (99u16, [0x7777, 0x8888, 0x9999]),
        ] {
            bus.write_word(pos, value);
            pos += 2;
            for component in rgb {
                bus.write_word(pos, component);
                pos += 2;
            }
        }

        let (end, colors, seed) = read_color_table(&bus, 0x100);

        assert_eq!(end, pos);
        assert_eq!(seed, 0x1234_5678);
        assert_eq!(colors[200], [0x1111, 0x2222, 0x3333]);
        assert_eq!(colors[3], [0x4444, 0x5555, 0x6666]);
        assert_eq!(colors[99], [0x7777, 0x8888, 0x9999]);
        assert_eq!(colors[0], [0, 0, 0]);
    }

    #[test]
    fn grayscale_luminance_mapping_prefers_low_chroma_match() {
        let mut dst = [[0u16; 3]; 256];
        dst[10] = [0xE000, 0x0000, 0x0000];
        dst[20] = [0x3939, 0x2C2C, 0x3939];
        assert_eq!(closest_grayscale_luminance_index(0x3939, &dst), 20);
    }

    #[test]
    fn dense_grayscale_source_uses_luminance_translation() {
        let mut src = [[0u16; 3]; 256];
        for (index, rgb) in src.iter_mut().enumerate() {
            let value = 0xFFFFu16.saturating_sub((index as u16) * 0x0101);
            *rgb = [value, value, value];
        }

        let mut dst = [[0u16; 3]; 256];
        dst[10] = [0xE000, 0x0000, 0x0000];
        dst[20] = [0x3939, 0x2C2C, 0x3939];
        dst[42] = [0x1111, 0x0202, 0x0000];

        let table = build_src_to_dst_table(&src, &dst);
        assert_eq!(table[198], 20);
    }

    #[test]
    fn dense_grayscale_source_does_not_preserve_indices_on_system_palette() {
        let mut src = [[0u16; 3]; 256];
        for (index, rgb) in src.iter_mut().enumerate() {
            let value = 0xFFFFu16.saturating_sub((index as u16) * 0x0101);
            *rgb = [value, value, value];
        }

        let dst = TrapDispatcher::standard_mac_8bpp_clut();
        let table = build_src_to_dst_table(&src, &dst);

        // Canonical index 16 is orange in the system color cube, not gray.
        // A grayscale PICT must remap this entry instead of passing it through.
        assert_ne!(table[16], 16);
        let mapped = dst[table[16] as usize];
        assert_eq!(mapped[0], mapped[1]);
        assert_eq!(mapped[1], mapped[2]);
    }

    #[test]
    fn exact_same_index_palette_entries_win_over_earlier_duplicates() {
        let mut src = [[0u16; 3]; 256];
        let mut dst = [[0u16; 3]; 256];
        let rgb = [0x2E2E, 0x0000, 0x3333];
        src[71] = rgb;
        dst[12] = rgb;
        dst[71] = rgb;

        let table = build_src_to_dst_table(&src, &dst);

        assert_eq!(table[71], 71);
    }

    #[test]
    fn exact_palette_entry_at_another_index_wins_over_inverse_cell_seed() {
        let mut src = [[0u16; 3]; 256];
        let mut dst = [[0u16; 3]; 256];
        dst[5] = [0x2000, 0x0000, 0x3000];
        dst[12] = [0x2E2E, 0x0202, 0x3333];
        src[71] = dst[12];

        let table = build_src_to_dst_table(&src, &dst);

        assert_eq!(table[71], 12);
    }

    #[test]
    fn eight_bit_device_matching_ignores_pict_rgb_low_bytes() {
        let mut src = [[0u16; 3]; 256];
        let mut dst = [[0u16; 3]; 256];
        src[14] = [0xFF0E, 0x2D0E, 0x890E];
        dst[73] = [0xFFFF, 0x2D2D, 0x8989];
        // A competing entry occupies the same 4-bit inverse-table cell but
        // does not display the requested 8-bit RGB value.
        dst[12] = [0xF000, 0x2000, 0x8000];

        let table = build_src_to_dst_table(&src, &dst);

        assert_eq!(table[14], 73);
    }

    #[test]
    fn standard_eight_bit_colors_use_rom_four_bit_color2index_mapping() {
        let src = TrapDispatcher::standard_mac_8bpp_clut();
        let mut dst = TrapDispatcher::standard_mac_4bpp_gworld_clut();
        let terminal = dst[15];
        dst[16..].fill(terminal);

        let table = build_src_to_dst_table(&src, &dst);

        for (source, destination) in [
            (0, 0),
            (1, 0),
            (7, 12),
            (16, 2),
            (42, 12),
            (64, 3),
            (128, 13),
            (214, 15),
            (215, 3),
            (225, 8),
            (235, 6),
            (245, 0),
            (255, 15),
        ] {
            assert_eq!(
                table[source], destination,
                "standard 8-bit source index {source}"
            );
        }
    }

    #[test]
    fn src_to_dst_table_cache_reuses_identical_cluts() {
        clear_src_to_dst_table_cache_for_tests();

        let mut src = [[0u16; 3]; 256];
        let mut dst = [[0u16; 3]; 256];
        src[1] = [0x2222, 0x3333, 0x4444];
        dst[7] = [0x2222, 0x3333, 0x4444];

        let first = build_src_to_dst_table(&src, &dst);
        let second = build_src_to_dst_table(&src, &dst);

        assert_eq!(second, first);
    }

    #[test]
    fn src_to_dst_table_cache_keys_on_destination_clut_contents() {
        clear_src_to_dst_table_cache_for_tests();

        let mut src = [[0u16; 3]; 256];
        src[1] = [0x2222, 0x3333, 0x4444];
        let mut first_dst = [[0u16; 3]; 256];
        first_dst[7] = [0x2222, 0x3333, 0x4444];
        let mut second_dst = first_dst;
        second_dst[9] = [0x2222, 0x3333, 0x4444];
        second_dst[7] = [0x1111, 0x1111, 0x1111];

        let first = build_src_to_dst_table(&src, &first_dst);
        let second = build_src_to_dst_table(&src, &second_dst);

        assert_ne!(first[1], second[1]);
    }

    #[test]
    fn complex_dst_clip_row_spans_follow_region_edges() {
        let clip = DstClip::new(
            (0, 0, 5, 20),
            vec![DstClipRegion::complex(
                1,
                0,
                4,
                20,
                vec![vec![3, 8, 12, 15], vec![5, 10], vec![]],
            )],
        );

        assert_eq!(
            dst_clip_row_spans(Some(&clip), 1, 0, 20),
            vec![(3, 8), (12, 15)]
        );
        assert_eq!(dst_clip_row_spans(Some(&clip), 2, 0, 20), vec![(5, 10)]);
        assert!(dst_clip_row_spans(Some(&clip), 3, 0, 20).is_empty());
    }

    #[test]
    fn eight_bit_src_copy_fast_path_respects_complex_dst_clip() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        bus.write_bytes(screen_base, &[0xEE; 8]);
        let pm = PixMapInfo {
            row_bytes: 8,
            bounds_top: 0,
            bounds_left: 0,
            bounds_bottom: 1,
            bounds_right: 8,
            pixel_size: 8,
            cmp_count: 1,
            pack_type: 0,
        };
        let mut src_to_dst = [0u8; 256];
        for (index, slot) in src_to_dst.iter_mut().enumerate() {
            *slot = 100u8.saturating_add(index as u8);
        }
        let clip = DstClip::new(
            (0, 0, 1, 8),
            vec![DstClipRegion::complex(0, 0, 1, 8, vec![vec![2, 5]])],
        );
        let mut scratch = Vec::new();

        assert!(try_blit_row_8bpp_src_copy_fast(
            &mut bus,
            &[1, 2, 3, 4, 5, 6, 7, 8],
            0,
            &pm,
            &src_to_dst,
            false,
            0,
            0,
            0,
            1,
            8,
            0,
            0,
            0,
            0,
            0,
            0,
            1,
            8,
            1.0,
            1.0,
            screen_base,
            8,
            8,
            1,
            8,
            None,
            Some(&clip),
            &mut scratch,
        ));

        assert_eq!(
            bus.read_bytes(screen_base, 8),
            vec![0xEE, 0xEE, 103, 104, 105, 0xEE, 0xEE, 0xEE]
        );
    }

    #[test]
    fn eight_bit_src_copy_fast_path_packs_one_bit_destination_rows() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        bus.write_byte(screen_base, 0xFF);
        let pm = PixMapInfo {
            row_bytes: 8,
            bounds_top: 0,
            bounds_left: 0,
            bounds_bottom: 1,
            bounds_right: 8,
            pixel_size: 8,
            cmp_count: 1,
            pack_type: 0,
        };
        let mut src_to_dst = [0u8; 256];
        for (index, slot) in src_to_dst.iter_mut().enumerate() {
            *slot = if index % 2 == 0 { 0 } else { 255 };
        }
        let mut scratch = Vec::new();

        assert!(try_blit_row_8bpp_src_copy_fast(
            &mut bus,
            &[0, 1, 2, 3, 4, 5, 6, 7],
            0,
            &pm,
            &src_to_dst,
            false,
            0,
            0,
            0,
            1,
            8,
            0,
            0,
            0,
            0,
            0,
            0,
            1,
            8,
            1.0,
            1.0,
            screen_base,
            1,
            8,
            1,
            1,
            None,
            None,
            &mut scratch,
        ));

        assert_eq!(bus.read_byte(screen_base), 0b0101_0101);
    }

    #[test]
    fn eight_bit_scaled_blit_fills_every_enlarged_destination_pixel() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        bus.write_bytes(screen_base, &[0xEE; 15]);
        let pm = PixMapInfo {
            row_bytes: 3,
            bounds_top: 0,
            bounds_left: 0,
            bounds_bottom: 2,
            bounds_right: 3,
            pixel_size: 8,
            cmp_count: 1,
            pack_type: 0,
        };
        let src_to_dst = std::array::from_fn(|index| index as u8);
        let indexed_transfer = std::array::from_fn(|index| PictIndexedTransfer::Write(index as u8));
        let device_clut = [[0u16; 3]; 256];
        let mut scratch = Vec::new();

        for (row, pixels) in [[1, 2, 3], [4, 5, 6]].iter().enumerate() {
            blit_row(
                &mut bus,
                pixels,
                0,
                &pm,
                &device_clut,
                &src_to_dst,
                true,
                &indexed_transfer,
                row as u32,
                0,
                0,
                2,
                3,
                0,
                0,
                0,
                0,
                0,
                0,
                3,
                5,
                1.0,
                1.0,
                screen_base,
                5,
                5,
                3,
                8,
                0,
                0,
                None,
                None,
                &mut scratch,
            );
        }

        assert_eq!(
            bus.read_bytes(screen_base, 15),
            vec![1, 1, 2, 3, 3, 4, 4, 5, 6, 6, 4, 4, 5, 6, 6]
        );
    }

    /// Draw a 12x4 8-bit source through `blit_row` under a PICT op mask
    /// region (PackBitsRgn), returning the destination rows. The picture
    /// frame is offset from the screen so picture-to-screen translation is
    /// exercised: picture x = screen x + 3, picture y = screen y + 5.
    fn draw_region_masked_rows(
        region: Option<&PictureRegion>,
        dst_clip: Option<&DstClip>,
        scrn_ps: u16,
    ) -> Vec<u8> {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        let screen_rb = if scrn_ps == 8 { 16 } else { 2 };
        bus.write_bytes(screen_base, &[0u8; 16 * 4]);
        let pm = PixMapInfo {
            row_bytes: 12,
            bounds_top: 0,
            bounds_left: 0,
            bounds_bottom: 4,
            bounds_right: 12,
            pixel_size: 8,
            cmp_count: 1,
            pack_type: 0,
        };
        // Distinct non-zero pixels; on a 1-bit screen every non-zero index
        // is "black" (bit set) via the identity map + threshold.
        let rows: Vec<Vec<u8>> = (0..4u8)
            .map(|y| (0..12u8).map(|x| 1 + y * 12 + x).collect())
            .collect();
        let src_to_dst = std::array::from_fn(|index| index as u8);
        let indexed_transfer = std::array::from_fn(|index| PictIndexedTransfer::Write(index as u8));
        let device_clut = [[0u16; 3]; 256];
        let mut scratch = Vec::new();
        for (row, pixels) in rows.iter().enumerate() {
            blit_row(
                &mut bus,
                pixels,
                0,
                &pm,
                &device_clut,
                &src_to_dst,
                true,
                &indexed_transfer,
                row as u32,
                0,
                0,
                4,
                12,
                0,  // dst_top
                0,  // dst_left
                5,  // frame_top
                3,  // frame_left
                5,  // pic_dst_top
                3,  // pic_dst_left
                9,  // pic_dst_bottom
                15, // pic_dst_right
                1.0,
                1.0,
                screen_base,
                screen_rb,
                16,
                4,
                scrn_ps,
                255,
                0,
                region,
                dst_clip,
                &mut scratch,
            );
        }
        bus.read_bytes(screen_base, (screen_rb * 4) as usize)
    }

    /// Reference: what the per-pixel path writes for `draw_region_masked_rows`
    /// on an 8-bit screen, using the very predicate it consults.
    fn expected_region_masked_rows(region: Option<&PictureRegion>) -> Vec<u8> {
        let mut expected = vec![0u8; 16 * 4];
        for y in 0..4i32 {
            for x in 0..12i32 {
                let (pic_y, pic_x) = (y + 5, x + 3);
                if region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                    continue;
                }
                expected[(y * 16 + x) as usize] = 1 + (y * 12 + x) as u8;
            }
        }
        expected
    }

    #[test]
    fn region_masked_8bpp_rows_take_the_span_fast_path_and_match_the_pixel_predicate() {
        // A notched region in picture coordinates: rows 5-6 cover picture
        // columns 4..13, row 7 covers 3..6 and 9..15 (a hole), row 8 is
        // empty. Row 5's edges also leave a trailing unpaired edge, which
        // `contains` reads as "to the right edge of the box".
        let notched = PictureRegion {
            top: 5,
            left: 3,
            bottom: 8,
            right: 15,
            rows: vec![vec![4, 13], vec![4], vec![3, 6, 9]],
        };
        assert_eq!(
            draw_region_masked_rows(Some(&notched), None, 8),
            expected_region_masked_rows(Some(&notched)),
            "notched op region: fast path must match the per-pixel predicate"
        );
        // A rectangular region (no rows) is its bounding box.
        let boxed = PictureRegion {
            top: 6,
            left: 5,
            bottom: 8,
            right: 11,
            rows: Vec::new(),
        };
        assert_eq!(
            draw_region_masked_rows(Some(&boxed), None, 8),
            expected_region_masked_rows(Some(&boxed))
        );
        // Combined with a complex port dst_clip: both intersect.
        let dst_clip = DstClip::new(
            (0, 0, 4, 16),
            vec![DstClipRegion::complex(
                0,
                0,
                4,
                16,
                vec![vec![0, 16], vec![2, 8], vec![0, 16], vec![0, 16]],
            )],
        );
        let both = draw_region_masked_rows(Some(&notched), Some(&dst_clip), 8);
        let mut expected = expected_region_masked_rows(Some(&notched));
        for x in (0..2).chain(8..16) {
            expected[16 + x] = 0;
        }
        assert_eq!(both, expected, "op region and dst_clip must both apply");
        // Non-vacuous: something was drawn and something was masked.
        assert!(both.iter().any(|&b| b != 0));
        assert!(expected_region_masked_rows(None) != expected_region_masked_rows(Some(&notched)));
    }

    #[test]
    fn region_masked_rows_are_admitted_to_the_row_fast_path() {
        // The eligibility gate used to refuse any op region, sending every
        // PackBitsRgn frame (EV Override's intro is a zoom sequence of them)
        // through the per-pixel loop. It must now be accepted, and it must
        // stay refused for scaling and non-srcCopy modes.
        let region = PictureRegion {
            top: 0,
            left: 0,
            bottom: 4,
            right: 12,
            rows: vec![vec![1, 5], vec![0, 12], vec![2, 3], vec![]],
        };
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let pm = PixMapInfo {
            row_bytes: 12,
            bounds_top: 0,
            bounds_left: 0,
            bounds_bottom: 4,
            bounds_right: 12,
            pixel_size: 8,
            cmp_count: 1,
            pack_type: 0,
        };
        let src_to_dst = std::array::from_fn(|index| index as u8);
        let row = [7u8; 12];
        let mut scratch = Vec::new();
        let taken = |bus: &mut MacMemoryBus, mode: u16, scale: f64, scratch: &mut Vec<u8>| {
            try_blit_row_8bpp_src_copy_fast(
                bus,
                &row,
                mode,
                &pm,
                &src_to_dst,
                true,
                1,
                0,
                0,
                4,
                12,
                0,
                0,
                0,
                0,
                0,
                0,
                4,
                12,
                scale,
                scale,
                0x08_0000,
                16,
                16,
                4,
                8,
                Some(&region),
                None,
                scratch,
            )
        };
        assert!(
            taken(&mut bus, 0, 1.0, &mut scratch),
            "srcCopy + region takes the row path"
        );
        assert!(
            !taken(&mut bus, 1, 1.0, &mut scratch),
            "srcOr still goes per-pixel"
        );
        assert!(
            !taken(&mut bus, 0, 2.0, &mut scratch),
            "scaling still goes per-pixel"
        );
        // Row 1 of the region is 0..12: whole row written; row 3 is empty.
        assert_eq!(bus.read_bytes(0x08_0000 + 16, 12), vec![7u8; 12]);
    }

    #[test]
    fn one_bit_screen_region_masked_rows_match_the_pixel_predicate() {
        let notched = PictureRegion {
            top: 5,
            left: 3,
            bottom: 9,
            right: 15,
            rows: vec![vec![4, 13], vec![4], vec![3, 6, 9], vec![]],
        };
        let bits = draw_region_masked_rows(Some(&notched), None, 1);
        let expected = expected_region_masked_rows(Some(&notched));
        for y in 0..4usize {
            for x in 0..16usize {
                let bit = (bits[y * 2 + x / 8] >> (7 - (x % 8))) & 1;
                let inside = expected[y * 16 + x] != 0;
                assert_eq!(
                    bit == 1,
                    inside,
                    "1-bit pixel ({y}, {x}) must follow the region"
                );
            }
        }
    }

    /// Draw a 20x4 1-bit source (bits from a fixed pattern, 3 bytes per
    /// row) through `blit_row` onto an 8-bit 24x4 screen with fg 200 /
    /// bg 30, in `mode`, under an optional op region; picture (x, y) =
    /// screen (x + 3, y + 5) as in the 8-bit tests. Returns the screen.
    fn draw_one_bit_source_rows(mode: u16, region: Option<&PictureRegion>) -> Vec<u8> {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        bus.write_bytes(screen_base, &[9u8; 24 * 4]);
        let pm = PixMapInfo {
            row_bytes: 3,
            bounds_top: 0,
            bounds_left: 0,
            bounds_bottom: 4,
            bounds_right: 20,
            pixel_size: 1,
            cmp_count: 1,
            pack_type: 0,
        };
        let rows: [[u8; 3]; 4] = [
            [0b1010_1100, 0b0011_1100, 0b1111_0000],
            [0b0101_0011, 0b1100_0011, 0b0000_1111],
            [0b1111_1111, 0b0000_0000, 0b1010_1010],
            [0b1000_0001, 0b0111_1110, 0b0101_0101],
        ];
        let src_to_dst = std::array::from_fn(|index| index as u8);
        let indexed_transfer = std::array::from_fn(|index| PictIndexedTransfer::Write(index as u8));
        let device_clut = [[0u16; 3]; 256];
        let mut scratch = Vec::new();
        for (row, bits) in rows.iter().enumerate() {
            blit_row(
                &mut bus,
                bits,
                mode,
                &pm,
                &device_clut,
                &src_to_dst,
                true,
                &indexed_transfer,
                row as u32,
                0,
                0,
                4,
                20,
                0,
                0,
                5,
                3,
                5,
                3,
                9,
                23,
                1.0,
                1.0,
                screen_base,
                24,
                24,
                4,
                8,
                200,
                30,
                region,
                None,
                &mut scratch,
            );
        }
        bus.read_bytes(screen_base, 24 * 4)
    }

    /// The per-pixel arm's answer for `draw_one_bit_source_rows`.
    fn expected_one_bit_source_rows(mode: u16, region: Option<&PictureRegion>) -> Vec<u8> {
        let rows: [[u8; 3]; 4] = [
            [0b1010_1100, 0b0011_1100, 0b1111_0000],
            [0b0101_0011, 0b1100_0011, 0b0000_1111],
            [0b1111_1111, 0b0000_0000, 0b1010_1010],
            [0b1000_0001, 0b0111_1110, 0b0101_0101],
        ];
        let mut screen = vec![9u8; 24 * 4];
        for y in 0..4i32 {
            for x in 0..20i32 {
                let set = rows[y as usize][(x / 8) as usize] & (0x80 >> (x % 8)) != 0;
                let index = if set {
                    200
                } else if mode == 0 {
                    30
                } else {
                    continue;
                };
                if region.is_some_and(|clip| !clip.contains(y + 5, x + 3)) {
                    continue;
                }
                screen[(y * 24 + x) as usize] = index;
            }
        }
        screen
    }

    #[test]
    fn one_bit_srccopy_source_rows_take_the_row_path_and_match_the_pixel_predicate() {
        let notched = PictureRegion {
            top: 5,
            left: 3,
            bottom: 9,
            right: 23,
            rows: vec![vec![4, 21], vec![6], vec![3, 9, 15], vec![]],
        };
        for region in [None, Some(&notched)] {
            assert_eq!(
                draw_one_bit_source_rows(0, region),
                expected_one_bit_source_rows(0, region),
                "srcCopy 1-bit rows (region {:?}) must match the per-pixel arm",
                region.is_some()
            );
        }
        // Non-srcCopy stays on the per-pixel arm (clear bits are skipped)
        // and must still be right.
        assert_eq!(
            draw_one_bit_source_rows(1, Some(&notched)),
            expected_one_bit_source_rows(1, Some(&notched))
        );
        // Non-vacuous: something was drawn with both indices.
        let drawn = draw_one_bit_source_rows(0, None);
        assert!(drawn.contains(&200) && drawn.contains(&30));
    }

    #[test]
    fn packbits_src_copy_fast_path_decodes_only_visible_mapped_span() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let pict_row = 0x04_0000u32;
        bus.write_byte(pict_row, 9); // byte count: flag + 8 literal pixels
        bus.write_byte(pict_row + 1, 7); // literal run of 8 bytes
        bus.write_bytes(pict_row + 2, &[1, 2, 3, 4, 5, 6, 7, 8]);

        let screen_base = 0x08_0000u32;
        bus.write_bytes(screen_base, &[0xEE; 4]);
        let pm = PixMapInfo {
            row_bytes: 8,
            bounds_top: 0,
            bounds_left: 0,
            bounds_bottom: 1,
            bounds_right: 8,
            pixel_size: 8,
            cmp_count: 1,
            pack_type: 0,
        };
        let mut src_to_dst = [0u8; 256];
        for (index, slot) in src_to_dst.iter_mut().enumerate() {
            *slot = index.wrapping_add(10) as u8;
        }
        let end = try_blit_packbits_8bpp_src_copy_fast(
            &mut bus,
            pict_row,
            1,
            &pm,
            0,
            &src_to_dst,
            false,
            0,
            0,
            1,
            8,
            0,
            -2,
            0,
            0,
            0,
            0,
            1,
            8,
            1.0,
            1.0,
            screen_base,
            4,
            4,
            1,
            8,
            None,
            None,
        )
        .expect("simple clipped 8bpp PackBits srcCopy should use the direct fast path");

        assert_eq!(end, pict_row + 10);
        assert_eq!(bus.read_bytes(screen_base, 4), vec![13, 14, 15, 16]);
    }

    fn write_peekable_indexed_packbits_pict(
        bus: &mut MacMemoryBus,
        pic: u32,
        mode: u16,
        leading_fill_rect: bool,
    ) {
        bus.write_word(pic, 0);
        bus.write_word(pic + 2, 0);
        bus.write_word(pic + 4, 0);
        bus.write_word(pic + 6, 1);
        bus.write_word(pic + 8, 2);

        let mut p = pic + 10;
        bus.write_byte(p, 0x11);
        p += 1;
        bus.write_byte(p, 0x01);
        p += 1;
        bus.write_byte(p, 0x0E); // FgColor state opcode: safe to skip.
        p += 1;
        bus.write_long(p, 0x0000_00CD);
        p += 4;

        if leading_fill_rect {
            bus.write_byte(p, 0x34);
            p += 1;
            for value in [0i16, 0, 1, 1] {
                bus.write_word(p, value as u16);
                p += 2;
            }
        }

        bus.write_byte(p, 0x98); // PackBitsRect
        p += 1;
        bus.write_word(p, 0x8002); // PixMap rowBytes = 2
        p += 2;
        for value in [0i16, 0, 1, 2] {
            bus.write_word(p, value as u16);
            p += 2;
        }
        bus.write_word(p, 0); // version
        p += 2;
        bus.write_word(p, 0); // packType
        p += 2;
        bus.write_long(p, 0); // packSize
        p += 4;
        bus.write_long(p, 0x0048_0000); // hRes
        p += 4;
        bus.write_long(p, 0x0048_0000); // vRes
        p += 4;
        bus.write_word(p, 0); // pixelType
        p += 2;
        bus.write_word(p, 8); // pixelSize
        p += 2;
        bus.write_word(p, 1); // cmpCount
        p += 2;
        bus.write_word(p, 8); // cmpSize
        p += 2;
        bus.write_long(p, 0); // planeBytes
        p += 4;
        bus.write_long(p, 0); // pmTable
        p += 4;
        bus.write_long(p, 0); // pmReserved
        p += 4;

        bus.write_long(p, 0); // ctSeed
        p += 4;
        bus.write_word(p, 0x8000); // implicit ColorSpec indexes
        p += 2;
        bus.write_word(p, 1); // entries 0 and 1
        p += 2;
        for (index, rgb) in [
            (0u16, [0x1111, 0x2222, 0x3333]),
            (1u16, [0xAAAA, 0xBBBB, 0xCCCC]),
        ] {
            bus.write_word(p, index);
            p += 2;
            for component in rgb {
                bus.write_word(p, component);
                p += 2;
            }
        }

        for _ in 0..2 {
            for value in [0i16, 0, 1, 2] {
                bus.write_word(p, value as u16);
                p += 2;
            }
        }
        bus.write_word(p, mode);
        p += 2;
        bus.write_bytes(p, &[0, 1]);
        p += 2;
        bus.write_byte(p, 0xFF);
        p += 1;
        bus.write_word(pic, (p - pic) as u16);
    }

    #[test]
    fn peek_initial_packbits_clut_accepts_simple_first_src_copy_image() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let pic = 0x10_0000u32;
        write_peekable_indexed_packbits_pict(&mut bus, pic, 0, false);

        let clut = peek_initial_packbits_clut(&bus, pic).expect("initial PackBitsRect CLUT");

        assert_eq!(clut[0], [0x1111, 0x2222, 0x3333]);
        assert_eq!(clut[1], [0xAAAA, 0xBBBB, 0xCCCC]);
    }

    #[test]
    fn peek_initial_packbits_clut_rejects_non_src_copy_images() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let pic = 0x10_0000u32;
        write_peekable_indexed_packbits_pict(&mut bus, pic, 1, false);

        assert!(peek_initial_packbits_clut(&bus, pic).is_none());
    }

    #[test]
    fn peek_initial_packbits_clut_rejects_after_prior_drawing_opcode() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let pic = 0x10_0000u32;
        write_peekable_indexed_packbits_pict(&mut bus, pic, 0, true);

        assert!(peek_initial_packbits_clut(&bus, pic).is_none());
    }

    #[test]
    fn src_copy_transfer_table_is_direct_write_mapping() {
        let mut src_to_dst = [0u8; 256];
        for (index, slot) in src_to_dst.iter_mut().enumerate() {
            *slot = 255u8.saturating_sub(index as u8);
        }
        let src = [[0u16; 3]; 256];
        let dst = [[0u16; 3]; 256];

        let table = build_pict_indexed_transfer_table(0, &src, &src_to_dst, &dst, 1, 2);

        assert_eq!(table[0], PictIndexedTransfer::Write(255));
        assert_eq!(table[42], PictIndexedTransfer::Write(213));
        assert_eq!(table[255], PictIndexedTransfer::Write(0));
    }

    #[test]
    fn four_bit_indexed_rows_fill_enlarged_destination_pixels() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        bus.write_bytes(screen_base, &[0xEE; 16]);

        let pm = PixMapInfo {
            row_bytes: 1,
            bounds_top: 0,
            bounds_left: 0,
            bounds_bottom: 2,
            bounds_right: 2,
            pixel_size: 4,
            cmp_count: 1,
            pack_type: 0,
        };
        let device_clut = [[0u16; 3]; 256];
        let mut src_to_dst = [0u8; 256];
        src_to_dst[1] = 10;
        src_to_dst[2] = 20;
        let transfer = build_pict_indexed_transfer_table(0, &[], &src_to_dst, &device_clut, 0, 0);
        let mut scratch = Vec::new();

        for (row, packed_pixels) in [(0, 0x12), (1, 0x21)] {
            blit_row(
                &mut bus,
                &[packed_pixels],
                0,
                &pm,
                &device_clut,
                &src_to_dst,
                false,
                &transfer,
                row,
                0,
                0,
                2,
                2,
                0,
                0,
                0,
                0,
                0,
                0,
                2,
                2,
                2.0,
                2.0,
                screen_base,
                4,
                4,
                4,
                8,
                0,
                0,
                None,
                None,
                &mut scratch,
            );
        }

        assert_eq!(
            bus.read_bytes(screen_base, 16),
            vec![
                10, 10, 20, 20, //
                10, 10, 20, 20, //
                20, 20, 10, 10, //
                20, 20, 10, 10,
            ]
        );
    }

    #[test]
    fn packbitsrect_matching_ctseed_still_translates_when_tables_differ() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        let screen_row_bytes = 8u32;
        bus.write_bytes(screen_base, &[0xEE; 8]);

        let mut device_clut = [[0u16; 3]; 256];
        device_clut[42] = [0xFFFF, 0, 0];

        let pic = 0x10_0000u32;
        let mut p = pic;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 1);
        p += 2;
        bus.write_word(p, 2);
        p += 2;

        bus.write_byte(p, 0x98); // PackBitsRect
        p += 1;
        bus.write_word(p, 0x8002); // PixMap rowBytes = 2
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 1);
        p += 2;
        bus.write_word(p, 2);
        p += 2;
        bus.write_word(p, 0); // version
        p += 2;
        bus.write_word(p, 0); // packType
        p += 2;
        bus.write_long(p, 0); // packSize
        p += 4;
        bus.write_long(p, 0x0048_0000); // hRes
        p += 4;
        bus.write_long(p, 0x0048_0000); // vRes
        p += 4;
        bus.write_word(p, 0); // pixelType
        p += 2;
        bus.write_word(p, 8); // pixelSize
        p += 2;
        bus.write_word(p, 1); // cmpCount
        p += 2;
        bus.write_word(p, 8); // cmpSize
        p += 2;
        bus.write_long(p, 0); // planeBytes
        p += 4;
        bus.write_long(p, 0); // pmTable
        p += 4;
        bus.write_long(p, 0); // pmReserved
        p += 4;

        bus.write_long(p, 8); // ctSeed matches destination seed, but table differs
        p += 4;
        bus.write_word(p, 0x8000);
        p += 2;
        bus.write_word(p, 1); // two ColorSpec entries
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 1);
        p += 2;
        bus.write_word(p, 0xFFFF);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;

        // srcRect, dstRect, mode
        for _ in 0..2 {
            bus.write_word(p, 0);
            p += 2;
            bus.write_word(p, 0);
            p += 2;
            bus.write_word(p, 1);
            p += 2;
            bus.write_word(p, 2);
            p += 2;
        }
        bus.write_word(p, 0); // srcCopy
        p += 2;

        // rowBytes < 8: raw row data, two source-index-1 red pixels.
        bus.write_byte(p, 1);
        p += 1;
        bus.write_byte(p, 1);
        p += 1;
        bus.write_byte(p, 0xFF); // EndOfPicture
        p += 1;
        bus.write_word(pic, (p - pic) as u16);

        let (ok, _) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            1,
            2,
            (screen_base, screen_row_bytes, 8, 1, 8),
            &device_clut,
            8,
            None,
        );

        assert!(ok);
        assert_eq!(
            bus.read_bytes(screen_base, 2),
            vec![42, 42],
            "matching ctSeed is not enough for identity mapping when ColorTable contents differ"
        );
    }

    #[test]
    fn one_bit_packbitsrect_uses_destination_clut_black_and_white() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        bus.write_bytes(screen_base, &[0x42; 8]);

        let mut clut = [[0x7777u16; 3]; 256];
        clut[4] = [0x0000, 0x0000, 0x0000];
        clut[7] = [0xFFFF, 0xFFFF, 0xFFFF];

        let pic = 0x10_0000u32;
        bus.write_word(pic, 42); // picSize
        bus.write_word(pic + 2, 0); // frame top
        bus.write_word(pic + 4, 0); // frame left
        bus.write_word(pic + 6, 1); // frame bottom
        bus.write_word(pic + 8, 8); // frame right
        let mut p = pic + 10;
        bus.write_byte(p, 0x11);
        p += 1;
        bus.write_byte(p, 0x01);
        p += 1;
        bus.write_byte(p, 0x98); // PackBitsRect
        p += 1;
        bus.write_word(p, 1); // rowBytes < 8: unpacked
        p += 2;
        for value in [0i16, 0, 1, 8] {
            bus.write_word(p, value as u16);
            p += 2;
        }
        for _ in 0..2 {
            for value in [0i16, 0, 1, 8] {
                bus.write_word(p, value as u16);
                p += 2;
            }
        }
        bus.write_word(p, 0); // srcCopy
        p += 2;
        bus.write_byte(p, 0b1010_0000);
        p += 1;
        bus.write_byte(p, 0xFF); // EndOfPicture

        let (ok, _) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            1,
            8,
            (screen_base, 8, 8, 1, 8),
            &clut,
            0,
            None,
        );

        assert!(ok);
        assert_eq!(bus.read_bytes(screen_base, 8), vec![4, 7, 4, 7, 7, 7, 7, 7]);
    }

    #[test]
    fn eight_bit_packbitsrect_srcor_preserves_white_source_pixels() {
        // Imaging With QuickDraw 1994, p. 4-33: with colored pixels,
        // srcOr applies foreground color for black source pixels and leaves
        // destination pixels alone for white source pixels.
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        bus.write_bytes(screen_base, &[42; 16]);

        let mut clut = [[0x7777u16; 3]; 256];
        clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
        clut[42] = [0x1234, 0x5678, 0x9ABC];
        clut[255] = [0x0000, 0x0000, 0x0000];

        let pic = 0x10_0000u32;
        bus.write_word(pic, 0); // picSize, patched at the end for clarity.
        bus.write_word(pic + 2, 0); // frame top
        bus.write_word(pic + 4, 0); // frame left
        bus.write_word(pic + 6, 4); // frame bottom
        bus.write_word(pic + 8, 4); // frame right
        let mut p = pic + 10;
        bus.write_byte(p, 0x11); // VersionOp
        p += 1;
        bus.write_byte(p, 0x01); // PICT v1
        p += 1;
        bus.write_byte(p, 0x98); // PackBitsRect
        p += 1;

        bus.write_word(p, 0x8004); // PixMap rowBytes, 8bpp, unpacked (< 8).
        p += 2;
        for value in [0i16, 0, 4, 4] {
            bus.write_word(p, value as u16);
            p += 2;
        }
        bus.write_word(p, 0); // pmVersion
        p += 2;
        bus.write_word(p, 0); // packType
        p += 2;
        bus.write_long(p, 0); // packSize
        p += 4;
        bus.write_long(p, 0x0048_0000); // hRes
        p += 4;
        bus.write_long(p, 0x0048_0000); // vRes
        p += 4;
        bus.write_word(p, 0); // pixelType indexed
        p += 2;
        bus.write_word(p, 8); // pixelSize
        p += 2;
        bus.write_word(p, 1); // cmpCount
        p += 2;
        bus.write_word(p, 8); // cmpSize
        p += 2;
        bus.write_long(p, 0); // planeBytes
        p += 4;
        bus.write_long(p, 0); // pmTable
        p += 4;
        bus.write_long(p, 0); // pmReserved
        p += 4;

        bus.write_long(p, 0); // ctSeed
        p += 4;
        bus.write_word(p, 0x8000); // ctFlags: entries are implicit indexes.
        p += 2;
        bus.write_word(p, 1); // ctSize: entries 0 and 1.
        p += 2;
        for (value, rgb) in [
            (0u16, [0xFFFF, 0xFFFF, 0xFFFF]),
            (1u16, [0x0000, 0x0000, 0x0000]),
        ] {
            bus.write_word(p, value);
            p += 2;
            for component in rgb {
                bus.write_word(p, component);
                p += 2;
            }
        }

        for _ in 0..2 {
            for value in [0i16, 0, 4, 4] {
                bus.write_word(p, value as u16);
                p += 2;
            }
        }
        bus.write_word(p, 1); // srcOr
        p += 2;
        bus.write_bytes(
            p,
            &[
                1, 1, 1, 0, //
                1, 0, 0, 0, //
                1, 0, 0, 0, //
                0, 0, 0, 0,
            ],
        );
        p += 16;
        bus.write_byte(p, 0xFF); // EndOfPicture
        p += 1;
        bus.write_word(pic, (p - pic) as u16);

        let (ok, _) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            4,
            4,
            (screen_base, 4, 4, 4, 8),
            &clut,
            0,
            None,
        );

        assert!(ok);
        assert_eq!(
            bus.read_bytes(screen_base, 16),
            vec![
                255, 255, 255, 42, //
                255, 42, 42, 42, //
                255, 42, 42, 42, //
                42, 42, 42, 42,
            ]
        );
    }

    #[test]
    fn indexed_transfer_table_precomputes_srcor_black_white_actions() {
        let mut src_clut = [[0u16; 3]; 256];
        src_clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
        src_clut[1] = [0x0000, 0x0000, 0x0000];
        src_clut[2] = [0x8000, 0x8000, 0x8000];

        let mut dst_clut = [[0x7777u16; 3]; 256];
        dst_clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
        dst_clut[42] = [0x8000, 0x8000, 0x8000];
        dst_clut[255] = [0x0000, 0x0000, 0x0000];

        let mut src_to_dst = [0u8; 256];
        src_to_dst[0] = 0;
        src_to_dst[1] = 255;
        src_to_dst[2] = 42;

        let table = build_pict_indexed_transfer_table(1, &src_clut, &src_to_dst, &dst_clut, 255, 0);

        assert_eq!(table[0], PictIndexedTransfer::Skip);
        assert_eq!(table[1], PictIndexedTransfer::Write(255));
        assert!(
            matches!(table[2], PictIndexedTransfer::Write(_)),
            "non-white source colors should be resolved once into the transfer table"
        );
    }

    #[test]
    fn indexed_src_copy_ignores_foreground_and_background_colors() {
        let mut src_clut = [[0u16; 3]; 256];
        src_clut[1] = [0xFFFF, 0x0000, 0x0000];

        let mut dst_clut = [[0x7777u16; 3]; 256];
        dst_clut[7] = [0x0000, 0xFFFF, 0x0000];
        dst_clut[42] = [0xFFFF, 0x0000, 0x0000];
        dst_clut[99] = [0x0000, 0x0000, 0xFFFF];

        let mut src_to_dst = [0u8; 256];
        src_to_dst[1] = 42;

        let table = build_pict_indexed_transfer_table(0, &src_clut, &src_to_dst, &dst_clut, 7, 99);

        assert_eq!(
            table[1],
            PictIndexedTransfer::Write(42),
            "indexed srcCopy should copy the source ColorTable color, not tint it through fg/bg"
        );
    }

    #[test]
    fn color_packbitsrect_to_one_bit_destination_maps_to_black_or_white() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        bus.write_byte(screen_base, 0);

        let pic = 0x10_0000u32;
        bus.write_word(pic, 0);
        bus.write_word(pic + 2, 0);
        bus.write_word(pic + 4, 0);
        bus.write_word(pic + 6, 1);
        bus.write_word(pic + 8, 3);
        let mut p = pic + 10;

        bus.write_byte(p, 0x98); // PackBitsRect
        p += 1;
        bus.write_word(p, 0x8003); // PixMap rowBytes = 3
        p += 2;
        for value in [0i16, 0, 1, 3] {
            bus.write_word(p, value as u16);
            p += 2;
        }
        bus.write_word(p, 0); // version
        p += 2;
        bus.write_word(p, 0); // packType
        p += 2;
        bus.write_long(p, 0); // packSize
        p += 4;
        bus.write_long(p, 0x0048_0000); // hRes
        p += 4;
        bus.write_long(p, 0x0048_0000); // vRes
        p += 4;
        bus.write_word(p, 0); // pixelType
        p += 2;
        bus.write_word(p, 8); // pixelSize
        p += 2;
        bus.write_word(p, 1); // cmpCount
        p += 2;
        bus.write_word(p, 8); // cmpSize
        p += 2;
        bus.write_long(p, 0); // planeBytes
        p += 4;
        bus.write_long(p, 0); // pmTable
        p += 4;
        bus.write_long(p, 0); // pmReserved
        p += 4;

        bus.write_long(p, 0); // ctSeed
        p += 4;
        bus.write_word(p, 0x8000); // ctFlags: ColorSpec values are indices
        p += 2;
        bus.write_word(p, 2); // ctSize: entries 0..2
        p += 2;
        for (index, [r, g, b]) in [
            (0u16, [0x0000, 0x0000, 0x0000]), // black
            (1u16, [0xFFFF, 0xFFFF, 0x0000]), // yellow, closer to white in 1bpp
            (2u16, [0xFFFF, 0xFFFF, 0xFFFF]), // white
        ] {
            bus.write_word(p, index);
            p += 2;
            bus.write_word(p, r);
            p += 2;
            bus.write_word(p, g);
            p += 2;
            bus.write_word(p, b);
            p += 2;
        }

        for _ in 0..2 {
            for value in [0i16, 0, 1, 3] {
                bus.write_word(p, value as u16);
                p += 2;
            }
        }
        bus.write_word(p, 0); // srcCopy
        p += 2;
        bus.write_byte(p, 0); // black source pixel
        p += 1;
        bus.write_byte(p, 1); // yellow should map to white on a 1bpp destination
        p += 1;
        bus.write_byte(p, 2); // white source pixel
        p += 1;
        bus.write_byte(p, 0xFF); // EndOfPicture
        p += 1;
        bus.write_word(pic, (p - pic) as u16);

        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        let (ok, _) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            1,
            3,
            (screen_base, 1, 8, 1, 1),
            &clut,
            0,
            None,
        );

        assert!(ok);
        assert_eq!(
            bus.read_byte(screen_base) & 0b1110_0000,
            0b1000_0000,
            "indexed color PICT pixels drawn into 1bpp must resolve against the black/white destination, not the full 8bpp CLUT"
        );
    }

    #[test]
    fn directbitsrect_packtype4_uses_pixmap_rowbytes_for_word_byte_counts() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        let screen_w: u16 = 80;
        let screen_h: u16 = 2;
        let row_bytes = u32::from(screen_w);
        bus.write_bytes(
            screen_base,
            &vec![0xAA; (row_bytes * u32::from(screen_h)) as usize],
        );

        let pic = 0x10_0000u32;
        let mut p = pic + 10;
        bus.write_byte(p, 0x11);
        p += 1; // VersionOp
        bus.write_byte(p, 0x02);
        p += 1; // PICT v2
        bus.write_byte(p, 0xFF);
        p += 1; // v2 version padding
        bus.write_byte(p, 0x00);
        p += 1; // align first word opcode

        bus.write_word(p, 0x009A);
        p += 2; // DirectBitsRect
        bus.write_long(p, 0x0000_00FF);
        p += 4; // baseAddr
        bus.write_word(p, 0x8000 | 320);
        p += 2; // PixMap rowBytes: 80 pixels * 4 bytes
        for value in [0i16, 0, 1, 80] {
            bus.write_word(p, value as u16);
            p += 2;
        }
        bus.write_word(p, 0);
        p += 2; // version
        bus.write_word(p, 4);
        p += 2; // packType 4: component PackBits, red first
        bus.write_long(p, 0);
        p += 4; // packSize
        bus.write_long(p, 0x0048_0000);
        p += 4; // hRes
        bus.write_long(p, 0x0048_0000);
        p += 4; // vRes
        bus.write_word(p, 16);
        p += 2; // direct pixelType
        bus.write_word(p, 32);
        p += 2; // pixelSize
        bus.write_word(p, 3);
        p += 2; // cmpCount: RGB only
        bus.write_word(p, 8);
        p += 2; // cmpSize
        bus.write_long(p, 0);
        p += 4; // planeBytes
        bus.write_long(p, 0);
        p += 4; // pmTable
        bus.write_long(p, 0);
        p += 4; // pmReserved

        for _ in 0..2 {
            for value in [0i16, 0, 1, 80] {
                bus.write_word(p, value as u16);
                p += 2;
            }
        }
        bus.write_word(p, 64);
        p += 2; // mode used by recorded direct-pixel PICTs

        // rowBytes is 320, so the scanline byte count is a word even though
        // the decoded 24-bit RGB planes are only 240 bytes.
        bus.write_word(p, 6);
        p += 2;
        for component in [0x12u8, 0x34, 0x56] {
            bus.write_byte(p, 0xB1); // repeat 80 bytes
            p += 1;
            bus.write_byte(p, component);
            p += 1;
        }

        bus.write_word(p, 0x0034);
        p += 2; // fillRect; proves the stream is still synchronized
        bus.write_word(p, 1);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 2);
        p += 2;
        bus.write_word(p, 80);
        p += 2;
        bus.write_word(p, 0x00FF);
        p += 2; // EndOfPicture

        bus.write_word(pic, (p - pic) as u16);
        bus.write_word(pic + 2, 0);
        bus.write_word(pic + 4, 0);
        bus.write_word(pic + 6, 2);
        bus.write_word(pic + 8, 80);

        let mut clut = [[0x8000u16, 0x8000, 0x8000]; 256];
        clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
        clut[42] = [0x1212, 0x3434, 0x5656];
        clut[255] = [0x0000, 0x0000, 0x0000];

        let (ok, _) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            screen_h as i16,
            screen_w as i16,
            (screen_base, row_bytes, screen_w, screen_h, 8),
            &clut,
            0,
            None,
        );

        assert!(ok);
        assert_eq!(bus.read_byte(screen_base), 42);
        assert_eq!(bus.read_byte(screen_base + row_bytes), 255);
    }

    #[test]
    fn directbitsrect_maps_rgbdirect_colors_into_indexed_and_16bpp_destinations() {
        for (pixel_size, expected_indexed) in [(2u16, Some(0x60u8)), (4, Some(0x12)), (16, None)] {
            for source_depth in [16u16, 32] {
                let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
                let screen_base = 0x08_0000u32;
                let pic = 0x10_0000u32;
                let mut p = pic + 10;
                bus.write_word(pic, 0);
                for (offset, value) in [(2, 0u16), (4, 0), (6, 1), (8, 2)] {
                    bus.write_word(pic + offset, value);
                }
                bus.write_byte(p, 0x11);
                p += 1;
                bus.write_byte(p, 0x02);
                p += 1;
                bus.write_byte(p, 0xFF);
                p += 1;
                bus.write_byte(p, 0);
                p += 1;
                bus.write_word(p, 0x009A);
                p += 2;
                bus.write_long(p, 0x0000_00FF);
                p += 4;
                let row_bytes = if source_depth == 16 { 4 } else { 8 };
                bus.write_word(p, 0x8000 | row_bytes);
                p += 2;
                for value in [0i16, 0, 1, 2] {
                    bus.write_word(p, value as u16);
                    p += 2;
                }
                bus.write_word(p, 0);
                p += 2;
                bus.write_word(p, 1);
                p += 2;
                bus.write_long(p, 0);
                p += 4;
                bus.write_long(p, 0x0048_0000);
                p += 4;
                bus.write_long(p, 0x0048_0000);
                p += 4;
                bus.write_word(p, 16);
                p += 2;
                bus.write_word(p, source_depth);
                p += 2;
                bus.write_word(p, if source_depth == 32 { 3 } else { 1 });
                p += 2;
                bus.write_word(p, if source_depth == 32 { 8 } else { 16 });
                p += 2;
                bus.write_long(p, 0);
                p += 4;
                bus.write_long(p, 0);
                p += 4;
                bus.write_long(p, 0);
                p += 4;
                for _ in 0..2 {
                    for value in [0i16, 0, 1, 2] {
                        bus.write_word(p, value as u16);
                        p += 2;
                    }
                }
                bus.write_word(p, 0);
                p += 2;
                let row: &[u8] = if source_depth == 16 {
                    &[0x7C, 0x00, 0x03, 0xE0]
                } else {
                    &[0xFF, 0, 0, 0xFF, 0, 0, 0, 0]
                };
                bus.write_bytes(p, row);
                p += row.len() as u32;
                bus.write_word(p, 0x00FF);
                p += 2;
                bus.write_word(pic, (p - pic) as u16);

                let mut clut = [[0u16; 3]; 256];
                clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
                clut[1] = [0xFFFF, 0, 0];
                clut[2] = [0, 0xFFFF, 0];
                if pixel_size < 8 {
                    clut[(1usize << pixel_size) - 1] = [0, 0, 0];
                }
                let (ok, _) = draw_picture(
                    &mut bus,
                    pic,
                    0,
                    0,
                    1,
                    2,
                    (
                        screen_base,
                        if pixel_size == 16 { 4 } else { 1 },
                        2,
                        1,
                        pixel_size,
                    ),
                    &clut,
                    0,
                    None,
                );
                assert!(ok);
                if let Some(expected) = expected_indexed {
                    assert_eq!(
                        bus.read_byte(screen_base),
                        expected,
                        "{source_depth}-bit DirectBitsRect into {pixel_size}-bit indexed"
                    );
                } else {
                    assert_eq!(bus.read_word(screen_base), 0x7c00);
                    assert_eq!(bus.read_word(screen_base + 2), 0x03e0);
                }
            }
        }
    }

    /// fillRect ($0x34) honors FillPat (0x0A) rather than PnPat (0x09) —
    /// otherwise it would be indistinguishable from paintRect ($0x31).
    /// Imaging With QuickDraw 1994, Appendix A, A-7.
    #[test]
    fn pict_fillrect_honors_fillpat_not_pnpat() {
        // 32×32 8bpp framebuffer at 0x08_0000.
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        let screen_w: u16 = 32;
        let screen_h: u16 = 32;
        let row_bytes = screen_w as u32;
        // Pre-fill with a sentinel so an untouched pixel is distinguishable.
        bus.write_bytes(
            screen_base,
            &vec![0x42; (row_bytes * screen_h as u32) as usize],
        );

        // Build a PICT v1 at 0x10_0000.
        let pic = 0x10_0000u32;
        let mut p = pic + 10;
        // version 1 (short opcodes)
        bus.write_byte(p, 0x11);
        p += 1; // versionOp
        bus.write_byte(p, 0x01);
        p += 1; // v1
                // PnPat (0x09): 8 bytes all 0x00 — set bits→fg, clear→bg.
                // An all-zero pattern fills with bg_idx (0 = white).
        bus.write_byte(p, 0x09);
        p += 1;
        bus.fill_zeros(p, 8);
        p += 8;
        // FillPat (0x0A): 8 bytes all 0xFF — fills with fg_idx
        // (255 = black).
        bus.write_byte(p, 0x0A);
        p += 1;
        bus.write_bytes(p, &[0xFFu8; 8]);
        p += 8;
        // fillRect (0x34): rect = (0, 0, 32, 32)
        bus.write_byte(p, 0x34);
        p += 1;
        bus.write_word(p, 0);
        p += 2; // top
        bus.write_word(p, 0);
        p += 2; // left
        bus.write_word(p, 32);
        p += 2; // bottom
        bus.write_word(p, 32);
        p += 2; // right
                // EndPic
        bus.write_byte(p, 0xFF);

        // picFrame = (0, 0, 32, 32)
        bus.write_word(pic, (p - pic + 1) as u16); // picSize (approx)
        bus.write_word(pic + 2, 0);
        bus.write_word(pic + 4, 0);
        bus.write_word(pic + 6, 32);
        bus.write_word(pic + 8, 32);

        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        let (_ok, _clut) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            32,
            32,
            (screen_base, row_bytes, screen_w, screen_h, 8),
            &clut,
            0,
            None,
        );

        // A representative interior pixel must be fg_idx (255 = black).
        // If fillRect were using PnPat (all-zeros), it would leak as
        // bg_idx (0 = white).
        let sample = bus.read_byte(screen_base + 8 * row_bytes + 8);
        assert_eq!(
            sample, 255,
            "fillRect must use FillPat (all-0xFF → fg=255), not PnPat \
             (all-0x00 → bg=0). Got 0x{:02X}.",
            sample,
        );
    }

    #[test]
    fn pict_v2_opcolor_advances_to_following_opcode() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        let screen_w: u16 = 16;
        let screen_h: u16 = 16;
        let row_bytes = screen_w as u32;
        bus.write_bytes(
            screen_base,
            &vec![0x42; (row_bytes * screen_h as u32) as usize],
        );

        let pic = 0x10_0000u32;
        let mut p = pic + 10;
        bus.write_byte(p, 0x11);
        p += 1; // versionOp
        bus.write_byte(p, 0x02);
        p += 1; // v2
        bus.write_byte(p, 0xFF);
        p += 1; // v2 version padding
        bus.write_byte(p, 0x00);
        p += 1; // align first word opcode
        bus.write_word(p, 0x001F);
        p += 2; // OpColor
        bus.write_word(p, 0x1111);
        p += 2;
        bus.write_word(p, 0x2222);
        p += 2;
        bus.write_word(p, 0x3333);
        p += 2;
        bus.write_word(p, 0x0034);
        p += 2; // fillRect
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 16);
        p += 2;
        bus.write_word(p, 16);
        p += 2;
        bus.write_word(p, 0x00FF);
        p += 2; // EndOfPicture

        bus.write_word(pic, (p - pic) as u16);
        bus.write_word(pic + 2, 0);
        bus.write_word(pic + 4, 0);
        bus.write_word(pic + 6, 16);
        bus.write_word(pic + 8, 16);

        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        let (ok, _) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            16,
            16,
            (screen_base, row_bytes, screen_w, screen_h, 8),
            &clut,
            0,
            None,
        );

        assert!(ok, "v2 OpColor should be skipped, not stop the PICT stream");
        assert_eq!(bus.read_byte(screen_base + 8 * row_bytes + 8), 255);
    }

    #[test]
    fn pict_v1_reserved_shape_opcode_advances_to_following_opcode() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        let screen_w: u16 = 16;
        let screen_h: u16 = 16;
        let row_bytes = screen_w as u32;
        bus.write_bytes(
            screen_base,
            &vec![0x42; (row_bytes * screen_h as u32) as usize],
        );

        let pic = 0x10_0000u32;
        let mut p = pic + 10;
        bus.write_byte(p, 0x11);
        p += 1; // versionOp
        bus.write_byte(p, 0x01);
        p += 1; // v1
        bus.write_byte(p, 0x35);
        p += 1; // reserved rect-family opcode: 8 bytes of data
        bus.write_bytes(p, &[0xAA; 8]);
        p += 8;
        bus.write_byte(p, 0x34);
        p += 1; // fillRect
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 16);
        p += 2;
        bus.write_word(p, 16);
        p += 2;
        bus.write_byte(p, 0xFF);
        p += 1; // EndOfPicture

        bus.write_word(pic, (p - pic) as u16);
        bus.write_word(pic + 2, 0);
        bus.write_word(pic + 4, 0);
        bus.write_word(pic + 6, 16);
        bus.write_word(pic + 8, 16);

        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        let (ok, _) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            16,
            16,
            (screen_base, row_bytes, screen_w, screen_h, 8),
            &clut,
            0,
            None,
        );

        assert!(
            ok,
            "v1 reserved shape opcodes should skip their data, not stop the PICT stream"
        );
        assert_eq!(bus.read_byte(screen_base + 8 * row_bytes + 8), 255);
    }

    /// fillPoly ($0x74) samples FillPat per pixel — alternating row pattern
    /// (rows 0,2,4,6 = 0xFF / rows 1,3,5,7 = 0x00) produces horizontal
    /// stripes of fg_idx / bg_idx.
    #[test]
    fn pict_fillpoly_honors_fillpat_striped_pattern() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        let screen_w: u16 = 32;
        let screen_h: u16 = 32;
        let row_bytes = screen_w as u32;
        bus.write_bytes(
            screen_base,
            &vec![0x42; (row_bytes * screen_h as u32) as usize],
        );
        let pic = 0x10_0000u32;
        let mut p = pic + 10;
        bus.write_byte(p, 0x11);
        p += 1; // VersionOp
        bus.write_byte(p, 0x01);
        p += 1; // v1
                // FillPat: alternating rows 0xFF / 0x00 → horizontal stripes.
        bus.write_byte(p, 0x0A);
        p += 1;
        for row in 0..8 {
            bus.write_byte(p, if row % 2 == 0 { 0xFF } else { 0x00 });
            p += 1;
        }
        // fillPoly ($0x74) — inline polySize(2) + bbox(8) + N*(v,h)(4)
        bus.write_byte(p, 0x74);
        p += 1;
        // polySize = 10 (header) + 4 verts × 4 bytes = 26
        let poly_size: u16 = 10 + 4 * 4;
        bus.write_word(p, poly_size);
        p += 2;
        bus.write_word(p, 4);
        p += 2; // bbox.top
        bus.write_word(p, 4);
        p += 2; // bbox.left
        bus.write_word(p, 28);
        p += 2; // bbox.bottom
        bus.write_word(p, 28);
        p += 2; // bbox.right
                // square verts
        for &(v, h) in &[(4i16, 4i16), (4, 28), (28, 28), (28, 4)] {
            bus.write_word(p, v as u16);
            p += 2;
            bus.write_word(p, h as u16);
            p += 2;
        }
        bus.write_byte(p, 0xFF); // EndPic

        bus.write_word(pic, (p - pic + 1) as u16);
        bus.write_word(pic + 2, 0);
        bus.write_word(pic + 4, 0);
        bus.write_word(pic + 6, 32);
        bus.write_word(pic + 8, 32);

        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        let _ = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            32,
            32,
            (screen_base, row_bytes, screen_w, screen_h, 8),
            &clut,
            0,
            None,
        );

        // Interior columns: sample two adjacent rows. With the stripe
        // pattern, even rows must land on an fg stripe, odd rows on
        // a bg stripe (or vice-versa). Assert they differ rather than
        // coupling to pattern phase, which depends on dy mod 8 within
        // the polygon bbox.
        let x = 10u32;
        let mut saw_fg = false;
        let mut saw_bg = false;
        for dy in 6..16u32 {
            let val = bus.read_byte(screen_base + dy * row_bytes + x);
            if val == 255 {
                saw_fg = true;
            }
            if val == 0 {
                saw_bg = true;
            }
        }
        assert!(saw_fg, "striped fillPoly must produce some fg_idx pixels");
        assert!(saw_bg, "striped fillPoly must produce some bg_idx pixels");
    }

    /// fillOval ($0x54) samples FillPat per pixel. Striped FillPat
    /// (alternating rows 0xFF / 0x00) must produce both fg_idx and bg_idx
    /// pixels inside the oval, not solid fg_idx.
    #[test]
    fn pict_filloval_honors_fillpat_striped_pattern() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        let screen_w: u16 = 32;
        let screen_h: u16 = 32;
        let row_bytes = screen_w as u32;
        bus.write_bytes(
            screen_base,
            &vec![0x42; (row_bytes * screen_h as u32) as usize],
        );
        let pic = 0x10_0000u32;
        let mut p = pic + 10;
        bus.write_byte(p, 0x11);
        p += 1;
        bus.write_byte(p, 0x01);
        p += 1;
        // FillPat: alternating rows 0xFF / 0x00.
        bus.write_byte(p, 0x0A);
        p += 1;
        for row in 0..8 {
            bus.write_byte(p, if row % 2 == 0 { 0xFF } else { 0x00 });
            p += 1;
        }
        // fillOval ($0x54): rect = (2, 2, 30, 30)
        bus.write_byte(p, 0x54);
        p += 1;
        bus.write_word(p, 2);
        p += 2;
        bus.write_word(p, 2);
        p += 2;
        bus.write_word(p, 30);
        p += 2;
        bus.write_word(p, 30);
        p += 2;
        bus.write_byte(p, 0xFF);

        bus.write_word(pic, (p - pic + 1) as u16);
        bus.write_word(pic + 2, 0);
        bus.write_word(pic + 4, 0);
        bus.write_word(pic + 6, 32);
        bus.write_word(pic + 8, 32);

        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        let _ = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            32,
            32,
            (screen_base, row_bytes, screen_w, screen_h, 8),
            &clut,
            0,
            None,
        );

        // Sample a vertical column through the oval's interior; expect
        // both fg and bg stripes to appear.
        let x = 16u32;
        let mut saw_fg = false;
        let mut saw_bg = false;
        for dy in 6..22u32 {
            let val = bus.read_byte(screen_base + dy * row_bytes + x);
            if val == 255 {
                saw_fg = true;
            }
            if val == 0 {
                saw_bg = true;
            }
        }
        assert!(saw_fg, "striped fillOval must produce some fg_idx pixels");
        assert!(saw_bg, "striped fillOval must produce some bg_idx pixels");
    }

    #[test]
    fn pict_bitsrect_decodes_unpacked_four_bit_pixmap() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let screen_base = 0x08_0000u32;
        let pic = 0x10_0000u32;
        let mut p = pic + 10;

        bus.write_byte(p, 0x00); // NOP, aligns the version opcode.
        p += 1;
        bus.write_byte(p, 0x11); // VersionOp.
        p += 1;
        bus.write_byte(p, 0x02);
        p += 1;
        bus.write_byte(p, 0xFF);
        p += 1;
        bus.write_word(p, 0x0090); // BitsRect with unpacked PixMap data.
        p += 2;
        bus.write_word(p, 0x8002); // PixMap flag and two bytes per row.
        p += 2;
        for value in [0i16, 0, 1, 4] {
            bus.write_word(p, value as u16);
            p += 2;
        }
        bus.write_word(p, 0); // pmVersion.
        p += 2;
        bus.write_word(p, 1); // packType: unpacked.
        p += 2;
        bus.write_long(p, 0); // packSize.
        p += 4;
        bus.write_long(p, 0x0048_0000); // hRes.
        p += 4;
        bus.write_long(p, 0x0048_0000); // vRes.
        p += 4;
        bus.write_word(p, 0); // indexed pixel type.
        p += 2;
        bus.write_word(p, 4); // pixelSize.
        p += 2;
        bus.write_word(p, 1); // cmpCount.
        p += 2;
        bus.write_word(p, 4); // cmpSize.
        p += 2;
        for _ in 0..3 {
            bus.write_long(p, 0); // planeBytes, pmTable, pmReserved.
            p += 4;
        }

        bus.write_long(p, 0); // ctSeed.
        p += 4;
        bus.write_word(p, 0); // explicit ColorSpec indexes.
        p += 2;
        bus.write_word(p, 1); // two entries.
        p += 2;
        for (index, component) in [(0u16, 0u16), (1, 0xFFFF)] {
            bus.write_word(p, index);
            p += 2;
            for _ in 0..3 {
                bus.write_word(p, component);
                p += 2;
            }
        }
        for _ in 0..2 {
            for value in [0i16, 0, 1, 4] {
                bus.write_word(p, value as u16);
                p += 2;
            }
        }
        bus.write_word(p, 0); // srcCopy.
        p += 2;
        bus.write_bytes(p, &[0x01, 0x10]);
        p += 2;
        bus.write_word(p, 0x00FF); // EndOfPicture.
        p += 2;

        bus.write_word(pic, (p - pic) as u16);
        for (offset, value) in [(2, 0u16), (4, 0), (6, 1), (8, 4)] {
            bus.write_word(pic + offset, value);
        }
        let mut clut = [[0u16; 3]; 256];
        clut[7] = [0xFFFF; 3];

        let (ok, _) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            1,
            4,
            (screen_base, 4, 4, 1, 8),
            &clut,
            0,
            None,
        );

        assert!(ok);
        assert_eq!(bus.read_bytes(screen_base, 4), vec![0, 7, 7, 0]);
        assert_eq!(
            picture_stream_len(&bus.read_bytes(pic, (p - pic) as usize)),
            Some((p - pic) as usize)
        );
    }

    #[test]
    fn pict_reserved_opcode_length_overflow_stops_without_panicking() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        let pic = 0x10_0000u32;
        for (offset, value) in [(2, 0u16), (4, 0), (6, 1), (8, 1)] {
            bus.write_word(pic + offset, value);
        }
        let mut p = pic + 10;
        bus.write_byte(p, 0x00);
        p += 1;
        bus.write_byte(p, 0x11);
        p += 1;
        bus.write_byte(p, 0x02);
        p += 1;
        bus.write_byte(p, 0xFF);
        p += 1;
        bus.write_word(p, 0x8100);
        p += 2;
        bus.write_long(p, u32::MAX);
        p += 4;
        bus.write_word(pic, (p - pic) as u16);

        let clut = [[0u16; 3]; 256];
        let result = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            1,
            1,
            (0x08_0000, 1, 1, 1, 8),
            &clut,
            0,
            None,
        );

        assert!(result.0);
    }
}
