//! Synchronous process-local fork I/O. No host paths or asynchronous callbacks.
use super::{Files, Fork};
use crate::{
    memory::{GuestPtr, Memory},
    Result,
};
pub(super) const LIMIT: usize = 64 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Open { kind: Fork, hierarchical: bool },
    Create { hierarchical: bool },
    Delete { hierarchical: bool },
    SetInfo { hierarchical: bool },
    Close,
    Read,
    Write,
    GetEof,
    SetEof,
    GetPosition,
    SetPosition,
    Flush,
}
fn word(pb: &[u8], at: usize) -> i16 {
    i16::from_be_bytes(pb[at..at + 2].try_into().unwrap())
}
fn long(pb: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(pb[at..at + 4].try_into().unwrap())
}
impl Files {
    fn target(
        &self,
        memory: &Memory,
        pb: &[u8],
        hierarchical: bool,
    ) -> Result<std::result::Result<u32, i16>> {
        let volume = word(pb, 22);
        let parent = if hierarchical { long(pb, 48) } else { 0 };
        let name = Self::name_input(memory, GuestPtr(long(pb, 18)))?;
        Ok(self.parent(volume, parent).and_then(|parent| {
            if self.name.is_empty() {
                Err(-43)
            } else {
                self.path(&name, parent)
            }
        }))
    }
    fn resize_fork(
        &mut self,
        node: u32,
        kind: Fork,
        length: usize,
    ) -> std::result::Result<(), i16> {
        if node == 16 {
            return Err(-45);
        }
        let used: usize = self
            .files
            .values()
            .map(|f| f.data.len() + f.resource.len())
            .sum();
        let old = self.node_fork(node, kind).ok_or(-43i16)?.len();
        if length > LIMIT || used - old > LIMIT - length {
            return Err(-34);
        }
        let file = self.files.get_mut(&node).ok_or(-43i16)?;
        let bytes = match kind {
            Fork::Data => &mut file.data,
            Fork::Resource => &mut file.resource,
        };
        if length > bytes.len() {
            bytes
                .try_reserve_exact(length - bytes.len())
                .map_err(|_| -34i16)?;
        }
        bytes.resize(length, 0);
        if length < old {
            bytes.shrink_to_fit();
        }
        Ok(())
    }
    fn position(pb: &[u8], mark: u32, length: usize, read: bool) -> std::result::Result<u32, i16> {
        let mode = word(pb, 44) as u16;
        if (!read && mode > 3)
            || (read && (mode & 0x7c != 0 || (mode & 0x80 == 0 && mode & 0xff00 != 0)))
        {
            return Err(-50);
        }
        let offset = i64::from(long(pb, 46) as i32);
        let position = match mode & 3 {
            0 => i64::from(mark),
            1 => offset,
            2 => length as i64 + offset,
            3 => i64::from(mark) + offset,
            _ => unreachable!(),
        };
        if position < 0 {
            Err(-40)
        } else if position > i64::from(i32::MAX) {
            Err(-50)
        } else {
            Ok(position as u32)
        }
    }
    pub fn io(&mut self, memory: &mut Memory, pointer: GuestPtr, op: Op) -> Result<i16> {
        let size = match op {
            Op::SetInfo { .. } => 80,
            Op::Open {
                hierarchical: true, ..
            }
            | Op::Create { hierarchical: true }
            | Op::Delete { hierarchical: true } => 52,
            Op::Open { .. } => 28,
            Op::Create { .. } => 48,
            Op::Delete { .. } => 24,
            Op::Close | Op::Flush => 26,
            Op::GetEof | Op::SetEof => 32,
            _ => 50,
        };
        let mut pb = memory.read(pointer, size)?.to_vec();
        memory.validate_write(pointer, size)?;
        let error = self.execute_io(memory, &mut pb, op)?;
        pb[16..18].copy_from_slice(&error.to_be_bytes());
        memory.write(pointer, &pb)?;
        Ok(error)
    }
    fn execute_io(&mut self, memory: &mut Memory, pb: &mut [u8], op: Op) -> Result<i16> {
        if let Op::Delete { hierarchical } = op {
            let node = match self.target(memory, pb, hierarchical)? {
                Ok(node) => node,
                Err(error) => return Ok(error),
            };
            if node == 16 {
                return Ok(-45);
            }
            if node == 2
                || self.open.values().any(|f| f.node == node)
                || self.directories.contains_key(&node)
                    && (!self.children(node, false).is_empty()
                        || self
                            .working
                            .values()
                            .any(|&(directory, _)| directory == node))
            {
                return Ok(-47);
            }
            if self.files.remove(&node).is_none() {
                self.directories.remove(&node);
            }
            return Ok(0);
        }
        if let Op::Open { kind, hierarchical } = op {
            let selected = self.target(memory, pb, hierarchical)?;
            let error = match selected {
                Err(error) => Err(error),
                Ok(node) => {
                    let permission = pb[27];
                    if permission > 4 {
                        Err(-50)
                    } else if let Some(view) = self.file(node) {
                        if self.node_fork(node, kind).is_none() {
                            Err(-43)
                        } else {
                            let writers: Vec<_> = self
                                .open
                                .values()
                                .filter(|f| f.node == node && f.kind == kind && f.writable)
                                .collect();
                            let writable = if permission == 0 {
                                !view.locked && writers.is_empty()
                            } else {
                                permission != 1
                            };
                            if writable && view.locked {
                                Err(-45)
                            } else if writable
                                && writers.iter().any(|f| permission != 4 || !f.shared)
                            {
                                Err(-49)
                            } else {
                                self.open_node(node, kind, writable, permission == 4)
                            }
                        }
                    } else {
                        Err(-43)
                    }
                }
            };
            match error {
                Ok(reference) => {
                    pb[24..26].copy_from_slice(&reference.to_be_bytes());
                    return Ok(0);
                }
                Err(error) => return Ok(error),
            }
        }
        if let Op::SetInfo { hierarchical } = op {
            let node = match self.target(memory, pb, hierarchical)? {
                Ok(n) => n,
                Err(e) => return Ok(e),
            };
            if node == 16 {
                return Ok(-45);
            }
            let Some(file) = self.files.get_mut(&node) else {
                return Ok(-43);
            };
            file.finder.copy_from_slice(&pb[32..48]);
            file.created = long(pb, 72);
            file.modified = long(pb, 76);
            return Ok(0);
        }
        if let Op::Create { hierarchical } = op {
            let volume = word(pb, 22);
            let parent = if hierarchical { long(pb, 48) } else { 0 };
            let name = Self::name_input(memory, GuestPtr(long(pb, 18)))?;
            let created = memory.u32(GuestPtr(0x20c))?;
            return Ok(
                match self.create_file(
                    &name,
                    volume,
                    parent,
                    created,
                    pb[32..48].try_into().unwrap(),
                ) {
                    Ok(_) => 0,
                    Err(e) => e,
                },
            );
        }
        let reference = word(pb, 24);
        if matches!(op, Op::Read | Op::Write) {
            pb[40..44].fill(0);
        }
        let Some(file) = self.open.get(&reference).copied() else {
            return Ok(-51);
        };
        let length = self.node_fork(file.node, file.kind).unwrap().len();
        match op {
            Op::Close => {
                self.open.remove(&reference);
            }
            Op::Flush => {} // writes already commit to this process-local fork.
            Op::GetEof => pb[28..32].copy_from_slice(&(length as u32).to_be_bytes()),
            Op::GetPosition => pb[46..50].copy_from_slice(&file.mark.to_be_bytes()),
            Op::SetPosition => {
                let position = match Self::position(pb, file.mark, length, false) {
                    Ok(p) => p,
                    Err(error) => {
                        pb[46..50].copy_from_slice(&file.mark.to_be_bytes());
                        return Ok(error);
                    }
                };
                let actual = position.min(length as u32);
                self.open.get_mut(&reference).unwrap().mark = actual;
                pb[46..50].copy_from_slice(&actual.to_be_bytes());
                return Ok(if position > length as u32 { -39 } else { 0 });
            }
            Op::SetEof => {
                if !file.writable {
                    return Ok(-54);
                }
                let length = long(pb, 28) as i32;
                if length < 0 {
                    return Ok(-50);
                }
                let now = memory.u32(GuestPtr(0x20c))?;
                if let Err(error) = self.resize_fork(file.node, file.kind, length as usize) {
                    return Ok(error);
                }
                self.files.get_mut(&file.node).unwrap().modified = now;
            }
            Op::Read | Op::Write => {
                let requested = long(pb, 36) as i32;
                if requested < 0 {
                    return Ok(-50);
                }
                if op == Op::Write && !file.writable {
                    return Ok(-54);
                }
                let start = match Self::position(pb, file.mark, length, op == Op::Read) {
                    Ok(p) => p as usize,
                    Err(error) => {
                        pb[46..50].copy_from_slice(&file.mark.to_be_bytes());
                        return Ok(error);
                    }
                };
                let buffer = GuestPtr(long(pb, 32));
                let requested = requested as usize;
                let (actual, error) = if op == Op::Read {
                    let bytes = self.node_fork(file.node, file.kind).unwrap();
                    let mut actual = requested.min(length.saturating_sub(start));
                    let mut newline = false;
                    if word(pb, 44) as u16 & 0x80 != 0 && actual != 0 {
                        if let Some(index) = bytes[start..start + actual]
                            .iter()
                            .position(|&b| b == (word(pb, 44) as u16 >> 8) as u8)
                        {
                            actual = index + 1;
                            newline = true;
                        }
                    }
                    let data = if actual == 0 {
                        Vec::new()
                    } else {
                        bytes[start..start + actual].to_vec()
                    };
                    if actual != 0 {
                        memory.validate_write(buffer, actual)?;
                        memory.write(buffer, &data)?;
                    }
                    (
                        actual,
                        if !newline && actual < requested {
                            -39
                        } else {
                            0
                        },
                    )
                } else {
                    let Some(end) = start.checked_add(requested).filter(|&e| e <= LIMIT) else {
                        return Ok(-34);
                    };
                    let data = if requested == 0 {
                        Vec::new()
                    } else {
                        memory.read(buffer, requested)?.to_vec()
                    };
                    if requested != 0 {
                        let now = memory.u32(GuestPtr(0x20c))?;
                        if let Err(error) = self.resize_fork(file.node, file.kind, end.max(length))
                        {
                            return Ok(error);
                        }
                        let stored = self.files.get_mut(&file.node).unwrap();
                        let bytes = match file.kind {
                            Fork::Data => &mut stored.data,
                            Fork::Resource => &mut stored.resource,
                        };
                        bytes[start..end].copy_from_slice(&data);
                        stored.modified = now;
                    }
                    (requested, 0)
                };
                let mark = (start + actual) as u32;
                self.open.get_mut(&reference).unwrap().mark = mark;
                pb[40..44].copy_from_slice(&(actual as u32).to_be_bytes());
                pb[46..50].copy_from_slice(&mark.to_be_bytes());
                return Ok(error);
            }
            _ => unreachable!(),
        }
        Ok(0)
    }
}
