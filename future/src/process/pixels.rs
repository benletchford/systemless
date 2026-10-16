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
    pub(super) fn pixel_locked(&self, handle: u32) -> bool {
        self.pixel_locks.values().any(|lock| lock.image == handle)
    }
    pub(super) fn pixel_pinned(&self, handle: u32) -> bool {
        self.cicons.pins(handle) || self.pixel_locked(handle)
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
    /// Save/restore the image's actual purge/movement policy, not PixMap flags.
    pub fn pixels_state(
        &mut self,
        memory: &mut Memory,
        pixmap: u32,
        action: PixelState,
    ) -> Result<u32> {
        self.pixel_base(memory, pixmap)?;
        let (_, base, _) = self.pixel_record(memory, pixmap)?;
        let image = self
            .pixel_locks
            .get(&pixmap)
            .map_or(base, |held| held.image);
        if image == pixmap {
            return Err(bad());
        }
        let block =
            self.handles.get(&image).copied().ok_or_else(|| {
                Error::Graphics("pixel states require managed image storage".into())
            })?;
        if self.cicons.pins(image) {
            return Err(Error::Graphics(
                "pixel states cannot manage private icon storage".into(),
            ));
        }
        let locked = block.locked || self.pixel_locked(image);
        match action {
            PixelState::Get => {
                Ok(u32::from(block.attributes & 0x40) | if locked { 0x80 } else { 0 })
            }
            PixelState::Purge(purge) => {
                // Apple requires UnlockPixels before AllowPurgePixels.
                if purge && locked {
                    return Err(Error::Graphics(
                        "unlock the image before AllowPurgePixels".into(),
                    ));
                }
                let block = self.handles.get_mut(&image).unwrap();
                block.attributes = if purge {
                    block.attributes | 0x40
                } else {
                    block.attributes & !0x40
                };
                Ok(0)
            }
            PixelState::Set(state) => {
                if state & !0xc0 != 0 {
                    return Err(Error::Graphics("unsupported pixel-state flags".into()));
                }
                let lock = state & 0x80 != 0;
                if lock && block.data == 0 {
                    return Err(Error::Graphics(
                        "cannot restore a lock on purged pixels".into(),
                    ));
                }
                // This preflights any baseAddr change before touching handle flags.
                // Other PixMap/CIcon owners retain their independent image pins.
                self.pixels(memory, pixmap, lock)?;
                let block = self.handles.get_mut(&image).unwrap();
                block.attributes = (block.attributes & !0x40) | (state as u8 & 0x40);
                block.locked = false; // Requested lock ownership is now the PixMap pin.
                Ok(0)
            }
        }
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
