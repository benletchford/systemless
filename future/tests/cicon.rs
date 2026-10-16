mod support;
use systemless_future::{
    abi::Abi,
    cicon::Op,
    cpu::{Architecture, Cpu},
    load::{resource::ResourceFork, LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
fn resource() -> Vec<u8> {
    let mut b = vec![0; 112];
    for (at, v) in [
        (4, 0x8001u16),
        (10, 2),
        (12, 2),
        (32, 4),
        (34, 1),
        (36, 4),
        (54, 1),
        (60, 2),
        (62, 2),
        (68, 1),
        (74, 2),
        (76, 2),
        (92, 1),
        (102, 1),
    ] {
        b[at..at + 2].copy_from_slice(&v.to_be_bytes());
    }
    for at in [22, 26] {
        b[at..at + 4].copy_from_slice(&0x00480000u32.to_be_bytes());
    }
    b[82..86].copy_from_slice(&[0xc0, 0x80, 0x80, 0x40]);
    b[96..102].fill(0xff);
    b[110..112].copy_from_slice(&[1, 0x10]);
    b
}
fn app(bytes: Vec<u8>) -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"GetCIcon", b"DisposeCIcon"], 24);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime.process.install_application_resources(
        ResourceFork::parse(&support::resource(vec![(*b"cicn", 128, None, bytes)])).unwrap(),
    );
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xaa, 0x1e, 0xaa, 0x25],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn invoke(a: &mut LoadedApplication, arch: Architecture, op: Op) -> u32 {
    let stack = STACK_BASE + 0x7000;
    let get = matches!(op, Op::Get(_));
    let arg = match op {
        Op::Get(id) => 0xabcd0000 | u32::from(id as u16),
        Op::Dispose(h) => h,
    };
    if arch == Architecture::M68k {
        let mut frame = if get {
            (arg as u16).to_be_bytes().to_vec()
        } else {
            arg.to_be_bytes().to_vec()
        };
        frame.extend([0xa5; 8]);
        a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x50000000 + 2 * u32::from(!get), stack);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        for i in 0..15 {
            c.dar[i] = 0x12340000 + i as u32;
        }
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + 4 * u32::from(!get)))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, stack);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        for i in 2..32 {
            c.gpr[i] = 0x12340000 + i as u32;
        }
        c.gpr[3] = arg;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16,
        a.runtime.process.mem_error
    );
    if get {
        assert_eq!(
            a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16,
            a.runtime.process.res_error
        );
    }
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            for i in 0..15 {
                assert_eq!(c.dar[i], 0x12340000 + i as u32);
            }
            assert_eq!(c.dar[15], stack + if get { 2 } else { 4 });
            if get {
                assert_eq!(
                    a.runtime.memory.read(GuestPtr(stack + 6), 4).unwrap(),
                    [0xa5; 4]
                );
                a.runtime.memory.u32(GuestPtr(stack + 2)).unwrap()
            } else {
                0
            }
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[1], stack);
            for i in 2..32 {
                if i != 3 {
                    assert_eq!(c.gpr[i], 0x12340000 + i as u32);
                }
            }
            if get {
                c.gpr[3]
            } else {
                assert_eq!(c.gpr[3], arg);
                0
            }
        }
    }
}
fn payload(a: &LoadedApplication, h: u32) -> u32 {
    a.runtime.memory.u32(GuestPtr(h)).unwrap()
}
fn field(a: &LoadedApplication, h: u32, at: u32) -> u32 {
    a.runtime.memory.u32(GuestPtr(payload(a, h) + at)).unwrap()
}
fn manager(a: &mut LoadedApplication, op: Op) -> systemless_future::Result<u32> {
    let mut m = std::mem::take(&mut a.runtime.process.cicons);
    let r = m.call(&mut a.runtime.memory, &mut a.runtime.process, op);
    a.runtime.process.cicons = m;
    r
}
#[test]
fn both_cpus_fresh_graphs_live_source_edits_compaction_release_and_full_disposal() {
    let mut a = app(resource());
    let raw = a
        .runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"cicn", 128)
        .unwrap();
    let free = a.runtime.process.free_memory();
    let one = invoke(&mut a, Architecture::M68k, Op::Get(128));
    assert_ne!(one, 0);
    let image = field(&a, one, 78);
    let table = field(&a, one, 42);
    assert_eq!(field(&a, one, 0), payload(&a, image));
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(field(&a, one, 50)), 2)
            .unwrap(),
        [0xc0, 0x80]
    );
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(field(&a, one, 64)), 2)
            .unwrap(),
        [0x80, 0x40]
    );
    a.runtime
        .memory
        .write(GuestPtr(payload(&a, raw) + 96), &0x1234u16.to_be_bytes())
        .unwrap();
    let two = invoke(&mut a, Architecture::PowerPc, Op::Get(128));
    assert_ne!(one, two);
    assert_ne!(image, field(&a, two, 78));
    assert_ne!(table, field(&a, two, 42));
    assert_eq!(
        a.runtime
            .memory
            .u16(GuestPtr(payload(&a, table) + 10))
            .unwrap(),
        0xffff
    );
    assert_eq!(
        a.runtime
            .memory
            .u16(GuestPtr(payload(&a, field(&a, two, 42)) + 10))
            .unwrap(),
        0x1234
    );
    for h in [one, two, image, table] {
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, h)
                .unwrap()
                & 0xa0,
            0
        );
    }
    a.runtime
        .process
        .set_handle_lock(&a.runtime.memory, image, false)
        .unwrap();
    let base = payload(&a, image);
    a.runtime
        .process
        .compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(payload(&a, image), base);
    assert_eq!(field(&a, one, 0), base);
    assert_eq!(a.runtime.memory.read(GuestPtr(base), 2).unwrap(), [1, 0x10]);
    invoke(&mut a, Architecture::PowerPc, Op::Dispose(one));
    invoke(&mut a, Architecture::M68k, Op::Dispose(two));
    assert_eq!(a.runtime.process.free_memory(), free);
    let three = invoke(&mut a, Architecture::PowerPc, Op::Get(128));
    a.runtime
        .process
        .release_resource(&mut a.runtime.memory, raw)
        .unwrap();
    a.runtime
        .process
        .compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(field(&a, three, 0)), 2)
            .unwrap(),
        [1, 0x10]
    );
    invoke(&mut a, Architecture::M68k, Op::Dispose(three));
}
#[test]
fn missing_resource_nil_disposal_and_partial_allocation_failure_recover() {
    let mut a = app(resource());
    assert_eq!(invoke(&mut a, Architecture::PowerPc, Op::Get(-123)), 0);
    assert_eq!(a.runtime.process.res_error, -192);
    invoke(&mut a, Architecture::M68k, Op::Dispose(0));
    a.runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"cicn", 128)
        .unwrap();
    let free = a.runtime.process.free_memory();
    let filler = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, free - 96, false)
        .unwrap();
    assert_ne!(filler, 0);
    let remaining = a.runtime.process.free_memory();
    assert_eq!(invoke(&mut a, Architecture::M68k, Op::Get(128)), 0);
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.process.free_memory(), remaining);
    a.runtime.process.dispose_pointer(filler);
    let h = invoke(&mut a, Architecture::PowerPc, Op::Get(128));
    assert_ne!(h, 0);
    invoke(&mut a, Architecture::M68k, Op::Dispose(h));
    assert_eq!(a.runtime.process.free_memory(), free);
}
#[test]
fn truncations_bad_layouts_and_undefined_pixels_reject_before_graph_allocation() {
    let mut cases: Vec<_> = (0..112).map(|n| resource()[..n].to_vec()).collect();
    for (at, v) in [
        (38, 1u16),
        (4, 0xc001),
        (16, 1),
        (32, 16),
        (54, 0x8001),
        (60, 1),
        (90, 0x4000),
        (102, 0),
        (110, 0x2300),
    ] {
        let mut b = resource();
        b[at..at + 2].copy_from_slice(&v.to_be_bytes());
        cases.push(b);
    }
    let mut tail = resource();
    tail.push(0);
    cases.push(tail);
    for b in cases {
        let mut a = app(b);
        a.runtime
            .process
            .get_resource(&mut a.runtime.memory, *b"cicn", 128)
            .unwrap();
        let free = a.runtime.process.free_memory();
        assert!(manager(&mut a, Op::Get(128)).is_err());
        assert_eq!(a.runtime.process.free_memory(), free);
    }
}
#[test]
fn modified_graph_disposal_is_atomic_and_native_result_preflight_preserves_stack() {
    let mut a = app(resource());
    let h = invoke(&mut a, Architecture::M68k, Op::Get(128));
    let table = field(&a, h, 42);
    let at = GuestPtr(payload(&a, h) + 42);
    a.runtime.memory.write(at, &0u32.to_be_bytes()).unwrap();
    let free = a.runtime.process.free_memory();
    assert!(manager(&mut a, Op::Dispose(h)).is_err());
    assert_eq!(a.runtime.process.free_memory(), free);
    a.runtime.memory.write(at, &table.to_be_bytes()).unwrap();
    invoke(&mut a, Architecture::PowerPc, Op::Dispose(h));
    for (len, perm) in [(5, Permissions::RW), (6, Permissions::RX)] {
        let mut memory = Memory::default();
        memory.map(GuestPtr(0x1000), vec![0; len], perm).unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        assert!(Abi::trap(0xaa1e, c, &memory).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
}
#[test]
fn optional_mono_is_nil_and_pixel_storage_cannot_be_relocated_or_freed() {
    let mut b = resource();
    b[68..70].fill(0);
    b.drain(84..86);
    let mut a = app(b);
    let h = invoke(&mut a, Architecture::PowerPc, Op::Get(128));
    assert_eq!(field(&a, h, 64), 0);
    let image = field(&a, h, 78);
    let base = payload(&a, image);
    assert!(a
        .runtime
        .process
        .dispose_handle(&mut a.runtime.memory, image)
        .is_err());
    a.runtime
        .process
        .move_handle_high(&mut a.runtime.memory, image)
        .unwrap();
    assert_eq!(a.runtime.process.mem_error, -117);
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, image, 4096)
        .unwrap();
    assert_ne!(a.runtime.process.mem_error, 0);
    a.runtime
        .process
        .purge_and_compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(payload(&a, image), base);
    assert_eq!(field(&a, h, 0), base);
    invoke(&mut a, Architecture::M68k, Op::Dispose(h));
}

#[test]
fn all_indexed_depths_and_nonzero_bounds_preserve_pixels_and_record_geometry() {
    for (depth, pixels) in [
        (1u16, vec![0x40, 0x80]),
        (2, vec![0x10, 0x40]),
        (4, vec![1, 0x10]),
        (8, vec![0, 1, 1, 0]),
    ] {
        let mut b = resource();
        b.truncate(110);
        b.extend(&pixels);
        let row = (2 * depth).div_ceil(8);
        b[4..6].copy_from_slice(&(0x8000 | row).to_be_bytes());
        for at in [32, 36] {
            b[at..at + 2].copy_from_slice(&depth.to_be_bytes());
        }
        for base in [0, 50, 64] {
            for (at, v) in [(6, -3i16), (8, -4), (10, -1), (12, -2)] {
                b[base + at..base + at + 2].copy_from_slice(&v.to_be_bytes());
            }
        }
        for arch in [Architecture::M68k, Architecture::PowerPc] {
            let mut a = app(b.clone());
            let h = invoke(&mut a, arch, Op::Get(128));
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(field(&a, h, 0)), pixels.len())
                    .unwrap(),
                pixels
            );
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(payload(&a, h) + 8)).unwrap() as i16,
                -4
            );
            invoke(&mut a, arch, Op::Dispose(h));
        }
    }
}

#[test]
fn cached_resource_pointer_placeholders_are_replaced_without_following_foreign_addresses() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app(resource());
        let raw = a
            .runtime
            .process
            .get_resource(&mut a.runtime.memory, *b"cicn", 128)
            .unwrap();
        let source = payload(&a, raw);
        for at in [0, 42, 50, 64, 78] {
            a.runtime
                .memory
                .write(GuestPtr(source + at), &0xdeadbeefu32.to_be_bytes())
                .unwrap();
        }
        let h = invoke(&mut a, arch, Op::Get(128));
        assert_ne!(h, 0);
        for at in [0, 42, 50, 64, 78] {
            assert_ne!(field(&a, h, at), 0xdeadbeef);
        }
        assert_eq!(
            a.runtime
                .memory
                .read(GuestPtr(field(&a, h, 50)), 2)
                .unwrap(),
            [0xc0, 0x80]
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(field(&a, h, 0)), 2).unwrap(),
            [1, 0x10]
        );
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(source + 50)).unwrap(),
            0xdeadbeef
        );
        invoke(&mut a, arch, Op::Dispose(h));
    }
}
