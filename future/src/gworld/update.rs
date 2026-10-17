//! UpdateGWorld stages GPU pixel work and all handle payloads before publication.
use super::*;
use crate::{
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::Permissions,
};
const CLIP: u32 = 1 << 28;
const STRETCH: u32 = 1 << 29;
const DITHER: u32 = 1 << 30;
fn failure(p: &mut Process, code: i16) -> u32 {
    p.color.set_error(code);
    1 << 31
}
fn colors(m: &Memory, p: &mut Process, table: u32) -> Result<Vec<u8>> {
    let data = owned(m, p, table, 8)?;
    let last = m.u16(GuestPtr(data.0 + 6))? as i16;
    if !(0..256).contains(&last) {
        return Err(bad("invalid GWorld update color table"));
    }
    let size = 8 + (last as u32 + 1) * 8;
    owned(m, p, table, size)?;
    Ok(m.read(data, size as usize)?.to_vec())
}
fn pixels(
    m: &Memory,
    gpu: &Graphics,
    old: &[u8],
    new: &[u8],
    colors: &[u8],
    length: usize,
    flags: u32,
) -> Result<Vec<u8>> {
    let bounds = |pm: &[u8]| {
        let word = |at| i32::from(i16::from_be_bytes([pm[at], pm[at + 1]]));
        Rect::new(word(8), word(6), word(12), word(10))
    };
    let src_bounds = bounds(old);
    let dst_bounds = bounds(new);
    let mut scratch = Memory::default();
    let mut source = old.to_vec();
    let map_handle = u32::from_be_bytes(old[42..46].try_into().unwrap());
    let base = u32::from_be_bytes(old[..4].try_into().unwrap());
    let image = base;
    let rows = u16::from_be_bytes([old[4], old[5]]) & 0x3fff;
    let source_length = usize::from(rows) * (src_bounds.bottom - src_bounds.top) as usize;
    scratch.map(
        GuestPtr(0x1000),
        m.read(GuestPtr(image), source_length)?.to_vec(),
        Permissions::RW,
    )?;
    let ct_data = GuestPtr(m.u32(GuestPtr(map_handle))?);
    let count = usize::from(m.u16(GuestPtr(ct_data.0 + 6))?) + 1;
    scratch.map(
        GuestPtr(0x10000000),
        m.read(ct_data, 8 + count * 8)?.to_vec(),
        Permissions::RW,
    )?;
    scratch.map(
        GuestPtr(0x20000000),
        0x10000000u32.to_be_bytes().to_vec(),
        Permissions::RW,
    )?;
    source[..4].copy_from_slice(&0x1000u32.to_be_bytes());
    source[42..46].copy_from_slice(&0x20000000u32.to_be_bytes());
    scratch.map(GuestPtr(0x30000000), source, Permissions::RW)?;
    scratch.map(GuestPtr(0x40000000), vec![0; length], Permissions::RW)?;
    scratch.map(GuestPtr(0x50000000), colors.to_vec(), Permissions::RW)?;
    scratch.map(
        GuestPtr(0x60000000),
        0x50000000u32.to_be_bytes().to_vec(),
        Permissions::RW,
    )?;
    let mut destination = new.to_vec();
    destination[..4].copy_from_slice(&0x40000000u32.to_be_bytes());
    destination[42..46].copy_from_slice(&0x60000000u32.to_be_bytes());
    scratch.map(GuestPtr(0x70000000), destination, Permissions::RW)?;
    let source = crate::pixmap::copy_source(&scratch, GuestPtr(0x30000000))?;
    let destination = crate::pixmap::copy_destination(&scratch, GuestPtr(0x70000000))?;
    let width = (src_bounds.right - src_bounds.left) as u32;
    let height = (src_bounds.bottom - src_bounds.top) as u32;
    let rect = if flags & STRETCH != 0 {
        dst_bounds
    } else {
        Rect::new(
            dst_bounds.left,
            dst_bounds.top,
            dst_bounds.left + width as i32,
            dst_bounds.top + height as i32,
        )
    };
    let draw = Draw::Blit {
        mask: None,
        rect,
        clip: dst_bounds,
        source: Rect::new(0, 0, width as i32, height as i32),
        width,
        height,
        foreground: destination.nearest([0; 3])?,
        background: destination.nearest([65535; 3])?,
        invert: false,
        mode: Transfer::Copy,
    };
    destination.copy_image(gpu, &mut scratch, draw, &source, flags & DITHER != 0)?;
    Ok(scratch.read(GuestPtr(0x40000000), length)?.to_vec())
}
impl Manager {
    pub(super) fn update(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        gpu: Option<&Graphics>,
        r: Update,
    ) -> Result<u32> {
        m.validate_write(r.output, 4)?;
        let pointer = m.u32(r.output)?;
        let world = self.world(m, p, pointer)?;
        let rect: [u8; 8] = m.read(r.bounds, 8)?.try_into().unwrap();
        let coordinate = |i| i32::from(i16::from_be_bytes([rect[i], rect[i + 1]]));
        let width = coordinate(6) - coordinate(2);
        let height = coordinate(4) - coordinate(0);
        if width <= 0
            || height <= 0
            || r.flags & !(8 | CLIP | STRETCH | DITHER) != 0
            || r.flags & CLIP != 0 && r.flags & STRETCH != 0
            || r.flags & DITHER != 0 && r.flags & (CLIP | STRETCH) == 0
        {
            return Ok(failure(p, -50));
        }
        if r.device == 0 && !matches!(r.depth, 0 | 1 | 2 | 4 | 8 | 16 | 32) {
            return Ok(failure(p, -157));
        }
        if r.device == 0 && matches!(r.depth, 16 | 32) {
            return Err(bad("direct-color GWorld update is not implemented"));
        }
        let pm = owned(m, p, world.attached[0], 50)?;
        let old = m.read(pm, 50)?.to_vec();
        if u16::from_be_bytes([old[32], old[33]]) > 8 {
            return Err(bad("direct-color GWorld update is not implemented"));
        }
        let old_colors = colors(m, p, world.table)?;
        let format = if r.device != 0 {
            Some(r.device)
        } else if r.depth == 0 {
            Some(device(m, p, devices::Op::Maximum(r.bounds))?)
        } else {
            None
        };
        let (depth, new_colors) = if let Some(gd) = format {
            if gd == 0 {
                return Ok(failure(p, -50));
            }
            let data = owned(m, p, gd, 62)?;
            let map = owned(m, p, m.u32(GuestPtr(data.0 + 22))?, 50)?;
            (
                m.u16(GuestPtr(map.0 + 32))?,
                colors(m, p, m.u32(GuestPtr(map.0 + 42))?)?,
            )
        } else {
            (
                r.depth as u16,
                if r.table == 0 {
                    crate::color_tables::standard(r.depth).unwrap()
                } else {
                    colors(m, p, r.table)?
                },
            )
        };
        if !matches!(depth, 1 | 2 | 4 | 8) {
            return Err(bad("unsupported GWorld update device depth"));
        }
        let row = (width as u32 * u32::from(depth)).div_ceil(32) * 4;
        if row > 0x3fff {
            return Ok(failure(p, -50));
        }
        let length = row * height as u32;
        if !p.can_resize_handles(m, &[(world.image, length)])? {
            return Ok(failure(p, -108));
        }
        // Pinned images may not be replaced, even by a same-size conversion.
        if p.pixel_image_is_pinned(world.image) {
            return Ok(failure(p, -108));
        }
        let old_depth = u16::from_be_bytes([old[32], old[33]]);
        let old_row = u16::from_be_bytes([old[4], old[5]]) & 0x3fff;
        let changed_bounds = old[6..14] != rect;
        let changed_depth = old_depth != depth;
        let changed_colors = old_colors != new_colors;
        let image_base = p.pixel_base(m, world.attached[0])?;
        let purged = image_base == 0;
        if !changed_bounds && !changed_depth && !changed_colors && world.owns_device && !purged {
            p.color.set_error(0);
            return Ok(0);
        }
        let inverse = crate::color::prepare_inverse(&new_colors, 3)?;
        let mut port_bytes = m.read(GuestPtr(pointer), 108)?.to_vec();
        port_bytes[16..24].copy_from_slice(&rect);
        for (rgb_at, index_at) in [(36, 80), (42, 84)] {
            let rgb = std::array::from_fn(|i| {
                u16::from_be_bytes([port_bytes[rgb_at + i * 2], port_bytes[rgb_at + i * 2 + 1]])
            });
            let index = crate::color::match_staged(&new_colors, rgb)?;
            port_bytes[index_at..index_at + 4].copy_from_slice(&u32::from(index).to_be_bytes());
        }
        let mut next = old.clone();
        next[4..6].copy_from_slice(&((row as u16) | 0x8000).to_be_bytes());
        next[6..14].copy_from_slice(&rect);
        next[32..34].copy_from_slice(&depth.to_be_bytes());
        next[36..38].copy_from_slice(&depth.to_be_bytes());
        let same_dimensions = i32::from(i16::from_be_bytes([old[12], old[13]]))
            - i32::from(i16::from_be_bytes([old[8], old[9]]))
            == width
            && i32::from(i16::from_be_bytes([old[10], old[11]]))
                - i32::from(i16::from_be_bytes([old[6], old[7]]))
                == height;
        let preserve =
            r.flags & (CLIP | STRETCH) != 0 || changed_depth || changed_colors || same_dimensions;
        let old_size = p.handle_size(m, world.image)?;
        let mut source = old.clone();
        source[..4].copy_from_slice(&image_base.to_be_bytes());
        let image = if purged {
            vec![0; length as usize]
        } else if !changed_bounds && !changed_depth && !changed_colors {
            m.read(GuestPtr(image_base), length as usize)?.to_vec()
        } else if preserve {
            pixels(
                m,
                gpu.ok_or_else(|| bad("GWorld pixel preservation requires wgpu"))?,
                &source,
                &next,
                &new_colors,
                length as usize,
                r.flags,
            )?
        } else {
            vec![0; length as usize]
        };
        let mut allocated_device = 0;
        let stage = (|| -> Result<Option<World>> {
            let mut updated = world;
            if !world.owns_device {
                allocated_device = device(
                    m,
                    p,
                    devices::Op::New {
                        reference: 0,
                        mode: -1,
                    },
                )?;
                if allocated_device == 0 {
                    return Ok(None);
                }
                updated.device = allocated_device;
                updated.owns_device = true;
            }
            let manager = std::mem::take(&mut p.devices);
            let topology = manager.storage_handles(m, p, updated.device);
            p.devices = manager;
            topology?;
            if p.pixel_image_is_pinned(world.table) {
                return Ok(None);
            }
            let gd = owned(m, p, updated.device, 62)?;
            let mut gd_bytes = m.read(gd, 62)?.to_vec();
            let device_pm = m.u32(GuestPtr(gd.0 + 22))?;
            let device_map = owned(m, p, device_pm, 50)?;
            let ct = m.u32(GuestPtr(device_map.0 + 42))?;
            if world.owns_device && ct != world.table {
                return Err(bad("GWorld device table ownership changed"));
            }
            updated.table = ct;
            let inverse_handle = m.u32(GuestPtr(gd.0 + 6))?;
            owned(m, p, inverse_handle, 6)?;
            gd_bytes[10..12].copy_from_slice(&3u16.to_be_bytes());
            gd_bytes[34..42].copy_from_slice(&rect);
            next[..4].copy_from_slice(&world.image.to_be_bytes());
            next[42..46].copy_from_slice(&ct.to_be_bytes());
            let mut visible = 10u16.to_be_bytes().to_vec();
            visible.extend(rect);
            m.validate_write(GuestPtr(pointer), 108)?;
            m.validate_write(devices::CURRENT, 4)?;
            if !world.owns_device {
                m.validate_write(GuestPtr(world.table), 4)?;
            }
            let selected = m.u32(devices::CURRENT)? == world.device
                && m.u32(p.ports.unwrap().current_slot)? == pointer;
            let changes = vec![
                (world.image, image),
                (ct, new_colors),
                (inverse_handle, inverse),
                (world.attached[0], next.clone()),
                (device_pm, next.clone()),
                (updated.device, gd_bytes),
                (world.attached[1], visible),
            ];
            let sizes: Vec<_> = changes.iter().map(|(h, b)| (*h, b.len() as u32)).collect();
            if !p.can_resize_handles(m, &sizes)? {
                return Ok(None);
            }
            p.replace_handles(m, changes)?;
            if p.mem_error != 0 {
                return Ok(None);
            }
            m.write(GuestPtr(pointer), &port_bytes)?;
            if selected {
                m.write(devices::CURRENT, &updated.device.to_be_bytes())?;
            }
            if !world.owns_device {
                p.dispose_handle(m, world.table)?;
            }
            Ok(Some(updated))
        })();
        match stage {
            Ok(Some(updated)) => {
                self.worlds.insert(pointer, updated);
                p.color.set_error(0);
                let mut flags = 0;
                if changed_colors {
                    flags |= 1 << 16;
                }
                if changed_depth {
                    flags |= 1 << 17;
                }
                if old[6..10] != rect[..4] {
                    flags |= 1 << 18;
                }
                if row != u32::from(old_row) {
                    flags |= 1 << 19;
                }
                if length != old_size
                    || (!preserve && changed_bounds)
                    || p.pixel_base(m, world.attached[0])? != image_base
                {
                    flags |= 1 << 20;
                }
                if changed_bounds && !same_dimensions && !purged {
                    flags |= r.flags & (CLIP | STRETCH);
                }
                if preserve && !purged {
                    flags |= r.flags & DITHER;
                }
                Ok(flags)
            }
            other => {
                if allocated_device != 0 {
                    device(m, p, devices::Op::Dispose(allocated_device))?;
                }
                other?;
                Ok(failure(p, -108))
            }
        }
    }
}
