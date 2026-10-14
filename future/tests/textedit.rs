use systemless_future::{
    load::resource::ResourceFork,
    memory::{GuestPtr, Memory, Permissions},
    process::{Process, HEAP_SIZE},
    quickdraw::Ports,
    textedit::{Manager, Op},
};
mod support;
fn setup() -> (Memory, Process, Manager) {
    let words = [
        0x9000u16, 65, 66, 4, 0xfffe, 0xffff, 4, 3, 12, 2, 1, 0xffff, 1, 0xe400, 0xba00, 0x4600, 0,
        3, 5, 7, 0x0104, 0xffff, 0x0203, 0xffff,
    ];
    let font = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let mut p = Process::default();
    p.install_application_resources(
        ResourceFork::parse(&support::resource(vec![(*b"FONT", 393, None, font)])).unwrap(),
    );
    let mut m = Memory::default();
    m.map(GuestPtr(0), vec![0; 4096], Permissions::RW).unwrap();
    m.map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0; 8], Permissions::RW)
        .unwrap();
    m.write(
        GuestPtr(0x1000),
        &[0, 0, 0x20, 0, 0, 2, 0, 0, 0, 0, 0, 4, 0, 8],
    )
    .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1020),
        screen_bitmap: GuestPtr(0x1000),
    };
    ports.open(&mut m, &mut p, GuestPtr(0x1080)).unwrap();
    m.write(GuestPtr(0x1080 + 68), &[0, 3, 0, 0xa5, 0, 1, 0, 9])
        .unwrap();
    p.ports = Some(ports);
    m.write(
        GuestPtr(0x1040),
        &[0, 2, 0, 3, 0, 40, 0, 80, 0, 1, 0, 1, 0, 50, 0, 90],
    )
    .unwrap();
    (m, p, Manager::default())
}
fn new(styled: bool) -> Op {
    Op::New {
        dest: GuestPtr(0x1040),
        view: GuestPtr(0x1048),
        styled,
    }
}
fn ptr(m: &Memory, h: u32) -> GuestPtr {
    GuestPtr(m.u32(GuestPtr(h)).unwrap())
}
#[test]
fn textedit_records_style_graph_and_disposal_use_live_guest_handles() {
    let (mut m, mut p, mut manager) = setup();
    m.write(GuestPtr(0xab2), &0xa55au16.to_be_bytes()).unwrap();
    manager.call(&mut m, &mut p, Op::Init, 0).unwrap();
    let scrap = m.u32(GuestPtr(0xab4)).unwrap();
    manager.call(&mut m, &mut p, Op::Init, 0).unwrap();
    assert_eq!(m.u32(GuestPtr(0xab4)).unwrap(), scrap);
    assert_eq!(m.u16(GuestPtr(0xab2)).unwrap(), 0xa55a);
    let plain = manager.call(&mut m, &mut p, new(false), 123).unwrap();
    let styled = manager.call(&mut m, &mut p, new(true), 123).unwrap();
    let plain_data = ptr(&m, plain);
    assert_eq!(
        m.read(GuestPtr(plain_data.0 + 74), 8).unwrap(),
        [0, 3, 0, 0, 0, 1, 0, 9]
    );
    assert_eq!(m.u16(GuestPtr(plain_data.0 + 24)).unwrap(), 2);
    assert_eq!(m.u16(GuestPtr(plain_data.0 + 26)).unwrap(), 2);
    assert_eq!(m.u32(GuestPtr(plain_data.0 + 52)).unwrap(), 123);
    assert_eq!(
        manager
            .call(&mut m, &mut p, Op::GetStyle(plain), 0)
            .unwrap(),
        0
    );
    let text = manager
        .call(&mut m, &mut p, Op::GetText(styled), 0)
        .unwrap();
    assert_eq!(p.handle_size(&m, text).unwrap(), 0);
    let record = ptr(&m, styled);
    assert_eq!(m.u16(GuestPtr(record.0 + 80)).unwrap(), 0xffff);
    assert_eq!(m.read(GuestPtr(record.0 + 24), 4).unwrap(), [255; 4]);
    let style = manager
        .call(&mut m, &mut p, Op::GetStyle(styled), 0)
        .unwrap();
    let table = ptr(&m, style);
    assert_eq!(m.read(table, 4).unwrap(), [0, 1, 0, 1]);
    let st = m.u32(GuestPtr(table.0 + 4)).unwrap();
    let stdata = ptr(&m, st);
    assert_eq!(
        m.read(stdata, 12).unwrap(),
        [0, 1, 0, 2, 0, 2, 0, 3, 0, 0, 0, 9]
    );
    let line = m.u32(GuestPtr(table.0 + 8)).unwrap();
    m.write(GuestPtr(table.0 + 8), &st.to_be_bytes()).unwrap();
    assert!(manager
        .call(&mut m, &mut p, Op::Dispose(styled), 0)
        .is_err());
    assert_eq!(p.handle_size(&m, styled).unwrap(), 128);
    assert_eq!(p.handle_size(&m, st).unwrap(), 18);
    m.write(GuestPtr(table.0 + 8), &line.to_be_bytes()).unwrap();
    p.set_handle_size(&mut m, styled, 256).unwrap();
    assert_ne!(ptr(&m, styled), record);
    assert_eq!(
        manager
            .call(&mut m, &mut p, Op::GetText(styled), 0)
            .unwrap(),
        text
    );
    manager.call(&mut m, &mut p, Op::Dispose(plain), 0).unwrap();
    manager
        .call(&mut m, &mut p, Op::Dispose(styled), 0)
        .unwrap();
    assert_eq!(m.u32(GuestPtr(styled)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(text)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(style)).unwrap(), 0);
    assert!(manager
        .call(&mut m, &mut p, Op::GetText(styled), 0)
        .is_err());
    assert_ne!(p.new_pointer(&mut m, HEAP_SIZE - 92, false).unwrap(), 0);
}
#[test]
fn partially_allocated_styled_record_is_rolled_back_on_exhaustion() {
    let (mut m, mut p, mut manager) = setup();
    manager.call(&mut m, &mut p, Op::Init, 0).unwrap();
    let prime = manager.call(&mut m, &mut p, new(false), 0).unwrap();
    manager.call(&mut m, &mut p, Op::Dispose(prime), 0).unwrap();
    let held = p.new_pointer(&mut m, HEAP_SIZE - 92 - 180, false).unwrap();
    assert_ne!(held, 0);
    assert_eq!(manager.call(&mut m, &mut p, new(true), 0).unwrap(), 0);
    assert_eq!(p.mem_error, -108);
    p.dispose_pointer(held);
    assert_ne!(p.new_pointer(&mut m, HEAP_SIZE - 92, false).unwrap(), 0);
}

#[test]
fn real_cpu_adapters_share_styled_textedit_record_and_owned_lifetime() {
    use systemless_future::{
        cpu::{Architecture, Cpu},
        load::{LoadedApplication, STACK_BASE},
        runtime::Stop,
    };
    let (source, process, _) = setup();
    let f = support::pef_program(
        &[0x4e800020],
        &[b"TEStyleNew", b"TEGetText", b"TESetText", b"TECalText"],
        32,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .process
        .install_application_resources(process.resources);
    app.runtime
        .memory
        .map(
            GuestPtr(0x1000),
            source.read(GuestPtr(0x1000), 512).unwrap().to_vec(),
            Permissions::RW,
        )
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x2000), vec![0; 8], Permissions::RW)
        .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1020),
        screen_bitmap: GuestPtr(0x1000),
    };
    ports
        .open(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            GuestPtr(0x1080),
        )
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1080 + 68), &[0, 3, 0, 0xa5, 0, 1, 0, 9])
        .unwrap();
    app.runtime.process.ports = Some(ports);
    let stack = STACK_BASE + 0x8000;
    let gate = |app: &LoadedApplication, index: u32| {
        let vector = app
            .runtime
            .memory
            .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        app.runtime.memory.u32(GuestPtr(vector)).unwrap()
    };
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate(&app, 0), stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = 0x1040;
        c.gpr[4] = 0x1048;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    let handle = c.gpr[3];
    assert_ne!(handle, 0);
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa8, 0x3d, 0xa9, 0xcd, 0xa9, 0xcf, 0xa9, 0xd0],
            Permissions::RX,
        )
        .unwrap();
    let mut args = 4u16.to_be_bytes().to_vec();
    args.extend(handle.to_be_bytes());
    args.extend([0x5a; 6]);
    app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 6);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(stack + 10)).unwrap(),
        0x5a5a
    );
    let style = app.runtime.memory.u32(GuestPtr(stack + 6)).unwrap();
    assert_ne!(style, 0);
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate(&app, 1), stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = handle;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    let text = c.gpr[3];
    assert_ne!(text, 0);
    assert_eq!(
        app.runtime
            .process
            .handle_size(&app.runtime.memory, text)
            .unwrap(),
        0
    );

    app.runtime
        .memory
        .map(GuestPtr(0x3000), b"AB CD EF".to_vec(), Permissions::RW)
        .unwrap();
    let mut frame = handle.to_be_bytes().to_vec();
    frame.extend(8u32.to_be_bytes());
    frame.extend(0x3000u32.to_be_bytes());
    frame.extend([0x5a; 4]);
    app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000004, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 12);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(stack + 12)).unwrap(),
        0x5a5a
    );
    let data = ptr(&app.runtime.memory, handle);
    assert_eq!(app.runtime.memory.u16(GuestPtr(data.0 + 60)).unwrap(), 8);
    assert_eq!(
        app.runtime
            .memory
            .read(ptr(&app.runtime.memory, text), 8)
            .unwrap(),
        b"AB CD EF"
    );
    app.runtime
        .memory
        .write(GuestPtr(data.0 + 6), &13i16.to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate(&app, 3), stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = handle;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime
            .memory
            .u16(GuestPtr(ptr(&app.runtime.memory, handle).0 + 94))
            .unwrap(),
        3
    );
    app.runtime
        .memory
        .write(GuestPtr(stack), &handle.to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000002, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.memory.u32(GuestPtr(handle)).unwrap(), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(style)).unwrap(), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(text)).unwrap(), 0);
}

#[test]
fn replacement_layout_and_recalculation_preserve_handles_and_capture_record_fonts() {
    let (mut m, mut p, mut manager) = setup();
    let handle = manager.call(&mut m, &mut p, new(true), 0).unwrap();
    let text = manager
        .call(&mut m, &mut p, Op::GetText(handle), 0)
        .unwrap();
    let data = ptr(&m, handle);
    m.write(GuestPtr(data.0 + 6), &13i16.to_be_bytes()).unwrap(); // destination width 10
    m.map(GuestPtr(0x3000), b"AB CD EF".to_vec(), Permissions::RW)
        .unwrap();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 8,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(p.mem_error, 0);
    assert_eq!(
        manager
            .call(&mut m, &mut p, Op::GetText(handle), 0)
            .unwrap(),
        text
    );
    assert_eq!(m.read(ptr(&m, text), 8).unwrap(), b"AB CD EF");
    let data = ptr(&m, handle);
    assert_eq!(m.u16(GuestPtr(data.0 + 94)).unwrap(), 3);
    assert_eq!(
        m.read(GuestPtr(data.0 + 96), 10).unwrap(),
        [0, 0, 0, 3, 0, 6, 0, 8, 0, 0]
    );
    assert_eq!(m.u16(GuestPtr(data.0 + 32)).unwrap(), 8);
    assert_eq!(m.u16(GuestPtr(data.0 + 34)).unwrap(), 8);
    let style = manager
        .call(&mut m, &mut p, Op::GetStyle(handle), 0)
        .unwrap();
    let table = ptr(&m, style);
    assert_eq!(m.u16(GuestPtr(table.0 + 24)).unwrap(), 9);
    let lines = m.u32(GuestPtr(table.0 + 8)).unwrap();
    assert_eq!(p.handle_size(&m, lines).unwrap(), 16);
    // Current port changes do not replace captured TextEdit attributes.
    m.write(GuestPtr(0x1080 + 68), &99i16.to_be_bytes())
        .unwrap();
    manager
        .call(&mut m, &mut p, Op::Calculate(handle), 0)
        .unwrap();
    assert_eq!(m.u16(GuestPtr(ptr(&m, handle).0 + 94)).unwrap(), 3);
    let source = ptr(&m, text);
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source,
                length: 2,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(m.read(ptr(&m, text), 2).unwrap(), b"AB");
    assert_eq!(m.u16(GuestPtr(ptr(&m, handle).0 + 94)).unwrap(), 1);
    let data = ptr(&m, handle);
    m.write(GuestPtr(data.0 + 72), &(-1i16).to_be_bytes())
        .unwrap();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 8,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(m.u16(GuestPtr(ptr(&m, handle).0 + 94)).unwrap(), 1);
    m.write(GuestPtr(0x3000), b"AB\r\nCD").unwrap();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 6,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(ptr(&m, handle).0 + 96), 8).unwrap(),
        [0, 0, 0, 4, 0, 6, 0, 0]
    );
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(u32::MAX),
                length: 0,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(p.handle_size(&m, text).unwrap(), 0);
    assert_eq!(m.u16(GuestPtr(ptr(&m, handle).0 + 94)).unwrap(), 0);
    m.write(GuestPtr(ptr(&m, handle).0 + 72), &0u16.to_be_bytes())
        .unwrap();
    m.map(GuestPtr(0x4000), vec![b' '; 32767], Permissions::RW)
        .unwrap();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x4000),
                length: 32767,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(m.u16(GuestPtr(ptr(&m, handle).0 + 60)).unwrap(), 32767);
    assert_eq!(m.u16(GuestPtr(ptr(&m, handle).0 + 94)).unwrap(), 1);
    assert_eq!(m.u16(GuestPtr(ptr(&m, style).0 + 24)).unwrap(), 32768);
}
#[test]
fn failed_text_group_resize_preserves_record_text_and_style_payloads() {
    let (mut m, mut p, mut manager) = setup();
    let handle = manager.call(&mut m, &mut p, new(true), 0).unwrap();
    let text = manager
        .call(&mut m, &mut p, Op::GetText(handle), 0)
        .unwrap();
    m.map(GuestPtr(0x3000), vec![b'A'; 100], Permissions::RW)
        .unwrap();
    let data = ptr(&m, handle);
    m.write(GuestPtr(data.0 + 6), &4i16.to_be_bytes()).unwrap();
    let held = p.new_pointer(&mut m, HEAP_SIZE - 512, false).unwrap();
    assert_ne!(held, 0);
    let original = m.read(ptr(&m, handle), 128).unwrap().to_vec();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 100,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(m.read(ptr(&m, handle), 128).unwrap(), original);
    assert_eq!(p.handle_size(&m, text).unwrap(), 0);
    p.dispose_pointer(held);
    p.set_handle_lock(&m, text, true).unwrap();
    let before = m.read(ptr(&m, handle), 128).unwrap().to_vec();
    let style = manager
        .call(&mut m, &mut p, Op::GetStyle(handle), 0)
        .unwrap();
    let before_style = m.read(ptr(&m, style), 28).unwrap().to_vec();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 100,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(m.read(ptr(&m, handle), 128).unwrap(), before);
    assert_eq!(m.read(ptr(&m, style), 28).unwrap(), before_style);
    assert_eq!(p.handle_size(&m, text).unwrap(), 0);
    p.set_handle_lock(&m, text, false).unwrap();
    p.set_handle_lock(&m, handle, true).unwrap();
    m.write(GuestPtr(ptr(&m, handle).0 + 6), &4i16.to_be_bytes())
        .unwrap();
    let before = m.read(ptr(&m, handle), 128).unwrap().to_vec();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 100,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(m.read(ptr(&m, handle), 128).unwrap(), before);
    assert_eq!(p.handle_size(&m, text).unwrap(), 0);
    p.set_handle_lock(&m, handle, false).unwrap();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 100,
                handle,
            },
            0,
        )
        .unwrap();
    assert_eq!(p.mem_error, 0);
    assert_eq!(m.u16(GuestPtr(ptr(&m, handle).0 + 94)).unwrap(), 100);
    assert_eq!(p.handle_size(&m, text).unwrap(), 100);
    assert!(manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 32768,
                handle
            },
            0
        )
        .is_err());
}
