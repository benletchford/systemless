mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    dialogs::{Dialogs, Op},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::{Process, HEAP_SIZE},
    runtime::Stop,
};
fn ptr(m: &Memory, h: u32) -> GuestPtr {
    GuestPtr(m.u32(GuestPtr(h)).unwrap())
}
fn setup() -> (Memory, Process, Dialogs) {
    let mut m = Memory::default();
    m.map(GuestPtr(0), vec![0; 4096], Permissions::RW).unwrap();
    m.map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    (m, Process::default(), Dialogs::default())
}
#[test]
fn dialog_strings_are_live_pascal_handles_with_atomic_replacement_and_reset() {
    let (mut m, mut p, mut d) = setup();
    m.write(GuestPtr(0xa98), &123u16.to_be_bytes()).unwrap();
    d.call(&mut m, &mut p, Op::Init(0x12345678)).unwrap();
    assert_eq!(m.u32(GuestPtr(0xa8c)).unwrap(), 0x12345678);
    assert_eq!(m.u16(GuestPtr(0xa98)).unwrap(), 123);
    m.write(GuestPtr(0x1000), &[1, b'A']).unwrap();
    m.write(GuestPtr(0x1010), &[1, b'B']).unwrap();
    m.write(GuestPtr(0x1020), &[6, b'l', b'o', b'n', b'g', b'e', b'r'])
        .unwrap();
    d.call(
        &mut m,
        &mut p,
        Op::Param([GuestPtr(0x1000), GuestPtr(0x1010), GuestPtr(0), GuestPtr(0)]),
    )
    .unwrap();
    let h0 = m.u32(GuestPtr(0xaa0)).unwrap();
    let h1 = m.u32(GuestPtr(0xaa4)).unwrap();
    let slots = m.read(GuestPtr(0xaa0), 16).unwrap().to_vec();
    p.set_handle_lock(&m, h1, true).unwrap();
    m.write(GuestPtr(0x1000), &[1, b'C']).unwrap();
    d.call(
        &mut m,
        &mut p,
        Op::Param([
            GuestPtr(0x1000),
            GuestPtr(0x1020),
            GuestPtr(0x1010),
            GuestPtr(0),
        ]),
    )
    .unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(m.read(GuestPtr(0xaa0), 16).unwrap(), slots);
    assert_eq!(m.read(ptr(&m, h0), 2).unwrap(), [1, b'A']);
    assert_eq!(m.read(ptr(&m, h1), 2).unwrap(), [1, b'B']);
    p.set_handle_lock(&m, h1, false).unwrap();
    // A source may alias a parameter handle being replaced; all inputs are frozen first.
    let alias = ptr(&m, h0);
    d.call(
        &mut m,
        &mut p,
        Op::Param([GuestPtr(0x1000), alias, GuestPtr(0), GuestPtr(0)]),
    )
    .unwrap();
    assert_eq!(m.u32(GuestPtr(0xaa0)).unwrap(), h0);
    assert_eq!(m.u32(GuestPtr(0xaa4)).unwrap(), h1);
    assert_eq!(m.read(ptr(&m, h0), 2).unwrap(), [1, b'C']);
    assert_eq!(m.read(ptr(&m, h1), 2).unwrap(), [1, b'A']);
    m.write(GuestPtr(ptr(&m, h0).0 + 1), &[0x80]).unwrap();
    m.map(GuestPtr(0x3000), vec![0xcc; 256], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x4000), vec![0xdd; 256], Permissions::RX)
        .unwrap();
    assert!(d
        .call(
            &mut m,
            &mut p,
            Op::Get([GuestPtr(0x3000), GuestPtr(0x4000), GuestPtr(0), GuestPtr(0)])
        )
        .is_err());
    assert_eq!(m.read(GuestPtr(0x3000), 2).unwrap(), [0xcc; 2]);
    d.call(
        &mut m,
        &mut p,
        Op::Get([GuestPtr(0x3000), GuestPtr(0), GuestPtr(0), GuestPtr(0)]),
    )
    .unwrap();
    assert_eq!(m.read(GuestPtr(0x3000), 2).unwrap(), [1, 0x80]);
    d.call(&mut m, &mut p, Op::Font(-123)).unwrap();
    d.call(&mut m, &mut p, Op::Sound(0x87654321)).unwrap();
    assert_eq!(m.u16(GuestPtr(0xafa)).unwrap(), (-123i16) as u16);
    d.call(&mut m, &mut p, Op::Init(0)).unwrap();
    assert_eq!(m.read(GuestPtr(0xaa0), 16).unwrap(), [0; 16]);
    assert_eq!(m.u32(GuestPtr(0xa9c)).unwrap(), 0);
    assert_eq!(p.handle_size(&m, h0).unwrap(), 0);
    assert_eq!(p.mem_error, -111);
    // The entire heap is available again; no hidden parameter copies remain.
    assert_ne!(p.new_pointer(&mut m, HEAP_SIZE, false).unwrap(), 0);
}
#[test]
fn partial_parameter_allocation_and_unwritable_startup_publish_nothing() {
    let (mut m, mut p, mut d) = setup();
    m.write(GuestPtr(0x1000), &[1, b'A']).unwrap();
    let held = p.new_pointer(&mut m, HEAP_SIZE - 8, false).unwrap();
    assert_ne!(held, 0);
    d.call(&mut m, &mut p, Op::Param([GuestPtr(0x1000); 4]))
        .unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(m.read(GuestPtr(0xaa0), 16).unwrap(), [0; 16]);
    p.dispose_pointer(held);
    assert_ne!(p.new_pointer(&mut m, HEAP_SIZE, false).unwrap(), 0);
    let mut m = Memory::default();
    m.map(GuestPtr(0), vec![0; 0xafa], Permissions::RW).unwrap();
    m.map(GuestPtr(0xafa), vec![1, 2], Permissions::RX).unwrap();
    let before = m.read(GuestPtr(0), 0xafa).unwrap().to_vec();
    let mut p = Process::default();
    let mut d = Dialogs::default();
    assert!(d.call(&mut m, &mut p, Op::Init(123)).is_err());
    assert_eq!(m.read(GuestPtr(0), 0xafa).unwrap(), before);
}
fn invoke(app: &mut LoadedApplication, index: u32, args: &[u32]) {
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
    if let Some(&arg) = args.first() {
        let Cpu::PowerPc(c) = &app.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.gpr[3], arg);
    }
}
#[test]
fn cpu_adapters_share_dialog_globals_and_parameter_handles() {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitDialogs",
            b"GetParamText",
            b"ParamText",
            b"SetDialogFont",
            b"ErrorSound",
        ],
        32,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1000), &[3, b'A', 0x80, b'B'])
        .unwrap();
    invoke(&mut app, 0, &[0x12345678]);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa8c)).unwrap(), 0x12345678);
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa9, 0x8b, 0xa9, 0x7b],
            Permissions::RX,
        )
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    let mut args = vec![0; 12];
    args.extend(0x1000u32.to_be_bytes());
    args.extend([0xa5; 4]);
    app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 16);
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(stack + 16)).unwrap(),
        0xa5a5a5a5
    );
    let h = app.runtime.memory.u32(GuestPtr(0xaa0)).unwrap();
    invoke(&mut app, 1, &[0x1100, 0, 0, 0]);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1100), 4).unwrap(),
        [3, b'A', 0x80, b'B']
    );
    invoke(&mut app, 2, &[0, 0x1100, 0, 0]);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xaa0)).unwrap(), h);
    assert_ne!(app.runtime.memory.u32(GuestPtr(0xaa4)).unwrap(), 0);
    invoke(&mut app, 3, &[0xffffff85]);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0xafa)).unwrap(),
        (-123i16) as u16
    );
    invoke(&mut app, 4, &[0x87654321]);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa9c)).unwrap(), 0x87654321);
    app.runtime
        .memory
        .write(GuestPtr(stack), &0u32.to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000002, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0xaa0), 16).unwrap(),
        [0; 16]
    );
}

#[test]
fn dialog_event_query_uses_boolean_cpu_abis_and_preserves_error_globals() {
    let f = support::pef_program(&[0x4e800020], &[b"IsDialogEvent"], 32);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let event = app.info.toc_or_a5 + 32;
    app.runtime.memory.write(GuestPtr(event), &[0; 16]).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
        .unwrap();
    app.runtime.process.mem_error = -111;
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = event;
        c.gpr[4] = 0x12345678;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], 0);
    assert_eq!(c.gpr[4], 0x12345678);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x7f], Permissions::RX)
        .unwrap();
    let sp = STACK_BASE + 0x8000;
    let mut frame = event.to_be_bytes().to_vec();
    frame.extend([0xa5; 4]);
    app.runtime.memory.write(GuestPtr(sp), &frame).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, sp);
    if let Cpu::M68k(c) = &mut app.runtime.cpu {
        c.dar[0] = 0x12345678;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], sp + 4);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(sp + 4), 4).unwrap(),
        [0, 0xa5, 0xa5, 0xa5]
    );
    assert_eq!(app.runtime.process.mem_error, -111);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
        (-108i16) as u16
    );
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(),
        (-192i16) as u16
    );
    assert_ne!(
        systemless_future::traps::address(0x17f, true),
        systemless_future::traps::UNIMPLEMENTED
    );
    // A truncated guest EventRecord faults without pretending it is a document event.
    app.runtime
        .memory
        .write(GuestPtr(sp), &0xfffffff8u32.to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, sp);
    assert!(app.runtime.advance(1).is_err());
}

#[test]
fn raw_item_text_handles_use_pascal_boundaries_frozen_aliases_flags_and_atomic_growth() {
    let (mut m, mut p, mut d) = setup();
    let h = p.new_handle(&mut m, 4, false).unwrap();
    let data = ptr(&m, h);
    m.write(data, b"\x03ABC").unwrap();
    p.set_handle_state(&m, h, 0xc0).unwrap();
    p.res_error = -52;
    d.item_text(&mut m, &mut p, None, h, data, true).unwrap();
    assert_eq!(p.handle_size(&m, h).unwrap(), 3);
    assert_eq!(m.read(ptr(&m, h), 3).unwrap(), b"ABC");
    assert_eq!(p.handle_state(&m, h).unwrap(), 0xc0);
    assert_eq!(p.res_error, -52);
    m.map(GuestPtr(0x2000), vec![0xcc; 258], Permissions::RW)
        .unwrap();
    p.mem_error = -51;
    d.item_text(&mut m, &mut p, None, h, GuestPtr(0x2001), false)
        .unwrap();
    assert_eq!(m.read(GuestPtr(0x2001), 4).unwrap(), b"\x03ABC");
    assert_eq!(p.mem_error, -51);
    assert_eq!(m.read(GuestPtr(0x2000), 1).unwrap(), [0xcc]);
    assert_eq!(m.read(GuestPtr(0x2005), 1).unwrap(), [0xcc]);
    m.map(GuestPtr(0x3000), vec![0xdd; 256], Permissions::RX)
        .unwrap();
    assert!(d
        .item_text(&mut m, &mut p, None, h, GuestPtr(0x3000), false)
        .is_err());
    assert_eq!(m.read(GuestPtr(0x3000), 4).unwrap(), [0xdd; 4]);
    assert_eq!(p.mem_error, -51);
    let mut long = vec![255];
    long.extend([0x80; 255]);
    m.write(GuestPtr(0x1000), &long).unwrap();
    let free = p.free_memory();
    let held = p.new_pointer(&mut m, free - 1, false).unwrap();
    assert_ne!(held, 0);
    let old = ptr(&m, h);
    let remaining = p.free_memory();
    d.item_text(&mut m, &mut p, None, h, GuestPtr(0x1000), true)
        .unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(ptr(&m, h), old);
    assert_eq!(m.read(old, 3).unwrap(), b"ABC");
    assert_eq!(p.free_memory(), remaining);
    p.dispose_pointer(held);
    p.set_handle_state(&m, h, 0).unwrap();
    d.item_text(&mut m, &mut p, None, h, GuestPtr(0x1000), true)
        .unwrap();
    assert_eq!(p.handle_size(&m, h).unwrap(), 255);
    assert_eq!(m.read(ptr(&m, h), 255).unwrap(), [0x80; 255]);
    p.set_handle_size(&mut m, h, 300).unwrap();
    m.write(ptr(&m, h), &[0x80; 300]).unwrap();
    d.item_text(&mut m, &mut p, None, h, GuestPtr(0x2001), false)
        .unwrap();
    assert_eq!(m.read(GuestPtr(0x2001), 256).unwrap(), long);
    assert_eq!(m.read(GuestPtr(0x2101), 1).unwrap(), [0xcc]);
    m.map(GuestPtr(0x5000), vec![255], Permissions::RW).unwrap();
    let free = p.free_memory();
    assert!(d
        .item_text(&mut m, &mut p, None, h, GuestPtr(0x5000), true)
        .is_err());
    assert_eq!(p.free_memory(), free);
    assert_eq!(p.handle_size(&m, h).unwrap(), 300);
}
