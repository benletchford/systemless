mod support;
use systemless_future::{
    abi::Service,
    cpu::{Architecture, Call, Cpu},
    events::{self, Input, QUEUE_HEADER, SYSTEM_MASK},
    load::LoadedApplication,
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
fn memory() -> Memory {
    let mut memory = LoadedApplication::code(&support::code_program(&[0xa9f4]))
        .unwrap()
        .runtime
        .memory;
    memory
        .map(GuestPtr(0x5000), vec![0; 16], Permissions::RW)
        .unwrap();
    memory
}
#[test]
fn queue_capacity_masks_guest_authority_and_event_record_layout() {
    let mut m = memory();
    let input = Input {
        vertical: -12,
        horizontal: 45,
        modifiers: 0x180,
    };
    assert_eq!(events::post(&mut m, 4, 1, 2, input).unwrap(), 1);
    assert_eq!(events::post(&mut m, 6, 1, 2, input).unwrap(), 1);
    assert_eq!(m.u32(GuestPtr(QUEUE_HEADER.0 + 2)).unwrap(), 0);
    m.write(SYSTEM_MASK, &0xffffu16.to_be_bytes()).unwrap();
    for i in 0..21 {
        assert_eq!(events::post(&mut m, 4, i, 100 + i, input).unwrap(), 0);
    }
    let head = m.u32(GuestPtr(QUEUE_HEADER.0 + 2)).unwrap();
    assert_eq!(m.u16(GuestPtr(head + 4)).unwrap(), 4);
    // Guest queue records, including guest changes, are the source of truth.
    m.write(GuestPtr(head + 8), &99u32.to_be_bytes()).unwrap();
    assert!(events::receive(&mut m, 16, GuestPtr(0x5000), false, 500, input).unwrap());
    assert_eq!(
        m.read(GuestPtr(0x5000), 16).unwrap(),
        &[0, 4, 0, 0, 0, 99, 0, 0, 0, 101, 0xff, 0xf4, 0, 45, 1, 0x80]
    );
    for i in 1..21 {
        assert!(events::receive(&mut m, 16, GuestPtr(0x5000), true, 500, input).unwrap());
        assert_eq!(
            m.u32(GuestPtr(0x5002)).unwrap(),
            if i == 1 { 99 } else { i }
        );
    }
    assert!(!events::receive(&mut m, 0xffff, GuestPtr(0x5000), true, 500, input).unwrap());
    assert_eq!(m.u16(GuestPtr(0x5000)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(0x5006)).unwrap(), 500);
    assert_eq!(m.u32(GuestPtr(QUEUE_HEADER.0 + 6)).unwrap(), 0);
}
#[test]
fn flush_stops_before_matching_event_and_failed_output_does_not_consume() {
    let mut m = memory();
    let input = Input::default();
    for (kind, msg) in [(3, 1), (7, 2), (3, 3)] {
        events::post(&mut m, kind, msg, 0, input).unwrap();
    }
    assert_eq!(events::flush(&mut m, 8, 128).unwrap(), 7);
    let head = m.u32(GuestPtr(QUEUE_HEADER.0 + 2)).unwrap();
    assert!(events::receive(&mut m, 8, GuestPtr(0x5001), true, 0, input).is_err());
    assert!(events::receive(&mut m, 8, GuestPtr(0x5000), true, 0, input).unwrap());
    assert_eq!(m.u32(GuestPtr(0x5002)).unwrap(), 3);
    assert_eq!(m.u32(GuestPtr(QUEUE_HEADER.0 + 2)).unwrap(), head);
    m.write(GuestPtr(head), &head.to_be_bytes()).unwrap();
    let before = m.read(GuestPtr(0x5000), 16).unwrap().to_vec();
    assert!(events::receive(&mut m, 128, GuestPtr(0x5000), true, 0, input).is_err());
    assert_eq!(m.read(GuestPtr(0x5000), 16).unwrap(), before);
}
#[test]
fn event_posted_by_68k_is_consumed_by_ppc_in_one_loaded_process() {
    let mut app = LoadedApplication::code(&support::code_program(&[
        0x207c, 0, 3, 0x203c, 0x1122, 0x3344, 0xa02f, 0xa9f4,
    ]))
    .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x5000), vec![0; 16], Permissions::RW)
        .unwrap();
    for _ in 0..8 {
        if !app.runtime.services.calls.is_empty() {
            break;
        }
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    }
    assert!(matches!(
        app.runtime.services.calls.as_slice(),
        [Call::PostEvent { what: 3, .. }]
    ));
    app.runtime
        .abi
        .imports
        .insert(0x70000000, Service::GetOSEvent);
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, 0x70000000, 0x0080fff0);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = 8;
        c.gpr[4] = 0x5000;
    }
    assert_eq!(app.runtime.advance(2).unwrap(), Stop::Returned(1));
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(0x5002)).unwrap(),
        0x11223344
    );
    assert!(matches!(
        app.runtime.services.calls.as_slice(),
        [
            Call::PostEvent { what: 3, .. },
            Call::OSEvent { remove: true, .. }
        ]
    ));
    assert_eq!(
        app.runtime
            .memory
            .u32(GuestPtr(QUEUE_HEADER.0 + 2))
            .unwrap(),
        0
    );
}
