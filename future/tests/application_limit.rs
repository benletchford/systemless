mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    process::{HEAP_BASE, HEAP_SIZE},
    runtime::Stop,
};
fn native(a: &mut LoadedApplication, limit: u32) -> i32 {
    let stack = STACK_BASE + 0x8000;
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.dar[8] = limit;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[8], limit);
    assert_eq!(c.dar[15], stack);
    c.dar[0] as i32
}
fn install(a: &mut LoadedApplication) {
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa0, 0x2d], Permissions::RX)
        .unwrap();
}
fn ppc(a: &mut LoadedApplication, index: u32, argument: u32) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.gpr[3] = argument;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
#[test]
fn code_heap_is_below_globals_and_allocation_resize_and_disposal_use_that_reservation() {
    let mut a = LoadedApplication::code(&support::code_program(&[0xa9f4])).unwrap();
    let end = a.runtime.memory.u32(GuestPtr(0x908)).unwrap();
    assert_eq!(
        a.runtime
            .process
            .application_limit(&a.runtime.memory)
            .unwrap(),
        end
    );
    assert!(a.runtime.memory.read(GuestPtr(HEAP_BASE), 1).is_err());
    let pointer = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 4, true)
        .unwrap();
    assert_eq!(pointer, 0x10000);
    let handle = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 32, true)
        .unwrap();
    assert!((pointer..end).contains(&handle));
    let data = a.runtime.memory.u32(GuestPtr(handle)).unwrap();
    a.runtime.memory.write(GuestPtr(data), &[0x42; 32]).unwrap();
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, handle, 1024)
        .unwrap();
    assert_eq!(a.runtime.process.mem_error, 0);
    let moved = a.runtime.memory.u32(GuestPtr(handle)).unwrap();
    assert!(moved + 1024 <= end);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(moved), 32).unwrap(),
        &[0x42; 32]
    );
    a.runtime
        .process
        .dispose_handle(&mut a.runtime.memory, handle)
        .unwrap();
    a.runtime.process.dispose_pointer(pointer);
    assert_eq!(
        a.runtime
            .process
            .new_pointer(&mut a.runtime.memory, end - 0x10000, false)
            .unwrap(),
        0x10000
    );
    assert_eq!(
        a.runtime
            .process
            .new_pointer(&mut a.runtime.memory, 1, false)
            .unwrap(),
        0
    );
    assert_eq!(a.runtime.process.mem_error, -108);
}
#[test]
fn native_limit_is_absolute_and_does_not_shrink_or_destroy_an_already_grown_zone() {
    let mut a = LoadedApplication::code(&support::code_program(&[0xa9f4])).unwrap();
    install(&mut a);
    let end = a
        .runtime
        .process
        .application_limit(&a.runtime.memory)
        .unwrap();
    let pointer = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, end - 0x10000, false)
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(end - 4), &[1, 2, 3, 4])
        .unwrap();
    let limit = end - 0x1e000;
    assert_eq!(native(&mut a, limit), 0);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0);
    assert_eq!(
        a.runtime
            .process
            .application_limit(&a.runtime.memory)
            .unwrap(),
        limit
    );
    a.runtime
        .process
        .maximize_heap(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(a.runtime.memory.u32(GuestPtr(0x114)).unwrap(), end);
    assert_eq!(a.runtime.memory.u32(GuestPtr(0x130)).unwrap(), limit);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(end - 4), 4).unwrap(),
        &[1, 2, 3, 4]
    );
    for invalid in [0xffff, end + 1, u32::MAX] {
        assert_eq!(native(&mut a, invalid), -108);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -108);
        assert_eq!(
            a.runtime
                .process
                .application_limit(&a.runtime.memory)
                .unwrap(),
            limit
        );
    }
    a.runtime.process.dispose_pointer(pointer);
    assert_eq!(native(&mut a, end), 0);
}
#[test]
fn native_and_ppc_share_limit_state_and_void_setter_preserves_ppc_return_register() {
    let fixture = support::pef_program(&[0x4e800020], &[b"SetApplLimit", b"GetApplLimit"], 32);
    let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    install(&mut a);
    let end = HEAP_BASE + HEAP_SIZE;
    assert_eq!(native(&mut a, end - 4096), 0);
    assert_eq!(ppc(&mut a, 1, 0), end - 4096);
    assert_eq!(ppc(&mut a, 0, end - 8192), end - 8192);
    assert_eq!(
        a.runtime
            .process
            .application_limit(&a.runtime.memory)
            .unwrap(),
        end - 8192
    );
    assert_eq!(ppc(&mut a, 0, end + 4), end + 4);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -108);
    assert_eq!(ppc(&mut a, 1, 0), end - 8192);
    assert_eq!(native(&mut a, end), 0);
    assert_eq!(ppc(&mut a, 1, 0), end);
}
