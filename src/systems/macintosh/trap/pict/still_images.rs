use super::*;
use crate::trap::TrapDispatcher;
use image::ImageEncoder;

#[test]
fn indexed_quicktime_preserves_source_colors_on_direct_destinations() {
    let rle = [0, 0, 0, 13, 0, 0, 1, 1, 1, 1, 1, 1, 0xff];
    let smc = [0, 0, 0, 6, 0x60, 1];
    for (codec, compressed, height) in [(b"rle ", &rle[..], 1), (b"smc ", &smc[..], 4)] {
        for depth in [8, 16, 32] {
            let mut bus = MacMemoryBus::new(1024 * 1024);
            let pos = 0x1000;
            let description_size = 110;
            bus.write_long(pos, 68 + description_size + compressed.len() as u32);
            bus.write_word(pos + 4 + 52 + 4, height);
            bus.write_word(pos + 4 + 52 + 6, 4);
            let desc = pos + 4 + 68;
            bus.write_long(desc, description_size);
            bus.write_long(desc + 4, u32::from_be_bytes(*codec));
            bus.write_word(desc + 32, 4);
            bus.write_word(desc + 34, height);
            bus.write_long(desc + 44, compressed.len() as u32);
            bus.write_word(desc + 82, 8);
            bus.write_word(desc + 92, 1); // two ColorTable entries
            bus.write_word(desc + 102, 1);
            bus.write_word(desc + 104, 0x1234);
            bus.write_word(desc + 106, 0x5678);
            bus.write_word(desc + 108, 0x9abc);
            for (index, byte) in compressed.iter().enumerate() {
                bus.write_byte(desc + description_size + index as u32, *byte);
            }
            let clut = TrapDispatcher::standard_mac_8bpp_clut();
            let rb = 4 * u32::from(depth) / 8;
            let clip = DstClip::new((0, 0, 1, 1), vec![]);
            let clipped_pixel = 0x2000 + u32::from(depth) / 8;
            bus.write_long(clipped_pixel, 0xdeadbeef);
            parse_compressed_quicktime(
                &mut bus,
                pos,
                0,
                0,
                0,
                0,
                1.0,
                1.0,
                (0x2000, rb, 4, height, depth),
                &clut,
                None,
                Some(&clip),
            );
            match depth {
                8 => assert_eq!(
                    bus.read_byte(0x2000),
                    closest_clut_index(0x1234, 0x5678, 0x9abc, &clut)
                ),
                16 => assert_eq!(
                    bus.read_word(0x2000),
                    (0x1234 >> 11) << 10 | (0x5678 >> 11) << 5 | (0x9abcu16 >> 11)
                ),
                32 => assert_eq!(bus.read_long(0x2000), 0x0012569a),
                _ => unreachable!(),
            }
            assert_eq!(bus.read_long(clipped_pixel), 0xdeadbeef);
        }
    }
}

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
