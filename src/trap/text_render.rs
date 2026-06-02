//! Text rendering helpers (draw_char, draw_string, etc).

use super::types::{Rect, ShapeOp, UnderlineInfo};
use crate::cpu::CpuOps;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::quickdraw::fonts::heuristics::{
    get_italic_end_extend, get_italic_slant, get_italic_slant_for_underline,
    get_italic_underline_extend_left, get_italic_underline_extend_right, get_underline_offset,
    use_baseline_analysis, use_smart_underline_break,
};
use crate::quickdraw::fonts::{get_font_face_scaled, Glyph};
use crate::quickdraw::text::{get_font_metrics, get_glyph, get_glyph_italic};
use std::collections::HashSet;
use std::sync::OnceLock;

static TRACE_DIALOG_TEXT: OnceLock<bool> = OnceLock::new();

fn trace_dialog_text_enabled() -> bool {
    *TRACE_DIALOG_TEXT.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_TEXT").is_some())
}

/// Temporary buffer for string rendering (EXPERIMENTAL - NOT USED)
/// This implements QuickDraw's combined-buffer approach of rendering all glyphs
/// to a buffer first, then applying outline/shadow to the combined result.
/// See draw_string_buffered for details on why this isn't currently used.
#[allow(dead_code)]
struct StringBuffer {
    /// 1bpp bitmap data (row-major, MSB first) - current working buffer
    data: Vec<u8>,
    /// Original glyph data before smearing (for XOR operations)
    original: Vec<u8>,
    /// Bytes per row
    row_bytes: usize,
    /// Buffer width in pixels
    width: i16,
    /// Buffer height in pixels
    height: i16,
    /// Left edge in screen coordinates
    left: i16,
    /// Top edge in screen coordinates
    top: i16,
}

#[allow(dead_code)]
impl StringBuffer {
    /// Create a new buffer covering the given rectangle
    fn new(left: i16, top: i16, right: i16, bottom: i16) -> Self {
        let width = (right - left).max(0);
        let height = (bottom - top).max(0);
        let row_bytes = (width as usize).div_ceil(16) * 2; // Word-aligned
        let data = vec![0u8; row_bytes * height as usize];
        Self {
            original: data.clone(),
            data,
            row_bytes,
            width,
            height,
            left,
            top,
        }
    }

    /// Set a pixel in the buffer (screen coordinates)
    fn set_pixel(&mut self, x: i16, y: i16) {
        let bx = x - self.left;
        let by = y - self.top;
        if bx >= 0 && bx < self.width && by >= 0 && by < self.height {
            let byte_idx = (by as usize) * self.row_bytes + (bx as usize / 8);
            let bit = 7 - (bx % 8);
            if byte_idx < self.data.len() {
                self.data[byte_idx] |= 1 << bit;
            }
        }
    }

    /// Get a pixel from the working buffer (screen coordinates)
    fn get_pixel(&self, x: i16, y: i16) -> bool {
        let bx = x - self.left;
        let by = y - self.top;
        if bx >= 0 && bx < self.width && by >= 0 && by < self.height {
            let byte_idx = (by as usize) * self.row_bytes + (bx as usize / 8);
            let bit = 7 - (bx % 8);
            if byte_idx < self.data.len() {
                return (self.data[byte_idx] & (1 << bit)) != 0;
            }
        }
        false
    }

    /// Get a pixel from the original (pre-smear) buffer
    fn get_original_pixel(&self, x: i16, y: i16) -> bool {
        let bx = x - self.left;
        let by = y - self.top;
        if bx >= 0 && bx < self.width && by >= 0 && by < self.height {
            let byte_idx = (by as usize) * self.row_bytes + (bx as usize / 8);
            let bit = 7 - (bx % 8);
            if byte_idx < self.original.len() {
                return (self.original[byte_idx] & (1 << bit)) != 0;
            }
        }
        false
    }

    /// Save the current buffer state as the "original" for later XOR
    fn save_original(&mut self) {
        self.original = self.data.clone();
    }

    /// Apply QuickDraw smear algorithm for outline/shadow
    /// smear_h: horizontal smear distance (1 for outline, 2-3 for shadow)
    /// smear_v: vertical smear distance (same as smear_h typically)
    fn apply_smear(&mut self, smear_h: i16, smear_v: i16) {
        // QuickDraw smears RIGHT then DOWN

        // Smear right (multiple times)
        for _ in 0..smear_h {
            for y in 0..self.height as usize {
                let row_start = y * self.row_bytes;
                let mut carry = 0u8;
                for x in (0..self.row_bytes).rev() {
                    let byte = self.data[row_start + x];
                    let new_byte = byte | (byte >> 1) | (carry << 7);
                    carry = byte & 1;
                    self.data[row_start + x] = new_byte;
                }
            }
        }

        // Smear down (multiple times)
        for _ in 0..smear_v {
            for y in 1..self.height as usize {
                let prev_row = (y - 1) * self.row_bytes;
                let curr_row = y * self.row_bytes;
                for x in 0..self.row_bytes {
                    self.data[curr_row + x] |= self.data[prev_row + x];
                }
            }
        }
    }

    /// XOR the working buffer with the original buffer
    fn xor_with_original(&mut self) {
        for i in 0..self.data.len().min(self.original.len()) {
            self.data[i] ^= self.original[i];
        }
    }
}

impl super::TrapDispatcher {
    #[allow(dead_code)]
    pub(super) fn draw_string_buffered<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        s_ptr: u32,
        len: u8,
    ) {
        let is_bold = (self.tx_face & 1) != 0;
        let is_italic_requested = (self.tx_face & 2) != 0;
        let is_outline = (self.tx_face & 8) != 0;
        let is_shadow = (self.tx_face & 16) != 0;

        // Check if we should use pre-captured italic glyphs
        let use_precaptured_italic =
            is_italic_requested && get_glyph_italic(self.tx_font, self.tx_size, 'A').is_some();
        let is_italic = is_italic_requested && !use_precaptured_italic;

        let metrics = get_font_metrics(self.tx_font, self.tx_size);
        let (v, start_h) = self.pn_loc;

        // Calculate advance_extra (bold + outline + shadow expansion)
        // Must match advance_extra() function: add 1 for bold, 1 for outline, 2 for shadow
        let bold_extra: i16 = if is_bold { 1 } else { 0 };
        let outline_extra: i16 = if is_outline { 1 } else { 0 };
        let shadow_extra: i16 = if is_shadow { 2 } else { 0 };
        let advance_extra = bold_extra + outline_extra + shadow_extra;

        // Calculate italic slant
        let font_height = metrics.ascent + metrics.descent;
        let slant_at_top = if is_italic { font_height / 2 } else { 0 };

        // First pass: calculate string bounds
        let mut total_advance = 0i16;
        let mut min_left = i16::MAX;
        let mut max_right = i16::MIN;
        let mut current_h = start_h;

        for i in 0..len {
            let ch = bus.read_byte(s_ptr + 1 + i as u32) as char;
            let glyph_result = if use_precaptured_italic {
                get_glyph_italic(self.tx_font, self.tx_size, ch)
                    .or_else(|| get_glyph(self.tx_font, self.tx_size, ch))
            } else {
                get_glyph(self.tx_font, self.tx_size, ch)
            };

            if let Some((glyph, _)) = glyph_result {
                let left = current_h + glyph.origin_x as i16;
                let right = left + glyph.width as i16 + bold_extra + slant_at_top;
                min_left = min_left.min(left);
                max_right = max_right.max(right);
                current_h += glyph.advance as i16 + advance_extra;
                total_advance += glyph.advance as i16 + advance_extra;
            } else {
                current_h += 6 + advance_extra;
                total_advance += 6 + advance_extra;
            }
        }

        if min_left == i16::MAX {
            // No glyphs to render
            self.pn_loc.1 += total_advance;
            self.sync_current_port_draw_state(bus);
            return;
        }

        // Buffer bounds with padding for smear
        let style_pad = if is_shadow { 2 } else { 1 };
        let buf_left = min_left - style_pad;
        let buf_right = max_right + style_pad;
        let buf_top = v - metrics.ascent - style_pad;
        let buf_bottom = v + metrics.descent + style_pad;

        // Create buffer
        let mut buffer = StringBuffer::new(buf_left, buf_top, buf_right, buf_bottom);

        // Second pass: render glyphs to buffer (with italic/bold, no outline/shadow)
        current_h = start_h;
        let _top = v - metrics.ascent;

        for i in 0..len {
            let ch = bus.read_byte(s_ptr + 1 + i as u32) as char;
            let glyph_result = if use_precaptured_italic {
                get_glyph_italic(self.tx_font, self.tx_size, ch)
                    .or_else(|| get_glyph(self.tx_font, self.tx_size, ch))
            } else {
                get_glyph(self.tx_font, self.tx_size, ch)
            };

            if let Some((glyph, data)) = glyph_result {
                let left = current_h + glyph.origin_x as i16;

                // Render glyph pixels to buffer
                for gy in 0..glyph.height as i16 {
                    // Content row in glyph data
                    let content_row = gy;
                    // Screen Y position
                    let screen_y = v + glyph.origin_y as i16 + gy;

                    // Calculate italic slant for this row
                    let slant = if is_italic {
                        get_italic_slant(self.tx_font, self.tx_size, &metrics, v, screen_y)
                    } else {
                        0
                    };

                    for gx in 0..glyph.width as i16 {
                        // Glyph data is 8-bit coverage per pixel. The
                        // outline/shadow buffered path uses a 1-bit scratch
                        // buffer so threshold at >=128 coverage.
                        let b_idx = glyph.data_offset
                            + (content_row as usize) * glyph.width as usize
                            + gx as usize;
                        if b_idx < data.len() {
                            let pixel_set = data[b_idx] >= 128;
                            if pixel_set {
                                let screen_x = left + gx + slant;
                                buffer.set_pixel(screen_x, screen_y);

                                // Bold: also set pixel to the right
                                if is_bold {
                                    buffer.set_pixel(screen_x + 1, screen_y);
                                }
                            }
                        }
                    }
                }

                current_h += glyph.advance as i16 + advance_extra;
            } else {
                current_h += 6 + advance_extra;
            }
        }

        // Don't apply smear to buffer - do range check in blit phase instead
        // This exactly matches the per-glyph approach

        // Smear distance: 1 for outline, 2 for shadow, 3 for outline+shadow
        let smear_max = if is_shadow && is_outline {
            3
        } else if is_shadow {
            2
        } else {
            1
        };

        // Blit buffer to screen
        let r = Rect {
            top: buf_top,
            left: buf_left,
            bottom: buf_bottom,
            right: buf_right,
        };

        self.draw_generic_shape(cpu, bus, &r, ShapeOp::Glyph(self.tx_mode), |y, x| {
            // QuickDraw shadow/outline algorithm (from draw_char lines 2964-2974):
            // for dy in -1..=smear_max:
            //     for dx in -1..=smear_max:
            //         if source_pixel(y - dy, x - dx): is_smeared = true
            // pixel = is_smeared && !original(y, x)
            //
            // This checks if any source pixel in a range exists, then masks with
            // NOT original to create the hollow outline effect.

            let mut is_smeared = false;
            'smear: for dy in -1..=smear_max {
                for dx in -1..=smear_max {
                    if buffer.get_pixel(x - dx, y - dy) {
                        is_smeared = true;
                        break 'smear;
                    }
                }
            }

            let orig_pixel = buffer.get_pixel(x, y);
            // Outline/shadow smear is a binary halo — return full alpha so
            // the 8bpp blend path writes the clean foreground index where
            // the smear covers.
            if is_smeared && !orig_pixel {
                255
            } else {
                0
            }
        });

        // Update pen position
        self.pn_loc.1 += total_advance;
        self.sync_current_port_draw_state(bus);
    }

    pub(super) fn draw_string<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        s_ptr: u32,
    ) {
        let len = bus.read_byte(s_ptr);
        if len == 0 {
            return;
        }

        let is_underline = (self.tx_face & 4) != 0;
        let _is_outline = (self.tx_face & 8) != 0;
        let _is_shadow = (self.tx_face & 16) != 0;

        if !is_underline {
            // No underline, no outline/shadow: simple per-character drawing
            for i in 0..len {
                let ch = bus.read_byte(s_ptr + 1 + i as u32);
                self.draw_char(cpu, bus, ch as char);
            }
            return;
        }

        // For underlined text, we need to pre-compute descender break positions
        // across ALL characters so the underline is continuous but breaks correctly
        let is_bold = (self.tx_face & 1) != 0;
        let is_italic_requested = (self.tx_face & 2) != 0;
        let is_outline = (self.tx_face & 8) != 0;

        // Check if we should use pre-captured italic glyphs (same logic as draw_char)
        let use_precaptured_italic =
            is_italic_requested && get_glyph_italic(self.tx_font, self.tx_size, 'A').is_some();
        let is_italic = is_italic_requested && !use_precaptured_italic;
        let metrics = get_font_metrics(self.tx_font, self.tx_size);
        let start_x = self.pn_loc.1;
        let v = self.pn_loc.0; // baseline

        // First pass: collect character info and find all descender break positions
        let mut char_infos: Vec<(char, i16, i16)> = Vec::new(); // (char, start_x, end_x)
        let mut current_x = start_x;

        // Calculate the full advance_extra (matching draw_char)
        let bold_extra: i16 = if is_bold { 1 } else { 0 };
        let advance_extra: i16 = self.advance_extra();

        for i in 0..len {
            let ch = bus.read_byte(s_ptr + 1 + i as u32) as char;
            // Use pre-captured italic glyphs when available
            let glyph_result = if use_precaptured_italic {
                get_glyph_italic(self.tx_font, self.tx_size, ch)
                    .or_else(|| get_glyph(self.tx_font, self.tx_size, ch))
            } else {
                get_glyph(self.tx_font, self.tx_size, ch)
            };
            if let Some((glyph, _)) = glyph_result {
                let char_start = current_x;
                // For underline extent, only use bold_extra (underline doesn't extend with outline/shadow)
                let char_end = current_x + glyph.advance as i16 + bold_extra;
                char_infos.push((ch, char_start, char_end));
                // But pen advances by full advance_extra (matching draw_char)
                current_x += glyph.advance as i16 + advance_extra;
            } else {
                let char_start = current_x;
                let char_end = current_x + 6 + bold_extra;
                char_infos.push((ch, char_start, char_end));
                current_x += 6 + advance_extra;
            }
        }
        // Underline end is at the last character's end (without extra shadow/outline width)
        // For italic, extend by the slant at the underline row (approximately descent/2 ≈ 1-2 pixels)
        // Plus a bit more for the visual extent
        // Font-specific adjustment: NY14 needs extra pixel due to exclusive range edge case
        // Font-specific adjustment: NY14 needs extra pixel due to exclusive range edge case
        // NY18: Reverted non-italic extension as it caused extra pixels. Keeping italic logic.
        let extra_end_extend = if is_italic {
            get_italic_end_extend(self.tx_font, self.tx_size, &metrics)
        } else if use_precaptured_italic {
            // Font-specific end extension for pre-captured italic underlines
            match (self.tx_font, self.tx_size) {
                (0, 9) => 1,  // Chicago 9
                (4, 10) => 1, // Monaco 10
                _ => 0,       // Others don't need this
            }
        } else if is_bold && is_underline {
            // Bold underline needs small end extension for some fonts
            match (self.tx_font, self.tx_size) {
                (0, 9) => 1,  // Chicago 9 Bold+Underline
                (4, 10) => 1, // Monaco 10 Bold+Underline
                _ => 0,
            }
        } else {
            0
        };
        let end_x = if char_infos.is_empty() {
            start_x
        } else {
            char_infos.last().unwrap().2 + extra_end_extend
        };

        // Build set of x positions where underline should break (has descender)
        let ul_thick = crate::quickdraw::text::get_underline_thickness(self.tx_font, self.tx_size);
        let mut descender_breaks: Vec<HashSet<i16>> = vec![HashSet::new(); ul_thick as usize];

        for &(ch, char_start, _char_end) in &char_infos {
            // Use pre-captured italic glyphs when available (matching draw_char)
            let glyph_result = if use_precaptured_italic {
                get_glyph_italic(self.tx_font, self.tx_size, ch)
                    .or_else(|| get_glyph(self.tx_font, self.tx_size, ch))
            } else {
                get_glyph(self.tx_font, self.tx_size, ch)
            };
            if let Some((glyph, data)) = glyph_result {
                let top = v - metrics.ascent;
                let left = char_start + glyph.origin_x as i16;

                // Helper to check if there's a glyph pixel at given position
                let check_pixel = |check_y: i16, check_x: i16| -> bool {
                    let slant = if is_italic {
                        get_italic_slant_for_underline(
                            self.tx_font,
                            self.tx_size,
                            &metrics,
                            v,
                            check_y,
                        )
                    } else {
                        0
                    };
                    let c = check_x - left - slant;
                    let r = check_y - top;

                    if c < 0 || c >= glyph.width as i16 {
                        return false;
                    }

                    let content_row = r - metrics.ascent - glyph.origin_y as i16;
                    if content_row < 0 || content_row >= glyph.height as i16 {
                        return false;
                    }

                    // Glyph data is 8-bit coverage per pixel (row-major,
                    // one byte per pixel). Threshold at >=128 (bitmap
                    // glyphs are exclusively 0 or 255).
                    let b_idx = glyph.data_offset
                        + (content_row as usize) * glyph.width as usize
                        + c as usize;
                    if b_idx >= data.len() {
                        return false;
                    }
                    data[b_idx] >= 128
                };

                // Check bold pixel (includes smear)
                let check_bold_pixel = |check_y: i16, check_x: i16| -> bool {
                    if check_pixel(check_y, check_x) {
                        return true;
                    }
                    if is_bold && check_pixel(check_y, check_x - 1) {
                        return true;
                    }
                    false
                };

                // Helper to check the actual pixel that would be drawn (handling Outline)
                let check_effective_pixel = |check_y: i16, check_x: i16| -> bool {
                    let p = check_bold_pixel(check_y, check_x);
                    if !is_outline {
                        return p;
                    }
                    if !p {
                        return false;
                    }
                    // For Outline: pixel is ON only if it is an edge (neighbor is OFF)
                    // Check 4 neighbors
                    if !check_bold_pixel(check_y - 1, check_x)
                        || !check_bold_pixel(check_y + 1, check_x)
                        || !check_bold_pixel(check_y, check_x - 1)
                        || !check_bold_pixel(check_y, check_x + 1)
                    {
                        return true;
                    }
                    // All neighbors ON -> Internal pixel -> OFF in Outline
                    false
                };

                // For each x in the full underline range
                for x in start_x..end_x {
                    for row_idx in 0..ul_thick {
                        let target_y = v + 1 + row_idx;
                        let mut check_ys = vec![target_y - 1, target_y];
                        if metrics.descent > 2 {
                            check_ys.push(target_y + 1);
                        }

                        let mut found_any = false;
                        let mut max_dy = 0;
                        let mut max_width = 0;

                        // Check for pixels obstructing this row
                        for &check_y in &check_ys {
                            if check_effective_pixel(check_y, x) {
                                let dy_desc = check_y - v;
                                // Measure horizontal run length (NY18 logic)
                                let mut width = 1;
                                let mut lx = x - 1;
                                let mut rx = x + 1;
                                let use_smart_break =
                                    use_smart_underline_break(self.tx_font, self.tx_size);

                                if use_smart_break {
                                    while check_effective_pixel(check_y, lx) {
                                        width += 1;
                                        lx -= 1;
                                        if width > 40 {
                                            break;
                                        }
                                    }
                                    while check_effective_pixel(check_y, rx) {
                                        width += 1;
                                        rx += 1;
                                        if width > 40 {
                                            break;
                                        }
                                    }
                                    max_width = std::cmp::max(max_width, width);
                                }

                                if use_baseline_analysis(self.tx_font, self.tx_size) && dy_desc == 0
                                {
                                    let mut connects_down = false;
                                    let mut connects_up = false;
                                    let scan_start = lx.saturating_sub(3);
                                    let scan_end = rx.saturating_add(3);

                                    for scan_x in scan_start..scan_end {
                                        for dy_check in 1..=(metrics.descent + 2) {
                                            if check_effective_pixel(v + dy_check, scan_x) {
                                                connects_down = true;
                                                break;
                                            }
                                        }
                                        if check_effective_pixel(v - 1, scan_x) {
                                            connects_up = true;
                                        }
                                        if connects_down {
                                            break;
                                        }
                                    }

                                    let mut height_above = 0;
                                    for scan_x in (lx + 1)..rx {
                                        let mut h = 0;
                                        for up_dy in 1..=metrics.ascent {
                                            if check_effective_pixel(v - up_dy, scan_x) {
                                                h += 1;
                                            } else {
                                                break;
                                            }
                                        }
                                        height_above = std::cmp::max(height_above, h);
                                    }

                                    if !connects_down && connects_up && height_above > 7 {
                                        continue;
                                    }
                                }

                                found_any = true;
                                max_dy = std::cmp::max(max_dy, dy_desc);
                                if !use_smart_break {
                                    break;
                                }
                            }
                        }

                        if found_any {
                            let use_smart_break =
                                use_smart_underline_break(self.tx_font, self.tx_size);
                            if use_smart_break {
                                if max_width > 2 || max_dy <= 2 {
                                    descender_breaks[row_idx as usize].insert(x - 1);
                                    descender_breaks[row_idx as usize].insert(x);
                                    descender_breaks[row_idx as usize].insert(x + 1);
                                } else {
                                    descender_breaks[row_idx as usize].insert(x);
                                }
                            } else {
                                descender_breaks[row_idx as usize].insert(x - 1);
                                descender_breaks[row_idx as usize].insert(x);
                                descender_breaks[row_idx as usize].insert(x + 1);
                            }
                        }
                    }
                }
            }
        }

        // Store the underline info for draw_char to use
        self.underline_info = Some(UnderlineInfo {
            start_x,
            end_x,
            breaks: descender_breaks,
        });

        // Draw all characters (they will use the global underline info)
        for i in 0..len {
            let ch = bus.read_byte(s_ptr + 1 + i as u32);
            self.draw_char(cpu, bus, ch as char);
        }

        // Clear underline info
        self.underline_info = None;
    }

    pub(super) fn draw_char<C: CpuOps>(&mut self, cpu: &mut C, bus: &mut MacMemoryBus, ch: char) {
        // Check text face for styling
        let is_bold = (self.tx_face & 1) != 0;
        let is_italic_requested = (self.tx_face & 2) != 0;

        // Compute font scale factor (e.g., Chicago 24 = Chicago 12 at 2x)
        let (_face, font_scale) = get_font_face_scaled(self.tx_font, self.tx_size);
        let fs = font_scale; // shorthand

        // Try to get pre-captured italic glyph if italic is requested
        let (glyph_result, use_precaptured_italic) = if is_italic_requested {
            if let Some(result) = get_glyph_italic(self.tx_font, self.tx_size, ch) {
                (Some(result), true)
            } else {
                (get_glyph(self.tx_font, self.tx_size, ch), false)
            }
        } else {
            (get_glyph(self.tx_font, self.tx_size, ch), false)
        };

        if let Some((glyph, data)) = glyph_result {
            let (v, h) = self.pn_loc;

            // origin_y is distance from baseline to top of glyph (usually positive?)
            // If top is ABOVE baseline, and y increases DOWN.
            // Baseline is at `v`. Top is `v - origin_y`.
            // Let's verify this assumption later, but standard typography:
            // "Ascent" is positive up. Coordinates y+ down.
            // When using pre-captured italic glyphs, we don't apply slant transformation
            let is_italic = is_italic_requested && !use_precaptured_italic;
            let is_underline = (self.tx_face & 4) != 0;
            let is_outline = (self.tx_face & 8) != 0;
            let is_shadow = (self.tx_face & 16) != 0;

            let metrics = get_font_metrics(self.tx_font, self.tx_size);

            // Apply font scaling to metrics for coordinate calculations
            let scaled_ascent = metrics.ascent * fs;
            let scaled_descent = metrics.descent * fs;

            // Baseline is at `v`. The font's logical top is `v - scaled_ascent`.
            let top = v - scaled_ascent;
            let left = h + glyph.origin_x as i16 * fs;

            // Glyph's actual visual position (origin_y is negative for above baseline)
            // visual_top = baseline + origin_y * scale
            // visual_bottom = visual_top + height * scale
            let glyph_visual_top = v + glyph.origin_y as i16 * fs;
            let glyph_visual_bottom = glyph_visual_top + glyph.height as i16 * fs;
            let bottom = glyph_visual_bottom;
            let right = left + glyph.width as i16 * fs;

            // Slant amount at top
            let font_height = scaled_ascent + scaled_descent;
            let slant_at_top = if is_italic {
                // Common Mac Italic (e.g. Chicago/Geneva): slant is roughly height/2.
                // We apply a consistent algorithm.
                // For even height, integer division `dy / 2` behaves differently than
                // odd height in terms of sequence.
                // We use the `dy / 2` formula directly in `get_slant_pixel`.
                // Here we just need the max slant to calculate bounds.
                font_height / 2
            } else {
                0
            };
            let slant_at_bottom = 0;

            let bold_extra: i16 = if is_bold { 1 } else { 0 };
            let style_smear = bold_extra;

            // For Outline/Shadow, we expand in all directions
            let style_pad = if is_shadow {
                2
            } else if is_outline {
                1
            } else {
                0
            };

            let advance_extra = self.advance_extra();
            let ul_thick =
                crate::quickdraw::text::get_underline_thickness(self.tx_font, self.tx_size);

            // The drawing rectangle must cover:
            // 1. The full advance (for underlining)
            // 2. The slanted glyph (which might overhang)
            // 3. Style expansion (outline/shadow)
            // 4. Pixels outside the nominal ascent/descent (e.g. high capitals)
            // Extended underline for italic (1 pixel left of pen position)
            let italic_underline_extend = if is_italic && is_underline {
                get_italic_underline_extend_left(self.tx_font, self.tx_size, is_bold, false)
            } else {
                0
            };

            let underline_offset = if is_underline {
                get_underline_offset(self.tx_font, self.tx_size, glyph, is_shadow)
            } else {
                0
            };

            let draw_left = (h - style_pad - italic_underline_extend + underline_offset)
                .min(left + slant_at_bottom - style_pad);
            let draw_right =
                (h + glyph.advance as i16 * fs + advance_extra + style_pad + 2 + underline_offset)
                    .max(right + style_smear + slant_at_top + style_pad + 2);

            // Adjust top/bottom conservatively to include the glyph's actual visual top
            let visual_top = v + glyph.origin_y as i16 * fs;
            let draw_top = top.min(visual_top) - style_pad;

            // For shadow, we need to extend below the baseline by the shadow distance
            // even if the current glyph doesn't visually extend there
            // Add 1 for exclusive range upper bound
            let font_bottom = v + scaled_descent;
            let draw_bottom = if is_underline && is_shadow {
                (bottom + style_pad + 1)
                    .max(font_bottom + style_pad + 1)
                    .max(v + 6) // Underline @ v+1 + Shadow dist 3 + 1 for exclusive
            } else if is_underline {
                (bottom + style_pad).max(v + 1 + ul_thick + 1) // Underline @ v+1 + thick + 1 for exclusive range
            } else if is_shadow {
                (bottom + style_pad + 1).max(font_bottom + style_pad + 1) // Shadow extends below baseline + 1 for exclusive
            } else {
                bottom + style_pad
            };

            let r = Rect {
                top: draw_top,
                left: draw_left,
                bottom: draw_bottom,
                right: draw_right,
            };

            if trace_dialog_text_enabled()
                && self.current_port == 0x00B16310
                && (353..603).contains(&h)
                && (8..148).contains(&v)
            {
                eprintln!(
                    "[DIALOG-TEXT] draw_char port=${:08X} ch={:?} pnLoc=({}, {}) glyphRect=({},{}..{},{} ) glyphOrigin=({}, {}) glyphSize=({}, {}) txFont={} txSize={}",
                    self.current_port,
                    ch,
                    v,
                    h,
                    r.top,
                    r.left,
                    r.bottom,
                    r.right,
                    glyph.origin_x,
                    glyph.origin_y,
                    glyph.width,
                    glyph.height,
                    self.tx_font,
                    self.tx_size,
                );
            }

            if self.tx_mode == 0 {
                // srcCopy is opaque: clear the glyph bounds to background before drawing.
                let fbbdy = scaled_ascent + scaled_descent;
                let copy_rect = Rect {
                    top: v - (fbbdy - scaled_descent),
                    left: h,
                    bottom: v + scaled_descent,
                    right: h + glyph.advance as i16 * fs,
                };
                self.draw_rect(cpu, bus, &copy_rect, ShapeOp::Erase);
            }

            // Extract underline info for continuous underline (if set by draw_string)
            let underline_info = self
                .underline_info
                .as_ref()
                .map(|info| (info.start_x, info.end_x, info.breaks.clone()));

            self.draw_generic_shape(cpu, bus, &r, ShapeOp::Glyph(self.tx_mode), |y, x| {
                // All helper closures below return the per-pixel coverage
                // byte (0=off, 255=fully on, 1..254=partial). Bold smear
                // and underline clamp to full alpha where they set a pixel
                // (they're auxiliary strokes, not antialiased).
                let check_pixel = |r: i16, c: i16| -> u8 {
                    // Bounds check in SCALED coordinates first (avoids Rust's
                    // truncate-toward-zero division giving wrong results for negatives)
                    if c < 0 || c >= glyph.width as i16 * fs {
                        return 0;
                    }

                    // Map screen-relative row to glyph data row
                    // r is relative to font top (v - scaled_ascent)
                    // content_row_scaled = r - scaled_ascent - origin_y * fs
                    let content_row_scaled = r - scaled_ascent - glyph.origin_y as i16 * fs;
                    if content_row_scaled < 0 || content_row_scaled >= glyph.height as i16 * fs {
                        return 0;
                    }

                    // Now safe to divide (both values are non-negative)
                    let gc = c / fs;
                    let content_row = content_row_scaled / fs;

                    // Row-major 8-bit coverage (one byte per pixel).
                    let b_idx = glyph.data_offset
                        + (content_row as usize) * glyph.width as usize
                        + gc as usize;
                    if b_idx >= data.len() {
                        return 0;
                    }
                    data[b_idx]
                };

                // 1. Raw glyph with Italic slant
                let get_italic_pixel = |curr_y: i16, curr_x: i16| -> u8 {
                    let slant = if is_italic {
                        get_italic_slant(self.tx_font, self.tx_size, &metrics, v, curr_y) * fs
                    } else {
                        0
                    };
                    let c = curr_x - left - slant;
                    let r = curr_y - top;
                    check_pixel(r, c)
                };

                // 2. Add Bold smear: take the max of this column and the
                //    one to its left so antialiased stem edges stay
                //    crisp at their new rightward boundary.
                let get_bold_pixel = |curr_y: i16, curr_x: i16| -> u8 {
                    let p = get_italic_pixel(curr_y, curr_x);
                    if is_bold {
                        p.max(get_italic_pixel(curr_y, curr_x - 1))
                    } else {
                        p
                    }
                };

                // 3. Add Underline (uses global descender info if available).
                //    The underline ribbon is a solid, non-antialiased
                //    stroke: when the pixel lies inside the ribbon it
                //    gets clamped to full coverage (255) regardless of
                //    the antialiased glyph value beneath.
                let get_underlined_pixel = |curr_y: i16, curr_x: i16| -> u8 {
                    let mut p = get_bold_pixel(curr_y, curr_x);

                    if is_underline && curr_y > v && curr_y < v + 1 + ul_thick {
                        let italic_extend = if is_italic {
                            get_italic_underline_extend_left(
                                self.tx_font,
                                self.tx_size,
                                is_bold,
                                use_precaptured_italic,
                            )
                        } else {
                            0
                        };

                        let underline_offset =
                            get_underline_offset(self.tx_font, self.tx_size, glyph, is_shadow);

                        let italic_extend_right = if is_italic {
                            get_italic_underline_extend_right(self.tx_font, self.tx_size)
                        } else {
                            0
                        };

                        // Check if this x is within the underline range
                        let (underline_start, underline_end, in_range) =
                            if let Some((start, end, ref breaks)) = underline_info {
                                // Use global underline info from draw_string
                                let effective_start = start - italic_extend + underline_offset;
                                let effective_end = end + underline_offset + italic_extend_right;
                                let in_range = curr_x >= effective_start && curr_x < effective_end;

                                let row_idx = (curr_y - (v + 1)) as usize;
                                let has_break = if row_idx < breaks.len() {
                                    breaks[row_idx].contains(&curr_x)
                                } else {
                                    false
                                };

                                if in_range && !has_break {
                                    p = 255;
                                }
                                (effective_start, effective_end, in_range)
                            } else {
                                // Fallback: per-character underline
                                let start = h;
                                let end = h + glyph.advance as i16 + bold_extra;
                                let effective_start = start - italic_extend + underline_offset;
                                let effective_end = end + underline_offset + italic_extend_right;
                                let in_range = curr_x >= effective_start && curr_x < effective_end;
                                if in_range {
                                    // Check this character's descenders only
                                    // Note: Fallback doesn't support complex per-row/smart breaks yet,
                                    // but Geneva 24 shouldn't be using fallback heavily in contiguous strings.
                                    // If it does, we assume simplified break logic for now.
                                    let mut has_descender = false;
                                    for dy_desc in 0..=metrics.descent {
                                        // We use check_bold_pixel here since we don't have check_effective_pixel in scope
                                        // But wait, get_bold_pixel IS defined here.
                                        if get_bold_pixel(v + dy_desc, curr_x - 1) >= 128
                                            || get_bold_pixel(v + dy_desc, curr_x) >= 128
                                            || get_bold_pixel(v + dy_desc, curr_x + 1) >= 128
                                        {
                                            has_descender = true;
                                            break;
                                        }
                                    }
                                    if !has_descender {
                                        p = 255;
                                    }
                                }
                                (effective_start, end, in_range)
                            };
                        let _ = (underline_start, underline_end, in_range); // suppress warnings
                    }
                    p
                };

                let mut pixel = get_underlined_pixel(y, x);

                if is_outline || is_shadow {
                    // Shadow offset for smear should match the advance_extra (always 2)
                    let shadow_offset = 2;

                    // For "Everything" style, tune horizontal smear to preserve gaps
                    // while keeping edge shadows intact.
                    let is_everything =
                        is_italic && is_bold && is_outline && is_shadow && is_underline;
                    let (underline_start, underline_end) = underline_info
                        .as_ref()
                        .map(|(start, end, _)| (*start, *end))
                        .unwrap_or((i16::MIN, i16::MAX));
                    let in_underline_range = x >= underline_start && x < underline_end;
                    let is_break = underline_info
                        .as_ref()
                        .map(|(_, _, breaks)| {
                            // For smear, assume checking breaks[0] or generic break?
                            // Smear logic uses 'is_break' for line 3 (v+2).
                            // v+2 corresponds to row_idx 1.
                            if breaks.len() > 1 {
                                breaks[1].contains(&x)
                            } else if !breaks.is_empty() {
                                breaks[0].contains(&x)
                            } else {
                                false
                            }
                        })
                        .unwrap_or(false);

                    let smear_max = if is_outline && is_shadow {
                        3
                    } else if is_shadow {
                        shadow_offset
                    } else {
                        1
                    };
                    let dx_max = if is_everything {
                        if y == v {
                            if in_underline_range {
                                1
                            } else {
                                smear_max
                            }
                        } else if y == v + 1 {
                            if in_underline_range {
                                1
                            } else {
                                0
                            }
                        } else if y == v + 2 {
                            if is_break {
                                0
                            } else {
                                smear_max
                            }
                        } else {
                            smear_max
                        }
                    } else {
                        smear_max
                    };

                    let mut is_smeared = false;
                    if !(is_everything && y == v + 1 && !in_underline_range) {
                        // QuickDraw shadow algorithm (DrawText.a lines 837-901):
                        // 1. Smear buffer RIGHT (ROXR.L) and DOWN (OR with line above)
                        // 2. Draw smeared buffer at (-1, -1) offset
                        // 3. XOR with original at normal position
                        // The (-1,-1) offset means outline appears on ALL sides.
                        // The -1 in our loop accounts for this offset.
                        // Outline/shadow uses a binary smear test (>=128
                        // coverage counts as "set") — the halo stroke is
                        // not antialiased.
                        'smear: for dy in -1..=smear_max {
                            for dx in -1..=dx_max {
                                if get_underlined_pixel(y - dy, x - dx) >= 128 {
                                    is_smeared = true;
                                    break 'smear;
                                }
                            }
                        }
                    }

                    // Outline/shadow halo: full-coverage where smeared
                    // but the original glyph pixel is empty — producing
                    // the hollow/shifted silhouette effect.
                    pixel = if is_smeared && get_underlined_pixel(y, x) < 128 {
                        255
                    } else {
                        0
                    };
                }

                pixel
            });

            // Update Pen (Advance). Apply font scale, plus port.spExtra
            // for space characters per IM:I I-171 (SpaceExtra) and
            // self.char_extra for non-space characters per IM:V V-149
            // (CharExtra).
            let space_bonus = if ch == ' ' {
                self.space_extra_pixels(bus)
            } else {
                0
            };
            let char_bonus = if ch != ' ' {
                (self.char_extra >> 16) as i16
            } else {
                0
            };
            self.pn_loc = (
                v,
                h + glyph.advance as i16 * fs + advance_extra + space_bonus + char_bonus,
            );
        } else {
            // Missing glyph: advance by scaled default width
            let (_f, fs_missing) = get_font_face_scaled(self.tx_font, self.tx_size);
            self.pn_loc.1 += self.missing_glyph_advance() * fs_missing;
        }
        self.sync_current_port_draw_state(bus);
    }

    pub(super) fn advance_extra(&self) -> i16 {
        let is_bold = (self.tx_face & 1) != 0;
        let is_outline = (self.tx_face & 8) != 0;
        let is_shadow = (self.tx_face & 16) != 0;

        let mut extra: i16 = if is_bold { 1 } else { 0 };
        if is_outline {
            extra += 1;
        }
        if is_shadow {
            // Note: advance is always +2, but rendering smear uses +3 for size >= 18 (separate calculation)
            extra += 2;
        }
        extra
    }

    /// Read the current port's spExtra (Fixed-point) and return the
    /// integer-pixel portion to add to a space character's advance.
    /// Per IM:I I-171, SpaceExtra adds this value to every space drawn.
    pub(super) fn space_extra_pixels(&self, bus: &MacMemoryBus) -> i16 {
        if self.current_port == 0 {
            return 0;
        }
        // Fixed = 16.16. >> 16 keeps the integer part with sign.
        ((bus.read_long(self.current_port + 76) as i32) >> 16) as i16
    }

    pub(super) fn glyph_advance(&self, glyph: &Glyph) -> i16 {
        glyph.advance as i16 + self.advance_extra()
    }

    pub(super) fn missing_glyph_advance(&self) -> i16 {
        6 + self.advance_extra()
    }
}
