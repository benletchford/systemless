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
const HOST_MODE: i16 = 128;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Depth {
        device: u32,
        depth: i16,
        which_flags: u16,
        flags: u16,
        set: bool,
    },
    Maximum(GuestPtr),
    Current,
    Select(u32),
    Main,
    First,
    Next(u32),
    Attribute {
        device: u32,
        bit: i16,
    },
    SetAttribute {
        device: u32,
        bit: i16,
        value: bool,
    },
    Index {
        index: i32,
        destination: GuestPtr,
    },
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
        let selected = memory.u32(CURRENT)?;
        if selected != 0 {
            record(memory, process, selected, 62)?;
        } else {
            memory.validate_write(CURRENT, 4)?;
        }
        for pointer in [MAIN, FIRST] {
            memory.validate_write(pointer, 4)?;
        }
        let screen = process.display.bitmap();
        let palette = process.display.colors();
        let depth = process.display.depth();
        memory.read(screen.base, screen.byte_len()?)?;
        let mut owned = Vec::new();
        let result = (|| {
            for size in [8 + palette.len() as u32 * 8, 50, 62] {
                let handle = process.new_handle(memory, size, true)?;
                if handle == 0 {
                    return Err(invalid("not enough memory for graphics device"));
                }
                owned.push(handle);
            }
            let [table, pixels, device] = owned[..] else {
                unreachable!()
            };
            let table_size = 8 + palette.len() * 8;
            let table_data = record(memory, process, table, table_size as u32)?;
            let mut colors = vec![0u8; table_size];
            colors[..4].copy_from_slice(&1u32.to_be_bytes());
            colors[4..6].copy_from_slice(&0x8000u16.to_be_bytes());
            colors[6..8].copy_from_slice(&((palette.len() - 1) as u16).to_be_bytes());
            for (i, rgb) in palette.iter().enumerate() {
                colors[8 + i * 8..10 + i * 8].copy_from_slice(&(i as u16).to_be_bytes());
                for (k, v) in rgb.iter().enumerate() {
                    colors[10 + i * 8 + k * 2..12 + i * 8 + k * 2]
                        .copy_from_slice(&v.to_be_bytes());
                }
            }
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
                map[at..at + 2]
                    .copy_from_slice(&(if at == 34 { 1u16 } else { depth }).to_be_bytes());
            }
            map[42..46].copy_from_slice(&table.to_be_bytes());
            memory.write(pixel_data, &map)?;
            let data = record(memory, process, device, 62)?;
            let mut gd = [0u8; 62];
            gd[10..12].copy_from_slice(&4u16.to_be_bytes());
            gd[20..22].copy_from_slice(&(0xe800u16 | u16::from(depth > 1)).to_be_bytes());
            gd[22..26].copy_from_slice(&pixels.to_be_bytes());
            gd[34..42].copy_from_slice(&bounds);
            // One real host-selected mode. It has a stable driver-style identity;
            // unsupported depths are not advertised as switchable modes.
            gd[42..46].copy_from_slice(&(HOST_MODE as u32).to_be_bytes());
            memory.write(data, &gd)?;
            Ok(device)
        })();
        match result {
            Ok(device) => {
                for pointer in [MAIN, FIRST] {
                    memory.write(pointer, &device.to_be_bytes())?;
                }
                if selected == 0 {
                    memory.write(CURRENT, &device.to_be_bytes())?;
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
    fn selected(&mut self, memory: &mut Memory, process: &mut Process) -> Result<u32> {
        let selected = memory.u32(CURRENT)?;
        if selected != 0 {
            record(memory, process, selected, 62)?;
            return Ok(selected);
        }
        if self.device.is_some() {
            return Err(invalid("nil current graphics device"));
        }
        self.initialize(memory, process)
    }
    pub fn call(&mut self, memory: &mut Memory, process: &mut Process, op: Op) -> Result<u32> {
        let saved = process.mem_error;
        let result = (|| {
            if let Op::Depth {
                device,
                depth,
                which_flags,
                flags,
                set,
            } = op
            {
                // Validate the requested record before any lazy allocation. Only
                // the main display is a video device; copied/offscreen records
                // cannot acquire its host capabilities by aliasing its PixMap.
                let data = record(memory, process, device, 62)?;
                let failure = if set { (-157i32) as u32 } else { 0 };
                if self.device != Some(device) {
                    return Ok(failure);
                }
                let actual_depth = process.display.depth();
                let mode = HOST_MODE;
                let pixels = memory.u32(GuestPtr(data.0 + 22))?;
                let pm = record(memory, process, pixels, 50)?;
                let screen = process.display.bitmap();
                let storage = crate::pixmap::destination(memory, pm)?;
                if storage.bounds != screen.bounds
                    || storage.depth != actual_depth
                    || memory.u32(GuestPtr(data.0 + 42))? != mode as u32
                    || memory.u16(GuestPtr(pm.0 + 32))? != actual_depth
                    || memory.u32(pm)? != screen.base.0
                    || memory.u16(GuestPtr(pm.0 + 4))? != (0x8000 | screen.row_bytes as u16)
                {
                    return Err(invalid(
                        "display mode record disagrees with actual screen storage",
                    ));
                }
                // gdDevType is bit zero. A zero mask preserves the configured
                // color format. Other flag controls have no host implementation.
                let color = u16::from(actual_depth > 1);
                let supported = which_flags & !1 == 0
                    && (flags & which_flags) == (color & which_flags)
                    && (depth == actual_depth as i16 || (set && depth == mode));
                return Ok(if supported {
                    if set {
                        0
                    } else {
                        mode as u32
                    }
                } else {
                    failure
                });
            }
            if let Op::Maximum(pointer) = op {
                // Read before lazy initialization: bad input must not allocate.
                let bytes = memory.read(pointer, 8)?;
                let mut rect = [0i16; 4];
                for (i, value) in rect.iter_mut().enumerate() {
                    *value = i16::from_be_bytes(bytes[i * 2..i * 2 + 2].try_into().unwrap());
                }
                let device = self.initialize(memory, process)?;
                let data = record(memory, process, device, 62)?;
                if memory.u32(FIRST)? != device || memory.u32(GuestPtr(data.0 + 30))? != 0 {
                    return Err(invalid("unregistered display list"));
                }
                let bounds = process.display.bitmap().bounds;
                // Only registered host screens participate, never SetGDevice's
                // application-owned offscreen selection. One host screen exists.
                let intersects = rect[0] < rect[2]
                    && rect[1] < rect[3]
                    && i32::from(rect[0]) < bounds.bottom
                    && i32::from(rect[2]) > bounds.top
                    && i32::from(rect[1]) < bounds.right
                    && i32::from(rect[3]) > bounds.left;
                return Ok(if intersects { device } else { 0 });
            }
            // Applications also own offscreen GDevice records. Attribute access
            // changes only that record; it must not initialize/select a screen.
            if let Op::Attribute { device, bit } | Op::SetAttribute { device, bit, .. } = op {
                if !(0..16).contains(&bit) {
                    return Err(invalid("invalid graphics device attribute bit"));
                }
                let data = record(memory, process, device, 62)?;
                let field = GuestPtr(data.0 + 20);
                let flags = memory.u16(field)?;
                let mask = 1u16 << bit;
                if let Op::SetAttribute { value, .. } = op {
                    let updated = if value { flags | mask } else { flags & !mask };
                    memory.write(field, &updated.to_be_bytes())?;
                    return Ok(0);
                }
                return Ok(u32::from(flags & mask != 0));
            }
            // Output faults must not create hidden heap allocations.
            if let Op::Index { destination, .. } = op {
                memory.validate_write(destination, 6)?;
            }
            if let Op::Select(requested) = op {
                memory.validate_write(CURRENT, 4)?;
                record(memory, process, requested, 62)?;
                memory.write(CURRENT, &requested.to_be_bytes())?;
                return Ok(0);
            }
            if let Op::Next(requested) = op {
                let data = record(memory, process, requested, 62)?;
                let next = memory.u32(GuestPtr(data.0 + 30))?;
                if next == requested {
                    return Err(invalid("cyclic graphics device link"));
                }
                if next != 0 {
                    record(memory, process, next, 62)?;
                }
                return Ok(next);
            }
            let device = if matches!(op, Op::Current | Op::Index { .. }) {
                self.selected(memory, process)?
            } else {
                self.initialize(memory, process)?
            };
            let data = record(memory, process, device, 62)?;
            match op {
                Op::Maximum(_) | Op::Depth { .. } | Op::Select(_) | Op::Next(_) => unreachable!(),
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
                Op::Attribute { .. } | Op::SetAttribute { .. } => unreachable!(),
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
