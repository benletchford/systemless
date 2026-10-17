mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    devices::{self, Op},
    display::Display,
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn invoke(
    app: &mut LoadedApplication,
    architecture: Architecture,
    index: u32,
    argument: Option<u32>,
) {
    let stack = STACK_BASE + 0x8000;
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(stack), &[0xa5; 8])
        .unwrap();
    if let Some(value) = argument {
        app.runtime
            .memory
            .write(GuestPtr(stack), &value.to_be_bytes())
            .unwrap();
    }
    app.runtime.cpu = Cpu::new(
        architecture,
        if architecture == Architecture::PowerPc {
            gate
        } else {
            0x50000000 + index * 2
        },
        stack,
    );
    match &mut app.runtime.cpu {
        Cpu::M68k(c) => c.dar[13] = app.info.toc_or_a5,
        Cpu::PowerPc(c) => {
            c.gpr[2] = app.info.toc_or_a5;
            c.gpr[3] = argument.unwrap_or(0);
        }
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    if let Cpu::M68k(c) = &app.runtime.cpu {
        let tail = stack + if argument.is_some() { 4 } else { 0 };
        assert_eq!(c.dar[15], tail);
        assert_eq!(app.runtime.memory.u32(GuestPtr(tail)).unwrap(), 0xa5a5a5a5);
    }
}
fn device(app: &mut LoadedApplication, operation: Op) -> u32 {
    let mut manager = std::mem::take(&mut app.runtime.process.devices);
    let result = manager.call(&mut app.runtime.memory, &mut app.runtime.process, operation);
    app.runtime.process.devices = manager;
    result.unwrap()
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn first_window_initialization_selects_its_owned_port_only_when_no_caller_port_exists() {
    let fixture = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitFonts",
            b"InitWindows",
            b"GetWMgrPort",
            b"GetPort",
            b"SetPort",
        ],
        512,
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut gpu = Some(
        pollster::block_on(Graphics::new(&instance, None, 640, 480, GuestPtr(0x4000))).unwrap(),
    );
    for architecture in [Architecture::PowerPc, Architecture::M68k] {
        for caller in [false, true] {
            let mut app = LoadedApplication::pef(&fixture.bytes, None)
                .unwrap()
                .with_display(Display::indexed8(640, 480).unwrap())
                .unwrap();
            app.runtime
                .memory
                .map(
                    GuestPtr(0x50000000),
                    support::words(&[0xa86e, 0xa8fe, 0xa912, 0xa910, 0xa874, 0xa873]),
                    Permissions::RX,
                )
                .unwrap();
            app.runtime
                .memory
                .map(GuestPtr(0x51000000), vec![0; 108], Permissions::RW)
                .unwrap();
            app.runtime.graphics = gpu.take();
            let data = app.info.toc_or_a5;
            invoke(&mut app, architecture, 0, Some(data + 400));
            let ports = app.runtime.process.ports.unwrap();
            // IM:Imaging With QuickDraw Table2-1: InitGraf leaves thePort NIL.
            assert_eq!(app.runtime.memory.u32(ports.current_slot).unwrap(), 0);
            invoke(&mut app, architecture, 1, None);
            let old = if caller {
                ports
                    .open(
                        &mut app.runtime.memory,
                        &mut app.runtime.process,
                        GuestPtr(0x51000000),
                    )
                    .unwrap();
                let other = device(
                    &mut app,
                    Op::New {
                        reference: 0,
                        mode: -1,
                    },
                );
                device(&mut app, Op::Select(other));
                0x51000000
            } else {
                0
            };
            let selected = app.runtime.memory.u32(devices::CURRENT).unwrap();
            invoke(&mut app, architecture, 2, None);
            let manager = app.runtime.memory.u32(GuestPtr(0x9de)).unwrap();
            assert_ne!(manager, 0);
            assert_eq!(
                app.runtime.memory.u32(ports.current_slot).unwrap(),
                if caller { old } else { manager },
                "{architecture:?}, preexisting port {caller}"
            );
            assert_eq!(app.runtime.memory.u32(devices::CURRENT).unwrap(), selected);
            invoke(&mut app, architecture, 3, Some(data + 64));
            invoke(&mut app, architecture, 4, Some(data + 68));
            assert_eq!(
                app.runtime.memory.u32(GuestPtr(data + 64)).unwrap(),
                manager
            );
            assert_eq!(
                app.runtime.memory.u32(GuestPtr(data + 68)).unwrap(),
                if caller { old } else { manager }
            );
            // The initial port is the real Window Manager-owned port and regions.
            for offset in [24, 28] {
                let handle = app.runtime.memory.u32(GuestPtr(manager + offset)).unwrap();
                assert!(
                    app.runtime
                        .process
                        .handle_size(&app.runtime.memory, handle)
                        .unwrap()
                        >= 10
                );
            }
            // Idempotent initialization does not repair a later explicit NIL selection.
            invoke(&mut app, architecture, 5, Some(0));
            invoke(&mut app, architecture, 2, None);
            assert_eq!(app.runtime.memory.u32(ports.current_slot).unwrap(), 0);
            assert_eq!(app.runtime.memory.u32(GuestPtr(0x9de)).unwrap(), manager);
            assert_eq!(app.runtime.memory.u32(devices::CURRENT).unwrap(), selected);
            gpu = app.runtime.graphics.take();
        }
    }
}
