mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    load::resource::ResourceFork,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::{Process, ResourceInfo},
    runtime::Stop,
};
fn loaded(name: Option<Vec<u8>>) -> (Process, Memory, u32) {
    let bytes = support::resource(vec![(*b"TEST", -123, name, vec![1, 2, 3])]);
    let mut p = Process::default();
    p.install_application_resources(ResourceFork::parse(&bytes).unwrap());
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0xaa; 512], Permissions::RW)
        .unwrap();
    let h = p.get_resource(&mut m, *b"TEST", -123).unwrap();
    (p, m, h)
}
fn request(handle: u32) -> ResourceInfo {
    ResourceInfo {
        handle,
        id: GuestPtr(0x1001),
        kind: GuestPtr(0x1005),
        name: GuestPtr(0x1010),
    }
}
#[test]
fn metadata_comes_from_map_after_payload_edits_and_relocation_without_allocating() {
    let name: Vec<u8> = (0..255).map(|v| v as u8).collect();
    let (mut p, mut m, h) = loaded(Some(name.clone()));
    let data = m.u32(GuestPtr(h)).unwrap();
    m.write(GuestPtr(data), &[0xff; 3]).unwrap();
    let held = p.new_pointer(&mut m, 64, false).unwrap();
    p.set_handle_size(&mut m, h, 512).unwrap();
    assert_ne!(m.u32(GuestPtr(h)).unwrap(), data);
    p.mem_error = -108;
    p.res_error = -192;
    p.resource_info(&mut m, request(h)).unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(p.res_error, 0);
    assert_eq!(m.u16(GuestPtr(0x1001)).unwrap(), (-123i16) as u16);
    assert_eq!(m.read(GuestPtr(0x1005), 4).unwrap(), b"TEST");
    assert_eq!(m.read(GuestPtr(0x1010), 1).unwrap(), [255]);
    assert_eq!(m.read(GuestPtr(0x1011), 255).unwrap(), name);
    assert_eq!(m.read(GuestPtr(0x1000), 1).unwrap(), [0xaa]);
    assert_eq!(m.read(GuestPtr(0x1110), 1).unwrap(), [0xaa]);
    p.dispose_pointer(held);
    p.set_handle_lock(&m, h, true).unwrap();
    p.resource_info(&mut m, request(h)).unwrap();
    // Exhaust remaining heap without requiring any allocation for a metadata query.
    let mut allocations = Vec::new();
    for size in [1024 * 1024, 4096, 4] {
        loop {
            let v = p.new_pointer(&mut m, size, false).unwrap();
            if v == 0 {
                break;
            }
            allocations.push(v);
        }
    }
    assert_eq!(p.mem_error, -108);
    p.resource_info(&mut m, request(h)).unwrap();
    assert_eq!(p.res_error, 0);
    assert_eq!(p.mem_error, -108);
    for v in allocations {
        p.dispose_pointer(v);
    }
}
#[test]
fn metadata_outputs_are_preflighted_and_overlapping_writes_use_snapshots() {
    let (mut p, mut m, h) = loaded(Some(b"Name".to_vec()));
    m.map(GuestPtr(0x2000), vec![0xbb; 32], Permissions::RX)
        .unwrap();
    for bad in [0x2000, u32::MAX, 0x11fe] {
        let before = m.read(GuestPtr(0x1000), 512).unwrap().to_vec();
        let mut r = request(h);
        r.name = GuestPtr(bad);
        p.mem_error = -111;
        p.res_error = -198;
        assert!(p.resource_info(&mut m, r).is_err());
        assert_eq!(m.read(GuestPtr(0x1000), 512).unwrap(), before);
        assert_eq!(p.mem_error, -111);
        assert_eq!(p.res_error, -198);
    }
    let r = ResourceInfo {
        handle: h,
        id: GuestPtr(0x1001),
        kind: GuestPtr(0x1001),
        name: GuestPtr(0x1001),
    };
    p.resource_info(&mut m, r).unwrap();
    assert_eq!(m.read(GuestPtr(0x1001), 5).unwrap(), b"\x04Name");
    p.resource_info(
        &mut m,
        ResourceInfo {
            handle: h,
            id: GuestPtr(0),
            kind: GuestPtr(0),
            name: GuestPtr(0),
        },
    )
    .unwrap();
    assert_eq!(p.res_error, 0);
    let data = m.u32(GuestPtr(h)).unwrap();
    m.write(GuestPtr(h), &0u32.to_be_bytes()).unwrap();
    let before = m.read(GuestPtr(0x1000), 512).unwrap().to_vec();
    assert!(p.resource_info(&mut m, request(h)).is_err());
    assert_eq!(m.read(GuestPtr(0x1000), 512).unwrap(), before);
    m.write(GuestPtr(h), &data.to_be_bytes()).unwrap();
}
#[test]
fn released_detached_unknown_handles_leave_outputs_untouched_and_fallbacks_are_unnamed() {
    let (mut p, mut m, h) = loaded(None);
    let ordinary = p.new_handle(&mut m, 8, false).unwrap();
    p.detach_resource(&m, h).unwrap();
    for handle in [0, 0xdeadbeef, h, ordinary] {
        let mut r = request(handle);
        r.name = GuestPtr(u32::MAX);
        let before = m.read(GuestPtr(0x1000), 512).unwrap().to_vec();
        p.resource_info(&mut m, r).unwrap();
        assert_eq!(p.res_error, -192);
        assert_eq!(m.read(GuestPtr(0x1000), 512).unwrap(), before);
    }
    let reloaded = p.get_resource(&mut m, *b"TEST", -123).unwrap();
    p.release_resource(&mut m, reloaded).unwrap();
    p.resource_info(&mut m, request(reloaded)).unwrap();
    assert_eq!(p.res_error, -192);
    let watch = p.get_resource(&mut m, *b"CURS", 4).unwrap();
    p.resource_info(&mut m, request(watch)).unwrap();
    assert_eq!(p.res_error, 0);
    assert_eq!(m.u16(GuestPtr(0x1001)).unwrap(), 4);
    assert_eq!(m.read(GuestPtr(0x1005), 4).unwrap(), b"CURS");
    assert_eq!(m.read(GuestPtr(0x1010), 2).unwrap(), [0, 0xaa]);
}
fn invoke(app: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        panic!()
    };
    for (i, &v) in args.iter().enumerate() {
        c.gpr[i + 3] = v;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
#[test]
fn both_real_cpu_adapters_share_metadata_void_results_error_globals_and_trap_availability() {
    let f = support::pef_program(&[0x4e800020], &[b"GetResource", b"GetResInfo"], 0);
    let rsrc = support::resource(vec![(*b"TEST", -123, Some(vec![0x80, 0xff, 0]), vec![42])]);
    let mut app = LoadedApplication::pef(&f.bytes, Some(&rsrc)).unwrap();
    let h = invoke(
        &mut app,
        0,
        &[u32::from_be_bytes(*b"TEST"), (-123i32) as u32],
    );
    assert_ne!(h, 0);
    let r = ResourceInfo {
        handle: h,
        id: GuestPtr(0x300),
        kind: GuestPtr(0x304),
        name: GuestPtr(0x310),
    };
    let sp = STACK_BASE + 0x8000;
    let args: Vec<u8> = [r.name.0, r.kind.0, r.id.0, h]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect();
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), args.clone(), Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa9a8, c, &m).unwrap(),
        (Call::ResourceInfo(r), Reply::None)
    );
    assert_eq!(c.dar[15], 0x1010);
    c.dar[15] = 0x1001;
    assert!(Abi::trap(0xa9a8, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1001);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xa8], Permissions::RX)
        .unwrap();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        app.runtime
            .memory
            .write(GuestPtr(0x300), &[0xaa; 32])
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
            .unwrap();
        if arch == Architecture::PowerPc {
            assert_eq!(invoke(&mut app, 1, &[h, r.id.0, r.kind.0, r.name.0]), h);
        } else {
            let mut bytes = args.clone();
            bytes.extend([0x5a; 4]);
            app.runtime.memory.write(GuestPtr(sp), &bytes).unwrap();
            app.runtime.cpu = Cpu::new(arch, 0x50000000, sp);
            let Cpu::M68k(c) = &mut app.runtime.cpu else {
                panic!()
            };
            c.dar[0] = 0xfeedbeef;
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            let Cpu::M68k(c) = &app.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.dar[0], 0xfeedbeef);
            assert_eq!(c.dar[15], sp + 16);
            assert_eq!(
                app.runtime.memory.read(GuestPtr(sp + 16), 4).unwrap(),
                [0x5a; 4]
            );
        }
        assert_eq!(app.runtime.memory.u16(r.id).unwrap(), (-123i16) as u16);
        assert_eq!(app.runtime.memory.read(r.kind, 4).unwrap(), b"TEST");
        assert_eq!(
            app.runtime.memory.read(r.name, 5).unwrap(),
            [3, 0x80, 0xff, 0, 0xaa]
        );
        assert_eq!(
            app.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
            (-108i16) as u16
        );
        assert_eq!(app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
    }
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    for (i, v) in [h, r.id.0, r.kind.0, r.name.0].into_iter().enumerate() {
        c.gpr[3 + i] = v;
    }
    assert_eq!(
        Abi::import(Service::GetResInfo, c, &m).unwrap(),
        (Call::ResourceInfo(r), Reply::None)
    );
    let entry = systemless_future::traps::lookup(&app.runtime.memory, 0xa9a8, true).unwrap();
    assert_ne!(entry, systemless_future::traps::UNIMPLEMENTED);
    assert_eq!(systemless_future::traps::opcode(entry), Some(0xa9a8));
}
