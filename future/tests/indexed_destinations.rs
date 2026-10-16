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
            vec![0xaa, 0x14, 0xaa, 0x15, 0xa8, 0xa2, 0xa8, 0xec],
            Permissions::RX,
        )
        .unwrap();
    invoke(&mut a, 0, &[0x600]);
    invoke(&mut a, 1, &[PORT]);
    let h = a.runtime.memory.u32(GuestPtr(PORT + 2)).unwrap();
    let map = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    let mut bytes = a.runtime.memory.read(GuestPtr(map), 50).unwrap().to_vec();
    let stride = (7 * usize::from(depth)).div_ceil(8) + 2;
    support::w32(&mut bytes, 0, BITS);
    support::w16(&mut bytes, 4, 0x8000 | stride as u16);
    for (i, v) in [-2i16, -3, 1, 4].into_iter().enumerate() {
        support::w16(&mut bytes, 6 + i * 2, v as u16);
    }
    for (at, v) in [(32, depth), (34, 1), (36, depth)] {
        support::w16(&mut bytes, at, v);
    }
    let colors = palette(depth);
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
    a.runtime
        .memory
        .write(GuestPtr(BITS), &packed(&[0; 21], depth, stride))
        .unwrap();
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
