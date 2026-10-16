mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    menus::Op,
    quickdraw::{Ports, SCREEN},
    runtime::Stop,
};
fn call(a: &mut LoadedApplication, op: Op) -> systemless_future::Result<u32> {
    let mut menus = std::mem::take(&mut a.runtime.process.menus);
    let r = menus.call(
        &mut a.runtime.memory,
        &mut a.runtime.process,
        a.runtime.graphics.as_ref(),
        op,
    );
    a.runtime.process.menus = menus;
    r
}
fn app(gpu: Graphics) -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"InsertMenu", b"DeleteMenu"], 64);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa9, 0x35, 0xa9, 0x36],
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .write(
            GuestPtr(0x1000),
            &[0x74, 0x10, 0, 0, 0, 64, 0, 0, 0, 0, 1, 0x56, 2, 0],
        )
        .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1020),
        screen_bitmap: GuestPtr(0x1000),
    };
    ports
        .open(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            GuestPtr(0x1100),
        )
        .unwrap();
    a.runtime.process.ports = Some(ports);
    systemless_future::font::initialize(&mut a.runtime.memory, &mut a.runtime.process).unwrap();
    a.runtime.graphics = Some(gpu);
    call(&mut a, Op::Init).unwrap();
    a
}
fn gpu() -> Graphics {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap()
}
fn bytes(a: &mut LoadedApplication, h: u32) -> Vec<u8> {
    let n = a.runtime.process.handle_size(&a.runtime.memory, h).unwrap();
    let p = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime
        .memory
        .read(GuestPtr(p), n as usize)
        .unwrap()
        .to_vec()
}
fn menu(a: &mut LoadedApplication, id: i16, title: u8) -> u32 {
    let mut b = vec![0; 14];
    b[..2].copy_from_slice(&id.to_be_bytes());
    b[10..14].copy_from_slice(&u32::MAX.to_be_bytes());
    b.extend([1, title, 0]);
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, b.len() as u32, false)
        .unwrap();
    let p = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(p), &b).unwrap();
    h
}
fn invoke(a: &mut LoadedApplication, arch: Architecture, insert: bool, h: u32, value: i16) {
    let stack = STACK_BASE + 0x8000;
    let index = u32::from(!insert);
    let mut frame = value.to_be_bytes().to_vec();
    if insert {
        frame.extend(h.to_be_bytes());
    }
    let n = frame.len() as u32;
    frame.extend([0xa5; 4]);
    a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(
        arch,
        if arch == Architecture::PowerPc {
            pc
        } else {
            0x50000000 + index * 2
        },
        stack,
    );
    let arg = if insert { h } else { value as i32 as u32 };
    if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
        c.gpr[3] = arg;
        c.gpr[4] = value as i32 as u32;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    match &a.runtime.cpu {
        Cpu::PowerPc(c) => assert_eq!(c.gpr[3], arg),
        Cpu::M68k(c) => assert_eq!(c.dar[15], stack + n),
    }
    assert_eq!(
        a.runtime.memory.u32(GuestPtr(stack + n)).unwrap(),
        0xa5a5a5a5
    );
}
fn handles(list: &[u8], hierarchical: bool) -> Vec<u32> {
    let n = u16::from_be_bytes(list[..2].try_into().unwrap()) as usize;
    let part = if hierarchical {
        &list[12 + n..]
    } else {
        &list[6..6 + n]
    };
    part.chunks_exact(6)
        .map(|e| u32::from_be_bytes(e[..4].try_into().unwrap()))
        .collect()
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_cpus_edit_live_partitions_remove_colors_and_keep_records_and_pixels() {
    let mut graphics = Some(gpu());
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        let mut a = app(graphics.take().unwrap());
        let list = a.runtime.memory.u32(GuestPtr(0xa1c)).unwrap();
        let colors = a.runtime.memory.u32(GuestPtr(0xd50)).unwrap();
        let first = menu(&mut a, -7, b'A');
        let second = menu(&mut a, 128, b'W');
        let third = menu(&mut a, 42, b'i');
        let sub = menu(&mut a, -7, b'S');
        let screen = a.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
        invoke(&mut a, arch, true, first, 0);
        invoke(&mut a, arch, true, second, 0);
        invoke(&mut a, arch, true, third, -7);
        invoke(&mut a, arch, true, sub, -1);
        let installed = bytes(&mut a, list);
        assert_eq!(handles(&installed, false), [third, first, second]);
        assert_eq!(handles(&installed, true), [sub]);
        assert_eq!(
            i16::from_be_bytes(installed[10..12].try_into().unwrap()),
            11
        );
        assert!(i16::from_be_bytes(installed[2..4].try_into().unwrap()) > 11);
        let free = a.runtime.process.free_memory();
        a.runtime.process.mem_error = -77;
        invoke(&mut a, arch, true, first, -1);
        assert_eq!(a.runtime.process.mem_error, -77);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(bytes(&mut a, list), installed);
        let mut entries = Vec::new();
        for id in [-7i16, -7, 128, 999] {
            let mut e = vec![0; 30];
            e[..2].copy_from_slice(&id.to_be_bytes());
            entries.extend(e);
        }
        a.runtime
            .process
            .set_handle_size(&mut a.runtime.memory, colors, entries.len() as u32)
            .unwrap();
        let p = a.runtime.memory.u32(GuestPtr(colors)).unwrap();
        a.runtime.memory.write(GuestPtr(p), &entries).unwrap();
        let source = bytes(&mut a, sub);
        invoke(&mut a, arch, false, 0, -7);
        let current = bytes(&mut a, list);
        assert_eq!(handles(&current, true), Vec::<u32>::new());
        assert_eq!(handles(&current, false), [third, first, second]);
        assert_eq!(bytes(&mut a, sub), source);
        assert_eq!(bytes(&mut a, colors), entries[60..]);
        let unchanged = bytes(&mut a, list);
        let color_before = bytes(&mut a, colors);
        invoke(&mut a, arch, false, 0, 999);
        assert_eq!(bytes(&mut a, list), unchanged);
        assert_eq!(bytes(&mut a, colors), color_before);
        invoke(&mut a, arch, false, 0, -7);
        assert_eq!(handles(&bytes(&mut a, list), false), [third, second]);
        invoke(&mut a, arch, false, 0, 42);
        invoke(&mut a, arch, false, 0, 128);
        assert_eq!(bytes(&mut a, list), vec![0; 12]);
        for h in [first, second, third, sub] {
            assert_ne!(a.runtime.memory.u32(GuestPtr(h)).unwrap(), 0);
        }
        assert_eq!(a.runtime.memory.read(SCREEN, 64 * 342).unwrap(), screen);
        assert_eq!(a.runtime.memory.u32(GuestPtr(0xa1c)).unwrap(), list);
        assert_eq!(a.runtime.memory.u32(GuestPtr(0xd50)).unwrap(), colors);
        graphics = a.runtime.graphics.take();
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn locked_growth_and_later_corruption_leave_all_published_state_unchanged() {
    let mut a = app(gpu());
    let list = a.runtime.memory.u32(GuestPtr(0xa1c)).unwrap();
    let colors = a.runtime.memory.u32(GuestPtr(0xd50)).unwrap();
    let h = menu(&mut a, 128, b'A');
    let old = bytes(&mut a, list);
    let original = bytes(&mut a, h);
    a.runtime
        .process
        .set_handle_lock(&a.runtime.memory, list, true)
        .unwrap();
    let free = a.runtime.process.free_memory();
    call(
        &mut a,
        Op::Insert {
            handle: h,
            before: 0,
        },
    )
    .unwrap();
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(bytes(&mut a, list), old);
    assert_eq!(bytes(&mut a, h), original);
    a.runtime
        .process
        .set_handle_lock(&a.runtime.memory, list, false)
        .unwrap();
    call(
        &mut a,
        Op::Insert {
            handle: h,
            before: 0,
        },
    )
    .unwrap();
    let second = menu(&mut a, 42, b'B');
    call(
        &mut a,
        Op::Insert {
            handle: second,
            before: 0,
        },
    )
    .unwrap();
    let before = bytes(&mut a, list);
    let ct = bytes(&mut a, colors);
    let p = a.runtime.memory.u32(GuestPtr(second)).unwrap();
    a.runtime.memory.write(GuestPtr(p + 6), &[1]).unwrap();
    let free = a.runtime.process.free_memory();
    assert!(call(&mut a, Op::Delete(128)).is_err());
    assert_eq!(bytes(&mut a, list), before);
    assert_eq!(bytes(&mut a, colors), ct);
    assert_eq!(a.runtime.process.free_memory(), free);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn full_submenu_partition_is_a_noop_without_touching_the_candidate() {
    let mut a = app(gpu());
    let list = a.runtime.memory.u32(GuestPtr(0xa1c)).unwrap();
    let mut contents = vec![0; 12];
    contents[6..8].copy_from_slice(&6144u16.to_be_bytes());
    for id in 1..=1024 {
        let h = menu(&mut a, id, b'A');
        contents.extend(h.to_be_bytes());
        contents.extend([0; 2]);
    }
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, list, contents.len() as u32)
        .unwrap();
    let p = a.runtime.memory.u32(GuestPtr(list)).unwrap();
    a.runtime.memory.write(GuestPtr(p), &contents).unwrap();
    let candidate = menu(&mut a, 2000, b'B');
    let original = bytes(&mut a, candidate);
    let free = a.runtime.process.free_memory();
    a.runtime.process.mem_error = -77;
    call(
        &mut a,
        Op::Insert {
            handle: candidate,
            before: -1,
        },
    )
    .unwrap();
    assert_eq!(a.runtime.process.mem_error, -77);
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(bytes(&mut a, list), contents);
    assert_eq!(bytes(&mut a, candidate), original);
}
#[test]
fn list_adapters_validate_complete_frames_and_decode_signed_word_parameters() {
    use systemless_future::{
        abi::{Abi, Reply, Service},
        cpu::Call,
        memory::Memory,
    };
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0xff, 0xf9, 0x12, 0x34, 0x56, 0x78],
        Permissions::RW,
    )
    .unwrap();
    let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
        panic!()
    };
    assert_eq!(
        Abi::trap(0xa935, &mut c, &m).unwrap(),
        (
            Call::Menu(Op::Insert {
                handle: 0x12345678,
                before: -7
            }),
            Reply::None
        )
    );
    assert_eq!(c.dar[15], 0x1006);
    c.dar[15] = 0x1002;
    assert!(Abi::trap(0xa935, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1002);
    c.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa936, &mut c, &m).unwrap(),
        (Call::Menu(Op::Delete(-7)), Reply::None)
    );
    assert_eq!(c.dar[15], 0x1002);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x12345678;
    c.gpr[4] = 0xabcdfff9;
    assert_eq!(
        Abi::import(Service::InsertMenu, &c, &m).unwrap(),
        (
            Call::Menu(Op::Insert {
                handle: 0x12345678,
                before: -7
            }),
            Reply::None
        )
    );
    c.gpr[3] = 0xabcdfff9;
    assert_eq!(
        Abi::import(Service::DeleteMenu, &c, &m).unwrap(),
        (Call::Menu(Op::Delete(-7)), Reply::None)
    );
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn saved_tracking_rejection_precedes_font_preparation_or_published_changes() {
    let mut a = app(gpu());
    let list = a.runtime.memory.u32(GuestPtr(0xa1c)).unwrap();
    let h = menu(&mut a, 128, b'A');
    let original = bytes(&mut a, h);
    let p = a.runtime.memory.u32(GuestPtr(list)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(p + 8), &0x12345678u32.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xba8), &9u16.to_be_bytes())
        .unwrap();
    let saved = bytes(&mut a, list);
    let free = a.runtime.process.free_memory();
    assert!(call(
        &mut a,
        Op::Insert {
            handle: h,
            before: 0
        }
    )
    .is_err());
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(bytes(&mut a, list), saved);
    assert_eq!(bytes(&mut a, h), original);
}
