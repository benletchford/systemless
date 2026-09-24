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
fn event_avail_peeks_without_consuming_matching_event() {
    let queue = VecDeque::from([PpcQueuedEvent {
        what: 3,
        message: 0x0000_4120,
        when: 0,
        where_v: 120,
        where_h: 240,
        modifiers: 0x0080,
    }]);

    let event = ppc_peek_event(&queue, 1 << 3, PpcInputSnapshot::default(), false, 7);

    assert_eq!(event, (3, 0x0000_4120, 0, 120, 240, 0x0080, true));
    assert_eq!(queue.len(), 1);
}

#[test]
fn os_event_accessors_skip_toolbox_and_high_level_events() {
    let mut queue = VecDeque::from([
        PpcQueuedEvent {
            what: 6,
            message: 0x1000,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 23,
            message: PPC_CORE_EVENT_CLASS,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 3,
            message: 0x0000_4120,
            when: 0,
            where_v: 120,
            where_h: 240,
            modifiers: 0x0080,
        },
    ]);

    // Macintosh Toolbox Essentials (1992), pp. 2-97--2-99:
    // GetOSEvent and OSEventAvail return only low-level events from the
    // Operating System event queue, never update or high-level events.
    let event =
        ppc_dequeue_event(&mut queue, u16::MAX, PpcInputSnapshot::default(), true, 7);

    assert_eq!(event, (3, 0x0000_4120, 0, 120, 240, 0x0080, true));
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].what, 6);
    assert_eq!(queue[1].what, 23);
    assert_eq!(
        ppc_peek_event(&queue, u16::MAX, PpcInputSnapshot::default(), true, 7),
        (0, 0, 7, 0, 0, 0, false)
    );
}

#[test]
fn toolbox_event_accessors_apply_documented_event_priority() {
    let mut queue = VecDeque::from([
        PpcQueuedEvent {
            what: 6,
            message: 0x1000,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 23,
            message: PPC_CORE_EVENT_CLASS,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 1,
            message: 0,
            when: 0,
            where_v: 120,
            where_h: 240,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 8,
            message: 0x2000,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 1,
        },
    ]);

    let event =
        ppc_dequeue_event(&mut queue, u16::MAX, PpcInputSnapshot::default(), false, 7);
    assert_eq!(event.0, 8, "activate events have highest priority");
    let event =
        ppc_dequeue_event(&mut queue, u16::MAX, PpcInputSnapshot::default(), false, 7);
    assert_eq!(event.0, 1, "user input precedes update events");
    let event =
        ppc_dequeue_event(&mut queue, u16::MAX, PpcInputSnapshot::default(), false, 7);
    assert_eq!(event.0, 6, "update events precede high-level events");
    assert_eq!(queue.front().map(|event| event.what), Some(23));
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
fn restore_entries_updates_selected_device_colors_without_reseeding() {
    let pef = synthetic_pef_with_import(b"RestoreEntries");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut source_bytes = vec![0; 24];
    source_bytes[0..4].copy_from_slice(&0x1234_5678u32.to_be_bytes());
    source_bytes[6..8].copy_from_slice(&1u16.to_be_bytes());
    source_bytes[8..10].copy_from_slice(&9u16.to_be_bytes());
    source_bytes[10..12].copy_from_slice(&0x1111u16.to_be_bytes());
    source_bytes[12..14].copy_from_slice(&0x2222u16.to_be_bytes());
    source_bytes[14..16].copy_from_slice(&0x3333u16.to_be_bytes());
    source_bytes[16..18].copy_from_slice(&10u16.to_be_bytes());
    source_bytes[18..20].copy_from_slice(&0x4444u16.to_be_bytes());
    source_bytes[20..22].copy_from_slice(&0x5555u16.to_be_bytes());
    source_bytes[22..24].copy_from_slice(&0x6666u16.to_be_bytes());
    let source = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &source_bytes,
    );
    let selection = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(selection, vec![0; 6]);
    loaded.memory.write_u16_be(selection, 1).unwrap();
    loaded.memory.write_u16_be(selection + 2, 42).unwrap();
    loaded.memory.write_u16_be(selection + 4, 300).unwrap();
    let destination = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
    let original_seed = loaded.memory.read_u32_be(destination).unwrap();
    let original_sp = loaded.cpu.gpr[1];
    loaded.cpu.gpr[3] = source;
    loaded.cpu.gpr[4] = PPC_MAIN_CTABLE_HANDLE;
    loaded.cpu.gpr[5] = selection;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[1], original_sp);
    assert_eq!(loaded.screen_clut[42], [0x1111, 0x2222, 0x3333]);
    assert_eq!(loaded.memory.read_u16_be(destination + 8 + 42 * 8), Some(9));
    assert_eq!(
        loaded.memory.read_u16_be(destination + 10 + 42 * 8),
        Some(0x1111)
    );
    assert_eq!(loaded.memory.read_u32_be(destination), Some(original_seed));
    assert_eq!(loaded.memory.read_u16_be(selection + 4), Some(u16::MAX));
}

#[test]
fn direct_video_set_entries_updates_screen_clut() {
    let parameter_block = 0x2000;
    let request = 0x2100;
    let table = 0x2200;
    let mut memory = PpcSectionMem::new();
    memory.add_region(parameter_block, vec![0; 64]);
    memory.add_region(request, vec![0; 8]);
    memory.add_region(table, vec![0; 8]);
    memory.write_u16_be(parameter_block + 26, 8).unwrap();
    memory.write_u32_be(parameter_block + 28, request).unwrap();
    memory.write_u32_be(request, table).unwrap();
    memory.write_u16_be(request + 4, 7).unwrap();
    memory.write_u16_be(request + 6, 0).unwrap();
    memory.write_u16_be(table, 99).unwrap();
    memory.write_u16_be(table + 2, 0x1111).unwrap();
    memory.write_u16_be(table + 4, 0x2222).unwrap();
    memory.write_u16_be(table + 6, 0x3333).unwrap();
    let mut screen_clut = [[0; 3]; 256];
    let display_gamma = SharedProcessDisplayGamma::default();
    let mut startup = PpcToolboxStartupState::default();
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = parameter_block;

    assert_eq!(
        ppc_pb_control(
            &cpu,
            &mut memory,
            0,
            &mut screen_clut,
            &display_gamma,
            &mut startup,
        ),
        PPC_NO_ERR
    );
    assert_eq!(screen_clut[7], [0x1111, 0x2222, 0x3333]);
    assert_eq!(display_gamma.table(), crate::display::linear_display_gamma());
    assert_eq!(memory.read_u16_be(parameter_block + 16), Some(0));

    let installed_gamma = [[42; 256]; 3];
    display_gamma.install(installed_gamma);
    assert_eq!(
        ppc_pb_control(
            &cpu,
            &mut memory,
            0,
            &mut screen_clut,
            &display_gamma,
            &mut startup,
        ),
        PPC_NO_ERR
    );
    assert_eq!(display_gamma.table(), installed_gamma);
}

#[test]
fn video_status_returns_device_owned_linear_gamma_table() {
    let pef = synthetic_pef_with_import(b"PBStatusSync");
    let mut loaded = load_pef_application(&pef).unwrap();
    let parameter_block = PPC_DATA_BASE + 0x1000;
    let vd_gamma = parameter_block + 64;
    loaded.memory.add_region(parameter_block, vec![0; 128]);
    loaded.memory.write_u16_be(parameter_block + 24, 0).unwrap();
    loaded.memory.write_u16_be(parameter_block + 26, 8).unwrap();
    loaded
        .memory
        .write_u32_be(parameter_block + 28, vd_gamma)
        .unwrap();
    loaded.cpu.gpr[3] = parameter_block;

    assert_eq!(ppc_pb_status(&loaded.cpu, &mut loaded.memory), PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(parameter_block + 16), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(vd_gamma),
        Some(PPC_MAIN_GAMMA_TABLE)
    );
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GAMMA_TABLE), Some(0));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GAMMA_TABLE + 6), Some(1));
    assert_eq!(
        loaded.memory.read_u16_be(PPC_MAIN_GAMMA_TABLE + 8),
        Some(256)
    );
    assert_eq!(
        loaded.memory.read_u16_be(PPC_MAIN_GAMMA_TABLE + 10),
        Some(8)
    );
    for value in 0..256u32 {
        assert_eq!(
            loaded.memory.read_u8(PPC_MAIN_GAMMA_TABLE + 12 + value),
            Some(value as u8)
        );
    }
}

#[test]
fn synthetic_main_color_table_does_not_overlap_gamma_storage() {
    let color_table_end = PPC_MAIN_CTABLE + PPC_MAIN_CTABLE_SIZE;
    let gamma_table_end = PPC_MAIN_GAMMA_TABLE + PPC_MAIN_GAMMA_TABLE_SIZE;

    assert!(
        color_table_end <= PPC_MAIN_GAMMA_TABLE || gamma_table_end <= PPC_MAIN_CTABLE,
        "the 256-entry ColorTable and device GammaTbl must occupy disjoint storage"
    );
}

#[test]
fn video_control_installs_three_channel_gamma_table() {
    let parameter_block = 0x2000;
    let vd_gamma = 0x2100;
    let table = 0x2200;
    let mut memory = PpcSectionMem::new();
    memory.add_region(parameter_block, vec![0; 64]);
    memory.add_region(vd_gamma, vec![0; 4]);
    memory.add_region(table, vec![0; 24]);
    memory.write_u16_be(parameter_block + 26, 4).unwrap();
    memory.write_u32_be(parameter_block + 28, vd_gamma).unwrap();
    memory.write_u32_be(vd_gamma, table).unwrap();
    memory.write_u16_be(table, 0).unwrap();
    memory.write_u16_be(table + 2, 0).unwrap();
    memory.write_u16_be(table + 4, 0).unwrap();
    memory.write_u16_be(table + 6, 3).unwrap();
    memory.write_u16_be(table + 8, 4).unwrap();
    memory.write_u16_be(table + 10, 2).unwrap();
    memory
        .write_bytes(table + 12, &[0, 10, 20, 30, 1, 11, 21, 31, 2, 12, 22, 32])
        .unwrap();
    let mut screen_clut = [[0; 3]; 256];
    let display_gamma = SharedProcessDisplayGamma::default();
    let mut startup = PpcToolboxStartupState::default();
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = parameter_block;

    assert_eq!(
        ppc_pb_control(
            &cpu,
            &mut memory,
            0,
            &mut screen_clut,
            &display_gamma,
            &mut startup,
        ),
        PPC_NO_ERR
    );
    let installed = display_gamma.table();
    assert_eq!(installed[0][0], 0);
    assert_eq!(installed[0][64], 10);
    assert_eq!(installed[0][255], 30);
    assert_eq!(installed[1][64], 11);
    assert_eq!(installed[2][255], 32);
    assert!(display_gamma.is_explicit());
}

#[test]
fn direct_video_set_entries_does_not_replace_quickdraws_logical_color_table() {
    let pef = synthetic_pef_with_import(b"PBControlSync");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let parameter_block = scratch;
    let request = scratch + 0x100;
    let table = scratch + 0x200;
    loaded.memory.add_region(scratch, vec![0; 0x300]);
    loaded.memory.write_u16_be(parameter_block + 26, 8).unwrap();
    loaded
        .memory
        .write_u32_be(parameter_block + 28, request)
        .unwrap();
    loaded.memory.write_u32_be(request, table).unwrap();
    loaded.memory.write_u16_be(request + 4, 7).unwrap();
    loaded.memory.write_u16_be(request + 6, 0).unwrap();
    loaded.memory.write_u16_be(table, 7).unwrap();
    loaded.memory.write_u16_be(table + 2, 0x1111).unwrap();
    loaded.memory.write_u16_be(table + 4, 0x2222).unwrap();
    loaded.memory.write_u16_be(table + 6, 0x3333).unwrap();
    let ctable_handle =
        ppc_gdevice_ctable_handle(&mut loaded.memory, *loaded.current_gdevice).unwrap();
    let logical_before = ppc_read_ctable_clut(
        &mut loaded.memory,
        ctable_handle,
        &loaded.color_manager_clut,
    )
    .unwrap();
    let screen_clut = loaded.screen_clut;
    let display_gamma = loaded.display_gamma.shared_handle();
    let mut startup = loaded.toolbox_startup;
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = parameter_block;

    assert_eq!(
        screen_clut.with_mut(|screen_clut| ppc_pb_control(
            &cpu,
            &mut loaded.memory,
            *loaded.current_gdevice,
            screen_clut,
            &display_gamma,
            &mut startup,
        )),
        PPC_NO_ERR
    );
    assert_eq!(screen_clut[7], [0x1111, 0x2222, 0x3333]);
    assert_eq!(
        ppc_read_ctable_clut(
            &mut loaded.memory,
            ctable_handle,
            &loaded.color_manager_clut,
        )
        .unwrap(),
        logical_before
    );
}

#[test]
fn ppc_rgb2hsl_converts_primary_and_gray_colors() {
    let rgb = 0x2000;
    let hsl = 0x2100;
    let mut memory = PpcSectionMem::new();
    memory.add_region(rgb, vec![0; 0x200]);
    memory.write_u16_be(rgb, 0xffff).unwrap();
    memory.write_u16_be(rgb + 2, 0).unwrap();
    memory.write_u16_be(rgb + 4, 0).unwrap();

    assert!(ppc_rgb2hsl(&mut memory, rgb, hsl));
    assert_eq!(memory.read_u16_be(hsl), Some(0));
    assert_eq!(memory.read_u16_be(hsl + 2), Some(0xffff));
    assert_eq!(memory.read_u16_be(hsl + 4), Some(0x8000));

    memory.write_u16_be(rgb, 0x2468).unwrap();
    memory.write_u16_be(rgb + 2, 0x2468).unwrap();
    memory.write_u16_be(rgb + 4, 0x2468).unwrap();
    assert!(ppc_rgb2hsl(&mut memory, rgb, hsl));
    assert_eq!(memory.read_u16_be(hsl), Some(0));
    assert_eq!(memory.read_u16_be(hsl + 2), Some(0));
    assert_eq!(memory.read_u16_be(hsl + 4), Some(0x2468));
}

#[test]
fn ppc_rgb2hsv_converts_primary_and_gray_colors() {
    let rgb = 0x2000;
    let hsv = 0x2100;
    let mut memory = PpcSectionMem::new();
    memory.add_region(rgb, vec![0; 0x200]);
    memory.write_u16_be(rgb, 0).unwrap();
    memory.write_u16_be(rgb + 2, 0xffff).unwrap();
    memory.write_u16_be(rgb + 4, 0).unwrap();

    assert!(ppc_rgb2hsv(&mut memory, rgb, hsv));
    assert_eq!(memory.read_u16_be(hsv), Some(0x5555));
    assert_eq!(memory.read_u16_be(hsv + 2), Some(0xffff));
    assert_eq!(memory.read_u16_be(hsv + 4), Some(0xffff));

    memory.write_u16_be(rgb, 0x2468).unwrap();
    memory.write_u16_be(rgb + 2, 0x2468).unwrap();
    memory.write_u16_be(rgb + 4, 0x2468).unwrap();
    assert!(ppc_rgb2hsv(&mut memory, rgb, hsv));
    assert_eq!(memory.read_u16_be(hsv), Some(0));
    assert_eq!(memory.read_u16_be(hsv + 2), Some(0));
    assert_eq!(memory.read_u16_be(hsv + 4), Some(0x2468));

    memory.write_u16_be(hsv, 0xaaaa).unwrap();
    memory.write_u16_be(hsv + 2, 0xffff).unwrap();
    memory.write_u16_be(hsv + 4, 0xffff).unwrap();
    assert!(ppc_hsv2rgb(&mut memory, hsv, rgb));
    assert_eq!(memory.read_u16_be(rgb), Some(0));
    assert_eq!(memory.read_u16_be(rgb + 2), Some(0));
    assert_eq!(memory.read_u16_be(rgb + 4), Some(0xffff));
}

#[test]
fn ppc_fix_ratio_truncates_signed_results_and_saturates_zero_divisors() {
    assert_eq!(ppc_fix_ratio(3, 2), 0x0001_8000);
    assert_eq!(ppc_fix_ratio(-1, 3), -0x0000_5555);
    assert_eq!(ppc_fix_ratio(1, 0), i32::MAX);
    assert_eq!(ppc_fix_ratio(-1, 0), i32::MIN + 1);
}

#[test]
fn ppc_fixed_point_helpers_round_multiply_and_saturate_conversion() {
    assert_eq!(ppc_fix_mul(0x0001_8000, 0x0001_4000), 0x0001_e000);
    assert_eq!(ppc_fix_mul(-0x0001_8000, 0x0001_4000), -0x0001_e000);
    assert_eq!(ppc_long_to_fix(72), 0x0048_0000);
    assert_eq!(ppc_long_to_fix(0x8000), i32::MAX);
    assert_eq!(ppc_long_to_fix(-0x8001), i32::MIN);
}

#[test]
fn ppc_fix_round_rounds_to_nearest_halves_away_from_zero() {
    assert_eq!(ppc_fix_round(0x0001_4000), 1);
    assert_eq!(ppc_fix_round(0x0001_8000), 2);
    assert_eq!(ppc_fix_round(0x0001_c000), 2);
    assert_eq!(ppc_fix_round(-0x0001_4000), -1);
    assert_eq!(ppc_fix_round(-0x0001_8000), -2);
    assert_eq!(ppc_fix_round(i32::MIN), i16::MIN);
    assert_eq!(ppc_fix_round(0), 0);
}

#[test]
fn ppc_fix_and_frac_conversions() {
    assert_eq!(ppc_fix_to_frac(0x0001_0000), 0x4000_0000);
    assert_eq!(ppc_fix_to_frac(-0x0001_0000), -0x4000_0000);
    assert_eq!(ppc_fix_to_frac(0x0002_0000), i32::MAX);
    assert_eq!(ppc_fix_to_frac(-0x0002_0001), i32::MIN);

    assert_eq!(ppc_frac_to_fix(0x4000_0000), 0x0001_0000);
    assert_eq!(ppc_frac_to_fix(0x7000_0000), 0x0001_c000);
    assert_eq!(ppc_frac_to_fix(-0x4000_0000), -0x0001_0000);
    assert_eq!(ppc_frac_to_fix(0x2000_0000), 0x0000_8000);

    assert_eq!(ppc_f64_to_frac(1.0), 0x4000_0000);
    assert_eq!(ppc_f64_to_frac(-1.0), (-0x4000_0000i32) as u32);
    assert_eq!(ppc_f64_to_frac(0.5), 0x2000_0000);
    assert_eq!(ppc_f64_to_frac(3.0), 0x7fff_ffff);
}

#[test]
fn ppc_frac_arithmetic_and_trig() {
    assert_eq!(ppc_frac_mul(0x2000_0000, 0x2000_0000), 0x1000_0000);
    assert_eq!(ppc_frac_mul(-0x2000_0000, 0x2000_0000), -0x1000_0000);
    assert_eq!(
        ppc_frac_mul(0xa000_0000u32 as i32, 0x5333_3333),
        0x8333_3333u32 as i32
    );

    assert_eq!(ppc_frac_div(0x1000_0000, 0x2000_0000), 0x2000_0000);
    assert_eq!(ppc_frac_div(0x7ccc_cccd, 0x5333_3333), 0x6000_0000);
    assert_eq!(
        ppc_frac_div(0x8333_3333u32 as i32, 0x5333_3333),
        0xa000_0000u32 as i32
    );
    assert_eq!(ppc_frac_div(0x4000_0000, 0x1000_0000), i32::MAX);
    assert_eq!(ppc_frac_div(0x1000_0000, 0), i32::MAX);
    assert_eq!(ppc_frac_div(-0x1000_0000, 0), i32::MIN);

    assert_eq!(ppc_frac_sqrt(0x4000_0000), 0x4000_0000);
    assert_eq!(ppc_frac_sqrt(0x1000_0000), 0x2000_0000);
    assert_eq!(ppc_frac_sqrt(0x7d70_a3d7), 0x5999_999a);
    assert_eq!(ppc_frac_sqrt(0), 0);

    assert_eq!(ppc_frac_sin(0), 0);
    assert_eq!(ppc_frac_cos(0), 0x4000_0000);
    let pi_over_2_fixed = 102_944;
    let sin_pi_2 = ppc_frac_sin(pi_over_2_fixed);
    assert_eq!(sin_pi_2, 0x4000_0000);
    let cos_pi_2 = ppc_frac_cos(pi_over_2_fixed);
    assert_eq!(cos_pi_2, 0);
    assert_eq!(ppc_frac_sin(205_888), 0);
    assert_eq!(ppc_frac_cos(205_888), -0x4000_0000);

    assert_eq!(ppc_fix_atan2(1, 0), 0);
    assert_eq!(ppc_fix_atan2(0, 1), 102_944);
    assert_eq!(ppc_fix_atan2(-1, -1), -154_416);
}

#[test]
fn ppc_fixmath_imports_execute_through_synthetic_pefs() {
    // Universal Interfaces 3.4 FixMath.h declares these as InterfaceLib
    // exports; examples and boundaries are from Inside Macintosh IV-65
    // and Operating System Utilities (1994), pp. 3-40 through 3-46.
    let cases: [(
        &str,
        PpcImportDispatcherTarget,
        u32,
        u32,
        Option<f64>,
        Option<u32>,
        Option<f64>,
    ); 11] = [
        (
            "FixRound",
            PpcImportDispatcherTarget::FixRound,
            i32::MIN as u32,
            0,
            None,
            Some(0xffff_8000),
            None,
        ),
        (
            "Fix2Frac",
            PpcImportDispatcherTarget::Fix2Frac,
            0x0001_c000,
            0,
            None,
            Some(0x7000_0000),
            None,
        ),
        (
            "Frac2Fix",
            PpcImportDispatcherTarget::Frac2Fix,
            0x9000_0000,
            0,
            None,
            Some(0xfffe_4000),
            None,
        ),
        (
            "Frac2X",
            PpcImportDispatcherTarget::Frac2X,
            0x7000_0000,
            0,
            None,
            None,
            Some(1.75),
        ),
        (
            "X2Frac",
            PpcImportDispatcherTarget::X2Frac,
            0,
            0,
            Some(-3.0),
            Some(0x8000_0000),
            None,
        ),
        (
            "FracSin",
            PpcImportDispatcherTarget::FracSin,
            205_888,
            0,
            None,
            Some(0),
            None,
        ),
        (
            "FracCos",
            PpcImportDispatcherTarget::FracCos,
            205_888,
            0,
            None,
            Some(0xc000_0000),
            None,
        ),
        (
            "FracSqrt",
            PpcImportDispatcherTarget::FracSqrt,
            0xc000_0000,
            0,
            None,
            Some(0x6ed9_eba1),
            None,
        ),
        (
            "FracMul",
            PpcImportDispatcherTarget::FracMul,
            0x6000_0000,
            0x5333_3333,
            None,
            Some(0x7ccc_cccd),
            None,
        ),
        (
            "FracDiv",
            PpcImportDispatcherTarget::FracDiv,
            0x8333_3333,
            0,
            None,
            Some(0x8000_0000),
            None,
        ),
        (
            "FixATan2",
            PpcImportDispatcherTarget::FixATan2,
            1,
            1,
            None,
            Some(0x0000_c910),
            None,
        ),
    ];

    for (name, target, r3, r4, f1, expected_r3, expected_f1) in cases {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(name.as_bytes()))
            .expect("synthetic FixMath PEF should load");
        assert_eq!(loaded.imports[0].dispatcher_target, target, "{name}");
        loaded.cpu.gpr[3] = r3;
        loaded.cpu.gpr[4] = r4;
        loaded.cpu.fpr[1] = f1.unwrap_or_default().to_bits();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;

        let result = loaded.run_with_hle_imports(64);

        assert_eq!(result.handled_import_count, 1, "{name}");
        if let Some(expected) = expected_r3 {
            assert_eq!(loaded.cpu.gpr[3], expected, "{name}");
        }
        if let Some(expected) = expected_f1 {
            assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), expected, "{name}");
        }
    }
}

#[test]
fn ppc_wide_fixmath_helpers_mutate_records_and_apply_documented_results() {
    // QuickDraw GX Environment and Utilities (1994), pp. 8-49--8-54.
    const TARGET: u32 = 0x2000;
    const SOURCE: u32 = 0x2008;
    const REMAINDER: u32 = 0x2010;
    let mut memory = PpcSectionMem::new();
    memory.add_region(TARGET, vec![0; 0x100]);

    ppc_write_wide(&mut memory, TARGET, 0x0000_0001_ffff_ffff).unwrap();
    ppc_write_wide(&mut memory, SOURCE, 2).unwrap();
    assert_eq!(ppc_wide_add(&mut memory, TARGET, SOURCE), TARGET);
    assert_eq!(
        ppc_read_wide(&mut memory, TARGET),
        Some(0x0000_0002_0000_0001)
    );
    assert_eq!(ppc_wide_subtract(&mut memory, TARGET, SOURCE), TARGET);
    assert_eq!(
        ppc_read_wide(&mut memory, TARGET),
        Some(0x0000_0001_ffff_ffff)
    );
    assert_eq!(ppc_wide_negate(&mut memory, TARGET), TARGET);
    assert_eq!(
        ppc_read_wide(&mut memory, TARGET),
        Some(-0x0000_0001_ffff_ffff)
    );

    ppc_write_wide(&mut memory, TARGET, 7).unwrap();
    assert_eq!(ppc_wide_shift(&mut memory, TARGET, 1), TARGET);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(4));
    ppc_write_wide(&mut memory, TARGET, -7).unwrap();
    ppc_wide_shift(&mut memory, TARGET, 1);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-3));
    ppc_wide_shift(&mut memory, TARGET, -2);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-12));

    ppc_write_wide(&mut memory, TARGET, 7).unwrap();
    assert_eq!(ppc_wide_bit_shift(&mut memory, TARGET, 1), TARGET);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(3));
    ppc_write_wide(&mut memory, TARGET, -7).unwrap();
    ppc_wide_bit_shift(&mut memory, TARGET, 1);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-4));
    ppc_wide_bit_shift(&mut memory, TARGET, -2);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-16));
    ppc_wide_bit_shift(&mut memory, TARGET, 64);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-16));
    ppc_wide_bit_shift(&mut memory, TARGET, 65);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-8));

    assert_eq!(
        ppc_wide_multiply(&mut memory, -2_000_000_000, 2, TARGET),
        TARGET
    );
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-4_000_000_000));

    ppc_write_wide(&mut memory, TARGET, 10).unwrap();
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 3, REMAINDER), 3);
    assert_eq!(memory.read_u32_be(REMAINDER), Some(1));
    ppc_write_wide(&mut memory, TARGET, 11).unwrap();
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 2, 0), 6);
    ppc_write_wide(&mut memory, TARGET, -11).unwrap();
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 2, 0), -5);
    ppc_write_wide(&mut memory, TARGET, i64::MAX).unwrap();
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 1, REMAINDER), i32::MAX);
    assert_eq!(memory.read_u32_be(REMAINDER), Some(i32::MIN as u32));
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 1, u32::MAX), i32::MIN);

    ppc_write_wide(&mut memory, TARGET, 11).unwrap();
    assert_eq!(
        ppc_wide_wide_divide(&mut memory, TARGET, 2, REMAINDER),
        TARGET
    );
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(5));
    assert_eq!(memory.read_u32_be(REMAINDER), Some(1));
    ppc_write_wide(&mut memory, TARGET, 11).unwrap();
    ppc_wide_wide_divide(&mut memory, TARGET, 2, 0);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(6));

    ppc_write_wide(&mut memory, TARGET, 81).unwrap();
    assert_eq!(ppc_wide_square_root(&mut memory, TARGET), 9);
    memory.write_u32_be(TARGET, u32::MAX).unwrap();
    memory.write_u32_be(TARGET + 4, u32::MAX).unwrap();
    assert_eq!(ppc_wide_square_root(&mut memory, TARGET), u32::MAX);

    ppc_write_wide(&mut memory, TARGET, -1).unwrap();
    ppc_write_wide(&mut memory, SOURCE, 1).unwrap();
    assert_eq!(ppc_wide_compare(&mut memory, TARGET, SOURCE), -1);
    ppc_write_wide(&mut memory, SOURCE, -1).unwrap();
    assert_eq!(ppc_wide_compare(&mut memory, TARGET, SOURCE), 0);
}

#[test]
fn ppc_wide_fixmath_imports_execute_through_synthetic_pefs() {
    // Universal Interfaces 3.4 FixMath.h declares these as InterfaceLib
    // exports; semantics are from QuickDraw GX Environment and Utilities
    // (1994), pp. 8-49 through 8-54.
    let cases = [
        ("WideAdd", PpcImportDispatcherTarget::WideAdd),
        ("WideSubtract", PpcImportDispatcherTarget::WideSubtract),
        ("WideNegate", PpcImportDispatcherTarget::WideNegate),
        ("WideShift", PpcImportDispatcherTarget::WideShift),
        ("WideBitShift", PpcImportDispatcherTarget::WideBitShift),
        ("WideMultiply", PpcImportDispatcherTarget::WideMultiply),
        ("WideDivide", PpcImportDispatcherTarget::WideDivide),
        ("WideWideDivide", PpcImportDispatcherTarget::WideWideDivide),
        ("WideSquareRoot", PpcImportDispatcherTarget::WideSquareRoot),
        ("WideCompare", PpcImportDispatcherTarget::WideCompare),
    ];

    for (name, target) in cases {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(name.as_bytes()))
            .expect("synthetic Wide FixMath PEF should load");
        assert_eq!(loaded.imports[0].dispatcher_target, target, "{name}");
        let scratch = PPC_DATA_BASE + 0x1000;
        let source = scratch + 8;
        let remainder = scratch + 16;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        ppc_write_wide(&mut loaded.memory, scratch, 9).unwrap();
        ppc_write_wide(&mut loaded.memory, source, 4).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = source;
        loaded.cpu.gpr[5] = remainder;
        match name {
            "WideShift" | "WideBitShift" => loaded.cpu.gpr[4] = 1,
            "WideMultiply" => {
                loaded.cpu.gpr[3] = (-3i32) as u32;
                loaded.cpu.gpr[4] = 7;
                loaded.cpu.gpr[5] = scratch;
            }
            "WideDivide" | "WideWideDivide" => {
                loaded.cpu.gpr[4] = 2;
            }
            "WideSquareRoot" => {}
            _ => {}
        }
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;

        let result = loaded.run_with_hle_imports(64);

        assert_eq!(result.handled_import_count, 1, "{name}");
        match name {
            "WideAdd" => assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(13)),
            "WideSubtract" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(5))
            }
            "WideNegate" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(-9))
            }
            "WideShift" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(5))
            }
            "WideBitShift" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(4))
            }
            "WideMultiply" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(-21))
            }
            "WideDivide" => {
                assert_eq!(loaded.cpu.gpr[3], 4);
                assert_eq!(loaded.memory.read_u32_be(remainder), Some(1));
            }
            "WideWideDivide" => {
                assert_eq!(loaded.cpu.gpr[3], scratch);
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(4));
                assert_eq!(loaded.memory.read_u32_be(remainder), Some(1));
            }
            "WideSquareRoot" => assert_eq!(loaded.cpu.gpr[3], 3),
            "WideCompare" => assert_eq!(loaded.cpu.gpr[3], 1),
            _ => unreachable!(),
        }
    }
}

#[test]
fn ppc_pb_get_fcb_info_reports_open_data_and_application_resource_forks() {
    let pb = 0x2000;
    let name = 0x2100;
    let mut memory = PpcSectionMem::new();
    memory.add_region(pb, vec![0; 0x200]);
    let directories = initial_ppc_vfs_directories();
    let path = "System Folder/Preferences/Test File";
    let files = vec![PpcFileRecord {
        ref_num: 130,
        path: path.to_string(),
        position: 2,
    }];
    let vfs_files = vec![PpcVfsFileRecord {
        path: path.to_string(),
        data: (b"data".to_vec()).into(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        dirty: false,
    }];
    memory.write_u32_be(pb + 18, name).unwrap();
    memory.write_u16_be(pb + 22, 0x1234).unwrap();
    memory.write_u16_be(pb + 24, 130).unwrap();
    memory.write_u16_be(pb + 28, 0).unwrap();
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = pb;

    assert_eq!(
        ppc_pb_get_fcb_info(
            &cpu,
            &mut memory,
            &files,
            &[],
            &directories,
            &vfs_files,
            &[],
            &[],
            Some("Escape Velocity"),
        ),
        PPC_NO_ERR
    );
    assert_eq!(
        ppc_read_pstring_bytes(&mut memory, name),
        Some(b"Test File".to_vec())
    );
    assert_eq!(memory.read_u16_be(pb + 36), Some(0x0100));
    assert_eq!(memory.read_u32_be(pb + 40), Some(4));
    assert_eq!(memory.read_u32_be(pb + 48), Some(2));
    assert_eq!(memory.read_u32_be(pb + 58), Some(PPC_PREFERENCES_DIR_ID));

    let resource_path = "Escape Velocity";
    let resource_forks = vec![PpcVfsResourceFileRecord {
        path: resource_path.to_string(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        resource_len: 3,
        raw_data: Some(b"fork".to_vec().into()),
        map_attrs: 0,
        dirty: false,
    }];
    memory.write_u16_be(pb + 24, 0).unwrap();
    assert_eq!(
        ppc_pb_get_fcb_info(
            &cpu,
            &mut memory,
            &files,
            &[],
            &directories,
            &vfs_files,
            &resource_forks,
            &[],
            Some(resource_path),
        ),
        PPC_NO_ERR
    );
    assert_eq!(memory.read_u16_be(pb + 36), Some(0x0200));
    assert_eq!(memory.read_u32_be(pb + 40), Some(4));
    assert_eq!(memory.read_u32_be(pb + 58), Some(PPC_ROOT_DIR_ID));
}

#[test]
fn protected_ppc_clut_entries_are_unchanged_by_set_entries() {
    let table = 0x2000;
    let mut memory = PpcSectionMem::new();
    memory.add_region(table, vec![0; 8]);
    memory.write_u16_be(table, 7).unwrap();
    memory.write_u16_be(table + 2, 0x1111).unwrap();
    memory.write_u16_be(table + 4, 0x2222).unwrap();
    memory.write_u16_be(table + 6, 0x3333).unwrap();
    let mut screen_clut = [[0; 3]; 256];
    let mut startup = PpcToolboxStartupState::default();
    ppc_set_clut_entry_flag(&mut startup.clut_protected, 7, true);

    let protected = startup.clut_protected;
    assert!(ppc_apply_set_entries(
        &mut memory,
        -1,
        0,
        table,
        0,
        &mut screen_clut,
        &protected,
        None,
        true,
        &mut startup,
    ));
    assert_eq!(screen_clut[7], [0, 0, 0]);

    ppc_set_clut_entry_flag(&mut startup.clut_protected, 7, false);
    let protected = startup.clut_protected;
    assert!(ppc_apply_set_entries(
        &mut memory,
        -1,
        0,
        table,
        0,
        &mut screen_clut,
        &protected,
        None,
        true,
        &mut startup,
    ));
    assert_eq!(screen_clut[7], [0x1111, 0x2222, 0x3333]);
}

#[test]
fn ppc_clut_entry_flags_ignore_invalid_indices_and_can_be_cleared() {
    let mut flags = [false; 256];
    ppc_set_clut_entry_flag(&mut flags, 3, true);
    ppc_set_clut_entry_flag(&mut flags, -1, true);
    ppc_set_clut_entry_flag(&mut flags, 256, true);
    assert!(flags[3]);
    assert_eq!(flags.iter().filter(|flag| **flag).count(), 1);

    ppc_set_clut_entry_flag(&mut flags, 3, false);
    assert!(!flags[3]);
}

#[test]
fn color_manager_entry_flags_are_scoped_per_gdevice() {
    let mut startup = PpcToolboxStartupState::default();
    let other_gdevice = PPC_DATA_BASE + 0x9000;
    ppc_set_clut_entry_flag(
        ppc_device_clut_protected_mut(&mut startup, other_gdevice),
        7,
        true,
    );
    ppc_set_clut_entry_flag(
        ppc_device_clut_reserved_mut(&mut startup, other_gdevice),
        8,
        true,
    );

    assert!(!ppc_device_clut_protected(&startup, PPC_MAIN_GDEVICE)[7]);
    assert!(!ppc_device_clut_reserved(&startup, PPC_MAIN_GDEVICE)[8]);
    assert!(ppc_device_clut_protected(&startup, other_gdevice)[7]);
    assert!(ppc_device_clut_reserved(&startup, other_gdevice)[8]);
}

#[test]
fn make_itable_resizes_target_and_excludes_reserved_colors() {
    let heap_base = 0x3000;
    let heap_limit = heap_base + 0x20_000;
    let mut memory = PpcSectionMem::new();
    memory.add_region(heap_base, vec![0; (heap_limit - heap_base) as usize]);
    let mut heap_cursor = heap_base;
    let mut handles = Vec::new();
    let mut ctable = Vec::new();
    ctable.extend_from_slice(&0x1234_5678u32.to_be_bytes());
    ctable.extend_from_slice(&0u16.to_be_bytes());
    ctable.extend_from_slice(&1u16.to_be_bytes());
    for (index, color) in [(7u16, [0u16, 0, 0]), (9u16, [0xffffu16; 3])] {
        ctable.extend_from_slice(&index.to_be_bytes());
        for component in color {
            ctable.extend_from_slice(&component.to_be_bytes());
        }
    }
    let ctable_handle = ppc_alloc_handle_with_bytes(
        &mut memory,
        &mut heap_cursor,
        heap_limit,
        &mut handles,
        &ctable,
    );
    let itable_handle = ppc_alloc_handle(
        &mut memory,
        &mut heap_cursor,
        heap_limit,
        &mut handles,
        1,
        true,
    );
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = ctable_handle;
    cpu.gpr[4] = itable_handle;
    cpu.gpr[5] = 4;
    let mut startup = PpcToolboxStartupState::default();
    ppc_set_clut_entry_flag(&mut startup.clut_reserved, 7, true);
    let mut last_mem_error = PPC_NO_ERR;

    ppc_make_itable(
        &cpu,
        None,
        &mut memory,
        &mut heap_cursor,
        heap_limit,
        &mut last_mem_error,
        &mut handles,
        PPC_MAIN_GDEVICE,
        &[[0; 3]; 256],
        &mut startup,
    );

    assert_eq!(*startup.last_quickdraw_error, PPC_NO_ERR);
    let record = handles
        .iter()
        .find(|record| record.handle == itable_handle)
        .unwrap();
    assert_eq!(record.size, 6 + 4096);
    let itable = memory.read_u32_be(itable_handle).unwrap();
    assert_eq!(memory.read_u32_be(itable), Some(0x1234_5678));
    assert_eq!(memory.read_u16_be(itable + 4), Some(4));
    assert!(ppc_memory_read_bytes(&mut memory, itable + 6, 4096)
        .unwrap()
        .iter()
        .all(|index| *index == 9));
}

#[test]
fn make_itable_reports_invalid_resolution_without_touching_target() {
    let heap_base = 0x3000;
    let heap_limit = heap_base + 0x1000;
    let mut memory = PpcSectionMem::new();
    memory.add_region(heap_base, vec![0; (heap_limit - heap_base) as usize]);
    let mut heap_cursor = heap_base;
    let mut handles = Vec::new();
    let itable_handle = ppc_alloc_handle_with_bytes(
        &mut memory,
        &mut heap_cursor,
        heap_limit,
        &mut handles,
        b"unchanged",
    );
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = itable_handle;
    cpu.gpr[5] = 2;
    let mut startup = PpcToolboxStartupState::default();
    let mut last_mem_error = PPC_NO_ERR;

    ppc_make_itable(
        &cpu,
        None,
        &mut memory,
        &mut heap_cursor,
        heap_limit,
        &mut last_mem_error,
        &mut handles,
        0,
        &[[0; 3]; 256],
        &mut startup,
    );

    assert_eq!(*startup.last_quickdraw_error, PPC_C_RES_ERR);
    let itable = memory.read_u32_be(itable_handle).unwrap();
    assert_eq!(
        ppc_memory_read_bytes(&mut memory, itable, 9),
        Some(b"unchanged".to_vec())
    );
}

#[test]
fn hand_to_hand_duplicates_tracked_handle_storage() {
    let heap_base = 0x3000;
    let handle_variable = 0x3f00;
    let mut memory = PpcSectionMem::new();
    memory.add_region(heap_base, vec![0; 0x1000]);
    let mut heap_cursor = heap_base;
    let mut handles = Vec::new();
    let mut free_handles = Vec::new();
    let source = ppc_alloc_handle_with_bytes(
        &mut memory,
        &mut heap_cursor,
        heap_base + 0x1000,
        &mut handles,
        b"copy me",
    );
    memory.write_u32_be(handle_variable, source).unwrap();
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = handle_variable;

    assert_eq!(
        ppc_hand_to_hand(
            &cpu,
            &mut memory,
            &mut heap_cursor,
            heap_base + 0x1000,
            &mut handles,
            &mut free_handles,
        ),
        PPC_NO_ERR
    );
    let copy = memory.read_u32_be(handle_variable).unwrap();
    assert_ne!(copy, source);
    let copy_data = memory.read_u32_be(copy).unwrap();
    assert_eq!(
        ppc_memory_read_bytes(&mut memory, copy_data, 7),
        Some(b"copy me".to_vec())
    );
}

#[test]
fn hand_to_hand_duplicates_main_device_color_table() {
    let heap_base = 0x0300_0000;
    let heap_limit = heap_base + 0x4000;
    let handle_variable = heap_base + 0x3f00;
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_MAIN_CTABLE_HANDLE, vec![0; 4]);
    memory.add_region(PPC_MAIN_CTABLE, vec![0; PPC_MAIN_CTABLE_SIZE as usize]);
    memory.add_region(heap_base, vec![0; (heap_limit - heap_base) as usize]);
    ppc_seed_main_color_table(&mut memory).unwrap();
    memory
        .write_u32_be(handle_variable, PPC_MAIN_CTABLE_HANDLE)
        .unwrap();
    let mut heap_cursor = heap_base;
    let mut handles = Vec::new();
    let mut free_handles = Vec::new();
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = handle_variable;

    assert_eq!(
        ppc_hand_to_hand(
            &cpu,
            &mut memory,
            &mut heap_cursor,
            heap_limit,
            &mut handles,
            &mut free_handles,
        ),
        PPC_NO_ERR
    );
    let copy = memory.read_u32_be(handle_variable).unwrap();
    let copy_data = memory.read_u32_be(copy).unwrap();
    assert_ne!(copy, PPC_MAIN_CTABLE_HANDLE);
    assert_eq!(handles[0].size, PPC_MAIN_CTABLE_SIZE);
    assert_eq!(
        ppc_memory_read_bytes(&mut memory, copy_data, PPC_MAIN_CTABLE_SIZE),
        ppc_memory_read_bytes(&mut memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE)
    );
}

#[test]
fn sprocket_trace_formatter_includes_draw_sprocket_state() {
    let mut attrs = PpcDspContextAttributes::default();
    attrs.width = 800;
    attrs.height = 600;
    attrs.page_count = 2;
    let draw_sprocket = PpcDrawSprocketState {
        started: true,
        reserved_context: Some(0x00ab_cdef),
        active_context: Some(0x00ab_cdef),
        context_state: PpcDspContextPlayState::Active,
        context_attributes: attrs,
        last_swap_context: Some(0x00ab_cdef),
        swap_count: 3,
        last_fade_context: Some(0x00ab_cdef),
        last_fade_kind: Some(PpcDspGammaFadeKind::Out),
        last_fade_percent: Some(0),
        last_fade_zero_color: Some(PpcRgbColor {
            red: 0x1111,
            green: 0x2222,
            blue: 0x3333,
        }),
        fade_count: 1,
        ..PpcDrawSprocketState::default()
    };
    let entry = PpcHleImportTraceEntry {
        import_index: 41,
        library_name: "DrawSprocketLib".to_string(),
        symbol_name: "DSpContext_SwapBuffers".to_string(),
        pc: 0x01f0_1000,
        lr: 0x0100_2000,
        rtoc: 0x0200_3000,
        sp: 0x03fe_f000,
        dispatcher_target: PpcImportDispatcherTarget::DSpContextSwapBuffers,
        repeat_count: 1,
    };

    assert_eq!(
        format_sprocket_trace(
            &entry,
            [0x00ab_cdef, 0, 0, 0, 0, 0],
            &format_sprocket_action(&PpcImportAction::Return(0)),
            &draw_sprocket,
            &PpcInputSprocketState::default(),
            &[],
        ),
        "[SPROCKET-TRACE] DrawSprocketLib:DSpContext_SwapBuffers pc=$01F01000 lr=$01002000 rtoc=$02003000 sp=$03FEF000 r3=$00ABCDEF r4=$00000000 r5=$00000000 r6=$00000000 r7=$00000000 r8=$00000000 action=return($00000000) dsp started=true reserved=$00ABCDEF active=$00ABCDEF state=active attrs=800x600 display_depth=16 back_depth=16 pages=2 front=$02F00000 back=$05010000 swaps=3 last_swap=$00ABCDEF fades=1 last_fade=out context=$00ABCDEF percent=0 zero=$1111/$2222/$3333"
    );
}

#[test]
fn sprocket_trace_formatter_includes_input_sprocket_state() {
    let input_sprocket = PpcInputSprocketState {
        initialized: true,
        suspended: false,
        keyboard_active: true,
        mouse_active: false,
        configure_count: 2,
        virtual_element_count: 5,
        last_virtual_need_count: 3,
        last_virtual_needs_ptr: 0x0200_1000,
        last_virtual_elements_out_ptr: 0x0200_2000,
    };
    let entry = PpcHleImportTraceEntry {
        import_index: 51,
        library_name: "InputSprocketLib".to_string(),
        symbol_name: "ISpElement_GetSimpleState".to_string(),
        pc: 0x01f0_1100,
        lr: 0x0100_2100,
        rtoc: 0x0200_3100,
        sp: 0x03fe_efc0,
        dispatcher_target: PpcImportDispatcherTarget::ISpElementGetSimpleState,
        repeat_count: 1,
    };

    assert_eq!(
        format_sprocket_trace(
            &entry,
            [0x0200_3000, 0x0200_4000, 0, 0, 0, 0],
            "return-preserve",
            &PpcDrawSprocketState::default(),
            &input_sprocket,
            &[],
        ),
        "[SPROCKET-TRACE] InputSprocketLib:ISpElement_GetSimpleState pc=$01F01100 lr=$01002100 rtoc=$02003100 sp=$03FEEFC0 r3=$02003000 r4=$02004000 r5=$00000000 r6=$00000000 r7=$00000000 r8=$00000000 action=return-preserve isp initialized=true suspended=false keyboard=true mouse=false virtuals=5 last_need_count=3 last_needs=$02001000 last_elements=$02002000 configure_count=2"
    );
}

#[test]
fn sprocket_trace_formatter_includes_input_sprocket_virtual_bindings_on_creation() {
    let input_sprocket = PpcInputSprocketState {
        initialized: true,
        suspended: false,
        keyboard_active: true,
        mouse_active: true,
        configure_count: 0,
        virtual_element_count: 2,
        last_virtual_need_count: 2,
        last_virtual_needs_ptr: 0x0200_1000,
        last_virtual_elements_out_ptr: 0x0200_2000,
    };
    let virtual_elements = vec![
        PpcInputSprocketVirtualElementRecord {
            element: 0x0300_0000,
            need_index: 0,
            need_source: 0x0200_1000,
            kind: PPC_ISP_ELEMENT_KIND_BUTTON,
            default_state: 0,
            action_binding: PpcInputSprocketActionBinding::ButtonFire,
            need_name: "Fire".to_string(),
            need_record: Vec::new(),
        },
        PpcInputSprocketVirtualElementRecord {
            element: 0x0300_0100,
            need_index: 1,
            need_source: 0x0200_1000 + PPC_ISP_NEED_SIZE,
            kind: PPC_ISP_ELEMENT_KIND_DELTA,
            default_state: 0,
            action_binding: PpcInputSprocketActionBinding::DeltaYaw,
            need_name: "Yaw (Mouse)".to_string(),
            need_record: Vec::new(),
        },
    ];
    let entry = PpcHleImportTraceEntry {
        import_index: 50,
        library_name: "InputSprocketLib".to_string(),
        symbol_name: "ISpElement_NewVirtualFromNeeds".to_string(),
        pc: 0x01f0_1000,
        lr: 0x0100_2000,
        rtoc: 0x0200_3000,
        sp: 0x03fe_f000,
        dispatcher_target: PpcImportDispatcherTarget::ISpElementNewVirtualFromNeeds,
        repeat_count: 1,
    };

    assert_eq!(
        format_sprocket_trace(
            &entry,
            [2, 0x0200_1000, 0x0200_2000, 0, 0, 0],
            "return($00000000)",
            &PpcDrawSprocketState::default(),
            &input_sprocket,
            &virtual_elements,
        ),
        "[SPROCKET-TRACE] InputSprocketLib:ISpElement_NewVirtualFromNeeds pc=$01F01000 lr=$01002000 rtoc=$02003000 sp=$03FEF000 r3=$00000002 r4=$02001000 r5=$02002000 r6=$00000000 r7=$00000000 r8=$00000000 action=return($00000000) isp initialized=true suspended=false keyboard=true mouse=true virtuals=2 last_need_count=2 last_needs=$02001000 last_elements=$02002000 configure_count=0 last_bindings=[#0 button 'Fire'=button/fire,#1 delta 'Yaw (Mouse)'=delta/yaw]"
    );
}

#[test]
fn qd3d_trace_formatter_includes_state_counts_and_latest_records() {
    let before = Qd3dTraceSnapshot {
        objects: 1,
        views: 1,
        latest_object: Some(PpcQ3ObjectRecord {
            object: 0x0500_0000,
            kind: PpcQ3ObjectKind::Generic,
            object_type: 0x1111_2222,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        }),
        ..Qd3dTraceSnapshot::default()
    };
    let after = Qd3dTraceSnapshot {
        objects: 2,
        views: 1,
        submissions: 1,
        completed_frames: 1,
        memory_storages: 1,
        files: 1,
        group_memberships: 2,
        trimeshes: 1,
        draw_contexts: 1,
        textures: 1,
        shaders: 1,
        styles: 1,
        cameras: 1,
        lights: 1,
        latest_object: Some(PpcQ3ObjectRecord {
            object: 0x0500_0010,
            kind: PpcQ3ObjectKind::MemoryStorage,
            object_type: 0x5354_4F52,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0x0200_1000,
            data_size: 32,
        }),
        latest_submission: Some(PpcQ3SubmissionRecord {
            view: 0x0500_0020,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: 0x0200_2000,
            secondary: 0,
        }),
        latest_frame_view: Some(0x0500_0020),
        latest_frame_submissions: 4,
    };
    let entry = PpcHleImportTraceEntry {
        import_index: 61,
        library_name: "QuickDraw\u{2122} 3D".to_string(),
        symbol_name: "Q3TriMesh_Submit".to_string(),
        pc: 0x01f0_1200,
        lr: 0x0100_2200,
        rtoc: 0x0200_3200,
        sp: 0x03fe_ef80,
        dispatcher_target: PpcImportDispatcherTarget::Q3TriMeshSubmit,
        repeat_count: 1,
    };

    assert_eq!(
        format_qd3d_trace(
            &entry,
            [0x0200_2000, 0x0500_0020, 0, 0, 0, 0],
            "return($00000001)",
            &before,
            &after,
        ),
        "[QD3D-TRACE] QuickDraw\u{2122} 3D:Q3TriMesh_Submit pc=$01F01200 lr=$01002200 rtoc=$02003200 sp=$03FEEF80 r3=$02002000 r4=$05000020 r5=$00000000 r6=$00000000 r7=$00000000 r8=$00000000 action=return($00000001) objects=2(+1) views=1(+0) submissions=1(+1) frames=1(+1) storages=1(+1) files=1(+1) groups=2(+2) trimeshes=1(+1) draw_contexts=1(+1) textures=1(+1) shaders=1(+1) styles=1(+1) cameras=1(+1) lights=1(+1) object=$05000010 kind=memory-storage type=$53544F52 data=$02001000/32 submission=trimesh view=$05000020 primary=$02002000 secondary=$00000000 frame_view=$05000020 frame_submissions=4"
    );
}

#[test]
fn qd3d_trace_snapshot_tracks_latest_object_submission_and_frame() {
    let object = PpcQ3ObjectRecord {
        object: 0x0500_0000,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0x0200_1000,
        data_size: 16,
    };
    let submission = PpcQ3SubmissionRecord {
        view: 0x0500_0010,
        kind: PpcQ3SubmissionKind::Object,
        primary: object.object,
        secondary: 0,
    };
    let frame = PpcQ3CompletedFrameRecord {
        view: 0x0500_0010,
        submissions: vec![submission],
        submission_transforms: Vec::new(),
        submission_materials: Vec::new(),
        submission_lights: Vec::new(),
        retained_trimeshes: Vec::new(),
    };

    let snapshot = qd3d_trace_snapshot(
        &[object],
        &[PpcQ3ViewStateRecord::new(0x0500_0010)],
        &[submission],
        &[frame],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
    );

    assert_eq!(snapshot.objects, 1);
    assert_eq!(snapshot.views, 1);
    assert_eq!(snapshot.submissions, 1);
    assert_eq!(snapshot.completed_frames, 1);
    assert_eq!(snapshot.latest_object, Some(object));
    assert_eq!(snapshot.latest_submission, Some(submission));
    assert_eq!(snapshot.latest_frame_view, Some(0x0500_0010));
    assert_eq!(snapshot.latest_frame_submissions, 1);
}

#[test]
fn pef_dump_json_includes_loader_sections_imports_and_entry() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let loaded = load_pef_application(&pef).unwrap();
    let header = parse_pef_header(&pef).unwrap();
    let loader = parse_pef_loader_header(&pef).unwrap();
    let raw_sections = parse_pef_sections(&pef).unwrap();
    let mapped_sections = map_instantiated_sections(&pef).unwrap();
    let reloc_headers = parse_pef_reloc_headers(&pef).unwrap_or_default();
    let report = format_pef_dump_json(&PefDumpContext {
        data_len: pef.len(),
        header,
        loader,
        raw_sections: &raw_sections,
        mapped_sections: &mapped_sections,
        imports: &loaded.imports,
        reloc_headers: &reloc_headers,
        entry_pc: loaded.entry_pc,
        rtoc: loaded.rtoc,
        stack_base: loaded.stack_base,
        stack_size: loaded.stack_size,
        stack_top: PPC_STACK_TOP,
    });

    assert!(report.contains("\"format\": \"systemless_pef_dump_v1\""));
    assert!(report.contains("\"architecture\": \"pwpc\""));
    assert!(report.contains("\"kind\": \"code\""));
    assert!(report.contains("\"mapped_base\": \"0x01000000\""));
    assert!(report.contains("\"library\": \"InterfaceLib\""));
    assert!(report.contains("\"symbol\": \"NewPtrClear\""));
    assert!(report.contains("\"dispatcher_target\": \"NewPtr { clear: true }\""));
    assert!(report.contains("\"entry_pc\": \"0x01000000\""));
    assert!(report.contains("\"rtoc\": \"0x02000000\""));
    assert!(report.contains("\"base\": \"0x04FF0000\""));
}

#[test]
fn load_pef_application_reports_detailed_relocation_apply_failure() {
    let pef = synthetic_pef_with_reloc_chunks(
        b"InterfaceLib",
        b"OnlyImport",
        &[delt(8), sm_index_reloc(0x30, 0)],
    );

    let error = load_pef_application(&pef).unwrap_err();

    assert_eq!(
        error,
        PpcLoadError::RelocationApply {
            section_index: 1,
            reloc_instr_offset: 2,
            section_position: 8,
            import_index: Some(0),
            import_symbol: Some(PpcRelocationImportSymbol {
                symbol_index: 0,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "OnlyImport".to_string(),
                class: 2,
                weak: false,
            }),
            error: PefRelocApplyError::OutOfRange {
                position: 8,
                section_len: 8,
            },
        }
    );
}

#[test]
fn load_pef_application_seeds_ppc_cpu_and_reaches_import_trace() {
    let pef = synthetic_pef();
    let mut loaded = load_pef_application(&pef).unwrap();

    assert_eq!(loaded.entry_pc, PPC_CODE_BASE);
    assert_eq!(loaded.rtoc, PPC_DATA_BASE);
    assert_eq!(loaded.cpu.pc, PPC_CODE_BASE);
    assert_eq!(loaded.cpu.gpr[1], PPC_STACK_TOP - 64);
    assert_eq!(loaded.stack_base, PPC_STACK_BASE);
    assert_eq!(loaded.stack_size, PPC_DEFAULT_STACK_SIZE);
    assert_eq!(loaded.heap_base(), PPC_HEAP_BASE);
    assert_eq!(loaded.heap_cursor(), PPC_HEAP_BASE);
    assert_eq!(test_heap_limit!(loaded), PPC_STACK_BASE);
    assert_eq!(loaded.last_mem_error(), 0);
    assert_eq!(*loaded.process_file_system.current_resource_file, 0);
    assert_eq!(loaded.test_resource_error(), 0);
    assert_eq!(loaded.cfm.as_ref().unwrap().connections.len(), 0);
    assert_eq!(
        loaded.cfm.as_ref().unwrap().next_connection_id,
        PPC_FIRST_CFM_CONNECTION_ID
    );
    assert_eq!(test_handle_records!(loaded).len(), 0);
    assert_eq!(loaded.gworlds.len(), 2);
    assert_eq!(loaded.gworlds[0].port, PPC_MAIN_GWORLD);
    assert_eq!(loaded.gworlds[0].pixmap_handle, PPC_MAIN_PIXMAP_HANDLE);
    assert_eq!(loaded.gworlds[0].base_addr, PPC_MAIN_SCREEN_BASE);
    assert_eq!(
        loaded.memory.read_u16_be(PPC_MAIN_GDEVICE_RECORD + 4),
        Some(0),
        "the default indexed display is a clutType GDevice"
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_GDEVICE_RECORD + 42),
        Some(0x83),
        "the default 8-bit display publishes the classic eightBitMode token"
    );
    assert_eq!(loaded.gworlds[1].port, PPC_DSP_BACK_GWORLD);
    assert_eq!(loaded.gworlds[1].pixmap_handle, PPC_DSP_BACK_PIXMAP_HANDLE);
    assert_eq!(loaded.gworlds[1].base_addr, PPC_DSP_BACK_SCREEN_BASE);
    assert_eq!(loaded.draw_sprocket.front_buffer_gworld, PPC_MAIN_GWORLD);
    assert_eq!(loaded.draw_sprocket.back_buffer_gworld, PPC_DSP_BACK_GWORLD);
    assert_eq!(loaded.q3_objects.len(), 0);
    assert_eq!(loaded.next_q3_object, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_error_state, PpcQ3ErrorState::default());
    assert_eq!(loaded.q3_lifecycle, PpcQ3LifecycleState::default());
    assert_eq!(loaded.q3_memory_storages.len(), 0);
    assert_eq!(loaded.q3_files.len(), 0);
    assert_eq!(loaded.q3_group_memberships.len(), 0);
    assert_eq!(loaded.q3_file_groups.len(), 0);
    assert_eq!(loaded.q3_views.len(), 0);
    assert_eq!(loaded.q3_submissions.len(), 0);
    assert_eq!(loaded.q3_view_transforms.len(), 0);
    assert_eq!(loaded.q3_submission_transforms.len(), 0);
    assert_eq!(loaded.q3_view_materials.len(), 0);
    assert_eq!(loaded.q3_submission_materials.len(), 0);
    assert_eq!(loaded.q3_submission_lights.len(), 0);
    assert_eq!(loaded.q3_fog_styles.len(), 0);
    assert_eq!(loaded.q3_attributes.len(), 0);
    assert_eq!(loaded.q3_shader_uv_transforms.len(), 0);
    assert_eq!(loaded.q3_mipmap_textures.len(), 0);
    assert_eq!(loaded.q3_texture_shaders.len(), 0);
    assert_eq!(loaded.q3_trimeshes.len(), 0);
    assert_eq!(loaded.q3_styles.len(), 0);
    assert_eq!(loaded.q3_cameras.len(), 0);
    assert_eq!(loaded.q3_lights.len(), 0);
    assert_eq!(loaded.input_sprocket, PpcInputSprocketState::default());
    assert_eq!(loaded.quicktime, PpcQuickTimeState::default());
    assert_eq!(loaded.sound, PpcSoundState::default());
    assert_eq!(loaded.draw_sprocket, PpcDrawSprocketState::default());
    assert_eq!(loaded.toolbox_startup, PpcToolboxStartupState::default());
    assert_eq!(loaded.files.len(), 0);
    assert_eq!(loaded.vfs_files.len(), 0);
    assert_eq!(loaded.resource_files.len(), 0);
    assert_eq!(loaded.vfs_resource_files.len(), 0);
    assert_eq!(loaded.process_file_system.vfs_resources.len(), 0);
    assert_eq!(loaded.next_file_ref_num, PPC_FIRST_FILE_REF_NUM);
    assert_eq!(
        loaded.sound.manager.default_output_volume(),
        PPC_DEFAULT_OUTPUT_VOLUME
    );
    assert_eq!(*loaded.current_gworld, PPC_MAIN_GWORLD);
    assert_eq!(*loaded.current_gdevice, PPC_MAIN_GDEVICE);
    assert_eq!(loaded.default_dir_id, PPC_ROOT_DIR_ID);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_GWORLD + 2),
        Some(PPC_MAIN_PIXMAP_HANDLE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_PIXMAP_HANDLE),
        Some(PPC_MAIN_PIXMAP)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_PIXMAP),
        Some(PPC_MAIN_SCREEN_BASE)
    );
    assert_eq!(loaded.cpu.gpr[2], PPC_DATA_BASE);
    assert_eq!(loaded.memory.read_u32_be(PPC_CFM_MAIN_STUB_BASE), Some(BLR));
    assert_eq!(
        loaded.memory.read_u32_be(PPC_PORT_LIST_ADDR),
        Some(PPC_PORT_LIST_HANDLE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_PORT_LIST_HANDLE),
        Some(PPC_PORT_LIST)
    );
    assert_eq!(loaded.memory.read_u16_be(PPC_PORT_LIST), Some(0));
    assert_eq!(loaded.memory.read_u32_be(0), Some(0));
    assert!(loaded.memory.write_u32_be(0, 0xdead_beef).is_some());
    assert_eq!(loaded.memory.read_u32_be(0), Some(0xdead_beef));
    assert_eq!(loaded.stack_pointer & 0x0f, 0);
    assert_eq!(loaded.import_count, 1);
    assert_eq!(loaded.imports[0].library_name, "InterfaceLib");
    assert_eq!(loaded.imports[0].symbol_name, "TestImport");
    assert_eq!(loaded.imports[0].class, 2);
    assert!(!loaded.imports[0].weak);
    assert_eq!(loaded.imports[0].address, PPC_IMPORT_TVECTOR_BASE);
    assert_eq!(
        loaded.imports[0].tvector_address,
        Some(PPC_IMPORT_TVECTOR_BASE)
    );
    assert_eq!(loaded.imports[0].trap_pc, PPC_IMPORT_TRAP_BASE);
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::Unsupported
    );

    let (result, trace) = loaded.run_import_trace(64);
    assert!(
        matches!(
            result,
            PpcRunResult::Halted {
                pc: PPC_HALT_PC,
                ..
            }
        ),
        "{result:?}"
    );
    assert_eq!(trace, vec![0]);
}

#[test]
fn import_bindings_preserve_symbol_metadata_and_synthetic_addresses() {
    let bindings = PpcImportBindingPlan::prepare(
        vec![
            PefResolvedImport {
                library_index: 0,
                symbol_index: 0,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "Gestalt".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 1,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "FindSymbol".to_string(),
                class: 0,
                weak: true,
            },
        ],
        2,
        0,
        ppc_import_layout(),
        &SystemlessPpcImportBindingPolicy,
    )
    .unwrap()
    .into_initial_bindings();

    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].library_name, "InterfaceLib");
    assert_eq!(bindings[0].symbol_name, "Gestalt");
    assert_eq!(bindings[0].address, PPC_IMPORT_TVECTOR_BASE);
    assert_eq!(bindings[0].tvector_address, Some(PPC_IMPORT_TVECTOR_BASE));
    assert_eq!(bindings[0].trap_pc, PPC_IMPORT_TRAP_BASE);
    assert!(!bindings[0].weak);

    assert_eq!(bindings[1].symbol_name, "FindSymbol");
    assert_eq!(bindings[1].address, PPC_IMPORT_TRAP_BASE + 4);
    assert_eq!(bindings[1].tvector_address, None);
    assert_eq!(bindings[1].trap_pc, PPC_IMPORT_TRAP_BASE + 4);
    assert!(bindings[1].weak);
    assert_eq!(
        bindings[1].dispatcher_target,
        PpcImportDispatcherTarget::FindSymbol
    );
}

#[test]
fn unavailable_weak_import_relocates_to_cfm_unresolved_symbol_address() {
    let pef = synthetic_pef_with_import_class(b"MissingOptionalAPI", 0x82);
    let mut loaded = load_pef_application(&pef).unwrap();

    assert_eq!(loaded.imports[0].address, 0);
    assert_eq!(loaded.imports[0].tvector_address, None);
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::UnresolvedWeak
    );
    assert_eq!(loaded.memory.read_u32_be(PPC_DATA_BASE), Some(0));
}

#[test]
fn legacy_memory_utility_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("BitClr", PpcLegacyMemoryUtilityOperation::BitClear),
        ("BitNot", PpcLegacyMemoryUtilityOperation::BitNot),
        ("BitSet", PpcLegacyMemoryUtilityOperation::BitSet),
        ("Fix2X", PpcLegacyMemoryUtilityOperation::FixToExtended),
        ("GetMyZone", PpcLegacyMemoryUtilityOperation::GetMyZone),
        ("HandleZone", PpcLegacyMemoryUtilityOperation::HandleZone),
        ("LockMemory", PpcLegacyMemoryUtilityOperation::LockMemory),
        ("MaxBlock", PpcLegacyMemoryUtilityOperation::MaxBlock),
        ("PurgeSpace", PpcLegacyMemoryUtilityOperation::PurgeSpace),
        ("ReserveMem", PpcLegacyMemoryUtilityOperation::ReserveMem),
        ("SetGrowZone", PpcLegacyMemoryUtilityOperation::SetGrowZone),
        ("StackSpace", PpcLegacyMemoryUtilityOperation::StackSpace),
        ("TempFreeMem", PpcLegacyMemoryUtilityOperation::TempFreeMem),
        ("UnlockMemory", PpcLegacyMemoryUtilityOperation::UnlockMemory),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::LegacyMemoryUtility(operation),
            "{symbol}"
        );
    }
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPtrSize"),
        PpcImportDispatcherTarget::SetPtrSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "RecoverHandle"),
        PpcImportDispatcherTarget::RecoverHandle
    );
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
fn hle_import_runner_handles_legacy_bit_utilities() {
    let mut set = load_pef_application(&synthetic_pef_with_import(b"BitSet")).unwrap();
    let byte = PPC_HEAP_BASE;
    set.memory.add_region(byte, vec![0; 2]);
    set.cpu.gpr[3] = byte;
    set.cpu.gpr[4] = 9;
    let set_probe = set.run_with_hle_imports(64);
    assert_eq!(set_probe.unsupported_import_index, None);
    assert_eq!(set.memory.read_u8(byte + 1), Some(0x40));

    let mut clear = load_pef_application(&synthetic_pef_with_import(b"BitClr")).unwrap();
    clear.memory.add_region(byte, vec![0xff; 2]);
    clear.cpu.gpr[3] = byte;
    clear.cpu.gpr[4] = 9;
    let clear_probe = clear.run_with_hle_imports(64);
    assert_eq!(clear_probe.unsupported_import_index, None);
    assert_eq!(clear.memory.read_u8(byte + 1), Some(0xbf));

    let mut not = load_pef_application(&synthetic_pef_with_import(b"BitNot")).unwrap();
    not.cpu.gpr[3] = 0x0f0f_55aa;
    let not_probe = not.run_with_hle_imports(64);
    assert_eq!(not_probe.unsupported_import_index, None);
    assert_eq!(not.cpu.gpr[3], 0xf0f0_aa55);
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
fn reserve_mem_reports_whether_a_contiguous_block_is_available() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ReserveMem")).unwrap();
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = u32::MAX;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
}

#[test]
fn set_ptr_size_grows_only_a_terminal_nonrelocatable_block() {
    let mut memory = PpcSectionMem::new();
    let heap_limit = PPC_HEAP_BASE + 0x1000;
    memory.add_region(PPC_HEAP_BASE, vec![0; 0x1000]);
    let mut manager = ProcessMemoryManager::default();
    manager.publish_native_allocator(
        ProcessNativeHeapState {
            heap_base: PPC_HEAP_BASE,
            heap_cursor: PPC_HEAP_BASE,
            heap_limit,
            last_mem_error: PPC_NO_ERR,
            heap_maximized: false,
            master_pointer_blocks_requested: 0,
        },
        &[],
        &[],
        &[],
    );
    let ptr = manager.new_native_ptr(&mut memory, 8, true);
    assert_ne!(ptr, 0);

    assert_eq!(
        manager.set_native_ptr_size(&mut memory, ptr, 24),
        PPC_NO_ERR
    );
    assert_eq!(manager.native_ptr_size(ptr), 24);
    assert_eq!(memory.read_u8(ptr + 23), Some(0));

    let second = manager.new_native_ptr(&mut memory, 8, true);
    assert_ne!(second, 0);
    assert_eq!(
        manager.set_native_ptr_size(&mut memory, ptr, 32),
        PPC_MEM_FULL_ERR
    );
    assert_eq!(manager.native_ptr_size(ptr), 24);
}


#[test]
fn import_bindings_classify_supported_memory_manager_imports() {
    let bindings = PpcImportBindingPlan::prepare(
        vec![
            PefResolvedImport {
                library_index: 0,
                symbol_index: 0,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "NewPtrClear".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 1,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "DisposePtr".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 2,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "MaxApplZone".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 3,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "MoreMasters".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 4,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "CompactMem".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 5,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "MaxMem".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 6,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "CurResFile".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 7,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "ResError".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 8,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "Gestalt".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 9,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "GetSharedLibrary".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 10,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "FindFolder".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 11,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "DirCreate".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 12,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "FSMakeFSSpec".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 13,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "SndSoundManagerVersion".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 14,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "ParamText".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 15,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "NoteAlert".to_string(),
                class: 2,
                weak: false,
            },
        ],
        16,
        0,
        ppc_import_layout(),
        &SystemlessPpcImportBindingPolicy,
    )
    .unwrap()
    .into_initial_bindings();

    assert_eq!(
        bindings[0].dispatcher_target,
        PpcImportDispatcherTarget::NewPtr { clear: true }
    );
    assert_eq!(
        bindings[1].dispatcher_target,
        PpcImportDispatcherTarget::DisposePtr
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPtrSize"),
        PpcImportDispatcherTarget::GetPtrSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewHandle"),
        PpcImportDispatcherTarget::NewHandle { clear: false }
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewHandleClear"),
        PpcImportDispatcherTarget::NewHandle { clear: true }
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TempNewHandle"),
        PpcImportDispatcherTarget::TempNewHandle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "BlockMove"),
        PpcImportDispatcherTarget::BlockMove
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PtrToHand"),
        PpcImportDispatcherTarget::PtrToHand
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeHandle"),
        PpcImportDispatcherTarget::DisposeHandle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "EmptyHandle"),
        PpcImportDispatcherTarget::EmptyHandle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ReleaseResource"),
        PpcImportDispatcherTarget::ReleaseResource
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DetachResource"),
        PpcImportDispatcherTarget::DetachResource
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetHandleSize"),
        PpcImportDispatcherTarget::GetHandleSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetHandleSize"),
        PpcImportDispatcherTarget::SetHandleSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PurgeMem"),
        PpcImportDispatcherTarget::PurgeMem
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PurgeMemSys"),
        PpcImportDispatcherTarget::PurgeMemSys
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MemError"),
        PpcImportDispatcherTarget::MemError
    );
    assert_eq!(
        bindings[2].dispatcher_target,
        PpcImportDispatcherTarget::MaxApplZone
    );
    assert_eq!(
        bindings[3].dispatcher_target,
        PpcImportDispatcherTarget::MoreMasters
    );
    assert_eq!(
        bindings[4].dispatcher_target,
        PpcImportDispatcherTarget::HeapFreeBytes
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FreeMem"),
        PpcImportDispatcherTarget::HeapFreeBytes
    );
    assert_eq!(
        bindings[5].dispatcher_target,
        PpcImportDispatcherTarget::MaxMem
    );
    assert_eq!(
        bindings[6].dispatcher_target,
        PpcImportDispatcherTarget::CurResFile
    );
    assert_eq!(
        bindings[7].dispatcher_target,
        PpcImportDispatcherTarget::ResError
    );
    assert_eq!(
        bindings[8].dispatcher_target,
        PpcImportDispatcherTarget::Gestalt
    );
    assert_eq!(
        bindings[9].dispatcher_target,
        PpcImportDispatcherTarget::GetSharedLibrary
    );
    assert_eq!(
        bindings[10].dispatcher_target,
        PpcImportDispatcherTarget::FindFolder
    );
    assert_eq!(
        bindings[11].dispatcher_target,
        PpcImportDispatcherTarget::DirCreate
    );
    assert_eq!(
        bindings[12].dispatcher_target,
        PpcImportDispatcherTarget::FSMakeFSSpec
    );
    assert_eq!(
        bindings[13].dispatcher_target,
        PpcImportDispatcherTarget::SndSoundManagerVersion
    );
    assert_eq!(
        bindings[14].dispatcher_target,
        PpcImportDispatcherTarget::ParamText
    );
    assert_eq!(
        bindings[15].dispatcher_target,
        PpcImportDispatcherTarget::AlertReturnDefault
    );
}

#[test]
fn import_bindings_reject_symbol_indexes_outside_import_table() {
    let error = PpcImportBindingPlan::prepare(
        vec![PefResolvedImport {
            library_index: 0,
            symbol_index: 3,
            library_name: "InterfaceLib".to_string(),
            symbol_name: "BadImport".to_string(),
            class: 2,
            weak: false,
        }],
        1,
        0,
        ppc_import_layout(),
        &SystemlessPpcImportBindingPolicy,
    )
    .map_err(ppc_initial_import_error)
    .unwrap_err();

    assert_eq!(
        error,
        PpcLoadError::ImportBindingOutOfRange {
            symbol_index: 3,
            import_count: 1,
        }
    );
}

#[test]
fn initial_import_plan_preserves_reachable_loader_errors() {
    let result = load_pef_application(&synthetic_pef_with_loader(
        synthetic_loader_with_repeated_imports(PPC_IMPORT_CAPACITY + 1),
    ));
    let error = match result {
        Ok(_) => panic!("over-capacity launch unexpectedly returned an app"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        PpcLoadError::ImportCapacityExceeded {
            import_count: PPC_IMPORT_CAPACITY + 1,
            capacity: PPC_IMPORT_CAPACITY,
        }
    );
}

#[test]
fn overlapping_library_ranges_preserve_duplicate_import_projection() {
    let pef = synthetic_pef_with_loader_and_data(
        synthetic_loader_with_overlapping_library_ranges(),
        &[0; 8],
    );
    let resolved = resolve_pef_imports(&pef).unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].symbol_index, 0);
    assert_eq!(resolved[1].symbol_index, 0);
    assert_ne!(resolved[0].library_name, resolved[1].library_name);

    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(loaded.imports.len(), 2);
    let first = loaded.import_binding(0).unwrap();
    assert_eq!(first.library_name, "InterfaceLib");
    assert_eq!(first.address, PPC_IMPORT_TVECTOR_BASE);
    assert_eq!(loaded.imports[1].address, PPC_IMPORT_STD_ERRNO);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(PPC_IMPORT_STD_ERRNO)
    );
}

#[test]
fn import_binding_error_maps_preserve_loader_and_fragment_results() {
    assert_eq!(
        ppc_initial_import_error(PpcImportBindingError::SymbolIndexOutOfRange {
            symbol_index: 3,
            import_count: 1,
        }),
        PpcLoadError::ImportBindingOutOfRange {
            symbol_index: 3,
            import_count: 1,
        }
    );
    assert_eq!(
        ppc_initial_import_error(PpcImportBindingError::CapacityExceeded {
            import_count: 9,
            capacity: 8,
        }),
        PpcLoadError::ImportCapacityExceeded {
            import_count: 9,
            capacity: 8,
        }
    );
    for error in [
        PpcImportBindingError::CountOverflow,
        PpcImportBindingError::BindingAddressOverflow,
        PpcImportBindingError::AddressTableOutOfRange,
    ] {
        assert_eq!(ppc_initial_import_error(error), PpcLoadError::AddressOverflow);
    }
    for (error, expected) in [
        (PpcImportBindingError::CountOverflow, PPC_FRAG_NO_MEM),
        (
            PpcImportBindingError::CapacityExceeded {
                import_count: 9,
                capacity: 8,
            },
            PPC_FRAG_NO_MEM,
        ),
        (PpcImportBindingError::AddressTableOutOfRange, PPC_FRAG_NO_MEM),
        (
            PpcImportBindingError::SymbolIndexOutOfRange {
                symbol_index: 3,
                import_count: 1,
            },
            PPC_FRAG_CORRUPT_ERR,
        ),
        (
            PpcImportBindingError::BindingAddressOverflow,
            PPC_FRAG_CORRUPT_ERR,
        ),
        (PpcImportBindingError::RegistryChanged, PPC_FRAG_CORRUPT_ERR),
    ] {
        assert_eq!(ppc_dynamic_import_error(error), expected);
    }
}

#[test]
fn import_bindings_classify_quickdraw_3d_initializer() {
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Initialize"),
        PpcImportDispatcherTarget::Q3Initialize
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Exit"),
        PpcImportDispatcherTarget::Q3Exit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MemoryStorage_New"),
        PpcImportDispatcherTarget::Q3MemoryStorageNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MemoryStorage_NewBuffer"),
        PpcImportDispatcherTarget::Q3MemoryStorageNewBuffer
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3FSSpecStorage_New"),
        PpcImportDispatcherTarget::Q3FSSpecStorageNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3File_New"),
        PpcImportDispatcherTarget::Q3FileNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_New"),
        PpcImportDispatcherTarget::Q3ViewNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3DisplayGroup_New"),
        PpcImportDispatcherTarget::Q3DisplayGroupNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MipmapTexture_New"),
        PpcImportDispatcherTarget::Q3MipmapTextureNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3TextureShader_New"),
        PpcImportDispatcherTarget::Q3TextureShaderNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3LambertIllumination_New"),
        PpcImportDispatcherTarget::Q3LambertIlluminationNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3NULLIllumination_New"),
        PpcImportDispatcherTarget::Q3NullIlluminationNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3PhongIllumination_New"),
        PpcImportDispatcherTarget::Q3PhongIlluminationNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Renderer_NewFromType"),
        PpcImportDispatcherTarget::Q3RendererNewFromType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Renderer_GetType"),
        PpcImportDispatcherTarget::Q3RendererGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Renderer_Sync"),
        PpcImportDispatcherTarget::Q3RendererSync
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Renderer_Flush"),
        PpcImportDispatcherTarget::Q3RendererFlush
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3InteractiveRenderer_SetDoubleBufferBypass"
        ),
        PpcImportDispatcherTarget::Q3InteractiveRendererSetDoubleBufferBypass
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3InteractiveRenderer_SetPreferences"
        ),
        PpcImportDispatcherTarget::Q3InteractiveRendererSetPreferences
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3InteractiveRenderer_SetRAVEContextHints"
        ),
        PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveContextHints
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3InteractiveRenderer_GetRAVEContextHints"
        ),
        PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveContextHints
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3InteractiveRenderer_GetRAVEDrawContexts"
        ),
        PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveDrawContexts
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3InteractiveRenderer_SetRAVETextureFilter"
        ),
        PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveTextureFilter
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3PixmapDrawContext_New"),
        PpcImportDispatcherTarget::Q3PixmapDrawContextNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MacDrawContext_New"),
        PpcImportDispatcherTarget::Q3MacDrawContextNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3ViewAngleAspectCamera_New"),
        PpcImportDispatcherTarget::Q3ViewAngleAspectCameraNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3OrthographicCamera_New"),
        PpcImportDispatcherTarget::Q3OrthographicCameraNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3ViewPlaneCamera_New"),
        PpcImportDispatcherTarget::Q3ViewPlaneCameraNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3BackfacingStyle_New"),
        PpcImportDispatcherTarget::Q3BackfacingStyleNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3InterpolationStyle_New"),
        PpcImportDispatcherTarget::Q3InterpolationStyleNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3FillStyle_New"),
        PpcImportDispatcherTarget::Q3FillStyleNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3OrientationStyle_New"),
        PpcImportDispatcherTarget::Q3OrientationStyleNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Object_Dispose"),
        PpcImportDispatcherTarget::Q3ObjectDispose
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Object_Duplicate"),
        PpcImportDispatcherTarget::Q3ObjectDuplicate
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shared_GetReference"),
        PpcImportDispatcherTarget::Q3SharedGetReference
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shared_IsReferenced"),
        PpcImportDispatcherTarget::Q3SharedIsReferenced
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shared_GetType"),
        PpcImportDispatcherTarget::Q3SharedGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shape_GetType"),
        PpcImportDispatcherTarget::Q3ShapeGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shape_GetLeafType"),
        PpcImportDispatcherTarget::Q3ShapeGetLeafType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Error_Get"),
        PpcImportDispatcherTarget::Q3ErrorGet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Object_IsDrawable"),
        PpcImportDispatcherTarget::Q3ObjectIsDrawable
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Object_IsType"),
        PpcImportDispatcherTarget::Q3ObjectIsType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Object_GetType"),
        PpcImportDispatcherTarget::Q3ObjectGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Object_GetLeafType"),
        PpcImportDispatcherTarget::Q3ObjectGetLeafType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Geometry_GetType"),
        PpcImportDispatcherTarget::Q3GeometryGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shader_GetType"),
        PpcImportDispatcherTarget::Q3ShaderGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Group_GetType"),
        PpcImportDispatcherTarget::Q3GroupGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3TextureShader_GetTexture"),
        PpcImportDispatcherTarget::Q3TextureShaderGetTexture
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MipmapTexture_GetMipmap"),
        PpcImportDispatcherTarget::Q3MipmapTextureGetMipmap
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Storage_GetType"),
        PpcImportDispatcherTarget::Q3StorageGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3DrawContext_GetPane"),
        PpcImportDispatcherTarget::Q3DrawContextGetPane
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MemoryStorage_Set"),
        PpcImportDispatcherTarget::Q3MemoryStorageSet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MemoryStorage_GetBuffer"),
        PpcImportDispatcherTarget::Q3MemoryStorageGetBuffer
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MemoryStorage_SetBuffer"),
        PpcImportDispatcherTarget::Q3MemoryStorageSetBuffer
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MemoryStorage_GetType"),
        PpcImportDispatcherTarget::Q3MemoryStorageGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Camera_GetPlacement"),
        PpcImportDispatcherTarget::Q3CameraGetPlacement
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Camera_SetPlacement"),
        PpcImportDispatcherTarget::Q3CameraSetPlacement
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Camera_GetRange"),
        PpcImportDispatcherTarget::Q3CameraGetRange
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Camera_SetRange"),
        PpcImportDispatcherTarget::Q3CameraSetRange
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Camera_GetViewPort"),
        PpcImportDispatcherTarget::Q3CameraGetViewPort
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Camera_SetViewPort"),
        PpcImportDispatcherTarget::Q3CameraSetViewPort
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Camera_GetWorldToView"),
        PpcImportDispatcherTarget::Q3CameraGetWorldToView
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Camera_GetViewToFrustum"),
        PpcImportDispatcherTarget::Q3CameraGetViewToFrustum
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Light_GetType"),
        PpcImportDispatcherTarget::Q3LightGetType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Light_GetState"),
        PpcImportDispatcherTarget::Q3LightGetState
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Light_SetState"),
        PpcImportDispatcherTarget::Q3LightSetState
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Light_GetBrightness"),
        PpcImportDispatcherTarget::Q3LightGetBrightness
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Light_SetBrightness"),
        PpcImportDispatcherTarget::Q3LightSetBrightness
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Light_GetColor"),
        PpcImportDispatcherTarget::Q3LightGetColor
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Light_SetColor"),
        PpcImportDispatcherTarget::Q3LightSetColor
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Light_GetData"),
        PpcImportDispatcherTarget::Q3LightGetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Light_SetData"),
        PpcImportDispatcherTarget::Q3LightSetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3AmbientLight_New"),
        PpcImportDispatcherTarget::Q3AmbientLightNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3AmbientLight_GetData"),
        PpcImportDispatcherTarget::Q3AmbientLightGetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3AmbientLight_SetData"),
        PpcImportDispatcherTarget::Q3AmbientLightSetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3DirectionalLight_New"),
        PpcImportDispatcherTarget::Q3DirectionalLightNew
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3DirectionalLight_GetCastShadowsState"
        ),
        PpcImportDispatcherTarget::Q3DirectionalLightGetCastShadowsState
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3DirectionalLight_SetCastShadowsState"
        ),
        PpcImportDispatcherTarget::Q3DirectionalLightSetCastShadowsState
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3DirectionalLight_GetDirection"),
        PpcImportDispatcherTarget::Q3DirectionalLightGetDirection
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3DirectionalLight_SetDirection"),
        PpcImportDispatcherTarget::Q3DirectionalLightSetDirection
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3DirectionalLight_GetData"),
        PpcImportDispatcherTarget::Q3DirectionalLightGetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3DirectionalLight_SetData"),
        PpcImportDispatcherTarget::Q3DirectionalLightSetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3PointLight_New"),
        PpcImportDispatcherTarget::Q3PointLightNew
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3PointLight_GetCastShadowsState"
        ),
        PpcImportDispatcherTarget::Q3PointLightGetCastShadowsState
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3PointLight_SetCastShadowsState"
        ),
        PpcImportDispatcherTarget::Q3PointLightSetCastShadowsState
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3PointLight_GetAttenuation"),
        PpcImportDispatcherTarget::Q3PointLightGetAttenuation
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3PointLight_SetAttenuation"),
        PpcImportDispatcherTarget::Q3PointLightSetAttenuation
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3PointLight_GetLocation"),
        PpcImportDispatcherTarget::Q3PointLightGetLocation
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3PointLight_SetLocation"),
        PpcImportDispatcherTarget::Q3PointLightSetLocation
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3PointLight_GetData"),
        PpcImportDispatcherTarget::Q3PointLightGetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3PointLight_SetData"),
        PpcImportDispatcherTarget::Q3PointLightSetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_New"),
        PpcImportDispatcherTarget::Q3SpotLightNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_GetCastShadowsState"),
        PpcImportDispatcherTarget::Q3SpotLightGetCastShadowsState
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_SetCastShadowsState"),
        PpcImportDispatcherTarget::Q3SpotLightSetCastShadowsState
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_GetAttenuation"),
        PpcImportDispatcherTarget::Q3SpotLightGetAttenuation
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_SetAttenuation"),
        PpcImportDispatcherTarget::Q3SpotLightSetAttenuation
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_GetLocation"),
        PpcImportDispatcherTarget::Q3SpotLightGetLocation
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_SetLocation"),
        PpcImportDispatcherTarget::Q3SpotLightSetLocation
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_GetDirection"),
        PpcImportDispatcherTarget::Q3SpotLightGetDirection
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_SetDirection"),
        PpcImportDispatcherTarget::Q3SpotLightSetDirection
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_GetHotAngle"),
        PpcImportDispatcherTarget::Q3SpotLightGetHotAngle
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_SetHotAngle"),
        PpcImportDispatcherTarget::Q3SpotLightSetHotAngle
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_GetOuterAngle"),
        PpcImportDispatcherTarget::Q3SpotLightGetOuterAngle
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_SetOuterAngle"),
        PpcImportDispatcherTarget::Q3SpotLightSetOuterAngle
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_GetFallOff"),
        PpcImportDispatcherTarget::Q3SpotLightGetFallOff
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_SetFallOff"),
        PpcImportDispatcherTarget::Q3SpotLightSetFallOff
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_GetData"),
        PpcImportDispatcherTarget::Q3SpotLightGetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3SpotLight_SetData"),
        PpcImportDispatcherTarget::Q3SpotLightSetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3LightGroup_New"),
        PpcImportDispatcherTarget::Q3LightGroupNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3BackfacingStyle_Get"),
        PpcImportDispatcherTarget::Q3BackfacingStyleGet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3BackfacingStyle_Set"),
        PpcImportDispatcherTarget::Q3BackfacingStyleSet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3InterpolationStyle_Get"),
        PpcImportDispatcherTarget::Q3InterpolationStyleGet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3InterpolationStyle_Set"),
        PpcImportDispatcherTarget::Q3InterpolationStyleSet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3FillStyle_Get"),
        PpcImportDispatcherTarget::Q3FillStyleGet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3FillStyle_Set"),
        PpcImportDispatcherTarget::Q3FillStyleSet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3OrientationStyle_Get"),
        PpcImportDispatcherTarget::Q3OrientationStyleGet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3OrientationStyle_Set"),
        PpcImportDispatcherTarget::Q3OrientationStyleSet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Storage_GetSize"),
        PpcImportDispatcherTarget::Q3StorageGetSize
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Storage_GetData"),
        PpcImportDispatcherTarget::Q3StorageGetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Storage_SetData"),
        PpcImportDispatcherTarget::Q3StorageSetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3TriMesh_New"),
        PpcImportDispatcherTarget::Q3TriMeshNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3TriMesh_GetData"),
        PpcImportDispatcherTarget::Q3TriMeshGetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3TriMesh_SetData"),
        PpcImportDispatcherTarget::Q3TriMeshSetData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3TriMesh_EmptyData"),
        PpcImportDispatcherTarget::Q3TriMeshEmptyData
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3AttributeSet_New"),
        PpcImportDispatcherTarget::Q3AttributeSetNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3AttributeSet_Get"),
        PpcImportDispatcherTarget::Q3AttributeSetGet
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D",
            "Q3AttributeSet_GetNextAttributeType"
        ),
        PpcImportDispatcherTarget::Q3AttributeSetGetNextAttributeType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Vector3D_Normalize"),
        PpcImportDispatcherTarget::Q3Vector3DNormalize
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Vector2D_Normalize"),
        PpcImportDispatcherTarget::Q3Vector2DNormalize
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Vector3D_Cross"),
        PpcImportDispatcherTarget::Q3Vector3DCross
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Point2D_Distance"),
        PpcImportDispatcherTarget::Q3Point2DDistance
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Point3D_Distance"),
        PpcImportDispatcherTarget::Q3Point3DDistance
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Point3D_CrossProductTri"),
        PpcImportDispatcherTarget::Q3Point3DCrossProductTri
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix3x3_SetTranslate"),
        PpcImportDispatcherTarget::Q3Matrix3x3SetTranslate
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_SetIdentity"),
        PpcImportDispatcherTarget::Q3Matrix4x4SetIdentity
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_SetTranslate"),
        PpcImportDispatcherTarget::Q3Matrix4x4SetTranslate
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_SetScale"),
        PpcImportDispatcherTarget::Q3Matrix4x4SetScale
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_SetRotate_X"),
        PpcImportDispatcherTarget::Q3Matrix4x4SetRotateX
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_SetRotate_Y"),
        PpcImportDispatcherTarget::Q3Matrix4x4SetRotateY
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_SetRotate_Z"),
        PpcImportDispatcherTarget::Q3Matrix4x4SetRotateZ
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_SetRotate_XYZ"),
        PpcImportDispatcherTarget::Q3Matrix4x4SetRotateXyz
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_Multiply"),
        PpcImportDispatcherTarget::Q3Matrix4x4Multiply
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_Transpose"),
        PpcImportDispatcherTarget::Q3Matrix4x4Transpose
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Matrix4x4_Invert"),
        PpcImportDispatcherTarget::Q3Matrix4x4Invert
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Point3D_Transform"),
        PpcImportDispatcherTarget::Q3Point3DTransform
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Point3D_To3DTransformArray"),
        PpcImportDispatcherTarget::Q3Point3DTo3DTransformArray
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Point3D_To4DTransformArray"),
        PpcImportDispatcherTarget::Q3Point3DTo4DTransformArray
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Vector3D_Transform"),
        PpcImportDispatcherTarget::Q3Vector3DTransform
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MatrixTransform_New"),
        PpcImportDispatcherTarget::Q3MatrixTransformNew
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MatrixTransform_Set"),
        PpcImportDispatcherTarget::Q3MatrixTransformSet
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Transform_GetMatrix"),
        PpcImportDispatcherTarget::Q3TransformGetMatrix
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shader_GetUVTransform"),
        PpcImportDispatcherTarget::Q3ShaderGetUVTransform
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shader_SetUVTransform"),
        PpcImportDispatcherTarget::Q3ShaderSetUVTransform
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shader_GetUBoundary"),
        PpcImportDispatcherTarget::Q3ShaderGetUBoundary
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shader_SetUBoundary"),
        PpcImportDispatcherTarget::Q3ShaderSetUBoundary
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shader_GetVBoundary"),
        PpcImportDispatcherTarget::Q3ShaderGetVBoundary
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shader_SetVBoundary"),
        PpcImportDispatcherTarget::Q3ShaderSetVBoundary
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3File_SetStorage"),
        PpcImportDispatcherTarget::Q3FileSetStorage
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3File_OpenRead"),
        PpcImportDispatcherTarget::Q3FileOpenRead
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3File_ReadObject"),
        PpcImportDispatcherTarget::Q3FileReadObject
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3File_IsEndOfFile"),
        PpcImportDispatcherTarget::Q3FileIsEndOfFile
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3File_Close"),
        PpcImportDispatcherTarget::Q3FileClose
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Group_AddObject"),
        PpcImportDispatcherTarget::Q3GroupAddObject
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Group_AddObjectBefore"),
        PpcImportDispatcherTarget::Q3GroupAddObjectBefore
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Group_CountObjects"),
        PpcImportDispatcherTarget::Q3GroupCountObjects
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Group_GetFirstPosition"),
        PpcImportDispatcherTarget::Q3GroupGetFirstPosition
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Group_GetNextPosition"),
        PpcImportDispatcherTarget::Q3GroupGetNextPosition
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Group_GetFirstPositionOfType"),
        PpcImportDispatcherTarget::Q3GroupGetFirstPositionOfType
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Group_GetPositionObject"),
        PpcImportDispatcherTarget::Q3GroupGetPositionObject
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Group_RemovePosition"),
        PpcImportDispatcherTarget::Q3GroupRemovePosition
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3AttributeSet_Add"),
        PpcImportDispatcherTarget::Q3AttributeSetAdd
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3AttributeSet_Clear"),
        PpcImportDispatcherTarget::Q3AttributeSetClear
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3AttributeSet_Contains"),
        PpcImportDispatcherTarget::Q3AttributeSetContains
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_SetCamera"),
        PpcImportDispatcherTarget::Q3ViewSetCamera
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_GetCamera"),
        PpcImportDispatcherTarget::Q3ViewGetCamera
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_SetRenderer"),
        PpcImportDispatcherTarget::Q3ViewSetRenderer
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_GetRenderer"),
        PpcImportDispatcherTarget::Q3ViewGetRenderer
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_SetLightGroup"),
        PpcImportDispatcherTarget::Q3ViewSetLightGroup
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_GetLightGroup"),
        PpcImportDispatcherTarget::Q3ViewGetLightGroup
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_SetDrawContext"),
        PpcImportDispatcherTarget::Q3ViewSetDrawContext
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_GetDrawContext"),
        PpcImportDispatcherTarget::Q3ViewGetDrawContext
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_StartRendering"),
        PpcImportDispatcherTarget::Q3ViewStartRendering
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3View_Cancel"),
        PpcImportDispatcherTarget::Q3ViewCancel
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Shader_Submit"),
        PpcImportDispatcherTarget::Q3ShaderSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Style_Submit"),
        PpcImportDispatcherTarget::Q3StyleSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3BackfacingStyle_Submit"),
        PpcImportDispatcherTarget::Q3BackfacingStyleSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3InterpolationStyle_Submit"),
        PpcImportDispatcherTarget::Q3InterpolationStyleSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3FillStyle_Submit"),
        PpcImportDispatcherTarget::Q3FillStyleSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3OrientationStyle_Submit"),
        PpcImportDispatcherTarget::Q3OrientationStyleSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3FogStyle_Submit"),
        PpcImportDispatcherTarget::Q3FogStyleSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3TriMesh_Submit"),
        PpcImportDispatcherTarget::Q3TriMeshSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3MatrixTransform_Submit"),
        PpcImportDispatcherTarget::Q3MatrixTransformSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3ResetTransform_Submit"),
        PpcImportDispatcherTarget::Q3ResetTransformSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Push_Submit"),
        PpcImportDispatcherTarget::Q3PushSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Pop_Submit"),
        PpcImportDispatcherTarget::Q3PopSubmit
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Object_Submit"),
        PpcImportDispatcherTarget::Q3ObjectSubmit
    );
}

#[test]
fn import_bindings_reject_unknown_quickdraw_3d_imports() {
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D", "Q3Unmodeled_ReturnSuccess"),
        PpcImportDispatcherTarget::Unsupported
    );
}

#[test]
fn import_bindings_classify_quickdraw_3d_accelerator_imports() {
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D Accelerator",
            "QADeviceGetFirstEngine"
        ),
        PpcImportDispatcherTarget::QADeviceGetFirstEngine
    );
    assert_eq!(
        dispatcher_target_for_import(
            "QuickDraw\u{2122} 3D Accelerator",
            "QADeviceGetNextEngine"
        ),
        PpcImportDispatcherTarget::QADeviceGetNextEngine
    );
    assert_eq!(
        dispatcher_target_for_import("QuickDraw\u{2122} 3D Accelerator", "QAEngineGestalt"),
        PpcImportDispatcherTarget::QAEngineGestalt
    );
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
fn import_bindings_classify_picture_bootstrap_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPicture"),
        PpcImportDispatcherTarget::GetPicture
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPixPat"),
        PpcImportDispatcherTarget::GetPixPat
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPictInfo"),
        PpcImportDispatcherTarget::GetPictInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DrawPicture"),
        PpcImportDispatcherTarget::DrawPicture
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "KillPicture"),
        PpcImportDispatcherTarget::KillPicture
    );
}

#[test]
fn import_bindings_classify_gworld_state_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPort"),
        PpcImportDispatcherTarget::GetPort
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetWMgrPort"),
        PpcImportDispatcherTarget::GetWMgrPort
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetCWMgrPort"),
        PpcImportDispatcherTarget::GetWMgrPort
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPort"),
        PpcImportDispatcherTarget::SetPort
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetGDevice"),
        PpcImportDispatcherTarget::GetGDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetGDevice"),
        PpcImportDispatcherTarget::SetGDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDeviceList"),
        PpcImportDispatcherTarget::GetDeviceList
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetNextDevice"),
        PpcImportDispatcherTarget::GetNextDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetMainDevice"),
        PpcImportDispatcherTarget::GetMainDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetMBarHeight"),
        PpcImportDispatcherTarget::GetMBarHeight
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TestDeviceAttribute"),
        PpcImportDispatcherTarget::TestDeviceAttribute
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HasDepth"),
        PpcImportDispatcherTarget::HasDepth
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetDepth"),
        PpcImportDispatcherTarget::SetDepth
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DMGetGDeviceByDisplayID"),
        PpcImportDispatcherTarget::DMGetGDeviceByDisplayID
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewCWindow"),
        PpcImportDispatcherTarget::NewCWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetNewCWindow"),
        PpcImportDispatcherTarget::GetNewCWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetWRefCon"),
        PpcImportDispatcherTarget::GetWRefCon
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetWRefCon"),
        PpcImportDispatcherTarget::SetWRefCon
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SizeWindow"),
        PpcImportDispatcherTarget::SizeWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MoveWindow"),
        PpcImportDispatcherTarget::MoveWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ShowWindow"),
        PpcImportDispatcherTarget::ShowWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HideWindow"),
        PpcImportDispatcherTarget::HideWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ShowHide"),
        PpcImportDispatcherTarget::ShowHide
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CloseWindow"),
        PpcImportDispatcherTarget::CloseWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SelectWindow"),
        PpcImportDispatcherTarget::SelectWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetWinColor"),
        PpcImportDispatcherTarget::SetWinColor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CalcVisBehind"),
        PpcImportDispatcherTarget::CalcVisBehind
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PaintBehind"),
        PpcImportDispatcherTarget::PaintBehind
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PaintOne"),
        PpcImportDispatcherTarget::PaintOne
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ActivatePalette"),
        PpcImportDispatcherTarget::ActivatePalette
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPalette"),
        PpcImportDispatcherTarget::NSetPalette
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NSetPalette"),
        PpcImportDispatcherTarget::NSetPalette
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPalette"),
        PpcImportDispatcherTarget::GetPalette
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewGWorld"),
        PpcImportDispatcherTarget::NewGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeGWorld"),
        PpcImportDispatcherTarget::DisposeGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetGWorld"),
        PpcImportDispatcherTarget::GetGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetGWorld"),
        PpcImportDispatcherTarget::SetGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetGWorldDevice"),
        PpcImportDispatcherTarget::GetGWorldDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetGWorldPixMap"),
        PpcImportDispatcherTarget::GetGWorldPixMap
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPixBaseAddr"),
        PpcImportDispatcherTarget::GetPixBaseAddr
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LockPixels"),
        PpcImportDispatcherTarget::LockPixels
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "UnlockPixels"),
        PpcImportDispatcherTarget::UnlockPixels
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPixelsState"),
        PpcImportDispatcherTarget::GetPixelsState
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPixelsState"),
        PpcImportDispatcherTarget::SetPixelsState
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AllowPurgePixels"),
        PpcImportDispatcherTarget::AllowPurgePixels
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NoPurgePixels"),
        PpcImportDispatcherTarget::NoPurgePixels
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CopyBits"),
        PpcImportDispatcherTarget::CopyBits
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "BitMapToRegion"),
        PpcImportDispatcherTarget::BitMapToRegion
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPenState"),
        PpcImportDispatcherTarget::GetPenState
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPenState"),
        PpcImportDispatcherTarget::SetPenState
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewRgn"),
        PpcImportDispatcherTarget::NewRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeRgn"),
        PpcImportDispatcherTarget::DisposeRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "OpenRgn"),
        PpcImportDispatcherTarget::OpenRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CloseRgn"),
        PpcImportDispatcherTarget::CloseRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetEmptyRgn"),
        PpcImportDispatcherTarget::SetEmptyRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetRectRgn"),
        PpcImportDispatcherTarget::SetRectRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "RectRgn"),
        PpcImportDispatcherTarget::RectRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "EmptyRgn"),
        PpcImportDispatcherTarget::EmptyRgn
    );
}

#[test]
fn import_bindings_classify_rect_utility_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetRect"),
        PpcImportDispatcherTarget::SetRect
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "UnionRect"),
        PpcImportDispatcherTarget::UnionRect
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPt"),
        PpcImportDispatcherTarget::SetPt
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LocalToGlobal"),
        PpcImportDispatcherTarget::LocalToGlobal
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GlobalToLocal"),
        PpcImportDispatcherTarget::GlobalToLocal
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PtInRect"),
        PpcImportDispatcherTarget::PtInRect
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "OffsetRect"),
        PpcImportDispatcherTarget::OffsetRect
    );
}

#[test]
fn import_bindings_classify_quickdraw_bootstrap_noops() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "InitCursor"),
        PpcImportDispatcherTarget::InitCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HideCursor"),
        PpcImportDispatcherTarget::HideCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ShowCursor"),
        PpcImportDispatcherTarget::ShowCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ShieldCursor"),
        PpcImportDispatcherTarget::ShieldCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetCursor"),
        PpcImportDispatcherTarget::GetCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetCursor"),
        PpcImportDispatcherTarget::SetCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetCCursor"),
        PpcImportDispatcherTarget::GetCCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetCIcon"),
        PpcImportDispatcherTarget::GetCIcon
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PlotCIcon"),
        PpcImportDispatcherTarget::PlotCIcon
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeCIcon"),
        PpcImportDispatcherTarget::DisposeCIcon
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetCCursor"),
        PpcImportDispatcherTarget::SetCCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeCCursor"),
        PpcImportDispatcherTarget::DisposeCCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetForeColor"),
        PpcImportDispatcherTarget::GetForeColor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetBackColor"),
        PpcImportDispatcherTarget::GetBackColor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ForeColor"),
        PpcImportDispatcherTarget::ForeColor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "BackColor"),
        PpcImportDispatcherTarget::BackColor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "RGBForeColor"),
        PpcImportDispatcherTarget::RGBForeColor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "RGBBackColor"),
        PpcImportDispatcherTarget::RGBBackColor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Color2Index"),
        PpcImportDispatcherTarget::Color2Index
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Index2Color"),
        PpcImportDispatcherTarget::Index2Color
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MoveTo"),
        PpcImportDispatcherTarget::MoveTo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LineTo"),
        PpcImportDispatcherTarget::LineTo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DrawChar"),
        PpcImportDispatcherTarget::DrawChar
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DrawString"),
        PpcImportDispatcherTarget::DrawString
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DrawText"),
        PpcImportDispatcherTarget::DrawText
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TextMode"),
        PpcImportDispatcherTarget::TextMode
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TextSize"),
        PpcImportDispatcherTarget::TextSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PaintRect"),
        PpcImportDispatcherTarget::PaintRect
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "EraseRect"),
        PpcImportDispatcherTarget::EraseRect
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "InvertRect"),
        PpcImportDispatcherTarget::InvertRect
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FrameRect"),
        PpcImportDispatcherTarget::FrameRect
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "InvalRect"),
        PpcImportDispatcherTarget::InvalRect
    );
}

#[test]
fn import_bindings_classify_classic_quickdraw_shape_imports() {
    for (symbol, target) in [
        ("Move", PpcImportDispatcherTarget::Move),
        ("Line", PpcImportDispatcherTarget::Line),
        ("GetPen", PpcImportDispatcherTarget::GetPen),
        ("HidePen", PpcImportDispatcherTarget::HidePen),
        ("ShowPen", PpcImportDispatcherTarget::ShowPen),
        ("GetClip", PpcImportDispatcherTarget::GetClip),
        ("SetClip", PpcImportDispatcherTarget::SetClip),
        ("FillRect", PpcImportDispatcherTarget::FillRect),
        ("FrameOval", PpcImportDispatcherTarget::FrameOval),
        ("PaintOval", PpcImportDispatcherTarget::PaintOval),
        ("EraseOval", PpcImportDispatcherTarget::EraseOval),
        ("PaintArc", PpcImportDispatcherTarget::PaintArc),
        ("FrameRgn", PpcImportDispatcherTarget::FrameRgn),
        ("PaintRgn", PpcImportDispatcherTarget::PaintRgn),
        ("FillRgn", PpcImportDispatcherTarget::FillRgn),
        ("InvertRgn", PpcImportDispatcherTarget::InvertRgn),
        ("PtInRgn", PpcImportDispatcherTarget::PtInRgn),
        ("RectInRgn", PpcImportDispatcherTarget::RectInRgn),
        ("OpenPoly", PpcImportDispatcherTarget::OpenPoly),
        ("ClosePoly", PpcImportDispatcherTarget::ClosePoly),
        ("KillPoly", PpcImportDispatcherTarget::KillPoly),
        ("PaintPoly", PpcImportDispatcherTarget::PaintPoly),
        ("FramePoly", PpcImportDispatcherTarget::FramePoly),
        ("FillPoly", PpcImportDispatcherTarget::FillPoly),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
}

#[test]
fn import_bindings_classify_resource_cleanup_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ReleaseResource"),
        PpcImportDispatcherTarget::ReleaseResource
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DetachResource"),
        PpcImportDispatcherTarget::DetachResource
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HLock"),
        PpcImportDispatcherTarget::HLock
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HLockHi"),
        PpcImportDispatcherTarget::HLockHi
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HUnlock"),
        PpcImportDispatcherTarget::HUnlock
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MoveHHi"),
        PpcImportDispatcherTarget::MoveHHi
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HNoPurge"),
        PpcImportDispatcherTarget::HNoPurge
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HPurge"),
        PpcImportDispatcherTarget::HPurge
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TickCount"),
        PpcImportDispatcherTarget::TickCount
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetZone"),
        PpcImportDispatcherTarget::GetZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetZone"),
        PpcImportDispatcherTarget::SetZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "InitZone"),
        PpcImportDispatcherTarget::InitZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SystemZone"),
        PpcImportDispatcherTarget::SystemZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ApplicZone"),
        PpcImportDispatcherTarget::ApplicationZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ApplicationZone"),
        PpcImportDispatcherTarget::ApplicationZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeCTable"),
        PpcImportDispatcherTarget::DisposeCTable
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CloseComponent"),
        PpcImportDispatcherTarget::CloseComponent
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewRoutineDescriptor"),
        PpcImportDispatcherTarget::NewRoutineDescriptor
    );
}

#[test]
fn import_bindings_classify_resource_write_imports() {
    for (symbol, target) in [
        ("AddResource", PpcImportDispatcherTarget::AddResource),
        (
            "ChangedResource",
            PpcImportDispatcherTarget::ChangedResource,
        ),
        ("WriteResource", PpcImportDispatcherTarget::WriteResource),
        ("RemoveResource", PpcImportDispatcherTarget::RemoveResource),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
}

#[test]
fn import_bindings_classify_resource_read_imports() {
    for (symbol, target) in [
        ("OpenResFile", PpcImportDispatcherTarget::OpenResFile),
        ("GetResource", PpcImportDispatcherTarget::GetResource),
        ("Get1Resource", PpcImportDispatcherTarget::Get1Resource),
        (
            "GetNamedResource",
            PpcImportDispatcherTarget::GetNamedResource,
        ),
        (
            "Get1NamedResource",
            PpcImportDispatcherTarget::Get1NamedResource,
        ),
        ("GetIndResource", PpcImportDispatcherTarget::GetIndResource),
        (
            "Get1IndResource",
            PpcImportDispatcherTarget::Get1IndResource,
        ),
        ("GetIndString", PpcImportDispatcherTarget::GetIndString),
        ("getindstring", PpcImportDispatcherTarget::GetIndString),
        ("GetString", PpcImportDispatcherTarget::GetString),
        ("GetResAttrs", PpcImportDispatcherTarget::GetResAttrs),
        ("SetResAttrs", PpcImportDispatcherTarget::SetResAttrs),
        ("GetResInfo", PpcImportDispatcherTarget::GetResInfo),
        ("SetResInfo", PpcImportDispatcherTarget::SetResInfo),
        ("HomeResFile", PpcImportDispatcherTarget::HomeResFile),
        ("CountResources", PpcImportDispatcherTarget::CountResources),
        (
            "Count1Resources",
            PpcImportDispatcherTarget::Count1Resources,
        ),
        ("CountTypes", PpcImportDispatcherTarget::CountTypes),
        ("Count1Types", PpcImportDispatcherTarget::Count1Types),
        ("GetIndType", PpcImportDispatcherTarget::GetIndType),
        ("Get1IndType", PpcImportDispatcherTarget::Get1IndType),
        ("UniqueID", PpcImportDispatcherTarget::UniqueID),
        ("Unique1ID", PpcImportDispatcherTarget::Unique1ID),
        ("UpdateResFile", PpcImportDispatcherTarget::UpdateResFile),
        (
            "ReadPartialResource",
            PpcImportDispatcherTarget::ReadPartialResource,
        ),
        ("HCreateResFile", PpcImportDispatcherTarget::HCreateResFile),
        ("GetIcon", PpcImportDispatcherTarget::GetIcon),
        ("GetPattern", PpcImportDispatcherTarget::GetPattern),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
}

#[test]
fn import_bindings_classify_sound_manager_volume_imports() {
    assert_eq!(
        dispatcher_target_for_import("SoundLib", "UnsignedFixedMulDiv"),
        PpcImportDispatcherTarget::UnsignedFixedMulDiv
    );
    assert_eq!(
        dispatcher_target_for_import("SoundLib", "GetSoundOutputInfo"),
        PpcImportDispatcherTarget::GetSoundOutputInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SysBeep"),
        PpcImportDispatcherTarget::SysBeep
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDefaultOutputVolume"),
        PpcImportDispatcherTarget::GetDefaultOutputVolume
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetDefaultOutputVolume"),
        PpcImportDispatcherTarget::SetDefaultOutputVolume
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SndNewChannel"),
        PpcImportDispatcherTarget::SndNewChannel
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SndDisposeChannel"),
        PpcImportDispatcherTarget::SndDisposeChannel
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SndChannelStatus"),
        PpcImportDispatcherTarget::SndChannelStatus
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SndDoImmediate"),
        PpcImportDispatcherTarget::SndDoImmediate
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SndDoCommand"),
        PpcImportDispatcherTarget::SndDoCommand
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SndPlayDoubleBuffer"),
        PpcImportDispatcherTarget::SndPlayDoubleBuffer
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SndStartFilePlay"),
        PpcImportDispatcherTarget::SndStartFilePlay
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SndPauseFilePlay"),
        PpcImportDispatcherTarget::SndPauseFilePlay
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SndStopFilePlay"),
        PpcImportDispatcherTarget::SndStopFilePlay
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetSoundHeaderOffset"),
        PpcImportDispatcherTarget::GetSoundHeaderOffset
    );
}

#[test]
fn import_bindings_classify_file_manager_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSpOpenDF"),
        PpcImportDispatcherTarget::FSpOpenDF
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSpOpenResFile"),
        PpcImportDispatcherTarget::FSpOpenResFile
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSpCreateResFile"),
        PpcImportDispatcherTarget::FSpCreateResFile
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSpDirCreate"),
        PpcImportDispatcherTarget::FSpDirCreate
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBGetFInfoSync"),
        PpcImportDispatcherTarget::PBGetFInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBHGetFInfo"),
        PpcImportDispatcherTarget::PBHGetFInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBSetFInfoAsync"),
        PpcImportDispatcherTarget::PBSetFInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBHSetFInfoSync"),
        PpcImportDispatcherTarget::PBHSetFInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HGetFInfo"),
        PpcImportDispatcherTarget::HGetFInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HSetFInfo"),
        PpcImportDispatcherTarget::HSetFInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "StandardGetFile"),
        PpcImportDispatcherTarget::StandardGetFile
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetWDInfo"),
        PpcImportDispatcherTarget::GetWDInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBGetCatInfoSync"),
        PpcImportDispatcherTarget::PBGetCatInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBGetCatInfo"),
        PpcImportDispatcherTarget::PBGetCatInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBGetFCBInfoSync"),
        PpcImportDispatcherTarget::PBGetFCBInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetVol"),
        PpcImportDispatcherTarget::GetVol
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HSetVol"),
        PpcImportDispatcherTarget::HSetVol
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HGetVol"),
        PpcImportDispatcherTarget::HGetVol
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FlushVol"),
        PpcImportDispatcherTarget::FlushVol
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBFlushVolSync"),
        PpcImportDispatcherTarget::PBFlushVol
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBSetCatInfoSync"),
        PpcImportDispatcherTarget::PBSetCatInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBSetCatInfo"),
        PpcImportDispatcherTarget::PBSetCatInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSpGetFInfo"),
        PpcImportDispatcherTarget::FSpGetFInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSpSetFInfo"),
        PpcImportDispatcherTarget::FSpSetFInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewAlias"),
        PpcImportDispatcherTarget::NewAlias
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "UpdateAlias"),
        PpcImportDispatcherTarget::UpdateAlias
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ResolveAlias"),
        PpcImportDispatcherTarget::ResolveAlias
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ResolveAliasFile"),
        PpcImportDispatcherTarget::ResolveAliasFile
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSClose"),
        PpcImportDispatcherTarget::FSClose
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBFlushFileSync"),
        PpcImportDispatcherTarget::PBFlushFile
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSRead"),
        PpcImportDispatcherTarget::FSRead
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSWrite"),
        PpcImportDispatcherTarget::FSWrite
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetEOF"),
        PpcImportDispatcherTarget::GetEOF
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetEOF"),
        PpcImportDispatcherTarget::SetEOF
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetFPos"),
        PpcImportDispatcherTarget::GetFPos
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetFPos"),
        PpcImportDispatcherTarget::SetFPos
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSpCreate"),
        PpcImportDispatcherTarget::FSpCreate
    );
    for symbol in ["PBHCreate", "PBHCreateSync", "PBHCreateAsync"] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::PBCreate(
                PpcParameterBlockCreateOperation::Hierarchical
            ),
            "{symbol}"
        );
    }
    for symbol in ["PBCreate", "PBCreateSync", "PBCreateAsync"] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::PBCreate(PpcParameterBlockCreateOperation::Legacy),
            "{symbol}"
        );
    }
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSpDelete"),
        PpcImportDispatcherTarget::FSpDelete
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HDelete"),
        PpcImportDispatcherTarget::DeleteByName(
            PpcDeleteByNameOperation::HierarchicalHighLevel
        )
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FSDelete"),
        PpcImportDispatcherTarget::DeleteByName(PpcDeleteByNameOperation::LegacyHighLevel)
    );
    for symbol in ["PBHDelete", "PBHDeleteSync", "PBHDeleteAsync"] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::DeleteByName(
                PpcDeleteByNameOperation::HierarchicalParameterBlock
            ),
            "{symbol}"
        );
    }
    for symbol in ["PBDelete", "PBDeleteSync", "PBDeleteAsync"] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::DeleteByName(
                PpcDeleteByNameOperation::LegacyParameterBlock
            ),
            "{symbol}"
        );
    }
}

#[test]
fn hle_import_runner_uses_typed_parameter_block_create_operation() {
    let pef = synthetic_pef_with_import(b"PBHCreate");
    let mut loaded = load_pef_application(&pef).unwrap();
    let folder_id = PPC_FIRST_DYNAMIC_DIR_ID;
    loaded.vfs_directories.push(PpcVfsDirectory {
        dir_id: folder_id,
        parent_dir_id: PPC_ROOT_DIR_ID,
        path: "Typed Folder".to_string(),
        creator: PPC_DIRECTORY_CREATOR,
        file_type: PPC_DIRECTORY_FILE_TYPE,
        finder_flags: 0,
        dirty: false,
    });
    let pb = PPC_DATA_BASE + 0x1000;
    let name_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(pb, vec![0; 64]);
    loaded.memory.add_region(name_ptr, vec![0; 64]);
    loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
    loaded
        .memory
        .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
        .unwrap();
    loaded.memory.write_u32_be(pb + 48, folder_id).unwrap();
    loaded.cpu.gpr[3] = pb;

    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::PBCreate(PpcParameterBlockCreateOperation::Hierarchical)
    );
    loaded.imports[0].symbol_name = "PBCreate".to_string();
    write_ppc_pstring(&mut loaded.memory, name_ptr, b"Hierarchical File");
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded
        .vfs_files
        .iter()
        .any(|file| file.path == "Typed Folder/Hierarchical File"));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::PBCreate(PpcParameterBlockCreateOperation::Legacy);
    loaded.imports[0].symbol_name = "PBHCreate".to_string();
    write_ppc_pstring(&mut loaded.memory, name_ptr, b"Legacy File");
    loaded.cpu.gpr[3] = pb;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.vfs_files.iter().any(|file| file.path == "Legacy File"));
    assert!(!loaded
        .vfs_files
        .iter()
        .any(|file| file.path == "Typed Folder/Legacy File"));
}

#[test]
fn hle_import_runner_uses_typed_delete_by_name_operation() {
    let pef = synthetic_pef_with_import(b"PBHDelete");
    let mut loaded = load_pef_application(&pef).unwrap();
    let folder_id = PPC_FIRST_DYNAMIC_DIR_ID;
    loaded.vfs_directories.push(PpcVfsDirectory {
        dir_id: folder_id,
        parent_dir_id: PPC_ROOT_DIR_ID,
        path: "Typed Folder".to_string(),
        creator: PPC_DIRECTORY_CREATOR,
        file_type: PPC_DIRECTORY_FILE_TYPE,
        finder_flags: 0,
        dirty: false,
    });
    for path in ["Victim", "Typed Folder/Victim"] {
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: path.to_string(),
            data: Vec::new().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
    }
    let pb = PPC_DATA_BASE + 0x1000;
    let name_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(pb, vec![0; 64]);
    loaded.memory.add_region(name_ptr, vec![0; 64]);
    loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
    loaded
        .memory
        .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
        .unwrap();
    loaded.memory.write_u32_be(pb + 48, folder_id).unwrap();
    write_ppc_pstring(&mut loaded.memory, name_ptr, b"Victim");
    loaded.cpu.gpr[3] = pb;

    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::DeleteByName(
            PpcDeleteByNameOperation::HierarchicalParameterBlock
        )
    );
    loaded.imports[0].symbol_name = "PBDelete".to_string();
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.vfs_files.iter().any(|file| file.path == "Victim"));
    assert!(!loaded
        .vfs_files
        .iter()
        .any(|file| file.path == "Typed Folder/Victim"));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DeleteByName(
        PpcDeleteByNameOperation::LegacyParameterBlock,
    );
    loaded.imports[0].symbol_name = "PBHDelete".to_string();
    loaded.cpu.gpr[3] = pb;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(!loaded.vfs_files.iter().any(|file| file.path == "Victim"));
}

#[test]
fn file_compatibility_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("OpenDF", PpcFileCompatibilityOperation::OpenDf),
        ("OpenRF", PpcFileCompatibilityOperation::OpenRf),
        ("PBCatSearchSync", PpcFileCompatibilityOperation::PbCatSearchSync),
        ("PBCloseWDSync", PpcFileCompatibilityOperation::PbCloseWdSync),
        ("PBDirCreateSync", PpcFileCompatibilityOperation::PbDirCreateSync),
        ("PBGetFPosSync", PpcFileCompatibilityOperation::PbGetFPosSync),
        ("PBGetWDInfoSync", PpcFileCompatibilityOperation::PbGetWdInfoSync),
        ("PBHGetVolParmsSync", PpcFileCompatibilityOperation::PbHGetVolParmsSync),
        ("PBHGetVolSync", PpcFileCompatibilityOperation::PbHGetVolSync),
        ("PBHOpenRFSync", PpcFileCompatibilityOperation::PbHOpenRfSync),
        ("PBHSetVolSync", PpcFileCompatibilityOperation::PbHSetVolSync),
        ("PBOpenWDSync", PpcFileCompatibilityOperation::PbOpenWdSync),
        ("create", PpcFileCompatibilityOperation::Create),
        ("fsopen", PpcFileCompatibilityOperation::FsOpen),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::FileCompatibility(operation),
        );
    }
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
fn pbhgetvolsync_returns_working_directory_fields_at_wdpb_offsets() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBHGetVolSync"),
        PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbHGetVolSync,
        )
    );
    let pef = synthetic_pef_with_import(b"PBHGetVolSync");
    let mut loaded = load_pef_application(&pef).unwrap();
    let parameter_block = PPC_DATA_BASE + 0x1000;
    let volume_name = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(parameter_block, vec![0xaa; 64]);
    loaded.memory.add_region(volume_name, vec![0; 64]);
    loaded
        .memory
        .write_u32_be(parameter_block + 18, volume_name)
        .unwrap();
    loaded
        .default_dir_id
        .with_mut(|default_dir_id| *default_dir_id = 0x1234_5678);
    loaded.cpu.gpr[3] = parameter_block;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u16_be(parameter_block + 22),
        Some(PPC_BOOT_VOLUME_REF_NUM as u16)
    );
    assert_eq!(
        loaded.memory.read_u16_be(parameter_block + 32),
        Some(PPC_BOOT_VOLUME_REF_NUM as u16)
    );
    assert_eq!(
        loaded.memory.read_u32_be(parameter_block + 48),
        Some(0x1234_5678)
    );
    assert_eq!(
        loaded.memory.read_u32_be(parameter_block + 28),
        Some(0),
        "ioWDProcID is cleared for the current process"
    );
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, volume_name),
        Some(TrapDispatcher::boot_volume_name().as_bytes().to_vec())
    );
}

#[test]
fn native_parameter_block_working_directory_lifecycle_uses_process_registry() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBCloseWDSync"),
        PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbCloseWdSync,
        )
    );
    let pef = synthetic_pef_with_import(b"PBOpenWDSync");
    let mut loaded = load_pef_application(&pef).unwrap();
    let parameter_block = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(parameter_block, vec![0; 64]);
    loaded
        .memory
        .write_u16_be(parameter_block + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
        .unwrap();
    loaded
        .memory
        .write_u32_be(parameter_block + 28, 0x1234_5678)
        .unwrap();
    loaded
        .memory
        .write_u32_be(parameter_block + 48, PPC_PREFERENCES_DIR_ID)
        .unwrap();
    loaded.cpu.gpr[3] = parameter_block;

    let open_probe = loaded.run_with_hle_imports(64);

    assert_eq!(open_probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let wd_ref_num = loaded.memory.read_u16_be(parameter_block + 22).unwrap() as i16;
    assert_ne!(wd_ref_num, PPC_BOOT_VOLUME_REF_NUM);
    assert_eq!(
        loaded.working_directories.get(&wd_ref_num),
        Some(&ProcessWorkingDirectory {
            ref_num: wd_ref_num,
            volume_ref_num: PPC_BOOT_VOLUME_REF_NUM,
            dir_id: PPC_PREFERENCES_DIR_ID,
            proc_id: 0x1234_5678,
        })
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FileCompatibility(
        PpcFileCompatibilityOperation::PbGetWdInfoSync,
    );
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = loaded.halt_pc;
    loaded.cpu.gpr[3] = parameter_block;
    loaded
        .memory
        .write_u16_be(parameter_block + 22, wd_ref_num as u16)
        .unwrap();
    loaded.memory.write_u16_be(parameter_block + 26, 0).unwrap();

    let info_probe = loaded.run_with_hle_imports(64);

    assert_eq!(info_probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u16_be(parameter_block + 32),
        Some(PPC_BOOT_VOLUME_REF_NUM as u16)
    );
    assert_eq!(
        loaded.memory.read_u32_be(parameter_block + 48),
        Some(PPC_PREFERENCES_DIR_ID)
    );
    assert_eq!(
        loaded.memory.read_u32_be(parameter_block + 28),
        Some(0x1234_5678)
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FileCompatibility(
        PpcFileCompatibilityOperation::PbCloseWdSync,
    );
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = loaded.halt_pc;
    loaded.cpu.gpr[3] = parameter_block;
    loaded
        .memory
        .write_u16_be(parameter_block + 22, wd_ref_num as u16)
        .unwrap();

    let close_probe = loaded.run_with_hle_imports(64);

    assert_eq!(close_probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(!loaded.working_directories.contains_key(&wd_ref_num));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = loaded.halt_pc;
    loaded.cpu.gpr[3] = parameter_block;
    loaded
        .memory
        .write_u16_be(parameter_block + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
        .unwrap();

    let volume_close_probe = loaded.run_with_hle_imports(64);

    assert_eq!(volume_close_probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
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
fn sound_input_open_failure_clears_the_output_reference() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"SPBOpenDevice")).unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0xaa; 4]);
    loaded.cpu.gpr[5] = output;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0));
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(PPC_NOT_ENOUGH_HARDWARE_ERR)
    );
}

#[test]
fn quickdraw_compatibility_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("AnimateEntry", PpcQuickDrawCompatibilityOperation::AnimateEntry),
        ("AnimatePalette", PpcQuickDrawCompatibilityOperation::AnimatePalette),
        ("BackPat", PpcQuickDrawCompatibilityOperation::BackPat),
        ("BackPixPat", PpcQuickDrawCompatibilityOperation::BackPixPat),
        ("ClosePicture", PpcQuickDrawCompatibilityOperation::ClosePicture),
        ("CopyDeepMask", PpcQuickDrawCompatibilityOperation::CopyDeepMask),
        ("CopyMask", PpcQuickDrawCompatibilityOperation::CopyMask),
        ("CopyPalette", PpcQuickDrawCompatibilityOperation::CopyPalette),
        ("CTab2Palette", PpcQuickDrawCompatibilityOperation::Ctab2Palette),
        ("DisposeGDevice", PpcQuickDrawCompatibilityOperation::DisposeGDevice),
        ("DisposePalette", PpcQuickDrawCompatibilityOperation::DisposePalette),
        ("Exp1to3", PpcQuickDrawCompatibilityOperation::Exp1To3),
        ("Exp1to6", PpcQuickDrawCompatibilityOperation::Exp1To6),
        ("GetCPixel", PpcQuickDrawCompatibilityOperation::GetCPixel),
        ("GetEntryUsage", PpcQuickDrawCompatibilityOperation::GetEntryUsage),
        ("GetItemIcon", PpcQuickDrawCompatibilityOperation::GetItemIcon),
        ("GetItemStyle", PpcQuickDrawCompatibilityOperation::GetItemStyle),
        ("GetNewPalette", PpcQuickDrawCompatibilityOperation::GetNewPalette),
        ("NewGDevice", PpcQuickDrawCompatibilityOperation::NewGDevice),
        ("NewPalette", PpcQuickDrawCompatibilityOperation::NewPalette),
        ("OpenPicture", PpcQuickDrawCompatibilityOperation::OpenPicture),
        ("Palette2CTab", PpcQuickDrawCompatibilityOperation::Palette2Ctab),
        ("PenPat", PpcQuickDrawCompatibilityOperation::PenPat),
        ("PlotIcon", PpcQuickDrawCompatibilityOperation::PlotIcon),
        ("ScrollRect", PpcQuickDrawCompatibilityOperation::ScrollRect),
        ("SetCPixel", PpcQuickDrawCompatibilityOperation::SetCPixel),
        ("SetEntryColor", PpcQuickDrawCompatibilityOperation::SetEntryColor),
        ("SetEntryUsage", PpcQuickDrawCompatibilityOperation::SetEntryUsage),
        ("SetItemIcon", PpcQuickDrawCompatibilityOperation::SetItemIcon),
        ("SetItemStyle", PpcQuickDrawCompatibilityOperation::SetItemStyle),
        ("SetStdCProcs", PpcQuickDrawCompatibilityOperation::SetStdCProcs),
        ("SetStdProcs", PpcQuickDrawCompatibilityOperation::SetStdProcs),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::QuickDrawCompatibility(operation),
        );
    }
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
fn import_bindings_classify_draw_sprocket_imports() {
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpStartup"),
        PpcImportDispatcherTarget::DSpStartup
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpShutdown"),
        PpcImportDispatcherTarget::DSpShutdown
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpCanUserSelectContext"),
        PpcImportDispatcherTarget::DSpCanUserSelectContext
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpGetMouse"),
        PpcImportDispatcherTarget::DSpGetMouse
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpFindContextFromPoint"),
        PpcImportDispatcherTarget::DSpFindContextFromPoint
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_GlobalToLocal"),
        PpcImportDispatcherTarget::DSpContextGlobalToLocal
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpFindBestContext"),
        PpcImportDispatcherTarget::DSpFindBestContext
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpUserSelectContext"),
        PpcImportDispatcherTarget::DSpUserSelectContext
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpSetBlankingColor"),
        PpcImportDispatcherTarget::DSpSetBlankingColor
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpAltBuffer_New"),
        PpcImportDispatcherTarget::DSpAltBufferNew
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpAltBuffer_GetCGrafPtr"),
        PpcImportDispatcherTarget::DSpAltBufferGetCGrafPtr
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_Reserve"),
        PpcImportDispatcherTarget::DSpContextReserve
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_Release"),
        PpcImportDispatcherTarget::DSpContextRelease
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_SetState"),
        PpcImportDispatcherTarget::DSpContextSetState
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_FadeGamma"),
        PpcImportDispatcherTarget::DSpContextFadeGamma
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_FadeGammaIn"),
        PpcImportDispatcherTarget::DSpContextFadeGammaIn
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_FadeGammaOut"),
        PpcImportDispatcherTarget::DSpContextFadeGammaOut
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_GetFrontBuffer"),
        PpcImportDispatcherTarget::DSpContextGetFrontBuffer
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_GetBackBuffer"),
        PpcImportDispatcherTarget::DSpContextGetBackBuffer
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_SwapBuffers"),
        PpcImportDispatcherTarget::DSpContextSwapBuffers
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_GetDisplayID"),
        PpcImportDispatcherTarget::DSpContextGetDisplayID
    );
    assert_eq!(
        dispatcher_target_for_import("DrawSprocketLib", "DSpContext_GetAttributes"),
        PpcImportDispatcherTarget::DSpContextGetAttributes
    );
}

#[test]
fn import_bindings_classify_input_sprocket_imports() {
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpElement_NewVirtualFromNeeds"),
        PpcImportDispatcherTarget::ISpElementNewVirtualFromNeeds
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevices_Extract"),
        PpcImportDispatcherTarget::ISpDevicesExtract
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevices_ExtractByClass"),
        PpcImportDispatcherTarget::ISpDevicesExtractByClass
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevice_GetElementList"),
        PpcImportDispatcherTarget::ISpDeviceGetElementList
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpElementList_Extract"),
        PpcImportDispatcherTarget::ISpElementListExtract
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpElement_GetInfo"),
        PpcImportDispatcherTarget::ISpElementGetInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpElement_GetSimpleState"),
        PpcImportDispatcherTarget::ISpElementGetSimpleState
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpInit"),
        PpcImportDispatcherTarget::ISpInit
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpStop"),
        PpcImportDispatcherTarget::ISpStop
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpSuspend"),
        PpcImportDispatcherTarget::ISpSuspend
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpResume"),
        PpcImportDispatcherTarget::ISpResume
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevices_Activate"),
        PpcImportDispatcherTarget::ISpDevicesActivate
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevices_Deactivate"),
        PpcImportDispatcherTarget::ISpDevicesDeactivate
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpConfigure"),
        PpcImportDispatcherTarget::ISpConfigure
    );
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
fn hle_import_runner_handles_new_ptr_clear_and_continues() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 24;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.last_import_index, Some(0));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 8,
        }
    );
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE);
    assert_eq!(loaded.last_mem_error(), 0);
    assert_eq!(loaded.cpu.gpr[3] & (PPC_HEAP_ALIGNMENT - 1), 0);
    assert_eq!(
        loaded.heap_cursor(),
        PPC_HEAP_BASE + ppc_allocation_size(24).unwrap()
    );
    for offset in 0..24 {
        assert_eq!(loaded.memory.read_u8(PPC_HEAP_BASE + offset), Some(0));
    }
    assert_eq!(
        loaded.ptrs(),
        vec![PpcPtrRecord {
            ptr: PPC_HEAP_BASE,
            size: 24
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetPtrSize;
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 24);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposePtr;
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.ptrs().is_empty());
    assert_eq!(
        loaded.free_ptr_blocks(),
        vec![PpcPtrRecord {
            ptr: PPC_HEAP_BASE,
            size: 24
        }]
    );
    let heap_cursor = loaded.heap_cursor();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NewPtr { clear: true };
    loaded.cpu.gpr[3] = 12;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(loaded.free_ptr_blocks().is_empty());
    assert_eq!(loaded.ptrs()[0].size, 12);
}

#[test]
fn hle_import_runner_ptr_to_hand_copies_bytes_into_a_new_handle() {
    let pef = synthetic_pef_with_import(b"PtrToHand");
    let mut loaded = load_pef_application(&pef).unwrap();
    let source_ptr = PPC_DATA_BASE + 0x1000;
    let destination_handle_ptr = source_ptr + 16;
    loaded.memory.add_region(source_ptr, vec![0; 32]);
    for (offset, byte) in b"Gridz".iter().copied().enumerate() {
        loaded
            .memory
            .write_u8(source_ptr + offset as u32, byte)
            .unwrap();
    }
    loaded.cpu.gpr[3] = source_ptr;
    loaded.cpu.gpr[4] = destination_handle_ptr;
    loaded.cpu.gpr[5] = 5;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let handle = loaded.memory.read_u32_be(destination_handle_ptr).unwrap();
    assert_ne!(handle, 0);
    assert_eq!(
        ppc_handle_bytes(&mut loaded.memory, &test_handle_records!(loaded), handle),
        Some(b"Gridz".to_vec())
    );
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
fn ppc_heap_allocation_skips_shared_system_reservation() {
    let mut memory = PpcSectionMem::new();
    let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
    let (reservation_base, reservation) = bus.shared_synthetic_reservation().unwrap();
    let reservation_len = u32::try_from(reservation.len()).unwrap();
    let local_base = reservation_base - PPC_HEAP_ALIGNMENT;
    memory.add_region(
        local_base,
        vec![0xff; usize::try_from(reservation_len + 3 * PPC_HEAP_ALIGNMENT).unwrap()],
    );
    bus.write_byte(reservation_base, 0x5a);
    // SAFETY: this focused fixture serializes the two adapters.
    unsafe {
        memory.add_shared_readonly_region(Some(GuestIsa::M68k), reservation_base, reservation);
    }

    let mut heap_cursor = local_base;
    let reservation_end = reservation_base + reservation_len;
    let tight_limit = reservation_end + PPC_HEAP_ALIGNMENT;
    let heap_limit = reservation_base + reservation_len + 3 * PPC_HEAP_ALIGNMENT;
    assert_eq!(
        ppc_heap_free_capacity(&memory, heap_cursor, heap_limit),
        (4 * PPC_HEAP_ALIGNMENT, 3 * PPC_HEAP_ALIGNMENT)
    );
    assert!(!ppc_heap_can_alloc_sequence(
        &memory,
        heap_cursor,
        tight_limit,
        &[PPC_HEAP_ALIGNMENT, PPC_HEAP_ALIGNMENT],
    ));
    assert!(!ppc_heap_can_alloc_repeated(
        &memory,
        heap_cursor,
        tight_limit,
        PPC_HEAP_ALIGNMENT,
        2,
    ));
    assert!(ppc_heap_can_alloc_sequence(
        &memory,
        heap_cursor,
        heap_limit,
        &[PPC_HEAP_ALIGNMENT, PPC_HEAP_ALIGNMENT],
    ));
    assert!(ppc_heap_can_alloc_repeated(
        &memory,
        heap_cursor,
        heap_limit,
        PPC_HEAP_ALIGNMENT,
        2,
    ));
    let allocation = ppc_heap_alloc(
        &mut memory,
        &mut heap_cursor,
        heap_limit,
        2 * PPC_HEAP_ALIGNMENT,
        true,
    );

    assert_eq!(allocation, reservation_end);
    assert_eq!(heap_cursor, allocation + 2 * PPC_HEAP_ALIGNMENT);
    assert_eq!(
        ppc_heap_free_capacity(&memory, heap_cursor, heap_limit),
        (PPC_HEAP_ALIGNMENT, PPC_HEAP_ALIGNMENT)
    );
    assert_eq!(bus.read_byte(reservation_base), 0x5a);
    assert_eq!(memory.read_u8(allocation), Some(0));
}

#[test]
fn ppc_loader_excludes_system_reservation_before_initializer_allocations() {
    let reservation_base = PPC_HEAP_BASE + PPC_HEAP_ALIGNMENT;
    let mut bus = MacMemoryBus::new((reservation_base + 0x0009_0000) as usize);
    let reservation = bus.synthetic_reservation_range().unwrap();
    assert_eq!(reservation.0, reservation_base);
    bus.write_byte(reservation_base, 0x5a);

    let mut loaded = load_pef_application_with_config_and_system_reservation(
        &synthetic_pef_with_initializer(),
        PpcLoadConfig::default(),
        reservation,
    )
    .unwrap();
    let reservation_end = reservation.0 + reservation.1;

    assert!(loaded.heap_cursor() > reservation_end);
    assert!(loaded
        .memory
        .has_readonly_allocation_exclusion(reservation.0, reservation.1));
    assert_eq!(
        loaded.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
        Some(
            ppc_heap_free_capacity(
                &loaded.memory,
                loaded.heap_cursor(),
                test_heap_limit!(loaded)
            )
            .0
        )
    );
    assert_eq!(bus.read_byte(reservation_base), 0x5a);

    let mut installed = loaded.clone();
    assert!(installed.prepare_shared_system_reservation(reservation.0, reservation.1));
    let (shared_base, shared) = bus.shared_synthetic_reservation().unwrap();
    // SAFETY: this focused fixture serializes both adapters.
    unsafe {
        installed
            .memory
            .add_shared_readonly_region(Some(GuestIsa::M68k), shared_base, shared)
    };
    assert!(!installed
        .memory
        .has_readonly_allocation_exclusion(reservation.0, reservation.1));
    assert!(installed
        .memory
        .shared_view()
        .is_shared_readonly_range(reservation.0, 1));
    assert_eq!(installed.memory.write_u8(reservation.0, 0xff), None);
    assert_eq!(bus.read_byte(reservation_base), 0x5a);
}

#[test]
fn ppc_memory_reporting_excludes_system_reservation() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FreeMem")).unwrap();
    let reservation_base = PPC_HEAP_BASE + 0x1000;
    let reservation_len = 0x1_0000;
    loaded
        .memory
        .add_readonly_allocation_exclusion(reservation_base, reservation_len)
        .unwrap();
    loaded.set_heap_limit(reservation_base + reservation_len + 0x2000);
    let expected = (0x3000, 0x2000);
    assert_eq!(
        ppc_heap_free_capacity(
            &loaded.memory,
            loaded.heap_cursor(),
            test_heap_limit!(loaded)
        ),
        expected
    );

    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], expected.0, "FreeMem total");

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MaxMem;
    loaded.cpu.gpr[3] = 0;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], expected.1, "MaxMem largest block");

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PurgeMem;
    loaded.cpu.gpr[3] = expected.1 + 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);

    let allocation = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        PPC_HEAP_ALIGNMENT,
        true,
    );
    assert_eq!(allocation, PPC_HEAP_BASE);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
        Some(expected.0 - PPC_HEAP_ALIGNMENT),
        "zcbFree total"
    );
}

#[test]
fn native_generated_code_uses_owned_system_provenance() {
    let mut loaded = load_pef_application(&synthetic_pef_with_initializer()).unwrap();
    for base in [
        PPC_IMPORT_TVECTOR_BASE,
        PPC_IMPORT_TRAP_BASE,
        PPC_CFM_MAIN_STUB_BASE,
        PPC_APPLICATION_INIT_RETURN_PC,
    ] {
        let word = loaded.memory.read_u32_be(base).unwrap();
        assert_eq!(loaded.memory.system_code_isa(base), Some(GuestIsa::PowerPc));
        assert!(loaded
            .memory
            .shared_view()
            .is_shared_readonly_range(base, 4));
        assert!(!loaded.prepare_shared_system_reservation(base, 4));
        assert_eq!(loaded.memory.write_u32_be(base, word ^ u32::MAX), None);
        loaded.memory.add_region(base, vec![0xff; 4]);
        assert_eq!(loaded.memory.read_u32_be(base), Some(word));
        let mut detached = loaded.memory.clone();
        assert_eq!(
            detached.write_shared_system_u32_be(base, word ^ u32::MAX),
            Some(())
        );
        assert_eq!(detached.read_u32_be(base), Some(word ^ u32::MAX));
        assert_eq!(loaded.memory.read_u32_be(base), Some(word));
    }
    assert!(!loaded
        .memory
        .shared_view()
        .is_shared_readonly_range(PPC_CODE_BASE, 4));
}

#[test]
fn ppc_loader_rejects_system_reservation_layout_collisions() {
    fn reservation_at(base: u32) -> (MacMemoryBus, (u32, u32)) {
        let bus = MacMemoryBus::new((base + 0x0009_0000) as usize);
        let reservation = bus.synthetic_reservation_range().unwrap();
        assert_eq!(reservation.0, base);
        (bus, reservation)
    }

    let safe_gap_bus = MacMemoryBus::new(8 * 1024 * 1024);
    load_pef_application_with_config_and_system_reservation(
        &synthetic_pef(),
        PpcLoadConfig::default(),
        safe_gap_bus.synthetic_reservation_range().unwrap(),
    )
    .expect("the 8 MiB runner reservation occupies a native-layout gap");

    for base in [
        PPC_CODE_BASE,
        PPC_MAIN_GWORLD,
        PPC_IMPORT_TVECTOR_BASE,
        PPC_IMPORT_TRAP_BASE,
        PPC_CFM_MAIN_STUB_BASE,
    ] {
        let (_bus, reservation) = reservation_at(base);
        assert_eq!(
            load_pef_application_with_config_and_system_reservation(
                &synthetic_pef(),
                PpcLoadConfig::default(),
                reservation,
            )
            .unwrap_err(),
            PpcLoadError::AddressOverflow,
            "reservation at {base:#010x}"
        );
    }

    let trampoline_reservation_base = PPC_APPLICATION_INIT_RETURN_PC + 44 - 64 * 1024;
    let (_bus, trampoline_reservation) = reservation_at(trampoline_reservation_base);
    load_pef_application_with_config_and_system_reservation(
        &synthetic_pef(),
        PpcLoadConfig::default(),
        trampoline_reservation,
    )
    .expect("the sparse non-initializer layout remains valid");
    assert_eq!(
        load_pef_application_with_config_and_system_reservation(
            &synthetic_pef_with_initializer(),
            PpcLoadConfig::default(),
            trampoline_reservation,
        )
        .unwrap_err(),
        PpcLoadError::AddressOverflow
    );

    let (_bus, initializers_reservation) = reservation_at(PPC_INITIALIZERS_TRAMPOLINE_BASE);
    assert_eq!(
        load_pef_application_with_config_and_optional_system_reservation(
            &synthetic_pef_with_library_import(b"BundledInitializer", b"Missing"),
            PpcLoadConfig::default(),
            Some(initializers_reservation),
            vec![PpcCfmLibraryFragment {
                name: "BundledInitializer".to_string(),
                bytes: synthetic_pef_with_initializer(),
            }],
        )
        .unwrap_err(),
        PpcLoadError::AddressOverflow
    );

    let stack_base = PPC_HEAP_BASE + 0x1000;
    let (_bus, stack_reservation) = reservation_at(stack_base);
    assert_eq!(
        load_pef_application_with_config_and_system_reservation(
            &synthetic_pef(),
            PpcLoadConfig {
                stack_size: PPC_STACK_TOP - stack_base,
                ..PpcLoadConfig::default()
            },
            stack_reservation,
        )
        .unwrap_err(),
        PpcLoadError::AddressOverflow
    );
}

#[test]
fn public_ppc_loader_late_reservation_guard_preserves_mapped_state() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let original_code = loaded.memory.read_u32_be(PPC_CODE_BASE);

    assert!(!loaded.prepare_shared_system_reservation(PPC_CODE_BASE, 64 * 1024));
    assert_eq!(loaded.memory.read_u32_be(PPC_CODE_BASE), original_code);
    assert!(loaded.prepare_shared_system_reservation(PPC_HEAP_BASE + 0x1000, 64 * 1024));
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
fn hle_import_runner_handles_vinstall_success() {
    let pef = synthetic_pef_with_import(b"VInstall");
    let mut loaded = load_pef_application(&pef).unwrap();
    let task_ptr = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        16,
        true,
    );
    loaded.cpu.gpr[3] = task_ptr;
    loaded.memory.write_u16_be(task_ptr + 4, 1).unwrap();
    loaded.memory.write_u16_be(task_ptr + 10, 2).unwrap();
    loaded.memory.write_u16_be(task_ptr + 12, 3).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u16_be(task_ptr + 10), Some(5));
    assert_eq!(loaded.memory.read_u32_be(task_ptr), Some(0));

    let second_task_ptr = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        16,
        true,
    );
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = second_task_ptr;
    loaded.memory.write_u16_be(second_task_ptr + 4, 1).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(task_ptr), Some(second_task_ptr));
    assert_eq!(loaded.memory.read_u32_be(second_task_ptr), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::VRemove;
    loaded.cpu.gpr[3] = task_ptr;
    loaded.memory.write_u16_be(task_ptr + 4, 1).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(second_task_ptr), Some(0));
}

#[test]
fn ppc_publish_tick_rejects_stale_candidate_after_guest_ticks_store() {
    use crate::memory::globals::addr;

    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    loaded.set_tick_count(41);
    loaded
        .memory
        .write_u32_be(addr::TICKS, 7)
        .expect("direct guest Ticks store");

    // A native slice that started at 41 must not restore its old
    // candidate after guest code rewound the writable low-memory cell.
    assert_eq!(loaded.publish_tick(42), 7);
    assert_eq!(loaded.memory.read_u32_be(addr::TICKS), Some(7));
}

#[test]
fn ppc_timer_manager_schedules_and_invokes_native_callbacks() {
    let pef = synthetic_pef_with_import(b"InsTime");
    let mut loaded = load_pef_application(&pef).unwrap();
    let task_ptr = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        20,
        true,
    );
    let callback_entry = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        16,
        true,
    );
    // li r4, $1234; stw r4, 16(r3); blr
    loaded
        .memory
        .write_u32_be(callback_entry, 0x3880_1234)
        .unwrap();
    loaded
        .memory
        .write_u32_be(callback_entry + 4, 0x9083_0010)
        .unwrap();
    loaded.memory.write_u32_be(callback_entry + 8, BLR).unwrap();
    loaded
        .memory
        .write_u32_be(task_ptr + 6, callback_entry)
        .unwrap();

    loaded.cpu.gpr[3] = task_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.timer_tasks.len(), 1);
    assert!(!loaded.timer_tasks[0].active);
    assert_eq!(loaded.memory.read_u16_be(task_ptr + 4), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PrimeTime;
    loaded.cpu.gpr[3] = task_ptr;
    loaded.cpu.gpr[4] = 33;
    loaded.set_tick_count(10);
    loaded.set_clock_cycle_timing(1_000, 0);
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.timer_tasks[0].active);
    assert_eq!(loaded.timer_tasks[0].fire_at_tick, 12);
    assert_eq!(loaded.memory.read_u16_be(task_ptr + 4), Some(0x8000));
    assert!(loaded
        .fire_timer_tasks_for_ticks(10, 1, 8, 64, false, false)
        .is_empty());

    // Timer callbacks use the normal PPC halt return address (0) as
    // their synthetic LR.  That Halted result belongs to the callback;
    // the interrupted foreground CPU must be restored by the dispatcher.
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    let foreground_pc = loaded.cpu.pc;
    let probes = loaded.fire_timer_tasks_for_ticks(11, 1, 8, 64, false, false);

    assert_eq!(probes.len(), 1);
    assert_eq!(probes[0].invocation.task_ptr, task_ptr);
    assert_eq!(probes[0].invocation.end_r3, task_ptr);
    assert!(matches!(
        probes[0].invocation.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(probes[0].invocation.end_pc, PPC_HALT_PC);
    assert_eq!(loaded.cpu.pc, foreground_pc);
    assert_eq!(loaded.memory.read_u32_be(task_ptr + 16), Some(0x1234));
    assert_eq!(loaded.memory.read_u16_be(task_ptr + 4), Some(0));
    assert!(!loaded.timer_tasks[0].active);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::RmvTime;
    loaded.cpu.gpr[3] = task_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.timer_tasks.is_empty());
    assert_eq!(loaded.memory.read_u32_be(task_ptr), Some(0));
}

#[test]
fn native_extended_timer_uses_process_owned_scheduling_metadata() {
    let pef = synthetic_pef_with_import(b"InsXTime");
    let mut native = load_pef_application(&pef).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);
    assert!(native
        .callback_scheduling
        .ptr_eq(&classic.callback_scheduling));

    let task = ppc_heap_alloc(
        &mut native.memory,
        test_heap_cursor!(native),
        test_heap_limit!(native),
        22,
        true,
    );
    native.memory.write_u32_be(task + 6, 0x1234_5678).unwrap();
    native.cpu.gpr[3] = task;
    native.run_with_hle_imports(64);
    assert!(native.timer_tasks[0].extended);
    assert!(classic.timer_tasks[0].extended);

    native.set_tick_count(100);
    native.set_clock_cycle_timing(1_000, 0);
    native.imports[0].dispatcher_target = PpcImportDispatcherTarget::PrimeTime;
    native.cpu.pc = native.entry_pc;
    native.cpu.lr = PPC_HALT_PC;
    native.cpu.gpr[3] = task;
    native.cpu.gpr[4] = (-5_000i32) as u32;
    native.run_with_hle_imports(64);
    assert_eq!(native.timer_tasks[0].fire_at_subtick, 100_300_000);
    assert_eq!(
        classic.callback_scheduling.extended_wakeup(task),
        Some(100_300_000)
    );
    assert_ne!(native.memory.read_u32_be(task + 14), Some(0));

    let detached = classic.callback_scheduling.clone();
    classic.callback_scheduling.set_primary_vbl_slot(7);
    classic.callback_scheduling.set_current_subtick(100_150_000);
    assert_eq!(native.callback_scheduling.primary_vbl_slot(), 7);

    native.imports[0].dispatcher_target = PpcImportDispatcherTarget::RmvTime;
    native.cpu.pc = native.entry_pc;
    native.cpu.lr = PPC_HALT_PC;
    native.cpu.gpr[3] = task;
    native.run_with_hle_imports(64);
    assert_eq!(native.memory.read_u32_be(task + 10), Some((-2_500i32) as u32));
    assert_eq!(detached.primary_vbl_slot(), 0);
    assert_eq!(detached.current_subtick(), 100_000_000);
}

#[test]
fn vbl_callback_receives_task_record_and_can_reschedule_itself() {
    let pef = synthetic_pef();
    let mut loaded = load_pef_application(&pef).unwrap();
    let task_ptr = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        16,
        true,
    );
    let callback_entry = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        16,
        true,
    );
    // li r4, 2; sth r4, 10(r3); blr
    loaded
        .memory
        .write_u32_be(callback_entry, 0x3880_0002)
        .unwrap();
    loaded
        .memory
        .write_u32_be(callback_entry + 4, 0xb083_000a)
        .unwrap();
    loaded.memory.write_u32_be(callback_entry + 8, BLR).unwrap();
    loaded
        .memory
        .write_u32_be(task_ptr + 6, callback_entry)
        .unwrap();
    loaded.memory.write_u16_be(task_ptr + 10, 1).unwrap();
    loaded.vbl_tasks.push(PpcVblTaskRecord {
        task_ptr,
        architecture: CallbackTaskArchitecture::PowerPc,
        slot: None,
        pending: false,
    });

    let probes = loaded.fire_vbl_tasks_for_ticks(0, 3, 8, 64, false, false);

    assert_eq!(probes.len(), 2);
    assert!(probes.iter().all(|probe| {
        probe.invocation.task_ptr == task_ptr
            && probe.invocation.end_r3 == task_ptr
            && matches!(
                probe.invocation.result,
                PpcRunResult::Halted {
                    pc: PPC_HALT_PC,
                    ..
                }
            )
    }));
    assert_eq!(loaded.memory.read_u16_be(task_ptr + 10), Some(2));
    assert_eq!(
        *loaded.vbl_tasks,
        vec![PpcVblTaskRecord {
            task_ptr,
            architecture: CallbackTaskArchitecture::PowerPc,
            slot: None,
            pending: false,
        }]
    );
}

#[test]
fn native_callback_task_imports_mutate_the_attached_process_registry() {
    let mut native = load_pef_application(&synthetic_pef()).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);

    assert!(native.timer_tasks.ptr_eq(&classic.timer_tasks));
    assert!(native.vbl_tasks.ptr_eq(&classic.vbl_tasks));

    let timer = PPC_HEAP_BASE + 0x800;
    let vbl = timer + 0x20;
    native.memory.add_region(timer, vec![0; 0x40]);
    native.memory.write_u32_be(timer + 6, 0x1234_5678).unwrap();
    native.memory.write_u16_be(vbl + 4, 1).unwrap();
    native.memory.write_u16_be(vbl + 10, 2).unwrap();

    let timer_tasks = native.timer_tasks.shared_handle();
    let vbl_tasks = native.vbl_tasks.shared_handle();
    timer_tasks.with_mut(|timer_tasks| {
        ppc_install_time_task(
            &mut native.memory,
            timer_tasks,
            &native.callback_scheduling,
            timer,
            false,
        );
    });
    vbl_tasks.with_mut(|vbl_tasks| {
        ppc_install_vbl_task(&mut native.memory, vbl_tasks, vbl, None);
    });
    assert_eq!(classic.timer_tasks.len(), 1);
    assert_eq!(classic.vbl_tasks.len(), 1);
    assert_eq!(classic.timer_tasks[0].architecture, CallbackTaskArchitecture::PowerPc);
    assert_eq!(classic.vbl_tasks[0].architecture, CallbackTaskArchitecture::PowerPc);

    let detached_timers = classic.timer_tasks.clone();
    let detached_vbls = classic.vbl_tasks.clone();
    timer_tasks.with_mut(|timer_tasks| {
        ppc_remove_time_task(
            &mut native.memory,
            timer_tasks,
            &native.callback_scheduling,
            timer,
        );
    });
    assert_eq!(
        vbl_tasks.with_mut(|vbl_tasks| {
            ppc_remove_vbl_task(&mut native.memory, vbl_tasks, vbl)
        }),
        PPC_NO_ERR
    );

    assert!(classic.timer_tasks.is_empty());
    assert!(classic.vbl_tasks.is_empty());
    assert_eq!(detached_timers.len(), 1);
    assert_eq!(detached_vbls.len(), 1);
}

#[test]
fn hle_import_runner_handles_zone_queries() {
    let pef = synthetic_pef_with_import(b"GetZone");
    let mut loaded = load_pef_application(&pef).unwrap();

    for (target, expected) in [
        (PpcImportDispatcherTarget::GetZone, PPC_APPLICATION_ZONE),
        (PpcImportDispatcherTarget::SystemZone, PPC_SYSTEM_ZONE),
        (
            PpcImportDispatcherTarget::ApplicationZone,
            PPC_APPLICATION_ZONE,
        ),
    ] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected);
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetZone;
    loaded.cpu.gpr[3] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetZone;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
}

#[test]
fn native_powerpc_launch_seeds_modern_application_zone() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetZone")).unwrap();

    assert_eq!(
        loaded.memory.read_u32_be(PPC_THE_ZONE_ADDR),
        Some(PPC_APPLICATION_ZONE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_APPL_ZONE_ADDR),
        Some(PPC_APPLICATION_ZONE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_SYS_ZONE_ADDR),
        Some(PPC_SYSTEM_ZONE)
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(PPC_APPLICATION_ZONE + PPC_ZONE_HEAP_TYPE_OFFSET),
        Some(PPC_ZONE_32_BIT_HEAP | PPC_ZONE_NEW_STYLE_HEAP)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_APPLICATION_ZONE),
        Some(test_heap_limit!(loaded))
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
        Some(test_heap_limit!(loaded) - loaded.heap_base())
    );
}

#[test]
fn hle_import_runner_init_zone_initializes_header_and_current_zone() {
    let pef = synthetic_pef_with_import(b"InitZone");
    let mut loaded = load_pef_application(&pef).unwrap();
    let start = 0x0308_0000;
    let limit = start + 0x400;
    loaded.memory.add_region(start, vec![0xaa; 0x400]);
    loaded.cpu.gpr[3] = 0x0012_3456;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = limit;
    loaded.cpu.gpr[6] = start;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(start), Some(limit));
    assert_eq!(loaded.memory.read_u32_be(start + 4), Some(0));
    assert_eq!(loaded.memory.read_u32_be(start + 8), Some(start + 60));
    assert_eq!(loaded.memory.read_u32_be(start + 12), Some(0x3a8));
    assert_eq!(loaded.memory.read_u32_be(start + 16), Some(0x0012_3456));
    assert_eq!(loaded.memory.read_u16_be(start + 20), Some(4));
    assert_eq!(loaded.memory.read_u32_be(start + 36), Some(0x3a8));
    assert_eq!(loaded.memory.read_u32_be(start + 48), Some(start + 52));
    assert_eq!(loaded.memory.read_u32_be(PPC_THE_ZONE_ADDR), Some(start));
}

#[test]
fn hle_import_runner_handles_max_appl_zone_as_successful_noop() {
    let pef = synthetic_pef_with_import(b"MaxApplZone");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = 0xfeed_face;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 8,
        }
    );
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(loaded.heap_cursor(), PPC_HEAP_BASE);
    assert!(loaded.heap_maximized());
    assert_eq!(loaded.master_pointer_blocks_requested(), 0);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
}

#[test]
fn native_import_application_limit_is_distinct_from_heap_ceiling() {
    let pef = synthetic_pef_with_import(b"SetApplLimit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let native_heap_limit = loaded.heap_limit();
    let requested = loaded.heap_cursor() + 0x1000;
    assert!(requested < native_heap_limit);

    loaded.cpu.gpr[3] = requested;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetApplLimit);
    assert_eq!(loaded.heap_limit(), native_heap_limit);
    assert_eq!(loaded.application_heap_limit(), requested);

    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetApplLimit);
    assert_eq!(loaded.cpu.gpr[3], requested);
}

#[test]
fn native_import_allocations_honor_application_limit_within_physical_heap() {
    // Inside Macintosh: Memory (1992), pp. 2-42--2-44 and 2-83--2-85:
    // NewPtr must stay below the current application boundary, while
    // SetApplLimit may raise that boundary without changing the native
    // heap's physical mapping ceiling.
    let pef = synthetic_pef_with_import(b"NewPtr");
    let mut loaded = load_pef_application(&pef).unwrap();
    let initial_cursor = loaded.heap_cursor();
    let native_heap_ceiling = loaded.heap_limit();
    let lowered_limit = initial_cursor + (2 * PPC_HEAP_ALIGNMENT);
    assert!(lowered_limit < native_heap_ceiling);

    loaded.cpu.gpr[3] = lowered_limit;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetApplLimit);
    assert_eq!(loaded.application_heap_limit(), lowered_limit);
    assert_eq!(loaded.heap_limit(), native_heap_ceiling);

    loaded.cpu.gpr[3] = PPC_HEAP_ALIGNMENT;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    let first_ptr = loaded.cpu.gpr[3];
    assert_eq!(first_ptr, initial_cursor);
    assert_eq!(loaded.heap_cursor(), initial_cursor + PPC_HEAP_ALIGNMENT);

    // A second aligned allocation would cross the lowered boundary. The
    // failed import is transactional: the cursor and pointer records stay
    // unchanged while MemError reports memFullErr.
    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.heap_cursor(), initial_cursor + PPC_HEAP_ALIGNMENT);
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
    assert_eq!(loaded.ptrs().len(), 1);

    // Raising ApplLimit re-enables the remaining physical heap space; the
    // native ceiling itself never changed during the logical update.
    loaded.cpu.gpr[3] = native_heap_ceiling;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetApplLimit);
    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    assert_eq!(loaded.cpu.gpr[3], initial_cursor + PPC_HEAP_ALIGNMENT);
    assert_eq!(loaded.heap_limit(), native_heap_ceiling);
}

#[test]
fn native_import_reusable_blocks_honor_application_limit() {
    // Inside Macintosh: Memory (1992), pp. 2-42--2-44: a disposed fixed
    // block can satisfy a later NewPtr, but the application limit still
    // bounds every address that the Memory Manager may reuse.
    let pef = synthetic_pef_with_import(b"NewPtr");
    let mut loaded = load_pef_application(&pef).unwrap();
    let initial_cursor = loaded.heap_cursor();
    let native_heap_ceiling = loaded.heap_limit();
    let lowered_limit = initial_cursor + (2 * PPC_HEAP_ALIGNMENT);
    let deferred_block = initial_cursor + (4 * PPC_HEAP_ALIGNMENT);

    loaded.with_process_memory_manager(|_, manager| {
        manager.set_application_heap_limit(lowered_limit);
        manager.mutate_native_allocator(|allocator| {
            allocator.free_ptr_blocks.push(ProcessPtrRecord {
                ptr: deferred_block,
                size: PPC_HEAP_ALIGNMENT,
            });
        });
    });

    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    assert_eq!(loaded.cpu.gpr[3], initial_cursor);
    assert_eq!(loaded.heap_cursor(), initial_cursor + PPC_HEAP_ALIGNMENT);

    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MaxMem);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_ALIGNMENT);

    // The same reusable block becomes eligible when the logical boundary
    // is raised; the physical native ceiling remains unchanged.
    loaded.with_process_memory_manager(|_, manager| {
        manager.set_application_heap_limit(native_heap_ceiling);
    });
    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    assert_eq!(loaded.cpu.gpr[3], deferred_block);
    assert_eq!(loaded.heap_cursor(), initial_cursor + PPC_HEAP_ALIGNMENT);
    assert_eq!(loaded.heap_limit(), native_heap_ceiling);
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

#[test]
fn hle_import_runner_tracks_more_masters_requests() {
    let pef = synthetic_pef_with_import(b"MoreMasters");
    let mut loaded = load_pef_application(&pef).unwrap();
    let heap_cursor = loaded.heap_cursor();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = 0xfeed_face;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(!loaded.heap_maximized());
    assert_eq!(loaded.master_pointer_blocks_requested(), 1);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0xface_feed;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xface_feed);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.master_pointer_blocks_requested(), 2);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
}

#[test]
fn hle_import_runner_handles_compact_mem_as_heap_free_query() {
    let pef = synthetic_pef_with_import(b"CompactMem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let free = test_heap_limit!(loaded) - loaded.heap_cursor();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free);
}

#[test]
fn hle_import_runner_handles_purge_mem_as_heap_free_probe() {
    let pef = synthetic_pef_with_import(b"PurgeMem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let heap_cursor = loaded.heap_cursor();
    let free = test_heap_limit!(loaded) - loaded.heap_cursor();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = free;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = free + 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free + 1);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PurgeMemSys;
    loaded.cpu.gpr[3] = free;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
}

#[test]
fn hle_import_runner_handles_mem_error() {
    let pef = synthetic_pef_with_import(b"MemError");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
}

#[test]
fn hle_import_runner_handles_block_move_with_overlap() {
    let pef = synthetic_pef_with_import(b"BlockMove");
    let mut loaded = load_pef_application(&pef).unwrap();
    let buffer_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(buffer_ptr, b"abcdefgh".to_vec());
    loaded.cpu.gpr[3] = buffer_ptr;
    loaded.cpu.gpr[4] = buffer_ptr + 2;
    loaded.cpu.gpr[5] = 6;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], buffer_ptr);
    for (offset, byte) in b"ababcdef".iter().copied().enumerate() {
        assert_eq!(
            loaded.memory.read_u8(buffer_ptr + offset as u32),
            Some(byte)
        );
    }
}

#[test]
fn hle_import_runner_handles_block_move_data_alias() {
    let pef = synthetic_pef_with_import(b"BlockMoveData");
    let mut loaded = load_pef_application(&pef).unwrap();
    let source_ptr = PPC_DATA_BASE + 0x1000;
    let dest_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(source_ptr, b"Gridz".to_vec());
    loaded.memory.add_region(dest_ptr, vec![0; 5]);
    loaded.cpu.gpr[3] = source_ptr;
    loaded.cpu.gpr[4] = dest_ptr;
    loaded.cpu.gpr[5] = 5;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    for (offset, byte) in b"Gridz".iter().copied().enumerate() {
        assert_eq!(loaded.memory.read_u8(dest_ptr + offset as u32), Some(byte));
    }
}

#[test]
fn hle_import_runner_handles_handle_size_queries() {
    let pef = synthetic_pef_with_import(b"GetHandleSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    test_handles!(loaded).push(PpcHandleRecord {
        handle: PPC_HEAP_BASE,
        ptr: PPC_HEAP_BASE + 4,
        size: 123,
        capacity: 123,
    });
    loaded
        .memory
        .write_u32_be(PPC_HEAP_BASE, PPC_HEAP_BASE + 4)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 123);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    let pef = synthetic_pef_with_import(b"GetHandleSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_CTABLE_HANDLE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_CTABLE_SIZE);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"abcdefghijkl",
    );
    assert_ne!(handle, 0);
    let old_ptr = loaded.memory.read_u32_be(handle).unwrap();
    let old_heap_cursor = loaded.heap_cursor();
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 48;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(test_handle_records!(loaded)[0].size, 48);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let new_ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(new_ptr, old_ptr);
    assert_eq!(test_handle_records!(loaded)[0].ptr, old_ptr);
    assert_eq!(
        loaded.heap_cursor(),
        old_heap_cursor + ppc_allocation_size(48).unwrap() - ppc_allocation_size(12).unwrap()
    );
    for (offset, byte) in b"abcdefghijkl".iter().copied().enumerate() {
        assert_eq!(loaded.memory.read_u8(new_ptr + offset as u32), Some(byte));
    }

    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"abcdefghijkl",
    );
    assert_ne!(handle, 0);
    let old_ptr = loaded.memory.read_u32_be(handle).unwrap();
    let blocker = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        16,
        true,
    );
    assert_ne!(blocker, 0);
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 48;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(test_handle_records!(loaded)[0].size, 48);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let new_ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_ne!(new_ptr, old_ptr);
    assert_eq!(test_handle_records!(loaded)[0].ptr, new_ptr);
    assert!(handle < new_ptr || handle >= new_ptr + 48);
    for (offset, byte) in b"abcdefghijkl".iter().copied().enumerate() {
        assert_eq!(loaded.memory.read_u8(new_ptr + offset as u32), Some(byte));
    }
}

#[test]
fn hle_import_runner_dispose_handle_invalidates_tracked_handle() {
    let pef = synthetic_pef_with_import(b"DisposeHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"data",
    );
    assert_ne!(handle, 0);
    let heap_cursor = loaded.heap_cursor();
    loaded.aliases.push(PpcAliasRecord {
        handle,
        target_vref: PPC_BOOT_VOLUME_REF_NUM,
        target_dir_id: PPC_ROOT_DIR_ID,
        target_name: b"Target".to_vec(),
    });
    let current_resource_refnum = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: "Test App".to_string(),
        res_type: u32::from_be_bytes(*b"PICT"),
        res_id: 128,
        name: b"Title".to_vec(),
        data: b"data".to_vec(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle,
    });

    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(test_handle_records!(loaded)
        .iter()
        .all(|record| record.handle != handle));
    assert_eq!(loaded.memory.read_u32_be(handle), Some(0));
    assert!(loaded.aliases.is_empty());
    assert_eq!(loaded.process_file_system.vfs_resources[0].handle, 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetHandleSize;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.last_mem_error(), PPC_NIL_HANDLE_ERR);
}

#[test]
fn hle_import_runner_tracks_handle_lock_and_no_purge_state() {
    let pef = synthetic_pef_with_import(b"HLock");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"lockable",
    );
    assert_ne!(handle, 0);
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: false,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HNoPurge;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HLockHi;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: true,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HUnlock;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: false,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HLock;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HPurge;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: false,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HNoPurge;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MoveHHi;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HGetState;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x80);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HSetState;
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 0x40;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: false,
            high_locked: false,
            no_purge: false,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x8000;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.handle_states().len(), 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeHandle;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.handle_states().is_empty());
    assert!(test_handle_records!(loaded)
        .iter()
        .all(|record| record.handle != handle));
    assert_eq!(loaded.memory.read_u32_be(handle), Some(0));
}

#[test]
fn hle_import_runner_dispose_handle_tolerates_unknown_handle() {
    let pef = synthetic_pef_with_import(b"DisposeHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let heap_cursor = loaded.heap_cursor();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x4000;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE + 0x4000);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(test_handle_records!(loaded).is_empty());
}

#[test]
fn hle_import_runner_handles_new_handle_clear_allocation() {
    let pef = synthetic_pef_with_import(b"NewHandleClear");
    let mut loaded = load_pef_application(&pef).unwrap();
    let heap_cursor = loaded.heap_cursor();
    loaded.cpu.gpr[3] = 12;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(test_handle_records!(loaded).len(), 1);
    assert_eq!(test_handle_records!(loaded)[0].handle, handle);
    assert_eq!(handle, heap_cursor);
    assert_eq!(handle & (PPC_HEAP_ALIGNMENT - 1), 0);
    assert_eq!(
        test_handle_records!(loaded)[0].ptr,
        heap_cursor + PPC_HEAP_ALIGNMENT
    );
    assert_eq!(
        test_handle_records!(loaded)[0].ptr & (PPC_HEAP_ALIGNMENT - 1),
        0
    );
    assert_eq!(test_handle_records!(loaded)[0].size, 12);
    assert_eq!(
        loaded.memory.read_u32_be(handle),
        Some(heap_cursor + PPC_HEAP_ALIGNMENT)
    );
    assert_eq!(
        loaded.heap_cursor(),
        heap_cursor + ppc_allocation_size(4).unwrap() + ppc_allocation_size(12).unwrap()
    );
    for offset in 0..12 {
        assert_eq!(
            loaded
                .memory
                .read_u8(heap_cursor + PPC_HEAP_ALIGNMENT + offset),
            Some(0)
        );
    }
}

#[test]
fn new_handle_abi_marshalling_shares_semantics_without_sharing_addresses() {
    fn classic_outcome(
        trap_word: u16,
        clear: bool,
        size: u32,
    ) -> (bool, i16, Option<u8>, Option<u32>, Option<bool>) {
        let (mut dispatcher, mut cpu, mut bus) = crate::trap::test_helpers::setup();
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::D0, size);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .expect("classic NewHandle should be handled")
            .expect("classic NewHandle should return cleanly");

        let handle = cpu.read_reg(Register::A0);
        let error = cpu.read_reg(Register::D0) as i16;
        if handle == 0 {
            return (false, error, None, None, None);
        }

        let state = dispatcher.handle_state_bits(handle);
        let ptr = bus.read_long(handle);
        let memory_manager = dispatcher.process_memory_manager();
        let memory_manager = memory_manager.borrow();
        let allocation_size = memory_manager.classic_allocation_size(ptr);
        let contents_zero = clear.then(|| {
            bus.read_bytes(ptr, allocation_size.unwrap_or(0) as usize)
                .iter()
                .all(|&byte| byte == 0)
        });
        (true, error, state, allocation_size, contents_zero)
    }

    fn native_outcome(
        clear: bool,
        size: u32,
    ) -> (bool, i16, Option<u8>, Option<u32>, Option<bool>) {
        let import = if clear {
            b"NewHandleClear".as_slice()
        } else {
            b"NewHandle".as_slice()
        };
        let pef = synthetic_pef_with_import(import);
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = size;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::NewHandle { clear },
        );

        let handle = loaded.cpu.gpr[3];
        let error = loaded.last_mem_error();
        let (state, record) = {
            let memory_manager = loaded.process_memory_manager.0.borrow();
            (
                memory_manager.state_for_handle(handle),
                memory_manager.native_allocation(handle),
            )
        };
        let contents_zero = if clear {
            record.map(|record| {
                (0..record.size)
                    .all(|offset| loaded.memory.read_u8(record.ptr + offset) == Some(0))
            })
        } else {
            // Ordinary NewHandle contents are undefined. Do not compare
            // whatever byte pattern a backend happens to leave there.
            None
        };
        (
            handle != 0,
            error,
            state,
            record.map(|record| record.size),
            contents_zero,
        )
    }

    // The four classic trap variants (current/system × clear/non-clear)
    // must reach the same semantic operation as the native InterfaceLib
    // entry points. Handles and data pointers are intentionally omitted
    // from the compared outcome because each allocator owns its physical
    // address, alignment, and layout.
    for (trap_word, clear) in [
        (0xA022, false),
        (0xA322, true),
        (0xA422, false),
        (0xA622, true),
    ] {
        let classic = classic_outcome(trap_word, clear, 13);
        let native = native_outcome(clear, 13);
        assert_eq!(classic, native, "trap ${trap_word:04X} semantic outcome");
        assert_eq!(
            classic,
            (true, 0, Some(0), Some(13), clear.then_some(true))
        );
    }

    // Macintosh Size is signed. Both ABI adapters reject the same
    // negative value before entering an unsigned physical allocator.
    let classic = classic_outcome(0xA022, false, u32::MAX);
    let native = native_outcome(false, u32::MAX);
    assert_eq!(classic, native);
    assert_eq!(classic, (false, -108, None, None, None));

    fn classic_mem_full_outcome() -> (bool, i16, Option<u8>, Option<u32>, Option<bool>) {
        let (mut dispatcher, mut cpu, mut bus) = crate::trap::test_helpers::setup();
        let heap_limit = bus.classic_heap_limit();
        bus.reserve_heap_until(heap_limit);
        dispatcher.current_trap_word = 0xA022;
        cpu.write_reg(Register::D0, 24);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .expect("classic NewHandle should be handled")
            .expect("classic NewHandle failure should stay in the ABI shim");
        (
            cpu.read_reg(Register::A0) != 0,
            cpu.read_reg(Register::D0) as i16,
            dispatcher.handle_state_bits(cpu.read_reg(Register::A0)),
            None,
            None,
        )
    }

    fn native_mem_full_outcome() -> (bool, i16, Option<u8>, Option<u32>, Option<bool>) {
        let pef = synthetic_pef_with_import(b"NewHandle");
        let mut loaded = load_pef_application(&pef).unwrap();
        let heap_cursor = loaded.heap_cursor();
        loaded.set_heap_limit(heap_cursor + 8);
        loaded.cpu.gpr[3] = 24;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::NewHandle { clear: false },
        );
        let handle = loaded.cpu.gpr[3];
        let state = loaded
            .process_memory_manager
            .0
            .borrow()
            .state_for_handle(handle);
        (
            handle != 0,
            loaded.last_mem_error(),
            state,
            None,
            None,
        )
    }

    let classic = classic_mem_full_outcome();
    let native = native_mem_full_outcome();
    assert_eq!(classic, native);
    assert_eq!(classic, (false, -108, None, None, None));

    // TempNewHandle has a result-code pointer and temporary lifetime, so
    // it remains a distinct ABI route rather than silently inheriting the
    // ordinary NewHandle request/result operation above.
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TempNewHandle"),
        PpcImportDispatcherTarget::TempNewHandle
    );
}

#[test]
fn hle_import_runner_reuses_disposed_handle_capacity() {
    let pef = synthetic_pef_with_import(b"NewHandleClear");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 64;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    let handle = loaded.cpu.gpr[3];
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    let heap_cursor = loaded.heap_cursor();
    loaded.memory.write_u8(ptr, 0xff).unwrap();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeHandle;
    loaded.cpu.gpr[3] = handle;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert!(test_handle_records!(loaded).is_empty());
    assert_eq!(loaded.free_handle_blocks().len(), 1);
    assert_eq!(loaded.free_handle_blocks()[0].capacity, 64);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NewHandle { clear: true };
    loaded.cpu.gpr[3] = 16;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(loaded.memory.read_u32_be(handle), Some(ptr));
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(test_handle_records!(loaded)[0].size, 16);
    assert_eq!(test_handle_records!(loaded)[0].capacity, 64);
    assert!(loaded.free_handle_blocks().is_empty());
    for offset in 0..16 {
        assert_eq!(loaded.memory.read_u8(ptr + offset), Some(0));
    }
}

#[test]
fn hle_import_runner_handles_temp_new_handle_result_code() {
    let pef = synthetic_pef_with_import(b"TempNewHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let result_code_ptr = PPC_DATA_BASE;
    loaded.memory.add_region(result_code_ptr, vec![0xff; 4]);
    loaded.cpu.gpr[3] = 5;
    loaded.cpu.gpr[4] = result_code_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    assert_eq!(
        loaded.memory.read_u16_be(result_code_ptr),
        Some(PPC_NO_ERR as u16)
    );
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(test_handle_records!(loaded).len(), 1);
    assert_eq!(test_handle_records!(loaded)[0].handle, handle);
    assert_eq!(test_handle_records!(loaded)[0].size, 5);
}

#[test]
fn hle_import_runner_temp_new_handle_heap_full_reports_result_code() {
    let pef = synthetic_pef_with_import(b"TempNewHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let result_code_ptr = PPC_DATA_BASE;
    let heap_cursor = loaded.heap_cursor();
    loaded.memory.add_region(result_code_ptr, vec![0xff; 4]);
    loaded.set_heap_limit(heap_cursor + 8);
    loaded.cpu.gpr[3] = 4;
    loaded.cpu.gpr[4] = result_code_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u16_be(result_code_ptr),
        Some(PPC_MEM_FULL_ERR as u16)
    );
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(test_handle_records!(loaded).is_empty());
}

#[test]
fn hle_import_runner_handles_max_mem_and_writes_zero_grow() {
    let pef = synthetic_pef_with_import(b"MaxMem");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_DATA_BASE;
    loaded
        .memory
        .write_u32_be(PPC_DATA_BASE, 0xffff_ffff)
        .unwrap();
    let free = test_heap_limit!(loaded) - loaded.heap_cursor();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free);
    assert_eq!(loaded.memory.read_u32_be(PPC_DATA_BASE), Some(0));
}

#[test]
fn hle_import_runner_handles_cur_res_file() {
    let pef = synthetic_pef_with_import(b"CurResFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_current_resource_refnum(7);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 7);
}

#[test]
fn hle_import_runner_handles_use_res_file_and_res_error_state() {
    let pef = synthetic_pef_with_import(b"UseResFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 5;
    loaded.set_test_resource_error(-192);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 5);
    assert_eq!(*loaded.process_file_system.current_resource_file, 5);
    assert_eq!(loaded.test_resource_error(), 0);
}

#[test]
fn hle_import_runner_handles_signed_res_error() {
    let pef = synthetic_pef_with_import(b"ResError");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_test_resource_error(-192);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], (-192i32) as u32);
}

#[test]
fn hle_import_runner_models_stdc_signal_install_ignore_and_default() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"signal");
    let mut loaded = load_pef_application(&pef).unwrap();
    let signal = 16;
    let first_handler = 0x0302_9b38;
    let second_handler = 0x0302_9b3c;

    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = first_handler;
    let first = loaded.run_with_hle_imports(64);
    assert_eq!(first.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = second_handler;
    let second = loaded.run_with_hle_imports(64);
    assert_eq!(second.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], first_handler);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = 1; // SIG_IGN
    let ignored = loaded.run_with_hle_imports(64);
    assert_eq!(ignored.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], second_handler);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = 0; // SIG_DFL
    let defaulted = loaded.run_with_hle_imports(64);
    assert_eq!(defaulted.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = first_handler;
    let reinstalled = loaded.run_with_hle_imports(64);
    assert_eq!(reinstalled.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0; // invalid signal number
    loaded.cpu.gpr[4] = second_handler;
    let invalid = loaded.run_with_hle_imports(64);
    assert_eq!(invalid.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], u32::MAX);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = 0;
    let unchanged = loaded.run_with_hle_imports(64);
    assert_eq!(unchanged.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], first_handler);
}

#[test]
fn hle_import_runner_handles_mathlib_ceil_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"ceil");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = (-300.1f64).to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), -300.0);
}

#[test]
fn mathlib_pi_tvector_import_binds_to_addressable_double_data() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"pi");
    let mut loaded = load_pef_application(&pef).unwrap();

    assert_eq!(loaded.imports[0].class, 2);
    assert_eq!(loaded.imports[0].address, PPC_IMPORT_MATH_PI);
    assert_eq!(loaded.imports[0].tvector_address, None);
    assert_eq!(
        loaded.memory.read_u64_be(PPC_IMPORT_MATH_PI),
        Some(std::f64::consts::PI.to_bits())
    );
}

#[test]
fn mathlib_default_environment_data_import_binds_to_zeroed_fenv() {
    let pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"MathLib",
        b"_FE_DFL_ENV",
        1,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut loaded = load_pef_application(&pef).unwrap();

    assert_eq!(loaded.imports[0].class, 1);
    assert_eq!(loaded.imports[0].address, PPC_IMPORT_MATH_FE_DFL_ENV);
    assert_eq!(loaded.imports[0].tvector_address, None);
    assert_ne!(PPC_IMPORT_MATH_FE_DFL_ENV, PPC_IMPORT_MATH_PI);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_IMPORT_MATH_FE_DFL_ENV),
        Some(0)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(PPC_IMPORT_MATH_FE_DFL_ENV)
    );
}

#[test]
fn stdclib_floating_point_data_imports_bind_to_ieee_constants() {
    let double_constants = [
        (
            b"_DBL_EPSILON".as_slice(),
            PPC_IMPORT_STD_DBL_EPSILON,
            f64::EPSILON.to_bits(),
        ),
        (
            b"_DBL_MAX".as_slice(),
            PPC_IMPORT_STD_DBL_MAX,
            f64::MAX.to_bits(),
        ),
        (
            b"_DBL_MIN".as_slice(),
            PPC_IMPORT_STD_DBL_MIN,
            f64::MIN_POSITIVE.to_bits(),
        ),
    ];
    for (symbol, expected_address, expected_bits) in double_constants {
        let pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
            b"StdCLib",
            symbol,
            1,
            &[sm_index_reloc(0x30, 0)],
        ));
        let mut loaded = load_pef_application(&pef).unwrap();

        assert_eq!(loaded.imports[0].class, 1);
        assert_eq!(loaded.imports[0].address, expected_address);
        assert_eq!(loaded.imports[0].tvector_address, None);
        assert_eq!(expected_address % 8, 0);
        assert_eq!(
            loaded.memory.read_u64_be(expected_address),
            Some(expected_bits)
        );
        assert_eq!(
            loaded.memory.read_u32_be(PPC_DATA_BASE),
            Some(expected_address)
        );
    }

    let float_constants = [
        (
            b"_FLT_EPSILON".as_slice(),
            PPC_IMPORT_STD_FLT_EPSILON,
            f32::EPSILON.to_bits(),
        ),
        (
            b"_FLT_MAX".as_slice(),
            PPC_IMPORT_STD_FLT_MAX,
            f32::MAX.to_bits(),
        ),
        (
            b"_FLT_MIN".as_slice(),
            PPC_IMPORT_STD_FLT_MIN,
            f32::MIN_POSITIVE.to_bits(),
        ),
    ];
    for (symbol, expected_address, expected_bits) in float_constants {
        let pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
            b"StdCLib",
            symbol,
            1,
            &[sm_index_reloc(0x30, 0)],
        ));
        let mut loaded = load_pef_application(&pef).unwrap();

        assert_eq!(loaded.imports[0].class, 1);
        assert_eq!(loaded.imports[0].address, expected_address);
        assert_eq!(loaded.imports[0].tvector_address, None);
        assert_eq!(expected_address % 4, 0);
        assert_eq!(
            loaded.memory.read_u32_be(expected_address),
            Some(expected_bits)
        );
        assert_eq!(
            loaded.memory.read_u32_be(PPC_DATA_BASE),
            Some(expected_address)
        );
    }

    let occupied = [
        PPC_IMPORT_MATH_PI,
        PPC_IMPORT_MATH_FE_DFL_ENV,
        PPC_IMPORT_STD_DBL_EPSILON,
        PPC_IMPORT_STD_DBL_MAX,
        PPC_IMPORT_STD_DBL_MIN,
        PPC_IMPORT_STD_FLT_EPSILON,
        PPC_IMPORT_STD_FLT_MAX,
        PPC_IMPORT_STD_FLT_MIN,
        PPC_IMPORT_STD_ERRNO,
        PPC_IMPORT_STD_MAC_OS_ERR,
    ];
    assert!(occupied.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(PPC_IMPORT_STD_MAC_OS_ERR + 2 <= PPC_IMPORT_CTYPE_TABLE);
}

#[test]
fn stdclib_error_globals_are_writable_process_scoped_data() {
    let errno_pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"StdCLib",
        b"errno",
        1,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut first = load_pef_application(&errno_pef).unwrap();
    assert_eq!(first.imports[0].class, 1);
    assert_eq!(first.imports[0].address, PPC_IMPORT_STD_ERRNO);
    assert_eq!(first.imports[0].tvector_address, None);
    assert_eq!(first.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(0));
    assert_eq!(
        first.memory.read_u32_be(PPC_DATA_BASE),
        Some(PPC_IMPORT_STD_ERRNO)
    );
    assert!(first
        .memory
        .write_u32_be(PPC_IMPORT_STD_ERRNO, 34)
        .is_some());
    assert_eq!(first.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(34));

    let mut second = load_pef_application(&errno_pef).unwrap();
    assert_eq!(second.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(0));

    let mac_os_err_pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"StdCLib",
        b"MacOSErr",
        1,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut loaded = load_pef_application(&mac_os_err_pef).unwrap();
    assert_eq!(loaded.imports[0].class, 1);
    assert_eq!(loaded.imports[0].address, PPC_IMPORT_STD_MAC_OS_ERR);
    assert_eq!(loaded.imports[0].tvector_address, None);
    assert_eq!(
        loaded.memory.read_u16_be(PPC_IMPORT_STD_MAC_OS_ERR),
        Some(0)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(PPC_IMPORT_STD_MAC_OS_ERR)
    );
    assert!(loaded
        .memory
        .write_u16_be(PPC_IMPORT_STD_MAC_OS_ERR, 0xffce)
        .is_some());
    assert_eq!(
        loaded.memory.read_u16_be(PPC_IMPORT_STD_MAC_OS_ERR),
        Some(0xffce)
    );
}

#[test]
fn native_ppc_math_ceil_preserves_special_values_and_reports_signaling_nan() {
    let mut cpu = PpcCpu::new();
    cpu.fpscr = 3;
    for input in [-0.0f64, f64::INFINITY, f64::NEG_INFINITY] {
        cpu.fpr[1] = input.to_bits();
        ppc_math_ceil(&mut cpu);
        assert_eq!(cpu.fpr[1], input.to_bits());
        assert_eq!(cpu.fpscr, 3);
    }

    let quiet_nan = 0x7ff8_0000_0000_0042;
    cpu.fpr[1] = quiet_nan;
    ppc_math_ceil(&mut cpu);
    assert_eq!(cpu.fpr[1], quiet_nan);
    assert_eq!(cpu.fpscr, 3);

    let signaling_nan = 0xfff0_0000_0000_0042;
    cpu.fpr[1] = signaling_nan;
    cpu.set_fpscr_bit(24, true);
    ppc_math_ceil(&mut cpu);
    assert_eq!(cpu.fpr[1], signaling_nan | 0x0008_0000_0000_0000);
    for bit in [0, 1, 2, 7, 24] {
        assert!(cpu.fpscr_bit(bit));
    }
}

#[test]
fn hle_import_runner_handles_mathlib_sqrt_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"sqrt");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = 9.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), 3.0);
}

#[test]
fn hle_import_runner_handles_mathlib_atan2_in_fprs() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"atan2");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.fpr[1] = 1.0f64.to_bits();
    loaded.cpu.fpr[2] = 1.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let result = f64::from_bits(loaded.cpu.fpr[1]);
    assert!((result - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
}

#[test]
fn hle_import_runner_handles_mathlib_fmod_in_fprs() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"fmod");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = (-5.5f64).to_bits();
    loaded.cpu.fpr[2] = 2.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), -1.5);

    loaded.cpu.fpr[1] = (-0.0f64).to_bits();
    loaded.cpu.fpr[2] = 3.0f64.to_bits();
    ppc_math_fmod(&mut loaded.cpu);
    assert_eq!(loaded.cpu.fpr[1], (-0.0f64).to_bits());
}

#[test]
fn hle_import_runner_handles_mathlib_atan_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"atan");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.fpr[1] = 1.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let result = f64::from_bits(loaded.cpu.fpr[1]);
    assert!((result - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
}

#[test]
fn hle_import_runner_handles_mathlib_log10_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"log10");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = 1000.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), 3.0);
}

#[test]
fn hle_import_runner_handles_mathlib_exp_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"exp");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = 1.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    let result = f64::from_bits(loaded.cpu.fpr[1]);
    assert!((result - std::f64::consts::E).abs() < 1e-12);
}

#[test]
fn hle_import_runner_handles_mathlib_dtox80() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"dtox80");
    let mut loaded = load_pef_application(&pef).unwrap();
    let input = PPC_HEAP_BASE + 0x100;
    let output = PPC_HEAP_BASE + 0x108;
    let bits = (-1.5f64).to_bits();
    loaded.memory.write_u32_be(input, (bits >> 32) as u32);
    loaded.memory.write_u32_be(input + 4, bits as u32);
    loaded.cpu.gpr[3] = input;
    loaded.cpu.gpr[4] = output;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(output), Some(0xbfff));
    assert_eq!(loaded.memory.read_u32_be(output + 2), Some(0xc000_0000));
    assert_eq!(loaded.memory.read_u32_be(output + 6), Some(0));
}

#[test]
fn hle_import_runner_handles_x2fix_in_fpr1() {
    let pef = synthetic_pef_with_import(b"X2Fix");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.fpr[1] = 1.75f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x0001_c000);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.fpr[1] = 40000.0f64.to_bits();
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x7fff_ffff);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.fpr[1] = (-40000.0f64).to_bits();
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x8000_0000);
}

mod palette_manager;
mod gworld;
mod blit;

mod file_manager;




mod text_edit;
mod scrap_manager;
mod list_manager;


#[test]
fn pbh_get_v_info_enumerates_and_selects_mounted_volumes() {
    let pef = synthetic_pef_with_import(b"PBGetVInfoSync");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.seed_vfs_volumes(vec![PpcVfsVolumeRecord {
        ref_num: -2,
        name: "Gridz™ CD".to_string(),
        root_dir_id: 42,
        attributes: 0x8080,
        file_count: 38,
        allocation_block_count: 400,
        allocation_block_size: 2048,
        clump_size: 4096,
        free_blocks: 12,
        bitmap_start: 3,
        allocation_pointer: 4,
        allocation_start: 5,
        next_catalog_id: 81,
        created_date: 0x1234_5678,
        modified_date: 0x2345_6789,
    }]);
    let pb = PPC_DATA_BASE + 0x1000;
    let name_ptr = pb + 0x100;
    loaded.memory.add_region(pb, vec![0; 0x200]);
    loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
    loaded.memory.write_u16_be(pb + 28, 2).unwrap();
    loaded.cpu.gpr[3] = pb;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(pb + 22), Some((-2i16) as u16));
    assert_eq!(loaded.memory.read_u16_be(pb + 38), Some(0x8080));
    assert_eq!(loaded.memory.read_u16_be(pb + 40), Some(38));
    assert_eq!(loaded.memory.read_u32_be(pb + 48), Some(2048));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, name_ptr),
        Some(encode_mac_roman_lossy("Gridz™ CD"))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = pb;
    loaded.memory.write_u16_be(pb + 28, 3).unwrap();
    let _ = loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NSV_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = pb;
    loaded.memory.write_u16_be(pb + 22, 0).unwrap();
    loaded.memory.write_u16_be(pb + 28, 0).unwrap();
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        name_ptr,
        &encode_mac_roman_lossy("Gridz™ CD:ToolBot Power Supply"),
    ));
    let _ = loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(pb + 22), Some((-2i16) as u16));
}

#[test]
fn pbh_get_v_info_keeps_relative_paths_on_the_default_volume() {
    let pef = synthetic_pef_with_import(b"PBHGetVInfoSync");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pb = PPC_DATA_BASE + 0x1000;
    let name_ptr = pb + 0x100;
    loaded.memory.add_region(pb, vec![0; 0x200]);
    loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
    loaded.memory.write_u16_be(pb + 22, 0).unwrap();
    loaded.memory.write_u16_be(pb + 28, (-1i16) as u16).unwrap();
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        name_ptr,
        b":data:title.phd",
    ));
    loaded.cpu.gpr[3] = pb;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u16_be(pb + 22),
        Some(PPC_BOOT_VOLUME_REF_NUM as u16)
    );
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, name_ptr).as_deref(),
        Some(crate::trap::TrapDispatcher::boot_volume_name().as_bytes())
    );
}


#[test]
fn hle_import_runner_gets_and_sets_cur_dir_store_low_memory_global() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMSetCurDirStore"),
        PpcImportDispatcherTarget::LMSetCurDirStore
    );

    let pef = synthetic_pef_with_import(b"LMSetCurDirStore");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.seed_vfs_directories(initial_ppc_vfs_directories(), 0x1122_3344, 18);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(crate::memory::globals::addr::CUR_DIR_STORE),
        Some(0x1122_3344)
    );

    loaded.cpu.gpr[3] = 0x5566_7788;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(crate::memory::globals::addr::CUR_DIR_STORE),
        Some(0x5566_7788)
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LMGetCurDirStore;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x5566_7788);
}

#[test]
fn hle_import_runner_gets_and_sets_sf_save_disk_low_memory_global() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMSetSFSaveDisk"),
        PpcImportDispatcherTarget::LMSetSFSaveDisk
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetSFSaveDisk"),
        PpcImportDispatcherTarget::LMGetSFSaveDisk
    );

    let pef = synthetic_pef_with_import(b"LMSetSFSaveDisk");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = (-7_i16) as u16 as u32;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(crate::memory::globals::addr::SF_SAVE_DISK),
        Some((-7_i16) as u16)
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LMGetSFSaveDisk;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], (-7_i16) as u32);
}

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
fn hle_import_runner_exposes_sound_driver_in_unit_table() {
    let pef = synthetic_pef_with_import(b"LMGetUTableBase");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_UNIT_TABLE);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_UNIT_TABLE + 3 * 4),
        Some(PPC_SOUND_DCE_HANDLE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_SOUND_DCE_HANDLE),
        Some(PPC_SOUND_DCE)
    );
    assert_eq!(
        loaded.memory.read_u16_be(PPC_SOUND_DCE + 24),
        Some((-4i16) as u16)
    );
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
fn hle_import_runner_gets_and_sets_random_seed_low_memory_global() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMSetRndSeed"),
        PpcImportDispatcherTarget::LMSetRndSeed
    );
    let pef = synthetic_pef_with_import(b"LMSetRndSeed");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_RAND_SEED_ADDR),
        Some(0x1234_5678)
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LMGetRndSeed;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
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
fn hle_import_runner_lm_get_sys_map_returns_the_hle_fallback_refnum() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetSysMap"),
        PpcImportDispatcherTarget::LMGetSysMap
    );
    let pef = synthetic_pef_with_import(b"LMGetSysMap");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xdead_beef;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_tracks_quickdraw_cursor_level() {
    let pef = synthetic_pef_with_import(b"HideCursor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
    assert_eq!(loaded.cursor_level(), -1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HideCursor;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cursor_level(), -2);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ShowCursor;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cursor_level(), -1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ShowCursor;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cursor_level(), 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ShowCursor;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cursor_level(), 0);

    loaded.cursor_state.set_level_for_test(-3);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::InitCursor;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cursor_level(), 0);
}

#[test]
fn hle_import_runner_obscure_cursor_preserves_cursor_level() {
    let pef = synthetic_pef_with_import(b"ObscureCursor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cursor_state.set_level_for_test(-2);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cursor_level(), -2);
}

#[test]
fn hle_import_runner_shield_cursor_hides_until_show_cursor() {
    let pef = synthetic_pef_with_import(b"ShieldCursor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 12;
    loaded.cpu.gpr[4] = 34;
    loaded.cpu.gpr[5] = PPC_HEAP_BASE + 0x40;
    loaded.cursor_state.set_level_for_test(-2);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 12);
    assert_eq!(loaded.cpu.gpr[4], 34);
    assert_eq!(loaded.cpu.gpr[5], PPC_HEAP_BASE + 0x40);
    assert_eq!(loaded.cursor_level(), -3);
}

#[test]
fn attached_resource_policy_mutations_cross_isa_immediately() {
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    classic_bus.write_word(TEST_SP, 0x00ff);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_toolbox(true, 0x19b, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert!(!native.policy.res_load());

    native.cpu.gpr[3] = 1;
    run_test_import(&mut native, PpcImportDispatcherTarget::SetResLoad);
    assert!(classic.policy.res_load());

    classic_bus.write_word(TEST_SP, 0x0100);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_toolbox(true, 0x193, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert!(native.policy.res_purge());
}

#[test]
fn attached_application_limit_mutations_cross_isa_immediately() {
    // Inside Macintosh: Memory (1992), pp. 2-83--2-85: SetApplLimit
    // changes the guest-visible application boundary without changing
    // the allocator's physical heap ceiling.
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);
    native.attach_unconverted_process_services(&mut context);

    let native_heap_ceiling = native.heap_limit();
    assert_eq!(
        classic_bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
        native.application_heap_limit(),
        "native attachment must synchronize the initial low-memory projection"
    );
    let requested = native.heap_cursor() + 0x1000;
    assert!(requested < native_heap_ceiling);
    native.cpu.gpr[3] = requested;
    run_test_import(&mut native, PpcImportDispatcherTarget::SetApplLimit);
    assert_eq!(native.application_heap_limit(), requested);
    assert_eq!(
        classic_bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
        requested,
        "native SetApplLimit must publish the process value through low memory"
    );

    let classic_requested = requested + 0x1000;
    classic_bus.write_long(crate::memory::globals::addr::HEAP_END, native.heap_cursor());
    classic_bus.write_long(
        crate::memory::globals::addr::APPL_LIMIT,
        requested,
    );
    classic_cpu.write_reg(Register::A0, classic_requested);
    assert!(classic
        .dispatch_memory(false, 0x2D, &mut classic_cpu, &mut classic_bus)
        .expect("SetApplLimit should be handled")
        .is_ok());

    // The immediately following native import is the nested cross-ISA
    // observation point: it must read process state, not a stale adapter
    // snapshot or the native allocator ceiling.
    assert_eq!(native.application_heap_limit(), classic_requested);
    native.cpu.gpr[3] = 0;
    run_test_import(&mut native, PpcImportDispatcherTarget::GetApplLimit);
    assert_eq!(native.cpu.gpr[3], classic_requested);
    assert_eq!(native.heap_limit(), native_heap_ceiling);
}

#[test]
fn attached_resource_errors_use_canonical_low_memory_cross_isa() {
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);

    classic_bus.write_word(
        crate::memory::globals::addr::RES_ERR,
        PPC_RES_NOT_FOUND_ERR as u16,
    );
    run_test_import(&mut native, PpcImportDispatcherTarget::ResError);
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_RES_NOT_FOUND_ERR));

    native.cpu.gpr[3] = 0xdead_beef;
    run_test_import(&mut native, PpcImportDispatcherTarget::LoadResource);
    assert_eq!(
        classic_bus.read_word(crate::memory::globals::addr::RES_ERR) as i16,
        PPC_RES_NOT_FOUND_ERR
    );

    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_resource(true, 0x1af, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP) as i16, PPC_RES_NOT_FOUND_ERR);
}

#[test]
fn cloned_native_adapter_detaches_resource_policy_and_error_state() {
    let mut original =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    original.policy.set_res_load(false);
    original.policy.set_res_purge(true);
    original.set_test_resource_error(PPC_RES_NOT_FOUND_ERR);
    let mut detached = original.clone();

    detached.policy.set_res_load(true);
    detached.policy.set_res_purge(false);
    detached.set_test_resource_error(PPC_RES_F_NOT_FOUND_ERR);

    assert!(!original.policy.res_load());
    assert!(original.policy.res_purge());
    assert_eq!(original.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
    assert!(detached.policy.res_load());
    assert!(!detached.policy.res_purge());
    assert_eq!(
        detached.test_resource_error(),
        PPC_RES_F_NOT_FOUND_ERR
    );
}

#[test]
fn cloned_native_adapter_detaches_process_cursor_state() {
    let loaded = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let detached = loaded.clone();
    let mut data = [0; 32];
    data[0] = 0x80;
    let mut mask = [0; 32];
    mask[0] = 0xc0;

    detached.cursor_state.hide();
    detached
        .cursor_state
        .install(crate::display::CursorImage::mono(data, mask, 3, 4));

    assert_eq!(loaded.cursor_level(), 0);
    assert_eq!(loaded.cursor_data(), Some(TrapDispatcher::default_arrow_cursor()));
    assert_eq!(detached.cursor_level(), -1);
    assert_eq!(detached.cursor_data(), Some((data, mask, 3, 4)));
}

#[test]
fn attached_cursor_visibility_mutations_cross_isa_immediately() {
    let pef = synthetic_pef_with_import(b"HideCursor");
    let mut native = load_pef_application(&pef).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    let probe = native.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(classic.cursor_level(), -1);
    assert!(!classic.cursor_visible());

    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_dialog(true, 0x053, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(native.cursor_level(), 0);
}

#[test]
fn attached_cursor_image_mutations_cross_isa_immediately() {
    let pef = synthetic_pef_with_import(b"SetCursor");
    let mut native = load_pef_application(&pef).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    let native_cursor = PPC_DATA_BASE + 0x1000;
    let mut native_data = [0; 32];
    native_data[0] = 0x80;
    let mut native_mask = [0; 32];
    native_mask[0] = 0xc0;
    let mut cursor = vec![0; 68];
    cursor[..32].copy_from_slice(&native_data);
    cursor[32..64].copy_from_slice(&native_mask);
    cursor[64..66].copy_from_slice(&3u16.to_be_bytes());
    cursor[66..68].copy_from_slice(&4u16.to_be_bytes());
    native.memory.add_region(native_cursor, cursor);
    native.cpu.gpr[3] = native_cursor;

    let probe = native.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(classic.cursor_data(), Some((native_data, native_mask, 3, 4)));

    let classic_cursor = 0x0002_0000;
    let mut classic_data = [0; 32];
    classic_data[0] = 0x40;
    let mut classic_mask = [0; 32];
    classic_mask[0] = 0xe0;
    for (index, byte) in classic_data.iter().enumerate() {
        classic_bus.write_byte(classic_cursor + index as u32, *byte);
    }
    for (index, byte) in classic_mask.iter().enumerate() {
        classic_bus.write_byte(classic_cursor + 32 + index as u32, *byte);
    }
    classic_bus.write_word(classic_cursor + 64, 5);
    classic_bus.write_word(classic_cursor + 66, 6);
    classic_bus.write_long(TEST_SP, classic_cursor);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_dialog(true, 0x051, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());

    assert_eq!(native.cursor_data(), Some((classic_data, classic_mask, 5, 6)));
}


#[test]
fn attached_control_manager_metadata_crosses_isa_immediately() {
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, _classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    let window = classic_bus.alloc(180);
    let (classic_handle, classic_pointer) = classic.create_control_record(
        &mut classic_bus,
        window,
        (10, 20, 30, 140),
        b"Mode",
        true,
        1,
        300,
        96,
        1009,
        0,
    );
    let classic_record = native
        .controls
        .records()
        .into_iter()
        .find(|record| record.handle == classic_handle)
        .unwrap();
    assert_eq!(classic_record.pointer, classic_pointer);
    assert_eq!(classic_record.proc_id, 1009);
    assert_eq!(classic_record.popup_menu_id, 300);
    assert_eq!(classic_record.popup_title_width, Some(96));

    native.controls.register(0x0030_1000, 0x0030_2000, 16, 0);
    assert_eq!(classic.control_manager.proc_id(0x0030_2000), 16);

    classic.dispose_control_handle(&mut classic_bus, classic_handle);
    assert!(!native.controls.contains_handle(classic_handle));
    native.controls.remove_handle(0x0030_1000);
    assert!(!classic.control_manager.contains_pointer(0x0030_2000));
}

#[test]
fn cloned_native_adapter_detaches_control_manager_metadata() {
    let original =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    original
        .controls
        .register(0x0031_1000, 0x0031_2000, 1, 0);
    let detached = original.clone();

    detached.controls.set_proc_id(0x0031_2000, 2);
    detached
        .controls
        .register(0x0031_3000, 0x0031_4000, 16, 0);

    assert_eq!(original.controls.proc_id(0x0031_2000), 1);
    assert_eq!(detached.controls.proc_id(0x0031_2000), 2);
    assert!(!original.controls.contains_handle(0x0031_3000));
}

#[test]
fn hle_import_runner_handles_color_cursor_resources() {
    let pef = synthetic_pef_with_import(b"GetCCursor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_current_resource_refnum(5);
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: 5,
        path: "Gridz Demo/Gridz Demo".to_string(),
        res_type: u32::from_be_bytes(*b"crsr"),
        res_id: 128,
        name: b"Gridz Cursor".to_vec(),
        data: (0..96).map(|value| value as u8).collect(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    loaded.cpu.gpr[3] = 128;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    assert_eq!(loaded.process_file_system.vfs_resources[0].handle, handle);
    let cursor_ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u8(cursor_ptr + 20), Some(20));
    assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 128;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 999;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
}

#[test]
fn hle_import_runner_handles_cursor_resources_and_system_fallbacks() {
    let pef = synthetic_pef_with_import(b"GetCursor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_current_resource_refnum(5);
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: 5,
        path: "Gridz Demo/Gridz Demo".to_string(),
        res_type: u32::from_be_bytes(*b"CURS"),
        res_id: 128,
        name: b"Gridz Cursor".to_vec(),
        data: (0..68).map(|value| value as u8).collect(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    loaded.cpu.gpr[3] = 128;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    assert_eq!(loaded.process_file_system.vfs_resources[0].handle, handle);
    let cursor_ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u8(cursor_ptr + 20), Some(20));
    assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 2;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let system_handle = loaded.cpu.gpr[3];
    assert_ne!(system_handle, 0);
    let system_cursor_ptr = loaded.memory.read_u32_be(system_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(system_cursor_ptr + 64), Some(7));
    assert_eq!(loaded.memory.read_u16_be(system_cursor_ptr + 66), Some(7));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 2;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], system_handle);

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 999;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
}

#[test]
fn hle_import_runner_gets_plots_and_disposes_color_icons() {
    let pef = synthetic_pef_with_import(b"GetCIcon");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_current_resource_refnum(5);
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: 5,
        path: "Escape Velocity".to_string(),
        res_type: u32::from_be_bytes(*b"cicn"),
        res_id: 128,
        name: b"Pilot".to_vec(),
        data: test_one_bit_cicon(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    loaded.cpu.gpr[3] = 128;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let icon_handle = loaded.cpu.gpr[3];
    let icon_ptr = loaded.memory.read_u32_be(icon_handle).unwrap();
    let color_table_handle = loaded.memory.read_u32_be(icon_ptr + 42).unwrap();
    let pixel_data_handle = loaded.memory.read_u32_be(icon_ptr + 78).unwrap();
    assert_ne!(icon_handle, 0);
    assert_ne!(color_table_handle, 0);
    assert_ne!(pixel_data_handle, 0);
    assert_eq!(
        loaded.memory.read_u32_be(icon_ptr + 50),
        Some(icon_ptr + 82)
    );
    assert_eq!(
        loaded.memory.read_u32_be(icon_ptr + 64),
        Some(icon_ptr + 83)
    );
    assert_eq!(test_handle_records!(loaded).len(), 3);

    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 1, 2, 2, 10).unwrap();
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PlotCIcon;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = icon_handle;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, *loaded.current_gworld).unwrap();
    let red_index = pict::closest_clut_index(0xffff, 0, 0, &loaded.screen_clut);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (2, 1)),
        Some(u16::from(red_index))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (5, 1)),
        Some(u16::from(red_index))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (6, 1)),
        Some(0)
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeCIcon;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = icon_handle;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(icon_handle), Some(0));
    assert!(test_handle_records!(loaded).is_empty());
    assert_eq!(loaded.free_handle_blocks().len(), 3);
}

#[test]
fn hle_import_runner_handles_color_cursor_procedures() {
    let pef = synthetic_pef_with_import(b"SetCCursor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE);

    let pef = synthetic_pef_with_import(b"DisposeCCursor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE);

    let pef = synthetic_pef_with_import(b"SetCursor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let cursor_ptr = PPC_DATA_BASE + 0x1000;
    let mut cursor = vec![0; 68];
    cursor[0] = 0x80;
    cursor[32] = 0xc0;
    cursor[64..66].copy_from_slice(&3u16.to_be_bytes());
    cursor[66..68].copy_from_slice(&4u16.to_be_bytes());
    loaded.memory.add_region(cursor_ptr, cursor);
    loaded.cpu.gpr[3] = cursor_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], cursor_ptr);
    let (data, mask, hot_v, hot_h) = loaded.cursor_data().expect("installed cursor");
    assert_eq!(data[0], 0x80);
    assert_eq!(mask[0], 0xc0);
    assert_eq!(hot_v, 3);
    assert_eq!(hot_h, 4);
}

#[test]
fn hle_import_runner_handles_color2_index() {
    let pef = synthetic_pef_with_import(b"Color2Index");
    let mut loaded = load_pef_application(&pef).unwrap();
    let color_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(color_ptr, vec![0; 8]);

    ppc_write_rgb_color(&mut loaded.memory, color_ptr, PPC_RGB_BLACK).unwrap();
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 255);

    ppc_write_rgb_color(&mut loaded.memory, color_ptr, PPC_RGB_WHITE).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    let teal = PpcRgbColor {
        red: 0,
        green: 0x8000,
        blue: 0x8000,
    };
    ppc_write_rgb_color(&mut loaded.memory, color_ptr, teal).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.cpu.gpr[3],
        u32::from(pict::closest_clut_index(
            teal.red,
            teal.green,
            teal.blue,
            &TrapDispatcher::standard_mac_8bpp_clut(),
        ))
    );

    loaded
        .memory
        .write_u16_be(PPC_MAIN_PIXMAP + 32, 16)
        .unwrap();
    ppc_write_rgb_color(&mut loaded.memory, color_ptr, PPC_RGB_WHITE).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x7fff);
}

#[test]
fn hle_import_runner_handles_index2_color() {
    let pef = synthetic_pef_with_import(b"Index2Color");
    let mut loaded = load_pef_application(&pef).unwrap();
    let color_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(color_ptr, vec![0; 8]);
    loaded.cpu.gpr[3] = 255;
    loaded.cpu.gpr[4] = color_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let [red, green, blue] = TrapDispatcher::standard_mac_8bpp_clut()[255];
    assert_eq!(
        ppc_read_rgb_color(&mut loaded.memory, color_ptr),
        Some(PpcRgbColor { red, green, blue })
    );
}

#[test]
fn input_snapshot_updates_powerpc_low_memory_device_state() {
    use crate::memory::globals::addr;

    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let mut key_map = [0; PPC_KEY_MAP_SIZE as usize];
    key_map[3] = 0x40;
    loaded.set_input_snapshot(PpcInputSnapshot {
        key_map,
        mouse_button: true,
        mouse_v: 123,
        mouse_h: 456,
    });

    assert_eq!(loaded.memory.read_u8(addr::MB_STATE), Some(0));
    assert_eq!(loaded.memory.read_u8(addr::KEY_MAP_LM + 3), Some(0x40));
    for point_addr in [addr::M_TEMP, addr::MOUSE_LOC, addr::MOUSE_LOC2] {
        assert_eq!(loaded.memory.read_u16_be(point_addr), Some(123));
        assert_eq!(loaded.memory.read_u16_be(point_addr + 2), Some(456));
    }

    loaded.set_input_snapshot(PpcInputSnapshot::default());
    assert_eq!(loaded.memory.read_u8(addr::MB_STATE), Some(0x80));
}

#[test]
fn hle_import_runner_posts_events_with_the_current_mouse_position() {
    let pef = synthetic_pef_with_import(b"PostEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_tick_count(42);
    loaded.set_clock_cycle_timing(1_000, 0);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 123,
        mouse_h: 456,
        ..PpcInputSnapshot::default()
    });
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 0x3120;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.event_queue(),
        &VecDeque::from([PpcQueuedEvent {
            what: 3,
            message: 0x3120,
            when: 42,
            where_v: 123,
            where_h: 456,
            modifiers: 0x0080,
        }])
    );
}

#[test]
fn hle_post_event_uses_current_button_and_modifier_state() {
    let pef = synthetic_pef_with_import(b"PostEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut key_map = [0; PPC_KEY_MAP_SIZE as usize];
    for key_code in [0x37_u8, 0x38, 0x3A, 0x3B] {
        key_map[usize::from(key_code >> 3)] |= 1 << (key_code & 0x07);
    }
    let posted_at = 0x1020_3040;
    loaded.set_tick_count(posted_at);
    loaded.set_input_snapshot(PpcInputSnapshot {
        key_map,
        mouse_button: true,
        mouse_v: 123,
        mouse_h: 456,
    });
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 0xA1B2_C3D4;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::PostEvent);
    let expected_when = posted_at.wrapping_add(4);

    assert_eq!(
        loaded.event_queue().front(),
        Some(PpcQueuedEvent {
            what: 3,
            message: 0xA1B2_C3D4,
            when: expected_when,
            where_v: 123,
            where_h: 456,
            modifiers: 0x1B00,
        })
    );
    assert_eq!(
        loaded.toolbox_startup.event_queue_probe.post_result,
        Some(PPC_NO_ERR)
    );

    loaded.set_event_queue([]);
    loaded.set_input_snapshot(PpcInputSnapshot {
        key_map,
        mouse_button: false,
        mouse_v: 123,
        mouse_h: 456,
    });
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 0x0102_0304;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::PostEvent);

    let event = loaded.event_queue().front().expect("PostEvent must enqueue");
    assert_eq!(event.message, 0x0102_0304);
    assert_eq!(event.modifiers, 0x1B80);
    assert_eq!(event.when, expected_when);
}

#[test]
fn hle_import_runner_posts_events_through_sys_evt_mask_low_memory() {
    let pef = synthetic_pef_with_import(b"PostEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mask_addr = crate::memory::globals::addr::SYS_EVT_MASK;
    assert_eq!(
        loaded.memory.read_u16_be(mask_addr),
        Some(crate::memory::globals::DEFAULT_SYS_EVT_MASK)
    );

    loaded.cpu.gpr[3] = 4;
    loaded.cpu.gpr[4] = 0x1234;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_EVT_NOT_ENB));
    assert!(loaded.event_queue().is_empty());

    loaded.memory.write_u16_be(mask_addr, 0xffff).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 4;
    loaded.cpu.gpr[4] = 0x5678;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.event_queue().front().map(|event| event.what),
        Some(4)
    );
    assert_eq!(
        loaded.event_queue().front().map(|event| event.message),
        Some(0x5678)
    );
}

#[test]
fn hle_import_runner_handles_event_button_and_exit_utilities() {
    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(event_ptr, vec![0xaa; 16]);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: false,
        mouse_v: 123,
        mouse_h: 456,
        ..PpcInputSnapshot::default()
    });
    loaded.set_clock_cycle_timing(1_000, 0);
    loaded.cpu.gpr[3] = 0xffff;
    loaded.cpu.gpr[4] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(event_ptr), Some(0));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 2), Some(0));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 6), Some(0));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 10), Some(123));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 12), Some(456));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 14), Some(0));

    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1080;
    loaded.memory.add_region(event_ptr, vec![0xaa; 16]);
    loaded.set_tick_count(42);
    loaded.set_clock_cycle_timing(1_000, 0);
    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (0x31 << 8) | 0x20,
        when: 42,
        where_v: 240,
        where_h: 320,
        modifiers: 0x0080,
    }]);
    loaded.cpu.gpr[3] = 0x0008;
    loaded.cpu.gpr[4] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u16_be(event_ptr), Some(3));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 2), Some(0x3120));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 6), Some(42));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 10), Some(240));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 12), Some(320));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 14), Some(0x0080));
    assert!(loaded.event_queue().is_empty());

    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1090;
    loaded.memory.add_region(event_ptr, vec![0xaa; 16]);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 17,
        mouse_h: 19,
        ..PpcInputSnapshot::default()
    });
    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (0x31 << 8) | 0x20,
        when: 0,
        where_v: 240,
        where_h: 320,
        modifiers: 0x0080,
    }]);
    loaded.cpu.gpr[3] = 0x0002;
    loaded.cpu.gpr[4] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(event_ptr), Some(0));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 10), Some(17));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 12), Some(19));
    assert_eq!(loaded.event_queue().len(), 1);

    let pef = synthetic_pef_with_import(b"WaitNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(event_ptr, vec![0xaa; 16]);
    loaded.imports[0].symbol_name = "GetNextEvent".to_string();
    loaded.cpu.gpr[3] = 0xffff;
    loaded.cpu.gpr[4] = event_ptr;
    loaded.cpu.gpr[5] = 10;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(event_ptr), Some(0));
    assert!(ppc_run_result_cycles(probe.result) >= PPC_Q3_IDLE_STATE_ONLY_FRAME_EXTRA_CYCLES);

    let pef = synthetic_pef_with_import(b"Button");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    });

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    let pef = synthetic_pef_with_import(b"StillDown");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    });

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    let pef = synthetic_pef_with_import(b"StillDown");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    });
    loaded.set_event_queue([PpcQueuedEvent {
        what: 1,
        message: 0,
        when: 0,
        where_v: 170,
        where_h: 352,
        modifiers: 0x0080,
    }]);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.event_queue().len(), 1);

    let pef = synthetic_pef_with_import(b"GetMouse");
    let mut loaded = load_pef_application(&pef).unwrap();
    let point_ptr = PPC_DATA_BASE + 0x1120;
    loaded.memory.add_region(point_ptr, vec![0xaa; 4]);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 250,
        mouse_h: 320,
        ..PpcInputSnapshot::default()
    });
    loaded.cpu.gpr[3] = point_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(point_ptr), Some(250));
    assert_eq!(loaded.memory.read_u16_be(point_ptr + 2), Some(320));

    let pef = synthetic_pef_with_import(b"ExitToShell");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
}

#[test]
fn get_mouse_returns_current_port_local_coordinates() {
    let pef = synthetic_pef_with_import(b"GetMouse");
    let mut loaded = load_pef_application(&pef).unwrap();
    let point_ptr = PPC_DATA_BASE + 0x1120;
    loaded.memory.add_region(point_ptr, vec![0xaa; 4]);
    ppc_write_rect(&mut loaded.memory, PPC_MAIN_PIXMAP + 6, -60, -80, 540, 720).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 360,
        mouse_h: 580,
        ..PpcInputSnapshot::default()
    });
    loaded.cpu.gpr[3] = point_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(point_ptr), Some(300));
    assert_eq!(loaded.memory.read_u16_be(point_ptr + 2), Some(500));
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

#[test]
fn getkeys_poll_fast_forward_requires_repeated_idle_caller() {
    let key_map_ptr = PPC_DATA_BASE + 0x1000;
    let mut memory = PpcSectionMem::new();
    memory.add_region(key_map_ptr, vec![0xaa; PPC_KEY_MAP_SIZE as usize]);
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = key_map_ptr;
    cpu.lr = 0x0100_46B8;
    let mut idle_poll_counts = HashMap::new();

    for _ in 0..PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        let action = dispatch_getkeys_import(
            &mut cpu,
            &mut memory,
            PpcInputSnapshot::default(),
            Some(&mut idle_poll_counts),
        );
        assert_eq!(action, PpcImportAction::ReturnPreserve);
    }
    let action = dispatch_getkeys_import(
        &mut cpu,
        &mut memory,
        PpcInputSnapshot::default(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(
        action,
        PpcImportAction::ReturnPreserveWithExtraCycles(PPC_GETKEYS_IDLE_POLL_EXTRA_CYCLES)
    );
    for offset in 0..PPC_KEY_MAP_SIZE {
        assert_eq!(memory.read_u8(key_map_ptr + offset), Some(0));
    }

    cpu.lr = 0x0100_4734;
    let action = dispatch_getkeys_import(
        &mut cpu,
        &mut memory,
        PpcInputSnapshot::default(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(action, PpcImportAction::ReturnPreserve);

    for _ in 0..=PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        let action =
            dispatch_getkeys_import(&mut cpu, &mut memory, PpcInputSnapshot::default(), None);
        assert_eq!(
            action,
            PpcImportAction::ReturnPreserve,
            "exact paths must not fast-forward GetKeys"
        );
    }

    let mut input = PpcInputSnapshot::default();
    input.key_map[(PPC_KEY_LEFT / 8) as usize] |= 1u8 << (PPC_KEY_LEFT % 8);
    let action =
        dispatch_getkeys_import(&mut cpu, &mut memory, input, Some(&mut idle_poll_counts));
    assert_eq!(action, PpcImportAction::ReturnPreserve);
    assert!(idle_poll_counts.is_empty());
}

#[test]
fn button_poll_fast_forward_requires_repeated_idle_caller() {
    let mut cpu = PpcCpu::new();
    cpu.lr = 0x0102_5D14;
    let mut idle_poll_counts = HashMap::new();

    for _ in 0..PPC_BUTTON_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        let action = dispatch_button_import(
            &cpu,
            PpcInputSnapshot::default(),
            Some(&mut idle_poll_counts),
        );
        assert_eq!(action, PpcImportAction::Return(0));
    }
    let action = dispatch_button_import(
        &cpu,
        PpcInputSnapshot::default(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(
        action,
        PpcImportAction::ReturnWithExtraCycles(0, PPC_BUTTON_IDLE_POLL_EXTRA_CYCLES)
    );

    cpu.lr = 0x0102_5E00;
    let action = dispatch_button_import(
        &cpu,
        PpcInputSnapshot::default(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(action, PpcImportAction::Return(0));

    let action = dispatch_button_import(
        &cpu,
        PpcInputSnapshot {
            mouse_button: true,
            ..PpcInputSnapshot::default()
        },
        Some(&mut idle_poll_counts),
    );
    assert_eq!(action, PpcImportAction::Return(1));
    assert!(idle_poll_counts.is_empty());
}

#[test]
fn still_down_requires_pressed_button_without_pending_mouse_events() {
    let mut cpu = PpcCpu::new();
    cpu.lr = 0x0102_5D14;
    let mut idle_poll_counts = HashMap::new();
    let pressed = PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    };

    let action = dispatch_still_down_import(&cpu, pressed, &VecDeque::new(), None);
    assert_eq!(action, PpcImportAction::Return(1));

    let event_queue = VecDeque::from([PpcQueuedEvent {
        what: 1,
        message: 0,
        when: 0,
        where_v: 172,
        where_h: 352,
        modifiers: 0x0080,
    }]);
    let action =
        dispatch_still_down_import(&cpu, pressed, &event_queue, Some(&mut idle_poll_counts));
    assert_eq!(action, PpcImportAction::Return(0));
    idle_poll_counts.clear();

    for _ in 0..PPC_BUTTON_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        let action = dispatch_still_down_import(
            &cpu,
            PpcInputSnapshot::default(),
            &VecDeque::new(),
            Some(&mut idle_poll_counts),
        );
        assert_eq!(action, PpcImportAction::Return(0));
    }
    let action = dispatch_still_down_import(
        &cpu,
        PpcInputSnapshot::default(),
        &VecDeque::new(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(
        action,
        PpcImportAction::ReturnWithExtraCycles(0, PPC_BUTTON_IDLE_POLL_EXTRA_CYCLES)
    );

    cpu.lr = 0;
    let action = dispatch_still_down_import(
        &cpu,
        PpcInputSnapshot::default(),
        &VecDeque::new(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(action, PpcImportAction::Return(0));
    assert!(idle_poll_counts.is_empty());
}

#[test]
fn hle_import_runner_event_time_outputs_are_all_or_nothing() {
    let pef = synthetic_pef_with_import(b"GetKeys");
    let mut loaded = load_pef_application(&pef).unwrap();
    let key_map_ptr = PPC_DATA_BASE + 0x1000;
    let mut input = PpcInputSnapshot::default();
    input.key_map[(PPC_KEY_LEFT / 8) as usize] |= 1u8 << (PPC_KEY_LEFT % 8);
    loaded.set_input_snapshot(input);
    loaded.memory.add_region(key_map_ptr, vec![0xcc; 4]);
    loaded.cpu.gpr[3] = key_map_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    for offset in 0..4 {
        assert_eq!(loaded.memory.read_u8(key_map_ptr + offset), Some(0xcc));
    }

    let pef = synthetic_pef_with_import(b"Microseconds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let microseconds_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(microseconds_ptr, vec![0xdd; 4]);
    loaded.cpu.gpr[3] = microseconds_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(microseconds_ptr),
        Some(0xdddd_dddd)
    );

    let pef = synthetic_pef_with_import(b"GetDateTime");
    let mut loaded = load_pef_application(&pef).unwrap();
    let secs_ptr = PPC_DATA_BASE + 0x1180;
    loaded.memory.add_region(secs_ptr, vec![0xbb; 2]);
    loaded.cpu.gpr[3] = secs_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(secs_ptr), Some(0xbbbb));

    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(event_ptr, vec![0xee; 4]);
    loaded.cpu.gpr[3] = 0xffff;
    loaded.cpu.gpr[4] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(event_ptr), Some(0xeeee_eeee));
}

#[test]
fn virtual_tick_count_includes_cycle_phase_and_elapsed_cycles() {
    assert_eq!(ppc_virtual_tick_count(42, 1_000, 250, 749), 42);
    assert_eq!(ppc_virtual_tick_count(42, 1_000, 250, 750), 43);
    assert_eq!(ppc_virtual_tick_count(u32::MAX, 1_000, 0, 1_000), 0);
}

#[test]
fn tick_count_poll_fast_forward_requires_stable_caller_and_tick() {
    let mut cpu = PpcCpu::new();
    cpu.lr = 0x0103_4560;
    let mut idle_poll = PpcTickCountIdlePollState::default();

    for _ in 0..PPC_TICK_COUNT_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        assert_eq!(
            dispatch_tick_count_import(&cpu, 42, 500, Some(&mut idle_poll)),
            PpcImportAction::Return(42)
        );
    }
    assert_eq!(
        dispatch_tick_count_import(&cpu, 42, 500, Some(&mut idle_poll)),
        PpcImportAction::ReturnWithExtraCycles(42, 499)
    );

    cpu.lr = 0x0103_4600;
    assert_eq!(
        dispatch_tick_count_import(&cpu, 42, 400, Some(&mut idle_poll)),
        PpcImportAction::Return(42),
        "a different polling caller must restart detection"
    );
    assert_eq!(idle_poll.repeat_count, 1);

    assert_eq!(
        dispatch_tick_count_import(&cpu, 43, 1_000, Some(&mut idle_poll)),
        PpcImportAction::Return(43),
        "a new TickCount value must restart detection"
    );
    assert_eq!(
        idle_poll.context.as_ref().map(|(lr, tick, _)| (*lr, *tick)),
        Some((cpu.lr, 43))
    );
    assert_eq!(idle_poll.repeat_count, 1);

    idle_poll.reset();
    assert_eq!(
        dispatch_tick_count_import(&cpu, 43, 500, Some(&mut idle_poll)),
        PpcImportAction::Return(43),
        "an intervening non-TickCount import must restart detection"
    );

    for _ in 0..=PPC_TICK_COUNT_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        assert_eq!(
            dispatch_tick_count_import(&cpu, 43, 500, None),
            PpcImportAction::Return(43),
            "exact dispatcher paths must not fast-forward TickCount"
        );
    }
}

#[test]
fn hle_import_runner_fast_forwards_tick_count_poll_to_boundary() {
    let pef = synthetic_pef_with_import(b"LMGetTicks");
    let mut loaded = load_pef_application(&pef).unwrap();
    let loop_pc = PPC_DATA_BASE + 0x1100;
    let trap_hi = (PPC_IMPORT_TRAP_BASE >> 16) as u16;
    let trap_lo = (PPC_IMPORT_TRAP_BASE & 0xffff) as u16;
    let mut loop_code = Vec::new();
    for word in [
        d_form_u(15, 12, 0, trap_hi),  // lis r12, trap@ha
        d_form_u(24, 12, 12, trap_lo), // ori r12, r12, trap@l
        xfx_form(31, 12, 9, 467),      // mtctr r12
        xl_form(19, 20, 0, 528, true), // bctrl
        0x4bff_fffc,                   // b -4
    ] {
        loop_code.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(loop_pc, loop_code);
    loaded.cpu.pc = loop_pc;
    loaded.set_tick_count(42);
    loaded.set_clock_cycle_timing(1_000, 250);

    let probe = loaded.run_with_hle_imports(750);

    assert_eq!(ppc_run_result_cycles(probe.result), 750);
    assert_eq!(loaded.cpu.gpr[3], 42);
    assert!(
        probe.handled_import_count
            <= PPC_TICK_COUNT_IDLE_POLL_FAST_FORWARD_THRESHOLD.saturating_add(1),
        "poll loop executed {} imports instead of returning at the tick boundary",
        probe.handled_import_count
    );
}

#[test]
fn hle_import_runner_tick_count_poll_respects_smaller_slice_budget() {
    let pef = synthetic_pef_with_import(b"LMGetTicks");
    let mut loaded = load_pef_application(&pef).unwrap();
    let loop_pc = PPC_DATA_BASE + 0x1100;
    let trap_hi = (PPC_IMPORT_TRAP_BASE >> 16) as u16;
    let trap_lo = (PPC_IMPORT_TRAP_BASE & 0xffff) as u16;
    let mut loop_code = Vec::new();
    for word in [
        d_form_u(15, 12, 0, trap_hi),
        d_form_u(24, 12, 12, trap_lo),
        xfx_form(31, 12, 9, 467),
        xl_form(19, 20, 0, 528, true),
        0x4bff_fffc,
    ] {
        loop_code.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(loop_pc, loop_code);
    loaded.cpu.pc = loop_pc;
    loaded.set_tick_count(42);
    loaded.set_clock_cycle_timing(1_000, 250);

    let probe = loaded.run_with_hle_imports(100);

    assert_eq!(ppc_run_result_cycles(probe.result), 100);
    assert_eq!(loaded.cpu.gpr[3], 42);
}

#[test]
fn hle_import_trace_keeps_tick_count_poll_exact() {
    let pef = synthetic_pef_with_import(b"LMGetTicks");
    let mut loaded = load_pef_application(&pef).unwrap();
    let loop_pc = PPC_DATA_BASE + 0x1100;
    let trap_hi = (PPC_IMPORT_TRAP_BASE >> 16) as u16;
    let trap_lo = (PPC_IMPORT_TRAP_BASE & 0xffff) as u16;
    let mut loop_code = Vec::new();
    for word in [
        d_form_u(15, 12, 0, trap_hi),
        d_form_u(24, 12, 12, trap_lo),
        xfx_form(31, 12, 9, 467),
        xl_form(19, 20, 0, 528, true),
        0x4bff_fffc,
    ] {
        loop_code.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(loop_pc, loop_code);
    loaded.cpu.pc = loop_pc;
    loaded.set_tick_count(42);
    loaded.set_clock_cycle_timing(10_000, 0);

    let probe = loaded.run_with_hle_import_trace(1_000);

    assert_eq!(ppc_run_result_cycles(probe.result), 1_000);
    assert!(probe.handled_import_count > 100);
    assert_eq!(
        probe
            .import_trace
            .iter()
            .map(|entry| entry.repeat_count)
            .sum::<u64>(),
        u64::from(probe.handled_import_count)
    );
}

#[test]
fn hle_import_runner_does_not_accelerate_stateful_tick_count_loop() {
    let pef = synthetic_pef_with_import(b"LMGetTicks");
    let mut loaded = load_pef_application(&pef).unwrap();
    let loop_pc = PPC_DATA_BASE + 0x1100;
    let trap_hi = (PPC_IMPORT_TRAP_BASE >> 16) as u16;
    let trap_lo = (PPC_IMPORT_TRAP_BASE & 0xffff) as u16;
    let mut loop_code = Vec::new();
    for word in [
        d_form_u(15, 12, 0, trap_hi),  // lis r12, trap@ha
        d_form_u(24, 12, 12, trap_lo), // ori r12, r12, trap@l
        xfx_form(31, 12, 9, 467),      // mtctr r12
        xl_form(19, 20, 0, 528, true), // bctrl
        d_form_u(14, 4, 4, 1),         // addi r4,r4,1
        0x4bff_fff8,                   // b -8
    ] {
        loop_code.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(loop_pc, loop_code);
    loaded.cpu.pc = loop_pc;
    loaded.set_tick_count(42);
    loaded.set_clock_cycle_timing(10_000, 0);

    let probe = loaded.run_with_hle_imports(1_000);

    assert_eq!(ppc_run_result_cycles(probe.result), 1_000);
    assert!(
        probe.handled_import_count > 100,
        "state-changing loop was incorrectly accelerated after {} imports",
        probe.handled_import_count
    );
    assert!(loaded.cpu.gpr[4] > 100);
}

#[test]
fn microseconds_poll_fast_forward_preserves_the_reported_clock() {
    let mut cpu = PpcCpu::new();
    cpu.lr = 0x0103_4560;
    cpu.gpr[3] = PPC_DATA_BASE + 0x1000;
    let mut memory = PpcSectionMem::new();
    memory.add_region(cpu.gpr[3], vec![0; 8]);
    let mut idle_poll_counts = HashMap::new();
    let microseconds = 0x1122_3344_5566_7788u64;

    for _ in 0..PPC_MICROSECONDS_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        assert_eq!(
            dispatch_microseconds_import(
                &cpu,
                &mut memory,
                microseconds,
                Some(&mut idle_poll_counts)
            ),
            PpcImportAction::ReturnPreserve
        );
    }
    assert_eq!(
        dispatch_microseconds_import(
            &cpu,
            &mut memory,
            microseconds,
            Some(&mut idle_poll_counts)
        ),
        PpcImportAction::ReturnPreserveWithExtraCycles(PPC_MICROSECONDS_IDLE_POLL_EXTRA_CYCLES)
    );
    assert_eq!(
        memory.read_u32_be(cpu.gpr[3]),
        Some((microseconds >> 32) as u32)
    );
    assert_eq!(
        memory.read_u32_be(cpu.gpr[3] + 4),
        Some(microseconds as u32)
    );

    cpu.lr = 0;
    assert_eq!(
        dispatch_microseconds_import(
            &cpu,
            &mut memory,
            microseconds,
            Some(&mut idle_poll_counts)
        ),
        PpcImportAction::ReturnPreserve
    );
    assert!(idle_poll_counts.is_empty());
}

#[test]
fn hle_import_runner_fast_forwards_microseconds_poll_loops() {
    let pef = synthetic_pef_with_import(b"Microseconds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let microseconds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(microseconds_ptr, vec![0; 8]);
    loaded.cpu.gpr[3] = microseconds_ptr;
    let loop_pc = PPC_DATA_BASE + 0x1100;
    let trap_hi = (PPC_IMPORT_TRAP_BASE >> 16) as u16;
    let trap_lo = (PPC_IMPORT_TRAP_BASE & 0xffff) as u16;
    let mut loop_code = Vec::new();
    for word in [
        d_form_u(15, 12, 0, trap_hi),  // lis r12, trap@ha
        d_form_u(24, 12, 12, trap_lo), // ori r12, r12, trap@l
        xfx_form(31, 12, 9, 467),      // mtctr r12
        xl_form(19, 20, 0, 528, true), // bctrl
        0x4bff_fffc,                   // b -4
    ] {
        loop_code.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(loop_pc, loop_code);
    loaded.cpu.pc = loop_pc;

    let probe = loaded.run_with_hle_imports(100_000);

    assert!(ppc_run_result_cycles(probe.result) >= 100_000);
    assert!(
        probe.handled_import_count
            <= PPC_MICROSECONDS_IDLE_POLL_FAST_FORWARD_THRESHOLD.saturating_add(20),
        "poll loop executed {} imports instead of charging guest cycles",
        probe.handled_import_count
    );
    assert_ne!(loaded.memory.read_u32_be(microseconds_ptr + 4), Some(0));
}

#[test]
fn hle_import_runner_handles_num_to_string() {
    let pef = synthetic_pef_with_import(b"NumToString");
    let mut loaded = load_pef_application(&pef).unwrap();
    let string_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(string_ptr, vec![0xaa; 32]);
    loaded.cpu.gpr[3] = (-12345i32) as u32;
    loaded.cpu.gpr[4] = string_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, string_ptr).as_deref(),
        Some(b"-12345".as_slice())
    );
}

mod standard_c_library;

#[test]
fn hle_import_runner_handles_string_to_num() {
    let pef = synthetic_pef_with_import(b"StringToNum");
    let mut loaded = load_pef_application(&pef).unwrap();
    let string_ptr = PPC_DATA_BASE + 0x1000;
    let number_ptr = PPC_DATA_BASE + 0x1040;
    loaded.memory.add_region(string_ptr, vec![0; 64]);
    loaded.memory.add_region(number_ptr, vec![0xaa; 4]);
    write_ppc_pstring(&mut loaded.memory, string_ptr, b" -12345x");
    loaded.cpu.gpr[3] = string_ptr;
    loaded.cpu.gpr[4] = number_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], string_ptr);
    assert_eq!(
        loaded.memory.read_u32_be(number_ptr),
        Some((-12345i32) as u32)
    );
}

#[test]
fn hle_import_runner_handles_equal_string() {
    let pef = synthetic_pef_with_import(b"EqualString");
    let mut loaded = load_pef_application(&pef).unwrap();
    let left_ptr = PPC_DATA_BASE + 0x1000;
    let right_ptr = PPC_DATA_BASE + 0x1040;
    loaded.memory.add_region(left_ptr, vec![0; 64]);
    loaded.memory.add_region(right_ptr, vec![0; 64]);
    write_ppc_pstring(&mut loaded.memory, left_ptr, b"Gridz");
    write_ppc_pstring(&mut loaded.memory, right_ptr, b"gridz");
    loaded.cpu.gpr[3] = left_ptr;
    loaded.cpu.gpr[4] = right_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = left_ptr;
    loaded.cpu.gpr[4] = right_ptr;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_handles_random() {
    let pef = synthetic_pef_with_import(b"Random");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .write_u32_be(PPC_RAND_SEED_ADDR, 12345)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_RAND_SEED_ADDR),
        Some(207482415)
    );
    assert_eq!(loaded.cpu.gpr[3], u32::from(207482415u32 as u16));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_ne!(loaded.cpu.gpr[3], u32::from(207482415u32 as u16));

    loaded
        .memory
        .write_u32_be(PPC_RAND_SEED_ADDR, 32768)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_bit_tst_uses_msb_first_bit_offsets() {
    let pef = synthetic_pef_with_import(b"BitTst");
    let mut loaded = load_pef_application(&pef).unwrap();
    let keymap = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(keymap, vec![0; 8]);
    // BitTst counts left-to-right from the high-order bit and accepts
    // offsets beyond the first byte. Inside Macintosh: Operating System
    // Utilities (1994), pp. 3-7 and 3-28.
    loaded.memory.write_u8(keymap, 0x20).unwrap();
    loaded.memory.write_u8(keymap + 6, 0x40).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::BitTst
    );

    for bit in [2, 49] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = keymap;
        loaded.cpu.gpr[4] = bit;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);
    }

    loaded.memory.write_u8(keymap, 0).unwrap();
    loaded.memory.write_u8(keymap + 6, 0).unwrap();
    for bit in [2, 49] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = keymap;
        loaded.cpu.gpr[4] = bit;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
}

#[test]
fn getkeys_lsb_bitmap_integrates_with_msb_first_bit_tst() {
    let pef = synthetic_pef_with_import(b"GetKeys");
    let mut loaded = load_pef_application(&pef).unwrap();
    let keymap = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(keymap, vec![0; 16]);
    let mut input = PpcInputSnapshot::default();
    for key_code in [0x00, 0x31] {
        let (byte, mask) = crate::trap::dispatch::key_map_byte_mask(key_code).unwrap();
        input.key_map[byte] |= mask;
    }
    loaded.set_input_snapshot(input);
    loaded.cpu.gpr[3] = keymap;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u8(keymap), Some(0x01));
    assert_eq!(loaded.memory.read_u8(keymap + 6), Some(0x02));

    // Inside Macintosh Volume I (1985), pp. I-259–I-260 maps virtual key
    // codes directly to KeyMap indexes. BitTst independently counts from
    // each byte's high-order bit (Operating System Utilities, 1994,
    // pp. 3-7 and 3-28), so a KeyMap key is tested at keyCode XOR 7.
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::BitTst;
    for (bit, expected) in [(7, 1), (54, 1), (0, 0), (49, 0)] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = keymap;
        loaded.cpu.gpr[4] = bit;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected, "BitTst offset {bit}");
    }
}

#[test]
fn hle_import_runner_handles_p2cstr() {
    let pef = synthetic_pef_with_import(b"p2cstr");
    let mut loaded = load_pef_application(&pef).unwrap();
    let string_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(string_ptr, vec![0xaa; 32]);
    write_ppc_pstring(&mut loaded.memory, string_ptr, b"Gridz");
    loaded.cpu.gpr[3] = string_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], string_ptr);
    assert_eq!(
        (0..6)
            .map(|offset| loaded.memory.read_u8(string_ptr + offset).unwrap())
            .collect::<Vec<_>>(),
        b"Gridz\0".to_vec()
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::C2PStr;
    loaded.cpu.gpr[3] = string_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], string_ptr);
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, string_ptr).as_deref(),
        Some(b"Gridz".as_slice())
    );
}

#[test]
fn hle_import_runner_upper_text_converts_only_the_requested_bytes() {
    let pef = synthetic_pef_with_import(b"UpperText");
    let mut loaded = load_pef_application(&pef).unwrap();
    let text_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(text_ptr, vec![b'a', b'z', 0x8e, b'!', b'q']);
    loaded.cpu.gpr[3] = text_ptr;
    loaded.cpu.gpr[4] = 3;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], text_ptr);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, text_ptr, 5),
        Some(vec![b'A', b'Z', 0x83, b'!', b'q'])
    );
}

mod thread_manager;

mod process_manager;

#[test]
fn hle_import_runner_handles_display_manager_gdevice_lookup() {
    let pef = synthetic_pef_with_import(b"DMGetGDeviceByDisplayID");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gdevice_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(gdevice_out_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = gdevice_out_ptr;
    loaded.cpu.gpr[5] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(gdevice_out_ptr),
        Some(PPC_MAIN_GDEVICE)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded
        .memory
        .write_u32_be(gdevice_out_ptr, 0xa5a5_5a5a)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_DSP_DISPLAY_ID + 1;
    loaded.cpu.gpr[4] = gdevice_out_ptr;
    loaded.cpu.gpr[5] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DMGetGDeviceByDisplayID,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(gdevice_out_ptr),
        Some(0xa5a5_5a5a),
        "failed lookup must leave the output untouched"
    );

    loaded.cpu.gpr[3] = PPC_DSP_DISPLAY_ID + 1;
    loaded.cpu.gpr[4] = gdevice_out_ptr;
    loaded.cpu.gpr[5] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DMGetGDeviceByDisplayID,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(gdevice_out_ptr),
        Some(PPC_MAIN_GDEVICE),
        "failToMain must resolve an unknown display ID to the main device"
    );
}

#[test]
fn display_manager_display_id_lookup_honors_fail_to_main() {
    let pef = synthetic_pef_with_import(b"DMGetDisplayIDByGDevice");
    let mut loaded = load_pef_application(&pef).unwrap();
    let display_id_out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(display_id_out, vec![0xa5; 4]);
    loaded.cpu.gpr[3] = 0x06ff_0000;
    loaded.cpu.gpr[4] = display_id_out;
    loaded.cpu.gpr[5] = 0;

    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DMGetDisplayIDByGDevice,
    );

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(display_id_out),
        Some(0xa5a5_a5a5),
        "failed lookup must leave the output untouched"
    );

    loaded.cpu.gpr[3] = 0x06ff_0000;
    loaded.cpu.gpr[4] = display_id_out;
    loaded.cpu.gpr[5] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DMGetDisplayIDByGDevice,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(display_id_out),
        Some(PPC_DSP_DISPLAY_ID),
        "failToMain must resolve an unknown GDevice to the main display"
    );
}

#[test]
fn hle_import_runner_handles_get_device_list() {
    let pef = synthetic_pef_with_import(b"GetDeviceList");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_GDEVICE);
}

#[test]
fn hle_import_runner_handles_get_gdevice() {
    let pef = synthetic_pef_with_import(b"GetGDevice");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .current_gdevice
        .with_mut(|current_gdevice| *current_gdevice = PPC_MAIN_GDEVICE);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_GDEVICE);
}

#[test]
fn hle_import_runner_handles_set_gdevice() {
    let pef = synthetic_pef_with_import(b"SetGDevice");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gdevice = PPC_HEAP_BASE + 0x100;
    loaded
        .current_gdevice
        .with_mut(|current_gdevice| *current_gdevice = PPC_MAIN_GDEVICE);
    loaded.cpu.gpr[3] = gdevice;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], gdevice);
    assert_eq!(*loaded.current_gdevice, gdevice);
    assert!(loaded.toolbox_startup.known_gdevices.contains(&gdevice));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(*loaded.current_gdevice, gdevice);
    assert!(!loaded.toolbox_startup.known_gdevices.contains(&0));
}

#[test]
fn hle_import_runner_handles_get_next_device_single_device_chain() {
    let pef = synthetic_pef_with_import(b"GetNextDevice");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_get_max_device_requires_screen_intersection() {
    let pef = synthetic_pef_with_import(b"GetMaxDevice");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 100, 100, 200, 200).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_GDEVICE);

    let top = ppc_main_screen_height() as i16 + 20;
    let left = ppc_main_screen_width() as i16 + 20;
    ppc_write_rect(
        &mut loaded.memory,
        rect_ptr,
        top,
        left,
        top + 100,
        left + 100,
    )
    .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}


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
