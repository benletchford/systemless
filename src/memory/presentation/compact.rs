//! Experimental GPU transport. Uniform guest cells cost one word; only cells
//! containing retained outline detail carry their scale × scale samples.
use super::{MacMemoryBus, Presentation};

/// Opaque RGB transport for the experimental desktop GPU presenter.
/// A cell with bit 31 clear stores RGB in bits 0..23; with bit 31 set, bits
/// 0..30 index its row-major scale × scale tile in `detail`. This is host
/// presentation data, never guest memory or a source for guest CopyBits.
#[derive(Default)]
pub struct CompactPresentation {
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub cells: Vec<u32>,
    pub detail: Vec<u32>,
}

/// Owns a reusable compact image and its visible-surface stamp.
/// Obtain mutable access through [`Self::frame_mut`] when replacing pixels or
/// adding software overlays; that invalidates reuse of the retained image.
#[derive(Default)]
pub struct CompactPresentationCache {
    frame: CompactPresentation,
    source: Option<super::VisibleImageStamp>,
}

impl CompactPresentationCache {
    /// The most recently prepared image, ready for upload or resubmission.
    pub fn frame(&self) -> &CompactPresentation {
        &self.frame
    }

    /// Invalidate retained-image reuse before the caller changes the output.
    pub fn frame_mut(&mut self) -> &mut CompactPresentation {
        self.source = None;
        &mut self.frame
    }

    /// Prepare the current opaque retained image without software overlays.
    /// Reuse it if no visible writes, coverage or palette changes occurred.
    /// Offscreen-only drawing does not invalidate this image. Returns false
    /// for an absent or mismatched surface, leaving the previous pixels intact.
    pub fn prepare(&mut self, bus: &MacMemoryBus, size: (u32, u32)) -> bool {
        let Some(p) = bus.presentation.as_ref() else {
            self.source = None;
            return false;
        };
        if (p.logical_width(), p.height) != size {
            self.source = None;
            return false;
        }
        if self
            .source
            .as_ref()
            .is_some_and(|source| source.matches(&p.visible_image))
        {
            return true;
        }
        self.source = None;
        if !p.export_compact(std::iter::repeat(None), &mut self.frame) {
            return false;
        }
        self.source = Some(p.visible_image.clone());
        true
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
    // Monomorphized for either overlay differences or a constant absence of
    // overlays, so the latter needs no input images, alpha scan or comparisons.
    fn export_compact(
        &self,
        overlays: impl Iterator<Item = Option<u32>>,
        output: &mut CompactPresentation,
    ) -> bool {
        let p = self;
        let width = p.logical_width();
        let count = width as usize * p.height as usize;
        if count
            .checked_mul((p.scale * p.scale) as usize)
            .is_none_or(|n| n >= 0x80000000)
        {
            return false;
        }
        output.width = width;
        output.height = p.height;
        output.scale = p.scale;
        // Reuse the initialized cell slice across frames. Every cell below is
        // overwritten; retaining its length avoids per-pixel Vec::push capacity
        // checks and length updates in the full-frame export loop.
        output.cells.resize(count, 0);
        output.detail.clear();
        if p.depth == 8 {
            // SC2K's indexed framebuffer is the common case. Resolve its
            // palette once, and avoid per-cell direct-color lane iteration.
            let palette = p.palette.map(|rgb| {
                (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])
            });
            let scale = p.scale as usize;
            for (cell, ((destination, (&text, &value)), overlay)) in output
                .cells
                .iter_mut()
                .zip(p.text_cells.iter().zip(&p.guest_values))
                .zip(overlays)
                .enumerate()
            {
                if let Some(rgb) = overlay {
                    *destination = rgb;
                } else if !text {
                    *destination = palette[value as u8 as usize];
                } else {
                    *destination = 0x80000000 | output.detail.len() as u32;
                    let samples = p.samples.get(cell);
                    for sy in 0..scale {
                        let start = sy * scale;
                        for rgb in &samples.rgb[start..start + scale] {
                            output.detail.push(
                                (u32::from(rgb[0]) << 16)
                                    | (u32::from(rgb[1]) << 8)
                                    | u32::from(rgb[2]),
                            );
                        }
                    }
                }
            }
            return true;
        }
        let lanes = p.bytes_per_pixel() as usize;
        let mut overlays = overlays;
        for y in 0..p.height as usize {
            for x in 0..width as usize {
                let logical = y * width as usize + x;
                let cell = y * p.width as usize + x * lanes;
                if let Some(rgb) = overlays.next().flatten() {
                    output.cells[logical] = rgb;
                } else if p.text_cells[cell..cell + lanes].iter().any(|&v| v) {
                    output.cells[logical] = 0x80000000 | output.detail.len() as u32;
                    for sy in 0..p.scale as usize {
                        for sx in 0..p.scale as usize {
                            output.detail.push(p.compact_sample(x, y, sx, sy));
                        }
                    }
                } else {
                    output.cells[logical] = p.compact_sample(x, y, 0, 0);
                }
            }
        }
        true
    }
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
                        pixels: vec![64, 255, 0, 128],
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
