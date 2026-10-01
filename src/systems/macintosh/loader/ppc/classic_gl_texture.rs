//! OpenGL 1.2.1 two-dimensional texture objects and guest pixel unpacking.
//! https://registry.khronos.org/OpenGL/specs/gl/glspec121.pdf

use super::PpcSectionMem;
use ppc::PpcMemory;

const MAX_TEXTURE_PIXELS: usize = 4096 * 4096;

#[derive(Debug, Clone, Copy)]
pub struct ClassicGlPixelUnpack {
    pub alignment: u32,
    pub row_length: u32,
    pub skip_rows: u32,
    pub skip_pixels: u32,
}

impl Default for ClassicGlPixelUnpack {
    fn default() -> Self {
        Self {
            alignment: 4,
            row_length: 0,
            skip_rows: 0,
            skip_pixels: 0,
        }
    }
}

impl ClassicGlPixelUnpack {
    pub fn set(&mut self, name: u32, value: i32) -> bool {
        if value < 0 {
            return false;
        }
        let value = value as u32;
        match name {
            0x0cf2 => self.row_length = value,  // UNPACK_ROW_LENGTH
            0x0cf3 => self.skip_rows = value,   // UNPACK_SKIP_ROWS
            0x0cf4 => self.skip_pixels = value, // UNPACK_SKIP_PIXELS
            0x0cf5 if matches!(value, 1 | 2 | 4 | 8) => self.alignment = value,
            _ => return false,
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct ClassicGlTextureImage {
    pub width: u32,
    pub height: u32,
    pub internal_format: u32,
    pixels: Vec<[u8; 4]>,
}

impl ClassicGlTextureImage {
    pub fn new(width: u32, height: u32, internal_format: u32) -> Option<Self> {
        if width == 0 || height == 0 || !width.is_power_of_two() || !height.is_power_of_two() {
            return None;
        }
        let count = usize::try_from(width.checked_mul(height)?).ok()?;
        if count > MAX_TEXTURE_PIXELS {
            return None;
        }
        Some(Self {
            width,
            height,
            internal_format,
            pixels: vec![[0, 0, 0, 0]; count],
        })
    }

    pub fn texel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels
            .get(y as usize * self.width as usize + x as usize)
            .copied()
    }

    pub fn upload_sub_image(
        &mut self,
        memory: &mut PpcSectionMem,
        origin: (u32, u32),
        size: (u32, u32),
        format: u32,
        pixel_type: u32,
        source: u32,
        unpack: ClassicGlPixelUnpack,
    ) -> bool {
        let (x_offset, y_offset) = origin;
        let (width, height) = size;
        if x_offset
            .checked_add(width)
            .is_none_or(|right| right > self.width)
            || y_offset
                .checked_add(height)
                .is_none_or(|top| top > self.height)
            || pixel_type != 0x1401
        // GL_UNSIGNED_BYTE
        {
            return false;
        }
        let components: u32 = match format {
            0x1906 | 0x1909 => 1, // ALPHA, LUMINANCE
            0x190a => 2,          // LUMINANCE_ALPHA
            0x1907 | 0x80e0 => 3, // RGB, BGR
            0x1908 | 0x80e1 => 4, // RGBA, BGRA
            _ => return false,
        };
        let row_length = if unpack.row_length == 0 {
            width
        } else {
            unpack.row_length
        };
        let Some(stride) = row_length
            .checked_mul(components)
            .and_then(|bytes| bytes.checked_add(unpack.alignment - 1))
            .map(|bytes| bytes & !(unpack.alignment - 1))
        else {
            return false;
        };
        let Some(start) = unpack
            .skip_rows
            .checked_mul(stride)
            .and_then(|offset| {
                unpack
                    .skip_pixels
                    .checked_mul(components)
                    .and_then(|pixels| offset.checked_add(pixels))
            })
            .and_then(|offset| source.checked_add(offset))
        else {
            return false;
        };
        let mut decoded = Vec::with_capacity((width as usize) * (height as usize));
        for row in 0..height {
            let Some(row_address) = row
                .checked_mul(stride)
                .and_then(|offset| start.checked_add(offset))
            else {
                return false;
            };
            for column in 0..width {
                let Some(address) = column
                    .checked_mul(components)
                    .and_then(|offset| row_address.checked_add(offset))
                else {
                    return false;
                };
                let read = |offset: u32, memory: &mut PpcSectionMem| {
                    address
                        .checked_add(offset)
                        .and_then(|address| memory.read_u8(address))
                };
                let rgba = match format {
                    0x1906 => {
                        let Some(a) = read(0, memory) else {
                            return false;
                        };
                        [0, 0, 0, a]
                    }
                    0x1909 => {
                        let Some(l) = read(0, memory) else {
                            return false;
                        };
                        [l, l, l, 255]
                    }
                    0x190a => {
                        let (Some(l), Some(a)) = (read(0, memory), read(1, memory)) else {
                            return false;
                        };
                        [l, l, l, a]
                    }
                    0x1907 => {
                        let (Some(r), Some(g), Some(b)) =
                            (read(0, memory), read(1, memory), read(2, memory))
                        else {
                            return false;
                        };
                        [r, g, b, 255]
                    }
                    0x80e0 => {
                        let (Some(b), Some(g), Some(r)) =
                            (read(0, memory), read(1, memory), read(2, memory))
                        else {
                            return false;
                        };
                        [r, g, b, 255]
                    }
                    0x1908 => {
                        let (Some(r), Some(g), Some(b), Some(a)) = (
                            read(0, memory),
                            read(1, memory),
                            read(2, memory),
                            read(3, memory),
                        ) else {
                            return false;
                        };
                        [r, g, b, a]
                    }
                    0x80e1 => {
                        let (Some(b), Some(g), Some(r), Some(a)) = (
                            read(0, memory),
                            read(1, memory),
                            read(2, memory),
                            read(3, memory),
                        ) else {
                            return false;
                        };
                        [r, g, b, a]
                    }
                    _ => unreachable!(),
                };
                decoded.push(rgba);
            }
        }
        for row in 0..height {
            let target = ((y_offset + row) * self.width + x_offset) as usize;
            let source = (row * width) as usize;
            self.pixels[target..target + width as usize]
                .copy_from_slice(&decoded[source..source + width as usize]);
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct ClassicGlTexture2D {
    pub name: u32,
    pub images: Vec<Option<ClassicGlTextureImage>>,
    pub min_filter: u32,
    pub mag_filter: u32,
    pub wrap_s: u32,
    pub wrap_t: u32,
}

impl ClassicGlTexture2D {
    fn new(name: u32) -> Self {
        Self {
            name,
            images: vec![None; 13],
            min_filter: 0x2702, // NEAREST_MIPMAP_LINEAR
            mag_filter: 0x2601, // LINEAR
            wrap_s: 0x2901,     // REPEAT
            wrap_t: 0x2901,
        }
    }

    pub fn set_parameter(&mut self, name: u32, value: u32) -> bool {
        match name {
            0x2801 if matches!(value, 0x2600 | 0x2601 | 0x2700..=0x2703) => self.min_filter = value,
            0x2800 if matches!(value, 0x2600 | 0x2601) => self.mag_filter = value,
            0x2802 if matches!(value, 0x2900 | 0x2901 | 0x812f) => self.wrap_s = value,
            0x2803 if matches!(value, 0x2900 | 0x2901 | 0x812f) => self.wrap_t = value,
            _ => return false,
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct ClassicGlTextures {
    next_name: u32,
    reserved: Vec<u32>,
    objects: Vec<ClassicGlTexture2D>,
    pub bound_2d: u32,
    pub unpack: ClassicGlPixelUnpack,
}

impl Default for ClassicGlTextures {
    fn default() -> Self {
        Self {
            next_name: 1,
            reserved: Vec::new(),
            objects: vec![ClassicGlTexture2D::new(0)],
            bound_2d: 0,
            unpack: ClassicGlPixelUnpack::default(),
        }
    }
}

impl ClassicGlTextures {
    pub fn reserve_names(&mut self, count: u32) -> Option<Vec<u32>> {
        if count > 1_000_000 {
            return None;
        }
        let next = self.next_name.checked_add(count)?;
        let names = (self.next_name..next).collect::<Vec<_>>();
        self.next_name = next;
        self.reserved.extend_from_slice(&names);
        Some(names)
    }

    pub fn bind_2d(&mut self, name: u32) {
        if name != 0 && !self.objects.iter().any(|object| object.name == name) {
            self.objects.push(ClassicGlTexture2D::new(name));
            self.reserved.retain(|reserved| *reserved != name);
            self.next_name = self.next_name.max(name.saturating_add(1));
        }
        self.bound_2d = name;
    }

    pub fn delete(&mut self, names: &[u32]) {
        for &name in names {
            if name == 0 {
                continue;
            }
            self.reserved.retain(|reserved| *reserved != name);
            self.objects.retain(|object| object.name != name);
            if self.bound_2d == name {
                self.bound_2d = 0;
            }
        }
    }

    pub fn bound_mut(&mut self) -> Option<&mut ClassicGlTexture2D> {
        self.objects
            .iter_mut()
            .find(|object| object.name == self.bound_2d)
    }

    pub fn bound(&self) -> Option<&ClassicGlTexture2D> {
        self.objects
            .iter()
            .find(|object| object.name == self.bound_2d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_alignment_and_rows_preserve_bottom_up_texels() {
        let mut memory = PpcSectionMem::new();
        memory.add_region(
            0x1000,
            vec![
                255, 0, 0, 0, 255, 0, 0, 0, // bottom row, RGB with 2-byte padding
                0, 0, 255, 255, 255, 255, 0, 0, // top row
            ],
        );
        let mut image = ClassicGlTextureImage::new(2, 2, 0x1907).unwrap();
        assert!(image.upload_sub_image(
            &mut memory,
            (0, 0),
            (2, 2),
            0x1907,
            0x1401,
            0x1000,
            ClassicGlPixelUnpack::default()
        ));
        assert_eq!(image.texel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(image.texel(1, 0), Some([0, 255, 0, 255]));
        assert_eq!(image.texel(0, 1), Some([0, 0, 255, 255]));
        assert_eq!(image.texel(1, 1), Some([255, 255, 255, 255]));
    }

    #[test]
    fn generated_names_bind_and_delete_without_reusing_live_objects() {
        let mut textures = ClassicGlTextures::default();
        let names = textures.reserve_names(2).unwrap();
        assert_eq!(names, vec![1, 2]);
        textures.bind_2d(1);
        assert!(textures.bound().is_some());
        textures.delete(&[1]);
        assert_eq!(textures.bound_2d, 0);
        assert_eq!(textures.reserve_names(1), Some(vec![3]));
    }
}
