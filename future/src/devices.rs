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
    New {
        reference: i16,
        mode: i32,
    },
    NewPixMap,
    DisposePixMap(u32),
    Dispose(u32),
    DisposeTable(u32),
    GetTable(i16),
    Maximum(GuestPtr),
    Client(i16),
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
    detached: std::collections::BTreeMap<u32, [u32; 3]>,
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
    fn new_pixmap(&mut self, memory: &mut Memory, process: &mut Process) -> Result<u32> {
        // Bootstrap device ownership before beginning this independent allocation.
        let device = self.selected(memory, process)?;
        let gd = record(memory, process, device, 62)?;
        let source = record(memory, process, memory.u32(GuestPtr(gd.0 + 22))?, 50)?;
        let mut bytes = memory.read(source, 50)?.to_vec();
        let mut allocated = Vec::new();
        let result = (|| {
            for size in [8, 50] {
                let handle = process.new_handle(memory, size, true)?;
                if handle == 0 {
                    return Ok(0);
                }
                allocated.push(handle);
            }
            let [table, map] = allocated[..] else {
                unreachable!()
            };
            // Deterministic empty storage, not a copy of the device palette.
            let ct = record(memory, process, table, 8)?;
            memory.write(GuestPtr(ct.0 + 6), &(-1i16).to_be_bytes())?;
            bytes[22..26].copy_from_slice(&(72u32 << 16).to_be_bytes());
            bytes[26..30].copy_from_slice(&(72u32 << 16).to_be_bytes());
            bytes[42..46].copy_from_slice(&table.to_be_bytes());
            let destination = record(memory, process, map, 50)?;
            memory.write(destination, &bytes)?;
            Ok(map)
        })();
        if !matches!(result, Ok(handle) if handle != 0) {
            let error = process.mem_error;
            for handle in allocated.into_iter().rev() {
                process.dispose_handle(memory, handle)?;
            }
            process.mem_error = error;
        }
        result
    }
    fn dispose_pixmap(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        map: u32,
    ) -> Result<u32> {
        // Device-owned descriptors cannot be detached by disposing their storage.
        for device in self.device.into_iter().chain(self.detached.keys().copied()) {
            let gd = record(memory, process, device, 62)?;
            if [
                device,
                memory.u32(GuestPtr(gd.0 + 22))?,
                memory.u32(GuestPtr(gd.0 + 6))?,
            ]
            .contains(&map)
            {
                return Err(invalid("cannot dispose device-owned PixMap storage"));
            }
        }
        let pm = record(memory, process, map, 50)?;
        let table = memory.u32(GuestPtr(pm.0 + 42))?;
        if table == map {
            return Err(invalid("PixMap aliases its own color table"));
        }
        // Validate the complete disposal before freeing either handle. The image
        // is referenced by baseAddr, but is never owned by this operation.
        if table != 0 {
            self.validate_table(memory, process, table)?;
        }
        for handle in [map, table].into_iter().filter(|h| *h != 0) {
            memory.validate_write(GuestPtr(handle), 4)?;
            if process.pixel_image_is_pinned(handle) {
                return Err(invalid(
                    "disposing pixel-locked PixMap storage is unsupported",
                ));
            }
        }
        if table != 0 {
            self.dispose_table(memory, process, table)?;
        }
        process.dispose_handle(memory, map)?;
        Ok(0)
    }
    fn new_detached(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        reference: i16,
        mode: i32,
    ) -> Result<u32> {
        if mode != -1 {
            return Err(invalid("driver-backed NewGDevice modes are unsupported"));
        }
        let _ = reference; // No driver initialization in the documented -1 case.
        let mut handles = Vec::new();
        let result = (|| {
            for size in [8, 50, 6, 62] {
                let h = process.new_handle(memory, size, true)?;
                if h == 0 {
                    return Ok(0);
                }
                handles.push(h);
            }
            let [table, pixels, inverse, device] = handles[..] else {
                unreachable!()
            };
            let ct = record(memory, process, table, 8)?;
            memory.write(GuestPtr(ct.0 + 6), &(-1i16).to_be_bytes())?;
            let pm = record(memory, process, pixels, 50)?;
            memory.write(GuestPtr(pm.0 + 42), &table.to_be_bytes())?;
            let gd = record(memory, process, device, 62)?;
            memory.write(GuestPtr(gd.0 + 6), &inverse.to_be_bytes())?;
            memory.write(GuestPtr(gd.0 + 22), &pixels.to_be_bytes())?;
            self.detached.insert(device, [table, pixels, inverse]);
            Ok(device)
        })();
        if !matches!(result, Ok(h) if h != 0) {
            let error = process.mem_error;
            for h in handles.into_iter().rev() {
                process.dispose_handle(memory, h)?;
            }
            process.mem_error = error;
        }
        result
    }
    fn get_table(&mut self, memory: &mut Memory, process: &mut Process, id: i16) -> Result<u32> {
        // Preflight error publication before resource loading or heap allocation.
        memory.validate_write(GuestPtr(0x220), 2)?;
        memory.validate_write(GuestPtr(0xa60), 2)?;
        let source = process.get_resource(memory, *b"clut", id)?;
        if source == 0 {
            if process.res_error == -192 && matches!(id, 33 | 66 | 68 | 72 | 127) {
                return Err(invalid("standard system color tables are not implemented"));
            }
            return Ok(0);
        }
        let size = process.handle_size(memory, source)?;
        if process.mem_error != 0 || size < 8 {
            return Err(invalid("truncated color table resource header"));
        }
        let data = GuestPtr(memory.u32(GuestPtr(source))?);
        let last = memory.u16(GuestPtr(data.0 + 6))? as i16;
        if !(-1..=255).contains(&last) || size < 8 + (i32::from(last) + 1) as u32 * 8 {
            return Err(invalid("invalid color table resource extent"));
        }
        // Resource-backed tables retain their seed, flags and ColorSpec values.
        // copy_handle snapshots the live payload and clears resource identity.
        process.copy_handle(memory, source)
    }
    fn main_table(&self, memory: &Memory, process: &mut Process) -> Result<u32> {
        let Some(device) = self.device else {
            return Ok(0);
        };
        let gd = record(memory, process, device, 62)?;
        let pm = record(memory, process, memory.u32(GuestPtr(gd.0 + 22))?, 50)?;
        memory.u32(GuestPtr(pm.0 + 42))
    }
    fn validate_table(&self, memory: &Memory, process: &mut Process, table: u32) -> Result<()> {
        if let Some(device) = self.device {
            let gd = record(memory, process, device, 62)?;
            if [
                device,
                memory.u32(GuestPtr(gd.0 + 22))?,
                memory.u32(GuestPtr(gd.0 + 6))?,
            ]
            .contains(&table)
            {
                return Err(invalid(
                    "cannot dispose main display storage as a color table",
                ));
            }
        }
        if table == 0
            || table == self.main_table(memory, process)?
            || self
                .detached
                .iter()
                .any(|(device, parts)| *device == table || parts[1..].contains(&table))
        {
            return Err(invalid("color table is nil or protected graphics storage"));
        }
        let ct = record(memory, process, table, 8)?;
        let last = memory.u16(GuestPtr(ct.0 + 6))? as i16;
        if !(-1..=255).contains(&last) {
            return Err(invalid("invalid color table extent"));
        }
        record(memory, process, table, 8 + (i32::from(last) + 1) as u32 * 8)?;
        Ok(())
    }
    fn dispose_table(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        table: u32,
    ) -> Result<u32> {
        self.validate_table(memory, process, table)?;
        memory.validate_write(GuestPtr(table), 4)?;
        process.dispose_handle(memory, table)?;
        // Do not rewrite the caller's PixMap. It must install its replacement.
        // Forget ownership before this freed master can be reused.
        for parts in self.detached.values_mut() {
            if parts[0] == table {
                parts[0] = 0;
            }
        }
        Ok(0)
    }
    fn dispose_detached(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        device: u32,
    ) -> Result<u32> {
        let [table, pixels, inverse] = *self
            .detached
            .get(&device)
            .ok_or_else(|| invalid("DisposeGDevice requires an owned detached device"))?;
        let gd = record(memory, process, device, 62)?;
        let pm = record(memory, process, pixels, 50)?;
        let live_table = memory.u32(GuestPtr(pm.0 + 42))?;
        if table == 0 {
            if live_table != 0 {
                self.validate_table(memory, process, live_table)?;
                for (&other, parts) in &self.detached {
                    if other == device {
                        continue;
                    }
                    let other_pm = record(memory, process, parts[1], 50)?;
                    if parts[0] == live_table
                        || memory.u32(GuestPtr(other_pm.0 + 42))? == live_table
                    {
                        return Err(invalid("replacement table is shared with another device"));
                    }
                }
            }
        } else if live_table != table {
            return Err(invalid(
                "dispose the original device table before replacing it",
            ));
        }
        let table = live_table;
        if table != 0 {
            record(memory, process, table, 8)?;
        }
        record(memory, process, inverse, 6)?;
        if memory.u32(GuestPtr(gd.0 + 6))? != inverse || memory.u32(GuestPtr(gd.0 + 22))? != pixels
        {
            return Err(invalid("detached device supporting handles were replaced"));
        }
        if memory.u32(CURRENT)? == device {
            return Err(invalid(
                "select another device before disposing the current device",
            ));
        }
        for h in [device, inverse, pixels, table]
            .into_iter()
            .filter(|h| *h != 0)
        {
            memory.validate_write(GuestPtr(h), 4)?;
        }
        for h in [device, inverse, pixels, table]
            .into_iter()
            .filter(|h| *h != 0)
        {
            process.dispose_handle(memory, h)?;
        }
        self.detached.remove(&device);
        Ok(0)
    }
    pub fn call(&mut self, memory: &mut Memory, process: &mut Process, op: Op) -> Result<u32> {
        if op == Op::NewPixMap {
            return self.new_pixmap(memory, process);
        }
        if let Op::GetTable(id) = op {
            return self.get_table(memory, process, id);
        }
        if let Op::New { reference, mode } = op {
            return self.new_detached(memory, process, reference, mode);
        }
        let saved = process.mem_error;
        let result = (|| {
            if let Op::DisposePixMap(map) = op {
                return self.dispose_pixmap(memory, process, map);
            }
            if let Op::DisposeTable(table) = op {
                return self.dispose_table(memory, process, table);
            }
            if let Op::Dispose(device) = op {
                return self.dispose_detached(memory, process, device);
            }
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
            let device = if matches!(op, Op::Current | Op::Index { .. } | Op::Client(_)) {
                self.selected(memory, process)?
            } else {
                self.initialize(memory, process)?
            };
            let data = record(memory, process, device, 62)?;
            match op {
                Op::New { .. }
                | Op::NewPixMap
                | Op::DisposePixMap(_)
                | Op::GetTable(_)
                | Op::DisposeTable(_)
                | Op::Dispose(_)
                | Op::Maximum(_)
                | Op::Depth { .. }
                | Op::Select(_)
                | Op::Next(_) => unreachable!(),
                Op::Client(id) => {
                    memory.write(GuestPtr(data.0 + 2), &id.to_be_bytes())?;
                    Ok(0)
                }
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
