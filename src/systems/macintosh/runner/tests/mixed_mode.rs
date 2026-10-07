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

// Interrupt-level PowerPC callbacks that wait on a 68K Mixed Mode call. The
// foreground is a counting loop with every other register seeded; callbacks
// use a private frame below its 224-byte Red Zone.
const IRQ_LOOP_PC: u32 = PPC_CODE_BASE + 0x800;
const IRQ_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
const IRQ_PLAIN_CALLBACK: u32 = PPC_CODE_BASE + 0x1400;
const IRQ_TVECTOR: u32 = 0x0301_0200;
const IRQ_PLAIN_TVECTOR: u32 = 0x0301_0210;
const IRQ_CALLBACK_RTOC: u32 = 0x0301_0300;
const IRQ_M68K_ROUTINE: u32 = 0x0301_0400;
const IRQ_DESCRIPTOR: u32 = 0x0301_0600;
const IRQ_DATA: u32 = 0x0305_0000;
const IRQ_RESULT: u32 = IRQ_DATA;
const IRQ_PRIVATE: u32 = IRQ_DATA + 4;
const IRQ_COUNTER: u32 = IRQ_DATA + 8;
const IRQ_FIRST_RESULT: u32 = IRQ_DATA + 12;
const IRQ_PLAIN_MARKER: u32 = IRQ_DATA + 16;
const IRQ_TASK: u32 = IRQ_DATA + 0x100;
const IRQ_PLAIN_TASK: u32 = IRQ_DATA + 0x120;
const IRQ_TIMER_TASK: u32 = IRQ_DATA + 0x140;
const IRQ_ARGUMENT: u32 = 0x10;
const IRQ_PRIVATE_VALUE: u32 = 0xc0de_cafe;
const IRQ_TICK_CYCLES: u32 = 200;
const IRQ_RED_ZONE: u32 = 224;
const IRQ_USE_RES_FILE_TRAP: u32 = PPC_IMPORT_TRAP_BASE + 4;

fn irq_proc_info() -> u32 {
    use crate::mixed_mode::proc_info;
    proc_info::PASCAL_STACK_BASED
        | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
        | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE)
}

/// A callback that saves r30, loads a private value into it, makes `calls`
/// sequential CallUniversalProc calls on the 68K routine (each passing the
/// previous result), records the results, bumps a counter, and optionally
/// re-arms a VBL task after the 68K work.
fn irq_mixed_mode_callback(calls: usize, rearm: Option<u32>) -> Vec<u32> {
    irq_mixed_mode_callback_with(calls, rearm, 0, None)
}

/// As [`irq_mixed_mode_callback`], optionally spinning `delay` iterations
/// before its first call and selecting resource file `before` ahead of the
/// 68K work and `after` once it has returned.
fn irq_mixed_mode_callback_with(
    calls: usize,
    rearm: Option<u32>,
    delay: u32,
    resource_files: Option<(u16, u16)>,
) -> Vec<u32> {
    let proc_info = irq_proc_info();
    let mut words = vec![
        0x7c08_02a6, // mflr r0
        0x9001_0008, // stw r0,8(r1)
        0x9421_ffb0, // stwu r1,-80(r1)
        0x93c1_0048, // stw r30,72(r1)
        0x3fc0_0000 | (IRQ_PRIVATE_VALUE >> 16),
        0x63de_0000 | (IRQ_PRIVATE_VALUE & 0xffff),
    ];
    if delay != 0 {
        words.extend([
            0x3d20_0000 | (delay >> 16),
            0x6129_0000 | (delay & 0xffff),
            0x7d29_03a6, // mtctr r9
            0x4200_0000, // bdnz .
        ]);
    }
    let use_res_file = |words: &mut Vec<u32>, refnum: u16| {
        words.push(0x3860_0000 | u32::from(refnum)); // li r3,refnum
        let branch_pc = IRQ_CALLBACK + u32::try_from(words.len()).unwrap() * 4;
        words.push(ppc_test_relative_branch(branch_pc, IRQ_USE_RES_FILE_TRAP) | 1);
    };
    if let Some((before, _)) = resource_files {
        use_res_file(&mut words, before);
    }
    words.push(0x38a0_0000 | IRQ_ARGUMENT); // li r5,ARGUMENT
    for call in 0..calls {
        words.extend([
            0x3c60_0000 | (IRQ_DESCRIPTOR >> 16),
            0x6063_0000 | (IRQ_DESCRIPTOR & 0xffff),
            0x3c80_0000 | (proc_info >> 16),
            0x6084_0000 | (proc_info & 0xffff),
        ]);
        let branch_pc = IRQ_CALLBACK + u32::try_from(words.len()).unwrap() * 4;
        words.push(ppc_test_relative_branch(branch_pc, PPC_IMPORT_TRAP_BASE) | 1);
        if call == 0 {
            words.extend([
                0x3cc0_0000 | (IRQ_DATA >> 16),
                0x9066_0000 | (IRQ_FIRST_RESULT & 0xffff), // stw r3,FIRST(r6)
            ]);
        }
        words.push(0x7c65_1b78); // mr r5,r3
    }
    words.extend([
        0x3cc0_0000 | (IRQ_DATA >> 16),
        0x9066_0000 | (IRQ_RESULT & 0xffff),  // stw r3,RESULT(r6)
        0x93c6_0000 | (IRQ_PRIVATE & 0xffff), // stw r30,PRIVATE(r6)
        0x80e6_0000 | (IRQ_COUNTER & 0xffff), // lwz r7,COUNTER(r6)
        0x38e7_0001,                          // addi r7,r7,1
        0x90e6_0000 | (IRQ_COUNTER & 0xffff), // stw r7,COUNTER(r6)
    ]);
    if let Some((_, after)) = resource_files {
        use_res_file(&mut words, after);
    }
    if let Some(task) = rearm {
        words.extend([
            0x3ce0_0000 | (task >> 16),
            0x60e7_0000 | (task & 0xffff),
            0x3900_0001, // li r8,1
            0xb107_000a, // sth r8,10(r7): vblCount
        ]);
    }
    words.extend([
        0x83c1_0048, // lwz r30,72(r1)
        0x3821_0050, // addi r1,r1,80
        0x8001_0008, // lwz r0,8(r1)
        0x7c08_03a6, // mtlr r0
        0x4e80_0020, // blr
    ]);
    words
}

/// Plain callback `index` lives at its own entry, transition vector and
/// marker word.
fn irq_plain_tvector(index: u32) -> u32 {
    IRQ_PLAIN_TVECTOR + index * 8
}

fn irq_plain_marker(index: u32) -> u32 {
    IRQ_PLAIN_MARKER + index * 4
}

/// A callback that never reaches 68K code: it clobbers volatile registers,
/// bumps its marker word, and re-arms the plain VBL task.
fn irq_plain_callback(index: u32) -> Vec<u32> {
    let marker = irq_plain_marker(index) & 0xffff;
    let mut words = vec![0x3800_0077]; // li r0,0x77
    for register in 3..=12u32 {
        words.push(0x3800_0000 | (register << 21) | (0x40 + register)); // li rN,0x40+N
    }
    words.extend([
        0x7d89_03a6, // mtctr r12
        0x2c03_0005, // cmpwi r3,5
        0x3cc0_0000 | (IRQ_DATA >> 16),
        0x80e6_0000 | marker, // lwz r7,MARKER(r6)
        0x38e7_0001,          // addi r7,r7,1
        0x90e6_0000 | marker, // stw r7,MARKER(r6)
        0x3ce0_0000 | (IRQ_PLAIN_TASK >> 16),
        0x60e7_0000 | (IRQ_PLAIN_TASK & 0xffff),
        0x3900_0001, // li r8,1
        0xb107_000a, // sth r8,10(r7): vblCount
        0x4e80_0020, // blr
    ]);
    words
}

fn irq_words(words: Vec<u32>) -> Vec<u8> {
    words.into_iter().flat_map(u32::to_be_bytes).collect()
}

/// A runner whose foreground PowerPC loop counts in r20 and whose 68K routine
/// (Pascal, one long argument, long result) returns its argument plus 7
/// after a delay loop that spans several runner slices.
fn irq_runner(callback: Vec<u32>) -> FixtureRunner {
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA,
        ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
    };

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app.memory.add_region(
        IRQ_LOOP_PC,
        irq_words(vec![
            0x3a94_0001, // addi r20,r20,1
            0x4bff_fffc, // b .-4
        ]),
    );
    ppc_app.memory.add_region(IRQ_CALLBACK, irq_words(callback));
    ppc_app.memory.add_region(IRQ_TVECTOR, vec![0; 0x40]);
    for index in 0..3 {
        let entry = IRQ_PLAIN_CALLBACK + index * 0x100;
        ppc_app
            .memory
            .add_region(entry, irq_words(irq_plain_callback(index)));
        ppc_app
            .memory
            .write_u32_be(irq_plain_tvector(index), entry)
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(irq_plain_tvector(index) + 4, IRQ_CALLBACK_RTOC)
            .unwrap();
    }
    ppc_app
        .memory
        .write_u32_be(IRQ_TVECTOR, IRQ_CALLBACK)
        .unwrap();
    ppc_app
        .memory
        .write_u32_be(IRQ_TVECTOR + 4, IRQ_CALLBACK_RTOC)
        .unwrap();
    let routine = [
        0x323c, 0x012c, // MOVE.W #300,D1
        0x51c9, 0xfffe, // DBRA D1,*
        0x202f, 0x0004, // MOVE.L 4(SP),D0
        0x5e80, // ADDQ.L #7,D0
        0x2f40, 0x0008, // MOVE.L D0,8(SP)
        0x4e74, 0x0004, // RTD #4
    ];
    ppc_app.memory.add_region(
        IRQ_M68K_ROUTINE,
        routine.into_iter().flat_map(u16::to_be_bytes).collect(),
    );
    ppc_app.memory.add_region(IRQ_DESCRIPTOR, vec![0; 0x100]);
    ppc_app
        .memory
        .write_u16_be(IRQ_DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
        .unwrap();
    ppc_app
        .memory
        .write_u8(IRQ_DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    let record = IRQ_DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    ppc_app
        .memory
        .write_u32_be(record, irq_proc_info())
        .unwrap();
    ppc_app
        .memory
        .write_u8(record + ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA)
        .unwrap();
    ppc_app
        .memory
        .write_u32_be(
            record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            IRQ_M68K_ROUTINE,
        )
        .unwrap();
    ppc_app.memory.add_region(IRQ_DATA, vec![0; 0x200]);
    let mut call_universal_proc = test_ppc_import_binding(0, "InterfaceLib", "CallUniversalProc");
    call_universal_proc.trap_pc = PPC_IMPORT_TRAP_BASE;
    call_universal_proc.dispatcher_target = PpcImportDispatcherTarget::CallUniversalProc;
    let mut use_res_file = test_ppc_import_binding(1, "InterfaceLib", "UseResFile");
    use_res_file.trap_pc = IRQ_USE_RES_FILE_TRAP;
    use_res_file.dispatcher_target = PpcImportDispatcherTarget::UseResFile;
    ppc_app.import_count = 2;
    ppc_app.imports = vec![call_universal_proc, use_res_file];

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.set_instructions_per_tick(IRQ_TICK_CYCLES);
    runner.tick_budget = IRQ_TICK_CYCLES as i32;
    let ppc_app = runner.native.application_mut().expect("PPC app");
    ppc_app.cpu.pc = IRQ_LOOP_PC;
    for register in (0..32).filter(|register| *register != 1) {
        ppc_app.cpu.gpr[register] = 0x5a00_0000 | (register as u32 * 0x0101);
        ppc_app.cpu.fpr[register] = 0x4000_0000_0000_0000 | register as u64;
    }
    ppc_app.cpu.gpr[20] = 0;
    ppc_app.cpu.cr = 0x2468_ace0;
    ppc_app.cpu.ctr = 0x1357_9bdf;
    ppc_app.cpu.lr = 0x0bad_f00c;
    ppc_app.cpu.xer = 0x2000_0000;
    let sp = ppc_app.cpu.gpr[1];
    for offset in 0..IRQ_RED_ZONE {
        ppc_app
            .memory
            .write_u8(sp - IRQ_RED_ZONE + offset, 0xa5 ^ offset as u8)
            .unwrap();
    }
    runner
}

fn irq_install_vbl(runner: &mut FixtureRunner, task_ptr: u32, tvector: u32) {
    let ppc_app = runner.native.application_mut().expect("PPC app");
    ppc_app.memory.write_u16_be(task_ptr + 4, 1).unwrap(); // vType
    ppc_app.memory.write_u32_be(task_ptr + 6, tvector).unwrap();
    ppc_app.memory.write_u16_be(task_ptr + 10, 1).unwrap();
    ppc_app.vbl_tasks.push(PpcVblTaskRecord {
        task_ptr,
        architecture: CallbackTaskArchitecture::PowerPc,
        slot: None,
        pending: false,
    });
}

fn irq_install_timer(runner: &mut FixtureRunner, task_ptr: u32, tvector: u32) {
    let fire_at_tick = runner.guest_tick().wrapping_add(1);
    let ppc_app = runner.native.application_mut().expect("PPC app");
    ppc_app.timer_tasks.push(PpcTimerTaskRecord {
        task_ptr,
        architecture: CallbackTaskArchitecture::PowerPc,
        extended: false,
        callback: tvector,
        active: true,
        fire_at_tick,
        fire_at_subtick: u64::from(fire_at_tick) * 1_000_000,
        last_fired_tick: None,
    });
}

fn irq_read(runner: &FixtureRunner, address: u32) -> u32 {
    runner
        .native
        .application()
        .expect("PPC app")
        .memory
        .clone()
        .read_u32_be(address)
        .expect("mapped fixture word")
}

fn irq_parked(runner: &FixtureRunner) -> bool {
    runner
        .native
        .application()
        .expect("PPC app")
        .parked_interrupt_callback
        .is_some()
}

/// Step until `done` holds while nothing is parked, checking that the
/// application never halts. Returns how many steps observed a parked callback.
fn irq_run_until(
    runner: &mut FixtureRunner,
    mut done: impl FnMut(&FixtureRunner) -> bool,
) -> usize {
    let mut parked_steps = 0;
    for step in 0..4000 {
        let (_, running) = runner.run_steps(50, None);
        let ppc_app = runner.native.application().expect("PPC app");
        assert!(
            running && !runner.is_halted(),
            "halted at step {step}: pc=${:08x} lr=${:08x} r1=${:08x} parked={}",
            ppc_app.cpu.pc,
            ppc_app.cpu.lr,
            ppc_app.cpu.gpr[1],
            ppc_app.parked_interrupt_callback.is_some(),
        );
        if irq_parked(runner) {
            parked_steps += 1;
        } else if done(runner) {
            return parked_steps;
        }
    }
    panic!("interrupt fixture did not finish");
}

/// The interrupted context comes back whole: every register except the
/// loop's pc and counter, and the Red Zone below its stack pointer.
fn irq_assert_foreground_intact(runner: &FixtureRunner, expected: &ppc::PpcExecutionContext) {
    let ppc_app = runner.native.application().expect("PPC app");
    assert!(ppc_app.parked_interrupt_callback.is_none());
    let mut actual = ppc_app.cpu.capture_execution_context();
    assert!(
        [IRQ_LOOP_PC, IRQ_LOOP_PC + 4].contains(&actual.architectural().pc),
        "foreground resumed at ${:08x}",
        actual.architectural().pc
    );
    actual.architectural_mut().pc = expected.architectural().pc;
    actual.architectural_mut().gpr[20] = expected.architectural().gpr[20];
    assert_eq!(actual.architectural(), expected.architectural());
    let sp = expected.architectural().gpr[1];
    let mut memory = ppc_app.memory.clone();
    for offset in 0..IRQ_RED_ZONE {
        assert_eq!(
            memory.read_u8(sp - IRQ_RED_ZONE + offset),
            Some(0xa5 ^ offset as u8),
            "red zone byte {offset}"
        );
    }
}

fn irq_foreground_context(runner: &FixtureRunner) -> ppc::PpcExecutionContext {
    runner
        .native
        .application()
        .expect("PPC app")
        .cpu
        .capture_execution_context()
}

#[test]
fn vbl_callback_awaiting_68k_resumes_with_its_own_registers() {
    let mut runner = irq_runner(irq_mixed_mode_callback(1, Some(IRQ_TASK)));
    let expected = irq_foreground_context(&runner);
    irq_install_vbl(&mut runner, IRQ_TASK, IRQ_TVECTOR);

    let parked_steps = irq_run_until(&mut runner, |runner| irq_read(runner, IRQ_COUNTER) >= 1);
    assert!(parked_steps > 1, "the 68K call spans several slices");
    assert_eq!(irq_read(&runner, IRQ_RESULT), IRQ_ARGUMENT + 7);
    assert_eq!(irq_read(&runner, IRQ_PRIVATE), IRQ_PRIVATE_VALUE);
    irq_assert_foreground_intact(&runner, &expected);
    let first_count = irq_foreground_context(&runner).architectural().gpr[20];

    irq_run_until(&mut runner, |runner| irq_read(runner, IRQ_COUNTER) >= 2);
    irq_assert_foreground_intact(&runner, &expected);
    assert!(irq_foreground_context(&runner).architectural().gpr[20] > first_count);
    assert!(runner.dispatcher.guest_calls.is_empty());
    let ppc_app = runner.native.application().expect("PPC app");
    assert!(ppc_app
        .vbl_tasks
        .iter()
        .any(|task| task.task_ptr == IRQ_TASK));
    assert_eq!(ppc_app.interrupt_callback_parks, 2);
}

#[test]
fn interrupt_callback_can_make_sequential_mixed_mode_calls() {
    let mut runner = irq_runner(irq_mixed_mode_callback(2, None));
    let expected = irq_foreground_context(&runner);
    irq_install_vbl(&mut runner, IRQ_TASK, IRQ_TVECTOR);

    irq_run_until(&mut runner, |runner| irq_read(runner, IRQ_COUNTER) >= 1);

    assert_eq!(irq_read(&runner, IRQ_FIRST_RESULT), IRQ_ARGUMENT + 7);
    assert_eq!(irq_read(&runner, IRQ_RESULT), IRQ_ARGUMENT + 14);
    assert_eq!(irq_read(&runner, IRQ_PRIVATE), IRQ_PRIVATE_VALUE);
    assert_eq!(irq_read(&runner, IRQ_COUNTER), 1);
    irq_assert_foreground_intact(&runner, &expected);
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(
        runner
            .native
            .application()
            .expect("PPC app")
            .interrupt_callback_parks,
        1
    );
}

#[test]
fn vbl_removal_waits_for_parked_return() {
    let mut runner = irq_runner(irq_mixed_mode_callback(1, None));
    let expected = irq_foreground_context(&runner);
    irq_install_vbl(&mut runner, IRQ_TASK, IRQ_TVECTOR);

    let mut parked_steps = 0;
    for _ in 0..4000 {
        let (_, running) = runner.run_steps(50, None);
        assert!(running && !runner.is_halted());
        let ppc_app = runner.native.application().expect("PPC app");
        if ppc_app.parked_interrupt_callback.is_some() {
            // The task did not reset vblCount, but it has not returned yet.
            assert_eq!(ppc_app.memory.clone().read_u16_be(IRQ_TASK + 10), Some(0));
            assert!(ppc_app
                .vbl_tasks
                .iter()
                .any(|task| task.task_ptr == IRQ_TASK));
            parked_steps += 1;
        } else if irq_read(&runner, IRQ_COUNTER) >= 1 {
            break;
        }
    }

    assert!(parked_steps > 0);
    assert_eq!(irq_read(&runner, IRQ_COUNTER), 1);
    assert!(!runner
        .native
        .application()
        .expect("PPC app")
        .vbl_tasks
        .iter()
        .any(|task| task.task_ptr == IRQ_TASK));
    irq_assert_foreground_intact(&runner, &expected);
}

#[test]
fn timer_callback_awaiting_68k_resumes_with_its_own_registers() {
    let mut runner = irq_runner(irq_mixed_mode_callback(1, None));
    let expected = irq_foreground_context(&runner);
    irq_install_timer(&mut runner, IRQ_TIMER_TASK, IRQ_TVECTOR);

    let parked_steps = irq_run_until(&mut runner, |runner| irq_read(runner, IRQ_COUNTER) >= 1);

    assert!(parked_steps > 1);
    assert_eq!(irq_read(&runner, IRQ_RESULT), IRQ_ARGUMENT + 7);
    assert_eq!(irq_read(&runner, IRQ_PRIVATE), IRQ_PRIVATE_VALUE);
    irq_assert_foreground_intact(&runner, &expected);
    assert!(runner.dispatcher.guest_calls.is_empty());
    let ppc_app = runner.native.application().expect("PPC app");
    assert_eq!(ppc_app.interrupt_callback_parks, 1);
    assert!(!ppc_app.timer_tasks[0].active);
}

#[test]
fn doubleback_buffer_stays_pending_until_parked_callback_returns() {
    const CHANNEL: u32 = 0x0300_1000;
    const HEADER: u32 = 0x0300_2000;
    const BUFFER: u32 = 0x0300_3000;

    let mut runner = irq_runner(irq_mixed_mode_callback(1, None));
    let expected = irq_foreground_context(&runner);
    let pending_mask = |runner: &FixtureRunner| {
        runner
            .native
            .application()
            .expect("PPC app")
            .sound
            .manager
            .double_buffer_playbacks[0]
            .callback_pending_mask
    };
    {
        let ppc_app = runner.native.application_mut().expect("PPC app");
        ppc_app.sound.manager.replace_double_buffer_playbacks(vec![
            PpcSoundDoubleBufferPlaybackRecord {
                channel: CHANNEL,
                header: HEADER,
                buffers: [BUFFER, 0],
                callback: IRQ_TVECTOR,
                callback_architecture: CallbackTaskArchitecture::PowerPc,
                sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
                num_channels: 1,
                sample_size: 8,
                compression_id: 0,
                format: 0,
                packet_size: 0,
                current_buffer_index: 0,
                callback_pending_mask: 1,
                active: false,
                host_initialized: false,
                host_buffer_loaded: false,
            },
        ]);
        ppc_app
            .sound
            .manager
            .replace_pending_process_doublebacks(vec![PpcSoundDoubleBackRecord {
                architecture: CallbackTaskArchitecture::PowerPc,
                channel: CHANNEL,
                header: HEADER,
                exhausted_buffer: BUFFER,
                exhausted_buffer_index: 0,
                callback: IRQ_TVECTOR,
                tick: 1,
                instruction_count: 1,
            }]);
    }

    runner.fire_pending_ppc_sound_doublebacks();

    assert!(!runner.is_halted());
    assert!(irq_parked(&runner));
    assert_eq!(pending_mask(&runner), 1);
    // The exhausted buffer is neither queued nor delivered again meanwhile.
    runner
        .native
        .application_mut()
        .expect("PPC app")
        .sound
        .manager
        .with_mut(|sound| FixtureRunner::queue_ppc_doubleback(sound, 0, 2, 2));
    runner.fire_pending_ppc_sound_doublebacks();
    let sound = &runner.native.application().expect("PPC app").sound;
    assert!(sound.manager.pending_process_doublebacks.is_empty());
    assert_eq!(sound.completion_invocations.len(), 1);

    irq_run_until(&mut runner, |runner| irq_read(runner, IRQ_COUNTER) >= 1);

    assert_eq!(pending_mask(&runner), 0);
    assert_eq!(irq_read(&runner, IRQ_RESULT), IRQ_ARGUMENT + 7);
    assert_eq!(irq_read(&runner, IRQ_PRIVATE), IRQ_PRIVATE_VALUE);
    irq_assert_foreground_intact(&runner, &expected);
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn interrupts_are_masked_while_a_callback_is_parked() {
    const SOUND_CHANNEL: u32 = 0x0300_1000;

    let mut runner = irq_runner(irq_mixed_mode_callback(1, None));
    let expected = irq_foreground_context(&runner);
    // The plain VBL task runs first in each tick; the parking task follows.
    irq_install_vbl(&mut runner, IRQ_PLAIN_TASK, irq_plain_tvector(0));
    irq_install_vbl(&mut runner, IRQ_TASK, IRQ_TVECTOR);
    let plain_vbl = |runner: &FixtureRunner| irq_read(runner, irq_plain_marker(0));
    let timer = |runner: &FixtureRunner| irq_read(runner, irq_plain_marker(1));
    let completion = |runner: &FixtureRunner| irq_read(runner, irq_plain_marker(2));
    let queued_completions = |runner: &FixtureRunner| {
        runner
            .native
            .application()
            .expect("PPC app")
            .sound
            .manager
            .pending_sound_callbacks
            .len()
    };

    for _ in 0..4000 {
        runner.run_steps(50, None);
        if irq_parked(&runner) {
            break;
        }
    }
    assert!(irq_parked(&runner));
    let plain_vbl_at_park = plain_vbl(&runner);
    irq_install_timer(&mut runner, IRQ_TIMER_TASK, irq_plain_tvector(1));
    queue_ppc_sound_completion(
        &mut runner.native.application_mut().expect("PPC app").sound,
        SOUND_CHANNEL,
        irq_plain_tvector(2),
    );

    let mut masked_steps = 0;
    while irq_parked(&runner) {
        runner.fire_pending_ppc_sound_completions();
        assert_eq!(plain_vbl(&runner), plain_vbl_at_park);
        assert_eq!(timer(&runner), 0);
        assert_eq!(completion(&runner), 0);
        assert_eq!(queued_completions(&runner), 1);
        let (_, running) = runner.run_steps(50, None);
        assert!(running && !runner.is_halted());
        masked_steps += 1;
        assert!(masked_steps < 4000);
    }
    assert!(masked_steps > 1);
    assert_eq!(irq_read(&runner, IRQ_COUNTER), 1);

    // After the return, the due timer fires late and once, the queued
    // completion is delivered, and the other VBL task counts again.
    irq_run_until(&mut runner, |runner| {
        timer(runner) == 1 && plain_vbl(runner) > plain_vbl_at_park + 1
    });
    runner.fire_pending_ppc_sound_completions();
    assert_eq!(completion(&runner), 1);
    assert_eq!(queued_completions(&runner), 0);
    for _ in 0..20 {
        runner.run_steps(50, None);
    }
    assert_eq!(timer(&runner), 1);
    assert!(!runner.is_halted());
    let ppc_app = runner.native.application().expect("PPC app");
    assert!(ppc_app.parked_interrupt_callback.is_none());
    assert_eq!(ppc_app.interrupt_callback_parks, 1);
    irq_assert_foreground_intact(&runner, &expected);
}

#[test]
fn plain_interrupt_callbacks_restore_the_interrupted_context() {
    let mut runner = irq_runner(irq_mixed_mode_callback(1, None));
    irq_install_vbl(&mut runner, IRQ_PLAIN_TASK, irq_plain_tvector(0));
    irq_install_timer(&mut runner, IRQ_TIMER_TASK, irq_plain_tvector(1));
    let tick = runner.guest_tick();
    let ppc_app = runner.native.application_mut().expect("PPC app");
    let before = ppc_app.cpu.capture_execution_context();
    let refnum = ppc_app.current_resource_refnum();

    runner
        .process_context
        .with_memory_and_cfm(|memory_manager, cfm| {
            let vbl = ppc_app.fire_vbl_tasks_for_ticks_with_process_services(
                tick,
                1,
                usize::MAX,
                u64::from(IRQ_TICK_CYCLES),
                false,
                false,
                memory_manager,
                cfm,
            );
            assert_eq!(vbl.len(), 1);
            let timers = ppc_app.fire_timer_tasks_for_ticks_with_process_services(
                tick,
                1,
                usize::MAX,
                u64::from(IRQ_TICK_CYCLES),
                false,
                false,
                memory_manager,
                cfm,
            );
            assert_eq!(timers.len(), 1);
        });

    let mut memory = ppc_app.memory.clone();
    assert_eq!(memory.read_u32_be(irq_plain_marker(0)), Some(1));
    assert_eq!(memory.read_u32_be(irq_plain_marker(1)), Some(1));
    assert!(ppc_app.parked_interrupt_callback.is_none());
    assert_eq!(ppc_app.interrupt_callback_parks, 0);
    assert_eq!(
        ppc_app.cpu.capture_execution_context().architectural(),
        before.architectural()
    );
    assert_eq!(ppc_app.current_resource_refnum(), refnum);
}

#[test]
fn parked_callback_return_is_matched_by_task_and_stack() {
    use crate::execution_kernel::{ExecutionTaskId, ExecutionTaskState};

    const CALLBACK_SP: u32 = PPC_STACK_TOP - 0x200;
    const INTERRUPTED_SP: u32 = PPC_STACK_TOP - 64;
    for (owner_matches, stack_matches) in [(true, true), (false, true), (true, false)] {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        let calls = ppc_app.toolbox_startup.execution.calls().shared_handle();
        let other = calls.create_task().expect("second task");
        assert!(calls.set_scheduling_state(other, ExecutionTaskState::Ready));
        assert!(calls.switch_to_task(other));
        assert!(calls.begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: IRQ_M68K_ROUTINE,
                rtoc: 0,
            },
            IRQ_M68K_ROUTINE,
            0x0303_0080,
            0x0304_0000,
            0x0303_0084,
            crate::guest_call::M68kRegisterState::default(),
            None,
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));
        let awaited_call = calls.top_call_id().expect("submitted call");
        assert!(calls.switch_to_task(ExecutionTaskId::APPLICATION));

        let mut interrupted = ppc_app.cpu.capture_execution_context();
        interrupted.architectural_mut().pc = PPC_CODE_BASE;
        interrupted.architectural_mut().lr = PPC_HALT_PC;
        interrupted.architectural_mut().gpr[1] = INTERRUPTED_SP;
        interrupted.architectural_mut().gpr[31] = 0x1234_5678;
        ppc_app.parked_interrupt_callback = Some(PpcParkedInterruptCallback {
            level: PpcCallbackLevel::Interrupt,
            task: if owner_matches {
                ExecutionTaskId::APPLICATION
            } else {
                other
            },
            awaited_call,
            callback_sp: CALLBACK_SP,
            interrupted,
            interrupted_refnum: None,
            on_return: PpcInterruptReturnWork::None,
        });
        // A callback epilogue that returns to `halt_pc`.
        let return_sp = if stack_matches {
            CALLBACK_SP
        } else {
            CALLBACK_SP - 0x40
        };
        ppc_app.cpu.pc = PPC_CODE_BASE;
        ppc_app.cpu.lr = PPC_HALT_PC;
        ppc_app.cpu.gpr[1] = return_sp;
        ppc_app.cpu.gpr[31] = 0;

        let probe = ppc_app.run_with_hle_imports(64);

        assert!(matches!(probe.result, PpcRunResult::Halted { pc, .. } if pc == PPC_HALT_PC));
        if owner_matches && stack_matches {
            assert!(ppc_app.parked_interrupt_callback.is_none());
            assert_eq!(ppc_app.cpu.gpr[1], INTERRUPTED_SP);
            assert_eq!(ppc_app.cpu.gpr[31], 0x1234_5678);
        } else {
            assert!(ppc_app.parked_interrupt_callback.is_some());
            assert_eq!(ppc_app.cpu.gpr[1], return_sp);
            assert_eq!(ppc_app.cpu.gpr[31], 0);
        }
    }
}

#[test]
fn interrupt_callbacks_outside_the_parking_gate_keep_the_old_restore_path() {
    use crate::execution_kernel::ExecutionTaskState;
    use crate::guest_procedure::GuestIsa;

    for pending_68k_frame in [false, true] {
        let mut runner = irq_runner(irq_mixed_mode_callback(1, None));
        irq_install_vbl(&mut runner, IRQ_TASK, IRQ_TVECTOR);
        let tick = runner.guest_tick();
        let ppc_app = runner.native.application_mut().expect("PPC app");
        let calls = ppc_app.toolbox_startup.execution.calls().shared_handle();
        if pending_68k_frame {
            // The interrupted code is already waiting on a 68K call.
            assert!(calls.begin_powerpc_to_m68k(
                crate::guest_call::GuestCallTarget {
                    isa: GuestIsa::M68k,
                    entry: IRQ_M68K_ROUTINE,
                    rtoc: 0,
                },
                IRQ_M68K_ROUTINE,
                0x0303_0080,
                0x0304_0000,
                0x0303_0084,
                crate::guest_call::M68kRegisterState::default(),
                None,
                IRQ_LOOP_PC,
                0,
                PpcNativeReturnGpr3::Preserve,
            ));
        } else {
            // A native thread is current but does not own the native CPU.
            let thread = calls.create_task().expect("second task");
            assert!(calls.bind_task_entry_isa(thread, GuestIsa::PowerPc));
            assert!(calls.set_scheduling_state(thread, ExecutionTaskState::Ready));
            assert!(calls.switch_to_task(thread));
        }
        let depth = calls.depth();
        let before = ppc_app.cpu.capture_execution_context();
        let refnum = ppc_app.current_resource_refnum();

        let probes = runner
            .process_context
            .with_memory_and_cfm(|memory_manager, cfm| {
                ppc_app.fire_vbl_tasks_for_ticks_with_process_services(
                    tick,
                    1,
                    usize::MAX,
                    u64::from(IRQ_TICK_CYCLES),
                    false,
                    false,
                    memory_manager,
                    cfm,
                )
            });

        assert_eq!(probes.len(), 1, "pending_68k_frame={pending_68k_frame}");
        assert!(matches!(
            probes[0].invocation.result,
            PpcRunResult::Halted { pc, .. } if pc == PPC_IMPORT_TRAP_BASE
        ));
        // The callback's call was submitted, but nothing parked: the old
        // path restored the interrupted context exactly.
        assert_eq!(calls.depth(), depth + 1);
        assert!(ppc_app.parked_interrupt_callback.is_none());
        assert_eq!(ppc_app.interrupt_callback_parks, 0);
        assert_eq!(
            ppc_app.cpu.capture_execution_context().architectural(),
            before.architectural()
        );
        assert_eq!(ppc_app.current_resource_refnum(), refnum);
    }
}

#[test]
fn file_completion_parks_after_a_continued_slice_and_pops_once() {
    const PARAMETER_BLOCK: u32 = IRQ_DATA + 0x180;

    // The delay outlasts one callback slice, so the completion first hits
    // its cycle cap and parks only in its continued slice.
    let mut runner = irq_runner(irq_mixed_mode_callback_with(1, None, 300_000, None));
    let expected = irq_foreground_context(&runner);
    let ppc_app = runner.native.application_mut().expect("PPC app");
    ppc_app.pending_file_completions.extend([
        (PARAMETER_BLOCK, IRQ_TVECTOR),
        (PARAMETER_BLOCK, irq_plain_tvector(1)),
    ]);
    let queued = |runner: &FixtureRunner| {
        runner
            .native
            .application()
            .expect("PPC app")
            .pending_file_completions
            .len()
    };

    let mut continued = false;
    let mut parked_steps = 0;
    for _ in 0..4000 {
        let (_, running) = runner.run_steps(50, None);
        assert!(running && !runner.is_halted());
        let ppc_app = runner.native.application().expect("PPC app");
        if ppc_app.file_completion_context.is_some() {
            assert!(!irq_parked(&runner));
            assert_eq!(queued(&runner), 2);
            continued = true;
        } else if irq_parked(&runner) {
            // Delivered once: popped, and the next completion stays queued.
            assert_eq!(queued(&runner), 1);
            assert_eq!(irq_read(&runner, irq_plain_marker(1)), 0);
            parked_steps += 1;
        } else if irq_read(&runner, irq_plain_marker(1)) == 1 {
            break;
        }
    }

    assert!(continued);
    assert!(parked_steps > 1);
    assert_eq!(queued(&runner), 0);
    assert_eq!(irq_read(&runner, IRQ_COUNTER), 1);
    assert_eq!(irq_read(&runner, IRQ_RESULT), IRQ_ARGUMENT + 7);
    assert_eq!(irq_read(&runner, IRQ_PRIVATE), IRQ_PRIVATE_VALUE);
    irq_assert_foreground_intact(&runner, &expected);
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(
        runner
            .native
            .application()
            .expect("PPC app")
            .interrupt_callback_parks,
        1
    );
}

#[test]
fn sound_completion_awaiting_68k_resumes_with_its_own_registers() {
    let mut runner = irq_runner(irq_mixed_mode_callback(1, None));
    let expected = irq_foreground_context(&runner);
    queue_ppc_sound_completion(
        &mut runner.native.application_mut().expect("PPC app").sound,
        0x0300_1000,
        IRQ_TVECTOR,
    );

    runner.fire_pending_ppc_sound_completions();

    assert!(!runner.is_halted());
    assert!(irq_parked(&runner));
    let sound = &runner.native.application().expect("PPC app").sound;
    assert!(sound.manager.pending_sound_callbacks.is_empty());
    assert_eq!(sound.completion_invocations.len(), 1);

    irq_run_until(&mut runner, |runner| irq_read(runner, IRQ_COUNTER) >= 1);

    assert_eq!(irq_read(&runner, IRQ_RESULT), IRQ_ARGUMENT + 7);
    assert_eq!(irq_read(&runner, IRQ_PRIVATE), IRQ_PRIVATE_VALUE);
    irq_assert_foreground_intact(&runner, &expected);
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn draw_sprocket_vbl_awaiting_68k_resumes_with_its_own_registers() {
    let mut runner = irq_runner(irq_mixed_mode_callback(1, None));
    let expected = irq_foreground_context(&runner);
    let draw_sprocket = &mut runner
        .native
        .application_mut()
        .expect("PPC app")
        .draw_sprocket;
    draw_sprocket.active_context = Some(IRQ_DATA + 0x1c0);
    draw_sprocket.context_state = crate::loader::ppc::PpcDspContextPlayState::Active;
    draw_sprocket.vbl_proc = Some(IRQ_TVECTOR);
    draw_sprocket.vbl_refcon = Some(0x1234);

    irq_run_until(&mut runner, |runner| irq_read(runner, IRQ_COUNTER) >= 2);

    assert_eq!(irq_read(&runner, IRQ_RESULT), IRQ_ARGUMENT + 7);
    assert_eq!(irq_read(&runner, IRQ_PRIVATE), IRQ_PRIVATE_VALUE);
    irq_assert_foreground_intact(&runner, &expected);
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(
        runner
            .native
            .application()
            .expect("PPC app")
            .interrupt_callback_parks,
        2
    );
}

#[test]
fn parked_callback_return_restores_the_interrupted_resource_file() {
    const FOREGROUND_FILE: i16 = 0x30;
    const CALLBACK_FILE: u16 = 0x41;
    const CONTINUATION_FILE: u16 = 0x42;

    let mut runner = irq_runner(irq_mixed_mode_callback_with(
        1,
        None,
        0,
        Some((CALLBACK_FILE, CONTINUATION_FILE)),
    ));
    runner
        .native
        .application_mut()
        .expect("PPC app")
        .set_current_resource_refnum(FOREGROUND_FILE);
    let expected = irq_foreground_context(&runner);
    irq_install_vbl(&mut runner, IRQ_TASK, IRQ_TVECTOR);
    let refnum = |runner: &FixtureRunner| {
        runner
            .native
            .application()
            .expect("PPC app")
            .current_resource_refnum()
    };

    let mut parked_steps = 0;
    for _ in 0..4000 {
        let (_, running) = runner.run_steps(50, None);
        assert!(running && !runner.is_halted());
        if irq_parked(&runner) {
            // The callback's own selection stands while it is parked.
            assert_eq!(refnum(&runner), CALLBACK_FILE as i16);
            parked_steps += 1;
        } else if irq_read(&runner, IRQ_COUNTER) >= 1 {
            break;
        }
    }

    assert!(parked_steps > 0);
    assert_eq!(refnum(&runner), FOREGROUND_FILE);
    assert_eq!(irq_read(&runner, IRQ_RESULT), IRQ_ARGUMENT + 7);
    irq_assert_foreground_intact(&runner, &expected);
}

#[test]
fn parked_event_loop_timer_masks_timers_but_not_interrupts_during_its_68k_call() {
    let mut runner = irq_runner(irq_mixed_mode_callback(1, None));
    let expected = irq_foreground_context(&runner);
    let tick = runner.guest_tick();
    runner
        .native
        .application_mut()
        .expect("PPC app")
        .install_test_event_loop_timer(
            IRQ_DATA + 0x1e0,
            IRQ_TVECTOR,
            tick.wrapping_add(1),
            1,
            tick.wrapping_add(0x10_0000),
        );
    irq_install_vbl(&mut runner, IRQ_PLAIN_TASK, irq_plain_tvector(0));
    let plain_vbl = |runner: &FixtureRunner| irq_read(runner, irq_plain_marker(0));

    let mut in_flight_vbls = 0;
    for _ in 0..4000 {
        let before = plain_vbl(&runner);
        let parks_before = runner
            .native
            .application()
            .expect("PPC app")
            .interrupt_callback_parks;
        let call_pending_before = irq_parked(&runner) && !runner.dispatcher.guest_calls.is_empty();
        let (_, running) = runner.run_steps(50, None);
        assert!(running && !runner.is_halted());
        let ppc_app = runner.native.application().expect("PPC app");
        if ppc_app.parked_interrupt_callback.is_some() {
            // The repeating timer is not re-entered while it is parked: one
            // park per firing that has not returned yet.
            assert_eq!(
                ppc_app.interrupt_callback_parks,
                u64::from(irq_read(&runner, IRQ_COUNTER)) + 1
            );
            // Count only steps that began and ended inside the same park
            // with its 68K call still pending.
            if call_pending_before
                && ppc_app.interrupt_callback_parks == parks_before
                && !runner.dispatcher.guest_calls.is_empty()
                && plain_vbl(&runner) > before
            {
                in_flight_vbls += 1;
            }
        } else if irq_read(&runner, IRQ_COUNTER) >= 1 {
            break;
        }
    }

    // A task-level timer does not hold off interrupt-level callbacks while
    // its 68K call runs.
    assert!(in_flight_vbls > 0);
    assert!(irq_read(&runner, IRQ_COUNTER) >= 1);
    assert_eq!(irq_read(&runner, IRQ_RESULT), IRQ_ARGUMENT + 7);
    assert_eq!(irq_read(&runner, IRQ_PRIVATE), IRQ_PRIVATE_VALUE);
    irq_assert_foreground_intact(&runner, &expected);
}
