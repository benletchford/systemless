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
