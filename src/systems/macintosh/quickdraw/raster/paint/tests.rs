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

/// A pixel image in host memory that records each `write_row` and `modify_row`
/// call.
struct RecordingImage {
    base: u32,
    bytes: Vec<u8>,
    writes: Vec<(u32, usize)>,
    reads: Vec<(u32, usize)>,
}

impl RecordingImage {
    fn new(base: u32, len: usize) -> Self {
        Self {
            base,
            bytes: (0..len).map(|i| (i * 37 + 11) as u8).collect(),
            writes: Vec::new(),
            reads: Vec::new(),
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

    fn modify_row(&mut self, addr: u32, buf: &mut [u8], f: impl FnOnce(&mut [u8])) -> Option<()> {
        self.reads.push((addr, buf.len()));
        let start = addr.checked_sub(self.base)? as usize;
        let slice = self.bytes.get_mut(start..start + buf.len())?;
        buf.copy_from_slice(slice);
        f(buf);
        self.writes.push((addr, buf.len()));
        slice.copy_from_slice(buf);
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
        TransferMode::NotPatOr,
        TransferMode::NotPatXor,
        TransferMode::NotPatBic,
    ] {
        assert!(supports(Depth::D8, mode, PatternKind::Mono), "{mode:?}");
        assert!(!supports(Depth::D16, mode, PatternKind::Mono), "{mode:?}");
        let surface = surface(Depth::D16);
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
        assert!(supports(depth, TransferMode::NotPatCopy, PatternKind::Mono));
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

fn reference_boolean_8(
    surface: &Surface,
    covered: impl Fn(i32, i32) -> bool,
    ink: &Ink,
    before: &[u8],
) -> Vec<u8> {
    let mut out = before.to_vec();
    let b = surface.bounds;
    let fg = ink.fg as u8;
    let bg = ink.bg as u8;
    for y in i32::from(b.top)..i32::from(b.bottom) {
        for x in i32::from(b.left)..i32::from(b.right) {
            if !covered(x, y) {
                continue;
            }
            let row = ink.pattern[(y - i32::from(ink.pat_origin.1)).rem_euclid(8) as usize];
            let bit = (x - i32::from(ink.pat_origin.0)).rem_euclid(8);
            let src_black = (row & (0x80 >> bit) != 0) ^ ink.mode.inverts_source();
            let offset = (y - i32::from(b.top)) as usize * surface.row_bytes as usize
                + (x - i32::from(b.left)) as usize;
            let old = out[offset];
            out[offset] = match ink.mode {
                TransferMode::PatCopy | TransferMode::NotPatCopy => {
                    if src_black {
                        fg
                    } else {
                        bg
                    }
                }
                TransferMode::PatOr | TransferMode::NotPatOr => {
                    if src_black {
                        fg
                    } else {
                        old
                    }
                }
                TransferMode::PatXor | TransferMode::NotPatXor => {
                    if src_black {
                        255 - old
                    } else {
                        old
                    }
                }
                TransferMode::PatBic | TransferMode::NotPatBic => {
                    if src_black {
                        bg
                    } else {
                        old
                    }
                }
            };
        }
    }
    out
}

#[test]
fn all_eight_8bpp_transfer_modes_match_reference_and_both_memories() {
    use crate::memory::{write_log, GuestAddressSpace, MacMemoryBus, MemoryBus, NOTE_WRITE_LOG};

    let all_modes = [
        TransferMode::PatCopy,
        TransferMode::PatOr,
        TransferMode::PatXor,
        TransferMode::PatBic,
        TransferMode::NotPatCopy,
        TransferMode::NotPatOr,
        TransferMode::NotPatXor,
        TransferMode::NotPatBic,
    ];
    let holed_rows = vec![
        vec![-4, 1, 4, 9],
        vec![-4, 1, 4, 9],
        vec![-10, 30],
        vec![],
        vec![10, 40],
    ];
    let mut scratch_bus = Vec::with_capacity(256);
    let mut scratch_guest = Vec::with_capacity(256);
    let mut scratch_rec = Vec::with_capacity(256);
    let bus_ptr = scratch_bus.as_ptr();
    let bus_cap = scratch_bus.capacity();

    for mode in all_modes {
        for pattern in [[0xFF; 8], [0x00; 8], GRAY, MIXED] {
            for pat_origin in [(0, 0), (3, -2)] {
                for use_holed_clip in [false, true] {
                    let surface = Surface {
                        base: 0x10_000,
                        ..surface(Depth::D8)
                    };
                    let clip = if use_holed_clip {
                        ClipRows::from_rows(0, holed_rows.clone())
                    } else {
                        ClipRows::rectangular(rect(-1, -2, 7, 15))
                    };
                    let target = rect(-2, -3, 8, 16);
                    let draw_ink = Ink {
                        fg: 0xC5,
                        bg: 0x3A,
                        pattern,
                        mode,
                        pat_origin,
                    };
                    let mut rec = image_for(&surface);
                    let before = rec.bytes.clone();
                    let rec_outcome =
                        paint_rect(&mut rec, &surface, &clip, &draw_ink, target, &mut scratch_rec);
                    assert!(matches!(rec_outcome, DrawOutcome::Drawn { .. }));
                    if mode.is_copy() {
                        assert!(rec.reads.is_empty(), "copy modes must not read destination");
                    } else {
                        assert_eq!(rec.reads, rec.writes);
                    }
                    let expected = reference_boolean_8(
                        &surface,
                        |x, y| {
                            inside(target, x, y)
                                && if use_holed_clip {
                                    (0..holed_rows.len() as i32).contains(&y)
                                        && endpoints_contain_point(&holed_rows[y as usize], x as i16)
                                } else {
                                    inside(rect(-1, -2, 7, 15), x, y)
                                }
                        },
                        &draw_ink,
                        &before,
                    );
                    assert_eq!(
                        rec.bytes, expected,
                        "{mode:?} {pattern:02x?} {pat_origin:?} holed={use_holed_clip}"
                    );

                    let mut bus = MacMemoryBus::new(0x4_0000);
                    bus.write_bytes(surface.base, &before);
                    write_log::start();
                    let _ =
                        paint_rect(&mut bus, &surface, &clip, &draw_ink, target, &mut scratch_bus);
                    let bus_writes = write_log::take();

                    let mut guest = GuestAddressSpace::new();
                    guest.add_region(surface.base, before.clone());
                    NOTE_WRITE_LOG.with_borrow_mut(|log| *log = Some(Vec::new()));
                    write_log::start();
                    let _ = paint_rect(
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

                    assert_eq!(bus.read_bytes(surface.base, before.len()), expected);
                    let mut guest_bytes = vec![0u8; before.len()];
                    guest.read_bytes_into(surface.base, &mut guest_bytes).unwrap();
                    assert_eq!(guest_bytes, expected);
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
    assert_eq!(scratch_bus.as_ptr(), bus_ptr);
    assert_eq!(scratch_bus.capacity(), bus_cap);
}

fn reference_solid_1bpp(
    surface: &Surface,
    covered: impl Fn(i32, i32) -> bool,
    op: BitRowOp,
    before: &[u8],
) -> Vec<u8> {
    let mut out = before.to_vec();
    let b = surface.bounds;
    let max_bits = surface.row_bytes * 8;
    for y in i32::from(b.top)..i32::from(b.bottom) {
        for x in i32::from(b.left)..i32::from(b.right) {
            if !covered(x, y) {
                continue;
            }
            let bit_index = (x - i32::from(b.left)) as u32;
            if bit_index >= max_bits {
                continue;
            }
            let byte_offset = (y - i32::from(b.top)) as usize * surface.row_bytes as usize
                + (bit_index / 8) as usize;
            let mask = 0x80u8 >> (bit_index % 8);
            match op {
                BitRowOp::Set => out[byte_offset] |= mask,
                BitRowOp::Clear => out[byte_offset] &= !mask,
                BitRowOp::Toggle => out[byte_offset] ^= mask,
            }
        }
    }
    out
}

#[test]
fn solid_1bpp_rect_matches_reference_and_both_memories() {
    use crate::memory::{write_log, GuestAddressSpace, MacMemoryBus, MemoryBus, NOTE_WRITE_LOG};

    // Bounds span 40 pixels wide, while row_bytes = 4 (32 bits) exercises the
    // row_bytes * 8 truncation guard.
    let surface = Surface {
        base: 0x10_000,
        row_bytes: 4,
        depth: Depth::D1,
        bounds: rect(-2, -3, 6, 37),
    };
    let byte_len =
        (surface.bounds.bottom - surface.bounds.top) as usize * surface.row_bytes as usize;
    let seed: Vec<u8> = (0..byte_len).map(|i| (i * 73 + 29) as u8).collect();
    let spans_to_test = [
        rect(-1, -2, 3, 2),   // single-byte unaligned interior (bits 1..5)
        rect(-1, -3, 3, 5),   // exact single whole byte (bits 0..8)
        rect(-1, 2, 4, 12),   // two adjacent bytes, zero middle bytes (bits 5..15)
        rect(-2, -1, 5, 25),  // head + middle + tail (bits 2..28)
        rect(0, 5, 4, 21),    // byte-aligned multi-byte (bits 8..24)
        rect(-2, 10, 6, 37),  // reaches past row_bytes * 8 (bits 13..40 -> clamped to 32)
        rect(0, 30, 4, 37),   // completely past row_bytes * 8 (bits 33..40 -> empty)
    ];

    let mut scratch_bus = Vec::with_capacity(16);
    let mut scratch_guest = Vec::with_capacity(16);
    let bus_ptr = scratch_bus.as_ptr();
    let bus_cap = scratch_bus.capacity();

    for op in [BitRowOp::Set, BitRowOp::Clear, BitRowOp::Toggle] {
        for target in spans_to_test {
            let clip = ClipRows::rectangular(surface.bounds);
            let expected =
                reference_solid_1bpp(&surface, |x, y| inside(target, x, y), op, &seed);

            let mut bus = MacMemoryBus::new(0x4_0000);
            bus.write_bytes(surface.base, &seed);
            write_log::start();
            let bus_out =
                paint_solid_1bpp_rect(&mut bus, &surface, &clip, target, op, &mut scratch_bus);
            let bus_writes = write_log::take();

            let mut guest = GuestAddressSpace::new();
            guest.add_region(surface.base, seed.clone());
            NOTE_WRITE_LOG.with_borrow_mut(|log| *log = Some(Vec::new()));
            write_log::start();
            let guest_out = paint_solid_1bpp_rect(
                &mut guest,
                &surface,
                &clip,
                target,
                op,
                &mut scratch_guest,
            );
            let guest_writes = write_log::take();
            let guest_notes =
                NOTE_WRITE_LOG.with_borrow_mut(|log| log.take().unwrap_or_default());

            assert!(matches!(bus_out, DrawOutcome::Drawn { .. }));
            assert!(matches!(guest_out, DrawOutcome::Drawn { .. }));
            assert_eq!(
                bus.read_bytes(surface.base, byte_len),
                expected,
                "{op:?} {target:?}"
            );
            let mut guest_bytes = vec![0u8; byte_len];
            guest.read_bytes_into(surface.base, &mut guest_bytes).unwrap();
            assert_eq!(guest_bytes, expected, "{op:?} {target:?}");
            assert_eq!(bus_writes, guest_writes, "{op:?} {target:?}");
            assert_eq!(
                guest_notes
                    .iter()
                    .map(|(addr, data)| (*addr, data.len()))
                    .collect::<Vec<_>>(),
                guest_writes
            );
        }
    }
    assert_eq!(scratch_bus.as_ptr(), bus_ptr);
    assert_eq!(scratch_bus.capacity(), bus_cap);

    // Non-1-bpp surfaces are declined without writing.
    let mut rec = image_for(&super::tests::surface(Depth::D8));
    let outcome = paint_solid_1bpp_rect(
        &mut rec,
        &super::tests::surface(Depth::D8),
        &ClipRows::rectangular(ClipRows::UNBOUNDED),
        rect(0, 0, 4, 4),
        BitRowOp::Set,
        &mut Vec::new(),
    );
    assert!(matches!(outcome, DrawOutcome::Declined));
    assert!(rec.writes.is_empty());
}

#[test]
fn m68k_patterned_8bpp_and_solid_1bpp_routes_match_reference_and_bus_policies() {
    use crate::cpu::{CpuOps, Register};
    use crate::memory::{CompactPresentation, MemoryBus};
    use crate::trap::test_helpers::{region_storage_bytes, setup};
    use crate::trap::types::ShapeOp;

    const BASE: u32 = 0x2_0000;
    const BOUNDS: (i16, i16, i16, i16) = (-3, -5, 9, 19);
    const ROW_BYTES_8: u16 = 32;
    const ROW_BYTES_1: u16 = 6;
    const WIDTH: u16 = 24;
    const HEIGHT: u16 = 12;
    let byte_len_8 = usize::from(ROW_BYTES_8) * usize::from(HEIGHT);
    let seed_8: Vec<u8> = (0..byte_len_8).map(|i| (i * 41 + 7) as u8).collect();
    let palette: [[u8; 3]; 256] =
        std::array::from_fn(|i| [i as u8, (i * 3) as u8, (255 - i) as u8]);
    let target = rect(-2, -3, 8, 17);
    let clipped = rect(-1, -2, 7, 15);
    let clip_tuple = (clipped.top, clipped.left, clipped.bottom, clipped.right);

    let init_port = |seed: &[u8], row_bytes: u16, is_color_8: bool| {
        let (mut d, mut cpu, mut bus) = setup();
        bus.write_bytes(BASE, seed);
        let port = bus.alloc(108);
        bus.write_word(port, 0);
        if is_color_8 {
            let pm_h = bus.alloc(4);
            let pm = bus.alloc(50);
            bus.write_long(pm_h, pm);
            bus.write_long(pm, BASE);
            bus.write_word(pm + 4, 0x8000 | row_bytes);
            for (o, w) in [(6, BOUNDS.0), (8, BOUNDS.1), (10, BOUNDS.2), (12, BOUNDS.3)] {
                bus.write_word(pm + o, w as u16);
            }
            bus.write_word(pm + 30, 0);
            bus.write_word(pm + 32, 8);
            bus.write_long(port + 2, pm_h);
            bus.write_word(port + 6, 0xC000);
            bus.write_long(port + 80, 0xC5);
            bus.write_long(port + 84, 0x3A);
            d.set_screen_mode_for_test(BASE, u32::from(row_bytes), WIDTH, HEIGHT, 8);
            d.resolved_port_color_fields.insert(port, 0x03);
        } else {
            bus.write_long(port + 2, BASE);
            bus.write_word(port + 6, row_bytes);
            bus.write_long(port + 68, 33);
            bus.write_long(port + 72, 30);
        }
        for (o, w) in [(8, BOUNDS.0), (10, BOUNDS.1), (12, BOUNDS.2), (14, BOUNDS.3)] {
            bus.write_word(port + o, w as u16);
        }
        for (o, r) in [(24, BOUNDS), (28, clip_tuple)] {
            let bytes = region_storage_bytes(r, &[]);
            let ptr = bus.alloc(bytes.len() as u32);
            bus.write_bytes(ptr, &bytes);
            let h = bus.alloc(4);
            bus.write_long(h, ptr);
            bus.write_long(port + o, h);
        }
        let qd_globals = bus.read_long(cpu.read_reg(Register::A5));
        bus.write_long(qd_globals, port);
        d.set_current_port_state(&mut bus, &mut cpu, port, None);
        (d, cpu, bus)
    };

    for qd_mode in 8..=15i16 {
        let (mut d, mut cpu, mut bus) = init_port(&seed_8, ROW_BYTES_8, true);
        d.pn_pat = MIXED;
        d.pn_mode = qd_mode;
        d.bk_pat = GRAY;
        bus.enable_outline_presentation(
            (BASE, u32::from(ROW_BYTES_8), WIDTH, HEIGHT, 8),
            palette,
            2,
        );
        bus.begin_write_probe();
        bus.arm_access_watch(BASE, BASE + byte_len_8 as u32);

        d.draw_rect(&mut cpu, &mut bus, &target, ShapeOp::Paint);

        let write_hits: Vec<_> = bus
            .take_access_watch()
            .unwrap()
            .into_iter()
            .filter(|h| h.write)
            .collect();
        let probe_ranges = bus.finish_write_probe_ranges();
        let expected_surface = Surface {
            base: BASE,
            row_bytes: u32::from(ROW_BYTES_8),
            depth: Depth::D8,
            bounds: rect(BOUNDS.0, BOUNDS.1, BOUNDS.2, BOUNDS.3),
        };
        let expected_ink = Ink {
            fg: 0xC5,
            bg: 0x3A,
            pattern: MIXED,
            mode: TransferMode::from_quickdraw_mode(qd_mode),
            pat_origin: (0, 0),
        };
        let expected =
            reference_boolean_8(&expected_surface, |x, y| inside(clipped, x, y), &expected_ink, &seed_8);
        assert_eq!(bus.read_bytes(BASE, byte_len_8), expected, "mode={qd_mode}");
        assert_eq!(probe_ranges.len(), 8);

        let mut bus_ref = crate::memory::MacMemoryBus::new(0x4_0000);
        bus_ref.write_bytes(BASE, &seed_8);
        bus_ref.enable_outline_presentation(
            (BASE, u32::from(ROW_BYTES_8), WIDTH, HEIGHT, 8),
            palette,
            2,
        );
        bus_ref.begin_write_probe();
        bus_ref.arm_access_watch(BASE, BASE + byte_len_8 as u32);
        let dx = (clipped.left - BOUNDS.1) as usize;
        let w = (clipped.right - clipped.left) as usize;
        for y in clipped.top..clipped.bottom {
            let row_off = (y - BOUNDS.0) as usize * usize::from(ROW_BYTES_8) + dx;
            bus_ref.write_bytes(BASE + row_off as u32, &expected[row_off..row_off + w]);
        }
        let ref_writes: Vec<_> = bus_ref
            .take_access_watch()
            .unwrap()
            .into_iter()
            .filter(|h| h.write)
            .collect();
        assert_eq!(write_hits, ref_writes, "mode={qd_mode}");
        assert_eq!(probe_ranges, bus_ref.finish_write_probe_ranges(), "mode={qd_mode}");
        let (mut c_new, mut c_ref) = (CompactPresentation::default(), CompactPresentation::default());
        assert!(bus.compact_presentation_without_overlays((WIDTH.into(), HEIGHT.into()), &mut c_new));
        assert!(bus_ref.compact_presentation_without_overlays((WIDTH.into(), HEIGHT.into()), &mut c_ref));
        assert_eq!((c_new.cells, c_new.detail), (c_ref.cells, c_ref.detail));
    }

    let byte_len_1 = usize::from(ROW_BYTES_1) * usize::from(HEIGHT);
    let seed_1: Vec<u8> = (0..byte_len_1).map(|i| (i * 29 + 3) as u8).collect();
    let surface_1 = Surface {
        base: BASE,
        row_bytes: u32::from(ROW_BYTES_1),
        depth: Depth::D1,
        bounds: rect(BOUNDS.0, BOUNDS.1, BOUNDS.2, BOUNDS.3),
    };
    for (op, expected_bit_op) in [
        (ShapeOp::Erase, BitRowOp::Clear),
        (ShapeOp::Paint, BitRowOp::Set),
        (ShapeOp::Fill([0xFF; 8]), BitRowOp::Set),
        (ShapeOp::Invert, BitRowOp::Toggle),
    ] {
        let (mut d, mut cpu, mut bus) = init_port(&seed_1, ROW_BYTES_1, false);
        d.pn_pat = [0xFF; 8];
        d.pn_mode = 8;
        d.bk_pat = [0x00; 8];
        d.draw_rect(&mut cpu, &mut bus, &target, op);
        let expected =
            reference_solid_1bpp(&surface_1, |x, y| inside(clipped, x, y), expected_bit_op, &seed_1);
        assert_eq!(bus.read_bytes(BASE, byte_len_1), expected, "{expected_bit_op:?}");
    }
}
