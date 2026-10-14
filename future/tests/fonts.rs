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
