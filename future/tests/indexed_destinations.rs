mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    graphics::{Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const PORT: u32 = 0x50000000;
const BITS: u32 = 0x52000001;
const SOURCE: u32 = 0x53000000;
fn invoke(a: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
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
fn native(a: &mut LoadedApplication, offset: u32, args: &[u8]) {
    let sp = STACK_BASE + 0x8000;
    let mut f = args.to_vec();
    f.extend([0x5a; 4]);
    a.runtime.memory.write(GuestPtr(sp), &f).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x51000000 + offset, sp);
    if let Cpu::M68k(c) = &mut a.runtime.cpu {
        c.dar[0] = 0x12345678;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], sp + args.len() as u32);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(sp + args.len() as u32), 4)
            .unwrap(),
        [0x5a; 4]
    );
}
fn rectangle(a: &mut LoadedApplication, at: u32, r: Rect) {
    let b: Vec<_> = [r.top, r.left, r.bottom, r.right]
        .into_iter()
        .flat_map(|v| (v as i16).to_be_bytes())
        .collect();
    a.runtime.memory.write(GuestPtr(at), &b).unwrap();
}
fn palette(depth: u16) -> Vec<[u16; 3]> {
    let n = 1usize << depth;
    (0..n)
        .map(|i| match i {
            0 => [65535; 3],
            1 => [65535, 0, 0],
            2 => [0, 65535, 0],
            3 if depth == 2 => [0; 3],
            3 => [0, 0, 65535],
            4 => [0, 65535, 65535],
            5 => [65535, 0, 65535],
            6 => [65535, 65535, 0],
            7 => [0; 3],
            _ => [65535 - ((i * 65535) / (n - 1)) as u16; 3],
        })
        .collect()
}
fn nearest(rgb: [u16; 3], colors: &[[u16; 3]]) -> u8 {
    colors
        .iter()
        .enumerate()
        .min_by_key(|(i, c)| {
            let distance = (0..3)
                .map(|k| {
                    let d = i128::from(rgb[k]) - i128::from(c[k]);
                    d * d
                })
                .sum::<i128>();
            (distance, *i)
        })
        .unwrap()
        .0 as u8
}
fn packed(indices: &[u8], depth: u16, stride: usize) -> Vec<u8> {
    let mut out = vec![0xa5; stride * 3];
    let depth = usize::from(depth);
    let mask = ((1u16 << depth) - 1) as u8;
    for y in 0..3 {
        for x in 0..7 {
            let bit = x * depth;
            let s = 8 - depth - bit % 8;
            let p = &mut out[y * stride + bit / 8];
            *p = (*p & !(mask << s)) | (indices[y * 7 + x] << s);
        }
    }
    out
}
fn unpack(a: &LoadedApplication, depth: u16, stride: usize) -> Vec<u8> {
    let bytes = a.runtime.memory.read(GuestPtr(BITS), stride * 3).unwrap();
    (0..21)
        .map(|i| {
            let bit = (i % 7) * usize::from(depth);
            (bytes[(i / 7) * stride + bit / 8] >> (8 - usize::from(depth) - bit % 8))
                & ((1u16 << depth) - 1) as u8
        })
        .collect()
}
fn check_gpu(a: &LoadedApplication, indices: &[u8], colors: &[[u16; 3]]) {
    let pixels = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
    for (i, &rgba) in pixels.as_chunks::<4>().0.iter().enumerate() {
        let c = colors[usize::from(indices[i])];
        assert_eq!(
            rgba,
            [(c[0] >> 8) as u8, (c[1] >> 8) as u8, (c[2] >> 8) as u8, 255],
            "pixel {i}"
        );
    }
}
fn rgb(a: &mut LoadedApplication, color: [u16; 3], background: bool, native_call: bool) {
    let at = PORT + 192;
    let b: Vec<_> = color.into_iter().flat_map(u16::to_be_bytes).collect();
    a.runtime.memory.write(GuestPtr(at), &b).unwrap();
    if native_call {
        native(a, if background { 2 } else { 0 }, &at.to_be_bytes());
    } else {
        assert_eq!(invoke(a, if background { 5 } else { 4 }, &[at]), at);
    }
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(PORT + if background { 42 } else { 36 }), 6)
            .unwrap(),
        b
    );
}
fn setup(depth: u16) -> (LoadedApplication, u32, usize, Vec<[u16; 3]>) {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"OpenCPort",
            b"PaintRect",
            b"CopyBits",
            b"RGBForeColor",
            b"RGBBackColor",
            b"PenMode",
            b"SetGDevice",
            b"DrawChar",
        ],
        64,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(PORT), vec![0; 1024], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xaa, 0x14, 0xaa, 0x15, 0xa8, 0xa2, 0xa8, 0xec, 0xa8, 0x83],
            Permissions::RX,
        )
        .unwrap();
    invoke(&mut a, 0, &[0x600]);
    invoke(&mut a, 1, &[PORT]);
    let h = a.runtime.memory.u32(GuestPtr(PORT + 2)).unwrap();
    let map = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    let mut bytes = a.runtime.memory.read(GuestPtr(map), 50).unwrap().to_vec();
    let stride = if depth > 8 {
        7 * usize::from(depth / 8) + 4
    } else {
        (7 * usize::from(depth)).div_ceil(8) + 2
    };
    support::w32(&mut bytes, 0, BITS);
    support::w16(&mut bytes, 4, 0x8000 | stride as u16);
    for (i, v) in [-2i16, -3, 1, 4].into_iter().enumerate() {
        support::w16(&mut bytes, 6 + i * 2, v as u16);
    }
    for (at, v) in [(32, depth), (34, 1), (36, depth)] {
        support::w16(&mut bytes, at, v);
    }
    if depth > 8 {
        for (at, v) in [(30, 16), (34, 3), (36, if depth == 16 { 5 } else { 8 })] {
            support::w16(&mut bytes, at, v);
        }
    }
    let colors = if depth > 8 {
        vec![[0; 3], [65535; 3]]
    } else {
        palette(depth)
    };
    let mut table = vec![0; 8 + colors.len() * 8];
    support::w16(&mut table, 4, 0x8000);
    support::w16(&mut table, 6, colors.len() as u16 - 1);
    for (i, c) in colors.iter().enumerate() {
        support::w16(&mut table, 8 + i * 8, i as u16);
        for (k, value) in c.iter().enumerate() {
            support::w16(&mut table, 10 + i * 8 + 2 * k, *value);
        }
    }
    let ct = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, table.len() as u32, false)
        .unwrap();
    let data = a.runtime.memory.u32(GuestPtr(ct)).unwrap();
    a.runtime.memory.write(GuestPtr(data), &table).unwrap();
    support::w32(&mut bytes, 42, ct);
    a.runtime.memory.write(GuestPtr(map), &bytes).unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(BITS - 1),
            vec![0xa5; stride * 3 + 2],
            Permissions::RW,
        )
        .unwrap();
    if depth <= 8 {
        a.runtime
            .memory
            .write(GuestPtr(BITS), &packed(&[0; 21], depth, stride))
            .unwrap();
    }
    let bounds = Rect::new(-3, -2, 4, 1);
    rectangle(&mut a, PORT + 16, bounds);
    for at in [24, 28] {
        let h = a.runtime.memory.u32(GuestPtr(PORT + at)).unwrap();
        let ptr = a.runtime.memory.u32(GuestPtr(h)).unwrap();
        let mut r = 10u16.to_be_bytes().to_vec();
        for v in [-2i16, -3, 1, 4] {
            r.extend(v.to_be_bytes());
        }
        a.runtime.memory.write(GuestPtr(ptr), &r).unwrap();
    }
    // A real offscreen GDevice supplies the same target table to RGB matching.
    let main = a
        .runtime
        .memory
        .u32(systemless_future::devices::MAIN)
        .unwrap();
    let record = a.runtime.memory.u32(GuestPtr(main)).unwrap();
    let mut device = a
        .runtime
        .memory
        .read(GuestPtr(record), 62)
        .unwrap()
        .to_vec();
    support::w32(&mut device, 22, h);
    support::w16(&mut device, 20, 0x4001);
    let offscreen = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 62, false)
        .unwrap();
    let record = a.runtime.memory.u32(GuestPtr(offscreen)).unwrap();
    a.runtime.memory.write(GuestPtr(record), &device).unwrap();
    invoke(&mut a, 7, &[offscreen]);
    assert_eq!(
        a.runtime
            .memory
            .u32(systemless_future::devices::MAIN)
            .unwrap(),
        main
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    a.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 7, 3, GuestPtr(0x4000))).unwrap());
    (a, map, stride, colors)
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_cpus_paint_real_packed_color_pixels_and_gpu_reads_cpu_changes_with_padding_preserved() {
    for depth in [2u16, 4, 8] {
        let (mut a, _, stride, colors) = setup(depth);
        rgb(&mut a, colors[2], false, true);
        rgb(&mut a, colors[0], true, false);
        assert_eq!(a.runtime.memory.u32(GuestPtr(PORT + 80)).unwrap(), 2);
        let r = Rect::new(-2, -1, 2, 1);
        rectangle(&mut a, PORT + 128, r);
        assert_eq!(invoke(&mut a, 2, &[PORT + 128]), PORT + 128);
        let mut expected = vec![0u8; 21];
        for y in 1..3 {
            for x in 1..5 {
                expected[y * 7 + x] = 2;
            }
        }
        assert_eq!(unpack(&a, depth, stride), expected);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(BITS), stride * 3).unwrap(),
            packed(&expected, depth, stride)
        );
        check_gpu(&a, &expected, &colors);
        // A direct guest write remains authoritative across the following GPU draw.
        expected[0] = 1;
        a.runtime
            .memory
            .write(GuestPtr(BITS), &packed(&expected, depth, stride))
            .unwrap();
        native(&mut a, 4, &(PORT + 128).to_be_bytes());
        check_gpu(&a, &expected, &colors);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(BITS - 1), 1).unwrap(),
            [0xa5]
        );
        assert_eq!(
            a.runtime
                .memory
                .read(GuestPtr(BITS + stride as u32 * 3), 1)
                .unwrap(),
            [0xa5]
        );
    }
}
fn source(a: &mut LoadedApplication) -> Vec<[u16; 3]> {
    let entries = [
        (7, [0u16, 65535, 0]),
        (5, [65535; 3]),
        (9, [32768, 0, 65535]),
        (8, [0; 3]),
        (6, [65535, 0, 0]),
    ];
    let mut table = vec![0; 8 + entries.len() * 8];
    support::w16(&mut table, 6, entries.len() as u16 - 1);
    for (i, (index, c)) in entries.iter().enumerate() {
        support::w16(&mut table, 8 + i * 8, *index);
        for (k, value) in c.iter().enumerate() {
            support::w16(&mut table, 10 + i * 8 + 2 * k, *value);
        }
    }
    let mut m = vec![0; 128];
    support::w32(&mut m, 0, 0x54000010);
    m[16..16 + table.len()].copy_from_slice(&table);
    a.runtime
        .memory
        .map(GuestPtr(0x54000000), m, Permissions::RX)
        .unwrap();
    let mut header = vec![0; 50];
    support::w32(&mut header, 0, SOURCE);
    support::w16(&mut header, 4, 0x8009);
    for (i, v) in [-2i16, -3, 1, 4].into_iter().enumerate() {
        support::w16(&mut header, 6 + 2 * i, v as u16);
    }
    for (at, v) in [(32, 8), (34, 1), (36, 8)] {
        support::w16(&mut header, at, v);
    }
    support::w32(&mut header, 42, 0x54000000);
    a.runtime
        .memory
        .write(GuestPtr(PORT + 256), &header)
        .unwrap();
    let mut pixels = vec![0xa5; 27];
    let mut rgb = Vec::new();
    for i in 0..21 {
        let e = entries[i % 5];
        pixels[(i / 7) * 9 + i % 7] = e.0 as u8;
        rgb.push(e.1);
    }
    a.runtime
        .memory
        .map(GuestPtr(SOURCE), pixels, Permissions::RX)
        .unwrap();
    rgb
}
fn copy(a: &mut LoadedApplication, map: u32, mode: u16, native_call: bool) {
    let src = PORT + 256;
    let r = PORT + 128;
    rectangle(a, r, Rect::new(-3, -2, 4, 1));
    if native_call {
        let mut f = 0u32.to_be_bytes().to_vec();
        f.extend(mode.to_be_bytes());
        f.extend(r.to_be_bytes());
        f.extend(r.to_be_bytes());
        f.extend(map.to_be_bytes());
        f.extend(src.to_be_bytes());
        native(a, 6, &f);
    } else {
        assert_eq!(invoke(a, 3, &[src, map, r, r, u32::from(mode), 0]), src);
    }
}
fn mix(s: [u16; 3], fg: [u16; 3], bg: [u16; 3]) -> [u16; 3] {
    std::array::from_fn(|i| {
        ((u64::from(s[i]) * u64::from(bg[i]) + u64::from(65535 - s[i]) * u64::from(fg[i]) + 32767)
            / 65535) as u16
    })
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn color_copy_modes_match_rgb_semantics_on_both_cpus_instead_of_bitwise_palette_indices() {
    for depth in [2u16, 4, 8] {
        let (mut a, map, stride, colors) = setup(depth);
        let source = source(&mut a);
        rgb(&mut a, [0; 3], false, true);
        rgb(&mut a, [65535; 3], true, false);
        let original: Vec<_> = (0..21).map(|i| if i % 2 == 0 { 1 } else { 2 }).collect();
        for mode in 0..8u16 {
            a.runtime
                .memory
                .write(GuestPtr(BITS), &packed(&original, depth, stride))
                .unwrap();
            copy(&mut a, map, mode, mode % 2 == 0);
            let expected: Vec<_> = source
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let s = if mode & 4 != 0 {
                        c.map(|v| 65535 - v)
                    } else {
                        *c
                    };
                    let d = colors[usize::from(original[i])];
                    let out = match mode & 3 {
                        0 => s,
                        1 => mix(s, [0; 3], d),
                        2 => {
                            if s == [0; 3] {
                                d.map(|v| 65535 - v)
                            } else {
                                d
                            }
                        }
                        3 => mix(s, [65535; 3], d),
                        _ => unreachable!(),
                    };
                    nearest(out, &colors)
                })
                .collect();
            assert_eq!(
                unpack(&a, depth, stride),
                expected,
                "depth {depth} mode {mode}"
            );
            check_gpu(&a, &expected, &colors);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(BITS), stride * 3).unwrap(),
                packed(&expected, depth, stride)
            );
        }
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn overlap_preserves_duplicate_indices_live_palettes_and_complex_clips() {
    let (mut a, map, stride, mut colors) = setup(8);
    rgb(&mut a, [0; 3], false, true);
    rgb(&mut a, [65535; 3], true, false);
    let old: Vec<_> = (0..21).map(|i| [255u8, 1, 2, 7, 3, 4, 0][i % 7]).collect();
    a.runtime
        .memory
        .write(GuestPtr(BITS), &packed(&old, 8, stride))
        .unwrap();
    rectangle(&mut a, PORT + 128, Rect::new(-3, -2, 3, 1));
    rectangle(&mut a, PORT + 136, Rect::new(-2, -2, 4, 1));
    assert_eq!(
        invoke(&mut a, 3, &[map, map, PORT + 128, PORT + 136, 0, 0]),
        map
    );
    let mut expected = old.clone();
    for y in 0..3 {
        for x in 1..7 {
            expected[y * 7 + x] = old[y * 7 + x - 1];
        }
    }
    assert_eq!(unpack(&a, 8, stride), expected);
    assert_eq!(expected[1], 255);
    check_gpu(&a, &expected, &colors);
    let table_h = a.runtime.memory.u32(GuestPtr(map + 42)).unwrap();
    let table = a.runtime.memory.u32(GuestPtr(table_h)).unwrap();
    colors[1] = [0, 0, 65535];
    let bytes: Vec<_> = colors[1].into_iter().flat_map(u16::to_be_bytes).collect();
    a.runtime
        .memory
        .write(GuestPtr(table + 18), &bytes)
        .unwrap();
    let before = a
        .runtime
        .memory
        .read(GuestPtr(BITS), stride * 3)
        .unwrap()
        .to_vec();
    rectangle(&mut a, PORT + 128, Rect::new(-3, -2, 4, 1));
    invoke(&mut a, 3, &[map, map, PORT + 128, PORT + 128, 0, 0]);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BITS), stride * 3).unwrap(),
        before
    );
    check_gpu(&a, &expected, &colors);
    let clip_h = a.runtime.memory.u32(GuestPtr(PORT + 28)).unwrap();
    let clip = support::words(&[
        36, 0xfffe, 0xfffd, 1, 4, 0xfffe, 0xfffd, 0xffff, 0, 4, 0x7fff, 1, 0xfffd, 0xffff, 0, 4,
        0x7fff, 0x7fff,
    ]);
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, clip_h, clip.len() as u32)
        .unwrap();
    let data = a.runtime.memory.u32(GuestPtr(clip_h)).unwrap();
    a.runtime.memory.write(GuestPtr(data), &clip).unwrap();
    rgb(&mut a, colors[2], false, false);
    native(&mut a, 4, &(PORT + 128).to_be_bytes());
    for y in 0..3 {
        for x in 0..7 {
            if x != 2 {
                expected[y * 7 + x] = 2;
            }
        }
    }
    assert_eq!(unpack(&a, 8, stride), expected);
    check_gpu(&a, &expected, &colors);
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn sparse_palette_matching_and_readonly_destinations_are_explicit_without_partial_writes() {
    let (mut a, map, stride, _) = setup(4);
    let mut table = vec![0; 32];
    support::w16(&mut table, 6, 2);
    for (i, (pixel, c)) in [
        (0u16, [65535u16; 3]),
        (3, [65535, 0, 0]),
        (9, [0, 65535, 0]),
    ]
    .into_iter()
    .enumerate()
    {
        support::w16(&mut table, 8 + i * 8, pixel);
        for (k, value) in c.iter().enumerate() {
            support::w16(&mut table, 10 + i * 8 + 2 * k, *value);
        }
    }
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 32, false)
        .unwrap();
    let p = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(p), &table).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(map + 42), &h.to_be_bytes())
        .unwrap();
    rgb(&mut a, [0, 65535, 0], false, true);
    rgb(&mut a, [65535; 3], true, false);
    assert_eq!(a.runtime.memory.u32(GuestPtr(PORT + 80)).unwrap(), 9);
    rectangle(&mut a, PORT + 128, Rect::new(-3, -2, 4, 1));
    invoke(&mut a, 2, &[PORT + 128]);
    assert_eq!(unpack(&a, 4, stride), [9; 21]);
    let before = a
        .runtime
        .memory
        .read(GuestPtr(BITS), stride * 3)
        .unwrap()
        .to_vec();
    let texture = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x55000000), before.clone(), Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(map), &0x55000000u32.to_be_bytes())
        .unwrap();
    let ports = a.runtime.process.ports.unwrap();
    assert!(ports
        .paint_rect(
            a.runtime.graphics.as_ref().unwrap(),
            &mut a.runtime.memory,
            Rect::new(-3, -2, 4, 1)
        )
        .is_err());
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(0x55000000), stride * 3)
            .unwrap(),
        before
    );
    assert_eq!(
        a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap(),
        texture
    );
    a.runtime
        .memory
        .write(GuestPtr(map), &BITS.to_be_bytes())
        .unwrap();
    let mut bad = before.clone();
    bad[0] = 0x19;
    a.runtime.memory.write(GuestPtr(BITS), &bad).unwrap();
    assert!(ports
        .paint_rect(
            a.runtime.graphics.as_ref().unwrap(),
            &mut a.runtime.memory,
            Rect::new(-3, -2, 4, 1)
        )
        .unwrap_err()
        .to_string()
        .contains("undefined"));
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BITS), stride * 3).unwrap(),
        bad
    );
    assert_eq!(
        a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap(),
        texture
    );
}
#[test]
fn rgb_native_frames_check_full_color_input_before_stack_changes() {
    use systemless_future::{abi::Abi, memory::Memory};
    for opcode in [0xaa14, 0xaa15] {
        let mut m = Memory::default();
        m.map(
            GuestPtr(0x1000),
            0x2000u32.to_be_bytes().to_vec(),
            Permissions::RW,
        )
        .unwrap();
        m.map(GuestPtr(0x2000), vec![0; 5], Permissions::RX)
            .unwrap();
        let mut c = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut c else { panic!() };
        assert!(Abi::trap(opcode, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn indexed_pattern_modes_apply_foreground_background_and_color_inversion() {
    for depth in [2u16, 4, 8] {
        let (mut a, _, stride, colors) = setup(depth);
        rgb(&mut a, colors[1], false, true);
        rgb(&mut a, colors[2], true, false);
        let h = a.runtime.memory.u32(GuestPtr(PORT + 58)).unwrap();
        let ptr = a.runtime.memory.u32(GuestPtr(h)).unwrap();
        let pattern = [0xaau8, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55];
        a.runtime
            .memory
            .write(GuestPtr(ptr + 20), &pattern)
            .unwrap();
        rectangle(&mut a, PORT + 128, Rect::new(-3, -2, 4, 1));
        let original: Vec<_> = (0..21).map(|i| if i % 3 == 0 { 0 } else { 2 }).collect();
        for mode in 0..8 {
            a.runtime
                .memory
                .write(GuestPtr(BITS), &packed(&original, depth, stride))
                .unwrap();
            invoke(&mut a, 6, &[8 + mode]);
            native(&mut a, 4, &(PORT + 128).to_be_bytes());
            let expected: Vec<_> = (0..21)
                .map(|i| {
                    let mut on = pattern[i / 7] & (0x80 >> (i % 7)) != 0;
                    if mode & 4 != 0 {
                        on = !on;
                    }
                    match mode & 3 {
                        0 => {
                            if on {
                                1
                            } else {
                                2
                            }
                        }
                        1 => {
                            if on {
                                1
                            } else {
                                original[i]
                            }
                        }
                        2 => {
                            if on {
                                nearest(
                                    colors[usize::from(original[i])].map(|v| 65535 - v),
                                    &colors,
                                )
                            } else {
                                original[i]
                            }
                        }
                        3 => {
                            if on {
                                2
                            } else {
                                original[i]
                            }
                        }
                        _ => unreachable!(),
                    }
                })
                .collect();
            assert_eq!(
                unpack(&a, depth, stride),
                expected,
                "depth {depth} pattern mode {mode}"
            );
            check_gpu(&a, &expected, &colors);
        }
        assert_eq!(
            a.runtime.memory.read(GuestPtr(ptr + 20), 8).unwrap(),
            pattern
        );
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn rgb_device_mismatch_fails_without_changing_the_port_or_switching_devices() {
    let (mut a, _, _, _) = setup(8);
    let main = a
        .runtime
        .memory
        .u32(systemless_future::devices::MAIN)
        .unwrap();
    invoke(&mut a, 7, &[main]);
    let before = a.runtime.memory.read(GuestPtr(PORT), 108).unwrap().to_vec();
    a.runtime
        .memory
        .write(GuestPtr(PORT + 192), &[0, 0, 255, 255, 0, 0])
        .unwrap();
    let ports = a.runtime.process.ports.unwrap();
    assert!(ports
        .set_rgb(&mut a.runtime.memory, false, GuestPtr(PORT + 192))
        .unwrap_err()
        .to_string()
        .contains("matching current-device"));
    assert_eq!(a.runtime.memory.read(GuestPtr(PORT), 108).unwrap(), before);
    assert_eq!(
        a.runtime
            .memory
            .u32(systemless_future::devices::CURRENT)
            .unwrap(),
        main
    );
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_text_masks_match_copybits_for_all_indexed_depths_modes_clips_and_padding() {
    use systemless_future::quickdraw::CopyBits;
    for depth in [2u16, 4, 8] {
        let (mut a, map, stride, colors) = setup(depth);
        rgb(&mut a, colors[1], false, true);
        rgb(&mut a, colors[2], true, false);
        let ports = a.runtime.process.ports.unwrap();
        ports
            .clip_rect(&mut a.runtime.memory, Rect::new(-2, -1, 3, 1))
            .unwrap();
        let source = [0xaau8, 0x5a, 0x54, 0xa5, 0xf0, 0xcc];
        a.runtime
            .memory
            .map(GuestPtr(SOURCE), source.to_vec(), Permissions::RX)
            .unwrap();
        let mut header = SOURCE.to_be_bytes().to_vec();
        header.extend(2u16.to_be_bytes());
        for value in [0i16, 0, 3, 7] {
            header.extend(value.to_be_bytes());
        }
        a.runtime
            .memory
            .write(GuestPtr(PORT + 256), &header)
            .unwrap();
        rectangle(&mut a, PORT + 272, Rect::new(0, 0, 7, 3));
        let destination = Rect::new(-3, -2, 4, 1);
        rectangle(&mut a, PORT + 288, destination);
        let gpu = a.runtime.graphics.as_ref().unwrap();
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("binary GPU text coverage"),
            size: wgpu::Extent3d {
                width: 7,
                height: 3,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let rgba: Vec<u8> = (0..21)
            .flat_map(|i| {
                let bit = (source[(i / 7) * 2] >> (7 - i % 7)) & 1;
                [bit * 255, 0, 0, 255]
            })
            .collect();
        gpu.queue.write_texture(
            texture.as_image_copy(),
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(28),
                rows_per_image: Some(3),
            },
            wgpu::Extent3d {
                width: 7,
                height: 3,
                depth_or_array_layers: 1,
            },
        );
        let original: Vec<_> = (0..21).map(|i| (i % (1usize << depth)) as u8).collect();
        let initial = packed(&original, depth, stride);
        for mode in 0..8i16 {
            a.runtime.memory.write(GuestPtr(BITS), &initial).unwrap();
            ports
                .copy_bits(
                    &mut a.runtime.memory,
                    gpu,
                    CopyBits {
                        source: GuestPtr(PORT + 256),
                        destination: GuestPtr(map),
                        source_rect: GuestPtr(PORT + 272),
                        destination_rect: GuestPtr(PORT + 288),
                        mode,
                        mask: GuestPtr(0),
                    },
                )
                .unwrap();
            let reference = a
                .runtime
                .memory
                .read(GuestPtr(BITS), initial.len())
                .unwrap()
                .to_vec();
            let display = gpu.read_rgba().unwrap();
            a.runtime.memory.write(GuestPtr(BITS), &initial).unwrap();
            ports
                .copy_gpu_mask(&mut a.runtime.memory, gpu, &texture, destination, mode)
                .unwrap();
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(BITS), initial.len())
                    .unwrap(),
                reference,
                "depth={depth} mode={mode}"
            );
            assert_eq!(
                gpu.read_rgba().unwrap(),
                display,
                "depth={depth} mode={mode}"
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(BITS - 1), 1).unwrap(),
                [0xa5]
            );
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(BITS + initial.len() as u32), 1)
                    .unwrap(),
                [0xa5]
            );
        }
        a.runtime.memory.write(GuestPtr(BITS), &initial).unwrap();
        assert!(ports
            .copy_gpu_mask(&mut a.runtime.memory, gpu, &texture, destination, 8)
            .is_err());
        let bad = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rejected text mask format"),
            size: wgpu::Extent3d {
                width: 7,
                height: 3,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        assert!(ports
            .copy_gpu_mask(&mut a.runtime.memory, gpu, &bad, destination, 0)
            .is_err());
        assert!(ports
            .copy_gpu_mask(&mut a.runtime.memory, gpu, &gpu.texture, destination, 0)
            .is_err());
        gpu.check_error().unwrap();
        assert_eq!(
            a.runtime
                .memory
                .read(GuestPtr(BITS), initial.len())
                .unwrap(),
            initial
        );
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn reserved_colors_keep_physical_pixels_but_match_only_for_the_owning_client() {
    use systemless_future::{color, devices};
    for depth in [2, 4, 8] {
        let (mut a, _, stride, colors) = setup(depth);
        let gd_h = a.runtime.memory.u32(devices::CURRENT).unwrap();
        let gd = a.runtime.memory.u32(GuestPtr(gd_h)).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(gd + 2), &7u16.to_be_bytes())
            .unwrap();
        let mut manager = std::mem::take(&mut a.runtime.process.color);
        manager
            .call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                color::Op::Reserve {
                    index: 0,
                    reserve: true,
                },
            )
            .unwrap();
        a.runtime.process.color = manager;
        for (client, native_call) in [(7u16, true), (8, false)] {
            a.runtime
                .memory
                .write(GuestPtr(gd + 2), &client.to_be_bytes())
                .unwrap();
            let closest = colors
                .iter()
                .enumerate()
                .filter(|(i, _)| client == 7 || *i != 0)
                .min_by_key(|(i, c)| {
                    (
                        c.iter()
                            .map(|v| {
                                let d = 65535u64 - u64::from(*v);
                                d * d
                            })
                            .sum::<u64>(),
                        *i,
                    )
                })
                .unwrap()
                .0 as u8;
            rgb(&mut a, [65535; 3], false, native_call);
            assert_eq!(
                a.runtime.memory.u32(GuestPtr(PORT + 80)).unwrap(),
                u32::from(closest)
            );
            let black = nearest([0; 3], &colors);
            let mut expected = vec![black; 21];
            expected[20] = 0; // A preexisting reserved pixel is still valid and visible.
            a.runtime
                .memory
                .write(GuestPtr(BITS), &packed(&expected, depth, stride))
                .unwrap();
            rectangle(&mut a, PORT + 128, Rect::new(-3, -2, 3, 1));
            invoke(&mut a, 6, &[10]);
            invoke(&mut a, 2, &[PORT + 128]);
            for y in 0..3 {
                for x in 0..6 {
                    expected[y * 7 + x] = closest;
                }
            }
            assert_eq!(unpack(&a, depth, stride), expected);
            check_gpu(&a, &expected, &colors);
        }
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn setentries_recolors_live_main_display_without_changing_guest_pixels_or_padding() {
    use systemless_future::{display::Display, quickdraw::SCREEN};
    let fixture = support::pef_program(&[0x4e800020], &[b"GetGDevice", b"SetEntries"], 32);
    let mut a = LoadedApplication::pef(&fixture.bytes, None)
        .unwrap()
        .with_display(Display::indexed8(7, 3).unwrap())
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xaa, 0x32, 0xaa, 0x3f],
            Permissions::RX,
        )
        .unwrap();
    let pixels: Vec<_> = (0..3).flat_map(|_| [0u8, 1, 0, 1, 0, 1, 0, 0xa5]).collect();
    a.runtime.memory.write(SCREEN, &pixels).unwrap();
    let indices: Vec<_> = (0..21).map(|i| (i % 7 % 2) as u8).collect();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    a.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 7, 3, GuestPtr(0x4000))).unwrap());
    let sequential: Vec<_> = [0xffffu16, 65535, 0, 0, 0xeeee, 0, 65535, 0]
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .collect();
    a.runtime
        .memory
        .map(GuestPtr(SOURCE), sequential.clone(), Permissions::RX)
        .unwrap();
    let mut frame = SOURCE.to_be_bytes().to_vec();
    frame.extend(1u16.to_be_bytes());
    frame.extend(0u16.to_be_bytes());
    native(&mut a, 2, &frame);
    a.runtime.refresh_screen().unwrap();
    check_gpu(&a, &indices, &[[65535, 0, 0], [0, 65535, 0]]);
    assert_eq!(a.runtime.memory.read(SCREEN, pixels.len()).unwrap(), pixels);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(SOURCE), 16).unwrap(),
        sequential
    );
    let indexed: Vec<_> = [1u16, 0, 0, 65535, 0, 65535, 65535, 0]
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .collect();
    a.runtime
        .memory
        .map(GuestPtr(SOURCE + 4096), indexed, Permissions::RX)
        .unwrap();
    invoke(&mut a, 1, &[u32::MAX, 1, SOURCE + 4096]);
    a.runtime.refresh_screen().unwrap();
    check_gpu(&a, &indices, &[[65535, 65535, 0], [0, 0, 65535]]);
    assert_eq!(a.runtime.memory.read(SCREEN, pixels.len()).unwrap(), pixels);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn live_direct_copybits_sources_share_rgb_modes_and_dither_across_cpu_adapters() {
    for source_depth in [16u16, 32] {
        for depth in [2u16, 4, 8] {
            let (mut a, map, stride, colors) = setup(depth);
            let bytes = usize::from(source_depth / 8);
            let row = 7 * bytes + 4;
            let mut pixels = vec![0xa5; row * 3];
            let mut source = Vec::new();
            for i in 0..21 {
                let c = [
                    [255, 255, 255],
                    [0, 0, 0],
                    [255, 0, 0],
                    [0, 255, 0],
                    [0, 0, 255],
                    [132, 132, 132],
                    [90, 222, 33],
                ][i % 7];
                let at = (i / 7) * row + (i % 7) * bytes;
                if source_depth == 16 {
                    let v = 0x8000
                        | (((c[0] as u16) >> 3) << 10)
                        | (((c[1] as u16) >> 3) << 5)
                        | ((c[2] as u16) >> 3);
                    pixels[at..at + 2].copy_from_slice(&v.to_be_bytes());
                    source.push([10, 5, 0].map(|s| {
                        let v = (v >> s) & 31;
                        (v << 11) | (v << 6) | (v << 1) | (v >> 4)
                    }));
                } else {
                    pixels[at..at + 4].copy_from_slice(&[0xa5, c[0], c[1], c[2]]);
                    source.push(c.map(|v| u16::from(v) * 257));
                }
            }
            a.runtime
                .memory
                .map(GuestPtr(SOURCE), pixels.clone(), Permissions::RW)
                .unwrap();
            let mut header = vec![0; 50];
            support::w32(&mut header, 0, SOURCE);
            support::w16(&mut header, 4, 0x8000 | row as u16);
            for (i, v) in [-2i16, -3, 1, 4].into_iter().enumerate() {
                support::w16(&mut header, 6 + i * 2, v as u16);
            }
            for (at, v) in [
                (30, 16),
                (32, source_depth),
                (34, 3),
                (36, if source_depth == 16 { 5 } else { 8 }),
            ] {
                support::w16(&mut header, at, v);
            }
            support::w32(&mut header, 42, 0xfffffffdu32);
            a.runtime
                .memory
                .write(GuestPtr(PORT + 256), &header)
                .unwrap();
            rgb(&mut a, [0; 3], false, true);
            rgb(&mut a, [65535; 3], true, false);
            let port_before = a.runtime.memory.read(GuestPtr(PORT), 108).unwrap().to_vec();
            let original: Vec<_> = (0..21).map(|i| if i % 2 == 0 { 1 } else { 2 }).collect();
            for mode in 0..8u16 {
                let expected: Vec<_> = source
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let s = if mode & 4 != 0 {
                            c.map(|v| 65535 - v)
                        } else {
                            *c
                        };
                        let d = colors[usize::from(original[i])];
                        let out = match mode & 3 {
                            0 => s,
                            1 => mix(s, [0; 3], d),
                            2 => {
                                if s == [0; 3] {
                                    d.map(|v| 65535 - v)
                                } else {
                                    d
                                }
                            }
                            _ => mix(s, [65535; 3], d),
                        };
                        nearest(out, &colors)
                    })
                    .collect();
                for native_call in [false, true] {
                    a.runtime
                        .memory
                        .write(GuestPtr(BITS), &packed(&original, depth, stride))
                        .unwrap();
                    copy(&mut a, map, mode, native_call);
                    assert_eq!(
                        unpack(&a, depth, stride),
                        expected,
                        "source{source_depth} target{depth} mode{mode} native{native_call}"
                    );
                    check_gpu(&a, &expected, &colors);
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(BITS), stride * 3).unwrap(),
                        packed(&expected, depth, stride)
                    );
                }
            }
            let mut dithered = None;
            for native_call in [false, true] {
                a.runtime
                    .memory
                    .write(GuestPtr(BITS), &packed(&original, depth, stride))
                    .unwrap();
                copy(&mut a, map, 0x40, native_call);
                let actual = unpack(&a, depth, stride);
                if let Some(expected) = &dithered {
                    assert_eq!(&actual, expected);
                }
                check_gpu(&a, &actual, &colors);
                dithered = Some(actual);
            }
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(SOURCE), pixels.len())
                    .unwrap(),
                pixels
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(PORT), 108).unwrap(),
                port_before
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(PORT + 256), 50).unwrap(),
                header
            );
            // Source guest edits must be observed without changing the descriptor.
            a.runtime
                .memory
                .write(GuestPtr(SOURCE), &[0; 4][..bytes])
                .unwrap();
            copy(&mut a, map, 0, true);
            assert_eq!(unpack(&a, depth, stride)[0], nearest([0; 3], &colors));
        }
    }
}

fn direct_word(depth: u16, rgb: [u16; 3]) -> Vec<u8> {
    if depth == 16 {
        (((rgb[0] >> 11) << 10) | ((rgb[1] >> 11) << 5) | (rgb[2] >> 11))
            .to_be_bytes()
            .to_vec()
    } else {
        vec![
            0,
            (rgb[0] >> 8) as u8,
            (rgb[1] >> 8) as u8,
            (rgb[2] >> 8) as u8,
        ]
    }
}
fn assert_direct(a: &LoadedApplication, depth: u16, stride: usize, expected: &[u8]) {
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BITS), stride * 3).unwrap(),
        expected
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(BITS - 1), 1).unwrap(),
        [0xa5]
    );
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(BITS + stride as u32 * 3), 1)
            .unwrap(),
        [0xa5]
    );
    let rgba = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
    for i in 0..21 {
        let at = (i / 7) * stride + (i % 7) * usize::from(depth / 8);
        let rgb = if depth == 16 {
            let v = u16::from_be_bytes(expected[at..at + 2].try_into().unwrap());
            [10, 5, 0].map(|s| {
                let v = (v >> s) & 31;
                ((v << 3) | (v >> 2)) as u8
            })
        } else {
            [expected[at + 1], expected[at + 2], expected[at + 3]]
        };
        assert_eq!(&rgba[i * 4..i * 4 + 4], &[rgb[0], rgb[1], rgb[2], 255]);
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn direct_destinations_share_paint_copybits_guest_writes_and_glyphs_on_both_cpus() {
    use systemless_future::load::resource::ResourceFork;
    for depth in [16u16, 32] {
        let (mut a, map, stride, _) = setup(depth);
        let bytes = usize::from(depth / 8);
        let fg = [0x1234, 0x4567, 0x89ab];
        rgb(&mut a, fg, false, true);
        rgb(&mut a, [65535; 3], true, false);
        let fg_bytes = direct_word(depth, fg);
        let mut expected = vec![0xa5; stride * 3];
        rectangle(&mut a, PORT + 128, Rect::new(-2, -1, 2, 1));
        invoke(&mut a, 2, &[PORT + 128]);
        for y in 1..3 {
            for x in 1..5 {
                let at = y * stride + x * bytes;
                expected[at..at + bytes].copy_from_slice(&fg_bytes);
            }
        }
        assert_direct(&a, depth, stride, &expected);
        // Direct guest writes are authoritative before the next native draw.
        a.runtime
            .memory
            .write(GuestPtr(BITS), &vec![0xa5; stride * 3])
            .unwrap();
        native(&mut a, 4, &(PORT + 128).to_be_bytes());
        assert_direct(&a, depth, stride, &expected);
        let samples = source(&mut a);
        rgb(&mut a, [0; 3], false, false);
        let initial = if depth == 16 {
            [9u16, 13, 5].map(|v| (v << 11) | (v << 6) | (v << 1) | (v >> 4))
        } else {
            [0xa5a5; 3]
        };
        for mode in 0..8u16 {
            for native_call in [false, true] {
                a.runtime
                    .memory
                    .write(GuestPtr(BITS), &vec![0xa5; stride * 3])
                    .unwrap();
                copy(&mut a, map, mode, native_call);
                let mut expected = vec![0xa5; stride * 3];
                for (i, c) in samples.iter().enumerate() {
                    let s = if mode & 4 != 0 {
                        c.map(|v| 65535 - v)
                    } else {
                        *c
                    };
                    let out = match mode & 3 {
                        0 => s,
                        1 => mix(s, [0; 3], initial),
                        2 => {
                            if s == [0; 3] {
                                initial.map(|v| 65535 - v)
                            } else {
                                initial
                            }
                        }
                        _ => mix(s, [65535; 3], initial),
                    };
                    let at = (i / 7) * stride + (i % 7) * bytes;
                    expected[at..at + bytes].copy_from_slice(&direct_word(depth, out));
                }
                assert_direct(&a, depth, stride, &expected);
            }
        }
        // FONT393: bearing-1, advance4, three rows of three ink bits.
        let words = [
            0x9000u16, 65, 66, 4, 0xfffe, 0xffff, 4, 3, 12, 2, 1, 0xffff, 1, 0xe400, 0xba00,
            0x4600, 0, 3, 5, 7, 0x0104, 0xffff, 0x0203, 0xffff,
        ];
        let font: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
        let fork = support::resource(vec![(*b"FONT", 393, None, font)]);
        a.runtime
            .process
            .install_application_resources(ResourceFork::parse(&fork).unwrap());
        a.runtime
            .memory
            .write(GuestPtr(PORT + 68), &[0, 3, 0, 0, 0, 0, 0, 9])
            .unwrap();
        rgb(&mut a, fg, false, false);
        for mode in 0..8u16 {
            for native_call in [false, true] {
                a.runtime
                    .memory
                    .write(GuestPtr(PORT + 72), &mode.to_be_bytes())
                    .unwrap();
                a.runtime
                    .memory
                    .write(GuestPtr(BITS), &vec![0xa5; stride * 3])
                    .unwrap();
                a.runtime
                    .memory
                    .write(GuestPtr(PORT + 48), &[0, 0, 255, 255])
                    .unwrap();
                if native_call {
                    native(&mut a, 8, &65u16.to_be_bytes());
                } else {
                    invoke(&mut a, 8, &[65]);
                }
                let mut expected = vec![0xa5; stride * 3];
                for (y, row) in [
                    [true, true, true, false, false],
                    [true, false, true, false, false],
                    [false, true, false, false, false],
                ]
                .iter()
                .enumerate()
                {
                    for (x, ink) in row.iter().enumerate() {
                        let at = y * stride + (x + 1) * bytes;
                        let on = *ink ^ (mode & 4 != 0);
                        let out = match mode & 3 {
                            0 => {
                                if on {
                                    fg
                                } else {
                                    [65535; 3]
                                }
                            }
                            1 => {
                                if on {
                                    fg
                                } else {
                                    initial
                                }
                            }
                            2 => {
                                if on {
                                    initial.map(|v| 65535 - v)
                                } else {
                                    initial
                                }
                            }
                            _ => {
                                if on {
                                    [65535; 3]
                                } else {
                                    initial
                                }
                            }
                        };
                        expected[at..at + bytes].copy_from_slice(&direct_word(depth, out));
                    }
                }
                assert_direct(&a, depth, stride, &expected);
                for _ in 0..2 {
                    a.runtime
                        .memory
                        .write(GuestPtr(PORT + 72), &2u16.to_be_bytes())
                        .unwrap();
                    a.runtime
                        .memory
                        .write(GuestPtr(PORT + 48), &[0, 0, 255, 255])
                        .unwrap();
                    if native_call {
                        native(&mut a, 8, &65u16.to_be_bytes());
                    } else {
                        invoke(&mut a, 8, &[65]);
                    }
                }
                assert_direct(&a, depth, stride, &expected);
                assert_eq!(a.runtime.memory.u16(GuestPtr(PORT + 50)).unwrap(), 3);
            }
        }
        let ports = a.runtime.process.ports.unwrap();
        let before = a
            .runtime
            .memory
            .read(GuestPtr(BITS), stride * 3)
            .unwrap()
            .to_vec();
        let port_before = a.runtime.memory.read(GuestPtr(PORT), 108).unwrap().to_vec();
        let gpu = a.runtime.graphics.as_ref().unwrap();
        let presented = gpu.read_rgba().unwrap();
        let error = ports
            .copy_bits(
                &mut a.runtime.memory,
                gpu,
                systemless_future::quickdraw::CopyBits {
                    source: GuestPtr(PORT + 256),
                    destination: GuestPtr(map),
                    source_rect: GuestPtr(PORT + 128),
                    destination_rect: GuestPtr(PORT + 128),
                    mode: 0x40,
                    mask: GuestPtr(0),
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("direct destination dithering"));
        assert_eq!(
            a.runtime.memory.read(GuestPtr(BITS), stride * 3).unwrap(),
            before
        );
        let readonly = GuestPtr(0x55000001);
        a.runtime
            .memory
            .map(readonly, before.clone(), Permissions::RX)
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(map), &readonly.0.to_be_bytes())
            .unwrap();
        assert!(ports
            .paint_rect(gpu, &mut a.runtime.memory, Rect::new(-3, -2, 4, 1))
            .is_err());
        assert_eq!(a.runtime.memory.read(readonly, stride * 3).unwrap(), before);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(PORT), 108).unwrap(),
            port_before
        );
        assert_eq!(gpu.read_rgba().unwrap(), presented);
    }
}
