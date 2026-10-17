mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    graphics::{Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::RectangleOp,
    runtime::Stop,
};
#[test]
fn rectangle_abis_have_void_results_and_fill_uses_two_pascal_pointers() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0, 0, 0x30, 0, 0, 0, 0x20, 0],
        Permissions::RX,
    )
    .unwrap();
    for (opcode, op) in [
        (0xa8a1, RectangleOp::Frame),
        (0xa8a3, RectangleOp::Erase),
        (0xa8a4, RectangleOp::Invert),
        (0xa8a5, RectangleOp::Fill(GuestPtr(0x3000))),
    ] {
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        let fill = opcode == 0xa8a5;
        assert_eq!(
            Abi::trap(opcode, c, &m).unwrap(),
            (
                Call::DrawRectangle {
                    rectangle: GuestPtr(if fill { 0x2000 } else { 0x3000 }),
                    op
                },
                Reply::None
            )
        );
        assert_eq!(c.dar[15], if fill { 0x1008 } else { 0x1004 });
    }
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0x8000);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0x2000;
    c.gpr[4] = 0x3000;
    for (service, op) in [
        (Service::FrameRect, RectangleOp::Frame),
        (Service::EraseRect, RectangleOp::Erase),
        (Service::InvertRect, RectangleOp::Invert),
        (Service::FillRect, RectangleOp::Fill(GuestPtr(0x3000))),
    ] {
        assert_eq!(
            Abi::import(service, c, &m).unwrap(),
            (
                Call::DrawRectangle {
                    rectangle: GuestPtr(0x2000),
                    op
                },
                Reply::None
            )
        );
    }
    m.map(GuestPtr(0x2000), vec![0; 7], Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x2000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa8a5, c, &m).is_err());
    assert_eq!(c.dar[15], 0x2000);
}
#[test]
fn frames_with_either_zero_pen_dimension_have_no_coverage() {
    for pen in [(0, 3), (3, 0), (0, 0)] {
        assert!(
            systemless_future::rounded::coverage(Rect::new(0, 0, 8, 8), 0, 0, Some(pen))
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        !systemless_future::rounded::coverage(Rect::new(0, 0, 8, 8), 0, 0, None)
            .unwrap()
            .is_empty()
    );
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
    for (i, &a) in args.iter().enumerate() {
        c.gpr[3 + i] = a;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn setup() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"OpenPort",
            b"SetPortBits",
            b"FrameRect",
            b"EraseRect",
            b"InvertRect",
            b"FillRect",
        ],
        0,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 0x1000], Permissions::RW)
        .unwrap();
    invoke(&mut app, 0, &[0x600]);
    invoke(&mut app, 1, &[0x1000]);
    let mut bits = 0x1200u32.to_be_bytes().to_vec();
    bits.extend([0, 2, 0, 0, 0, 0, 0, 8, 0, 8]);
    app.runtime.memory.write(GuestPtr(0x1100), &bits).unwrap();
    invoke(&mut app, 2, &[0x1100]);
    let ports = app.runtime.process.ports.unwrap();
    ports.port_size(&mut app.runtime.memory, 8, 8).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x300), &[0, 1, 0, 1, 0, 7, 0, 7])
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x3000), vec![0x96; 8], Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa8, 0xa1, 0xa8, 0xa3, 0xa8, 0xa4, 0xa8, 0xa5],
            Permissions::RX,
        )
        .unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 8, 8, GuestPtr(0x4000))).unwrap());
    app
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_cpus_draw_frame_erase_invert_and_fill_with_distinct_patterns_and_state() {
    let mut app = setup();
    app.runtime
        .memory
        .write(GuestPtr(0x1030), &[0, 2, 0, 3, 0, 1, 0, 2])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x103a), &[0xaa; 8])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1020), &[0x33; 8])
        .unwrap();
    let initial = [
        0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a,
        0xa5,
    ];
    for index in 0..4 {
        app.runtime
            .memory
            .write(
                GuestPtr(0x1038),
                &(if index == 0 { 10u16 } else { 0x7fff }).to_be_bytes(),
            )
            .unwrap();
        let original = app
            .runtime
            .memory
            .read(GuestPtr(0x1000), 108)
            .unwrap()
            .to_vec();
        let mut expected = initial;
        for y in 1..7 {
            for x in 1..7 {
                let bit = match index {
                    0 => {
                        if (2..6).contains(&y) && (3..5).contains(&x) {
                            continue;
                        }
                        ((0x5a >> (7 - x)) & 1) ^ ((0xaa >> (7 - x)) & 1)
                    }
                    1 => (0x33 >> (7 - x)) & 1,
                    2 => ((0x5a >> (7 - x)) & 1) ^ 1,
                    _ => (0x96 >> (7 - x)) & 1,
                };
                expected[y * 2] = (expected[y * 2] & !(1 << (7 - x))) | (bit << (7 - x));
            }
        }
        for architecture in [Architecture::PowerPc, Architecture::M68k] {
            app.runtime
                .memory
                .write(GuestPtr(0x1000), &original)
                .unwrap();
            app.runtime
                .memory
                .write(GuestPtr(0x1200), &initial)
                .unwrap();
            if architecture == Architecture::PowerPc {
                assert_eq!(invoke(&mut app, 3 + index, &[0x300, 0x3000]), 0x300);
            } else {
                let stack = STACK_BASE + 0x8000;
                let mut args = if index == 3 {
                    0x3000u32.to_be_bytes().to_vec()
                } else {
                    vec![]
                };
                args.extend(0x300u32.to_be_bytes());
                args.extend([0xa5; 4]);
                app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
                app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000 + index * 2, stack);
                let Cpu::M68k(c) = &mut app.runtime.cpu else {
                    panic!()
                };
                c.dar[0] = 0x12345678;
                assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
                let Cpu::M68k(c) = &app.runtime.cpu else {
                    panic!()
                };
                assert_eq!(c.dar[0], 0x12345678);
                assert_eq!(c.dar[15], stack + if index == 3 { 8 } else { 4 });
                assert_eq!(
                    app.runtime.memory.read(GuestPtr(c.dar[15]), 4).unwrap(),
                    [0xa5; 4]
                );
            }
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x1200), 16).unwrap(),
                expected,
                "{architecture:?} operation {index}"
            );
            let mut record = original.clone();
            if index == 3 {
                record[40..48].fill(0x96);
            }
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
                record
            );
        }
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn rectangle_clips_hidden_frames_and_fill_output_permissions_are_checked() {
    let mut app = setup();
    let ports = app.runtime.process.ports.unwrap();
    let gpu = app.runtime.graphics.take().unwrap();
    for size in [[0, 0, 0, 2], [0, 2, 0, 0]] {
        app.runtime.memory.write(GuestPtr(0x1034), &size).unwrap();
        ports
            .draw_rectangle(
                &mut app.runtime.memory,
                &gpu,
                GuestPtr(0x300),
                RectangleOp::Frame,
            )
            .unwrap();
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x1200), 16).unwrap(),
            [0; 16]
        );
    }
    app.runtime
        .memory
        .write(GuestPtr(0x1034), &[0, 1, 0, 1])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1042), &[0xff, 0xff])
        .unwrap();
    ports
        .draw_rectangle(
            &mut app.runtime.memory,
            &gpu,
            GuestPtr(0x300),
            RectangleOp::Frame,
        )
        .unwrap();
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 16).unwrap(),
        [0; 16]
    );
    let h = app.runtime.memory.u32(GuestPtr(0x101c)).unwrap();
    let words = [
        44i16, 0, 0, 8, 8, 0, 0, 8, 32767, 2, 2, 6, 32767, 6, 2, 6, 32767, 8, 0, 8, 32767, 32767,
    ];
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, h, 44)
        .unwrap();
    let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
    app.runtime
        .memory
        .write(
            GuestPtr(data),
            &words
                .iter()
                .flat_map(|w| w.to_be_bytes())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1050), &0x12345678u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1038), &0x7fffu16.to_be_bytes())
        .unwrap();
    ports
        .draw_rectangle(
            &mut app.runtime.memory,
            &gpu,
            GuestPtr(0x300),
            RectangleOp::Invert,
        )
        .unwrap();
    let expected = [
        0, 0, 0x7e, 0, 0x42, 0, 0x42, 0, 0x42, 0, 0x42, 0, 0x7e, 0, 0, 0,
    ];
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 16).unwrap(),
        expected
    );
    let record = app
        .runtime
        .memory
        .read(GuestPtr(0x1000), 108)
        .unwrap()
        .to_vec();
    app.runtime
        .memory
        .map(GuestPtr(0x4000), record, Permissions::RX)
        .unwrap();
    ports
        .set(&mut app.runtime.memory, GuestPtr(0x4000))
        .unwrap();
    assert!(ports
        .draw_rectangle(
            &mut app.runtime.memory,
            &gpu,
            GuestPtr(0x300),
            RectangleOp::Fill(GuestPtr(0x3000))
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 16).unwrap(),
        expected
    );
}
