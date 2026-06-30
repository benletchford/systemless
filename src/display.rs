//! Shared framebuffer rendering for all Systemless frontends.
//!
//! Produces RGBA `Vec<u8>` pixel buffers from emulator screen memory,
//! supporting both 1bpp monochrome and 8bpp color modes.

use crate::memory::{MacMemoryBus, MemoryBus};

const BLACK_ARGB: u32 = 0xFF000000;
const WHITE_ARGB: u32 = 0xFFFFFFFF;
const BLACK_RGBA_WORD: u32 = u32::from_le_bytes([0x00, 0x00, 0x00, 0xFF]);
const WHITE_RGBA_WORD: u32 = u32::from_le_bytes([0xFF, 0xFF, 0xFF, 0xFF]);
const COMPACT_MAC_VIEWPORT_WIDTH: usize = 512;
const COMPACT_MAC_VIEWPORT_HEIGHT: usize = 342;
const COMPACT_MAC_VIEWPORT_SIDE_PAD: usize = 1;
const LETTERBOX_SAMPLE_STRIDE: usize = 8;
const LETTERBOX_FLOOD_TOLERANCE: u8 = 4;
const LETTERBOX_MIN_FLOOD_RATIO_NUM: usize = 3;
const LETTERBOX_MIN_FLOOD_RATIO_DEN: usize = 4;
const LETTERBOX_EDGE_TRIM_LIMIT: usize = 32;

pub type RgbaPalette = [u32; 256];
const UNUSED_RGBA_PALETTE: RgbaPalette = [0; 256];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CursorImage {
    Mono {
        data: [u8; 32],
        mask: [u8; 32],
        hot_v: i16,
        hot_h: i16,
    },
    Color {
        width: u16,
        height: u16,
        pixels_argb: Vec<u32>,
        mask: [u8; 32],
        hot_v: i16,
        hot_h: i16,
        mono_data: [u8; 32],
        mono_mask: [u8; 32],
    },
}

impl CursorImage {
    pub fn mono(data: [u8; 32], mask: [u8; 32], hot_v: i16, hot_h: i16) -> Self {
        Self::Mono {
            data,
            mask,
            hot_v,
            hot_h,
        }
    }

    pub fn mono_parts(&self) -> ([u8; 32], [u8; 32], i16, i16) {
        match self {
            Self::Mono {
                data,
                mask,
                hot_v,
                hot_h,
            } => (*data, *mask, *hot_v, *hot_h),
            Self::Color {
                mono_data,
                mono_mask,
                hot_v,
                hot_h,
                ..
            } => (*mono_data, *mono_mask, *hot_v, *hot_h),
        }
    }
}

/// Render the current screen to an RGBA pixel buffer (4 bytes per pixel).
///
/// Uses `ram_slice()` for bulk memory access. Supports both 1bpp and 8bpp modes.
/// The returned buffer has dimensions `width * height * 4` bytes.
pub fn render_screen(
    bus: &MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    device_clut: &[[u16; 3]; 256],
) -> Vec<u8> {
    let (_, _, scrn_w, scrn_h, _) = screen_mode;
    let mut pixels = Vec::with_capacity(scrn_w as usize * scrn_h as usize * 4);
    render_screen_into(bus, screen_mode, device_clut, &mut pixels);
    pixels
}

/// Render the current screen into a reusable RGBA pixel buffer.
pub fn render_screen_into(
    bus: &MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    device_clut: &[[u16; 3]; 256],
    pixels: &mut Vec<u8>,
) {
    if screen_mode.4 == 8 {
        let palette = rgba_palette_from_clut(device_clut);
        render_screen_with_rgba_palette_into(bus, screen_mode, &palette, pixels);
    } else {
        render_screen_with_rgba_palette_into(bus, screen_mode, &UNUSED_RGBA_PALETTE, pixels);
    }
}

pub fn rgba_palette_from_clut(device_clut: &[[u16; 3]; 256]) -> RgbaPalette {
    let mut palette = [0u32; 256];
    for (index, slot) in palette.iter_mut().enumerate() {
        let [r, g, b] = clut_to_rgba8(device_clut, index as u8);
        *slot = rgba_word(r, g, b);
    }
    palette
}

/// Render the current screen with a precomputed 8bpp RGBA palette.
///
/// This is useful for interactive frontends where the framebuffer is
/// converted every host frame but the CLUT changes much less often.
pub fn render_screen_with_rgba_palette_into(
    bus: &MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    palette: &RgbaPalette,
    pixels: &mut Vec<u8>,
) {
    let (scrn_base, row_bytes, scrn_w, scrn_h, pixel_size) = screen_mode;
    let w = scrn_w as u32;
    let h = scrn_h as u32;
    let is_8bpp = pixel_size == 8;

    pixels.resize((w * h * 4) as usize, 0);

    if w == 0 || h == 0 || row_bytes == 0 {
        pixels.fill(0);
        return;
    }

    let fb = bus.ram_slice(scrn_base, row_bytes * h);

    if is_8bpp {
        render_8bpp_rgba_rows(fb, row_bytes, w, h, palette, pixels);
    } else {
        render_1bpp_rgba_rows(fb, row_bytes, w, h, pixels);
    }
    normalize_centered_compact_mac_viewport_margins_rgba(pixels, w as usize, h as usize);
}

fn normalize_centered_compact_mac_viewport_margins_rgba(
    pixels: &mut [u8],
    width: usize,
    height: usize,
) -> bool {
    if width <= COMPACT_MAC_VIEWPORT_WIDTH + COMPACT_MAC_VIEWPORT_SIDE_PAD * 2
        || height <= COMPACT_MAC_VIEWPORT_HEIGHT
        || pixels.len() < width.saturating_mul(height).saturating_mul(4)
    {
        return false;
    }

    // The original compact Macintosh display was 512x342 pixels (Inside
    // Macintosh Volume III, hardware video chapter). Some early color titles
    // keep that logical viewport centered inside larger later-Mac screens.
    let content_left =
        ((width - COMPACT_MAC_VIEWPORT_WIDTH) / 2).saturating_sub(COMPACT_MAC_VIEWPORT_SIDE_PAD);
    let content_top = (height - COMPACT_MAC_VIEWPORT_HEIGHT) / 2;
    let content_right =
        (content_left + COMPACT_MAC_VIEWPORT_WIDTH + COMPACT_MAC_VIEWPORT_SIDE_PAD * 2).min(width);
    let content_bottom = (content_top + COMPACT_MAC_VIEWPORT_HEIGHT).min(height);

    if content_left == 0 || content_top == 0 || content_right >= width || content_bottom >= height {
        return false;
    }

    let Some(flood) = first_nonblack_outside_sample(
        pixels,
        width,
        height,
        content_left,
        content_top,
        content_right,
        content_bottom,
    ) else {
        return false;
    };

    let mut outside_samples = 0usize;
    let mut outside_flood_samples = 0usize;
    let mut inside_samples = 0usize;
    let mut inside_non_flood_samples = 0usize;

    for y in (0..height).step_by(LETTERBOX_SAMPLE_STRIDE) {
        for x in (0..width).step_by(LETTERBOX_SAMPLE_STRIDE) {
            let rgb = rgba_pixel_rgb(pixels, width, x, y);
            let inside =
                x >= content_left && x < content_right && y >= content_top && y < content_bottom;
            if inside {
                inside_samples += 1;
                if !rgb_near(rgb, flood, LETTERBOX_FLOOD_TOLERANCE) {
                    inside_non_flood_samples += 1;
                }
            } else {
                outside_samples += 1;
                if rgb_near(rgb, flood, LETTERBOX_FLOOD_TOLERANCE) {
                    outside_flood_samples += 1;
                }
            }
        }
    }

    if outside_samples == 0
        || outside_flood_samples * LETTERBOX_MIN_FLOOD_RATIO_DEN
            < outside_samples * LETTERBOX_MIN_FLOOD_RATIO_NUM
    {
        return false;
    }
    if inside_samples == 0 || inside_non_flood_samples < inside_samples / 4 {
        return false;
    }

    let (content_left, content_top, content_right, content_bottom) = trim_flood_edges_rgba(
        pixels,
        width,
        flood,
        content_left,
        content_top,
        content_right,
        content_bottom,
    );

    black_outside_rect_rgba(
        pixels,
        width,
        height,
        content_left,
        content_top,
        content_right,
        content_bottom,
    );
    true
}

fn trim_flood_edges_rgba(
    pixels: &[u8],
    width: usize,
    flood: [u8; 3],
    mut left: usize,
    mut top: usize,
    mut right: usize,
    mut bottom: usize,
) -> (usize, usize, usize, usize) {
    let min_width = COMPACT_MAC_VIEWPORT_WIDTH.saturating_sub(LETTERBOX_EDGE_TRIM_LIMIT);
    let min_height = COMPACT_MAC_VIEWPORT_HEIGHT.saturating_sub(LETTERBOX_EDGE_TRIM_LIMIT);
    let max_top = top.saturating_add(LETTERBOX_EDGE_TRIM_LIMIT).min(bottom);
    while top < max_top
        && bottom.saturating_sub(top) > min_height
        && row_or_column_is_mostly_flood(pixels, width, flood, left, top, right, top + 1)
    {
        top += 1;
    }

    let min_bottom = bottom.saturating_sub(LETTERBOX_EDGE_TRIM_LIMIT).max(top);
    while bottom > min_bottom
        && bottom.saturating_sub(top) > min_height
        && row_or_column_is_mostly_flood(pixels, width, flood, left, bottom - 1, right, bottom)
    {
        bottom -= 1;
    }

    let max_left = left.saturating_add(LETTERBOX_EDGE_TRIM_LIMIT).min(right);
    while left < max_left
        && right.saturating_sub(left) > min_width
        && row_or_column_is_mostly_flood(pixels, width, flood, left, top, left + 1, bottom)
    {
        left += 1;
    }

    let min_right = right.saturating_sub(LETTERBOX_EDGE_TRIM_LIMIT).max(left);
    while right > min_right
        && right.saturating_sub(left) > min_width
        && row_or_column_is_mostly_flood(pixels, width, flood, right - 1, top, right, bottom)
    {
        right -= 1;
    }

    (left, top, right, bottom)
}

fn row_or_column_is_mostly_flood(
    pixels: &[u8],
    width: usize,
    flood: [u8; 3],
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
) -> bool {
    let mut total = 0usize;
    let mut matches = 0usize;
    for y in (top..bottom).step_by(LETTERBOX_SAMPLE_STRIDE) {
        for x in (left..right).step_by(LETTERBOX_SAMPLE_STRIDE) {
            total += 1;
            if rgb_near(
                rgba_pixel_rgb(pixels, width, x, y),
                flood,
                LETTERBOX_FLOOD_TOLERANCE,
            ) {
                matches += 1;
            }
        }
    }
    total > 0 && matches * 8 >= total * 7
}

fn first_nonblack_outside_sample(
    pixels: &[u8],
    width: usize,
    height: usize,
    content_left: usize,
    content_top: usize,
    content_right: usize,
    content_bottom: usize,
) -> Option<[u8; 3]> {
    let probes = [
        (0, 0),
        (width / 2, 0),
        (0, height / 2),
        (width.saturating_sub(1), height / 2),
        (width / 2, height.saturating_sub(1)),
    ];
    for (x, y) in probes {
        let inside =
            x >= content_left && x < content_right && y >= content_top && y < content_bottom;
        if inside {
            continue;
        }
        let rgb = rgba_pixel_rgb(pixels, width, x, y);
        if !rgb_is_near_black(rgb) {
            return Some(rgb);
        }
    }
    None
}

#[inline]
fn rgba_pixel_rgb(pixels: &[u8], width: usize, x: usize, y: usize) -> [u8; 3] {
    let idx = (y * width + x) * 4;
    [pixels[idx], pixels[idx + 1], pixels[idx + 2]]
}

#[inline]
fn rgb_is_near_black(rgb: [u8; 3]) -> bool {
    rgb[0] <= 8 && rgb[1] <= 8 && rgb[2] <= 8
}

#[inline]
fn rgb_near(a: [u8; 3], b: [u8; 3], tolerance: u8) -> bool {
    a[0].abs_diff(b[0]) <= tolerance
        && a[1].abs_diff(b[1]) <= tolerance
        && a[2].abs_diff(b[2]) <= tolerance
}

fn black_outside_rect_rgba(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
) {
    for y in 0..height {
        let row = y * width * 4;
        if y < top || y >= bottom {
            for px in pixels[row..row + width * 4].chunks_exact_mut(4) {
                px.copy_from_slice(&[0, 0, 0, 0xFF]);
            }
            continue;
        }
        for px in pixels[row..row + left * 4].chunks_exact_mut(4) {
            px.copy_from_slice(&[0, 0, 0, 0xFF]);
        }
        for px in pixels[row + right * 4..row + width * 4].chunks_exact_mut(4) {
            px.copy_from_slice(&[0, 0, 0, 0xFF]);
        }
    }
}

#[inline]
fn render_8bpp_rgba_rows(
    fb: &[u8],
    row_bytes: u32,
    w: u32,
    h: u32,
    palette: &RgbaPalette,
    pixels: &mut [u8],
) {
    let dst = pixels.as_mut_ptr().cast::<u32>();
    let w = w as usize;
    let row_bytes = row_bytes as usize;
    for gy in 0..h as usize {
        let src_row = &fb[gy * row_bytes..];
        let dst_row = unsafe { dst.add(gy * w) };
        for gx in 0..w {
            let rgba = palette[src_row[gx] as usize];
            unsafe {
                std::ptr::write_unaligned(dst_row.add(gx), rgba);
            }
        }
    }
}

#[inline]
fn render_1bpp_rgba_rows(fb: &[u8], row_bytes: u32, w: u32, h: u32, pixels: &mut [u8]) {
    let dst = pixels.as_mut_ptr().cast::<u32>();
    let w = w as usize;
    let row_bytes = row_bytes as usize;
    for gy in 0..h as usize {
        let src_row = &fb[gy * row_bytes..];
        let dst_row = unsafe { dst.add(gy * w) };
        for gx in 0..w {
            let byte = src_row[gx / 8];
            let bit = 7 - (gx & 7);
            let rgba = if (byte & (1 << bit)) != 0 {
                BLACK_RGBA_WORD
            } else {
                WHITE_RGBA_WORD
            };
            unsafe {
                std::ptr::write_unaligned(dst_row.add(gx), rgba);
            }
        }
    }
}

/// Sample a single screen pixel as gamma-corrected RGB.
///
/// This uses the same framebuffer and CLUT semantics as `render_screen_into`
/// without allocating or rendering the full frame.
pub fn screen_pixel_rgb(
    bus: &MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    device_clut: &[[u16; 3]; 256],
    x: u32,
    y: u32,
) -> Option<[u8; 3]> {
    let (scrn_base, row_bytes, scrn_w, scrn_h, pixel_size) = screen_mode;
    let w = scrn_w as u32;
    let h = scrn_h as u32;
    if x >= w || y >= h || row_bytes == 0 {
        return None;
    }

    match pixel_size {
        8 => {
            let addr = scrn_base + y * row_bytes + x;
            let pixel = bus.read_byte(addr);
            Some(clut_to_rgba8(device_clut, pixel))
        }
        1 => {
            let addr = scrn_base + y * row_bytes + x / 8;
            let byte = bus.read_byte(addr);
            let bit = 7 - (x % 8);
            if (byte & (1 << bit)) != 0 {
                Some([0, 0, 0])
            } else {
                Some([255, 255, 255])
            }
        }
        _ => None,
    }
}

/// Render the current screen to an ARGB pixel buffer suitable for desktop backends.
///
/// Reuses the provided allocation so interactive frontends do not allocate a new
/// frame buffer on every render.
pub fn render_screen_argb(
    bus: &MacMemoryBus,
    screen_mode: (u32, u32, u16, u16, u16),
    device_clut: &[[u16; 3]; 256],
    pixels: &mut Vec<u32>,
) {
    let (scrn_base, row_bytes, scrn_w, scrn_h, pixel_size) = screen_mode;
    let w = scrn_w as usize;
    let h = scrn_h as usize;
    let is_8bpp = pixel_size == 8;
    let len = w.saturating_mul(h);

    pixels.resize(len, BLACK_ARGB);

    if w == 0 || h == 0 || row_bytes == 0 {
        pixels.fill(BLACK_ARGB);
        return;
    }

    let fb = bus.ram_slice(scrn_base, row_bytes * scrn_h as u32);

    if is_8bpp {
        let mut palette = [0u32; 256];
        for (dst, rgb) in palette.iter_mut().zip(device_clut.iter()) {
            let [r, g, b] = *rgb;
            *dst = 0xFF000000
                | (u32::from(clut_component_to_u8(r)) << 16)
                | (u32::from(clut_component_to_u8(g)) << 8)
                | u32::from(clut_component_to_u8(b));
        }

        for gy in 0..h {
            let row_start = gy * row_bytes as usize;
            let dst_row = &mut pixels[gy * w..(gy + 1) * w];
            for gx in 0..w {
                dst_row[gx] = palette[fb[row_start + gx] as usize];
            }
        }
    } else {
        for gy in 0..h {
            let row_start = gy * row_bytes as usize;
            let dst_row = &mut pixels[gy * w..(gy + 1) * w];
            for gx in 0..w {
                let byte = fb[row_start + (gx / 8)];
                let bit = 7 - (gx % 8);
                dst_row[gx] = if (byte & (1 << bit)) != 0 {
                    BLACK_ARGB
                } else {
                    WHITE_ARGB
                };
            }
        }
    }
}

/// Overlay the cursor onto an RGBA pixel buffer.
///
/// `cursor` is `(bitmap, mask, hotspot_v, hotspot_h)` — 16x16 1-bit cursor.
/// `mouse_pos` is `(v, h)` in Mac screen coordinates.
pub fn render_cursor(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    cursor: &CursorImage,
    mouse_pos: (i16, i16),
) {
    match cursor {
        CursorImage::Mono {
            data,
            mask,
            hot_v,
            hot_h,
        } => render_mono_cursor(pixels, width, height, data, mask, *hot_v, *hot_h, mouse_pos),
        CursorImage::Color {
            width: cursor_w,
            height: cursor_h,
            pixels_argb,
            mask,
            hot_v,
            hot_h,
            ..
        } => render_color_cursor_rgba(
            pixels,
            width,
            height,
            *cursor_w,
            *cursor_h,
            pixels_argb,
            mask,
            *hot_v,
            *hot_h,
            mouse_pos,
        ),
    }
}

fn render_mono_cursor(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    data: &[u8; 32],
    mask: &[u8; 32],
    hot_v: i16,
    hot_h: i16,
    mouse_pos: (i16, i16),
) {
    let (mouse_v, mouse_h) = mouse_pos;
    let cx = mouse_h as i32 - hot_h as i32;
    let cy = mouse_v as i32 - hot_v as i32;

    for row in 0..16i32 {
        let data_word =
            ((data[(row * 2) as usize] as u16) << 8) | data[(row * 2 + 1) as usize] as u16;
        let mask_word =
            ((mask[(row * 2) as usize] as u16) << 8) | mask[(row * 2 + 1) as usize] as u16;

        for col in 0..16i32 {
            let bit = 15 - col;
            if (mask_word >> bit) & 1 == 0 {
                continue;
            }
            let gx = cx + col;
            let gy = cy + row;
            if gx < 0 || gy < 0 || gx >= width as i32 || gy >= height as i32 {
                continue;
            }
            let data_bit = (data_word >> bit) & 1;
            let idx = ((gy as u32 * width + gx as u32) * 4) as usize;
            if data_bit == 1 {
                pixels[idx] = 0;
                pixels[idx + 1] = 0;
                pixels[idx + 2] = 0;
            } else {
                pixels[idx] = 255;
                pixels[idx + 1] = 255;
                pixels[idx + 2] = 255;
            }
            pixels[idx + 3] = 255;
        }
    }
}

/// Overlay the cursor onto an ARGB pixel buffer.
pub fn render_cursor_argb(
    pixels: &mut [u32],
    width: u32,
    height: u32,
    cursor: &CursorImage,
    mouse_pos: (i16, i16),
) {
    match cursor {
        CursorImage::Mono {
            data,
            mask,
            hot_v,
            hot_h,
        } => render_mono_cursor_argb(pixels, width, height, data, mask, *hot_v, *hot_h, mouse_pos),
        CursorImage::Color {
            width: cursor_w,
            height: cursor_h,
            pixels_argb,
            mask,
            hot_v,
            hot_h,
            ..
        } => render_color_cursor_argb(
            pixels,
            width,
            height,
            *cursor_w,
            *cursor_h,
            pixels_argb,
            mask,
            *hot_v,
            *hot_h,
            mouse_pos,
        ),
    }
}

fn render_mono_cursor_argb(
    pixels: &mut [u32],
    width: u32,
    height: u32,
    data: &[u8; 32],
    mask: &[u8; 32],
    hot_v: i16,
    hot_h: i16,
    mouse_pos: (i16, i16),
) {
    let (mouse_v, mouse_h) = mouse_pos;
    let cx = mouse_h as i32 - hot_h as i32;
    let cy = mouse_v as i32 - hot_v as i32;

    for row in 0..16i32 {
        let data_word =
            ((data[(row * 2) as usize] as u16) << 8) | data[(row * 2 + 1) as usize] as u16;
        let mask_word =
            ((mask[(row * 2) as usize] as u16) << 8) | mask[(row * 2 + 1) as usize] as u16;

        for col in 0..16i32 {
            let bit = 15 - col;
            if (mask_word >> bit) & 1 == 0 {
                continue;
            }
            let gx = cx + col;
            let gy = cy + row;
            if gx < 0 || gy < 0 || gx >= width as i32 || gy >= height as i32 {
                continue;
            }
            let data_bit = (data_word >> bit) & 1;
            let idx = gy as usize * width as usize + gx as usize;
            pixels[idx] = if data_bit == 1 {
                BLACK_ARGB
            } else {
                WHITE_ARGB
            };
        }
    }
}

fn color_cursor_mask_bit(mask: &[u8; 32], row: u16, col: u16) -> bool {
    if row >= 16 || col >= 16 {
        return false;
    }
    let row = row as usize;
    let bit = 15 - col;
    let word = ((mask[row * 2] as u16) << 8) | mask[row * 2 + 1] as u16;
    ((word >> bit) & 1) != 0
}

fn render_color_cursor_rgba(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    cursor_w: u16,
    cursor_h: u16,
    pixels_argb: &[u32],
    mask: &[u8; 32],
    hot_v: i16,
    hot_h: i16,
    mouse_pos: (i16, i16),
) {
    let (mouse_v, mouse_h) = mouse_pos;
    let cx = mouse_h as i32 - hot_h as i32;
    let cy = mouse_v as i32 - hot_v as i32;

    for row in 0..cursor_h {
        for col in 0..cursor_w {
            let gx = cx + col as i32;
            let gy = cy + row as i32;
            if gx < 0 || gy < 0 || gx >= width as i32 || gy >= height as i32 {
                continue;
            }
            let Some(&argb) = pixels_argb.get(row as usize * cursor_w as usize + col as usize)
            else {
                continue;
            };
            let idx = ((gy as u32 * width + gx as u32) * 4) as usize;
            if color_cursor_mask_bit(mask, row, col) {
                pixels[idx] = ((argb >> 16) & 0xFF) as u8;
                pixels[idx + 1] = ((argb >> 8) & 0xFF) as u8;
                pixels[idx + 2] = (argb & 0xFF) as u8;
                pixels[idx + 3] = 0xFF;
            } else if argb == BLACK_ARGB {
                pixels[idx] = !pixels[idx];
                pixels[idx + 1] = !pixels[idx + 1];
                pixels[idx + 2] = !pixels[idx + 2];
                pixels[idx + 3] = 0xFF;
            }
        }
    }
}

fn render_color_cursor_argb(
    pixels: &mut [u32],
    width: u32,
    height: u32,
    cursor_w: u16,
    cursor_h: u16,
    pixels_argb: &[u32],
    mask: &[u8; 32],
    hot_v: i16,
    hot_h: i16,
    mouse_pos: (i16, i16),
) {
    let (mouse_v, mouse_h) = mouse_pos;
    let cx = mouse_h as i32 - hot_h as i32;
    let cy = mouse_v as i32 - hot_v as i32;

    for row in 0..cursor_h {
        for col in 0..cursor_w {
            let gx = cx + col as i32;
            let gy = cy + row as i32;
            if gx < 0 || gy < 0 || gx >= width as i32 || gy >= height as i32 {
                continue;
            }
            let Some(&argb) = pixels_argb.get(row as usize * cursor_w as usize + col as usize)
            else {
                continue;
            };
            let idx = gy as usize * width as usize + gx as usize;
            if color_cursor_mask_bit(mask, row, col) {
                pixels[idx] = argb | 0xFF00_0000;
            } else if argb == BLACK_ARGB {
                pixels[idx] ^= 0x00FF_FFFF;
            }
        }
    }
}

/// Overlay line-oriented debug text onto an ARGB framebuffer.
pub fn render_debug_overlay_argb(pixels: &mut [u32], width: u32, height: u32, lines: &[String]) {
    if pixels.is_empty() || width == 0 || height == 0 || lines.is_empty() {
        return;
    }

    let width = width as usize;
    let height = height as usize;
    if pixels.len() < width.saturating_mul(height) {
        return;
    }

    let margin = 6usize;
    let pad = 4usize;
    let char_w = 6usize;
    let line_h = 8usize;
    if width <= margin * 2 || height <= margin * 2 {
        return;
    }

    let max_chars = ((width - margin * 2).saturating_sub(pad * 2) / char_w).max(1);
    let max_lines = ((height - margin * 2).saturating_sub(pad * 2) / line_h).max(1);
    let visible_lines = lines.len().min(max_lines);
    let longest = lines
        .iter()
        .take(visible_lines)
        .map(|line| line.chars().count().min(max_chars))
        .max()
        .unwrap_or(0);
    if longest == 0 {
        return;
    }

    let panel_x = margin;
    let panel_y = margin;
    let panel_w = (longest * char_w + pad * 2).min(width - panel_x);
    let panel_h = (visible_lines * line_h + pad * 2).min(height - panel_y);
    fill_rect_argb(
        pixels, width, height, panel_x, panel_y, panel_w, panel_h, 0xFF101010,
    );
    stroke_rect_argb(
        pixels, width, height, panel_x, panel_y, panel_w, panel_h, 0xFF4A9C63,
    );

    let text_x = panel_x + pad;
    let mut text_y = panel_y + pad;
    for line in lines.iter().take(visible_lines) {
        draw_debug_text_argb(
            pixels, width, height, text_x, text_y, line, max_chars, 0xFFE8F0EA,
        );
        text_y += line_h;
    }
}

/// Overlay line-oriented debug text onto an RGBA framebuffer.
pub fn render_debug_overlay_rgba(pixels: &mut [u8], width: u32, height: u32, lines: &[String]) {
    if pixels.is_empty() || width == 0 || height == 0 || lines.is_empty() {
        return;
    }

    let width = width as usize;
    let height = height as usize;
    if pixels.len() < width.saturating_mul(height).saturating_mul(4) {
        return;
    }

    let margin = 6usize;
    let pad = 4usize;
    let char_w = 6usize;
    let line_h = 8usize;
    if width <= margin * 2 || height <= margin * 2 {
        return;
    }

    let max_chars = ((width - margin * 2).saturating_sub(pad * 2) / char_w).max(1);
    let max_lines = ((height - margin * 2).saturating_sub(pad * 2) / line_h).max(1);
    let visible_lines = lines.len().min(max_lines);
    let longest = lines
        .iter()
        .take(visible_lines)
        .map(|line| line.chars().count().min(max_chars))
        .max()
        .unwrap_or(0);
    if longest == 0 {
        return;
    }

    let panel_x = margin;
    let panel_y = margin;
    let panel_w = (longest * char_w + pad * 2).min(width - panel_x);
    let panel_h = (visible_lines * line_h + pad * 2).min(height - panel_y);
    fill_rect_rgba(
        pixels,
        width,
        height,
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        [0x10, 0x10, 0x10, 0xFF],
    );
    stroke_rect_rgba(
        pixels,
        width,
        height,
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        [0x4A, 0x9C, 0x63, 0xFF],
    );

    let text_x = panel_x + pad;
    let mut text_y = panel_y + pad;
    for line in lines.iter().take(visible_lines) {
        draw_debug_text_rgba(
            pixels,
            width,
            height,
            text_x,
            text_y,
            line,
            max_chars,
            [0xE8, 0xF0, 0xEA, 0xFF],
        );
        text_y += line_h;
    }
}

fn fill_rect_argb(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    rect_w: usize,
    rect_h: usize,
    color: u32,
) {
    let right = x.saturating_add(rect_w).min(width);
    let bottom = y.saturating_add(rect_h).min(height);
    for row in y..bottom {
        let start = row * width + x.min(width);
        let end = row * width + right;
        pixels[start..end].fill(color);
    }
}

fn fill_rect_rgba(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    rect_w: usize,
    rect_h: usize,
    color: [u8; 4],
) {
    let right = x.saturating_add(rect_w).min(width);
    let bottom = y.saturating_add(rect_h).min(height);
    for row in y..bottom {
        for col in x.min(width)..right {
            let idx = (row * width + col) * 4;
            pixels[idx..idx + 4].copy_from_slice(&color);
        }
    }
}

fn stroke_rect_argb(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    rect_w: usize,
    rect_h: usize,
    color: u32,
) {
    if rect_w == 0 || rect_h == 0 || x >= width || y >= height {
        return;
    }
    let right = x.saturating_add(rect_w).min(width).saturating_sub(1);
    let bottom = y.saturating_add(rect_h).min(height).saturating_sub(1);
    for col in x..=right {
        pixels[y * width + col] = color;
        pixels[bottom * width + col] = color;
    }
    for row in y..=bottom {
        pixels[row * width + x] = color;
        pixels[row * width + right] = color;
    }
}

fn stroke_rect_rgba(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    rect_w: usize,
    rect_h: usize,
    color: [u8; 4],
) {
    if rect_w == 0 || rect_h == 0 || x >= width || y >= height {
        return;
    }
    let right = x.saturating_add(rect_w).min(width).saturating_sub(1);
    let bottom = y.saturating_add(rect_h).min(height).saturating_sub(1);
    for col in x..=right {
        let top_idx = (y * width + col) * 4;
        let bottom_idx = (bottom * width + col) * 4;
        pixels[top_idx..top_idx + 4].copy_from_slice(&color);
        pixels[bottom_idx..bottom_idx + 4].copy_from_slice(&color);
    }
    for row in y..=bottom {
        let left_idx = (row * width + x) * 4;
        let right_idx = (row * width + right) * 4;
        pixels[left_idx..left_idx + 4].copy_from_slice(&color);
        pixels[right_idx..right_idx + 4].copy_from_slice(&color);
    }
}

fn draw_debug_text_argb(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    text: &str,
    max_chars: usize,
    color: u32,
) {
    let mut cursor_x = x;
    for ch in text.chars().take(max_chars) {
        draw_debug_char_argb(pixels, width, height, cursor_x, y, ch, color);
        cursor_x += 6;
    }
}

fn draw_debug_text_rgba(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    text: &str,
    max_chars: usize,
    color: [u8; 4],
) {
    let mut cursor_x = x;
    for ch in text.chars().take(max_chars) {
        draw_debug_char_rgba(pixels, width, height, cursor_x, y, ch, color);
        cursor_x += 6;
    }
}

fn draw_debug_char_argb(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    ch: char,
    color: u32,
) {
    let glyph = debug_glyph(ch.to_ascii_uppercase());
    for (row, bits) in glyph.iter().enumerate() {
        let gy = y + row;
        if gy >= height {
            break;
        }
        for col in 0..5 {
            let gx = x + col;
            if gx >= width {
                continue;
            }
            if ((bits >> (4 - col)) & 1) != 0 {
                pixels[gy * width + gx] = color;
            }
        }
    }
}

fn draw_debug_char_rgba(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    ch: char,
    color: [u8; 4],
) {
    let glyph = debug_glyph(ch.to_ascii_uppercase());
    for (row, bits) in glyph.iter().enumerate() {
        let gy = y + row;
        if gy >= height {
            break;
        }
        for col in 0..5 {
            let gx = x + col;
            if gx >= width {
                continue;
            }
            if ((bits >> (4 - col)) & 1) != 0 {
                let idx = (gy * width + gx) * 4;
                pixels[idx..idx + 4].copy_from_slice(&color);
            }
        }
    }
}

fn debug_glyph(ch: char) -> [u8; 7] {
    match ch {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        ' ' => [0, 0, 0, 0, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        ',' => [0, 0, 0, 0, 0b01100, 0b01100, 0b01000],
        ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '/' => [
            0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000,
        ],
        '[' => [
            0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110,
        ],
        ']' => [
            0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110,
        ],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
        _ => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
    }
}

/// Convert a 16-bit Mac CLUT entry to 0xAARRGGBB.
pub fn clut_to_argb(clut: &[[u16; 3]; 256], index: u8) -> u32 {
    let [r8, g8, b8] = clut_to_rgba8(clut, index);
    0xFF000000 | (u32::from(r8) << 16) | (u32::from(g8) << 8) | u32::from(b8)
}

fn clut_to_rgba8(clut: &[[u16; 3]; 256], index: u8) -> [u8; 3] {
    let [r, g, b] = clut[index as usize];
    [
        clut_component_to_u8(r),
        clut_component_to_u8(g),
        clut_component_to_u8(b),
    ]
}

#[inline]
fn rgba_word(r: u8, g: u8, b: u8) -> u32 {
    u32::from_le_bytes([r, g, b, 0xFF])
}

fn clut_component_to_u8(component: u16) -> u8 {
    MAC_ROM_GAMMA_LUT[(component >> 8) as usize]
}

/// Mac ROM gamma LUT applied after `>> 8` truncation of 16-bit CLUT entries.
/// Matches BasiliskII's video gamma path (the `SetEntries` handler's
/// `have_gamma` branch).
/// Values empirically derived from paired-pixel observations
/// on 01_ambrosia_splash — every Systemless palette top-byte mapped 100% of the
/// time to the Basilisk-rendered value below. 16 observed pairs linearly
/// interpolated to 256 entries.
///
/// The authoritative "Mac HiRes Std Gamma" LUT from BasiliskII's
/// `slot_rom.cpp` `defaultGamma` resource was tested as a swap-in. Result
/// was a small regression — EV's effective gamma does not match the
/// standard HiRes gamma on non-grid inputs. Kept the linear-interpolated
/// LUT.
const MAC_ROM_GAMMA_LUT: [u8; 256] = [
    0x00, 0x02, 0x05, 0x07, 0x09, 0x0B, 0x0E, 0x10, 0x12, 0x15, 0x17, 0x19, 0x1C, 0x1E, 0x20, 0x22,
    0x25, 0x27, 0x28, 0x2A, 0x2B, 0x2D, 0x2E, 0x2F, 0x31, 0x32, 0x34, 0x35, 0x37, 0x38, 0x39, 0x3B,
    0x3C, 0x3E, 0x3F, 0x40, 0x41, 0x43, 0x44, 0x45, 0x46, 0x48, 0x49, 0x4A, 0x4B, 0x4D, 0x4E, 0x4F,
    0x50, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5E, 0x5F, 0x60, 0x61,
    0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x70, 0x71,
    0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F, 0x7F, 0x80,
    0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8B, 0x8C, 0x8D, 0x8E, 0x8F,
    0x90, 0x91, 0x92, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0x9A, 0x9B, 0x9C, 0x9D,
    0x9E, 0x9F, 0xA0, 0xA1, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB,
    0xAC, 0xAC, 0xAD, 0xAE, 0xAF, 0xB0, 0xB1, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB5, 0xB6, 0xB7, 0xB8,
    0xB8, 0xB9, 0xBA, 0xBB, 0xBB, 0xBC, 0xBD, 0xBE, 0xBE, 0xBF, 0xC0, 0xC1, 0xC2, 0xC2, 0xC3, 0xC4,
    0xC5, 0xC5, 0xC6, 0xC7, 0xC8, 0xC8, 0xC9, 0xCA, 0xCB, 0xCB, 0xCC, 0xCD, 0xCE, 0xCF, 0xCF, 0xD0,
    0xD1, 0xD2, 0xD2, 0xD3, 0xD4, 0xD5, 0xD5, 0xD6, 0xD7, 0xD8, 0xD8, 0xD9, 0xDA, 0xDB, 0xDC, 0xDC,
    0xDD, 0xDE, 0xDF, 0xDF, 0xE0, 0xE1, 0xE2, 0xE2, 0xE3, 0xE4, 0xE5, 0xE5, 0xE6, 0xE7, 0xE8, 0xE8,
    0xE9, 0xEA, 0xEB, 0xEB, 0xEC, 0xED, 0xED, 0xEE, 0xEF, 0xEF, 0xF0, 0xF1, 0xF2, 0xF2, 0xF3, 0xF4,
    0xF4, 0xF5, 0xF6, 0xF7, 0xF7, 0xF8, 0xF9, 0xF9, 0xFA, 0xFB, 0xFB, 0xFC, 0xFD, 0xFE, 0xFE, 0xFF,
];

#[cfg(test)]
mod tests {
    use super::{
        clut_component_to_u8, clut_to_argb, normalize_centered_compact_mac_viewport_margins_rgba,
        render_cursor, render_cursor_argb, render_screen_into,
        render_screen_with_rgba_palette_into, rgba_palette_from_clut, screen_pixel_rgb,
        CursorImage,
    };
    use crate::memory::{MacMemoryBus, MemoryBus};

    #[test]
    fn clut_component_applies_mac_rom_gamma() {
        // Endpoints fixed to raw values.
        assert_eq!(clut_component_to_u8(0x0000), 0x00);
        assert_eq!(clut_component_to_u8(0xFFFF), 0xFF);
        // Mac palette grid values map to the 16 pairs observed from
        // `01_ambrosia_splash` (empirical derivation).
        assert_eq!(clut_component_to_u8(0x4444), 0x66);
        assert_eq!(clut_component_to_u8(0x6666), 0x87);
        assert_eq!(clut_component_to_u8(0xAAAA), 0xC0);
    }

    #[test]
    fn clut_to_argb_applies_gamma() {
        let mut clut = [[0u16; 3]; 256];
        // Use palette-grid values so the LUT mapping is exact.
        clut[7] = [0x4444, 0x8888, 0xCCCC];
        let argb = clut_to_argb(&clut, 7);
        assert_eq!(argb, 0xFF66A5DA);
    }

    #[test]
    fn render_screen_into_8bpp_writes_rgba_bytes() {
        let mut bus = MacMemoryBus::new(1024);
        let base = 128;
        bus.write_byte(base, 0);
        bus.write_byte(base + 1, 1);
        bus.write_byte(base + 2, 2);
        let mut clut = [[0u16; 3]; 256];
        clut[0] = [0x0000, 0x0000, 0x0000];
        clut[1] = [0xFFFF, 0x0000, 0x0000];
        clut[2] = [0x0000, 0xFFFF, 0x0000];

        let mut pixels = Vec::new();
        render_screen_into(&bus, (base, 4, 3, 1, 8), &clut, &mut pixels);

        assert_eq!(
            pixels,
            vec![
                0x00, 0x00, 0x00, 0xFF, //
                0xFF, 0x00, 0x00, 0xFF, //
                0x00, 0xFF, 0x00, 0xFF,
            ]
        );
    }

    #[test]
    fn render_screen_with_precomputed_palette_matches_clut_path() {
        let mut bus = MacMemoryBus::new(1024);
        let base = 128;
        bus.write_byte(base, 0);
        bus.write_byte(base + 1, 1);
        bus.write_byte(base + 2, 2);
        let mut clut = [[0u16; 3]; 256];
        clut[0] = [0x0000, 0x0000, 0x0000];
        clut[1] = [0xFFFF, 0x0000, 0x0000];
        clut[2] = [0x0000, 0xFFFF, 0x0000];

        let mut direct = Vec::new();
        render_screen_into(&bus, (base, 4, 3, 1, 8), &clut, &mut direct);

        let palette = rgba_palette_from_clut(&clut);
        let mut precomputed = Vec::new();
        render_screen_with_rgba_palette_into(&bus, (base, 4, 3, 1, 8), &palette, &mut precomputed);

        assert_eq!(precomputed, direct);
    }

    #[test]
    fn compact_viewport_normalizer_blacks_uniform_outer_flood() {
        let width = 800usize;
        let height = 600usize;
        let mut pixels = vec![0u8; width * height * 4];
        for px in pixels.chunks_exact_mut(4) {
            px.copy_from_slice(&[255, 106, 37, 0xFF]);
        }
        let left = 143usize;
        let top = 129usize;
        let right = 657usize;
        let bottom = 471usize;
        for y in top + 9..bottom {
            for x in left..right {
                let idx = (y * width + x) * 4;
                pixels[idx..idx + 4].copy_from_slice(&[
                    (x & 0xFF) as u8,
                    245,
                    (y & 0xFF) as u8,
                    0xFF,
                ]);
            }
        }

        assert!(normalize_centered_compact_mac_viewport_margins_rgba(
            &mut pixels,
            width,
            height
        ));

        assert_eq!(&pixels[0..4], &[0, 0, 0, 0xFF]);
        let trimmed_top_idx = ((top + 1) * width + 400) * 4;
        assert_eq!(
            &pixels[trimmed_top_idx..trimmed_top_idx + 4],
            &[0, 0, 0, 0xFF]
        );
        let inside_idx = (300 * width + 400) * 4;
        assert_eq!(
            &pixels[inside_idx..inside_idx + 4],
            &[(400 & 0xFF) as u8, 245, (300 & 0xFF) as u8, 0xFF]
        );
        let lower_margin_idx = (500 * width + 400) * 4;
        assert_eq!(
            &pixels[lower_margin_idx..lower_margin_idx + 4],
            &[0, 0, 0, 0xFF]
        );
    }

    #[test]
    fn compact_viewport_normalizer_preserves_detailed_outer_pixels() {
        let width = 800usize;
        let height = 600usize;
        let mut pixels = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                pixels[idx..idx + 4].copy_from_slice(&[
                    (x & 0xFF) as u8,
                    (y & 0xFF) as u8,
                    ((x + y) & 0xFF) as u8,
                    0xFF,
                ]);
            }
        }
        let before = pixels.clone();

        assert!(!normalize_centered_compact_mac_viewport_margins_rgba(
            &mut pixels,
            width,
            height
        ));
        assert_eq!(pixels, before);
    }

    #[test]
    fn screen_pixel_rgb_samples_8bpp_with_gamma() {
        let mut bus = MacMemoryBus::new(1024);
        let base = 128;
        bus.write_byte(base, 0);
        bus.write_byte(base + 1, 7);
        let mut clut = [[0u16; 3]; 256];
        clut[7] = [0x4444, 0x8888, 0xCCCC];

        assert_eq!(
            screen_pixel_rgb(&bus, (base, 4, 2, 1, 8), &clut, 1, 0),
            Some([0x66, 0xA5, 0xDA])
        );
        assert_eq!(
            screen_pixel_rgb(&bus, (base, 4, 2, 1, 8), &clut, 2, 0),
            None
        );
    }

    #[test]
    fn render_screen_into_1bpp_writes_rgba_bytes() {
        let mut bus = MacMemoryBus::new(1024);
        let base = 128;
        bus.write_byte(base, 0b1010_0000);
        let clut = [[0u16; 3]; 256];

        let mut pixels = Vec::new();
        render_screen_into(&bus, (base, 1, 4, 1, 1), &clut, &mut pixels);

        assert_eq!(
            pixels,
            vec![
                0x00, 0x00, 0x00, 0xFF, //
                0xFF, 0xFF, 0xFF, 0xFF, //
                0x00, 0x00, 0x00, 0xFF, //
                0xFF, 0xFF, 0xFF, 0xFF,
            ]
        );
    }

    #[test]
    fn render_color_cursor_replaces_masked_pixels_and_inverts_black_outside_mask() {
        let mut mask = [0u8; 32];
        mask[0] = 0x80;
        let cursor = CursorImage::Color {
            width: 2,
            height: 2,
            pixels_argb: vec![0xFFFF_0000, 0xFF00_0000, 0xFFFF_FFFF, 0xFF00_FF00],
            mask,
            hot_v: 0,
            hot_h: 0,
            mono_data: [0; 32],
            mono_mask: mask,
        };

        let mut rgba = vec![
            0x40, 0x50, 0x60, 0xFF, 0x40, 0x50, 0x60, 0xFF, 0x40, 0x50, 0x60, 0xFF, 0x40, 0x50,
            0x60, 0xFF,
        ];
        render_cursor(&mut rgba, 2, 2, &cursor, (0, 0));
        assert_eq!(&rgba[0..4], &[0xFF, 0x00, 0x00, 0xFF]);
        assert_eq!(&rgba[4..8], &[0xBF, 0xAF, 0x9F, 0xFF]);
        assert_eq!(&rgba[8..12], &[0x40, 0x50, 0x60, 0xFF]);
        assert_eq!(&rgba[12..16], &[0x40, 0x50, 0x60, 0xFF]);

        let mut argb = vec![0xFF40_5060; 4];
        render_cursor_argb(&mut argb, 2, 2, &cursor, (0, 0));
        assert_eq!(argb[0], 0xFFFF_0000);
        assert_eq!(argb[1], 0xFFBF_AF9F);
        assert_eq!(argb[2], 0xFF40_5060);
        assert_eq!(argb[3], 0xFF40_5060);
    }

    #[test]
    fn screen_pixel_rgb_samples_1bpp() {
        let mut bus = MacMemoryBus::new(1024);
        let base = 128;
        bus.write_byte(base, 0b1010_0000);
        let clut = [[0u16; 3]; 256];

        assert_eq!(
            screen_pixel_rgb(&bus, (base, 1, 4, 1, 1), &clut, 0, 0),
            Some([0, 0, 0])
        );
        assert_eq!(
            screen_pixel_rgb(&bus, (base, 1, 4, 1, 1), &clut, 1, 0),
            Some([255, 255, 255])
        );
        assert_eq!(
            screen_pixel_rgb(&bus, (base, 1, 4, 1, 1), &clut, 4, 0),
            None
        );
    }
}
