//! `paint_rect` against a per-pixel reference, with a memory that records
//! every row write.

use super::*;
use crate::quickdraw::raster::region_rows::endpoints_contain_point;

/// A pixel image in host memory that records each `write_row` call.
struct RecordingImage {
    base: u32,
    bytes: Vec<u8>,
    writes: Vec<(u32, usize)>,
}

impl RecordingImage {
    fn new(base: u32, len: usize) -> Self {
        Self {
            base,
            bytes: (0..len).map(|i| (i * 37 + 11) as u8).collect(),
            writes: Vec::new(),
        }
    }
}

impl RasterMemory for RecordingImage {
    fn write_row(&mut self, addr: u32, bytes: &[u8]) -> Option<()> {
        self.writes.push((addr, bytes.len()));
        let start = addr.checked_sub(self.base)? as usize;
        self.bytes
            .get_mut(start..start + bytes.len())?
            .copy_from_slice(bytes);
        Some(())
    }
}

fn rect(top: i16, left: i16, bottom: i16, right: i16) -> Rect {
    Rect {
        top,
        left,
        bottom,
        right,
    }
}

const GRAY: [u8; 8] = [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];
const MIXED: [u8; 8] = [0x11, 0x22, 0x44, 0x88, 0x0F, 0xF0, 0x3C, 0xC3];

fn ink(pattern: [u8; 8]) -> Ink {
    Ink {
        fg: 0x7C1F,
        bg: 0x03E0,
        pattern,
        mode: TransferMode::PatCopy,
        pat_origin: (0, 0),
    }
}

/// A 24×12 image whose top-left pixel is port point (-5, -3), so the
/// pattern phase differs from the pixel offset.
fn surface(depth: Depth) -> Surface {
    let pixel_len = match depth {
        Depth::D16 => 2,
        _ => 1,
    };
    Surface {
        base: 0x1000,
        row_bytes: 24 * pixel_len + 6,
        depth,
        bounds: rect(-3, -5, 9, 19),
    }
}

/// What `paint_rect` must leave in `before`: covered pixels take the pattern
/// pixel of their port point, all other bytes are unchanged.
fn reference(
    surface: &Surface,
    covered: impl Fn(i32, i32) -> bool,
    ink: &Ink,
    before: &[u8],
) -> Vec<u8> {
    let pixel_len = surface.depth.whole_byte_pixel_len().unwrap();
    let mut out = before.to_vec();
    let b = surface.bounds;
    for y in i32::from(b.top)..i32::from(b.bottom) {
        for x in i32::from(b.left)..i32::from(b.right) {
            if !covered(x, y) {
                continue;
            }
            let row = ink.pattern[(y - i32::from(ink.pat_origin.1)).rem_euclid(8) as usize];
            let bit = (x - i32::from(ink.pat_origin.0)).rem_euclid(8);
            let value = if row & (0x80 >> bit) != 0 {
                ink.fg
            } else {
                ink.bg
            };
            let offset = (y - i32::from(b.top)) as usize * surface.row_bytes as usize
                + (x - i32::from(b.left)) as usize * pixel_len;
            out[offset..offset + pixel_len].copy_from_slice(&value.to_be_bytes()[4 - pixel_len..]);
        }
    }
    out
}

fn image_for(surface: &Surface) -> RecordingImage {
    let height = (surface.bounds.bottom - surface.bounds.top) as usize;
    RecordingImage::new(surface.base, height * surface.row_bytes as usize)
}

fn inside(r: Rect, x: i32, y: i32) -> bool {
    y >= i32::from(r.top)
        && y < i32::from(r.bottom)
        && x >= i32::from(r.left)
        && x < i32::from(r.right)
}

#[test]
fn rectangular_clip_fills_the_intersection_with_one_write_per_row() {
    for depth in [Depth::D8, Depth::D16] {
        for pattern in [[0xFF; 8], [0x00; 8], GRAY, MIXED] {
            for pat_origin in [(0, 0), (3, -2)] {
                let surface = surface(depth);
                let ink = Ink {
                    pat_origin,
                    ..ink(pattern)
                };
                let clip = ClipRows::rectangular(rect(-1, -2, 7, 15));
                let target = rect(-6, -4, 5, 30);
                let mut image = image_for(&surface);
                let before = image.bytes.clone();
                let outcome =
                    paint_rect(&mut image, &surface, &clip, &ink, target, &mut Vec::new());
                let area = rect(-1, -2, 5, 15);
                assert!(matches!(
                    outcome,
                    DrawOutcome::Drawn { touched } if touched.top == area.top
                        && touched.left == area.left
                        && touched.bottom == area.bottom
                        && touched.right == area.right
                ));
                let expected = reference(&surface, |x, y| inside(area, x, y), &ink, &before);
                assert_eq!(
                    image.bytes, expected,
                    "{depth:?} {pattern:02x?} {pat_origin:?}"
                );
                assert_eq!(image.writes.len(), 6, "one write per scanline");
            }
        }
    }
}

#[test]
fn clip_rows_write_one_run_per_covered_interval() {
    // A clip with a hole: rows 0..2 are two intervals, row 2 one interval,
    // row 3 empty, row 4 an interval that reaches past the image.
    let rows = vec![
        vec![-4, 1, 4, 9],
        vec![-4, 1, 4, 9],
        vec![-10, 30],
        vec![],
        vec![10, 40],
    ];
    for depth in [Depth::D8, Depth::D16] {
        for pattern in [[0xFF; 8], GRAY, MIXED] {
            let surface = surface(depth);
            let ink = ink(pattern);
            let clip = ClipRows::from_rows(0, rows.clone());
            let target = rect(-2, -3, 8, 12);
            let mut image = image_for(&surface);
            let before = image.bytes.clone();
            paint_rect(&mut image, &surface, &clip, &ink, target, &mut Vec::new());
            let covered = |x: i32, y: i32| {
                inside(target, x, y)
                    && (0..rows.len() as i32).contains(&y)
                    && endpoints_contain_point(&rows[y as usize], x as i16)
            };
            assert_eq!(
                image.bytes,
                reference(&surface, covered, &ink, &before),
                "{depth:?} {pattern:02x?}"
            );
            // Rows 0 and 1: two runs each; row 2: one; row 3: none; row 4:
            // one (10..12).
            assert_eq!(
                image.writes.len(),
                2 + 2 + 1 + 1,
                "{depth:?} {pattern:02x?}"
            );
        }
    }
}

#[test]
fn nothing_is_written_outside_the_image_or_an_empty_clip() {
    let surface = surface(Depth::D8);
    for (target, clip) in [
        (
            rect(20, 0, 30, 10),
            ClipRows::rectangular(ClipRows::UNBOUNDED),
        ),
        (rect(0, 0, 5, 5), ClipRows::rectangular(EMPTY_RECT)),
        (rect(0, 0, 0, 5), ClipRows::rectangular(ClipRows::UNBOUNDED)),
        (rect(0, 0, 5, 5), ClipRows::from_rows(0, vec![vec![]; 5])),
    ] {
        let mut image = image_for(&surface);
        let before = image.bytes.clone();
        let outcome = paint_rect(
            &mut image,
            &surface,
            &clip,
            &ink(GRAY),
            target,
            &mut Vec::new(),
        );
        assert!(matches!(outcome, DrawOutcome::Drawn { .. }));
        assert!(image.writes.is_empty());
        assert_eq!(image.bytes, before);
    }
}

#[test]
fn unsupported_depths_modes_and_pixel_patterns_are_declined_before_any_write() {
    for depth in [Depth::D1, Depth::D2, Depth::D4, Depth::D32] {
        assert!(
            !supports(depth, TransferMode::PatCopy, PatternKind::Mono),
            "{depth:?}"
        );
        let surface = Surface {
            depth,
            ..surface(Depth::D8)
        };
        let mut image = image_for(&surface);
        let outcome = paint_rect(
            &mut image,
            &surface,
            &ClipRows::rectangular(ClipRows::UNBOUNDED),
            &ink(GRAY),
            rect(0, 0, 4, 4),
            &mut Vec::new(),
        );
        assert!(matches!(outcome, DrawOutcome::Declined), "{depth:?}");
        assert!(image.writes.is_empty());
    }
    for mode in [
        TransferMode::PatOr,
        TransferMode::PatXor,
        TransferMode::PatBic,
        TransferMode::NotPatCopy,
        TransferMode::NotPatOr,
        TransferMode::NotPatXor,
        TransferMode::NotPatBic,
    ] {
        assert!(!supports(Depth::D8, mode, PatternKind::Mono), "{mode:?}");
        let surface = surface(Depth::D8);
        let mut image = image_for(&surface);
        let outcome = paint_rect(
            &mut image,
            &surface,
            &ClipRows::rectangular(ClipRows::UNBOUNDED),
            &Ink { mode, ..ink(GRAY) },
            rect(0, 0, 4, 4),
            &mut Vec::new(),
        );
        assert!(matches!(outcome, DrawOutcome::Declined), "{mode:?}");
        assert!(image.writes.is_empty());
    }
    for depth in [Depth::D8, Depth::D16] {
        assert!(supports(depth, TransferMode::PatCopy, PatternKind::Mono));
        assert!(!supports(depth, TransferMode::PatCopy, PatternKind::Pixel));
    }
    assert_eq!(Depth::from_bits(8), Some(Depth::D8));
    assert_eq!(Depth::from_bits(16), Some(Depth::D16));
    assert_eq!(Depth::from_bits(3), None);
}
