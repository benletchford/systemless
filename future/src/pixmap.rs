//! Checked live indexed and direct PixMap sources for the shared CopyBits path.
//! IM: Imaging With QuickDraw, Color QuickDraw PixMap and ColorTable records.
use crate::{
    bitmap::Bitmap,
    graphics::Rect,
    memory::{GuestPtr, Memory},
    Error, Result,
};
mod direct;

pub(crate) struct Source {
    pub bitmap: Bitmap,
    pub indices: Vec<u8>,
    pub rgb: Option<Vec<[u16; 3]>>,
    pub palette: Option<Box<[[u16; 3]; 256]>>,
}
impl Source {
    pub fn colors(&self) -> std::borrow::Cow<'_, [[u16; 3]]> {
        if let Some(rgb) = &self.rgb {
            std::borrow::Cow::Borrowed(rgb)
        } else {
            std::borrow::Cow::Owned(
                self.indices
                    .iter()
                    .map(|&i| {
                        self.palette
                            .as_ref()
                            .map_or(if i == 0 { [65535; 3] } else { [0; 3] }, |p| p[i as usize])
                    })
                    .collect(),
            )
        }
    }
}

fn bad() -> Error {
    Error::Graphics("unsupported or invalid indexed PixMap source".into())
}
/// CopyBits accepts a BitMap-shaped CGrafPort alias: its rowBytes position is
/// portVersion (high two bits set), and baseAddr is actually a PixMapHandle.
/// IM QuickDraw Drawing, CopyBits: https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-166.html
fn descriptor(memory: &Memory, address: GuestPtr) -> Result<GuestPtr> {
    if address.0 == 0 {
        return Err(bad());
    }
    let header = memory.read(address, 14)?;
    let row = u16::from_be_bytes(header[4..6].try_into().unwrap());
    if row & 0xc000 != 0xc000 {
        return Ok(address);
    }
    let handle = u32::from_be_bytes(header[..4].try_into().unwrap());
    if handle == 0 {
        return Err(Error::Graphics("nil color-port PixMap handle".into()));
    }
    let data = GuestPtr(memory.u32(GuestPtr(handle))?);
    if data.0 == 0 {
        return Err(Error::Graphics("nil color-port PixMap data".into()));
    }
    let map = memory.read(data, 50)?;
    if u16::from_be_bytes(map[4..6].try_into().unwrap()) & 0xc000 != 0x8000 {
        return Err(Error::Graphics(
            "color-port alias must reference an actual PixMap".into(),
        ));
    }
    Ok(data)
}
pub(crate) fn copy_source(memory: &Memory, address: GuestPtr) -> Result<Source> {
    source(memory, descriptor(memory, address)?)
}
pub(crate) fn copy_destination(
    memory: &Memory,
    address: GuestPtr,
) -> Result<crate::surface::Surface> {
    destination(memory, descriptor(memory, address)?)
}
fn source(memory: &Memory, address: GuestPtr) -> Result<Source> {
    let header = memory.read(address, 14)?;
    let row = u16::from_be_bytes(header[4..6].try_into().unwrap());
    if row & 0x8000 == 0 {
        let bitmap = Bitmap::read(memory, address)?;
        let surface = bitmap.surface(memory)?;
        let indices = surface.unpack(memory)?;
        return Ok(Source {
            bitmap,
            indices,
            rgb: None,
            palette: surface.colors.map(|c| c.rgb),
        });
    }
    if memory.u16(GuestPtr(address.0.checked_add(30).ok_or_else(bad)?))? == 16 {
        return direct::source(memory, address);
    }
    indexed_source(memory, indexed(memory, address)?)
}
/// CIcon images are owned by iconData; iconPMap.baseAddr is not the image owner.
pub(crate) fn icon_source(memory: &Memory, address: GuestPtr, image: GuestPtr) -> Result<Source> {
    indexed_source(memory, indexed_with_base(memory, address, Some(image))?)
}
fn indexed_source(memory: &Memory, map: Indexed) -> Result<Source> {
    let Indexed {
        bitmap,
        depth,
        palette,
        defined,
        ..
    } = map;
    let indices = crate::surface::unpack_pixels(memory, bitmap, depth)?;
    if indices.iter().any(|v| !defined[*v as usize]) {
        return Err(bad());
    }
    Ok(Source {
        bitmap,
        indices,
        rgb: None,
        palette: Some(palette),
    })
}

struct Indexed {
    bitmap: Bitmap,
    depth: u16,
    palette: Box<[[u16; 3]; 256]>,
    defined: [bool; 256],
    matchable: [bool; 256],
}
fn indexed(memory: &Memory, address: GuestPtr) -> Result<Indexed> {
    indexed_with_base(memory, address, None)
}
fn indexed_with_base(
    memory: &Memory,
    address: GuestPtr,
    base: Option<GuestPtr>,
) -> Result<Indexed> {
    let row = memory.u16(GuestPtr(address.0.checked_add(4).ok_or_else(bad)?))?;
    if row & 0xc000 != 0x8000 {
        return Err(bad());
    }
    let bytes = memory.read(address, 50)?;
    let word = |at| u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap());
    let long = |at| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    let depth = word(32);
    if word(14) != 0
        || word(16) > 1
        || word(30) != 0
        || ![1, 2, 4, 8].contains(&depth)
        || word(34) != 1
        || word(36) != depth
        || long(38) != 0
        || long(46) != 0
    {
        return Err(bad());
    }
    let bitmap = Bitmap {
        base: base.unwrap_or(GuestPtr(long(0))),
        row_bytes: usize::from(row & 0x3fff),
        bounds: Rect::new(
            word(8) as i16 as i32,
            word(6) as i16 as i32,
            word(12) as i16 as i32,
            word(10) as i16 as i32,
        ),
    };
    let (width, height) = bitmap.extent()?;
    let _count = width
        .checked_mul(height)
        .filter(|n| *n <= crate::load::IMAGE_LIMIT)
        .ok_or_else(bad)?;
    let raw_len = bitmap.byte_len()?;
    if width * usize::from(depth) > bitmap.row_bytes * 8 || raw_len > crate::load::IMAGE_LIMIT {
        return Err(bad());
    }
    let table_handle = long(42);
    if table_handle == 0 {
        return Err(bad());
    }
    let table = GuestPtr(memory.u32(GuestPtr(table_handle))?);
    if table.0 == 0 {
        return Err(bad());
    }
    let head = memory.read(table, 8)?;
    let flags = u16::from_be_bytes(head[4..6].try_into().unwrap());
    if flags & 0x4000 != 0 {
        return Err(Error::Graphics(
            "palette-relative color tables are unsupported".into(),
        ));
    }
    let entries = usize::from(u16::from_be_bytes(head[6..8].try_into().unwrap())) + 1;
    if entries > 256 {
        return Err(bad());
    }
    let colors = memory.read(table, 8 + entries * 8)?;
    let mut palette = Box::new([[65535; 3]; 256]);
    let mut defined = [false; 256];
    let mut matchable = [false; 256];
    for ordinal in 0..entries {
        let at = 8 + ordinal * 8;
        let value = usize::from(u16::from_be_bytes(colors[at..at + 2].try_into().unwrap()));
        let index = if flags & 0x8000 != 0 { ordinal } else { value };
        if index >= 256 || defined[index] {
            return Err(bad());
        }
        defined[index] = true;
        matchable[index] = if flags & 0x8000 != 0
            && value & usize::from(crate::color::RESERVED) != 0
        {
            let selected = memory.u32(crate::devices::CURRENT)?;
            if selected == 0 {
                return Err(bad());
            }
            let gd = memory.u32(GuestPtr(selected))?;
            if gd == 0 {
                return Err(bad());
            }
            memory.u16(GuestPtr(gd.checked_add(2).ok_or_else(bad)?))? & 255 == value as u16 & 255
        } else {
            true
        };
        palette[index] = std::array::from_fn(|i| {
            u16::from_be_bytes([colors[at + 2 + i * 2], colors[at + 3 + i * 2]])
        });
    }
    Ok(Indexed {
        bitmap,
        depth,
        palette,
        defined,
        matchable,
    })
}
/// Port initialization selects colors without interpreting or changing pixel bytes.
pub(crate) fn default_colors(memory: &Memory, address: GuestPtr) -> Result<(u32, u32)> {
    if memory.u16(GuestPtr(address.0.checked_add(30).ok_or_else(bad)?))? == 16 {
        let (_, format) = direct::layout(memory, address)?;
        return Ok((0, format.mask()));
    }
    let Indexed {
        depth,
        palette,
        matchable,
        ..
    } = indexed(memory, address)?;
    let nearest = |rgb| {
        crate::color::nearest_color(
            rgb,
            (0..1usize << depth)
                .filter(|i| matchable[*i])
                .map(|i| (i as u8, palette[i])),
        )
        .map(u32::from)
        .ok_or_else(bad)
    };
    Ok((nearest([0; 3])?, nearest([65535; 3])?))
}
/// Resolve PixMap matching metadata separately from live pixel validation.
pub(crate) fn matching_surface(
    memory: &Memory,
    address: GuestPtr,
) -> Result<crate::surface::Surface> {
    if memory.u16(GuestPtr(address.0.checked_add(4).ok_or_else(bad)?))? & 0x8000 == 0 {
        return Bitmap::read(memory, address)?.surface(memory);
    }
    if memory.u16(GuestPtr(address.0.checked_add(30).ok_or_else(bad)?))? == 16 {
        let (bitmap, format) = direct::layout(memory, address)?;
        return crate::surface::Surface::direct(bitmap, format);
    }
    let Indexed {
        bitmap,
        depth,
        palette,
        defined,
        matchable,
    } = indexed(memory, address)?;
    if depth == 1 && matchable != defined {
        return Err(Error::Graphics(
            "reserved monochrome matching is unsupported".into(),
        ));
    }
    let mut surface = crate::surface::Surface::indexed(bitmap, depth, palette, defined)?;
    if let Some(colors) = &mut surface.colors {
        colors.matchable = matchable;
    }
    Ok(surface)
}
/// Drawing/source access also validates the attached packed image bytes.
pub(crate) fn destination(memory: &Memory, address: GuestPtr) -> Result<crate::surface::Surface> {
    let surface = matching_surface(memory, address)?;
    if surface.depth > 8 {
        memory.read(surface.base, surface.byte_len()?)?;
    } else {
        surface.unpack(memory)?;
    }
    Ok(surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Permissions;
    fn fixture(depth: u16, flags: u16) -> (Memory, Vec<u8>) {
        let mut memory = Memory::default();
        let stride = (5 * usize::from(depth)).div_ceil(8) + 1;
        let mut header = [0u8; 50];
        header[..4].copy_from_slice(&0x3000u32.to_be_bytes());
        header[4..6].copy_from_slice(&(0x8000 | stride as u16).to_be_bytes());
        for (i, v) in [-2i16, -3, 0, 2].into_iter().enumerate() {
            header[6 + i * 2..8 + i * 2].copy_from_slice(&v.to_be_bytes());
        }
        for (at, v) in [(32, depth), (34, 1), (36, depth)] {
            header[at..at + 2].copy_from_slice(&v.to_be_bytes());
        }
        header[42..46].copy_from_slice(&0x2000u32.to_be_bytes());
        memory
            .map(GuestPtr(0x1000), header.to_vec(), Permissions::RW)
            .unwrap();
        memory
            .map(
                GuestPtr(0x2000),
                0x2100u32.to_be_bytes().to_vec(),
                Permissions::RX,
            )
            .unwrap();
        let count = 1usize << depth;
        let mut table = vec![0; 8 + count * 8];
        table[4..6].copy_from_slice(&flags.to_be_bytes());
        table[6..8].copy_from_slice(&((count - 1) as u16).to_be_bytes());
        for i in 0..count {
            let index = if flags & 0x8000 == 0 {
                count - 1 - i
            } else {
                i
            };
            let at = 8 + i * 8;
            table[at..at + 2].copy_from_slice(&(index as u16).to_be_bytes());
            for (n, v) in [index as u16 * 257, 65535 - index as u16 * 257, 0x1234]
                .into_iter()
                .enumerate()
            {
                table[at + 2 + n * 2..at + 4 + n * 2].copy_from_slice(&v.to_be_bytes());
            }
        }
        memory
            .map(GuestPtr(0x2100), table, Permissions::RW)
            .unwrap();
        let expected: Vec<u8> = (0..10).map(|i| ((i * 3) % count) as u8).collect();
        let mut data = vec![0; stride * 2];
        for y in 0..2 {
            for x in 0..5 {
                let bit = x * usize::from(depth);
                data[y * stride + bit / 8] |=
                    expected[y * 5 + x] << (8 - usize::from(depth) - bit % 8);
            }
            data[(y + 1) * stride - 1] = 0xff;
        }
        memory.map(GuestPtr(0x3000), data, Permissions::RX).unwrap();
        (memory, expected)
    }
    fn alias(memory: &mut Memory, version: u16) -> GuestPtr {
        memory
            .map(
                GuestPtr(0x4000),
                0x1000u32.to_be_bytes().to_vec(),
                Permissions::RW,
            )
            .unwrap();
        let mut bytes = vec![0x5a; 14];
        bytes[..4].copy_from_slice(&0x4000u32.to_be_bytes());
        bytes[4..6].copy_from_slice(&version.to_be_bytes());
        memory
            .map(GuestPtr(0x4100), bytes, Permissions::RX)
            .unwrap();
        GuestPtr(0x4100)
    }
    #[test]
    fn color_port_aliases_decode_live_handles_across_packed_depths_and_preserve_descriptors() {
        for depth in [1, 2, 4, 8] {
            let (mut memory, expected) = fixture(depth, 0x8000);
            if depth == 1 {
                memory.write(GuestPtr(0x210a), &[255; 6]).unwrap();
                memory.write(GuestPtr(0x2112), &[0; 6]).unwrap();
            }
            let address = alias(&mut memory, 0xc005);
            assert!(source(&memory, address).is_err());
            assert!(destination(&memory, address).is_err());
            let before = memory.read(address, 14).unwrap().to_vec();
            let decoded = copy_source(&memory, address).unwrap();
            assert_eq!(decoded.indices, expected);
            let direct = source(&memory, GuestPtr(0x1000)).unwrap();
            assert_eq!(
                (
                    decoded.bitmap.base,
                    decoded.bitmap.row_bytes,
                    decoded.bitmap.bounds
                ),
                (
                    direct.bitmap.base,
                    direct.bitmap.row_bytes,
                    direct.bitmap.bounds
                )
            );
            assert_eq!(decoded.palette, direct.palette);
            let wrapped = copy_destination(&memory, address).unwrap();
            let direct = destination(&memory, GuestPtr(0x1000)).unwrap();
            assert_eq!(
                wrapped.unpack(&memory).unwrap(),
                direct.unpack(&memory).unwrap()
            );
            let mut relocated = memory.read(GuestPtr(0x1000), 50).unwrap().to_vec();
            relocated[6..14].copy_from_slice(&[0, 2, 0, 3, 0, 4, 0, 8]);
            memory
                .map(GuestPtr(0x4200), relocated, Permissions::RW)
                .unwrap();
            memory
                .write(GuestPtr(0x4000), &0x4200u32.to_be_bytes())
                .unwrap();
            assert_eq!(
                copy_source(&memory, address).unwrap().bitmap.bounds,
                Rect::new(3, 2, 8, 4)
            );
            assert_eq!(memory.read(address, 14).unwrap(), before);
        }
    }
    #[test]
    fn aliases_reject_nil_faulting_cycles_and_malformed_underlying_pixmaps_without_writes() {
        for target in [0, 0xfffffff0, 0x4100] {
            let (mut memory, _) = fixture(8, 0x8000);
            let address = alias(&mut memory, 0xc000);
            memory
                .write(GuestPtr(0x4000), &u32::to_be_bytes(target))
                .unwrap();
            let before = memory.read(GuestPtr(0x1000), 50).unwrap().to_vec();
            assert!(copy_source(&memory, address).is_err());
            assert!(copy_destination(&memory, address).is_err());
            assert_eq!(memory.read(GuestPtr(0x1000), 50).unwrap(), before);
        }
        let (mut memory, _) = fixture(8, 0x8000);
        let address = alias(&mut memory, 0xc000);
        memory
            .write(GuestPtr(0x1020), &16u16.to_be_bytes())
            .unwrap();
        assert!(copy_source(&memory, address).is_err());
        assert!(copy_destination(&memory, address).is_err());
        memory.write(GuestPtr(0x1020), &8u16.to_be_bytes()).unwrap();
        memory.write(GuestPtr(0x102a), &0u32.to_be_bytes()).unwrap();
        assert!(copy_source(&memory, address).is_err());
        assert!(copy_destination(&memory, address).is_err());
        assert!(source(&memory, GuestPtr(0)).is_err());
    }
    #[test]
    fn default_color_metadata_does_not_require_attached_pixels_but_drawing_does() {
        let (mut memory, _) = fixture(8, 0x8000);
        let colors = default_colors(&memory, GuestPtr(0x1000)).unwrap();
        memory.write(GuestPtr(0x1000), &0u32.to_be_bytes()).unwrap();
        assert_eq!(default_colors(&memory, GuestPtr(0x1000)).unwrap(), colors);
        assert!(source(&memory, GuestPtr(0x1000)).is_err());
        assert!(destination(&memory, GuestPtr(0x1000)).is_err());
    }
    #[test]
    fn packed_depths_padding_negative_origins_and_entry_order_decode_exactly() {
        for depth in [1, 2, 4, 8] {
            for flags in [0, 0x8000] {
                let (memory, expected) = fixture(depth, flags);
                let decoded = source(&memory, GuestPtr(0x1000)).unwrap();
                assert_eq!(decoded.indices, expected);
                assert_eq!(decoded.bitmap.bounds, Rect::new(-3, -2, 2, 0));
                let table = decoded.palette.unwrap();
                for index in expected {
                    assert_eq!(
                        table[index as usize],
                        [
                            u16::from(index) * 257,
                            65535 - u16::from(index) * 257,
                            0x1234
                        ]
                    );
                }
            }
        }
    }
    #[test]
    fn malformed_components_tables_and_missing_pixel_entries_are_rejected() {
        for (at, value) in [(14, 1u16), (16, 2), (30, 16), (32, 16), (34, 3), (36, 2)] {
            let (mut memory, _) = fixture(8, 0x8000);
            memory
                .write(GuestPtr(0x1000 + at), &value.to_be_bytes())
                .unwrap();
            assert!(source(&memory, GuestPtr(0x1000)).is_err());
        }
        for pointer in [0u32, 0x4000] {
            let (mut memory, _) = fixture(8, 0x8000);
            memory
                .map(GuestPtr(0x4000), vec![0; 4], Permissions::RX)
                .unwrap();
            memory
                .write(GuestPtr(0x102a), &pointer.to_be_bytes())
                .unwrap();
            assert!(source(&memory, GuestPtr(0x1000)).is_err());
        }
        let (mut memory, _) = fixture(2, 0x8000);
        memory.write(GuestPtr(0x2106), &0u16.to_be_bytes()).unwrap();
        assert!(source(&memory, GuestPtr(0x1000)).is_err());
        let (mut memory, _) = fixture(2, 0);
        memory.write(GuestPtr(0x2110), &3u16.to_be_bytes()).unwrap();
        assert!(source(&memory, GuestPtr(0x1000)).is_err());
        memory
            .write(GuestPtr(0x2106), &0xffffu16.to_be_bytes())
            .unwrap();
        assert!(source(&memory, GuestPtr(0x1000)).is_err());
    }
}
