mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    process::{Process, HEAP_SIZE},
    runtime::Stop,
};
fn query(a: &mut LoadedApplication, expected: u32) {
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-42i16).to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-43i16).to_be_bytes())
        .unwrap();
    for native in [false, true] {
        let pc = if native {
            0x50000000
        } else {
            let tv = a
                .runtime
                .memory
                .u32(GuestPtr(a.info.toc_or_a5 + 8))
                .unwrap();
            a.runtime.memory.u32(GuestPtr(tv)).unwrap()
        };
        let stack = STACK_BASE + 0x8000;
        a.runtime.cpu = Cpu::new(
            if native {
                Architecture::M68k
            } else {
                Architecture::PowerPc
            },
            pc,
            stack,
        );
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[8] = 0x12345678;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let result = match &a.runtime.cpu {
            Cpu::M68k(c) => {
                assert_eq!(c.dar[15], stack);
                assert_eq!(c.dar[8], 0x12345678);
                c.dar[0]
            }
            Cpu::PowerPc(c) => c.gpr[3],
        };
        assert_eq!(result, expected);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -42);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -43);
    }
}
#[test]
fn free_memory_reports_real_fragmented_extents_and_reclaims_pointer_and_handle_storage() {
    assert_eq!(Process::default().free_memory(), HEAP_SIZE);
    let f = support::pef_program(&[0x4e800020], &[b"FreeMem"], 16);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa0, 0x1c], Permissions::RX)
        .unwrap();
    let initial = a.runtime.process.free_memory();
    query(&mut a, initial);
    let first = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 13, false)
        .unwrap();
    let second = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 17, false)
        .unwrap();
    let third = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 5, false)
        .unwrap();
    let handle = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 9, false)
        .unwrap();
    query(&mut a, initial - 60);
    let tail = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, initial - 60, false)
        .unwrap();
    assert_ne!(tail, 0);
    query(&mut a, 0);
    a.runtime.process.dispose_pointer(first);
    a.runtime.process.dispose_pointer(third);
    query(&mut a, 24);
    assert_eq!(
        a.runtime
            .process
            .new_pointer(&mut a.runtime.memory, 24, false)
            .unwrap(),
        0
    );
    query(&mut a, 24); // Sum exceeds the largest available block (16).
    a.runtime.process.dispose_pointer(tail);
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, handle, 100)
        .unwrap();
    assert_eq!(a.runtime.process.mem_error, 0);
    query(&mut a, initial - 124);
    a.runtime.process.dispose_pointer(second);
    a.runtime
        .process
        .dispose_handle(&mut a.runtime.memory, handle)
        .unwrap();
    query(&mut a, initial);
}
#[test]
fn free_memory_uses_the_actual_code_heap_reservation() {
    let a = LoadedApplication::code(&support::code_program(&[0x4e71])).unwrap();
    let end = a.runtime.memory.u32(GuestPtr(0x114)).unwrap();
    assert!(a.runtime.process.free_memory() <= end - 0x10000);
    assert!(a.runtime.process.free_memory() < HEAP_SIZE);
}
