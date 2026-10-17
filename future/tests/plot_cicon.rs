mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cicon::Op,
    cpu::{Architecture, Call, Cpu},
    display::Display,
    graphics::Graphics,
    load::{resource::ResourceFork, LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
fn ptr(a: &LoadedApplication, h: u32) -> u32 {
    a.runtime.memory.u32(GuestPtr(h)).unwrap()
}
fn write(a: &mut LoadedApplication, p: u32, b: &[u8]) {
    a.runtime.memory.write(GuestPtr(p), b).unwrap();
}
fn rect(a: &mut LoadedApplication, p: u32, r: [i16; 4]) {
    write(
        a,
        p,
        &r.into_iter().flat_map(i16::to_be_bytes).collect::<Vec<_>>(),
    );
}
fn h(a: &mut LoadedApplication, n: u32) -> u32 {
    a.runtime
        .process
        .new_handle(&mut a.runtime.memory, n, true)
        .unwrap()
}
fn ppc(a: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = ptr(a, a.info.toc_or_a5 + 8 + index * 4);
    let pc = ptr(a, tv);
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x7000);
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
fn icon() -> Vec<u8> {
    let mut b = vec![0; 122];
    for (at, v) in [
        (4, 0x8002u16),
        (32, 4),
        (34, 1),
        (36, 4),
        (54, 1),
        (68, 1),
        (92, 2),
        (102, 1),
        (110, 2),
    ] {
        b[at..at + 2].copy_from_slice(&v.to_be_bytes());
    }
    for at in [22, 26] {
        b[at..at + 4].copy_from_slice(&0x00480000u32.to_be_bytes());
    }
    for (base, r) in [
        (0, [-10i16, -20, -8, -17]),
        (50, [10, 20, 12, 23]),
        (64, [4, 5, 6, 8]),
    ] {
        for (i, v) in r.into_iter().enumerate() {
            b[base + 6 + i * 2..base + 8 + i * 2].copy_from_slice(&v.to_be_bytes());
        }
    }
    b[82..86].copy_from_slice(&[0xa0, 0x60, 0x40, 0xa0]);
    b[96..102].fill(0xff);
    b[112..114].fill(0xff);
    b[118..122].copy_from_slice(&[0x01, 0x20, 0x20, 0x10]);
    b
}
fn base() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"InitGraf", b"OpenPort", b"PlotCIcon", b"OpenCPort"],
        24,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None)
        .unwrap()
        .with_display(Display::indexed8(8, 6).unwrap())
        .unwrap();
    a.runtime.process.install_application_resources(
        ResourceFork::parse(&support::resource(vec![(*b"cicn", 128, None, icon())])).unwrap(),
    );
    a.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 0x2000], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xaa, 0x1f], Permissions::RX)
        .unwrap();
    a
}
fn get(a: &mut LoadedApplication) -> u32 {
    let mut m = std::mem::take(&mut a.runtime.process.cicons);
    let h = m
        .call(&mut a.runtime.memory, &mut a.runtime.process, Op::Get(128))
        .unwrap();
    a.runtime.process.cicons = m;
    h
}
fn plot(a: &mut LoadedApplication, arch: Architecture, handle: u32) {
    let sp = STACK_BASE + 0x7000;
    if arch == Architecture::M68k {
        let mut b = handle.to_be_bytes().to_vec();
        b.extend(0x300u32.to_be_bytes());
        b.extend([0xa5; 4]);
        write(a, sp, &b);
        a.runtime.cpu = Cpu::new(arch, 0x50000000, sp);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        for i in 0..15 {
            c.dar[i] = 0x12340000 + i as u32;
        }
    } else {
        let tv = ptr(a, a.info.toc_or_a5 + 16);
        let pc = ptr(a, tv);
        a.runtime.cpu = Cpu::new(arch, pc, sp);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        for i in 2..32 {
            c.gpr[i] = 0x12340000 + i as u32;
        }
        c.gpr[3] = 0x300;
        c.gpr[4] = handle;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            for i in 0..15 {
                assert_eq!(c.dar[i], 0x12340000 + i as u32);
            }
            assert_eq!(c.dar[15], sp + 8);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(sp + 8), 4).unwrap(),
                [0xa5; 4]
            );
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[1], sp);
            for i in 2..32 {
                assert_eq!(
                    c.gpr[i],
                    match i {
                        3 => 0x300,
                        4 => handle,
                        _ => 0x12340000 + i as u32,
                    }
                );
            }
        }
    }
}
fn palette(i: usize) -> [u16; 3] {
    match i % 4 {
        0 => [65535; 3],
        1 => [0; 3],
        2 => [65535, 0, 0],
        _ => [0, 0, 65535],
    }
}
fn set_pixel(b: &mut [u8], row: usize, depth: usize, x: usize, y: usize, v: u8) {
    let bit = x * depth;
    let shift = 8 - depth - bit % 8;
    let mask = ((1u16 << depth) - 1) as u8;
    b[y * row + bit / 8] = (b[y * row + bit / 8] & !(mask << shift)) | (v << shift);
}
fn setup(depth: usize) -> (LoadedApplication, u32, u32, usize) {
    let mut a = base();
    ppc(&mut a, 0, &[0x600]);
    ppc(&mut a, if depth == 1 { 1 } else { 3 }, &[0x1000]);
    let row = (7 * depth).div_ceil(8) + 2;
    let mut b = vec![0; if depth == 1 { 14 } else { 50 }];
    b[..4].copy_from_slice(&0x2000u32.to_be_bytes());
    b[4..6].copy_from_slice(&((row as u16) | if depth == 1 { 0 } else { 0x8000 }).to_be_bytes());
    for (i, v) in [-2i16, -3, 3, 4].into_iter().enumerate() {
        b[6 + i * 2..8 + i * 2].copy_from_slice(&v.to_be_bytes());
    }
    if depth == 1 {
        write(&mut a, 0x1002, &b);
    } else {
        for (at, v) in [(32, depth as u16), (34, 1), (36, depth as u16)] {
            b[at..at + 2].copy_from_slice(&v.to_be_bytes());
        }
        let ct = h(&mut a, (8 + (1 << depth) * 8) as u32);
        let mut table = vec![0; 8 + (1 << depth) * 8];
        table[6..8].copy_from_slice(&((1u16 << depth) - 1).to_be_bytes());
        for i in 0..1 << depth {
            table[8 + i * 8..10 + i * 8].copy_from_slice(&(i as u16).to_be_bytes());
            for (j, v) in palette(i).into_iter().enumerate() {
                table[10 + i * 8 + j * 2..12 + i * 8 + j * 2].copy_from_slice(&v.to_be_bytes());
            }
        }
        let data = ptr(&a, ct);
        write(&mut a, data, &table);
        b[42..46].copy_from_slice(&ct.to_be_bytes());
        let pm = ptr(&a, ptr(&a, 0x1002));
        write(&mut a, pm, &b);
    }
    rect(&mut a, 0x1010, [-2, -3, 3, 4]);
    let vis = ptr(&a, 0x1018);
    let vp = ptr(&a, vis);
    write(&mut a, vp, &10u16.to_be_bytes());
    rect(&mut a, vp + 2, [-1, -3, 3, 4]);
    let clip = ptr(&a, 0x101c);
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, clip, 36)
        .unwrap();
    let words: [i16; 18] = [
        36, -2, -3, 3, 4, -2, -3, 0, 1, 4, 32767, 3, -3, 0, 1, 4, 32767, 32767,
    ];
    let cp = ptr(&a, clip);
    write(
        &mut a,
        cp,
        &words
            .into_iter()
            .flat_map(i16::to_be_bytes)
            .collect::<Vec<_>>(),
    );
    // These routines must not be called or recorded by PlotCIcon.
    for at in [92, 96, 100, 104] {
        write(&mut a, 0x1000 + at, &0xdeadbeefu32.to_be_bytes());
    }
    let gd = h(&mut a, 62);
    let map = h(&mut a, 50);
    let gp = ptr(&a, gd);
    write(&mut a, gp + 22, &map.to_be_bytes());
    write(&mut a, 0xcc8, &gd.to_be_bytes());
    let device_map = ptr(&a, map);
    let own = get(&mut a);
    let raw = a
        .runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"cicn", 128)
        .unwrap();
    // A live icon uses iconData; a compiled resource ignores pointer placeholders.
    let root = ptr(&a, own);
    write(&mut a, root, &0xdeadbeefu32.to_be_bytes());
    let rp = ptr(&a, raw);
    for at in [0, 42, 50, 64, 78] {
        write(&mut a, rp + at, &0xdeadbeefu32.to_be_bytes());
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    a.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 7, 5, GuestPtr(0x2000))).unwrap());
    (a, own, device_map, row)
}
#[test]
fn abi_frames_validate_all_arguments_and_install_a_readable_plot_veneer() {
    let mut m = Memory::default();
    systemless_future::traps::install(&mut m).unwrap();
    let entry = systemless_future::traps::lookup(&m, 0xaa1f, true).unwrap();
    assert_ne!(entry, systemless_future::traps::UNIMPLEMENTED);
    assert_eq!(m.u16(GuestPtr(entry)).unwrap(), 0xaa1f);
    m.map(
        GuestPtr(0x1000),
        [0x11223344u32.to_be_bytes(), 0x55667788u32.to_be_bytes()].concat(),
        Permissions::RX,
    )
    .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xaa1f, c, &m).unwrap(),
        (
            Call::PlotCIcon {
                handle: 0x11223344,
                destination: GuestPtr(0x55667788)
            },
            Reply::None
        )
    );
    assert_eq!(c.dar[15], 0x1008);
    c.dar[15] = 0x1001;
    assert!(Abi::trap(0xaa1f, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1001);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0x55667788;
    c.gpr[4] = 0x11223344;
    assert_eq!(
        Abi::import(Service::PlotCIcon, c, &m).unwrap(),
        (
            Call::PlotCIcon {
                handle: 0x11223344,
                destination: GuestPtr(0x55667788)
            },
            Reply::None
        )
    );
}
#[test]
fn nil_arguments_are_inert_without_graphics() {
    let mut a = base();
    plot(&mut a, Architecture::M68k, 0);
    plot(&mut a, Architecture::PowerPc, 0);
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_icons_cover_both_cpus_raw_and_owned_sources_depth_selection_scaling_clips_and_colors() {
    for depth in [1usize, 2, 4, 8] {
        let (mut a, own, dm, row) = setup(depth);
        let raw = a
            .runtime
            .process
            .get_resource(&mut a.runtime.memory, *b"cicn", 128)
            .unwrap();
        for device_depth in [1u16, 2, 8] {
            write(&mut a, dm + 32, &device_depth.to_be_bytes());
            for dest in [[-2i16, -3, 3, 4], [-3, -4, 4, 5], [-1, -2, 1, 0]] {
                rect(&mut a, 0x300, dest);
                for (fg, bg) in if depth == 1 {
                    vec![(1, 0), (0, 1)]
                } else {
                    vec![(1, 0), (0, 1), (2, 3)]
                } {
                    write(
                        &mut a,
                        0x1050,
                        &(if depth == 1 {
                            if fg == 1 {
                                33u32
                            } else {
                                30
                            }
                        } else {
                            fg as u32
                        })
                        .to_be_bytes(),
                    );
                    write(
                        &mut a,
                        0x1054,
                        &(if depth == 1 {
                            if bg == 0 {
                                30u32
                            } else {
                                33
                            }
                        } else {
                            bg as u32
                        })
                        .to_be_bytes(),
                    );
                    let state = a
                        .runtime
                        .memory
                        .read(GuestPtr(0x1000), 108)
                        .unwrap()
                        .to_vec();
                    let mut initial = vec![0x5a; row * 5];
                    for y in 0..5 {
                        for x in 0..7 {
                            set_pixel(
                                &mut initial,
                                row,
                                depth,
                                x,
                                y,
                                if depth == 1 { 1 } else { 3 },
                            );
                        }
                    }
                    let mut expected = initial.clone();
                    for y in -2..3 {
                        for x in -3..4 {
                            if y < -1
                                || x == 0
                                || y < i32::from(dest[0])
                                || y >= i32::from(dest[2])
                                || x < i32::from(dest[1])
                                || x >= i32::from(dest[3])
                            {
                                continue;
                            }
                            let sx = ((x - i32::from(dest[1])) * 3 / (i32::from(dest[3] - dest[1])))
                                as usize;
                            let sy = ((y - i32::from(dest[0])) * 2 / (i32::from(dest[2] - dest[0])))
                                as usize;
                            if [1, 0, 1, 0, 1, 1][sy * 3 + sx] == 0 {
                                continue;
                            }
                            let rgb = if device_depth <= 2 {
                                if [0, 1, 0, 1, 0, 1][sy * 3 + sx] != 0 {
                                    palette(fg)
                                } else {
                                    palette(bg)
                                }
                            } else {
                                let src = palette([0, 1, 2, 2, 0, 1][sy * 3 + sx]);
                                std::array::from_fn(|j| {
                                    ((u64::from(src[j]) * u64::from(palette(bg)[j])
                                        + (65535 - u64::from(src[j])) * u64::from(palette(fg)[j])
                                        + 32767)
                                        / 65535) as u16
                                })
                            };
                            let best = (0..if depth == 1 { 2 } else { 1 << depth })
                                .min_by_key(|&i| {
                                    palette(i)
                                        .into_iter()
                                        .zip(rgb)
                                        .map(|(a, b)| {
                                            let d = i64::from(a) - i64::from(b);
                                            (d * d) as u64
                                        })
                                        .sum::<u64>()
                                })
                                .unwrap();
                            set_pixel(
                                &mut expected,
                                row,
                                depth,
                                (x + 3) as usize,
                                (y + 2) as usize,
                                best as u8,
                            );
                        }
                    }
                    for handle in [own, raw] {
                        for arch in [Architecture::M68k, Architecture::PowerPc] {
                            write(&mut a, 0x2000, &initial);
                            a.runtime.process.mem_error = -108;
                            a.runtime.process.res_error = -192;
                            plot(&mut a, arch, handle);
                            assert_eq!(a.runtime.memory.read(GuestPtr(0x2000),row*5).unwrap(),expected,"depth{depth} device{device_depth} dest{dest:?} fg{fg} bg{bg} handle{handle:x} arch{arch:?}");
                            assert_eq!(
                                a.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
                                state
                            );
                            assert_eq!(a.runtime.process.mem_error, -108);
                            assert_eq!(a.runtime.process.res_error, -192);
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn missing_mono_falls_back_to_live_color_data_and_edits_are_visible_on_the_next_draw() {
    let (mut a, own, dm, row) = setup(8);
    write(&mut a, dm + 32, &1u16.to_be_bytes());
    let raw = a
        .runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"cicn", 128)
        .unwrap();
    let mut compiled = icon();
    compiled[68..70].fill(0);
    compiled.drain(84..86);
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, raw, 120)
        .unwrap();
    let rp = ptr(&a, raw);
    write(&mut a, rp, &compiled);
    let root = ptr(&a, own);
    write(&mut a, root + 68, &0u16.to_be_bytes());
    write(&mut a, 0x1050, &1u32.to_be_bytes());
    write(&mut a, 0x1054, &0u32.to_be_bytes());
    rect(&mut a, 0x300, [-1, -3, 1, 0]);
    let mut initial = vec![0x5a; row * 5];
    for y in 0..5 {
        for x in 0..7 {
            set_pixel(&mut initial, row, 8, x, y, 3);
        }
    }
    for edited in [false, true] {
        if edited {
            let image = ptr(&a, root + 78);
            let ip = ptr(&a, image);
            write(&mut a, ip, &[0x21, 0, 0, 0x20]);
            let table = ptr(&a, root + 42);
            let tp = ptr(&a, table);
            write(&mut a, tp + 26, &[0, 0, 0, 0, 0xff, 0xff]);
            write(&mut a, rp + 116, &[0x21, 0, 0, 0x20]);
            write(&mut a, rp + 110, &[0, 0, 0, 0, 0xff, 0xff]);
        }
        let mut expected = initial.clone();
        for (i, v) in if edited {
            [3, 3, 0, 3, 0, 3]
        } else {
            [0, 3, 2, 3, 0, 1]
        }
        .into_iter()
        .enumerate()
        {
            set_pixel(&mut expected, row, 8, i % 3, 1 + i / 3, v);
        }
        for handle in [own, raw] {
            for arch in [Architecture::M68k, Architecture::PowerPc] {
                write(&mut a, 0x2000, &initial);
                let source = a.runtime.memory.read(GuestPtr(rp), 120).unwrap().to_vec();
                plot(&mut a, arch, handle);
                assert_eq!(
                    a.runtime.memory.read(GuestPtr(0x2000), row * 5).unwrap(),
                    expected,
                    "edited{edited} arch{arch:?} handle{handle:x}"
                );
                assert_eq!(a.runtime.memory.read(GuestPtr(rp), 120).unwrap(), source);
            }
        }
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn malformed_live_sources_fault_before_destination_changes_and_empty_rectangles_are_inert() {
    let (mut a, own, dm, row) = setup(8);
    write(&mut a, dm + 32, &8u16.to_be_bytes());
    rect(&mut a, 0x300, [-1, -3, 1, 0]);
    let initial = vec![0x5a; row * 5];
    write(&mut a, 0x2000, &initial);
    let root = ptr(&a, own);
    let record = a.runtime.memory.read(GuestPtr(root), 82).unwrap().to_vec();
    let free = a.runtime.process.free_memory();
    for (at, bytes) in [
        (50, 0xdeadbeefu32.to_be_bytes().to_vec()),
        (78, 0xdeadbeefu32.to_be_bytes().to_vec()),
        (12, (-20i16).to_be_bytes().to_vec()),
    ] {
        write(&mut a, root + at, &bytes);
        a.runtime.process.mem_error = -108;
        let ports = a.runtime.process.ports.unwrap();
        let gpu = a.runtime.graphics.as_ref().unwrap();
        assert!(systemless_future::cicon::plot(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            ports,
            gpu,
            own,
            GuestPtr(0x300)
        )
        .is_err());
        assert_eq!(
            a.runtime.memory.read(GuestPtr(0x2000), row * 5).unwrap(),
            initial
        );
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(a.runtime.process.mem_error, -108);
        write(&mut a, root, &record);
    }
    rect(&mut a, 0x300, [0, 0, 0, 0]);
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        plot(&mut a, arch, 0xdeadbeef);
    }
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x2000), row * 5).unwrap(),
        initial
    );
}
