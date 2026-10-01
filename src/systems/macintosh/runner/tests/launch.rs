use super::*;
use crate::memory::globals::addr;

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
