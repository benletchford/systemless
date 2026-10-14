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
