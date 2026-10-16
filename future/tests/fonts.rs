mod support;
use support::*;
use systemless_future::{
    cpu::{Architecture, Cpu},
    font::Strike,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn strike() -> Vec<u8> {
    [
        0x9000u16, 65, 66, 4, 0xfffe, 0xffff, 4, 3, 12, 2, 1, 0xffff, 1, 0xe400, 0xba00, 0x4600, 0,
        3, 5, 7, 0x0104, 0xffff, 0x0203, 0xffff,
    ]
    .into_iter()
    .flat_map(u16::to_be_bytes)
    .collect()
}
#[test]
fn bitmap_font_metrics_bearings_masks_and_missing_character_are_exact() {
    let bytes = strike();
    let font = Strike::parse(&bytes).unwrap();
    assert_eq!(font.metrics.ascent, 2);
    assert_eq!(font.metrics.descent, 1);
    assert_eq!(font.metrics.leading, -1);
    assert_eq!(font.glyph(b'A').bearing, -1);
    assert_eq!(font.glyph(b'A').top, -2);
    assert_eq!(font.glyph(b'A').advance, 4);
    assert_eq!(font.mask(b'A').unwrap(), [1, 1, 1, 1, 0, 1, 0, 1, 0]);
    assert_eq!(font.glyph(b'B'), font.glyph(b'?'));
    assert_eq!(font.glyph(b'B').bearing, 0);
    assert_eq!(font.mask(b'B').unwrap(), [1, 0, 0, 1, 1, 1]);
    assert_eq!(font.width(b"AB?").unwrap(), 10);
}
#[test]
fn truncated_and_corrupt_font_tables_never_parse_or_panic() {
    let bytes = strike();
    for end in 0..bytes.len() {
        assert!(Strike::parse(&bytes[..end]).is_err());
    }
    for (offset, value) in [
        (0, 0x9001u16),
        (0, 0x9004),
        (0, 0x9080),
        (2, 67),
        (4, 256),
        (14, 0),
        (24, 0),
        (16, 1),
        (10, 0x7fff),
        (36, 2),
        (38, 17),
        (44, 0xffff),
        (46, 0),
    ] {
        let mut corrupt = bytes.clone();
        corrupt[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        assert!(
            Strike::parse(&corrupt).is_err(),
            "offset {offset}, value {value}"
        );
    }
}
#[test]
fn extended_width_table_offset_uses_positive_ndescent_without_truncation() {
    let mut bytes = strike();
    let table = bytes[40..48].to_vec();
    let offset = 16 + 2 * ((1 << 16) + 12);
    bytes.resize(offset + 8, 0);
    bytes[10..12].copy_from_slice(&1u16.to_be_bytes());
    bytes[offset..].copy_from_slice(&table);
    let parsed = Strike::parse(&bytes).unwrap();
    assert_eq!(parsed.glyph(b'A').advance, 4);
    assert_eq!(parsed.mask(b'A').unwrap(), [1, 1, 1, 1, 0, 1, 0, 1, 0]);
}
#[test]
fn real_get_font_info_adapters_share_mutable_guest_font_resource() {
    let fork = resource(vec![(*b"FONT", 393, None, strike())]);
    let f = pef_program(&[0x4e800020], &[b"GetFontInfo"], 64);
    let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    let ports = systemless_future::quickdraw::Ports {
        current_slot: GuestPtr(0x51000000),
        screen_bitmap: GuestPtr(0),
    };
    app.runtime
        .memory
        .write(ports.current_slot, &0x51000100u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x51000100 + 68), &[0, 3, 0, 0xa5, 0, 1, 0, 9])
        .unwrap();
    app.runtime.process.ports = Some(ports);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x8b], Permissions::RX)
        .unwrap();
    let out = app.info.toc_or_a5 + 32;
    let stack = STACK_BASE + 0x8000;
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate, stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = out;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(out), 8).unwrap(),
        [0, 2, 0, 1, 0, 4, 255, 255]
    );
    let h = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"FONT", 393)
        .unwrap();
    let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(data + 6), &99i16.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(stack), &out.to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.memory.u16(GuestPtr(out + 4)).unwrap(), 99);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 4);
    assert_eq!(
        app.runtime
            .process
            .get_resource(&mut app.runtime.memory, *b"FONT", 393)
            .unwrap(),
        h
    );
}

#[test]
fn init_fonts_allocates_once_and_default_globals_select_live_metrics_on_both_cpus() {
    let f = pef_program(&[0x4e800020], &[b"InitFonts", b"GetFontInfo"], 64);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    let ports = systemless_future::quickdraw::Ports {
        current_slot: GuestPtr(0x51000000),
        screen_bitmap: GuestPtr(0),
    };
    app.runtime
        .memory
        .write(ports.current_slot, &0x51000100u32.to_be_bytes())
        .unwrap();
    app.runtime.process.ports = Some(ports);
    let stack = STACK_BASE + 0x8000;
    let out = app.info.toc_or_a5 + 32;
    let gate = |app: &LoadedApplication, index: u32| {
        let vector = app
            .runtime
            .memory
            .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        app.runtime.memory.u32(GuestPtr(vector)).unwrap()
    };
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate(&app, 0), stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert!(app.runtime.process.fonts.initialized);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0xba8)).unwrap(), 12);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x984)).unwrap(), 3);
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate(&app, 1), stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = out;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let metrics = app.runtime.memory.read(GuestPtr(out), 8).unwrap().to_vec();
    assert!(app.runtime.memory.u16(GuestPtr(out)).unwrap() > 0);
    let first = app
        .runtime
        .process
        .new_pointer(&mut app.runtime.memory, 4, false)
        .unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa8, 0xfe, 0xa8, 0x8b],
            Permissions::RX,
        )
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let second = app
        .runtime
        .process
        .new_pointer(&mut app.runtime.memory, 4, false)
        .unwrap();
    assert_eq!(second, first + 4);
    app.runtime
        .memory
        .write(GuestPtr(stack), &out.to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000002, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.memory.read(GuestPtr(out), 8).unwrap(), metrics);
    app.runtime
        .memory
        .write(GuestPtr(0x51000100 + 68), &1u16.to_be_bytes())
        .unwrap();
    systemless_future::text::font_info(
        &mut app.runtime.memory,
        &mut app.runtime.process,
        ports,
        GuestPtr(out),
    )
    .unwrap();
    assert_eq!(app.runtime.memory.read(GuestPtr(out), 8).unwrap(), metrics);
    app.runtime
        .memory
        .write(GuestPtr(0xba8), &9u16.to_be_bytes())
        .unwrap();
    systemless_future::text::font_info(
        &mut app.runtime.memory,
        &mut app.runtime.process,
        ports,
        GuestPtr(out),
    )
    .unwrap();
    assert_ne!(app.runtime.memory.read(GuestPtr(out), 8).unwrap(), metrics);
}

#[test]
fn guest_system_font_resource_precedes_the_substitute_and_survives_reinitialization() {
    use systemless_future::{
        load::resource::ResourceFork, memory::Memory, process::Process, quickdraw::Ports,
    };
    let fork = resource(vec![(*b"FONT", 12, None, strike())]);
    let mut process = Process::default();
    process.install_application_resources(ResourceFork::parse(&fork).unwrap());
    let mut memory = Memory::default();
    memory
        .map(GuestPtr(0), vec![0; 4096], Permissions::RW)
        .unwrap();
    memory
        .map(GuestPtr(0x5000), vec![0; 512], Permissions::RW)
        .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x5000),
        screen_bitmap: GuestPtr(0),
    };
    memory
        .write(ports.current_slot, &0x5100u32.to_be_bytes())
        .unwrap();
    systemless_future::font::initialize(&mut memory, &mut process).unwrap();
    let handle = process.get_resource(&mut memory, *b"FONT", 12).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    systemless_future::text::font_info(&mut memory, &mut process, ports, GuestPtr(0x5080)).unwrap();
    assert_eq!(
        memory.read(GuestPtr(0x5080), 8).unwrap(),
        [0, 2, 0, 1, 0, 4, 255, 255]
    );
    memory
        .write(GuestPtr(data + 6), &99i16.to_be_bytes())
        .unwrap();
    systemless_future::font::initialize(&mut memory, &mut process).unwrap();
    systemless_future::text::font_info(&mut memory, &mut process, ports, GuestPtr(0x5080)).unwrap();
    assert_eq!(memory.u16(GuestPtr(0x5084)).unwrap(), 99);
    assert_eq!(
        process.get_resource(&mut memory, *b"FONT", 12).unwrap(),
        handle
    );
}

#[test]
fn string_width_returns_signed_pascal_and_ppc_results_without_moving_pen() {
    let mut font = strike();
    font[6..8].copy_from_slice(&255u16.to_be_bytes());
    font[40..42].copy_from_slice(&0x01ffu16.to_be_bytes());
    let fork = resource(vec![(*b"FONT", 393, None, font)]);
    let f = pef_program(&[0x4e800020], &[b"StringWidth"], 512);
    let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    let ports = systemless_future::quickdraw::Ports {
        current_slot: GuestPtr(0x51000000),
        screen_bitmap: GuestPtr(0),
    };
    app.runtime
        .memory
        .write(ports.current_slot, &0x51000100u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x51000100 + 68), &[0, 3, 0, 0, 0, 1, 0, 9])
        .unwrap();
    app.runtime.process.ports = Some(ports);
    systemless_future::text::move_to(&mut app.runtime.memory, ports, 123, -45).unwrap();
    let string = app.info.toc_or_a5 + 64;
    let mut bytes = vec![b'A'; 256];
    bytes[0] = 255;
    app.runtime.memory.write(GuestPtr(string), &bytes).unwrap();
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate, stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = string;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], (-511i32) as u32);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x8c], Permissions::RX)
        .unwrap();
    let mut frame = string.to_be_bytes().to_vec();
    frame.extend([0x5a; 4]);
    app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 4);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(stack + 4)).unwrap() as i16,
        -511
    );
    assert_eq!(app.runtime.memory.u16(GuestPtr(stack + 6)).unwrap(), 0x5a5a);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0x51000100 + 50)).unwrap(),
        123
    );
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0x51000100 + 48)).unwrap() as i16,
        -45
    );
    use systemless_future::text::Input;
    for (pointer, first, count, expected) in [
        (string + 1, 0, 2, 510i16),
        (string + 2, -1, 2, 510),
        (u32::MAX, 1, 0, 0),
        (u32::MAX, 1, -1, 0),
    ] {
        assert_eq!(
            systemless_future::text::width(
                &mut app.runtime.memory,
                &mut app.runtime.process,
                ports,
                Input::Buffer {
                    pointer: GuestPtr(pointer),
                    first,
                    count
                }
            )
            .unwrap(),
            expected
        );
    }
    for (pointer, first, count) in [(0, -1, 1), (u32::MAX, 1, 1), (string + 511, 0, 2)] {
        assert!(systemless_future::text::width(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            ports,
            Input::Buffer {
                pointer: GuestPtr(pointer),
                first,
                count
            }
        )
        .is_err());
    }
}

#[test]
fn synthetic_bold_dilates_glyphs_and_preserves_source_and_missing_mapping() {
    let source = strike();
    let bold = systemless_future::font::embolden(&source).unwrap();
    let font = Strike::parse(&bold).unwrap();
    assert_eq!(
        font.mask(b'A').unwrap(),
        [1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0]
    );
    assert_eq!(font.mask(b'?').unwrap(), [1, 1, 0, 0, 1, 1, 1, 1, 1]);
    assert_eq!(font.glyph(b'A').bearing, -1);
    assert_eq!(font.glyph(b'A').advance, 5);
    assert_eq!(font.glyph(b'B'), font.glyph(b'?'));
    assert_eq!(font.width(b"AB?").unwrap(), 13);
    assert_eq!(font.metrics.maximum_width, 5);
    assert_eq!(font.metrics.ascent, 2);
    assert_eq!(font.metrics.descent, 1);
    assert_eq!(font.metrics.leading, -1);
    assert_eq!(&bold[10..12], &source[10..12]);
    assert_eq!(source, strike());
    let mut overflow = source.clone();
    overflow[41] = 255;
    assert!(systemless_future::font::embolden(&overflow).is_err());
    for end in 0..source.len() {
        assert!(systemless_future::font::embolden(&source[..end]).is_err());
    }
}

fn family_resource(size: u16, style: u16, id: i16, widths: bool) -> Vec<u8> {
    let mut bytes = vec![0; if widths { 580 } else { 60 }];
    for (offset, value) in [
        (2, 2020u16),
        (4, 0),
        (6, 255),
        (54, size),
        (56, style),
        (58, id as u16),
    ] {
        w16(&mut bytes, offset, value);
    }
    if widths {
        w32(&mut bytes, 16, 60);
        for code in 0..257 {
            w16(&mut bytes, 64 + code * 2, 2048);
        }
    }
    bytes
}
#[test]
fn fond_bitmap_association_uses_arbitrary_resource_ids_and_live_guest_data_on_both_cpus() {
    for kind in [*b"NFNT", *b"FONT"] {
        let fork = resource(vec![
            (*b"FOND", 2020, None, family_resource(9, 0, -123, false)),
            (kind, -123, None, strike()),
        ]);
        let f = pef_program(&[0x4e800020], &[b"GetFontInfo"], 64);
        let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
        let ports = systemless_future::quickdraw::Ports {
            current_slot: GuestPtr(0x51000000),
            screen_bitmap: GuestPtr(0),
        };
        app.runtime
            .memory
            .map(ports.current_slot, vec![0; 512], Permissions::RW)
            .unwrap();
        app.runtime
            .memory
            .write(ports.current_slot, &0x51000100u32.to_be_bytes())
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x51000100 + 68), &[7, 228, 0, 0xa5, 0, 1, 0, 9])
            .unwrap();
        app.runtime.process.ports = Some(ports);
        let vector = app
            .runtime
            .memory
            .u32(GuestPtr(app.info.toc_or_a5 + 8))
            .unwrap();
        let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
        let stack = STACK_BASE + 0x8000;
        let out = app.info.toc_or_a5 + 32;
        app.runtime
            .memory
            .map(GuestPtr(0x50000000), vec![0xa8, 0x8b], Permissions::RX)
            .unwrap();
        for (architecture, expected) in [(Architecture::PowerPc, 4), (Architecture::M68k, 99)] {
            if architecture == Architecture::M68k {
                let handle = app
                    .runtime
                    .process
                    .get_resource(&mut app.runtime.memory, kind, -123)
                    .unwrap();
                let data = app.runtime.memory.u32(GuestPtr(handle)).unwrap();
                app.runtime
                    .memory
                    .write(GuestPtr(data + 6), &99u16.to_be_bytes())
                    .unwrap();
                app.runtime
                    .memory
                    .write(GuestPtr(stack), &out.to_be_bytes())
                    .unwrap();
            }
            app.runtime.cpu = Cpu::new(
                architecture,
                if architecture == Architecture::PowerPc {
                    gate
                } else {
                    0x50000000
                },
                stack,
            );
            if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
                c.gpr[3] = out;
            }
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            assert_eq!(app.runtime.memory.u16(GuestPtr(out + 4)).unwrap(), expected);
        }
        // A family with only a different-size bitmap must not silently scale or substitute.
        app.runtime
            .memory
            .write(GuestPtr(0x51000100 + 74), &10u16.to_be_bytes())
            .unwrap();
        assert!(systemless_future::text::font_info(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            ports,
            GuestPtr(out)
        )
        .is_err());
    }
}
#[test]
fn embedded_outline_uses_family_widths_and_invalidates_cached_live_resource_edits() {
    use systemless_future::{
        load::resource::ResourceFork,
        memory::Memory,
        process::Process,
        quickdraw::Ports,
        text::{self, Input},
    };
    let source = include_bytes!("../assets/fonts/NimbusSans-Regular.ttf").to_vec();
    let fork = resource(vec![
        (*b"FOND", 2020, None, family_resource(0, 0, 9295, true)),
        (*b"sfnt", 9295, None, source),
    ]);
    let mut process = Process::default();
    process.install_application_resources(ResourceFork::parse(&fork).unwrap());
    let mut memory = Memory::default();
    memory
        .map(GuestPtr(0x5000), vec![0; 512], Permissions::RW)
        .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x5000),
        screen_bitmap: GuestPtr(0),
    };
    memory
        .write(ports.current_slot, &0x5100u32.to_be_bytes())
        .unwrap();
    memory
        .write(GuestPtr(0x5100 + 68), &[7, 228, 0, 0xa5, 0, 1, 0, 14])
        .unwrap();
    assert_eq!(
        text::width(&mut memory, &mut process, ports, Input::Character(b'A')).unwrap(),
        7
    );
    let handle = process.get_resource(&mut memory, *b"FOND", 2020).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    memory
        .write(
            GuestPtr(data + 64 + u32::from(b'A') * 2),
            &4096u16.to_be_bytes(),
        )
        .unwrap();
    assert_eq!(
        text::width(&mut memory, &mut process, ports, Input::Character(b'A')).unwrap(),
        14
    );
    // Source corruption must not return a previously prepared strike.
    let handle = process.get_resource(&mut memory, *b"sfnt", 9295).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    memory.write(GuestPtr(data), &[0xff; 4]).unwrap();
    assert!(text::width(&mut memory, &mut process, ports, Input::Character(b'A')).is_err());
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn embedded_outline_draw_string_on_both_cpus_uses_gpu_clip_and_family_advances() {
    use systemless_future::{
        graphics::{Graphics, Rect},
        quickdraw::Ports,
        text,
    };
    let fork = resource(vec![
        (*b"FOND", 2020, None, family_resource(0, 0, 9295, true)),
        (
            *b"sfnt",
            9295,
            None,
            include_bytes!("../assets/fonts/NimbusSans-Regular.ttf").to_vec(),
        ),
    ]);
    let f = pef_program(&[0x4e800020], &[b"DrawString"], 128);
    let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    let mut before = vec![0; 320];
    for row in 0..32 {
        before[row * 10 + 8..row * 10 + 10].fill(0xa5);
    }
    app.runtime
        .memory
        .map(GuestPtr(0x52000000), before.clone(), Permissions::RW)
        .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x51000020),
        screen_bitmap: GuestPtr(0x51000000),
    };
    let mut bitmap = vec![0; 14];
    w32(&mut bitmap, 0, 0x52000000);
    w16(&mut bitmap, 4, 10);
    w16(&mut bitmap, 10, 32);
    w16(&mut bitmap, 12, 64);
    app.runtime
        .memory
        .write(ports.screen_bitmap, &bitmap)
        .unwrap();
    let port = GuestPtr(0x51000080);
    ports
        .open(&mut app.runtime.memory, &mut app.runtime.process, port)
        .unwrap();
    ports
        .clip_rect(&mut app.runtime.memory, Rect::new(8, 0, 40, 32))
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(port.0 + 68), &[7, 228, 0, 0xa5, 0, 0, 0, 14])
        .unwrap();
    app.runtime.process.ports = Some(ports);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 64, 32, GuestPtr(0x4000))).unwrap());
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x84], Permissions::RX)
        .unwrap();
    let string = app.info.toc_or_a5 + 64;
    app.runtime
        .memory
        .write(GuestPtr(string), &[3, b'A', 0x80, b' '])
        .unwrap();
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    let stack = STACK_BASE + 0x8000;
    let mut outputs = Vec::new();
    for architecture in [Architecture::PowerPc, Architecture::M68k] {
        app.runtime
            .memory
            .write(GuestPtr(0x52000000), &before)
            .unwrap();
        text::move_to(&mut app.runtime.memory, ports, 8, 22).unwrap();
        app.runtime
            .memory
            .write(GuestPtr(stack), &string.to_be_bytes())
            .unwrap();
        app.runtime.cpu = Cpu::new(
            architecture,
            if architecture == Architecture::PowerPc {
                gate
            } else {
                0x50000000
            },
            stack,
        );
        if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
            c.gpr[3] = string;
        }
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        let output = app
            .runtime
            .memory
            .read(GuestPtr(0x52000000), 320)
            .unwrap()
            .to_vec();
        assert!(output
            .chunks_exact(10)
            .any(|row| row[1..5].iter().any(|pixel| *pixel != 0)));
        for row in output.chunks_exact(10) {
            assert_eq!(row[0], 0);
            assert_eq!(&row[5..8], &[0; 3]);
            assert_eq!(&row[8..], &[0xa5; 2]);
        }
        assert_eq!(app.runtime.memory.u16(GuestPtr(port.0 + 50)).unwrap(), 29);
        assert_eq!(app.runtime.memory.u16(GuestPtr(port.0 + 48)).unwrap(), 22);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(port.0 + 71), 1).unwrap(),
            [0xa5]
        );
        outputs.push(output);
    }
    assert_eq!(outputs[0], outputs[1]);
}
