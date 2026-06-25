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

const MAC_ROMAN_HIGH: [char; 128] = [
    '\u{00C4}', '\u{00C5}', '\u{00C7}', '\u{00C9}', '\u{00D1}', '\u{00D6}', '\u{00DC}', '\u{00E1}',
    '\u{00E0}', '\u{00E2}', '\u{00E4}', '\u{00E3}', '\u{00E5}', '\u{00E7}', '\u{00E9}', '\u{00E8}',
    '\u{00EA}', '\u{00EB}', '\u{00ED}', '\u{00EC}', '\u{00EE}', '\u{00EF}', '\u{00F1}', '\u{00F3}',
    '\u{00F2}', '\u{00F4}', '\u{00F6}', '\u{00F5}', '\u{00FA}', '\u{00F9}', '\u{00FB}', '\u{00FC}',
    '\u{2020}', '\u{00B0}', '\u{00A2}', '\u{00A3}', '\u{00A7}', '\u{2022}', '\u{00B6}', '\u{00DF}',
    '\u{00AE}', '\u{00A9}', '\u{2122}', '\u{00B4}', '\u{00A8}', '\u{2260}', '\u{00C6}', '\u{00D8}',
    '\u{221E}', '\u{00B1}', '\u{2264}', '\u{2265}', '\u{00A5}', '\u{00B5}', '\u{2202}', '\u{2211}',
    '\u{220F}', '\u{03C0}', '\u{222B}', '\u{00AA}', '\u{00BA}', '\u{03A9}', '\u{00E6}', '\u{00F8}',
    '\u{00BF}', '\u{00A1}', '\u{00AC}', '\u{221A}', '\u{0192}', '\u{2248}', '\u{2206}', '\u{00AB}',
    '\u{00BB}', '\u{2026}', '\u{00A0}', '\u{00C0}', '\u{00C3}', '\u{00D5}', '\u{0152}', '\u{0153}',
    '\u{2013}', '\u{2014}', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}', '\u{00F7}', '\u{25CA}',
    '\u{00FF}', '\u{0178}', '\u{2044}', '\u{20AC}', '\u{2039}', '\u{203A}', '\u{FB01}', '\u{FB02}',
    '\u{2021}', '\u{00B7}', '\u{201A}', '\u{201E}', '\u{2030}', '\u{00C2}', '\u{00CA}', '\u{00C1}',
    '\u{00CB}', '\u{00C8}', '\u{00CD}', '\u{00CE}', '\u{00CF}', '\u{00CC}', '\u{00D3}', '\u{00D4}',
    '\u{F8FF}', '\u{00D2}', '\u{00DA}', '\u{00DB}', '\u{00D9}', '\u{0131}', '\u{02C6}', '\u{02DC}',
    '\u{00AF}', '\u{02D8}', '\u{02D9}', '\u{02DA}', '\u{00B8}', '\u{02DD}', '\u{02DB}', '\u{02C7}',
];

pub(crate) fn decode_mac_roman(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| {
            if byte < 0x80 {
                byte as char
            } else {
                MAC_ROMAN_HIGH[(byte - 0x80) as usize]
            }
        })
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
            0xC9 => out.push_str("..."),
            0xCA => out.push(' '),
            0xD0 | 0xD1 => out.push('-'),
            0xD2 | 0xD3 => out.push('"'),
            0xD4 | 0xD5 => out.push('\''),
            _ => out.push(MAC_ROMAN_HIGH[(byte - 0x80) as usize]),
        }
    }
    out
}

pub(crate) fn encode_mac_roman_lossy(value: &str) -> Vec<u8> {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii() {
                ch as u8
            } else {
                MAC_ROMAN_HIGH
                    .iter()
                    .position(|&candidate| candidate == ch)
                    .map(|idx| idx as u8 + 0x80)
                    .unwrap_or(b'?')
            }
        })
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
    fn mac_roman_render_decode_expands_classic_punctuation() {
        assert_eq!(
            decode_mac_roman_for_render(b"Choose Monitor\xC9"),
            "Choose Monitor..."
        );
        assert_eq!(decode_mac_roman_for_render(b"Marathon\xD5s"), "Marathon's");
    }
}
