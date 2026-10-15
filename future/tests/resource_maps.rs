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
            let mut b = vec![1, 0x7f, 0, 0];
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
        c.gpr[5] = 1;
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
