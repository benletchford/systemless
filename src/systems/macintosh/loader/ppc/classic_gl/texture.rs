//! Texture objects, guest pixel unpacking and texel sampling.
//! OpenGL 1.2.1 specification, sections 3.6 (pixel rectangles) and 3.8
//! (texturing).

use super::consts::*;

pub const MAX_TEXTURE_DIMENSION: u32 = 2048;
pub const MAX_LEVELS: usize = 12;

/// Base internal format, which selects the texture environment equations
/// (OpenGL 1.2.1, table 3.15 and table 3.22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseFormat {
    Alpha,
    Luminance,
    LuminanceAlpha,
    Intensity,
    Rgb,
    Rgba,
}

impl BaseFormat {
    pub fn from_internal(internal: u32) -> Option<Self> {
        Some(match internal {
            1 | LUMINANCE | LUMINANCE4 | LUMINANCE8 | 0x8041 | 0x8042 => Self::Luminance,
            2 | LUMINANCE_ALPHA | LUMINANCE4_ALPHA4 | 0x8044 | LUMINANCE8_ALPHA8 | 0x8046
            | 0x8047 | 0x8048 => Self::LuminanceAlpha,
            ALPHA | ALPHA4 | ALPHA8 | 0x803D | 0x803E => Self::Alpha,
            INTENSITY | INTENSITY4 | INTENSITY8 | 0x804C | 0x804D => Self::Intensity,
            3 | RGB | BGR | R3_G3_B2 | RGB4 | RGB5 | RGB8 | 0x8052 | 0x8053 | 0x8054 => Self::Rgb,
            4 | RGBA | BGRA | RGBA2 | RGBA4 | RGB5_A1 | RGBA8 | 0x8059 | 0x805A | 0x805B => {
                Self::Rgba
            }
            _ => return None,
        })
    }

    pub fn gl_enum(self) -> u32 {
        match self {
            Self::Alpha => ALPHA,
            Self::Luminance => LUMINANCE,
            Self::LuminanceAlpha => LUMINANCE_ALPHA,
            Self::Intensity => INTENSITY,
            Self::Rgb => RGB,
            Self::Rgba => RGBA,
        }
    }

    /// Convert unpacked RGBA (after pixel transfer) to the stored texel.
    fn store(self, rgba: [f32; 4]) -> [f32; 4] {
        match self {
            Self::Alpha => [0.0, 0.0, 0.0, rgba[3]],
            Self::Luminance => [rgba[0], rgba[0], rgba[0], 1.0],
            Self::LuminanceAlpha => [rgba[0], rgba[0], rgba[0], rgba[3]],
            Self::Intensity => [rgba[0], rgba[0], rgba[0], rgba[0]],
            Self::Rgb => [rgba[0], rgba[1], rgba[2], 1.0],
            Self::Rgba => rgba,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TexImage {
    pub width: u32,
    pub height: u32,
    pub texels: Vec<[u8; 4]>,
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub levels: Vec<Option<TexImage>>,
    pub base_format: BaseFormat,
    pub internal_format: u32,
    pub min_filter: u32,
    pub mag_filter: u32,
    pub wrap_s: u32,
    pub wrap_t: u32,
    pub border_color: [f32; 4],
    pub priority: f32,
    pub min_lod: f32,
    pub max_lod: f32,
    pub base_level: u32,
    pub max_level: u32,
}

impl Default for Texture {
    fn default() -> Self {
        Self {
            levels: Vec::new(),
            base_format: BaseFormat::Rgba,
            internal_format: 1,
            min_filter: NEAREST_MIPMAP_LINEAR,
            mag_filter: LINEAR,
            wrap_s: REPEAT,
            wrap_t: REPEAT,
            border_color: [0.0; 4],
            priority: 1.0,
            min_lod: -1000.0,
            max_lod: 1000.0,
            base_level: 0,
            max_level: 1000,
        }
    }
}

/// Client pixel storage modes for one direction (pack or unpack).
#[derive(Debug, Clone, Copy)]
pub struct PixelStore {
    pub swap_bytes: bool,
    pub lsb_first: bool,
    pub row_length: i32,
    pub skip_rows: i32,
    pub skip_pixels: i32,
    pub alignment: i32,
}

impl Default for PixelStore {
    fn default() -> Self {
        Self {
            swap_bytes: false,
            lsb_first: false,
            row_length: 0,
            skip_rows: 0,
            skip_pixels: 0,
            alignment: 4,
        }
    }
}

/// Pixel transfer scale and bias for RGBA (OpenGL 1.2.1, section 3.6.3).
#[derive(Debug, Clone, Copy)]
pub struct PixelTransfer {
    pub scale: [f32; 4],
    pub bias: [f32; 4],
    pub depth_scale: f32,
    pub depth_bias: f32,
}

impl Default for PixelTransfer {
    fn default() -> Self {
        Self {
            scale: [1.0; 4],
            bias: [0.0; 4],
            depth_scale: 1.0,
            depth_bias: 0.0,
        }
    }
}

impl PixelTransfer {
    pub fn is_identity(&self) -> bool {
        self.scale == [1.0; 4] && self.bias == [0.0; 4]
    }
}

/// The layout of a client pixel rectangle described by `format` and `kind`.
#[derive(Debug, Clone, Copy)]
pub struct PixelLayout {
    pub components: usize,
    /// Bytes per element, or per pixel for packed types.
    pub element_bytes: usize,
    pub packed: bool,
}

impl PixelLayout {
    pub fn new(format: u32, kind: u32) -> Option<Self> {
        let components = match format {
            RED | GREEN | BLUE | ALPHA | LUMINANCE | INTENSITY | DEPTH_COMPONENT
            | STENCIL_INDEX | COLOR_INDEX => 1,
            LUMINANCE_ALPHA => 2,
            RGB | BGR => 3,
            RGBA | BGRA => 4,
            _ => return None,
        };
        let (element_bytes, packed, packed_components) = match kind {
            UNSIGNED_BYTE | BYTE => (1, false, 0),
            UNSIGNED_SHORT | SHORT => (2, false, 0),
            UNSIGNED_INT | INT | FLOAT => (4, false, 0),
            UNSIGNED_BYTE_3_3_2 | UNSIGNED_BYTE_2_3_3_REV => (1, true, 3),
            UNSIGNED_SHORT_5_6_5 | UNSIGNED_SHORT_5_6_5_REV => (2, true, 3),
            UNSIGNED_SHORT_4_4_4_4
            | UNSIGNED_SHORT_4_4_4_4_REV
            | UNSIGNED_SHORT_5_5_5_1
            | UNSIGNED_SHORT_1_5_5_5_REV => (2, true, 4),
            UNSIGNED_INT_8_8_8_8
            | UNSIGNED_INT_8_8_8_8_REV
            | UNSIGNED_INT_10_10_10_2
            | UNSIGNED_INT_2_10_10_10_REV => (4, true, 4),
            _ => return None,
        };
        if packed && packed_components != components {
            return None;
        }
        Some(Self {
            components,
            element_bytes,
            packed,
        })
    }

    pub fn group_bytes(&self) -> usize {
        if self.packed {
            self.element_bytes
        } else {
            self.components * self.element_bytes
        }
    }

    /// Byte offset of the first pixel and the stride between rows.
    pub fn row_geometry(&self, width: usize, store: &PixelStore) -> (usize, usize) {
        let group = self.group_bytes();
        let pixels_per_row = if store.row_length > 0 {
            store.row_length as usize
        } else {
            width
        };
        let alignment = store.alignment.max(1) as usize;
        let raw = pixels_per_row * group;
        let row_bytes = if self.element_bytes >= alignment || self.packed && group >= alignment {
            raw
        } else {
            raw.div_ceil(alignment) * alignment
        };
        let offset =
            store.skip_rows.max(0) as usize * row_bytes + store.skip_pixels.max(0) as usize * group;
        (offset, row_bytes)
    }

    /// Total bytes spanned by a `width` x `height` rectangle.
    pub fn span_bytes(&self, width: usize, height: usize, store: &PixelStore) -> usize {
        if width == 0 || height == 0 {
            return 0;
        }
        let (offset, row_bytes) = self.row_geometry(width, store);
        offset + (height - 1) * row_bytes + width * self.group_bytes()
    }
}

fn read_element(bytes: &[u8], kind: u32, swap: bool) -> u32 {
    match bytes.len() {
        1 => u32::from(bytes[0]),
        2 => {
            let value = u16::from_be_bytes([bytes[0], bytes[1]]);
            u32::from(if swap { value.swap_bytes() } else { value })
        }
        _ => {
            let value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let _ = kind;
            if swap {
                value.swap_bytes()
            } else {
                value
            }
        }
    }
}

fn normalize_component(raw: u32, kind: u32) -> f32 {
    match kind {
        UNSIGNED_BYTE => raw as f32 / 255.0,
        BYTE => ((raw as u8 as i8) as f32 * 2.0 + 1.0) / 255.0,
        UNSIGNED_SHORT => raw as f32 / 65535.0,
        SHORT => ((raw as u16 as i16) as f32 * 2.0 + 1.0) / 65535.0,
        UNSIGNED_INT => (f64::from(raw) / 4_294_967_295.0) as f32,
        INT => ((f64::from(raw as i32) * 2.0 + 1.0) / 4_294_967_295.0) as f32,
        FLOAT => f32::from_bits(raw),
        _ => 0.0,
    }
}

/// Unpack packed pixels to components in the format's component order.
fn unpack_packed(raw: u32, kind: u32) -> [f32; 4] {
    let field = |shift: u32, bits: u32| {
        ((raw >> shift) & ((1 << bits) - 1)) as f32 / ((1u32 << bits) - 1) as f32
    };
    match kind {
        UNSIGNED_BYTE_3_3_2 => [field(5, 3), field(2, 3), field(0, 2), 0.0],
        UNSIGNED_BYTE_2_3_3_REV => [field(0, 3), field(3, 3), field(6, 2), 0.0],
        UNSIGNED_SHORT_5_6_5 => [field(11, 5), field(5, 6), field(0, 5), 0.0],
        UNSIGNED_SHORT_5_6_5_REV => [field(0, 5), field(5, 6), field(11, 5), 0.0],
        UNSIGNED_SHORT_4_4_4_4 => [field(12, 4), field(8, 4), field(4, 4), field(0, 4)],
        UNSIGNED_SHORT_4_4_4_4_REV => [field(0, 4), field(4, 4), field(8, 4), field(12, 4)],
        UNSIGNED_SHORT_5_5_5_1 => [field(11, 5), field(6, 5), field(1, 5), field(0, 1)],
        UNSIGNED_SHORT_1_5_5_5_REV => [field(0, 5), field(5, 5), field(10, 5), field(15, 1)],
        UNSIGNED_INT_8_8_8_8 => [field(24, 8), field(16, 8), field(8, 8), field(0, 8)],
        UNSIGNED_INT_8_8_8_8_REV => [field(0, 8), field(8, 8), field(16, 8), field(24, 8)],
        UNSIGNED_INT_10_10_10_2 => [field(22, 10), field(12, 10), field(2, 10), field(0, 2)],
        UNSIGNED_INT_2_10_10_10_REV => [field(0, 10), field(10, 10), field(20, 10), field(30, 2)],
        _ => [0.0; 4],
    }
}

/// Map unpacked components to RGBA according to the client `format`.
fn components_to_rgba(format: u32, c: [f32; 4]) -> [f32; 4] {
    match format {
        RED => [c[0], 0.0, 0.0, 1.0],
        GREEN => [0.0, c[0], 0.0, 1.0],
        BLUE => [0.0, 0.0, c[0], 1.0],
        ALPHA => [0.0, 0.0, 0.0, c[0]],
        LUMINANCE => [c[0], c[0], c[0], 1.0],
        INTENSITY => [c[0], c[0], c[0], c[0]],
        LUMINANCE_ALPHA => [c[0], c[0], c[0], c[1]],
        RGB => [c[0], c[1], c[2], 1.0],
        BGR => [c[2], c[1], c[0], 1.0],
        RGBA => c,
        BGRA => [c[2], c[1], c[0], c[3]],
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

/// Decode a client pixel rectangle into RGBA floats in row order (the first
/// row is the bottom row of the image, as OpenGL addresses it).
pub fn unpack_rgba(
    bytes: &[u8],
    width: usize,
    height: usize,
    format: u32,
    kind: u32,
    store: &PixelStore,
    transfer: &PixelTransfer,
) -> Option<Vec<[f32; 4]>> {
    let layout = PixelLayout::new(format, kind)?;
    let (offset, row_bytes) = layout.row_geometry(width, store);
    let group = layout.group_bytes();
    let mut out = Vec::with_capacity(width * height);
    for row in 0..height {
        let row_start = offset + row * row_bytes;
        for column in 0..width {
            let start = row_start + column * group;
            let pixel = bytes.get(start..start + group)?;
            let components = if layout.packed {
                unpack_packed(read_element(pixel, kind, store.swap_bytes), kind)
            } else {
                let mut c = [0.0; 4];
                for (index, value) in c.iter_mut().enumerate().take(layout.components) {
                    let element =
                        &pixel[index * layout.element_bytes..(index + 1) * layout.element_bytes];
                    *value =
                        normalize_component(read_element(element, kind, store.swap_bytes), kind);
                }
                c
            };
            let mut rgba = components_to_rgba(format, components);
            if !transfer.is_identity() {
                for index in 0..4 {
                    rgba[index] = rgba[index] * transfer.scale[index] + transfer.bias[index];
                }
            }
            for value in &mut rgba {
                *value = value.clamp(0.0, 1.0);
            }
            out.push(rgba);
        }
    }
    Some(out)
}

#[inline]
pub fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

impl Texture {
    pub fn specify_level(
        &mut self,
        level: usize,
        internal_format: u32,
        base: BaseFormat,
        width: u32,
        height: u32,
        rgba: Option<Vec<[f32; 4]>>,
    ) {
        if self.levels.len() <= level {
            self.levels.resize(level + 1, None);
        }
        if level == 0 || self.levels.iter().all(Option::is_none) {
            self.base_format = base;
            self.internal_format = internal_format;
        }
        let count = width as usize * height as usize;
        let texels = match rgba {
            Some(pixels) => pixels
                .into_iter()
                .map(|p| {
                    let stored = base.store(p);
                    [
                        to_u8(stored[0]),
                        to_u8(stored[1]),
                        to_u8(stored[2]),
                        to_u8(stored[3]),
                    ]
                })
                .collect(),
            None => vec![[0, 0, 0, 0]; count],
        };
        self.levels[level] = Some(TexImage {
            width,
            height,
            texels,
        });
    }

    pub fn update_level(
        &mut self,
        level: usize,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        rgba: &[[f32; 4]],
    ) -> bool {
        let base = self.base_format;
        let Some(Some(image)) = self.levels.get_mut(level) else {
            return false;
        };
        if x + width > image.width || y + height > image.height {
            return false;
        }
        for row in 0..height {
            for column in 0..width {
                let stored = base.store(rgba[(row * width + column) as usize]);
                image.texels[((y + row) * image.width + x + column) as usize] = [
                    to_u8(stored[0]),
                    to_u8(stored[1]),
                    to_u8(stored[2]),
                    to_u8(stored[3]),
                ];
            }
        }
        true
    }

    fn uses_mipmaps(&self) -> bool {
        matches!(
            self.min_filter,
            NEAREST_MIPMAP_NEAREST
                | LINEAR_MIPMAP_NEAREST
                | NEAREST_MIPMAP_LINEAR
                | LINEAR_MIPMAP_LINEAR
        )
    }

    /// Texture completeness (OpenGL 1.2.1, section 3.8.9).
    pub fn is_complete(&self) -> bool {
        let base = self.base_level as usize;
        let Some(Some(image)) = self.levels.get(base) else {
            return false;
        };
        if image.width == 0 || image.height == 0 {
            return false;
        }
        if !self.uses_mipmaps() {
            return true;
        }
        let (mut w, mut h) = (image.width, image.height);
        let mut level = base;
        while (w > 1 || h > 1) && level < self.max_level as usize {
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            level += 1;
            match self.levels.get(level) {
                Some(Some(next)) if next.width == w && next.height == h => {}
                _ => return false,
            }
        }
        true
    }

    /// The minification filter used for sampling, or `None` when the texture
    /// has no base image. A texture whose mipmap chain is incomplete samples
    /// its base level with the non-mipmapped form of its filter rather than
    /// disabling texturing as OpenGL 1.2.1 section 3.8.9 requires: Mac OS 9
    /// titles set filters on the default texture object and upload only
    /// level 0 to their own objects, which rendered on the classic Mac
    /// OpenGL renderers.
    pub fn effective_min_filter(&self) -> Option<u32> {
        let Some(Some(image)) = self.levels.get(self.base_level as usize) else {
            return None;
        };
        if image.width == 0 || image.height == 0 {
            return None;
        }
        if self.is_complete() {
            return Some(self.min_filter);
        }
        Some(match self.min_filter {
            LINEAR_MIPMAP_NEAREST | LINEAR_MIPMAP_LINEAR => LINEAR,
            _ => NEAREST,
        })
    }

    pub fn base_size(&self) -> (u32, u32) {
        match self.levels.get(self.base_level as usize) {
            Some(Some(image)) => (image.width, image.height),
            _ => (0, 0),
        }
    }

    fn last_level(&self) -> usize {
        let mut level = self.base_level as usize;
        while level + 1 < self.levels.len()
            && level + 1 <= self.max_level as usize
            && matches!(self.levels.get(level + 1), Some(Some(_)))
        {
            level += 1;
        }
        level
    }

    /// Sample at (s, t) with level-of-detail `lambda` (log2 of the texel
    /// footprint at the base level).
    pub fn sample(&self, s: f32, t: f32, lambda: f32, min_filter: u32) -> [f32; 4] {
        let lambda = lambda.clamp(self.min_lod, self.max_lod);
        let magnify_threshold = if self.mag_filter == LINEAR
            && matches!(min_filter, NEAREST_MIPMAP_NEAREST | NEAREST_MIPMAP_LINEAR)
        {
            0.5
        } else {
            0.0
        };
        let base = self.base_level as usize;
        if lambda <= magnify_threshold {
            return self.sample_level(base, s, t, self.mag_filter == LINEAR);
        }
        match min_filter {
            NEAREST => self.sample_level(base, s, t, false),
            LINEAR => self.sample_level(base, s, t, true),
            NEAREST_MIPMAP_NEAREST | LINEAR_MIPMAP_NEAREST => {
                let last = self.last_level();
                let level = if lambda <= 0.5 {
                    base
                } else {
                    (base + (lambda + 0.5).ceil() as usize - 1).min(last)
                };
                self.sample_level(level, s, t, min_filter == LINEAR_MIPMAP_NEAREST)
            }
            _ => {
                let last = self.last_level();
                let linear = min_filter == LINEAR_MIPMAP_LINEAR;
                let floor = lambda.floor();
                let level0 = (base + floor as usize).min(last);
                let level1 = (level0 + 1).min(last);
                let a = self.sample_level(level0, s, t, linear);
                if level0 == level1 {
                    return a;
                }
                let b = self.sample_level(level1, s, t, linear);
                let f = lambda - floor;
                [
                    a[0] + (b[0] - a[0]) * f,
                    a[1] + (b[1] - a[1]) * f,
                    a[2] + (b[2] - a[2]) * f,
                    a[3] + (b[3] - a[3]) * f,
                ]
            }
        }
    }

    fn texel(&self, image: &TexImage, x: i32, y: i32) -> [f32; 4] {
        let wrap = |coordinate: i32, size: u32, mode: u32| -> i32 {
            let size = size as i32;
            if mode == REPEAT {
                if size & (size - 1) == 0 {
                    // Power-of-two sizes wrap with a mask.
                    coordinate & (size - 1)
                } else {
                    coordinate.rem_euclid(size)
                }
            } else {
                coordinate.clamp(0, size - 1)
            }
        };
        let x = wrap(x, image.width, self.wrap_s);
        let y = wrap(y, image.height, self.wrap_t);
        let texel = image.texels[(y as u32 * image.width + x as u32) as usize];
        [
            f32::from(texel[0]) / 255.0,
            f32::from(texel[1]) / 255.0,
            f32::from(texel[2]) / 255.0,
            f32::from(texel[3]) / 255.0,
        ]
    }

    fn sample_level(&self, level: usize, s: f32, t: f32, linear: bool) -> [f32; 4] {
        let Some(Some(image)) = self.levels.get(level) else {
            return [0.0, 0.0, 0.0, 1.0];
        };
        // GL_CLAMP samples clamp to the edge texel. Texture borders are not
        // supported, so this matches CLAMP_TO_EDGE as the Mac hardware
        // renderers of the period implemented it.
        let clamp = |coordinate: f32, mode: u32| {
            if mode == REPEAT {
                coordinate
            } else {
                coordinate.clamp(0.0, 1.0)
            }
        };
        let u = clamp(s, self.wrap_s) * image.width as f32;
        let v = clamp(t, self.wrap_t) * image.height as f32;
        if !linear {
            let x = u.floor() as i32;
            let y = v.floor() as i32;
            return self.texel(image, x, y);
        }
        let u = u - 0.5;
        let v = v - 0.5;
        let x0 = u.floor();
        let y0 = v.floor();
        let fx = u - x0;
        let fy = v - y0;
        let (x0, y0) = (x0 as i32, y0 as i32);
        let a = self.texel(image, x0, y0);
        let b = self.texel(image, x0 + 1, y0);
        let c = self.texel(image, x0, y0 + 1);
        let d = self.texel(image, x0 + 1, y0 + 1);
        let mut out = [0.0; 4];
        for index in 0..4 {
            let top = a[index] + (b[index] - a[index]) * fx;
            let bottom = c[index] + (d[index] - c[index]) * fx;
            out[index] = top + (bottom - top) * fy;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpacks_aligned_rgb_rows_bottom_up() {
        // 1x2 RGB image with 4-byte row alignment: rows are 3 bytes + 1 pad.
        let bytes = [255, 0, 0, 0, 0, 255, 0, 0];
        let pixels = unpack_rgba(
            &bytes,
            1,
            2,
            RGB,
            UNSIGNED_BYTE,
            &PixelStore::default(),
            &PixelTransfer::default(),
        )
        .unwrap();
        assert_eq!(pixels, vec![[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]]);
    }

    #[test]
    fn unpacks_big_endian_packed_argb1555() {
        // APPLE_packed_pixels BGRA + 1_5_5_5_REV: bit 15 alpha, red 10-14.
        let bytes = 0xFC00u16.to_be_bytes();
        let pixels = unpack_rgba(
            &bytes,
            1,
            1,
            BGRA,
            UNSIGNED_SHORT_1_5_5_5_REV,
            &PixelStore::default(),
            &PixelTransfer::default(),
        )
        .unwrap();
        assert_eq!(pixels, vec![[1.0, 0.0, 0.0, 1.0]]);
    }

    #[test]
    fn luminance_alpha_textures_store_luminance_in_rgb() {
        let mut texture = Texture::default();
        texture.min_filter = NEAREST;
        texture.specify_level(
            0,
            LUMINANCE_ALPHA,
            BaseFormat::LuminanceAlpha,
            1,
            1,
            Some(vec![[0.5, 0.0, 0.0, 0.25]]),
        );
        assert!(texture.is_complete());
        let texel = texture.sample(0.5, 0.5, 0.0, texture.min_filter);
        assert!((texel[1] - 0.5).abs() < 0.01 && (texel[3] - 0.25).abs() < 0.01);
    }

    #[test]
    fn mipmapped_texture_requires_every_level() {
        let mut texture = Texture::default();
        assert_eq!(texture.effective_min_filter(), None);
        texture.specify_level(0, RGBA, BaseFormat::Rgba, 2, 2, None);
        assert!(!texture.is_complete());
        // The incomplete chain falls back to sampling the base level.
        assert_eq!(texture.effective_min_filter(), Some(NEAREST));
        texture.min_filter = LINEAR_MIPMAP_LINEAR;
        assert_eq!(texture.effective_min_filter(), Some(LINEAR));
        texture.specify_level(1, RGBA, BaseFormat::Rgba, 1, 1, None);
        assert!(texture.is_complete());
        assert_eq!(texture.effective_min_filter(), Some(LINEAR_MIPMAP_LINEAR));
    }
}
