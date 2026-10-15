//! Shared initial display device. IM: Imaging With QuickDraw, chapter 5,
//! GDevice (5-14), device queries (5-26–5-29); Color QuickDraw PixMap/ColorTable.
use crate::{
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
// Verified against Quickdraw.h low-memory accessors in the pinned SDK.
pub const MAIN: GuestPtr = GuestPtr(0x08a4);
pub const FIRST: GuestPtr = GuestPtr(0x08a8);
pub const CURRENT: GuestPtr = GuestPtr(0x0cc8);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Current,
    Main,
    First,
    Next(u32),
    Attribute { device: u32, bit: i16 },
    Index { index: i32, destination: GuestPtr },
}
#[derive(Default)]
pub struct Manager {
    device: Option<u32>,
}
fn invalid(message: &str) -> Error {
    Error::Graphics(message.into())
}
fn record(memory: &Memory, process: &mut Process, handle: u32, size: u32) -> Result<GuestPtr> {
    let n = process.handle_size(memory, handle)?;
    if process.mem_error != 0 || n < size {
        return Err(invalid("graphics device record is not a live owned handle"));
    }
    Ok(GuestPtr(memory.u32(GuestPtr(handle))?))
}
impl Manager {
    fn initialize(&mut self, memory: &mut Memory, process: &mut Process) -> Result<u32> {
        if let Some(device) = self.device {
            record(memory, process, device, 62)?;
            return Ok(device);
        }
        for pointer in [MAIN, FIRST, CURRENT] {
            memory.validate_write(pointer, 4)?;
        }
        let screen = crate::quickdraw::screen_bitmap();
        memory.read(screen.base, screen.byte_len()?)?;
        let mut owned = Vec::new();
        let result = (|| {
            for size in [24, 50, 62] {
                let handle = process.new_handle(memory, size, true)?;
                if handle == 0 {
                    return Err(invalid("not enough memory for graphics device"));
                }
                owned.push(handle);
            }
            let [table, pixels, device] = owned[..] else {
                unreachable!()
            };
            let table_data = record(memory, process, table, 24)?;
            let mut colors = [0u8; 24];
            colors[..4].copy_from_slice(&1u32.to_be_bytes());
            colors[4..6].copy_from_slice(&0x8000u16.to_be_bytes());
            colors[6..8].copy_from_slice(&1u16.to_be_bytes());
            colors[10..16].fill(255); // pixel 0 is white
            colors[16..18].copy_from_slice(&1u16.to_be_bytes()); // pixel 1 black
            memory.write(table_data, &colors)?;
            let pixel_data = record(memory, process, pixels, 50)?;
            let mut map = [0u8; 50];
            map[..4].copy_from_slice(&screen.base.0.to_be_bytes());
            map[4..6].copy_from_slice(&(0x8000 | screen.row_bytes as u16).to_be_bytes());
            let mut bounds = [0; 8];
            for (index, coordinate) in [
                screen.bounds.top,
                screen.bounds.left,
                screen.bounds.bottom,
                screen.bounds.right,
            ]
            .into_iter()
            .enumerate()
            {
                let value = i16::try_from(coordinate)
                    .map_err(|_| invalid("device bounds exceed Macintosh coordinates"))?;
                bounds[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
            }
            map[6..14].copy_from_slice(&bounds);
            map[22..26].copy_from_slice(&(72u32 << 16).to_be_bytes());
            map[26..30].copy_from_slice(&(72u32 << 16).to_be_bytes());
            for at in [32, 34, 36] {
                map[at..at + 2].copy_from_slice(&1u16.to_be_bytes());
            }
            map[42..46].copy_from_slice(&table.to_be_bytes());
            memory.write(pixel_data, &map)?;
            let data = record(memory, process, device, 62)?;
            let mut gd = [0u8; 62];
            gd[20..22].copy_from_slice(&0xe800u16.to_be_bytes());
            gd[22..26].copy_from_slice(&pixels.to_be_bytes());
            gd[34..42].copy_from_slice(&bounds);
            memory.write(data, &gd)?;
            Ok(device)
        })();
        match result {
            Ok(device) => {
                for pointer in [MAIN, FIRST, CURRENT] {
                    memory.write(pointer, &device.to_be_bytes())?;
                }
                self.device = Some(device);
                Ok(device)
            }
            Err(error) => {
                for handle in owned.into_iter().rev() {
                    process.dispose_handle(memory, handle)?;
                }
                Err(error)
            }
        }
    }
    pub fn call(&mut self, memory: &mut Memory, process: &mut Process, op: Op) -> Result<u32> {
        let saved = process.mem_error;
        let result = (|| {
            // Output faults must not create hidden heap allocations.
            if let Op::Index { destination, .. } = op {
                memory.validate_write(destination, 6)?;
            }
            let device = self.initialize(memory, process)?;
            let data = record(memory, process, device, 62)?;
            match op {
                Op::Current | Op::Main | Op::First => {
                    let pointer = match op {
                        Op::Current => CURRENT,
                        Op::Main => MAIN,
                        _ => FIRST,
                    };
                    let selected = memory.u32(pointer)?;
                    if selected != device {
                        return Err(invalid("unregistered graphics device selection"));
                    }
                    Ok(selected)
                }
                Op::Next(requested) => {
                    if requested != device {
                        return Err(invalid("unknown graphics device"));
                    }
                    let next = memory.u32(GuestPtr(data.0 + 30))?;
                    if next != 0 {
                        return Err(invalid("unregistered graphics device link"));
                    }
                    Ok(0)
                }
                Op::Attribute {
                    device: requested,
                    bit,
                } => {
                    if requested != device || !(0..16).contains(&bit) {
                        return Err(invalid("invalid graphics device attribute query"));
                    }
                    Ok(u32::from(
                        memory.u16(GuestPtr(data.0 + 20))? & (1 << bit) != 0,
                    ))
                }
                Op::Index { index, destination } => {
                    if memory.u32(CURRENT)? != device {
                        return Err(invalid("unregistered current graphics device"));
                    }
                    let pixels = memory.u32(GuestPtr(data.0 + 22))?;
                    let pm = record(memory, process, pixels, 50)?;
                    let table = memory.u32(GuestPtr(pm.0 + 42))?;
                    let ct = record(memory, process, table, 8)?;
                    let last = memory.u16(GuestPtr(ct.0 + 6))? as i16;
                    if !(0..=4095).contains(&last) || index < 0 || index > i32::from(last) {
                        return Err(invalid("color index is outside the current device table"));
                    }
                    let length = 8 + (last as u32 + 1) * 8;
                    record(memory, process, table, length)?;
                    if memory.u16(GuestPtr(ct.0 + 4))? & 0x8000 == 0 {
                        return Err(invalid("non-device color tables are unsupported"));
                    }
                    let rgb = memory
                        .read(GuestPtr(ct.0 + 10 + index as u32 * 8), 6)?
                        .to_vec();
                    memory.write(destination, &rgb)?;
                    Ok(0)
                }
            }
        })();
        process.mem_error = saved;
        result
    }
}
