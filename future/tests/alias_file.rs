mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    files::ResolveAliasFile,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
    traps,
};
fn spec(volume: i16, parent: u32, name: &[u8]) -> [u8; 70] {
    let mut bytes = [0x5a; 70];
    bytes[..2].copy_from_slice(&volume.to_be_bytes());
    bytes[2..6].copy_from_slice(&parent.to_be_bytes());
    bytes[6] = name.len() as u8;
    bytes[7..7 + name.len()].copy_from_slice(name);
    bytes
}
#[test]
fn both_adapters_share_catalog_lookup_errors_booleans_and_unchanged_specs() {
    let f = support::pef_program(&[0x4e800020], &[b"ResolveAliasFile"], 16);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x23], Permissions::RX)
        .unwrap();
    let vector = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8))
        .unwrap();
    let gate = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
    let stack = STACK_BASE + 0x7000;
    let pointer = GuestPtr(STACK_BASE + 0x6000);
    let folder = GuestPtr(pointer.0 + 72);
    let aliased = GuestPtr(pointer.0 + 74);
    let free = a.runtime.process.free_memory();
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    for architecture in [Architecture::M68k, Architecture::PowerPc] {
        for (volume, parent, name, error, is_folder) in [
            (-1, 2, b"Application".as_slice(), 0, false),
            (-1, 2, b"aPpLiCaTiOn".as_slice(), 0, false),
            (-1, 2, b"".as_slice(), 0, true),
            (-1, 1, b"Application Volume".as_slice(), 0, true),
            (-1, 2, b"Pilots".as_slice(), -43, false),
            (7, 2, b"Application".as_slice(), -35, false),
            (-1, 999, b"Application".as_slice(), -120, false),
        ] {
            for chains in [false, true] {
                let input = spec(volume, parent, name);
                a.runtime.memory.write(pointer, &input).unwrap();
                a.runtime
                    .memory
                    .write(folder, &[0xaa, 0x55, 0xbb, 0x66])
                    .unwrap();
                let mut frame = aliased.0.to_be_bytes().to_vec();
                frame.extend(folder.0.to_be_bytes());
                frame.extend([u8::from(chains), 0x5a]);
                frame.extend(pointer.0.to_be_bytes());
                frame.extend([0x5a; 6]);
                a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
                if architecture == Architecture::M68k {
                    a.runtime.cpu = Cpu::new(architecture, 0x50000000, stack);
                    let Cpu::M68k(c) = &mut a.runtime.cpu else {
                        unreachable!()
                    };
                    c.dar[0] = 0x1234000c;
                    c.dar[1] = 0xaabbccdd;
                } else {
                    a.runtime.cpu = Cpu::new(architecture, gate, stack);
                    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
                        unreachable!()
                    };
                    c.gpr[3] = pointer.0;
                    c.gpr[4] = 0xaabbcc00 | u32::from(chains);
                    c.gpr[5] = folder.0;
                    c.gpr[6] = aliased.0;
                    c.gpr[7] = 0xaabbccdd;
                }
                assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
                match &a.runtime.cpu {
                    Cpu::M68k(c) => {
                        assert_eq!(
                            (c.dar[0], c.dar[1], c.dar[15]),
                            (0x1234000c, 0xaabbccdd, stack + 14)
                        );
                        assert_eq!(
                            a.runtime.memory.u16(GuestPtr(stack + 14)).unwrap() as i16,
                            error
                        );
                        assert_eq!(
                            a.runtime.memory.read(GuestPtr(stack), 14).unwrap(),
                            &frame[..14]
                        );
                        assert_eq!(
                            a.runtime.memory.read(GuestPtr(stack + 16), 4).unwrap(),
                            [0x5a; 4]
                        );
                    }
                    Cpu::PowerPc(c) => assert_eq!(
                        (c.gpr[3] as i32, c.gpr[5], c.gpr[6], c.gpr[7]),
                        (i32::from(error), folder.0, aliased.0, 0xaabbccdd)
                    ),
                }
                assert_eq!(a.runtime.memory.read(pointer, 70).unwrap(), input);
                assert_eq!(
                    a.runtime.memory.read(folder, 4).unwrap(),
                    if error == 0 {
                        [u8::from(is_folder), 0x55, 0, 0x66]
                    } else {
                        [0xaa, 0x55, 0xbb, 0x66]
                    }
                );
            }
        }
    }
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    let entry = traps::lookup(&a.runtime.memory, 0x23, true).unwrap();
    assert_ne!(entry, traps::UNIMPLEMENTED);
    assert_eq!(a.runtime.memory.u16(GuestPtr(entry)).unwrap(), 0xa823);
}
#[test]
fn preflight_faults_preserve_native_stack_and_all_outputs() {
    for which in 0..5 {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0x5a; 32], Permissions::RW)
            .unwrap();
        m.map(
            GuestPtr(0x2000),
            spec(-1, 2, b"Application").to_vec(),
            if which == 0 {
                Permissions::RX
            } else {
                Permissions::RW
            },
        )
        .unwrap();
        m.map(
            GuestPtr(0x3000),
            vec![0x55; 4],
            if which == 1 {
                Permissions::RX
            } else {
                Permissions::RW
            },
        )
        .unwrap();
        let request = ResolveAliasFile {
            spec: GuestPtr(if which == 2 { 0x2002 } else { 0x2000 }),
            chains: true,
            folder: GuestPtr(0x3000),
            aliased: GuestPtr(if which == 3 { 0x3004 } else { 0x3002 }),
        };
        let mut frame = request.aliased.0.to_be_bytes().to_vec();
        frame.extend(request.folder.0.to_be_bytes());
        frame.extend([1, 0]);
        frame.extend(request.spec.0.to_be_bytes());
        m.write(GuestPtr(0x1000), &frame).unwrap();
        let mut cpu = Cpu::new(
            Architecture::M68k,
            0,
            if which == 4 { 0x1014 } else { 0x1000 },
        );
        let Cpu::M68k(c) = &mut cpu else {
            unreachable!()
        };
        c.dar[0] = 12;
        let sp = c.dar[15];
        assert!(Abi::trap(0xa823, c, &m).is_err());
        assert_eq!(c.dar[15], sp);
        assert_eq!(m.read(GuestPtr(0x1000), 14).unwrap(), frame);
        assert_eq!(m.read(GuestPtr(0x3000), 4).unwrap(), [0x55; 4]);
    }
}
