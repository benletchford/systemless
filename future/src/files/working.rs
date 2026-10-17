//! Process-local working-directory references for classic Standard File replies.
//! IM: Files (1992), pp. 2-201–2-204. Only synchronous parameter-block routes.
use super::Files;
use crate::{
    memory::{GuestPtr, Memory},
    Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingOp {
    Open,
    Close,
    Info,
    Current { hierarchical: bool },
    Set { hierarchical: bool },
}
const FIRST: i16 = -32767;
const LAST: i16 = -32640;
fn missing(reference: i16) -> i16 {
    if reference < -32000 {
        -51
    } else {
        -35
    }
}
#[derive(Clone, Copy)]
pub(super) struct DefaultDirectory {
    directory: u32,
    reference: i16,
    classic: i16,
    owner: u32,
}
impl Default for DefaultDirectory {
    fn default() -> Self {
        Self {
            directory: 2,
            reference: -1,
            classic: -1,
            owner: 0,
        }
    }
}
impl Files {
    pub fn default_directory(&self) -> u32 {
        self.default_directory.directory
    }
    fn select_directory(
        &mut self,
        name: &[u8],
        reference: i16,
        directory: u32,
        hierarchical: bool,
    ) -> std::result::Result<(), i16> {
        if self.name.is_empty() {
            return Err(-35);
        }
        if name.contains(&0) {
            return Err(-37);
        }
        if !hierarchical {
            if !name.is_empty() {
                let volume = name.strip_suffix(b":").unwrap_or(name);
                if !volume.eq_ignore_ascii_case(b"Application Volume") {
                    return Err(-35);
                }
                self.default_directory = DefaultDirectory::default();
                return Ok(());
            }
            let (directory, owner) = if matches!(reference, -1..=1) {
                (2, 0)
            } else {
                *self.working.get(&reference).ok_or(-35i16)?
            };
            self.default_directory = DefaultDirectory {
                directory,
                reference: if matches!(reference, -1..=1) {
                    -1
                } else {
                    reference
                },
                classic: if matches!(reference, -1..=1) {
                    -1
                } else {
                    reference
                },
                owner,
            };
            return Ok(());
        }
        let full = !name.starts_with(b":") && name.contains(&b':');
        let wd = if full || matches!(reference, -1..=1) {
            None
        } else {
            Some(*self.working.get(&reference).ok_or(-35i16)?)
        };
        let base = if full {
            2
        } else if let Some((d, _)) = wd {
            d
        } else if directory != 0 {
            directory
        } else {
            self.default_directory.directory
        };
        let selected = self
            .path(name, base)
            .map_err(|e| if e == -120 { -43 } else { e })?;
        if !self.directory_exists(selected) {
            return Err(-43);
        }
        let keep = wd.is_some() && name.is_empty();
        self.default_directory = DefaultDirectory {
            directory: selected,
            reference: if keep { reference } else { -1 },
            classic: -1,
            owner: if keep { wd.unwrap().1 } else { 0 },
        };
        Ok(())
    }

    pub(super) fn classic_parent(&self, volume: i16) -> std::result::Result<u32, i16> {
        self.parent(volume, if matches!(volume, -1 | 1) { 2 } else { 0 })
    }
    // A nonzero HFS directory ID overrides the directory carried by a WD.
    // A plain File Manager operation supplies zero and follows the WD itself.
    pub(super) fn parent(&self, volume: i16, directory: u32) -> std::result::Result<u32, i16> {
        let default = if matches!(volume, -1..=1) {
            self.default_directory.directory
        } else {
            self.working.get(&volume).ok_or_else(|| missing(volume))?.0
        };
        Ok(if directory == 0 { default } else { directory })
    }
    pub fn open_working_directory(
        &mut self,
        volume: i16,
        directory: u32,
        owner: u32,
    ) -> std::result::Result<i16, i16> {
        let directory = self.parent(volume, directory)?;
        if self.name.is_empty() {
            return Err(-43);
        }
        if !self.directory_exists(directory) {
            return Err(-120);
        }
        if let Some((&reference, _)) = self
            .working
            .iter()
            .find(|(_, entry)| **entry == (directory, owner))
        {
            return Ok(reference);
        }
        let reference = (FIRST..=LAST)
            .find(|r| !self.working.contains_key(r))
            .ok_or(-121i16)?;
        self.working.insert(reference, (directory, owner));
        Ok(reference)
    }
    pub fn close_working_directory(&mut self, reference: i16) -> std::result::Result<(), i16> {
        if matches!(reference, -1..=1) {
            return Ok(());
        }
        self.working
            .remove(&reference)
            .ok_or_else(|| missing(reference))?;
        if self.default_directory.reference == reference {
            self.default_directory.reference = -1;
            self.default_directory.classic = -1;
            self.default_directory.owner = 0;
        }
        Ok(())
    }
    pub fn working_directory(
        &mut self,
        memory: &mut Memory,
        pointer: GuestPtr,
        op: WorkingOp,
    ) -> Result<i16> {
        let size = match op {
            WorkingOp::Close
            | WorkingOp::Set {
                hierarchical: false,
            }
            | WorkingOp::Current {
                hierarchical: false,
            } => 24,
            _ => 52,
        };
        let mut pb = memory.read(pointer, size)?.to_vec();
        memory.validate_write(pointer, size)?;
        let reference = i16::from_be_bytes(pb[22..24].try_into().unwrap());
        let name_pointer = GuestPtr(u32::from_be_bytes(pb[18..22].try_into().unwrap()));
        let error = match op {
            WorkingOp::Current { hierarchical } => {
                if self.name.is_empty() {
                    -35
                } else {
                    let raw = if !hierarchical && self.default_directory.classic < -32000 {
                        self.directories
                            .get(&self.default_directory.directory)
                            .map(|d| d.name.as_slice())
                            .unwrap_or(b"Application Volume")
                    } else {
                        b"Application Volume"
                    };
                    let mut name = vec![raw.len() as u8];
                    name.extend(raw);
                    if name_pointer.0 != 0 {
                        memory.validate_write(name_pointer, name.len())?;
                    }
                    pb[22..24].copy_from_slice(
                        &(if hierarchical {
                            self.default_directory.reference
                        } else {
                            self.default_directory.classic
                        })
                        .to_be_bytes(),
                    );
                    if hierarchical {
                        pb[28..32].copy_from_slice(&self.default_directory.owner.to_be_bytes());
                        pb[32..34].copy_from_slice(&(-1i16).to_be_bytes());
                        pb[48..52].copy_from_slice(&self.default_directory.directory.to_be_bytes());
                    }
                    if name_pointer.0 != 0 {
                        memory.write(name_pointer, &name)?;
                    }
                    0
                }
            }
            WorkingOp::Set { hierarchical } => {
                let name = Self::name_input(memory, name_pointer)?;
                let directory = if hierarchical {
                    u32::from_be_bytes(pb[48..52].try_into().unwrap())
                } else {
                    0
                };
                self.select_directory(&name, reference, directory, hierarchical)
                    .err()
                    .unwrap_or(0)
            }
            WorkingOp::Open => {
                let directory = u32::from_be_bytes(pb[48..52].try_into().unwrap());
                let owner = u32::from_be_bytes(pb[28..32].try_into().unwrap());
                let name = Self::name_input(memory, name_pointer)?;
                let directory = self
                    .parent(reference, directory)
                    .and_then(|parent| self.path(&name, parent));
                match directory
                    .and_then(|directory| self.open_working_directory(-1, directory, owner))
                {
                    Ok(reference) => {
                        pb[22..24].copy_from_slice(&reference.to_be_bytes());
                        0
                    }
                    Err(error) => error,
                }
            }
            WorkingOp::Close => self.close_working_directory(reference).err().unwrap_or(0),
            WorkingOp::Info => {
                let index = i16::from_be_bytes(pb[26..28].try_into().unwrap());
                let selected = if index < 0 {
                    Err(-50)
                } else if index == 0 {
                    if matches!(reference, -1..=1) {
                        Ok((-1, 2, 0))
                    } else {
                        self.working
                            .get(&reference)
                            .map(|&(d, o)| (reference, d, o))
                            .ok_or_else(|| missing(reference))
                    }
                } else {
                    self.parent(reference, 0).and_then(|_| {
                        self.working
                            .values()
                            .nth(index as usize - 1)
                            .map(|&(d, o)| (-1, d, o))
                            .ok_or(-51)
                    })
                };
                match selected {
                    Err(error) => error,
                    Ok((returned, directory, owner)) => {
                        let name = b"\x12Application Volume";
                        if name_pointer.0 != 0 {
                            memory.validate_write(name_pointer, name.len())?;
                        }
                        pb[22..24].copy_from_slice(&returned.to_be_bytes());
                        pb[28..32].copy_from_slice(&owner.to_be_bytes());
                        pb[32..34].copy_from_slice(&(-1i16).to_be_bytes());
                        pb[48..52].copy_from_slice(&directory.to_be_bytes());
                        if name_pointer.0 != 0 {
                            memory.write(name_pointer, name)?;
                        }
                        0
                    }
                }
            }
        };
        pb[16..18].copy_from_slice(&error.to_be_bytes());
        memory.write(pointer, &pb)?;
        Ok(error)
    }
}
