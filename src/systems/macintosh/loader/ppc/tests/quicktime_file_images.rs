use super::*;
use image::ImageEncoder;

const SCRATCH: u32 = PPC_DATA_BASE + 0x1000;
const IMPORTER_OUT: u32 = SCRATCH + 80;
const PORT: u32 = 0x0600_1000;
const PIXELS: u32 = PPC_HEAP_BASE + 0x6000;

fn import_file(name: &[u8], data: Vec<u8>) -> PpcLoadedApp {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"GetGraphicsImporterForFile",
    ))
    .unwrap();
    loaded.memory.add_region(SCRATCH, vec![0; 128]);
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: String::from_utf8(name.to_vec()).unwrap(),
        data: data.into(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"PICT"), // Content must override stale Finder metadata.
        finder_flags: 0,
        dirty: false,
    });
    write_ppc_fsspec(
        &mut loaded.memory,
        SCRATCH,
        PPC_BOOT_VOLUME_REF_NUM,
        PPC_ROOT_DIR_ID,
        name,
    );
    loaded.cpu.gpr[3] = SCRATCH;
    loaded.cpu.gpr[4] = IMPORTER_OUT;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QtGetGraphicsImporterForFile,
    );
    loaded
}

fn jpeg() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 100)
        .encode(
            &[220, 30, 20].repeat(64),
            8,
            8,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    bytes
}

fn destination(loaded: &mut PpcLoadedApp, depth: u32) -> u32 {
    let row_bytes = 8 * depth / 8 + 4;
    loaded
        .memory
        .add_region(PIXELS, vec![0xaa; (row_bytes * 8) as usize]);
    loaded.memory.add_region(PORT, vec![0; 256]);
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PORT,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: PIXELS,
        gdevice: 0,
        width: 8,
        height: 8,
        depth,
        row_bytes,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = PORT;
    loaded.cpu.gpr[5] = 0;
    run_test_import(loaded, PpcImportDispatcherTarget::QtGraphicsImportSetGWorld);
    assert_eq!(loaded.cpu.gpr[3], 0);
    row_bytes
}

fn draw(loaded: &mut PpcLoadedApp) {
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    run_test_import(loaded, PpcImportDispatcherTarget::QtGraphicsImportDraw);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn extensionless_jpeg_file_reports_metadata_and_draws_real_pixels_at_both_direct_depths() {
    for depth in [16, 32] {
        let bytes = jpeg();
        let encoded_size = bytes.len() as u32;
        let mut loaded = import_file(b"Title", bytes);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(
            loaded.memory.read_u32_be(IMPORTER_OUT),
            Some(PPC_QT_GRAPHICS_IMPORTER)
        );
        assert_eq!(
            loaded.quicktime.graphics_importer_bounds,
            Some((0, 0, 8, 8))
        );
        assert_eq!(
            loaded.quicktime.graphics_importer_subtype,
            u32::from_be_bytes(*b"jpeg")
        );
        loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
        loaded.cpu.gpr[4] = SCRATCH + 88;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QtGraphicsImportGetImageDescription,
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        let handle = loaded.memory.read_u32_be(SCRATCH + 88).unwrap();
        let description = loaded.memory.read_u32_be(handle).unwrap();
        assert_eq!(
            loaded.memory.read_u32_be(description + 4),
            Some(u32::from_be_bytes(*b"jpeg"))
        );
        assert_eq!(
            loaded.memory.read_u32_be(description + 44),
            Some(encoded_size)
        );
        assert_eq!(loaded.memory.read_u16_be(description + 82), Some(24));
        let row_bytes = destination(&mut loaded, depth);
        draw(&mut loaded);
        assert_eq!(loaded.quicktime.graphics_import_source_draw_count, 1);
        for y in 0..8 {
            for x in 0..8 {
                let at = PIXELS + y * row_bytes + x * (depth / 8);
                if depth == 32 {
                    let color = loaded.memory.read_u32_be(at).unwrap();
                    assert!((color >> 16) & 255 > 210);
                    assert!((color >> 8) & 255 < 40);
                    assert!(color & 255 < 30);
                } else {
                    let color = loaded.memory.read_u16_be(at).unwrap();
                    assert!((color >> 10) & 31 >= 26);
                    assert!((color >> 5) & 31 <= 5);
                    assert!(color & 31 <= 4);
                }
            }
            assert_eq!(
                loaded
                    .memory
                    .read_u32_be(PIXELS + y * row_bytes + 8 * depth / 8),
                Some(0xaaaa_aaaa)
            );
        }
    }
}

#[test]
fn file_image_draw_obeys_clip_region_and_fully_clipped_draw_succeeds() {
    let mut loaded = import_file(b"Title", jpeg());
    let row_bytes = destination(&mut loaded, 32);
    let handle = SCRATCH + 0x200;
    let region = handle + 4;
    loaded.memory.add_region(handle, vec![0; 14]);
    loaded.memory.write_u32_be(handle, region).unwrap();
    loaded.memory.write_u16_be(region, 10).unwrap();
    ppc_write_rect(&mut loaded.memory, region + 2, 2, 3, 4, 5).unwrap();
    loaded
        .memory
        .write_u32_be(PORT + PPC_CGRAF_PORT_CLIP_RGN_OFFSET, handle)
        .unwrap();
    draw(&mut loaded);
    for y in 0..8 {
        for x in 0..8 {
            let pixel = loaded
                .memory
                .read_u32_be(PIXELS + y * row_bytes + x * 4)
                .unwrap();
            assert_eq!(
                pixel != 0xaaaa_aaaa,
                (2..4).contains(&y) && (3..5).contains(&x)
            );
        }
    }
    ppc_write_rect(&mut loaded.memory, region + 2, 0, 0, 0, 0).unwrap();
    let before = (0..row_bytes * 8)
        .map(|i| loaded.memory.read_u8(PIXELS + i).unwrap())
        .collect::<Vec<_>>();
    draw(&mut loaded);
    let after = (0..row_bytes * 8)
        .map(|i| loaded.memory.read_u8(PIXELS + i).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(before, after);
}

#[test]
fn png_file_sniffing_preserves_transparency_without_a_filename_extension() {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &[255, 0, 0, 0, 0, 255, 0, 255],
            2,
            1,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    let mut loaded = import_file(b"Alpha", bytes);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.quicktime.graphics_importer_bounds,
        Some((0, 0, 1, 2))
    );
    destination(&mut loaded, 32);
    draw(&mut loaded);
    assert_eq!(loaded.memory.read_u32_be(PIXELS), Some(0xaaaa_aaaa));
    assert_eq!(loaded.memory.read_u32_be(PIXELS + 4), Some(0x0000_ff00));
}

#[test]
fn damaged_and_unknown_file_images_do_not_publish_an_importer() {
    let mut oversized = jpeg();
    let sof = oversized
        .windows(2)
        .position(|bytes| bytes == [0xff, 0xc0])
        .unwrap();
    oversized[sof + 7..sof + 9].copy_from_slice(&8193u16.to_be_bytes());
    for (bytes, error) in [
        (oversized, -8969),
        (vec![b'B', b'M', 0, 0, 0, 0, 0, 8, 0, 8], -8961),
        (vec![0xff, 0xd8, 0xff, 0xe0, 0, 16, 0], -8969),
        (b"not an image".to_vec(), -8961),
    ] {
        let mut loaded = import_file(b"Broken", bytes);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(error));
        assert_eq!(loaded.memory.read_u32_be(IMPORTER_OUT), Some(0));
        assert!(!loaded.quicktime.graphics_importer_open);
        assert!(loaded.quicktime.graphics_importer_image.is_none());
    }
}

#[test]
fn file_image_draw_maps_natural_coordinates_into_shifted_pixmap_bounds() {
    for (left, top, xs, ys) in [(3, 2, 0..5, 0..6), (-3, -2, 3..8, 2..8)] {
        let mut loaded = import_file(b"Title", jpeg());
        let row_bytes = destination(&mut loaded, 32);
        let handle = SCRATCH + 0x300;
        let pixmap = handle + 4;
        loaded.memory.add_region(handle, vec![0; 54]);
        loaded.memory.write_u32_be(handle, pixmap).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            pixmap,
            PIXELS,
            row_bytes,
            top,
            left,
            top + 8,
            left + 8,
            32,
        )
        .unwrap();
        loaded.memory.write_u32_be(PORT + 2, handle).unwrap();
        loaded.memory.write_u16_be(PORT + 6, 0xc000).unwrap();
        draw(&mut loaded);
        for y in 0..8 {
            for x in 0..8 {
                let pixel = loaded
                    .memory
                    .read_u32_be(PIXELS + y * row_bytes + x * 4)
                    .unwrap();
                assert_eq!(
                    pixel != 0xaaaa_aaaa,
                    xs.contains(&x) && ys.contains(&y),
                    "origin=({left},{top}) pixel=({x},{y})"
                );
            }
        }
    }
}
