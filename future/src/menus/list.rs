//! Live DynamicMenuList edits; pixels change only through explicit menu drawing.
use super::{bad, data, record::Record, Menus, Op};
use crate::{font::Strike, memory::Memory, process::Process, Result};
type Entry = (u32, i16);
fn entries(bytes: &[u8]) -> Vec<Entry> {
    bytes
        .as_chunks::<6>()
        .0
        .iter()
        .map(|b| {
            (
                u32::from_be_bytes(b[..4].try_into().unwrap()),
                i16::from_be_bytes(b[4..6].try_into().unwrap()),
            )
        })
        .collect()
}
fn append(bytes: &mut Vec<u8>, entries: &[Entry]) {
    for (handle, value) in entries {
        bytes.extend(handle.to_be_bytes());
        bytes.extend(value.to_be_bytes());
    }
}
fn idle(list: &[u8], count: usize) -> Result<()> {
    if list[8 + count..12 + count] != [0; 4] {
        return Err(bad("editing saved menu tracking state is unsupported"));
    }
    Ok(())
}
impl Menus {
    pub(super) fn edit_list(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        op: Op,
    ) -> Result<u32> {
        let saved = process.mem_error;
        let result = (|| {
            let (old, mut colors) = self.check(memory, process)?;
            self.validate_menus(memory, process, &old)?;
            let count = usize::from(u16::from_be_bytes(old[..2].try_into().unwrap()));
            let mut regular = entries(&old[6..6 + count]);
            let mut submenus = entries(&old[12 + count..]);
            for &(h, _) in regular.iter().chain(&submenus) {
                if h == self.list || h == self.colors {
                    return Err(bad("menu record aliases manager storage"));
                }
            }
            let mut changes = Vec::new();
            let regular_changed;
            match op {
                Op::Insert { handle, before } => {
                    if handle == self.list || handle == self.colors {
                        return Err(bad("menu record aliases manager storage"));
                    }
                    let mut bytes = data(memory, process, handle)?;
                    Record::parse(&bytes)?;
                    if regular.iter().chain(&submenus).any(|e| e.0 == handle) {
                        return Ok(false);
                    }
                    let target = if before == -1 {
                        &mut submenus
                    } else {
                        &mut regular
                    };
                    if target.len() >= 1024 {
                        return Ok(false);
                    }
                    let mut index = target.len();
                    if before != 0 && before != -1 {
                        for (i, &(h, _)) in target.iter().enumerate() {
                            if Record::parse(&data(memory, process, h)?)?.id == before {
                                index = i;
                                break;
                            }
                        }
                    }
                    idle(&old, count)?;
                    self.measure_menu(memory, process, &mut bytes)?;
                    changes.push((handle, bytes));
                    target.insert(index, (handle, 0));
                    regular_changed = before != -1;
                }
                Op::Delete(id) => {
                    let mut found = None;
                    for (hierarchical, target) in [(true, &submenus), (false, &regular)] {
                        for (i, &(h, _)) in target.iter().enumerate() {
                            if Record::parse(&data(memory, process, h)?)?.id == id {
                                found = Some((hierarchical, i));
                                break;
                            }
                        }
                        if found.is_some() {
                            break;
                        }
                    }
                    let Some((hierarchical, index)) = found else {
                        return Ok(false);
                    };
                    idle(&old, count)?;
                    if hierarchical {
                        submenus.remove(index);
                    } else {
                        regular.remove(index);
                    }
                    colors = colors
                        .as_chunks::<30>()
                        .0
                        .iter()
                        .filter(|e| i16::from_be_bytes(e[..2].try_into().unwrap()) != id)
                        .flatten()
                        .copied()
                        .collect();
                    changes.push((self.colors, colors));
                    regular_changed = !hierarchical;
                }
                _ => unreachable!(),
            }
            let mut right = i16::from_be_bytes(old[2..4].try_into().unwrap());
            if regular_changed {
                let mut left = 11i32;
                if !regular.is_empty() {
                    let bytes = crate::text::font_bytes(memory, process, 0, 0, 0)?;
                    let font = Strike::parse(&bytes)?;
                    for (handle, origin) in &mut regular {
                        *origin =
                            i16::try_from(left).map_err(|_| bad("menu title position overflow"))?;
                        let bytes = data(memory, process, *handle)?;
                        let menu = Record::parse(&bytes)?;
                        left = left
                            .checked_add(font.width(menu.title)? + 13)
                            .ok_or_else(|| bad("menu title position overflow"))?;
                    }
                    right = i16::try_from(left).map_err(|_| bad("menu title position overflow"))?;
                } else {
                    right = 0;
                }
            }
            let mut list = Vec::with_capacity(12 + 6 * (regular.len() + submenus.len()));
            list.extend((regular.len() as u16 * 6).to_be_bytes());
            list.extend(right.to_be_bytes());
            list.extend_from_slice(&old[4..6]);
            append(&mut list, &regular);
            list.extend((submenus.len() as u16 * 6).to_be_bytes());
            list.extend_from_slice(&old[8 + count..12 + count]);
            append(&mut list, &submenus);
            changes.push((self.list, list));
            process.replace_handles(memory, changes)?;
            Ok(true)
        })();
        match result {
            Ok(changed) => {
                if !changed {
                    process.mem_error = saved;
                }
                Ok(0)
            }
            Err(error) => {
                process.mem_error = saved;
                Err(error)
            }
        }
    }
}
