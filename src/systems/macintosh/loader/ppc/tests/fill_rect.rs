//! FillRect through the shared raster core: against the 68K FillRect path,
//! against the earlier PowerPC solid fill, and the cases the core declines.

use super::*;
use crate::memory::write_log;
use crate::trap::test_helpers::{fill_rect_8bpp_68k, region_storage_bytes, IndexedPortFixture};

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
fn fill_rect_keeps_the_solid_fill_at_depths_the_core_declines() {
    let pixels = initial_pixels();
    let fixture = fixture(&pixels, WHOLE_PORT, CLIP_RECT);
    for depth in [1, 2, 4] {
        let (filled, solid) = fill_rect_and_solid_fill(&fixture, depth, (-6, -4, 5, 30), GRAY);
        assert_eq!(filled, solid, "{depth} bpp");
        assert_ne!(filled, pixels, "{depth} bpp must still draw");
    }
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
