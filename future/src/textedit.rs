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
}
