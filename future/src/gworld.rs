//! Shared offscreen-world ownership. Inside Macintosh: Imaging With QuickDraw,
//! chapter 6; QDOffscreen.h (Universal Interfaces 3.4) defines ABI selectors.
use crate::{
    color_ports::Action,
    devices,
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
use std::collections::BTreeMap;
mod update;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Update {
    pub output: GuestPtr,
    pub depth: i16,
    pub bounds: GuestPtr,
    pub table: u32,
    pub device: u32,
    pub flags: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Update(Update),
    New {
        output: GuestPtr,
        depth: i16,
        bounds: GuestPtr,
        table: u32,
        device: u32,
        flags: u32,
    },
    Dispose(u32),
    Get {
        port: GuestPtr,
        device: GuestPtr,
    },
    Set {
        port: u32,
        device: u32,
    },
    PixMap(u32),
    Device(u32),
}
#[derive(Clone, Copy)]
struct World {
    device: u32,
    owns_device: bool,
    table: u32,
    image: u32,
    attached: [u32; 7],
}
#[derive(Default)]
pub struct Manager {
    worlds: BTreeMap<u32, World>,
}
fn bad(s: &str) -> Error {
    Error::Graphics(s.into())
}
fn owned(m: &Memory, p: &mut Process, h: u32, size: u32) -> Result<GuestPtr> {
    let actual = p.handle_size(m, h)?;
    if p.mem_error != 0 || actual < size {
        return Err(bad("GWorld requires a live owned handle"));
    }
    Ok(GuestPtr(m.u32(GuestPtr(h))?))
}
fn device(m: &mut Memory, p: &mut Process, op: devices::Op) -> Result<u32> {
    let mut manager = std::mem::take(&mut p.devices);
    let result = manager.call(m, p, op);
    p.devices = manager;
    result
}
fn port(m: &mut Memory, p: &mut Process, action: Action, pointer: u32) -> Result<()> {
    let mut manager = std::mem::take(&mut p.color_ports);
    let result = manager.call(m, p, action, GuestPtr(pointer));
    p.color_ports = manager;
    result
}
fn attached(m: &Memory, p: &mut Process, pointer: u32) -> Result<[u32; 7]> {
    let manager = std::mem::take(&mut p.color_ports);
    let result = manager.attached(m, p, GuestPtr(pointer));
    p.color_ports = manager;
    result
}
impl Manager {
    fn world(&self, m: &Memory, p: &mut Process, pointer: u32) -> Result<World> {
        let world = *self
            .worlds
            .get(&pointer)
            .ok_or_else(|| bad("unknown or disposed GWorld"))?;
        if p.pointer_size(pointer) < 108
            || p.mem_error != 0
            || attached(m, p, pointer)? != world.attached
        {
            return Err(bad("GWorld port ownership changed"));
        }
        let pm = owned(m, p, world.attached[0], 50)?;
        if m.u32(GuestPtr(pm.0 + 42))? != world.table {
            return Err(bad("GWorld color table ownership changed"));
        }
        // GetPixBaseAddr validates both the unlocked handle and the locked data
        // representation, including the image extent and pixel lock identity.
        let image = owned(m, p, world.image, 0)?;
        let stored = m.u32(pm)?;
        if p.pixel_base(m, world.attached[0])? != image.0
            || (stored != world.image && stored != image.0)
        {
            return Err(bad("GWorld image ownership changed"));
        }
        owned(m, p, world.device, 62)?;
        owned(m, p, world.table, 8)?;
        Ok(world)
    }
    pub fn call(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        op: Op,
        gpu: Option<&crate::graphics::Graphics>,
    ) -> Result<u32> {
        let ports = p
            .ports
            .ok_or_else(|| bad("GWorld services require InitGraf"))?;
        match op {
            Op::Update(request) => self.update(m, p, gpu, request),
            Op::Get {
                port,
                device: output,
            } => {
                m.validate_write(port, 4)?;
                m.validate_write(output, 4)?;
                let current = m.u32(ports.current_slot)?;
                let gd = device(m, p, devices::Op::Current)?;
                m.write(port, &current.to_be_bytes())?;
                m.write(output, &gd.to_be_bytes())?;
                Ok(0)
            }
            Op::Set {
                port: pointer,
                device: requested,
            } => {
                m.validate_write(ports.current_slot, 4)?;
                m.validate_write(devices::CURRENT, 4)?;
                m.read(GuestPtr(pointer), 108)?;
                let gd = if self.worlds.contains_key(&pointer) {
                    self.world(m, p, pointer)?.device
                } else {
                    requested
                };
                owned(m, p, gd, 62)?;
                // Both writes were preflighted. An offscreen world determines its
                // own device; the caller's device argument is deliberately ignored.
                m.write(ports.current_slot, &pointer.to_be_bytes())?;
                m.write(devices::CURRENT, &gd.to_be_bytes())?;
                Ok(0)
            }
            Op::PixMap(pointer) => {
                // GetGWorldPixMap also accepts ordinary color ports.
                if self.worlds.contains_key(&pointer) {
                    return Ok(self.world(m, p, pointer)?.attached[0]);
                }
                m.read(GuestPtr(pointer), 108)?;
                if m.u16(GuestPtr(pointer + 6))? & 0xc000 != 0xc000 {
                    return Err(bad("GetGWorldPixMap requires a color port"));
                }
                let pm = m.u32(GuestPtr(pointer + 2))?;
                owned(m, p, pm, 50)?;
                Ok(pm)
            }
            Op::Device(pointer) => Ok(self.world(m, p, pointer)?.device),
            Op::Dispose(pointer) => {
                let world = self.world(m, p, pointer)?;
                if world.owns_device
                    && self.worlds.iter().any(|(&other, borrower)| {
                        other != pointer && borrower.device == world.device
                    })
                {
                    return Err(bad("dispose GWorld device borrowers before their owner"));
                }
                if m.u32(ports.current_slot)? == pointer || p.pixel_image_is_pinned(world.image) {
                    return Err(bad(
                        "select another port and unlock pixels before DisposeGWorld",
                    ));
                }
                for h in world.attached.into_iter().chain([world.image, world.table]) {
                    if p.pixel_image_is_pinned(h) {
                        return Err(bad("GWorld storage is pinned by another pixel image"));
                    }
                    m.validate_write(GuestPtr(h), 4)?;
                }
                if world.owns_device {
                    let manager = std::mem::take(&mut p.devices);
                    let result = manager.disposal_handles(m, p, world.device);
                    p.devices = manager;
                    result?;
                } else {
                    m.validate_write(GuestPtr(world.table), 4)?;
                }
                port(m, p, Action::Close, pointer)?;
                if world.owns_device {
                    device(m, p, devices::Op::Dispose(world.device))?;
                } else {
                    p.dispose_handle(m, world.table)?;
                }
                p.dispose_handle(m, world.image)?;
                p.dispose_pointer(pointer);
                self.worlds.remove(&pointer);
                Ok(0)
            }
            Op::New {
                output,
                depth,
                bounds,
                table,
                device: requested,
                flags,
            } => {
                m.validate_write(output, 4)?;
                let rect: [u8; 8] = m.read(bounds, 8)?.try_into().unwrap();
                let coordinate = |i| i32::from(i16::from_be_bytes([rect[i], rect[i + 1]]));
                let width = coordinate(6) - coordinate(2);
                let height = coordinate(4) - coordinate(0);
                if width <= 0 || height <= 0 {
                    return Ok((-50i32) as u32);
                }
                if !matches!(depth, 0 | 1 | 2 | 4 | 8 | 16 | 32) {
                    return Ok((-157i32) as u32);
                }
                if flags & !11 != 0 {
                    return Err(bad("temporary/hardware GWorld creation is not implemented"));
                }
                m.validate_write(ports.current_slot, 4)?;
                m.validate_write(devices::CURRENT, 4)?;
                let saved_port = m.u32(ports.current_slot)?;
                let saved_device = device(m, p, devices::Op::Current)?;
                let shared = depth == 0 || flags & 2 != 0;
                let source_device = if depth == 0 {
                    device(m, p, devices::Op::Maximum(bounds))?
                } else if flags & 2 != 0 {
                    requested
                } else {
                    device(m, p, devices::Op::Main)?
                };
                if source_device == 0 {
                    return Ok((-50i32) as u32);
                }
                let gd = owned(m, p, source_device, 62)?;
                let source_pm = owned(m, p, m.u32(GuestPtr(gd.0 + 22))?, 50)?;
                let mut pm = m.read(source_pm, 50)?.to_vec();
                let depth = if shared {
                    m.u16(GuestPtr(source_pm.0 + 32))?
                } else {
                    depth as u16
                };
                if !matches!(depth, 1 | 2 | 4 | 8 | 16 | 32) {
                    return Err(bad("unsupported GWorld device depth"));
                }
                let row = ((width as u32 * u32::from(depth)).div_ceil(32)) * 4;
                if row > 0x3fff {
                    return Ok((-50i32) as u32);
                }
                let colors = if depth > 8 {
                    let mut bytes = vec![0; 16];
                    let bits = if depth == 16 { 5u32 } else { 8 };
                    bytes[..4].copy_from_slice(&(3 * bits).to_be_bytes());
                    bytes
                } else if shared || table != 0 {
                    let table = if shared {
                        m.u32(GuestPtr(source_pm.0 + 42))?
                    } else {
                        table
                    };
                    let data = owned(m, p, table, 8)?;
                    let last = m.u16(GuestPtr(data.0 + 6))? as i16;
                    if !(0..=255).contains(&last) {
                        return Err(bad("invalid GWorld color table"));
                    }
                    let size = 8 + (last as u32 + 1) * 8;
                    owned(m, p, table, size)?;
                    m.read(data, size as usize)?.to_vec()
                } else {
                    crate::color_tables::standard(depth as i16).unwrap()
                };
                let mut image = 0;
                let mut new_port = 0;
                let mut new_device = 0;
                let mut new_table = 0;
                let mut opened = false;
                let creation = (|| -> Result<Option<World>> {
                    image = p.new_handle(m, row * height as u32, true)?;
                    if image == 0 {
                        return Ok(None);
                    }
                    if flags & 1 != 0 {
                        p.set_handle_attribute(m, image, false, true)?;
                    }
                    new_port = p.new_pointer(m, 108, true)?;
                    if new_port == 0 {
                        return Ok(None);
                    }
                    let gd = if shared {
                        source_device
                    } else {
                        new_device = device(
                            m,
                            p,
                            devices::Op::New {
                                reference: 0,
                                mode: -1,
                            },
                        )?;
                        if new_device == 0 {
                            return Ok(None);
                        }
                        new_device
                    };
                    let gd_data = owned(m, p, gd, 62)?;
                    let map = if shared {
                        0
                    } else {
                        m.u32(GuestPtr(gd_data.0 + 22))?
                    };
                    let ct = if shared {
                        new_table = p.new_handle(m, colors.len() as u32, true)?;
                        if new_table == 0 {
                            return Ok(None);
                        }
                        new_table
                    } else {
                        let map_data = owned(m, p, map, 50)?;
                        let ct = m.u32(GuestPtr(map_data.0 + 42))?;
                        p.set_handle_size(m, ct, colors.len() as u32)?;
                        if p.mem_error != 0 {
                            return Ok(None);
                        }
                        ct
                    };
                    let ct_data = owned(m, p, ct, colors.len() as u32)?;
                    m.write(ct_data, &colors)?;
                    pm[..4].copy_from_slice(&image.to_be_bytes());
                    pm[4..6].copy_from_slice(&((row as u16) | 0x8000).to_be_bytes());
                    pm[6..14].copy_from_slice(&rect);
                    pm[14..22].fill(0);
                    pm[30..32].copy_from_slice(&(if depth > 8 { 16u16 } else { 0 }).to_be_bytes());
                    pm[32..34].copy_from_slice(&depth.to_be_bytes());
                    pm[34..36].copy_from_slice(&(if depth > 8 { 3u16 } else { 1 }).to_be_bytes());
                    pm[36..38].copy_from_slice(
                        &(if depth == 16 {
                            5u16
                        } else if depth == 32 {
                            8
                        } else {
                            depth
                        })
                        .to_be_bytes(),
                    );
                    pm[38..42].fill(0);
                    pm[42..46].copy_from_slice(&ct.to_be_bytes());
                    pm[46..50].fill(0);
                    if !shared {
                        let map_data = owned(m, p, map, 50)?;
                        m.write(map_data, &pm)?;
                        m.write(
                            GuestPtr(gd_data.0 + 4),
                            &(if depth > 8 { 2u16 } else { 0 }).to_be_bytes(),
                        )?;
                        m.write(GuestPtr(gd_data.0 + 10), &3u16.to_be_bytes())?;
                        m.write(
                            GuestPtr(gd_data.0 + 20),
                            &(0x4000u16 | u16::from(depth > 1)).to_be_bytes(),
                        )?;
                        m.write(GuestPtr(gd_data.0 + 42), &(-1i32).to_be_bytes())?;
                        m.write(GuestPtr(gd_data.0 + 34), &rect)?;
                        let inverse = m.u32(GuestPtr(gd_data.0 + 6))?;
                        if depth <= 8 {
                            let mut color = std::mem::take(&mut p.color);
                            let result = color.call(
                                m,
                                p,
                                crate::color::Op::Make {
                                    source: ct,
                                    destination: inverse,
                                    resolution: 3,
                                },
                            );
                            let error = color.call(m, p, crate::color::Op::Error);
                            p.color = color;
                            result?;
                            if error? != 0 {
                                return Ok(None);
                            }
                        }
                    }
                    device(m, p, devices::Op::Select(gd))?;
                    port(m, p, Action::Open, new_port)?;
                    if p.mem_error != 0 {
                        return Ok(None);
                    }
                    opened = true;
                    let handles = attached(m, p, new_port)?;
                    let target = owned(m, p, handles[0], 50)?;
                    m.write(target, &pm)?;
                    let visible = owned(m, p, handles[1], 10)?;
                    m.write(GuestPtr(visible.0 + 2), &rect)?;
                    m.write(GuestPtr(new_port + 16), &rect)?;
                    Ok(Some(World {
                        device: gd,
                        owns_device: !shared,
                        image,
                        table: ct,
                        attached: handles,
                    }))
                })();
                // OpenCPort selects its port. NewGWorld must preserve the caller's
                // drawing environment on success as well as allocation failure.
                m.write(ports.current_slot, &saved_port.to_be_bytes())?;
                m.write(devices::CURRENT, &saved_device.to_be_bytes())?;
                if let Ok(Some(world)) = creation {
                    self.worlds.insert(new_port, world);
                    m.write(output, &new_port.to_be_bytes())?;
                    p.mem_error = 0;
                    return Ok(0);
                }
                if opened {
                    port(m, p, Action::Close, new_port)?;
                }
                if new_table != 0 {
                    p.dispose_handle(m, new_table)?;
                }
                if new_device != 0 {
                    device(m, p, devices::Op::Dispose(new_device))?;
                }
                if new_port != 0 {
                    p.dispose_pointer(new_port);
                }
                if image != 0 {
                    p.dispose_handle(m, image)?;
                }
                p.mem_error = -108;
                creation?;
                Ok((-108i32) as u32)
            }
        }
    }
}
