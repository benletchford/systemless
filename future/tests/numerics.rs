mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn app(selector: u16, pointer: u32) -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[], 64);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xec], Permissions::RX)
        .unwrap();
    let stack = STACK_BASE + 0x7000;
    let mut frame = selector.to_be_bytes().to_vec();
    frame.extend(pointer.to_be_bytes());
    frame.extend([0xcc; 4]);
    a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        c.dar[i] = 0x12340000 + i as u32;
    }
    a
}
#[test]
fn native_sane_arctangent_updates_ten_bytes_and_preserves_registers_and_frame() {
    let pointer = STACK_BASE + 0x6000;
    let mut a = app(0x1e, pointer);
    let input = [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 1];
    a.runtime.memory.write(GuestPtr(pointer), &input).unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(pointer), 10).unwrap(),
        [0x3f, 0xfe, 0xc9, 0x0f, 0xda, 0xa2, 0x21, 0x68, 0xc2, 0x36]
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
    assert_eq!(c.dar[15], STACK_BASE + 0x7006);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(c.dar[15]), 4).unwrap(),
        [0xcc; 4]
    );
    assert!(a.runtime.process.numerics.inexact);
}
#[test]
fn unsupported_selectors_and_bad_destinations_fault_without_popping_the_frame() {
    for (selector, pointer) in [(0x22, STACK_BASE + 0x6000), (0x1e, 0xfffffff0)] {
        let mut a = app(selector, pointer);
        assert!(a.runtime.advance(1).is_err());
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK_BASE + 0x7000);
        assert!(!a.runtime.process.numerics.inexact);
    }
}
