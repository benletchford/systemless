//! Shared Dialog Manager startup, resource records and item ownership.
//! Guest DAStrings are Pascal StringHandles; no architecture-specific text cache is used.
use crate::{
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
mod resources;
use std::collections::BTreeMap;
const STRINGS: GuestPtr = GuestPtr(0xaa0);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Resource {
        id: i16,
        storage: GuestPtr,
        behind: u32,
    },
    Item {
        dialog: u32,
        number: i16,
        kind: GuestPtr,
        handle: GuestPtr,
        bounds: GuestPtr,
    },
    Dispose {
        dialog: u32,
        close: bool,
    },
    Text {
        handle: u32,
        pointer: GuestPtr,
        set: bool,
    },
    Draw(u32),
    SelectText {
        dialog: u32,
        number: i16,
        start: i16,
        end: i16,
    },
    DefaultItem {
        dialog: u32,
        number: i16,
    },
    Init(u32),
    IsEvent(GuestPtr),
    Slot(i16),
    Param([GuestPtr; 4]),
    Get([GuestPtr; 4]),
    Font(i16),
    Sound(u32),
}
#[derive(Default)]
pub struct Dialogs {
    strings: [u32; 4],
    records: BTreeMap<u32, resources::Record>,
}
fn bad(message: &str) -> Error {
    Error::Graphics(message.into())
}
fn pstring(memory: &Memory, pointer: GuestPtr) -> Result<Vec<u8>> {
    let length = usize::from(memory.read(pointer, 1)?[0]);
    Ok(memory.read(pointer, length + 1)?.to_vec())
}
pub fn slot(memory: &Memory, index: i16) -> Result<u32> {
    if !(0..4).contains(&index) {
        return Err(bad("dialog parameter slot index is out of range"));
    }
    memory.u32(GuestPtr(STRINGS.0 + index as u32 * 4))
}
impl Dialogs {
    fn check(&self, memory: &Memory, process: &mut Process) -> Result<[Vec<u8>; 4]> {
        let mut result = std::array::from_fn(|_| vec![0]);
        for (i, &handle) in self.strings.iter().enumerate() {
            if memory.u32(GuestPtr(STRINGS.0 + i as u32 * 4))? != handle {
                return Err(bad("foreign dialog parameter handles are unsupported"));
            }
            if handle != 0 {
                let size = process.handle_size(memory, handle)?;
                if process.mem_error != 0 || size == 0 {
                    return Err(bad("invalid dialog StringHandle"));
                }
                let pointer = GuestPtr(memory.u32(GuestPtr(handle))?);
                let length = usize::from(memory.read(pointer, 1)?[0]) + 1;
                if length > size as usize {
                    return Err(bad("truncated dialog parameter string"));
                }
                result[i] = memory.read(pointer, length)?.to_vec();
            }
        }
        Ok(result)
    }
    pub fn call(&mut self, memory: &mut Memory, process: &mut Process, op: Op) -> Result<()> {
        match op {
            Op::Resource { .. }
            | Op::Item { .. }
            | Op::Dispose { .. }
            | Op::DefaultItem { .. }
            | Op::SelectText { .. }
            | Op::Draw(_)
            | Op::Text { .. } => Err(bad("dialog operation requires its record/value route")),
            Op::IsEvent(_) => Err(bad("dialog event query requires a value route")),
            Op::Slot(_) => Err(bad("dialog parameter slot requires a value route")),
            Op::Init(resume) => {
                self.check(memory, process)?;
                for (pointer, len) in [
                    (GuestPtr(0xa8c), 4),
                    (GuestPtr(0xa9a), 6),
                    (STRINGS, 16),
                    (GuestPtr(0xafa), 2),
                ] {
                    memory.validate_write(pointer, len)?;
                }
                for &handle in &self.strings {
                    if handle != 0 {
                        memory.validate_write(GuestPtr(handle), 4)?;
                    }
                }
                // NIL DABeeper is the explicit legacy no-sound boundary. No fake sound procedure is installed.
                memory.write(GuestPtr(0xa8c), &resume.to_be_bytes())?;
                memory.write(GuestPtr(0xa9a), &[0; 6])?;
                memory.write(STRINGS, &[0; 16])?;
                memory.write(GuestPtr(0xafa), &[0; 2])?;
                for handle in self.strings {
                    if handle != 0 {
                        process.dispose_handle(memory, handle)?;
                    }
                }
                self.strings = [0; 4];
                process.mem_error = 0;
                Ok(())
            }
            Op::Font(font) => memory.write(GuestPtr(0xafa), &font.to_be_bytes()),
            Op::Sound(proc) => memory.write(GuestPtr(0xa9c), &proc.to_be_bytes()),
            Op::Get(outputs) => {
                let strings = self.check(memory, process)?;
                for (pointer, bytes) in outputs.iter().zip(&strings) {
                    if pointer.0 != 0 {
                        memory.validate_write(*pointer, bytes.len())?;
                    }
                }
                for (pointer, bytes) in outputs.into_iter().zip(strings) {
                    if pointer.0 != 0 {
                        memory.write(pointer, &bytes)?;
                    }
                }
                Ok(())
            }
            Op::Param(inputs) => {
                self.check(memory, process)?;
                memory.validate_write(STRINGS, 16)?;
                let mut values: [Option<Vec<u8>>; 4] = std::array::from_fn(|_| None);
                for (i, pointer) in inputs.into_iter().enumerate() {
                    if pointer.0 != 0 {
                        values[i] = Some(pstring(memory, pointer)?);
                    }
                }
                let mut handles = self.strings;
                let mut fresh = Vec::new();
                for (i, value) in values.iter().enumerate() {
                    if let Some(bytes) = value {
                        if handles[i] == 0 {
                            let handle = process.new_handle(memory, bytes.len() as u32, false)?;
                            if handle == 0 {
                                for handle in fresh {
                                    process.dispose_handle(memory, handle)?;
                                }
                                process.mem_error = -108;
                                return Ok(());
                            }
                            handles[i] = handle;
                            fresh.push(handle);
                        }
                    }
                }
                let changes = values
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, bytes)| bytes.map(|bytes| (handles[i], bytes)))
                    .collect();
                let result = process.replace_handles(memory, changes);
                if result.is_err() || process.mem_error != 0 {
                    let error = process.mem_error;
                    for handle in fresh {
                        process.dispose_handle(memory, handle)?;
                    }
                    process.mem_error = error;
                    return result;
                }
                let bytes = handles
                    .into_iter()
                    .flat_map(u32::to_be_bytes)
                    .collect::<Vec<_>>();
                memory.write(STRINGS, &bytes)?;
                self.strings = handles;
                Ok(())
            }
        }
    }
}
