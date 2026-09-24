use super::*;
use crate::loader::pef::PefResolvedImport;

// Low-level allocator tests borrow adapter fields independently. Keep the
// cursor canonical by publishing the temporary value when each call ends.
macro_rules! test_heap_cursor {
    ($app:ident) => {
        &mut *$app.process_memory_manager.heap_cursor_mut()
    };
}

macro_rules! test_handles {
    ($app:ident) => {
        &mut *$app.process_memory_manager.handles_mut()
    };
}

macro_rules! test_handle_records {
    ($app:ident) => {
        $app.process_memory_manager.handles()
    };
}

// Read the canonical heap limit through the disjoint manager field.
macro_rules! test_heap_limit {
    ($app:ident) => {
        $app.process_memory_manager.heap_limit($app.stack_base)
    };
}

macro_rules! with_test_screen_clut {
    ($app:ident, |$screen:ident| $body:expr) => {{
        let screen_clut = $app.screen_clut.shared_handle();
        screen_clut.with_mut(|$screen| $body)
    }};
}

macro_rules! with_test_color_manager_clut {
    ($app:ident, |$logical:ident| $body:expr) => {{
        let color_manager_clut = $app.color_manager_clut.shared_handle();
        color_manager_clut.with_mut(|$logical| $body)
    }};
}

macro_rules! with_test_display_cluts {
    ($app:ident, |$screen:ident, $logical:ident| $body:expr) => {{
        let screen_clut = $app.screen_clut.shared_handle();
        let color_manager_clut = $app.color_manager_clut.shared_handle();
        screen_clut.with_mut(|$screen| {
            color_manager_clut.with_mut(|$logical| $body)
        })
    }};
}

macro_rules! with_test_controls {
    ($app:ident, |$controls:ident| $body:expr) => {{
        let controls = $app.controls.shared_handle();
        controls.with_mut(|$controls| $body)
    }};
}
use crate::cpu::{CpuOps, Register};
use crate::managers::resource::serialize_resource_fork;
use crate::memory::MemoryBus;
use crate::trap::test_helpers::{setup_with_port, TEST_SP};
use ppc::PpcMemory;

fn test_q3_object(object: u32, object_type: u32) -> PpcQ3ObjectRecord {
    PpcQ3ObjectRecord {
        object,
        kind: PpcQ3ObjectKind::Generic,
        object_type,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    }
}

fn run_test_import(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) {
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = target;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    drain_test_m68k_guest_calls(loaded);
}

fn initialize_test_cgraf_port(bus: &mut MacMemoryBus, port: u32) {
    bus.write_word(port + 6, 0xc000);
    for offset in [36, 38, 40] {
        bus.write_word(port + offset, 0);
    }
    for offset in [42, 44, 46] {
        bus.write_word(port + offset, u16::MAX);
    }
    bus.write_word(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 1);
    bus.write_word(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, 1);
    bus.write_word(port + PPC_CGRAF_PORT_PN_MODE_OFFSET, PPC_QD_PEN_MODE_PAT_COPY as u16);
    bus.write_word(
        port + PPC_CGRAF_PORT_TX_MODE_OFFSET,
        PPC_QD_TEXT_MODE_SRC_OR as u16,
    );
    bus.write_word(
        port + PPC_CGRAF_PORT_TX_SIZE_OFFSET,
        PPC_QD_TEXT_SIZE_SYSTEM as u16,
    );
}

fn run_test_import_with_process_memory_manager(
    loaded: &mut PpcLoadedApp,
    target: PpcImportDispatcherTarget,
    memory_manager: &SharedProcessMemoryManager,
) {
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = target;
    let probe = loaded.run_with_process_memory_manager(
        64,
        false,
        false,
        &mut memory_manager.borrow_mut(),
    );
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    drain_test_m68k_guest_calls(loaded);
}

fn attach_test_classic_heap(
    native: &mut PpcLoadedApp,
    context: &mut ProcessContext,
    ram_size: usize,
    heap_len: usize,
) -> MacMemoryBus {
    let mut classic_bus = MacMemoryBus::new(ram_size);
    context.attach_classic_memory_bus(&mut classic_bus);
    let classic_heap = classic_bus
        .shared_ram_region(0x0020_0000, heap_len as u32)
        .expect("classic adapter owns its heap range");
    context.attach_memory(0x0020_0000, classic_heap, &mut native.memory);
    classic_bus
}

fn drain_test_m68k_guest_calls(loaded: &mut PpcLoadedApp) {
    while let Some(pending) = loaded.guest_calls()
        .active_m68k()
        .or_else(|| loaded.guest_calls().activate_m68k())
    {
        let mut cpu = m68k::CpuCore::new();
        cpu.set_cpu_type(REFERENCE_MACHINE_PROFILE.cpu_type());
        cpu.pc = pending.entry;
        cpu.set_a(7, pending.initial_sp);
        for (index, value) in pending.registers.data.into_iter().enumerate() {
            cpu.set_d(index, value);
        }
        for (index, value) in pending.registers.address.into_iter().enumerate() {
            cpu.set_a(index, value);
        }
        let result = cpu.run_batch(&mut loaded.memory, 4096, &[pending.return_pc]);
        assert_eq!(
            result.exit,
            m68k::BatchExit::WatchedPc {
                pc: pending.return_pc
            },
            "entry=${:08X} pc=${:08X} sp=${:08X} return=${:08X}",
            pending.entry,
            cpu.pc,
            cpu.a(7),
            pending.return_pc,
        );
        let result = match pending.result {
            None => None,
            Some(crate::guest_call::M68kResultSource::Data(index)) => {
                Some(cpu.d(usize::from(index)))
            }
            Some(crate::guest_call::M68kResultSource::Address(index)) => {
                Some(cpu.a(usize::from(index)))
            }
            Some(crate::guest_call::M68kResultSource::Memory { address, size }) => {
                Some(match size {
                    1 => u32::from(loaded.memory.read_u8(address).unwrap()),
                    2 => u32::from(loaded.memory.read_u16_be(address).unwrap()),
                    4 => loaded.memory.read_u32_be(address).unwrap(),
                    _ => panic!("unsupported 68k result size {size}"),
                })
            }
            Some(crate::guest_call::M68kResultSource::SpecialCase { selector, .. }) => {
                panic!("special-case result selector {selector} requires the shared runner")
            }
        };
        assert!(loaded
            .toolbox_startup
            .execution
            .calls()
            .complete_m68k_operation_for_powerpc(
            cpu.pc,
            cpu.a(7),
            result,
            &mut loaded.cpu,
            &mut loaded.memory,
            loaded.process_memory_manager.0.borrow_mut().native_mut(),
        ));

        if loaded.cpu.pc == PPC_GUEST_CALL_RETURN_PC
            || (loaded.cpu.pc >= loaded.import_trap_base
                && loaded.cpu.pc
                    < loaded
                        .import_trap_base
                        .saturating_add(loaded.import_count.saturating_mul(4)))
        {
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.unsupported_import_index, None);
        }
    }
}

#[test]
fn ppc_watch_parser_accepts_hex_address_and_optional_length() {
    assert_eq!(
        parse_ppc_watch_range_value(std::ffi::OsStr::new("0x02000000")),
        Some(PpcWatchRange {
            start: 0x0200_0000,
            len: 1
        })
    );
    assert_eq!(
        parse_ppc_watch_range_value(std::ffi::OsStr::new("$02000000:4")),
        Some(PpcWatchRange {
            start: 0x0200_0000,
            len: 4
        })
    );
    assert_eq!(
        parse_ppc_watch_range_value(std::ffi::OsStr::new("02000000:0x10")),
        Some(PpcWatchRange {
            start: 0x0200_0000,
            len: 16
        })
    );
}

#[test]
fn ppc_watch_parser_rejects_empty_zero_and_overflow_ranges() {
    assert_eq!(parse_ppc_watch_range_value(std::ffi::OsStr::new("")), None);
    assert_eq!(
        parse_ppc_watch_range_value(std::ffi::OsStr::new("02000000:0")),
        None
    );
    assert_eq!(
        parse_ppc_watch_range_value(std::ffi::OsStr::new("FFFFFFFF:2")),
        None
    );
    assert_eq!(
        parse_ppc_watch_range_value(std::ffi::OsStr::new("02000000:4:extra")),
        None
    );
}

#[test]
fn ppc_watch_range_contains_only_selected_bytes() {
    let range = PpcWatchRange {
        start: 0x0200_0002,
        len: 3,
    };

    assert!(!range.contains(0x0200_0001));
    assert!(range.contains(0x0200_0002));
    assert!(range.contains(0x0200_0004));
    assert!(!range.contains(0x0200_0005));
}

#[test]
fn ppc_watch_formatter_includes_context_and_written_byte() {
    let line = format_ppc_watch_write(PpcWatchWriteRecord {
        pc: 0x0100_0004,
        lr: 0x0100_0010,
        rtoc: 0x0200_0000,
        sp: 0x03fe_ffc0,
        addr: 0x0200_0002,
        value: 0xaa,
    });

    assert_eq!(
        line,
        "[PPC-WATCH] pc=$01000004 lr=$01000010 rtoc=$02000000 sp=$03FEFFC0 addr=$02000002 value=$AA"
    );
}

#[test]
fn ppc_trace_fetch_formatter_includes_pc_and_instruction_word() {
    assert_eq!(
        format_ppc_trace_fetch(0x0100_0004, 0x4e80_0020),
        "[PPC-TRACE] fetch pc=$01000004 word=$4E800020"
    );
}

#[test]
fn ppc_trace_import_formatter_includes_import_and_context() {
    let entry = PpcHleImportTraceEntry {
        import_index: 12,
        library_name: "InterfaceLib".to_string(),
        symbol_name: "NewPtrClear".to_string(),
        pc: 0x01f0_0030,
        lr: 0x0100_0010,
        rtoc: 0x0200_0000,
        sp: 0x03fe_ffc0,
        dispatcher_target: PpcImportDispatcherTarget::NewPtr { clear: true },
        repeat_count: 1,
    };

    assert_eq!(
        format_ppc_trace_import(&entry),
        "[PPC-TRACE] import #12 InterfaceLib:NewPtrClear pc=$01F00030 lr=$01000010 rtoc=$02000000 sp=$03FEFFC0 target=NewPtr { clear: true }"
    );
    assert_eq!(
        format_ppc_trace_unknown_import(13, 0x01f0_0034, 0x0100_0020, 0x0200_0004, 0x03fe_ffb0),
        "[PPC-TRACE] import #13 <unknown> pc=$01F00034 lr=$01000020 rtoc=$02000004 sp=$03FEFFB0"
    );
}

#[test]
fn marathon_runtime_imports_have_native_dispatch_targets() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "EventAvail"),
        PpcImportDispatcherTarget::EventAvail
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBControlSync"),
        PpcImportDispatcherTarget::PBControl
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HandToHand"),
        PpcImportDispatcherTarget::HandToHand
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetEntries"),
        PpcImportDispatcherTarget::SetEntries
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "RestoreEntries"),
        PpcImportDispatcherTarget::RestoreEntries
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MakeITable"),
        PpcImportDispatcherTarget::MakeITable
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "QDError"),
        PpcImportDispatcherTarget::QDError
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "RGB2HSL"),
        PpcImportDispatcherTarget::RGB2HSL
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "RGB2HSV"),
        PpcImportDispatcherTarget::RGB2HSV
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HSV2RGB"),
        PpcImportDispatcherTarget::HSV2RGB
    );
    assert_eq!(
        dispatcher_target_for_import("AppearanceLib", "MenuEvent"),
        PpcImportDispatcherTarget::MenuEvent
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TEUseStyleScrap"),
        PpcImportDispatcherTarget::TEUseStyleScrap
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FixRatio"),
        PpcImportDispatcherTarget::FixRatio
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FixMul"),
        PpcImportDispatcherTarget::FixMul
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Long2Fix"),
        PpcImportDispatcherTarget::Long2Fix
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CountMItems"),
        PpcImportDispatcherTarget::CountMItems
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetItemCmd"),
        PpcImportDispatcherTarget::SetItemCmd
    );
}

#[test]
fn native_lmgetdefltstack_reads_the_live_low_memory_long() {
    let pef = synthetic_pef_with_import(b"LMGetDefltStack");
    let mut loaded = load_pef_application(&pef).unwrap();
    let address = crate::memory::globals::addr::DEFLT_STACK;

    assert_eq!(
        loaded.memory.read_u32_be(address),
        Some(crate::memory::globals::DEFAULT_DEFLT_STACK_SIZE)
    );

    loaded.memory.write_u32_be(address, 0x1234_5678).unwrap();
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetDefltStack);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
}

#[test]
fn native_lmgetcurstackbase_reads_the_live_low_memory_long() {
    let pef = synthetic_pef_with_import(b"LMGetCurStackBase");
    let mut loaded = load_pef_application(&pef).unwrap();
    let address = crate::memory::globals::addr::CUR_STACK_BASE;

    assert_eq!(loaded.memory.read_u32_be(address), Some(loaded.stack_base));

    loaded.memory.write_u32_be(address, 0x2345_6780).unwrap();
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetCurStackBase);
    assert_eq!(loaded.cpu.gpr[3], 0x2345_6780);
}

#[test]
fn legacy_window_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("BringToFront", PpcLegacyWindowOperation::BringToFront),
        ("CalcVis", PpcLegacyWindowOperation::CalculateVisibleRegion),
        ("DisposeWindow", PpcLegacyWindowOperation::DisposeWindow),
        ("DragWindow", PpcLegacyWindowOperation::DragWindow),
        ("GetNewWindow", PpcLegacyWindowOperation::GetNewWindow),
        ("GetWTitle", PpcLegacyWindowOperation::GetWindowTitle),
        ("GrowWindow", PpcLegacyWindowOperation::GrowWindow),
        ("HiliteWindow", PpcLegacyWindowOperation::HighlightWindow),
        ("NewWindow", PpcLegacyWindowOperation::NewWindow),
        ("SendBehind", PpcLegacyWindowOperation::SendBehind),
        ("SetWTitle", PpcLegacyWindowOperation::SetWindowTitle),
        ("TrackBox", PpcLegacyWindowOperation::TrackBox),
        ("TrackGoAway", PpcLegacyWindowOperation::TrackGoAway),
        ("ZoomWindow", PpcLegacyWindowOperation::ZoomWindow),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::LegacyWindow(operation),
        );
    }
}


#[test]
fn hle_import_runner_converts_fixed_to_extended_result_register() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"Fix2X")).unwrap();
    loaded.cpu.gpr[3] = (-98_304i32) as u32;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), -1.5);
}

#[test]
fn import_bindings_classify_mathlib_imports() {
    assert_eq!(
        dispatcher_target_for_import("MathLib", "ceil"),
        PpcImportDispatcherTarget::MathCeil
    );
    assert_eq!(
        dispatcher_target_for_import("MathLib", "sqrt"),
        PpcImportDispatcherTarget::MathSqrt
    );
    assert_eq!(
        dispatcher_target_for_import("MathLib", "exp"),
        PpcImportDispatcherTarget::MathExp
    );
    assert_eq!(
        dispatcher_target_for_import("MathLib", "sin"),
        PpcImportDispatcherTarget::MathSin
    );
    assert_eq!(
        dispatcher_target_for_import("MathLib", "cos"),
        PpcImportDispatcherTarget::MathCos
    );
    assert_eq!(
        dispatcher_target_for_import("MathLib", "atan2"),
        PpcImportDispatcherTarget::MathAtan2
    );
    assert_eq!(
        dispatcher_target_for_import("MathLib", "fmod"),
        PpcImportDispatcherTarget::MathFmod
    );
    assert_eq!(
        dispatcher_target_for_import("MathLib", "log10"),
        PpcImportDispatcherTarget::MathLog10
    );
    assert_eq!(
        dispatcher_target_for_import("MathLib", "dtox80"),
        PpcImportDispatcherTarget::MathDtox80
    );
}



#[test]
fn stdio_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("clearerr", PpcStdIoOperation::ClearErr),
        ("_filbuf", PpcStdIoOperation::FileBuffer),
        ("fclose", PpcStdIoOperation::FileClose),
        ("feof", PpcStdIoOperation::FileEof),
        ("ferror", PpcStdIoOperation::FileError),
        ("fflush", PpcStdIoOperation::FileFlush),
        ("fopen", PpcStdIoOperation::FileOpen),
        ("fprintf", PpcStdIoOperation::FilePrintf),
        ("fread", PpcStdIoOperation::FileRead),
        ("fseek", PpcStdIoOperation::FileSeek),
        ("ftell", PpcStdIoOperation::FileTell),
        ("fwrite", PpcStdIoOperation::FileWrite),
        ("_iob", PpcStdIoOperation::IoBuffer),
    ] {
        assert_eq!(
            dispatcher_target_for_import("StdCLib", symbol),
            PpcImportDispatcherTarget::StdIoCompatibility(operation),
        );
    }
}

#[test]
fn legacy_control_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("DisposeControl", PpcLegacyControlOperation::DisposeControl),
        ("Draw1Control", PpcLegacyControlOperation::DrawOneControl),
        ("FindControl", PpcLegacyControlOperation::FindControl),
        ("GetControlMaximum", PpcLegacyControlOperation::GetControlMaximum),
        ("GetControlMinimum", PpcLegacyControlOperation::GetControlMinimum),
        ("GetControlTitle", PpcLegacyControlOperation::GetControlTitle),
        ("GetControlValue", PpcLegacyControlOperation::GetControlValue),
        ("GetNewControl", PpcLegacyControlOperation::GetNewControl),
        ("HideControl", PpcLegacyControlOperation::HideControl),
        ("KillControls", PpcLegacyControlOperation::KillControls),
        ("MoveControl", PpcLegacyControlOperation::MoveControl),
        ("NewControl", PpcLegacyControlOperation::NewControl),
        ("SetControlMaximum", PpcLegacyControlOperation::SetControlMaximum),
        ("SetControlMinimum", PpcLegacyControlOperation::SetControlMinimum),
        ("ShowControl", PpcLegacyControlOperation::ShowControl),
        ("SizeControl", PpcLegacyControlOperation::SizeControl),
        ("TestControl", PpcLegacyControlOperation::TestControl),
        ("TrackControl", PpcLegacyControlOperation::TrackControl),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::LegacyControl(operation),
        );
    }
}

#[test]
fn import_bindings_classify_dialog_and_utility_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetNewDialog"),
        PpcImportDispatcherTarget::GetNewDialog
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewDialog"),
        PpcImportDispatcherTarget::NewDialog
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDialogItem"),
        PpcImportDispatcherTarget::GetDialogItem
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDialogItemText"),
        PpcImportDispatcherTarget::GetDialogItemText
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "getdialogitemtext"),
        PpcImportDispatcherTarget::GetDialogItemText
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetDialogItemText"),
        PpcImportDispatcherTarget::SetDialogItemText
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "setdialogitemtext"),
        PpcImportDispatcherTarget::SetDialogItemText
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetDialogTracksCursor"),
        PpcImportDispatcherTarget::SetDialogTracksCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "StdFilterProc"),
        PpcImportDispatcherTarget::StdFilterProc
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ModalDialog"),
        PpcImportDispatcherTarget::ModalDialog
    );
    for (symbol, operation) in [
        ("AppendDITL", PpcDialogCompatibilityOperation::AppendDitl),
        ("CountDITL", PpcDialogCompatibilityOperation::CountDitl),
        ("DialogSelect", PpcDialogCompatibilityOperation::DialogSelect),
        (
            "FindDialogItem",
            PpcDialogCompatibilityOperation::FindDialogItem,
        ),
        (
            "HideDialogItem",
            PpcDialogCompatibilityOperation::HideDialogItem,
        ),
        (
            "IsDialogEvent",
            PpcDialogCompatibilityOperation::IsDialogEvent,
        ),
        ("ShortenDITL", PpcDialogCompatibilityOperation::ShortenDitl),
        (
            "ShowDialogItem",
            PpcDialogCompatibilityOperation::ShowDialogItem,
        ),
        ("UpdateDialog", PpcDialogCompatibilityOperation::UpdateDialog),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::DialogCompatibility(operation),
        );
    }
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetControlTitle"),
        PpcImportDispatcherTarget::SetControlTitle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetControlValue"),
        PpcImportDispatcherTarget::SetControlValue
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetNextEvent"),
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent)
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "WaitNextEvent"),
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent)
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetOSEvent"),
        PpcImportDispatcherTarget::GetOSEvent
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "OSEventAvail"),
        PpcImportDispatcherTarget::OSEventAvail
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PostEvent"),
        PpcImportDispatcherTarget::PostEvent
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Button"),
        PpcImportDispatcherTarget::Button
    );
    for (symbol, target) in [
        ("InitGraf", PpcImportDispatcherTarget::InitGraf),
        ("InitFonts", PpcImportDispatcherTarget::InitFonts),
        ("InitWindows", PpcImportDispatcherTarget::InitWindows),
        ("InitMenus", PpcImportDispatcherTarget::InitMenus),
        ("TEInit", PpcImportDispatcherTarget::TEInit),
        ("InitDialogs", PpcImportDispatcherTarget::InitDialogs),
        ("FlushEvents", PpcImportDispatcherTarget::FlushEvents),
        ("CloseDialog", PpcImportDispatcherTarget::CloseDialog),
        ("DisposeDialog", PpcImportDispatcherTarget::DisposeDialog),
        ("DisposDialog", PpcImportDispatcherTarget::DisposeDialog),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetKeys"),
        PpcImportDispatcherTarget::GetKeys
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDateTime"),
        PpcImportDispatcherTarget::GetDateTime
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetTime"),
        PpcImportDispatcherTarget::GetTime
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Delay"),
        PpcImportDispatcherTarget::Delay
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDblTime"),
        PpcImportDispatcherTarget::GetDblTime
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetTime"),
        PpcImportDispatcherTarget::LMGetTime
    );
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "__setjmp"),
        PpcImportDispatcherTarget::ReturnNoErr
    );
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "sprintf"),
        PpcImportDispatcherTarget::StdSprintf
    );
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "time"),
        PpcImportDispatcherTarget::StdTime
    );
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "signal"),
        PpcImportDispatcherTarget::StdCCompatibility(PpcStdCCompatibilityOperation::Signal)
    );
    for (symbol, operation) in [
        ("qsort", PpcStdCCompatibilityOperation::Qsort),
        ("sscanf", PpcStdCCompatibilityOperation::Sscanf),
        ("strftime", PpcStdCCompatibilityOperation::Strftime),
        ("vsprintf", PpcStdCCompatibilityOperation::Vsprintf),
    ] {
        assert_eq!(
            dispatcher_target_for_import("StdCLib", symbol),
            PpcImportDispatcherTarget::StdCCompatibility(operation),
        );
    }
    for (symbol, operation) in [
        ("CountVoices", PpcSpeechCompatibilityOperation::CountVoices),
        (
            "DisposeSpeechChannel",
            PpcSpeechCompatibilityOperation::DisposeSpeechChannel,
        ),
        ("GetIndVoice", PpcSpeechCompatibilityOperation::GetIndVoice),
        (
            "GetVoiceDescription",
            PpcSpeechCompatibilityOperation::GetVoiceDescription,
        ),
        (
            "NewSpeechChannel",
            PpcSpeechCompatibilityOperation::NewSpeechChannel,
        ),
        ("SpeakString", PpcSpeechCompatibilityOperation::SpeakString),
        ("SpeakText", PpcSpeechCompatibilityOperation::SpeakText),
        ("SpeechBusy", PpcSpeechCompatibilityOperation::SpeechBusy),
    ] {
        assert_eq!(
            dispatcher_target_for_import("SpeechLib", symbol),
            PpcImportDispatcherTarget::SpeechCompatibility(operation),
        );
    }
    for (symbol, operation) in [
        ("SPBCloseDevice", PpcSoundInputCompatibilityOperation::CloseDevice),
        ("SPBGetDeviceInfo", PpcSoundInputCompatibilityOperation::GetDeviceInfo),
        ("SPBOpenDevice", PpcSoundInputCompatibilityOperation::OpenDevice),
        ("SPBRecord", PpcSoundInputCompatibilityOperation::Record),
        ("SPBSetDeviceInfo", PpcSoundInputCompatibilityOperation::SetDeviceInfo),
        ("SPBStopRecording", PpcSoundInputCompatibilityOperation::StopRecording),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::SoundInputCompatibility(operation),
        );
    }
    for (symbol, operation) in [
        ("PrClose", PpcPrintingCompatibilityOperation::PrClose),
        ("PrCloseDoc", PpcPrintingCompatibilityOperation::PrCloseDoc),
        ("PrClosePage", PpcPrintingCompatibilityOperation::PrClosePage),
        ("PrError", PpcPrintingCompatibilityOperation::PrError),
        ("PrJobDialog", PpcPrintingCompatibilityOperation::PrJobDialog),
        ("PrOpen", PpcPrintingCompatibilityOperation::PrOpen),
        ("PrOpenDoc", PpcPrintingCompatibilityOperation::PrOpenDoc),
        ("PrOpenPage", PpcPrintingCompatibilityOperation::PrOpenPage),
        ("PrPicFile", PpcPrintingCompatibilityOperation::PrPicFile),
        ("PrStlDialog", PpcPrintingCompatibilityOperation::PrStlDialog),
        ("PrintDefault", PpcPrintingCompatibilityOperation::PrintDefault),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::PrintingCompatibility(operation),
        );
    }
    for (symbol, operation) in [
        (
            "ISpDevices_ActivateClass",
            PpcInputSprocketCompatibilityOperation::DevicesActivateClass,
        ),
        (
            "ISpElement_DisposeVirtual",
            PpcInputSprocketCompatibilityOperation::ElementDisposeVirtual,
        ),
        (
            "ISpElement_Flush",
            PpcInputSprocketCompatibilityOperation::ElementFlush,
        ),
        (
            "ISpElement_GetNextEvent",
            PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
        ),
        ("ISpTickle", PpcInputSprocketCompatibilityOperation::Tickle),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InputSprocketLib", symbol),
            PpcImportDispatcherTarget::InputSprocketCompatibility(operation),
        );
    }
    for (symbol, operation) in [
        (
            "GetMovieTimeBase",
            PpcQuickTimeCompatibilityOperation::GetMovieTimeBase,
        ),
        (
            "GetMovieVolume",
            PpcQuickTimeCompatibilityOperation::GetMovieVolume,
        ),
        (
            "NewMovieFromDataFork",
            PpcQuickTimeCompatibilityOperation::NewMovieFromDataFork,
        ),
        ("PrerollMovie", PpcQuickTimeCompatibilityOperation::PrerollMovie),
        (
            "SetMovieVolume",
            PpcQuickTimeCompatibilityOperation::SetMovieVolume,
        ),
        (
            "SetTimeBaseFlags",
            PpcQuickTimeCompatibilityOperation::SetTimeBaseFlags,
        ),
        ("UpdateMovie", PpcQuickTimeCompatibilityOperation::UpdateMovie),
    ] {
        assert_eq!(
            dispatcher_target_for_import("QuickTimeLib", symbol),
            PpcImportDispatcherTarget::QuickTimeCompatibility(operation),
        );
    }
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SecondsToDate"),
        PpcImportDispatcherTarget::SecondsToDate
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Secs2Date"),
        PpcImportDispatcherTarget::SecondsToDate
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Microseconds"),
        PpcImportDispatcherTarget::Microseconds
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SysEnvirons"),
        PpcImportDispatcherTarget::SysEnvirons
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TextWidth"),
        PpcImportDispatcherTarget::TextWidth
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "StringWidth"),
        PpcImportDispatcherTarget::StringWidth
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "EqualString"),
        PpcImportDispatcherTarget::EqualString
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "X2Fix"),
        PpcImportDispatcherTarget::X2Fix
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NumToString"),
        PpcImportDispatcherTarget::NumToString
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "StringToNum"),
        PpcImportDispatcherTarget::StringToNum
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Random"),
        PpcImportDispatcherTarget::Random
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "p2cstr"),
        PpcImportDispatcherTarget::P2CStr
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "c2pstr"),
        PpcImportDispatcherTarget::C2PStr
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetCurrentProcess"),
        PpcImportDispatcherTarget::GetCurrentProcess
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "WakeUpProcess"),
        PpcImportDispatcherTarget::WakeUpProcess
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetProcessInformation"),
        PpcImportDispatcherTarget::GetProcessInformation
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ExitToShell"),
        PpcImportDispatcherTarget::ExitToShell
    );
}


#[test]
fn system_compatibility_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("BuildDDPwds", PpcSystemCompatibilityOperation::BuildDdPwds),
        ("CTBGetCTBVersion", PpcSystemCompatibilityOperation::CtbGetCtbVersion),
        ("CallComponentUPP", PpcSystemCompatibilityOperation::CallComponentUpp),
        ("DIBadMount", PpcSystemCompatibilityOperation::DiBadMount),
        ("DILoad", PpcSystemCompatibilityOperation::DiLoad),
        ("DIUnload", PpcSystemCompatibilityOperation::DiUnload),
        ("Debugger", PpcSystemCompatibilityOperation::Debugger),
        ("Dequeue", PpcSystemCompatibilityOperation::Dequeue),
        ("Enqueue", PpcSystemCompatibilityOperation::Enqueue),
        ("FindNextComponent", PpcSystemCompatibilityOperation::FindNextComponent),
        ("GetNextProcess", PpcSystemCompatibilityOperation::GetNextProcess),
        ("GetScript", PpcSystemCompatibilityOperation::GetScript),
        ("GetScriptManagerVariable", PpcSystemCompatibilityOperation::GetScriptManagerVariable),
        ("GetScriptVariable", PpcSystemCompatibilityOperation::GetScriptVariable),
        ("GetSysBeepVolume", PpcSystemCompatibilityOperation::GetSysBeepVolume),
        ("IUCompString", PpcSystemCompatibilityOperation::IuCompString),
        ("IUDateString", PpcSystemCompatibilityOperation::IuDateString),
        ("InitCRM", PpcSystemCompatibilityOperation::InitCrm),
        ("InitCTBUtilities", PpcSystemCompatibilityOperation::InitCtbUtilities),
        ("KeyTranslate", PpcSystemCompatibilityOperation::KeyTranslate),
        ("LaunchApplication", PpcSystemCompatibilityOperation::LaunchApplication),
        ("LMGetCurApName", PpcSystemCompatibilityOperation::LmGetCurApName),
        ("LMGetSysFontFam", PpcSystemCompatibilityOperation::LmGetSysFontFam),
        ("LMGetSysFontSize", PpcSystemCompatibilityOperation::LmGetSysFontSize),
        ("MIDIAddPort", PpcSystemCompatibilityOperation::MidiAddPort),
        ("MIDIRemovePort", PpcSystemCompatibilityOperation::MidiRemovePort),
        ("MIDISignOut", PpcSystemCompatibilityOperation::MidiSignOut),
        ("MIDIWritePacket", PpcSystemCompatibilityOperation::MidiWritePacket),
        ("Munger", PpcSystemCompatibilityOperation::Munger),
        ("NMRemove", PpcSystemCompatibilityOperation::NmRemove),
        ("ObscureCursor", PpcSystemCompatibilityOperation::ObscureCursor),
        ("OpenDefaultComponent", PpcSystemCompatibilityOperation::OpenDefaultComponent),
        ("ResetAlertStage", PpcSystemCompatibilityOperation::ResetAlertStage),
        ("SetFrontProcess", PpcSystemCompatibilityOperation::SetFrontProcess),
        ("StyledLineBreak", PpcSystemCompatibilityOperation::StyledLineBreak),
        ("SystemEdit", PpcSystemCompatibilityOperation::SystemEdit),
        ("TruncText", PpcSystemCompatibilityOperation::TruncText),
        ("UpperString", PpcSystemCompatibilityOperation::UpperString),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::SystemCompatibility(operation),
        );
    }
}

#[test]
fn import_run_state_preserves_sparse_first_match_lookup() {
    let first = PpcImportBinding {
        library_index: 0,
        symbol_index: 52,
        library_name: "InterfaceLib".to_string(),
        symbol_name: "GetKeys".to_string(),
        class: 0,
        weak: false,
        address: 0,
        tvector_address: None,
        trap_pc: 0,
        dispatcher_target: PpcImportDispatcherTarget::GetKeys,
    };
    let mut duplicate = first.clone();
    duplicate.symbol_name = "DuplicateGetKeys".to_string();
    duplicate.dispatcher_target = PpcImportDispatcherTarget::Unsupported;
    let mut out_of_range = first.clone();
    out_of_range.symbol_index = 999;

    let state = PpcImportRunState::from_parts(
        vec![first.clone(), duplicate, out_of_range],
        64,
        ppc_import_layout(),
    );

    assert_eq!(state.binding_cloned(52), Some(first));
    assert_eq!(state.binding_cloned(51), None);
    assert_eq!(state.binding_cloned(53), None);
    assert_eq!(state.binding_cloned(999), None);
}

#[test]
fn startup_probe_halts_at_first_import_and_preserves_symbol_index() {
    let pef = synthetic_pef();
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_until_import_or_fault(64);

    assert_eq!(probe.first_import_index, Some(0));
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_IMPORT_TRAP_BASE,
            cycles: 4,
        }
    );
    let binding = loaded.import_binding(0).unwrap();
    assert_eq!(binding.library_name, "InterfaceLib");
    assert_eq!(binding.symbol_name, "TestImport");
}

#[test]
fn hle_import_runner_reports_unsupported_import_without_side_effects() {
    let pef = synthetic_pef();
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 0);
    assert_eq!(probe.last_import_index, Some(0));
    assert_eq!(probe.unsupported_import_index, Some(0));
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_IMPORT_TRAP_BASE,
            cycles: 4,
        }
    );
    assert_eq!(loaded.heap_cursor(), PPC_HEAP_BASE);
}


#[test]
fn hle_import_runner_reuses_retained_state_allocations_between_slices() {
    let pef = synthetic_pef_with_import(b"TickCount");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.q3_objects.reserve(8);
    loaded.q3_objects.push(test_q3_object(0x1000, 0x2000));
    let imports_ptr = loaded.imports.as_ptr();
    let imports_capacity = loaded.imports.capacity();
    let q3_objects_ptr = loaded.q3_objects.as_ptr();
    let q3_objects_capacity = loaded.q3_objects.capacity();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.imports.as_ptr(), imports_ptr);
    assert_eq!(loaded.imports.capacity(), imports_capacity);
    assert_eq!(loaded.q3_objects.as_ptr(), q3_objects_ptr);
    assert_eq!(loaded.q3_objects.capacity(), q3_objects_capacity);
    assert_eq!(loaded.q3_objects.len(), 1);
}

#[test]
fn tick_count_import_calls_live_native_trap_patch_with_and_without_tracing() {
    const DESCRIPTOR: u32 = PPC_HEAP_BASE + 0x1000;
    const TVECTOR: u32 = PPC_HEAP_BASE + 0x1080;
    const CALLBACK: u32 = PPC_HEAP_BASE + 0x2000;
    const CALLBACK_RTOC: u32 = PPC_HEAP_BASE + 0x3000;
    const RESULT: u32 = 0x1234_5678;

    for trace in [false, true] {
        let mut loaded =
            load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
        let table_entry = ppc_raw_trap_table_entry(0xA975, true);
        let initial_handler = if trace { 0x00f0_0000 } else { DESCRIPTOR };
        loaded
            .memory
            .add_region(table_entry, initial_handler.to_be_bytes().to_vec());
        if trace {
            assert!(ppc_set_logical_trap_address(
                &mut loaded.memory,
                0xA975,
                true,
                DESCRIPTOR,
            ));
        }
        loaded.memory.add_region(DESCRIPTOR, vec![0; 0x100]);
        loaded.memory.add_region(CALLBACK_RTOC, vec![0; 0x100]);

        let mut callback = Vec::new();
        for word in [
            d_form_u(15, 3, 0, (RESULT >> 16) as u16),
            d_form_u(24, 3, 3, RESULT as u16),
            BLR,
        ] {
            callback.extend_from_slice(&word.to_be_bytes());
        }
        loaded.memory.add_region(CALLBACK, callback);
        loaded
            .memory
            .write_u16_be(DESCRIPTOR, PPC_MIXED_MODE_TRAP)
            .unwrap();
        loaded
            .memory
            .write_u8(DESCRIPTOR + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
            .unwrap();
        loaded.memory.write_u16_be(DESCRIPTOR + 10, 0).unwrap();
        ppc_write_routine_record(
            &mut loaded.memory,
            DESCRIPTOR + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE,
            0x30,
            PPC_ROUTINE_RECORD_POWERPC_ISA,
            PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
            TVECTOR,
        );
        loaded.memory.write_u32_be(TVECTOR, CALLBACK).unwrap();
        loaded
            .memory
            .write_u32_be(TVECTOR + 4, CALLBACK_RTOC)
            .unwrap();

        let probe = if trace {
            loaded.run_with_hle_import_trace(64)
        } else {
            loaded.run_with_hle_imports(64)
        };

        assert_eq!(probe.handled_import_count, 1, "trace={trace}");
        assert_eq!(probe.unsupported_import_index, None, "trace={trace}");
        assert_eq!(loaded.cpu.gpr[3], RESULT, "trace={trace}");
        assert!(loaded.guest_calls().is_empty(), "trace={trace}");
        assert_eq!(probe.import_trace.len(), usize::from(trace), "trace={trace}");
    }
}

#[test]
fn tick_count_import_keeps_explicit_system_gateway_on_hle_path() {
    const DEFAULT_GATEWAY: u32 = 0x00f0_1000;
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let table_entry = ppc_raw_trap_table_entry(0xA975, true);
    loaded
        .memory
        .add_region(table_entry, DEFAULT_GATEWAY.to_be_bytes().to_vec());
    loaded.attach_trap_default_gateway(0xA975, DEFAULT_GATEWAY);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 4);
    assert!(loaded.guest_calls().is_empty());
}

#[test]
fn tick_count_import_refuses_invalid_live_patch_atomically() {
    const INVALID_HANDLER: u32 = 0x00f0_2000;
    const ORIGINAL_R3: u32 = 0xcafe_babe;
    const ORIGINAL_R4: u32 = 0x1234_5678;
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let table_entry = ppc_raw_trap_table_entry(0xA975, true);
    loaded
        .memory
        .add_region(table_entry, INVALID_HANDLER.to_be_bytes().to_vec());
    loaded.cpu.gpr[3] = ORIGINAL_R3;
    loaded.cpu.gpr[4] = ORIGINAL_R4;
    loaded.cpu.gpr[5] = 0x8765_4321;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 0);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ORIGINAL_R3);
    assert_eq!(loaded.cpu.gpr[4], ORIGINAL_R4);
    assert_eq!(loaded.cpu.gpr[5], 0x8765_4321);
    assert!(loaded.guest_calls().is_empty());
}

#[test]
fn tick_count_import_schedules_live_classic_trap_patch_through_mixed_mode() {
    const CLASSIC_HANDLER: u32 = PPC_HEAP_BASE + 0x1000;
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let table_entry = ppc_raw_trap_table_entry(0xA975, true);
    loaded
        .memory
        .add_region(table_entry, CLASSIC_HANDLER.to_be_bytes().to_vec());
    loaded
        .memory
        .add_region(CLASSIC_HANDLER, 0x4e75u16.to_be_bytes().to_vec());

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let pending = loaded
        .guest_calls()
        .activate_m68k()
        .expect("live classic TickCount patch should hand off to the 68K adapter");
    assert_eq!(pending.entry, CLASSIC_HANDLER);
    assert_eq!(pending.final_sp, pending.initial_sp + 4);
}

#[test]
fn boolean_input_imports_call_live_native_trap_patches_before_hot_paths() {
    const DESCRIPTOR: u32 = PPC_HEAP_BASE + 0x1000;
    const TVECTOR: u32 = PPC_HEAP_BASE + 0x1080;
    const CALLBACK: u32 = PPC_HEAP_BASE + 0x2000;
    const CALLBACK_RTOC: u32 = PPC_HEAP_BASE + 0x3000;
    const RESULT: u32 = 0x1234_5678;

    for (symbol, trap_word) in [
        (b"StillDown".as_slice(), 0xA973),
        (b"Button".as_slice(), 0xA974),
        (b"WaitMouseUp".as_slice(), 0xA977),
    ] {
        for trace in [false, true] {
            let mut loaded =
                load_pef_application(&synthetic_pef_with_import(symbol)).unwrap();
            let table_entry = ppc_raw_trap_table_entry(trap_word, true);
            loaded
                .memory
                .add_region(table_entry, DESCRIPTOR.to_be_bytes().to_vec());
            loaded.memory.add_region(DESCRIPTOR, vec![0; 0x100]);
            loaded.memory.add_region(CALLBACK_RTOC, vec![0; 0x100]);
            loaded.memory.add_region(
                CALLBACK,
                [
                    d_form_u(15, 3, 0, (RESULT >> 16) as u16),
                    d_form_u(24, 3, 3, RESULT as u16),
                    BLR,
                ]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
            );
            loaded
                .memory
                .write_u16_be(DESCRIPTOR, PPC_MIXED_MODE_TRAP)
                .unwrap();
            loaded
                .memory
                .write_u8(DESCRIPTOR + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
                .unwrap();
            loaded.memory.write_u16_be(DESCRIPTOR + 10, 0).unwrap();
            ppc_write_routine_record(
                &mut loaded.memory,
                DESCRIPTOR + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE,
                0x10,
                PPC_ROUTINE_RECORD_POWERPC_ISA,
                PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
                TVECTOR,
            );
            loaded.memory.write_u32_be(TVECTOR, CALLBACK).unwrap();
            loaded
                .memory
                .write_u32_be(TVECTOR + 4, CALLBACK_RTOC)
                .unwrap();

            let probe = if trace {
                loaded.run_with_hle_import_trace(64)
            } else {
                loaded.run_with_hle_imports(64)
            };

            assert_eq!(probe.handled_import_count, 1, "{symbol:?} trace={trace}");
            assert_eq!(loaded.cpu.gpr[3], RESULT & 0xff, "{symbol:?} trace={trace}");
            assert!(loaded.guest_calls().is_empty(), "{symbol:?} trace={trace}");
        }
    }
}

#[test]
fn get_keys_import_calls_live_native_trap_patch_with_pointer_argument() {
    const DESCRIPTOR: u32 = PPC_HEAP_BASE + 0x1000;
    const TVECTOR: u32 = PPC_HEAP_BASE + 0x1080;
    const CALLBACK: u32 = PPC_HEAP_BASE + 0x2000;
    const CALLBACK_RTOC: u32 = PPC_HEAP_BASE + 0x3000;
    const OUTPUT: u32 = PPC_HEAP_BASE + 0x4000;
    const MARKER: u32 = 0xcafe_babe;

    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetKeys")).unwrap();
    let table_entry = ppc_raw_trap_table_entry(0xA976, true);
    loaded
        .memory
        .add_region(table_entry, DESCRIPTOR.to_be_bytes().to_vec());
    loaded.memory.add_region(DESCRIPTOR, vec![0; 0x100]);
    loaded.memory.add_region(CALLBACK_RTOC, vec![0; 0x100]);
    loaded.memory.add_region(OUTPUT, vec![0; 16]);
    loaded.memory.add_region(
        CALLBACK,
        [
            d_form_u(15, 4, 0, (MARKER >> 16) as u16),
            d_form_u(24, 4, 4, MARKER as u16),
            d_form_u(36, 4, 3, 0),
            BLR,
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    loaded
        .memory
        .write_u16_be(DESCRIPTOR, PPC_MIXED_MODE_TRAP)
        .unwrap();
    loaded
        .memory
        .write_u8(DESCRIPTOR + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    loaded.memory.write_u16_be(DESCRIPTOR + 10, 0).unwrap();
    ppc_write_routine_record(
        &mut loaded.memory,
        DESCRIPTOR + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE,
        0xC0,
        PPC_ROUTINE_RECORD_POWERPC_ISA,
        PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
        TVECTOR,
    );
    loaded.memory.write_u32_be(TVECTOR, CALLBACK).unwrap();
    loaded
        .memory
        .write_u32_be(TVECTOR + 4, CALLBACK_RTOC)
        .unwrap();
    loaded.cpu.gpr[3] = OUTPUT;

    let probe = loaded.run_with_hle_import_trace(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(MARKER));
    assert_eq!(loaded.cpu.gpr[3], OUTPUT);
    assert!(loaded.guest_calls().is_empty());
}

#[test]
fn get_keys_import_adapts_pointer_for_live_classic_trap_patch() {
    const CLASSIC_HANDLER: u32 = PPC_HEAP_BASE + 0x1000;
    const OUTPUT: u32 = PPC_HEAP_BASE + 0x2000;
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetKeys")).unwrap();
    let table_entry = ppc_raw_trap_table_entry(0xA976, true);
    loaded
        .memory
        .add_region(table_entry, CLASSIC_HANDLER.to_be_bytes().to_vec());
    loaded
        .memory
        .add_region(CLASSIC_HANDLER, 0x4e75u16.to_be_bytes().to_vec());
    loaded.memory.add_region(OUTPUT, vec![0; 16]);
    loaded.cpu.gpr[3] = OUTPUT;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    let pending = loaded
        .guest_calls()
        .activate_m68k()
        .expect("live classic GetKeys patch should hand off to the 68K adapter");
    assert_eq!(pending.entry, CLASSIC_HANDLER);
    assert_eq!(loaded.memory.read_u32_be(pending.initial_sp + 4), Some(OUTPUT));
    assert_eq!(pending.final_sp, pending.initial_sp + 8);
}

#[test]
fn hle_import_runner_traces_supported_import_context_when_requested() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[2] = 0x1234_5678;
    loaded.cpu.gpr[3] = 16;

    let probe = loaded.run_with_hle_import_trace(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(probe.import_trace.len(), 1);
    let entry = &probe.import_trace[0];
    assert_eq!(entry.import_index, 0);
    assert_eq!(entry.library_name, "InterfaceLib");
    assert_eq!(entry.symbol_name, "NewPtrClear");
    assert_eq!(entry.pc, PPC_IMPORT_TRAP_BASE);
    assert_eq!(entry.rtoc, 0x1234_5678);
    assert_eq!(entry.sp, loaded.stack_pointer);
    assert_eq!(
        entry.dispatcher_target,
        PpcImportDispatcherTarget::NewPtr { clear: true }
    );
}

#[test]
fn hle_import_runner_records_fetch_histogram_when_requested() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 16;

    let normal_probe = loaded.run_with_hle_imports(64);
    assert!(normal_probe.fetch_histogram.is_none());

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 16;
    let probe = loaded.run_with_hle_import_fetch_histogram(64);
    let histogram = probe
        .fetch_histogram
        .as_ref()
        .expect("histogram should be present for explicit fetch probe");

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(histogram.total(), 7);
    assert_eq!(histogram.primary_count(19), 2);
    assert_eq!(histogram.primary_count(31), 2);
    assert_eq!(histogram.secondary_count(19, 16), 1);
    assert_eq!(histogram.secondary_count(19, 528), 1);
    assert_eq!(histogram.secondary_count(31, 467), 2);

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 16;
    let probe = loaded.run_with_hle_import_trace_and_fetch_histogram(64);

    assert_eq!(probe.import_trace.len(), 1);
    assert!(probe.fetch_histogram.is_some());
}

#[test]
fn hle_import_runner_handles_trap_address_queries() {
    for (symbol, target, toolbox) in [
        (
            b"NGetTrapAddress".as_slice(),
            PpcImportDispatcherTarget::NGetTrapAddress,
            true,
        ),
        (
            b"GetToolTrapAddress".as_slice(),
            PpcImportDispatcherTarget::GetToolTrapAddress,
            true,
        ),
        (
            b"GetOSTrapAddress".as_slice(),
            PpcImportDispatcherTarget::GetOSTrapAddress,
            false,
        ),
    ] {
        let pef = synthetic_pef_with_import(symbol);
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = 0xFFFF_A800;
        loaded.cpu.gpr[4] = u32::from(toolbox);
        let expected: u32 = if toolbox { 0x1234_5678 } else { 0x8765_4321 };
        loaded.memory.add_region(
            ppc_raw_trap_table_entry(0xA800, toolbox),
            expected.to_be_bytes().to_vec(),
        );

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1, "{symbol:?}");
        assert_eq!(probe.unsupported_import_index, None, "{symbol:?}");
        assert_eq!(loaded.cpu.gpr[3], expected, "{symbol:?}");
    }
}

#[test]
fn hle_import_runner_handles_supported_trap_address_setters() {
    for (symbol, target, toolbox, general) in [
        (
            b"SetToolTrapAddress".as_slice(),
            PpcImportDispatcherTarget::SetToolTrapAddress,
            true,
            false,
        ),
        (
            b"SetOSTrapAddress".as_slice(),
            PpcImportDispatcherTarget::SetOSTrapAddress,
            false,
            false,
        ),
        (
            b"NSetTrapAddress".as_slice(),
            PpcImportDispatcherTarget::NSetTrapAddress,
            true,
            true,
        ),
        (
            b"NSetTrapAddress".as_slice(),
            PpcImportDispatcherTarget::NSetTrapAddress,
            false,
            true,
        ),
    ] {
        let pef = synthetic_pef_with_import(symbol);
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.imports[0].dispatcher_target = target;
        let trap_word = 0xFFFF_A823u32;
        let entry = ppc_raw_trap_table_entry(trap_word as u16, toolbox);
        loaded
            .memory
            .add_region(entry, 0x1234_5678u32.to_be_bytes().to_vec());
        loaded.cpu.gpr[3] = PPC_CODE_BASE;
        loaded.cpu.gpr[4] = trap_word;
        loaded.cpu.gpr[5] = u32::from(general && toolbox);

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1, "{symbol:?}");
        assert_eq!(probe.unsupported_import_index, None, "{symbol:?}");
        assert_eq!(loaded.memory.read_u32_be(entry), Some(PPC_CODE_BASE));
    }
}

#[test]
fn native_trap_apis_do_not_promote_a_writable_signature_to_a_protected_head() {
    let trap_word = 0xA823;
    let table_entry = ppc_raw_trap_table_entry(trap_word, true);
    let writable_head: u32 = 0x0030_0000;
    let apparent_successor: u32 = 0x0040_0000;
    let replacement: u32 = 0x0050_0000;
    let mut memory = PpcSectionMem::new();
    memory.add_region(table_entry, writable_head.to_be_bytes().to_vec());
    let mut head_bytes = Vec::from(
        crate::trap::manager::COME_FROM_PATCH_SIGNATURE.to_be_bytes(),
    );
    head_bytes.extend_from_slice(&apparent_successor.to_be_bytes());
    memory.add_region(writable_head, head_bytes);

    assert_eq!(
        ppc_logical_trap_address(&mut memory, trap_word, true),
        Some(writable_head)
    );
    assert!(ppc_set_logical_trap_address(
        &mut memory,
        trap_word,
        true,
        replacement,
    ));
    assert_eq!(memory.read_u32_be(table_entry), Some(replacement));
    assert_eq!(
        memory.read_u32_be(writable_head + 4),
        Some(apparent_successor)
    );
}

#[test]
fn native_trap_apis_share_permanent_come_from_topology() {
    let pef = synthetic_pef_with_import(b"NGetTrapAddress");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
    let mut dispatcher = TrapDispatcher::new();
    dispatcher
        .materialize_trap_tables(
            &mut bus,
            crate::trap::dispatch::TrapTableProfile::PowerPc604,
        )
        .expect("trap table construction requires writable cells and system storage");
    let trap_word = 0xA823u16;
    let table_entry = ppc_raw_trap_table_entry(trap_word, true);
    let head = bus.read_long(table_entry);
    let original = bus.read_long(head + 4);
    let second_head = bus.alloc_synthetic(10);
    bus.write_readonly_code_word(second_head, 0x6006);
    bus.write_readonly_code_word(second_head + 2, 0x4ef9);
    bus.write_readonly_code_word(second_head + 4, (original >> 16) as u16);
    bus.write_readonly_code_word(second_head + 6, original as u16);
    bus.write_readonly_code_word(second_head + 8, 0x60f8);
    bus.protect_readonly_code(second_head, 10);
    bus.write_readonly_code_word(head + 4, (second_head >> 16) as u16);
    bus.write_readonly_code_word(head + 6, second_head as u16);
    for (base, len) in [
        (
            crate::trap::dispatch::OS_TRAP_TABLE_BASE,
            u32::from(crate::trap::dispatch::OS_TRAP_TABLE_SLOTS) * 4,
        ),
        (
            crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE,
            u32::from(crate::trap::dispatch::TOOLBOX_TRAP_TABLE_SLOTS) * 4,
        ),
    ] {
        let region = bus.shared_ram_region(base, len).unwrap();
        // SAFETY: this focused fixture serializes the two adapters.
        unsafe { loaded.memory.add_shared_region(base, region) };
    }
    let (synthetic_base, synthetic) = bus.shared_synthetic_reservation().unwrap();
    // SAFETY: this focused fixture serializes the two adapters.
    unsafe {
        loaded.memory.add_shared_readonly_region(
            Some(GuestIsa::M68k),
            synthetic_base,
            synthetic,
        )
    };
    let ds_err = bus
        .shared_ram_region(crate::memory::globals::addr::DS_ERR_CODE, 2)
        .unwrap();
    // SAFETY: this focused fixture serializes the two adapters.
    unsafe {
        loaded
            .memory
            .add_shared_region(crate::memory::globals::addr::DS_ERR_CODE, ds_err)
    };

    assert_eq!(bus.read_long(head), 0x6006_4ef9);
    assert_eq!(bus.read_long(head + 4), second_head);
    assert_eq!(bus.read_long(second_head), 0x6006_4ef9);
    assert_eq!(bus.read_long(second_head + 4), original);

    // The 68K Trap Manager and the PPC import adapter must observe one
    // raw guest table, including the hidden successor of a permanent
    // come-from chain.  Mutate the chain through the 68K-side service and
    // read it through the PPC side before exercising the inverse route.
    assert_eq!(
        dispatcher.trap_table_address(&bus, trap_word),
        Some(original)
    );
    let m68k_patch = 0x0022_0000;
    dispatcher
        .install_trap_address(&mut bus, trap_word, m68k_patch)
        .expect("68K patch must install into the materialized table");
    assert_eq!(bus.read_long(table_entry), head);
    assert_eq!(bus.read_long(head + 4), second_head);
    assert_eq!(bus.read_long(second_head + 4), m68k_patch);

    loaded.cpu.gpr[3] = u32::from(trap_word);
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], m68k_patch);

    loaded.cpu.gpr[3] = 0xAA6E;
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    let unimplemented = loaded.cpu.gpr[3];

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0xAA57;
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_ne!(loaded.cpu.gpr[3], unimplemented);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = u32::from(trap_word);
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], m68k_patch);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NSetTrapAddress;
    loaded.cpu.gpr[3] = head;
    loaded.cpu.gpr[4] = u32::from(trap_word);
    loaded.cpu.gpr[5] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(bus.read_word(crate::memory::globals::addr::DS_ERR_CODE), 12);
    assert_eq!(bus.read_long(table_entry), head);
    assert_eq!(bus.read_long(head + 4), second_head);
    assert_eq!(bus.read_long(second_head + 4), m68k_patch);

    let replacement = PPC_CODE_BASE;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = replacement;
    loaded.cpu.gpr[4] = u32::from(trap_word);
    loaded.cpu.gpr[5] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(bus.read_long(table_entry), head);
    assert_eq!(bus.read_long(head + 4), second_head);
    assert_eq!(bus.read_long(second_head + 4), replacement);
    assert_eq!(loaded.memory.write_u32_be(second_head + 4, original), None);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NGetTrapAddress;
    loaded.cpu.gpr[3] = u32::from(trap_word);
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], replacement);

    loaded
        .memory
        .write_u32_be(table_entry, original)
        .expect("raw trap table remains guest-writable");
    assert_eq!(bus.read_long(table_entry), original);
    assert_eq!(
        dispatcher.trap_table_address(&bus, trap_word),
        Some(original),
        "68K getter must observe a PPC-side raw table overwrite"
    );
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = u32::from(trap_word);
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], original);
}

#[test]
fn hle_import_runner_handles_lm_get_current_a5() {
    let pef = synthetic_pef_with_import(b"LMGetCurrentA5");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_DATA_BASE);
}

#[test]
fn hle_import_runner_tracks_toolbox_startup_manager_state() {
    fn run_toolbox_import(
        loaded: &mut PpcLoadedApp,
        target: PpcImportDispatcherTarget,
        gpr3: u32,
        gpr4: u32,
    ) -> PpcHleRunProbe {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = gpr3;
        loaded.cpu.gpr[4] = gpr4;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert!(
            matches!(
                probe.result,
                PpcRunResult::Halted {
                    pc: PPC_HALT_PC,
                    ..
                }
            ),
            "{:?}",
            probe.result
        );
        assert_eq!(loaded.cpu.gpr[3], gpr3);
        assert_eq!(loaded.cpu.gpr[4], gpr4);
        probe
    }

    let pef = synthetic_pef_with_import(b"InitGraf");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(loaded.toolbox_startup, PpcToolboxStartupState::default());

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitGraf,
        PPC_DATA_BASE + 0x40,
        0x1234_5678,
    );
    assert_eq!(loaded.toolbox_startup.init_graf_count, 1);
    assert_eq!(
        loaded.toolbox_startup.init_graf_global_ptr,
        PPC_DATA_BASE + 0x40
    );

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitFonts,
        0xfeed_face,
        0x1111_2222,
    );
    assert!(loaded.toolbox_startup.fonts_initialized);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitWindows,
        0xabcd_1234,
        0x2222_3333,
    );
    assert!(loaded.toolbox_startup.windows_initialized);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitMenus,
        0x8765_4321,
        0x3333_4444,
    );
    assert!(loaded.toolbox_startup.menus_initialized);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::TEInit,
        0x1357_2468,
        0x4444_5555,
    );
    assert!(loaded.toolbox_startup.text_edit_initialized);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitDialogs,
        0x2468_1357,
        0x5555_6666,
    );
    assert!(loaded.toolbox_startup.dialogs_initialized);
    assert_eq!(loaded.toolbox_startup.dialog_resume_proc, 0x2468_1357);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::FlushEvents,
        0x0001_ffff,
        0x0002_0002,
    );
    assert_eq!(loaded.toolbox_startup.flush_events_count, 1);
    assert_eq!(loaded.toolbox_startup.last_flush_event_mask, 0xffff);
    assert_eq!(loaded.toolbox_startup.last_flush_stop_mask, 0x0002);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::SetEventMask,
        0x0000_ffdf,
        0,
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(crate::memory::globals::addr::SYS_EVT_MASK),
        Some(0xffdf)
    );

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::DisposeDialog,
        PPC_HEAP_BASE + 0x80,
        0x6666_7777,
    );
    assert_eq!(loaded.toolbox_startup.dispose_dialog_count, 1);
    assert_eq!(
        loaded.toolbox_startup.last_disposed_dialog,
        PPC_HEAP_BASE + 0x80
    );
}

#[test]
fn init_graf_initializes_application_quickdraw_globals() {
    let pef = synthetic_pef_with_import(b"InitGraf");
    let mut loaded = load_pef_application(&pef).unwrap();
    let global_ptr = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(global_ptr - 126, vec![0; 130]);
    loaded.cpu.gpr[3] = global_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(global_ptr - 126), Some(1));
    assert_eq!(
        loaded.memory.read_u32_be(global_ptr - 122),
        Some(PPC_MAIN_SCREEN_BASE)
    );
    assert_eq!(
        loaded.memory.read_u16_be(global_ptr - 118),
        Some(ppc_main_screen_row_bytes() as u16)
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, global_ptr - 116),
        Some((
            0,
            0,
            ppc_main_screen_height() as i16,
            ppc_main_screen_width() as i16,
        ))
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, global_ptr - 24, 8),
        Some(vec![0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55])
    );
    assert_eq!(loaded.memory.read_u32_be(global_ptr), Some(PPC_MAIN_GWORLD));
}


mod palette_manager;
mod gworld;
mod blit;

mod file_manager;
mod event_manager;

mod text_edit;
mod scrap_manager;
mod list_manager;

#[test]
fn hle_import_runner_gets_and_sets_gray_region_low_memory_handle() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMSetGrayRgn"),
        PpcImportDispatcherTarget::LMSetGrayRgn
    );

    let pef = synthetic_pef_with_import(b"LMSetGrayRgn");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.memory.read_u32_be(PPC_GRAY_RGN_ADDR),
        Some(PPC_GRAY_RGN_HANDLE)
    );

    loaded.cpu.gpr[3] = 0x0300_1234;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_GRAY_RGN_ADDR),
        Some(0x0300_1234)
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetGrayRgn;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x0300_1234);
}


#[test]
fn hle_import_runner_get_adb_info_exposes_standard_devices() {
    for (address, expected) in [(2, [2, 2]), (3, [1, 3])] {
        let pef = synthetic_pef_with_import(b"GetADBInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let info_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(info_ptr, vec![0xaa; 12]);
        loaded.cpu.gpr[3] = info_ptr;
        loaded.cpu.gpr[4] = address;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.memory.read_u8(info_ptr), Some(expected[0]));
        assert_eq!(loaded.memory.read_u8(info_ptr + 1), Some(expected[1]));
        assert_eq!(loaded.memory.read_u32_be(info_ptr + 2), Some(0));
        assert_eq!(loaded.memory.read_u32_be(info_ptr + 6), Some(0));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 10), Some(0xaaaa));
    }

    let pef = synthetic_pef_with_import(b"GetADBInfo");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1));
}


#[test]
fn hle_import_runner_handles_keyboard_time_and_text_width_utilities() {
    assert_eq!(
        ppc_virtual_microseconds(100, 100, 25, 50),
        100 * PPC_MICROSECONDS_PER_TICK + 3 * PPC_MICROSECONDS_PER_TICK / 4
    );

    let pef = synthetic_pef_with_import(b"GetKeys");
    let mut loaded = load_pef_application(&pef).unwrap();
    let key_map_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(key_map_ptr, vec![0xaa; PPC_KEY_MAP_SIZE as usize]);
    loaded.cpu.gpr[3] = key_map_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    for offset in 0..PPC_KEY_MAP_SIZE {
        assert_eq!(loaded.memory.read_u8(key_map_ptr + offset), Some(0));
    }

    let pef = synthetic_pef_with_import(b"GetKeys");
    let mut loaded = load_pef_application(&pef).unwrap();
    let key_map_ptr = PPC_DATA_BASE + 0x1000;
    let mut input = PpcInputSnapshot::default();
    input.key_map[(PPC_KEY_LEFT / 8) as usize] |= 1u8 << (PPC_KEY_LEFT % 8);
    loaded.set_input_snapshot(input);
    loaded
        .memory
        .add_region(key_map_ptr, vec![0; PPC_KEY_MAP_SIZE as usize]);
    loaded.cpu.gpr[3] = key_map_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_ne!(
        loaded
            .memory
            .read_u8(key_map_ptr + u32::from(PPC_KEY_LEFT / 8))
            .unwrap()
            & (1u8 << (PPC_KEY_LEFT % 8)),
        0
    );

    let pef = synthetic_pef_with_import(b"GetDateTime");
    let mut loaded = load_pef_application(&pef).unwrap();
    let secs_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(secs_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = secs_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(secs_ptr),
        Some(PPC_FIXED_MAC_TIME)
    );

    let pef = synthetic_pef_with_import(b"LMGetTime");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_FIXED_MAC_TIME);

    let pef = synthetic_pef_with_import(b"Delay");
    let mut loaded = load_pef_application(&pef).unwrap();
    let final_ticks_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(final_ticks_ptr, vec![0xaa; 4]);
    loaded.set_clock_cycle_timing(1_000_000, 0);
    loaded.set_tick_count(42);
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = final_ticks_ptr;

    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.result, PpcRunResult::CycleLimit { cycles: 64 });
    assert_eq!(loaded.cpu.pc, loaded.import_trap_base);
    assert_eq!(
        loaded.memory.read_u32_be(final_ticks_ptr),
        Some(0xaaaa_aaaa)
    );

    loaded.set_tick_count(43);
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.result, PpcRunResult::CycleLimit { cycles: 64 });

    loaded.set_tick_count(44);
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(final_ticks_ptr), Some(44));
    assert_eq!(loaded.toolbox_startup.delay_deadline, None);

    for (seconds, expected) in [
        (0, [1904, 1, 1, 0, 0, 0, 6]),
        (
            59 * 86_400 + 23 * 3_600 + 59 * 60 + 58,
            [1904, 2, 29, 23, 59, 58, 2],
        ),
        (366 * 86_400, [1905, 1, 1, 0, 0, 0, 1]),
    ] {
        let pef = synthetic_pef_with_import(b"SecondsToDate");
        let mut loaded = load_pef_application(&pef).unwrap();
        let date_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(date_ptr, vec![0xaa; 14]);
        loaded.cpu.gpr[3] = seconds;
        loaded.cpu.gpr[4] = date_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        for (index, field) in expected.into_iter().enumerate() {
            assert_eq!(
                loaded.memory.read_u16_be(date_ptr + index as u32 * 2),
                Some(field),
                "field {index} for {seconds} seconds"
            );
        }
    }

    let pef = synthetic_pef_with_import(b"Microseconds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let microseconds_ptr = PPC_DATA_BASE + 0x1000;
    let tick_count = 100u32;
    let cycles_per_tick = 64;
    let expected_usecs = ppc_virtual_microseconds(tick_count, cycles_per_tick, 0, 4);
    loaded.memory.add_region(microseconds_ptr, vec![0xaa; 8]);
    loaded.cpu.gpr[3] = microseconds_ptr;
    loaded.set_tick_count(tick_count);
    loaded.set_clock_cycle_timing(cycles_per_tick, 0);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(microseconds_ptr),
        Some((expected_usecs >> 32) as u32)
    );
    assert_eq!(
        loaded.memory.read_u32_be(microseconds_ptr + 4),
        Some(expected_usecs as u32)
    );

    let pef = synthetic_pef_with_import(b"TickCount");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_tick_count(0x1234_5678);
    loaded.set_clock_cycle_timing(64, 0);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);

    let pef = synthetic_pef_with_import(b"SysEnvirons");
    let mut loaded = load_pef_application(&pef).unwrap();
    let sys_env_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(sys_env_ptr, vec![0xaa; 16]);
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = sys_env_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(sys_env_ptr), Some(2));
    assert_eq!(
        loaded.memory.read_u16_be(sys_env_ptr + 2),
        Some(REFERENCE_MACHINE_PROFILE.gestalt_machine_type)
    );
    assert_eq!(
        loaded.memory.read_u16_be(sys_env_ptr + 4),
        Some(REFERENCE_MACHINE_PROFILE.system_version_bcd)
    );
    assert_eq!(
        loaded.memory.read_u16_be(sys_env_ptr + 6),
        Some(REFERENCE_MACHINE_PROFILE.gestalt_processor_type as u16)
    );
    assert_eq!(
        loaded.memory.read_u8(sys_env_ptr + 8),
        Some(u8::from(REFERENCE_MACHINE_PROFILE.has_fpu()))
    );
    assert_eq!(loaded.memory.read_u8(sys_env_ptr + 9), Some(1));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[4] = 0x30;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));

    let pef = synthetic_pef_with_import(b"TextWidth");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_DATA_BASE;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 7;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 42);
}

#[test]
fn hle_import_runner_handles_quickdraw_color_and_control_defaults() {
    let pef = synthetic_pef_with_import(b"GetForeColor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let color_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(color_ptr, vec![0xaa; 6]);
    loaded.cpu.gpr[3] = color_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], color_ptr);
    assert_eq!(
        ppc_read_rgb_color(&mut loaded.memory, color_ptr),
        Some(PpcRgbColor {
            red: 0,
            green: 0,
            blue: 0,
        })
    );

    let pef = synthetic_pef_with_import(b"SetControlValue");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x100;
    loaded.cpu.gpr[4] = 7;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE + 0x100);
    assert_eq!(loaded.cpu.gpr[4], 7);
}


#[test]
fn ppc_vfs_seed_registers_fond_associated_application_font() {
    let pef = synthetic_pef_with_import(b"WaitNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let family_id = 31001i16;
    let font_resource_id = 2558i16;
    let point_size = 17i16;

    let mut fond = vec![0u8; 60];
    fond[2..4].copy_from_slice(&(family_id as u16).to_be_bytes());
    fond[52..54].copy_from_slice(&0u16.to_be_bytes());
    fond[54..56].copy_from_slice(&(point_size as u16).to_be_bytes());
    fond[56..58].copy_from_slice(&0u16.to_be_bytes());
    fond[58..60].copy_from_slice(&(font_resource_id as u16).to_be_bytes());

    let mut nfnt = vec![0u8; 38];
    nfnt[2..4].copy_from_slice(&32u16.to_be_bytes());
    nfnt[4..6].copy_from_slice(&32u16.to_be_bytes());
    nfnt[6..8].copy_from_slice(&1u16.to_be_bytes());
    nfnt[14..16].copy_from_slice(&1u16.to_be_bytes());
    nfnt[16..18].copy_from_slice(&9u16.to_be_bytes());
    nfnt[18..20].copy_from_slice(&1u16.to_be_bytes());
    nfnt[24..26].copy_from_slice(&1u16.to_be_bytes());
    nfnt[26] = 0xc0;
    nfnt[30..32].copy_from_slice(&1u16.to_be_bytes());
    nfnt[32..34].copy_from_slice(&2u16.to_be_bytes());
    nfnt[34..36].copy_from_slice(&1u16.to_be_bytes());
    nfnt[36..38].copy_from_slice(&1u16.to_be_bytes());

    let record = |res_type: [u8; 4], res_id: i16, data: Vec<u8>| PpcVfsResourceRecord {
        ref_num: 0,
        path: "Test App".to_string(),
        res_type: u32::from_be_bytes(res_type),
        res_id,
        name: Vec::new(),
        data,
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    };
    loaded.seed_vfs_files_and_resources(
        Vec::new(),
        Vec::new(),
        vec![
            record(*b"FOND", family_id, fond),
            record(*b"NFNT", font_resource_id, nfnt),
        ],
    );

    let face = crate::quickdraw::fonts::get_font_face(family_id, point_size)
        .expect("PPC application font should be registered");
    assert_eq!(face.font_id, family_id);
    assert_eq!(face.size, point_size);
    assert_eq!(face.metrics.ascent, 1);
}
mod standard_c_library;
mod toolbox_utilities;

mod thread_manager;

mod process_manager;


mod font_manager;

mod gestalt;

mod display_depth;

mod desk_manager;

mod device_manager;

#[path = "native_exceptions.rs"]
mod native_exceptions;

mod collection_manager;

mod draw_sprocket;

mod input_sprocket;
mod qd3d;
mod menu_manager;
pub(crate) use menu_manager::*;
mod window_manager;
mod mixed_mode;
pub(crate) use mixed_mode::*;
mod quicktime;
mod memory_manager;
mod resource_manager;
mod process_services;
mod code_fragment_manager;
pub(crate) use code_fragment_manager::synthetic_pef_with_enumerable_exports;
mod quickdraw;



mod sound_manager;
fn establish_loaded_reservation(loaded: &mut PpcLoadedApp, address: u32) {
    const LWARX_R12_R4_R5: u32 =
        (31 << 26) | (12 << 21) | (4 << 16) | (5 << 11) | (20 << 1);
    let preserved = (
        loaded.cpu.pc,
        loaded.cpu.gpr[4],
        loaded.cpu.gpr[5],
        loaded.cpu.gpr[12],
    );
    loaded.cpu.gpr[4] = address;
    loaded.cpu.gpr[5] = 0;
    assert_eq!(
        loaded.cpu.step(&mut loaded.memory, LWARX_R12_R4_R5),
        ppc::PpcStepResult::Stepped
    );
    assert_eq!(loaded.cpu.reservation_address(), Some(address));
    (
        loaded.cpu.pc,
        loaded.cpu.gpr[4],
        loaded.cpu.gpr[5],
        loaded.cpu.gpr[12],
    ) = preserved;
}

mod interrupt_callbacks;

mod dialog_parameters;
mod dialog_manager;
mod control_manager;

mod loader_configuration;

pub(crate) fn synthetic_pef() -> Vec<u8> {
    synthetic_pef_with_import(b"TestImport")
}

fn synthetic_pef_with_initializer() -> Vec<u8> {
    let mut pef = synthetic_pef();
    // The synthetic loader section begins at $80. Reuse its main TVector
    // as a valid initializer TVector so loading exercises CFM's startup
    // fragment and InitBlock allocations without executing the code.
    write_i32(&mut pef, 0x80 + 8, 1);
    write_u32(&mut pef, 0x80 + 12, 0);
    pef
}

fn assert_ppc_bytes_equal(memory: &mut PpcSectionMem, start: u32, len: u32, expected: u8) {
    for offset in 0..len {
        assert_eq!(memory.read_u8(start + offset), Some(expected));
    }
}

mod standard_c_library_integration;

pub(crate) fn synthetic_pef_with_import(symbol_name: &[u8]) -> Vec<u8> {
    synthetic_pef_with_library_import(b"InterfaceLib", symbol_name)
}

fn synthetic_pef_with_import_class(symbol_name: &[u8], symbol_class: u8) -> Vec<u8> {
    synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"InterfaceLib",
        symbol_name,
        symbol_class,
        &[sm_index_reloc(0x30, 0)],
    ))
}

fn write_test_dsp_context_attributes(
    memory: &mut PpcSectionMem,
    attributes: u32,
    context_attributes: PpcDspContextAttributes,
) {
    memory
        .write_u32_be(attributes, context_attributes.frequency)
        .unwrap();
    memory
        .write_u32_be(attributes + 4, context_attributes.width)
        .unwrap();
    memory
        .write_u32_be(attributes + 8, context_attributes.height)
        .unwrap();
    memory
        .write_u32_be(attributes + 28, context_attributes.context_options)
        .unwrap();
    memory
        .write_u32_be(
            attributes + 32,
            context_attributes.back_buffer_best_depth_mask,
        )
        .unwrap();
    memory
        .write_u32_be(attributes + 36, context_attributes.display_best_depth_mask)
        .unwrap();
    memory
        .write_u32_be(attributes + 40, context_attributes.back_buffer_depth)
        .unwrap();
    memory
        .write_u32_be(attributes + 44, context_attributes.display_depth)
        .unwrap();
    memory
        .write_u32_be(attributes + 48, context_attributes.page_count)
        .unwrap();
}

#[test]
fn driver_services_uptime_uses_deterministic_virtual_clock() {
    let pef = synthetic_pef_with_library_import(b"DriverServicesLib", b"UpTime");
    let mut loaded = load_pef_application(&pef).unwrap();
    let time_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(time_ptr, vec![0xaa; 8]);
    loaded.cpu.gpr[3] = time_ptr;
    loaded.set_tick_count(100);
    loaded.set_clock_cycle_timing(64, 0);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u64_be(time_ptr),
        Some(ppc_virtual_microseconds(100, 64, 0, 4))
    );
}

#[test]
fn driver_services_absolute_time_converts_to_nanoseconds() {
    let pef = synthetic_pef_with_library_import(b"DriverServicesLib", b"AbsoluteToNanoseconds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0xaa; 8]);
    loaded.cpu.gpr[3] = output;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 2;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u64_be(output),
        Some(((1u64 << 32) | 2) * 1_000)
    );
}

fn synthetic_pef_with_library_import(library_name: &[u8], symbol_name: &[u8]) -> Vec<u8> {
    synthetic_pef_with_loader(synthetic_loader(library_name, symbol_name))
}

fn synthetic_pef_with_reloc_chunks(
    library_name: &[u8],
    symbol_name: &[u8],
    chunks: &[u16],
) -> Vec<u8> {
    synthetic_pef_with_loader(synthetic_loader_with_chunks(
        library_name,
        symbol_name,
        chunks,
    ))
}

fn synthetic_pef_with_loader(loader: Vec<u8>) -> Vec<u8> {
    synthetic_pef_with_loader_and_data(loader, &[0; 8])
}

fn synthetic_pef_with_loader_and_data(loader: Vec<u8>, data: &[u8]) -> Vec<u8> {
    let code = synthetic_code();
    let loader_offset = 0x80usize;
    let code_offset = align_test_offset((loader_offset + loader.len()).max(0x100), 0x10);
    let data_offset = code_offset + code.len();
    let total_len = data_offset + data.len();
    let mut bytes = vec![0u8; total_len.max(loader_offset + loader.len())];

    bytes[0..4].copy_from_slice(b"Joy!");
    bytes[4..8].copy_from_slice(b"peff");
    bytes[8..12].copy_from_slice(b"pwpc");
    write_u32(&mut bytes, 12, 1);
    write_u16(&mut bytes, 32, 3);
    write_u16(&mut bytes, 34, 2);

    write_section(
        &mut bytes,
        0,
        SectionSpec {
            total_size: code.len() as u32,
            unpacked_size: code.len() as u32,
            packed_size: code.len() as u32,
            container_offset: code_offset as u32,
            section_kind: SECTION_KIND_CODE,
        },
    );
    write_section(
        &mut bytes,
        1,
        SectionSpec {
            total_size: data.len() as u32,
            unpacked_size: data.len() as u32,
            packed_size: data.len() as u32,
            container_offset: data_offset as u32,
            section_kind: SECTION_KIND_UNPACKED_DATA,
        },
    );
    write_section(
        &mut bytes,
        2,
        SectionSpec {
            total_size: 0,
            unpacked_size: 0,
            packed_size: loader.len() as u32,
            container_offset: loader_offset as u32,
            section_kind: super::super::pef::SECTION_KIND_LOADER,
        },
    );

    bytes[loader_offset..loader_offset + loader.len()].copy_from_slice(&loader);
    bytes[code_offset..code_offset + code.len()].copy_from_slice(&code);
    bytes[data_offset..data_offset + data.len()].copy_from_slice(&data);
    bytes
}

fn align_test_offset(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn synthetic_code() -> Vec<u8> {
    let mut code = Vec::new();
    let hi = (PPC_IMPORT_TRAP_BASE >> 16) as u16;
    let lo = (PPC_IMPORT_TRAP_BASE & 0xffff) as u16;
    for word in [
        d_form_u(15, 12, 0, hi),       // lis r12, trap@ha
        d_form_u(24, 12, 12, lo),      // ori r12, r12, trap@l
        xfx_form(31, 12, 9, 467),      // mtctr r12
        xl_form(19, 20, 0, 528, true), // bctrl
        d_form_u(14, 0, 0, 0),         // li r0, 0
        xfx_form(31, 0, 8, 467),       // mtlr r0
        BLR,
    ] {
        code.extend_from_slice(&word.to_be_bytes());
    }
    code
}

fn synthetic_loader(library_name: &[u8], symbol_name: &[u8]) -> Vec<u8> {
    let chunks = [run_reloc(0x23, 1)];
    synthetic_loader_with_chunks(library_name, symbol_name, &chunks)
}

fn synthetic_loader_with_chunks(
    library_name: &[u8],
    symbol_name: &[u8],
    chunks: &[u16],
) -> Vec<u8> {
    synthetic_loader_with_symbol_class(library_name, symbol_name, 0x02, chunks)
}

fn synthetic_loader_with_symbol_class(
    library_name: &[u8],
    symbol_name: &[u8],
    symbol_class: u8,
    chunks: &[u16],
) -> Vec<u8> {
    let mut strings = Vec::new();
    let library_name = push_c_string(&mut strings, library_name);
    let symbol_name = push_c_string(&mut strings, symbol_name);
    let reloc_header_offset = 56 + 24 + 4;
    let reloc_instr_offset = reloc_header_offset + 12;
    let strings_offset = reloc_instr_offset + chunks.len() * 2;
    let mut bytes = vec![0u8; strings_offset + strings.len()];

    write_i32(&mut bytes, 0, 1);
    write_u32(&mut bytes, 4, 0);
    write_i32(&mut bytes, 8, -1);
    write_i32(&mut bytes, 16, -1);
    write_u32(&mut bytes, 24, 1);
    write_u32(&mut bytes, 28, 1);
    write_u32(&mut bytes, 32, 1);
    write_u32(&mut bytes, 36, reloc_instr_offset as u32);
    write_u32(&mut bytes, 40, strings_offset as u32);

    write_u32(&mut bytes, 56, library_name);
    write_u32(&mut bytes, 56 + 12, 1);
    write_symbol(&mut bytes, 56 + 24, symbol_class, symbol_name);

    write_u16(&mut bytes, reloc_header_offset, 1);
    write_u32(&mut bytes, reloc_header_offset + 4, chunks.len() as u32);
    write_u32(&mut bytes, reloc_header_offset + 8, 0);
    for (index, chunk) in chunks.iter().enumerate() {
        write_u16(&mut bytes, reloc_instr_offset + index * 2, *chunk);
    }

    bytes[strings_offset..].copy_from_slice(&strings);
    bytes
}

fn synthetic_loader_with_repeated_imports(import_count: u32) -> Vec<u8> {
    let mut strings = Vec::new();
    let library_name = push_c_string(&mut strings, b"InterfaceLib");
    let symbol_name = push_c_string(&mut strings, b"TickCount");
    let symbol_count = usize::try_from(import_count).unwrap();
    let strings_offset = 56 + 24 + symbol_count * 4;
    let mut bytes = vec![0u8; strings_offset + strings.len()];

    write_i32(&mut bytes, 0, 1);
    write_i32(&mut bytes, 8, -1);
    write_i32(&mut bytes, 16, -1);
    write_u32(&mut bytes, 24, 1);
    write_u32(&mut bytes, 28, import_count);
    write_u32(&mut bytes, 36, strings_offset as u32);
    write_u32(&mut bytes, 40, strings_offset as u32);
    write_u32(&mut bytes, 56, library_name);
    write_u32(&mut bytes, 56 + 12, import_count);
    for index in 0..symbol_count {
        write_symbol(&mut bytes, 56 + 24 + index * 4, 2, symbol_name);
    }
    bytes[strings_offset..].copy_from_slice(&strings);
    bytes
}

fn synthetic_loader_with_overlapping_library_ranges() -> Vec<u8> {
    let mut strings = Vec::new();
    let first_library = push_c_string(&mut strings, b"InterfaceLib");
    let second_library = push_c_string(&mut strings, b"StdCLib");
    let symbol = push_c_string(&mut strings, b"errno");
    let reloc_header_offset = 56 + 2 * 24 + 4;
    let reloc_instr_offset = reloc_header_offset + 12;
    let strings_offset = reloc_instr_offset + 2;
    let mut bytes = vec![0u8; strings_offset + strings.len()];

    write_i32(&mut bytes, 0, 1);
    write_i32(&mut bytes, 8, -1);
    write_i32(&mut bytes, 16, -1);
    write_u32(&mut bytes, 24, 2);
    write_u32(&mut bytes, 28, 1);
    write_u32(&mut bytes, 32, 1);
    write_u32(&mut bytes, 36, reloc_instr_offset as u32);
    write_u32(&mut bytes, 40, strings_offset as u32);

    write_u32(&mut bytes, 56, first_library);
    write_u32(&mut bytes, 56 + 12, 1);
    write_u32(&mut bytes, 56 + 24, second_library);
    write_u32(&mut bytes, 56 + 24 + 12, 1);
    write_symbol(&mut bytes, 56 + 2 * 24, 2, symbol);

    write_u16(&mut bytes, reloc_header_offset, 1);
    write_u32(&mut bytes, reloc_header_offset + 4, 1);
    write_u32(&mut bytes, reloc_header_offset + 8, 0);
    write_u16(&mut bytes, reloc_instr_offset, sm_index_reloc(0x30, 0));
    bytes[strings_offset..].copy_from_slice(&strings);
    bytes
}

fn synthetic_pef_with_exports() -> Vec<u8> {
    let names = [b"tvector".as_slice(), b"absolute", b"reexport"];
    let mut strings = Vec::new();
    let mut name_offsets = Vec::new();
    for name in names {
        name_offsets.push(strings.len() as u32);
        strings.extend_from_slice(name);
        strings.push(0);
    }

    let strings_offset = 56usize;
    let export_hash_offset = strings_offset + strings.len();
    let key_table_offset = export_hash_offset + 4;
    let symbol_table_offset = key_table_offset + names.len() * 4;
    let loader_len = symbol_table_offset + names.len() * 10;
    let mut loader = vec![0; loader_len];
    write_i32(&mut loader, 0, -1);
    write_i32(&mut loader, 8, -1);
    write_i32(&mut loader, 16, -1);
    write_u32(&mut loader, 40, strings_offset as u32);
    write_u32(&mut loader, 44, export_hash_offset as u32);
    write_u32(&mut loader, 48, 0);
    write_u32(&mut loader, 52, names.len() as u32);
    loader[strings_offset..export_hash_offset].copy_from_slice(&strings);

    // All three entries occupy one valid hash chain. The resolver uses
    // the flattened export table, but the parser still validates chains.
    write_u32(&mut loader, export_hash_offset, 3 << 18);
    for (index, name) in names.iter().enumerate() {
        write_u32(
            &mut loader,
            key_table_offset + index * 4,
            (name.len() as u32) << 16,
        );
    }
    let records = [
        (2u8, name_offsets[0], 0u32, 1i16),
        (1u8, name_offsets[1], 0x1234_5678, -2i16),
        (2u8, name_offsets[2], 0u32, -3i16),
    ];
    for (index, (class, name_offset, value, section_index)) in records.into_iter().enumerate() {
        let base = symbol_table_offset + index * 10;
        write_u32(&mut loader, base, (u32::from(class) << 24) | name_offset);
        write_u32(&mut loader, base + 4, value);
        write_u16(&mut loader, base + 8, section_index as u16);
    }
    synthetic_pef_with_loader_and_data(loader, &[0; 8])
}

fn write_ppc_pstring(memory: &mut PpcSectionMem, addr: u32, bytes: &[u8]) {
    assert!(bytes.len() <= 255);
    memory.write_u8(addr, bytes.len() as u8).unwrap();
    for (offset, byte) in bytes.iter().enumerate() {
        memory.write_u8(addr + 1 + offset as u32, *byte).unwrap();
    }
}

fn compressed_resource_bytes(decompressed_len: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\xA8\x9Fer");
    out.extend_from_slice(&0x12u16.to_be_bytes());
    out.extend_from_slice(&0x0801u16.to_be_bytes());
    out.extend_from_slice(&decompressed_len.to_be_bytes());
    out.extend_from_slice(&[0x80, 0x03]);
    out.extend_from_slice(&0i16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(body);
    out
}

fn noncanonical_single_resource_fork_bytes(
    res_type: [u8; 4],
    res_id: i16,
    data: &[u8],
    attrs: u8,
    map_attrs: u16,
) -> Vec<u8> {
    let data_offset = 16u32;
    let data_length = (4 + data.len()) as u32;
    let map_offset = data_offset + data_length;
    let type_list_offset = 30u16;
    let ref_list_offset = 10u16;
    let name_list_offset = 40u16;
    let map_length = 52u32;

    let mut bytes = vec![0u8; (map_offset + map_length) as usize];
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&data_offset.to_be_bytes());
    header[4..8].copy_from_slice(&map_offset.to_be_bytes());
    header[8..12].copy_from_slice(&data_length.to_be_bytes());
    header[12..16].copy_from_slice(&map_length.to_be_bytes());
    bytes[0..16].copy_from_slice(&header);

    let data_start = data_offset as usize;
    bytes[data_start..data_start + 4].copy_from_slice(&(data.len() as u32).to_be_bytes());
    bytes[data_start + 4..data_start + 4 + data.len()].copy_from_slice(data);

    let map_start = map_offset as usize;
    bytes[map_start..map_start + 16].copy_from_slice(&header);
    bytes[map_start + 22..map_start + 24].copy_from_slice(&map_attrs.to_be_bytes());
    bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
    bytes[map_start + 26..map_start + 28].copy_from_slice(&name_list_offset.to_be_bytes());

    let type_list_start = map_start + type_list_offset as usize;
    bytes[type_list_start..type_list_start + 2].copy_from_slice(&0u16.to_be_bytes());
    bytes[type_list_start + 2..type_list_start + 6].copy_from_slice(&res_type);
    bytes[type_list_start + 6..type_list_start + 8].copy_from_slice(&0u16.to_be_bytes());
    bytes[type_list_start + 8..type_list_start + 10]
        .copy_from_slice(&ref_list_offset.to_be_bytes());

    let ref_list_start = type_list_start + ref_list_offset as usize;
    bytes[ref_list_start..ref_list_start + 2].copy_from_slice(&(res_id as u16).to_be_bytes());
    bytes[ref_list_start + 2..ref_list_start + 4].copy_from_slice(&0xffffu16.to_be_bytes());
    bytes[ref_list_start + 4] = attrs;
    bytes[ref_list_start + 5..ref_list_start + 8].copy_from_slice(&0u32.to_be_bytes()[1..4]);
    bytes
}

fn test_sound_file_playback(channel: u32) -> PpcSoundFilePlaybackRecord {
    PpcSoundFilePlaybackRecord {
        channel,
        ref_num: PPC_FIRST_FILE_REF_NUM,
        resource_id: 0,
        buffer_size: 0,
        buffer: 0,
        selection: 0,
        completion: 0,
        completion_command: None,
        async_play: false,
        aiff: None,
        decoded_aiff: None,
    }
}

fn write_ppc_fsspec(
    memory: &mut PpcSectionMem,
    addr: u32,
    vref: i16,
    dir_id: u32,
    name: &[u8],
) {
    memory.write_u16_be(addr, vref as u16).unwrap();
    memory.write_u32_be(addr + 2, dir_id).unwrap();
    write_ppc_pstring(memory, addr + 6, name);
}

pub(crate) fn test_v1_one_bit_packbits_pict() -> Vec<u8> {
    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    let mut bytes = Vec::new();
    push_u16(&mut bytes, 0); // patched picSize
    for value in [0i16, 0, 1, 8] {
        push_u16(&mut bytes, value as u16);
    }
    bytes.push(0x11); // versionOp
    bytes.push(0x01); // version 1
    bytes.push(0x98); // PackBitsRect
    push_u16(&mut bytes, 1); // rowBytes < 8: unpacked bitmap data
    for value in [0i16, 0, 1, 8] {
        push_u16(&mut bytes, value as u16);
    }
    for _ in 0..2 {
        for value in [0i16, 0, 1, 8] {
            push_u16(&mut bytes, value as u16);
        }
    }
    push_u16(&mut bytes, 0); // srcCopy
    bytes.push(0b1010_0000);
    bytes.push(0xff); // EndOfPicture
    let size = u16::try_from(bytes.len()).unwrap();
    bytes[0..2].copy_from_slice(&size.to_be_bytes());
    bytes
}

fn test_one_bit_cicon() -> Vec<u8> {
    let mut data = vec![0u8; 101];
    data[4..6].copy_from_slice(&0x8001u16.to_be_bytes());
    data[10..12].copy_from_slice(&1u16.to_be_bytes());
    data[12..14].copy_from_slice(&8u16.to_be_bytes());
    data[32..34].copy_from_slice(&1u16.to_be_bytes());
    data[34..36].copy_from_slice(&1u16.to_be_bytes());
    data[36..38].copy_from_slice(&1u16.to_be_bytes());

    data[54..56].copy_from_slice(&1u16.to_be_bytes());
    data[60..62].copy_from_slice(&1u16.to_be_bytes());
    data[62..64].copy_from_slice(&8u16.to_be_bytes());
    data[68..70].copy_from_slice(&1u16.to_be_bytes());
    data[74..76].copy_from_slice(&1u16.to_be_bytes());
    data[76..78].copy_from_slice(&8u16.to_be_bytes());

    data[82] = 0xf0; // first four pixels are opaque
    data[83] = 0x00; // monochrome fallback
    data[90..92].copy_from_slice(&0u16.to_be_bytes()); // one ColorSpec
    data[92..94].copy_from_slice(&0u16.to_be_bytes()); // pixel value 0
    data[94..96].copy_from_slice(&0xffffu16.to_be_bytes()); // red
    data[96..98].copy_from_slice(&0u16.to_be_bytes());
    data[98..100].copy_from_slice(&0u16.to_be_bytes());
    data[100] = 0x00; // eight one-bit red pixels
    data
}

fn compatibility_binding(
    library_name: &str,
    symbol_name: &str,
    dispatcher_target: PpcImportDispatcherTarget,
) -> PpcImportBinding {
    PpcImportBinding {
        library_index: 0,
        symbol_index: 0,
        library_name: library_name.to_string(),
        symbol_name: symbol_name.to_string(),
        class: 0,
        weak: false,
        address: 0,
        tvector_address: None,
        trap_pc: 0,
        dispatcher_target,
    }
}

mod apple_event_handlers;

mod apple_event_descriptors;

mod hardware_compatibility;

mod math_compatibility;

mod fixmath;

struct SectionSpec {
    total_size: u32,
    unpacked_size: u32,
    packed_size: u32,
    container_offset: u32,
    section_kind: u8,
}

fn write_section(bytes: &mut [u8], index: usize, spec: SectionSpec) {
    let off = 40 + index * 28;
    write_i32(bytes, off, -1);
    write_u32(bytes, off + 8, spec.total_size);
    write_u32(bytes, off + 12, spec.unpacked_size);
    write_u32(bytes, off + 16, spec.packed_size);
    write_u32(bytes, off + 20, spec.container_offset);
    bytes[off + 24] = spec.section_kind;
    bytes[off + 25] = 4;
    bytes[off + 26] = 4;
}

fn push_c_string(strings: &mut Vec<u8>, value: &[u8]) -> u32 {
    let offset = strings.len() as u32;
    strings.extend_from_slice(value);
    strings.push(0);
    offset
}

fn write_symbol(bytes: &mut [u8], offset: usize, class_byte: u8, name_offset: u32) {
    bytes[offset] = class_byte;
    bytes[offset + 1] = ((name_offset >> 16) & 0xff) as u8;
    bytes[offset + 2] = ((name_offset >> 8) & 0xff) as u8;
    bytes[offset + 3] = (name_offset & 0xff) as u8;
}

fn run_reloc(dispatch: u8, run_length: u16) -> u16 {
    (u16::from(dispatch) << 9) | ((run_length - 1) & 0x01ff)
}

fn sm_index_reloc(dispatch: u8, index: u16) -> u16 {
    (u16::from(dispatch) << 9) | (index & 0x01ff)
}

fn delt(offset: u16) -> u16 {
    (0x40u16 << 9) | ((offset - 1) & 0x0fff)
}

fn d_form_u(opcd: u8, rt: u8, ra: u8, value: u16) -> u32 {
    ((opcd as u32) << 26)
        | ((rt as u32 & 0x1f) << 21)
        | ((ra as u32 & 0x1f) << 16)
        | u32::from(value)
}

fn x_form(opcd: u8, rt: u8, ra: u8, rb: u8, xo: u16, rc: bool) -> u32 {
    ((opcd as u32 & 0x3f) << 26)
        | ((rt as u32 & 0x1f) << 21)
        | ((ra as u32 & 0x1f) << 16)
        | ((rb as u32 & 0x1f) << 11)
        | ((xo as u32 & 0x3ff) << 1)
        | u32::from(rc)
}

fn a_form_fp(opcd: u8, frt: u8, fra: u8, frb: u8, frc: u8, xo_5: u8, rc: bool) -> u32 {
    ((opcd as u32 & 0x3f) << 26)
        | ((frt as u32 & 0x1f) << 21)
        | ((fra as u32 & 0x1f) << 16)
        | ((frb as u32 & 0x1f) << 11)
        | ((frc as u32 & 0x1f) << 6)
        | ((xo_5 as u32 & 0x1f) << 1)
        | u32::from(rc)
}

fn xfx_form(opcd: u8, rt_or_rs: u8, spr_decimal: u16, xo: u16) -> u32 {
    let high_5 = (spr_decimal >> 5) & 0x1f;
    let low_5 = spr_decimal & 0x1f;
    ((opcd as u32 & 0x3f) << 26)
        | ((rt_or_rs as u32 & 0x1f) << 21)
        | ((low_5 as u32) << 16)
        | ((high_5 as u32) << 11)
        | ((xo as u32 & 0x3ff) << 1)
}

fn xl_form(opcd: u8, bo: u8, bi: u8, xo: u16, lk: bool) -> u32 {
    ((opcd as u32 & 0x3f) << 26)
        | ((bo as u32 & 0x1f) << 21)
        | ((bi as u32 & 0x1f) << 16)
        | ((xo as u32 & 0x3ff) << 1)
        | u32::from(lk)
}

fn write_i32(bytes: &mut [u8], offset: usize, value: i32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

#[test]
fn drawing_imports_charge_guest_time_below_one_tick_per_redraw() {
    use PpcImportDispatcherTarget as T;
    for target in [T::DrawText, T::DrawPicture, T::CopyBits, T::PaintRect, T::GetIndString] {
        assert!(ppc_import_extra_cycles_for_target(&target) > 0);
    }
    let tick_cycles = (crate::runner::DEFAULT_REALTIME_PPC_CPU_MHZ * 1_000_000.0
        / crate::runner::DEFAULT_VBL_HZ) as u64;
    let redraw = 370 * ppc_import_extra_cycles_for_target(&T::DrawText)
        + 165 * ppc_import_extra_cycles_for_target(&T::DrawPicture)
        + 135 * ppc_import_extra_cycles_for_target(&T::CopyBits);
    assert!(redraw < tick_cycles / 2, "{redraw} of {tick_cycles}");
}

#[test]
fn hle_import_runner_holds_resident_native_memory() {
    let pef = synthetic_pef_with_import(b"HoldMemory");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_DATA_BASE;
    loaded.cpu.gpr[4] = 4096;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
}

#[test]
fn hle_import_runner_gets_desktop_database_path_for_boot_volume() {
    let pef = synthetic_pef_with_import(b"PBDTGetPath");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pb = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(pb, vec![0; 26]);
    let _ = loaded.memory.write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16);
    loaded.cpu.gpr[3] = pb;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
    assert_eq!(loaded.memory.read_u16_be(pb + 24), Some(0x7f00));
}

#[test]
fn hle_import_runner_reports_missing_desktop_comment() {
    let pef = synthetic_pef_with_import(b"PBDTGetCommentSync");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pb = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(pb, vec![0; 44]);
    let _ = loaded.memory.write_u16_be(pb + 24, 0x7f00);
    loaded.cpu.gpr[3] = pb;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-5012));
    assert_eq!(loaded.memory.read_u16_be(pb + 16), Some((-5012i16) as u16));
    assert_eq!(loaded.memory.read_u32_be(pb + 40), Some(0));
}

#[test]
fn hle_import_runner_reports_resource_autoload_state() {
    let pef = synthetic_pef_with_import(b"LMGetResLoad");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
}

#[test]
fn hle_run_mirrors_shared_process_input_into_powerpc_low_memory() {
    use crate::memory::globals::addr;

    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let mut key_map = [0; PPC_KEY_MAP_SIZE as usize];
    key_map[2] = 0x20;
    loaded.process_input.set_key_map_snapshot(key_map);
    loaded.process_input.set_mouse_state((115, 210), true);

    let _ = loaded.run_with_hle_imports(0);

    assert_eq!(loaded.memory.read_u8(addr::MB_STATE), Some(0));
    assert_eq!(loaded.memory.read_u8(addr::KEY_MAP_LM + 2), Some(0x20));
    assert_eq!(loaded.memory.read_u16_be(addr::MOUSE_LOC2), Some(115));
    assert_eq!(loaded.memory.read_u16_be(addr::MOUSE_LOC2 + 2), Some(210));
}

#[test]
fn system_arena_leaves_a_separate_resource_tail_after_max_block() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    loaded.reserve_ppc_system_storage();

    let limit = loaded.heap_limit();
    let arena = limit - 2 * 1024 * 1024 - PPC_SYSTEM_ALLOCATION_POOL_SIZE;
    let (total, largest) = ppc_heap_free_capacity(&loaded.memory, loaded.heap_cursor(), limit);
    assert!(loaded
        .memory
        .has_readonly_allocation_exclusion(arena, PPC_SYSTEM_ALLOCATION_POOL_SIZE));
    assert!(loaded.memory.read_u8(arena).is_some());
    assert_eq!(total - largest, 2 * 1024 * 1024);
}
