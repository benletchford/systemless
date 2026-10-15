mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::Process,
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"HGetState",
            b"HSetState",
            b"HPurge",
            b"HNoPurge",
            b"HSetRBit",
            b"HClrRBit",
            b"HLock",
            b"HUnlock",
            b"MoveHHi",
        ],
        64,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            [
                0xa069u16, 0xa06a, 0xa049, 0xa04a, 0xa067, 0xa068, 0xa029, 0xa02a, 0xa064,
            ]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect(),
            Permissions::RX,
        )
        .unwrap();
    a
}
fn invoke(
    a: &mut LoadedApplication,
    arch: Architecture,
    index: u32,
    handle: u32,
    flags: u8,
) -> u32 {
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-42i16).to_be_bytes())
        .unwrap();
    if arch == Architecture::M68k {
        a.runtime.cpu = Cpu::new(arch, 0x50000000 + index * 2, stack);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[8] = handle;
        c.dar[0] = 0xaabbcc00 | u32::from(flags);
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
        c.gpr[3] = handle;
        c.gpr[4] = u32::from(flags);
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -42);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[8], handle);
            assert_eq!(c.dar[15], stack);
            c.dar[0]
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[1], stack);
            if index != 0 {
                assert_eq!(c.gpr[3], handle);
            }
            c.gpr[3]
        }
    }
}
fn state(a: &mut LoadedApplication, arch: Architecture, h: u32, expected: u8) {
    let value = invoke(a, arch, 0, h, 0x5a);
    assert_eq!(
        value,
        if arch == Architecture::M68k {
            0xaabbcc00 | u32::from(expected)
        } else {
            expected as i8 as i32 as u32
        }
    );
}
#[test]
fn actual_cpus_share_full_flags_error_publication_and_unmodified_32bit_master_addresses() {
    let mut a = app();
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 16, true)
        .unwrap();
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        invoke(&mut a, arch, 1, h, 0);
        state(&mut a, arch, h, 0);
        invoke(&mut a, arch, 2, h, 0);
        state(&mut a, arch, h, 0x40);
        invoke(&mut a, arch, 6, h, 0);
        state(&mut a, arch, h, 0xc0);
        invoke(&mut a, arch, 4, h, 0);
        state(&mut a, arch, h, 0xe0);
        invoke(&mut a, arch, 7, h, 0);
        state(&mut a, arch, h, 0x60);
        invoke(&mut a, arch, 3, h, 0);
        state(&mut a, arch, h, 0x20);
        invoke(&mut a, arch, 5, h, 0);
        state(&mut a, arch, h, 0);
        invoke(&mut a, arch, 1, h, 0xff);
        state(&mut a, arch, h, 0xff);
        invoke(&mut a, arch, 7, h, 0);
        state(&mut a, arch, h, 0x7f);
        assert_eq!(a.runtime.memory.u32(GuestPtr(h)).unwrap(), data);
        for (invalid, error) in [(0, -109i16), (0xdeadbeef, -111)] {
            state(&mut a, arch, invalid, error as i8 as u8);
            assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, error);
            invoke(&mut a, arch, 1, invalid, 0xff);
            assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, error);
        }
        a.runtime
            .memory
            .write(GuestPtr(h), &0u32.to_be_bytes())
            .unwrap();
        state(&mut a, arch, h, (-109i16) as u8);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -109);
        a.runtime
            .memory
            .write(GuestPtr(h), &data.to_be_bytes())
            .unwrap();
        state(&mut a, arch, h, 0x7f);
    }
}
#[test]
fn restoring_flags_controls_real_relocation_and_preserves_attributes_through_resize() {
    let mut a = app();
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 16, true)
        .unwrap();
    let blocker = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 16, false)
        .unwrap();
    let original = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(original), &[0x5a; 16])
        .unwrap();
    invoke(&mut a, Architecture::M68k, 1, h, 0xc7);
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, h, 64)
        .unwrap();
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.memory.u32(GuestPtr(h)).unwrap(), original);
    invoke(&mut a, Architecture::PowerPc, 1, h, 0x47);
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, h, 64)
        .unwrap();
    assert_eq!(a.runtime.process.mem_error, 0);
    let moved = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    assert_ne!(moved, original);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(moved), 16).unwrap(),
        [0x5a; 16]
    );
    state(&mut a, Architecture::M68k, h, 0x47);
    a.runtime.process.dispose_pointer(blocker);
}
#[test]
fn resource_attributes_initialize_flags_and_detach_clears_only_the_resource_bit() {
    let mut bytes = support::resource(vec![(*b"DATA", 128, None, vec![1, 2, 3])]);
    let map = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let types = map + u16::from_be_bytes(bytes[map + 24..map + 26].try_into().unwrap()) as usize;
    let reference =
        types + u16::from_be_bytes(bytes[types + 8..types + 10].try_into().unwrap()) as usize;
    bytes[reference + 4] = 0x30;
    let mut p = Process::default();
    p.install_application_resources(
        systemless_future::load::resource::ResourceFork::parse(&bytes).unwrap(),
    );
    let mut m = Memory::default();
    let h = p.get_resource(&mut m, *b"DATA", 128).unwrap();
    assert_eq!(p.handle_state(&m, h).unwrap(), 0xe0);
    p.set_handle_state(&m, h, 0xef).unwrap();
    let pointer = m.u32(GuestPtr(h)).unwrap();
    p.detach_resource(&m, h).unwrap();
    assert_eq!(p.handle_state(&m, h).unwrap(), 0xcf);
    assert_eq!(m.u32(GuestPtr(h)).unwrap(), pointer);
    let reloaded = p.get_resource(&mut m, *b"DATA", 128).unwrap();
    assert_ne!(reloaded, h);
    assert_eq!(p.handle_state(&m, reloaded).unwrap(), 0xe0);
    p.set_handle_attribute(&m, reloaded, true, false).unwrap();
    assert_eq!(p.get_resource(&mut m, *b"DATA", 128).unwrap(), reloaded);
    p.release_resource(&mut m, reloaded).unwrap();
    assert_eq!(p.handle_state(&m, reloaded).unwrap(), (-111i16) as u8);
}

#[test]
fn high_moves_preserve_flags_bytes_free_space_and_obey_real_allocation_barriers() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let h = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 16, true)
            .unwrap();
        let initial = a.runtime.memory.u32(GuestPtr(h)).unwrap();
        invoke(&mut a, arch, 8, h, 0);
        assert_eq!(a.runtime.memory.u32(GuestPtr(h)).unwrap(), initial); // own nonrelocatable master is above the payload
        a.runtime
            .process
            .set_handle_size(&mut a.runtime.memory, h, 64)
            .unwrap();
        let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
        assert!(data > initial);
        a.runtime.memory.write(GuestPtr(data), &[0x5a; 64]).unwrap();
        invoke(&mut a, arch, 1, h, 0x47);
        let before = a.runtime.process.free_memory();
        invoke(&mut a, arch, 8, h, 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0);
        let moved = a.runtime.memory.u32(GuestPtr(h)).unwrap();
        assert_eq!(moved, a.runtime.memory.u32(GuestPtr(0x114)).unwrap() - 64);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(moved), 64).unwrap(),
            [0x5a; 64]
        );
        assert_eq!(a.runtime.process.free_memory(), before);
        state(&mut a, arch, h, 0x47);
        invoke(&mut a, arch, 6, h, 0);
        invoke(&mut a, arch, 8, h, 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -117);
        assert_eq!(a.runtime.memory.u32(GuestPtr(h)).unwrap(), moved);
        invoke(&mut a, arch, 8, 0, 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -109);
    }
    // A short adjacent hole permits an overlapping move, but cannot cross a Ptr.
    let mut p = Process::default();
    let mut m = Memory::default();
    let h = p.new_handle(&mut m, 16, false).unwrap();
    p.set_handle_size(&mut m, h, 64).unwrap();
    let data = m.u32(GuestPtr(h)).unwrap();
    m.write(GuestPtr(data), &(0u8..64).collect::<Vec<_>>())
        .unwrap();
    let hole = p.new_pointer(&mut m, 8, false).unwrap();
    let barrier = p.new_pointer(&mut m, 16, false).unwrap();
    p.dispose_pointer(hole);
    p.move_handle_high(&mut m, h).unwrap();
    assert_eq!(m.u32(GuestPtr(h)).unwrap(), barrier - 64);
    assert_eq!(
        m.read(GuestPtr(barrier - 64), 64).unwrap(),
        (0u8..64).collect::<Vec<_>>()
    );
    assert_eq!(p.pointer_size(barrier), 16);
}
