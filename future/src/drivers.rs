//! Synchronous virtual driver dispatch; video CLUT is separate from QuickDraw.
use crate::{
    display::Display,
    memory::{GuestPtr, Memory},
    Error, Result,
};
/// Process-owned virtual display unit127; Sound remains unit3/reference-4.
/// This is a stable virtual identity, not an installed ROM driver claim.
pub const VIDEO_REFERENCE: i16 = -128;
pub struct Manager {
    palette: Box<[[u16; 3]; 256]>,
}
impl Default for Manager {
    fn default() -> Self {
        Self::new(Display::default())
    }
}
impl Manager {
    pub(crate) fn new(display: Display) -> Self {
        let mut palette = Box::new([[65535; 3]; 256]);
        for (out, rgb) in palette.iter_mut().zip(display.colors()) {
            *out = rgb;
        }
        Self { palette }
    }
    pub(crate) fn patch(&mut self, entries: &[(usize, [u16; 3])]) {
        for &(i, rgb) in entries {
            self.palette[i] = rgb;
        }
    }
    pub(crate) fn presentation(&self, display: Display) -> Result<crate::surface::Surface> {
        crate::surface::Surface::physical(display.bitmap(), display.depth(), self.palette.clone())
    }
    /// The ioResult word is committed before publishing infallible host metadata.
    /// Invalid driver references never dereference unused driver-specific params.
    pub fn control(
        &mut self,
        memory: &mut Memory,
        display: Display,
        pointer: GuestPtr,
    ) -> Result<i16> {
        let head = memory.read(pointer, 28)?;
        let reference = i16::from_be_bytes(head[24..26].try_into().unwrap());
        let code = i16::from_be_bytes(head[26..28].try_into().unwrap());
        let result = GuestPtr(
            pointer
                .0
                .checked_add(16)
                .ok_or_else(|| Error::Cpu("control parameter overflow".into()))?,
        );
        memory.validate_write(result, 2)?;
        if reference == -4 && code == 2 {
            return crate::sound::control(memory, pointer);
        }
        let mut replacement = None;
        let error: i16 = if reference != VIDEO_REFERENCE && reference != -4 {
            -21
        } else if reference == -4 || code != 3 || display.depth() != 8 {
            -17
        } else {
            let params = memory.u32(GuestPtr(
                pointer
                    .0
                    .checked_add(28)
                    .ok_or_else(|| Error::Cpu("control parameter overflow".into()))?,
            ))?;
            if params == 0 {
                memory.write(result, &(-50i16).to_be_bytes())?;
                return Ok(-50);
            }
            let record = memory.read(GuestPtr(params), 8)?;
            let table = GuestPtr(u32::from_be_bytes(record[..4].try_into().unwrap()));
            let start = i16::from_be_bytes(record[4..6].try_into().unwrap());
            let count = i16::from_be_bytes(record[6..8].try_into().unwrap());
            if !(-1..256).contains(&start)
                || !(-1..256).contains(&count)
                || (start >= 0 && i32::from(start) + i32::from(count) + 1 > 256)
            {
                -50
            } else if count == -1 {
                0
            } else if table.0 == 0 {
                -50
            } else {
                let input = memory.read(table, (count as usize + 1) * 8)?.to_vec();
                let mut palette = self.palette.clone();
                let mut valid = true;
                for (i, entry) in input.chunks_exact(8).enumerate() {
                    let index = if start == -1 {
                        usize::from(u16::from_be_bytes(entry[..2].try_into().unwrap()))
                    } else {
                        start as usize + i
                    };
                    if index >= 256 {
                        valid = false;
                        break;
                    }
                    palette[index] = std::array::from_fn(|c| {
                        u16::from_be_bytes([entry[2 + c * 2], entry[3 + c * 2]])
                    });
                }
                if valid {
                    replacement = Some(palette);
                    0
                } else {
                    -50
                }
            }
        };
        memory.write(result, &error.to_be_bytes())?;
        if let Some(palette) = replacement {
            self.palette = palette;
        }
        Ok(error)
    }
}
