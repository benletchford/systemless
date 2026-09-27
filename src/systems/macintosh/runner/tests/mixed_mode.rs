use super::*;
use crate::loader::{Code0Header, LoadedApp};
use crate::systems::macintosh::runner::NativeEngineRole;

#[test]
fn universal_proc_preserves_native_isa_for_protected_transition_vectors() {
    use crate::loader::ppc::tests::synthetic_pef_with_import;

    const VECTOR: u32 = 0x0180_0000;
    const ENTRY: u32 = VECTOR + 8;
    const RESULT: u32 = 0x1234_5678;
    for installed in [false, true] {
        for protected in [false, true] {
            let mut native =
                load_pef_application(&synthetic_pef_with_import(b"CallUniversalProc")).unwrap();
            let words = [ENTRY, PPC_DATA_BASE, 0x3c60_1234, 0x6063_5678, 0x4e80_0020];
            let bytes = words.into_iter().flat_map(u32::to_be_bytes).collect();
            if protected {
                native
                    .memory
                    .publish_system_code(GuestIsa::PowerPc, VECTOR, bytes)
                    .unwrap();
            } else {
                native.memory.add_readonly_region(VECTOR, bytes);
            }
            native.cpu.gpr[3] = VECTOR;
            native.cpu.gpr[4] = 0x30; // Pascal, no arguments, long result.
            if installed {
                let mut runner =
                    FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
                runner.init_app(&LoadedApp::from_ppc(native));
                let (_, running) = runner.run_steps(128, None);
                assert!(!running, "installed protected={protected}");
                assert_eq!(runner.native.application_mut().unwrap().cpu.gpr[3], RESULT);
            } else {
                let probe = native.run_with_hle_imports(128);
                assert_eq!(probe.unsupported_import_index, None);
                assert_eq!(native.cpu.pc, native.halt_pc);
                assert_eq!(
                    native.cpu.gpr[3], RESULT,
                    "standalone protected={protected}"
                );
            }
        }
    }
}

#[test]
fn native_tick_count_import_observes_live_process_trap_patch() {
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_POWERPC_ISA,
    };
    use crate::loader::ppc::tests::synthetic_pef_with_import;
    use crate::trap::manager::{TrapManager, TrapTableKind};

    const DESCRIPTOR: u32 = 0x0180_0000;
    const TVECTOR: u32 = DESCRIPTOR + 0x80;
    const ENTRY: u32 = DESCRIPTOR + 0x100;
    const RTOC: u32 = DESCRIPTOR + 0x200;
    const RESULT: u32 = 0x1234_5678;

    let native = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&LoadedApp::from_ppc(native));
    let native = runner.native.application_mut().expect("native application");
    native.memory.add_region(DESCRIPTOR, vec![0; 0x300]);
    native
        .memory
        .write_u16_be(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
        .unwrap();
    native
        .memory
        .write_u8(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    native.memory.write_u16_be(DESCRIPTOR + 10, 0).unwrap();
    let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    native.memory.write_u32_be(record, 0x30).unwrap();
    native
        .memory
        .write_u8(record + 5, ROUTINE_RECORD_POWERPC_ISA)
        .unwrap();
    native
        .memory
        .write_u16_be(record + 6, ROUTINE_FLAG_USE_NATIVE_ISA)
        .unwrap();
    native.memory.write_u32_be(record + 8, TVECTOR).unwrap();
    native.memory.write_u32_be(TVECTOR, ENTRY).unwrap();
    native.memory.write_u32_be(TVECTOR + 4, RTOC).unwrap();
    for (offset, word) in [0x3c60_1234, 0x6063_5678, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        native
            .memory
            .write_u32_be(ENTRY + u32::try_from(offset).unwrap() * 4, word)
            .unwrap();
    }
    let table_entry = TrapManager::table_address(0xA975, TrapTableKind::Toolbox);
    runner.bus.write_long(table_entry, DESCRIPTOR);
    let (_, running) = runner.run_steps(128, None);

    assert!(!running);
    let native = runner
        .native
        .application()
        .expect("native application retained");
    assert_eq!(native.cpu.gpr[3], RESULT);
    assert!(native.guest_calls().is_empty());
}

#[test]
fn native_tick_count_import_completes_live_classic_trap_patch() {
    use crate::loader::ppc::tests::synthetic_pef_with_import;
    use crate::trap::manager::{TrapManager, TrapTableKind};

    const RESULT: u32 = 0x1234_5678;
    let native = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&LoadedApp::from_ppc(native));
    let handler = runner.bus.alloc(12);
    for (offset, word) in [
        0x2f7c, // MOVE.L #RESULT,4(SP), the Pascal result slot
        (RESULT >> 16) as u16,
        RESULT as u16,
        0x0004,
        0x4e75,
    ]
    .into_iter()
    .enumerate()
    {
        runner
            .bus
            .write_word(handler + u32::try_from(offset).unwrap() * 2, word);
    }
    let table_entry = TrapManager::table_address(0xA975, TrapTableKind::Toolbox);
    runner.bus.write_long(table_entry, handler);

    let (_, running) = runner.run_steps(256, None);

    assert!(!running);
    let native = runner
        .native
        .application()
        .expect("native application retained");
    assert_eq!(native.cpu.gpr[3], RESULT);
    assert!(native.guest_calls().is_empty());
}

#[test]
fn native_system_code_survives_large_process_ram_attachment() {
    use crate::loader::ppc::tests::synthetic_pef_with_import;

    let mut native = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let pools = [
        PPC_IMPORT_TVECTOR_BASE,
        PPC_IMPORT_TRAP_BASE,
        PPC_CFM_MAIN_STUB_BASE,
    ];
    let words = pools.map(|base| native.memory.read_u32_be(base).unwrap());
    // Call the imported transition vector, as compiled PEF glue does.
    // Inside Macintosh: PowerPC System Software (1994), pp. 1-27--1-28.
    let code = [
        0x3d80_0000 | (PPC_IMPORT_TVECTOR_BASE >> 16), // lis r12, vector@h
        0x618c_0000 | (PPC_IMPORT_TVECTOR_BASE & 0xffff), // ori r12,r12,vector@l
        0x800c_0000,                                   // lwz r0,0(r12)
        0x804c_0004,                                   // lwz r2,4(r12)
        0x7c09_03a6,                                   // mtctr r0
        0x4e80_0420,                                   // bctr
    ];
    const ENTRY: u32 = 0x0180_0000;
    native
        .memory
        .add_readonly_region(ENTRY, code.into_iter().flat_map(u32::to_be_bytes).collect());
    native.entry_pc = ENTRY;
    native.cpu.pc = ENTRY;
    native.cpu.lr = native.halt_pc;
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    let (reservation_base, _) = runner.bus.synthetic_reservation_range().unwrap();
    runner.set_launch_state(17, 1, 0);
    runner.init_app(&LoadedApp::from_ppc(native));
    let native = runner.native.application_mut().unwrap();
    for (base, word) in pools.into_iter().zip(words) {
        assert_eq!(native.memory.read_u32_be(base), Some(word));
        assert!(native
            .memory
            .shared_view()
            .is_shared_readonly_range(base, 1));
        assert_eq!(native.memory.write_u32_be(base, 0), None);
    }
    assert!(native
        .memory
        .shared_view()
        .is_shared_readonly_range(reservation_base, 1));
    assert_eq!(native.memory.write_u8(reservation_base, 0xff), None);
    let (steps, running) = runner.run_steps(64, None);
    assert!(steps >= 6);
    assert!(!running);
    let native = runner.native.application_mut().unwrap();
    assert_eq!(native.cpu.pc, native.halt_pc);
    assert_eq!(native.cpu.gpr[3], 17);
}

#[test]
fn parked_native_call_executes_68k_code_and_nested_traps_in_shared_memory() {
    const M68K_ENTRY: u32 = 0x0301_0000;
    const RESULT: u32 = 0x0302_0000;
    const STACK_BASE: u32 = 0x0303_0000;
    const INITIAL_SP: u32 = STACK_BASE + 0x80;
    const RETURN_PC: u32 = 0x0304_0000;

    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_launch_state(41, 1, 0);
    runner.init_app(&app);

    let ppc_app = runner.native.application_mut().expect("PPC app");
    ppc_app.memory.add_region(
        M68K_ENTRY,
        vec![
            0x59, 0x8f, // SUBQ.L #4,SP: TickCount result slot
            0xa9, 0x75, // TickCount
            0x20, 0x1f, // MOVE.L (SP)+,D0
            0x23, 0xc0, 0x03, 0x02, 0x00, 0x00, // MOVE.L D0,RESULT
            0x4e, 0x75, // RTS
        ],
    );
    ppc_app.memory.add_region(RESULT, vec![0; 4]);
    ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(crate::guest_call::M68kResultSource::Data(0)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));
    let (first_steps, first_running) = runner.run_steps(2, None);
    assert_eq!(first_steps, 2);
    assert!(first_running);
    assert_eq!(runner.bus.read_long(RESULT), 0);
    assert!(!runner.dispatcher.guest_calls.is_empty());

    let (steps, running) = runner.run_steps(64, None);

    assert_eq!(steps, 64);
    assert!(running);
    assert!(!runner.is_halted());
    assert!(runner.dispatcher.guest_calls.is_empty());
    let ppc_app = runner.native.application_mut().expect("PPC app retained");
    assert_eq!(ppc_app.memory.read_u32_be(RESULT), Some(41));
    assert_eq!(ppc_app.cpu.gpr[3], 41);
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
    assert_eq!(ppc_app.cpu.lr, PPC_CODE_BASE);
    assert_eq!(runner.bus.read_long(RESULT), 41);
}

#[test]
fn parked_powerpc_to_68k_call_obeys_guest_vector_10() {
    const M68K_ENTRY: u32 = 0x0301_1000;
    const HANDLER: u32 = 0x0301_1100;
    const STACK_BASE: u32 = 0x0303_1000;
    const INITIAL_SP: u32 = STACK_BASE + 0x80;
    const RETURN_PC: u32 = 0x0304_1000;
    const MARKER: u32 = 0xA10E_6040;

    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    let ppc_app = runner.native.application_mut().expect("PPC app");
    ppc_app.memory.add_region(
        M68K_ENTRY,
        vec![
            0xA9, 0x75, // TickCount: must enter the replacement vector
            0x4E, 0x75, // RTS
        ],
    );
    ppc_app.memory.add_region(
        HANDLER,
        vec![
            0x2C, 0x3C, 0xA1, 0x0E, 0x60, 0x40, // MOVE.L #MARKER,D6
            0x54, 0xAF, 0x00, 0x02, // ADDQ.L #2,2(SP)
            0x4E, 0x73, // RTE
        ],
    );
    ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(crate::guest_call::M68kResultSource::Data(6)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

    // Interapplication Communication (1993), p. 1-87: an A-line causes
    // the processor to fetch vector 10 from `$28` and jump to it. The
    // parked 68k adapter must preserve that rule while PPC owns the app.
    runner.bus.write_long(0x28, HANDLER);
    let (_steps, running) = runner.run_steps(64, None);

    assert!(running);
    assert!(runner.dispatcher.guest_calls.is_empty());
    let ppc_app = runner.native.application().expect("PPC app retained");
    assert_eq!(ppc_app.cpu.gpr[3], MARKER);
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
}

#[test]
fn parked_native_special_case_executes_68k_and_writes_native_outputs() {
    use crate::guest_call::{M68kResultSource, PowerPcArguments};
    use crate::mixed_mode::special_case;

    const M68K_ENTRY: u32 = 0x0305_0000;
    const OUTPUTS: u32 = 0x0305_1000;
    const STACK_BASE: u32 = 0x0305_2000;
    const INITIAL_SP: u32 = STACK_BASE + 0x80;
    const RETURN_PC: u32 = 0x0305_3000;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app.memory.add_region(
        M68K_ENTRY,
        [
            0x203c, 0x0001, 0x1111, // MOVE.L #$00011111,D0
            0x323c, 0x2222, // MOVE.W #$2222,D1
            0x343c, 0x0033, // MOVE.W #$0033,D2
            0x4e75, // RTS
        ]
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .collect(),
    );
    ppc_app.memory.add_region(OUTPUTS, vec![0; 8]);
    ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    let arguments =
        PowerPcArguments::from_slice(&[0, 0, 0, 0, 0, 0, OUTPUTS, OUTPUTS + 2, OUTPUTS + 4])
            .unwrap();
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(M68kResultSource::SpecialCase {
                selector: u8::try_from(special_case::HIT_TEST_HOOK).unwrap(),
                arguments,
                stack_result: None,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Mask(0xff),
        ));

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let (steps, running) = runner.run_steps(64, None);

    assert!(steps > 0);
    assert!(running);
    assert!(!runner.is_halted());
    assert!(runner.dispatcher.guest_calls.is_empty());
    let ppc_app = runner.native.application_mut().expect("PPC app retained");
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
    assert_eq!(ppc_app.cpu.gpr[3], 1);
    assert_eq!(ppc_app.memory.read_u16_be(OUTPUTS), Some(0x1111));
    assert_eq!(ppc_app.memory.read_u16_be(OUTPUTS + 2), Some(0x2222));
    assert_eq!(ppc_app.memory.read_u8(OUTPUTS + 4), Some(0x33));
}

#[test]
fn standalone_classic_process_enters_powerpc_routine_descriptor_and_resumes_once() {
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_POWERPC_ISA,
        ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
    };
    use crate::mixed_mode::proc_info;

    const DESCRIPTOR: u32 = 0x0030_0000;
    const TVECTOR: u32 = 0x0030_0100;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    const CALLBACK_RTOC: u32 = 0x0030_0200;
    const M68K_RETURN: u32 = 0x0030_0300;
    const M68K_STACK: u32 = 0x0070_0000;
    const ARGUMENT: u32 = 0x1234_0000;

    let mut native = halted_ppc_app_with_sound(PpcSoundState::default())
        .ppc
        .take()
        .expect("native execution adapter");
    native
        .memory
        .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
    native.memory.add_region(
        CALLBACK,
        [
            0x3863_0007u32, // addi r3,r3,7
            0x4e80_0020,    // blr
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
    runner.stage_ppc_companion(native);
    runner.init_app(&classic);
    assert!(!runner.is_powerpc_app());

    let proc_info = proc_info::PASCAL_STACK_BASED
        | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
        | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE);
    runner
        .bus
        .write_word(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
    runner
        .bus
        .write_byte(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION);
    runner.bus.write_word(DESCRIPTOR + 10, 0);
    let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    runner.bus.write_long(record, proc_info);
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
        .write_long(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR);
    runner.bus.write_long(TVECTOR, CALLBACK);
    runner.bus.write_long(TVECTOR + 4, CALLBACK_RTOC);
    runner.bus.write_long(M68K_STACK, M68K_RETURN);
    runner.bus.write_long(M68K_STACK + 4, ARGUMENT);
    runner.bus.write_word(M68K_RETURN, 0x201f); // MOVE.L (SP)+,D0
    runner.bus.write_word(M68K_RETURN + 2, 0x4e71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, DESCRIPTOR);
    runner.m68k.cpu.write_reg(Register::A7, M68K_STACK);

    let (classic_steps, classic_running) = runner.run_steps(2, None);
    assert!(classic_steps > 0);
    assert!(classic_running);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), DESCRIPTOR + 2);
    assert!(runner.dispatcher.guest_calls.has_powerpc_from_m68k());

    let (native_steps, native_running) = runner.run_steps(64, None);
    assert!(native_steps > 0);
    assert!(native_running);
    assert!(!runner.is_halted());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), M68K_RETURN);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), M68K_STACK + 8);
    assert_eq!(runner.bus.read_long(M68K_STACK + 8), ARGUMENT + 7);

    let (resumed_steps, resumed_running) = runner.run_steps(1, None);
    assert_eq!(resumed_steps, 1);
    assert!(resumed_running);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), ARGUMENT + 7);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), M68K_STACK + 12);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), M68K_RETURN + 2);
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn raw_os_trap_patch_can_execute_a_native_routine_descriptor() {
    use crate::guest_call::{GuestCallTarget, M68kRegisterState, M68kResultSource};
    use crate::guest_procedure::{
        GuestIsa, ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA, ROUTINE_RECORD_POWERPC_ISA,
        ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, ROUTINE_RECORD_SIZE,
    };
    use crate::mixed_mode::proc_info;

    const TRAP: u16 = 0xA11E; // NewPtr
    const BYTE_COUNT: u32 = 0x1234;
    const DESCRIPTOR: u32 = 0x0301_0000;
    const TVECTOR: u32 = 0x0301_0100;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    const M68K_FALLBACK: u32 = 0x0301_0200;
    const M68K_ENTRY: u32 = 0x0301_0300;
    const M68K_STACK: u32 = 0x0302_0000;
    const INITIAL_SP: u32 = M68K_STACK + 0x80;
    const RETURN_PC: u32 = 0x0302_0100;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app
        .memory
        .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
    ppc_app
        .memory
        .add_region(CALLBACK, 0x4e80_0020u32.to_be_bytes().to_vec()); // blr
    ppc_app.memory.add_region(DESCRIPTOR, vec![0; 0x400]);
    ppc_app.memory.add_region(M68K_STACK, vec![0; 0x200]);
    ppc_app
        .memory
        .write_u16_be(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
        .unwrap();
    ppc_app
        .memory
        .write_u8(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    ppc_app.memory.write_u16_be(DESCRIPTOR + 10, 1).unwrap();

    // NewPtr's documented register-based ProcInfo passes the actual trap
    // word from D1 first, then the allocation size from D0, and returns
    // the pointer in A0. Inside Macintosh: PowerPC System Software (1994),
    // pp. 1-67--1-68, Listing 1-14.
    let proc_info = proc_info::REGISTER_BASED
        | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
        | (4 << proc_info::REGISTER_RESULT_LOCATION_PHASE)
        | (6 << proc_info::REGISTER_PARAMETER_PHASE)
        | (3 << (proc_info::REGISTER_PARAMETER_PHASE + proc_info::REGISTER_PARAMETER_WIDTH));
    let m68k_record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    ppc_app.memory.write_u32_be(m68k_record, proc_info).unwrap();
    ppc_app
        .memory
        .write_u8(
            m68k_record + ROUTINE_RECORD_ISA_OFFSET,
            ROUTINE_RECORD_M68K_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u32_be(
            m68k_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            M68K_FALLBACK,
        )
        .unwrap();
    let native_record = m68k_record + ROUTINE_RECORD_SIZE;
    ppc_app
        .memory
        .write_u32_be(native_record, proc_info)
        .unwrap();
    ppc_app
        .memory
        .write_u8(
            native_record + ROUTINE_RECORD_ISA_OFFSET,
            ROUTINE_RECORD_POWERPC_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u16_be(
            native_record + ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_FLAG_USE_NATIVE_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u32_be(
            native_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            TVECTOR,
        )
        .unwrap();
    ppc_app.memory.write_u32_be(TVECTOR, CALLBACK).unwrap();
    ppc_app.memory.write_u32_be(TVECTOR + 4, 0).unwrap();
    ppc_app.memory.write_u16_be(M68K_FALLBACK, 0x4e75).unwrap();
    ppc_app.memory.write_u16_be(M68K_ENTRY, TRAP).unwrap();
    ppc_app.memory.write_u16_be(M68K_ENTRY + 2, 0x4e75).unwrap();
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    let mut registers = M68kRegisterState::default();
    registers.data[0] = BYTE_COUNT;
    registers.data[1] = 0xdead_beef;
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            registers,
            Some(M68kResultSource::Address(0)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner
        .dispatcher
        .install_trap_address(&mut runner.bus, TRAP, DESCRIPTOR)
        .expect("native descriptor patch must install");

    let (m68k_steps, m68k_running) = runner.run_steps(2, None);
    assert!(m68k_steps > 0);
    assert!(m68k_running);
    let pending = runner
        .dispatcher
        .guest_calls
        .pending_powerpc_from_m68k()
        .expect("native record should be pending");
    assert_eq!(pending.arguments.as_slice(), &[u32::from(TRAP), BYTE_COUNT]);

    let (steps, running) = runner.run_steps(64, None);

    assert!(steps > 0);
    assert!(running);
    assert!(!runner.is_halted());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert!(runner.dispatcher.pending_native_trap_calls.is_empty());
    let ppc_app = runner.native.application().expect("PPC app retained");
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
    assert_eq!(ppc_app.cpu.gpr[3], u32::from(TRAP));
}

#[test]
fn nested_cross_isa_calls_restore_each_68k_cpu_context() {
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA, ROUTINE_RECORD_POWERPC_ISA,
        ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
    };
    use crate::mixed_mode::proc_info;

    const DESCRIPTOR: u32 = 0x0301_0100;
    const TVECTOR: u32 = 0x0301_0200;
    const PPC_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    const CALLBACK_RTOC: u32 = 0x0301_0300;
    const M68K_INNER: u32 = 0x0301_0400;
    const INNER_DESCRIPTOR: u32 = 0x0301_0600;
    const M68K_STACK: u32 = 0x0302_0000;
    const INITIAL_SP: u32 = M68K_STACK + 0x80;
    const RETURN_PC: u32 = 0x0302_0100;
    const ARGUMENT: u32 = 0x10;
    const OUTER_D6: u32 = 0x1357_2468;
    const INNER_D6: u32 = 0xdead_beef;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app
        .memory
        .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);

    let inner_words = [
        0x2c3c,
        (INNER_D6 >> 16) as u16,
        INNER_D6 as u16, // MOVE.L #INNER_D6,D6
        0x202f,
        0x0004, // MOVE.L 4(SP),D0
        0x5e80, // ADDQ.L #7,D0
        0x2f40,
        0x0008, // MOVE.L D0,8(SP)
        0x4e74,
        0x0004, // RTD #4
    ];
    ppc_app.memory.add_region(
        M68K_INNER,
        inner_words.into_iter().flat_map(u16::to_be_bytes).collect(),
    );

    let proc_info = proc_info::PASCAL_STACK_BASED
        | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
        | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE);
    let first_branch_pc = PPC_CALLBACK + 6 * 4;
    let second_branch_pc = PPC_CALLBACK + 12 * 4;
    let callback_words = [
        0x7fe8_02a6, // MFLR R31
        0x3c60_0000 | (INNER_DESCRIPTOR >> 16),
        0x6063_0000 | (INNER_DESCRIPTOR & 0xffff),
        0x3c80_0000 | (proc_info >> 16),
        0x6084_0000 | (proc_info & 0xffff),
        0x38a0_0000 | ARGUMENT, // LI R5,ARGUMENT
        ppc_test_relative_branch(first_branch_pc, PPC_IMPORT_TRAP_BASE) | 1, // BL CallUniversalProc
        0x3c60_0000 | (INNER_DESCRIPTOR >> 16),
        0x6063_0000 | (INNER_DESCRIPTOR & 0xffff),
        0x3c80_0000 | (proc_info >> 16),
        0x6084_0000 | (proc_info & 0xffff),
        0x38a0_0000 | ARGUMENT, // LI R5,ARGUMENT
        ppc_test_relative_branch(second_branch_pc, PPC_IMPORT_TRAP_BASE) | 1, // BL CallUniversalProc
        0x7fe8_03a6,                                                          // MTLR R31
        0x3863_0007,                                                          // ADDI R3,R3,7
        0x4e80_0020,                                                          // BLR
    ];
    ppc_app.memory.add_region(
        PPC_CALLBACK,
        callback_words
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    ppc_app.memory.add_region(DESCRIPTOR, vec![0; 0x200]);
    ppc_app
        .memory
        .write_u16_be(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
        .unwrap();
    ppc_app
        .memory
        .write_u8(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    ppc_app.memory.write_u16_be(DESCRIPTOR + 10, 0).unwrap();
    let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    ppc_app.memory.write_u32_be(record, proc_info).unwrap();
    ppc_app
        .memory
        .write_u8(
            record + ROUTINE_RECORD_ISA_OFFSET,
            ROUTINE_RECORD_POWERPC_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u16_be(
            record + ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_FLAG_USE_NATIVE_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u32_be(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR)
        .unwrap();
    ppc_app.memory.write_u32_be(TVECTOR, PPC_CALLBACK).unwrap();
    ppc_app
        .memory
        .write_u32_be(TVECTOR + 4, CALLBACK_RTOC)
        .unwrap();
    ppc_app.memory.add_region(INNER_DESCRIPTOR, vec![0; 0x100]);
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
        .write_u32_be(inner_record, proc_info)
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
            M68K_INNER,
        )
        .unwrap();
    ppc_app.memory.add_region(M68K_STACK, vec![0; 0x200]);
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    ppc_app
        .memory
        .write_u32_be(INITIAL_SP + 4, ARGUMENT)
        .unwrap();

    let mut call_universal_proc = test_ppc_import_binding(0, "InterfaceLib", "CallUniversalProc");
    call_universal_proc.trap_pc = PPC_IMPORT_TRAP_BASE;
    call_universal_proc.dispatcher_target = PpcImportDispatcherTarget::CallUniversalProc;
    ppc_app.import_count = 1;
    ppc_app.imports = vec![call_universal_proc];
    let mut outer_registers = crate::guest_call::M68kRegisterState::default();
    outer_registers.data[6] = OUTER_D6;
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: DESCRIPTOR,
                rtoc: 0,
            },
            DESCRIPTOR,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 8,
            outer_registers,
            Some(crate::guest_call::M68kResultSource::Memory {
                address: INITIAL_SP + 8,
                size: 4,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut maximum_parked = 0;
    let mut inner_entries = 0;
    let mut checked_wrong_boundary = false;
    for iteration in 0..128 {
        let (steps, running) = runner.run_steps(1, None);
        assert!(
                running,
                "cross-ISA execution stopped at iteration {iteration} after {steps} steps: pc=${:08x} sp=${:08x} frames={} parked={} ppc_pc=${:08x}",
                runner.m68k.cpu.read_reg(Register::PC),
                runner.m68k.cpu.read_reg(Register::A7),
                runner.dispatcher.guest_calls.len(),
                runner.dispatcher.guest_calls.m68k_context_bank().borrow().len(),
                runner.native.application()
                    .map_or(0, |ppc_app| ppc_app.cpu.pc),
            );
        maximum_parked = maximum_parked.max(
            runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .len(),
        );
        if !runner
            .dispatcher
            .guest_calls
            .m68k_context_bank()
            .borrow()
            .is_empty()
            && runner.m68k.cpu.read_reg(Register::PC) == M68K_INNER + 6
        {
            inner_entries += 1;
        }
        if !checked_wrong_boundary
            && !runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .is_empty()
        {
            let frame_count = runner.dispatcher.guest_calls.len();
            let mut native_context = runner
                .native
                .take(NativeEngineRole::Application)
                .expect("PPC app");
            let mut ppc_app = native_context.adapter_mut();
            assert!(!runner.resume_m68k_after_powerpc(&mut ppc_app));
            assert_eq!(
                runner
                    .dispatcher
                    .guest_calls
                    .m68k_context_bank()
                    .borrow()
                    .len(),
                1
            );
            assert_eq!(ppc_app.toolbox_startup.execution.calls().len(), frame_count);
            runner
                .native
                .restore(native_context)
                .unwrap_or_else(|_| panic!("native context lost its owner"));
            checked_wrong_boundary = true;
        }
        if runner.dispatcher.guest_calls.is_empty() {
            break;
        }
    }

    assert!(checked_wrong_boundary);
    assert_eq!(inner_entries, 2);
    assert_eq!(maximum_parked, 1);
    assert!(runner
        .dispatcher
        .guest_calls
        .m68k_context_bank()
        .borrow()
        .is_empty());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(runner.m68k.cpu.core.d(6), OUTER_D6);
    let ppc_app = runner.native.application_mut().expect("PPC app retained");
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
    assert_eq!(ppc_app.cpu.gpr[3], ARGUMENT + 14);
    assert_eq!(
        ppc_app.memory.read_u32_be(INITIAL_SP + 8),
        Some(ARGUMENT + 14)
    );
}

#[test]
fn reverse_powerpc_return_sets_only_the_selected_68k_ccr_bit() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.m68k.cpu.core.set_ccr(0x10);
    assert!(runner.dispatcher.guest_calls.begin_m68k_to_powerpc(
        crate::guest_call::GuestCallTarget {
            isa: crate::guest_procedure::GuestIsa::PowerPc,
            entry: PPC_CODE_BASE,
            rtoc: 0,
        },
        crate::guest_call::PowerPcArguments::from_slice(&[]).unwrap(),
        0x0010_0000,
        0x0010_1000,
        Some(crate::guest_call::M68kResultTarget::Ccr { mask: 0x04 }),
    ));
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app");
    let mut ppc_app = native_context.adapter_mut();
    let return_pc = 0x01f0_4000;
    ppc_app
        .toolbox_startup
        .execution
        .calls()
        .activate_powerpc_from_m68k(&mut ppc_app.cpu, return_pc)
        .unwrap();
    ppc_app.cpu.pc = return_pc;
    ppc_app.cpu.gpr[3] = 1;
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .complete_powerpc_for_m68k(&mut ppc_app.cpu));

    assert!(runner.resume_m68k_after_powerpc(&mut ppc_app));

    assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x14);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0x0010_0000);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x0010_1000);
    assert!(ppc_app.toolbox_startup.execution.calls().is_empty());
    runner
        .native
        .restore(native_context)
        .unwrap_or_else(|_| panic!("native context lost its owner"));
}

#[test]
fn reverse_special_case_results_restore_every_classic_output_layout() {
    use crate::mixed_mode::special_case;

    const SCRATCH: u32 = 0x0305_0000;
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app");
    let ppc_app = native_context.adapter_mut();
    ppc_app.memory.add_region(SCRATCH, vec![0; 8]);

    runner.m68k.cpu.core.set_ccr(0x13);
    for selector in [
        special_case::EOL_HOOK,
        special_case::PROTOCOL_HANDLER,
        special_case::SOCKET_LISTENER,
    ] {
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(selector).unwrap(),
            SCRATCH,
            1,
        ));
        assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x17);
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(selector).unwrap(),
            SCRATCH,
            0,
        ));
        assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x13);
    }

    runner.m68k.cpu.core.set_d(1, 0xaaaa_0000);
    for selector in [special_case::WIDTH_HOOK, special_case::NWIDTH_HOOK] {
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(selector).unwrap(),
            SCRATCH,
            0x1234_5678,
        ));
        assert_eq!(runner.m68k.cpu.core.d(1), 0xaaaa_5678);
    }

    runner.m68k.cpu.core.set_d(1, 0xbbbb_0000);
    runner.m68k.cpu.core.set_d(2, 0xcccc_0000);
    ppc_app.memory.write_u16_be(SCRATCH, 0x1111).unwrap();
    ppc_app.memory.write_u16_be(SCRATCH + 2, 0x2222).unwrap();
    ppc_app.memory.write_u8(SCRATCH + 4, 1).unwrap();
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::HIT_TEST_HOOK).unwrap(),
        SCRATCH,
        1,
    ));
    assert_eq!(runner.m68k.cpu.core.d(0), 0x0001_1111);
    assert_eq!(runner.m68k.cpu.core.d(1), 0xbbbb_2222);
    assert_eq!(runner.m68k.cpu.core.d(2), 0xcccc_0001);

    runner.m68k.cpu.core.set_d(0, 0xaaaa_0000);
    runner.m68k.cpu.core.set_d(1, 0xbbbb_0000);
    ppc_app.memory.write_u16_be(SCRATCH, 0x3333).unwrap();
    ppc_app.memory.write_u16_be(SCRATCH + 2, 0x4444).unwrap();
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::TE_FIND_WORD).unwrap(),
        SCRATCH,
        0,
    ));
    assert_eq!(runner.m68k.cpu.core.d(0), 0xaaaa_3333);
    assert_eq!(runner.m68k.cpu.core.d(1), 0xbbbb_4444);

    for (offset, value) in [(0, 0x5555), (2, 0x6666), (4, 0x7777)] {
        ppc_app
            .memory
            .write_u16_be(SCRATCH + offset, value)
            .unwrap();
    }
    runner.m68k.cpu.core.set_d(2, 0xaaaa_0000);
    runner.m68k.cpu.core.set_d(3, 0xbbbb_0000);
    runner.m68k.cpu.core.set_d(4, 0xcccc_0000);
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::TE_RECALC).unwrap(),
        SCRATCH,
        0,
    ));
    assert_eq!(runner.m68k.cpu.core.d(2), 0xaaaa_5555);
    assert_eq!(runner.m68k.cpu.core.d(3), 0xbbbb_6666);
    assert_eq!(runner.m68k.cpu.core.d(4), 0xcccc_7777);

    ppc_app.memory.write_u32_be(SCRATCH, 0xcafe_babe).unwrap();
    ppc_app.memory.write_u16_be(SCRATCH + 4, 0x8888).unwrap();
    runner.m68k.cpu.core.set_d(0, 0xdddd_0000);
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::TE_DO_TEXT).unwrap(),
        SCRATCH,
        0,
    ));
    assert_eq!(runner.m68k.cpu.core.a(0), 0xcafe_babe);
    assert_eq!(runner.m68k.cpu.core.d(0), 0xdddd_8888);

    runner.m68k.cpu.core.set_d(0, 0xeeee_0000);
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::MBAR_HOOK).unwrap(),
        SCRATCH,
        0x1234_9999,
    ));
    assert_eq!(runner.m68k.cpu.core.d(0), 0xeeee_9999);
    assert!(!M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        13,
        SCRATCH,
        0
    ));
    runner
        .native
        .restore(native_context)
        .unwrap_or_else(|_| panic!("native context lost its owner"));
}

#[test]
fn forward_special_case_results_restore_every_native_output_layout() {
    use crate::guest_call::PowerPcArguments;
    use crate::mixed_mode::special_case;

    const SCRATCH: u32 = 0x0306_0000;
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app");
    let ppc_app = native_context.adapter_mut();
    ppc_app.memory.add_region(SCRATCH, vec![0; 0x100]);
    let arguments = |values: &[u32]| PowerPcArguments::from_slice(values).unwrap();

    for selector in [special_case::HIGH_HOOK, special_case::DRAW_HOOK] {
        let values = vec![
            0;
            if selector == special_case::HIGH_HOOK {
                2
            } else {
                5
            }
        ];
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                arguments(&values),
                None,
            ),
            Ok(None),
        );
    }

    for selector in [
        special_case::EOL_HOOK,
        special_case::PROTOCOL_HANDLER,
        special_case::SOCKET_LISTENER,
    ] {
        let values = vec![
            0;
            match selector {
                special_case::EOL_HOOK => 3,
                special_case::PROTOCOL_HANDLER => 6,
                special_case::SOCKET_LISTENER => 7,
                _ => unreachable!(),
            }
        ];
        runner.m68k.cpu.core.set_ccr(0x04);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                arguments(&values),
                None,
            ),
            Ok(Some(1)),
        );
        runner.m68k.cpu.core.set_ccr(0);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                arguments(&values),
                None,
            ),
            Ok(Some(0)),
        );
    }

    runner.m68k.cpu.core.set_d(1, 0xaaaa_5678);
    for selector in [special_case::WIDTH_HOOK, special_case::NWIDTH_HOOK] {
        let values = vec![
            0;
            if selector == special_case::WIDTH_HOOK {
                5
            } else {
                8
            }
        ];
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                arguments(&values),
                None,
            ),
            Ok(Some(0x5678)),
        );
    }

    runner.m68k.cpu.core.set_d(0, 0x0001_1111);
    runner.m68k.cpu.core.set_d(1, 0xaaaa_2222);
    runner.m68k.cpu.core.set_d(2, 0xbbbb_0033);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::HIT_TEST_HOOK).unwrap(),
            arguments(&[0, 0, 0, 0, 0, 0, SCRATCH, SCRATCH + 2, SCRATCH + 4]),
            None,
        ),
        Ok(Some(1)),
    );
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH), Some(0x1111));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 2), Some(0x2222));
    assert_eq!(ppc_app.memory.read_u8(SCRATCH + 4), Some(0x33));

    runner.m68k.cpu.core.set_d(0, 0xaaaa_4444);
    runner.m68k.cpu.core.set_d(1, 0xbbbb_5555);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_FIND_WORD).unwrap(),
            arguments(&[0, 0, 0, 0, SCRATCH + 8, SCRATCH + 10]),
            None,
        ),
        Ok(None),
    );
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 8), Some(0x4444));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 10), Some(0x5555));

    runner.m68k.cpu.core.set_d(2, 0xaaaa_6666);
    runner.m68k.cpu.core.set_d(3, 0xbbbb_7777);
    runner.m68k.cpu.core.set_d(4, 0xcccc_8888);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_RECALC).unwrap(),
            arguments(&[0, 0, SCRATCH + 12, SCRATCH + 14, SCRATCH + 16]),
            None,
        ),
        Ok(None),
    );
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 12), Some(0x6666));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 14), Some(0x7777));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 16), Some(0x8888));

    runner.m68k.cpu.core.set_a(0, 0xcafe_babe);
    runner.m68k.cpu.core.set_d(0, 0xaaaa_9999);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_DO_TEXT).unwrap(),
            arguments(&[0, 0, 0, 0, SCRATCH + 20, SCRATCH + 24]),
            None,
        ),
        Ok(None),
    );
    assert_eq!(ppc_app.memory.read_u32_be(SCRATCH + 20), Some(0xcafe_babe));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 24), Some(0x9999));

    ppc_app.memory.write_u16_be(SCRATCH + 28, 1).unwrap();
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::GNE_FILTER_PROC).unwrap(),
            arguments(&[0, SCRATCH + 26]),
            Some(SCRATCH + 28),
        ),
        Ok(None),
    );
    assert_eq!(ppc_app.memory.read_u8(SCRATCH + 26), Some(1));

    // The callback writes multiple output locations as one ABI result.
    // A bad later destination must not leave an earlier output changed.
    ppc_app.memory.write_u16_be(SCRATCH + 30, 0xaaaa).unwrap();
    runner.m68k.cpu.core.set_d(0, 0x1111);
    runner.m68k.cpu.core.set_d(1, 0x2222);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_FIND_WORD).unwrap(),
            arguments(&[0, 0, 0, 0, SCRATCH + 30, 0x0500_0000]),
            None,
        ),
        Err(()),
    );
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 30), Some(0xaaaa));

    runner.m68k.cpu.core.set_d(0, 0xaaaa_abcd);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::MBAR_HOOK).unwrap(),
            arguments(&[0]),
            None,
        ),
        Ok(Some(0xabcd)),
    );
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::HIGH_HOOK).unwrap(),
            arguments(&[]),
            None,
        ),
        Err(()),
    );
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            13,
            arguments(&[]),
            None,
        ),
        Err(()),
    );
    runner
        .native
        .restore(native_context)
        .unwrap_or_else(|_| panic!("native context lost its owner"));
}

#[test]
fn void_special_case_completion_preserves_nonzero_native_r3() {
    use crate::guest_call::{M68kResultSource, PowerPcArguments};
    use crate::mixed_mode::special_case;

    const M68K_ENTRY: u32 = 0x0306_1000;
    const INITIAL_SP: u32 = 0x0306_2000;
    const RETURN_PC: u32 = 0x0306_3000;
    const FINAL_SP: u32 = INITIAL_SP + 4;
    const NATIVE_R3: u32 = 0xCAFE_BABE;

    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app");
    let ppc_app = native_context.adapter_mut();
    ppc_app.cpu.gpr[3] = NATIVE_R3;
    let arguments = PowerPcArguments::from_slice(&[0, 0]).unwrap();
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            FINAL_SP,
            crate::guest_call::M68kRegisterState::default(),
            Some(M68kResultSource::SpecialCase {
                selector: u8::try_from(special_case::HIGH_HOOK).unwrap(),
                arguments,
                stack_result: None,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));
    let pending = ppc_app
        .toolbox_startup
        .execution
        .calls()
        .activate_m68k()
        .unwrap();
    runner.m68k.cpu.write_reg(Register::PC, pending.return_pc);
    runner.m68k.cpu.write_reg(Register::A7, pending.final_sp);

    assert!(runner
        .process_context
        .with_memory_and_cfm(|manager, _| runner.m68k.complete_pending(
            &mut ppc_app.memory,
            &mut ppc_app.cpu,
            pending,
            manager
        )));
    assert_eq!(ppc_app.cpu.gpr[3], NATIVE_R3);
    assert!(ppc_app.toolbox_startup.execution.calls().is_empty());
}
