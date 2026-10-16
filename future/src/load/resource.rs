//! Resource file format: Inside Macintosh: More Macintosh Toolbox (1993),
//! pp. 1-121–1-128, figures 1-11–1-18. Names remain original MacRoman bytes.
mod encode;
use super::{bytes, invalid, limit, u16_at, u32_at};
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub kind: [u8; 4],
    pub id: i16,
    pub attributes: u8,
    pub name: Option<Vec<u8>>,
    pub data: Vec<u8>,
}
#[derive(Debug, Default, Clone)]
pub struct ResourceFork {
    pub attributes: u16,
    entries: BTreeMap<([u8; 4], i16), Resource>,
}
impl ResourceFork {
    pub(crate) fn insert_unique(&mut self, resource: Resource) -> Result<()> {
        if self.entries.contains_key(&(resource.kind, resource.id)) {
            return Err(invalid(
                "duplicate resource type/ID mutation is not supported",
            ));
        }
        self.entries.insert((resource.kind, resource.id), resource);
        Ok(())
    }
    pub fn get(&self, kind: [u8; 4], id: i16) -> Option<&Resource> {
        self.entries.get(&(kind, id))
    }
    pub fn entries(&self) -> impl Iterator<Item = &Resource> {
        self.entries.values()
    }
    pub fn parse(input: &[u8]) -> Result<Self> {
        limit(input.len())?;
        let d = u32_at(input, 0)? as usize;
        let m = u32_at(input, 4)? as usize;
        let dl = u32_at(input, 8)? as usize;
        let ml = u32_at(input, 12)? as usize;
        let data = bytes(input, d, dl)?;
        let map = bytes(input, m, ml)?;
        if d < 16 || m < 16 || overlaps(&(d..d + dl), &(m..m + ml)) {
            return Err(invalid("resource header/data/map overlap"));
        }
        bytes(map, 0, 28)?;
        let types = u16_at(map, 24)? as usize;
        let names = u16_at(map, 26)? as usize;
        if types < 28 || names > map.len() {
            return Err(invalid("resource type/name offset"));
        }
        let count = count_minus_one(u16_at(map, types)?);
        let type_end = types + 2 + count * 8;
        bytes(map, types, 2 + count * 8)?;
        if names < type_end {
            return Err(invalid("resource names overlap type list"));
        }
        let mut result = Self {
            attributes: u16_at(map, 22)?,
            ..Default::default()
        };
        let mut kinds = BTreeSet::new();
        let mut reference_ranges = BTreeMap::from([(0, type_end)]);
        let mut data_ranges = BTreeMap::new();
        for n in 0..count {
            let t = types + 2 + n * 8;
            let kind = bytes(map, t, 4)?.try_into().unwrap();
            if !kinds.insert(kind) {
                return Err(invalid("duplicate resource type"));
            }
            let refs = count_minus_one(u16_at(map, t + 4)?);
            let start = types + u16_at(map, t + 6)? as usize;
            bytes(map, start, refs * 12)?;
            let range = start..start + refs * 12;
            if range.end > names || !insert_range(&mut reference_ranges, &range) {
                return Err(invalid("overlapping resource references"));
            }

            for n in 0..refs {
                let r = start + n * 12;
                let id = u16_at(map, r)? as i16;
                let name_offset = u16_at(map, r + 2)?;
                let packed = u32_at(map, r + 4)?;
                let offset = (packed & 0xffffff) as usize;
                let len = u32_at(data, offset)? as usize;
                let contents = bytes(data, offset + 4, len)?;
                let range = offset..offset + 4 + len;
                if data_ranges
                    .range(..=range.start)
                    .next_back()
                    .is_some_and(|(_, end)| *end > range.start)
                    || data_ranges
                        .range(range.start..)
                        .next()
                        .is_some_and(|(start, _)| *start < range.end)
                {
                    return Err(invalid("overlapping resource payloads"));
                }
                data_ranges.insert(range.start, range.end);
                if result.entries.len() >= 65536 {
                    return Err(invalid("resource count limit"));
                }
                let name = if name_offset == 0xffff {
                    None
                } else {
                    let p = names + name_offset as usize;
                    let len = bytes(map, p, 1)?[0] as usize;
                    Some(bytes(map, p + 1, len)?.to_vec())
                };
                let resource = Resource {
                    kind,
                    id,
                    attributes: (packed >> 24) as u8,
                    name,
                    data: contents.to_vec(),
                };
                if result.entries.insert((kind, id), resource).is_some() {
                    return Err(invalid("duplicate resource ID"));
                }
            }
        }
        Ok(result)
    }
}
fn count_minus_one(v: u16) -> usize {
    if v == 0xffff {
        0
    } else {
        usize::from(v) + 1
    }
}
fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < a.end && b.start < b.end && a.start < b.end && b.start < a.end
}

fn insert_range(ranges: &mut BTreeMap<usize, usize>, range: &Range<usize>) -> bool {
    if range.is_empty() {
        return true;
    }
    if ranges
        .range(..=range.start)
        .next_back()
        .is_some_and(|(_, end)| *end > range.start)
        || ranges
            .range(range.start..)
            .next()
            .is_some_and(|(start, _)| *start < range.end)
    {
        return false;
    }
    ranges.insert(range.start, range.end);
    true
}
