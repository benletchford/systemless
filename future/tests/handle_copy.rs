mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const SLOT: u32 = 0x50000000;
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"HandToHand"], 32);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(SLOT), vec![0; 8], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(SLOT + 0x1000), vec![0xa9, 0xe1], Permissions::RX)
        .unwrap();
    a
}
fn copy(a: &mut LoadedApplication, arch: Architecture, source: u32) -> (u32, i16) {
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .write(GuestPtr(SLOT), &source.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
        .unwrap();
    if arch == Architecture::M68k {
        a.runtime.cpu = Cpu::new(arch, SLOT + 0x1000, stack);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        c.dar[8] = source;
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, stack);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        c.gpr[3] = SLOT;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let error = a.runtime.process.mem_error;
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, error);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -192);
    let result = match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[0] as i16, error);
            assert_eq!(c.dar[15], stack);
            c.dar[8]
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[3] as i16, error);
            assert_eq!(c.gpr[1], stack);
            a.runtime.memory.u32(GuestPtr(SLOT)).unwrap()
        }
    };
    (result, error)
}
#[test]
fn both_cpus_duplicate_payload_without_aliasing_or_copying_flags() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let source = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 5, false)
            .unwrap();
        let data = a.runtime.memory.u32(GuestPtr(source)).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(data), &[1, 2, 3, 4, 5])
            .unwrap();
        a.runtime
            .process
            .set_handle_state(&a.runtime.memory, source, 0xe0)
            .unwrap();
        let (duplicate, error) = copy(&mut a, arch, source);
        assert_eq!(error, 0);
        assert_ne!(duplicate, source);
        let target = a.runtime.memory.u32(GuestPtr(duplicate)).unwrap();
        assert_ne!(target, data);
        assert_eq!(
            a.runtime
                .process
                .handle_size(&a.runtime.memory, duplicate)
                .unwrap(),
            5
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(target), 5).unwrap(),
            [1, 2, 3, 4, 5]
        );
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, duplicate)
                .unwrap(),
            0
        );
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, source)
                .unwrap(),
            0xe0
        );
        a.runtime.memory.write(GuestPtr(target), &[9]).unwrap();
        assert_eq!(
            a.runtime.memory.read(GuestPtr(data), 5).unwrap(),
            [1, 2, 3, 4, 5]
        );
        a.runtime
            .process
            .dispose_handle(&mut a.runtime.memory, source)
            .unwrap();
        assert_eq!(
            a.runtime.memory.read(GuestPtr(target), 5).unwrap(),
            [9, 2, 3, 4, 5]
        );
        let zero = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 0, false)
            .unwrap();
        let (z, e) = copy(&mut a, arch, zero);
        assert_eq!(e, 0);
        assert_ne!(z, zero);
        assert_eq!(
            a.runtime.process.handle_size(&a.runtime.memory, z).unwrap(),
            0
        );
    }
}
#[test]
fn bad_empty_and_out_of_memory_sources_do_not_allocate_or_change_source() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let valid = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 16, false)
            .unwrap();
        for (source, error) in [(0, -109), (SLOT + 4, -111)] {
            let free = a.runtime.process.free_memory();
            assert_eq!(copy(&mut a, arch, source), (0, error));
            assert_eq!(a.runtime.process.free_memory(), free);
        }
        // A nil master is an empty handle, not a zero-byte allocated block.
        let original = a.runtime.memory.u32(GuestPtr(valid)).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(valid), &0u32.to_be_bytes())
            .unwrap();
        let free = a.runtime.process.free_memory();
        assert_eq!(copy(&mut a, arch, valid), (0, -109));
        assert_eq!(a.runtime.process.free_memory(), free);
        a.runtime
            .memory
            .write(GuestPtr(valid), &original.to_be_bytes())
            .unwrap();
        let size = a.runtime.process.free_memory() - 64;
        let large = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, size, false)
            .unwrap();
        assert_ne!(large, 0);
        let free = a.runtime.process.free_memory();
        assert_eq!(copy(&mut a, arch, large), (0, -108));
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(
            a.runtime
                .process
                .handle_size(&a.runtime.memory, large)
                .unwrap(),
            size
        );
    }
}
