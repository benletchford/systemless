use super::*;

fn d_form(op: u32, register: u32, base: u32, displacement: i16) -> u32 {
    (op << 26) | (register << 21) | (base << 16) | u32::from(displacement as u16)
}

fn theme_button_fixture() -> (PpcLoadedApp, u32) {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawThemeButton")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 1024]);
    for (offset, value) in [(0, 30), (2, 40), (4, 54), (6, 120)] {
        loaded.memory.write_u16_be(scratch + offset, value).unwrap();
    }
    loaded.memory.write_u32_be(scratch + 8, 1).unwrap();
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 0;
    loaded.cpu.gpr[9] = scratch + 64;
    (loaded, scratch)
}

#[test]
fn theme_button_callbacks_run_erase_before_label_and_restore_the_port() {
    let (mut loaded, scratch) = theme_button_fixture();
    let port = *loaded.current_gworld;
    let clip = loaded
        .memory
        .read_u32_be(port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET)
        .unwrap();
    let drawing = ppc_memory_read_bytes(&mut loaded.memory, port + 36, 40).unwrap();
    let rtoc = loaded.cpu.gpr[2];
    let output = scratch + 64;
    loaded.memory.write_u32_be(output + 48, port).unwrap();
    let erase = scratch + 256;
    let label = scratch + 384;
    // Erase: record arguments and mark the first callback.
    let erase_words = [
        d_form(36, 3, 4, 0),
        d_form(36, 5, 4, 4),
        d_form(36, 6, 4, 8),
        d_form(14, 0, 0, 1),
        d_form(36, 0, 4, 12),
        BLR,
    ];
    // Label: record arguments and prior callback marker; copy its transient
    // content rectangle before the Appearance Manager frees scratch storage.
    let label_words = [
        d_form(36, 3, 6, 16),
        d_form(36, 4, 6, 20),
        d_form(36, 5, 6, 24),
        d_form(36, 7, 6, 28),
        d_form(36, 8, 6, 32),
        d_form(32, 0, 6, 12),
        d_form(36, 0, 6, 36),
        d_form(32, 0, 3, 0),
        d_form(36, 0, 6, 40),
        d_form(32, 0, 3, 4),
        d_form(36, 0, 6, 44),
        // Capture the actual callback clip region, not just its rectangle argument.
        d_form(32, 9, 6, 48),
        d_form(32, 9, 9, PPC_CGRAF_PORT_CLIP_RGN_OFFSET as i16),
        d_form(32, 9, 9, 0),
        d_form(32, 0, 9, 2),
        d_form(36, 0, 6, 52),
        d_form(32, 0, 9, 6),
        d_form(36, 0, 6, 56),
        d_form(14, 0, 0, 2),
        d_form(36, 0, 6, 12),
        BLR,
    ];
    for (entry, words) in [
        (erase, erase_words.as_slice()),
        (label, label_words.as_slice()),
    ] {
        for (i, word) in words.iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + i as u32 * 4, *word)
                .unwrap();
        }
    }
    for (vector, entry) in [(scratch + 192, erase), (scratch + 200, label)] {
        loaded.memory.write_u32_be(vector, entry).unwrap();
        loaded.memory.write_u32_be(vector + 4, rtoc).unwrap();
    }
    loaded.cpu.gpr[3] = scratch + 192;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewThemeEraseUPP);
    let erase_upp = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = scratch + 200;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::NewThemeButtonDrawUPP,
    );
    let label_upp = loaded.cpu.gpr[3];
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DrawThemeButton;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = erase_upp;
    loaded.cpu.gpr[8] = label_upp;
    loaded.cpu.gpr[9] = output;
    loaded.cpu.lr = PPC_HALT_PC;
    let result = loaded.run_with_hle_imports(512);
    assert_eq!(result.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output + 12), Some(2));
    assert_eq!(loaded.memory.read_u32_be(output + 36), Some(1));
    assert_eq!(loaded.memory.read_u32_be(output + 20), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(output + 4),
        loaded.memory.read_u32_be(output + 28)
    );
    assert_eq!(
        loaded.memory.read_u32_be(output + 8),
        loaded.memory.read_u32_be(output + 32)
    );
    assert_eq!(loaded.memory.read_u16_be(output + 40), Some(34));
    assert_eq!(loaded.memory.read_u16_be(output + 42), Some(44));
    assert_eq!(loaded.memory.read_u16_be(output + 44), Some(50));
    assert_eq!(loaded.memory.read_u16_be(output + 46), Some(116));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, output + 52, 8),
        ppc_memory_read_bytes(&mut loaded.memory, output + 40, 8)
    );
    assert_eq!(loaded.current_gworld, port);
    assert_eq!(loaded.cpu.gpr[2], rtoc);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET),
        Some(clip)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, port + 36, 40),
        Some(drawing)
    );
}

#[test]
fn theme_button_rejects_invalid_info_and_callback_without_leaking_port_state() {
    let (mut loaded, scratch) = theme_button_fixture();
    loaded.cpu.gpr[5] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawThemeButton);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[7] = 0xffff_ffff;
    let port = *loaded.current_gworld;
    let clip = loaded
        .memory
        .read_u32_be(port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawThemeButton);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(
        loaded
            .memory
            .read_u32_be(port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET),
        clip
    );
}

#[test]
fn theme_button_rendering_distinguishes_shapes_values_and_pressed_halves() {
    use super::super::dispatch_theme_button::theme_button_bitmap;
    for theme in [UiThemeId::ClassicSystem7, UiThemeId::SystemlessDefault] {
        for kind in 0..=15 {
            for state in [0, 1, 2, 3, 6, 7, 8] {
                let (bitmap, _, transparent) =
                    theme_button_bitmap(theme, kind, state, 1, 0, 40, 24).unwrap();
                assert!(
                    bitmap
                        .rgba()
                        .chunks_exact(4)
                        .any(|p| p[0..3] != [transparent.r, transparent.g, transparent.b]),
                    "kind {kind}, state {state}"
                );
            }
            // Tiny valid bounds must not overflow or turn into enormous allocations.
            assert!(theme_button_bitmap(theme, kind, 1, 0, 0, 1, 1).is_some());
        }
        let draw = |kind, state, value| {
            theme_button_bitmap(theme, kind, state, value, 0, 40, 24)
                .unwrap()
                .0
        };
        assert_ne!(draw(3, 1, 0).rgba(), draw(15, 1, 0).rgba());
        assert_ne!(draw(7, 2, 0).rgba(), draw(7, 3, 0).rgba());
        assert_ne!(draw(1, 1, 0).rgba(), draw(1, 1, 1).rgba());
        assert_ne!(draw(1, 1, 1).rgba(), draw(1, 1, 2).rgba());
        assert_ne!(draw(2, 1, 0).rgba(), draw(2, 1, 1).rgba());
        assert_ne!(draw(1, 1, 0).rgba(), draw(2, 1, 0).rgba());
        let (round, _, clear) = theme_button_bitmap(theme, 11, 1, 0, 0, 40, 24).unwrap();
        assert_eq!(&round.rgba()[0..3], &[clear.r, clear.g, clear.b]);
        assert_ne!(
            &round.rgba()[(12 * 40 + 20) * 4..(12 * 40 + 20) * 4 + 3],
            &[clear.r, clear.g, clear.b]
        );
    }
}

#[test]
fn theme_button_label_can_draw_another_button_with_a_nested_label_callback() {
    let (mut loaded, scratch) = theme_button_fixture();
    let output = scratch + 64;
    let outer = scratch + 256;
    let inner = scratch + 640;
    let outer_vector = scratch + 768;
    let inner_vector = scratch + 776;
    let port = *loaded.current_gworld;
    let clip = loaded
        .memory
        .read_u32_be(port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET)
        .unwrap();
    let sp = loaded.cpu.gpr[1];
    let rtoc = loaded.cpu.gpr[2];
    let saved_r31 = loaded.cpu.gpr[31];
    for (offset, value) in [
        (4, port),
        (16, scratch + 192),
        (20, scratch + 200),
        (24, loaded.imports[0].trap_pc),
        (28, inner_vector),
        (32, output + 96),
    ] {
        loaded.memory.write_u32_be(output + offset, value).unwrap();
    }
    for (offset, value) in [(0, 36), (2, 50), (4, 48), (6, 90)] {
        loaded
            .memory
            .write_u16_be(scratch + 192 + offset, value)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 200, 1).unwrap();
    let words = [
        d_form(37, 1, 1, -64),
        xfx_form(31, 0, 8, 339),
        d_form(36, 0, 1, 68),
        d_form(36, 31, 1, 60),
        x_form(31, 6, 31, 6, 444, false),
        d_form(32, 10, 31, 4),
        d_form(32, 9, 10, PPC_CGRAF_PORT_CLIP_RGN_OFFSET as i16),
        d_form(36, 9, 31, 8),
        d_form(32, 3, 31, 16),
        d_form(14, 4, 0, 1),
        d_form(32, 5, 31, 20),
        d_form(14, 6, 0, 0),
        d_form(14, 7, 0, 0),
        d_form(32, 8, 31, 28),
        d_form(32, 9, 31, 32),
        d_form(32, 12, 31, 24),
        xfx_form(31, 12, 9, 467),
        xl_form(19, 20, 0, 528, true),
        d_form(36, 3, 31, 0),
        d_form(32, 10, 31, 4),
        d_form(32, 9, 10, PPC_CGRAF_PORT_CLIP_RGN_OFFSET as i16),
        d_form(36, 9, 31, 12),
        d_form(32, 31, 1, 60),
        d_form(32, 0, 1, 68),
        xfx_form(31, 0, 8, 467),
        d_form(14, 1, 1, 64),
        BLR,
    ];
    for (i, word) in words.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(outer + i as u32 * 4, word)
            .unwrap();
    }
    for (i, word) in [d_form(14, 0, 0, 3), d_form(36, 0, 6, 0), BLR]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(inner + i as u32 * 4, word)
            .unwrap();
    }
    for (vector, entry) in [(outer_vector, outer), (inner_vector, inner)] {
        loaded.memory.write_u32_be(vector, entry).unwrap();
        loaded.memory.write_u32_be(vector + 4, rtoc).unwrap();
    }
    loaded.cpu.gpr[8] = outer_vector;
    loaded.cpu.lr = PPC_HALT_PC;
    let result = loaded.run_with_hle_imports(1024);
    assert_eq!(result.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0));
    assert_eq!(loaded.memory.read_u32_be(output + 96), Some(3));
    let outer_clip = loaded.memory.read_u32_be(output + 8).unwrap();
    assert_ne!(outer_clip, clip);
    assert_eq!(loaded.memory.read_u32_be(output + 12), Some(outer_clip));
    assert_eq!(
        loaded
            .memory
            .read_u32_be(port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET),
        Some(clip)
    );
    assert_eq!(loaded.cpu.gpr[1], sp);
    assert_eq!(loaded.cpu.gpr[2], rtoc);
    assert_eq!(loaded.cpu.gpr[31], saved_r31);
}

#[test]
fn theme_button_header_shadow_passes_preserve_each_other_in_either_order() {
    let (mut loaded, scratch) = theme_button_fixture();
    let port = *loaded.current_gworld;
    let surface = ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, port).unwrap();
    let frame = surface.front_buffer;
    let initial = vec![0x55; (frame.row_bytes * frame.height) as usize];
    let draw = |loaded: &mut PpcLoadedApp, adornment| {
        loaded.memory.write_u16_be(scratch + 12, 1).unwrap();
        loaded.memory.write_u16_be(scratch + 14, adornment).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 10;
        loaded.cpu.gpr[5] = scratch + 8;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(loaded, PpcImportDispatcherTarget::DrawThemeButton);
        assert_eq!(loaded.cpu.gpr[3], 0);
    };
    for adornment in [0, 1 << 6, 1 << 7, 1 << 8, 1 << 9] {
        loaded
            .memory
            .write_bytes(frame.base_addr, &initial)
            .unwrap();
        draw(&mut loaded, adornment);
        let whole =
            ppc_memory_read_bytes(&mut loaded.memory, frame.base_addr, initial.len() as u32)
                .unwrap();
        assert_ne!(whole, initial);
        for order in [[0x0400, 0x0800], [0x0800, 0x0400]] {
            loaded
                .memory
                .write_bytes(frame.base_addr, &initial)
                .unwrap();
            for flag in order {
                draw(&mut loaded, adornment | flag);
            }
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, frame.base_addr, initial.len() as u32),
                Some(whole.clone()),
                "header adornment {adornment:#x}, order {order:?}"
            );
        }
    }
}

#[test]
fn theme_button_draws_at_each_depth_without_touching_pixels_outside_the_port_clip() {
    for depth in [1, 2, 4, 8, 16, 32] {
        let (mut loaded, scratch) = theme_button_fixture();
        ppc_write_rect(&mut loaded.memory, scratch + 64, -5, -7, 27, 41).unwrap();
        loaded.cpu.gpr[3] = scratch + 48;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = scratch + 64;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        assert_eq!(loaded.cpu.gpr[3], 0);
        let port = loaded.memory.read_u32_be(scratch + 48).unwrap();
        loaded.cpu.gpr[3] = port;
        loaded.cpu.gpr[4] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetGWorld);
        let surface =
            ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, port).unwrap();
        let frame = surface.front_buffer;
        let initial = vec![0x55; (frame.row_bytes * frame.height) as usize];
        loaded
            .memory
            .write_bytes(frame.base_addr, &initial)
            .unwrap();
        // In surface coordinates this is x=8..24, y=4..20. Even at one bit
        // per pixel, both horizontal boundaries fall on complete bytes.
        ppc_write_rect(&mut loaded.memory, scratch + 80, -1, 1, 15, 17).unwrap();
        loaded.cpu.gpr[3] = scratch + 80;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ClipRect);
        ppc_write_rect(&mut loaded.memory, scratch, -3, -5, 23, 37).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = scratch + 8;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawThemeButton);
        assert_eq!(loaded.cpu.gpr[3], 0);
        let actual =
            ppc_memory_read_bytes(&mut loaded.memory, frame.base_addr, initial.len() as u32)
                .unwrap();
        assert!(actual != initial, "button did not draw at depth {depth}");
        if depth == 32 {
            let color = ppc_ui_theme(&loaded.gworlds)
                .provider()
                .palette()
                .frame_light;
            let expected =
                (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b);
            assert_eq!(
                loaded
                    .memory
                    .read_u32_be(frame.base_addr + 8 * frame.row_bytes + 12 * 4),
                Some(expected)
            );
        }
        for (i, (&before, &after)) in initial.iter().zip(&actual).enumerate() {
            let y = i as u32 / frame.row_bytes;
            let column = i as u32 % frame.row_bytes;
            if !(4..20).contains(&y) || !(depth..3 * depth).contains(&column) {
                assert_eq!(after, before, "depth {depth}, row {y}, byte {column}");
            }
        }
    }
}
