use super::*;
use crate::cpu::Register;
use crate::loader::ppc::PpcSoundState;
use crate::loader::{Code0Header, LoadedApp};
use crate::memory::globals::addr;
use crate::runner::ActiveInterruptCallbackSource;
use std::collections::HashMap;

#[test]
fn cursor_state_is_immediately_shared_between_cpu_adapters() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    let mut data = [0; 32];
    data[0] = 0x80;
    let mut mask = [0; 32];
    mask[0] = 0xc0;
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    ppc_app
        .cursor_state
        .install(crate::display::CursorImage::mono(data, mask, 3, 4));
    ppc_app.cursor_state.hide();

    assert_eq!(runner.dispatcher.cursor_level(), -1);
    assert!(!runner.dispatcher.cursor_visible());
    assert_eq!(runner.dispatcher.cursor_data(), Some((data, mask, 3, 4)));

    let hidden = runner.cursor_snapshot();
    assert_eq!(hidden.image, Some(crate::display::CursorImage::mono(data, mask, 3, 4)));
    assert_eq!(hidden.level, -1);
    assert!(!hidden.visible);
    assert_eq!(hidden.position, runner.dispatcher.mouse_position());

    runner.dispatcher.cursor_state.show();
    assert_eq!(
        runner
            .native
            .application()
            .expect("PPC app installed")
            .cursor_level(),
        0
    );
    let visible = runner.cursor_snapshot();
    assert_eq!(visible.image, hidden.image);
    assert!(visible.visible);
    assert_eq!(visible.level, 0);

}

fn cursor_warp_runner() -> FixtureRunner {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
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
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    runner.set_mouse_position(352, 380);
    let return_pc = 0x0002_0000;
    runner.bus.write_word(return_pc, 0x60FE); // BRA.S *
    runner.m68k.cpu.write_reg(Register::PC, return_pc);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FE00);
    runner
}

fn request_cursor_warp(runner: &mut FixtureRunner) {
    runner.bus.write_long(addr::M_TEMP, (140 << 16) | 300);
    runner.bus.write_long(addr::MOUSE_LOC, (140 << 16) | 300);
    runner.bus.write_byte(0x08CE, 1); // CrsrNew
}

#[test]
fn cursor_task_direct_call_adopts_guest_warp() {
    let mut runner = cursor_warp_runner();
    request_cursor_warp(&mut runner);
    let task = runner.bus.read_long(addr::J_CRSR_TASK);
    let sp = runner.m68k.cpu.read_reg(Register::A7);
    let pc = runner.m68k.cpu.read_reg(Register::PC);
    runner.bus.write_long(sp - 4, pc);
    runner.m68k.cpu.write_reg(Register::A7, sp - 4);
    runner.m68k.cpu.write_reg(Register::PC, task);
    runner.m68k.cpu.write_reg(Register::D0, 0x12345678);
    runner.m68k.cpu.write_reg(Register::A0, 0x87654321);

    assert!(runner.run_steps(30, None).1);

    assert_eq!(runner.bus.read_long(addr::MOUSE_LOC2), (140 << 16) | 300);
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    assert_eq!(runner.take_guest_cursor_warp(), Some((140, 300)));
    assert_eq!(runner.take_guest_cursor_warp(), None);
    assert_eq!(runner.bus.read_byte(0x08CE), 0);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x12345678);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0x87654321);
    runner.advance_guest_tick();
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    runner.set_mouse_position(150, 310);
    assert_eq!(runner.take_guest_cursor_warp(), None);
    assert_eq!(runner.dispatcher.mouse_position(), (150, 310));
    assert_eq!(runner.bus.read_long(addr::MOUSE_LOC2), (150 << 16) | 310);
}

#[test]
fn cursor_task_vbl_adopts_pending_guest_warp() {
    let mut runner = cursor_warp_runner();
    request_cursor_warp(&mut runner);
    runner.advance_guest_tick();
    runner.run_steps(30, None);
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    assert_eq!(runner.bus.read_byte(0x08CE), 0);
}

#[test]
fn cursor_task_warp_reaches_event_trap_in_same_batch() {
    let mut runner = cursor_warp_runner();
    request_cursor_warp(&mut runner);
    let task = runner.bus.read_long(addr::J_CRSR_TASK);
    let sp = runner.m68k.cpu.read_reg(Register::A7);
    let pc = runner.m68k.cpu.read_reg(Register::PC);
    let event = 0x0003_0000;
    runner.bus.write_word(pc, 0xA970); // GetNextEvent
    runner.bus.write_word(pc + 2, 0x60FE);
    runner.bus.write_long(sp - 4, pc);
    runner.bus.write_long(sp, event);
    runner.bus.write_word(sp + 4, 0); // null event only
    runner.m68k.cpu.write_reg(Register::A7, sp - 4);
    runner.m68k.cpu.write_reg(Register::PC, task);
    runner.run_steps(30, None);
    assert_eq!(runner.bus.read_word(event), 0);
    assert_eq!(runner.bus.read_word(event + 10), 140);
    assert_eq!(runner.bus.read_word(event + 12), 300);
}

#[test]
fn cursor_task_guest_wrapper_can_chain_to_default_task() {
    let mut runner = cursor_warp_runner();
    let task = runner.bus.read_long(addr::J_CRSR_TASK);
    let wrapper = runner.bus.alloc(8);
    runner.bus.write_word(wrapper, 0x4EB9); // JSR default cursor task
    runner.bus.write_long(wrapper + 2, task);
    runner.bus.write_word(wrapper + 6, 0x4E75);
    runner.bus.write_long(addr::J_CRSR_TASK, wrapper);
    request_cursor_warp(&mut runner);
    runner.advance_guest_tick();
    assert!(runner.active_interrupt_callback.is_some());
    runner.run_steps(40, None);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    assert_eq!(runner.bus.read_byte(addr::CRSR_NEW), 0);
}

#[test]
fn cursor_task_waits_for_request_and_respects_interrupt_mask() {
    let mut runner = cursor_warp_runner();
    request_cursor_warp(&mut runner);
    runner.bus.write_byte(addr::CRSR_NEW, 0);
    runner.advance_guest_tick();
    assert_eq!(runner.dispatcher.mouse_position(), (352, 380));
    runner.bus.write_byte(addr::CRSR_NEW, 1);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2100);
    runner.advance_guest_tick();
    assert_eq!(runner.dispatcher.mouse_position(), (352, 380));
    assert_eq!(runner.bus.read_byte(addr::CRSR_NEW), 1);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
    runner.advance_guest_tick();
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    assert_eq!(runner.bus.read_byte(addr::CRSR_NEW), 0);
}

#[test]
fn init_app_seeds_cursor_task_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
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
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    assert_eq!(
        runner.bus.read_word(runner.default_cursor_task),
        0x4A38,
        "default cursor task should test the pending update flag"
    );
    assert_eq!(
        runner.bus.read_long(addr::J_CRSR_TASK),
        runner.default_cursor_task,
        "JCrsrTask ($08EE) should boot to the callable cursor updater"
    );
}

#[test]
fn init_app_seeds_callable_show_cursor_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
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
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let entry = runner.bus.read_long(addr::J_SHOW_CURSOR);
    assert_ne!(entry, 0);
    assert_eq!(
        [runner.bus.read_word(entry), runner.bus.read_word(entry + 2)],
        [0xA853, 0x4E75],
        "JShowCursor should target ShowCursor followed by RTS"
    );

    let call_site = 0x0002_0000u32;
    let initial_sp = 0x007F_FE00u32;
    runner.bus.write_word(call_site, 0x2078); // MOVEA.L ($0804).W,A0
    runner.bus.write_word(call_site + 2, 0x0804);
    runner.bus.write_word(call_site + 4, 0x4E90); // JSR (A0)
    runner.bus.write_word(call_site + 6, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, call_site);
    runner.m68k.cpu.write_reg(Register::A7, initial_sp);
    runner.dispatcher.cursor_state.set_level_for_test(-1);

    let (steps, running) = runner.run_steps(4, None);

    assert!(running);
    assert_eq!(steps, 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), call_site + 6);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), initial_sp);
    assert_eq!(runner.dispatcher.cursor_level(), 0);
    assert!(runner.dispatcher.cursor_visible());
}

#[test]
fn init_app_seeds_callable_init_cursor_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
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
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let entry = runner.bus.read_long(addr::J_INIT_CRSR);
    assert_ne!(entry, 0);
    assert_eq!(
        [runner.bus.read_word(entry), runner.bus.read_word(entry + 2)],
        [0xA850, 0x4E75],
        "JInitCrsr should target InitCursor followed by RTS"
    );

    let call_site = 0x0002_0000u32;
    let initial_sp = 0x007F_FE00u32;
    runner.bus.write_word(call_site, 0x2078); // MOVEA.L ($0814).W,A0
    runner.bus.write_word(call_site + 2, 0x0814);
    runner.bus.write_word(call_site + 4, 0x4E90); // JSR (A0)
    runner.bus.write_word(call_site + 6, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, call_site);
    runner.m68k.cpu.write_reg(Register::A7, initial_sp);
    runner.dispatcher.cursor_state.set_level_for_test(-1);

    let (steps, running) = runner.run_steps(4, None);

    assert!(running);
    assert_eq!(steps, 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), call_site + 6);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), initial_sp);
    assert_eq!(runner.dispatcher.cursor_level(), 0);
    assert!(runner.dispatcher.cursor_visible());
}

#[test]
fn init_app_seeds_callable_shield_cursor_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
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
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let shield_cursor_trampoline = runner.bus.read_long(addr::J_SHIELD_CURSOR);
    assert_ne!(shield_cursor_trampoline, 0);
    assert_eq!(
        [
            runner.bus.read_word(shield_cursor_trampoline),
            runner.bus.read_word(shield_cursor_trampoline + 2),
            runner.bus.read_word(shield_cursor_trampoline + 4),
        ],
        [0x205F, 0xA855, 0x4ED0]
    );

    let args_sp = 0x007F_FE00u32;
    let return_pc = 0x0002_0000u32;
    runner.bus.write_word(args_sp, 100); // left
    runner.bus.write_word(args_sp + 2, 120); // top
    runner.bus.write_word(args_sp + 4, 500); // right
    runner.bus.write_word(args_sp + 6, 420); // bottom
    runner.bus.write_long(args_sp - 4, return_pc);
    runner.bus.write_word(return_pc, 0x4E71); // NOP
    runner
        .m68k
        .cpu
        .write_reg(Register::PC, shield_cursor_trampoline);
    runner.m68k.cpu.write_reg(Register::A7, args_sp - 4);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::A7),
        args_sp + 8,
        "JShieldCursor should consume its four Pascal INTEGER arguments"
    );
}

#[test]
fn init_app_seeds_callable_hide_cursor_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
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
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let hide_cursor_trampoline = runner.bus.read_long(addr::J_HIDE_CURSOR);
    assert_ne!(hide_cursor_trampoline, 0);
    assert_eq!(
        [
            runner.bus.read_word(hide_cursor_trampoline),
            runner.bus.read_word(hide_cursor_trampoline + 2),
            runner.bus.read_word(hide_cursor_trampoline + 4),
        ],
        [0x205F, 0xA852, 0x4ED0],
        "JHideCursor should pop the JSR return address, trap, and jump back"
    );

    let call_sp = 0x007F_FE00u32;
    let return_pc = 0x0002_0000u32;
    runner.bus.write_long(call_sp - 4, return_pc);
    runner.bus.write_word(return_pc, 0x4E71);
    runner
        .m68k
        .cpu
        .write_reg(Register::PC, hide_cursor_trampoline);
    runner.m68k.cpu.write_reg(Register::A7, call_sp - 4);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), call_sp);
    assert_eq!(runner.dispatcher().cursor_level(), -1);
}

#[test]
fn cursor_task_default_vector_does_not_inject_interrupt_on_guest_tick() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.install_cursor_task();
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;

    runner
        .bus
        .write_long(addr::J_CRSR_TASK, runner.default_cursor_task);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.advance_guest_tick();

    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.cursor_task_trampoline, 0);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert_eq!(runner.bus.read_long(addr::TICKS), 1);
}

#[test]
fn cursor_task_callback_arms_interrupt_from_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = 0x0004_1234;

    runner.bus.write_long(addr::J_CRSR_TASK, callback_addr);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::D0, 0x1111_1111);
    runner.m68k.cpu.write_reg(Register::D7, 0x7777_7777);
    runner.m68k.cpu.write_reg(Register::A0, 0xAAAA_0000);
    runner.m68k.cpu.write_reg(Register::A6, 0xCCCC_0000);
    runner.m68k.cpu.core.set_ccr(0x04);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2004);

    runner.advance_guest_tick();

    let active = runner
        .active_interrupt_callback
        .expect("cursor task callback should have been armed");
    assert!(matches!(
        active.source,
        ActiveInterruptCallbackSource::CursorTask
    ));
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);
    assert_eq!(active.a_regs[7], interrupted_sp);
    assert_eq!(active.a_regs[6], 0xCCCC_0000);
    assert_eq!(active.d_regs[0], 0x1111_1111);
    assert_eq!(active.d_regs[7], 0x7777_7777);
    assert_eq!(active.sr, 0x2004);
    assert_eq!(active.ccr, 0x04);
    assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2104);

    assert_ne!(runner.cursor_task_trampoline, 0);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        runner.cursor_task_trampoline
    );
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp - 4);
    assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
    assert_eq!(runner.bus.read_word(runner.cursor_task_trampoline), 0x48E7);
    assert_eq!(
        runner.bus.read_word(runner.cursor_task_trampoline + 4),
        0x4EB9
    );
    assert_eq!(
        runner.bus.read_long(runner.cursor_task_trampoline + 6),
        callback_addr
    );
    assert_eq!(
        runner.bus.read_word(runner.cursor_task_trampoline + 10),
        0x4CDF
    );
    assert_eq!(
        runner.bus.read_word(runner.cursor_task_trampoline + 14),
        0x4E75
    );
}

#[test]
fn cursor_task_defers_while_processor_priority_masks_level_one() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;

    runner.bus.write_long(addr::J_CRSR_TASK, 0x0004_1234);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2100);

    runner.advance_guest_tick();

    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.cursor_task_trampoline, 0);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
}

#[test]
fn guest_cursor_recenter_does_not_generate_physical_adb_motion() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    assert!(runner
        .dispatcher
        .adb
        .set_device_handler(3, 0x0012_3456, 0, false));

    runner.set_mouse_position(100, 100);
    runner.dispatcher.adb.flush(3);

    let previous_mouse = runner.bus.read_long(addr::MOUSE_LOC2);
    runner.bus.write_long(addr::MOUSE_LOC2, (300 << 16) | 296);
    runner.sync_guest_mouse_position(previous_mouse);

    assert_eq!(runner.dispatcher.mouse_position(), (300, 296));
    assert_eq!(runner.dispatcher.adb.pending_packet_count(), 0);

    runner.set_mouse_position(100, 120);
    assert_eq!(runner.dispatcher.adb.pending_packet_count(), 1);
    assert_eq!(
        runner.dispatcher.adb.pop_pending_packet().unwrap().packet,
        [2, 0x80, 0x94]
    );
}

#[test]
fn adb_mouse_callback_uses_documented_registers_and_restores_foreground() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);
    let data_area = runner.bus.alloc(16);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS
    for offset in (0..20).step_by(2) {
        runner.bus.write_word(interrupted_pc + offset, 0x4E71); // NOP
    }
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::A0, 0x1111_1111);
    runner.m68k.cpu.write_reg(Register::A1, 0x2222_2222);
    runner.m68k.cpu.write_reg(Register::A2, 0x3333_3333);
    runner.m68k.cpu.write_reg(Register::D0, 0x4444_4444);
    assert!(runner
        .dispatcher
        .adb
        .set_device_handler(3, callback_addr, data_area, false));
    runner.dispatcher.adb.note_mouse_state((5, -10), true);

    assert!(runner.fire_adb_callback());
    let packet_ptr = runner.m68k.cpu.read_reg(Register::A0);
    assert_eq!(runner.bus.read_bytes(packet_ptr, 3), &[2, 5, 0xF6]);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A1), callback_addr);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A2), data_area);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x3C);
    assert!(matches!(
        runner.active_interrupt_callback.map(|active| active.source),
        Some(ActiveInterruptCallbackSource::Adb)
    ));
    assert!(
        runner
            .bus
            .get_alloc_size(runner.adb_callback_trampoline)
            .is_none(),
        "Systemless-owned ADB trampoline must stay outside the guest heap"
    );

    let (_, running) = runner.run_steps(6, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0x1111_1111);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A1), 0x2222_2222);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A2), 0x3333_3333);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x4444_4444);
    assert!(!runner.is_halted());
}

#[test]
fn cursor_snapshot_retains_colour_image_and_mask_when_hidden() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let image = crate::display::CursorImage::Color {
        width: 2, height: 2, pixels_argb: vec![0xffff0000, 0xff00ff00, 0xff0000ff, 0xffffffff],
        mask: [0xa5; 32], hot_v: 1, hot_h: 2, mono_data: [0x81; 32], mono_mask: [0x42; 32],
    };
    let ppc = runner.native.application_mut().unwrap();
    ppc.cursor_state.install(image.clone()); ppc.cursor_state.hide();
    let snapshot = runner.cursor_snapshot();
    assert_eq!(snapshot.image, Some(image.clone()));
    assert!(!snapshot.visible); assert_eq!(snapshot.level, -1);
    runner.dispatcher.cursor_state.show();
    let snapshot = runner.cursor_snapshot();
    assert_eq!(snapshot.image, Some(image));
    assert!(snapshot.visible); assert_eq!(snapshot.level, 0);
}
