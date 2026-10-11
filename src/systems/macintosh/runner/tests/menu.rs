use super::*;
use crate::guest_call::CooperativeThread;
use crate::loader::{Code0Header, LoadedApp};
use crate::runner::MenuBarPolicy;
use crate::systems::macintosh::runner::{NativeEngineRole, UiThemeId};

#[test]
fn ppc_initialization_attaches_both_cpu_adapters_to_one_native_menu_selection() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    assert!(runner
        .dispatcher
        .pending_native_menu_selection
        .stage((128, 2)));
    let ppc_app = runner.native.application_mut().expect("PPC app");
    assert_eq!(
        ppc_app
            .toolbox_startup
            .pending_native_menu_selection
            .snapshot(),
        Some((128, 2))
    );
    assert_eq!(
        ppc_app.toolbox_startup.pending_native_menu_selection.take(),
        Some((128, 2))
    );
    assert!(runner.dispatcher.pending_native_menu_selection.is_none());

    assert!(ppc_app
        .toolbox_startup
        .pending_native_menu_selection
        .stage((129, 3)));
    assert_eq!(
        runner.dispatcher.pending_native_menu_selection.take(),
        Some((129, 3))
    );
    assert!(ppc_app
        .toolbox_startup
        .pending_native_menu_selection
        .is_none());
}

#[test]
fn native_menu_select_observes_68k_disable_item_after_mdef_returns() {
    use crate::loader::ppc::tests::cross_abi_menu_select_fixture;
    use crate::memory::globals::addr;

    const CALLBACK_VALUE: u32 = 0x68c0_ab1e;
    const ROOT_MENU_ID: i16 = 140;
    const TARGET_ITEM: i16 = 2;

    let fixture = cross_abi_menu_select_fixture();
    let root_menu = fixture.root_menu;
    let root_record = fixture.root_record;
    let callback_marker = fixture.callback_marker;
    let title_h = fixture.title_h;
    let app = LoadedApp::from_ppc(fixture.app);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    // This fixture pre-renders classic menu pixels before attaching to the runner.
    runner.set_ui_theme(UiThemeId::ClassicSystem7);
    runner.init_app(&app);
    let menus = runner.guest_menu_snapshot();
    assert!(menus.requires_guest_menu_rendering());
    assert!(menus.menus.iter().any(|menu| {
        menu.id == 141 && menu.hierarchical && !menu.standard_definition
    }));

    let framebuffer_before = {
        let native = runner.native.application_mut().expect("native app");
        let front = native.current_front_buffer().expect("front buffer");
        let mut framebuffer = Vec::with_capacity((front.row_bytes * front.height) as usize);
        let mut row = vec![0; front.row_bytes as usize];
        for y in 0..front.height {
            native
                .read_front_buffer_row(front, y, &mut row)
                .expect("front-buffer row");
            framebuffer.extend_from_slice(&row);
        }
        framebuffer
    };
    assert_ne!(
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(root_record + 10)
            .unwrap()
            & (1 << TARGET_ITEM),
        0,
        "the target row must begin enabled"
    );

    runner.push_canonical_mouse_down(10, title_h);
    let root_rect = (0..16)
        .find_map(|_| {
            let (_, running) = runner.run_steps(512, None);
            assert!(
                running,
                "native MenuSelect halted before opening the root menu"
            );
            runner
                .process_context
                .menu_tracking()
                .filter(|tracking| tracking.menu_handle == root_menu)
                .map(|tracking| tracking.dropdown_rect())
        })
        .expect("native MenuSelect should retain the canonical root handle");

    runner
        .dispatcher
        .set_mouse_position(root_rect.0 + 8, root_rect.1 + 16);
    runner.sync_mouse_position_lowmem();
    let callback_completed = (0..32).any(|_| {
        let (_, running) = runner.run_steps(512, None);
        assert!(
            running,
            "native MenuSelect halted during the 68k MDEF callback"
        );
        let callback_value = runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(callback_marker);
        callback_value == Some(CALLBACK_VALUE) && runner.dispatcher.guest_calls.depth() == 0
    });
    assert!(
        callback_completed,
        "the real 68k MDEF callback did not return"
    );
    let tracking = runner
        .process_context
        .menu_tracking()
        .expect("native interaction should remain retained");
    assert_eq!(tracking.menu_handle, root_menu);
    assert!(runner.guest_menu_snapshot().requires_guest_menu_rendering());
    let framebuffer_during = {
        let native = runner.native.application_mut().expect("native app");
        let front = native.current_front_buffer().expect("front buffer");
        let mut framebuffer = Vec::with_capacity((front.row_bytes * front.height) as usize);
        let mut row = vec![0; front.row_bytes as usize];
        for y in 0..front.height {
            native.read_front_buffer_row(front, y, &mut row).unwrap();
            framebuffer.extend_from_slice(&row);
        }
        framebuffer
    };
    assert_ne!(
        framebuffer_during, framebuffer_before,
        "guest menu pixels must remain in the front buffer for GPUI fallback"
    );
    assert_eq!(
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(root_menu),
        Some(root_record),
        "a fixed-size 68k mutation must preserve the native handle allocation"
    );
    assert_eq!(
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(root_record + 10)
            .unwrap()
            & (1 << TARGET_ITEM),
        0,
        "the 68k DisableItem trap must mutate the live native MenuInfo"
    );

    let target_v = root_rect.0 + 24;
    let target_h = root_rect.1 + 16;
    runner.dispatcher.set_mouse_position(target_v, target_h);
    runner.sync_mouse_position_lowmem();
    let raw_choice = (u32::from(ROOT_MENU_ID as u16) << 16) | u32::from(TARGET_ITEM as u16);
    let native_observed_disabled_row = (0..16).any(|_| {
        let (_, running) = runner.run_steps(512, None);
        assert!(running, "native MenuSelect halted before the mouse release");
        runner.bus.read_long(addr::MENU_DISABLE) == raw_choice
            && runner
                .process_context
                .menu_tracking()
                .is_some_and(|tracking| {
                    tracking.menu_handle == root_menu && tracking.highlighted_item == 0
                })
    });
    assert!(
        native_observed_disabled_row,
        "native tracking did not reread the live disabled enableFlags"
    );

    runner.push_canonical_mouse_up(target_v, target_h);
    for _ in 0..16 {
        let (_, running) = runner.run_steps(512, None);
        if !running {
            break;
        }
    }
    assert!(runner.is_halted());
    assert!(runner.process_context.menu_tracking().is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());

    let native = runner
        .native
        .application_mut()
        .expect("native app retained");
    assert_eq!(native.cpu.gpr[3], 0, "disabled rows cannot be selected");
    let framebuffer_after = {
        let front = native.current_front_buffer().expect("front buffer");
        let mut framebuffer = Vec::with_capacity((front.row_bytes * front.height) as usize);
        let mut row = vec![0; front.row_bytes as usize];
        for y in 0..front.height {
            native
                .read_front_buffer_row(front, y, &mut row)
                .expect("front-buffer row");
            framebuffer.extend_from_slice(&row);
        }
        framebuffer
    };
    assert_eq!(
        framebuffer_after, framebuffer_before,
        "the retained interaction must restore its saved presentation"
    );

    native.cpu.pc = native.entry_pc;
    native.cpu.lr = PPC_HALT_PC;
    native.imports[0].dispatcher_target = PpcImportDispatcherTarget::MenuChoice;
    let probe = runner
        .process_context
        .with_memory_and_cfm(|memory_manager, cfm| {
            native.run_with_process_services(64, false, false, memory_manager, cfm)
        });
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(native.cpu.gpr[3], raw_choice);
}

#[test]
fn native_menu_select_observes_growing_68k_append_menu_after_mdef_returns() {
    use crate::loader::ppc::tests::cross_abi_menu_select_fixture;

    const APPEND_STRING: u32 = crate::loader::ppc::PPC_DATA_BASE + 0x7300;
    const CALLBACK_VALUE: u32 = 0x68c0_ab1e;
    const ROOT_MENU_ID: i16 = 140;
    const TARGET_ITEM: i16 = 2;
    const APPENDED_TEXT: &[u8] =
        b"Cross-ABI growth must remain visible through the original native MenuHandle";

    let mut fixture = cross_abi_menu_select_fixture();
    let root_menu = fixture.root_menu;
    let original_record = fixture.root_record;
    let original_handle_record = fixture
        .app
        .handles()
        .iter()
        .copied()
        .find(|record| record.handle == root_menu)
        .expect("native root-menu allocation");
    let callback_marker = fixture.callback_marker;
    let title_h = fixture.title_h;

    let mut append_string = Vec::with_capacity(APPENDED_TEXT.len() + 1);
    append_string.push(APPENDED_TEXT.len() as u8);
    append_string.extend_from_slice(APPENDED_TEXT);
    fixture.app.memory.add_region(APPEND_STRING, append_string);

    // The real 68k MDEF grows the native MenuInfo with AppendMenu. A
    // relocatable block can move during SetHandleSize, but its master
    // pointer and Handle identity remain authoritative. Inside Macintosh:
    // Memory (1992), pp. 1-16--1-17 and 2-40--2-41.
    let mut mdef = Vec::new();
    mdef.extend_from_slice(&0x4ab9u16.to_be_bytes()); // TST.L marker
    mdef.extend_from_slice(&callback_marker.to_be_bytes());
    mdef.extend_from_slice(&0x660eu16.to_be_bytes()); // BNE.S after AppendMenu
    mdef.extend_from_slice(&0x2f3cu16.to_be_bytes()); // MOVE.L #rootMenu,-(SP)
    mdef.extend_from_slice(&root_menu.to_be_bytes());
    mdef.extend_from_slice(&0x2f3cu16.to_be_bytes()); // MOVE.L #appendString,-(SP)
    mdef.extend_from_slice(&APPEND_STRING.to_be_bytes());
    mdef.extend_from_slice(&0xa933u16.to_be_bytes()); // AppendMenu
    mdef.extend_from_slice(&0x23fcu16.to_be_bytes()); // MOVE.L #value,marker
    mdef.extend_from_slice(&CALLBACK_VALUE.to_be_bytes());
    mdef.extend_from_slice(&callback_marker.to_be_bytes());
    mdef.extend_from_slice(&0x4e74u16.to_be_bytes()); // RTD #18
    mdef.extend_from_slice(&0x0012u16.to_be_bytes());
    fixture.app.memory.add_region(fixture.mdef_entry, mdef);

    let app = LoadedApp::from_ppc(fixture.app);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    runner.push_canonical_mouse_down(10, title_h);
    let root_rect = (0..16)
        .find_map(|_| {
            runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
            let (_, running) = runner.run_steps(512, None);
            assert!(
                running,
                "native MenuSelect halted before opening the root menu"
            );
            runner
                .process_context
                .menu_tracking()
                .filter(|tracking| tracking.menu_handle == root_menu)
                .map(|tracking| tracking.dropdown_rect())
        })
        .expect("native MenuSelect should retain the canonical root handle");

    runner
        .dispatcher
        .set_mouse_position(root_rect.0 + 8, root_rect.1 + 16);
    runner.sync_mouse_position_lowmem();
    let callback_completed = (0..32).any(|_| {
        runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
        let (_, running) = runner.run_steps(512, None);
        assert!(
            running,
            "native MenuSelect halted during the growing 68k callback"
        );
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(callback_marker)
            == Some(CALLBACK_VALUE)
            && runner.dispatcher.guest_calls.depth() == 0
    });
    assert!(
        callback_completed,
        "the growing real 68k MDEF callback did not return"
    );

    let native = runner
        .native
        .application_mut()
        .expect("native app retained");
    let relocated_record = native
        .memory
        .read_u32_be(root_menu)
        .expect("live native root-menu master pointer");
    let handle_record = native
        .handles()
        .iter()
        .copied()
        .find(|record| record.handle == root_menu)
        .expect("updated native root-menu allocation");
    assert_eq!(handle_record.ptr, relocated_record);
    assert_eq!(handle_record.handle, root_menu);
    assert_eq!(handle_record.size, handle_record.capacity);
    assert!(handle_record.size > original_handle_record.size);
    assert_ne!(
        relocated_record, original_record,
        "the fixture must force relocation"
    );
    assert_eq!(
        runner.process_context.handle_for_ptr(relocated_record),
        Some(root_menu),
        "the process Memory Manager must publish the relocated native handle"
    );
    let mut menu_bytes = vec![0; handle_record.size as usize];
    native
        .memory
        .read_bytes_into(relocated_record, &mut menu_bytes)
        .expect("complete relocated MenuInfo bytes");
    let items = crate::menu_manager::MenuItems::decode(&menu_bytes)
        .expect("relocated native MenuInfo remains decodable");
    assert!(
        items.items.iter().any(|item| item.text == APPENDED_TEXT),
        "native decoding must observe the item appended by the 68k callback"
    );
    assert_eq!(native.last_mem_error(), 0);
    assert_eq!(
        runner.bus.read_word(crate::memory::globals::addr::MEM_ERR),
        0
    );

    let target_v = root_rect.0 + 24;
    let target_h = root_rect.1 + 16;
    runner.dispatcher.set_mouse_position(target_v, target_h);
    runner.sync_mouse_position_lowmem();
    let target_observed = (0..16).any(|_| {
        runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
        let (_, running) = runner.run_steps(512, None);
        assert!(running, "native MenuSelect halted before the mouse release");
        runner
            .process_context
            .menu_tracking()
            .is_some_and(|tracking| {
                tracking.menu_handle == root_menu && tracking.highlighted_item == TARGET_ITEM
            })
    });
    assert!(
        target_observed,
        "native tracking did not reach the live regular row"
    );
    runner.push_canonical_mouse_up(target_v, target_h);
    for _ in 0..128 {
        runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
        let (_, running) = runner.run_steps(512, None);
        if !running {
            break;
        }
    }
    assert!(runner.is_halted());
    assert!(runner.process_context.menu_tracking().is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(
        runner.native.application().unwrap().cpu.gpr[3],
        (u32::from(ROOT_MENU_ID as u16) << 16) | u32::from(TARGET_ITEM as u16),
        "native MenuSelect must continue and return the live regular row"
    );
}

struct ClassicPowerPcMdefFixture {
    runner: FixtureRunner,
    menu: u32,
    record: u32,
    marker: u32,
    entry: u32,
    stack: u32,
}

fn classic_powerpc_mdef_fixture() -> ClassicPowerPcMdefFixture {
    classic_powerpc_mdef_fixture_with_tick_identity(false)
}

fn classic_powerpc_mdef_fixture_with_tick_identity(
    already_shared_tick: bool,
) -> ClassicPowerPcMdefFixture {
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_POWERPC_ISA,
        ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
    };
    const MENU: u32 = 0x0030_0000;
    const RECORD: u32 = MENU + 0x100;
    const MDEF: u32 = MENU + 0x200;
    const DESCRIPTOR: u32 = MENU + 0x300;
    const TVECTOR: u32 = MENU + 0x400;
    const MARKER: u32 = MENU + 0x500;
    const ENTRY: u32 = MENU + 0x600;
    const STACK: u32 = 0x0070_0000;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    let mut native = halted_ppc_app_with_sound(PpcSoundState::default())
        .ppc
        .take()
        .unwrap();
    native
        .memory
        .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
    native.memory.add_region(
        CALLBACK,
        [
            0x8124_0000u32, // lwz r9,0(r4): live MenuHandle
            0x3940_01b0,    // li r10,432
            0xb149_0002,    // sth r10,menuWidth(r9)
            0x3940_007b,    // li r10,123
            0xb149_0004,    // sth r10,menuHeight(r9)
            0x3d20_0000 | (MARKER >> 16),
            0x6129_0000 | (MARKER & 0xffff),
            0x8149_0000, // lwz r10,0(r9)
            0x394a_0001, // addi r10,r10,1
            0x9149_0000, // stw r10,0(r9)
            0x3940_0002, // li r10,2
            0xb147_0000, // sth r10,0(r7): chosen item
            0x4e80_0020, // blr
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    let classic = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007f_ffc0,
        size_resource: None,
    };
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    if already_shared_tick {
        native.tick_state = runner
            .process_context
            .migrated_handles()
            .ticks
            .shared_handle();
    }
    runner.stage_ppc_companion(native);
    runner.init_app(&classic);
    runner.bus.write_long(MENU, RECORD);
    runner.bus.write_word(RECORD, 140);
    runner.bus.write_long(RECORD + 6, MDEF);
    runner.bus.write_long(RECORD + 10, u32::MAX);
    runner.bus.write_long(MDEF, DESCRIPTOR);
    runner
        .bus
        .write_word(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
    runner
        .bus
        .write_byte(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION);
    runner.bus.write_word(DESCRIPTOR + 10, 0);
    let routine = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    runner.bus.write_long(routine, 0x0000_ff80);
    runner.bus.write_byte(
        routine + ROUTINE_RECORD_ISA_OFFSET,
        ROUTINE_RECORD_POWERPC_ISA,
    );
    runner.bus.write_word(
        routine + ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_FLAG_USE_NATIVE_ISA,
    );
    runner
        .bus
        .write_long(routine + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR);
    runner.bus.write_long(TVECTOR, CALLBACK);
    runner.bus.write_long(TVECTOR + 4, 0);
    runner.bus.write_word(ENTRY, 0xA948); // CalcMenuSize
    runner.bus.write_word(ENTRY + 2, 0x60fe); // park after the call
    runner.bus.write_long(STACK, MENU);
    runner.m68k.cpu.write_reg(Register::PC, ENTRY);
    runner.m68k.cpu.write_reg(Register::A7, STACK);
    ClassicPowerPcMdefFixture {
        runner,
        menu: MENU,
        record: RECORD,
        marker: MARKER,
        entry: ENTRY,
        stack: STACK,
    }
}

#[test]
fn nested_classic_mdef_preserves_its_wrapper_arguments_and_caller_stack() {
    let ClassicPowerPcMdefFixture {
        mut runner,
        menu,
        record,
        marker,
        entry,
        stack,
    } = classic_powerpc_mdef_fixture();
    let inner = menu + 0x1000;
    let inner_record = inner + 0x100;
    let inner_handle = inner + 0x200;
    let outer_code = menu + 0x2000;
    let inner_code = outer_code + 0x200;
    let outer_handle = runner.bus.read_long(record + 6);
    runner.bus.write_long(outer_handle, outer_code);
    runner.bus.write_long(inner, inner_record);
    runner.bus.write_word(inner_record, 141);
    runner.bus.write_long(inner_record + 6, inner_handle);
    runner.bus.write_long(inner_record + 10, u32::MAX);
    runner.bus.write_long(inner_handle, inner_code);
    for (address, words) in [
        (
            outer_code,
            vec![
                0x206f,
                12, // MOVEA.L menuRect(SP),A0
                0x30bc,
                0x1122, // MOVE.W #$1122,(A0)
                0x2f08, // retain outer rectangle pointer
                0x2f3c,
                (inner >> 16) as u16,
                inner as u16,
                0xa948, // nested CalcMenuSize
                0x205f, // restore outer pointer
                0x33d0,
                (marker >> 16) as u16,
                marker as u16,
                0x4e74,
                18, // RTD #18
            ],
        ),
        (inner_code, vec![0x206f, 12, 0x30bc, 0x3344, 0x4e74, 18]),
    ] {
        for (index, word) in words.into_iter().enumerate() {
            runner.bus.write_word(address + index as u32 * 2, word);
        }
    }
    for _ in 0..8 {
        let (_, running) = runner.run_steps(128, None);
        assert!(running);
    }
    assert_eq!(runner.bus.read_word(marker), 0x1122);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), entry + 2);
    assert!(runner.dispatcher.guest_calls.is_empty());
}

fn fire_menu_test_timer(runner: &mut FixtureRunner, menu: u32, marker: u32) {
    let timer = menu + 0x3000;
    for (index, word) in [
        0x33fc,
        1,
        ((marker + 4) >> 16) as u16,
        (marker + 4) as u16,
        0x4e75,
    ]
    .into_iter()
    .enumerate()
    {
        runner.bus.write_word(timer + index as u32 * 2, word);
    }
    runner.dispatcher.timer_tasks.push(TimerTask {
        task_ptr: timer + 0x100,
        architecture: CallbackTaskArchitecture::M68k,
        extended: false,
        callback: timer,
        active: true,
        fire_at_tick: 1,
        fire_at_subtick: 1_000_000,
        last_fired_tick: None,
    });
    runner.fire_timer_tasks(1);
}

#[test]
fn timer_at_classic_mdef_return_preserves_callback_code_and_stack() {
    let ClassicPowerPcMdefFixture {
        mut runner,
        menu,
        marker,
        entry,
        stack,
        ..
    } = classic_powerpc_mdef_fixture();
    let frame = stack + 4 - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION;
    let mut reached_return = false;
    for _ in 0..512 {
        let return_instruction = if runner.bus.read_word(frame + 48) == 0x4e74 {
            frame + 48
        } else {
            frame + 54
        };
        if runner.m68k.cpu.read_reg(Register::PC) == return_instruction {
            reached_return = true;
            break;
        }
        let (_, running) = runner.run_steps(1, None);
        assert!(running);
    }
    assert!(
        reached_return,
        "callback must reach its final return instruction"
    );
    fire_menu_test_timer(&mut runner, menu, marker);
    assert!(runner.active_interrupt_callback.is_some());
    for _ in 0..8 {
        let (_, running) = runner.run_steps(128, None);
        assert!(running);
    }
    assert_eq!(runner.bus.read_word(marker + 4), 1);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), entry + 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert!(runner.active_interrupt_callback.is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn classic_calc_menu_size_executes_powerpc_mdef_and_resumes_once() {
    let ClassicPowerPcMdefFixture {
        mut runner,
        record,
        marker,
        entry,
        stack,
        ..
    } = classic_powerpc_mdef_fixture();
    for _ in 0..8 {
        let (_, running) = runner.run_steps(128, None);
        assert!(running);
    }
    assert_eq!(
        runner.bus.read_long(marker),
        1,
        "PowerPC MDEF must run exactly once"
    );
    assert_eq!(runner.bus.read_word(record + 2), 432);
    assert_eq!(runner.bus.read_word(record + 4), 123);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), entry + 2);
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn classic_menu_select_retains_powerpc_mdef_until_mouse_release() {
    run_classic_menu_select_with_powerpc_mdef(false);
}

#[test]
fn timer_after_classic_mdef_return_preserves_pending_tracking_results() {
    run_classic_menu_select_with_powerpc_mdef(true);
}

fn run_classic_menu_select_with_powerpc_mdef(interrupt: bool) {
    run_classic_menu_select_with_powerpc_mdef_identity(interrupt, false);
}

pub(crate) fn run_classic_menu_select_with_powerpc_mdef_identity(
    interrupt: bool,
    already_shared_tick: bool,
) {
    use crate::memory::globals::addr;
    let ClassicPowerPcMdefFixture {
        mut runner,
        menu,
        record,
        marker,
        entry,
        stack,
    } = classic_powerpc_mdef_fixture_with_tick_identity(already_shared_tick);
    let migrated_handles = runner.process_context.migrated_handles();
    runner.dispatcher.menu_bar_hidden = false;
    runner.bus.write_word(addr::MBAR_HEIGHT, 20);
    runner.bus.write_word(addr::MENU_FLASH, 0);
    runner.bus.write_word(record + 2, 80);
    runner.bus.write_word(record + 4, 32);
    runner.bus.write_bytes(
        record + 14,
        b"\x06Custom\x01A\x00\x00\x00\x00\x01B\x00\x00\x00\x00\x00",
    );
    runner.bus.write_word(stack, 0);
    runner.bus.write_long(stack + 2, menu);
    runner
        .dispatcher
        .dispatch_menu(true, 0x135, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap()
        .unwrap();
    runner.dispatcher.draw_menu_bar_to_fb(&mut runner.bus);
    let original_port = *runner.dispatcher.current_port;
    runner.bus.write_word(entry, 0xA93D);
    runner.bus.write_word(stack, 10);
    runner.bus.write_word(stack + 2, 16);
    runner.bus.write_long(stack + 4, 0);
    runner.m68k.cpu.write_reg(Register::A7, stack);
    runner.push_canonical_mouse_down(10, 16);
    if interrupt {
        let mut parked = false;
        for _ in 0..512 {
            if runner.m68k.cpu.read_reg(Register::PC)
                == stack - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION + 52
                && runner.m68k.cpu.read_reg(Register::A7)
                    == stack - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
            {
                parked = true;
                break;
            }
            assert!(runner.run_steps(1, None).1);
        }
        assert!(parked, "MDEF return must keep its result reservation live");
        fire_menu_test_timer(&mut runner, menu, marker);
    }

    for _ in 0..8 {
        assert!(runner.run_steps(128, None).1);
    }
    let rect = runner
        .process_context
        .menu_tracking()
        .expect("classic tracking remains active")
        .dropdown_rect();
    assert!(
        runner.bus.read_long(marker) > 0,
        "PowerPC draw callback ran"
    );
    let (v, h) = (rect.0 + 24, rect.1 + 16);
    runner.dispatcher.set_mouse_position(v, h);
    for _ in 0..8 {
        assert!(runner.run_steps(128, None).1);
    }
    runner.push_canonical_mouse_up(v, h);
    for _ in 0..16 {
        assert!(runner.run_steps(128, None).1);
        if runner.process_context.menu_tracking().is_none()
            && runner.dispatcher.guest_calls.is_empty()
        {
            break;
        }
    }
    assert!(runner.process_context.menu_tracking().is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert!(runner
        .native
        .adapter_mut(NativeEngineRole::Companion)
        .expect("mixed callback companion retained")
        .is_constructed_from_migrated_handles(&migrated_handles));
    assert_eq!(runner.bus.read_long(stack + 4), (140 << 16) | 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert_eq!(*runner.dispatcher.current_port, original_port);
    if interrupt {
        assert_eq!(runner.bus.read_word(marker + 4), 1);
    }

    let completed_result = runner.bus.read_long(stack + 4);
    let completed_sp = runner.m68k.cpu.read_reg(Register::A7);
    let completed_pc = runner.m68k.cpu.read_reg(Register::PC);
    let completed_marker = runner.bus.read_long(marker);
    assert!(runner.run_steps(16, None).1);
    assert_eq!(runner.bus.read_long(stack + 4), completed_result);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), completed_sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), completed_pc);
    assert_eq!(runner.bus.read_long(marker), completed_marker);
    assert!(runner.process_context.menu_tracking().is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn classic_menu_wait_resumes_without_refiring_new_trap_patch() {
    for auto_pop in [false, true] {
        for custom in [false, true] {
            run_menu_patch_during_tracking(auto_pop, custom, false, false);
        }
    }
}

#[test]
fn native_menu_hook_runs_classic_guest_code_and_releases_ownership() {
    use crate::loader::ppc::tests::native_menu_hook_fixture;
    use crate::memory::globals::addr;
    for cancel in [false, true] {
        let (mut native, _, _) = native_menu_hook_fixture();
        native.cpu.gpr[3] = (10 << 16) | 12;
        native.memory.write_u16_be(addr::MENU_FLASH, 0).unwrap();
        let original_sp = native.cpu.gpr[1];
        let original_return = native.cpu.lr;
        let app = LoadedApp::from_ppc(native);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_ui_theme(UiThemeId::ClassicSystem7);
        runner.init_app(&app);
        let code = 0x0030_8000;
        let marker = code + 0x100;
        for (index, word) in [
            0x42a7, // CLR.L -(SP): inner result
            0x2f3c,
            0x01f4,
            0x01f4, // inner MenuSelect outside the menu bar
            0xa93d,
            0x23df,
            ((marker + 4) >> 16) as u16,
            (marker + 4) as u16,
            0x52b9,
            (marker >> 16) as u16,
            marker as u16,
            0x4e75,
        ]
        .into_iter()
        .enumerate()
        {
            runner.bus.write_word(code + index as u32 * 2, word);
        }
        runner.bus.write_long(marker + 4, u32::MAX);
        runner.bus.write_long(0x0a30, code);
        runner.push_canonical_mouse_down(10, 12);
        for _ in 0..8 {
            assert!(runner.run_steps(128, None).1);
        }
        assert!(
            runner.bus.read_long(marker) > 0,
            "native MenuSelect invoked the classic hook"
        );
        assert!(runner.process_context.menu_tracking().is_some());
        assert_eq!(
            runner.bus.read_long(marker + 4),
            0,
            "nested no-hit MenuSelect returned independently"
        );
        if cancel {
            runner.push_canonical_mouse_up(500, 500);
        } else {
            runner.push_canonical_mouse_up(28, 20);
        }
        for _ in 0..32 {
            runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
            if !runner.run_steps(128, None).1 {
                break;
            }
        }
        assert!(runner.is_halted());
        assert!(runner.process_context.menu_tracking().is_none());
        assert!(runner.dispatcher.guest_calls.is_empty());
        let native = runner.native.application_mut().unwrap();
        assert_eq!(native.cpu.pc, original_return);
        assert_eq!(native.cpu.gpr[1], original_sp);
        assert_eq!(native.cpu.gpr[3], if cancel { 0 } else { (128 << 16) | 1 });
    }
}

#[test]
fn classic_menu_hook_uses_owned_stack_frame_and_restores_registers() {
    for auto_pop in [false, true] {
        for native_hook in [false, true] {
            run_menu_patch_during_tracking(auto_pop, false, true, native_hook);
        }
    }
}

fn run_menu_patch_during_tracking(auto_pop: bool, custom: bool, hook: bool, native_hook: bool) {
    use crate::memory::globals::addr;
    let ClassicPowerPcMdefFixture {
        mut runner,
        menu,
        record,
        marker,
        entry,
        stack,
    } = classic_powerpc_mdef_fixture();
    runner.dispatcher.menu_bar_hidden = false;
    runner.bus.write_word(addr::MBAR_HEIGHT, 20);
    runner.bus.write_word(addr::MENU_FLASH, 0);
    if !custom {
        let code = runner
            .bus
            .alloc(crate::menu_manager::STANDARD_MENU_DEFINITION_SHIM.len() as u32);
        runner
            .bus
            .write_bytes(code, &crate::menu_manager::STANDARD_MENU_DEFINITION_SHIM);
        let handle = runner.bus.alloc(4);
        runner.bus.write_long(handle, code);
        runner.bus.write_long(record + 6, handle);
    }
    runner.bus.write_word(record + 2, 80);
    runner.bus.write_word(record + 4, 32);
    runner.bus.write_bytes(
        record + 14,
        b"\x06Custom\x01A\x00\x00\x00\x00\x01B\x00\x00\x00\x00\x00",
    );
    runner.bus.write_word(stack, 0);
    runner.bus.write_long(stack + 2, menu);
    runner
        .dispatcher
        .dispatch_menu(true, 0x135, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap()
        .unwrap();
    runner.dispatcher.draw_menu_bar_to_fb(&mut runner.bus);
    let original_port = *runner.dispatcher.current_port;
    let hook_port = runner.bus.alloc(170);
    let original_port_image = runner.bus.read_bytes(original_port, 170).to_vec();
    runner.bus.write_bytes(hook_port, &original_port_image);
    let parameters = stack + if auto_pop { 4 } else { 0 };
    let return_pc = entry + if auto_pop { 0x100 } else { 2 };
    runner.bus.write_word(return_pc, 0x60fe);
    runner
        .bus
        .write_word(entry, if auto_pop { 0xAD3D } else { 0xA93D });
    if auto_pop {
        runner.bus.write_long(stack, return_pc);
    }
    runner.bus.write_word(parameters, 10);
    runner.bus.write_word(parameters + 2, 16);
    runner.bus.write_long(parameters + 4, 0);
    runner.m68k.cpu.write_reg(Register::A7, stack);
    let hook_marker = runner.bus.alloc(4);
    let hook_after_yield_marker = runner.bus.alloc(4);
    let cooperative_switch = hook && !native_hook && !auto_pop;
    let mut hook_yield_resume_pc = None;
    let mut worker_yield_result = None;
    if hook {
        let mut hook_words = vec![
            0x7e63, // MOVEQ #99,D7
            0x2c7c, // MOVEA.L #value,A6
            0x1234, 0x5678,
        ];
        if !native_hook {
            hook_words.extend([
                0x2f3c,
                (hook_port >> 16) as u16,
                hook_port as u16,
                0xa873, // SetPort(hook_port)
            ]);
        }
        hook_words.extend([
            0x52b9, // ADDQ.L #1,marker
            (hook_marker >> 16) as u16,
            hook_marker as u16,
        ]);
        if cooperative_switch {
            hook_words.extend([
                0x558f, // SUBQ.L #2,SP: Pascal result word
                0x42a7, // CLR.L -(SP): synthetic suggested ThreadID
                0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                0xabf2, // ThreadDispatch
                0x548f, // ADDQ.L #2,SP: pop Pascal result
            ]);
            hook_words.extend([
                0x52b9, // ADDQ.L #1,after-yield marker
                (hook_after_yield_marker >> 16) as u16,
                hook_after_yield_marker as u16,
            ]);
        }
        hook_words.extend([
            0x5279, // ADDQ.W #1,menuWidth
            ((record + 2) >> 16) as u16,
            (record + 2) as u16,
            0x4e75,
        ]);
        let code = runner.bus.alloc((hook_words.len() * 2) as u32);
        if cooperative_switch {
            let trap_index = hook_words.iter().position(|word| *word == 0xabf2).unwrap();
            hook_yield_resume_pc = Some(code + (trap_index as u32 + 1) * 2);
        }
        for (index, word) in hook_words.into_iter().enumerate() {
            runner.bus.write_word(code + index as u32 * 2, word);
        }
        runner.bus.write_long(0x0a30, code);
        if native_hook {
            use crate::guest_procedure::{
                ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
                ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA,
                ROUTINE_RECORD_FLAGS_OFFSET, ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_POWERPC_ISA,
                ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            };
            let native_code = runner.bus.alloc(48);
            for (index, word) in [
                0x3ce0_0000 | (record >> 16),
                0x60e7_0000 | (record & 0xffff),
                0xa147_0002,
                0x394a_0001,
                0xb147_0002,
                0x3d00_0000 | (hook_marker >> 16),
                0x6108_0000 | (hook_marker & 0xffff),
                0x8128_0000,
                0x3929_0001,
                0x9128_0000,
                0x4e80_0020,
            ]
            .into_iter()
            .enumerate()
            {
                runner.bus.write_long(native_code + index as u32 * 4, word);
            }
            let descriptor = runner.bus.alloc(64);
            let tvector = descriptor + 48;
            let record = descriptor + ROUTINE_DESCRIPTOR_HEADER_SIZE;
            runner
                .bus
                .write_word(descriptor, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
            runner
                .bus
                .write_byte(descriptor + 2, ROUTINE_DESCRIPTOR_VERSION);
            runner.bus.write_byte(
                record + ROUTINE_RECORD_ISA_OFFSET,
                ROUTINE_RECORD_POWERPC_ISA,
            );
            runner.bus.write_word(
                record + ROUTINE_RECORD_FLAGS_OFFSET,
                ROUTINE_FLAG_USE_NATIVE_ISA,
            );
            runner
                .bus
                .write_long(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, tvector);
            runner.bus.write_long(tvector, native_code);
            runner.bus.write_long(tvector + 4, 0);
            runner.bus.write_long(0x0a30, descriptor);
        }

        runner.m68k.cpu.write_reg(Register::D7, 0x77777777);
        runner.m68k.cpu.write_reg(Register::A6, 0x66666666);
    }
    if hook && !native_hook {
        let hook_pointer = runner.bus.read_long(0x0a30);
        runner.bus.write_long(0x0a30, 0);
        runner.push_canonical_mouse_down(10, 16);
        for _ in 0..8 {
            assert!(runner.run_steps(128, None).1);
            if runner
                .dispatcher
                .menu_tracking
                .request_menu_hook(true)
                .is_some()
            {
                break;
            }
        }
        let key = runner
            .dispatcher
            .menu_tracking
            .request_menu_hook(true)
            .expect("held menu requests its classic hook");
        let call_depth = runner.dispatcher.guest_calls.depth();
        runner.bus.write_long(0x0a30, hook_pointer);
        let procedure = crate::guest_procedure::resolve_guest_procedure(
            &mut runner.bus,
            hook_pointer,
            0,
            None,
            GuestIsa::M68k,
            GuestIsa::M68k,
        )
        .expect("classic hook remains resolvable");
        assert_eq!(procedure.isa, GuestIsa::M68k);
        assert_eq!(procedure.entry, hook_pointer);
        let valid_sp = runner.m68k.cpu.read_reg(Register::A7);
        let frame_start = runner.bus.alloc(114);
        let frame_len = 114;
        runner
            .bus
            .protect_readonly_code(frame_start, frame_len as u32);
        assert!(runner.bus.is_guest_address_mapped(frame_start, frame_len));
        assert!(!runner.bus.is_guest_address_writable(frame_start, frame_len));
        let frame_snapshot = runner.bus.read_bytes(frame_start, frame_len).to_vec();
        runner
            .m68k
            .cpu
            .write_reg(Register::A7, frame_start + frame_len as u32);
        assert!(!runner.fire_menu_hook_proc(0xa93d));
        assert_eq!(
            runner.bus.read_bytes(frame_start, frame_len),
            frame_snapshot
        );
        assert_eq!(
            runner.dispatcher.menu_tracking.request_menu_hook(true),
            Some(key)
        );
        assert_eq!(runner.dispatcher.menu_tracking.context().classic_port, None);
        assert_eq!(runner.dispatcher.guest_calls.depth(), call_depth);
        runner.m68k.cpu.write_reg(Register::A7, valid_sp);
        if cooperative_switch {
            let worker = ExecutionTaskId::from_thread_id(3);
            let worker_entry = runner.bus.alloc(8);
            let worker_stack = runner.bus.alloc(64);
            let worker_sp = worker_stack + 58;
            for (index, word) in [
                0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                0xabf2, // ThreadDispatch back to the application
                0x60fe, // BRA.S -2 if no successor is runnable
            ]
            .into_iter()
            .enumerate()
            {
                runner.bus.write_word(worker_entry + index as u32 * 2, word);
            }
            runner.bus.write_long(worker_sp, 0);
            runner.bus.write_word(worker_sp + 4, 0xbeef);
            worker_yield_result = Some(worker_sp + 4);
            assert!(runner.dispatcher.guest_calls.register_task(worker));
            assert!(runner.dispatcher.guest_calls.set_thread_storage(
                worker,
                crate::guest_call::ThreadStorage {
                    stack_base: worker_stack,
                    stack_limit: worker_stack + 64,
                    ..Default::default()
                }
            ));
            assert!(runner.dispatcher.guest_calls.save_cooperative_context(
                worker,
                CooperativeThread {
                    a_regs: [0, 0, 0, 0, 0, 0, 0, worker_sp],
                    pc: worker_entry,
                    ..Default::default()
                }
            ));
            assert!(runner
                .dispatcher
                .guest_calls
                .set_scheduling_state(worker, crate::execution_kernel::ExecutionTaskState::Ready));
        }
        assert!(runner.fire_menu_hook_proc(0xa93d));
        assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), Some(key));
        assert!(runner
            .dispatcher
            .menu_tracking
            .context()
            .classic_port
            .is_some());
        if cooperative_switch {
            runner.bus.write_long(0x0a30, 0);
            let worker = ExecutionTaskId::from_thread_id(3);
            for _ in 0..32 {
                runner.run_steps(1, None);
                if runner.dispatcher.guest_calls.current_task() == worker {
                    break;
                }
            }
            assert_eq!(runner.dispatcher.guest_calls.current_task(), worker);
            assert_eq!(runner.bus.read_long(hook_marker), 1);
            assert_eq!(runner.bus.read_long(hook_after_yield_marker), 0);
            assert_eq!(runner.bus.read_byte(0x0172), 0);
            assert_eq!(*runner.dispatcher.current_port, hook_port);
            assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), None);
            assert!(runner.dispatcher.menu_tracking.menu_hook_is_pending(key));
            assert!(runner
                .dispatcher
                .menu_tracking
                .ready_call(GuestIsa::M68k)
                .is_none());
            let parked = runner
                .dispatcher
                .guest_calls
                .cooperative_context(ExecutionTaskId::APPLICATION)
                .expect("suspended hook context");
            assert_eq!(parked.pc, hook_yield_resume_pc.unwrap());
            for _ in 0..32 {
                runner.run_steps(1, None);
                if runner.dispatcher.guest_calls.current_task() == ExecutionTaskId::APPLICATION {
                    break;
                }
            }
            assert_eq!(
                runner.dispatcher.guest_calls.current_task(),
                ExecutionTaskId::APPLICATION
            );
            assert_eq!(runner.bus.read_word(worker_yield_result.unwrap()), 0);
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), parked.pc);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), parked.a_regs[7]);
            assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), Some(key));
            for _ in 0..32 {
                runner.run_steps(1, None);
                assert!(
                    runner.process_context.menu_tracking().is_some(),
                    "held root vanished before hook receipt consumption: task={:?} pc={:08x} button={:02x}",
                    runner.dispatcher.guest_calls.current_task(),
                    runner.m68k.cpu.read_reg(Register::PC),
                    runner.bus.read_byte(0x0172),
                );
                if runner.bus.read_long(hook_after_yield_marker) == 1
                    && runner.dispatcher.menu_tracking.menu_hook_key().is_none()
                {
                    break;
                }
            }
            assert_eq!(runner.bus.read_long(hook_after_yield_marker), 1);
            assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), None);
            assert!(runner.process_context.menu_tracking().is_some());
        }
    } else {
        runner.push_canonical_mouse_down(10, 16);
    }

    if !cooperative_switch {
        for _ in 0..8 {
            assert!(runner.run_steps(128, None).1);
        }
    }
    if hook && !native_hook {
        assert_eq!(*runner.dispatcher.current_port, hook_port);
        if cooperative_switch {
            assert_eq!(runner.bus.read_long(hook_after_yield_marker), 1);
        }
    }
    let rect = runner
        .process_context
        .menu_tracking()
        .expect("classic tracking remains active")
        .dropdown_rect();
    if custom {
        assert!(
            runner.bus.read_long(marker) > 0,
            "PowerPC draw callback ran"
        );
    }
    let (v, h) = (rect.0 + 24, rect.1 + 16);
    runner.dispatcher.set_mouse_position(v, h);
    for _ in 0..8 {
        assert!(runner.run_steps(128, None).1);
    }
    runner.push_canonical_mouse_up(v, h);
    let patch_marker = runner.bus.alloc(4);
    let patch = runner.bus.alloc(12);
    for (index, word) in [
        0x23fc,
        0,
        1,
        (patch_marker >> 16) as u16,
        patch_marker as u16,
        0x4e75,
    ]
    .into_iter()
    .enumerate()
    {
        runner.bus.write_word(patch + index as u32 * 2, word);
    }
    runner
        .dispatcher
        .install_trap_address(&mut runner.bus, 0xa93d, patch)
        .unwrap();
    for _ in 0..16 {
        assert!(runner.run_steps(128, None).1);
        assert_eq!(
            runner.bus.read_long(patch_marker),
            0,
            "a new MenuSelect patch intercepted the already-active interaction"
        );
        if runner.process_context.menu_tracking().is_none()
            && runner.dispatcher.guest_calls.is_empty()
        {
            break;
        }
    }
    assert!(runner.process_context.menu_tracking().is_none(),
            "tracking stayed live: pc={:08x}, sp={:08x}, patch={:08x}, marker={}, depth={}, pending={:?}",
            runner.m68k.cpu.read_reg(Register::PC), runner.m68k.cpu.read_reg(Register::A7), patch,
            runner.bus.read_long(patch_marker), runner.dispatcher.guest_calls.depth(),
            runner.dispatcher.menu_tracking.as_ref().and_then(|tracking| tracking.definition.as_ref()).and_then(|definition| definition.pending_invocation()));
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(runner.bus.read_long(parameters + 4), (140 << 16) | 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), parameters + 4);
    assert_eq!(*runner.dispatcher.current_port, original_port);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    if hook {
        assert!(runner.bus.read_long(hook_marker) > 0, "the guest hook ran");
        assert!(runner.bus.read_word(record + 2) > 80);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D7), 0x77777777);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A6), 0x66666666);
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert!(runner.active_interrupt_callback.is_none());
    }

    if auto_pop {
        runner.bus.write_long(stack, return_pc);
    }
    runner.m68k.cpu.write_reg(Register::PC, entry);
    runner.m68k.cpu.write_reg(Register::A7, stack);
    assert!(runner.run_steps(128, None).1);
    assert_eq!(
        runner.bus.read_long(patch_marker),
        1,
        "fresh entries must still honor the new patch"
    );
}

#[test]
fn menu_bar_policy_defaults_to_guest_control_and_supports_explicit_kiosk_modes() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::GuestControlled);
    assert!(
        runner.menu_bar_visible(),
        "library runners should permit guest menu chrome by default"
    );

    runner.set_menu_bar_policy(MenuBarPolicy::InitialKiosk);
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::InitialKiosk);
    assert!(!runner.menu_bar_visible());

    runner.set_menu_bar_visible(false);
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::ForceHidden);
    assert!(!runner.menu_bar_visible());

    runner.set_menu_bar_visible(true);
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::GuestControlled);
    assert!(runner.menu_bar_visible());
}

#[test]
fn initial_kiosk_releases_after_guest_hides_and_reveals_menu_bar() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_menu_bar_policy(MenuBarPolicy::InitialKiosk);
    runner.dispatcher.front_window = 1;

    runner
        .bus
        .write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
    runner.force_advance_guest_tick();
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::InitialKiosk);
    assert!(!runner.menu_bar_visible());

    runner
        .bus
        .write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    runner.force_advance_guest_tick();
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::GuestControlled);
    assert!(runner.menu_bar_visible());
}
