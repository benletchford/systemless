use super::*;

#[test]
fn trunc_string_uses_current_font_and_preserves_pascal_storage() {
    for (size, style) in [(12, 0), (18, 1)] {
        for middle in [false, true] {
            let mut loaded =
                load_pef_application(&synthetic_pef_with_import(b"TruncString")).unwrap();
            let text = PPC_DATA_BASE + 0x1000;
            let original = b"Wide letters and a pathname";
            loaded.memory.add_region(text, vec![0xa5; 258]);
            assert!(ppc_write_pstring_bytes(&mut loaded.memory, text, original));
            loaded.quickdraw_text_size = size;
            loaded
                .memory
                .write_u8(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_TX_FACE_OFFSET, style)
                .unwrap();
            let font = ppc_current_text_font(&mut loaded.memory, PPC_MAIN_GWORLD);
            let width = ppc_text_width_bytes(font, size, style, b"Wide letters");
            loaded.cpu.gpr[3] = width as u32;
            loaded.cpu.gpr[4] = text;
            loaded.cpu.gpr[5] = if middle { 0x4000 } else { 0 };
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 1);
            let result = ppc_read_pstring_bytes(&mut loaded.memory, text).unwrap();
            assert!(ppc_text_width_bytes(font, size, style, &result) <= width);
            assert!(result.contains(&0xc9));
            assert_eq!(result[0], original[0]);
            if middle {
                assert_eq!(result.last(), original.last());
            } else {
                assert_eq!(result.last(), Some(&0xc9));
            }
            assert_eq!(loaded.memory.read_u8(text + 257), Some(0xa5));
        }
    }
    for (width, original, expected) in [
        (32767i16, b"Fits".as_slice(), 0i16),
        (0, b"", 0),
        (0, b"Too wide", -1),
        (-1, b"Negative", -1),
    ] {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TruncString")).unwrap();
        let text = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(text, vec![0xa5; 258]);
        assert!(ppc_write_pstring_bytes(&mut loaded.memory, text, original));
        loaded.cpu.gpr[3] = width as i32 as u32;
        loaded.cpu.gpr[4] = text;
        loaded.cpu.gpr[5] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(expected));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, text).unwrap(),
            original
        );
    }
}

#[test]
fn hle_import_runner_handles_get_def_font_size() {
    let pef = synthetic_pef_with_import(b"GetDefFontSize");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 12);
}

#[test]
fn hle_import_runner_handles_get_sys_font() {
    let pef = synthetic_pef_with_import(b"GetSysFont");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_handles_get_app_font() {
    let pef = synthetic_pef_with_import(b"GetAppFont");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 3);
}

#[test]
fn hle_import_runner_handles_font_metrics() {
    let pef = synthetic_pef_with_import(b"FontMetrics");
    let mut loaded = load_pef_application(&pef).unwrap();
    let metrics_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(metrics_ptr, vec![0xaa; 20]);
    loaded.cpu.gpr[3] = metrics_ptr;
    let text_font = ppc_current_text_font(&mut loaded.memory, *loaded.current_gworld);
    let (face, scale) = get_font_face_scaled(text_font, loaded.quickdraw_text_size);
    let metrics = face.metrics;
    let to_fixed = |value: i16| -> u32 { (i32::from(value) as u32) << 16 };

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(metrics_ptr),
        Some(to_fixed(metrics.ascent.saturating_mul(scale)))
    );
    assert_eq!(
        loaded.memory.read_u32_be(metrics_ptr + 4),
        Some(to_fixed(metrics.descent.saturating_mul(scale)))
    );
    assert_eq!(
        loaded.memory.read_u32_be(metrics_ptr + 8),
        Some(to_fixed(metrics.leading.saturating_mul(scale)))
    );
    assert_eq!(
        loaded.memory.read_u32_be(metrics_ptr + 12),
        Some(to_fixed(metrics.wid_max.saturating_mul(scale)))
    );
    assert_eq!(loaded.memory.read_u32_be(metrics_ptr + 16), Some(0));
    assert_eq!(loaded.cpu.gpr[3], metrics_ptr);
}

#[test]
fn hle_import_runner_handles_get_font_name() {
    let pef = synthetic_pef_with_import(b"GetFontName");
    let mut loaded = load_pef_application(&pef).unwrap();
    let name_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(name_ptr, vec![0; 256]);
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = name_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u8(name_ptr), Some(6));
    let mut name = [0; 6];
    assert!(loaded
        .memory
        .read_bytes_into(name_ptr + 1, &mut name)
        .is_some());
    assert_eq!(&name, b"Geneva");
}
