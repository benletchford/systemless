//! Checked FOND association lookup, with live resource data and bounded outline preparation.
//! Layout: Apple Inside Macintosh: Text; Adobe technical note 0091 (width tables).
use crate::{
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};

fn bad(message: &str) -> Error {
    Error::Load(message.into())
}
fn word(bytes: &[u8], offset: usize) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| bad("truncated FOND table"))?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}
fn long(bytes: &[u8], offset: usize) -> Result<usize> {
    Ok((u32::from(word(bytes, offset)?) << 16 | u32::from(word(bytes, offset + 2)?)) as usize)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Association {
    size: u16,
    style: u16,
    id: i16,
}
struct Family<'a> {
    bytes: &'a [u8],
    first: u16,
    last: u16,
    associations: Vec<Association>,
}
impl<'a> Family<'a> {
    fn parse(bytes: &'a [u8], id: i16) -> Result<Self> {
        if bytes.len() < 60 || bytes.len() > 1024 * 1024 {
            return Err(bad("invalid FOND size"));
        }
        if word(bytes, 2)? as i16 != id {
            return Err(bad("FOND family ID mismatch"));
        }
        let (first, last) = (word(bytes, 4)?, word(bytes, 6)?);
        if first > last || last > 255 {
            return Err(bad("invalid FOND character range"));
        }
        let count = usize::from(word(bytes, 52)?) + 1;
        let end = 54 + count * 6;
        if end > bytes.len() {
            return Err(bad("truncated FOND association table"));
        }
        let mut associations: Vec<Association> = Vec::with_capacity(count);
        let mut seen = std::collections::BTreeSet::new();
        for offset in (54..end).step_by(6) {
            let entry = Association {
                size: word(bytes, offset)?,
                style: word(bytes, offset + 2)?,
                id: word(bytes, offset + 4)? as i16,
            };
            if entry.size > i16::MAX as u16 {
                return Err(bad("invalid FOND association size"));
            }
            if !seen.insert((entry.size, entry.style)) {
                return Err(bad("duplicate FOND association"));
            }
            if let Some(previous) = associations.last() {
                if entry.size < previous.size || (entry.size == previous.size && entry.style == 0) {
                    return Err(bad(
                        "FOND associations must sort sizes and place plain first",
                    ));
                }
            }
            associations.push(entry);
        }
        for offset in [16, 20, 24] {
            let table = long(bytes, offset)?;
            if table != 0 && (table < end || table > bytes.len().saturating_sub(2)) {
                return Err(bad("invalid FOND optional table offset"));
            }
        }
        Ok(Self {
            bytes,
            first,
            last,
            associations,
        })
    }
    fn widths(&self, style: u16, size: i16) -> Result<Option<[u8; 257]>> {
        let offset = long(self.bytes, 16)?;
        if offset == 0 || word(self.bytes, 0)? & 0x4000 != 0 {
            return Ok(None);
        }
        let count = usize::from(word(self.bytes, offset)?) + 1;
        // Adobe 0091: style, each character, missing character, unused zero word.
        let stride = usize::from(self.last - self.first + 4) * 2;
        let end = offset
            .checked_add(2 + count * stride)
            .ok_or_else(|| bad("FOND width table overflow"))?;
        let next = [20, 24]
            .into_iter()
            .map(|field| long(self.bytes, field))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|table| *table > offset)
            .min()
            .unwrap_or(self.bytes.len());
        if end > self.bytes.len() || end > next {
            return Err(bad("truncated or overlapping FOND width table"));
        }
        let mut selected = None;
        for index in 0..count {
            let entry = offset + 2 + index * stride;
            if word(self.bytes, entry + stride - 2)? != 0 {
                return Err(bad("invalid FOND width table terminator"));
            }
            if word(self.bytes, entry)? != style {
                continue;
            }
            if selected.is_some() {
                return Err(bad("duplicate FOND width style"));
            }
            let scaled = |code: usize| -> Result<u8> {
                let value = i32::from(word(self.bytes, entry + 2 + code * 2)? as i16);
                let pixels = (value * i32::from(size) + 2048) >> 12;
                u8::try_from(pixels).map_err(|_| bad("FOND integer glyph width outside 0..255"))
            };
            let mut widths = [scaled(usize::from(self.last - self.first + 1))?; 257];
            for code in self.first..=self.last {
                widths[usize::from(code)] = scaled(usize::from(code - self.first))?;
            }
            selected = Some(widths);
        }
        Ok(selected)
    }
}
pub(crate) struct CachedOutline {
    family: Vec<u8>,
    source: Vec<u8>,
    size: i16,
    style: u16,
    strike: Vec<u8>,
}
fn resource(memory: &mut Memory, process: &mut Process, kind: [u8; 4], id: i16) -> Result<Vec<u8>> {
    let handle = process.get_resource(memory, kind, id)?;
    let length = process.handle_size(memory, handle)? as usize;
    if length > 4 * 1024 * 1024 {
        return Err(bad("font source resource exceeds 4 MiB"));
    }
    let pointer = GuestPtr(memory.u32(GuestPtr(handle))?);
    Ok(memory.read(pointer, length)?.to_vec())
}
pub(crate) fn resolve(
    memory: &mut Memory,
    process: &mut Process,
    family: i16,
    size: i16,
    face: u8,
) -> Result<Option<Vec<u8>>> {
    if process.find_resource(*b"FOND", family).is_none() {
        return Ok(None);
    }
    let bytes = resource(memory, process, *b"FOND", family)?;
    let fond = Family::parse(&bytes, family)?;
    // Prefer an intrinsic style; synthesize bold from plain only if necessary.
    for style in [u16::from(face), 0]
        .into_iter()
        .take(if face == 0 { 1 } else { 2 })
    {
        for kind in [*b"sfnt", *b"NFNT", *b"FONT"] {
            for entry in &fond.associations {
                if entry.style != style
                    || entry.size != if kind == *b"sfnt" { 0 } else { size as u16 }
                {
                    continue;
                }
                if process.find_resource(kind, entry.id).is_none() {
                    continue;
                }
                let source = resource(memory, process, kind, entry.id)?;
                let mut strike = if kind == *b"sfnt" {
                    if let Some(cached) = process.fonts.outlines.iter().find(|c| {
                        c.family == bytes
                            && c.source == source
                            && c.size == size
                            && c.style == style
                    }) {
                        cached.strike.clone()
                    } else {
                        let font = ab_glyph::FontRef::try_from_slice(&source)
                            .map_err(|_| bad("invalid sfnt outline font"))?;
                        let widths = fond.widths(style, size)?;
                        let strike = crate::font_fallback::outline(font, size, widths.as_ref())?;
                        let cost = bytes.len() + source.len() + strike.len();
                        const LIMIT: usize = 8 * 1024 * 1024;
                        if cost <= LIMIT {
                            while process.fonts.outlines.len() >= 8
                                || process
                                    .fonts
                                    .outlines
                                    .iter()
                                    .map(|c| c.family.len() + c.source.len() + c.strike.len())
                                    .sum::<usize>()
                                    + cost
                                    > LIMIT
                            {
                                process.fonts.outlines.remove(0);
                            }
                            process.fonts.outlines.push(CachedOutline {
                                family: bytes.clone(),
                                source: source.clone(),
                                size,
                                style,
                                strike: strike.clone(),
                            });
                        }
                        strike
                    }
                } else {
                    crate::font::Strike::parse(&source)?;
                    source
                };
                if face == 1 && style == 0 {
                    strike = crate::font::embolden(&strike)?;
                }
                return Ok(Some(strike));
            }
        }
    }
    Err(bad(
        "FOND has no supported outline or exact-size monochrome association",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 74];
        for (offset, value) in [
            (2, 2020u16),
            (4, 65),
            (6, 66),
            (54, 14),
            (58, 9295),
            (62, 0),
            (64, 2048),
            (66, 4096),
            (68, 1024),
        ] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }
        bytes[16..20].copy_from_slice(&60u32.to_be_bytes());
        bytes
    }
    #[test]
    fn family_widths_include_missing_character_and_round_4_12_values() {
        let bytes = fixture();
        let family = Family::parse(&bytes, 2020).unwrap();
        let widths = family.widths(0, 14).unwrap().unwrap();
        assert_eq!(
            (widths[65], widths[66], widths[256], widths[0]),
            (7, 14, 4, 4)
        );
        assert!(family.widths(1, 14).unwrap().is_none());
        let mut disabled = bytes.clone();
        disabled[0..2].copy_from_slice(&0x4000u16.to_be_bytes());
        assert!(Family::parse(&disabled, 2020)
            .unwrap()
            .widths(0, 14)
            .unwrap()
            .is_none());
    }
    #[test]
    fn malformed_family_tables_return_errors_without_panics() {
        let bytes = fixture();
        for length in 0..60 {
            assert!(Family::parse(&bytes[..length], 2020).is_err());
        }
        for length in 60..72 {
            assert!(Family::parse(&bytes[..length], 2020)
                .and_then(|family| family.widths(0, 14))
                .is_err());
        }
        assert!(Family::parse(&bytes, 2021).is_err());
        for (offset, value) in [
            (4, 256u16),
            (52, 0xffff),
            (54, 0x8000),
            (16, 0xffff),
            (70, 1),
        ] {
            let mut corrupt = bytes.clone();
            corrupt[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
            let result = Family::parse(&corrupt, 2020).and_then(|f| f.widths(0, 14));
            assert!(result.is_err(), "offset {offset}");
        }
        let mut overlap = bytes;
        overlap[24..28].copy_from_slice(&68u32.to_be_bytes());
        assert!(Family::parse(&overlap, 2020)
            .unwrap()
            .widths(0, 14)
            .is_err());
    }
}

#[cfg(test)]
mod association_order_tests {
    use super::*;
    #[test]
    fn styled_associations_need_not_be_numerically_sorted_but_plain_and_sizes_do() {
        let mut bytes = vec![0; 72];
        for (offset, value) in [
            (2, 2020u16),
            (6, 255),
            (52, 2),
            (54, 14),
            (58, 100),
            (60, 14),
            (62, 2),
            (64, 101),
            (66, 14),
            (68, 1),
            (70, 102),
        ] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }
        assert_eq!(Family::parse(&bytes, 2020).unwrap().associations.len(), 3);
        for (offset, value) in [(62, 0u16), (68, 2), (66, 13)] {
            let mut corrupt = bytes.clone();
            corrupt[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
            assert!(Family::parse(&corrupt, 2020).is_err());
        }
    }
}
