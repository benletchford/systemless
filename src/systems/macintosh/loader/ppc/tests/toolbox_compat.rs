use super::*;

fn compat(operation: PpcToolboxCompatOperation, loaded: &mut PpcLoadedApp) -> PpcImportAction {
    dispatch_toolbox_compat::ppc_dispatch_toolbox_compat(operation, &loaded.cpu, &mut loaded.memory)
}

#[test]
fn audit_toolbox_calls_resolve_to_native_targets() {
    for symbol in [
        "CloseDriver",
        "KillIO",
        "SerReset",
        "SerSetBuf",
        "SerGetBuf",
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::UnopenedDriver),
            "{symbol}"
        );
    }
    for (library, symbol, target) in [
        (
            "InterfaceLib",
            "HMRemoveBalloon",
            PpcImportDispatcherTarget::HMRemoveBalloon,
        ),
        (
            "InterfaceLib",
            "GZSaveHnd",
            PpcImportDispatcherTarget::ReturnNoErr,
        ),
        ("CarbonLib", "EraseRgn", PpcImportDispatcherTarget::EraseRgn),
        (
            "InterfaceLib",
            "FSpRename",
            PpcImportDispatcherTarget::FSpRename,
        ),
        (
            "InterfaceLib",
            "PBDTGetAPPLSync",
            PpcImportDispatcherTarget::PBDTGetAPPL,
        ),
        (
            "CarbonLib",
            "TESetScrapLength",
            PpcImportDispatcherTarget::TEScrapLength { set: true },
        ),
    ] {
        assert_eq!(
            dispatcher_target_for_import(library, symbol),
            target,
            "{symbol}"
        );
    }
}

#[test]
fn region_bounds_and_port_text_accessors_read_guest_records() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetRegionBounds")).unwrap();
    let base = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(base, vec![0; 0x200]);
    let (handle, master, out, port) = (base, base + 0x10, base + 0x40, base + 0x80);
    loaded.memory.write_u32_be(handle, master).unwrap();
    for (offset, value) in [(0u32, 10u16), (2, 1), (4, 2), (6, 3), (8, 4)] {
        loaded.memory.write_u16_be(master + offset, value).unwrap();
    }
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = out;
    assert!(matches!(
        compat(PpcToolboxCompatOperation::GetRegionBounds, &mut loaded),
        PpcImportAction::Return(value) if value == out
    ));
    let bounds: Vec<u16> = (0..4)
        .map(|index| loaded.memory.read_u16_be(out + index * 2).unwrap())
        .collect();
    assert_eq!(bounds, [1, 2, 3, 4]);

    loaded.memory.write_u16_be(port + 68, 21).unwrap();
    loaded.memory.write_u16_be(port + 74, 12).unwrap();
    loaded.cpu.gpr[3] = port;
    assert!(matches!(
        compat(PpcToolboxCompatOperation::GetPortTextFont, &mut loaded),
        PpcImportAction::Return(21)
    ));
    assert!(matches!(
        compat(PpcToolboxCompatOperation::GetPortTextSize, &mut loaded),
        PpcImportAction::Return(12)
    ));
    assert!(matches!(
        compat(PpcToolboxCompatOperation::UnopenedDriver, &mut loaded),
        PpcImportAction::Return(value) if value == ppc_i16_result(-28)
    ));
}

#[test]
fn pb_dt_get_appl_finds_an_application_by_creator() {
    let pef = synthetic_pef_with_import(b"PBDTGetAPPLSync");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pb = PPC_DATA_BASE + 0x1000;
    let name = pb + 0x100;
    loaded.memory.add_region(pb, vec![0; 0x200]);
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Viewer".to_string(),
        data: b"code".to_vec().into(),
        creator: u32::from_be_bytes(*b"VIEW"),
        file_type: u32::from_be_bytes(*b"APPL"),
        finder_flags: 0,
        dirty: false,
    });
    loaded.memory.write_u32_be(pb + 18, name).unwrap();
    loaded
        .memory
        .write_u32_be(pb + 52, u32::from_be_bytes(*b"VIEW"))
        .unwrap();
    loaded.cpu.gpr[3] = pb;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, name).as_deref(),
        Some(&b"Viewer"[..])
    );
    assert_eq!(loaded.memory.read_u32_be(pb + 100), Some(PPC_ROOT_DIR_ID));

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(pb, vec![0; 0x200]);
    loaded
        .memory
        .write_u32_be(pb + 52, u32::from_be_bytes(*b"NONE"))
        .unwrap();
    loaded.cpu.gpr[3] = pb;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-5018));
}

#[test]
fn fsp_rename_renames_the_spec_target() {
    let pef = synthetic_pef_with_import(b"FSpRename");
    let mut loaded = load_pef_application(&pef).unwrap();
    let spec = PPC_DATA_BASE + 0x1000;
    let new_name = spec + 0x80;
    loaded.memory.add_region(spec, vec![0; 0x100]);
    let old_path = "System Folder/Preferences/Old Log";
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: old_path.to_string(),
        data: b"data fork".to_vec().into(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        dirty: false,
    });
    loaded
        .memory
        .write_u16_be(spec, PPC_BOOT_VOLUME_REF_NUM as u16)
        .unwrap();
    loaded
        .memory
        .write_u32_be(spec + 2, PPC_PREFERENCES_DIR_ID)
        .unwrap();
    write_ppc_pstring(&mut loaded.memory, spec + 6, b"Old Log");
    write_ppc_pstring(&mut loaded.memory, new_name, b"New Log");
    loaded.cpu.gpr[3] = spec;
    loaded.cpu.gpr[4] = new_name;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.vfs_files[0].path,
        "System Folder/Preferences/New Log"
    );
}

#[test]
fn otto_matic_weak_imports_resolve_and_absolute_times_subtract() {
    for (library, symbol, target) in [
        (
            "CarbonLib",
            "SubAbsoluteFromAbsolute",
            PpcImportDispatcherTarget::SubAbsoluteFromAbsolute,
        ),
        (
            "Apple;Carbon;Multimedia",
            "SetMoviePlayHints",
            PpcImportDispatcherTarget::NoOpPreserve,
        ),
        (
            "Apple;Carbon;Multimedia",
            "GraphicsImportSetQuality",
            PpcImportDispatcherTarget::ReturnNoErr,
        ),
        (
            "DrawSprocketLib",
            "DSpSetWindowToFront",
            PpcImportDispatcherTarget::ReturnNoErr,
        ),
    ] {
        assert_eq!(
            dispatcher_target_for_import(library, symbol),
            target,
            "{symbol}"
        );
    }

    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"SubAbsoluteFromAbsolute")).unwrap();
    let out = PPC_STACK_TOP - 64;
    loaded.cpu.gpr[3] = out;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 10;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 20;
    let action = dispatch_simple_hot_import_fast(
        &PpcImportDispatcherTarget::SubAbsoluteFromAbsolute,
        &mut loaded.cpu,
        &mut loaded.memory,
        0,
    );
    assert!(matches!(action, Some(PpcImportAction::ReturnPreserve)));
    assert_eq!(loaded.memory.read_u64_be(out), Some((1u64 << 32) - 10));
}

#[test]
fn audit_batch_three_calls_resolve_and_behave() {
    for (symbol, target) in [
        ("MapRgn", PpcImportDispatcherTarget::MapRgn),
        (
            "FollowFinderAlias",
            PpcImportDispatcherTarget::FollowFinderAlias,
        ),
        ("Eject", PpcImportDispatcherTarget::ReturnNoErr),
        (
            "CMNew",
            PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::CommToolboxNil),
        ),
        (
            "CMGetProcID",
            PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::CommToolboxError),
        ),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
    assert_eq!(
        import_data_address_for("CarbonLib", "kCFAllocatorDefault"),
        Some(PPC_IMPORT_CF_ALLOCATOR_DEFAULT)
    );

    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"MapRgn")).unwrap();
    assert!(matches!(
        compat(PpcToolboxCompatOperation::CommToolboxNil, &mut loaded),
        PpcImportAction::Return(0)
    ));
    assert!(matches!(
        compat(PpcToolboxCompatOperation::CommToolboxError, &mut loaded),
        PpcImportAction::Return(value) if value == ppc_i16_result(-1)
    ));

    // A 10-by-10 rectangular region mapped from a 100-unit square onto a
    // 200-unit square offset by 50 doubles and moves.
    let base = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(base, vec![0; 0x100]);
    let (handle, region, src, dst) = (base, base + 0x10, base + 0x40, base + 0x50);
    loaded.memory.write_u32_be(handle, region).unwrap();
    loaded.memory.write_u16_be(region, 10).unwrap();
    ppc_write_rect(&mut loaded.memory, region + 2, 10, 10, 20, 20).unwrap();
    ppc_write_rect(&mut loaded.memory, src, 0, 0, 100, 100).unwrap();
    ppc_write_rect(&mut loaded.memory, dst, 50, 50, 250, 250).unwrap();
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = src;
    loaded.cpu.gpr[5] = dst;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MapRgn);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, region + 2),
        Some((70, 70, 90, 90))
    );
}

#[test]
fn carbon_port_accessors_read_cgrafport_fields() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetPortPenMode")).unwrap();
    let base = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(base, vec![0; 0x200]);
    let (port, out) = (base, base + 0x100);
    loaded.memory.write_u16_be(port + 6, 0xc000).unwrap();
    for (offset, value) in [(36u32, 0x1111u16), (38, 0x2222), (40, 0x3333)] {
        loaded.memory.write_u16_be(port + offset, value).unwrap();
    }
    loaded.memory.write_u16_be(port + 42, 0xaaaa).unwrap();
    loaded.memory.write_u32_be(port + 52, 0x0002_0003).unwrap();
    loaded.memory.write_u16_be(port + 56, 10).unwrap();
    loaded.memory.write_u8(port + 70, 0x05).unwrap();
    loaded.memory.write_u16_be(port + 72, 1).unwrap();
    loaded.memory.write_u32_be(port + 104, 0x0123_4560).unwrap();
    loaded.cpu.gpr[3] = port;
    loaded.cpu.gpr[4] = out;

    for (operation, expected) in [
        (PpcToolboxCompatOperation::GetPortPenMode, 10),
        (PpcToolboxCompatOperation::GetPortTextFace, 5),
        (PpcToolboxCompatOperation::GetPortTextMode, 1),
        (PpcToolboxCompatOperation::GetPortGrafProcs, 0x0123_4560),
    ] {
        assert!(
            matches!(compat(operation, &mut loaded), PpcImportAction::Return(value) if value == expected),
            "{operation:?}"
        );
    }
    compat(PpcToolboxCompatOperation::GetPortPenSize, &mut loaded);
    assert_eq!(loaded.memory.read_u32_be(out), Some(0x0002_0003));
    compat(PpcToolboxCompatOperation::GetPortForeColor, &mut loaded);
    assert_eq!(loaded.memory.read_u16_be(out), Some(0x1111));
    assert_eq!(loaded.memory.read_u16_be(out + 4), Some(0x3333));
    compat(PpcToolboxCompatOperation::GetPortBackColor, &mut loaded);
    assert_eq!(loaded.memory.read_u16_be(out), Some(0xaaaa));

    // A basic GrafPort reports black on white.
    loaded.memory.write_u16_be(port + 6, 0).unwrap();
    compat(PpcToolboxCompatOperation::GetPortBackColor, &mut loaded);
    assert_eq!(loaded.memory.read_u16_be(out + 2), Some(0xffff));

    let (pixmap, master) = (base + 0x40, base + 0x50);
    loaded.memory.write_u32_be(pixmap, master).unwrap();
    for (index, value) in [0u16, 0, 480, 640].into_iter().enumerate() {
        loaded
            .memory
            .write_u16_be(master + 6 + 2 * index as u32, value)
            .unwrap();
    }
    loaded.cpu.gpr[3] = pixmap;
    compat(PpcToolboxCompatOperation::GetPixBounds, &mut loaded);
    assert_eq!(loaded.memory.read_u16_be(out + 6), Some(640));
}

#[test]
fn carbon_and_legacy_gap_symbols_resolve_to_native_targets() {
    for (library, symbol, target) in [
        (
            "CarbonLib",
            "GetPortForeColor",
            PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::GetPortForeColor),
        ),
        (
            "CarbonLib",
            "GetPortClipRegion",
            PpcImportDispatcherTarget::GetPortClipRegion,
        ),
        (
            "InterfaceLib",
            "LMSetResLoad",
            PpcImportDispatcherTarget::SetResLoad,
        ),
        (
            "InterfaceLib",
            "PtInIconID",
            PpcImportDispatcherTarget::PtInRect,
        ),
        (
            "InterfaceLib",
            "UnmountVol",
            PpcImportDispatcherTarget::ReturnError(-47),
        ),
        (
            "InterfaceLib",
            "CountADBs",
            PpcImportDispatcherTarget::ReturnNoErr,
        ),
        (
            "InterfaceLib",
            "GetIndADB",
            PpcImportDispatcherTarget::ReturnError(-1),
        ),
        (
            "InterfaceLib",
            "AERemoveEventHandler",
            PpcImportDispatcherTarget::AERemoveEventHandler,
        ),
        (
            "InterfaceLib",
            "TESelView",
            PpcImportDispatcherTarget::TESelView,
        ),
    ] {
        assert_eq!(
            dispatcher_target_for_import(library, symbol),
            target,
            "{symbol}"
        );
    }
}
