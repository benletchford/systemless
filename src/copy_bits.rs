//! Shared byte-aligned and packed, unscaled srcCopy transfers.
//! Imaging With QuickDraw (1994), pp. 3-112–3-117 and 4-27–4-28.
//! ABI decoding, port/mask resolution and picture recording stay at the callers
//! until their corresponding operation families migrate.

use std::ops::Range;

use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};

const INDEXED_8_GUARD_BYTES: usize = 4;
const INDEXED_8_MAP_ENTRIES: usize = 256;

/// Pure horizontal plan for the indexed 8-bit identity-palette shrink path.
/// Destination indices are relative to the complete, unclipped destination
/// rectangle. `source_range` is relative to the submitted source pixel.
///
/// Imaging With QuickDraw defines rectangle scaling, but not the indexed
/// reducer. The truncated fixed-point carry and rounded source/guard/map
/// prefix are measured compatibility behavior; the finite-prefix saturation
/// follows from unsigned-byte maximum and the complete identity map.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Indexed8HorizontalShrink {
    groups: Vec<Range<usize>>,
    source_range: Range<usize>,
    staged_len: usize,
}

impl Indexed8HorizontalShrink {
    pub(crate) fn new(
        source_width: usize,
        destination_width: usize,
        visible: Range<usize>,
    ) -> Option<Self> {
        if !(1..=i16::MAX as usize).contains(&source_width)
            || destination_width == 0
            || destination_width >= source_width
            || visible.start > visible.end
            || visible.end > destination_width
        {
            return None;
        }

        let source = i64::try_from(source_width).ok()?;
        let destination = i64::try_from(destination_width).ok()?;
        let step = destination.checked_mul(1 << 16)?.checked_div(source)?;
        if step <= 0 {
            return None;
        }
        let phase = step / 2;
        let boundary = |index: usize| {
            let numerator = i64::try_from(index)
                .ok()?
                .checked_mul(1 << 16)?
                .checked_sub(phase)?;
            let quotient = numerator.div_euclid(step);
            let rounded = quotient.checked_add(i64::from(numerator.rem_euclid(step) != 0))?;
            usize::try_from(rounded).ok()
        };

        let endpoint_count = visible.end.checked_sub(visible.start)?.checked_add(1)?;
        let mut endpoints = Vec::new();
        endpoints.try_reserve_exact(endpoint_count).ok()?;
        for index in visible.start..=visible.end {
            endpoints.push(boundary(index)?);
        }
        let mut groups = Vec::new();
        groups
            .try_reserve_exact(endpoint_count.saturating_sub(1))
            .ok()?;
        groups.extend(endpoints.windows(2).map(|pair| pair[0]..pair[1]));
        if groups.iter().any(|group| group.is_empty()) {
            return None;
        }

        let staged_len = source_width.checked_add(3)? & !3;
        let source_range = match (groups.first(), groups.last()) {
            (Some(first), Some(last)) => first.start.min(staged_len)..last.end.min(staged_len),
            (None, None) => 0..0,
            _ => return None,
        };
        Some(Self {
            groups,
            source_range,
            staged_len,
        })
    }

    pub(crate) fn groups(&self) -> &[Range<usize>] {
        &self.groups
    }

    pub(crate) fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }

    /// Reduces an exact snapshot of `source_range`. The remaining arena is the
    /// operation-owned zero guard followed by big-endian identity-map entries.
    pub(crate) fn reduce(&self, staged_source: &[u8]) -> Option<Vec<u8>> {
        if staged_source.len() != self.source_range.len() {
            return None;
        }
        let map_start = self.staged_len.checked_add(INDEXED_8_GUARD_BYTES)?;
        let prefix_end = map_start.checked_add(INDEXED_8_MAP_ENTRIES.checked_mul(4)?)?;
        let mut output = Vec::new();
        output.try_reserve_exact(self.groups.len()).ok()?;

        for group in &self.groups {
            let mut maximum = None;
            for index in group.start..group.end.min(prefix_end) {
                let value = if index < self.staged_len {
                    let source_index = index.checked_sub(self.source_range.start)?;
                    *staged_source.get(source_index)?
                } else if index < map_start {
                    0
                } else {
                    let map_offset = index - map_start;
                    let entry = u32::try_from(map_offset / 4).ok()?;
                    entry.to_be_bytes()[map_offset % 4]
                };
                maximum = Some(maximum.map_or(value, |current: u8| current.max(value)));
            }
            let maximum = maximum?;
            if group.end > prefix_end && maximum != u8::MAX {
                return None;
            }
            output.push(maximum);
        }
        Some(output)
    }

    fn reduce_rows(&self, snapshots: &[u8], row_count: usize) -> Option<Vec<u8>> {
        let row_len = self.source_range.len();
        if snapshots.len() != row_count.checked_mul(row_len)? {
            return None;
        }
        let output_len = row_count.checked_mul(self.groups.len())?;
        let mut output = Vec::new();
        output.try_reserve_exact(output_len).ok()?;
        for row_index in 0..row_count {
            let start = row_index.checked_mul(row_len)?;
            let end = start.checked_add(row_len)?;
            output.extend_from_slice(&self.reduce(snapshots.get(start..end)?)?);
        }
        Some(output)
    }
}

/// Pure vertical plan for indexed 8-bit identity-palette scaling. Destination
/// indices are relative to the complete, unclipped destination rectangle, so
/// slicing `visible` preserves the original vertical phase.
///
/// Imaging With QuickDraw defines rectangle scaling. The source-row carry,
/// exact-integral reduction branch, and maximum-index reduction are measured
/// Mac OS 8.1 compatibility behavior.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Indexed8VerticalScale {
    groups: Vec<Range<usize>>,
    source_range: Range<usize>,
}

impl Indexed8VerticalScale {
    pub(crate) fn new(
        source_height: usize,
        destination_height: usize,
        visible: Range<usize>,
    ) -> Option<Self> {
        if !(1..=i16::MAX as usize).contains(&source_height)
            || !(1..=i16::MAX as usize).contains(&destination_height)
            || visible.start > visible.end
            || visible.end > destination_height
        {
            return None;
        }

        let mut all_groups = Vec::new();
        all_groups.try_reserve_exact(destination_height).ok()?;
        if source_height == destination_height {
            for index in 0..destination_height {
                all_groups.push(index..index.checked_add(1)?);
            }
        } else if source_height > destination_height && source_height % destination_height == 0 {
            let group_height = source_height / destination_height;
            for destination in 0..destination_height {
                let start = destination.checked_mul(group_height)?;
                all_groups.push(start..start.checked_add(group_height)?);
            }
        } else {
            let source_height = i64::try_from(source_height).ok()?;
            let destination_height = i64::try_from(destination_height).ok()?;
            let mut error = -(source_height / 2);
            let mut source = -1i64;
            let mut endpoints = Vec::new();
            endpoints
                .try_reserve_exact(usize::try_from(destination_height).ok()?)
                .ok()?;
            while endpoints.len() < usize::try_from(destination_height).ok()? {
                source = source.checked_add(1)?;
                error = error.checked_add(destination_height)?;
                while error <= 0 && source.checked_add(1)? < source_height {
                    source = source.checked_add(1)?;
                    error = error.checked_add(destination_height)?;
                }
                loop {
                    endpoints.push(usize::try_from(source).ok()?);
                    error = error.checked_sub(source_height)?;
                    if error < 0 || endpoints.len() == usize::try_from(destination_height).ok()? {
                        break;
                    }
                }
            }

            if source_height > destination_height {
                let mut start = 0usize;
                for endpoint in endpoints {
                    let end = endpoint.checked_add(1)?;
                    all_groups.push(start..end);
                    start = end;
                }
            } else {
                for source in endpoints {
                    all_groups.push(source..source.checked_add(1)?);
                }
            }
        }
        if all_groups
            .iter()
            .any(|group| group.is_empty() || group.end > source_height)
        {
            return None;
        }

        let visible_groups = all_groups.get(visible.clone())?;
        let mut groups = Vec::new();
        groups.try_reserve_exact(visible_groups.len()).ok()?;
        groups.extend(visible_groups.iter().cloned());
        let source_range = match (groups.first(), groups.last()) {
            (Some(first), Some(last)) => first.start..last.end,
            (None, None) => 0..0,
            _ => return None,
        };
        Some(Self {
            groups,
            source_range,
        })
    }

    pub(crate) fn groups(&self) -> &[Range<usize>] {
        &self.groups
    }

    pub(crate) fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }
}

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

/// Adapter facts which are not retained in [`RowCopy`]. Callers must keep
/// using their existing path unless the request had no mask, its source bounds
/// are original guest bounds rather than sanitized substitutes, its destination
/// bounds are authoritative, its missing palette map means known index identity,
/// and its raw mode is exactly `srcCopy` without the separately defined dither
/// flag.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Indexed8HorizontalSelection(());

impl Indexed8HorizontalSelection {
    pub(crate) fn from_adapter_facts(
        mask_free: bool,
        source_bounds_are_original: bool,
        destination_bounds_are_authoritative: bool,
        indexed_palette_identity_known: bool,
        raw_mode: u16,
    ) -> Option<Self> {
        (mask_free
            && source_bounds_are_original
            && destination_bounds_are_authoritative
            && indexed_palette_identity_known
            && raw_mode == 0)
            .then_some(Self(()))
    }
}

impl RowCopy<'_> {
    /// Adds the shared indexed scaling families ahead of the existing paths.
    /// Adapters receive one final outcome, so only a decline before memory is
    /// touched reaches the legacy implementation.
    pub(crate) fn execute_with_indexed8_scaling(
        self,
        memory: &mut impl CopyBitsMemory,
        selection: Option<Indexed8HorizontalSelection>,
    ) -> RowCopyOutcome {
        if let Some(selection) = selection {
            let horizontal = self.execute_indexed8_horizontal(memory, selection);
            if horizontal != RowCopyOutcome::Declined {
                return horizontal;
            }
            let vertical = self.execute_indexed8_vertical(memory, selection);
            if vertical != RowCopyOutcome::Declined {
                return vertical;
            }
        }
        self.execute(memory)
    }

    /// Adds the indexed horizontal family ahead of the existing shared paths.
    /// The adapters still receive one final outcome and therefore cannot
    /// accidentally fall back after a selected family has touched memory.
    pub(crate) fn execute_with_indexed8_horizontal(
        self,
        memory: &mut impl CopyBitsMemory,
        selection: Option<Indexed8HorizontalSelection>,
    ) -> RowCopyOutcome {
        if let Some(selection) = selection {
            let outcome = self.execute_indexed8_horizontal(memory, selection);
            if outcome != RowCopyOutcome::Declined {
                return outcome;
            }
        }
        self.execute(memory)
    }

    /// Executes the measured indexed 8-bit horizontal shrink family.
    ///
    /// Only [`RowCopyOutcome::Declined`] permits the adapter to try its old
    /// path. Once this method selects the family, address/read failures and
    /// partial writes remain terminal. Source spans for every visible row are
    /// snapshotted before the first destination write, including physical
    /// bytes beyond the declared source bounds and row stride.
    fn execute_indexed8_horizontal(
        &self,
        memory: &mut impl CopyBitsMemory,
        _selection: Indexed8HorizontalSelection,
    ) -> RowCopyOutcome {
        if self.mode != 0
            || self.source.depth != 8
            || self.destination.depth != 8
            || self.palette.is_some()
        {
            return RowCopyOutcome::Declined;
        }

        if self
            .source_rect
            .iter()
            .chain(self.destination_rect.iter())
            .chain(self.source.bounds.iter())
            .chain(self.destination.bounds.iter())
            .chain(self.clip.iter())
            .any(|&coordinate| i16::try_from(coordinate).is_err())
        {
            return RowCopyOutcome::Declined;
        }

        let [st, sl, sb, sr] = self.source_rect;
        let [dt, dl, db, dr] = self.destination_rect;
        let Some(source_width) = sr.checked_sub(sl) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(source_height) = sb.checked_sub(st) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(destination_width) = dr.checked_sub(dl) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(destination_height) = db.checked_sub(dt) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        if source_width <= 0
            || source_height <= 0
            || destination_width <= 0
            || destination_height <= 0
        {
            return RowCopyOutcome::NoOp;
        }
        if destination_width >= source_width || destination_height != source_height {
            return RowCopyOutcome::Declined;
        }
        if source_width > i32::from(i16::MAX) || source_height > i32::from(i16::MAX) {
            return RowCopyOutcome::Declined;
        }

        let [sbt, sbl, sbb, _] = self.source.bounds;
        let Some(source_bounds_height) = sbb.checked_sub(sbt) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        if source_bounds_height <= 0 {
            return RowCopyOutcome::ReadOrGeometryFailure;
        }
        if st < sbt || sb > sbb {
            return RowCopyOutcome::Declined;
        }
        let Some(source_x_delta) = sl.checked_sub(sbl) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        if i16::try_from(source_x_delta).is_err() {
            return RowCopyOutcome::Declined;
        }
        // Classic QuickDraw's signed source-bounds-height gate is observed at
        // 32767/32768 for this otherwise-selected family.
        if source_bounds_height > i32::from(i16::MAX) {
            return RowCopyOutcome::NoOp;
        }

        let [dbt, dbl, dbb, dbr] = self.destination.bounds;
        let [ct, cl, cb, cr] = self.clip;
        let top = dt.max(dbt).max(ct);
        let left = dl.max(dbl).max(cl);
        let bottom = db.min(dbb).min(cb);
        let right = dr.min(dbr).min(cr);
        if top >= bottom || left >= right {
            return RowCopyOutcome::NoOp;
        }

        let Some(visible_start) = left
            .checked_sub(dl)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(visible_end) = right
            .checked_sub(dl)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(plan) = Indexed8HorizontalShrink::new(
            source_width as usize,
            destination_width as usize,
            visible_start..visible_end,
        ) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let source_range = plan.source_range();
        let Some(row_count) = bottom
            .checked_sub(top)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(output_len) = right
            .checked_sub(left)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };

        let mut addresses = Vec::new();
        if addresses.try_reserve_exact(row_count).is_err() {
            return RowCopyOutcome::ReadOrGeometryFailure;
        }
        for destination_y in top..bottom {
            let Some(source_y) = destination_y
                .checked_sub(dt)
                .and_then(|offset| st.checked_add(offset))
            else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let source_address = if source_range.is_empty() {
                None
            } else {
                let Some(row_delta) = source_y.checked_sub(sbt).map(i64::from) else {
                    return RowCopyOutcome::ReadOrGeometryFailure;
                };
                let Some(submitted_origin) = row_delta
                    .checked_mul(i64::from(self.source.row_bytes))
                    .and_then(|offset| i64::from(self.source.base).checked_add(offset))
                    .and_then(|address| address.checked_add(i64::from(source_x_delta)))
                else {
                    return RowCopyOutcome::ReadOrGeometryFailure;
                };
                let Some(source_start) = i64::try_from(source_range.start)
                    .ok()
                    .and_then(|offset| submitted_origin.checked_add(offset))
                else {
                    return RowCopyOutcome::ReadOrGeometryFailure;
                };
                let Some(source_end) = i64::try_from(source_range.end)
                    .ok()
                    .and_then(|offset| submitted_origin.checked_add(offset))
                else {
                    return RowCopyOutcome::ReadOrGeometryFailure;
                };
                if source_start < 0 || source_end < source_start || source_end > (1i64 << 32) {
                    return RowCopyOutcome::ReadOrGeometryFailure;
                }
                Some(source_start as u32)
            };
            let Some(destination_address) =
                self.destination
                    .row_address(left, destination_y, output_len)
            else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            addresses.push((source_address, destination_address));
        }

        let Some(snapshot_len) = row_count.checked_mul(source_range.len()) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let mut snapshots = Vec::new();
        if snapshots.try_reserve_exact(snapshot_len).is_err() {
            return RowCopyOutcome::ReadOrGeometryFailure;
        }
        snapshots.resize(snapshot_len, 0);
        for (row_index, (source, _)) in addresses.iter().enumerate() {
            let Some(start) = row_index.checked_mul(source_range.len()) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let Some(end) = start.checked_add(source_range.len()) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            if let Some(source) = source {
                if memory
                    .read_copy_row(*source, &mut snapshots[start..end])
                    .is_none()
                {
                    return RowCopyOutcome::ReadOrGeometryFailure;
                }
            }
        }

        let Some(output) = plan.reduce_rows(&snapshots, row_count) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        for (rows_written, ((_, destination), row)) in addresses
            .iter()
            .zip(output.chunks_exact(output_len))
            .enumerate()
        {
            if memory.write_copy_row(*destination, row).is_none() {
                return RowCopyOutcome::WriteFailure { rows_written };
            }
        }
        RowCopyOutcome::Completed
    }

    /// Executes the measured indexed 8-bit vertical scale family. Required
    /// source rows are snapshotted before the first write. Rows belonging only
    /// to clipped-away destination groups are never addressed.
    fn execute_indexed8_vertical(
        &self,
        memory: &mut impl CopyBitsMemory,
        _selection: Indexed8HorizontalSelection,
    ) -> RowCopyOutcome {
        if self.mode != 0
            || self.source.depth != 8
            || self.destination.depth != 8
            || self.palette.is_some()
        {
            return RowCopyOutcome::Declined;
        }

        if self
            .source_rect
            .iter()
            .chain(self.destination_rect.iter())
            .chain(self.source.bounds.iter())
            .chain(self.destination.bounds.iter())
            .chain(self.clip.iter())
            .any(|&coordinate| i16::try_from(coordinate).is_err())
        {
            return RowCopyOutcome::Declined;
        }

        let [st, sl, sb, sr] = self.source_rect;
        let [dt, dl, db, dr] = self.destination_rect;
        let Some(source_width) = sr.checked_sub(sl) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(source_height) = sb.checked_sub(st) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(destination_width) = dr.checked_sub(dl) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(destination_height) = db.checked_sub(dt) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        if source_width <= 0
            || source_height <= 0
            || destination_width <= 0
            || destination_height <= 0
        {
            return RowCopyOutcome::NoOp;
        }
        if destination_width != source_width || destination_height == source_height {
            return RowCopyOutcome::Declined;
        }
        if source_width > i32::from(i16::MAX)
            || source_height > i32::from(i16::MAX)
            || destination_height > i32::from(i16::MAX)
        {
            return RowCopyOutcome::Declined;
        }

        let [sbt, sbl, sbb, sbr] = self.source.bounds;
        let Some(source_bounds_height) = sbb.checked_sub(sbt) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        if source_bounds_height <= 0 {
            return RowCopyOutcome::ReadOrGeometryFailure;
        }
        if st < sbt || sl < sbl || sb > sbb || sr > sbr {
            return RowCopyOutcome::Declined;
        }
        let Some(source_y_delta) = st.checked_sub(sbt) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(source_x_delta) = sl.checked_sub(sbl) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        if i16::try_from(source_y_delta).is_err() || i16::try_from(source_x_delta).is_err() {
            return RowCopyOutcome::Declined;
        }
        // Classic QuickDraw's signed source-bounds-height gate is observed at
        // 32767/32768 for otherwise-identical contained vertical transfers.
        if source_bounds_height > i32::from(i16::MAX) {
            return RowCopyOutcome::NoOp;
        }

        let [dbt, dbl, dbb, dbr] = self.destination.bounds;
        let [ct, cl, cb, cr] = self.clip;
        let top = dt.max(dbt).max(ct);
        let left = dl.max(dbl).max(cl);
        let bottom = db.min(dbb).min(cb);
        let right = dr.min(dbr).min(cr);
        if top >= bottom || left >= right {
            return RowCopyOutcome::NoOp;
        }

        let Some(visible_start) = top
            .checked_sub(dt)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(visible_end) = bottom
            .checked_sub(dt)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let Some(plan) = Indexed8VerticalScale::new(
            source_height as usize,
            destination_height as usize,
            visible_start..visible_end,
        ) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let source_range = plan.source_range();
        let Some(output_width) = right
            .checked_sub(left)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };

        let Some(source_x) = left
            .checked_sub(dl)
            .and_then(|offset| sl.checked_add(offset))
        else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let mut source_addresses = Vec::new();
        if source_addresses
            .try_reserve_exact(source_range.len())
            .is_err()
        {
            return RowCopyOutcome::ReadOrGeometryFailure;
        }
        for source_index in source_range.clone() {
            let Some(source_y) = i32::try_from(source_index)
                .ok()
                .and_then(|offset| st.checked_add(offset))
            else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let Some(address) = self.source.row_address(source_x, source_y, output_width) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            source_addresses.push(address);
        }

        let mut destination_addresses = Vec::new();
        if destination_addresses
            .try_reserve_exact(plan.groups().len())
            .is_err()
        {
            return RowCopyOutcome::ReadOrGeometryFailure;
        }
        for destination_y in top..bottom {
            let Some(address) = self
                .destination
                .row_address(left, destination_y, output_width)
            else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            destination_addresses.push(address);
        }

        let Some(snapshot_len) = source_range.len().checked_mul(output_width) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let mut snapshots = Vec::new();
        if snapshots.try_reserve_exact(snapshot_len).is_err() {
            return RowCopyOutcome::ReadOrGeometryFailure;
        }
        snapshots.resize(snapshot_len, 0);
        for (row, address) in source_addresses.iter().copied().enumerate() {
            let Some(start) = row.checked_mul(output_width) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            let Some(end) = start.checked_add(output_width) else {
                return RowCopyOutcome::ReadOrGeometryFailure;
            };
            if memory
                .read_copy_row(address, &mut snapshots[start..end])
                .is_none()
            {
                return RowCopyOutcome::ReadOrGeometryFailure;
            }
        }

        let Some(output_len) = plan.groups().len().checked_mul(output_width) else {
            return RowCopyOutcome::ReadOrGeometryFailure;
        };
        let mut output = Vec::new();
        if output.try_reserve_exact(output_len).is_err() {
            return RowCopyOutcome::ReadOrGeometryFailure;
        }
        for group in plan.groups() {
            for x in 0..output_width {
                let mut maximum = None;
                for source_index in group.clone() {
                    let Some(row) = source_index.checked_sub(source_range.start) else {
                        return RowCopyOutcome::ReadOrGeometryFailure;
                    };
                    let Some(index) = row
                        .checked_mul(output_width)
                        .and_then(|start| start.checked_add(x))
                    else {
                        return RowCopyOutcome::ReadOrGeometryFailure;
                    };
                    let Some(value) = snapshots.get(index).copied() else {
                        return RowCopyOutcome::ReadOrGeometryFailure;
                    };
                    maximum = Some(maximum.map_or(value, |current: u8| current.max(value)));
                }
                let Some(maximum) = maximum else {
                    return RowCopyOutcome::ReadOrGeometryFailure;
                };
                output.push(maximum);
            }
        }

        for (rows_written, (address, row)) in destination_addresses
            .iter()
            .copied()
            .zip(output.chunks_exact(output_width))
            .enumerate()
        {
            if memory.write_copy_row(address, row).is_none() {
                return RowCopyOutcome::WriteFailure { rows_written };
            }
        }
        RowCopyOutcome::Completed
    }

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

    #[test]
    fn indexed_horizontal_groups_match_independent_oracle_boundaries() {
        for (source, destination, expected) in [
            (3, 2, &[0..2, 2..3][..]),
            (5, 3, &[0..2, 2..3, 3..5][..]),
            (
                17,
                7,
                &[0..2, 2..5, 5..7, 7..10, 10..12, 12..15, 15..17][..],
            ),
            (
                17,
                16,
                &[
                    0..1,
                    1..2,
                    2..3,
                    3..4,
                    4..5,
                    5..6,
                    6..7,
                    7..9,
                    9..10,
                    10..11,
                    11..12,
                    12..13,
                    13..14,
                    14..15,
                    15..16,
                    16..17,
                ][..],
            ),
        ] {
            let plan = Indexed8HorizontalShrink::new(source, destination, 0..destination)
                .expect("oracle geometry is supported");
            assert_eq!(plan.groups(), expected);
        }
    }

    #[test]
    fn indexed_horizontal_boundaries_match_independent_carry_loop() {
        for source in 2usize..=32 {
            for destination in 1usize..source {
                let step = (destination * (1 << 16) / source) as u16;
                let mut error = step / 2;
                let mut source_index = 0;
                let mut expected = Vec::with_capacity(destination);
                for _ in 0..destination {
                    let start = source_index;
                    loop {
                        source_index += 1;
                        let (next, carry) = error.overflowing_add(step);
                        error = next;
                        if carry {
                            break;
                        }
                    }
                    expected.push(start..source_index);
                }
                let plan = Indexed8HorizontalShrink::new(source, destination, 0..destination)
                    .expect("small positive shrink is supported");
                assert_eq!(
                    plan.groups(),
                    expected,
                    "source={source}, destination={destination}"
                );
            }
        }
    }

    #[test]
    fn indexed_horizontal_plan_uses_signed_zero_boundary_and_visible_groups() {
        let first = Indexed8HorizontalShrink::new(285, 2, 0..1).unwrap();
        assert_eq!(first.groups(), &[0..143]);
        assert_eq!(first.source_range(), 0..143);

        let last = Indexed8HorizontalShrink::new(285, 2, 1..2).unwrap();
        assert_eq!(last.groups(), &[143..286]);
        assert_eq!(last.source_range(), 143..286);

        let clipped_away = Indexed8HorizontalShrink::new(190, 1, 0..0).unwrap();
        assert!(clipped_away.groups().is_empty());
        assert_eq!(clipped_away.source_range(), 0..0);
        assert_eq!(clipped_away.reduce(&[]), Some(vec![]));
    }

    #[test]
    fn indexed_horizontal_origin_residues_use_only_submitted_pixels() {
        for residue in 0..4 {
            let mut row = vec![0x11; 256];
            row[..residue].fill(250);
            row[residue..residue + 190].fill(20);
            row[residue + 189] = 30;
            row[residue + 190..residue + 193].copy_from_slice(&[200, 150, 140]);
            let plan = Indexed8HorizontalShrink::new(190, 1, 0..1).unwrap();
            assert_eq!(plan.groups(), &[0..191]);
            assert_eq!(plan.source_range(), 0..191);
            let range = plan.source_range();
            assert_eq!(
                plan.reduce(&row[residue + range.start..residue + range.end]),
                Some(vec![200])
            );

            row.fill(0x11);
            row[..residue].fill(250);
            row[residue..residue + 191].fill(20);
            row[residue + 190] = 30;
            row[residue + 191..residue + 194].copy_from_slice(&[240, 150, 140]);
            let plan = Indexed8HorizontalShrink::new(191, 1, 0..1).unwrap();
            assert_eq!(plan.groups(), &[0..191]);
            assert_eq!(plan.source_range(), 0..191);
            let range = plan.source_range();
            assert_eq!(
                plan.reduce(&row[residue + range.start..residue + range.end]),
                Some(vec![30])
            );
        }
    }

    #[test]
    fn indexed_horizontal_owned_map_matches_large_trace_pixels() {
        for (source_width, endpoint, expected) in [
            (4_097, 4_369, 65),
            (5_000, 5_041, 8),
            (5_001, 5_041, 7),
            (10_924, 13_107, u8::MAX),
        ] {
            let plan = Indexed8HorizontalShrink::new(source_width, 1, 0..1).unwrap();
            assert_eq!(plan.groups(), &[0..endpoint]);
            let range = plan.source_range();
            assert_eq!(range.start, 0);
            assert_eq!(
                plan.reduce(&vec![0; range.len()]),
                Some(vec![expected]),
                "source_width={source_width}"
            );
        }
    }

    #[test]
    fn indexed_horizontal_final_column_stays_bounded_near_signed_limit() {
        let guarded = Indexed8HorizontalShrink::new(32_761, 2, 1..2).unwrap();
        assert_eq!(guarded.groups(), &[16_384..32_768]);
        assert_eq!(guarded.source_range(), 16_384..32_764);
        let mut guarded_source = vec![0; guarded.source_range().len()];
        *guarded_source.last_mut().unwrap() = 77;
        assert_eq!(guarded.reduce(&guarded_source), Some(vec![77]));

        let unrounded = Indexed8HorizontalShrink::new(32_767, 2, 1..2).unwrap();
        assert_eq!(unrounded.groups(), &[16_384..32_768]);
        assert_eq!(unrounded.source_range(), 16_384..32_768);
        let mut unrounded_source = vec![0; unrounded.source_range().len()];
        *unrounded_source.last_mut().unwrap() = 88;
        assert_eq!(unrounded.reduce(&unrounded_source), Some(vec![88]));

        let saturated = Indexed8HorizontalShrink::new(31_736, 2, 1..2).unwrap();
        assert_eq!(saturated.groups(), &[16_384..32_768]);
        assert_eq!(saturated.source_range(), 16_384..31_736);
        assert_eq!(
            saturated.reduce(&vec![0; saturated.source_range().len()]),
            Some(vec![u8::MAX])
        );
    }

    #[test]
    fn indexed_horizontal_plan_rejects_unproved_domains_and_wrong_snapshot() {
        for (source, destination, visible) in [
            (0, 1, 0..1),
            (1, 1, 0..1),
            (1, 2, 0..2),
            (i16::MAX as usize + 1, 1, 0..1),
            (5, 3, std::ops::Range { start: 2, end: 1 }),
            (5, 3, 0..4),
        ] {
            assert!(Indexed8HorizontalShrink::new(source, destination, visible).is_none());
        }
        let plan = Indexed8HorizontalShrink::new(190, 1, 0..1).unwrap();
        assert_eq!(plan.reduce(&vec![0; 190]), None);
        assert_eq!(plan.reduce(&vec![0; 192]), None);
    }

    // Monotonic source-row indices captured through CopyBits on Mac OS 8.1
    // for every source/destination height pair in 1..=16.
    const VERTICAL_GRID_SOURCE_ROWS: &[u8] = &[
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 1,
        1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 1, 1, 1,
        1, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 1,
        1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1,
        1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1,
        1, 1, 2, 0, 2, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 1, 2, 0, 0, 1, 1, 2, 2, 0, 0, 0, 1, 1, 2, 2,
        0, 0, 0, 1, 1, 1, 2, 2, 0, 0, 0, 1, 1, 1, 2, 2, 2, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 0, 0, 0,
        0, 1, 1, 1, 1, 2, 2, 2, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2,
        2, 2, 2, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2,
        2, 2, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 1, 3, 0, 2, 3, 0, 1, 2, 3, 0, 1,
        1, 2, 3, 0, 0, 1, 2, 2, 3, 0, 0, 1, 1, 2, 3, 3, 0, 0, 1, 1, 2, 2, 3, 3, 0, 0, 1, 1, 1, 2,
        2, 3, 3, 0, 0, 0, 1, 1, 2, 2, 2, 3, 3, 0, 0, 0, 1, 1, 1, 2, 2, 3, 3, 3, 0, 0, 0, 1, 1, 1,
        2, 2, 2, 3, 3, 3, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 3, 3, 3, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 2,
        3, 3, 3, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2,
        3, 3, 3, 3, 4, 1, 3, 0, 2, 4, 0, 1, 3, 4, 0, 1, 2, 3, 4, 0, 1, 1, 2, 3, 4, 0, 0, 1, 2, 3,
        3, 4, 0, 0, 1, 2, 2, 3, 3, 4, 0, 0, 1, 1, 2, 2, 3, 4, 4, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 0,
        0, 1, 1, 1, 2, 2, 3, 3, 4, 4, 0, 0, 0, 1, 1, 2, 2, 3, 3, 3, 4, 4, 0, 0, 0, 1, 1, 2, 2, 2,
        3, 3, 3, 4, 4, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 4, 4, 4, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3,
        3, 4, 4, 4, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 2, 5, 1, 3, 5, 0, 2, 3, 5,
        0, 1, 3, 4, 5, 0, 1, 2, 3, 4, 5, 0, 1, 2, 2, 3, 4, 5, 0, 1, 1, 2, 3, 4, 4, 5, 0, 0, 1, 2,
        2, 3, 4, 4, 5, 0, 0, 1, 2, 2, 3, 3, 4, 5, 5, 0, 0, 1, 1, 2, 2, 3, 4, 4, 5, 5, 0, 0, 1, 1,
        2, 2, 3, 3, 4, 4, 5, 5, 0, 0, 1, 1, 2, 2, 2, 3, 3, 4, 4, 5, 5, 0, 0, 1, 1, 1, 2, 2, 3, 3,
        4, 4, 4, 5, 5, 0, 0, 0, 1, 1, 2, 2, 2, 3, 3, 4, 4, 4, 5, 5, 0, 0, 0, 1, 1, 2, 2, 2, 3, 3,
        3, 4, 4, 5, 5, 5, 6, 1, 5, 1, 3, 5, 0, 2, 4, 6, 0, 2, 3, 4, 6, 0, 1, 2, 4, 5, 6, 0, 1, 2,
        3, 4, 5, 6, 0, 1, 2, 2, 3, 4, 5, 6, 0, 1, 1, 2, 3, 4, 4, 5, 6, 0, 0, 1, 2, 3, 3, 4, 5, 5,
        6, 0, 0, 1, 2, 2, 3, 4, 4, 5, 5, 6, 0, 0, 1, 1, 2, 3, 3, 4, 4, 5, 6, 6, 0, 0, 1, 1, 2, 2,
        3, 3, 4, 5, 5, 6, 6, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 0, 0, 1, 1, 2, 2, 2, 3, 3,
        4, 4, 5, 5, 6, 6, 0, 0, 1, 1, 1, 2, 2, 3, 3, 4, 4, 4, 5, 5, 6, 6, 7, 3, 7, 1, 4, 6, 1, 3,
        5, 7, 0, 2, 4, 5, 7, 0, 2, 3, 4, 6, 7, 0, 1, 2, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7, 0, 1,
        2, 3, 3, 4, 5, 6, 7, 0, 1, 1, 2, 3, 4, 5, 5, 6, 7, 0, 1, 1, 2, 3, 3, 4, 5, 6, 6, 7, 0, 0,
        1, 2, 2, 3, 4, 4, 5, 6, 6, 7, 0, 0, 1, 2, 2, 3, 3, 4, 5, 5, 6, 7, 7, 0, 0, 1, 1, 2, 3, 3,
        4, 4, 5, 5, 6, 7, 7, 0, 0, 1, 1, 2, 2, 3, 3, 4, 5, 5, 6, 6, 7, 7, 0, 0, 1, 1, 2, 2, 3, 3,
        4, 4, 5, 5, 6, 6, 7, 7, 8, 2, 6, 2, 5, 8, 1, 3, 5, 7, 0, 2, 4, 6, 8, 0, 2, 3, 5, 6, 8, 0,
        1, 3, 4, 5, 7, 8, 0, 1, 2, 3, 5, 6, 7, 8, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 1, 2, 3, 3, 4, 5,
        6, 7, 8, 0, 1, 1, 2, 3, 4, 5, 6, 6, 7, 8, 0, 1, 1, 2, 3, 4, 4, 5, 6, 7, 7, 8, 0, 0, 1, 2,
        3, 3, 4, 5, 5, 6, 7, 7, 8, 0, 0, 1, 2, 2, 3, 4, 4, 5, 6, 6, 7, 7, 8, 0, 0, 1, 2, 2, 3, 3,
        4, 5, 5, 6, 6, 7, 8, 8, 0, 0, 1, 1, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 9, 4, 9, 1, 5, 8,
        1, 3, 6, 8, 1, 3, 5, 7, 9, 0, 2, 4, 5, 7, 9, 0, 2, 3, 5, 6, 7, 9, 0, 1, 3, 4, 5, 6, 8, 9,
        0, 1, 2, 3, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 4, 5, 6, 7, 8, 9,
        0, 1, 2, 2, 3, 4, 5, 6, 7, 7, 8, 9, 0, 1, 1, 2, 3, 4, 4, 5, 6, 7, 8, 8, 9, 0, 1, 1, 2, 3,
        3, 4, 5, 6, 6, 7, 8, 8, 9, 0, 0, 1, 2, 2, 3, 4, 4, 5, 6, 6, 7, 8, 8, 9, 0, 0, 1, 2, 2, 3,
        4, 4, 5, 5, 6, 7, 7, 8, 9, 9, 10, 2, 8, 1, 5, 9, 1, 4, 6, 9, 1, 3, 5, 7, 9, 0, 2, 4, 6, 8,
        10, 0, 2, 3, 5, 7, 8, 10, 0, 2, 3, 4, 6, 7, 8, 10, 0, 1, 3, 4, 5, 6, 7, 9, 10, 0, 1, 2, 3,
        4, 6, 7, 8, 9, 10, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 1, 2, 3, 4, 4, 5, 6, 7, 8, 9, 10,
        0, 1, 2, 2, 3, 4, 5, 6, 7, 7, 8, 9, 10, 0, 1, 1, 2, 3, 4, 5, 5, 6, 7, 8, 8, 9, 10, 0, 1, 1,
        2, 3, 3, 4, 5, 6, 6, 7, 8, 9, 9, 10, 0, 0, 1, 2, 3, 3, 4, 5, 5, 6, 7, 7, 8, 9, 9, 10, 11,
        5, 11, 3, 7, 11, 2, 5, 8, 11, 1, 3, 6, 8, 10, 1, 3, 5, 7, 9, 11, 0, 2, 4, 6, 7, 9, 11, 0,
        2, 3, 5, 6, 8, 9, 11, 0, 2, 3, 4, 6, 7, 8, 10, 11, 0, 1, 3, 4, 5, 6, 7, 9, 10, 11, 0, 1, 2,
        3, 4, 6, 7, 8, 9, 10, 11, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 0, 1, 2, 3, 4, 5, 5, 6, 7,
        8, 9, 10, 11, 0, 1, 2, 2, 3, 4, 5, 6, 7, 8, 8, 9, 10, 11, 0, 1, 1, 2, 3, 4, 5, 5, 6, 7, 8,
        9, 9, 10, 11, 0, 1, 1, 2, 3, 4, 4, 5, 6, 7, 7, 8, 9, 10, 10, 11, 12, 3, 9, 2, 6, 10, 1, 4,
        8, 11, 1, 3, 6, 9, 11, 1, 3, 5, 7, 9, 11, 0, 2, 4, 6, 8, 10, 12, 0, 2, 4, 5, 7, 8, 10, 12,
        0, 2, 3, 5, 6, 7, 9, 10, 12, 0, 1, 3, 4, 5, 7, 8, 9, 11, 12, 0, 1, 2, 4, 5, 6, 7, 8, 10,
        11, 12, 0, 1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 0,
        1, 2, 3, 4, 5, 5, 6, 7, 8, 9, 10, 11, 12, 0, 1, 2, 2, 3, 4, 5, 6, 7, 8, 9, 9, 10, 11, 12,
        0, 1, 1, 2, 3, 4, 5, 6, 6, 7, 8, 9, 10, 10, 11, 12, 13, 6, 13, 2, 7, 11, 1, 5, 8, 12, 1, 4,
        7, 9, 12, 1, 3, 5, 8, 10, 12, 1, 3, 5, 7, 9, 11, 13, 0, 2, 4, 6, 7, 9, 11, 13, 0, 2, 3, 5,
        7, 8, 10, 11, 13, 0, 2, 3, 4, 6, 7, 9, 10, 11, 13, 0, 1, 3, 4, 5, 7, 8, 9, 10, 12, 13, 0,
        1, 2, 4, 5, 6, 7, 8, 9, 11, 12, 13, 0, 1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12, 13, 0, 1, 2, 3,
        4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 0, 1, 2, 3, 4, 5, 6, 6, 7, 8, 9, 10, 11, 12, 13, 0, 1, 2,
        3, 3, 4, 5, 6, 7, 8, 9, 10, 10, 11, 12, 13, 14, 3, 11, 4, 9, 14, 1, 5, 9, 13, 2, 5, 8, 11,
        14, 1, 3, 6, 8, 11, 13, 1, 3, 5, 7, 9, 11, 13, 0, 2, 4, 6, 8, 10, 12, 14, 0, 2, 4, 5, 7, 9,
        10, 12, 14, 0, 2, 3, 5, 6, 8, 9, 11, 12, 14, 0, 2, 3, 4, 6, 7, 8, 10, 11, 12, 14, 0, 1, 3,
        4, 5, 6, 8, 9, 10, 11, 13, 14, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 12, 13, 14, 0, 1, 2, 3, 4, 5,
        6, 8, 9, 10, 11, 12, 13, 14, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 0, 1, 2, 3,
        4, 5, 6, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 7, 15, 2, 8, 13, 3, 7, 11, 15, 1, 4, 8, 11,
        14, 1, 4, 6, 9, 12, 14, 1, 3, 5, 8, 10, 12, 14, 1, 3, 5, 7, 9, 11, 13, 15, 0, 2, 4, 6, 8,
        9, 11, 13, 15, 0, 2, 4, 5, 7, 8, 10, 12, 13, 15, 0, 2, 3, 5, 6, 8, 9, 10, 12, 13, 15, 0, 2,
        3, 4, 6, 7, 8, 10, 11, 12, 14, 15, 0, 1, 3, 4, 5, 6, 8, 9, 10, 11, 12, 14, 15, 0, 1, 2, 4,
        5, 6, 7, 8, 9, 10, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1,
        2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ];

    #[test]
    fn indexed_vertical_groups_match_independent_oracle_boundaries() {
        for (source, destination, expected) in [
            (3, 2, &[0..1, 1..3][..]),
            (5, 3, &[0..1, 1..3, 3..5][..]),
            (17, 7, &[0..2, 2..4, 4..7, 7..9, 9..11, 11..14, 14..16][..]),
            (
                17,
                16,
                &[
                    0..1,
                    1..2,
                    2..3,
                    3..4,
                    4..5,
                    5..6,
                    6..7,
                    7..8,
                    8..10,
                    10..11,
                    11..12,
                    12..13,
                    13..14,
                    14..15,
                    15..16,
                    16..17,
                ][..],
            ),
            (
                17,
                18,
                &[
                    0..1,
                    1..2,
                    2..3,
                    3..4,
                    4..5,
                    5..6,
                    6..7,
                    7..8,
                    7..8,
                    8..9,
                    9..10,
                    10..11,
                    11..12,
                    12..13,
                    13..14,
                    14..15,
                    15..16,
                    16..17,
                ][..],
            ),
        ] {
            let plan = Indexed8VerticalScale::new(source, destination, 0..destination)
                .expect("captured vertical geometry is supported");
            assert_eq!(plan.groups(), expected);
        }

        for (source, destination, expected) in [
            (4, 1, &[0..4][..]),
            (8, 2, &[0..4, 4..8][..]),
            (6, 2, &[0..3, 3..6][..]),
        ] {
            let plan = Indexed8VerticalScale::new(source, destination, 0..destination).unwrap();
            assert_eq!(plan.groups(), expected);
        }
        for (source, factor) in [(34, 2), (51, 3)] {
            let expected: Vec<_> = (0..17)
                .map(|index| index * factor..index * factor + factor)
                .collect();
            let plan = Indexed8VerticalScale::new(source, 17, 0..17).unwrap();
            assert_eq!(plan.groups(), expected);
        }
    }

    #[test]
    fn indexed_vertical_groups_match_complete_captured_small_grid() {
        let mut cursor = 0;
        for source in 1..=16 {
            for destination in 1..=16 {
                let expected = &VERTICAL_GRID_SOURCE_ROWS[cursor..cursor + destination];
                cursor += destination;
                let plan = Indexed8VerticalScale::new(source, destination, 0..destination).unwrap();
                let selected: Vec<u8> = plan
                    .groups()
                    .iter()
                    .map(|group| u8::try_from(group.end - 1).unwrap())
                    .collect();
                assert_eq!(
                    selected, expected,
                    "source={source}, destination={destination}"
                );
            }
        }
        assert_eq!(cursor, VERTICAL_GRID_SOURCE_ROWS.len());
    }

    #[test]
    fn indexed_vertical_phase_holdouts_and_visible_slice_match_captures() {
        let shrink = Indexed8VerticalScale::new(257, 256, 0..256).unwrap();
        let shrink_rows: Vec<_> = shrink.groups().iter().map(|group| group.end - 1).collect();
        let expected_shrink: Vec<_> = (0..128).chain(129..257).collect();
        assert_eq!(shrink_rows, expected_shrink);
        let enlarge = Indexed8VerticalScale::new(256, 257, 0..257).unwrap();
        let enlarge_rows: Vec<_> = enlarge.groups().iter().map(|group| group.start).collect();
        let expected_enlarge: Vec<_> = (0..128).chain(127..256).collect();
        assert_eq!(enlarge_rows, expected_enlarge);

        let full = Indexed8VerticalScale::new(17, 7, 0..7).unwrap();
        let clipped = Indexed8VerticalScale::new(17, 7, 2..6).unwrap();
        assert_eq!(clipped.groups(), &full.groups()[2..6]);
        assert_eq!(clipped.source_range(), 4..14);
        for visible in [0..1, 3..4, 6..7] {
            let sliced = Indexed8VerticalScale::new(17, 7, visible.clone()).unwrap();
            assert_eq!(sliced.groups(), &full.groups()[visible]);
        }
        assert!(Indexed8VerticalScale::new(17, 7, 3..3)
            .unwrap()
            .source_range()
            .is_empty());
    }

    #[test]
    fn indexed_vertical_plan_has_checked_nonempty_groups_over_broad_domain() {
        for source in 1..=128 {
            for destination in 1..=128 {
                let plan = Indexed8VerticalScale::new(source, destination, 0..destination).unwrap();
                assert_eq!(plan.groups().len(), destination);
                assert!(plan.groups().iter().all(|group| !group.is_empty()));
                assert!(plan
                    .groups()
                    .windows(2)
                    .all(|pair| pair[0].start <= pair[1].start));
                assert!(plan.groups().iter().all(|group| group.end <= source));
            }
        }
        for (source, destination) in [
            (190, 1),
            (191, 1),
            (256, 257),
            (257, 256),
            (285, 2),
            (511, 3),
            (4097, 1),
            (5000, 3),
            (10_924, 1),
            (32_767, 2),
            (2, 32_767),
        ] {
            let plan = Indexed8VerticalScale::new(source, destination, 0..destination).unwrap();
            assert_eq!(plan.groups().len(), destination);
            assert!(plan
                .groups()
                .iter()
                .all(|group| !group.is_empty() && group.end <= source));
            assert!(plan.source_range().end <= source);
        }
        for (source, destination, visible) in [
            (0, 1, 0..1),
            (1, 0, 0..0),
            (i16::MAX as usize + 1, 1, 0..1),
            (1, i16::MAX as usize + 1, 0..1),
            (5, 3, std::ops::Range { start: 2, end: 1 }),
            (5, 3, 0..4),
        ] {
            assert!(Indexed8VerticalScale::new(source, destination, visible).is_none());
        }
    }

    #[test]
    fn indexed_vertical_groups_match_independent_source_row_walk() {
        for source in 1usize..=64 {
            for destination in 1usize..=64 {
                if source == destination {
                    continue;
                }
                let expected = if source > destination && source % destination == 0 {
                    let height = source / destination;
                    (0..destination)
                        .map(|index| index * height..(index + 1) * height)
                        .collect::<Vec<_>>()
                } else {
                    // This reference walks each source row once and emits all
                    // destination rows reached there, rather than advancing
                    // source rows on demand for each destination endpoint.
                    let mut error = -(i64::try_from(source).unwrap() / 2);
                    let mut endpoints = Vec::new();
                    for source_row in 0..source {
                        error += i64::try_from(destination).unwrap();
                        if error > 0 {
                            loop {
                                endpoints.push(source_row);
                                error -= i64::try_from(source).unwrap();
                                if error < 0 || endpoints.len() == destination {
                                    break;
                                }
                            }
                        }
                    }
                    if source > destination {
                        let mut start = 0;
                        endpoints
                            .into_iter()
                            .map(|endpoint| {
                                let group = start..endpoint + 1;
                                start = endpoint + 1;
                                group
                            })
                            .collect()
                    } else {
                        endpoints
                            .into_iter()
                            .map(|source_row| source_row..source_row + 1)
                            .collect()
                    }
                };
                let plan = Indexed8VerticalScale::new(source, destination, 0..destination).unwrap();
                assert_eq!(
                    plan.groups(),
                    expected,
                    "source={source}, destination={destination}"
                );
            }
        }
    }

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

    fn indexed_selection() -> Indexed8HorizontalSelection {
        Indexed8HorizontalSelection::from_adapter_facts(true, true, true, true, 0).unwrap()
    }

    #[derive(Default)]
    struct SparseMemory {
        bytes: std::collections::BTreeMap<u32, u8>,
        fail_read: Option<u32>,
        fail_write: Option<u32>,
        writes: Vec<u32>,
    }

    impl SparseMemory {
        fn insert(&mut self, address: u32, bytes: &[u8]) {
            for (offset, byte) in bytes.iter().copied().enumerate() {
                self.bytes.insert(address + offset as u32, byte);
            }
        }

        fn bytes(&self, address: u32, len: usize) -> Vec<u8> {
            (0..len)
                .map(|offset| self.bytes[&(address + offset as u32)])
                .collect()
        }
    }

    impl CopyBitsMemory for SparseMemory {
        fn read_copy_row(&mut self, address: u32, bytes: &mut [u8]) -> Option<()> {
            if self.fail_read == Some(address) {
                return None;
            }
            for (offset, byte) in bytes.iter_mut().enumerate() {
                *byte = *self.bytes.get(&address.checked_add(offset as u32)?)?;
            }
            Some(())
        }

        fn write_copy_row(&mut self, address: u32, bytes: &[u8]) -> Option<()> {
            if self.fail_write == Some(address) {
                return None;
            }
            self.writes.push(address);
            for (offset, byte) in bytes.iter().copied().enumerate() {
                self.bytes.insert(address.checked_add(offset as u32)?, byte);
            }
            Some(())
        }
    }

    #[test]
    fn indexed_vertical_executor_preserves_phase_and_avoids_discarded_rows_and_columns() {
        let mut memory = SparseMemory::default();
        // Only the source rows and columns needed by logical destination rows
        // 2..6 and columns 1..3 are mapped.
        for source_index in 4u32..14 {
            memory.insert(
                SOURCE + (source_index + 5) * 4 + 1,
                &[20 + source_index as u8, 40 + source_index as u8],
            );
        }
        memory.insert(DESTINATION, &[0xa5; 28]);
        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 4, 8, [0, 0, 30, 3]),
            destination: pixmap(DESTINATION, 4, 8, [0, 10, 7, 13]),
            source_rect: [5, 0, 22, 3],
            destination_rect: [0, 10, 7, 13],
            clip: [2, 11, 6, 13],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_scaling(&mut memory, Some(indexed_selection())),
            RowCopyOutcome::Completed
        );
        assert_eq!(
            memory.bytes(DESTINATION, 28),
            [
                0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 26, 46, 0xa5, 0xa5, 28, 48,
                0xa5, 0xa5, 30, 50, 0xa5, 0xa5, 33, 53, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5,
            ]
        );
    }

    #[test]
    fn indexed_vertical_executor_enlarges_by_repeating_captured_rows() {
        let mut memory = SparseMemory::default();
        memory.insert(SOURCE, &[10, 20, 30, 40, 50]);
        memory.insert(DESTINATION, &[0xa5; 14]);
        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 1, 8, [0, 0, 5, 1]),
            destination: pixmap(DESTINATION, 2, 8, [0, 0, 7, 1]),
            source_rect: [0, 0, 5, 1],
            destination_rect: [0, 0, 7, 1],
            clip: [0, 0, 7, 1],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_scaling(&mut memory, Some(indexed_selection())),
            RowCopyOutcome::Completed
        );
        assert_eq!(
            memory.bytes(DESTINATION, 14),
            [10, 0xa5, 10, 0xa5, 20, 0xa5, 30, 0xa5, 40, 0xa5, 40, 0xa5, 50, 0xa5]
        );
    }

    #[test]
    fn indexed_vertical_executor_snapshots_aliasing_rows_before_writes() {
        let mut memory = SparseMemory::default();
        memory.insert(SOURCE, &[1, 11, 4, 14, 3, 13, 8, 18, 2, 12]);
        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 2, 8, [0, 0, 5, 2]),
            destination: pixmap(SOURCE + 2, 2, 8, [0, 0, 3, 2]),
            source_rect: [0, 0, 5, 2],
            destination_rect: [0, 0, 3, 2],
            clip: [0, 0, 3, 2],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_scaling(&mut memory, Some(indexed_selection())),
            RowCopyOutcome::Completed
        );
        assert_eq!(memory.bytes(SOURCE + 2, 6), [1, 11, 4, 14, 8, 18]);

        let mut reverse = SparseMemory::default();
        reverse.insert(SOURCE, &[0xa5, 0xa5, 1, 11, 4, 14, 3, 13, 8, 18, 2, 12]);
        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE + 2, 2, 8, [0, 0, 5, 2]),
            destination: pixmap(SOURCE, 2, 8, [0, 0, 3, 2]),
            source_rect: [0, 0, 5, 2],
            destination_rect: [0, 0, 3, 2],
            clip: [0, 0, 3, 2],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_scaling(&mut reverse, Some(indexed_selection())),
            RowCopyOutcome::Completed
        );
        assert_eq!(reverse.bytes(SOURCE, 6), [1, 11, 4, 14, 8, 18]);

        let mut same_base = SparseMemory::default();
        same_base.insert(SOURCE, &[1, 11, 4, 14, 3, 13, 8, 18, 2, 12]);
        let reduction = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 2, 8, [0, 0, 5, 2]),
            destination: pixmap(SOURCE, 2, 8, [0, 0, 3, 2]),
            source_rect: [0, 0, 5, 2],
            destination_rect: [0, 0, 3, 2],
            clip: [0, 0, 3, 2],
            palette: None,
        };
        assert_eq!(
            reduction.execute_with_indexed8_scaling(&mut same_base, Some(indexed_selection())),
            RowCopyOutcome::Completed
        );
        assert_eq!(same_base.bytes(SOURCE, 6), [1, 11, 4, 14, 8, 18]);

        let mut same_base_enlarge = SparseMemory::default();
        same_base_enlarge.insert(
            SOURCE,
            &[
                10, 0xa5, 20, 0xa5, 30, 0xa5, 40, 0xa5, 50, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5,
            ],
        );
        let enlargement = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 2, 8, [0, 0, 5, 1]),
            destination: pixmap(SOURCE, 2, 8, [0, 0, 7, 1]),
            source_rect: [0, 0, 5, 1],
            destination_rect: [0, 0, 7, 1],
            clip: [0, 0, 7, 1],
            palette: None,
        };
        assert_eq!(
            enlargement
                .execute_with_indexed8_scaling(&mut same_base_enlarge, Some(indexed_selection()),),
            RowCopyOutcome::Completed
        );
        assert_eq!(
            same_base_enlarge.bytes(SOURCE, 14),
            [10, 0xa5, 10, 0xa5, 20, 0xa5, 30, 0xa5, 40, 0xa5, 40, 0xa5, 50, 0xa5]
        );
    }

    #[test]
    fn indexed_vertical_failures_preserve_prewrite_and_partial_row_contracts() {
        let request = || RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 2, 8, [0, 0, 5, 2]),
            destination: pixmap(DESTINATION, 2, 8, [0, 0, 3, 2]),
            source_rect: [0, 0, 5, 2],
            destination_rect: [0, 0, 3, 2],
            clip: [0, 0, 3, 2],
            palette: None,
        };

        let mut read_failure = SparseMemory::default();
        read_failure.insert(SOURCE, &[1, 11, 4, 14, 3, 13, 8, 18, 2, 12]);
        read_failure.insert(DESTINATION, &[0xa5; 6]);
        read_failure.fail_read = Some(SOURCE + 4);
        assert_eq!(
            request().execute_with_indexed8_scaling(&mut read_failure, Some(indexed_selection()),),
            RowCopyOutcome::ReadOrGeometryFailure
        );
        assert!(read_failure.writes.is_empty());
        assert_eq!(read_failure.bytes(DESTINATION, 6), [0xa5; 6]);

        let mut write_failure = SparseMemory::default();
        write_failure.insert(SOURCE, &[1, 11, 4, 14, 3, 13, 8, 18, 2, 12]);
        write_failure.insert(DESTINATION, &[0xa5; 6]);
        write_failure.fail_write = Some(DESTINATION + 2);
        assert_eq!(
            request().execute_with_indexed8_scaling(&mut write_failure, Some(indexed_selection()),),
            RowCopyOutcome::WriteFailure { rows_written: 1 }
        );
        assert_eq!(write_failure.writes, [DESTINATION]);
        assert_eq!(
            write_failure.bytes(DESTINATION, 6),
            [1, 11, 0xa5, 0xa5, 0xa5, 0xa5]
        );

        let mut first_write_failure = SparseMemory::default();
        first_write_failure.insert(SOURCE, &[1, 11, 4, 14, 3, 13, 8, 18, 2, 12]);
        first_write_failure.insert(DESTINATION, &[0xa5; 6]);
        first_write_failure.fail_write = Some(DESTINATION);
        assert_eq!(
            request().execute_with_indexed8_scaling(
                &mut first_write_failure,
                Some(indexed_selection()),
            ),
            RowCopyOutcome::WriteFailure { rows_written: 0 }
        );
        assert!(first_write_failure.writes.is_empty());
        assert_eq!(first_write_failure.bytes(DESTINATION, 6), [0xa5; 6]);
    }

    #[test]
    fn indexed_vertical_signed_source_height_noop_and_exclusions_do_not_access_memory() {
        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("excluded vertical transfer reached source memory");
            }

            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("excluded vertical transfer reached destination memory");
            }
        }

        let request = |source_bounds, source_rect, destination_rect, palette| RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 8, 8, source_bounds),
            destination: pixmap(DESTINATION, 8, 8, [0, 0, 8, 8]),
            source_rect,
            destination_rect,
            clip: [0, 0, 8, 8],
            palette,
        };
        assert_eq!(
            request([-1, 0, 32_767, 7], [-1, 0, 0, 7], [0, 0, 3, 7], None,)
                .execute_indexed8_vertical(&mut NoAccess, indexed_selection()),
            RowCopyOutcome::NoOp
        );
        assert_eq!(
            request([0, 0, 5, 7], [0, 0, 5, 7], [0, 0, 3, 6], None)
                .execute_indexed8_vertical(&mut NoAccess, indexed_selection()),
            RowCopyOutcome::Declined
        );
        assert_eq!(
            request([0, 0, 5, 7], [0, 0, 5, 7], [0, 0, 3, 7], Some(&[0; 256]))
                .execute_indexed8_vertical(&mut NoAccess, indexed_selection()),
            RowCopyOutcome::Declined
        );
        assert_eq!(
            request(
                [i16::MIN.into(), 0, i16::MAX.into(), 7],
                [0, 0, 5, 7],
                [0, 0, 3, 7],
                None,
            )
            .execute_indexed8_vertical(&mut NoAccess, indexed_selection()),
            RowCopyOutcome::Declined
        );
        assert_eq!(
            request(
                [i16::MIN.into(), 0, i16::MAX.into(), 7],
                [i16::MIN.into(), 0, 1, 7],
                [0, 0, 3, 7],
                None,
            )
            .execute_indexed8_vertical(&mut NoAccess, indexed_selection()),
            RowCopyOutcome::Declined
        );
    }

    #[test]
    fn indexed_vertical_signed_source_bounds_threshold_matches_capture() {
        let mut source = Vec::new();
        for row in 1..=17u8 {
            source.extend_from_slice(&[row * 10; 7]);
            source.push(0xa5);
        }
        let mut memory = SparseMemory::default();
        memory.insert(SOURCE, &source);
        memory.insert(DESTINATION, &[0xa5; 56]);
        let selected = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 8, 8, [-1, 0, 32_766, 7]),
            destination: pixmap(DESTINATION, 8, 8, [0, 0, 7, 7]),
            source_rect: [-1, 0, 16, 7],
            destination_rect: [0, 0, 7, 7],
            clip: [0, 0, 7, 7],
            palette: None,
        };
        assert_eq!(
            selected.execute_indexed8_vertical(&mut memory, indexed_selection()),
            RowCopyOutcome::Completed
        );
        let mut expected = Vec::new();
        for value in [20, 40, 70, 90, 110, 140, 160] {
            expected.extend_from_slice(&[value; 7]);
            expected.push(0xa5);
        }
        assert_eq!(memory.bytes(DESTINATION, 56), expected);

        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("height-32768 no-op reached source memory");
            }

            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("height-32768 no-op reached destination memory");
            }
        }
        let excluded = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 8, 8, [-1, 0, 32_767, 7]),
            destination: pixmap(DESTINATION, 8, 8, [0, 0, 7, 7]),
            source_rect: [-1, 0, 16, 7],
            destination_rect: [0, 0, 7, 7],
            clip: [0, 0, 7, 7],
            palette: None,
        };
        assert_eq!(
            excluded.execute_indexed8_vertical(&mut NoAccess, indexed_selection()),
            RowCopyOutcome::NoOp
        );
    }

    #[test]
    fn indexed_vertical_invalid_final_addresses_fail_before_memory_access() {
        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("invalid vertical geometry reached source memory");
            }

            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("invalid vertical geometry reached destination memory");
            }
        }

        let request = |source, destination| RowCopy {
            mode: 0,
            source: pixmap(source, 4, 8, [0, 0, 5, 4]),
            destination: pixmap(destination, 4, 8, [0, 0, 3, 4]),
            source_rect: [0, 0, 5, 4],
            destination_rect: [0, 0, 3, 4],
            clip: [0, 0, 3, 4],
            palette: None,
        };
        assert_eq!(
            request(u32::MAX - 3, DESTINATION)
                .execute_indexed8_vertical(&mut NoAccess, indexed_selection()),
            RowCopyOutcome::ReadOrGeometryFailure
        );
        assert_eq!(
            request(SOURCE, u32::MAX - 3)
                .execute_indexed8_vertical(&mut NoAccess, indexed_selection()),
            RowCopyOutcome::ReadOrGeometryFailure
        );
    }

    #[test]
    fn indexed_horizontal_selection_requires_all_adapter_provenance() {
        assert!(
            Indexed8HorizontalSelection::from_adapter_facts(true, true, true, true, 0).is_some()
        );
        for facts in [
            (false, true, true, true, 0),
            (true, false, true, true, 0),
            (true, true, false, true, 0),
            (true, true, true, false, 0),
            (true, true, true, true, 0x40),
        ] {
            assert!(Indexed8HorizontalSelection::from_adapter_facts(
                facts.0, facts.1, facts.2, facts.3, facts.4
            )
            .is_none());
        }
    }

    #[test]
    fn indexed_horizontal_raw_dither_flag_keeps_existing_unscaled_route() {
        let mut memory = SparseMemory::default();
        memory.insert(SOURCE, &[1, 2, 3, 0xaa]);
        memory.insert(DESTINATION, &[0xaa; 4]);
        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 4, 8, [0, 0, 1, 3]),
            destination: pixmap(DESTINATION, 4, 8, [0, 0, 1, 3]),
            source_rect: [0, 0, 1, 3],
            destination_rect: [0, 0, 1, 3],
            clip: [0, 0, 1, 3],
            palette: None,
        };
        let selection =
            Indexed8HorizontalSelection::from_adapter_facts(true, true, true, true, 0x40);
        assert_eq!(
            copy.execute_with_indexed8_horizontal(&mut memory, selection),
            RowCopyOutcome::Completed
        );
        assert_eq!(memory.bytes(DESTINATION, 4), [1, 2, 3, 0xaa]);
    }

    #[test]
    fn indexed_horizontal_global_clip_reads_only_final_physical_ranges() {
        let mut memory = SparseMemory::default();
        // The submitted row origins are -1 and 7. Clipping away destination
        // column zero makes the required physical starts 1 and 9, both valid.
        memory.insert(1, &[22, 23, 24, 25, 26]);
        memory.insert(9, &[32, 33, 34, 35, 36]);
        memory.insert(DESTINATION, &[0xaa; 8]);
        let copy = RowCopy {
            mode: 0,
            source: pixmap(1, 8, 8, [0, 8, 2, 13]),
            destination: pixmap(DESTINATION, 4, 8, [0, 20, 2, 23]),
            source_rect: [0, 6, 2, 13],
            destination_rect: [0, 20, 2, 23],
            clip: [0, 21, 2, 23],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_horizontal(&mut memory, Some(indexed_selection())),
            RowCopyOutcome::Completed
        );
        assert_eq!(
            memory.bytes(DESTINATION, 8),
            [0xaa, 24, 26, 0xaa, 0xaa, 34, 36, 0xaa]
        );
    }

    #[test]
    fn indexed_horizontal_snapshots_all_aliasing_rows_before_writes() {
        let mut memory = SparseMemory::default();
        memory.insert(
            SOURCE,
            &[
                1, 9, 2, 3, 4, 0xaa, 0xaa, 0xaa, 5, 1, 6, 0, 7, 0xaa, 0xaa, 0xaa,
            ],
        );
        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 8, 8, [0, 0, 2, 5]),
            destination: pixmap(SOURCE + 8, 3, 8, [0, 0, 2, 3]),
            source_rect: [0, 0, 2, 5],
            destination_rect: [0, 0, 2, 3],
            clip: [0, 0, 2, 3],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_horizontal(&mut memory, Some(indexed_selection())),
            RowCopyOutcome::Completed
        );
        assert_eq!(memory.bytes(SOURCE + 8, 6), [9, 2, 4, 5, 6, 7]);
    }

    #[test]
    fn indexed_horizontal_snapshots_declared_same_row_overlap() {
        let mut memory = SparseMemory::default();
        memory.insert(SOURCE, &[1, 9, 2, 3, 4, 0xaa, 0xaa, 0xaa]);
        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 8, 8, [0, 0, 1, 5]),
            destination: pixmap(SOURCE + 1, 3, 8, [0, 0, 1, 3]),
            source_rect: [0, 0, 1, 5],
            destination_rect: [0, 0, 1, 3],
            clip: [0, 0, 1, 3],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_horizontal(&mut memory, Some(indexed_selection())),
            RowCopyOutcome::Completed
        );
        assert_eq!(memory.bytes(SOURCE, 5), [1, 9, 2, 4, 4]);
    }

    #[test]
    fn indexed_horizontal_snapshots_rounded_tail_overlap() {
        let mut memory = SparseMemory::default();
        let mut backing = vec![10; 381];
        backing[0] = 100;
        backing[190..380].fill(20);
        backing[190] = 80;
        backing[380] = 90;
        memory.insert(SOURCE, &backing);
        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 190, 8, [0, 0, 2, 190]),
            destination: pixmap(SOURCE + 190, 1, 8, [0, 0, 2, 1]),
            source_rect: [0, 0, 2, 190],
            destination_rect: [0, 0, 2, 1],
            clip: [0, 0, 2, 1],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_horizontal(&mut memory, Some(indexed_selection())),
            RowCopyOutcome::Completed
        );
        assert_eq!(memory.bytes(SOURCE + 190, 2), [100, 90]);
    }

    #[test]
    fn indexed_horizontal_read_failure_is_prewrite_and_write_failure_is_partial() {
        let request = || RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 8, 8, [0, 0, 2, 5]),
            destination: pixmap(DESTINATION, 3, 8, [0, 0, 2, 3]),
            source_rect: [0, 0, 2, 5],
            destination_rect: [0, 0, 2, 3],
            clip: [0, 0, 2, 3],
            palette: None,
        };

        let mut read_failure = SparseMemory::default();
        read_failure.insert(SOURCE, &[1, 2, 3, 4, 5]);
        read_failure.insert(SOURCE + 8, &[6, 7, 8, 9, 10]);
        read_failure.insert(DESTINATION, &[0xaa; 6]);
        read_failure.fail_read = Some(SOURCE + 8);
        assert_eq!(
            request()
                .execute_with_indexed8_horizontal(&mut read_failure, Some(indexed_selection()),),
            RowCopyOutcome::ReadOrGeometryFailure
        );
        assert!(read_failure.writes.is_empty());
        assert_eq!(read_failure.bytes(DESTINATION, 6), [0xaa; 6]);

        let mut write_failure = SparseMemory::default();
        write_failure.insert(SOURCE, &[1, 2, 3, 4, 5]);
        write_failure.insert(SOURCE + 8, &[6, 7, 8, 9, 10]);
        write_failure.insert(DESTINATION, &[0xaa; 6]);
        write_failure.fail_write = Some(DESTINATION + 3);
        assert_eq!(
            request()
                .execute_with_indexed8_horizontal(&mut write_failure, Some(indexed_selection()),),
            RowCopyOutcome::WriteFailure { rows_written: 1 }
        );
        assert_eq!(write_failure.writes, [DESTINATION]);
        assert_eq!(
            write_failure.bytes(DESTINATION, 6),
            [2, 3, 5, 0xaa, 0xaa, 0xaa]
        );

        let mut first_write_failure = SparseMemory::default();
        first_write_failure.insert(SOURCE, &[1, 2, 3, 4, 5]);
        first_write_failure.insert(SOURCE + 8, &[6, 7, 8, 9, 10]);
        first_write_failure.insert(DESTINATION, &[0xaa; 6]);
        first_write_failure.fail_write = Some(DESTINATION);
        assert_eq!(
            request().execute_with_indexed8_horizontal(
                &mut first_write_failure,
                Some(indexed_selection()),
            ),
            RowCopyOutcome::WriteFailure { rows_written: 0 }
        );
        assert!(first_write_failure.writes.is_empty());
        assert_eq!(first_write_failure.bytes(DESTINATION, 6), [0xaa; 6]);
    }

    #[test]
    fn indexed_horizontal_signed_source_height_is_noop_without_memory_access() {
        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("signed-height no-op reached source memory");
            }

            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("signed-height no-op reached destination memory");
            }
        }

        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 8, 8, [-1, 0, 32_767, 7]),
            destination: pixmap(DESTINATION, 3, 8, [0, 0, 1, 3]),
            source_rect: [-1, 0, 0, 7],
            destination_rect: [0, 0, 1, 3],
            clip: [0, 0, 1, 3],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_horizontal(&mut NoAccess, Some(indexed_selection())),
            RowCopyOutcome::NoOp
        );
    }

    #[test]
    fn indexed_horizontal_wide_relative_origin_declines_without_memory_access() {
        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("declined wide origin reached source memory");
            }

            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("declined wide origin reached destination memory");
            }
        }

        let copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 8, 8, [0, i16::MIN.into(), 1, -32_760]),
            destination: pixmap(DESTINATION, 3, 8, [0, 0, 1, 3]),
            source_rect: [0, 1, 1, 6],
            destination_rect: [0, 0, 1, 3],
            clip: [0, 0, 1, 3],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_horizontal(&mut NoAccess, Some(indexed_selection())),
            RowCopyOutcome::Declined
        );

        let tall_copy = RowCopy {
            mode: 0,
            source: pixmap(SOURCE, 8, 8, [-1, i16::MIN.into(), 32_767, -32_760]),
            destination: pixmap(DESTINATION, 3, 8, [0, 0, 1, 3]),
            source_rect: [-1, 1, 0, 6],
            destination_rect: [0, 0, 1, 3],
            clip: [0, 0, 1, 3],
            palette: None,
        };
        assert_eq!(
            tall_copy.execute_with_indexed8_horizontal(&mut NoAccess, Some(indexed_selection())),
            RowCopyOutcome::Declined
        );
    }

    #[test]
    fn indexed_horizontal_invalid_final_range_fails_before_memory_access() {
        struct NoAccess;
        impl CopyBitsMemory for NoAccess {
            fn read_copy_row(&mut self, _: u32, _: &mut [u8]) -> Option<()> {
                panic!("invalid final range reached source memory");
            }

            fn write_copy_row(&mut self, _: u32, _: &[u8]) -> Option<()> {
                panic!("invalid final range reached destination memory");
            }
        }

        let copy = RowCopy {
            mode: 0,
            source: pixmap(u32::MAX - 1, 8, 8, [0, 0, 1, 5]),
            destination: pixmap(DESTINATION, 3, 8, [0, 0, 1, 3]),
            source_rect: [0, 0, 1, 5],
            destination_rect: [0, 0, 1, 3],
            clip: [0, 0, 1, 3],
            palette: None,
        };
        assert_eq!(
            copy.execute_with_indexed8_horizontal(&mut NoAccess, Some(indexed_selection())),
            RowCopyOutcome::ReadOrGeometryFailure
        );
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
