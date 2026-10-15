mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    graphics::{Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    region_geometry,
    runtime::Stop,
};
fn invoke(app: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        panic!()
    };
    for (i, &v) in args.iter().enumerate() {
        c.gpr[3 + i] = v;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn setup() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"OpenPort",
            b"SetPortBits",
            b"SetOrigin",
            b"PaintRect",
            b"OffsetRgn",
        ],
        0,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 4096], Permissions::RW)
        .unwrap();
    invoke(&mut app, 0, &[0x600]);
    invoke(&mut app, 1, &[0x1000]);
    let mut bitmap = 0x1200u32.to_be_bytes().to_vec();
    bitmap.extend([0, 2, 0, 0, 0, 0, 0, 4, 0, 8]);
    app.runtime.memory.write(GuestPtr(0x1100), &bitmap).unwrap();
    invoke(&mut app, 2, &[0x1100]);
    app.runtime
        .process
        .ports
        .unwrap()
        .port_size(&mut app.runtime.memory, 8, 4)
        .unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            support::words(&[0xa878, 0xa8e0]),
            Permissions::RX,
        )
        .unwrap();
    app
}
fn native(app: &mut LoadedApplication, h: i16, v: i16) {
    let sp = STACK_BASE + 0x8000;
    let mut args = v.to_be_bytes().to_vec();
    args.extend(h.to_be_bytes());
    args.extend([0x5a; 4]);
    app.runtime.memory.write(GuestPtr(sp), &args).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, sp);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0xfeedbeef;
    c.dar[8] = 0xaabbccdd;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0xfeedbeef);
    assert_eq!(c.dar[8], 0xaabbccdd);
    assert_eq!(c.dar[15], sp + 4);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(sp + 4), 4).unwrap(),
        [0x5a; 4]
    );
}
#[test]
fn origin_translates_complex_visibility_in_place_but_preserves_clip_pen_and_other_port_fields() {
    let mut app = setup();
    let ports = app.runtime.process.ports.unwrap();
    let visible = app.runtime.memory.u32(GuestPtr(0x1018)).unwrap();
    let clip = app.runtime.memory.u32(GuestPtr(0x101c)).unwrap();
    let words = [
        44i16, 0, 0, 4, 8, 0, 0, 8, 32767, 1, 3, 5, 32767, 3, 3, 5, 32767, 4, 0, 8, 32767, 32767,
    ];
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, visible, 44)
        .unwrap();
    let data = app.runtime.memory.u32(GuestPtr(visible)).unwrap();
    let original: Vec<u8> = words.into_iter().flat_map(i16::to_be_bytes).collect();
    app.runtime.memory.write(GuestPtr(data), &original).unwrap();
    app.runtime
        .process
        .set_handle_lock(&app.runtime.memory, visible, true)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1030), &[0, 7, 0, 9])
        .unwrap();
    let before = app
        .runtime
        .memory
        .read(GuestPtr(0x1000), 108)
        .unwrap()
        .to_vec();
    let clip_data = app.runtime.memory.u32(GuestPtr(clip)).unwrap();
    let clip_before = app
        .runtime
        .memory
        .read(GuestPtr(clip_data), 10)
        .unwrap()
        .to_vec();
    app.runtime.process.mem_error = -108;
    ports
        .set_origin(&mut app.runtime.memory, &mut app.runtime.process, 20, 30)
        .unwrap();
    assert_eq!(app.runtime.process.mem_error, -108);
    assert_eq!(app.runtime.memory.u32(GuestPtr(visible)).unwrap(), data);
    let moved = app.runtime.memory.read(GuestPtr(data), 44).unwrap();
    let expected: Vec<_> = region_geometry::decode(&original)
        .unwrap()
        .into_iter()
        .map(|r| Rect::new(r.left + 20, r.top + 30, r.right + 20, r.bottom + 30))
        .collect();
    assert_eq!(region_geometry::decode(moved).unwrap(), expected);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(clip_data), 10).unwrap(),
        clip_before
    );
    let after = app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap();
    assert_eq!(&after[..8], &before[..8]);
    assert_eq!(&after[24..], &before[24..]);
    assert_eq!(
        &after[8..24],
        &[0, 30, 0, 20, 0, 34, 0, 28, 0, 30, 0, 20, 0, 34, 0, 28]
    );
    assert_eq!(
        ports
            .point_coordinates(&app.runtime.memory, [0; 4], false)
            .unwrap(),
        [0, 30, 0, 20]
    );
    ports
        .set_origin(&mut app.runtime.memory, &mut app.runtime.process, 0, 0)
        .unwrap();
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
        before
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(data), 44).unwrap(),
        original
    );
    systemless_future::regions::call(
        &mut app.runtime.memory,
        &mut app.runtime.process,
        systemless_future::regions::Op::Offset,
        visible,
        (43u32 << 16) | ((-6i16) as u16 as u32),
    )
    .unwrap();
    let shifted = app.runtime.memory.read(GuestPtr(data), 44).unwrap();
    let expected: Vec<_> = region_geometry::decode(&original)
        .unwrap()
        .into_iter()
        .map(|r| Rect::new(r.left - 6, r.top + 43, r.right - 6, r.bottom + 43))
        .collect();
    assert_eq!(region_geometry::decode(shifted).unwrap(), expected);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
        before
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(clip_data), 10).unwrap(),
        clip_before
    );
    assert_eq!(app.runtime.process.mem_error, -108);
}
#[test]
fn failed_origin_changes_preserve_port_and_visibility_without_partial_publication() {
    let mut app = setup();
    let ports = app.runtime.process.ports.unwrap();
    let visible = app.runtime.memory.u32(GuestPtr(0x1018)).unwrap();
    let data = app.runtime.memory.u32(GuestPtr(visible)).unwrap();
    let before = app
        .runtime
        .memory
        .read(GuestPtr(0x1000), 108)
        .unwrap()
        .to_vec();
    let region = app
        .runtime
        .memory
        .read(GuestPtr(data), 10)
        .unwrap()
        .to_vec();
    app.runtime.process.mem_error = -111;
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), before.clone(), Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .write(ports.current_slot, &0x51000000u32.to_be_bytes())
        .unwrap();
    assert!(ports
        .set_origin(&mut app.runtime.memory, &mut app.runtime.process, 1, 1)
        .is_err());
    assert_eq!(app.runtime.memory.read(GuestPtr(data), 10).unwrap(), region);
    assert_eq!(app.runtime.process.mem_error, -111);
    app.runtime
        .memory
        .write(ports.current_slot, &0x1000u32.to_be_bytes())
        .unwrap();
    assert!(ports
        .set_origin(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            32767,
            32767
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
        before
    );
    assert_eq!(app.runtime.memory.read(GuestPtr(data), 10).unwrap(), region);
    let clip = app.runtime.memory.u32(GuestPtr(0x101c)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x101c), &visible.to_be_bytes())
        .unwrap();
    assert!(ports
        .set_origin(&mut app.runtime.memory, &mut app.runtime.process, 1, 1)
        .is_err());
    app.runtime
        .memory
        .write(GuestPtr(0x101c), &clip.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(visible), &0u32.to_be_bytes())
        .unwrap();
    assert!(ports
        .set_origin(&mut app.runtime.memory, &mut app.runtime.process, 1, 1)
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
        before
    );
    app.runtime
        .memory
        .write(GuestPtr(visible), &data.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1006), &0xc002u16.to_be_bytes())
        .unwrap();
    assert!(ports
        .set_origin(&mut app.runtime.memory, &mut app.runtime.process, 1, 1)
        .is_err());
    app.runtime
        .memory
        .write(GuestPtr(0x1006), &2u16.to_be_bytes())
        .unwrap();
    systemless_future::regions::call(
        &mut app.runtime.memory,
        &mut app.runtime.process,
        systemless_future::regions::Op::Clear,
        visible,
        0,
    )
    .unwrap();
    assert!(ports
        .set_origin(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            32767,
            32767
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
        before
    );
    assert_eq!(app.runtime.process.mem_error, -111);
}
#[test]
fn origin_abis_keep_signed_word_order_void_replies_and_shared_port_state() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0, 3, 0xff, 0xfe], Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa878, c, &m).unwrap(),
        (Call::SetOrigin { h: -2, v: 3 }, Reply::None)
    );
    assert_eq!(c.dar[15], 0x1004);
    c.dar[15] = 0x1001;
    assert!(Abi::trap(0xa878, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1001);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0xabcdfffe;
    c.gpr[4] = 0xfedc0003;
    assert_eq!(
        Abi::import(Service::SetOrigin, c, &m).unwrap(),
        (Call::SetOrigin { h: -2, v: 3 }, Reply::None)
    );
    let mut app = setup();
    app.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    native(&mut app, -2, 3);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1010), 4).unwrap(),
        [0, 3, 0xff, 0xfe]
    );
    assert_eq!(invoke(&mut app, 3, &[0, 0]), 0);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1010), 4).unwrap(),
        [0; 4]
    );
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
        (-108i16) as u16
    );
    let entry = systemless_future::traps::lookup(&app.runtime.memory, 0xa878, true).unwrap();
    assert_ne!(entry, systemless_future::traps::UNIMPLEMENTED);
    assert_eq!(systemless_future::traps::opcode(entry), Some(0xa878));
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_drawing_after_origin_uses_live_visibility_and_unmoved_clip() {
    let mut app = setup();
    let ports = app.runtime.process.ports.unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap());
    let initial = [0, 0x5a, 0, 0x5a, 0, 0x5a, 0, 0x5a];
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        ports
            .set_origin(&mut app.runtime.memory, &mut app.runtime.process, 0, 0)
            .unwrap();
        ports
            .clip_rect(&mut app.runtime.memory, Rect::new(0, 0, 8, 4))
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x1200), &initial)
            .unwrap();
        if arch == Architecture::M68k {
            native(&mut app, 20, 30);
        } else {
            assert_eq!(invoke(&mut app, 3, &[20, 30]), 20);
        }
        app.runtime
            .memory
            .write(GuestPtr(0x350), &[0, 30, 0, 20, 0, 34, 0, 28])
            .unwrap();
        invoke(&mut app, 4, &[0x350]);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
            initial
        );
        ports
            .clip_rect(&mut app.runtime.memory, Rect::new(20, 30, 28, 34))
            .unwrap();
        invoke(&mut app, 4, &[0x350]);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
            [0xff, 0x5a, 0xff, 0x5a, 0xff, 0x5a, 0xff, 0x5a]
        );
    }
}

#[test]
fn offset_region_adapters_unpack_signed_deltas_and_share_live_region_storage() {
    let mut app = setup();
    let visible = app.runtime.memory.u32(GuestPtr(0x1018)).unwrap();
    let data = app.runtime.memory.u32(GuestPtr(visible)).unwrap();
    let original = app
        .runtime
        .memory
        .read(GuestPtr(data), 10)
        .unwrap()
        .to_vec();
    let mut m = Memory::default();
    let mut args = 4i16.to_be_bytes().to_vec();
    args.extend(3i16.to_be_bytes());
    args.extend(visible.to_be_bytes());
    m.map(GuestPtr(0x1000), args.clone(), Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa8e0, c, &m).unwrap(),
        (
            Call::Region {
                op: systemless_future::regions::Op::Offset,
                a: visible,
                b: 0x00040003
            },
            Reply::None
        )
    );
    assert_eq!(c.dar[15], 0x1008);
    c.dar[15] = 0x1001;
    assert!(Abi::trap(0xa8e0, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1001);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = visible;
    c.gpr[4] = 0xabcdfffa;
    c.gpr[5] = 0x7654002b;
    assert_eq!(
        Abi::import(
            Service::Region(systemless_future::regions::Op::Offset),
            c,
            &m
        )
        .unwrap(),
        (
            Call::Region {
                op: systemless_future::regions::Op::Offset,
                a: visible,
                b: 0x002bfffa
            },
            Reply::None
        )
    );
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        app.runtime.memory.write(GuestPtr(data), &original).unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
            .unwrap();
        if arch == Architecture::PowerPc {
            assert_eq!(invoke(&mut app, 5, &[visible, 3, 4]), visible);
        } else {
            let sp = STACK_BASE + 0x8000;
            let mut bytes = args.clone();
            bytes.extend([0x5a; 4]);
            app.runtime.memory.write(GuestPtr(sp), &bytes).unwrap();
            app.runtime.cpu = Cpu::new(arch, 0x50000002, sp);
            let Cpu::M68k(c) = &mut app.runtime.cpu else {
                panic!()
            };
            c.dar[0] = 0xfeedbeef;
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            let Cpu::M68k(c) = &app.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.dar[0], 0xfeedbeef);
            assert_eq!(c.dar[15], sp + 8);
            assert_eq!(
                app.runtime.memory.read(GuestPtr(sp + 8), 4).unwrap(),
                [0x5a; 4]
            );
        }
        assert_eq!(app.runtime.memory.u32(GuestPtr(visible)).unwrap(), data);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(data + 2), 8).unwrap(),
            [0, 4, 0, 3, 1, 90, 2, 3]
        );
        assert_eq!(
            app.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
            (-108i16) as u16
        );
    }
    let before = app
        .runtime
        .memory
        .read(GuestPtr(data), 10)
        .unwrap()
        .to_vec();
    assert!(systemless_future::regions::call(
        &mut app.runtime.memory,
        &mut app.runtime.process,
        systemless_future::regions::Op::Offset,
        visible,
        0x7fff7fff
    )
    .is_err());
    assert_eq!(app.runtime.memory.read(GuestPtr(data), 10).unwrap(), before);
    let entry = systemless_future::traps::lookup(&app.runtime.memory, 0xa8e0, true).unwrap();
    assert_eq!(systemless_future::traps::opcode(entry), Some(0xa8e0));
}
