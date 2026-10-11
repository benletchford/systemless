//! Row kernels: expand pattern rows into destination pixels.
//!
//! patCopy does not depend on the old pixel, so its rows are prepared once
//! and written without reading the destination.

use super::paint::Ink;

#[cfg(test)]
mod tests;

/// The patCopy rows of one rectangle draw, `width` pixels starting at port
/// x `left`. Above 1 bit per pixel, patCopy gives black pattern bits the
/// foreground pixel and white bits the background pixel. Each distinct
/// pattern row is expanded once, the first time a scanline needs it, into
/// caller-owned scratch.
pub(crate) struct PatCopyRows<'s> {
    scratch: &'s mut Vec<u8>,
    /// Pattern rows, rotated so that bit 7 is the pixel at `left`.
    aligned: [u8; 8],
    /// Scratch slot of each pattern row; equal rows share a slot.
    slot: [u8; 8],
    /// Bit `n` set once slot `n` holds its expanded row.
    built: u8,
    row_len: usize,
    fg: u32,
    bg: u32,
    bytes_per_pixel: usize,
}

impl<'s> PatCopyRows<'s> {
    /// Rows of `ink`'s pattern, aligned to `ink.pat_origin`; the caller
    /// picks the pattern row of each scanline.
    pub(crate) fn new(
        scratch: &'s mut Vec<u8>,
        ink: &Ink,
        left: i32,
        width: usize,
        bytes_per_pixel: usize,
    ) -> Self {
        let pattern = ink.pattern;
        let rotate = (left - i32::from(ink.pat_origin.0)).rem_euclid(8) as u32;
        let mut aligned = [0u8; 8];
        let mut slot = [0u8; 8];
        let mut slots = 0u8;
        for row in 0..8 {
            aligned[row] = pattern[row].rotate_left(rotate);
            slot[row] = match (0..row).find(|&earlier| aligned[earlier] == aligned[row]) {
                Some(earlier) => slot[earlier],
                None => {
                    slots += 1;
                    slots - 1
                }
            };
        }
        let row_len = width * bytes_per_pixel;
        scratch.clear();
        scratch.resize(row_len * usize::from(slots), 0);
        Self {
            scratch,
            aligned,
            slot,
            built: 0,
            row_len,
            fg: ink.fg,
            bg: ink.bg,
            bytes_per_pixel,
        }
    }

    /// The prepared bytes for pattern row `row` (0..8).
    #[inline]
    pub(crate) fn row(&mut self, row: usize) -> &[u8] {
        let slot = usize::from(self.slot[row]);
        let start = slot * self.row_len;
        let out = &mut self.scratch[start..start + self.row_len];
        if self.built & (1 << slot) == 0 {
            expand_pat_copy_row(
                self.aligned[row],
                self.fg,
                self.bg,
                self.bytes_per_pixel,
                out,
            );
            self.built |= 1 << slot;
        }
        out
    }
}

/// Fills `out` with pixels chosen by `bits`, repeating every 8 pixels: a set
/// bit (MSB first) gives `fg`, a clear bit `bg`. Pixels are stored big-endian
/// in `bytes_per_pixel` bytes (1 or 2).
fn expand_pat_copy_row(bits: u8, fg: u32, bg: u32, bytes_per_pixel: usize, out: &mut [u8]) {
    let mut tile = [0u8; 16];
    for (pixel, chunk) in tile.chunks_exact_mut(bytes_per_pixel).take(8).enumerate() {
        let value = if bits & (0x80 >> pixel) != 0 { fg } else { bg };
        chunk.copy_from_slice(&value.to_be_bytes()[4 - bytes_per_pixel..]);
    }
    let tile = &tile[..8 * bytes_per_pixel];
    for chunk in out.chunks_mut(tile.len()) {
        chunk.copy_from_slice(&tile[..chunk.len()]);
    }
}
