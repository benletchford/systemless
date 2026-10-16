mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    integer::fixed_multiply,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
    traps,
};
const CASES: &[(i32, i32, u32)] = &[
    (0, i32::MIN, 0),
    (65536, 65536, 65536),
    (1, 32767, 0),
    (1, 32768, 1),
    (1, 32769, 1),
    (-1, 32767, 0),
    (-1, 32768, 0xffffffff),
    (-1, 32769, 0xffffffff),
    (i32::MIN, 65536, 0x80000000),
    (i32::MIN, -65536, 0x7fffffff),
    (i32::MIN, i32::MIN, 0x7fffffff),
    (i32::MAX, i32::MAX, 0x7fffffff),
    (i32::MAX, i32::MIN, 0x80000000),
    (i32::MAX, 2, 65536),
    (0x12345678, 65536, 0x12345678),
    (-65536, 65536, 0xffff0000),
];
#[test]
fn multiplication_rounds_signed_halves_and_saturates_overflow() {
    for &(left, right, expected) in CASES {
        assert_eq!(fixed_multiply(left, right) as u32, expected);
        assert_eq!(fixed_multiply(right, left) as u32, expected);
    }
    for left in i16::MIN..=i16::MAX {
        for right in [1, -1, 32767, 32768, 32769, 65535, 65536, -65536, 131071] {
            let product = i64::from(left) * i64::from(right);
            let result = i64::from(fixed_multiply(i32::from(left), right));
            assert!((result * 65536 - product).abs() * 2 <= 65536);
            assert_eq!(fixed_multiply(i32::from(left), 65536), i32::from(left));
        }
    }
}
#[test]
fn both_cpu_adapters_convert_full_signed_long_values_without_state_or_frame_damage() {
    let fixture = support::pef_program(&[0x4e800020], &[b"FixMul"], 16);
    let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x68], Permissions::RX)
        .unwrap();
    let vector = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8))
        .unwrap();
    let gate = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
    let free = a.runtime.process.free_memory();
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
    for &(left, right, expected) in CASES {
        a.runtime
            .memory
            .write(GuestPtr(stack), &[0x5a; 16])
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(stack), &right.to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(stack + 4), &left.to_be_bytes())
            .unwrap();
        a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[0] = 0x12345678;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            unreachable!()
        };
        assert_eq!((c.dar[0], c.dar[15]), (0x12345678, stack + 8));
        assert_eq!(a.runtime.memory.u32(GuestPtr(stack + 8)).unwrap(), expected);
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(stack + 12)).unwrap(),
            0x5a5a5a5a
        );
        assert_eq!(a.runtime.memory.u32(GuestPtr(stack)).unwrap(), right as u32);
        a.runtime.cpu = Cpu::new(Architecture::PowerPc, gate, stack);
        if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
            c.gpr[3] = left as u32;
            c.gpr[4] = right as u32;
            c.gpr[5] = 0xaabbccdd;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::PowerPc(c) = &a.runtime.cpu else {
            unreachable!()
        };
        assert_eq!(
            (c.gpr[3], c.gpr[4], c.gpr[5]),
            (expected, right as u32, 0xaabbccdd)
        );
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
    let entry = traps::lookup(&a.runtime.memory, 0x68, true).unwrap();
    assert_ne!(entry, traps::UNIMPLEMENTED);
    assert_eq!(a.runtime.memory.u16(GuestPtr(entry)).unwrap(), 0xa868);
}

#[test]
fn native_multiply_preflights_arguments_and_result_without_mutation() {
    for (address, size, permissions) in [
        (0x1000, 12, Permissions::RX),
        (0x2000, 7, Permissions::RW),
        (0x3000, 10, Permissions::RW),
    ] {
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(address), vec![0x5a; size], permissions)
            .unwrap();
        let before = memory.read(GuestPtr(address), size).unwrap().to_vec();
        let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, address) else {
            unreachable!()
        };
        assert!(Abi::trap(0xa868, &mut c, &memory).is_err());
        assert_eq!(c.dar[15], address);
        assert_eq!(memory.read(GuestPtr(address), size).unwrap(), before);
    }
}
