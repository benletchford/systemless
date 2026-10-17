mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
    traps,
};
const BASE: u32 = 0x50000000;
const CODE: u32 = 0x51000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"EqualString", b"GetOSTrapAddress"], 0);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(BASE), vec![0x5a; 140000], Permissions::RW)
        .unwrap();
    let code = [0xa03cu16, 0xa23c, 0xa43c, 0xa63c, 0x4e92, 0x4e71];
    a.runtime
        .memory
        .map(
            GuestPtr(CODE),
            code.into_iter().flat_map(u16::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    a
}
fn prepare(
    a: &mut LoadedApplication,
    arch: Architecture,
    case: bool,
    marks: bool,
    first: (u32, u16),
    second: (u32, u16),
) {
    let (x, nx) = first;
    let (y, ny) = second;
    let mode = u32::from(marks) + 2 * u32::from(case);
    if arch == Architecture::M68k {
        a.runtime.cpu = Cpu::new(arch, CODE + mode * 2, STACK);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[0] = (u32::from(nx) << 16) | u32::from(ny);
        c.dar[8] = x;
        c.dar[9] = y;
        c.dar[1] = 0x12345678;
        c.dar[7] = 0xfeedbeef;
        c.dar[13] = 0x76543210;
        c.set_sr((c.get_sr() & !0x071f) | 0x031f);
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.gpr[3] = x;
        c.gpr[4] = y;
        c.gpr[5] = 0x12345600 | u32::from(case);
        c.gpr[6] = 0x76543200 | u32::from(marks);
        c.gpr[13] = 0xfeedbeef;
    }
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
}
fn result(a: &mut LoadedApplication, arch: Architecture) -> systemless_future::Result<bool> {
    assert_eq!(a.runtime.cpu.architecture(), arch);
    let status = if let Cpu::M68k(c) = &a.runtime.cpu {
        c.get_sr()
    } else {
        0
    };
    assert_eq!(a.runtime.advance(1)?, Stop::Budget);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    Ok(match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert!(c.dar[0] <= 1);
            assert_eq!(c.dar[1], 0x12345678);
            assert_eq!(c.dar[7], 0xfeedbeef);
            assert_eq!(c.dar[13], 0x76543210);
            assert_eq!(c.dar[15], STACK);
            assert_eq!(c.get_sr(), status);
            c.dar[0] == 0
        }
        Cpu::PowerPc(c) => {
            assert!(c.gpr[3] <= 1);
            assert_eq!(c.gpr[13], 0xfeedbeef);
            assert_eq!(c.gpr[1], STACK);
            assert_eq!(c.gpr[5] & 0xffffff00, 0x12345600);
            c.gpr[3] != 0
        }
    })
}
fn compare(
    a: &mut LoadedApplication,
    arch: Architecture,
    case: bool,
    marks: bool,
    x: &[u8],
    y: &[u8],
) -> bool {
    let left = BASE;
    let right = BASE + 70000;
    if arch == Architecture::PowerPc {
        assert!(x.len() <= 255 && y.len() <= 255);
        let mut b = vec![x.len() as u8];
        b.extend(x);
        a.runtime.memory.write(GuestPtr(left), &b).unwrap();
        let mut b = vec![y.len() as u8];
        b.extend(y);
        a.runtime.memory.write(GuestPtr(right), &b).unwrap();
    } else {
        a.runtime.memory.write(GuestPtr(left), x).unwrap();
        a.runtime.memory.write(GuestPtr(right), y).unwrap();
    }
    prepare(
        a,
        arch,
        case,
        marks,
        (left, x.len() as u16),
        (right, y.len() as u16),
    );
    result(a, arch).unwrap()
}
#[test]
fn both_adapters_observe_all_case_and_mark_modes_and_opposite_result_conventions() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        for case in [false, true] {
            for marks in [false, true] {
                assert_eq!(compare(&mut a, arch, case, marks, b"AbC", b"abc"), !case);
                assert_eq!(compare(&mut a, arch, case, marks, &[0x8e], b"e"), !marks);
                assert_eq!(
                    compare(&mut a, arch, case, marks, &[0x8e], b"E"),
                    !marks && !case
                );
                assert_eq!(compare(&mut a, arch, case, marks, &[0xbe], &[0xae]), !case);
                for (x, y) in [
                    (&[0xae][..], &b"A"[..]),
                    (&[0xce][..], &b"O"[..]),
                    (&[0xa7][..], &b"SS"[..]),
                    (&[0xde][..], &b"fi"[..]),
                ] {
                    assert!(!compare(&mut a, arch, case, marks, x, y));
                }
                assert!(compare(&mut a, arch, case, marks, b"a\0b", b"a\0b"));
                assert!(!compare(&mut a, arch, case, marks, b"a\0b", b"a\0c"));
                assert!(!compare(&mut a, arch, case, marks, b"ab", b"abc"));
            }
        }
    }
}
#[test]
fn documented_roman_strip_and_case_tables_are_shared_and_do_not_expand_ligatures() {
    let strip: &[(u8, &[u8])] = &[
        (b'A', &[0x80, 0x81, 0xcb, 0xcc]),
        (b'C', &[0x82]),
        (b'E', &[0x83]),
        (b'N', &[0x84]),
        (b'O', &[0x85, 0xcd, 0xaf]),
        (b'U', &[0x86]),
        (b'a', &[0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0xbb]),
        (b'c', &[0x8d]),
        (b'e', &[0x8e, 0x8f, 0x90, 0x91]),
        (b'i', &[0x92, 0x93, 0x94, 0x95]),
        (b'n', &[0x96]),
        (b'o', &[0x97, 0x98, 0x99, 0x9a, 0x9b, 0xbf, 0xbc]),
        (b'u', &[0x9c, 0x9d, 0x9e, 0x9f]),
        (b'y', &[0xd8]),
    ];
    let upper = &[
        (0x88, 0xcb),
        (0x8b, 0xcc),
        (0x8a, 0x80),
        (0x8c, 0x81),
        (0xbe, 0xae),
        (0x8d, 0x82),
        (0x8e, 0x83),
        (0x96, 0x84),
        (0x9a, 0x85),
        (0x9b, 0xcd),
        (0xbf, 0xaf),
        (0xcf, 0xce),
        (0x9f, 0x86),
    ];
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        for &(base, group) in strip {
            for &b in group {
                assert!(compare(&mut a, arch, true, false, &[base], &[b]));
                assert!(!compare(&mut a, arch, true, true, &[base], &[b]));
            }
        }
        for &(lower, upper) in upper {
            assert!(compare(&mut a, arch, false, true, &[lower], &[upper]));
            assert!(!compare(&mut a, arch, true, true, &[lower], &[upper]));
        }
        for b in 0..=255u8 {
            assert!(compare(&mut a, arch, true, true, &[b], &[b]));
        }
    }
}
#[test]
fn native_raw_lengths_cover_the_full_register_range_without_pascal_or_nul_truncation() {
    let mut a = app();
    let x = vec![0xa5; 65535];
    let mut y = x.clone();
    assert!(compare(&mut a, Architecture::M68k, true, true, &x, &y));
    y[65534] = 0xa6;
    assert!(!compare(&mut a, Architecture::M68k, true, true, &x, &y));
    // Str255's unsigned prefix is independent of the first character byte.
    let x: Vec<u8> = (0..255).collect();
    let mut y = x.clone();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert!(compare(&mut a, arch, true, true, &x, &y));
    }
    y[254] ^= 1;
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert!(!compare(&mut a, arch, true, true, &x, &y));
    }
}
#[test]
fn zero_length_native_spans_are_not_dereferenced_and_empty_pascal_can_end_at_address_limit() {
    let mut a = app();
    prepare(
        &mut a,
        Architecture::M68k,
        false,
        false,
        (u32::MAX, 0),
        (0, 0),
    );
    assert!(result(&mut a, Architecture::M68k).unwrap());
    let mut a = app();
    a.runtime
        .memory
        .map(
            GuestPtr(u32::MAX),
            vec![0],
            Permissions {
                read: true,
                write: false,
                execute: false,
            },
        )
        .unwrap();
    prepare(
        &mut a,
        Architecture::PowerPc,
        false,
        false,
        (u32::MAX, 0),
        (u32::MAX, 0),
    );
    assert!(result(&mut a, Architecture::PowerPc).unwrap());
}
#[test]
fn invalid_nonempty_spans_fault_before_result_publication_and_do_not_mutate_sources() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        a.runtime.memory.write(GuestPtr(BASE), &[1, b'A']).unwrap();
        a.runtime
            .memory
            .map(GuestPtr(0x52000000), vec![2, b'B'], Permissions::RX)
            .unwrap();
        let right = if arch == Architecture::M68k {
            0x52000001
        } else {
            0x52000000
        };
        prepare(&mut a, arch, true, true, (BASE, 1), (right, 2));
        let before = a.runtime.memory.read(GuestPtr(BASE), 8).unwrap().to_vec();
        assert!(result(&mut a, arch).is_err());
        assert_eq!(a.runtime.memory.read(GuestPtr(BASE), 8).unwrap(), before);
        match &a.runtime.cpu {
            Cpu::M68k(c) => {
                assert_eq!(c.dar[0], 0x00010002);
                assert_eq!(c.dar[8], BASE);
                assert_eq!(c.dar[9], right);
            }
            Cpu::PowerPc(c) => assert_eq!(c.gpr[3], BASE),
        }
    }
}
#[test]
fn callable_os_veneer_preserves_original_modifier_word_from_patch_chain() {
    for (op, equal) in [
        (0xa03c, true),
        (0xa23c, false),
        (0xa43c, true),
        (0xa63c, false),
    ] {
        let mut a = app();
        a.runtime.memory.write(GuestPtr(BASE), &[0x8e]).unwrap();
        a.runtime.memory.write(GuestPtr(BASE + 8), b"e").unwrap();
        let entry = traps::address(op, false);
        assert_ne!(entry, traps::UNIMPLEMENTED);
        assert_eq!(traps::opcode(entry), Some(0xa03c));
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 12))
            .unwrap();
        let lookup = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(Architecture::PowerPc, lookup, STACK);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.gpr[3] = u32::from(op);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::PowerPc(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.gpr[3], entry);
        a.runtime.cpu = Cpu::new(Architecture::M68k, CODE + 8, STACK);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[0] = 0x00010001;
        c.dar[1] = u32::from(op);
        c.dar[8] = BASE;
        c.dar[9] = BASE + 8;
        c.dar[10] = entry;
        assert_eq!(a.runtime.advance(2).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[0], u32::from(!equal));
        assert_eq!(c.dar[1], u32::from(op));
        assert_eq!(c.dar[15], STACK);
        assert_eq!(c.pc, CODE + 10);
    }
}
