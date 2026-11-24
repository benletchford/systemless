//! `paint_rect` against a per-pixel reference, with a memory that records
//! every row write.

use super::*;
use crate::quickdraw::raster::region_rows::endpoints_contain_point;

#[test]
fn gpu_clip_rows_preserve_empty_bands_and_decline_malformed_intervals() {
    let area = Rect {
        top: -2,
        left: -4,
        bottom: 3,
        right: 9,
    };
    let clip = ClipRows::from_rows(-1, vec![vec![-8, 0, 3, 8], vec![], vec![0, 12]]);
    assert_eq!(
        clip.clipped_rows(area),
        Some(vec![vec![], vec![-4, 0, 3, 8], vec![], vec![0, 9], vec![]])
    );
    let clip = ClipRows::rectangular(Rect {
        top: 0,
        left: -1,
        bottom: 2,
        right: 5,
    });
    assert_eq!(
        clip.clipped_rows(area),
        Some(vec![vec![], vec![], vec![-1, 5], vec![-1, 5], vec![]])
    );
    for row in [vec![0, 3, 5], vec![3, 0], vec![0, 3, 2, 4]] {
        assert!(ClipRows::from_rows(0, vec![row])
            .clipped_rows(area)
            .is_none());
    }
}

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

#[test]
fn mac_memory_bus_and_guest_address_space_match_bytes_and_write_runs() {
    use crate::memory::{write_log, GuestAddressSpace, MacMemoryBus, MemoryBus, NOTE_WRITE_LOG};

    let holed_rows = vec![
        vec![-4, 1, 4, 9],
        vec![-4, 1, 4, 9],
        vec![-10, 30],
        vec![],
        vec![10, 40],
    ];
    for depth in [Depth::D8, Depth::D16] {
        for pattern in [[0xFF; 8], [0x00; 8], GRAY, MIXED] {
            for pat_origin in [(0, 0), (3, -2)] {
                for use_holed_clip in [false, true] {
                    let surface = Surface {
                        base: 0x10_000,
                        ..surface(depth)
                    };
                    let clip = if use_holed_clip {
                        ClipRows::from_rows(0, holed_rows.clone())
                    } else {
                        ClipRows::rectangular(rect(-1, -2, 7, 15))
                    };
                    let target = rect(-2, -3, 8, 16);
                    let draw_ink = Ink {
                        pat_origin,
                        ..ink(pattern)
                    };
                    let byte_len =
                        (surface.bounds.bottom - surface.bounds.top) as usize * surface.row_bytes as usize;
                    let seed: Vec<u8> = (0..byte_len).map(|i| (i * 37 + 11) as u8).collect();

                    let mut bus = MacMemoryBus::new(0x4_0000);
                    bus.write_bytes(surface.base, &seed);
                    let mut scratch_bus = Vec::new();
                    write_log::start();
                    let bus_outcome = paint_rect(
                        &mut bus,
                        &surface,
                        &clip,
                        &draw_ink,
                        target,
                        &mut scratch_bus,
                    );
                    let bus_writes = write_log::take();

                    let mut guest = GuestAddressSpace::new();
                    guest.add_region(surface.base, seed.clone());
                    let mut scratch_guest = Vec::new();
                    NOTE_WRITE_LOG.with_borrow_mut(|log| *log = Some(Vec::new()));
                    write_log::start();
                    let guest_outcome = paint_rect(
                        &mut guest,
                        &surface,
                        &clip,
                        &draw_ink,
                        target,
                        &mut scratch_guest,
                    );
                    let guest_writes = write_log::take();
                    let guest_notes =
                        NOTE_WRITE_LOG.with_borrow_mut(|log| log.take().unwrap_or_default());

                    assert!(matches!(bus_outcome, DrawOutcome::Drawn { .. }));
                    assert!(matches!(guest_outcome, DrawOutcome::Drawn { .. }));
                    let mut guest_bytes = vec![0u8; byte_len];
                    guest.read_bytes_into(surface.base, &mut guest_bytes).unwrap();
                    assert_eq!(
                        bus.read_bytes(surface.base, byte_len),
                        guest_bytes,
                        "{depth:?} {pattern:02x?} {pat_origin:?} holed={use_holed_clip}"
                    );
                    assert_eq!(bus_writes, guest_writes);
                    assert_eq!(
                        guest_notes
                            .iter()
                            .map(|(addr, data)| (*addr, data.len()))
                            .collect::<Vec<_>>(),
                        guest_writes
                    );
                }
            }
        }
    }
}

#[test]
fn m68k_solid_rect_route_matches_legacy_row_loop_across_presentation_probes_and_readonly_code() {
    use crate::cpu::{CpuOps, Register};
    use crate::memory::{write_log, CompactPresentation, MemoryBus};
    use crate::trap::test_helpers::{region_storage_bytes, setup};
    use crate::trap::types::ShapeOp;

    const BASE: u32 = 0x2_0000;
    const BOUNDS: (i16, i16, i16, i16) = (-3, -5, 9, 19);
    const ROW_BYTES: u16 = 32;
    const WIDTH: u16 = 24;
    const HEIGHT: u16 = 12;
    let byte_len = usize::from(ROW_BYTES) * usize::from(HEIGHT);
    let seed: Vec<u8> = (0..byte_len).map(|i| (i * 53 + 19) as u8).collect();
    let palette: [[u8; 3]; 256] =
        std::array::from_fn(|i| [i as u8, (i * 5) as u8, (255 - i) as u8]);

    let build_env = |with_presentation: bool, protect_row: Option<u32>, arm_probe: bool| {
        let (mut d, mut cpu, mut bus) = setup();
        bus.write_bytes(BASE, &seed);
        let pixmap_handle = bus.alloc(4);
        let pixmap = bus.alloc(50);
        bus.write_long(pixmap_handle, pixmap);
        bus.write_long(pixmap, BASE);
        bus.write_word(pixmap + 4, 0x8000 | ROW_BYTES);
        for (offset, word) in [(6, BOUNDS.0), (8, BOUNDS.1), (10, BOUNDS.2), (12, BOUNDS.3)] {
            bus.write_word(pixmap + offset, word as u16);
        }
        bus.write_word(pixmap + 30, 0);
        bus.write_word(pixmap + 32, 8);
        bus.write_word(pixmap + 34, 1);
        bus.write_word(pixmap + 36, 8);
        bus.write_long(pixmap + 42, 0);

        let port = bus.alloc(108);
        bus.write_long(port + 2, pixmap_handle);
        bus.write_word(port + 6, 0xC000);
        for (offset, word) in [(16, BOUNDS.0), (18, BOUNDS.1), (20, BOUNDS.2), (22, BOUNDS.3)] {
            bus.write_word(port + offset, word as u16);
        }
        for (offset, bbox) in [(24, (-2, -4, 8, 17)), (28, (-1, -2, 7, 15))] {
            let storage = region_storage_bytes(bbox, &[]);
            let handle = bus.alloc(4);
            let ptr = bus.alloc(storage.len() as u32);
            bus.write_bytes(ptr, &storage);
            bus.write_long(handle, ptr);
            bus.write_long(port + offset, handle);
        }
        bus.write_long(port + 80, 103);
        bus.write_long(port + 84, 42);
        d.set_screen_mode_for_test(BASE, u32::from(ROW_BYTES), WIDTH, HEIGHT, 8);
        d.set_current_port_state(&mut bus, &mut cpu, port, None);
        d.resolved_port_color_fields.insert(port, 0x03);
        if with_presentation {
            bus.enable_outline_presentation(
                (BASE, u32::from(ROW_BYTES), WIDTH, HEIGHT, 8),
                palette,
                2,
            );
            // Seed retained outline glyph coverage inside the port so the solid
            // rectangle must clear covered detail cells identically to the legacy loop.
            d.pn_loc = (2, 4);
            bus.write_word(port + 48, 4);
            bus.write_word(port + 50, 2);
            bus.write_word(port + 74, 9);
            d.tx_size = 9;
            let sp = cpu.read_reg(Register::A7) - 2;
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, u16::from(b'M'));
            let _ = d.dispatch(0xA883, &mut cpu, &mut bus);
        }
        if let Some(row_index) = protect_row {
            let row_addr = BASE + row_index * u32::from(ROW_BYTES) + 5;
            bus.protect_readonly_code(row_addr, 2);
        }
        if arm_probe {
            bus.begin_write_probe();
            bus.arm_access_watch(BASE, BASE + byte_len as u32);
        }
        (d, cpu, bus)
    };

    let target = rect(-4, -3, 6, 20);
    // Clipped intersection against vis (-2,-4,8,17) and clip (-1,-2,7,15) is (-1,-2,6,15).
    let (clip_top, clip_left, clip_bottom, clip_right) = (-1i16, -2i16, 6i16, 15i16);

    for (op, pn_pat, bk_pat, fill_idx) in [
        (ShapeOp::Paint, [0xFF; 8], [0x00; 8], 103u8),
        (ShapeOp::Paint, [0x00; 8], [0x00; 8], 42u8),
        (ShapeOp::Erase, [0xFF; 8], [0x00; 8], 42u8),
        (ShapeOp::Erase, [0xFF; 8], [0xFF; 8], 103u8),
        (ShapeOp::Fill([0xFF; 8]), [0xAA; 8], [0x55; 8], 103u8),
        (ShapeOp::Fill([0x00; 8]), [0xAA; 8], [0x55; 8], 42u8),
    ] {
        for (with_presentation, protect_row, arm_probe) in [
            (false, None, false),
            (true, None, false),
            (false, Some(3), false),
            (false, None, true),
        ] {
            let (mut d_new, mut cpu_new, mut bus_new) =
                build_env(with_presentation, protect_row, arm_probe);
            d_new.pn_pat = pn_pat;
            d_new.bk_pat = bk_pat;
            write_log::start();
            d_new.draw_rect(&mut cpu_new, &mut bus_new, &target, op.clone());
            let new_writes = write_log::take();
            let watch_new = arm_probe.then(|| bus_new.take_access_watch());

            let (_, _, mut bus_legacy) = build_env(with_presentation, protect_row, arm_probe);
            let legacy_row = vec![fill_idx; (clip_right - clip_left) as usize];
            let dx = (clip_left - BOUNDS.1) as u32;
            write_log::start();
            for y in clip_top..clip_bottom {
                let dy = (y - BOUNDS.0) as u32;
                let addr = BASE + dy * u32::from(ROW_BYTES) + dx;
                bus_legacy.write_bytes(addr, &legacy_row);
            }
            let legacy_writes = write_log::take();
            let watch_legacy = arm_probe.then(|| bus_legacy.take_access_watch());

            assert_eq!(new_writes, legacy_writes);
            assert_eq!(
                bus_new.read_bytes(BASE, byte_len),
                bus_legacy.read_bytes(BASE, byte_len)
            );
            if with_presentation {
                let mut compact_new = CompactPresentation::default();
                let mut compact_legacy = CompactPresentation::default();
                assert!(bus_new
                    .compact_presentation_without_overlays((WIDTH.into(), HEIGHT.into()), &mut compact_new));
                assert!(bus_legacy.compact_presentation_without_overlays(
                    (WIDTH.into(), HEIGHT.into()),
                    &mut compact_legacy,
                ));
                assert_eq!(compact_new.cells, compact_legacy.cells);
                assert_eq!(compact_new.detail, compact_legacy.detail);
            }
            if arm_probe {
                assert_eq!(watch_new, watch_legacy);
                assert_eq!(
                    bus_new.finish_write_probe_changes(),
                    bus_legacy.finish_write_probe_changes()
                );
            }
        }
    }

    // Verify zero steady-state scratch reallocations across repeated 68K solid fills.
    let (mut d, mut cpu, mut bus) = build_env(false, None, false);
    d.draw_rect(&mut cpu, &mut bus, &target, ShapeOp::Paint);
    let ptr_after_first = d.raster_scratch.as_ptr();
    let cap_after_first = d.raster_scratch.capacity();
    assert!(cap_after_first >= (clip_right - clip_left) as usize);
    for r in [target, rect(0, 0, 4, 8), rect(-1, -2, 6, 15)] {
        d.draw_rect(&mut cpu, &mut bus, &r, ShapeOp::Fill([0xFF; 8]));
        assert_eq!(d.raster_scratch.as_ptr(), ptr_after_first);
        assert_eq!(d.raster_scratch.capacity(), cap_after_first);
    }
}

#[test]
#[ignore = "measurement harness; prints raster paint_rect timings and write counts"]
fn paint_rect_solid_and_patterned_microbench() {
    use crate::memory::{write_log, GuestAddressSpace, MacMemoryBus};
    use std::time::Instant;

    const ITERS_SMALL: usize = 50_000;
    const ITERS_LARGE: usize = 2_000;
    let mut bus = MacMemoryBus::new(0x20_0000);
    let mut guest = GuestAddressSpace::new();
    guest.add_region(0x10_000, vec![0; 640 * 480 * 2]);
    let mut scratch = Vec::with_capacity(1280);

    for (label, depth, pattern, rect_size, iters) in [
        ("8bpp-solid-8x8", Depth::D8, [0xFF; 8], (8i16, 8i16), ITERS_SMALL),
        ("8bpp-gray-8x8", Depth::D8, GRAY, (8, 8), ITERS_SMALL),
        ("8bpp-solid-640x480", Depth::D8, [0xFF; 8], (640, 480), ITERS_LARGE),
        ("8bpp-gray-640x480", Depth::D8, GRAY, (640, 480), ITERS_LARGE),
        ("16bpp-solid-640x480", Depth::D16, [0xFF; 8], (640, 480), ITERS_LARGE),
        ("1bpp-declined-8x8", Depth::D1, [0xFF; 8], (8, 8), ITERS_SMALL),
    ] {
        let bpp = if depth == Depth::D16 { 2 } else { 1 };
        let surface = Surface {
            base: 0x10_000,
            row_bytes: 640 * bpp,
            depth,
            bounds: rect(0, 0, 480, 640),
        };
        let clip = ClipRows::rectangular(surface.bounds);
        let draw_ink = ink(pattern);
        let target = rect(0, 0, rect_size.1, rect_size.0);

        let _ = paint_rect(&mut bus, &surface, &clip, &draw_ink, target, &mut scratch);
        let cap_before = scratch.capacity();
        write_log::start();
        let _ = paint_rect(&mut bus, &surface, &clip, &draw_ink, target, &mut scratch);
        let writes_per_draw = write_log::take().len();
        assert_eq!(scratch.capacity(), cap_before, "steady-state scratch reallocated");

        let start_bus = Instant::now();
        for _ in 0..iters {
            std::hint::black_box(paint_rect(
                &mut bus,
                &surface,
                &clip,
                &draw_ink,
                target,
                &mut scratch,
            ));
        }
        let ns_bus = start_bus.elapsed().as_nanos() as f64 / iters as f64;

        let start_guest = Instant::now();
        for _ in 0..iters {
            std::hint::black_box(paint_rect(
                &mut guest,
                &surface,
                &clip,
                &draw_ink,
                target,
                &mut scratch,
            ));
        }
        let ns_guest = start_guest.elapsed().as_nanos() as f64 / iters as f64;

        eprintln!(
            "[RASTER-BENCH] case={label} writes_per_draw={writes_per_draw} \
             m68k_bus_ns={ns_bus:.1} ppc_guest_ns={ns_guest:.1}"
        );
    }
}
