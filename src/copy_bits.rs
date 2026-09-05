//! Shared byte-aligned, unscaled srcCopy transfers.
//! Imaging With QuickDraw (1994), pp. 3-112–3-117 and 4-27–4-28.
//! ABI decoding, port/mask resolution and picture recording stay at the callers
//! until their corresponding operation families migrate.

use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};

pub(crate) trait CopyBitsMemory {
    fn read_copy_row(&mut self, address: u32, bytes: &mut [u8]) -> Option<()>;
    fn write_copy_row(&mut self, address: u32, bytes: &[u8]) -> Option<()>;
}

impl CopyBitsMemory for GuestAddressSpace {
    fn read_copy_row(&mut self, address: u32, bytes: &mut [u8]) -> Option<()> {
        self.read_bytes_into(address, bytes)
    }

    fn write_copy_row(&mut self, address: u32, bytes: &[u8]) -> Option<()> {
        self.write_bytes(address, bytes)
    }
}

impl CopyBitsMemory for MacMemoryBus {
    fn read_copy_row(&mut self, address: u32, bytes: &mut [u8]) -> Option<()> {
        if !self.is_guest_address_mapped(address, bytes.len()) {
            return None;
        }
        self.read_bytes_into(address, bytes);
        Some(())
    }

    fn write_copy_row(&mut self, address: u32, bytes: &[u8]) -> Option<()> {
        if !self.is_guest_address_writable(address, bytes.len()) {
            return None;
        }
        // The exclusive view remains held; no guest execution or mapping
        // mutation intervenes between the range check and the bulk write.
        self.write_bytes(address, bytes);
        Some(())
    }
}

/// Bounds and rectangles are [top, left, bottom, right] in guest coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BytePixmap {
    pub(crate) base: u32,
    pub(crate) row_bytes: u32,
    pub(crate) depth: u32,
    pub(crate) bounds: [i32; 4],
}

impl BytePixmap {
    fn row_address(self, x: i32, y: i32, len: usize) -> Option<u32> {
        let [top, left, bottom, right] = self.bounds;
        if x < left || x >= right || y < top || y >= bottom {
            return None;
        }
        let x_bytes = u32::try_from(x.checked_sub(left)?)
            .ok()?
            .checked_mul(self.depth / 8)?;
        let len = u32::try_from(len).ok()?;
        if x_bytes.checked_add(len)? > self.row_bytes {
            return None;
        }
        let y_bytes = u32::try_from(y.checked_sub(top)?)
            .ok()?
            .checked_mul(self.row_bytes)?;
        let address = self.base.checked_add(y_bytes)?.checked_add(x_bytes)?;
        if u64::from(address) + u64::from(len) > 1u64 << 32 {
            return None;
        }
        Some(address)
    }
}

/// One synchronous transfer, consumed before any guest callback can run.
pub(crate) struct RowCopy<'a> {
    pub(crate) source: BytePixmap,
    pub(crate) destination: BytePixmap,
    pub(crate) source_rect: [i32; 4],
    pub(crate) destination_rect: [i32; 4],
    pub(crate) clip: [i32; 4],
    pub(crate) palette: Option<&'a [u8; 256]>,
}

impl RowCopy<'_> {
    /// Snapshot all source rows before writing, including across different
    /// addresses that alias the same backing. Geometry/read failures write
    /// nothing. A destination failure preserves that row but may follow rows
    /// already committed; this does not promise rectangle-wide atomicity.
    pub(crate) fn execute(self, memory: &mut impl CopyBitsMemory) -> Option<()> {
        let depth = self.source.depth;
        if depth != self.destination.depth
            || !matches!(depth, 8 | 16 | 24 | 32)
            || (self.palette.is_some() && depth != 8)
        {
            return None;
        }
        let [st, sl, sb, sr] = self.source_rect;
        let [dt, dl, db, dr] = self.destination_rect;
        let width = sr.checked_sub(sl)?;
        let height = sb.checked_sub(st)?;
        if width <= 0
            || height <= 0
            || dr.checked_sub(dl)? != width
            || db.checked_sub(dt)? != height
        {
            return None;
        }
        let x_delta = sl.checked_sub(dl)?;
        let y_delta = st.checked_sub(dt)?;
        let [sbt, sbl, sbb, sbr] = self.source.bounds;
        let [dbt, dbl, dbb, dbr] = self.destination.bounds;
        let [ct, cl, cb, cr] = self.clip;
        let top = dt.max(dbt).max(ct).max(sbt.checked_sub(y_delta)?);
        let left = dl.max(dbl).max(cl).max(sbl.checked_sub(x_delta)?);
        let bottom = db.min(dbb).min(cb).min(sbb.checked_sub(y_delta)?);
        let right = dr.min(dbr).min(cr).min(sbr.checked_sub(x_delta)?);
        if top >= bottom || left >= right {
            return None;
        }
        let row_len = usize::try_from(right.checked_sub(left)?)
            .ok()?
            .checked_mul((depth / 8) as usize)?;
        let count = usize::try_from(bottom.checked_sub(top)?).ok()?;
        let mut addresses = Vec::with_capacity(count);
        // Check every row's arithmetic before allocating or reading pixels.
        for y in top..bottom {
            addresses.push((
                self.source.row_address(
                    left.checked_add(x_delta)?,
                    y.checked_add(y_delta)?,
                    row_len,
                )?,
                self.destination.row_address(left, y, row_len)?,
            ));
        }
        let mut pixels = vec![0; count.checked_mul(row_len)?];
        for ((source, _), row) in addresses.iter().zip(pixels.chunks_exact_mut(row_len)) {
            memory.read_copy_row(*source, row)?;
            if let Some(palette) = self.palette {
                for pixel in row {
                    *pixel = palette[usize::from(*pixel)];
                }
            }
        }
        for ((_, destination), row) in addresses.iter().zip(pixels.chunks_exact(row_len)) {
            memory.write_copy_row(*destination, row)?;
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: u32 = 0x0100_0000;
    const DESTINATION: u32 = 0x0200_0000;

    fn run(memory: &mut GuestAddressSpace, classic: bool, copy: RowCopy<'_>) -> Option<()> {
        if classic {
            let mut bus = MacMemoryBus::new(0x10000);
            bus.set_addressing_32_bit(true);
            bus.attach_guest_address_space(memory.shared_view());
            copy.execute(&mut bus)
        } else {
            copy.execute(memory)
        }
    }

    fn pixmap(base: u32, row_bytes: u32, depth: u32, bounds: [i32; 4]) -> BytePixmap {
        BytePixmap {
            base,
            row_bytes,
            depth,
            bounds,
        }
    }

    #[test]
    fn clipped_offset_rows_preserve_padding_in_both_memory_views() {
        for classic in [false, true] {
            for depth in [8, 16, 24, 32] {
                let bytes = depth / 8;
                let stride = 4 * bytes + 3;
                let mut memory = GuestAddressSpace::new();
                let source: Vec<u8> = (0..stride * 3).map(|n| n as u8).collect();
                memory.add_region(SOURCE, source.clone());
                memory.add_region(DESTINATION, vec![0xAA; (stride * 3) as usize]);
                let copy = RowCopy {
                    source: pixmap(SOURCE, stride, depth, [-2, -3, 1, 1]),
                    destination: pixmap(DESTINATION, stride, depth, [10, 20, 13, 24]),
                    source_rect: [-3, -4, 1, 1],
                    destination_rect: [9, 19, 13, 24],
                    clip: [11, 21, 13, 23],
                    palette: None,
                };
                assert_eq!(run(&mut memory, classic, copy), Some(()));
                let mut expected = vec![0xAA; (stride * 3) as usize];
                for row in 1..3 {
                    let start = (row * stride + bytes) as usize;
                    let end = start + (2 * bytes) as usize;
                    expected[start..end].copy_from_slice(&source[start..end]);
                }
                let mut actual = vec![0; expected.len()];
                memory.read_bytes_into(DESTINATION, &mut actual).unwrap();
                assert_eq!(actual, expected, "classic={classic}, depth={depth}");
            }
        }
    }

    #[test]
    fn overlapping_rows_snapshot_before_palette_mapping_and_writes() {
        for classic in [false, true] {
            for downward in [false, true] {
                let mut memory = GuestAddressSpace::new();
                memory.add_region(SOURCE, (0..16).collect());
                let palette = std::array::from_fn(|n| 255 - n as u8);
                let (source, destination) = if downward {
                    (SOURCE, SOURCE + 4)
                } else {
                    (SOURCE + 4, SOURCE)
                };
                assert_eq!(
                    run(
                        &mut memory,
                        classic,
                        RowCopy {
                            source: pixmap(source, 4, 8, [0, 0, 3, 3]),
                            destination: pixmap(destination, 4, 8, [0, 0, 3, 3]),
                            source_rect: [0, 0, 3, 3],
                            destination_rect: [0, 0, 3, 3],
                            clip: [0, 0, 3, 3],
                            palette: Some(&palette),
                        }
                    ),
                    Some(())
                );
                let mut expected: Vec<u8> = (0..16).collect();
                for row in 0..3 {
                    for x in 0..3 {
                        expected[(destination - SOURCE) as usize + row * 4 + x] =
                            255 - ((source - SOURCE) as usize + row * 4 + x) as u8;
                    }
                }
                let mut actual = vec![0; 16];
                memory.read_bytes_into(SOURCE, &mut actual).unwrap();
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn distinct_guest_aliases_snapshot_the_same_backing() {
        for classic in [false, true] {
            let mut memory = GuestAddressSpace::new();
            let backing = crate::memory::bus::SharedRamRegion::from_owned_bytes((0..16).collect());
            // SAFETY: this test accesses both aliases serially through one
            // operation; no borrowed byte slices survive a memory call.
            unsafe {
                memory.add_shared_region(SOURCE, backing.clone());
                memory.add_shared_region(DESTINATION, backing);
            }
            assert_eq!(
                run(
                    &mut memory,
                    classic,
                    RowCopy {
                        source: pixmap(SOURCE, 4, 8, [0, 0, 3, 3]),
                        destination: pixmap(DESTINATION + 4, 4, 8, [0, 0, 3, 3]),
                        source_rect: [0, 0, 3, 3],
                        destination_rect: [0, 0, 3, 3],
                        clip: [0, 0, 3, 3],
                        palette: None,
                    }
                ),
                Some(())
            );
            let mut actual = [0; 16];
            memory.read_bytes_into(SOURCE, &mut actual).unwrap();
            assert_eq!(actual, [0, 1, 2, 3, 0, 1, 2, 7, 4, 5, 6, 11, 8, 9, 10, 15]);
        }
    }

    #[test]
    fn overflowing_geometry_is_rejected_before_memory_access() {
        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("invalid geometry reached source memory");
            }
            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("invalid geometry reached destination memory");
            }
        }
        for (base, stride, bounds, rect) in [
            (u32::MAX - 1, 4, [0, 0, 2, 4], [0, 0, 2, 4]),
            (SOURCE, u32::MAX, [0, 0, 3, 4], [0, 0, 3, 4]),
            (
                SOURCE,
                4,
                [0, i32::MIN, 1, i32::MAX],
                [0, i32::MIN, 1, i32::MAX],
            ),
        ] {
            assert_eq!(
                RowCopy {
                    source: pixmap(base, stride, 8, bounds),
                    destination: pixmap(DESTINATION, stride, 8, bounds),
                    source_rect: rect,
                    destination_rect: rect,
                    clip: rect,
                    palette: None,
                }
                .execute(&mut NoAccess),
                None
            );
        }
    }

    #[test]
    fn source_hole_or_invalid_stride_never_writes_destination() {
        for classic in [false, true] {
            for bad_stride in [false, true] {
                let mut memory = GuestAddressSpace::new();
                memory.add_region(SOURCE, vec![7; if bad_stride { 8 } else { 4 }]);
                memory.add_region(DESTINATION, vec![0xAA; 8]);
                assert_eq!(
                    run(
                        &mut memory,
                        classic,
                        RowCopy {
                            source: pixmap(SOURCE, if bad_stride { 3 } else { 4 }, 8, [0, 0, 2, 4]),
                            destination: pixmap(DESTINATION, 4, 8, [0, 0, 2, 4]),
                            source_rect: [0, 0, 2, 4],
                            destination_rect: [0, 0, 2, 4],
                            clip: [0, 0, 2, 4],
                            palette: None,
                        }
                    ),
                    None
                );
                let mut actual = [0; 8];
                memory.read_bytes_into(DESTINATION, &mut actual).unwrap();
                assert_eq!(actual, [0xAA; 8]);
            }
        }
    }

    #[test]
    fn protected_later_row_preserves_that_row_after_prior_row_commits() {
        for classic in [false, true] {
            let mut memory = GuestAddressSpace::new();
            memory.add_region(SOURCE, vec![7; 8]);
            memory.add_region(DESTINATION, vec![0xAA; 8]);
            memory.add_readonly_region(DESTINATION + 6, vec![0xAA]);
            assert_eq!(
                run(
                    &mut memory,
                    classic,
                    RowCopy {
                        source: pixmap(SOURCE, 4, 8, [0, 0, 2, 4]),
                        destination: pixmap(DESTINATION, 4, 8, [0, 0, 2, 4]),
                        source_rect: [0, 0, 2, 4],
                        destination_rect: [0, 0, 2, 4],
                        clip: [0, 0, 2, 4],
                        palette: None,
                    }
                ),
                None
            );
            let mut actual = [0; 8];
            memory.read_bytes_into(DESTINATION, &mut actual).unwrap();
            assert_eq!(actual, [7, 7, 7, 7, 0xAA, 0xAA, 0xAA, 0xAA]);
        }
    }
}
