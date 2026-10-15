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
