use systemless_future::{
    bitmap::Bitmap,
    graphics::{DirectFormat, Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory, Permissions},
};

#[test]
#[ignore = "requires a real wgpu adapter"]
fn direct_gpu_storage_preserves_padding_clips_and_overlap() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 4, 2, GuestPtr(0x4000))).unwrap();
    for (format, bytes, white, red) in [
        (DirectFormat::Rgb555, 2, 0x7fff, 0x7c00),
        (DirectFormat::Xrgb8888, 4, 0xffffff, 0xff0000),
    ] {
        let row = 4 * bytes + 4;
        let original = vec![0xa5; row * 2];
        let mut memory = Memory::default();
        let base = GuestPtr(0x1001);
        memory.map(base, original.clone(), Permissions::RW).unwrap();
        let bitmap = Bitmap {
            base,
            row_bytes: row,
            bounds: Rect::new(-7, -3, -3, -1),
        };
        let whole = Rect::new(0, 0, 4, 2);
        let fill = |color, mode| Draw::Fill {
            rect: whole,
            clip: Rect::new(1, 0, 3, 1),
            pattern: [255; 8],
            foreground: color,
            background: white,
            mode,
        };
        gpu.execute_direct(
            &mut memory,
            bitmap,
            format,
            fill(red, Transfer::Copy),
            None,
            None,
        )
        .unwrap();
        let mut expected = original.clone();
        for x in 1..3 {
            let at = x * bytes;
            if bytes == 2 {
                expected[at..at + 2].copy_from_slice(&(red as u16).to_be_bytes());
            } else {
                expected[at..at + 4].copy_from_slice(&red.to_be_bytes());
            }
        }
        assert_eq!(memory.read(base, row * 2).unwrap(), expected);
        let rgba = gpu.read_rgba().unwrap();
        assert_eq!(&rgba[4..8], &[255, 0, 0, 255]);
        assert_eq!(&rgba[8..12], &[255, 0, 0, 255]);
        for _ in 0..2 {
            gpu.execute_direct(
                &mut memory,
                bitmap,
                format,
                fill(white, Transfer::Xor),
                None,
                None,
            )
            .unwrap();
        }
        assert_eq!(memory.read(base, row * 2).unwrap(), expected);
        let copy = Draw::Copy {
            rect: Rect::new(0, 0, 4, 1),
            clip: whole,
            source_x: -1,
            source_y: 0,
            mode: Transfer::Copy,
        };
        gpu.execute_direct(&mut memory, bitmap, format, copy, None, None)
            .unwrap();
        let source = expected.clone();
        for x in 1..4 {
            let at = x * bytes;
            if bytes == 2 {
                let v = u16::from_be_bytes(source[(x - 1) * 2..x * 2].try_into().unwrap()) & 0x7fff;
                expected[at..at + 2].copy_from_slice(&v.to_be_bytes());
            } else {
                expected[at..at + bytes].copy_from_slice(&source[(x - 1) * bytes..x * bytes]);
                expected[at] = 0;
            }
        }
        assert_eq!(memory.read(base, row * 2).unwrap(), expected);
        let saved = expected;
        assert!(gpu
            .execute_direct(
                &mut memory,
                bitmap,
                format,
                fill(white + 1, Transfer::Copy),
                None,
                None
            )
            .is_err());
        assert_eq!(memory.read(base, row * 2).unwrap(), saved);
        let bad_stride = Bitmap {
            row_bytes: row - 1,
            ..bitmap
        };
        assert!(gpu
            .execute_direct(
                &mut memory,
                bad_stride,
                format,
                fill(red, Transfer::Copy),
                None,
                None
            )
            .is_err());
        assert_eq!(memory.read(base, row * 2).unwrap(), saved);
        let readonly = GuestPtr(0x2001);
        memory
            .map(readonly, saved.clone(), Permissions::RX)
            .unwrap();
        assert!(gpu
            .execute_direct(
                &mut memory,
                Bitmap {
                    base: readonly,
                    ..bitmap
                },
                format,
                fill(red, Transfer::Copy),
                None,
                None
            )
            .is_err());
        assert_eq!(memory.read(readonly, row * 2).unwrap(), saved);
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn direct_rgb_source_keeps_more_than_256_colors_and_rejects_short_sources() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 300, 1, GuestPtr(0x4000))).unwrap();
    for (format, bytes, white) in [
        (DirectFormat::Rgb555, 2, 0x7fff),
        (DirectFormat::Xrgb8888, 4, 0xffffff),
    ] {
        let mut memory = Memory::default();
        let base = GuestPtr(0x1001);
        let row = 300 * bytes + 4;
        let original = vec![0xa5; row];
        memory.map(base, original.clone(), Permissions::RW).unwrap();
        let bitmap = Bitmap {
            base,
            row_bytes: row,
            bounds: Rect::new(0, 0, 300, 1),
        };
        let rect = Rect::new(0, 0, 300, 1);
        let draw = Draw::Blit {
            mask: None,
            rect,
            clip: rect,
            source: rect,
            width: 300,
            height: 1,
            foreground: 0,
            background: white,
            invert: false,
            mode: Transfer::Copy,
        };
        let colors: Vec<[u16; 3]> = (0..300u32)
            .map(|i| {
                if bytes == 2 {
                    [
                        ((i >> 5) * 65535 / 31) as u16,
                        ((i & 31) * 65535 / 31) as u16,
                        0,
                    ]
                } else {
                    [((i >> 8) * 257) as u16, ((i & 255) * 257) as u16, 0]
                }
            })
            .collect();
        assert!(gpu
            .execute_direct(
                &mut memory,
                bitmap,
                format,
                draw,
                None,
                Some(&colors[..299])
            )
            .is_err());
        assert_eq!(memory.read(base, row).unwrap(), original);
        gpu.execute_direct(&mut memory, bitmap, format, draw, None, Some(&colors))
            .unwrap();
        let output = memory.read(base, row).unwrap();
        for i in 0..300 {
            if bytes == 2 {
                let expected = (((i as u16) >> 5) << 10) | (((i as u16) & 31) << 5);
                assert_eq!(&output[i * 2..i * 2 + 2], &expected.to_be_bytes());
            } else {
                assert_eq!(&output[i * 4..i * 4 + 4], &[0, (i >> 8) as u8, i as u8, 0]);
            }
        }
        assert_eq!(&output[300 * bytes..], &[0xa5; 4]);
    }
}
