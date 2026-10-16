mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    integer::long_to_fixed,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
    traps,
};
const CASES: &[(i32, u32)] = &[
    (i32::MIN, 0x80000000),
    (-65536, 0x80000000),
    (-32769, 0x80000000),
    (-32768, 0x80000000),
    (-32767, 0x80010000),
    (-1, 0xffff0000),
    (0, 0),
    (1, 0x00010000),
    (32767, 0x7fff0000),
    (32768, 0x7fffffff),
    (65535, 0x7fffffff),
    (65536, 0x7fffffff),
    (i32::MAX, 0x7fffffff),
];
#[test]
fn long_to_fixed_preserves_the_full_exact_integer_range_and_saturates_boundaries() {
    for &(input, expected) in CASES {
        assert_eq!(long_to_fixed(input) as u32, expected);
    }
    for input in i16::MIN..=i16::MAX {
        let output = long_to_fixed(i32::from(input));
        assert_eq!(output >> 16, i32::from(input));
        assert_eq!(output as u16, 0);
    }
}
#[test]
fn both_cpu_adapters_convert_full_signed_long_values_without_state_or_frame_damage() {
    let fixture = support::pef_program(&[0x4e800020], &[b"Long2Fix"], 16);
    let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x3f], Permissions::RX)
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
    for &(input, expected) in CASES {
        a.runtime
            .memory
            .write(GuestPtr(stack), &[0x5a; 16])
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(stack), &input.to_be_bytes())
            .unwrap();
        a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[0] = 0x12345678;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            unreachable!()
        };
        assert_eq!((c.dar[0], c.dar[15]), (0x12345678, stack + 4));
        assert_eq!(a.runtime.memory.u32(GuestPtr(stack + 4)).unwrap(), expected);
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(stack + 8)).unwrap(),
            0x5a5a5a5a
        );
        assert_eq!(a.runtime.memory.u32(GuestPtr(stack)).unwrap(), input as u32);
        a.runtime.cpu = Cpu::new(Architecture::PowerPc, gate, stack);
        if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
            c.gpr[3] = input as u32;
            c.gpr[4] = 0xaabbccdd;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::PowerPc(c) = &a.runtime.cpu else {
            unreachable!()
        };
        assert_eq!((c.gpr[3], c.gpr[4]), (expected, 0xaabbccdd));
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
    let entry = traps::lookup(&a.runtime.memory, 0x3f, true).unwrap();
    assert_ne!(entry, traps::UNIMPLEMENTED);
    assert_eq!(a.runtime.memory.u16(GuestPtr(entry)).unwrap(), 0xa83f);
}
