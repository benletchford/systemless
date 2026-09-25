use super::*;

    fn invoke_worker_interrupt_for_test(loaded: &mut PpcLoadedApp, kind: u8) -> PpcRunResult {
        const OUTPUT: u32 = PPC_DATA_BASE + 0x3000;
        const VECTOR: u32 = PPC_DATA_BASE + 0x4000;
        match kind {
            0 => {
                loaded
                    .run_timer_callback(OUTPUT, VECTOR, 64, false, false, None, None)
                    .invocation
                    .result
            }
            1 => {
                loaded
                    .run_vbl_callback(OUTPUT, VECTOR, 64, false, false, None, None)
                    .invocation
                    .result
            }
            2 => {
                loaded
                    .run_sound_completion_callback(
                        PpcSoundCompletionRecord {
                            file_playback_index: 0,
                            channel: OUTPUT,
                            completion: VECTOR,
                            command: Some(PpcSndCommandRecord {
                                channel: OUTPUT,
                                command: 13,
                                param1: 7,
                                param2: 42,
                            }),
                            tick: 0,
                            instruction_count: 0,
                            scheduled_tick: 0,
                            scheduled_instruction_count: 0,
                        },
                        64,
                        false,
                        false,
                    )
                    .invocation
                    .result
            }
            3 => {
                loaded
                    .run_sound_doubleback_callback(
                        PpcSoundDoubleBackRecord {
                            architecture: CallbackTaskArchitecture::PowerPc,
                            channel: OUTPUT,
                            header: 0,
                            exhausted_buffer: 0,
                            exhausted_buffer_index: 0,
                            callback: VECTOR,
                            tick: 0,
                            instruction_count: 0,
                        },
                        64,
                        false,
                        false,
                    )
                    .invocation
                    .result
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn interrupt_callbacks_preserve_time_base_on_return_fault_and_cycle_limit() {
        const OUTPUT: u32 = PPC_DATA_BASE + 0x3000;
        const VECTOR: u32 = PPC_DATA_BASE + 0x4000;
        const ENTRY: u32 = PPC_CODE_BASE + 0x2000;
        const START: u64 = 0xffff_fffc;
        for kind in 0..4 {
            for outcome in 0..3 {
                let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
                loaded.memory.add_region(OUTPUT, vec![0; 8]);
                loaded.memory.add_region(VECTOR, vec![0; 8]);
                loaded.memory.write_u32_be(VECTOR, ENTRY).unwrap();
                loaded
                    .memory
                    .write_u32_be(VECTOR + 4, PPC_DATA_BASE)
                    .unwrap();
                // Cross the lower-word carry boundary, then let the callback
                // publish both time-base halves before returning or stopping.
                let mut code = vec![0x6000_0000; 4];
                code.extend([
                    xfx_form(31, 5, 268, 371), // mftb r5
                    d_form_u(36, 5, 3, 0),
                    xfx_form(31, 6, 269, 371), // mftbu r6
                    d_form_u(36, 6, 3, 4),
                    match outcome {
                        0 => BLR,
                        1 => d_form_u(32, 8, 20, 0), // unmapped load
                        _ => 0x4800_0000,            // b .
                    },
                ]);
                loaded
                    .memory
                    .add_region(ENTRY, code.into_iter().flat_map(u32::to_be_bytes).collect());
                loaded.cpu.gpr[20] = 0xdead_0000;
                loaded.cpu.fpr[20] = 0x4009_21fb_5444_2d18;
                loaded.cpu.cr = 0x1357_2468;
                loaded.cpu.fpscr = 0x1020_3040;
                loaded.cpu.msr = 0x5060_7080;
                loaded.cpu.alignment_policy = PpcAlignmentPolicy::EmulateData;
                loaded.cpu.set_time_base(START);
                establish_loaded_reservation(&mut loaded, OUTPUT);
                let saved = loaded.cpu.clone();
                let result = invoke_worker_interrupt_for_test(&mut loaded, kind);
                match outcome {
                    0 => assert!(matches!(result, PpcRunResult::Halted { .. })),
                    1 => assert!(matches!(
                        result,
                        PpcRunResult::MemoryFault {
                            addr: 0xdead_0000,
                            ..
                        }
                    )),
                    _ => assert!(matches!(result, PpcRunResult::CycleLimit { .. })),
                }
                let observed = (u64::from(loaded.memory.read_u32_be(OUTPUT + 4).unwrap()) << 32)
                    | u64::from(loaded.memory.read_u32_be(OUTPUT).unwrap());
                assert!(observed > START);
                let elapsed = loaded.cpu.time_base();
                assert!(
                    elapsed >= observed,
                    "callback family {kind}, outcome {outcome} rewound time"
                );
                assert!(elapsed >= START + ppc_run_result_cycles(result));
                assert_eq!(loaded.cpu.pc, saved.pc);
                assert_eq!(loaded.cpu.gpr, saved.gpr);
                assert_eq!(loaded.cpu.fpr, saved.fpr);
                assert_eq!(loaded.cpu.cr, saved.cr);
                assert_eq!(loaded.cpu.lr, saved.lr);
                assert_eq!(loaded.cpu.ctr, saved.ctr);
                assert_eq!(loaded.cpu.xer, saved.xer);
                assert_eq!(loaded.cpu.fpscr, saved.fpscr);
                assert_eq!(loaded.cpu.msr, saved.msr);
                assert_eq!(loaded.cpu.alignment_policy, saved.alignment_policy);
                assert_eq!(loaded.cpu.reservation_address(), None);
                // The resumed caller observes that same elapsed time through
                // guest instructions, rather than only a host-side accessor.
                loaded.cpu.step_instruction(xfx_form(31, 11, 268, 371));
                assert_eq!(loaded.cpu.gpr[11], elapsed as u32);
                loaded.cpu.step_instruction(xfx_form(31, 12, 269, 371));
                assert_eq!(loaded.cpu.gpr[12], ((elapsed + 1) >> 32) as u32);
            }
        }
    }

    #[test]
    fn interrupt_callbacks_retain_wrapped_live_engine_time() {
        const VECTOR: u32 = PPC_DATA_BASE + 0x4000;
        const ENTRY: u32 = PPC_CODE_BASE + 0x2000;
        for kind in 0..4 {
            let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
            loaded.memory.add_region(VECTOR, vec![0; 8]);
            loaded.memory.write_u32_be(VECTOR, ENTRY).unwrap();
            loaded
                .memory
                .write_u32_be(VECTOR + 4, PPC_DATA_BASE)
                .unwrap();
            loaded.memory.add_region(ENTRY, BLR.to_be_bytes().to_vec());
            loaded.cpu.set_time_base(u64::MAX);

            let result = invoke_worker_interrupt_for_test(&mut loaded, kind);
            assert!(matches!(result, PpcRunResult::Halted { cycles: 1, .. }), "{kind}");
            assert_eq!(loaded.cpu.time_base(), 0, "callback family {kind}");
            assert_eq!(
                loaded
                    .cpu
                    .step(&mut loaded.memory, xfx_form(31, 11, 268, 371)),
                ppc::PpcStepResult::Stepped
            );
            assert_eq!(loaded.cpu.gpr[11], 0, "callback family {kind}");
            assert_eq!(loaded.cpu.time_base(), 1, "callback family {kind}");
        }
    }

    #[test]
    fn interrupt_callbacks_restore_the_interrupted_private_import_continuation() {
        const VECTOR: u32 = PPC_DATA_BASE + 0x4000;
        const ENTRY: u32 = PPC_CODE_BASE + 0x2000;
        const TRAP: u32 = PPC_CODE_BASE + 0x3000;
        const RETURN: u32 = PPC_CODE_BASE + 0x3100;
        const FINAL: u32 = PPC_CODE_BASE + 0x3200;
        for kind in 0..4 {
            let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
            loaded.memory.add_region(VECTOR, vec![0; 8]);
            loaded.memory.write_u32_be(VECTOR, ENTRY).unwrap();
            loaded
                .memory
                .write_u32_be(VECTOR + 4, PPC_DATA_BASE)
                .unwrap();
            loaded.memory.add_region(ENTRY, BLR.to_be_bytes().to_vec());
            crate::guest_call::seed_pending_native_import_context(
                &mut loaded.cpu,
                &mut loaded.memory,
                TRAP,
                RETURN,
                0xaaaa_0002,
                RETURN,
                FINAL,
                0xbbbb_0002,
                PpcNativeReturnGpr3::Set(0xcccc_0003),
            );
            let result = invoke_worker_interrupt_for_test(&mut loaded, kind);
            assert!(matches!(result, PpcRunResult::Halted { cycles: 1, .. }));
            assert_eq!(loaded.cpu.pc, RETURN);
            assert_eq!(
                loaded.cpu.run_with_imports(
                    &mut loaded.memory,
                    2,
                    FINAL,
                    0,
                    0,
                    |_, _, _| unreachable!()
                ),
                PpcRunResult::Halted {
                    pc: FINAL,
                    cycles: 1
                }
            );
            assert_eq!(
                (loaded.cpu.gpr[2], loaded.cpu.gpr[3]),
                (0xbbbb_0002, 0xcccc_0003)
            );
            assert_eq!(
                loaded.cpu.run_with_imports(
                    &mut loaded.memory,
                    2,
                    FINAL,
                    0,
                    0,
                    |_, _, _| unreachable!()
                ),
                PpcRunResult::Halted {
                    pc: FINAL,
                    cycles: 0
                }
            );
        }
    }

    #[test]
    fn interrupt_callbacks_protect_worker_frames_and_refuse_exhausted_stacks() {
        use crate::guest_call::ExecutionTaskId;
        const MADE: u32 = PPC_DATA_BASE + 0x2000;
        const OUTPUT: u32 = PPC_DATA_BASE + 0x3000;
        const VECTOR: u32 = PPC_DATA_BASE + 0x4000;
        const ENTRY: u32 = PPC_CODE_BASE + 0x2000;
        for kind in 0..4 {
            let mut loaded =
                load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
            loaded.memory.add_region(MADE, vec![0; 4]);
            loaded.cpu.gpr[3] = 1;
            loaded.cpu.gpr[4] = PPC_CODE_BASE;
            loaded.cpu.gpr[5] = 17;
            loaded.cpu.gpr[6] = 4096;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            loaded.cpu.gpr[9] = MADE;
            loaded.run_with_hle_imports(64);
            assert_eq!(loaded.cpu.gpr[3], 0);
            let worker = ExecutionTaskId::from_thread_id(loaded.memory.read_u32_be(MADE).unwrap());
            assert!(loaded.toolbox_startup.execution.calls()
                .yield_native_thread(&mut loaded.cpu, worker.thread_id())
                .unwrap());
            let storage = loaded.guest_calls().thread_storage(worker).unwrap();
            let worker_sp = loaded.cpu.gpr[1];
            assert!(worker_sp < loaded.stack_base);
            loaded.memory.add_region(OUTPUT, vec![0; 4]);
            loaded.memory.add_region(VECTOR, vec![0; 8]);
            loaded.memory.write_u32_be(VECTOR, ENTRY).unwrap();
            loaded
                .memory
                .write_u32_be(VECTOR + 4, PPC_DATA_BASE)
                .unwrap();
            // Store the callback SP for observation, then overwrite a parameter
            // slot. This must never touch the interrupted frame or Red Zone.
            loaded.memory.add_region(
                ENTRY,
                [
                    d_form_u(36, 1, 3, 0),
                    d_form_u(36, 3, 1, PPC_PARAMETER_AREA_OFFSET as u16),
                    BLR,
                ]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
            );
            loaded
                .memory
                .add_region(storage.stack_base - 512, vec![0x5a; 512]);
            loaded
                .memory
                .add_region(storage.stack_limit, vec![0x5a; 128]);
            for invalid_sp in [storage.stack_base + 128, storage.stack_limit + 16, 16] {
                loaded.cpu.gpr[1] = invalid_sp;
                establish_loaded_reservation(&mut loaded, OUTPUT);
                let saved = loaded.cpu.clone();
                let frame = ppc_interrupt_callback_stack_pointer(invalid_sp);
                let before = ppc_memory_read_bytes(&mut loaded.memory, frame, 64);
                let result = invoke_worker_interrupt_for_test(&mut loaded, kind);
                assert_eq!(
                    result,
                    PpcRunResult::MemoryFault {
                        pc: saved.pc,
                        addr: frame,
                        was_write: true,
                        cycles: 0,
                    },
                    "callback family {kind}"
                );
                assert_eq!(ppc_memory_read_bytes(&mut loaded.memory, frame, 64), before);
                assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(0));
                assert_eq!(loaded.cpu.gpr, saved.gpr);
                assert_eq!(loaded.cpu.fpr, saved.fpr);
                assert_eq!(loaded.cpu.pc, saved.pc);
                assert_eq!(loaded.cpu.lr, saved.lr);
                assert_eq!(loaded.cpu.cr, saved.cr);
                assert_eq!(loaded.cpu.time_base(), saved.time_base());
                assert_eq!(loaded.cpu.reservation_address(), saved.reservation_address());
                assert_eq!(loaded.guest_calls().current_task(), worker);
            }
            loaded.cpu.gpr[1] = worker_sp;
            // A valid SP is insufficient when only part of the frame is
            // writable. Refusal must not clear even the accessible prefix.
            let frame = ppc_interrupt_callback_stack_pointer(worker_sp);
            establish_loaded_reservation(&mut loaded, OUTPUT);
            let memory = std::mem::replace(&mut loaded.memory, PpcSectionMem::new());
            loaded.memory.add_region(frame, vec![0x5a; 32]);
            let saved = loaded.cpu.clone();
            let result = invoke_worker_interrupt_for_test(&mut loaded, kind);
            assert!(matches!(
                result,
                PpcRunResult::MemoryFault { cycles: 0, .. }
            ));
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, frame, 32),
                Some(vec![0x5a; 32])
            );
            assert_eq!(loaded.cpu.gpr, saved.gpr);
            assert_eq!(loaded.cpu.pc, saved.pc);
            assert_eq!(loaded.cpu.reservation_address(), saved.reservation_address());
            loaded.memory = memory;
            let protected_base = worker_sp - PPC_INTERRUPT_RED_ZONE_SIZE;
            let protected = vec![0xa5; (PPC_INTERRUPT_RED_ZONE_SIZE + 64) as usize];
            loaded
                .memory
                .write_bytes(protected_base, &protected)
                .unwrap();
            establish_loaded_reservation(&mut loaded, OUTPUT);
            let saved = loaded.cpu.clone();
            let result = invoke_worker_interrupt_for_test(&mut loaded, kind);
            assert!(
                matches!(result, PpcRunResult::Halted { .. }),
                "callback family {kind}: {result:?}"
            );
            let frame = ppc_interrupt_callback_stack_pointer(worker_sp);
            assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(frame));
            assert_eq!(loaded.memory.read_u32_be(frame), Some(worker_sp));
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, protected_base, protected.len() as u32),
                Some(protected)
            );
            assert_eq!(loaded.cpu.gpr, saved.gpr);
            assert_eq!(loaded.cpu.pc, saved.pc);
            assert_eq!(loaded.cpu.reservation_address(), None);
            assert_eq!(loaded.guest_calls().current_task(), worker);
        }
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
