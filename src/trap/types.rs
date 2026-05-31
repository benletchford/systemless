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
    let name = String::from_utf8_lossy(&bytes[..n]).into_owned();
    // Strip "Unix:" volume prefix if present (MPW convention)
    name.strip_prefix("Unix:").unwrap_or(&name).to_string()
}
