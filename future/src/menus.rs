//! Shared Menu Manager initialization and empty-bar lifecycle. DynamicMenuList
//! and MCEntry layouts follow the legacy Systemless Menu Manager reference.
use crate::{
    bitmap::Bitmap,
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory},
    process::Process,
    region_geometry::{self, Boolean},
    Error, Result,
};
const LIST: GuestPtr = GuestPtr(0xa1c);
const COLORS: GuestPtr = GuestPtr(0xd50);
const HEIGHT: GuestPtr = GuestPtr(0xbaa);
const EMPTY: [u8; 12] = [0; 12];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Init,
    Draw,
    Clear,
    Snapshot,
}
#[derive(Default)]
pub struct Menus {
    list: u32,
    colors: u32,
}
fn bad(message: &str) -> Error {
    Error::Graphics(message.into())
}
fn data(memory: &Memory, process: &mut Process, handle: u32) -> Result<Vec<u8>> {
    let size = process.handle_size(memory, handle)?;
    if process.mem_error != 0 {
        return Err(bad("invalid Menu Manager handle"));
    }
    Ok(memory
        .read(GuestPtr(memory.u32(GuestPtr(handle))?), size as usize)?
        .to_vec())
}
fn incoming(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < 2 {
        return Err(bad("truncated mctb resource"));
    }
    let count = i16::from_be_bytes(bytes[..2].try_into().unwrap());
    if !(0..=1024).contains(&count) || bytes.len() != 2 + count as usize * 30 {
        return Err(bad("invalid mctb entry count or storage"));
    }
    Ok(bytes[2..]
        .chunks_exact(30)
        .filter(|e| e[..2] != (-99i16).to_be_bytes())
        .flatten()
        .copied()
        .collect())
}
fn merge(mut current: Vec<u8>, incoming: &[u8]) -> Result<Vec<u8>> {
    if !current.len().is_multiple_of(30) || current.len() > 30720 {
        return Err(bad("invalid live menu color table"));
    }
    for entry in incoming.chunks_exact(30) {
        if let Some(old) = current
            .chunks_exact_mut(30)
            .find(|old| old[..4] == entry[..4])
        {
            old.copy_from_slice(entry);
        } else {
            current.extend(entry);
        }
    }
    if current.len() > 30720 {
        return Err(bad("menu color table limit exceeded"));
    }
    Ok(current)
}
fn background(colors: &[u8]) -> Result<u8> {
    let Some(entry) = colors.chunks_exact(30).find(|entry| entry[..4] == [0; 4]) else {
        return Ok(0);
    };
    match &entry[22..28] {
        [0, 0, 0, 0, 0, 0] => Ok(1),
        [255, 255, 255, 255, 255, 255] => Ok(0),
        _ => Err(bad("nonmonochrome menu bar background is unsupported")),
    }
}
impl Menus {
    fn check(&self, memory: &Memory, process: &mut Process) -> Result<(Vec<u8>, Vec<u8>)> {
        if self.list == 0 || memory.u32(LIST)? != self.list || memory.u32(COLORS)? != self.colors {
            return Err(bad("Menu Manager globals were replaced or not initialized"));
        }
        let list = data(memory, process, self.list)?;
        if list.len() < 12 {
            return Err(bad("truncated DynamicMenuList"));
        }
        let regular = usize::from(u16::from_be_bytes(list[..2].try_into().unwrap()));
        if !regular.is_multiple_of(6) || regular > 6144 || 12 + regular > list.len() {
            return Err(bad("invalid regular menu list"));
        }
        let hierarchical = usize::from(u16::from_be_bytes(
            list[6 + regular..8 + regular].try_into().unwrap(),
        ));
        if !hierarchical.is_multiple_of(6)
            || hierarchical > 6144
            || 12 + regular + hierarchical != list.len()
        {
            return Err(bad("invalid hierarchical menu list"));
        }
        let colors = merge(data(memory, process, self.colors)?, &[])?;
        Ok((list, colors))
    }
    pub fn call(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&Graphics>,
        op: Op,
    ) -> Result<u32> {
        if op == Op::Init {
            let gpu = gpu.ok_or_else(|| bad("InitMenus requires a GPU"))?;
            let ports = process
                .ports
                .ok_or_else(|| bad("InitMenus requires InitGraf"))?;
            let bitmap = Bitmap::read(memory, ports.screen_bitmap)?;
            memory.validate_write(LIST, 4)?;
            memory.validate_write(COLORS, 4)?;
            memory.validate_write(HEIGHT, 2)?;
            memory.validate_write(GuestPtr(0xa26), 2)?;
            if memory.u16(HEIGHT)? != 20 && memory.u32(crate::windows::LIST)? != 0 {
                return Err(bad(
                    "reserving the menu bar with existing windows is not implemented",
                ));
            }
            let mut colors = if self.list == 0 {
                if memory.u32(LIST)? != 0 || memory.u32(COLORS)? != 0 {
                    return Err(bad("foreign Menu Manager state is unsupported"));
                }
                Vec::new()
            } else {
                if memory.u32(LIST)? != self.list || memory.u32(COLORS)? != self.colors {
                    return Err(bad("Menu Manager globals were replaced"));
                }
                data(memory, process, self.list)?;
                data(memory, process, self.colors)?
            };
            process.res_error = memory.u16(GuestPtr(0xa60))? as i16;
            if process.resources.get(*b"mctb", 0).is_some() {
                let source = process.get_resource(memory, *b"mctb", 0)?;
                if source == 0 {
                    return Ok(0);
                }
                colors = merge(colors, &incoming(&data(memory, process, source)?)?)?;
            } else {
                colors = merge(colors, &[])?;
            }
            background(&colors)?;
            let gray = memory.u32(GuestPtr(0x9ee))?;
            let mut changes = Vec::new();
            if gray != 0 {
                let old = crate::regions::read(memory, process, gray)?;
                let desktop = Rect::new(
                    bitmap.bounds.left,
                    (bitmap.bounds.top + 20).min(bitmap.bounds.bottom),
                    bitmap.bounds.right,
                    bitmap.bounds.bottom,
                );
                changes.push((
                    gray,
                    region_geometry::combine(
                        &old,
                        &region_geometry::encode(&[desktop])?,
                        Boolean::Intersection,
                    )?,
                ));
            }
            let fresh = self.list == 0;
            let (list, color_handle) = if fresh {
                let list = process.new_handle(memory, 12, true)?;
                if list == 0 {
                    return Ok(0);
                }
                let color = process.new_handle(memory, colors.len() as u32, true)?;
                if color == 0 {
                    process.dispose_handle(memory, list)?;
                    process.mem_error = -108;
                    return Ok(0);
                }
                (list, color)
            } else {
                (self.list, self.colors)
            };
            changes.push((list, EMPTY.to_vec()));
            changes.push((color_handle, colors));
            if let Err(error) = process.replace_handles(memory, changes) {
                if fresh {
                    process.dispose_handle(memory, color_handle)?;
                    process.dispose_handle(memory, list)?;
                }
                return Err(error);
            }
            if process.mem_error != 0 {
                if fresh {
                    process.dispose_handle(memory, color_handle)?;
                    process.dispose_handle(memory, list)?;
                    process.mem_error = -108;
                }
                return Ok(0);
            }
            self.list = list;
            self.colors = color_handle;
            memory.write(LIST, &list.to_be_bytes())?;
            memory.write(COLORS, &color_handle.to_be_bytes())?;
            memory.write(HEIGHT, &20u16.to_be_bytes())?;
            memory.write(GuestPtr(0xa26), &0u16.to_be_bytes())?;
            self.draw(memory, process, gpu)?;
            return Ok(0);
        }
        let (list, _) = self.check(memory, process)?;
        match op {
            Op::Snapshot => {
                let handle = process.new_handle(memory, list.len() as u32, false)?;
                if handle != 0 {
                    memory.write(GuestPtr(memory.u32(GuestPtr(handle))?), &list)?;
                }
                Ok(handle)
            }
            Op::Clear => {
                memory.validate_write(GuestPtr(0xa26), 2)?;
                process.replace_handles(memory, vec![(self.list, EMPTY.to_vec())])?;
                if process.mem_error == 0 {
                    memory.write(GuestPtr(0xa26), &0u16.to_be_bytes())?;
                }
                Ok(0)
            }
            Op::Draw => {
                self.draw(
                    memory,
                    process,
                    gpu.ok_or_else(|| bad("DrawMenuBar requires a GPU"))?,
                )?;
                Ok(0)
            }
            Op::Init => unreachable!(),
        }
    }
    fn draw(&self, memory: &mut Memory, process: &mut Process, gpu: &Graphics) -> Result<()> {
        let (list, colors) = self.check(memory, process)?;
        if list[..2] != [0; 2] || list[4..6] != [0; 2] || list[6..] != [0; 6] {
            return Err(bad(
                "populated menu drawing or custom menu-bar definitions are not implemented",
            ));
        }
        let background = background(&colors)?;
        let ports = process
            .ports
            .ok_or_else(|| bad("menu drawing requires InitGraf"))?;
        let bitmap = Bitmap::read(memory, ports.screen_bitmap)?;
        let height = i32::from(memory.u16(HEIGHT)? as i16);
        if height < 0 {
            return Err(bad("negative menu-bar height"));
        }
        let bounds = bitmap.bounds;
        let bar = Rect::new(
            bounds.left,
            bounds.top,
            bounds.right,
            (bounds.top + height).min(bounds.bottom),
        );
        if bar.top >= bar.bottom {
            return Ok(());
        }
        bitmap.execute(
            gpu,
            memory,
            Draw::Fill {
                rect: bar,
                clip: bounds,
                pattern: [0; 8],
                foreground: 1,
                background,
                mode: Transfer::Copy,
            },
        )?;
        let edge = Rect::new(bar.left, bar.bottom - 1, bar.right, bar.bottom);
        let mut shape = vec![edge];
        for (y, width) in [(0, 5), (1, 3), (2, 2), (3, 1), (4, 1)] {
            if bar.top + y >= bar.bottom {
                break;
            }
            shape.push(Rect::new(
                bar.left,
                bar.top + y,
                (bar.left + width).min(bar.right),
                bar.top + y + 1,
            ));
            shape.push(Rect::new(
                (bar.right - width).max(bar.left),
                bar.top + y,
                bar.right,
                bar.top + y + 1,
            ));
        }
        bitmap.execute_clipped(
            gpu,
            memory,
            Draw::Fill {
                rect: bar,
                clip: bounds,
                pattern: [255; 8],
                foreground: 1,
                background: 0,
                mode: Transfer::Copy,
            },
            Some(&shape),
        )
    }
}
