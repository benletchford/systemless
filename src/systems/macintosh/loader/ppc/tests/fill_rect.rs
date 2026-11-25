//! FillRect through the shared raster core: against the 68K FillRect path,
//! against the earlier PowerPC solid fill, and the cases the core declines.

use super::*;
use crate::memory::write_log;
use crate::trap::test_helpers::{
    draw_arc_8bpp_68k, draw_oval_8bpp_68k, draw_poly_8bpp_68k, draw_round_rect_8bpp_68k, fill_rect_8bpp_68k, region_storage_bytes,
    seed_fill_68k, IndexedPortFixture,
};
use crate::trap::types::{Rect, ShapeOp};

const STOP: i16 = i16::MAX;
const GRAY: [u8; 8] = [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];
const DIAGONAL: [u8; 8] = [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01];
const MIXED: [u8; 8] = [0x11, 0x22, 0x44, 0x88, 0x0F, 0xF0, 0x3C, 0xC3];

/// A 24×12 image whose top-left pixel is port point (-5, -3), so pattern
/// alignment to the port differs from alignment to the pixel image.
const BOUNDS: (i16, i16, i16, i16) = (-3, -5, 9, 19);
/// Room for 24 pixels at 16 bits, plus padding.
const ROW_BYTES: u16 = 50;
const HEIGHT: usize = 12;

const SCRATCH: u32 = PPC_DATA_BASE + 0x1c00;
const PORT: u32 = SCRATCH;
const PIXMAP_HANDLE: u32 = SCRATCH + 0x100;
const PIXMAP: u32 = SCRATCH + 0x120;
const REGION_HANDLES: [u32; 2] = [SCRATCH + 0x180, SCRATCH + 0x184];
const REGION_STORAGE: [u32; 2] = [SCRATCH + 0x200, SCRATCH + 0x300];
const RECT: u32 = SCRATCH + 0x400;
const PATTERN: u32 = SCRATCH + 0x410;
const PIXELS: u32 = SCRATCH + 0x800;

fn initial_pixels() -> Vec<u8> {
    (0..usize::from(ROW_BYTES) * HEIGHT)
        .map(|i| (i * 37 + 11) as u8)
        .collect()
}

/// Clips as (visRgn, clipRgn) bounding boxes and data words.
type RegionSpec = ((i16, i16, i16, i16), &'static [i16]);

const WHOLE_PORT: RegionSpec = (BOUNDS, &[]);
const CLIP_RECT: RegionSpec = ((-1, -2, 7, 15), &[]);
/// A rectangle with a hole at rows 1..4, columns 3..8.
const CLIP_WITH_HOLE: RegionSpec = (
    (-2, -4, 8, 17),
    &[
        -2, -4, 17, STOP, 1, 3, 8, STOP, 4, 3, 8, STOP, 8, -4, 17, STOP, STOP,
    ],
);
/// A rectangle notched at row 2, columns 10..12.
const VIS_NOTCHED: RegionSpec = (
    (-1, -3, 7, 16),
    &[
        -1, -3, 16, STOP, 2, 10, 12, STOP, 3, 10, 12, STOP, 7, -3, 16, STOP, STOP,
    ],
);

fn fixture<'a>(pixels: &'a [u8], vis: RegionSpec, clip: RegionSpec) -> IndexedPortFixture<'a> {
    IndexedPortFixture {
        bounds: BOUNDS,
        row_bytes: ROW_BYTES,
        pixels,
        fg_index: 103,
        bg_index: 42,
        vis,
        clip,
    }
}

/// Builds `fixture`'s port as a `depth`-bit GWorld in `loaded` and makes it
/// current, with the fixture's indices as explicit fore/back indices.
fn install_port(loaded: &mut PpcLoadedApp, fixture: &IndexedPortFixture<'_>, depth: u32) {
    let memory = &mut loaded.memory;
    memory.add_region(SCRATCH, vec![0; 0x1000]);
    let (top, left, bottom, right) = fixture.bounds;
    ppc_write_gworld_port(memory, PORT, PIXMAP_HANDLE, top, left, bottom, right).unwrap();
    memory.write_u32_be(PIXMAP_HANDLE, PIXMAP).unwrap();
    ppc_write_pixmap(
        memory,
        PIXMAP,
        PIXELS,
        u32::from(fixture.row_bytes),
        top,
        left,
        bottom,
        right,
        depth,
    )
    .unwrap();
    memory.write_bytes(PIXELS, fixture.pixels).unwrap();
    for (index, (bbox, data)) in [fixture.vis, fixture.clip].into_iter().enumerate() {
        memory
            .write_bytes(REGION_STORAGE[index], &region_storage_bytes(bbox, data))
            .unwrap();
        memory
            .write_u32_be(REGION_HANDLES[index], REGION_STORAGE[index])
            .unwrap();
    }
    memory
        .write_u32_be(PORT + PPC_CGRAF_PORT_VIS_RGN_OFFSET, REGION_HANDLES[0])
        .unwrap();
    memory
        .write_u32_be(PORT + PPC_CGRAF_PORT_CLIP_RGN_OFFSET, REGION_HANDLES[1])
        .unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PORT,
        pixmap_handle: PIXMAP_HANDLE,
        pixmap: PIXMAP,
        base_addr: PIXELS,
        gdevice: PPC_MAIN_GDEVICE,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
        depth,
        row_bytes: u32::from(fixture.row_bytes),
        pixels_locked: false,
        pixels_no_purge: false,
        private_gdevice: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PORT);
    loaded.quickdraw_fore_indices.insert(PORT, fixture.fg_index);
    loaded
        .toolbox_startup
        .quickdraw_back_indices
        .insert(PORT, fixture.bg_index);
}

fn read_pixels(loaded: &mut PpcLoadedApp, len: usize) -> Vec<u8> {
    let mut out = vec![0; len];
    loaded.memory.read_bytes_into(PIXELS, &mut out).unwrap();
    out
}

/// Runs the FillRect import on `fixture` and returns the pixel bytes.
fn fill_rect_ppc(
    fixture: &IndexedPortFixture<'_>,
    depth: u32,
    (top, left, bottom, right): (i16, i16, i16, i16),
    pattern: [u8; 8],
) -> Vec<u8> {
    fill_rect_ppc_route(fixture, depth, (top, left, bottom, right), pattern, None).0
}

fn fill_rect_ppc_route(
    fixture: &IndexedPortFixture<'_>,
    depth: u32,
    (top, left, bottom, right): (i16, i16, i16, i16),
    pattern: [u8; 8],
    gpu_failure: Option<u8>,
) -> (Vec<u8>, usize) {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FillRect")).unwrap();
    install_port(&mut loaded, fixture, depth);
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    if let Some(failure) = gpu_failure {
        loaded.memory.shared_view().set_quickdraw_gpu_renderer(
            crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                crate::quickdraw::gpu::ReferenceRenderer {
                    calls: calls.clone(),
                    failure,
                },
            )),
        );
    }

    ppc_write_rect(&mut loaded.memory, RECT, top, left, bottom, right).unwrap();
    loaded.memory.write_bytes(PATTERN, &pattern).unwrap();
    loaded.cpu.gpr[3] = RECT;
    loaded.cpu.gpr[4] = PATTERN;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FillRect);
    (read_pixels(&mut loaded, fixture.pixels.len()), calls.get())
}

const RECTS: [(i16, i16, i16, i16); 4] = [
    (-6, -4, 5, 30),
    (0, 0, 1, 1),
    (2, -1, 8, 13),
    (-3, -5, 9, 19),
];
const CLIPS: [(RegionSpec, RegionSpec); 4] = [
    (WHOLE_PORT, WHOLE_PORT),
    (WHOLE_PORT, CLIP_RECT),
    (WHOLE_PORT, CLIP_WITH_HOLE),
    (VIS_NOTCHED, CLIP_WITH_HOLE),
];

#[test]
fn gpu_native_polygon_fills_match_concave_crossed_translated_and_degenerate_shapes() {
    let polygons: Vec<Vec<(i16, i16)>> = vec![
        vec![(-4, -2), (17, -2), (-4, 8)],
        vec![(-4, -2), (17, 8), (-4, 8), (17, -2)],
        vec![(-5, -3), (18, -3), (18, 2), (5, 2), (5, 9), (-5, 9)],
        vec![(-4, 1), (6, 1), (6, 1), (17, 8), (-4, 8), (-4, 1)],
        vec![(-4, 0), (5, 0), (17, 0)],
        vec![
            (i16::MIN, i16::MIN),
            (i16::MAX, i16::MAX),
            (i16::MIN, i16::MAX),
        ],
        vec![(-100, -100), (-90, -100), (-100, -90)],
    ];
    for depth in [1, 2, 4, 8, 16] {
        for points in &polygons {
            for reversed in [false, true] {
                for op in 0..3 {
                    let mut expected = None;
                    for backend in [None, Some(0), Some(1), Some(2)] {
                        let pixels = initial_pixels();
                        let fixture = fixture(&pixels, CLIPS[3].0, CLIPS[3].1);
                        let mut loaded =
                            load_pef_application(&synthetic_pef_with_import(b"FillPoly")).unwrap();
                        install_port(&mut loaded, &fixture, depth);
                        let points: Vec<_> = if reversed {
                            points.iter().rev().copied().collect()
                        } else {
                            points.clone()
                        };
                        let handle = SCRATCH + 0x500;
                        let storage = SCRATCH + 0x520;
                        loaded.memory.write_u32_be(handle, storage).unwrap();
                        loaded
                            .memory
                            .write_u16_be(storage, (10 + points.len() * 4) as u16)
                            .unwrap();
                        let top = points.iter().map(|p| p.1).min().unwrap();
                        let bottom = points.iter().map(|p| p.1).max().unwrap();
                        let left = points.iter().map(|p| p.0).min().unwrap();
                        let right = points.iter().map(|p| p.0).max().unwrap();
                        let (top, left, bottom, right) = if points.iter().any(|p| p.0 == i16::MIN) {
                            BOUNDS
                        } else {
                            (top, left, bottom, right)
                        };
                        ppc_write_rect(&mut loaded.memory, storage + 2, top, left, bottom, right)
                            .unwrap();
                        for (i, (x, y)) in points.iter().enumerate() {
                            loaded
                                .memory
                                .write_u16_be(storage + 10 + i as u32 * 4, *y as u16)
                                .unwrap();
                            loaded
                                .memory
                                .write_u16_be(storage + 12 + i as u32 * 4, *x as u16)
                                .unwrap();
                        }
                        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                        if let Some(failure) = backend {
                            loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                    crate::quickdraw::gpu::ReferenceRenderer {
                                        calls: calls.clone(),
                                        failure,
                                    },
                                )),
                            );
                        }
                        let wrote = if op == 0 {
                            ppc_paint_polygon(
                                &mut loaded.memory,
                                &loaded.gworlds,
                                PORT,
                                handle,
                                PPC_RGB_BLACK,
                                Some(fixture.fg_index),
                            )
                        } else {
                            ppc_fill_polygon_pattern(
                                &mut loaded.memory,
                                &loaded.gworlds,
                                PORT,
                                handle,
                                if op == 1 { MIXED } else { [255; 8] },
                                PPC_RGB_BLACK,
                                Some(fixture.fg_index),
                                PPC_RGB_WHITE,
                                Some(fixture.bg_index),
                            )
                        };
                        if backend.is_some() && wrote {
                            assert_eq!(calls.get(), 1);
                        }
                        let actual = (wrote, read_pixels(&mut loaded, pixels.len()));
                        if let Some(expected) = &expected {
                            assert_eq!(
                                &actual, expected,
                                "depth={depth},points={points:?},op={op},backend={backend:?}"
                            );
                        } else {
                            expected = Some(actual);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn gpu_native_glyph_writes_preserve_retained_outline_presentation() {
    use crate::memory::{MacMemoryBus, MemoryBus};
    let pixels = vec![255; usize::from(ROW_BYTES) * HEIGHT];
    for depth in [8, 16] {
        let mut outputs = Vec::new();
        for gpu in [false, true] {
            let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
            let mut loaded =
                load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
            install_port(&mut loaded, &fixture, depth);
            loaded.memory.add_region(0x1000, pixels.clone());
            loaded.memory.write_u32_be(PIXMAP, 0x1000).unwrap();
            loaded.gworlds.last_mut().unwrap().base_addr = 0x1000;
            let mut bus = MacMemoryBus::new(1024 * 1024);
            bus.write_bytes(0x1000, &pixels);
            bus.enable_outline_presentation(
                (
                    0x1000,
                    u32::from(ROW_BYTES),
                    24,
                    HEIGHT as u16,
                    depth as u16,
                ),
                std::array::from_fn(|i| [i as u8; 3]),
                3,
            );
            loaded
                .memory
                .shared_view()
                .set_presentation(bus.presentation.clone());
            if gpu {
                loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                    crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                        crate::quickdraw::gpu::ReferenceRenderer {
                            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
                            failure: 0,
                        },
                    )),
                );
            }
            ppc_draw_text_bytes(
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                (-4, 7),
                PPC_QD_TEXT_FONT_DEFAULT,
                12,
                1,
                PPC_RGB_BLACK,
                Some(0),
                b"Ag",
            );
            assert!(
                bus.has_visible_outline_detail(),
                "native text must retain outline samples"
            );
            let mut bytes = vec![0; pixels.len()];
            loaded.memory.read_bytes_into(0x1000, &mut bytes).unwrap();
            outputs.push((bytes, bus.outline_presentation_rgb()));
        }
        assert_eq!(outputs[1], outputs[0], "depth={depth}");
    }
}

#[test]
fn gpu_native_scaled_copies_preserve_retained_outline_presentation() {
    use crate::memory::{MacMemoryBus, MemoryBus};
    let pixels = vec![255; usize::from(ROW_BYTES) * HEIGHT];
    for depth in [8, 16] {
        let mut outputs = Vec::new();
        for gpu in [false, true] {
            let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
            let mut loaded =
                load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
            install_port(&mut loaded, &fixture, depth);
            loaded.memory.add_region(0x1000, pixels.clone());
            loaded.memory.write_u32_be(PIXMAP, 0x1000).unwrap();
            loaded.gworlds.last_mut().unwrap().base_addr = 0x1000;
            let mut bus = MacMemoryBus::new(1024 * 1024);
            bus.write_bytes(0x1000, &pixels);
            bus.enable_outline_presentation(
                (
                    0x1000,
                    u32::from(ROW_BYTES),
                    24,
                    HEIGHT as u16,
                    depth as u16,
                ),
                std::array::from_fn(|i| [i as u8; 3]),
                3,
            );
            loaded
                .memory
                .shared_view()
                .set_presentation(bus.presentation.clone());
            if gpu {
                loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                    crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                        crate::quickdraw::gpu::ReferenceRenderer {
                            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
                            failure: 0,
                        },
                    )),
                );
            }
            ppc_draw_text_bytes(
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                (-4, 7),
                PPC_QD_TEXT_FONT_DEFAULT,
                12,
                1,
                PPC_RGB_BLACK,
                Some(0),
                b"Ag",
            );
            assert!(
                bus.has_visible_outline_detail(),
                "native text must retain outline samples"
            );
            ppc_write_rect(
                &mut loaded.memory,
                RECT,
                BOUNDS.0,
                BOUNDS.1,
                BOUNDS.2,
                BOUNDS.3,
            )
            .unwrap();
            ppc_write_rect(&mut loaded.memory, RECT + 0x20, 0, 0, 8, 18).unwrap();
            loaded.cpu.gpr[3] = PIXMAP;
            loaded.cpu.gpr[4] = PIXMAP;
            loaded.cpu.gpr[5] = RECT;
            loaded.cpu.gpr[6] = RECT + 0x20;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            let clut = std::array::from_fn(|i| [i as u16 * 257; 3]);
            assert!(ppc_copy_bits(
                &mut loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                0,
                &loaded.toolbox_startup,
                &clut,
                PPC_RGB_BLACK,
                Some(0),
                PPC_RGB_WHITE,
                Some(255),
                PPC_RGB_BLACK
            ));
            let mut bytes = vec![0; pixels.len()];
            loaded.memory.read_bytes_into(0x1000, &mut bytes).unwrap();
            outputs.push((bytes, bus.outline_presentation_rgb()));
        }
        assert_eq!(outputs[1], outputs[0], "depth={depth}");
    }
}

#[test]
fn gpu_native_masked_copies_preserve_retained_outline_presentation() {
    use crate::memory::{MacMemoryBus, MemoryBus};
    let pixels = vec![255; usize::from(ROW_BYTES) * HEIGHT];
    for depth in [8, 16] {
        let mut outputs = Vec::new();
        for gpu in [false, true] {
            let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
            let mut loaded =
                load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
            install_port(&mut loaded, &fixture, depth);
            loaded.memory.add_region(0x1000, pixels.clone());
            loaded.memory.write_u32_be(PIXMAP, 0x1000).unwrap();
            loaded.gworlds.last_mut().unwrap().base_addr = 0x1000;
            let mut bus = MacMemoryBus::new(1024 * 1024);
            bus.write_bytes(0x1000, &pixels);
            bus.enable_outline_presentation(
                (
                    0x1000,
                    u32::from(ROW_BYTES),
                    24,
                    HEIGHT as u16,
                    depth as u16,
                ),
                std::array::from_fn(|i| [i as u8; 3]),
                3,
            );
            loaded
                .memory
                .shared_view()
                .set_presentation(bus.presentation.clone());
            if gpu {
                loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                    crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                        crate::quickdraw::gpu::ReferenceRenderer {
                            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
                            failure: 0,
                        },
                    )),
                );
            }
            ppc_draw_text_bytes(
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                (-4, 7),
                PPC_QD_TEXT_FONT_DEFAULT,
                12,
                1,
                PPC_RGB_BLACK,
                Some(0),
                b"Ag",
            );
            assert!(
                bus.has_visible_outline_detail(),
                "native text must retain outline samples"
            );
            ppc_write_rect(
                &mut loaded.memory,
                RECT,
                BOUNDS.0,
                BOUNDS.1,
                BOUNDS.2,
                BOUNDS.3,
            )
            .unwrap();
            ppc_write_rect(&mut loaded.memory, RECT + 0x20, 0, 0, 8, 18).unwrap();
            loaded.cpu.gpr[3] = PIXMAP;
            loaded.cpu.gpr[4] = PIXMAP;
            loaded.cpu.gpr[5] = RECT;
            loaded.cpu.gpr[6] = RECT + 0x20;
            loaded.cpu.gpr[7] = 36;
            let storage = region_storage_bytes(
                (0, 0, 8, 18),
                &[0, 0, 5, 9, 18, i16::MAX, 8, 0, 5, 9, 18, i16::MAX, i16::MAX],
            );
            loaded
                .memory
                .write_u32_be(SCRATCH + 0x500, SCRATCH + 0x520)
                .unwrap();
            loaded
                .memory
                .write_bytes(SCRATCH + 0x520, &storage)
                .unwrap();
            loaded.cpu.gpr[8] = SCRATCH + 0x500;
            let clut = std::array::from_fn(|i| [i as u16 * 257; 3]);
            assert!(ppc_copy_bits(
                &mut loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                0,
                &loaded.toolbox_startup,
                &clut,
                PPC_RGB_BLACK,
                Some(0),
                PPC_RGB_WHITE,
                Some(255),
                PPC_RGB_BLACK
            ));
            let mut bytes = vec![0; pixels.len()];
            loaded.memory.read_bytes_into(0x1000, &mut bytes).unwrap();
            outputs.push((bytes, bus.outline_presentation_rgb()));
        }
        assert_eq!(outputs[1], outputs[0], "depth={depth}");
    }
}

#[test]
fn gpu_native_arithmetic_copies_clear_outline_detail_like_software() {
    use crate::memory::{MacMemoryBus, MemoryBus};
    let pixels = vec![255; usize::from(ROW_BYTES) * HEIGHT];
    for depth in [8, 16] {
        let mut outputs = Vec::new();
        for gpu in [false, true] {
            let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
            let mut loaded =
                load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
            install_port(&mut loaded, &fixture, depth);
            loaded.memory.add_region(0x1000, pixels.clone());
            loaded.memory.write_u32_be(PIXMAP, 0x1000).unwrap();
            loaded.gworlds.last_mut().unwrap().base_addr = 0x1000;
            let mut bus = MacMemoryBus::new(1024 * 1024);
            bus.write_bytes(0x1000, &pixels);
            bus.enable_outline_presentation(
                (
                    0x1000,
                    u32::from(ROW_BYTES),
                    24,
                    HEIGHT as u16,
                    depth as u16,
                ),
                std::array::from_fn(|i| [i as u8; 3]),
                3,
            );
            loaded
                .memory
                .shared_view()
                .set_presentation(bus.presentation.clone());
            if gpu {
                loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                    crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                        crate::quickdraw::gpu::ReferenceRenderer {
                            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
                            failure: 0,
                        },
                    )),
                );
            }
            ppc_draw_text_bytes(
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                (-4, 7),
                PPC_QD_TEXT_FONT_DEFAULT,
                12,
                1,
                PPC_RGB_BLACK,
                Some(0),
                b"Ag",
            );
            assert!(
                bus.has_visible_outline_detail(),
                "native text must retain outline samples"
            );
            ppc_write_rect(
                &mut loaded.memory,
                RECT,
                BOUNDS.0,
                BOUNDS.1,
                BOUNDS.2,
                BOUNDS.3,
            )
            .unwrap();
            ppc_write_rect(&mut loaded.memory, RECT + 0x20, 0, 0, 8, 18).unwrap();
            loaded.cpu.gpr[3] = PIXMAP;
            loaded.cpu.gpr[4] = PIXMAP;
            loaded.cpu.gpr[5] = RECT;
            loaded.cpu.gpr[6] = RECT + 0x20;
            loaded.cpu.gpr[7] = 32;
            let storage = region_storage_bytes(
                (0, 0, 8, 18),
                &[0, 0, 5, 9, 18, i16::MAX, 8, 0, 5, 9, 18, i16::MAX, i16::MAX],
            );
            loaded
                .memory
                .write_u32_be(SCRATCH + 0x500, SCRATCH + 0x520)
                .unwrap();
            loaded
                .memory
                .write_bytes(SCRATCH + 0x520, &storage)
                .unwrap();
            loaded.cpu.gpr[8] = SCRATCH + 0x500;
            let clut = std::array::from_fn(|i| [i as u16 * 257; 3]);
            assert!(ppc_copy_bits(
                &mut loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                0,
                &loaded.toolbox_startup,
                &clut,
                PPC_RGB_BLACK,
                Some(0),
                PPC_RGB_WHITE,
                Some(255),
                PPC_RGB_BLACK
            ));
            let mut bytes = vec![0; pixels.len()];
            loaded.memory.read_bytes_into(0x1000, &mut bytes).unwrap();
            outputs.push((bytes, bus.outline_presentation_rgb()));
        }
        assert_eq!(outputs[1], outputs[0], "depth={depth}");
    }
}

#[test]
fn gpu_native_deep_masks_clear_outline_detail_with_white_color_masks() {
    use crate::memory::{MacMemoryBus, MemoryBus};
    let pixels = vec![255; usize::from(ROW_BYTES) * HEIGHT];
    for depth in [8, 16] {
        let mut outputs = Vec::new();
        for gpu in [false, true] {
            let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
            let mut loaded =
                load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
            install_port(&mut loaded, &fixture, depth);
            loaded.memory.add_region(0x1000, pixels.clone());
            loaded.memory.write_u32_be(PIXMAP, 0x1000).unwrap();
            loaded.gworlds.last_mut().unwrap().base_addr = 0x1000;
            let mut bus = MacMemoryBus::new(1024 * 1024);
            bus.write_bytes(0x1000, &pixels);
            bus.enable_outline_presentation(
                (
                    0x1000,
                    u32::from(ROW_BYTES),
                    24,
                    HEIGHT as u16,
                    depth as u16,
                ),
                std::array::from_fn(|i| [i as u8; 3]),
                3,
            );
            loaded
                .memory
                .shared_view()
                .set_presentation(bus.presentation.clone());
            if gpu {
                loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                    crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                        crate::quickdraw::gpu::ReferenceRenderer {
                            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
                            failure: 0,
                        },
                    )),
                );
            }
            ppc_draw_text_bytes(
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                (-4, 7),
                PPC_QD_TEXT_FONT_DEFAULT,
                12,
                1,
                PPC_RGB_BLACK,
                Some(0),
                b"Ag",
            );
            assert!(
                bus.has_visible_outline_detail(),
                "native text must retain outline samples"
            );
            ppc_write_rect(
                &mut loaded.memory,
                RECT,
                BOUNDS.0,
                BOUNDS.1,
                BOUNDS.2,
                BOUNDS.3,
            )
            .unwrap();
            ppc_write_rect(&mut loaded.memory, RECT + 0x20, 0, 0, 8, 18).unwrap();
            loaded.memory.add_region(0x3000, vec![255; pixels.len()]);
            let mask = SCRATCH + 0x600;
            ppc_write_pixmap(
                &mut loaded.memory,
                mask,
                0x3000,
                u32::from(ROW_BYTES),
                BOUNDS.0,
                BOUNDS.1,
                BOUNDS.2,
                BOUNDS.3,
                16,
            )
            .unwrap();
            loaded.cpu.gpr[3] = PIXMAP;
            loaded.cpu.gpr[4] = mask;
            loaded.cpu.gpr[5] = PIXMAP;
            loaded.cpu.gpr[6] = RECT;
            loaded.cpu.gpr[7] = RECT;
            loaded.cpu.gpr[8] = RECT + 0x20;
            loaded.cpu.gpr[9] = 0;
            loaded.cpu.gpr[10] = 0;
            let clut = std::array::from_fn(|i| [i as u16 * 257; 3]);
            assert!(ppc_copy_deep_mask(
                &loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                &clut
            ));
            let mut bytes = vec![0; pixels.len()];
            loaded.memory.read_bytes_into(0x1000, &mut bytes).unwrap();
            outputs.push((bytes, bus.outline_presentation_rgb()));
        }
        assert_eq!(outputs[1], outputs[0], "depth={depth}");
    }
}

#[test]
fn gpu_native_glyph_masks_match_software_text_modes_and_explicit_clips() {
    let pixels = initial_pixels();
    for depth in [1, 2, 4, 8, 16] {
        for (vis, clip) in [CLIPS[0], CLIPS[3]] {
            let fixture = fixture(&pixels, vis, clip);
            for mode in [0, 1, 2, 3] {
                for extra_clip in [None, Some((0, -1, 5, 10))] {
                    let mut outputs = Vec::new();
                    for gpu in [false, true] {
                        let mut loaded =
                            load_pef_application(&synthetic_pef_with_import(b"DrawString"))
                                .unwrap();
                        install_port(&mut loaded, &fixture, depth);
                        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                        if gpu {
                            loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                    crate::quickdraw::gpu::ReferenceRenderer {
                                        calls: calls.clone(),
                                        failure: 0,
                                    },
                                )),
                            );
                        }
                        ppc_draw_text_bytes(
                            &mut loaded.memory,
                            &loaded.gworlds,
                            PORT,
                            (-4, 7),
                            PPC_QD_TEXT_FONT_DEFAULT,
                            12,
                            mode,
                            PPC_RGB_WHITE,
                            Some(fixture.fg_index),
                            b"Ag",
                        );
                        // Exercise an explicit clip through the same unstyled text path.
                        ppc_draw_text_bytes_clipped(
                            &mut loaded.memory,
                            &loaded.gworlds,
                            PORT,
                            (3, 7),
                            PPC_QD_TEXT_FONT_DEFAULT,
                            12,
                            mode,
                            PPC_RGB_WHITE,
                            Some(fixture.fg_index),
                            extra_clip,
                            b"A",
                        );
                        if gpu {
                            assert!(
                                calls.get() > 0,
                                "native text must reach the GPU at depth {depth}"
                            );
                        }
                        outputs.push(read_pixels(&mut loaded, pixels.len()));
                    }
                    assert_eq!(
                        outputs[1], outputs[0],
                        "depth={depth},mode={mode},extra_clip={extra_clip:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn gpu_native_lines_match_software_octants_pens_clips_and_fallback() {
    let pixels = initial_pixels();
    for depth in [1, 2, 4, 8, 16] {
        for (vis, clip) in [CLIPS[0], CLIPS[3]] {
            let fixture = fixture(&pixels, vis, clip);
            for to in [
                (-1, 3),
                (-8, 3),
                (25, 3),
                (-1, -8),
                (-1, 14),
                (-12, -3),
                (-5, -7),
                (10, -3),
                (3, -7),
                (-12, 9),
                (-5, 13),
                (10, 9),
                (3, 13),
            ] {
                for (pen, hidden) in [
                    ((0, 0), false),
                    ((1, 1), false),
                    ((3, 2), false),
                    ((2, 4), false),
                    ((3, 2), true),
                ] {
                    let mut expected = None;
                    for backend in [None, Some(0), Some(1), Some(2)] {
                        let mut loaded =
                            load_pef_application(&synthetic_pef_with_import(b"LineTo")).unwrap();
                        install_port(&mut loaded, &fixture, depth);
                        loaded
                            .memory
                            .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET, pen.1)
                            .unwrap();
                        loaded
                            .memory
                            .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, pen.0)
                            .unwrap();
                        loaded
                            .memory
                            .write_u16_be(
                                PORT + PPC_CGRAF_PORT_PN_VIS_OFFSET,
                                if hidden { 65535 } else { 0 },
                            )
                            .unwrap();
                        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                        if let Some(failure) = backend {
                            loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                    crate::quickdraw::gpu::ReferenceRenderer {
                                        calls: calls.clone(),
                                        failure,
                                    },
                                )),
                            );
                        }
                        let wrote = ppc_line_to(
                            &mut loaded.memory,
                            &loaded.gworlds,
                            PORT,
                            (-1, 3),
                            to,
                            PPC_RGB_WHITE,
                            Some(fixture.fg_index),
                        );
                        if backend.is_some() && wrote {
                            assert_eq!(calls.get(), 1);
                        }
                        if hidden || pen.0 == 0 || pen.1 == 0 {
                            assert_eq!(calls.get(), 0);
                        }
                        let result = (wrote, read_pixels(&mut loaded, pixels.len()));
                        if let Some(expected) = &expected {
                            assert_eq!(&result,expected,"depth={depth},to={to:?},pen={pen:?},hidden={hidden},backend={backend:?}");
                        } else {
                            expected = Some(result);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn gpu_native_small_region_frames_stay_accelerated_on_large_surfaces() {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, CLIPS[0].0, CLIPS[0].1);
    let mut expected = None;
    for gpu in [false, true] {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FrameRgn")).unwrap();
        install_port(&mut loaded, &fixture, 8);
        let base = 0x30000000;
        loaded.memory.add_region(base, vec![0xa5; 4003 * 12]);
        ppc_write_pixmap(&mut loaded.memory, PIXMAP, base, 4003, 0, 0, 3000, 4000, 8).unwrap();
        let record = loaded.gworlds.iter_mut().find(|r| r.port == PORT).unwrap();
        record.base_addr = base;
        record.width = 4000;
        record.height = 3000;
        record.row_bytes = 4003;
        let handle = SCRATCH + 0x500;
        let storage = SCRATCH + 0x520;
        loaded.memory.write_u32_be(handle, storage).unwrap();
        loaded
            .memory
            .write_bytes(storage, &region_storage_bytes((0, 0, 10, 16), &[]))
            .unwrap();
        loaded
            .memory
            .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 8)
            .unwrap();
        loaded
            .memory
            .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, 5)
            .unwrap();
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        if gpu {
            loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                    crate::quickdraw::gpu::ReferenceRenderer {
                        calls: calls.clone(),
                        failure: 0,
                    },
                )),
            );
        }
        assert!(ppc_frame_region(
            &mut loaded.memory,
            &loaded.gworlds,
            PORT,
            handle,
            PPC_RGB_WHITE,
            Some(103)
        ));
        assert_eq!(calls.get(), usize::from(gpu));
        let mut result = vec![0; 4003 * 12];
        loaded.memory.read_bytes_into(base, &mut result).unwrap();
        if let Some(expected) = &expected {
            assert_eq!(&result, expected);
        } else {
            expected = Some(result);
        }
    }
}

#[test]
fn gpu_native_regions_match_software_paint_frame_patterns_and_fallback() {
    let pixels = initial_pixels();
    for depth in [1, 2, 4, 8, 16] {
        for (vis, clip) in [CLIPS[0], CLIPS[3]] {
            let fixture = fixture(&pixels, vis, clip);
            for region in [
                WHOLE_PORT,
                CLIP_WITH_HOLE,
                ((20, 20, 24, 27), &[]),
                ((-20, -20, -17, -14), &[]),
            ] {
                for (op, pen, pattern) in [
                    (0, (1, 1), [255; 8]),
                    (1, (0, 0), [255; 8]),
                    (1, (2, 3), [255; 8]),
                    (1, (8, 5), [255; 8]),
                    (2, (1, 1), [255; 8]),
                    (2, (1, 1), [0; 8]),
                    (2, (1, 1), DIAGONAL),
                ] {
                    let mut expected = None;
                    for backend in [None, Some(0), Some(1), Some(2)] {
                        let mut loaded =
                            load_pef_application(&synthetic_pef_with_import(b"PaintRgn")).unwrap();
                        install_port(&mut loaded, &fixture, depth);
                        let handle = SCRATCH + 0x500;
                        let storage = SCRATCH + 0x520;
                        loaded.memory.write_u32_be(handle, storage).unwrap();
                        loaded
                            .memory
                            .write_bytes(storage, &region_storage_bytes(region.0, region.1))
                            .unwrap();
                        loaded
                            .memory
                            .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET, pen.0)
                            .unwrap();
                        loaded
                            .memory
                            .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, pen.1)
                            .unwrap();
                        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                        if let Some(failure) = backend {
                            loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                    crate::quickdraw::gpu::ReferenceRenderer {
                                        calls: calls.clone(),
                                        failure,
                                    },
                                )),
                            );
                        }
                        let wrote = match op {
                            0 => ppc_paint_region(
                                &mut loaded.memory,
                                &loaded.gworlds,
                                PORT,
                                handle,
                                PPC_RGB_WHITE,
                                Some(fixture.fg_index),
                            ),
                            1 => ppc_frame_region(
                                &mut loaded.memory,
                                &loaded.gworlds,
                                PORT,
                                handle,
                                PPC_RGB_WHITE,
                                Some(fixture.fg_index),
                            ),
                            _ => ppc_fill_region_with_mono_pattern(
                                &mut loaded.memory,
                                &loaded.gworlds,
                                PORT,
                                handle,
                                Some(pattern),
                                PPC_RGB_WHITE,
                                Some(fixture.fg_index),
                                PPC_RGB_BLACK,
                                Some(fixture.bg_index),
                            ),
                        };
                        if backend.is_some() && wrote {
                            assert_eq!(calls.get(), 1, "region must reach GPU once");
                        }
                        let result = (wrote, read_pixels(&mut loaded, pixels.len()));
                        if let Some(expected) = &expected {
                            assert_eq!(&result,expected,"depth={depth},op={op},pen={pen:?},pattern={pattern:?},backend={backend:?}");
                        } else {
                            expected = Some(result);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn gpu_native_arc_paints_match_software_angles_depths_and_fallback() {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, CLIPS[0].0, CLIPS[0].1);
    for depth in [1, 2, 4, 8, 16] {
        for rect in [RECTS[0], RECTS[1], RECTS[3]] {
            for (start, extent) in [
                (0, 0),
                (0, 90),
                (90, -180),
                (-45, 270),
                (320, 55),
                (0, 360),
                (65, 900),
            ] {
                let mut expected = None;
                for backend in [None, Some(0), Some(1), Some(2)] {
                    let mut loaded =
                        load_pef_application(&synthetic_pef_with_import(b"PaintArc")).unwrap();
                    install_port(&mut loaded, &fixture, depth);
                    ppc_write_rect(&mut loaded.memory, RECT, rect.0, rect.1, rect.2, rect.3)
                        .unwrap();
                    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                    if let Some(failure) = backend {
                        loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                            crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                crate::quickdraw::gpu::ReferenceRenderer {
                                    calls: calls.clone(),
                                    failure,
                                },
                            )),
                        );
                    }
                    let wrote = ppc_paint_arc(
                        &mut loaded.memory,
                        &loaded.gworlds,
                        PORT,
                        RECT,
                        start,
                        extent,
                        PPC_RGB_WHITE,
                        Some(fixture.fg_index),
                    );
                    if backend.is_some() && wrote {
                        assert_eq!(calls.get(), 1);
                    }
                    let result = (wrote, read_pixels(&mut loaded, pixels.len()));
                    if let Some(expected) = &expected {
                        assert_eq!(&result,expected,"depth={depth},rect={rect:?},start={start},extent={extent},backend={backend:?}");
                    } else {
                        expected = Some(result);
                    }
                }
            }
        }
    }
}

#[test]
fn gpu_native_scaled_text_matches_repeated_transfers_and_fallback() {
    // A dense 4x4 strike guarantees four source writes per destination pixel
    // when halved. Register a private family so nearby strikes cannot mask scaling.
    let family = 31079;
    let mut strike = vec![0u8; 44];
    for (at, value) in [
        (2, 65u16),
        (4, 65),
        (6, 4),
        (12, 8),
        (14, 4),
        (16, 12),
        (18, 4),
        (24, 1),
        (36, 4),
        (38, 8),
        (40, 4),
        (42, 4),
    ] {
        strike[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    for row in 0..4 {
        strike[26 + row * 2] = 255;
    }
    assert!(crate::quickdraw::fonts::register_resource_font_strike_for_family(family, 12, &strike));
    let pixels = initial_pixels();
    for depth in [1, 2, 4, 8, 16] {
        for (vis, clip) in [CLIPS[0], CLIPS[3]] {
            let fixture = fixture(&pixels, vis, clip);
            for size in [6, 9, 18, 24] {
                let (_, n, d) = get_font_face_scale_ratio(family, size);
                assert_ne!(n, d);
                for style in [0, 1, 4, 8, 31] {
                    for mode in [0, 1, 2, 3] {
                        let mut expected = None;
                        for backend in [None, Some(0), Some(1), Some(2)] {
                            let mut loaded =
                                load_pef_application(&synthetic_pef_with_import(b"DrawString"))
                                    .unwrap();
                            install_port(&mut loaded, &fixture, depth);
                            let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                            if let Some(failure) = backend {
                                loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                    crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                        crate::quickdraw::gpu::ReferenceRenderer {
                                            calls: calls.clone(),
                                            failure,
                                        },
                                    )),
                                );
                            }
                            let advance = ppc_draw_text_bytes_styled_clipped(
                                &mut loaded.memory,
                                &loaded.gworlds,
                                PORT,
                                (-2, 6),
                                family,
                                size,
                                mode,
                                PPC_RGB_WHITE,
                                Some(fixture.fg_index),
                                style,
                                None,
                                b"AAA",
                            );
                            if backend.is_some() {
                                assert!(calls.get() > 0, "scaled text must reach GPU: depth={depth},size={size},style={style},mode={mode}");
                            }
                            let result = (advance, read_pixels(&mut loaded, pixels.len()));
                            if let Some(expected) = &expected {
                                assert_eq!(&result, expected, "depth={depth},size={size},style={style},mode={mode},backend={backend:?}");
                            } else {
                                expected = Some(result);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn gpu_native_styled_text_matches_software_transfers_clips_and_fallback() {
    let pixels = initial_pixels();
    for depth in [1, 2, 4, 8, 16] {
        for (vis, clip) in [CLIPS[0], CLIPS[3]] {
            let fixture = fixture(&pixels, vis, clip);
            for style in [1, 2, 4, 8, 16, 31] {
                for mode in [0, 1, 2, 3] {
                    let mut expected = None;
                    for backend in [None, Some(0), Some(1), Some(2)] {
                        let mut loaded =
                            load_pef_application(&synthetic_pef_with_import(b"DrawString"))
                                .unwrap();
                        install_port(&mut loaded, &fixture, depth);
                        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                        if let Some(failure) = backend {
                            loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                    crate::quickdraw::gpu::ReferenceRenderer {
                                        calls: calls.clone(),
                                        failure,
                                    },
                                )),
                            );
                        }
                        let advance = ppc_draw_text_bytes_styled_clipped(
                            &mut loaded.memory,
                            &loaded.gworlds,
                            PORT,
                            (-4, 7),
                            PPC_QD_TEXT_FONT_DEFAULT,
                            12,
                            mode,
                            PPC_RGB_WHITE,
                            Some(fixture.fg_index),
                            style,
                            Some((-2, -3, 9, 12)),
                            b"Ag",
                        );
                        if backend.is_some() {
                            assert!(calls.get() > 0, "styled text must reach GPU: depth={depth}, style={style}, mode={mode}");
                        }
                        let result = (advance, read_pixels(&mut loaded, pixels.len()));
                        if let Some(expected) = &expected {
                            assert_eq!(
                                &result, expected,
                                "depth={depth},style={style},mode={mode},backend={backend:?}"
                            );
                        } else {
                            expected = Some(result);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn gpu_native_rounded_fills_and_frames_match_software_at_every_depth() {
    let pixels = initial_pixels();
    for depth in [1, 2, 4, 8, 16] {
        for (vis, clip) in [CLIPS[0], CLIPS[3]] {
            let fixture = fixture(&pixels, vis, clip);
            for rect in [RECTS[0], RECTS[3], RECTS[1]] {
                for frame in [false, true] {
                    for pen in [(1, 1), (2, 3), (0, 0)] {
                        for corner in [(11, 5), (0, 0)] {
                            let mut outputs = Vec::new();
                            for gpu in [false, true] {
                                let mut loaded = load_pef_application(&synthetic_pef_with_import(
                                    b"PaintRoundRect",
                                ))
                                .unwrap();
                                install_port(&mut loaded, &fixture, depth);
                                ppc_write_rect(
                                    &mut loaded.memory,
                                    RECT,
                                    rect.0,
                                    rect.1,
                                    rect.2,
                                    rect.3,
                                )
                                .unwrap();
                                loaded.cpu.gpr[3] = RECT;
                                loaded.cpu.gpr[4] = corner.0;
                                loaded.cpu.gpr[5] = corner.1;
                                loaded
                                    .memory
                                    .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET, pen.0)
                                    .unwrap();
                                loaded
                                    .memory
                                    .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, pen.1)
                                    .unwrap();
                                let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                                if gpu {
                                    loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                        crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                            crate::quickdraw::gpu::ReferenceRenderer {
                                                calls: calls.clone(),
                                                failure: 0,
                                            },
                                        )),
                                    );
                                }
                                let wrote = if frame {
                                    ppc_frame_round_rect(
                                        &loaded.cpu,
                                        &mut loaded.memory,
                                        &loaded.gworlds,
                                        PORT,
                                        PPC_RGB_BLACK,
                                        Some(fixture.fg_index),
                                    )
                                } else {
                                    ppc_paint_round_rect(
                                        &loaded.cpu,
                                        &mut loaded.memory,
                                        &loaded.gworlds,
                                        PORT,
                                        PPC_RGB_BLACK,
                                        Some(fixture.fg_index),
                                    )
                                };
                                assert_eq!(
                                    calls.get(),
                                    usize::from(
                                        gpu && wrote && !(frame && (pen.0 == 0 || pen.1 == 0))
                                    )
                                );
                                if rect == RECTS[3] && !frame {
                                    assert!(wrote);
                                }
                                outputs.push((wrote, read_pixels(&mut loaded, pixels.len())));
                            }
                            assert_eq!(outputs[1],outputs[0],"depth={depth},rect={rect:?},frame={frame},pen={pen:?},corner={corner:?}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn gpu_native_oval_fills_and_frames_match_software_at_every_depth() {
    let pixels = initial_pixels();
    for depth in [1, 2, 4, 8, 16] {
        for (vis, clip) in CLIPS {
            let fixture = fixture(&pixels, vis, clip);
            for rect in [
                RECTS[0],
                RECTS[3],
                (0, 0, 1, 1),
                (-20, -30, -10, -20),
                (20, 30, 25, 35),
            ] {
                for frame in [false, true] {
                    for pen in [(1, 1), (2, 3), (0, 0)] {
                        let mut outputs = Vec::new();
                        for gpu in [false, true] {
                            let mut loaded =
                                load_pef_application(&synthetic_pef_with_import(b"PaintOval"))
                                    .unwrap();
                            install_port(&mut loaded, &fixture, depth);
                            loaded
                                .memory
                                .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET, pen.0)
                                .unwrap();
                            loaded
                                .memory
                                .write_u16_be(PORT + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, pen.1)
                                .unwrap();
                            let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                            if gpu {
                                loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                    crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                        crate::quickdraw::gpu::ReferenceRenderer {
                                            calls: calls.clone(),
                                            failure: 0,
                                        },
                                    )),
                                );
                            }
                            let wrote = ppc_draw_oval_bounds(
                                &mut loaded.memory,
                                &loaded.gworlds,
                                PORT,
                                rect,
                                PPC_RGB_BLACK,
                                Some(fixture.fg_index),
                                frame,
                            );
                            assert_eq!(calls.get(), usize::from(gpu && wrote));
                            if rect == RECTS[3] && !frame {
                                assert!(wrote);
                            }
                            outputs.push((wrote, read_pixels(&mut loaded, pixels.len())));
                        }
                        assert_eq!(
                            outputs[1], outputs[0],
                            "depth={depth},rect={rect:?},frame={frame},pen={pen:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn gpu_native_fill_rect_patterns_match_software_and_error_fallback() {
    let pixels = initial_pixels();
    for depth in [8, 16] {
        for (vis, clip) in CLIPS {
            let fixture = fixture(&pixels, vis, clip);
            for rect in RECTS {
                for pattern in [[0xff; 8], [0; 8], GRAY, DIAGONAL, MIXED] {
                    let expected = fill_rect_ppc(&fixture, depth, rect, pattern);
                    let (actual, calls) =
                        fill_rect_ppc_route(&fixture, depth, rect, pattern, Some(0));
                    assert_eq!(
                        actual, expected,
                        "depth={depth},rect={rect:?},pattern={pattern:?}"
                    );
                    if rect == RECTS[3] {
                        assert_eq!(calls, 1);
                    }
                }
            }
        }
    }
    let fixture = fixture(&pixels, VIS_NOTCHED, CLIP_WITH_HOLE);
    let expected = fill_rect_ppc(&fixture, 16, RECTS[3], MIXED);
    for failure in [1, 2] {
        let (actual, calls) = fill_rect_ppc_route(&fixture, 16, RECTS[3], MIXED, Some(failure));
        assert_eq!(calls, 1);
        assert_eq!(actual, expected);
    }
}

#[test]
fn gpu_native_paint_patterns_keep_pixel_image_phase_at_every_depth() {
    let pixels = initial_pixels();
    for depth in [1, 2, 4, 8, 16] {
        for (vis, clip) in CLIPS {
            let fixture = fixture(&pixels, vis, clip);
            for rect in RECTS {
                for pattern in [GRAY, MIXED] {
                    let mut outputs = Vec::new();
                    for gpu in [false, true] {
                        let mut loaded =
                            load_pef_application(&synthetic_pef_with_import(b"PaintRect")).unwrap();
                        install_port(&mut loaded, &fixture, depth);
                        ppc_write_rect(&mut loaded.memory, RECT, rect.0, rect.1, rect.2, rect.3)
                            .unwrap();
                        loaded.cpu.gpr[3] = RECT;
                        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                        if gpu {
                            loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                    crate::quickdraw::gpu::ReferenceRenderer {
                                        calls: calls.clone(),
                                        failure: 0,
                                    },
                                )),
                            );
                        }
                        let wrote = ppc_paint_rect(
                            &loaded.cpu,
                            &mut loaded.memory,
                            &loaded.gworlds,
                            PORT,
                            PPC_RGB_BLACK,
                            Some(fixture.fg_index),
                            PPC_RGB_WHITE,
                            &pattern,
                        );
                        if rect == RECTS[3] {
                            assert!(wrote);
                            assert_eq!(calls.get(), usize::from(gpu));
                        }
                        outputs.push((wrote, read_pixels(&mut loaded, pixels.len())));
                    }
                    assert_eq!(
                        outputs[1], outputs[0],
                        "depth={depth},rect={rect:?},pattern={pattern:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn gpu_native_solid_fills_match_software_for_packed_pixels_and_complex_regions() {
    let pixels = initial_pixels();
    for depth in [1, 2, 4, 8, 16] {
        for (vis, clip) in CLIPS {
            let fixture = fixture(&pixels, vis, clip);
            for rect in RECTS {
                for respect_clip in [false, true] {
                    let mut outputs = Vec::new();
                    for gpu in [false, true] {
                        let mut loaded =
                            load_pef_application(&synthetic_pef_with_import(b"PaintRect")).unwrap();
                        install_port(&mut loaded, &fixture, depth);
                        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                        if gpu {
                            loaded.memory.shared_view().set_quickdraw_gpu_renderer(
                                crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(
                                    crate::quickdraw::gpu::ReferenceRenderer {
                                        calls: calls.clone(),
                                        failure: 0,
                                    },
                                )),
                            );
                        }
                        let wrote = if respect_clip {
                            ppc_paint_rect_bounds(
                                &mut loaded.memory,
                                &loaded.gworlds,
                                PORT,
                                rect,
                                PPC_RGB_BLACK,
                                Some(fixture.fg_index),
                            )
                        } else {
                            ppc_paint_window_background_bounds(
                                &mut loaded.memory,
                                &loaded.gworlds,
                                PORT,
                                rect,
                                PPC_RGB_BLACK,
                            )
                        };
                        if rect == RECTS[3] {
                            assert!(wrote, "whole-port fill must draw at depth {depth}");
                        }
                        assert_eq!(
                            calls.get(),
                            usize::from(gpu && wrote),
                            "depth={depth},rect={rect:?}"
                        );
                        outputs.push((wrote, read_pixels(&mut loaded, pixels.len())));
                    }
                    assert_eq!(outputs[1],outputs[0],"depth={depth},rect={rect:?},respect_clip={respect_clip},vis={:?},clip={:?}",vis.0,clip.0);
                }
            }
        }
    }
}

#[test]
fn fill_rect_matches_the_68k_fill_rect_path_on_an_8bpp_port() {
    let pixels = initial_pixels();
    let mut differing_from_solid = 0;
    for (vis, clip) in CLIPS {
        let fixture = fixture(&pixels, vis, clip);
        for rect in RECTS {
            let solid = fill_rect_8bpp_68k(&fixture, rect, [0xFF; 8]);
            for pattern in [[0xFF; 8], [0x00; 8], GRAY, DIAGONAL, MIXED] {
                let expected = fill_rect_8bpp_68k(&fixture, rect, pattern);
                let actual = fill_rect_ppc(&fixture, 8, rect, pattern);
                assert_eq!(
                    actual, expected,
                    "rect {rect:?} pattern {pattern:02x?} vis {:?} clip {:?}",
                    vis.0, clip.0
                );
                differing_from_solid += usize::from(expected != solid);
            }
        }
    }
    // The comparison is not vacuous: patterns other than solid black change
    // the result in nearly every case (a one-pixel rect can land on a
    // black bit).
    assert!(differing_from_solid >= 3 * 4 * 4, "{differing_from_solid}");
}

#[test]
fn solid_black_fill_rect_makes_the_same_writes_as_the_solid_fill() {
    let pixels = initial_pixels();
    for (vis, clip) in CLIPS {
        let fixture = fixture(&pixels, vis, clip);
        for depth in [8, 16] {
            for (top, left, bottom, right) in RECTS {
                let mut runs = Vec::new();
                for core in [false, true] {
                    let mut loaded =
                        load_pef_application(&synthetic_pef_with_import(b"FillRect")).unwrap();
                    install_port(&mut loaded, &fixture, depth);
                    let fore = (PPC_RGB_BLACK, Some(fixture.fg_index));
                    let back = (PPC_RGB_WHITE, Some(fixture.bg_index));
                    let rect = (top, left, bottom, right);
                    write_log::start();
                    if core {
                        ppc_fill_rect(
                            &mut loaded.memory,
                            &loaded.gworlds,
                            PORT,
                            rect,
                            [0xFF; 8],
                            fore,
                            back,
                        );
                    } else {
                        ppc_paint_rect_bounds(
                            &mut loaded.memory,
                            &loaded.gworlds,
                            PORT,
                            rect,
                            fore.0,
                            fore.1,
                        );
                    }
                    let writes = write_log::take();
                    runs.push((writes, read_pixels(&mut loaded, pixels.len())));
                }
                let context = format!(
                    "depth {depth} rect {:?} vis {:?} clip {:?}",
                    (top, left, bottom, right),
                    vis.0,
                    clip.0
                );
                assert_eq!(runs[1].1, runs[0].1, "pixels: {context}");
                assert_eq!(runs[1].0, runs[0].0, "write_bytes calls: {context}");
            }
        }
    }
}

#[test]
fn fill_rect_draws_its_pattern_with_the_ports_explicit_indices() {
    let pef = synthetic_pef_with_import(b"FillRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let rect = scratch;
    let gray = scratch + 0x10;
    let white = scratch + 0x18;
    loaded.memory.add_region(scratch, vec![0; 0x20]);
    loaded.memory.write_bytes(gray, &GRAY).unwrap();
    loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
    loaded
        .toolbox_startup
        .quickdraw_back_indices
        .insert(PPC_MAIN_GWORLD, 42);
    // The pen pattern stays solid: FillRect must not consult it.
    assert_eq!(loaded.toolbox_startup.quickdraw_pen_pattern, [0xff; 8]);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert_eq!(front.depth, 8);

    ppc_write_rect(&mut loaded.memory, rect, 4, 8, 7, 16).unwrap();
    loaded.cpu.gpr[3] = rect;
    loaded.cpu.gpr[4] = gray;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FillRect);
    // Row 4 is 0xaa (even columns set); row 5 is 0x55 (odd columns set).
    for (point, expected) in [
        ((8, 4), 103),
        ((9, 4), 42),
        ((8, 5), 42),
        ((9, 5), 103),
        ((15, 6), 42),
    ] {
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, point),
            Some(expected),
            "{point:?}"
        );
    }
    let outside = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (8, 7));
    assert_ne!(outside, Some(103));
    assert_ne!(outside, Some(42));

    // An all-white pattern paints the whole rectangle in the background.
    loaded.cpu.gpr[3] = rect;
    loaded.cpu.gpr[4] = white;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FillRect);
    for point in [(8, 4), (9, 5), (15, 6)] {
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, point),
            Some(42),
            "{point:?}"
        );
    }
}

#[test]
fn fill_rect_at_16bpp_uses_the_rgb_fore_and_back_colours() {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, WHOLE_PORT, CLIP_WITH_HOLE);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FillRect")).unwrap();
    install_port(&mut loaded, &fixture, 16);
    loaded.quickdraw_fore_color = PpcRgbColor {
        red: 0xFFFF,
        green: 0,
        blue: 0,
    };
    loaded.quickdraw_back_color = PpcRgbColor {
        red: 0,
        green: 0,
        blue: 0xFFFF,
    };
    ppc_write_rect(&mut loaded.memory, RECT, -3, -5, 9, 19).unwrap();
    loaded.memory.write_bytes(PATTERN, &MIXED).unwrap();
    loaded.cpu.gpr[3] = RECT;
    loaded.cpu.gpr[4] = PATTERN;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FillRect);
    let after = read_pixels(&mut loaded, pixels.len());
    let ((t, l, b, r), _) = CLIP_WITH_HOLE;
    for y in BOUNDS.0..BOUNDS.2 {
        for x in BOUNDS.1..BOUNDS.3 {
            let offset =
                (y - BOUNDS.0) as usize * usize::from(ROW_BYTES) + (x - BOUNDS.1) as usize * 2;
            let in_hole = (1..4).contains(&y) && (3..8).contains(&x);
            let covered = y >= t && y < b && x >= l && x < r && !in_hole;
            let pixel = u16::from_be_bytes([after[offset], after[offset + 1]]);
            if covered {
                let set = MIXED[y.rem_euclid(8) as usize] & (0x80 >> x.rem_euclid(8)) != 0;
                assert_eq!(pixel, if set { 0x7C00 } else { 0x001F }, "({x}, {y})");
            } else {
                assert_eq!(
                    [after[offset], after[offset + 1]],
                    [pixels[offset], pixels[offset + 1]],
                    "({x}, {y}) must be unchanged"
                );
            }
        }
    }
}

/// Runs FillRect with `pattern` and, separately, the earlier solid fill,
/// on the same port; returns both images.
fn fill_rect_and_solid_fill(
    fixture: &IndexedPortFixture<'_>,
    depth: u32,
    rect: (i16, i16, i16, i16),
    pattern: [u8; 8],
) -> (Vec<u8>, Vec<u8>) {
    let filled = fill_rect_ppc(fixture, depth, rect, pattern);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FillRect")).unwrap();
    install_port(&mut loaded, fixture, depth);
    ppc_paint_rect_bounds(
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        rect,
        PPC_RGB_BLACK,
        Some(fixture.fg_index),
    );
    (filled, read_pixels(&mut loaded, fixture.pixels.len()))
}

#[test]
fn fill_rect_draws_its_pattern_at_packed_depths() {
    // Imaging With QuickDraw (1994), p. 3-67: FillRect draws the pattern's
    // clear bits in the background colour at every depth. The packed depths
    // the core declines used to take a solid foreground fill instead, so
    // Bad Mojo's FillRect(qd.white) of a 1-bit occlusion mask blackened it
    // and the roach was never drawn. The 8-bit fill is the oracle for which
    // pixels take the fore and back indices; the rest stay untouched.
    let width = (BOUNDS.3 - BOUNDS.1) as usize;
    let zero = vec![0u8; usize::from(ROW_BYTES) * HEIGHT];
    let ones = vec![0xFFu8; usize::from(ROW_BYTES) * HEIGHT];
    let mut drew_back = 0;
    for (vis, clip) in CLIPS {
        for rect in RECTS {
            for pattern in [[0x00; 8], [0xFF; 8], GRAY, DIAGONAL, MIXED] {
                let oracle = fill_rect_ppc(&fixture(&zero, vis, clip), 8, rect, pattern);
                for depth in [1u32, 2, 4] {
                    let mask = (1u8 << depth) - 1;
                    let filled = fill_rect_ppc(&fixture(&ones, vis, clip), depth, rect, pattern);
                    for y in 0..HEIGHT {
                        for x in 0..width {
                            let row = y * usize::from(ROW_BYTES);
                            let bit = x * depth as usize;
                            let shift = 8 - depth as usize - bit % 8;
                            let actual = (filled[row + bit / 8] >> shift) & mask;
                            let expected = match oracle[row + x] {
                                103 => 103 & mask,
                                42 => {
                                    drew_back += 1;
                                    42 & mask
                                }
                                _ => mask,
                            };
                            assert_eq!(
                                actual, expected,
                                "{depth} bpp ({x},{y}) rect {rect:?} pattern {pattern:02x?}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(drew_back > 0);
}

#[test]
fn fill_rect_keeps_the_solid_fill_when_a_clip_region_is_malformed() {
    let pixels = initial_pixels();
    // The data runs past the region's stored size: the parser cannot finish.
    let truncated: RegionSpec = ((-1, -2, 7, 15), &[-1, -2, 15]);
    let fixture = fixture(&pixels, WHOLE_PORT, truncated);
    let (filled, solid) = fill_rect_and_solid_fill(&fixture, 8, (-6, -4, 5, 30), GRAY);
    assert_eq!(filled, solid);
    // The solid fill gives up on such a region and draws nothing.
    assert_eq!(filled, pixels);
}

#[test]
fn fill_rect_with_an_unreadable_pattern_keeps_the_solid_fill() {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, WHOLE_PORT, CLIP_RECT);
    let rect = (-6, -4, 5, 30);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FillRect")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    ppc_write_rect(&mut loaded.memory, RECT, rect.0, rect.1, rect.2, rect.3).unwrap();
    loaded.cpu.gpr[3] = RECT;
    loaded.cpu.gpr[4] = 0xFFFF_FFF0;
    assert!(loaded.memory.read_u8(0xFFFF_FFF0).is_none());
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FillRect);
    let filled = read_pixels(&mut loaded, pixels.len());
    let (_, solid) = fill_rect_and_solid_fill(&fixture, 8, rect, GRAY);
    assert_eq!(filled, solid);
    assert_ne!(filled, pixels);
}

#[test]
fn get_pixel_reports_whether_a_local_pixel_is_not_white() {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetPixel")).unwrap();
    install_port(&mut loaded, &fixture, 16);
    let offset = |x: i16, y: i16| {
        PIXELS + (y - BOUNDS.0) as u32 * u32::from(ROW_BYTES) + (x - BOUNDS.1) as u32 * 2
    };
    loaded.memory.write_u16_be(offset(1, 0), 0x7C00).unwrap();
    loaded.memory.write_u16_be(offset(0, 1), 0x7FFF).unwrap();
    let operation = PpcImportDispatcherTarget::QuickDrawCompatibility(
        PpcQuickDrawCompatibilityOperation::GetPixel,
    );
    for ((h, v), expected) in [((1u32, 0u32), 1), ((0, 1), 0)] {
        loaded.cpu.gpr[3] = h;
        loaded.cpu.gpr[4] = v;
        run_test_import(&mut loaded, operation.clone());
        assert_eq!(loaded.cpu.gpr[3], expected, "({h}, {v})");
    }
}

/// Runs `target` on a fresh 8-bit port with r3..r5 and returns the pixels.
fn shape_pixels(target: PpcImportDispatcherTarget, regs: [u32; 3], pattern: [u8; 8]) -> Vec<u8> {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FillOval")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    ppc_write_rect(&mut loaded.memory, RECT, -2, -3, 8, 15).unwrap();
    loaded.memory.write_bytes(PATTERN, &pattern).unwrap();
    loaded.cpu.gpr[3] = regs[0];
    loaded.cpu.gpr[4] = regs[1];
    loaded.cpu.gpr[5] = regs[2];
    run_test_import(&mut loaded, target);
    read_pixels(&mut loaded, pixels.len())
}

#[test]
fn fill_oval_with_a_solid_pattern_matches_paint_oval() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FillOval"),
        PpcImportDispatcherTarget::QuickDrawShape(PpcQuickDrawShapeOperation::FillOval)
    );
    let fill = shape_pixels(
        PpcImportDispatcherTarget::QuickDrawShape(PpcQuickDrawShapeOperation::FillOval),
        [RECT, PATTERN, 0],
        [0xff; 8],
    );
    let paint = shape_pixels(PpcImportDispatcherTarget::PaintOval, [RECT, 0, 0], [0; 8]);
    assert_eq!(fill, paint);
    assert_ne!(fill, initial_pixels());
    let gray = shape_pixels(
        PpcImportDispatcherTarget::QuickDrawShape(PpcQuickDrawShapeOperation::FillOval),
        [RECT, PATTERN, 0],
        GRAY,
    );
    assert_ne!(gray, paint);
}

#[test]
fn frame_arc_draws_only_the_outline_part_of_the_wedge() {
    let original = initial_pixels();
    let paint = shape_pixels(PpcImportDispatcherTarget::PaintArc, [RECT, 0, 180], [0; 8]);
    let frame = shape_pixels(
        PpcImportDispatcherTarget::QuickDrawShape(PpcQuickDrawShapeOperation::FrameArc),
        [RECT, 0, 180],
        [0; 8],
    );
    let changed = |pixels: &[u8]| {
        pixels
            .iter()
            .zip(&original)
            .filter(|(now, before)| now != before)
            .count()
    };
    assert!(changed(&frame) > 0);
    assert!(changed(&frame) < changed(&paint));
    for (index, value) in frame.iter().enumerate() {
        if *value != original[index] {
            assert_eq!(
                *value, paint[index],
                "frame pixel {index} is outside the wedge"
            );
        }
    }
}

#[test]
fn std_rect_and_std_rgn_dispatch_each_graf_verb() {
    for (verb, equivalent) in [
        (0u32, PpcImportDispatcherTarget::FrameRect),
        (1, PpcImportDispatcherTarget::PaintRect),
        (2, PpcImportDispatcherTarget::EraseRect),
        (3, PpcImportDispatcherTarget::InvertRect),
    ] {
        let bottleneck = shape_pixels(
            PpcImportDispatcherTarget::QuickDrawShape(PpcQuickDrawShapeOperation::StdRect),
            [verb, RECT, 0],
            [0; 8],
        );
        let direct = shape_pixels(equivalent.clone(), [RECT, 0, 0], [0; 8]);
        assert_eq!(bottleneck, direct, "verb {verb} as {equivalent:?}");
    }
    let region = REGION_HANDLES[1];
    let bottleneck = shape_pixels(
        PpcImportDispatcherTarget::QuickDrawShape(PpcQuickDrawShapeOperation::StdRgn),
        [1, region, 0],
        [0; 8],
    );
    let direct = shape_pixels(PpcImportDispatcherTarget::PaintRgn, [region, 0, 0], [0; 8]);
    assert_eq!(bottleneck, direct);
}

#[test]
fn invert_round_rect_complements_only_the_rounded_interior() {
    let original = initial_pixels();
    let square = shape_pixels(PpcImportDispatcherTarget::InvertRect, [RECT, 0, 0], [0; 8]);
    let round = shape_pixels(
        PpcImportDispatcherTarget::InvertRoundRect,
        [RECT, 8, 8],
        [0; 8],
    );
    let changed = |pixels: &[u8]| {
        pixels
            .iter()
            .zip(&original)
            .filter(|(now, before)| now != before)
            .count()
    };
    assert!(changed(&round) > 0, "InvertRoundRect must draw");
    assert!(
        changed(&round) < changed(&square),
        "rounded corners must stay untouched"
    );
    for (index, value) in round.iter().enumerate() {
        if *value != original[index] {
            assert_eq!(*value, square[index], "pixel {index} is not an inversion");
        }
    }
}

const PIXPAT_HANDLE: u32 = SCRATCH + 0x500;
const PIXPAT: u32 = SCRATCH + 0x510;
const SHAPE: Rect = Rect {
    top: -2,
    left: -3,
    bottom: 8,
    right: 15,
};

/// Runs `target` on an 8-bit port clipped by `vis` and `clip`, with `pattern`
/// as both the raw pattern at `PATTERN` and pat1Data of a bit-pattern PixPat.
fn clipped_shape_pixels(
    target: PpcImportDispatcherTarget,
    regs: [u32; 4],
    pattern: [u8; 8],
    clips: (RegionSpec, RegionSpec),
) -> Vec<u8> {
    prepared_shape_pixels(target, regs, pattern, clips, |_| {})
}

/// `clipped_shape_pixels` with `prepare` run on the port before the call.
fn prepared_shape_pixels(
    target: PpcImportDispatcherTarget,
    regs: [u32; 4],
    pattern: [u8; 8],
    (vis, clip): (RegionSpec, RegionSpec),
    prepare: impl FnOnce(&mut PpcLoadedApp),
) -> Vec<u8> {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, vis, clip);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"InvertOval")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    ppc_write_rect(
        &mut loaded.memory,
        RECT,
        SHAPE.top,
        SHAPE.left,
        SHAPE.bottom,
        SHAPE.right,
    )
    .unwrap();
    loaded.memory.write_bytes(PATTERN, &pattern).unwrap();
    loaded.memory.write_u32_be(PIXPAT_HANDLE, PIXPAT).unwrap();
    loaded.memory.write_u16_be(PIXPAT, 0).unwrap();
    loaded.memory.write_bytes(PIXPAT + 20, &pattern).unwrap();
    for (register, value) in regs.into_iter().enumerate() {
        loaded.cpu.gpr[3 + register] = value;
    }
    prepare(&mut loaded);
    run_test_import(&mut loaded, target);
    read_pixels(&mut loaded, pixels.len())
}

fn hide_pen(loaded: &mut PpcLoadedApp) {
    loaded
        .memory
        .write_u16_be(PORT + PPC_CGRAF_PORT_PN_VIS_OFFSET, (-1i16) as u16)
        .unwrap();
}

const SHAPE_CLIPS: [(RegionSpec, RegionSpec); 3] = [
    (WHOLE_PORT, WHOLE_PORT),
    (WHOLE_PORT, CLIP_WITH_HOLE),
    (VIS_NOTCHED, CLIP_RECT),
];

fn shape_target(operation: PpcQuickDrawShapeOperation) -> PpcImportDispatcherTarget {
    PpcImportDispatcherTarget::QuickDrawShape(operation)
}

#[test]
fn oval_and_round_rect_imports_resolve_in_both_libraries() {
    for (symbol, operation) in [
        ("InvertOval", PpcQuickDrawShapeOperation::InvertOval),
        ("FillCOval", PpcQuickDrawShapeOperation::FillCOval),
        ("FillCRoundRect", PpcQuickDrawShapeOperation::FillCRoundRect),
    ] {
        for library in ["InterfaceLib", "CarbonLib"] {
            assert_eq!(
                dispatcher_target_for_import(library, symbol),
                shape_target(operation),
                "{library} {symbol}"
            );
        }
    }
}

#[test]
fn invert_oval_matches_the_68k_path_with_clipping() {
    for (vis, clip) in SHAPE_CLIPS {
        let pixels = initial_pixels();
        let expected = draw_oval_8bpp_68k(&fixture(&pixels, vis, clip), SHAPE, ShapeOp::Invert);
        assert_ne!(expected, pixels);
        assert_eq!(
            clipped_shape_pixels(
                shape_target(PpcQuickDrawShapeOperation::InvertOval),
                [RECT, 0, 0, 0],
                [0; 8],
                (vis, clip),
            ),
            expected
        );
    }
}

#[test]
fn fill_c_oval_with_a_bit_pattern_matches_the_68k_path_with_clipping() {
    for (vis, clip) in SHAPE_CLIPS {
        for pattern in [GRAY, DIAGONAL, MIXED, [0xff; 8]] {
            let pixels = initial_pixels();
            let expected =
                draw_oval_8bpp_68k(&fixture(&pixels, vis, clip), SHAPE, ShapeOp::Fill(pattern));
            assert_ne!(expected, pixels);
            assert_eq!(
                clipped_shape_pixels(
                    shape_target(PpcQuickDrawShapeOperation::FillCOval),
                    [RECT, PIXPAT_HANDLE, 0, 0],
                    pattern,
                    (vis, clip),
                ),
                expected
            );
        }
    }
}

#[test]
fn fill_c_round_rect_with_a_bit_pattern_matches_the_68k_path_with_clipping() {
    for (vis, clip) in SHAPE_CLIPS {
        for pattern in [GRAY, DIAGONAL, MIXED, [0xff; 8]] {
            let pixels = initial_pixels();
            let expected = draw_round_rect_8bpp_68k(
                &fixture(&pixels, vis, clip),
                SHAPE,
                (6, 4),
                ShapeOp::Fill(pattern),
            );
            assert_ne!(expected, pixels);
            assert_eq!(
                clipped_shape_pixels(
                    shape_target(PpcQuickDrawShapeOperation::FillCRoundRect),
                    [RECT, 6, 4, PIXPAT_HANDLE],
                    pattern,
                    (vis, clip),
                ),
                expected
            );
        }
    }
}

#[test]
fn fill_c_shapes_ignore_a_nil_pixel_pattern() {
    for (operation, regs) in [
        (PpcQuickDrawShapeOperation::FillCOval, [RECT, 0, 0, 0]),
        (PpcQuickDrawShapeOperation::FillCRoundRect, [RECT, 6, 4, 0]),
    ] {
        assert_eq!(
            clipped_shape_pixels(shape_target(operation), regs, GRAY, (WHOLE_PORT, WHOLE_PORT)),
            initial_pixels()
        );
    }
}

#[test]
fn oval_and_round_rect_imports_draw_nothing_with_a_hidden_pen() {
    for (operation, regs) in [
        (PpcQuickDrawShapeOperation::InvertOval, [RECT, 0, 0, 0]),
        (PpcQuickDrawShapeOperation::FillCOval, [RECT, PIXPAT_HANDLE, 0, 0]),
        (PpcQuickDrawShapeOperation::FillCRoundRect, [RECT, 6, 4, PIXPAT_HANDLE]),
    ] {
        assert_eq!(
            prepared_shape_pixels(
                shape_target(operation),
                regs,
                GRAY,
                (WHOLE_PORT, WHOLE_PORT),
                hide_pen,
            ),
            initial_pixels()
        );
    }
}

/// Arcs including negative, wrapping and over-360 angles, as (start, arc).
const ARCS: [(i16, i16); 5] = [(0, 90), (45, 180), (315, 90), (90, -135), (10, 400)];

fn angle_regs(angles: (i16, i16), last: u32) -> [u32; 4] {
    [RECT, angles.0 as u16 as u32, angles.1 as u16 as u32, last]
}

#[test]
fn arc_imports_resolve_in_both_libraries() {
    for (symbol, operation) in [
        ("EraseArc", PpcQuickDrawShapeOperation::EraseArc),
        ("InvertArc", PpcQuickDrawShapeOperation::InvertArc),
        ("FillArc", PpcQuickDrawShapeOperation::FillArc),
        ("FillCArc", PpcQuickDrawShapeOperation::FillCArc),
    ] {
        for library in ["InterfaceLib", "CarbonLib"] {
            assert_eq!(
                dispatcher_target_for_import(library, symbol),
                shape_target(operation),
                "{library} {symbol}"
            );
        }
    }
}

#[test]
fn invert_arc_matches_the_68k_path_with_clipping() {
    for (vis, clip) in SHAPE_CLIPS {
        for angles in ARCS {
            let pixels = initial_pixels();
            let expected =
                draw_arc_8bpp_68k(&fixture(&pixels, vis, clip), SHAPE, angles, ShapeOp::Invert);
            assert_ne!(expected, pixels, "{angles:?}");
            assert_eq!(
                clipped_shape_pixels(
                    shape_target(PpcQuickDrawShapeOperation::InvertArc),
                    angle_regs(angles, 0),
                    [0; 8],
                    (vis, clip),
                ),
                expected,
                "{angles:?}"
            );
        }
    }
}

#[test]
fn fill_arc_and_fill_c_arc_match_the_68k_path_with_clipping() {
    for (vis, clip) in SHAPE_CLIPS {
        for angles in ARCS {
            for pattern in [GRAY, MIXED, [0xff; 8]] {
                let pixels = initial_pixels();
                let expected = draw_arc_8bpp_68k(
                    &fixture(&pixels, vis, clip),
                    SHAPE,
                    angles,
                    ShapeOp::Fill(pattern),
                );
                assert_ne!(expected, pixels, "{angles:?}");
                for (operation, last) in [
                    (PpcQuickDrawShapeOperation::FillArc, PATTERN),
                    (PpcQuickDrawShapeOperation::FillCArc, PIXPAT_HANDLE),
                ] {
                    assert_eq!(
                        clipped_shape_pixels(
                            shape_target(operation),
                            angle_regs(angles, last),
                            pattern,
                            (vis, clip),
                        ),
                        expected,
                        "{operation:?} {angles:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn erase_arc_paints_the_background_inside_the_wedge_only() {
    let pixels = initial_pixels();
    let (vis, clip) = (WHOLE_PORT, CLIP_WITH_HOLE);
    let erased = clipped_shape_pixels(
        shape_target(PpcQuickDrawShapeOperation::EraseArc),
        angle_regs((45, 180), 0),
        [0; 8],
        (vis, clip),
    );
    let wedge = draw_arc_8bpp_68k(
        &fixture(&pixels, vis, clip),
        SHAPE,
        (45, 180),
        ShapeOp::Fill([0; 8]),
    );
    assert_ne!(erased, pixels);
    // An all-clear pattern is the back index everywhere the wedge reaches.
    assert_eq!(erased, wedge);
}

#[test]
fn arc_imports_draw_nothing_with_a_hidden_pen_or_a_zero_angle() {
    for (operation, last) in [
        (PpcQuickDrawShapeOperation::EraseArc, 0),
        (PpcQuickDrawShapeOperation::InvertArc, 0),
        (PpcQuickDrawShapeOperation::FillArc, PATTERN),
        (PpcQuickDrawShapeOperation::FillCArc, PIXPAT_HANDLE),
    ] {
        let target = shape_target(operation);
        let clips = (WHOLE_PORT, WHOLE_PORT);
        assert_eq!(
            prepared_shape_pixels(
                target.clone(),
                angle_regs((0, 90), last),
                GRAY,
                clips,
                hide_pen
            ),
            initial_pixels()
        );
        assert_eq!(
            clipped_shape_pixels(target, angle_regs((0, 0), last), GRAY, clips),
            initial_pixels()
        );
    }
}

const POLY_HANDLE: u32 = SCRATCH + 0x5f0;
const POLY: u32 = SCRATCH + 0x600;
const MAP_SOURCE: u32 = SCRATCH + 0x420;
const MAP_DESTINATION: u32 = SCRATCH + 0x430;

/// polySize, polyBBox and vertices as big-endian words.
fn polygon_record(bbox: [i16; 4], vertices: &[(i16, i16)]) -> Vec<u8> {
    let mut words = vec![(10 + vertices.len() * 4) as i16];
    words.extend(bbox);
    for &(v, h) in vertices {
        words.extend([v, h]);
    }
    words.iter().flat_map(|word| word.to_be_bytes()).collect()
}

/// A concave polygon, partly outside the pixel image, and a self-crossing
/// bowtie that the even-odd rule leaves two lobes of.
fn test_polygons() -> [Vec<u8>; 2] {
    [
        polygon_record(
            [-5, -8, 8, 16],
            &[(-5, -8), (8, 1), (0, 4), (6, 16), (-2, 9), (-5, -8)],
        ),
        polygon_record([-1, -2, 7, 14], &[(-1, -2), (7, 14), (-1, 14), (7, -2)]),
    ]
}

fn write_polygon(loaded: &mut PpcLoadedApp, record: &[u8]) {
    loaded.memory.write_u32_be(POLY_HANDLE, POLY).unwrap();
    loaded.memory.write_bytes(POLY, record).unwrap();
}

#[test]
fn polygon_imports_resolve_in_both_libraries() {
    for (symbol, target) in [
        ("InvertPoly", PpcImportDispatcherTarget::InvertPoly),
        ("FillCPoly", PpcImportDispatcherTarget::FillCPoly),
        ("OffsetPoly", PpcImportDispatcherTarget::OffsetPoly),
        ("MapPoly", PpcImportDispatcherTarget::MapPoly),
    ] {
        for library in ["InterfaceLib", "CarbonLib"] {
            assert_eq!(
                dispatcher_target_for_import(library, symbol),
                target.clone(),
                "{library} {symbol}"
            );
        }
    }
}

#[test]
fn invert_poly_matches_the_68k_path_with_clipping() {
    for record in test_polygons() {
        for (vis, clip) in SHAPE_CLIPS {
            let pixels = initial_pixels();
            let expected =
                draw_poly_8bpp_68k(&fixture(&pixels, vis, clip), &record, ShapeOp::Invert);
            assert_ne!(expected, pixels);
            assert_eq!(
                prepared_shape_pixels(
                    PpcImportDispatcherTarget::InvertPoly,
                    [POLY_HANDLE, 0, 0, 0],
                    [0; 8],
                    (vis, clip),
                    |loaded| write_polygon(loaded, &record),
                ),
                expected
            );
        }
    }
}

#[test]
fn fill_c_poly_with_a_bit_pattern_matches_the_68k_path_with_clipping() {
    for record in test_polygons() {
        for (vis, clip) in SHAPE_CLIPS {
            for pattern in [GRAY, MIXED, [0xff; 8]] {
                let pixels = initial_pixels();
                let expected = draw_poly_8bpp_68k(
                    &fixture(&pixels, vis, clip),
                    &record,
                    ShapeOp::Fill(pattern),
                );
                assert_ne!(expected, pixels);
                assert_eq!(
                    prepared_shape_pixels(
                        PpcImportDispatcherTarget::FillCPoly,
                        [POLY_HANDLE, PIXPAT_HANDLE, 0, 0],
                        pattern,
                        (vis, clip),
                        |loaded| write_polygon(loaded, &record),
                    ),
                    expected
                );
            }
        }
    }
}

#[test]
fn polygon_drawing_imports_honour_a_hidden_pen_and_a_nil_pixel_pattern() {
    let [record, _] = test_polygons();
    let clips = (WHOLE_PORT, WHOLE_PORT);
    for (target, regs, hide) in [
        (PpcImportDispatcherTarget::InvertPoly, [POLY_HANDLE, 0, 0, 0], true),
        (PpcImportDispatcherTarget::FillCPoly, [POLY_HANDLE, PIXPAT_HANDLE, 0, 0], true),
        (PpcImportDispatcherTarget::FillCPoly, [POLY_HANDLE, 0, 0, 0], false),
    ] {
        assert_eq!(
            prepared_shape_pixels(target, regs, GRAY, clips, |loaded| {
                write_polygon(loaded, &record);
                if hide {
                    hide_pen(loaded);
                }
            }),
            initial_pixels()
        );
    }
}

/// Runs a polygon-record import with `regs` and returns the polygon record.
fn transformed_polygon(
    target: PpcImportDispatcherTarget,
    regs: [u32; 3],
    record: &[u8],
) -> Vec<u8> {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"OffsetPoly")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    write_polygon(&mut loaded, record);
    ppc_write_rect(&mut loaded.memory, MAP_SOURCE, 0, 0, 100, 200).unwrap();
    ppc_write_rect(&mut loaded.memory, MAP_DESTINATION, 10, 20, 60, 420).unwrap();
    for (register, value) in regs.into_iter().enumerate() {
        loaded.cpu.gpr[3 + register] = value;
    }
    run_test_import(&mut loaded, target);
    let mut out = vec![0; record.len()];
    loaded.memory.read_bytes_into(POLY, &mut out).unwrap();
    out
}

#[test]
fn offset_poly_moves_the_bounding_box_and_every_vertex() {
    let record = polygon_record([1, 2, 5, 8], &[(1, 2), (5, 8), (3, 2)]);
    assert_eq!(
        transformed_polygon(
            PpcImportDispatcherTarget::OffsetPoly,
            // dh then dv, with junk in the upper halves of the registers.
            [POLY_HANDLE, 0x1234_0003, 0x5678_fff6],
            &record,
        ),
        polygon_record([-9, 5, -5, 11], &[(-9, 5), (-5, 11), (-7, 5)])
    );
}

#[test]
fn map_poly_scales_the_bounding_box_and_every_vertex() {
    // v maps by 50/100 from 10, h by 400/200 from 20.
    let record = polygon_record([0, 0, 100, 200], &[(0, 0), (100, 200), (50, 25), (0, 0)]);
    assert_eq!(
        transformed_polygon(
            PpcImportDispatcherTarget::MapPoly,
            [POLY_HANDLE, MAP_SOURCE, MAP_DESTINATION],
            &record,
        ),
        polygon_record(
            [10, 20, 60, 420],
            &[(10, 20), (60, 420), (35, 70), (10, 20)]
        )
    );
}

const MASK_SOURCE: u32 = PPC_DATA_BASE + 0x3000;
const MASK_DESTINATION: u32 = PPC_DATA_BASE + 0x3800;

/// Runs SeedFill or CalcMask with r3..r10 on a source image and a destination
/// of `destination_len` bytes filled with $A5, and returns the destination.
fn mask_import(
    operation: PpcQuickDrawCompatibilityOperation,
    source: &[u8],
    (source_row, destination_row): (i16, i16),
    (height, words): (i16, i16),
    seed: (i16, i16),
    destination_len: usize,
) -> Vec<u8> {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"SeedFill")).unwrap();
    loaded.memory.add_region(MASK_SOURCE, source.to_vec());
    loaded
        .memory
        .add_region(MASK_DESTINATION, vec![0xa5; destination_len]);
    let high = 0x1234_0000u32;
    for (register, value) in [
        MASK_SOURCE,
        MASK_DESTINATION,
        high | source_row as u16 as u32,
        high | destination_row as u16 as u32,
        high | height as u16 as u32,
        high | words as u16 as u32,
        high | seed.0 as u16 as u32,
        high | seed.1 as u16 as u32,
    ]
    .into_iter()
    .enumerate()
    {
        loaded.cpu.gpr[3 + register] = value;
    }
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(operation),
    );
    ppc_memory_read_bytes(&mut loaded.memory, MASK_DESTINATION, destination_len as u32).unwrap()
}

/// 16×5 image: a closed box at columns 1..=6 around a hole, and a box at
/// columns 9..=14 open at the top.
const MASK_IMAGE: [u8; 10] = [
    0b0111_1110, 0b0100_0010,
    0b0100_0010, 0b0100_0010,
    0b0100_0010, 0b0100_0010,
    0b0100_0010, 0b0100_0010,
    0b0111_1110, 0b0111_1110,
];

#[test]
fn mask_imports_resolve_in_both_libraries() {
    for (symbol, operation) in [
        ("SeedFill", PpcQuickDrawCompatibilityOperation::SeedFill),
        ("CalcMask", PpcQuickDrawCompatibilityOperation::CalcMask),
    ] {
        for library in ["InterfaceLib", "CarbonLib"] {
            assert_eq!(
                dispatcher_target_for_import(library, symbol),
                PpcImportDispatcherTarget::QuickDrawCompatibility(operation.clone()),
                "{library} {symbol}"
            );
        }
    }
}

#[test]
fn seed_fill_matches_the_68k_path() {
    // Inside each box, outside them, on an edge, and outside the image.
    for seed in [(3, 2), (11, 2), (0, 0), (1, 0), (16, 0), (-1, 2), (0, 5)] {
        assert_eq!(
            mask_import(
                PpcQuickDrawCompatibilityOperation::SeedFill,
                &MASK_IMAGE,
                (2, 2),
                (5, 1),
                seed,
                MASK_IMAGE.len(),
            ),
            seed_fill_68k(&MASK_IMAGE, 5, 1, seed),
            "{seed:?}"
        );
    }
}

#[test]
fn calc_mask_sets_the_bits_paint_cannot_reach_from_an_edge() {
    assert_eq!(
        mask_import(
            PpcQuickDrawCompatibilityOperation::CalcMask,
            &MASK_IMAGE,
            (2, 2),
            (5, 1),
            (0, 0),
            MASK_IMAGE.len(),
        ),
        // The closed box keeps its hole; the open box keeps only its walls.
        [
            0b0111_1110, 0b0100_0010,
            0b0111_1110, 0b0100_0010,
            0b0111_1110, 0b0100_0010,
            0b0111_1110, 0b0100_0010,
            0b0111_1110, 0b0111_1110,
        ]
    );
}

#[test]
fn mask_imports_keep_bytes_beyond_the_image_width() {
    let source = [0xff, 0xff, 0x12, 0x34, 0xff, 0xff, 0x56, 0x78];
    assert_eq!(
        mask_import(
            PpcQuickDrawCompatibilityOperation::CalcMask,
            &source,
            (4, 3),
            (2, 1),
            (0, 0),
            6,
        ),
        [0xff, 0xff, 0xa5, 0xff, 0xff, 0xa5]
    );
}

#[test]
fn mask_imports_with_invalid_geometry_change_nothing() {
    for (operation, rows, size) in [
        (PpcQuickDrawCompatibilityOperation::SeedFill, (2, 2), (0, 1)),
        (PpcQuickDrawCompatibilityOperation::SeedFill, (2, 2), (5, 0)),
        (PpcQuickDrawCompatibilityOperation::CalcMask, (1, 2), (5, 1)),
        (PpcQuickDrawCompatibilityOperation::CalcMask, (2, -2), (5, 1)),
        (PpcQuickDrawCompatibilityOperation::CalcMask, (2, 2), (-5, 1)),
    ] {
        assert_eq!(
            mask_import(operation, &MASK_IMAGE, rows, size, (0, 0), MASK_IMAGE.len()),
            [0xa5; 10]
        );
    }
}

fn line_initial_pixels(depth: u32) -> Vec<u8> {
    if depth == 1 {
        // The foreground index is odd (one). Start clear so even a vertical
        // line cannot already match every destination bit.
        vec![0; usize::from(ROW_BYTES) * HEIGHT]
    } else {
        initial_pixels()
    }
}

fn line_pixels(depth: u32, to: (i16, i16), standard: bool) -> (Vec<u8>, (i16, i16)) {
    let pixels = line_initial_pixels(depth);
    let fixture = fixture(&pixels, WHOLE_PORT, CLIP_WITH_HOLE);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"StdLine")).unwrap();
    install_port(&mut loaded, &fixture, depth);
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveTo);
    if standard {
        loaded.cpu.gpr[3] = (u32::from(to.1 as u16) << 16) | u32::from(to.0 as u16);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::StdLine);
    } else {
        loaded.cpu.gpr[3] = u32::from(to.0 as u16);
        loaded.cpu.gpr[4] = u32::from(to.1 as u16);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::LineTo);
    }
    let pen = (loaded.quickdraw_pen_h, loaded.quickdraw_pen_v);
    (read_pixels(&mut loaded, pixels.len()), pen)
}

#[test]
fn std_line_unpacks_its_point_and_draws_like_line_to() {
    for library in ["InterfaceLib", "CarbonLib"] {
        assert_eq!(
            dispatcher_target_for_import(library, "StdLine"),
            PpcImportDispatcherTarget::StdLine,
            "{library}"
        );
    }
    for depth in [1, 8, 16] {
        // Negative fields check the packed order and sign of both halves.
        for to in [(14, 1), (2, 7), (-4, -2), (-4, 6), (16, -2)] {
            let standard = line_pixels(depth, to, true);
            assert_eq!(standard, line_pixels(depth, to, false), "{depth} {to:?}");
            assert_ne!(standard.0, line_initial_pixels(depth), "{depth} {to:?}");
            assert_eq!(standard.1, to);
        }
    }
}

/// Fills `SHAPE` through a NewPixPat handle made by MakeRGBPat, clipped by a
/// hole, and returns the pixels and the handle's record type.
fn rgb_pattern_fill(depth: u32, rgb: [u16; 3]) -> (Vec<u8>, u16) {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, WHOLE_PORT, CLIP_WITH_HOLE);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"MakeRGBPat")).unwrap();
    install_port(&mut loaded, &fixture, depth);
    ppc_write_rect(
        &mut loaded.memory,
        RECT,
        SHAPE.top,
        SHAPE.left,
        SHAPE.bottom,
        SHAPE.right,
    )
    .unwrap();
    for (index, value) in rgb.into_iter().enumerate() {
        loaded
            .memory
            .write_u16_be(PATTERN + index as u32 * 2, value)
            .unwrap();
    }
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::NewPixPat,
        ),
    );
    let handle = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = PATTERN;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::MakeRGBPat,
        ),
    );
    let record = loaded.memory.read_u32_be(handle).unwrap();
    let pat_type = loaded.memory.read_u16_be(record).unwrap();
    let map_handle = loaded.memory.read_u32_be(record + 2).unwrap();
    let map = loaded.memory.read_u32_be(map_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(map + 4).unwrap() & 0x3FFF, 2);
    assert_eq!(ppc_read_rect(&mut loaded.memory, map + 6), Some((0, 0, 8, 8)));
    loaded.cpu.gpr[3] = RECT;
    loaded.cpu.gpr[4] = handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FillCRect);
    (read_pixels(&mut loaded, pixels.len()), pat_type)
}

#[test]
fn make_rgb_pat_fills_with_the_requested_color_inside_the_clip() {
    for library in ["InterfaceLib", "CarbonLib"] {
        assert_eq!(
            dispatcher_target_for_import(library, "MakeRGBPat"),
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::MakeRGBPat,
            ),
            "{library}"
        );
    }
    let original = initial_pixels();
    let row = usize::from(ROW_BYTES);
    let pixel = |bytes: &[u8], h: i32, v: i32| {
        let at = (v + 3) as usize * row + (h + 5) as usize * 2;
        u16::from_be_bytes([bytes[at], bytes[at + 1]])
    };
    let (red, pat_type) = rgb_pattern_fill(16, [0xFFFF, 0, 0]);
    assert_eq!(pat_type, 2);
    let (blue, _) = rgb_pattern_fill(16, [0, 0, 0xFFFF]);
    // Solid across every row of the 8-by-8 tile, whatever pat1Data holds.
    for v in -2..1 {
        for h in -3..15 {
            assert_eq!(pixel(&red, h, v), 0x7C00, "red {h},{v}");
            assert_eq!(pixel(&blue, h, v), 0x001F, "blue {h},{v}");
        }
    }
    // The clip's hole and everything outside the rectangle stay untouched.
    assert_eq!(pixel(&red, 4, 2), pixel(&original, 4, 2));
    assert_eq!(pixel(&red, 16, 0), pixel(&original, 16, 0));

    // At 8 bits the colour resolves through the destination's table.
    let (indexed_red, _) = rgb_pattern_fill(8, [0xFFFF, 0, 0]);
    let (indexed_blue, _) = rgb_pattern_fill(8, [0, 0, 0xFFFF]);
    let at = |h: i32, v: i32| (v + 3) as usize * row + (h + 5) as usize;
    let solid = indexed_red[at(0, 0)];
    assert!((-3..15).all(|h| indexed_red[at(h, 0)] == solid));
    assert_ne!(solid, indexed_blue[at(0, 0)]);
    assert_eq!(indexed_red[at(4, 2)], original[at(4, 2)]);
}

#[test]
fn make_rgb_pat_rejects_a_nil_handle_or_color_without_changing_the_record() {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"MakeRGBPat")).unwrap();
    install_port(&mut loaded, &fixture, 16);
    let make = PpcImportDispatcherTarget::QuickDrawCompatibility(
        PpcQuickDrawCompatibilityOperation::MakeRGBPat,
    );
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = PATTERN;
    run_test_import(&mut loaded, make.clone());
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::NewPixPat,
        ),
    );
    let handle = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, make);
    let record = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(record), Some(1));
}

#[test]
fn indexed_offscreen_text_keeps_outline_presentation_when_copied_to_a_direct_screen() {
    // Applications commonly compose a frame, text included, in an 8-bit
    // offscreen GWorld and copy it to a 16-bit screen each frame.
    use crate::memory::{MacMemoryBus, MemoryBus};
    const SCREEN: u32 = 0x3000;
    const SCREEN_PIXMAP: u32 = SCRATCH + 0x500;
    let white16: Vec<u8> = [0x7f, 0xff].repeat(usize::from(ROW_BYTES) * HEIGHT / 2);
    // A background index whose value differs from the high byte of its
    // 16-bit pixel, so each destination byte needs its own mapping.
    const WHITE_INDEX: u8 = 0x12;
    let clut: [[u16; 3]; 256] = std::array::from_fn(|i| {
        if i == usize::from(WHITE_INDEX) {
            [u16::MAX; 3]
        } else {
            [i as u16 * 257; 3]
        }
    });
    let mut outputs = Vec::new();
    for copied in [false, true] {
        let indexed = vec![WHITE_INDEX; usize::from(ROW_BYTES) * HEIGHT];
        let fixture = fixture(&indexed, WHOLE_PORT, WHOLE_PORT);
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
        install_port(&mut loaded, &fixture, if copied { 8 } else { 16 });
        loaded.memory.add_region(SCREEN, white16.clone());
        let mut bus = MacMemoryBus::new(1024 * 1024);
        bus.write_bytes(SCREEN, &white16);
        bus.enable_outline_presentation(
            (SCREEN, u32::from(ROW_BYTES), 24, HEIGHT as u16, 16),
            std::array::from_fn(|i| [i as u8; 3]),
            3,
        );
        loaded
            .memory
            .shared_view()
            .set_presentation(bus.presentation.clone());
        let text_pixels = if copied {
            loaded.memory.add_region(0x1000, indexed.clone());
            0x1000
        } else {
            SCREEN
        };
        loaded.memory.write_u32_be(PIXMAP, text_pixels).unwrap();
        loaded.gworlds.last_mut().unwrap().base_addr = text_pixels;
        ppc_draw_text_bytes(
            &mut loaded.memory,
            &loaded.gworlds,
            PORT,
            (-4, 7),
            PPC_QD_TEXT_FONT_DEFAULT,
            12,
            1,
            PPC_RGB_BLACK,
            Some(0),
            b"Ag",
        );
        if copied {
            assert!(!bus.has_visible_outline_detail(), "the text starts offscreen");
            let (top, left, bottom, right) = BOUNDS;
            ppc_write_pixmap(
                &mut loaded.memory,
                SCREEN_PIXMAP,
                SCREEN,
                u32::from(ROW_BYTES),
                top,
                left,
                bottom,
                right,
                16,
            )
            .unwrap();
            ppc_write_rect(&mut loaded.memory, RECT, top, left, bottom, right).unwrap();
            loaded.cpu.gpr[3] = PIXMAP;
            loaded.cpu.gpr[4] = SCREEN_PIXMAP;
            loaded.cpu.gpr[5] = RECT;
            loaded.cpu.gpr[6] = RECT;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            assert!(ppc_copy_bits(
                &mut loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                0,
                &loaded.toolbox_startup,
                &clut,
                PPC_RGB_BLACK,
                Some(0),
                PPC_RGB_WHITE,
                Some(255),
                PPC_RGB_BLACK
            ));
        }
        assert!(
            bus.has_visible_outline_detail(),
            "copied={copied}: the screen must retain outline samples"
        );
        let mut bytes = vec![0; white16.len()];
        loaded.memory.read_bytes_into(SCREEN, &mut bytes).unwrap();
        let (width, height, rgb, _) = bus.outline_presentation_rgb().unwrap();
        outputs.push((bytes, width, height, rgb));
    }
    assert_eq!(outputs[1], outputs[0]);
}

#[test]
fn offscreen_text_detail_survives_unchanged_stores_and_goes_when_erased() {
    // A guest blitter that composes a frame reads, masks and writes back
    // every byte of its offscreen buffer, text included. Storing a byte's
    // own value must not cost the text its outline detail; storing the
    // background over the text must remove all of it, edges included.
    use crate::memory::{MacMemoryBus, MemoryBus};
    const SCREEN: u32 = 0x3000;
    const OFFSCREEN: u32 = 0x1000;
    const SCREEN_PIXMAP: u32 = SCRATCH + 0x500;
    const DIRECT_OFFSCREEN: u32 = 0x5000;
    const WHITE_INDEX: u8 = 0x12;
    let white16: Vec<u8> = [0x7f, 0xff].repeat(usize::from(ROW_BYTES) * HEIGHT / 2);
    let clut: [[u16; 3]; 256] = std::array::from_fn(|i| {
        if i == usize::from(WHITE_INDEX) {
            [u16::MAX; 3]
        } else {
            [i as u16 * 257; 3]
        }
    });
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Stores {
        None,
        SameValues,
        Background,
        InkColour,
        /// Erase with the background, then draw other text over the place.
        Redrawn,
    }
    // Rows of a second, partial copy whose edges cut through the text.
    let partial_rows = std::cell::Cell::new(None::<(i16, i16)>);
    // The depth of the offscreen buffer the text is drawn into.
    let source_depth = std::cell::Cell::new(8u8);
    let run = |stores: Stores, text: bool, destination: u32, copy_first: bool, depth: u8| {
        let source_depth = source_depth.get();
        let screen_bytes = if depth == 16 {
            white16.clone()
        } else {
            vec![WHITE_INDEX; usize::from(ROW_BYTES) * HEIGHT]
        };
        let indexed = vec![WHITE_INDEX; usize::from(ROW_BYTES) * HEIGHT];
        let fixture = fixture(&indexed, WHOLE_PORT, WHOLE_PORT);
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
        install_port(&mut loaded, &fixture, source_depth.into());
        loaded.memory.add_region(SCREEN, screen_bytes.clone());
        let mut bus = MacMemoryBus::new(1024 * 1024);
        bus.write_bytes(SCREEN, &screen_bytes);
        bus.enable_outline_presentation(
            (SCREEN, u32::from(ROW_BYTES), 24, HEIGHT as u16, u16::from(depth)),
            std::array::from_fn(|i| [i as u8; 3]),
            3,
        );
        loaded
            .memory
            .shared_view()
            .set_presentation(bus.presentation.clone());
        let background = if source_depth == 16 { white16.clone() } else { indexed.clone() };
        loaded.memory.add_region(OFFSCREEN, background.clone());
        loaded.memory.write_u32_be(PIXMAP, OFFSCREEN).unwrap();
        loaded.gworlds.last_mut().unwrap().base_addr = OFFSCREEN;
        let draw = |loaded: &mut PpcLoadedApp, bytes: &[u8]| {
            ppc_draw_text_bytes(
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                (-4, 7),
                PPC_QD_TEXT_FONT_DEFAULT,
                12,
                1,
                PPC_RGB_BLACK,
                Some(0),
                bytes,
            );
        };
        if text {
            draw(&mut loaded, b"Ag");
        }
        let mut drawn = vec![0; indexed.len()];
        loaded.memory.read_bytes_into(OFFSCREEN, &mut drawn).unwrap();
        loaded.memory.add_region(DIRECT_OFFSCREEN, white16.clone());
        let (top, left, bottom, right) = BOUNDS;
        let copy = |loaded: &mut PpcLoadedApp, rows: Option<(i16, i16)>| {
            ppc_write_pixmap(
                &mut loaded.memory,
                SCREEN_PIXMAP,
                destination,
                u32::from(ROW_BYTES),
                top,
                left,
                bottom,
                right,
                if destination == SCREEN { depth.into() } else { 16 },
            )
            .unwrap();
            let (rect_top, rect_bottom) = rows.map_or((top, bottom), |(from, to)| (top + from, top + to));
            ppc_write_rect(&mut loaded.memory, RECT, rect_top, left, rect_bottom, right).unwrap();
            loaded.cpu.gpr[3] = PIXMAP;
            loaded.cpu.gpr[4] = SCREEN_PIXMAP;
            loaded.cpu.gpr[5] = RECT;
            loaded.cpu.gpr[6] = RECT;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            assert!(ppc_copy_bits(
                &mut loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                0,
                &loaded.toolbox_startup,
                &clut,
                PPC_RGB_BLACK,
                Some(0),
                PPC_RGB_WHITE,
                Some(255),
                PPC_RGB_BLACK
            ));
        };
        if copy_first {
            copy(&mut loaded, None);
        }
        // The stores below stand for guest code's own blitter.
        loaded.memory.presentation().set_guest_stores(true);
        match stores {
            Stores::None => {}
            Stores::SameValues => {
                for (offset, byte) in drawn.iter().enumerate() {
                    loaded.memory.write_u8(OFFSCREEN + offset as u32, *byte).unwrap();
                }
            }
            Stores::Background | Stores::InkColour | Stores::Redrawn => {
                for (offset, &white) in background.iter().enumerate() {
                    let fill = if stores == Stores::InkColour { 0 } else { white };
                    loaded.memory.write_u8(OFFSCREEN + offset as u32, fill).unwrap();
                }
            }
        }
        loaded.memory.presentation().set_guest_stores(false);
        if stores == Stores::Redrawn {
            draw(&mut loaded, b"il");
        }
        copy(&mut loaded, None);
        if let Some(rows) = partial_rows.get() {
            copy(&mut loaded, Some(rows));
        }
        let cells = (0..screen_bytes.len() as u32)
            .filter(|offset| bus.test_has_detail(destination + offset))
            .count();
        let (width, height, rgb, _) = bus.outline_presentation_rgb().unwrap();
        (cells, bus.has_visible_outline_detail(), width, height, rgb)
    };
    // A 16-bit screen converts each copied 8-bit pixel; an 8-bit one copies
    // it as is, and so does a 16-bit screen copying from a 16-bit buffer.
    for (source, depth) in [(8, 16), (8, 8), (16, 16)] {
        source_depth.set(source);
        let drawn = run(Stores::None, true, SCREEN, false, depth);
        assert!(drawn.1, "the copied text carries outline detail");
        assert_eq!(
            run(Stores::SameValues, true, SCREEN, false, depth),
            drawn,
            "{source}-bit buffer, {depth}-bit screen: unchanged stores keep the text as it was"
        );
        let blank = run(Stores::None, false, SCREEN, false, depth);
        assert!(!blank.1);
        assert_eq!(
            run(Stores::Background, true, SCREEN, false, depth),
            blank,
            "{source}-bit buffer, {depth}-bit screen: erased text leaves no outline behind"
        );
        // Filling with the ink colour changes no inked pixel's byte.
        let filled = run(Stores::InkColour, false, SCREEN, false, depth);
        assert!(!filled.1);
        assert_eq!(
            run(Stores::InkColour, true, SCREEN, false, depth),
            filled,
            "{source}-bit buffer, {depth}-bit screen: text filled over in its own colour leaves no outline behind"
        );
        // Other text drawn where erased text stood must look as it does on a
        // clean buffer: the erased glyphs' edges may not survive beside it.
        let fresh = run(Stores::Redrawn, false, SCREEN, false, depth);
        assert!(fresh.1);
        assert_eq!(
            run(Stores::Redrawn, true, SCREEN, false, depth),
            fresh,
            "{source}-bit buffer, {depth}-bit screen: new text shows no trace of the text it replaced"
        );
    }
    source_depth.set(8);
    // The frame is copied with its text, and again after the text is erased.
    // The second copy brings plain pixels whose bytes equal the first copy's
    // around the glyphs; they still replace the detail the first copy left,
    // on the screen and in a direct-colour offscreen buffer alike.
    for destination in [SCREEN, DIRECT_OFFSCREEN] {
        assert!(run(Stores::None, true, destination, true, 16).0 > 0);
        assert_eq!(
            run(Stores::Background, true, destination, true, 16).0,
            0,
            "destination ${destination:04X}"
        );
    }
    // A later copy of only some rows, cutting through the glyphs, leaves the
    // text exactly as the whole copy made it.
    for destination in [SCREEN, DIRECT_OFFSCREEN] {
        partial_rows.set(None);
        let whole = run(Stores::None, true, destination, false, 16);
        assert!(whole.0 > 0);
        for rows in (1..HEIGHT as i16).flat_map(|cut| [(0, cut), (cut, HEIGHT as i16)]) {
            partial_rows.set(Some(rows));
            assert_eq!(
                run(Stores::None, true, destination, false, 16),
                whole,
                "rows {rows:?} copied again to {destination:#x}"
            );
        }
    }
}

#[test]
fn import_runner_keeps_text_detail_through_guest_stores_of_unchanged_bytes() {
    // Code the import runner executes may write back text bytes it read,
    // keeping their detail. The same stores made outside guest code, as a
    // Toolbox call's drawing, replace it.
    use crate::memory::{MacMemoryBus, MemoryBus};
    const SCREEN: u32 = 0x3000;
    const OFFSCREEN: u32 = 0x1000;
    const WHITE_INDEX: u8 = 0x12;
    let white16: Vec<u8> = [0x7f, 0xff].repeat(usize::from(ROW_BYTES) * HEIGHT / 2);
    let indexed = vec![WHITE_INDEX; usize::from(ROW_BYTES) * HEIGHT];
    let fixture = fixture(&indexed, WHOLE_PORT, WHOLE_PORT);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    loaded.memory.add_region(SCREEN, white16.clone());
    let mut bus = MacMemoryBus::new(1024 * 1024);
    bus.write_bytes(SCREEN, &white16);
    bus.enable_outline_presentation(
        (SCREEN, u32::from(ROW_BYTES), 24, HEIGHT as u16, 16),
        std::array::from_fn(|i| [i as u8; 3]),
        3,
    );
    loaded
        .memory
        .shared_view()
        .set_presentation(bus.presentation.clone());
    loaded.memory.add_region(OFFSCREEN, indexed.clone());
    loaded.memory.write_u32_be(PIXMAP, OFFSCREEN).unwrap();
    loaded.gworlds.last_mut().unwrap().base_addr = OFFSCREEN;
    ppc_draw_text_bytes(
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        (-4, 7),
        PPC_QD_TEXT_FONT_DEFAULT,
        12,
        1,
        PPC_RGB_BLACK,
        Some(0),
        b"Ag",
    );
    let detail: Vec<u32> = (OFFSCREEN..OFFSCREEN + indexed.len() as u32)
        .filter(|&address| bus.test_has_detail(address))
        .collect();
    assert!(!detail.is_empty(), "the text carries offscreen detail");

    // lis/ori r3, address; lbz r4, 0(r3); stb r4, 0(r3), for each cell.
    let code_pc = PPC_DATA_BASE + 0x1100;
    let mut code = Vec::new();
    for &address in &detail {
        for word in [
            d_form_u(15, 3, 0, (address >> 16) as u16),
            d_form_u(24, 3, 3, address as u16),
            d_form_u(34, 4, 3, 0),
            d_form_u(38, 4, 3, 0),
        ] {
            code.extend_from_slice(&word.to_be_bytes());
        }
    }
    code.extend_from_slice(&0x4800_0000u32.to_be_bytes()); // b .
    loaded.memory.add_region(code_pc, code);
    loaded.cpu.pc = code_pc;
    loaded.run_with_hle_imports(detail.len() as u64 * 8 + 1_000);
    assert_eq!(loaded.cpu.pc, code_pc + detail.len() as u32 * 16, "every store ran");
    assert!(
        detail.iter().all(|&address| bus.test_has_detail(address)),
        "guest stores of unchanged bytes keep the detail"
    );

    for &address in &detail {
        let byte = loaded.memory.read_u8(address).unwrap();
        loaded.memory.write_u8(address, byte).unwrap();
    }
    assert!(
        !detail.iter().any(|&address| bus.test_has_detail(address)),
        "the same stores outside guest code replace it"
    );
}

#[test]
fn a_guest_store_that_changes_copied_text_drops_everything_that_copy_brought() {
    // Text copied from one 8-bit offscreen buffer to another is one run in
    // the destination: a guest store that changes any of its pixels removes
    // all of its detail there, also where the bytes stay the same.
    use crate::memory::{MacMemoryBus, MemoryBus};
    const SCREEN: u32 = 0x3000;
    const OFFSCREEN: u32 = 0x1000;
    const COPY: u32 = 0x5000;
    const COPY_PIXMAP: u32 = SCRATCH + 0x500;
    const WHITE_INDEX: u8 = 0x12;
    let clut: [[u16; 3]; 256] = std::array::from_fn(|i| {
        if i == usize::from(WHITE_INDEX) {
            [u16::MAX; 3]
        } else {
            [i as u16 * 257; 3]
        }
    });
    let indexed = vec![WHITE_INDEX; usize::from(ROW_BYTES) * HEIGHT];
    let fixture = fixture(&indexed, WHOLE_PORT, WHOLE_PORT);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    loaded.memory.add_region(SCREEN, indexed.clone());
    let mut bus = MacMemoryBus::new(1024 * 1024);
    bus.write_bytes(SCREEN, &indexed);
    bus.enable_outline_presentation(
        (SCREEN, u32::from(ROW_BYTES), 24, HEIGHT as u16, 8),
        std::array::from_fn(|i| [i as u8; 3]),
        3,
    );
    loaded
        .memory
        .shared_view()
        .set_presentation(bus.presentation.clone());
    loaded.memory.add_region(OFFSCREEN, indexed.clone());
    loaded.memory.write_u32_be(PIXMAP, OFFSCREEN).unwrap();
    loaded.gworlds.last_mut().unwrap().base_addr = OFFSCREEN;
    ppc_draw_text_bytes(
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        (-4, 7),
        PPC_QD_TEXT_FONT_DEFAULT,
        12,
        1,
        PPC_RGB_BLACK,
        Some(0),
        b"Ag",
    );
    loaded.memory.add_region(COPY, indexed.clone());
    let (top, left, bottom, right) = BOUNDS;
    ppc_write_pixmap(
        &mut loaded.memory,
        COPY_PIXMAP,
        COPY,
        u32::from(ROW_BYTES),
        top,
        left,
        bottom,
        right,
        8,
    )
    .unwrap();
    ppc_write_rect(&mut loaded.memory, RECT, top, left, bottom, right).unwrap();
    loaded.cpu.gpr[3] = PIXMAP;
    loaded.cpu.gpr[4] = COPY_PIXMAP;
    loaded.cpu.gpr[5] = RECT;
    loaded.cpu.gpr[6] = RECT;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 0;
    assert!(ppc_copy_bits(
        &mut loaded.cpu,
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        0,
        &loaded.toolbox_startup,
        &clut,
        PPC_RGB_BLACK,
        Some(0),
        PPC_RGB_WHITE,
        Some(255),
        PPC_RGB_BLACK
    ));
    let copied: Vec<u32> = (0..indexed.len() as u32)
        .map(|offset| COPY + offset)
        .filter(|&address| bus.test_has_detail(address))
        .collect();
    assert!(copied.len() > 1, "the copy carried the text's detail");
    // Guest code changes one pixel of the copy and stores every other byte
    // back unchanged.
    let mut held = vec![0u8; indexed.len()];
    loaded.memory.read_bytes_into(COPY, &mut held).unwrap();
    let changed = (copied[0] - COPY) as usize;
    held[changed] ^= 1;
    loaded.memory.presentation().set_guest_stores(true);
    loaded.memory.write_bytes(COPY, &held).unwrap();
    loaded.memory.presentation().set_guest_stores(false);
    let left_over = copied.iter().filter(|&&address| bus.test_has_detail(address)).count();
    assert_eq!(left_over, 0, "detail the copy brought survived");
    // The source keeps its own run.
    assert!((0..indexed.len() as u32).any(|offset| bus.test_has_detail(OFFSCREEN + offset)));
}

#[test]
fn glyphs_whose_strike_reaches_past_their_outline_leave_no_bare_pixels() {
    // The one-bit glyph can reach past its outline (the arm of a bold "r").
    // A pixel it inks without a detail cell shows as a solid block in the
    // high-resolution presentation, beside the outline text.
    use crate::memory::{MacMemoryBus, MemoryBus};
    const SCREEN: u32 = 0x3_0000;
    const OFFSCREEN: u32 = 0x1_0000;
    const WHITE_INDEX: u8 = 0x12;
    const WIDE: (i16, i16, i16, i16) = (0, 0, 40, 160);
    const WIDE_ROW: u16 = 160;
    let rows = 40usize;
    let indexed = vec![WHITE_INDEX; usize::from(WIDE_ROW) * rows];
    // Glyphs whose one-bit strike reaches past the outline at that size.
    let cases: [(i16, u8, &[u8]); 5] = [
        (20, 1, b"r"),
        (20, 0, b"r"),
        (15, 0, b"B"),
        (16, 1, b"D"),
        (21, 0, b"p"),
    ];
    for (size, style, bytes) in cases {
        let fixture = IndexedPortFixture {
            bounds: WIDE,
            row_bytes: WIDE_ROW,
            pixels: &[],
            fg_index: 103,
            bg_index: 42,
            vis: (WIDE, &[]),
            clip: (WIDE, &[]),
        };
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
        install_port(&mut loaded, &fixture, 8);
        loaded.memory.add_region(SCREEN, indexed.clone());
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        bus.write_bytes(SCREEN, &indexed);
        bus.enable_outline_presentation(
            (SCREEN, u32::from(WIDE_ROW), 160, rows as u16, 8),
            std::array::from_fn(|i| [i as u8; 3]),
            4,
        );
        loaded
            .memory
            .shared_view()
            .set_presentation(bus.presentation.clone());
        loaded.memory.add_region(OFFSCREEN, indexed.clone());
        loaded.memory.write_u32_be(PIXMAP, OFFSCREEN).unwrap();
        loaded.gworlds.last_mut().unwrap().base_addr = OFFSCREEN;
        ppc_draw_text_bytes_styled(
            &mut loaded.memory,
            &loaded.gworlds,
            PORT,
            (4, 28),
            crate::quickdraw::fonts::families::FONT_TIMES,
            size,
            1,
            PPC_RGB_BLACK,
            Some(0),
            style,
            bytes,
        );
        let mut drawn = vec![0; indexed.len()];
        loaded
            .memory
            .read_bytes_into(OFFSCREEN, &mut drawn)
            .unwrap();
        let width = usize::from(WIDE_ROW);
        let ink = |offset: usize| drawn[offset] != WHITE_INDEX;
        let has_detail = |offset: usize| bus.test_has_detail(OFFSCREEN + offset as u32);
        // An inked pixel with no detail and no inked or detailed neighbour on
        // some side: the outline does not reach it.
        let stray: Vec<usize> = (0..drawn.len())
            .filter(|&offset| {
                let column = offset % width;
                ink(offset)
                    && !has_detail(offset)
                    && [
                        column.checked_sub(1).map(|_| offset - 1),
                        (column + 1 < width).then_some(offset + 1),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|beside| !ink(beside) && !has_detail(beside))
            })
            .collect();
        assert!(drawn.iter().any(|&b| b != WHITE_INDEX), "nothing drawn");
        assert!(
            stray.is_empty(),
            "{size} {style} {:?}: stray bitmap pixels at {stray:?}",
            std::str::from_utf8(bytes).unwrap()
        );
    }
}

#[test]
fn full_width_8_to_16_copy_clears_second_row_detail_across_plain_first_row() {
    // When a full-width 16-bit destination has contiguous rows (`row_bytes ==
    // width * 2`), a plain first row followed by a row with retained detail
    // must still discard the second row's detail—including when the copied
    // bytes equal the destination's existing bytes.
    use crate::memory::{MacMemoryBus, MemoryBus};
    const SCREEN: u32 = 0x3_0000;
    const SRC_8: u32 = 0x1_0000;
    const SCREEN_PIXMAP: u32 = SCRATCH + 0x500;
    const WIDTH: u16 = 24;
    const ROWS: u16 = 16;
    const DST_ROW_BYTES: u16 = WIDTH * 2;
    const SRC_ROW_BYTES: u16 = WIDTH;
    const BOUNDS_FULL: (i16, i16, i16, i16) = (0, 0, ROWS as i16, WIDTH as i16);
    const WHITE_INDEX: u8 = 0x12;

    let white16: Vec<u8> = [0x7f, 0xff].repeat(usize::from(WIDTH) * usize::from(ROWS));
    let clut: [[u16; 3]; 256] = std::array::from_fn(|i| {
        if i == usize::from(WHITE_INDEX) {
            [u16::MAX; 3]
        } else {
            [i as u16 * 257; 3]
        }
    });

    let src_fixture = IndexedPortFixture {
        bounds: BOUNDS_FULL,
        row_bytes: SRC_ROW_BYTES,
        pixels: &[],
        fg_index: 0,
        bg_index: WHITE_INDEX,
        vis: (BOUNDS_FULL, &[]),
        clip: (BOUNDS_FULL, &[]),
    };
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
    install_port(&mut loaded, &src_fixture, 8);
    loaded.memory.add_region(SCREEN, white16.clone());
    let mut bus = MacMemoryBus::new(1024 * 1024);
    bus.write_bytes(SCREEN, &white16);
    bus.enable_outline_presentation(
        (SCREEN, u32::from(DST_ROW_BYTES), WIDTH, ROWS, 16),
        std::array::from_fn(|i| [i as u8; 3]),
        3,
    );
    loaded
        .memory
        .shared_view()
        .set_presentation(bus.presentation.clone());

    // Draw text directly onto the 16-bit screen starting below row 0, so row 0
    // has no detail while later rows carry retained outline detail.
    ppc_write_pixmap(
        &mut loaded.memory,
        SCREEN_PIXMAP,
        SCREEN,
        u32::from(DST_ROW_BYTES),
        0,
        0,
        ROWS as i16,
        WIDTH as i16,
        16,
    )
    .unwrap();
    let dst_port = PORT + 0x400;
    // Both ports use the main screen device and the same bounds.
    let source_world = *loaded.gworlds.last().unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        port: dst_port,
        pixmap_handle: 0,
        pixmap: SCREEN_PIXMAP,
        base_addr: SCREEN,
        depth: 16,
        row_bytes: u32::from(DST_ROW_BYTES),
        ..source_world
    });
    ppc_draw_text_bytes(
        &mut loaded.memory,
        &loaded.gworlds,
        dst_port,
        (2, 12),
        PPC_QD_TEXT_FONT_DEFAULT,
        10,
        1,
        PPC_RGB_BLACK,
        Some(0),
        b"A",
    );
    let first_detail_row = (0..usize::from(ROWS))
        .find(|&row| {
            let row_base = SCREEN + (row as u32) * u32::from(DST_ROW_BYTES);
            (0..u32::from(DST_ROW_BYTES)).any(|off| bus.test_has_detail(row_base + off))
        })
        .expect("screen must hold outline detail on a row below row 0");
    assert!(
        first_detail_row >= 1,
        "row 0 must be plain so the copy crosses from a plain row into a detailed row"
    );

    // Build an 8-bit plain source whose mapped 16-bit pixels match the
    // screen's exact existing bytes (including equal destination bytes on the
    // detailed row), with no outline detail of its own.
    let mut screen_now = vec![0u8; white16.len()];
    loaded
        .memory
        .read_bytes_into(SCREEN, &mut screen_now)
        .unwrap();
    let mut src_bytes = vec![WHITE_INDEX; usize::from(WIDTH) * usize::from(ROWS)];
    for (i, pair) in screen_now.chunks_exact(2).enumerate() {
        let px = u16::from_be_bytes([pair[0], pair[1]]);
        src_bytes[i] = if px == 0x7fff { WHITE_INDEX } else { 0 };
    }
    loaded.memory.add_region(SRC_8, src_bytes);
    loaded.memory.write_u32_be(PIXMAP, SRC_8).unwrap();
    loaded.gworlds[0].base_addr = SRC_8;

    // Copy a 2-row full-width span starting at the plain row immediately above
    // `first_detail_row` and covering `first_detail_row`.
    let copy_top = (first_detail_row - 1) as i16;
    let copy_bottom = (first_detail_row + 1) as i16;
    ppc_write_rect(&mut loaded.memory, RECT, copy_top, 0, copy_bottom, WIDTH as i16).unwrap();
    loaded.cpu.gpr[3] = PIXMAP;
    loaded.cpu.gpr[4] = SCREEN_PIXMAP;
    loaded.cpu.gpr[5] = RECT;
    loaded.cpu.gpr[6] = RECT;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 0;
    assert!(ppc_copy_bits(
        &mut loaded.cpu,
        &mut loaded.memory,
        &loaded.gworlds,
        dst_port,
        0,
        &loaded.toolbox_startup,
        &clut,
        PPC_RGB_BLACK,
        Some(0),
        PPC_RGB_WHITE,
        Some(255),
        PPC_RGB_BLACK
    ));

    let row_base = SCREEN + (first_detail_row as u32) * u32::from(DST_ROW_BYTES);
    let remaining = (0..u32::from(DST_ROW_BYTES))
        .filter(|&off| bus.test_has_detail(row_base + off))
        .count();
    assert_eq!(
        remaining, 0,
        "plain 8-to-16 copy spanning a plain first row must discard detail on the second row"
    );
}

#[test]
fn bold_times_measures_and_draws_with_mac_os_9_widths() {
    // `CharWidth` and string widths measured on Mac OS 9.0 for Times bold:
    // the bold face's own widths from 12 points up (O and K stay 9 at 12
    // points, and are 16 at 20), and plain + 1 at 10 points, where bold is
    // synthesized.
    use crate::quickdraw::fonts::{FONT_NEWYORK, FONT_TIMES};
    let width = |font, size, style, text: &[u8]| ppc_text_width_bytes(font, size, style, text);
    for (size, text, bold) in [
        (12, b"O".as_slice(), 9),
        (12, b"K", 9),
        (20, b"O", 16),
        (20, b"K", 16),
        (20, b"New Player", 98),
        (20, b"OK", 32),
    ] {
        let label = std::str::from_utf8(text).unwrap();
        assert_eq!(
            width(FONT_TIMES, size, 1, text),
            bold,
            "bold {size} {label}"
        );
        assert_eq!(
            width(FONT_NEWYORK, size, 1, text),
            bold,
            "New York bold {size} {label}"
        );
    }
    for text in [b"O".as_slice(), b"K", b"New Player"] {
        let label = std::str::from_utf8(text).unwrap();
        assert_eq!(
            width(FONT_TIMES, 10, 1, text),
            width(FONT_TIMES, 10, 0, text) + text.len() as i16,
            "bold 10 {label} is synthesized"
        );
    }

    // Drawing advances the pen by the same width.
    const WIDE: (i16, i16, i16, i16) = (0, 0, 40, 160);
    let indexed = vec![0x12u8; 160 * 40];
    let fixture = IndexedPortFixture {
        bounds: WIDE,
        row_bytes: 160,
        pixels: &[],
        fg_index: 103,
        bg_index: 42,
        vis: (WIDE, &[]),
        clip: (WIDE, &[]),
    };
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    loaded.memory.add_region(0x1_0000, indexed);
    loaded.memory.write_u32_be(PIXMAP, 0x1_0000).unwrap();
    loaded.gworlds.last_mut().unwrap().base_addr = 0x1_0000;
    let advance = ppc_draw_text_bytes_styled(
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        (4, 28),
        FONT_TIMES,
        20,
        1,
        PPC_RGB_BLACK,
        Some(0),
        1,
        b"New Player",
    );
    assert_eq!(advance, 98);

    // And it draws the bold face's glyphs once: exactly what the bold face
    // draws as plain text, with no second strike one pixel to the right.
    let drawn = |loaded: &mut PpcLoadedApp, font, style| {
        loaded.memory.write_bytes(0x1_0000, &vec![0x12u8; 160 * 40]).unwrap();
        ppc_draw_text_bytes_styled(
            &mut loaded.memory,
            &loaded.gworlds,
            PORT,
            (4, 28),
            font,
            20,
            1,
            PPC_RGB_BLACK,
            Some(0),
            style,
            b"Delete Player",
        );
        let mut pixels = vec![0u8; 160 * 40];
        loaded.memory.read_bytes_into(0x1_0000, &mut pixels).unwrap();
        pixels
    };
    let bold = drawn(&mut loaded, FONT_TIMES, 1);
    let face = drawn(
        &mut loaded,
        crate::quickdraw::fonts::families::FONT_BUNDLED_ROMAN_BOLD,
        0,
    );
    assert!(bold.iter().any(|&p| p != 0x12));
    assert!(
        bold == face,
        "bold Times draws other pixels than its bold face"
    );
}

fn write_test_polygon(loaded: &mut PpcLoadedApp) {
    write_polygon(loaded, &test_polygons()[0]);
}

/// Every PowerPC shape import with arguments that draw on the 8-bit test port.
fn pen_gated_shape_imports() -> Vec<(&'static str, PpcImportDispatcherTarget, [u32; 4])> {
    use PpcImportDispatcherTarget as T;
    use PpcQuickDrawShapeOperation as S;
    let region = REGION_HANDLES[1];
    let mut imports = vec![
        ("PaintRect", T::PaintRect, [RECT, 0, 0, 0]),
        ("FrameRect", T::FrameRect, [RECT, 0, 0, 0]),
        ("EraseRect", T::EraseRect, [RECT, 0, 0, 0]),
        ("InvertRect", T::InvertRect, [RECT, 0, 0, 0]),
        ("FillRect", T::FillRect, [RECT, PATTERN, 0, 0]),
        ("FillCRect", T::FillCRect, [RECT, PIXPAT_HANDLE, 0, 0]),
        ("PaintOval", T::PaintOval, [RECT, 0, 0, 0]),
        ("FrameOval", T::FrameOval, [RECT, 0, 0, 0]),
        ("EraseOval", T::EraseOval, [RECT, 0, 0, 0]),
        ("FillOval", shape_target(S::FillOval), [RECT, PATTERN, 0, 0]),
        ("PaintRoundRect", T::PaintRoundRect, [RECT, 8, 8, 0]),
        ("FrameRoundRect", T::FrameRoundRect, [RECT, 8, 8, 0]),
        ("EraseRoundRect", T::EraseRoundRect, [RECT, 8, 8, 0]),
        ("InvertRoundRect", T::InvertRoundRect, [RECT, 8, 8, 0]),
        ("FillRoundRect", T::FillRoundRect, [RECT, 8, 8, PATTERN]),
        ("PaintArc", T::PaintArc, [RECT, 0, 180, 0]),
        ("FrameArc", shape_target(S::FrameArc), [RECT, 0, 180, 0]),
        ("PaintPoly", T::PaintPoly, [POLY_HANDLE, 0, 0, 0]),
        ("ErasePoly", T::ErasePoly, [POLY_HANDLE, 0, 0, 0]),
        ("FillPoly", T::FillPoly, [POLY_HANDLE, PATTERN, 0, 0]),
        ("PaintRgn", T::PaintRgn, [region, 0, 0, 0]),
        ("FrameRgn", T::FrameRgn, [region, 0, 0, 0]),
        ("EraseRgn", T::EraseRgn, [region, 0, 0, 0]),
        ("InvertRgn", T::InvertRgn, [region, 0, 0, 0]),
        ("FillRgn", T::FillRgn, [region, PATTERN, 0, 0]),
        ("FillCRgn", T::FillCRgn, [region, PIXPAT_HANDLE, 0, 0]),
    ];
    // StdRect and StdRgn redirect each GrafVerb to the matching verb.
    let std_rect = [
        "StdRect frame",
        "StdRect paint",
        "StdRect erase",
        "StdRect invert",
        "StdRect fill",
    ];
    let std_rgn = [
        "StdRgn frame",
        "StdRgn paint",
        "StdRgn erase",
        "StdRgn invert",
        "StdRgn fill",
    ];
    for (verb, (rect_name, rgn_name)) in std_rect.into_iter().zip(std_rgn).enumerate() {
        let verb = verb as u32;
        imports.push((rect_name, shape_target(S::StdRect), [verb, RECT, 0, 0]));
        imports.push((rgn_name, shape_target(S::StdRgn), [verb, region, 0, 0]));
    }
    imports
}

#[test]
fn shape_imports_draw_nothing_while_the_pen_is_hidden() {
    let original = initial_pixels();
    for (name, target, regs) in pen_gated_shape_imports() {
        let shown = prepared_shape_pixels(
            target.clone(),
            regs,
            GRAY,
            (WHOLE_PORT, WHOLE_PORT),
            write_test_polygon,
        );
        assert_ne!(
            shown, original,
            "{name} ({target:?}) must draw with pnVis 0"
        );
        let hidden = prepared_shape_pixels(
            target.clone(),
            regs,
            GRAY,
            (WHOLE_PORT, WHOLE_PORT),
            |loaded| {
                write_test_polygon(loaded);
                hide_pen(loaded);
            },
        );
        assert_eq!(hidden, original, "{name} ({target:?}) drew with pnVis -1");
    }
}

#[test]
fn open_rgn_still_records_shapes_while_the_pen_is_hidden() {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, WHOLE_PORT, WHOLE_PORT);
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"OpenRgn")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    ppc_write_rect(&mut loaded.memory, RECT, -1, 2, 6, 11).unwrap();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRgn);
    let destination = loaded.cpu.gpr[3];
    assert_ne!(destination, 0);

    run_test_import(&mut loaded, PpcImportDispatcherTarget::OpenRgn);
    let pn_vis = |loaded: &mut PpcLoadedApp| {
        loaded
            .memory
            .read_u16_be(PORT + PPC_CGRAF_PORT_PN_VIS_OFFSET)
            .map(|value| value as i16)
    };
    assert_eq!(pn_vis(&mut loaded), Some(-1), "OpenRgn hides the pen");
    // FrameRect adds its rectangle to the open region; PaintRect does not
    // record into regions, and neither may draw.
    for target in [
        PpcImportDispatcherTarget::FrameRect,
        PpcImportDispatcherTarget::PaintRect,
    ] {
        loaded.cpu.gpr[3] = RECT;
        run_test_import(&mut loaded, target);
    }
    loaded.cpu.gpr[3] = destination;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CloseRgn);

    assert_eq!(read_pixels(&mut loaded, pixels.len()), pixels);
    assert_eq!(pn_vis(&mut loaded), Some(0), "CloseRgn shows the pen");
    let storage = ppc_region_storage(&mut loaded.memory, destination).unwrap();
    assert_eq!(storage, region_storage_bytes((-1, 2, 6, 11), &[]));
}

#[test]
fn ppc_shared_raster_rect_routes_preserve_bg_clip_erase_paint_invert_and_hidden_pen() {
    let seed = initial_pixels();
    let fixture = fixture(&seed, WHOLE_PORT, CLIP_WITH_HOLE);
    let ((ct, cl, cb, cr), _) = CLIP_WITH_HOLE;
    let in_clip = |x: i16, y: i16| {
        let in_hole = (1..4).contains(&y) && (3..8).contains(&x);
        y >= ct && y < cb && x >= cl && x < cr && !in_hole
    };

    // 1. Window Manager background fill ignores clipRgn (uses visRgn only).
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"PaintRect")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    assert!(ppc_paint_window_background_bounds(
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        (0, 0, 5, 10),
        PPC_RGB_BLACK,
    ));
    let after = read_pixels(&mut loaded, seed.len());
    // Hole pixel (5, 2) is outside clipRgn but inside visRgn, so it must be painted.
    let hole_off = (2 - BOUNDS.0) as usize * usize::from(ROW_BYTES) + (5 - BOUNDS.1) as usize;
    assert_ne!(after[hole_off], seed[hole_off]);

    // 2. Monochrome EraseRect at 8-bpp matches port-aligned FillRect and returns false when empty.
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"EraseRect")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    assert!(ppc_erase_rect(
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        (-3, -5, 9, 19),
        0,
        MIXED,
        (PPC_RGB_BLACK, Some(fixture.fg_index)),
        (PPC_RGB_WHITE, Some(fixture.bg_index)),
    ));
    let erased = read_pixels(&mut loaded, seed.len());
    assert_eq!(erased, fill_rect_ppc(&fixture, 8, (-3, -5, 9, 19), MIXED));
    assert!(!ppc_erase_rect(
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        (50, 50, 60, 60),
        0,
        MIXED,
        (PPC_RGB_BLACK, Some(fixture.fg_index)),
        (PPC_RGB_WHITE, Some(fixture.bg_index)),
    ));

    // 3. Patterned PaintRect at 8-bpp aligns its pattern to surface origin (left, top).
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"PaintRect")).unwrap();
    install_port(&mut loaded, &fixture, 8);
    ppc_write_rect(&mut loaded.memory, RECT, -3, -5, 9, 19).unwrap();
    loaded.cpu.gpr[3] = RECT;
    assert!(ppc_paint_rect(
        &loaded.cpu,
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        PPC_RGB_BLACK,
        Some(fixture.fg_index),
        PPC_RGB_WHITE,
        &MIXED,
    ));
    let painted = read_pixels(&mut loaded, seed.len());
    for y in BOUNDS.0..BOUNDS.2 {
        for x in BOUNDS.1..BOUNDS.3 {
            let off = (y - BOUNDS.0) as usize * usize::from(ROW_BYTES) + (x - BOUNDS.1) as usize;
            if in_clip(x, y) {
                let ly = (y - BOUNDS.0) as usize;
                let lx = (x - BOUNDS.1) as usize;
                let set = MIXED[ly & 7] & (0x80 >> (lx & 7)) != 0;
                assert_eq!(painted[off], if set { fixture.fg_index } else { 0 });
            } else {
                assert_eq!(painted[off], seed[off]);
            }
        }
    }

    // 4. 1-bpp InvertRect complements unaligned bit spans without clip regions.
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"InvertRect")).unwrap();
    install_port(&mut loaded, &fixture, 1);
    let before_1 = read_pixels(&mut loaded, seed.len());
    assert!(ppc_invert_rect_bounds(
        &mut loaded.memory,
        &loaded.gworlds,
        PORT,
        (-1, -2, 6, 14),
    ));
    let after_1 = read_pixels(&mut loaded, seed.len());
    for y in BOUNDS.0..BOUNDS.2 {
        for x in BOUNDS.1..BOUNDS.3 {
            let lx = (x - BOUNDS.1) as usize;
            let off = (y - BOUNDS.0) as usize * usize::from(ROW_BYTES) + lx / 8;
            let mask = 0x80u8 >> (lx % 8);
            let inside = (-1..6).contains(&y) && (-2..14).contains(&x);
            let expected = if inside {
                before_1[off] ^ mask
            } else {
                before_1[off]
            };
            assert_eq!(after_1[off] & mask, expected & mask, "({x}, {y})");
        }
    }

    // 5. A solid 1-bpp FillRect still draws nothing while the pen is hidden.
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FillRect")).unwrap();
    install_port(&mut loaded, &fixture, 1);
    let before_hidden = read_pixels(&mut loaded, seed.len());
    loaded
        .memory
        .write_u16_be(PORT + PPC_CGRAF_PORT_PN_VIS_OFFSET, (-1i16) as u16)
        .unwrap();
    ppc_write_rect(&mut loaded.memory, RECT, -3, -5, 9, 19).unwrap();
    loaded.memory.write_bytes(PATTERN, &[0xFF; 8]).unwrap();
    loaded.cpu.gpr[3] = RECT;
    loaded.cpu.gpr[4] = PATTERN;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FillRect);
    assert_eq!(read_pixels(&mut loaded, seed.len()), before_hidden);
}

#[test]
fn ppc_paint_and_erase_rect_preserve_writable_pixels_around_readonly_overlay() {
    let seed = vec![0x33; usize::from(ROW_BYTES) * HEIGHT];
    let fixture = fixture(&seed, WHOLE_PORT, WHOLE_PORT);
    let pattern = [0xff, 0, 0, 0, 0, 0, 0, 0];
    let run = |import: &[u8], with_overlay: bool| {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(import)).unwrap();
        install_port(&mut loaded, &fixture, 8);
        let mut backing = crate::memory::MacMemoryBus::new(0x1000);
        if with_overlay {
            let overlay = backing.shared_ram_region(0, 2).unwrap();
            // SAFETY: this test serializes all access to the backing and the mapped view.
            unsafe {
                loaded
                    .memory
                    .add_shared_readonly_region(None, PIXELS + 3, overlay);
            }
        }
        let wrote = if import == b"PaintRect" {
            ppc_write_rect(&mut loaded.memory, RECT, -3, -5, -2, 3).unwrap();
            loaded.cpu.gpr[3] = RECT;
            ppc_paint_rect(
                &loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                PPC_RGB_BLACK,
                Some(fixture.fg_index),
                PPC_RGB_WHITE,
                &pattern,
            )
        } else {
            ppc_erase_rect(
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                (-3, -5, -2, 3),
                0,
                pattern,
                (PPC_RGB_BLACK, Some(fixture.fg_index)),
                (PPC_RGB_WHITE, Some(fixture.bg_index)),
            )
        };
        let row: Vec<_> = (0..8)
            .map(|i| loaded.memory.read_u8(PIXELS + i).unwrap())
            .collect();
        (wrote, row)
    };
    for import in [&b"PaintRect"[..], &b"EraseRect"[..]] {
        let (wrote, mut expected) = run(import, false);
        assert!(wrote);
        assert!(expected.iter().all(|&pixel| pixel != 0x33));
        // The two overlay bytes read back as the zeroed backing.
        expected[3..5].fill(0);
        let name = String::from_utf8_lossy(import);
        assert_eq!(run(import, true), (true, expected), "{name}");
    }
}

#[test]
fn ppc_pixel_fallback_stores_and_modifies_only_whole_accessible_pixels() {
    use crate::quickdraw::raster::memory::RasterMemory;
    const BASE: u32 = 0x7000_0000;
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"PaintRect")).unwrap();

    // A two-byte read-only overlay straddling the second and third 16-bit pixels.
    loaded.memory.add_region(BASE, vec![0x33; 8]);
    let mut backing = crate::memory::MacMemoryBus::new(0x1000);
    let overlay = backing.shared_ram_region(0, 2).unwrap();
    // SAFETY: this test serializes all access to the backing and the mapped view.
    unsafe {
        loaded
            .memory
            .add_shared_readonly_region(None, BASE + 3, overlay);
    }
    let mut memory = PpcPixelFallback {
        memory: &mut loaded.memory,
        pixel_len: 2,
    };
    assert_eq!(memory.write_row(BASE, &[1, 2, 3, 4, 5, 6, 7, 8]), Some(()));
    let mut actual = [0; 8];
    loaded.memory.read_bytes_into(BASE, &mut actual).unwrap();
    assert_eq!(actual, [1, 2, 0x33, 0, 0, 0x33, 7, 8]);

    // An unmapped middle pixel is skipped; the readable ones are modified.
    const GAP: u32 = 0x7100_0000;
    loaded.memory.add_region(GAP, vec![0x10, 0x11]);
    loaded.memory.add_region(GAP + 4, vec![0x14, 0x15]);
    let mut memory = PpcPixelFallback {
        memory: &mut loaded.memory,
        pixel_len: 2,
    };
    let mut scratch = [0; 6];
    let result = memory.modify_row(GAP, &mut scratch, |row| {
        row.iter_mut().for_each(|byte| *byte = !*byte)
    });
    assert_eq!(result, Some(()));
    let (mut head, mut tail) = ([0; 2], [0; 2]);
    loaded.memory.read_bytes_into(GAP, &mut head).unwrap();
    loaded.memory.read_bytes_into(GAP + 4, &mut tail).unwrap();
    assert_eq!((head, tail), ([0xef, 0xee], [0xeb, 0xea]));
}

#[test]
fn ppc_1bpp_rect_routes_store_writable_bytes_around_readonly_overlay() {
    // One 64-pixel row, so the whole bytes between the edges span the overlay.
    const ROW: (i16, i16, i16, i16) = (0, 0, 1, 64);
    let seed = vec![0x5a; 8];
    let fixture = IndexedPortFixture {
        bounds: ROW,
        row_bytes: 8,
        pixels: &seed,
        fg_index: 1,
        bg_index: 0,
        vis: (ROW, &[]),
        clip: (ROW, &[]),
    };
    let run = |import: &[u8], with_overlay: bool| {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(import)).unwrap();
        install_port(&mut loaded, &fixture, 1);
        let mut backing = crate::memory::MacMemoryBus::new(0x1000);
        if with_overlay {
            let overlay = backing.shared_ram_region(0, 2).unwrap();
            // SAFETY: this test serializes all access to the backing and the mapped view.
            unsafe {
                loaded
                    .memory
                    .add_shared_readonly_region(None, PIXELS + 3, overlay);
            }
        }
        let wrote = if import == b"InvertRect" {
            ppc_invert_rect_bounds(&mut loaded.memory, &loaded.gworlds, PORT, ROW)
        } else {
            ppc_paint_window_background_bounds(
                &mut loaded.memory,
                &loaded.gworlds,
                PORT,
                ROW,
                PPC_RGB_BLACK,
            )
        };
        let row: Vec<_> = (0..8)
            .map(|i| loaded.memory.read_u8(PIXELS + i).unwrap())
            .collect();
        (wrote, row)
    };
    for import in [&b"InvertRect"[..], &b"PaintRect"[..]] {
        let (wrote, mut expected) = run(import, false);
        assert!(wrote);
        assert!(expected.iter().all(|&byte| byte != 0x5a));
        // The two overlay bytes read back as the zeroed backing.
        expected[3..5].fill(0);
        let name = String::from_utf8_lossy(import);
        assert_eq!(run(import, true), (true, expected), "{name}");
    }
}
