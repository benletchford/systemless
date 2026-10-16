//! Shared GrafPort/CGrafPort field access. Common geometry/text/pen fields keep
//! their classic offsets; bitmaps and pattern storage differ between layouts.
use crate::{
    bitmap::Bitmap,
    memory::{GuestPtr, Memory},
    Error, Result,
};
#[derive(Clone, Copy)]
pub(crate) enum Pattern {
    Background,
    Pen,
    Fill,
}
fn bad() -> Error {
    Error::Graphics("invalid or unsupported color port layout".into())
}
pub(crate) fn color(memory: &Memory, port: GuestPtr) -> Result<bool> {
    memory.read(port, 108)?;
    match memory.u16(GuestPtr(port.0 + 6))? & 0xc000 {
        0 => Ok(false),
        0xc000 => Ok(true),
        _ => Err(bad()),
    }
}
pub(crate) fn pixmap(memory: &Memory, port: GuestPtr) -> Result<GuestPtr> {
    let handle = memory.u32(GuestPtr(port.0 + 2))?;
    if handle == 0 {
        return Err(bad());
    }
    let data = memory.u32(GuestPtr(handle))?;
    if data == 0 {
        return Err(bad());
    }
    memory.read(GuestPtr(data), 50)?;
    Ok(GuestPtr(data))
}
pub(crate) fn bounds_pointer(memory: &Memory, port: GuestPtr) -> Result<GuestPtr> {
    Ok(if color(memory, port)? {
        let map = pixmap(memory, port)?;
        GuestPtr(map.0.checked_add(6).ok_or_else(bad)?)
    } else {
        GuestPtr(port.0 + 8)
    })
}
pub(crate) fn bitmap(memory: &Memory, port: GuestPtr) -> Result<crate::surface::Surface> {
    if color(memory, port)? {
        crate::pixmap::destination(memory, pixmap(memory, port)?)
    } else {
        let address = GuestPtr(port.0 + 2);
        match Bitmap::read(memory, address) {
            Ok(bitmap) => bitmap.surface(memory),
            Err(error) => {
                // Some original applications retain screen bounds while replacing
                // a basic port's base/stride. Keep standalone BitMap validation
                // strict; stage only the prefix reachable through this portRect.
                let bytes = memory.read(address, 14)?;
                let word = |at| i16::from_be_bytes(bytes[at..at + 2].try_into().unwrap()) as i32;
                let base = u32::from_be_bytes(bytes[..4].try_into().unwrap());
                let declared = crate::graphics::Rect::new(word(8), word(6), word(12), word(10));
                let rect = crate::quickdraw::read_rect(memory, GuestPtr(port.0 + 16))?;
                if (crate::quickdraw::SCREEN.0..crate::quickdraw::SCREEN.0 + 4096 * 4096)
                    .contains(&base)
                    || declared.right <= declared.left
                    || declared.bottom <= declared.top
                    || declared.right - declared.left > 4096
                    || declared.bottom - declared.top > 4096
                    || rect.right <= rect.left
                    || rect.bottom <= rect.top
                {
                    return Err(error);
                }
                let bitmap = Bitmap {
                    base: GuestPtr(base),
                    row_bytes: usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap())),
                    bounds: crate::graphics::Rect::new(
                        declared.left,
                        declared.top,
                        declared.right.min(rect.right),
                        declared.bottom.min(rect.bottom),
                    ),
                };
                bitmap.extent()?;
                memory.read(bitmap.base, bitmap.byte_len()?)?;
                Ok(crate::surface::Surface::mono(bitmap))
            }
        }
    }
}
pub(crate) fn pattern_pointer(
    memory: &Memory,
    port: GuestPtr,
    pattern: Pattern,
) -> Result<GuestPtr> {
    if !color(memory, port)? {
        return Ok(GuestPtr(
            port.0
                + match pattern {
                    Pattern::Background => 32,
                    Pattern::Pen => 58,
                    Pattern::Fill => 40,
                },
        ));
    }
    let at = match pattern {
        Pattern::Background => 32,
        Pattern::Pen => 58,
        Pattern::Fill => 62,
    };
    let handle = memory.u32(GuestPtr(port.0 + at))?;
    if handle == 0 {
        return Err(bad());
    }
    let data = memory.u32(GuestPtr(handle))?;
    if data == 0 {
        return Err(bad());
    }
    let bytes = memory.read(GuestPtr(data), 28)?;
    if bytes[..20].iter().any(|byte| *byte != 0) {
        return Err(Error::Graphics(
            "expanded/color pixel patterns are not implemented".into(),
        ));
    }
    Ok(GuestPtr(data.checked_add(20).ok_or_else(bad)?))
}
pub(crate) fn pattern(memory: &Memory, port: GuestPtr, kind: Pattern) -> Result<[u8; 8]> {
    Ok(memory
        .read(pattern_pointer(memory, port, kind)?, 8)?
        .try_into()
        .unwrap())
}
pub(crate) fn write_pattern(
    memory: &mut Memory,
    port: GuestPtr,
    kind: Pattern,
    pattern: &[u8],
) -> Result<()> {
    if pattern.len() != 8 {
        return Err(bad());
    }
    let pointer = pattern_pointer(memory, port, kind)?;
    memory.write(pointer, pattern)
}

pub(crate) fn colors(
    memory: &Memory,
    port: GuestPtr,
    target: &crate::surface::Surface,
) -> Result<(u8, u8)> {
    let resolve = |at| {
        let value = memory.u32(GuestPtr(port.0 + at))?;
        if color(memory, port)? {
            target.index(value)
        } else {
            match value {
                33 => target.nearest([0; 3]),
                30 => target.nearest([65535; 3]),
                _ => Err(Error::Graphics("unsupported basic port color".into())),
            }
        }
    };
    Ok((resolve(80)?, resolve(84)?))
}

pub(crate) fn selected_surface(memory: &Memory) -> Result<crate::surface::Surface> {
    let h = memory.u32(crate::devices::CURRENT)?;
    if h == 0 {
        return Err(bad());
    }
    let data = memory.u32(GuestPtr(h))?;
    if data == 0 {
        return Err(bad());
    }
    let record = memory.read(GuestPtr(data), 62)?;
    if record[12..20].iter().any(|v| *v != 0) {
        return Err(Error::Graphics(
            "custom color search/complement procedures are unsupported".into(),
        ));
    }
    let map_h = memory.u32(GuestPtr(data.checked_add(22).ok_or_else(bad)?))?;
    if map_h == 0 {
        return Err(bad());
    }
    let map = memory.u32(GuestPtr(map_h))?;
    if map == 0 {
        return Err(bad());
    }
    crate::pixmap::destination(memory, GuestPtr(map))
}

#[cfg(test)]
mod clipped_basic_port_tests {
    use super::*;
    fn fixture() -> Memory {
        let mut memory = Memory::default();
        let mut port = vec![0; 108];
        port[2..6].copy_from_slice(&0x2000u32.to_be_bytes());
        port[6..8].copy_from_slice(&2u16.to_be_bytes());
        port[12..14].copy_from_slice(&480u16.to_be_bytes());
        port[14..16].copy_from_slice(&640u16.to_be_bytes());
        port[20..22].copy_from_slice(&4u16.to_be_bytes());
        port[22..24].copy_from_slice(&8u16.to_be_bytes());
        memory
            .map(GuestPtr(0x1000), port, crate::memory::Permissions::RW)
            .unwrap();
        memory
            .map(
                GuestPtr(0x2000),
                vec![0xa5; 8],
                crate::memory::Permissions::RW,
            )
            .unwrap();
        memory
    }
    #[test]
    fn port_prefix_is_checked_without_accepting_the_standalone_descriptor() {
        let memory = fixture();
        assert!(Bitmap::read(&memory, GuestPtr(0x1002)).is_err());
        let target = bitmap(&memory, GuestPtr(0x1000)).unwrap();
        assert_eq!(target.extent().unwrap(), (8, 4));
        assert_eq!(target.row_bytes, 2);
        assert_eq!(target.base, GuestPtr(0x2000));
    }
    #[test]
    fn prefix_rejects_bad_stride_flags_geometry_and_short_or_screen_storage() {
        for (at, bytes) in [
            (6, 0u16.to_be_bytes().to_vec()),
            (6, 0x4002u16.to_be_bytes().to_vec()),
            (22, 17u16.to_be_bytes().to_vec()),
            (20, 5u16.to_be_bytes().to_vec()),
            (2, crate::quickdraw::SCREEN.0.to_be_bytes().to_vec()),
            (2, 0x2001u32.to_be_bytes().to_vec()),
        ] {
            let mut memory = fixture();
            memory.write(GuestPtr(0x1000 + at), &bytes).unwrap();
            assert!(bitmap(&memory, GuestPtr(0x1000)).is_err(), "offset {at}");
            assert_eq!(memory.read(GuestPtr(0x2000), 8).unwrap(), [0xa5; 8]);
        }
    }
}
