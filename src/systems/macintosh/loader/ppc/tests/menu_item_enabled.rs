use super::*;

fn mutate(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) {
    let original = loaded.imports[0].dispatcher_target.clone();
    assert_eq!(original, PpcImportDispatcherTarget::IsMenuItemEnabled);
    run_test_import(loaded, target);
    loaded.imports[0].dispatcher_target = original;
}

fn query(loaded: &mut PpcLoadedApp, menu: u32, item: u32) -> u32 {
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = menu;
    loaded.cpu.gpr[4] = item;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    loaded.cpu.gpr[3]
}

#[test]
fn menu_item_enabled_queries_item_state_independently_of_parent_title() {
    for lib in [
        b"CarbonLib".as_slice(),
        b"InterfaceLib",
        b"AppearanceLib",
        b"MenusLib",
    ] {
        let pef = synthetic_pef_with_library_import(lib, b"IsMenuItemEnabled");
        let mut loaded = load_pef_application(&pef).unwrap();
        let menu = install_test_menu(&mut loaded, 0x60000, 201, b"File", b"Open;Save;-");
        assert_eq!(query(&mut loaded, menu, 0), 1);
        assert_eq!(query(&mut loaded, menu, 1), 1);
        assert_eq!(query(&mut loaded, menu, 3), 1); // enabled separator is not selectable
        loaded.cpu.gpr[3] = menu;
        loaded.cpu.gpr[4] = 0;
        mutate(&mut loaded, PpcImportDispatcherTarget::DisableMenuItem);
        assert_eq!(query(&mut loaded, menu, 0), 0);
        assert_eq!(query(&mut loaded, menu, 1), 1);
        loaded.cpu.gpr[3] = menu;
        loaded.cpu.gpr[4] = 1;
        mutate(&mut loaded, PpcImportDispatcherTarget::DisableMenuItem);
        assert_eq!(query(&mut loaded, menu, 1), 0);
        loaded.cpu.gpr[3] = menu;
        loaded.cpu.gpr[4] = 1;
        mutate(&mut loaded, PpcImportDispatcherTarget::EnableMenuItem);
        assert_eq!(query(&mut loaded, menu, 1), 1);
        assert_eq!(query(&mut loaded, menu, 4), 0);
        assert_eq!(query(&mut loaded, menu, u32::MAX), 0);
        // A null MenuRef stays null even when low memory contains a valid menu address.
        let menu_ptr = loaded.memory.read_u32_be(menu).unwrap();
        loaded.memory.add_region(0, menu_ptr.to_be_bytes().to_vec());
        assert_eq!(query(&mut loaded, 0, 1), 0);
        assert_eq!(query(&mut loaded, 0xdeadbeef, 1), 0);
    }
}

#[test]
fn menu_item_enabled_queries_existing_rows_beyond_classic_enable_flag_width() {
    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"IsMenuItemEnabled");
    let mut loaded = load_pef_application(&pef).unwrap();
    let specs = (0..40).map(|_| "Item").collect::<Vec<_>>().join(";");
    let menu = install_test_menu(&mut loaded, 0x60000, 202, b"File", specs.as_bytes());
    loaded.cpu.gpr[3] = menu;
    loaded.cpu.gpr[4] = 0;
    mutate(&mut loaded, PpcImportDispatcherTarget::DisableMenuItem);
    assert_eq!(query(&mut loaded, menu, 31), 1);
    assert_eq!(query(&mut loaded, menu, 32), 1);
    assert_eq!(query(&mut loaded, menu, 40), 1);
    assert_eq!(query(&mut loaded, menu, 41), 0);
    // MenuItemIndex is unsigned 16-bit; unrelated upper register bits are ignored.
    assert_eq!(query(&mut loaded, menu, 0xffff0028), 1);
}
