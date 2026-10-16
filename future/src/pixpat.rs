//! Owned GetPixPat graphs. IM: Color QuickDraw, PixPat and compiled 'ppat' format.
use crate::{
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Get(i16),
    Dispose(u32),
}
#[derive(Default)]
pub struct Manager {
    owned: BTreeMap<u32, [u32; 5]>,
}
struct Pattern {
    record: [u8; 28],
    map: [u8; 50],
    pixels: Vec<u8>,
    table: Vec<u8>,
}
fn bad() -> Error {
    Error::Graphics("invalid or unsupported ppat resource".into())
}
fn bytes(input: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    input
        .get(at..at.checked_add(n).ok_or_else(bad)?)
        .ok_or_else(bad)
}
impl Pattern {
    fn decode(input: &[u8]) -> Result<Self> {
        let record: [u8; 28] = bytes(input, 0, 28)?.try_into().unwrap();
        let short = |at| u16::from_be_bytes(record[at..at + 2].try_into().unwrap());
        let long = |at| u32::from_be_bytes(record[at..at + 4].try_into().unwrap()) as usize;
        if short(0) != 1 || long(10) != 0 || short(14) != 0xffff || long(16) != 0 {
            return Err(bad());
        }
        let map_at = long(2);
        let pixel_at = long(6);
        let map: [u8; 50] = bytes(input, map_at, 50)?.try_into().unwrap();
        let word = |at| u16::from_be_bytes(map[at..at + 2].try_into().unwrap());
        let value = |at| u32::from_be_bytes(map[at..at + 4].try_into().unwrap());
        let width = i32::from(word(12) as i16) - i32::from(word(8) as i16);
        let height = i32::from(word(10) as i16) - i32::from(word(6) as i16);
        let depth = usize::from(word(32));
        let row = usize::from(word(4) & 0x3fff);
        if value(0) != 0
            || word(4) & 0xc000 != 0x8000
            || word(14) != 0
            || word(16) != 0
            || value(18) != 0
            || value(22) != 0x00480000
            || value(26) != 0x00480000
            || word(30) != 0
            || ![1, 2, 4, 8].contains(&depth)
            || word(34) != 1
            || usize::from(word(36)) != depth
            || value(38) != 0
            || value(46) != 0
            || width <= 0
            || height <= 0
            || width > 4096
            || height > 4096
            || !(width as u32).is_power_of_two()
            || !(height as u32).is_power_of_two()
            || row == 0
            || width as usize * depth > row * 8
        {
            return Err(bad());
        }
        let pixel_len = row * height as usize;
        let table_at = value(42) as usize;
        let head = bytes(input, table_at, 8)?;
        let flags = u16::from_be_bytes(head[4..6].try_into().unwrap());
        let count = usize::from(u16::from_be_bytes(head[6..8].try_into().unwrap())) + 1;
        if count > 256 || flags & 0x4000 != 0 {
            return Err(bad());
        }
        let table_len = 8 + count * 8;
        let ranges = [
            (0, 28),
            (map_at, 50),
            (pixel_at, pixel_len),
            (table_at, table_len),
        ];
        for (i, &(at, n)) in ranges.iter().enumerate() {
            bytes(input, at, n)?;
            for &(other, len) in &ranges[..i] {
                if at < other + len && other < at + n {
                    return Err(bad());
                }
            }
        }
        let table = bytes(input, table_at, table_len)?;
        let mut defined = [false; 256];
        for ordinal in 0..count {
            let at = 8 + ordinal * 8;
            let index = if flags & 0x8000 != 0 {
                ordinal
            } else {
                usize::from(u16::from_be_bytes(table[at..at + 2].try_into().unwrap()))
            };
            if index >= 1usize << depth || defined[index] {
                return Err(bad());
            }
            defined[index] = true;
        }
        let pixels = bytes(input, pixel_at, pixel_len)?;
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
            map,
            pixels: pixels.to_vec(),
            table: table.to_vec(),
        })
    }
}
impl Manager {
    pub(crate) fn forget(&mut self, handle: u32) {
        self.owned.remove(&handle);
    }
    pub fn call(&mut self, memory: &mut Memory, process: &mut Process, op: Op) -> Result<u32> {
        match op {
            Op::Get(id) => self.get(memory, process, id),
            Op::Dispose(handle) => {
                self.dispose(memory, process, handle)?;
                Ok(0)
            }
        }
    }
    fn get(&mut self, memory: &mut Memory, process: &mut Process, id: i16) -> Result<u32> {
        let source = process.get_resource(memory, *b"ppat", id)?;
        if source == 0 {
            return Ok(0);
        }
        let size = process.handle_size(memory, source)? as usize;
        let data = GuestPtr(memory.u32(GuestPtr(source))?);
        let mut pattern = Pattern::decode(memory.read(data, size)?)?;
        let sizes = [
            28,
            50,
            pattern.pixels.len() as u32,
            pattern.table.len() as u32,
            0,
            0,
        ];
        let mut handles = Vec::new();
        let result = (|| -> Result<bool> {
            for size in sizes {
                let h = process.new_handle(memory, size, true)?;
                if h == 0 {
                    return Ok(false);
                }
                handles.push(h);
            }
            let [root, map, pixels, table, expanded, expanded_map]: [u32; 6] =
                handles.clone().try_into().unwrap();
            pattern.record[2..6].copy_from_slice(&map.to_be_bytes());
            pattern.record[6..10].copy_from_slice(&pixels.to_be_bytes());
            pattern.record[10..14].copy_from_slice(&expanded.to_be_bytes());
            pattern.record[16..20].copy_from_slice(&expanded_map.to_be_bytes());
            pattern.map[42..46].copy_from_slice(&table.to_be_bytes());
            for (h, bytes) in [
                (root, pattern.record.as_slice()),
                (map, pattern.map.as_slice()),
                (pixels, pattern.pixels.as_slice()),
                (table, pattern.table.as_slice()),
            ] {
                let pointer = GuestPtr(memory.u32(GuestPtr(h))?);
                memory.write(pointer, bytes)?;
            }
            self.owned
                .insert(root, [map, pixels, table, expanded, expanded_map]);
            Ok(true)
        })();
        match result {
            Ok(true) => Ok(handles[0]),
            other => {
                let error = process.mem_error;
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
        let children = *self
            .owned
            .get(&root)
            .ok_or_else(|| Error::Graphics("unknown owned PixPat".into()))?;
        let data = GuestPtr(memory.u32(GuestPtr(root))?);
        if process.handle_size(memory, root)? < 28 || process.mem_error != 0 {
            return Err(bad());
        }
        let record = memory.read(data, 28)?;
        for (at, h) in [
            (2, children[0]),
            (6, children[1]),
            (10, children[3]),
            (16, children[4]),
        ] {
            if u32::from_be_bytes(record[at..at + 4].try_into().unwrap()) != h {
                return Err(bad());
            }
        }
        for h in children.into_iter().chain([root]) {
            process.handle_size(memory, h)?;
            if process.mem_error != 0 {
                return Err(bad());
            }
            memory.validate_write(GuestPtr(h), 4)?;
        }
        if process.handle_size(memory, children[0])? < 50
            || process.handle_size(memory, children[2])? < 8
        {
            return Err(bad());
        }
        let map = GuestPtr(memory.u32(GuestPtr(children[0]))?);
        if memory.u32(GuestPtr(map.0 + 42))? != children[2] {
            return Err(bad());
        }
        // Preflight all handles, including independent pixel pins, before freeing any graph member.
        for h in children.into_iter().chain([root]) {
            if process.pixel_image_is_pinned(h) {
                return Err(Error::Graphics(
                    "disposing a pinned PixPat graph is unsupported".into(),
                ));
            }
        }
        self.owned.remove(&root);
        for h in children.into_iter().rev().chain([root]) {
            process.dispose_handle(memory, h)?;
        }
        Ok(())
    }
}
