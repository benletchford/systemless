//! Checked PEF preparation, based on Apple's Mac OS Runtime Architectures
//! (1997), chapter 8. No legacy loader/runtime dependency.
use super::resource::ResourceFork;
use super::{
    bytes, finish, invalid, limit, reloc, u16_at, u32_at, LoadedApplication, SECTION_BASE,
};
use crate::{
    abi::Abi,
    cpu::Architecture,
    memory::{GuestPtr, Memory, Permissions},
    Result,
};
use std::collections::BTreeSet;
const GATES: u32 = 0x7000_0000;
const VECTORS: u32 = 0x7001_0000;
struct Section {
    base: u32,
    default: u32,
    kind: u8,
    data: Vec<u8>,
}
pub(super) fn load(input: &[u8], resources: ResourceFork) -> Result<LoadedApplication> {
    limit(input.len())?;
    if bytes(input, 0, 12)? != b"Joy!peffpwpc" || u32_at(input, 12)? != 1 || u32_at(input, 36)? != 0
    {
        return Err(invalid("unsupported PEF header/architecture/version"));
    }
    let count = u16_at(input, 32)? as usize;
    let instantiated = u16_at(input, 34)? as usize;
    if count > 4096 || instantiated == 0 || instantiated > count || instantiated > 32767 {
        return Err(invalid("invalid PEF section count"));
    }
    let table_end = 40 + count * 28;
    bytes(input, 40, count * 28)?;
    let mut sections = Vec::new();
    let mut loader = None;
    let mut ranges = Vec::new();
    let mut total = 0usize;
    let mut next = SECTION_BASE;
    for i in 0..count {
        let h = 40 + i * 28;
        let default = u32_at(input, h + 4)?;
        let size = u32_at(input, h + 8)? as usize;
        let unpacked = u32_at(input, h + 12)? as usize;
        let packed = u32_at(input, h + 16)? as usize;
        let offset = u32_at(input, h + 20)? as usize;
        let kind = input[h + 24];
        let align = input[h + 26];
        if input[h + 27] != 0 || align > 28 {
            return Err(invalid("PEF section reserved/alignment field"));
        }
        let raw = bytes(input, offset, packed)?;
        if packed > 0 {
            if offset < table_end
                || ranges
                    .iter()
                    .any(|&(a, b)| offset < b && a < offset + packed)
            {
                return Err(invalid("overlapping PEF section contents"));
            }
            ranges.push((offset, offset + packed));
        }
        if i < instantiated {
            if !matches!(kind, 0 | 1 | 2 | 3 | 6) || unpacked > size || size == 0 {
                return Err(invalid("invalid instantiated PEF section"));
            }
            total = total
                .checked_add(size)
                .ok_or_else(|| invalid("PEF allocation overflow"))?;
            limit(total)?;
            let mut data = if kind == 2 {
                unpack(raw, unpacked)?
            } else {
                if packed != unpacked {
                    return Err(invalid("PEF raw section size mismatch"));
                }
                raw.to_vec()
            };
            data.resize(size, 0);
            let alignment = 1u32 << align;
            let base = next
                .checked_add(alignment - 1)
                .map(|v| v & !(alignment - 1))
                .ok_or_else(|| invalid("PEF section address overflow"))?;
            next = base
                .checked_add(size as u32)
                .and_then(|v| v.checked_add(15))
                .map(|v| v & !15)
                .filter(|v| *v < crate::process::HEAP_BASE)
                .ok_or_else(|| invalid("PEF exceeds address reservation"))?;
            sections.push(Section {
                base,
                default,
                kind,
                data,
            });
        } else if kind == 4 {
            if loader.replace(raw).is_some() {
                return Err(invalid("multiple PEF loader sections"));
            }
        } else if !matches!(kind, 5 | 7 | 8) {
            return Err(invalid("unsupported noninstantiated PEF section"));
        }
    }
    let l = loader.ok_or_else(|| invalid("missing PEF loader"))?;
    bytes(l, 0, 56)?;
    let main = u32_at(l, 0)? as usize;
    let main_offset = u32_at(l, 4)? as usize;
    let libraries = u32_at(l, 24)? as usize;
    let symbols = u32_at(l, 28)? as usize;
    let relocs = u32_at(l, 32)? as usize;
    if libraries > 4096 || symbols > 4096 || relocs > instantiated {
        return Err(invalid("PEF loader table limit"));
    }
    let imports_at = 56 + libraries * 24;
    let relocs_at = imports_at + symbols * 4;
    let tables_end = relocs_at + relocs * 12;
    bytes(l, 0, tables_end)?;
    let reloc_at = u32_at(l, 36)? as usize;
    let strings_at = u32_at(l, 40)? as usize;
    let exports_at = u32_at(l, 44)? as usize;
    if strings_at < tables_end
        || exports_at < strings_at
        || exports_at > l.len()
        || !exports_at.is_multiple_of(4)
    {
        return Err(invalid("PEF loader string/export boundaries"));
    }
    let strings = bytes(l, strings_at, exports_at - strings_at)?;
    // Application exports are retained in the file but need structural validation.
    let power = u32_at(l, 48)?;
    let exports = u32_at(l, 52)? as usize;
    if power > 18 || exports > 1 << 18 {
        return Err(invalid("PEF export limits"));
    }
    let hash_size = (1usize << power) * 4;
    let keys_at = exports_at + hash_size;
    let exports_table = keys_at + exports * 4;
    bytes(l, exports_at, hash_size + exports * 14)?;
    for slot in 0..1usize << power {
        let entry = u32_at(l, exports_at + slot * 4)?;
        if (entry & 0x3ffff) as usize + (entry >> 18) as usize > exports {
            return Err(invalid("PEF export hash range"));
        }
    }
    for e in 0..exports {
        let key = u32_at(l, keys_at + e * 4)?;
        let name = u32_at(l, exports_table + e * 10)? & 0xffffff;
        bytes(strings, name as usize, (key >> 16) as usize)?;
    }
    let mut abi = Abi::macintosh();
    let mut resolved = vec![None; symbols];
    let mut vectors = Vec::new();
    for i in 0..libraries {
        let p = 56 + i * 24;
        let name = cstring(strings, u32_at(l, p)? as usize, 63)?;
        let first = u32_at(l, p + 16)? as usize;
        let n = u32_at(l, p + 12)? as usize;
        let options = l[p + 20];
        if options & !0xc0 != 0
            || bytes(l, p + 21, 3)? != [0; 3]
            || first.checked_add(n).is_none_or(|v| v > symbols)
        {
            return Err(invalid("PEF import library range/options"));
        }
        for (index, slot) in resolved.iter_mut().enumerate().skip(first).take(n) {
            if slot.is_some() {
                return Err(invalid("overlapping PEF import libraries"));
            }
            let word = u32_at(l, imports_at + index * 4)?;
            let class = (word >> 24) as u8;
            let symbol = cstring(strings, (word & 0xffffff) as usize, 255)?;
            if class & 0x70 != 0 || class & 15 > 4 {
                return Err(invalid("invalid PEF symbol class"));
            }
            let address = if let Some(service) = Abi::resolve(name, symbol) {
                let gate = GATES + index as u32 * 4;
                abi.imports.insert(gate, service);
                match class & 15 {
                    0 | 4 => gate,
                    2 => {
                        let ptr = VECTORS + vectors.len() as u32;
                        vectors.extend(gate.to_be_bytes());
                        vectors.extend(0u32.to_be_bytes());
                        ptr
                    }
                    _ => return Err(invalid("function imported as data/TOC")),
                }
            } else if options & 0x40 != 0 || class & 0x80 != 0 {
                0
            } else {
                return Err(invalid(format!(
                    "unresolved import {}:{}",
                    String::from_utf8_lossy(name),
                    String::from_utf8_lossy(symbol)
                )));
            };
            *slot = Some(address);
        }
    }
    let imports = resolved
        .into_iter()
        .map(|s| s.ok_or_else(|| invalid("unowned PEF import")))
        .collect::<Result<Vec<_>>>()?;
    let deltas: Vec<_> = sections
        .iter()
        .map(|s| s.base.wrapping_sub(s.default))
        .collect();
    let mut relocated = BTreeSet::new();
    let mut work_budget = 1_000_000;
    if relocs > 0 && (reloc_at < tables_end || reloc_at > strings_at || !reloc_at.is_multiple_of(2))
    {
        return Err(invalid("PEF relocation area offset"));
    }
    for i in 0..relocs {
        let h = relocs_at + i * 12;
        let section = u16_at(l, h)? as usize;
        let count = u32_at(l, h + 4)? as usize;
        let start = u32_at(l, h + 8)? as usize;
        if u16_at(l, h + 2)? != 0 || !relocated.insert(section) || !start.is_multiple_of(2) {
            return Err(invalid("PEF relocation header"));
        }
        let area = bytes(l, reloc_at, strings_at - reloc_at)?;
        let stream = bytes(
            area,
            start,
            count
                .checked_mul(2)
                .ok_or_else(|| invalid("relocation size overflow"))?,
        )?;
        let target = sections
            .get_mut(section)
            .ok_or_else(|| invalid("PEF relocation target"))?;
        reloc::apply(
            &mut target.data,
            stream,
            &deltas,
            &imports,
            &mut work_budget,
        )?;
    }
    let main = routine(&sections, main, main_offset)?;
    let special = |field| -> Result<Option<crate::lifecycle::Routine>> {
        let section = u32_at(l, field)?;
        if section == u32::MAX {
            Ok(None)
        } else {
            routine(&sections, section as usize, u32_at(l, field + 4)? as usize).map(Some)
        }
    };
    let init = special(8)?;
    let term = special(16)?;
    let entry = main.entry;
    let toc = main.toc;
    let mut memory = Memory::default();
    let mut info = Vec::new();
    for section in sections {
        info.push((section.base, section.data.len() as u32));
        let permissions = match section.kind {
            0 => Permissions::RX,
            3 => Permissions {
                read: true,
                write: false,
                execute: false,
            },
            6 => Permissions {
                read: true,
                write: true,
                execute: true,
            },
            _ => Permissions::RW,
        };
        memory.map(GuestPtr(section.base), section.data, permissions)?;
    }
    if !vectors.is_empty() {
        memory.map(
            GuestPtr(VECTORS),
            vectors,
            Permissions {
                read: true,
                write: false,
                execute: false,
            },
        )?;
    }
    // Main TOC must refer to instantiated storage, never a host pointer.
    memory.read(GuestPtr(toc), 1)?;
    // Byte-slice loading is a memory container, even when the host CLI read a file.
    // System 7 CFragInitBlock: IDs at 0/4/8, locator at 12, name at 28.
    const CONTAINER: u32 = 0x6000_0000;
    const INIT_BLOCK: u32 = 0x6800_0000;
    for routine in [init, term].into_iter().flatten() {
        memory.fetch(GuestPtr(routine.entry), 4, 4)?;
        memory.read(GuestPtr(routine.toc), 1)?;
    }
    if init.is_some() {
        let ro = Permissions {
            read: true,
            write: false,
            execute: false,
        };
        memory.map(GuestPtr(CONTAINER), input.to_vec(), ro)?;
        let mut block = vec![0; 49];
        for (offset, value) in [
            (0, 1u32),
            (4, 1),
            (8, 1),
            (16, CONTAINER),
            (20, input.len() as u32),
            (28, INIT_BLOCK + 48),
        ] {
            block[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
        memory.map(GuestPtr(INIT_BLOCK), block, ro)?;
    }
    let mut app = finish(
        memory,
        Architecture::PowerPc,
        entry,
        toc,
        resources,
        info,
        abi,
    )?;
    app.runtime.lifecycle = Some(crate::lifecycle::Lifecycle::start(
        &mut app.runtime.cpu,
        init,
        term,
        INIT_BLOCK,
    ));
    Ok(app)
}
fn routine(sections: &[Section], index: usize, offset: usize) -> Result<crate::lifecycle::Routine> {
    let section = sections
        .get(index)
        .ok_or_else(|| invalid("PEF routine section"))?;
    if !offset.is_multiple_of(4) {
        return Err(invalid("unaligned PEF transition vector"));
    }
    let vector = bytes(&section.data, offset, 8)?;
    Ok(crate::lifecycle::Routine {
        entry: u32_at(vector, 0)?,
        toc: u32_at(vector, 4)?,
    })
}
fn cstring(b: &[u8], offset: usize, max: usize) -> Result<&[u8]> {
    let tail = bytes(
        b,
        offset,
        b.len()
            .checked_sub(offset)
            .ok_or_else(|| invalid("PEF string offset"))?,
    )?;
    let end = tail
        .iter()
        .position(|b| *b == 0)
        .filter(|n| *n <= max)
        .ok_or_else(|| invalid("PEF unterminated/oversized name"))?;
    Ok(&tail[..end])
}
/// All five Apple pidata operations. Output and loops are bounded by declared
/// unpacked size plus an explicit work budget; malformed input cannot spin.
fn unpack(input: &[u8], size: usize) -> Result<Vec<u8>> {
    limit(size)?;
    let mut out = Vec::with_capacity(size);
    let mut p = 0;
    let mut budget = 1_000_000usize;
    while p < input.len() {
        budget = budget
            .checked_sub(1)
            .ok_or_else(|| invalid("pidata work limit"))?;
        let opcode = input[p];
        p += 1;
        let n = if opcode & 31 == 0 {
            argument(input, &mut p)?
        } else {
            usize::from(opcode & 31)
        };
        match opcode >> 5 {
            0 => append(&mut out, None, n, size)?,
            1 => {
                let raw = bytes(input, p, n)?;
                append(&mut out, Some(raw), n, size)?;
                p += n;
            }
            2 => {
                let repeats = argument(input, &mut p)?
                    .checked_add(1)
                    .ok_or_else(|| invalid("pidata repeat overflow"))?;
                let raw = bytes(input, p, n)?;
                p += n;
                reserve_repeat(n, repeats, out.len(), size, &mut budget)?;
                for _ in 0..repeats {
                    append(&mut out, Some(raw), n, size)?;
                }
            }
            kind @ (3 | 4) => {
                let custom = argument(input, &mut p)?;
                let repeats = argument(input, &mut p)?;
                let common = if kind == 3 {
                    let raw = bytes(input, p, n)?;
                    p += n;
                    Some(raw)
                } else {
                    None
                };
                reserve_repeat(
                    n.checked_add(custom)
                        .ok_or_else(|| invalid("pidata size overflow"))?,
                    repeats,
                    out.len()
                        .checked_add(n)
                        .ok_or_else(|| invalid("pidata size overflow"))?,
                    size,
                    &mut budget,
                )?;
                for _ in 0..repeats {
                    append(&mut out, common, n, size)?;
                    let raw = bytes(input, p, custom)?;
                    p += custom;
                    append(&mut out, Some(raw), custom, size)?;
                }
                append(&mut out, common, n, size)?;
            }
            _ => return Err(invalid("reserved pidata opcode")),
        }
    }
    if out.len() != size {
        return Err(invalid("pidata expanded size mismatch"));
    }
    Ok(out)
}
fn argument(input: &[u8], p: &mut usize) -> Result<usize> {
    let mut value = 0u32;
    for _ in 0..5 {
        let b = bytes(input, *p, 1)?[0];
        *p += 1;
        value = value.wrapping_shl(7) | u32::from(b & 127);
        if b & 128 == 0 {
            return Ok(value as usize);
        }
    }
    Err(invalid("unterminated pidata count"))
}
fn append(out: &mut Vec<u8>, data: Option<&[u8]>, n: usize, size: usize) -> Result<()> {
    let end = out
        .len()
        .checked_add(n)
        .filter(|v| *v <= size)
        .ok_or_else(|| invalid("pidata output exceeds section"))?;
    if let Some(data) = data {
        out.extend_from_slice(data);
    } else {
        out.resize(end, 0);
    }
    Ok(())
}
fn reserve_repeat(
    n: usize,
    count: usize,
    current: usize,
    size: usize,
    budget: &mut usize,
) -> Result<()> {
    if n.checked_mul(count)
        .and_then(|v| v.checked_add(current))
        .is_none_or(|v| v > size)
    {
        return Err(invalid("pidata repetition exceeds section"));
    }
    *budget = budget
        .checked_sub(count)
        .ok_or_else(|| invalid("pidata work limit"))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_pidata_operations_and_extended_count() {
        assert_eq!(
            unpack(&[3, 0x22, 9, 8, 0x42, 2, 7, 6], 11).unwrap(),
            [0, 0, 0, 9, 8, 7, 6, 7, 6, 7, 6]
        );
        assert_eq!(
            unpack(&[0x61, 1, 2, b'A', b'1', b'2'], 5).unwrap(),
            b"A1A2A"
        );
        assert_eq!(
            unpack(&[0x81, 1, 2, b'1', b'2'], 5).unwrap(),
            [0, b'1', 0, b'2', 0]
        );
        assert_eq!(unpack(&[0, 0x81, 0], 128).unwrap(), vec![0; 128]);
    }
    #[test]
    fn pidata_rejects_truncation_overflow_reserved_and_zero_output_work_bombs() {
        for (b, size) in [
            (vec![0x23, 1], 3),
            (vec![0, 0x80], 4),
            (vec![0x42, 0xff, 0xff, 0xff, 0xff, 0x7f, 1, 2], 8),
            (vec![0xe1], 1),
            (vec![1], 2),
            (vec![3], 2),
            (vec![0x40, 0, 0xff, 0xff, 0xff, 0xff, 0x7f], 0),
        ] {
            assert!(unpack(&b, size).is_err(), "{b:x?}");
        }
    }
}
