//! patCopy rows against a per-pixel reading of the pattern.

use super::PatCopyRows;
use crate::quickdraw::raster::paint::{Ink, TransferMode};

/// The pixel patCopy gives port point `(x, y)`: the pattern bit at the
/// point's offset from the pattern origin, MSB first.
fn reference_pixel(ink: &Ink, x: i32, y: i32) -> u32 {
    let row = ink.pattern[(y - i32::from(ink.pat_origin.1)).rem_euclid(8) as usize];
    let bit = (x - i32::from(ink.pat_origin.0)).rem_euclid(8);
    if row & (0x80 >> bit) != 0 {
        ink.fg
    } else {
        ink.bg
    }
}

fn ink(pattern: [u8; 8], pat_origin: (i16, i16)) -> Ink {
    Ink {
        fg: 0xA1B2,
        bg: 0x0C3D,
        pattern,
        mode: TransferMode::PatCopy,
        pat_origin,
    }
}

const PATTERNS: [[u8; 8]; 6] = [
    [0xFF; 8],
    [0x00; 8],
    [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
    [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01],
    [0x11, 0x22, 0x44, 0x88, 0x0F, 0xF0, 0x3C, 0xC3],
    // A repeated row whose first occurrence is not at the index of its
    // prepared slot (rows 2 and 3 share slot 1).
    [0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF],
];

#[test]
fn rows_match_the_pattern_bit_of_every_pixel() {
    for pattern in PATTERNS {
        for pat_origin in [(0, 0), (3, -5), (-9, 13)] {
            let ink = ink(pattern, pat_origin);
            for bytes_per_pixel in [1usize, 2] {
                for left in [-17, -8, 0, 1, 5, 8, 23] {
                    for width in [1usize, 3, 8, 9, 16, 37] {
                        let mut scratch = Vec::new();
                        let mut rows =
                            PatCopyRows::new(&mut scratch, &ink, left, width, bytes_per_pixel);
                        // Scanlines past one pattern period reuse its rows.
                        for y in -9..11 {
                            let row = (y - i32::from(pat_origin.1)).rem_euclid(8) as usize;
                            let prepared = rows.row(row).to_vec();
                            assert_eq!(prepared.len(), width * bytes_per_pixel);
                            for (i, pixel) in prepared.chunks_exact(bytes_per_pixel).enumerate() {
                                let x = left + i as i32;
                                let expected = reference_pixel(&ink, x, y).to_be_bytes();
                                assert_eq!(
                                    pixel,
                                    &expected[4 - bytes_per_pixel..],
                                    "pattern {pattern:02x?} origin {pat_origin:?} \
                                     bpp {bytes_per_pixel} left {left} x {x} y {y}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn equal_pattern_rows_share_one_prepared_row() {
    let solid = ink([0xFF; 8], (0, 0));
    let mut scratch = Vec::new();
    PatCopyRows::new(&mut scratch, &solid, 0, 10, 2);
    assert_eq!(scratch.len(), 10 * 2);

    let gray = ink([0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55], (0, 0));
    PatCopyRows::new(&mut scratch, &gray, 0, 10, 1);
    assert_eq!(scratch.len(), 10 * 2);

    let mixed = ink(PATTERNS[4], (0, 0));
    PatCopyRows::new(&mut scratch, &mixed, 0, 10, 1);
    assert_eq!(scratch.len(), 10 * 8);
}
