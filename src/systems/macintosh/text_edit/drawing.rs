//! Evidence that a TextEdit view still contains the pixels it last painted.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextEditDrawing {
    pub(crate) base: u32,
    port: u32,
    bitmap: u32,
    row_bytes: u32,
    pub(crate) depth: u16,
    bounds: (i16, i16, i16, i16),
    view: (i16, i16, i16, i16),
    pixels: Vec<u8>,
    editor_recipe: Option<Box<super::TextEditSnapshot>>,
    pub(crate) painted_regions: Vec<(i16, i16, i16, i16)>,
    pub(crate) solid_caret: Option<((i16, i16, i16, i16), u16, u16)>,
}

impl TextEditDrawing {
    /// Establish a uniform caret raster from this completed native drawing.
    /// Mixed patterns and partially clipped/nonuniform paint stay guest-owned.
    pub(crate) fn qualify_solid_caret(&mut self, rect: (i16, i16, i16, i16)) {
        self.solid_caret = None;
        let (top, left, bottom, right) = rect;
        if top < self.view.0 || left < self.view.1 || bottom > self.view.2
            || right > self.view.3 || top >= bottom || left >= right
            || !matches!(self.depth, 1 | 8 | 16) { return; }
        let depth = u32::from(self.depth);
        let first_byte = (i32::from(self.view.1) - i32::from(self.bounds.1)) as u32 * depth / 8;
        let end_byte = ((i32::from(self.view.3) - i32::from(self.bounds.1)) as u32 * depth).div_ceil(8);
        let stride = (end_byte - first_byte) as usize;
        let mut uniform = None;
        for y in top..bottom { for x in left..right {
            let bit = (i32::from(x) - i32::from(self.bounds.1)) as u32 * depth;
            let at = (i32::from(y) - i32::from(self.view.0)) as usize * stride
                + (bit / 8 - first_byte) as usize;
            let value = match self.depth {
                1 => u16::from((self.pixels[at] >> (7 - bit % 8)) & 1),
                8 => u16::from(self.pixels[at]),
                16 => u16::from_be_bytes([self.pixels[at], self.pixels[at + 1]]),
                _ => unreachable!(),
            };
            if uniform.is_some_and(|held| held != value) { return; }
            uniform = Some(value);
        } }
        self.solid_caret = uniform.map(|pixel| (rect, self.depth, pixel));
    }

    fn same_surface(&self, other: &Self) -> bool {
        self.base == other.base
            && self.port == other.port
            && self.bitmap == other.bitmap
            && self.row_bytes == other.row_bytes
            && self.depth == other.depth
            && self.bounds == other.bounds
            && self.view == other.view
    }

    pub(crate) fn same_pixels(&self, other: &Self) -> bool {
        self.same_surface(other) && self.pixels == other.pixels
            && self.editor_recipe == other.editor_recipe
    }

    /// Keep every unchanged pixel of a completed native draw. Application
    /// modifications stay native, including borders crossing standard cells.
    pub(crate) fn unchanged_painted_regions(&self, other: &Self) -> Vec<(i16, i16, i16, i16)> {
        if !self.same_surface(other) { return Vec::new(); }
        if self.pixels == other.pixels { return self.painted_regions.clone(); }
        let sample = |drawing: &Self, gx: i16, gy: i16| -> Option<u32> {
            let x = i32::from(gx) + i32::from(drawing.bounds.1);
            let y = i32::from(gy) + i32::from(drawing.bounds.0);
            if x < i32::from(drawing.view.1) || x >= i32::from(drawing.view.3)
                || y < i32::from(drawing.view.0) || y >= i32::from(drawing.view.2) { return None; }
            let depth = u32::from(drawing.depth);
            let first = (i32::from(drawing.view.1) - i32::from(drawing.bounds.1)) as u32 * depth / 8;
            let end = ((i32::from(drawing.view.3) - i32::from(drawing.bounds.1)) as u32 * depth).div_ceil(8);
            let bit = (x - i32::from(drawing.bounds.1)) as u32 * depth;
            let at = (y - i32::from(drawing.view.0)) as usize * (end - first) as usize
                + (bit / 8 - first) as usize;
            Some(match drawing.depth {
                1 | 2 | 4 => u32::from(*drawing.pixels.get(at)? >> (8 - depth - bit % 8)) & ((1 << depth) - 1),
                8 => u32::from(*drawing.pixels.get(at)?),
                16 => u32::from(u16::from_be_bytes(drawing.pixels.get(at..at + 2)?.try_into().ok()?)),
                32 => u32::from_be_bytes(drawing.pixels.get(at..at + 4)?.try_into().ok()?),
                _ => return None,
            })
        };
        let mut output: Vec<(i16, i16, i16, i16)> = Vec::new();
        for &(top, left, bottom, right) in &self.painted_regions {
            let mut previous: std::collections::HashMap<(i16, i16), usize> = std::collections::HashMap::new();
            for y in top..bottom {
                let mut current = std::collections::HashMap::new();
                let mut x = left;
                while x < right {
                    let unchanged = |x| sample(self, x, y).is_some_and(|pixel| Some(pixel) == sample(other, x, y));
                    if !unchanged(x) { x += 1; continue; }
                    let start = x;
                    while x < right && unchanged(x) { x += 1; }
                    let index = match previous.get(&(start, x)).copied() {
                        Some(index) => { output[index].2 = y + 1; index }
                        None => { output.push((y, start, y + 1, x)); output.len() - 1 }
                    };
                    current.insert((start, x), index);
                    // Complex custom modifications retain the native cell.
                    if output.len() > 256 { return Vec::new(); }
                }
                previous = current;
            }
        }
        output
    }

    /// Bind completed editor ink to the canonical guest contents and layout.
    /// Lists use `capture` directly because their cells have separate ownership.
    pub(crate) fn capture_editor(
        handle: u32,
        port: u32,
        view: (i16, i16, i16, i16),
        mut read: impl FnMut(u32) -> Option<u8>,
    ) -> Option<Self> {
        let recipe = super::snapshot_guest_records(&[(handle, 0)], &mut read)
            .records.into_iter().next()?;
        if recipe.owner_port != port || recipe.view_rect != view { return None; }
        let mut drawing = Self::capture(port, view, read)?;
        drawing.editor_recipe = Some(Box::new(recipe));
        Some(drawing)
    }

    /// Text (1993), pp. 2-16, 2-88: allocation does not paint a view.
    /// Capture only after drawing; inactive records can still contain visible text.
    /// BitMap/PixMap layout: Imaging With QuickDraw (1994), pp. 3-12, 4-46.
    pub(crate) fn capture(
        port: u32,
        view: (i16, i16, i16, i16),
        mut read: impl FnMut(u32) -> Option<u8>,
    ) -> Option<Self> {
        fn word(read: &mut impl FnMut(u32) -> Option<u8>, addr: u32) -> Option<u16> {
            Some(u16::from_be_bytes([
                read(addr)?,
                read(addr.checked_add(1)?)?,
            ]))
        }
        fn long(read: &mut impl FnMut(u32) -> Option<u8>, addr: u32) -> Option<u32> {
            Some(
                (u32::from(word(read, addr)?) << 16) | u32::from(word(read, addr.checked_add(2)?)?),
            )
        }
        if port == 0 {
            return None;
        }
        let bitmap = if word(&mut read, port.checked_add(6)?)? & 0xc000 == 0xc000 {
            let handle = long(&mut read, port.checked_add(2)?)?;
            if handle == 0 {
                return None;
            }
            long(&mut read, handle)?
        } else {
            port.checked_add(2)?
        };
        if bitmap == 0 {
            return None;
        }
        let base = long(&mut read, bitmap)?;
        let packed = word(&mut read, bitmap.checked_add(4)?)?;
        let row_bytes = u32::from(packed & 0x3fff);
        let depth = if packed & 0x8000 == 0 {
            1
        } else {
            word(&mut read, bitmap.checked_add(32)?)?
        };
        if base == 0 || row_bytes == 0 || !matches!(depth, 1 | 2 | 4 | 8 | 16 | 32) {
            return None;
        }
        let bounds = (
            word(&mut read, bitmap.checked_add(6)?)? as i16,
            word(&mut read, bitmap.checked_add(8)?)? as i16,
            word(&mut read, bitmap.checked_add(10)?)? as i16,
            word(&mut read, bitmap.checked_add(12)?)? as i16,
        );
        // A partially offscreen view needs clipping-aware composition first.
        if view.0 < bounds.0
            || view.1 < bounds.1
            || view.2 > bounds.2
            || view.3 > bounds.3
            || view.0 >= view.2
            || view.1 >= view.3
        {
            return None;
        }
        let first_bit =
            u32::try_from(i32::from(view.1) - i32::from(bounds.1)).ok()? * u32::from(depth);
        let end_bit =
            u32::try_from(i32::from(view.3) - i32::from(bounds.1)).ok()? * u32::from(depth);
        let first_byte = first_bit / 8;
        let end_byte = end_bit.div_ceil(8);
        if end_byte > row_bytes {
            return None;
        }
        let height = usize::try_from(i32::from(view.2) - i32::from(view.0)).ok()?;
        let len = height.checked_mul((end_byte - first_byte) as usize)?;
        // Reject corrupt guest geometry before allocating or reading large regions.
        if len > 16 * 1024 * 1024 {
            return None;
        }
        let mut pixels = Vec::with_capacity(len);
        for y in i32::from(view.0)..i32::from(view.2) {
            let row = base.checked_add((y - i32::from(bounds.0)) as u32 * row_bytes)?;
            for byte in first_byte..end_byte {
                let mut value = read(row.checked_add(byte)?)?;
                if byte == first_byte {
                    value &= 0xff >> (first_bit % 8);
                }
                if byte + 1 == end_byte && end_bit % 8 != 0 {
                    value &= 0xff << (8 - end_bit % 8);
                }
                pixels.push(value);
            }
        }
        let mut painted_regions = Vec::new();
        let visible = crate::window_manager::snapshot_port_region_rects(port, 24, false, |a| {
            read(a).unwrap_or(0)
        });
        let clipped = crate::window_manager::snapshot_port_region_rects(port, 28, false, |a| {
            read(a).unwrap_or(0)
        });
        // Intersect before translating: the usual +/-32767 clip rectangle
        // would wrap if translated into 16-bit screen coordinates first.
        let local_view = view;
        if let (Some(visible), Some(clipped)) = (visible, clipped) {
            for vis in visible {
                for clip in &clipped {
                    let rect = (
                        vis.0.max(clip.0).max(local_view.0),
                        vis.1.max(clip.1).max(local_view.1),
                        vis.2.min(clip.2).min(local_view.2),
                        vis.3.min(clip.3).min(local_view.3),
                    );
                    if rect.0 < rect.2 && rect.1 < rect.3 {
                        painted_regions.push(crate::window_manager::snapshot_local_rect_to_global(
                            rect,
                            (bounds.0, bounds.1),
                        ));
                    }
                }
            }
        }
        Some(Self {
            base,
            port,
            bitmap,
            row_bytes,
            depth,
            bounds,
            view,
            pixels,
            editor_recipe: None,
            painted_regions,
            solid_caret: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_caret_evidence_checks_full_fragment_and_packed_view_edges() {
        for depth in [1, 8, 16] {
            let mut mem = memory(depth);
            // The view starts midway through a packed monochrome byte.
            let mut drawing = TextEditDrawing::capture(16, (0, 3, 2, 7), |a| mem.get(a as usize).copied()).unwrap();
            drawing.qualify_solid_caret((0, 3, 2, 4));
            assert_eq!(drawing.solid_caret, Some(((0, 3, 2, 4), depth, 0)));
            let stride = (16 * depth).div_ceil(8) as usize;
            let bit = 3 * usize::from(depth);
            let at = 256 + stride + bit / 8;
            mem[at] = if depth == 1 { 1 << (7 - bit % 8) } else { 1 };
            let mut mixed = TextEditDrawing::capture(16, (0, 3, 2, 7), |a| mem.get(a as usize).copied()).unwrap();
            mixed.qualify_solid_caret((0, 3, 2, 4));
            assert_eq!(mixed.solid_caret, None, "mixed depth {depth}");
            drawing.qualify_solid_caret((0, 2, 2, 4));
            assert_eq!(drawing.solid_caret, None, "outside the owned view");
            drawing.qualify_solid_caret((0, 3, 0, 4));
            assert_eq!(drawing.solid_caret, None, "empty fragment");
        }
    }

    fn memory(depth: u16) -> Vec<u8> {
        let mut mem = vec![0; 1024];
        let bitmap = if depth == 1 {
            18
        } else {
            mem[18..22].copy_from_slice(&64u32.to_be_bytes());
            mem[22..24].copy_from_slice(&0xc000u16.to_be_bytes());
            mem[64..68].copy_from_slice(&80u32.to_be_bytes());
            80
        };
        mem[bitmap..bitmap + 4].copy_from_slice(&256u32.to_be_bytes());
        let row_bytes = (16 * depth).div_ceil(8);
        mem[bitmap + 4..bitmap + 6]
            .copy_from_slice(&(row_bytes | if depth == 1 { 0 } else { 0x8000 }).to_be_bytes());
        mem[bitmap + 10..bitmap + 12].copy_from_slice(&4u16.to_be_bytes());
        mem[bitmap + 12..bitmap + 14].copy_from_slice(&16u16.to_be_bytes());
        if depth != 1 {
            mem[bitmap + 32..bitmap + 34].copy_from_slice(&depth.to_be_bytes());
        }
        for (offset, handle, region) in [(24, 144u32, 160usize), (28, 148u32, 176usize)] {
            mem[16 + offset..20 + offset].copy_from_slice(&handle.to_be_bytes());
            mem[handle as usize..handle as usize + 4]
                .copy_from_slice(&(region as u32).to_be_bytes());
            mem[region..region + 2].copy_from_slice(&10u16.to_be_bytes());
            mem[region + 6..region + 8].copy_from_slice(&4u16.to_be_bytes());
            mem[region + 8..region + 10].copy_from_slice(&16u16.to_be_bytes());
        }
        mem
    }

    #[test]
    fn unchanged_regions_preserve_every_unmodified_native_pixel() {
        for depth in [1, 2, 4, 8, 16, 32] {
            let mut mem = memory(depth);
            let capture = |mem: &[u8]| TextEditDrawing::capture(16, (0, 0, 2, 16),
                |address| mem.get(address as usize).copied()).unwrap();
            let held = capture(&mem);
            assert_eq!(held.unchanged_painted_regions(&held), held.painted_regions);
            mem[256] ^= 0x80;
            let current = capture(&mem);
            let regions = held.unchanged_painted_regions(&current);
            for y in 0..2 { for x in 0..16 {
                let retained = regions.iter().any(|&(t, l, b, r)| y >= t && y < b && x >= l && x < r);
                assert_eq!(retained, (x, y) != (0, 0), "depth={depth}, point=({x},{y})");
            } }
        }
    }

    #[test]
    fn records_pixels_without_optional_outline_surface_at_every_depth() {
        for depth in [1, 2, 4, 8, 16, 32] {
            let mut mem = memory(depth);
            let capture = |mem: &[u8]| {
                TextEditDrawing::capture(16, (0, 1, 2, 7), |a| mem.get(a as usize).copied())
            };
            let before = capture(&mem).unwrap();
            let byte = 256 + usize::from(depth) / 8;
            mem[byte] ^= if depth == 1 { 0x40 } else { 1 };
            assert_ne!(capture(&mem), Some(before), "depth {depth}");
        }
    }

    #[test]
    fn monochrome_neighbours_do_not_invalidate_the_view() {
        let mut mem = memory(1);
        let capture = |mem: &[u8]| {
            TextEditDrawing::capture(16, (0, 1, 2, 7), |a| mem.get(a as usize).copied())
        };
        let before = capture(&mem).unwrap();
        mem[256] = 0x81;
        mem[260] = 0xff;
        assert_eq!(capture(&mem), Some(before));
    }

    #[test]
    fn unlimited_clip_is_intersected_before_translating_port_origin() {
        let mut mem = memory(8);
        mem.resize(2048, 0);
        mem[86..88].copy_from_slice(&(-1i16).to_be_bytes());
        mem[88..90].copy_from_slice(&(-1i16).to_be_bytes());
        for offset in [178, 180] {
            mem[offset..offset + 2].copy_from_slice(&(-32767i16).to_be_bytes());
        }
        for offset in [182, 184] {
            mem[offset..offset + 2].copy_from_slice(&32767i16.to_be_bytes());
        }
        let drawing =
            TextEditDrawing::capture(16, (0, 1, 2, 7), |a| mem.get(a as usize).copied()).unwrap();
        assert_eq!(drawing.painted_regions, vec![(1, 2, 3, 8)]);
    }

    #[test]
    fn retained_coverage_uses_the_clip_at_draw_time() {
        let mut mem = memory(8);
        mem[180..182].copy_from_slice(&2u16.to_be_bytes());
        mem[182..184].copy_from_slice(&1u16.to_be_bytes());
        mem[184..186].copy_from_slice(&5u16.to_be_bytes());
        let capture = |mem: &[u8]| {
            TextEditDrawing::capture(16, (0, 1, 2, 7), |a| mem.get(a as usize).copied())
        };
        let drawing = capture(&mem).unwrap();
        assert_eq!(drawing.painted_regions, vec![(0, 2, 1, 5)]);
        let slot = crate::memory::presentation::PresentationSlot::default();
        slot.record_text_edit_drawing(42, Some(drawing));
        // Restoring the caller's clip must not extend the last drawing's coverage.
        mem[180..182].copy_from_slice(&0u16.to_be_bytes());
        mem[182..184].copy_from_slice(&4u16.to_be_bytes());
        mem[184..186].copy_from_slice(&16u16.to_be_bytes());
        assert_eq!(
            slot.text_edit_drawing_regions(42, capture(&mem), 256),
            vec![(0, 2, 1, 5)]
        );
        // Drawing with an empty clip owns no text pixels.
        mem[182..184].copy_from_slice(&0u16.to_be_bytes());
        slot.record_text_edit_drawing(42, capture(&mem));
        assert!(!slot.text_edit_drawing_intact(42, capture(&mem), 256));
    }

    #[test]
    fn editor_mutation_requires_fresh_ink_even_when_pixels_match() {
        for depth in [1, 8, 16] {
            let mut mem = memory(depth);
            mem.resize(0x900, 0);
            for (at, value) in [(0x500, 0x600u32), (0x63e, 0x510),
                (0x510, 0x800), (0x652, 16)] {
                mem[at..at + 4].copy_from_slice(&value.to_be_bytes());
            }
            for (at, value) in [(0x608, 0u16), (0x60a, 1), (0x60c, 2),
                (0x60e, 7), (0x63c, 3), (0x65e, 1), (0x662, 3)] {
                mem[at..at + 2].copy_from_slice(&value.to_be_bytes());
            }
            mem[0x800..0x803].copy_from_slice(b"abc");
            let capture = |mem: &[u8]| TextEditDrawing::capture_editor(
                0x500, 16, (0, 1, 2, 7), |a| mem.get(a as usize).copied());
            let slot = crate::memory::presentation::PresentationSlot::default();
            slot.record_text_edit_drawing(0x500, capture(&mem));
            assert!(slot.text_edit_drawing_intact(0x500, capture(&mem), 256));
            for address in [0x800, 0x650, 0x64c, 0x63a, 0x64a, 0x620, 0x662] {
                let previous = mem[address];
                mem[address] ^= 1;
                assert!(!slot.text_edit_drawing_intact(0x500, capture(&mem), 256),
                    "raw editor mutation at {address:x}, depth {depth}");
                mem[address] = previous;
            }
            mem[0x800] = b'z';
            slot.record_text_edit_drawing(0x500, capture(&mem));
            assert!(slot.text_edit_drawing_intact(0x500, capture(&mem), 256));
        }
    }

    #[test]
    fn disposal_and_handle_reuse_require_fresh_drawing() {
        let mem = memory(1);
        let drawing = TextEditDrawing::capture(16, (0, 1, 2, 7), |a| mem.get(a as usize).copied());
        let slot = crate::memory::presentation::PresentationSlot::default();
        assert!(!slot.text_edit_drawing_intact(42, drawing.clone(), 256));
        let mut caret_drawing = drawing.clone().unwrap();
        caret_drawing.qualify_solid_caret((0, 3, 2, 4));
        slot.record_text_edit_drawing(42, Some(caret_drawing));
        assert_eq!(slot.text_edit_solid_caret(42), Some(((0, 3, 2, 4), 1, 0)));
        assert!(slot.text_edit_drawing_intact(42, drawing.clone(), 256));
        assert!(
            !slot.text_edit_drawing_intact(42, drawing.clone(), 512),
            "offscreen drawing is not visible"
        );
        slot.forget_text_edit_drawing(42);
        assert_eq!(slot.text_edit_solid_caret(42), None);
        assert!(!slot.text_edit_drawing_intact(42, drawing.clone(), 256));
        slot.record_text_edit_drawing(42, drawing);
        assert_eq!(slot.text_edit_solid_caret(42), None, "fresh non-caret drawing clears retained ink");
    }
}
