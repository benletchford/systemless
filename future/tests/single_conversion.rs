mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
const STACK: u32 = STACK_BASE + 0x7000;
#[test]
fn native_single_conversion_reads_exact_four_bytes_and_preserves_registers() {
    for (input, exponent, mantissa, invalid) in [
        (0u32, 0u16, 0u64, false),
        (0x80000000, 0x8000, 0, false),
        (0x3f800000, 0x3fff, 0x8000000000000000, false),
        (1, 0x3f6a, 0x8000000000000000, false),
        (0x007fffff, 0x3f80, 0xfffffe0000000000, false),
        (0x00800000, 0x3f81, 0x8000000000000000, false),
        (0x7f7fffff, 0x407e, 0xffffff0000000000, false),
        (0x7f800000, 0x7fff, 0x8000000000000000, false),
        (0xff800000, 0xffff, 0x8000000000000000, false),
        (0x7fc12345, 0x7fff, 0xc123450000000000, false),
        (0x7f812345, 0x7fff, 0xc123450000000000, true),
    ] {
        let f = support::pef_program(&[0x4e800020], &[], 32);
        let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
        a.runtime
            .memory
            .map(GuestPtr(0x50000000), vec![0xa9, 0xeb], Permissions::RX)
            .unwrap();
        a.runtime
            .memory
            .map(
                GuestPtr(0x52000000),
                input.to_be_bytes().to_vec(),
                Permissions::RX,
            )
            .unwrap();
        let destination = STACK - 0x100;
        a.runtime
            .memory
            .write(GuestPtr(destination), &[0xa5; 12])
            .unwrap();
        let mut frame = 0x100eu16.to_be_bytes().to_vec();
        frame.extend(destination.to_be_bytes());
        frame.extend(0x52000000u32.to_be_bytes());
        frame.extend([0xcc; 4]);
        a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
        a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, STACK);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        for i in 0..15 {
            c.dar[i] = 0x12340000 + i as u32;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let mut expected = exponent.to_be_bytes().to_vec();
        expected.extend(mantissa.to_be_bytes());
        expected.extend([0xa5; 2]);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(destination), 12).unwrap(),
            expected,
            "{input:08x}"
        );
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        for i in 0..15 {
            assert_eq!(c.dar[i], 0x12340000 + i as u32);
        }
        assert_eq!(c.dar[15], STACK + 10);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(STACK + 10), 4).unwrap(),
            [0xcc; 4]
        );
        assert_eq!(a.runtime.process.numerics.invalid, invalid);
        assert!(!a.runtime.process.numerics.inexact);
    }
}
#[test]
fn conversion_snapshots_overlaps_and_faults_before_flags_or_stack_changes() {
    use systemless_future::numerics::Numerics;
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0xa5; 32], Permissions::RW)
        .unwrap();
    m.write(GuestPtr(0x1000), &0x3f800000u32.to_be_bytes())
        .unwrap();
    let mut state = Numerics::default();
    state
        .single_to_extended(&mut m, GuestPtr(0x1000), GuestPtr(0x1002))
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x1002), 10).unwrap(),
        [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0]
    );
    m.map(
        GuestPtr(0x2000),
        0x7f800001u32.to_be_bytes().to_vec(),
        Permissions::RX,
    )
    .unwrap();
    m.map(GuestPtr(0x3000), vec![0; 10], Permissions::RX)
        .unwrap();
    assert!(state
        .single_to_extended(&mut m, GuestPtr(0x2000), GuestPtr(0x3000))
        .is_err());
    assert!(!state.invalid);
    let mut frame = 0x100eu16.to_be_bytes().to_vec();
    frame.extend(0x3000u32.to_be_bytes());
    frame.extend(0x2000u32.to_be_bytes());
    m.map(GuestPtr(0x4000), frame, Permissions::RW).unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x4000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa9eb, c, &m).is_err());
    assert_eq!(c.dar[15], 0x4000);
}

#[test]
fn single_storage_rounds_once_with_even_ties_subnormals_and_exception_flags() {
    use systemless_future::numerics::Numerics;
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0xa5; 32], Permissions::RW)
        .unwrap();
    for (e, mantissa, expected, inexact, underflow, overflow, invalid) in [
        (
            0x3fffu16,
            0x8000000000000000u64,
            0x3f800000u32,
            false,
            false,
            false,
            false,
        ),
        (
            0x3fff,
            0x8000008000000000,
            0x3f800000,
            true,
            false,
            false,
            false,
        ),
        (
            0x3fff,
            0x8000008000000001,
            0x3f800001,
            true,
            false,
            false,
            false,
        ),
        (
            0x3fff,
            0x8000018000000000,
            0x3f800002,
            true,
            false,
            false,
            false,
        ),
        (0x3f6a, 0x8000000000000000, 1, false, false, false, false),
        (0x3f69, 0x8000000000000000, 0, true, true, false, false),
        (0x3f69, 0x8000000000000001, 1, true, true, false, false),
        (
            0x3f80,
            0xffffff0000000000,
            0x00800000,
            true,
            false,
            false,
            false,
        ),
        (
            0x407f,
            0x8000000000000000,
            0x7f800000,
            true,
            false,
            true,
            false,
        ),
        (0x8000, 0, 0x80000000, false, false, false, false),
        (
            0x7fff,
            0x8000000000000001,
            0x7fc00000,
            false,
            false,
            false,
            true,
        ),
        (
            0xffff,
            0xc123450000000000,
            0xffc12345,
            false,
            false,
            false,
            false,
        ),
    ] {
        let mut n = Numerics::default();
        let mut bytes = e.to_be_bytes().to_vec();
        bytes.extend(mantissa.to_be_bytes());
        m.write(GuestPtr(0x1000), &bytes).unwrap();
        m.write(GuestPtr(0x1010), &[0xa5; 8]).unwrap();
        n.extended_to_single(&mut m, GuestPtr(0x1000), GuestPtr(0x1010))
            .unwrap();
        assert_eq!(
            m.u32(GuestPtr(0x1010)).unwrap(),
            expected,
            "{e:04x}/{mantissa:016x}"
        );
        assert_eq!(m.read(GuestPtr(0x1014), 4).unwrap(), [0xa5; 4]);
        assert_eq!(
            (n.inexact, n.underflow, n.overflow, n.invalid),
            (inexact, underflow, overflow, invalid)
        );
    }
}
#[test]
fn native_single_storage_accepts_exact_four_byte_destination_and_preserves_frame() {
    let f = support::pef_program(&[0x4e800020], &[], 32);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xeb], Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x52000000),
            vec![0x3f, 0xff, 0x80, 0, 0, 0x80, 0, 0, 0, 1],
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x53000000), vec![0xa5; 4], Permissions::RW)
        .unwrap();
    let mut frame = 0x1010u16.to_be_bytes().to_vec();
    frame.extend(0x53000000u32.to_be_bytes());
    frame.extend(0x52000000u32.to_be_bytes());
    frame.extend([0xcc; 4]);
    a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, STACK);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        c.dar[i] = 0x12340000 + i as u32;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.u32(GuestPtr(0x53000000)).unwrap(),
        0x3f800001
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
    assert_eq!(c.dar[15], STACK + 10);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(STACK + 10), 4).unwrap(),
        [0xcc; 4]
    );
    assert!(a.runtime.process.numerics.inexact);
}
