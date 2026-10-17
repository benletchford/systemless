mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    decimal::{self, Op},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
#[test]
fn decimal_storage_preserves_guards_and_faults_atomically() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0xaa; 32], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0xbb; 12], Permissions::RX)
        .unwrap();
    for (n, expected) in [
        (0, "0"),
        (5, "5"),
        (-1, "-1"),
        (i32::MIN, "-2147483648"),
        (i32::MAX, "2147483647"),
    ] {
        m.write(GuestPtr(0x1000), &[0xaa; 32]).unwrap();
        decimal::call(
            &mut m,
            Op::Format {
                number: n,
                destination: GuestPtr(0x1001),
            },
        )
        .unwrap();
        assert_eq!(m.read(GuestPtr(0x1001), 1).unwrap(), [expected.len() as u8]);
        assert_eq!(
            m.read(GuestPtr(0x1002), expected.len()).unwrap(),
            expected.as_bytes()
        );
        assert_eq!(m.read(GuestPtr(0x1000), 1).unwrap(), [0xaa]);
        assert_eq!(
            m.read(GuestPtr(0x1002 + expected.len() as u32), 1).unwrap(),
            [0xaa]
        );
    }
    let before = m.read(GuestPtr(0x1000), 32).unwrap().to_vec();
    for p in [0x101e, 0x2000, u32::MAX] {
        assert!(decimal::call(
            &mut m,
            Op::Format {
                number: i32::MIN,
                destination: GuestPtr(p)
            }
        )
        .is_err());
        assert_eq!(m.read(GuestPtr(0x1000), 32).unwrap(), before);
    }
    m.write(GuestPtr(0x1000), b"\x03123").unwrap();
    for p in [0x101e, 0x2000, u32::MAX] {
        let snapshot = m.read(GuestPtr(0x1000), 32).unwrap().to_vec();
        assert!(decimal::call(
            &mut m,
            Op::Parse {
                source: GuestPtr(0x1000),
                destination: Some(GuestPtr(p))
            }
        )
        .is_err());
        assert_eq!(m.read(GuestPtr(0x1000), 32).unwrap(), snapshot);
    }
    m.write(GuestPtr(0x1000), &before).unwrap();
    m.write(GuestPtr(0x101f), &[255]).unwrap();
    assert!(decimal::call(
        &mut m,
        Op::Parse {
            source: GuestPtr(0x101f),
            destination: Some(GuestPtr(0x1000))
        }
    )
    .is_err());
    assert_eq!(m.read(GuestPtr(0x1000), 4).unwrap(), &before[..4]);
}
#[test]
fn decimal_parser_follows_roman_nibbles_signs_and_wrapping_with_overlapping_output() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0xaa; 260], Permissions::RW)
        .unwrap();
    for (text, n) in [
        ("", 0),
        ("+", 0),
        ("-0", 0),
        ("055", 55),
        ("2147483648", i32::MIN),
        ("4294967295", -1),
        ("-4294967295", 1),
        ("CAT", 314),
        ("+CAT", 314),
        ("-CAT", -314),
        ("2:", 30),
        ("1 2", 102),
        (" -1", 131),
    ] {
        let mut p = vec![text.len() as u8];
        p.extend(text.as_bytes());
        m.write(GuestPtr(0x1000), &p).unwrap();
        assert_eq!(
            decimal::call(
                &mut m,
                Op::Parse {
                    source: GuestPtr(0x1000),
                    destination: Some(GuestPtr(0x1000))
                }
            )
            .unwrap(),
            n as u32,
            "{text}"
        );
        assert_eq!(m.u32(GuestPtr(0x1000)).unwrap(), n as u32);
    }
    let mut p = vec![255];
    p.extend([0xff; 255]);
    m.write(GuestPtr(0x1000), &p).unwrap();
    let expected = (0..255).fold(0u32, |n, _| n.wrapping_mul(10).wrapping_add(15));
    assert_eq!(
        decimal::call(
            &mut m,
            Op::Parse {
                source: GuestPtr(0x1000),
                destination: None
            }
        )
        .unwrap(),
        expected
    );
}
#[test]
fn decimal_adapters_pop_only_supported_selectors_and_keep_void_results() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0, 0, 0x5a, 0xa5], Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    c.dar[0] = 0x80000000;
    c.dar[8] = 0x2000;
    assert_eq!(
        Abi::trap(0xa9ee, c, &m).unwrap(),
        (
            Call::Decimal(Op::Format {
                number: i32::MIN,
                destination: GuestPtr(0x2000)
            }),
            Reply::None
        )
    );
    assert_eq!(c.dar[15], 0x1002);
    c.dar[15] = 0x1000;
    m.write(GuestPtr(0x1000), &1u16.to_be_bytes()).unwrap();
    assert_eq!(
        Abi::trap(0xa9ee, c, &m).unwrap(),
        (
            Call::Decimal(Op::Parse {
                source: GuestPtr(0x2000),
                destination: None
            }),
            Reply::M68kD0
        )
    );
    for selector in [2u16, 3, 4, 65535] {
        c.dar[15] = 0x1000;
        m.write(GuestPtr(0x1000), &selector.to_be_bytes()).unwrap();
        assert!(Abi::trap(0xa9ee, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
    c.dar[15] = 0x1003;
    assert!(Abi::trap(0xa9ee, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1003);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0x1000);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0x80000000;
    c.gpr[4] = 0x2000;
    assert_eq!(
        Abi::import(Service::NumToString, c, &m).unwrap(),
        (
            Call::Decimal(Op::Format {
                number: i32::MIN,
                destination: GuestPtr(0x2000)
            }),
            Reply::None
        )
    );
    assert_eq!(
        Abi::import(Service::StringToNum, c, &m).unwrap(),
        (
            Call::Decimal(Op::Parse {
                source: GuestPtr(0x80000000),
                destination: Some(GuestPtr(0x2000))
            }),
            Reply::None
        )
    );
}
#[test]
fn real_cpus_share_decimal_services_without_changing_error_globals_or_other_registers() {
    let f = support::pef_program(&[0x4e800020], &[b"NumToString", b"StringToNum"], 0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            support::words(&[0xa9ee, 0x4e71]),
            Permissions::RX,
        )
        .unwrap();
    let sp = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
        .unwrap();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for (index, n) in [(0, i32::MIN), (1, i32::MIN)] {
            app.runtime
                .memory
                .write(GuestPtr(sp), &(index as u16).to_be_bytes())
                .unwrap();
            let pc = if arch == Architecture::M68k {
                0x50000000
            } else {
                let tv = app
                    .runtime
                    .memory
                    .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
                    .unwrap();
                app.runtime.memory.u32(GuestPtr(tv)).unwrap()
            };
            app.runtime.cpu = Cpu::new(arch, pc, sp);
            match &mut app.runtime.cpu {
                Cpu::M68k(c) => {
                    c.dar[0] = if index == 0 { n as u32 } else { 123 };
                    c.dar[8] = 0x300;
                    c.dar[1] = 0xfeedbeef;
                }
                Cpu::PowerPc(c) => {
                    c.gpr[3] = if index == 0 { n as u32 } else { 0x300 };
                    c.gpr[4] = if index == 0 { 0x300 } else { 0x400 };
                    c.gpr[5] = 0xfeedbeef;
                }
            }
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x300), 12).unwrap(),
                b"\x0b-2147483648"
            );
            match &app.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(c.dar[0], n as u32);
                    assert_eq!(c.dar[8], 0x300);
                    assert_eq!(c.dar[1], 0xfeedbeef);
                    assert_eq!(c.dar[15], sp + 2);
                }
                Cpu::PowerPc(c) => {
                    assert_eq!(c.gpr[3], if index == 0 { n as u32 } else { 0x300 });
                    assert_eq!(c.gpr[5], 0xfeedbeef);
                    if index == 1 {
                        assert_eq!(app.runtime.memory.u32(GuestPtr(0x400)).unwrap(), n as u32);
                    }
                }
            }
            assert_eq!(
                app.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
                (-108i16) as u16
            );
            assert_eq!(
                app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(),
                (-192i16) as u16
            );
        }
    }
    assert_eq!(
        app.runtime
            .services
            .calls
            .iter()
            .filter(|c| matches!(c, Call::Decimal(_)))
            .count(),
        4
    );
}
