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
pub enum Op {
    Init,
    New {
        dest: GuestPtr,
        view: GuestPtr,
        styled: bool,
    },
    GetText(u32),
    GetStyle(u32),
    Dispose(u32),
    SetText {
        source: GuestPtr,
        length: u32,
        handle: u32,
    },
    Calculate(u32),
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
impl Manager {
    pub fn call(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        op: Op,
        ticks: u32,
    ) -> Result<u32> {
        match op {
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
                self.layout(memory, process, handle, bytes, true)
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
                self.layout(memory, process, handle, bytes, false)
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
    fn layout(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        handle: u32,
        bytes: Vec<u8>,
        replace: bool,
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
            record[32..34].copy_from_slice(&(bytes.len() as u16).to_be_bytes());
            record[34..36].copy_from_slice(&(bytes.len() as u16).to_be_bytes());
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
