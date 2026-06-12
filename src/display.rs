//! Shared framebuffer rendering for all Systemless frontends.
//!
//! Produces RGBA `Vec<u8>` pixel buffers from emulator screen memory,
//! supporting both 1bpp monochrome and 8bpp color modes.

use crate::memory::{MacMemoryBus, MemoryBus};

const BLACK_ARGB: u32 = 0xFF000000;
const WHITE_ARGB: u32 = 0xFFFFFFFF;
const BLACK_RGBA_WORD: u32 = u32::from_le_bytes([0x00, 0x00, 0x00, 0xFF]);
const WHITE_RGBA_WORD: u32 = u32::from_le_bytes([0xFF, 0xFF, 0xFF, 0xFF]);

pub type RgbaPalette = [u32; 256];
const UNUSED_RGBA_PALETTE: RgbaPalette = [0; 256];

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

    for gy in 0..h {
        let row_start = (gy * row_bytes) as usize;
        let px_row = (gy * w * 4) as usize;
        if is_8bpp {
            for gx in 0..w {
                let pixel = fb[row_start + gx as usize];
                let idx = px_row + (gx * 4) as usize;
                write_rgba_word(pixels, idx, palette[pixel as usize]);
            }
        } else {
            for gx in 0..w {
                let byte = fb[row_start + (gx / 8) as usize];
                let bit = 7 - (gx % 8);
                let idx = px_row + (gx * 4) as usize;
                let rgba = if (byte & (1 << bit)) != 0 {
                    BLACK_RGBA_WORD
                } else {
                    WHITE_RGBA_WORD
                };
                write_rgba_word(pixels, idx, rgba);
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
    cursor: &([u8; 32], [u8; 32], i16, i16),
    mouse_pos: (i16, i16),
) {
    let (data, mask, hot_v, hot_h) = cursor;
    let (mouse_v, mouse_h) = mouse_pos;
    let cx = mouse_h as i32 - *hot_h as i32;
    let cy = mouse_v as i32 - *hot_v as i32;

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
    cursor: &([u8; 32], [u8; 32], i16, i16),
    mouse_pos: (i16, i16),
) {
    let (data, mask, hot_v, hot_h) = cursor;
    let (mouse_v, mouse_h) = mouse_pos;
    let cx = mouse_h as i32 - *hot_h as i32;
    let cy = mouse_v as i32 - *hot_v as i32;

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

#[inline]
fn write_rgba_word(pixels: &mut [u8], idx: usize, rgba: u32) {
    debug_assert!(idx + 4 <= pixels.len());
    unsafe {
        std::ptr::write_unaligned(pixels.as_mut_ptr().add(idx).cast::<u32>(), rgba);
    }
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
        clut_component_to_u8, clut_to_argb, render_screen_into,
        render_screen_with_rgba_palette_into, rgba_palette_from_clut, screen_pixel_rgb,
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
