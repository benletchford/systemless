mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::{Process, HEAP_BASE, HEAP_SIZE},
    runtime::Stop,
};
#[test]
fn reserved_slots_are_real_nonrelocatable_storage_and_are_reused() {
    let mut p = Process::default();
    let mut m = Memory::default();
    p.more_masters(&mut m).unwrap();
    assert_eq!(p.mem_error, 0);
    // Consume every byte outside the reserved block: a handle still gets a
    // master pointer from that block, without allocating another four bytes.
    let fill = p.new_pointer(&mut m, HEAP_SIZE - 260, false).unwrap();
    assert_eq!(fill, HEAP_BASE + 256);
    let h = p.new_handle(&mut m, 4, true).unwrap();
    assert_eq!(h, HEAP_BASE);
    assert_eq!(m.u32(GuestPtr(h)).unwrap(), HEAP_BASE + HEAP_SIZE - 4);
    p.dispose_pointer(h);
    assert_eq!(p.mem_error, -111);
    p.dispose_handle(&mut m, h).unwrap();
    assert_eq!(m.u32(GuestPtr(h)).unwrap(), 0);
    assert_eq!(p.new_handle(&mut m, 4, false).unwrap(), h);
    // Further reservation fails without corrupting the live handle.
    let data = m.u32(GuestPtr(h)).unwrap();
    p.more_masters(&mut m).unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(m.u32(GuestPtr(h)).unwrap(), data);
    p.dispose_handle(&mut m, h).unwrap();
    p.dispose_pointer(fill);
    assert_ne!(p.new_pointer(&mut m, HEAP_SIZE - 256, false).unwrap(), 0);
}
#[test]
fn both_cpus_reserve_slots_in_one_process_and_publish_memory_errors() {
    let fixture = support::pef_program(&[0x4e800020], &[b"MoreMasters"], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let ppc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa0, 0x36], Permissions::RX)
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0);
    assert_eq!(c.dar[15], stack);
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, ppc, stack);
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.gpr[3] = 0x12345678;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], 0x12345678); // void C import
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0);
    let fill = app
        .runtime
        .process
        .new_pointer(&mut app.runtime.memory, HEAP_SIZE - 512, false)
        .unwrap();
    assert_eq!(fill, HEAP_BASE + 512);
    // No remaining heap space: OS trap returns memFullErr and publishes MemErr.
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0] as i32, -108);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16,
        -108
    );
}

#[test]
fn all_reserved_slots_are_distinct_and_data_failure_does_not_consume_them() {
    let mut p = Process::default();
    let mut m = Memory::default();
    p.more_masters(&mut m).unwrap();
    assert_eq!(p.new_handle(&mut m, HEAP_SIZE, false).unwrap(), 0);
    assert_eq!(p.mem_error, -108);
    let mut handles = Vec::new();
    for index in 0..64 {
        let h = p.new_handle(&mut m, 4, true).unwrap();
        assert_eq!(h, HEAP_BASE + index * 4);
        assert_eq!(m.u32(GuestPtr(h)).unwrap(), HEAP_BASE + 256 + index * 4);
        handles.push(h);
    }
    // Exhausting the explicit pool retains the existing individual fallback.
    let extra = p.new_handle(&mut m, 4, false).unwrap();
    assert_eq!(extra, HEAP_BASE + 516);
    p.dispose_handle(&mut m, extra).unwrap();
    for h in handles.into_iter().rev() {
        p.dispose_handle(&mut m, h).unwrap();
    }
    assert_eq!(p.new_handle(&mut m, 4, false).unwrap(), HEAP_BASE);
}
