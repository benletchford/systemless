mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    controls::{Controls, Geometry, Op},
    cpu::{Architecture, Call, Cpu},
    graphics::{Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::{Process, HEAP_SIZE},
    quickdraw::{RectangleOp, SCREEN},
    runtime::Stop,
};
const STACK: u32 = STACK_BASE + 0x8000;
fn invoke(a: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let gate = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, gate, STACK);
    if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
        for (i, v) in args.iter().enumerate() {
            c.gpr[3 + i] = *v;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn direct(a: &mut LoadedApplication, op: Op) -> systemless_future::Result<u32> {
    let mut c = std::mem::take(&mut a.runtime.process.controls);
    let result = c.call(
        &mut a.runtime.memory,
        &mut a.runtime.process,
        a.runtime.graphics.as_ref(),
        op,
    );
    a.runtime.process.controls = c;
    result
}
fn set_rect(a: &mut LoadedApplication, address: u32, r: Rect) {
    let b: Vec<_> = [r.top, r.left, r.bottom, r.right]
        .into_iter()
        .flat_map(|n| (n as i16).to_be_bytes())
        .collect();
    a.runtime.memory.write(GuestPtr(address), &b).unwrap();
}
fn data(a: &LoadedApplication, h: u32) -> GuestPtr {
    GuestPtr(a.runtime.memory.u32(GuestPtr(h)).unwrap())
}
fn region(a: &LoadedApplication, h: u32) -> Vec<Rect> {
    let p = data(a, h);
    let n = a.runtime.memory.u16(p).unwrap();
    systemless_future::region_geometry::decode(a.runtime.memory.read(p, n as usize).unwrap())
        .unwrap()
}
fn screen(a: &LoadedApplication) -> Vec<u8> {
    a.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec()
}
fn assert_screen(a: &LoadedApplication, b: &LoadedApplication) {
    let x = screen(a);
    let y = screen(b);
    let differences: Vec<_> = x
        .iter()
        .zip(&y)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .take(16)
        .collect();
    assert!(differences.is_empty(), "differing bytes: {differences:?}");
}
fn setup(
    instance: &wgpu::Instance,
    kind: u32,
    r: Rect,
    visible: bool,
) -> (LoadedApplication, u32, u32) {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitFonts",
            b"InitWindows",
            b"NewWindow",
            b"NewControl",
            b"MoveControl",
            b"SizeControl",
            b"HideWindow",
        ],
        512,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    let storage = a.info.toc_or_a5;
    a.runtime.graphics = Some(
        pollster::block_on(Graphics::new(instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut a, 0, &[storage + 330]);
    invoke(&mut a, 1, &[]);
    invoke(&mut a, 2, &[]);
    set_rect(&mut a, storage + 64, Rect::new(40, 40, 260, 200));
    a.runtime
        .memory
        .write(GuestPtr(storage + 80), b"\x04Test")
        .unwrap();
    let owner = invoke(
        &mut a,
        3,
        &[0, storage + 64, storage + 80, 1, 2, u32::MAX, 0, 0],
    );
    set_rect(&mut a, storage + 64, r);
    a.runtime
        .memory
        .write(GuestPtr(storage + 96), &[0])
        .unwrap();
    let control = invoke(
        &mut a,
        4,
        &[
            owner,
            storage + 64,
            storage + 96,
            u32::from(visible),
            1,
            0,
            1,
            kind,
        ],
    );
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa9, 0x59, 0xa9, 0x5c],
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .write(
            GuestPtr(owner + 32),
            &[0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
        )
        .unwrap();
    (a, owner, control)
}
fn change(a: &mut LoadedApplication, arch: Architecture, h: u32, g: Geometry) {
    let (x, y, index) = match g {
        Geometry::Move { h, v } => (h, v, 0u32),
        Geometry::Size { width, height } => (width, height, 1),
    };
    if arch == Architecture::PowerPc {
        let incoming = invoke(a, 5 + index, &[h, x as i32 as u32, y as i32 as u32]);
        assert_eq!(incoming, h);
    } else {
        let mut frame = y.to_be_bytes().to_vec();
        frame.extend(x.to_be_bytes());
        frame.extend(h.to_be_bytes());
        frame.extend([0xa5; 4]);
        a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x50000000 + index * 2, STACK);
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[0] = 0x12345678;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK + 8);
        assert_eq!(c.dar[0], 0x12345678);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(STACK + 8), 4).unwrap(),
            [0xa5; 4]
        );
    }
}
fn clear_dirty(a: &mut LoadedApplication, owner: u32) -> u32 {
    let h = a.runtime.memory.u32(GuestPtr(owner + 122)).unwrap();
    systemless_future::regions::call(
        &mut a.runtime.memory,
        &mut a.runtime.process,
        systemless_future::regions::Op::Clear,
        h,
        0,
    )
    .unwrap();
    h
}
fn exhaust(a: &mut LoadedApplication) {
    for shift in (2..=HEAP_SIZE.ilog2()).rev() {
        while a
            .runtime
            .process
            .new_pointer(&mut a.runtime.memory, 1 << shift, false)
            .unwrap()
            != 0
        {}
    }
    assert_eq!(
        a.runtime
            .process
            .new_pointer(&mut a.runtime.memory, 4, false)
            .unwrap(),
        0
    );
}
#[test]
fn geometry_adapters_decode_signed_words_and_keep_void_returns_and_nil_requests() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0xff, 0xfd, 0, 7, 0x10, 0, 0x20, 0, 0xa5, 0xa5],
        Permissions::RW,
    )
    .unwrap();
    for (trap, g) in [
        (0xa959, Geometry::Move { h: 7, v: -3 }),
        (
            0xa95c,
            Geometry::Size {
                width: 7,
                height: -3,
            },
        ),
    ] {
        let mut c = m68k::CpuCore::default();
        c.dar[15] = 0x1000;
        assert_eq!(
            Abi::trap(trap, &mut c, &m).unwrap(),
            (
                Call::Control(Op::Geometry {
                    handle: 0x10002000,
                    change: g
                }),
                Reply::None
            )
        );
        assert_eq!(c.dar[15], 0x1008);
        c.dar[15] = 0x1004;
        assert!(Abi::trap(trap, &mut c, &m).is_err());
        assert_eq!(c.dar[15], 0x1004);
    }
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x10002000;
    c.gpr[4] = 0x12340007;
    c.gpr[5] = 0xabcdfffd;
    for (resize, g) in [
        (false, Geometry::Move { h: 7, v: -3 }),
        (
            true,
            Geometry::Size {
                width: 7,
                height: -3,
            },
        ),
    ] {
        assert_eq!(
            Abi::import(Service::ControlGeometry(resize), &c, &m).unwrap(),
            (
                Call::Control(Op::Geometry {
                    handle: 0x10002000,
                    change: g
                }),
                Reply::None
            )
        );
    }
    let mut p = Process::default();
    p.mem_error = -108;
    let mut controls = Controls::default();
    assert_eq!(
        controls
            .call(
                &mut m,
                &mut p,
                None,
                Op::Geometry {
                    handle: 0,
                    change: Geometry::Move { h: 1, v: 2 }
                }
            )
            .unwrap(),
        0
    );
    assert_eq!(p.mem_error, -108);
    for op in [0x159, 0x15c] {
        assert_ne!(
            systemless_future::traps::address(op, true),
            systemless_future::traps::UNIMPLEMENTED
        );
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn move_and_size_share_owner_erasure_dirty_regions_and_restored_port_state_on_both_cpus() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for kind in [0, 1, 2] {
            let (mut a, owner, h) = setup(&instance, kind, Rect::new(10, 10, 90, 30), true);
            let dirty = clear_dirty(&mut a, owner);
            let old_control = a.runtime.memory.read(data(&a, h), 296).unwrap().to_vec();
            let port = a
                .runtime
                .memory
                .read(GuestPtr(owner), 108)
                .unwrap()
                .to_vec();
            let ports = a.runtime.process.ports.unwrap();
            ports.set(&mut a.runtime.memory, GuestPtr(0)).unwrap();
            a.runtime
                .process
                .set_handle_lock(&a.runtime.memory, h, true)
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
                .unwrap();
            a.runtime.process.mem_error = -111;
            change(&mut a, arch, h, Geometry::Move { h: 100, v: 60 });
            change(
                &mut a,
                arch,
                h,
                Geometry::Size {
                    width: 60,
                    height: 28,
                },
            );
            assert_eq!(
                systemless_future::quickdraw::read_rect(
                    &a.runtime.memory,
                    GuestPtr(data(&a, h).0 + 8)
                )
                .unwrap(),
                Rect::new(100, 60, 160, 88)
            );
            let mut expected = old_control;
            for (i, n) in [60i16, 100, 88, 160].into_iter().enumerate() {
                expected[8 + i * 2..10 + i * 2].copy_from_slice(&n.to_be_bytes());
            }
            assert_eq!(a.runtime.memory.read(data(&a, h), 296).unwrap(), expected);
            assert_eq!(a.runtime.memory.read(GuestPtr(owner), 108).unwrap(), port);
            assert_eq!(a.runtime.memory.u32(ports.current_slot).unwrap(), 0);
            assert_eq!(a.runtime.process.mem_error, -111);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
                (-108i16) as u16
            );
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(),
                (-192i16) as u16
            );
            let r = region(&a, dirty);
            assert_eq!(
                r,
                vec![Rect::new(50, 50, 130, 70), Rect::new(140, 100, 220, 120)]
            );
            let (mut b, other, control) = setup(&instance, kind, Rect::new(100, 60, 160, 88), true);
            let other_ports = b.runtime.process.ports.unwrap();
            other_ports
                .set(&mut b.runtime.memory, GuestPtr(other))
                .unwrap();
            for r in [Rect::new(10, 10, 90, 30), Rect::new(100, 60, 180, 80)] {
                let rectangle = b.info.toc_or_a5 + 64;
                set_rect(&mut b, rectangle, r);
                other_ports
                    .draw_rectangle(
                        &mut b.runtime.memory,
                        b.runtime.graphics.as_ref().unwrap(),
                        GuestPtr(rectangle),
                        RectangleOp::Erase,
                    )
                    .unwrap();
            }
            direct(&mut b, Op::Draw(control)).unwrap();
            assert_screen(&a, &b);
        }
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn hidden_controls_only_change_geometry_and_full_heap_moves_do_not_allocate() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, owner, h) = setup(&instance, 0, Rect::new(10, 10, 90, 30), false);
        let dirty = clear_dirty(&mut a, owner);
        let pixels = screen(&a);
        let master = data(&a, h);
        a.runtime.graphics = None;
        change(&mut a, arch, h, Geometry::Move { h: -12, v: -7 });
        change(
            &mut a,
            arch,
            h,
            Geometry::Size {
                width: 22,
                height: 13,
            },
        );
        assert_eq!(data(&a, h), master);
        assert_eq!(screen(&a), pixels);
        assert!(region(&a, dirty).is_empty());
        assert_eq!(
            systemless_future::quickdraw::read_rect(&a.runtime.memory, GuestPtr(master.0 + 8))
                .unwrap(),
            Rect::new(-12, -7, 10, 6)
        );
    }
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, owner, h) = setup(&instance, 0, Rect::new(10, 10, 90, 30), true);
        invoke(&mut a, 7, &[owner]);
        let dirty = clear_dirty(&mut a, owner);
        let pixels = screen(&a);
        a.runtime.graphics = None;
        change(&mut a, arch, h, Geometry::Move { h: 100, v: 60 });
        assert_eq!(screen(&a), pixels);
        assert_eq!(
            a.runtime
                .memory
                .read(GuestPtr(data(&a, h).0 + 16), 1)
                .unwrap(),
            [255]
        );
        assert_eq!(region(&a, dirty), vec![Rect::new(50, 50, 130, 70)]);
    }
    let (mut a, owner, h) = setup(&instance, 0, Rect::new(10, 10, 90, 30), true);
    clear_dirty(&mut a, owner);
    exhaust(&mut a);
    a.runtime.process.mem_error = -111;
    change(
        &mut a,
        Architecture::M68k,
        h,
        Geometry::Move { h: 100, v: 60 },
    );
    assert_eq!(a.runtime.process.mem_error, -111);
    assert_eq!(
        a.runtime
            .process
            .new_pointer(&mut a.runtime.memory, 4, false)
            .unwrap(),
        0
    );
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn bad_geometry_graphics_state_and_allocation_fail_before_rect_or_pixels_change() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let (mut a, owner, h) = setup(&instance, 0, Rect::new(10, 10, 90, 30), true);
    let dirty = clear_dirty(&mut a, owner);
    let old_rect = a
        .runtime
        .memory
        .read(GuestPtr(data(&a, h).0 + 8), 8)
        .unwrap()
        .to_vec();
    let pixels = screen(&a);
    for change in [
        Geometry::Move { h: 32767, v: 10 },
        Geometry::Size {
            width: 0,
            height: 10,
        },
        Geometry::Size {
            width: -1,
            height: 10,
        },
    ] {
        assert!(direct(&mut a, Op::Geometry { handle: h, change }).is_err());
        assert_eq!(screen(&a), pixels);
        assert_eq!(
            a.runtime
                .memory
                .read(GuestPtr(data(&a, h).0 + 8), 8)
                .unwrap(),
            old_rect
        );
        assert!(region(&a, dirty).is_empty());
    }
    a.runtime
        .memory
        .write(GuestPtr(owner + 92), &1u32.to_be_bytes())
        .unwrap();
    assert!(direct(
        &mut a,
        Op::Geometry {
            handle: h,
            change: Geometry::Move { h: 100, v: 60 }
        }
    )
    .is_err());
    assert_eq!(screen(&a), pixels);
    a.runtime
        .memory
        .write(GuestPtr(owner + 92), &0u32.to_be_bytes())
        .unwrap();
    // Growing a disjoint update region needs space. Preflight must catch its
    // allocation failure before erasing even one old-control pixel.
    let d = data(&a, dirty);
    let b: Vec<_> = [10i16, 170, 230, 180, 240]
        .into_iter()
        .flat_map(i16::to_be_bytes)
        .collect();
    a.runtime.memory.write(d, &b).unwrap();
    let dirty_before = a.runtime.memory.read(d, 10).unwrap().to_vec();
    exhaust(&mut a);
    a.runtime.process.mem_error = -111;
    assert!(direct(
        &mut a,
        Op::Geometry {
            handle: h,
            change: Geometry::Move { h: 100, v: 60 }
        }
    )
    .is_err());
    assert_eq!(a.runtime.process.mem_error, -111);
    assert_eq!(screen(&a), pixels);
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(data(&a, h).0 + 8), 8)
            .unwrap(),
        old_rect
    );
    assert_eq!(a.runtime.memory.read(d, 10).unwrap(), dirty_before);
}
