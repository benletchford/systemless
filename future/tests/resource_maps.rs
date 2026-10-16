mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::{Fork, Op},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const NAME: GuestPtr = GuestPtr(0x50000000);
const PB: GuestPtr = GuestPtr(0x50000100);
const BUFFER: GuestPtr = GuestPtr(0x50000200);
fn app() -> LoadedApplication {
    let p = support::pef_program(
        &[0x4e800020],
        &[
            b"OpenRFPerm",
            b"UseResFile",
            b"CloseResFile",
            b"HomeResFile",
            b"FSpOpenResFile",
            b"CountResources",
            b"Count1Resources",
            b"GetIndResource",
            b"Get1IndResource",
        ],
        32,
    );
    let mut a = LoadedApplication::pef(&p.bytes, Some(&support::code())).unwrap();
    a.runtime
        .memory
        .map(NAME, vec![0; 4096], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xa9, 0xc4, 0xa9, 0x98, 0xa9, 0x9a, 0xa9, 0xa4],
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51001000),
            vec![0xa9, 0x9d, 0xa8, 0x0e],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn name(a: &mut LoadedApplication, s: &[u8]) {
    let mut p = vec![s.len() as u8];
    p.extend(s);
    a.runtime.memory.write(NAME, &p).unwrap();
}
fn pb(a: &mut LoadedApplication, reference: i16) {
    let mut b = vec![0; 80];
    b[18..22].copy_from_slice(&NAME.0.to_be_bytes());
    b[24..26].copy_from_slice(&reference.to_be_bytes());
    b[27] = 3;
    b[32..36].copy_from_slice(&BUFFER.0.to_be_bytes());
    a.runtime.memory.write(PB, &b).unwrap();
}
fn io(a: &mut LoadedApplication, op: Op) -> i16 {
    a.runtime
        .process
        .files
        .io(&mut a.runtime.memory, PB, op)
        .unwrap()
}
fn open(a: &mut LoadedApplication, kind: Fork) -> i16 {
    pb(a, 0);
    assert_eq!(
        io(
            a,
            Op::Open {
                kind,
                hierarchical: false
            }
        ),
        0
    );
    a.runtime.memory.u16(GuestPtr(PB.0 + 24)).unwrap() as i16
}
fn create(a: &mut LoadedApplication) -> i16 {
    a.runtime
        .process
        .files
        .create_resource_file(&a.runtime.memory, NAME)
        .unwrap()
}

fn file(a: &mut LoadedApplication, n: &[u8], bytes: &[u8]) {
    name(a, n);
    assert_eq!(create(a), 0);
    let reference = open(a, Fork::Resource);
    pb(a, reference);
    a.runtime.memory.write(BUFFER, bytes).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 36), &(bytes.len() as u32).to_be_bytes())
        .unwrap();
    assert_eq!(io(a, Op::Write), 0);
    pb(a, reference);
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 28), &(bytes.len() as u32).to_be_bytes())
        .unwrap();
    assert_eq!(io(a, Op::SetEof), 0);
    pb(a, reference);
    assert_eq!(io(a, Op::Close), 0);
}
fn call(a: &mut LoadedApplication, arch: Architecture, index: u32, arg: u32) -> i16 {
    let stack = STACK_BASE + 0x8000;
    if arch == Architecture::M68k {
        let frame = if index == 0 {
            let mut b = vec![0, 0x7f, 0, 0];
            b.extend(NAME.0.to_be_bytes());
            b.extend([0xa5; 2]);
            b
        } else if index == 3 {
            let mut b = arg.to_be_bytes().to_vec();
            b.extend([0xa5; 2]);
            b
        } else {
            (arg as u16).to_be_bytes().to_vec()
        };
        a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x51000000 + 2 * index, stack);
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, stack);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.gpr[3] = if index == 0 { NAME.0 } else { arg };
        c.gpr[4] = 0;
        c.gpr[5] = 0;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16,
        a.runtime.process.res_error
    );
    match &a.runtime.cpu {
        Cpu::PowerPc(c) => c.gpr[3] as i16,
        Cpu::M68k(c) => {
            let consumed = match index {
                0 => 8,
                3 => 4,
                _ => 2,
            };
            assert_eq!(c.dar[15], stack + consumed);
            if index == 0 || index == 3 {
                a.runtime.memory.u16(GuestPtr(stack + consumed)).unwrap() as i16
            } else {
                0
            }
        }
    }
}
fn get(a: &mut LoadedApplication, id: i16) -> u32 {
    a.runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"DATA", id)
        .unwrap()
}
#[test]
fn both_cpus_open_select_search_and_close_real_maps_without_aliasing_resource_ids() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        file(
            &mut a,
            b"A",
            &support::resource(vec![
                (*b"DATA", 7, Some(b"Lower".to_vec()), vec![11]),
                (*b"DATA", 8, None, vec![18]),
            ]),
        );
        file(
            &mut a,
            b"B",
            &support::resource(vec![(*b"DATA", 7, Some(b"Upper".to_vec()), vec![22])]),
        );
        let original = a.runtime.process.current_resource_file();
        name(&mut a, b"A");
        let ra = call(&mut a, arch, 0, 0);
        assert!(ra > 0);
        let ha = get(&mut a, 7);
        assert_ne!(ha, 0);
        name(&mut a, b"B");
        let rb = call(&mut a, arch, 0, 0);
        assert!(rb > 0);
        assert_ne!(ra, rb);
        let hb = get(&mut a, 7);
        assert_ne!(ha, hb);
        assert_eq!(call(&mut a, arch, 3, ha), ra);
        assert_eq!(call(&mut a, arch, 3, hb), rb);
        assert_ne!(get(&mut a, 8), 0);
        let pname = GuestPtr(NAME.0 + 100);
        a.runtime.memory.write(pname, b"\x05Lower").unwrap();
        assert_eq!(
            a.runtime
                .process
                .get_named_resource(&mut a.runtime.memory, *b"DATA", pname, false)
                .unwrap(),
            ha
        );
        call(&mut a, arch, 1, ra as u32);
        assert_eq!(get(&mut a, 7), ha);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa5a)).unwrap() as i16, ra);
        name(&mut a, b"B");
        assert_eq!(call(&mut a, arch, 0, 0), rb);
        assert_eq!(a.runtime.process.current_resource_file(), ra);
        call(&mut a, arch, 1, 0x7fff);
        assert_eq!(a.runtime.process.res_error, -193);
        assert_eq!(a.runtime.process.current_resource_file(), ra);
        a.runtime
            .process
            .detach_resource(&a.runtime.memory, ha)
            .unwrap();
        call(&mut a, arch, 2, ra as u32);
        assert_eq!(a.runtime.process.current_resource_file(), rb);
        assert_eq!(call(&mut a, arch, 3, ha), -1);
        assert_eq!(a.runtime.process.res_error, -192);
        assert_eq!(
            a.runtime
                .process
                .handle_size(&a.runtime.memory, ha)
                .unwrap(),
            1
        );
        assert_eq!(get(&mut a, 7), hb);
        pb(&mut a, rb);
        assert_eq!(io(&mut a, Op::Close), -47);
        call(&mut a, arch, 2, rb as u32);
        assert_eq!(a.runtime.process.current_resource_file(), original);
        call(&mut a, arch, 2, rb as u32);
        assert_eq!(a.runtime.process.res_error, -193);
        call(&mut a, arch, 1, 0);
        assert_ne!(
            a.runtime
                .process
                .get_current_resource(&mut a.runtime.memory, *b"CURS", 1)
                .unwrap(),
            0
        );
    }
}
#[test]
fn malformed_missing_and_empty_forks_preserve_selection_and_fail_with_resource_errors() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        file(&mut a, b"Bad", &[0; 32]);
        file(&mut a, b"Short", &[0; 2]);
        let current = a.runtime.process.current_resource_file();
        for (n, e) in [
            (b"Bad".as_slice(), -199),
            (b"Short".as_slice(), -39),
            (b"Missing".as_slice(), -43),
        ] {
            name(&mut a, n);
            assert_eq!(call(&mut a, arch, 0, 0), -1);
            assert_eq!(a.runtime.process.res_error, e);
            assert_eq!(a.runtime.process.current_resource_file(), current);
        }
    }
}

#[test]
fn native_resource_file_results_are_checked_before_consuming_arguments() {
    use systemless_future::{abi::Abi, memory::Memory};
    for (opcode, args) in [(0xa997, 4), (0xa9c4, 8), (0xa9a4, 4)] {
        for (length, permissions) in [(args + 1, Permissions::RW), (args + 2, Permissions::RX)] {
            let mut memory = Memory::default();
            let mut frame = vec![0; length];
            if opcode != 0xa9a4 {
                let offset = if opcode == 0xa9c4 { 4 } else { 0 };
                frame[offset..offset + 4].copy_from_slice(&0x2000u32.to_be_bytes());
            }
            memory.map(GuestPtr(0x1000), frame, permissions).unwrap();
            memory
                .map(GuestPtr(0x2000), vec![1, b'A'], Permissions::RW)
                .unwrap();
            let mut c = m68k::CpuCore::default();
            c.dar[15] = 0x1000;
            assert!(Abi::trap(opcode, &mut c, &memory).is_err());
            assert_eq!(c.dar[15], 0x1000);
        }
    }
}

#[test]
fn preload_failure_rolls_back_handles_file_access_and_selection() {
    use systemless_future::process::resources::Op as ResourceOp;
    let mut a = app();
    let mut bytes = support::resource(vec![
        (*b"DATA", 7, None, vec![11]),
        (*b"DATA", 8, None, vec![22]),
    ]);
    let map = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let types = map + u16::from_be_bytes(bytes[map + 24..map + 26].try_into().unwrap()) as usize;
    let refs =
        types + u16::from_be_bytes(bytes[types + 8..types + 10].try_into().unwrap()) as usize;
    bytes[refs + 4] = 4;
    bytes[refs + 12 + 4] = 5;
    file(&mut a, b"Preload", &bytes);
    name(&mut a, b"Preload");
    let current = a.runtime.process.current_resource_file();
    let free = a.runtime.process.free_memory();
    assert!(a
        .runtime
        .process
        .resource_file(
            &mut a.runtime.memory,
            ResourceOp::Open {
                name: NAME,
                volume: 0,
                permission: 1
            }
        )
        .is_err());
    assert_eq!(a.runtime.process.current_resource_file(), current);
    assert_eq!(a.runtime.process.free_memory(), free);
    // Failed preload must release its access path: an exclusive raw writer can reopen.
    pb(&mut a, 0);
    assert_eq!(
        io(
            &mut a,
            Op::Open {
                kind: Fork::Resource,
                hierarchical: false
            }
        ),
        0
    );
}

fn open_spec(a: &mut LoadedApplication, arch: Architecture, leaf: &[u8], permission: i8) -> i16 {
    let spec = GuestPtr(NAME.0 + 3000);
    let stack = STACK_BASE + 0x8000;
    let mut bytes = [0; 70];
    bytes[..2].copy_from_slice(&(-1i16).to_be_bytes());
    bytes[2..6].copy_from_slice(&2u32.to_be_bytes());
    bytes[6] = leaf.len() as u8;
    bytes[7..7 + leaf.len()].copy_from_slice(leaf);
    a.runtime.memory.write(spec, &bytes).unwrap();
    let mut frame = vec![permission as u8, 0x5a];
    frame.extend(spec.0.to_be_bytes());
    frame.extend([0x5a; 6]);
    a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    let gate = if arch == Architecture::M68k {
        0x51000008
    } else {
        let v = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 24))
            .unwrap();
        a.runtime.memory.u32(GuestPtr(v)).unwrap()
    };
    a.runtime.cpu = Cpu::new(arch, gate, stack);
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => {
            c.dar[0] = 0x1234000d;
            c.dar[1] = 0xaabbccdd;
        }
        Cpu::PowerPc(c) => {
            c.gpr[3] = spec.0;
            c.gpr[4] = 0xabcd0000 | u32::from(permission as u8);
            c.gpr[5] = 0xaabbccdd;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let result = match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(
                (c.dar[0], c.dar[1], c.dar[15]),
                (0x1234000d, 0xaabbccdd, stack + 6)
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(stack), 6).unwrap(),
                &frame[..6]
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(stack + 8), 4).unwrap(),
                [0x5a; 4]
            );
            a.runtime.memory.u16(GuestPtr(stack + 6)).unwrap() as i16
        }
        Cpu::PowerPc(c) => {
            assert_eq!(
                (c.gpr[4], c.gpr[5]),
                (0xabcd0000 | u32::from(permission as u8), 0xaabbccdd)
            );
            c.gpr[3] as i16
        }
    };
    assert_eq!(a.runtime.memory.read(spec, 70).unwrap(), bytes);
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16,
        a.runtime.process.res_error
    );
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(0xa5a)).unwrap() as i16,
        a.runtime.process.current_resource_file()
    );
    result
}
#[test]
fn both_spec_adapters_share_maps_permissions_reopen_selection_and_file_scoped_handles() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        a.runtime
            .memory
            .map(GuestPtr(0x51000008), vec![0xaa, 0x52], Permissions::RX)
            .unwrap();
        file(
            &mut a,
            b"A",
            &support::resource(vec![(*b"DATA", 7, None, vec![11])]),
        );
        file(
            &mut a,
            b"B",
            &support::resource(vec![(*b"DATA", 7, None, vec![22])]),
        );
        let original = a.runtime.process.current_resource_file();
        let ra = open_spec(&mut a, arch, b"A", 3);
        assert!(ra > 0);
        let ha = get(&mut a, 7);
        let rb = open_spec(&mut a, arch, b"B", 3);
        assert!(rb > 0);
        assert_ne!(ra, rb);
        assert_eq!(a.runtime.process.current_resource_file(), rb);
        assert_eq!(open_spec(&mut a, arch, b"A", 3), ra);
        assert_eq!(a.runtime.process.current_resource_file(), rb);
        let readonly = open_spec(&mut a, arch, b"A", 1);
        assert!(readonly > 0);
        assert_ne!(readonly, ra);
        assert_eq!(a.runtime.process.current_resource_file(), rb);
        call(&mut a, arch, 1, readonly as u32);
        let separate = get(&mut a, 7);
        assert_ne!(separate, ha);
        let data = a.runtime.memory.u32(GuestPtr(separate)).unwrap();
        assert_eq!(a.runtime.memory.read(GuestPtr(data), 1).unwrap(), [11]);
        for (leaf, permission, error) in
            [(b"Missing".as_slice(), 1, -43), (b"A".as_slice(), -1, -50)]
        {
            let current = a.runtime.process.current_resource_file();
            assert_eq!(open_spec(&mut a, arch, leaf, permission), -1);
            assert_eq!(a.runtime.process.res_error, error);
            assert_eq!(a.runtime.process.current_resource_file(), current);
        }
        call(&mut a, arch, 2, readonly as u32);
        call(&mut a, arch, 2, rb as u32);
        call(&mut a, arch, 2, ra as u32);
        assert_eq!(a.runtime.process.current_resource_file(), original);
    }
}
#[test]
fn spec_open_native_preflight_preserves_frames_and_globals_on_fault() {
    use systemless_future::{abi::Abi, memory::Memory};
    for which in 0..4 {
        let mut m = Memory::default();
        m.map(
            GuestPtr(0x1000),
            vec![0; if which == 0 { 7 } else { 12 }],
            Permissions::RW,
        )
        .unwrap();
        m.map(
            GuestPtr(0x2000),
            vec![0; if which == 1 { 69 } else { 70 }],
            Permissions::RX,
        )
        .unwrap();
        m.map(
            GuestPtr(0xa5a),
            vec![0x5a; 2],
            if which == 2 {
                Permissions::RX
            } else {
                Permissions::RW
            },
        )
        .unwrap();
        m.map(
            GuestPtr(0xa60),
            vec![0x5a; 2],
            if which == 3 {
                Permissions::RX
            } else {
                Permissions::RW
            },
        )
        .unwrap();
        let mut frame = vec![3, 0x7f];
        frame.extend(0x2000u32.to_be_bytes());
        m.write(GuestPtr(0x1000), &frame).unwrap();
        let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
            unreachable!()
        };
        c.dar[0] = 13;
        assert!(Abi::trap(0xaa52, &mut c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
        assert_eq!(m.read(GuestPtr(0x1000), 6).unwrap(), frame);
        assert_eq!(m.read(GuestPtr(0xa5a), 2).unwrap(), [0x5a; 2]);
        assert_eq!(m.read(GuestPtr(0xa60), 2).unwrap(), [0x5a; 2]);
    }
}

fn count(a: &mut LoadedApplication, arch: Architecture, current: bool, kind: [u8; 4]) -> u16 {
    let stack = STACK_BASE + 0x8000;
    a.runtime.process.res_error = -192;
    a.runtime.process.mem_error = -108;
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    if arch == Architecture::M68k {
        let mut frame = kind.to_vec();
        frame.extend([0xa5; 6]);
        a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x52000000 + u32::from(current) * 2, stack);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        for i in 0..15 {
            c.dar[i] = 0x12340000 + i as u32;
        }
    } else {
        let index = if current { 6 } else { 5 };
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, stack);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.gpr[3] = u32::from_be_bytes(kind);
        c.gpr[4] = 0x76543210;
    }
    let free = a.runtime.process.free_memory();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(a.runtime.process.res_error, 0);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -108);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], stack + 4);
            for i in 0..15 {
                assert_eq!(c.dar[i], 0x12340000 + i as u32);
            }
            assert_eq!(
                a.runtime.memory.read(GuestPtr(stack + 6), 4).unwrap(),
                [0xa5; 4]
            );
            a.runtime.memory.u16(GuestPtr(stack + 4)).unwrap()
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[1], stack);
            assert_eq!(c.gpr[4], 0x76543210);
            c.gpr[3] as u16
        }
    }
}

#[test]
fn count_all_open_maps_ignores_lookup_position_and_counts_duplicate_ids_per_file() {
    let mut a = app();
    a.runtime
        .memory
        .map(
            GuestPtr(0x52000000),
            vec![0xa9, 0x9c, 0xa8, 0x0d],
            Permissions::RX,
        )
        .unwrap();
    a.runtime.process.install_application_resources(
        systemless_future::load::resource::ResourceFork::parse(&support::resource(vec![
            (*b"DATA", 7, None, vec![1]),
            (*b"DATA", 8, None, vec![2]),
        ]))
        .unwrap(),
    );
    file(
        &mut a,
        b"A",
        &support::resource(vec![(*b"DATA", 7, None, vec![3])]),
    );
    file(
        &mut a,
        b"B",
        &support::resource(vec![
            (*b"DATA", 7, None, vec![4]),
            (*b"STR ", 1, None, vec![]),
        ]),
    );
    name(&mut a, b"A");
    let lower = call(&mut a, Architecture::M68k, 0, 0);
    name(&mut a, b"B");
    let mut upper = call(&mut a, Architecture::PowerPc, 0, 0);
    assert!(lower > 0 && upper > 0);
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        call(&mut a, arch, 1, lower as u16 as u32);
        assert_eq!(count(&mut a, arch, false, *b"DATA"), 4);
        assert_eq!(count(&mut a, arch, true, *b"DATA"), 1);
        assert_eq!(count(&mut a, arch, false, *b"STR "), 1);
        assert_eq!(count(&mut a, arch, true, *b"STR "), 0);
        assert_eq!(count(&mut a, arch, false, *b"NONE"), 0);
        call(&mut a, arch, 2, upper as u16 as u32);
        assert_eq!(count(&mut a, arch, false, *b"DATA"), 3);
        // Reopen for the next architecture, keeping the same process maps.
        name(&mut a, b"B");
        upper = call(&mut a, arch, 0, 0);
        assert!(upper > 0);
    }
}

#[test]
fn count_frames_validate_the_word_result_before_consuming_type() {
    use systemless_future::{abi::Abi, memory::Memory};
    for opcode in [0xa99c, 0xa80d] {
        for (length, permissions) in [(5, Permissions::RW), (6, Permissions::RX)] {
            let mut memory = Memory::default();
            memory
                .map(GuestPtr(0x1000), vec![0xa5; length], permissions)
                .unwrap();
            let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
            let Cpu::M68k(c) = &mut cpu else { panic!() };
            assert!(Abi::trap(opcode, c, &memory).is_err());
            assert_eq!(c.dar[15], 0x1000);
            assert_eq!(
                memory.read(GuestPtr(0x1000), length).unwrap(),
                vec![0xa5; length]
            );
        }
    }
}

fn indexed(
    a: &mut LoadedApplication,
    arch: Architecture,
    kind: [u8; 4],
    index: i16,
    current_only: bool,
) -> u32 {
    let stack = STACK_BASE + 0x8000;
    a.runtime.process.mem_error = -99;
    if arch == Architecture::M68k {
        let mut frame = index.to_be_bytes().to_vec();
        frame.extend(kind);
        frame.extend([0xa5; 8]);
        a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x51001000 + u32::from(current_only) * 2, stack);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        c.dar[0] = 0x12345678;
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(
                a.info.toc_or_a5 + 8 + (7 + u32::from(current_only)) * 4,
            ))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, stack);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        c.gpr[3] = u32::from_be_bytes(kind);
        c.gpr[4] = 0xa5a50000 | u32::from(index as u16);
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16,
        a.runtime.process.res_error
    );
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16,
        a.runtime.process.mem_error
    );
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], stack + 6);
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(stack + 10), 4).unwrap(),
                [0xa5; 4]
            );
            a.runtime.memory.u32(GuestPtr(stack + 6)).unwrap()
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[4], 0xa5a50000 | u32::from(index as u16));
            c.gpr[3]
        }
    }
}
fn payload(a: &LoadedApplication, h: u32) -> u8 {
    let p = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.read(GuestPtr(p), 1).unwrap()[0]
}
#[test]
fn indexed_resources_use_all_open_files_and_reference_order_independent_of_use_res_file() {
    use systemless_future::load::resource::ResourceFork;
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let raw = support::resource(vec![
            (*b"DATA", 90, None, vec![90]),
            (*b"DATA", -7, None, vec![77]),
            (*b"DATA", 15, None, vec![15]),
        ]);
        a.runtime
            .process
            .install_application_resources(ResourceFork::parse(&raw).unwrap());
        let original = a.runtime.process.current_resource_file();
        file(
            &mut a,
            b"A",
            &support::resource(vec![
                (*b"DATA", 30, None, vec![30]),
                (*b"DATA", 9, None, vec![19]),
            ]),
        );
        file(
            &mut a,
            b"B",
            &support::resource(vec![
                (*b"DATA", 9, None, vec![29]),
                (*b"DATA", 100, None, vec![100]),
            ]),
        );
        name(&mut a, b"A");
        let ra = call(&mut a, arch, 0, 0);
        name(&mut a, b"B");
        let rb = call(&mut a, arch, 0, 0);
        call(&mut a, arch, 1, ra as u32);
        let mut first = 0;
        for (i, (value, owner)) in [
            (29, rb),
            (100, rb),
            (30, ra),
            (19, ra),
            (90, original),
            (77, original),
            (15, original),
        ]
        .into_iter()
        .enumerate()
        {
            let h = indexed(&mut a, arch, *b"DATA", i as i16 + 1, false);
            assert_ne!(h, 0);
            assert_eq!(payload(&a, h), value);
            assert_eq!(call(&mut a, arch, 3, h), owner);
            if i == 0 {
                first = h;
            }
        }
        assert_eq!(a.runtime.process.current_resource_file(), ra);
        assert_ne!(get(&mut a, 9), first);
        for (index, wanted) in [(1, 30), (2, 19)] {
            let h = indexed(&mut a, arch, *b"DATA", index, true);
            assert_eq!(payload(&a, h), wanted);
        }
        for (kind, index, one) in [
            (*b"DATA", 0, false),
            (*b"DATA", -1, false),
            (*b"DATA", 8, false),
            (*b"NONE", 1, false),
            (*b"DATA", 3, true),
        ] {
            let free = a.runtime.process.free_memory();
            assert_eq!(indexed(&mut a, arch, kind, index, one), 0);
            assert_eq!(a.runtime.process.res_error, -192);
            assert_eq!(a.runtime.process.mem_error, -99);
            assert_eq!(a.runtime.process.free_memory(), free);
        }
        let ptr = a.runtime.memory.u32(GuestPtr(first)).unwrap();
        a.runtime.memory.write(GuestPtr(ptr), &[42]).unwrap();
        let other = if arch == Architecture::M68k {
            Architecture::PowerPc
        } else {
            Architecture::M68k
        };
        assert_eq!(indexed(&mut a, other, *b"DATA", 1, false), first);
        assert_eq!(payload(&a, first), 42);
        a.runtime
            .process
            .release_resource(&mut a.runtime.memory, first)
            .unwrap();
        let h = indexed(&mut a, arch, *b"DATA", 1, false);
        assert_eq!(payload(&a, h), 29);
        call(&mut a, arch, 2, rb as u32);
        let h = indexed(&mut a, arch, *b"DATA", 1, false);
        assert_eq!(payload(&a, h), 30);
        assert_eq!(
            a.runtime.process.count_resources(*b"DATA", false).unwrap(),
            5
        );
    }
}
#[test]
fn reference_list_order_survives_serialization_without_sorting_signed_ids() {
    use systemless_future::load::resource::ResourceFork;
    let raw = support::resource(vec![
        (*b"DATA", 100, None, vec![1]),
        (*b"DATA", -7, None, vec![2]),
        (*b"DATA", 2, None, vec![3]),
    ]);
    let map = ResourceFork::parse(&raw).unwrap();
    assert_eq!(
        map.indexed(*b"DATA").map(|r| r.id).collect::<Vec<_>>(),
        [100, -7, 2]
    );
    let reparsed = ResourceFork::parse(&map.encode().unwrap()).unwrap();
    assert_eq!(reparsed.attributes, map.attributes);
    assert_eq!(
        reparsed.entries().collect::<Vec<_>>(),
        map.entries().collect::<Vec<_>>()
    );
    assert_eq!(
        reparsed
            .indexed(*b"DATA")
            .map(|r| r.data[0])
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
}
