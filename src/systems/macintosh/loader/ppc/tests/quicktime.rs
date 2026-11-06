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

fn cinepak_movie_draw_fixture() -> (PpcLoadedApp, u32, u32) {
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
        movie_active: true,
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

    (loaded, gworld, base)
}

#[test]
fn quicktime_movie_frame_draw_uses_cinepak_sample_pixels() {
    let (mut loaded, gworld, base) = cinepak_movie_draw_fixture();

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
    // The same decoded frame must be usable on direct and indexed GWorlds.
    for (depth, row_bytes, expected_last) in [
        (1, 1, 0x00u32),
        (2, 1, 0x00),
        (4, 2, 0x00),
        (8, 4, 0x00),
        (32, 16, 0x00ff_ffff),
    ] {
        loaded.gworlds[0].depth = depth;
        loaded.gworlds[0].row_bytes = row_bytes;
        let destination = base + 0x1000 + depth * 0x100;
        loaded.gworlds[0].base_addr = destination;
        loaded
            .memory
            .add_region(destination, vec![0xee; row_bytes as usize * 4]);
        assert!(ppc_qt_draw_movie_frame(
            &mut loaded.memory,
            &loaded.gworlds,
            gworld,
            &mut loaded.quicktime,
            0x21
        ));
        let last_row = destination + row_bytes * 3;
        let last_pixel = match depth {
            1 => u32::from(loaded.memory.read_u8(last_row).unwrap() >> 4 & 1),
            2 => u32::from(loaded.memory.read_u8(last_row).unwrap() & 3),
            4 => u32::from(loaded.memory.read_u8(last_row + 1).unwrap() & 15),
            8 => u32::from(loaded.memory.read_u8(last_row + 3).unwrap()),
            32 => loaded.memory.read_u32_be(last_row + 12).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(last_pixel, expected_last, "depth {depth}");
        if depth == 32 {
            assert_eq!(loaded.memory.read_u32_be(destination), Some(0));
            assert_eq!(
                loaded.memory.read_u32_be(destination + 8),
                Some(0x0055_5555)
            );
            assert_eq!(
                loaded.memory.read_u32_be(destination + 32),
                Some(0x00aa_aaaa)
            );
        }
    }
    // Resolve a live port PixMap, including a nonzero coordinate origin.
    let pixmap = base + 0x8000;
    let pixmap_handle = pixmap + 0x100;
    let destination = pixmap + 0x200;
    loaded.memory.add_region(gworld, vec![0; 64]);
    loaded.memory.add_region(pixmap, vec![0; 0x300]);
    loaded.memory.write_u16_be(gworld + 6, 0xc000).unwrap();
    loaded
        .memory
        .write_u32_be(gworld + 2, pixmap_handle)
        .unwrap();
    loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
    loaded.memory.write_u32_be(pixmap, destination).unwrap();
    loaded.memory.write_u16_be(pixmap + 4, 0x8010).unwrap();
    loaded.memory.write_u16_be(pixmap + 32, 32).unwrap();
    ppc_write_rect(&mut loaded.memory, pixmap + 6, 10, 20, 14, 24).unwrap();
    loaded.quicktime.movie_box = (10, 20, 14, 24);
    assert!(ppc_qt_draw_movie_frame(
        &mut loaded.memory,
        &loaded.gworlds,
        gworld,
        &mut loaded.quicktime,
        0x21
    ));
    assert_eq!(
        loaded.memory.read_u32_be(destination + 8),
        Some(0x0055_5555)
    );
    assert_eq!(
        loaded.memory.read_u32_be(destination + 60),
        Some(0x00ff_ffff)
    );

    // A custom indexed table maps white to 7 rather than the standard 0.
    let table_handle = pixmap + 0x400;
    let table = table_handle + 4;
    loaded.memory.add_region(table_handle, vec![0; 12 + 8 * 8]);
    loaded.memory.write_u32_be(table_handle, table).unwrap();
    loaded.memory.write_u16_be(table + 6, 7).unwrap();
    for index in 0..8u32 {
        let entry = table + 8 + index * 8;
        loaded.memory.write_u16_be(entry, index as u16).unwrap();
        for component in [2, 4, 6] {
            loaded
                .memory
                .write_u16_be(entry + component, if index == 7 { 0xffff } else { 0 })
                .unwrap();
        }
    }
    loaded
        .memory
        .write_u32_be(pixmap + 42, table_handle)
        .unwrap();
    loaded.memory.write_u16_be(pixmap + 32, 8).unwrap();
    loaded.memory.write_u16_be(pixmap + 4, 0x8004).unwrap();
    assert!(ppc_qt_draw_movie_frame(
        &mut loaded.memory,
        &loaded.gworlds,
        gworld,
        &mut loaded.quicktime,
        0x21
    ));
    assert_eq!(loaded.memory.read_u8(destination + 15), Some(7));
    // Rotate the four known Cinepak quadrants clockwise about the movie box.
    loaded.memory.write_u16_be(pixmap + 32, 32).unwrap();
    loaded.memory.write_u16_be(pixmap + 4, 0x8010).unwrap();
    loaded.quicktime.movie_file_bounds = Some((0, 0, 4, 4));
    loaded.quicktime.movie_matrix = Some([
        0,
        0x0001_0000,
        0,
        0xffff_0000,
        0,
        0,
        24 << 16,
        10 << 16,
        0x4000_0000,
    ]);
    assert!(ppc_qt_draw_movie_frame(
        &mut loaded.memory,
        &loaded.gworlds,
        gworld,
        &mut loaded.quicktime,
        0x21
    ));
    assert_eq!(loaded.memory.read_u32_be(destination), Some(0x00aa_aaaa));
    assert_eq!(loaded.memory.read_u32_be(destination + 12), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(destination + 48),
        Some(0x00ff_ffff)
    );
    assert_eq!(
        loaded.memory.read_u32_be(destination + 60),
        Some(0x0055_5555)
    );

    // A valid decoded frame with no visible pixels must not paint a fallback.
    loaded.memory.write_u16_be(pixmap + 32, 16).unwrap();
    loaded.memory.write_u16_be(pixmap + 4, 0x8008).unwrap();
    loaded.quicktime.movie_display_clip = Some(vec![0, 10, 0, 0, 0, 0, 0, 0, 0, 0]);
    for index in 0..16 {
        loaded
            .memory
            .write_u16_be(destination + index * 2, 0x1234)
            .unwrap();
    }
    assert!(!ppc_qt_draw_movie_frame(
        &mut loaded.memory,
        &loaded.gworlds,
        gworld,
        &mut loaded.quicktime,
        0x21
    ));
    for index in 0..16 {
        assert_eq!(
            loaded.memory.read_u16_be(destination + index * 2),
            Some(0x1234)
        );
    }
}

#[test]
fn quicktime_attached_controller_bounds_position_movie_pixels() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"StartMovie");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gworld = 0x0600_3000;
    let base = PPC_HEAP_BASE + 0x8000;
    let sample_offset = 12u64;
    let sample = test_cinepak_v1_frame_sample();
    let mut movie_file_data = vec![0; sample_offset as usize];
    movie_file_data.extend_from_slice(&sample);
    loaded.memory.add_region(base, vec![0xee; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: base,
        gdevice: 0x0600_4000,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded.quicktime = PpcQuickTimeState {
        movie_file_data,
        movie_box: (0, 0, 4, 4),
        movie_video_samples: Some(PpcQuickTimeVideoSampleTableRecord {
            media_time_scale: 1,
            media_duration: 1,
            sample_count: 1,
            codec: u32::from_be_bytes(*b"cvid"),
            samples: vec![PpcQuickTimeVideoSampleRecord {
                offset: sample_offset,
                size: sample.len() as u32,
                media_start_time: 0,
                duration: 1,
                data_len: sample.len() as u32,
                checksum: sample.iter().copied().map(u32::from).sum(),
                preview_len: 0,
                preview: [0; 16],
            }],
        }),
        movie_controllers: vec![PpcQuickTimeControllerRecord {
            handle: 0x4000_0010,
            movie: PPC_QT_MOVIE,
            rect: (2, 2, 6, 6),
            flags: 0,
            window: gworld,
            origin: 0,
            action_filter: 0,
            action_ref_con: 0,
            action_filter_rtoc: 0,
        }],
        ..PpcQuickTimeState::default()
    };

    assert!(ppc_qt_draw_movie_frame(
        &mut loaded.memory,
        &loaded.gworlds,
        gworld,
        &mut loaded.quicktime,
        0x21,
    ));
    assert_eq!(loaded.memory.read_u16_be(base), Some(0xeeee));
    assert_eq!(loaded.memory.read_u16_be(base + 2 * 16 + 2 * 2), Some(0));
    assert_eq!(loaded.memory.read_u16_be(base + 5 * 16 + 5 * 2), Some(0x7fff));
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
    advance_quicktime_test_clock(&mut loaded, 20_000_000);
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
        advance_quicktime_test_clock(&mut loaded, 20_000_000);
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
    for library in [
        b"QuickTimeLib".as_slice(),
        b"Apple;Carbon;Multimedia".as_slice(),
    ] {
        let pef = synthetic_pef_with_library_import(library, b"MCDoAction");
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
}

#[test]
fn quicktime_mc_do_action_play_starts_and_stops_controller_movie() {
    for library in [
        b"QuickTimeLib".as_slice(),
        b"Apple;Carbon;Multimedia".as_slice(),
    ] {
        let pef = synthetic_pef_with_library_import(library, b"MCDoAction");
        let mut loaded = load_pef_application(&pef).unwrap();
        let controller = 0x4001_3010;
        loaded
            .quicktime
            .movie_controllers
            .push(PpcQuickTimeControllerRecord {
                handle: controller,
                movie: PPC_QT_MOVIE,
                rect: (0, 0, 100, 100),
                flags: 0,
                window: 0,
                origin: 0,
                action_filter: 0,
                action_ref_con: 0,
                action_filter_rtoc: 0,
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
}

#[test]
fn quicktime_mc_do_action_idle_tasks_associated_movie() {
    for library in [
        b"QuickTimeLib".as_slice(),
        b"Apple;Carbon;Multimedia".as_slice(),
    ] {
        let pef = synthetic_pef_with_library_import(library, b"MCDoAction");
        let mut loaded = load_pef_application(&pef).unwrap();
        let controller = 0x4001_3010;
        loaded
            .quicktime
            .movie_controllers
            .push(PpcQuickTimeControllerRecord {
                handle: controller,
                movie: PPC_QT_MOVIE,
                rect: (0, 0, 100, 100),
                flags: 0,
                window: 0,
                origin: 0,
                action_filter: 0,
                action_ref_con: 0,
                action_filter_rtoc: 0,
            });
        loaded.quicktime.movie_active = true;
        loaded.quicktime.movie_started = true;
        loaded.quicktime.movie_tasks_until_done = 10;
        loaded.cpu.gpr[3] = controller;
        loaded.cpu.gpr[4] = 1; // mcActionIdle
        loaded.cpu.gpr[5] = 0;

        advance_quicktime_test_clock(&mut loaded, 6_000_000);
        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.quicktime.movie_task_count, 1);
        assert!(loaded.quicktime.movie_started);
    }
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
            action_filter_rtoc: 0,
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
            action_filter_rtoc: 0,
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
            action_filter_rtoc: 0,
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
            action_filter_rtoc: 0,
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
            action_filter_rtoc: 0,
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
            action_filter_rtoc: 0,
    });
    loaded.quicktime.movie_active = true;
    loaded.cpu.gpr[3] = controller;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.quicktime.movie_task_count, 0);

    loaded.quicktime.movie_started = true;
    advance_quicktime_test_clock(&mut loaded, 20_000_000);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = controller;
    loaded.run_with_hle_imports(64);
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
            action_filter_rtoc: 0,
    });
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_file_duration = 100;
    loaded.quicktime.movie_tasks_until_done = 10;
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

#[test]
fn quicktime_carbon_movie_rewind_uses_existing_movie_state_and_void_return() {
    for library in [
        b"QuickTimeLib".as_slice(),
        b"Apple;Carbon;Multimedia".as_slice(),
    ] {
        let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
            library,
            b"GoToBeginningOfMovie",
        ))
        .unwrap();
        loaded.quicktime.movie_started = true;
        loaded.quicktime.movie_at_beginning = false;
        loaded.quicktime.movie_task_count = 7;
        loaded.cpu.gpr[3] = PPC_QT_MOVIE;
        loaded.cpu.lr = PPC_HALT_PC;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert!(matches!(
            probe.result,
            PpcRunResult::Halted {
                pc: PPC_HALT_PC,
                ..
            }
        ));
        assert_eq!(loaded.cpu.gpr[3], PPC_QT_MOVIE);
        assert_eq!(loaded.quicktime.movie_task_count, 0);
        assert_eq!(loaded.quicktime.movie_beginning_count, 1);
        assert!(loaded.quicktime.movie_at_beginning);
        assert!(!loaded.quicktime.movie_started);
    }
}

#[test]
fn quicktime_callback_lifecycle_delivers_guest_arguments_and_honours_cancel_dispose() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"Apple;Carbon;Multimedia", b"NewCallBack",
    )).unwrap();
    let operation = |op| PpcImportDispatcherTarget::QuickTimeCompatibility(op);
    use PpcQuickTimeCompatibilityOperation as Op;
    let code = PPC_CODE_BASE + 0x4000;
    let vector = PPC_DATA_BASE + 0x1000;
    let observed = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(code, [0x90620000u32, 0x90820004, 0x4e800020]
        .into_iter().flat_map(u32::to_be_bytes).collect());
    loaded.memory.add_region(vector, [code, observed].into_iter().flat_map(u32::to_be_bytes).collect());
    loaded.memory.add_region(observed, vec![0; 8]);
    loaded.cpu.gpr[3] = PPC_QT_MOVIE + 0x10;
    loaded.cpu.gpr[4] = 1;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::NewCallBack));
    let callback = loaded.cpu.gpr[3];
    assert_ne!(callback, 0);
    assert!(loaded.quicktime.callbacks.get(callback).is_some());
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_file_duration = 600;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_tasks_until_done = 6;
    let arm = |loaded: &mut PpcLoadedApp| {
        loaded.cpu.gpr[3] = callback;
        loaded.cpu.gpr[4] = vector;
        loaded.cpu.gpr[5] = 0xdeadbeef;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 50;
        loaded.cpu.gpr[8] = 600;
        run_quicktime_callback_test_import(loaded, operation(Op::CallMeWhen));
        assert_eq!(loaded.cpu.gpr[3], 0);
    };
    arm(&mut loaded);
    loaded.cpu.gpr[3] = callback;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::CancelCallBack));
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    advance_quicktime_test_clock(&mut loaded, 10_000_000);
    run_quicktime_callback_test_import(&mut loaded, PpcImportDispatcherTarget::QtMoviesTask);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(0));
    loaded.quicktime.movie_task_count = 0;
    arm(&mut loaded);
    let original_rtoc = loaded.cpu.gpr[2];
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    advance_quicktime_test_clock(&mut loaded, 10_000_000);
    run_quicktime_callback_test_import(&mut loaded, PpcImportDispatcherTarget::QtMoviesTask);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(callback));
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(0xdeadbeef));
    assert_eq!(loaded.cpu.gpr[2], original_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.memory.write_u32_be(observed, 0).unwrap();
    loaded.quicktime.movie_task_count = 0;
    arm(&mut loaded);
    loaded.cpu.gpr[3] = callback;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::DisposeCallBack));
    assert!(loaded.quicktime.callbacks.get(callback).is_none());
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    advance_quicktime_test_clock(&mut loaded, 10_000_000);
    run_quicktime_callback_test_import(&mut loaded, PpcImportDispatcherTarget::QtMoviesTask);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(0));
}

#[test]
fn disposing_standalone_time_base_preserves_queued_movie_callback() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let base = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    let registration = quicktime::callbacks::Registration {
        procedure: 0x1000,
        rtoc: 0x2000,
        refcon: 0x3000,
        parameters: [0; 3],
    };
    loaded.quicktime.callbacks.queue(PPC_QT_MOVIE, registration);
    call_movie_toolbox(&mut loaded, "DisposeTimeBase", &[base]);
    assert_eq!(
        loaded.quicktime.callbacks.take_ready(),
        Some((PPC_QT_MOVIE, registration))
    );
    assert!(!loaded.quicktime.time_bases.records.contains_key(&base));
}

#[test]
fn standalone_time_base_callbacks_fire_when_nonlooping_bounds_are_reached() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let base = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    let bound = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(bound, 0).unwrap();
    loaded.memory.write_u32_be(bound + 8, 1).unwrap();
    loaded.memory.write_u32_be(bound + 12, 0).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
    loaded.memory.write_u64_be(bound, 1).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[base, bound]);
    let registration = quicktime::callbacks::Registration {
        procedure: 0x1000,
        rtoc: 0,
        refcon: 0,
        parameters: [2, 0, 0],
    };
    assert!(loaded.quicktime.callbacks.create(0x3000, base, 4));
    assert!(loaded.quicktime.callbacks.arm(0x3000, registration));
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
    advance_quicktime_test_clock(&mut loaded, 60_000_000);
    let now = loaded.callback_scheduling.current_subtick();
    quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
    assert_eq!(
        loaded.quicktime.callbacks.take_ready(),
        Some((0x3000, registration))
    );
    assert!(loaded.quicktime.callbacks.arm(0x3000, registration));
    advance_quicktime_test_clock(&mut loaded, 60_000_000);
    let now = loaded.callback_scheduling.current_subtick();
    quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
    assert_eq!(loaded.quicktime.callbacks.take_ready(), None);
    call_movie_toolbox(&mut loaded, "SetTimeBaseValue", &[base, 1, 1]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0xffff0000]);
    let reverse = quicktime::callbacks::Registration {
        parameters: [1, 0, 0],
        ..registration
    };
    assert!(loaded.quicktime.callbacks.arm(0x3000, reverse));
    advance_quicktime_test_clock(&mut loaded, 60_000_000);
    let now = loaded.callback_scheduling.current_subtick();
    quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
    assert_eq!(
        loaded.quicktime.callbacks.take_ready(),
        Some((0x3000, reverse))
    );
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[base, 3]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseValue", &[base, 0, 1]);
    assert!(loaded.quicktime.callbacks.arm(0x3000, registration));
    advance_quicktime_test_clock(&mut loaded, 60_000_000);
    let now = loaded.callback_scheduling.current_subtick();
    quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
    assert_eq!(
        loaded.quicktime.callbacks.take_ready(),
        Some((0x3000, registration))
    );
    assert!(loaded.quicktime.callbacks.arm(0x3000, reverse));
    advance_quicktime_test_clock(&mut loaded, 60_000_000);
    let now = loaded.callback_scheduling.current_subtick();
    quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
    assert_eq!(
        loaded.quicktime.callbacks.take_ready(),
        Some((0x3000, reverse))
    );
}

#[test]
fn reentrant_time_base_disposal_does_not_free_pending_base() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let base = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    loaded
        .quicktime
        .callback_frames
        .push(quicktime::callbacks::ContinuationFrame::Delivery(
            quicktime::callbacks::DeliveryFrame {
                import_pc: 0x1234,
                final_pc: 0x5678,
                restore_rtoc: 0,
                result: base,
                dispose_movie: false,
                dispose_time_base: Some(base),
            },
        ));
    call_movie_toolbox(&mut loaded, "DisposeTimeBase", &[base]);
    assert!(loaded.quicktime.time_bases.records.contains_key(&base));
    loaded.quicktime.callback_frames.pop();
    call_movie_toolbox(&mut loaded, "DisposeTimeBase", &[base]);
    assert!(!loaded.quicktime.time_bases.records.contains_key(&base));
}

#[test]
fn disposal_callback_can_reenter_disposal_without_freeing_base_early() {
    use PpcQuickTimeCompatibilityOperation as Op;
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let op = |operation| PpcImportDispatcherTarget::QuickTimeCompatibility(operation);
    let code = PPC_CODE_BASE + 0x4000;
    let vector = PPC_DATA_BASE + 0x1000;
    let data = PPC_DATA_BASE + 0x1200;
    let d = |op: u32, reg: u32, base: u32, imm: i16| {
        (op << 26) | (reg << 21) | (base << 16) | u32::from(imm as u16)
    };
    loaded.memory.add_region(
        code,
        [
            d(37, 1, 1, -64),
            xfx_form(31, 0, 8, 339),
            d(36, 0, 1, 68),
            d(36, 31, 1, 60),
            x_form(31, 4, 31, 4, 444, false),
            d(32, 3, 31, 0),
            d(32, 12, 31, 4),
            xfx_form(31, 12, 9, 467),
            xl_form(19, 20, 0, 528, true),
            d(32, 5, 31, 0),
            d(32, 5, 5, 0),
            d(36, 5, 31, 8),
            d(32, 31, 1, 60),
            d(32, 0, 1, 68),
            xfx_form(31, 0, 8, 467),
            d(14, 1, 1, 64),
            0x4e800020,
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    loaded.memory.add_region(
        vector,
        [code, 0].into_iter().flat_map(u32::to_be_bytes).collect(),
    );
    loaded.memory.add_region(data, vec![0; 12]);
    run_quicktime_callback_test_import(&mut loaded, op(Op::NewTimeBase));
    let base = loaded.cpu.gpr[3];
    loaded.memory.write_u32_be(base, 0x12345678).unwrap();
    loaded.memory.write_u32_be(data, base).unwrap();
    loaded
        .memory
        .write_u32_be(data + 4, loaded.imports[0].trap_pc)
        .unwrap();
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = 5;
    run_quicktime_callback_test_import(&mut loaded, op(Op::NewCallBack));
    loaded.cpu.gpr[4] = vector;
    loaded.cpu.gpr[5] = data;
    loaded.cpu.gpr[6..9].fill(0);
    run_quicktime_callback_test_import(&mut loaded, op(Op::CallMeWhen));
    loaded.cpu.gpr[3] = base;
    run_quicktime_callback_test_import(&mut loaded, op(Op::DisposeTimeBase));
    assert_eq!(loaded.memory.read_u32_be(data + 8), Some(0x12345678));
    assert!(!loaded.quicktime.time_bases.records.contains_key(&base));
}

#[test]
fn standalone_time_base_disposal_delivers_callback_before_freeing_base() {
    use PpcQuickTimeCompatibilityOperation as Op;
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let op = |operation| PpcImportDispatcherTarget::QuickTimeCompatibility(operation);
    let code = PPC_CODE_BASE + 0x4000;
    let vector = PPC_DATA_BASE + 0x1000;
    let observed = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(
        code,
        [
            0x90620000u32,
            0x90820004,
            0x80a20008,
            0x80a50000,
            0x90a20008,
            0x4e800020,
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    loaded.memory.add_region(
        vector,
        [code, observed, code, observed + 12]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    loaded.memory.add_region(observed, vec![0; 24]);
    run_quicktime_callback_test_import(&mut loaded, op(Op::NewTimeBase));
    let base = loaded.cpu.gpr[3];
    loaded.memory.write_u32_be(base, 0x12345678).unwrap();
    loaded.memory.write_u32_be(observed + 8, base).unwrap();
    loaded.memory.write_u32_be(observed + 20, base).unwrap();
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = 5;
    run_quicktime_callback_test_import(&mut loaded, op(Op::NewCallBack));
    let callback = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = callback;
    loaded.cpu.gpr[4] = vector;
    loaded.cpu.gpr[5] = 0xdeadbeef;
    loaded.cpu.gpr[6..9].fill(0);
    run_quicktime_callback_test_import(&mut loaded, op(Op::CallMeWhen));
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = 5;
    run_quicktime_callback_test_import(&mut loaded, op(Op::NewCallBack));
    let second_callback = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = second_callback;
    loaded.cpu.gpr[4] = vector + 8;
    loaded.cpu.gpr[5] = 0xfeedface;
    loaded.cpu.gpr[6..9].fill(0);
    run_quicktime_callback_test_import(&mut loaded, op(Op::CallMeWhen));
    loaded.cpu.gpr[3] = base;
    run_quicktime_callback_test_import(&mut loaded, op(Op::DisposeTimeBase));
    assert_eq!(loaded.cpu.gpr[3], base);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(callback));
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(0xdeadbeef));
    assert_eq!(loaded.memory.read_u32_be(observed + 8), Some(0x12345678));
    assert_eq!(
        loaded.memory.read_u32_be(observed + 12),
        Some(second_callback)
    );
    assert_eq!(loaded.memory.read_u32_be(observed + 16), Some(0xfeedface));
    assert_eq!(loaded.memory.read_u32_be(observed + 20), Some(0x12345678));
    assert!(!loaded.quicktime.time_bases.records.contains_key(&base));
}

#[test]
fn standalone_time_base_delivers_due_callback_and_preserves_query_result() {
    use PpcQuickTimeCompatibilityOperation as Op;
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let operation = |op| PpcImportDispatcherTarget::QuickTimeCompatibility(op);
    let code = PPC_CODE_BASE + 0x4000;
    let vector = PPC_DATA_BASE + 0x1000;
    let observed = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(
        code,
        [0x90620000u32, 0x90820004, 0x4e800020]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    loaded.memory.add_region(
        vector,
        [code, observed]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    loaded.memory.add_region(observed, vec![0; 8]);
    run_quicktime_callback_test_import(&mut loaded, operation(Op::NewTimeBase));
    let base = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = 1;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::NewCallBack));
    let callback = loaded.cpu.gpr[3];
    assert_ne!(callback, 0);
    loaded.cpu.gpr[3] = callback;
    loaded.cpu.gpr[4] = vector;
    loaded.cpu.gpr[5] = 0xdeadbeef;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 300;
    loaded.cpu.gpr[8] = 600;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::CallMeWhen));
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = 0x10000;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::SetTimeBaseRate));
    advance_quicktime_test_clock(&mut loaded, 60_000_000);
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = 600;
    loaded.cpu.gpr[5] = 0;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::GetTimeBaseTime));
    assert_eq!(loaded.cpu.gpr[3], 600);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(callback));
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(0xdeadbeef));

    let bound = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(bound, 0).unwrap();
    loaded.memory.write_u32_be(bound + 8, 1).unwrap();
    loaded.memory.write_u32_be(bound + 12, 0).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
    loaded.memory.write_u64_be(bound, 1).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[base, bound]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[base, 1]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseValue", &[base, 0, 1]);
    loaded.cpu.gpr[3] = callback;
    loaded.cpu.gpr[4] = vector;
    loaded.cpu.gpr[5] = 0x12345678;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 300;
    loaded.cpu.gpr[8] = 600;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::CallMeWhen));
    loaded.memory.write_u32_be(observed, 0).unwrap();
    advance_quicktime_test_clock(&mut loaded, 120_000_000);
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = 600;
    loaded.cpu.gpr[5] = 0;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::GetTimeBaseTime));
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(callback));
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(0x12345678));

    call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[base, 3]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseValue", &[base, 0, 1]);
    loaded.cpu.gpr[3] = callback;
    loaded.cpu.gpr[4] = vector;
    loaded.cpu.gpr[5] = 0x87654321;
    loaded.cpu.gpr[6] = 2;
    loaded.cpu.gpr[7] = 300;
    loaded.cpu.gpr[8] = 600;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::CallMeWhen));
    loaded.memory.write_u32_be(observed, 0).unwrap();
    advance_quicktime_test_clock(&mut loaded, 120_000_000);
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = 600;
    loaded.cpu.gpr[5] = 0;
    run_quicktime_callback_test_import(&mut loaded, operation(Op::GetTimeBaseTime));
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(callback));
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(0x87654321));
}

fn run_quicktime_callback_test_import(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) {
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = target;
    let probe = loaded.run_with_hle_imports(512);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(matches!(probe.result, PpcRunResult::Halted { pc: PPC_HALT_PC, .. }), "{probe:?}");
    assert!(loaded.guest_calls().is_empty());
    assert!(loaded.quicktime.callback_frames.is_empty());
}

#[test]
fn quicktime_callback_end_delivers_all_due_events_and_disposal_callback() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"Apple;Carbon;Multimedia", b"NewCallBack",
    )).unwrap();
    use PpcQuickTimeCompatibilityOperation as Op;
    let operation = |op| PpcImportDispatcherTarget::QuickTimeCompatibility(op);
    let code = PPC_CODE_BASE + 0x4000;
    let vector = PPC_DATA_BASE + 0x1000;
    let observed = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(code, [0x80a20000u32, 0x38a50001, 0x90a20000, 0x90820004, 0x4e800020]
        .into_iter().flat_map(u32::to_be_bytes).collect());
    loaded.memory.add_region(vector, [code, observed].into_iter().flat_map(u32::to_be_bytes).collect());
    loaded.memory.add_region(observed, vec![0; 8]);
    for kind in [2, 4, 5] {
        loaded.cpu.gpr[3] = PPC_QT_MOVIE + 0x10;
        loaded.cpu.gpr[4] = kind;
        run_quicktime_callback_test_import(&mut loaded, operation(Op::NewCallBack));
        let reference = loaded.cpu.gpr[3];
        assert_ne!(reference, 0);
        loaded.cpu.gpr[3] = reference;
        loaded.cpu.gpr[4] = vector;
        loaded.cpu.gpr[5] = kind;
        loaded.cpu.gpr[6] = if kind == 2 { 16 } else { 2 };
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_quicktime_callback_test_import(&mut loaded, operation(Op::CallMeWhen));
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_file_duration = 600;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_tasks_until_done = 6;
    loaded.quicktime.movie_task_count = 5;
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    advance_quicktime_test_clock(&mut loaded, 10_000_000);
    run_quicktime_callback_test_import(&mut loaded, PpcImportDispatcherTarget::QtMoviesTask);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(2));
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(4));
    assert!(!loaded.quicktime.movie_disposed);
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    run_quicktime_callback_test_import(&mut loaded, PpcImportDispatcherTarget::QtDisposeMovie);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(3));
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(5));
    assert!(loaded.quicktime.movie_disposed);
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    run_quicktime_callback_test_import(&mut loaded, PpcImportDispatcherTarget::QtDisposeMovie);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(3));
}


#[test]
fn quicktime_controller_idle_delivers_due_guest_callback_once() {
    use PpcQuickTimeCompatibilityOperation as Op;
    for operation in [Op::MCIdle, Op::MCDoAction] {
        let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
            b"Apple;Carbon;Multimedia",
            b"NewCallBack",
        ))
        .unwrap();
        let code = PPC_CODE_BASE + 0x4000;
        let vector = PPC_DATA_BASE + 0x1000;
        let observed = PPC_DATA_BASE + 0x1200;
        loaded.memory.add_region(
            code,
            [
                0x80a20000u32,
                0x38a50001,
                0x90a20000,
                0x90620004,
                0x90820008,
                0x4e800020,
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        loaded.memory.add_region(
            vector,
            [code, observed]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        loaded.memory.add_region(observed, vec![0; 12]);
        loaded.cpu.gpr[3] = PPC_QT_MOVIE + 0x10;
        loaded.cpu.gpr[4] = 1;
        run_quicktime_callback_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickTimeCompatibility(Op::NewCallBack),
        );
        let callback = loaded.cpu.gpr[3];
        assert_ne!(callback, 0);
        loaded.cpu.gpr[3] = callback;
        loaded.cpu.gpr[4] = vector;
        loaded.cpu.gpr[5] = 0xdeadbeef;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 50;
        loaded.cpu.gpr[8] = 600;
        run_quicktime_callback_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickTimeCompatibility(Op::CallMeWhen),
        );
        loaded.quicktime.movie_active = true;
        loaded.quicktime.movie_started = true;
        loaded.quicktime.movie_file_duration = 600;
        loaded.quicktime.movie_time_scale = 600;
        loaded.quicktime.movie_tasks_until_done = 6;
        let controller = 0x5000;
        loaded
            .quicktime
            .movie_controllers
            .push(PpcQuickTimeControllerRecord {
                handle: controller,
                movie: PPC_QT_MOVIE,
                rect: (0, 0, 100, 100),
                flags: 0,
                window: 0,
                origin: 0,
                action_filter: 0,
                action_ref_con: 0,
                action_filter_rtoc: 0,
            });
        let rtoc = loaded.cpu.gpr[2];
        advance_quicktime_test_clock(&mut loaded, 10_000_000);
        for _ in 0..2 {
            loaded.cpu.gpr[3] = controller;
            loaded.cpu.gpr[4] = 1;
            run_quicktime_callback_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::QuickTimeCompatibility(operation),
            );
            assert_eq!(loaded.cpu.gpr[2], rtoc);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }
        assert_eq!(loaded.memory.read_u32_be(observed), Some(1));
        assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(callback));
        assert_eq!(loaded.memory.read_u32_be(observed + 8), Some(0xdeadbeef));
    }
}


#[test]
fn quicktime_callback_type_returns_signed_short_flags_and_preserves_time_base() {
    use PpcQuickTimeCompatibilityOperation as Op;
    for library in [
        b"QuickTimeLib".as_slice(),
        b"Apple;Carbon;Multimedia".as_slice(),
    ] {
        let mut loaded =
            load_pef_application(&synthetic_pef_with_library_import(library, b"NewCallBack"))
                .unwrap();
        for (kind, result) in [(1, 1), (0x4001, 0x4001), (0x8001, 0xffff8001)] {
            loaded.cpu.gpr[3] = PPC_QT_MOVIE + 0x10;
            loaded.cpu.gpr[4] = kind;
            run_quicktime_callback_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::QuickTimeCompatibility(Op::NewCallBack),
            );
            let callback = loaded.cpu.gpr[3];
            assert_ne!(callback, 0);
            loaded.cpu.gpr[3] = callback;
            run_quicktime_callback_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::QuickTimeCompatibility(Op::GetCallBackType),
            );
            assert_eq!(loaded.cpu.gpr[3], result);
            loaded.cpu.gpr[3] = callback;
            run_quicktime_callback_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::QuickTimeCompatibility(Op::GetCallBackTimeBase),
            );
            assert_eq!(loaded.cpu.gpr[3], PPC_QT_MOVIE + 0x10);
            loaded.cpu.gpr[3] = callback;
            run_quicktime_callback_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::QuickTimeCompatibility(Op::DisposeCallBack),
            );
            assert!(loaded.quicktime.callbacks.get(callback).is_none());
        }
    }
}

#[test]
fn movie_time_base_loop_wraps_and_marks_callbacks_without_stopping() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"GetMovieTimeBase",
    ))
    .unwrap();
    loaded.quicktime.movie_file_duration = 6000;
    loaded.quicktime.movie_creation_time_scale = 600;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_tasks_until_done = 600;
    let base = call_movie_toolbox(&mut loaded, "GetMovieTimeBase", &[PPC_QT_MOVIE]);
    let record = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(record, 1).unwrap();
    loaded.memory.write_u32_be(record + 8, 1).unwrap();
    loaded.memory.write_u32_be(record + 12, 0).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[base, record]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[base, 1]);
    let registration = quicktime::callbacks::Registration {
        procedure: 0x1000,
        rtoc: 0,
        refcon: 0,
        parameters: [1, 300, 600],
    };
    assert!(loaded.quicktime.callbacks.create(0x3000, base, 1));
    assert!(loaded.quicktime.callbacks.arm(0x3000, registration));
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_playback_rate = 0x10000;
    loaded.quicktime.movie_clock_subtick = None;
    let mut sound = PpcSoundState::default();
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 0);
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 120_000_000);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        0
    );
    assert!(loaded.quicktime.movie_started);
    assert_eq!(
        loaded.quicktime.callbacks.take_ready(),
        Some((0x3000, registration))
    );
    loaded.quicktime.movie_playback_rate = -0x10000;
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 135_000_000);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        450
    );
    assert!(loaded.quicktime.movie_started);
}

#[test]
fn movie_time_base_palindrome_reverses_at_each_extreme() {
    let mut quicktime = PpcQuickTimeState::default();
    quicktime.movie_file_duration = 6000;
    quicktime.movie_creation_time_scale = 600;
    quicktime.movie_time_scale = 600;
    quicktime.movie_tasks_until_done = 600;
    quicktime.movie_time_base_stop =
        Some(i128::from(crate::time_manager::SUBTICKS_PER_TICK) * 60 * 65536);
    quicktime.movie_time_base_flags = 3;
    quicktime.movie_started = true;
    quicktime.movie_active = true;
    quicktime.movie_playback_rate = 0x10000;
    let mut sound = PpcSoundState::default();
    ppc_qt_service_clock(&mut quicktime, &mut sound, 0);
    for (clock, expected) in [
        (30_000_000, 300),
        (60_000_000, 600),
        (90_000_000, 300),
        (120_000_000, 0),
        (150_000_000, 300),
    ] {
        ppc_qt_service_clock(&mut quicktime, &mut sound, clock);
        assert_eq!(
            dispatch_quicktime::quicktime_callback_time(&quicktime),
            expected
        );
        assert!(quicktime.movie_started);
    }
}

#[test]
fn movie_palindrome_effective_rate_tracks_turns_and_mastered_bases() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"GetMovieTimeBase",
    ))
    .unwrap();
    loaded.quicktime.movie_file_duration = 6000;
    loaded.quicktime.movie_creation_time_scale = 600;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_tasks_until_done = 600;
    let movie_base = call_movie_toolbox(&mut loaded, "GetMovieTimeBase", &[PPC_QT_MOVIE]);
    let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[slave, 0x8000]);
    call_movie_toolbox(
        &mut loaded,
        "SetTimeBaseMasterTimeBase",
        &[slave, movie_base, 0],
    );
    let record = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(record, 1).unwrap();
    loaded.memory.write_u32_be(record + 8, 1).unwrap();
    loaded.memory.write_u32_be(record + 12, 0).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[movie_base, record]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[movie_base, 3]);
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_playback_rate = 0x10000;
    loaded.quicktime.movie_clock_subtick = None;
    let mut sound = PpcSoundState::default();
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 0);
    for (clock, expected) in [
        (30_000_000, 0x10000i32),
        (60_000_000, -0x10000),
        (90_000_000, -0x10000),
        (120_000_000, 0x10000),
    ] {
        ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, clock);
        assert_eq!(
            call_movie_toolbox(&mut loaded, "GetTimeBaseEffectiveRate", &[movie_base]) as i32,
            expected
        );
        assert_eq!(
            call_movie_toolbox(&mut loaded, "GetTimeBaseEffectiveRate", &[slave]) as i32,
            expected / 2
        );
        assert_eq!(
            call_movie_toolbox(&mut loaded, "GetTimeBaseRate", &[movie_base]),
            0x10000
        );
    }
    loaded.quicktime.movie_playback_rate = -0x10000;
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseEffectiveRate", &[movie_base]) as i32,
        0x10000
    );
    call_movie_toolbox(
        &mut loaded,
        "SetMovieActiveSegment",
        &[PPC_QT_MOVIE, 0, 1200],
    );
    assert_eq!(loaded.quicktime.movie_time_base_phase, None);
}

#[test]
fn movie_time_base_bounds_limit_playback_with_active_segment() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"GetMovieTimeBase",
    ))
    .unwrap();
    loaded.quicktime.movie_file_duration = 6000;
    loaded.quicktime.movie_creation_time_scale = 600;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_tasks_until_done = 600;
    let base = call_movie_toolbox(&mut loaded, "GetMovieTimeBase", &[PPC_QT_MOVIE]);
    let record = PPC_STACK_TOP - 64;
    loaded.memory.write_u32_be(record + 8, 1).unwrap();
    loaded.memory.write_u32_be(record + 12, 0).unwrap();
    loaded.memory.write_u64_be(record, 2).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, record]);
    loaded.memory.write_u64_be(record, 4).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[base, record]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseStartTime", &[base, 600, 0]),
        1200
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseStopTime", &[base, 600, 0]),
        2400
    );
    call_movie_toolbox(
        &mut loaded,
        "SetMovieActiveSegment",
        &[PPC_QT_MOVIE, 600, 3000],
    );
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_playback_rate = 0x10000;
    loaded.quicktime.movie_clock_subtick = None;
    let mut sound = PpcSoundState::default();
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 0);
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 300_000_000);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        2400
    );
    assert!(!loaded.quicktime.movie_started);
    assert_eq!(loaded.quicktime.movie_active_segment, Some((600, 3000)));
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_playback_rate = -0x10000;
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 600_000_000);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        1200
    );
    assert!(!loaded.quicktime.movie_started);
}

#[test]
fn movie_time_base_zero_scale_uses_movie_time_scale() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"GetTimeBaseTime",
    ))
    .unwrap();
    loaded.quicktime.movie_file_duration = 10000;
    loaded.quicktime.movie_creation_time_scale = 1000;
    loaded.quicktime.movie_time_scale = 1000;
    loaded.quicktime.movie_tasks_until_done = 1000;
    call_movie_toolbox(&mut loaded, "SetMovieTimeValue", &[PPC_QT_MOVIE, 2500]);
    let base = call_movie_toolbox(&mut loaded, "GetMovieTimeBase", &[PPC_QT_MOVIE]);
    let record = PPC_STACK_TOP - 64;
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 0, record]),
        2500
    );
    assert_eq!(loaded.memory.read_u64_be(record), Some(2500));
    assert_eq!(loaded.memory.read_u32_be(record + 8), Some(1000));
    assert_eq!(loaded.memory.read_u32_be(record + 12), Some(base));
    loaded.memory.write_u64_be(record, 1500).unwrap();
    loaded.memory.write_u32_be(record + 8, 0).unwrap();
    call_movie_toolbox(&mut loaded, "SetMovieTime", &[PPC_QT_MOVIE, record]);
    assert_eq!(loaded.quicktime.movie_error, PPC_NO_ERR);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        1500
    );
    loaded.memory.write_u64_be(record, 1200).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseTime", &[base, record]);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        1200
    );
}

#[test]
fn movie_time_record_converts_standalone_time_base_with_active_segment() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    loaded.quicktime.movie_file_duration = 6000;
    loaded.quicktime.movie_creation_time_scale = 600;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_tasks_until_done = 600;
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_playback_rate = 0x10000;
    let base = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x8000]);
    call_movie_toolbox(
        &mut loaded,
        "SetMovieActiveSegment",
        &[PPC_QT_MOVIE, 1200, 1800],
    );
    let record = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(record, 2).unwrap();
    loaded.memory.write_u32_be(record + 8, 1).unwrap();
    loaded.memory.write_u32_be(record + 12, base).unwrap();
    call_movie_toolbox(&mut loaded, "SetMovieTime", &[PPC_QT_MOVIE, record]);
    assert_eq!(loaded.quicktime.movie_error, PPC_NO_ERR);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        2400
    );
    assert_eq!(loaded.quicktime.movie_active_segment, Some((1200, 1800)));
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0]);
    loaded.memory.write_u64_be(record, 3).unwrap();
    call_movie_toolbox(&mut loaded, "SetMovieTime", &[PPC_QT_MOVIE, record]);
    assert_eq!(loaded.quicktime.movie_error, PPC_PARAM_ERR);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        2400
    );
}

#[test]
fn movie_active_segment_bounds_playback_and_time_record_seeking() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"SetMovieActiveSegment",
    ))
    .unwrap();
    loaded.quicktime.movie_file_duration = 6000;
    loaded.quicktime.movie_creation_time_scale = 600;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_tasks_until_done = 600;
    let op = |operation| PpcImportDispatcherTarget::QuickTimeCompatibility(operation);
    let record = PPC_STACK_TOP - 64;
    for (symbol, operation) in [
        (
            "SetMovieActiveSegment",
            PpcQuickTimeCompatibilityOperation::SetMovieActiveSegment,
        ),
        (
            "GetMovieActiveSegment",
            PpcQuickTimeCompatibilityOperation::GetMovieActiveSegment,
        ),
        (
            "SetMovieTime",
            PpcQuickTimeCompatibilityOperation::SetMovieTime,
        ),
    ] {
        assert_eq!(
            dispatcher_target_for_import("QuickTimeLib", symbol),
            op(operation)
        );
    }
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 1200;
    loaded.cpu.gpr[5] = 1800;
    run_test_import(
        &mut loaded,
        op(PpcQuickTimeCompatibilityOperation::SetMovieActiveSegment),
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = record;
    loaded.cpu.gpr[5] = record + 4;
    run_test_import(
        &mut loaded,
        op(PpcQuickTimeCompatibilityOperation::GetMovieActiveSegment),
    );
    assert_eq!(loaded.memory.read_u32_be(record), Some(1200));
    assert_eq!(loaded.memory.read_u32_be(record + 4), Some(1800));

    loaded.memory.write_u64_be(record, 4).unwrap();
    loaded.memory.write_u32_be(record + 8, 1).unwrap();
    loaded.memory.write_u32_be(record + 12, 0).unwrap();
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = record;
    run_test_import(
        &mut loaded,
        op(PpcQuickTimeCompatibilityOperation::SetMovieTime),
    );
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        2400
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QtGoToBeginningOfMovie,
    );
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        1200
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::QtGoToEndOfMovie);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        3000
    );
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_playback_rate = -0x10000;
    let mut sound = PpcSoundState::default();
    loaded.quicktime.movie_clock_subtick = None;
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 0);
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 600_000_000);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        1200
    );
    assert!(!loaded.quicktime.movie_started);
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_playback_rate = 0x10000;
    loaded.quicktime.movie_clock_subtick = None;
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 0);
    ppc_qt_service_clock(&mut loaded.quicktime, &mut sound, 600_000_000);
    assert_eq!(
        dispatch_quicktime::quicktime_callback_time(&loaded.quicktime),
        3000
    );
    assert!(!loaded.quicktime.movie_started);
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 1200;
    run_test_import(
        &mut loaded,
        op(PpcQuickTimeCompatibilityOperation::SetMovieTimeScale),
    );
    assert_eq!(loaded.quicktime.movie_active_segment, Some((2400, 3600)));
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 100;
    loaded.cpu.gpr[5] = u32::MAX;
    run_test_import(
        &mut loaded,
        op(PpcQuickTimeCompatibilityOperation::SetMovieActiveSegment),
    );
    assert_eq!(loaded.quicktime.movie_error, -2014);
    assert_eq!(loaded.quicktime.movie_active_segment, Some((2400, 3600)));
    loaded.memory.write_u32_be(record + 8, 0).unwrap();
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = record;
    let before = dispatch_quicktime::quicktime_callback_time(&loaded.quicktime);
    run_test_import(
        &mut loaded,
        op(PpcQuickTimeCompatibilityOperation::SetMovieTime),
    );
    assert_eq!(loaded.quicktime.movie_error, PPC_PARAM_ERR);
    assert_eq!(dispatch_quicktime::quicktime_callback_time(&loaded.quicktime), before);
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = u32::MAX;
    run_test_import(
        &mut loaded,
        op(PpcQuickTimeCompatibilityOperation::SetMovieActiveSegment),
    );
    assert_eq!(loaded.quicktime.movie_active_segment, None);
    loaded.memory.write_u32_be(record + 4, 0x12345678).unwrap();
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = record;
    loaded.cpu.gpr[5] = record + 4;
    run_test_import(
        &mut loaded,
        op(PpcQuickTimeCompatibilityOperation::GetMovieActiveSegment),
    );
    assert_eq!(loaded.memory.read_u32_be(record), Some(u32::MAX));
    assert_eq!(loaded.memory.read_u32_be(record + 4), Some(0x12345678));
}

#[test]
fn movie_time_follows_set_movie_time_value() {
    let pef = synthetic_pef_with_import(b"SetMovieTimeValue");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.quicktime.movie_file_duration = 600;
    loaded.quicktime.movie_tasks_until_done = 4;
    assert_eq!(
        dispatcher_target_for_import("QuickTimeLib", "SetMovieTimeValue"),
        PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::SetMovieTimeValue
        )
    );
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QuickTimeCompatibility(
        PpcQuickTimeCompatibilityOperation::SetMovieTimeValue,
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = 300;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quicktime.movie_task_count, 2);
    assert!(!loaded.quicktime.movie_at_beginning);

    let record = PPC_STACK_TOP - 64;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QuickTimeCompatibility(
        PpcQuickTimeCompatibilityOperation::GetMovieTime,
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = record;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 300);
    assert_eq!(loaded.memory.read_u32_be(record + 4), Some(300));
    assert_eq!(
        loaded.memory.read_u32_be(record + 8),
        Some(loaded.quicktime.movie_time_scale)
    );
}

#[test]
fn movie_color_table_is_nil_and_component_version_rejects_instances() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetComponentVersion"),
        PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::GetComponentVersion
        )
    );
    let pef = synthetic_pef_with_import(b"GetMovieColorTable");
    let mut loaded = load_pef_application(&pef).unwrap();
    let out = PPC_STACK_TOP - 64;
    loaded.memory.write_u32_be(out, 0xDEAD_BEEF).unwrap();
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QuickTimeCompatibility(
        PpcQuickTimeCompatibilityOperation::GetMovieColorTable,
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = out;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(out), Some(0));
}

#[test]
fn quicktime_mc_controller_port_round_trips() {
    let pef = synthetic_pef_with_library_import(b"QuickTimeLib", b"MCSetControllerPort");
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
            action_filter_rtoc: 0,
    });
    loaded.cpu.gpr[3] = controller;
    loaded.cpu.gpr[4] = 0x0204_0000;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QuickTimeCompatibility(
        PpcQuickTimeCompatibilityOperation::MCGetControllerPort,
    );
    loaded.cpu.gpr[3] = controller;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0x0204_0000);
}

fn quicktime_audio_file_fixture(compressed: bool) -> Vec<u8> {
    let (channels, frames, bits, rate, samples) = if compressed {
        let mut packet = vec![0x77; 34];
        packet[..2].copy_from_slice(&0i16.to_be_bytes());
        (
            2u16,
            2u32,
            16u16,
            [0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0],
            packet.repeat(4),
        )
    } else {
        (
            1u16,
            8000u32,
            8u16,
            [0x40, 0x0b, 0xfa, 0, 0, 0, 0, 0, 0, 0],
            vec![0x40; 8000],
        )
    };
    let mut comm = Vec::new();
    comm.extend_from_slice(&channels.to_be_bytes());
    comm.extend_from_slice(&frames.to_be_bytes());
    comm.extend_from_slice(&bits.to_be_bytes());
    comm.extend_from_slice(&rate);
    if compressed {
        comm.extend_from_slice(b"ima4\0\0");
    }
    let mut body = if compressed {
        b"AIFC".to_vec()
    } else {
        b"AIFF".to_vec()
    };
    body.extend_from_slice(b"COMM");
    body.extend_from_slice(&(comm.len() as u32).to_be_bytes());
    body.extend_from_slice(&comm);
    body.extend_from_slice(b"SSND");
    body.extend_from_slice(&(samples.len() as u32 + 8).to_be_bytes());
    body.extend_from_slice(&[0; 8]);
    body.extend_from_slice(&samples);
    let mut file = b"FORM".to_vec();
    file.extend_from_slice(&(body.len() as u32).to_be_bytes());
    file.extend_from_slice(&body);
    file
}

#[test]
fn quicktime_imports_aiff_and_compressed_aifc_as_audio_movies() {
    for compressed in [false, true] {
        let data = quicktime_audio_file_fixture(compressed);
        let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
            b"QuickTimeLib",
            b"OpenMovieFile",
        ))
        .unwrap();
        const SCRATCH: u32 = PPC_HEAP_BASE + 0x1000;
        loaded.memory.add_region(SCRATCH, vec![0; 128]);
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Music.aif".into(),
            data: data.into(),
            creator: 0,
            file_type: u32::from_be_bytes(*b"AIFF"),
            finder_flags: 0,
            dirty: false,
        });
        write_ppc_fsspec(
            &mut loaded.memory,
            SCRATCH,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_ROOT_DIR_ID,
            b"Music.aif",
        );
        loaded.cpu.gpr[3] = SCRATCH;
        loaded.cpu.gpr[4] = SCRATCH + 80;
        loaded.cpu.gpr[5] = 1;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QtOpenMovieFile);
        assert_eq!(loaded.cpu.gpr[3], 0);
        let file_ref = loaded.memory.read_u16_be(SCRATCH + 80).unwrap();
        loaded.cpu.gpr[3] = SCRATCH + 84;
        loaded.cpu.gpr[4] = u32::from(file_ref);
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 1;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QtNewMovieFromFile);
        assert_eq!(loaded.cpu.gpr[3], 0, "compressed={compressed}");
        assert_eq!(loaded.memory.read_u32_be(SCRATCH + 84), Some(PPC_QT_MOVIE));
        let (rate, count) = if compressed {
            (44_100, 128)
        } else {
            (8000, 8000)
        };
        assert_eq!(loaded.quicktime.movie_time_scale, rate);
        assert_eq!(loaded.quicktime.movie_file_duration, count);
        assert!(loaded.quicktime.movie_video_track.is_none());
        assert_eq!(loaded.quicktime.movie_tracks.len(), 1);
        assert_eq!(
            loaded.quicktime.movie_tracks[0].media_type,
            u32::from_be_bytes(*b"soun")
        );
        loaded.cpu.gpr[3] = PPC_QT_MOVIE;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QtStartMovie);
        let playback = loaded
            .sound
            .decoded_file_playbacks
            .last()
            .expect("audio playback");
        assert_eq!(playback.samples.len(), count as usize);
        assert_eq!(playback.sample_rate_fixed, rate << 16);
        assert!(playback.samples.iter().any(|sample| *sample != 128));
        let initial_samples = playback.samples.clone();
        let midpoint = (count / 2) as u32;
        // Seek through a movie controller, using a different time scale.
        ppc_write_rect(&mut loaded.memory, SCRATCH + 100, 0, 0, 4, 4).unwrap();
        let controller = call_movie_toolbox(
            &mut loaded,
            "NewMovieController",
            &[PPC_QT_MOVIE, SCRATCH + 100, 0],
        );
        loaded
            .memory
            .write_u64_be(SCRATCH + 120, u64::from(midpoint) * 2)
            .unwrap();
        loaded.memory.write_u32_be(SCRATCH + 128, rate * 2).unwrap();
        loaded.memory.write_u32_be(SCRATCH + 132, 0).unwrap();
        assert_eq!(
            call_movie_toolbox(&mut loaded, "MCDoAction", &[controller, 12, SCRATCH + 120]),
            0
        );
        assert_eq!(
            call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
            midpoint
        );
        assert_eq!(
            loaded.sound.decoded_file_playbacks.last().unwrap().samples,
            initial_samples[midpoint as usize..]
        );
        call_movie_toolbox(&mut loaded, "SetMovieRate", &[PPC_QT_MOVIE, 0x8000]);
        assert_eq!(
            loaded
                .sound
                .decoded_file_playbacks
                .last()
                .unwrap()
                .sample_rate_fixed,
            rate << 15
        );
        call_movie_toolbox(
            &mut loaded,
            "SetMovieRate",
            &[PPC_QT_MOVIE, (-0x10000i32) as u32],
        );
        assert_eq!(
            loaded.sound.decoded_file_playbacks.last().unwrap().samples,
            initial_samples[..midpoint as usize]
                .iter()
                .rev()
                .copied()
                .collect::<Vec<_>>()
        );
        advance_quicktime_test_clock(&mut loaded, 120_000_000);
        assert_eq!(
            call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
            0
        );
        assert!(!loaded.quicktime.movie_started);
        let base = SCRATCH + 0x200;
        let port = SCRATCH + 0x400;
        loaded.memory.add_region(base, vec![0xee; 32]);
        loaded.gworlds = vec![PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: base,
            gdevice: 0,
            width: 4,
            height: 4,
            depth: 16,
            row_bytes: 8,
            pixels_locked: false,
            pixels_no_purge: false,
        }];
        loaded.quicktime.movie_box = (0, 0, 4, 4);
        assert!(
            ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, port).is_some()
        );
        assert!(!ppc_qt_draw_movie_frame(
            &mut loaded.memory,
            &loaded.gworlds,
            port,
            &mut loaded.quicktime,
            0
        ));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, base, 32),
            Some(vec![0xee; 32])
        );
    }
}

#[test]
fn quicktime_aiff_import_preserves_resource_selection_and_rejects_bad_data() {
    let original = quicktime_audio_file_fixture(true);
    for requested in [None, Some(0), Some(-1)] {
        let mut data = original.clone();
        assert_eq!(
            ppc_qt_append_movie_resource(&mut data, &[], &[], "Music.aif", requested),
            Some(-1)
        );
        assert_eq!(data, original);
    }
    let mut data = original.clone();
    assert_eq!(
        ppc_qt_append_movie_resource(&mut data, &[], &[], "Music.aif", Some(128)),
        None
    );
    assert_eq!(data, original);
    let mut truncated = original.clone();
    truncated.pop();
    let mut unknown_codec = original.clone();
    unknown_codec[38..42].copy_from_slice(b"????");
    for mut invalid in [truncated, unknown_codec, b"FORM\0\0\0\x04AIFF".to_vec()] {
        assert_eq!(
            ppc_qt_append_movie_resource(&mut invalid, &[], &[], "Music.aif", None),
            None
        );
        assert!(ppc_qt_movie_first_audio_track(&invalid).is_none());
    }
}

fn movie_toolbox_fixture() -> PpcLoadedApp {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"GetMovieTrackCount",
    ))
    .unwrap();
    let mut movie = test_quicktime_tkhd_movie(320, 240);
    // Supply valid tkhd duration and identity matrices; the older fixture
    // leaves these fields zero because it only probes dimensions and media.
    let headers: Vec<_> = movie
        .windows(4)
        .enumerate()
        .filter_map(|(index, kind)| (kind == b"tkhd").then_some(index + 4))
        .collect();
    for header in headers {
        movie[header + 20..header + 24].copy_from_slice(&120u32.to_be_bytes());
        for (offset, value) in [(40, 0x10000u32), (56, 0x10000), (72, 0x40000000)] {
            movie[header + offset..header + offset + 4].copy_from_slice(&value.to_be_bytes());
        }
        movie[header + 32..header + 34].copy_from_slice(&(-2i16).to_be_bytes());
        movie[header + 36..header + 38].copy_from_slice(&0x100u16.to_be_bytes());
    }
    loaded.quicktime.movie_tracks = ppc_qt_movie_tracks(&movie);
    loaded.quicktime.movie_file_data = movie;
    loaded.quicktime.movie_creation_time_scale = 60;
    loaded.quicktime.movie_time_scale = 60;
    loaded.quicktime.movie_file_duration = 120;
    loaded.quicktime.movie_file_bounds = Some((0, 0, 240, 320));
    loaded.quicktime.movie_box = (0, 0, 240, 320);
    loaded
        .memory
        .add_region(PPC_DATA_BASE + 0x4000, vec![0; 0x400]);
    loaded
}

fn call_movie_toolbox(loaded: &mut PpcLoadedApp, symbol: &str, args: &[u32]) -> u32 {
    let target = dispatcher_target_for_import("QuickTimeLib", symbol);
    assert!(
        matches!(target, PpcImportDispatcherTarget::QuickTimeCompatibility(_)),
        "{symbol}"
    );
    loaded.cpu.gpr[3..3 + args.len()].copy_from_slice(args);
    run_test_import(loaded, target);
    loaded.cpu.gpr[3]
}

#[test]
fn standalone_time_base_advances_from_guest_clock_and_preserves_scaled_times() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let base = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    assert_ne!(base, 0);
    call_movie_toolbox(&mut loaded, "SetTimeBaseValue", &[base, 1, 7]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 7, 0]),
        1
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseRate", &[base]),
        0
    );
    call_movie_toolbox(&mut loaded, "SetTimeBaseValue", &[base, 3, 2]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x8000]);
    advance_quicktime_test_clock(&mut loaded, 60_000_000);
    let record = PPC_STACK_TOP - 64;
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 600, record]),
        1200
    );
    assert_eq!(loaded.memory.read_u64_be(record), Some(1200));
    assert_eq!(loaded.memory.read_u32_be(record + 8), Some(600));
    assert_eq!(loaded.memory.read_u32_be(record + 12), Some(base));
    loaded.memory.write_u64_be(record, 300).unwrap();
    loaded.memory.write_u32_be(record + 8, 0).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseTime", &[base, record]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 0, 0]),
        300
    );
    loaded.memory.write_u64_be(record, 600).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, record]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseStartTime", &[base, 0, 0]),
        600
    );
    call_movie_toolbox(&mut loaded, "DisposeTimeBase", &[base]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseRate", &[base]),
        0
    );
}

#[test]
fn palindrome_time_base_rate_update_preserves_reverse_phase() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let base = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    let bound = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(bound, 0).unwrap();
    loaded.memory.write_u32_be(bound + 8, 1).unwrap();
    loaded.memory.write_u32_be(bound + 12, 0).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
    loaded.memory.write_u64_be(bound, 1).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[base, bound]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[base, 3]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
    advance_quicktime_test_clock(&mut loaded, 90_000_000);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 4, 0]),
        2
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseEffectiveRate", &[base]),
        0xffff0000
    );
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseEffectiveRate", &[base]),
        0xffff0000
    );
    advance_quicktime_test_clock(&mut loaded, 15_000_000);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 4, 0]),
        1
    );
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0xffff0000]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseEffectiveRate", &[base]),
        0x10000
    );
    advance_quicktime_test_clock(&mut loaded, 15_000_000);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 4, 0]),
        2
    );
}

#[test]
fn zero_scale_master_offset_uses_source_time_base_scale() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[master, 0x10000]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[slave, 0x10000]);
    advance_quicktime_test_clock(&mut loaded, 60_000_000);
    let record = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(record, 300).unwrap();
    loaded.memory.write_u32_be(record + 8, 0).unwrap();
    loaded.memory.write_u32_be(record + 12, master).unwrap();
    call_movie_toolbox(
        &mut loaded,
        "SetTimeBaseMasterTimeBase",
        &[slave, master, record],
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseMasterTimeBase", &[slave]),
        master
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[slave, 600, 0]),
        300
    );
}

#[test]
fn standalone_time_bases_convert_through_a_shared_master() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[master, 0x10000]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[slave, 0x8000]);
    advance_quicktime_test_clock(&mut loaded, 60_000_000);
    let record = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(record, 1).unwrap();
    loaded.memory.write_u32_be(record + 8, 4).unwrap();
    loaded.memory.write_u32_be(record + 12, master).unwrap();
    call_movie_toolbox(
        &mut loaded,
        "SetTimeBaseMasterTimeBase",
        &[slave, master, record],
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseMasterTimeBase", &[slave]),
        master
    );
    call_movie_toolbox(
        &mut loaded,
        "SetTimeBaseMasterTimeBase",
        &[master, slave, 0],
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseMasterTimeBase", &[master]),
        0
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseEffectiveRate", &[slave]),
        0x8000
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[slave, 600, 0]),
        225
    );
    advance_quicktime_test_clock(&mut loaded, 120_000_000);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[slave, 600, 0]),
        825
    );
    loaded.memory.write_u64_be(record, 825).unwrap();
    loaded.memory.write_u32_be(record + 8, 600).unwrap();
    loaded.memory.write_u32_be(record + 12, slave).unwrap();
    loaded.quicktime.movie_error = -2010;
    call_movie_toolbox(&mut loaded, "ConvertTime", &[record, master]);
    assert_eq!(loaded.memory.read_u64_be(record), Some(1800));
    assert_eq!(loaded.memory.read_u32_be(record + 12), Some(master));
    assert_eq!(loaded.quicktime.movie_error, -2010);
    loaded.memory.write_u64_be(record, 825).unwrap();
    loaded.memory.write_u32_be(record + 8, 0).unwrap();
    loaded.memory.write_u32_be(record + 12, slave).unwrap();
    call_movie_toolbox(&mut loaded, "ConvertTime", &[record, master]);
    assert_eq!(loaded.memory.read_u64_be(record), Some(1800));
    assert_eq!(loaded.memory.read_u32_be(record + 8), Some(600));
    assert_eq!(loaded.memory.read_u32_be(record + 12), Some(master));
    assert_eq!(loaded.quicktime.movie_error, -2010);
    call_movie_toolbox(&mut loaded, "ConvertTime", &[record, 0x7fff_ffff]);
    assert_eq!(loaded.memory.read_u64_be(record), Some(1800));
    assert_eq!(loaded.memory.read_u32_be(record + 12), Some(master));
    assert_eq!(loaded.quicktime.movie_error, -2010);
}

#[test]
fn standalone_time_base_bounds_and_loop_flags_affect_reported_time() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"NewTimeBase",
    ))
    .unwrap();
    let base = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
    let record = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(record, 2).unwrap();
    loaded.memory.write_u32_be(record + 8, 1).unwrap();
    loaded.memory.write_u32_be(record + 12, base).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, record]);
    loaded.memory.write_u64_be(record, 5).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[base, record]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseStartTime", &[base, 1, record]),
        2
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseStopTime", &[base, 1, record]),
        5
    );
    call_movie_toolbox(&mut loaded, "SetTimeBaseValue", &[base, 6, 1]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 1, 0]),
        5
    );
    call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[base, 1]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseFlags", &[base]),
        1
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 1, 0]),
        3
    );
    call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[base, 3]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 1, 0]),
        4
    );
}

#[test]
fn quicktime_toolbox_track_metadata_scales_and_rejects_bad_indices() {
    let mut loaded = movie_toolbox_fixture();
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTrackCount", &[PPC_QT_MOVIE]),
        2
    );
    let track = call_movie_toolbox(&mut loaded, "GetMovieIndTrack", &[PPC_QT_MOVIE, 1]);
    assert_eq!(track, loaded.quicktime.movie_tracks[0].handle);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTrackDuration", &[track]),
        120
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTrackLayer", &[track]) as i16,
        -2
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTrackVolume", &[track]),
        0x100
    );
    call_movie_toolbox(&mut loaded, "SetMovieTimeScale", &[PPC_QT_MOVIE, 600]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTrackDuration", &[track]),
        1200
    );
    call_movie_toolbox(&mut loaded, "SetTrackOffset", &[track, 300]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTrackOffset", &[track]),
        300
    );
    call_movie_toolbox(&mut loaded, "SetTrackEnabled", &[track, 0]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetTrackEnabled", &[track]),
        0
    );
    for index in [0, 3, u32::MAX, i32::MIN as u32] {
        assert_eq!(
            call_movie_toolbox(&mut loaded, "GetMovieIndTrack", &[PPC_QT_MOVIE, index]),
            0
        );
        assert_eq!(loaded.quicktime.movie_error, -2028);
    }
    assert_eq!(call_movie_toolbox(&mut loaded, "GetTrackDuration", &[0]), 0);
    assert_eq!(loaded.quicktime.movie_error, -2009);
}

#[test]
fn quicktime_toolbox_media_characteristics_write_boolean_output() {
    let mut loaded = movie_toolbox_fixture();
    let out = PPC_DATA_BASE + 0x4000;
    let track = loaded.quicktime.movie_tracks[1].handle;
    let handler = call_movie_toolbox(&mut loaded, "GetMediaHandler", &[track + 1]);
    for (kind, expected) in [(b"ears", 1), (b"eyes", 0)] {
        loaded.memory.write_u32_be(out, 0xa5a5a5a5).unwrap();
        assert_eq!(
            call_movie_toolbox(
                &mut loaded,
                "MediaHasCharacteristic",
                &[handler, u32::from_be_bytes(*kind), out]
            ),
            0
        );
        assert_eq!(loaded.memory.read_u8(out), Some(expected));
        assert_eq!(loaded.memory.read_u8(out + 1), Some(0xa5));
    }
    assert_eq!(
        call_movie_toolbox(
            &mut loaded,
            "MediaSetSoundBalance",
            &[handler, (-100i32) as u32]
        ),
        0
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MediaGetSoundBalance", &[handler, out]),
        0
    );
    assert_eq!(loaded.memory.read_u16_be(out), Some((-100i16) as u16));
    assert_eq!(
        call_movie_toolbox(
            &mut loaded,
            "MediaHasCharacteristic",
            &[handler, u32::from_be_bytes(*b"ears"), 0]
        ) as i32,
        -50
    );
}

#[test]
fn quicktime_toolbox_matrices_transform_and_invert_rectangles() {
    let mut loaded = movie_toolbox_fixture();
    let base = PPC_DATA_BASE + 0x4000;
    let (matrix, inverse, src, dst, corners) = (base, base + 40, base + 80, base + 88, base + 100);
    ppc_write_rect(&mut loaded.memory, src, 0, 0, 10, 20).unwrap();
    ppc_write_rect(&mut loaded.memory, dst, 10, 30, 30, 90).unwrap();
    call_movie_toolbox(&mut loaded, "RectMatrix", &[matrix, src, dst]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "TransformRect", &[matrix, src, corners]),
        1
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, src),
        Some((10, 30, 30, 90))
    );
    assert_eq!(loaded.memory.read_u32_be(corners), Some(30 << 16));
    assert_eq!(loaded.memory.read_u32_be(corners + 4), Some(10 << 16));
    assert_eq!(
        call_movie_toolbox(&mut loaded, "InverseMatrix", &[matrix, inverse]),
        1
    );
    call_movie_toolbox(&mut loaded, "TransformRect", &[inverse, src, 0]);
    assert_eq!(ppc_read_rect(&mut loaded.memory, src), Some((0, 0, 10, 20)));
    loaded.memory.write_bytes(matrix, &[0; 36]).unwrap();
    loaded.memory.write_bytes(inverse, &[0xa5; 36]).unwrap();
    assert_eq!(
        call_movie_toolbox(&mut loaded, "InverseMatrix", &[matrix, inverse]),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(inverse), Some(0xa5a5a5a5));
}

#[test]
fn quicktime_toolbox_time_records_preserve_signed_values_and_timebase() {
    let mut loaded = movie_toolbox_fixture();
    let record = PPC_DATA_BASE + 0x4000;
    for value in [-9_000_000_001i64, -3, 3, 9_000_000_001] {
        loaded.memory.write_u64_be(record, value as u64).unwrap();
        loaded.memory.write_u32_be(record + 8, 3).unwrap();
        loaded.memory.write_u32_be(record + 12, 0x12345678).unwrap();
        call_movie_toolbox(&mut loaded, "ConvertTimeScale", &[record, 600]);
        assert_eq!(
            loaded.memory.read_u64_be(record),
            Some((value * 200) as u64)
        );
        assert_eq!(loaded.memory.read_u32_be(record + 8), Some(600));
        assert_eq!(loaded.memory.read_u32_be(record + 12), Some(0x12345678));
    }
}

#[test]
fn quicktime_toolbox_controller_visibility_attachment_and_clip_ownership() {
    let mut loaded = movie_toolbox_fixture();
    let base = PPC_DATA_BASE + 0x4000;
    ppc_write_rect(&mut loaded.memory, base, 0, 0, 240, 320).unwrap();
    let controller =
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, base, 0]);
    assert_ne!(controller, 0);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCGetVisible", &[controller]),
        1
    );
    call_movie_toolbox(&mut loaded, "MCSetVisible", &[controller, 0]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCGetVisible", &[controller]),
        0
    );
    call_movie_toolbox(&mut loaded, "MCSetControllerAttached", &[controller, 0]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCIsControllerAttached", &[controller]),
        0
    );
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRgn);
    let region = loaded.cpu.gpr[3];
    let data = loaded.memory.read_u32_be(region).unwrap();
    ppc_write_rect(&mut loaded.memory, data + 2, 10, 20, 30, 40).unwrap();
    call_movie_toolbox(&mut loaded, "MCSetClip", &[controller, region, 0]);
    ppc_write_rect(&mut loaded.memory, data + 2, 0, 0, 1, 1).unwrap();
    assert_eq!(
        call_movie_toolbox(
            &mut loaded,
            "MCGetClip",
            &[controller, base + 40, base + 44]
        ),
        0
    );
    let copied = loaded.memory.read_u32_be(base + 40).unwrap();
    assert_ne!(copied, region);
    let copied_data = loaded.memory.read_u32_be(copied).unwrap();
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, copied_data + 2),
        Some((10, 20, 30, 40))
    );
    assert_eq!(loaded.memory.read_u32_be(base + 44), Some(0));
}

#[test]
fn quicktime_toolbox_sample_sizes_and_interesting_times_follow_track_tables() {
    let mut loaded = movie_toolbox_fixture();
    let base = PPC_DATA_BASE + 0x4000;
    // Exclude the audio track: the fixture has two video samples, each
    // four bytes and one second long at a movie time scale of 60.
    loaded.quicktime.movie_tracks[1].enabled = false;
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieDataSize", &[PPC_QT_MOVIE, 0, 120]),
        8
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieDataSize", &[PPC_QT_MOVIE, 60, 60]),
        4
    );
    loaded
        .memory
        .write_u32_be(base, u32::from_be_bytes(*b"vide"))
        .unwrap();
    for (flags, time, rate, expected, duration) in [
        (1, 0, 0x10000, 60i32, 60),
        (1 | 0x4000, 0, 0x10000, 0, 60),
        (1, 60, 0xffff0000, 0, 60),
        (1, 60, 0x10000, -1, 0),
    ] {
        call_movie_toolbox(
            &mut loaded,
            "GetMovieNextInterestingTime",
            &[PPC_QT_MOVIE, flags, 1, base, time, rate, base + 4, base + 8],
        );
        assert_eq!(loaded.memory.read_u32_be(base + 4), Some(expected as u32));
        assert_eq!(loaded.memory.read_u32_be(base + 8), Some(duration));
    }
}

#[test]
fn quicktime_toolbox_display_regions_preserve_gaps_between_tracks() {
    let mut loaded = movie_toolbox_fixture();
    let mut second = loaded.quicktime.movie_tracks[0];
    second.handle += 0x100;
    second.matrix[6] = 400 << 16;
    loaded.quicktime.movie_tracks.push(second);
    let region = call_movie_toolbox(&mut loaded, "GetMovieDisplayBoundsRgn", &[PPC_QT_MOVIE]);
    let storage = ppc_region_storage(&mut loaded.memory, region).unwrap();
    assert!(ppc_point_in_region_storage(&storage, 10, 10));
    assert!(!ppc_point_in_region_storage(&storage, 10, 350));
    assert!(ppc_point_in_region_storage(&storage, 10, 410));
}

#[test]
fn quicktime_toolbox_set_movie_box_updates_an_explicit_matrix() {
    let mut loaded = movie_toolbox_fixture();
    let base = PPC_DATA_BASE + 0x4000;
    call_movie_toolbox(&mut loaded, "SetIdentityMatrix", &[base]);
    loaded.memory.write_u32_be(base + 24, 10 << 16).unwrap();
    call_movie_toolbox(&mut loaded, "SetMovieMatrix", &[PPC_QT_MOVIE, base]);
    assert_eq!(loaded.quicktime.movie_box, (0, 10, 240, 330));
    ppc_write_rect(&mut loaded.memory, base + 40, 20, 30, 500, 670).unwrap();
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = base + 40;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::QtSetMovieBox);
    assert_eq!(loaded.quicktime.movie_error, 0);
    call_movie_toolbox(&mut loaded, "GetMovieMatrix", &[PPC_QT_MOVIE, base]);
    assert_eq!(loaded.memory.read_u32_be(base), Some(2 << 16));
    assert_eq!(loaded.memory.read_u32_be(base + 16), Some(2 << 16));
    assert_eq!(loaded.memory.read_u32_be(base + 24), Some(30 << 16));
    assert_eq!(loaded.memory.read_u32_be(base + 28), Some(20 << 16));
}

#[test]
fn quicktime_toolbox_preferred_rate_does_not_change_current_playback_rate() {
    let mut loaded = movie_toolbox_fixture();
    call_movie_toolbox(
        &mut loaded,
        "SetMoviePreferredRate",
        &[PPC_QT_MOVIE, 0x20000],
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::QtStartMovie);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieRate", &[PPC_QT_MOVIE]),
        0x20000
    );
    call_movie_toolbox(
        &mut loaded,
        "SetMoviePreferredRate",
        &[PPC_QT_MOVIE, 0x10000],
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieRate", &[PPC_QT_MOVIE]),
        0x20000
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::QtStopMovie);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieRate", &[PPC_QT_MOVIE]),
        0
    );
}

#[test]
fn quicktime_toolbox_movies_task_obeys_fractional_reverse_and_stopped_rates() {
    let mut loaded = movie_toolbox_fixture();
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_tasks_until_done = 120;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_file_duration = 120;
    let task = |loaded: &mut PpcLoadedApp| {
        advance_quicktime_test_clock(loaded, 1_000_000);
        loaded.cpu.gpr[3] = PPC_QT_MOVIE;
        run_test_import(loaded, PpcImportDispatcherTarget::QtMoviesTask);
    };
    task(&mut loaded);
    assert_eq!(loaded.quicktime.movie_task_count, 0);
    loaded.quicktime.movie_started = true;
    loaded.quicktime.movie_playback_rate = 0x8000;
    task(&mut loaded);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
        5
    );
    task(&mut loaded);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
        10
    );
    loaded.quicktime.movie_playback_rate = 0x20000;
    task(&mut loaded);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
        30
    );
    loaded.quicktime.movie_playback_rate = 0;
    task(&mut loaded);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
        30
    );
    loaded.quicktime.movie_playback_rate = -0x8000;
    task(&mut loaded);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
        25
    );
    for _ in 0..5 {
        task(&mut loaded);
    }
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
        0
    );
    assert!(!loaded.quicktime.movie_started);
    assert!(loaded.quicktime.movie_at_beginning);
    task(&mut loaded);
    assert_eq!(loaded.quicktime.movie_task_count, 0);
}

#[test]
fn movie_palindrome_audio_follows_direction_and_bounds() {
    let mut loaded = movie_toolbox_fixture();
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_creation_time_scale = 600;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_file_duration = 600;
    loaded.quicktime.movie_tasks_until_done = 120;
    loaded.quicktime.movie_file_data = vec![0, 0, 0x40, 0, 0xC0, 0, 0x10, 0, 0x20, 0, 0x30, 0];
    loaded.quicktime.movie_audio_track = Some(PpcQuickTimeAudioTrackRecord {
        media_time_scale: 6,
        media_duration: 6,
        sample_count: 6,
        first_sample_duration: 1,
        first_sample_size: 2,
        first_chunk_offset: 0,
        first_sample_data_len: 2,
        first_sample_checksum: 0,
        first_sample_preview_len: 0,
        first_sample_preview: [0; 16],
        first_samples_per_chunk: 6,
        sample_description_id: 1,
        codec: u32::from_be_bytes(*b"twos"),
        channel_count: 1,
        sample_size_bits: 16,
        sample_rate_fixed: 6 << 16,
    });
    let base = call_movie_toolbox(&mut loaded, "GetMovieTimeBase", &[PPC_QT_MOVIE]);
    let record = PPC_STACK_TOP - 64;
    loaded.memory.write_u64_be(record, 300).unwrap();
    loaded.memory.write_u32_be(record + 8, 600).unwrap();
    loaded.memory.write_u32_be(record + 12, 0).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[base, record]);
    call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[base, 3]);
    call_movie_toolbox(&mut loaded, "SetMovieRate", &[PPC_QT_MOVIE, 0x10000]);
    assert_eq!(
        loaded.sound.decoded_file_playbacks.last().unwrap().samples,
        [0x80, 0xC0, 0x40]
    );
    loaded.quicktime.movie_clock_subtick = None;
    ppc_qt_service_clock(&mut loaded.quicktime, &mut loaded.sound, 0);
    ppc_qt_service_clock(&mut loaded.quicktime, &mut loaded.sound, 30_000_000);
    assert_eq!(
        loaded.sound.decoded_file_playbacks.last().unwrap().samples,
        [0x40, 0xC0, 0x80]
    );
    ppc_qt_service_clock(&mut loaded.quicktime, &mut loaded.sound, 60_000_000);
    assert_eq!(
        loaded.sound.decoded_file_playbacks.last().unwrap().samples,
        [0x80, 0xC0, 0x40]
    );
    loaded.memory.write_u64_be(record, 600).unwrap();
    call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[base, record]);
    assert_eq!(
        loaded.sound.decoded_file_playbacks.last().unwrap().samples,
        [0x80, 0xC0, 0x40, 0x90, 0xA0, 0xB0]
    );
}

#[test]
fn quicktime_toolbox_set_movie_rate_seeks_audio_and_preserves_preferred_rate() {
    let mut loaded = movie_toolbox_fixture();
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_creation_time_scale = 600;
    loaded.quicktime.movie_time_scale = 600;
    loaded.quicktime.movie_file_duration = 600;
    loaded.quicktime.movie_tasks_until_done = 120;
    loaded.quicktime.movie_task_count = 60;
    loaded.quicktime.movie_preferred_rate = 0x8000;
    let mut data = vec![0; 12];
    data.extend_from_slice(&[0, 0, 0x40, 0, 0xC0, 0, 0x10, 0, 0x20, 0, 0x30, 0]);
    loaded.quicktime.movie_file_data = data;
    loaded.quicktime.movie_audio_track = Some(PpcQuickTimeAudioTrackRecord {
        media_time_scale: 6,
        media_duration: 6,
        sample_count: 6,
        first_sample_duration: 1,
        first_sample_size: 1,
        first_chunk_offset: 12,
        first_sample_data_len: 1,
        first_sample_checksum: 0,
        first_sample_preview_len: 0,
        first_sample_preview: [0; 16],
        first_samples_per_chunk: 6,
        sample_description_id: 1,
        codec: u32::from_be_bytes(*b"twos"),
        channel_count: 1,
        sample_size_bits: 16,
        sample_rate_fixed: 6 << 16,
    });
    call_movie_toolbox(&mut loaded, "SetMovieRate", &[PPC_QT_MOVIE, 0x20000]);
    assert_eq!(loaded.quicktime.movie_error, 0);
    assert_eq!(loaded.quicktime.movie_preferred_rate, 0x8000);
    assert_eq!(loaded.quicktime.movie_task_count, 60);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieRate", &[PPC_QT_MOVIE]),
        0x20000
    );
    let audio = loaded.sound.decoded_file_playbacks.last().unwrap();
    assert_eq!(audio.samples, [0x90, 0xA0, 0xB0]);
    assert_eq!(audio.sample_rate_fixed, 12 << 16);

    call_movie_toolbox(
        &mut loaded,
        "SetMovieRate",
        &[PPC_QT_MOVIE, (-0x10000i32) as u32],
    );
    let audio = loaded.sound.decoded_file_playbacks.last().unwrap();
    assert_eq!(audio.samples, [0x40, 0xC0, 0x80]);
    assert_eq!(audio.sample_rate_fixed, 6 << 16);
    call_movie_toolbox(&mut loaded, "SetMovieRate", &[PPC_QT_MOVIE, 0]);
    assert!(!loaded.quicktime.movie_started);
    assert_eq!(loaded.quicktime.movie_task_count, 60);
    assert_eq!(
        loaded.sound.manager.file_playback_paused(PPC_QT_MOVIE),
        None
    );
    call_movie_toolbox(&mut loaded, "SetMovieRate", &[0, 0x10000]);
    assert_eq!(loaded.quicktime.movie_error, -2010);
    assert!(!loaded.quicktime.movie_started);
}

#[test]
fn quicktime_toolbox_clock_progress_is_independent_of_poll_frequency() {
    let state = PpcQuickTimeState {
        movie_started: true,
        movie_active: true,
        movie_file_duration: 6000,
        movie_creation_time_scale: 600,
        movie_time_scale: 600,
        movie_tasks_until_done: 600,
        movie_playback_rate: 0x8000,
        ..PpcQuickTimeState::default()
    };
    let mut sparse = state.clone();
    let mut frequent = state;
    let mut sound = PpcSoundState::default();
    ppc_qt_service_clock(&mut sparse, &mut sound, 0);
    ppc_qt_service_clock(&mut frequent, &mut sound, 0);
    ppc_qt_service_clock(&mut sparse, &mut sound, 60_000_000);
    for tick in 1..=1000 {
        ppc_qt_service_clock(&mut frequent, &mut sound, tick * 60_000);
    }
    assert_eq!(dispatch_quicktime::quicktime_callback_time(&sparse), 300);
    assert_eq!(dispatch_quicktime::quicktime_callback_time(&frequent), 300);
    assert_eq!(frequent.movie_task_fraction, sparse.movie_task_fraction);
    // Re-reading or temporarily regressing the clock cannot advance twice.
    ppc_qt_service_clock(&mut frequent, &mut sound, 59_000_000);
    ppc_qt_service_clock(&mut frequent, &mut sound, 60_000_000);
    assert_eq!(dispatch_quicktime::quicktime_callback_time(&frequent), 300);
    frequent.movie_playback_rate = -0x10000;
    ppc_qt_service_clock(&mut frequent, &mut sound, 90_000_000);
    assert_eq!(dispatch_quicktime::quicktime_callback_time(&frequent), 0);
    assert!(!frequent.movie_started);
    assert!(frequent.movie_at_beginning);
}

#[test]
fn quicktime_toolbox_time_queries_follow_guest_clock_without_movies_task() {
    let mut loaded = movie_toolbox_fixture();
    loaded.quicktime.movie_active = true;
    loaded.quicktime.movie_tasks_until_done = 120;
    loaded.set_tick_count(0);
    call_movie_toolbox(&mut loaded, "SetMovieRate", &[PPC_QT_MOVIE, 0x10000]);
    let baseline = loaded.quicktime.movie_clock_subtick.unwrap();
    loaded
        .callback_scheduling
        .set_current_subtick(baseline + 30_000_000);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
        30
    );
    for _ in 0..4 {
        loaded.cpu.gpr[3] = PPC_QT_MOVIE;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QtMoviesTask);
    }
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
        30
    );
    call_movie_toolbox(&mut loaded, "SetMovieRate", &[PPC_QT_MOVIE, 0]);
    loaded
        .callback_scheduling
        .set_current_subtick(baseline + 60_000_000);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
        30
    );
}

// Move time explicitly so service/poll frequency does not determine playback.
fn advance_quicktime_test_clock(loaded: &mut PpcLoadedApp, elapsed: u64) {
    let now = loaded
        .callback_scheduling
        .current_subtick()
        .max(u64::from(loaded.current_tick()) * crate::time_manager::SUBTICKS_PER_TICK)
        .max(loaded.quicktime.movie_clock_subtick.unwrap_or(0));
    loaded.quicktime.movie_clock_subtick.get_or_insert(now);
    loaded
        .callback_scheduling
        .set_current_subtick(now + elapsed);
}

#[test]
fn quicktime_controller_draw_handles_only_its_movie_window() {
    let (mut loaded, gworld, pixels) = cinepak_movie_draw_fixture();
    loaded.quicktime.movie_gworld = gworld;
    let rect = PPC_DATA_BASE + 0x4000;
    loaded.memory.add_region(rect, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect, 0, 0, 4, 4).unwrap();
    let controller =
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, rect, 0]);
    call_movie_toolbox(&mut loaded, "MCSetControllerPort", &[controller, gworld]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDraw", &[controller, gworld + 0x100]),
        0
    );
    for offset in 0..32 {
        assert_eq!(loaded.memory.read_u8(pixels + offset), Some(0xee));
    }
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDraw", &[controller, gworld]),
        1
    );
    assert_eq!(loaded.memory.read_u16_be(pixels), Some(0));
    assert_eq!(loaded.memory.read_u16_be(pixels + 30), Some(0x7fff));

    // An update is still consumed when the movie is completely clipped.
    loaded.quicktime.movie_display_clip = Some(vec![0, 10, 0, 0, 0, 0, 0, 0, 0, 0]);
    loaded.memory.write_bytes(pixels, &[0xee; 32]).unwrap();
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDraw", &[controller, gworld]),
        1
    );
    for offset in 0..32 {
        assert_eq!(loaded.memory.read_u8(pixels + offset), Some(0xee));
    }
}

#[test]
fn quicktime_detached_controller_bounds_do_not_resize_the_movie() {
    let (mut loaded, gworld, pixels) = cinepak_movie_draw_fixture();
    loaded.quicktime.movie_gworld = gworld;
    let rect = PPC_DATA_BASE + 0x4000;
    loaded.memory.add_region(rect, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect, 0, 0, 2, 2).unwrap();
    let controller =
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, rect, 0]);
    call_movie_toolbox(&mut loaded, "MCSetControllerPort", &[controller, gworld]);
    call_movie_toolbox(&mut loaded, "MCSetControllerAttached", &[controller, 0]);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDraw", &[controller, gworld]),
        1
    );
    assert_eq!(
        loaded.memory.read_u16_be(pixels + 4),
        Some(ppc_qt_rgb555_from_u8(85, 85, 85))
    );
    assert_eq!(
        loaded.memory.read_u16_be(pixels + 16),
        Some(ppc_qt_rgb555_from_u8(170, 170, 170))
    );
    assert_eq!(loaded.memory.read_u16_be(pixels + 30), Some(0x7fff));
    assert_eq!(loaded.quicktime.movie_box, (0, 0, 4, 4));
    assert_eq!(loaded.quicktime.movie_controllers[0].rect, (0, 0, 2, 2));
}

#[test]
fn quicktime_mc_set_movie_moves_pixels_and_preserves_detached_controller() {
    let (mut loaded, old_port, old_pixels) = cinepak_movie_draw_fixture();
    let port = old_port + 0x2000;
    let pixels = old_pixels + 0x8000;
    let mut world = loaded.gworlds[0].clone();
    world.port = port;
    world.base_addr = pixels;
    world.width = 8;
    world.height = 8;
    world.row_bytes = 16;
    loaded.gworlds.push(world);
    loaded.memory.add_region(pixels, vec![0xee; 128]);
    let rect = PPC_DATA_BASE + 0x4000;
    loaded.memory.add_region(rect, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect, 0, 0, 4, 4).unwrap();
    let controller =
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, rect, 0]);
    assert_eq!(
        call_movie_toolbox(
            &mut loaded,
            "MCSetMovie",
            &[controller, PPC_QT_MOVIE, port, 0x0002_0003]
        ),
        0
    );
    assert_eq!(loaded.quicktime.movie_gworld, port);
    assert_eq!(loaded.quicktime.movie_box, (2, 3, 6, 7));
    assert_eq!(loaded.quicktime.movie_controllers[0].rect, (2, 3, 6, 7));
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDraw", &[controller, port]),
        1
    );
    assert_eq!(loaded.memory.read_u16_be(pixels + 2 * 16 + 3 * 2), Some(0));
    assert_eq!(
        loaded.memory.read_u16_be(pixels + 5 * 16 + 6 * 2),
        Some(0x7fff)
    );
    for offset in 0..32 {
        assert_eq!(loaded.memory.read_u8(old_pixels + offset), Some(0xee));
    }
    // A second placement uses the same natural source coordinates.
    assert_eq!(
        call_movie_toolbox(
            &mut loaded,
            "MCSetMovie",
            &[controller, PPC_QT_MOVIE, port, 0]
        ),
        0
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDraw", &[controller, port]),
        1
    );
    assert_eq!(loaded.memory.read_u16_be(pixels), Some(0));
    assert_eq!(
        loaded.memory.read_u16_be(pixels + 3 * 16 + 3 * 2),
        Some(0x7fff)
    );
    assert_eq!(loaded.quicktime.movie_file_bounds, Some((0, 0, 4, 4)));

    call_movie_toolbox(&mut loaded, "MCSetControllerAttached", &[controller, 0]);
    loaded.cpu.gpr[3] = old_port;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetGWorld);
    assert_eq!(
        call_movie_toolbox(
            &mut loaded,
            "MCSetMovie",
            &[controller, PPC_QT_MOVIE, 0, 0xfffe_fffd]
        ),
        0
    );
    assert_eq!(loaded.quicktime.movie_gworld, old_port);
    assert_eq!(loaded.quicktime.movie_box, (-2, -3, 2, 1));
    assert_eq!(loaded.quicktime.movie_controllers[0].window, port);
    assert_eq!(loaded.quicktime.movie_controllers[0].rect, (0, 0, 4, 4));
    let matrix = loaded.quicktime.movie_matrix;
    assert_eq!(
        call_movie_toolbox(
            &mut loaded,
            "MCSetMovie",
            &[controller, PPC_QT_MOVIE, port, 0x7fff_7fff]
        ) as i32,
        -50
    );
    assert_eq!(loaded.quicktime.movie_matrix, matrix);
    assert_eq!(loaded.quicktime.movie_gworld, old_port);
    assert_eq!(loaded.quicktime.movie_box, (-2, -3, 2, 1));
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCSetMovie", &[controller, 0, 0, 0]),
        0
    );
    assert_eq!(loaded.quicktime.movie_controllers[0].movie, 0);
    assert_eq!(loaded.quicktime.movie_box, (-2, -3, 2, 1));
}

fn controller_filter_fixture(consume: bool) -> (PpcLoadedApp, u32, u32, u32, u32) {
    let (mut loaded, port, _) = cinepak_movie_draw_fixture();
    loaded.quicktime.movie_gworld = port;
    let data = PPC_DATA_BASE + 0x6000;
    let code = PPC_CODE_BASE + 0x8000;
    loaded.memory.add_region(data, vec![0; 0x1000]);
    ppc_write_rect(&mut loaded.memory, data, 0, 0, 4, 4).unwrap();
    let controller =
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, data, 0]);
    let observed = data + 0x100;
    let vector = data + 0x200;
    // Record mc, action (by value), params and the callback's TOC through refCon.
    loaded.memory.add_region(
        code,
        [
            0x90660000u32,
            0x90860004,
            0x90a60008,
            0x9046000c,
            0x38600000 | u32::from(consume),
            0x4e800020,
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    loaded.memory.write_u32_be(vector, code).unwrap();
    loaded.memory.write_u32_be(vector + 4, 0x11223344).unwrap();
    call_movie_toolbox(
        &mut loaded,
        "MCSetActionFilterWithRefCon",
        &[controller, vector, observed],
    );
    (loaded, controller, code, vector, observed)
}

fn call_controller_with_filter(loaded: &mut PpcLoadedApp, symbol: &str, args: &[u32]) -> u32 {
    loaded.cpu.gpr[3..3 + args.len()].copy_from_slice(args);
    run_quicktime_callback_test_import(
        loaded,
        dispatcher_target_for_import("QuickTimeLib", symbol),
    );
    loaded.cpu.gpr[3]
}

#[test]
fn quicktime_controller_filter_consumes_or_permits_play_with_guest_abi() {
    for consume in [false, true] {
        let (mut loaded, controller, _, _, observed) = controller_filter_fixture(consume);
        let sp = loaded.cpu.gpr[1];
        let rtoc = loaded.cpu.gpr[2];
        assert_eq!(
            call_controller_with_filter(&mut loaded, "MCDoAction", &[controller, 8, 0x10000]),
            0
        );
        assert_eq!(loaded.quicktime.movie_started, !consume);
        assert_eq!(loaded.memory.read_u32_be(observed), Some(controller));
        assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(8));
        assert_eq!(loaded.memory.read_u32_be(observed + 8), Some(0x10000));
        assert_eq!(loaded.memory.read_u32_be(observed + 12), Some(0x11223344));
        assert_eq!(loaded.cpu.gpr[1], sp);
        assert_eq!(loaded.cpu.gpr[2], rtoc);
    }
    let (mut loaded, controller, code, _, observed) = controller_filter_fixture(true);
    // A raw code pointer uses the installer TOC, not a later caller's TOC.
    loaded.cpu.gpr[2] = 0x55667788;
    call_movie_toolbox(
        &mut loaded,
        "MCSetActionFilterWithRefCon",
        &[controller, code, observed],
    );
    loaded.cpu.gpr[2] = 0x12345678;
    assert_eq!(
        call_controller_with_filter(&mut loaded, "MCIdle", &[controller]),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(1));
    assert_eq!(loaded.memory.read_u32_be(observed + 8), Some(0));
    assert_eq!(loaded.memory.read_u32_be(observed + 12), Some(0x55667788));
    assert_eq!(loaded.cpu.gpr[2], 0x12345678);
    let port = loaded.quicktime.movie_gworld;
    assert_eq!(
        call_controller_with_filter(&mut loaded, "MCDraw", &[controller, port]),
        1
    );
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(2));
    assert_eq!(loaded.memory.read_u32_be(observed + 8), Some(port));
    call_movie_toolbox(
        &mut loaded,
        "MCSetActionFilterWithRefCon",
        &[controller, 0, 0],
    );
    assert_eq!(
        call_controller_with_filter(&mut loaded, "MCDoAction", &[controller, 8, 0x10000]),
        0
    );
    assert!(loaded.quicktime.movie_started);
    call_movie_toolbox(
        &mut loaded,
        "MCSetActionFilterWithRefCon",
        &[controller, u32::MAX, 0],
    );
    assert_eq!(
        call_controller_with_filter(&mut loaded, "MCIdle", &[controller]) as i32,
        -50
    );
}

#[test]
fn quicktime_controller_click_filter_receives_event_and_releases_temporary_storage() {
    let (mut loaded, controller, _, vector, observed) = controller_filter_fixture(true);
    let code = PPC_CODE_BASE + 0x9000;
    // Save the temporary pointer, and copy all 16 EventRecord bytes before return.
    loaded.memory.add_region(
        code,
        [
            0x90a60000u32,
            0x80e50000,
            0x90e60004,
            0x80e50004,
            0x90e60008,
            0x80e50008,
            0x90e6000c,
            0x80e5000c,
            0x90e60010,
            0x38600001,
            0x4e800020,
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    loaded.memory.write_u32_be(vector, code).unwrap();
    assert_eq!(
        call_controller_with_filter(
            &mut loaded,
            "MCClick",
            &[controller, 0x02345678, 0xfffe0003, 0x12345678, 0x0201]
        ),
        1
    );
    let event = loaded.memory.read_u32_be(observed).unwrap();
    assert_ne!(event, 0);
    assert_eq!(loaded.memory.read_u16_be(observed + 4), Some(1));
    assert_eq!(loaded.memory.read_u32_be(observed + 6), Some(0x02345678));
    assert_eq!(loaded.memory.read_u32_be(observed + 10), Some(0x12345678));
    assert_eq!(loaded.memory.read_u32_be(observed + 14), Some(0xfffe0003));
    assert_eq!(loaded.memory.read_u16_be(observed + 18), Some(0x0201));
    loaded.cpu.gpr[3] = event;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetPtrSize);
    assert_eq!(
        loaded.cpu.gpr[3], 0,
        "the callback's temporary event is not retained"
    );
}

#[test]
fn quicktime_controller_filter_nests_with_rate_callbacks_at_the_same_import() {
    let (mut loaded, outer, simple_code, outer_vector, observed) = controller_filter_fixture(false);
    let rect = observed - 0x100;
    let inner = call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, rect, 0]);
    let third = call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, rect, 0]);
    let inner_output = observed + 0x300;
    let third_output = observed + 0x400;
    let rate_data = observed + 0x500;
    let outer_code = PPC_CODE_BASE + 0xa000;
    let rate_code = PPC_CODE_BASE + 0xb000;
    let third_code = PPC_CODE_BASE + 0xc000;
    let inner_vector = observed + 0x700;
    let third_vector = observed + 0x710;
    let rate_vector = observed + 0x720;
    let d = |op: u32, reg: u32, base: u32, imm: i16| {
        (op << 26) | (reg << 21) | (base << 16) | u32::from(imm as u16)
    };
    let nested_code = |refcon_register, action, rate_high| {
        [
            d(37, 1, 1, -64),
            xfx_form(31, 0, 8, 339),
            d(36, 0, 1, 68),
            d(36, 31, 1, 60),
            x_form(31, refcon_register, 31, refcon_register, 444, false),
            d(32, 3, 31, 16),
            d(14, 4, 0, action),
            d(15, 5, 0, rate_high),
            d(32, 12, 31, 20),
            xfx_form(31, 12, 9, 467),
            xl_form(19, 20, 0, 528, true),
            d(36, 3, 31, 24),
            d(14, 0, 0, 99),
            d(36, 0, 31, 28),
            d(14, 3, 0, 1),
            d(32, 31, 1, 60),
            d(32, 0, 1, 68),
            xfx_form(31, 0, 8, 467),
            d(14, 1, 1, 64),
            0x4e800020,
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect::<Vec<_>>()
    };
    // Outer filter -> inner controller -> rate callback -> third controller's
    // filter. All three MCDoAction calls deliberately use the same import PC.
    loaded.memory.add_region(outer_code, nested_code(6, 8, 1));
    loaded.memory.add_region(rate_code, nested_code(4, 17, 0));
    loaded.memory.add_region(
        third_code,
        [
            0x90660000u32,
            0x90860004,
            0x90a60008,
            0x38600001,
            0x4e800020,
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    for (vector, code) in [
        (outer_vector, outer_code),
        (inner_vector, simple_code),
        (third_vector, third_code),
        (rate_vector, rate_code),
    ] {
        loaded.memory.write_u32_be(vector, code).unwrap();
        loaded.memory.write_u32_be(vector + 4, 0x11223344).unwrap();
    }
    for (data, controller) in [(observed, inner), (rate_data, third)] {
        loaded.memory.write_u32_be(data + 16, controller).unwrap();
        loaded
            .memory
            .write_u32_be(data + 20, loaded.imports[0].trap_pc)
            .unwrap();
    }
    call_movie_toolbox(
        &mut loaded,
        "MCSetActionFilterWithRefCon",
        &[inner, inner_vector, inner_output],
    );
    call_movie_toolbox(
        &mut loaded,
        "MCSetActionFilterWithRefCon",
        &[third, third_vector, third_output],
    );
    let callback = call_movie_toolbox(&mut loaded, "NewCallBack", &[PPC_QT_MOVIE + 0x10, 2]);
    assert_ne!(callback, 0);
    assert_eq!(
        call_movie_toolbox(
            &mut loaded,
            "CallMeWhen",
            &[callback, rate_vector, rate_data, 0, 0, 0]
        ),
        0
    );
    let sp = loaded.cpu.gpr[1];
    let rtoc = loaded.cpu.gpr[2];
    let r31 = loaded.cpu.gpr[31];
    assert_eq!(
        call_controller_with_filter(&mut loaded, "MCDoAction", &[outer, 8, 0]),
        0
    );
    assert!(
        loaded.quicktime.movie_started,
        "outer filter consumes stop after inner play"
    );
    assert_eq!(loaded.memory.read_u32_be(inner_output), Some(inner));
    assert_eq!(loaded.memory.read_u32_be(inner_output + 4), Some(8));
    assert_eq!(loaded.memory.read_u32_be(inner_output + 8), Some(0x10000));
    assert_eq!(loaded.memory.read_u32_be(third_output), Some(third));
    assert_eq!(loaded.memory.read_u32_be(third_output + 4), Some(17));
    for data in [observed, rate_data] {
        assert_eq!(loaded.memory.read_u32_be(data + 24), Some(0));
        assert_eq!(loaded.memory.read_u32_be(data + 28), Some(99));
    }
    assert_eq!(loaded.cpu.gpr[1], sp);
    assert_eq!(loaded.cpu.gpr[2], rtoc);
    assert_eq!(loaded.cpu.gpr[31], r31);
}

#[test]
fn quicktime_controller_seek_converts_signed_wide_times_and_reports_play_rate() {
    let mut loaded = movie_toolbox_fixture();
    let data = PPC_DATA_BASE + 0x4000;
    ppc_write_rect(&mut loaded.memory, data, 0, 0, 240, 320).unwrap();
    let controller =
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, data, 0]);
    loaded.quicktime.movie_tasks_until_done = 120;
    let record = data + 32;
    let time_base = call_movie_toolbox(&mut loaded, "GetMovieTimeBase", &[PPC_QT_MOVIE]);
    for (value, scale, expected) in [(1000i64, 1000u32, 60), (-1, 60, 0), (i64::MAX, 1, 120)] {
        loaded.memory.write_u64_be(record, value as u64).unwrap();
        loaded.memory.write_u32_be(record + 8, scale).unwrap();
        loaded.memory.write_u32_be(record + 12, time_base).unwrap();
        assert_eq!(
            call_movie_toolbox(&mut loaded, "MCDoAction", &[controller, 12, record]),
            0
        );
        assert_eq!(
            call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
            expected
        );
    }
    for (scale, base) in [(0, 0), (u32::MAX, 0), (60, 123)] {
        loaded.memory.write_u32_be(record + 8, scale).unwrap();
        loaded.memory.write_u32_be(record + 12, base).unwrap();
        assert_eq!(
            call_movie_toolbox(&mut loaded, "MCDoAction", &[controller, 12, record]) as i32,
            -50
        );
        assert_eq!(
            call_movie_toolbox(&mut loaded, "GetMovieTime", &[PPC_QT_MOVIE, 0]),
            120
        );
    }
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDoAction", &[controller, 12, 0xfffffff8]) as i32,
        -50
    );
    loaded.quicktime.movie_playback_rate = -0x8000;
    for playing in [true, false] {
        loaded.quicktime.movie_started = playing;
        assert_eq!(
            call_movie_toolbox(&mut loaded, "MCDoAction", &[controller, 42, record]),
            0
        );
        assert_eq!(
            loaded.memory.read_u32_be(record).unwrap() as i32,
            if playing { -0x8000 } else { 0 }
        );
    }
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDoAction", &[controller, 42, 0]) as i32,
        -50
    );
    loaded.quicktime.movie_disposed = true;
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDoAction", &[controller, 12, record]) as i32,
        -2010
    );
}

#[test]
fn quicktime_controller_seek_delivers_jump_without_spurious_drawing_callback() {
    let mut loaded = movie_toolbox_fixture();
    let data = PPC_DATA_BASE + 0x4000;
    let code = PPC_CODE_BASE + 0x8000;
    loaded.memory.add_region(
        code,
        [0x90640000u32, 0x4e800020]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    loaded.memory.write_u32_be(data + 200, code).unwrap();
    loaded
        .memory
        .write_u32_be(data + 204, loaded.cpu.gpr[2])
        .unwrap();
    ppc_write_rect(&mut loaded.memory, data, 0, 0, 240, 320).unwrap();
    let controller =
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, data, 0]);
    // Flag 1 requests drawing notifications even when MoviesTask draws nothing;
    // a query or explicit seek is not a MoviesTask/drawing request at all.
    call_movie_toolbox(
        &mut loaded,
        "SetMovieDrawingCompleteProc",
        &[PPC_QT_MOVIE, 1, data + 200, data + 224],
    );
    assert!(loaded.quicktime.movie_drawing_complete.is_some());
    let base = call_movie_toolbox(&mut loaded, "GetMovieTimeBase", &[PPC_QT_MOVIE]);
    let callback = call_movie_toolbox(&mut loaded, "NewCallBack", &[base, 3]);
    assert_eq!(
        call_movie_toolbox(
            &mut loaded,
            "CallMeWhen",
            &[callback, data + 200, data + 220, 0, 0, 0]
        ),
        0
    );
    loaded.memory.write_u64_be(data + 32, 60).unwrap();
    loaded.memory.write_u32_be(data + 40, 60).unwrap();
    loaded.memory.write_u32_be(data + 44, base).unwrap();
    assert_eq!(
        call_controller_with_filter(&mut loaded, "MCDoAction", &[controller, 42, data + 64]),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(data + 220), Some(0));
    assert_eq!(loaded.memory.read_u32_be(data + 224), Some(0));
    assert_eq!(
        call_controller_with_filter(&mut loaded, "MCDoAction", &[controller, 12, data + 32]),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(data + 220), Some(callback));
    assert_eq!(loaded.memory.read_u32_be(data + 224), Some(0));
}

#[test]
fn movie_master_palindrome_crossing_should_trigger_slave_callback() {
    for (rate, elapsed) in [
        (0x10000, 120_000_000),
        (-0x10000, 120_000_000),
        (0x10000, 600_000_000),
    ] {
        let mut loaded = movie_toolbox_fixture();
        loaded.quicktime.movie_creation_time_scale = 600;
        loaded.quicktime.movie_time_scale = 600;
        loaded.quicktime.movie_file_duration = 600;
        loaded.quicktime.movie_tasks_until_done = 120;
        let master = call_movie_toolbox(&mut loaded, "GetMovieTimeBase", &[PPC_QT_MOVIE]);
        let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[slave, 0x10000]);
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[slave, master, 0],
        );
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 3]);
        let registration = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [1, 300, 600],
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, slave, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3000, registration));
        loaded.quicktime.movie_started = true;
        loaded.quicktime.movie_active = true;
        loaded.quicktime.movie_playback_rate = rate;
        loaded.quicktime.movie_clock_subtick = None;
        ppc_qt_service_clock(&mut loaded.quicktime, &mut loaded.sound, 0);
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, 0);
        ppc_qt_service_clock(&mut loaded.quicktime, &mut loaded.sound, elapsed);
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, elapsed);
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3000, registration))
        );
    }
}

#[test]
fn movie_master_wrap_preserves_slave_time_and_jump_callbacks() {
    for (rate, elapsed) in [
        (0x10000i32, 120_000_000),
        (-0x10000, 120_000_000),
        (0x10000, 600_000_000),
    ] {
        let mut loaded = movie_toolbox_fixture();
        loaded.quicktime.movie_creation_time_scale = 600;
        loaded.quicktime.movie_time_scale = 600;
        loaded.quicktime.movie_file_duration = 600;
        loaded.quicktime.movie_tasks_until_done = 120;
        let master = call_movie_toolbox(&mut loaded, "GetMovieTimeBase", &[PPC_QT_MOVIE]);
        let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[slave, 0x10000]);
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[slave, master, 0],
        );
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 1]);
        let forward = rate > 0;
        let crossing = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [if forward { 1 } else { 2 }, 300, 600],
        };
        let opposite = quicktime::callbacks::Registration {
            parameters: [if forward { 2 } else { 1 }, 300, 600],
            ..crossing
        };
        let jump = quicktime::callbacks::Registration {
            parameters: [0, 0, 0],
            ..crossing
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, slave, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
        assert!(loaded.quicktime.callbacks.create(0x3010, slave, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3010, opposite));
        assert!(loaded.quicktime.callbacks.create(0x3020, slave, 3));
        assert!(loaded.quicktime.callbacks.arm(0x3020, jump));
        assert!(loaded.quicktime.callbacks.create(0x3030, master, 3));
        assert!(loaded.quicktime.callbacks.arm(0x3030, jump));
        loaded.quicktime.movie_started = true;
        loaded.quicktime.movie_active = true;
        loaded.quicktime.movie_playback_rate = rate;
        loaded.quicktime.movie_clock_subtick = None;
        ppc_qt_service_clock(&mut loaded.quicktime, &mut loaded.sound, 0);
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, 0);
        ppc_qt_service_clock(&mut loaded.quicktime, &mut loaded.sound, elapsed);
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, elapsed);
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3000, crossing))
        );
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3020, jump))
        );
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3030, jump))
        );
        assert_eq!(loaded.quicktime.callbacks.take_ready(), None);
    }
}

#[test]
fn standalone_master_palindrome_crossing_triggers_nested_slave_callback() {
    for nested_loops in [false, true] {
        for split_service in [true, false] {
            let mut loaded = movie_toolbox_fixture();
            let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
            let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
            let nested = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
            let bound = PPC_STACK_TOP - 64;
            loaded.memory.write_u64_be(bound, 0).unwrap();
            loaded.memory.write_u32_be(bound + 8, 1).unwrap();
            loaded.memory.write_u32_be(bound + 12, 0).unwrap();
            call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[master, bound]);
            loaded.memory.write_u64_be(bound, 1).unwrap();
            call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[master, bound]);
            call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 3]);
            for base in [master, slave, nested] {
                call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
            }
            call_movie_toolbox(
                &mut loaded,
                "SetTimeBaseMasterTimeBase",
                &[slave, master, 0],
            );
            call_movie_toolbox(
                &mut loaded,
                "SetTimeBaseMasterTimeBase",
                &[nested, slave, 0],
            );
            if nested_loops {
                loaded.memory.write_u64_be(bound, 0).unwrap();
                loaded.memory.write_u32_be(bound + 8, 4).unwrap();
                call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[nested, bound]);
                loaded.memory.write_u64_be(bound, 1).unwrap();
                call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[nested, bound]);
                call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[nested, 1]);
            }
            let crossing = quicktime::callbacks::Registration {
                procedure: 0x1000,
                rtoc: 0,
                refcon: 0,
                parameters: [1, if nested_loops { 75 } else { 300 }, 600],
            };
            assert!(loaded.quicktime.callbacks.create(0x3010, master, 1));
            assert!(loaded.quicktime.callbacks.arm(0x3010, crossing));
            assert!(loaded.quicktime.callbacks.create(0x3000, nested, 1));
            assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
            let backward = quicktime::callbacks::Registration {
                parameters: [2, if nested_loops { 75 } else { 300 }, 600],
                ..crossing
            };
            assert!(loaded.quicktime.callbacks.create(0x3020, nested, 1));
            assert!(loaded.quicktime.callbacks.arm(0x3020, backward));
            let now = loaded.callback_scheduling.current_subtick();
            quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
            if split_service {
                advance_quicktime_test_clock(&mut loaded, 60_000_000);
                let now = loaded.callback_scheduling.current_subtick();
                quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
            }
            advance_quicktime_test_clock(
                &mut loaded,
                if split_service {
                    60_000_000
                } else {
                    120_000_000
                },
            );
            let now = loaded.callback_scheduling.current_subtick();
            quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
            assert_eq!(
                loaded.quicktime.callbacks.take_ready(),
                Some((0x3010, crossing))
            );
            assert_eq!(
                loaded.quicktime.callbacks.take_ready(),
                Some((0x3000, crossing)),
                "split service: {split_service}"
            );
            assert_eq!(
                loaded.quicktime.callbacks.take_ready(),
                Some((0x3020, backward)),
                "split service: {split_service}"
            );
        }
    }
}

#[test]
fn standalone_palindrome_master_propagates_intermediate_slave_wraps() {
    for split_service in [true, false] {
        let mut loaded = movie_toolbox_fixture();
        let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let nested = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let bound = PPC_STACK_TOP - 64;
        loaded.memory.write_u64_be(bound, 0).unwrap();
        loaded.memory.write_u32_be(bound + 8, 4).unwrap();
        loaded.memory.write_u32_be(bound + 12, 0).unwrap();
        for base in [master, slave] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
        }
        loaded.memory.write_u64_be(bound, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[slave, bound]);
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[master, bound]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 3]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[slave, 1]);
        for base in [master, slave, nested] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
        }
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[slave, master, 0],
        );
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[nested, slave, 0],
        );
        let crossing = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [1, 75, 600],
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, nested, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        for _ in 0..if split_service { 24 } else { 1 } {
            advance_quicktime_test_clock(
                &mut loaded,
                if split_service {
                    5_000_000
                } else {
                    120_000_000
                },
            );
            let now = loaded.callback_scheduling.current_subtick();
            quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        }
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3000, crossing)),
            "split service: {split_service}"
        );
    }
}

#[test]
fn standalone_palindrome_master_propagates_intermediate_slave_turns() {
    for split_service in [true, false] {
        let mut loaded = movie_toolbox_fixture();
        let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let nested = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let bound = PPC_STACK_TOP - 64;
        loaded.memory.write_u64_be(bound, 0).unwrap();
        loaded.memory.write_u32_be(bound + 8, 4).unwrap();
        loaded.memory.write_u32_be(bound + 12, 0).unwrap();
        for base in [master, slave] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
        }
        loaded.memory.write_u64_be(bound, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[slave, bound]);
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[master, bound]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 3]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[slave, 3]);
        for base in [master, slave, nested] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
        }
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[slave, master, 0],
        );
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[nested, slave, 0],
        );
        let crossing = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [1, 75, 600],
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, nested, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        for _ in 0..if split_service { 24 } else { 1 } {
            advance_quicktime_test_clock(
                &mut loaded,
                if split_service {
                    5_000_000
                } else {
                    120_000_000
                },
            );
            let now = loaded.callback_scheduling.current_subtick();
            quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        }
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3000, crossing)),
            "split service: {split_service}"
        );
    }
}

#[test]
fn standalone_palindrome_master_propagates_long_interval() {
    for split_service in [true, false] {
        let mut loaded = movie_toolbox_fixture();
        let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let nested = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let bound = PPC_STACK_TOP - 64;
        loaded.memory.write_u64_be(bound, 0).unwrap();
        loaded.memory.write_u32_be(bound + 8, 4).unwrap();
        loaded.memory.write_u32_be(bound + 12, 0).unwrap();
        for base in [master, slave] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
        }
        loaded.memory.write_u64_be(bound, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[slave, bound]);
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[master, bound]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 3]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[slave, 1]);
        for base in [master, slave, nested] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
        }
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[slave, master, 0],
        );
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[nested, slave, 0],
        );
        let crossing = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [1, 75, 600],
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, nested, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        for _ in 0..if split_service { 24 } else { 1 } {
            advance_quicktime_test_clock(
                &mut loaded,
                if split_service {
                    5_000_000
                } else {
                    1_200_000_000_000
                },
            );
            let now = loaded.callback_scheduling.current_subtick();
            quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        }
        advance_quicktime_test_clock(&mut loaded, 10_000_000);
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3000, crossing)),
            "split service: {split_service}"
        );
        for base in [master, slave, nested] {
            assert_eq!(
                call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 600, 0]),
                100
            );
        }
    }
}

#[test]
fn standalone_time_base_service_handles_multiple_looping_roots() {
    let mut loaded = movie_toolbox_fixture();
    let width = 60 * crate::time_manager::SUBTICKS_PER_TICK as i128 * 65536;
    let base = quicktime::timebase::TimeBase {
        value: 0,
        source: 0,
        remainder: 0,
        rate: 0x10000,
        start: Some(0),
        stop: Some(width),
        flags: 1,
        master: None,
    };
    let registration = quicktime::callbacks::Registration {
        procedure: 0x1000,
        rtoc: 0,
        refcon: 0,
        parameters: [1, 300, 600],
    };
    for root in [0x4000_0000, 0x5000_0000] {
        loaded.quicktime.time_bases.records.insert(root, base);
        loaded.quicktime.time_bases.records.insert(
            root + 1,
            quicktime::timebase::TimeBase {
                start: None,
                stop: None,
                flags: 0,
                master: Some(root),
                ..base
            },
        );
        assert!(loaded.quicktime.callbacks.create(root + 2, root + 1, 1));
        assert!(loaded.quicktime.callbacks.arm(root + 2, registration));
    }
    for id in 0x6000_0000..0x6000_1000 {
        loaded.quicktime.time_bases.records.insert(
            id,
            quicktime::timebase::TimeBase {
                start: None,
                stop: None,
                flags: 0,
                ..base
            },
        );
    }
    quicktime::ppc_qt_service_time_bases(
        &mut loaded.quicktime,
        60 * crate::time_manager::SUBTICKS_PER_TICK as u64,
    );
    let mut ready = [
        loaded.quicktime.callbacks.take_ready().unwrap().0,
        loaded.quicktime.callbacks.take_ready().unwrap().0,
    ];
    ready.sort();
    assert_eq!(ready, [0x4000_0002, 0x5000_0002]);
    assert!(loaded.quicktime.callbacks.take_ready().is_none());
    assert_eq!(
        loaded.quicktime.time_bases.records[&0x6000_0fff].value,
        width
    );
}

#[test]
fn standalone_loop_service_handles_deep_master_chain() {
    let mut loaded = movie_toolbox_fixture();
    let root = 0x4000_0000;
    let width = 60 * crate::time_manager::SUBTICKS_PER_TICK as i128 * 65536;
    let base = quicktime::timebase::TimeBase {
        value: 0,
        source: 0,
        remainder: 0,
        rate: 0x10000,
        start: Some(0),
        stop: Some(width),
        flags: 1,
        master: None,
    };
    loaded.quicktime.time_bases.records.insert(root, base);
    for index in 1..=10_000u32 {
        loaded.quicktime.time_bases.records.insert(
            root + index,
            quicktime::timebase::TimeBase {
                start: None,
                stop: None,
                flags: 0,
                master: Some(root + index - 1),
                ..base
            },
        );
    }
    let callback = quicktime::callbacks::Registration {
        procedure: 0x1000,
        rtoc: 0,
        refcon: 0,
        parameters: [1, 150, 600],
    };
    assert!(loaded.quicktime.callbacks.create(0x3000, root + 10_000, 1));
    assert!(loaded.quicktime.callbacks.arm(0x3000, callback));
    quicktime::ppc_qt_service_time_bases(
        &mut loaded.quicktime,
        30 * crate::time_manager::SUBTICKS_PER_TICK as u64,
    );
    assert_eq!(
        loaded.quicktime.callbacks.take_ready(),
        Some((0x3000, callback))
    );
    assert_eq!(
        loaded.quicktime.time_bases.records[&(root + 10_000)].value,
        width / 2
    );
}

#[test]
fn standalone_loop_master_delivers_callbacks_for_wide_tree() {
    let mut loaded = movie_toolbox_fixture();
    let master = 0x4000_0000;
    let width = 60 * crate::time_manager::SUBTICKS_PER_TICK as i128 * 65536;
    let base = quicktime::timebase::TimeBase {
        value: 0,
        source: 0,
        remainder: 0,
        rate: 0x10000,
        start: Some(0),
        stop: Some(width),
        flags: 1,
        master: None,
    };
    loaded.quicktime.time_bases.records.insert(master, base);
    for child in 1..=4100u32 {
        loaded.quicktime.time_bases.records.insert(
            master + child,
            quicktime::timebase::TimeBase {
                start: None,
                stop: None,
                flags: 0,
                master: Some(master),
                ..base
            },
        );
    }
    let crossing = quicktime::callbacks::Registration {
        procedure: 0x1000,
        rtoc: 0,
        refcon: 0,
        parameters: [1, 300, 600],
    };
    assert!(loaded.quicktime.callbacks.create(0x3000, master + 1, 1));
    assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
    quicktime::ppc_qt_service_time_bases(
        &mut loaded.quicktime,
        60 * crate::time_manager::SUBTICKS_PER_TICK as u64,
    );
    assert_eq!(
        loaded.quicktime.callbacks.take_ready(),
        Some((0x3000, crossing))
    );
}

#[test]
fn standalone_loop_master_delivers_callbacks_through_wide_looping_branches() {
    let mut loaded = movie_toolbox_fixture();
    let master = 0x4000_0000;
    let width = 60 * crate::time_manager::SUBTICKS_PER_TICK as i128 * 65536;
    let base = quicktime::timebase::TimeBase {
        value: 0,
        source: 0,
        remainder: 0,
        rate: 0x10000,
        start: Some(0),
        stop: Some(width),
        flags: 1,
        master: None,
    };
    loaded.quicktime.time_bases.records.insert(master, base);
    for child in 1..=2100u32 {
        let branch = master + child;
        loaded.quicktime.time_bases.records.insert(
            branch,
            quicktime::timebase::TimeBase {
                start: Some(0),
                stop: Some(width / 2),
                flags: 1,
                master: Some(master),
                ..base
            },
        );
        loaded.quicktime.time_bases.records.insert(
            master + 10_000 + child,
            quicktime::timebase::TimeBase {
                start: None,
                stop: None,
                flags: 0,
                master: Some(branch),
                ..base
            },
        );
    }
    let crossing = quicktime::callbacks::Registration {
        procedure: 0x1000,
        rtoc: 0,
        refcon: 0,
        parameters: [1, 150, 600],
    };
    assert!(loaded
        .quicktime
        .callbacks
        .create(0x3000, master + 10_001, 1));
    assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
    quicktime::ppc_qt_service_time_bases(
        &mut loaded.quicktime,
        60 * crate::time_manager::SUBTICKS_PER_TICK as u64,
    );
    assert_eq!(
        loaded.quicktime.callbacks.take_ready(),
        Some((0x3000, crossing))
    );
}

#[test]
fn standalone_palindrome_master_propagates_long_interval_fractional_rate() {
    for split_service in [true, false] {
        let mut loaded = movie_toolbox_fixture();
        let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let nested = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let bound = PPC_STACK_TOP - 64;
        loaded.memory.write_u64_be(bound, 0).unwrap();
        loaded.memory.write_u32_be(bound + 8, 4).unwrap();
        loaded.memory.write_u32_be(bound + 12, 0).unwrap();
        for base in [master, slave] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
        }
        loaded.memory.write_u64_be(bound, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[slave, bound]);
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[master, bound]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 3]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[slave, 1]);
        for base in [master, slave, nested] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
        }
        call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[slave, 0x8000]);
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[slave, master, 0],
        );
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[nested, slave, 0],
        );
        let crossing = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [1, 75, 600],
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, nested, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        for _ in 0..if split_service { 24 } else { 1 } {
            advance_quicktime_test_clock(
                &mut loaded,
                if split_service {
                    5_000_000
                } else {
                    1_200_000_000_000
                },
            );
            let now = loaded.callback_scheduling.current_subtick();
            quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        }
        advance_quicktime_test_clock(&mut loaded, 10_000_000);
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3000, crossing)),
            "split service: {split_service}"
        );
        for (base, expected) in [(master, 100), (slave, 50), (nested, 50)] {
            assert_eq!(
                call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 600, 0]),
                expected
            );
        }
    }
}

#[test]
fn standalone_palindrome_master_propagates_long_interval_reverse() {
    for split_service in [true, false] {
        let mut loaded = movie_toolbox_fixture();
        let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let nested = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let bound = PPC_STACK_TOP - 64;
        loaded.memory.write_u64_be(bound, 0).unwrap();
        loaded.memory.write_u32_be(bound + 8, 4).unwrap();
        loaded.memory.write_u32_be(bound + 12, 0).unwrap();
        for base in [master, slave] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
        }
        loaded.memory.write_u64_be(bound, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[slave, bound]);
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[master, bound]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 3]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[slave, 1]);
        for base in [master, slave, nested] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
        }
        call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[slave, 0x8000]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[master, 0xffff0000]);
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[slave, master, 0],
        );
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[nested, slave, 0],
        );
        let crossing = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [1, 75, 600],
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, nested, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        for _ in 0..if split_service { 24 } else { 1 } {
            advance_quicktime_test_clock(
                &mut loaded,
                if split_service {
                    5_000_000
                } else {
                    1_200_000_000_000
                },
            );
            let now = loaded.callback_scheduling.current_subtick();
            quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        }
        advance_quicktime_test_clock(&mut loaded, 10_000_000);
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3000, crossing)),
            "split service: {split_service}"
        );
        for (base, expected) in [(master, 100), (slave, 50), (nested, 50)] {
            assert_eq!(
                call_movie_toolbox(&mut loaded, "GetTimeBaseTime", &[base, 600, 0]),
                expected
            );
        }
    }
}

#[test]
fn standalone_reverse_loop_master_propagates_slave_crossing() {
    for split_service in [true, false] {
        let mut loaded = movie_toolbox_fixture();
        let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let nested = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let bound = PPC_STACK_TOP - 64;
        loaded.memory.write_u64_be(bound, 0).unwrap();
        loaded.memory.write_u32_be(bound + 8, 4).unwrap();
        loaded.memory.write_u32_be(bound + 12, 0).unwrap();
        for base in [master, slave] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
        }
        loaded.memory.write_u64_be(bound, 1).unwrap();
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[slave, bound]);
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[master, bound]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 1]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[slave, 0]);
        for base in [master, slave, nested] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
        }
        call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[master, 0xffff0000]);
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[slave, master, 0],
        );
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[nested, slave, 0],
        );
        let crossing = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [2, 300, 600],
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, nested, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        for _ in 0..if split_service { 12 } else { 1 } {
            advance_quicktime_test_clock(
                &mut loaded,
                if split_service { 5_000_000 } else { 60_000_000 },
            );
            let now = loaded.callback_scheduling.current_subtick();
            quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        }
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3000, crossing)),
            "split service: {split_service}"
        );
    }
}

#[test]
fn standalone_reverse_loop_master_propagates_initial_jump() {
    for split_service in [true, false] {
        let mut loaded = movie_toolbox_fixture();
        let master = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let slave = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let nested = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let bound = PPC_STACK_TOP - 64;
        loaded.memory.write_u64_be(bound, 0).unwrap();
        loaded.memory.write_u32_be(bound + 8, 4).unwrap();
        loaded.memory.write_u32_be(bound + 12, 0).unwrap();
        for base in [master, slave] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
        }
        loaded.memory.write_u64_be(bound, 1).unwrap();
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[slave, bound]);
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[master, bound]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[master, 1]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[slave, 0]);
        for base in [master, slave, nested] {
            call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, 0x10000]);
        }
        call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[master, 0xffff0000]);
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[slave, master, 0],
        );
        call_movie_toolbox(
            &mut loaded,
            "SetTimeBaseMasterTimeBase",
            &[nested, slave, 0],
        );
        let crossing = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [1, 300, 600],
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, nested, 1));
        assert!(loaded.quicktime.callbacks.arm(0x3000, crossing));
        let jump = quicktime::callbacks::Registration {
            parameters: [0, 0, 0],
            ..crossing
        };
        assert!(loaded.quicktime.callbacks.create(0x3020, master, 3));
        assert!(loaded.quicktime.callbacks.arm(0x3020, jump));
        assert!(loaded.quicktime.callbacks.create(0x3010, nested, 3));
        assert!(loaded.quicktime.callbacks.arm(0x3010, jump));
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        for _ in 0..if split_service { 6 } else { 1 } {
            advance_quicktime_test_clock(
                &mut loaded,
                if split_service { 1_000_000 } else { 6_000_000 },
            );
            let now = loaded.callback_scheduling.current_subtick();
            quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        }
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3020, jump)),
            "split service: {split_service}"
        );
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3010, jump))
        );
        assert_eq!(loaded.quicktime.callbacks.take_ready(), None);
    }
}

#[test]
fn standalone_loop_without_slave_signals_wrap_jump() {
    for rate in [0x10000, 0xffff0000] {
        let mut loaded = movie_toolbox_fixture();
        let base = call_movie_toolbox(&mut loaded, "NewTimeBase", &[]);
        let bound = PPC_STACK_TOP - 64;
        loaded.memory.write_u64_be(bound, 0).unwrap();
        loaded.memory.write_u32_be(bound + 8, 1).unwrap();
        loaded.memory.write_u32_be(bound + 12, 0).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStartTime", &[base, bound]);
        loaded.memory.write_u64_be(bound, 1).unwrap();
        call_movie_toolbox(&mut loaded, "SetTimeBaseStopTime", &[base, bound]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseFlags", &[base, 1]);
        call_movie_toolbox(&mut loaded, "SetTimeBaseRate", &[base, rate]);
        let jump = quicktime::callbacks::Registration {
            procedure: 0x1000,
            rtoc: 0,
            refcon: 0,
            parameters: [0, 0, 0],
        };
        assert!(loaded.quicktime.callbacks.create(0x3000, base, 3));
        assert!(loaded.quicktime.callbacks.arm(0x3000, jump));
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        advance_quicktime_test_clock(&mut loaded, 60_000_000);
        let now = loaded.callback_scheduling.current_subtick();
        quicktime::ppc_qt_service_time_bases(&mut loaded.quicktime, now);
        assert_eq!(
            loaded.quicktime.callbacks.take_ready(),
            Some((0x3000, jump))
        );
    }
}
