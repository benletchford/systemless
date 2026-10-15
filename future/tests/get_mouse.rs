mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    events::MOUSE,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::Ports,
    runtime::Stop,
};
fn application() -> LoadedApplication {
    let fixture = support::pef_program(&[0x4e800020], &[b"GetMouse"], 16);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let m = &mut app.runtime.memory;
    m.map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x50000000), vec![0xa9, 0x72], Permissions::RX)
        .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x51000020),
        screen_bitmap: GuestPtr(0x51000000),
    };
    m.write(GuestPtr(0x51000004), &[0, 2, 0, 0, 0, 0, 0, 4, 0, 8])
        .unwrap();
    ports
        .open(m, &mut app.runtime.process, GuestPtr(0x51000080))
        .unwrap();
    app.runtime.process.ports = Some(ports);
    app
}
fn invoke(app: &mut LoadedApplication, architecture: Architecture, destination: u32) {
    let stack = STACK_BASE + 0x8000;
    let pc = if architecture == Architecture::M68k {
        0x50000000
    } else {
        let vector = app
            .runtime
            .memory
            .u32(GuestPtr(app.info.toc_or_a5 + 8))
            .unwrap();
        app.runtime.memory.u32(GuestPtr(vector)).unwrap()
    };
    app.runtime
        .memory
        .write(GuestPtr(stack), &destination.to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(architecture, pc, stack);
    match &mut app.runtime.cpu {
        Cpu::M68k(c) => c.dar[0] = 0xfacebeef,
        Cpu::PowerPc(c) => c.gpr[3] = destination,
    }
}
#[test]
fn native_adapters_decode_pointer_and_leave_short_stack_unchanged() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0x12, 0x34, 0x56, 0x78],
        Permissions::RW,
    )
    .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa972, c, &m).unwrap(),
        (Call::GetMouse(GuestPtr(0x12345678)), Reply::None)
    );
    assert_eq!(c.dar[15], 0x1004);
    c.dar[15] = 0x1001;
    assert!(Abi::trap(0xa972, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1001);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0x12345678;
    assert_eq!(
        Abi::import(Service::GetMouse, c, &m).unwrap(),
        (Call::GetMouse(GuestPtr(0x12345678)), Reply::None)
    );
}
#[test]
fn both_real_cpus_read_live_guest_mouse_and_port_origin_without_gpu() {
    for architecture in [Architecture::M68k, Architecture::PowerPc] {
        let mut app = application();
        for (global, origin, expected) in [
            ([123i16, 234], [-100, -200], [23i16, 34]),
            ([i16::MAX, i16::MIN], [1, -1], [i16::MIN, i16::MAX]),
            ([-7, 19], [0, 0], [-7, 19]),
        ] {
            app.runtime
                .memory
                .write(
                    MOUSE,
                    &global
                        .into_iter()
                        .flat_map(i16::to_be_bytes)
                        .collect::<Vec<_>>(),
                )
                .unwrap();
            app.runtime
                .memory
                .write(
                    GuestPtr(0x51000088),
                    &origin
                        .into_iter()
                        .flat_map(i16::to_be_bytes)
                        .collect::<Vec<_>>(),
                )
                .unwrap();
            app.runtime
                .memory
                .write(GuestPtr(0x51000180), &[0xa5; 8])
                .unwrap();
            let port = app
                .runtime
                .memory
                .read(GuestPtr(0x51000080), 108)
                .unwrap()
                .to_vec();
            invoke(&mut app, architecture, 0x51000182);
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            let mut bytes = vec![0xa5; 2];
            bytes.extend(expected.into_iter().flat_map(i16::to_be_bytes));
            bytes.extend([0xa5; 2]);
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x51000180), 8).unwrap(),
                bytes
            );
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x51000080), 108).unwrap(),
                port
            );
            assert_eq!(
                [
                    app.runtime.process.input.vertical,
                    app.runtime.process.input.horizontal
                ],
                global
            );
            match &app.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(c.dar[0], 0xfacebeef);
                    assert_eq!(c.dar[15], STACK_BASE + 0x8004);
                }
                Cpu::PowerPc(c) => assert_eq!(c.gpr[3], 0x51000182),
            }
        }
    }
}
#[test]
fn invalid_port_and_readonly_output_do_not_publish_partial_points_or_host_state() {
    for architecture in [Architecture::M68k, Architecture::PowerPc] {
        for color in [false, true] {
            let mut app = application();
            app.runtime
                .memory
                .map(GuestPtr(0x52000000), vec![0xa5; 4], Permissions::RX)
                .unwrap();
            app.runtime.memory.write(MOUSE, &[0, 12, 0, 34]).unwrap();
            if color {
                app.runtime
                    .memory
                    .write(GuestPtr(0x51000086), &0x8000u16.to_be_bytes())
                    .unwrap();
            }
            invoke(
                &mut app,
                architecture,
                if color { 0x51000180 } else { 0x52000000 },
            );
            assert!(app.runtime.advance(1).is_err());
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x52000000), 4).unwrap(),
                [0xa5; 4]
            );
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x51000180), 4).unwrap(),
                [0; 4]
            );
            assert_eq!(
                [
                    app.runtime.process.input.vertical,
                    app.runtime.process.input.horizontal
                ],
                [0; 2]
            );
        }
    }
}
