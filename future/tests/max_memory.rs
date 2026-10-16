mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
#[test]
fn maxmem_purges_compacts_and_preserves_native_a0_according_to_trap_flag() {
    for variant in 0..3 {
        let f = support::pef_program(&[0x4e800020], &[b"MaxMem"], 16);
        let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
        a.runtime
            .memory
            .map(
                GuestPtr(0x50000000),
                vec![0xa0, 0x1d, 0xa1, 0x1d],
                Permissions::RX,
            )
            .unwrap();
        let gap = a
            .runtime
            .process
            .new_pointer(&mut a.runtime.memory, 512, false)
            .unwrap();
        let locked = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 32, false)
            .unwrap();
        let data = a.runtime.memory.u32(GuestPtr(locked)).unwrap();
        a.runtime.memory.write(GuestPtr(data), &[0x5a; 32]).unwrap();
        a.runtime
            .process
            .set_handle_state(&a.runtime.memory, locked, 0xc0)
            .unwrap();
        let purged = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 64, false)
            .unwrap();
        a.runtime
            .process
            .set_handle_attribute(&a.runtime.memory, purged, false, true)
            .unwrap();
        a.runtime.process.dispose_pointer(gap);
        a.runtime
            .memory
            .write(GuestPtr(0x220), &(-42i16).to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(0xa60), &(-43i16).to_be_bytes())
            .unwrap();
        let stack = STACK_BASE + 0x7000;
        let output = GuestPtr(stack + 65);
        a.runtime
            .memory
            .write(GuestPtr(output.0 - 1), &[0x5a; 6])
            .unwrap();
        let pc = if variant < 2 {
            0x50000000 + variant * 2
        } else {
            let tv = a
                .runtime
                .memory
                .u32(GuestPtr(a.info.toc_or_a5 + 8))
                .unwrap();
            a.runtime.memory.u32(GuestPtr(tv)).unwrap()
        };
        a.runtime.cpu = Cpu::new(
            if variant < 2 {
                Architecture::M68k
            } else {
                Architecture::PowerPc
            },
            pc,
            stack,
        );
        match &mut a.runtime.cpu {
            Cpu::M68k(c) => {
                c.dar[8] = 0x12345678;
                c.dar[1] = 0x87654321;
            }
            Cpu::PowerPc(c) => {
                c.gpr[3] = output.0;
                c.gpr[4] = 0x87654321;
            }
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let largest = match &a.runtime.cpu {
            Cpu::M68k(c) => {
                assert_eq!(c.dar[15], stack);
                assert_eq!(c.dar[1], 0x87654321);
                assert_eq!(c.dar[8], if variant == 0 { 0x12345678 } else { 0 });
                c.dar[0]
            }
            Cpu::PowerPc(c) => {
                assert_eq!(c.gpr[4], 0x87654321);
                assert_eq!(
                    a.runtime.memory.read(GuestPtr(output.0 - 1), 6).unwrap(),
                    [0x5a, 0, 0, 0, 0, 0x5a]
                );
                c.gpr[3]
            }
        };
        assert_eq!(a.runtime.memory.u32(GuestPtr(purged)).unwrap(), 0);
        assert_eq!(a.runtime.memory.u32(GuestPtr(locked)).unwrap(), data);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(data), 32).unwrap(),
            [0x5a; 32]
        );
        assert!(largest < a.runtime.process.free_memory());
        assert_ne!(
            a.runtime
                .process
                .new_pointer(&mut a.runtime.memory, largest, false)
                .unwrap(),
            0
        );
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -43);
    }
}
#[test]
fn ppc_output_fault_prevents_purging() {
    let f = support::pef_program(&[0x4e800020], &[b"MaxMem"], 16);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 64, false)
        .unwrap();
    a.runtime
        .process
        .set_handle_attribute(&a.runtime.memory, h, false, true)
        .unwrap();
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    let free = a.runtime.process.free_memory();
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x7000);
    if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
        c.gpr[3] = 0xfffffff0;
    }
    assert!(a.runtime.advance(1).is_err());
    assert_eq!(a.runtime.memory.u32(GuestPtr(h)).unwrap(), data);
    assert_eq!(a.runtime.process.free_memory(), free);
}
