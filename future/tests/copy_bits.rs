mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::CopyBits,
    runtime::Stop,
};
fn request() -> CopyBits {
    CopyBits {
        source: GuestPtr(0x3000),
        destination: GuestPtr(0x1002),
        source_rect: GuestPtr(0x300),
        destination_rect: GuestPtr(0x310),
        mode: 0,
        mask: GuestPtr(0),
    }
}
fn args(r: CopyBits) -> Vec<u8> {
    let mut b = r.mask.0.to_be_bytes().to_vec();
    b.extend(r.mode.to_be_bytes());
    for p in [r.destination_rect, r.source_rect, r.destination, r.source] {
        b.extend(p.0.to_be_bytes());
    }
    b
}
fn invoke(app: &mut LoadedApplication, index: u32, arguments: &[u32]) -> u32 {
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
    for (i, &v) in arguments.iter().enumerate() {
        c.gpr[3 + i] = v;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
#[test]
fn adapters_decode_pascal_22_bytes_and_six_ppc_arguments_without_results() {
    let mut r = request();
    r.mode = -3;
    r.mask = GuestPtr(0x11223344);
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), args(r), Permissions::RX).unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa8ec, c, &m).unwrap(),
        (Call::CopyBits(r), Reply::None)
    );
    assert_eq!(c.dar[15], 0x1016);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0x8000);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    for (i, v) in [
        r.source.0,
        r.destination.0,
        r.source_rect.0,
        r.destination_rect.0,
        0xabcdfffdu32,
        r.mask.0,
    ]
    .into_iter()
    .enumerate()
    {
        c.gpr[3 + i] = v;
    }
    assert_eq!(
        Abi::import(Service::CopyBits, c, &m).unwrap(),
        (Call::CopyBits(r), Reply::None)
    );
    m.map(GuestPtr(0x2000), vec![0; 21], Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x2000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa8ec, c, &m).is_err());
    assert_eq!(c.dar[15], 0x2000);
}
#[test]
fn nil_bitmap_service_is_inert_without_graphics_or_port_initialization() {
    let f = support::pef_program(&[0x4e800020], &[b"CopyBits"], 0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let before = app
        .runtime
        .memory
        .read(GuestPtr(0), 0x1000)
        .unwrap()
        .to_vec();
    assert_eq!(
        invoke(
            &mut app,
            0,
            &[0, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff]
        ),
        0
    );
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0xec], Permissions::RX)
        .unwrap();
    native(
        &mut app,
        CopyBits {
            source: GuestPtr(0),
            destination: GuestPtr(u32::MAX),
            source_rect: GuestPtr(u32::MAX),
            destination_rect: GuestPtr(u32::MAX),
            mode: -1,
            mask: GuestPtr(u32::MAX),
        },
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0), 0x1000).unwrap(),
        before
    );
}
fn bitmap(base: u32) -> Vec<u8> {
    let mut b = base.to_be_bytes().to_vec();
    b.extend([0, 2, 0, 0, 0, 0, 0, 4, 0, 8]);
    b
}
fn setup() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"InitGraf", b"OpenPort", b"SetPortBits", b"CopyBits"],
        0,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 0x1000], Permissions::RW)
        .unwrap();
    invoke(&mut app, 0, &[0x600]);
    invoke(&mut app, 1, &[0x1000]);
    app.runtime
        .memory
        .write(GuestPtr(0x1100), &bitmap(0x1200))
        .unwrap();
    invoke(&mut app, 2, &[0x1100]);
    let ports = app.runtime.process.ports.unwrap();
    ports.port_size(&mut app.runtime.memory, 8, 4).unwrap();
    let mut source = bitmap(0x3010);
    source.resize(16, 0);
    source.extend([0xa5, 0x5a, 0x5a, 0x5a, 0xf0, 0x5a, 0x0f, 0x5a]);
    app.runtime
        .memory
        .map(GuestPtr(0x3000), source, Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x300), &[0, 0, 0, 0, 0, 4, 0, 8])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x310), &[0, 0, 0, 2, 0, 4, 0, 6])
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0xec], Permissions::RX)
        .unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap());
    app
}
fn ppc(app: &mut LoadedApplication, r: CopyBits) {
    assert_eq!(
        invoke(
            app,
            3,
            &[
                r.source.0,
                r.destination.0,
                r.source_rect.0,
                r.destination_rect.0,
                r.mode as i32 as u32,
                r.mask.0
            ]
        ),
        r.source.0
    );
}
fn native(app: &mut LoadedApplication, r: CopyBits) {
    let stack = STACK_BASE + 0x8000;
    let mut b = args(r);
    b.extend([0x5a; 4]);
    app.runtime.memory.write(GuestPtr(stack), &b).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0x12345678;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 22);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 22), 4).unwrap(),
        [0x5a; 4]
    );
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_cpus_scale_readonly_bits_with_hole_masks_all_modes_and_live_colors() {
    let mut app = setup();
    let h = app
        .runtime
        .process
        .new_handle(&mut app.runtime.memory, 44, false)
        .unwrap();
    let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
    let words = [
        44i16, 0, 0, 4, 8, 0, 0, 8, 32767, 1, 3, 5, 32767, 3, 3, 5, 32767, 4, 0, 8, 32767, 32767,
    ];
    let bytes: Vec<_> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    app.runtime.memory.write(GuestPtr(data), &bytes).unwrap();
    let mut r = request();
    r.mask = GuestPtr(h);
    let initial = [0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a];
    for swap in [false, true] {
        app.runtime
            .memory
            .write(
                GuestPtr(0x1050),
                &(if swap { 30u32 } else { 33 }).to_be_bytes(),
            )
            .unwrap();
        app.runtime
            .memory
            .write(
                GuestPtr(0x1054),
                &(if swap { 33u32 } else { 30 }).to_be_bytes(),
            )
            .unwrap();
        let state = app
            .runtime
            .memory
            .read(GuestPtr(0x1000), 108)
            .unwrap()
            .to_vec();
        for mode in 0..8 {
            r.mode = mode;
            let mut expected = initial;
            for y in 0..4 {
                for x in 2..6 {
                    if (1..3).contains(&y) && (3..5).contains(&x) {
                        continue;
                    }
                    let sx = [0, 2, 4, 6][x - 2];
                    let row = [0xa5u8, 0x5a, 0xf0, 0x0f][y];
                    let bit = ((row >> (7 - sx)) | (row >> (6 - sx))) & 1;
                    let bit = bit ^ u8::from(mode & 4 != 0) ^ u8::from(swap);
                    let old = (0xa5 >> (7 - x)) & 1;
                    let value = match mode & 3 {
                        0 => bit,
                        1 => old | bit,
                        2 => old ^ bit,
                        _ => old & (!bit & 1),
                    };
                    expected[y * 2] = (expected[y * 2] & !(1 << (7 - x))) | (value << (7 - x));
                }
            }
            for architecture in [Architecture::PowerPc, Architecture::M68k] {
                app.runtime
                    .memory
                    .write(GuestPtr(0x1200), &initial)
                    .unwrap();
                if architecture == Architecture::PowerPc {
                    ppc(&mut app, r);
                } else {
                    native(&mut app, r);
                }
                assert_eq!(
                    app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
                    expected,
                    "{architecture:?} mode {mode} swap {swap}"
                );
                assert_eq!(
                    app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
                    state
                );
            }
        }
    }
    // Reduce vertically: opposite adjacent rows must combine instead of losing ink.
    app.runtime
        .memory
        .write(GuestPtr(0x1050), &33u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1054), &30u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x310), &[0, 0, 0, 0, 0, 2, 0, 8])
        .unwrap();
    r.mode = 0;
    r.mask = GuestPtr(0);
    for architecture in [Architecture::PowerPc, Architecture::M68k] {
        app.runtime
            .memory
            .write(GuestPtr(0x1200), &[0, 0x5a, 0, 0x5a, 0, 0x5a, 0, 0x5a])
            .unwrap();
        if architecture == Architecture::PowerPc {
            ppc(&mut app, r);
        } else {
            native(&mut app, r);
        }
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
            [0xff, 0x5a, 0xff, 0x5a, 0, 0x5a, 0, 0x5a]
        );
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn overlap_uses_snapshot_and_foreign_destination_ignores_current_port_visibility() {
    let mut app = setup();
    let initial = [0xa5, 0x5a, 0x5a, 0x5a, 0xf0, 0x5a, 0x0f, 0x5a];
    let mut r = request();
    r.source = GuestPtr(0x1002);
    app.runtime
        .memory
        .write(GuestPtr(0x300), &[0, 0, 0, 0, 0, 4, 0, 6])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x310), &[0, 0, 0, 1, 0, 4, 0, 7])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1200), &initial)
        .unwrap();
    ppc(&mut app, r);
    let mut expected = initial;
    for y in 0..4 {
        for x in 1..7 {
            let bit = (initial[y * 2] >> (8 - x)) & 1;
            expected[y * 2] = (expected[y * 2] & !(1 << (7 - x))) | (bit << (7 - x));
        }
    }
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
        expected
    );
    let vis = app.runtime.memory.u32(GuestPtr(0x1018)).unwrap();
    let data = app.runtime.memory.u32(GuestPtr(vis)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(data), &[0, 10, 0, 0, 0, 0, 0, 0, 0, 0])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1200), &initial)
        .unwrap();
    ppc(&mut app, r);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
        initial
    );
    app.runtime
        .memory
        .write(GuestPtr(0x1120), &bitmap(0x1400))
        .unwrap();
    r.destination = GuestPtr(0x1120);
    ppc(&mut app, r);
    let mut foreign = [0; 8];
    for y in 0..4 {
        foreign[y * 2] = (initial[y * 2] >> 1) & 0x7e;
    }
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1400), 8).unwrap(),
        foreign
    );
    let ports = app.runtime.process.ports.unwrap();
    let gpu = app.runtime.graphics.take().unwrap();
    r.mode = 99;
    let before = app
        .runtime
        .memory
        .read(GuestPtr(0x1400), 8)
        .unwrap()
        .to_vec();
    assert!(ports.copy_bits(&mut app.runtime.memory, &gpu, r).is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1400), 8).unwrap(),
        before
    );
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn current_port_clip_translates_to_alias_bounds_and_readonly_destinations_fault() {
    let mut app = setup();
    let ports = app.runtime.process.ports.unwrap();
    ports
        .clip_rect(
            &mut app.runtime.memory,
            systemless_future::graphics::Rect::new(2, 1, 6, 3),
        )
        .unwrap();
    let mut b = bitmap(0x1200);
    b[6..14].copy_from_slice(&[0xff, 0xff, 0xff, 0xfe, 0, 3, 0, 6]);
    app.runtime.memory.write(GuestPtr(0x1120), &b).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x310), &[0xff, 0xff, 0xff, 0xfe, 0, 3, 0, 6])
        .unwrap();
    let mut r = request();
    r.destination = GuestPtr(0x1120);
    ppc(&mut app, r);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
        [0, 0, 0x18, 0, 0x30, 0, 0, 0]
    );
    let mut read_only = bitmap(0x3110);
    read_only.resize(24, 0x55);
    app.runtime
        .memory
        .map(GuestPtr(0x3100), read_only, Permissions::RX)
        .unwrap();
    r.destination = GuestPtr(0x3100);
    let gpu = app.runtime.graphics.take().unwrap();
    assert!(ports.copy_bits(&mut app.runtime.memory, &gpu, r).is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x3110), 8).unwrap(),
        [0x55; 8]
    );
}
