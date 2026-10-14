mod support;
use systemless_future::{
    events,
    host_input::Pointer,
    load::LoadedApplication,
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
#[test]
fn pointer_maps_letterboxing_edges_and_rejects_outside_presses() {
    let file = support::pef_program(&[0x4e800020], &[], 8);
    let mut rt = LoadedApplication::pef(&file.bytes, None).unwrap().runtime;
    let mut pointer = Pointer::default();
    pointer.motion(&mut rt.process.input, 0.0, 0.0, (640, 360), (512, 342));
    pointer
        .button(&mut rt.memory, &mut rt.process.input, true)
        .unwrap();
    assert!(!events::available(&rt.memory, 2).unwrap());
    pointer.motion(&mut rt.process.input, 320.0, 180.0, (640, 360), (512, 342));
    assert_eq!(
        (rt.process.input.horizontal, rt.process.input.vertical),
        (256, 171)
    );
    pointer.motion(&mut rt.process.input, 1000.0, -1.0, (640, 360), (512, 342));
    assert_eq!(
        (rt.process.input.horizontal, rt.process.input.vertical),
        (511, 0)
    );
    pointer.motion(&mut rt.process.input, f64::NAN, 0.0, (640, 360), (512, 342));
    pointer
        .button(&mut rt.memory, &mut rt.process.input, true)
        .unwrap();
    assert!(!events::available(&rt.memory, 2).unwrap());
}
#[test]
fn host_mouse_wakes_guest_wait_and_records_button_position_modifiers_and_time() {
    let file = support::pef_program(
        &[
            0x7fc802a6, 0x38600002, 0x38805000, 0x38a00064, 0x38c00000, 0x81820008, 0x800c0000,
            0x804c0004, 0x7c0903a6, 0x4e800421, 0x7fc803a6, 0x4e800020,
        ],
        &[b"WaitNextEvent"],
        12,
    );
    let mut rt = LoadedApplication::pef(&file.bytes, None).unwrap().runtime;
    rt.memory
        .map(GuestPtr(0x5000), vec![0; 16], Permissions::RW)
        .unwrap();
    assert_eq!(rt.advance(100).unwrap(), Stop::Waiting { ticks: 100 });
    rt.elapse_ticks(7).unwrap();
    let mut pointer = Pointer::default();
    pointer.motion(&mut rt.process.input, 320.0, 180.0, (640, 360), (512, 342));
    rt.process.input.modifiers |= 0x300;
    pointer
        .button(&mut rt.memory, &mut rt.process.input, true)
        .unwrap();
    pointer
        .button(&mut rt.memory, &mut rt.process.input, true)
        .unwrap();
    assert_eq!(rt.advance(100).unwrap(), Stop::Returned(1));
    assert_eq!(rt.memory.u16(GuestPtr(0x5000)).unwrap(), 1);
    assert_eq!(rt.memory.u32(GuestPtr(0x5006)).unwrap(), 7);
    assert_eq!(rt.memory.u16(GuestPtr(0x500a)).unwrap(), 171);
    assert_eq!(rt.memory.u16(GuestPtr(0x500c)).unwrap(), 256);
    assert_eq!(rt.memory.u16(GuestPtr(0x500e)).unwrap(), 0x300);
    assert!(!events::available(&rt.memory, 2).unwrap());
    pointer.leave();
    pointer
        .button(&mut rt.memory, &mut rt.process.input, false)
        .unwrap();
    assert!(events::receive(
        &mut rt.memory,
        4,
        GuestPtr(0x5000),
        true,
        7,
        rt.process.input
    )
    .unwrap());
    assert_eq!(rt.memory.u16(GuestPtr(0x500e)).unwrap(), 0x380);
}
#[test]
fn masked_button_changes_state_and_focus_loss_releases_it() {
    let file = support::pef_program(&[0x4e800020], &[], 8);
    let mut rt = LoadedApplication::pef(&file.bytes, None).unwrap().runtime;
    rt.memory
        .write(events::SYSTEM_MASK, &0u16.to_be_bytes())
        .unwrap();
    let mut pointer = Pointer::default();
    pointer.motion(&mut rt.process.input, 1.0, 1.0, (512, 342), (512, 342));
    pointer
        .button(&mut rt.memory, &mut rt.process.input, true)
        .unwrap();
    assert_eq!(rt.process.input.modifiers & 0x80, 0);
    assert!(!events::available(&rt.memory, 0xffff).unwrap());
    rt.process.input.modifiers |= 0x1f00;
    pointer
        .unfocus(&mut rt.memory, &mut rt.process.input)
        .unwrap();
    assert_eq!(rt.process.input.modifiers, 0x80);
}
