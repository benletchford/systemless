mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    graphics::{Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::{OvalOp, Ports},
    runtime::Stop,
};
const PORT: u32 = 0x51000080;
const RECT: u32 = 0x51000100;
const PATTERN: u32 = 0x51000120;
const PIXELS: u32 = 0x52000000;
#[test]
fn oval_abis_marshal_every_verb_and_preserve_stack_on_truncated_fill() {
    let mut memory = Memory::default();
    memory
        .map(
            GuestPtr(0x1000),
            vec![0, 0, 0x30, 0, 0, 0, 0x20, 0, 0xa5, 0x5a],
            Permissions::RX,
        )
        .unwrap();
    let mut ppc = Cpu::new(Architecture::PowerPc, 0, 0x9000);
    let Cpu::PowerPc(c) = &mut ppc else { panic!() };
    c.gpr[3] = 0x2000;
    c.gpr[4] = 0x3000;
    for (index, service, op) in [
        (0, Service::FrameOval, OvalOp::Frame),
        (1, Service::PaintOval, OvalOp::Paint),
        (2, Service::EraseOval, OvalOp::Erase),
        (3, Service::InvertOval, OvalOp::Invert),
        (4, Service::FillOval, OvalOp::Fill(GuestPtr(0x3000))),
    ] {
        assert_eq!(
            Abi::import(service, c, &memory).unwrap(),
            (
                Call::DrawOval {
                    rectangle: GuestPtr(0x2000),
                    op
                },
                Reply::None
            )
        );
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        assert_eq!(
            Abi::trap(0xa8b7 + index, c, &memory).unwrap(),
            (
                Call::DrawOval {
                    rectangle: GuestPtr(if index == 4 { 0x2000 } else { 0x3000 }),
                    op
                },
                Reply::None
            )
        );
        assert_eq!(c.dar[15], if index == 4 { 0x1008 } else { 0x1004 });
        assert_eq!(memory.u16(GuestPtr(0x1008)).unwrap(), 0xa55a);
    }
    memory
        .map(GuestPtr(0x2000), vec![0; 7], Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x2000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa8bb, c, &memory).is_err());
    assert_eq!(c.dar[15], 0x2000);
}
fn rows(rectangles: &[Rect], bounds: Rect) -> Vec<u8> {
    let mut result = vec![0; (bounds.bottom - bounds.top) as usize];
    for y in bounds.top..bounds.bottom {
        for x in bounds.left..bounds.right {
            let count = rectangles
                .iter()
                .filter(|r| r.left <= x && x < r.right && r.top <= y && y < r.bottom)
                .count();
            assert!(count <= 1, "overlap would apply XOR twice");
            if count == 1 {
                result[(y - bounds.top) as usize] |= 0x80 >> (x - bounds.left);
            }
        }
    }
    result
}
#[test]
fn oval_coverage_matches_preserved_reference_and_keeps_full_guest_extents() {
    use systemless_future::rounded::oval;
    let r = Rect::new(-4, -2, 4, 2);
    // Captured from preserved Systemless compute_oval_spans(8,4), not native System7.
    assert_eq!(rows(&oval(r, None).unwrap(), r), [0x7e, 0xff, 0xff, 0x7e]);
    assert_eq!(
        rows(&oval(r, Some((1, 1))).unwrap(), r),
        [0x7e, 0x81, 0x81, 0x7e]
    );
    for pen in [(0, 1), (1, 0)] {
        assert!(oval(r, Some(pen)).unwrap().is_empty());
    }
    assert!(oval(r, Some((-1, 1))).is_err());
    assert!(oval(Rect::new(i32::MIN, 0, i32::MAX, 1), None).is_err());
    for r in [
        Rect::new(-32768, -32768, 32767, 32767),
        Rect::new(-32768, 0, 32767, 1),
        Rect::new(0, -32768, 1, 32767),
    ] {
        let coverage = oval(r, None).unwrap();
        assert!(!coverage.is_empty());
        assert!(coverage.iter().all(|p| p.left >= r.left
            && p.right <= r.right
            && p.top >= r.top
            && p.bottom <= r.bottom));
        if r.right - r.left == 65535 {
            assert!(coverage.iter().any(|p| p.right - p.left > 32767));
        }
    }
}
fn setup(gpu: bool) -> LoadedApplication {
    let fixture = support::pef_program(
        &[0x4e800020],
        &[
            b"FrameOval",
            b"PaintOval",
            b"EraseOval",
            b"InvertOval",
            b"FillOval",
        ],
        0,
    );
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(PIXELS), vec![0; 8], Permissions::RW)
        .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x51000020),
        screen_bitmap: GuestPtr(0x51000000),
    };
    let mut bitmap = PIXELS.to_be_bytes().to_vec();
    bitmap.extend([0, 2, 0, 0, 0, 0, 0, 4, 0, 8]);
    app.runtime
        .memory
        .write(ports.screen_bitmap, &bitmap)
        .unwrap();
    ports
        .open(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            GuestPtr(PORT),
        )
        .unwrap();
    app.runtime.process.ports = Some(ports);
    app.runtime
        .memory
        .write(GuestPtr(RECT), &[0, 0, 0, 0, 0, 4, 0, 8])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(PATTERN), &[0xf0; 8])
        .unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            (0xa8b7..=0xa8bbu16).flat_map(u16::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    if gpu {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        app.runtime.graphics = Some(
            pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap(),
        );
    }
    app
}
fn invoke(app: &mut LoadedApplication, architecture: Architecture, index: u32) {
    let stack = STACK_BASE + 0x8000;
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(stack), &[0xa5; 12])
        .unwrap();
    let mut args = Vec::new();
    if index == 4 {
        args.extend(PATTERN.to_be_bytes());
    }
    args.extend(RECT.to_be_bytes());
    app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
    app.runtime.cpu = Cpu::new(
        architecture,
        if architecture == Architecture::PowerPc {
            gate
        } else {
            0x50000000 + index * 2
        },
        stack,
    );
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = RECT;
        c.gpr[4] = PATTERN;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    if let Cpu::M68k(c) = &app.runtime.cpu {
        assert_eq!(c.dar[15], stack + args.len() as u32);
        assert_eq!(
            app.runtime.memory.u32(GuestPtr(c.dar[15])).unwrap(),
            0xa5a5a5a5
        );
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn all_oval_verbs_use_gpu_with_shared_cpu_results_clipping_patterns_and_padding() {
    let mut app = setup(true);
    let ports = app.runtime.process.ports.unwrap();
    ports
        .clip_rect(&mut app.runtime.memory, Rect::new(1, 0, 7, 4))
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(PORT + 48), &[0, 2, 0, 3])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(PORT + 58), &[0xaa; 8])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(PORT + 32), &[0xcc; 8])
        .unwrap();
    let initial = [0x55, 0xa5, 0x55, 0xa5, 0x55, 0xa5, 0x55, 0xa5];
    for index in 0..5 {
        app.runtime
            .memory
            .write(
                GuestPtr(PORT + 56),
                &(if index <= 1 { 10u16 } else { 0x7fff }).to_be_bytes(),
            )
            .unwrap();
        let saved = app
            .runtime
            .memory
            .read(GuestPtr(PORT), 108)
            .unwrap()
            .to_vec();
        let mut expected = initial;
        for row in 0..4 {
            let mask = if index == 0 && (1..3).contains(&row) {
                0
            } else {
                0x7e
            };
            let value = match index {
                0 | 1 => 0x55 ^ 0xaa,
                2 => 0xcc,
                3 => !0x55u8,
                _ => 0xf0,
            };
            expected[row * 2] = (0x55 & !mask) | (value & mask);
        }
        for architecture in [Architecture::PowerPc, Architecture::M68k] {
            app.runtime.memory.write(GuestPtr(PORT), &saved).unwrap();
            app.runtime
                .memory
                .write(GuestPtr(PIXELS), &initial)
                .unwrap();
            invoke(&mut app, architecture, index);
            assert_eq!(
                app.runtime.memory.read(GuestPtr(PIXELS), 8).unwrap(),
                expected,
                "{architecture:?} verb{index}"
            );
            let mut expected_port = saved.clone();
            if index == 4 {
                expected_port[40..48].fill(0xf0);
            }
            assert_eq!(
                app.runtime.memory.read(GuestPtr(PORT), 108).unwrap(),
                expected_port
            );
        }
    }
    // Faults must not paint or publish a fill pattern.
    let before = app
        .runtime
        .memory
        .read(GuestPtr(PORT), 108)
        .unwrap()
        .to_vec();
    let pixels = app
        .runtime
        .memory
        .read(GuestPtr(PIXELS), 8)
        .unwrap()
        .to_vec();
    assert!(ports
        .draw_oval(
            app.runtime.graphics.as_ref(),
            &mut app.runtime.memory,
            Rect::new(0, 0, 8, 4),
            OvalOp::Fill(GuestPtr(u32::MAX))
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(PORT), 108).unwrap(),
        before
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(PIXELS), 8).unwrap(),
        pixels
    );
    app.runtime
        .memory
        .write(GuestPtr(PORT + 104), &1u32.to_be_bytes())
        .unwrap();
    assert!(ports
        .draw_oval(
            app.runtime.graphics.as_ref(),
            &mut app.runtime.memory,
            Rect::new(0, 0, 8, 4),
            OvalOp::Paint
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(PIXELS), 8).unwrap(),
        pixels
    );
}
#[test]
fn recorded_oval_outlines_ignore_pen_size_and_clipping_without_requiring_gpu_on_both_cpus() {
    use systemless_future::{
        region_geometry,
        region_recording::{Manager, Op},
        regions,
    };
    let mut app = setup(false);
    let ports = app.runtime.process.ports.unwrap();
    let destination = regions::call(
        &mut app.runtime.memory,
        &mut app.runtime.process,
        regions::Op::New,
        0,
        0,
    )
    .unwrap();
    ports
        .clip_rect(&mut app.runtime.memory, Rect::new(0, 0, 0, 0))
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(PORT + 52), &[0; 6])
        .unwrap();
    for architecture in [Architecture::PowerPc, Architecture::M68k] {
        let mut manager = Manager::default();
        manager
            .call(&mut app.runtime.memory, &mut app.runtime.process, Op::Open)
            .unwrap();
        invoke(&mut app, architecture, 0);
        manager
            .call(
                &mut app.runtime.memory,
                &mut app.runtime.process,
                Op::Close(destination),
            )
            .unwrap();
        let data = GuestPtr(app.runtime.memory.u32(GuestPtr(destination)).unwrap());
        let length = app.runtime.memory.u16(data).unwrap() as usize;
        let coverage =
            region_geometry::decode(app.runtime.memory.read(data, length).unwrap()).unwrap();
        assert_eq!(
            rows(&coverage, Rect::new(0, 0, 8, 4)),
            [0x7e, 0xff, 0xff, 0x7e]
        );
        assert_eq!(
            app.runtime.memory.read(GuestPtr(PIXELS), 8).unwrap(),
            [0; 8]
        );
        assert_eq!(app.runtime.memory.u16(GuestPtr(PORT + 66)).unwrap(), 0);
    }
}

#[test]
fn hidden_frames_and_empty_fill_keep_nonpixel_semantics_without_gpu() {
    let mut app = setup(false);
    let ports = app.runtime.process.ports.unwrap();
    app.runtime
        .memory
        .write(GuestPtr(PORT + 66), &(-1i16).to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(PORT + 52), &[0xff; 4])
        .unwrap();
    ports
        .draw_oval(
            None,
            &mut app.runtime.memory,
            Rect::new(0, 0, 8, 4),
            OvalOp::Frame,
        )
        .unwrap();
    let port = app
        .runtime
        .memory
        .read(GuestPtr(PORT), 108)
        .unwrap()
        .to_vec();
    ports
        .draw_oval(
            None,
            &mut app.runtime.memory,
            Rect::new(0, 0, 0, 4),
            OvalOp::Fill(GuestPtr(PATTERN)),
        )
        .unwrap();
    let mut expected = port;
    expected[40..48].fill(0xf0);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(PORT), 108).unwrap(),
        expected
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(PIXELS), 8).unwrap(),
        [0; 8]
    );
    assert!(ports
        .draw_oval(
            None,
            &mut app.runtime.memory,
            Rect::new(0, 0, 0, 4),
            OvalOp::Fill(GuestPtr(u32::MAX))
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(PORT), 108).unwrap(),
        expected
    );
}
