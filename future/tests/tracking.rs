mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    controls::{Op, TrackControl},
    cpu::{Architecture, Call, Cpu},
    events,
    graphics::Graphics,
    host_input::Pointer,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::SCREEN,
    runtime::Stop,
};
const STACK: u32 = STACK_BASE + 0x8000;
fn invoke(a: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for (i, &v) in args.iter().take(8).enumerate() {
        c.gpr[3 + i] = v;
    }
    if args.len() > 8 {
        a.runtime
            .memory
            .write(GuestPtr(STACK + 56), &args[8].to_be_bytes())
            .unwrap();
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn setup(instance: &wgpu::Instance, kind: i16) -> (LoadedApplication, u32, Pointer) {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitFonts",
            b"InitWindows",
            b"NewWindow",
            b"NewControl",
            b"TrackControl",
            b"SetPort",
        ],
        512,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = a.info.toc_or_a5;
    a.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    let mut code = vec![0; 64];
    code[..4].copy_from_slice(&[0xa9, 0x68, 0x4e, 0x71]);
    code[32..36].copy_from_slice(&0x60000000u32.to_be_bytes());
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), code, Permissions::RX)
        .unwrap();
    invoke(&mut a, 0, &[data + 330]);
    invoke(&mut a, 1, &[]);
    a.runtime.graphics = Some(
        pollster::block_on(Graphics::new(instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut a, 2, &[]);
    a.runtime
        .memory
        .write(GuestPtr(0x51000000), &support::words(&[50, 50, 200, 300]))
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0x51000010), b"\x04test")
        .unwrap();
    let window = invoke(
        &mut a,
        3,
        &[0, 0x51000000, 0x51000010, 1, 2, u32::MAX, 0, 0],
    );
    a.runtime
        .memory
        .write(GuestPtr(0x51000000), &support::words(&[10, 10, 30, 90]))
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0x51000010), b"\x02OK")
        .unwrap();
    let button = invoke(
        &mut a,
        4,
        &[
            window,
            0x51000000,
            0x51000010,
            255,
            u32::from(kind != 0),
            0,
            1,
            kind as u32,
            0,
        ],
    );
    invoke(&mut a, 6, &[0]);
    (a, button, Pointer::default())
}
fn motion(a: &mut LoadedApplication, p: &mut Pointer, x: f64, y: f64) {
    p.motion(
        &mut a.runtime.process.input,
        &mut a.runtime.memory,
        (x, y),
        (512, 342),
        (512, 342),
    )
    .unwrap();
}
fn button(a: &mut LoadedApplication, p: &mut Pointer, down: bool) {
    p.button(&mut a.runtime.memory, &mut a.runtime.process.input, down)
        .unwrap();
}
fn start(
    a: &mut LoadedApplication,
    arch: Architecture,
    h: u32,
    point: [i16; 2],
    action: u32,
) -> systemless_future::Result<Stop> {
    match arch {
        Architecture::M68k => {
            let mut frame = vec![0xa5; 16];
            frame[..4].copy_from_slice(&action.to_be_bytes());
            frame[4..6].copy_from_slice(&point[0].to_be_bytes());
            frame[6..8].copy_from_slice(&point[1].to_be_bytes());
            frame[8..12].copy_from_slice(&h.to_be_bytes());
            a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
            a.runtime.cpu = Cpu::new(arch, 0x50000000, STACK);
            let Cpu::M68k(c) = &mut a.runtime.cpu else {
                panic!()
            };
            c.dar[0] = 0x12345678;
            c.dar[2] = 0x87654321;
        }
        Architecture::PowerPc => {
            let tv = a
                .runtime
                .memory
                .u32(GuestPtr(a.info.toc_or_a5 + 8 + 5 * 4))
                .unwrap();
            let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
            a.runtime.cpu = Cpu::new(arch, pc, STACK);
            let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
                panic!()
            };
            c.gpr[3] = h;
            c.gpr[4] = (u32::from(point[0] as u16) << 16) | u32::from(point[1] as u16);
            c.gpr[5] = action;
            c.gpr[14] = 0x87654321;
            c.lr = 0x50000020;
        }
    }
    a.runtime.advance(1)
}
fn result(a: &LoadedApplication) -> u32 {
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(c.dar[2], 0x87654321);
            assert_eq!(c.dar[15], STACK + 12);
            a.runtime.memory.u16(GuestPtr(STACK + 12)).unwrap().into()
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[14], 0x87654321);
            c.gpr[3]
        }
    }
}
fn control(a: &LoadedApplication, h: u32) -> GuestPtr {
    GuestPtr(a.runtime.memory.u32(GuestPtr(h)).unwrap())
}
fn hilite(a: &LoadedApplication, h: u32) -> u8 {
    a.runtime
        .memory
        .read(GuestPtr(control(a, h).0 + 17), 1)
        .unwrap()[0]
}
fn screen(a: &LoadedApplication) -> Vec<u8> {
    a.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec()
}
#[test]
fn tracking_adapters_validate_deferred_word_output_and_unpack_point_order() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0, 0, 0, 0, 255, 254, 0, 3, 0x10, 0, 0x20, 0, 0xa5, 0x5a],
        Permissions::RW,
    )
    .unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    let expected = Call::TrackControl(TrackControl {
        handle: 0x10002000,
        point: [-2, 3],
        action: 0,
    });
    assert_eq!(
        Abi::trap(0xa968, &mut c, &m).unwrap(),
        (expected, Reply::StackWord(GuestPtr(0x100c)))
    );
    assert_eq!(c.dar[15], 0x100c);
    c.dar[15] = 0x1002;
    assert!(Abi::trap(0xa968, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1002);
    let mut readonly = Memory::default();
    readonly
        .map(GuestPtr(0x1000), vec![0; 12], Permissions::RW)
        .unwrap();
    readonly
        .map(GuestPtr(0x100c), vec![0xa5; 2], Permissions::RX)
        .unwrap();
    c.dar[15] = 0x1000;
    assert!(Abi::trap(0xa968, &mut c, &readonly).is_err());
    assert_eq!(c.dar[15], 0x1000);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x10002000;
    c.gpr[4] = 0xfffe0003;
    assert_eq!(
        Abi::import(Service::TrackControl, &c, &m).unwrap(),
        (expected, Reply::PpcR3)
    );
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn tracking_suspends_both_cpus_redraws_drag_out_reentry_and_returns_release_point() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, h, mut pointer) = setup(&instance, 0);
        let normal = screen(&a);
        let current = a.runtime.process.ports.unwrap().current_slot;
        motion(&mut a, &mut pointer, 70., 70.);
        button(&mut a, &mut pointer, true);
        a.runtime.process.mem_error = -117;
        a.runtime
            .memory
            .write(GuestPtr(0x220), &(-117i16).to_be_bytes())
            .unwrap();
        assert_eq!(
            start(&mut a, arch, h, [20, 20], 0).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        let instructions = a.runtime.instructions;
        assert_eq!(hilite(&a, h), 10);
        let pressed = screen(&a);
        assert_ne!(pressed, normal);
        assert_eq!(a.runtime.memory.u32(current).unwrap(), 0);
        assert_eq!(
            result(&a),
            if arch == Architecture::M68k {
                0xa5a5
            } else {
                h
            }
        );
        a.runtime.elapse_ticks(5).unwrap();
        assert_eq!(
            a.runtime.advance(10000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(a.runtime.instructions, instructions);
        motion(&mut a, &mut pointer, 300., 250.);
        assert_eq!(
            a.runtime.advance(10000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(hilite(&a, h), 0);
        assert_eq!(screen(&a), normal);
        motion(&mut a, &mut pointer, 70., 70.);
        assert_eq!(
            a.runtime.advance(10000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(hilite(&a, h), 10);
        assert_eq!(screen(&a), pressed);
        assert_eq!(a.runtime.instructions, instructions);
        let input = a.runtime.process.input;
        events::post(&mut a.runtime.memory, 3, 0x1234, 5, input).unwrap();
        events::post(&mut a.runtime.memory, 7, 0x600d, 5, input).unwrap();
        button(&mut a, &mut pointer, false);
        motion(&mut a, &mut pointer, 300., 250.);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(result(&a), 10);
        assert_eq!(hilite(&a, h), 0);
        assert_eq!(screen(&a), normal);
        assert_eq!(a.runtime.memory.u32(current).unwrap(), 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -117);
        assert!(!events::available(&a.runtime.memory, 4).unwrap());
        assert!(events::available(&a.runtime.memory, 8).unwrap());
        assert!(events::available(&a.runtime.memory, 128).unwrap());
        if arch == Architecture::M68k {
            assert_eq!(
                a.runtime.memory.read(GuestPtr(STACK + 14), 2).unwrap(),
                [0xa5; 2]
            );
        }
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn tracking_cancels_outside_uses_masked_physical_release_and_does_not_toggle_values() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for kind in [1, 2] {
            let (mut a, h, mut pointer) = setup(&instance, kind);
            let value = a
                .runtime
                .memory
                .u16(GuestPtr(control(&a, h).0 + 18))
                .unwrap();
            let normal = screen(&a);
            motion(&mut a, &mut pointer, 70., 70.);
            button(&mut a, &mut pointer, true);
            assert_eq!(
                start(&mut a, arch, h, [20, 20], 0).unwrap(),
                Stop::Waiting { ticks: 1 }
            );
            assert_eq!(hilite(&a, h), 11);
            assert_ne!(screen(&a), normal);
            motion(&mut a, &mut pointer, 300., 250.);
            button(&mut a, &mut pointer, false);
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            assert_eq!(result(&a), 0);
            assert_eq!(hilite(&a, h), 0);
            assert_eq!(screen(&a), normal);
            assert_eq!(
                a.runtime
                    .memory
                    .u16(GuestPtr(control(&a, h).0 + 18))
                    .unwrap(),
                value
            );
            motion(&mut a, &mut pointer, 70., 70.);
            button(&mut a, &mut pointer, true);
            assert_eq!(
                start(&mut a, arch, h, [20, 20], u32::MAX).unwrap(),
                Stop::Waiting { ticks: 1 }
            );
            let mask = a.runtime.memory.u16(events::SYSTEM_MASK).unwrap();
            a.runtime
                .memory
                .write(events::SYSTEM_MASK, &(mask & !4).to_be_bytes())
                .unwrap();
            button(&mut a, &mut pointer, false);
            assert!(!events::available(&a.runtime.memory, 4).unwrap());
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            assert_eq!(result(&a), 11);
            assert_eq!(
                a.runtime
                    .memory
                    .u16(GuestPtr(control(&a, h).0 + 18))
                    .unwrap(),
                value
            );
        }
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn tracking_preserves_later_release_events_and_rejects_callbacks_before_mutation() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, h, mut pointer) = setup(&instance, 0);
        motion(&mut a, &mut pointer, 70., 70.);
        button(&mut a, &mut pointer, true);
        assert_eq!(
            start(&mut a, arch, h, [20, 20], 0).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        button(&mut a, &mut pointer, false);
        motion(&mut a, &mut pointer, 300., 250.);
        button(&mut a, &mut pointer, true);
        button(&mut a, &mut pointer, false);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(result(&a), 10);
        assert!(events::available(&a.runtime.memory, 4).unwrap());
        let (mut a, h, mut pointer) = setup(&instance, 0);
        motion(&mut a, &mut pointer, 70., 70.);
        button(&mut a, &mut pointer, true);
        let before = screen(&a);
        assert!(start(&mut a, arch, h, [20, 20], 0x1000).is_err());
        assert_eq!(hilite(&a, h), 0);
        assert_eq!(screen(&a), before);
        let (mut a, h, mut pointer) = setup(&instance, 0);
        motion(&mut a, &mut pointer, 70., 70.);
        button(&mut a, &mut pointer, true);
        let p = control(&a, h);
        a.runtime
            .memory
            .write(GuestPtr(p.0 + 32), &0x1000u32.to_be_bytes())
            .unwrap();
        let before = screen(&a);
        assert!(start(&mut a, arch, h, [20, 20], u32::MAX).is_err());
        assert_eq!(hilite(&a, h), 0);
        assert_eq!(screen(&a), before);
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn tracking_disabled_or_missed_controls_return_immediately_and_failed_draw_restores_hilite() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, h, mut pointer) = setup(&instance, 0);
        motion(&mut a, &mut pointer, 70., 70.);
        button(&mut a, &mut pointer, true);
        assert_eq!(start(&mut a, arch, h, [-1, -1], 0).unwrap(), Stop::Budget);
        assert_eq!(result(&a), 0);
        assert_eq!(hilite(&a, h), 0);
        let mut controls = std::mem::take(&mut a.runtime.process.controls);
        controls
            .call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                a.runtime.graphics.as_ref(),
                Op::Hilite(h, 255),
            )
            .unwrap();
        a.runtime.process.controls = controls;
        let before = screen(&a);
        assert_eq!(start(&mut a, arch, h, [20, 20], 0).unwrap(), Stop::Budget);
        assert_eq!(result(&a), 0);
        assert_eq!(hilite(&a, h), 255);
        assert_eq!(screen(&a), before);
        let (mut a, h, mut pointer) = setup(&instance, 0);
        motion(&mut a, &mut pointer, 70., 70.);
        button(&mut a, &mut pointer, true);
        assert_eq!(
            start(&mut a, arch, h, [20, 20], 0).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        let mut controls = std::mem::take(&mut a.runtime.process.controls);
        controls
            .call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                a.runtime.graphics.as_ref(),
                Op::Hilite(h, 255),
            )
            .unwrap();
        a.runtime.process.controls = controls;
        button(&mut a, &mut pointer, false);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(result(&a), 0);
        assert_eq!(hilite(&a, h), 255);
        let (mut a, h, mut pointer) = setup(&instance, 0);
        motion(&mut a, &mut pointer, 70., 70.);
        button(&mut a, &mut pointer, true);
        assert_eq!(
            start(&mut a, arch, h, [20, 20], 0).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        button(&mut a, &mut pointer, false);
        let p = control(&a, h);
        a.runtime
            .memory
            .write(GuestPtr(p.0 + 24), &1u32.to_be_bytes())
            .unwrap();
        assert!(a.runtime.advance(1).is_err());
        assert!(events::available(&a.runtime.memory, 4).unwrap());
        assert_eq!(
            result(&a),
            if arch == Architecture::M68k {
                0xa5a5
            } else {
                h
            }
        );
        let (mut a, h, mut pointer) = setup(&instance, 0);
        motion(&mut a, &mut pointer, 70., 70.);
        button(&mut a, &mut pointer, true);
        let before = screen(&a);
        a.runtime.graphics = None;
        assert!(start(&mut a, arch, h, [20, 20], 0).is_err());
        assert_eq!(hilite(&a, h), 0);
        assert_eq!(screen(&a), before);
    }
}
