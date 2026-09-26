use super::{
    blit_row, build_device_itable, build_pict_indexed_transfer_table, build_src_to_dst_table,
    clear_src_to_dst_table_cache_for_tests, closest_grayscale_luminance_index, draw_picture,
    dst_clip_row_spans, peek_initial_packbits_clut, picture_stream_len, read_color_table,
    try_blit_packbits_8bpp_src_copy_fast, try_blit_row_8bpp_src_copy_fast, DstClip, DstClipRegion,
    PictIndexedTransfer, PictureRegion, PixMapInfo,
};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::trap::dispatch::TrapDispatcher;

#[test]
fn extended_v2_header_maps_source_rect_into_draw_picture_destination() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let pic = 0x10_0000u32;
    let screen_base = 0x08_0000u32;
    let mut p = pic;
    bus.write_word(p, 0);
    p += 2;
    for value in [10u16, 25, 20, 35] {
        bus.write_word(p, value);
        p += 2;
    }
    for value in [0x0011u16, 0x02FF, 0x0C00, 0xFFFE, 0] {
        bus.write_word(p, value);
        p += 2;
    }
    for value in [0x0120_0000u32, 0x0120_0000] {
        bus.write_long(p, value);
        p += 4;
    }
    for value in [40u16, 100, 80, 140, 0, 0, 0x0031, 50, 110, 60, 120, 0x00FF] {
        bus.write_word(p, value);
        p += 2;
    }
    bus.write_word(pic, (p - pic) as u16);

    let clut = TrapDispatcher::standard_mac_8bpp_clut();
    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        10,
        10,
        (screen_base, 10, 10, 10, 8),
        &clut,
        0,
        None,
    );
    assert!(ok);
    let painted = bus.read_byte(screen_base + 3 * 10 + 3);
    assert_ne!(
        painted, 0,
        "high-resolution picture coordinates must reach the destination"
    );
    assert_eq!(bus.read_byte(screen_base), 0);
    assert_eq!(bus.read_byte(screen_base + 6 * 10 + 6), 0);
}

fn push_v2_relative_text(commands: &mut Vec<u8>, opcode: u16, deltas: &[u8], text: &[u8]) {
    super::recording_push_word(commands, opcode);
    commands.extend_from_slice(deltas);
    commands.push(text.len() as u8);
    commands.extend_from_slice(text);
    if !(deltas.len() + 1 + text.len()).is_multiple_of(2) {
        commands.push(0);
    }
}

fn push_v1_long_text(commands: &mut Vec<u8>, v: i16, h: i16, text: &[u8]) {
    commands.push(0x28);
    commands.extend_from_slice(&v.to_be_bytes());
    commands.extend_from_slice(&h.to_be_bytes());
    commands.push(text.len() as u8);
    commands.extend_from_slice(text);
}

fn finish_v1_picture(frame: (i16, i16, i16, i16), commands: &[u8]) -> Vec<u8> {
    let mut picture = vec![0; 10];
    for (index, value) in [frame.0, frame.1, frame.2, frame.3].into_iter().enumerate() {
        picture[2 + index * 2..4 + index * 2].copy_from_slice(&value.to_be_bytes());
    }
    picture.extend_from_slice(&[0x11, 0x01]);
    picture.extend_from_slice(commands);
    picture.push(0xFF);
    let size = u16::try_from(picture.len()).unwrap();
    picture[0..2].copy_from_slice(&size.to_be_bytes());
    picture
}

fn render_text_picture(picture: &[u8], width: u16, height: u16) -> Vec<u8> {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen = 0x08_0000;
    let pic = 0x10_0000;
    bus.write_bytes(pic, picture);
    bus.fill_zeros(screen, u32::from(width) * u32::from(height));
    let clut = TrapDispatcher::standard_mac_8bpp_clut();

    let (drawn, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        height as i16,
        width as i16,
        (screen, u32::from(width), width, height, 8),
        &clut,
        0,
        None,
    );

    assert!(drawn);
    bus.read_bytes(screen, usize::from(width) * usize::from(height))
}

#[test]
fn pict_v2_relative_text_uses_unsigned_deltas_from_previous_origins() {
    // Imaging With QuickDraw 1994, Appendix A, pp. A-6--A-7 defines
    // text deltas as 0..255, unlike signed ShortLine deltas. Text 1993,
    // p. 3-64 shows that the compressed delta base is the prior text
    // origin rather than the pen position after glyph advance.
    let frame = (0, 0, 320, 700);
    let mut relative = Vec::new();
    push_v2_relative_text(&mut relative, 0x002B, &[10, 0xFE], b"A");
    push_v2_relative_text(&mut relative, 0x0029, &[0xFE], b"B");
    push_v2_relative_text(&mut relative, 0x0029, &[0xF8], b"C");
    super::recording_push_long_text(&mut relative, 20, 620, b"D");
    push_v2_relative_text(&mut relative, 0x002A, &[0xFE], b"E");
    let relative = super::finish_recording(frame, relative);

    let mut absolute = Vec::new();
    super::recording_push_long_text(&mut absolute, 254, 10, b"A");
    super::recording_push_long_text(&mut absolute, 254, 264, b"B");
    super::recording_push_long_text(&mut absolute, 254, 512, b"C");
    super::recording_push_long_text(&mut absolute, 20, 620, b"D");
    super::recording_push_long_text(&mut absolute, 274, 620, b"E");
    let absolute = super::finish_recording(frame, absolute);

    let actual = render_text_picture(&relative, 700, 320);
    let expected = render_text_picture(&absolute, 700, 320);
    assert_eq!(actual, expected);
    for (left, right) in [(0, 100), (250, 350), (500, 600), (600, 680)] {
        assert!(expected
            .chunks_exact(700)
            .any(|row| row[left..right].contains(&255)));
    }
}

#[test]
fn pict_v1_dvtext_keeps_the_previous_text_origin() {
    let frame = (0, 0, 64, 320);
    let mut relative = Vec::new();
    push_v1_long_text(&mut relative, 20, 20, b"MMMMMMMMMMMM");
    relative.extend_from_slice(&[0x2A, 20, 1, b'X']);
    let relative = finish_v1_picture(frame, &relative);

    let mut absolute = Vec::new();
    push_v1_long_text(&mut absolute, 20, 20, b"MMMMMMMMMMMM");
    push_v1_long_text(&mut absolute, 40, 20, b"X");
    let absolute = finish_v1_picture(frame, &absolute);

    let actual = render_text_picture(&relative, 320, 64);
    let expected = render_text_picture(&absolute, 320, 64);
    assert_eq!(actual, expected);
    assert!(expected
        .chunks_exact(320)
        .skip(32)
        .any(|row| row[16..48].contains(&255)));
}

fn render_v1_styled_text_pair(face: u8, missing: bool) -> (Vec<u8>, Vec<u8>) {
    let glyph_advance = i16::from(
        crate::quickdraw::text::get_glyph(3, 9, 'A')
            .expect("bundled Geneva substitute must contain A")
            .0
            .advance,
    );
    let picture = |face: u8, split: bool, missing: bool| {
        let mut commands = Vec::new();
        commands.extend_from_slice(&[0x03, 0x00, 0x03]); // TxFont Geneva
        commands.extend_from_slice(&[0x04, face]); // TxFace
        commands.extend_from_slice(&[0x0D, 0x00, 0x09]); // TxSize 9
        if split {
            push_v1_long_text(&mut commands, 20, 20, b"A");
            push_v1_long_text(
                &mut commands,
                20,
                20 + glyph_advance
                    + i16::from(face & 1)
                    + if missing { 6 + i16::from(face & 1) } else { 0 },
                b"A",
            );
        } else {
            push_v1_long_text(
                &mut commands,
                20,
                20,
                if missing { b"A\x01A" } else { b"AA" },
            );
        }
        finish_v1_picture((0, 0, 48, 80), &commands)
    };

    (
        render_text_picture(&picture(face, false, missing), 80, 48),
        render_text_picture(&picture(face, true, missing), 80, 48),
    )
}

#[test]
fn pict_v1_bold_text_advances_each_glyph_by_style_width() {
    let (plain_run, plain_absolute) = render_v1_styled_text_pair(0, false);
    assert_eq!(plain_run, plain_absolute);
    assert!(plain_run.contains(&255));

    let (bold_run, bold_absolute) = render_v1_styled_text_pair(1, false);
    assert_eq!(bold_run, bold_absolute);
}

#[test]
fn pict_v1_bold_text_advances_missing_glyphs_by_style_width() {
    let (bold_missing_run, bold_missing_absolute) = render_v1_styled_text_pair(1, true);
    assert_eq!(bold_missing_run, bold_missing_absolute);
}

#[test]
fn pict_geneva9_bold_run_matches_classic_relative_text_origins() {
    const FIRST: &[u8] = b"This is your shooter.  There are many like it, ";
    const SECOND: &[u8] = b"but this one is yours.  Mind it well, cuz it is ";
    const THIRD: &[u8] = b"made of ";
    let frame = (0, 0, 96, 700);
    let text_state = |commands: &mut Vec<u8>| {
        super::recording_push_word(commands, 0x0003); // TxFont
        super::recording_push_word(commands, 3); // Geneva
        super::recording_push_word(commands, 0x0004); // TxFace
        commands.extend_from_slice(&[1, 0]); // bold and v2 padding
        super::recording_push_word(commands, 0x000D); // TxSize
        super::recording_push_word(commands, 9);
    };

    let mut relative = Vec::new();
    text_state(&mut relative);
    push_v2_relative_text(&mut relative, 0x002B, &[65, 56], FIRST);
    push_v2_relative_text(&mut relative, 0x0029, &[0xFE], SECOND);
    push_v2_relative_text(&mut relative, 0x0029, &[0xF8], THIRD);
    let relative = super::finish_recording(frame, relative);

    let mut continuous = Vec::new();
    text_state(&mut continuous);
    let mut text = Vec::new();
    text.extend_from_slice(FIRST);
    text.extend_from_slice(SECOND);
    text.extend_from_slice(THIRD);
    super::recording_push_long_text(&mut continuous, 56, 65, &text);
    let continuous = super::finish_recording(frame, continuous);

    let relative = render_text_picture(&relative, 700, 96);
    let continuous = render_text_picture(&continuous, 700, 96);
    assert_eq!(relative, continuous);
    assert!(relative.contains(&255));
}

#[test]
fn device_itable_matches_rom_propagation_samples() {
    let table = build_device_itable(&TrapDispatcher::standard_mac_8bpp_clut());

    assert_eq!(table[0x564], 130);
    assert_eq!(table[0x631], 137);
    assert_eq!(table[0x431], 173);
    assert_eq!(table[0x666], 129);
    assert_eq!(table[0x333], 172);
    assert_eq!(table[0x555], 251);
}

#[test]
fn device_itable_keeps_a_dark_shade_available_beside_exact_black() {
    let mut clut = [[0xFFFF; 3]; 256];
    clut[1] = [0x0000, 0x0000, 0x0000];
    clut[104] = [0x0F0F, 0x0A0A, 0x0F0F];

    let table = build_device_itable(&clut);

    assert_eq!(table[0x000], 104);
    assert_eq!(table[0x010], 104);
}

#[test]
fn color_table_uses_sparse_colorspec_values_as_source_indexes() {
    let mut bus = MacMemoryBus::new(1024);
    let mut pos = 0x100;
    bus.write_long(pos, 0x1234_5678);
    pos += 4;
    bus.write_word(pos, 0); // explicit ColorSpec.value indexes
    pos += 2;
    bus.write_word(pos, 2); // three entries
    pos += 2;
    for (value, rgb) in [
        (200u16, [0x1111, 0x2222, 0x3333]),
        (3u16, [0x4444, 0x5555, 0x6666]),
        (99u16, [0x7777, 0x8888, 0x9999]),
    ] {
        bus.write_word(pos, value);
        pos += 2;
        for component in rgb {
            bus.write_word(pos, component);
            pos += 2;
        }
    }

    let (end, colors, seed) = read_color_table(&bus, 0x100);

    assert_eq!(end, pos);
    assert_eq!(seed, 0x1234_5678);
    assert_eq!(colors[200], [0x1111, 0x2222, 0x3333]);
    assert_eq!(colors[3], [0x4444, 0x5555, 0x6666]);
    assert_eq!(colors[99], [0x7777, 0x8888, 0x9999]);
    assert_eq!(colors[0], [0, 0, 0]);
}

#[test]
fn grayscale_luminance_mapping_prefers_low_chroma_match() {
    let mut dst = [[0u16; 3]; 256];
    dst[10] = [0xE000, 0x0000, 0x0000];
    dst[20] = [0x3939, 0x2C2C, 0x3939];
    assert_eq!(closest_grayscale_luminance_index(0x3939, &dst), 20);
}

#[test]
fn dense_grayscale_source_uses_luminance_translation() {
    let mut src = [[0u16; 3]; 256];
    for (index, rgb) in src.iter_mut().enumerate() {
        let value = 0xFFFFu16.saturating_sub((index as u16) * 0x0101);
        *rgb = [value, value, value];
    }

    let mut dst = [[0u16; 3]; 256];
    dst[10] = [0xE000, 0x0000, 0x0000];
    dst[20] = [0x3939, 0x2C2C, 0x3939];
    dst[42] = [0x1111, 0x0202, 0x0000];

    let table = build_src_to_dst_table(&src, &dst);
    assert_eq!(table[198], 20);
}

#[test]
fn dense_grayscale_source_does_not_preserve_indices_on_system_palette() {
    let mut src = [[0u16; 3]; 256];
    for (index, rgb) in src.iter_mut().enumerate() {
        let value = 0xFFFFu16.saturating_sub((index as u16) * 0x0101);
        *rgb = [value, value, value];
    }

    let dst = TrapDispatcher::standard_mac_8bpp_clut();
    let table = build_src_to_dst_table(&src, &dst);

    // Canonical index 16 is orange in the system color cube, not gray.
    // A grayscale PICT must remap this entry instead of passing it through.
    assert_ne!(table[16], 16);
    let mapped = dst[table[16] as usize];
    assert_eq!(mapped[0], mapped[1]);
    assert_eq!(mapped[1], mapped[2]);
}

#[test]
fn exact_same_index_palette_entries_win_over_earlier_duplicates() {
    let mut src = [[0u16; 3]; 256];
    let mut dst = [[0u16; 3]; 256];
    let rgb = [0x2E2E, 0x0000, 0x3333];
    src[71] = rgb;
    dst[12] = rgb;
    dst[71] = rgb;

    let table = build_src_to_dst_table(&src, &dst);

    assert_eq!(table[71], 71);
}

#[test]
fn exact_palette_entry_at_another_index_wins_over_inverse_cell_seed() {
    let mut src = [[0u16; 3]; 256];
    let mut dst = [[0u16; 3]; 256];
    dst[5] = [0x2000, 0x0000, 0x3000];
    dst[12] = [0x2E2E, 0x0202, 0x3333];
    src[71] = dst[12];

    let table = build_src_to_dst_table(&src, &dst);

    assert_eq!(table[71], 12);
}

#[test]
fn eight_bit_device_matching_ignores_pict_rgb_low_bytes() {
    let mut src = [[0u16; 3]; 256];
    let mut dst = [[0u16; 3]; 256];
    src[14] = [0xFF0E, 0x2D0E, 0x890E];
    dst[73] = [0xFFFF, 0x2D2D, 0x8989];
    // A competing entry occupies the same 4-bit inverse-table cell but
    // does not display the requested 8-bit RGB value.
    dst[12] = [0xF000, 0x2000, 0x8000];

    let table = build_src_to_dst_table(&src, &dst);

    assert_eq!(table[14], 73);
}

#[test]
fn display_equivalent_palettes_preserve_authored_indices() {
    let mut src = [[0u16; 3]; 256];
    let mut dst = [[0u16; 3]; 256];
    src[14] = [0xFF0E, 0x2D0E, 0x890E];
    dst[14] = [0xFFFF, 0x2D2D, 0x8989];
    dst[73] = [0xFFFF, 0x2D2D, 0x8989];

    let table = build_src_to_dst_table(&src, &dst);

    assert_eq!(table[14], 14);
}

#[test]
fn standard_eight_bit_colors_use_rom_four_bit_color2index_mapping() {
    let src = TrapDispatcher::standard_mac_8bpp_clut();
    let mut dst = TrapDispatcher::standard_mac_4bpp_gworld_clut();
    let terminal = dst[15];
    dst[16..].fill(terminal);

    let table = build_src_to_dst_table(&src, &dst);

    for (source, destination) in [
        (0, 0),
        (1, 0),
        (7, 12),
        (16, 2),
        (42, 12),
        (64, 3),
        (128, 13),
        (214, 15),
        (215, 3),
        (225, 8),
        (235, 6),
        (245, 0),
        (255, 15),
    ] {
        assert_eq!(
            table[source], destination,
            "standard 8-bit source index {source}"
        );
    }
}

#[test]
fn src_to_dst_table_cache_reuses_identical_cluts() {
    clear_src_to_dst_table_cache_for_tests();

    let mut src = [[0u16; 3]; 256];
    let mut dst = [[0u16; 3]; 256];
    src[1] = [0x2222, 0x3333, 0x4444];
    dst[7] = [0x2222, 0x3333, 0x4444];

    let first = build_src_to_dst_table(&src, &dst);
    let second = build_src_to_dst_table(&src, &dst);

    assert_eq!(second, first);
}

#[test]
fn src_to_dst_table_cache_keys_on_destination_clut_contents() {
    clear_src_to_dst_table_cache_for_tests();

    let mut src = [[0u16; 3]; 256];
    src[1] = [0x2222, 0x3333, 0x4444];
    let mut first_dst = [[0u16; 3]; 256];
    first_dst[7] = [0x2222, 0x3333, 0x4444];
    let mut second_dst = first_dst;
    second_dst[9] = [0x2222, 0x3333, 0x4444];
    second_dst[7] = [0x1111, 0x1111, 0x1111];

    let first = build_src_to_dst_table(&src, &first_dst);
    let second = build_src_to_dst_table(&src, &second_dst);

    assert_ne!(first[1], second[1]);
}

#[test]
fn complex_dst_clip_row_spans_follow_region_edges() {
    let clip = DstClip::new(
        (0, 0, 5, 20),
        vec![DstClipRegion::complex(
            1,
            0,
            4,
            20,
            vec![vec![3, 8, 12, 15], vec![5, 10], vec![]],
        )],
    );

    assert_eq!(
        dst_clip_row_spans(Some(&clip), 1, 0, 20),
        vec![(3, 8), (12, 15)]
    );
    assert_eq!(dst_clip_row_spans(Some(&clip), 2, 0, 20), vec![(5, 10)]);
    assert!(dst_clip_row_spans(Some(&clip), 3, 0, 20).is_empty());
}

#[test]
fn eight_bit_src_copy_fast_path_respects_complex_dst_clip() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    bus.write_bytes(screen_base, &[0xEE; 8]);
    let pm = PixMapInfo {
        row_bytes: 8,
        bounds_top: 0,
        bounds_left: 0,
        bounds_bottom: 1,
        bounds_right: 8,
        pixel_size: 8,
        cmp_count: 1,
        pack_type: 0,
    };
    let mut src_to_dst = [0u8; 256];
    for (index, slot) in src_to_dst.iter_mut().enumerate() {
        *slot = 100u8.saturating_add(index as u8);
    }
    let clip = DstClip::new(
        (0, 0, 1, 8),
        vec![DstClipRegion::complex(0, 0, 1, 8, vec![vec![2, 5]])],
    );
    let mut scratch = Vec::new();

    assert!(try_blit_row_8bpp_src_copy_fast(
        &mut bus,
        &[1, 2, 3, 4, 5, 6, 7, 8],
        0,
        &pm,
        &src_to_dst,
        false,
        0,
        0,
        0,
        1,
        8,
        0,
        0,
        0,
        0,
        0,
        0,
        1,
        8,
        1.0,
        1.0,
        screen_base,
        8,
        8,
        1,
        8,
        None,
        Some(&clip),
        &mut scratch,
    ));

    assert_eq!(
        bus.read_bytes(screen_base, 8),
        vec![0xEE, 0xEE, 103, 104, 105, 0xEE, 0xEE, 0xEE]
    );
}

#[test]
fn eight_bit_src_copy_fast_path_packs_one_bit_destination_rows() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    bus.write_byte(screen_base, 0xFF);
    let pm = PixMapInfo {
        row_bytes: 8,
        bounds_top: 0,
        bounds_left: 0,
        bounds_bottom: 1,
        bounds_right: 8,
        pixel_size: 8,
        cmp_count: 1,
        pack_type: 0,
    };
    let mut src_to_dst = [0u8; 256];
    for (index, slot) in src_to_dst.iter_mut().enumerate() {
        *slot = if index % 2 == 0 { 0 } else { 255 };
    }
    let mut scratch = Vec::new();

    assert!(try_blit_row_8bpp_src_copy_fast(
        &mut bus,
        &[0, 1, 2, 3, 4, 5, 6, 7],
        0,
        &pm,
        &src_to_dst,
        false,
        0,
        0,
        0,
        1,
        8,
        0,
        0,
        0,
        0,
        0,
        0,
        1,
        8,
        1.0,
        1.0,
        screen_base,
        1,
        8,
        1,
        1,
        None,
        None,
        &mut scratch,
    ));

    assert_eq!(bus.read_byte(screen_base), 0b0101_0101);
}

#[test]
fn eight_bit_scaled_blit_fills_every_enlarged_destination_pixel() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    bus.write_bytes(screen_base, &[0xEE; 15]);
    let pm = PixMapInfo {
        row_bytes: 3,
        bounds_top: 0,
        bounds_left: 0,
        bounds_bottom: 2,
        bounds_right: 3,
        pixel_size: 8,
        cmp_count: 1,
        pack_type: 0,
    };
    let src_to_dst = std::array::from_fn(|index| index as u8);
    let indexed_transfer = std::array::from_fn(|index| PictIndexedTransfer::Write(index as u8));
    let device_clut = [[0u16; 3]; 256];
    let mut scratch = Vec::new();

    for (row, pixels) in [[1, 2, 3], [4, 5, 6]].iter().enumerate() {
        blit_row(
            &mut bus,
            pixels,
            0,
            &pm,
            &device_clut,
            &device_clut,
            &src_to_dst,
            true,
            &indexed_transfer,
            row as u32,
            0,
            0,
            2,
            3,
            0,
            0,
            0,
            0,
            0,
            0,
            3,
            5,
            1.0,
            1.0,
            screen_base,
            5,
            5,
            3,
            8,
            0,
            0,
            None,
            None,
            &mut scratch,
        );
    }

    assert_eq!(
        bus.read_bytes(screen_base, 15),
        vec![1, 1, 2, 3, 3, 4, 4, 5, 6, 6, 4, 4, 5, 6, 6]
    );
}

/// Draw a 12x4 8-bit source through `blit_row` under a PICT op mask
/// region (PackBitsRgn), returning the destination rows. The picture
/// frame is offset from the screen so picture-to-screen translation is
/// exercised: picture x = screen x + 3, picture y = screen y + 5.
fn draw_region_masked_rows(
    region: Option<&PictureRegion>,
    dst_clip: Option<&DstClip>,
    scrn_ps: u16,
) -> Vec<u8> {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let screen_rb = if scrn_ps == 8 { 16 } else { 2 };
    bus.write_bytes(screen_base, &[0u8; 16 * 4]);
    let pm = PixMapInfo {
        row_bytes: 12,
        bounds_top: 0,
        bounds_left: 0,
        bounds_bottom: 4,
        bounds_right: 12,
        pixel_size: 8,
        cmp_count: 1,
        pack_type: 0,
    };
    // Distinct non-zero pixels; on a 1-bit screen every non-zero index
    // is "black" (bit set) via the identity map + threshold.
    let rows: Vec<Vec<u8>> = (0..4u8)
        .map(|y| (0..12u8).map(|x| 1 + y * 12 + x).collect())
        .collect();
    let src_to_dst = std::array::from_fn(|index| index as u8);
    let indexed_transfer = std::array::from_fn(|index| PictIndexedTransfer::Write(index as u8));
    let device_clut = [[0u16; 3]; 256];
    let mut scratch = Vec::new();
    for (row, pixels) in rows.iter().enumerate() {
        blit_row(
            &mut bus,
            pixels,
            0,
            &pm,
            &device_clut,
            &device_clut,
            &src_to_dst,
            true,
            &indexed_transfer,
            row as u32,
            0,
            0,
            4,
            12,
            0,  // dst_top
            0,  // dst_left
            5,  // frame_top
            3,  // frame_left
            5,  // pic_dst_top
            3,  // pic_dst_left
            9,  // pic_dst_bottom
            15, // pic_dst_right
            1.0,
            1.0,
            screen_base,
            screen_rb,
            16,
            4,
            scrn_ps,
            255,
            0,
            region,
            dst_clip,
            &mut scratch,
        );
    }
    bus.read_bytes(screen_base, (screen_rb * 4) as usize)
}

/// Reference: what the per-pixel path writes for `draw_region_masked_rows`
/// on an 8-bit screen, using the very predicate it consults.
fn expected_region_masked_rows(region: Option<&PictureRegion>) -> Vec<u8> {
    let mut expected = vec![0u8; 16 * 4];
    for y in 0..4i32 {
        for x in 0..12i32 {
            let (pic_y, pic_x) = (y + 5, x + 3);
            if region.is_some_and(|clip| !clip.contains(pic_y, pic_x)) {
                continue;
            }
            expected[(y * 16 + x) as usize] = 1 + (y * 12 + x) as u8;
        }
    }
    expected
}

#[test]
fn region_masked_8bpp_rows_take_the_span_fast_path_and_match_the_pixel_predicate() {
    // A notched region in picture coordinates: rows 5-6 cover picture
    // columns 4..13, row 7 covers 3..6 and 9..15 (a hole), row 8 is
    // empty. Row 5's edges also leave a trailing unpaired edge, which
    // `contains` reads as "to the right edge of the box".
    let notched = PictureRegion {
        top: 5,
        left: 3,
        bottom: 8,
        right: 15,
        rows: vec![vec![4, 13], vec![4], vec![3, 6, 9]],
    };
    assert_eq!(
        draw_region_masked_rows(Some(&notched), None, 8),
        expected_region_masked_rows(Some(&notched)),
        "notched op region: fast path must match the per-pixel predicate"
    );
    // A rectangular region (no rows) is its bounding box.
    let boxed = PictureRegion {
        top: 6,
        left: 5,
        bottom: 8,
        right: 11,
        rows: Vec::new(),
    };
    assert_eq!(
        draw_region_masked_rows(Some(&boxed), None, 8),
        expected_region_masked_rows(Some(&boxed))
    );
    // Combined with a complex port dst_clip: both intersect.
    let dst_clip = DstClip::new(
        (0, 0, 4, 16),
        vec![DstClipRegion::complex(
            0,
            0,
            4,
            16,
            vec![vec![0, 16], vec![2, 8], vec![0, 16], vec![0, 16]],
        )],
    );
    let both = draw_region_masked_rows(Some(&notched), Some(&dst_clip), 8);
    let mut expected = expected_region_masked_rows(Some(&notched));
    for x in (0..2).chain(8..16) {
        expected[16 + x] = 0;
    }
    assert_eq!(both, expected, "op region and dst_clip must both apply");
    // Non-vacuous: something was drawn and something was masked.
    assert!(both.iter().any(|&b| b != 0));
    assert!(expected_region_masked_rows(None) != expected_region_masked_rows(Some(&notched)));
}

#[test]
fn region_masked_rows_are_admitted_to_the_row_fast_path() {
    // The eligibility gate used to refuse any op region, sending every
    // PackBitsRgn frame (EV Override's intro is a zoom sequence of them)
    // through the per-pixel loop. It must now be accepted, and it must
    // stay refused for scaling and non-srcCopy modes.
    let region = PictureRegion {
        top: 0,
        left: 0,
        bottom: 4,
        right: 12,
        rows: vec![vec![1, 5], vec![0, 12], vec![2, 3], vec![]],
    };
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let pm = PixMapInfo {
        row_bytes: 12,
        bounds_top: 0,
        bounds_left: 0,
        bounds_bottom: 4,
        bounds_right: 12,
        pixel_size: 8,
        cmp_count: 1,
        pack_type: 0,
    };
    let src_to_dst = std::array::from_fn(|index| index as u8);
    let row = [7u8; 12];
    let mut scratch = Vec::new();
    let taken = |bus: &mut MacMemoryBus, mode: u16, scale: f64, scratch: &mut Vec<u8>| {
        try_blit_row_8bpp_src_copy_fast(
            bus,
            &row,
            mode,
            &pm,
            &src_to_dst,
            true,
            1,
            0,
            0,
            4,
            12,
            0,
            0,
            0,
            0,
            0,
            0,
            4,
            12,
            scale,
            scale,
            0x08_0000,
            16,
            16,
            4,
            8,
            Some(&region),
            None,
            scratch,
        )
    };
    assert!(
        taken(&mut bus, 0, 1.0, &mut scratch),
        "srcCopy + region takes the row path"
    );
    assert!(
        !taken(&mut bus, 1, 1.0, &mut scratch),
        "srcOr still goes per-pixel"
    );
    assert!(
        !taken(&mut bus, 0, 2.0, &mut scratch),
        "scaling still goes per-pixel"
    );
    // Row 1 of the region is 0..12: whole row written; row 3 is empty.
    assert_eq!(bus.read_bytes(0x08_0000 + 16, 12), vec![7u8; 12]);
}

#[test]
fn one_bit_screen_region_masked_rows_match_the_pixel_predicate() {
    let notched = PictureRegion {
        top: 5,
        left: 3,
        bottom: 9,
        right: 15,
        rows: vec![vec![4, 13], vec![4], vec![3, 6, 9], vec![]],
    };
    let bits = draw_region_masked_rows(Some(&notched), None, 1);
    let expected = expected_region_masked_rows(Some(&notched));
    for y in 0..4usize {
        for x in 0..16usize {
            let bit = (bits[y * 2 + x / 8] >> (7 - (x % 8))) & 1;
            let inside = expected[y * 16 + x] != 0;
            assert_eq!(
                bit == 1,
                inside,
                "1-bit pixel ({y}, {x}) must follow the region"
            );
        }
    }
}

/// Draw a 20x4 1-bit source (bits from a fixed pattern, 3 bytes per
/// row) through `blit_row` onto an 8-bit 24x4 screen with fg 200 /
/// bg 30, in `mode`, under an optional op region; picture (x, y) =
/// screen (x + 3, y + 5) as in the 8-bit tests. Returns the screen.
fn draw_one_bit_source_rows(mode: u16, region: Option<&PictureRegion>) -> Vec<u8> {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    bus.write_bytes(screen_base, &[9u8; 24 * 4]);
    let pm = PixMapInfo {
        row_bytes: 3,
        bounds_top: 0,
        bounds_left: 0,
        bounds_bottom: 4,
        bounds_right: 20,
        pixel_size: 1,
        cmp_count: 1,
        pack_type: 0,
    };
    let rows: [[u8; 3]; 4] = [
        [0b1010_1100, 0b0011_1100, 0b1111_0000],
        [0b0101_0011, 0b1100_0011, 0b0000_1111],
        [0b1111_1111, 0b0000_0000, 0b1010_1010],
        [0b1000_0001, 0b0111_1110, 0b0101_0101],
    ];
    let src_to_dst = std::array::from_fn(|index| index as u8);
    let indexed_transfer = std::array::from_fn(|index| PictIndexedTransfer::Write(index as u8));
    let device_clut = [[0u16; 3]; 256];
    let mut scratch = Vec::new();
    for (row, bits) in rows.iter().enumerate() {
        blit_row(
            &mut bus,
            bits,
            mode,
            &pm,
            &device_clut,
            &device_clut,
            &src_to_dst,
            true,
            &indexed_transfer,
            row as u32,
            0,
            0,
            4,
            20,
            0,
            0,
            5,
            3,
            5,
            3,
            9,
            23,
            1.0,
            1.0,
            screen_base,
            24,
            24,
            4,
            8,
            200,
            30,
            region,
            None,
            &mut scratch,
        );
    }
    bus.read_bytes(screen_base, 24 * 4)
}

/// The per-pixel arm's answer for `draw_one_bit_source_rows`.
fn expected_one_bit_source_rows(mode: u16, region: Option<&PictureRegion>) -> Vec<u8> {
    let rows: [[u8; 3]; 4] = [
        [0b1010_1100, 0b0011_1100, 0b1111_0000],
        [0b0101_0011, 0b1100_0011, 0b0000_1111],
        [0b1111_1111, 0b0000_0000, 0b1010_1010],
        [0b1000_0001, 0b0111_1110, 0b0101_0101],
    ];
    let mut screen = vec![9u8; 24 * 4];
    for y in 0..4i32 {
        for x in 0..20i32 {
            let set = rows[y as usize][(x / 8) as usize] & (0x80 >> (x % 8)) != 0;
            let index = if set {
                200
            } else if mode == 0 {
                30
            } else {
                continue;
            };
            if region.is_some_and(|clip| !clip.contains(y + 5, x + 3)) {
                continue;
            }
            screen[(y * 24 + x) as usize] = index;
        }
    }
    screen
}

#[test]
fn one_bit_srccopy_source_rows_take_the_row_path_and_match_the_pixel_predicate() {
    let notched = PictureRegion {
        top: 5,
        left: 3,
        bottom: 9,
        right: 23,
        rows: vec![vec![4, 21], vec![6], vec![3, 9, 15], vec![]],
    };
    for region in [None, Some(&notched)] {
        assert_eq!(
            draw_one_bit_source_rows(0, region),
            expected_one_bit_source_rows(0, region),
            "srcCopy 1-bit rows (region {:?}) must match the per-pixel arm",
            region.is_some()
        );
    }
    // Non-srcCopy stays on the per-pixel arm (clear bits are skipped)
    // and must still be right.
    assert_eq!(
        draw_one_bit_source_rows(1, Some(&notched)),
        expected_one_bit_source_rows(1, Some(&notched))
    );
    // Non-vacuous: something was drawn with both indices.
    let drawn = draw_one_bit_source_rows(0, None);
    assert!(drawn.contains(&200) && drawn.contains(&30));
}

#[test]
fn packbits_src_copy_fast_path_decodes_only_visible_mapped_span() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let pict_row = 0x04_0000u32;
    bus.write_byte(pict_row, 9); // byte count: flag + 8 literal pixels
    bus.write_byte(pict_row + 1, 7); // literal run of 8 bytes
    bus.write_bytes(pict_row + 2, &[1, 2, 3, 4, 5, 6, 7, 8]);

    let screen_base = 0x08_0000u32;
    bus.write_bytes(screen_base, &[0xEE; 4]);
    let pm = PixMapInfo {
        row_bytes: 8,
        bounds_top: 0,
        bounds_left: 0,
        bounds_bottom: 1,
        bounds_right: 8,
        pixel_size: 8,
        cmp_count: 1,
        pack_type: 0,
    };
    let mut src_to_dst = [0u8; 256];
    for (index, slot) in src_to_dst.iter_mut().enumerate() {
        *slot = index.wrapping_add(10) as u8;
    }
    let end = try_blit_packbits_8bpp_src_copy_fast(
        &mut bus,
        pict_row,
        1,
        &pm,
        0,
        &src_to_dst,
        false,
        0,
        0,
        1,
        8,
        0,
        -2,
        0,
        0,
        0,
        0,
        1,
        8,
        1.0,
        1.0,
        screen_base,
        4,
        4,
        1,
        8,
        None,
        None,
    )
    .expect("simple clipped 8bpp PackBits srcCopy should use the direct fast path");

    assert_eq!(end, pict_row + 10);
    assert_eq!(bus.read_bytes(screen_base, 4), vec![13, 14, 15, 16]);
}

fn write_peekable_indexed_packbits_pict(
    bus: &mut MacMemoryBus,
    pic: u32,
    mode: u16,
    leading_fill_rect: bool,
) {
    bus.write_word(pic, 0);
    bus.write_word(pic + 2, 0);
    bus.write_word(pic + 4, 0);
    bus.write_word(pic + 6, 1);
    bus.write_word(pic + 8, 2);

    let mut p = pic + 10;
    bus.write_byte(p, 0x11);
    p += 1;
    bus.write_byte(p, 0x01);
    p += 1;
    bus.write_byte(p, 0x0E); // FgColor state opcode: safe to skip.
    p += 1;
    bus.write_long(p, 0x0000_00CD);
    p += 4;

    if leading_fill_rect {
        bus.write_byte(p, 0x34);
        p += 1;
        for value in [0i16, 0, 1, 1] {
            bus.write_word(p, value as u16);
            p += 2;
        }
    }

    bus.write_byte(p, 0x98); // PackBitsRect
    p += 1;
    bus.write_word(p, 0x8002); // PixMap rowBytes = 2
    p += 2;
    for value in [0i16, 0, 1, 2] {
        bus.write_word(p, value as u16);
        p += 2;
    }
    bus.write_word(p, 0); // version
    p += 2;
    bus.write_word(p, 0); // packType
    p += 2;
    bus.write_long(p, 0); // packSize
    p += 4;
    bus.write_long(p, 0x0048_0000); // hRes
    p += 4;
    bus.write_long(p, 0x0048_0000); // vRes
    p += 4;
    bus.write_word(p, 0); // pixelType
    p += 2;
    bus.write_word(p, 8); // pixelSize
    p += 2;
    bus.write_word(p, 1); // cmpCount
    p += 2;
    bus.write_word(p, 8); // cmpSize
    p += 2;
    bus.write_long(p, 0); // planeBytes
    p += 4;
    bus.write_long(p, 0); // pmTable
    p += 4;
    bus.write_long(p, 0); // pmReserved
    p += 4;

    bus.write_long(p, 0); // ctSeed
    p += 4;
    bus.write_word(p, 0x8000); // implicit ColorSpec indexes
    p += 2;
    bus.write_word(p, 1); // entries 0 and 1
    p += 2;
    for (index, rgb) in [
        (0u16, [0x1111, 0x2222, 0x3333]),
        (1u16, [0xAAAA, 0xBBBB, 0xCCCC]),
    ] {
        bus.write_word(p, index);
        p += 2;
        for component in rgb {
            bus.write_word(p, component);
            p += 2;
        }
    }

    for _ in 0..2 {
        for value in [0i16, 0, 1, 2] {
            bus.write_word(p, value as u16);
            p += 2;
        }
    }
    bus.write_word(p, mode);
    p += 2;
    bus.write_bytes(p, &[0, 1]);
    p += 2;
    bus.write_byte(p, 0xFF);
    p += 1;
    bus.write_word(pic, (p - pic) as u16);
}

#[test]
fn peek_initial_packbits_clut_accepts_simple_first_src_copy_image() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let pic = 0x10_0000u32;
    write_peekable_indexed_packbits_pict(&mut bus, pic, 0, false);

    let clut = peek_initial_packbits_clut(&bus, pic).expect("initial PackBitsRect CLUT");

    assert_eq!(clut[0], [0x1111, 0x2222, 0x3333]);
    assert_eq!(clut[1], [0xAAAA, 0xBBBB, 0xCCCC]);
}

#[test]
fn peek_initial_packbits_clut_rejects_non_src_copy_images() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let pic = 0x10_0000u32;
    write_peekable_indexed_packbits_pict(&mut bus, pic, 1, false);

    assert!(peek_initial_packbits_clut(&bus, pic).is_none());
}

#[test]
fn peek_initial_packbits_clut_rejects_after_prior_drawing_opcode() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let pic = 0x10_0000u32;
    write_peekable_indexed_packbits_pict(&mut bus, pic, 0, true);

    assert!(peek_initial_packbits_clut(&bus, pic).is_none());
}

#[test]
fn src_copy_transfer_table_is_direct_write_mapping() {
    let mut src_to_dst = [0u8; 256];
    for (index, slot) in src_to_dst.iter_mut().enumerate() {
        *slot = 255u8.saturating_sub(index as u8);
    }
    let src = [[0u16; 3]; 256];
    let dst = [[0u16; 3]; 256];

    let table = build_pict_indexed_transfer_table(0, &src, &src_to_dst, &dst, 1, 2);

    assert_eq!(table[0], PictIndexedTransfer::Write(255));
    assert_eq!(table[42], PictIndexedTransfer::Write(213));
    assert_eq!(table[255], PictIndexedTransfer::Write(0));
}

#[test]
fn four_bit_indexed_rows_fill_enlarged_destination_pixels() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    bus.write_bytes(screen_base, &[0xEE; 16]);

    let pm = PixMapInfo {
        row_bytes: 1,
        bounds_top: 0,
        bounds_left: 0,
        bounds_bottom: 2,
        bounds_right: 2,
        pixel_size: 4,
        cmp_count: 1,
        pack_type: 0,
    };
    let device_clut = [[0u16; 3]; 256];
    let mut src_to_dst = [0u8; 256];
    src_to_dst[1] = 10;
    src_to_dst[2] = 20;
    let transfer = build_pict_indexed_transfer_table(0, &[], &src_to_dst, &device_clut, 0, 0);
    let mut scratch = Vec::new();

    for (row, packed_pixels) in [(0, 0x12), (1, 0x21)] {
        blit_row(
            &mut bus,
            &[packed_pixels],
            0,
            &pm,
            &device_clut,
            &device_clut,
            &src_to_dst,
            false,
            &transfer,
            row,
            0,
            0,
            2,
            2,
            0,
            0,
            0,
            0,
            0,
            0,
            2,
            2,
            2.0,
            2.0,
            screen_base,
            4,
            4,
            4,
            8,
            0,
            0,
            None,
            None,
            &mut scratch,
        );
    }

    assert_eq!(
        bus.read_bytes(screen_base, 16),
        vec![
            10, 10, 20, 20, //
            10, 10, 20, 20, //
            20, 20, 10, 10, //
            20, 20, 10, 10,
        ]
    );
}

#[test]
fn indexed_src_copy_preserves_source_colors_on_direct_destinations() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let pm = PixMapInfo {
        row_bytes: 1,
        bounds_top: 0,
        bounds_left: 0,
        bounds_bottom: 1,
        bounds_right: 1,
        pixel_size: 8,
        cmp_count: 1,
        pack_type: 0,
    };
    let mut source_clut = vec![[0u16; 3]; 256];
    source_clut[5] = [0x7000, 0x8800, 0x9800];
    let mut device_clut = [[0u16; 3]; 256];
    device_clut[42] = [0xffff, 0, 0];
    let mut src_to_dst = [0u8; 256];
    src_to_dst[5] = 42;
    let transfer =
        build_pict_indexed_transfer_table(0, &source_clut, &src_to_dst, &device_clut, 0, 0);
    let mut scratch = Vec::new();

    blit_row(
        &mut bus,
        &[5],
        0,
        &pm,
        &source_clut,
        &device_clut,
        &src_to_dst,
        false,
        &transfer,
        0,
        0,
        0,
        1,
        1,
        0,
        0,
        0,
        0,
        0,
        0,
        1,
        1,
        1.0,
        1.0,
        screen_base,
        2,
        1,
        1,
        16,
        0,
        0,
        None,
        None,
        &mut scratch,
    );

    assert_eq!(
        bus.read_word(screen_base),
        ((0x7000u16 >> 11) << 10) | ((0x8800u16 >> 11) << 5) | (0x9800u16 >> 11)
    );
}

#[test]
fn packbitsrect_matching_ctseed_still_translates_when_tables_differ() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let screen_row_bytes = 8u32;
    bus.write_bytes(screen_base, &[0xEE; 8]);

    let mut device_clut = [[0u16; 3]; 256];
    device_clut[42] = [0xFFFF, 0, 0];

    let pic = 0x10_0000u32;
    let mut p = pic;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 1);
    p += 2;
    bus.write_word(p, 2);
    p += 2;

    bus.write_byte(p, 0x98); // PackBitsRect
    p += 1;
    bus.write_word(p, 0x8002); // PixMap rowBytes = 2
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 1);
    p += 2;
    bus.write_word(p, 2);
    p += 2;
    bus.write_word(p, 0); // version
    p += 2;
    bus.write_word(p, 0); // packType
    p += 2;
    bus.write_long(p, 0); // packSize
    p += 4;
    bus.write_long(p, 0x0048_0000); // hRes
    p += 4;
    bus.write_long(p, 0x0048_0000); // vRes
    p += 4;
    bus.write_word(p, 0); // pixelType
    p += 2;
    bus.write_word(p, 8); // pixelSize
    p += 2;
    bus.write_word(p, 1); // cmpCount
    p += 2;
    bus.write_word(p, 8); // cmpSize
    p += 2;
    bus.write_long(p, 0); // planeBytes
    p += 4;
    bus.write_long(p, 0); // pmTable
    p += 4;
    bus.write_long(p, 0); // pmReserved
    p += 4;

    bus.write_long(p, 8); // ctSeed matches destination seed, but table differs
    p += 4;
    bus.write_word(p, 0x8000);
    p += 2;
    bus.write_word(p, 1); // two ColorSpec entries
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 1);
    p += 2;
    bus.write_word(p, 0xFFFF);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 0);
    p += 2;

    // srcRect, dstRect, mode
    for _ in 0..2 {
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 0);
        p += 2;
        bus.write_word(p, 1);
        p += 2;
        bus.write_word(p, 2);
        p += 2;
    }
    bus.write_word(p, 0); // srcCopy
    p += 2;

    // rowBytes < 8: raw row data, two source-index-1 red pixels.
    bus.write_byte(p, 1);
    p += 1;
    bus.write_byte(p, 1);
    p += 1;
    bus.write_byte(p, 0xFF); // EndOfPicture
    p += 1;
    bus.write_word(pic, (p - pic) as u16);

    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        1,
        2,
        (screen_base, screen_row_bytes, 8, 1, 8),
        &device_clut,
        8,
        None,
    );

    assert!(ok);
    assert_eq!(
        bus.read_bytes(screen_base, 2),
        vec![42, 42],
        "matching ctSeed is not enough for identity mapping when ColorTable contents differ"
    );
}

#[test]
fn one_bit_packbitsrect_uses_destination_clut_black_and_white() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    bus.write_bytes(screen_base, &[0x42; 8]);

    let mut clut = [[0x7777u16; 3]; 256];
    clut[4] = [0x0000, 0x0000, 0x0000];
    clut[7] = [0xFFFF, 0xFFFF, 0xFFFF];

    let pic = 0x10_0000u32;
    bus.write_word(pic, 42); // picSize
    bus.write_word(pic + 2, 0); // frame top
    bus.write_word(pic + 4, 0); // frame left
    bus.write_word(pic + 6, 1); // frame bottom
    bus.write_word(pic + 8, 8); // frame right
    let mut p = pic + 10;
    bus.write_byte(p, 0x11);
    p += 1;
    bus.write_byte(p, 0x01);
    p += 1;
    bus.write_byte(p, 0x98); // PackBitsRect
    p += 1;
    bus.write_word(p, 1); // rowBytes < 8: unpacked
    p += 2;
    for value in [0i16, 0, 1, 8] {
        bus.write_word(p, value as u16);
        p += 2;
    }
    for _ in 0..2 {
        for value in [0i16, 0, 1, 8] {
            bus.write_word(p, value as u16);
            p += 2;
        }
    }
    bus.write_word(p, 0); // srcCopy
    p += 2;
    bus.write_byte(p, 0b1010_0000);
    p += 1;
    bus.write_byte(p, 0xFF); // EndOfPicture

    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        1,
        8,
        (screen_base, 8, 8, 1, 8),
        &clut,
        0,
        None,
    );

    assert!(ok);
    assert_eq!(bus.read_bytes(screen_base, 8), vec![4, 7, 4, 7, 7, 7, 7, 7]);
}

#[test]
fn eight_bit_packbitsrect_srcor_preserves_white_source_pixels() {
    // Imaging With QuickDraw 1994, p. 4-33: with colored pixels,
    // srcOr applies foreground color for black source pixels and leaves
    // destination pixels alone for white source pixels.
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    bus.write_bytes(screen_base, &[42; 16]);

    let mut clut = [[0x7777u16; 3]; 256];
    clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
    clut[42] = [0x1234, 0x5678, 0x9ABC];
    clut[255] = [0x0000, 0x0000, 0x0000];

    let pic = 0x10_0000u32;
    bus.write_word(pic, 0); // picSize, patched at the end for clarity.
    bus.write_word(pic + 2, 0); // frame top
    bus.write_word(pic + 4, 0); // frame left
    bus.write_word(pic + 6, 4); // frame bottom
    bus.write_word(pic + 8, 4); // frame right
    let mut p = pic + 10;
    bus.write_byte(p, 0x11); // VersionOp
    p += 1;
    bus.write_byte(p, 0x01); // PICT v1
    p += 1;
    bus.write_byte(p, 0x98); // PackBitsRect
    p += 1;

    bus.write_word(p, 0x8004); // PixMap rowBytes, 8bpp, unpacked (< 8).
    p += 2;
    for value in [0i16, 0, 4, 4] {
        bus.write_word(p, value as u16);
        p += 2;
    }
    bus.write_word(p, 0); // pmVersion
    p += 2;
    bus.write_word(p, 0); // packType
    p += 2;
    bus.write_long(p, 0); // packSize
    p += 4;
    bus.write_long(p, 0x0048_0000); // hRes
    p += 4;
    bus.write_long(p, 0x0048_0000); // vRes
    p += 4;
    bus.write_word(p, 0); // pixelType indexed
    p += 2;
    bus.write_word(p, 8); // pixelSize
    p += 2;
    bus.write_word(p, 1); // cmpCount
    p += 2;
    bus.write_word(p, 8); // cmpSize
    p += 2;
    bus.write_long(p, 0); // planeBytes
    p += 4;
    bus.write_long(p, 0); // pmTable
    p += 4;
    bus.write_long(p, 0); // pmReserved
    p += 4;

    bus.write_long(p, 0); // ctSeed
    p += 4;
    bus.write_word(p, 0x8000); // ctFlags: entries are implicit indexes.
    p += 2;
    bus.write_word(p, 1); // ctSize: entries 0 and 1.
    p += 2;
    for (value, rgb) in [
        (0u16, [0xFFFF, 0xFFFF, 0xFFFF]),
        (1u16, [0x0000, 0x0000, 0x0000]),
    ] {
        bus.write_word(p, value);
        p += 2;
        for component in rgb {
            bus.write_word(p, component);
            p += 2;
        }
    }

    for _ in 0..2 {
        for value in [0i16, 0, 4, 4] {
            bus.write_word(p, value as u16);
            p += 2;
        }
    }
    bus.write_word(p, 1); // srcOr
    p += 2;
    bus.write_bytes(
        p,
        &[
            1, 1, 1, 0, //
            1, 0, 0, 0, //
            1, 0, 0, 0, //
            0, 0, 0, 0,
        ],
    );
    p += 16;
    bus.write_byte(p, 0xFF); // EndOfPicture
    p += 1;
    bus.write_word(pic, (p - pic) as u16);

    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        4,
        4,
        (screen_base, 4, 4, 4, 8),
        &clut,
        0,
        None,
    );

    assert!(ok);
    assert_eq!(
        bus.read_bytes(screen_base, 16),
        vec![
            255, 255, 255, 42, //
            255, 42, 42, 42, //
            255, 42, 42, 42, //
            42, 42, 42, 42,
        ]
    );
}

#[test]
fn indexed_transfer_table_precomputes_srcor_black_white_actions() {
    let mut src_clut = [[0u16; 3]; 256];
    src_clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
    src_clut[1] = [0x0000, 0x0000, 0x0000];
    src_clut[2] = [0x8000, 0x8000, 0x8000];

    let mut dst_clut = [[0x7777u16; 3]; 256];
    dst_clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
    dst_clut[42] = [0x8000, 0x8000, 0x8000];
    dst_clut[255] = [0x0000, 0x0000, 0x0000];

    let mut src_to_dst = [0u8; 256];
    src_to_dst[0] = 0;
    src_to_dst[1] = 255;
    src_to_dst[2] = 42;

    let table = build_pict_indexed_transfer_table(1, &src_clut, &src_to_dst, &dst_clut, 255, 0);

    assert_eq!(table[0], PictIndexedTransfer::Skip);
    assert_eq!(table[1], PictIndexedTransfer::Write(255));
    assert!(
        matches!(table[2], PictIndexedTransfer::Write(_)),
        "non-white source colors should be resolved once into the transfer table"
    );
}

#[test]
fn indexed_src_copy_ignores_foreground_and_background_colors() {
    let mut src_clut = [[0u16; 3]; 256];
    src_clut[1] = [0xFFFF, 0x0000, 0x0000];

    let mut dst_clut = [[0x7777u16; 3]; 256];
    dst_clut[7] = [0x0000, 0xFFFF, 0x0000];
    dst_clut[42] = [0xFFFF, 0x0000, 0x0000];
    dst_clut[99] = [0x0000, 0x0000, 0xFFFF];

    let mut src_to_dst = [0u8; 256];
    src_to_dst[1] = 42;

    let table = build_pict_indexed_transfer_table(0, &src_clut, &src_to_dst, &dst_clut, 7, 99);

    assert_eq!(
        table[1],
        PictIndexedTransfer::Write(42),
        "indexed srcCopy should copy the source ColorTable color, not tint it through fg/bg"
    );
}

#[test]
fn color_packbitsrect_to_one_bit_destination_maps_to_black_or_white() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    bus.write_byte(screen_base, 0);

    let pic = 0x10_0000u32;
    bus.write_word(pic, 0);
    bus.write_word(pic + 2, 0);
    bus.write_word(pic + 4, 0);
    bus.write_word(pic + 6, 1);
    bus.write_word(pic + 8, 3);
    let mut p = pic + 10;

    bus.write_byte(p, 0x98); // PackBitsRect
    p += 1;
    bus.write_word(p, 0x8003); // PixMap rowBytes = 3
    p += 2;
    for value in [0i16, 0, 1, 3] {
        bus.write_word(p, value as u16);
        p += 2;
    }
    bus.write_word(p, 0); // version
    p += 2;
    bus.write_word(p, 0); // packType
    p += 2;
    bus.write_long(p, 0); // packSize
    p += 4;
    bus.write_long(p, 0x0048_0000); // hRes
    p += 4;
    bus.write_long(p, 0x0048_0000); // vRes
    p += 4;
    bus.write_word(p, 0); // pixelType
    p += 2;
    bus.write_word(p, 8); // pixelSize
    p += 2;
    bus.write_word(p, 1); // cmpCount
    p += 2;
    bus.write_word(p, 8); // cmpSize
    p += 2;
    bus.write_long(p, 0); // planeBytes
    p += 4;
    bus.write_long(p, 0); // pmTable
    p += 4;
    bus.write_long(p, 0); // pmReserved
    p += 4;

    bus.write_long(p, 0); // ctSeed
    p += 4;
    bus.write_word(p, 0x8000); // ctFlags: ColorSpec values are indices
    p += 2;
    bus.write_word(p, 2); // ctSize: entries 0..2
    p += 2;
    for (index, [r, g, b]) in [
        (0u16, [0x0000, 0x0000, 0x0000]), // black
        (1u16, [0xFFFF, 0xFFFF, 0x0000]), // yellow, closer to white in 1bpp
        (2u16, [0xFFFF, 0xFFFF, 0xFFFF]), // white
    ] {
        bus.write_word(p, index);
        p += 2;
        bus.write_word(p, r);
        p += 2;
        bus.write_word(p, g);
        p += 2;
        bus.write_word(p, b);
        p += 2;
    }

    for _ in 0..2 {
        for value in [0i16, 0, 1, 3] {
            bus.write_word(p, value as u16);
            p += 2;
        }
    }
    bus.write_word(p, 0); // srcCopy
    p += 2;
    bus.write_byte(p, 0); // black source pixel
    p += 1;
    bus.write_byte(p, 1); // yellow should map to white on a 1bpp destination
    p += 1;
    bus.write_byte(p, 2); // white source pixel
    p += 1;
    bus.write_byte(p, 0xFF); // EndOfPicture
    p += 1;
    bus.write_word(pic, (p - pic) as u16);

    let clut = TrapDispatcher::standard_mac_8bpp_clut();
    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        1,
        3,
        (screen_base, 1, 8, 1, 1),
        &clut,
        0,
        None,
    );

    assert!(ok);
    assert_eq!(
            bus.read_byte(screen_base) & 0b1110_0000,
            0b1000_0000,
            "indexed color PICT pixels drawn into 1bpp must resolve against the black/white destination, not the full 8bpp CLUT"
        );
}

#[test]
fn directbitsrect_packtype4_uses_pixmap_rowbytes_for_word_byte_counts() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let screen_w: u16 = 80;
    let screen_h: u16 = 2;
    let row_bytes = u32::from(screen_w);
    bus.write_bytes(
        screen_base,
        &vec![0xAA; (row_bytes * u32::from(screen_h)) as usize],
    );

    let pic = 0x10_0000u32;
    let mut p = pic + 10;
    bus.write_byte(p, 0x11);
    p += 1; // VersionOp
    bus.write_byte(p, 0x02);
    p += 1; // PICT v2
    bus.write_byte(p, 0xFF);
    p += 1; // v2 version padding
    bus.write_byte(p, 0x00);
    p += 1; // align first word opcode

    bus.write_word(p, 0x009A);
    p += 2; // DirectBitsRect
    bus.write_long(p, 0x0000_00FF);
    p += 4; // baseAddr
    bus.write_word(p, 0x8000 | 320);
    p += 2; // PixMap rowBytes: 80 pixels * 4 bytes
    for value in [0i16, 0, 1, 80] {
        bus.write_word(p, value as u16);
        p += 2;
    }
    bus.write_word(p, 0);
    p += 2; // version
    bus.write_word(p, 4);
    p += 2; // packType 4: component PackBits, red first
    bus.write_long(p, 0);
    p += 4; // packSize
    bus.write_long(p, 0x0048_0000);
    p += 4; // hRes
    bus.write_long(p, 0x0048_0000);
    p += 4; // vRes
    bus.write_word(p, 16);
    p += 2; // direct pixelType
    bus.write_word(p, 32);
    p += 2; // pixelSize
    bus.write_word(p, 3);
    p += 2; // cmpCount: RGB only
    bus.write_word(p, 8);
    p += 2; // cmpSize
    bus.write_long(p, 0);
    p += 4; // planeBytes
    bus.write_long(p, 0);
    p += 4; // pmTable
    bus.write_long(p, 0);
    p += 4; // pmReserved

    for _ in 0..2 {
        for value in [0i16, 0, 1, 80] {
            bus.write_word(p, value as u16);
            p += 2;
        }
    }
    bus.write_word(p, 64);
    p += 2; // mode used by recorded direct-pixel PICTs

    // rowBytes is 320, so the scanline byte count is a word even though
    // the decoded 24-bit RGB planes are only 240 bytes.
    bus.write_word(p, 6);
    p += 2;
    for component in [0x12u8, 0x34, 0x56] {
        bus.write_byte(p, 0xB1); // repeat 80 bytes
        p += 1;
        bus.write_byte(p, component);
        p += 1;
    }

    bus.write_word(p, 0x0034);
    p += 2; // fillRect; proves the stream is still synchronized
    bus.write_word(p, 1);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 2);
    p += 2;
    bus.write_word(p, 80);
    p += 2;
    bus.write_word(p, 0x00FF);
    p += 2; // EndOfPicture

    bus.write_word(pic, (p - pic) as u16);
    bus.write_word(pic + 2, 0);
    bus.write_word(pic + 4, 0);
    bus.write_word(pic + 6, 2);
    bus.write_word(pic + 8, 80);

    let mut clut = [[0x8000u16, 0x8000, 0x8000]; 256];
    clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
    clut[42] = [0x1212, 0x3434, 0x5656];
    clut[255] = [0x0000, 0x0000, 0x0000];

    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        screen_h as i16,
        screen_w as i16,
        (screen_base, row_bytes, screen_w, screen_h, 8),
        &clut,
        0,
        None,
    );

    assert!(ok);
    assert_eq!(bus.read_byte(screen_base), 42);
    assert_eq!(bus.read_byte(screen_base + row_bytes), 255);
}

#[test]
fn directbitsrect_maps_rgbdirect_colors_into_indexed_and_16bpp_destinations() {
    for (pixel_size, expected_indexed) in [(2u16, Some(0x60u8)), (4, Some(0x12)), (16, None)] {
        for source_depth in [16u16, 32] {
            let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
            let screen_base = 0x08_0000u32;
            let pic = 0x10_0000u32;
            let mut p = pic + 10;
            bus.write_word(pic, 0);
            for (offset, value) in [(2, 0u16), (4, 0), (6, 1), (8, 2)] {
                bus.write_word(pic + offset, value);
            }
            bus.write_byte(p, 0x11);
            p += 1;
            bus.write_byte(p, 0x02);
            p += 1;
            bus.write_byte(p, 0xFF);
            p += 1;
            bus.write_byte(p, 0);
            p += 1;
            bus.write_word(p, 0x009A);
            p += 2;
            bus.write_long(p, 0x0000_00FF);
            p += 4;
            let row_bytes = if source_depth == 16 { 4 } else { 8 };
            bus.write_word(p, 0x8000 | row_bytes);
            p += 2;
            for value in [0i16, 0, 1, 2] {
                bus.write_word(p, value as u16);
                p += 2;
            }
            bus.write_word(p, 0);
            p += 2;
            bus.write_word(p, 1);
            p += 2;
            bus.write_long(p, 0);
            p += 4;
            bus.write_long(p, 0x0048_0000);
            p += 4;
            bus.write_long(p, 0x0048_0000);
            p += 4;
            bus.write_word(p, 16);
            p += 2;
            bus.write_word(p, source_depth);
            p += 2;
            bus.write_word(p, if source_depth == 32 { 3 } else { 1 });
            p += 2;
            bus.write_word(p, if source_depth == 32 { 8 } else { 16 });
            p += 2;
            bus.write_long(p, 0);
            p += 4;
            bus.write_long(p, 0);
            p += 4;
            bus.write_long(p, 0);
            p += 4;
            for _ in 0..2 {
                for value in [0i16, 0, 1, 2] {
                    bus.write_word(p, value as u16);
                    p += 2;
                }
            }
            bus.write_word(p, 0);
            p += 2;
            let row: &[u8] = if source_depth == 16 {
                &[0x7C, 0x00, 0x03, 0xE0]
            } else {
                &[0xFF, 0, 0, 0xFF, 0, 0, 0, 0]
            };
            bus.write_bytes(p, row);
            p += row.len() as u32;
            bus.write_word(p, 0x00FF);
            p += 2;
            bus.write_word(pic, (p - pic) as u16);

            let mut clut = [[0u16; 3]; 256];
            clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
            clut[1] = [0xFFFF, 0, 0];
            clut[2] = [0, 0xFFFF, 0];
            if pixel_size < 8 {
                clut[(1usize << pixel_size) - 1] = [0, 0, 0];
            }
            let (ok, _) = draw_picture(
                &mut bus,
                pic,
                0,
                0,
                1,
                2,
                (
                    screen_base,
                    if pixel_size == 16 { 4 } else { 1 },
                    2,
                    1,
                    pixel_size,
                ),
                &clut,
                0,
                None,
            );
            assert!(ok);
            if let Some(expected) = expected_indexed {
                assert_eq!(
                    bus.read_byte(screen_base),
                    expected,
                    "{source_depth}-bit DirectBitsRect into {pixel_size}-bit indexed"
                );
            } else {
                assert_eq!(bus.read_word(screen_base), 0x7c00);
                assert_eq!(bus.read_word(screen_base + 2), 0x03e0);
            }
        }
    }
}

/// fillRect ($0x34) honors FillPat (0x0A) rather than PnPat (0x09) —
/// otherwise it would be indistinguishable from paintRect ($0x31).
/// Imaging With QuickDraw 1994, Appendix A, A-7.
#[test]
fn pict_fillrect_honors_fillpat_not_pnpat() {
    // 32×32 8bpp framebuffer at 0x08_0000.
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let screen_w: u16 = 32;
    let screen_h: u16 = 32;
    let row_bytes = screen_w as u32;
    // Pre-fill with a sentinel so an untouched pixel is distinguishable.
    bus.write_bytes(
        screen_base,
        &vec![0x42; (row_bytes * screen_h as u32) as usize],
    );

    // Build a PICT v1 at 0x10_0000.
    let pic = 0x10_0000u32;
    let mut p = pic + 10;
    // version 1 (short opcodes)
    bus.write_byte(p, 0x11);
    p += 1; // versionOp
    bus.write_byte(p, 0x01);
    p += 1; // v1
            // PnPat (0x09): 8 bytes all 0x00 — set bits→fg, clear→bg.
            // An all-zero pattern fills with bg_idx (0 = white).
    bus.write_byte(p, 0x09);
    p += 1;
    bus.fill_zeros(p, 8);
    p += 8;
    // FillPat (0x0A): 8 bytes all 0xFF — fills with fg_idx
    // (255 = black).
    bus.write_byte(p, 0x0A);
    p += 1;
    bus.write_bytes(p, &[0xFFu8; 8]);
    p += 8;
    // fillRect (0x34): rect = (0, 0, 32, 32)
    bus.write_byte(p, 0x34);
    p += 1;
    bus.write_word(p, 0);
    p += 2; // top
    bus.write_word(p, 0);
    p += 2; // left
    bus.write_word(p, 32);
    p += 2; // bottom
    bus.write_word(p, 32);
    p += 2; // right
            // EndPic
    bus.write_byte(p, 0xFF);

    // picFrame = (0, 0, 32, 32)
    bus.write_word(pic, (p - pic + 1) as u16); // picSize (approx)
    bus.write_word(pic + 2, 0);
    bus.write_word(pic + 4, 0);
    bus.write_word(pic + 6, 32);
    bus.write_word(pic + 8, 32);

    let clut = TrapDispatcher::standard_mac_8bpp_clut();
    let (_ok, _clut) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        32,
        32,
        (screen_base, row_bytes, screen_w, screen_h, 8),
        &clut,
        0,
        None,
    );

    // A representative interior pixel must be fg_idx (255 = black).
    // If fillRect were using PnPat (all-zeros), it would leak as
    // bg_idx (0 = white).
    let sample = bus.read_byte(screen_base + 8 * row_bytes + 8);
    assert_eq!(
        sample, 255,
        "fillRect must use FillPat (all-0xFF → fg=255), not PnPat \
             (all-0x00 → bg=0). Got 0x{:02X}.",
        sample,
    );
}

#[test]
fn pict_v2_opcolor_advances_to_following_opcode() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let screen_w: u16 = 16;
    let screen_h: u16 = 16;
    let row_bytes = screen_w as u32;
    bus.write_bytes(
        screen_base,
        &vec![0x42; (row_bytes * screen_h as u32) as usize],
    );

    let pic = 0x10_0000u32;
    let mut p = pic + 10;
    bus.write_byte(p, 0x11);
    p += 1; // versionOp
    bus.write_byte(p, 0x02);
    p += 1; // v2
    bus.write_byte(p, 0xFF);
    p += 1; // v2 version padding
    bus.write_byte(p, 0x00);
    p += 1; // align first word opcode
    bus.write_word(p, 0x001F);
    p += 2; // OpColor
    bus.write_word(p, 0x1111);
    p += 2;
    bus.write_word(p, 0x2222);
    p += 2;
    bus.write_word(p, 0x3333);
    p += 2;
    bus.write_word(p, 0x0034);
    p += 2; // fillRect
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 16);
    p += 2;
    bus.write_word(p, 16);
    p += 2;
    bus.write_word(p, 0x00FF);
    p += 2; // EndOfPicture

    bus.write_word(pic, (p - pic) as u16);
    bus.write_word(pic + 2, 0);
    bus.write_word(pic + 4, 0);
    bus.write_word(pic + 6, 16);
    bus.write_word(pic + 8, 16);

    let clut = TrapDispatcher::standard_mac_8bpp_clut();
    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        16,
        16,
        (screen_base, row_bytes, screen_w, screen_h, 8),
        &clut,
        0,
        None,
    );

    assert!(ok, "v2 OpColor should be skipped, not stop the PICT stream");
    assert_eq!(bus.read_byte(screen_base + 8 * row_bytes + 8), 255);
}

#[test]
fn pict_v2_rgb_colors_map_foreground_and_background_through_destination_clut() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let screen_w = 4u16;
    let screen_h = 4u16;
    let row_bytes = u32::from(screen_w);
    bus.write_bytes(
        screen_base,
        &vec![0xEE; (row_bytes * u32::from(screen_h)) as usize],
    );

    let pic = 0x10_0000u32;
    let mut p = pic + 10;
    bus.write_byte(p, 0x11);
    p += 1; // VersionOp
    bus.write_byte(p, 0x02);
    p += 1; // PICT v2
    bus.write_byte(p, 0xFF);
    p += 1; // version padding
    bus.write_byte(p, 0x00);
    p += 1; // align first word opcode

    bus.write_word(p, 0x001A);
    p += 2; // RGBFgCol
    for component in [0x1234u16, 0x5678, 0x9ABC] {
        bus.write_word(p, component);
        p += 2;
    }
    bus.write_word(p, 0x0034);
    p += 2; // fillRect with the default all-set FillPat
    for value in [0u16, 0, 2, 4] {
        bus.write_word(p, value);
        p += 2;
    }

    bus.write_word(p, 0x001B);
    p += 2; // RGBBkCol
    for component in [0xDEADu16, 0xBEEF, 0x1111] {
        bus.write_word(p, component);
        p += 2;
    }
    bus.write_word(p, 0x000A);
    p += 2; // FillPat
    bus.fill_zeros(p, 8); // clear pattern bits select the background color
    p += 8;
    bus.write_word(p, 0x0034);
    p += 2; // fillRect
    for value in [2u16, 0, 4, 4] {
        bus.write_word(p, value);
        p += 2;
    }
    bus.write_word(p, 0x00FF);
    p += 2; // EndOfPicture

    bus.write_word(pic, (p - pic) as u16);
    bus.write_word(pic + 2, 0);
    bus.write_word(pic + 4, 0);
    bus.write_word(pic + 6, screen_h);
    bus.write_word(pic + 8, screen_w);

    let mut clut = [[0x8000u16; 3]; 256];
    clut[42] = [0x1234, 0x5678, 0x9ABC];
    clut[77] = [0xDEAD, 0xBEEF, 0x1111];
    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        screen_h as i16,
        screen_w as i16,
        (screen_base, row_bytes, screen_w, screen_h, 8),
        &clut,
        0,
        None,
    );

    assert!(ok);
    assert_eq!(bus.read_bytes(screen_base, 8), vec![42; 8]);
    assert_eq!(bus.read_bytes(screen_base + 2 * row_bytes, 8), vec![77; 8]);
}

#[test]
fn pict_v1_reserved_shape_opcode_advances_to_following_opcode() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let screen_w: u16 = 16;
    let screen_h: u16 = 16;
    let row_bytes = screen_w as u32;
    bus.write_bytes(
        screen_base,
        &vec![0x42; (row_bytes * screen_h as u32) as usize],
    );

    let pic = 0x10_0000u32;
    let mut p = pic + 10;
    bus.write_byte(p, 0x11);
    p += 1; // versionOp
    bus.write_byte(p, 0x01);
    p += 1; // v1
    bus.write_byte(p, 0x35);
    p += 1; // reserved rect-family opcode: 8 bytes of data
    bus.write_bytes(p, &[0xAA; 8]);
    p += 8;
    bus.write_byte(p, 0x34);
    p += 1; // fillRect
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 0);
    p += 2;
    bus.write_word(p, 16);
    p += 2;
    bus.write_word(p, 16);
    p += 2;
    bus.write_byte(p, 0xFF);
    p += 1; // EndOfPicture

    bus.write_word(pic, (p - pic) as u16);
    bus.write_word(pic + 2, 0);
    bus.write_word(pic + 4, 0);
    bus.write_word(pic + 6, 16);
    bus.write_word(pic + 8, 16);

    let clut = TrapDispatcher::standard_mac_8bpp_clut();
    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        16,
        16,
        (screen_base, row_bytes, screen_w, screen_h, 8),
        &clut,
        0,
        None,
    );

    assert!(
        ok,
        "v1 reserved shape opcodes should skip their data, not stop the PICT stream"
    );
    assert_eq!(bus.read_byte(screen_base + 8 * row_bytes + 8), 255);
}

/// fillPoly ($0x74) samples FillPat per pixel — alternating row pattern
/// (rows 0,2,4,6 = 0xFF / rows 1,3,5,7 = 0x00) produces horizontal
/// stripes of fg_idx / bg_idx.
#[test]
fn pict_fillpoly_honors_fillpat_striped_pattern() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let screen_w: u16 = 32;
    let screen_h: u16 = 32;
    let row_bytes = screen_w as u32;
    bus.write_bytes(
        screen_base,
        &vec![0x42; (row_bytes * screen_h as u32) as usize],
    );
    let pic = 0x10_0000u32;
    let mut p = pic + 10;
    bus.write_byte(p, 0x11);
    p += 1; // VersionOp
    bus.write_byte(p, 0x01);
    p += 1; // v1
            // FillPat: alternating rows 0xFF / 0x00 → horizontal stripes.
    bus.write_byte(p, 0x0A);
    p += 1;
    for row in 0..8 {
        bus.write_byte(p, if row % 2 == 0 { 0xFF } else { 0x00 });
        p += 1;
    }
    // fillPoly ($0x74) — inline polySize(2) + bbox(8) + N*(v,h)(4)
    bus.write_byte(p, 0x74);
    p += 1;
    // polySize = 10 (header) + 4 verts × 4 bytes = 26
    let poly_size: u16 = 10 + 4 * 4;
    bus.write_word(p, poly_size);
    p += 2;
    bus.write_word(p, 4);
    p += 2; // bbox.top
    bus.write_word(p, 4);
    p += 2; // bbox.left
    bus.write_word(p, 28);
    p += 2; // bbox.bottom
    bus.write_word(p, 28);
    p += 2; // bbox.right
            // square verts
    for &(v, h) in &[(4i16, 4i16), (4, 28), (28, 28), (28, 4)] {
        bus.write_word(p, v as u16);
        p += 2;
        bus.write_word(p, h as u16);
        p += 2;
    }
    bus.write_byte(p, 0xFF); // EndPic

    bus.write_word(pic, (p - pic + 1) as u16);
    bus.write_word(pic + 2, 0);
    bus.write_word(pic + 4, 0);
    bus.write_word(pic + 6, 32);
    bus.write_word(pic + 8, 32);

    let clut = TrapDispatcher::standard_mac_8bpp_clut();
    let _ = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        32,
        32,
        (screen_base, row_bytes, screen_w, screen_h, 8),
        &clut,
        0,
        None,
    );

    // Interior columns: sample two adjacent rows. With the stripe
    // pattern, even rows must land on an fg stripe, odd rows on
    // a bg stripe (or vice-versa). Assert they differ rather than
    // coupling to pattern phase, which depends on dy mod 8 within
    // the polygon bbox.
    let x = 10u32;
    let mut saw_fg = false;
    let mut saw_bg = false;
    for dy in 6..16u32 {
        let val = bus.read_byte(screen_base + dy * row_bytes + x);
        if val == 255 {
            saw_fg = true;
        }
        if val == 0 {
            saw_bg = true;
        }
    }
    assert!(saw_fg, "striped fillPoly must produce some fg_idx pixels");
    assert!(saw_bg, "striped fillPoly must produce some bg_idx pixels");
}

/// fillOval ($0x54) samples FillPat per pixel. Striped FillPat
/// (alternating rows 0xFF / 0x00) must produce both fg_idx and bg_idx
/// pixels inside the oval, not solid fg_idx.
#[test]
fn pict_filloval_honors_fillpat_striped_pattern() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let screen_w: u16 = 32;
    let screen_h: u16 = 32;
    let row_bytes = screen_w as u32;
    bus.write_bytes(
        screen_base,
        &vec![0x42; (row_bytes * screen_h as u32) as usize],
    );
    let pic = 0x10_0000u32;
    let mut p = pic + 10;
    bus.write_byte(p, 0x11);
    p += 1;
    bus.write_byte(p, 0x01);
    p += 1;
    // FillPat: alternating rows 0xFF / 0x00.
    bus.write_byte(p, 0x0A);
    p += 1;
    for row in 0..8 {
        bus.write_byte(p, if row % 2 == 0 { 0xFF } else { 0x00 });
        p += 1;
    }
    // fillOval ($0x54): rect = (2, 2, 30, 30)
    bus.write_byte(p, 0x54);
    p += 1;
    bus.write_word(p, 2);
    p += 2;
    bus.write_word(p, 2);
    p += 2;
    bus.write_word(p, 30);
    p += 2;
    bus.write_word(p, 30);
    p += 2;
    bus.write_byte(p, 0xFF);

    bus.write_word(pic, (p - pic + 1) as u16);
    bus.write_word(pic + 2, 0);
    bus.write_word(pic + 4, 0);
    bus.write_word(pic + 6, 32);
    bus.write_word(pic + 8, 32);

    let clut = TrapDispatcher::standard_mac_8bpp_clut();
    let _ = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        32,
        32,
        (screen_base, row_bytes, screen_w, screen_h, 8),
        &clut,
        0,
        None,
    );

    // Sample a vertical column through the oval's interior; expect
    // both fg and bg stripes to appear.
    let x = 16u32;
    let mut saw_fg = false;
    let mut saw_bg = false;
    for dy in 6..22u32 {
        let val = bus.read_byte(screen_base + dy * row_bytes + x);
        if val == 255 {
            saw_fg = true;
        }
        if val == 0 {
            saw_bg = true;
        }
    }
    assert!(saw_fg, "striped fillOval must produce some fg_idx pixels");
    assert!(saw_bg, "striped fillOval must produce some bg_idx pixels");
}

#[test]
fn pict_bitsrect_decodes_unpacked_four_bit_pixmap() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let screen_base = 0x08_0000u32;
    let pic = 0x10_0000u32;
    let mut p = pic + 10;

    bus.write_byte(p, 0x00); // NOP, aligns the version opcode.
    p += 1;
    bus.write_byte(p, 0x11); // VersionOp.
    p += 1;
    bus.write_byte(p, 0x02);
    p += 1;
    bus.write_byte(p, 0xFF);
    p += 1;
    bus.write_word(p, 0x0090); // BitsRect with unpacked PixMap data.
    p += 2;
    bus.write_word(p, 0x8002); // PixMap flag and two bytes per row.
    p += 2;
    for value in [0i16, 0, 1, 4] {
        bus.write_word(p, value as u16);
        p += 2;
    }
    bus.write_word(p, 0); // pmVersion.
    p += 2;
    bus.write_word(p, 1); // packType: unpacked.
    p += 2;
    bus.write_long(p, 0); // packSize.
    p += 4;
    bus.write_long(p, 0x0048_0000); // hRes.
    p += 4;
    bus.write_long(p, 0x0048_0000); // vRes.
    p += 4;
    bus.write_word(p, 0); // indexed pixel type.
    p += 2;
    bus.write_word(p, 4); // pixelSize.
    p += 2;
    bus.write_word(p, 1); // cmpCount.
    p += 2;
    bus.write_word(p, 4); // cmpSize.
    p += 2;
    for _ in 0..3 {
        bus.write_long(p, 0); // planeBytes, pmTable, pmReserved.
        p += 4;
    }

    bus.write_long(p, 0); // ctSeed.
    p += 4;
    bus.write_word(p, 0); // explicit ColorSpec indexes.
    p += 2;
    bus.write_word(p, 1); // two entries.
    p += 2;
    for (index, component) in [(0u16, 0u16), (1, 0xFFFF)] {
        bus.write_word(p, index);
        p += 2;
        for _ in 0..3 {
            bus.write_word(p, component);
            p += 2;
        }
    }
    for _ in 0..2 {
        for value in [0i16, 0, 1, 4] {
            bus.write_word(p, value as u16);
            p += 2;
        }
    }
    bus.write_word(p, 0); // srcCopy.
    p += 2;
    bus.write_bytes(p, &[0x01, 0x10]);
    p += 2;
    bus.write_word(p, 0x00FF); // EndOfPicture.
    p += 2;

    bus.write_word(pic, (p - pic) as u16);
    for (offset, value) in [(2, 0u16), (4, 0), (6, 1), (8, 4)] {
        bus.write_word(pic + offset, value);
    }
    let mut clut = [[0u16; 3]; 256];
    clut[7] = [0xFFFF; 3];

    let (ok, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        1,
        4,
        (screen_base, 4, 4, 1, 8),
        &clut,
        0,
        None,
    );

    assert!(ok);
    assert_eq!(bus.read_bytes(screen_base, 4), vec![0, 7, 7, 0]);
    assert_eq!(
        picture_stream_len(&bus.read_bytes(pic, (p - pic) as usize)),
        Some((p - pic) as usize)
    );
}

#[test]
fn pict_reserved_opcode_length_overflow_stops_without_panicking() {
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let pic = 0x10_0000u32;
    for (offset, value) in [(2, 0u16), (4, 0), (6, 1), (8, 1)] {
        bus.write_word(pic + offset, value);
    }
    let mut p = pic + 10;
    bus.write_byte(p, 0x00);
    p += 1;
    bus.write_byte(p, 0x11);
    p += 1;
    bus.write_byte(p, 0x02);
    p += 1;
    bus.write_byte(p, 0xFF);
    p += 1;
    bus.write_word(p, 0x8100);
    p += 2;
    bus.write_long(p, u32::MAX);
    p += 4;
    bus.write_word(pic, (p - pic) as u16);

    let clut = [[0u16; 3]; 256];
    let result = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        1,
        1,
        (0x08_0000, 1, 1, 1, 8),
        &clut,
        0,
        None,
    );

    assert!(result.0);
}

#[test]
fn recorded_round_rect_honors_ovsize_when_scaled() {
    // Imaging With QuickDraw (1994), Appendix A, p. A-8: OvSize
    // supplies the corner oval for the rounded-rectangle opcode family.
    let mut commands = Vec::new();
    super::recording_push_round_rect(&mut commands, 0x0041, (0, 0, 40, 40), 10, 10);
    let picture = super::finish_recording((0, 0, 40, 40), commands);
    let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
    let pic = 0x10_0000;
    let screen = 0x08_0000;
    bus.write_bytes(pic, &picture);
    bus.fill_zeros(screen, 60 * 60);
    let mut clut = [[0u16; 3]; 256];
    clut[0] = [0xFFFF; 3];

    let (drawn, _) = draw_picture(
        &mut bus,
        pic,
        0,
        0,
        50,
        50,
        (screen, 60, 60, 60, 8),
        &clut,
        0,
        None,
    );

    assert!(drawn);
    assert_eq!(bus.read_byte(screen + 25 * 60 + 25), 255);
    assert_eq!(bus.read_byte(screen), 0);
    assert_eq!(bus.read_byte(screen + 60 + 1), 0);
    assert_eq!(bus.read_byte(screen + 6 * 60 + 6), 255);
}
