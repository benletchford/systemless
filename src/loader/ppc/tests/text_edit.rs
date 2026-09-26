use super::*;

#[test]
fn hle_import_runner_creates_and_disposes_native_styled_textedit_records() {
    let pef = synthetic_pef_with_import(b"TEStyleNew");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rects = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rects, vec![0; 16]);
    for (offset, value) in [10u16, 20, 110, 220].into_iter().enumerate() {
        loaded
            .memory
            .write_u16_be(rects + offset as u32 * 2, value)
            .unwrap();
    }
    for (offset, value) in [12u16, 22, 108, 218].into_iter().enumerate() {
        loaded
            .memory
            .write_u16_be(rects + 8 + offset as u32 * 2, value)
            .unwrap();
    }
    loaded.cpu.gpr[3] = rects;
    loaded.cpu.gpr[4] = rects + 8;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let te_handle = loaded.cpu.gpr[3];
    let te_ptr = loaded.memory.read_u32_be(te_handle).unwrap();
    assert_ne!(te_handle, 0);
    assert_eq!(
        ppc_te_read_rect(&mut loaded.memory, te_ptr + PPC_TE_DEST_RECT_OFFSET),
        Some([10, 20, 110, 220])
    );
    assert_eq!(
        ppc_te_read_rect(&mut loaded.memory, te_ptr + PPC_TE_VIEW_RECT_OFFSET),
        Some([12, 22, 108, 218])
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(te_ptr + PPC_TE_LINE_HEIGHT_OFFSET),
        Some(0xffff)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(te_ptr + PPC_TE_FONT_ASCENT_OFFSET),
        Some(0xffff)
    );
    assert_eq!(
        loaded.memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET),
        Some(0xffff)
    );
    let text_handle = loaded
        .memory
        .read_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET)
        .unwrap();
    let style_handle = loaded
        .memory
        .read_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
        .unwrap();
    assert_ne!(text_handle, 0);
    assert_ne!(style_handle, 0);
    assert_eq!(test_handle_records!(loaded).len(), 7);

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::TEGetText;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = te_handle;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], text_handle);

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::TEDispose;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = te_handle;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.memory.read_u32_be(te_handle), Some(0));
    assert!(test_handle_records!(loaded).is_empty());
}

#[test]
fn te_use_style_scrap_applies_the_first_scrap_style_to_the_requested_range() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TEStyleNew")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 0x100]);
    ppc_write_rect(&mut loaded.memory, scratch, 10, 20, 80, 220).unwrap();
    ppc_write_rect(&mut loaded.memory, scratch + 8, 10, 20, 80, 220).unwrap();
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = scratch + 8;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEStyleNew);
    let te_handle = loaded.cpu.gpr[3];

    loaded.memory.write_bytes(scratch + 0x20, b"Styled").unwrap();
    loaded.cpu.gpr[3] = scratch + 0x20;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetText);

    loaded.cpu.gpr[3] = 22;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::NewHandle { clear: true },
    );
    let scrap_handle = loaded.cpu.gpr[3];
    let scrap_ptr = loaded.memory.read_u32_be(scrap_handle).unwrap();
    let element = scrap_ptr + PPC_TE_SCRAP_STYLE_TAB_OFFSET;
    loaded
        .memory
        .write_u16_be(scrap_ptr + PPC_TE_SCRAP_N_STYLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u16_be(element + PPC_TE_SCRAP_STYLE_FONT_OFFSET, 4)
        .unwrap();
    loaded
        .memory
        .write_u8(element + PPC_TE_SCRAP_STYLE_FACE_OFFSET, 0x04)
        .unwrap();
    loaded
        .memory
        .write_u16_be(element + PPC_TE_SCRAP_STYLE_SIZE_OFFSET, 11)
        .unwrap();
    loaded
        .memory
        .write_u16_be(element + PPC_TE_SCRAP_STYLE_COLOR_OFFSET + 2, 0xffff)
        .unwrap();

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 3;
    loaded.cpu.gpr[5] = scrap_handle;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEUseStyleScrap);

    let runs = ppc_te_style_runs(
        &mut loaded.memory,
        &test_handle_records!(loaded),
        te_handle,
        6,
    );
    assert_eq!(runs[0].start, 0);
    assert_eq!(runs[0].style.font, 4);
    assert_eq!(runs[0].style.face, 0x04);
    assert_eq!(runs[0].style.size, 11);
    assert_eq!(runs[0].style.color.green, 0xffff);
    assert_eq!(runs[1].start, 3);
}

#[test]
fn native_styled_textedit_styles_runs_and_reports_real_measurements() {
    let pef = synthetic_pef_with_import(b"TEStyleNew");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rects = PPC_DATA_BASE + 0x1000;
    let text_ptr = PPC_DATA_BASE + 0x1020;
    let style_ptr = PPC_DATA_BASE + 0x1040;
    let mode_ptr = PPC_DATA_BASE + 0x1060;
    let result_style_ptr = PPC_DATA_BASE + 0x1064;
    let locations_ptr = PPC_DATA_BASE + 0x1080;
    let name_ptr = PPC_DATA_BASE + 0x10a0;
    loaded
        .memory
        .add_region(PPC_DATA_BASE + 0x1000, vec![0; 0x100]);
    ppc_write_rect(&mut loaded.memory, rects, 10, 20, 80, 220).unwrap();
    ppc_write_rect(&mut loaded.memory, rects + 8, 10, 20, 80, 220).unwrap();
    loaded.cpu.gpr[3] = rects;
    loaded.cpu.gpr[4] = rects + 8;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEStyleNew);
    let te_handle = loaded.cpu.gpr[3];
    let constructor_te_ptr = loaded.memory.read_u32_be(te_handle).unwrap();
    let style_handle = loaded
        .memory
        .read_u32_be(constructor_te_ptr + PPC_TE_TX_FONT_OFFSET)
        .unwrap();

    loaded.memory.write_bytes(text_ptr, b"Styled").unwrap();
    loaded.cpu.gpr[3] = text_ptr;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetText);

    let write_style =
        |memory: &mut PpcSectionMem, font: i16, face: u8, size: i16, color: PpcRgbColor| {
            memory.write_u16_be(style_ptr, font as u16).unwrap();
            memory.write_u8(style_ptr + 2, face).unwrap();
            memory.write_u16_be(style_ptr + 4, size as u16).unwrap();
            memory.write_u16_be(style_ptr + 6, color.red).unwrap();
            memory.write_u16_be(style_ptr + 8, color.green).unwrap();
            memory.write_u16_be(style_ptr + 10, color.blue).unwrap();
        };
    write_style(
        &mut loaded.memory,
        3,
        0x01,
        14,
        PpcRgbColor {
            red: 0xffff,
            green: 0,
            blue: 0,
        },
    );
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 3;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetSelect);
    loaded.cpu.gpr[3] = 0x000f;
    loaded.cpu.gpr[4] = style_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetStyle);

    write_style(
        &mut loaded.memory,
        4,
        0x02,
        10,
        PpcRgbColor {
            red: 0,
            green: 0,
            blue: 0xffff,
        },
    );
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetSelect);
    loaded.cpu.gpr[3] = 0x000f;
    loaded.cpu.gpr[4] = style_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetStyle);

    let runs = ppc_te_style_runs(
        &mut loaded.memory,
        &test_handle_records!(loaded),
        te_handle,
        6,
    );
    assert_eq!(
        runs.iter().map(|run| run.start).collect::<Vec<_>>(),
        vec![0, 3]
    );
    assert_eq!(runs[0].style.font, 3);
    assert_eq!(runs[0].style.face, 0x01);
    assert_eq!(runs[0].style.size, 14);
    assert_eq!(runs[0].style.color.red, 0xffff);
    assert_eq!(runs[1].style.font, 4);
    assert_eq!(runs[1].style.face, 0x02);
    assert_eq!(runs[1].style.size, 10);
    assert_eq!(runs[1].style.color.blue, 0xffff);

    loaded.cpu.gpr[3] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TECalText);
    let te_ptr = loaded.memory.read_u32_be(te_handle).unwrap();
    let n_lines = usize::from(
        loaded
            .memory
            .read_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET)
            .unwrap(),
    );
    assert!(n_lines > 0);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + n_lines as u32 * 2),
        Some(6)
    );
    assert_eq!(
        loaded.memory.read_u16_be(
            te_ptr + PPC_TE_LINE_STARTS_OFFSET + n_lines.saturating_add(1) as u32 * 2,
        ),
        Some(0)
    );
    let style_record_ptr = loaded.memory.read_u32_be(style_handle).unwrap();
    let n_runs = usize::from(
        loaded
            .memory
            .read_u16_be(style_record_ptr + PPC_TE_STYLE_N_RUNS_OFFSET)
            .unwrap(),
    );
    assert_eq!(
        loaded.memory.read_u16_be(
            style_record_ptr + PPC_TE_STYLE_RUNS_OFFSET + n_runs as u32 * 4,
        ),
        Some(7)
    );
    let lh_handle = loaded
        .memory
        .read_u32_be(style_record_ptr + PPC_TE_STYLE_LH_TABLE_OFFSET)
        .unwrap();
    let handle_records = test_handle_records!(loaded);
    let lh_record = handle_records
        .iter()
        .find(|record| record.handle == lh_handle)
        .unwrap();
    assert!(lh_record.size >= (n_lines.saturating_add(1) * 4) as u32);
    let lh_ptr = loaded.memory.read_u32_be(lh_handle).unwrap();
    assert!(loaded.memory.read_u16_be(lh_ptr + n_lines as u32 * 4).unwrap() > 0);

    // Text 1993, p. 2-102: a mixed face selection reports the bits
    // common to every face, so bold plus bold-italic reports bold.
    write_style(
        &mut loaded.memory,
        4,
        0x03,
        10,
        PpcRgbColor {
            red: 0,
            green: 0,
            blue: 0xffff,
        },
    );
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetSelect);
    loaded.cpu.gpr[3] = 0x0002;
    loaded.cpu.gpr[4] = style_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetStyle);
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetSelect);
    loaded.memory.write_u16_be(mode_ptr, 0x0002).unwrap();
    loaded.cpu.gpr[3] = mode_ptr;
    loaded.cpu.gpr[4] = result_style_ptr;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEContinuousStyle);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u16_be(mode_ptr), Some(0x0002));
    assert_eq!(loaded.memory.read_u8(result_style_ptr + 2), Some(0x01));

    // TESetStyle at an insertion point updates the null scrap used by
    // later TEInsert/TEStyleInsert calls instead of changing no runs.
    write_style(
        &mut loaded.memory,
        4,
        0x04,
        11,
        PpcRgbColor {
            red: 0,
            green: 0xffff,
            blue: 0,
        },
    );
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 3;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetSelect);
    loaded.cpu.gpr[3] = 0x000f;
    loaded.cpu.gpr[4] = style_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetStyle);
    let style_record_ptr = loaded.memory.read_u32_be(style_handle).unwrap();
    let null_style_handle = loaded
        .memory
        .read_u32_be(style_record_ptr + PPC_TE_STYLE_NULL_STYLE_OFFSET)
        .unwrap();
    let null_style_ptr = loaded.memory.read_u32_be(null_style_handle).unwrap();
    let null_scrap_handle = loaded
        .memory
        .read_u32_be(null_style_ptr + PPC_TE_NULL_STYLE_SCRAP_OFFSET)
        .unwrap();
    let null_scrap_ptr = loaded.memory.read_u32_be(null_scrap_handle).unwrap();
    let null_element = null_scrap_ptr + PPC_TE_SCRAP_STYLE_TAB_OFFSET;
    assert_eq!(
        loaded
            .memory
            .read_u16_be(null_scrap_ptr + PPC_TE_SCRAP_N_STYLES_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(null_element + PPC_TE_SCRAP_STYLE_FONT_OFFSET),
        Some(4)
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(null_element + PPC_TE_SCRAP_STYLE_FACE_OFFSET),
        Some(0x04)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(null_element + PPC_TE_SCRAP_STYLE_SIZE_OFFSET),
        Some(11)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(null_element + PPC_TE_SCRAP_STYLE_COLOR_OFFSET + 2),
        Some(0xffff)
    );
    loaded.memory.write_u16_be(mode_ptr, 0x000f).unwrap();
    loaded.cpu.gpr[3] = mode_ptr;
    loaded.cpu.gpr[4] = result_style_ptr;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEContinuousStyle);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u16_be(mode_ptr), Some(0x000f));
    assert_eq!(loaded.memory.read_u8(result_style_ptr + 2), Some(0x04));
    assert_eq!(loaded.memory.read_u16_be(result_style_ptr + 4), Some(11));

    loaded.memory.write_u16_be(mode_ptr, 0x000f).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetSelect);
    loaded.cpu.gpr[3] = mode_ptr;
    loaded.cpu.gpr[4] = result_style_ptr;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEContinuousStyle);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(mode_ptr), Some(0x0002));

    loaded.memory.write_u16_be(mode_ptr, 0x000f).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 3;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetSelect);
    loaded.cpu.gpr[3] = mode_ptr;
    loaded.cpu.gpr[4] = result_style_ptr;
    loaded.cpu.gpr[5] = te_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEContinuousStyle);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u16_be(mode_ptr), Some(0x000f));
    assert_eq!(loaded.memory.read_u16_be(result_style_ptr + 4), Some(14));

    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 10;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TextSize);
    loaded.cpu.gpr[3] = 6;
    loaded.cpu.gpr[4] = text_ptr;
    loaded.cpu.gpr[5] = locations_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MeasureText);
    let measure_width = loaded.memory.read_u16_be(locations_ptr + 12).unwrap();
    assert!(measure_width > 0);
    loaded.cpu.gpr[3] = text_ptr;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 6;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TextWidth);
    let text_width = loaded.cpu.gpr[3];
    assert!(text_width > 0);
    assert!((text_width as i32 - measure_width as i32).abs() <= 6);

    write_ppc_pstring(&mut loaded.memory, name_ptr, b"Geneva");
    loaded.cpu.gpr[3] = name_ptr;
    loaded.cpu.gpr[4] = result_style_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetFNum);
    assert_eq!(loaded.memory.read_u16_be(result_style_ptr), Some(3));
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 9;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::RealFont);
    assert_eq!(loaded.cpu.gpr[3], 1);
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 12;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::RealFont);
    assert_eq!(loaded.cpu.gpr[3], 1);
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = 12;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::RealFont);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn native_textedit_key_and_click_update_the_public_edit_record() {
    let pef = synthetic_pef_with_import(b"TEKey");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rects = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rects, vec![0; 16]);
    ppc_write_rect(&mut loaded.memory, rects, 10, 20, 80, 220).unwrap();
    ppc_write_rect(&mut loaded.memory, rects + 8, 10, 20, 80, 220).unwrap();
    let mut last_mem_error = PPC_NO_ERR;
    let te_handle = ppc_te_initialize_record(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        rects,
        rects + 8,
        PPC_MAIN_GWORLD,
        1,
        PPC_QD_TEXT_MODE_SRC_OR,
        12,
        PpcRgbColor {
            red: 0,
            green: 0,
            blue: 0,
        },
        false,
    );
    assert_eq!(
        ppc_te_set_text(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            te_handle,
            b"abc",
        ),
        PPC_NO_ERR
    );

    loaded.cpu.gpr[3] = 0x08;
    loaded.cpu.gpr[4] = te_handle;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_te_text_bytes(&mut loaded.memory, &test_handle_records!(loaded), te_handle),
        Some(b"ab".to_vec())
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = u32::from(b'Z');
    loaded.run_with_hle_imports(64);
    assert_eq!(
        ppc_te_text_bytes(&mut loaded.memory, &test_handle_records!(loaded), te_handle),
        Some(b"abZ".to_vec())
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0x1c;
    loaded.run_with_hle_imports(64);
    let te_ptr = loaded.memory.read_u32_be(te_handle).unwrap();
    assert_eq!(
        loaded.memory.read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET),
        Some(2)
    );
    assert_eq!(
        loaded.memory.read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET),
        Some(2)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0x1d;
    loaded.run_with_hle_imports(64);
    assert_eq!(
        loaded.memory.read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET),
        Some(3)
    );
    assert_eq!(
        ppc_te_text_bytes(&mut loaded.memory, &test_handle_records!(loaded), te_handle),
        Some(b"abZ".to_vec())
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::TEClick;
    loaded.set_tick_count(100);
    loaded.cpu.gpr[3] = (12u32 << 16) | 20;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = te_handle;
    loaded.run_with_hle_imports(64);
    assert_eq!(
        loaded.memory.read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded.memory.read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET),
        Some(0)
    );
    let click_time = loaded
        .memory
        .read_u32_be(te_ptr + PPC_TE_CLICK_TIME_OFFSET)
        .unwrap();
    assert!((100..=164).contains(&click_time));
}

#[test]
fn native_textedit_line_starts_follow_shared_word_boundaries() {
    // Inside Macintosh: Text (1993), pp. 5-24--5-27: prefer a word
    // boundary over splitting the next word at the overflowing glyph.
    let pef = synthetic_pef_with_import(b"TECalText");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rects = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rects, vec![0; 16]);
    let font = PPC_QD_TEXT_FONT_DEFAULT;
    let size = PPC_QD_TEXT_SIZE_SYSTEM;
    let first_line_width = ppc_text_bytes_advance_for_font(b"one two", font, size);
    ppc_write_rect(&mut loaded.memory, rects, 10, 20, 80, 20 + first_line_width).unwrap();
    ppc_write_rect(
        &mut loaded.memory,
        rects + 8,
        10,
        20,
        80,
        20 + first_line_width,
    )
    .unwrap();
    let mut last_mem_error = PPC_NO_ERR;
    let te_handle = ppc_te_initialize_record(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        rects,
        rects + 8,
        PPC_MAIN_GWORLD,
        1,
        PPC_QD_TEXT_MODE_SRC_OR,
        size,
        PPC_RGB_BLACK,
        false,
    );

    assert_eq!(
        ppc_te_set_text(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            te_handle,
            b"one two three",
        ),
        PPC_NO_ERR
    );
    let te_ptr = loaded.memory.read_u32_be(te_handle).unwrap();
    assert_eq!(
        loaded.memory.read_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET),
        Some(2)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + 2),
        Some(8)
    );
}

#[test]
fn te_text_box_replaces_contents_and_erases_empty_text() {
    let pef = synthetic_pef_with_import(b"TETextBox");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let rect = scratch + 0x20;
    loaded.memory.add_region(scratch, vec![0; 0x40]);
    loaded.memory.write_u8(scratch, b'0').unwrap();
    ppc_write_rect(&mut loaded.memory, rect, 10, 10, 40, 100).unwrap();
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    for y in 9..41 {
        for x in 9..101 {
            assert!(ppc_quickdraw_write_raw_pixel(
                &mut loaded.memory,
                front,
                (x, y),
                103
            ));
        }
    }
    let clip = loaded
        .memory
        .read_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_CLIP_RGN_OFFSET)
        .unwrap();
    ppc_set_rect_rgn(&mut loaded.memory, clip, 0, 0, 80, 100).unwrap();
    loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 99);
    loaded
        .toolbox_startup
        .quickdraw_back_indices
        .insert(PPC_MAIN_GWORLD, 17);
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = rect;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TETextBox);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (70, 35)),
        Some(17)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (90, 35)),
        Some(103)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (9, 9)),
        Some(103)
    );
    assert!((10..40).any(|y| (10..80)
        .any(|x| { ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)) == Some(99) })));

    // An empty replacement still removes every previous glyph inside
    // the box, while the portion outside clipRgn stays untouched.
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = rect;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TETextBox);
    for y in 10..40 {
        for x in 10..100 {
            assert_eq!(
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)),
                Some(if x < 80 { 17 } else { 103 }),
                "replacement pixel ({x}, {y})",
            );
        }
    }
}

#[test]
fn textedit_and_te_text_box_use_explicit_foreground_index() {
    let pef = synthetic_pef_with_import(b"TETextBox");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let text = scratch;
    let text_box_rect = scratch + 0x20;
    let te_rects = scratch + 0x40;
    loaded.memory.add_region(scratch, vec![0; 0x60]);
    loaded.memory.write_u8(text, b'0').unwrap();
    ppc_write_rect(&mut loaded.memory, text_box_rect, 10, 10, 40, 100).unwrap();
    loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
    loaded.cpu.gpr[3] = text;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = text_box_rect;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert!((10..40).any(|y| {
        (10..100)
            .any(|x| ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)) == Some(103))
    }));

    ppc_write_rect(&mut loaded.memory, te_rects, 60, 10, 100, 100).unwrap();
    ppc_write_rect(&mut loaded.memory, te_rects + 8, 60, 10, 100, 100).unwrap();
    let mut last_mem_error = PPC_NO_ERR;
    let te_handle = ppc_te_initialize_record(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        te_rects,
        te_rects + 8,
        PPC_MAIN_GWORLD,
        0,
        PPC_QD_TEXT_MODE_SRC_OR,
        12,
        loaded.quickdraw_fore_color,
        false,
    );
    assert_eq!(
        ppc_te_set_text(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            te_handle,
            b"0",
        ),
        PPC_NO_ERR
    );
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::TEUpdate;
    loaded.cpu.gpr[3] = te_rects;
    loaded.cpu.gpr[4] = te_handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert!((60..100).any(|y| {
        (10..100)
            .any(|x| ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)) == Some(103))
    }));
}

#[test]
fn te_update_clips_text_to_the_view_rect() {
    // Text (1993), pp. 2-16 and 2-29: TextEdit draws only inside viewRect.
    let pef = synthetic_pef_with_import(b"TEUpdate");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let dest_rect = scratch;
    let view_rect = scratch + 8;
    loaded.memory.add_region(scratch, vec![0; 0x20]);
    ppc_write_rect(&mut loaded.memory, dest_rect, 60, 10, 200, 100).unwrap();
    ppc_write_rect(&mut loaded.memory, view_rect, 60, 10, 80, 100).unwrap();
    loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
    let mut last_mem_error = PPC_NO_ERR;
    let te_handle = ppc_te_initialize_record(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        dest_rect,
        view_rect,
        PPC_MAIN_GWORLD,
        0,
        PPC_QD_TEXT_MODE_SRC_OR,
        12,
        loaded.quickdraw_fore_color,
        false,
    );
    assert_eq!(
        ppc_te_set_text(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            te_handle,
            b"0\r0\r0\r0\r0",
        ),
        PPC_NO_ERR
    );
    loaded.cpu.gpr[3] = view_rect;
    loaded.cpu.gpr[4] = te_handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let mut ink_rows = |rows: std::ops::Range<i32>| {
        rows.filter(|&y| {
            (10..100)
                .any(|x| ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)) == Some(103))
        })
        .count()
    };
    assert!(ink_rows(60..80) > 0, "lines inside viewRect must still draw");
    assert_eq!(ink_rows(80..200), 0, "lines below viewRect must be clipped");
}

#[test]
fn te_scroll_erases_the_previous_text_position() {
    // Text (1993), p. 2-89: TEScroll scrolls the text within viewRect, so
    // srcOr text must not remain at its previous position.
    let pef = synthetic_pef_with_import(b"TEScroll");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let dest_rect = scratch;
    let view_rect = scratch + 8;
    loaded.memory.add_region(scratch, vec![0; 0x20]);
    ppc_write_rect(&mut loaded.memory, dest_rect, 60, 10, 200, 100).unwrap();
    ppc_write_rect(&mut loaded.memory, view_rect, 60, 10, 120, 100).unwrap();
    loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
    let mut last_mem_error = PPC_NO_ERR;
    let te_handle = ppc_te_initialize_record(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        dest_rect,
        view_rect,
        PPC_MAIN_GWORLD,
        0,
        PPC_QD_TEXT_MODE_SRC_OR,
        12,
        loaded.quickdraw_fore_color,
        false,
    );
    assert_eq!(
        ppc_te_set_text(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            te_handle,
            b"0\r0\r0\r0\r0\r0\r0\r0",
        ),
        PPC_NO_ERR
    );
    ppc_te_draw(
        &mut loaded.memory,
        test_handles!(loaded),
        &loaded.gworlds,
        te_handle,
        PPC_MAIN_GWORLD,
        loaded.quickdraw_fore_color,
        &loaded.quickdraw_fore_indices,
    );
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let ink_rows = |memory: &mut PpcSectionMem| {
        (60..120)
            .filter(|&y| {
                (10..100).any(|x| ppc_quickdraw_read_pixel(memory, front, (x, y)) == Some(103))
            })
            .collect::<Vec<i32>>()
    };
    let before = ink_rows(&mut loaded.memory);
    assert!(!before.is_empty());
    let dv = -7i32;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = dv as u32;
    loaded.cpu.gpr[5] = te_handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    let after = ink_rows(&mut loaded.memory);
    let shifted_top = before.iter().map(|y| y + dv).filter(|y| *y >= 60).collect::<Vec<_>>();
    assert_eq!(
        after[..shifted_top.len()],
        shifted_top[..],
        "only the scrolled text may remain inside the view"
    );
}

#[test]
fn te_get_height_accepts_zero_for_the_first_line() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TENew")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 16]);
    ppc_write_rect(&mut loaded.memory, scratch, 0, 0, 100, 200).unwrap();
    ppc_write_rect(&mut loaded.memory, scratch + 8, 0, 0, 100, 200).unwrap();
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = scratch + 8;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TENew);
    let te_handle = loaded.cpu.gpr[3];
    let te_ptr = loaded.memory.read_u32_be(te_handle).unwrap();
    loaded.memory.write_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET, 3).unwrap();
    loaded.memory.write_u16_be(te_ptr + PPC_TE_LINE_HEIGHT_OFFSET, 12).unwrap();

    assert_eq!(ppc_te_get_height(&mut loaded.memory, te_handle, 0, 32767), 36);
    assert_eq!(ppc_te_get_height(&mut loaded.memory, te_handle, 1, 32767), 36);
    assert_eq!(ppc_te_get_height(&mut loaded.memory, te_handle, 2, 3), 24);
}


#[test]
fn te_update_skips_text_lines_below_the_view_rectangle() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TEUpdate")).unwrap();
    let rects = PPC_DATA_BASE + 0x1800;
    loaded.memory.add_region(rects, vec![0; 16]);
    ppc_write_rect(&mut loaded.memory, rects, 60, 10, 100, 100).unwrap();
    ppc_write_rect(&mut loaded.memory, rects + 8, 10, 10, 40, 100).unwrap();
    let mut last_mem_error = PPC_NO_ERR;
    let te_handle = ppc_te_initialize_record(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        rects,
        rects + 8,
        PPC_MAIN_GWORLD,
        0,
        PPC_QD_TEXT_MODE_SRC_OR,
        12,
        loaded.quickdraw_fore_color,
        false,
    );
    assert_eq!(
        ppc_te_set_text(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            te_handle,
            b"OUTSIDE VIEW",
        ),
        PPC_NO_ERR
    );
    loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
    loaded.cpu.gpr[4] = te_handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert!((60..100).all(|y| (10..100).all(|x| {
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)) != Some(103)
    })));
}


#[test]
fn te_update_clips_partial_glyphs_at_the_view_bottom() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TEUpdate")).unwrap();
    let rects = PPC_DATA_BASE + 0x1800;
    loaded.memory.add_region(rects, vec![0; 16]);
    ppc_write_rect(&mut loaded.memory, rects, 20, 10, 100, 100).unwrap();
    ppc_write_rect(&mut loaded.memory, rects + 8, 20, 10, 26, 100).unwrap();
    let mut last_mem_error = PPC_NO_ERR;
    let te_handle = ppc_te_initialize_record(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        rects,
        rects + 8,
        PPC_MAIN_GWORLD,
        0,
        PPC_QD_TEXT_MODE_SRC_OR,
        12,
        loaded.quickdraw_fore_color,
        false,
    );
    assert_eq!(
        ppc_te_set_text(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            te_handle,
            b"M",
        ),
        PPC_NO_ERR
    );
    loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
    loaded.cpu.gpr[4] = te_handle;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert!((20..26).any(|y| (10..100).any(|x| {
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)) == Some(103)
    })));
    assert!((26..40).all(|y| (10..100).all(|x| {
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)) != Some(103)
    })));
}
