mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    numerics::Numerics,
    runtime::Stop,
};
fn operands() -> Vec<[u8; 10]> {
    let mut values = Vec::new();
    for exponent in [0u16, 1, 0x3fff, 0x7ffe, 0x7fff] {
        for mantissa in [
            0u64,
            1,
            0x8000000000000000,
            0x8000000000000001,
            0xc123456789abcdef,
            u64::MAX,
        ] {
            for sign in [0, 0x8000] {
                let mut bytes = [0; 10];
                bytes[..2].copy_from_slice(&(exponent | sign).to_be_bytes());
                bytes[2..].copy_from_slice(&mantissa.to_be_bytes());
                values.push(bytes);
            }
        }
    }
    values
}
#[test]
fn sign_operations_preserve_all_payload_bits_and_exception_flags() {
    let mut memory = Memory::default();
    memory
        .map(GuestPtr(0x1000), vec![0x5a; 12], Permissions::RW)
        .unwrap();
    let mut numerics = Numerics {
        invalid: true,
        inexact: true,
        divide_by_zero: true,
        ..Numerics::default()
    };
    let flags = numerics.environment_word();
    for input in operands() {
        for absolute in [false, true] {
            memory.write(GuestPtr(0x1001), &input).unwrap();
            numerics
                .sign_extended(&mut memory, GuestPtr(0x1001), absolute)
                .unwrap();
            let result = memory.read(GuestPtr(0x1001), 10).unwrap();
            assert_eq!(&result[1..], &input[1..]);
            assert_eq!(result[0] & 0x7f, input[0] & 0x7f);
            assert_eq!(
                result[0] >> 7,
                if absolute { 0 } else { 1 - (input[0] >> 7) }
            );
            numerics
                .sign_extended(&mut memory, GuestPtr(0x1001), absolute)
                .unwrap();
            if !absolute {
                assert_eq!(memory.read(GuestPtr(0x1001), 10).unwrap(), input);
            }
            assert_eq!(numerics.environment_word(), flags);
            assert_eq!(memory.read(GuestPtr(0x1000), 1).unwrap(), [0x5a]);
            assert_eq!(memory.read(GuestPtr(0x100b), 1).unwrap(), [0x5a]);
        }
    }
    // Signaling NaNs must also leave an initially clear invalid flag clear.
    let mut clean = Numerics::default();
    memory
        .write(GuestPtr(0x1001), &[0x7f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 1])
        .unwrap();
    clean
        .sign_extended(&mut memory, GuestPtr(0x1001), false)
        .unwrap();
    clean
        .sign_extended(&mut memory, GuestPtr(0x1001), true)
        .unwrap();
    assert_eq!(clean.environment_word(), 0);
}
#[test]
fn native_sign_calls_use_one_pointer_and_pop_six_bytes_with_registers_intact() {
    let f = support::pef_program(&[0x4e800020], &[], 64);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xeb], Permissions::RX)
        .unwrap();
    let stack = STACK_BASE + 0x7000;
    let pointer = STACK_BASE + 0x6001;
    let free = a.runtime.process.free_memory();
    for selector in [0x0du16, 0x0f] {
        for input in operands() {
            let mut frame = selector.to_be_bytes().to_vec();
            frame.extend(pointer.to_be_bytes());
            frame.extend([0xcc; 4]);
            a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
            a.runtime.memory.write(GuestPtr(pointer), &input).unwrap();
            a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
            let Cpu::M68k(c) = &mut a.runtime.cpu else {
                unreachable!()
            };
            for i in 0..15 {
                c.dar[i] = 0x12340000 + i as u32;
            }
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            let Cpu::M68k(c) = &a.runtime.cpu else {
                unreachable!()
            };
            for i in 0..15 {
                assert_eq!(c.dar[i], 0x12340000 + i as u32);
            }
            assert_eq!(c.dar[15], stack + 6);
            assert_eq!(a.runtime.memory.read(GuestPtr(stack), 10).unwrap(), frame);
            let mut expected = input;
            if selector == 0x0d {
                expected[0] ^= 0x80;
            } else {
                expected[0] &= 0x7f;
            }
            assert_eq!(
                a.runtime.memory.read(GuestPtr(pointer), 10).unwrap(),
                expected
            );
            assert_eq!(a.runtime.process.numerics.environment_word(), 0);
        }
    }
    assert_eq!(a.runtime.process.free_memory(), free);
}
#[test]
fn invalid_sign_frames_and_operands_preserve_stack_memory_and_flags() {
    for selector in [0x0du16, 0x0f] {
        for (size, permissions) in [(9, Permissions::RW), (10, Permissions::RX)] {
            let mut memory = Memory::default();
            let mut frame = selector.to_be_bytes().to_vec();
            frame.extend(0x2000u32.to_be_bytes());
            memory
                .map(GuestPtr(0x1000), frame.clone(), Permissions::RW)
                .unwrap();
            memory
                .map(GuestPtr(0x2000), vec![0x5a; size], permissions)
                .unwrap();
            let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
                unreachable!()
            };
            assert!(Abi::trap(0xa9eb, &mut c, &memory).is_err());
            assert_eq!(c.dar[15], 0x1000);
            assert_eq!(memory.read(GuestPtr(0x1000), 6).unwrap(), frame);
            let mut n = Numerics::default();
            assert!(n
                .sign_extended(&mut memory, GuestPtr(0x2000), selector == 0xf)
                .is_err());
            assert_eq!(
                memory.read(GuestPtr(0x2000), size).unwrap(),
                vec![0x5a; size]
            );
            assert_eq!(n.environment_word(), 0);
        }
    }
}
