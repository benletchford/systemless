//! Reading `PixPat` records, shared by the 68K trap path and the PowerPC
//! import path.
//!
//! A `PixPat` (Imaging With QuickDraw (1994), p. 4-58) reaches QuickDraw in
//! two forms. A compiled 'ppat' resource keeps `patMap` and `patData` as byte
//! offsets from the start of the record, with the pattern map's colour table
//! as an offset too (pp. 4-103--4-104). A record built by NewPixPat, GetPixPat
//! or MakeRGBPat keeps live handles there instead. This module reads either
//! form into a [`PixPatImage`] that names where the pixels and colour table
//! are; each caller maps the colours to its own destination.

use super::color::{color_table_entry_count, decode_color_table_clut, OutOfRangeColorIndex};
use crate::guest_procedure::GuestProcedureMemory;

/// Which form of `PixPat` record a [`PixPatImage`] came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PixPatForm {
    /// A compiled 'ppat' resource: offsets from the record.
    Compiled,
    /// Live `PixMapHandle` and pixel-data `Handle` fields.
    Live,
}

/// Where a pixel pattern's image lives, read from a `PixPat` record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PixPatImage {
    pub(crate) form: PixPatForm,
    pub(crate) row_bytes: u32,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) pixel_size: u16,
    /// The first byte of the pixel image.
    pub(crate) data_ptr: u32,
    /// The pattern map's colour table record, or 0 when it has none (direct
    /// pixels, or a live map whose `pmTable` is NIL).
    pub(crate) table_ptr: u32,
}

/// A compiled pattern map, rowBytes through pmTable: (rowBytes, bounds,
/// pixelSize, pmTable offset).
type MapFields = (u32, i16, i16, i16, i16, u16, u32);

/// The fields of a compiled 'ppat' pattern map at `map`. A raw resource stores
/// the map without `baseAddr` (rowBytes at +0); a GetPixPat-expanded record can
/// hold a full PixMap with `baseAddr` first (rowBytes at +4). `valid` decides
/// which one this record holds, the raw layout first.
fn compiled_map_fields<M: GuestProcedureMemory + ?Sized>(
    memory: &mut M,
    map: u32,
    valid: impl Fn(&MapFields) -> bool,
) -> Option<MapFields> {
    let mut read = |base: u32| -> MapFields {
        let word = |memory: &mut M, offset: u32| {
            memory
                .procedure_read_u16(map.wrapping_add(base + offset))
                .unwrap_or(0)
        };
        (
            u32::from(word(memory, 0) & 0x3fff),
            word(memory, 2) as i16,
            word(memory, 4) as i16,
            word(memory, 6) as i16,
            word(memory, 8) as i16,
            word(memory, 28),
            memory
                .procedure_read_u32(map.wrapping_add(base + 38))
                .unwrap_or(0),
        )
    };
    let raw = read(0);
    if valid(&raw) {
        return Some(raw);
    }
    let live = read(4);
    valid(&live).then_some(live)
}

/// Whether `pat_map` and `pat_data` are the offsets of a compiled 'ppat'
/// resource rather than live handles.
pub(crate) fn compiled_offsets(pat_map: u32, pat_data: u32) -> bool {
    (28..=0x10000).contains(&pat_map) && (pat_map + 50..=0x200000).contains(&pat_data)
}

/// The image of the compiled 'ppat' record at `record`, whose pattern map
/// fields `valid` accepts.
pub(crate) fn compiled_image<M: GuestProcedureMemory + ?Sized>(
    memory: &mut M,
    record: u32,
    valid: impl Fn(&MapFields) -> bool,
) -> Option<PixPatImage> {
    let pat_map = memory.procedure_read_u32(record.wrapping_add(2))?;
    let pat_data = memory.procedure_read_u32(record.wrapping_add(6))?;
    if !compiled_offsets(pat_map, pat_data) {
        return None;
    }
    let (row_bytes, top, left, bottom, right, pixel_size, table_offset) =
        compiled_map_fields(memory, record.checked_add(pat_map)?, valid)?;
    Some(PixPatImage {
        form: PixPatForm::Compiled,
        row_bytes,
        width: i32::from(right) - i32::from(left),
        height: i32::from(bottom) - i32::from(top),
        pixel_size,
        data_ptr: record.checked_add(pat_data)?,
        table_ptr: if table_offset == 0 {
            0
        } else {
            record.checked_add(table_offset)?
        },
    })
}

/// The image of the live `PixPat` record at `record`: its `patMap` PixMap
/// handle and `patData` handle, dereferenced.
pub(crate) fn live_image<M: GuestProcedureMemory + ?Sized>(
    memory: &mut M,
    record: u32,
) -> Option<PixPatImage> {
    let map_handle = memory.procedure_read_u32(record.wrapping_add(2))?;
    let map = memory
        .procedure_read_u32(map_handle)
        .filter(|map| *map != 0)?;
    let row_bytes = u32::from(memory.procedure_read_u16(map.checked_add(4)?)? & 0x3fff);
    let top = memory.procedure_read_u16(map.checked_add(6)?)? as i16;
    let left = memory.procedure_read_u16(map.checked_add(8)?)? as i16;
    let bottom = memory.procedure_read_u16(map.checked_add(10)?)? as i16;
    let right = memory.procedure_read_u16(map.checked_add(12)?)? as i16;
    let pixel_size = memory.procedure_read_u16(map.checked_add(32)?)?;
    let table_handle = memory.procedure_read_u32(map.checked_add(42)?)?;
    let data_handle = memory.procedure_read_u32(record.wrapping_add(6))?;
    let data_ptr = memory.procedure_read_u32(data_handle)?;
    let table_ptr = match table_handle {
        0 => 0,
        handle => memory.procedure_read_u32(handle).unwrap_or(0),
    };
    Some(PixPatImage {
        form: PixPatForm::Live,
        row_bytes,
        width: i32::from(right) - i32::from(left),
        height: i32::from(bottom) - i32::from(top),
        pixel_size,
        data_ptr,
        table_ptr,
    })
}

/// The entry count of the colour table record at `table`: ctSize + 1.
pub(crate) fn color_table_count<M: GuestProcedureMemory + ?Sized>(
    memory: &mut M,
    table: u32,
) -> Option<usize> {
    Some(usize::from(memory.procedure_read_u16(table.wrapping_add(6))?) + 1)
}

/// `base` with the colour table record at `table` applied, at most 256
/// entries, through the shared [`decode_color_table_clut`]: a device table
/// (ctFlags bit 15) keeps its pixel values implicit in each entry's position,
/// and an entry whose value names no 8-bit slot is skipped. Imaging With
/// QuickDraw (1994), pp. 4-56--4-57.
pub(crate) fn read_color_table<M: GuestProcedureMemory + ?Sized>(
    memory: &mut M,
    table: u32,
    base: [[u16; 3]; 256],
) -> [[u16; 3]; 256] {
    let flags = memory
        .procedure_read_u16(table.wrapping_add(4))
        .unwrap_or(0);
    let count = memory
        .procedure_read_u16(table.wrapping_add(6))
        .map_or(0, color_table_entry_count);
    let mut entries = Vec::with_capacity(count * 8);
    for ordinal in 0..count as u32 {
        let entry = table.wrapping_add(8 + ordinal * 8);
        let (Some(first), Some(second)) = (
            memory.procedure_read_u32(entry),
            memory.procedure_read_u32(entry.wrapping_add(4)),
        ) else {
            break;
        };
        entries.extend_from_slice(&first.to_be_bytes());
        entries.extend_from_slice(&second.to_be_bytes());
    }
    decode_color_table_clut(flags, &entries, &base, OutOfRangeColorIndex::Skip)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guest memory from address 0.
    struct Bytes(Vec<u8>);

    impl Bytes {
        fn word(&mut self, at: u32, value: u16) {
            self.0[at as usize..at as usize + 2].copy_from_slice(&value.to_be_bytes());
        }
        fn long(&mut self, at: u32, value: u32) {
            self.0[at as usize..at as usize + 4].copy_from_slice(&value.to_be_bytes());
        }
    }

    impl GuestProcedureMemory for Bytes {
        fn procedure_read_u8(&mut self, address: u32) -> Option<u8> {
            self.0.get(address as usize).copied()
        }
        fn procedure_read_u16(&mut self, address: u32) -> Option<u16> {
            let at = address as usize;
            Some(u16::from_be_bytes(self.0.get(at..at + 2)?.try_into().ok()?))
        }
        fn procedure_read_u32(&mut self, address: u32) -> Option<u32> {
            let at = address as usize;
            Some(u32::from_be_bytes(self.0.get(at..at + 4)?.try_into().ok()?))
        }
    }

    fn any_indexed(map: &MapFields) -> bool {
        map.0 != 0 && map.3 > map.1 && map.4 > map.2 && matches!(map.5, 1 | 2 | 4 | 8)
    }

    /// A compiled 8-by-8, 2-bit 'ppat' record at 0x100 whose pattern map
    /// starts `map_base` bytes into the map (0 raw, 4 with baseAddr).
    fn compiled(map_base: u32) -> Bytes {
        let mut m = Bytes(vec![0; 0x400]);
        let record = 0x100;
        m.long(record + 2, 28); // patMap offset
        m.long(record + 6, 200); // patData offset
        let map = record + 28 + map_base;
        m.word(map, 0x8002); // rowBytes, with the PixMap flag
        m.word(map + 6, 8); // bottom
        m.word(map + 8, 8); // right
        m.word(map + 28, 2); // pixelSize
        m.long(map + 38, 120); // pmTable offset
        m
    }

    #[test]
    fn compiled_records_are_read_with_either_map_layout() {
        for map_base in [0, 4] {
            let mut m = compiled(map_base);
            let image = compiled_image(&mut m, 0x100, any_indexed).unwrap();
            assert_eq!(
                image,
                PixPatImage {
                    form: PixPatForm::Compiled,
                    row_bytes: 2,
                    width: 8,
                    height: 8,
                    pixel_size: 2,
                    data_ptr: 0x100 + 200,
                    table_ptr: 0x100 + 120,
                },
                "map at +{map_base}"
            );
        }
        // Offsets out of range mean live handles, not a compiled record.
        let mut m = compiled(0);
        m.long(0x102, 0x2000);
        assert_eq!(compiled_image(&mut m, 0x100, any_indexed), None);
    }

    #[test]
    fn live_records_reject_out_of_range_map_fields() {
        let mut m = Bytes(vec![0; 0x400]);
        m.long(0x102, 0x200);
        m.long(0x200, u32::MAX);
        assert_eq!(live_image(&mut m, 0x100), None);
    }

    #[test]
    fn live_records_follow_their_handles() {
        let mut m = Bytes(vec![0; 0x400]);
        let record = 0x100;
        m.long(record + 2, 0x200); // patMap handle
        m.long(0x200, 0x210); // -> PixMap
        m.long(record + 6, 0x204); // patData handle
        m.long(0x204, 0x300); // -> pixels
        m.word(0x210 + 4, 0x8004);
        m.word(0x210 + 10, 4);
        m.word(0x210 + 12, 8);
        m.word(0x210 + 32, 4);
        m.long(0x210 + 42, 0x208); // pmTable handle
        m.long(0x208, 0x380); // -> colour table
        assert_eq!(
            live_image(&mut m, record),
            Some(PixPatImage {
                form: PixPatForm::Live,
                row_bytes: 4,
                width: 8,
                height: 4,
                pixel_size: 4,
                data_ptr: 0x300,
                table_ptr: 0x380,
            })
        );
    }

    #[test]
    fn color_tables_use_explicit_values_unless_the_device_flag_is_set() {
        let mut m = Bytes(vec![0; 0x100]);
        m.word(6, 1); // two entries
        for (i, (value, red)) in [(5u16, 0x1111u16), (9, 0x2222)].into_iter().enumerate() {
            let entry = 8 + i as u32 * 8;
            m.word(entry, value);
            m.word(entry + 2, red);
        }
        let base = [[0xffff; 3]; 256];
        let explicit = read_color_table(&mut m, 0, base);
        assert_eq!(
            (explicit[5][0], explicit[9][0], explicit[0][0]),
            (0x1111, 0x2222, 0xffff)
        );
        m.word(4, 0x8000);
        let device = read_color_table(&mut m, 0, base);
        assert_eq!(
            (device[0][0], device[1][0], device[5][0]),
            (0x1111, 0x2222, 0xffff)
        );
        assert_eq!(color_table_count(&mut m, 0), Some(2));
    }
}
