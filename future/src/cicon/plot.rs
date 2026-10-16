//! IM More Macintosh Toolbox, PlotCIcon (5-25–5-26), and Apple TN QD18.
use super::{bad, Icon};
use crate::{
    bitmap::Bitmap,
    graphics::{BlitMask, Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory},
    process::Process,
    quickdraw::Ports,
    Result,
};
struct Raster {
    width: u32,
    height: u32,
    mask: Vec<u8>,
    pixels: Vec<u8>,
    palette: Option<Box<[[u16; 3]; 256]>>,
}
fn word(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes(b[at..at + 2].try_into().unwrap())
}
fn unpack(bytes: &[u8], w: usize, h: usize, row: usize, depth: usize) -> Vec<u8> {
    (0..h)
        .flat_map(|y| {
            (0..w).map(move |x| {
                let bit = x * depth;
                (bytes[y * row + bit / 8] >> (8 - depth - bit % 8)) & ((1u16 << depth) - 1) as u8
            })
        })
        .collect()
}
fn device_depth(memory: &Memory, process: &Process) -> Result<u16> {
    let h = memory.u32(crate::devices::CURRENT)?;
    if h == 0 {
        return Ok(process.display.depth());
    }
    let gd = memory.u32(GuestPtr(h))?;
    if gd == 0 {
        return Err(bad());
    }
    let pm = memory.u32(GuestPtr(gd.checked_add(22).ok_or_else(bad)?))?;
    if pm == 0 {
        return Err(bad());
    }
    let p = memory.u32(GuestPtr(pm))?;
    if p == 0 {
        return Err(bad());
    }
    let depth = memory.u16(GuestPtr(p.checked_add(32).ok_or_else(bad)?))?;
    if ![1, 2, 4, 8].contains(&depth) {
        return Err(bad());
    }
    Ok(depth)
}
fn raw(icon: Icon, shallow: bool) -> Raster {
    let r = &icon.record;
    let w = (i32::from(word(r, 12) as i16) - i32::from(word(r, 8) as i16)) as usize;
    let h = (i32::from(word(r, 10) as i16) - i32::from(word(r, 6) as i16)) as usize;
    let mono = shallow && word(r, 68) != 0;
    let palette = if mono {
        None
    } else {
        let mut p = Box::new([[65535; 3]; 256]);
        let ordinal = word(&icon.table, 4) & 0x8000 != 0;
        for n in 0..usize::from(word(&icon.table, 6)) + 1 {
            let at = 8 + n * 8;
            let i = if ordinal {
                n
            } else {
                usize::from(word(&icon.table, at))
            };
            p[i] = std::array::from_fn(|j| word(&icon.table, at + 2 + j * 2));
        }
        Some(p)
    };
    let pixels = if mono {
        unpack(
            &icon.masks[icon.mask_len..],
            w,
            h,
            usize::from(word(r, 68)),
            1,
        )
    } else {
        unpack(
            &icon.pixels,
            w,
            h,
            usize::from(word(r, 4) & 0x3fff),
            usize::from(word(r, 32)),
        )
    };
    Raster {
        width: w as u32,
        height: h as u32,
        mask: unpack(&icon.masks, w, h, usize::from(word(r, 54)), 1),
        pixels,
        palette,
    }
}
fn prepare(
    memory: &Memory,
    process: &mut Process,
    handle: u32,
    shallow: bool,
) -> Result<Option<Raster>> {
    let size = process.handle_size(memory, handle)?;
    if process.mem_error != 0 {
        return Err(bad());
    }
    let p = memory.u32(GuestPtr(handle))?;
    if p == 0 {
        return Ok(None);
    }
    if size < 82 {
        return Err(bad());
    }
    let record = memory.read(GuestPtr(p), 82)?;
    let image = memory.u32(GuestPtr(p + 78))?;
    // Resource Manager clients may pass the compiled resource itself. Its
    // placeholder pointers must never be followed or patched as a side effect.
    if process.handle_state(memory, handle)? & 0x20 != 0 || image == 0 {
        return Ok(Some(raw(
            Icon::decode(memory.read(GuestPtr(p), size as usize)?)?,
            shallow,
        )));
    }
    let bounds = crate::quickdraw::read_rect(memory, GuestPtr(p + 6))?;
    let w = bounds.right - bounds.left;
    let h = bounds.bottom - bounds.top;
    if w <= 0 || h <= 0 || w > 4096 || h > 4096 {
        return Err(bad());
    }
    let mask = Bitmap::read(memory, GuestPtr(p + 50))?;
    if mask.extent()? != (w as usize, h as usize) {
        return Err(bad());
    }
    let mask_bits = mask.unpack(memory)?;
    let mono = shallow && word(record, 68) != 0;
    let (pixels, palette) = if mono {
        let bitmap = Bitmap::read(memory, GuestPtr(p + 64))?;
        if bitmap.extent()? != (w as usize, h as usize) {
            return Err(bad());
        }
        (bitmap.unpack(memory)?, None)
    } else {
        let bytes = u32::from(word(record, 4) & 0x3fff) * h as u32;
        if process.handle_size(memory, image)? < bytes || process.mem_error != 0 {
            return Err(bad());
        }
        let base = GuestPtr(memory.u32(GuestPtr(image))?);
        if base.0 == 0 {
            return Err(bad());
        }
        let source = crate::pixmap::icon_source(memory, GuestPtr(p), base)?;
        (source.indices, source.palette)
    };
    Ok(Some(Raster {
        width: w as u32,
        height: h as u32,
        mask: mask_bits,
        pixels,
        palette,
    }))
}
/// Source decoding is CPU preparation; every destination pixel is drawn by wgpu.
pub fn plot(
    memory: &mut Memory,
    process: &mut Process,
    ports: Ports,
    gpu: &Graphics,
    handle: u32,
    destination: GuestPtr,
) -> Result<()> {
    if handle == 0 || destination.0 == 0 {
        return Ok(());
    }
    let rect = crate::quickdraw::read_rect(memory, destination)?;
    if rect.left >= rect.right || rect.top >= rect.bottom {
        return Ok(());
    }
    let saved = process.mem_error;
    let result = (|| {
        let shallow = device_depth(memory, process)? <= 2;
        let Some(source) = prepare(memory, process, handle, shallow)? else {
            return Ok(());
        };
        let (target, clips, foreground, background) = ports.icon_target(memory)?;
        // Snapshot mask/image/palette before uploading or changing aliased storage.
        let full = Rect::new(0, 0, source.width as i32, source.height as i32);
        target.execute_source_palette(
            gpu,
            memory,
            Draw::Blit {
                mask: Some(BlitMask {
                    rect: full,
                    width: source.width,
                    height: source.height,
                    bits: &source.mask,
                }),
                rect,
                clip: target.bounds,
                source: full,
                width: source.width,
                height: source.height,
                foreground: u32::from(foreground),
                background: u32::from(background),
                invert: false,
                mode: Transfer::Copy,
            },
            Some(&clips),
            Some(&source.pixels),
            source.palette.as_deref(),
        )
    })();
    process.mem_error = saved;
    result
}
