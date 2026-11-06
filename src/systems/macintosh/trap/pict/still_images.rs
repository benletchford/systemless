use super::*;
use crate::trap::TrapDispatcher;
use image::ImageEncoder;

fn png_record(bus: &mut MacMemoryBus, damaged: bool) -> (u32, u32) {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &[255, 0, 0, 0, 255, 0],
            2,
            1,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    if damaged {
        png.fill(0);
    }
    let pos = 0x1000;
    let payload = 68 + 86 + png.len() as u32;
    bus.write_long(pos, payload);
    bus.write_long(pos + 6, 0x10000);
    bus.write_long(pos + 22, 0x10000);
    bus.write_long(pos + 38, 0x4000_0000);
    bus.write_word(pos + 4 + 52 + 4, 1); // source bottom
    bus.write_word(pos + 4 + 52 + 6, 2); // source right
    let desc = pos + 4 + 68;
    bus.write_long(desc, 86);
    bus.write_long(desc + 4, u32::from_be_bytes(*b"png "));
    bus.write_word(desc + 32, 2);
    bus.write_word(desc + 34, 1);
    bus.write_long(desc + 44, png.len() as u32);
    bus.write_word(desc + 82, 24);
    for (i, byte) in png.iter().enumerate() {
        bus.write_byte(desc + 86 + i as u32, *byte);
    }
    let end = align_pict_pos(pos + 4 + payload);
    bus.write_long(end, 0x0007_00AE);
    bus.write_word(end + 4, 4);
    (pos, end)
}

#[test]
fn quicktime_png_draws_direct_and_indexed_colours_and_suppresses_fallback() {
    for depth in [8, 16, 32] {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let (pos, end) = png_record(&mut bus, false);
        let base = 0x2000;
        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        let rb = 4 * u32::from(depth) / 8;
        let (next, palette) = parse_compressed_quicktime(
            &mut bus,
            pos,
            0,
            1,
            0,
            0,
            1.0,
            1.0,
            (base, rb, 4, 2, depth),
            &clut,
            None,
            None,
        );
        assert_eq!(next, end + 10);
        assert!(palette.is_none());
        match depth {
            32 => {
                assert_eq!(bus.read_long(base + 4), 0xFF0000);
                assert_eq!(bus.read_long(base + 8), 0x00FF00);
            }
            16 => {
                assert_eq!(bus.read_word(base + 2), 0x7C00);
                assert_eq!(bus.read_word(base + 4), 0x03E0);
            }
            _ => {
                assert_eq!(
                    bus.read_byte(base + 1),
                    closest_clut_index(65535, 0, 0, &clut)
                );
                assert_eq!(
                    bus.read_byte(base + 2),
                    closest_clut_index(0, 65535, 0, &clut)
                );
            }
        }
        assert_eq!(bus.read_byte(base), 0);
        assert_eq!(bus.read_long(base + rb), 0);
    }
}

#[test]
fn damaged_quicktime_png_preserves_destination_and_warning() {
    let mut bus = MacMemoryBus::new(1024 * 1024);
    let (pos, end) = png_record(&mut bus, true);
    bus.write_long(0x2000, 0x12345678);
    let (next, palette) = parse_compressed_quicktime(
        &mut bus,
        pos,
        0,
        0,
        0,
        0,
        1.0,
        1.0,
        (0x2000, 16, 4, 2, 32),
        &[[0; 3]; 256],
        None,
        None,
    );
    assert_eq!(next, end);
    assert!(palette.is_none());
    assert_eq!(bus.read_long(0x2000), 0x12345678);
}

#[test]
fn quicktime_png_obeys_source_crop_and_destination_clip() {
    for crop_source in [false, true] {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let (pos, _) = png_record(&mut bus, false);
        if crop_source {
            bus.write_word(pos + 4 + 52 + 2, 1); // source left excludes red
        }
        bus.write_long(0x2000, 0x00112233);
        bus.write_long(0x2004, 0x00445566);
        let clip = DstClip::new((0, 1, 1, 2), vec![]);
        parse_compressed_quicktime(
            &mut bus,
            pos,
            0,
            0,
            0,
            0,
            1.0,
            1.0,
            (0x2000, 16, 4, 2, 32),
            &[[0; 3]; 256],
            None,
            if crop_source { None } else { Some(&clip) },
        );
        assert_eq!(bus.read_long(0x2000), 0x00112233);
        assert_eq!(bus.read_long(0x2004), 0x0000FF00);
    }
}

#[test]
fn compressed_picture_matrix_translates_scales_and_rotates_pixels() {
    for (matrix, expected) in [
        ([0x10000, 0, 0, 0x10000, 0x10000, 0x10000], [(1, 1), (2, 1)]),
        ([0, 0x10000, -0x10000, 0, 0x20000, 0], [(1, 0), (1, 1)]),
        ([0x20000, 0, 0, 0x10000, 0, 0], [(0, 0), (2, 0)]),
        ([-0x10000, 0, 0, 0x10000, 0x20000, 0], [(1, 0), (0, 0)]),
        ([0x10000, 0x10000, 0, 0x10000, 0, 0], [(0, 0), (1, 1)]),
    ] {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let (pos, _) = png_record(&mut bus, false);
        for (offset, value) in [0, 4, 12, 16, 24, 28].into_iter().zip(matrix) {
            bus.write_long(pos + 6 + offset, value as u32);
        }
        let base = 0x2000;
        parse_compressed_quicktime(
            &mut bus,
            pos,
            0,
            0,
            0,
            0,
            1.0,
            1.0,
            (base, 16, 4, 4, 32),
            &[[0; 3]; 256],
            None,
            None,
        );
        for ((x, y), color) in expected.into_iter().zip([0xFF0000, 0x00FF00]) {
            assert_eq!(bus.read_long(base + y * 16 + x * 4), color);
        }
        if matrix[0] == 0x20000 {
            assert_eq!(bus.read_long(base + 4), 0xFF0000);
            assert_eq!(bus.read_long(base + 12), 0x00FF00);
        }
    }
}

#[test]
fn compressed_picture_matrix_samples_small_nonzero_scale() {
    let matrix = PictAffine {
        a: 1.0 / 65536.0,
        b: 0.0,
        c: 0.0,
        d: 1.0 / 65536.0,
        tx: 128.0 - 1.0 / 65536.0,
        ty: 128.0 - 1.0 / 65536.0,
    };
    let samples: Vec<_> = matrix
        .sample_points(
            (0, 0, 2, 2),
            (0, 0),
            (0, 0),
            (1.0 / 256.0, 1.0 / 256.0),
            (1, 1),
        )
        .collect();
    assert_eq!(samples, vec![(1, 1, 0, 0, 128, 128)]);
}

#[test]
fn compressed_picture_clip_uses_transformed_picture_coordinates() {
    for scale in [1, 2] {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let (pos, _) = png_record(&mut bus, false);
        bus.write_long(pos + 30, 1 << 16); // translate right one
        bus.write_long(pos + 34, 1 << 16); // translate down one
        let clip = PictureRegion {
            top: 1,
            left: 2,
            bottom: 2,
            right: 3,
            rows: vec![],
        };
        parse_compressed_quicktime(
            &mut bus,
            pos,
            0,
            0,
            0,
            0,
            f64::from(scale),
            f64::from(scale),
            (0x2000, 32, 8, 8, 32),
            &[[0; 3]; 256],
            Some(&clip),
            None,
        );
        for y in scale..2 * scale {
            for x in scale..3 * scale {
                let pixel = bus.read_long(0x2000 + y * 32 + x * 4);
                assert_eq!(pixel, if x >= 2 * scale { 0x00ff00 } else { 0 });
            }
        }
    }
}

#[test]
fn compressed_picture_fractional_translation_obeys_nonrectangular_clip() {
    let mut bus = MacMemoryBus::new(1024 * 1024);
    let (pos, _) = png_record(&mut bus, false);
    bus.write_long(pos + 30, 0x8000); // translate right half a pixel
    let clip = PictureRegion {
        top: 0,
        left: 0,
        bottom: 1,
        right: 2,
        rows: vec![vec![1, 2]],
    };
    parse_compressed_quicktime(
        &mut bus,
        pos,
        0,
        0,
        0,
        0,
        2.0,
        2.0,
        (0x2000, 24, 6, 2, 32),
        &[[0; 3]; 256],
        Some(&clip),
        None,
    );
    for row in 0..2 {
        let base = 0x2000 + row * 24;
        assert_eq!(bus.read_long(base + 4), 0);
        assert_eq!(bus.read_long(base + 8), 0x00ff0000);
        assert_eq!(bus.read_long(base + 12), 0x0000ff00);
        assert_eq!(bus.read_long(base + 16), 0);
    }
}

#[test]
fn compressed_picture_matrix_applies_to_smc_and_rle() {
    for format in 0..3 {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let pos = 0x1000;
        let (data, width, height, depth, codec): (&[u8], u16, u16, u16, [u8; 4]) = match format {
            0 => (
                &[0, 0, 0, 13, 0, 0, 1, 2, 0x7c, 0x00, 0x03, 0xe0, 0xff],
                2,
                1,
                16,
                *b"rle ",
            ),
            1 => (&[0, 0, 0, 6, 0x60, 1], 4, 4, 8, *b"smc "),
            _ => (
                &[0, 0, 0, 13, 0, 0, 1, 1, 1, 1, 1, 1, 0xff],
                4,
                1,
                8,
                *b"rle ",
            ),
        };
        bus.write_long(pos, 68 + 86 + data.len() as u32);
        bus.write_long(pos + 6, 0x10000);
        bus.write_long(pos + 22, 0x10000);
        bus.write_long(pos + 30, 1 << 16);
        bus.write_long(pos + 34, 1 << 16);
        bus.write_long(pos + 38, 0x4000_0000);
        bus.write_word(pos + 4 + 52 + 4, height);
        bus.write_word(pos + 4 + 52 + 6, width);
        let desc = pos + 4 + 68;
        bus.write_long(desc, 86);
        bus.write_long(desc + 4, u32::from_be_bytes(codec));
        bus.write_word(desc + 32, width);
        bus.write_word(desc + 34, height);
        bus.write_long(desc + 44, data.len() as u32);
        bus.write_word(desc + 82, depth);
        bus.write_bytes(desc + 86, data);
        let base = 0x2000;
        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        let output_depth = if format == 0 { 16 } else { 8 };
        let rb = 8 * u32::from(output_depth) / 8;
        parse_compressed_quicktime(
            &mut bus,
            pos,
            0,
            0,
            0,
            0,
            1.0,
            1.0,
            (base, rb, 8, 8, output_depth),
            &clut,
            None,
            None,
        );
        if format != 0 {
            assert_eq!(bus.read_byte(base), 0);
            assert_eq!(bus.read_byte(base + rb + 1), 255);
            assert_eq!(bus.read_byte(base + u32::from(height) * rb + 4), 255);
            assert_eq!(bus.read_byte(base + (u32::from(height) + 1) * rb + 4), 0);
        } else {
            assert_eq!(bus.read_word(base), 0);
            assert_eq!(bus.read_word(base + rb + 2), 0x7c00);
            assert_eq!(bus.read_word(base + rb + 4), 0x03e0);
        }
    }
}

#[test]
fn quicktime_matrix_translation_positions_pixels_in_picture_space() {
    for depth in [8, 16, 32] {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let (pos, _) = png_record(&mut bus, false);
        // MatrixRecord's translation is a signed 16.16 point, in picture coordinates.
        bus.write_long(pos + 4 + 2 + 24, 1 << 16);
        bus.write_long(pos + 4 + 2 + 28, 1 << 16);
        let base = 0x2000;
        let rb = 4 * u32::from(depth) / 8;
        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        parse_compressed_quicktime(
            &mut bus, pos, 0, 0, 0, 0, 1.0, 1.0,
            (base, rb, 4, 3, depth), &clut, None, None,
        );
        let red = base + rb + u32::from(depth) / 8;
        let green = red + u32::from(depth) / 8;
        match depth {
            8 => {
                assert_eq!(bus.read_byte(red), closest_clut_index(65535, 0, 0, &clut));
                assert_eq!(bus.read_byte(green), closest_clut_index(0, 65535, 0, &clut));
            }
            16 => {
                assert_eq!(bus.read_word(red), 0x7c00);
                assert_eq!(bus.read_word(green), 0x03e0);
            }
            32 => {
                assert_eq!(bus.read_long(red), 0x00ff0000);
                assert_eq!(bus.read_long(green), 0x0000ff00);
            }
            _ => unreachable!(),
        }
        assert_eq!(bus.read_long(base), 0);
    }
}

#[test]
fn quicktime_translation_precedes_picture_clip_and_destination_mapping() {
    let mut bus = MacMemoryBus::new(1024 * 1024);
    let (pos, _) = png_record(&mut bus, false);
    bus.write_long(pos + 4 + 2 + 24, (-1i32 << 16) as u32);
    bus.write_long(pos + 4 + 2 + 28, 1 << 16);
    // The translated red pixel is at (-1, 1), and the green pixel at (0, 1).
    let picture_clip = PictureRegion { top: 1, left: 0, bottom: 2, right: 1, rows: vec![] };
    parse_compressed_quicktime(
        &mut bus, pos, 0, 1, 0, 0, 1.0, 1.0,
        (0x2000, 16, 4, 3, 32), &[[0; 3]; 256], Some(&picture_clip), None,
    );
    assert_eq!(bus.read_long(0x2000 + 16), 0);
    assert_eq!(bus.read_long(0x2000 + 16 + 4), 0x0000ff00);
}

#[test]
fn quicktime_warning_fingerprint_does_not_skip_other_bitmaps() {
    let mut bus = MacMemoryBus::new(1024 * 1024);
    let pos = 0x3000;
    bus.write_word(pos, 0x0098);
    bus.write_word(pos + 2, 10);
    bus.write_long(pos + 8, 0x002c_0045);
    bus.write_long(pos + 16, 0x002c_0045);
    let mut fingerprint = 0xcbf2_9ce4_8422_2325u64;
    for row in 0..44 {
        let start = pos + 30 + row * 2;
        bus.write_byte(start, 1);
        bus.write_byte(start + 1, row as u8);
        for byte in [1, row as u8] {
            fingerprint ^= u64::from(byte);
            fingerprint = fingerprint.wrapping_mul(0x100_0000_01b3);
        }
    }
    assert_eq!(skip_quicktime_packbits_warning(&bus, pos, fingerprint), Some(pos + 118));
    assert_eq!(skip_quicktime_packbits_warning(&bus, pos, fingerprint ^ 1), None);
    bus.write_word(pos + 8, 43);
    assert_eq!(skip_quicktime_packbits_warning(&bus, pos, fingerprint), None);
}
