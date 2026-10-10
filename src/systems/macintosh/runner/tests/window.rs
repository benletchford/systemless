use super::*;
use crate::window_manager::WindowRect;

#[test]
fn init_ppc_app_attaches_one_bidirectional_process_window_list() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.dispatcher.window_list.extend([0x1000, 0x2000]);
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    runner.init_app(&app);

    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    let detached = ppc_app.window_list.clone();
    assert_eq!(ppc_app.window_list, [0x1000, 0x2000]);
    ppc_app.window_list.insert(0, 0x3000);
    assert_eq!(runner.dispatcher.window_list, [0x3000, 0x1000, 0x2000]);
    runner
        .dispatcher
        .window_list
        .retain(|window| *window != 0x1000);
    assert_eq!(ppc_app.window_list, [0x3000, 0x2000]);
    assert_eq!(detached, [0x1000, 0x2000]);
}

fn write_snapshot_rect(bus: &mut MacMemoryBus, address: u32, rect: WindowRect) {
    for (index, value) in [rect.0, rect.1, rect.2, rect.3].into_iter().enumerate() {
        bus.write_word(address.wrapping_add(index as u32 * 2), value as u16);
    }
}

fn write_snapshot_region_bounds(bus: &mut MacMemoryBus, handle: u32, bounds: WindowRect) {
    let region = bus.read_long(handle);
    assert_ne!(region, 0, "window operation must allocate region data");
    bus.write_word(region, 10);
    write_snapshot_rect(bus, region.wrapping_add(2), bounds);
}

fn configure_snapshot_window(runner: &mut FixtureRunner, window: u32, color: bool) {
    runner.bus.write_byte(window.wrapping_add(110), 0xFF);
    runner.bus.write_byte(window.wrapping_add(111), 0xFF);
    write_snapshot_rect(&mut runner.bus, window.wrapping_add(16), (10, 20, 30, 40));
    if color {
        let pixmap_handle = runner.bus.read_long(window.wrapping_add(2));
        let pixmap = runner.bus.read_long(pixmap_handle);
        assert_ne!(pixmap, 0, "NewCWindow must install a PixMap");
        runner
            .bus
            .write_word(pixmap.wrapping_add(6), (-100i16) as u16);
        runner
            .bus
            .write_word(pixmap.wrapping_add(8), (-200i16) as u16);
    } else {
        runner
            .bus
            .write_word(window.wrapping_add(8), (-100i16) as u16);
        runner
            .bus
            .write_word(window.wrapping_add(10), (-200i16) as u16);
    }
    for (offset, bounds) in [
        (114, (100, 200, 140, 250)),
        (24, (5, 7, 15, 17)),
        (122, (101, 202, 111, 212)),
    ] {
        let handle = runner.bus.read_long(window.wrapping_add(offset));
        assert_ne!(handle, 0, "window operation must install region handle");
        write_snapshot_region_bounds(&mut runner.bus, handle, bounds);
    }
    runner.dispatcher.window_list.replace(vec![window]);
    runner
        .bus
        .write_long(crate::memory::globals::addr::GHOST_WINDOW, 0);
}

fn create_classic_snapshot_window(title: &[u8]) -> (FixtureRunner, u32) {
    const BOUNDS: u32 = 0x0030_0000;
    const TITLE: u32 = 0x0030_0100;
    const STACK: u32 = 0x0030_1000;
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    write_snapshot_rect(&mut runner.bus, BOUNDS, (20, 10, 260, 330));
    runner.bus.write_pstring(TITLE, title);
    for offset in 0..30 {
        runner.bus.write_byte(STACK + offset, 0);
    }
    runner.bus.write_byte(STACK + 4, 1);
    runner.bus.write_long(STACK + 6, u32::MAX);
    runner.bus.write_byte(STACK + 12, 1);
    runner.bus.write_long(STACK + 14, TITLE);
    runner.bus.write_long(STACK + 18, BOUNDS);
    runner.m68k.cpu.write_reg(Register::A7, STACK);
    runner
        .dispatcher
        .dispatch(0xA913, &mut runner.m68k.cpu, &mut runner.bus)
        .expect("classic NewWindow must return");
    let window = runner.bus.read_long(STACK + 26);
    assert_ne!(window, 0);
    (runner, window)
}

fn create_native_snapshot_window(title: &[u8]) -> (FixtureRunner, u32) {
    use crate::loader::ppc::tests::synthetic_pef_with_import;
    const SCRATCH: u32 = PPC_DATA_BASE + 0x1000;
    let mut native = load_pef_application(&synthetic_pef_with_import(b"NewCWindow")).unwrap();
    native.memory.add_region(SCRATCH, vec![0; 512]);
    for (index, value) in [20i16, 10, 260, 330].into_iter().enumerate() {
        native
            .memory
            .write_u16_be(SCRATCH + index as u32 * 2, value as u16)
            .unwrap();
    }
    native
        .memory
        .write_u8(SCRATCH + 16, title.len() as u8)
        .unwrap();
    native.memory.write_bytes(SCRATCH + 17, title).unwrap();
    native.cpu.gpr[3] = 0;
    native.cpu.gpr[4] = SCRATCH;
    native.cpu.gpr[5] = SCRATCH + 16;
    native.cpu.gpr[6] = 1;
    native.cpu.gpr[7] = 0;
    native.cpu.gpr[8] = u32::MAX;
    native.cpu.gpr[9] = 1;
    native.cpu.gpr[10] = 0;
    let probe = native.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let window = native.cpu.gpr[3];
    assert_ne!(window, 0);

    let app = LoadedApp::from_ppc(native);
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    (runner, window)
}

#[test]
fn nonrectangular_window_regions_keep_guest_presentation_on_both_cpus() {
    for (mut runner, window) in [
        create_classic_snapshot_window(b"Classic"),
        create_native_snapshot_window(b"PowerPC"),
    ] {
        let frame = runner.window_frame_snapshot().remove(0);
        assert!(frame.rectangular_regions);
        assert!(frame.presentation_definition_id().is_some());
        let definition_id = frame.definition_id;

        for offset in [
            crate::window_manager::WINDOW_STRUCTURE_RGN_OFFSET,
            crate::window_manager::WINDOW_CONTENT_RGN_OFFSET,
        ] {
            let handle = runner.bus.read_long(window + offset);
            let region = runner.bus.read_long(handle);
            runner.bus.write_word(region, 12);
            let frame = runner.window_frame_snapshot().remove(0);
            assert_eq!(frame.definition_id, definition_id);
            assert!(!frame.rectangular_regions);
            assert_eq!(frame.presentation_definition_id(), None);
            runner.bus.write_word(region, 10);
        }
        assert!(runner.window_frame_snapshot()[0].rectangular_regions);
    }
}

// Retain the two divergent expressions from the deleted native projector so
// the integration test proves the common projection changes their result.
// This is source-model evidence; it is not presented as a captured old run.
fn deleted_native_visibility_and_visible_region_model(
    app: &mut PpcLoadedApp,
    window: u32,
) -> (bool, Option<WindowRect>) {
    let visible = app.memory.read_u8(window.wrapping_add(110)) == Some(1);
    let port = (
        app.memory.read_u16_be(window.wrapping_add(16)).unwrap_or(0) as i16,
        app.memory.read_u16_be(window.wrapping_add(18)).unwrap_or(0) as i16,
        app.memory.read_u16_be(window.wrapping_add(20)).unwrap_or(0) as i16,
        app.memory.read_u16_be(window.wrapping_add(22)).unwrap_or(0) as i16,
    );
    let pixmap = app
        .memory
        .read_u32_be(window.wrapping_add(2))
        .and_then(|handle| app.memory.read_u32_be(handle));
    let origin = pixmap.map(|pixmap| {
        (
            app.memory.read_u16_be(pixmap.wrapping_add(6)).unwrap_or(0) as i16,
            app.memory.read_u16_be(pixmap.wrapping_add(8)).unwrap_or(0) as i16,
        )
    });
    let global_port = origin.map(|origin| {
        (
            port.0.saturating_sub(origin.0),
            port.1.saturating_sub(origin.1),
            port.2.saturating_sub(origin.0),
            port.3.saturating_sub(origin.1),
        )
    });
    let visible_region = app
        .memory
        .read_u32_be(window.wrapping_add(24))
        .and_then(|handle| app.memory.read_u32_be(handle))
        .map(|region| {
            (
                app.memory.read_u16_be(region.wrapping_add(2)).unwrap_or(0) as i16,
                app.memory.read_u16_be(region.wrapping_add(4)).unwrap_or(0) as i16,
                app.memory.read_u16_be(region.wrapping_add(6)).unwrap_or(0) as i16,
                app.memory.read_u16_be(region.wrapping_add(8)).unwrap_or(0) as i16,
            )
        })
        .zip(global_port)
        .map(|(rect, port)| {
            (
                rect.0.saturating_add(port.0),
                rect.1.saturating_add(port.1),
                rect.2.saturating_add(port.0),
                rect.3.saturating_add(port.1),
            )
        });
    (visible, visible_region)
}

#[test]
fn runner_window_snapshot_matches_classic_and_native_window_operations() {
    let title = b"Caf\x8e";
    let (mut classic, classic_window) = create_classic_snapshot_window(title);
    let initial_classic = classic.window_stack_snapshot();

    let (mut native, native_window) = create_native_snapshot_window(title);
    let initial_native = native.window_stack_snapshot();
    assert_eq!(initial_classic.len(), 1);
    assert_eq!(initial_native.len(), 1);
    let classic_created = &initial_classic[0];
    let native_created = &initial_native[0];
    assert_eq!(classic_created.title, native_created.title);
    assert_eq!(classic_created.bounds, native_created.bounds);
    assert_eq!(
        classic_created.structure_bounds,
        native_created.structure_bounds
    );
    assert_eq!(
        classic_created.visible_region,
        native_created.visible_region
    );
    assert_eq!(classic_created.visible, native_created.visible);
    assert_eq!(classic_created.active, native_created.active);
    // Preserve the routes' existing initial-update policy: the classic
    // NewWindow route invalidates its content immediately, while the native
    // NewCWindow import currently does not. Snapshot unification must expose
    // that difference without taking ownership of window-creation behavior.
    assert_eq!(classic_created.update_region, Some((20, 10, 260, 330)));
    assert_eq!(native_created.update_region, None);

    configure_snapshot_window(&mut classic, classic_window, false);
    let classic_snapshot = classic.window_stack_snapshot();

    configure_snapshot_window(&mut native, native_window, true);
    let deleted_native_model = deleted_native_visibility_and_visible_region_model(
        native.native.application_mut().expect("native app"),
        native_window,
    );
    assert_eq!(deleted_native_model, (false, Some((115, 227, 125, 237))));
    let native_snapshot = native.window_stack_snapshot();

    assert_eq!(classic_snapshot, native_snapshot);
    assert_eq!(native_snapshot[0].bounds, (110, 220, 130, 240));
    assert_eq!(
        native_snapshot[0].visible_region,
        Some((105, 207, 115, 217))
    );
    assert_eq!(
        classic.window_frame_snapshot()[0].visible_content_rects,
        Some(vec![(105, 207, 115, 217)])
    );
    assert_eq!(
        native.window_frame_snapshot()[0].visible_content_rects,
        Some(vec![(105, 207, 115, 217)])
    );
    assert!(native_snapshot[0].visible);
    assert!(native_snapshot[0].active);
    assert_eq!(
        native_snapshot[0].title,
        crate::mac_roman::decode_mac_roman(title)
    );
}

fn write_minimal_snapshot_window(
    bus: &mut MacMemoryBus,
    window: u32,
    title_handle: u32,
    title: u32,
    bytes: &[u8],
) {
    bus.write_word(window.wrapping_add(6), 0);
    write_snapshot_rect(bus, window.wrapping_add(16), (1, 2, 11, 22));
    bus.write_byte(window.wrapping_add(110), 0xFF);
    bus.write_long(window.wrapping_add(134), title_handle);
    bus.write_long(title_handle, title);
    bus.write_pstring(title, bytes);
}

#[test]
fn runner_snapshot_routes_flat_and_native_sparse_records_through_one_bus() {
    const FLAT: u32 = 0x0002_0000;
    const SPARSE: u32 = 0x0188_0000;
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("native app")
        .memory
        .add_region(SPARSE, vec![0; 0x1000]);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    write_minimal_snapshot_window(&mut runner.bus, FLAT, FLAT + 0x200, FLAT + 0x300, b"Flat");
    write_minimal_snapshot_window(
        &mut runner.bus,
        SPARSE,
        SPARSE + 0x200,
        SPARSE + 0x300,
        b"Sparse",
    );
    runner.dispatcher.window_list.replace(vec![FLAT, SPARSE]);

    assert_eq!(
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u8(SPARSE + 110),
        Some(0xFF)
    );
    let result = runner.window_stack_snapshot();
    assert_eq!(
        result
            .iter()
            .map(|window| window.title.as_str())
            .collect::<Vec<_>>(),
        ["Flat", "Sparse"]
    );
    assert!(result.iter().all(|window| window.visible));
}

#[test]
fn mixed_isa_window_snapshot_uses_shared_process_state() {
    use crate::guest_call::{ExecutionTaskId, NativeThreadContext, ThreadStorage};
    use crate::guest_procedure::GuestIsa;
    use crate::loader::ppc::tests::synthetic_pef_with_import;
    const CLASSIC_CODE: u32 = 0x0031_0000;
    const CLASSIC_STACK: u32 = 0x0031_1000;
    let (mut runner, window) = create_classic_snapshot_window(b"Mixed");
    let mut companion = load_pef_application(&synthetic_pef_with_import(b"YieldToThread")).unwrap();
    companion.cpu.gpr[3] = ExecutionTaskId::APPLICATION.thread_id();
    let worker_context = companion.cpu.capture_execution_context();
    runner.init_ppc_companion(companion);
    let calls = runner.dispatcher.guest_calls.shared_handle();
    assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::M68k));
    let worker = calls
        .create_native_thread(
            NativeThreadContext {
                context: worker_context,
            },
            ThreadStorage::default(),
            false,
            |_| true,
        )
        .unwrap();

    let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
    context
        .adapter_mut()
        .memory
        .write_u8(window + 110, 0x7F)
        .unwrap();
    context.adapter_mut().window_list.replace(vec![window]);
    assert!(runner.native.restore(context).is_ok());
    assert_eq!(runner.bus.read_byte(window + 110), 0x7F);
    assert_eq!(runner.dispatcher.window_list, [window]);

    for (index, word) in [
        0x303c, 0x0205, // MOVE.W #YieldToThread,D0
        0xabf2, // ThreadDispatch to the native worker
        0x60fe, // BRA.S -2 after the native import yields back
    ]
    .into_iter()
    .enumerate()
    {
        runner.bus.write_word(CLASSIC_CODE + index as u32 * 2, word);
    }
    runner.bus.write_long(CLASSIC_STACK, worker.thread_id());
    runner.bus.write_word(CLASSIC_STACK + 4, 0xBEEF);
    runner.m68k.cpu.write_reg(Register::PC, CLASSIC_CODE);
    runner.m68k.cpu.write_reg(Register::A7, CLASSIC_STACK);

    let (steps, running) = runner.run_steps(64, None);
    assert!(steps > 0 && running);
    assert_eq!(calls.current_task(), worker);
    assert!(calls.has_pending_task_handoff());
    let (steps, running) = runner.run_steps(64, None);
    assert!(steps > 0 && running);
    assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
    assert_eq!(runner.bus.read_word(CLASSIC_STACK + 4), 0);
    assert!(!calls.has_pending_task_handoff());
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), CLASSIC_CODE + 6);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), CLASSIC_STACK + 4);
    let companion = runner.native.companion().unwrap();
    assert_eq!(companion.cpu.gpr[3], 0);
    // synthetic_code calls the import trap with bctrl at +12, so the
    // native yield saves its successful return at the encoded LR (+16).
    assert_eq!(companion.cpu.pc, PPC_CODE_BASE + 16);

    let result = runner.window_stack_snapshot();
    assert_eq!(runner.bus.read_byte(window + 110), 0x7F);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].title, "Mixed");
    assert!(result[0].visible);
    assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
}

#[test]
fn window_snapshot_poll_is_read_only() {
    let (mut runner, window) = create_native_snapshot_window(b"Read only");
    configure_snapshot_window(&mut runner, window, true);
    let list = runner.dispatcher.window_list.windows();
    let events = runner.event_manager_snapshot();
    let m68k = CpuArchitecturalSnapshot::capture(&runner.m68k.cpu.core);
    let calls = runner.dispatcher.guest_calls.shared_handle();
    let calls_before = calls.clone();
    let native = &runner.native.application().unwrap().cpu;
    let native_context = native.capture_execution_context();
    let native_alignment = native.alignment_policy;
    let native_time = native.time_base();
    let native_reservation = native.reservation_address();

    runner.bus.begin_uncapped_write_probe();
    let first = runner.window_stack_snapshot();
    let second = runner.window_stack_snapshot();
    assert!(runner.bus.finish_write_probe_unchanged());
    assert_eq!(first, second);
    assert_eq!(runner.dispatcher.window_list, list.as_slice());
    assert_eq!(runner.event_manager_snapshot(), events);
    assert_eq!(
        CpuArchitecturalSnapshot::capture(&runner.m68k.cpu.core),
        m68k
    );
    assert_eq!(calls, calls_before);
    let after = &runner.native.application().unwrap().cpu;
    assert_eq!(
        after.capture_execution_context().architectural(),
        native_context.architectural()
    );
    assert_eq!(after.alignment_policy, native_alignment);
    assert_eq!(after.time_base(), native_time);
    assert_eq!(after.reservation_address(), native_reservation);
}
