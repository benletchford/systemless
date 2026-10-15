//! Shared TextEdit records and ownership. Layout follows MPW TextEdit.h and
//! legacy loader/ppc/textedit.rs; editing and callbacks are separate stages.
use crate::{
    memory::{GuestPtr, Memory},
    process::Process,
    quickdraw::Ports,
    Error, Result,
};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextBox {
    pub text: GuestPtr,
    pub length: i32,
    pub rectangle: GuestPtr,
    pub alignment: i16,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    TextBox(TextBox),
    Init,
    New {
        dest: GuestPtr,
        view: GuestPtr,
        styled: bool,
    },
    GetText(u32),
    Height {
        handle: u32,
        start: i32,
        end: i32,
    },
    GetStyle(u32),
    Dispose(u32),
    SetText {
        source: GuestPtr,
        length: u32,
        handle: u32,
    },
    Calculate(u32),
    Delete(u32),
    Key {
        handle: u32,
        byte: u8,
    },
    Idle(u32),
    Activate {
        handle: u32,
        active: bool,
    },
    Select {
        start: u32,
        end: u32,
        handle: u32,
    },
    Update {
        rect: GuestPtr,
        handle: u32,
    },
}
#[derive(Default)]
pub struct Manager {
    scrap: u32,
    records: BTreeMap<u32, Vec<u32>>,
}
fn bad(message: &str) -> Error {
    Error::Graphics(message.into())
}
fn pointer(memory: &Memory, process: &mut Process, handle: u32, size: u32) -> Result<GuestPtr> {
    if process.handle_size(memory, handle)? < size || process.mem_error != 0 {
        return Err(bad("invalid TextEdit handle storage"));
    }
    let address = GuestPtr(memory.u32(GuestPtr(handle))?);
    memory.read(address, size as usize)?;
    Ok(address)
}
fn write_handle(
    memory: &mut Memory,
    process: &mut Process,
    handle: u32,
    bytes: &[u8],
) -> Result<()> {
    let data = pointer(memory, process, handle, bytes.len() as u32)?;
    memory.write(data, bytes)
}
fn validate_overlay_hooks(memory: &Memory, data: GuestPtr) -> Result<()> {
    for offset in [66, 86, 90] {
        if memory.u32(GuestPtr(data.0 + offset))? != 0 {
            return Err(bad(
                "custom TextEdit selection/caret callbacks are unsupported",
            ));
        }
    }
    Ok(())
}
impl Manager {
    /// Inclusive one-based line heights, read from live layout storage. Zero or
    /// negative endpoints denote line one; reversed endpoints are sorted before
    /// intersection with the actual nonblank line range. Never invent metrics
    /// for lines beyond the record, as legacy out-of-range loops could do.
    fn height(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        handle: u32,
        start: i32,
        end: i32,
    ) -> Result<u32> {
        let errors = (process.mem_error, process.res_error);
        let result = (|| {
            self.call(memory, process, Op::GetText(handle), 0)?;
            let owned = self.records[&handle].clone();
            let data = pointer(memory, process, handle, 128)?;
            let length = usize::from(memory.u16(GuestPtr(data.0 + 60))?);
            let count = usize::from(memory.u16(GuestPtr(data.0 + 94))?);
            if length > 32767 || count > 32767 {
                return Err(bad("invalid TextEdit line/text count"));
            }
            let record = pointer(
                memory,
                process,
                handle,
                (128.max(96 + (count + 1) * 2)) as u32,
            )?;
            let text = pointer(memory, process, owned[1], length as u32)?;
            let bytes = memory.read(text, length)?;
            let starts_bytes = memory.read(GuestPtr(record.0 + 96), (count + 1) * 2)?;
            let starts: Vec<usize> = starts_bytes
                .chunks_exact(2)
                .map(|b| usize::from(u16::from_be_bytes(b.try_into().unwrap())))
                .collect();
            if starts[0] != 0
                || starts[count] != length
                || starts.windows(2).any(|p| p[0] > p[1] || p[1] > length)
            {
                return Err(bad("invalid TextEdit line starts"));
            }
            let metrics = if owned.len() == 7 {
                let p = pointer(memory, process, owned[3], ((count + 1) * 4) as u32)?;
                memory
                    .read(p, (count + 1) * 4)?
                    .chunks_exact(4)
                    .take(count)
                    .map(|b| i16::from_be_bytes(b[..2].try_into().unwrap()))
                    .collect::<Vec<_>>()
            } else {
                vec![memory.u16(GuestPtr(data.0 + 24))? as i16; count]
            };
            if metrics.iter().any(|&h| h <= 0) {
                return Err(bad("invalid TextEdit line height"));
            }
            let mut effective = count;
            while effective > 0
                && bytes[starts[effective - 1]..starts[effective]]
                    .iter()
                    .all(|&c| matches!(c, b'\r' | b'\n'))
            {
                effective -= 1;
            }
            let a = start.max(1) as usize;
            let b = end.max(1) as usize;
            let first = a.min(b) - 1;
            let last = a.max(b).min(effective);
            if first >= last {
                return Ok(0);
            }
            metrics[first..last].iter().try_fold(0u32, |total, &h| {
                total
                    .checked_add(h as u32)
                    .ok_or_else(|| bad("TextEdit height overflow"))
            })
        })();
        (process.mem_error, process.res_error) = errors;
        result
    }
    pub fn call(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        op: Op,
        ticks: u32,
    ) -> Result<u32> {
        match op {
            Op::Height { handle, start, end } => self.height(memory, process, handle, start, end),
            Op::Init => {
                if self.scrap != 0 {
                    return Ok(0);
                }
                memory.validate_write(GuestPtr(0xab0), 2)?;
                memory.validate_write(GuestPtr(0xab4), 4)?;
                let scrap = process.new_handle(memory, 0, true)?;
                if scrap == 0 {
                    return Err(bad("TextEdit scrap allocation failed"));
                }
                memory.write(GuestPtr(0xab0), &0u16.to_be_bytes())?;
                memory.write(GuestPtr(0xab4), &scrap.to_be_bytes())?;
                self.scrap = scrap;
                Ok(0)
            }
            Op::New { dest, view, styled } => {
                let dest: Vec<u8> = memory.read(dest, 8)?.to_vec();
                let view: Vec<u8> = memory.read(view, 8)?.to_vec();
                let ports: Ports = process
                    .ports
                    .ok_or_else(|| bad("TextEdit requires InitGraf"))?;
                let port = ports.current(memory)?;
                let attrs = memory.read(GuestPtr(port.0 + 68), 8)?.to_vec();
                let color = memory.u32(GuestPtr(port.0 + 80))?;
                let component = match color {
                    33 => 0u16,
                    30 => u16::MAX,
                    _ => return Err(bad("TextEdit style color is unsupported")),
                };
                let metrics = crate::text::metrics(memory, process, ports)?;
                let height = i16::try_from(
                    i32::from(metrics.ascent)
                        + i32::from(metrics.descent)
                        + i32::from(metrics.leading),
                )
                .map_err(|_| bad("TextEdit line-height overflow"))?;
                if height <= 0 {
                    return Err(bad("TextEdit requires a positive line height"));
                }
                let sizes: &[u32] = if styled {
                    &[128, 0, 18, 4, 22, 8, 28]
                } else {
                    &[128, 0]
                };
                let mut owned = Vec::new();
                for &size in sizes {
                    let handle = process.new_handle(memory, size, true)?;
                    if handle == 0 {
                        for handle in owned {
                            process.dispose_handle(memory, handle)?;
                        }
                        process.mem_error = -108;
                        return Ok(0);
                    }
                    owned.push(handle);
                }
                let mut record = vec![0; 128];
                record[..8].copy_from_slice(&dest);
                record[8..16].copy_from_slice(&view);
                record[16..24].copy_from_slice(&dest);
                record[28..32].copy_from_slice(&dest[..4]);
                record[24..26].copy_from_slice(&height.to_be_bytes());
                record[26..28].copy_from_slice(&metrics.ascent.to_be_bytes());
                record[52..56].copy_from_slice(&ticks.to_be_bytes());
                record[62..66].copy_from_slice(&owned[1].to_be_bytes());
                record[74..76].copy_from_slice(&attrs[..2]);
                record[76] = attrs[2];
                record[78..82].copy_from_slice(&attrs[4..8]);
                record[82..86].copy_from_slice(&port.0.to_be_bytes());
                if styled {
                    let mut style = vec![0; 18];
                    style[..2].copy_from_slice(&1u16.to_be_bytes());
                    style[2..4].copy_from_slice(&height.to_be_bytes());
                    style[4..6].copy_from_slice(&metrics.ascent.to_be_bytes());
                    style[6..8].copy_from_slice(&attrs[..2]);
                    style[8] = attrs[2];
                    style[10..12].copy_from_slice(&attrs[6..8]);
                    for offset in [12, 14, 16] {
                        style[offset..offset + 2].copy_from_slice(&component.to_be_bytes());
                    }
                    write_handle(memory, process, owned[2], &style)?;
                    let mut line = height.to_be_bytes().to_vec();
                    line.extend(metrics.ascent.to_be_bytes());
                    write_handle(memory, process, owned[3], &line)?;
                    let mut scrap = vec![0; 22];
                    scrap[6..10].copy_from_slice(&style[2..6]);
                    scrap[10..12].copy_from_slice(&style[6..8]);
                    scrap[12] = style[8];
                    scrap[14..22].copy_from_slice(&style[10..18]);
                    write_handle(memory, process, owned[4], &scrap)?;
                    let mut null = vec![0; 8];
                    null[4..8].copy_from_slice(&owned[4].to_be_bytes());
                    write_handle(memory, process, owned[5], &null)?;
                    let mut table = vec![0; 28];
                    table[..2].copy_from_slice(&1u16.to_be_bytes());
                    table[2..4].copy_from_slice(&1u16.to_be_bytes());
                    table[4..8].copy_from_slice(&owned[2].to_be_bytes());
                    table[8..12].copy_from_slice(&owned[3].to_be_bytes());
                    table[16..20].copy_from_slice(&owned[5].to_be_bytes());
                    table[24..26].copy_from_slice(&1u16.to_be_bytes());
                    table[26..28].copy_from_slice(&(-1i16).to_be_bytes());
                    write_handle(memory, process, owned[6], &table)?;
                    record[24..28].fill(255);
                    record[74..78].copy_from_slice(&owned[6].to_be_bytes());
                    record[80..82].fill(255);
                }
                write_handle(memory, process, owned[0], &record)?;
                let handle = owned[0];
                self.records.insert(handle, owned);
                process.mem_error = 0;
                Ok(handle)
            }
            Op::TextBox(_)
            | Op::Key { .. }
            | Op::Activate { .. }
            | Op::Idle(_)
            | Op::Delete(_)
            | Op::Update { .. } => Err(bad("TextEdit redraw requires the graphics route")),
            Op::SetText {
                source,
                length,
                handle,
            } => {
                if length > 32767 {
                    return Err(bad("TextEdit text length exceeds 32767"));
                }
                let bytes = if length == 0 {
                    vec![]
                } else {
                    memory.read(source, length as usize)?.to_vec()
                };
                self.layout(memory, process, handle, bytes, true, None)
            }
            Op::Select { start, end, handle } => {
                self.select_storage(memory, process, handle, (start, end), false)
            }
            Op::Calculate(handle) => {
                self.call(memory, process, Op::GetText(handle), ticks)?;
                let data = pointer(memory, process, handle, 128)?;
                let length = memory.u16(GuestPtr(data.0 + 60))?;
                if length > 32767 {
                    return Err(bad("invalid TextEdit text length"));
                }
                let text = memory.u32(GuestPtr(data.0 + 62))?;
                let data = pointer(memory, process, text, u32::from(length))?;
                let bytes = memory.read(data, usize::from(length))?.to_vec();
                self.layout(memory, process, handle, bytes, false, None)
            }
            Op::GetText(handle) | Op::GetStyle(handle) | Op::Dispose(handle) => {
                let owned = self
                    .records
                    .get(&handle)
                    .ok_or_else(|| bad("unknown TextEdit record"))?;
                let data = pointer(memory, process, handle, 128)?;
                if memory.u32(GuestPtr(data.0 + 62))? != owned[1] {
                    return Err(bad("TextEdit text-handle replacement is unsupported"));
                }
                let styled = owned.len() == 7;
                if (memory.u16(GuestPtr(data.0 + 80))? == 0xffff) != styled {
                    return Err(bad("TextEdit variant sentinel was changed"));
                }
                if styled {
                    if memory.u16(GuestPtr(data.0 + 80))? != 0xffff
                        || memory.u32(GuestPtr(data.0 + 74))? != owned[6]
                    {
                        return Err(bad("corrupt TextEdit style overlay"));
                    }
                    let table = pointer(memory, process, owned[6], 28)?;
                    let null = pointer(memory, process, owned[5], 8)?;
                    for (offset, expected) in [(4, owned[2]), (8, owned[3]), (16, owned[5])] {
                        if memory.u32(GuestPtr(table.0 + offset))? != expected {
                            return Err(bad("TextEdit style-handle replacement is unsupported"));
                        }
                    }
                    if memory.u32(GuestPtr(null.0 + 4))? != owned[4] {
                        return Err(bad("TextEdit scrap-handle replacement is unsupported"));
                    }
                }
                if matches!(op, Op::GetText(_)) {
                    return Ok(owned[1]);
                }
                if matches!(op, Op::GetStyle(_)) {
                    return Ok(if styled { owned[6] } else { 0 });
                }
                for &nested in owned {
                    pointer(memory, process, nested, 0)?;
                    memory.validate_write(GuestPtr(nested), 4)?;
                }
                let owned = self.records.remove(&handle).unwrap();
                for nested in owned.into_iter().rev() {
                    process.dispose_handle(memory, nested)?;
                }
                process.mem_error = 0;
                Ok(0)
            }
        }
    }
    fn select_storage(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        handle: u32,
        range: (u32, u32),
        allow_active: bool,
    ) -> Result<u32> {
        self.call(memory, process, Op::GetText(handle), 0)?;
        let data = pointer(memory, process, handle, 128)?;
        let active = memory.u16(GuestPtr(data.0 + 36))? != 0;
        if active && !allow_active {
            return Err(bad("active TextEdit selection requires the graphics route"));
        }
        if active {
            validate_overlay_hooks(memory, data)?;
        }
        let length = memory.u16(GuestPtr(data.0 + 60))?;
        if length > 32767 {
            return Err(bad("invalid TextEdit text length"));
        }
        let start = range.0.min(u32::from(length)) as u16;
        let end = range.1.min(u32::from(length)) as u16;
        let mut selection = start.to_be_bytes().to_vec();
        selection.extend(end.to_be_bytes());
        memory.write(GuestPtr(data.0 + 32), &selection)?;
        Ok(0)
    }
    pub fn select(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&crate::graphics::Graphics>,
        handle: u32,
        range: (u32, u32),
        ticks: u32,
    ) -> Result<()> {
        self.call(memory, process, Op::GetText(handle), 0)?;
        let data = pointer(memory, process, handle, 128)?;
        let active = memory.u16(GuestPtr(data.0 + 36))? != 0;
        let gpu = if active {
            Some(gpu.ok_or_else(|| bad("active TextEdit selection requires a GPU"))?)
        } else {
            None
        };
        if active {
            memory.validate_write(GuestPtr(data.0 + 52), 6)?;
        }
        self.select_storage(memory, process, handle, range, true)?;
        if let Some(gpu) = gpu {
            let mut caret = ticks.to_be_bytes().to_vec();
            caret.extend(1u16.to_be_bytes());
            memory.write(GuestPtr(data.0 + 52), &caret)?;
            self.redraw(memory, process, gpu, GuestPtr(data.0 + 8), handle, true)?;
        }
        Ok(())
    }
    pub fn idle(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&crate::graphics::Graphics>,
        handle: u32,
        ticks: u32,
    ) -> Result<()> {
        self.call(memory, process, Op::GetText(handle), 0)?;
        let data = pointer(memory, process, handle, 128)?;
        if memory.u16(GuestPtr(data.0 + 36))? == 0
            || memory.u16(GuestPtr(data.0 + 32))? != memory.u16(GuestPtr(data.0 + 34))?
        {
            return Ok(());
        }
        let previous = memory.u32(GuestPtr(data.0 + 52))?;
        let interval = memory.u32(GuestPtr(0x2f4))?;
        if ticks.wrapping_sub(previous) < interval {
            return Ok(());
        }
        let gpu = gpu.ok_or_else(|| bad("TextEdit caret blinking requires a GPU"))?;
        validate_overlay_hooks(memory, data)?;
        let visible = memory.u16(GuestPtr(data.0 + 56))? == 0;
        let mut caret = ticks.to_be_bytes().to_vec();
        caret.extend(u16::from(visible).to_be_bytes());
        memory.write(GuestPtr(data.0 + 52), &caret)?;
        self.redraw(memory, process, gpu, GuestPtr(data.0 + 8), handle, true)
    }
    pub fn activate(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &crate::graphics::Graphics,
        handle: u32,
        active: bool,
        ticks: u32,
    ) -> Result<()> {
        self.call(memory, process, Op::GetText(handle), 0)?;
        let data = pointer(memory, process, handle, 128)?;
        validate_overlay_hooks(memory, data)?;
        memory.validate_write(GuestPtr(data.0 + 36), 2)?;
        memory.validate_write(GuestPtr(data.0 + 52), 6)?;
        memory.write(GuestPtr(data.0 + 36), &u16::from(active).to_be_bytes())?;
        let mut caret = ticks.to_be_bytes().to_vec();
        caret.extend(u16::from(active).to_be_bytes());
        memory.write(GuestPtr(data.0 + 52), &caret)?;
        self.redraw(memory, process, gpu, GuestPtr(data.0 + 8), handle, true)
    }
    pub fn key(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &crate::graphics::Graphics,
        handle: u32,
        byte: u8,
        ticks: u32,
    ) -> Result<()> {
        if matches!(byte, 0x1e | 0x1f) {
            return Err(bad("vertical TextEdit key navigation is not implemented"));
        }
        self.call(memory, process, Op::GetText(handle), 0)?;
        let data = pointer(memory, process, handle, 128)?;
        if memory.u16(GuestPtr(data.0 + 36))? != 0 {
            validate_overlay_hooks(memory, data)?;
        }
        let length = memory.u16(GuestPtr(data.0 + 60))?;
        if length > 32767 {
            return Err(bad("invalid TextEdit text length"));
        }
        let a = memory.u16(GuestPtr(data.0 + 32))?.min(length);
        let b = memory.u16(GuestPtr(data.0 + 34))?.min(length);
        let mut start = a.min(b);
        let end = a.max(b);
        if matches!(byte, 0x1c | 0x1d) {
            let caret = if start != end {
                if byte == 0x1c {
                    start
                } else {
                    end
                }
            } else if byte == 0x1c {
                start.saturating_sub(1)
            } else {
                end.saturating_add(1).min(length)
            };
            return self.select(
                memory,
                process,
                Some(gpu),
                handle,
                (u32::from(caret), u32::from(caret)),
                ticks,
            );
        }
        let deletion = matches!(byte, 0x08 | 0x7f);
        if deletion && start == end {
            start = start.saturating_sub(1);
        }
        let added = u16::from(!deletion);
        let new_length = length - (end - start) + added;
        if new_length > 32767 {
            return Err(bad("TextEdit text length exceeds 32767"));
        }
        let text = memory.u32(GuestPtr(data.0 + 62))?;
        let source = pointer(memory, process, text, u32::from(length))?;
        let mut bytes = memory.read(source, usize::from(length))?.to_vec();
        bytes.splice(
            usize::from(start)..usize::from(end),
            (!deletion).then_some(byte),
        );
        self.layout(
            memory,
            process,
            handle,
            bytes,
            true,
            Some((start + added, ticks)),
        )?;
        if process.mem_error != 0 {
            return Ok(());
        }
        let data = pointer(memory, process, handle, 128)?;
        self.redraw(memory, process, gpu, GuestPtr(data.0 + 8), handle, true)
    }
    /// Delete a selection without touching either TextEdit scrap.
    /// Layout publication is transactional; GPU failures after publication are terminal.
    pub fn delete(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &crate::graphics::Graphics,
        handle: u32,
        ticks: u32,
    ) -> Result<()> {
        self.call(memory, process, Op::GetText(handle), 0)?;
        let data = pointer(memory, process, handle, 128)?;
        if memory.u16(GuestPtr(data.0 + 36))? != 0 {
            validate_overlay_hooks(memory, data)?;
        }
        let length = memory.u16(GuestPtr(data.0 + 60))?;
        if length > 32767 {
            return Err(bad("invalid TextEdit text length"));
        }
        let a = memory.u16(GuestPtr(data.0 + 32))?.min(length);
        let b = memory.u16(GuestPtr(data.0 + 34))?.min(length);
        let start = a.min(b);
        let end = a.max(b);
        if start == end {
            return Ok(());
        }
        let text = memory.u32(GuestPtr(data.0 + 62))?;
        let source = pointer(memory, process, text, u32::from(length))?;
        let mut bytes = memory.read(source, usize::from(length))?.to_vec();
        bytes.drain(usize::from(start)..usize::from(end));
        self.layout(memory, process, handle, bytes, true, Some((start, ticks)))?;
        if process.mem_error != 0 {
            return Ok(());
        }
        let data = pointer(memory, process, handle, 128)?;
        self.redraw(memory, process, gpu, GuestPtr(data.0 + 8), handle, true)
    }
    pub fn text_box(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &crate::graphics::Graphics,
        request: TextBox,
        ticks: u32,
    ) -> Result<()> {
        if !(0..=32767).contains(&request.length) {
            return Err(bad("text box length must be between 0 and 32767"));
        }
        if !matches!(request.alignment, -2..=1) {
            return Err(bad("unsupported text box alignment"));
        }
        let rect = crate::quickdraw::read_rect(memory, request.rectangle)?;
        if rect.left >= rect.right || rect.top >= rect.bottom {
            return Ok(());
        }
        let text = if request.length == 0 {
            vec![]
        } else {
            if request.text.0 == 0 {
                return Err(bad("nonempty text box requires a text pointer"));
            }
            memory.read(request.text, request.length as usize)?.to_vec()
        };
        let handle = self.call(
            memory,
            process,
            Op::New {
                dest: request.rectangle,
                view: request.rectangle,
                styled: false,
            },
            ticks,
        )?;
        if handle == 0 {
            return Err(bad("text box record allocation failed"));
        }
        let result = (|| {
            let data = pointer(memory, process, handle, 128)?;
            memory.write(GuestPtr(data.0 + 58), &request.alignment.to_be_bytes())?;
            self.layout(memory, process, handle, text, true, None)?;
            if process.mem_error != 0 {
                return Err(bad("text box layout allocation failed"));
            }
            let data = pointer(memory, process, handle, 128)?;
            self.redraw(memory, process, gpu, GuestPtr(data.0 + 8), handle, true)
        })();
        let allocation_error = process.mem_error;
        let cleanup = self
            .call(memory, process, Op::Dispose(handle), ticks)
            .map(|_| ());
        if result.is_err() && allocation_error != 0 {
            process.mem_error = allocation_error;
        }
        result.and(cleanup)
    }
    pub fn update(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &crate::graphics::Graphics,
        rect: GuestPtr,
        handle: u32,
    ) -> Result<()> {
        self.redraw(memory, process, gpu, rect, handle, false)
    }
    fn redraw(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &crate::graphics::Graphics,
        rect: GuestPtr,
        handle: u32,
        erase: bool,
    ) -> Result<()> {
        use crate::{
            quickdraw::read_rect,
            region_geometry::{self, Boolean},
        };
        self.call(memory, process, Op::GetText(handle), 0)?;
        let update = memory.read(rect, 8)?.to_vec();
        let data = pointer(memory, process, handle, 128)?;
        let record = memory.read(data, 128)?.to_vec();
        if record[66..70] != [0; 4] {
            return Err(bad("custom TextEdit redraw dispatch is unsupported"));
        }
        let active = record[36..38] != [0, 0];
        if active {
            validate_overlay_hooks(memory, data)?;
        }
        let erase = erase || active;
        let length = usize::from(u16::from_be_bytes(record[60..62].try_into().unwrap()));
        let count = usize::from(u16::from_be_bytes(record[94..96].try_into().unwrap()));
        if length > 32767 || count > length {
            return Err(bad("invalid TextEdit line count or length"));
        }
        let text_handle = memory.u32(GuestPtr(data.0 + 62))?;
        let text = memory
            .read(
                pointer(memory, process, text_handle, length as u32)?,
                length,
            )?
            .to_vec();
        let line_data = pointer(memory, process, handle, (96 + (count + 2) * 2) as u32)?;
        let starts = memory
            .read(GuestPtr(line_data.0 + 96), (count + 1) * 2)?
            .chunks_exact(2)
            .map(|b| usize::from(u16::from_be_bytes([b[0], b[1]])))
            .collect::<Vec<_>>();
        if starts.first() != Some(&0)
            || starts.last() != Some(&length)
            || starts.windows(2).any(|pair| pair[0] > pair[1])
        {
            return Err(bad("invalid TextEdit line starts"));
        }
        let owned = self.records[&handle].clone();
        let styled = owned.len() == 7;
        let port = GuestPtr(u32::from_be_bytes(record[82..86].try_into().unwrap()));
        let saved = memory.read(port, 108)?.to_vec();
        if u16::from_be_bytes(saved[6..8].try_into().unwrap()) & 0xc000 != 0 {
            return Err(bad("TextEdit rendering in color ports is not implemented"));
        }
        memory.validate_write(port, 108)?;
        let ports = process
            .ports
            .ok_or_else(|| bad("TextEdit redraw requires InitGraf"))?;
        let old = memory.u32(ports.current_slot)?;
        memory.validate_write(ports.current_slot, 4)?;
        let (font, size, face, color, line_metrics) = if styled {
            let table = pointer(memory, process, owned[6], 28)?;
            if memory.u16(table)? != 1
                || memory.u16(GuestPtr(table.0 + 2))? != 1
                || memory.u16(GuestPtr(table.0 + 20))? != 0
                || memory.u16(GuestPtr(table.0 + 22))? != 0
            {
                return Err(bad("mixed-style redraw is not implemented"));
            }
            let style = memory
                .read(pointer(memory, process, owned[2], 18)?, 18)?
                .to_vec();
            let color = if style[12..18] == [0; 6] {
                33
            } else if style[12..18] == [255; 6] {
                30
            } else {
                return Err(bad("colored TextEdit redraw is unsupported"));
            };
            let metrics = memory
                .read(
                    pointer(memory, process, owned[3], ((count + 1) * 4) as u32)?,
                    (count + 1) * 4,
                )?
                .to_vec();
            (
                i16::from_be_bytes(style[6..8].try_into().unwrap()),
                i16::from_be_bytes(style[10..12].try_into().unwrap()),
                style[8],
                color,
                metrics,
            )
        } else {
            let height = record[24..26].to_vec();
            let ascent = record[26..28].to_vec();
            let mut metrics = Vec::new();
            for _ in 0..=count {
                metrics.extend(&height);
                metrics.extend(&ascent);
            }
            (
                i16::from_be_bytes(record[74..76].try_into().unwrap()),
                i16::from_be_bytes(record[80..82].try_into().unwrap()),
                record[76],
                memory.u32(GuestPtr(port.0 + 80))?,
                metrics,
            )
        };
        let font_bytes = crate::text::font_bytes(memory, process, font, size, face)?;
        let strike = crate::font::Strike::parse(&font_bytes)?;
        let mode = u16::from_be_bytes(record[78..80].try_into().unwrap());
        if mode > 7 {
            return Err(bad("unsupported TextEdit text mode"));
        }
        let mut view = 10u16.to_be_bytes().to_vec();
        view.extend(&record[8..16]);
        let mut update_region = 10u16.to_be_bytes().to_vec();
        update_region.extend(&update);
        let clip = crate::regions::read(memory, process, memory.u32(GuestPtr(port.0 + 28))?)?;
        let clip = region_geometry::combine(&clip, &view, Boolean::Intersection)?;
        let clip = region_geometry::combine(&clip, &update_region, Boolean::Intersection)?;
        let bitmap = crate::bitmap::Bitmap::read(memory, GuestPtr(port.0 + 2))?;
        let port_bounds = read_rect(memory, GuestPtr(port.0 + 16))?;
        let clip = region_geometry::combine(
            &clip,
            &region_geometry::encode(&[port_bounds])?,
            Boolean::Intersection,
        )?;
        let clip = region_geometry::combine(
            &clip,
            &region_geometry::encode(&[bitmap.bounds])?,
            Boolean::Intersection,
        )?;
        let screen = crate::bitmap::Bitmap::read(memory, ports.screen_bitmap)?;
        let clip = if bitmap.base == screen.base {
            region_geometry::combine(
                &clip,
                &crate::regions::read(memory, process, memory.u32(GuestPtr(port.0 + 24))?)?,
                Boolean::Intersection,
            )?
        } else {
            clip
        };
        let clip_rects = region_geometry::decode(&clip)?;
        if count.saturating_mul(clip_rects.len()) > 1_000_000 {
            return Err(bad("TextEdit line/clip work limit exceeded"));
        }
        let dest = read_rect(memory, data)?;
        let just = i16::from_be_bytes(record[58..60].try_into().unwrap());
        if !matches!(just, -2..=1) {
            return Err(bad("unsupported TextEdit justification"));
        }
        let mut top = dest.top;
        let mut lines = Vec::new();
        let mut overlays = Vec::new();
        let a = usize::from(u16::from_be_bytes(record[32..34].try_into().unwrap())).min(length);
        let b = usize::from(u16::from_be_bytes(record[34..36].try_into().unwrap())).min(length);
        let selection = a.min(b)..a.max(b);
        let caret_visible = record[56..58] != [0, 0];
        let mut work = 0usize;
        for line in 0..count {
            let height = i32::from(i16::from_be_bytes(
                line_metrics[line * 4..line * 4 + 2].try_into().unwrap(),
            ));
            let ascent = i32::from(i16::from_be_bytes(
                line_metrics[line * 4 + 2..line * 4 + 4].try_into().unwrap(),
            ));
            if height <= 0 || ascent < 0 {
                return Err(bad("invalid TextEdit line metrics"));
            }
            let start = starts[line];
            let raw_end = starts[line + 1];
            let mut content_end = raw_end;
            while content_end > start && matches!(text[content_end - 1], b'\r' | b'\n') {
                content_end -= 1;
            }
            let mut end = raw_end;
            while end > start && matches!(text[end - 1], b' ' | b'\r' | b'\n') {
                end -= 1;
            }
            let width = strike.width(&text[start..end])?;
            let left = dest.left
                + match just {
                    1 => (dest.right - dest.left - width) / 2,
                    -1 => dest.right - dest.left - width,
                    _ => 0,
                };
            let baseline = top + ascent;
            if active {
                use crate::graphics::Rect;
                if !selection.is_empty() {
                    let first = selection.start.max(start);
                    let last = selection.end.min(raw_end);
                    if first < last {
                        let x1 = left + strike.width(&text[start..first.min(content_end)])?;
                        let x2 = if last > content_end {
                            read_rect(memory, GuestPtr(data.0 + 8))?.right
                        } else {
                            left + strike.width(&text[start..last])?
                        };
                        if x1 < x2 {
                            overlays.push(Rect::new(x1, top, x2, top + height));
                        }
                    }
                } else if caret_visible
                    && selection.start >= start
                    && (selection.start < raw_end || (line + 1 == count && content_end == raw_end))
                {
                    let offset = selection.start.min(content_end);
                    let x = left + strike.width(&text[start..offset])? - i32::from(offset > start);
                    overlays.push(Rect::new(x, top, x + 1, top + height));
                }
            }
            if clip_rects.iter().any(|r| {
                baseline - i32::from(strike.metrics.ascent) < r.bottom
                    && baseline + i32::from(strike.metrics.descent) > r.top
            }) {
                for &c in &text[start..end] {
                    let glyph = strike.glyph(c);
                    let cells = (i32::from(glyph.advance)
                        .max(glyph.bearing + i32::from(glyph.width))
                        - 0.min(glyph.bearing)) as usize
                        * usize::from(glyph.height);
                    work = work.saturating_add(cells);
                    if work > 1_000_000 {
                        return Err(bad("TextEdit visible glyph work limit exceeded"));
                    }
                }
                lines.push((
                    i16::try_from(left)
                        .map_err(|_| bad("TextEdit horizontal coordinate overflow"))?,
                    i16::try_from(baseline).map_err(|_| bad("TextEdit baseline overflow"))?,
                    start,
                    end,
                ));
            }
            top += height;
        }
        if active
            && caret_visible
            && selection.is_empty()
            && selection.start == length
            && (count == 0 || text.last().is_some_and(|c| matches!(c, b'\r' | b'\n')))
        {
            let height = i32::from(i16::from_be_bytes(
                line_metrics[count * 4..count * 4 + 2].try_into().unwrap(),
            ));
            if height <= 0 {
                return Err(bad("invalid TextEdit caret height"));
            }
            let left = dest.left
                + match just {
                    1 => (dest.right - dest.left) / 2,
                    -1 => dest.right - dest.left,
                    _ => 0,
                };
            overlays.push(crate::graphics::Rect::new(
                left,
                top,
                left + 1,
                top + height,
            ));
        }
        overlays.retain(|rect| {
            clip_rects.iter().any(|clip| {
                rect.left < clip.right
                    && rect.right > clip.left
                    && rect.top < clip.bottom
                    && rect.bottom > clip.top
            })
        });
        if clip_rects.is_empty() {
            return Ok(());
        }
        let temporary = process.new_handle(memory, clip.len() as u32, false)?;
        if temporary == 0 {
            return Err(bad("TextEdit clip allocation failed"));
        }
        write_handle(memory, process, temporary, &clip)?;
        let result = (|| {
            ports.set(memory, port)?;
            memory.write(GuestPtr(port.0 + 28), &temporary.to_be_bytes())?;
            if erase {
                memory.write(GuestPtr(port.0 + 56), &8u16.to_be_bytes())?;
                memory.write(GuestPtr(port.0 + 58), &saved[32..40])?;
                ports.paint_rect(gpu, memory, read_rect(memory, GuestPtr(data.0 + 8))?)?;
            }
            memory.write(GuestPtr(port.0 + 68), &font.to_be_bytes())?;
            memory.write(GuestPtr(port.0 + 70), &[face])?;
            memory.write(GuestPtr(port.0 + 72), &mode.to_be_bytes())?;
            memory.write(GuestPtr(port.0 + 74), &size.to_be_bytes())?;
            memory.write(GuestPtr(port.0 + 80), &color.to_be_bytes())?;
            for (h, v, start, end) in lines {
                crate::text::move_to(memory, ports, h, v)?;
                crate::text::draw_strike(memory, ports, gpu, &text[start..end], &strike)?;
            }
            if !overlays.is_empty() {
                memory.write(GuestPtr(port.0 + 56), &10u16.to_be_bytes())?;
                memory.write(GuestPtr(port.0 + 58), &[255; 8])?;
                memory.write(GuestPtr(port.0 + 80), &33u32.to_be_bytes())?;
                memory.write(GuestPtr(port.0 + 84), &30u32.to_be_bytes())?;
                for rect in overlays {
                    ports.paint_rect(gpu, memory, rect)?;
                }
            }
            Ok(())
        })();
        memory.write(port, &saved)?;
        memory.write(ports.current_slot, &old.to_be_bytes())?;
        process.dispose_handle(memory, temporary)?;
        result
    }
    fn layout(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        handle: u32,
        bytes: Vec<u8>,
        replace: bool,
        caret: Option<(u16, u32)>,
    ) -> Result<u32> {
        self.call(memory, process, Op::GetText(handle), 0)?;
        let owned = self.records[&handle].clone();
        let data = pointer(memory, process, handle, 128)?;
        let mut record = memory.read(data, 128)?.to_vec();
        if memory.u32(GuestPtr(data.0 + 38))? != 0 || memory.u32(GuestPtr(data.0 + 66))? != 0 {
            return Err(bad("custom TextEdit layout callbacks are unsupported"));
        }
        let styled = owned.len() == 7;
        let mut style = if styled {
            let table = pointer(memory, process, owned[6], 28)?;
            if memory.u16(table)? != 1 || memory.u16(GuestPtr(table.0 + 2))? != 1 {
                return Err(bad("mixed-style layout is not implemented"));
            }
            if memory.u16(GuestPtr(table.0 + 20))? != 0
                || memory.u16(GuestPtr(table.0 + 22))? != 0
                || memory.u16(GuestPtr(table.0 + 26))? != 0xffff
            {
                return Err(bad("invalid single-style TextEdit run"));
            }

            memory
                .read(pointer(memory, process, owned[2], 18)?, 18)?
                .to_vec()
        } else {
            vec![]
        };
        let (font, size, face) = if styled {
            (
                i16::from_be_bytes(style[6..8].try_into().unwrap()),
                i16::from_be_bytes(style[10..12].try_into().unwrap()),
                style[8],
            )
        } else {
            (
                i16::from_be_bytes(record[74..76].try_into().unwrap()),
                i16::from_be_bytes(record[80..82].try_into().unwrap()),
                record[76],
            )
        };
        let source = crate::text::font_bytes(memory, process, font, size, face)?;
        let strike = crate::font::Strike::parse(&source)?;
        let width = (i32::from(i16::from_be_bytes(record[6..8].try_into().unwrap()))
            - i32::from(i16::from_be_bytes(record[2..4].try_into().unwrap())))
        .clamp(1, 32767);
        let starts = line_starts(&bytes, width, record[72..74] != [0, 0], |byte| {
            i32::from(strike.glyph(byte).advance)
        });
        let height = i16::try_from(
            i32::from(strike.metrics.ascent)
                + i32::from(strike.metrics.descent)
                + i32::from(strike.metrics.leading),
        )
        .map_err(|_| bad("TextEdit line-height overflow"))?;
        if height <= 0 {
            return Err(bad("invalid TextEdit line height"));
        }
        record.resize(128.max(96 + (starts.len() + 2) * 2), 0);
        record[94..96].copy_from_slice(&(starts.len() as u16).to_be_bytes());
        for (index, start) in starts
            .iter()
            .copied()
            .chain([bytes.len() as u16, 0])
            .enumerate()
        {
            record[96 + index * 2..98 + index * 2].copy_from_slice(&start.to_be_bytes());
        }
        record[60..62].copy_from_slice(&(bytes.len() as u16).to_be_bytes());
        if replace {
            let offset = caret.map_or(bytes.len() as u16, |(offset, _)| offset);
            record[32..34].copy_from_slice(&offset.to_be_bytes());
            record[34..36].copy_from_slice(&offset.to_be_bytes());
            if record[36..38] != [0, 0] {
                record[56..58].copy_from_slice(&1u16.to_be_bytes());
                if let Some((_, ticks)) = caret {
                    record[52..56].copy_from_slice(&ticks.to_be_bytes());
                }
            }
        }
        let mut changes = Vec::new();
        if replace {
            changes.push((owned[1], bytes.clone()));
        }
        if styled {
            style[2..4].copy_from_slice(&height.to_be_bytes());
            style[4..6].copy_from_slice(&strike.metrics.ascent.to_be_bytes());
            changes.push((owned[2], style));
            let mut lines = Vec::new();
            for _ in 0..=starts.len() {
                lines.extend(height.to_be_bytes());
                lines.extend(strike.metrics.ascent.to_be_bytes());
            }
            changes.push((owned[3], lines));
            let mut table = memory
                .read(pointer(memory, process, owned[6], 28)?, 28)?
                .to_vec();
            table[24..26].copy_from_slice(&((bytes.len() + 1) as u16).to_be_bytes());
            changes.push((owned[6], table));
        } else {
            record[24..26].copy_from_slice(&height.to_be_bytes());
            record[26..28].copy_from_slice(&strike.metrics.ascent.to_be_bytes());
        }
        changes.push((handle, record));
        process.replace_handles(memory, changes)?;
        Ok(0)
    }
}

fn line_starts(text: &[u8], width: i32, cr_only: bool, advance: impl Fn(u8) -> i32) -> Vec<u16> {
    let mut starts = Vec::new();
    let mut start = 0;
    while start < text.len() {
        starts.push(start as u16);
        let mut index = start;
        let mut used = 0;
        let mut break_after = None;
        let mut done = false;
        while index < text.len() {
            if matches!(text[index], b'\r' | b'\n') {
                start = index + 1;
                if text[index] == b'\r' && text.get(start) == Some(&b'\n') {
                    start += 1;
                }
                done = true;
                break;
            }
            let next = used + advance(text[index]);
            if !cr_only && index > start && next > width {
                start = if text[index] <= b' ' {
                    let mut end = index + 1;
                    while end < text.len()
                        && text[end] <= b' '
                        && !matches!(text[end], b'\r' | b'\n')
                    {
                        end += 1;
                    }
                    end
                } else {
                    break_after.unwrap_or(index)
                };
                done = true;
                break;
            }
            used = next;
            if text[index] <= b' ' && (index == start || text[index - 1] > b' ') {
                let mut end = index + 1;
                while end < text.len() && text[end] <= b' ' && !matches!(text[end], b'\r' | b'\n') {
                    end += 1;
                }
                break_after = Some(end);
            }
            index += 1;
        }
        if !done {
            break;
        }
    }
    starts
}
