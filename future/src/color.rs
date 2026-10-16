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
    RgbToHsl {
        source: GuestPtr,
        destination: GuestPtr,
    },
    SetEntries {
        start: i16,
        count: i16,
        source: GuestPtr,
    },
    Seed,
    Make {
        source: u32,
        destination: u32,
        resolution: i16,
    },
    Protect {
        index: i16,
        protect: bool,
    },
    Reserve {
        index: i16,
        reserve: bool,
    },
    Error,
}
#[derive(Default)]
pub struct Manager {
    error: i16,
    serial: u32,
}
// The device ColorSpec high byte is reserved to the Color/Palette Managers.
// This runtime stores protection in bit 15; public pixel indices use ordinals.
pub(crate) const PROTECTED: u16 = 0x8000;
pub(crate) const RESERVED: u16 = 0x4000;
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
        // Protection does not hide colors from matching. Reserved colors are excluded
        // from generated inverse tables, independently of the current client.
        if value
            & !(if device {
                PROTECTED | RESERVED | 255
            } else {
                255
            })
            != 0
        {
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
        if !device || value & RESERVED == 0 {
            entries.push((index, rgb));
        }
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
// Normalized sixteen-bit components, with exact rational intermediates and
// nearest rounding (half upward). This matches the legacy numerical policy;
// native Color Picker last-bit rounding remains a separate conformance check.
fn rgb_to_hsl(rgb: [u16; 3]) -> [u16; 3] {
    let [r, g, b] = rgb.map(i64::from);
    let high = r.max(g).max(b);
    let low = r.min(g).min(b);
    let sum = high + low;
    let delta = high - low;
    let lightness = ((sum + 1) / 2) as u16;
    if delta == 0 {
        return [0, 0, lightness];
    }
    let mut hue = if high == r {
        g - b
    } else if high == g {
        2 * delta + b - r
    } else {
        4 * delta + r - g
    };
    if hue < 0 {
        hue += 6 * delta;
    }
    let round = |numerator: i64, denominator: i64| {
        ((numerator * 65535 + denominator / 2) / denominator) as u16
    };
    let saturation = round(delta, if sum <= 65535 { sum } else { 131070 - sum });
    [round(hue, 6 * delta), saturation, lightness]
}
impl Manager {
    pub fn call(&mut self, m: &mut Memory, p: &mut Process, op: Op) -> Result<u32> {
        if let Op::RgbToHsl {
            source,
            destination,
        } = op
        {
            if source.0 == 0 || destination.0 == 0 {
                return Err(bad("nil Color Picker conversion record"));
            }
            let raw = m.read(source, 6)?;
            let rgb = std::array::from_fn(|i| u16::from_be_bytes([raw[i * 2], raw[i * 2 + 1]]));
            m.validate_write(destination, 6)?;
            let bytes: Vec<_> = rgb_to_hsl(rgb)
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect();
            m.write(destination, &bytes)?;
            return Ok(0);
        }
        if op == Op::Seed {
            let seed = self.next_seed(0)?;
            self.serial = seed;
            return Ok(seed);
        }
        if op == Op::Error {
            return Ok(self.error as i32 as u32);
        }
        let saved = (p.mem_error, p.res_error);
        let result = match op {
            Op::Make {
                source,
                destination,
                resolution,
            } => self.make(m, p, source, destination, resolution),
            Op::Protect { index, protect } => self.protect(m, p, index, protect),
            Op::SetEntries {
                start,
                count,
                source,
            } => self.set_entries(m, p, start, count, source),
            Op::Reserve { index, reserve } => self.entry(m, p, index, reserve, RESERVED),
            Op::Error | Op::Seed | Op::RgbToHsl { .. } => unreachable!(),
        };
        (p.mem_error, p.res_error) = saved;
        let error = result?;
        self.error = error;
        Ok(0)
    }
    fn set_entries(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        start: i16,
        count: i16,
        source: GuestPtr,
    ) -> Result<i16> {
        if count == -1 {
            return Ok(0);
        }
        if start < -1 || !(0..256).contains(&count) {
            return Ok(-153);
        }
        if source.0 == 0 {
            return Err(bad("nil SetEntries source"));
        }
        // Snapshot before lazy allocation or destination writes: input may alias
        // the device table, and index-mode requests may repeat an index.
        let input = m.read(source, (count as usize + 1) * 8)?.to_vec();
        let mut devices = std::mem::take(&mut p.devices);
        let device = devices.call(m, p, devices::Op::Current);
        p.devices = devices;
        let gd = owned(m, p, device?, 62)?;
        if m.u16(GuestPtr(gd.0 + 4))? != 0 {
            return Ok(-155);
        }
        let pm = owned(m, p, m.u32(GuestPtr(gd.0 + 22))?, 50)?;
        let depth = m.u16(GuestPtr(pm.0 + 32))?;
        if ![1, 2, 4, 8].contains(&depth) {
            return Ok(-155);
        }
        let handle = m.u32(GuestPtr(pm.0 + 42))?;
        let table = owned(m, p, handle, 8)?;
        let last = m.u16(GuestPtr(table.0 + 6))? as i16;
        if !(-1..256).contains(&last) {
            return Err(bad("invalid SetEntries device table count"));
        }
        if m.u16(GuestPtr(table.0 + 4))? != 0x8000 {
            return Err(bad("SetEntries requires an ordinary device table"));
        }
        let length = 8 + (i32::from(last) + 1) as usize * 8;
        owned(m, p, handle, length as u32)?;
        let mut replacement = m.read(table, length)?.to_vec();
        let client = m.u16(GuestPtr(gd.0 + 2))? & 255;
        for (i, entry) in input.chunks_exact(8).enumerate() {
            let index = if start == -1 {
                usize::from(u16::from_be_bytes(entry[..2].try_into().unwrap()))
            } else {
                start as usize + i
            };
            if index >= (1usize << depth) || index as i32 > i32::from(last) {
                return Ok(-154);
            }
            let at = 8 + index * 8;
            let metadata = u16::from_be_bytes(replacement[at..at + 2].try_into().unwrap());
            if metadata & PROTECTED != 0 || (metadata & RESERVED != 0 && metadata & 255 != client) {
                return Ok(-154);
            }
            replacement[at..at + 2].copy_from_slice(&((metadata & 0xff00) | client).to_be_bytes());
            replacement[at + 2..at + 8].copy_from_slice(&entry[2..]);
        }
        let seed = self.next_seed(m.u32(table)?)?;
        replacement[..4].copy_from_slice(&seed.to_be_bytes());
        m.validate_write(table, length)?;
        m.write(table, &replacement)?;
        self.serial = seed;
        Ok(0)
    }
    fn next_seed(&self, previous: u32) -> Result<u32> {
        self.serial
            .max(previous)
            .max(1023)
            .checked_add(1)
            .ok_or_else(|| bad("color seed namespace exhausted"))
    }
    fn protect(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        index: i16,
        protect: bool,
    ) -> Result<i16> {
        self.entry(m, p, index, protect, PROTECTED)
    }
    fn entry(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        index: i16,
        enable: bool,
        flag: u16,
    ) -> Result<i16> {
        if !(0..256).contains(&index) {
            return Ok(-153);
        }
        let mut devices = std::mem::take(&mut p.devices);
        let device = devices.call(m, p, devices::Op::Current);
        p.devices = devices;
        let gd = owned(m, p, device?, 62)?;
        let pixels = owned(m, p, m.u32(GuestPtr(gd.0 + 22))?, 50)?;
        let table = m.u32(GuestPtr(pixels.0 + 42))?;
        let data = owned(m, p, table, 8)?;
        let last = m.u16(GuestPtr(data.0 + 6))? as i16;
        if !(-1..256).contains(&last) {
            return Err(bad("invalid device color count"));
        }
        if index > last {
            return Ok(-153);
        }
        if m.u16(GuestPtr(data.0 + 4))? != 0x8000 {
            return Err(bad("protection requires an ordinary device table"));
        }
        owned(m, p, table, 8 + (last as u32 + 1) * 8)?;
        let entry = GuestPtr(data.0 + 8 + index as u32 * 8);
        let old = m.u16(entry)?;
        if enable && old & flag != 0 {
            return Ok(-154);
        }
        let value = if enable {
            if flag == RESERVED {
                (old & 0xff00) | flag | (m.u16(GuestPtr(gd.0 + 2))? & 255)
            } else {
                old | flag
            }
        } else {
            old & !flag
        };
        let seed = self.next_seed(m.u32(data)?)?;
        m.validate_write(data, 4)?;
        m.validate_write(entry, 2)?;
        m.write(entry, &value.to_be_bytes())?;
        m.write(data, &seed.to_be_bytes())?;
        self.serial = seed;
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
        if colors.entries.is_empty() {
            return Ok(-150);
        }
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
