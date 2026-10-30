use super::*;
use crate::memory::globals::addr;

#[test]
fn owned_native_launch_transfers_heap_without_changing_launch_state() {
    for owned in [false, true] {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(4321, 0x7654_3210, 0x1234_5678);
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let native = app.ppc.as_mut().unwrap();
        native.memory.add_region(PPC_HEAP_BASE, vec![0x5a; 65536]);
        native.cpu.gpr[31] = 0xfeed_beef;
        let original_heap = native.memory.readable_bytes(PPC_HEAP_BASE, 65536).unwrap().as_ptr();
        if owned {
            runner.init_app_owned(app);
        } else {
            runner.init_app(&app);
        }
        assert_eq!(runner.guest_tick(), 4321);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x7654_3210);
        let native = runner.native.application_mut().unwrap();
        let heap = native.memory.readable_bytes(PPC_HEAP_BASE, 65536).unwrap();
        assert_eq!(heap.as_ptr() == original_heap, owned,
            "only the consuming launch may retain the loader's allocation");
        assert!(heap.iter().all(|byte| *byte == 0x5a));
        assert_eq!(native.cpu.gpr[31], 0xfeed_beef);
        assert_eq!(native.cpu.pc, PPC_CODE_BASE);
        assert_eq!(native.memory.read_u32_be(addr::TICKS), Some(4321));
        assert!(native.cfm.is_none(), "CFM ownership moves into the process");
        native.memory.write_u32_be(addr::TICKS, 4322).unwrap();
        assert_eq!(runner.bus.read_long(addr::TICKS), 4322,
            "the native adapter and runner must retain shared low memory");
    }
}

#[test]
fn owned_classic_launch_preserves_entry_stack_and_pinned_state() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&bytes).unwrap();
    let mut states = Vec::new();
    for owned in [false, true] {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(1234, 0x89ab_cdef, 0);
        let app = runner.load_app(&fork).unwrap();
        if owned { runner.init_app_owned(app); } else { runner.init_app(&app); }
        states.push((runner.m68k.cpu.read_reg(Register::PC), runner.m68k.cpu.read_reg(Register::A7),
            runner.m68k.cpu.read_reg(Register::A5), runner.bus.read_long(addr::TICKS),
            runner.bus.read_long(addr::RND_SEED)));
    }
    assert_eq!(states[0], states[1]);
    assert_eq!((states[1].3, states[1].4), (1234, 0x89ab_cdef));
}

#[test]
fn control_panel_init_runs_before_application_entry() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let app_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let app_fork = ResourceFork::parse(&app_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = runner.load_app(&app_fork).expect("load app");
    runner.init_app(&app);
    let app_entry = runner.m68k.cpu.read_reg(Register::PC);

    // MOVEQ #42,D0; MOVE.L #$12345678,$00180000; RTS
    let init_code = [
        0x70, 0x2a, 0x23, 0xfc, 0x12, 0x34, 0x56, 0x78, 0x00, 0x18, 0x00, 0x00, 0x4e,
        0x75,
    ];
    let extension_bytes = make_resource_fork_bytes(&[(*b"INIT", 128, &init_code)]);
    let path = "Test/System Folder/Control Panels/Example";
    runner.dispatcher.vfs.insert(path.into(), Vec::new());
    runner.dispatcher.vfs_rsrc.insert(path.into(), extension_bytes);
    runner
        .dispatcher
        .set_vfs_entry_metadata(path, *b"cdev", *b"TEST", 0);
    runner
        .dispatcher
        .vfs
        .insert("Test/System Folder/System".into(), Vec::new());

    assert_eq!(runner.start_system_extensions(), 1);
    assert!(runner.dispatcher.vfs_directories.iter().any(|directory| {
        directory.path == "Test/Desktop Folder"
    }));
    assert_ne!(runner.m68k.cpu.read_reg(Register::PC), app_entry);
    for _ in 0..100 {
        if runner.m68k.cpu.read_reg(Register::PC) == app_entry {
            break;
        }
        let (_, running) = runner.run_steps(1, None);
        assert!(running);
    }
    assert_eq!(runner.bus.read_long(0x0018_0000), 0x1234_5678);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), app_entry);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0);
}

#[test]
fn init_app_preserves_resources_allocated_before_zone_header() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let bgas = [0x4E, 0x56, 0xFF, 0xA6, 0x2D, 0x7A, 0x1C, 0x72];
    let fork_bytes = make_resource_fork_bytes(&[(*b"BGAS", 128, &bgas), (*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");
    let (_, bgas_ptr) = runner
        .dispatcher
        .find_or_load_resource_any(&mut runner.bus, *b"BGAS", 128)
        .expect("BGAS resource loaded");
    assert_eq!(runner.bus.read_bytes(bgas_ptr, bgas.len()), bgas);

    runner.init_app(&app);

    assert_eq!(
        runner.bus.read_bytes(bgas_ptr, bgas.len()),
        bgas,
        "init_app must not overwrite resources loaded before zone setup"
    );
}

#[test]
fn init_app_seeds_post_boot_ticks() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");
    runner.init_app(&app);

    assert_eq!(
        runner.bus.read_long(addr::TICKS),
        DEFAULT_LAUNCH_TICKS,
        "fresh app launch should see a realistic nonzero post-boot TickCount"
    );
    assert_eq!(
        runner.guest_tick(),
        DEFAULT_LAUNCH_TICKS,
        "TickCount fast path must stay in sync with low-memory Ticks"
    );
}

#[test]
fn init_app_applies_pinned_68k_launch_state() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_launch_state(1234, 0x89AB_CDEF, 0x1122_3344_5566_7788);

    let app = runner.load_app(&fork).expect("load app");
    runner.init_app(&app);

    assert_eq!(runner.bus.read_long(addr::TICKS), 1234);
    assert_eq!(runner.guest_tick(), 1234);
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x89AB_CDEF);
}

#[test]
fn optional_68k_launch_state_uses_tick_floor_and_exact_seed() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_optional_launch_state(Some(10), Some(0x1020_3040), None);

    let app = runner.load_app(&fork).expect("load app");
    runner.init_app(&app);

    assert_eq!(runner.bus.read_long(addr::TICKS), DEFAULT_LAUNCH_TICKS);
    assert_eq!(runner.guest_tick(), DEFAULT_LAUNCH_TICKS);
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x1020_3040);
}

#[test]
fn init_ppc_app_applies_pinned_launch_state_to_both_memories() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_launch_state(4321, 0x7654_3210, u64::MAX);
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    let tick_entry = crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
    runner.bus.write_long(tick_entry, 0x0021_1000);
    runner.bus.write_long(0x28, 0x0021_2000);
    runner.init_app(&app);
    assert_eq!(
        runner.dispatcher.trap_table_profile,
        Some(TrapTableProfile::PowerPc604)
    );
    assert_ne!(runner.bus.read_long(tick_entry), 0x0021_1000);
    assert!(runner.dispatcher.aline_vector_is_default(&runner.bus));

    assert_eq!(runner.bus.read_long(addr::TICKS), 4321);
    assert_eq!(runner.guest_tick(), 4321);
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x7654_3210);
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    assert_eq!(
        ppc_app
            .memory
            .read_u32_be(crate::memory::globals::addr::TICKS),
        Some(4321)
    );
    assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(4321));
    assert_eq!(
        ppc_app.memory.read_u32_be(addr::RND_SEED),
        Some(0x7654_3210)
    );
    let toolbox_entry = crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
    let default_tick_count = runner.bus.read_long(toolbox_entry);
    assert_eq!(
        ppc_app.memory.read_u32_be(toolbox_entry),
        Some(default_tick_count),
        "PPC and 68k adapters must see one materialized trap table"
    );
    ppc_app
        .memory
        .write_u32_be(toolbox_entry, 0x0021_0000)
        .expect("write shared Toolbox trap entry");
    assert_eq!(runner.bus.read_long(toolbox_entry), 0x0021_0000);
    assert_eq!(ppc_app.cpu.time_base(), u64::MAX);
}

#[test]
fn optional_ppc_seed_preserves_tick_and_time_base_defaults() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_optional_launch_state(None, Some(0x1020_3040), None);
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    runner.init_app(&app);

    assert_eq!(runner.bus.read_long(addr::TICKS), 0);
    assert_eq!(runner.guest_tick(), 0);
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x1020_3040);
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(0));
    assert_eq!(
        ppc_app.memory.read_u32_be(addr::RND_SEED),
        Some(0x1020_3040)
    );
    assert_eq!(ppc_app.cpu.time_base(), 0);
}

#[test]
fn repeated_partial_launch_state_updates_preserve_prior_controls() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_optional_launch_state(Some(4321), None, Some(0x1122_3344_5566_7788));
    runner.set_optional_launch_state(None, Some(0x7654_3210), None);
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    runner.init_app(&app);

    assert_eq!(runner.bus.read_long(addr::TICKS), 4321);
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x7654_3210);
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(4321));
    assert_eq!(
        ppc_app.memory.read_u32_be(addr::RND_SEED),
        Some(0x7654_3210)
    );
    assert_eq!(ppc_app.cpu.time_base(), 0x1122_3344_5566_7788);
}

#[test]
fn native_launch_honors_a_larger_application_partition() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = crate::game::new_runner();
    runner.set_application_partition_size(Some(96 * 1024 * 1024));
    runner.init_app(&app);
    let limit = runner
        .native
        .application()
        .unwrap()
        .application_heap_limit();
    assert!(limit > PPC_STACK_TOP);
    assert_eq!(runner.bus.read_long(addr::APPL_LIMIT), limit);
    // Sparse native mappings can live above the classic RAM backing.
    let address = limit - 16;
    assert!(address > runner.bus.ram_size());
    runner
        .native
        .application_mut()
        .unwrap()
        .memory
        .add_region(address, vec![0x73; 16]);
    assert_eq!(runner.bus.read_byte(address), 0x73);
    runner.bus.write_byte(address, 0x41);
    assert_eq!(
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u8(address),
        Some(0x41)
    );
}

#[test]
fn init_ppc_app_preserves_launch_defaults_without_override() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_app_start_time(0x1020_3040);
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    runner.init_app(&app);

    assert_eq!(runner.bus.read_long(addr::TICKS), 0);
    assert_eq!(runner.guest_tick(), 0);
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x1020_3040);
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    assert_eq!(
        ppc_app
            .memory
            .read_u32_be(crate::memory::globals::addr::TICKS),
        Some(0)
    );
    assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(0));
    assert_eq!(ppc_app.memory.read_u32_be(addr::TIME), Some(0x1020_3040));
    assert_eq!(
        ppc_app.memory.read_u32_be(addr::RND_SEED),
        Some(0x1020_3040)
    );
    assert_eq!(ppc_app.cpu.time_base(), 0);
}

#[test]
fn init_ppc_app_merges_detached_events_then_attaches_the_process_queue() {
    // Test case 1: Context has event and invalidation=true, detached PPC has event and invalidation=false
    {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .process_context
            .shared_event_queue()
            .push_back(QueuedEvent {
                delivery_id: None,
                what: 3,
                message: 0x1111,
                when: 0,
                where_v: 10,
                where_h: 20,
                modifiers: 0x0100,
            });
        runner
            .process_context
            .shared_event_queue()
            .invalidate_menu_bar();

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        ppc_app.event_queue.push_back(QueuedEvent {
            delivery_id: None,
            what: 1,
            message: 0x2222,
            when: 0,
            where_v: 30,
            where_h: 40,
            modifiers: 0x0200,
        });

        runner.init_app(&app);

        let queue = runner.process_context.event_queue();
        assert_eq!(queue.len(), 2);
        assert_eq!(
            queue.get(0).unwrap().message,
            0x1111,
            "canonical event must remain in front"
        );
        assert_eq!(
            queue.get(1).unwrap().message,
            0x2222,
            "detached PPC event must be appended after canonical events"
        );
        assert!(
            queue.menu_bar_is_invalid(),
            "menu_bar_invalid should be true when context was true"
        );
        let native_queue = &runner.native.application().unwrap().event_queue;
        assert_eq!(native_queue.len(), 2);
        assert_eq!(native_queue.get(0).unwrap().message, 0x1111);
        assert_eq!(native_queue.get(1).unwrap().message, 0x2222);
        assert!(native_queue.menu_bar_is_invalid());
    }

    // Test case 2: Context has event and invalidation=false, detached PPC has event and invalidation=true
    {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .process_context
            .shared_event_queue()
            .push_back(QueuedEvent {
                delivery_id: None,
                what: 3,
                message: 0x3333,
                when: 0,
                where_v: 10,
                where_h: 20,
                modifiers: 0x0100,
            });

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        ppc_app.event_queue.push_back(QueuedEvent {
            delivery_id: None,
            what: 1,
            message: 0x4444,
            when: 0,
            where_v: 30,
            where_h: 40,
            modifiers: 0x0200,
        });
        ppc_app.event_queue.invalidate_menu_bar();

        runner.init_app(&app);

        let queue = runner.process_context.event_queue();
        assert_eq!(queue.len(), 2);
        assert_eq!(
            queue.get(0).unwrap().message,
            0x3333,
            "canonical event must remain in front"
        );
        assert_eq!(
            queue.get(1).unwrap().message,
            0x4444,
            "detached PPC event must be appended after canonical events"
        );
        assert!(
            queue.menu_bar_is_invalid(),
            "menu_bar_invalid should be true when detached PPC was true"
        );
        let native_queue = &runner.native.application().unwrap().event_queue;
        assert_eq!(native_queue.len(), 2);
        assert_eq!(native_queue.get(0).unwrap().message, 0x3333);
        assert_eq!(native_queue.get(1).unwrap().message, 0x4444);
        assert!(native_queue.menu_bar_is_invalid());
    }
}

#[test]
fn init_app_writes_cur_ap_name_as_mac_roman() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner
        .dispatcher
        .set_launched_app_path("Games/Shufflepuck Café v1.0");

    let app = runner.load_app(&fork).expect("load app");
    runner.init_app(&app);

    assert_eq!(
        runner.bus.read_pstring(addr::CUR_APNAME),
        b"Shufflepuck Caf\x8E v1.0",
        "CurApName is a classic Str31 and must preserve MacRoman filenames"
    );
    assert_eq!(
        crate::trap::dispatch::TrapDispatcher::read_pb_filename(&runner.bus, addr::CUR_APNAME),
        "Shufflepuck Café v1.0"
    );
}

#[test]
fn fixture_runner_defaults_to_classic_theme() {
    let runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    assert_eq!(runner.ui_theme_id(), UiThemeId::ClassicSystem7);
    assert_eq!(runner.dispatcher().ui_theme_id(), UiThemeId::ClassicSystem7);
    assert_eq!(runner.ui_theme().id(), UiThemeId::ClassicSystem7);
    assert_eq!(
        runner.theme_metrics_mode(),
        ThemeMetricsMode::ClassicGuestMetrics
    );
    assert!(runner.uses_classic_guest_metrics());
}

#[test]
fn fixture_runner_accepts_explicit_classic_theme_without_themed_metrics() {
    let runner = FixtureRunner::new(
        8 * 1024 * 1024,
        FixtureRunnerConfig {
            ui_theme: UiThemeId::ClassicSystem7,
            theme_metrics_mode: ThemeMetricsMode::ClassicGuestMetrics,
            ..FixtureRunnerConfig::default()
        },
    );
    let systemless = UiThemeId::SystemlessDefault.provider();

    assert_eq!(runner.ui_theme_id(), UiThemeId::ClassicSystem7);
    assert_eq!(runner.dispatcher().ui_theme_id(), UiThemeId::ClassicSystem7);
    assert_eq!(runner.ui_theme().id(), UiThemeId::ClassicSystem7);
    assert!(runner.uses_classic_guest_metrics());
    assert_eq!(runner.ui_theme().menu_metrics(), systemless.menu_metrics());
    assert_eq!(
        runner.ui_theme().control_metrics(),
        systemless.control_metrics()
    );
    assert_ne!(runner.ui_theme().palette(), systemless.palette());
}

#[test]
fn fixture_runner_can_select_classic_theme_before_guest_initialization() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner.set_ui_theme(UiThemeId::ClassicSystem7);

    assert_eq!(runner.ui_theme_id(), UiThemeId::ClassicSystem7);
    assert_eq!(runner.dispatcher().ui_theme_id(), UiThemeId::ClassicSystem7);
    assert!(runner.uses_classic_guest_metrics());
}

#[test]
fn ppc_launchapplication_switches_to_bundled_application_on_either_cpu() {
    for powerpc in [false, true] {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let native = app.ppc.as_mut().unwrap();
        native
            .memory
            .write_u32_be(
                PPC_CODE_BASE,
                ppc_test_relative_branch(PPC_CODE_BASE, PPC_IMPORT_TRAP_BASE),
            )
            .unwrap();
        let mut binding = test_ppc_import_binding(0, "InterfaceLib", "LaunchApplication");
        binding.trap_pc = PPC_IMPORT_TRAP_BASE;
        binding.dispatcher_target = PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::LaunchApplication,
        );
        native.import_count = 1;
        native.imports = vec![binding];
        let pb = PPC_DATA_BASE + 0x3000;
        let spec = pb + 64;
        native.memory.add_region(pb, vec![0; 256]);
        native.memory.write_u16_be(pb + 6, 0x4c43).unwrap();
        native.memory.write_u32_be(pb + 8, 32).unwrap();
        native.memory.write_u32_be(pb + 16, spec).unwrap();
        native
            .memory
            .write_u32_be(spec + 2, PPC_ROOT_DIR_ID)
            .unwrap();
        native
            .memory
            .write_bytes(spec + 6, b"\x0fregister helper")
            .unwrap();
        native.cpu.gpr[3] = pb;
        let helper_code0 = minimal_code0(0, 0x2000, 0, 0);
        let (helper_data, helper_bytes) = if powerpc {
            let mut cfrg = vec![0; 76];
            cfrg[8..12].copy_from_slice(&1u32.to_be_bytes());
            cfrg[28..32].copy_from_slice(&1u32.to_be_bytes());
            cfrg[32..36].copy_from_slice(b"pwpc");
            cfrg[54] = 1; // application
            cfrg[55] = 1; // data fork
            cfrg[72..74].copy_from_slice(&44u16.to_be_bytes());
            let mut pef = crate::loader::ppc::tests::synthetic_pef_with_import(b"TickCount");
            let code_offset = u32::from_be_bytes(pef[60..64].try_into().unwrap()) as usize;
            // Keep the helper alive after its real Toolbox call, rather than
            // returning to address zero as the generic loader fixture does.
            pef[code_offset + 16..code_offset + 20].copy_from_slice(&0x4800_0000u32.to_be_bytes());
            (pef, make_resource_fork_bytes(&[(*b"cfrg", 0, &cfrg)]))
        } else {
            (
                Vec::new(),
                make_resource_fork_bytes(&[(*b"CODE", 0, &helper_code0)]),
            )
        };
        let mut runner = FixtureRunner::new(128 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher
            .vfs
            .insert("Register Helper".into(), helper_data);
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Register Helper".into(), helper_bytes);
        runner.dispatcher.ensure_vfs_catalog();
        runner.dispatcher.set_launched_app_path("Main App");
        runner.init_app(&app);
        runner.set_instructions_per_tick(100_000);
        runner.tick_budget = 50_000;
        let (_, running) = runner.run_steps(8, None);
        assert!(running);
        assert!(
            runner.tick_budget > 0 && runner.tick_budget < 50_000,
            "launch must preserve the remaining fraction of the current tick"
        );
        assert!(!runner.is_halted());
        assert_eq!(
            runner.dispatcher.launched_app_path(),
            Some("Register Helper")
        );
        assert_eq!(runner.native.application().is_some(), powerpc);
        assert!(runner.dispatcher.vfs_rsrc.contains_key("Register Helper"));
    }
}

#[test]
fn loaded_code_recovers_its_resource_handle_and_survives_detach() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let code = [0, 0, 0, 0, 0x4e, 0x75, 0x12];
    let bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code)]);
    let fork = ResourceFork::parse(&bytes).unwrap();
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = runner.load_app(&fork).unwrap();
    let ptr = app.segment_bases[&1];
    runner.init_app(&app);
    runner.m68k.cpu.write_reg(Register::A0, ptr);
    runner
        .dispatcher
        .dispatch(0xA128, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap();
    let handle = runner.m68k.cpu.read_reg(Register::A0);
    assert_ne!(handle, 0, "executing CODE must have a recoverable handle");
    assert_eq!(runner.bus.read_long(handle), ptr);
    runner
        .dispatcher
        .dispatch(0xA025, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap();
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), code.len() as u32);
    let resource_handle = runner.dispatcher.get_or_create_resource_handle_in_file(
        &mut runner.bus,
        *b"CODE",
        1,
        ptr,
        0,
    );
    assert_eq!(resource_handle, handle);
    runner.bus.write_byte(ptr + 6, 0x34);
    assert_eq!(
        runner
            .bus
            .read_byte(runner.bus.read_long(resource_handle) + 6),
        0x34
    );
    let sp = runner.m68k.cpu.read_reg(Register::A7) - 4;
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.bus.write_long(sp, handle);
    runner
        .dispatcher
        .dispatch(0xA992, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap();
    runner.m68k.cpu.write_reg(Register::A0, ptr);
    runner
        .dispatcher
        .dispatch(0xA128, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap();
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), handle);
    runner
        .dispatcher
        .dispatch(0xA025, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap();
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), code.len() as u32);
    assert_eq!(
        runner.bus.read_bytes(ptr, code.len()),
        [0, 0, 0, 0, 0x4e, 0x75, 0x34]
    );
    let (_, reloaded_ptr) = runner
        .dispatcher
        .find_or_load_resource_any(&mut runner.bus, *b"CODE", 1)
        .unwrap();
    assert_ne!(reloaded_ptr, ptr);
    assert_eq!(runner.bus.read_bytes(reloaded_ptr, code.len()), code);
    assert_eq!(runner.bus.read_long(handle), ptr);
    assert_eq!(runner.bus.read_byte(ptr + 6), 0x34);
}

#[test]
fn word_aligned_code_zero_keeps_its_address_and_resource_handle() {
    let code0 = minimal_code0(2, 0x2000, 0, 0);
    let code1 = [0, 0, 0, 0, 0x4e, 0x75, 0x12];
    let bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code1)]);
    let fork = ResourceFork::parse(&bytes).unwrap();
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = runner.load_app(&fork).unwrap();
    assert_eq!(app.segment_bases[&0] & 3, 2);
    for (id, expected) in [(0, code0.as_slice()), (1, code1.as_slice())] {
        let ptr = app.segment_bases[&id];
        assert_eq!(runner.bus.read_bytes(ptr, expected.len()), expected);
        runner.m68k.cpu.write_reg(Register::A0, ptr);
        runner
            .dispatcher
            .dispatch(0xA128, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        let handle = runner.m68k.cpu.read_reg(Register::A0);
        assert_ne!(handle, 0);
        assert_eq!(runner.bus.read_long(handle), ptr);
        runner
            .dispatcher
            .dispatch(0xA025, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::D0),
            expected.len() as u32
        );
    }
}
