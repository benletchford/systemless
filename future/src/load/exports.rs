//! PEF export tables: Mac OS Runtime Architectures (1997), chapter 8,
//! "Exported Symbols". Names are length-delimited original bytes, not UTF-8.
use super::{bytes, invalid, limit, u16_at, u32_at};
use crate::{memory::GuestPtr, Result};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: Vec<u8>,
    /// PEF annotation: code=0, data=1, transition vector=2, TOC=3, glue=4.
    pub class: u8,
    pub address: GuestPtr,
}

pub(super) fn resolve(
    loader: &[u8],
    strings: &[u8],
    sections: &[(u32, u32)],
    imports: &[u32],
) -> Result<Vec<Export>> {
    let start = u32_at(loader, 44)? as usize;
    let power = u32_at(loader, 48)?;
    let count = u32_at(loader, 52)? as usize;
    if power > 18 || count > 1 << 18 {
        return Err(invalid("PEF export limits"));
    }
    let slots = 1usize << power;
    let keys = start + slots * 4;
    let symbols = keys + count * 4;
    bytes(loader, start, slots * 4 + count * 14)?;
    // Each table entry must belong to exactly one hash chain. Validating before
    // resolving names bounds traversal even for intentionally overlapping chains.
    let mut owners = vec![None; count];
    for slot in 0..slots {
        let chain = u32_at(loader, start + slot * 4)?;
        let first = (chain & 0x3ffff) as usize;
        let length = (chain >> 18) as usize;
        let end = first + length;
        if end > count {
            return Err(invalid("PEF export hash range"));
        }
        for owner in &mut owners[first..end] {
            if owner.replace(slot as u32).is_some() {
                return Err(invalid("overlapping PEF export hash chains"));
            }
        }
    }
    let mut exports = Vec::with_capacity(count);
    let mut names = BTreeSet::new();
    let mut name_bytes = 0usize;
    for (index, owner) in owners.into_iter().enumerate() {
        let key = u32_at(loader, keys + index * 4)?;
        let entry = symbols + index * 10;
        let word = u32_at(loader, entry)?;
        let class = (word >> 24) as u8;
        if class > 4 {
            return Err(invalid("PEF export class/flags"));
        }
        let name = bytes(strings, (word & 0xffffff) as usize, (key >> 16) as usize)?;
        name_bytes = name_bytes
            .checked_add(name.len())
            .ok_or_else(|| invalid("PEF export name size overflow"))?;
        limit(name_bytes)?;
        let expected_slot = (key ^ (key >> power)) & (slots as u32 - 1);
        if hash(name) != key || owner != Some(expected_slot) {
            return Err(invalid("PEF export hash mismatch or unowned symbol"));
        }
        if !names.insert(name) {
            return Err(invalid("duplicate PEF export name"));
        }
        let value = u32_at(loader, entry + 4)?;
        let section = u16_at(loader, entry + 8)? as i16;
        let address = match section {
            -2 => value,
            -3 => *imports
                .get(value as usize)
                .ok_or_else(|| invalid("PEF reexport import index"))?,
            0.. => {
                let &(base, size) = sections
                    .get(section as usize)
                    .ok_or_else(|| invalid("PEF export section index"))?;
                // Linkers may export an end-of-section marker. Its address is
                // valid metadata; any later guest dereference remains checked.
                if value > size {
                    return Err(invalid("PEF export offset outside section"));
                }
                base.checked_add(value)
                    .ok_or_else(|| invalid("PEF export address overflow"))?
            }
            _ => return Err(invalid("PEF export special section")),
        };
        exports.push(Export {
            name: name.to_vec(),
            class,
            address: GuestPtr(address),
        });
    }
    Ok(exports)
}

fn hash(name: &[u8]) -> u32 {
    let mut state = 0i32;
    let mut length = 0u32;
    for &byte in name {
        if byte == 0 {
            break;
        }
        state = state.wrapping_shl(1).wrapping_sub(state >> 16) ^ i32::from(byte);
        length += 1;
    }
    (length << 16) | ((state ^ (state >> 16)) as u32 & 0xffff)
}
