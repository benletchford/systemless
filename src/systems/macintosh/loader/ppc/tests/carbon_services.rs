use super::*;

fn pm(operation: PpcCarbonPrintingOperation) -> PpcImportDispatcherTarget {
    PpcImportDispatcherTarget::CarbonPrinting(operation)
}

fn run_pm(loaded: &mut PpcLoadedApp, operation: PpcCarbonPrintingOperation) -> i16 {
    run_test_import(loaded, pm(operation));
    loaded.cpu.gpr[3] as i16
}

#[test]
fn carbon_scrap_flavors_follow_the_current_scrap_reference() {
    for (symbol, target) in [
        (
            "GetCurrentScrap",
            PpcImportDispatcherTarget::GetCurrentScrap,
        ),
        (
            "GetScrapFlavorSize",
            PpcImportDispatcherTarget::GetScrapFlavorSize,
        ),
        (
            "GetScrapFlavorData",
            PpcImportDispatcherTarget::GetScrapFlavorData,
        ),
        ("PutScrapFlavor", PpcImportDispatcherTarget::PutScrapFlavor),
    ] {
        assert_eq!(dispatcher_target_for_import("CarbonLib", symbol), target);
    }
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetCurrentScrap")).unwrap();
    let scratch = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(scratch, vec![0; 0x100]);
    loaded.memory.write_bytes(scratch + 0x40, b"hello").unwrap();
    let text = u32::from_be_bytes(*b"TEXT");
    let current = |loaded: &mut PpcLoadedApp| {
        loaded.cpu.gpr[3] = scratch;
        run_test_import(loaded, PpcImportDispatcherTarget::GetCurrentScrap);
        assert_eq!(loaded.cpu.gpr[3], 0);
        loaded.memory.read_u32_be(scratch).unwrap()
    };
    let put = |loaded: &mut PpcLoadedApp, scrap: u32| {
        loaded.cpu.gpr[3] = scrap;
        loaded.cpu.gpr[4] = text;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 5;
        loaded.cpu.gpr[7] = scratch + 0x40;
        run_test_import(loaded, PpcImportDispatcherTarget::PutScrapFlavor);
        loaded.cpu.gpr[3] as i16
    };

    // ClearCurrentScrap replaces the scrap, so an older reference is stale.
    let stale = current(&mut loaded);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ZeroScrap);
    let scrap = current(&mut loaded);
    assert_ne!(scrap, stale);
    assert_eq!(put(&mut loaded, stale), -4990);
    assert_eq!(put(&mut loaded, scrap), 0);
    // A flavor can be put only once per scrap.
    assert_eq!(put(&mut loaded, scrap), -4989);

    loaded.cpu.gpr[3] = scrap;
    loaded.cpu.gpr[4] = text;
    loaded.cpu.gpr[5] = scratch + 4;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetScrapFlavorSize);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(scratch + 4), Some(5));

    // GetScrapFlavorData copies at most the requested byte count, but
    // reports the full available size so the caller can resize its buffer.
    loaded.memory.write_u32_be(scratch + 4, 3).unwrap();
    loaded.cpu.gpr[3] = scrap;
    loaded.cpu.gpr[4] = text;
    loaded.cpu.gpr[5] = scratch + 4;
    loaded.cpu.gpr[6] = scratch + 0x80;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetScrapFlavorData);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(scratch + 4), Some(5));
    assert_eq!(
        (0..4)
            .map(|offset| loaded.memory.read_u8(scratch + 0x80 + offset).unwrap())
            .collect::<Vec<_>>(),
        b"hel\0"
    );

    // The classic Scrap Manager sees the same desk scrap.
    assert_eq!(
        loaded.scrap.desktop.flavor(*b"TEXT").unwrap().data,
        b"hello"
    );
    loaded.cpu.gpr[3] = scrap;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"PICT");
    loaded.cpu.gpr[5] = scratch + 4;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetScrapFlavorSize);
    assert_eq!(loaded.cpu.gpr[3] as i16, -102);
}

#[test]
fn carbon_printing_objects_work_without_a_printer() {
    for (symbol, operation) in [
        ("PMBegin", PpcCarbonPrintingOperation::Begin),
        ("PMNewPageFormat", PpcCarbonPrintingOperation::NewPageFormat),
        (
            "PMFlattenPrintSettings",
            PpcCarbonPrintingOperation::FlattenPrintSettings,
        ),
        ("PMEndPage", PpcCarbonPrintingOperation::EndPage),
    ] {
        assert_eq!(
            dispatcher_target_for_import("CarbonLib", symbol),
            pm(operation)
        );
    }
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"PMBegin")).unwrap();
    let scratch = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(scratch, vec![0; 0x100]);

    // Calls outside PMBegin and PMEnd are out of scope.
    loaded.cpu.gpr[3] = scratch;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::NewPageFormat),
        -30871
    );
    assert_eq!(run_pm(&mut loaded, PpcCarbonPrintingOperation::Begin), 0);
    loaded.cpu.gpr[3] = scratch;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::NewPageFormat),
        0
    );
    let format = loaded.memory.read_u32_be(scratch).unwrap();
    assert_ne!(format, 0);
    loaded.cpu.gpr[3] = scratch + 4;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::NewPrintSettings),
        0
    );
    let settings = loaded.memory.read_u32_be(scratch + 4).unwrap();
    loaded.cpu.gpr[3] = format;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::DefaultPageFormat),
        0
    );
    // A print settings object is not a page format.
    loaded.cpu.gpr[3] = settings;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::DefaultPageFormat),
        -30876
    );
    loaded.memory.write_u8(scratch + 8, 0xff).unwrap();
    loaded.cpu.gpr[3] = format;
    loaded.cpu.gpr[4] = scratch + 8;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::ValidatePageFormat),
        0
    );
    assert_eq!(loaded.memory.read_u8(scratch + 8), Some(0));

    loaded.cpu.gpr[3] = format;
    loaded.cpu.gpr[4] = scratch + 0x10;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::GetAdjustedPageRect),
        0
    );
    let rect: Vec<f64> = (0..4)
        .map(|index| {
            let bits = loaded
                .memory
                .read_u64_be(scratch + 0x10 + 8 * index)
                .unwrap();
            f64::from_bits(bits)
        })
        .collect();
    assert_eq!(rect, [0.0, 0.0, 756.0, 576.0]);

    // Flattening and unflattening round-trips through a caller-owned handle.
    loaded.cpu.gpr[3] = format;
    loaded.cpu.gpr[4] = scratch + 0x30;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::FlattenPageFormat),
        0
    );
    let flat = loaded.memory.read_u32_be(scratch + 0x30).unwrap();
    assert_ne!(flat, 0);
    loaded.cpu.gpr[3] = flat;
    loaded.cpu.gpr[4] = scratch + 0x34;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::UnflattenPageFormat),
        0
    );
    let copy = loaded.memory.read_u32_be(scratch + 0x34).unwrap();
    assert_ne!(copy, format);
    loaded.cpu.gpr[3] = flat;
    loaded.cpu.gpr[4] = scratch + 0x34;
    assert_eq!(
        run_pm(
            &mut loaded,
            PpcCarbonPrintingOperation::UnflattenPrintSettings
        ),
        -30875
    );

    // With no printer the dialogs are not accepted and no job can start.
    loaded.memory.write_u8(scratch + 8, 1).unwrap();
    loaded.cpu.gpr[3] = format;
    loaded.cpu.gpr[4] = scratch + 8;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::PageSetupDialog),
        0
    );
    assert_eq!(loaded.memory.read_u8(scratch + 8), Some(0));
    loaded.cpu.gpr[3] = settings;
    loaded.cpu.gpr[4] = format;
    loaded.cpu.gpr[5] = scratch + 0x38;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::BeginDocument),
        -30872
    );
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x38), Some(0));
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::EndPage),
        -30871
    );

    loaded.cpu.gpr[3] = copy;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::DisposePageFormat),
        0
    );
    loaded.cpu.gpr[3] = copy;
    assert_eq!(
        run_pm(&mut loaded, PpcCarbonPrintingOperation::DisposePageFormat),
        -30876
    );
    assert_eq!(run_pm(&mut loaded, PpcCarbonPrintingOperation::End), 0);
    assert_eq!(run_pm(&mut loaded, PpcCarbonPrintingOperation::End), -30871);
}

fn start_nav_put_file(loaded: &mut PpcLoadedApp, options: u32, reply: u32) {
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = reply;
    loaded.cpu.gpr[5] = options;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[8] = u32::from_be_bytes(*b"ttxt");
    loaded.cpu.gpr[9] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert!(loaded.toolbox_startup.standard_file_put_tracking.is_some());
}

fn key_event(message: u32) -> PpcQueuedEvent {
    PpcQueuedEvent {
        what: 3,
        message,
        when: 0,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    }
}

#[test]
fn nav_put_file_uses_the_put_dialog_and_returns_an_fsspec_selection() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "NavPutFile"),
        PpcImportDispatcherTarget::StandardFileCompatibility(PpcStandardFileOperation::NavPutFile)
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "NavGetDefaultDialogOptions"),
        PpcImportDispatcherTarget::NavGetDefaultDialogOptions
    );
    let options = PPC_DATA_BASE + 0x4000;
    let reply = PPC_DATA_BASE + 0x5000;
    let count = PPC_DATA_BASE + 0x5200;

    let mut setup =
        load_pef_application(&synthetic_pef_with_import(b"NavGetDefaultDialogOptions")).unwrap();
    setup.set_launched_app_path("Airburst/Airburst Carbon");
    setup.memory.add_region(options, vec![0xaa; 2048]);
    setup.cpu.gpr[3] = options;
    run_test_import(
        &mut setup,
        PpcImportDispatcherTarget::NavGetDefaultDialogOptions,
    );
    assert_eq!(setup.cpu.gpr[3], 0);
    assert_eq!(setup.memory.read_u16_be(options), Some(0));
    assert_eq!(setup.memory.read_u32_be(options + 2), Some(0xe4));
    assert_eq!(setup.memory.read_u32_be(options + 6), Some(u32::MAX));
    assert_eq!(
        ppc_read_pstring_bytes(&mut setup.memory, options + 10),
        Some(b"Airburst Carbon".to_vec())
    );
    assert_eq!(setup.memory.read_u8(options + 1034), Some(0));

    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NavPutFile")).unwrap();
    loaded.memory.add_region(options, vec![0; 2048]);
    loaded.memory.add_region(reply, vec![0xaa; 0x300]);
    write_ppc_pstring(&mut loaded.memory, options + 1034, b"Scores");
    start_nav_put_file(&mut loaded, options, reply);
    loaded.set_event_queue([key_event((u32::from(PPC_KEY_RETURN) << 8) | 0x0d)]);
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(reply), Some(0));
    assert_eq!(loaded.memory.read_u8(reply + 2), Some(1));
    assert_eq!(loaded.memory.read_u8(reply + 3), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(reply + 6),
        Some(u32::from_be_bytes(*b"list"))
    );
    assert_ne!(loaded.memory.read_u32_be(reply + 10), Some(0));
    assert_eq!(loaded.memory.read_u8(reply + 255), Some(0));

    // The selection is a one-item AEDescList holding the FSSpec.
    loaded.cpu.gpr[3] = reply + 6;
    loaded.cpu.gpr[4] = count;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CountItems,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(count), Some(1));

    loaded.cpu.gpr[3] = reply;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::NavDisposeReply,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(reply + 6), Some(0));
    assert_eq!(loaded.memory.read_u32_be(reply + 10), Some(0));

    // Cancelling returns userCanceledErr and an invalid reply.
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NavPutFile")).unwrap();
    loaded.memory.add_region(options, vec![0; 2048]);
    loaded.memory.add_region(reply, vec![0xaa; 0x300]);
    start_nav_put_file(&mut loaded, options, reply);
    loaded.set_event_queue([key_event(u32::from(PPC_KEY_ESCAPE) << 8)]);
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(loaded.cpu.gpr[3] as i16, -128);
    assert_eq!(loaded.memory.read_u8(reply + 2), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(reply + 6),
        Some(u32::from_be_bytes(*b"null"))
    );
}

#[test]
fn nav_put_file_host_dialog_preserves_save_and_cancel_replies() {
    use crate::systems::macintosh::host_ui::{FileDialogAction, FileDialogReply};

    for cancel in [false, true] {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NavPutFile")).unwrap();
        loaded.toolbox_startup.host_ui.enabled = true;
        let options = PPC_DATA_BASE + 0x4000;
        let result = PPC_DATA_BASE + 0x5000;
        loaded.memory.add_region(options, vec![0; 2048]);
        loaded.memory.add_region(result, vec![0xaa; 0x300]);
        write_ppc_pstring(&mut loaded.memory, options + 1034, b"Scores");
        start_nav_put_file(&mut loaded, options, result);
        let snapshot = loaded
            .host_file_dialog()
            .expect("Navigation save is projected");
        let reply = FileDialogReply {
            id: snapshot.id,
            revision: snapshot.revision,
            action: if cancel {
                FileDialogAction::Cancel
            } else {
                FileDialogAction::Save("Scores".into())
            },
        };
        assert!(loaded.reply_host_file_dialog(reply.clone()));
        assert!(!loaded.reply_host_file_dialog(reply));
        assert!(matches!(
            loaded.run_with_hle_imports(64).result,
            PpcRunResult::Halted { .. }
        ));
        assert!(loaded.host_file_dialog().is_none());
        assert_eq!(loaded.cpu.gpr[3] as i16, if cancel { -128 } else { 0 });
        assert_eq!(loaded.memory.read_u8(result + 2), Some(u8::from(!cancel)));
        assert_eq!(
            loaded.memory.read_u32_be(result + 6),
            Some(u32::from_be_bytes(if cancel { *b"null" } else { *b"list" }))
        );
        if !cancel {
            let count = result + 0x200;
            loaded.cpu.gpr[3] = result + 6;
            loaded.cpu.gpr[4] = count;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::CountItems,
                ),
            );
            assert_eq!(loaded.memory.read_u32_be(count), Some(1));
        }
    }
}
