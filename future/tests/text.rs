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
    m.write(GuestPtr(0x1100 + 6), &0x8000u16.to_be_bytes())
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

mod support;
#[test]
#[ignore = "requires a real wgpu adapter"]
fn glyph_drawing_uses_gpu_transfer_modes_clip_and_preserves_port_and_padding() {
    use systemless_future::{
        graphics::{Graphics, Rect},
        load::resource::ResourceFork,
        process::Process,
        text::Input,
    };
    let words = [
        0x9000u16, 65, 66, 4, 0xfffe, 0xffff, 4, 3, 12, 2, 1, 0xffff, 1, 0xe400, 0xba00, 0x4600, 0,
        3, 5, 7, 0x0104, 0xffff, 0x0203, 0xffff,
    ];
    let font: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let fork = support::resource(vec![(*b"FONT", 393, None, font)]);
    let mut process = Process::default();
    process.install_application_resources(ResourceFork::parse(&fork).unwrap());
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0; 8], Permissions::RW)
        .unwrap();
    let bitmap = [0, 0, 0x20, 0, 0, 2, 0, 0, 0, 0, 0, 4, 0, 8];
    m.write(GuestPtr(0x1000), &bitmap).unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1020),
        screen_bitmap: GuestPtr(0x1000),
    };
    let port = GuestPtr(0x1080);
    ports.open(&mut m, &mut process, port).unwrap();
    ports.clip_rect(&mut m, Rect::new(1, 0, 3, 3)).unwrap();
    m.write(GuestPtr(port.0 + 68), &[0, 3, 0, 0xa5, 0, 0, 0, 9])
        .unwrap();
    m.write(GuestPtr(port.0 + 56), &10u16.to_be_bytes())
        .unwrap();
    m.write(GuestPtr(port.0 + 58), &[0xa5; 8]).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap();
    let before = [0x5a, 0x33, 0x5a, 0x44, 0x5a, 0x55, 0x5a, 0x66];
    for mode in 0..8u16 {
        m.write(GuestPtr(0x2000), &before).unwrap();
        m.write(GuestPtr(port.0 + 72), &mode.to_be_bytes()).unwrap();
        text::move_to(&mut m, ports, 1, 2).unwrap();
        text::draw(&mut m, &mut process, ports, &gpu, Input::Character(b'A')).unwrap();
        let mut expected = before;
        for (row, src) in [0xe0u8, 0xa0, 0x40].into_iter().enumerate() {
            let src = if mode & 4 != 0 { !src } else { src };
            let d = before[row * 2];
            let transformed = match mode & 3 {
                0 => src,
                1 => d | src,
                2 => d ^ src,
                _ => d & !src,
            };
            expected[row * 2] = (d & !0x60) | (transformed & 0x60);
        }
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            expected,
            "mode {mode}"
        );
        assert_eq!(m.u16(GuestPtr(port.0 + 50)).unwrap(), 5);
        assert_eq!(m.u16(GuestPtr(port.0 + 48)).unwrap(), 2);
        assert_eq!(m.u16(GuestPtr(port.0 + 56)).unwrap(), 10);
        assert_eq!(m.read(GuestPtr(port.0 + 58), 8).unwrap(), [0xa5; 8]);
        assert_eq!(m.read(GuestPtr(port.0 + 71), 1).unwrap(), [0xa5]);
    }
    let bytes = m.read(GuestPtr(0x2000), 8).unwrap().to_vec();
    m.write(GuestPtr(port.0 + 72), &8u16.to_be_bytes()).unwrap();
    let state = m.read(port, 108).unwrap().to_vec();
    assert!(text::draw(&mut m, &mut process, ports, &gpu, Input::Character(b'A')).is_err());
    assert_eq!(m.read(port, 108).unwrap(), state);
    assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), bytes);
    m.write(GuestPtr(port.0 + 72), &0u16.to_be_bytes()).unwrap();
    text::move_to(&mut m, ports, 32767, 2).unwrap();
    assert!(text::draw(&mut m, &mut process, ports, &gpu, Input::Character(b'A')).is_err());
    assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), bytes);
    text::move_to(&mut m, ports, 1, 2).unwrap();
    m.map(GuestPtr(0x3000), bytes.clone(), Permissions::RX)
        .unwrap();
    m.write(GuestPtr(port.0 + 2), &0x3000u32.to_be_bytes())
        .unwrap();
    let state = m.read(port, 108).unwrap().to_vec();
    assert!(text::draw(&mut m, &mut process, ports, &gpu, Input::Character(b'A')).is_err());
    assert_eq!(m.read(port, 108).unwrap(), state);
    assert_eq!(m.read(GuestPtr(0x3000), 8).unwrap(), bytes);
}

#[test]
fn text_buffer_adapter_marshals_reversed_pascal_words_and_validates_result() {
    let mut m = Memory::default();
    let mut frame = 3i16.to_be_bytes().to_vec();
    frame.extend((-2i16).to_be_bytes());
    frame.extend(0x12345678u32.to_be_bytes());
    frame.extend([0x5a; 4]);
    m.map(GuestPtr(0x1000), frame, Permissions::RW).unwrap();
    let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
        panic!()
    };
    let (call, reply) = Abi::trap(0xa886, &mut c, &m).unwrap();
    assert_eq!(
        call,
        Call::TextWidth(text::Input::Buffer {
            pointer: GuestPtr(0x12345678),
            first: -2,
            count: 3
        })
    );
    assert_eq!(reply, Reply::StackWord(GuestPtr(0x1008)));
    assert_eq!(c.dar[15], 0x1008);
    assert_eq!(m.u16(GuestPtr(0x100a)).unwrap(), 0x5a5a);
    let mut read_only = Memory::default();
    read_only
        .map(
            GuestPtr(0x1000),
            m.read(GuestPtr(0x1000), 12).unwrap().to_vec(),
            Permissions::RX,
        )
        .unwrap();
    let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
        panic!()
    };
    assert!(Abi::trap(0xa886, &mut c, &read_only).is_err());
    assert_eq!(c.dar[15], 0x1000);
}

#[test]
fn shared_cells_keep_bearings_opaque_spacing_order_and_pen() {
    use systemless_future::{font::Strike, graphics::Rect};
    let words = [
        0x9000u16, 65, 66, 4, 0xfffe, 0xffff, 4, 3, 12, 2, 1, 0xffff, 1, 0xe400, 0xba00, 0x4600, 0,
        3, 5, 7, 0x0104, 0xffff, 0x0203, 0xffff,
    ];
    let data: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let font = Strike::parse(&data).unwrap();
    let (cells, pen) = text::plan_cells(&font, b"AA", 1, 2).unwrap();
    assert_eq!(pen, 9);
    assert_eq!(cells[0].bounds, Rect::new(0, 0, 5, 3));
    assert_eq!(cells[1].bounds, Rect::new(4, 0, 9, 3));
    assert_eq!(cells[0].mask, [1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
    assert_eq!(cells[0].mask, cells[1].mask);
    assert!(text::plan_cells(&font, b"A", i16::MAX, 2).is_err());
    assert!(text::plan_cells(&font, b"A", 1, i16::MIN).is_err());
    assert_eq!(text::plan_cells(&font, b"", 12, 2).unwrap(), (vec![], 12));
}
