use super::*;
use image::ImageEncoder;

#[test]
fn decompress_image_scales_into_offset_pixmaps_with_mask_and_transfer_mode() {
    for depth in [16, 32] {
        let mut app = load_pef_application(&synthetic_pef_with_library_import(
            b"QuickTimeLib",
            b"DecompressImage",
        ))
        .unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let pixels = PPC_HEAP_BASE + 0x6000;
        app.memory.add_region(scratch, vec![0; 2048]);
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                &[255, 0, 0, 0, 255, 0],
                2,
                1,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let mut desc = vec![0; 86];
        desc[..4].copy_from_slice(&86u32.to_be_bytes());
        desc[4..8].copy_from_slice(b"png ");
        desc[32..34].copy_from_slice(&2u16.to_be_bytes());
        desc[34..36].copy_from_slice(&1u16.to_be_bytes());
        desc[44..48].copy_from_slice(&(png.len() as u32).to_be_bytes());
        desc[82..84].copy_from_slice(&24u16.to_be_bytes());
        desc[84..86].copy_from_slice(&(-1i16).to_be_bytes());
        app.memory.write_u32_be(scratch, scratch + 128).unwrap();
        for (i, b) in desc.iter().enumerate() {
            app.memory.write_u8(scratch + 128 + i as u32, *b).unwrap();
        }
        test_handles!(app).push(PpcHandleRecord {
            handle: scratch,
            ptr: scratch + 128,
            size: 86,
            capacity: 86,
        });
        for (i, b) in png.iter().enumerate() {
            app.memory.write_u8(scratch + 512 + i as u32, *b).unwrap();
        }
        app.memory.write_u32_be(scratch + 4, scratch + 256).unwrap();
        let row_bytes = 4 * depth / 8 + 4;
        app.memory
            .add_region(pixels, vec![0xaa; (row_bytes * 4) as usize]);
        ppc_write_pixmap(
            &mut app.memory,
            scratch + 256,
            pixels,
            row_bytes,
            10,
            20,
            14,
            24,
            depth,
        )
        .unwrap();
        ppc_write_rect(&mut app.memory, scratch + 64, 0, 0, 1, 2).unwrap();
        ppc_write_rect(&mut app.memory, scratch + 72, 10, 20, 14, 24).unwrap();
        app.memory.write_u32_be(scratch + 8, scratch + 96).unwrap();
        app.memory.write_u16_be(scratch + 96, 10).unwrap();
        ppc_write_rect(&mut app.memory, scratch + 98, 11, 21, 13, 23).unwrap();
        let invoke = |app: &mut PpcLoadedApp, mode| {
            app.cpu.gpr[3..10].copy_from_slice(&[
                scratch + 512,
                scratch,
                scratch + 4,
                scratch + 64,
                scratch + 72,
                mode,
                scratch + 8,
            ]);
            run_test_import(app, PpcImportDispatcherTarget::QtDecompressImage);
        };
        invoke(&mut app, 0);
        assert_eq!(app.cpu.gpr[3], 0);
        let bits = ppc_read_pixmap_handle_bits(&mut app.memory, scratch + 4).unwrap();
        let red = if depth == 16 { 0x7c00 } else { 0xff0000 };
        let green = if depth == 16 { 0x03e0 } else { 0x00ff00 };
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut app.memory, bits, 21, 11),
            Some(red)
        );
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut app.memory, bits, 22, 12),
            Some(green)
        );
        assert_eq!(app.memory.read_u8(pixels), Some(0xaa));
        assert_eq!(app.memory.read_u8(pixels + row_bytes - 1), Some(0xaa));
        invoke(&mut app, 2);
        assert_eq!(app.cpu.gpr[3], 0);
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut app.memory, bits, 21, 11),
            Some(0)
        );
        // Sequence state owns the transform. Decode without touching the screen,
        // then redraw that saved frame with no source-data pointer.
        let port = scratch + 1024;
        app.memory.write_u32_be(port + 2, scratch + 4).unwrap();
        app.memory.write_u16_be(port + 6, 0xc000).unwrap();
        let matrix = scratch + 320;
        for (i, value) in [
            2 * 65536,
            0,
            0,
            0,
            4 * 65536,
            0,
            20 * 65536,
            10 * 65536,
            1 << 30,
        ]
        .iter()
        .enumerate()
        {
            app.memory
                .write_u32_be(matrix + i as u32 * 4, *value)
                .unwrap();
        }
        app.cpu.gpr[3..11].copy_from_slice(&[
            scratch + 12,
            scratch,
            port,
            0,
            scratch + 64,
            matrix,
            0,
            0,
        ]);
        app.cpu.gpr[1] = scratch + 1536;
        for (offset, value) in [(56, 3), (60, 1024), (64, 0)] {
            app.memory
                .write_u32_be(app.cpu.gpr[1] + offset, value)
                .unwrap();
        }
        run_test_import(
            &mut app,
            PpcImportDispatcherTarget::QtDecompressSequenceBegin,
        );
        assert_eq!(app.cpu.gpr[3], 0);
        let sequence = app.memory.read_u32_be(scratch + 12).unwrap();
        app.memory.write_u32_be(matrix, 0).unwrap();
        app.cpu.gpr[3..8].copy_from_slice(&[sequence, scratch + 512, 8, scratch + 20, 0]);
        run_test_import(
            &mut app,
            PpcImportDispatcherTarget::QtDecompressSequenceFrame,
        );
        assert_eq!(app.cpu.gpr[3], 0);
        assert_eq!(app.memory.read_u16_be(scratch + 20), Some(0xc000));
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut app.memory, bits, 21, 11),
            Some(0)
        );
        // DontOffscreen controls temporal decompression; a PNG ignores it.
        app.cpu.gpr[3..8].copy_from_slice(&[sequence, scratch + 512, 32 | 8, scratch + 20, 0]);
        run_test_import(
            &mut app,
            PpcImportDispatcherTarget::QtDecompressSequenceFrame,
        );
        assert_eq!(app.cpu.gpr[3], 0);
        assert_eq!(app.memory.read_u16_be(scratch + 20), Some(0x8000));
        let callback_code = scratch + 1200;
        let tvector = scratch + 1240;
        let completion = scratch + 1260;
        for (i, word) in [
            0x3cc0_0200u32,
            0x9066_1500,
            0x9086_1504,
            0x90a6_1508,
            0x9046_150c,
            0x4e80_0020,
        ]
        .iter()
        .enumerate()
        {
            app.memory
                .write_u32_be(callback_code + i as u32 * 4, *word)
                .unwrap();
        }
        app.memory.write_u32_be(tvector, callback_code).unwrap();
        app.memory.write_u32_be(tvector + 4, app.rtoc).unwrap();
        app.memory.write_u32_be(completion, tvector).unwrap();
        app.memory
            .write_u32_be(completion + 4, 0x1234_5678)
            .unwrap();
        app.cpu.gpr[3..8].copy_from_slice(&[sequence, 0, 256, scratch + 20, completion]);
        run_test_import(
            &mut app,
            PpcImportDispatcherTarget::QtDecompressSequenceFrame,
        );
        assert_eq!(app.cpu.gpr[3], 0);
        assert_eq!(app.memory.read_u32_be(scratch + 1280), Some(0));
        assert_eq!(app.memory.read_u32_be(scratch + 1284), Some(11));
        assert_eq!(app.memory.read_u32_be(scratch + 1288), Some(0x1234_5678));
        assert_eq!(app.memory.read_u32_be(scratch + 1292), Some(app.rtoc));
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut app.memory, bits, 20, 10),
            Some(red)
        );
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut app.memory, bits, 23, 13),
            Some(green)
        );
        // -1 requests asynchronous completion without a completion routine.
        app.cpu.gpr[3..8].copy_from_slice(&[sequence, 0, 256, scratch + 20, u32::MAX]);
        run_test_import(
            &mut app,
            PpcImportDispatcherTarget::QtDecompressSequenceFrame,
        );
        assert_eq!(app.cpu.gpr[3], 0);
        app.cpu.gpr[3] = sequence;
        run_test_import(&mut app, PpcImportDispatcherTarget::QtCDSequenceEnd);
        assert_eq!(app.cpu.gpr[3], 0);
        app.cpu.gpr[3] = sequence;
        run_test_import(&mut app, PpcImportDispatcherTarget::QtCDSequenceEnd);
        assert_eq!(app.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        // A disposed sequence reports its error through the completion routine,
        // restores the caller TOC, and leaves the output flags untouched.
        app.memory.write_u16_be(scratch + 20, 0x55aa).unwrap();
        app.memory
            .write_u32_be(scratch + 1280, 0xdead_beef)
            .unwrap();
        app.cpu.gpr[3..8].copy_from_slice(&[sequence, 0, 256, scratch + 20, completion]);
        run_test_import(
            &mut app,
            PpcImportDispatcherTarget::QtDecompressSequenceFrame,
        );
        assert_eq!(app.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(
            app.memory.read_u32_be(scratch + 1280),
            Some(ppc_i16_result(PPC_PARAM_ERR))
        );
        assert_eq!(app.memory.read_u32_be(scratch + 1284), Some(3));
        assert_eq!(app.memory.read_u32_be(scratch + 1288), Some(0x1234_5678));
        assert_eq!(app.memory.read_u32_be(scratch + 1292), Some(app.rtoc));
        assert_eq!(app.memory.read_u16_be(scratch + 20), Some(0x55aa));
        // An inaccessible completion record cannot trigger a guest call.
        app.memory
            .write_u32_be(scratch + 1280, 0xdead_beef)
            .unwrap();
        app.cpu.gpr[3..8].copy_from_slice(&[sequence, 0, 256, scratch + 20, 0xffff_fffc]);
        run_test_import(
            &mut app,
            PpcImportDispatcherTarget::QtDecompressSequenceFrame,
        );
        assert_eq!(app.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(app.memory.read_u32_be(scratch + 1280), Some(0xdead_beef));
        // A stateless sequence without an image buffer can decode with
        // DontOffscreen/DontUseNewImageBuffer, but cannot redraw old pixels.
        for screen_flag in [0, 2] {
            app.memory.write_u32_be(matrix, 2 * 65536).unwrap();
            app.memory
                .write_u32_be(app.cpu.gpr[1] + 56, screen_flag)
                .unwrap();
            app.cpu.gpr[3..11].copy_from_slice(&[
                scratch + 12,
                scratch,
                port,
                0,
                scratch + 64,
                matrix,
                0,
                0,
            ]);
            run_test_import(
                &mut app,
                PpcImportDispatcherTarget::QtDecompressSequenceBegin,
            );
            assert_eq!(app.cpu.gpr[3], 0);
            let unbuffered = app.memory.read_u32_be(scratch + 12).unwrap();
            assert_ne!(unbuffered, sequence);
            // NoScreenUpdate does not require a retained image buffer. Decode
            // and discard the image, without touching the destination.
            ppc_write_pixmap_raw_pixel(&mut app.memory, bits, 21, 11, 0).unwrap();
            app.cpu.gpr[3..8].copy_from_slice(&[
                unbuffered,
                scratch + 512,
                8,
                scratch + 20,
                completion,
            ]);
            run_test_import(
                &mut app,
                PpcImportDispatcherTarget::QtDecompressSequenceFrame,
            );
            assert_eq!(app.cpu.gpr[3], 0);
            assert_eq!(app.memory.read_u16_be(scratch + 20), Some(0));
            assert_eq!(app.memory.read_u32_be(scratch + 1284), Some(3));
            assert_eq!(
                ppc_read_pixmap_raw_pixel(&mut app.memory, bits, 21, 11),
                Some(0)
            );
            app.cpu.gpr[3..8].copy_from_slice(&[
                unbuffered,
                scratch + 512,
                if screen_flag == 0 { 32 | 1024 } else { 1024 },
                scratch + 20,
                completion,
            ]);
            run_test_import(
                &mut app,
                PpcImportDispatcherTarget::QtDecompressSequenceFrame,
            );
            assert_eq!(app.cpu.gpr[3], 0);
            assert_eq!(app.memory.read_u16_be(scratch + 20), Some(0));
            assert_eq!(
                app.memory.read_u32_be(scratch + 1284),
                Some(if screen_flag == 0 { 3 } else { 11 })
            );
            assert_eq!(
                ppc_read_pixmap_raw_pixel(&mut app.memory, bits, 21, 11),
                Some(red)
            );
            app.cpu.gpr[3..8].copy_from_slice(&[unbuffered, 0, 256, scratch + 20, 0]);
            run_test_import(
                &mut app,
                PpcImportDispatcherTarget::QtDecompressSequenceFrame,
            );
            assert_eq!(app.cpu.gpr[3], ppc_i16_result(-8965));
            app.cpu.gpr[3] = unbuffered;
            run_test_import(&mut app, PpcImportDispatcherTarget::QtCDSequenceEnd);
            assert_eq!(app.cpu.gpr[3], 0);
        }
        app.memory.write_u32_be(scratch + 128, 17).unwrap();
        invoke(&mut app, 0);
        assert_eq!(app.cpu.gpr[3], ppc_i16_result(-8969));
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut app.memory, bits, 21, 11),
            Some(red)
        );
    }
}

#[test]
fn image_description_color_table_returns_an_owned_copy_and_noncustom_marker() {
    let mut app = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"GetImageDescriptionCTable",
    ))
    .unwrap();
    let scratch = PPC_DATA_BASE + 0x2000;
    app.memory.add_region(scratch, vec![0; 256]);
    let mut description = vec![0; 86];
    let table = [
        0, 0, 0, 42, 0, 0, 0, 1, 0, 0, 255, 255, 0, 0, 0, 0, 0, 1, 0, 0, 255, 255, 0, 0,
    ];
    description[..4].copy_from_slice(&110u32.to_be_bytes());
    description[4..8].copy_from_slice(b"raw ");
    description[32..34].copy_from_slice(&2u16.to_be_bytes());
    description[34..36].copy_from_slice(&1u16.to_be_bytes());
    description[82..84].copy_from_slice(&8u16.to_be_bytes());
    description.extend_from_slice(&table);
    app.memory.write_u32_be(scratch, scratch + 128).unwrap();
    for (i, byte) in description.iter().enumerate() {
        app.memory
            .write_u8(scratch + 128 + i as u32, *byte)
            .unwrap();
    }
    test_handles!(app).push(PpcHandleRecord {
        handle: scratch,
        ptr: scratch + 128,
        size: 110,
        capacity: 110,
    });
    app.cpu.gpr[3] = scratch;
    app.cpu.gpr[4] = scratch + 16;
    run_test_import(
        &mut app,
        PpcImportDispatcherTarget::QtGetImageDescriptionCTable,
    );
    assert_eq!(app.cpu.gpr[3], 0);
    let handle = app.memory.read_u32_be(scratch + 16).unwrap();
    let ptr = app.memory.read_u32_be(handle).unwrap();
    let mut copied = [0; 24];
    app.memory.read_bytes_into(ptr, &mut copied).unwrap();
    assert_eq!(copied, table);
    assert_eq!(
        test_handles!(app)
            .iter()
            .find(|r| r.handle == handle)
            .unwrap()
            .size,
        24
    );
    app.memory.write_u16_be(scratch + 128 + 96, 0).unwrap();
    assert_eq!(app.memory.read_u16_be(ptr + 10), Some(65535));
    app.memory.write_u16_be(scratch + 128 + 84, 0xffff).unwrap();
    app.memory.write_u16_be(scratch + 128 + 82, 24).unwrap();
    app.cpu.gpr[3] = scratch;
    app.cpu.gpr[4] = scratch + 16;
    run_test_import(
        &mut app,
        PpcImportDispatcherTarget::QtGetImageDescriptionCTable,
    );
    assert_eq!(app.cpu.gpr[3], 0);
    let plain_handle = app.memory.read_u32_be(scratch + 16).unwrap();
    let plain = app.memory.read_u32_be(plain_handle).unwrap();
    assert_ne!(plain_handle, handle);
    assert_eq!(app.memory.read_u32_be(plain), Some(u32::MAX));
    assert_eq!(app.memory.read_u16_be(plain + 6), Some(0));
    app.memory.write_u32_be(scratch + 128, 16).unwrap();
    app.cpu.gpr[3] = scratch;
    app.cpu.gpr[4] = scratch + 16;
    run_test_import(
        &mut app,
        PpcImportDispatcherTarget::QtGetImageDescriptionCTable,
    );
    assert_eq!(app.cpu.gpr[3], ppc_i16_result(-8969));
    assert_eq!(app.memory.read_u32_be(scratch + 16), Some(plain_handle));
}

#[test]
fn decompress_raw_dither_copy_preserves_average_color_at_indexed_and_16bit_depths() {
    for depth in [1, 2, 4, 8, 16] {
        let mut app = load_pef_application(&synthetic_pef_with_library_import(
            b"QuickTimeLib",
            b"DecompressImage",
        ))
        .unwrap();
        let scratch = PPC_DATA_BASE + 0x3000;
        let pixels = PPC_HEAP_BASE + 0x8000;
        app.memory.add_region(scratch, vec![0; 4096]);
        let mut description = vec![0; 86];
        description[..4].copy_from_slice(&86u32.to_be_bytes());
        description[4..8].copy_from_slice(b"raw ");
        description[32..34].copy_from_slice(&16u16.to_be_bytes());
        description[34..36].copy_from_slice(&8u16.to_be_bytes());
        description[82..84].copy_from_slice(&24u16.to_be_bytes());
        description[84..86].copy_from_slice(&(-1i16).to_be_bytes());
        app.memory.write_u32_be(scratch, scratch + 128).unwrap();
        for (i, b) in description.iter().enumerate() {
            app.memory.write_u8(scratch + 128 + i as u32, *b).unwrap();
        }
        test_handles!(app).push(PpcHandleRecord {
            handle: scratch,
            ptr: scratch + 128,
            size: 86,
            capacity: 86,
        });
        for i in 0..384 {
            app.memory.write_u8(scratch + 512 + i, 137).unwrap();
        }
        let row_bytes = 16 * depth / 8 + 2;
        app.memory
            .add_region(pixels, vec![0xaa; (row_bytes * 8) as usize]);
        app.memory.write_u32_be(scratch + 4, scratch + 256).unwrap();
        ppc_write_pixmap(
            &mut app.memory,
            scratch + 256,
            pixels,
            row_bytes,
            0,
            0,
            8,
            16,
            depth,
        )
        .unwrap();
        let clut = if depth <= 8 {
            app.cpu.gpr[3] = depth;
            run_test_import(&mut app, PpcImportDispatcherTarget::GetCTable);
            let handle = app.cpu.gpr[3];
            assert_ne!(handle, 0);
            app.memory.write_u32_be(scratch + 256 + 42, handle).unwrap();
            Some(ppc_read_ctable_clut(&mut app.memory, handle, &[[0; 3]; 256]).unwrap())
        } else {
            None
        };
        ppc_write_rect(&mut app.memory, scratch + 64, 0, 0, 8, 16).unwrap();
        let bits = ppc_read_pixmap_handle_bits(&mut app.memory, scratch + 4).unwrap();
        let mut averages = Vec::new();
        let mut samples = Vec::new();
        for mode in [0, 64] {
            app.cpu.gpr[3..10].copy_from_slice(&[
                scratch + 512,
                scratch,
                scratch + 4,
                0,
                scratch + 64,
                mode,
                0,
            ]);
            run_test_import(&mut app, PpcImportDispatcherTarget::QtDecompressImage);
            assert_eq!(app.cpu.gpr[3], 0);
            let mut total = [0u64; 3];
            let mut pixels_seen = Vec::new();
            for y in 0..8 {
                for x in 0..16 {
                    let pixel = ppc_read_pixmap_raw_pixel(&mut app.memory, bits, x, y).unwrap();
                    pixels_seen.push(pixel);
                    let rgb = ppc_pixmap_pixel_rgb(bits, pixel, clut.as_ref()).unwrap();
                    for i in 0..3 {
                        total[i] += u64::from(rgb[i]);
                    }
                }
            }
            averages.push(total.map(|v| v / 128));
            samples.push(pixels_seen);
        }
        let error = |rgb: [u64; 3]| rgb.into_iter().map(|v| v.abs_diff(137 * 257)).sum::<u64>();
        assert!(
            error(averages[1]) <= error(averages[0]),
            "depth {depth}: {averages:?}"
        );
        if depth == 1 || depth == 16 {
            assert!(error(averages[1]) < error(averages[0]));
            assert_ne!(samples[0], samples[1]);
        }
        for y in 0..8 {
            assert_eq!(
                app.memory.read_u8(pixels + y * row_bytes + row_bytes - 1),
                Some(0xaa)
            );
        }
    }
}
