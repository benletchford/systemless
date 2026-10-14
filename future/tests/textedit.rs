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
    app.runtime.elapse_ticks(123).unwrap();
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
    let created = ptr(&app.runtime.memory, handle);
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(created.0 + 52)).unwrap(),
        123
    );
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

#[test]
#[ignore = "requires a real wgpu adapter"]
fn redraw_honors_complex_clip_update_view_and_restores_ports_even_on_failure() {
    use systemless_future::{
        graphics::Graphics,
        region_geometry::{self, Boolean},
    };
    let (mut m, mut p, mut manager) = setup();
    m.write(
        GuestPtr(0x1040),
        &[0, 0, 0, 1, 0, 4, 0, 6, 0, 0, 0, 0, 0, 4, 0, 8],
    )
    .unwrap();
    let handle = manager.call(&mut m, &mut p, new(false), 0).unwrap();
    m.map(GuestPtr(0x3000), vec![0; 64], Permissions::RW)
        .unwrap();
    m.write(GuestPtr(0x3000), b"A").unwrap();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 1,
                handle,
            },
            0,
        )
        .unwrap();
    m.write(GuestPtr(0x3020), &[0, 1, 0, 0, 0, 3, 0, 8])
        .unwrap();
    let ports = p.ports.unwrap();
    let port = GuestPtr(0x1080);
    let clip = m.u32(GuestPtr(port.0 + 28)).unwrap();
    let complex = region_geometry::combine(
        &[0, 10, 0, 0, 0, 0, 0, 4, 0, 8],
        &[0, 10, 0, 0, 0, 3, 0, 4, 0, 7],
        Boolean::Difference,
    )
    .unwrap();
    p.set_handle_size(&mut m, clip, complex.len() as u32)
        .unwrap();
    m.write(ptr(&m, clip), &complex).unwrap();
    ports.open(&mut m, &mut p, GuestPtr(0x1180)).unwrap();
    let old = m.u32(ports.current_slot).unwrap();
    let saved = m.read(port, 108).unwrap().to_vec();
    let other = m.read(GuestPtr(old), 108).unwrap().to_vec();
    let before = [1, 0x33, 1, 0x44, 1, 0x55, 1, 0x66];
    m.write(GuestPtr(0x2000), &before).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap();
    manager
        .update(&mut m, &mut p, &gpu, GuestPtr(0x3020), handle)
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x2000), 8).unwrap(),
        [1, 0x33, 0xa1, 0x44, 0x41, 0x55, 1, 0x66]
    );
    assert_eq!(m.u32(ports.current_slot).unwrap(), old);
    assert_eq!(m.read(port, 108).unwrap(), saved);
    assert_eq!(m.read(GuestPtr(old), 108).unwrap(), other);
    assert_eq!(m.read(ptr(&m, clip), complex.len()).unwrap(), complex);
    m.write(GuestPtr(0x3000), b"AA").unwrap();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 2,
                handle,
            },
            0,
        )
        .unwrap();
    m.write(GuestPtr(0x3020), &[0, 0, 0, 0, 0, 4, 0, 8])
        .unwrap();
    m.write(GuestPtr(0x2000), &before).unwrap();
    manager
        .update(&mut m, &mut p, &gpu, GuestPtr(0x3020), handle)
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x2000), 8).unwrap(),
        [0xe1, 0x33, 0xa1, 0x44, 0xe1, 0x55, 0xa1, 0x66]
    );
    assert_eq!(m.read(port, 108).unwrap(), saved);
    assert_eq!(m.u32(ports.current_slot).unwrap(), old);
    let pixels = m.read(GuestPtr(0x2000), 8).unwrap().to_vec();
    m.map(GuestPtr(0x5000), pixels.clone(), Permissions::RX)
        .unwrap();
    m.write(GuestPtr(port.0 + 2), &0x5000u32.to_be_bytes())
        .unwrap();
    let saved = m.read(port, 108).unwrap().to_vec();
    assert!(manager
        .update(&mut m, &mut p, &gpu, GuestPtr(0x3020), handle)
        .is_err());
    assert_eq!(m.read(port, 108).unwrap(), saved);
    assert_eq!(m.u32(ports.current_slot).unwrap(), old);
    assert_eq!(m.read(GuestPtr(0x5000), 8).unwrap(), pixels);
    assert_eq!(m.read(ptr(&m, clip), complex.len()).unwrap(), complex);
    m.write(GuestPtr(port.0 + 2), &0x2000u32.to_be_bytes())
        .unwrap();
    ports.set(&mut m, port).unwrap();
    let styled = manager.call(&mut m, &mut p, new(true), 0).unwrap();
    manager
        .call(
            &mut m,
            &mut p,
            Op::SetText {
                source: GuestPtr(0x3000),
                length: 1,
                handle: styled,
            },
            0,
        )
        .unwrap();
    m.write(GuestPtr(ptr(&m, styled).0 + 58), &(-1i16).to_be_bytes())
        .unwrap();
    ports.set(&mut m, GuestPtr(old)).unwrap();
    m.write(GuestPtr(0x2000), &before).unwrap();
    manager
        .update(&mut m, &mut p, &gpu, GuestPtr(0x3020), styled)
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x2000), 8).unwrap(),
        [0x61, 0x33, 0x41, 0x44, 0x21, 0x55, 1, 0x66]
    );
    assert_eq!(m.u32(ports.current_slot).unwrap(), old);
    let style = manager
        .call(&mut m, &mut p, Op::GetStyle(styled), 0)
        .unwrap();
    let style_table = m.u32(GuestPtr(ptr(&m, style).0 + 4)).unwrap();
    m.write(GuestPtr(ptr(&m, style_table).0 + 12), &[255; 6])
        .unwrap();
    m.write(GuestPtr(0x2000), &before).unwrap();
    manager
        .update(&mut m, &mut p, &gpu, GuestPtr(0x3020), styled)
        .unwrap();
    assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), before);
}

#[test]
fn inactive_selection_clamps_live_length_and_rejects_active_records() {
    for styled in [false, true] {
        let (mut m, mut p, mut manager) = setup();
        let handle = manager.call(&mut m, &mut p, new(styled), 0).unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0x1040),
                    length: 8,
                    handle,
                },
                0,
            )
            .unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::Select {
                    start: 2,
                    end: u32::MAX,
                    handle,
                },
                0,
            )
            .unwrap();
        let data = ptr(&m, handle);
        assert_eq!(m.read(GuestPtr(data.0 + 32), 4).unwrap(), [0, 2, 0, 8]);
        manager
            .call(
                &mut m,
                &mut p,
                Op::Select {
                    start: 7,
                    end: 1,
                    handle,
                },
                0,
            )
            .unwrap();
        assert_eq!(m.read(GuestPtr(data.0 + 32), 4).unwrap(), [0, 7, 0, 1]);
        m.write(GuestPtr(data.0 + 36), &1u16.to_be_bytes()).unwrap();
        let before = m.read(data, 128).unwrap().to_vec();
        assert!(manager
            .call(
                &mut m,
                &mut p,
                Op::Select {
                    start: 0,
                    end: 0,
                    handle
                },
                0
            )
            .is_err());
        assert_eq!(m.read(data, 128).unwrap(), before);
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn deletion_commits_selection_layout_and_erases_stale_pixels_without_touching_scrap() {
    use systemless_future::graphics::Graphics;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap();
    for styled in [false, true] {
        let (mut m, mut p, mut manager) = setup();
        m.write(
            GuestPtr(0x1040),
            &[0, 0, 0, 1, 0, 4, 0, 6, 0, 0, 0, 0, 0, 4, 0, 8],
        )
        .unwrap();
        m.map(GuestPtr(0x3000), b"AAA".to_vec(), Permissions::RW)
            .unwrap();
        manager.call(&mut m, &mut p, Op::Init, 0).unwrap();
        let scrap = m.u32(GuestPtr(0xab4)).unwrap();
        p.set_handle_size(&mut m, scrap, 3).unwrap();
        m.write(ptr(&m, scrap), b"xyz").unwrap();
        m.write(GuestPtr(0xab0), &3u16.to_be_bytes()).unwrap();
        let handle = manager.call(&mut m, &mut p, new(styled), 0).unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0x3000),
                    length: 3,
                    handle,
                },
                0,
            )
            .unwrap();
        let text = manager
            .call(&mut m, &mut p, Op::GetText(handle), 0)
            .unwrap();
        m.write(
            GuestPtr(0x2000),
            &[0xff, 0x33, 0xff, 0x44, 0xff, 0x55, 0xff, 0x66],
        )
        .unwrap();
        let port = GuestPtr(0x1080);
        let ports = p.ports.unwrap();
        ports.open(&mut m, &mut p, GuestPtr(0x1180)).unwrap();
        let old = m.u32(ports.current_slot).unwrap();
        let saved = m.read(port, 108).unwrap().to_vec();
        // Reversed live endpoints are normalized by editing, as in the legacy buffer.
        manager
            .call(
                &mut m,
                &mut p,
                Op::Select {
                    start: 3,
                    end: 1,
                    handle,
                },
                0,
            )
            .unwrap();
        manager.delete(&mut m, &mut p, &gpu, handle, 129).unwrap();
        assert_eq!(
            manager
                .call(&mut m, &mut p, Op::GetText(handle), 0)
                .unwrap(),
            text
        );
        assert_eq!(p.handle_size(&m, text).unwrap(), 1);
        assert_eq!(m.read(ptr(&m, text), 1).unwrap(), b"A");
        let data = ptr(&m, handle);
        assert_eq!(m.read(GuestPtr(data.0 + 32), 4).unwrap(), [0, 1, 0, 1]);
        assert_eq!(m.u16(GuestPtr(data.0 + 94)).unwrap(), 1);
        assert_eq!(m.read(GuestPtr(data.0 + 96), 4).unwrap(), [0, 0, 0, 1]);
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0xe0, 0x33, 0xa0, 0x44, 0x40, 0x55, 0, 0x66]
        );
        assert_eq!(m.read(port, 108).unwrap(), saved);
        assert_eq!(m.u32(ports.current_slot).unwrap(), old);
        assert_eq!(m.read(ptr(&m, scrap), 3).unwrap(), b"xyz");
        assert_eq!(m.u16(GuestPtr(0xab0)).unwrap(), 3);
        // A collapsed selection does not erase even when guest pixels changed.
        m.write(GuestPtr(0x2000), &[0x77]).unwrap();
        manager.delete(&mut m, &mut p, &gpu, handle, 129).unwrap();
        assert_eq!(m.read(GuestPtr(0x2000), 1).unwrap(), [0x77]);
        // Reject unsupported layout callbacks before committing text/selection.
        manager
            .call(
                &mut m,
                &mut p,
                Op::Select {
                    start: 0,
                    end: 1,
                    handle,
                },
                0,
            )
            .unwrap();
        m.write(GuestPtr(data.0 + 38), &1u32.to_be_bytes()).unwrap();
        let before = m.read(data, 128).unwrap().to_vec();
        assert!(manager.delete(&mut m, &mut p, &gpu, handle, 129).is_err());
        assert_eq!(m.read(data, 128).unwrap(), before);
        assert_eq!(m.read(ptr(&m, text), 1).unwrap(), b"A");
        m.write(GuestPtr(data.0 + 38), &0u32.to_be_bytes()).unwrap();
        manager.delete(&mut m, &mut p, &gpu, handle, 129).unwrap();
        assert_eq!(p.handle_size(&m, text).unwrap(), 0);
        let data = ptr(&m, handle);
        assert_eq!(m.read(GuestPtr(data.0 + 32), 4).unwrap(), [0; 4]);
        assert_eq!(m.u16(GuestPtr(data.0 + 94)).unwrap(), 0);
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0, 0x33, 0, 0x44, 0, 0x55, 0, 0x66]
        );
        assert_eq!(m.read(port, 108).unwrap(), saved);
        assert_eq!(m.u32(ports.current_slot).unwrap(), old);
        // Erasure uses bkPat and the original clip, preserving padding and outside pixels.
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0x3000),
                    length: 3,
                    handle,
                },
                0,
            )
            .unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::Select {
                    start: 0,
                    end: 3,
                    handle,
                },
                0,
            )
            .unwrap();
        let clip = m.u32(GuestPtr(port.0 + 28)).unwrap();
        m.write(ptr(&m, clip), &[0, 10, 0, 1, 0, 2, 0, 3, 0, 6])
            .unwrap();
        m.write(
            GuestPtr(port.0 + 32),
            &[0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
        )
        .unwrap();
        let saved = m.read(port, 108).unwrap().to_vec();
        m.write(
            GuestPtr(0x2000),
            &[0xff, 0x33, 0xff, 0x44, 0xff, 0x55, 0xff, 0x66],
        )
        .unwrap();
        manager.delete(&mut m, &mut p, &gpu, handle, 129).unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0xff, 0x33, 0xd7, 0x44, 0xeb, 0x55, 0xff, 0x66]
        );
        assert_eq!(m.read(port, 108).unwrap(), saved);
        assert_eq!(m.u32(ports.current_slot).unwrap(), old);
        // A destination fault after publication preserves the committed edit and port state.
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0x3000),
                    length: 1,
                    handle,
                },
                0,
            )
            .unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::Select {
                    start: 0,
                    end: 1,
                    handle,
                },
                0,
            )
            .unwrap();
        let pixels = m.read(GuestPtr(0x2000), 8).unwrap().to_vec();
        m.map(GuestPtr(0x5000), pixels.clone(), Permissions::RX)
            .unwrap();
        m.write(GuestPtr(port.0 + 2), &0x5000u32.to_be_bytes())
            .unwrap();
        let saved = m.read(port, 108).unwrap().to_vec();
        assert!(manager.delete(&mut m, &mut p, &gpu, handle, 129).is_err());
        assert_eq!(p.handle_size(&m, text).unwrap(), 0);
        assert_eq!(m.read(GuestPtr(0x5000), 8).unwrap(), pixels);
        assert_eq!(m.read(port, 108).unwrap(), saved);
        assert_eq!(m.u32(ports.current_slot).unwrap(), old);
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn active_highlight_caret_and_deletion_are_repeatable_and_restore_port_state() {
    use systemless_future::graphics::Graphics;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap();
    for styled in [false, true] {
        let (mut m, mut p, mut manager) = setup();
        m.write(
            GuestPtr(0x1040),
            &[0, 0, 0, 1, 0, 4, 0, 6, 0, 0, 0, 0, 0, 4, 0, 8],
        )
        .unwrap();
        m.map(GuestPtr(0x3000), b"AA\r".to_vec(), Permissions::RW)
            .unwrap();
        let handle = manager.call(&mut m, &mut p, new(styled), 0).unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0x3000),
                    length: 1,
                    handle,
                },
                0,
            )
            .unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::Select {
                    start: 0,
                    end: 1,
                    handle,
                },
                0,
            )
            .unwrap();
        let port = GuestPtr(0x1080);
        let ports = p.ports.unwrap();
        ports.open(&mut m, &mut p, GuestPtr(0x1180)).unwrap();
        let old = m.u32(ports.current_slot).unwrap();
        let saved = m.read(port, 108).unwrap().to_vec();
        m.write(
            GuestPtr(0x2000),
            &[0xff, 0x33, 0xff, 0x44, 0xff, 0x55, 0xff, 0x66],
        )
        .unwrap();
        manager
            .activate(&mut m, &mut p, &gpu, handle, true, 123)
            .unwrap();
        let data = ptr(&m, handle);
        assert_eq!(m.u16(GuestPtr(data.0 + 36)).unwrap(), 1);
        assert_eq!(m.u32(GuestPtr(data.0 + 52)).unwrap(), 123);
        let highlighted = [0x98, 0x33, 0xd8, 0x44, 0x40, 0x55, 0, 0x66];
        assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), highlighted);
        manager
            .update(&mut m, &mut p, &gpu, GuestPtr(data.0 + 8), handle)
            .unwrap();
        assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), highlighted);
        manager
            .activate(&mut m, &mut p, &gpu, handle, true, 124)
            .unwrap();
        assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), highlighted);
        manager
            .select(&mut m, &mut p, Some(&gpu), handle, (1, 1), 125)
            .unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0xe8, 0x33, 0xa8, 0x44, 0x40, 0x55, 0, 0x66]
        );
        assert_eq!(m.u16(GuestPtr(data.0 + 56)).unwrap(), 1);
        assert_eq!(m.u32(GuestPtr(data.0 + 52)).unwrap(), 125);
        let before = m.read(data, 128).unwrap().to_vec();
        assert!(manager
            .select(&mut m, &mut p, None, handle, (0, 0), 126)
            .is_err());
        assert_eq!(m.read(data, 128).unwrap(), before);
        manager
            .activate(&mut m, &mut p, &gpu, handle, false, 127)
            .unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0xe0, 0x33, 0xa0, 0x44, 0x40, 0x55, 0, 0x66]
        );
        assert_eq!(m.u16(GuestPtr(data.0 + 36)).unwrap(), 0);
        assert_eq!(m.u16(GuestPtr(data.0 + 56)).unwrap(), 0);
        manager
            .activate(&mut m, &mut p, &gpu, handle, true, 128)
            .unwrap();
        manager
            .select(&mut m, &mut p, Some(&gpu), handle, (0, 1), 129)
            .unwrap();
        manager.delete(&mut m, &mut p, &gpu, handle, 129).unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0x40, 0x33, 0x40, 0x44, 0, 0x55, 0, 0x66]
        );
        let data = ptr(&m, handle);
        assert_eq!(m.u16(GuestPtr(data.0 + 60)).unwrap(), 0);
        assert_eq!(m.read(port, 108).unwrap(), saved);
        assert_eq!(m.u32(ports.current_slot).unwrap(), old);
        // Caret blinking uses the guest interval and shared wrapping tick value.
        m.write(GuestPtr(0x2f4), &32u32.to_be_bytes()).unwrap();
        manager.idle(&mut m, &mut p, None, handle, 160).unwrap();
        assert_eq!(m.u16(GuestPtr(data.0 + 56)).unwrap(), 1);
        manager
            .idle(&mut m, &mut p, Some(&gpu), handle, 161)
            .unwrap();
        assert_eq!(m.u16(GuestPtr(data.0 + 56)).unwrap(), 0);
        // A hidden caret stays hidden during an update.
        manager
            .update(&mut m, &mut p, &gpu, GuestPtr(data.0 + 8), handle)
            .unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0, 0x33, 0, 0x44, 0, 0x55, 0, 0x66]
        );
        manager
            .idle(&mut m, &mut p, Some(&gpu), handle, 193)
            .unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0x40, 0x33, 0x40, 0x44, 0, 0x55, 0, 0x66]
        );
        m.write(GuestPtr(data.0 + 52), &0xfffffff0u32.to_be_bytes())
            .unwrap();
        manager
            .idle(&mut m, &mut p, Some(&gpu), handle, 16)
            .unwrap();
        assert_eq!(m.u16(GuestPtr(data.0 + 56)).unwrap(), 0);
        assert_eq!(m.u32(GuestPtr(data.0 + 52)).unwrap(), 16);
        // Wrapped lines select separate bands; update clipping leaves other rows intact.
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0x3000),
                    length: 2,
                    handle,
                },
                0,
            )
            .unwrap();
        manager
            .select(&mut m, &mut p, Some(&gpu), handle, (0, 2), 130)
            .unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0x98, 0x33, 0xd8, 0x44, 0x98, 0x55, 0xd8, 0x66]
        );
        m.map(
            GuestPtr(0x3100),
            vec![0, 1, 0, 0, 0, 3, 0, 8],
            Permissions::RW,
        )
        .unwrap();
        m.write(GuestPtr(0x2000), &[0x11]).unwrap();
        m.write(GuestPtr(0x2006), &[0x22]).unwrap();
        manager
            .update(&mut m, &mut p, &gpu, GuestPtr(0x3100), handle)
            .unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0x11, 0x33, 0xd8, 0x44, 0x98, 0x55, 0x22, 0x66]
        );
        // A selected newline extends the highlight to the right edge of the view.
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0x3001),
                    length: 2,
                    handle,
                },
                0,
            )
            .unwrap();
        manager
            .select(&mut m, &mut p, Some(&gpu), handle, (0, 2), 131)
            .unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0x9f, 0x33, 0xdf, 0x44, 0x40, 0x55, 0, 0x66]
        );
        // The insertion point after a trailing CR lies on the next empty line.
        manager
            .select(&mut m, &mut p, Some(&gpu), handle, (2, 2), 132)
            .unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0xe0, 0x33, 0xa0, 0x44, 0, 0x55, 0x40, 0x66]
        );
        let data = ptr(&m, handle);
        // Application selection hooks must fail before activation state changes.
        m.write(GuestPtr(data.0 + 86), &1u32.to_be_bytes()).unwrap();
        let before = m.read(data, 128).unwrap().to_vec();
        assert!(manager
            .activate(&mut m, &mut p, &gpu, handle, false, 130)
            .is_err());
        assert_eq!(m.read(data, 128).unwrap(), before);
        assert_eq!(m.read(port, 108).unwrap(), saved);
        assert_eq!(m.u32(ports.current_slot).unwrap(), old);
    }
}

fn live_text(m: &Memory, handle: u32) -> Vec<u8> {
    let data = ptr(m, handle);
    let length = m.u16(GuestPtr(data.0 + 60)).unwrap() as usize;
    let text = m.u32(GuestPtr(data.0 + 62)).unwrap();
    m.read(ptr(m, text), length).unwrap().to_vec()
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn key_editing_uses_live_selection_preserves_scrap_and_rolls_back_failed_growth() {
    use systemless_future::graphics::Graphics;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap();
    for styled in [false, true] {
        let (mut m, mut p, mut manager) = setup();
        m.write(
            GuestPtr(0x1040),
            &[0, 0, 0, 1, 0, 4, 0, 6, 0, 0, 0, 0, 0, 4, 0, 8],
        )
        .unwrap();
        manager.call(&mut m, &mut p, Op::Init, 0).unwrap();
        let scrap = m.u32(GuestPtr(0xab4)).unwrap();
        p.set_handle_size(&mut m, scrap, 3).unwrap();
        m.write(ptr(&m, scrap), b"xyz").unwrap();
        m.write(GuestPtr(0xab0), &3u16.to_be_bytes()).unwrap();
        let handle = manager.call(&mut m, &mut p, new(styled), 0).unwrap();
        let text = manager
            .call(&mut m, &mut p, Op::GetText(handle), 0)
            .unwrap();
        let ports = p.ports.unwrap();
        let port = GuestPtr(0x1080);
        ports.open(&mut m, &mut p, GuestPtr(0x1180)).unwrap();
        let saved = m.read(port, 108).unwrap().to_vec();
        let old = m.u32(ports.current_slot).unwrap();
        m.write(GuestPtr(0x2000), &[0, 0x33, 0, 0x44, 0, 0x55, 0, 0x66])
            .unwrap();
        // Growing past the locked four-byte allocation cannot move its block.
        m.map(GuestPtr(0x3000), b"AAAA".to_vec(), Permissions::RW)
            .unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0x3000),
                    length: 4,
                    handle,
                },
                0,
            )
            .unwrap();
        p.set_handle_lock(&m, text, true).unwrap();
        let data = ptr(&m, handle);
        let before = m.read(data, 128).unwrap().to_vec();
        manager.key(&mut m, &mut p, &gpu, handle, b'A', 1).unwrap();
        assert_eq!(p.mem_error, -108);
        assert_eq!(m.read(data, 128).unwrap(), before);
        assert_eq!(live_text(&m, handle), b"AAAA");
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0, 0x33, 0, 0x44, 0, 0x55, 0, 0x66]
        );
        p.set_handle_lock(&m, text, false).unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0),
                    length: 0,
                    handle,
                },
                0,
            )
            .unwrap();
        manager.key(&mut m, &mut p, &gpu, handle, b'A', 2).unwrap();
        assert_eq!(live_text(&m, handle), b"A");
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0xe0, 0x33, 0xa0, 0x44, 0x40, 0x55, 0, 0x66]
        );
        manager
            .activate(&mut m, &mut p, &gpu, handle, true, 3)
            .unwrap();
        assert_eq!(
            m.read(GuestPtr(0x2000), 8).unwrap(),
            [0xe8, 0x33, 0xa8, 0x44, 0x40, 0x55, 0, 0x66]
        );
        for (byte, expected, caret) in [
            (0x1c, &b"A"[..], 0),
            (b'B', &b"BA"[..], 1),
            (0x1d, &b"BA"[..], 2),
            (0x08, &b"B"[..], 1),
            (0x7f, &b""[..], 0),
            (0x08, &b""[..], 0),
            (0x80, &[0x80][..], 1),
            (0x08, &b""[..], 0),
            (13, &[13][..], 1),
            (0x1c, &[13][..], 0),
            (0x1d, &[13][..], 1),
            (0x08, &b""[..], 0),
        ] {
            manager.key(&mut m, &mut p, &gpu, handle, byte, 10).unwrap();
            assert_eq!(live_text(&m, handle), expected);
            let data = ptr(&m, handle);
            assert_eq!(m.u16(GuestPtr(data.0 + 32)).unwrap(), caret);
            assert_eq!(m.u16(GuestPtr(data.0 + 34)).unwrap(), caret);
            assert_eq!(m.u32(GuestPtr(data.0 + 52)).unwrap(), 10);
            assert_eq!(
                manager
                    .call(&mut m, &mut p, Op::GetText(handle), 0)
                    .unwrap(),
                text
            );
        }
        // Guest-written reversed selection is normalized once during replacement.
        manager.key(&mut m, &mut p, &gpu, handle, b'A', 11).unwrap();
        manager.key(&mut m, &mut p, &gpu, handle, b'B', 12).unwrap();
        manager
            .select(&mut m, &mut p, Some(&gpu), handle, (2, 0), 13)
            .unwrap();
        manager.key(&mut m, &mut p, &gpu, handle, b'A', 14).unwrap();
        assert_eq!(live_text(&m, handle), b"A");
        let data = ptr(&m, handle);
        let before = m.read(data, 128).unwrap().to_vec();
        assert!(manager.key(&mut m, &mut p, &gpu, handle, 0x1e, 15).is_err());
        assert_eq!(m.read(data, 128).unwrap(), before);
        assert_eq!(m.read(port, 108).unwrap(), saved);
        assert_eq!(m.u32(ports.current_slot).unwrap(), old);
        assert_eq!(m.read(ptr(&m, scrap), 3).unwrap(), b"xyz");
        assert_eq!(m.u16(GuestPtr(0xab0)).unwrap(), 3);
        // Length overflow must fail before allocation or editing.
        m.map(GuestPtr(0x6000), vec![b'A'; 32767], Permissions::RW)
            .unwrap();
        manager
            .call(
                &mut m,
                &mut p,
                Op::SetText {
                    source: GuestPtr(0x6000),
                    length: 32767,
                    handle,
                },
                0,
            )
            .unwrap();
        let data = ptr(&m, handle);
        let before = m.read(data, 128).unwrap().to_vec();
        assert!(manager.key(&mut m, &mut p, &gpu, handle, b'A', 16).is_err());
        assert_eq!(m.read(data, 128).unwrap(), before);
        assert_eq!(p.handle_size(&m, text).unwrap(), 32767);
        assert_eq!(live_text(&m, handle), vec![b'A'; 32767]);
    }
}

#[test]
fn key_adapters_read_the_low_character_byte_and_preserve_void_results() {
    use systemless_future::{
        abi::{Abi, Reply, Service},
        cpu::{Architecture, Call, Cpu},
    };
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0x11, 0x22, 0x33, 0x44, 0xab, 0x80, 0x5a, 0xa5],
        Permissions::RW,
    )
    .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    c.dar[0] = 0x1234;
    let (call, reply) = Abi::trap(0xa9dc, c, &m).unwrap();
    assert_eq!(
        call,
        Call::TextEdit(Op::Key {
            handle: 0x11223344,
            byte: 0x80
        })
    );
    assert_eq!(reply, Reply::None);
    assert_eq!(c.dar[15], 0x1006);
    reply.apply(&mut cpu, &mut m, 0xffff, 0).unwrap();
    let Cpu::M68k(c) = &cpu else { panic!() };
    assert_eq!(c.dar[0], 0x1234);
    assert_eq!(m.read(GuestPtr(0x1006), 2).unwrap(), [0x5a, 0xa5]);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0x1000);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0xffffff80;
    c.gpr[4] = 0x11223344;
    let (call, reply) = Abi::import(Service::TEKey, c, &m).unwrap();
    assert_eq!(
        call,
        Call::TextEdit(Op::Key {
            handle: 0x11223344,
            byte: 0x80
        })
    );
    reply.apply(&mut cpu, &mut m, 0xffff, 0).unwrap();
    let Cpu::PowerPc(c) = &cpu else { panic!() };
    assert_eq!(c.gpr[3], 0xffffff80);
    let mut short = Memory::default();
    short
        .map(GuestPtr(0x1000), vec![0; 4], Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa9dc, c, &short).is_err());
    assert_eq!(c.dar[15], 0x1000);
}
