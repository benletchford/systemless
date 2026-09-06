//! Shared byte-aligned and packed, unscaled srcCopy transfers.
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

    fn packed_row_span(self, x: i32, y: i32, width: i32) -> Option<(u32, usize, u32)> {
        let [top, left, bottom, right] = self.bounds;
        let end = x.checked_add(width)?;
        if x < left || end > right || width <= 0 || y < top || y >= bottom {
            return None;
        }
        let first_pixel = u32::try_from(x.checked_sub(left)?).ok()?;
        let end_pixel = u32::try_from(end.checked_sub(left)?).ok()?;
        let first_bit = first_pixel.checked_mul(self.depth)?;
        let end_bit = end_pixel.checked_mul(self.depth)?;
        let first_byte = first_bit / 8;
        let end_byte = end_bit.checked_add(7)? / 8;
        if end_byte > self.row_bytes {
            return None;
        }
        let len = usize::try_from(end_byte.checked_sub(first_byte)?).ok()?;
        let y_bytes = u32::try_from(y.checked_sub(top)?)
            .ok()?
            .checked_mul(self.row_bytes)?;
        let address = self.base.checked_add(y_bytes)?.checked_add(first_byte)?;
        if u64::from(address) + len as u64 > 1u64 << 32 {
            return None;
        }
        Some((address, len, first_bit % 8))
    }
}

/// One synchronous transfer, consumed before any guest callback can run.
pub(crate) struct RowCopy<'a> {
    pub(crate) mode: u16,
    pub(crate) source: BytePixmap,
    pub(crate) destination: BytePixmap,
    pub(crate) source_rect: [i32; 4],
    pub(crate) destination_rect: [i32; 4],
    pub(crate) clip: [i32; 4],
    pub(crate) palette: Option<&'a [u8; 256]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use = "only Declined permits a caller to try another raster path"]
pub(crate) enum RowCopyOutcome {
    Completed,
    NoOp,
    Declined,
    ReadOrGeometryFailure,
    WriteFailure { rows_written: usize },
}

impl RowCopy<'_> {
    /// Snapshot all source rows before writing, including across different
    /// addresses that alias the same backing. Geometry/read failures write
    /// nothing. A destination failure preserves that row but may follow rows
    /// already committed; this does not promise rectangle-wide atomicity.
    pub(crate) fn execute(self, memory: &mut impl CopyBitsMemory) -> RowCopyOutcome {
        let depth = self.source.depth;
        let packed = matches!(depth, 2 | 4) && self.palette.is_none();
        let byte_aligned =
            matches!(depth, 8 | 16 | 24 | 32) && (self.palette.is_none() || depth == 8);
        if self.mode != 0 || depth != self.destination.depth || (!packed && !byte_aligned) {
            return RowCopyOutcome::Declined;
        }
        let [st, sl, sb, sr] = self.source_rect;
        let [dt, dl, db, dr] = self.destination_rect;
        let Some(width) = sr.checked_sub(sl) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(height) = sb.checked_sub(st) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(destination_width) = dr.checked_sub(dl) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(destination_height) = db.checked_sub(dt) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        if width <= 0 || height <= 0 || destination_width <= 0 || destination_height <= 0 {
            return RowCopyOutcome::NoOp;
        }
        if destination_width != width || destination_height != height {
            return RowCopyOutcome::Declined;
        }
        let Some(x_delta) = sl.checked_sub(dl) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(y_delta) = st.checked_sub(dt) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let [sbt, sbl, sbb, sbr] = self.source.bounds;
        let [dbt, dbl, dbb, dbr] = self.destination.bounds;
        let [ct, cl, cb, cr] = self.clip;
        let Some(source_top) = sbt.checked_sub(y_delta) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(source_left) = sbl.checked_sub(x_delta) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(source_bottom) = sbb.checked_sub(y_delta) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(source_right) = sbr.checked_sub(x_delta) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let top = dt.max(dbt).max(ct).max(source_top);
        let left = dl.max(dbl).max(cl).max(source_left);
        let bottom = db.min(dbb).min(cb).min(source_bottom);
        let right = dr.min(dbr).min(cr).min(source_right);
        if top >= bottom || left >= right {
            return RowCopyOutcome::NoOp;
        }
        let Some(pixel_width) = right.checked_sub(left) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(row_len) = usize::try_from(pixel_width)
            .ok()
            .and_then(|width| width.checked_mul((depth / 8) as usize))
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(count) = bottom
            .checked_sub(top)
            .and_then(|count| usize::try_from(count).ok())
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        if packed {
            return self.execute_packed(
                memory,
                [top, left, bottom, right],
                [y_delta, x_delta],
                count,
            );
        }
        let mut addresses = Vec::with_capacity(count);
        // Check every row's arithmetic before allocating or reading pixels.
        for y in top..bottom {
            let Some(source_x) = left.checked_add(x_delta) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let Some(source_y) = y.checked_add(y_delta) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let Some(source_address) = self.source.row_address(source_x, source_y, row_len) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let Some(destination_address) = self.destination.row_address(left, y, row_len) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            addresses.push((source_address, destination_address));
        }
        let Some(pixel_count) = count.checked_mul(row_len) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let mut pixels = vec![0; pixel_count];
        for ((source, _), row) in addresses.iter().zip(pixels.chunks_exact_mut(row_len)) {
            if memory.read_copy_row(*source, row).is_none() {
                return RowCopyOutcome::ReadOrGeometryFailure;
            }
            if let Some(palette) = self.palette {
                for pixel in row {
                    *pixel = palette[usize::from(*pixel)];
                }
            }
        }
        for (rows_written, ((_, destination), row)) in addresses
            .iter()
            .zip(pixels.chunks_exact(row_len))
            .enumerate()
        {
            if memory.write_copy_row(*destination, row).is_none() {
                return RowCopyOutcome::WriteFailure { rows_written };
            }
        }
        RowCopyOutcome::Completed
    }

    fn execute_packed(
        self,
        memory: &mut impl CopyBitsMemory,
        rectangle: [i32; 4],
        delta: [i32; 2],
        count: usize,
    ) -> RowCopyOutcome {
        let [top, left, bottom, right] = rectangle;
        let [y_delta, x_delta] = delta;
        let depth = self.source.depth;
        let Some(width) = right
            .checked_sub(left)
            .and_then(|width| usize::try_from(width).ok())
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(pixel_count) = count.checked_mul(width) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let mut spans = Vec::with_capacity(count);
        for y in top..bottom {
            let Some(source_x) = left.checked_add(x_delta) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let Some(source_y) = y.checked_add(y_delta) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let Some(source) = self
                .source
                .packed_row_span(source_x, source_y, right - left)
            else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let Some(destination) = self.destination.packed_row_span(left, y, right - left) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            spans.push((source, destination));
        }

        let mut pixels = vec![0; pixel_count];
        for (((source_address, source_len, source_bit), _), row) in
            spans.iter().zip(pixels.chunks_exact_mut(width))
        {
            let mut source = vec![0; *source_len];
            if memory.read_copy_row(*source_address, &mut source).is_none() {
                return RowCopyOutcome::ReadOrGeometryFailure;
            }
            for (x, pixel) in row.iter_mut().enumerate() {
                let bit = *source_bit as usize + x * depth as usize;
                let shift = 8 - depth as usize - bit % 8;
                *pixel = (source[bit / 8] >> shift) & ((1 << depth) - 1) as u8;
            }
        }

        let mut destinations = Vec::with_capacity(count);
        for (_, (address, len, _)) in &spans {
            let mut row = vec![0; *len];
            if memory.read_copy_row(*address, &mut row).is_none() {
                return RowCopyOutcome::ReadOrGeometryFailure;
            }
            destinations.push(row);
        }
        for (((_, (_, _, destination_bit)), pixels), destination) in spans
            .iter()
            .zip(pixels.chunks_exact(width))
            .zip(destinations.iter_mut())
        {
            for (x, pixel) in pixels.iter().copied().enumerate() {
                let bit = *destination_bit as usize + x * depth as usize;
                let shift = 8 - depth as usize - bit % 8;
                let mask = (((1 << depth) - 1) as u8) << shift;
                destination[bit / 8] = (destination[bit / 8] & !mask) | (pixel << shift);
            }
        }
        for (rows_written, ((_, (address, _, _)), row)) in
            spans.iter().zip(destinations.iter()).enumerate()
        {
            if memory.write_copy_row(*address, row).is_none() {
                return RowCopyOutcome::WriteFailure { rows_written };
            }
        }
        RowCopyOutcome::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: u32 = 0x0100_0000;
    const DESTINATION: u32 = 0x0200_0000;

    fn run(memory: &mut GuestAddressSpace, classic: bool, copy: RowCopy<'_>) -> RowCopyOutcome {
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
    fn no_op_and_decline_are_distinct_prewrite_outcomes() {
        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("prewrite outcome reached source memory");
            }

            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("prewrite outcome reached destination memory");
            }
        }

        let request = |mode, source_rect, destination_rect, clip| RowCopy {
            mode,
            source: pixmap(SOURCE, 4, 8, [0, 0, 2, 4]),
            destination: pixmap(DESTINATION, 4, 8, [0, 0, 2, 4]),
            source_rect,
            destination_rect,
            clip,
            palette: None,
        };
        assert_eq!(
            request(0, [0, 0, 0, 4], [0, 0, 0, 4], [0, 0, 2, 4]).execute(&mut NoAccess),
            RowCopyOutcome::NoOp
        );
        assert_eq!(
            request(0, [0, 0, 2, 4], [0, 0, 2, 4], [3, 0, 4, 4]).execute(&mut NoAccess),
            RowCopyOutcome::NoOp
        );
        assert_eq!(
            request(1, [0, 0, 2, 4], [0, 0, 2, 4], [0, 0, 2, 4]).execute(&mut NoAccess),
            RowCopyOutcome::Declined
        );
        assert_eq!(
            request(0, [0, 0, 2, 4], [0, 0, 1, 4], [0, 0, 2, 4]).execute(&mut NoAccess),
            RowCopyOutcome::Declined
        );
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
                    mode: 0,
                    source: pixmap(SOURCE, stride, depth, [-2, -3, 1, 1]),
                    destination: pixmap(DESTINATION, stride, depth, [10, 20, 13, 24]),
                    source_rect: [-3, -4, 1, 1],
                    destination_rect: [9, 19, 13, 24],
                    clip: [11, 21, 13, 23],
                    palette: None,
                };
                assert_eq!(run(&mut memory, classic, copy), RowCopyOutcome::Completed);
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
    fn packed_rows_are_msb_first_and_preserve_edge_fields_and_padding() {
        for classic in [false, true] {
            for (depth, source_row, destination_row, expected_row, width) in [
                (
                    2,
                    &[0x1b, 0xe4, 0x91][..],
                    &[0x80, 0x01, 0x5a][..],
                    &[0x9b, 0xe5, 0x5a][..],
                    8,
                ),
                (
                    4,
                    &[0x01, 0x23, 0x45, 0x92][..],
                    &[0xa0, 0x00, 0x0b, 0x5a][..],
                    &[0xa1, 0x23, 0x4b, 0x5a][..],
                    6,
                ),
            ] {
                let stride = source_row.len() as u32;
                let mut memory = GuestAddressSpace::new();
                let mut source = source_row.repeat(2);
                *source.last_mut().unwrap() ^= 1;
                let mut destination = destination_row.repeat(2);
                *destination.last_mut().unwrap() ^= 1;
                memory.add_region(SOURCE, source);
                memory.add_region(DESTINATION, destination);
                assert_eq!(
                    run(
                        &mut memory,
                        classic,
                        RowCopy {
                            mode: 0,
                            source: pixmap(SOURCE, stride, depth, [-2, 10, 0, 10 + width]),
                            destination: pixmap(
                                DESTINATION,
                                stride,
                                depth,
                                [20, 30, 22, 30 + width],
                            ),
                            source_rect: [-2, 11, 0, 10 + width - 1],
                            destination_rect: [20, 31, 22, 30 + width - 1],
                            clip: [20, 31, 22, 30 + width - 1],
                            palette: None,
                        },
                    ),
                    RowCopyOutcome::Completed
                );
                let mut actual = vec![0; destination_row.len() * 2];
                memory.read_bytes_into(DESTINATION, &mut actual).unwrap();
                let mut expected = expected_row.repeat(2);
                *expected.last_mut().unwrap() ^= 1;
                assert_eq!(actual, expected, "classic={classic}, depth={depth}");
            }
        }
    }

    #[test]
    fn packed_rows_translate_unequal_source_and_destination_field_offsets() {
        for classic in [false, true] {
            for (depth, source, destination, expected, source_rect, destination_rect) in [
                (
                    2,
                    &[0x1b, 0xe4][..],
                    &[0x80, 0x01][..],
                    &[0x6f, 0x91][..],
                    [0, 1, 1, 7],
                    [0, 0, 1, 6],
                ),
                (
                    4,
                    &[0x01, 0x23, 0x45][..],
                    &[0xa0, 0x00, 0x0b][..],
                    &[0x12, 0x34, 0x0b][..],
                    [0, 1, 1, 5],
                    [0, 0, 1, 4],
                ),
            ] {
                let mut memory = GuestAddressSpace::new();
                memory.add_region(SOURCE, source.to_vec());
                memory.add_region(DESTINATION, destination.to_vec());
                assert_eq!(
                    run(
                        &mut memory,
                        classic,
                        RowCopy {
                            mode: 0,
                            source: pixmap(SOURCE, source.len() as u32, depth, [0, 0, 1, 8]),
                            destination: pixmap(
                                DESTINATION,
                                destination.len() as u32,
                                depth,
                                [0, 0, 1, 8],
                            ),
                            source_rect,
                            destination_rect,
                            clip: destination_rect,
                            palette: None,
                        },
                    ),
                    RowCopyOutcome::Completed
                );
                let mut actual = vec![0; destination.len()];
                memory.read_bytes_into(DESTINATION, &mut actual).unwrap();
                assert_eq!(actual, expected, "classic={classic}, depth={depth}");
            }
        }
    }

    #[test]
    fn packed_distinct_aliases_snapshot_later_source_rows() {
        for classic in [false, true] {
            let mut memory = GuestAddressSpace::new();
            let backing = crate::memory::bus::SharedRamRegion::from_owned_bytes(vec![
                0x1b, 0xe4, 0xaa, 0xe4, 0x1b, 0xbb, 0, 0, 0xcc,
            ]);
            // SAFETY: both aliases are accessed serially through one copy.
            unsafe {
                memory.add_shared_region(SOURCE, backing.clone());
                memory.add_shared_region(DESTINATION, backing);
            }
            assert_eq!(
                run(
                    &mut memory,
                    classic,
                    RowCopy {
                        mode: 0,
                        source: pixmap(SOURCE, 3, 2, [0, 0, 2, 8]),
                        destination: pixmap(DESTINATION + 3, 3, 2, [0, 0, 2, 8]),
                        source_rect: [0, 0, 2, 8],
                        destination_rect: [0, 0, 2, 8],
                        clip: [0, 0, 2, 8],
                        palette: None,
                    },
                ),
                RowCopyOutcome::Completed
            );
            let mut actual = [0; 9];
            memory.read_bytes_into(SOURCE, &mut actual).unwrap();
            assert_eq!(
                actual,
                [0x1b, 0xe4, 0xaa, 0x1b, 0xe4, 0xbb, 0xe4, 0x1b, 0xcc]
            );
        }
    }

    #[test]
    fn packed_source_failure_is_prewrite_and_later_refusal_is_row_atomic() {
        for classic in [false, true] {
            let request = || RowCopy {
                mode: 0,
                source: pixmap(SOURCE, 3, 2, [0, 0, 2, 8]),
                destination: pixmap(DESTINATION, 3, 2, [0, 0, 2, 8]),
                source_rect: [0, 1, 2, 7],
                destination_rect: [0, 1, 2, 7],
                clip: [0, 1, 2, 7],
                palette: None,
            };

            let mut missing_source = GuestAddressSpace::new();
            missing_source.add_region(SOURCE, vec![0x1b, 0xe4, 0xaa]);
            missing_source.add_region(DESTINATION, vec![0x80, 0x01, 0x5a, 0x81, 0x02, 0x5b]);
            assert_eq!(
                run(&mut missing_source, classic, request()),
                RowCopyOutcome::ReadOrGeometryFailure
            );
            let mut unchanged = [0; 6];
            missing_source
                .read_bytes_into(DESTINATION, &mut unchanged)
                .unwrap();
            assert_eq!(unchanged, [0x80, 0x01, 0x5a, 0x81, 0x02, 0x5b]);

            let mut protected_destination = GuestAddressSpace::new();
            protected_destination.add_region(SOURCE, vec![0x1b, 0xe4, 0xaa, 0xe4, 0x1b, 0xbb]);
            protected_destination.add_region(DESTINATION, vec![0x80, 0x01, 0x5a, 0x81, 0x02, 0x5b]);
            protected_destination.add_readonly_region(DESTINATION + 3, vec![0x81, 0x02]);
            assert_eq!(
                run(&mut protected_destination, classic, request()),
                RowCopyOutcome::WriteFailure { rows_written: 1 }
            );
            let mut actual = [0; 6];
            protected_destination
                .read_bytes_into(DESTINATION, &mut actual)
                .unwrap();
            assert_eq!(actual, [0x9b, 0xe5, 0x5a, 0x81, 0x02, 0x5b]);
        }
    }

    #[test]
    fn packed_destination_read_failure_precedes_all_publication() {
        for classic in [false, true] {
            let mut memory = GuestAddressSpace::new();
            memory.add_region(SOURCE, vec![0x1b, 0xe4, 0xaa, 0xe4, 0x1b, 0xbb]);
            memory.add_region(DESTINATION, vec![0x80, 0x01, 0x5a]);
            assert_eq!(
                run(
                    &mut memory,
                    classic,
                    RowCopy {
                        mode: 0,
                        source: pixmap(SOURCE, 3, 2, [0, 0, 2, 8]),
                        destination: pixmap(DESTINATION, 3, 2, [0, 0, 2, 8]),
                        source_rect: [0, 1, 2, 7],
                        destination_rect: [0, 1, 2, 7],
                        clip: [0, 1, 2, 7],
                        palette: None,
                    },
                ),
                RowCopyOutcome::ReadOrGeometryFailure
            );
            let mut actual = [0; 3];
            memory.read_bytes_into(DESTINATION, &mut actual).unwrap();
            assert_eq!(actual, [0x80, 0x01, 0x5a]);
        }
    }

    #[test]
    fn packed_stride_and_address_overflow_fail_before_memory_access() {
        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("invalid packed geometry reached memory");
            }
            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("invalid packed geometry reached memory");
            }
        }
        for source in [
            pixmap(SOURCE, 2, 4, [0, 0, 1, 6]),
            pixmap(u32::MAX, 3, 2, [0, 0, 1, 8]),
        ] {
            assert_eq!(
                RowCopy {
                    mode: 0,
                    source,
                    destination: pixmap(DESTINATION, 3, source.depth, source.bounds),
                    source_rect: source.bounds,
                    destination_rect: source.bounds,
                    clip: source.bounds,
                    palette: None,
                }
                .execute(&mut NoAccess),
                RowCopyOutcome::ReadOrGeometryFailure
            );
        }
    }

    #[test]
    fn packed_unmigrated_families_decline_before_memory_access() {
        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("declined request reached memory");
            }
            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("declined request reached memory");
            }
        }
        let palette = [0; 256];
        for (depth, destination_rect, palette) in [
            (1, [0, 0, 1, 8], None),
            (2, [0, 0, 1, 8], Some(&palette)),
            (4, [0, 0, 1, 4], None),
        ] {
            assert_eq!(
                RowCopy {
                    mode: 0,
                    source: pixmap(SOURCE, 2, depth, [0, 0, 1, 8]),
                    destination: pixmap(DESTINATION, 2, depth, [0, 0, 1, 8]),
                    source_rect: [0, 0, 1, 8],
                    destination_rect,
                    clip: [0, 0, 1, 8],
                    palette,
                }
                .execute(&mut NoAccess),
                RowCopyOutcome::Declined
            );
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
                            mode: 0,
                            source: pixmap(source, 4, 8, [0, 0, 3, 3]),
                            destination: pixmap(destination, 4, 8, [0, 0, 3, 3]),
                            source_rect: [0, 0, 3, 3],
                            destination_rect: [0, 0, 3, 3],
                            clip: [0, 0, 3, 3],
                            palette: Some(&palette),
                        }
                    ),
                    RowCopyOutcome::Completed
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
                        mode: 0,
                        source: pixmap(SOURCE, 4, 8, [0, 0, 3, 3]),
                        destination: pixmap(DESTINATION + 4, 4, 8, [0, 0, 3, 3]),
                        source_rect: [0, 0, 3, 3],
                        destination_rect: [0, 0, 3, 3],
                        clip: [0, 0, 3, 3],
                        palette: None,
                    }
                ),
                RowCopyOutcome::Completed
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
                    mode: 0,
                    source: pixmap(base, stride, 8, bounds),
                    destination: pixmap(DESTINATION, stride, 8, bounds),
                    source_rect: rect,
                    destination_rect: rect,
                    clip: rect,
                    palette: None,
                }
                .execute(&mut NoAccess),
                RowCopyOutcome::ReadOrGeometryFailure
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
                            mode: 0,
                            source: pixmap(SOURCE, if bad_stride { 3 } else { 4 }, 8, [0, 0, 2, 4]),
                            destination: pixmap(DESTINATION, 4, 8, [0, 0, 2, 4]),
                            source_rect: [0, 0, 2, 4],
                            destination_rect: [0, 0, 2, 4],
                            clip: [0, 0, 2, 4],
                            palette: None,
                        }
                    ),
                    RowCopyOutcome::ReadOrGeometryFailure
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
                        mode: 0,
                        source: pixmap(SOURCE, 4, 8, [0, 0, 2, 4]),
                        destination: pixmap(DESTINATION, 4, 8, [0, 0, 2, 4]),
                        source_rect: [0, 0, 2, 4],
                        destination_rect: [0, 0, 2, 4],
                        clip: [0, 0, 2, 4],
                        palette: None,
                    }
                ),
                RowCopyOutcome::WriteFailure { rows_written: 1 }
            );
            let mut actual = [0; 8];
            memory.read_bytes_into(DESTINATION, &mut actual).unwrap();
            assert_eq!(actual, [7, 7, 7, 7, 0xAA, 0xAA, 0xAA, 0xAA]);
        }
    }
}
