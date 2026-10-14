use systemless_future::{
    abi::{Abi, Reply},
    cpu::{Architecture, Call, Cpu},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::Ports,
    text::{self, Field},
};
#[test]
fn text_fields_preserve_guest_padding_and_reject_invalid_ports_atomically() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1000),
        screen_bitmap: GuestPtr(0),
    };
    m.write(ports.current_slot, &0x1100u32.to_be_bytes())
        .unwrap();
    m.write(GuestPtr(0x1100 + 71), &[0xa5]).unwrap();
    for (field, value) in [
        (Field::Font, 0xff85),
        (Field::Face, 0x0103),
        (Field::Mode, 1),
        (Field::Size, 18),
    ] {
        text::set(&mut m, ports, field, value).unwrap();
    }
    assert_eq!(
        m.read(GuestPtr(0x1100 + 68), 8).unwrap(),
        [0xff, 0x85, 3, 0xa5, 0, 1, 0, 18]
    );
    let saved = m.read(GuestPtr(0x1100), 108).unwrap().to_vec();
    m.map(GuestPtr(0x2000), saved.clone(), Permissions::RX)
        .unwrap();
    m.write(ports.current_slot, &0x2000u32.to_be_bytes())
        .unwrap();
    assert!(text::set(&mut m, ports, Field::Face, 0).is_err());
    assert_eq!(m.read(GuestPtr(0x2000), 108).unwrap(), saved);
    m.write(ports.current_slot, &0x1100u32.to_be_bytes())
        .unwrap();
    m.write(GuestPtr(0x1100 + 92), &1u32.to_be_bytes()).unwrap();
    assert!(text::set(&mut m, ports, Field::Font, 0).is_err());
    assert_eq!(m.u16(GuestPtr(0x1100 + 68)).unwrap(), 0xff85);
    m.write(GuestPtr(0x1100 + 92), &0u32.to_be_bytes()).unwrap();
    m.write(GuestPtr(0x1100 + 6), &0xc000u16.to_be_bytes())
        .unwrap();
    assert!(text::set(&mut m, ports, Field::Size, 12).is_err());
    assert_eq!(m.u16(GuestPtr(0x1100 + 74)).unwrap(), 18);
}
#[test]
fn text_face_uses_word_parameter_and_preserves_stack_tail() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0xa5, 3, 0x5a, 0x5a], Permissions::RW)
        .unwrap();
    let Cpu::M68k(mut cpu) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
        panic!()
    };
    let (call, reply) = Abi::trap(0xa888, &mut cpu, &m).unwrap();
    assert_eq!(
        call,
        Call::TextState {
            field: Field::Face,
            value: 0xa503
        }
    );
    assert_eq!(reply, Reply::None);
    assert_eq!(cpu.dar[15], 0x1002);
    assert_eq!(m.u16(GuestPtr(0x1002)).unwrap(), 0x5a5a);
}
