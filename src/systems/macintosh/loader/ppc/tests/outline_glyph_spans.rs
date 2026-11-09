use super::*;
use crate::memory::SavedPixels;

#[test]
fn outline_registration_matches_scalar_detail_across_clips_lanes_and_faults() {
    const WIDTH: u32 = 24;
    const HEIGHT: u32 = 20;
    const SCREEN: u32 = 0x1000;
    let (glyph, data) = get_glyph(PPC_QD_TEXT_FONT_DEFAULT, 12, 'W').unwrap();
    for depth in [8u32, 16] {
        let lanes = depth / 8;
        let stride = WIDTH * lanes + 5;
        let original: Vec<u8> = (0..stride * HEIGHT).map(|i| (i * 37 + 11) as u8).collect();
        for scale in [2, 3, 4] {
            for base in [SCREEN, 0x4_00f8, 0x4_0ff8] {
                for fault in [false, true] {
                    for bits in [
                        0,
                        QuickDrawTextStyle::BOLD_BIT | QuickDrawTextStyle::SHADOW_BIT,
                    ] {
                        let style = QuickDrawTextStyle::from_bits(bits);
                        let mut snapshots = Vec::new();
                        for scalar in [true, false] {
                            let mut memory = PpcSectionMem::new();
                            let gap = (10 * stride + 8 * lanes + lanes - 1) as usize;
                            if fault {
                                memory.add_region(base, original[..gap].to_vec());
                                memory.add_region(
                                    base + gap as u32 + 1,
                                    original[gap + 1..].to_vec(),
                                );
                                assert_eq!(memory.read_u8(base + gap as u32), None);
                            } else {
                                memory.add_region(base, original.clone());
                            }
                            let mut bus = MacMemoryBus::new(1024 * 1024);
                            bus.write_bytes(SCREEN, &original);
                            bus.enable_outline_presentation(
                                (SCREEN, stride, WIDTH as u16, HEIGHT as u16, depth as u16),
                                std::array::from_fn(|i| [i as u8; 3]),
                                scale,
                            );
                            memory
                                .shared_view()
                                .set_presentation(bus.presentation.clone());
                            let surface = PpcQuickDrawSurface {
                                front_buffer: PpcFrontBuffer {
                                    base_addr: base,
                                    row_bytes: stride,
                                    width: WIDTH,
                                    height: HEIGHT,
                                    depth,
                                },
                                top: -3,
                                left: -5,
                                ctable_handle: None,
                            };
                            // Buffer-space vis: x=2..22, y=2..18. Clip: y=4..16,
                            // with a hole x=10..14 on rows 4..8. Regions are in
                            // port coordinates, deliberately offset from the buffer.
                            let vis_rows = vec![vec![-3, 17]; 16];
                            let vis = ppc_region_storage_from_rows(-1, &vis_rows).unwrap();
                            let clip_rows: Vec<_> = (4..16)
                                .map(|y| {
                                    if y < 8 {
                                        vec![-5, 5, 9, 19]
                                    } else {
                                        vec![-5, 19]
                                    }
                                })
                                .collect();
                            let clip = ppc_region_storage_from_rows(1, &clip_rows).unwrap();
                            for color in [0x1439u16, 0x1439, 0x2812] {
                                if scalar {
                                    let mut slot = memory.presentation();
                                    slot.begin_outline_glyph(
                                        glyph,
                                        data,
                                        5,
                                        14,
                                        style.bold(),
                                        Some(3),
                                        Some((15, 1)),
                                    );
                                    slot.style_outline_glyph(style);
                                    let (top, left, bottom, right) = slot
                                        .as_ref()
                                        .unwrap()
                                        .glyph_bounds()
                                        .expect("bundled outline must be active");
                                    for y in top.max(0)..bottom.min(HEIGHT as i32) {
                                        for x in left.max(0)..right.min(WIDTH as i32) {
                                            if !(2..22).contains(&x)
                                                || !(4..16).contains(&y)
                                                || (y < 8 && (10..14).contains(&x))
                                            {
                                                continue;
                                            }
                                            let address =
                                                base + y as u32 * stride + x as u32 * lanes;
                                            for lane in 0..lanes {
                                                if let Some(background) =
                                                    memory.read_u8(address + lane)
                                                {
                                                    slot.as_mut().unwrap().glyph_pixel(
                                                        address + lane,
                                                        x as i16,
                                                        y as i16,
                                                        (color >> ((lanes - 1 - lane) * 8)) as u8,
                                                        background,
                                                    );
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    ppc_begin_outline_text_glyph(
                                        &mut memory,
                                        surface,
                                        Some(&vis),
                                        Some(&clip),
                                        glyph,
                                        data,
                                        5,
                                        14,
                                        color,
                                        style,
                                        Some(3),
                                        Some((15, 1)),
                                    );
                                }
                            }
                            let mut detail = SavedPixels::from(original.clone());
                            memory.presentation().capture_detail(
                                &mut detail,
                                0,
                                base,
                                original.len(),
                            );
                            assert!(
                                detail.has_detail_in(0..original.len()),
                                "comparison must include actual outline detail"
                            );
                            let bytes: Vec<_> = (0..original.len())
                                .map(|i| memory.read_u8(base + i as u32))
                                .collect();
                            assert_eq!(
                                bytes.iter().filter(|b| b.is_none()).count(),
                                usize::from(fault)
                            );
                            assert!(bytes.iter().zip(&original).all(|(actual, expected)| actual.is_none_or(|a| a == *expected)), "registration must not change guest bytes");
                            snapshots.push((detail, bytes, bus.outline_presentation_rgb()));
                        }
                        assert_eq!(
                            snapshots[1], snapshots[0],
                            "depth={depth} scale={scale} base={base:#x} fault={fault} style={bits}"
                        );
                    }
                }
            }
        }
    }
}
