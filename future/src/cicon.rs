//! Owned color-icon records: IM More Macintosh Toolbox, Color Icon Record;
//! IM Imaging With QuickDraw, compiled cicn resource (figure 4-18).
mod plot;
use crate::{
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
pub use plot::plot;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Get(i16),
    Dispose(u32),
}
#[derive(Default)]
pub struct Manager {
    owned: BTreeMap<u32, Owned>,
}
#[derive(Clone, Copy)]
struct Owned {
    pixels: u32,
    table: u32,
    masks: u32,
    mask_len: u32,
    total_masks: u32,
}
struct Icon {
    record: [u8; 82],
    masks: Vec<u8>,
    mask_len: usize,
    table: Vec<u8>,
    pixels: Vec<u8>,
}
fn bad() -> Error {
    Error::Graphics("invalid or unsupported cicn resource or owned icon graph".into())
}
fn bytes(b: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    b.get(at..at.checked_add(len).ok_or_else(bad)?)
        .ok_or_else(bad)
}
impl Icon {
    fn decode(b: &[u8]) -> Result<Self> {
        let record: [u8; 82] = bytes(b, 0, 82)?.try_into().unwrap();
        let w = |at| u16::from_be_bytes(record[at..at + 2].try_into().unwrap());
        let l = |at| u32::from_be_bytes(record[at..at + 4].try_into().unwrap());
        let width = i32::from(w(12) as i16) - i32::from(w(8) as i16);
        let height = i32::from(w(10) as i16) - i32::from(w(6) as i16);
        let depth = usize::from(w(32));
        let row = usize::from(w(4) & 0x3fff);
        // Resource clients may have populated pointer placeholders in the live
        // cached bytes (EV does this for iconMask.baseAddr). Never follow these
        // addresses: all five pointer/handle fields are replaced below.
        if [18, 38, 46].into_iter().any(|at| l(at) != 0)
            || w(4) & 0xc000 != 0x8000
            || w(14) != 0
            || w(16) != 0
            || l(22) != 0x00480000
            || l(26) != 0x00480000
            || w(30) != 0
            || ![1, 2, 4, 8].contains(&depth)
            || w(34) != 1
            || usize::from(w(36)) != depth
            || width <= 0
            || height <= 0
            || width > 4096
            || height > 4096
            || row == 0
            || width as usize * depth > row * 8
        {
            return Err(bad());
        }
        let mono_len = |at: usize, optional: bool| -> Result<usize> {
            let stride = usize::from(w(at + 4));
            if optional && stride == 0 {
                return Ok(0);
            }
            if stride == 0
                || stride >= 0x4000
                || width as usize > stride * 8
                || i32::from(w(at + 12) as i16) - i32::from(w(at + 8) as i16) != width
                || i32::from(w(at + 10) as i16) - i32::from(w(at + 6) as i16) != height
            {
                return Err(bad());
            }
            Ok(stride * height as usize)
        };
        let mask_len = mono_len(50, false)?;
        let mono = mono_len(64, true)?;
        let table_at = 82 + mask_len + mono;
        let head = bytes(b, table_at, 8)?;
        let flags = u16::from_be_bytes(head[4..6].try_into().unwrap());
        let count = usize::from(u16::from_be_bytes(head[6..8].try_into().unwrap())) + 1;
        if count > 256 || flags & 0x4000 != 0 {
            return Err(bad());
        }
        let table = bytes(b, table_at, 8 + count * 8)?;
        let mut defined = [false; 256];
        for n in 0..count {
            let at = 8 + n * 8;
            let index = if flags & 0x8000 != 0 {
                n
            } else {
                usize::from(u16::from_be_bytes(table[at..at + 2].try_into().unwrap()))
            };
            if index >= 1usize << depth || defined[index] {
                return Err(bad());
            }
            defined[index] = true;
        }
        let pixels = bytes(b, table_at + table.len(), row * height as usize)?;
        // A complete resource has exactly the documented fields, not an inferred tail.
        if table_at + table.len() + pixels.len() != b.len() {
            return Err(bad());
        }
        for y in 0..height as usize {
            for x in 0..width as usize {
                let bit = x * depth;
                let index = (pixels[y * row + bit / 8] >> (8 - depth - bit % 8))
                    & ((1u16 << depth) - 1) as u8;
                if !defined[usize::from(index)] {
                    return Err(bad());
                }
            }
        }
        Ok(Self {
            record,
            masks: bytes(b, 82, mask_len + mono)?.to_vec(),
            mask_len,
            table: table.to_vec(),
            pixels: pixels.to_vec(),
        })
    }
}
impl Manager {
    pub(crate) fn forget(&mut self, root: u32) {
        self.owned.remove(&root);
    }
    // Live PixMap.baseAddr is a pointer; keep this owned image stable independently
    // of guest HLock/HUnlock, without changing the guest-visible handle flags.
    pub(crate) fn pins(&self, handle: u32) -> bool {
        self.owned.values().any(|o| o.pixels == handle)
    }
    pub fn call(&mut self, memory: &mut Memory, process: &mut Process, op: Op) -> Result<u32> {
        match op {
            Op::Get(id) => self.get(memory, process, id),
            Op::Dispose(root) => {
                self.dispose(memory, process, root)?;
                Ok(0)
            }
        }
    }
    fn get(&mut self, memory: &mut Memory, process: &mut Process, id: i16) -> Result<u32> {
        let source = process.get_resource(memory, *b"cicn", id)?;
        if source == 0 {
            return Ok(0);
        }
        let size = process.handle_size(memory, source)? as usize;
        let mut icon = Icon::decode(memory.read(GuestPtr(memory.u32(GuestPtr(source))?), size)?)?;
        let mut handles = Vec::new();
        let mut masks = 0;
        let result = (|| -> Result<bool> {
            for size in [82, icon.pixels.len() as u32, icon.table.len() as u32] {
                let h = process.new_handle(memory, size, true)?;
                if h == 0 {
                    return Ok(false);
                }
                handles.push(h);
            }
            masks = process.new_pointer(memory, icon.masks.len() as u32, false)?;
            if masks == 0 {
                return Ok(false);
            }
            let [root, pixels, table]: [u32; 3] = handles.clone().try_into().unwrap();
            let image = memory.u32(GuestPtr(pixels))?;
            icon.record[..4].copy_from_slice(&image.to_be_bytes());
            icon.record[42..46].copy_from_slice(&table.to_be_bytes());
            icon.record[50..54].copy_from_slice(&masks.to_be_bytes());
            let mono = if icon.masks.len() > icon.mask_len {
                masks + icon.mask_len as u32
            } else {
                0
            };
            icon.record[64..68].copy_from_slice(&mono.to_be_bytes());
            icon.record[78..82].copy_from_slice(&pixels.to_be_bytes());
            for (h, data) in [
                (root, icon.record.as_slice()),
                (pixels, icon.pixels.as_slice()),
                (table, icon.table.as_slice()),
            ] {
                memory.write(GuestPtr(memory.u32(GuestPtr(h))?), data)?;
            }
            memory.write(GuestPtr(masks), &icon.masks)?;
            self.owned.insert(
                root,
                Owned {
                    pixels,
                    table,
                    masks,
                    mask_len: icon.mask_len as u32,
                    total_masks: icon.masks.len() as u32,
                },
            );
            Ok(true)
        })();
        match result {
            Ok(true) => Ok(handles[0]),
            other => {
                let error = process.mem_error;
                if masks != 0 {
                    process.dispose_pointer(masks);
                }
                for h in handles.into_iter().rev() {
                    process.dispose_handle(memory, h)?;
                }
                process.mem_error = error;
                other.map(|_| 0)
            }
        }
    }
    fn dispose(&mut self, memory: &mut Memory, process: &mut Process, root: u32) -> Result<()> {
        if root == 0 {
            return Ok(());
        }
        let o = *self.owned.get(&root).ok_or_else(bad)?;
        for (h, min) in [(root, 82), (o.pixels, 1), (o.table, 8)] {
            if process.handle_size(memory, h)? < min || process.mem_error != 0 {
                return Err(bad());
            }
            memory.validate_write(GuestPtr(h), 4)?;
            if process.pixel_image_is_pinned(h) {
                return Err(bad());
            }
        }
        if process.pointer_size(o.masks) < o.total_masks || process.mem_error != 0 {
            return Err(bad());
        }
        let p = memory.u32(GuestPtr(root))?;
        let mono = if o.total_masks > o.mask_len {
            o.masks + o.mask_len
        } else {
            0
        };
        for (at, expected) in [
            (0, memory.u32(GuestPtr(o.pixels))?),
            (42, o.table),
            (50, o.masks),
            (64, mono),
            (78, o.pixels),
        ] {
            if memory.u32(GuestPtr(p + at))? != expected {
                return Err(bad());
            }
        }
        self.owned.remove(&root);
        for h in [o.table, o.pixels, root] {
            process.dispose_handle(memory, h)?;
        }
        process.dispose_pointer(o.masks);
        Ok(())
    }
}
