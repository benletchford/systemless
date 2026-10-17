mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"GetItemCmd"], 64);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x4e], Permissions::RX)
        .unwrap();
    a
}
#[test]
fn both_cpus_read_live_command_bytes_as_words_without_changing_menu_or_void_results() {
    let mut a = app();
    let mut bytes = vec![0; 14];
    bytes.extend_from_slice(b"\x01M\x01X\x09\x51\x07\x01\0");
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, bytes.len() as u32, false)
        .unwrap();
    let base = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(base), &bytes).unwrap();
    let out = a.info.toc_or_a5 + 33;
    let stack = STACK_BASE + 0x8000;
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8))
        .unwrap();
    let gate = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        for value in [0u8, b'Q', 0x1b, 0x1c, 0x1d, 0x1e, 0x80, 0xff] {
            bytes[19] = value;
            a.runtime.memory.write(GuestPtr(base), &bytes).unwrap();
            a.runtime
                .memory
                .write(GuestPtr(out - 1), &[0xa5; 4])
                .unwrap();
            let mut frame = out.to_be_bytes().to_vec();
            frame.extend(1u16.to_be_bytes());
            frame.extend(h.to_be_bytes());
            frame.extend([0x5a; 4]);
            a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
            let free = a.runtime.process.free_memory();
            a.runtime.process.mem_error = -77;
            a.runtime.cpu = Cpu::new(
                arch,
                if arch == Architecture::PowerPc {
                    gate
                } else {
                    0x50000000
                },
                stack,
            );
            if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
                c.gpr[3] = h;
                c.gpr[4] = 1;
                c.gpr[5] = out;
            }
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            match &a.runtime.cpu {
                Cpu::PowerPc(c) => assert_eq!(c.gpr[3], h),
                Cpu::M68k(c) => {
                    assert_eq!(c.dar[15], stack + 10);
                    assert_eq!(
                        a.runtime.memory.u32(GuestPtr(stack + 10)).unwrap(),
                        0x5a5a5a5a
                    );
                }
            }
            assert_eq!(
                a.runtime.memory.read(GuestPtr(out - 1), 4).unwrap(),
                [0xa5, 0, value, 0xa5]
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(base), bytes.len()).unwrap(),
                bytes
            );
            assert_eq!(a.runtime.process.free_memory(), free);
            assert_eq!(a.runtime.process.mem_error, -77);
        }
    }
}

fn direct(
    a: &mut LoadedApplication,
    h: u32,
    item: i16,
    out: GuestPtr,
) -> systemless_future::Result<u32> {
    let mut manager = std::mem::take(&mut a.runtime.process.menus);
    let r = manager.call(
        &mut a.runtime.memory,
        &mut a.runtime.process,
        None,
        systemless_future::menus::Op::ReadCommand {
            handle: h,
            item,
            destination: out,
        },
    );
    a.runtime.process.menus = manager;
    r
}
#[test]
fn invalid_outputs_fault_before_stack_changes_or_allocations_on_both_cpus() {
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        for out in [0u32, 0x53000000, 0x53000100] {
            let mut a = app();
            a.runtime
                .memory
                .map(GuestPtr(0x53000000), vec![0xa5; 4], Permissions::RX)
                .unwrap();
            a.runtime
                .memory
                .map(GuestPtr(0x53000100), vec![0xa5], Permissions::RW)
                .unwrap();
            let stack = STACK_BASE + 0x8000;
            let mut frame = out.to_be_bytes().to_vec();
            frame.extend(1u16.to_be_bytes());
            frame.extend(0xdeadbeefu32.to_be_bytes());
            a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
            let tv = a
                .runtime
                .memory
                .u32(GuestPtr(a.info.toc_or_a5 + 8))
                .unwrap();
            let gate = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
            let free = a.runtime.process.free_memory();
            a.runtime.cpu = Cpu::new(
                arch,
                if arch == Architecture::PowerPc {
                    gate
                } else {
                    0x50000000
                },
                stack,
            );
            if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
                c.gpr[3] = 0xdeadbeef;
                c.gpr[4] = 1;
                c.gpr[5] = out;
            }
            assert!(a.runtime.advance(1).is_err());
            match &a.runtime.cpu {
                Cpu::M68k(c) => assert_eq!(c.dar[15], stack),
                Cpu::PowerPc(c) => assert_eq!(c.gpr[3], 0xdeadbeef),
            }
            assert_eq!(a.runtime.process.free_memory(), free);
            assert_eq!(a.runtime.memory.read(GuestPtr(stack), 10).unwrap(), frame);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(0x53000000), 4).unwrap(),
                [0xa5; 4]
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(0x53000100), 1).unwrap(),
                [0xa5]
            );
        }
    }
}
#[test]
fn relocated_menus_errors_and_aliased_output_follow_live_guest_storage() {
    let mut a = app();
    let mut bytes = vec![0; 14];
    bytes.extend_from_slice(b"\x01M\x01X\x09\x51\x07\x01\0");
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, bytes.len() as u32, false)
        .unwrap();
    let old = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(old), &bytes).unwrap();
    let blocker = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 32, false)
        .unwrap();
    assert_ne!(blocker, 0);
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, h, 128)
        .unwrap();
    let base = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    assert_ne!(base, old);
    let out = GuestPtr(a.info.toc_or_a5 + 32);
    a.runtime.memory.write(out, &[0xa5; 4]).unwrap();
    let free = a.runtime.process.free_memory();
    for item in [-1, 0, 2] {
        a.runtime.process.mem_error = -77;
        assert!(direct(&mut a, h, item, out).is_err());
        assert_eq!(a.runtime.process.mem_error, -77);
        assert_eq!(a.runtime.memory.read(out, 4).unwrap(), [0xa5; 4]);
        assert_eq!(a.runtime.process.free_memory(), free);
    }
    a.runtime.memory.write(GuestPtr(base + 6), &[1]).unwrap();
    assert!(direct(&mut a, h, 1, out).is_err());
    assert_eq!(a.runtime.memory.read(out, 4).unwrap(), [0xa5; 4]);
    a.runtime.memory.write(GuestPtr(base + 6), &[0]).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(h), &0xdeadbeefu32.to_be_bytes())
        .unwrap();
    assert!(direct(&mut a, h, 1, out).is_err());
    assert_eq!(a.runtime.memory.read(out, 4).unwrap(), [0xa5; 4]);
    a.runtime
        .memory
        .write(GuestPtr(h), &base.to_be_bytes())
        .unwrap();
    direct(&mut a, h, 1, GuestPtr(base + 19)).unwrap();
    assert_eq!(
        a.runtime.memory.read(GuestPtr(base + 19), 2).unwrap(),
        [0, b'Q']
    );
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(a.runtime.process.mem_error, -77);
    let address = systemless_future::traps::address(0xa84e, true);
    assert_ne!(address, systemless_future::traps::UNIMPLEMENTED);
    assert_eq!(a.runtime.memory.u16(GuestPtr(address)).unwrap(), 0xa84e);
}
