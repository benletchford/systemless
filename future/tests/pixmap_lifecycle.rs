mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    devices::{self, Op},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let fixture = support::pef_program(
        &[0x4e800020],
        &[b"NewPixMap", b"DisposePixMap", b"DisposPixMap"],
        64,
    );
    let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xaa, 3, 0xaa, 4],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn call(a: &mut LoadedApplication, op: Op) -> systemless_future::Result<u32> {
    let mut d = std::mem::take(&mut a.runtime.process.devices);
    let r = d.call(&mut a.runtime.memory, &mut a.runtime.process, op);
    a.runtime.process.devices = d;
    r
}
fn invoke(a: &mut LoadedApplication, arch: Architecture, index: u32, arg: u32) -> u32 {
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .write(GuestPtr(stack), &[0xa5; 12])
        .unwrap();
    if index != 0 {
        a.runtime
            .memory
            .write(GuestPtr(stack), &arg.to_be_bytes())
            .unwrap();
    }
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let gate = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(
        arch,
        if arch == Architecture::PowerPc {
            gate
        } else {
            0x50000000 + u32::from(index != 0) * 2
        },
        stack,
    );
    if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
        c.gpr[3] = arg;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    match &a.runtime.cpu {
        Cpu::PowerPc(c) => {
            if index != 0 {
                assert_eq!(c.gpr[3], arg);
            }
            c.gpr[3]
        }
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], stack + u32::from(index != 0) * 4);
            assert_eq!(
                a.runtime.memory.u32(GuestPtr(stack + 4)).unwrap(),
                0xa5a5a5a5
            );
            a.runtime.memory.u32(GuestPtr(stack)).unwrap()
        }
    }
}
#[test]
fn both_cpus_clone_the_selected_descriptor_with_owned_table_and_preserve_shared_pixels() {
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        let mut a = app();
        let main = call(&mut a, Op::Current).unwrap();
        let gd = a.runtime.memory.u32(GuestPtr(main)).unwrap();
        let original = a.runtime.memory.u32(GuestPtr(gd + 22)).unwrap();
        let source = a.runtime.memory.u32(GuestPtr(original)).unwrap();
        let mut descriptor = a
            .runtime
            .memory
            .read(GuestPtr(source), 50)
            .unwrap()
            .to_vec();
        let pixels = a
            .runtime
            .process
            .new_pointer(&mut a.runtime.memory, 32, true)
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(pixels), &[0x5a; 32])
            .unwrap();
        descriptor[..4].copy_from_slice(&pixels.to_be_bytes());
        descriptor[22..26].copy_from_slice(&(144u32 << 16).to_be_bytes());
        descriptor[26..30].copy_from_slice(&(96u32 << 16).to_be_bytes());
        let off = call(
            &mut a,
            Op::New {
                reference: 0,
                mode: -1,
            },
        )
        .unwrap();
        let off_data = a.runtime.memory.u32(GuestPtr(off)).unwrap();
        let pm = a.runtime.memory.u32(GuestPtr(off_data + 22)).unwrap();
        let pm_data = a.runtime.memory.u32(GuestPtr(pm)).unwrap();
        let ct = a.runtime.memory.u32(GuestPtr(pm_data + 42)).unwrap();
        descriptor[42..46].copy_from_slice(&ct.to_be_bytes());
        a.runtime
            .memory
            .write(GuestPtr(pm_data), &descriptor)
            .unwrap();
        call(&mut a, Op::Select(off)).unwrap();
        let before = a.runtime.process.free_memory();
        let copy = invoke(&mut a, arch, 0, 0xdeadbeef);
        assert_ne!(copy, 0);
        let data = a.runtime.memory.u32(GuestPtr(copy)).unwrap();
        let table = a.runtime.memory.u32(GuestPtr(data + 42)).unwrap();
        assert_ne!(table, ct);
        let mut expected = descriptor.clone();
        expected[22..26].copy_from_slice(&(72u32 << 16).to_be_bytes());
        expected[26..30].copy_from_slice(&(72u32 << 16).to_be_bytes());
        expected[42..46].copy_from_slice(&table.to_be_bytes());
        assert_eq!(a.runtime.memory.read(GuestPtr(data), 50).unwrap(), expected);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(pm_data), 50).unwrap(),
            descriptor
        );
        assert_eq!(a.runtime.memory.u32(devices::CURRENT).unwrap(), off);
        invoke(
            &mut a,
            arch,
            if arch == Architecture::PowerPc { 2 } else { 1 },
            copy,
        );
        assert_eq!(a.runtime.memory.u32(GuestPtr(copy)).unwrap(), 0);
        assert_eq!(a.runtime.memory.u32(GuestPtr(table)).unwrap(), 0);
        assert_eq!(a.runtime.process.free_memory(), before);
        assert_eq!(a.runtime.process.pointer_size(pixels), 32);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(pixels), 32).unwrap(),
            [0x5a; 32]
        );
        assert_eq!(a.runtime.memory.u32(devices::CURRENT).unwrap(), off);
    }
}

#[test]
fn allocation_failures_reclaim_partial_storage_and_leave_the_device_unchanged() {
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        for remaining in [0, 16, 48] {
            let mut a = app();
            let device = call(&mut a, Op::Current).unwrap();
            let gd = a.runtime.memory.u32(GuestPtr(device)).unwrap();
            let saved = a.runtime.memory.read(GuestPtr(gd), 62).unwrap().to_vec();
            let free = a.runtime.process.free_memory();
            assert_ne!(
                a.runtime
                    .process
                    .new_pointer(&mut a.runtime.memory, free - remaining, false)
                    .unwrap(),
                0
            );
            let before = a.runtime.process.free_memory();
            assert_eq!(invoke(&mut a, arch, 0, 0), 0);
            assert_eq!(a.runtime.process.mem_error, -108);
            assert_eq!(a.runtime.process.free_memory(), before);
            assert_eq!(a.runtime.memory.read(GuestPtr(gd), 62).unwrap(), saved);
            assert_eq!(a.runtime.memory.u32(devices::CURRENT).unwrap(), device);
        }
    }
}
#[test]
fn disposal_rejects_protected_or_corrupt_tables_before_freeing_either_handle() {
    let mut a = app();
    let device = call(&mut a, Op::Current).unwrap();
    let gd = a.runtime.memory.u32(GuestPtr(device)).unwrap();
    let original = a.runtime.memory.u32(GuestPtr(gd + 22)).unwrap();
    let data = a.runtime.memory.u32(GuestPtr(original)).unwrap();
    let protected = a.runtime.memory.u32(GuestPtr(data + 42)).unwrap();
    let main_data = a.runtime.memory.read(GuestPtr(data), 50).unwrap().to_vec();
    assert!(call(&mut a, Op::DisposePixMap(original)).is_err());
    assert_eq!(
        a.runtime.memory.read(GuestPtr(data), 50).unwrap(),
        main_data
    );
    let map = call(&mut a, Op::NewPixMap).unwrap();
    let data = a.runtime.memory.u32(GuestPtr(map)).unwrap();
    let table = a.runtime.memory.u32(GuestPtr(data + 42)).unwrap();
    let table_data = a.runtime.memory.u32(GuestPtr(table)).unwrap();
    for replacement in [protected, map, 0xdeadbeef] {
        a.runtime
            .memory
            .write(GuestPtr(data + 42), &replacement.to_be_bytes())
            .unwrap();
        let before = a.runtime.process.free_memory();
        assert!(call(&mut a, Op::DisposePixMap(map)).is_err());
        assert_eq!(a.runtime.process.free_memory(), before);
        assert_eq!(a.runtime.memory.u32(GuestPtr(map)).unwrap(), data);
        assert_eq!(a.runtime.memory.u32(GuestPtr(table)).unwrap(), table_data);
    }
    a.runtime
        .memory
        .write(GuestPtr(data + 42), &table.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(table), &0xdeadbeefu32.to_be_bytes())
        .unwrap();
    let before = a.runtime.process.free_memory();
    assert!(call(&mut a, Op::DisposePixMap(map)).is_err());
    assert_eq!(a.runtime.process.free_memory(), before);
    assert_eq!(a.runtime.memory.u32(GuestPtr(map)).unwrap(), data);
    a.runtime
        .memory
        .write(GuestPtr(table), &table_data.to_be_bytes())
        .unwrap();
    call(&mut a, Op::DisposePixMap(map)).unwrap();
}
#[test]
fn native_result_preflight_precedes_allocation_and_discovery_veneers_are_callable() {
    use systemless_future::{abi::Abi, traps};
    let mut a = app();
    a.runtime
        .memory
        .map(GuestPtr(0x53000000), vec![0; 4], Permissions::RX)
        .unwrap();
    let Cpu::M68k(mut cpu) = Cpu::new(Architecture::M68k, 0, 0x53000000) else {
        panic!()
    };
    let before = a.runtime.process.free_memory();
    assert!(Abi::trap(0xaa03, &mut cpu, &a.runtime.memory).is_err());
    assert_eq!(cpu.dar[15], 0x53000000);
    assert_eq!(a.runtime.process.free_memory(), before);
    for opcode in [0xaa03, 0xaa04] {
        let address = traps::address(opcode, true);
        assert_ne!(address, traps::UNIMPLEMENTED);
        assert_eq!(a.runtime.memory.u16(GuestPtr(address)).unwrap(), opcode);
    }
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .write(GuestPtr(stack), &[0xa5; 12])
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(stack), &0x50000000u32.to_be_bytes())
        .unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, traps::address(0xaa03, true), stack);
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(cpu) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!((cpu.pc, cpu.dar[15]), (0x50000000, stack + 4));
    let map = a.runtime.memory.u32(GuestPtr(stack + 4)).unwrap();
    assert_ne!(map, 0);
    assert_eq!(
        a.runtime.memory.u32(GuestPtr(stack + 8)).unwrap(),
        0xa5a5a5a5
    );
    a.runtime
        .memory
        .write(GuestPtr(stack + 4), &map.to_be_bytes())
        .unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, traps::address(0xaa04, true), stack);
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(a.runtime.memory.u32(GuestPtr(map)).unwrap(), 0);
    let Cpu::M68k(cpu) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!((cpu.pc, cpu.dar[15]), (0x50000000, stack + 8));
    assert_eq!(
        a.runtime.memory.u32(GuestPtr(stack + 8)).unwrap(),
        0xa5a5a5a5
    );
}
