use super::*;
use crate::cpu::Register;

fn cfm_test_connection(id: u32) -> crate::cfm::CfmConnection {
    crate::cfm::CfmConnection {
        id,
        library_name: format!("existing-{id}"),
        main_addr: 0,
        init_addr: 0,
        term_addr: 0,
        exports: vec![],
    }
}

#[test]
fn find_symbol_classic_lookup_stages_native_bindings_and_returns_callable_identity() {
    use crate::loader::ppc::tests::synthetic_pef_with_import;
    const OUTPUT: u32 = PPC_HEAP_BASE + 0x1000;
    const CODE: u32 = 0x18000;
    const STACK: u32 = 0x19000;
    let mut native = load_pef_application(&synthetic_pef_with_import(b"GetSharedLibrary")).unwrap();
    native.memory.add_region(OUTPUT, vec![0xa5; 256]);
    native
        .memory
        .write_bytes(OUTPUT + 32, b"\x0cInterfaceLib")
        .unwrap();
    native.cpu.gpr[3] = OUTPUT + 32;
    native.cpu.gpr[4] = u32::from_be_bytes(*b"pwpc");
    native.cpu.gpr[5] = 1;
    native.cpu.gpr[6] = OUTPUT;
    native.cpu.gpr[7] = OUTPUT + 4;
    native.cpu.gpr[8] = OUTPUT + 8;
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_ppc_companion(native);
    let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
    let native = context.adapter_mut();
    let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
        native.run_with_process_services(128, false, false, mm, cfm)
    });
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(native.cpu.gpr[3], 0);
    let id = native.memory.read_u32_be(OUTPUT).unwrap();
    let initial_count = native.import_count;
    native
        .memory
        .write_bytes(OUTPUT + 32, b"\x09TickCount")
        .unwrap();
    native.memory.add_readonly_region(OUTPUT + 64, vec![0xa5]);
    assert!(runner.native.restore(context).is_ok());
    for attempt in 0..3 {
        for (offset, word) in [0x3f3c, 5, 0xaa5a, 0x60fe].into_iter().enumerate() {
            runner.bus.write_word(CODE + offset as u32 * 2, word);
        }
        runner
            .bus
            .write_long(STACK + 2, OUTPUT + if attempt == 0 { 64 } else { 65 });
        runner.bus.write_long(STACK + 6, OUTPUT + 60);
        runner.bus.write_long(STACK + 10, OUTPUT + 32);
        runner.bus.write_long(STACK + 14, id);
        runner.m68k.cpu.write_reg(Register::PC, CODE);
        runner.m68k.cpu.write_reg(Register::A7, STACK + 2);
        runner.m68k.cpu.write_reg(Register::D0, 0xdead_beef);
        let (steps, running) = runner.run_steps(8, None);
        assert!(running && steps > 0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), STACK + 18);
        assert_eq!(
            runner.bus.read_word(STACK + 18) as i16,
            if attempt == 0 { -50 } else { 0 }
        );
        let native = runner.native.companion().unwrap();
        assert_eq!(native.import_count, initial_count + u32::from(attempt != 0));
        if attempt == 0 {
            assert_eq!(runner.bus.read_long(OUTPUT + 60), 0xa5a5_a5a5);
        } else {
            assert_eq!(runner.bus.read_byte(OUTPUT + 65), 2);
        }
    }
    let address = runner.bus.read_long(OUTPUT + 60);
    runner
        .bus
        .write_long(crate::memory::globals::addr::TICKS, 0x2345);
    let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
    let native = context.adapter_mut();
    native.cpu.pc = native.memory.read_u32_be(address).unwrap();
    native.cpu.gpr[2] = native.memory.read_u32_be(address + 4).unwrap();
    native.cpu.lr = PPC_HALT_PC;
    let probe = runner
        .process_context
        .with_memory_and_cfm(|mm, cfm| native.run_with_process_services(64, false, false, mm, cfm));
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(native.cpu.gpr[3], 0x2345);
    native.imports[0].dispatcher_target = PpcImportDispatcherTarget::FindSymbol;
    native.cpu.pc = native.entry_pc;
    native.cpu.lr = PPC_HALT_PC;
    native.cpu.gpr[3] = id;
    native.cpu.gpr[4] = OUTPUT + 32;
    native.cpu.gpr[5] = OUTPUT + 80;
    native.cpu.gpr[6] = OUTPUT + 84;
    let probe = runner
        .process_context
        .with_memory_and_cfm(|mm, cfm| native.run_with_process_services(64, false, false, mm, cfm));
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(native.cpu.gpr[3], 0);
    assert_eq!(native.memory.read_u32_be(OUTPUT + 80), Some(address));
    assert_eq!(native.import_count, initial_count + 1);
}

#[test]
fn find_symbol_during_native_to_classic_callback_borrows_the_checked_out_adapter() {
    use crate::guest_call::{GuestCallTarget, M68kRegisterState, M68kResultSource};
    use crate::guest_procedure::GuestIsa;
    use ppc::PpcNativeReturnGpr3;
    const CALLBACK: u32 = 0x18000;
    const STACK: u32 = 0x19000;
    const RETURN: u32 = 0x1a000;
    const OUTPUT: u32 = 0x1b000;
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let native = app.ppc.as_mut().unwrap();
    let mut connection = cfm_test_connection(7);
    connection.library_name = "InterfaceLib".into();
    native.cfm.as_mut().unwrap().connections = vec![connection];
    native.memory.add_region(CALLBACK, vec![0; 0x4000]);
    native
        .memory
        .write_bytes(OUTPUT + 32, b"\x09TickCount")
        .unwrap();
    let mut words = vec![0x3f3c, 0]; // Pascal result slot.
    for argument in [7, OUTPUT + 32, OUTPUT, OUTPUT + 4] {
        words.extend([0x2f3c, (argument >> 16) as u16, argument as u16]);
    }
    words.extend([0x3f3c, 5, 0xaa5a, 0x548f, 0x4e75]);
    for (i, word) in words.into_iter().enumerate() {
        native
            .memory
            .write_u16_be(CALLBACK + i as u32 * 2, word)
            .unwrap();
    }
    native.memory.write_u32_be(STACK, RETURN).unwrap();
    assert!(native.guest_calls().begin_powerpc_to_m68k(
        GuestCallTarget {
            isa: GuestIsa::M68k,
            entry: CALLBACK,
            rtoc: 0
        },
        CALLBACK,
        STACK,
        RETURN,
        STACK + 4,
        M68kRegisterState::default(),
        Some(M68kResultSource::Data(0)),
        PPC_CODE_BASE,
        0,
        PpcNativeReturnGpr3::Preserve,
    ));
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let (steps, _) = runner.run_steps(64, None);
    assert!(steps > 0);
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(runner.bus.read_byte(OUTPUT + 4), 2);
    let native = runner.native.application().unwrap();
    assert_eq!(native.imports.len(), 1);
    assert_eq!(native.imports[0].symbol_name, "TickCount");
    assert_eq!(runner.bus.read_long(OUTPUT), native.imports[0].address);
}

#[test]
fn cfm_symbol_enumeration_observes_native_load_and_close_from_classic_execution() {
    use crate::loader::ppc::tests::{
        synthetic_pef_with_enumerable_exports, synthetic_pef_with_import,
    };
    const OUTPUT: u32 = PPC_HEAP_BASE + 0x1000;
    const FRAGMENT: u32 = PPC_HEAP_BASE + 0x2000;
    const CODE: u32 = 0x18000;
    const STACK: u32 = 0x19000;
    let fragment = synthetic_pef_with_enumerable_exports();
    let mut native = load_pef_application(&synthetic_pef_with_import(b"GetMemFragment")).unwrap();
    native.memory.add_region(OUTPUT, vec![0xa5; 256]);
    native.memory.add_region(FRAGMENT, fragment.clone());
    native.cpu.gpr[3] = FRAGMENT;
    native.cpu.gpr[4] = fragment.len() as u32;
    native.cpu.gpr[5] = 0;
    native.cpu.gpr[6] = 1;
    native.cpu.gpr[7] = OUTPUT;
    native.cpu.gpr[8] = OUTPUT + 4;
    native.cpu.gpr[9] = OUTPUT + 8;
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_ppc_companion(native);
    let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
    let native = context.adapter_mut();
    let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
        native.run_with_process_services(128, false, false, mm, cfm)
    });
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(native.cpu.gpr[3], 0);
    let id = native.memory.read_u32_be(OUTPUT).unwrap();
    assert_ne!(id, 0xa5a5_a5a5);
    assert_eq!(
        runner.process_context.cfm().connections[0].exports[0].name,
        "Café™"
    );
    assert!(runner.native.restore(context).is_ok());
    for selector in [5u16, 6, 7] {
        runner.bus.write_word(CODE, 0x3f3c); // MOVE.W #selector,-(SP), Apple inline glue.
        runner.bus.write_word(CODE + 2, selector);
        runner.bus.write_word(CODE + 4, 0xaa5a);
        runner.bus.write_word(CODE + 6, 0x60fe);
        runner.m68k.cpu.write_reg(Register::PC, CODE);
        runner.m68k.cpu.write_reg(Register::A7, STACK + 2);
        runner.m68k.cpu.write_reg(Register::D0, 0xdead_beef);
        if selector == 5 {
            for (i, byte) in b"\x05Caf\x8e\xaa".iter().enumerate() {
                runner.bus.write_byte(OUTPUT + 96 + i as u32, *byte);
            }
            runner.bus.write_long(STACK + 2, OUTPUT + 64);
            runner.bus.write_long(STACK + 6, OUTPUT + 60);
            runner.bus.write_long(STACK + 10, OUTPUT + 96);
            runner.bus.write_long(STACK + 14, id);
        } else if selector == 6 {
            runner.bus.write_long(STACK + 2, OUTPUT + 16);
            runner.bus.write_long(STACK + 6, id);
        } else {
            runner.bus.write_long(STACK + 2, OUTPUT + 64);
            runner.bus.write_long(STACK + 6, OUTPUT + 60);
            runner.bus.write_long(STACK + 10, OUTPUT + 32);
            runner.bus.write_long(STACK + 14, 0);
            runner.bus.write_long(STACK + 18, id);
        }
        let (steps, running) = runner.run_steps(8, None);
        assert!(running && steps > 0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::A7),
            STACK
                + match selector {
                    5 => 18,
                    6 => 10,
                    _ => 22,
                }
        );
    }
    let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
    let native = context.adapter_mut();
    assert_eq!(native.memory.read_u32_be(OUTPUT + 16), Some(1));
    assert_eq!(native.memory.read_u32_be(OUTPUT + 60), Some(0x1234_5678));
    assert_eq!(native.memory.read_u8(OUTPUT + 64), Some(1));
    assert_eq!(native.memory.read_u8(OUTPUT + 36), Some(0x8e));
    native.imports[0].dispatcher_target = PpcImportDispatcherTarget::CloseConnection;
    native.cpu.pc = native.entry_pc;
    native.cpu.lr = PPC_HALT_PC;
    native.cpu.gpr[3] = OUTPUT;
    let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
        native.run_with_process_services(128, false, false, mm, cfm)
    });
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(native.cpu.gpr[3], 0);
    assert!(runner.process_context.cfm().connections.is_empty());
    assert!(runner.native.restore(context).is_ok());
    runner.bus.write_word(CODE + 2, 6);
    runner.m68k.cpu.write_reg(Register::PC, CODE);
    runner.m68k.cpu.write_reg(Register::A7, STACK + 2);
    runner.bus.write_long(STACK + 2, OUTPUT + 16);
    runner.bus.write_long(STACK + 6, id);
    let _ = runner.run_steps(8, None);
    assert_eq!(runner.bus.read_word(STACK + 10) as i16, -2801);
    assert_eq!(
        runner.bus.read_long(OUTPUT + 16),
        1,
        "refused query preserves its previous output"
    );
}

#[test]
fn runner_cfm_owns_native_connections_ids_and_library_seeds() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut native = load_pef_application(&crate::loader::ppc::tests::synthetic_pef_with_import(
        b"GetSharedLibrary",
    ))
    .unwrap();
    const OUTPUT: u32 = PPC_HEAP_BASE + 0x200;
    native.memory.add_region(OUTPUT, vec![0; 128]);
    native
        .memory
        .write_bytes(OUTPUT, b"\x0cInterfaceLib")
        .unwrap();
    native.cpu.gpr[3] = OUTPUT;
    native.cpu.gpr[4] = u32::from_be_bytes(*b"pwpc");
    native.cpu.gpr[5] = 1;
    native.cpu.gpr[6] = OUTPUT + 64;
    native.cpu.gpr[7] = OUTPUT + 68;
    native.cpu.gpr[8] = OUTPUT + 72;
    native
        .cfm
        .as_mut()
        .unwrap()
        .connections
        .push(cfm_test_connection(3));
    native.cfm.as_mut().unwrap().next_connection_id = 7;
    native.seed_cfm_library_fragments(vec![PpcCfmLibraryFragment {
        name: "seeded library".into(),
        bytes: vec![1, 2, 3],
    }]);
    app.ppc = Some(native);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    assert!(runner.native.application().unwrap().cfm.is_none());
    assert_eq!(runner.process_context.cfm_mut().next_connection_id, 7);
    runner.run_steps(128, None);
    assert_eq!(runner.bus.read_long(OUTPUT + 64), 7);
    assert_eq!(runner.process_context.cfm_mut().next_connection_id, 8);
    assert_eq!(
        runner
            .process_context
            .cfm_mut()
            .connections
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>(),
        vec![3, 7]
    );
    assert_eq!(
        runner.process_context.cfm_mut().library_fragments[0].bytes,
        vec![1, 2, 3]
    );
    let registry = runner.process_context.cfm_mut().clone();
    let native = runner.native.application_mut().unwrap();
    assert!(native.cfm.is_none());
    let before_cpu = native.cpu.clone();
    let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        native.run_with_hle_imports(64)
    }));
    assert!(refused.is_err());
    assert_eq!(native.cpu.gpr, before_cpu.gpr);
    assert_eq!(native.cpu.pc, before_cpu.pc);
    assert_eq!(*runner.process_context.cfm_mut(), registry);
    // The caller's launch blueprint remains an independent, unchanged seed.
    assert_eq!(
        app.ppc
            .as_ref()
            .unwrap()
            .cfm
            .as_ref()
            .unwrap()
            .next_connection_id,
        7
    );
    runner.init_app(&app);
    assert_eq!(runner.process_context.cfm_mut().next_connection_id, 7);
    assert_eq!(runner.process_context.cfm_mut().connections.len(), 1);
}

#[test]
fn runner_cfm_is_used_by_timer_vbl_and_sound_callback_entries() {
    use crate::callback_manager::{CallbackTaskArchitecture, ProcessTimerTask, ProcessVblTask};
    const OUTPUT: u32 = PPC_HEAP_BASE + 0x200;
    for callback_kind in 0..4 {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut native = load_pef_application(
            &crate::loader::ppc::tests::synthetic_pef_with_import(b"CloseConnection"),
        )
        .unwrap();
        native.memory.add_region(OUTPUT, vec![0; 64]);
        native.memory.write_u32_be(OUTPUT, 3).unwrap();
        let callback = native.imports[0].address;
        native.cfm.as_mut().unwrap().connections =
            vec![cfm_test_connection(3), cfm_test_connection(9)];
        native.cfm.as_mut().unwrap().next_connection_id = 10;
        app.ppc = Some(native);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let native = runner.native.application_mut().unwrap();
        let saved_cpu = native.cpu.clone();
        runner
            .process_context
            .with_memory_and_cfm(|memory_manager, cfm| match callback_kind {
                0 => {
                    native.timer_tasks.push(ProcessTimerTask {
                        task_ptr: OUTPUT,
                        architecture: CallbackTaskArchitecture::PowerPc,
                        extended: false,
                        callback,
                        active: true,
                        fire_at_tick: 1,
                        fire_at_subtick: 0,
                        last_fired_tick: None,
                    });
                    let probes = native.fire_timer_tasks_for_ticks_with_process_services(
                        0,
                        1,
                        1,
                        64,
                        false,
                        false,
                        memory_manager,
                        cfm,
                    );
                    assert_eq!(probes.len(), 1);
                    assert_eq!(probes[0].invocation.unsupported_import_index, None);
                }
                1 => {
                    native.memory.write_u32_be(OUTPUT + 6, callback).unwrap();
                    native.memory.write_u16_be(OUTPUT + 10, 1).unwrap();
                    native.vbl_tasks.push(ProcessVblTask {
                        task_ptr: OUTPUT,
                        architecture: CallbackTaskArchitecture::PowerPc,
                        slot: None,
                        pending: false,
                    });
                    let probes = native.fire_vbl_tasks_for_ticks_with_process_services(
                        0,
                        1,
                        1,
                        64,
                        false,
                        false,
                        memory_manager,
                        cfm,
                    );
                    assert_eq!(probes.len(), 1);
                    assert_eq!(probes[0].invocation.unsupported_import_index, None);
                }
                2 => {
                    let probe = native.run_sound_completion_callback_with_process_services(
                        PpcSoundCompletionRecord {
                            file_playback_index: 0,
                            channel: OUTPUT,
                            completion: callback,
                            command: None,
                            tick: 0,
                            instruction_count: 0,
                            scheduled_tick: 0,
                            scheduled_instruction_count: 0,
                        },
                        64,
                        false,
                        false,
                        memory_manager,
                        cfm,
                    );
                    assert_eq!(probe.invocation.unsupported_import_index, None);
                }
                _ => {
                    let probe = native.run_sound_doubleback_callback_with_process_services(
                        PpcSoundDoubleBackRecord {
                            architecture: CallbackTaskArchitecture::PowerPc,
                            channel: OUTPUT,
                            header: 0,
                            exhausted_buffer: 0,
                            exhausted_buffer_index: 0,
                            callback,
                            tick: 0,
                            instruction_count: 0,
                        },
                        64,
                        false,
                        false,
                        memory_manager,
                        cfm,
                    );
                    assert_eq!(probe.invocation.unsupported_import_index, None);
                }
            });
        assert!(native.cfm.is_none());
        assert_eq!(native.cpu.gpr, saved_cpu.gpr);
        assert_eq!(native.cpu.pc, saved_cpu.pc);
        assert_eq!(
            runner.bus.read_long(OUTPUT),
            0,
            "callback kind {callback_kind}"
        );
        assert_eq!(
            runner
                .process_context
                .cfm_mut()
                .connections
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            vec![9]
        );
        assert_eq!(runner.process_context.cfm_mut().next_connection_id, 10);
    }
}
