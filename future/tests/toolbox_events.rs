mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
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
fn toolbox_event_peeking_and_consumption_share_os_records_across_cpu_adapters() {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"PostEvent", b"EventAvail", b"GetNextEvent"],
        32,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert_eq!(invoke(&mut app, 0, &[3, 0x123456]), 0);
    let queue = app
        .runtime
        .memory
        .read(GuestPtr(0x14a), 10)
        .unwrap()
        .to_vec();
    assert_eq!(invoke(&mut app, 1, &[8, 0x300]), 1);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x14a), 10).unwrap(), queue);
    let event = app
        .runtime
        .memory
        .read(GuestPtr(0x300), 16)
        .unwrap()
        .to_vec();
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x302)).unwrap(), 0x123456);
    let stack = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa9, 0x71, 0xa9, 0x70],
            Permissions::RX,
        )
        .unwrap();
    let mut args = 0x320u32.to_be_bytes().to_vec();
    args.extend(8u16.to_be_bytes());
    args.extend([0xa5; 2]);
    app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 6);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 6), 2).unwrap(),
        [1, 0xa5]
    );
    assert_eq!(app.runtime.memory.read(GuestPtr(0x320), 16).unwrap(), event);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x14a), 10).unwrap(), queue);
    assert_eq!(invoke(&mut app, 2, &[8, 0x340]), 1);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x340), 16).unwrap(), event);
    assert_eq!(invoke(&mut app, 1, &[8, 0x360]), 0);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x360)).unwrap(), 0);
}

#[test]
fn activation_slots_are_guest_authority_peeked_then_consumed_without_os_queue_mutation() {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"EventAvail",
            b"GetNextEvent",
            b"OSEventAvail",
            b"WaitNextEvent",
        ],
        32,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa68), &0x11223344u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa64), &0x55667788u32.to_be_bytes())
        .unwrap();
    app.runtime.process.input.modifiers = 0x180;
    let queue = app
        .runtime
        .memory
        .read(GuestPtr(0x14a), 10)
        .unwrap()
        .to_vec();
    assert_eq!(invoke(&mut app, 2, &[1 << 8, 0x300]), 0);
    assert_eq!(invoke(&mut app, 0, &[1 << 8, 0x300]), 1);
    assert_eq!(invoke(&mut app, 0, &[1 << 8, 0x300]), 1);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x300)).unwrap(), 8);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x302)).unwrap(), 0x11223344);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x30e)).unwrap(), 0x180);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa68)).unwrap(), 0x11223344);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x70], Permissions::RX)
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    let mut frame = 0x320u32.to_be_bytes().to_vec();
    frame.extend(0x100u16.to_be_bytes());
    frame.extend([0xa5; 2]);
    app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 6), 2).unwrap(),
        [1, 0xa5]
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa68)).unwrap(), 0);
    // A pending activation wakes the wait immediately, with activeFlag set.
    assert_eq!(invoke(&mut app, 3, &[1 << 8, 0x340, 100, 0]), 1);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x342)).unwrap(), 0x55667788);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x34e)).unwrap(), 0x181);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa64)).unwrap(), 0);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x14a), 10).unwrap(), queue);
    app.runtime
        .memory
        .write(GuestPtr(0xa64), &99u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 16], Permissions::RX)
        .unwrap();
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 12))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = 1 << 8;
        c.gpr[4] = 0x51000000;
    }
    assert!(app.runtime.advance(1).is_err());
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa64)).unwrap(), 99);
}
