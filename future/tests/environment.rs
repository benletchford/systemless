mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    environment,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::{HEAP_BASE, HEAP_SIZE},
    runtime::Stop,
};
#[test]
fn environment_version_errors_and_record_permissions_are_explicit() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0xaa; 32], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0xbb; 16], Permissions::RX)
        .unwrap();
    assert_eq!(
        environment::query(&mut m, 0, GuestPtr(0x1000)).unwrap(),
        -5501
    );
    assert_eq!(m.read(GuestPtr(0x1000), 16).unwrap(), [0xaa; 16]);
    assert_eq!(environment::query(&mut m, 1, GuestPtr(0)).unwrap(), -50);
    assert!(environment::query(&mut m, 2, GuestPtr(0x2000)).is_err());
    assert_eq!(m.read(GuestPtr(0x2000), 16).unwrap(), [0xbb; 16]);
    assert_eq!(
        environment::query(&mut m, 3, GuestPtr(0x1000)).unwrap(),
        -5502
    );
    assert_eq!(
        m.read(GuestPtr(0x1000), 16).unwrap(),
        [0, 2, 0, 0, 7, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(m.read(GuestPtr(0x1010), 16).unwrap(), [0xaa; 16]);
}
fn invoke(app: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        panic!()
    };
    for (i, &arg) in args.iter().enumerate() {
        c.gpr[3 + i] = arg;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
#[test]
fn both_cpu_adapters_report_one_profile_and_preserve_the_maximized_heap() {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"SysEnvirons", b"MaxApplZone", b"GetApplLimit"],
        32,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let end = HEAP_BASE + HEAP_SIZE;
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x114)).unwrap(), end);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x130)).unwrap(), end);
    let all = app
        .runtime
        .process
        .new_pointer(&mut app.runtime.memory, HEAP_SIZE, false)
        .unwrap();
    assert_eq!(all, HEAP_BASE);
    app.runtime.memory.write(GuestPtr(all), &[42]).unwrap();
    assert_eq!(invoke(&mut app, 0, &[1, 0x300]), 0);
    let record = app
        .runtime
        .memory
        .read(GuestPtr(0x300), 16)
        .unwrap()
        .to_vec();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa0, 0x90, 0xa0, 0x63],
            Permissions::RX,
        )
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 2;
    c.dar[8] = 0x320;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0);
    assert_eq!(c.dar[8], 0x320);
    assert_eq!(c.dar[15], stack);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x320), 16).unwrap(),
        record
    );
    app.runtime
        .memory
        .write(GuestPtr(0x114), &(end - 4096).to_be_bytes())
        .unwrap();
    assert_eq!(invoke(&mut app, 1, &[0xa55a]), 0xa55a);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x114)).unwrap(), end);
    assert_eq!(invoke(&mut app, 2, &[]), end);
    assert_eq!(app.runtime.memory.read(GuestPtr(all), 1).unwrap(), [42]);
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000002, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0);
    app.runtime.process.dispose_pointer(all);
    assert_ne!(
        app.runtime
            .process
            .new_handle(&mut app.runtime.memory, HEAP_SIZE - 4, false)
            .unwrap(),
        0
    );
}
#[test]
fn foreign_application_limits_fail_without_changing_heap_or_globals() {
    let f = support::pef_program(&[0x4e800020], &[], 0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let end = HEAP_BASE + HEAP_SIZE;
    app.runtime
        .memory
        .write(GuestPtr(0x130), &(end + 4).to_be_bytes())
        .unwrap();
    assert!(app
        .runtime
        .process
        .maximize_heap(&mut app.runtime.memory)
        .is_err());
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x114)).unwrap(), end);
    assert!(app
        .runtime
        .process
        .application_limit(&app.runtime.memory)
        .is_err());
    assert_ne!(
        app.runtime
            .process
            .new_pointer(&mut app.runtime.memory, HEAP_SIZE, false)
            .unwrap(),
        0
    );
}
