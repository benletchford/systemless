mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::CopyMask,
    runtime::Stop,
};
fn request() -> CopyMask {
    CopyMask {
        source: GuestPtr(0x3000),
        mask: GuestPtr(0x1300),
        destination: GuestPtr(0x1002),
        source_rect: GuestPtr(0x300),
        mask_rect: GuestPtr(0x320),
        destination_rect: GuestPtr(0x310),
    }
}
fn args(r: CopyMask) -> Vec<u8> {
    [
        r.destination_rect,
        r.mask_rect,
        r.source_rect,
        r.destination,
        r.mask,
        r.source,
    ]
    .iter()
    .flat_map(|p| p.0.to_be_bytes())
    .collect()
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
fn bitmap(base: u32) -> Vec<u8> {
    let mut b = base.to_be_bytes().to_vec();
    b.extend([0, 2, 0, 0, 0, 0, 0, 4, 0, 8]);
    b
}
fn setup() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"InitGraf", b"OpenPort", b"SetPortBits", b"CopyMask"],
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
        .map(GuestPtr(0x50000000), vec![0xa8, 0x17], Permissions::RX)
        .unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap());
    app
}

fn prepare() -> LoadedApplication {
    let mut app = setup();
    app.runtime
        .memory
        .write(GuestPtr(0x1300), &bitmap(0x1310))
        .unwrap();
    app.runtime
        .memory
        .write(
            GuestPtr(0x1310),
            &[0xf0, 0x33, 0x0f, 0x44, 0xaa, 0x55, 0x55, 0x66],
        )
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x320), &[0, 0, 0, 0, 0, 4, 0, 8])
        .unwrap();
    app
}
fn run(app: &mut LoadedApplication, arch: Architecture, r: CopyMask) {
    if arch == Architecture::PowerPc {
        assert_eq!(
            invoke(
                app,
                3,
                &[
                    r.source.0,
                    r.mask.0,
                    r.destination.0,
                    r.source_rect.0,
                    r.mask_rect.0,
                    r.destination_rect.0
                ]
            ),
            r.source.0
        );
    } else {
        let sp = STACK_BASE + 0x8000;
        let mut bytes = args(r);
        bytes.extend([0x5a; 4]);
        app.runtime.memory.write(GuestPtr(sp), &bytes).unwrap();
        app.runtime.cpu = Cpu::new(arch, 0x50000000, sp);
        let Cpu::M68k(c) = &mut app.runtime.cpu else {
            panic!()
        };
        c.dar[0] = 0xfeedbeef;
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &app.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], sp + 24);
        assert_eq!(c.dar[0], 0xfeedbeef);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(sp + 24), 4).unwrap(),
            [0x5a; 4]
        );
    }
}
fn rect(app: &mut LoadedApplication, p: u32, r: [i16; 4]) {
    app.runtime
        .memory
        .write(
            GuestPtr(p),
            &r.into_iter().flat_map(i16::to_be_bytes).collect::<Vec<_>>(),
        )
        .unwrap();
}
#[test]
fn mask_adapters_decode_six_pointers_and_check_complete_native_storage() {
    let mut installed = Memory::default();
    systemless_future::traps::install(&mut installed).unwrap();
    let entry = systemless_future::traps::lookup(&installed, 0xa817, true).unwrap();
    assert_ne!(entry, systemless_future::traps::UNIMPLEMENTED);
    assert_eq!(systemless_future::traps::opcode(entry), Some(0xa817));
    assert_eq!(installed.u16(GuestPtr(entry)).unwrap(), 0xa817);
    let r = request();
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), args(r), Permissions::RX).unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa817, c, &m).unwrap(),
        (Call::CopyMask(r), Reply::None)
    );
    assert_eq!(c.dar[15], 0x1018);
    c.dar[15] = 0x1001;
    assert!(Abi::trap(0xa817, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1001);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    for (i, p) in [
        r.source,
        r.mask,
        r.destination,
        r.source_rect,
        r.mask_rect,
        r.destination_rect,
    ]
    .iter()
    .enumerate()
    {
        c.gpr[i + 3] = p.0;
    }
    assert_eq!(
        Abi::import(Service::CopyMask, c, &m).unwrap(),
        (Call::CopyMask(r), Reply::None)
    );
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn masked_pixels_scale_on_both_cpus_preserve_holes_padding_and_live_port_state() {
    let mut app = prepare();
    let r = request();
    let initial = [0x69, 0x5a, 0x69, 0x5a, 0x69, 0x5a, 0x69, 0x5a];
    for dest in [[0, 0, 4, 8], [0, 2, 4, 6], [0, 0, 4, 4]] {
        rect(&mut app, 0x310, dest);
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
            let mut expected = initial;
            for y in 0..4usize {
                for x in dest[1] as usize..dest[3] as usize {
                    let sx = (x - dest[1] as usize) * 8 / (dest[3] - dest[1]) as usize;
                    if ([0xf0u8, 0x0f, 0xaa, 0x55][y] >> (7 - sx)) & 1 != 0 {
                        let bit =
                            (([0xa5u8, 0x5a, 0xf0, 0x0f][y] >> (7 - sx)) & 1) ^ u8::from(swap);
                        expected[2 * y] = (expected[2 * y] & !(1 << (7 - x))) | (bit << (7 - x));
                    }
                }
            }
            for arch in [Architecture::M68k, Architecture::PowerPc] {
                app.runtime
                    .memory
                    .write(GuestPtr(0x1200), &initial)
                    .unwrap();
                run(&mut app, arch, r);
                assert_eq!(
                    app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
                    expected
                );
                assert_eq!(
                    app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
                    state
                );
            }
        }
    }
    // Different mask bitmap origin and an offset mask rectangle select the same bits.
    rect(&mut app, 0x1306, [-10, -20, -6, -12]);
    rect(&mut app, 0x320, [-10, -20, -6, -12]);
    rect(&mut app, 0x310, [0, 0, 4, 8]);
    app.runtime
        .memory
        .write(GuestPtr(0x1050), &33u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1054), &30u32.to_be_bytes())
        .unwrap();
    app.runtime.memory.write(GuestPtr(0x1200), &[0; 8]).unwrap();
    run(&mut app, Architecture::M68k, r);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
        [0xa0, 0, 0x0a, 0, 0xa0, 0, 0x05, 0]
    );
    // Enlarge both axes through a complex live port clip with an interior hole.
    rect(&mut app, 0x1306, [0, 0, 4, 8]);
    rect(&mut app, 0x300, [0, 0, 2, 4]);
    rect(&mut app, 0x320, [0, 0, 2, 4]);
    let handle = app
        .runtime
        .process
        .new_handle(&mut app.runtime.memory, 44, false)
        .unwrap();
    let data = app.runtime.memory.u32(GuestPtr(handle)).unwrap();
    let words = [
        44i16, 0, 0, 4, 8, 0, 0, 8, 32767, 1, 3, 5, 32767, 3, 3, 5, 32767, 4, 0, 8, 32767, 32767,
    ];
    app.runtime
        .memory
        .write(
            GuestPtr(data),
            &words
                .into_iter()
                .flat_map(i16::to_be_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x101c), &handle.to_be_bytes())
        .unwrap();
    let mut expected = initial;
    for y in 0..4 {
        for x in 0..8 {
            if (1..3).contains(&y) && (3..5).contains(&x) {
                continue;
            }
            let sy = y / 2;
            let sx = x / 2;
            if ([0xf0u8, 0x0f][sy] >> (7 - sx)) & 1 != 0 {
                let bit = ([0xa5u8, 0x5a][sy] >> (7 - sx)) & 1;
                expected[y * 2] = (expected[y * 2] & !(1 << (7 - x))) | (bit << (7 - x));
            }
        }
    }
    app.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        app.runtime
            .memory
            .write(GuestPtr(0x1200), &initial)
            .unwrap();
        run(&mut app, arch, r);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
            expected
        );
        assert_eq!(
            app.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
            (-108i16) as u16
        );
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn overlapping_source_and_mask_are_snapshotted_before_writes_and_foreign_targets_ignore_port_clips()
{
    let mut app = prepare();
    let mut r = request();
    r.source = r.destination;
    r.mask = r.destination;
    rect(&mut app, 0x300, [0, 0, 4, 6]);
    rect(&mut app, 0x320, [0, 0, 4, 6]);
    rect(&mut app, 0x310, [0, 2, 4, 8]);
    let initial = [0xa5, 0x5a, 0x5a, 0x5a, 0xf0, 0x5a, 0x0f, 0x5a];
    let mut expected = initial;
    for y in 0..4 {
        for x in 2..8 {
            if (initial[y * 2] >> (9 - x)) & 1 != 0 {
                expected[y * 2] |= 1 << (7 - x);
            }
        }
    }
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        app.runtime
            .memory
            .write(GuestPtr(0x1200), &initial)
            .unwrap();
        run(&mut app, arch, r);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
            expected
        );
    }
    // A separate offscreen destination uses its own bounds and a resized GPU target.
    let mut bytes = 0x1500u32.to_be_bytes().to_vec();
    bytes.extend([0, 2, 0xff, 0xfb, 0xff, 0xf6, 0xff, 0xfd, 0xff, 0xfa]);
    app.runtime.memory.write(GuestPtr(0x1400), &bytes).unwrap();
    rect(&mut app, 0x310, [-5, -10, -3, -6]);
    rect(&mut app, 0x300, [0, 0, 4, 8]);
    rect(&mut app, 0x320, [0, 0, 4, 8]);
    r = request();
    r.destination = GuestPtr(0x1400);
    app.runtime
        .memory
        .write(GuestPtr(0x1500), &[0, 0x5a, 0, 0x5a])
        .unwrap();
    run(&mut app, Architecture::PowerPc, r);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1500), 4).unwrap(),
        [0xc0, 0x5a, 0xc0, 0x5a]
    );
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn malformed_masks_sizes_and_protected_destinations_do_not_publish_pixels() {
    let mut app = prepare();
    let r = request();
    let ports = app.runtime.process.ports.unwrap();
    let gpu = app.runtime.graphics.take().unwrap();
    let initial = [0x69; 8];
    app.runtime
        .memory
        .write(GuestPtr(0x1200), &initial)
        .unwrap();
    rect(&mut app, 0x320, [0, 0, 4, 7]);
    assert!(ports.copy_mask(&mut app.runtime.memory, &gpu, r).is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
        initial
    );
    rect(&mut app, 0x320, [0, 0, 4, 8]);
    app.runtime
        .memory
        .write(GuestPtr(0x1304), &0x8002u16.to_be_bytes())
        .unwrap();
    assert!(ports.copy_mask(&mut app.runtime.memory, &gpu, r).is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
        initial
    );
    app.runtime
        .memory
        .write(GuestPtr(0x1304), &2u16.to_be_bytes())
        .unwrap();
    rect(&mut app, 0x1306, [0, -32768, 4, -32760]);
    rect(&mut app, 0x320, [0, 32759, 4, 32767]);
    assert!(ports.copy_mask(&mut app.runtime.memory, &gpu, r).is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
        initial
    );
    rect(&mut app, 0x1306, [0, 0, 4, 8]);
    rect(&mut app, 0x320, [0, 0, 4, 8]);
    app.runtime
        .memory
        .map(GuestPtr(0x6000), vec![0x55; 8], Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1400), &bitmap(0x6000))
        .unwrap();
    let mut foreign = r;
    foreign.destination = GuestPtr(0x1400);
    assert!(ports
        .copy_mask(&mut app.runtime.memory, &gpu, foreign)
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x6000), 8).unwrap(),
        [0x55; 8]
    );
}
