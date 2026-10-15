mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
#[test]
fn word_extraction_keeps_signed_words_and_exact_native_and_ppc_returns() {
    let f = support::pef_program(&[0x4e800020], &[b"HiWord", b"LoWord"], 16);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    let sp = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa8, 0x6a, 0xa8, 0x6b],
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
        .unwrap();
    a.runtime.process.mem_error = -111;
    for value in [
        0u32, 0xffffffff, 0x80007fff, 0x7fff8000, 0x1234abcd, 0x00020001,
    ] {
        for high in [true, false] {
            let expected = if high {
                (value >> 16) as i16
            } else {
                value as i16
            };
            assert_eq!(systemless_future::integer::word(value, high), expected);
            let index = u32::from(!high);
            let tv = a
                .runtime
                .memory
                .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
                .unwrap();
            let gate = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
            a.runtime.cpu = Cpu::new(Architecture::PowerPc, gate, sp);
            if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
                c.gpr[3] = value;
                c.gpr[4] = 0xaabbccdd;
            }
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            let Cpu::PowerPc(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.gpr[3], expected as i32 as u32);
            assert_eq!(c.gpr[4], 0xaabbccdd);
            let mut frame = value.to_be_bytes().to_vec();
            frame.extend([0xa5; 4]);
            a.runtime.memory.write(GuestPtr(sp), &frame).unwrap();
            a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000 + index * 2, sp);
            if let Cpu::M68k(c) = &mut a.runtime.cpu {
                c.dar[0] = 0x12345678;
            }
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            let Cpu::M68k(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.dar[15], sp + 4);
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(sp + 4)).unwrap(),
                expected as u16
            );
            assert_eq!(a.runtime.memory.u16(GuestPtr(sp + 6)).unwrap(), 0xa5a5);
            assert_eq!(a.runtime.process.mem_error, -111);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
                (-108i16) as u16
            );
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(),
                (-192i16) as u16
            );
        }
    }
    for op in [0x6a, 0x6b] {
        assert_ne!(
            systemless_future::traps::address(op, true),
            systemless_future::traps::UNIMPLEMENTED
        );
    }
}
#[test]
fn word_extraction_faults_before_popping_bad_native_results_or_arguments() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 8], Permissions::RX)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0; 3], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0xfffffffc), vec![0; 4], Permissions::RW)
        .unwrap();
    for trap in [0xa86a, 0xa86b] {
        for sp in [0x1000, 0x2000, 0xfffffffc] {
            let mut c = m68k::CpuCore::default();
            c.dar[15] = sp;
            c.dar[0] = 0x12345678;
            assert!(Abi::trap(trap, &mut c, &m).is_err());
            assert_eq!(c.dar[15], sp);
            assert_eq!(c.dar[0], 0x12345678);
        }
    }
}
