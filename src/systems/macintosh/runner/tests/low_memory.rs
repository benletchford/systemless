use super::*;
use crate::loader::ppc::PpcVblTaskRecord;
use crate::memory::globals::addr;

#[test]
fn ppc_slice_keeps_ticks_coherent_across_runner_and_guest_memory() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
    ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
    ppc_app
        .memory
        .write_u32_be(PPC_CODE_BASE, 0x4800_0000)
        .expect("rewrite entry as infinite branch");
    ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_launch_state(41, 0x1234_5678, 0);
    runner.init_app(&app);
    runner.set_instructions_per_tick(1_000_000);
    runner.tick_budget = 1;

    let (steps, running) = runner.run_steps(1, None);

    assert_eq!(steps, 1);
    assert!(running);
    assert_eq!(runner.bus.read_long(addr::TICKS), 42);
    assert_eq!(runner.guest_tick(), 42);
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    assert_eq!(
        ppc_app
            .memory
            .read_u32_be(crate::memory::globals::addr::TICKS),
        Some(42)
    );
    assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(42));
}

#[test]
fn process_tick_advance_observes_direct_low_memory_mutation() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_launch_state(41, 0x1234_5678, 0);
    runner.init_app(&app);

    // Ticks is writable guest state. The next vertical-retrace update
    // resolves the shared semantic clock from the directly-written cell
    // before advancing it, so both adapters continue from the same value.
    runner.bus.write_long(addr::TICKS, 9_000);
    assert_eq!(runner.advance_guest_tick(), 9_001);
    assert_eq!(runner.guest_tick(), 9_001);
    assert_eq!(runner.bus.read_long(addr::TICKS), 9_001);
    assert_eq!(
        runner
            .native
            .application_mut()
            .expect("PPC app installed")
            .memory
            .read_u32_be(addr::TICKS),
        Some(9_001)
    );
}

#[test]
fn ppc_direct_ticks_store_survives_the_next_native_slice() {
    const WRITE_TICKS: u32 = PPC_CODE_BASE + 0x100;
    const DIRECT_TICKS: u32 = 0xCAFE_BABE;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
    ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
    ppc_app.cpu.pc = WRITE_TICKS;
    ppc_app.memory.add_region(
        WRITE_TICKS,
        [
            0x3C60_CAFEu32, // lis r3,$CAFE
            0x6063_BABE,    // ori r3,r3,$BABE
            0x3C80_0000,    // lis r4,0
            0x6084_016A,    // ori r4,r4,$016A
            0x9064_0000,    // stw r3,0(r4)
            0x4800_0000,    // b .
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_launch_state(41, 0x1234_5678, 0);
    runner.init_app(&app);
    runner.set_instructions_per_tick(100);
    runner.tick_budget = 100;

    let (steps, running) = runner.run_steps(5, None);
    assert_eq!(steps, 5);
    assert!(running);
    assert_eq!(runner.bus.read_long(addr::TICKS), DIRECT_TICKS);
    assert_eq!(
        runner.guest_tick(),
        DIRECT_TICKS,
        "the native slice boundary must import a direct guest Ticks store"
    );

    // The next PPC slice prepares its HLE clock from the shared low-memory
    // bytes. It must not restore the dispatcher's stale compatibility
    // scalar over a direct store made by native guest code.
    let (steps, running) = runner.run_steps(1, None);
    assert_eq!(steps, 1);
    assert!(running);
    assert_eq!(runner.bus.read_long(addr::TICKS), DIRECT_TICKS);
    assert_eq!(runner.guest_tick(), DIRECT_TICKS);
    assert_eq!(
        runner
            .native
            .application_mut()
            .expect("PPC app installed")
            .memory
            .read_u32_be(addr::TICKS),
        Some(DIRECT_TICKS)
    );
}

#[test]
fn ppc_slice_direct_ticks_store_crossing_one_vbl_uses_host_epoch() {
    const WRITE_TICKS: u32 = PPC_CODE_BASE + 0x100;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x200;
    const TASK_PTR: u32 = PPC_DATA_BASE + 0x3000;
    const START_TICK: u32 = 41;

    // The PPC guest rewinds or jumps the writable Ticks cell, then the
    // host cycle budget crosses exactly one VBL.  Callback delivery must
    // follow that guest baseline by one tick; it must not subtract the
    // arbitrary store from the slice's pre-execution snapshot.
    for direct_tick in [7, 0xCAFE_BABE] {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        ppc_app.cpu.pc = WRITE_TICKS;
        ppc_app.memory.add_region(
            WRITE_TICKS,
            [
                0x3C60_0000u32 | (direct_tick >> 16), // lis r3, direct_tick
                0x6063_0000 | (direct_tick & 0xffff), // ori r3, direct_tick
                0x3C80_0000,                          // lis r4, 0
                0x6084_016A,                          // ori r4, $016A
                0x9064_0000,                          // stw r3,0(r4)
                0x4800_0000,                          // b .
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(
            CALLBACK,
            [
                0x3880_BEEFu32, // li r4,$BEEF
                0xB083_000E,    // sth r4,14(r3): callback marker
                0x4E80_0020,    // blr
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(TASK_PTR, vec![0; 16]);
        ppc_app.memory.write_u32_be(TASK_PTR + 6, CALLBACK).unwrap();
        ppc_app.memory.write_u16_be(TASK_PTR + 10, 1).unwrap();
        ppc_app.vbl_tasks.push(PpcVblTaskRecord {
            task_ptr: TASK_PTR,
            architecture: CallbackTaskArchitecture::PowerPc,
            slot: None,
            pending: false,
        });
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(START_TICK, 0x1234_5678, 0);
        runner.init_app(&app);
        runner.set_instructions_per_tick(5);
        runner.tick_budget = 5;

        let (steps, running) = runner.run_steps(5, None);

        assert!(running);
        assert!(steps >= 5);
        let expected_tick = direct_tick.wrapping_add(1);
        assert_eq!(runner.guest_tick(), expected_tick);
        assert_eq!(runner.bus.read_long(addr::TICKS), expected_tick);
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        assert_eq!(
            ppc_app.memory.read_u32_be(addr::TICKS),
            Some(expected_tick),
            "the stale pre-slice candidate must not overwrite direct Ticks"
        );
        assert_eq!(ppc_app.memory.read_u16_be(TASK_PTR + 10), Some(0));
        assert_eq!(ppc_app.memory.read_u16_be(TASK_PTR + 14), Some(0xBEEF));
    }
}

#[test]
fn ppc_slice_direct_ticks_store_replays_three_vbl_epochs_in_order() {
    const WRITE_TICKS: u32 = PPC_CODE_BASE + 0x100;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x200;
    const TASK_BASE: u32 = PPC_DATA_BASE + 0x3000;
    const START_TICK: u32 = 41;

    // Each task starts one count later than the previous task. The common
    // callback records the Ticks value it observes, so exactly one task
    // fires at each of the three host VBL epochs.
    for direct_tick in [7, 0xCAFE_BABE] {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        ppc_app.cpu.pc = WRITE_TICKS;
        ppc_app.memory.add_region(
            WRITE_TICKS,
            [
                0x3C60_0000u32 | (direct_tick >> 16), // lis r3, direct_tick
                0x6063_0000 | (direct_tick & 0xffff), // ori r3, direct_tick
                0x3C80_0000,                          // lis r4, 0
                0x6084_016A,                          // ori r4, $016A
                0x9064_0000,                          // stw r3,0(r4)
                0x4800_0000,                          // b .
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(
            CALLBACK,
            [
                0x3880_016Au32, // li r4,$016A
                0x80A4_0000,    // lwz r5,0(r4): read guest Ticks
                0x90A3_0010,    // stw r5,16(r3): record on task
                0x4E80_0020,    // blr
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(TASK_BASE, vec![0; 0x80]);
        for (task_offset, count) in [(0, 1), (0x20, 2), (0x40, 3)] {
            let task_ptr = TASK_BASE + task_offset;
            ppc_app.memory.write_u32_be(task_ptr + 6, CALLBACK).unwrap();
            ppc_app.memory.write_u16_be(task_ptr + 10, count).unwrap();
            ppc_app.vbl_tasks.push(PpcVblTaskRecord {
                task_ptr,
                architecture: CallbackTaskArchitecture::PowerPc,
                slot: None,
                pending: false,
            });
        }
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(START_TICK, 0x1234_5678, 0);
        runner.init_app(&app);
        runner.set_instructions_per_tick(5);
        runner.tick_budget = 5;

        for epoch in 1..=3 {
            let (steps, running) = runner.run_steps(5, None);
            assert!(running);
            assert!(steps >= 5);
            let expected_tick = direct_tick.wrapping_add(epoch);
            assert_eq!(runner.guest_tick(), expected_tick);
            assert_eq!(runner.bus.read_long(addr::TICKS), expected_tick);
        }

        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        for (task_offset, epoch) in [(0, 1), (0x20, 2), (0x40, 3)] {
            let task_ptr = TASK_BASE + task_offset;
            assert_eq!(ppc_app.memory.read_u16_be(task_ptr + 10), Some(0));
            assert_eq!(
                ppc_app.memory.read_u32_be(task_ptr + 16),
                Some(direct_tick.wrapping_add(epoch)),
                "callback must observe host epoch {epoch}"
            );
        }
    }
}

#[test]
fn mixed_isa_tickcount_and_lmgetticks_observe_shared_low_memory_bytes() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
    ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
    ppc_app
        .memory
        .add_region(PPC_IMPORT_TRAP_BASE, 0x4e80_0020u32.to_be_bytes().to_vec());
    let mut lmget_ticks = test_ppc_import_binding(0, "InterfaceLib", "LMGetTicks");
    lmget_ticks.address = PPC_IMPORT_TRAP_BASE;
    lmget_ticks.trap_pc = PPC_IMPORT_TRAP_BASE;
    lmget_ticks.dispatcher_target = PpcImportDispatcherTarget::TickCount;
    ppc_app.import_count = 1;
    ppc_app.imports = vec![lmget_ticks];

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_launch_state(100, 0x1234_5678, 0);
    runner.init_app(&app);

    let sp = crate::trap::test_helpers::TEST_SP;
    runner.m68k.cpu.write_reg(Register::A7, sp);
    let first = 0x1020_3040;
    runner.bus.write_long(addr::TICKS, first);
    let first_result =
        runner
            .dispatcher
            .dispatch_toolbox(true, 0x175, &mut runner.m68k.cpu, &mut runner.bus);
    assert!(first_result.is_some_and(|result| result.is_ok()));
    assert_eq!(runner.bus.read_long(sp), first);

    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app installed");
    let ppc_app = native_context.adapter_mut();
    assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(first));

    // Write through the native view between ABI calls. The shared low
    // memory overlay makes the new bytes immediately visible to the 68K
    // bus and to the next native LMGetTicks import.
    let second = 0x5566_7788;
    ppc_app
        .memory
        .write_u32_be(addr::TICKS, second)
        .expect("shared Ticks write");
    runner.m68k.cpu.write_reg(Register::A7, sp);
    let second_result =
        runner
            .dispatcher
            .dispatch_toolbox(true, 0x175, &mut runner.m68k.cpu, &mut runner.bus);
    assert!(second_result.is_some_and(|result| result.is_ok()));
    assert_eq!(runner.bus.read_long(sp), second);

    let third = 0xAABB_CCDD;
    runner.bus.write_long(addr::TICKS, third);
    ppc_app.cpu.pc = PPC_IMPORT_TRAP_BASE;
    ppc_app.cpu.lr = PPC_HALT_PC;
    let probe = runner
        .process_context
        .with_memory_and_cfm(|memory_manager, cfm| {
            ppc_app.run_with_process_services(64, false, false, memory_manager, cfm)
        });
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(ppc_app.cpu.gpr[3], third);
    assert_eq!(runner.guest_tick(), third);
}

#[test]
fn ppc_slice_shares_low_memory_time_after_sixtieth_tick() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
    ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
    ppc_app
        .memory
        .write_u32_be(PPC_CODE_BASE, 0x4800_0000)
        .expect("rewrite entry as infinite branch");
    ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_app_start_time(0x1020_3040);
    runner.set_launch_state(59, 1, 0);
    runner.init_app(&app);
    runner.set_instructions_per_tick(1);

    let (steps, running) = runner.run_steps(1, None);

    assert_eq!(steps, 1);
    assert!(running);
    assert_eq!(runner.bus.read_long(addr::TICKS), 60);
    assert_eq!(runner.bus.read_long(addr::TIME), 0x1020_3041);
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(60));
    assert_eq!(ppc_app.memory.read_u32_be(addr::TIME), Some(0x1020_3041));
}

#[test]
fn tick_count_poll_fast_forward_returns_for_due_ppc_vbl_callback() {
    const LOOP: u32 = PPC_CODE_BASE + 0x1000;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x2000;
    const TASK: u32 = PPC_DATA_BASE + 0x3000;
    const CALLBACK_TICK: u32 = PPC_DATA_BASE + 0x3100;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
    ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
    ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
    ppc_app.cpu.pc = LOOP;
    ppc_app.memory.add_region(
        LOOP,
        [
            0x3d80_01f0u32, // lis r12,$01f0
            0x618c_0000,    // ori r12,r12,0
            0x7d89_03a6,    // mtctr r12
            0x4e80_0421,    // bctrl
            0x4bff_fffc,    // b -4
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    ppc_app.memory.add_region(
        CALLBACK,
        [
            0x8060_016au32, // lwz r3,$016a(0)
            0x3c80_0200,    // lis r4,$0200
            0x9064_3100,    // stw r3,$3100(r4)
            0x4e80_0020,    // blr
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    ppc_app.memory.add_region(TASK, vec![0; 0x104]);
    ppc_app.memory.write_u32_be(TASK + 6, CALLBACK).unwrap();
    ppc_app.memory.write_u16_be(TASK + 10, 1).unwrap();
    ppc_app.vbl_tasks.push(PpcVblTaskRecord {
        task_ptr: TASK,
        architecture: CallbackTaskArchitecture::PowerPc,
        slot: None,
        pending: false,
    });
    let mut tick_count = test_ppc_import_binding(0, "InterfaceLib", "LMGetTicks");
    tick_count.address = PPC_IMPORT_TRAP_BASE;
    tick_count.trap_pc = PPC_IMPORT_TRAP_BASE;
    tick_count.dispatcher_target = PpcImportDispatcherTarget::TickCount;
    ppc_app.import_count = 1;
    ppc_app.imports = vec![tick_count];

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_optional_launch_state(Some(41), None, None);
    runner.init_app(&app);
    runner.set_instructions_per_tick(1_000);

    let (_steps, running) = runner.run_steps(1_000, None);

    assert!(running);
    assert_eq!(runner.bus.read_long(addr::TICKS), 42);
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    assert_eq!(ppc_app.memory.read_u32_be(CALLBACK_TICK), Some(42));
}

#[test]
fn batched_ppc_vbl_callbacks_read_their_own_tick_from_low_memory() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_app_start_time(0x1020_3040);
    runner.set_optional_launch_state(Some(41), None, None);
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app installed");
    let mut ppc_app = native_context.adapter_mut();
    ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;

    for (index, count) in [1u16, 2].into_iter().enumerate() {
        let task = PPC_DATA_BASE + 0x1000 + index as u32 * 0x100;
        let callback = task + 0x20;
        ppc_app.memory.add_region(task, vec![0; 0x60]);
        ppc_app.memory.write_u32_be(task + 6, callback).unwrap();
        ppc_app.memory.write_u16_be(task + 10, count).unwrap();
        for (offset, instruction) in [
            0x8060_016au32, // lwz r3,$016a(0)
            0x4e80_0020,    // blr
        ]
        .into_iter()
        .enumerate()
        {
            ppc_app
                .memory
                .write_u32_be(callback + offset as u32 * 4, instruction)
                .unwrap();
        }
        ppc_app.vbl_tasks.push(PpcVblTaskRecord {
            task_ptr: task,
            architecture: CallbackTaskArchitecture::PowerPc,
            slot: None,
            pending: false,
        });
    }

    let (vbl, timer) = runner
        .process_context
        .with_memory_and_cfm(|memory_manager, cfm| {
            FixtureRunner::fire_ppc_tick_callbacks(
                &mut ppc_app,
                memory_manager,
                cfm,
                41,
                0x1020_3040,
                2,
                64,
                false,
                false,
            )
        });

    assert_eq!(vbl.len(), 2);
    assert!(timer.is_empty());
    assert_eq!(vbl[0].invocation.tick, 42);
    assert_eq!(vbl[0].invocation.end_r3, 42);
    assert_eq!(vbl[1].invocation.tick, 43);
    assert_eq!(vbl[1].invocation.end_r3, 43);
    assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(43));
}

#[test]
fn ppc_slice_shares_rnd_seed_in_both_directions() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
    ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
    ppc_app
        .memory
        .write_u32_be(PPC_CODE_BASE, 0x3C60_1234) // lis r3,$1234
        .unwrap();
    ppc_app.memory.add_region(
        PPC_CODE_BASE + 4,
        [
            0x6063_5678u32, // ori r3,r3,$5678
            0x3880_0156,    // li r4,$0156
            0x9064_0000,    // stw r3,0(r4)
            0x4800_0000,    // b .
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_launch_state(0, 1, 0);
    runner.init_app(&app);
    runner.set_instructions_per_tick(100);

    let (steps, running) = runner.run_steps(64, None);
    assert!(steps > 0);
    assert!(running);
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x1234_5678);

    runner.bus.write_long(addr::RND_SEED, 0x89AB_CDEF);
    let (steps, running) = runner.run_steps(1, None);
    assert_eq!(steps, 1);
    assert!(running);
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    assert_eq!(
        ppc_app.memory.read_u32_be(addr::RND_SEED),
        Some(0x89AB_CDEF)
    );
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x89AB_CDEF);
}

#[test]
fn ppc_process_low_memory_has_immediate_bidirectional_visibility() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_optional_launch_state(None, Some(1), None);
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app installed");
    let ppc_app = native_context.adapter_mut();

    for (address, ppc_value, runner_value) in [
        (addr::RND_SEED, 0x1234_5678, 0x89ab_cdef),
        (addr::TICKS, 0x1122_3344, 0x5566_7788),
        (addr::TIME, 0x1020_3040, 0x5060_7080),
    ] {
        ppc_app.memory.write_u32_be(address, ppc_value).unwrap();
        assert_eq!(runner.bus.read_long(address), ppc_value);

        runner.bus.write_long(address, runner_value);
        assert_eq!(ppc_app.memory.read_u32_be(address), Some(runner_value));
    }

    assert_eq!(
        runner.bus.read_word(addr::MENU_FLASH),
        crate::memory::globals::DEFAULT_MENU_FLASH_COUNT
    );
    ppc_app.memory.write_u16_be(addr::MENU_FLASH, 1).unwrap();
    assert_eq!(runner.bus.read_word(addr::MENU_FLASH), 1);
    runner.bus.write_word(addr::MENU_FLASH, 2);
    assert_eq!(ppc_app.memory.read_u16_be(addr::MENU_FLASH), Some(2));

    ppc_app
        .memory
        .write_u32_be(addr::MENU_DISABLE, 0x0080_0002)
        .unwrap();
    assert_eq!(runner.bus.read_long(addr::MENU_DISABLE), 0x0080_0002);
    runner.bus.write_long(addr::MENU_DISABLE, 0x0081_0003);
    assert_eq!(
        ppc_app.memory.read_u32_be(addr::MENU_DISABLE),
        Some(0x0081_0003),
    );

    // Addresses outside the old hand-maintained global list belong to the
    // same process too, including compatibility RAM used by 68K callbacks.
    for address in [0x0000_5000, 0x000f_0000] {
        ppc_app.memory.write_u32_be(address, 0x1234_5678).unwrap();
        assert_eq!(runner.bus.read_long(address), 0x1234_5678);
        runner.bus.write_long(address, 0x89ab_cdef);
        assert_eq!(ppc_app.memory.read_u32_be(address), Some(0x89ab_cdef));
    }
}

#[test]
fn ppc_process_memory_holes_share_runner_ram_without_overlaying_pef_mappings() {
    const FIRST_HOLE: u32 = 0x0018_0000;
    const PEF_MAPPING: u32 = 0x0020_0000;
    const SECOND_HOLE: u32 = PEF_MAPPING + 4;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
    ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
    ppc_app
        .memory
        .add_readonly_region(PEF_MAPPING, 0x1234_5678u32.to_be_bytes().to_vec());

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.bus.write_long(PEF_MAPPING, 0xaabb_ccdd);
    assert_eq!(
        runner.process_context.memory_ranges(),
        vec![
            (0, PROCESS_LOW_MEMORY_SIZE as usize),
            (PROCESS_LOW_MEMORY_SIZE, 0x0010_0000),
            (SECOND_HOLE, (8 * 1024 * 1024 - SECOND_HOLE) as usize),
        ]
    );

    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app installed");
    let ppc_app = native_context.adapter_mut();
    ppc_app
        .memory
        .write_u32_be(FIRST_HOLE, 0x0102_0304)
        .unwrap();
    assert_eq!(runner.bus.read_long(FIRST_HOLE), 0x0102_0304);
    runner.bus.write_long(SECOND_HOLE, 0x0506_0708);
    assert_eq!(ppc_app.memory.read_u32_be(SECOND_HOLE), Some(0x0506_0708));

    assert_eq!(ppc_app.memory.read_u32_be(PEF_MAPPING), Some(0x1234_5678));
    // Ordinary PEF mappings stay authoritative for the process lifetime,
    // including while the native adapter is parked outside execution.
    assert_eq!(runner.bus.read_long(PEF_MAPPING), 0x1234_5678);
    runner.bus.write_long(PEF_MAPPING, 0);
    assert_eq!(runner.bus.read_long(PEF_MAPPING), 0x1234_5678);
    assert_eq!(ppc_app.memory.read_u32_be(PEF_MAPPING), Some(0x1234_5678));
}

#[test]
fn ppc_input_globals_have_immediate_bidirectional_visibility() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app installed");
    let ppc_app = native_context.adapter_mut();

    assert_eq!(
        runner.bus.read_word(addr::SYS_EVT_MASK),
        crate::memory::globals::DEFAULT_SYS_EVT_MASK
    );
    assert_eq!(
        ppc_app.memory.read_u16_be(addr::SYS_EVT_MASK),
        Some(crate::memory::globals::DEFAULT_SYS_EVT_MASK)
    );
    ppc_app
        .memory
        .write_u16_be(addr::SYS_EVT_MASK, 0xffdf)
        .unwrap();
    assert_eq!(runner.bus.read_word(addr::SYS_EVT_MASK), 0xffdf);
    runner.bus.write_word(addr::SYS_EVT_MASK, 0x1234);
    assert_eq!(ppc_app.memory.read_u16_be(addr::SYS_EVT_MASK), Some(0x1234));

    let mut detached = ppc_app.memory.clone();
    detached.write_u16_be(addr::SYS_EVT_MASK, 0xabcd).unwrap();
    assert_eq!(runner.bus.read_word(addr::SYS_EVT_MASK), 0x1234);
    assert_eq!(ppc_app.memory.read_u16_be(addr::SYS_EVT_MASK), Some(0x1234));

    ppc_app.memory.write_u8(addr::MB_STATE, 0).unwrap();
    assert_eq!(runner.bus.read_byte(addr::MB_STATE), 0);
    runner.bus.write_byte(addr::MB_STATE, 0x80);
    assert_eq!(ppc_app.memory.read_u8(addr::MB_STATE), Some(0x80));

    let ppc_key_map = [
        0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ];
    ppc_app
        .memory
        .write_bytes(addr::KEY_MAP_LM, &ppc_key_map)
        .unwrap();
    assert_eq!(runner.bus.read_bytes(addr::KEY_MAP_LM, 16), ppc_key_map);

    let runner_key_map = [
        0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40,
        0x80,
    ];
    runner.bus.write_bytes(addr::KEY_MAP_LM, &runner_key_map);
    let mut observed_key_map = [0; 16];
    ppc_app
        .memory
        .read_bytes_into(addr::KEY_MAP_LM, &mut observed_key_map)
        .unwrap();
    assert_eq!(observed_key_map, runner_key_map);

    let ppc_points = [
        0x01, 0x02, 0x03, 0x04, 0x11, 0x12, 0x13, 0x14, 0x21, 0x22, 0x23, 0x24,
    ];
    ppc_app
        .memory
        .write_bytes(addr::M_TEMP, &ppc_points)
        .unwrap();
    assert_eq!(runner.bus.read_bytes(addr::M_TEMP, 12), ppc_points);

    let runner_points = [
        0x24, 0x23, 0x22, 0x21, 0x14, 0x13, 0x12, 0x11, 0x04, 0x03, 0x02, 0x01,
    ];
    runner.bus.write_bytes(addr::M_TEMP, &runner_points);
    let mut observed_points = [0; 12];
    ppc_app
        .memory
        .read_bytes_into(addr::M_TEMP, &mut observed_points)
        .unwrap();
    assert_eq!(observed_points, runner_points);

    let native_key_map = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32,
        0x10,
    ];
    ppc_app.set_input_snapshot(PpcInputSnapshot {
        key_map: native_key_map,
        mouse_button: true,
        mouse_v: 123,
        mouse_h: 456,
    });
    assert_eq!(runner.bus.read_byte(addr::MB_STATE), 0);
    assert_eq!(runner.bus.read_bytes(addr::KEY_MAP_LM, 16), native_key_map);
    for point_addr in [addr::M_TEMP, addr::MOUSE_LOC, addr::MOUSE_LOC2] {
        assert_eq!(runner.bus.read_word(point_addr), 123);
        assert_eq!(runner.bus.read_word(point_addr + 2), 456);
    }

    runner.push_key_down(6, b'z');
    runner.push_mouse_down(-7, 321);
    let mut host_key_map = [0; 16];
    ppc_app
        .memory
        .read_bytes_into(addr::KEY_MAP_LM, &mut host_key_map)
        .unwrap();
    assert_eq!(
        host_key_map,
        runner.dispatcher.input_state.key_map_snapshot()
    );
    assert_eq!(ppc_app.memory.read_u8(addr::MB_STATE), Some(0));
    for point_addr in [addr::M_TEMP, addr::MOUSE_LOC, addr::MOUSE_LOC2] {
        assert_eq!(ppc_app.memory.read_u16_be(point_addr), Some((-7i16) as u16));
        assert_eq!(ppc_app.memory.read_u16_be(point_addr + 2), Some(321));
    }

    ppc_app
        .memory
        .write_u32_be(addr::THE_ZONE, 0x1122_3344)
        .unwrap();
    assert_eq!(runner.bus.read_long(addr::THE_ZONE), 0x1122_3344);
    runner.bus.write_long(addr::THE_ZONE, 0x5566_7788);
    assert_eq!(
        ppc_app.memory.read_u32_be(addr::THE_ZONE),
        Some(0x5566_7788)
    );
}
