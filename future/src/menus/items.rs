//! Live menu mutations share the process resource map and checked heap.
//! AppendResMenu: Macintosh Toolbox Essentials (1992), pp. 3-101–3-102.
//! Case ordering follows the legacy MacRoman-to-Unicode lowercase comparison;
//! full Script Manager collation is not implemented.
use super::{bad, data, record::Record};
use crate::{
    memory::{GuestPtr, Memory},
    process::Process,
    Result,
};
fn sort_key(name: &[u8]) -> String {
    const HIGH: [char; 128] = [
        '\u{c4}', '\u{c5}', '\u{c7}', '\u{c9}', '\u{d1}', '\u{d6}', '\u{dc}', '\u{e1}', '\u{e0}',
        '\u{e2}', '\u{e4}', '\u{e3}', '\u{e5}', '\u{e7}', '\u{e9}', '\u{e8}', '\u{ea}', '\u{eb}',
        '\u{ed}', '\u{ec}', '\u{ee}', '\u{ef}', '\u{f1}', '\u{f3}', '\u{f2}', '\u{f4}', '\u{f6}',
        '\u{f5}', '\u{fa}', '\u{f9}', '\u{fb}', '\u{fc}', '\u{2020}', '\u{b0}', '\u{a2}', '\u{a3}',
        '\u{a7}', '\u{2022}', '\u{b6}', '\u{df}', '\u{ae}', '\u{a9}', '\u{2122}', '\u{b4}',
        '\u{a8}', '\u{2260}', '\u{c6}', '\u{d8}', '\u{221e}', '\u{b1}', '\u{2264}', '\u{2265}',
        '\u{a5}', '\u{b5}', '\u{2202}', '\u{2211}', '\u{220f}', '\u{3c0}', '\u{222b}', '\u{aa}',
        '\u{ba}', '\u{3a9}', '\u{e6}', '\u{f8}', '\u{bf}', '\u{a1}', '\u{ac}', '\u{221a}',
        '\u{192}', '\u{2248}', '\u{2206}', '\u{ab}', '\u{bb}', '\u{2026}', '\u{a0}', '\u{c0}',
        '\u{c3}', '\u{d5}', '\u{152}', '\u{153}', '\u{2013}', '\u{2014}', '\u{201c}', '\u{201d}',
        '\u{2018}', '\u{2019}', '\u{f7}', '\u{25ca}', '\u{ff}', '\u{178}', '\u{2044}', '\u{20ac}',
        '\u{2039}', '\u{203a}', '\u{fb01}', '\u{fb02}', '\u{2021}', '\u{b7}', '\u{201a}',
        '\u{201e}', '\u{2030}', '\u{c2}', '\u{ca}', '\u{c1}', '\u{cb}', '\u{c8}', '\u{cd}',
        '\u{ce}', '\u{cf}', '\u{cc}', '\u{d3}', '\u{d4}', '\u{f8ff}', '\u{d2}', '\u{da}', '\u{db}',
        '\u{d9}', '\u{131}', '\u{2c6}', '\u{2dc}', '\u{af}', '\u{2d8}', '\u{2d9}', '\u{2da}',
        '\u{b8}', '\u{2dd}', '\u{2db}', '\u{2c7}',
    ];
    name.iter()
        .map(|&c| {
            if c < 128 {
                char::from(c)
            } else {
                HIGH[usize::from(c - 128)]
            }
        })
        .flat_map(char::to_lowercase)
        .collect()
}
pub(super) fn append_resources(
    memory: &mut Memory,
    process: &mut Process,
    handle: u32,
    kind: [u8; 4],
) -> Result<u32> {
    let original = data(memory, process, handle)?;
    let menu = Record::parse(&original)?;
    memory.validate_write(GuestPtr(0xa5e), 1)?;
    let records = process
        .resources
        .entries()
        .filter(|r| r.kind == kind || (kind == *b"FONT" && r.kind == *b"FOND"))
        .filter_map(|r| r.name.as_ref().map(|name| (r.kind, r.id, name.clone())))
        .collect::<Vec<_>>();
    if matches!(&kind, b"FONT" | b"FOND") && records.iter().any(|(_, id, _)| *id as u16 > 0x4000) {
        return Err(bad("non-Roman resource menu scripts are unsupported"));
    }
    let existing = menu
        .items
        .iter()
        .map(|i| i.text.to_vec())
        .collect::<std::collections::BTreeSet<_>>();
    let mut names = std::collections::BTreeSet::new();
    for (kind, id, name) in records {
        let h = process.get_resource(memory, kind, id)?;
        if h == 0 {
            memory.write(GuestPtr(0xa5e), &[1])?;
            return Ok(0);
        }
        if !name.is_empty() && !matches!(name[0], b'.' | b'%') && !existing.contains(&name) {
            names.insert(name);
        }
    }
    let mut names = names
        .into_iter()
        .map(|name| (sort_key(&name), name))
        .collect::<Vec<_>>();
    names.sort();
    if menu.items.len() + names.len() > 1024 {
        return Err(bad("resource menu item limit exceeded"));
    }
    if !names.is_empty() {
        let start = 15 + menu.title.len();
        let end = start
            + menu
                .items
                .iter()
                .map(|i| 1 + i.text.len() + 4)
                .sum::<usize>();
        let mut bytes = original[..end].to_vec();
        let mut enabled = menu.enabled;
        for (n, (_, name)) in names.into_iter().enumerate() {
            bytes.push(name.len() as u8);
            bytes.extend(name);
            bytes.extend([0; 4]);
            let index = menu.items.len() + n + 1;
            if index <= 31 {
                enabled |= 1u32 << index;
            }
        }
        bytes.push(0);
        bytes[10..14].copy_from_slice(&enabled.to_be_bytes());
        process.replace_handles(memory, vec![(handle, bytes)])?;
    }
    memory.write(GuestPtr(0xa5e), &[1])?;
    Ok(0)
}

pub(super) fn set_byte(
    memory: &mut Memory,
    process: &mut Process,
    handle: u32,
    item: i16,
    value: u8,
    attribute: usize,
) -> Result<u32> {
    let error = process.mem_error;
    let result = (|| {
        let bytes = data(memory, process, handle)?;
        let menu = Record::parse(&bytes)?;
        if item <= 0 || item as usize > menu.items.len() {
            return Ok(0);
        }
        let at = 15
            + menu.title.len()
            + menu.items[..item as usize - 1]
                .iter()
                .map(|i| 1 + i.text.len() + 4)
                .sum::<usize>()
            + 1
            + menu.items[item as usize - 1].text.len()
            + attribute;
        let base = memory.u32(GuestPtr(handle))?;
        memory.write(GuestPtr(base + at as u32), &[value])?;
        Ok(0)
    })();
    process.mem_error = error;
    result
}

pub(super) fn set_text(
    memory: &mut Memory,
    process: &mut Process,
    handle: u32,
    item: i16,
    source: GuestPtr,
) -> Result<u32> {
    let bytes = data(memory, process, handle)?;
    let menu = Record::parse(&bytes)?;
    if item <= 0 || item as usize > menu.items.len() {
        return Ok(0);
    }
    let length = usize::from(memory.read(source, 1)?[0]);
    let text = memory.read(source, length + 1)?.to_vec();
    if length == 0 {
        return Err(bad("empty menu item text is unsupported"));
    }
    let start = 15
        + menu.title.len()
        + menu.items[..item as usize - 1]
            .iter()
            .map(|i| 1 + i.text.len() + 4)
            .sum::<usize>();
    let old_end = start + 1 + menu.items[item as usize - 1].text.len();
    let end = 15
        + menu.title.len()
        + menu
            .items
            .iter()
            .map(|i| 1 + i.text.len() + 4)
            .sum::<usize>()
        + 1;
    let mut result = bytes[..start].to_vec();
    result.extend(text);
    result.extend_from_slice(&bytes[old_end..end]);
    process.replace_handles(memory, vec![(handle, result)])?;
    Ok(0)
}

/// AppendMenu's metacharacters describe item attributes; unlike AppendResMenu,
/// the supplied names are parsed rather than copied literally.
pub(super) fn append_specs(
    memory: &mut Memory,
    process: &mut Process,
    handle: u32,
    source: GuestPtr,
) -> Result<u32> {
    let original = data(memory, process, handle)?;
    let menu = Record::parse(&original)?;
    let n = usize::from(memory.read(source, 1)?[0]);
    let input = memory.read(source, n + 1)?;
    let mut additions = Vec::new();
    for spec in input[1..]
        .split(|c| matches!(*c, b';' | b'\r'))
        .filter(|s| !s.is_empty())
    {
        let mut text = Vec::new();
        let mut fields = [0u8; 4];
        let mut enabled = true;
        let mut at = 0;
        while at < spec.len() {
            match spec[at] {
                b'(' => {
                    enabled = false;
                    at += 1;
                }
                b'^' | b'/' | b'!' if at + 1 < spec.len() => {
                    let field = match spec[at] {
                        b'^' => 0,
                        b'/' => 1,
                        _ => 2,
                    };
                    fields[field] = if field == 0 {
                        spec[at + 1].saturating_sub(b'0')
                    } else {
                        spec[at + 1]
                    };
                    at += 2;
                }
                b'<' if at + 1 < spec.len() => {
                    at += 1;
                    while at < spec.len() {
                        let style = match spec[at] {
                            b'B' => 1,
                            b'I' => 2,
                            b'U' => 4,
                            b'O' => 8,
                            b'S' => 16,
                            _ => break,
                        };
                        fields[3] |= style;
                        at += 1;
                    }
                }
                c => {
                    text.push(c);
                    at += 1;
                }
            }
        }
        if text.is_empty() {
            return Err(bad("menu item specification has no text"));
        }
        additions.push((text, fields, enabled));
    }
    if additions.is_empty() {
        return Ok(0);
    }
    if menu.items.len() + additions.len() > 1024 {
        return Err(bad("menu item limit exceeded"));
    }
    let end = 15
        + menu.title.len()
        + menu
            .items
            .iter()
            .map(|i| 1 + i.text.len() + 4)
            .sum::<usize>();
    let mut bytes = original[..end].to_vec();
    let mut flags = menu.enabled;
    for (i, (text, fields, enabled)) in additions.into_iter().enumerate() {
        bytes.push(text.len() as u8);
        bytes.extend(text);
        bytes.extend(fields);
        let item = menu.items.len() + i + 1;
        if item <= 31 {
            if enabled {
                flags |= 1u32 << item;
            } else {
                flags &= !(1u32 << item);
            }
        }
    }
    bytes.push(0);
    bytes[10..14].copy_from_slice(&flags.to_be_bytes());
    process.replace_handles(memory, vec![(handle, bytes)])?;
    Ok(0)
}
