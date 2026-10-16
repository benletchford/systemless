//! Shared Menu Manager resources, bar installation, and GPU title drawing. DynamicMenuList
//! and MCEntry layouts follow the legacy Systemless Menu Manager reference.
use crate::{
    bitmap::Bitmap,
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory},
    process::Process,
    region_geometry::{self, Boolean},
    Error, Result,
};
mod items;
mod list;
pub mod record;
mod resources;
mod tracking;
pub use tracking::Tracking;
const LIST: GuestPtr = GuestPtr(0xa1c);
const COLORS: GuestPtr = GuestPtr(0xd50);
const HEIGHT: GuestPtr = GuestPtr(0xbaa);
const EMPTY: [u8; 12] = [0; 12];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Insert {
        handle: u32,
        before: i16,
    },
    Delete(i16),
    New {
        id: i16,
        title: GuestPtr,
    },
    Append {
        handle: u32,
        source: GuestPtr,
    },
    ReadCommand {
        handle: u32,
        item: i16,
        destination: GuestPtr,
    },
    ReadText {
        handle: u32,
        item: i16,
        destination: GuestPtr,
    },
    Dispose(u32),
    Count(u32),
    Enable {
        handle: u32,
        item: i16,
        enabled: bool,
    },
    Command {
        handle: u32,
        item: i16,
        command: u8,
    },
    Text {
        handle: u32,
        item: i16,
        source: GuestPtr,
    },
    Mark {
        handle: u32,
        item: i16,
        mark: u8,
    },
    AppendResources {
        handle: u32,
        kind: [u8; 4],
    },
    Load(i16),
    Bar(i16),
    Set(u32),
    Handle(i16),
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
        .as_chunks::<30>()
        .0
        .iter()
        .filter(|e| e[..2] != (-99i16).to_be_bytes())
        .flatten()
        .copied()
        .collect())
}
fn merge(mut current: Vec<u8>, incoming: &[u8]) -> Result<Vec<u8>> {
    if !current.len().is_multiple_of(30) || current.len() > 30720 {
        return Err(bad("invalid live menu color table"));
    }
    for entry in incoming.as_chunks::<30>().0 {
        if let Some(old) = current
            .as_chunks_mut::<30>()
            .0
            .iter_mut()
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
    let Some(entry) = colors
        .as_chunks::<30>()
        .0
        .iter()
        .find(|entry| entry[..4] == [0; 4])
    else {
        return Ok(0);
    };
    match &entry[22..28] {
        [0, 0, 0, 0, 0, 0] => Ok(1),
        [255, 255, 255, 255, 255, 255] => Ok(0),
        _ => Err(bad("nonmonochrome menu bar background is unsupported")),
    }
}
fn title_foreground(colors: &[u8], id: i16, bar_background: u8) -> Result<u8> {
    let entry = |id: i16| {
        colors
            .as_chunks::<30>()
            .0
            .iter()
            .find(|e| e[..2] == id.to_be_bytes() && e[2..4] == [0; 2])
    };
    let mono = |rgb: &[u8]| match rgb {
        [0, 0, 0, 0, 0, 0] => Ok(1),
        [255, 255, 255, 255, 255, 255] => Ok(0),
        _ => Err(bad("nonmonochrome menu title colors are unsupported")),
    };
    if let Some(e) = entry(id) {
        if mono(&e[10..16])? != bar_background {
            return Err(bad("distinct menu title backgrounds are unsupported"));
        }
    }
    entry(id)
        .or_else(|| entry(0))
        .map_or(Ok(1), |e| mono(&e[4..10]))
}
fn validate_list(list: &[u8]) -> Result<()> {
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
    Ok(())
}
impl Menus {
    fn read_list(&self, memory: &Memory, process: &mut Process) -> Result<Vec<u8>> {
        if self.list == 0 || memory.u32(LIST)? != self.list {
            return Err(bad("Menu Manager list was replaced or not initialized"));
        }
        let list = data(memory, process, self.list)?;
        validate_list(&list)?;
        Ok(list)
    }
    fn check(&self, memory: &Memory, process: &mut Process) -> Result<(Vec<u8>, Vec<u8>)> {
        let list = self.read_list(memory, process)?;
        if memory.u32(COLORS)? != self.colors {
            return Err(bad("Menu Manager colors were replaced"));
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
        if matches!(op, Op::Insert { .. } | Op::Delete(_)) {
            return self.edit_list(memory, process, op);
        }
        if let Op::Dispose(handle) = op {
            if handle != 0 {
                let bytes = data(memory, process, handle)?;
                record::Record::parse(&bytes)?;
                process.dispose_handle(memory, handle)?;
            }
            return Ok(0);
        }
        if let Op::ReadCommand {
            handle,
            item,
            destination,
        } = op
        {
            return items::read_command(memory, process, handle, item, destination);
        }
        if let Op::ReadText {
            handle,
            item,
            destination,
        } = op
        {
            return items::read_text(memory, process, handle, item, destination);
        }
        if let Op::Append { handle, source } = op {
            return items::append_specs(memory, process, handle, source);
        }
        if let Op::New { id, title } = op {
            let text = if title.0 == 0 {
                Vec::new()
            } else {
                let n = usize::from(memory.read(title, 1)?[0]);
                memory.read(title, n + 1)?[1..].to_vec()
            };
            let mut bytes = vec![0; 14];
            bytes[..2].copy_from_slice(&id.to_be_bytes());
            bytes[10..14].copy_from_slice(&u32::MAX.to_be_bytes());
            bytes.push(text.len() as u8);
            bytes.extend(text);
            bytes.push(0);
            let h = process.new_handle(memory, bytes.len() as u32, false)?;
            if h != 0 {
                memory.write(GuestPtr(memory.u32(GuestPtr(h))?), &bytes)?;
            }
            return Ok(h);
        }
        if let Op::Text {
            handle,
            item,
            source,
        } = op
        {
            return items::set_text(memory, process, handle, item, source);
        }
        if let Op::Command {
            handle,
            item,
            command,
        } = op
        {
            return items::set_byte(memory, process, handle, item, command, 1);
        }
        if let Op::Mark { handle, item, mark } = op {
            return items::set_byte(memory, process, handle, item, mark, 2);
        }
        if let Op::AppendResources { handle, kind } = op {
            return items::append_resources(memory, process, handle, kind);
        }
        if let Op::Enable {
            handle,
            item,
            enabled,
        } = op
        {
            return items::set_enabled(memory, process, handle, item, enabled);
        }
        if let Op::Count(handle) = op {
            let error = process.mem_error;
            let result = (|| {
                let bytes = data(memory, process, handle)?;
                Ok(record::Record::parse(&bytes)?.items.len() as u32)
            })();
            process.mem_error = error;
            return result;
        }
        if let Op::Load(id) | Op::Bar(id) = op {
            return self.load_resources(memory, process, id, matches!(op, Op::Bar(_)));
        }
        if let Op::Set(source) = op {
            let bytes = data(memory, process, source)?;
            validate_list(&bytes)?;
            self.read_list(memory, process)?;
            self.validate_menus(memory, process, &bytes)?;
            memory.validate_write(GuestPtr(0xa26), 2)?;
            process.replace_handles(memory, vec![(self.list, bytes)])?;
            if process.mem_error == 0 {
                memory.write(GuestPtr(0xa26), &0u16.to_be_bytes())?;
            }
            return Ok(0);
        }
        if let Op::Handle(id) = op {
            let error = process.mem_error;
            let result = (|| {
                let list = self.read_list(memory, process)?;
                let regular = usize::from(u16::from_be_bytes(list[..2].try_into().unwrap()));
                for entries in [&list[12 + regular..], &list[6..6 + regular]] {
                    for entry in entries.as_chunks::<6>().0 {
                        let handle = u32::from_be_bytes(entry[..4].try_into().unwrap());
                        let menu = data(memory, process, handle)?;
                        let bytes: [u8; 2] = menu
                            .get(..2)
                            .ok_or_else(|| bad("truncated menu ID"))?
                            .try_into()
                            .unwrap();
                        if i16::from_be_bytes(bytes) == id {
                            return Ok(handle);
                        }
                    }
                }
                Ok(0)
            })();
            process.mem_error = error;
            return result;
        }
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
            if process.find_resource(*b"mctb", 0).is_some() {
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
                let mut empty = EMPTY.to_vec();
                empty[4..6].copy_from_slice(&list[4..6]);
                process.replace_handles(memory, vec![(self.list, empty)])?;
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
            Op::Insert { .. }
            | Op::Delete(_)
            | Op::Dispose(_)
            | Op::ReadCommand { .. }
            | Op::ReadText { .. }
            | Op::Append { .. }
            | Op::New { .. }
            | Op::Command { .. }
            | Op::Init
            | Op::Text { .. }
            | Op::Mark { .. }
            | Op::AppendResources { .. }
            | Op::Enable { .. }
            | Op::Count(_)
            | Op::Handle(_)
            | Op::Load(_)
            | Op::Bar(_)
            | Op::Set(_) => unreachable!(),
        }
    }
    fn draw(&self, memory: &mut Memory, process: &mut Process, gpu: &Graphics) -> Result<()> {
        let (list, colors) = self.check(memory, process)?;
        self.validate_menus(memory, process, &list)?;
        let regular = usize::from(u16::from_be_bytes(list[..2].try_into().unwrap()));
        if list[8 + regular..12 + regular] != [0; 4] {
            return Err(bad("saved menu tracking state is unsupported"));
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
        let mut titles = Vec::new();
        if regular != 0 {
            let bytes = crate::text::font_bytes(memory, process, 0, 0, 0)?;
            let font = crate::font::Strike::parse(&bytes)?;
            let baseline = bar.top
                + (height - i32::from(font.metrics.ascent) - i32::from(font.metrics.descent)) / 2
                + i32::from(font.metrics.ascent);
            for entry in list[6..6 + regular].as_chunks::<6>().0 {
                let bytes = data(
                    memory,
                    process,
                    u32::from_be_bytes(entry[..4].try_into().unwrap()),
                )?;
                let menu = record::Record::parse(&bytes)?;
                let left = i32::from(i16::from_be_bytes(entry[4..6].try_into().unwrap())) + 7;
                let (plans, _) = crate::text::plan_strike(
                    &font,
                    menu.title,
                    i16::try_from(left).map_err(|_| bad("menu title origin overflow"))?,
                    i16::try_from(baseline).map_err(|_| bad("menu baseline overflow"))?,
                )?;
                titles.push((
                    menu.enabled & 1 != 0,
                    title_foreground(&colors, menu.id, background)?,
                    plans,
                ));
            }
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
        )?;
        for (enabled, foreground, plans) in titles {
            for (rect, ink, _) in plans {
                if ink.is_empty() {
                    continue;
                }
                bitmap.execute_clipped(
                    gpu,
                    memory,
                    Draw::Fill {
                        rect,
                        clip: Rect::new(bar.left, bar.top, bar.right, bar.bottom - 1),
                        pattern: if enabled {
                            [255; 8]
                        } else {
                            [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55]
                        },
                        foreground,
                        background,
                        mode: Transfer::Copy,
                    },
                    Some(&ink),
                )?;
            }
        }
        self.draw_highlight(memory, process, gpu)
    }
}
