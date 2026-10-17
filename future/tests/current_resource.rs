mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{resource::ResourceFork, LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"Get1Resource", b"GetResource"], 32);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime.process.install_application_resources(
        ResourceFork::parse(&support::resource(vec![
            (*b"DATA", -123, None, vec![1, 2, 3]),
            (*b"CURS", 2, None, vec![0; 68]),
        ]))
        .unwrap(),
    );
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa8, 0x1f, 0xa9, 0xa0],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn get(
    a: &mut LoadedApplication,
    arch: Architecture,
    current: bool,
    kind: [u8; 4],
    id: i16,
) -> u32 {
    let stack = STACK_BASE + 0x8000;
    let index = u32::from(!current);
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-77i16).to_be_bytes())
        .unwrap();
    if arch == Architecture::M68k {
        let mut frame = id.to_be_bytes().to_vec();
        frame.extend(kind);
        frame.extend([0xa5; 8]);
        a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x50000000 + index * 2, stack);
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, stack);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        c.gpr[3] = u32::from_be_bytes(kind);
        c.gpr[4] = 0xabcd0000 | u32::from(id as u16);
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16,
        a.runtime.process.res_error
    );
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], stack + 6);
            assert_eq!(
                a.runtime.memory.u32(GuestPtr(stack + 10)).unwrap(),
                0xa5a5a5a5
            );
            a.runtime.memory.u32(GuestPtr(stack + 6)).unwrap()
        }
        Cpu::PowerPc(c) => c.gpr[3],
    }
}
#[test]
fn both_cpu_scopes_share_application_identity_but_current_lookup_excludes_cached_fallbacks() {
    let mut a = app();
    let h = get(&mut a, Architecture::M68k, true, *b"DATA", -123);
    assert_ne!(h, 0);
    let free = a.runtime.process.free_memory();
    assert_eq!(get(&mut a, Architecture::PowerPc, false, *b"DATA", -123), h);
    assert_eq!(get(&mut a, Architecture::PowerPc, true, *b"DATA", -123), h);
    assert_eq!(a.runtime.process.free_memory(), free);
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(data), &[9]).unwrap();
    assert_eq!(get(&mut a, Architecture::M68k, true, *b"DATA", -123), h);
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let fallback = get(&mut a, arch, false, *b"CURS", 1);
        assert_ne!(fallback, 0);
        let free = a.runtime.process.free_memory();
        assert_eq!(get(&mut a, arch, true, *b"CURS", 1), 0);
        assert_eq!(a.runtime.process.res_error, -192);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(get(&mut a, arch, false, *b"CURS", 1), fallback);
        assert_eq!(get(&mut a, arch, true, *b"NONE", 1), 0);
        assert_eq!(a.runtime.process.res_error, 0);
    }
    a.runtime
        .process
        .release_resource(&mut a.runtime.memory, h)
        .unwrap();
    let reloaded = get(&mut a, Architecture::PowerPc, true, *b"DATA", -123);
    let data = a.runtime.memory.u32(GuestPtr(reloaded)).unwrap();
    assert_eq!(a.runtime.memory.read(GuestPtr(data), 3).unwrap(), [1, 2, 3]);
}
#[test]
fn a_fallback_type_does_not_exist_in_the_current_application_map() {
    let mut a = app();
    a.runtime.process.install_application_resources(
        ResourceFork::parse(&support::resource(vec![(*b"DATA", 1, None, vec![7])])).unwrap(),
    );
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert_ne!(get(&mut a, arch, false, *b"CURS", 1), 0);
        let free = a.runtime.process.free_memory();
        assert_eq!(get(&mut a, arch, true, *b"CURS", 1), 0);
        assert_eq!(a.runtime.process.res_error, 0);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(get(&mut a, arch, true, *b"DATA", 2), 0);
        assert_eq!(a.runtime.process.res_error, -192);
    }
}
#[test]
fn native_resource_frames_validate_output_storage_before_consuming_arguments() {
    for (opcode, size) in [(0xa81f, 10), (0xa9a0, 10), (0xa820, 12), (0xa9a1, 12)] {
        for (length, permissions) in [(size - 1, Permissions::RW), (size, Permissions::RX)] {
            let mut m = Memory::default();
            m.map(GuestPtr(0x1000), vec![0; length], permissions)
                .unwrap();
            let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
            let Cpu::M68k(c) = &mut cpu else {
                unreachable!()
            };
            assert!(Abi::trap(opcode, c, &m).is_err());
            assert_eq!(c.dar[15], 0x1000);
        }
    }
}
