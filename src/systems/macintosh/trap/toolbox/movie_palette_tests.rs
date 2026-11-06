use super::super::dispatch::TrapDispatcher;
use super::super::test_helpers::setup;
use crate::memory::{MacMemoryBus, MemoryBus};

fn classic_movie_palette_fixture() -> (TrapDispatcher, MacMemoryBus, u32, u32, u32) {
    let (disp, _, mut bus) = setup();
    let (port, handle, pixmap, table_handle, table, pixels) =
        (0x300100, 0x300200, 0x300300, 0x300400, 0x300500, 0x310000);
    bus.write_word(port + 6, 0xC000);
    bus.write_long(port + 2, handle);
    bus.write_long(handle, pixmap);
    bus.write_long(pixmap, pixels);
    bus.write_word(pixmap + 4, 0x8000 | 4);
    bus.write_word(pixmap + 10, 1);
    bus.write_word(pixmap + 12, 4);
    bus.write_word(pixmap + 32, 8);
    bus.write_long(pixmap + 42, table_handle);
    bus.write_long(table_handle, table);
    (disp, bus, port, pixels, table)
}

#[test]
fn classic_movie_palette_uses_device_positions_and_live_entry_edits() {
    let (disp, mut bus, port, pixels, table) = classic_movie_palette_fixture();
    let colors = [[0u8, 0, 0], [240, 16, 8], [8, 32, 224], [16, 200, 32]];
    bus.write_word(table + 4, 0x8000);
    bus.write_word(table + 6, 3);
    for (index, color) in colors.iter().enumerate() {
        let entry = table + 8 + index as u32 * 8;
        // Device ColorSpec.value carries flags/client identity, not a pixel index.
        bus.write_word(entry, 0);
        for (channel, &value) in color.iter().enumerate() {
            bus.write_word(entry + 2 + channel as u32 * 2, u16::from(value) * 257);
        }
    }
    let rgb = [colors[1], colors[2], colors[3], colors[0]].concat();
    disp.blit_rgb_into_port(&mut bus, port, (0, 0, 1, 4), 4, 1, &rgb);
    assert_eq!(bus.read_bytes(pixels, 4), [1, 2, 3, 0]);
    // Swap two colours without a seed change; every draw consults the live table.
    for (index, color) in [(1, colors[2]), (2, colors[1])] {
        for (channel, value) in color.into_iter().enumerate() {
            bus.write_word(
                table + 8 + index * 8 + 2 + channel as u32 * 2,
                u16::from(value) * 257,
            );
        }
    }
    disp.blit_rgb_into_port(&mut bus, port, (0, 0, 1, 4), 4, 1, &rgb);
    assert_eq!(bus.read_bytes(pixels, 4), [2, 1, 3, 0]);
}

#[test]
fn classic_movie_palette_uses_logical_fallback_and_explicit_color_indices() {
    let (disp, mut bus, port, pixels, table) = classic_movie_palette_fixture();
    disp.color_manager_clut
        .set_entry(3, [0x1010, 0xC8C8, 0x2020]);
    disp.device_clut.set_entry(3, [0, 0, 0]);
    bus.write_word(table + 4, 0);
    bus.write_word(table + 6, 0);
    bus.write_word(table + 8, 7);
    for (channel, value) in [200u8, 16, 32].into_iter().enumerate() {
        bus.write_word(table + 10 + channel as u32 * 2, u16::from(value) * 257);
    }
    disp.blit_rgb_into_port(
        &mut bus,
        port,
        (0, 0, 1, 2),
        2,
        1,
        &[200, 16, 32, 16, 200, 32],
    );
    assert_eq!(bus.read_bytes(pixels, 2), [7, 3]);
    // A nil table uses the same live logical Color Manager fallback.
    bus.write_long(0x300300 + 42, 0);
    disp.blit_rgb_into_port(&mut bus, port, (0, 0, 1, 1), 1, 1, &[16, 200, 32]);
    assert_eq!(bus.read_byte(pixels), 3);
}
