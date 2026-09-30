//! Experimental GPU transport. Uniform guest cells cost one word; only cells
//! containing retained outline detail carry their scale × scale samples.
use super::{screen_tiles_per_row, MacMemoryBus, Presentation, ScreenMark};

/// Opaque RGB transport for the experimental desktop GPU presenter.
/// A cell with bit 31 clear stores RGB in bits 0..23; with bit 31 set, bits
/// 0..30 index its row-major scale × scale tile in `detail`. This is host
/// presentation data, never guest memory or a source for guest CopyBits.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CompactPresentation {
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub cells: Vec<u32>,
    pub detail: Vec<u32>,
}

impl Clone for CompactPresentation {
    fn clone(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            scale: self.scale,
            cells: self.cells.clone(),
            detail: self.detail.clone(),
        }
    }

    /// Copy into this image's existing buffers, so a recycled snapshot takes
    /// a new frame without allocating.
    fn clone_from(&mut self, source: &Self) {
        self.width = source.width;
        self.height = source.height;
        self.scale = source.scale;
        self.cells.clone_from(&source.cells);
        self.detail.clone_from(&source.detail);
    }
}

impl CompactPresentation {
    /// Replace cells changed by a host software overlay. Unchanged cells retain
    /// their original high-resolution coverage. Invalid inputs leave this image
    /// intact, matching the opaque transport's existing export requirements.
    pub fn apply_overlay(&mut self, guest: &[u32], overlays: &[u32]) -> bool {
        let count = self.width as usize * self.height as usize;
        if self.cells.len() != count
            || guest.len() != count
            || overlays.len() != count
            || overlays.iter().any(|pixel| pixel >> 24 != 255)
        {
            return false;
        }
        for (cell, (&before, &after)) in self.cells.iter_mut().zip(guest.iter().zip(overlays)) {
            if before != after {
                *cell = after & 0xffffff;
            }
        }
        true
    }

    /// Resolve an owned retained image at a host drawable size without guest
    /// memory. Uses the same coverage footprints and rounding as the live
    /// retained-image renderer. Invalid packets leave the previous output intact.
    pub fn render_argb_resized(&self, size: (u32, u32), output: &mut Vec<u32>) -> bool {
        if self.width == 0
            || self.height == 0
            || self.width > u16::MAX.into()
            || self.height > u16::MAX.into()
            || !(1..=4).contains(&self.scale)
            || size.0 == 0
            || size.1 == 0
        {
            return false;
        }
        let Some(count) = (self.width as usize).checked_mul(self.height as usize) else {
            return false;
        };
        let tile = (self.scale * self.scale) as usize;
        if self.cells.len() != count
            || self.cells.iter().any(|&cell| {
                cell >> 31 != 0
                    && ((cell & 0x7fffffff) as usize)
                        .checked_add(tile)
                        .is_none_or(|end| end > self.detail.len())
            })
        {
            return false;
        }
        let Some(count) = (size.0 as usize).checked_mul(size.1 as usize) else {
            return false;
        };
        if output
            .try_reserve(count.saturating_sub(output.len()))
            .is_err()
        {
            return false;
        }
        let horizontal = super::resample::axis(self.width, self.scale, size.0);
        let vertical = super::resample::axis(self.height, self.scale, size.1);
        let source_width = self.width * self.scale;
        let source_height = self.height * self.scale;
        let total = u64::from(if source_width > size.0 {
            source_width
        } else {
            1
        }) * u64::from(if source_height > size.1 {
            source_height
        } else {
            1
        });
        let reciprocal = u64::MAX / total;
        output.clear();
        for rows in &vertical {
            for columns in &horizontal {
                let mut sum = [0u64; 3];
                let mut accumulate = |rgb: u32, weight: u64| {
                    for (channel, value) in sum.iter_mut().enumerate() {
                        *value += u64::from((rgb >> (channel * 8)) & 255) * weight;
                    }
                };
                for row in rows {
                    for column in columns {
                        let cell = self.cells[row.cell * self.width as usize + column.cell];
                        if cell >> 31 == 0 {
                            accumulate(cell, row.weight * column.weight);
                        } else {
                            let offset = (cell & 0x7fffffff) as usize;
                            for &(sy, wy) in &row.samples {
                                for &(sx, wx) in &column.samples {
                                    accumulate(
                                        self.detail[offset + sy * self.scale as usize + sx],
                                        wy * wx,
                                    );
                                }
                            }
                        }
                    }
                }
                output.push(
                    sum.iter()
                        .enumerate()
                        .fold(0xff000000, |pixel, (channel, value)| {
                            pixel
                                | ((crate::display::coverage_quotient(
                                    value + total / 2,
                                    total,
                                    reciprocal,
                                ) as u32)
                                    << (channel * 8))
                        }),
                );
            }
        }
        true
    }
}

/// Owns a reusable compact image and its visible-surface stamp.
/// Obtain mutable access through [`Self::frame_mut`] when replacing pixels or
/// adding software overlays; that invalidates reuse of the retained image.
#[derive(Default)]
pub struct CompactPresentationCache {
    frame: CompactPresentation,
    source: Option<super::VisibleImageStamp>,
    rows: Option<ExportedRows>,
}

/// What the next export needs to rebuild only rows changed since `frame` was
/// exported: the screen history point and palette it was built from, and
/// which of its rows own detail tiles. Rows holding detail are always rebuilt
/// because detail offsets are assigned in row order.
struct ExportedRows {
    mark: ScreenMark,
    palette: [[u8; 3]; 256],
    detail_rows: Vec<bool>,
}

impl CompactPresentationCache {
    /// The most recently prepared image, ready for upload or resubmission.
    pub fn frame(&self) -> &CompactPresentation {
        &self.frame
    }

    /// Invalidate retained-image reuse before the caller changes the output.
    pub fn frame_mut(&mut self) -> &mut CompactPresentation {
        self.source = None;
        self.rows = None;
        &mut self.frame
    }

    /// Prepare the current opaque retained image without software overlays.
    /// Reuse it if no visible writes, coverage or palette changes occurred.
    /// Offscreen-only drawing does not invalidate this image. Returns false
    /// for an absent or mismatched surface, leaving the previous pixels intact.
    pub fn prepare(&mut self, bus: &MacMemoryBus, size: (u32, u32)) -> bool {
        self.prepare_changed(bus, size).is_some()
    }

    /// Like `prepare`, but distinguish a fresh export from retained reuse.
    /// Owners can share an immutable copy until visible content changes, without
    /// comparing or copying all cells for every guest tick. None means invalid.
    pub fn prepare_changed(&mut self, bus: &MacMemoryBus, size: (u32, u32)) -> Option<bool> {
        let Some(p) = bus.presentation.as_ref() else {
            self.source = None;
            self.rows = None;
            return None;
        };
        if (p.logical_width(), p.height) != size {
            self.source = None;
            self.rows = None;
            return None;
        }
        if self
            .source
            .as_ref()
            .is_some_and(|source| source.matches(&p.visible_image))
        {
            return Some(false);
        }
        self.source = None;
        // Rows unchanged since the previous export keep their cells, provided
        // that export came from this surface with the same geometry and colors.
        let previous = self.rows.take().filter(|rows| {
            rows.palette == p.palette
                && (self.frame.width, self.frame.height, self.frame.scale)
                    == (p.logical_width(), p.height, p.scale)
        });
        let (since, mut detail_rows) = match previous {
            Some(rows) => (Some(rows.mark), rows.detail_rows),
            None => (None, Vec::new()),
        };
        if !p.export_compact_rows(
            std::iter::repeat(None),
            &mut self.frame,
            since,
            &mut detail_rows,
        ) {
            return None;
        }
        self.source = Some(p.visible_image.clone());
        self.rows = Some(ExportedRows {
            mark: p.screen_mark(),
            palette: p.palette,
            detail_rows,
        });
        Some(true)
    }
}

impl Presentation {
    fn compact_sample(&self, x: usize, y: usize, sx: usize, sy: usize) -> u32 {
        let lanes = self.bytes_per_pixel() as usize;
        let mut rgb = [0u8; 3];
        for lane in 0..lanes {
            let bx = x * lanes + lane;
            let cell = y * self.width as usize + bx;
            let color = if self.text_cells[cell] {
                let scale = self.scale as usize;
                self.samples.get(cell).rgb[sy * scale + sx]
            } else {
                self.palette_at(bx as u32)[self.guest_values[cell] as u8 as usize]
            };
            for channel in 0..3 {
                rgb[channel] = rgb[channel].saturating_add(color[channel]);
            }
        }
        (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])
    }
}

impl MacMemoryBus {
    /// Export retained opaque coverage without expanding ordinary pixels.
    /// Returns false when the inputs cannot be represented by this transport;
    /// callers must then use their existing software presentation path.
    pub fn compact_presentation(
        &self,
        guest: &[u32],
        overlays: &[u32],
        output: &mut CompactPresentation,
    ) -> bool {
        let Some(p) = self.presentation.as_ref() else {
            return false;
        };
        let width = p.logical_width();
        let count = width as usize * p.height as usize;
        if guest.len() != count
            || overlays.len() != count
            || overlays.iter().any(|pixel| pixel >> 24 != 255)
        {
            return false;
        }
        p.export_compact(
            guest
                .iter()
                .zip(overlays)
                .map(|(&before, &after)| (before != after).then_some(after & 0xffffff)),
            output,
        )
    }

    /// Export the opaque retained image when the frontend has no software
    /// overlays. No logical ARGB image or overlay-reference copy is needed.
    /// Returns false without modifying `output` if the surface is absent or
    /// does not match the current logical screen size.
    pub fn compact_presentation_without_overlays(
        &self,
        size: (u32, u32),
        output: &mut CompactPresentation,
    ) -> bool {
        let Some(p) = self.presentation.as_ref() else {
            return false;
        };
        if (p.logical_width(), p.height) != size {
            return false;
        }
        p.export_compact(std::iter::repeat(None), output)
    }
}

impl Presentation {
    /// Whether no on-screen cell of row `y` changed after `mark`.
    fn screen_row_unchanged_since(&self, mark: ScreenMark, y: usize) -> bool {
        let per_row = screen_tiles_per_row(self.width);
        mark.identity == self.identity
            && self.tile_epochs[y * per_row..(y + 1) * per_row]
                .iter()
                .all(|&epoch| epoch <= mark.epoch)
    }

    fn export_compact(
        &self,
        overlays: impl Iterator<Item = Option<u32>>,
        output: &mut CompactPresentation,
    ) -> bool {
        self.export_compact_rows(overlays, output, None, &mut Vec::new())
    }

    // Monomorphized for either overlay differences or a constant absence of
    // overlays, so the latter needs no input images, alpha scan or comparisons.
    //
    // With `since`, `output` must hold this surface's export at that mark with
    // the current palette, and `detail_rows` its per-row detail flags: rows
    // without detail and without later changes keep their cells. Overlays must
    // then be absent. `detail_rows` is updated for the new export.
    fn export_compact_rows(
        &self,
        mut overlays: impl Iterator<Item = Option<u32>>,
        output: &mut CompactPresentation,
        since: Option<ScreenMark>,
        detail_rows: &mut Vec<bool>,
    ) -> bool {
        let p = self;
        let width = p.logical_width() as usize;
        let height = p.height as usize;
        let count = width * height;
        if count
            .checked_mul((p.scale * p.scale) as usize)
            .is_none_or(|n| n >= 0x80000000)
        {
            return false;
        }
        let since = since.filter(|_| {
            output.cells.len() == count && detail_rows.len() == height
        });
        if since.is_none() {
            detail_rows.clear();
            detail_rows.resize(height, true);
        }
        output.width = width as u32;
        output.height = p.height;
        output.scale = p.scale;
        // Reuse the initialized cell slice across frames. Every rebuilt cell
        // below is overwritten; retaining its length avoids per-pixel
        // Vec::push capacity checks and length updates in the export loop.
        output.cells.resize(count, 0);
        output.detail.clear();
        let scale = p.scale as usize;
        let lanes = p.bytes_per_pixel() as usize;
        // SC2K's indexed framebuffer uses the CLUT; direct-color lanes each
        // contribute one channel slice. Resolve the tables once per export.
        let pack = |rgb: &[u8; 3]| {
            (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])
        };
        let tables: [[u32; 256]; 4] = if p.depth == 8 {
            [p.palette.map(|rgb| pack(&rgb)), [0; 256], [0; 256], [0; 256]]
        } else {
            p.direct_palettes.map(|palette| palette.map(|rgb| pack(&rgb)))
        };
        for (y, has_detail) in detail_rows.iter_mut().enumerate() {
            if since.is_some_and(|mark| !*has_detail && p.screen_row_unchanged_since(mark, y)) {
                overlays.by_ref().take(width).for_each(drop);
                continue;
            }
            let first_detail = output.detail.len();
            let cells = &mut output.cells[y * width..(y + 1) * width];
            let row = y * p.width as usize;
            if lanes == 1 {
                let values = &p.guest_values[row..row + width];
                let text = &p.text_cells[row..row + width];
                for (x, ((destination, (&text, &value)), overlay)) in cells
                    .iter_mut()
                    .zip(text.iter().zip(values))
                    .zip(overlays.by_ref())
                    .enumerate()
                {
                    if let Some(rgb) = overlay {
                        *destination = rgb;
                    } else if !text {
                        *destination = tables[0][value as u8 as usize];
                    } else {
                        *destination = 0x80000000 | output.detail.len() as u32;
                        output.detail.extend(
                            p.samples.get(row + x).rgb[..scale * scale].iter().map(pack),
                        );
                    }
                }
            } else {
                for (x, (destination, overlay)) in
                    cells.iter_mut().zip(overlays.by_ref()).enumerate()
                {
                    let cell = row + x * lanes;
                    if let Some(rgb) = overlay {
                        *destination = rgb;
                    } else if p.text_cells[cell..cell + lanes].iter().any(|&v| v) {
                        *destination = 0x80000000 | output.detail.len() as u32;
                        for sy in 0..scale {
                            for sx in 0..scale {
                                output.detail.push(p.compact_sample(x, y, sx, sy));
                            }
                        }
                    } else {
                        *destination = p.guest_values[cell..cell + lanes]
                            .iter()
                            .zip(&tables)
                            .fold(0, |rgb, (&value, table)| {
                                saturating_rgb_add(rgb, table[value as u8 as usize])
                            });
                    }
                }
            }
            *has_detail = output.detail.len() != first_detail;
        }
        true
    }
}

/// Channel-wise saturating sum of two packed RGB words, as `compact_sample`
/// combines direct-color lanes.
#[inline]
fn saturating_rgb_add(a: u32, b: u32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |rgb, shift| {
        let sum = ((a >> shift) & 255) + ((b >> shift) & 255);
        rgb | (sum.min(255) << shift)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryBus;

    fn check_cached(bus: &MacMemoryBus, cache: &mut CompactPresentationCache, size: (u32, u32)) {
        let mut fresh = CompactPresentation::default();
        assert!(bus.compact_presentation_without_overlays(size, &mut fresh));
        assert!(cache.prepare(bus, size));
        assert_eq!((cache.frame().width, cache.frame().height), size);
        assert_eq!(cache.frame().scale, fresh.scale);
        assert_eq!(cache.frame().cells, fresh.cells);
        assert_eq!(cache.frame().detail, fresh.detail);
    }

    #[test]
    fn changed_export_distinguishes_reuse_from_visible_changes() {
        use crate::memory::MemoryBus;
        let mut bus = super::super::tests::bus();
        bus.enable_outline_presentation((0x1000, 8, 8, 8, 8), [[0, 0, 0]; 256], 2);
        let mut cache = CompactPresentationCache::default();
        assert_eq!(cache.prepare_changed(&bus, (8, 8)), Some(true));
        assert_eq!(cache.prepare_changed(&bus, (8, 8)), Some(false));
        bus.write_byte(0x1000, 42);
        assert_eq!(cache.prepare_changed(&bus, (8, 8)), Some(true));
        assert_eq!(cache.prepare_changed(&bus, (8, 8)), Some(false));
        assert_eq!(cache.prepare_changed(&bus, (4, 8)), None);
        assert_eq!(cache.prepare_changed(&bus, (8, 8)), Some(true));
    }

    #[test]
    fn owned_compact_resizing_matches_live_retained_pixels_and_overlays() {
        for depth in [8u16, 16, 32] {
            for scale in 2..=4 {
                let mut bus = super::super::tests::bus();
                bus.enable_outline_presentation(
                    (0x1000, u32::from(depth), 8, 8, depth),
                    std::array::from_fn(|i| [i as u8, (i * 7) as u8, (255 - i) as u8]),
                    scale,
                );
                super::super::tests::paint_detail(&mut bus, 0x1000);
                let guest = vec![0xff123456; 64];
                for overlay in [false, true] {
                    let mut overlays = guest.clone();
                    if overlay {
                        overlays[0] = 0xffa71d6b;
                        overlays[27] = 0xff234567;
                    }
                    let mut compact = CompactPresentation::default();
                    assert!(bus.compact_presentation(&guest, &overlays, &mut compact));
                    let mut owned_overlay = CompactPresentation::default();
                    assert!(bus.compact_presentation_without_overlays((8, 8), &mut owned_overlay));
                    assert!(owned_overlay.apply_overlay(&guest, &overlays));
                    for size in [
                        (8, 8),
                        (16, 16),
                        (24, 24),
                        (32, 32),
                        (3, 5),
                        (11, 13),
                        (41, 37),
                    ] {
                        let mut expected = Vec::new();
                        assert_eq!(
                            bus.presented_argb_resized(&guest, &overlays, size, &mut expected),
                            Some(size)
                        );
                        let mut actual = Vec::new();
                        assert!(compact.render_argb_resized(size, &mut actual));
                        let mut owned_actual = Vec::new();
                        assert!(owned_overlay.render_argb_resized(size, &mut owned_actual));
                        assert_eq!(owned_actual, actual);
                        if size.0 == size.1 && size.0 % 8 == 0 {
                            let mut scaled = Vec::new();
                            assert_eq!(
                                bus.presented_argb_scaled(
                                    &guest,
                                    &overlays,
                                    size.0 / 8,
                                    &mut scaled
                                ),
                                Some(size)
                            );
                            assert_eq!(actual, scaled, "integer-scale native presentation");
                        }
                        assert_eq!(
                            actual, expected,
                            "depth={depth} scale={scale} overlay={overlay} size={size:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn malformed_compact_packets_do_not_partially_replace_output() {
        let mut frame = CompactPresentation {
            width: 2,
            height: 1,
            scale: 2,
            cells: vec![0xabcdef, 0x80000000],
            detail: vec![0; 3],
        };
        let mut output = vec![0xff123456];
        assert!(!frame.render_argb_resized((4, 2), &mut output));
        assert_eq!(output, [0xff123456]);
        frame.detail.push(0);
        assert!(frame.render_argb_resized((4, 2), &mut output));
        assert_eq!(
            output,
            [
                0xffabcdef, 0xffabcdef, 0xff000000, 0xff000000, 0xffabcdef, 0xffabcdef, 0xff000000,
                0xff000000
            ]
        );
    }

    #[test]
    fn cached_export_tracks_visible_writes_and_retained_coverage() {
        for depth in [8u16, 16, 32] {
            for scale in 2..=4 {
                let mut bus = MacMemoryBus::new(1024 * 1024);
                bus.enable_outline_presentation(
                    (0x1000, 8 * u32::from(depth / 8), 8, 8, depth),
                    std::array::from_fn(|i| [i as u8; 3]),
                    scale,
                );
                let mut cache = CompactPresentationCache::default();
                let mut second = CompactPresentationCache::default();
                check_cached(&bus, &mut cache, (8, 8));
                check_cached(&bus, &mut second, (8, 8));
                // Equal plain bytes preserve the visible stamp.
                let initial = cache.source.clone().unwrap();
                bus.write_byte(0x1000, bus.read_byte(0x1000));
                assert!(initial.matches(&bus.presentation.as_ref().unwrap().visible_image));
                check_cached(&bus, &mut cache, (8, 8));
                bus.write_byte(0x1000, 77);
                check_cached(&bus, &mut cache, (8, 8));
                check_cached(&bus, &mut second, (8, 8));
                // Coverage changes must invalidate even without a RAM write.
                let value = bus.read_byte(0x1000);
                bus.presentation.as_mut().unwrap().glyph = Some((
                    super::super::OutlineGlyph {
                        pixels: vec![64, 255, 0, 128].into(),
                        width: 2,
                        height: 2,
                        left: 0,
                        top: 0,
                    },
                    0,
                    0,
                ));
                bus.outline_glyph_pixel(0x1000, 0, 0, 0);
                bus.end_outline_glyph();
                assert_eq!(bus.read_byte(0x1000), value);
                check_cached(&bus, &mut cache, (8, 8));
                check_cached(&bus, &mut second, (8, 8));
                let before = cache.source.clone().unwrap();
                let saved = bus.save_pixel_bytes(0x1000, 2);
                bus.write_byte(0x1000, value);
                assert!(!before.matches(&bus.presentation.as_ref().unwrap().visible_image));
                check_cached(&bus, &mut cache, (8, 8));
                bus.restore_saved_pixels(0x1000, &saved, 0, 2);
                check_cached(&bus, &mut cache, (8, 8));
                check_cached(&bus, &mut second, (8, 8));
            }
        }
    }

    #[test]
    fn row_reusing_export_matches_fresh_exports_through_mixed_updates() {
        for depth in [8u16, 16, 32] {
            for scale in [2, 4] {
                let (width, height) = (20u32, 12u32);
                let lanes = u32::from(depth / 8);
                let row_bytes = width * lanes + 4;
                let mut bus = MacMemoryBus::new(1024 * 1024);
                bus.enable_outline_presentation(
                    (0x1000, row_bytes, width as u16, height as u16, depth),
                    std::array::from_fn(|i| [i as u8, (i * 3) as u8, (255 - i) as u8]),
                    scale,
                );
                let mut cache = CompactPresentationCache::default();
                check_cached(&bus, &mut cache, (width, height));
                let mut seed = 0x9e37_79b9u32;
                let mut next = || {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed
                };
                for step in 0..300 {
                    let y = next() % height;
                    let x = next() % (width * lanes);
                    let address = 0x1000 + y * row_bytes + x;
                    match next() % 5 {
                        0 => super::super::tests::paint_detail(&mut bus, address),
                        1 => bus.write_byte(address, bus.read_byte(address)),
                        // Row padding lies outside the visible surface.
                        2 => bus.write_byte(0x1000 + y * row_bytes + width * lanes, next() as u8),
                        _ => bus.write_byte(address, next() as u8),
                    }
                    if step % 3 == 0 {
                        let before = cache.rows.as_ref().map(|rows| rows.mark);
                        check_cached(&bus, &mut cache, (width, height));
                        assert!(before.is_none() || cache.rows.is_some());
                    }
                }
                // Clearing every text cell must also clear its rows' flags.
                bus.fill_bytes(0x1000, row_bytes * height, 7);
                check_cached(&bus, &mut cache, (width, height));
                assert!(cache.frame().detail.is_empty());
                assert!(cache.rows.as_ref().unwrap().detail_rows.iter().all(|&d| !d));
            }
        }
    }

    #[test]
    fn cached_export_ignores_offscreen_changes_until_copied_visible() {
        let mut bus = super::super::tests::bus();
        let mut cache = CompactPresentationCache::default();
        check_cached(&bus, &mut cache, (8, 8));
        let source = cache.source.clone().unwrap();
        let global = bus.presentation_epoch();
        super::super::tests::paint_detail(&mut bus, 0x2000);
        assert_ne!(bus.presentation_epoch(), global);
        assert!(source.matches(&bus.presentation.as_ref().unwrap().visible_image));
        check_cached(&bus, &mut cache, (8, 8));
        let saved = bus.save_pixel_bytes(0x2000, 2);
        bus.presentation.write_bytes(0x2000, &[255; 2]);
        assert!(source.matches(&bus.presentation.as_ref().unwrap().visible_image));
        bus.restore_saved_pixels(0x2000, &saved, 0, 2);
        assert!(source.matches(&bus.presentation.as_ref().unwrap().visible_image));
        bus.restore_saved_pixels(0x1000, &saved, 0, 2);
        assert!(!source.matches(&bus.presentation.as_ref().unwrap().visible_image));
        check_cached(&bus, &mut cache, (8, 8));
    }

    #[test]
    fn cached_export_invalidates_palette_and_replacement_surfaces() {
        let mut cache = CompactPresentationCache::default();
        for depth in [8u16, 16, 32] {
            let mut bus = MacMemoryBus::new(1024 * 1024);
            for (width, height, scale) in [(8u16, 8u16, 4), (4, 16, 4), (4, 16, 2)] {
                let screen = (
                    0x1000,
                    u32::from(width) * u32::from(depth / 8),
                    width,
                    height,
                    depth,
                );
                let palette = std::array::from_fn(|i| [i as u8; 3]);
                let size = (u32::from(width), u32::from(height));
                bus.enable_outline_presentation(screen, palette, scale);
                check_cached(&bus, &mut cache, size);
                let source = cache.source.clone().unwrap();
                // Replacing a surface with identical geometry/revision still
                // needs a distinct identity, including after the old bus dies.
                bus.enable_outline_presentation(screen, palette, scale);
                assert!(!source.matches(&bus.presentation.as_ref().unwrap().visible_image));
                check_cached(&bus, &mut cache, size);
                let mut changed_palette = palette;
                changed_palette[0] = [230, 17, 53];
                super::super::tests::paint_detail(&mut bus, 0x1000);
                bus.prepare_outline_presentation(screen, changed_palette);
                check_cached(&bus, &mut cache, size);
                assert!(!cache.prepare(&bus, (size.1 + 1, size.0)));
                check_cached(&bus, &mut cache, size);
                bus.presentation.set(None);
                assert!(!cache.prepare(&bus, size));
            }
        }
    }

    #[test]
    fn mutable_compact_output_invalidates_reuse_for_overlays() {
        let bus = super::super::tests::bus();
        let mut cache = CompactPresentationCache::default();
        check_cached(&bus, &mut cache, (8, 8));
        let logical = vec![0xff000000; 64];
        let mut overlay = logical.clone();
        overlay[0] = 0xffabcdef;
        assert!(bus.compact_presentation(&logical, &overlay, cache.frame_mut()));
        assert_eq!(cache.frame().cells[0], 0xabcdef);
        check_cached(&bus, &mut cache, (8, 8));
        *cache.frame_mut() = CompactPresentation::default();
        check_cached(&bus, &mut cache, (8, 8));
    }

    #[test]
    fn cached_export_cannot_alias_a_wrapped_revision() {
        let mut bus = super::super::tests::bus();
        let mut cache = CompactPresentationCache::default();
        bus.presentation.as_mut().unwrap().visible_image.revision = 0;
        check_cached(&bus, &mut cache, (8, 8));
        let source = cache.source.clone().unwrap();
        bus.presentation.as_mut().unwrap().revision = u64::MAX;
        bus.write_byte(0x1000, 77);
        let current = bus.presentation.as_ref().unwrap().visible_image.clone();
        assert_eq!(source.revision, current.revision);
        assert!(!source.matches(&current));
        check_cached(&bus, &mut cache, (8, 8));
    }

    #[test]
    fn export_without_overlays_matches_validated_overlay_path() {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let mut before = CompactPresentation::default();
        let mut after = CompactPresentation::default();
        after.cells.push(123);
        assert!(!bus.compact_presentation_without_overlays((8, 8), &mut after));
        assert_eq!(after.cells, [123]);
        for depth in [8u16, 16, 32] {
            for scale in 2..=4 {
                for (width, height) in [(8, 8), (3, 5), (11, 9)] {
                    let lanes = u32::from(depth / 8);
                    bus.enable_outline_presentation(
                        (0x1000, width * lanes, width as u16, height as u16, depth),
                        std::array::from_fn(|i| [i as u8, (i * 7) as u8, (255 - i) as u8]),
                        scale,
                    );
                    // The old API uses these only to identify overlay changes;
                    // equal images never supply RGB values to the transport.
                    let logical = vec![0xff123456; (width * height) as usize];
                    for step in 0..3 {
                        match step {
                            1 => super::super::tests::paint_detail(&mut bus, 0x1000),
                            2 => bus.write_byte(0x1000, 27),
                            _ => (),
                        }
                        assert!(bus.compact_presentation(&logical, &logical, &mut before));
                        assert!(
                            bus.compact_presentation_without_overlays((width, height), &mut after)
                        );
                        assert_eq!(
                            (before.width, before.height, before.scale),
                            (after.width, after.height, after.scale)
                        );
                        assert_eq!(
                            before.cells, after.cells,
                            "depth={depth} scale={scale} step={step}"
                        );
                        assert_eq!(before.detail, after.detail);
                        // Reject stale geometry, including the same pixel count
                        // in a different shape, without replacing a pending image.
                        assert!(!bus.compact_presentation_without_overlays(
                            (width * height, 1),
                            &mut after
                        ));
                        assert_eq!(before.cells, after.cells);
                        assert_eq!(before.detail, after.detail);
                    }
                }
            }
        }
    }

    #[test]
    fn switching_software_overlays_preserves_retained_image() {
        use crate::display::{render_cursor_argb, render_debug_overlay_argb, CursorImage};

        let (width, height) = (128u32, 48u32);
        for depth in [8u16, 16, 32] {
            let mut bus = MacMemoryBus::new(1024 * 1024);
            bus.enable_outline_presentation(
                (
                    0x1000,
                    width * u32::from(depth / 8),
                    width as u16,
                    height as u16,
                    depth,
                ),
                std::array::from_fn(|i| [i as u8; 3]),
                2,
            );
            super::super::tests::paint_detail(&mut bus, 0x1000);
            let logical = vec![0xff123456; (width * height) as usize];
            let cursor = CursorImage::mono([0x80; 32], [0xff; 32], 0, 0);
            let mut compact = CompactPresentation::default();
            for (cursor_visible, debug_visible) in [
                (false, false),
                (true, false),
                (false, false),
                (false, true),
                (true, true),
                (false, false),
            ] {
                let mut overlay = logical.clone();
                if cursor_visible {
                    render_cursor_argb(&mut overlay, width, height, &cursor, (-3, -2));
                }
                if debug_visible {
                    render_debug_overlay_argb(&mut overlay, width, height, &["FPS 60".into()]);
                }
                if cursor_visible || debug_visible {
                    assert_ne!(overlay, logical, "test overlay must affect pixels");
                    assert!(bus.compact_presentation(&logical, &overlay, &mut compact));
                } else {
                    assert!(
                        bus.compact_presentation_without_overlays((width, height), &mut compact)
                    );
                }
                let (w, h, expected) = bus.presented_argb(&logical, &overlay).unwrap();
                let mut actual = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let cell = compact.cells[((y / 2) * width + x / 2) as usize];
                        let rgb = if cell >> 31 == 0 {
                            cell
                        } else {
                            compact.detail
                                [(cell & 0x7fffffff) as usize + ((y % 2) * 2 + x % 2) as usize]
                        };
                        actual.push(0xff000000 | rgb);
                    }
                }
                assert_eq!(
                    actual, expected,
                    "depth={depth} cursor={cursor_visible} debug={debug_visible}"
                );
            }
        }
    }

    #[test]
    fn reused_compact_cells_are_overwritten_after_size_and_depth_changes() {
        let mut bus = super::super::tests::bus();
        let mut compact = CompactPresentation::default();
        for (width, height, depth, ink) in [
            (8, 8, 8u16, true),
            (4, 3, 8, false),
            (12, 9, 8, true),
            (5, 4, 16, false),
            (9, 6, 32, true),
        ] {
            let lanes = u32::from(depth / 8);
            bus.enable_outline_presentation(
                (0x1000, width * lanes, width as u16, height as u16, depth),
                std::array::from_fn(|i| [i as u8; 3]),
                2,
            );
            if ink {
                super::super::tests::paint_detail(&mut bus, 0x1000);
            }
            let guest = vec![0xff123456; (width * height) as usize];
            let mut overlay = guest.clone();
            *overlay.last_mut().unwrap() = 0xffabcdef;
            assert!(bus.compact_presentation(&guest, &overlay, &mut compact));
            assert_eq!(compact.cells.len(), guest.len());
            if !ink {
                assert!(compact.detail.is_empty());
            }
            let (w, h, expected) = bus.presented_argb(&guest, &overlay).unwrap();
            let actual: Vec<_> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let cell = compact.cells[((y / 2) * width + x / 2) as usize];
                    0xff000000
                        | if cell >> 31 == 0 {
                            cell
                        } else {
                            compact.detail
                                [(cell & 0x7fffffff) as usize + ((y % 2) * 2 + x % 2) as usize]
                        }
                })
                .collect();
            assert_eq!(actual, expected, "{width}x{height} depth={depth}");
        }
    }

    #[test]
    fn compact_transport_matches_full_retained_image_after_updates() {
        for depth in [8u16, 16, 32] {
            for scale in 2..=4 {
                let mut bus = super::super::tests::bus();
                let palette = std::array::from_fn(|i| [i as u8, (i * 7) as u8, (255 - i) as u8]);
                bus.enable_outline_presentation(
                    (0x1000, u32::from(depth), 8, 8, depth),
                    palette,
                    scale,
                );
                let guest = vec![0xff123456; 64];
                let mut overlays = guest.clone();
                let mut compact = CompactPresentation::default();
                for step in 0..4 {
                    match step {
                        0 => super::super::tests::paint_detail(&mut bus, 0x1000),
                        1 => overlays[0] = 0xffa71d6b,
                        2 => overlays[0] = guest[0],
                        _ => bus.write_byte(0x1000, 27),
                    }
                    assert!(bus.compact_presentation(&guest, &overlays, &mut compact));
                    let (w, h, expected) = bus.presented_argb(&guest, &overlays).unwrap();
                    let mut actual = Vec::new();
                    for y in 0..h {
                        for x in 0..w {
                            let cell = compact.cells[((y / scale) * 8 + x / scale) as usize];
                            actual.push(
                                0xff000000
                                    | if cell >> 31 == 0 {
                                        cell
                                    } else {
                                        compact.detail[(cell & 0x7fffffff) as usize
                                            + ((y % scale) * scale + x % scale) as usize]
                                    },
                            );
                        }
                    }
                    assert_eq!(actual, expected, "depth={depth} scale={scale} step={step}");
                }
                overlays[0] = 0x7f123456;
                assert!(!bus.compact_presentation(&guest, &overlays, &mut compact));
            }
        }
    }
}
