//! IM: Imaging With QuickDraw, pp. 6-26–6-27: lock image storage, not its PixMap.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct Lock {
    image: u32,
    data: u32,
}
fn bad() -> Error {
    Error::Graphics("invalid owned pixel image or PixMap".into())
}
impl Process {
    pub(super) fn pixel_pinned(&self, handle: u32) -> bool {
        self.cicons.pins(handle) || self.pixel_locks.values().any(|lock| lock.image == handle)
    }
    fn pixel_record(&self, memory: &Memory, pixmap: u32) -> Result<(u32, u32, u32)> {
        let pm = self.handles.get(&pixmap).copied().ok_or_else(bad)?;
        if pm.data == 0 || pm.size < 50 || memory.u32(GuestPtr(pixmap))? != pm.data {
            return Err(bad());
        }
        let bytes = memory.read(GuestPtr(pm.data), 50)?;
        let base = u32::from_be_bytes(bytes[..4].try_into().unwrap());
        let row = u16::from_be_bytes(bytes[4..6].try_into().unwrap());
        let top = i16::from_be_bytes(bytes[6..8].try_into().unwrap());
        let bottom = i16::from_be_bytes(bytes[10..12].try_into().unwrap());
        if row & 0xc000 != 0x8000 || bottom < top {
            return Err(bad());
        }
        let extent = u32::from(row & 0x3fff) * (i32::from(bottom) - i32::from(top)) as u32;
        Ok((pm.data, base, extent))
    }
    fn fixed_pixels(&self, memory: &Memory, base: u32, extent: u32) -> Result<()> {
        let fixed = self.pointers.get(&base).is_some_and(|size| *size >= extent)
            || (base == crate::quickdraw::SCREEN.0
                && extent <= self.display.bitmap().row_bytes as u32 * self.display.height());
        if !fixed {
            return Err(bad());
        }
        memory.read(GuestPtr(base), extent as usize)?;
        Ok(())
    }
    pub fn pixel_base(&self, memory: &Memory, pixmap: u32) -> Result<u32> {
        let (_, base, extent) = self.pixel_record(memory, pixmap)?;
        if let Some(held) = self.pixel_locks.get(&pixmap) {
            let image = self.handles.get(&held.image).ok_or_else(bad)?;
            if base != held.data
                || image.data != held.data
                || image.size < extent
                || memory.u32(GuestPtr(held.image))? != held.data
            {
                return Err(bad());
            }
            return Ok(held.data);
        }
        if let Some(image) = self.handles.get(&base) {
            if base == pixmap || memory.u32(GuestPtr(base))? != image.data {
                return Err(bad());
            }
            if image.data == 0 {
                return Ok(0);
            }
            if image.size < extent {
                return Err(bad());
            }
            memory.read(GuestPtr(image.data), extent as usize)?;
            return Ok(image.data);
        }
        self.fixed_pixels(memory, base, extent)?;
        Ok(base)
    }
    pub fn pixels(&mut self, memory: &mut Memory, pixmap: u32, lock: bool) -> Result<bool> {
        let (pm, base, extent) = self.pixel_record(memory, pixmap)?;
        if let Some(held) = self.pixel_locks.get(&pixmap).copied() {
            let image = self.handles.get(&held.image).ok_or_else(bad)?;
            if base != held.data
                || image.data != held.data
                || image.size < extent
                || memory.u32(GuestPtr(held.image))? != held.data
            {
                return Err(bad());
            }
            if !lock {
                memory.validate_write(GuestPtr(pm), 4)?;
                memory.write(GuestPtr(pm), &held.image.to_be_bytes())?;
                self.pixel_locks.remove(&pixmap);
            }
            return Ok(true);
        }
        if let Some(image) = self.handles.get(&base).copied() {
            if base == pixmap || memory.u32(GuestPtr(base))? != image.data {
                return Err(bad());
            }
            if image.data == 0 {
                return Ok(false);
            }
            if image.size < extent {
                return Err(bad());
            }
            memory.read(GuestPtr(image.data), extent as usize)?;
            if lock {
                memory.validate_write(GuestPtr(pm), 4)?;
                memory.write(GuestPtr(pm), &image.data.to_be_bytes())?;
                self.pixel_locks.insert(
                    pixmap,
                    Lock {
                        image: base,
                        data: image.data,
                    },
                );
            }
            return Ok(true);
        }
        // NewPtr-backed application PixMaps and the fixed main screen cannot move.
        self.fixed_pixels(memory, base, extent)?;
        Ok(true)
    }
}
