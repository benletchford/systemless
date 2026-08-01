//! Shared types used across trap handler modules.

use crate::memory::MacMemoryBus;
use crate::memory::MemoryBus;
use std::collections::HashSet;

/// Rectangle in Mac coordinate space (top, left, bottom, right).
/// Bottom and right are exclusive (standard Mac Rect convention).
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}

/// Shape drawing operation mode
#[derive(Clone, Copy, Debug)]
pub enum ShapeOp {
    Frame,
    Paint,
    Erase,
    Invert,
    Fill([u8; 8]),
    Glyph(i16), // Text Mode
}

/// Info for continuous underline drawing across a string
pub struct UnderlineInfo {
    /// Start x position of the underline
    pub start_x: i16,
    /// End x position of the underline (exclusive)
    pub end_x: i16,
    /// X positions where underline should break for descenders (per underline row)
    pub breaks: Vec<HashSet<i16>>,
}

/// Temporary buffer for string rendering (EXPERIMENTAL - NOT USED)
/// This implements QuickDraw's combined-buffer approach of rendering all glyphs
/// to a buffer first, then applying outline/shadow to the combined result.
#[allow(dead_code)]
pub struct StringBuffer {
    pub width: i16,
    pub height: i16,
    pub baseline_y: i16,
    pub pixels: Vec<u8>,
}

pub(crate) fn decode_mac_roman(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| crate::mac_roman::decode_byte(byte))
        .collect()
}

pub(crate) fn decode_mac_roman_for_render(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            0x00..=0x7F => out.push(byte as char),
            // Classic dialog/control strings use Mac Roman. The HLE chrome
            // renderer only has ASCII glyphs plus a few symbol slots today, so
            // expand common punctuation into renderable equivalents instead of
            // letting it become replacement or blank glyphs. IM:I I-247.
            0xA5 => out.push('*'),
            // Keep the horizontal ellipsis as one character. TextEdit line
            // breaking operates on guest-encoded bytes; expanding this to
            // three ASCII periods would give it the wrong character count
            // and advance. The glyph layer maps U+2026 back to Mac Roman C9.
            0xC9 => out.push('\u{2026}'),
            0xCA => out.push(' '),
            0xD0 | 0xD1 => out.push('-'),
            0xD2 | 0xD3 => out.push('"'),
            0xD4 | 0xD5 => out.push('\''),
            _ => out.push(crate::mac_roman::decode_byte(byte)),
        }
    }
    out
}

pub(crate) fn encode_mac_roman_lossy(value: &str) -> Vec<u8> {
    value
        .chars()
        .map(|ch| crate::mac_roman::encode_char(ch).unwrap_or(b'?'))
        .collect()
}

#[allow(dead_code)]
impl StringBuffer {
    pub fn new(width: i16, height: i16, baseline_y: i16) -> Self {
        let w = width.max(1) as usize;
        let h = height.max(1) as usize;
        Self {
            width,
            height,
            baseline_y,
            pixels: vec![0; w * h],
        }
    }

    #[allow(dead_code)]
    pub fn get(&self, x: i16, y: i16) -> bool {
        if x < 0 || x >= self.width || y < 0 || y >= self.height {
            return false;
        }
        self.pixels[y as usize * self.width as usize + x as usize] != 0
    }

    #[allow(dead_code)]
    pub fn set(&mut self, x: i16, y: i16, val: bool) {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.pixels[y as usize * self.width as usize + x as usize] = if val { 1 } else { 0 };
        }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    #[allow(dead_code)]
    pub fn any_neighbor_set(&self, x: i16, y: i16) -> bool {
        for dy in -1..=1i16 {
            for dx in -1..=1i16 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if self.get(x + dx, y + dy) {
                    return true;
                }
            }
        }
        false
    }
}

/// Read a Rect from guest memory at the given address.
pub fn read_rect(bus: &MacMemoryBus, ptr: u32) -> Rect {
    Rect {
        top: bus.read_word(ptr) as i16,
        left: bus.read_word(ptr + 2) as i16,
        bottom: bus.read_word(ptr + 4) as i16,
        right: bus.read_word(ptr + 6) as i16,
    }
}

/// Read the filename from an FSSpec structure in guest memory.
/// FSSpec layout: vRefNum (2), dirID (4), name (Str63: length byte + up to 63 chars).
/// The name starts at offset 6 (length) and offset 7 (characters).
/// Strips any leading "Unix:" volume prefix used by MPW tools.
pub fn read_fsspec_name(bus: &MacMemoryBus, spec_ptr: u32) -> String {
    // FSSpec stores at most a 63-byte HFS filename; clamp here in case
    // the length byte is stale or out-of-spec.
    let bytes = bus.read_pstring(spec_ptr + 6);
    let n = bytes.len().min(63);
    let name = decode_mac_roman(&bytes[..n]);
    // Strip "Unix:" volume prefix if present (MPW convention)
    name.strip_prefix("Unix:").unwrap_or(&name).to_string()
}

#[cfg(test)]
mod tests {
    use super::{decode_mac_roman, decode_mac_roman_for_render, encode_mac_roman_lossy};

    #[test]
    fn mac_roman_round_trips_classic_filename_symbols() {
        let bytes = b"MORE\xAA Library";
        let decoded = decode_mac_roman(bytes);
        assert_eq!(decoded, "MORE\u{2122} Library");
        assert_eq!(encode_mac_roman_lossy(&decoded), bytes);
    }

    #[test]
    fn mac_roman_render_decode_preserves_classic_ellipsis_as_one_character() {
        assert_eq!(
            decode_mac_roman_for_render(b"Choose Monitor\xC9"),
            "Choose Monitor\u{2026}"
        );
        assert_eq!(
            encode_mac_roman_lossy(&decode_mac_roman_for_render(b"Choose Monitor\xC9")),
            b"Choose Monitor\xC9"
        );
        assert_eq!(decode_mac_roman_for_render(b"Marathon\xD5s"), "Marathon's");
    }
}
