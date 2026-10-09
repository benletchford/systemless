use super::*;
use crate::managers::resource::ResourceFork;
use crate::memory::globals::{addr, DEFAULT_UNIT_TABLE_ENTRY_COUNT};
use crate::trap::dispatch::TrapTableProfile;
use std::collections::HashMap;

#[test]
fn init_app_zeroes_fresh_top_of_stack() {
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

    let stack_seed_start = app.initial_sp.saturating_sub(0x8000);
    assert_eq!(
        runner.bus.read_long(stack_seed_start),
        0,
        "fresh process stack should match a newly initialized application partition"
    );
}

#[test]
fn init_app_seeds_appparmhandle_with_empty_finder_information() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner
        .dispatcher_mut()
        .set_launched_app_path("Games/Armor Alley");
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

    let handle = runner.bus.read_long(addr::APP_PARM_HANDLE);
    assert_ne!(
        handle, 0,
        "AppParmHandle should point at Finder launch information"
    );
    let data_ptr = runner.bus.read_long(handle);
    assert_ne!(
        data_ptr, 0,
        "Finder launch information handle should be loaded"
    );
    assert_eq!(
        runner.bus.get_alloc_size(data_ptr),
        Some(4),
        "empty Finder launch information is message/count only"
    );
    assert_eq!(
        runner.bus.read_word(data_ptr),
        0,
        "message should be appOpen for a normal application launch"
    );
    assert_eq!(
        runner.bus.read_word(data_ptr + 2),
        0,
        "normal application launch has no selected documents"
    );
}

#[test]
fn init_app_seeds_current_application_fcb_low_memory_state() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner
        .dispatcher_mut()
        .vfs_rsrc
        .insert("Games/Armor Alley".to_string(), vec![0xA5; 1234]);
    runner
        .dispatcher_mut()
        .set_vfs_entry_metadata("Games/Armor Alley", *b"APPL", *b"TEST", 0);
    runner
        .dispatcher_mut()
        .set_launched_app_path("Games/Armor Alley");
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
        runner.bus.read_word(addr::CUR_APREF_NUM),
        APPLICATION_RESOURCE_REFNUM,
        "CurApRefNum should be the app resource fork access path"
    );
    assert_eq!(
        runner.bus.read_word(addr::FS_FCB_LEN),
        HFS_FCB_SIZE,
        "System 7 FCB size should be exposed for direct low-memory readers"
    );
    let fcb_buffer = runner.bus.read_long(addr::FCB_S_PTR);
    assert_ne!(fcb_buffer, 0, "FCBSPtr should point to an FCB buffer");
    assert_eq!(
        runner.bus.read_word(fcb_buffer),
        HFS_FCB_BUFFER_SIZE,
        "FCB buffer length should include the leading length word"
    );
    let fcb = fcb_buffer + APPLICATION_RESOURCE_REFNUM as u32;
    assert_eq!(
        runner.bus.read_word(fcb + 4),
        0x0200,
        "the application access path should describe a resource fork"
    );
    assert_eq!(runner.bus.read_long(fcb + 8), 1234);
    assert_eq!(runner.bus.read_long(fcb + 12), 1234);
    assert_eq!(runner.bus.read_long(fcb + 50), u32::from_be_bytes(*b"APPL"));
    assert_eq!(
        runner.bus.read_long(fcb + 58),
        *runner.dispatcher.default_dir_id
    );

    let vcb = runner.bus.read_long(fcb + 20);
    assert_ne!(vcb, 0, "fcbVPtr should point to the boot volume VCB");
    assert_eq!(runner.bus.read_long(addr::DEF_VCB_PTR), vcb);
    assert_eq!(runner.bus.read_long(addr::VCB_Q_HDR + 2), vcb);
    assert_eq!(runner.bus.read_long(addr::VCB_Q_HDR + 6), vcb);
    assert_eq!(runner.bus.read_word(vcb + 8), 0x4244);
    assert_eq!(
        runner.bus.read_word(vcb + 78) as i16,
        crate::trap::dispatch::BOOT_VOLUME_REF_NUM
    );
    assert_eq!(
        runner
            .dispatcher
            .open_files
            .get(&APPLICATION_RESOURCE_REFNUM),
        Some(&"__rsrc__Games/Armor Alley".to_string())
    );
}

#[test]
fn init_app_sets_legacy_sound_driver_low_memory_defaults() {
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
        runner
            .bus
            .read_byte(crate::memory::globals::addr::SD_VOLUME),
        1,
        "SdVolume ($0260) should boot to the nonzero legacy compatibility value"
    );
    assert_eq!(
        runner
            .bus
            .read_byte(crate::memory::globals::addr::SOUND_LEVEL),
        0,
        "SoundLevel ($027F) is a distinct Sound Driver amplitude byte"
    );
    let sound_base = runner
        .bus
        .read_long(crate::memory::globals::addr::SOUND_BASE);
    assert_eq!(
        sound_base, 0x007F_7880,
        "SoundBase ($0266) should point at the 370-word legacy sound buffer in reserved display memory"
    );
    assert_eq!(
        runner.bus.read_byte(sound_base),
        0x80,
        "legacy SoundBase buffer starts at neutral amplitude"
    );
}

#[test]
fn init_app_materializes_the_device_manager_unit_table() {
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

    let table = runner.bus.read_long(addr::U_TABLE_BASE);
    assert_ne!(table, 0, "UTableBase should address the unit table");
    assert_eq!(
        runner.bus.read_word(addr::UNIT_NTRY_CNT),
        DEFAULT_UNIT_TABLE_ENTRY_COUNT
    );
    assert_eq!(
        runner.bus.get_alloc_size(table),
        Some(u32::from(DEFAULT_UNIT_TABLE_ENTRY_COUNT) * 4)
    );
    let display_dce_handle = runner.bus.read_long(table);
    assert_ne!(display_dce_handle, 0, "unit zero should hold the main display DCE");
    let display_dce = runner.bus.read_long(display_dce_handle);
    assert_ne!(display_dce, 0);
    assert_eq!(runner.bus.read_word(display_dce + 24), u16::MAX);
    let gdevice = runner.bus.read_long(runner.bus.read_long(0x8A4));
    assert_eq!(runner.bus.read_word(gdevice), u16::MAX);
    assert!(
        runner
            .bus
            .read_bytes(table + 4, usize::from(DEFAULT_UNIT_TABLE_ENTRY_COUNT - 1) * 4)
            .iter()
            .all(|&byte| byte == 0),
        "other unit-table slots should start empty"
    );
}

#[test]
fn init_app_preserves_tagged_pointer_addresses_with_lo3bytes() {
    for addressing_32_bit in [false, true] {
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            FixtureRunnerConfig {
                addressing_32_bit,
                ..FixtureRunnerConfig::default()
            },
        );
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

        for _launch in 0..2 {
            runner.bus.write_long(addr::LO3_BYTES, 0);
            runner.init_app(&app);
            let call_site = 0x0002_0000;
            runner.bus.write_word(call_site, 0xC0B8); // AND.L ($031A).W,D0
            runner.bus.write_word(call_site + 2, 0x031A);
            runner.m68k.cpu.write_reg(Register::PC, call_site);
            runner.m68k.cpu.write_reg(Register::D0, 0xA521_3456);

            let (steps, running) = runner.run_steps(1, None);

            assert!(running);
            assert_eq!(steps, 1);
            assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x0021_3456);
        }
    }
}

#[test]
fn init_app_seeds_mmu32bit_low_memory_flag() {
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
        runner
            .bus
            .read_byte(crate::memory::globals::addr::MMU32_BIT),
        1,
        "MMU32Bit ($0CB2) should mirror Systemless's default 32-bit addressing mode"
    );
}

#[test]
fn init_app_can_start_in_twenty_four_bit_addressing_mode() {
    let mut runner = FixtureRunner::new(
        8 * 1024 * 1024,
        FixtureRunnerConfig {
            addressing_32_bit: false,
            ..FixtureRunnerConfig::default()
        },
    );
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

    assert!(!runner.bus.addressing_32_bit());
    assert_eq!(runner.bus.ram_size(), 8 * 1024 * 1024);
    assert_eq!(runner.dispatcher.mmu_mode, 0);
    assert_eq!(
        runner
            .bus
            .read_byte(crate::memory::globals::addr::MMU32_BIT),
        0
    );
}

#[test]
fn twenty_four_bit_runner_caps_guest_ram_at_sixteen_megabytes() {
    let runner = FixtureRunner::new(
        64 * 1024 * 1024,
        FixtureRunnerConfig {
            addressing_32_bit: false,
            ..FixtureRunnerConfig::default()
        },
    );

    assert_eq!(runner.bus.ram_size(), 0x0100_0000);
}

#[test]
fn init_app_seeds_callable_swap_mmu_mode_trap_table_entry() {
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

    let entry = runner
        .bus
        .read_long(crate::memory::globals::addr::SWAP_MMU_MODE_TRAP);
    assert_ne!(entry, 0);
    assert_eq!(
        [runner.bus.read_word(entry), runner.bus.read_word(entry + 2),],
        [0xA05D, 0x4E75],
        "the $0574 OS trap-table entry should target SwapMMUMode followed by RTS"
    );

    let call_site = 0x0002_0000u32;
    let initial_sp = 0x007F_FE00u32;
    runner.bus.write_word(call_site, 0x2078); // MOVEA.L ($0574).W,A0
    runner.bus.write_word(call_site + 2, 0x0574);
    runner.bus.write_word(call_site + 4, 0x4E90); // JSR (A0)
    runner.bus.write_word(call_site + 6, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, call_site);
    runner.m68k.cpu.write_reg(Register::A7, initial_sp);
    runner.m68k.cpu.write_reg(Register::D0, 0);

    let (steps, running) = runner.run_steps(4, None);

    assert!(running);
    assert_eq!(steps, 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), call_site + 6);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), initial_sp);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::D0),
        1,
        "SwapMMUMode should return the previous 32-bit mode in D0"
    );
    assert_eq!(
        runner
            .bus
            .read_byte(crate::memory::globals::addr::MMU32_BIT),
        0,
        "the indirect call should update the requested addressing mode"
    );
    runner.bus.write_long(0x0002_1000, 0x1234_5678);
    assert_eq!(
        runner.bus.read_long(0xAB02_1000),
        0x1234_5678,
        "SwapMMUMode must change actual guest address translation"
    );
}

#[test]
fn swap_mmu_mode_returns_on_classic_stack_with_more_than_sixteen_megabytes() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = runner.load_app(&fork).expect("load app");
    runner.init_app(&app);

    let call_site = 0x0002_0000;
    let return_pc = call_site + 8;
    let stack = app.initial_sp - 0x200;
    runner.bus.write_word(call_site, 0xA05D); // SwapMMUMode
    runner.bus.write_word(call_site + 2, 0x4E75); // RTS
    runner.bus.write_long(stack, return_pc);
    runner.m68k.cpu.write_reg(Register::PC, call_site);
    runner.m68k.cpu.write_reg(Register::A7, stack);
    runner.m68k.cpu.write_reg(Register::D0, 0);

    let (steps, running) = runner.run_steps(2, None);

    assert!(running);
    assert_eq!(steps, 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert!(!runner.bus.addressing_32_bit());

    // PenMode has a permanent system-owned come-from head. Its raw
    // synthetic identity remains protected provenance while 24-bit guest
    // accesses are masked, so inline dispatch must not mistake the head
    // for an application-installed native patch.
    let pen_mode_site = call_site + 0x10;
    runner.bus.write_word(pen_mode_site, 0xA89C);
    runner.bus.write_word(stack + 4, 8);
    runner.m68k.cpu.write_reg(Register::PC, pen_mode_site);

    let (steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), pen_mode_site + 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 6);
}

#[test]
fn init_app_seeds_callable_profile_come_from_head() {
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
    let entry = crate::trap::dispatch::OS_TRAP_TABLE_BASE + 0x78 * 4;
    let previous_head = runner.bus.read_long(entry);
    runner
        .dispatcher
        .install_trap_address(&mut runner.bus, 0xA078, 0x0021_1000)
        .unwrap();
    runner.bus.write_long(0x28, 0x0021_2000);
    runner.init_app(&app);
    assert_eq!(
        runner.dispatcher.trap_table_profile,
        Some(TrapTableProfile::M68k68040)
    );
    assert_ne!(runner.bus.read_long(entry), previous_head);
    assert_ne!(
        runner.dispatcher.trap_table_address(&runner.bus, 0xA078),
        Some(0x0021_1000)
    );
    assert!(runner.dispatcher.aline_vector_is_default(&runner.bus));

    let head = runner
        .bus
        .read_long(crate::trap::dispatch::OS_TRAP_TABLE_BASE + 0x78 * 4);
    let successor = runner.bus.read_long(head + 4);
    assert_eq!(runner.bus.read_long(head), 0x6006_4EF9);
    runner.m68k.cpu.write_reg(Register::PC, head);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), successor);
}

#[test]
fn profile_unimplemented_gateway_executes_system_error_12() {
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

    let gateway = runner
        .dispatcher
        .trap_table_address(&runner.bus, 0xAA6E)
        .unwrap();
    assert_eq!(runner.bus.read_word(gateway), 0xAE6E);
    let call_site = 0x0002_0000;
    let initial_sp = 0x007F_FE00;
    runner.bus.write_word(call_site, 0x4E90); // JSR (A0)
    runner.m68k.cpu.write_reg(Register::A0, gateway);
    runner.m68k.cpu.write_reg(Register::A7, initial_sp);
    runner.m68k.cpu.write_reg(Register::PC, call_site);

    let (steps, running) = runner.run_steps(2, None);

    assert_eq!(steps, 2);
    assert!(!running);
    assert_eq!(runner.dispatcher.current_trap_caller, Some(call_site + 2));
    assert_eq!(
        runner
            .bus
            .read_word(crate::memory::globals::addr::DS_ERR_CODE),
        12
    );
}

#[test]
fn init_app_seeds_callable_swap_font_low_memory_vector() {
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

    let swap_font_trampoline = runner
        .bus
        .read_long(crate::memory::globals::addr::J_SWAP_FONT);
    assert_ne!(swap_font_trampoline, 0);
    assert_eq!(
        [
            runner.bus.read_word(swap_font_trampoline),
            runner.bus.read_word(swap_font_trampoline + 2),
            runner.bus.read_word(swap_font_trampoline + 4),
        ],
        [0x205F, 0xA901, 0x4ED0]
    );

    let fm_input_sp = 0x007F_FE00u32;
    let fm_input = 0x0002_1000u32;
    let return_pc = 0x0002_0000u32;
    runner.bus.write_word(fm_input, 3); // family
    runner.bus.write_word(fm_input + 2, 12); // size
    runner.bus.write_byte(fm_input + 4, 0); // face
    runner.bus.write_byte(fm_input + 5, 1); // needBits
    runner.bus.write_word(fm_input + 6, 0); // device
    runner.bus.write_word(fm_input + 8, 1); // numer.v
    runner.bus.write_word(fm_input + 10, 1); // numer.h
    runner.bus.write_word(fm_input + 12, 1); // denom.v
    runner.bus.write_word(fm_input + 14, 1); // denom.h
    runner.bus.write_long(fm_input_sp, fm_input); // CONST VAR inRec
    runner.bus.write_long(fm_input_sp + 4, 0); // result slot
    runner.bus.write_long(fm_input_sp - 4, return_pc);
    runner.bus.write_word(return_pc, 0x4E71); // NOP
    runner
        .m68k
        .cpu
        .write_reg(Register::PC, swap_font_trampoline);
    runner.m68k.cpu.write_reg(Register::A7, fm_input_sp - 4);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), fm_input_sp + 4);
    assert_ne!(
        runner.bus.read_long(fm_input_sp + 4),
        0,
        "JSwapFont should return a non-NIL FMOutPtr through the Pascal result slot"
    );
}
