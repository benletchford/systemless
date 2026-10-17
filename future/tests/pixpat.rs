mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{resource::ResourceFork, LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    pixpat::Op,
    runtime::Stop,
};
fn resource() -> Vec<u8> {
    let mut bytes = vec![0; 104];
    bytes[..2].copy_from_slice(&1u16.to_be_bytes());
    bytes[2..6].copy_from_slice(&28u32.to_be_bytes());
    bytes[6..10].copy_from_slice(&78u32.to_be_bytes());
    bytes[14..16].copy_from_slice(&0xffffu16.to_be_bytes());
    bytes[20..28].copy_from_slice(&[0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55]);
    for (at, value) in [(4, 0x8001u16), (10, 2), (12, 2), (32, 4), (34, 1), (36, 4)] {
        bytes[28 + at..30 + at].copy_from_slice(&value.to_be_bytes());
    }
    for at in [22, 26] {
        bytes[28 + at..32 + at].copy_from_slice(&0x00480000u32.to_be_bytes());
    }
    bytes[70..74].copy_from_slice(&80u32.to_be_bytes());
    bytes[78..80].copy_from_slice(&[0x01, 0x10]);
    bytes[86..88].copy_from_slice(&1u16.to_be_bytes());
    bytes[90..96].fill(0xff);
    bytes[96..98].copy_from_slice(&1u16.to_be_bytes());
    bytes
}
fn app(bytes: Vec<u8>) -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"GetPixPat", b"DisposePixPat"], 24);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime.process.install_application_resources(
        ResourceFork::parse(&support::resource(vec![(*b"ppat", 128, None, bytes)])).unwrap(),
    );
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xaa, 0x0c, 0xaa, 0x08],
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
fn graph(a: &LoadedApplication, h: u32) -> [u32; 5] {
    let p = payload(a, h);
    let field = |at| a.runtime.memory.u32(GuestPtr(p + at)).unwrap();
    let map = field(2);
    [
        map,
        field(6),
        a.runtime
            .memory
            .u32(GuestPtr(payload(a, map) + 42))
            .unwrap(),
        field(10),
        field(16),
    ]
}
#[test]
fn both_cpu_gets_create_independent_live_resource_copies_and_dispose_every_owned_child() {
    let mut a = app(resource());
    let raw = a
        .runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"ppat", 128)
        .unwrap();
    let before = a.runtime.process.free_memory();
    let first = invoke(&mut a, Architecture::M68k, Op::Get(128));
    assert_ne!(first, 0);
    let one = graph(&a, first);
    assert_eq!(
        a.runtime
            .memory
            .u16(GuestPtr(payload(&a, first) + 14))
            .unwrap(),
        0xffff
    );
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(payload(&a, one[1])), 2)
            .unwrap(),
        [0x01, 0x10]
    );
    assert_eq!(
        a.runtime
            .process
            .handle_size(&a.runtime.memory, one[3])
            .unwrap(),
        0
    );
    assert_eq!(
        a.runtime
            .process
            .handle_size(&a.runtime.memory, one[4])
            .unwrap(),
        0
    );
    a.runtime
        .memory
        .write(GuestPtr(payload(&a, raw) + 90), &0x1234u16.to_be_bytes())
        .unwrap();
    let second = invoke(&mut a, Architecture::PowerPc, Op::Get(128));
    let two = graph(&a, second);
    assert_ne!(first, second);
    assert!(one.iter().all(|h| !two.contains(h) && *h != raw));
    assert_eq!(
        a.runtime
            .memory
            .u16(GuestPtr(payload(&a, one[2]) + 10))
            .unwrap(),
        0xffff
    );
    assert_eq!(
        a.runtime
            .memory
            .u16(GuestPtr(payload(&a, two[2]) + 10))
            .unwrap(),
        0x1234
    );
    for h in one.into_iter().chain(two).chain([first, second]) {
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, h)
                .unwrap()
                & 0x20,
            0
        );
    }
    invoke(&mut a, Architecture::PowerPc, Op::Dispose(first));
    a.runtime
        .process
        .compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(
        a.runtime
            .memory
            .u16(GuestPtr(payload(&a, two[2]) + 10))
            .unwrap(),
        0x1234
    );
    invoke(&mut a, Architecture::M68k, Op::Dispose(second));
    assert_eq!(a.runtime.process.free_memory(), before);
}
#[test]
fn typed_pattern_survives_source_release_and_compaction() {
    let mut a = app(resource());
    let raw = a
        .runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"ppat", 128)
        .unwrap();
    let pat = invoke(&mut a, Architecture::PowerPc, Op::Get(128));
    a.runtime
        .process
        .release_resource(&mut a.runtime.memory, raw)
        .unwrap();
    a.runtime
        .process
        .compact_memory(&mut a.runtime.memory)
        .unwrap();
    let members = graph(&a, pat);
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(payload(&a, members[1])), 2)
            .unwrap(),
        [0x01, 0x10]
    );
    invoke(&mut a, Architecture::M68k, Op::Dispose(pat));
}
#[test]
fn allocation_failure_rolls_back_the_partial_graph_and_can_retry() {
    let mut a = app(resource());
    a.runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"ppat", 128)
        .unwrap();
    let free = a.runtime.process.free_memory();
    let filler = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, free - 48, false)
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
}
#[test]
fn bad_graph_disposal_is_atomic_and_missing_resource_returns_nil() {
    let mut a = app(resource());
    assert_eq!(invoke(&mut a, Architecture::M68k, Op::Get(-123)), 0);
    assert_eq!(a.runtime.process.res_error, -192);
    let h = invoke(&mut a, Architecture::PowerPc, Op::Get(128));
    let root = payload(&a, h);
    let original = a.runtime.memory.u32(GuestPtr(root + 6)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(root + 6), &0u32.to_be_bytes())
        .unwrap();
    let free = a.runtime.process.free_memory();
    let mut manager = std::mem::take(&mut a.runtime.process.pixpats);
    let error = manager.call(
        &mut a.runtime.memory,
        &mut a.runtime.process,
        Op::Dispose(h),
    );
    a.runtime.process.pixpats = manager;
    assert!(error.is_err());
    assert_eq!(a.runtime.process.free_memory(), free);
    a.runtime
        .memory
        .write(GuestPtr(root + 6), &original.to_be_bytes())
        .unwrap();
    invoke(&mut a, Architecture::M68k, Op::Dispose(h));
}
#[test]
fn malformed_resources_do_not_allocate_typed_children_and_native_frames_preflight_results() {
    for n in 0..104 {
        let mut a = app(resource()[..n].to_vec());
        a.runtime
            .process
            .get_resource(&mut a.runtime.memory, *b"ppat", 128)
            .unwrap();
        let free = a.runtime.process.free_memory();
        let mut manager = std::mem::take(&mut a.runtime.process.pixpats);
        assert!(manager
            .call(&mut a.runtime.memory, &mut a.runtime.process, Op::Get(128))
            .is_err());
        a.runtime.process.pixpats = manager;
        assert_eq!(a.runtime.process.free_memory(), free, "prefix {n}");
    }
    for (length, permissions) in [(5, Permissions::RW), (6, Permissions::RX)] {
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0; length], permissions)
            .unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        assert!(Abi::trap(0xaa0c, c, &memory).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
}

#[test]
fn malformed_offsets_layouts_color_indices_and_truncated_owned_maps_are_rejected() {
    for (at, bytes) in [
        (2, 0u32.to_be_bytes().to_vec()),
        (6, 28u32.to_be_bytes().to_vec()),
        (70, 78u32.to_be_bytes().to_vec()),
        (32, 0xc001u16.to_be_bytes().to_vec()),
        (60, 16u16.to_be_bytes().to_vec()),
        (84, 0x4000u16.to_be_bytes().to_vec()),
        (96, 0u16.to_be_bytes().to_vec()),
        (78, vec![0x23]),
    ] {
        let mut input = resource();
        input[at..at + bytes.len()].copy_from_slice(&bytes);
        let mut a = app(input);
        a.runtime
            .process
            .get_resource(&mut a.runtime.memory, *b"ppat", 128)
            .unwrap();
        let free = a.runtime.process.free_memory();
        let mut manager = std::mem::take(&mut a.runtime.process.pixpats);
        assert!(
            manager
                .call(&mut a.runtime.memory, &mut a.runtime.process, Op::Get(128))
                .is_err(),
            "offset{at}"
        );
        a.runtime.process.pixpats = manager;
        assert_eq!(a.runtime.process.free_memory(), free);
    }
    let mut a = app(resource());
    let h = invoke(&mut a, Architecture::M68k, Op::Get(128));
    let map = graph(&a, h)[0];
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, map, 4)
        .unwrap();
    let free = a.runtime.process.free_memory();
    let mut manager = std::mem::take(&mut a.runtime.process.pixpats);
    assert!(manager
        .call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            Op::Dispose(h)
        )
        .is_err());
    a.runtime.process.pixpats = manager;
    assert_eq!(a.runtime.process.free_memory(), free);
}
