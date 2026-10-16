mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    graphics::{Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    picture::decode,
    runtime::Stop,
};
fn rect(b: &mut Vec<u8>, r: [i16; 4]) {
    b.extend(r.iter().flat_map(|v| v.to_be_bytes()));
}
fn opcode(b: &mut Vec<u8>, v2: bool, v: u16) {
    if v2 {
        if !b.len().is_multiple_of(2) {
            b.push(0);
        }
        b.extend(v.to_be_bytes());
    } else {
        b.push(v as u8);
    }
}
fn fixture(v2: bool, packed: bool, mode: u16) -> Vec<u8> {
    let mut b = vec![0, 0];
    rect(&mut b, [10, 20, 12, 22]);
    if v2 {
        b.extend([0, 0x11, 2, 0xff]);
    } else {
        b.extend([0x11, 1]);
    }
    opcode(&mut b, v2, 0xa0);
    b.extend([0, 0x82]);
    opcode(&mut b, v2, if packed { 0x98 } else { 0x90 });
    b.extend((if packed { 8u16 } else { 1 }).to_be_bytes());
    rect(&mut b, [-2, 3, 0, 5]);
    rect(&mut b, [-2, 3, 0, 5]);
    rect(&mut b, [10, 20, 12, 22]);
    b.extend(mode.to_be_bytes());
    if packed {
        b.extend([4, 0, 0x80, 0xfa, 0, 4, 0, 0x40, 0xfa, 0]);
    } else {
        b.extend([0x80, 0x40]);
    }
    opcode(&mut b, v2, 0xff);
    let n = b.len() as u16;
    b[..2].copy_from_slice(&n.to_be_bytes());
    b
}
#[test]
fn v1_v2_raw_and_packbits_have_equivalent_commands_and_normalized_sources() {
    for v2 in [false, true] {
        for packed in [false, true] {
            let p = decode(&fixture(v2, packed, 1)).unwrap();
            assert_eq!(p.frame, Rect::new(20, 10, 22, 12));
            assert_eq!(p.commands.len(), 1);
            let c = &p.commands[0];
            assert_eq!(c.indices, [1, 0, 0, 1]);
            assert_eq!(c.source, Rect::new(0, 0, 2, 2));
            assert_eq!(c.destination, p.frame);
            assert_eq!(c.mode, 1);
        }
    }
}
#[test]
fn truncation_unknown_opcodes_bad_rows_and_color_bitmaps_are_explicit_errors() {
    let good = fixture(false, true, 0);
    for n in 0..good.len() {
        assert!(decode(&good[..n]).is_err(), "prefix {n}");
    }
    for (at, value) in [(12, 0x20), (16, 0x80), (44, 127), (45, 0xff)] {
        let mut b = good.clone();
        b[at] = value;
        assert!(decode(&b).is_err(), "offset {at}");
    }
    let mut bad = good.clone();
    let end = bad.len() - 1;
    bad[end] = 0x22;
    assert!(decode(&bad).is_err());
    let mut seed = 42u32;
    for n in 0..512 {
        let mut b = good.clone();
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let at = seed as usize % b.len();
        b[at] = (n & 255) as u8;
        let _ = decode(&b);
    }
}
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
    for (i, &a) in args.iter().enumerate() {
        c.gpr[3 + i] = a;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn app(picture: Vec<u8>) -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"OpenPort",
            b"SetPort",
            b"SetPortBits",
            b"DrawPicture",
        ],
        0,
    );
    let fork = support::resource(vec![(*b"PICT", 128, None, picture)]);
    let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 0x1000], Permissions::RW)
        .unwrap();
    invoke(&mut app, 0, &[0x600]);
    invoke(&mut app, 1, &[0x1000]);
    invoke(&mut app, 2, &[0x1000]);
    let mut bits = 0x1200u32.to_be_bytes().to_vec();
    bits.extend(2u16.to_be_bytes());
    rect(&mut bits, [0, 0, 4, 8]);
    app.runtime.memory.write(GuestPtr(0x1100), &bits).unwrap();
    invoke(&mut app, 3, &[0x1100]);
    let ports = app.runtime.process.ports.unwrap();
    ports.port_size(&mut app.runtime.memory, 8, 4).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x300), &[0, 0, 0, 2, 0, 4, 0, 6])
        .unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap());
    app
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_cpu_adapters_scale_picture_bits_and_preserve_port_state_and_padding() {
    let mut app = app(fixture(false, true, 0));
    let h = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"PICT", 128)
        .unwrap();
    let original = app
        .runtime
        .memory
        .read(GuestPtr(0x1000), 108)
        .unwrap()
        .to_vec();
    let initial = [0, 0x5a, 0, 0x5a, 0, 0x5a, 0, 0x5a];
    app.runtime
        .memory
        .write(GuestPtr(0x1200), &initial)
        .unwrap();
    assert_eq!(invoke(&mut app, 4, &[h, 0x300]), h);
    let expected = [0x30, 0x5a, 0x30, 0x5a, 0x0c, 0x5a, 0x0c, 0x5a];
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
        expected
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
        original
    );
    app.runtime
        .memory
        .write(GuestPtr(0x1200), &initial)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0xf6], Permissions::RX)
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    let mut args = 0x300u32.to_be_bytes().to_vec();
    args.extend(h.to_be_bytes());
    args.extend([0xa5; 4]);
    app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0x12345678;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 8);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
        expected
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 8), 4).unwrap(),
        [0xa5; 4]
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
        original
    );
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn picture_modes_clips_guest_writes_and_decode_errors_preserve_canonical_storage() {
    for mode in 0..8 {
        let mut bytes = fixture(true, false, mode);
        let mut clip = vec![0, 1, 0, 10];
        rect(&mut clip, [10, 20, 11, 22]);
        bytes.splice(14..14, clip);
        let end = bytes.len() as u32 - 1;
        let mut app = app(bytes);
        let h = app
            .runtime
            .process
            .get_resource(&mut app.runtime.memory, *b"PICT", 128)
            .unwrap();
        let ports = app.runtime.process.ports.unwrap();
        ports
            .clip_rect(&mut app.runtime.memory, Rect::new(3, 0, 6, 4))
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x1200), &[0xa5; 8])
            .unwrap();
        invoke(&mut app, 4, &[h, 0x300]);
        let actual = app
            .runtime
            .memory
            .read(GuestPtr(0x1200), 8)
            .unwrap()
            .to_vec();
        for y in 0..4 {
            let mut expected = 0xa5u8;
            for x in 3..6 {
                if y >= 2 {
                    continue;
                }
                let bit = u8::from((x < 4) == (y < 2)) ^ u8::from(mode & 4 != 0);
                let old = (0xa5 >> (7 - x)) & 1;
                let value = match mode & 3 {
                    0 => bit,
                    1 => old | bit,
                    2 => old ^ bit,
                    _ => old & (!bit & 1),
                };
                expected = (expected & !(1 << (7 - x))) | (value << (7 - x));
            }
            assert_eq!(actual[y * 2], expected, "mode {mode} row {y}");
            assert_eq!(actual[y * 2 + 1], 0xa5);
        }
        let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
        app.runtime
            .memory
            .write(GuestPtr(data + end), &[0x22])
            .unwrap();
        let before = actual.to_vec();
        let gpu = app.runtime.graphics.take().unwrap();
        assert!(systemless_future::picture::draw(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            ports,
            &gpu,
            h,
            GuestPtr(0x300)
        )
        .is_err());
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
            before
        );
    }
}

#[test]
fn picture_clip_and_bitmap_mask_intersect_and_v2_metadata_alignment_is_checked() {
    let mut b = fixture(false, false, 2);
    b[15] = 0x91;
    let mut mask = 10u16.to_be_bytes().to_vec();
    rect(&mut mask, [10, 21, 12, 22]);
    b.splice(44..44, mask);
    let mut clip = vec![1, 0, 10];
    rect(&mut clip, [10, 20, 11, 22]);
    b.splice(15..15, clip);
    let p = decode(&b).unwrap();
    assert_eq!(p.commands[0].clips, Some(vec![Rect::new(21, 10, 22, 11)]));
    let mut v2 = fixture(true, false, 0);
    let metadata = [0, 0xa1, 0, 7, 0, 1, 0x42, 0];
    v2.splice(14..14, metadata);
    assert_eq!(decode(&v2).unwrap().commands[0].indices, [1, 0, 0, 1]);
    let b = vec![0; 16 * 1024 * 1024 + 1];
    assert!(decode(&b).is_err());
}

fn indexed_fixture(
    ordered: bool,
    stride: usize,
    packed: bool,
    shaped_mask: bool,
) -> (Vec<u8>, usize, usize) {
    indexed_depth_fixture(ordered, stride, packed, shaped_mask, 8)
}
fn indexed_depth_fixture(
    ordered: bool,
    stride: usize,
    packed: bool,
    shaped_mask: bool,
    depth: u8,
) -> (Vec<u8>, usize, usize) {
    let mut b = vec![0, 0];
    rect(&mut b, [10, 20, 12, 22]);
    b.extend([0, 0x11, 2, 0xff]);
    opcode(&mut b, true, 0x1d);
    b.extend([0, 1, 0, 2, 0, 3]);
    opcode(&mut b, true, 0x1e);
    opcode(&mut b, true, 0x99);
    b.extend((0x8000 | stride as u16).to_be_bytes());
    rect(&mut b, [0, 0, 2, 2]);
    let layout = b.len();
    b.extend([0, 0, 0, 0]);
    b.extend([0; 12]);
    b.extend([0, 0, 0, depth, 0, 1, 0, depth]);
    b.extend([0; 12]);
    b.extend([0; 4]);
    b.extend((if ordered { 0x8000u16 } else { 0 }).to_be_bytes());
    let count = b.len();
    b.extend(3u16.to_be_bytes());
    let entries = if ordered {
        [0u16, 0, 0, 0]
    } else {
        [255, 7, 42, 99]
    };
    for (i, rgb) in [[65535u16; 3], [0; 3], [0, 40000, 0], [0, 65535, 65535]]
        .into_iter()
        .enumerate()
    {
        b.extend(entries[i].to_be_bytes());
        b.extend(rgb.into_iter().flat_map(|v| v.to_be_bytes()));
    }
    rect(&mut b, [0, 0, 2, 2]);
    rect(&mut b, [10, 20, 12, 22]);
    b.extend(0u16.to_be_bytes());
    if shaped_mask {
        let words = [
            36i16, 10, 20, 12, 22, 10, 20, 22, 32767, 11, 21, 22, 32767, 12, 20, 21, 32767, 32767,
        ];
        b.extend(words.iter().flat_map(|w| w.to_be_bytes()));
    } else {
        b.extend(10u16.to_be_bytes());
        rect(&mut b, [10, 20, 12, 22]);
    }
    for pair in if ordered {
        [[1, 0], [3, 2]]
    } else {
        [[7, 255], [99, 42]]
    } {
        let mut row = vec![0xcc; stride];
        if depth == 8 {
            row[..2].copy_from_slice(&pair);
        } else {
            assert!(ordered);
            let values = if depth == 1 {
                [pair[0] & 1, pair[1] & 1]
            } else {
                pair
            };
            row[0] = (values[0] << (8 - depth))
                | (values[1] << (8 - 2 * depth))
                | if depth < 4 {
                    (1 << (8 - 2 * depth)) - 1
                } else {
                    0
                };
        }
        if packed && stride >= 8 {
            let mut encoded = Vec::new();
            for chunk in row.chunks(128) {
                encoded.push((chunk.len() - 1) as u8);
                encoded.extend(chunk);
            }
            if stride > 250 {
                b.extend((encoded.len() as u16).to_be_bytes());
            } else {
                b.push(encoded.len() as u8);
            }
            b.extend(encoded);
        } else {
            if !packed {
                b[layout + 2..layout + 4].copy_from_slice(&1u16.to_be_bytes());
            }
            b.extend(row);
        }
    }
    opcode(&mut b, true, 0xff);
    let len = b.len() as u16;
    b[..2].copy_from_slice(&len.to_be_bytes());
    (b, layout, count)
}
#[test]
fn indexed_tables_packbits_padding_and_highlight_metadata_decode_explicitly() {
    for (ordered, stride, packed) in [(false, 8, true), (true, 252, true), (false, 8, false)] {
        let (b, _, _) = indexed_fixture(ordered, stride, packed, true);
        let p = decode(&b).unwrap();
        assert_eq!(p.highlight, systemless_future::picture::Highlight::Default);
        let c = &p.commands[0];
        assert_eq!(
            c.indices,
            if ordered {
                vec![1, 0, 3, 2]
            } else {
                vec![7, 255, 99, 42]
            }
        );
        let table = c.palette.as_ref().unwrap();
        assert_eq!(table[if ordered { 1 } else { 7 }], [0; 3]);
        assert_eq!(table[if ordered { 0 } else { 255 }], [65535; 3]);
        assert_eq!(
            c.clips,
            Some(vec![Rect::new(20, 10, 22, 11), Rect::new(20, 11, 21, 12)])
        );
    }
    let (mut b, _, _) = indexed_fixture(false, 8, true, false);
    b.drain(22..24);
    assert_eq!(
        decode(&b).unwrap().highlight,
        systemless_future::picture::Highlight::Rgb([1, 2, 3])
    );
}
#[test]
fn indexed_truncation_tables_components_and_rendering_modes_are_checked() {
    let (good, layout, count) = indexed_fixture(false, 8, true, false);
    for n in 0..good.len() {
        assert!(decode(&good[..n]).is_err(), "indexed prefix {n}");
    }
    for (offset, value) in [
        (layout + 19, 16),
        (layout + 21, 3),
        (count, 0xff),
        (count + 2, 1),
        (count + 10, 1),
        (count + 11, 0xff),
    ] {
        let mut b = good.clone();
        b[offset] = value;
        assert!(decode(&b).is_err(), "indexed offset {offset}");
    }
    let mut b = good.clone();
    b[22] = 0;
    b[23] = 0x1c;
    assert!(decode(&b).is_err());
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn indexed_gpu_palette_conversion_masks_and_mutable_resources_match_both_cpus() {
    for (ordered, stride, packed, mask, depth) in [
        (false, 8, true, false, 8),
        (true, 252, true, true, 8),
        (false, 8, false, false, 8),
        (true, 8, true, false, 1),
        (true, 8, true, true, 2),
        (true, 252, true, false, 4),
        (true, 2, true, false, 4),
    ] {
        let (b, _, count) = indexed_depth_fixture(ordered, stride, packed, mask, depth);
        let mut app = app(b);
        let h = app
            .runtime
            .process
            .get_resource(&mut app.runtime.memory, *b"PICT", 128)
            .unwrap();
        let original = app
            .runtime
            .memory
            .read(GuestPtr(0x1000), 108)
            .unwrap()
            .to_vec();
        let initial = [0, 0x5a, 0, 0x5a, 0, 0x5a, 0, 0x5a];
        let expected = if depth == 1 {
            [0x30, 0x5a, 0x30, 0x5a, 0x30, 0x5a, 0x30, 0x5a]
        } else if mask {
            [0x30, 0x5a, 0x30, 0x5a, 0, 0x5a, 0, 0x5a]
        } else {
            [0x30, 0x5a, 0x30, 0x5a, 0x0c, 0x5a, 0x0c, 0x5a]
        };
        for architecture in [Architecture::PowerPc, Architecture::M68k] {
            app.runtime
                .memory
                .write(GuestPtr(0x1200), &initial)
                .unwrap();
            if architecture == Architecture::PowerPc {
                assert_eq!(invoke(&mut app, 4, &[h, 0x300]), h);
            } else {
                app.runtime
                    .memory
                    .map(GuestPtr(0x50000000), vec![0xa8, 0xf6], Permissions::RX)
                    .unwrap();
                let stack = STACK_BASE + 0x8000;
                let mut args = 0x300u32.to_be_bytes().to_vec();
                args.extend(h.to_be_bytes());
                app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
                app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
                assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
                let Cpu::M68k(c) = &app.runtime.cpu else {
                    panic!()
                };
                assert_eq!(c.dar[15], stack + 8);
            }
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
                expected,
                "{architecture:?} ordered {ordered} stride {stride}"
            );
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
                original
            );
        }
        let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
        let black_rgb = count as u32 + 2 + 8 + 2;
        app.runtime
            .memory
            .write(GuestPtr(data + black_rgb), &[0xff; 6])
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x1200), &initial)
            .unwrap();
        invoke(&mut app, 4, &[h, 0x300]);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x1200), 2).unwrap(),
            [0, 0x5a]
        );
    }
}

#[test]
fn packed_indexed_depths_preserve_pixel_order_and_ignore_row_padding() {
    for depth in [1, 2, 4] {
        for (stride, packed) in [(2, true), (8, true), (252, true), (8, false)] {
            let (bytes, layout, _) = indexed_depth_fixture(true, stride, packed, false, depth);
            let picture = decode(&bytes).unwrap();
            assert_eq!(
                picture.commands[0].indices,
                if depth == 1 {
                    vec![1, 0, 1, 0]
                } else {
                    vec![1, 0, 3, 2]
                }
            );
            for n in 0..bytes.len() {
                assert!(decode(&bytes[..n]).is_err(), "depth {depth} prefix {n}");
            }
            let mut too_wide = bytes.clone();
            too_wide[layout - 2..layout]
                .copy_from_slice(&((stride * 8 / usize::from(depth) + 1) as i16).to_be_bytes());
            assert!(decode(&too_wide).is_err());
            let mut bad = bytes.clone();
            bad[layout + 23] = 8;
            assert!(decode(&bad).is_err());
            let mut bad = bytes.clone();
            bad[layout + 19] = 3;
            assert!(decode(&bad).is_err());
        }
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn inherited_screen_bounds_in_a_basic_offscreen_port_use_only_the_checked_drawing_prefix() {
    let mut app = app(fixture(false, true, 2));
    let h = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"PICT", 128)
        .unwrap();
    let pixels = GuestPtr(0x55000000);
    app.runtime
        .memory
        .map(pixels, vec![0; 8], Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1002), &pixels.0.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x1008), &[0, 0, 0, 0, 1, 0xe0, 2, 0x80])
        .unwrap();
    let ports = app.runtime.process.ports.unwrap();
    ports
        .clip_rect(&mut app.runtime.memory, Rect::new(2, 1, 6, 3))
        .unwrap();
    let original = app
        .runtime
        .memory
        .read(GuestPtr(0x1000), 108)
        .unwrap()
        .to_vec();
    assert!(
        systemless_future::bitmap::Bitmap::read(&app.runtime.memory, GuestPtr(0x1002)).is_err()
    );
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0xf6], Permissions::RX)
        .unwrap();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        app.runtime
            .memory
            .write(pixels, &[0x0f, 0x5a, 0x0f, 0x5a, 0x0f, 0x5a, 0x0f, 0x5a])
            .unwrap();
        for repeat in 0..2 {
            if arch == Architecture::PowerPc {
                invoke(&mut app, 4, &[h, 0x300]);
            } else {
                let stack = STACK_BASE + 0x8000;
                let mut frame = 0x300u32.to_be_bytes().to_vec();
                frame.extend(h.to_be_bytes());
                frame.extend([0xa5; 4]);
                app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
                app.runtime.cpu = Cpu::new(arch, 0x50000000, stack);
                assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
                let Cpu::M68k(c) = &app.runtime.cpu else {
                    panic!()
                };
                assert_eq!(c.dar[15], stack + 8);
            }
            let expected = if repeat == 0 {
                [0x0f, 0x5a, 0x3f, 0x5a, 0x03, 0x5a, 0x0f, 0x5a]
            } else {
                [0xa5, 0x5a, 0x0f, 0x5a, 0x0f, 0x5a, 0x0f, 0x5a]
            };
            assert_eq!(app.runtime.memory.read(pixels, 8).unwrap(), expected);
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
                original
            );
            if repeat == 0 {
                app.runtime.memory.write(pixels, &[0xa5]).unwrap();
            }
        }
    }
}

fn direct_fixture(
    depth: u16,
    pack: u16,
    width: usize,
    components: u16,
    mask: bool,
) -> (Vec<u8>, Vec<[u16; 3]>) {
    let stride = (width * usize::from(depth / 8)).div_ceil(4) * 4;
    let mut bytes = vec![0, 0];
    rect(&mut bytes, [0, 0, 2, width as i16]);
    bytes.extend([0, 0x11, 2, 0xff]);
    opcode(&mut bytes, true, if mask { 0x9b } else { 0x9a });
    bytes.extend(0xffu32.to_be_bytes());
    bytes.extend((0x8000 | stride as u16).to_be_bytes());
    rect(&mut bytes, [-2, 3, 0, 3 + width as i16]);
    for word in [0, pack] {
        bytes.extend(word.to_be_bytes());
    }
    bytes.extend([0; 12]);
    for word in [16, depth, components, if depth == 16 { 5 } else { 8 }] {
        bytes.extend(word.to_be_bytes());
    }
    bytes.extend([0; 12]);
    rect(&mut bytes, [-2, 3, 0, 3 + width as i16]);
    rect(&mut bytes, [0, 0, 2, width as i16]);
    bytes.extend(0u16.to_be_bytes());
    if mask {
        bytes.extend(10u16.to_be_bytes());
        rect(&mut bytes, [0, 0, 1, width as i16]);
    }
    let packing = if pack == 0 {
        if depth == 16 {
            3
        } else {
            4
        }
    } else {
        pack
    };
    let mut expected = Vec::new();
    for y in 0..2 {
        let colors: Vec<[u8; 3]> = (0..width)
            .map(|x| match (x + y) % 4 {
                0 => [255, 0, 0],
                1 => [0, 255, 0],
                2 => [0, 0, 255],
                _ => [255; 3],
            })
            .collect();
        expected.extend(colors.iter().map(|c| c.map(|v| u16::from(v) * 257)));
        let raw = stride < 8 || packing == 1;
        let mut row = Vec::new();
        if depth == 16 {
            for c in &colors {
                let v = ((u16::from(c[0]) >> 3) << 10)
                    | ((u16::from(c[1]) >> 3) << 5)
                    | (u16::from(c[2]) >> 3);
                row.extend(v.to_be_bytes());
            }
            row.resize(stride, 0x5a);
        } else if raw {
            for c in &colors {
                row.push(0x7e);
                row.extend(c);
            }
        } else if packing == 2 {
            for c in &colors {
                row.extend(c);
            }
        } else {
            if components == 4 {
                row.extend(vec![0x7e; width]);
            }
            for channel in 0..3 {
                row.extend(colors.iter().map(|c| c[channel]));
            }
        }
        if raw || packing == 2 {
            bytes.extend(row);
        } else {
            let unit = if depth == 16 { 2 } else { 1 };
            let mut packed = vec![128];
            for block in row.chunks(128 * unit) {
                packed.push((block.len() / unit - 1) as u8);
                packed.extend(block);
            }
            if stride > 250 {
                bytes.extend((packed.len() as u16).to_be_bytes());
            } else {
                bytes.push(packed.len() as u8);
            }
            bytes.extend(packed);
        }
    }
    opcode(&mut bytes, true, 0xff);
    (bytes, expected)
}
#[test]
fn direct_16_and_32_bit_rows_preserve_rgb_for_raw_word_runs_dropped_padding_and_component_planes() {
    for (depth, pack, width, components) in [
        (16, 1, 3, 3),
        (16, 3, 3, 3),
        (16, 0, 128, 3),
        (32, 1, 3, 3),
        (32, 2, 3, 3),
        (32, 4, 3, 3),
        (32, 4, 3, 4),
        (32, 0, 64, 3),
        (32, 4, 1, 3),
    ] {
        for mask in [false, true] {
            let (bytes, expected) = direct_fixture(depth, pack, width, components, mask);
            let picture = decode(&bytes).unwrap();
            let command = &picture.commands[0];
            assert_eq!(
                command.rgb.as_ref().unwrap(),
                &expected,
                "{depth}/{pack}/{width}/{components}"
            );
            assert!(command.indices.is_empty());
            assert!(command.palette.is_none());
            assert_eq!(command.source, Rect::new(0, 0, width as i32, 2));
            assert_eq!(command.clips.is_some(), mask);
        }
    }
}
#[test]
fn direct_truncations_bad_layouts_and_word_run_overflow_are_rejected() {
    for (depth, pack) in [(16, 3), (32, 4), (32, 2)] {
        let (bytes, _) = direct_fixture(depth, pack, 3, 3, false);
        for n in 0..bytes.len() {
            assert!(decode(&bytes[..n]).is_err(), "{depth}/{pack} prefix{n}");
        }
        // Opcode at14, base placeholder16, rowBytes20, version30, pack32,
        // pixelType46, pixelSize48, cmpCount50, cmpSize52, planeBytes54.
        for (at, value) in [
            (20, 0xc008u16),
            (30, 1),
            (32, 7),
            (46, 0),
            (48, 8),
            (50, 2),
            (52, 1),
            (54, 1),
        ] {
            let mut bad = bytes.clone();
            bad[at..at + 2].copy_from_slice(&value.to_be_bytes());
            assert!(decode(&bad).is_err(), "field{at}");
        }
    }
    let (mut bytes, _) = direct_fixture(16, 3, 4, 3, false);
    // First row at84: packed byte count, NOP, literal control, pixel words.
    bytes[86] = 127;
    assert!(decode(&bytes).is_err());
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_cpu_adapters_draw_direct_rgb_pictures_with_clipping_and_unchanged_port_storage() {
    for (depth, pack) in [(16, 3), (32, 2), (32, 4)] {
        let (bytes, _) = direct_fixture(depth, pack, 4, 3, true);
        let mut app = app(bytes);
        let h = app
            .runtime
            .process
            .get_resource(&mut app.runtime.memory, *b"PICT", 128)
            .unwrap();
        let before = app
            .runtime
            .memory
            .read(GuestPtr(0x1000), 108)
            .unwrap()
            .to_vec();
        app.runtime
            .memory
            .map(GuestPtr(0x50000000), vec![0xa8, 0xf6], Permissions::RX)
            .unwrap();
        for arch in [Architecture::M68k, Architecture::PowerPc] {
            app.runtime
                .memory
                .write(GuestPtr(0x1200), &[0, 0x5a, 0, 0x5a, 0, 0x5a, 0, 0x5a])
                .unwrap();
            if arch == Architecture::PowerPc {
                invoke(&mut app, 4, &[h, 0x300]);
            } else {
                let stack = STACK_BASE + 0x8000;
                let mut args = 0x300u32.to_be_bytes().to_vec();
                args.extend(h.to_be_bytes());
                args.extend([0xa5; 4]);
                app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
                app.runtime.cpu = Cpu::new(arch, 0x50000000, stack);
                assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            }
            // Primary red/green/blue are dark under the existing white/black policy;
            // mask leaves the lower half of the scaled picture untouched.
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x1200), 8).unwrap(),
                [0x38, 0x5a, 0x38, 0x5a, 0, 0x5a, 0, 0x5a]
            );
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x1000), 108).unwrap(),
                before
            );
        }
    }
}
