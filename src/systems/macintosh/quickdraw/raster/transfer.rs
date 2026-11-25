//! Row kernels: expand pattern rows into destination pixels and apply
//! boolean transfer modes and whole-row 1-bpp operations.
//!
//! `patCopy` and `notPatCopy` do not depend on the old pixel, so their rows
//! are prepared once and written without reading the destination.

use super::memory::RasterMemory;
use super::paint::{Ink, TransferMode};

#[cfg(test)]
mod tests;

/// What a whole-row 1-bpp operation does to every bit of a covered run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BitRowOp {
    Set,
    Clear,
    Toggle,
}

/// Apply `op` to bits `first_bit..end_bit` (MSB-first, as QuickDraw packs
/// them) of the 1-bpp row at `row_base`: masked read-modify-write of the two
/// edge bytes, one bulk write or read-modify-write for the whole bytes
/// between.
///
/// For `Set` / `Clear`, `middle_scratch` must be pre-filled with `0xFF` /
/// `0x00` and have length at least the interior whole-byte span. For `Toggle`,
/// `middle_scratch` is reused in place as the read-modify-write buffer.
pub(crate) fn apply_bit_row_op<M: RasterMemory>(
    memory: &mut M,
    row_base: u32,
    first_bit: u32,
    end_bit: u32,
    op: BitRowOp,
    middle_scratch: &mut [u8],
) -> bool {
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
        let addr = row_base.wrapping_add(first_byte);
        return memory
            .modify_byte(addr, |byte| apply(byte, head_mask & tail_mask))
            .is_some();
    }
    let head_addr = row_base.wrapping_add(first_byte);
    let mut wrote = memory
        .modify_byte(head_addr, |byte| apply(byte, head_mask))
        .is_some();
    let middle = (last_byte - first_byte - 1) as usize;
    if middle > 0 {
        let middle_addr = head_addr.wrapping_add(1);
        wrote |= match op {
            BitRowOp::Set | BitRowOp::Clear => {
                memory.write_row(middle_addr, &middle_scratch[..middle]).is_some()
            }
            BitRowOp::Toggle => memory
                .modify_row(middle_addr, &mut middle_scratch[..middle], |row| {
                    for byte in row.iter_mut() {
                        *byte = !*byte;
                    }
                })
                .is_some(),
        };
    }
    let tail_addr = row_base.wrapping_add(last_byte);
    wrote
        | memory
            .modify_byte(tail_addr, |byte| apply(byte, tail_mask))
            .is_some()
}

/// Applies one of the six read-modify-write 8-bpp boolean transfer modes
/// (`patOr`, `patXor`, `patBic`, and their `notPat*` counterparts) across
/// `dst`, using `aligned_bits` rotated so bit 7 corresponds to `dst[0]`.
#[inline]
pub(crate) fn apply_boolean_row_8(
    dst: &mut [u8],
    aligned_bits: u8,
    mode: TransferMode,
    fg: u8,
    bg: u8,
) {
    let bits = if mode.inverts_source() {
        !aligned_bits
    } else {
        aligned_bits
    };
    if bits == 0 {
        return;
    }
    match mode {
        TransferMode::PatOr | TransferMode::NotPatOr if bits == 0xFF => dst.fill(fg),
        TransferMode::PatBic | TransferMode::NotPatBic if bits == 0xFF => dst.fill(bg),
        TransferMode::PatXor | TransferMode::NotPatXor if bits == 0xFF => {
            for pixel in dst {
                *pixel = !*pixel;
            }
        }
        TransferMode::PatOr | TransferMode::NotPatOr => {
            for (i, pixel) in dst.iter_mut().enumerate() {
                if bits & (0x80 >> (i & 7)) != 0 {
                    *pixel = fg;
                }
            }
        }
        TransferMode::PatXor | TransferMode::NotPatXor => {
            for (i, pixel) in dst.iter_mut().enumerate() {
                if bits & (0x80 >> (i & 7)) != 0 {
                    *pixel = !*pixel;
                }
            }
        }
        TransferMode::PatBic | TransferMode::NotPatBic => {
            for (i, pixel) in dst.iter_mut().enumerate() {
                if bits & (0x80 >> (i & 7)) != 0 {
                    *pixel = bg;
                }
            }
        }
        TransferMode::PatCopy | TransferMode::NotPatCopy => {
            debug_assert!(false, "copy modes use PatCopyRows without reading destination");
        }
    }
}

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
        let pattern = if ink.mode.inverts_source() {
            ink.pattern.map(|row| !row)
        } else {
            ink.pattern
        };
        if bytes_per_pixel == 1 && (pattern == [0xFF; 8] || pattern == [0x00; 8]) {
            let fill = if pattern[0] == 0xFF {
                ink.fg as u8
            } else {
                ink.bg as u8
            };
            scratch.clear();
            scratch.resize(width, fill);
            return Self {
                scratch,
                aligned: [pattern[0]; 8],
                slot: [0; 8],
                built: 1,
                row_len: width,
                fg: ink.fg,
                bg: ink.bg,
                bytes_per_pixel,
            };
        }
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
    #[inline(always)]
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
