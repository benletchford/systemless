mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    integer::fixed_ratio,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
    traps,
};
const CASES: &[(i16, i16, i32)] = &[
    (0, 1, 0),
    (1, 2, 32768),
    (1, 3, 21845),
    (2, 3, 43691),
    (1, 6, 10923),
    (-1, 3, -21845),
    (-2, 3, -43691),
    (1, -6, -10923),
    (-1, -6, 10923),
    (-1, i16::MIN, 2),
    (i16::MIN, i16::MIN, 65536),
    (i16::MIN, -1, i32::MAX),
    (i16::MIN, 1, i32::MIN),
    (i16::MAX, -1, -2147418112),
    (0, 0, i32::MAX),
    (1, 0, i32::MAX),
    (-1, 0, i32::MIN),
];
#[test]
fn ratio_obeys_modern_rounding_saturation_and_nearest_rational_error_bounds() {
    for &(n, d, expected) in CASES {
        assert_eq!(fixed_ratio(n, d), expected);
    }
    // Check nearest representable distance without reproducing the division
    // algorithm. Exact half ties cannot occur for these signed-word operands.
    for d in [
        i16::MIN,
        -32767,
        -1025,
        -17,
        -7,
        -6,
        -3,
        -2,
        -1,
        1,
        2,
        3,
        6,
        7,
        17,
        1025,
        32767,
    ] {
        for n in i16::MIN..=i16::MAX {
            let q = fixed_ratio(n, d);
            if n == i16::MIN && d == -1 {
                assert_eq!(q, i32::MAX);
                continue;
            }
            let error = (i64::from(q) * i64::from(d) - i64::from(n) * 65536).abs();
            assert!(error * 2 <= i64::from(d).abs(), "{n}/{d}: {q}");
        }
    }
}
#[test]
fn both_cpu_adapters_preserve_signed_inputs_exact_results_frames_and_error_state() {
    let fixture = support::pef_program(&[0x4e800020], &[b"FixRatio"], 16);
    let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x69], Permissions::RX)
        .unwrap();
    let sp = STACK_BASE + 0x8000;
    let vector = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8))
        .unwrap();
    let gate = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
    a.runtime.process.mem_error = -111;
    a.runtime.process.res_error = -192;
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-43i16).to_be_bytes())
        .unwrap();
    let free = a.runtime.process.free_memory();
    for &(n, d, expected) in CASES {
        let mut frame = d.to_be_bytes().to_vec();
        frame.extend(n.to_be_bytes());
        a.runtime.memory.write(GuestPtr(sp), &[0x5a; 16]).unwrap();
        a.runtime.memory.write(GuestPtr(sp), &frame).unwrap();
        a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, sp);
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[0] = 0x12345678;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            unreachable!()
        };
        assert_eq!(c.dar[0], 0x12345678);
        assert_eq!(c.dar[15], sp + 4);
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(sp + 4)).unwrap(),
            expected as u32
        );
        assert_eq!(a.runtime.memory.u32(GuestPtr(sp + 8)).unwrap(), 0x5a5a5a5a);
        assert_eq!(a.runtime.memory.read(GuestPtr(sp), 4).unwrap(), frame);
        a.runtime.cpu = Cpu::new(Architecture::PowerPc, gate, sp);
        let high_denominator = 0x56780000 | u32::from(d as u16);
        if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
            c.gpr[3] = 0x12340000 | u32::from(n as u16);
            c.gpr[4] = high_denominator;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::PowerPc(c) = &a.runtime.cpu else {
            unreachable!()
        };
        assert_eq!(c.gpr[3], expected as u32);
        assert_eq!(c.gpr[4], high_denominator);
        assert_eq!(
            (a.runtime.process.mem_error, a.runtime.process.res_error),
            (-111, -192)
        );
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -108);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -43);
    }
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(
        a.runtime
            .memory
            .u32(systemless_future::devices::MAIN)
            .unwrap(),
        0
    );
    let entry = traps::lookup(&a.runtime.memory, 0x69, true).unwrap();
    assert_ne!(entry, traps::UNIMPLEMENTED);
    assert_eq!(a.runtime.memory.u16(GuestPtr(entry)).unwrap(), 0xa869);
}
#[test]
fn native_ratio_validates_arguments_and_full_result_before_popping_anything() {
    let mut memory = Memory::default();
    for (address, size, permissions) in [
        (0x1000, 8, Permissions::RX),
        (0x2000, 3, Permissions::RW),
        (0x3000, 6, Permissions::RW),
    ] {
        memory
            .map(GuestPtr(address), vec![0x5a; size], permissions)
            .unwrap();
        let before = memory.read(GuestPtr(address), size).unwrap().to_vec();
        let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, address) else {
            unreachable!()
        };
        assert!(Abi::trap(0xa869, &mut c, &memory).is_err());
        assert_eq!(c.dar[15], address);
        assert_eq!(memory.read(GuestPtr(address), size).unwrap(), before);
    }
}
