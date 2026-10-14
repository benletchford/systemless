mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
#[test]
fn block_move_adapters_share_overlap_and_fetch_writable_code_after_copy() {
    let f = support::pef_program(&[0x4e800020], &[b"BlockMoveData"], 32);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x3000), (0..8).collect(), Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x4000), vec![0xaa; 8], Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa2, 0x2e, 0x70, 0x07],
            Permissions::RWX,
        )
        .unwrap();
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, stack);
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.gpr[3] = 0x3000;
    c.gpr[4] = 0x3002;
    c.gpr[5] = 6;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], 0x3000);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x3000), 8).unwrap(),
        [0, 1, 0, 1, 2, 3, 4, 5]
    );
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[8] = 0x3002;
    c.dar[9] = 0x3000;
    c.dar[0] = 6;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0);
    assert_eq!(c.dar[15], stack);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x3000), 8).unwrap(),
        [0, 1, 2, 3, 4, 5, 4, 5]
    );
    assert!(app
        .runtime
        .memory
        .move_bytes(GuestPtr(0x3000), GuestPtr(0x4000), 8)
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x4000), 8).unwrap(),
        [0xaa; 8]
    );
    assert!(app
        .runtime
        .memory
        .move_bytes(GuestPtr(0x3006), GuestPtr(0x3000), 4)
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x3000), 8).unwrap(),
        [0, 1, 2, 3, 4, 5, 4, 5]
    );
    app.runtime
        .memory
        .move_bytes(GuestPtr(u32::MAX), GuestPtr(u32::MAX), 0)
        .unwrap();
    app.runtime
        .memory
        .move_bytes(GuestPtr(u32::MAX), GuestPtr(u32::MAX), -1)
        .unwrap();
    assert!(app
        .runtime
        .memory
        .move_bytes(GuestPtr(0), GuestPtr(0), 64 * 1024 * 1024 + 1)
        .is_err());
    // Modify the very next instruction through the OS call, not a host code write.
    app.runtime
        .memory
        .write(GuestPtr(0x3000), &[0x70, 0x03])
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[8] = 0x3000;
    c.dar[9] = 0x50000002;
    c.dar[0] = 2;
    assert_eq!(app.runtime.advance(2).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 3);
}
