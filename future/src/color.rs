//! Color Manager table storage, independent of CPU and host presentation.
//! Apple Advanced Color Imaging, ch. 7; QuickDraw Reference, MakeITable/QDError.
use crate::{
    devices,
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Make {
        source: u32,
        destination: u32,
        resolution: i16,
    },
    Error,
}
#[derive(Default)]
pub struct Manager {
    error: i16,
}
fn bad(message: &str) -> Error {
    Error::Graphics(message.into())
}
fn owned(m: &Memory, p: &mut Process, h: u32, min: u32) -> Result<GuestPtr> {
    let size = p.handle_size(m, h)?;
    if p.mem_error != 0 || size < min {
        return Err(bad(
            "Color Manager requires a live owned handle of sufficient size",
        ));
    }
    let data = m.u32(GuestPtr(h))?;
    if data == 0 {
        return Err(bad("Color Manager received an empty master pointer"));
    }
    Ok(GuestPtr(data))
}
pub(crate) fn nearest_color(
    rgb: [u16; 3],
    entries: impl Iterator<Item = (u8, [u16; 3])>,
) -> Option<u8> {
    entries
        .map(|(index, color)| {
            let distance = color
                .into_iter()
                .zip(rgb)
                .map(|(a, b)| {
                    let delta = i64::from(a) - i64::from(b);
                    (delta * delta) as u64
                })
                .sum::<u64>();
            ((distance, index), index)
        })
        .min_by_key(|(key, _)| *key)
        .map(|(_, i)| i)
}
struct Palette {
    seed: u32,
    entries: Vec<(u8, [u16; 3])>,
}
fn palette(m: &Memory, p: &mut Process, h: u32) -> Result<Palette> {
    let data = owned(m, p, h, 8)?;
    let head = m.read(data, 8)?;
    let last = i16::from_be_bytes(head[6..8].try_into().unwrap());
    if !(0..256).contains(&last) {
        return Err(bad("invalid inverse-table color count"));
    }
    let count = last as usize + 1;
    owned(m, p, h, (8 + 8 * count) as u32)?;
    let raw = m.read(data, 8 + 8 * count)?;
    let flags = u16::from_be_bytes(raw[4..6].try_into().unwrap());
    if flags & 0x4000 != 0 {
        return Err(bad("palette-relative inverse tables are unsupported"));
    }
    let device = flags & 0x8000 != 0;
    let mut used = [false; 256];
    let mut entries = Vec::with_capacity(count);
    for ordinal in 0..count {
        let at = 8 + ordinal * 8;
        let value = u16::from_be_bytes(raw[at..at + 2].try_into().unwrap());
        // Tagged/reserved client entries need a reservation model, not accidental
        // treatment as ordinary colors. Reject them until that model is available.
        if value > 255 {
            return Err(bad("tagged/reserved color entries are unsupported"));
        }
        let index = if device { ordinal as u8 } else { value as u8 };
        if used[index as usize] {
            return Err(bad("duplicate color table pixel value"));
        }
        used[index as usize] = true;
        let rgb = std::array::from_fn(|i| {
            u16::from_be_bytes(raw[at + 2 + i * 2..at + 4 + i * 2].try_into().unwrap())
        });
        entries.push((index, rgb));
    }
    Ok(Palette {
        seed: u32::from_be_bytes(raw[..4].try_into().unwrap()),
        entries,
    })
}
// Bounded integer cell-centre matcher. This is a generated software inverse
// table, not ROM lookup data. ROM tie/propagation and hidden-color matching
// conformance are separate unfinished Color2Index/drawing requirements.
fn table(palette: &Palette, resolution: u16) -> Vec<u8> {
    let count = 1usize << (3 * resolution);
    let mask = (1usize << resolution) - 1;
    let shift = 16 - resolution;
    let mut bytes = Vec::with_capacity(6 + count);
    bytes.extend(palette.seed.to_be_bytes());
    bytes.extend(resolution.to_be_bytes());
    for cell in 0..count {
        let rgb: [i64; 3] = std::array::from_fn(|i| {
            let q = (cell >> ((2 - i) * usize::from(resolution))) & mask;
            ((q << shift) + (1usize << (shift - 1))) as i64
        });
        let best = nearest_color(rgb.map(|v| v as u16), palette.entries.iter().copied()).unwrap();
        bytes.push(best);
    }
    bytes
}
impl Manager {
    pub fn call(&mut self, m: &mut Memory, p: &mut Process, op: Op) -> Result<u32> {
        let Op::Make {
            source,
            destination,
            resolution,
        } = op
        else {
            return Ok(self.error as i32 as u32);
        };
        let saved = (p.mem_error, p.res_error);
        let result = self.make(m, p, source, destination, resolution);
        (p.mem_error, p.res_error) = saved;
        let error = result?;
        self.error = error;
        Ok(0)
    }
    fn make(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        source: u32,
        destination: u32,
        mut resolution: i16,
    ) -> Result<i16> {
        if resolution != 0 && !(3..=5).contains(&resolution) {
            return Ok(-156);
        }
        // Validate explicit inputs before lazy default-device initialization.
        let explicit = if source != 0 {
            Some(palette(m, p, source)?)
        } else {
            None
        };
        if destination != 0 {
            owned(m, p, destination, 0)?;
            m.validate_write(GuestPtr(destination), 4)?;
        }
        let current = if source == 0 || destination == 0 || resolution == 0 {
            let mut manager = std::mem::take(&mut p.devices);
            let device = manager.call(m, p, devices::Op::Current);
            p.devices = manager;
            Some(owned(m, p, device?, 62)?)
        } else {
            None
        };
        if resolution == 0 {
            resolution = m.u16(GuestPtr(current.unwrap().0 + 10))? as i16;
        }
        if !(3..=5).contains(&resolution) {
            return Ok(-156);
        }
        let colors = if let Some(colors) = explicit {
            colors
        } else {
            let pixels = m.u32(GuestPtr(current.unwrap().0 + 22))?;
            let pm = owned(m, p, pixels, 50)?;
            palette(m, p, m.u32(GuestPtr(pm.0 + 42))?)?
        };
        let bytes = table(&colors, resolution as u16);
        let field = current.map(|gd| GuestPtr(gd.0 + 6));
        let target = if destination != 0 {
            destination
        } else {
            m.u32(field.unwrap())?
        };
        if target == 0 {
            let field = field.unwrap();
            m.validate_write(field, 4)?;
            let h = p.new_handle(m, bytes.len() as u32, false)?;
            if h == 0 {
                return Ok(p.mem_error);
            }
            // The allocator has prevalidated the writable heap and fresh master.
            let data = m.u32(GuestPtr(h))?;
            m.write(GuestPtr(data), &bytes)?;
            m.write(field, &h.to_be_bytes())?;
        } else {
            owned(m, p, target, 0)?;
            p.replace_handles(m, vec![(target, bytes)])?;
            if p.mem_error != 0 {
                return Ok(p.mem_error);
            }
        }
        Ok(0)
    }
}
