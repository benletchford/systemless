//! Region storage and copying; geometry interpretation remains in QuickDraw.
use crate::{
    memory::{GuestPtr, Memory},
    process::Process,
    quickdraw::read_rect,
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    New,
    Contains,
    Empty,
    Dispose,
    Copy,
    Rect,
    GetClip,
    SetClip,
}
fn read(memory: &Memory, process: &mut Process, handle: u32) -> Result<Vec<u8>> {
    let allocated = process.handle_size(memory, handle)?;
    if allocated < 10 {
        return Err(Error::Graphics("region requires an owned handle".into()));
    }
    let data = GuestPtr(memory.u32(GuestPtr(handle))?);
    let size = u32::from(memory.u16(data)?);
    if !(10..=32766).contains(&size) || !size.is_multiple_of(2) || size > allocated {
        return Err(Error::Graphics("invalid region storage size".into()));
    }
    Ok(memory.read(data, size as usize)?.to_vec())
}
fn write(memory: &mut Memory, process: &mut Process, handle: u32, bytes: &[u8]) -> Result<()> {
    process.set_handle_size(memory, handle, bytes.len() as u32)?;
    if process.mem_error == 0 {
        let data = GuestPtr(memory.u32(GuestPtr(handle))?);
        memory.write(data, bytes)?;
    }
    Ok(())
}
pub fn call(memory: &mut Memory, process: &mut Process, op: Op, a: u32, b: u32) -> Result<u32> {
    match op {
        Op::Contains | Op::Empty => {
            let bytes = read(memory, process, if op == Op::Contains { b } else { a })?;
            let rectangles = crate::region_geometry::decode(&bytes)?;
            let y = (a >> 16) as i16 as i32;
            let x = a as i16 as i32;
            return Ok(u32::from(if op == Op::Empty {
                rectangles.is_empty()
            } else {
                rectangles
                    .iter()
                    .any(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
            }));
        }

        Op::New => {
            let handle = process.new_handle(memory, 10, true)?;
            if handle != 0 {
                let data = GuestPtr(memory.u32(GuestPtr(handle))?);
                memory.write(data, &10u16.to_be_bytes())?;
            }
            return Ok(handle);
        }
        Op::Dispose => process.dispose_handle(memory, a)?,
        Op::Rect => {
            let r = read_rect(memory, GuestPtr(b))?;
            let mut bytes = [0; 10];
            bytes[..2].copy_from_slice(&10u16.to_be_bytes());
            if r.left < r.right && r.top < r.bottom {
                bytes[2..].copy_from_slice(memory.read(GuestPtr(b), 8)?);
            }
            write(memory, process, a, &bytes)?;
        }
        Op::Copy | Op::GetClip | Op::SetClip => {
            let (source, destination) = if op == Op::Copy {
                (a, b)
            } else {
                let ports = process
                    .ports
                    .ok_or_else(|| Error::Graphics("QuickDraw has not been initialized".into()))?;
                let port = ports.current(memory)?;
                let clip = memory.u32(GuestPtr(port.0 + 28))?;
                if op == Op::GetClip {
                    (clip, a)
                } else {
                    (a, clip)
                }
            };
            // Snapshot before resizing to support self-copy and relocation.
            let bytes = read(memory, process, source)?;
            write(memory, process, destination, &bytes)?;
        }
    }
    Ok(0)
}

pub fn combine(
    memory: &mut Memory,
    process: &mut Process,
    op: crate::region_geometry::Boolean,
    a: u32,
    b: u32,
    destination: u32,
) -> Result<()> {
    let a = read(memory, process, a)?;
    let b = read(memory, process, b)?;
    let bytes = crate::region_geometry::combine(&a, &b, op)?;
    write(memory, process, destination, &bytes)
}
