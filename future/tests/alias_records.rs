mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
const BASE: u32 = 0x51000000;
#[derive(Clone)]
struct Entry {
    id: u32,
    parent: u32,
    name: Vec<u8>,
    created: u32,
    finder: [u8; 16],
    resource: Vec<u8>,
}
fn entry(id: u32, parent: u32, name: &[u8], created: u32) -> Entry {
    let mut finder = [0; 16];
    finder[..8].copy_from_slice(b"PILTTEST");
    Entry {
        id,
        parent,
        name: name.to_vec(),
        created,
        finder,
        resource: Vec::new(),
    }
}
fn standard() -> Vec<Entry> {
    vec![
        entry(20, 17, b"Hero", 123),
        entry(21, 17, b"Source", 124),
        entry(22, 18, b"OtherSource", 125),
    ]
}
fn image(files: &[Entry]) -> Vec<u8> {
    let mut b = b"SLVOLUME".to_vec();
    for n in [1u32, 100, 3, files.len() as u32, 0, 0] {
        b.extend(n.to_be_bytes());
    }
    for (id, parent, name, date) in [
        (17u32, 2u32, b"Pilots".as_slice(), 100u32),
        (18, 2, b"Other".as_slice(), 200),
        (19, 17, b"Nested".as_slice(), 300),
    ] {
        for n in [id, parent, date] {
            b.extend(n.to_be_bytes());
        }
        b.push(name.len() as u8);
        b.extend(name);
    }
    for f in files {
        for n in [f.id, f.parent, f.created, f.created] {
            b.extend(n.to_be_bytes());
        }
        b.extend(f.finder);
        b.extend(0u32.to_be_bytes());
        b.extend((f.resource.len() as u32).to_be_bytes());
        b.push(f.name.len() as u8);
        b.extend(&f.name);
        b.extend(&f.resource);
    }
    let size = (b.len() - 32) as u32;
    b[24..28].copy_from_slice(&size.to_be_bytes());
    let mut crc = !0u32;
    for byte in &b {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    b.extend((!crc).to_be_bytes());
    b
}
fn mount(a: &mut LoadedApplication, files: &[Entry]) {
    let fixture = support::pef_program(&[0x4e800020], &[], 0);
    let mut catalog = LoadedApplication::pef(&fixture.bytes, None)
        .unwrap()
        .runtime
        .process
        .files;
    catalog.restore_volume(&image(files)).unwrap();
    a.runtime.process.files = catalog;
}
fn setup() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"NewAlias", b"ResolveAlias", b"ResolveAliasFile"],
        16,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x23], Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(BASE), vec![0x5a; 4096], Permissions::RW)
        .unwrap();
    mount(&mut a, &standard());
    a
}
fn spec(parent: u32, name: &[u8]) -> [u8; 70] {
    let mut b = [0; 70];
    b[..2].copy_from_slice(&(-1i16).to_be_bytes());
    b[2..6].copy_from_slice(&parent.to_be_bytes());
    b[6] = name.len() as u8;
    b[7..7 + name.len()].copy_from_slice(name);
    b
}
fn call(a: &mut LoadedApplication, native: bool, index: u32, args: &[u32]) -> i16 {
    let sp = STACK_BASE + 0x7000;
    let selector = [2u32, 3, 12][index as usize];
    if native {
        let mut f = Vec::new();
        if index == 2 {
            f.extend(args[3].to_be_bytes());
            f.extend(args[2].to_be_bytes());
            f.extend([args[1] as u8, 0xa5]);
            f.extend(args[0].to_be_bytes());
        } else {
            for arg in args.iter().rev() {
                f.extend(arg.to_be_bytes());
            }
        }
        let n = f.len() as u32;
        f.extend([0xa5; 6]);
        a.runtime.memory.write(GuestPtr(sp), &f).unwrap();
        a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, sp);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        for i in 0..15 {
            c.dar[i] = 0xabcd0000 + i as u32;
        }
        c.dar[0] = 0xcafe0000 | selector;
        let saved = c.dar;
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            unreachable!()
        };
        assert_eq!(&c.dar[..15], &saved[..15]);
        assert_eq!(c.dar[15], sp + n);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(sp + n + 2), 4).unwrap(),
            [0xa5; 4]
        );
        a.runtime.memory.u16(GuestPtr(sp + n)).unwrap() as i16
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, sp);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        for i in 2..32 {
            c.gpr[i] = 0xabcd0000 + i as u32;
        }
        for (i, arg) in args.iter().enumerate() {
            c.gpr[3 + i] = *arg;
        }
        let saved = c.gpr;
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::PowerPc(c) = &a.runtime.cpu else {
            unreachable!()
        };
        assert_eq!(&c.gpr[4..], &saved[4..]);
        assert_eq!(c.gpr[1], sp);
        c.gpr[3] as i32 as i16
    }
}
fn create(
    a: &mut LoadedApplication,
    native: bool,
    from: Option<(u32, &[u8])>,
    parent: u32,
    name: &[u8],
) -> u32 {
    a.runtime
        .memory
        .write(GuestPtr(BASE), &spec(parent, name))
        .unwrap();
    let from = if let Some((p, n)) = from {
        a.runtime
            .memory
            .write(GuestPtr(BASE + 0x100), &spec(p, n))
            .unwrap();
        BASE + 0x100
    } else {
        0
    };
    assert_eq!(call(a, native, 0, &[from, BASE, BASE + 0x80]), 0);
    a.runtime.memory.u32(GuestPtr(BASE + 0x80)).unwrap()
}
fn bytes(a: &mut LoadedApplication, h: u32) -> Vec<u8> {
    let n = a.runtime.process.handle_size(&a.runtime.memory, h).unwrap();
    let p = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime
        .memory
        .read(GuestPtr(p), n as usize)
        .unwrap()
        .to_vec()
}
#[test]
fn both_cpus_create_real_independent_alias_records_and_resolve_the_same_catalog_identity() {
    let mut a = setup();
    a.runtime.process.res_error = -192;
    for native in [false, true] {
        let h = create(&mut a, native, None, 17, b"Hero");
        let b = bytes(&mut a, h);
        assert_eq!(b.len(), 198);
        assert_eq!(&b[..10], &[0, 0, 0, 0, 0, 198, 0, 2, 0, 0]);
        assert_eq!(&b[10..29], b"\x12Application Volume");
        assert_eq!(&b[38..46], &[0, 0, 0, 0, b'B', b'D', 0, 0]);
        assert_eq!(&b[46..55], b"\0\0\0\x11\x04Hero");
        assert_eq!(&b[114..122], &[0, 0, 0, 20, 0, 0, 0, 123]);
        assert_eq!(&b[122..134], b"PILTTEST\xff\xff\xff\xff");
        assert_eq!(&b[150..160], b"\0\0\0\x06Pilots");
        assert_eq!(&b[160..194], b"\0\x02\0\x1eApplication Volume:Pilots:Hero");
        assert_eq!(&b[194..], &[255, 255, 0, 0]);
        let other = create(&mut a, !native, None, 17, b"Hero");
        assert_ne!(h, other);
        assert_eq!(bytes(&mut a, other), b);
        a.runtime
            .process
            .set_handle_lock(&a.runtime.memory, h, true)
            .unwrap();
        assert_eq!(
            call(&mut a, !native, 1, &[0, h, BASE + 0x200, BASE + 0x280]),
            0
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(BASE + 0x200), 70).unwrap(),
            spec(17, b"Hero")
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(BASE + 0x280), 1).unwrap(),
            [0]
        );
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, h)
                .unwrap(),
            0x80
        );
        assert_eq!(a.runtime.process.res_error, -192);
        a.runtime
            .process
            .dispose_handle(&mut a.runtime.memory, h)
            .unwrap();
        a.runtime
            .process
            .dispose_handle(&mut a.runtime.memory, other)
            .unwrap();
    }
}
#[test]
fn stable_identity_tracks_moves_and_renames_and_preserves_user_private_data_and_slack() {
    let mut a = setup();
    let h = create(&mut a, false, None, 17, b"Hero");
    let mut b = bytes(&mut a, h);
    b[..4].copy_from_slice(b"USER");
    b[140] = 0xa5;
    b.truncate(b.len() - 4);
    b.extend([0, 99, 0, 6]);
    b.extend(b"opaque");
    b.extend([255, 255, 0, 0]);
    let n = b.len() as u16;
    b[4..6].copy_from_slice(&n.to_be_bytes());
    b.extend(b"KEEP");
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, h, b.len() as u32)
        .unwrap();
    assert_eq!(a.runtime.process.mem_error, 0);
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(data), &b).unwrap();
    let mut files = standard();
    files[0].name = b"Renamed".to_vec();
    files[0].parent = 18;
    mount(&mut a, &files);
    assert_eq!(
        call(&mut a, true, 1, &[0, h, BASE + 0x200, BASE + 0x280]),
        0
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BASE + 0x200), 70).unwrap(),
        spec(18, b"Renamed")
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BASE + 0x280), 1).unwrap(),
        [1]
    );
    let b = bytes(&mut a, h);
    assert_eq!(&b[..4], b"USER");
    assert_eq!(b[140], 0xa5);
    assert_eq!(&b[b.len() - 4..], b"KEEP");
    assert!(b.windows(6).any(|v| v == b"opaque"));
    assert_eq!(&b[46..50], &18u32.to_be_bytes());
    assert_eq!(&b[51..58], b"Renamed");
    assert_eq!(
        call(&mut a, false, 1, &[0, h, BASE + 0x200, BASE + 0x280]),
        0
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BASE + 0x280), 1).unwrap(),
        [0]
    );
    files[0].created = 999;
    mount(&mut a, &files);
    let saved = a
        .runtime
        .memory
        .read(GuestPtr(BASE + 0x200), 129)
        .unwrap()
        .to_vec();
    assert_eq!(
        call(&mut a, false, 1, &[0, h, BASE + 0x200, BASE + 0x280]),
        -43
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BASE + 0x200), 129).unwrap(),
        saved
    );
}
#[test]
fn relative_search_prefers_the_corresponding_copied_tree_and_root_directory_aliases_work() {
    let mut a = setup();
    let h = create(&mut a, true, Some((17, b"Source")), 17, b"Hero");
    let b = bytes(&mut a, h);
    assert_eq!(&b[130..134], &[0, 1, 0, 1]);
    let mut files = standard();
    files.push(entry(40, 18, b"Hero", 123));
    files.push(entry(41, 18, b"Source", 124));
    mount(&mut a, &files);
    a.runtime
        .memory
        .write(GuestPtr(BASE + 0x100), &spec(18, b"Source"))
        .unwrap();
    assert_eq!(
        call(
            &mut a,
            false,
            1,
            &[BASE + 0x100, h, BASE + 0x200, BASE + 0x280]
        ),
        0
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BASE + 0x200), 70).unwrap(),
        spec(18, b"Hero")
    );
    assert_eq!(&bytes(&mut a, h)[114..118], &40u32.to_be_bytes());
    let root = create(&mut a, false, None, 2, b"");
    let b = bytes(&mut a, root);
    assert_eq!(&b[8..10], &[0, 1]);
    assert_eq!(
        call(&mut a, true, 1, &[0, root, BASE + 0x200, BASE + 0x280]),
        0
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BASE + 0x200), 70).unwrap(),
        spec(1, b"Application Volume")
    );
}
#[test]
fn finder_alias_resources_follow_chain_control_and_detect_cycles_before_writing_outputs() {
    let mut a = setup();
    let h = create(&mut a, false, None, 17, b"Hero");
    let raw = bytes(&mut a, h);
    let mut alias = entry(30, 2, b"A", 150);
    alias.finder[8] = 0x80;
    alias.resource = support::resource(vec![(*b"alis", 0, None, raw)]);
    let mut files = standard();
    files.push(alias.clone());
    mount(&mut a, &files);
    let h = create(&mut a, true, None, 2, b"A");
    let to_a = bytes(&mut a, h);
    let mut second = entry(31, 2, b"B", 151);
    second.finder[8] = 0x80;
    second.resource = support::resource(vec![(*b"alis", 0, None, to_a)]);
    files.push(second.clone());
    mount(&mut a, &files);
    for native in [false, true] {
        for chains in [false, true] {
            a.runtime
                .memory
                .write(GuestPtr(BASE), &spec(2, b"B"))
                .unwrap();
            assert_eq!(
                call(
                    &mut a,
                    native,
                    2,
                    &[BASE, u32::from(chains), BASE + 0x80, BASE + 0x82]
                ),
                0
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(BASE), 70).unwrap(),
                if chains {
                    spec(17, b"Hero")
                } else {
                    spec(2, b"A")
                }
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(BASE + 0x80), 1).unwrap(),
                [0]
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(BASE + 0x82), 1).unwrap(),
                [1]
            );
        }
    }
    let h = create(&mut a, false, None, 2, b"B");
    alias.resource = support::resource(vec![(*b"alis", 0, None, bytes(&mut a, h))]);
    files = standard();
    files.extend([alias, second]);
    mount(&mut a, &files);
    a.runtime
        .memory
        .write(GuestPtr(BASE), &spec(2, b"B"))
        .unwrap();
    let before = a
        .runtime
        .memory
        .read(GuestPtr(BASE), 0x84)
        .unwrap()
        .to_vec();
    assert_eq!(
        call(&mut a, true, 2, &[BASE, 1, BASE + 0x80, BASE + 0x82]),
        -50
    );
    assert_eq!(a.runtime.memory.read(GuestPtr(BASE), 0x84).unwrap(), before);
}
#[test]
fn new_alias_failures_clear_the_handle_without_leaks_and_overlapping_fsspec_is_frozen() {
    let mut a = setup();
    a.runtime
        .memory
        .write(GuestPtr(BASE), &spec(17, b"Missing"))
        .unwrap();
    assert_eq!(call(&mut a, false, 0, &[0, BASE, BASE + 0x80]), -43);
    assert_eq!(a.runtime.memory.u32(GuestPtr(BASE + 0x80)).unwrap(), 0);
    a.runtime
        .memory
        .write(GuestPtr(BASE), &spec(17, b"Hero"))
        .unwrap();
    let free = a.runtime.process.free_memory();
    let held = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, free - 128, false)
        .unwrap();
    assert_ne!(held, 0);
    assert_eq!(call(&mut a, true, 0, &[0, BASE, BASE + 0x80]), -108);
    assert_eq!(a.runtime.memory.u32(GuestPtr(BASE + 0x80)).unwrap(), 0);
    assert_eq!(a.runtime.process.free_memory(), 128);
    a.runtime.process.dispose_pointer(held);
    assert_eq!(call(&mut a, true, 0, &[0, BASE, BASE]), 0);
    let h = a.runtime.memory.u32(GuestPtr(BASE)).unwrap();
    let b = bytes(&mut a, h);
    assert_eq!(&b[51..55], b"Hero");
}
#[test]
fn native_new_alias_preflights_parameters_and_result_before_popping() {
    for (len, permission) in [(13, Permissions::RW), (14, Permissions::RX)] {
        let mut m = Memory::default();
        let mut f = vec![0; len];
        f[..4].copy_from_slice(&0x3000u32.to_be_bytes());
        f[4..8].copy_from_slice(&0x2000u32.to_be_bytes());
        m.map(GuestPtr(0x1000), f, permission).unwrap();
        m.map(
            GuestPtr(0x2000),
            spec(17, b"Hero").to_vec(),
            Permissions::RX,
        )
        .unwrap();
        m.map(GuestPtr(0x3000), vec![0xa5; 4], Permissions::RW)
            .unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else {
            unreachable!()
        };
        c.dar[0] = 2;
        assert!(Abi::trap(0xa823, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
        assert_eq!(m.read(GuestPtr(0x3000), 4).unwrap(), [0xa5; 4]);
    }
}

#[test]
fn both_alias_selectors_preflight_short_readonly_nil_and_missing_outputs_without_pop() {
    for selector in [2u32, 3] {
        for which in 0..5 {
            let mut m = Memory::default();
            m.map(GuestPtr(0x1000), vec![0xa5; 24], Permissions::RW)
                .unwrap();
            m.map(
                GuestPtr(0x2000),
                vec![0; if which == 0 { 69 } else { 70 }],
                Permissions::RW,
            )
            .unwrap();
            m.map(
                GuestPtr(0x3000),
                vec![0x5a; 4],
                if which == 1 {
                    Permissions::RX
                } else {
                    Permissions::RW
                },
            )
            .unwrap();
            m.map(
                GuestPtr(0x4000),
                vec![0; if which == 4 { 69 } else { 70 }],
                Permissions::RW,
            )
            .unwrap();
            let target = if which == 2 && selector == 2 {
                0
            } else {
                0x2000u32
            };
            let out = if which == 2 && selector == 3 {
                0
            } else if which == 3 {
                0x3004u32
            } else {
                0x3000
            };
            let mut f = out.to_be_bytes().to_vec();
            f.extend(target.to_be_bytes());
            if selector == 3 {
                f.extend(0x4444u32.to_be_bytes());
            }
            f.extend(0x4000u32.to_be_bytes());
            m.write(GuestPtr(0x1000), &f).unwrap();
            let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
            let Cpu::M68k(c) = &mut cpu else {
                unreachable!()
            };
            c.dar[0] = selector;
            assert!(
                Abi::trap(0xa823, c, &m).is_err(),
                "selector{selector} case{which}"
            );
            assert_eq!(c.dar[15], 0x1000);
            assert_eq!(m.read(GuestPtr(0x3000), 4).unwrap(), [0x5a; 4]);
            assert_eq!(m.read(GuestPtr(0x1000), f.len()).unwrap(), f);
        }
    }
}

#[test]
fn resolve_nil_arguments_are_guest_errors_and_missing_targets_publish_a_hint() {
    for native in [false, true] {
        let mut a = setup();
        let h = create(&mut a, native, None, 17, b"Hero");
        let original = bytes(&mut a, h);
        for (alias, target) in [(0, BASE + 0x200), (h, 0)] {
            a.runtime
                .memory
                .write(GuestPtr(BASE + 0x200), &[0xa5; 129])
                .unwrap();
            assert_eq!(
                call(&mut a, native, 1, &[0, alias, target, BASE + 0x280]),
                -50
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(BASE + 0x200), 129).unwrap(),
                [0xa5; 129]
            );
        }
        let mut files = standard();
        files.remove(0);
        mount(&mut a, &files);
        assert_eq!(
            call(&mut a, native, 1, &[0, h, BASE + 0x200, BASE + 0x280]),
            -43
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(BASE + 0x200), 70).unwrap(),
            spec(17, b"Hero")
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(BASE + 0x280), 1).unwrap(),
            [0]
        );
        assert_eq!(bytes(&mut a, h), original);
    }
}
