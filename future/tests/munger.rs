mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    process::munger::Request,
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"Munger"], 64);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xe0], Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 4096], Permissions::RW)
        .unwrap();
    a
}
fn handle(a: &mut LoadedApplication, b: &[u8]) -> u32 {
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, b.len() as u32, false)
        .unwrap();
    let p = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(p), b).unwrap();
    h
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
fn invoke(a: &mut LoadedApplication, arch: Architecture, r: Request) -> i32 {
    let stack = STACK_BASE + 0x8000;
    let words = [
        r.replacement_len as u32,
        r.replacement.0,
        r.needle_len as u32,
        r.needle.0,
        r.offset as u32,
        r.handle,
    ];
    let mut frame: Vec<u8> = words.into_iter().flat_map(u32::to_be_bytes).collect();
    frame.extend([0xa5; 8]);
    a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8))
        .unwrap();
    let gate = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(
        arch,
        if arch == Architecture::PowerPc {
            gate
        } else {
            0x50000000
        },
        stack,
    );
    if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
        for (i, v) in [
            r.handle,
            r.offset as u32,
            r.needle.0,
            r.needle_len as u32,
            r.replacement.0,
            r.replacement_len as u32,
        ]
        .into_iter()
        .enumerate()
        {
            c.gpr[3 + i] = v;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    match &a.runtime.cpu {
        Cpu::PowerPc(c) => c.gpr[3] as i32,
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], stack + 24);
            assert_eq!(
                a.runtime.memory.u32(GuestPtr(stack + 28)).unwrap(),
                0xa5a5a5a5
            );
            assert_eq!(c.dar[0], a.runtime.process.mem_error as i32 as u32);
            a.runtime.memory.u32(GuestPtr(stack + 24)).unwrap() as i32
        }
    }
}
#[test]
fn both_cpus_share_search_insert_replace_delete_and_reference_edge_modes() {
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        let mut a = app();
        for (source, offset, needle, len1, replacement, len2, expected, result) in [
            (
                b"aa cat bb cat".as_slice(),
                0,
                Some(b"cat".as_slice()),
                3,
                Some(b"dog".as_slice()),
                3,
                b"aa dog bb cat".as_slice(),
                6,
            ),
            (
                b"aa cat bb cat",
                7,
                Some(b"cat"),
                3,
                Some(b"X"),
                1,
                b"aa cat bb X",
                11,
            ),
            (b"ab", 1, Some(b""), 0, Some(b"XY"), 2, b"aXYb", 3),
            (b"cat dog", 0, Some(b"cat"), 3, Some(b""), 0, b" dog", 0),
            (b"abcdef", 2, None, 2, Some(b"XY"), 2, b"abXYef", 4),
            (b"abcdef", 2, None, -1, Some(b"XY"), 2, b"abXY", 4),
            (b"abcdef", 2, None, 2, None, 99, b"abef", 2),
            (b"abc", 0, None, -1, Some(b""), 0, b"", 0),
            (b"xabc", 0, Some(b"ab"), 2, None, -1, b"xabc", 1),
            (b"abc", 0, Some(b"zz"), 2, Some(b"X"), 1, b"abc", -1),
            (b"ab", 0, Some(b"abc"), 3, Some(b"Z"), 1, b"ab", -1),
            (b"fooab", 3, Some(b"abc"), 3, Some(b"Z"), 1, b"fooab", -1),
            (b"", 0, None, 0, Some(b"A"), 1, b"A", 1),
            (b"abcd", 2, None, 500, Some(b"XY"), 2, b"abXY", 4),
            (b"a\0b", 0, Some(b"\0"), 1, Some(b"X"), 1, b"aXb", 2),
            (
                b"a\x80b",
                0,
                Some(b"\x80"),
                1,
                Some(b"\xff"),
                1,
                b"a\xffb",
                2,
            ),
        ] {
            let h = handle(&mut a, source);
            let p1 = if let Some(b) = needle {
                a.runtime.memory.write(GuestPtr(0x51000000), b).unwrap();
                if len1 == 0 {
                    GuestPtr(u32::MAX)
                } else {
                    GuestPtr(0x51000000)
                }
            } else {
                GuestPtr(0)
            };
            let p2 = if let Some(b) = replacement {
                a.runtime.memory.write(GuestPtr(0x51000100), b).unwrap();
                if len2 == 0 {
                    GuestPtr(u32::MAX)
                } else {
                    GuestPtr(0x51000100)
                }
            } else {
                GuestPtr(0)
            };
            assert_eq!(
                invoke(
                    &mut a,
                    arch,
                    Request {
                        handle: h,
                        offset,
                        needle: p1,
                        needle_len: len1,
                        replacement: p2,
                        replacement_len: len2
                    }
                ),
                result,
                "{arch:?} {source:?}"
            );
            assert_eq!(a.runtime.process.mem_error, 0);
            assert_eq!(bytes(&mut a, h), expected);
        }
    }
}
#[test]
fn aliased_sources_survive_relocation_and_locked_failures_are_atomic() {
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        let mut a = app();
        let h = handle(&mut a, b"abcabc");
        let old = a.runtime.memory.u32(GuestPtr(h)).unwrap();
        assert_ne!(
            a.runtime
                .process
                .new_pointer(&mut a.runtime.memory, 32, false)
                .unwrap(),
            0
        );
        a.runtime
            .process
            .set_handle_state(&a.runtime.memory, h, 0xe0)
            .unwrap();
        // Six logical bytes occupy eight allocated bytes: locked growth to
        // eight is legal and must not be confused with a required relocation.
        let in_place = Request {
            handle: h,
            offset: 0,
            needle: GuestPtr(old),
            needle_len: 3,
            replacement: GuestPtr(old + 1),
            replacement_len: 5,
        };
        assert_eq!(invoke(&mut a, arch, in_place), 5);
        assert_eq!(a.runtime.memory.u32(GuestPtr(h)).unwrap(), old);
        assert_eq!(bytes(&mut a, h), b"bcabcabc");
        a.runtime
            .process
            .set_handle_size(&mut a.runtime.memory, h, 6)
            .unwrap();
        a.runtime.memory.write(GuestPtr(old), b"abcabc").unwrap();
        let r = Request {
            handle: h,
            offset: 0,
            needle: GuestPtr(old),
            needle_len: 1,
            replacement: GuestPtr(old + 1),
            replacement_len: 5,
        };
        let free = a.runtime.process.free_memory();
        assert_eq!(invoke(&mut a, arch, r), -1);
        assert_eq!(a.runtime.process.mem_error, -108);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(bytes(&mut a, h), b"abcabc");
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, h)
                .unwrap(),
            0xe0
        );
        a.runtime
            .process
            .set_handle_state(&a.runtime.memory, h, 0x60)
            .unwrap();
        assert_eq!(invoke(&mut a, arch, r), 5);
        assert_ne!(a.runtime.memory.u32(GuestPtr(h)).unwrap(), old);
        assert_eq!(bytes(&mut a, h), b"bcabcbcabc");
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, h)
                .unwrap(),
            0x60
        );
    }
}
#[test]
fn search_misses_ignore_replacement_and_memory_faults_leave_the_handle_unchanged() {
    let mut a = app();
    let h = handle(&mut a, b"abcdef");
    let original = bytes(&mut a, h);
    let free = a.runtime.process.free_memory();
    a.runtime.memory.write(GuestPtr(0x51000000), b"zz").unwrap();
    let r = Request {
        handle: h,
        offset: 0,
        needle: GuestPtr(0x51000000),
        needle_len: 2,
        replacement: GuestPtr(0xdeadbeef),
        replacement_len: 3,
    };
    assert_eq!(
        a.runtime.process.munger(&mut a.runtime.memory, r).unwrap(),
        -1
    );
    for r in [
        Request {
            needle: GuestPtr(0xdeadbeef),
            ..r
        },
        Request {
            needle: GuestPtr(0),
            needle_len: 1,
            ..r
        },
        Request {
            needle_len: -1,
            ..r
        },
    ] {
        assert!(a.runtime.process.munger(&mut a.runtime.memory, r).is_err());
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(bytes(&mut a, h), original);
    }
    assert_eq!(
        a.runtime
            .process
            .munger(
                &mut a.runtime.memory,
                Request {
                    needle: GuestPtr(0),
                    needle_len: 0,
                    replacement_len: i32::MAX,
                    ..r
                }
            )
            .unwrap(),
        -1
    );
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(bytes(&mut a, h), original);
}
#[test]
fn nil_handles_and_invalid_offsets_return_failure_without_reading_unused_inputs() {
    let mut a = app();
    let r = Request {
        handle: 0,
        offset: 0,
        needle: GuestPtr(0xdeadbeef),
        needle_len: 2,
        replacement: GuestPtr(0xdeadbeef),
        replacement_len: 3,
    };
    assert_eq!(
        a.runtime.process.munger(&mut a.runtime.memory, r).unwrap(),
        -1
    );
    assert_eq!(a.runtime.process.mem_error, -109);
    let h = handle(&mut a, b"abc");
    for offset in [-1, 4, i32::MAX] {
        assert_eq!(
            a.runtime
                .process
                .munger(
                    &mut a.runtime.memory,
                    Request {
                        handle: h,
                        offset,
                        ..r
                    }
                )
                .unwrap(),
            -1
        );
        assert_eq!(bytes(&mut a, h), b"abc");
    }
}
#[test]
fn native_result_preflight_precedes_stack_pop_and_trap_discovery_is_available() {
    use systemless_future::{abi::Abi, memory::Memory, traps};
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 24], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x1018), vec![0xa5; 4], Permissions::RX)
        .unwrap();
    let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
        panic!()
    };
    assert!(Abi::trap(0xa9e0, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1000);
    m.map(GuestPtr(0x2000), vec![0xa5; 4], Permissions::RW)
        .unwrap();
    let mut other = Cpu::new(Architecture::PowerPc, 0, 0);
    assert!(systemless_future::abi::Reply::M68kMunger(GuestPtr(0x2000))
        .apply(&mut other, &mut m, 7, 0)
        .is_err());
    assert_eq!(m.read(GuestPtr(0x2000), 4).unwrap(), [0xa5; 4]);
    let a = app();
    let address = traps::address(0xa9e0, true);
    assert_ne!(address, traps::UNIMPLEMENTED);
    assert_eq!(a.runtime.memory.u16(GuestPtr(address)).unwrap(), 0xa9e0);
}
