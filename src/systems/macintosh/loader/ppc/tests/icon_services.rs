use super::*;

#[test]
fn carbon_weak_icon_import_relocates_to_callable_tvector() {
    let pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"CarbonLib",
        b"GetIconRefFromFile",
        0x82,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::GetIconRefFromFile
    );
    assert_ne!(loaded.imports[0].address, 0);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(loaded.imports[0].address)
    );
}

#[test]
fn carbon_icon_ref_from_file_plots_custom_icon_and_releases_owners() {
    for (symbol, target) in [
        (
            "GetIconRefFromFile",
            PpcImportDispatcherTarget::GetIconRefFromFile,
        ),
        ("GetIconRef", PpcImportDispatcherTarget::GetIconRef),
        ("PlotIconRef", PpcImportDispatcherTarget::PlotIconRef),
        ("ReleaseIconRef", PpcImportDispatcherTarget::ReleaseIconRef),
    ] {
        assert_eq!(dispatcher_target_for_import("CarbonLib", symbol), target);
    }

    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"GetIconRefFromFile")).unwrap();
    let scratch = PPC_DATA_BASE + 0x3000;
    let icon_out = scratch + 80;
    let label_out = scratch + 84;
    let rect = scratch + 96;
    loaded.memory.add_region(scratch, vec![0; 128]);
    write_ppc_fsspec(
        &mut loaded.memory,
        scratch,
        PPC_BOOT_VOLUME_REF_NUM,
        PPC_ROOT_DIR_ID,
        b"Icon Test",
    );
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Icon Test".to_string(),
        data: Vec::new().into(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"TEXT"),
        finder_flags: 0x0006,
        dirty: false,
    });
    let mut icon_data = vec![0; 256];
    let center = 16 * 4 + 16 / 8;
    icon_data[center] = 0x80;
    icon_data[128 + center] = 0x80;
    loaded
        .process_file_system
        .push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "Icon Test".to_string(),
            res_type: u32::from_be_bytes(*b"ICN#"),
            res_id: -16455,
            name: Vec::new(),
            data: icon_data,
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });

    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = icon_out;
    loaded.cpu.gpr[5] = label_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetIconRefFromFile);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let reference = loaded.memory.read_u32_be(icon_out).unwrap();
    assert_ne!(reference, 0);
    assert_eq!(loaded.memory.read_u16_be(label_out), Some(3));

    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = icon_out;
    loaded.cpu.gpr[5] = label_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetIconRefFromFile);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(icon_out), Some(reference));
    assert_eq!(loaded.toolbox_startup.icon_refs.owners(reference), Some(2));

    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = label_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetIconRefFromFile);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u16_be(label_out), Some(3));
    assert_eq!(loaded.toolbox_startup.icon_refs.owners(reference), Some(2));

    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ReleaseIconRef);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.toolbox_startup.icon_refs.owners(reference), Some(1));

    let bounds = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds, vec![0; 32]);
    let window = super::window_manager::create_test_cwindow(
        &mut loaded,
        bounds,
        (40, 50, 200, 300),
        0,
        true,
        u32::MAX,
    );
    loaded.cpu.gpr[3] = window;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetPortWindowPort);
    assert_eq!(*loaded.current_gworld, window);
    loaded.memory.write_u16_be(rect, 40).unwrap();
    loaded.memory.write_u16_be(rect + 2, 50).unwrap();
    loaded.memory.write_u16_be(rect + 4, 72).unwrap();
    loaded.memory.write_u16_be(rect + 6, 82).unwrap();
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, rect),
        Some((40, 50, 72, 82))
    );
    let surface = ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, window).unwrap();
    let (local_top, local_left, _, _) = surface.local_rect((40, 50, 72, 82));
    let center_point = (local_left + 16, local_top + 16);
    assert!(ppc_quickdraw_write_pixel(
        &mut loaded.memory,
        surface.front_buffer,
        center_point,
        PPC_RGB_WHITE,
    ));
    let before = ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, center_point);
    loaded.cpu.gpr[3] = rect;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::PlotIconRef);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let after = ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, center_point);
    assert_ne!(before, after);
    assert_eq!(
        after,
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK)
    );

    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ReleaseIconRef);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ReleaseIconRef);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn carbon_icon_ref_from_file_reports_missing_paths_without_touching_outputs() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"GetIconRefFromFile")).unwrap();
    let scratch = PPC_HEAP_BASE;
    let icon_out = scratch + 80;
    let label_out = scratch + 84;
    loaded.memory.add_region(scratch, vec![0; 128]);
    write_ppc_fsspec(
        &mut loaded.memory,
        scratch,
        PPC_BOOT_VOLUME_REF_NUM,
        PPC_ROOT_DIR_ID,
        b"Missing File",
    );
    loaded.memory.write_u32_be(icon_out, 0x1234_5678).unwrap();
    loaded.memory.write_u16_be(label_out, 0x7fff).unwrap();
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = icon_out;
    loaded.cpu.gpr[5] = label_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetIconRefFromFile);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
    assert_eq!(loaded.memory.read_u32_be(icon_out), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u16_be(label_out), Some(0x7fff));
}

#[test]
fn carbon_get_icon_ref_returns_a_releasable_generic_folder_icon() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetIconRef")).unwrap();
    let icon_out = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(icon_out, vec![0; 16]);
    loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"macs");
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"fldr");
    loaded.cpu.gpr[6] = icon_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetIconRef);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let reference = loaded.memory.read_u32_be(icon_out).unwrap();
    assert_ne!(reference, 0);
    assert_eq!(loaded.toolbox_startup.icon_refs.owners(reference), Some(1));
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ReleaseIconRef);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.toolbox_startup.icon_refs.owners(reference), None);
}
