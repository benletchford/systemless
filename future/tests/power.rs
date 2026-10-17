mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    numerics::Numerics,
    runtime::Stop,
};
fn v(e: u16, m: u64) -> [u8; 10] {
    let mut b = [0; 10];
    b[..2].copy_from_slice(&e.to_be_bytes());
    b[2..].copy_from_slice(&m.to_be_bytes());
    b
}
const ONE: [u8; 10] = [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0];
fn evaluate(a: [u8; 10], b: [u8; 10]) -> ([u8; 10], Numerics) {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), a.to_vec(), Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), b.to_vec(), Permissions::RX)
        .unwrap();
    let mut n = Numerics::default();
    n.power_extended(&mut m, GuestPtr(0x2000), GuestPtr(0x1000))
        .unwrap();
    assert_eq!(m.read(GuestPtr(0x2000), 10).unwrap(), b);
    (m.read(GuestPtr(0x1000), 10).unwrap().try_into().unwrap(), n)
}
#[test]
fn powers_preserve_extended_low_bits_and_round_general_fractional_results() {
    for (a, b, expected) in [
        (
            v(0x4000, 1 << 63),
            v(0x4000, 0xc000000000000000),
            v(0x4002, 1 << 63),
        ),
        (
            v(0xc000, 1 << 63),
            v(0x4000, 0xc000000000000000),
            v(0xc002, 1 << 63),
        ),
        (
            v(0xc000, 1 << 63),
            v(0xc000, 0xc000000000000000),
            v(0xbffc, 1 << 63),
        ),
        (v(0x4001, 1 << 63), v(0x3ffe, 1 << 63), v(0x4000, 1 << 63)),
        (
            v(0x4000, 1 << 63),
            v(0x3ffe, 1 << 63),
            v(0x3fff, 0xb504f333f9de6484),
        ),
        (
            v(0x3fff, 0x8000000000000001),
            v(0x4000, 1 << 63),
            v(0x3fff, 0x8000000000000002),
        ),
        // (3000001^2)^(3/2) is an exact 65-bit odd integer: an extended tie.
        (
            v(0x402a, 0x82f7a291d8100000),
            v(0x3fff, 0xc000000000000000),
            v(0x403f, 0xbb59aec08abf0220),
        ),
    ] {
        let (out, n) = evaluate(a, b);
        assert_eq!(out, expected, "{a:02x?}^{b:02x?}");
        assert!(!n.invalid && !n.divide_by_zero && !n.overflow && !n.underflow);
    }
}
#[test]
fn documented_domain_signed_zero_infinity_and_nan_rules_are_explicit() {
    let zero = v(0, 0);
    let minus_zero = v(0x8000, 0);
    let inf = v(0x7fff, 1 << 63);
    let minus_inf = v(0xffff, 1 << 63);
    let half = v(0x3ffe, 1 << 63);
    let two = v(0x4000, 1 << 63);
    let three = v(0x4000, 0xc000000000000000);
    for (a, b) in [
        (zero, zero),
        (minus_zero, zero),
        (inf, zero),
        (minus_inf, zero),
        (ONE, inf),
        (v(0xbfff, 1 << 63), inf),
        (v(0xc000, 1 << 63), half),
        (minus_zero, half),
    ] {
        let (out, n) = evaluate(a, b);
        assert_eq!(out, v(0x7fff, 0xc025000000000000));
        assert!(n.invalid);
        assert!(!n.divide_by_zero);
    }
    for (a, b, expected, divzero) in [
        (minus_zero, three, minus_zero, false),
        (minus_zero, two, zero, false),
        (minus_zero, v(0xc000, 0xc000000000000000), minus_inf, true),
        (minus_zero, v(0xc000, 1 << 63), inf, true),
        (zero, v(0xbffe, 1 << 63), inf, true),
        (two, zero, ONE, false),
        (inf, v(0xbffe, 1 << 63), zero, false),
        (half, inf, zero, false),
        (half, minus_inf, inf, false),
        (two, inf, inf, false),
        (two, minus_inf, zero, false),
    ] {
        let (out, n) = evaluate(a, b);
        assert_eq!(out, expected);
        assert_eq!(n.divide_by_zero, divzero);
        assert!(!n.invalid && !n.inexact && !n.overflow && !n.underflow);
    }
    let (out, n) = evaluate(v(0xffff, 0x8000000000001234), ONE);
    assert_eq!(out, v(0xffff, 0xc000000000001234));
    assert!(n.invalid);
    let (out, n) = evaluate(ONE, v(0x7fff, 0xc123456789abcdef));
    assert_eq!(out, v(0x7fff, 0xc123456789abcdef));
    assert!(!n.invalid);
}
#[test]
fn power_gradual_underflow_overflow_aliases_and_faults_preserve_state() {
    let (out, n) = evaluate(v(0x4000, 1 << 63), v(0x400d, 1 << 63));
    assert_eq!(out, v(0x7fff, 1 << 63));
    assert!(n.overflow && n.inexact);
    // 16447 = 2^14+63; 2^-16447 is halfway between zero and minimum denormal.
    let (out, n) = evaluate(v(0x4000, 1 << 63), v(0xc00d, 16447u64 << 49));
    assert_eq!(out, v(0, 0));
    assert!(n.underflow && n.inexact);
    let (out, _) = evaluate(v(0x4000, 1 << 63), v(0xc00d, 16446u64 << 49));
    assert_eq!(out, v(0, 1));
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        v(0x4000, 0xc000000000000000).to_vec(),
        Permissions::RW,
    )
    .unwrap();
    let mut n = Numerics::default();
    n.power_extended(&mut m, GuestPtr(0x1000), GuestPtr(0x1000))
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x1000), 10).unwrap(),
        v(0x4003, 0xd800000000000000)
    );
    m.map(
        GuestPtr(0x2000),
        v(0x7fff, 0x8000000000000001).to_vec(),
        Permissions::RX,
    )
    .unwrap();
    m.map(GuestPtr(0x3000), ONE.to_vec(), Permissions::RX)
        .unwrap();
    let before = (
        n.invalid,
        n.inexact,
        n.underflow,
        n.overflow,
        n.divide_by_zero,
    );
    assert!(n
        .power_extended(&mut m, GuestPtr(0x2000), GuestPtr(0x3000))
        .is_err());
    assert_eq!(
        (
            n.invalid,
            n.inexact,
            n.underflow,
            n.overflow,
            n.divide_by_zero
        ),
        before
    );
}
#[test]
fn real_native_power_uses_two_pointer_frame_and_preserves_registers_and_source() {
    let f = support::pef_program(&[0x4e800020], &[], 32);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    let sp = STACK_BASE + 0x7000;
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xec], Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x52000000),
            v(0x3ffe, 1 << 63).to_vec(),
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x53000000),
            v(0x4000, 1 << 63).to_vec(),
            Permissions::RW,
        )
        .unwrap();
    let mut frame = 0x8012u16.to_be_bytes().to_vec();
    frame.extend(0x53000000u32.to_be_bytes());
    frame.extend(0x52000000u32.to_be_bytes());
    frame.extend([0xa5; 4]);
    a.runtime.memory.write(GuestPtr(sp), &frame).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, sp);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        c.dar[i] = 0x12340000 + i as u32;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x53000000), 10).unwrap(),
        v(0x3fff, 0xb504f333f9de6484)
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
    assert_eq!(c.dar[15], sp + 10);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(sp + 10), 4).unwrap(),
        [0xa5; 4]
    );
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), frame[..9].to_vec(), Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa9ec, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1000);
}
