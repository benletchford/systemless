mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
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
fn menu(id: i16, title: &[u8], flags: u32) -> Vec<u8> {
    let mut b = vec![0; 14];
    b[..2].copy_from_slice(&id.to_be_bytes());
    b[10..14].copy_from_slice(&flags.to_be_bytes());
    b.push(title.len() as u8);
    b.extend(title);
    for text in [&b"One"[..], &b"Two"[..], &b"-"[..], &b"Four"[..]] {
        b.push(text.len() as u8);
        b.extend(text);
        b.extend([0; 4]);
    }
    b.push(0);
    b
}
fn import(a: &LoadedApplication, index: u32) -> u32 {
    let v = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    a.runtime.memory.u32(GuestPtr(v)).unwrap()
}
fn invoke(a: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, import(a, index), STACK);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for (i, v) in args.iter().enumerate() {
        c.gpr[3 + i] = *v;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn setup(instance: &wgpu::Instance) -> (LoadedApplication, Pointer) {
    let r = support::resource(vec![
        (*b"MENU", 7, None, menu(70, b"File", !4)),
        (*b"MENU", 8, None, menu(80, b"Edit", u32::MAX)),
        (*b"MBAR", 42, None, vec![0, 2, 0, 7, 0, 8]),
    ]);
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitFonts",
            b"InitWindows",
            b"InitMenus",
            b"GetNewMBar",
            b"SetMenuBar",
            b"MenuSelect",
            b"HiliteMenu",
            b"MenuChoice",
            b"DrawMenuBar",
        ],
        512,
    );
    let mut a = LoadedApplication::pef(&f.bytes, Some(&r)).unwrap();
    let slot = a.info.toc_or_a5 + 64 + 202;
    a.runtime.graphics = Some(
        pollster::block_on(Graphics::new(instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut a, 0, &[slot]);
    invoke(&mut a, 1, &[]);
    invoke(&mut a, 2, &[]);
    invoke(&mut a, 3, &[]);
    let bar = invoke(&mut a, 4, &[42]);
    invoke(&mut a, 5, &[bar]);
    invoke(&mut a, 9, &[]);
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa9, 0x3d, 0x4e, 0x71, 0xa9, 0x38, 0xaa, 0x66],
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0x144), &u16::MAX.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x52000000),
            0x60000000u32.to_be_bytes().to_vec(),
            Permissions::RX,
        )
        .unwrap();
    (a, Pointer::default())
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
    point: [i16; 2],
) -> systemless_future::Result<Stop> {
    match arch {
        Architecture::M68k => {
            let mut b = point
                .into_iter()
                .flat_map(i16::to_be_bytes)
                .collect::<Vec<_>>();
            b.extend([0xa5; 8]);
            a.runtime.memory.write(GuestPtr(STACK), &b).unwrap();
            a.runtime.cpu = Cpu::new(arch, 0x50000000, STACK);
            if let Cpu::M68k(c) = &mut a.runtime.cpu {
                c.dar[0] = 0x12345678;
            }
        }
        Architecture::PowerPc => {
            a.runtime.cpu = Cpu::new(arch, import(a, 6), STACK);
            if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
                c.gpr[3] = (u32::from(point[0] as u16) << 16) | u32::from(point[1] as u16);
                c.gpr[4] = 0xaabbccdd;
                c.lr = 0x52000000;
            }
        }
    }
    a.runtime.advance(1)
}
fn screen(a: &LoadedApplication) -> Vec<u8> {
    a.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec()
}
fn result(a: &LoadedApplication, arch: Architecture) -> u32 {
    match arch {
        Architecture::M68k => {
            let Cpu::M68k(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.dar[15], STACK + 4);
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(STACK + 8), 4).unwrap(),
                [0xa5; 4]
            );
            a.runtime.memory.u32(GuestPtr(STACK + 4)).unwrap()
        }
        Architecture::PowerPc => {
            let Cpu::PowerPc(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.gpr[4], 0xaabbccdd);
            c.gpr[3]
        }
    }
}
fn hilite(a: &mut LoadedApplication, arch: Architecture, id: i16) {
    if arch == Architecture::PowerPc {
        invoke(a, 7, &[id as i32 as u32]);
    } else {
        a.runtime
            .memory
            .write(GuestPtr(STACK), &id.to_be_bytes())
            .unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x50000004, STACK);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK + 2);
    }
}
#[test]
fn menu_tracking_abi_checks_results_before_popping_and_decodes_point_order() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0xff, 0xfe, 0, 35, 0xa5, 0xa5, 0xa5, 0xa5],
        Permissions::RW,
    )
    .unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa93d, &mut c, &m).unwrap(),
        (
            Call::MenuSelect([-2, 35]),
            Reply::StackLong(GuestPtr(0x1004))
        )
    );
    assert_eq!(c.dar[15], 0x1004);
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 8], Permissions::RX)
        .unwrap();
    for trap in [0xa93d, 0xaa66] {
        let mut c = m68k::CpuCore::default();
        c.dar[15] = 0x1000;
        assert!(Abi::trap(trap, &mut c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0xfffe0023;
    assert_eq!(
        Abi::import(Service::MenuSelect, &c, &m).unwrap().0,
        Call::MenuSelect([-2, 35])
    );
    assert_eq!(
        Abi::import(Service::HiliteMenu, &c, &m).unwrap().1,
        Reply::None
    );
    for op in [0x138, 0x13d, 0x266] {
        assert_ne!(
            systemless_future::traps::address(op, true),
            systemless_future::traps::UNIMPLEMENTED
        );
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn menus_wait_without_spinning_track_real_motion_restore_pixels_and_return_on_both_cpus() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, mut pointer) = setup(&instance);
        // A 511-pixel logical screen in 64-byte rows leaves one padding bit.
        // Every GPU menu operation must preserve it, including restoration.
        let bitmap = a.runtime.process.ports.unwrap().screen_bitmap;
        a.runtime
            .memory
            .write(GuestPtr(bitmap.0 + 12), &511u16.to_be_bytes())
            .unwrap();
        for y in 0..342 {
            let at = GuestPtr(SCREEN.0 + y * 64 + 63);
            let b = a.runtime.memory.read(at, 1).unwrap()[0] | 1;
            a.runtime.memory.write(at, &[b]).unwrap();
        }
        // Establish the bar at the new logical width before taking the baseline;
        // its rounded right corner legitimately shifts by one pixel.
        invoke(&mut a, 9, &[]);
        // A nonuniform background makes restoration observable, including row padding.
        a.runtime
            .memory
            .write(GuestPtr(SCREEN.0 + 64 * 20), &vec![0x97; 64 * 322])
            .unwrap();
        let mut before = screen(&a);
        a.runtime
            .memory
            .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
            .unwrap();
        a.runtime.process.mem_error = -111;
        motion(&mut a, &mut pointer, 15., 10.);
        button(&mut a, &mut pointer, true);
        events::flush(&mut a.runtime.memory, 1 << 1, 0).unwrap();
        assert_eq!(
            start(&mut a, arch, [10, 15]).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        let drawn = screen(&a);
        assert_ne!(drawn, before);
        let instructions = a.runtime.instructions;
        let calls = a.runtime.services.calls.len();
        assert_eq!(
            a.runtime.advance(100000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(a.runtime.instructions, instructions);
        assert_eq!(a.runtime.services.calls.len(), calls);
        assert_eq!(screen(&a), drawn);
        // Host edits outside the overlay remain canonical after restoration.
        before[64 * 341 + 63] = 0xe3;
        a.runtime
            .memory
            .write(GuestPtr(SCREEN.0 + 64 * 341 + 63), &[0xe3])
            .unwrap();
        // Traversing the bar's bottom divider must keep the open menu.
        motion(&mut a, &mut pointer, 30., 19.);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 1 });
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa26)).unwrap(), 70);
        motion(&mut a, &mut pointer, 30., 25.);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 1 });
        assert_eq!(a.runtime.memory.u32(GuestPtr(0xb54)).unwrap(), 70 << 16 | 1);
        // Unrelated queued input survives menu tracking. Release coordinates, rather
        // than the most recent motion, determine the selected item.
        events::post(&mut a.runtime.memory, 3, 0x1234, 0, a.runtime.process.input).unwrap();
        button(&mut a, &mut pointer, false);
        motion(&mut a, &mut pointer, 490., 300.);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(result(&a, arch), 70 << 16 | 1);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa26)).unwrap(), 70);
        assert_eq!(&screen(&a)[64 * 20..], &before[64 * 20..]);
        assert_eq!(a.runtime.process.mem_error, -111);
        assert_eq!(
            a.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
            (-108i16) as u16
        );
        assert_eq!(
            a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(),
            (-192i16) as u16
        );
        assert!(events::available(&a.runtime.memory, 1 << 3).unwrap());
        assert!(!events::available(&a.runtime.memory, 1 << 2).unwrap());
        // DrawMenuBar retains TheMenu until the caller explicitly clears it.
        let highlighted = screen(&a);
        invoke(&mut a, 9, &[]);
        assert_eq!(screen(&a), highlighted);
        hilite(&mut a, arch, 0);
        assert_eq!(screen(&a), before);
        assert_eq!(invoke(&mut a, 8, &[]), 70 << 16 | 1);
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn menu_switch_disabled_separator_and_outside_releases_cancel_with_exact_restoration() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for (point, expected, choice) in [
        ([42i16, 30i16], 0, 70 << 16 | 2),
        ([55, 30], 0, 0),
        ([300, 490], 0, 0),
    ] {
        let (mut a, mut pointer) = setup(&instance);
        let before = screen(&a);
        motion(&mut a, &mut pointer, 15., 10.);
        button(&mut a, &mut pointer, true);
        events::flush(&mut a.runtime.memory, 1 << 1, 0).unwrap();
        assert_eq!(
            start(&mut a, Architecture::PowerPc, [10, 15]).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        motion(
            &mut a,
            &mut pointer,
            f64::from(point[1]),
            f64::from(point[0]),
        );
        button(&mut a, &mut pointer, false);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(result(&a, Architecture::PowerPc), expected);
        assert_eq!(screen(&a), before);
        assert_eq!(a.runtime.memory.u32(GuestPtr(0xb54)).unwrap(), choice);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa26)).unwrap(), 0);
    }
    let (mut a, mut pointer) = setup(&instance);
    let before = screen(&a);
    let list = a.runtime.memory.u32(GuestPtr(0xa1c)).unwrap();
    let base = a.runtime.memory.u32(GuestPtr(list)).unwrap();
    let second = a.runtime.memory.u16(GuestPtr(base + 16)).unwrap() + 2;
    motion(&mut a, &mut pointer, 15., 10.);
    button(&mut a, &mut pointer, true);
    events::flush(&mut a.runtime.memory, 1 << 1, 0).unwrap();
    assert_eq!(
        start(&mut a, Architecture::PowerPc, [10, 15]).unwrap(),
        Stop::Waiting { ticks: 1 }
    );
    motion(&mut a, &mut pointer, f64::from(second), 10.);
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 1 });
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa26)).unwrap(), 80);
    motion(&mut a, &mut pointer, f64::from(second) + 15., 25.);
    button(&mut a, &mut pointer, false);
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(result(&a, Architecture::PowerPc), 80 << 16 | 1);
    hilite(&mut a, Architecture::PowerPc, 0);
    assert_eq!(screen(&a), before);
    // Physical release still works when the OS posting mask suppresses mouseUp.
    a.runtime
        .memory
        .write(GuestPtr(0x144), &(!4u16).to_be_bytes())
        .unwrap();
    motion(&mut a, &mut pointer, 15., 10.);
    button(&mut a, &mut pointer, true);
    events::flush(&mut a.runtime.memory, 1 << 1, 0).unwrap();
    assert_eq!(
        start(&mut a, Architecture::PowerPc, [10, 15]).unwrap(),
        Stop::Waiting { ticks: 1 }
    );
    motion(&mut a, &mut pointer, 30., 25.);
    button(&mut a, &mut pointer, false);
    assert!(!events::available(&a.runtime.memory, 1 << 2).unwrap());
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(result(&a, Architecture::PowerPc), 70 << 16 | 1);
    hilite(&mut a, Architecture::PowerPc, 0);
    assert_eq!(screen(&a), before);
    assert_eq!(
        start(&mut a, Architecture::PowerPc, [300, 490]).unwrap(),
        Stop::Budget
    );
    assert_eq!(result(&a, Architecture::PowerPc), 0);
    assert_eq!(screen(&a), before);
}

#[test]
fn menu_choice_reads_live_packed_identity_through_both_cpus_without_graphics() {
    let f = support::pef_program(&[0x4e800020], &[b"MenuChoice"], 16);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xaa, 0x66], Permissions::RX)
        .unwrap();
    for choice in [0u32, 0x12340002, 0xffff001f] {
        a.runtime
            .memory
            .write(GuestPtr(0xb54), &choice.to_be_bytes())
            .unwrap();
        assert_eq!(invoke(&mut a, 0, &[]), choice);
        a.runtime.memory.write(GuestPtr(STACK), &[0xa5; 8]).unwrap();
        a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, STACK);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK);
        assert_eq!(a.runtime.memory.u32(GuestPtr(STACK)).unwrap(), choice);
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(STACK + 4)).unwrap(),
            0xa5a5a5a5
        );
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn live_menu_edits_and_faults_do_not_consume_release_or_publish_partial_results() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let (mut a, mut pointer) = setup(&instance);
    let list = a.runtime.memory.u32(GuestPtr(0xa1c)).unwrap();
    let list_base = a.runtime.memory.u32(GuestPtr(list)).unwrap();
    let handle = a.runtime.memory.u32(GuestPtr(list_base + 6)).unwrap();
    let base = a.runtime.memory.u32(GuestPtr(handle)).unwrap();
    motion(&mut a, &mut pointer, 15., 10.);
    button(&mut a, &mut pointer, true);
    events::flush(&mut a.runtime.memory, 1 << 1, 0).unwrap();
    assert_eq!(
        start(&mut a, Architecture::M68k, [10, 15]).unwrap(),
        Stop::Waiting { ticks: 1 }
    );
    motion(&mut a, &mut pointer, 30., 25.);
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 1 });
    let selected = screen(&a);
    a.runtime
        .memory
        .write(GuestPtr(base + 10), &1u32.to_be_bytes())
        .unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 1 });
    assert_ne!(screen(&a), selected);
    assert_eq!(a.runtime.memory.u32(GuestPtr(0xb54)).unwrap(), 70 << 16 | 1);
    let before = screen(&a);
    let calls = a.runtime.services.calls.len();
    let ins = a.runtime.instructions;
    button(&mut a, &mut pointer, false);
    a.runtime
        .memory
        .write(GuestPtr(handle), &0x1234u32.to_be_bytes())
        .unwrap();
    assert!(a.runtime.advance(1).is_err());
    assert_eq!(screen(&a), before);
    assert_eq!(a.runtime.services.calls.len(), calls);
    assert_eq!(a.runtime.instructions, ins);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(STACK + 4), 4).unwrap(),
        [0xa5; 4]
    );
    assert!(events::available(&a.runtime.memory, 1 << 2).unwrap());
    // Unsupported callbacks fail before the initial screen or highlight changes.
    let (mut a, mut pointer) = setup(&instance);
    let before = screen(&a);
    a.runtime
        .memory
        .write(GuestPtr(0xa30), &0x1234u32.to_be_bytes())
        .unwrap();
    motion(&mut a, &mut pointer, 15., 10.);
    button(&mut a, &mut pointer, true);
    assert!(start(&mut a, Architecture::PowerPc, [10, 15]).is_err());
    assert_eq!(screen(&a), before);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa26)).unwrap(), 0);
    // Global per-item color overrides are also unsupported, rather than silently
    // drawing them with the standard monochrome palette.
    let (mut a, mut pointer) = setup(&instance);
    let before = screen(&a);
    let h = a.runtime.memory.u32(GuestPtr(0xd50)).unwrap();
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, h, 30)
        .unwrap();
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    let mut color = vec![0; 30];
    color[3] = 1;
    a.runtime.memory.write(GuestPtr(data), &color).unwrap();
    motion(&mut a, &mut pointer, 15., 10.);
    button(&mut a, &mut pointer, true);
    assert!(start(&mut a, Architecture::PowerPc, [10, 15]).is_err());
    assert_eq!(screen(&a), before);
}
