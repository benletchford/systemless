use super::*;

fn replacement_table(app: &mut PpcLoadedApp, base: u32) -> (u32, u32) {
    app.memory.add_region(base, vec![0; 4096]);
    let table = base + 32;
    app.memory.write_u32_be(base, table).unwrap();
    app.memory.write_u32_be(table, 0x1234_5678).unwrap();
    app.memory.write_u16_be(table + 4, 0x8000).unwrap();
    app.memory.write_u16_be(table + 6, 255).unwrap();
    for index in 0..256u32 {
        app.memory
            .write_u16_be(table + 8 + index * 8, index as u16)
            .unwrap();
    }
    app.memory
        .write_u16_be(table + 8 + 17 * 8 + 2, u16::MAX)
        .unwrap();
    (base, table)
}

#[test]
fn gdevice_changed_replaced_main_table_refreshes_mapping_without_hardware_fade() {
    for main_is_current in [false, true] {
        let mut app = load_pef_application(&synthetic_pef_with_import(b"GDeviceChanged")).unwrap();
        assert_eq!(
            app.imports[0].dispatcher_target,
            PpcImportDispatcherTarget::GDeviceChanged
        );
        let base = PPC_DATA_BASE + 0x9000;
        let (table_handle, table) = replacement_table(&mut app, base);
        let device = app.memory.read_u32_be(PPC_MAIN_GDEVICE).unwrap();
        app.memory
            .write_u32_be(PPC_MAIN_PIXMAP + 42, table_handle)
            .unwrap();
        let inverse_handle = base + 2200;
        let inverse = base + 2220;
        app.memory.write_u32_be(inverse_handle, inverse).unwrap();
        app.memory.write_u32_be(inverse, 0x1234_5678).unwrap();
        app.memory.write_u32_be(device + 6, inverse_handle).unwrap();
        let current = if main_is_current {
            PPC_MAIN_GDEVICE
        } else {
            base + 2300
        };
        app.cpu.gpr[3] = current;
        run_test_import(&mut app, PpcImportDispatcherTarget::SetGDevice);
        let hardware = *app.screen_clut;
        let before = ppc_memory_read_bytes(&mut app.memory, table, 2056).unwrap();
        app.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        run_test_import(&mut app, PpcImportDispatcherTarget::GDeviceChanged);
        assert_eq!(app.cpu.gpr[3], PPC_MAIN_GDEVICE); // void ABI
        assert_eq!(*app.current_gdevice, current);
        assert_eq!(*app.screen_clut, hardware);
        assert_eq!(app.color_manager_clut[17], [u16::MAX, 0, 0]);
        assert_eq!(app.memory.read_u32_be(inverse), Some(!0x1234_5678));
        assert_eq!(app.memory.read_u32_be(device + 6), Some(inverse_handle));
        assert_eq!(
            ppc_memory_read_bytes(&mut app.memory, table, 2056).unwrap(),
            before
        );
        app.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        run_test_import(&mut app, PpcImportDispatcherTarget::SetGDevice);
        let rgb = base + 2400;
        app.memory.write_u16_be(rgb, u16::MAX).unwrap();
        app.cpu.gpr[3] = rgb;
        run_test_import(&mut app, PpcImportDispatcherTarget::Color2Index);
        assert_eq!(app.cpu.gpr[3], 17);
    }
}

#[test]
fn gdevice_changed_offscreen_table_preserves_main_palette_and_current_device() {
    let mut app = load_pef_application(&synthetic_pef_with_import(b"GDeviceChanged")).unwrap();
    let base = PPC_DATA_BASE + 0x9000;
    let (table_handle, _) = replacement_table(&mut app, base);
    let handle = base + 2200;
    let record = base + 2220;
    let pm_handle = base + 2300;
    let pixmap = base + 2320;
    app.memory.write_u32_be(handle, record).unwrap();
    app.memory.write_u32_be(record + 22, pm_handle).unwrap();
    app.memory.write_u32_be(pm_handle, pixmap).unwrap();
    app.memory.write_u16_be(pixmap + 32, 8).unwrap();
    app.memory.write_u32_be(pixmap + 42, table_handle).unwrap();
    let hardware = *app.screen_clut;
    let logical = *app.color_manager_clut;
    app.cpu.gpr[3] = handle;
    run_test_import(&mut app, PpcImportDispatcherTarget::GDeviceChanged);
    assert_eq!(*app.current_gdevice, PPC_MAIN_GDEVICE);
    assert_eq!(*app.screen_clut, hardware);
    assert_eq!(*app.color_manager_clut, logical);
    app.cpu.gpr[3] = handle;
    run_test_import(&mut app, PpcImportDispatcherTarget::SetGDevice);
    let rgb = base + 2400;
    app.memory.write_u16_be(rgb, u16::MAX).unwrap();
    app.cpu.gpr[3] = rgb;
    run_test_import(&mut app, PpcImportDispatcherTarget::Color2Index);
    assert_eq!(app.cpu.gpr[3], 17);
}

#[test]
fn gdevice_changed_nil_and_unreadable_devices_preserve_palette_and_void_argument() {
    let mut app = load_pef_application(&synthetic_pef_with_import(b"GDeviceChanged")).unwrap();
    let hardware = *app.screen_clut;
    let logical = *app.color_manager_clut;
    for handle in [0, 0xffff_fffc] {
        app.cpu.gpr[3] = handle;
        run_test_import(&mut app, PpcImportDispatcherTarget::GDeviceChanged);
        assert_eq!(app.cpu.gpr[3], handle);
        assert_eq!(*app.current_gdevice, PPC_MAIN_GDEVICE);
        assert_eq!(*app.screen_clut, hardware);
        assert_eq!(*app.color_manager_clut, logical);
    }
}
