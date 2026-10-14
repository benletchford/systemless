mod support;
use support::*;
use systemless_future::{
    cpu::{Call, Cpu},
    load::{resource::ResourceFork, LoadedApplication, A5_BASE},
    memory::{Access, GuestPtr},
    runtime::Stop,
    Error,
};
#[test]
fn resource_nonstandard_type_offset_names_signed_ids_and_empty_fork() {
    let parsed = ResourceFork::parse(&code()).unwrap();
    let r = parsed.get(*b"DATA", -7).unwrap();
    assert_eq!(r.name, Some(vec![0x80, b'x']));
    assert_eq!(r.data, [1, 2, 3]);
    assert_eq!(parsed.entries().count(), 4);
    assert_eq!(
        ResourceFork::parse(&resource(vec![]))
            .unwrap()
            .entries()
            .count(),
        0
    );
}
#[test]
fn code_launches_via_a5_jump_table_and_calls_another_segment() {
    let mut app = LoadedApplication::code(&code()).unwrap();
    assert_eq!(app.info.sections.len(), 2);
    app.runtime
        .memory
        .write(GuestPtr(0x16a), &42u32.to_be_bytes())
        .unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(app.runtime.memory.u32(GuestPtr(A5_BASE - 4)).unwrap(), 42);
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(A5_BASE - 8)).unwrap(),
        0x12345678
    );
    assert_eq!(app.runtime.services.calls, [Call::Ticks, Call::Exit(0)]);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(A5_BASE + 34)).unwrap(),
        0x4ef9
    );
    assert!(!app.runtime.abi.fixture);
}
#[test]
fn pef_relocates_main_and_import_vectors_then_runs_real_cfm_calls() {
    let f = pef();
    let mut app = LoadedApplication::pef(&f.bytes, Some(&code())).unwrap();
    let data = app.info.sections[1].0;
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(data)).unwrap(),
        app.info.entry
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 4)).unwrap(), data);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(data + 24), 40).unwrap(),
        [0; 40]
    );
    assert!(matches!(
        app.runtime.memory.write(GuestPtr(app.info.entry), &[0]),
        Err(Error::Memory {
            access: Access::Write,
            ..
        })
    ));
    app.runtime
        .memory
        .write(GuestPtr(0x16a), &42u32.to_be_bytes())
        .unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 16)).unwrap(), 42);
    assert_eq!(app.runtime.services.calls, [Call::Ticks, Call::Exit(0)]);
    assert!(app.runtime.process.resources.get(*b"DATA", -7).is_some());
    if let Cpu::PowerPc(c) = &app.runtime.cpu {
        assert_eq!(c.gpr[31], data);
    } else {
        panic!()
    }
}
#[test]
fn truncated_files_never_panic_or_launch() {
    let r = code();
    for end in 0..r.len() {
        assert!(
            LoadedApplication::code(&r[..end]).is_err(),
            "accepted resource prefix {end}"
        );
    }
    let f = pef();
    for end in 0..f.bytes.len() {
        assert!(
            LoadedApplication::pef(&f.bytes[..end], None).is_err(),
            "accepted PEF prefix {end}"
        );
    }
}
#[test]
fn reject_resource_ranges_duplicates_and_code_layout_errors() {
    let valid = code();
    let map = u32::from_be_bytes(valid[4..8].try_into().unwrap()) as usize;
    let types = map + 36;
    let first_ref =
        types + u16::from_be_bytes(valid[types + 8..types + 10].try_into().unwrap()) as usize;
    let mut cases = Vec::new();
    let mut b = valid.clone();
    w32(&mut b, 4, 256);
    cases.push(b);
    let mut b = valid.clone();
    w16(&mut b, first_ref + 12, 0);
    cases.push(b); // duplicate CODE ID
    let mut b = valid.clone();
    w32(&mut b, first_ref + 4, 0x00fffffe);
    cases.push(b);
    let mut b = valid.clone();
    w16(&mut b, map + 26, 28);
    cases.push(b);
    for b in cases {
        assert!(ResourceFork::parse(&b).is_err());
    }
    let mut b = valid;
    w32(&mut b, 260 + 8, 15);
    assert!(LoadedApplication::code(&b).is_err());
}
#[test]
fn reject_pef_unresolved_imports_bad_relocations_and_invalid_entry() {
    let f = pef();
    let mut cases = Vec::new();
    let mut b = f.bytes.clone();
    b[f.strings + 13] = b'Z';
    cases.push(b);
    let mut b = f.bytes.clone();
    w16(&mut b, f.reloc, 0x600f);
    cases.push(b); // nonexistent import
    let mut b = f.bytes.clone();
    w16(&mut b, f.reloc, 0xffff);
    cases.push(b);
    let mut b = f.bytes.clone();
    w32(&mut b, f.data, 0x7fff0000);
    cases.push(b);
    let mut b = f.bytes.clone();
    w32(&mut b, 40 + 8, u32::MAX);
    cases.push(b);
    let mut b = f.bytes.clone();
    w32(&mut b, f.loader + 8, 0);
    cases.push(b); // init cannot silently skip
    let mut b = f.bytes.clone();
    w32(&mut b, f.loader + 92, u32::MAX);
    cases.push(b);
    for b in cases {
        assert!(LoadedApplication::pef(&b, None).is_err());
    }
}
#[test]
fn pef_imports_add_to_nonzero_slots_and_weak_missing_symbols_resolve_zero() {
    let mut f = pef();
    w32(&mut f.bytes, f.data + 8, 4);
    let app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.sections[1].0;
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(data + 8)).unwrap(),
        0x70010004
    );
    let mut f = pef();
    f.bytes[f.strings + 13] = b'Z';
    f.bytes[f.loader + 80] = 0x82;
    let app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.sections[1].0;
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 8)).unwrap(), 0);
}
#[test]
fn loaded_apps_cannot_call_private_fixture_traps() {
    let mut b = code(); // find the TickCount opcode in the CODE payload
    let at = b.windows(2).position(|w| w == [0xa9, 0x75]).unwrap();
    b[at..at + 2].copy_from_slice(&[0xa0, 0]);
    let mut app = LoadedApplication::code(&b).unwrap();
    assert!(matches!(
        app.runtime.advance(100),
        Err(Error::Service(0xa000))
    ));
    assert!(app.runtime.services.calls.is_empty());
}
#[test]
fn both_loaded_architectures_read_resources_and_allocate_through_one_process_model() {
    let resource_fork = resource_code();
    let f = resource_pef();
    let mut applications = [
        LoadedApplication::code(&resource_fork).unwrap(),
        LoadedApplication::pef(&f.bytes, Some(&resource_fork)).unwrap(),
    ];
    for app in &mut applications {
        assert_eq!(app.runtime.advance(200).unwrap(), Stop::Exit(0));
        let slots = if app.info.architecture == systemless_future::cpu::Architecture::M68k {
            [
                A5_BASE - 4,
                A5_BASE - 8,
                A5_BASE - 12,
                A5_BASE - 16,
                A5_BASE - 20,
            ]
        } else {
            let data = app.info.sections[1].0;
            [data + 64, data + 68, data + 72, data + 76, data + 80]
        };
        let values: Vec<_> = slots
            .into_iter()
            .map(|p| app.runtime.memory.u32(GuestPtr(p)).unwrap())
            .collect();
        assert_eq!(values[1], 1);
        assert_eq!(values[2], 3);
        assert_eq!(values[0], values[3]);
        let master = app.runtime.memory.u32(GuestPtr(values[0])).unwrap();
        assert_eq!(
            app.runtime.memory.read(GuestPtr(master), 3).unwrap(),
            [1, 2, 3]
        );
        let allocated = app.runtime.memory.u32(GuestPtr(values[4])).unwrap();
        assert_eq!(
            app.runtime.memory.read(GuestPtr(allocated), 12).unwrap(),
            [0; 12]
        );
        assert_eq!(
            app.runtime
                .process
                .handle_size(&app.runtime.memory, values[4])
                .unwrap(),
            12
        );
    }
    assert_eq!(
        applications[0].runtime.services.calls,
        applications[1].runtime.services.calls
    );
}
#[test]
fn heap_reuses_disposed_storage_reports_exhaustion_and_retains_resource_identity() {
    use systemless_future::memory::Memory;
    use systemless_future::process::{Process, HEAP_SIZE};
    let mut p = Process::default();
    let mut m = Memory::default();
    let h = p.new_handle(&mut m, 16, true).unwrap();
    let data = m.u32(GuestPtr(h)).unwrap();
    m.write(GuestPtr(data), &[99; 16]).unwrap();
    p.dispose_handle(&mut m, h).unwrap();
    let next = p.new_handle(&mut m, 16, true).unwrap();
    assert_eq!(next, h);
    assert_eq!(m.read(GuestPtr(data), 16).unwrap(), [0; 16]);
    assert_eq!(p.new_handle(&mut m, u32::MAX, false).unwrap(), 0);
    assert_eq!(p.mem_error, -108);
    assert_eq!(p.new_handle(&mut m, HEAP_SIZE, false).unwrap(), 0);
    assert_eq!(p.mem_error, -108);
    assert_eq!(p.handle_size(&m, 0).unwrap(), 0);
    assert_eq!(p.mem_error, -109);
    p.resources = ResourceFork::parse(&code()).unwrap();
    let first = p.get_resource(&mut m, *b"DATA", -7).unwrap();
    let second = p.get_resource(&mut m, *b"DATA", -7).unwrap();
    assert_eq!(first, second);
    assert_eq!(p.get_resource(&mut m, *b"DATA", 123).unwrap(), 0);
    assert_eq!(p.res_error, -192);
    assert_eq!(p.get_resource(&mut m, *b"NONE", 123).unwrap(), 0);
    assert_eq!(p.res_error, 0);
}
#[test]
fn resource_handle_and_guest_bytes_survive_cpu_switch_in_one_loaded_runtime() {
    use systemless_future::{abi::Service, cpu::Architecture, memory::Permissions};
    let mut app = LoadedApplication::code(&resource_code()).unwrap();
    while app.runtime.services.calls.is_empty() {
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    }
    let sp = match &app.runtime.cpu {
        Cpu::M68k(c) => c.dar[15],
        _ => unreachable!(),
    };
    let handle = app.runtime.memory.u32(GuestPtr(sp)).unwrap();
    let data = app.runtime.memory.u32(GuestPtr(handle)).unwrap();
    app.runtime.memory.write(GuestPtr(data), &[0xa5]).unwrap();
    let instructions = [
        0x3c604441u32,
        0x60635441,
        0x3880fff9,
        systemless_future::fixtures::branch_link(0x900c, 0xa000),
    ];
    app.runtime
        .memory
        .map(
            GuestPtr(0x9000),
            instructions.iter().flat_map(|w| w.to_be_bytes()).collect(),
            Permissions::RX,
        )
        .unwrap();
    app.runtime.abi.imports.insert(0xa000, Service::GetResource);
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, 0x9000, sp);
    assert_eq!(app.runtime.advance(5).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.cpu.result(), handle);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(data), 3).unwrap(),
        [0xa5, 2, 3]
    );
}
#[test]
fn guest_error_globals_and_clock_are_observable_without_host_state_copies() {
    let code = code_program(&[0x70ff, 0xa122, 0x3038, 0x0220, 0x2b40, 0xfffc, 0xa9f4]);
    let mut app = LoadedApplication::code(&code).unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16,
        -108
    );
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(A5_BASE - 4)).unwrap() as i32,
        -108
    );
    let f = pef();
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime.instructions = 999;
    app.runtime
        .memory
        .write(GuestPtr(0x16a), &41u32.to_be_bytes())
        .unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(
        app.runtime
            .memory
            .u32(GuestPtr(app.info.sections[1].0 + 16))
            .unwrap(),
        42
    );
}

#[test]
fn pef_lifecycle_runs_once_across_budgets_with_a_real_initialization_block() {
    let f = lifecycle_pef(0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.sections[1].0;
    assert_eq!(app.runtime.advance(0).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.instructions, 0);
    assert_eq!(app.runtime.advance(6).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 24)).unwrap(), 1);
    let block = app.runtime.memory.u32(GuestPtr(data + 28)).unwrap();
    for offset in [0, 4, 8] {
        assert_ne!(app.runtime.memory.u32(GuestPtr(block + offset)).unwrap(), 0);
    }
    assert_eq!(app.runtime.memory.u32(GuestPtr(block + 12)).unwrap(), 0); // kInMem
    let container = app.runtime.memory.u32(GuestPtr(block + 16)).unwrap();
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(block + 20)).unwrap(),
        f.bytes.len() as u32
    );
    assert_eq!(
        app.runtime
            .memory
            .read(GuestPtr(container), f.bytes.len())
            .unwrap(),
        f.bytes
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(block + 24)).unwrap(), 0); // not in place, reserved zero
    let name = app.runtime.memory.u32(GuestPtr(block + 28)).unwrap();
    assert_eq!(app.runtime.memory.read(GuestPtr(name), 1).unwrap(), &[0]);
    assert_eq!(app.runtime.advance(6).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 24)).unwrap(), 2);
    assert_eq!(app.runtime.advance(5).unwrap(), Stop::Returned(7));
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 24)).unwrap(), 3);
    let instructions = app.runtime.instructions;
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Returned(7));
    assert_eq!(app.runtime.instructions, instructions);
}

#[test]
fn pef_failed_initialization_never_runs_main_or_cleanup() {
    let f = lifecycle_pef(-50);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.sections[1].0;
    let error = app.runtime.advance(100).unwrap_err().to_string();
    assert!(error.contains("initializer returned -50"), "{error}");
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 24)).unwrap(), 1);
    assert!(app.runtime.advance(100).is_err());
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 24)).unwrap(), 1);
}

#[test]
fn pef_lifecycle_rejects_bad_vectors_and_bounds_nonreturning_initializers() {
    for field in [8, 16] {
        let mut f = lifecycle_pef(0);
        w32(&mut f.bytes, f.loader + field, 100);
        assert!(LoadedApplication::pef(&f.bytes, None).is_err());
        let mut f = lifecycle_pef(0);
        w32(&mut f.bytes, f.loader + field + 4, 3);
        assert!(LoadedApplication::pef(&f.bytes, None).is_err());
    }
    let mut f = lifecycle_pef(0);
    let code = u32::from_be_bytes(f.bytes[60..64].try_into().unwrap()) as usize;
    w32(&mut f.bytes, code + 20, 0x48000000); // b .
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert_eq!(app.runtime.advance(25).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.advance(25).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.instructions, 50);
    let data = app.info.sections[1].0;
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 24)).unwrap(), 0);
}

#[test]
fn pef_exit_to_shell_runs_cleanup_with_its_toc_once_with_or_without_init() {
    for initialize in [true, false] {
        let mut f = lifecycle_exit_pef();
        if !initialize {
            w32(&mut f.bytes, f.loader + 8, u32::MAX);
        }
        let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
        let data = app.info.sections[1].0;
        assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
        assert_eq!(
            app.runtime.memory.u32(GuestPtr(data + 32)).unwrap(),
            if initialize { 2 } else { 1 }
        );
        assert_eq!(
            app.runtime.services.calls,
            vec![systemless_future::cpu::Call::Exit(0)]
        );
        assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
        assert_eq!(
            app.runtime.memory.u32(GuestPtr(data + 32)).unwrap(),
            if initialize { 2 } else { 1 }
        );
    }
}

#[test]
fn pef_faults_do_not_run_cleanup_and_cleanup_itself_is_budgeted() {
    let mut f = lifecycle_pef(0);
    let code = u32::from_be_bytes(f.bytes[60..64].try_into().unwrap()) as usize;
    w32(&mut f.bytes, code, 0x80608000); // unmapped load in main
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert!(app.runtime.advance(100).is_err());
    let data = app.info.sections[1].0;
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 24)).unwrap(), 1);
    let mut f = lifecycle_pef(0);
    w32(&mut f.bytes, code + 40, 0x48000000); // cleanup loop
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.instructions, 200);
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 24)).unwrap(), 2);
}

#[test]
fn code_startup_can_inspect_and_return_through_exit_to_shell() {
    // MOVEA.L (SP),A0; MOVE.L (A0),D0; MOVE.L D0,-4(A5); RTS.
    let file = code_program(&[0x2057, 0x2010, 0x2b40, 0xfffc, 0x4e75]);
    let mut app = LoadedApplication::code(&file).unwrap();
    assert_eq!(app.runtime.advance(20).unwrap(), Stop::Exit(0));
    assert_eq!(
        app.runtime
            .memory
            .u32(GuestPtr(systemless_future::load::A5_BASE - 4))
            .unwrap(),
        0xa9f44afc
    );
    assert!(app
        .runtime
        .memory
        .write(GuestPtr(systemless_future::load::APPLICATION_RETURN), &[0])
        .is_err());
}

#[test]
fn trap_lookup_masks_numbers_preserves_errors_and_calls_toolbox_stack_abi() {
    use systemless_future::traps::{address, UNIMPLEMENTED};
    assert_eq!(address(0xa1ad, false), UNIMPLEMENTED);
    assert_eq!(address(0xa89f, true), UNIMPLEMENTED);
    assert_eq!(address(0xf975, true), address(0x175, true));
    assert_ne!(address(0xa975, true), UNIMPLEMENTED);
    // Get TickCount's address, reserve a long result and JSR through it.
    let file = code_program(&[
        0x303c, 0xa975, 0xa746, 0x598f, 0x4e90, 0x201f, 0x2b40, 0xfffc, 0xa9f4,
    ]);
    let mut app = LoadedApplication::code(&file).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x16a), &42u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(
        app.runtime
            .memory
            .u32(GuestPtr(systemless_future::load::A5_BASE - 4))
            .unwrap(),
        42
    );
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16,
        -108
    );
}

#[test]
fn callable_os_trap_uses_original_word_in_d1_and_shared_heap() {
    let file = code_program(&[
        0x303c, 0xa122, 0xa346, 0x700c, 0x323c, 0xa322, 0x4e90, 0x2b48, 0xfffc, 0xa9f4,
    ]);
    let mut app = LoadedApplication::code(&file).unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    let h = app
        .runtime
        .memory
        .u32(GuestPtr(systemless_future::load::A5_BASE - 4))
        .unwrap();
    assert_ne!(h, 0);
    assert_eq!(
        app.runtime
            .process
            .handle_size(&app.runtime.memory, h)
            .unwrap(),
        12
    );
    let p = app.runtime.memory.u32(GuestPtr(h)).unwrap();
    assert_eq!(app.runtime.memory.read(GuestPtr(p), 12).unwrap(), &[0; 12]);
    assert!(app
        .runtime
        .services
        .calls
        .contains(&systemless_future::cpu::Call::NewHandle {
            size: 12,
            clear: true
        }));
}

#[test]
fn unsupported_trap_pointer_is_callable_and_faults_explicitly() {
    let file = code_program(&[0x303c, 0xa1ad, 0xa346, 0x4e90, 0xa9f4]);
    let mut app = LoadedApplication::code(&file).unwrap();
    let error = app.runtime.advance(100).unwrap_err().to_string();
    assert!(error.contains("dsCoreErr 12"), "{error}");
    assert!(app.runtime.advance(1).is_err());
}

#[test]
fn ppc_trap_lookup_has_the_same_identity_as_68k() {
    let f = pef_program(
        &[
            0x7fc802a6, 0x7c5f1378, 0x3860a975, 0x38800001, 0x819f0008, 0x800c0000, 0x804c0004,
            0x7c0903a6, 0x4e800421, 0x907f0010, 0x7fc803a6, 0x4e800020,
        ],
        &[b"NGetTrapAddress"],
        24,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let expected = systemless_future::traps::address(0xa975, true);
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Returned(expected));
    assert_eq!(
        app.runtime
            .memory
            .u32(GuestPtr(app.info.sections[1].0 + 16))
            .unwrap(),
        expected
    );
}

#[test]
fn callable_resource_trap_pops_arguments_but_preserves_pascal_result() {
    let f = code_program(&[
        0x303c, 0xa9a0, 0xa746, 0x598f, 0x2f3c, 0x4441, 0x5441, 0x3f3c, 0xfff9, 0x4e90, 0x201f,
        0x2b40, 0xfffc, 0xa9f4,
    ]);
    let mut app = LoadedApplication::code(&f).unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    let h = app
        .runtime
        .memory
        .u32(GuestPtr(systemless_future::load::A5_BASE - 4))
        .unwrap();
    let p = app.runtime.memory.u32(GuestPtr(h)).unwrap();
    assert_eq!(app.runtime.memory.read(GuestPtr(p), 3).unwrap(), &[1, 2, 3]);
    if let systemless_future::cpu::Cpu::M68k(c) = &app.runtime.cpu {
        assert_eq!(
            c.dar[15],
            systemless_future::load::STACK_BASE + systemless_future::load::STACK_SIZE - 0x100
        );
    }
}

#[test]
fn current_resource_file_is_shared_process_state_on_both_abis() {
    let file = code_program(&[0x554f, 0xa994, 0x301f, 0x48c0, 0x2b40, 0xfffc, 0xa9f4]);
    let mut app = LoadedApplication::code(&file).unwrap();
    let reference = app.runtime.process.current_resource_file();
    assert!(reference > 0);
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(
        app.runtime
            .memory
            .u32(GuestPtr(systemless_future::load::A5_BASE - 4))
            .unwrap(),
        reference as u32
    );
    let f = pef_program(
        &[
            0x7fc802a6, 0x81820008, 0x800c0000, 0x804c0004, 0x7c0903a6, 0x4e800421, 0x7fc803a6,
            0x4e800020,
        ],
        &[b"CurResFile"],
        16,
    );
    let mut app = LoadedApplication::pef(&f.bytes, Some(&file)).unwrap();
    assert_eq!(
        app.runtime.advance(100).unwrap(),
        Stop::Returned(app.runtime.process.current_resource_file() as u32)
    );
}

#[test]
fn named_resources_are_case_sensitive_and_share_cached_handles_with_id_lookup() {
    let fork = resource(vec![(*b"DATA", 7, Some(vec![0x80, b'X']), vec![3, 4, 5])]);
    let f = pef();
    let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    let pointer = GuestPtr(0x50000000);
    app.runtime
        .memory
        .map(
            pointer,
            vec![2, 0x80, b'X'],
            systemless_future::memory::Permissions::RW,
        )
        .unwrap();
    let h = app
        .runtime
        .process
        .get_named_resource(&mut app.runtime.memory, *b"DATA", pointer, false)
        .unwrap();
    assert_ne!(h, 0);
    assert_eq!(
        h,
        app.runtime
            .process
            .get_resource(&mut app.runtime.memory, *b"DATA", 7)
            .unwrap()
    );
    assert_eq!(
        h,
        app.runtime
            .process
            .get_named_resource(&mut app.runtime.memory, *b"DATA", pointer, true)
            .unwrap()
    );
    app.runtime.memory.write(pointer, &[2, 0x80, b'x']).unwrap();
    assert_eq!(
        app.runtime
            .process
            .get_named_resource(&mut app.runtime.memory, *b"DATA", pointer, true)
            .unwrap(),
        0
    );
    assert_eq!(app.runtime.process.res_error, -192);
    assert_eq!(
        app.runtime
            .process
            .get_named_resource(&mut app.runtime.memory, *b"NONE", pointer, false)
            .unwrap(),
        0
    );
    assert_eq!(app.runtime.process.res_error, 0);
    app.runtime.memory.write(pointer, &[255]).unwrap();
    assert!(app
        .runtime
        .process
        .get_named_resource(&mut app.runtime.memory, *b"DATA", pointer, false)
        .is_err());
}

#[test]
fn named_resource_stack_and_cfm_adapters_return_the_same_resource_contents() {
    let f = pef_program(
        &[
            0x7fc802a6, 0x3c604441, 0x60635441, 0x3c805000, 0x81820008, 0x800c0000, 0x804c0004,
            0x7c0903a6, 0x4e800421, 0x7fc803a6, 0x4e800020,
        ],
        &[b"Get1NamedResource"],
        16,
    );
    let fork = resource(vec![(*b"DATA", 7, Some(b"test".to_vec()), vec![3, 4, 5])]);
    let mut p = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    let code = code_program(&[
        0x598f, 0x2f3c, 0x4441, 0x5441, 0x2f3c, 0x5000, 0, 0xa820, 0x201f, 0x2b40, 0xfffc, 0xa9f4,
    ]);
    let mut m = LoadedApplication::code(&code).unwrap();
    m.runtime.process.resources = ResourceFork::parse(&fork).unwrap();
    for app in [&mut p, &mut m] {
        app.runtime
            .memory
            .map(
                GuestPtr(0x50000000),
                b"\x04test".to_vec(),
                systemless_future::memory::Permissions::RW,
            )
            .unwrap();
        let stop = app.runtime.advance(100).unwrap();
        let h = match stop {
            Stop::Returned(h) => h,
            Stop::Exit(0) => app
                .runtime
                .memory
                .u32(GuestPtr(systemless_future::load::A5_BASE - 4))
                .unwrap(),
            _ => panic!("{stop:?}"),
        };
        let ptr = app.runtime.memory.u32(GuestPtr(h)).unwrap();
        assert_eq!(
            app.runtime.memory.read(GuestPtr(ptr), 3).unwrap(),
            &[3, 4, 5]
        );
    }
}

#[test]
fn stdlib_data_imports_share_guest_storage_and_enforce_symbol_classes() {
    let names: &[&[u8]] = &[
        b"_IntEnv",
        b"__C_phase",
        b"__target_for_exit",
        b"_exit_status",
        b"__NubAt3",
        b"__C_phase",
    ];
    let mut fixture = pef_program(&[0x38600000, 0x4e800020], names, 32);
    fixture.bytes[fixture.strings..fixture.strings + 8].copy_from_slice(b"StdCLib\0");
    for i in 0..names.len() {
        fixture.bytes[fixture.loader + 80 + i * 4] = 1;
    }
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let toc = app.info.toc_or_a5;
    let memory = &mut app.runtime.memory;
    let mut pointers = Vec::new();
    for i in 0..names.len() {
        pointers.push(memory.u32(GuestPtr(toc + 8 + i as u32 * 4)).unwrap());
    }
    assert_eq!(pointers[1], pointers[5]);
    assert_ne!(pointers[1], pointers[3]);
    let env = pointers[0];
    assert_eq!(memory.u16(GuestPtr(env)).unwrap(), 0x5348);
    assert_eq!(memory.u32(GuestPtr(env + 2)).unwrap(), 0);
    for offset in [6, 10] {
        let array = memory.u32(GuestPtr(env + offset)).unwrap();
        assert_ne!(array, 0);
        assert_eq!(memory.u32(GuestPtr(array)).unwrap(), 0);
    }
    assert_eq!(memory.read(GuestPtr(pointers[2]), 256).unwrap(), &[0; 256]);
    assert_eq!(memory.u32(GuestPtr(pointers[4])).unwrap(), 0);
    memory
        .write(GuestPtr(pointers[1]), &2u32.to_be_bytes())
        .unwrap();
    assert_eq!(memory.u32(GuestPtr(pointers[5])).unwrap(), 2);
    assert_eq!(app.runtime.advance(10).unwrap(), Stop::Returned(0));
    fixture.bytes[fixture.loader + 80] = 2;
    assert!(
        matches!(LoadedApplication::pef(&fixture.bytes,None),Err(Error::Load(message)) if message.contains("data imported as function"))
    );
    fixture.bytes[fixture.loader + 80] = 0x82; // weak still must match the known symbol's type
    assert!(LoadedApplication::pef(&fixture.bytes, None).is_err());
}

#[test]
fn pef_stdlib_jump_calls_resume_saved_guest_execution() {
    // Real PPC instructions and CFM transition-vector calls. longjmp returns
    // through the earlier setjmp continuation, never its own following opcode.
    let mut fixture = pef_program(
        &[
            0x7fc802a6, // mflr r30
            0x7c5f1378, // mr r31,r2
            0x80620010, // lwz r3,16(r2): guest jump buffer
            0x81820008, 0x800c0000, 0x804c0004, 0x7c0903a6, 0x4e800421,
            0x7fe2fb78, // mr r2,r31
            0x2c030000, // cmpwi r3,0
            0x40820024, // bne saved return path
            0x80620010, 0x38800007, // buffer, result 7
            0x8182000c, 0x800c0000, 0x804c0004, 0x7c0903a6, 0x4e800421,
            0x00000000, // unreachable: longjmp must not return here
            0x7fc803a6, 0x4e800020, // mtlr r30; blr
        ],
        &[b"__setjmp", b"longjmp", b"__target_for_exit"],
        20,
    );
    fixture.bytes[fixture.strings..fixture.strings + 8].copy_from_slice(b"StdCLib\0");
    fixture.bytes[fixture.loader + 88] = 1;
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Returned(7));
    assert!(matches!(
        app.runtime.services.calls.as_slice(),
        [Call::SetJump(_), Call::LongJump { value: 7, .. }]
    ));
}

#[test]
fn stdlib_exit_uses_shared_fragment_termination_once() {
    let mut f = lifecycle_exit_pef();
    f.bytes[f.strings..f.strings + 8].copy_from_slice(b"StdCLib\0");
    f.bytes[f.strings + 13..f.strings + 18].copy_from_slice(b"exit\0");
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.sections[1].0;
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 32)).unwrap(), 2);
    assert_eq!(app.runtime.services.calls, vec![Call::CExit(0)]);
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 32)).unwrap(), 2);
}

#[test]
fn stdlib_breakpoint_faults_instead_of_silently_continuing() {
    let mut f = pef_program(
        &[
            0x81820008, 0x800c0000, 0x804c0004, 0x7c0903a6, 0x4e800421, 0x4e800020,
        ],
        &[b"_BreakPoint"],
        12,
    );
    f.bytes[f.strings..f.strings + 8].copy_from_slice(b"StdCLib\0");
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert!(
        matches!(app.runtime.advance(100),Err(Error::Cpu(message)) if message.contains("debugger breakpoint"))
    );
    assert_eq!(
        app.runtime.services.calls,
        vec![Call::BreakPoint(GuestPtr(0))]
    );
    assert!(app.runtime.advance(100).is_err());
}

#[test]
fn code_startup_can_initialize_globals_through_low_memory_launch_metadata() {
    let fork = code_program(&[
        0x2278, 0x0908, // MOVEA.L CurStackBase,A1
        0x32bc, 0x1234, // MOVE.W #$1234,(A1)
        0x2038, 0x0904, // MOVE.L CurrentA5,D0
        0x3238, 0x0934, // MOVE.W CurJTOffset,D1
        0xa9f4, // ExitToShell
    ]);
    let mut app = LoadedApplication::code(&fork).unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(A5_BASE - 256)).unwrap(),
        0x1234
    );
    let Cpu::M68k(cpu) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(cpu.dar[9], A5_BASE - 256);
    assert_eq!(cpu.dar[0], A5_BASE);
    assert_eq!(cpu.dar[1] as u16, 32);
}

#[test]
fn classic_code_relocates_operands_and_executes_patched_instructions() {
    let fork = code_program(&[
        0x2038, 0x0904, // MOVE.L CurrentA5,D0
        0xd1b9, 0x0100, 0x001c, // ADD.L D0,$0100001C (embedded pointer)
        0x33fc, 0x702a, 0x0100, 0x0018, // MOVE.W #$702A,$01000018
        0x4ef9, 0x0100, 0x0018, // JMP to the patched instruction
        0x7000, // originally MOVEQ #0,D0, now MOVEQ #42,D0
        0xa9f4, // ExitToShell
        0x0000, 0x0028, // relocation addend: jump-table offset
    ]);
    let mut app = LoadedApplication::code(&fork).unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(0x0100001c)).unwrap(),
        A5_BASE + 40
    );
    let Cpu::M68k(cpu) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(cpu.dar[0], 42);
    // Permission compatibility does not extend mappings beyond their bounds.
    assert!(matches!(
        app.runtime.memory.write(GuestPtr(0x01000020), &[1]),
        Err(Error::Memory {
            access: Access::Write,
            ..
        })
    ));
}
