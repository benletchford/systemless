use super::*;
use crate::memory::globals::addr;
use crate::trap::TrapDispatcher;

#[test]
fn four_bit_runner_publishes_consistent_screen_metadata() {
    let config = FixtureRunnerConfig {
        addressing_32_bit: false,
        ..FixtureRunnerConfig::default()
    }
    .with_screen_depth(4)
    .expect("4-bit indexed mode should be supported");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
    let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
    let gdevice = runner.bus.read_long(gdevice_handle);
    let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
    let ctab = runner.bus.read_long(runner.bus.read_long(pixmap + 42));

    assert_eq!(runner.dispatcher.screen_mode.1, 416);
    assert_eq!(runner.dispatcher.screen_mode.4, 4);
    assert_eq!(runner.bus.read_word(addr::SCREEN_ROW), 416);
    assert_eq!(runner.bus.read_word(addr::SCREEN_BITS + 4), 416);
    assert_eq!(runner.bus.read_word(pixmap + 4), 0x8000 | 416);
    assert_eq!(runner.bus.read_word(pixmap + 32), 4);
    assert_eq!(runner.bus.read_word(pixmap + 36), 4);
    assert_eq!(runner.bus.read_word(ctab + 6), 15);
    assert_eq!(runner.bus.read_long(gdevice + 42), 0x0082);
    assert!(!runner.bus.addressing_32_bit());
    assert_eq!(runner.dispatcher.mmu_mode, 0);
}

#[test]
fn one_bit_runner_publishes_consistent_screen_metadata() {
    let config = FixtureRunnerConfig::default()
        .with_screen_depth(1)
        .expect("1-bit monochrome mode should be supported");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
    let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
    let gdevice = runner.bus.read_long(gdevice_handle);
    let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
    let ctab = runner.bus.read_long(runner.bus.read_long(pixmap + 42));

    assert_eq!(runner.dispatcher.screen_mode.1, 112);
    assert_eq!(runner.dispatcher.screen_mode.4, 1);
    assert_eq!(runner.bus.read_word(addr::SCREEN_ROW), 112);
    assert_eq!(runner.bus.read_word(addr::SCREEN_BITS + 4), 112);
    assert_eq!(runner.bus.read_word(pixmap + 4), 0x8000 | 112);
    assert_eq!(runner.bus.read_word(pixmap + 32), 1);
    assert_eq!(runner.bus.read_word(pixmap + 36), 1);
    assert_eq!(runner.bus.read_word(ctab + 6), 1);
    assert_eq!(runner.bus.read_long(gdevice + 42), 0x0080);
}

#[test]
fn two_bit_runner_publishes_consistent_screen_metadata() {
    let config = FixtureRunnerConfig::default()
        .with_screen_depth(2)
        .expect("2-bit indexed mode should be supported");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
    let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
    let gdevice = runner.bus.read_long(gdevice_handle);
    let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
    let ctab = runner.bus.read_long(runner.bus.read_long(pixmap + 42));

    assert_eq!(runner.configured_screen_depth(), 2);
    assert_eq!(runner.dispatcher.screen_mode.1, 208);
    assert_eq!(runner.dispatcher.screen_mode.4, 2);
    assert_eq!(runner.bus.read_word(addr::SCREEN_ROW), 208);
    assert_eq!(runner.bus.read_word(addr::SCREEN_BITS + 4), 208);
    assert_eq!(runner.bus.read_word(pixmap + 4), 0x8000 | 208);
    assert_eq!(runner.bus.read_word(pixmap + 32), 2);
    assert_eq!(runner.bus.read_word(pixmap + 36), 2);
    assert_eq!(runner.bus.read_word(ctab + 6), 3);
    assert_eq!(runner.bus.read_long(gdevice + 42), 0x0081);
}

#[test]
fn runner_config_rejects_nonselectable_screen_depths() {
    assert!(FixtureRunnerConfig::default().with_screen_depth(3).is_err());
    assert!(FixtureRunnerConfig::default().with_screen_depth(5).is_err());
}

#[test]
fn runner_config_preserves_architecture_defaults_until_depth_is_explicit() {
    let default_runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    assert_eq!(default_runner.configured_screen_depth(), 8);
    assert_eq!(default_runner.configured_powerpc_screen_depth(), 16);

    for depth in [1, 2, 4, 8] {
        let config = FixtureRunnerConfig::default()
            .with_screen_depth(depth)
            .expect("indexed depth should be supported");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
        assert_eq!(
            runner.configured_powerpc_screen_depth(),
            16,
            "68K config depth {depth} must not become an implicit PPC override"
        );
        runner
            .set_powerpc_screen_depth(depth)
            .expect("PowerPC indexed depth should be supported");
        assert_eq!(runner.configured_screen_depth(), depth);
        assert_eq!(runner.configured_powerpc_screen_depth(), u32::from(depth));
    }

    let direct_nondefault = FixtureRunner::new(
        8 * 1024 * 1024,
        FixtureRunnerConfig {
            screen_depth: 4,
            ..FixtureRunnerConfig::default()
        },
    );
    assert_eq!(direct_nondefault.configured_powerpc_screen_depth(), 16);

    let mut explicit_eight = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    explicit_eight.set_powerpc_screen_depth(8).unwrap();
    assert_eq!(explicit_eight.configured_powerpc_screen_depth(), 8);

    let mut default_runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    assert!(default_runner.set_powerpc_screen_depth(3).is_err());
}

#[test]
fn ppc_host_sync_excludes_scanline_padding_from_visible_rows() {
    for (depth, padded_row_bytes, visible_row_bytes) in [
        (1, 96, 80),
        (2, 176, 160),
        (4, 336, 320),
        (8, 656, 640),
        (16, 1296, 1280),
    ] {
        let front_buffer = PpcFrontBuffer {
            base_addr: 0x1000,
            row_bytes: padded_row_bytes,
            width: 640,
            height: 480,
            depth,
        };
        assert_eq!(
            FixtureRunner::ppc_front_buffer_visible_row_bytes(front_buffer),
            Some(visible_row_bytes),
            "{depth}bpp visible row"
        );
    }
}

#[test]
fn ppc_host_canvas_presents_draw_sprocket_context_without_matte() {
    let context_buffer = PpcFrontBuffer {
        base_addr: 0x1000,
        row_bytes: 1296,
        width: 640,
        height: 480,
        depth: 16,
    };
    // An active DrawSprocket context owns the display mode and is shown 1:1,
    // so the window switches to 640x480 instead of matting the buffer inside
    // the machine profile.
    assert_eq!(
        FixtureRunner::ppc_host_canvas_dimensions(context_buffer, true),
        (640, 480)
    );
    // Other guest buffers still pad to the machine profile.
    let profile = crate::machine_profile::reference_machine_profile();
    assert_eq!(
        FixtureRunner::ppc_host_canvas_dimensions(context_buffer, false),
        (
            640u32.max(u32::from(profile.screen_width)),
            480u32.max(u32::from(profile.screen_height))
        )
    );
}

#[test]
fn ppc_packed_indexed_row_copy_preserves_neighbors_at_non_byte_offsets() {
    let mut one_bit = [0u8; 2];
    assert!(FixtureRunner::copy_ppc_packed_indexed_row(
        &[0b1010_0000],
        1,
        4,
        &mut one_bit,
        3,
    ));
    assert_eq!(one_bit, [0x14, 0x00]);

    let mut two_bit = [0xffu8; 3];
    assert!(FixtureRunner::copy_ppc_packed_indexed_row(
        &[0b00_01_10_11, 0b01_00_00_00],
        2,
        5,
        &mut two_bit,
        1,
    ));
    assert_eq!(two_bit, [0xc6, 0xdf, 0xff]);

    let mut four_bit = [0xaau8; 3];
    assert!(FixtureRunner::copy_ppc_packed_indexed_row(
        &[0x12, 0x30],
        4,
        3,
        &mut four_bit,
        1,
    ));
    assert_eq!(four_bit, [0xa1, 0x23, 0xaa]);
}

#[test]
fn ppc_indexed_matte_repeats_darkest_representable_clut_index() {
    for depth in [1u16, 2, 4, 8] {
        let (clut, _) =
            TrapDispatcher::standard_mac_indexed_clut(depth).expect("standard indexed depth");
        assert_eq!(
            FixtureRunner::ppc_indexed_matte_byte(u32::from(depth), &clut),
            Some(0xff),
            "{depth}bpp standard black index"
        );
    }

    let mut clut = [[0u16; 3]; 256];
    clut[0] = [0xffff, 0xffff, 0xffff];
    clut[1] = [0xaaaa, 0xaaaa, 0xaaaa];
    clut[2] = [0x0000, 0x0000, 0x0000];
    clut[3] = [0x5555, 0x5555, 0x5555];
    assert_eq!(FixtureRunner::ppc_indexed_matte_byte(2, &clut), Some(0xaa));
}

#[test]
fn ppc_host_clut_sync_grows_a_lower_depth_main_color_table() {
    let config = FixtureRunnerConfig::default()
        .with_screen_depth(1)
        .expect("1bpp mode");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
    let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
    let gdevice = runner.bus.read_long(gdevice_handle);
    let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
    let color_table_handle = runner.bus.read_long(pixmap + 42);
    let old_color_table = runner.bus.read_long(color_table_handle);
    let (mut clut, _) = TrapDispatcher::standard_mac_indexed_clut(4).expect("4bpp CLUT");
    clut[7] = [0x1234, 0x5678, 0x9abc];

    assert!(runner.sync_ppc_host_indexed_color_table(4, &clut));
    assert_eq!(runner.dispatcher.device_clut, clut);

    let color_table = runner.bus.read_long(color_table_handle);
    assert_ne!(color_table, old_color_table);
    assert_eq!(runner.bus.get_alloc_size(old_color_table), None);
    assert_eq!(runner.bus.get_alloc_size(color_table), Some(8 + 16 * 8));
    assert_eq!(runner.bus.read_word(color_table + 6), 15);
    assert_eq!(
        [
            runner.bus.read_word(color_table + 8 + 7 * 8 + 2),
            runner.bus.read_word(color_table + 8 + 7 * 8 + 4),
            runner.bus.read_word(color_table + 8 + 7 * 8 + 6),
        ],
        clut[7]
    );
}

#[test]
fn ppc_host_clut_sync_restores_a_changed_table_and_keeps_an_unchanged_one() {
    let config = FixtureRunnerConfig::default()
        .with_screen_depth(8)
        .expect("8bpp mode");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
    let (mut clut, _) = TrapDispatcher::standard_mac_indexed_clut(8).expect("8bpp CLUT");
    clut[200] = [0x1234, 0x5678, 0x9abc];
    assert!(runner.sync_ppc_host_indexed_color_table(8, &clut));
    let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
    let gdevice = runner.bus.read_long(gdevice_handle);
    let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
    let color_table = runner.bus.read_long(runner.bus.read_long(pixmap + 42));
    let installed = runner.bus.read_bytes(color_table, 8 + 256 * 8);

    // A repeated sync over an unchanged table leaves it byte for byte.
    assert!(runner.sync_ppc_host_indexed_color_table(8, &clut));
    assert_eq!(runner.bus.read_bytes(color_table, 8 + 256 * 8), installed);

    // Guest changes to the header or a ColorSpec are overwritten again.
    runner.bus.write_word(color_table + 8 + 200 * 8 + 4, 0);
    runner.bus.write_word(color_table + 4, 0);
    assert!(runner.sync_ppc_host_indexed_color_table(8, &clut));
    assert_eq!(runner.bus.read_bytes(color_table, 8 + 256 * 8), installed);
    assert_eq!(runner.dispatcher.device_clut, clut);
}

#[test]
fn ppc_host_sync_restores_indexed_pm_table_across_direct_color_transitions() {
    const WIDTH: u32 = 8;
    const HEIGHT: u32 = 1;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app.memory.add_region(PPC_HEAP_BASE, vec![0; 16]);
    ppc_app.set_heap_cursor(PPC_HEAP_BASE + 16);
    ppc_app.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: PPC_HEAP_BASE,
        gdevice: PPC_MAIN_GDEVICE,
        width: WIDTH,
        height: HEIGHT,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    });

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
    let gdevice = runner.bus.read_long(gdevice_handle);
    let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
    let indexed_ctab_handle = runner.bus.read_long(pixmap + 42);
    assert_ne!(indexed_ctab_handle, 0);

    runner.sync_ppc_front_buffer_to_host(ppc_app);

    assert_eq!(runner.bus.read_word(pixmap + 30), 16);
    assert_eq!(runner.bus.read_word(pixmap + 32), 16);
    assert_eq!(runner.bus.read_word(pixmap + 34), 3);
    assert_eq!(runner.bus.read_word(pixmap + 36), 5);
    assert_eq!(runner.bus.read_long(pixmap + 42), 0);
    assert_eq!(runner.bus.read_long(gdevice + 42), 0x0084);
    let host_mirror_base = runner.ppc_host_mirror_base;
    let host_mirror_capacity = runner.ppc_host_mirror_capacity;
    assert_eq!(runner.dispatcher.screen_mode.0, host_mirror_base);
    assert_eq!(host_mirror_capacity, 16);
    assert_eq!(runner.bus.get_alloc_size(host_mirror_base), Some(16));
    let canary = runner.bus.alloc(8);
    runner.bus.fill_bytes(canary, 8, 0x5a);

    let (two_bit_clut, _) = TrapDispatcher::standard_mac_indexed_clut(2).expect("2bpp CLUT");
    ppc_app.screen_clut.replace(two_bit_clut);
    ppc_app.color_manager_clut.replace(two_bit_clut);
    ppc_app.gworlds[0].depth = 2;
    ppc_app.gworlds[0].row_bytes = 2;

    runner.sync_ppc_front_buffer_to_host(ppc_app);

    assert_eq!(runner.bus.read_word(pixmap + 30), 0);
    assert_eq!(runner.bus.read_word(pixmap + 32), 2);
    assert_eq!(runner.bus.read_word(pixmap + 34), 1);
    assert_eq!(runner.bus.read_word(pixmap + 36), 2);
    assert_eq!(runner.bus.read_long(pixmap + 42), indexed_ctab_handle);
    let indexed_ctab = runner.bus.read_long(indexed_ctab_handle);
    assert_ne!(indexed_ctab, 0);
    assert_eq!(runner.bus.read_word(indexed_ctab + 6), 3);
    assert_eq!(runner.bus.read_long(gdevice + 42), 0x0081);
    let heap_after_first_transition = runner.bus.heap_bump_ptr();

    for _ in 0..8 {
        ppc_app.gworlds[0].depth = 16;
        ppc_app.gworlds[0].row_bytes = 16;
        runner.sync_ppc_front_buffer_to_host(ppc_app);
        assert_eq!(runner.dispatcher.screen_mode.0, host_mirror_base);
        assert_eq!(runner.ppc_host_mirror_base, host_mirror_base);
        assert_eq!(runner.ppc_host_mirror_capacity, host_mirror_capacity);

        ppc_app.gworlds[0].depth = 2;
        ppc_app.gworlds[0].row_bytes = 2;
        runner.sync_ppc_front_buffer_to_host(ppc_app);
        assert_eq!(runner.dispatcher.screen_mode.0, host_mirror_base);
        assert_eq!(runner.ppc_host_mirror_base, host_mirror_base);
        assert_eq!(runner.ppc_host_mirror_capacity, host_mirror_capacity);
    }

    ppc_app.gworlds[0].depth = 16;
    ppc_app.gworlds[0].row_bytes = 16;
    runner.sync_ppc_front_buffer_to_host(ppc_app);

    assert_eq!(runner.bus.read_word(pixmap + 30), 16);
    assert_eq!(runner.bus.read_word(pixmap + 32), 16);
    assert_eq!(runner.bus.read_word(pixmap + 34), 3);
    assert_eq!(runner.bus.read_word(pixmap + 36), 5);
    assert_eq!(runner.bus.read_long(pixmap + 42), 0);
    assert_eq!(runner.ppc_host_indexed_ctab_handle, indexed_ctab_handle);
    assert_eq!(runner.bus.read_word(indexed_ctab + 6), 3);
    assert_eq!(runner.bus.read_long(gdevice + 42), 0x0084);
    assert_eq!(runner.bus.get_alloc_size(host_mirror_base), Some(16));
    assert_eq!(runner.bus.read_bytes(canary, 8), vec![0x5a; 8]);
    assert_eq!(runner.bus.heap_bump_ptr(), heap_after_first_transition);
}

#[test]
fn ppc_packed_front_buffers_sync_centered_pixels_with_process_color_state() {
    const WIDTH: u32 = 513;
    const HEIGHT: u32 = 342;
    const CANVAS_WIDTH: u32 = 800;
    const CANVAS_HEIGHT: u32 = 600;
    const DESTINATION_X: u32 = (CANVAS_WIDTH - WIDTH) / 2;
    const DESTINATION_Y: u32 = (CANVAS_HEIGHT - HEIGHT) / 2;

    for (depth, source_pixels) in [
        (1u16, [1u8, 0, 1, 0]),
        (2u16, [0u8, 1, 2, 3]),
        (4u16, [1u8, 2, 3, 0]),
    ] {
        let row_bytes = WIDTH.checked_mul(u32::from(depth)).unwrap().div_ceil(8);
        let mut pixels = vec![0u8; (row_bytes * HEIGHT) as usize];
        let field_mask = ((1u16 << depth) - 1) as u8;
        for (x, pixel) in source_pixels.into_iter().enumerate() {
            let bit = x as u32 * u32::from(depth);
            let shift = 8 - u32::from(depth) - (bit & 7);
            pixels[(bit / 8) as usize] |= (pixel & field_mask) << shift;
        }

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app.memory.add_region(PPC_HEAP_BASE, pixels);
        ppc_app.set_heap_cursor(PPC_HEAP_BASE + row_bytes * HEIGHT);
        ppc_app.gworlds.push(PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: PPC_HEAP_BASE,
            gdevice: PPC_MAIN_GDEVICE,
            width: WIDTH,
            height: HEIGHT,
            depth: u32::from(depth),
            row_bytes,
            pixels_locked: false,
            pixels_no_purge: false,
        });
        let config = FixtureRunnerConfig::default()
            .with_screen_depth(depth)
            .expect("packed indexed mode");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
        let mut ppc_app = app.ppc.take().expect("PPC app");
        ppc_app.attach_unconverted_process_services(&mut runner.process_context);
        let (mut device_clut, _) =
            TrapDispatcher::standard_mac_indexed_clut(depth).expect("standard indexed depth");
        device_clut[0][0] = 0xfffe;
        let mut color_manager_clut = device_clut;
        color_manager_clut[0][1] = 0xfffd;
        ppc_app.screen_clut.replace(device_clut);
        ppc_app.color_manager_clut.replace(color_manager_clut);
        ppc_app
            .display_gamma
            .install(crate::display::linear_display_gamma());
        runner.sync_ppc_front_buffer_to_host(&mut ppc_app);

        let (base, host_row_bytes, width, height, host_depth) = runner.dispatcher.screen_mode;
        assert_eq!(
            (width, height, host_depth),
            (CANVAS_WIDTH as u16, CANVAS_HEIGHT as u16, depth)
        );
        assert_eq!(
            host_row_bytes,
            CANVAS_WIDTH * u32::from(depth) / 8,
            "{depth}bpp packed canvas stride"
        );
        assert_eq!(runner.dispatcher.device_clut, device_clut);
        assert_eq!(runner.dispatcher.color_manager_clut, color_manager_clut);
        assert_eq!(
            runner.dispatcher.device_gamma(),
            crate::display::linear_display_gamma()
        );
        let gdevice = runner.bus.read_long(runner.dispatcher.main_gdevice_handle);
        let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
        let color_table = runner.bus.read_long(runner.bus.read_long(pixmap + 42));
        assert_eq!(runner.bus.read_word(color_table + 6), (1u16 << depth) - 1);
        for index in [0u32, (1u32 << depth) - 1] {
            let entry = color_table + 8 + index * 8;
            assert_eq!(
                [
                    runner.bus.read_word(entry + 2),
                    runner.bus.read_word(entry + 4),
                    runner.bus.read_word(entry + 6),
                ],
                device_clut[index as usize],
                "{depth}bpp host GDevice CLUT entry {index}"
            );
        }

        let pixel_at = |x: u32| {
            let bit = x * u32::from(depth);
            let packed = runner
                .bus
                .read_byte(base + DESTINATION_Y * host_row_bytes + bit / 8);
            let shift = 8 - u32::from(depth) - (bit & 7);
            (packed >> shift) & field_mask
        };
        assert_eq!(pixel_at(DESTINATION_X - 1), field_mask);
        for (offset, expected) in source_pixels.into_iter().enumerate() {
            assert_eq!(
                pixel_at(DESTINATION_X + offset as u32),
                expected,
                "{depth}bpp source pixel {offset}"
            );
        }
        assert_eq!(pixel_at(DESTINATION_X + WIDTH), field_mask);
    }
}

#[test]
fn ppc_host_sync_paints_the_matte_only_when_its_geometry_changes() {
    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 480;
    const ROW_BYTES: u32 = WIDTH * 2;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut ppc_app = app.ppc.take().expect("PPC app");
    ppc_app
        .memory
        .add_region(PPC_HEAP_BASE, vec![0x7f; (ROW_BYTES * HEIGHT) as usize]);
    ppc_app.set_heap_cursor(PPC_HEAP_BASE + ROW_BYTES * HEIGHT);
    ppc_app.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: PPC_HEAP_BASE,
        gdevice: PPC_MAIN_GDEVICE,
        width: WIDTH,
        height: HEIGHT,
        depth: 16,
        row_bytes: ROW_BYTES,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    let mut runner = FixtureRunner::new(16 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.dispatcher.ensure_main_gdevice(&mut runner.bus);

    runner.sync_ppc_front_buffer_to_host(&mut ppc_app);
    let (base, _, canvas_width, _, _) = runner.dispatcher.screen_mode;
    assert!(
        u32::from(canvas_width) > WIDTH,
        "the profile canvas must leave a matte around a 640x480 image"
    );
    assert_eq!(runner.bus.read_byte(base), 0, "matte painted on first sync");

    // Nothing else writes the host mirror, so an unchanged matte is left
    // alone rather than repainted on every sync.
    runner.bus.write_byte(base, 0xab);
    runner.sync_ppc_front_buffer_to_host(&mut ppc_app);
    assert_eq!(runner.bus.read_byte(base), 0xab);

    // An image the size of the canvas covers the whole mirror, so returning
    // to the smaller image must paint its matte again.
    let (_, _, _, canvas_height, _) = runner.dispatcher.screen_mode;
    let full_row_bytes = u32::from(canvas_width) * 2;
    let full_base = PPC_HEAP_BASE + ROW_BYTES * HEIGHT;
    ppc_app.memory.add_region(
        full_base,
        vec![0x7f; (full_row_bytes * u32::from(canvas_height)) as usize],
    );
    let small = ppc_app.gworlds[0];
    ppc_app.gworlds[0] = PpcGWorldRecord {
        base_addr: full_base,
        width: u32::from(canvas_width),
        height: u32::from(canvas_height),
        row_bytes: full_row_bytes,
        ..small
    };
    runner.sync_ppc_front_buffer_to_host(&mut ppc_app);
    assert_eq!(runner.bus.read_byte(runner.dispatcher.screen_mode.0), 0x7f);
    ppc_app.gworlds[0] = small;
    runner.sync_ppc_front_buffer_to_host(&mut ppc_app);
    assert_eq!(runner.dispatcher.screen_mode.2, canvas_width);
    assert_eq!(runner.bus.read_byte(runner.dispatcher.screen_mode.0), 0);
}
