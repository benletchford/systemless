use super::*;

fn push_quicktime_atom(bytes: &mut Vec<u8>, atom_type: &[u8; 4], payload: &[u8]) {
    let size = 8usize.checked_add(payload.len()).unwrap();
    bytes.extend_from_slice(&u32::try_from(size).unwrap().to_be_bytes());
    bytes.extend_from_slice(atom_type);
    bytes.extend_from_slice(payload);
}

fn ppc_test_push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn ppc_test_push_u24(bytes: &mut Vec<u8>, value: usize) {
    assert!(value <= 0x00ff_ffff);
    bytes.push(((value >> 16) & 0xff) as u8);
    bytes.push(((value >> 8) & 0xff) as u8);
    bytes.push((value & 0xff) as u8);
}

fn test_cinepak_frame_sample(frame_flags: u8, strip_id: u8, strip_payload: &[u8]) -> Vec<u8> {
    let strip_size = 12usize.checked_add(strip_payload.len()).unwrap();
    let frame_size = 10usize.checked_add(strip_size).unwrap();
    let mut bytes = Vec::new();
    bytes.push(frame_flags);
    ppc_test_push_u24(&mut bytes, frame_size);
    ppc_test_push_u16(&mut bytes, 4);
    ppc_test_push_u16(&mut bytes, 4);
    ppc_test_push_u16(&mut bytes, 1);
    bytes.push(strip_id);
    ppc_test_push_u24(&mut bytes, strip_size);
    ppc_test_push_u16(&mut bytes, 0);
    ppc_test_push_u16(&mut bytes, 0);
    ppc_test_push_u16(&mut bytes, 4);
    ppc_test_push_u16(&mut bytes, 4);
    bytes.extend_from_slice(strip_payload);
    bytes
}

fn test_cinepak_v1_frame_sample() -> Vec<u8> {
    let mut strip_payload = Vec::new();
    strip_payload.push(0x22);
    ppc_test_push_u24(&mut strip_payload, 10);
    strip_payload.extend_from_slice(&[0, 85, 170, 255, 0, 0]);
    strip_payload.push(0x32);
    ppc_test_push_u24(&mut strip_payload, 5);
    strip_payload.push(0);
    test_cinepak_frame_sample(0, 0x10, &strip_payload)
}

fn test_cinepak_v4_frame_sample() -> Vec<u8> {
    let mut strip_payload = Vec::new();
    strip_payload.push(0x24);
    ppc_test_push_u24(&mut strip_payload, 20);
    strip_payload.extend_from_slice(&[
        10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160,
    ]);
    strip_payload.push(0x30);
    ppc_test_push_u24(&mut strip_payload, 12);
    strip_payload.extend_from_slice(&0x8000_0000u32.to_be_bytes());
    strip_payload.extend_from_slice(&[0, 1, 2, 3]);
    test_cinepak_frame_sample(0, 0x10, &strip_payload)
}

fn test_cinepak_flagged_v1_update_sample() -> Vec<u8> {
    let mut strip_payload = Vec::new();
    strip_payload.push(0x23);
    ppc_test_push_u24(&mut strip_payload, 14);
    strip_payload.extend_from_slice(&0x4000_0000u32.to_be_bytes());
    strip_payload.extend_from_slice(&[255, 0, 0, 255, 0, 0]);
    strip_payload.push(0x32);
    ppc_test_push_u24(&mut strip_payload, 5);
    strip_payload.push(1);
    test_cinepak_frame_sample(0, 0x11, &strip_payload)
}

fn test_cinepak_skip_frame_sample() -> Vec<u8> {
    let mut strip_payload = Vec::new();
    strip_payload.push(0x31);
    ppc_test_push_u24(&mut strip_payload, 8);
    strip_payload.extend_from_slice(&0u32.to_be_bytes());
    test_cinepak_frame_sample(0, 0x11, &strip_payload)
}

fn ppc_test_rgb_at(frame: &PpcQuickTimeDecodedVideoFrame, x: usize, y: usize) -> [u8; 3] {
    let offset = y
        .checked_mul(frame.width)
        .and_then(|base| base.checked_add(x))
        .and_then(|pixel| pixel.checked_mul(3))
        .unwrap();
    [
        frame.rgb[offset],
        frame.rgb[offset + 1],
        frame.rgb[offset + 2],
    ]
}

fn test_quicktime_gif_bytes(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    use image::codecs::gif::GifEncoder;
    let mut bytes = Vec::new();
    let mut encoder = GifEncoder::new(&mut bytes);
    encoder
        .encode(rgba, width, height, image::ExtendedColorType::Rgba8)
        .unwrap();
    drop(encoder);
    bytes
}

fn test_quicktime_tga_bytes(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    use image::codecs::tga::TgaEncoder;
    let mut bytes = Vec::new();
    TgaEncoder::new(&mut bytes)
        .encode(rgba, width, height, image::ExtendedColorType::Rgba8)
        .unwrap();
    bytes
}

fn test_quicktime_tga_header_depth(bytes: &[u8]) -> u16 {
    u16::from(bytes[16])
}

/// Build a 16-bit truecolor TGA with the real Deimos Rising header shape:
/// 18-byte header, pixel depth 16, `image_desc` selecting the row order.
/// Pixel values are written in file order (bottom-left first by default).
fn test_quicktime_tga_16_bytes(image_type: u8, image_desc: u8, pixels: &[u16]) -> Vec<u8> {
    let width = 2u16;
    let height = 2u16;
    let mut bytes = vec![0u8; 18];
    bytes[2] = image_type;
    bytes[12..14].copy_from_slice(&width.to_le_bytes());
    bytes[14..16].copy_from_slice(&height.to_le_bytes());
    bytes[16] = 16;
    bytes[17] = image_desc;
    match image_type {
        2 => {
            for pixel in pixels {
                bytes.extend_from_slice(&pixel.to_le_bytes());
            }
        }
        10 => {
            bytes.push(((pixels.len() as u8).saturating_sub(1)) & 0x7f);
            for pixel in pixels {
                bytes.extend_from_slice(&pixel.to_le_bytes());
            }
        }
        _ => unreachable!(),
    }
    bytes
}

fn test_quicktime_tga_16_color(red: u16, green: u16, blue: u16) -> u16 {
    ((red & 0x1f) << 10) | ((green & 0x1f) << 5) | (blue & 0x1f)
}

fn test_quicktime_importer_decode_image_bytes(
    subtype: &[u8; 4],
    data: &[u8],
    expected_status: i16,
) -> Option<PpcQtGraphicsImage> {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GraphicsImportSetDataHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = PPC_DATA_BASE + 0x1800;
    let data_ptr = PPC_DATA_BASE + 0x2000;
    test_quicktime_push_importer_data_handle(&mut loaded, handle, data_ptr, data);
    loaded.quicktime.graphics_importer_open = true;
    loaded.quicktime.graphics_importer_subtype = u32::from_be_bytes(*subtype);
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(expected_status));
    loaded.quicktime.graphics_importer_image
}

fn test_quicktime_push_importer_data_handle(
    loaded: &mut PpcLoadedApp,
    handle: u32,
    data_ptr: u32,
    data: &[u8],
) {
    loaded.memory.add_region(data_ptr, data.to_vec());
    test_handles!(loaded).push(PpcHandleRecord {
        handle,
        ptr: data_ptr,
        size: u32::try_from(data.len()).unwrap(),
        capacity: u32::try_from(data.len()).unwrap(),
    });
}

fn test_quicktime_tkhd_movie(width: u16, height: u16) -> Vec<u8> {
    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    let mut mvhd_payload = vec![0; 100];
    mvhd_payload[12..16].copy_from_slice(&60u32.to_be_bytes());
    mvhd_payload[16..20].copy_from_slice(&120u32.to_be_bytes());
    mvhd_payload[20..24].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    mvhd_payload[24..26].copy_from_slice(&0x0100u16.to_be_bytes());

    let mut tkhd_payload = vec![0; 84];
    tkhd_payload[3] = 0x07;
    tkhd_payload[12..16].copy_from_slice(&1u32.to_be_bytes());
    tkhd_payload[76..80].copy_from_slice(&(u32::from(width) << 16).to_be_bytes());
    tkhd_payload[80..84].copy_from_slice(&(u32::from(height) << 16).to_be_bytes());

    let mut video_mdhd_payload = vec![0; 24];
    video_mdhd_payload[12..16].copy_from_slice(&60u32.to_be_bytes());
    video_mdhd_payload[16..20].copy_from_slice(&120u32.to_be_bytes());

    let mut video_hdlr_payload = vec![0; 24];
    video_hdlr_payload[4..8].copy_from_slice(b"mhlr");
    video_hdlr_payload[8..12].copy_from_slice(b"vide");

    let mut video_stsd_payload = Vec::new();
    push_u32(&mut video_stsd_payload, 0);
    push_u32(&mut video_stsd_payload, 1);
    push_u32(&mut video_stsd_payload, 16);
    video_stsd_payload.extend_from_slice(b"rle ");
    video_stsd_payload.extend_from_slice(&[0; 8]);

    let mut video_stts_payload = Vec::new();
    push_u32(&mut video_stts_payload, 0);
    push_u32(&mut video_stts_payload, 1);
    push_u32(&mut video_stts_payload, 2);
    push_u32(&mut video_stts_payload, 60);

    let mut video_stsc_payload = Vec::new();
    push_u32(&mut video_stsc_payload, 0);
    push_u32(&mut video_stsc_payload, 1);
    push_u32(&mut video_stsc_payload, 1);
    push_u32(&mut video_stsc_payload, 2);
    push_u32(&mut video_stsc_payload, 1);

    let mut video_stsz_payload = Vec::new();
    push_u32(&mut video_stsz_payload, 0);
    push_u32(&mut video_stsz_payload, 4);
    push_u32(&mut video_stsz_payload, 2);

    let mut video_stco_payload = Vec::new();
    push_u32(&mut video_stco_payload, 0);
    push_u32(&mut video_stco_payload, 1);
    push_u32(&mut video_stco_payload, 8);

    let mut video_stbl_payload = Vec::new();
    push_quicktime_atom(&mut video_stbl_payload, b"stsd", &video_stsd_payload);
    push_quicktime_atom(&mut video_stbl_payload, b"stts", &video_stts_payload);
    push_quicktime_atom(&mut video_stbl_payload, b"stsc", &video_stsc_payload);
    push_quicktime_atom(&mut video_stbl_payload, b"stsz", &video_stsz_payload);
    push_quicktime_atom(&mut video_stbl_payload, b"stco", &video_stco_payload);

    let mut video_minf_payload = Vec::new();
    push_quicktime_atom(&mut video_minf_payload, b"stbl", &video_stbl_payload);

    let mut video_mdia_payload = Vec::new();
    push_quicktime_atom(&mut video_mdia_payload, b"mdhd", &video_mdhd_payload);
    push_quicktime_atom(&mut video_mdia_payload, b"hdlr", &video_hdlr_payload);
    push_quicktime_atom(&mut video_mdia_payload, b"minf", &video_minf_payload);

    let mut video_trak_payload = Vec::new();
    push_quicktime_atom(&mut video_trak_payload, b"tkhd", &tkhd_payload);
    push_quicktime_atom(&mut video_trak_payload, b"mdia", &video_mdia_payload);

    let mut audio_mdhd_payload = vec![0; 24];
    audio_mdhd_payload[12..16].copy_from_slice(&22_050u32.to_be_bytes());
    audio_mdhd_payload[16..20].copy_from_slice(&6u32.to_be_bytes());

    let mut audio_hdlr_payload = vec![0; 24];
    audio_hdlr_payload[4..8].copy_from_slice(b"mhlr");
    audio_hdlr_payload[8..12].copy_from_slice(b"soun");

    let mut audio_stsd_payload = Vec::new();
    push_u32(&mut audio_stsd_payload, 0);
    push_u32(&mut audio_stsd_payload, 1);
    push_u32(&mut audio_stsd_payload, 36);
    audio_stsd_payload.extend_from_slice(b"raw ");
    push_u32(&mut audio_stsd_payload, 0);
    push_u16(&mut audio_stsd_payload, 0);
    push_u16(&mut audio_stsd_payload, 1);
    push_u16(&mut audio_stsd_payload, 0);
    push_u16(&mut audio_stsd_payload, 0);
    push_u32(&mut audio_stsd_payload, 0);
    push_u16(&mut audio_stsd_payload, 1);
    push_u16(&mut audio_stsd_payload, 8);
    push_u16(&mut audio_stsd_payload, 0);
    push_u16(&mut audio_stsd_payload, 0);
    push_u32(&mut audio_stsd_payload, 22_050u32 << 16);

    let mut audio_stts_payload = Vec::new();
    push_u32(&mut audio_stts_payload, 0);
    push_u32(&mut audio_stts_payload, 1);
    push_u32(&mut audio_stts_payload, 6);
    push_u32(&mut audio_stts_payload, 1);

    let mut audio_stsc_payload = Vec::new();
    push_u32(&mut audio_stsc_payload, 0);
    push_u32(&mut audio_stsc_payload, 1);
    push_u32(&mut audio_stsc_payload, 1);
    push_u32(&mut audio_stsc_payload, 6);
    push_u32(&mut audio_stsc_payload, 1);

    let mut audio_stsz_payload = Vec::new();
    push_u32(&mut audio_stsz_payload, 0);
    push_u32(&mut audio_stsz_payload, 1);
    push_u32(&mut audio_stsz_payload, 6);

    let mut audio_stco_payload = Vec::new();
    push_u32(&mut audio_stco_payload, 0);
    push_u32(&mut audio_stco_payload, 1);
    push_u32(&mut audio_stco_payload, 16);

    let mut audio_stbl_payload = Vec::new();
    push_quicktime_atom(&mut audio_stbl_payload, b"stsd", &audio_stsd_payload);
    push_quicktime_atom(&mut audio_stbl_payload, b"stts", &audio_stts_payload);
    push_quicktime_atom(&mut audio_stbl_payload, b"stsc", &audio_stsc_payload);
    push_quicktime_atom(&mut audio_stbl_payload, b"stsz", &audio_stsz_payload);
    push_quicktime_atom(&mut audio_stbl_payload, b"stco", &audio_stco_payload);

    let mut audio_minf_payload = Vec::new();
    push_quicktime_atom(&mut audio_minf_payload, b"stbl", &audio_stbl_payload);

    let mut audio_mdia_payload = Vec::new();
    push_quicktime_atom(&mut audio_mdia_payload, b"mdhd", &audio_mdhd_payload);
    push_quicktime_atom(&mut audio_mdia_payload, b"hdlr", &audio_hdlr_payload);
    push_quicktime_atom(&mut audio_mdia_payload, b"minf", &audio_minf_payload);

    let mut audio_trak_payload = Vec::new();
    let mut audio_tkhd_payload = vec![0; 84];
    audio_tkhd_payload[3] = 0x07;
    audio_tkhd_payload[12..16].copy_from_slice(&2u32.to_be_bytes());
    push_quicktime_atom(&mut audio_trak_payload, b"tkhd", &audio_tkhd_payload);
    push_quicktime_atom(&mut audio_trak_payload, b"mdia", &audio_mdia_payload);

    let mut moov_payload = Vec::new();
    push_quicktime_atom(&mut moov_payload, b"mvhd", &mvhd_payload);
    push_quicktime_atom(&mut moov_payload, b"trak", &video_trak_payload);
    push_quicktime_atom(&mut moov_payload, b"trak", &audio_trak_payload);
    let mut bytes = Vec::new();
    push_quicktime_atom(
        &mut bytes,
        b"mdat",
        &[
            0x10, 0x20, 0x30, 0x40, 0x80, 0x90, 0x70, 0x60, 0x80, 0x90, 0x70, 0x60, 0x50, 0x40,
        ],
    );
    push_quicktime_atom(&mut bytes, b"moov", &moov_payload);
    bytes
}

#[test]
fn import_bindings_classify_quicktime_imports() {
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GetGraphicsImporterForFile"),
        PpcImportDispatcherTarget::QtGetGraphicsImporterForFile
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GraphicsImportGetBoundsRect"),
        PpcImportDispatcherTarget::QtGraphicsImportGetBoundsRect
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GraphicsImportSetGWorld"),
        PpcImportDispatcherTarget::QtGraphicsImportSetGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GraphicsImportDraw"),
        PpcImportDispatcherTarget::QtGraphicsImportDraw
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "OpenMovieFile"),
        PpcImportDispatcherTarget::QtOpenMovieFile
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "NewMovieFromFile"),
        PpcImportDispatcherTarget::QtNewMovieFromFile
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GetMovieBox"),
        PpcImportDispatcherTarget::QtGetMovieBox
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "SetMovieBox"),
        PpcImportDispatcherTarget::QtSetMovieBox
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "IsMovieDone"),
        PpcImportDispatcherTarget::QtIsMovieDone
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "SetMovieGWorld"),
        PpcImportDispatcherTarget::QtSetMovieGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "StartMovie"),
        PpcImportDispatcherTarget::QtStartMovie
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "StopMovie"),
        PpcImportDispatcherTarget::QtStopMovie
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "MoviesTask"),
        PpcImportDispatcherTarget::QtMoviesTask
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "DisposeMovie"),
        PpcImportDispatcherTarget::QtDisposeMovie
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GoToBeginningOfMovie"),
        PpcImportDispatcherTarget::QtGoToBeginningOfMovie
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GoToEndOfMovie"),
        PpcImportDispatcherTarget::QtGoToEndOfMovie
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GetMovieDuration"),
        PpcImportDispatcherTarget::QtGetMovieDuration
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "LoadMovieIntoRam"),
        PpcImportDispatcherTarget::QtLoadMovieIntoRam
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "CloseMovieFile"),
        PpcImportDispatcherTarget::QtCloseMovieFile
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "EnterMovies"),
        PpcImportDispatcherTarget::QtEnterMovies
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "ExitMovies"),
        PpcImportDispatcherTarget::QtExitMovies
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GetMoviesError"),
        PpcImportDispatcherTarget::QtGetMoviesError
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "GetMoviesStickyError"),
        PpcImportDispatcherTarget::QtGetMoviesStickyError
    );
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "ClearMoviesStickyError"),
        PpcImportDispatcherTarget::QtClearMoviesStickyError
    );
}

#[test]
fn carbon_multimedia_movie_imports_bind_existing_quicktime_handlers() {
    for (name, target) in [
        ("GetMovieBox", PpcImportDispatcherTarget::QtGetMovieBox),
        ("DisposeMovie", PpcImportDispatcherTarget::QtDisposeMovie),
        ("SetMovieBox", PpcImportDispatcherTarget::QtSetMovieBox),
        ("EnterMovies", PpcImportDispatcherTarget::QtEnterMovies),
        ("SetMovieGWorld", PpcImportDispatcherTarget::QtSetMovieGWorld),
        ("StopMovie", PpcImportDispatcherTarget::QtStopMovie),
        ("GetMoviesError", PpcImportDispatcherTarget::QtGetMoviesError),
        ("MoviesTask", PpcImportDispatcherTarget::QtMoviesTask),
        ("StartMovie", PpcImportDispatcherTarget::QtStartMovie),
        ("IsMovieDone", PpcImportDispatcherTarget::QtIsMovieDone),
    ] {
        assert_eq!(
            dispatcher_target_for_import("Apple;Carbon;Multimedia", name),
            target,
            "{name}"
        );
    }

    let pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"Apple;Carbon;Multimedia",
        b"MoviesTask",
        0x82,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut weak_loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        weak_loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::QtMoviesTask
    );
    assert_ne!(weak_loaded.imports[0].address, 0);
    assert_eq!(
        weak_loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(weak_loaded.imports[0].address)
    );

    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"Apple;Carbon;Multimedia",
        b"MoviesTask",
    ))
    .unwrap();
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    assert_eq!(
        dispatcher_target_for_import("Apple;Carbon;Multimedia", "MovieImportFile"),
        PpcImportDispatcherTarget::Unsupported
    );
}

#[test]
fn hle_import_runner_tracks_quicktime_init_and_error_state() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"EnterMovies");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.movie_toolbox_enter_count, 1);
    assert_eq!(loaded.quicktime.movie_toolbox_exit_count, 0);
    assert_eq!(loaded.quicktime.movie_toolbox_init_depth, 1);
    assert_eq!(loaded.quicktime.movie_error, PPC_NO_ERR);
    assert_eq!(loaded.quicktime.movie_sticky_error, PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGraphicsImportSetGWorld;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0x0600_3000;
    loaded.cpu.gpr[5] = 0x0600_4000;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.quicktime.movie_error, PPC_PARAM_ERR);
    assert_eq!(loaded.quicktime.movie_sticky_error, PPC_PARAM_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGetMoviesError;
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.quicktime.movie_error, PPC_NO_ERR);
    assert_eq!(loaded.quicktime.movie_sticky_error, PPC_PARAM_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGetMoviesStickyError;
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.quicktime.movie_error, PPC_NO_ERR);
    assert_eq!(loaded.quicktime.movie_sticky_error, PPC_PARAM_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtClearMoviesStickyError;
    loaded.cpu.gpr[3] = 0xfeed_face;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(loaded.quicktime.movie_error, PPC_NO_ERR);
    assert_eq!(loaded.quicktime.movie_sticky_error, PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtExitMovies;
    loaded.cpu.gpr[3] = 0xcafe_babe;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xcafe_babe);
    assert_eq!(loaded.quicktime.movie_toolbox_enter_count, 1);
    assert_eq!(loaded.quicktime.movie_toolbox_exit_count, 1);
    assert_eq!(loaded.quicktime.movie_toolbox_init_depth, 0);
}

#[test]
fn hle_import_runner_tracks_quicktime_movie_file_box_and_beginning_state() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"OpenMovieFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let ref_num_out_ptr = scratch;
    let rect_ptr = scratch + 16;
    let box_out_ptr = scratch + 32;
    let movie_out_ptr = scratch + 48;
    let movie_spec_ptr = scratch + 64;
    let movie_data = test_quicktime_tkhd_movie(320, 240);
    let expected_audio_samples = vec![0x80, 0x90, 0x70, 0x60, 0x50, 0x40];
    let expected_video_track = PpcQuickTimeVideoTrackRecord {
        media_time_scale: 60,
        media_duration: 120,
        sample_count: 2,
        first_sample_duration: 60,
        first_sample_size: 4,
        first_chunk_offset: 8,
        first_sample_data_len: 4,
        first_sample_checksum: 0xa0,
        first_sample_preview_len: 4,
        first_sample_preview: [0x10, 0x20, 0x30, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        first_samples_per_chunk: 2,
        sample_description_id: 1,
        codec: u32::from_be_bytes(*b"rle "),
    };
    let expected_video_samples = PpcQuickTimeVideoSampleTableRecord {
        media_time_scale: 60,
        media_duration: 120,
        sample_count: 2,
        codec: u32::from_be_bytes(*b"rle "),
        samples: vec![
            PpcQuickTimeVideoSampleRecord {
                offset: 8,
                size: 4,
                media_start_time: 0,
                duration: 60,
                data_len: 4,
                checksum: 0xa0,
                preview_len: 4,
                preview: [0x10, 0x20, 0x30, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            },
            PpcQuickTimeVideoSampleRecord {
                offset: 12,
                size: 4,
                media_start_time: 60,
                duration: 60,
                data_len: 4,
                checksum: 0x1e0,
                preview_len: 4,
                preview: [0x80, 0x90, 0x70, 0x60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            },
        ],
    };
    let expected_audio_track = PpcQuickTimeAudioTrackRecord {
        media_time_scale: 22_050,
        media_duration: 6,
        sample_count: 6,
        first_sample_duration: 1,
        first_sample_size: 1,
        first_chunk_offset: 16,
        first_sample_data_len: 1,
        first_sample_checksum: 0x80,
        first_sample_preview_len: 1,
        first_sample_preview: [0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        first_samples_per_chunk: 6,
        sample_description_id: 1,
        codec: u32::from_be_bytes(*b"raw "),
        channel_count: 1,
        sample_size_bits: 8,
        sample_rate_fixed: 22_050u32 << 16,
    };
    loaded.memory.add_region(scratch, vec![0xaa; 80]);
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Intro.mov".to_string(),
        data: (movie_data.clone()).into(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"MooV"),
        finder_flags: 0,
        dirty: false,
    });
    write_ppc_fsspec(
        &mut loaded.memory,
        movie_spec_ptr,
        PPC_BOOT_VOLUME_REF_NUM,
        PPC_ROOT_DIR_ID,
        b"Intro.mov",
    );
    loaded.cpu.gpr[3] = movie_spec_ptr;
    loaded.cpu.gpr[4] = ref_num_out_ptr;
    loaded.cpu.gpr[5] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u16_be(ref_num_out_ptr),
        Some(PPC_FIRST_FILE_REF_NUM as u16)
    );
    assert_eq!(loaded.quicktime.movie_file_open_count, 1);
    assert_eq!(loaded.quicktime.movie_file_ref_num, PPC_FIRST_FILE_REF_NUM);
    assert_eq!(loaded.quicktime.movie_file_path, "Intro.mov");
    assert_eq!(loaded.quicktime.movie_file_data, movie_data);
    assert_eq!(loaded.quicktime.movie_file_bounds, Some((0, 0, 240, 320)));
    assert_eq!(loaded.quicktime.movie_file_time_scale, 60);
    assert_eq!(loaded.quicktime.movie_file_preferred_rate, 0x0001_0000);
    assert_eq!(loaded.quicktime.movie_file_preferred_volume, 0x0100);
    assert_eq!(loaded.quicktime.movie_file_tracks.len(), 2);
    assert_eq!(loaded.quicktime.movie_file_duration, 120);
    assert_eq!(loaded.quicktime.movie_file_tasks_until_done, 120);
    assert_eq!(
        loaded.quicktime.movie_file_video_track,
        Some(expected_video_track)
    );
    assert_eq!(
        loaded.quicktime.movie_file_video_samples,
        Some(expected_video_samples.clone())
    );
    assert_eq!(
        loaded.quicktime.movie_file_audio_track,
        Some(expected_audio_track)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtNewMovieFromFile;
    loaded.cpu.gpr[3] = movie_out_ptr;
    loaded.cpu.gpr[4] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[7] = 1; // newMovieActive
    loaded.cpu.gpr[8] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(movie_out_ptr), Some(PPC_QT_MOVIE));
    assert_eq!(loaded.quicktime.movie_time_scale, 60);
    assert_eq!(loaded.quicktime.movie_preferred_rate, 0x0001_0000);
    assert_eq!(loaded.quicktime.movie_preferred_volume, 0x0100);
    assert_eq!(loaded.quicktime.movie_volume, 0x0100);
    assert_eq!(loaded.quicktime.movie_tracks.len(), 2);
    assert!(loaded.quicktime.movie_at_beginning);
    assert_eq!(loaded.quicktime.movie_box, (0, 0, 240, 320));
    assert_eq!(loaded.quicktime.movie_tasks_until_done, 120);
    assert_eq!(
        loaded.quicktime.movie_video_track,
        Some(expected_video_track)
    );
    assert_eq!(
        loaded.quicktime.movie_video_samples,
        Some(expected_video_samples.clone())
    );
    assert_eq!(
        loaded.quicktime.movie_audio_track,
        Some(expected_audio_track)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtStartMovie;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.quicktime.movie_started);
    assert_eq!(loaded.sound.file_playbacks.len(), 1);
    let movie_audio_playback = loaded.sound.file_playbacks.last().copied().unwrap();
    assert_eq!(movie_audio_playback.channel, PPC_QT_MOVIE);
    assert_eq!(movie_audio_playback.ref_num, PPC_FIRST_FILE_REF_NUM);
    assert_eq!(
        loaded.sound.manager.file_playback_paused(PPC_QT_MOVIE),
        Some(false)
    );
    assert_eq!(
        movie_audio_playback.decoded_aiff,
        Some(PpcDecodedAiffSamples {
            sample_rate_fixed: 22_050u32 << 16,
            sample_count: 6,
            preview_len: 6,
            preview: [0x80, 0x90, 0x70, 0x60, 0x50, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        })
    );
    assert_eq!(
        loaded.sound.decoded_file_playbacks,
        vec![PpcDecodedAiffPlaybackRecord {
            file_playback_index: 0,
            channel: PPC_QT_MOVIE,
            sample_rate_fixed: 22_050u32 << 16,
            samples: expected_audio_samples.clone(),
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtStopMovie;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(!loaded.quicktime.movie_started);
    assert_eq!(
        loaded.sound.manager.file_playback_paused(PPC_QT_MOVIE),
        None
    );

    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_task_count = 119;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtIsMovieDone;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.quicktime.movie_task_count = 120;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    ppc_write_rect(&mut loaded.memory, rect_ptr, 10, 20, 80, 120).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtSetMovieBox;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_QT_MOVIE);
    assert_eq!(loaded.quicktime.movie_box, (10, 20, 80, 120));
    assert_eq!(loaded.quicktime.movie_set_box_count, 1);
    assert_eq!(loaded.quicktime.movie_error, PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGetMovieBox;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = box_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_QT_MOVIE);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, box_out_ptr),
        Some((10, 20, 80, 120))
    );

    loaded.sound.manager.play_file_buffer(
        PPC_QT_MOVIE,
        vec![0x80],
        crate::sound::OUTPUT_RATE << 16,
        None,
    );
    assert_eq!(
        loaded.sound.manager.toggle_file_paused(PPC_QT_MOVIE),
        Some(true)
    );
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_task_count = 3;
    loaded.quicktime.movie_at_beginning = false;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGoToBeginningOfMovie;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_QT_MOVIE);
    assert_eq!(loaded.quicktime.movie_beginning_count, 1);
    assert!(loaded.quicktime.movie_at_beginning);
    assert!(!loaded.quicktime.movie_started);
    assert_eq!(loaded.quicktime.movie_task_count, 0);
    assert_eq!(
        loaded.sound.manager.file_playback_paused(PPC_QT_MOVIE),
        None
    );

    loaded.quicktime.movie_file_duration = 240;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGetMovieDuration;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 240);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtLoadMovieIntoRam;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 240;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_at_beginning = true;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGoToEndOfMovie;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(!loaded.quicktime.movie_at_beginning);
    assert!(!loaded.quicktime.movie_started);
    assert!(loaded.quicktime.movie_task_count >= 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtCloseMovieFile;
    loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.movie_file_close_count, 1);
    assert_eq!(
        loaded.quicktime.movie_file_last_closed_ref_num,
        PPC_FIRST_FILE_REF_NUM
    );
    assert_eq!(loaded.quicktime.movie_file_ref_num, 0);
}

#[test]
fn quicktime_open_movie_file_combines_data_and_movie_resource_forks() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"OpenMovieFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let ref_num_out_ptr = PPC_DATA_BASE + 0x1000;
    let movie_spec_ptr = PPC_DATA_BASE + 0x1020;
    let movie_out_ptr = PPC_DATA_BASE + 0x1080;
    let res_id_ptr = PPC_DATA_BASE + 0x1090;
    let changed_ptr = PPC_DATA_BASE + 0x1092;
    loaded.memory.add_region(ref_num_out_ptr, vec![0; 2]);
    loaded.memory.add_region(movie_spec_ptr, vec![0; 80]);
    loaded.memory.add_region(movie_out_ptr, vec![0; 4]);
    loaded.memory.add_region(res_id_ptr, vec![0; 2]);
    loaded.memory.add_region(changed_ptr, vec![0; 1]);

    let complete_movie = test_quicktime_tkhd_movie(320, 240);
    let movie_resource = ppc_qt_top_level_atom(&complete_movie, b"moov")
        .expect("synthetic movie has a movie atom")
        .to_vec();
    let data_fork_len = complete_movie.len() - movie_resource.len();
    let data_fork = complete_movie[..data_fork_len].to_vec();
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Music/Theme".to_string(),
        data: (data_fork).into(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"MooV"),
        finder_flags: 0,
        dirty: false,
    });
    let resource_fork = serialize_resource_fork(&[ResourceForkEntry {
        res_type: *b"moov",
        id: 128,
        name: Vec::new(),
        data: movie_resource,
        attrs: 0,
    }])
    .unwrap();
    loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
        path: "music/theme".to_string(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"MooV"),
        finder_flags: 0,
        resource_len: u32::try_from(resource_fork.len()).unwrap(),
        raw_data: Some(resource_fork.into()),
        map_attrs: 0,
        dirty: false,
    });
    write_ppc_fsspec(
        &mut loaded.memory,
        movie_spec_ptr,
        PPC_BOOT_VOLUME_REF_NUM,
        PPC_ROOT_DIR_ID,
        b"Music:Theme",
    );
    loaded.cpu.gpr[3] = movie_spec_ptr;
    loaded.cpu.gpr[4] = ref_num_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.quicktime.movie_file_data,
        complete_movie[..data_fork_len]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtNewMovieFromFile;
    loaded.cpu.gpr[3] = movie_out_ptr;
    loaded.cpu.gpr[4] = PPC_FIRST_FILE_REF_NUM as u32;
    loaded.cpu.gpr[5] = res_id_ptr;
    loaded.cpu.gpr[8] = changed_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u16_be(res_id_ptr), Some(128));
    assert_eq!(loaded.quicktime.movie_file_data, complete_movie);
    assert_eq!(loaded.quicktime.movie_file_bounds, Some((0, 0, 240, 320)));
    assert_eq!(loaded.quicktime.movie_file_time_scale, 60);
    assert_eq!(loaded.quicktime.movie_file_duration, 120);
    assert!(loaded.quicktime.movie_file_video_track.is_some());
    assert!(loaded.quicktime.movie_file_audio_track.is_some());

    loaded
        .memory
        .write_u32_be(movie_out_ptr, 0xdead_beef)
        .unwrap();
    loaded.memory.write_u16_be(res_id_ptr, 777).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = movie_out_ptr;
    loaded.cpu.gpr[4] = PPC_FIRST_FILE_REF_NUM as u32;
    loaded.cpu.gpr[5] = res_id_ptr;
    loaded.cpu.gpr[8] = changed_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_RES_NOT_FOUND_ERR));
    assert_eq!(loaded.memory.read_u32_be(movie_out_ptr), Some(0));
}

#[test]
fn quicktime_movie_resource_selection_honors_requested_id() {
    let first_movie = test_quicktime_tkhd_movie(320, 240);
    let second_movie = test_quicktime_tkhd_movie(640, 480);
    let first_moov = ppc_qt_top_level_atom(&first_movie, b"moov").unwrap();
    let second_moov = ppc_qt_top_level_atom(&second_movie, b"moov").unwrap();
    let data_fork = first_movie[..first_movie.len() - first_moov.len()].to_vec();
    let resource_fork = serialize_resource_fork(&[
        ResourceForkEntry {
            res_type: *b"moov",
            id: 128,
            name: Vec::new(),
            data: first_moov.to_vec(),
            attrs: 0,
        },
        ResourceForkEntry {
            res_type: *b"moov",
            id: 129,
            name: Vec::new(),
            data: second_moov.to_vec(),
            attrs: 0,
        },
    ])
    .unwrap();
    let resource_files = [PpcVfsResourceFileRecord {
        path: "Music/Theme".to_string(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"MooV"),
        finder_flags: 0,
        resource_len: u32::try_from(resource_fork.len()).unwrap(),
        raw_data: Some(resource_fork.into()),
        map_attrs: 0,
        dirty: false,
    }];

    let mut selected = data_fork.clone();
    assert_eq!(
        ppc_qt_append_movie_resource(
            &mut selected,
            &resource_files,
            &[],
            "music/theme",
            Some(129),
        ),
        Some(129)
    );
    assert_eq!(ppc_qt_movie_bounds(&selected), Some((0, 0, 480, 640)));
    selected = data_fork.clone();
    assert_eq!(
        ppc_qt_append_movie_resource(
            &mut selected,
            &resource_files,
            &[],
            "Music/Theme",
            Some(128),
        ),
        Some(128)
    );
    assert_eq!(ppc_qt_movie_bounds(&selected), Some((0, 0, 240, 320)));
    selected = first_movie.clone();
    assert_eq!(
        ppc_qt_append_movie_resource(
            &mut selected,
            &resource_files,
            &[],
            "Music/Theme",
            Some(129),
        ),
        Some(129)
    );
    assert_eq!(ppc_qt_movie_bounds(&selected), Some((0, 0, 480, 640)));
    selected = data_fork;
    assert_eq!(
        ppc_qt_append_movie_resource(
            &mut selected,
            &resource_files,
            &[],
            "Music/Theme",
            Some(-1),
        ),
        None
    );
}

#[test]
fn quicktime_movie_audio_decoder_downmixes_twos_16_bit_first_chunk() {
    let mut movie_file_data = vec![0; 12];
    movie_file_data.extend_from_slice(&[0x00, 0x00, 0x40, 0x00, 0xc0, 0x00]);
    let decoded = ppc_qt_decode_movie_audio_samples(&PpcQuickTimeState {
        movie_file_data,
        movie_audio_track: Some(PpcQuickTimeAudioTrackRecord {
            media_time_scale: 44_100,
            media_duration: 3,
            sample_count: 3,
            first_sample_duration: 1,
            first_sample_size: 1,
            first_chunk_offset: 12,
            first_sample_data_len: 1,
            first_sample_checksum: 0,
            first_sample_preview_len: 0,
            first_sample_preview: [0; 16],
            first_samples_per_chunk: 3,
            sample_description_id: 1,
            codec: u32::from_be_bytes(*b"twos"),
            channel_count: 1,
            sample_size_bits: 16,
            sample_rate_fixed: 44_100u32 << 16,
        }),
        ..PpcQuickTimeState::default()
    })
    .expect("16-bit twos movie chunk decodes");

    assert_eq!(decoded.samples, vec![0x80, 0xc0, 0x40]);
    assert_eq!(
        decoded.summary,
        PpcDecodedAiffSamples {
            sample_rate_fixed: 44_100u32 << 16,
            sample_count: 3,
            preview_len: 3,
            preview: [0x80, 0xc0, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        }
    );
}

#[test]
fn quicktime_music_track_synthesizes_note_events() {
    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    let note = (1u32 << 29) | (28 << 18) | (127 << 11) | 300;
    let mut movie_file_data = Vec::new();
    push_quicktime_atom(&mut movie_file_data, b"mdat", &note.to_be_bytes());

    let mut mdhd_payload = vec![0; 24];
    mdhd_payload[12..16].copy_from_slice(&600u32.to_be_bytes());
    mdhd_payload[16..20].copy_from_slice(&300u32.to_be_bytes());
    let mut hdlr_payload = vec![0; 24];
    hdlr_payload[4..8].copy_from_slice(b"mhlr");
    hdlr_payload[8..12].copy_from_slice(b"musi");
    let mut stsc_payload = Vec::new();
    push_u32(&mut stsc_payload, 0);
    push_u32(&mut stsc_payload, 1);
    push_u32(&mut stsc_payload, 1);
    push_u32(&mut stsc_payload, 1);
    let mut stts_payload = Vec::new();
    push_u32(&mut stts_payload, 0);
    push_u32(&mut stts_payload, 1);
    push_u32(&mut stts_payload, 1);
    push_u32(&mut stts_payload, 300);
    push_u32(&mut stsc_payload, 1);
    let mut stsd_payload = Vec::new();
    push_u32(&mut stsd_payload, 0);
    push_u32(&mut stsd_payload, 1);
    push_u32(&mut stsd_payload, 20);
    stsd_payload.extend_from_slice(b"musi");
    stsd_payload.extend_from_slice(&[0; 12]);
    let mut stsz_payload = Vec::new();
    push_u32(&mut stsz_payload, 0);
    push_u32(&mut stsz_payload, 4);
    push_u32(&mut stsz_payload, 1);
    let mut stco_payload = Vec::new();
    push_u32(&mut stco_payload, 0);
    push_u32(&mut stco_payload, 1);
    push_u32(&mut stco_payload, 8);
    let mut stbl_payload = Vec::new();
    push_quicktime_atom(&mut stbl_payload, b"stsd", &stsd_payload);
    push_quicktime_atom(&mut stbl_payload, b"stts", &stts_payload);
    push_quicktime_atom(&mut stbl_payload, b"stsc", &stsc_payload);
    push_quicktime_atom(&mut stbl_payload, b"stsz", &stsz_payload);
    push_quicktime_atom(&mut stbl_payload, b"stco", &stco_payload);
    let mut minf_payload = Vec::new();
    push_quicktime_atom(&mut minf_payload, b"stbl", &stbl_payload);
    let mut mdia_payload = Vec::new();
    push_quicktime_atom(&mut mdia_payload, b"mdhd", &mdhd_payload);
    push_quicktime_atom(&mut mdia_payload, b"hdlr", &hdlr_payload);
    push_quicktime_atom(&mut mdia_payload, b"minf", &minf_payload);
    let mut trak_payload = Vec::new();
    push_quicktime_atom(&mut trak_payload, b"mdia", &mdia_payload);
    let mut moov_payload = Vec::new();
    push_quicktime_atom(&mut moov_payload, b"trak", &trak_payload);
    push_quicktime_atom(&mut movie_file_data, b"moov", &moov_payload);

    let decoded = ppc_qt_movie_audio_samples(&movie_file_data).expect("music track decodes");

    assert_eq!(decoded.summary.sample_rate_fixed, 22_050u32 << 16);
    assert_eq!(decoded.samples.len(), 11_025);
    assert!(decoded.samples.iter().any(|sample| *sample != 0x80));
}

#[test]
fn quicktime_music_sample_description_maps_parts_to_gm_instruments() {
    let mut note_request = vec![0; 84];
    note_request[76..80].copy_from_slice(&40u32.to_be_bytes());
    note_request[80..84].copy_from_slice(&82u32.to_be_bytes());
    let mut entry = Vec::new();
    entry.extend_from_slice(&112u32.to_be_bytes());
    entry.extend_from_slice(b"musi");
    entry.extend_from_slice(&[0; 6]);
    entry.extend_from_slice(&1u16.to_be_bytes());
    entry.extend_from_slice(&0u32.to_be_bytes());
    entry.extend_from_slice(&0xf005_0017u32.to_be_bytes());
    entry.extend_from_slice(&note_request);
    entry.extend_from_slice(&0xc001_0017u32.to_be_bytes());
    let mut stsd = vec![0; 4];
    stsd.extend_from_slice(&1u32.to_be_bytes());
    stsd.extend_from_slice(&entry);

    let descriptions = ppc_qt_music_sample_descriptions(&stsd, 0, stsd.len()).unwrap();

    assert_eq!(
        descriptions,
        vec![vec![PpcQuickTimeMusicPart {
            part: 5,
            instrument_number: 40,
            gm_number: 82,
        }]]
    );
    let last = stsd.len() - 4;
    stsd[last..].copy_from_slice(&0xc001_0016u32.to_be_bytes());
    assert!(ppc_qt_music_sample_descriptions(&stsd, 0, stsd.len()).is_none());
    stsd[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(ppc_qt_music_sample_descriptions(&stsd, 0, stsd.len()).is_none());
    stsd[4..8].copy_from_slice(&1u32.to_be_bytes());
    stsd[12..16].copy_from_slice(b"soun");
    assert!(ppc_qt_music_sample_descriptions(&stsd, 0, stsd.len()).is_none());
}

#[test]
fn quicktime_music_uses_stts_sample_starts_and_controller_volume() {
    let note = (1u32 << 29) | (28 << 18) | (127 << 11) | 10;
    let data = [note.to_be_bytes(), note.to_be_bytes()].concat();
    let mut info = PpcQuickTimeMusicDecodeInfo {
        handler_subtype: u32::from_be_bytes(*b"musi"),
        media_time_scale: 100,
        media_duration: 200,
        sample_durations: vec![100, 100],
        sample_sizes: vec![4, 4],
        stsc_entries: vec![PpcQuickTimeStscEntry {
            first_chunk: 1,
            samples_per_chunk: 2,
            sample_description_id: 1,
        }],
        chunk_offsets: vec![0],
        sample_descriptions: vec![vec![PpcQuickTimeMusicPart {
            part: 0,
            instrument_number: 40,
            gm_number: 40,
        }]],
    };

    let decoded = ppc_qt_synthesize_music_track(&data, &info).unwrap();

    assert!(decoded.samples[..2_205]
        .iter()
        .any(|sample| *sample != 0x80));
    assert!(decoded.samples[2_205..22_050]
        .iter()
        .all(|sample| *sample == 0x80));
    assert!(decoded.samples[22_050..24_255]
        .iter()
        .any(|sample| *sample != 0x80));

    let mute = (2u32 << 29) | (7 << 16);
    let mut mixed = vec![0; 4_000];
    let mut parts = vec![PpcQuickTimeMusicPartState {
        part: 0,
        instrument: 40,
        volume: i16::MAX,
        pitch_bend: 0,
        sustain: false,
    }];
    let events = [mute.to_be_bytes(), note.to_be_bytes()].concat();
    assert!(
        ppc_qt_synthesize_music_events(&events, 600, 0, &mut mixed, 22_050, &mut parts,)
            .is_some()
    );
    assert!(mixed.iter().all(|sample| *sample == 0));

    let malformed_extended_note = [0x9000_003cu32.to_be_bytes(), 0u32.to_be_bytes()].concat();
    assert!(ppc_qt_synthesize_music_events(
        &malformed_extended_note,
        600,
        0,
        &mut mixed,
        22_050,
        &mut parts,
    )
    .is_none());

    let invalid_end_value = 0x6000_0001u32.to_be_bytes();
    assert!(ppc_qt_synthesize_music_events(
        &invalid_end_value,
        600,
        0,
        &mut mixed,
        22_050,
        &mut parts,
    )
    .is_none());
    let end_with_trailing_data = [0x6000_0000u32.to_be_bytes(), note.to_be_bytes()].concat();
    assert!(ppc_qt_synthesize_music_events(
        &end_with_trailing_data,
        600,
        0,
        &mut mixed,
        22_050,
        &mut parts,
    )
    .is_none());

    info.stsc_entries[0].sample_description_id = 2;
    assert!(ppc_qt_synthesize_music_track(&data, &info).is_none());

    let invalid_stsc = [
        0u32, 2, // version/flags, entry count
        1, 1, 1, // first entry
        1, 1, 1, // non-increasing second entry
    ]
    .into_iter()
    .flat_map(u32::to_be_bytes)
    .collect::<Vec<_>>();
    assert!(ppc_qt_stsc_entries(&invalid_stsc, 0, invalid_stsc.len()).is_none());
}

#[test]
fn quicktime_music_timbre_follows_instrument_not_part_number() {
    let mut first = vec![0; 2_000];
    let mut same = vec![0; 2_000];
    let mut different = vec![0; 2_000];
    let state = PpcQuickTimeMusicPartState {
        part: 0,
        instrument: 40,
        volume: i16::MAX,
        pitch_bend: 0,
        sustain: false,
    };
    ppc_qt_mix_music_note(&mut first, 0, 30, 60.0, 100, state, 600, 22_050).unwrap();
    ppc_qt_mix_music_note(
        &mut same,
        0,
        30,
        60.0,
        100,
        PpcQuickTimeMusicPartState { part: 5, ..state },
        600,
        22_050,
    )
    .unwrap();
    ppc_qt_mix_music_note(
        &mut different,
        0,
        30,
        60.0,
        100,
        PpcQuickTimeMusicPartState {
            instrument: 82,
            ..state
        },
        600,
        22_050,
    )
    .unwrap();

    assert_eq!(first, same);
    assert_ne!(first, different);
}

#[test]
fn quicktime_movie_frame_salt_uses_current_video_sample_payload() {
    let mut quicktime = PpcQuickTimeState {
        movie_tasks_until_done: 2,
        movie_video_samples: Some(PpcQuickTimeVideoSampleTableRecord {
            media_time_scale: 60,
            media_duration: 2,
            sample_count: 2,
            codec: u32::from_be_bytes(*b"rle "),
            samples: vec![
                PpcQuickTimeVideoSampleRecord {
                    offset: 8,
                    size: 4,
                    media_start_time: 0,
                    duration: 1,
                    data_len: 4,
                    checksum: 0x10,
                    preview_len: 1,
                    preview: [0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                },
                PpcQuickTimeVideoSampleRecord {
                    offset: 12,
                    size: 5,
                    media_start_time: 1,
                    duration: 1,
                    data_len: 5,
                    checksum: 0x2f,
                    preview_len: 1,
                    preview: [0x2f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                },
            ],
        }),
        ..PpcQuickTimeState::default()
    };

    let first_salt = ppc_qt_movie_frame_salt(&quicktime, 0x21);
    quicktime.movie_task_count = 1;
    let second_salt = ppc_qt_movie_frame_salt(&quicktime, 0x21);

    assert_ne!(first_salt, second_salt);
}

#[test]
fn quicktime_stts_expands_sample_durations() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload.extend_from_slice(&2u32.to_be_bytes());
    payload.extend_from_slice(&1u32.to_be_bytes());
    payload.extend_from_slice(&10u32.to_be_bytes());
    payload.extend_from_slice(&2u32.to_be_bytes());
    payload.extend_from_slice(&20u32.to_be_bytes());

    assert_eq!(
        ppc_qt_stts_sample_durations(&payload, 0, payload.len()),
        Some(vec![10, 20, 20])
    );
}

#[test]
fn quicktime_movie_timed_sample_index_uses_media_time() {
    let mut quicktime = PpcQuickTimeState {
        movie_tasks_until_done: 4,
        ..PpcQuickTimeState::default()
    };
    let samples = PpcQuickTimeVideoSampleTableRecord {
        media_time_scale: 4,
        media_duration: 4,
        sample_count: 2,
        codec: u32::from_be_bytes(*b"cvid"),
        samples: vec![
            PpcQuickTimeVideoSampleRecord {
                offset: 0,
                size: 1,
                media_start_time: 0,
                duration: 1,
                data_len: 1,
                checksum: 0,
                preview_len: 0,
                preview: [0; 16],
            },
            PpcQuickTimeVideoSampleRecord {
                offset: 1,
                size: 1,
                media_start_time: 1,
                duration: 3,
                data_len: 1,
                checksum: 0,
                preview_len: 0,
                preview: [0; 16],
            },
        ],
    };

    assert_eq!(
        ppc_qt_movie_timed_sample_index(&quicktime, &samples),
        Some(0)
    );
    quicktime.movie_task_count = 1;
    assert_eq!(
        ppc_qt_movie_timed_sample_index(&quicktime, &samples),
        Some(1)
    );
    quicktime.movie_task_count = 3;
    assert_eq!(
        ppc_qt_movie_timed_sample_index(&quicktime, &samples),
        Some(1)
    );
}

#[test]
fn quicktime_cinepak_decoder_expands_v1_blocks() {
    let sample = test_cinepak_v1_frame_sample();
    let mut decoder = PpcQuickTimeCinepakDecoder::new(4, 4).unwrap();

    decoder.decode_sample(&sample).unwrap();
    let frame = decoder.decoded_frame();

    assert_eq!(ppc_test_rgb_at(&frame, 0, 0), [0, 0, 0]);
    assert_eq!(ppc_test_rgb_at(&frame, 2, 0), [85, 85, 85]);
    assert_eq!(ppc_test_rgb_at(&frame, 0, 2), [170, 170, 170]);
    assert_eq!(ppc_test_rgb_at(&frame, 3, 3), [255, 255, 255]);
}

#[test]
fn quicktime_cinepak_decoder_expands_v4_blocks() {
    let sample = test_cinepak_v4_frame_sample();
    let mut decoder = PpcQuickTimeCinepakDecoder::new(4, 4).unwrap();

    decoder.decode_sample(&sample).unwrap();
    let frame = decoder.decoded_frame();

    assert_eq!(ppc_test_rgb_at(&frame, 0, 0), [10, 10, 10]);
    assert_eq!(ppc_test_rgb_at(&frame, 1, 0), [20, 20, 20]);
    assert_eq!(ppc_test_rgb_at(&frame, 2, 0), [50, 50, 50]);
    assert_eq!(ppc_test_rgb_at(&frame, 3, 0), [60, 60, 60]);
    assert_eq!(ppc_test_rgb_at(&frame, 0, 2), [90, 90, 90]);
    assert_eq!(ppc_test_rgb_at(&frame, 3, 3), [160, 160, 160]);
}

#[test]
fn quicktime_cinepak_decoder_applies_flagged_codebook_updates() {
    let mut decoder = PpcQuickTimeCinepakDecoder::new(4, 4).unwrap();
    decoder
        .decode_sample(&test_cinepak_v1_frame_sample())
        .unwrap();

    decoder
        .decode_sample(&test_cinepak_flagged_v1_update_sample())
        .unwrap();
    let frame = decoder.decoded_frame();

    assert_eq!(ppc_test_rgb_at(&frame, 0, 0), [255, 255, 255]);
    assert_eq!(ppc_test_rgb_at(&frame, 2, 0), [0, 0, 0]);
    assert_eq!(ppc_test_rgb_at(&frame, 0, 2), [0, 0, 0]);
    assert_eq!(ppc_test_rgb_at(&frame, 3, 3), [255, 255, 255]);
}

#[test]
fn quicktime_cinepak_decoder_preserves_skipped_interframe_blocks() {
    let mut decoder = PpcQuickTimeCinepakDecoder::new(4, 4).unwrap();
    decoder
        .decode_sample(&test_cinepak_v1_frame_sample())
        .unwrap();
    let before = decoder.decoded_frame().rgb;

    decoder
        .decode_sample(&test_cinepak_skip_frame_sample())
        .unwrap();
    let after = decoder.decoded_frame().rgb;

    assert_eq!(before, after);
}

#[test]
fn quicktime_movie_video_decode_cache_advances_without_redecoding_prior_samples() {
    let first_sample = test_cinepak_v1_frame_sample();
    let second_sample = test_cinepak_skip_frame_sample();
    let second_offset = u64::try_from(first_sample.len()).unwrap();
    let mut movie_file_data = first_sample.clone();
    movie_file_data.extend_from_slice(&second_sample);
    let mut quicktime = PpcQuickTimeState {
        movie_file_data,
        movie_tasks_until_done: 2,
        movie_video_samples: Some(PpcQuickTimeVideoSampleTableRecord {
            media_time_scale: 1,
            media_duration: 2,
            sample_count: 2,
            codec: u32::from_be_bytes(*b"cvid"),
            samples: vec![
                PpcQuickTimeVideoSampleRecord {
                    offset: 0,
                    size: u32::try_from(first_sample.len()).unwrap(),
                    media_start_time: 0,
                    duration: 1,
                    data_len: u32::try_from(first_sample.len()).unwrap(),
                    checksum: 0,
                    preview_len: 0,
                    preview: [0; 16],
                },
                PpcQuickTimeVideoSampleRecord {
                    offset: second_offset,
                    size: u32::try_from(second_sample.len()).unwrap(),
                    media_start_time: 1,
                    duration: 1,
                    data_len: u32::try_from(second_sample.len()).unwrap(),
                    checksum: 0,
                    preview_len: 0,
                    preview: [0; 16],
                },
            ],
        }),
        ..PpcQuickTimeState::default()
    };

    let first_frame = ppc_qt_decode_current_movie_video_frame(&mut quicktime).unwrap();
    assert_eq!(
        quicktime
            .movie_video_decode_cache
            .as_ref()
            .map(|cache| cache.sample_index),
        Some(0)
    );
    quicktime.movie_file_data[10] = 0xff;
    quicktime.movie_task_count = 1;

    let second_frame = ppc_qt_decode_current_movie_video_frame(&mut quicktime).unwrap();

    assert_eq!(second_frame.rgb, first_frame.rgb);
    assert_eq!(
        quicktime
            .movie_video_decode_cache
            .as_ref()
            .map(|cache| cache.sample_index),
        Some(1)
    );
}

#[test]
fn quicktime_go_to_beginning_resets_video_decode_cache() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = PPC_QT_MOVIE;
    let mut sound = PpcSoundState::default();
    let mut quicktime = PpcQuickTimeState {
        movie_started: true,
        movie_task_count: 1,
        movie_video_decode_cache: Some(PpcQuickTimeVideoDecodeCacheRecord {
            codec: u32::from_be_bytes(*b"cvid"),
            width: 4,
            height: 4,
            sample_index: 1,
            rgb: vec![0; 4 * 4 * 3],
            cinepak_strips: vec![
                PpcQuickTimeCinepakStripState::default();
                PpcQuickTimeCinepakDecoder::MAX_STRIPS
            ],
        }),
        ..PpcQuickTimeState::default()
    };

    assert_eq!(
        ppc_qt_go_to_beginning_of_movie(&mut cpu, &mut quicktime, &mut sound),
        PPC_NO_ERR
    );

    assert!(!quicktime.movie_started);
    assert_eq!(quicktime.movie_task_count, 0);
    assert!(quicktime.movie_video_decode_cache.is_none());
}

fn external_ppc_fixture_archive() -> Option<std::path::PathBuf> {
    let Some(path) = std::env::var_os("SYSTEMLESS_PPC_TEST_ARCHIVE") else {
        eprintln!(
            "[TEST] skipping external PPC archive fixture: set SYSTEMLESS_PPC_TEST_ARCHIVE"
        );
        return None;
    };
    let archive_path = std::path::PathBuf::from(path);
    if !archive_path.is_file() {
        eprintln!(
            "[TEST] skipping external PPC archive fixture: missing {}",
            archive_path.display()
        );
        return None;
    }
    Some(archive_path)
}

#[test]
fn quicktime_decodes_cinepak_movie_prefixes_from_external_archive_when_configured() {
    let Some(archive_path) = external_ppc_fixture_archive() else {
        return;
    };
    let archive_bytes = std::fs::read(&archive_path).unwrap();
    let archive = stuffit::SitArchive::parse(&archive_bytes).unwrap();

    for movie_path in ["Data/Movies/Win.mov", "Data/Movies/Lose.mov"] {
        let entry = archive
            .entries
            .iter()
            .find(|entry| entry.name == movie_path)
            .unwrap_or_else(|| panic!("missing {movie_path} in {}", archive_path.display()));
        let (movie_file_data, _) = entry.decompressed_forks().unwrap();
        let video_samples = ppc_qt_movie_video_samples(&movie_file_data)
            .unwrap_or_else(|| panic!("{movie_path}: missing video sample table"));
        assert_eq!(video_samples.codec, u32::from_be_bytes(*b"cvid"));
        assert!(video_samples.sample_count >= 4);
        let mut quicktime = PpcQuickTimeState {
            movie_file_data,
            movie_tasks_until_done: u32::try_from(video_samples.media_duration).unwrap(),
            movie_video_samples: Some(video_samples.clone()),
            ..PpcQuickTimeState::default()
        };

        for sample_index in 0..4usize {
            quicktime.movie_task_count =
                u32::try_from(video_samples.samples[sample_index].media_start_time).unwrap();
            let frame = ppc_qt_decode_current_movie_video_frame(&mut quicktime)
                .unwrap_or_else(|| panic!("{movie_path}: decode sample {sample_index}"));
            assert_eq!(
                frame.rgb.len(),
                frame.width * frame.height * 3,
                "{movie_path}: decoded RGB buffer size"
            );
            assert!(frame.width > 0 && frame.height > 0);
            assert_eq!(
                quicktime
                    .movie_video_decode_cache
                    .as_ref()
                    .map(|cache| cache.sample_index),
                Some(sample_index),
                "{movie_path}: cache should advance to decoded sample"
            );
        }
    }
}

#[test]
fn quicktime_draws_boot_picts_from_external_archive_when_configured() {
    let Some(archive_path) = external_ppc_fixture_archive() else {
        return;
    };
    let archive_bytes = std::fs::read(&archive_path).unwrap();
    let archive = stuffit::SitArchive::parse(&archive_bytes).unwrap();

    for pict_path in ["Data/Images/Boot1.PICT", "Data/Images/Boot2.PICT"] {
        let entry = archive
            .entries
            .iter()
            .find(|entry| entry.name == pict_path)
            .unwrap_or_else(|| panic!("missing {pict_path} in {}", archive_path.display()));
        let (pict_data, _) = entry.decompressed_forks().unwrap();
        let mut memory = PpcSectionMem::new();
        let base = PPC_HEAP_BASE + 0x8000;
        memory.add_region(base, vec![0; 640 * 480 * 2]);
        let front_buffer = PpcFrontBuffer {
            base_addr: base,
            row_bytes: 640 * 2,
            width: 640,
            height: 480,
            depth: 16,
        };

        assert!(
            ppc_qt_draw_pict_source_to_16bpp(&mut memory, front_buffer, &pict_data),
            "{pict_path}: source PICT should render instead of falling back"
        );
        assert_ne!(
            memory.read_u16_be(base),
            Some(0),
            "{pict_path}: first destination pixel should be populated"
        );
    }
}

#[test]
fn quicktime_movie_frame_draw_uses_cinepak_sample_pixels() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"StartMovie");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gworld = 0x0600_3000;
    let gdevice = 0x0600_4000;
    let base = PPC_HEAP_BASE + 0x8000;
    let sample_offset = 12u64;
    let sample = test_cinepak_v1_frame_sample();
    let mut movie_file_data = vec![0; usize::try_from(sample_offset).unwrap()];
    movie_file_data.extend_from_slice(&sample);
    loaded.memory.add_region(base, vec![0xee; 4 * 4 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: base,
        gdevice,
        width: 4,
        height: 4,
        depth: 16,
        row_bytes: 8,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded.quicktime = PpcQuickTimeState {
        movie_file_data,
        movie_box: (0, 0, 4, 4),
        movie_tasks_until_done: 1,
        movie_video_samples: Some(PpcQuickTimeVideoSampleTableRecord {
            media_time_scale: 1,
            media_duration: 1,
            sample_count: 1,
            codec: u32::from_be_bytes(*b"cvid"),
            samples: vec![PpcQuickTimeVideoSampleRecord {
                offset: sample_offset,
                size: u32::try_from(sample.len()).unwrap(),
                media_start_time: 0,
                duration: 1,
                data_len: u32::try_from(sample.len()).unwrap(),
                checksum: sample.iter().copied().map(u32::from).sum(),
                preview_len: 0,
                preview: [0; 16],
            }],
        }),
        ..PpcQuickTimeState::default()
    };

    assert!(ppc_qt_draw_movie_frame(
        &mut loaded.memory,
        &loaded.gworlds,
        gworld,
        &mut loaded.quicktime,
        0x21,
    ));

    assert_eq!(loaded.memory.read_u16_be(base), Some(0x0000));
    assert_eq!(
        loaded.memory.read_u16_be(base + 4),
        Some(ppc_qt_rgb555_from_u8(85, 85, 85))
    );
    assert_eq!(
        loaded.memory.read_u16_be(base + 16),
        Some(ppc_qt_rgb555_from_u8(170, 170, 170))
    );
    assert_eq!(loaded.memory.read_u16_be(base + 30), Some(0x7fff));
}

#[test]
fn quicktime_movie_audio_decoder_iterates_sample_table_chunks() {
    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    let mut mdhd_payload = vec![0; 24];
    mdhd_payload[12..16].copy_from_slice(&22_050u32.to_be_bytes());
    mdhd_payload[16..20].copy_from_slice(&4u32.to_be_bytes());

    let mut hdlr_payload = vec![0; 24];
    hdlr_payload[4..8].copy_from_slice(b"mhlr");
    hdlr_payload[8..12].copy_from_slice(b"soun");

    let mut stsd_payload = Vec::new();
    push_u32(&mut stsd_payload, 0);
    push_u32(&mut stsd_payload, 1);
    push_u32(&mut stsd_payload, 36);
    stsd_payload.extend_from_slice(b"raw ");
    push_u32(&mut stsd_payload, 0);
    push_u16(&mut stsd_payload, 0);
    push_u16(&mut stsd_payload, 1);
    push_u16(&mut stsd_payload, 0);
    push_u16(&mut stsd_payload, 0);
    push_u32(&mut stsd_payload, 0);
    push_u16(&mut stsd_payload, 1);
    push_u16(&mut stsd_payload, 8);
    push_u16(&mut stsd_payload, 0);
    push_u16(&mut stsd_payload, 0);
    push_u32(&mut stsd_payload, 22_050u32 << 16);

    let mut stts_payload = Vec::new();
    push_u32(&mut stts_payload, 0);
    push_u32(&mut stts_payload, 1);
    push_u32(&mut stts_payload, 4);
    push_u32(&mut stts_payload, 1);

    let mut stsc_payload = Vec::new();
    push_u32(&mut stsc_payload, 0);
    push_u32(&mut stsc_payload, 1);
    push_u32(&mut stsc_payload, 1);
    push_u32(&mut stsc_payload, 2);
    push_u32(&mut stsc_payload, 1);

    let mut stsz_payload = Vec::new();
    push_u32(&mut stsz_payload, 0);
    push_u32(&mut stsz_payload, 1);
    push_u32(&mut stsz_payload, 4);

    let mut stco_payload = Vec::new();
    push_u32(&mut stco_payload, 0);
    push_u32(&mut stco_payload, 2);
    push_u32(&mut stco_payload, 8);
    push_u32(&mut stco_payload, 10);

    let mut stbl_payload = Vec::new();
    push_quicktime_atom(&mut stbl_payload, b"stsd", &stsd_payload);
    push_quicktime_atom(&mut stbl_payload, b"stts", &stts_payload);
    push_quicktime_atom(&mut stbl_payload, b"stsc", &stsc_payload);
    push_quicktime_atom(&mut stbl_payload, b"stsz", &stsz_payload);
    push_quicktime_atom(&mut stbl_payload, b"stco", &stco_payload);

    let mut minf_payload = Vec::new();
    push_quicktime_atom(&mut minf_payload, b"stbl", &stbl_payload);

    let mut mdia_payload = Vec::new();
    push_quicktime_atom(&mut mdia_payload, b"mdhd", &mdhd_payload);
    push_quicktime_atom(&mut mdia_payload, b"hdlr", &hdlr_payload);
    push_quicktime_atom(&mut mdia_payload, b"minf", &minf_payload);

    let mut trak_payload = Vec::new();
    push_quicktime_atom(&mut trak_payload, b"mdia", &mdia_payload);

    let mut moov_payload = Vec::new();
    push_quicktime_atom(&mut moov_payload, b"trak", &trak_payload);

    let mut movie_file_data = Vec::new();
    push_quicktime_atom(&mut movie_file_data, b"mdat", &[0x80, 0x90, 0x70, 0x60]);
    push_quicktime_atom(&mut movie_file_data, b"moov", &moov_payload);

    let decoded = ppc_qt_decode_movie_audio_samples(&PpcQuickTimeState {
        movie_file_data,
        movie_audio_track: Some(PpcQuickTimeAudioTrackRecord {
            media_time_scale: 22_050,
            media_duration: 4,
            sample_count: 4,
            first_sample_duration: 1,
            first_sample_size: 1,
            first_chunk_offset: 8,
            first_sample_data_len: 1,
            first_sample_checksum: 0,
            first_sample_preview_len: 0,
            first_sample_preview: [0; 16],
            first_samples_per_chunk: 2,
            sample_description_id: 1,
            codec: u32::from_be_bytes(*b"raw "),
            channel_count: 1,
            sample_size_bits: 8,
            sample_rate_fixed: 22_050u32 << 16,
        }),
        ..PpcQuickTimeState::default()
    })
    .expect("all audio chunks decode");

    assert_eq!(decoded.samples, vec![0x80, 0x90, 0x70, 0x60]);
}

#[test]
fn quicktime_movie_audio_decoder_decodes_ima4_first_chunk() {
    let mut movie_file_data = vec![0; 12];
    movie_file_data.extend_from_slice(&[0x40, 0x00]);
    movie_file_data.extend_from_slice(&[0; 32]);
    let decoded = ppc_qt_decode_movie_audio_samples(&PpcQuickTimeState {
        movie_file_data,
        movie_audio_track: Some(PpcQuickTimeAudioTrackRecord {
            media_time_scale: 44_100,
            media_duration: 64,
            sample_count: 64,
            first_sample_duration: 1,
            first_sample_size: 1,
            first_chunk_offset: 12,
            first_sample_data_len: 1,
            first_sample_checksum: 0,
            first_sample_preview_len: 0,
            first_sample_preview: [0; 16],
            first_samples_per_chunk: 64,
            sample_description_id: 1,
            codec: u32::from_be_bytes(*b"ima4"),
            channel_count: 1,
            sample_size_bits: 16,
            sample_rate_fixed: 44_100u32 << 16,
        }),
        ..PpcQuickTimeState::default()
    })
    .expect("IMA4 movie chunk decodes");

    assert_eq!(decoded.samples, vec![0xc0; 64]);
    assert_eq!(
        decoded.summary,
        PpcDecodedAiffSamples {
            sample_rate_fixed: 44_100u32 << 16,
            sample_count: 64,
            preview_len: 16,
            preview: [0xc0; 16],
        }
    );
}

#[test]
fn quicktime_start_movie_audio_hands_off_ima4_to_sound_state() {
    let mut movie_file_data = vec![0; 12];
    movie_file_data.extend_from_slice(&[0x00, 0x00]);
    movie_file_data.extend_from_slice(&[0; 32]);
    let quicktime = PpcQuickTimeState {
        movie_file_ref_num: PPC_FIRST_FILE_REF_NUM,
        movie_file_data,
        movie_audio_track: Some(PpcQuickTimeAudioTrackRecord {
            media_time_scale: 44_100,
            media_duration: 64,
            sample_count: 64,
            first_sample_duration: 1,
            first_sample_size: 1,
            first_chunk_offset: 12,
            first_sample_data_len: 1,
            first_sample_checksum: 0,
            first_sample_preview_len: 0,
            first_sample_preview: [0; 16],
            first_samples_per_chunk: 64,
            sample_description_id: 1,
            codec: u32::from_be_bytes(*b"ima4"),
            channel_count: 1,
            sample_size_bits: 16,
            sample_rate_fixed: 44_100u32 << 16,
        }),
        ..PpcQuickTimeState::default()
    };
    let mut sound = PpcSoundState::default();

    assert!(ppc_qt_start_movie_audio(&quicktime, &mut sound));
    assert_eq!(sound.file_playbacks.len(), 1);
    assert_eq!(
        sound.file_playbacks[0].decoded_aiff,
        Some(PpcDecodedAiffSamples {
            sample_rate_fixed: 44_100u32 << 16,
            sample_count: 64,
            preview_len: 16,
            preview: [0x80; 16],
        })
    );
    assert_eq!(
        sound.decoded_file_playbacks,
        vec![PpcDecodedAiffPlaybackRecord {
            file_playback_index: 0,
            channel: PPC_QT_MOVIE,
            sample_rate_fixed: 44_100u32 << 16,
            samples: vec![0x80; 64],
        }]
    );
}

#[test]
fn hle_import_runner_handles_quicktime_importer_and_movie_outputs() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetGraphicsImporterForFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let importer_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(importer_out_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = PPC_DATA_BASE;
    loaded.cpu.gpr[4] = importer_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(importer_out_ptr),
        Some(PPC_QT_GRAPHICS_IMPORTER)
    );
    assert!(loaded.quicktime.graphics_importer_open);

    let pef =
        synthetic_pef_with_library_import(b"QuickTimeLib", b"GraphicsImportGetBoundsRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 8]);
    loaded.quicktime.graphics_importer_open = true;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = bounds_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, bounds_ptr),
        Some((
            0,
            0,
            ppc_main_screen_height() as i16,
            ppc_main_screen_width() as i16
        ))
    );

    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"OpenMovieFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let ref_num_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(ref_num_out_ptr, vec![0; 2]);
    loaded.cpu.gpr[3] = PPC_DATA_BASE;
    loaded.cpu.gpr[4] = ref_num_out_ptr;
    loaded.cpu.gpr[5] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u16_be(ref_num_out_ptr),
        Some(PPC_FIRST_FILE_REF_NUM as u16)
    );

    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"NewMovieFromFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let movie_out_ptr = scratch;
    let res_id_ptr = scratch + 4;
    let data_ref_changed_ptr = scratch + 6;
    loaded.memory.add_region(scratch, vec![0xaa; 16]);
    loaded.cpu.gpr[3] = movie_out_ptr;
    loaded.cpu.gpr[4] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
    loaded.cpu.gpr[5] = res_id_ptr;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = data_ref_changed_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(movie_out_ptr), Some(PPC_QT_MOVIE));
    assert_eq!(loaded.memory.read_u16_be(res_id_ptr), Some(0));
    assert_eq!(loaded.memory.read_u8(data_ref_changed_ptr), Some(0));

    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetMovieBox");
    let mut loaded = load_pef_application(&pef).unwrap();
    let box_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(box_ptr, vec![0; 8]);
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = box_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_QT_MOVIE);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, box_ptr),
        Some((
            0,
            0,
            ppc_main_screen_height() as i16,
            ppc_main_screen_width() as i16
        ))
    );

    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"IsMovieDone");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
}

#[test]
fn hle_import_runner_quicktime_new_movie_from_file_quiets_previous_movie_audio() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"NewMovieFromFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let movie_out_ptr = scratch;
    let res_id_ptr = scratch + 4;
    let data_ref_changed_ptr = scratch + 6;
    loaded.memory.add_region(scratch, vec![0xaa; 16]);
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_task_count = 9;
    loaded
        .sound
        .file_playbacks
        .push(test_sound_file_playback(PPC_QT_MOVIE));
    loaded
        .sound
        .file_playbacks
        .push(test_sound_file_playback(0x1234_5678));
    loaded.sound.manager.play_file_buffer(
        PPC_QT_MOVIE,
        vec![0x80],
        crate::sound::OUTPUT_RATE << 16,
        None,
    );
    loaded.sound.manager.play_file_buffer(
        0x1234_5678,
        vec![0x80],
        crate::sound::OUTPUT_RATE << 16,
        None,
    );
    loaded.cpu.gpr[3] = movie_out_ptr;
    loaded.cpu.gpr[4] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
    loaded.cpu.gpr[5] = res_id_ptr;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = data_ref_changed_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(movie_out_ptr), Some(PPC_QT_MOVIE));
    assert_eq!(loaded.memory.read_u16_be(res_id_ptr), Some(0));
    assert_eq!(loaded.memory.read_u8(data_ref_changed_ptr), Some(0));
    assert!(!loaded.quicktime.movie_started);
    assert_eq!(loaded.quicktime.movie_task_count, 0);
    assert!(loaded.quicktime.movie_at_beginning);
    assert_eq!(
        loaded.sound.file_playbacks[0],
        test_sound_file_playback(PPC_QT_MOVIE)
    );
    assert_eq!(
        loaded.sound.file_playbacks[1],
        test_sound_file_playback(0x1234_5678)
    );
    assert_eq!(
        loaded.sound.manager.file_playback_paused(PPC_QT_MOVIE),
        None
    );
    assert_eq!(
        loaded.sound.manager.file_playback_paused(0x1234_5678),
        Some(false)
    );
}

#[test]
fn hle_import_runner_prevalidates_quicktime_output_buffers() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetGraphicsImporterForFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let importer_out_ptr = PPC_DATA_BASE + 0x1000;
    let bounds_ptr = PPC_DATA_BASE + 0x1100;
    let ref_num_out_ptr = PPC_DATA_BASE + 0x1200;
    let movie_out_ptr = PPC_DATA_BASE + 0x1300;
    let res_id_ptr = PPC_DATA_BASE + 0x1400;
    let data_ref_changed_ptr = PPC_DATA_BASE + 0x1500;
    let box_ptr = PPC_DATA_BASE + 0x1600;
    loaded.memory.add_region(importer_out_ptr, vec![0xd1; 3]);
    loaded.memory.add_region(bounds_ptr, vec![0xd2; 7]);
    loaded.memory.add_region(ref_num_out_ptr, vec![0xd3; 1]);
    loaded.memory.add_region(movie_out_ptr, vec![0xd4; 4]);
    loaded.memory.add_region(res_id_ptr, vec![0xd5; 1]);
    loaded
        .memory
        .add_region(data_ref_changed_ptr, vec![0xd6; 1]);
    loaded.memory.add_region(box_ptr, vec![0xd7; 7]);
    loaded.quicktime.graphics_importer_path = "Previous PICT".to_string();
    loaded.quicktime.graphics_importer_data = b"previous".to_vec();
    loaded.quicktime.graphics_importer_bounds = Some((1, 2, 3, 4));
    loaded.quicktime.graphics_importer_open = true;
    let previous_video_track = PpcQuickTimeVideoTrackRecord {
        media_time_scale: 30,
        media_duration: 90,
        sample_count: 3,
        first_sample_duration: 30,
        first_sample_size: 12,
        first_chunk_offset: 0x44,
        first_sample_data_len: 0,
        first_sample_checksum: 0,
        first_sample_preview_len: 0,
        first_sample_preview: [0; 16],
        first_samples_per_chunk: 3,
        sample_description_id: 1,
        codec: u32::from_be_bytes(*b"jpeg"),
    };
    let previous_video_samples = PpcQuickTimeVideoSampleTableRecord {
        media_time_scale: 30,
        media_duration: 90,
        sample_count: 1,
        codec: u32::from_be_bytes(*b"jpeg"),
        samples: vec![PpcQuickTimeVideoSampleRecord {
            offset: 0x44,
            size: 12,
            media_start_time: 0,
            duration: 90,
            data_len: 0,
            checksum: 0,
            preview_len: 0,
            preview: [0; 16],
        }],
    };
    let previous_audio_track = PpcQuickTimeAudioTrackRecord {
        media_time_scale: 11_025,
        media_duration: 2205,
        sample_count: 2205,
        first_sample_duration: 1,
        first_sample_size: 1,
        first_chunk_offset: 0x88,
        first_sample_data_len: 0,
        first_sample_checksum: 0,
        first_sample_preview_len: 0,
        first_sample_preview: [0; 16],
        first_samples_per_chunk: 2205,
        sample_description_id: 1,
        codec: u32::from_be_bytes(*b"twos"),
        channel_count: 1,
        sample_size_bits: 8,
        sample_rate_fixed: 11_025u32 << 16,
    };
    loaded.quicktime.movie_file_path = "Previous.mov".to_string();
    loaded.quicktime.movie_file_data = b"previous movie".to_vec();
    loaded.quicktime.movie_file_bounds = Some((5, 6, 7, 8));
    loaded.quicktime.movie_file_time_scale = 30;
    loaded.quicktime.movie_file_duration = 90;
    loaded.quicktime.movie_file_tasks_until_done = 180;
    loaded.quicktime.movie_file_video_track = Some(previous_video_track);
    loaded.quicktime.movie_file_video_samples = Some(previous_video_samples.clone());
    loaded.quicktime.movie_file_audio_track = Some(previous_audio_track);
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = importer_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    for offset in 0..3 {
        assert_eq!(loaded.memory.read_u8(importer_out_ptr + offset), Some(0xd1));
    }
    assert_eq!(loaded.quicktime.graphics_importer_path, "Previous PICT");
    assert_eq!(loaded.quicktime.graphics_importer_data, b"previous");
    assert_eq!(
        loaded.quicktime.graphics_importer_bounds,
        Some((1, 2, 3, 4))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::QtGraphicsImportGetBoundsRect;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = bounds_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    for offset in 0..7 {
        assert_eq!(loaded.memory.read_u8(bounds_ptr + offset), Some(0xd2));
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtOpenMovieFile;
    loaded.cpu.gpr[3] = PPC_DATA_BASE;
    loaded.cpu.gpr[4] = ref_num_out_ptr;
    loaded.cpu.gpr[5] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u8(ref_num_out_ptr), Some(0xd3));
    assert_eq!(loaded.quicktime.movie_file_open_count, 0);
    assert_eq!(loaded.quicktime.movie_file_ref_num, 0);
    assert_eq!(loaded.quicktime.movie_file_path, "Previous.mov");
    assert_eq!(loaded.quicktime.movie_file_data, b"previous movie");
    assert_eq!(loaded.quicktime.movie_file_bounds, Some((5, 6, 7, 8)));
    assert_eq!(loaded.quicktime.movie_file_time_scale, 30);
    assert_eq!(loaded.quicktime.movie_file_duration, 90);
    assert_eq!(loaded.quicktime.movie_file_tasks_until_done, 180);
    assert_eq!(
        loaded.quicktime.movie_file_video_track,
        Some(previous_video_track)
    );
    assert_eq!(
        loaded.quicktime.movie_file_video_samples,
        Some(previous_video_samples.clone())
    );
    assert_eq!(
        loaded.quicktime.movie_file_audio_track,
        Some(previous_audio_track)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtNewMovieFromFile;
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_task_count = 7;
    loaded.quicktime.movie_disposed = true;
    loaded.quicktime.movie_at_beginning = false;
    loaded.quicktime.movie_tasks_until_done = 42;
    loaded.quicktime.movie_video_track = Some(previous_video_track);
    loaded.quicktime.movie_video_samples = Some(previous_video_samples.clone());
    loaded.quicktime.movie_audio_track = Some(previous_audio_track);
    loaded
        .sound
        .file_playbacks
        .push(test_sound_file_playback(PPC_QT_MOVIE));
    loaded.cpu.gpr[3] = movie_out_ptr;
    loaded.cpu.gpr[4] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
    loaded.cpu.gpr[5] = res_id_ptr;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = data_ref_changed_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(movie_out_ptr), Some(0xd4d4_d4d4));
    assert_eq!(loaded.memory.read_u8(res_id_ptr), Some(0xd5));
    assert_eq!(loaded.memory.read_u8(data_ref_changed_ptr), Some(0xd6));
    assert!(loaded.quicktime.movie_started);
    assert_eq!(loaded.quicktime.movie_task_count, 7);
    assert!(loaded.quicktime.movie_disposed);
    assert!(!loaded.quicktime.movie_at_beginning);
    assert_eq!(loaded.quicktime.movie_tasks_until_done, 42);
    assert_eq!(
        loaded.quicktime.movie_video_track,
        Some(previous_video_track)
    );
    assert_eq!(
        loaded.quicktime.movie_video_samples,
        Some(previous_video_samples)
    );
    assert_eq!(
        loaded.quicktime.movie_audio_track,
        Some(previous_audio_track)
    );
    assert_eq!(
        loaded.sound.file_playbacks[0],
        test_sound_file_playback(PPC_QT_MOVIE)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGetMovieBox;
    loaded.quicktime.movie_disposed = false;
    loaded.quicktime.movie_box = (10, 20, 30, 40);
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = box_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_QT_MOVIE);
    for offset in 0..7 {
        assert_eq!(loaded.memory.read_u8(box_ptr + offset), Some(0xd7));
    }
    assert_eq!(loaded.quicktime.movie_box, (10, 20, 30, 40));
    assert_eq!(loaded.quicktime.movie_error, PPC_PARAM_ERR);
}

#[test]
fn hle_import_runner_tracks_quicktime_gworld_targets_and_visible_draws() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GraphicsImportSetGWorld");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gworld = 0x0600_1000;
    let gdevice = 0x0600_2000;
    let base = PPC_HEAP_BASE + 0x6000;
    loaded.memory.add_region(base, vec![0; 4 * 3 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: base,
        gdevice,
        width: 4,
        height: 3,
        depth: 16,
        row_bytes: 8,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded.quicktime.graphics_importer_open = true;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = gworld;
    loaded.cpu.gpr[5] = gdevice;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.graphics_importer_gworld, gworld);
    assert_eq!(loaded.quicktime.graphics_importer_gdevice, gdevice);
    assert_eq!(loaded.memory.read_u16_be(base), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGraphicsImportDraw;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.graphics_import_draw_count, 1);
    assert_ne!(loaded.memory.read_u16_be(base), Some(0));
    assert_ne!(loaded.memory.read_u16_be(base + 4 * 3 * 2 - 2), Some(0));

    let _ = loaded.memory.write_u16_be(base, 0);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtSetMovieGWorld;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = gworld;
    loaded.cpu.gpr[5] = gdevice;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.movie_gworld, gworld);
    assert_eq!(loaded.quicktime.movie_gdevice, gdevice);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtStartMovie;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.quicktime.movie_active = true;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.quicktime.movie_started);
    assert_ne!(loaded.memory.read_u16_be(base), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtIsMovieDone;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_error, PPC_NO_ERR);

    loaded
        .sound
        .file_playbacks
        .push(test_sound_file_playback(PPC_QT_MOVIE));
    loaded.sound.manager.play_file_buffer(
        PPC_QT_MOVIE,
        vec![0x80],
        crate::sound::OUTPUT_RATE << 16,
        None,
    );
    assert_eq!(
        loaded.sound.manager.toggle_file_paused(PPC_QT_MOVIE),
        Some(true)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtMoviesTask;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.movie_task_count, 1);
    assert!(loaded.quicktime.movie_started);
    assert_eq!(
        loaded.sound.manager.file_playback_paused(PPC_QT_MOVIE),
        Some(true)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtIsMovieDone;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    for expected_count in 2..=PPC_QT_FALLBACK_MOVIE_TASKS_UNTIL_DONE {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtMoviesTask;
        loaded.cpu.gpr[3] = PPC_QT_MOVIE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.quicktime.movie_task_count, expected_count);
    }
    assert!(!loaded.quicktime.movie_started);
    assert_eq!(
        loaded.sound.manager.file_playback_paused(PPC_QT_MOVIE),
        None
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtIsMovieDone;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtStopMovie;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(!loaded.quicktime.movie_started);
}

#[test]
fn hle_import_runner_close_component_closes_quicktime_importer() {
    let pef = synthetic_pef_with_import(b"CloseComponent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0xcc; 8]);
    loaded.quicktime.graphics_importer_open = true;
    loaded.quicktime.graphics_importer_gworld = 0x0600_1000;
    loaded.quicktime.graphics_importer_gdevice = 0x0600_2000;
    loaded.quicktime.graphics_importer_path = "Splash Pict".to_string();
    loaded.quicktime.graphics_importer_data = b"pict".to_vec();
    loaded.quicktime.graphics_importer_bounds = Some((1, 2, 3, 4));
    loaded.quicktime.graphics_import_draw_count = 7;
    loaded.quicktime.graphics_import_source_draw_count = 3;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(!loaded.quicktime.graphics_importer_open);
    assert_eq!(loaded.quicktime.graphics_importer_gworld, 0);
    assert_eq!(loaded.quicktime.graphics_importer_gdevice, 0);
    assert!(loaded.quicktime.graphics_importer_path.is_empty());
    assert!(loaded.quicktime.graphics_importer_data.is_empty());
    assert_eq!(loaded.quicktime.graphics_importer_bounds, None);
    assert_eq!(loaded.quicktime.graphics_import_draw_count, 7);
    assert_eq!(loaded.quicktime.graphics_import_source_draw_count, 3);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::QtGraphicsImportGetBoundsRect;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = bounds_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    for offset in 0..8 {
        assert_eq!(loaded.memory.read_u8(bounds_ptr + offset), Some(0xcc));
    }
}

#[test]
fn hle_import_runner_close_component_rejects_unknown_instances() {
    let pef = synthetic_pef_with_import(b"CloseComponent");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.graphics_importer_open = true;
    loaded.quicktime.graphics_importer_path = "Still Open".to_string();
    loaded.cpu.gpr[3] = 0x0bad_cafe;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_INVALID_COMPONENT_ID));
    assert!(loaded.quicktime.graphics_importer_open);
    assert_eq!(loaded.quicktime.graphics_importer_path, "Still Open");

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(!loaded.quicktime.graphics_importer_open);
}

#[test]
fn hle_import_runner_draws_quicktime_pict_file_into_gworld() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetGraphicsImporterForFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let importer_out_ptr = scratch + 80;
    let bounds_ptr = scratch + 96;
    let gworld = 0x0600_3000;
    let gdevice = 0x0600_4000;
    let base = PPC_HEAP_BASE + 0x7000;
    loaded.memory.add_region(scratch, vec![0; 128]);
    loaded.memory.add_region(base, vec![0xff; 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: base,
        gdevice,
        width: 8,
        height: 1,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Splash Pict".to_string(),
        data: (test_v1_one_bit_packbits_pict()).into(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"PICT"),
        finder_flags: 0,
        dirty: false,
    });
    write_ppc_fsspec(
        &mut loaded.memory,
        scratch,
        PPC_BOOT_VOLUME_REF_NUM,
        PPC_ROOT_DIR_ID,
        b"Splash Pict",
    );
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = importer_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(importer_out_ptr),
        Some(PPC_QT_GRAPHICS_IMPORTER)
    );
    assert_eq!(loaded.quicktime.graphics_importer_path, "Splash Pict");
    assert_eq!(
        loaded.quicktime.graphics_importer_bounds,
        Some((0, 0, 1, 8))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::QtGraphicsImportGetBoundsRect;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = bounds_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, bounds_ptr),
        Some((0, 0, 1, 8))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGraphicsImportSetGWorld;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = gworld;
    loaded.cpu.gpr[5] = gdevice;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGraphicsImportDraw;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.graphics_import_draw_count, 1);
    assert_eq!(loaded.quicktime.graphics_import_source_draw_count, 1);
    let pixels: Vec<u16> = (0..8)
        .map(|x| loaded.memory.read_u16_be(base + x * 2).unwrap())
        .collect();
    assert_eq!(
        pixels,
        vec![0x0000, 0x7fff, 0x0000, 0x7fff, 0x7fff, 0x7fff, 0x7fff, 0x7fff]
    );
}

#[test]
fn hle_import_runner_quicktime_importer_resolves_unique_archive_suffix_path() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetGraphicsImporterForFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let importer_out_ptr = scratch + 80;
    let images_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
    let mut directories = initial_ppc_vfs_directories();
    directories.push(PpcVfsDirectory {
        dir_id: images_dir_id,
        parent_dir_id: PPC_ROOT_DIR_ID,
        path: "Images".to_string(),
        creator: PPC_DIRECTORY_CREATOR,
        file_type: PPC_DIRECTORY_FILE_TYPE,
        finder_flags: 0,
        dirty: false,
    });
    loaded.seed_vfs_directories(directories, PPC_ROOT_DIR_ID, images_dir_id + 1);
    loaded.memory.add_region(scratch, vec![0; 128]);
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Data/Images/Boot1.PICT".to_string(),
        data: (test_v1_one_bit_packbits_pict()).into(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"PICT"),
        finder_flags: 0,
        dirty: false,
    });
    write_ppc_fsspec(
        &mut loaded.memory,
        scratch,
        PPC_BOOT_VOLUME_REF_NUM,
        images_dir_id,
        b"Boot1.pict",
    );
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = importer_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(importer_out_ptr),
        Some(PPC_QT_GRAPHICS_IMPORTER)
    );
    assert_eq!(
        loaded.quicktime.graphics_importer_path,
        "Data/Images/Boot1.PICT"
    );
    assert_eq!(
        loaded.quicktime.graphics_importer_bounds,
        Some((0, 0, 1, 8))
    );
}

#[test]
fn import_bindings_classify_quicktime_compatibility_imports() {
    for (symbol, operation) in [
        ("GetMovieActive", PpcQuickTimeCompatibilityOperation::GetMovieActive),
        (
            "GetMovieTimeBase",
            PpcQuickTimeCompatibilityOperation::GetMovieTimeBase,
        ),
        (
            "GetMovieTimeScale",
            PpcQuickTimeCompatibilityOperation::GetMovieTimeScale,
        ),
        (
            "GetMoviePreferredRate",
            PpcQuickTimeCompatibilityOperation::GetMoviePreferredRate,
        ),
        (
            "GetMoviePreferredVolume",
            PpcQuickTimeCompatibilityOperation::GetMoviePreferredVolume,
        ),
        (
            "GetMovieIndTrackType",
            PpcQuickTimeCompatibilityOperation::GetMovieIndTrackType,
        ),
        (
            "GetMovieUserData",
            PpcQuickTimeCompatibilityOperation::GetMovieUserData,
        ),
        ("GetUserData", PpcQuickTimeCompatibilityOperation::GetUserData),
        ("GetTrackMedia", PpcQuickTimeCompatibilityOperation::GetTrackMedia),
        (
            "GetMediaSampleDescription",
            PpcQuickTimeCompatibilityOperation::GetMediaSampleDescription,
        ),
        ("MCDoAction", PpcQuickTimeCompatibilityOperation::MCDoAction),
        (
            "NewMovieController",
            PpcQuickTimeCompatibilityOperation::NewMovieController,
        ),
        (
            "DisposeMovieController",
            PpcQuickTimeCompatibilityOperation::DisposeMovieController,
        ),
        ("MCSetMovie", PpcQuickTimeCompatibilityOperation::MCSetMovie),
        (
            "MCGetControllerBoundsRect",
            PpcQuickTimeCompatibilityOperation::MCGetControllerBoundsRect,
        ),
        (
            "MCSetActionFilterWithRefCon",
            PpcQuickTimeCompatibilityOperation::MCSetActionFilterWithRefCon,
        ),
        (
            "MCSetControllerBoundsRect",
            PpcQuickTimeCompatibilityOperation::MCSetControllerBoundsRect,
        ),
        ("MCIdle", PpcQuickTimeCompatibilityOperation::MCIdle),
        (
            "MCGetCurrentTime",
            PpcQuickTimeCompatibilityOperation::MCGetCurrentTime,
        ),
        (
            "GetMovieVolume",
            PpcQuickTimeCompatibilityOperation::GetMovieVolume,
        ),
        (
            "NewMovieFromDataFork",
            PpcQuickTimeCompatibilityOperation::NewMovieFromDataFork,
        ),
        ("PrerollMovie", PpcQuickTimeCompatibilityOperation::PrerollMovie),
        ("SetMovieActive", PpcQuickTimeCompatibilityOperation::SetMovieActive),
        (
            "SetMovieVolume",
            PpcQuickTimeCompatibilityOperation::SetMovieVolume,
        ),
        (
            "SetTimeBaseFlags",
            PpcQuickTimeCompatibilityOperation::SetTimeBaseFlags,
        ),
        ("UpdateMovie", PpcQuickTimeCompatibilityOperation::UpdateMovie),
    ] {
        assert_eq!(
            dispatcher_target_for_import("QuickTimeLib", symbol),
            PpcImportDispatcherTarget::QuickTimeCompatibility(operation),
        );
    }
}

#[test]
fn quicktime_movie_activation_controls_task_progress() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"SetMovieActive");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 1;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.quicktime.movie_active);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QuickTimeCompatibility(
        PpcQuickTimeCompatibilityOperation::GetMovieActive,
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QuickTimeCompatibility(
        PpcQuickTimeCompatibilityOperation::SetMovieActive,
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 0;
    loaded.run_with_hle_imports(64);
    assert!(!loaded.quicktime.movie_active);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtMoviesTask;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.quicktime.movie_task_count, 0);
}

#[test]
fn quicktime_movie_time_scale_uses_loaded_metadata_and_rejects_invalid_movies() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetMovieTimeScale");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.movie_time_scale = 30;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 30);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_error, -2010);
}

#[test]
fn quicktime_movie_preferred_rate_reads_mvhd_and_rejects_invalid_movies() {
    let mut mvhd = vec![0; 24];
    mvhd[12..16].copy_from_slice(&60u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&120u32.to_be_bytes());
    mvhd[20..24].copy_from_slice(&0x0001_8000u32.to_be_bytes());
    let mut moov = Vec::new();
    push_quicktime_atom(&mut moov, b"mvhd", &mvhd);
    let mut movie = Vec::new();
    push_quicktime_atom(&mut movie, b"moov", &moov);
    assert_eq!(ppc_qt_movie_preferred_rate(&movie), Some(0x0001_8000));
    assert_eq!(ppc_qt_movie_preferred_rate(&movie[..movie.len() - 1]), None);

    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetMoviePreferredRate");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.movie_preferred_rate = 0x0001_8000;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x0001_8000);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_error, -2010);
}

#[test]
fn quicktime_movie_preferred_volume_reads_mvhd_and_rejects_invalid_movies() {
    let mut mvhd = vec![0; 26];
    mvhd[12..16].copy_from_slice(&60u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&120u32.to_be_bytes());
    mvhd[20..24].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    mvhd[24..26].copy_from_slice(&0x0080u16.to_be_bytes());
    let mut moov = Vec::new();
    push_quicktime_atom(&mut moov, b"mvhd", &mvhd);
    let mut movie = Vec::new();
    push_quicktime_atom(&mut movie, b"moov", &moov);
    assert_eq!(ppc_qt_movie_preferred_volume(&movie), Some(0x0080));
    assert_eq!(ppc_qt_movie_preferred_volume(&movie[..movie.len() - 1]), None);

    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetMoviePreferredVolume");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.movie_preferred_volume = 0x0080;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x0080);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_error, -2010);
}

#[test]
fn quicktime_movie_track_type_filters_media_and_enabled_tracks() {
    let movie = test_quicktime_tkhd_movie(320, 240);
    let tracks = ppc_qt_movie_tracks(&movie);
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].id, 1);
    assert_eq!(tracks[0].media_type, u32::from_be_bytes(*b"vide"));
    assert!(tracks[0].enabled);
    assert_eq!(tracks[1].id, 2);
    assert_eq!(tracks[1].media_type, u32::from_be_bytes(*b"soun"));

    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetMovieIndTrackType");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.movie_tracks = tracks;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"vide");
    loaded.cpu.gpr[6] = 1 | 4; // movieTrackMediaType | movieTrackEnabledOnly
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], loaded.quicktime.movie_tracks[0].handle);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"ears");
    loaded.cpu.gpr[6] = 2 | 4; // movieTrackCharacteristic | movieTrackEnabledOnly
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], loaded.quicktime.movie_tracks[1].handle);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 2;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.quicktime.movie_tracks[1].enabled = false;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_error, -2010);
}

#[test]
fn quicktime_get_movie_user_data_reflects_udta_atom() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetMovieUserData");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.movie_file_data = [
        0, 0, 0, 16, b'm', b'o', b'o', b'v', 0, 0, 0, 8, b'u', b'd', b't', b'a',
    ]
    .to_vec();
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    let reference = loaded.cpu.gpr[3];
    assert_ne!(reference, 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], reference);

    loaded.quicktime.movie_file_data = vec![0, 0, 0, 8, b'm', b'o', b'o', b'v'];
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_error, -2010);
}

#[test]
fn quicktime_get_user_data_reads_indexed_items_into_handle() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetUserData");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.movie_file_data = [
        0, 0, 0, 40, b'm', b'o', b'o', b'v',
        0, 0, 0, 32, b'u', b'd', b't', b'a',
        0, 0, 0, 12, b'n', b'a', b'm', b'e', b'A', b'B', b'C', b'D',
        0, 0, 0, 12, b'n', b'a', b'm', b'e', b'E', b'F', b'G', b'H',
    ].to_vec();
    let handle = loaded.process_memory_manager.0.borrow_mut().new_native_handle(&mut loaded.memory, 0, false);
    assert_ne!(handle, 0);
    loaded.cpu.gpr[3] = PPC_QT_MOVIE + 0x20;
    loaded.cpu.gpr[4] = handle;
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"name");
    loaded.cpu.gpr[6] = 2;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!((0..4).map(|i| loaded.memory.read_u8(ptr + i).unwrap()).collect::<Vec<_>>(), b"EFGH");

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE + 0x20;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"name");
    loaded.cpu.gpr[6] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE + 0x20;
    loaded.cpu.gpr[6] = 3;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-2026));

    loaded.quicktime.movie_file_data = vec![
        0, 0, 0, 20, b'm', b'o', b'o', b'v',
        0, 0, 0, 12, b'u', b'd', b't', b'a',
        0, 0, 0, 40,
    ];
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE + 0x20;
    loaded.cpu.gpr[6] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-2026));
}

#[test]
fn quicktime_get_track_media_returns_stable_media_reference() {
    let tracks = ppc_qt_movie_tracks(&test_quicktime_tkhd_movie(320, 240));
    assert!(!tracks.is_empty());
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetTrackMedia");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.movie_tracks = tracks;
    loaded.cpu.gpr[3] = loaded.quicktime.movie_tracks[0].handle;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    let media = loaded.cpu.gpr[3];
    assert_ne!(media, 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = loaded.quicktime.movie_tracks[0].handle;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], media);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_error, -2009);
}

#[test]
fn quicktime_get_media_sample_description_copies_stsd_entry() {
    let movie = test_quicktime_tkhd_movie(320, 240);
    let tracks = ppc_qt_movie_tracks(&movie);
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GetMediaSampleDescription");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.movie_file_data = movie;
    loaded.quicktime.movie_tracks = tracks;
    let handle = loaded.process_memory_manager.0.borrow_mut().new_native_handle(&mut loaded.memory, 0, false);
    let media = loaded.quicktime.movie_tracks[0].handle + 1;
    loaded.cpu.gpr[3] = media;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = handle;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quicktime.movie_error, 0);
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(ptr), Some(16));
    assert_eq!(loaded.memory.read_u32_be(ptr + 4), Some(u32::from_be_bytes(*b"rle ")));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = media;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = handle;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.memory.read_u32_be(handle), Some(ptr));
    assert_eq!(loaded.memory.read_u32_be(ptr + 4), Some(u32::from_be_bytes(*b"rle ")));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.quicktime.movie_error, -2008);
}

#[test]
fn quicktime_mc_do_action_rejects_unknown_controller() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCDoAction");
    let mut loaded = load_pef_application(&pef).unwrap();
    let params = PPC_DATA_BASE + 0x1800;
    loaded.memory.add_region(params, vec![0x5a; 16]);
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 29; // mcActionSetSelectionBegin
    loaded.cpu.gpr[5] = params;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_INVALID_COMPONENT_ID));
    assert_eq!(loaded.memory.read_u32_be(params), Some(0x5a5a_5a5a));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0x1234_5678;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_INVALID_COMPONENT_ID));
}

#[test]
fn quicktime_mc_do_action_play_starts_and_stops_controller_movie() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCDoAction");
    let mut loaded = load_pef_application(&pef).unwrap();
    let controller = 0x4001_3010;
    loaded.quicktime.movie_controllers.push(PpcQuickTimeControllerRecord {
        handle: controller,
        movie: PPC_QT_MOVIE,
        rect: (0, 0, 100, 100),
        flags: 0,
        window: 0,
        origin: 0,
        action_filter: 0,
        action_ref_con: 0,
    });
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 8; // mcActionPlay
    loaded.cpu.gpr[5] = 0x0001_0000; // normal forward playback
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.quicktime.movie_started);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = 0; // stop
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(!loaded.quicktime.movie_started);
}

#[test]
fn quicktime_mc_do_action_idle_tasks_associated_movie() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCDoAction");
    let mut loaded = load_pef_application(&pef).unwrap();
    let controller = 0x4001_3010;
    loaded.quicktime.movie_controllers.push(PpcQuickTimeControllerRecord {
        handle: controller,
        movie: PPC_QT_MOVIE,
        rect: (0, 0, 100, 100),
        flags: 0,
        window: 0,
        origin: 0,
        action_filter: 0,
        action_ref_con: 0,
    });
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_tasks_until_done = 10;
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 1; // mcActionIdle
    loaded.cpu.gpr[5] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.movie_task_count, 1);
    assert!(loaded.quicktime.movie_started);
}

#[test]
fn quicktime_new_movie_controller_records_movie_and_bounds() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"NewMovieController");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_DATA_BASE + 0x1a00;
    loaded.memory.add_region(rect_ptr, vec![0, 10, 0, 20, 1, 44, 2, 88]);
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = rect_ptr;
    loaded.cpu.gpr[5] = 1;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    let controller = loaded.cpu.gpr[3];
    assert_ne!(controller, 0);
    assert_eq!(loaded.quicktime.movie_controllers[0].handle, controller);
    assert_eq!(loaded.quicktime.movie_controllers[0].movie, PPC_QT_MOVIE);
    assert_eq!(loaded.quicktime.movie_controllers[0].rect, (10, 20, 300, 600));
    assert_eq!(loaded.quicktime.movie_controllers[0].flags, 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.run_with_hle_imports(64);
    assert_ne!(loaded.cpu.gpr[3], controller);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_controllers.len(), 2);
}

#[test]
fn quicktime_dispose_movie_controller_preserves_associated_movie() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"DisposeMovieController");
    let mut loaded = load_pef_application(&pef).unwrap();
    let controller = 0x4001_3010;
    loaded.quicktime.movie_controllers.push(PpcQuickTimeControllerRecord {
        handle: controller,
        movie: PPC_QT_MOVIE,
        rect: (0, 0, 100, 100),
        flags: 0,
        window: 0,
        origin: 0,
        action_filter: 0,
        action_ref_con: 0,
    });
    loaded.quicktime.movie_started = true;
    loaded.cpu.gpr[3] = controller;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.quicktime.movie_controllers.is_empty());
    assert!(!loaded.quicktime.movie_disposed);
    assert!(loaded.quicktime.movie_started);
}

#[test]
fn quicktime_mc_set_movie_updates_controller_association() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCSetMovie");
    let mut loaded = load_pef_application(&pef).unwrap();
    let controller = PPC_QT_MOVIE + 0x1_0000;
    loaded.quicktime.movie_controllers.push(PpcQuickTimeControllerRecord {
        handle: controller,
        movie: PPC_QT_MOVIE,
        rect: (0, 0, 100, 100),
        flags: 0,
        window: 0,
        origin: 0,
        action_filter: 0,
        action_ref_con: 0,
    });
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = PPC_QT_MOVIE;
    loaded.cpu.gpr[5] = 0x1234;
    loaded.cpu.gpr[6] = 0x000a_0014;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_controllers[0].window, 0x1234);
    assert_eq!(loaded.quicktime.movie_controllers[0].origin, 0x000a_0014);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.quicktime.movie_controllers[0].movie, 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_INVALID_COMPONENT_ID));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 0x1234_5678;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-2010));
    assert_eq!(loaded.quicktime.movie_controllers[0].movie, 0);
}

#[test]
fn quicktime_mc_get_controller_bounds_writes_recorded_rect() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCGetControllerBoundsRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let controller = PPC_QT_MOVIE + 0x1_0000;
    loaded.quicktime.movie_controllers.push(PpcQuickTimeControllerRecord {
        handle: controller,
        movie: PPC_QT_MOVIE,
        rect: (10, 20, 300, 600),
        flags: 0,
        window: 0,
        origin: 0,
        action_filter: 0,
        action_ref_con: 0,
    });
    let out = PPC_DATA_BASE + 0x1b00;
    loaded.memory.add_region(out, vec![0; 8]);
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = out;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(ppc_read_rect(&mut loaded.memory, out), Some((10, 20, 300, 600)));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_INVALID_COMPONENT_ID));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn quicktime_mc_set_action_filter_records_and_removes_callback() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCSetActionFilterWithRefCon");
    let mut loaded = load_pef_application(&pef).unwrap();
    let controller = PPC_QT_MOVIE + 0x1_0000;
    loaded.quicktime.movie_controllers.push(PpcQuickTimeControllerRecord {
        handle: controller,
        movie: PPC_QT_MOVIE,
        rect: (0, 0, 100, 100),
        flags: 0,
        window: 0,
        origin: 0,
        action_filter: 0,
        action_ref_con: 0,
    });
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 0x1234_5678;
    loaded.cpu.gpr[5] = 0x9abc_def0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_controllers[0].action_filter, 0x1234_5678);
    assert_eq!(loaded.quicktime.movie_controllers[0].action_ref_con, 0x9abc_def0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.quicktime.movie_controllers[0].action_filter, 0);
    assert_eq!(loaded.quicktime.movie_controllers[0].action_ref_con, 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_INVALID_COMPONENT_ID));
}

#[test]
fn quicktime_mc_set_controller_bounds_updates_rect() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCSetControllerBoundsRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let controller = PPC_QT_MOVIE + 0x1_0000;
    loaded.quicktime.movie_controllers.push(PpcQuickTimeControllerRecord {
        handle: controller,
        movie: PPC_QT_MOVIE,
        rect: (0, 0, 100, 100),
        flags: 0,
        window: 0,
        origin: 0,
        action_filter: 0,
        action_ref_con: 0,
    });
    let rect_ptr = PPC_DATA_BASE + 0x1c00;
    loaded.memory.add_region(rect_ptr, vec![0, 10, 0, 20, 1, 44, 2, 88]);
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_controllers[0].rect, (10, 20, 300, 600));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.quicktime.movie_controllers[0].rect, (10, 20, 300, 600));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_INVALID_COMPONENT_ID));
}

#[test]
fn quicktime_mc_idle_services_attached_movie() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCIdle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let controller = PPC_QT_MOVIE + 0x1_0000;
    loaded.quicktime.movie_controllers.push(PpcQuickTimeControllerRecord {
        handle: controller,
        movie: PPC_QT_MOVIE,
        rect: (0, 0, 100, 100),
        flags: 0,
        window: 0,
        origin: 0,
        action_filter: 0,
        action_ref_con: 0,
    });
    loaded.quicktime.movie_active = true;
    loaded.cpu.gpr[3] = controller;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_task_count, 1);

    loaded.quicktime.movie_controllers[0].movie = 0;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.quicktime.movie_task_count, 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_INVALID_COMPONENT_ID));
}

#[test]
fn quicktime_mc_get_current_time_returns_value_and_scale() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCGetCurrentTime");
    let mut loaded = load_pef_application(&pef).unwrap();
    let controller = PPC_QT_MOVIE + 0x1_0000;
    loaded.quicktime.movie_controllers.push(PpcQuickTimeControllerRecord {
        handle: controller,
        movie: PPC_QT_MOVIE,
        rect: (0, 0, 100, 100),
        flags: 0,
        window: 0,
        origin: 0,
        action_filter: 0,
        action_ref_con: 0,
    });
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_file_duration = 100;
    loaded.quicktime.movie_task_count = 5;
    let out = PPC_DATA_BASE + 0x1d00;
    loaded.memory.add_region(out, vec![0; 4]);
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = out;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 50);
    assert_eq!(loaded.memory.read_u32_be(out), Some(600));

    loaded.quicktime.movie_task_count = 20;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 100);

    loaded.quicktime.movie_controllers[0].movie = 0;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn quicktime_new_movie_from_data_fork_reads_atom_at_requested_offset() {
    let movie = test_quicktime_tkhd_movie(320, 240);
    let movie_offset = movie.windows(4).position(|bytes| bytes == b"moov").unwrap() - 4;
    let mut fork = vec![0xAA; 13];
    fork[8..12].copy_from_slice(&[0x10, 0x20, 0x30, 0x40]);
    fork.extend_from_slice(&movie[movie_offset..]);

    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"NewMovieFromDataFork");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Data/movie.mhk".to_string(),
        data: fork.clone().into(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        dirty: false,
    });
    loaded.push_test_open_file(PpcFileRecord {
        ref_num: PPC_FIRST_FILE_REF_NUM,
        path: "Data/movie.mhk".to_string(),
        position: 0,
    });
    let movie_out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(movie_out, vec![0; 4]);
    loaded.cpu.gpr[3] = movie_out;
    loaded.cpu.gpr[4] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
    loaded.cpu.gpr[5] = 13;
    loaded.cpu.gpr[6] = 1; // newMovieActive
    loaded.cpu.gpr[7] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(movie_out), Some(PPC_QT_MOVIE));
    assert_eq!(loaded.quicktime.movie_file_path, "Data/movie.mhk");
    let sample_fork = loaded.quicktime.movie_file_sample_fork.as_ref().unwrap();
    assert_eq!(sample_fork.as_slice(), fork.as_slice());
    let samples = loaded.quicktime.movie_video_samples.as_ref().unwrap();
    assert_eq!(
        ppc_qt_movie_video_sample_bytes(sample_fork.as_slice(), samples, 0),
        Some(&[0x10, 0x20, 0x30, 0x40][..])
    );
    assert_eq!(loaded.quicktime.movie_time_scale, 60);
    assert_eq!(loaded.quicktime.movie_tracks.len(), 2);
    assert_eq!(loaded.quicktime.movie_tracks[0].media_type, u32::from_be_bytes(*b"vide"));
    assert!(loaded.quicktime.movie_active);
}

#[test]
fn hle_import_runner_quicktime_open_default_component_routes_by_type() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"OpenADefaultComponent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let instance_out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(instance_out, vec![0; 4]);
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"grip");
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"GIF ");
    loaded.cpu.gpr[5] = instance_out;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(instance_out),
        Some(PPC_QT_GRAPHICS_IMPORTER)
    );
    assert!(loaded.quicktime.graphics_importer_open);
    assert_eq!(
        loaded.quicktime.graphics_importer_subtype,
        u32::from_be_bytes(*b"GIF ")
    );

    // A different component type must not be silently mapped onto the
    // graphics importer.
    loaded.cpu.pc = loaded.entry_pc;
    loaded.quicktime.graphics_importer_open = false;
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"snd ");
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"????");
    loaded.cpu.gpr[5] = instance_out;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_INVALID_COMPONENT_ID));
    assert!(!loaded.quicktime.graphics_importer_open);
}

#[test]
fn hle_import_runner_quicktime_graphics_importer_decodes_gif() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GraphicsImportSetDataHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gif_rgba = [
        255, 0, 0, 255, //
        0, 255, 0, 255, //
        0, 0, 255, 255, //
        255, 255, 255, 255,
    ];
    let gif = test_quicktime_gif_bytes(2, 2, &gif_rgba);
    let handle = PPC_DATA_BASE + 0x1800;
    let data_ptr = PPC_DATA_BASE + 0x2000;
    test_quicktime_push_importer_data_handle(&mut loaded, handle, data_ptr, &gif);
    loaded.quicktime.graphics_importer_open = true;
    loaded.quicktime.graphics_importer_subtype = u32::from_be_bytes(*b"GIF ");
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.graphics_importer_depth, 8);
    assert_eq!(
        loaded.quicktime.graphics_importer_bounds,
        Some((0, 0, 2, 2))
    );
    assert_eq!(loaded.quicktime.graphics_importer_data, gif);
    let image = loaded
        .quicktime
        .graphics_importer_image
        .as_ref()
        .expect("GIF decoded");
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(image.rgba, gif_rgba);

    // GetImageDescription reports the decoded dimensions and depth.
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::QtGraphicsImportGetImageDescription;
    let description_out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(description_out, vec![0; 4]);
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = description_out;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let description_handle = loaded.memory.read_u32_be(description_out).unwrap();
    let description_ptr = loaded.memory.read_u32_be(description_handle).unwrap();
    let mut description = [0u8; 86];
    loaded
        .memory
        .read_bytes_into(description_ptr, &mut description)
        .unwrap();
    assert_eq!(
        u32::from_be_bytes(description[..4].try_into().unwrap()),
        86
    );
    assert_eq!(&description[4..8], b"GIF ");
    assert_eq!(
        i16::from_be_bytes(description[32..34].try_into().unwrap()),
        2
    );
    assert_eq!(
        i16::from_be_bytes(description[34..36].try_into().unwrap()),
        2
    );
    assert_eq!(&description[51..54], b"GIF");
    assert_eq!(
        i16::from_be_bytes(description[82..84].try_into().unwrap()),
        8
    );

    // Draw converts the decoded pixels into the 16-bit destination.
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGraphicsImportDraw;
    let gworld = 0x0600_1000;
    let base = PPC_HEAP_BASE + 0x6000;
    loaded.memory.add_region(base, vec![0; 2 * 2 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: base,
        gdevice: 0x0600_2000,
        width: 2,
        height: 2,
        depth: 16,
        row_bytes: 4,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded.quicktime.graphics_importer_gworld = gworld;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.graphics_import_source_draw_count, 1);
    let pixels: Vec<u16> = (0..4)
        .map(|index| loaded.memory.read_u16_be(base + index * 2).unwrap())
        .collect();
    assert_eq!(
        pixels,
        vec![
            ppc_qt_rgb555_from_u8(255, 0, 0),
            ppc_qt_rgb555_from_u8(0, 255, 0),
            ppc_qt_rgb555_from_u8(0, 0, 255),
            ppc_qt_rgb555_from_u8(255, 255, 255),
        ]
    );
}

#[test]
fn hle_import_runner_quicktime_graphics_importer_decodes_16_bit_tga_uncompressed() {
    // Real Interface.pak shape: image type 2, pixel depth 16, image_desc 0x01
    // (one attribute bit, bottom-left origin). File order is the bottom row
    // first, so the decoder must flip the rows.
    let blue = test_quicktime_tga_16_color(0, 0, 31);
    let white = test_quicktime_tga_16_color(31, 31, 31);
    let red = test_quicktime_tga_16_color(31, 0, 0);
    let green = test_quicktime_tga_16_color(0, 31, 0);
    let tga = test_quicktime_tga_16_bytes(2, 0x01, &[blue, white, red, green]);

    let image = test_quicktime_importer_decode_image_bytes(b"TGA ", &tga, PPC_NO_ERR)
        .expect("16-bit TGA decoded");
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(
        image.rgba,
        [
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
            0, 0, 255, 255, // blue
            255, 255, 255, 255, // white
        ]
    );
}

#[test]
fn hle_import_runner_quicktime_graphics_importer_decodes_16_bit_tga_rle() {
    // One RLE run packet (blue x2) followed by one raw packet (red, green),
    // in bottom-left file order.
    let blue = test_quicktime_tga_16_color(0, 0, 31);
    let red = test_quicktime_tga_16_color(31, 0, 0);
    let green = test_quicktime_tga_16_color(0, 31, 0);
    let mut tga = test_quicktime_tga_16_bytes(10, 0x01, &[blue, blue, red, green]);
    // Rewrite the payload as: run packet 0x81 + blue, raw packet 0x01 + red + green.
    tga.truncate(18);
    tga.push(0x81);
    tga.extend_from_slice(&blue.to_le_bytes());
    tga.push(0x01);
    tga.extend_from_slice(&red.to_le_bytes());
    tga.extend_from_slice(&green.to_le_bytes());

    let image = test_quicktime_importer_decode_image_bytes(b"TGA ", &tga, PPC_NO_ERR)
        .expect("RLE 16-bit TGA decoded");
    assert_eq!(image.rgba[..8], [255, 0, 0, 255, 0, 255, 0, 255]);
    assert_eq!(
        image.rgba[8..],
        [0, 0, 255, 255, 0, 0, 255, 255]
    );
}

#[test]
fn hle_import_runner_quicktime_graphics_importer_16_bit_tga_honors_origin_bit() {
    // Top-left origin: image_desc bit 5 set, so the first file row is the top
    // row and the decoded bytes keep file order.
    let red = test_quicktime_tga_16_color(31, 0, 0);
    let green = test_quicktime_tga_16_color(0, 31, 0);
    let blue = test_quicktime_tga_16_color(0, 0, 31);
    let white = test_quicktime_tga_16_color(31, 31, 31);
    let tga = test_quicktime_tga_16_bytes(2, 0x20, &[red, green, blue, white]);

    let image = test_quicktime_importer_decode_image_bytes(b"TGA ", &tga, PPC_NO_ERR)
        .expect("top-left TGA decoded");
    assert_eq!(
        image.rgba,
        [
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
            0, 0, 255, 255, // blue
            255, 255, 255, 255, // white
        ]
    );
}

#[test]
fn hle_import_runner_quicktime_graphics_importer_rejects_truncated_16_bit_tga() {
    let blue = test_quicktime_tga_16_color(0, 0, 31);
    let mut tga = test_quicktime_tga_16_bytes(2, 0x01, &[blue, blue, blue, blue]);
    tga.truncate(20);

    let image = test_quicktime_importer_decode_image_bytes(b"TGA ", &tga, PPC_PARAM_ERR);
    assert!(image.is_none());
}

#[test]
fn hle_import_runner_quicktime_graphics_importer_decodes_tga() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GraphicsImportSetDataHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let tga_rgba = [
        10, 20, 30, 255, //
        40, 50, 60, 255, //
        70, 80, 90, 255, //
        100, 110, 120, 255,
    ];
    let tga = test_quicktime_tga_bytes(2, 2, &tga_rgba);
    let handle = PPC_DATA_BASE + 0x1800;
    let data_ptr = PPC_DATA_BASE + 0x2000;
    test_quicktime_push_importer_data_handle(&mut loaded, handle, data_ptr, &tga);
    loaded.quicktime.graphics_importer_open = true;
    loaded.quicktime.graphics_importer_subtype = u32::from_be_bytes(*b"TGA ");
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.quicktime.graphics_importer_depth,
        test_quicktime_tga_header_depth(&tga)
    );
    assert_eq!(
        loaded.quicktime.graphics_importer_bounds,
        Some((0, 0, 2, 2))
    );
    let image = loaded
        .quicktime
        .graphics_importer_image
        .as_ref()
        .expect("TGA decoded");
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(image.rgba, tga_rgba);
}

#[test]
fn hle_import_runner_quicktime_graphics_importer_rejects_undecodable_data() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GraphicsImportSetDataHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = PPC_DATA_BASE + 0x1800;
    let data_ptr = PPC_DATA_BASE + 0x2000;
    test_quicktime_push_importer_data_handle(&mut loaded, handle, data_ptr, b"not a gif");
    loaded.quicktime.graphics_importer_open = true;
    loaded.quicktime.graphics_importer_subtype = u32::from_be_bytes(*b"GIF ");
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert!(loaded.quicktime.graphics_importer_image.is_none());
    assert_eq!(loaded.quicktime.graphics_importer_bounds, None);
}

#[test]
fn hle_import_runner_quicktime_graphics_importer_maps_gif_through_destination_clut() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GraphicsImportSetDataHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gif_rgba = [
        255, 0, 0, 255, //
        0, 255, 0, 255, //
        0, 0, 255, 255, //
        255, 255, 255, 255,
    ];
    let gif = test_quicktime_gif_bytes(2, 2, &gif_rgba);
    let handle = PPC_DATA_BASE + 0x1800;
    let data_ptr = PPC_DATA_BASE + 0x2000;
    test_quicktime_push_importer_data_handle(&mut loaded, handle, data_ptr, &gif);
    loaded.quicktime.graphics_importer_open = true;
    loaded.quicktime.graphics_importer_subtype = u32::from_be_bytes(*b"GIF ");
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = handle;

    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGraphicsImportDraw;
    let gworld = 0x0600_1000;
    let base = PPC_HEAP_BASE + 0x6000;
    loaded.memory.add_region(base, vec![0; 2 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: base,
        gdevice: 0x0600_2000,
        width: 2,
        height: 2,
        depth: 8,
        row_bytes: 2,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded.quicktime.graphics_importer_gworld = gworld;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;

    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    let surface = PpcQuickDrawSurface {
        front_buffer: PpcFrontBuffer {
            base_addr: base,
            row_bytes: 2,
            width: 2,
            height: 2,
            depth: 8,
        },
        top: 0,
        left: 0,
        ctable_handle: None,
    };
    let colors = [
        PpcRgbColor {
            red: 0xffff,
            green: 0,
            blue: 0,
        },
        PpcRgbColor {
            red: 0,
            green: 0xffff,
            blue: 0,
        },
        PpcRgbColor {
            red: 0,
            green: 0,
            blue: 0xffff,
        },
        PPC_RGB_WHITE,
    ];
    let expected: Vec<u8> = colors
        .iter()
        .map(|color| {
            ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, *color).unwrap() as u8
        })
        .collect();
    let drawn: Vec<u8> = (0..4)
        .map(|index| loaded.memory.read_u8(base + index).unwrap())
        .collect();
    assert_eq!(drawn, expected);
    // The three sprite colours must remain distinguishable from the cleared
    // background index once mapped through the destination ColorTable.
    assert_ne!(expected[0], 0);
    assert_ne!(expected[1], 0);
    assert_ne!(expected[2], 0);
    assert_ne!(expected[0], expected[1]);
    assert_ne!(expected[1], expected[2]);
}

#[test]
fn hle_import_runner_quicktime_graphics_importer_draws_gif_into_32_bit_gworld() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"GraphicsImportSetDataHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gif_rgba = [
        255, 0, 0, 255, //
        0, 255, 0, 255, //
        0, 0, 255, 255, //
        255, 255, 255, 255,
    ];
    let gif = test_quicktime_gif_bytes(2, 2, &gif_rgba);
    let handle = PPC_DATA_BASE + 0x1800;
    let data_ptr = PPC_DATA_BASE + 0x2000;
    test_quicktime_push_importer_data_handle(&mut loaded, handle, data_ptr, &gif);
    loaded.quicktime.graphics_importer_open = true;
    loaded.quicktime.graphics_importer_subtype = u32::from_be_bytes(*b"GIF ");
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;
    loaded.cpu.gpr[4] = handle;

    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtGraphicsImportDraw;
    let gworld = 0x0600_1000;
    let base = PPC_HEAP_BASE + 0x6000;
    loaded.memory.add_region(base, vec![0; 2 * 2 * 4]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: base,
        gdevice: 0x0600_2000,
        width: 2,
        height: 2,
        depth: 32,
        row_bytes: 8,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded.quicktime.graphics_importer_gworld = gworld;
    loaded.cpu.gpr[3] = PPC_QT_GRAPHICS_IMPORTER;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.quicktime.graphics_import_source_draw_count, 1);
    let pixels: Vec<u32> = (0..4)
        .map(|index| loaded.memory.read_u32_be(base + index * 4).unwrap())
        .collect();
    assert_eq!(
        pixels,
        vec![0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0x00ff_ffff]
    );
}
