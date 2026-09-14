use super::*;

    #[test]
    fn hle_import_runner_get_palette_returns_nil_without_association() {
        let pef = synthetic_pef_with_import(b"GetPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let window_ptr = PPC_DATA_BASE + 0x1000;
        loaded
            .memory
            .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded.cpu.gpr[3] = window_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }

    #[test]
    fn hle_import_runner_new_palette_initializes_entries_from_color_table() {
        let pef = synthetic_pef_with_import(b"NewPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let ctab_handle = PPC_DATA_BASE + 0x1000;
        let ctab_ptr = PPC_DATA_BASE + 0x2000;
        loaded.memory.add_region(ctab_handle, vec![0; 4]);
        loaded.memory.add_region(ctab_ptr, vec![0; 24]);
        loaded.memory.write_u32_be(ctab_handle, ctab_ptr).unwrap();
        loaded.memory.write_u16_be(ctab_ptr + 6, 1).unwrap();
        for (entry, rgb) in [[0x1111, 0x2222, 0x3333], [0xaaaa, 0xbbbb, 0xcccc]]
            .into_iter()
            .enumerate()
        {
            let spec_ptr = ctab_ptr + 8 + entry as u32 * 8;
            loaded.memory.write_u16_be(spec_ptr, entry as u16).unwrap();
            loaded.memory.write_u16_be(spec_ptr + 2, rgb[0]).unwrap();
            loaded.memory.write_u16_be(spec_ptr + 4, rgb[1]).unwrap();
            loaded.memory.write_u16_be(spec_ptr + 6, rgb[2]).unwrap();
        }
        loaded.cpu.gpr[3] = 3;
        loaded.cpu.gpr[4] = ctab_handle;
        loaded.cpu.gpr[5] = 0x0002;
        loaded.cpu.gpr[6] = 0x3456;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let palette_handle = loaded.cpu.gpr[3];
        let palette_ptr = loaded.memory.read_u32_be(palette_handle).unwrap();
        assert_eq!(loaded.memory.read_u16_be(palette_ptr), Some(3));
        for (entry, rgb) in [[0x1111, 0x2222, 0x3333], [0xaaaa, 0xbbbb, 0xcccc]]
            .into_iter()
            .enumerate()
        {
            let info_ptr = palette_ptr + 16 + entry as u32 * 16;
            assert_eq!(loaded.memory.read_u16_be(info_ptr), Some(rgb[0]));
            assert_eq!(loaded.memory.read_u16_be(info_ptr + 2), Some(rgb[1]));
            assert_eq!(loaded.memory.read_u16_be(info_ptr + 4), Some(rgb[2]));
            assert_eq!(loaded.memory.read_u16_be(info_ptr + 6), Some(0x0002));
            assert_eq!(loaded.memory.read_u16_be(info_ptr + 8), Some(0x3456));
        }
        let third_info = palette_ptr + 16 + 2 * 16;
        assert_eq!(loaded.memory.read_u16_be(third_info), Some(0));
        assert_eq!(loaded.memory.read_u16_be(third_info + 6), Some(0x0002));
        assert_eq!(loaded.memory.read_u16_be(third_info + 8), Some(0x3456));
    }

    #[test]
    fn hle_import_runner_copy_palette_resizes_and_copies_color_info() {
        let pef = synthetic_pef_with_import(b"CopyPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let src_palette = ppc_alloc_handle(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            48,
            true,
        );
        let dst_palette = ppc_alloc_handle(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            32,
            true,
        );
        let src_ptr = loaded.memory.read_u32_be(src_palette).unwrap();
        let dst_ptr = loaded.memory.read_u32_be(dst_palette).unwrap();
        loaded.memory.write_u16_be(src_ptr, 2).unwrap();
        loaded.memory.write_u16_be(dst_ptr, 1).unwrap();
        let source_info = src_ptr + 32;
        loaded
            .memory
            .write_bytes(
                source_info,
                &[0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x00, 0x02, 0x44, 0x44],
            )
            .unwrap();
        loaded.cpu.gpr[3] = src_palette;
        loaded.cpu.gpr[4] = dst_palette;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = 2;
        loaded.cpu.gpr[7] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let dst_ptr = loaded.memory.read_u32_be(dst_palette).unwrap();
        assert_eq!(loaded.memory.read_u16_be(dst_ptr), Some(3));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, dst_ptr + 48, 10),
            Some(vec![
                0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x00, 0x02, 0x44, 0x44
            ])
        );
        assert_eq!(
            loaded
                .handles()
                .iter()
                .find(|record| record.handle == dst_palette)
                .map(|record| record.size),
            Some(64)
        );
    }

    #[test]
    fn hle_import_runner_get_new_palette_returns_initialized_resource_copy() {
        let pef = synthetic_pef_with_import(b"GetNewPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let window = PPC_DATA_BASE + 0x3000;
        loaded
            .memory
            .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = window);
        let mut palette_resource = vec![0xbb; 32];
        palette_resource[..2].copy_from_slice(&1u16.to_be_bytes());
        palette_resource[16..22].copy_from_slice(&[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: current_resource_refnum,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"pltt"),
            res_id: 700,
            name: Vec::new(),
            data: palette_resource,
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = 700;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        let palette_handle = loaded.cpu.gpr[3];
        assert_ne!(palette_handle, 0);
        let palette = loaded.memory.read_u32_be(palette_handle).unwrap();
        assert_eq!(loaded.memory.read_u16_be(palette), Some(1));
        for offset in (2..16).step_by(2) {
            assert_eq!(loaded.memory.read_u16_be(palette + offset), Some(0));
        }
        assert_eq!(loaded.memory.read_u16_be(palette + 16), Some(0x1234));
        assert_eq!(loaded.memory.read_u16_be(palette + 18), Some(0x5678));
        assert_eq!(loaded.memory.read_u16_be(palette + 20), Some(0x9abc));
        for offset in 26..32 {
            assert_eq!(loaded.memory.read_u8(palette + offset), Some(0));
        }
        assert_eq!(
            loaded
                .memory
                .read_u32_be(window + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET),
            Some(palette_handle)
        );
        assert_eq!(
            loaded
                .memory
                .read_u16_be(window + PPC_CGRAF_PORT_PALETTE_UPDATES_OFFSET),
            Some(1)
        );
    }

    #[test]
    fn hle_import_runner_ctab_to_palette_resizes_and_copies_entries() {
        let pef = synthetic_pef_with_import(b"CTab2Palette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let ctable_handle = ppc_alloc_handle(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            24,
            true,
        );
        let palette_handle = ppc_alloc_handle(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            32,
            true,
        );
        let ctable_ptr = loaded.memory.read_u32_be(ctable_handle).unwrap();
        let palette_ptr = loaded.memory.read_u32_be(palette_handle).unwrap();
        loaded.memory.write_u16_be(ctable_ptr + 6, 1).unwrap();
        loaded.memory.write_u16_be(palette_ptr, 1).unwrap();
        for (entry, rgb) in [[0x1111, 0x2222, 0x3333], [0xaaaa, 0xbbbb, 0xcccc]]
            .into_iter()
            .enumerate()
        {
            let spec_ptr = ctable_ptr + 8 + entry as u32 * 8;
            loaded.memory.write_u16_be(spec_ptr, entry as u16).unwrap();
            loaded.memory.write_u16_be(spec_ptr + 2, rgb[0]).unwrap();
            loaded.memory.write_u16_be(spec_ptr + 4, rgb[1]).unwrap();
            loaded.memory.write_u16_be(spec_ptr + 6, rgb[2]).unwrap();
        }
        loaded.cpu.gpr[3] = ctable_handle;
        loaded.cpu.gpr[4] = palette_handle;
        loaded.cpu.gpr[5] = 0x0002;
        loaded.cpu.gpr[6] = 0x4567;
        let defaults = TrapDispatcher::standard_mac_8bpp_clut();
        loaded.screen_clut.set_entry(42, [0x1111, 0x2222, 0x3333]);
        loaded.color_manager_clut.set_entry(42, [0x1111, 0x2222, 0x3333]);
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: palette_handle,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            });

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert!(loaded.toolbox_startup.palette_allocations.is_empty());
        assert_eq!(loaded.screen_clut[42], defaults[42]);
        assert_eq!(loaded.color_manager_clut[42], defaults[42]);
        let palette_ptr = loaded.memory.read_u32_be(palette_handle).unwrap();
        assert_eq!(loaded.memory.read_u16_be(palette_ptr), Some(2));
        for (entry, rgb) in [[0x1111, 0x2222, 0x3333], [0xaaaa, 0xbbbb, 0xcccc]]
            .into_iter()
            .enumerate()
        {
            let info_ptr = palette_ptr + 16 + entry as u32 * 16;
            assert_eq!(loaded.memory.read_u16_be(info_ptr), Some(rgb[0]));
            assert_eq!(loaded.memory.read_u16_be(info_ptr + 2), Some(rgb[1]));
            assert_eq!(loaded.memory.read_u16_be(info_ptr + 4), Some(rgb[2]));
            assert_eq!(loaded.memory.read_u16_be(info_ptr + 6), Some(0x0002));
            assert_eq!(loaded.memory.read_u16_be(info_ptr + 8), Some(0x4567));
        }
    }

    #[test]
    fn hle_import_runner_palette_to_ctab_resizes_and_copies_colors() {
        let pef = synthetic_pef_with_import(b"Palette2CTab");
        let mut loaded = load_pef_application(&pef).unwrap();
        let palette_handle = PPC_DATA_BASE + 0x1000;
        let palette_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette_ptr, vec![0; 64]);
        loaded
            .memory
            .write_u32_be(palette_handle, palette_ptr)
            .unwrap();
        loaded.memory.write_u16_be(palette_ptr, 3).unwrap();
        let colors = [
            [0x1111, 0x2222, 0x3333],
            [0x4444, 0x5555, 0x6666],
            [0xaaaa, 0xbbbb, 0xcccc],
        ];
        for (entry, rgb) in colors.into_iter().enumerate() {
            let info_ptr = palette_ptr + 16 + entry as u32 * 16;
            loaded.memory.write_u16_be(info_ptr, rgb[0]).unwrap();
            loaded.memory.write_u16_be(info_ptr + 2, rgb[1]).unwrap();
            loaded.memory.write_u16_be(info_ptr + 4, rgb[2]).unwrap();
            loaded.memory.write_u16_be(info_ptr + 6, 0x0002).unwrap();
            loaded.memory.write_u16_be(info_ptr + 8, 0x1234).unwrap();
        }
        let ctable_handle = ppc_alloc_handle(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            16,
            true,
        );
        assert_ne!(ctable_handle, 0);
        loaded.cpu.gpr[3] = palette_handle;
        loaded.cpu.gpr[4] = ctable_handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let ctable_ptr = loaded.memory.read_u32_be(ctable_handle).unwrap();
        assert!(loaded.memory.read_u32_be(ctable_ptr).unwrap() >= 1024);
        assert_eq!(loaded.memory.read_u16_be(ctable_ptr + 4), Some(0));
        assert_eq!(loaded.memory.read_u16_be(ctable_ptr + 6), Some(2));
        for (entry, rgb) in colors.into_iter().enumerate() {
            let spec_ptr = ctable_ptr + 8 + entry as u32 * 8;
            assert_eq!(loaded.memory.read_u16_be(spec_ptr), Some(entry as u16));
            assert_eq!(loaded.memory.read_u16_be(spec_ptr + 2), Some(rgb[0]));
            assert_eq!(loaded.memory.read_u16_be(spec_ptr + 4), Some(rgb[1]));
            assert_eq!(loaded.memory.read_u16_be(spec_ptr + 6), Some(rgb[2]));
        }
        assert_eq!(
            loaded
                .handles()
                .iter()
                .find(|record| record.handle == ctable_handle)
                .map(|record| record.size),
            Some(32)
        );

        loaded.memory.write_u16_be(palette_ptr, 0).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = palette_handle;
        loaded.cpu.gpr[4] = ctable_handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        let ctable_ptr = loaded.memory.read_u32_be(ctable_handle).unwrap();
        assert_eq!(loaded.memory.read_u16_be(ctable_ptr + 6), Some(u16::MAX));
        assert_eq!(
            loaded
                .handles()
                .iter()
                .find(|record| record.handle == ctable_handle)
                .map(|record| record.size),
            Some(8)
        );
    }

    #[test]
    fn hle_import_runner_handles_palette_association() {
        let pef = synthetic_pef_with_import(b"NSetPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let window_ptr = PPC_DATA_BASE + 0x1000;
        let palette_handle = PPC_DATA_BASE + 0x2000;
        loaded
            .memory
            .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded.cpu.gpr[3] = window_ptr;
        loaded.cpu.gpr[4] = palette_handle;
        loaded.cpu.gpr[5] = 0x1234;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded
                .memory
                .read_u32_be(window_ptr + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET),
            Some(palette_handle)
        );
        assert_eq!(
            loaded
                .memory
                .read_u16_be(window_ptr + PPC_CGRAF_PORT_PALETTE_UPDATES_OFFSET),
            Some(0x1234)
        );

        let pef = synthetic_pef_with_import(b"GetPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded
            .memory
            .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded
            .memory
            .write_u32_be(
                window_ptr + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                palette_handle,
            )
            .unwrap();
        loaded.cpu.gpr[3] = window_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], palette_handle);
    }

    #[test]
    fn application_default_palette_round_trips_and_colors_unassigned_front_window() {
        let pef = synthetic_pef_with_import(b"NSetPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let window = PPC_DATA_BASE + 0x1000;
        let palette_handle = PPC_DATA_BASE + 0x2000;
        let palette = PPC_DATA_BASE + 0x2100;
        loaded
            .memory
            .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded
            .memory
            .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 1)
            .unwrap();
        loaded.gworlds.push(PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: window,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: 0,
            gdevice: PPC_MAIN_GDEVICE,
            width: 1,
            height: 1,
            depth: 8,
            row_bytes: 1,
            pixels_locked: false,
            pixels_no_purge: false,
        });
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 32]);
        loaded.memory.write_u32_be(palette_handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            palette + 16,
            PpcRgbColor {
                red: 0x1234,
                green: 0x5678,
                blue: 0x9abc,
            },
        )
        .unwrap();
        loaded.memory.write_u16_be(palette + 22, 0x0002).unwrap();
        loaded.cpu.gpr[3] = u32::MAX;
        loaded.cpu.gpr[4] = palette_handle;
        loaded.cpu.gpr[5] = 0xc000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.toolbox_startup.application_palette, palette_handle);
        assert_eq!(loaded.toolbox_startup.application_palette_updates, 0xc000);
        assert_eq!(loaded.screen_clut[1], [0x1234, 0x5678, 0x9abc]);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetPalette;
        loaded.cpu.gpr[3] = u32::MAX;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.cpu.gpr[3], palette_handle);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PmForeColor;
        loaded.cpu.gpr[3] = 0;
        loaded.run_with_hle_imports(64);
        assert_eq!(
            loaded.quickdraw_fore_color,
            PpcRgbColor {
                red: 0x1234,
                green: 0x5678,
                blue: 0x9abc,
            }
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].symbol_name = "DisposePalette".to_string();
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QuickDrawCompatibility;
        loaded.cpu.gpr[3] = palette_handle;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.toolbox_startup.application_palette, 0);
        assert_eq!(loaded.toolbox_startup.application_palette_updates, 0);
    }

    #[test]
    fn window_front_transitions_activate_associated_palettes() {
        fn add_window(loaded: &mut PpcLoadedApp, window: u32, visible: bool) {
            loaded
                .memory
                .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
            loaded
                .memory
                .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, u8::from(visible))
                .unwrap();
            loaded.gworlds.push(PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: window,
                pixmap_handle: 0,
                pixmap: 0,
                base_addr: 0,
                gdevice: PPC_MAIN_GDEVICE,
                width: 1,
                height: 1,
                depth: 8,
                row_bytes: 1,
                pixels_locked: false,
                pixels_no_purge: false,
            });
        }
        fn add_palette(loaded: &mut PpcLoadedApp, handle: u32, palette: u32, color: [u16; 3]) {
            loaded.memory.add_region(handle, vec![0; 4]);
            loaded.memory.add_region(palette, vec![0; 32]);
            loaded.memory.write_u32_be(handle, palette).unwrap();
            loaded.memory.write_u16_be(palette, 1).unwrap();
            loaded.memory.write_u16_be(palette + 16, color[0]).unwrap();
            loaded.memory.write_u16_be(palette + 18, color[1]).unwrap();
            loaded.memory.write_u16_be(palette + 20, color[2]).unwrap();
            loaded.memory.write_u16_be(palette + 22, 0x0002).unwrap();
        }
        fn run_target(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) {
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.imports[0].dispatcher_target = target;
            loaded.run_with_hle_imports(64);
        }

        let pef = synthetic_pef_with_import(b"NSetPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let window_b = PPC_DATA_BASE + 0x1000;
        let window_a = PPC_DATA_BASE + 0x2000;
        let palette_a_handle = PPC_DATA_BASE + 0x3000;
        let palette_a = PPC_DATA_BASE + 0x3100;
        let palette_b_handle = PPC_DATA_BASE + 0x4000;
        let palette_b = PPC_DATA_BASE + 0x4100;
        let palette_b2_handle = PPC_DATA_BASE + 0x5000;
        let palette_b2 = PPC_DATA_BASE + 0x5100;
        let color_a = [0x1111, 0x2222, 0x3333];
        let color_b = [0x4444, 0x5555, 0x6666];
        let color_b2 = [0x7777, 0x8888, 0x9999];
        add_window(&mut loaded, window_b, true);
        add_window(&mut loaded, window_a, true);
        add_palette(&mut loaded, palette_a_handle, palette_a, color_a);
        add_palette(&mut loaded, palette_b_handle, palette_b, color_b);
        add_palette(&mut loaded, palette_b2_handle, palette_b2, color_b2);

        loaded.cpu.gpr[3] = window_a;
        loaded.cpu.gpr[4] = palette_a_handle;
        loaded.cpu.gpr[5] = 1;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.screen_clut[1], color_a);
        assert_eq!(
            ppc_front_visible_window(&mut loaded.memory, &loaded.gworlds),
            Some(window_a)
        );

        loaded.cpu.gpr[3] = window_b;
        loaded.cpu.gpr[4] = palette_b_handle;
        loaded.cpu.gpr[5] = 1;
        run_target(&mut loaded, PpcImportDispatcherTarget::NSetPalette);
        assert_eq!(
            loaded.screen_clut[1], color_a,
            "nonfront NSetPalette is deferred"
        );

        loaded.cpu.gpr[3] = window_b;
        run_target(&mut loaded, PpcImportDispatcherTarget::SelectWindow);
        assert_eq!(loaded.screen_clut[1], color_b);
        assert_eq!(*loaded.current_gworld, window_b);
        assert_eq!(
            ppc_front_visible_window(&mut loaded.memory, &loaded.gworlds),
            Some(window_b)
        );

        loaded.cpu.gpr[3] = window_b;
        run_target(&mut loaded, PpcImportDispatcherTarget::HideWindow);
        assert_eq!(loaded.screen_clut[1], color_a);
        assert_eq!(*loaded.current_gworld, window_b);

        loaded.cpu.gpr[3] = window_b;
        loaded.cpu.gpr[4] = palette_b2_handle;
        loaded.cpu.gpr[5] = 1;
        run_target(&mut loaded, PpcImportDispatcherTarget::NSetPalette);
        assert_eq!(loaded.screen_clut[1], color_a);

        loaded.cpu.gpr[3] = window_b;
        run_target(&mut loaded, PpcImportDispatcherTarget::ShowWindow);
        assert_eq!(loaded.screen_clut[1], color_b2);
    }

    #[test]
    fn palette_less_front_window_preserves_direct_device_entries() {
        let pef = synthetic_pef_with_import(b"NewCWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        let direct_clut = std::array::from_fn(|index| {
            let component = (index as u16).wrapping_mul(0x0101);
            [
                component,
                component.rotate_left(3),
                component.rotate_left(7),
            ]
        });
        loaded.screen_clut.replace(direct_clut);
        loaded.color_manager_clut.replace(direct_clut);
        ppc_write_device_color_table(
            &mut loaded.memory,
            PPC_MAIN_GDEVICE,
            &direct_clut,
            &mut loaded.toolbox_startup,
        );

        let scratch = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(scratch, vec![0; 32]);
        ppc_write_rect(&mut loaded.memory, scratch, 100, 100, 200, 300).unwrap();
        ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Registration");
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = scratch;
        loaded.cpu.gpr[5] = scratch + 8;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 1;
        loaded.cpu.gpr[8] = u32::MAX;
        loaded.cpu.gpr[9] = 0;
        loaded.cpu.gpr[10] = 0;

        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);

        assert_eq!(loaded.screen_clut, direct_clut);
        assert_eq!(loaded.color_manager_clut, direct_clut);
        assert!(loaded.toolbox_startup.active_device_palettes.is_empty());
    }

    #[test]
    fn send_close_and_palette_less_front_transitions_activate_next_palette() {
        fn add_window(loaded: &mut PpcLoadedApp, window: u32, visible: bool) {
            loaded
                .memory
                .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
            loaded
                .memory
                .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, u8::from(visible))
                .unwrap();
            loaded.gworlds.push(PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: window,
                pixmap_handle: 0,
                pixmap: 0,
                base_addr: 0,
                gdevice: PPC_MAIN_GDEVICE,
                width: 1,
                height: 1,
                depth: 8,
                row_bytes: 1,
                pixels_locked: false,
                pixels_no_purge: false,
            });
        }
        fn associate_palette(
            loaded: &mut PpcLoadedApp,
            window: u32,
            handle: u32,
            palette: u32,
            color: [u16; 3],
        ) {
            loaded.memory.add_region(handle, vec![0; 4]);
            loaded.memory.add_region(palette, vec![0; 32]);
            loaded.memory.write_u32_be(handle, palette).unwrap();
            loaded.memory.write_u16_be(palette, 1).unwrap();
            loaded.memory.write_u16_be(palette + 16, color[0]).unwrap();
            loaded.memory.write_u16_be(palette + 18, color[1]).unwrap();
            loaded.memory.write_u16_be(palette + 20, color[2]).unwrap();
            loaded.memory.write_u16_be(palette + 22, 0x0002).unwrap();
            loaded
                .memory
                .write_u32_be(window + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, handle)
                .unwrap();
        }
        fn run_target(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) {
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.imports[0].dispatcher_target = target;
            loaded.run_with_hle_imports(64);
        }

        let pef = synthetic_pef_with_import(b"BringToFront");
        let mut loaded = load_pef_application(&pef).unwrap();
        let windows = [
            PPC_DATA_BASE + 0x1000,
            PPC_DATA_BASE + 0x2000,
            PPC_DATA_BASE + 0x3000,
        ];
        let no_palette = PPC_DATA_BASE + 0x4000;
        let colors = [
            [0x1111, 0x2222, 0x3333],
            [0x4444, 0x5555, 0x6666],
            [0x7777, 0x8888, 0x9999],
        ];
        for window in windows {
            add_window(&mut loaded, window, true);
        }
        add_window(&mut loaded, no_palette, false);
        for (index, (window, color)) in windows.into_iter().zip(colors).enumerate() {
            associate_palette(
                &mut loaded,
                window,
                PPC_DATA_BASE + 0x5000 + index as u32 * 0x1000,
                PPC_DATA_BASE + 0x5100 + index as u32 * 0x1000,
                color,
            );
        }
        assert!(with_test_display_cluts!(
            loaded,
            |screen_clut, color_manager_clut| ppc_activate_window_palette(
                &mut loaded.memory,
                windows[2],
                PPC_MAIN_GDEVICE,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        loaded.imports[0].symbol_name = "SendBehind".to_string();
        loaded.cpu.gpr[3] = windows[2];
        loaded.cpu.gpr[4] = windows[0];
        run_target(&mut loaded, PpcImportDispatcherTarget::LegacyWindow);
        assert_eq!(loaded.screen_clut[1], colors[1]);
        assert_eq!(*loaded.current_gworld, PPC_MAIN_GWORLD);

        loaded.imports[0].symbol_name = "BringToFront".to_string();
        loaded.cpu.gpr[3] = windows[2];
        run_target(&mut loaded, PpcImportDispatcherTarget::LegacyWindow);
        assert_eq!(loaded.screen_clut[1], colors[2]);

        loaded.imports[0].symbol_name = "SendBehind".to_string();
        loaded.cpu.gpr[3] = windows[2];
        loaded.cpu.gpr[4] = windows[0];
        run_target(&mut loaded, PpcImportDispatcherTarget::LegacyWindow);
        assert_eq!(loaded.screen_clut[1], colors[1]);

        loaded.cpu.gpr[3] = windows[1];
        run_target(&mut loaded, PpcImportDispatcherTarget::CloseWindow);
        assert_eq!(loaded.screen_clut[1], colors[0]);

        loaded.cpu.gpr[3] = no_palette;
        run_target(&mut loaded, PpcImportDispatcherTarget::ShowWindow);
        let defaults = TrapDispatcher::standard_mac_8bpp_clut();
        assert_eq!(
            loaded.screen_clut, defaults,
            "a palette-less front window uses the built-in default palette"
        );
        assert_eq!(
            ppc_front_visible_window(&mut loaded.memory, &loaded.gworlds),
            Some(no_palette)
        );

        loaded.cpu.gpr[3] = no_palette;
        run_target(&mut loaded, PpcImportDispatcherTarget::HideWindow);
        assert_eq!(loaded.screen_clut[1], colors[0]);

        loaded.cpu.gpr[3] = windows[0];
        run_target(&mut loaded, PpcImportDispatcherTarget::CloseWindow);
        assert_eq!(loaded.screen_clut[1], colors[2]);

        loaded.cpu.gpr[3] = windows[2];
        run_target(&mut loaded, PpcImportDispatcherTarget::CloseWindow);
        assert_eq!(
            loaded.screen_clut, defaults,
            "closing the last visible colored window restores the default palette"
        );
    }

    #[test]
    fn hiding_last_window_preserves_its_reserved_animated_cells() {
        let pef = synthetic_pef_with_import(b"HideWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        let window = PPC_DATA_BASE + 0x1000;
        let handle = PPC_DATA_BASE + 0x2000;
        let palette = PPC_DATA_BASE + 0x2100;
        let color = [0x1234, 0x5678, 0x9abc];
        loaded
            .memory
            .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded
            .memory
            .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 1)
            .unwrap();
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 48]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 2).unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            palette + 32,
            PpcRgbColor {
                red: color[0],
                green: color[1],
                blue: color[2],
            },
        )
        .unwrap();
        loaded.memory.write_u16_be(palette + 38, 0x000c).unwrap();
        loaded
            .memory
            .write_u32_be(window + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, handle)
            .unwrap();
        loaded.gworlds.push(PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: window,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: 0,
            gdevice: PPC_MAIN_GDEVICE,
            width: 1,
            height: 1,
            depth: 8,
            row_bytes: 1,
            pixels_locked: false,
            pixels_no_purge: false,
        });
        assert!(with_test_display_cluts!(
            loaded,
            |screen_clut, color_manager_clut| ppc_activate_window_palette(
                &mut loaded.memory,
                window,
                PPC_MAIN_GDEVICE,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
                &mut loaded.toolbox_startup,
            )
        ));
        assert_eq!(loaded.screen_clut[1], color);

        loaded.cpu.gpr[3] = window;
        loaded.run_with_hle_imports(64);

        assert_eq!(loaded.screen_clut[1], color);
        assert_eq!(loaded.color_manager_clut[1], color);
        assert!(loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .any(|allocation| allocation.palette == handle && allocation.reserved_indices == [1]));
        let device_ctable = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, device_ctable + 10 + 8),
            Some(PpcRgbColor {
                red: color[0],
                green: color[1],
                blue: color[2],
            })
        );
    }

    #[test]
    fn hle_import_runner_palette_replacement_queues_requested_update() {
        let pef = synthetic_pef_with_import(b"NSetPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let palette_handle = PPC_DATA_BASE + 0x1000;
        let palette_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette_ptr, vec![0; 48]);
        loaded
            .memory
            .write_u32_be(palette_handle, palette_ptr)
            .unwrap();
        loaded.memory.write_u16_be(palette_ptr, 2).unwrap();
        for (index, rgb) in [[0x1234, 0x5678, 0x9abc], [0xdef0, 0x1357, 0x2468]]
            .into_iter()
            .enumerate()
        {
            let info = palette_ptr + 16 + index as u32 * 16;
            loaded.memory.write_u16_be(info, rgb[0]).unwrap();
            loaded.memory.write_u16_be(info + 2, rgb[1]).unwrap();
            loaded.memory.write_u16_be(info + 4, rgb[2]).unwrap();
            loaded.memory.write_u16_be(info + 6, 0x0002).unwrap();
        }
        loaded
            .memory
            .write_u32_be(
                PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                0x1234,
            )
            .unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = palette_handle;
        loaded.cpu.gpr[5] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.screen_clut[1], [0x1234, 0x5678, 0x9abc]);
        assert_eq!(
            loaded.display_gamma.table(),
            crate::display::default_display_gamma()
        );
        assert!(loaded
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == PPC_MAIN_GWORLD));
    }

    #[test]
    fn hle_import_runner_pm_fore_color_resolves_palette_usage() {
        for (usage, palette_rgb, expected) in [
            (
                0x0002,
                PpcRgbColor {
                    red: 0x1234,
                    green: 0x5678,
                    blue: 0x9abc,
                },
                PpcRgbColor {
                    red: 0x1234,
                    green: 0x5678,
                    blue: 0x9abc,
                },
            ),
            (
                0x0008,
                PpcRgbColor {
                    red: 0x1234,
                    green: 0x5678,
                    blue: 0x9abc,
                },
                PpcRgbColor {
                    red: 0x1234,
                    green: 0x5678,
                    blue: 0x9abc,
                },
            ),
        ] {
            let pef = synthetic_pef_with_import(b"PmForeColor");
            let mut loaded = load_pef_application(&pef).unwrap();
            let palette_handle = PPC_DATA_BASE + 0x1000;
            let palette_ptr = PPC_DATA_BASE + 0x2000;
            loaded.memory.add_region(palette_handle, vec![0; 4]);
            loaded.memory.add_region(palette_ptr, vec![0; 48]);
            loaded
                .memory
                .write_u32_be(palette_handle, palette_ptr)
                .unwrap();
            loaded.memory.write_u16_be(palette_ptr, 2).unwrap();
            let info_ptr = palette_ptr + 32;
            loaded
                .memory
                .write_u16_be(info_ptr, palette_rgb.red)
                .unwrap();
            loaded
                .memory
                .write_u16_be(info_ptr + 2, palette_rgb.green)
                .unwrap();
            loaded
                .memory
                .write_u16_be(info_ptr + 4, palette_rgb.blue)
                .unwrap();
            loaded.memory.write_u16_be(info_ptr + 6, usage).unwrap();
            loaded
                .memory
                .write_u32_be(
                    PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                    palette_handle,
                )
                .unwrap();
            loaded.cpu.gpr[3] = 1;

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.quickdraw_fore_color, expected);
            assert_eq!(
                loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD).copied(),
                (usage & 0x0008 != 0).then_some(1),
            );
        }
    }

    #[test]
    fn pm_colors_without_a_palette_do_not_borrow_the_active_device_palette() {
        let pef = synthetic_pef_with_import(b"PmForeColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x1800;
        let palette = PPC_DATA_BASE + 0x1900;
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 32]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            palette + 16,
            PpcRgbColor {
                red: 0x1234,
                green: 0x5678,
                blue: 0x9abc,
            },
        )
        .unwrap();
        loaded
            .toolbox_startup
            .active_device_palettes
            .insert(PPC_MAIN_GDEVICE, handle);
        loaded.toolbox_startup.application_palette = 0;
        loaded
            .memory
            .write_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, 0)
            .unwrap();
        let expected_fore = loaded.quickdraw_fore_color;
        let expected_back = loaded.quickdraw_back_color;
        loaded.cpu.gpr[3] = 0;

        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.quickdraw_fore_color, expected_fore);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PmBackColor;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.quickdraw_back_color, expected_back);
    }

    #[test]
    fn animation_without_a_palette_does_not_borrow_active_device_state() {
        let pef = synthetic_pef_with_import(b"AnimatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1a00;
        let handle = scratch;
        let palette = scratch + 0x10;
        let ctable_handle = scratch + 0x40;
        let ctable = scratch + 0x50;
        let color_ptr = scratch + 0x70;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        loaded.memory.write_u16_be(palette + 22, 0x0004).unwrap();
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u16_be(ctable + 6, 0).unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            ctable + 10,
            PpcRgbColor {
                red: 0x1234,
                green: 0x5678,
                blue: 0x9abc,
            },
        )
        .unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            color_ptr,
            PpcRgbColor {
                red: 0xabcd,
                green: 0x2468,
                blue: 0x1357,
            },
        )
        .unwrap();
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: handle,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            });
        loaded
            .toolbox_startup
            .active_device_palettes
            .insert(PPC_MAIN_GDEVICE, handle);
        loaded.toolbox_startup.application_palette = 0;
        loaded
            .memory
            .write_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, 0)
            .unwrap();
        let original = ppc_read_rgb_color(&mut loaded.memory, palette + 16).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = ctable_handle;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 1;

        loaded.run_with_hle_imports(64);
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, palette + 16),
            Some(original)
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].symbol_name = "AnimateEntry".to_string();
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = color_ptr;
        loaded.run_with_hle_imports(64);
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, palette + 16),
            Some(original)
        );
    }

    #[test]
    fn pm_fore_color_explicit_index_reaches_text_and_primitives() {
        let pef = synthetic_pef_with_import(b"PmForeColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        let palette_handle = PPC_DATA_BASE + 0x1000;
        let palette_ptr = PPC_DATA_BASE + 0x2000;
        let entry = 103u16;
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded
            .memory
            .add_region(palette_ptr, vec![0; 16 + 104 * 16]);
        loaded
            .memory
            .write_u32_be(palette_handle, palette_ptr)
            .unwrap();
        loaded.memory.write_u16_be(palette_ptr, 104).unwrap();
        let info_ptr = palette_ptr + 16 + u32::from(entry) * 16;
        loaded.memory.write_u16_be(info_ptr, 0xffff).unwrap();
        loaded.memory.write_u16_be(info_ptr + 2, 0xf331).unwrap();
        loaded.memory.write_u16_be(info_ptr + 4, 0xcccc).unwrap();
        loaded.memory.write_u16_be(info_ptr + 6, 0x000c).unwrap();
        loaded
            .memory
            .write_u32_be(
                PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                palette_handle,
            )
            .unwrap();
        loaded.cpu.gpr[3] = u32::from(entry);

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        let explicit_index = loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD).copied();
        assert_eq!(explicit_index, Some(entry as u8));
        let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        let _ = ppc_draw_text_bytes(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
            (10, 20),
            PPC_QD_TEXT_FONT_DEFAULT,
            12,
            PPC_QD_TEXT_MODE_SRC_OR,
            loaded.quickdraw_fore_color,
            explicit_index,
            b"0",
        );
        let text_has_explicit_pixel = (8..24).any(|y| {
            (8..24).any(|x| {
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y))
                    == Some(u16::from(entry))
            })
        });
        assert!(text_has_explicit_pixel);

        assert!(ppc_paint_rect_bounds(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
            (30, 30, 32, 32),
            loaded.quickdraw_fore_color,
            explicit_index,
        ));
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (30, 30)),
            Some(u16::from(entry)),
        );
    }

    #[test]
    fn rgb_fore_color_resolves_main_8bpp_index_while_legacy_fore_color_clears_override() {
        for symbol in [b"RGBForeColor".as_slice(), b"ForeColor".as_slice()] {
            let pef = synthetic_pef_with_import(symbol);
            let mut loaded = load_pef_application(&pef).unwrap();
            loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
            if symbol == b"RGBForeColor" {
                let color_ptr = PPC_DATA_BASE + 0x1000;
                loaded.memory.add_region(color_ptr, vec![0; 6]);
                ppc_write_rgb_color(&mut loaded.memory, color_ptr, PPC_RGB_WHITE).unwrap();
                loaded.cpu.gpr[3] = color_ptr;
            } else {
                loaded.cpu.gpr[3] = 30;
            }

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.unsupported_import_index, None);
            if symbol == b"RGBForeColor" {
                assert_eq!(
                    loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD),
                    Some(&0),
                    "RGBForeColor must retain the resolved main-screen inverse-table pixel"
                );
            } else {
                assert!(!loaded.quickdraw_fore_indices.contains_key(&PPC_MAIN_GWORLD));
            }
        }
    }

    #[test]
    fn rgb_fore_color_uses_replaced_logical_screen_table_not_hardware_palette() {
        let pef = synthetic_pef_with_import(b"RGBForeColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        let gray = [0x9F9F; 3];
        loaded.color_manager_clut.set_entry(86, [0, 0, 0x9B9B]);
        loaded.color_manager_clut.set_entry(144, gray);
        loaded.screen_clut.set_entry(86, gray);
        loaded.screen_clut.set_entry(144, [0, 0, 0x9B9B]);

        let color_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(color_ptr, vec![0; 6]);
        ppc_write_rgb_color(
            &mut loaded.memory,
            color_ptr,
            PpcRgbColor {
                red: gray[0],
                green: gray[1],
                blue: gray[2],
            },
        )
        .unwrap();
        loaded.cpu.gpr[3] = color_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD),
            Some(&144)
        );
        assert_eq!(loaded.screen_clut[144], [0, 0, 0x9B9B]);
    }

    #[test]
    fn invalid_rgb_fore_color_preserves_explicit_index() {
        let pef = synthetic_pef_with_import(b"RGBForeColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
        loaded.cpu.gpr[3] = 0xffff_fffc;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD),
            Some(&103)
        );
    }

    #[test]
    fn plot_icon_uses_explicit_foreground_index() {
        let pef = synthetic_pef_with_import(b"PlotIcon");
        let mut loaded = load_pef_application(&pef).unwrap();
        let rect = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(rect, vec![0; 8]);
        ppc_write_rect(&mut loaded.memory, rect, 4, 4, 36, 36).unwrap();
        let icon = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &[0xff; 128],
        );
        loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
        loaded.cpu.gpr[3] = rect;
        loaded.cpu.gpr[4] = icon;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (4, 4)),
            Some(103)
        );
    }

    #[test]
    fn hle_import_runner_activates_associated_palette_colors() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let window_ptr = PPC_DATA_BASE + 0x1000;
        let palette_handle = PPC_DATA_BASE + 0x2000;
        let palette_ptr = PPC_DATA_BASE + 0x3000;
        loaded
            .memory
            .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette_ptr, vec![0; 48]);
        loaded
            .memory
            .write_u32_be(palette_handle, palette_ptr)
            .unwrap();
        loaded.memory.write_u16_be(palette_ptr, 2).unwrap();
        for (index, rgb) in [[0x1234, 0x5678, 0x9abc], [0xdef0, 0x1357, 0x2468]]
            .into_iter()
            .enumerate()
        {
            let info_ptr = palette_ptr + 16 + index as u32 * 16;
            loaded.memory.write_u16_be(info_ptr, rgb[0]).unwrap();
            loaded.memory.write_u16_be(info_ptr + 2, rgb[1]).unwrap();
            loaded.memory.write_u16_be(info_ptr + 4, rgb[2]).unwrap();
            loaded.memory.write_u16_be(info_ptr + 6, 0x0002).unwrap();
        }
        loaded
            .memory
            .write_u32_be(
                window_ptr + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                palette_handle,
            )
            .unwrap();
        loaded
            .memory
            .write_u8(window_ptr + PPC_CWINDOW_VISIBLE_OFFSET, 1)
            .unwrap();
        loaded.gworlds.push(PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: window_ptr,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: 0,
            gdevice: PPC_MAIN_GDEVICE,
            width: 1,
            height: 1,
            depth: 8,
            row_bytes: 1,
            pixels_locked: false,
            pixels_no_purge: false,
        });
        loaded.cpu.gpr[3] = window_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.screen_clut[1], [0x1234, 0x5678, 0x9abc]);
        assert_eq!(loaded.screen_clut[2], [0xdef0, 0x1357, 0x2468]);
        assert_eq!(
            loaded.display_gamma.table(),
            crate::display::default_display_gamma()
        );
        let device_ctable = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
        assert_eq!(loaded.memory.read_u16_be(device_ctable + 18), Some(0x1234));
        assert_eq!(loaded.memory.read_u16_be(device_ctable + 20), Some(0x5678));
        assert_eq!(loaded.memory.read_u16_be(device_ctable + 22), Some(0x9abc));
        assert!(loaded
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == window_ptr));
    }

    #[test]
    fn activate_palette_ignores_background_and_offscreen_ports() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let background = PPC_DATA_BASE + 0x1000;
        let front = PPC_DATA_BASE + 0x2000;
        let offscreen = PPC_DATA_BASE + 0x3000;
        let palette_handle = PPC_DATA_BASE + 0x4000;
        let palette = PPC_DATA_BASE + 0x4100;
        for window in [background, front, offscreen] {
            loaded
                .memory
                .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        }
        for window in [background, front] {
            loaded
                .memory
                .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 1)
                .unwrap();
            loaded.gworlds.push(PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: window,
                pixmap_handle: 0,
                pixmap: 0,
                base_addr: 0,
                gdevice: PPC_MAIN_GDEVICE,
                width: 1,
                height: 1,
                depth: 8,
                row_bytes: 1,
                pixels_locked: false,
                pixels_no_purge: false,
            });
        }
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 32]);
        loaded.memory.write_u32_be(palette_handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            palette + 16,
            PpcRgbColor {
                red: 0x1234,
                green: 0x5678,
                blue: 0x9abc,
            },
        )
        .unwrap();
        loaded.memory.write_u16_be(palette + 22, 0x0002).unwrap();
        for window in [background, offscreen] {
            loaded
                .memory
                .write_u32_be(
                    window + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                    palette_handle,
                )
                .unwrap();
        }
        let original = *loaded.screen_clut;

        for window in [background, offscreen] {
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.run_with_hle_imports(64);
            assert_eq!(loaded.screen_clut, original);
            assert!(loaded.toolbox_startup.palette_allocations.is_empty());
        }
    }

    #[test]
    fn palette_allocation_respects_usage_tolerance_and_unrelated_slots() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x5000;
        let palette = PPC_DATA_BASE + 0x5100;
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 16 + 5 * 16]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 5).unwrap();
        let original = *loaded.screen_clut;
        let entries = [
            ([0x3333, 0, 0], 0x0000, 0),
            ([0x4444, 0, 0], 0x0002, 0xffff),
            ([0x1234, 0x5678, 0x9abc], 0x0002, 0),
            ([0xabcd, 0xbcde, 0xcdef], 0x0008, 0),
            ([0x1111, 0x2222, 0x3333], 0x2004, 0),
        ];
        for (entry, (rgb, usage, tolerance)) in entries.into_iter().enumerate() {
            let info = palette + 16 + entry as u32 * 16;
            loaded.memory.write_u16_be(info, rgb[0]).unwrap();
            loaded.memory.write_u16_be(info + 2, rgb[1]).unwrap();
            loaded.memory.write_u16_be(info + 4, rgb[2]).unwrap();
            loaded.memory.write_u16_be(info + 6, usage).unwrap();
            loaded.memory.write_u16_be(info + 8, tolerance).unwrap();
        }
        loaded.toolbox_startup.clut_protected[0] = true;
        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));
        let allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == handle)
            .unwrap();
        assert_eq!(
            allocation.entry_to_index[0],
            Some(pict::closest_clut_index(0x3333, 0, 0, &original))
        );
        assert_eq!(
            allocation.entry_to_index[1],
            Some(pict::closest_clut_index(0x4444, 0, 0, &original))
        );
        assert_eq!(allocation.entry_to_index[2], Some(1));
        assert_eq!(allocation.entry_to_index[3], Some(3));
        assert_eq!(allocation.entry_to_index[4], None);
        assert_eq!(loaded.screen_clut[0], original[0]);
        assert_eq!(
            loaded.screen_clut[3], original[3],
            "pure explicit does not write"
        );
        assert_eq!(loaded.screen_clut[100], original[100]);
    }

    #[test]
    fn palette_matching_excludes_other_animated_reservations() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x5800;
        let palette = PPC_DATA_BASE + 0x5900;
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 32]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            palette + 16,
            PpcRgbColor {
                red: 0x8000,
                green: 0x8000,
                blue: 0x8000,
            },
        )
        .unwrap();
        loaded.screen_clut.fill([0xffff; 3]);
        loaded.screen_clut.set_entry(42, [0x8000; 3]);
        loaded.screen_clut.set_entry(43, [0x8001; 3]);
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: handle + 4,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            });

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));
        let allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == handle)
            .unwrap();
        assert_eq!(allocation.entry_to_index[0], Some(43));
        assert_eq!(
            allocation.entry_mappings[0],
            PpcPaletteEntryMapping::MatchOnly(43)
        );
    }

    #[test]
    fn tolerant_palette_steals_one_same_device_animated_reservation() {
        fn add_palette(
            loaded: &mut PpcLoadedApp,
            handle: u32,
            palette: u32,
            color: [u16; 3],
            usage: u16,
        ) {
            loaded.memory.add_region(handle, vec![0; 4]);
            loaded.memory.add_region(palette, vec![0; 32]);
            loaded.memory.write_u32_be(handle, palette).unwrap();
            loaded.memory.write_u16_be(palette, 1).unwrap();
            ppc_write_rgb_color(
                &mut loaded.memory,
                palette + 16,
                PpcRgbColor {
                    red: color[0],
                    green: color[1],
                    blue: color[2],
                },
            )
            .unwrap();
            loaded.memory.write_u16_be(palette + 22, usage).unwrap();
        }

        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let current = PPC_DATA_BASE + 0x8000;
        let victim_a = PPC_DATA_BASE + 0x8100;
        let victim_b = PPC_DATA_BASE + 0x8200;
        add_palette(
            &mut loaded,
            current,
            PPC_DATA_BASE + 0x8300,
            [0xffff; 3],
            0x0002,
        );
        add_palette(
            &mut loaded,
            victim_a,
            PPC_DATA_BASE + 0x8400,
            [0xffff; 3],
            0x0004,
        );
        add_palette(
            &mut loaded,
            victim_b,
            PPC_DATA_BASE + 0x8500,
            [0, 0x8000, 0],
            0x0004,
        );
        let other_gdevice = PPC_DATA_BASE + 0x9000;
        loaded.screen_clut.fill([0; 3]);
        loaded.screen_clut.set_entry(42, [0xffff; 3]);
        loaded.screen_clut.set_entry(43, [0, 0x8000, 0]);
        loaded.toolbox_startup.clut_protected.fill(true);
        loaded.toolbox_startup.clut_protected[42] = false;
        loaded.toolbox_startup.clut_protected[43] = false;
        loaded.toolbox_startup.palette_allocations.extend([
            PpcPaletteAllocation {
                palette: victim_a,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            },
            PpcPaletteAllocation {
                palette: victim_b,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(43)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(43)],
                reserved_indices: vec![43],
            },
            PpcPaletteAllocation {
                palette: victim_a,
                gdevice: other_gdevice,
                entry_to_index: vec![Some(44)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(44)],
                reserved_indices: vec![44],
            },
        ]);

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                current,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        let current_allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == current)
            .unwrap();
        assert_eq!(
            current_allocation.entry_mappings,
            [PpcPaletteEntryMapping::TolerantInstalled(42)]
        );
        let victim_a_main = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| {
                allocation.palette == victim_a && allocation.gdevice == PPC_MAIN_GDEVICE
            })
            .unwrap();
        assert_eq!(
            victim_a_main.entry_mappings,
            [PpcPaletteEntryMapping::MatchOnly(42)]
        );
        assert_eq!(victim_a_main.entry_to_index, [Some(42)]);
        assert!(victim_a_main.reserved_indices.is_empty());
        let victim_b_main = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == victim_b)
            .unwrap();
        assert_eq!(victim_b_main.reserved_indices, [43]);
        let victim_a_other = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| {
                allocation.palette == victim_a && allocation.gdevice == other_gdevice
            })
            .unwrap();
        assert_eq!(victim_a_other.reserved_indices, [44]);
        assert_eq!(loaded.screen_clut[42], [0xffff; 3]);
        assert_eq!(loaded.screen_clut[43], [0, 0x8000, 0]);
    }

    #[test]
    fn tolerant_palette_uses_an_ordinary_slot_before_stealing() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let current = PPC_DATA_BASE + 0x8600;
        let current_palette = PPC_DATA_BASE + 0x8700;
        let victim = PPC_DATA_BASE + 0x8800;
        let victim_palette = PPC_DATA_BASE + 0x8900;
        for (handle, palette, color, usage) in [
            (current, current_palette, [0xffff; 3], 0x0002),
            (victim, victim_palette, [0x8000, 0, 0], 0x0004),
        ] {
            loaded.memory.add_region(handle, vec![0; 4]);
            loaded.memory.add_region(palette, vec![0; 32]);
            loaded.memory.write_u32_be(handle, palette).unwrap();
            loaded.memory.write_u16_be(palette, 1).unwrap();
            ppc_write_rgb_color(
                &mut loaded.memory,
                palette + 16,
                PpcRgbColor {
                    red: color[0],
                    green: color[1],
                    blue: color[2],
                },
            )
            .unwrap();
            loaded.memory.write_u16_be(palette + 22, usage).unwrap();
        }
        loaded.screen_clut.fill([0; 3]);
        loaded.screen_clut.set_entry(42, [0x8000, 0, 0]);
        loaded.toolbox_startup.clut_protected.fill(true);
        loaded.toolbox_startup.clut_protected[41] = false;
        loaded.toolbox_startup.clut_protected[42] = false;
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: victim,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            });

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                current,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        let current_allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == current)
            .unwrap();
        assert_eq!(
            current_allocation.entry_mappings,
            [PpcPaletteEntryMapping::TolerantInstalled(41)]
        );
        let victim_allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == victim)
            .unwrap();
        assert_eq!(
            victim_allocation.entry_mappings,
            [PpcPaletteEntryMapping::AnimatedReserved(42)]
        );
        assert_eq!(victim_allocation.reserved_indices, [42]);
    }

    #[test]
    fn palette_reactivation_preserves_a_slot_owned_by_another_allocation() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let current = PPC_DATA_BASE + 0x8a00;
        let palette = PPC_DATA_BASE + 0x8b00;
        loaded.memory.add_region(current, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 32]);
        loaded.memory.write_u32_be(current, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            palette + 16,
            PpcRgbColor {
                red: 0x1111,
                green: 0x2222,
                blue: 0x3333,
            },
        )
        .unwrap();
        loaded.memory.write_u16_be(palette + 22, 0x0002).unwrap();
        loaded.memory.write_u16_be(palette + 24, 0xffff).unwrap();
        let other = current + 4;
        loaded.screen_clut.set_entry(42, [0x1111, 0x2222, 0x3333]);
        loaded.toolbox_startup.palette_allocations.extend([
            PpcPaletteAllocation {
                palette: current,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::TolerantInstalled(42)],
                reserved_indices: vec![],
            },
            PpcPaletteAllocation {
                palette: other,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::TolerantInstalled(42)],
                reserved_indices: vec![],
            },
        ]);

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                current,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        assert_eq!(loaded.screen_clut[42], [0x1111, 0x2222, 0x3333]);
        assert!(loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .any(|allocation| allocation.palette == other
                && allocation.entry_mappings == [PpcPaletteEntryMapping::TolerantInstalled(42)]));
    }

    #[test]
    fn releasing_inactive_tolerant_ownership_does_not_restore_an_active_cell() {
        let pef = synthetic_pef_with_import(b"DisposePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let inactive = PPC_DATA_BASE + 0x8c00;
        let active = PPC_DATA_BASE + 0x8d00;
        let color = [0x1234, 0x5678, 0x9abc];
        loaded.screen_clut.set_entry(42, color);
        loaded.color_manager_clut.set_entry(42, color);
        loaded.toolbox_startup.palette_allocations.extend([
            PpcPaletteAllocation {
                palette: inactive,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::TolerantInstalled(42)],
                reserved_indices: vec![],
            },
            PpcPaletteAllocation {
                palette: active,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::TolerantInstalled(42)],
                reserved_indices: vec![],
            },
        ]);
        loaded
            .toolbox_startup
            .active_device_palettes
            .insert(PPC_MAIN_GDEVICE, active);

        with_test_display_cluts!(loaded, |screen_clut, color_manager_clut| {
            ppc_release_palette_allocations_and_restore(
                &mut loaded.memory,
                &mut loaded.toolbox_startup,
                inactive,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
            )
        });

        assert_eq!(loaded.screen_clut[42], color);
        assert_eq!(loaded.color_manager_clut[42], color);

        let replacement = PPC_DATA_BASE + 0x8d10;
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: replacement,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::TolerantInstalled(42)],
                reserved_indices: vec![],
            });
        loaded
            .toolbox_startup
            .active_device_palettes
            .insert(PPC_MAIN_GDEVICE, active);
        with_test_display_cluts!(loaded, |screen_clut, color_manager_clut| {
            ppc_release_palette_allocations_and_restore(
                &mut loaded.memory,
                &mut loaded.toolbox_startup,
                active,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
            )
        });
        assert_eq!(loaded.screen_clut[42], color);
        assert!(loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .any(|allocation| allocation.palette == replacement));
    }

    #[test]
    fn protected_and_global_reservations_are_not_stolen() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let current = PPC_DATA_BASE + 0x8e00;
        let palette = PPC_DATA_BASE + 0x8f00;
        loaded.memory.add_region(current, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 32]);
        loaded.memory.write_u32_be(current, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        for offset in [16, 18, 20] {
            loaded
                .memory
                .write_u16_be(palette + offset, 0xffff)
                .unwrap();
        }
        loaded.memory.write_u16_be(palette + 22, 0x0002).unwrap();
        loaded.screen_clut.fill([0; 3]);
        loaded.toolbox_startup.clut_protected.fill(true);
        loaded.toolbox_startup.clut_protected[42] = false;
        loaded.toolbox_startup.clut_reserved[42] = true;
        for (victim, victim_palette, index) in [
            (PPC_DATA_BASE + 0x9a00, PPC_DATA_BASE + 0x9c00, 42),
            (PPC_DATA_BASE + 0x9b00, PPC_DATA_BASE + 0x9d00, 43),
        ] {
            loaded.memory.add_region(victim, vec![0; 4]);
            loaded.memory.add_region(victim_palette, vec![0; 32]);
            loaded.memory.write_u32_be(victim, victim_palette).unwrap();
            loaded.memory.write_u16_be(victim_palette, 1).unwrap();
            loaded
                .toolbox_startup
                .palette_allocations
                .push(PpcPaletteAllocation {
                    palette: victim,
                    gdevice: PPC_MAIN_GDEVICE,
                    entry_to_index: vec![Some(index)],
                    entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(index)],
                    reserved_indices: vec![index],
                });
        }

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                current,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        let current_allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == current)
            .unwrap();
        assert!(matches!(
            current_allocation.entry_mappings[0],
            PpcPaletteEntryMapping::MatchOnly(_)
        ));
        assert!(loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .filter(|allocation| allocation.palette != current)
            .all(|allocation| !allocation.reserved_indices.is_empty()));
    }

    #[test]
    fn tolerant_explicit_fallback_can_steal_its_requested_index() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let current = PPC_DATA_BASE + 0x9100;
        let palette = PPC_DATA_BASE + 0x9200;
        let victim = PPC_DATA_BASE + 0x9300;
        let victim_palette = PPC_DATA_BASE + 0x9400;
        loaded.memory.add_region(current, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 16 + 43 * 16]);
        loaded.memory.add_region(victim, vec![0; 4]);
        loaded.memory.add_region(victim_palette, vec![0; 32]);
        loaded.memory.write_u32_be(current, palette).unwrap();
        loaded.memory.write_u16_be(palette, 43).unwrap();
        loaded.memory.write_u32_be(victim, victim_palette).unwrap();
        loaded.memory.write_u16_be(victim_palette, 1).unwrap();
        let info = palette + 16 + 42 * 16;
        for offset in [0, 2, 4] {
            loaded.memory.write_u16_be(info + offset, 0xffff).unwrap();
        }
        loaded.memory.write_u16_be(info + 6, 0x000a).unwrap();
        loaded.screen_clut.fill([0; 3]);
        loaded.toolbox_startup.clut_protected.fill(true);
        loaded.toolbox_startup.clut_protected[42] = false;
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: victim,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            });

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                current,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        let allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == current)
            .unwrap();
        assert_eq!(
            allocation.entry_mappings[42],
            PpcPaletteEntryMapping::TolerantInstalled(42)
        );
        assert!(loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == victim)
            .unwrap()
            .reserved_indices
            .is_empty());
    }

    #[test]
    fn stealing_skips_a_malformed_lower_index_victim() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let current = PPC_DATA_BASE + 0x9500;
        let palette = PPC_DATA_BASE + 0x9600;
        let valid_victim = PPC_DATA_BASE + 0x9700;
        let victim_palette = PPC_DATA_BASE + 0x9800;
        loaded.memory.add_region(current, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 32]);
        loaded.memory.add_region(valid_victim, vec![0; 4]);
        loaded.memory.add_region(victim_palette, vec![0; 32]);
        loaded.memory.write_u32_be(current, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        loaded
            .memory
            .write_u32_be(valid_victim, victim_palette)
            .unwrap();
        loaded.memory.write_u16_be(victim_palette, 1).unwrap();
        for offset in [16, 18, 20] {
            loaded
                .memory
                .write_u16_be(palette + offset, 0xffff)
                .unwrap();
        }
        loaded.memory.write_u16_be(palette + 22, 0x0002).unwrap();
        loaded.screen_clut.fill([0; 3]);
        loaded.toolbox_startup.clut_protected.fill(true);
        loaded.toolbox_startup.clut_protected[41] = false;
        loaded.toolbox_startup.clut_protected[42] = false;
        let malformed_victim = PPC_DATA_BASE + 0x9900;
        loaded.toolbox_startup.palette_allocations.extend([
            PpcPaletteAllocation {
                palette: malformed_victim,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(41)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(41)],
                reserved_indices: vec![41],
            },
            PpcPaletteAllocation {
                palette: valid_victim,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            },
        ]);

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                current,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        assert_eq!(loaded.screen_clut[42], [0xffff; 3]);
        assert_eq!(
            loaded
                .toolbox_startup
                .palette_allocations
                .iter()
                .find(|allocation| allocation.palette == malformed_victim)
                .unwrap()
                .reserved_indices,
            [41]
        );
        assert!(loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == valid_victim)
            .unwrap()
            .reserved_indices
            .is_empty());
    }

    #[test]
    fn noncurrent_device_activation_and_steal_use_its_own_color_table() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0xa000;
        let window = scratch;
        let current = scratch + 0x100;
        let current_palette = scratch + 0x110;
        let victim = scratch + 0x200;
        let victim_palette = scratch + 0x210;
        let other_gdevice = scratch + 0x1800;
        let other_device = scratch + 0x1810;
        let pixmap_handle = scratch + 0x1850;
        let pixmap = scratch + 0x1860;
        let ctable_handle = scratch + 0x18a0;
        let ctable = scratch + 0x1900;
        loaded.memory.add_region(scratch, vec![0; 0x3000]);
        loaded
            .memory
            .write_u32_be(current, current_palette)
            .unwrap();
        loaded.memory.write_u16_be(current_palette, 1).unwrap();
        let installed = [0x1234, 0x5678, 0x9abc];
        for (offset, component) in [16, 18, 20].into_iter().zip(installed) {
            loaded
                .memory
                .write_u16_be(current_palette + offset, component)
                .unwrap();
        }
        loaded
            .memory
            .write_u16_be(current_palette + 22, 0x0002)
            .unwrap();
        loaded.memory.write_u32_be(victim, victim_palette).unwrap();
        loaded.memory.write_u16_be(victim_palette, 254).unwrap();
        loaded
            .memory
            .write_u32_be(other_gdevice, other_device)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_device + 22, pixmap_handle)
            .unwrap();
        loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
        loaded
            .memory
            .write_u32_be(pixmap + 42, ctable_handle)
            .unwrap();
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u16_be(ctable + 6, 255).unwrap();
        let mut other_clut = TrapDispatcher::standard_mac_8bpp_clut();
        other_clut[1] = [0x8000, 0, 0];
        ppc_write_device_color_table(
            &mut loaded.memory,
            other_gdevice,
            &other_clut,
            &mut loaded.toolbox_startup,
        );
        loaded
            .memory
            .write_u32_be(window + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, current)
            .unwrap();
        let mappings = (1..=254)
            .map(|index| PpcPaletteEntryMapping::AnimatedReserved(index))
            .collect::<Vec<_>>();
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: victim,
                gdevice: other_gdevice,
                entry_to_index: (1..=254).map(Some).collect(),
                entry_mappings: mappings,
                reserved_indices: (1..=254).collect(),
            });
        loaded.screen_clut.set_entry(1, [0xaaaa, 0xbbbb, 0xcccc]);
        loaded.color_manager_clut.set_entry(1, [0xaaaa, 0xbbbb, 0xcccc]);
        loaded.toolbox_startup.clut_protected.fill(true);
        let expected_screen = *loaded.screen_clut;
        let expected_manager = *loaded.color_manager_clut;

        assert!(with_test_display_cluts!(
            loaded,
            |screen_clut, color_manager_clut| ppc_activate_window_palette(
                &mut loaded.memory,
                window,
                other_gdevice,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        assert_eq!(loaded.screen_clut, expected_screen);
        assert_eq!(loaded.color_manager_clut, expected_manager);
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, ctable + 10 + 8),
            Some(PpcRgbColor {
                red: installed[0],
                green: installed[1],
                blue: installed[2],
            })
        );
        let victim_allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == victim && allocation.gdevice == other_gdevice)
            .unwrap();
        assert!(!victim_allocation.reserved_indices.contains(&1));
        assert_eq!(victim_allocation.reserved_indices.len(), 253);
    }

    #[test]
    fn restore_device_clut_targets_one_device_and_nil_restores_all() {
        let pef = synthetic_pef_with_import(b"RestoreDeviceClut");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0xd000;
        let other_gdevice = scratch;
        let other_device = scratch + 0x10;
        let pixmap_handle = scratch + 0x50;
        let pixmap = scratch + 0x60;
        let ctable_handle = scratch + 0xa0;
        let ctable = scratch + 0x100;
        loaded.memory.add_region(scratch, vec![0; 0x1000]);
        loaded
            .memory
            .write_u32_be(other_gdevice, other_device)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_device + 22, pixmap_handle)
            .unwrap();
        loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
        loaded
            .memory
            .write_u32_be(pixmap + 42, ctable_handle)
            .unwrap();
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u16_be(ctable + 6, 255).unwrap();
        let canonical = TrapDispatcher::standard_mac_8bpp_clut();
        let mut other_clut = canonical;
        other_clut[42] = [0x1111, 0x2222, 0x3333];
        ppc_write_device_color_table(
            &mut loaded.memory,
            other_gdevice,
            &other_clut,
            &mut loaded.toolbox_startup,
        );
        loaded.screen_clut.set_entry(42, [0xaaaa, 0xbbbb, 0xcccc]);
        loaded.color_manager_clut.set_entry(42, [0xaaaa, 0xbbbb, 0xcccc]);
        let expected_screen = *loaded.screen_clut;
        let expected_manager = *loaded.color_manager_clut;

        with_test_display_cluts!(loaded, |screen_clut, color_manager_clut| {
            ppc_restore_device_clut(
                &mut loaded.memory,
                other_gdevice,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
                &mut loaded.toolbox_startup,
            )
        });

        assert_eq!(loaded.screen_clut, expected_screen);
        assert_eq!(loaded.color_manager_clut, expected_manager);
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, ctable + 10 + 42 * 8),
            Some(PpcRgbColor {
                red: canonical[42][0],
                green: canonical[42][1],
                blue: canonical[42][2],
            })
        );

        let second_custom = [0x4444, 0x5555, 0x6666];
        loaded.screen_clut.set_entry(43, second_custom);
        other_clut[43] = second_custom;
        ppc_write_device_color_table(
            &mut loaded.memory,
            other_gdevice,
            &other_clut,
            &mut loaded.toolbox_startup,
        );
        ppc_register_gdevice(&mut loaded.toolbox_startup, other_gdevice);
        assert!(!loaded
            .toolbox_startup
            .active_device_palettes
            .contains_key(&other_gdevice));
        assert!(!loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .any(|allocation| allocation.gdevice == other_gdevice));
        assert!(!loaded
            .toolbox_startup
            .clut_protected_by_device
            .contains_key(&other_gdevice));
        assert!(!loaded
            .toolbox_startup
            .clut_reserved_by_device
            .contains_key(&other_gdevice));
        with_test_display_cluts!(loaded, |screen_clut, color_manager_clut| {
            ppc_restore_device_clut(
                &mut loaded.memory,
                0,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
                &mut loaded.toolbox_startup,
            )
        });

        assert_eq!(loaded.screen_clut, canonical);
        assert_eq!(loaded.color_manager_clut, canonical);
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, ctable + 10 + 43 * 8),
            Some(PpcRgbColor {
                red: canonical[43][0],
                green: canonical[43][1],
                blue: canonical[43][2],
            })
        );
    }

    #[test]
    fn palette_less_activation_preserves_per_device_protected_and_reserved_cells() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0xe000;
        let window = scratch;
        let other_gdevice = scratch + 0x100;
        let other_device = scratch + 0x110;
        let pixmap_handle = scratch + 0x150;
        let pixmap = scratch + 0x160;
        let ctable_handle = scratch + 0x1a0;
        let ctable = scratch + 0x200;
        loaded.memory.add_region(scratch, vec![0; 0x1200]);
        loaded
            .memory
            .write_u32_be(other_gdevice, other_device)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_device + 22, pixmap_handle)
            .unwrap();
        loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
        loaded
            .memory
            .write_u32_be(pixmap + 42, ctable_handle)
            .unwrap();
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u16_be(ctable + 6, 255).unwrap();
        let canonical = TrapDispatcher::standard_mac_8bpp_clut();
        let protected_color = [0x1111, 0x2222, 0x3333];
        let reserved_color = [0x4444, 0x5555, 0x6666];
        let mut other_clut = canonical;
        other_clut[42] = protected_color;
        other_clut[43] = reserved_color;
        other_clut[44] = [0x7777, 0x8888, 0x9999];
        ppc_write_device_color_table(
            &mut loaded.memory,
            other_gdevice,
            &other_clut,
            &mut loaded.toolbox_startup,
        );
        ppc_device_clut_protected_mut(&mut loaded.toolbox_startup, other_gdevice)[42] = true;
        ppc_device_clut_reserved_mut(&mut loaded.toolbox_startup, other_gdevice)[43] = true;
        let expected_screen = *loaded.screen_clut;

        assert!(with_test_display_cluts!(
            loaded,
            |screen_clut, color_manager_clut| ppc_activate_window_palette(
                &mut loaded.memory,
                window,
                other_gdevice,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        assert_eq!(loaded.screen_clut, expected_screen);
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, ctable + 10 + 42 * 8),
            Some(PpcRgbColor {
                red: protected_color[0],
                green: protected_color[1],
                blue: protected_color[2],
            })
        );
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, ctable + 10 + 43 * 8),
            Some(PpcRgbColor {
                red: reserved_color[0],
                green: reserved_color[1],
                blue: reserved_color[2],
            })
        );
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, ctable + 10 + 44 * 8),
            Some(PpcRgbColor {
                red: canonical[44][0],
                green: canonical[44][1],
                blue: canonical[44][2],
            })
        );

        ppc_device_clut_protected_mut(&mut loaded.toolbox_startup, other_gdevice)[42] = false;
        ppc_device_clut_reserved_mut(&mut loaded.toolbox_startup, other_gdevice)[43] = false;
        assert!(with_test_display_cluts!(
            loaded,
            |screen_clut, color_manager_clut| ppc_activate_window_palette(
                &mut loaded.memory,
                window,
                other_gdevice,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
                &mut loaded.toolbox_startup,
            )
        ));
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, ctable + 10 + 42 * 8),
            Some(PpcRgbColor {
                red: canonical[42][0],
                green: canonical[42][1],
                blue: canonical[42][2],
            })
        );
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, ctable + 10 + 43 * 8),
            Some(PpcRgbColor {
                red: canonical[43][0],
                green: canonical[43][1],
                blue: canonical[43][2],
            })
        );
    }

    #[test]
    fn combined_explicit_fallbacks_preserve_ownership_semantics() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x5a00;
        let palette = PPC_DATA_BASE + 0x5b00;
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 48]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 2).unwrap();
        for (entry, usage) in [0x000c, 0x000a].into_iter().enumerate() {
            let info = palette + 16 + entry as u32 * 16;
            ppc_write_rgb_color(
                &mut loaded.memory,
                info,
                PpcRgbColor {
                    red: 0x8000,
                    green: 0x8000,
                    blue: 0x8000,
                },
            )
            .unwrap();
            loaded.memory.write_u16_be(info + 6, usage).unwrap();
        }
        loaded.toolbox_startup.clut_protected[1] = true;

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));
        let allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == handle)
            .unwrap();
        assert!(matches!(
            allocation.entry_mappings[0],
            PpcPaletteEntryMapping::MatchOnly(_)
        ));
        assert_eq!(
            allocation.entry_mappings[1],
            PpcPaletteEntryMapping::TolerantInstalled(2)
        );
        assert!(allocation.reserved_indices.is_empty());
    }

    #[test]
    fn animated_explicit_entry_above_255_wraps_to_8_bit_device_index() {
        let pef = synthetic_pef_with_import(b"AnimatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x5a00;
        let palette = PPC_DATA_BASE + 0x5b00;
        let ctable_handle = PPC_DATA_BASE + 0x7000;
        let ctable = PPC_DATA_BASE + 0x7100;
        let color = [0x1234, 0x5678, 0x9abc];
        let animated = [0xabcd, 0x2468, 0x1357];
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 16 + 301 * 16]);
        loaded.memory.add_region(ctable_handle, vec![0; 4]);
        loaded.memory.add_region(ctable, vec![0; 16]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 301).unwrap();
        let info = palette + 16 + 300 * 16;
        loaded.memory.write_u16_be(info, color[0]).unwrap();
        loaded.memory.write_u16_be(info + 2, color[1]).unwrap();
        loaded.memory.write_u16_be(info + 4, color[2]).unwrap();
        loaded.memory.write_u16_be(info + 6, 0x000c).unwrap();
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u16_be(ctable + 6, 0).unwrap();
        loaded
            .memory
            .write_u16_be(ctable + 10, animated[0])
            .unwrap();
        loaded
            .memory
            .write_u16_be(ctable + 12, animated[1])
            .unwrap();
        loaded
            .memory
            .write_u16_be(ctable + 14, animated[2])
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                handle,
            )
            .unwrap();

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        let allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == handle)
            .unwrap();
        assert_eq!(allocation.entry_mappings.len(), 301);
        assert_eq!(
            allocation.entry_mappings[300],
            PpcPaletteEntryMapping::AnimatedReserved(44)
        );
        assert_eq!(loaded.screen_clut[44], color);

        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = ctable_handle;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 300;
        loaded.cpu.gpr[7] = 1;
        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.screen_clut[44], animated);
        assert_eq!(loaded.color_manager_clut[44], animated);
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, info),
            Some(PpcRgbColor {
                red: animated[0],
                green: animated[1],
                blue: animated[2],
            })
        );
    }

    #[test]
    fn invalid_palette_reactivation_is_atomic() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x5c00;
        let palette = PPC_DATA_BASE + 0x5d00;
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 32]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 2).unwrap();
        let original_clut = *loaded.screen_clut;
        loaded.screen_clut.set_entry(42, [0x1234, 0x5678, 0x9abc]);
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: handle,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            });
        let expected_clut = *loaded.screen_clut;
        let expected_allocations = loaded.toolbox_startup.palette_allocations.clone();

        assert!(!with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        assert_ne!(expected_clut, original_clut);
        assert_eq!(loaded.screen_clut, expected_clut);
        assert_eq!(
            loaded.toolbox_startup.palette_allocations,
            expected_allocations
        );
    }

    #[test]
    fn same_palette_matching_excludes_an_earlier_animated_claim() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x5e00;
        let palette = PPC_DATA_BASE + 0x5f00;
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 16 + 43 * 16]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 43).unwrap();
        loaded.screen_clut.fill([0xffff; 3]);
        loaded.screen_clut.set_entry(42, [0x8000; 3]);
        loaded.screen_clut.set_entry(43, [0x8001; 3]);
        for entry in [0u32, 42] {
            ppc_write_rgb_color(
                &mut loaded.memory,
                palette + 16 + entry * 16,
                PpcRgbColor {
                    red: 0x8000,
                    green: 0x8000,
                    blue: 0x8000,
                },
            )
            .unwrap();
        }
        loaded
            .memory
            .write_u16_be(palette + 16 + 42 * 16 + 6, 0x000c)
            .unwrap();

        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        let allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == handle)
            .unwrap();
        assert_eq!(
            allocation.entry_mappings[42],
            PpcPaletteEntryMapping::AnimatedReserved(42)
        );
        assert_eq!(
            allocation.entry_mappings[0],
            PpcPaletteEntryMapping::MatchOnly(43)
        );
    }

    #[test]
    fn identical_animated_entries_keep_distinct_stable_device_indices() {
        let pef = synthetic_pef_with_import(b"AnimatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x6000;
        let palette = PPC_DATA_BASE + 0x6100;
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 16 + 256 * 16]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 256).unwrap();
        for entry in 0..256u32 {
            let info = palette + 16 + entry * 16;
            ppc_write_rgb_color(&mut loaded.memory, info, PPC_RGB_WHITE).unwrap();
            loaded.memory.write_u16_be(info + 6, 0x0004).unwrap();
        }
        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));
        let before = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == handle)
            .unwrap()
            .entry_to_index
            .clone();
        let mut expected = (0..256).map(|entry| Some(entry as u8)).collect::<Vec<_>>();
        expected[255] = Some(0);
        assert_eq!(before, expected);
        let allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == handle)
            .unwrap();
        assert_eq!(allocation.reserved_indices, (1..=254).collect::<Vec<_>>());

        let ctable_handle = PPC_DATA_BASE + 0x8000;
        let ctable = PPC_DATA_BASE + 0x8100;
        loaded.memory.add_region(ctable_handle, vec![0; 4]);
        loaded.memory.add_region(ctable, vec![0; 8 + 256 * 8]);
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u16_be(ctable + 4, 0x4000).unwrap();
        loaded.memory.write_u16_be(ctable + 6, 255).unwrap();
        for entry in 0..256u32 {
            loaded
                .memory
                .write_u16_be(ctable + 8 + entry * 8, entry as u16)
                .unwrap();
        }
        loaded
            .memory
            .write_u32_be(
                PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                handle,
            )
            .unwrap();
        let linked = ppc_copy_bits_palette_index_map(
            &mut loaded.memory,
            ctable_handle,
            PPC_MAIN_GWORLD,
            PPC_MAIN_GDEVICE,
            &loaded.toolbox_startup,
            &loaded.screen_clut,
        )
        .unwrap();
        let mut expected_linked = std::array::from_fn(|entry| entry as u8);
        expected_linked[255] = 0;
        assert_eq!(linked, expected_linked);

        for entry in 0..256u32 {
            let info = palette + 16 + entry * 16;
            let color = PpcRgbColor {
                red: entry as u16 * 257,
                green: (255 - entry as u16) * 257,
                blue: 0x5555,
            };
            ppc_write_rgb_color(&mut loaded.memory, info, color).unwrap();
        }
        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));
        let after = &loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == handle)
            .unwrap()
            .entry_to_index;
        assert_eq!(&after[1..255], &before[1..255]);
        assert_eq!(loaded.screen_clut[24], [0x1818, 0xe7e7, 0x5555]);
        assert_eq!(loaded.screen_clut[245], [0xf5f5, 0x0a0a, 0x5555]);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].symbol_name = "DisposePalette".to_string();
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::QuickDrawCompatibility;
        loaded.cpu.gpr[3] = handle;
        loaded.run_with_hle_imports(64);
        assert!(loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .all(|allocation| allocation.palette != handle));
        assert_eq!(
            loaded.screen_clut[24],
            TrapDispatcher::standard_mac_8bpp_clut()[24]
        );
    }

    #[test]
    fn reactivation_restores_reservations_removed_by_usage_change() {
        let pef = synthetic_pef_with_import(b"ActivatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x6000;
        let palette = PPC_DATA_BASE + 0x6100;
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 48]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 2).unwrap();
        for entry in 0..2u32 {
            let info = palette + 16 + entry * 16;
            ppc_write_rgb_color(
                &mut loaded.memory,
                info,
                PpcRgbColor {
                    red: 0x1111 + entry as u16,
                    green: 0x2222,
                    blue: 0x3333,
                },
            )
            .unwrap();
            loaded.memory.write_u16_be(info + 6, 0x0004).unwrap();
        }
        let defaults = TrapDispatcher::standard_mac_8bpp_clut();
        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));
        assert_ne!(loaded.screen_clut[1], defaults[1]);

        loaded.memory.write_u16_be(palette + 32 + 6, 0).unwrap();
        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));

        assert_eq!(loaded.screen_clut[1], defaults[1]);
        let allocation = loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .find(|allocation| allocation.palette == handle)
            .unwrap();
        assert_eq!(allocation.reserved_indices, vec![2]);
    }

    #[test]
    fn pm_fore_and_back_use_allocated_animated_device_index() {
        let pef = synthetic_pef_with_import(b"PmForeColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = PPC_DATA_BASE + 0x9000;
        let palette = PPC_DATA_BASE + 0x9100;
        loaded.memory.add_region(handle, vec![0; 4]);
        loaded.memory.add_region(palette, vec![0; 32]);
        loaded.memory.write_u32_be(handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            palette + 16,
            PpcRgbColor {
                red: 0x1234,
                green: 0x5678,
                blue: 0x9abc,
            },
        )
        .unwrap();
        loaded.memory.write_u16_be(palette + 22, 0x0004).unwrap();
        loaded.toolbox_startup.clut_protected[0] = true;
        assert!(with_test_screen_clut!(
            loaded,
            |screen_clut| ppc_apply_palette(
                &mut loaded.memory,
                handle,
                PPC_MAIN_GDEVICE,
                screen_clut,
                &mut loaded.toolbox_startup,
            )
        ));
        loaded
            .memory
            .write_u32_be(
                PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                handle,
            )
            .unwrap();
        loaded.cpu.gpr[3] = 0;
        loaded.run_with_hle_imports(64);
        assert_eq!(
            loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD),
            Some(&1)
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PmBackColor;
        loaded.cpu.gpr[3] = 0;
        loaded.run_with_hle_imports(64);
        assert_eq!(
            loaded
                .toolbox_startup
                .quickdraw_back_indices
                .get(&PPC_MAIN_GWORLD),
            Some(&1)
        );
    }

    #[test]
    fn animate_entry_updates_every_device_reservation_for_the_palette() {
        let pef = synthetic_pef_with_import(b"AnimateEntry");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0xa000;
        let palette_handle = scratch;
        let palette = scratch + 0x10;
        let color_ptr = scratch + 0x40;
        let other_gdevice = scratch + 0x60;
        let other_device = scratch + 0x70;
        let other_pixmap_handle = scratch + 0xb0;
        let other_pixmap = scratch + 0xc0;
        let other_ctable_handle = scratch + 0x100;
        let other_ctable = scratch + 0x110;
        loaded.memory.add_region(scratch, vec![0; 0xa00]);
        loaded.memory.write_u32_be(palette_handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 1).unwrap();
        loaded.memory.write_u16_be(palette + 22, 0x0004).unwrap();
        loaded
            .memory
            .write_u32_be(
                PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                palette_handle,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_gdevice, other_device)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_device + 22, other_pixmap_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_pixmap_handle, other_pixmap)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_pixmap + 42, other_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_ctable_handle, other_ctable)
            .unwrap();
        loaded.memory.write_u16_be(other_ctable + 6, 255).unwrap();
        let color = PpcRgbColor {
            red: 0x1234,
            green: 0x5678,
            blue: 0x9abc,
        };
        ppc_write_rgb_color(&mut loaded.memory, color_ptr, color).unwrap();
        loaded.toolbox_startup.palette_allocations.extend([
            PpcPaletteAllocation {
                palette: palette_handle,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            },
            PpcPaletteAllocation {
                palette: palette_handle,
                gdevice: other_gdevice,
                entry_to_index: vec![Some(43)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(43)],
                reserved_indices: vec![43],
            },
        ]);
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = color_ptr;

        loaded.run_with_hle_imports(64);

        let main_ctable = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, main_ctable + 10 + 42 * 8),
            Some(color)
        );
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, other_ctable + 10 + 43 * 8),
            Some(color)
        );
        assert_eq!(loaded.screen_clut[42], [0x1234, 0x5678, 0x9abc]);
    }

    #[test]
    fn palette_reassignment_keeps_allocation_shared_by_another_window() {
        let pef = synthetic_pef_with_import(b"NSetPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let window = PPC_DATA_BASE + 0xa000;
        let old_palette = PPC_DATA_BASE + 0xa100;
        let new_palette = PPC_DATA_BASE + 0xa200;
        loaded
            .memory
            .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded.gworlds.push(PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: window,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: 0,
            gdevice: PPC_MAIN_GDEVICE,
            width: 1,
            height: 1,
            depth: 8,
            row_bytes: 1,
            pixels_locked: false,
            pixels_no_purge: false,
        });
        for port in [PPC_MAIN_GWORLD, window] {
            loaded
                .memory
                .write_u32_be(port + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, old_palette)
                .unwrap();
        }
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: old_palette,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            });
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = new_palette;
        loaded.cpu.gpr[5] = 0;
        loaded.run_with_hle_imports(64);
        assert!(loaded
            .toolbox_startup
            .palette_allocations
            .iter()
            .any(|allocation| allocation.palette == old_palette));
    }

    #[test]
    fn close_window_releases_only_the_last_owner_palette_allocation() {
        let pef = synthetic_pef_with_import(b"CloseWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        let windows = [PPC_DATA_BASE + 0xa000, PPC_DATA_BASE + 0xb000];
        let palette = PPC_DATA_BASE + 0xc000;
        for window in windows {
            loaded
                .memory
                .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
            loaded
                .memory
                .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 1)
                .unwrap();
            loaded
                .memory
                .write_u32_be(window + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, palette)
                .unwrap();
            loaded.gworlds.push(PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: window,
                pixmap_handle: 0,
                pixmap: 0,
                base_addr: 0,
                gdevice: PPC_MAIN_GDEVICE,
                width: 1,
                height: 1,
                depth: 8,
                row_bytes: 1,
                pixels_locked: false,
                pixels_no_purge: false,
            });
        }
        let defaults = TrapDispatcher::standard_mac_8bpp_clut();
        loaded.screen_clut.set_entry(42, [0x1111, 0x2222, 0x3333]);
        loaded.color_manager_clut.set_entry(42, [0x1111, 0x2222, 0x3333]);
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            });

        loaded.cpu.gpr[3] = windows[1];
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.screen_clut[42], [0x1111, 0x2222, 0x3333]);
        assert_eq!(loaded.toolbox_startup.palette_allocations.len(), 1);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = windows[0];
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.screen_clut[42], defaults[42]);
        assert_eq!(loaded.color_manager_clut[42], defaults[42]);
        assert!(loaded.toolbox_startup.palette_allocations.is_empty());
    }

    #[test]
    fn releasing_a_noncurrent_device_palette_restores_only_that_device() {
        let pef = synthetic_pef_with_import(b"DisposePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0xd000;
        let palette = scratch;
        let other_gdevice = scratch + 0x10;
        let other_device = scratch + 0x20;
        let other_pixmap_handle = scratch + 0x60;
        let other_pixmap = scratch + 0x70;
        let other_ctable_handle = scratch + 0xb0;
        let other_ctable = scratch + 0xc0;
        loaded.memory.add_region(scratch, vec![0; 0x1000]);
        loaded
            .memory
            .write_u32_be(other_gdevice, other_device)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_device + 22, other_pixmap_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_pixmap_handle, other_pixmap)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_pixmap + 42, other_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(other_ctable_handle, other_ctable)
            .unwrap();
        loaded.memory.write_u16_be(other_ctable + 6, 255).unwrap();
        let defaults = TrapDispatcher::standard_mac_8bpp_clut();
        let mut other_clut = defaults;
        other_clut[42] = [0x1111, 0x2222, 0x3333];
        other_clut[43] = [0x4444, 0x5555, 0x6666];
        ppc_write_device_color_table(
            &mut loaded.memory,
            other_gdevice,
            &other_clut,
            &mut loaded.toolbox_startup,
        );
        loaded.screen_clut.set_entry(42, [0xaaaa, 0xbbbb, 0xcccc]);
        loaded.color_manager_clut.set_entry(42, [0xaaaa, 0xbbbb, 0xcccc]);
        let expected_screen = *loaded.screen_clut;
        let expected_manager = *loaded.color_manager_clut;
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette,
                gdevice: other_gdevice,
                entry_to_index: vec![Some(42)],
                entry_mappings: vec![PpcPaletteEntryMapping::AnimatedReserved(42)],
                reserved_indices: vec![42],
            });

        with_test_display_cluts!(loaded, |screen_clut, color_manager_clut| {
            ppc_release_palette_allocations_and_restore(
                &mut loaded.memory,
                &mut loaded.toolbox_startup,
                palette,
                PPC_MAIN_GDEVICE,
                screen_clut,
                color_manager_clut,
            )
        });

        assert_eq!(loaded.screen_clut, expected_screen);
        assert_eq!(loaded.color_manager_clut, expected_manager);
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, other_ctable + 10 + 42 * 8),
            Some(PpcRgbColor {
                red: defaults[42][0],
                green: defaults[42][1],
                blue: defaults[42][2],
            })
        );
        assert_eq!(
            ppc_read_rgb_color(&mut loaded.memory, other_ctable + 10 + 43 * 8),
            Some(PpcRgbColor {
                red: other_clut[43][0],
                green: other_clut[43][1],
                blue: other_clut[43][2],
            })
        );
        assert!(loaded.toolbox_startup.palette_allocations.is_empty());
    }

    #[test]
    fn hle_import_runner_animates_only_reserved_device_rgb_entries() {
        let pef = synthetic_pef_with_import(b"AnimatePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let palette_handle = PPC_DATA_BASE + 0x1000;
        let palette_ptr = PPC_DATA_BASE + 0x2000;
        let ctable_handle = PPC_DATA_BASE + 0x3000;
        let ctable_ptr = PPC_DATA_BASE + 0x4000;
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette_ptr, vec![0; 48]);
        loaded.memory.add_region(ctable_handle, vec![0; 4]);
        loaded.memory.add_region(ctable_ptr, vec![0; 24]);
        loaded
            .memory
            .write_u32_be(palette_handle, palette_ptr)
            .unwrap();
        loaded.memory.write_u16_be(palette_ptr, 2).unwrap();
        let old_colors = [[0x0101, 0x0202, 0x0303], [0x0404, 0x0505, 0x0606]];
        for (entry, old) in old_colors.into_iter().enumerate() {
            let info = palette_ptr + 16 + entry as u32 * 16;
            loaded.memory.write_u16_be(info, old[0]).unwrap();
            loaded.memory.write_u16_be(info + 2, old[1]).unwrap();
            loaded.memory.write_u16_be(info + 4, old[2]).unwrap();
            loaded.memory.write_u16_be(info + 6, 0x0004).unwrap();
        }
        loaded
            .memory
            .write_u32_be(ctable_handle, ctable_ptr)
            .unwrap();
        loaded.memory.write_u16_be(ctable_ptr + 6, 1).unwrap();
        for (entry, rgb) in [[0x1234u16, 0x5678, 0x9abc], [0xdef0u16, 0x1357, 0x2468]]
            .into_iter()
            .enumerate()
        {
            let spec = ctable_ptr + 8 + entry as u32 * 8;
            loaded.memory.write_u16_be(spec, entry as u16).unwrap();
            loaded.memory.write_u16_be(spec + 2, rgb[0]).unwrap();
            loaded.memory.write_u16_be(spec + 4, rgb[1]).unwrap();
            loaded.memory.write_u16_be(spec + 6, rgb[2]).unwrap();
        }
        loaded
            .memory
            .write_u32_be(
                PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                palette_handle,
            )
            .unwrap();
        let device_ctable = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
        loaded
            .memory
            .write_u32_be(device_ctable, 0x1234_5678)
            .unwrap();
        for (entry, (index, old, value)) in
            [(42u32, old_colors[0], 0xcafe), (43, old_colors[1], 0xbabe)]
                .into_iter()
                .enumerate()
        {
            let spec = device_ctable + 8 + index * 8;
            loaded.memory.write_u16_be(spec, value).unwrap();
            loaded.memory.write_u16_be(spec + 2, old[0]).unwrap();
            loaded.memory.write_u16_be(spec + 4, old[1]).unwrap();
            loaded.memory.write_u16_be(spec + 6, old[2]).unwrap();
            loaded.screen_clut.set_entry(index as usize, old);
            loaded.color_manager_clut.set_entry(index as usize, old);
            assert_eq!(entry as u32, index - 42);
        }
        let untouched =
            ppc_memory_read_bytes(&mut loaded.memory, device_ctable + 8 + 44 * 8, 8).unwrap();
        let old_zero = loaded.screen_clut[0];
        let old_one = loaded.screen_clut[1];
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: palette_handle,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![Some(42), Some(43)],
                entry_mappings: vec![
                    PpcPaletteEntryMapping::AnimatedReserved(42),
                    PpcPaletteEntryMapping::AnimatedReserved(43),
                ],
                reserved_indices: vec![42, 43],
            });
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = ctable_handle;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 2;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.screen_clut[42], [0x1234, 0x5678, 0x9abc]);
        assert_eq!(loaded.screen_clut[43], [0xdef0, 0x1357, 0x2468]);
        assert_eq!(loaded.color_manager_clut[42], [0x1234, 0x5678, 0x9abc]);
        assert_eq!(loaded.screen_clut[0], old_zero);
        assert_eq!(loaded.screen_clut[1], old_one);
        assert_eq!(loaded.memory.read_u32_be(device_ctable), Some(0x1234_5678));
        assert_eq!(
            loaded.memory.read_u16_be(device_ctable + 8 + 42 * 8),
            Some(0xcafe)
        );
        assert_eq!(
            loaded.memory.read_u16_be(device_ctable + 8 + 43 * 8),
            Some(0xbabe)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, device_ctable + 8 + 44 * 8, 8),
            Some(untouched)
        );
        assert_eq!(loaded.memory.read_u16_be(palette_ptr), Some(2));
        assert_eq!(loaded.memory.read_u16_be(palette_ptr + 16), Some(0x1234));
        assert_eq!(loaded.memory.read_u16_be(palette_ptr + 32), Some(0xdef0));
    }

    #[test]
    fn hle_import_runner_set_entry_color_preserves_usage_and_tolerance() {
        let pef = synthetic_pef_with_import(b"SetEntryColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        let palette_handle = PPC_DATA_BASE + 0x1000;
        let palette_ptr = PPC_DATA_BASE + 0x2000;
        let rgb_ptr = PPC_DATA_BASE + 0x3000;
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette_ptr, vec![0; 48]);
        loaded.memory.add_region(rgb_ptr, vec![0; 6]);
        loaded
            .memory
            .write_u32_be(palette_handle, palette_ptr)
            .unwrap();
        loaded.memory.write_u16_be(palette_ptr, 2).unwrap();
        let info_ptr = palette_ptr + 32;
        for (offset, value) in [
            (0, 0x1111),
            (2, 0x2222),
            (4, 0x3333),
            (6, 0x0008),
            (8, 0x1234),
            (10, 0xaaaa),
            (12, 0xbbbb),
            (14, 0xcccc),
        ] {
            loaded
                .memory
                .write_u16_be(info_ptr + offset, value)
                .unwrap();
        }
        for (offset, value) in [(0, 0x4567), (2, 0x89ab), (4, 0xcdef)] {
            loaded.memory.write_u16_be(rgb_ptr + offset, value).unwrap();
        }
        loaded.cpu.gpr[3] = palette_handle;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = rgb_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u16_be(info_ptr), Some(0x4567));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 2), Some(0x89ab));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 4), Some(0xcdef));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 6), Some(0x0008));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 8), Some(0x1234));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 10), Some(0xaaaa));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 12), Some(0xbbbb));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 14), Some(0xcccc));
    }

    #[test]
    fn hle_import_runner_set_entry_color_ignores_out_of_range_entry() {
        let pef = synthetic_pef_with_import(b"SetEntryColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        let palette_handle = PPC_DATA_BASE + 0x1000;
        let palette_ptr = PPC_DATA_BASE + 0x2000;
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette_ptr, vec![0; 32]);
        loaded
            .memory
            .write_u32_be(palette_handle, palette_ptr)
            .unwrap();
        loaded.memory.write_u16_be(palette_ptr, 1).unwrap();
        loaded.cpu.gpr[3] = palette_handle;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = 0xffff_fffc;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u16_be(palette_ptr + 16), Some(0));
    }

    #[test]
    fn hle_import_runner_set_entry_usage_updates_only_usage_fields() {
        let pef = synthetic_pef_with_import(b"SetEntryUsage");
        let mut loaded = load_pef_application(&pef).unwrap();
        let palette_handle = PPC_DATA_BASE + 0x1000;
        let palette_ptr = PPC_DATA_BASE + 0x2000;
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette_ptr, vec![0; 32]);
        loaded
            .memory
            .write_u32_be(palette_handle, palette_ptr)
            .unwrap();
        loaded.memory.write_u16_be(palette_ptr, 1).unwrap();
        let info_ptr = palette_ptr + 16;
        loaded.memory.write_u16_be(info_ptr, 0x1234).unwrap();
        loaded.cpu.gpr[3] = palette_handle;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0x0008;
        loaded.cpu.gpr[6] = 0x4567;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u16_be(info_ptr), Some(0x1234));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 6), Some(0x0008));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 8), Some(0x4567));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[5] = u32::from(u16::MAX);
        loaded.cpu.gpr[6] = 0x2222;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 6), Some(0x0008));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 8), Some(0x2222));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[5] = 0x0004;
        loaded.cpu.gpr[6] = u32::from(u16::MAX);
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 6), Some(0x0004));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 8), Some(0x2222));
    }

    #[test]
    fn hle_import_runner_get_entry_usage_reads_usage_fields() {
        let pef = synthetic_pef_with_import(b"GetEntryUsage");
        let mut loaded = load_pef_application(&pef).unwrap();
        let palette_handle = PPC_DATA_BASE + 0x1000;
        let palette_ptr = PPC_DATA_BASE + 0x2000;
        let outputs = PPC_DATA_BASE + 0x3000;
        loaded.memory.add_region(palette_handle, vec![0; 4]);
        loaded.memory.add_region(palette_ptr, vec![0; 32]);
        loaded.memory.add_region(outputs, vec![0; 4]);
        loaded
            .memory
            .write_u32_be(palette_handle, palette_ptr)
            .unwrap();
        loaded.memory.write_u16_be(palette_ptr, 1).unwrap();
        loaded
            .memory
            .write_u16_be(palette_ptr + 16 + 6, 0x0008)
            .unwrap();
        loaded
            .memory
            .write_u16_be(palette_ptr + 16 + 8, 0x4567)
            .unwrap();
        loaded.cpu.gpr[3] = palette_handle;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = outputs;
        loaded.cpu.gpr[6] = outputs + 2;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u16_be(outputs), Some(0x0008));
        assert_eq!(loaded.memory.read_u16_be(outputs + 2), Some(0x4567));
    }

    #[test]
    fn hle_import_runner_dispose_palette_releases_handle() {
        let pef = synthetic_pef_with_import(b"DisposePalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &[0; 16],
        );
        assert_ne!(handle, 0);
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert!(test_handle_records!(loaded)
            .iter()
            .all(|record| record.handle != handle));
        assert_eq!(loaded.memory.read_u32_be(handle), Some(0));
    }

    #[test]
    fn hle_import_runner_nset_palette_nil_window_is_noop() {
        let pef = synthetic_pef_with_import(b"NSetPalette");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = PPC_DATA_BASE + 0x2000;
        loaded.cpu.gpr[5] = 0x1234;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
