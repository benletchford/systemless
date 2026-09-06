//! Indexed-screen outline presentation. Guest memory and text metrics stay unchanged.
//! Ordinary framebuffer writes replace enlarged pixels in drawing order; supported
//! outline glyphs retain indexed coverage through snapshots and pixel transfers.
//! Frontends consume the presentation at its physical dimensions.
use super::{MacMemoryBus, MemoryBus};
use crate::quickdraw::fonts::{outline, Glyph};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(crate) struct OutlineGlyph {
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub left: i32,
    pub top: i32,
}

// Keep palette indexes through antialiasing so a CLUT change recolors existing
// coverage without rasterizing the guest's one-bit text again.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ink {
    foreground: u8,
    alpha: u32,
    background: IndexedColor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum IndexedColor {
    Solid(u8),
    Mix(Box<IndexedColor>, Box<IndexedColor>, u32),
}

impl IndexedColor {
    fn map(&mut self, map: &mut impl FnMut(u8) -> u8) {
        match self {
            Self::Solid(index) => *index = map(*index),
            Self::Mix(fg, bg, _) => {
                fg.map(map);
                bg.map(map);
            }
        }
    }
    fn over(self, foreground: u8, alpha: u32) -> Self {
        if alpha == 0 {
            self
        } else if alpha == 255 {
            Self::Solid(foreground)
        } else {
            Self::Mix(Box::new(Self::Solid(foreground)), Box::new(self), alpha)
        }
    }
    fn combine(&self, dst: &Self, map: &mut impl FnMut(u8, u8) -> u8) -> Self {
        match (self, dst) {
            (Self::Solid(src), Self::Solid(dst)) => Self::Solid(map(*src, *dst)),
            (Self::Mix(fg, bg, alpha), _) => Self::Mix(
                Box::new(fg.combine(dst, map)),
                Box::new(bg.combine(dst, map)),
                *alpha,
            ),
            (_, Self::Mix(fg, bg, alpha)) => Self::Mix(
                Box::new(self.combine(fg, map)),
                Box::new(self.combine(bg, map)),
                *alpha,
            ),
        }
    }
    fn rgb(&self, palette: &[[u8; 3]; 256]) -> [u8; 3] {
        match self {
            Self::Solid(index) => palette[*index as usize],
            Self::Mix(foreground, background, alpha) => {
                blend(foreground.rgb(palette), background.rgb(palette), *alpha)
            }
        }
    }
}

fn blend(foreground: [u8; 3], background: [u8; 3], alpha: u32) -> [u8; 3] {
    std::array::from_fn(|c| {
        ((u32::from(foreground[c]) * alpha + u32::from(background[c]) * (255 - alpha) + 127) / 255)
            as u8
    })
}

impl Ink {
    fn rgb(&self, palette: &[[u8; 3]; 256]) -> [u8; 3] {
        blend(
            palette[self.foreground as usize],
            self.background.rgb(palette),
            self.alpha,
        )
    }
}

/// An owned pixel snapshot carries the indexed subpixels with the guest bytes.
/// Cloning a snapshot preserves its coverage even after its source is erased.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SavedPixels<T = u8> {
    values: Vec<T>,
    detail: HashMap<usize, DetailCell>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DetailCell {
    value: u8,
    indices: Vec<u8>,
    ink: HashMap<usize, Ink>,
}

impl<T> From<Vec<T>> for SavedPixels<T> {
    fn from(values: Vec<T>) -> Self {
        Self {
            values,
            detail: HashMap::new(),
        }
    }
}
impl<T> std::ops::Deref for SavedPixels<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Vec<T> {
        &self.values
    }
}
impl<T> std::ops::DerefMut for SavedPixels<T> {
    fn deref_mut(&mut self) -> &mut Vec<T> {
        // Raw logical edits cannot retain stale coverage. Region refreshes use
        // replace_range so unrelated parts of a snapshot retain their detail.
        self.detail.clear();
        &mut self.values
    }
}
impl<'a, T> IntoIterator for &'a SavedPixels<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
impl<T> SavedPixels<T> {
    pub fn map<U>(self, map: impl FnMut(T) -> U) -> SavedPixels<U> {
        SavedPixels {
            values: self.values.into_iter().map(map).collect(),
            detail: self.detail,
        }
    }
    pub fn into_vec(self) -> Vec<T> {
        self.values
    }
    pub(crate) fn slice(&self, range: std::ops::Range<usize>) -> Self
    where
        T: Clone,
    {
        Self {
            values: self.values[range.clone()].to_vec(),
            detail: self
                .detail
                .iter()
                .filter(|(i, _)| range.contains(i))
                .map(|(i, cell)| (i - range.start, cell.clone()))
                .collect(),
        }
    }
    pub(crate) fn replace_range(&mut self, offset: usize, values: &[T])
    where
        T: Clone,
    {
        let end = offset + values.len();
        self.detail.retain(|i, _| *i < offset || *i >= end);
        self.values[offset..end].clone_from_slice(values);
    }
}

pub(crate) struct Presentation {
    offscreen: HashMap<u32, DetailCell>,
    base: u32,
    row_bytes: u32,
    width: u32,
    height: u32,
    pub scale: u32,
    palette: [[u8; 3]; 256],
    pixels: Vec<u8>,
    pixel_indices: Vec<u8>,
    guest_values: Vec<u16>,
    text_cells: Vec<bool>,
    ink: HashMap<usize, Ink>,
    run_ink: HashSet<usize>,
    offscreen_run_ink: HashSet<(u32, usize)>,
    in_text_run: bool,
    pub erasing_text: bool,
    glyph: Option<(OutlineGlyph, i16, i16)>,
    pub glyph_count: usize,
}

impl Presentation {
    fn position(&self, address: u32) -> Option<(u32, u32)> {
        let offset = address.checked_sub(self.base)?;
        let (x, y) = (offset % self.row_bytes, offset / self.row_bytes);
        (x < self.width && y < self.height).then_some((x, y))
    }

    fn detail(&self, address: u32) -> Option<DetailCell> {
        let Some((x, y)) = self.position(address) else {
            return self.offscreen.get(&address).cloned();
        };
        if !self.text_cells[(y * self.width + x) as usize] {
            return None;
        }
        let mut cell = DetailCell {
            value: self.guest_values[(y * self.width + x) as usize] as u8,
            indices: Vec::new(),
            ink: HashMap::new(),
        };
        for sy in 0..self.scale {
            for sx in 0..self.scale {
                let pixel = ((y * self.scale + sy) * self.width * self.scale + x * self.scale + sx)
                    as usize;
                let i = cell.indices.len();
                cell.indices.push(self.pixel_indices[pixel]);
                if let Some(ink) = self.ink.get(&(pixel * 3)) {
                    cell.ink.insert(i, ink.clone());
                }
            }
        }
        Some(cell)
    }

    fn matches_detail(&self, address: u32, detail: Option<&DetailCell>) -> bool {
        let Some((x, y)) = self.position(address) else {
            return self.offscreen.get(&address) == detail;
        };
        let Some(cell) = detail else {
            return !self.text_cells[(y * self.width + x) as usize];
        };
        if cell.indices.len() != (self.scale * self.scale) as usize {
            return false;
        }
        for sy in 0..self.scale {
            for sx in 0..self.scale {
                let i = (sy * self.scale + sx) as usize;
                let pixel = ((y * self.scale + sy) * self.width * self.scale + x * self.scale + sx)
                    as usize;
                if self.pixel_indices[pixel] != cell.indices[i]
                    || self.ink.get(&(pixel * 3)) != cell.ink.get(&i)
                {
                    return false;
                }
            }
        }
        true
    }

    fn put_detail(&mut self, address: u32, cell: &DetailCell) {
        let Some((x, y)) = self.position(address) else {
            self.offscreen.insert(address, cell.clone());
            return;
        };
        if cell.indices.len() != (self.scale * self.scale) as usize {
            return;
        }
        self.text_cells[(y * self.width + x) as usize] = true;
        self.guest_values[(y * self.width + x) as usize] = cell.value.into();
        for sy in 0..self.scale {
            for sx in 0..self.scale {
                let i = (sy * self.scale + sx) as usize;
                let pixel = ((y * self.scale + sy) * self.width * self.scale + x * self.scale + sx)
                    as usize;
                self.pixel_indices[pixel] = cell.indices[i];
                self.ink.remove(&(pixel * 3));
                let rgb = if let Some(ink) = cell.ink.get(&i) {
                    self.ink.insert(pixel * 3, ink.clone());
                    ink.rgb(&self.palette)
                } else {
                    self.palette[cell.indices[i] as usize]
                };
                self.pixels[pixel * 3..pixel * 3 + 3].copy_from_slice(&rgb);
            }
        }
    }

    pub fn glyph_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let (g, h, v) = self.glyph.as_ref()?;
        let scale = self.scale as i32;
        Some((
            i32::from(*v) + g.top.div_euclid(scale),
            i32::from(*h) + g.left.div_euclid(scale),
            i32::from(*v) + (g.top + g.height + scale - 1).div_euclid(scale),
            i32::from(*h) + (g.left + g.width + scale - 1).div_euclid(scale),
        ))
    }

    pub fn write(&mut self, address: u32, value: u8) {
        let Some((x, y)) = self.position(address) else {
            if self.glyph.is_some() {
                if let Some(cell) = self.offscreen.get_mut(&address) {
                    cell.value = value;
                }
            } else if self.erasing_text
                && self
                    .offscreen_run_ink
                    .iter()
                    .any(|(addr, _)| *addr == address)
            {
                if let Some(cell) = self.offscreen.get_mut(&address) {
                    cell.value = value;
                    for i in 0..cell.indices.len() {
                        if !self.offscreen_run_ink.contains(&(address, i)) {
                            cell.indices[i] = value;
                            cell.ink.remove(&i);
                        }
                    }
                }
            } else {
                self.offscreen.remove(&address);
            }
            return;
        };
        let cell = (y * self.width + x) as usize;
        if self.glyph.is_some() {
            self.guest_values[cell] = u16::from(value);
            self.text_cells[cell] = true;
            return;
        }
        if self.guest_values[cell] == u16::from(value) && !self.text_cells[cell] {
            return;
        }
        self.guest_values[cell] = u16::from(value);
        self.text_cells[cell] = false;
        let color = self.palette[value as usize];
        for sy in 0..self.scale {
            for sx in 0..self.scale {
                let offset =
                    (((y * self.scale + sy) * self.width * self.scale + x * self.scale + sx) * 3)
                        as usize;
                // A following character's opaque background must not shave off
                // an outline overhang already painted by this same text run.
                if self.erasing_text && self.run_ink.contains(&offset) {
                    self.text_cells[cell] = true;
                    continue;
                }
                self.run_ink.remove(&offset);
                self.ink.remove(&offset);
                self.pixel_indices[offset / 3] = value;
                self.pixels[offset..offset + 3].copy_from_slice(&color);
            }
        }
    }

    /// Called for every visible glyph cell, including cells with zero 1x ink.
    /// QuickDraw has already applied both the visibility and clipping regions.
    pub fn glyph_pixel(&mut self, address: u32, x: i16, y: i16, foreground: u8, background: u8) {
        let Some((px, py)) = self.position(address) else {
            let Some((glyph, h, v)) = &self.glyph else {
                return;
            };
            let cell = self.offscreen.entry(address).or_insert_with(|| DetailCell {
                value: background,
                indices: vec![background; (self.scale * self.scale) as usize],
                ink: HashMap::new(),
            });
            for sy in 0..self.scale {
                for sx in 0..self.scale {
                    let gx =
                        (i32::from(x) - i32::from(*h)) * self.scale as i32 + sx as i32 - glyph.left;
                    let gy =
                        (i32::from(y) - i32::from(*v)) * self.scale as i32 + sy as i32 - glyph.top;
                    if gx < 0 || gy < 0 || gx >= glyph.width || gy >= glyph.height {
                        continue;
                    }
                    let alpha = u32::from(glyph.pixels[(gy * glyph.width + gx) as usize]);
                    if alpha == 0 {
                        continue;
                    }
                    let i = (sy * self.scale + sx) as usize;
                    if self.in_text_run {
                        self.offscreen_run_ink.insert((address, i));
                    }
                    if alpha == 255 {
                        cell.indices[i] = foreground;
                        cell.ink.remove(&i);
                    } else {
                        let ink = cell.ink.entry(i).or_insert_with(|| Ink {
                            foreground,
                            alpha: 0,
                            background: IndexedColor::Solid(cell.indices[i]),
                        });
                        if ink.foreground != foreground {
                            let previous = ink.clone();
                            *ink = Ink {
                                foreground,
                                alpha: 0,
                                background: previous
                                    .background
                                    .over(previous.foreground, previous.alpha),
                            };
                        }
                        ink.alpha = ink.alpha.max(alpha);
                    }
                }
            }
            return;
        };
        let Some((glyph, h, v)) = &self.glyph else {
            return;
        };
        self.text_cells[(py * self.width + px) as usize] = true;
        let color = self.palette[foreground as usize];
        for sy in 0..self.scale {
            for sx in 0..self.scale {
                let gx =
                    (i32::from(x) - i32::from(*h)) * self.scale as i32 + sx as i32 - glyph.left;
                let gy = (i32::from(y) - i32::from(*v)) * self.scale as i32 + sy as i32 - glyph.top;
                if gx < 0 || gy < 0 || gx >= glyph.width || gy >= glyph.height {
                    continue;
                }
                let alpha = u32::from(glyph.pixels[(gy * glyph.width + gx) as usize]);
                if alpha == 0 {
                    continue;
                }
                let offset =
                    (((py * self.scale + sy) * self.width * self.scale + px * self.scale + sx) * 3)
                        as usize;
                if self.in_text_run {
                    self.run_ink.insert(offset);
                }
                if alpha == 255 {
                    self.pixels[offset..offset + 3].copy_from_slice(&color);
                    self.ink.remove(&offset);
                    self.pixel_indices[offset / 3] = foreground;
                    continue;
                }
                // Inside Macintosh I, "Transfer Modes": srcOr forces source
                // ink on and leaves other bits alone; repeated ink is idempotent.
                // Retain coverage rather than
                // repeatedly blending the same ink into its own antialiased edge.
                let ink = self.ink.entry(offset).or_insert_with(|| Ink {
                    foreground,
                    alpha: 0,
                    background: IndexedColor::Solid(self.pixel_indices[offset / 3]),
                });
                if ink.foreground != foreground {
                    let previous = std::mem::replace(
                        ink,
                        Ink {
                            foreground,
                            alpha: 0,
                            background: IndexedColor::Solid(0),
                        },
                    );
                    ink.background = previous
                        .background
                        .over(previous.foreground, previous.alpha);
                }
                ink.alpha = ink.alpha.max(alpha);
                self.pixels[offset..offset + 3].copy_from_slice(&ink.rgb(&self.palette));
            }
        }
    }
}

impl MacMemoryBus {
    pub(crate) fn outline_glyph_pixel(&mut self, address: u32, x: i16, y: i16, foreground: u8) {
        if self.presentation.is_none() {
            return;
        }
        let background = self.read_byte(address);
        if let Some(p) = &mut self.presentation {
            p.glyph_pixel(address, x, y, foreground, background);
        }
    }

    pub(crate) fn save_pixel_bytes(&self, address: u32, len: usize) -> SavedPixels {
        let mut pixels = SavedPixels::from(self.read_bytes(address, len));
        self.capture_pixel_detail(&mut pixels, 0, address, len);
        pixels
    }

    pub(crate) fn transfer_saved_pixel(
        &mut self,
        address: u32,
        source: &SavedPixels,
        offset: usize,
        mut map: impl FnMut(&MacMemoryBus, u8, u8) -> u8,
    ) -> bool {
        let src = source.detail.get(&offset);
        let dst = self.presentation.as_ref().and_then(|p| p.detail(address));
        if src.is_none() && dst.is_none() {
            return false;
        }
        let old = self.read_byte(address);
        let value = map(self, source[offset], old);
        let scale = self.presentation.as_ref().unwrap().scale;
        let mut cell = DetailCell {
            value,
            indices: vec![value; (scale * scale) as usize],
            ink: HashMap::new(),
        };
        let color = |cell: Option<&DetailCell>, i: usize, fallback| {
            cell.map_or(IndexedColor::Solid(fallback), |cell| {
                cell.ink
                    .get(&i)
                    .map_or(IndexedColor::Solid(cell.indices[i]), |ink| {
                        ink.background.clone().over(ink.foreground, ink.alpha)
                    })
            })
        };
        for i in 0..cell.indices.len() {
            let src_color = color(src, i, source[offset]);
            let dst_color = color(dst.as_ref(), i, old);
            let output = if src_color == dst_color {
                let mut same = src_color;
                same.map(&mut |index| map(self, index, index));
                same
            } else {
                src_color.combine(&dst_color, &mut |s, d| map(self, s, d))
            };
            match output {
                IndexedColor::Solid(index) => cell.indices[i] = index,
                background => {
                    cell.ink.insert(
                        i,
                        Ink {
                            foreground: value,
                            alpha: 0,
                            background,
                        },
                    );
                }
            }
        }
        self.write_byte(address, value);
        if let Some(p) = &mut self.presentation {
            p.put_detail(address, &cell);
        }
        true
    }

    pub(crate) fn copy_saved_pixel(
        &mut self,
        address: u32,
        pixels: &SavedPixels,
        offset: usize,
        mut map: impl Fn(u8) -> u8,
    ) {
        let value = map(pixels[offset]);
        self.write_byte(address, value);
        if let Some(cell) = pixels.detail.get(&offset) {
            let mut cell = cell.clone();
            cell.value = value;
            for index in &mut cell.indices {
                *index = map(*index);
            }
            for ink in cell.ink.values_mut() {
                ink.foreground = map(ink.foreground);
                ink.background.map(&mut map);
            }
            if let Some(p) = &mut self.presentation {
                p.put_detail(address, &cell);
            }
        }
    }

    pub(crate) fn capture_pixel_detail<T>(
        &self,
        pixels: &mut SavedPixels<T>,
        offset: usize,
        address: u32,
        len: usize,
    ) {
        if let Some(p) = &self.presentation {
            for i in 0..len {
                if let Some(cell) = p.detail(address + i as u32) {
                    pixels.detail.insert(offset + i, cell);
                }
            }
        }
    }

    pub(crate) fn restore_saved_pixels<T: Copy + Into<u16>>(
        &mut self,
        address: u32,
        pixels: &SavedPixels<T>,
        offset: usize,
        len: usize,
    ) {
        if self.presentation.is_none() {
            let end = offset.saturating_add(len).min(pixels.len());
            if offset < end {
                let bytes: Vec<u8> = pixels[offset..end]
                    .iter()
                    .map(|value| (*value).into() as u8)
                    .collect();
                self.write_bytes(address, &bytes);
            }
            return;
        }
        for i in offset..offset.saturating_add(len).min(pixels.len()) {
            let dst = address + (i - offset) as u32;
            let value = pixels[i].into() as u8;
            let detail = pixels.detail.get(&i).filter(|cell| cell.value == value);
            if self.read_byte(dst) == value
                && self
                    .presentation
                    .as_ref()
                    .is_some_and(|p| p.matches_detail(dst, detail))
            {
                continue;
            }
            self.write_byte(dst, value);
            if let Some(cell) = detail {
                if let Some(p) = &mut self.presentation {
                    p.put_detail(dst, cell);
                }
            }
        }
    }

    /// Apply the same indexed operation to each physical sample, retaining coverage.
    pub(crate) fn map_screen_byte(&mut self, address: u32, mut map: impl Fn(u8) -> u8) {
        let mut cell = self.presentation.as_ref().and_then(|p| p.detail(address));
        let value = map(self.read_byte(address));
        self.write_byte(address, value);
        if let Some(cell) = &mut cell {
            cell.value = value;
            for index in &mut cell.indices {
                *index = map(*index);
            }
            for ink in cell.ink.values_mut() {
                ink.foreground = map(ink.foreground);
                ink.background.map(&mut map);
            }
            if let Some(p) = &mut self.presentation {
                p.put_detail(address, cell);
            }
        }
    }

    /// Invert indexed dialog selection pixels without flattening their outline
    /// coverage. Transform palette indexes, matching the guest's byte inversion.
    pub(crate) fn invert_screen_byte(&mut self, address: u32) {
        self.map_screen_byte(address, |index| !index);
    }

    /// Maintain a 4x outline surface for an indexed screen. Geometry changes
    /// recreate the surface; palette changes recolor the retained coverage.
    pub fn prepare_outline_presentation(
        &mut self,
        screen: (u32, u32, u16, u16, u16),
        palette: [[u8; 3]; 256],
    ) {
        if screen.4 != 8 {
            self.presentation = None;
            return;
        }
        if let Some(p) = self.presentation.as_mut().filter(|p| {
            (p.base, p.row_bytes, p.width, p.height)
                == (screen.0, screen.1, screen.2 as u32, screen.3 as u32)
                && p.scale == 4
        }) {
            if p.palette != palette {
                // Indexed pixels retain their CLUT indexes when the device's
                // colors change (Imaging With QuickDraw, 1994, 4-5–4-6).
                p.palette = palette;
                for (index, pixel) in p.pixel_indices.iter().zip(p.pixels.chunks_exact_mut(3)) {
                    pixel.copy_from_slice(&palette[*index as usize]);
                }
                for (&offset, ink) in &p.ink {
                    p.pixels[offset..offset + 3].copy_from_slice(&ink.rgb(&palette));
                }
            }
        } else {
            self.enable_outline_presentation(screen, palette, 4);
        }
    }

    /// Whether an outline presentation surface is available to a frontend.
    pub fn has_outline_presentation(&self) -> bool {
        self.presentation.is_some()
    }

    /// Composite host overlays onto the outline surface. Overlay positions stay
    /// in guest coordinates; only the returned presentation dimensions change.
    pub fn presented_argb(
        &self,
        guest: &[u32],
        with_overlays: &[u32],
    ) -> Option<(u32, u32, Vec<u32>)> {
        let p = self.presentation.as_ref()?;
        if guest.len() != (p.width * p.height) as usize || with_overlays.len() != guest.len() {
            return None;
        }
        let width = p.width * p.scale;
        let height = p.height * p.scale;
        let mut pixels: Vec<u32> = p
            .pixels
            .chunks_exact(3)
            .map(|c| 0xff000000 | ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32)
            .collect();
        for (index, (&before, &after)) in guest.iter().zip(with_overlays).enumerate() {
            if before == after {
                continue;
            }
            let x = index as u32 % p.width;
            let y = index as u32 / p.width;
            for dy in 0..p.scale {
                let start = ((y * p.scale + dy) * width + x * p.scale) as usize;
                pixels[start..start + p.scale as usize].fill(after);
            }
        }
        Some((width, height, pixels))
    }

    /// Start an indexed outline surface at 2x through 4x resolution.
    /// Frontends normally use `prepare_outline_presentation` to track mode changes.
    pub fn enable_outline_presentation(
        &mut self,
        screen: (u32, u32, u16, u16, u16),
        palette: [[u8; 3]; 256],
        scale: u32,
    ) {
        let (base, row_bytes, width, height, depth) = screen;
        assert_eq!(depth, 8, "outline presentation supports 8-bit screens only");
        assert!((2..=4).contains(&scale));
        assert!(width > 0 && height > 0 && row_bytes >= u32::from(width));
        assert!(
            u64::from(base) + u64::from(row_bytes) * u64::from(height)
                <= u64::from(self.ram_size())
        );
        let mut presentation = Presentation {
            offscreen: HashMap::new(),
            base,
            row_bytes,
            width: width.into(),
            height: height.into(),
            scale,
            palette,
            pixels: vec![0; width as usize * height as usize * scale as usize * scale as usize * 3],
            pixel_indices: vec![
                0;
                width as usize * height as usize * scale as usize * scale as usize
            ],
            guest_values: vec![256; width as usize * height as usize],
            text_cells: vec![false; width as usize * height as usize],
            ink: HashMap::new(),
            run_ink: HashSet::new(),
            offscreen_run_ink: HashSet::new(),
            in_text_run: false,
            erasing_text: false,
            glyph: None,
            glyph_count: 0,
        };
        for y in 0..u32::from(height) {
            for x in 0..u32::from(width) {
                let address = base + y * row_bytes + x;
                presentation.write(address, self.read_byte(address));
            }
        }
        self.presentation = Some(presentation);
    }

    /// Return the real guest's presentation capture and number of outline draws.
    pub fn outline_presentation_rgb(&self) -> Option<(u32, u32, Vec<u8>, usize)> {
        let p = self.presentation.as_ref()?;
        Some((
            p.width * p.scale,
            p.height * p.scale,
            p.pixels.clone(),
            p.glyph_count,
        ))
    }

    pub(crate) fn begin_presentation_text_run(&mut self, opaque: bool) {
        if let Some(p) = &mut self.presentation {
            p.run_ink.clear();
            p.offscreen_run_ink.clear();
            p.in_text_run = opaque;
        }
    }

    pub(crate) fn end_presentation_text_run(&mut self) {
        if let Some(p) = &mut self.presentation {
            p.run_ink.clear();
            p.offscreen_run_ink.clear();
            p.in_text_run = false;
        }
    }

    pub(crate) fn begin_outline_glyph(
        &mut self,
        glyph: &Glyph,
        data: &[u8],
        x: i16,
        y: i16,
        bold: bool,
        italic_descent: Option<i16>,
        underline: Option<(i16, i16)>,
    ) {
        let Some(p) = &mut self.presentation else {
            return;
        };
        if let Some(mut outline) = outline::presentation_glyph(glyph, data, p.scale) {
            if let Some(descent) = italic_descent {
                // Apply the shared QuickDraw shear on the physical grid rather
                // than enlarging the already sheared one-bit strike.
                let bottom = (i32::from(descent) - 1) * p.scale as i32;
                let shift = |row: i32| (bottom - outline.top - row).max(0) / 2;
                let width = outline.width + shift(0);
                let mut pixels = vec![0; (width * outline.height) as usize];
                for row in 0..outline.height {
                    let src = (row * outline.width) as usize;
                    let dst = (row * width + shift(row)) as usize;
                    pixels[dst..dst + outline.width as usize]
                        .copy_from_slice(&outline.pixels[src..src + outline.width as usize]);
                }
                outline.width = width;
                outline.pixels = pixels;
            }
            if bold && outline.width > 0 {
                let width = outline.width + p.scale as i32;
                let mut pixels = vec![0; (width * outline.height) as usize];
                for y in 0..outline.height {
                    for x in 0..outline.width {
                        let alpha = outline.pixels[(y * outline.width + x) as usize];
                        for shift in 0..=p.scale as i32 {
                            let dst = &mut pixels[(y * width + x + shift) as usize];
                            *dst = (*dst).max(alpha);
                        }
                    }
                }
                outline.width = width;
                outline.pixels = pixels;
            }
            if let Some((advance, thickness)) = underline {
                let scale = p.scale as i32;
                let left = outline.left.min(0);
                let top = outline.top.min(scale);
                let right = (outline.left + outline.width).max(i32::from(advance) * scale);
                let bottom = (outline.top + outline.height).max((1 + i32::from(thickness)) * scale);
                let width = right - left;
                let height = bottom - top;
                let mut pixels = vec![0; (width * height) as usize];
                // Underlines break around descenders. Measure their coverage at
                // the physical resolution, keeping a one-guest-pixel clearance.
                let mut descenders = vec![false; width as usize];
                for row in 0..outline.height {
                    for col in 0..outline.width {
                        let alpha = outline.pixels[(row * outline.width + col) as usize];
                        let px = outline.left + col - left;
                        let py = outline.top + row - top;
                        pixels[(py * width + px) as usize] = alpha;
                        if outline.top + row >= 0 && alpha >= 128 {
                            for nearby in (px - scale).max(0)..=(px + scale).min(width - 1) {
                                descenders[nearby as usize] = true;
                            }
                        }
                    }
                }
                for px in -left..i32::from(advance) * scale - left {
                    if !descenders[px as usize] {
                        for py in scale - top..(1 + i32::from(thickness)) * scale - top {
                            pixels[(py * width + px) as usize] = 255;
                        }
                    }
                }
                outline = OutlineGlyph {
                    pixels,
                    width,
                    height,
                    left,
                    top,
                };
            }
            p.glyph = Some((outline, x, y));
            p.glyph_count += 1;
        }
    }

    /// Synthesize hollow outline/shadow masks on the physical grid using the
    /// same smear-and-remove rule as the logical QuickDraw renderer.
    pub(crate) fn style_outline_glyph(
        &mut self,
        style: crate::quickdraw::text::QuickDrawTextStyle,
    ) {
        let Some(p) = &mut self.presentation else {
            return;
        };
        let Some((glyph, _, _)) = &mut p.glyph else {
            return;
        };
        let Some(radius) = style.smear_max() else {
            return;
        };
        let scale = p.scale as i32;
        let pad = scale;
        let width = glyph.width + pad + radius * scale;
        let height = glyph.height + pad + radius * scale;
        let mut pixels = vec![0u8; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                let mut alpha = 0;
                for dy in -scale..=radius * scale {
                    for dx in -scale..=radius * scale {
                        let gx = x - pad - dx;
                        let gy = y - pad - dy;
                        if gx >= 0 && gy >= 0 && gx < glyph.width && gy < glyph.height {
                            alpha = alpha.max(glyph.pixels[(gy * glyph.width + gx) as usize]);
                        }
                    }
                }
                let gx = x - pad;
                let gy = y - pad;
                if gx >= 0 && gy >= 0 && gx < glyph.width && gy < glyph.height {
                    alpha = alpha.saturating_sub(glyph.pixels[(gy * glyph.width + gx) as usize]);
                }
                pixels[(y * width + x) as usize] = alpha;
            }
        }
        glyph.left -= pad;
        glyph.top += style.glyph_y_offset() * scale - pad;
        glyph.width = width;
        glyph.height = height;
        glyph.pixels = pixels;
    }

    pub(crate) fn end_outline_glyph(&mut self) {
        if let Some(p) = &mut self.presentation {
            p.glyph = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus() -> MacMemoryBus {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.fill_bytes(0x1000, 64, 255);
        bus.enable_outline_presentation((0x1000, 8, 8, 8, 8), palette, 2);
        bus
    }

    fn paint_detail(bus: &mut MacMemoryBus, address: u32) {
        bus.presentation.as_mut().unwrap().glyph = Some((
            OutlineGlyph {
                pixels: vec![64, 255, 0, 128],
                width: 2,
                height: 2,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        bus.outline_glyph_pixel(address, 0, 0, 0);
        bus.write_byte(address, 0);
        bus.end_outline_glyph();
    }

    #[test]
    fn snapshots_restore_covered_ink_and_clear_ink_absent_from_snapshot() {
        let mut bus = bus();
        let blank = bus.save_pixel_bytes(0x1000, 8);
        paint_detail(&mut bus, 0x1000);
        let expected = bus.outline_presentation_rgb().unwrap().2;
        let saved = bus.save_pixel_bytes(0x1000, 8).clone();
        bus.fill_bytes(0x1000, 8, 42);
        bus.restore_saved_pixels(0x1000, &saved, 0, 8);
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, expected);
        // Restore blank pixels even where their guest byte matches a glyph's
        // empty logical cell: no stale subpixel ink may remain behind.
        bus.restore_saved_pixels(0x1000, &blank, 0, 8);
        assert!(bus.presentation.as_ref().unwrap().ink.is_empty());
        assert!(bus
            .outline_presentation_rgb()
            .unwrap()
            .2
            .iter()
            .all(|&v| v == 255));
    }

    #[test]
    fn offscreen_round_trip_overlap_and_palette_translation_preserve_detail() {
        let mut bus = bus();
        bus.write_byte(0x2000, 255);
        paint_detail(&mut bus, 0x2000);
        assert!(bus.presentation.as_ref().unwrap().detail(0x2000).is_some());
        bus.block_move(0x2000, 0x1000, 1);
        let original = bus.presentation.as_ref().unwrap().detail(0x1000).unwrap();
        assert!(bus.copy_ram_bytes(0x1000, 0x1001, 7));
        assert_eq!(
            bus.presentation.as_ref().unwrap().detail(0x1001),
            Some(original.clone())
        );
        let table = std::array::from_fn(|i| 255 - i as u8);
        assert!(bus.copy_mapped_ram_bytes(0x1001, 0x2001, 1, &table));
        assert!(bus.copy_mapped_ram_bytes(0x2001, 0x1002, 1, &table));
        assert_eq!(
            bus.presentation.as_ref().unwrap().detail(0x1002),
            Some(original)
        );
        bus.write_byte(0x2002, 255);
        bus.presentation.as_mut().unwrap().glyph = Some((
            OutlineGlyph {
                pixels: vec![0; 4],
                width: 2,
                height: 2,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        bus.outline_glyph_pixel(0x2002, 0, 0, 0);
        bus.write_byte(0x2002, 0);
        bus.end_outline_glyph();
        bus.block_move(0x2002, 0x1003, 1);
        assert_eq!(
            &bus.outline_presentation_rgb().unwrap().2[18..24],
            &[255; 6],
            "a logical ink pixel with zero physical coverage must stay clear after copying"
        );
        // Guest overwrites must invalidate offscreen metadata, including equal bytes.
        bus.write_byte(0x2000, 0);
        assert!(bus.presentation.as_ref().unwrap().detail(0x2000).is_none());
    }

    #[test]
    fn indexed_transfers_keep_edges_and_identical_xor_clears_them() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        let expected = bus.outline_presentation_rgb().unwrap().2;
        let saved = bus.save_pixel_bytes(0x1000, 1);
        assert!(bus.transfer_saved_pixel(0x1000, &saved, 0, |_, s, d| s | d));
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, expected);
        assert!(bus.transfer_saved_pixel(0x1000, &saved, 0, |_, s, d| s ^ d));
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[..6], &[0; 6]);
        // Transparent source background reveals destination coverage.
        bus.restore_saved_pixels(0x1000, &saved, 0, 1);
        let transparent = vec![255].into();
        assert!(
            bus.transfer_saved_pixel(0x1000, &transparent, 0, |_, s, d| if s == 255 {
                d
            } else {
                s
            })
        );
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, expected);
    }

    #[test]
    fn palette_changes_preserve_coverage_and_distinct_indexes_with_equal_colors() {
        let mut bus = bus();
        let mut palette = [[0; 3]; 256];
        palette[255] = [255; 3];
        let screen = (0x1000, 8, 8, 8, 8);
        bus.prepare_outline_presentation(screen, palette);
        let p = bus.presentation.as_mut().unwrap();
        p.glyph = Some((
            OutlineGlyph {
                pixels: vec![128, 255, 0, 0],
                width: 4,
                height: 1,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        p.glyph_pixel(0x1000, 0, 0, 0, 255);
        p.glyph.as_mut().unwrap().0.pixels[1] = 0;
        p.glyph_pixel(0x1000, 0, 0, 2, 255);
        p.glyph = None;
        // Both foreground indexes were black when drawn. Retaining only RGB
        // cannot distinguish them after the CLUT assigns different colors.
        palette[0] = [255, 0, 0];
        palette[2] = [0, 0, 255];
        palette[255] = [0, 255, 0];
        bus.prepare_outline_presentation(screen, palette);
        let (_, _, rgb, _) = bus.outline_presentation_rgb().unwrap();
        assert_eq!(&rgb[..9], &[64, 63, 128, 255, 0, 0, 0, 255, 0]);
        let original_byte = bus.read_byte(0x1000);
        bus.invert_screen_byte(0x1000);
        assert_eq!(bus.read_byte(0x1000), !original_byte);
        assert_ne!(bus.outline_presentation_rgb().unwrap().2, rgb);
        bus.invert_screen_byte(0x1000);
        assert_eq!(bus.read_byte(0x1000), original_byte);
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, rgb);
        // A same-value guest erase still discards coverage after recoloring.
        bus.write_byte(0x1000, 255);
        palette[255] = [255; 3];
        bus.prepare_outline_presentation(screen, palette);
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[..12], &[255; 12]);
    }

    #[test]
    fn default_surface_preserves_overlay_coordinates_and_invalidates_changed_modes() {
        let mut bus = bus();
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.prepare_outline_presentation((0x1000, 8, 8, 8, 8), palette);
        // The default path replaces an explicitly configured 2x surface.
        let guest = vec![0xffffffff; 64];
        let mut overlay = guest.clone();
        overlay[10] = 0xff123456;
        let (width, height, pixels) = bus.presented_argb(&guest, &overlay).unwrap();
        assert_eq!((width, height), (32, 32));
        assert_eq!(pixels[4 * 32 + 8], 0xff123456);
        assert_eq!(pixels[7 * 32 + 11], 0xff123456);
        assert_eq!(pixels[4 * 32 + 12], 0xffffffff);
        bus.prepare_outline_presentation((0x1000, 8, 8, 8, 1), palette);
        assert!(!bus.has_outline_presentation());
        bus.prepare_outline_presentation((0x1000, 8, 8, 8, 8), palette);
        assert_eq!(bus.outline_presentation_rgb().unwrap().0, 32);
    }

    #[test]
    fn clipped_coverage_blends_over_background_and_later_writes_erase_it() {
        let mut bus = bus();
        let p = bus.presentation.as_mut().unwrap();
        p.glyph = Some((
            OutlineGlyph {
                pixels: vec![128; 8],
                width: 4,
                height: 2,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        assert_eq!(p.glyph_bounds(), Some((0, 0, 1, 2)));
        // Only the first logical cell survives the caller's clipping region.
        p.glyph_pixel(0x1000, 0, 0, 0, 255);
        p.glyph_pixel(0x1000, 0, 0, 0, 255); // Repainting must not darken the edge.
        bus.write_byte(0x1000, 0); // Guest ink must not replace the blended plane.
        bus.end_outline_glyph();
        let (_, _, rgb, _) = bus.outline_presentation_rgb().unwrap();
        assert_eq!(&rgb[0..6], &[127; 6]);
        assert_eq!(&rgb[6..12], &[255; 6]);
        assert_eq!(bus.read_byte(0x1000), 0);
        bus.write_byte(0x1000, 0); // Even a same-value later write invalidates ink.
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[0..6], &[0; 6]);
    }

    #[test]
    fn text_run_overhang_survives_adjacent_erase_but_not_a_new_run() {
        let mut bus = bus();
        bus.begin_presentation_text_run(true);
        let p = bus.presentation.as_mut().unwrap();
        p.glyph = Some((
            OutlineGlyph {
                pixels: vec![128; 4],
                width: 2,
                height: 2,
                left: 2,
                top: 0,
            },
            0,
            0,
        ));
        p.glyph_pixel(0x1001, 1, 0, 0, 255);
        bus.end_outline_glyph();
        bus.presentation.as_mut().unwrap().erasing_text = true;
        bus.write_byte(0x1001, 255);
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[6..12], &[127; 6]);
        assert_eq!(bus.read_byte(0x1001), 255);
        bus.end_presentation_text_run();
        bus.begin_presentation_text_run(true);
        bus.write_byte(0x1001, 255);
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[6..12], &[255; 6]);
    }

    #[test]
    fn four_times_capture_has_four_times_the_linear_resolution() {
        let mut bus = bus();
        bus.enable_outline_presentation((0x1000, 8, 8, 8, 8), [[255; 3]; 256], 4);
        let (w, h, pixels, _) = bus.outline_presentation_rgb().unwrap();
        assert_eq!((w, h, pixels.len()), (32, 32, 32 * 32 * 3));
    }

    #[test]
    fn bulk_writes_and_overlapping_copies_update_both_planes() {
        let mut bus = bus();
        bus.write_word(0x1000, 0x1020);
        bus.write_long(0x1002, 0x30405060);
        bus.write_bytes(0x1006, &[0x70, 0x80]);
        assert!(bus.copy_ram_bytes(0x1000, 0x1001, 7));
        let expected = [0x10, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70];
        assert_eq!(bus.read_bytes(0x1000, 8), expected);
        let rgb = bus.outline_presentation_rgb().unwrap().2;
        for (i, value) in expected.iter().enumerate() {
            assert_eq!(&rgb[i * 6..i * 6 + 6], &[*value; 6]);
        }
        bus.fill_bytes_strided(0x1000, 2, 4, 42);
        bus.fill_zeros(0x1008, 8);
        bus.fill_bytes(0x1010, 8, 99);
        let rgb = bus.outline_presentation_rgb().unwrap().2;
        assert_eq!(&rgb[..6], &[42; 6]);
        assert_eq!(&rgb[16 * 2 * 3..16 * 3 * 3], &[0; 48]);
        assert_eq!(&rgb[16 * 4 * 3..16 * 5 * 3], &[99; 48]);
        assert!(bus.fast_mem_window().is_none());
    }

    #[test]
    fn native_outline_capture_preserves_logical_font_metrics() {
        use crate::quickdraw::{fonts::FONT_GENEVA, text::get_glyph};
        let mut bus = bus();
        let (glyph, data) = get_glyph(FONT_GENEVA, 9, 'a').unwrap();
        let advance = glyph.advance;
        bus.begin_outline_glyph(glyph, data, 0, 7, false, None, None);
        let p = bus.presentation.as_ref().unwrap();
        let native = &p.glyph.as_ref().unwrap().0;
        assert!(native.pixels.iter().any(|&a| a > 0 && a < 255));
        assert!(native.height > i32::from(glyph.height));
        let plain_width = native.width;
        bus.begin_outline_glyph(glyph, data, 0, 7, false, Some(2), Some((advance as i16, 1)));
        let styled = &bus.presentation.as_ref().unwrap().glyph.as_ref().unwrap().0;
        assert!(styled.width > plain_width);
        assert!(styled.top + styled.height >= 4);
        assert!(styled.pixels.iter().any(|&a| a > 0 && a < 255));
        assert_eq!(get_glyph(FONT_GENEVA, 9, 'a').unwrap().0.advance, advance);
    }
}
