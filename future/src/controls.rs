//! Shared monochrome button-family controls. Guest records are canonical;
//! registration retains immutable owner/definition identities only.
use crate::{
    graphics::{Graphics, Rect},
    memory::{GuestPtr, Memory},
    process::Process,
    quickdraw::read_rect,
    Error, Result,
};
use std::collections::BTreeMap;
mod geometry;
pub use geometry::Geometry;
mod tracking;
pub use tracking::TrackControl;
pub(crate) use tracking::Tracking;
pub const RECORD_SIZE: u32 = 296;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewControl {
    pub owner: GuestPtr,
    pub bounds: GuestPtr,
    pub title: GuestPtr,
    pub visible: bool,
    pub value: i16,
    pub min: i16,
    pub max: i16,
    pub proc_id: i16,
    pub ref_con: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Geometry {
        handle: u32,
        change: Geometry,
    },
    Find {
        owner: u32,
        point: [i16; 2],
        destination: GuestPtr,
    },
    Draw(u32),
    DrawAll(u32),
    Dispose(u32),
    Kill(u32),
    Visible(u32, bool),
    Hilite(u32, u8),
    Value(u32),
    Min(u32),
    Max(u32),
    SetValue(u32, i16),
    Reference(u32),
    SetReference(u32, u32),
    Title(u32, GuestPtr),
    SetTitle(u32, GuestPtr),
    Test(u32, i16, i16),
}
#[derive(Default)]
pub struct Controls {
    records: BTreeMap<u32, (u32, i16)>,
}
fn fail(s: &str) -> Error {
    Error::Graphics(s.into())
}
fn title(memory: &Memory, p: GuestPtr) -> Result<Vec<u8>> {
    let n = usize::from(memory.read(p, 1)?[0]);
    Ok(memory.read(p, n + 1)?.to_vec())
}
impl Controls {
    pub(crate) fn dialog_button(
        &self,
        m: &Memory,
        p: &mut Process,
        h: u32,
        owner: u32,
    ) -> Result<Option<Rect>> {
        let ptr = self.record(m, p, h)?;
        if self.records[&h] != (owner, 0) {
            return Err(fail("default item is not an owned push button"));
        }
        let r = read_rect(m, GuestPtr(ptr.0 + 8))?;
        if r.left >= r.right || r.top >= r.bottom {
            return Err(fail("invalid default button bounds"));
        }
        Ok((m.read(GuestPtr(ptr.0 + 16), 1)?[0] != 0).then_some(r))
    }

    fn record(&self, memory: &Memory, process: &mut Process, h: u32) -> Result<GuestPtr> {
        let &(owner, _) = self
            .records
            .get(&h)
            .ok_or_else(|| fail("unknown control handle"))?;
        let old_error = process.mem_error;
        let size = process.handle_size(memory, h);
        process.mem_error = old_error;
        if size? < RECORD_SIZE {
            return Err(fail("control record allocation is too small"));
        }
        let p = GuestPtr(memory.u32(GuestPtr(h))?);
        memory.read(p, RECORD_SIZE as usize)?;
        if memory.u32(GuestPtr(p.0 + 4))? != owner {
            return Err(fail("control owner was changed"));
        }
        if memory.u32(GuestPtr(p.0 + 24))? != 0 || memory.u32(GuestPtr(p.0 + 28))? != 0 {
            return Err(fail("custom control definition/data is unsupported"));
        }
        Ok(p)
    }
    fn list(&self, memory: &Memory, process: &mut Process, owner: u32) -> Result<Vec<u32>> {
        let mut list = Vec::new();
        let mut h = memory.u32(GuestPtr(owner + 140))?;
        while h != 0 {
            if list.len() >= 256 || list.contains(&h) {
                return Err(fail("corrupt control list"));
            }
            let p = self.record(memory, process, h)?;
            if self.records[&h].0 != owner {
                return Err(fail("control belongs to another window"));
            }
            list.push(h);
            h = memory.u32(p)?;
        }
        if self.records.values().filter(|&&(w, _)| w == owner).count() != list.len() {
            return Err(fail("registered control is missing from its owner list"));
        }
        Ok(list)
    }
    pub fn create(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&Graphics>,
        request: NewControl,
    ) -> Result<u32> {
        if !matches!(request.proc_id, 0..=2) {
            return Err(fail("unsupported standard or custom CDEF"));
        }
        if !process.windows.contains(memory, request.owner.0)? {
            return Err(fail("control owner is not a live window"));
        }
        let r = read_rect(memory, request.bounds)?;
        if r.left >= r.right || r.top >= r.bottom || request.min > request.max {
            return Err(fail("invalid control bounds/range"));
        }
        let list = self.list(memory, process, request.owner.0)?;
        if list.len() >= 256 {
            return Err(fail("control count limit exceeded"));
        }
        memory.validate_write(GuestPtr(request.owner.0 + 140), 4)?;
        let mut label = title(memory, request.title)?;
        if label[0] != 0 {
            let data = crate::text::font_bytes(memory, process, 0, 12, 0)?;
            let font = crate::font::Strike::parse(&data)?;
            let width = (r.right - r.left - if request.proc_id == 0 { 8 } else { 18 }).max(0);
            let mut out = vec![0];
            for (index, line) in label[1..].split(|&c| c == 13).enumerate() {
                if index != 0 {
                    out.push(13);
                }
                let mut n = 0;
                while n < line.len() && font.width(&line[..n + 1])? <= width {
                    n += 1;
                }
                out.extend(&line[..n]);
            }
            out[0] = (out.len() - 1) as u8;
            label = out;
        }
        let h = process.new_handle(memory, RECORD_SIZE, true)?;
        if h == 0 {
            return Ok(0);
        }
        let p = GuestPtr(memory.u32(GuestPtr(h))?);
        // Built-in definitions dispatch by registration identity. contrlDefProc
        // remains NIL until guest-callable CDEF handles are implemented.
        let mut bytes = vec![0; RECORD_SIZE as usize];
        bytes[..4].copy_from_slice(&list.first().copied().unwrap_or(0).to_be_bytes());
        bytes[4..8].copy_from_slice(&request.owner.0.to_be_bytes());
        bytes[8..16].copy_from_slice(memory.read(request.bounds, 8)?);
        bytes[16] = if request.visible { 255 } else { 0 };
        for (at, value) in [
            (18, request.value.clamp(request.min, request.max)),
            (20, request.min),
            (22, request.max),
        ] {
            bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
        }
        bytes[36..40].copy_from_slice(&request.ref_con.to_be_bytes());
        bytes[40..40 + label.len()].copy_from_slice(&label);
        memory.write(p, &bytes)?;
        self.records.insert(h, (request.owner.0, request.proc_id));
        let drawn = self.draw(memory, process, gpu, h, false);
        if let Err(error) = drawn {
            self.records.remove(&h);
            process.dispose_handle(memory, h)?;
            return Err(error);
        }
        memory.write(GuestPtr(request.owner.0 + 140), &h.to_be_bytes())?;
        process.mem_error = 0;
        Ok(h)
    }
    fn draw(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&Graphics>,
        h: u32,
        erase: bool,
    ) -> Result<()> {
        let p = self.record(memory, process, h)?;
        let r = read_rect(memory, GuestPtr(p.0 + 8))?;
        self.draw_at(memory, process, gpu, h, r, erase)
    }
    fn draw_at(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&Graphics>,
        h: u32,
        r: Rect,
        erase: bool,
    ) -> Result<()> {
        let p = self.record(memory, process, h)?;
        let (owner, kind) = self.records[&h];
        if memory.read(GuestPtr(p.0 + 16), 1)?[0] == 0 {
            return Ok(());
        }
        let ports = process
            .ports
            .ok_or_else(|| fail("control drawing requires InitGraf"))?;
        // Invisible/occluded owner windows have no destination pixels.
        let vis = memory.u32(GuestPtr(owner + 24))?;
        if crate::regions::call(memory, process, crate::regions::Op::Empty, vis, 0)? != 0 {
            return Ok(());
        }
        let gpu = gpu.ok_or_else(|| fail("visible control drawing requires a GPU"))?;
        if r.left >= r.right || r.top >= r.bottom {
            return Err(fail("invalid live control rectangle"));
        }
        let old = memory.u32(ports.current_slot)?;
        let saved = memory.read(GuestPtr(owner), 108)?.to_vec();
        let saved_pattern =
            crate::port_layout::pattern(memory, GuestPtr(owner), crate::port_layout::Pattern::Pen)?;
        let saved_background = crate::port_layout::pattern(
            memory,
            GuestPtr(owner),
            crate::port_layout::Pattern::Background,
        )?;
        let clip = memory.u32(GuestPtr(owner + 28))?;
        let old_clip = crate::regions::read(memory, process, clip)?;
        let mut clipped = crate::region_geometry::decode(&old_clip)?;
        for c in &mut clipped {
            c.left = c.left.max(r.left);
            c.top = c.top.max(r.top);
            c.right = c.right.min(r.right);
            c.bottom = c.bottom.min(r.bottom);
        }
        clipped.retain(|c| c.left < c.right && c.top < c.bottom);
        let clipped = crate::region_geometry::encode(&clipped)?;
        let data = if erase {
            Vec::new()
        } else {
            crate::text::font_bytes(memory, process, 0, 12, 0)?
        };
        let label = title(memory, GuestPtr(p.0 + 40))?;
        let hilite = memory.read(GuestPtr(p.0 + 17), 1)?[0];
        let value = memory.u16(GuestPtr(p.0 + 18))? as i16;
        if !erase && !matches!(hilite, 0 | 10 | 11 | 255) {
            return Err(fail("unsupported control highlight part"));
        }
        let result = (|| {
            process.set_handle_size(memory, clip, clipped.len() as u32)?;
            if process.mem_error != 0 {
                return Err(fail("control clip allocation failed"));
            }
            memory.write(GuestPtr(memory.u32(GuestPtr(clip))?), &clipped)?;
            ports.set(memory, GuestPtr(owner))?;
            if erase {
                memory.write(GuestPtr(owner + 80), &saved[80..88])?;
            } else {
                memory.write(GuestPtr(owner + 80), &33u32.to_be_bytes())?;
                memory.write(GuestPtr(owner + 84), &30u32.to_be_bytes())?;
            }
            memory.write(GuestPtr(owner + 52), &[0, 1, 0, 1])?;
            memory.write(GuestPtr(owner + 56), &8u16.to_be_bytes())?;
            let background: [u8; 8] = if erase { saved_background } else { [0; 8] };
            crate::port_layout::write_pattern(
                memory,
                GuestPtr(owner),
                crate::port_layout::Pattern::Pen,
                &background,
            )?;
            ports.paint_coverage(gpu, memory, r, None)?;
            if erase {
                return Ok(());
            }
            let inactive = hilite == 255;
            let pressed = hilite != 0 && !inactive;
            let pattern = if inactive {
                [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55]
            } else {
                [255; 8]
            };
            crate::port_layout::write_pattern(
                memory,
                GuestPtr(owner),
                crate::port_layout::Pattern::Pen,
                &pattern,
            )?;
            let (indicator, label_left) = if kind == 0 {
                (r, r.left + 4)
            } else {
                let size = 12.min(r.bottom - r.top);
                let y = r.top + (r.bottom - r.top - size) / 2;
                (
                    Rect::new(r.left + 2, y, r.left + 2 + size, y + size),
                    r.left + size + 6,
                )
            };
            if kind == 0 {
                ports.round_rect(gpu, memory, r, 16, 16, !pressed)?;
            } else if kind == 1 {
                let inner = Rect::new(
                    indicator.left + 1,
                    indicator.top + 1,
                    indicator.right - 1,
                    indicator.bottom - 1,
                );
                let outer = crate::region_geometry::encode(&[indicator])?;
                let inside = crate::region_geometry::encode(&[inner])?;
                let ring = crate::region_geometry::combine(
                    &outer,
                    &inside,
                    crate::region_geometry::Boolean::Difference,
                )?;
                ports.paint_coverage(
                    gpu,
                    memory,
                    indicator,
                    Some(&crate::region_geometry::decode(&ring)?),
                )?;
                if value != 0 {
                    let n = indicator.right - indicator.left - 2;
                    let mut mark = Vec::new();
                    for i in 0..n {
                        mark.push(Rect::new(
                            indicator.left + 1 + i,
                            indicator.top + 1 + i,
                            indicator.left + 2 + i,
                            indicator.top + 2 + i,
                        ));
                        if 2 * i != n - 1 {
                            mark.push(Rect::new(
                                indicator.right - 2 - i,
                                indicator.top + 1 + i,
                                indicator.right - 1 - i,
                                indicator.top + 2 + i,
                            ));
                        }
                    }
                    ports.paint_coverage(gpu, memory, indicator, Some(&mark))?;
                }
            } else {
                let size = (indicator.right - indicator.left) as i16;
                ports.round_rect(gpu, memory, indicator, size, size, true)?;
                if value != 0 {
                    let dot = Rect::new(
                        indicator.left + 3,
                        indicator.top + 3,
                        indicator.right - 3,
                        indicator.bottom - 3,
                    );
                    ports.round_rect(gpu, memory, dot, size - 6, size - 6, false)?;
                }
            }
            if pressed && kind != 0 {
                memory.write(GuestPtr(owner + 56), &10u16.to_be_bytes())?;
                crate::port_layout::write_pattern(
                    memory,
                    GuestPtr(owner),
                    crate::port_layout::Pattern::Pen,
                    &[255; 8],
                )?;
                if kind == 2 {
                    let size = (indicator.right - indicator.left) as i16;
                    ports.round_rect(gpu, memory, indicator, size, size, false)?;
                } else {
                    ports.paint_coverage(gpu, memory, indicator, None)?;
                }
            }
            let font = crate::font::Strike::parse(&data)?;
            memory.write(GuestPtr(owner + 72), &1u16.to_be_bytes())?;
            if pressed && kind == 0 {
                memory.write(GuestPtr(owner + 80), &30u32.to_be_bytes())?;
                memory.write(GuestPtr(owner + 84), &33u32.to_be_bytes())?;
            }
            let lines: Vec<_> = label[1..].split(|&c| c == 13).collect();
            let height = i32::from(font.metrics.ascent + font.metrics.descent);
            let step = height + i32::from(font.metrics.leading);
            let block = height + step * (lines.len() as i32 - 1);
            let mut baseline =
                r.top + (r.bottom - r.top - block) / 2 + i32::from(font.metrics.ascent);
            for line in lines {
                let x = if kind == 0 {
                    r.left + (r.right - r.left - font.width(line)?) / 2
                } else {
                    label_left
                };
                crate::text::move_to(
                    memory,
                    ports,
                    i16::try_from(x).map_err(|_| fail("control title overflow"))?,
                    i16::try_from(baseline).map_err(|_| fail("control title overflow"))?,
                )?;
                crate::text::draw_strike(memory, ports, gpu, line, &font)?;
                baseline += step;
            }
            if inactive {
                memory.write(GuestPtr(owner + 56), &11u16.to_be_bytes())?;
                crate::port_layout::write_pattern(
                    memory,
                    GuestPtr(owner),
                    crate::port_layout::Pattern::Pen,
                    &[0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
                )?;
                let label_rect = Rect::new(label_left, r.top + 2, r.right - 4, r.bottom - 2);
                ports.paint_coverage(gpu, memory, label_rect, None)?;
            }
            Ok(())
        })();
        process.set_handle_size(memory, clip, old_clip.len() as u32)?;
        if process.mem_error != 0 {
            return Err(fail("control clip restore failed"));
        }
        memory.write(GuestPtr(memory.u32(GuestPtr(clip))?), &old_clip)?;
        memory.write(GuestPtr(owner), &saved)?;
        crate::port_layout::write_pattern(
            memory,
            GuestPtr(owner),
            crate::port_layout::Pattern::Pen,
            &saved_pattern,
        )?;
        memory.write(ports.current_slot, &old.to_be_bytes())?;
        result
    }
    fn dispose(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&Graphics>,
        h: u32,
    ) -> Result<()> {
        let p = self.record(memory, process, h)?;
        let owner = self.records[&h].0;
        let list = self.list(memory, process, owner)?;
        let index = list
            .iter()
            .position(|&c| c == h)
            .ok_or_else(|| fail("control is unlinked"))?;
        let previous = if index == 0 {
            GuestPtr(owner + 140)
        } else {
            self.record(memory, process, list[index - 1])?
        };
        memory.validate_write(previous, 4)?;
        memory.validate_write(GuestPtr(h), 4)?;
        self.draw(memory, process, gpu, h, true)?;
        memory.write(previous, &memory.u32(p)?.to_be_bytes())?;
        process.dispose_handle(memory, h)?;
        self.records.remove(&h);
        Ok(())
    }
    /// Window teardown has already validated the owner; validate every control
    /// before freeing any. No drawing is needed when the whole window closes.
    pub fn close_window(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        owner: u32,
    ) -> Result<()> {
        let list = self.list(memory, process, owner)?;
        memory.validate_write(GuestPtr(owner + 140), 4)?;
        for &h in &list {
            memory.validate_write(GuestPtr(h), 4)?;
        }
        for h in list {
            process.dispose_handle(memory, h)?;
            self.records.remove(&h);
        }
        memory.write(GuestPtr(owner + 140), &0u32.to_be_bytes())?;
        Ok(())
    }
    pub fn call(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&Graphics>,
        op: Op,
    ) -> Result<u32> {
        if let Op::Geometry { handle, change } = op {
            return self.change_geometry(memory, process, gpu, handle, change);
        }
        if let Op::Find {
            owner,
            point,
            destination,
        } = op
        {
            memory.validate_write(destination, 4)?;
            if !process.windows.contains(memory, owner)? {
                return Err(fail("control owner is not a live window"));
            }
            let list = self.list(memory, process, owner)?;
            let mut found = (0u32, 0u32);
            for h in list {
                let part = self.call(memory, process, None, Op::Test(h, point[1], point[0]))?;
                if part != 0 {
                    found = (part, h);
                    break;
                }
            }
            memory.write(destination, &found.1.to_be_bytes())?;
            return Ok(found.0);
        }
        if let Op::DrawAll(owner) | Op::Kill(owner) = op {
            if !process.windows.contains(memory, owner)? {
                return Err(fail("control owner is not a live window"));
            }
            let list = self.list(memory, process, owner)?;
            for h in list {
                if matches!(op, Op::Kill(_)) {
                    self.dispose(memory, process, gpu, h)?;
                } else {
                    self.draw(memory, process, gpu, h, false)?;
                }
            }
            return Ok(0);
        }
        let h = match op {
            Op::Draw(h)
            | Op::Dispose(h)
            | Op::Visible(h, _)
            | Op::Hilite(h, _)
            | Op::Value(h)
            | Op::Min(h)
            | Op::Max(h)
            | Op::SetValue(h, _)
            | Op::Reference(h)
            | Op::SetReference(h, _)
            | Op::Title(h, _)
            | Op::SetTitle(h, _)
            | Op::Test(h, _, _) => h,
            _ => unreachable!(),
        };
        let p = self.record(memory, process, h)?;
        match op {
            Op::Value(_) | Op::Min(_) | Op::Max(_) => {
                let offset = if matches!(op, Op::Value(_)) {
                    18
                } else if matches!(op, Op::Min(_)) {
                    20
                } else {
                    22
                };
                return Ok(memory.u16(GuestPtr(p.0 + offset))? as i16 as i32 as u32);
            }
            Op::Reference(_) => return memory.u32(GuestPtr(p.0 + 36)),
            Op::SetReference(_, value) => memory.write(GuestPtr(p.0 + 36), &value.to_be_bytes())?,
            Op::Title(_, out) => {
                let label = title(memory, GuestPtr(p.0 + 40))?;
                memory.write(out, &label)?;
            }
            Op::SetTitle(_, input) => {
                let label = title(memory, input)?;
                memory.validate_write(GuestPtr(p.0 + 40), 256)?;
                memory.write(GuestPtr(p.0 + 40), &[0; 256])?;
                memory.write(GuestPtr(p.0 + 40), &label)?;
                self.draw(memory, process, gpu, h, false)?;
            }
            Op::SetValue(_, value) => {
                let min = memory.u16(GuestPtr(p.0 + 20))? as i16;
                let max = memory.u16(GuestPtr(p.0 + 22))? as i16;
                if min > max {
                    return Err(fail("invalid live control range"));
                }
                memory.write(GuestPtr(p.0 + 18), &value.clamp(min, max).to_be_bytes())?;
                self.draw(memory, process, gpu, h, false)?;
            }
            Op::Hilite(_, part) => {
                if !matches!(part, 0 | 10 | 11 | 255) {
                    return Err(fail("unsupported control highlight part"));
                }
                memory.write(GuestPtr(p.0 + 17), &[part])?;
                self.draw(memory, process, gpu, h, false)?;
            }
            Op::Visible(_, visible) => {
                if (memory.read(GuestPtr(p.0 + 16), 1)?[0] != 0) == visible {
                    return Ok(0);
                }
                if !visible && memory.read(GuestPtr(p.0 + 16), 1)?[0] != 0 {
                    self.draw(memory, process, gpu, h, true)?;
                }
                memory.write(GuestPtr(p.0 + 16), &[if visible { 255 } else { 0 }])?;
                if visible {
                    self.draw(memory, process, gpu, h, false)?;
                }
            }
            Op::Draw(_) => self.draw(memory, process, gpu, h, false)?,
            Op::Dispose(_) => self.dispose(memory, process, gpu, h)?,
            Op::Test(control, x, y) => {
                let r = read_rect(memory, GuestPtr(p.0 + 8))?;
                let active = memory.read(GuestPtr(p.0 + 16), 1)?[0] != 0
                    && memory.read(GuestPtr(p.0 + 17), 1)?[0] != 255;
                return Ok(
                    if active
                        && i32::from(x) >= r.left
                        && i32::from(x) < r.right
                        && i32::from(y) >= r.top
                        && i32::from(y) < r.bottom
                    {
                        if self.records[&control].1 == 0 {
                            10
                        } else {
                            11
                        }
                    } else {
                        0
                    },
                );
            }
            _ => unreachable!(),
        }
        Ok(0)
    }
}
