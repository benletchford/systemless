//! Shared Color Picker and `ColorTable` conversions.

/// How [`decode_color_table_clut`] handles an image-table `ColorSpec.value`
/// outside `0..256` when `ctFlags & 0x8000 == 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutOfRangeColorIndex {
    /// Leave the CLUT untouched for that entry (PowerPC adapter policy).
    Skip,
    /// Clamp the destination slot to index `255` (classic 68K adapter policy).
    ClampToLast,
}

/// Number of `ColorSpec` records decoded from a `ColorTable` with `ctSize`,
/// capped to the 256-entry 8-bit CLUT capacity.
#[inline]
pub(crate) fn color_table_entry_count(ct_size: u16) -> usize {
    usize::from(ct_size.min(255)) + 1
}

/// Decode raw big-endian `ColorSpec` bytes (`value`, `rgb.red`, `rgb.green`,
/// `rgb.blue` — 8 bytes per entry) onto `base_clut`.
///
/// Inside Macintosh Volume V (1986), pp. V-135 and V-142..V-143: when bit 15
/// of `ctFlags` is set (a `GDevice` table), entries are indexed by position
/// and `ColorSpec.value` holds private Color Manager client data; otherwise
/// each entry writes the slot named by `ColorSpec.value`.
#[inline]
pub(crate) fn decode_color_table_clut(
    flags: u16,
    entries: &[u8],
    base_clut: &[[u16; 3]; 256],
    out_of_range: OutOfRangeColorIndex,
) -> [[u16; 3]; 256] {
    let mut clut = *base_clut;
    if flags & 0x8000 != 0 {
        for (slot, entry) in entries.chunks_exact(8).take(256).enumerate() {
            clut[slot] = [
                u16::from_be_bytes([entry[2], entry[3]]),
                u16::from_be_bytes([entry[4], entry[5]]),
                u16::from_be_bytes([entry[6], entry[7]]),
            ];
        }
    } else {
        for entry in entries.chunks_exact(8).take(256) {
            let value = usize::from(u16::from_be_bytes([entry[0], entry[1]]));
            let index = if value < 256 {
                value
            } else {
                match out_of_range {
                    OutOfRangeColorIndex::Skip => continue,
                    OutOfRangeColorIndex::ClampToLast => 255,
                }
            };
            clut[index] = [
                u16::from_be_bytes([entry[2], entry[3]]),
                u16::from_be_bytes([entry[4], entry[5]]),
                u16::from_be_bytes([entry[6], entry[7]]),
            ];
        }
    }
    clut
}

pub(crate) fn hsl_to_rgb(hue: u16, saturation: u16, lightness: u16) -> [u16; 3] {
    fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
        if t < 0.0 {
            t += 1.0;
        } else if t > 1.0 {
            t -= 1.0;
        }

        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }
    let hue = f64::from(hue) / 65535.0;
    let saturation = f64::from(saturation) / 65535.0;
    let lightness = f64::from(lightness) / 65535.0;
    let (red, green, blue) = if saturation == 0.0 {
        (lightness, lightness, lightness)
    } else {
        let q = if lightness < 0.5 {
            lightness * (1.0 + saturation)
        } else {
            lightness + saturation - lightness * saturation
        };
        let p = 2.0 * lightness - q;
        (
            hue_to_rgb(p, q, hue + 1.0 / 3.0),
            hue_to_rgb(p, q, hue),
            hue_to_rgb(p, q, hue - 1.0 / 3.0),
        )
    };
    let to_word = |component: f64| -> u16 { (component.clamp(0.0, 1.0) * 65535.0).round() as u16 };
    [to_word(red), to_word(green), to_word(blue)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl2rgb_small_fract_endpoints_and_primaries() {
        for hue in [0, 0x1357, 0x5555, 0xaaaa, 0xffff] {
            assert_eq!(hsl_to_rgb(hue, 0, 0x2468), [0x2468; 3]);
            assert_eq!(hsl_to_rgb(hue, 0xffff, 0), [0; 3]);
            assert_eq!(hsl_to_rgb(hue, 0xffff, 0xffff), [0xffff; 3]);
        }
        assert_eq!(hsl_to_rgb(0, 0xffff, 0x7fff), [0xfffe, 0, 0]);
        assert_eq!(hsl_to_rgb(0xffff, 0xffff, 0x7fff), [0xfffe, 0, 0]);
        assert_eq!(hsl_to_rgb(0x5555, 0xffff, 0x7fff), [0, 0xfffe, 0]);
        assert_eq!(hsl_to_rgb(0xaaaa, 0xffff, 0x7fff), [0, 0, 0xfffe]);
        assert_eq!(hsl_to_rgb(0, 0xffff, 0x8000), [0xffff, 1, 1]);
    }

    fn push_spec(out: &mut Vec<u8>, value: u16, rgb: [u16; 3]) {
        out.extend_from_slice(&value.to_be_bytes());
        for channel in rgb {
            out.extend_from_slice(&channel.to_be_bytes());
        }
    }

    #[test]
    fn color_table_entry_count_caps_at_256() {
        assert_eq!(color_table_entry_count(0), 1);
        assert_eq!(color_table_entry_count(15), 16);
        assert_eq!(color_table_entry_count(255), 256);
        assert_eq!(color_table_entry_count(256), 256);
        assert_eq!(color_table_entry_count(u16::MAX), 256);
    }

    #[test]
    fn decode_color_table_clut_distinguishes_device_and_image_policies() {
        let base = std::array::from_fn(|i| [i as u16 * 17, i as u16 * 31, i as u16 * 43]);
        let mut entries = Vec::new();
        push_spec(&mut entries, 9, [0x1111, 0x2222, 0x3333]);
        push_spec(&mut entries, 300, [0x4444, 0x5555, 0x6666]);
        push_spec(&mut entries, 9, [0x7777, 0x8888, 0x9999]);

        let skipped =
            decode_color_table_clut(0x0000, &entries, &base, OutOfRangeColorIndex::Skip);
        assert_eq!(skipped[9], [0x7777, 0x8888, 0x9999]);
        assert_eq!(skipped[0], base[0]);
        assert_eq!(skipped[255], base[255]);

        let clamped =
            decode_color_table_clut(0x0000, &entries, &base, OutOfRangeColorIndex::ClampToLast);
        assert_eq!(clamped[9], [0x7777, 0x8888, 0x9999]);
        assert_eq!(clamped[255], [0x4444, 0x5555, 0x6666]);

        let device =
            decode_color_table_clut(0x8000, &entries, &base, OutOfRangeColorIndex::Skip);
        assert_eq!(device[0], [0x1111, 0x2222, 0x3333]);
        assert_eq!(device[1], [0x4444, 0x5555, 0x6666]);
        assert_eq!(device[2], [0x7777, 0x8888, 0x9999]);
        assert_eq!(device[9], base[9]);
    }

    #[test]
    fn classic_and_ppc_color_table_adapters_agree_on_valid_tables_and_keep_edge_policies() {
        use crate::loader::ppc::palettes::ppc_read_ctable_clut;
        use crate::memory::{GuestAddressSpace, MemoryBus};
        use crate::trap::test_helpers::setup;
        use ppc::PpcMemory;

        let (dispatcher, _cpu, mut bus) = setup();
        let base_clut: [[u16; 3]; 256] =
            std::array::from_fn(|i| [0x0100 + i as u16, 0x0200 + i as u16, 0x0300 + i as u16]);
        dispatcher.color_manager_clut.replace(base_clut);

        for flags in [0x0000u16, 0x8000u16] {
            let mut raw = Vec::new();
            raw.extend_from_slice(&0x1234_5678u32.to_be_bytes());
            raw.extend_from_slice(&flags.to_be_bytes());
            raw.extend_from_slice(&3u16.to_be_bytes());
            push_spec(&mut raw, 5, [0xaaaa, 0xbbbb, 0xcccc]);
            push_spec(&mut raw, 0, [0x1234, 0x5678, 0x9abc]);
            push_spec(&mut raw, 250, [0xfedc, 0xba98, 0x7654]);
            push_spec(&mut raw, 42, [0x0011, 0x0022, 0x0033]);

            let handle = 0x20_0000u32;
            let table = 0x20_0100u32;
            bus.write_long(handle, table);
            bus.write_bytes(table, &raw);

            let mut ppc_mem = GuestAddressSpace::new();
            ppc_mem.add_region(handle, vec![0; 0x1000]);
            ppc_mem.write_u32_be(handle, table).unwrap();
            ppc_mem.write_bytes(table, &raw).unwrap();

            let m68k_clut = dispatcher.read_port_clut(&bus, handle);
            let ppc_clut = ppc_read_ctable_clut(&mut ppc_mem, handle, &base_clut).unwrap();
            assert_eq!(m68k_clut, ppc_clut, "flags=0x{flags:04x}");
        }

        // PowerPC partial trailing out-of-range ColorSpec still succeeds when
        // its RGB words are unmapped, while a truncated in-range spec fails.
        let mut ppc_mem = GuestAddressSpace::new();
        let handle = 0x30_0000u32;
        let table = 0x30_1000u32;
        ppc_mem.add_region(handle, table.to_be_bytes().to_vec());
        let mut truncated = Vec::new();
        truncated.extend_from_slice(&0u32.to_be_bytes());
        truncated.extend_from_slice(&0u16.to_be_bytes());
        truncated.extend_from_slice(&1u16.to_be_bytes());
        push_spec(&mut truncated, 7, [0x1111, 0x2222, 0x3333]);
        truncated.extend_from_slice(&300u16.to_be_bytes());
        ppc_mem.add_region(table, truncated);
        let decoded = ppc_read_ctable_clut(&mut ppc_mem, handle, &base_clut).unwrap();
        assert_eq!(decoded[7], [0x1111, 0x2222, 0x3333]);

        ppc_mem.write_u16_be(table + 16, 8).unwrap();
        assert!(ppc_read_ctable_clut(&mut ppc_mem, handle, &base_clut).is_none());
    }
}

/// The nearest entry to `rgb` among the first `entry_count` entries of
/// `clut`, as an inverse table would choose it.
///
/// QuickDraw's inverse tables reserve exact white and black for the first and
/// last entries when those entries are white and black, rather than returning
/// an arbitrary duplicate colour-cube entry. During fade-to-black steps entry
/// 0 can temporarily equal black too; canonical black stays pinned to the last
/// entry in that collapsed case. Imaging With QuickDraw (1994), p. 4-82;
/// references/executor/src/quickdraw/qColorMgr.cpp (MakeITable).
pub(crate) fn nearest_color_index(clut: &[[u16; 3]; 256], rgb: [u16; 3], entry_count: usize) -> u8 {
    let entry_count = entry_count.clamp(1, 256);
    let terminal_index = entry_count - 1;
    if rgb == [0, 0, 0] && clut[terminal_index] == [0, 0, 0] {
        return terminal_index as u8;
    }
    if rgb == [0xFFFF, 0xFFFF, 0xFFFF] && clut[0] == [0xFFFF, 0xFFFF, 0xFFFF] {
        return 0;
    }
    if rgb == clut[terminal_index] {
        return terminal_index as u8;
    }
    if rgb == clut[0] {
        return 0;
    }
    if let Some(index) = clut[..entry_count].iter().position(|entry| *entry == rgb) {
        return index as u8;
    }
    let mut best_index = 0u8;
    let mut best_distance = u64::MAX;
    for (index, entry) in clut[..entry_count].iter().enumerate() {
        let dr = i64::from(entry[0]) - i64::from(rgb[0]);
        let dg = i64::from(entry[1]) - i64::from(rgb[1]);
        let db = i64::from(entry[2]) - i64::from(rgb[2]);
        let distance = (dr * dr + dg * dg + db * db) as u64;
        if distance < best_distance {
            best_distance = distance;
            best_index = index as u8;
        }
    }
    best_index
}

/// The parts of a pixel map that tell whether a port draws on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PixelMapLayout {
    pub(crate) base: u32,
    pub(crate) row_bytes: u32,
    pub(crate) pixel_size: u16,
}

/// The colour table that resolves RGB colours to a port's pixel values.
///
/// RGBForeColor and RGBBackColor place "the pixel value most closely
/// matching that color" in the port (Imaging With QuickDraw (1994), p. 4-70),
/// matched against the current device. While an offscreen graphics world is
/// current, that is the world's own GDevice, with the world's colour table
/// and inverse table (pp. 6-19--6-20 and 6-29). Its pixel values mean
/// whatever its own table says, not what the screen's says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortColorTable {
    /// An 8-bit port on the screen: the screen GDevice's inverse table, which
    /// follows its logical colour table rather than the animated video DAC.
    ScreenInverseTable,
    /// Another port on the screen: the screen's colour table.
    Screen,
    /// An offscreen port: its own pixel map's colour table.
    Own,
}

/// Which colour table resolves RGB colours for a port whose pixel map is
/// `port`, given the screen's.
pub(crate) fn port_color_table(port: PixelMapLayout, screen: PixelMapLayout) -> PortColorTable {
    if port != screen {
        PortColorTable::Own
    } else if port.pixel_size == 8 {
        PortColorTable::ScreenInverseTable
    } else {
        PortColorTable::Screen
    }
}

#[cfg(test)]
mod port_color_tests {
    use super::*;

    #[test]
    fn nearest_color_keeps_white_and_black_on_the_table_ends() {
        let mut clut = [[0x8000u16; 3]; 256];
        clut[0] = [0xFFFF; 3];
        clut[255] = [0; 3];
        clut[7] = [0; 3]; // a duplicate black
        clut[9] = [0, 0, 0x7FFF];
        assert_eq!(nearest_color_index(&clut, [0; 3], 256), 255);
        assert_eq!(nearest_color_index(&clut, [0xFFFF; 3], 256), 0);
        assert_eq!(nearest_color_index(&clut, [0, 0, 0x7FFF], 256), 9);
        assert_eq!(nearest_color_index(&clut, [0, 0, 0x7000], 256), 9);
        // A 4-bit table's last entry is 15.
        clut[15] = [0; 3];
        assert_eq!(nearest_color_index(&clut, [0; 3], 16), 15);
        // When a fade leaves both ends white, white stays on entry 0.
        clut[255] = [0xFFFF; 3];
        assert_eq!(nearest_color_index(&clut, [0xFFFF; 3], 256), 0);
    }

    #[test]
    fn only_ports_drawing_on_the_screen_use_its_table() {
        let screen = PixelMapLayout {
            base: 0x2a0_0000,
            row_bytes: 816,
            pixel_size: 8,
        };
        assert_eq!(
            port_color_table(screen, screen),
            PortColorTable::ScreenInverseTable
        );
        let offscreen = PixelMapLayout {
            base: 0x314_6df0,
            row_bytes: 160,
            pixel_size: 8,
        };
        assert_eq!(port_color_table(offscreen, screen), PortColorTable::Own);
        let deep = PixelMapLayout {
            pixel_size: 16,
            row_bytes: 1600,
            ..screen
        };
        assert_eq!(port_color_table(deep, deep), PortColorTable::Screen);
    }
}
