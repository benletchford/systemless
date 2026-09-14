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
