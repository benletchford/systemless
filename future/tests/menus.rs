mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    graphics::Graphics,
    load::{resource::ResourceFork, LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    menus::{Menus, Op},
    process::{Process, HEAP_SIZE},
    quickdraw::{Ports, SCREEN},
    runtime::Stop,
};
fn ptr(m: &Memory, h: u32) -> GuestPtr {
    GuestPtr(m.u32(GuestPtr(h)).unwrap())
}
fn payload(m: &Memory, p: &mut Process, h: u32) -> Vec<u8> {
    let n = p.handle_size(m, h).unwrap();
    m.read(ptr(m, h), n as usize).unwrap().to_vec()
}
fn ports(m: &mut Memory, p: &mut Process) {
    m.map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    m.write(
        GuestPtr(0x1000),
        &[0x74, 0x10, 0, 0, 0, 64, 0, 0, 0, 0, 1, 0x56, 2, 0],
    )
    .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1020),
        screen_bitmap: GuestPtr(0x1000),
    };
    ports.open(m, p, GuestPtr(0x1100)).unwrap();
    p.ports = Some(ports);
}
fn direct(app: &mut LoadedApplication, op: Op) -> systemless_future::Result<u32> {
    let mut manager = std::mem::take(&mut app.runtime.process.menus);
    let result = manager.call(
        &mut app.runtime.memory,
        &mut app.runtime.process,
        app.runtime.graphics.as_ref(),
        op,
    );
    app.runtime.process.menus = manager;
    result
}
#[test]
fn menu_adapters_preserve_void_results_and_validate_snapshot_storage() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 8], Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa93b, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1000);
    let (call, reply) = Abi::trap(0xa930, c, &m).unwrap();
    assert_eq!(call, Call::Menu(Op::Init));
    assert_eq!(reply, Reply::None);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0x1000);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0xa55a;
    let (call, reply) = Abi::import(Service::Menu(Op::Init), c, &m).unwrap();
    assert_eq!(call, Call::Menu(Op::Init));
    reply.apply(&mut cpu, &mut m, 99, 0).unwrap();
    let Cpu::PowerPc(c) = &cpu else { panic!() };
    assert_eq!(c.gpr[3], 0xa55a);
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn menu_state_resources_pixels_and_allocation_failures_share_one_world() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu =
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap();
    let mut entry = vec![0; 30];
    entry[22..28].fill(255);
    entry[28..].copy_from_slice(&0xbeefu16.to_be_bytes());
    let mut color_resource = 1u16.to_be_bytes().to_vec();
    color_resource.extend(&entry);
    let resources = support::resource(vec![(*b"mctb", 0, None, color_resource)]);
    let fixture = support::pef_program(&[0x4e800020], &[b"InitMenus"], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, Some(&resources)).unwrap();
    ports(&mut app.runtime.memory, &mut app.runtime.process);
    let gray = app
        .runtime
        .process
        .new_handle(&mut app.runtime.memory, 10, false)
        .unwrap();
    app.runtime
        .memory
        .write(
            ptr(&app.runtime.memory, gray),
            &[0, 10, 0, 0, 0, 0, 1, 0x56, 2, 0],
        )
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x9ee), &gray.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(SCREEN, &vec![0xaa; 64 * 342])
        .unwrap();
    let port = app
        .runtime
        .memory
        .read(GuestPtr(0x1100), 108)
        .unwrap()
        .to_vec();
    let old = app.runtime.memory.u32(GuestPtr(0x1020)).unwrap();
    app.runtime.graphics = Some(gpu);
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
    let list = app.runtime.memory.u32(GuestPtr(0xa1c)).unwrap();
    let colors = app.runtime.memory.u32(GuestPtr(0xd50)).unwrap();
    assert_ne!(list, 0);
    assert_ne!(colors, 0);
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, list),
        [0; 12]
    );
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, colors),
        entry
    );
    assert_eq!(app.runtime.memory.u16(GuestPtr(0xbaa)).unwrap(), 20);
    assert_eq!(
        app.runtime
            .memory
            .u16(GuestPtr(ptr(&app.runtime.memory, gray).0 + 2))
            .unwrap(),
        20
    );
    let pixels = app.runtime.memory.read(SCREEN, 64 * 342).unwrap();
    for y in 0..342 {
        for x in 0..512 {
            let corner = match y {
                0 => 5,
                1 => 3,
                2 => 2,
                3 | 4 => 1,
                _ => 0,
            };
            let expected = if y >= 20 {
                (0xaa >> (7 - (x % 8))) & 1
            } else {
                u8::from(y == 19 || x < corner || x >= 512 - corner)
            };
            assert_eq!(
                (pixels[y * 64 + x / 8] >> (7 - x % 8)) & 1,
                expected,
                "{x},{y}"
            );
        }
    }
    // Resource handles are canonical: an already loaded mctb edit is observed on reinit.
    let source = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"mctb", 0)
        .unwrap();
    let source_data = ptr(&app.runtime.memory, source);
    app.runtime
        .memory
        .write(GuestPtr(source_data.0 + 24), &[0; 6])
        .unwrap();
    direct(&mut app, Op::Init).unwrap();
    assert_eq!(
        app.runtime.memory.read(SCREEN, 64 * 20).unwrap(),
        vec![255; 64 * 20]
    );
    assert_eq!(
        app.runtime
            .process
            .get_resource(&mut app.runtime.memory, *b"mctb", 0)
            .unwrap(),
        source
    );
    app.runtime
        .memory
        .write(GuestPtr(source_data.0 + 24), &[255; 6])
        .unwrap();
    direct(&mut app, Op::Init).unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa9, 0x30, 0xa9, 0x3b, 0xa9, 0x34],
            Permissions::RX,
        )
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(stack), &[0xa5; 8])
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000002, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let copy = app.runtime.memory.u32(GuestPtr(stack)).unwrap();
    assert_ne!(copy, list);
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, copy),
        [0; 12]
    );
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(stack + 4)).unwrap(),
        0xa5a5a5a5
    );
    app.runtime
        .process
        .dispose_handle(&mut app.runtime.memory, copy)
        .unwrap();
    // A valid list may refer to menus whose rendering is still unsupported.
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, list, 18)
        .unwrap();
    let mut populated = vec![0; 18];
    populated[1] = 6;
    populated[6..10].copy_from_slice(&0xdeadbeefu32.to_be_bytes());
    app.runtime
        .memory
        .write(ptr(&app.runtime.memory, list), &populated)
        .unwrap();
    let copy = direct(&mut app, Op::Snapshot).unwrap();
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, copy),
        populated
    );
    app.runtime
        .process
        .dispose_handle(&mut app.runtime.memory, copy)
        .unwrap();
    assert!(direct(&mut app, Op::Draw).is_err());
    app.runtime
        .memory
        .write(GuestPtr(0xa26), &77u16.to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000004, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, list),
        [0; 12]
    );
    assert_eq!(app.runtime.memory.u16(GuestPtr(0xa26)).unwrap(), 0);
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa1c)).unwrap(), list);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xd50)).unwrap(), colors);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1100), 108).unwrap(),
        port
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x1020)).unwrap(), old);
    // Malformed color resources are rejected before publishing changes.
    let before = app.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, source, 2)
        .unwrap();
    app.runtime
        .memory
        .write(ptr(&app.runtime.memory, source), &[0, 2])
        .unwrap();
    assert!(direct(&mut app, Op::Init).is_err());
    assert_eq!(app.runtime.memory.read(SCREEN, 64 * 342).unwrap(), before);
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, colors),
        entry
    );
    // A black live RGB4 controls the background; initialization retains unrelated live color state.
    app.runtime
        .process
        .release_resource(&mut app.runtime.memory, source)
        .unwrap();
    app.runtime
        .process
        .install_application_resources(ResourceFork::default());
    app.runtime
        .memory
        .write(GuestPtr(ptr(&app.runtime.memory, colors).0 + 22), &[0; 6])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(),
        (-192i16) as u16
    );
    assert_eq!(
        app.runtime.memory.read(SCREEN, 64 * 20).unwrap(),
        vec![255; 64 * 20]
    );
    assert_eq!(
        app.runtime
            .memory
            .read(GuestPtr(SCREEN.0 + 64 * 20), 64)
            .unwrap(),
        [0xaa; 64]
    );
    // Partial first initialization rolls back both handles and never publishes globals.
    let mut m = Memory::default();
    m.map(GuestPtr(0), vec![0; 4096], Permissions::RW).unwrap();
    m.map(SCREEN, vec![0; 64 * 342], Permissions::RW).unwrap();
    let mut p = Process::default();
    ports(&mut m, &mut p);
    let held = p.new_pointer(&mut m, HEAP_SIZE - 48, false).unwrap();
    assert_ne!(held, 0);
    let mut manager = Menus::default();
    manager
        .call(&mut m, &mut p, app.runtime.graphics.as_ref(), Op::Init)
        .unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(m.u32(GuestPtr(0xa1c)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(0xd50)).unwrap(), 0);
    p.dispose_pointer(held);
    let all = p.new_pointer(&mut m, HEAP_SIZE - 32, false).unwrap();
    assert_ne!(all, 0);
}

#[test]
fn menu_handle_adapters_pop_signed_id_and_validate_long_result_before_mutation() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0xff, 0xf9, 0xa5, 0xa5, 0xa5, 0xa5],
        Permissions::RW,
    )
    .unwrap();
    let mut cpu = m68k::CpuCore::default();
    cpu.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa949, &mut cpu, &m).unwrap(),
        (
            Call::Menu(Op::Handle(-7)),
            Reply::StackLong(GuestPtr(0x1002))
        )
    );
    assert_eq!(cpu.dar[15], 0x1002);
    cpu.dar[15] = 0x1001;
    assert!(Abi::trap(0xa949, &mut cpu, &m).is_err());
    assert_eq!(cpu.dar[15], 0x1001);
    let mut cpu = ppc::PpcCpu::default();
    cpu.gpr[3] = 0xa5a5fff9;
    assert_eq!(
        Abi::import(Service::GetMenuHandle, &cpu, &m).unwrap(),
        (Call::Menu(Op::Handle(-7)), Reply::PpcR3)
    );
}
#[test]
#[ignore = "requires a real wgpu adapter for InitMenus"]
fn menu_handle_lookup_follows_live_partitions_ids_and_relocated_handles_without_gpu() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu =
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap();
    let f = support::pef_program(&[0x4e800020], &[b"GetMHandle"], 32);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    ports(&mut app.runtime.memory, &mut app.runtime.process);
    app.runtime.graphics = Some(gpu);
    direct(&mut app, Op::Init).unwrap();
    app.runtime.graphics = None;
    assert_eq!(direct(&mut app, Op::Handle(-7)).unwrap(), 0);
    let list = app.runtime.memory.u32(GuestPtr(0xa1c)).unwrap();
    let mut handles = Vec::new();
    for id in [-7i16, 128, -7] {
        let h = app
            .runtime
            .process
            .new_handle(&mut app.runtime.memory, 16, true)
            .unwrap();
        app.runtime
            .memory
            .write(ptr(&app.runtime.memory, h), &id.to_be_bytes())
            .unwrap();
        handles.push(h);
    }
    let mut bytes = vec![0; 30];
    bytes[..2].copy_from_slice(&12u16.to_be_bytes());
    bytes[6..10].copy_from_slice(&handles[0].to_be_bytes());
    bytes[12..16].copy_from_slice(&handles[1].to_be_bytes());
    bytes[18..20].copy_from_slice(&6u16.to_be_bytes());
    bytes[24..28].copy_from_slice(&handles[2].to_be_bytes());
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, list, 30)
        .unwrap();
    app.runtime
        .memory
        .write(ptr(&app.runtime.memory, list), &bytes)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x49], Permissions::RX)
        .unwrap();
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    for (id, expected) in [(-7i16, handles[2]), (128, handles[1]), (999, 0)] {
        for arch in [Architecture::M68k, Architecture::PowerPc] {
            app.runtime.process.mem_error = -108;
            let stack = STACK_BASE + 0x8000;
            let mut frame = id.to_be_bytes().to_vec();
            frame.extend([0xa5; 8]);
            app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
            app.runtime.cpu = Cpu::new(
                arch,
                if arch == Architecture::M68k {
                    0x50000000
                } else {
                    gate
                },
                stack,
            );
            if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
                c.gpr[3] = 0xa5a50000 | id as u16 as u32;
            }
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            assert_eq!(app.runtime.process.mem_error, -108);
            match &app.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(c.dar[15], stack + 2);
                    assert_eq!(
                        app.runtime.memory.u32(GuestPtr(stack + 2)).unwrap(),
                        expected
                    );
                    assert_eq!(
                        app.runtime.memory.u32(GuestPtr(stack + 6)).unwrap(),
                        0xa5a5a5a5
                    );
                }
                Cpu::PowerPc(c) => assert_eq!(c.gpr[3], expected),
            }
        }
    }
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, handles[2], 32)
        .unwrap();
    app.runtime
        .memory
        .write(ptr(&app.runtime.memory, handles[2]), &9i16.to_be_bytes())
        .unwrap();
    assert_eq!(direct(&mut app, Op::Handle(-7)).unwrap(), handles[0]);
    assert_eq!(direct(&mut app, Op::Handle(9)).unwrap(), handles[2]);
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, list),
        bytes
    );
    let colors = app.runtime.memory.u32(GuestPtr(0xd50)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xd50), &0u32.to_be_bytes())
        .unwrap();
    assert_eq!(direct(&mut app, Op::Handle(9)).unwrap(), handles[2]);
    app.runtime
        .memory
        .write(GuestPtr(0xd50), &colors.to_be_bytes())
        .unwrap();
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, handles[2], 1)
        .unwrap();
    app.runtime.process.mem_error = -108;
    assert!(direct(&mut app, Op::Handle(-7)).is_err());
    assert_eq!(app.runtime.process.mem_error, -108);
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, list, 13)
        .unwrap();
    assert!(direct(&mut app, Op::Handle(-7)).is_err());
}
fn standard_menu(id: i16, title: u8) -> Vec<u8> {
    let mut b = vec![0; 14];
    b[..2].copy_from_slice(&id.to_be_bytes());
    b[10..14].copy_from_slice(&u32::MAX.to_be_bytes());
    b.extend([1, title, 1, 65, 0, 0, 0, 0, 0]);
    b
}
#[test]
fn resource_menu_records_bound_every_field_and_preserve_bar_order() {
    use systemless_future::menus::record::{bar_ids, Record};
    let bytes = standard_menu(-7, 65);
    let r = Record::parse(&bytes).unwrap();
    assert_eq!((r.id, r.title, r.items[0].text), (-7, &b"A"[..], &b"A"[..]));
    for n in 0..bytes.len() {
        assert!(Record::parse(&bytes[..n]).is_err(), "prefix {n}");
    }
    let mut custom = bytes.clone();
    custom[7] = 1;
    assert!(Record::parse(&custom).is_err());
    assert_eq!(bar_ids(&[0, 3, 0, 2, 255, 249, 0, 2]).unwrap(), [2, -7, 2]);
    for bytes in [&[255, 255][..], &[0, 1, 0][..], &[0, 0, 0][..]] {
        assert!(bar_ids(bytes).is_err());
    }
}
#[test]
fn menu_resource_adapters_share_signed_ids_handles_and_void_installation() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![255, 249, 0, 0, 0, 0],
        Permissions::RW,
    )
    .unwrap();
    for (trap, service, op) in [
        (0xa9bf, Service::GetMenu, Op::Load(-7)),
        (0xa9c0, Service::GetNewMBar, Op::Bar(-7)),
    ] {
        let mut c = m68k::CpuCore::default();
        c.dar[15] = 0x1000;
        assert_eq!(
            Abi::trap(trap, &mut c, &m).unwrap(),
            (Call::Menu(op), Reply::StackLong(GuestPtr(0x1002)))
        );
        assert_eq!(c.dar[15], 0x1002);
        c.dar[15] = 0x1001;
        assert!(Abi::trap(trap, &mut c, &m).is_err());
        assert_eq!(c.dar[15], 0x1001);
        let mut c = ppc::PpcCpu::default();
        c.gpr[3] = 0xabcdfff9;
        assert_eq!(
            Abi::import(service, &c, &m).unwrap(),
            (Call::Menu(op), Reply::PpcR3)
        );
    }
    m.write(GuestPtr(0x1000), &0x12345678u32.to_be_bytes())
        .unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa93c, &mut c, &m).unwrap(),
        (Call::Menu(Op::Set(0x12345678)), Reply::None)
    );
    assert_eq!(c.dar[15], 0x1004);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x12345678;
    let (call, reply) = Abi::import(Service::SetMenuBar, &c, &m).unwrap();
    assert_eq!(call, Call::Menu(Op::Set(0x12345678)));
    let mut cpu = Cpu::PowerPc(Box::new(c));
    reply.apply(&mut cpu, &mut m, 99, 0).unwrap();
    let Cpu::PowerPc(c) = cpu else { panic!() };
    assert_eq!(c.gpr[3], 0x12345678);
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn resource_bars_keep_live_menu_identity_draw_titles_and_copy_installed_lists() {
    let font: Vec<u8> = [
        0x9000u16, 65, 66, 4, 0xfffe, 0xffff, 4, 3, 12, 2, 1, 0xffff, 1, 0xe400, 0xba00, 0x4600, 0,
        3, 5, 7, 0x0104, 0xffff, 0x0203, 0xffff,
    ]
    .into_iter()
    .flat_map(u16::to_be_bytes)
    .collect();
    let resource = support::resource(vec![
        (*b"MENU", 7, None, standard_menu(70, 65)),
        (*b"MENU", 8, None, standard_menu(-8, 66)),
        (*b"MBAR", 42, None, vec![0, 2, 0, 7, 0, 8]),
        (*b"MBAR", 43, None, vec![0, 2, 0, 7, 0, 99]),
        (*b"FONT", 12, None, font),
    ]);
    let fixture = support::pef_program(&[0x4e800020], &[], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, Some(&resource)).unwrap();
    ports(&mut app.runtime.memory, &mut app.runtime.process);
    systemless_future::font::initialize(&mut app.runtime.memory, &mut app.runtime.process).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics = Some(
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    direct(&mut app, Op::Init).unwrap();
    let list = app.runtime.memory.u32(GuestPtr(0xa1c)).unwrap();
    let port = app
        .runtime
        .memory
        .read(GuestPtr(0x1100), 108)
        .unwrap()
        .to_vec();
    let bar = direct(&mut app, Op::Bar(42)).unwrap();
    assert_ne!(bar, 0);
    let b = payload(&app.runtime.memory, &mut app.runtime.process, bar);
    assert_eq!(&b[..6], &[0, 12, 0, 44, 0, 42]);
    let first = u32::from_be_bytes(b[6..10].try_into().unwrap());
    assert_eq!(direct(&mut app, Op::Load(7)).unwrap(), first);
    assert_eq!(
        app.runtime
            .process
            .get_resource(&mut app.runtime.memory, *b"MENU", 7)
            .unwrap(),
        first
    );
    let menu = payload(&app.runtime.memory, &mut app.runtime.process, first);
    assert_eq!(&menu[2..6], &[0, 36, 0, 16]);
    assert_eq!(direct(&mut app, Op::Bar(43)).unwrap(), 0);
    assert_eq!(app.runtime.process.res_error, -192);
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, list),
        [0; 12]
    );
    direct(&mut app, Op::Set(bar)).unwrap();
    app.runtime
        .process
        .dispose_handle(&mut app.runtime.memory, bar)
        .unwrap();
    assert_eq!(direct(&mut app, Op::Handle(70)).unwrap(), first);
    direct(&mut app, Op::Draw).unwrap();
    let pixels = app.runtime.memory.read(SCREEN, 64 * 342).unwrap();
    for (y, row) in [0b111, 0b101, 0b010].into_iter().enumerate() {
        for x in 0..3 {
            assert_eq!(
                (pixels[(8 + y) * 64 + (17 + x) / 8] >> (7 - (17 + x) % 8)) & 1,
                (row >> (2 - x)) & 1
            );
        }
    }
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1100), 108).unwrap(),
        port
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa1c)).unwrap(), list);
    app.runtime
        .memory
        .write(GuestPtr(0x1020), &0u32.to_be_bytes())
        .unwrap();
    direct(&mut app, Op::Draw).unwrap();
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x1020)).unwrap(), 0);
    let screen = app.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
    let menu_ptr = ptr(&app.runtime.memory, first);
    app.runtime
        .memory
        .write(GuestPtr(menu_ptr.0 + 7), &[1])
        .unwrap();
    assert!(direct(&mut app, Op::Draw).is_err());
    assert_eq!(app.runtime.memory.read(SCREEN, 64 * 342).unwrap(), screen);
    app.runtime
        .memory
        .write(GuestPtr(menu_ptr.0 + 7), &[0])
        .unwrap();
    direct(&mut app, Op::Clear).unwrap();
    assert_eq!(
        &payload(&app.runtime.memory, &mut app.runtime.process, list)[4..6],
        &42i16.to_be_bytes()
    );
}

#[test]
fn menu_counts_read_uninstalled_live_records_without_graphics_and_preserve_memerr() {
    let fixture = support::pef_program(&[0x4e800020], &[], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let bytes = standard_menu(-7, 65);
    let handle = app
        .runtime
        .process
        .new_handle(&mut app.runtime.memory, bytes.len() as u32, false)
        .unwrap();
    app.runtime
        .memory
        .write(ptr(&app.runtime.memory, handle), &bytes)
        .unwrap();
    app.runtime.process.mem_error = -108;
    assert_eq!(direct(&mut app, Op::Count(handle)).unwrap(), 1);
    assert_eq!(app.runtime.process.mem_error, -108);
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, handle, 64)
        .unwrap();
    let p = ptr(&app.runtime.memory, handle);
    app.runtime
        .memory
        .write(GuestPtr(p.0 + 22), &[1, 66, 0, 0, 0, 0, 0])
        .unwrap();
    let before = app.runtime.memory.read(p, 64).unwrap().to_vec();
    app.runtime.process.mem_error = -117;
    assert_eq!(direct(&mut app, Op::Count(handle)).unwrap(), 2);
    assert_eq!(app.runtime.memory.read(p, 64).unwrap(), before);
    assert_eq!(app.runtime.process.mem_error, -117);
    app.runtime
        .memory
        .write(GuestPtr(p.0 + 22), &[255])
        .unwrap();
    assert!(direct(&mut app, Op::Count(handle)).is_err());
    assert_eq!(app.runtime.process.mem_error, -117);
    assert!(direct(&mut app, Op::Count(0)).is_err());
    assert_eq!(app.runtime.process.mem_error, -117);
}
#[test]
fn menu_count_adapters_validate_word_result_before_popping_handle() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0x10, 0x00, 0x20, 0x00, 0xa5, 0xa5],
        Permissions::RW,
    )
    .unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    let (call, reply) = Abi::trap(0xa950, &mut c, &m).unwrap();
    assert_eq!(call, Call::Menu(Op::Count(0x10002000)));
    assert_eq!(reply, Reply::StackWord(GuestPtr(0x1004)));
    assert_eq!(c.dar[15], 0x1004);
    c.dar[15] = 0x1002;
    assert!(Abi::trap(0xa950, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1002);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x10002000;
    assert_eq!(
        Abi::import(Service::CountMenuItems, &c, &m).unwrap(),
        (call, Reply::PpcR3)
    );
    for name in [b"CountMItems".as_slice(), b"CountMenuItems"] {
        assert_eq!(
            Abi::resolve(b"InterfaceLib", name),
            Some(Service::CountMenuItems)
        );
    }
}

#[test]
fn resource_names_append_as_literal_sorted_items_with_live_resource_ownership() {
    let mut original = standard_menu(70, 65);
    original[10..14].copy_from_slice(&1u32.to_be_bytes());
    let resource = support::resource(vec![
        (*b"MENU", 7, None, original),
        (*b"TEST", 1, Some(b"zeta".to_vec()), vec![1]),
        (*b"TEST", 2, Some(b"alpha".to_vec()), vec![2]),
        (*b"TEST", 3, Some(b".hidden".to_vec()), vec![3]),
        (*b"TEST", 4, Some(b"%private".to_vec()), vec![4]),
        (*b"TEST", 5, Some(b"alpha".to_vec()), vec![5]),
        (*b"TEST", 6, Some(b"A".to_vec()), vec![6]),
        (*b"TEST", 7, Some(b"Beta;^/".to_vec()), vec![7]),
    ]);
    let fixture = support::pef_program(&[0x4e800020], &[], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, Some(&resource)).unwrap();
    let handle = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"MENU", 7)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa5e), &[0, 0x5a])
        .unwrap();
    direct(
        &mut app,
        Op::AppendResources {
            handle,
            kind: *b"TEST",
        },
    )
    .unwrap();
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0xa5e), 2).unwrap(),
        [1, 0x5a]
    );
    let bytes = payload(&app.runtime.memory, &mut app.runtime.process, handle);
    let menu = systemless_future::menus::record::Record::parse(&bytes).unwrap();
    assert_eq!(
        menu.items.iter().map(|i| i.text).collect::<Vec<_>>(),
        [&b"A"[..], b"alpha", b"Beta;^/", b"zeta"]
    );
    assert_eq!(menu.enabled, 29);
    for item in &menu.items[1..] {
        assert_eq!(
            (item.icon, item.command, item.mark, item.style),
            (0, 0, 0, 0)
        );
    }
    assert_eq!(direct(&mut app, Op::Count(handle)).unwrap(), 4);
    assert_eq!(
        app.runtime
            .process
            .get_resource(&mut app.runtime.memory, *b"MENU", 7)
            .unwrap(),
        handle
    );
    let h = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"TEST", 2)
        .unwrap();
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, h),
        [2]
    );
    direct(
        &mut app,
        Op::AppendResources {
            handle,
            kind: *b"TEST",
        },
    )
    .unwrap();
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, handle),
        bytes
    );
    direct(
        &mut app,
        Op::AppendResources {
            handle,
            kind: *b"NONE",
        },
    )
    .unwrap();
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, handle),
        bytes
    );
}
#[test]
fn resource_menu_adapters_unpack_type_before_handle_and_keep_void_ppc_results() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![b'T', b'E', b'S', b'T', 0x10, 0, 0x20, 0],
        Permissions::RW,
    )
    .unwrap();
    let expected = Call::Menu(Op::AppendResources {
        handle: 0x10002000,
        kind: *b"TEST",
    });
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa94d, &mut c, &m).unwrap(),
        (expected, Reply::None)
    );
    assert_eq!(c.dar[15], 0x1008);
    c.dar[15] = 0x1002;
    assert!(Abi::trap(0xa94d, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1002);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x10002000;
    c.gpr[4] = u32::from_be_bytes(*b"TEST");
    let (call, reply) = Abi::import(Service::AppendResMenu, &c, &m).unwrap();
    assert_eq!(call, expected);
    let mut cpu = Cpu::PowerPc(Box::new(c));
    reply.apply(&mut cpu, &mut m, 99, 0).unwrap();
    let Cpu::PowerPc(c) = cpu else { panic!() };
    assert_eq!(c.gpr[3], 0x10002000);
    for name in [b"AddResMenu".as_slice(), b"AppendResMenu"] {
        assert_eq!(
            Abi::resolve(b"InterfaceLib", name),
            Some(Service::AppendResMenu)
        );
    }
}

#[test]
fn item_marks_change_one_live_byte_without_allocating_or_touching_flags() {
    let fixture = support::pef_program(&[0x4e800020], &[], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let bytes = standard_menu(7, 65);
    let handle = app
        .runtime
        .process
        .new_handle(&mut app.runtime.memory, bytes.len() as u32, false)
        .unwrap();
    let p = ptr(&app.runtime.memory, handle);
    app.runtime.memory.write(p, &bytes).unwrap();
    app.runtime.process.mem_error = -108;
    for item in [-1, 0, 2, 32767] {
        direct(
            &mut app,
            Op::Mark {
                handle,
                item,
                mark: 0x12,
            },
        )
        .unwrap();
        assert_eq!(app.runtime.memory.read(p, bytes.len()).unwrap(), bytes);
    }
    direct(
        &mut app,
        Op::Mark {
            handle,
            item: 1,
            mark: 0x34,
        },
    )
    .unwrap();
    let mut expected = bytes;
    expected[20] = 0x34;
    assert_eq!(
        app.runtime.memory.read(p, expected.len()).unwrap(),
        expected
    );
    assert_eq!(ptr(&app.runtime.memory, handle), p);
    assert_eq!(app.runtime.process.mem_error, -108);
    direct(
        &mut app,
        Op::Mark {
            handle,
            item: 1,
            mark: 0,
        },
    )
    .unwrap();
    assert_eq!(app.runtime.memory.read(GuestPtr(p.0 + 20), 1).unwrap(), [0]);
    direct(
        &mut app,
        Op::Command {
            handle,
            item: 1,
            command: 0x50,
        },
    )
    .unwrap();
    assert_eq!(
        app.runtime.memory.read(GuestPtr(p.0 + 19), 2).unwrap(),
        [0x50, 0]
    );
    assert_eq!(app.runtime.process.mem_error, -108);
}
#[test]
fn item_mark_adapters_distinguish_short_characters_and_boolean_stack_bytes() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0, 0xff, 0, 1, 0x10, 0, 0x20, 0],
        Permissions::RW,
    )
    .unwrap();
    for (trap, mark) in [(0xa944, 0xff), (0xa945, 0)] {
        let mut c = m68k::CpuCore::default();
        c.dar[15] = 0x1000;
        assert_eq!(
            Abi::trap(trap, &mut c, &m).unwrap(),
            (
                Call::Menu(Op::Mark {
                    handle: 0x10002000,
                    item: 1,
                    mark
                }),
                Reply::None
            )
        );
        assert_eq!(c.dar[15], 0x1008);
    }
    m.write(GuestPtr(0x1000), &[1, 0]).unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa945, &mut c, &m).unwrap().0,
        Call::Menu(Op::Mark {
            handle: 0x10002000,
            item: 1,
            mark: 0x12
        })
    );
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x10002000;
    c.gpr[4] = 0xabcd0001;
    c.gpr[5] = 0xdead0012;
    for service in [Service::SetItemMark, Service::CheckItem] {
        let (call, reply) = Abi::import(service, &c, &m).unwrap();
        assert_eq!(
            call,
            Call::Menu(Op::Mark {
                handle: 0x10002000,
                item: 1,
                mark: 0x12
            })
        );
        let mut cpu = Cpu::PowerPc(Box::new(c.clone()));
        reply.apply(&mut cpu, &mut m, 99, 0).unwrap();
        let Cpu::PowerPc(c) = cpu else { panic!() };
        assert_eq!(c.gpr[3], 0x10002000);
    }
    m.write(GuestPtr(0x1000), &[0xab, 0x50]).unwrap();
    let mut native = m68k::CpuCore::default();
    native.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa84f, &mut native, &m).unwrap(),
        (
            Call::Menu(Op::Command {
                handle: 0x10002000,
                item: 1,
                command: 0x50
            }),
            Reply::None
        )
    );
    c.gpr[5] = 0xdead0050;
    assert_eq!(
        Abi::import(Service::SetItemCommand, &c, &m).unwrap().0,
        Call::Menu(Op::Command {
            handle: 0x10002000,
            item: 1,
            command: 0x50
        })
    );
}

#[test]
fn menu_text_replacement_preserves_attributes_and_failed_growth_preserves_record() {
    let fixture = support::pef_program(&[0x4e800020], &[], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let mut bytes = standard_menu(7, 65);
    bytes[18..22].copy_from_slice(&[3, 65, 0x12, 1]);
    let h = app
        .runtime
        .process
        .new_handle(&mut app.runtime.memory, bytes.len() as u32, false)
        .unwrap();
    let before = ptr(&app.runtime.memory, h);
    app.runtime.memory.write(before, &bytes).unwrap();
    let source = GuestPtr(
        app.runtime
            .process
            .new_pointer(&mut app.runtime.memory, 256, false)
            .unwrap(),
    );
    app.runtime
        .memory
        .write(source, b"\x0cLonger title")
        .unwrap();
    app.runtime
        .process
        .set_handle_lock(&app.runtime.memory, h, true)
        .unwrap();
    direct(
        &mut app,
        Op::Text {
            handle: h,
            item: 1,
            source,
        },
    )
    .unwrap();
    assert_eq!(app.runtime.process.mem_error, -108);
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, h),
        bytes
    );
    assert_eq!(ptr(&app.runtime.memory, h), before);
    app.runtime
        .process
        .set_handle_lock(&app.runtime.memory, h, false)
        .unwrap();
    direct(
        &mut app,
        Op::Text {
            handle: h,
            item: 1,
            source,
        },
    )
    .unwrap();
    assert_eq!(app.runtime.process.mem_error, 0);
    let current = payload(&app.runtime.memory, &mut app.runtime.process, h);
    let r = systemless_future::menus::record::Record::parse(&current).unwrap();
    assert_eq!(r.items[0].text, b"Longer title");
    assert_eq!(
        (
            r.items[0].icon,
            r.items[0].command,
            r.items[0].mark,
            r.items[0].style
        ),
        (3, 65, 0x12, 1)
    );
    assert_eq!(&current[..16], &bytes[..16]);
    assert!(direct(
        &mut app,
        Op::Text {
            handle: h,
            item: 1,
            source: GuestPtr(0xdead0000)
        }
    )
    .is_err());
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, h),
        current
    );
    assert!(direct(
        &mut app,
        Op::Text {
            handle: h,
            item: 0,
            source: GuestPtr(0xdead0000)
        }
    )
    .is_ok());
    app.runtime.memory.write(source, b"\x01B").unwrap();
    direct(
        &mut app,
        Op::Text {
            handle: h,
            item: 1,
            source,
        },
    )
    .unwrap();
    assert_eq!(direct(&mut app, Op::Count(h)).unwrap(), 1);
}
#[test]
fn menu_text_adapters_unpack_pascal_pointer_and_signed_item_without_return_value() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0, 0, 0x30, 0, 255, 249, 0x10, 0, 0x20, 0],
        Permissions::RW,
    )
    .unwrap();
    let expected = Call::Menu(Op::Text {
        handle: 0x10002000,
        item: -7,
        source: GuestPtr(0x3000),
    });
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa947, &mut c, &m).unwrap(),
        (expected, Reply::None)
    );
    assert_eq!(c.dar[15], 0x100a);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x10002000;
    c.gpr[4] = 0xabcdfff9;
    c.gpr[5] = 0x3000;
    let (call, reply) = Abi::import(Service::SetMenuItemText, &c, &m).unwrap();
    assert_eq!(call, expected);
    let mut cpu = Cpu::PowerPc(Box::new(c));
    reply.apply(&mut cpu, &mut m, 99, 0).unwrap();
    let Cpu::PowerPc(c) = cpu else { panic!() };
    assert_eq!(c.gpr[3], 0x10002000);
    for name in [b"SetItem".as_slice(), b"SetMenuItemText"] {
        assert_eq!(
            Abi::resolve(b"InterfaceLib", name),
            Some(Service::SetMenuItemText)
        );
    }
}

#[test]
fn new_menus_are_owned_empty_records_without_graphics_resources_or_installation() {
    let fixture = support::pef_program(&[0x4e800020], &[], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let source = GuestPtr(
        app.runtime
            .process
            .new_pointer(&mut app.runtime.memory, 256, false)
            .unwrap(),
    );
    app.runtime.memory.write(source, b"\x04Menu").unwrap();
    app.runtime.process.res_error = -192;
    let h = direct(
        &mut app,
        Op::New {
            id: -17,
            title: source,
        },
    )
    .unwrap();
    assert_ne!(h, 0);
    assert_eq!(app.runtime.process.res_error, -192);
    let bytes = payload(&app.runtime.memory, &mut app.runtime.process, h);
    let menu = systemless_future::menus::record::Record::parse(&bytes).unwrap();
    assert_eq!(
        (menu.id, menu.title, menu.enabled),
        (-17, &b"Menu"[..], u32::MAX)
    );
    assert!(menu.items.is_empty());
    assert_eq!(&bytes[2..10], &[0; 8]);
    assert_eq!(direct(&mut app, Op::Count(h)).unwrap(), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa1c)).unwrap(), 0);
    let empty = direct(
        &mut app,
        Op::New {
            id: 0,
            title: GuestPtr(0),
        },
    )
    .unwrap();
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, empty).len(),
        16
    );
    assert!(direct(
        &mut app,
        Op::New {
            id: 1,
            title: GuestPtr(0xdead0000)
        }
    )
    .is_err());
    app.runtime
        .process
        .dispose_handle(&mut app.runtime.memory, h)
        .unwrap();
    app.runtime
        .process
        .dispose_handle(&mut app.runtime.memory, empty)
        .unwrap();
}
#[test]
fn new_menu_adapters_validate_long_results_and_unpack_signed_id_before_allocation() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0, 0, 0x30, 0, 255, 239, 0xa5, 0xa5, 0xa5, 0xa5],
        Permissions::RW,
    )
    .unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    let expected = Call::Menu(Op::New {
        id: -17,
        title: GuestPtr(0x3000),
    });
    assert_eq!(
        Abi::trap(0xa931, &mut c, &m).unwrap(),
        (expected, Reply::StackLong(GuestPtr(0x1006)))
    );
    assert_eq!(c.dar[15], 0x1006);
    c.dar[15] = 0x1002;
    assert!(Abi::trap(0xa931, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1002);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0xabcdffef;
    c.gpr[4] = 0x3000;
    assert_eq!(
        Abi::import(Service::NewMenu, &c, &m).unwrap(),
        (expected, Reply::PpcR3)
    );
}

#[test]
fn append_menu_decodes_items_attributes_styles_and_enable_bits_atomically() {
    let fixture = support::pef_program(&[0x4e800020], &[], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let h = direct(
        &mut app,
        Op::New {
            id: 12,
            title: GuestPtr(0),
        },
    )
    .unwrap();
    let source = GuestPtr(
        app.runtime
            .process
            .new_pointer(&mut app.runtime.memory, 256, false)
            .unwrap(),
    );
    let spec = b"One^2!*/O<BI;(Disabled<US\r-";
    let mut p = vec![spec.len() as u8];
    p.extend(spec);
    app.runtime.memory.write(source, &p).unwrap();
    direct(&mut app, Op::Append { handle: h, source }).unwrap();
    let bytes = payload(&app.runtime.memory, &mut app.runtime.process, h);
    let menu = systemless_future::menus::record::Record::parse(&bytes).unwrap();
    assert_eq!(
        menu.items.iter().map(|i| i.text).collect::<Vec<_>>(),
        [&b"One"[..], b"Disabled", b"-"]
    );
    assert_eq!(
        (
            menu.items[0].icon,
            menu.items[0].command,
            menu.items[0].mark,
            menu.items[0].style
        ),
        (2, b'O', b'*', 3)
    );
    assert_eq!(menu.items[1].style, 20);
    assert_eq!(menu.enabled & 15, 11);
    assert!(direct(
        &mut app,
        Op::Append {
            handle: h,
            source: GuestPtr(0xdead0000)
        }
    )
    .is_err());
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, h),
        bytes
    );
    app.runtime.memory.write(source, b"\x02!*").unwrap();
    assert!(direct(&mut app, Op::Append { handle: h, source }).is_err());
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, h),
        bytes
    );
    app.runtime.memory.write(source, b"\x03;;;").unwrap();
    direct(&mut app, Op::Append { handle: h, source }).unwrap();
    assert_eq!(direct(&mut app, Op::Count(h)).unwrap(), 3);
}
#[test]
fn append_menu_adapters_preserve_void_results_and_validate_complete_argument_storage() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0, 0, 0x30, 0, 0x10, 0, 0x20, 0],
        Permissions::RW,
    )
    .unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    let expected = Call::Menu(Op::Append {
        handle: 0x10002000,
        source: GuestPtr(0x3000),
    });
    assert_eq!(
        Abi::trap(0xa933, &mut c, &m).unwrap(),
        (expected, Reply::None)
    );
    assert_eq!(c.dar[15], 0x1008);
    c.dar[15] = 0x1002;
    assert!(Abi::trap(0xa933, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1002);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x10002000;
    c.gpr[4] = 0x3000;
    let (call, reply) = Abi::import(Service::AppendMenu, &c, &m).unwrap();
    assert_eq!(call, expected);
    let mut cpu = Cpu::PowerPc(Box::new(c));
    reply.apply(&mut cpu, &mut m, 99, 0).unwrap();
    let Cpu::PowerPc(c) = cpu else { panic!() };
    assert_eq!(c.gpr[3], 0x10002000);
}

#[test]
fn menu_text_reads_live_uninstalled_records_preserve_guards_memerr_and_overlap() {
    let fixture = support::pef_program(&[0x4e800020], &[], 32);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let source = GuestPtr(
        app.runtime
            .process
            .new_pointer(&mut app.runtime.memory, 256, false)
            .unwrap(),
    );
    app.runtime.memory.write(source, b"\x03One").unwrap();
    let h = direct(
        &mut app,
        Op::New {
            id: 12,
            title: GuestPtr(0),
        },
    )
    .unwrap();
    direct(&mut app, Op::Append { handle: h, source }).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa5; 256], Permissions::RW)
        .unwrap();
    let destination = GuestPtr(0x50000000);
    let original = payload(&app.runtime.memory, &mut app.runtime.process, h);
    app.runtime.process.mem_error = -117;
    direct(
        &mut app,
        Op::ReadText {
            handle: h,
            item: 1,
            destination,
        },
    )
    .unwrap();
    assert_eq!(
        app.runtime.memory.read(destination, 6).unwrap(),
        [3, b'O', b'n', b'e', 0xa5, 0xa5]
    );
    assert_eq!(app.runtime.process.mem_error, -117);
    assert_eq!(
        payload(&app.runtime.memory, &mut app.runtime.process, h),
        original
    );
    app.runtime.process.mem_error = -117;
    for item in [-1, 0, 2, 32767] {
        app.runtime.memory.write(destination, &[0xa5; 6]).unwrap();
        direct(
            &mut app,
            Op::ReadText {
                handle: h,
                item,
                destination,
            },
        )
        .unwrap();
        assert_eq!(
            app.runtime.memory.read(destination, 6).unwrap(),
            [0, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5]
        );
    }
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0x5a; 2], Permissions::RW)
        .unwrap();
    assert!(direct(
        &mut app,
        Op::ReadText {
            handle: h,
            item: 1,
            destination: GuestPtr(0x51000000)
        }
    )
    .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x51000000), 2).unwrap(),
        [0x5a; 2]
    );
    assert_eq!(app.runtime.process.mem_error, -117);
    app.runtime
        .memory
        .map(GuestPtr(0x52000000), vec![0x5a; 256], Permissions::RX)
        .unwrap();
    assert!(direct(
        &mut app,
        Op::ReadText {
            handle: h,
            item: 1,
            destination: GuestPtr(0x52000000)
        }
    )
    .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x52000000), 4).unwrap(),
        [0x5a; 4]
    );
    direct(
        &mut app,
        Op::ReadText {
            handle: 0,
            item: 1,
            destination: GuestPtr(0),
        },
    )
    .unwrap();
    assert_eq!(app.runtime.process.mem_error, -117);
    let alias = GuestPtr(ptr(&app.runtime.memory, h).0 + 17);
    direct(
        &mut app,
        Op::ReadText {
            handle: h,
            item: 1,
            destination: alias,
        },
    )
    .unwrap();
    assert_eq!(app.runtime.memory.read(alias, 4).unwrap(), b"\x03One");
}
#[test]
fn menu_text_read_adapters_keep_void_results_and_unpack_signed_items() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0, 0, 0x30, 0, 255, 249, 0x10, 0, 0x20, 0],
        Permissions::RW,
    )
    .unwrap();
    let expected = Call::Menu(Op::ReadText {
        handle: 0x10002000,
        item: -7,
        destination: GuestPtr(0x3000),
    });
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa946, &mut c, &m).unwrap(),
        (expected, Reply::None)
    );
    assert_eq!(c.dar[15], 0x100a);
    c.dar[15] = 0x1002;
    assert!(Abi::trap(0xa946, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1002);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x10002000;
    c.gpr[4] = 0xabcdfff9;
    c.gpr[5] = 0x3000;
    let (call, reply) = Abi::import(Service::GetMenuItemText, &c, &m).unwrap();
    assert_eq!(call, expected);
    let mut cpu = Cpu::PowerPc(Box::new(c));
    reply.apply(&mut cpu, &mut m, 99, 0).unwrap();
    let Cpu::PowerPc(c) = cpu else { panic!() };
    assert_eq!(c.gpr[3], 0x10002000);
    for name in [b"GetItem".as_slice(), b"GetMenuItemText"] {
        assert_eq!(
            Abi::resolve(b"InterfaceLib", name),
            Some(Service::GetMenuItemText)
        );
    }
}
