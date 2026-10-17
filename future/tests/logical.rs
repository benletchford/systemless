mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Call, Cpu},
    integer::Logic,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
#[test]
fn both_cpu_adapters_share_full_width_logical_results_without_initializing_managers_or_errors() {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"BitAnd", b"BitOr", b"BitXor", b"BitNot"],
        32,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            [0xa858u16, 0xa85b, 0xa859, 0xa85a]
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect(),
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-42i16).to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-43i16).to_be_bytes())
        .unwrap();
    for (index, op) in [Logic::And, Logic::Or, Logic::Xor, Logic::Not]
        .into_iter()
        .enumerate()
    {
        for (left, right) in [
            (0, 0),
            (u32::MAX, 0),
            (0x89abcdef, 0x13579bdf),
            (0x80000001, u32::MAX),
        ] {
            let expected = match op {
                Logic::And => left & right,
                Logic::Or => left | right,
                Logic::Xor => left ^ right,
                Logic::Not => !left,
            };
            let stack = STACK_BASE + 0x8000;
            let mut args = Vec::new();
            if op != Logic::Not {
                args.extend(right.to_be_bytes());
            }
            args.extend(left.to_be_bytes());
            a.runtime
                .memory
                .write(GuestPtr(stack), &[0x5a; 16])
                .unwrap();
            a.runtime.memory.write(GuestPtr(stack), &args).unwrap();
            a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000 + index as u32 * 2, stack);
            let Cpu::M68k(c) = &mut a.runtime.cpu else {
                panic!()
            };
            c.dar[0] = 0x12345678;
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            let Cpu::M68k(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(c.dar[15], stack + args.len() as u32);
            assert_eq!(a.runtime.memory.u32(GuestPtr(c.dar[15])).unwrap(), expected);
            assert_eq!(
                a.runtime.memory.u32(GuestPtr(c.dar[15] + 4)).unwrap(),
                0x5a5a5a5a
            );
            let tv = a
                .runtime
                .memory
                .u32(GuestPtr(a.info.toc_or_a5 + 8 + index as u32 * 4))
                .unwrap();
            let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
            a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, stack);
            let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
                panic!()
            };
            c.gpr[3] = left;
            c.gpr[4] = right;
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            let Cpu::PowerPc(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.gpr[3], expected);
            assert_eq!(c.gpr[4], right);
            let expected_call = Call::Logical {
                op,
                left,
                right: if op == Logic::Not { 0 } else { right },
            };
            assert_eq!(
                a.runtime
                    .services
                    .calls
                    .iter()
                    .rev()
                    .take(2)
                    .copied()
                    .collect::<Vec<_>>(),
                [expected_call; 2]
            );
            assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -42);
            assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -43);
        }
    }
    assert!(a.runtime.process.ports.is_none());
}
#[test]
fn logical_native_frames_reject_short_or_readonly_results_before_stack_mutation() {
    for (opcode, size) in [(0xa858, 8), (0xa859, 8), (0xa85a, 4), (0xa85b, 8)] {
        for len in [size - 1, size, size + 3] {
            let mut m = Memory::default();
            m.map(GuestPtr(0x1000), vec![0x5a; len], Permissions::RW)
                .unwrap();
            let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
                panic!()
            };
            assert!(Abi::trap(opcode, &mut c, &m).is_err());
            assert_eq!(c.dar[15], 0x1000);
            assert_eq!(m.read(GuestPtr(0x1000), len).unwrap(), vec![0x5a; len]);
        }
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0x5a; size + 4], Permissions::RX)
            .unwrap();
        let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
            panic!()
        };
        assert!(Abi::trap(opcode, &mut c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
}
