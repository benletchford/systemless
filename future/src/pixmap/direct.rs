//! Live direct PixMaps store unpacked pixels, unlike serialized DirectBits PICT rows.
use super::*;

pub(super) fn layout(
    memory: &Memory,
    address: GuestPtr,
) -> Result<(Bitmap, crate::graphics::DirectFormat)> {
    let data = memory.read(address, 50)?;
    let word = |at| u16::from_be_bytes(data[at..at + 2].try_into().unwrap());
    let long = |at| u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
    let depth = word(32);
    if word(4) & 0xc000 != 0x8000
        || !matches!(word(14), 0 | 4)
        || word(16) > 1
        || long(18) != 0
        || word(30) != 16
        || !matches!(depth, 16 | 32)
        || word(34) != 3
        || word(36) != if depth == 16 { 5 } else { 8 }
        || long(38) != 0
        || long(46) != 0
    {
        return Err(Error::Graphics("unsupported direct PixMap layout".into()));
    }
    let bitmap = Bitmap {
        base: GuestPtr(long(0)),
        row_bytes: usize::from(word(4) & 0x3fff),
        bounds: Rect::new(
            word(8) as i16 as i32,
            word(6) as i16 as i32,
            word(12) as i16 as i32,
            word(10) as i16 as i32,
        ),
    };
    let (width, _) = bitmap.extent()?;
    let stride = usize::from(depth / 8);
    let length = bitmap.byte_len()?;
    if width * stride > bitmap.row_bytes
        || !bitmap.row_bytes.is_multiple_of(stride)
        || length > crate::load::IMAGE_LIMIT
    {
        return Err(Error::Graphics("invalid direct PixMap storage".into()));
    }
    let format = crate::graphics::DirectFormat::from_depth(depth).unwrap();
    Ok((bitmap, format))
}

pub(super) fn source(memory: &Memory, address: GuestPtr) -> Result<Source> {
    let (bitmap, format) = layout(memory, address)?;
    let (width, height) = bitmap.extent()?;
    let depth = format.depth();
    let stride = usize::from(depth / 8);
    let bytes = memory.read(bitmap.base, bitmap.byte_len()?)?;
    let mut rgb = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let at = y * bitmap.row_bytes + x * stride;
            let color = if depth == 16 {
                let pixel = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
                crate::graphics::rgb555(pixel)
            } else {
                [bytes[at + 1], bytes[at + 2], bytes[at + 3]].map(|v| u16::from(v) * 257)
            };
            rgb.push(color);
        }
    }
    // Direct pixels ignore pmTable; it is not an indexed palette authority.
    Ok(Source {
        bitmap,
        indices: Vec::new(),
        rgb: Some(rgb),
        palette: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Permissions;

    fn fixture(depth: u16) -> Memory {
        let mut memory = Memory::default();
        let stride = usize::from(depth / 8);
        let row = stride * 32 + 4;
        let mut header = vec![0; 50];
        header[..4].copy_from_slice(&0x2001u32.to_be_bytes());
        header[4..6].copy_from_slice(&(0x8000 | row as u16).to_be_bytes());
        for (at, v) in [(6, -3i16), (8, -7), (10, -2), (12, 25)] {
            header[at..at + 2].copy_from_slice(&v.to_be_bytes());
        }
        for (at, v) in [
            (30, 16u16),
            (32, depth),
            (34, 3),
            (36, if depth == 16 { 5 } else { 8 }),
        ] {
            header[at..at + 2].copy_from_slice(&v.to_be_bytes());
        }
        // Deliberately invalid table address: direct pixels do not dereference it.
        header[42..46].copy_from_slice(&0xfffffffdu32.to_be_bytes());
        memory
            .map(GuestPtr(0x1000), header, Permissions::RW)
            .unwrap();
        let mut pixels = vec![0xa5; row];
        for x in 0..32 {
            if depth == 16 {
                let v = 0x8000 | ((x as u16) << 10) | (((31 - x) as u16) << 5) | x as u16;
                pixels[x * 2..x * 2 + 2].copy_from_slice(&v.to_be_bytes());
            } else {
                pixels[x * 4..x * 4 + 4].copy_from_slice(&[0xa5, x as u8, 255 - x as u8, 127]);
            }
        }
        memory
            .map(GuestPtr(0x2001), pixels, Permissions::RX)
            .unwrap();
        memory
    }

    #[test]
    fn live_direct_sources_decode_components_without_table_or_padding_authority() {
        for depth in [16, 32] {
            let mut memory = fixture(depth);
            for version in [0u16, 4] {
                memory
                    .write(GuestPtr(0x100e), &version.to_be_bytes())
                    .unwrap();
                let before = memory
                    .read(GuestPtr(0x2001), usize::from(depth / 8) * 32 + 4)
                    .unwrap()
                    .to_vec();
                let decoded = crate::pixmap::copy_source(&memory, GuestPtr(0x1000)).unwrap();
                assert!(decoded.indices.is_empty());
                assert!(decoded.palette.is_none());
                let colors = decoded.rgb.unwrap();
                if version == 0 {
                    memory
                        .map(
                            GuestPtr(0x3000),
                            0x1000u32.to_be_bytes().to_vec(),
                            Permissions::RX,
                        )
                        .unwrap();
                    let mut alias = vec![0; 14];
                    alias[..4].copy_from_slice(&0x3000u32.to_be_bytes());
                    alias[4..6].copy_from_slice(&0xc000u16.to_be_bytes());
                    memory
                        .map(GuestPtr(0x3100), alias, Permissions::RX)
                        .unwrap();
                }
                assert_eq!(
                    crate::pixmap::copy_source(&memory, GuestPtr(0x3100))
                        .unwrap()
                        .rgb
                        .as_ref()
                        .unwrap(),
                    &colors
                );
                for (x, c) in colors.into_iter().enumerate() {
                    let expected = if depth == 16 {
                        let expand = |v: usize| ((v << 11) | (v << 6) | (v << 1) | (v >> 4)) as u16;
                        [expand(x), expand(31 - x), expand(x)]
                    } else {
                        [x as u16 * 257, (255 - x as u16) * 257, 127 * 257]
                    };
                    assert_eq!(c, expected);
                }
                assert_eq!(memory.read(GuestPtr(0x2001), before.len()).unwrap(), before);
            }
        }
    }

    #[test]
    fn direct_metadata_defaults_and_full_width_colors_do_not_depend_on_a_palette() {
        for depth in [16, 32] {
            let memory = fixture(depth);
            let surface = crate::pixmap::destination(&memory, GuestPtr(0x1000)).unwrap();
            let (pixel, mask, rgb) = if depth == 16 {
                (0x0911, 0x7fff, [4228, 16912, 35939])
            } else {
                (0x124589, 0xffffff, [0x1212, 0x4545, 0x8989])
            };
            assert_eq!(surface.nearest([0x1234, 0x4567, 0x89ab]).unwrap(), pixel);
            assert_eq!(surface.rgb(pixel).unwrap(), rgb);
            assert_eq!(surface.index(mask).unwrap(), mask);
            assert!(surface.index(mask + 1).is_err());
            let mut metadata = Memory::default();
            metadata
                .map(
                    GuestPtr(0x1000),
                    memory.read(GuestPtr(0x1000), 50).unwrap().to_vec(),
                    Permissions::RX,
                )
                .unwrap();
            assert_eq!(
                crate::pixmap::default_colors(&metadata, GuestPtr(0x1000)).unwrap(),
                (0, mask)
            );
            assert!(crate::pixmap::destination(&metadata, GuestPtr(0x1000)).is_err());
            assert!(crate::pixmap::copy_source(&metadata, GuestPtr(0x1000)).is_err());
        }
    }

    #[test]
    fn malformed_direct_metadata_and_truncated_backing_are_rejected() {
        for depth in [16, 32] {
            for (offset, value) in [
                (4, 0xc008u32),
                (14, 1),
                (16, 2),
                (30, 0),
                (32, 24),
                (34, 4),
                (36, 6),
                (38, 1),
                (46, 1),
            ] {
                let mut memory = fixture(depth);
                memory
                    .write(GuestPtr(0x1000 + offset), &(value as u16).to_be_bytes())
                    .unwrap();
                assert!(
                    crate::pixmap::copy_source(&memory, GuestPtr(0x1000)).is_err(),
                    "depth {depth} field {offset}"
                );
            }
            let mut memory = fixture(depth);
            memory.write(GuestPtr(0x1012), &1u32.to_be_bytes()).unwrap();
            assert!(crate::pixmap::copy_source(&memory, GuestPtr(0x1000)).is_err());
            let mut memory = fixture(depth);
            memory
                .write(GuestPtr(0x1000), &0x2100u32.to_be_bytes())
                .unwrap();
            assert!(crate::pixmap::copy_source(&memory, GuestPtr(0x1000)).is_err());
        }
    }
}
