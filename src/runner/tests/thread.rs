use super::*;
use crate::cpu::CpuOps;
use crate::execution_kernel::ExecutionTaskState;
use crate::guest_call::{
    seed_pending_native_import_context, CooperativeThread, ExecutionTaskId, GuestCallTarget,
    M68kResultTarget, NativeThreadContext, PowerPcArguments, ThreadStorage,
};
use crate::guest_procedure::{
    GuestIsa, ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
    ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
    ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA, ROUTINE_RECORD_POWERPC_ISA,
    ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
};
use crate::loader::ppc::tests::synthetic_pef_with_import;
use crate::mixed_mode::proc_info;
use crate::trap::test_helpers::{setup, TEST_SP};
use ppc::{PpcCpu, PpcNativeReturnGpr3, PpcRunResult};

#[test]
fn companion_engine_installs_native_worker_context_and_hands_back_to_classic() {
    const ADDRESS: u32 = PPC_DATA_BASE + 0x5000;
    const A_RETURN: u32 = 0x5678;
    const A_FINAL: u32 = 0x6678;
    const B_RETURN: u32 = 0x7678;
    const B_FINAL: u32 = 0x8678;
    const LWARX_R12_R4_R5: u32 = (31 << 26) | (12 << 21) | (4 << 16) | (5 << 11) | (20 << 1);
    let native = halted_ppc_app_with_sound(PpcSoundState::default())
        .ppc
        .take()
        .unwrap();
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_ppc_companion(native);
    let calls = runner.dispatcher.guest_calls.shared_handle();
    assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::M68k));
    let mut classic = CooperativeThread::default();
    classic.pc = 0x1234;
    assert!(calls.save_cooperative_context(ExecutionTaskId::APPLICATION, classic));
    let pending_context = |trap_pc, return_pc, final_pc, rtoc, result| {
        let mut cpu = PpcCpu::new();
        let mut memory = PpcSectionMem::new();
        seed_pending_native_import_context(
            &mut cpu,
            &mut memory,
            trap_pc,
            return_pc,
            rtoc ^ 0xffff_0000,
            return_pc,
            final_pc,
            rtoc,
            PpcNativeReturnGpr3::Set(result),
        );
        cpu.capture_execution_context()
    };
    let worker_a = calls
        .create_native_thread(
            NativeThreadContext {
                context: pending_context(0x5000, A_RETURN, A_FINAL, 0xaaaa_0002, 0xaaaa_0003),
            },
            ThreadStorage::default(),
            false,
            |_| true,
        )
        .unwrap();
    let worker_b = calls
        .create_native_thread(
            NativeThreadContext {
                context: pending_context(0x7000, B_RETURN, B_FINAL, 0xbbbb_0002, 0xbbbb_0003),
            },
            ThreadStorage::default(),
            false,
            |_| true,
        )
        .unwrap();
    assert_eq!(calls.switch_from_classic(worker_a), Some(None));
    let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
    {
        let companion = context.adapter_mut();
        assert!(calls.prepare_native_task(&mut companion.cpu));
        assert_eq!(companion.cpu.pc, A_RETURN);
        assert!(calls
            .yield_native_thread(&mut companion.cpu, worker_b.thread_id())
            .unwrap());
        assert_eq!(companion.cpu.pc, B_RETURN);
        assert_eq!(
            companion.cpu.run_with_imports(
                &mut companion.memory,
                2,
                B_FINAL,
                0,
                0,
                |_, _, _| unreachable!()
            ),
            PpcRunResult::Halted {
                pc: B_FINAL,
                cycles: 1
            }
        );
        assert_eq!(
            (companion.cpu.gpr[2], companion.cpu.gpr[3]),
            (0xbbbb_0002, 0xbbbb_0003)
        );
        assert!(calls
            .yield_native_thread(&mut companion.cpu, worker_a.thread_id())
            .unwrap());
        assert_eq!(
            companion.cpu.run_with_imports(
                &mut companion.memory,
                2,
                A_FINAL,
                0,
                0,
                |_, _, _| unreachable!()
            ),
            PpcRunResult::Halted {
                pc: A_FINAL,
                cycles: 1
            }
        );
        assert_eq!(
            (companion.cpu.gpr[2], companion.cpu.gpr[3]),
            (0xaaaa_0002, 0xaaaa_0003)
        );
        companion
            .memory
            .add_region(ADDRESS, 0x5566_7788u32.to_be_bytes().to_vec());
        companion.cpu.gpr[4] = ADDRESS;
        assert_eq!(
            companion.cpu.step(&mut companion.memory, LWARX_R12_R4_R5),
            ppc::PpcStepResult::Stepped
        );
        assert_eq!(companion.cpu.reservation_address(), Some(ADDRESS));
        assert!(calls
            .yield_native_thread(&mut companion.cpu, worker_b.thread_id())
            .unwrap());
        assert_eq!(
            companion.cpu.run_with_imports(
                &mut companion.memory,
                2,
                B_FINAL,
                0,
                0,
                |_, _, _| unreachable!()
            ),
            PpcRunResult::Halted {
                pc: B_FINAL,
                cycles: 0
            }
        );
        assert_eq!(companion.cpu.reservation_address(), None);
        assert!(calls
            .yield_native_thread(&mut companion.cpu, ExecutionTaskId::APPLICATION.thread_id())
            .unwrap());
        assert!(calls.has_classic_task_handoff());
        assert_eq!(companion.cpu.reservation_address(), None);
    }
    assert!(runner.native.restore(context).is_ok());
    assert!(runner.native.application().is_none());
    assert!(runner.native.companion().is_some());
}

#[test]
fn companion_new_thread_import_installs_a_fresh_worker_on_the_live_engine() {
    const MADE: u32 = PPC_DATA_BASE + 0x5000;
    const THREAD_RETURN: u32 = PPC_IMPORT_TRAP_BASE + (4096 + 1) * 4;
    let native = load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
    let entry = native.entry_pc;
    let expected_rtoc = native.cpu.gpr[2];
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_ppc_companion(native);
    let calls = runner.dispatcher.guest_calls.shared_handle();
    assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::M68k));
    let worker;
    let live_time;
    {
        let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
        let companion = context.adapter_mut();
        companion.memory.add_region(MADE, vec![0; 4]);
        companion.cpu.msr = 0x5060_7080;
        companion.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        companion.cpu.set_time_base(0xffff_ffff_0000_0000);
        companion.cpu.gpr[3] = 1;
        companion.cpu.gpr[4] = entry;
        companion.cpu.gpr[5] = 0x1234_5678;
        companion.cpu.gpr[6] = 4096;
        companion.cpu.gpr[7] = 0;
        companion.cpu.gpr[8] = 0;
        companion.cpu.gpr[9] = MADE;
        let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
            companion.run_with_process_services(64, false, false, mm, cfm)
        });
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(companion.cpu.gpr[3], 0);
        worker = ExecutionTaskId::from_thread_id(companion.memory.read_u32_be(MADE).unwrap());
        live_time = companion.cpu.time_base();
        assert!(runner.native.restore(context).is_ok());
    }
    let mut classic = CooperativeThread::default();
    classic.pc = 0x1234;
    assert!(calls.save_cooperative_context(ExecutionTaskId::APPLICATION, classic));
    assert_eq!(calls.switch_from_classic(worker), Some(None));
    let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
    {
        let companion = context.adapter_mut();
        assert!(calls.prepare_native_task(&mut companion.cpu));
        assert_eq!(companion.cpu.pc, entry);
        assert_eq!(companion.cpu.lr, THREAD_RETURN);
        assert_eq!(companion.cpu.gpr[1] & 15, 0);
        assert_eq!(companion.cpu.gpr[2], expected_rtoc);
        assert_eq!(companion.cpu.gpr[3], 0x1234_5678);
        assert_eq!(companion.cpu.msr, 0x5060_7080);
        assert_eq!(
            companion.cpu.alignment_policy,
            ppc::PpcAlignmentPolicy::EmulateData
        );
        assert_eq!(companion.cpu.time_base(), live_time);
        assert_eq!(companion.cpu.reservation_address(), None);
    }
    assert!(runner.native.restore(context).is_ok());
}

#[test]
fn opposite_abi_thread_disposal_preserves_stack_provenance_and_retries_results() {
    const FRAME: u32 = 0x6000;
    const MADE: u32 = 0x7000;
    const RESULT: u32 = 0x7100;
    for (native_worker, recycle) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let loaded = app.ppc.as_mut().unwrap();
        loaded.entry_pc = PPC_IMPORT_TRAP_BASE;
        loaded.cpu.pc = PPC_IMPORT_TRAP_BASE;
        loaded.memory.add_region(PPC_IMPORT_TRAP_BASE, vec![0; 4]);
        let name = if native_worker {
            "NewThread"
        } else {
            "DisposeThread"
        };
        let mut binding = test_ppc_import_binding(0, "InterfaceLib", name);
        binding.dispatcher_target = if native_worker {
            PpcImportDispatcherTarget::NewThread
        } else {
            PpcImportDispatcherTarget::DisposeThread
        };
        binding.trap_pc = PPC_IMPORT_TRAP_BASE;
        loaded.import_count = 1;
        loaded.imports.push(binding);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        if native_worker {
            let cpu = &mut runner.native.application_mut().unwrap().cpu;
            cpu.lr = PPC_CODE_BASE;
            cpu.gpr[3] = 1;
            cpu.gpr[4] = PPC_CODE_BASE;
            cpu.gpr[5] = 0;
            cpu.gpr[6] = 1024;
            cpu.gpr[7] = 1;
            cpu.gpr[8] = RESULT;
            cpu.gpr[9] = MADE;
            assert!(runner.run_steps(32, None).1);
        } else {
            runner.m68k.cpu.write_reg(Register::A7, FRAME);
            runner.m68k.cpu.write_reg(Register::D0, 0x0e03);
            for (offset, value) in [
                (0, MADE),
                (4, RESULT),
                (8, 1),
                (12, 1024),
                (16, 0),
                (20, 0x8000),
                (24, 1),
            ] {
                runner.bus.write_long(FRAME + offset, value);
            }
            runner
                .dispatcher
                .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
                .unwrap()
                .unwrap();
            assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0);
        }
        let calls = runner.dispatcher.guest_calls.shared_handle();
        let worker = ExecutionTaskId::from_thread_id(runner.bus.read_long(MADE));
        assert_eq!(worker.thread_id(), 3);
        let mut storage = calls.thread_storage(worker).unwrap();
        assert_eq!(storage.managed_pointer, native_worker);
        assert_ne!(storage.stack_base, 0);
        runner.bus.write_long(RESULT, 0x12345678);
        storage.result_destination = u32::MAX - 1;
        assert!(calls.set_thread_storage(worker, storage));
        for (attempt, expected) in [(0, -619_i16), (1, 0), (2, -618)] {
            if attempt == 1 {
                storage.result_destination = RESULT;
                assert!(calls.set_thread_storage(worker, storage));
            }
            if native_worker {
                runner.m68k.cpu.write_reg(Register::A7, FRAME);
                runner.m68k.cpu.write_reg(Register::D0, 0x0504);
                runner
                    .bus
                    .write_word(FRAME, if recycle { 0x0100 } else { 0 });
                runner.bus.write_long(FRAME + 2, 0xcafebabe);
                runner.bus.write_long(FRAME + 6, worker.thread_id());
                runner
                    .dispatcher
                    .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
                    .unwrap()
                    .unwrap();
                assert_eq!(runner.m68k.cpu.read_reg(Register::D0) as i16, expected);
            } else {
                let cpu = &mut runner.native.application_mut().unwrap().cpu;
                cpu.pc = PPC_IMPORT_TRAP_BASE;
                cpu.lr = PPC_CODE_BASE;
                cpu.gpr[3] = worker.thread_id();
                cpu.gpr[4] = 0xcafebabe;
                cpu.gpr[5] = u32::from(recycle);
                assert!(runner.run_steps(32, None).1);
                assert_eq!(
                    runner.native.application().unwrap().cpu.gpr[3] as i16,
                    expected
                );
            }
            assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
            assert_eq!(calls.thread_storage(worker).is_some(), attempt == 0);
            assert_eq!(
                runner.bus.read_long(RESULT),
                if attempt == 0 { 0x12345678 } else { 0xcafebabe }
            );
            if native_worker {
                let manager = runner.dispatcher.process_memory_manager();
                let live = manager
                    .borrow_mut()
                    .native_mut()
                    .native_ptr_records()
                    .iter()
                    .any(|record| record.ptr == storage.stack_base);
                assert_eq!(live, attempt == 0 || recycle);
                assert_eq!(calls.classic_thread_pool_count(0), 0);
            } else {
                assert_eq!(
                    runner.bus.get_alloc_size(storage.stack_base).is_some(),
                    attempt == 0 || recycle
                );
                assert_eq!(
                    calls.classic_thread_pool_count(0),
                    usize::from(attempt != 0 && recycle)
                );
            }
        }
        // Request the recycled stack through its original ABI. A released
        // stack is unavailable, while recycled storage keeps its allocation
        // and acquires a fresh identity and result destination.
        if native_worker {
            let cpu = &mut runner.native.application_mut().unwrap().cpu;
            cpu.pc = PPC_IMPORT_TRAP_BASE;
            cpu.lr = PPC_CODE_BASE;
            cpu.gpr[3] = 1;
            cpu.gpr[4] = PPC_CODE_BASE;
            cpu.gpr[5] = 0xabcdef;
            cpu.gpr[6] = 1024;
            cpu.gpr[7] = 1 | 2 | 16;
            cpu.gpr[8] = RESULT + 4;
            cpu.gpr[9] = MADE;
            assert!(runner.run_steps(32, None).1);
            assert_eq!(
                runner.native.application().unwrap().cpu.gpr[3] as i16,
                if recycle { 0 } else { -617 }
            );
        } else {
            runner.m68k.cpu.write_reg(Register::A7, FRAME);
            runner.m68k.cpu.write_reg(Register::D0, 0x0e03);
            for (offset, value) in [
                (0, MADE),
                (4, RESULT + 4),
                (8, 1 | 2 | 16),
                (12, 1024),
                (16, 0xabcdef),
                (20, 0x8000),
                (24, 1),
            ] {
                runner.bus.write_long(FRAME + offset, value);
            }
            runner
                .dispatcher
                .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
                .unwrap()
                .unwrap();
            assert_eq!(
                runner.m68k.cpu.read_reg(Register::D0) as i16,
                if recycle { 0 } else { -617 }
            );
        }
        assert_eq!(runner.bus.read_long(MADE), if recycle { 4 } else { 0 });
        if recycle {
            let reused = calls
                .thread_storage(ExecutionTaskId::from_thread_id(4))
                .unwrap();
            assert_eq!(reused.stack_base, storage.stack_base);
            assert_eq!(reused.stack_limit, storage.stack_limit);
            assert_eq!(reused.managed_pointer, native_worker);
            assert_eq!(reused.result_destination, RESULT + 4);
            assert!(calls.thread_storage(worker).is_none());
        }
    }
}

#[test]
fn dispose_thread_refuses_the_application_through_both_public_abis() {
    const FRAME: u32 = 0x6000;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let loaded = app.ppc.as_mut().unwrap();
    loaded.entry_pc = PPC_IMPORT_TRAP_BASE;
    loaded.cpu.pc = PPC_IMPORT_TRAP_BASE;
    loaded.memory.add_region(PPC_IMPORT_TRAP_BASE, vec![0; 4]);
    let mut binding = test_ppc_import_binding(0, "InterfaceLib", "DisposeThread");
    binding.dispatcher_target = PpcImportDispatcherTarget::DisposeThread;
    binding.trap_pc = PPC_IMPORT_TRAP_BASE;
    loaded.import_count = 1;
    loaded.imports.push(binding);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let calls = runner.dispatcher.guest_calls.shared_handle();
    let before = calls.clone();
    {
        let cpu = &mut runner.native.application_mut().unwrap().cpu;
        cpu.lr = PPC_CODE_BASE;
        cpu.gpr[3] = ExecutionTaskId::APPLICATION.thread_id();
        cpu.gpr[4] = 0xcafe_babe;
        cpu.gpr[5] = 0;
    }
    assert!(runner.run_steps(32, None).1);
    assert_eq!(
        runner.native.application().unwrap().cpu.gpr[3] as i16,
        crate::thread_manager::THREAD_PROTOCOL_ERR
    );
    assert_eq!(calls, before);

    runner.m68k.cpu.write_reg(Register::A7, FRAME);
    runner.bus.write_word(FRAME, 0);
    runner.bus.write_long(FRAME + 2, 0xcafe_babe);
    runner
        .bus
        .write_long(FRAME + 6, ExecutionTaskId::APPLICATION.thread_id());
    runner.bus.write_word(FRAME + 10, 0xbeef);
    runner.m68k.cpu.write_reg(Register::D0, 0x0504);
    runner
        .dispatcher
        .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::D0) as i16,
        crate::thread_manager::THREAD_PROTOCOL_ERR
    );
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), FRAME + 10);
    assert_eq!(
        runner.bus.read_word(FRAME + 10) as i16,
        crate::thread_manager::THREAD_PROTOCOL_ERR
    );
    assert_eq!(calls, before);
}

#[test]
fn classic_thread_return_trampoline_retries_refused_retirement_without_rts_fallthrough() {
    const FRAME: u32 = 0x6000;
    const YIELD_FRAME: u32 = 0x6100;
    const MADE: u32 = 0x7000;
    const RESULT: u32 = 0x7100;
    const ENTRY: u32 = 0x8000;
    const APP_PC: u32 = 0x9000;
    const PARAM: u32 = 0xdead_beef;
    const THREAD_RESULT: u32 = 0xcafe_babe;

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.bus.write_word(ENTRY, 0x4e75); // RTS
    runner.m68k.cpu.write_reg(Register::A7, FRAME);
    for (offset, value) in [
        (0, MADE),
        (4, RESULT),
        (8, 0),
        (12, 1024),
        (16, PARAM),
        (20, ENTRY),
        (24, 1),
    ] {
        runner.bus.write_long(FRAME + offset, value);
    }
    runner.m68k.cpu.write_reg(Register::D0, 0x0e03);
    runner
        .dispatcher
        .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap()
        .unwrap();
    let worker = ExecutionTaskId::from_thread_id(runner.bus.read_long(MADE));
    let worker_context = runner
        .dispatcher
        .guest_calls
        .cooperative_context(worker)
        .unwrap();
    let worker_sp = worker_context.a_regs[7];
    let trampoline = runner.bus.read_long(worker_sp);
    assert_eq!(runner.bus.read_long(worker_sp + 4), PARAM);

    runner.m68k.cpu.write_reg(Register::PC, APP_PC);
    runner.m68k.cpu.write_reg(Register::A0, 0x1111_2222);
    runner.m68k.cpu.write_reg(Register::A7, YIELD_FRAME);
    runner.bus.write_long(YIELD_FRAME, worker.thread_id());
    runner.bus.write_word(YIELD_FRAME + 4, 0xbeef);
    runner.m68k.cpu.write_reg(Register::D0, 0x0205);
    runner
        .dispatcher
        .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap()
        .unwrap();
    let application_context = runner
        .dispatcher
        .guest_calls
        .cooperative_context(ExecutionTaskId::APPLICATION)
        .unwrap();
    assert_eq!(runner.dispatcher.guest_calls.current_task(), worker);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), ENTRY);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), worker_sp);

    runner.m68k.cpu.write_reg(Register::A0, THREAD_RESULT);
    runner.dispatcher.guest_calls.begin_critical();
    assert!(matches!(
        runner.m68k.cpu.step(&mut runner.bus),
        crate::cpu::StepResult::Ok
    ));
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), worker_sp + 4);
    assert_eq!(runner.bus.read_long(worker_sp + 4), PARAM);

    for _ in 0..2 {
        assert!(matches!(
            runner.m68k.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0) & 0xffff, 0xfffe);
        assert!(matches!(
            runner.m68k.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Aline(0xabf2)
        ));
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline + 6);
        let before = runner.dispatcher.guest_calls.clone();
        runner
            .dispatch_classic_with_process_services(0xabf2)
            .unwrap();
        assert_eq!(runner.dispatcher.guest_calls, before);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), worker_sp + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A0), THREAD_RESULT);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0) as i16, -619);
        assert_eq!(runner.bus.read_long(worker_sp + 4), PARAM);
        assert_eq!(runner.bus.read_long(RESULT), 0);
    }

    let before_bounded_retry = runner.dispatcher.guest_calls.clone();
    let (steps, running) = runner.run_steps(8, None);
    assert_eq!(steps, 8);
    assert!(running);
    assert_eq!(runner.dispatcher.guest_calls, before_bounded_retry);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), worker_sp + 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), THREAD_RESULT);
    assert_eq!(runner.bus.read_long(worker_sp + 4), PARAM);
    assert_eq!(runner.bus.read_long(RESULT), 0);

    assert!(runner.dispatcher.guest_calls.end_critical());
    assert!(matches!(
        runner.m68k.cpu.step(&mut runner.bus),
        crate::cpu::StepResult::Ok
    ));
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline + 4);
    assert!(matches!(
        runner.m68k.cpu.step(&mut runner.bus),
        crate::cpu::StepResult::Aline(0xabf2)
    ));
    runner
        .dispatch_classic_with_process_services(0xabf2)
        .unwrap();

    assert_eq!(
        runner.dispatcher.guest_calls.current_task(),
        ExecutionTaskId::APPLICATION
    );
    assert_eq!(runner.bus.read_long(RESULT), THREAD_RESULT);
    assert_eq!(
        CooperativeThread::capture(&runner.m68k.cpu),
        application_context
    );
    assert_eq!(runner.dispatcher.guest_calls.scheduling_state(worker), None);
    assert_eq!(
        runner.dispatcher.guest_calls.cooperative_context(worker),
        None
    );
}

#[test]
fn stopped_last_thread_waits_for_a_task_reference_wakeup() {
    const CLASSIC_PC: u32 = 0x4000;
    const CLASSIC_SP: u32 = 0x5000;
    for native in [false, true] {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        if native {
            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let loaded = app.ppc.as_mut().unwrap();
            loaded.entry_pc = PPC_IMPORT_TRAP_BASE;
            loaded.cpu.pc = PPC_IMPORT_TRAP_BASE;
            loaded.memory.add_region(PPC_IMPORT_TRAP_BASE, vec![0; 4]);
            loaded.memory.add_region(
                PPC_CODE_BASE,
                [0x3a800055_u32, 0x48000000]
                    .into_iter()
                    .flat_map(u32::to_be_bytes)
                    .collect(),
            );
            let mut binding = test_ppc_import_binding(0, "InterfaceLib", "SetThreadState");
            binding.dispatcher_target = PpcImportDispatcherTarget::SetThreadState;
            binding.trap_pc = PPC_IMPORT_TRAP_BASE;
            loaded.import_count = 1;
            loaded.imports.push(binding);
            runner.init_app(&app);
            let cpu = &mut runner.native.application_mut().unwrap().cpu;
            cpu.lr = PPC_CODE_BASE;
            cpu.gpr[3] = 1;
            cpu.gpr[4] = 1;
            cpu.gpr[5] = 0;
            cpu.gpr[20] = 0;
        } else {
            for (i, word) in [0x303cu16, 0x0508, 0xabf2, 0x7c55, 0x60fe]
                .into_iter()
                .enumerate()
            {
                runner.bus.write_word(CLASSIC_PC + i as u32 * 2, word);
            }
            runner.bus.write_long(CLASSIC_SP, 0);
            runner.bus.write_word(CLASSIC_SP + 4, 1);
            runner.bus.write_long(CLASSIC_SP + 6, 1);
            runner.m68k.cpu.write_reg(Register::PC, CLASSIC_PC);
            runner.m68k.cpu.write_reg(Register::A7, CLASSIC_SP);
            runner.m68k.cpu.write_reg(Register::D6, 0);
        }
        let calls = runner.dispatcher.guest_calls.shared_handle();
        let (steps, running) = runner.run_steps(32, None);
        assert!(steps > 0 && running);
        assert_eq!(
            calls.scheduling_state(ExecutionTaskId::APPLICATION),
            Some(ExecutionTaskState::Stopped)
        );
        assert!(!runner.m68k.can_relaunch());
        for _ in 0..3 {
            assert_eq!(runner.run_steps(32, None), (0, true));
        }
        if native {
            let cpu = &runner.native.application().unwrap().cpu;
            assert_eq!(cpu.pc, PPC_CODE_BASE);
            assert_eq!(cpu.gpr[20], 0);
        } else {
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), CLASSIC_PC + 6);
            assert_eq!(runner.m68k.cpu.read_reg(Register::D6), 0);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), CLASSIC_SP + 10);
        }
        // An interrupt/completion edge may mark the stopped thread ready;
        // the wake call must not execute or replace the suspended CPU.
        let (mut wake, mut cpu, mut bus) = setup();
        wake.guest_calls = calls.shared_handle();
        bus.write_long(TEST_SP, ExecutionTaskId::APPLICATION.thread_id());
        bus.write_long(TEST_SP + 4, 2);
        cpu.write_reg(Register::D0, 0x0410);
        wake.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            calls.scheduling_state(ExecutionTaskId::APPLICATION),
            Some(ExecutionTaskState::Ready)
        );
        let (steps, running) = runner.run_steps(32, None);
        assert!(steps > 0 && running);
        assert!(calls.current_task_is_running());
        if native {
            assert_eq!(runner.native.application().unwrap().cpu.gpr[20], 0x55);
        } else {
            assert_eq!(runner.m68k.cpu.read_reg(Register::D6), 0x55);
        }
    }
}

#[test]
fn native_yield_roundtrips_classic_worker_yield_and_retirement() {
    const CLASSIC_ENTRY: u32 = 0x0305_0000;
    const CLASSIC_SP: u32 = 0x0305_1100;
    const RESULT: u32 = 0x0305_2000;
    for retire in [false, true] {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let native = app.ppc.as_mut().unwrap();
        native.cpu.pc = PPC_IMPORT_TRAP_BASE;
        native.entry_pc = PPC_IMPORT_TRAP_BASE;
        native.cpu.lr = PPC_CODE_BASE;
        native.cpu.gpr[20] = 0x1122_3344;
        native.cpu.fpr[20] = 0x4009_21fb_5444_2d18;
        native.cpu.cr = 0x1357_2468;
        native.memory.add_region(PPC_IMPORT_TRAP_BASE, vec![0; 4]);
        native
            .memory
            .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
        native.memory.add_region(
            CLASSIC_ENTRY,
            [
                0x7c55u16,
                0x303c,
                if retire { 0xfffe } else { 0x0205 },
                0xabf2,
                0x60fe,
            ]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect(),
        );
        native.memory.add_region(CLASSIC_SP, vec![0; 16]);
        native.memory.write_u32_be(CLASSIC_SP, 2).unwrap();
        native.memory.add_region(RESULT, vec![0; 4]);
        let mut binding = test_ppc_import_binding(0, "InterfaceLib", "YieldToAnyThread");
        binding.dispatcher_target = PpcImportDispatcherTarget::YieldToAnyThread;
        binding.trap_pc = PPC_IMPORT_TRAP_BASE;
        native.import_count = 1;
        native.imports.push(binding);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        // Launch installs its entry return convention; use a live continuation
        // that loops after the yield so it cannot halt the test's process.
        runner.native.application_mut().unwrap().cpu.lr = PPC_CODE_BASE;
        let calls = runner.dispatcher.guest_calls.shared_handle();
        let worker = calls.create_task().unwrap();
        let mut context = CooperativeThread::default();
        context.pc = CLASSIC_ENTRY;
        context.a_regs[0] = 0x5566_7788;
        context.a_regs[7] = CLASSIC_SP;
        assert!(calls.set_thread_storage(
            worker,
            crate::guest_call::ThreadStorage {
                result_destination: RESULT,
                ..Default::default()
            }
        ));
        assert!(calls.save_cooperative_context(worker, context));
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        let (_, running) = runner.run_steps(64, None);
        assert!(running);
        assert_eq!(calls.current_task(), worker);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), CLASSIC_ENTRY);
        let (_, running) = runner.run_steps(64, None);
        assert!(running);
        assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D6), 0x55);
        assert!(calls.has_pending_task_handoff());
        assert!(!runner.m68k.can_relaunch());
        if retire {
            assert_eq!(calls.scheduling_state(worker), None);
            assert_eq!(runner.bus.read_long(RESULT), 0x5566_7788);
        }
        let (_, running) = runner.run_steps(64, None);
        assert!(running);
        assert!(!calls.has_pending_task_handoff());
        let native = runner.native.application().unwrap();
        assert_eq!(native.cpu.pc, PPC_CODE_BASE);
        assert_eq!(native.cpu.gpr[20], 0x1122_3344);
        assert_eq!(native.cpu.fpr[20], 0x4009_21fb_5444_2d18);
        assert_eq!(native.cpu.cr, 0x1357_2468);
    }
}

#[test]
fn classic_worker_uses_native_application_engine_and_stops_at_its_return() {
    const ENTRY: u32 = PPC_CODE_BASE + 0x1000;
    const CLASSIC_RETURN: u32 = 0x0304_0000;
    const CLASSIC_SP: u32 = 0x0304_1080;
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let native = app.ppc.as_mut().unwrap();
    native
        .memory
        .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
    native.memory.add_region(
        ENTRY,
        [0x3860_002au32, 0x4e80_0020]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    native.memory.add_region(CLASSIC_RETURN, vec![0x60, 0xfe]);
    native.memory.add_region(CLASSIC_SP - 0x80, vec![0; 0x100]);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let native_pc = runner.native.application().unwrap().cpu.pc;
    let calls = runner.dispatcher.guest_calls.shared_handle();
    let worker = ExecutionTaskId::from_thread_id(3);
    assert!(calls.register_task(worker));
    assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
    assert!(calls.switch_to_task(worker));
    runner.m68k.cpu.write_reg(Register::PC, CLASSIC_RETURN);
    runner.m68k.cpu.write_reg(Register::A7, CLASSIC_SP);
    assert!(calls.begin_m68k_to_powerpc(
        GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: ENTRY,
            rtoc: 0,
        },
        PowerPcArguments::from_slice(&[]).unwrap(),
        CLASSIC_RETURN,
        CLASSIC_SP,
        Some(M68kResultTarget::Data { index: 0, size: 4 }),
    ));
    let (steps, running) = runner.run_steps(64, None);
    assert!(steps > 0);
    assert!(
        running,
        "coalescing must not continue into the suspended application's halt"
    );
    assert!(
        calls.is_empty(),
        "the worker's native call must execute without a companion"
    );
    assert_eq!(calls.current_task(), worker);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 42);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), CLASSIC_RETURN);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), CLASSIC_SP);
    assert_eq!(runner.native.application().unwrap().cpu.pc, native_pc);
    assert_eq!(
        calls.execution_route(runner.native.availability()),
        ExecutionRoute::Classic
    );
}

#[test]
fn nested_cross_isa_callback_survives_a_cooperative_task_switch() {
    for native_worker in [false, true] {
        const OUTER_ENTRY: u32 = 0x0301_0000;
        const OUTER_DESCRIPTOR: u32 = 0x0301_0100;
        const OUTER_TVECTOR: u32 = 0x0301_0200;
        const PPC_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        const CALLBACK_RTOC: u32 = 0x0301_0300;
        const INNER_DESCRIPTOR: u32 = 0x0301_0400;
        const INNER_ENTRY: u32 = 0x0301_0500;
        const STACK_BASE: u32 = 0x0302_0000;
        const INITIAL_SP: u32 = STACK_BASE + 0x80;
        const OUTER_RETURN: u32 = 0x0302_0110;
        const WORKER_ENTRY: u32 = 0x0303_0000;
        const WORKER_STACK: u32 = 0x0303_1000;
        const WORKER_SP: u32 = WORKER_STACK + 0x10;
        const ARGUMENT: u32 = 0x1020_3040;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app
            .memory
            .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
        ppc_app.memory.add_region(
            OUTER_ENTRY,
            [
                0x4ef9,
                (OUTER_DESCRIPTOR >> 16) as u16,
                OUTER_DESCRIPTOR as u16,
            ]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(
            INNER_ENTRY,
            [
                0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                0xabf2, // ThreadDispatch
                0x598f, // SUBQ.L #4,SP (undo the selector frame pop)
                0x4e75, // RTS through the PPC Mixed Mode gateway
            ]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(
            WORKER_ENTRY,
            [
                0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                0xabf2, // ThreadDispatch back to the application task
                0x60fe, // BRA.S -2 if no successor is currently runnable
            ]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(OUTER_DESCRIPTOR, vec![0; 0x100]);
        ppc_app.memory.add_region(INNER_DESCRIPTOR, vec![0; 0x100]);
        ppc_app.memory.add_region(OUTER_TVECTOR, vec![0; 8]);
        ppc_app.memory.add_region(STACK_BASE, vec![0; 0x200]);
        ppc_app.memory.add_region(WORKER_STACK, vec![0; 0x40]);

        // A Pascal native-to-68K call owns one return long followed by its
        // argument. The descriptor consumes both, so the completion boundary
        // is exactly eight bytes above the initial stack pointer.
        ppc_app
            .memory
            .write_u32_be(INITIAL_SP, OUTER_RETURN)
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(INITIAL_SP + 4, ARGUMENT)
            .unwrap();
        ppc_app.memory.write_u32_be(WORKER_SP, 0).unwrap();

        let proc_info = proc_info::PASCAL_STACK_BASED
            | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE);
        ppc_app
            .memory
            .write_u16_be(OUTER_DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
            .unwrap();
        ppc_app
            .memory
            .write_u8(OUTER_DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
            .unwrap();
        ppc_app
            .memory
            .write_u16_be(OUTER_DESCRIPTOR + 10, 0)
            .unwrap();
        let outer_record = OUTER_DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
        ppc_app
            .memory
            .write_u32_be(outer_record, proc_info)
            .unwrap();
        ppc_app
            .memory
            .write_u8(
                outer_record + ROUTINE_RECORD_ISA_OFFSET,
                ROUTINE_RECORD_POWERPC_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u16_be(
                outer_record + ROUTINE_RECORD_FLAGS_OFFSET,
                ROUTINE_FLAG_USE_NATIVE_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(
                outer_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
                OUTER_TVECTOR,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(OUTER_TVECTOR, PPC_CALLBACK)
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(OUTER_TVECTOR + 4, CALLBACK_RTOC)
            .unwrap();

        let inner_proc_info = proc_info::PASCAL_STACK_BASED;
        ppc_app
            .memory
            .write_u16_be(INNER_DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
            .unwrap();
        ppc_app
            .memory
            .write_u8(INNER_DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
            .unwrap();
        ppc_app
            .memory
            .write_u16_be(INNER_DESCRIPTOR + 10, 0)
            .unwrap();
        let inner_record = INNER_DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
        ppc_app
            .memory
            .write_u32_be(inner_record, inner_proc_info)
            .unwrap();
        ppc_app
            .memory
            .write_u8(
                inner_record + ROUTINE_RECORD_ISA_OFFSET,
                ROUTINE_RECORD_M68K_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(
                inner_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
                INNER_ENTRY,
            )
            .unwrap();

        let callback_words = [
            0x7fe8_02a6, // MFLR R31
            0x3c60_0000 | (INNER_DESCRIPTOR >> 16),
            0x6063_0000 | (INNER_DESCRIPTOR & 0xffff),
            0x3c80_0000, // LIS R4,0 (void ProcInfo)
            0x6084_0000, // ORI R4,R4,0
            0x38a0_0000, // LI R5,0 (unused for a void signature)
            ppc_test_relative_branch(PPC_CALLBACK + 6 * 4, PPC_IMPORT_TRAP_BASE) | 1,
            0x7fe8_03a6, // MTLR R31
            0x3863_0007, // ADDI R3,R3,7
            0x4e80_0020, // BLR
        ];
        ppc_app.memory.add_region(
            PPC_CALLBACK,
            callback_words
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        let mut call_universal_proc =
            test_ppc_import_binding(0, "InterfaceLib", "CallUniversalProc");
        call_universal_proc.trap_pc = PPC_IMPORT_TRAP_BASE;
        call_universal_proc.dispatcher_target = PpcImportDispatcherTarget::CallUniversalProc;
        ppc_app.import_count = 1;
        ppc_app.imports = vec![call_universal_proc];
        if native_worker {
            let mut yielding = test_ppc_import_binding(1, "InterfaceLib", "YieldToThread");
            yielding.dispatcher_target = PpcImportDispatcherTarget::YieldToThread;
            yielding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
            ppc_app
                .memory
                .add_region(PPC_IMPORT_TRAP_BASE + 4, vec![0; 4]);
            ppc_app.imports.push(yielding);
            ppc_app.import_count = 2;
        }

        assert!(ppc_app
            .toolbox_startup
            .execution
            .calls()
            .begin_powerpc_to_m68k(
                crate::guest_call::GuestCallTarget {
                    isa: crate::guest_procedure::GuestIsa::M68k,
                    entry: OUTER_ENTRY,
                    rtoc: 0,
                },
                OUTER_ENTRY,
                INITIAL_SP,
                OUTER_RETURN,
                INITIAL_SP + 8,
                crate::guest_call::M68kRegisterState::default(),
                None,
                PPC_CODE_BASE,
                0,
                PpcNativeReturnGpr3::Preserve,
            ));

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        if native_worker {
            let mut cpu = PpcCpu::new();
            cpu.pc = PPC_IMPORT_TRAP_BASE + 4;
            cpu.lr = PPC_CODE_BASE;
            cpu.gpr[1] = WORKER_SP;
            cpu.gpr[3] = ExecutionTaskId::APPLICATION.thread_id();
            let task = runner
                .dispatcher
                .guest_calls
                .create_native_thread(
                    crate::guest_call::NativeThreadContext {
                        context: cpu.capture_execution_context(),
                    },
                    crate::guest_call::ThreadStorage {
                        result_destination: 0,
                        stack_base: 0,
                        stack_limit: 0,
                        managed_pointer: true,
                    },
                    true,
                    |_| true,
                )
                .unwrap();
            assert_eq!(task.thread_id(), 3);
        } else {
            assert!(runner
                .dispatcher
                .guest_calls
                .register_task(ExecutionTaskId::from_thread_id(3)));
            assert!(runner.dispatcher.guest_calls.set_thread_storage(
                ExecutionTaskId::from_thread_id(3),
                crate::guest_call::ThreadStorage {
                    stack_base: WORKER_STACK,
                    stack_limit: WORKER_STACK + 0x40,
                    ..Default::default()
                }
            ));
            runner.dispatcher.guest_calls.save_cooperative_context(
                ExecutionTaskId::from_thread_id(3),
                CooperativeThread {
                    d_regs: [0; 8],
                    a_regs: [0, 0, 0, 0, 0, 0, 0, WORKER_SP],
                    pc: WORKER_ENTRY,
                    ccr: 0,
                    extended: None,
                    switch_in: (0, 0),
                    switch_out: (0, 0),
                    terminator: (0, 0),
                },
            );
        }
        assert!(runner.dispatcher.guest_calls.set_scheduling_state(
            ExecutionTaskId::from_thread_id(3),
            crate::execution_kernel::ExecutionTaskState::Ready
        ));

        let (_, running) = runner.run_steps(128, None);
        assert!(running);
        assert_eq!(runner.dispatcher.guest_calls.current_task().thread_id(), 3);
        assert_eq!(
            runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .task_len(ExecutionTaskId::APPLICATION),
            1,
            "the application-owned nested 68K context must stay parked while its callback yields"
        );
        assert_eq!(
            runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .task_len(ExecutionTaskId::from_thread_id(3)),
            0,
            "the worker must not consume the application's parked context"
        );

        let (_, running) = runner.run_steps(128, None);
        assert!(running);
        assert_eq!(runner.dispatcher.guest_calls.current_task().thread_id(), 2);
        if !native_worker {
            assert_eq!(
                runner
                    .dispatcher
                    .guest_calls
                    .m68k_context_bank()
                    .borrow()
                    .task_len(ExecutionTaskId::APPLICATION),
                1,
                "returning from the worker must leave the nested application context parked"
            );
        }

        if !runner.dispatcher.guest_calls.is_empty() {
            let (_, running) = runner.run_steps(128, None);
            assert!(running);
        }
        assert_eq!(runner.dispatcher.guest_calls.current_task().thread_id(), 2);
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert!(runner
            .dispatcher
            .guest_calls
            .m68k_context_bank()
            .borrow()
            .is_empty());
        let ppc_app = runner.native.application().expect("PPC app retained");
        assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
        // The outer routine has a void ProcInfo, so its internal callback's
        // transient R3 value must not leak into the parked native caller.
        assert_eq!(ppc_app.cpu.gpr[3], 0);
    }
}
