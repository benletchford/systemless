mod support;
use support::*;
use systemless_future::{
    abi::{Abi, Reply, Service},
    controls::NewControl,
    cpu::{Architecture, Call, Cpu},
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::SCREEN,
    runtime::Stop,
};
#[test]
fn native_newcontrol_arguments_and_ppc_spill_are_checked_before_cpu_return() {
    let mut memory = Memory::default();
    let mut frame = vec![0x5a; 30];
    w32(&mut frame, 0, 0x12345678);
    w16(&mut frame, 4, 2);
    w16(&mut frame, 6, 7);
    w16(&mut frame, 8, (-3i16) as u16);
    w16(&mut frame, 10, 4);
    frame[12] = 0xff;
    frame[13] = 0xa5;
    w32(&mut frame, 14, 0x4000);
    w32(&mut frame, 18, 0x3000);
    w32(&mut frame, 22, 0x2000);
    memory
        .map(GuestPtr(0x1000), frame, Permissions::RW)
        .unwrap();
    let expected = Call::NewControl(NewControl {
        owner: GuestPtr(0x2000),
        bounds: GuestPtr(0x3000),
        title: GuestPtr(0x4000),
        visible: true,
        value: 4,
        min: -3,
        max: 7,
        proc_id: 2,
        ref_con: 0x12345678,
    });
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    let (call, reply) = Abi::trap(0xa954, c, &memory).unwrap();
    assert_eq!(call, expected);
    assert_eq!(reply, Reply::StackLong(GuestPtr(0x101a)));
    assert_eq!(c.dar[15], 0x101a);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0x2000);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    for (register, value) in (3..=10).zip([0x2000, 0x3000, 0x4000, 255, 4, (-3i32) as u32, 7, 2]) {
        c.gpr[register] = value;
    }
    c.lr = 0x1234;
    assert!(Abi::import(Service::NewControl, c, &memory).is_err());
    assert_eq!(c.pc, 0);
    assert_eq!(c.lr, 0x1234);
    memory
        .map(
            GuestPtr(0x2038),
            0x12345678u32.to_be_bytes().to_vec(),
            Permissions::RW,
        )
        .unwrap();
    assert_eq!(
        Abi::import(Service::NewControl, c, &memory).unwrap(),
        (expected, Reply::PpcR3)
    );
    // A read-only native result slot rejects the trap before popping arguments.
    memory
        .map(GuestPtr(0x5000), vec![0; 26], Permissions::RW)
        .unwrap();
    memory
        .map(GuestPtr(0x501a), vec![0; 4], Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x5000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa954, c, &memory).is_err());
    assert_eq!(c.dar[15], 0x5000);
    memory
        .map(GuestPtr(0x6000), vec![0; 26], Permissions::RW)
        .unwrap();
    memory
        .map(
            GuestPtr(0x601a),
            vec![0; 4],
            Permissions {
                read: false,
                write: true,
                execute: false,
            },
        )
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x6000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa954, c, &memory).is_ok());
    assert_eq!(c.dar[15], 0x601a); // result is written, never read during decoding.
}
fn invoke(app: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        for (i, &value) in args.iter().take(8).enumerate() {
            c.gpr[3 + i] = value;
        }
    }
    if args.len() > 8 {
        app.runtime
            .memory
            .write(GuestPtr(stack + 56), &args[8].to_be_bytes())
            .unwrap();
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn pixel(app: &LoadedApplication, x: u32, y: u32) -> bool {
    app.runtime
        .memory
        .read(GuestPtr(SCREEN.0 + y * 64 + x / 8), 1)
        .unwrap()[0]
        & (0x80 >> (x % 8))
        != 0
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn controls_share_records_pixels_live_values_and_window_ownership_across_cpus() {
    let f = pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitFonts",
            b"InitWindows",
            b"NewWindow",
            b"NewControl",
            b"SetCtlValue",
            b"GetCtlValue",
            b"Draw1Control",
            b"HideControl",
            b"ShowControl",
            b"HiliteControl",
            b"DisposeControl",
            b"DisposeWindow",
            b"TestControl",
        ],
        512,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x54], Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    invoke(&mut app, 0, &[data + 330]);
    invoke(&mut app, 1, &[]);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics = Some(
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut app, 2, &[]);
    app.runtime
        .memory
        .write(GuestPtr(0x51000000), &words(&[80, 100, 180, 300]))
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x51000010), b"\x04test")
        .unwrap();
    let window = invoke(
        &mut app,
        3,
        &[0, 0x51000000, 0x51000010, 1, 2, u32::MAX, 0, 0],
    );
    app.runtime
        .memory
        .write(GuestPtr(0x51000000), &words(&[10, 10, 30, 80]))
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x51000010), b"\x02OK")
        .unwrap();
    let saved = app
        .runtime
        .memory
        .read(GuestPtr(window), 108)
        .unwrap()
        .to_vec();
    let button = invoke(
        &mut app,
        4,
        &[window, 0x51000000, 0x51000010, 255, 0, 0, 1, 0, 0x12345678],
    );
    let ptr = GuestPtr(app.runtime.memory.u32(GuestPtr(button)).unwrap());
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(window + 140)).unwrap(),
        button
    );
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(ptr.0 + 36)).unwrap(),
        0x12345678
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(window), 108).unwrap(),
        saved
    );
    let first = app.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
    assert!(pixel(&app, 130, 90));
    assert!(!pixel(&app, 110, 90)); // rounded button top / corner
    invoke(&mut app, 11, &[button]);
    let mut frame = vec![0; 30];
    w32(&mut frame, 0, 0x12345678);
    w16(&mut frame, 6, 1);
    frame[12] = 255;
    frame[13] = 0xa5;
    w32(&mut frame, 14, 0x51000010);
    w32(&mut frame, 18, 0x51000000);
    w32(&mut frame, 22, window);
    frame[26..].fill(0x5a);
    let stack = STACK_BASE + 0x8000;
    app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 26);
    let native = app.runtime.memory.u32(GuestPtr(stack + 26)).unwrap();
    assert_ne!(native, 0);
    assert_eq!(app.runtime.memory.read(SCREEN, 64 * 342).unwrap(), first);
    app.runtime
        .memory
        .write(GuestPtr(0x51000000), &words(&[40, 10, 60, 100]))
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x51000010), b"\x05Check")
        .unwrap();
    let check = invoke(
        &mut app,
        4,
        &[window, 0x51000000, 0x51000010, 1, 0, 0, 1, 1, 9],
    );
    assert!(pixel(&app, 112, 124));
    assert!(!pixel(&app, 113, 125));
    invoke(&mut app, 5, &[check, 99]);
    assert_eq!(invoke(&mut app, 6, &[check]), 1);
    assert!(pixel(&app, 113, 125));
    let check_ptr = app.runtime.memory.u32(GuestPtr(check)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(check_ptr + 18), &0u16.to_be_bytes())
        .unwrap();
    invoke(&mut app, 7, &[check]);
    assert_eq!(invoke(&mut app, 6, &[check]), 0);
    assert!(!pixel(&app, 113, 125));
    invoke(&mut app, 8, &[check]);
    assert_eq!(invoke(&mut app, 13, &[check, (50 << 16) | 20]), 0);
    invoke(&mut app, 9, &[check]);
    assert_eq!(invoke(&mut app, 13, &[check, (50 << 16) | 20]), 11);
    invoke(&mut app, 10, &[check, 255]);
    assert_eq!(invoke(&mut app, 13, &[check, (50 << 16) | 20]), 0);
    invoke(&mut app, 10, &[check, 0]);
    app.runtime
        .memory
        .write(GuestPtr(0x51000000), &words(&[70, 10, 90, 100]))
        .unwrap();
    let radio = invoke(
        &mut app,
        4,
        &[window, 0x51000000, 0x51000010, 1, 1, 0, 1, 2, 10],
    );
    assert!(pixel(&app, 117, 160));
    // Corrupt linkage must not destroy any registered controls or the owner.
    let radio_ptr = app.runtime.memory.u32(GuestPtr(radio)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(radio_ptr), &radio.to_be_bytes())
        .unwrap();
    let mut windows = std::mem::take(&mut app.runtime.process.windows);
    let gpu = app.runtime.graphics.as_ref().unwrap();
    assert!(windows
        .call(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            gpu,
            systemless_future::windows::Op::Dispose,
            window
        )
        .is_err());
    app.runtime.process.windows = windows;
    assert_ne!(app.runtime.memory.u32(GuestPtr(radio)).unwrap(), 0);
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        window
    );
    app.runtime
        .memory
        .write(GuestPtr(radio_ptr), &check.to_be_bytes())
        .unwrap();
    invoke(&mut app, 12, &[window]);
    for h in [native, check, radio] {
        assert_eq!(app.runtime.memory.u32(GuestPtr(h)).unwrap(), 0);
    }
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        0
    );
}

#[test]
fn find_control_adapters_preserve_word_results_and_unpack_local_point_order() {
    let mut memory = Memory::default();
    memory
        .map(
            GuestPtr(0x1000),
            [0x11223344u32, 0x55667788, 0xfff9000d, 0xa5a55a5a]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
            Permissions::RW,
        )
        .unwrap();
    let expected = Call::Control(systemless_future::controls::Op::Find {
        owner: 0x55667788,
        point: [-7, 13],
        destination: GuestPtr(0x11223344),
    });
    let mut cpu = m68k::CpuCore::default();
    cpu.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa96c, &mut cpu, &memory).unwrap(),
        (expected, Reply::StackWord(GuestPtr(0x100c)))
    );
    assert_eq!(cpu.dar[15], 0x100c);
    cpu.dar[15] = 0x1003;
    assert!(Abi::trap(0xa96c, &mut cpu, &memory).is_err());
    assert_eq!(cpu.dar[15], 0x1003);
    cpu.dar[15] = 0x1005;
    assert!(Abi::trap(0xa96c, &mut cpu, &memory).is_err());
    assert_eq!(cpu.dar[15], 0x1005);
    let mut cpu = ppc::PpcCpu::default();
    cpu.gpr[3] = 0xfff9000d;
    cpu.gpr[4] = 0x55667788;
    cpu.gpr[5] = 0x11223344;
    assert_eq!(
        Abi::import(Service::ControlFind, &cpu, &memory).unwrap(),
        (expected, Reply::PpcR3)
    );
}
