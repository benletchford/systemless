mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    graphics::{Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::Process,
    quickdraw::Ports,
    region_geometry,
    region_recording::{Manager, Op},
    regions,
    runtime::Stop,
};
fn setup() -> (Memory, Process, Ports) {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0; 32], Permissions::RW)
        .unwrap();
    m.write(
        GuestPtr(0x1000),
        &[0, 0, 0x20, 0, 0, 2, 0, 0, 0, 0, 0, 16, 0, 16],
    )
    .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1020),
        screen_bitmap: GuestPtr(0x1000),
    };
    let mut p = Process::default();
    ports.open(&mut m, &mut p, GuestPtr(0x1080)).unwrap();
    p.ports = Some(ports);
    (m, p, ports)
}
fn bytes(m: &Memory, p: &mut Process, h: u32) -> Vec<u8> {
    let n = p.handle_size(m, h).unwrap();
    m.read(GuestPtr(m.u32(GuestPtr(h)).unwrap()), n as usize)
        .unwrap()
        .to_vec()
}
fn destination(m: &mut Memory, p: &mut Process) -> u32 {
    regions::call(m, p, regions::Op::New, 0, 0).unwrap()
}
fn position(m: &mut Memory, x: i16, y: i16) {
    let mut data = y.to_be_bytes().to_vec();
    data.extend(x.to_be_bytes());
    m.write(GuestPtr(0x1080 + 48), &data).unwrap();
}
#[test]
fn hidden_outlines_are_unclipped_and_balanced_and_save_can_be_disabled_and_restored() {
    let (mut m, mut p, ports) = setup();
    let mut manager = Manager::default();
    let dst = destination(&mut m, &mut p);
    m.write(GuestPtr(0x2000), &[0x5a; 32]).unwrap();
    m.write(GuestPtr(0x1080 + 66), &(-2i16).to_be_bytes())
        .unwrap();
    manager.call(&mut m, &mut p, Op::Open).unwrap();
    let save = m.u32(GuestPtr(0x1080 + 96)).unwrap();
    assert_ne!(save, 0);
    assert!(manager.call(&mut m, &mut p, Op::Open).is_err());
    assert_eq!(m.u16(GuestPtr(0x1080 + 66)).unwrap() as i16, -3);
    // Zero pen, arbitrary mode and empty clipping must not change the definition.
    m.write(GuestPtr(0x1080 + 52), &[0; 6]).unwrap();
    let clip = m.u32(GuestPtr(0x1080 + 28)).unwrap();
    regions::call(&mut m, &mut p, regions::Op::Clear, clip, 0).unwrap();
    position(&mut m, -4, -3);
    for (x, y) in [(6, -3), (-4, 7), (-4, -3)] {
        ports.line(&mut m, None, x, y, false).unwrap();
    }
    m.write(GuestPtr(0x1080 + 96), &0u32.to_be_bytes()).unwrap();
    position(&mut m, 0, 0);
    ports.line(&mut m, None, 15, 15, false).unwrap();
    m.write(GuestPtr(0x1080 + 96), &save.to_be_bytes()).unwrap();
    manager.call(&mut m, &mut p, Op::Close(dst)).unwrap();
    let rects = region_geometry::decode(&bytes(&m, &mut p, dst)).unwrap();
    for y in -5..10 {
        for x in -6..10 {
            assert_eq!(
                rects
                    .iter()
                    .any(|r| r.left <= x && x < r.right && r.top <= y && y < r.bottom),
                x >= -4 && y >= -3 && x + y < 3
            );
        }
    }
    assert_eq!(m.read(GuestPtr(0x2000), 32).unwrap(), [0x5a; 32]);
    assert_eq!(m.u16(GuestPtr(0x1080 + 66)).unwrap() as i16, -2);
    assert_eq!(m.u32(GuestPtr(0x1080 + 96)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(save)).unwrap(), 0);
    assert!(manager.call(&mut m, &mut p, Op::Close(dst)).is_err());
}
#[test]
fn invalid_close_and_record_capacity_fail_without_replacing_destination_or_pen() {
    let (mut m, mut p, ports) = setup();
    let mut manager = Manager::default();
    let dst = destination(&mut m, &mut p);
    let before = bytes(&m, &mut p, dst);
    manager.call(&mut m, &mut p, Op::Open).unwrap();
    let save = m.u32(GuestPtr(0x1080 + 96)).unwrap();
    assert!(manager.call(&mut m, &mut p, Op::Close(save)).is_err());
    assert!(manager.call(&mut m, &mut p, Op::Close(0)).is_err());
    assert_eq!(bytes(&m, &mut p, dst), before);
    position(&mut m, 0, 0);
    // Exhaust the bounded edge buffer with reversible vertical segments.
    for i in 0..4093 {
        ports
            .line(&mut m, None, 0, if i % 2 == 0 { 1 } else { 0 }, false)
            .unwrap();
    }
    let pen = m.read(GuestPtr(0x1080 + 48), 4).unwrap().to_vec();
    let buffer = bytes(&m, &mut p, save);
    assert!(ports.line(&mut m, None, 0, 0, false).is_err());
    assert_eq!(m.read(GuestPtr(0x1080 + 48), 4).unwrap(), pen);
    assert_eq!(bytes(&m, &mut p, save), buffer);
    assert_eq!(bytes(&m, &mut p, dst), before);
}
#[test]
fn both_real_cpu_adapters_share_recording_and_void_calling_conventions() {
    for architecture in [Architecture::M68k, Architecture::PowerPc] {
        let fixture = support::pef_program(&[0x4e800020], &[b"OpenRgn", b"CloseRgn"], 24);
        let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
        app.runtime
            .memory
            .map(
                GuestPtr(0x50000000),
                vec![0xa8, 0xda, 0xa8, 0xdb],
                Permissions::RX,
            )
            .unwrap();
        let (m, _, ports) = setup();
        // Install the same port shape in the real application's independent memory.
        app.runtime
            .memory
            .map(
                GuestPtr(0x1000),
                m.read(GuestPtr(0x1000), 512).unwrap().to_vec(),
                Permissions::RW,
            )
            .unwrap();
        app.runtime
            .memory
            .map(GuestPtr(0x2000), vec![0; 32], Permissions::RW)
            .unwrap();
        // Recreate owned port regions in the application heap, rather than sharing handles.
        ports
            .open(
                &mut app.runtime.memory,
                &mut app.runtime.process,
                GuestPtr(0x1080),
            )
            .unwrap();
        app.runtime.process.ports = Some(ports);
        let dst = destination(&mut app.runtime.memory, &mut app.runtime.process);
        for close in [false, true] {
            let index = u32::from(close);
            let vector = app
                .runtime
                .memory
                .u32(GuestPtr(app.info.toc_or_a5 + 8 + 4 * index))
                .unwrap();
            let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
            let stack = STACK_BASE + 0x7000;
            app.runtime
                .memory
                .write(GuestPtr(stack), &dst.to_be_bytes())
                .unwrap();
            app.runtime.cpu = Cpu::new(
                architecture,
                if architecture == Architecture::M68k {
                    0x50000000 + index * 2
                } else {
                    gate
                },
                stack,
            );
            match &mut app.runtime.cpu {
                Cpu::M68k(c) => {
                    c.dar[0] = 0x12345678;
                    c.dar[1] = 0xfeedface;
                }
                Cpu::PowerPc(c) => {
                    c.gpr[3] = dst;
                    c.gpr[4] = 0xfeedface;
                }
            }
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            match &app.runtime.cpu {
                Cpu::M68k(c) => assert_eq!(
                    (c.dar[0], c.dar[1], c.dar[15]),
                    (0x12345678, 0xfeedface, stack + index * 4)
                ),
                Cpu::PowerPc(c) => assert_eq!((c.gpr[3], c.gpr[4]), (dst, 0xfeedface)),
            }
            if !close {
                position(&mut app.runtime.memory, 1, 1);
                for (x, y) in [(10, 1), (10, 10), (1, 10), (1, 1)] {
                    ports
                        .line(&mut app.runtime.memory, None, x, y, false)
                        .unwrap();
                }
            }
        }
        assert_eq!(
            bytes(&app.runtime.memory, &mut app.runtime.process, dst),
            support::words(&[10, 1, 1, 10, 10])
        );
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_frames_record_boundaries_independent_of_pen_and_showpen_draws_normally() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 16, 16, GuestPtr(0x4000))).unwrap();
    let (mut m, mut p, ports) = setup();
    let mut manager = Manager::default();
    let dst = destination(&mut m, &mut p);
    manager.call(&mut m, &mut p, Op::Open).unwrap();
    m.write(GuestPtr(0x1080 + 52), &[0; 4]).unwrap();
    ports
        .round_rect(&gpu, &mut m, Rect::new(-3, -2, 12, 12), 0, 0, true)
        .unwrap();
    ports
        .round_rect(&gpu, &mut m, Rect::new(2, 2, 8, 8), 0, 0, true)
        .unwrap();
    assert_eq!(m.read(GuestPtr(0x2000), 32).unwrap(), [0; 32]);
    manager.call(&mut m, &mut p, Op::Close(dst)).unwrap();
    let rects = region_geometry::decode(&bytes(&m, &mut p, dst)).unwrap();
    for y in -4..15 {
        for x in -5..15 {
            let expected = (-3..12).contains(&x)
                && (-2..12).contains(&y)
                && !((2..8).contains(&x) && (2..8).contains(&y));
            assert_eq!(
                rects
                    .iter()
                    .any(|r| r.left <= x && x < r.right && r.top <= y && y < r.bottom),
                expected
            );
        }
    }
    manager.call(&mut m, &mut p, Op::Open).unwrap();
    ports.pen_normal(&mut m).unwrap();
    ports.pen_visibility(&mut m, 1).unwrap();
    ports
        .round_rect(&gpu, &mut m, Rect::new(2, 2, 10, 10), 0, 0, true)
        .unwrap();
    assert_ne!(m.read(GuestPtr(0x2000), 32).unwrap(), [0; 32]);
    manager.call(&mut m, &mut p, Op::Close(dst)).unwrap();
    assert_eq!(bytes(&m, &mut p, dst), support::words(&[10, 2, 2, 10, 10]));
    assert_eq!(m.u16(GuestPtr(0x1080 + 66)).unwrap(), 1);
}
#[test]
fn region_and_polygon_recordings_cannot_be_nested_across_different_ports() {
    let (mut m, mut p, ports) = setup();
    let dst = destination(&mut m, &mut p);
    let mut region = std::mem::take(&mut p.region_recording);
    region.call(&mut m, &mut p, Op::Open).unwrap();
    p.region_recording = region;
    ports.open(&mut m, &mut p, GuestPtr(0x1180)).unwrap();
    let mut polygon = std::mem::take(&mut p.polygons);
    assert!(polygon
        .call(&mut m, &mut p, None, systemless_future::polygons::Op::Open)
        .is_err());
    p.polygons = polygon;
    ports.set(&mut m, GuestPtr(0x1080)).unwrap();
    let mut region = std::mem::take(&mut p.region_recording);
    region.call(&mut m, &mut p, Op::Close(dst)).unwrap();
    p.region_recording = region;
    ports.set(&mut m, GuestPtr(0x1180)).unwrap();
    let mut polygon = std::mem::take(&mut p.polygons);
    polygon
        .call(&mut m, &mut p, None, systemless_future::polygons::Op::Open)
        .unwrap();
    p.polygons = polygon;
    ports.set(&mut m, GuestPtr(0x1080)).unwrap();
    let mut region = std::mem::take(&mut p.region_recording);
    assert!(region.call(&mut m, &mut p, Op::Open).is_err());
    p.region_recording = region;
    ports.set(&mut m, GuestPtr(0x1180)).unwrap();
    let mut polygon = std::mem::take(&mut p.polygons);
    polygon
        .call(&mut m, &mut p, None, systemless_future::polygons::Op::Close)
        .unwrap();
    p.polygons = polygon;
}
