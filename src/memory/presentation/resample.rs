//! Integrate retained coverage directly into the drawable. Group samples in
//! ordinary guest pixels so bitmap content does not require a full 4x image.
use super::{MacMemoryBus, Presentation};
use crate::display::coverage_quotient;

struct Span {
    cell: usize,
    weight: u64,
    samples: Vec<(usize, u64)>,
}

fn axis(logical: u32, scale: u32, destination: u32) -> Vec<Vec<Span>> {
    let source = u64::from(logical) * u64::from(scale);
    let unit = u64::from(destination);
    (0..destination)
        .map(|position| {
            let mut spans: Vec<Span> = Vec::new();
            let mut add = |sample: u64, weight: u64| {
                let cell = (sample / u64::from(scale)) as usize;
                if spans.last().is_none_or(|span| span.cell != cell) {
                    spans.push(Span {
                        cell,
                        weight: 0,
                        samples: Vec::new(),
                    });
                }
                let span = spans.last_mut().unwrap();
                span.weight += weight;
                span.samples
                    .push(((sample % u64::from(scale)) as usize, weight));
            };
            if source <= unit {
                add(
                    ((u64::from(position) * 2 + 1) * source / (unit * 2)).min(source - 1),
                    1,
                );
            } else {
                let left = u64::from(position) * source;
                let right = (u64::from(position) + 1) * source;
                for sample in left / unit..right.div_ceil(unit) {
                    add(
                        sample,
                        right.min((sample + 1) * unit) - left.max(sample * unit),
                    );
                }
            }
            spans
        })
        .collect()
}

#[inline]
fn accumulate<const CHANNELS: usize>(sum: &mut [u64; CHANNELS], pixel: u32, weight: u64) {
    for (channel, value) in sum.iter_mut().enumerate() {
        *value += u64::from((pixel >> (channel * 8)) & 255) * weight;
    }
}

impl Presentation {
    #[inline]
    fn sample_argb(&self, x: usize, y: usize, sx: usize, sy: usize) -> u32 {
        let lanes = self.bytes_per_pixel() as usize;
        let mut rgb = [0u8; 3];
        for lane in 0..lanes {
            let bx = x * lanes + lane;
            let cell = y * self.width as usize + bx;
            let color = if self.text_cells[cell] {
                let scale = self.scale as usize;
                let offset = ((y * scale + sy) * self.width as usize * scale + bx * scale + sx) * 3;
                [
                    self.pixels[offset],
                    self.pixels[offset + 1],
                    self.pixels[offset + 2],
                ]
            } else {
                self.palette_at(bx as u32)[self.guest_values[cell] as u8 as usize]
            };
            for channel in 0..3 {
                rgb[channel] = rgb[channel].saturating_add(color[channel]);
            }
        }
        0xff000000 | (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])
    }

    fn render_resized<const CHANNELS: usize>(
        &self,
        size: (u32, u32),
        overlays: Option<(&[u32], &[u32])>,
        output: &mut Vec<u32>,
    ) {
        let width = self.logical_width();
        let horizontal = axis(width, self.scale, size.0);
        let vertical = axis(self.height, self.scale, size.1);
        let sw = width * self.scale;
        let sh = self.height * self.scale;
        let total = u64::from(if sw > size.0 { sw } else { 1 })
            * u64::from(if sh > size.1 { sh } else { 1 });
        let reciprocal = u64::MAX / total;
        let lanes = self.bytes_per_pixel() as usize;
        // Resolve each logical cell once. Fractional output footprints often
        // overlap several cells; repeating palette and direct-color lane
        // resolution in that inner loop is substantially more expensive than
        // integrating their already-resolved colors.
        let mut cells = Vec::with_capacity(width as usize * self.height as usize);
        for y in 0..self.height as usize {
            for x in 0..width as usize {
                let cell = y * self.width as usize + x * lanes;
                let text = self.text_cells[cell..cell + lanes].iter().any(|&text| text);
                // The retained surface is opaque. Reuse its otherwise constant
                // alpha byte for the text flag in this temporary cell table.
                cells.push((self.sample_argb(x, y, 0, 0) & 0x00ffffff) | ((text as u32) << 24));
            }
        }
        output.clear();
        output.reserve(size.0 as usize * size.1 as usize);
        for rows in &vertical {
            for columns in &horizontal {
                let mut sum = [0u64; CHANNELS];
                for row in rows {
                    for column in columns {
                        let logical = row.cell * width as usize + column.cell;
                        if let Some((_, overlays)) =
                            overlays.filter(|(guest, overlays)| guest[logical] != overlays[logical])
                        {
                            accumulate(&mut sum, overlays[logical], row.weight * column.weight);
                        } else if cells[logical] >> 24 == 0 {
                            // All retained samples of an ordinary guest pixel
                            // have one color: accumulate their combined area.
                            accumulate(
                                &mut sum,
                                cells[logical] | 0xff000000,
                                row.weight * column.weight,
                            );
                        } else {
                            for &(sy, wy) in &row.samples {
                                for &(sx, wx) in &column.samples {
                                    let pixel = self.sample_argb(column.cell, row.cell, sx, sy);
                                    accumulate(&mut sum, pixel, wy * wx);
                                }
                            }
                        }
                    }
                }
                let opaque = if CHANNELS == 3 { 0xff000000 } else { 0 };
                output.push(
                    sum.iter()
                        .enumerate()
                        .fold(opaque, |pixel, (channel, value)| {
                            pixel
                                | ((coverage_quotient(value + total / 2, total, reciprocal) as u32)
                                    << (channel * 8))
                        }),
                );
            }
        }
    }

    fn resolved_argb_resized(&self, size: (u32, u32)) -> std::cell::Ref<'_, [u32]> {
        if !self
            .output_cache
            .borrow()
            .as_ref()
            .is_some_and(|(revision, cached_size, _)| {
                *revision == self.revision && *cached_size == size
            })
        {
            let mut cache = self.output_cache.borrow_mut();
            let (revision, cached_size, pixels) =
                cache.get_or_insert_with(|| (0, (0, 0), Vec::new()));
            self.render_resized::<3>(size, None, pixels);
            *revision = self.revision;
            *cached_size = size;
        }
        std::cell::Ref::map(self.output_cache.borrow(), |cache| {
            cache.as_ref().unwrap().2.as_slice()
        })
    }
}

impl MacMemoryBus {
    /// Resolve retained text at the final drawable size, without first filtering
    /// it into an integer-scale intermediate image. This is equivalent to
    /// `presented_argb` followed by one `resize_argb_coverage`, including host
    /// overlays; guest pixels, font hinting, and classic advances are unchanged.
    pub fn presented_argb_resized(
        &self,
        guest: &[u32],
        with_overlays: &[u32],
        size: (u32, u32),
        output: &mut Vec<u32>,
    ) -> Option<(u32, u32)> {
        let p = self.presentation.as_ref()?;
        let width = p.logical_width();
        if size.0 == 0
            || size.1 == 0
            || guest.len() != width as usize * p.height as usize
            || with_overlays.len() != guest.len()
        {
            return None;
        }
        let scale = size.0 / width;
        if (1..=4).contains(&scale) && size == (width * scale, p.height * scale) {
            // Keep the existing fast integer path, particularly native size.
            return self.presented_argb_scaled(guest, with_overlays, scale, output);
        }
        if guest == with_overlays {
            output.clear();
            output.extend_from_slice(&p.resolved_argb_resized(size));
        } else {
            // Composite before integration, so software cursors and debug
            // overlays also get the correct fractional edge coverage.
            p.render_resized::<4>(size, Some((guest, with_overlays)), output);
        }
        Some(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryBus;

    fn assert_reference(bus: &MacMemoryBus, guest: &[u32], overlays: &[u32], size: (u32, u32)) {
        let (w, h, full) = bus.presented_argb(guest, overlays).unwrap();
        let mut expected = Vec::new();
        crate::display::resize_argb_coverage(&full, (w, h), size, &mut expected);
        let mut actual = Vec::new();
        assert_eq!(
            bus.presented_argb_resized(guest, overlays, size, &mut actual),
            Some(size)
        );
        assert_eq!(actual, expected, "output={size:?}");
    }

    #[test]
    fn final_size_matches_full_retained_image_with_mixed_depths_and_overlays() {
        let mut seed = 1234567u32;
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 16) as u8
        };
        for depth in [8, 16, 32] {
            for retained in 2..=4 {
                let mut bus = super::super::tests::bus();
                let lanes = depth / 8;
                bus.enable_outline_presentation(
                    (0x1000, u32::from(9 * lanes), 7, 5, depth),
                    std::array::from_fn(|_| [next(), next(), next()]),
                    retained,
                );
                {
                    let mut p = bus.presentation.as_mut().unwrap();
                    for i in 0..p.guest_values.len() {
                        p.guest_values[i] = u16::from(next());
                        p.text_cells[i] = next() % 3 == 0;
                    }
                    for value in &mut p.pixels {
                        *value = next();
                    }
                    p.revision += 1;
                }
                let guest = vec![0xff123456; 35];
                let mut overlay = guest.clone();
                overlay[0] = 0xffabcdef;
                overlay[17] = 0x80445566;
                overlay[34] = 0xff665544;
                for size in [
                    (1, 1),
                    (7, 5),
                    (9, 7),
                    (14, 10),
                    (21, 15),
                    (28, 20),
                    (6, 13),
                    (33, 27),
                ] {
                    assert_reference(&bus, &guest, &guest, size);
                    assert_reference(&bus, &guest, &overlay, size);
                    assert_reference(&bus, &guest, &guest, size);
                }
            }
        }
    }

    #[test]
    fn fractional_cache_tracks_writes_restores_palette_and_size_changes() {
        let mut bus = super::super::tests::bus();
        let screen = (0x1000, 8, 8, 8, 8);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.prepare_outline_presentation(screen, palette);
        super::super::tests::paint_detail(&mut bus, 0x1000);
        let saved = bus.save_pixel_bytes(0x1000, 64);
        let guest = [0xff123456; 64];
        let mut overlay = guest;
        overlay[0] = 0xffabcdef;
        for size in [(11, 11), (8, 8), (11, 11), (16, 16), (13, 11)] {
            assert_reference(&bus, &guest, &guest, size);
            assert_reference(&bus, &guest, &overlay, size);
            assert_reference(&bus, &guest, &guest, size);
        }
        bus.write_byte(0x1000, bus.read_byte(0x1000));
        assert_reference(&bus, &guest, &guest, (11, 11));
        bus.restore_saved_pixels(0x1000, &saved, 0, 64);
        assert_reference(&bus, &guest, &guest, (11, 11));
        bus.prepare_outline_presentation(screen, std::array::from_fn(|i| [255 - i as u8; 3]));
        assert_reference(&bus, &guest, &guest, (11, 11));
    }
}
