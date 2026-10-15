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
