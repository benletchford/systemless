//! Process-local, read-only virtual application file with independent forks and
//! open-file marks. Host paths never enter File Manager service implementations.
use crate::{
    memory::{GuestPtr, Memory},
    Result,
};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fork {
    Data,
    Resource,
}
struct OpenFork {
    kind: Fork,
    mark: u32,
}
#[derive(Default)]
pub struct Files {
    name: Vec<u8>,
    data: Vec<u8>,
    resource: Option<Vec<u8>>,
    open: BTreeMap<i16, OpenFork>,
    next_ref: i16,
}
impl Files {
    pub(crate) fn application(data: &[u8], resource: Option<&[u8]>) -> Self {
        Self {
            name: b"Application".to_vec(),
            data: data.to_vec(),
            resource: resource.map(Vec::from),
            open: BTreeMap::new(),
            next_ref: 1,
        }
    }
    /// A classic HFS Str31, passed as raw Macintosh bytes by the host boundary.
    pub fn set_name(&mut self, name: &[u8]) -> std::result::Result<(), i16> {
        if self.name.is_empty() {
            return Err(-43);
        }
        if name.is_empty() || name.len() > 31 || name.contains(&b':') || name.contains(&0) {
            return Err(-37);
        }
        self.name = name.to_vec();
        Ok(())
    }
    pub fn fork_bytes(&self, kind: Fork) -> Option<&[u8]> {
        if self.name.is_empty() {
            return None;
        }
        match kind {
            Fork::Data => Some(&self.data),
            Fork::Resource => self.resource.as_deref(),
        }
    }
    pub fn open(&mut self, kind: Fork) -> std::result::Result<i16, i16> {
        if self.fork_bytes(kind).is_none() {
            return Err(-43);
        }
        let reference = self.next_ref;
        self.next_ref = self.next_ref.checked_add(1).ok_or(-42i16)?;
        self.open.insert(reference, OpenFork { kind, mark: 0 });
        Ok(reference)
    }
    /// Read-only catalog for the loaded application on virtual volume -1.
    /// Root ID 2, parent ID 1, application ID 16 match the FCB projection.
    pub fn catalog_info(&self, memory: &mut Memory, pointer: GuestPtr) -> Result<i16> {
        let header = memory.read(pointer, 52)?.to_vec();
        let volume = i16::from_be_bytes(header[22..24].try_into().unwrap());
        let index = i16::from_be_bytes(header[28..30].try_into().unwrap());
        let directory = u32::from_be_bytes(header[48..52].try_into().unwrap());
        let name_ptr = GuestPtr(u32::from_be_bytes(header[18..22].try_into().unwrap()));
        let error: i16 = if !matches!(volume, -1..=1) {
            -35
        } else if index < 0 && !matches!(directory, 0 | 2) {
            -120
        } else if self.name.is_empty() {
            -43
        } else {
            0
        };
        let mut error = error;
        let mut root = index < 0;
        if error == 0 && index == 0 {
            let name = if name_ptr.0 == 0 {
                &[][..]
            } else {
                let length = usize::from(memory.read(name_ptr, 1)?[0]);
                memory.read(
                    GuestPtr(
                        name_ptr
                            .0
                            .checked_add(1)
                            .ok_or_else(|| crate::Error::Cpu("catalog name overflow".into()))?,
                    ),
                    length,
                )?
            };
            root = name.is_empty()
                || name == b":"
                || name.eq_ignore_ascii_case(b"Application Volume:");
            let leaf = name.strip_prefix(b":").unwrap_or(name);
            if !root && !leaf.eq_ignore_ascii_case(&self.name) {
                error = -43;
            }
        } else if error == 0 && index > 1 {
            error = -43;
        }
        memory.validate_write(GuestPtr(pointer.0 + 12), 6)?;
        if error != 0 {
            memory.write(GuestPtr(pointer.0 + 12), &[0; 4])?;
            memory.write(GuestPtr(pointer.0 + 16), &error.to_be_bytes())?;
            return Ok(error);
        }
        let size = if root { 104 } else { 108 };
        let mut pb = memory.read(pointer, size)?.to_vec();
        memory.validate_write(pointer, size)?;
        let name = if root {
            b"Application Volume".as_slice()
        } else {
            &self.name
        };
        let mut output_name = vec![name.len() as u8];
        output_name.extend(name);
        if name_ptr.0 != 0 {
            memory.validate_write(name_ptr, output_name.len())?;
        }
        pb[12..18].fill(0);
        pb[22..24].copy_from_slice(&(-1i16).to_be_bytes());
        let reference = if root {
            0
        } else {
            self.open.keys().next().copied().unwrap_or(0)
        };
        pb[24..26].copy_from_slice(&reference.to_be_bytes());
        pb[30] = if root {
            0x10
        } else {
            self.open.values().fold(1, |flags, fork| {
                flags
                    | 0x80
                    | match fork.kind {
                        Fork::Resource => 4,
                        Fork::Data => 8,
                    }
            })
        };
        pb[31] = 0;
        pb[32..48].fill(0); // Finder info: virtual files have no imported Finder metadata.
        pb[48..52].copy_from_slice(&(if root { 2u32 } else { 16 }).to_be_bytes());
        if root {
            pb[52..54].copy_from_slice(&1u16.to_be_bytes());
        } else {
            let data = self.data.len() as u32;
            let resource = self.resource.as_ref().map_or(0, |bytes| bytes.len() as u32);
            pb[52..54].copy_from_slice(&(if data == 0 { 0u16 } else { 1 }).to_be_bytes());
            pb[62..64].copy_from_slice(
                &(if resource == 0 {
                    0u16
                } else {
                    1 + data.div_ceil(4096) as u16
                })
                .to_be_bytes(),
            );
            for (at, value) in [
                (54, data),
                (58, data.div_ceil(4096) * 4096),
                (64, resource),
                (68, resource.div_ceil(4096) * 4096),
                (104, 4096),
            ] {
                pb[at..at + 4].copy_from_slice(&value.to_be_bytes());
            }
        }
        pb[72..100].fill(0); // deterministic dates and extended Finder info.
        pb[100..104].copy_from_slice(&(if root { 1u32 } else { 2 }).to_be_bytes());
        // Every output validated before either guest destination changes.
        if name_ptr.0 != 0 {
            memory.write(name_ptr, &output_name)?;
        }
        memory.write(pointer, &pb)?;
        Ok(0)
    }
    /// PBGetFCBInfo synchronous contract, IM: Files (1992), pp. 2-237–2-238.
    pub fn fcb_info(&self, memory: &mut Memory, pointer: GuestPtr) -> Result<i16> {
        let mut pb: [u8; 62] = memory.read(pointer, 62)?.try_into().unwrap();
        memory.validate_write(pointer, 62)?;
        let word = |at| i16::from_be_bytes([pb[at], pb[at + 1]]);
        let index = word(28);
        let volume = word(22);
        let reference = word(24);
        let selected = if index == 0 {
            self.open.get_key_value(&reference)
        } else if index > 0 && matches!(volume, -1..=1) {
            self.open.iter().nth(index as usize - 1)
        } else {
            None
        };
        let error = if index < 0 {
            -50
        } else if index > 0 && !matches!(volume, -1..=1) {
            -35
        } else if selected.is_none() {
            -51
        } else {
            0
        };
        pb[16..18].copy_from_slice(&(error as i16).to_be_bytes());
        if let Some((&reference, file)) = selected.filter(|_| error == 0) {
            let length = self.fork_bytes(file.kind).unwrap().len() as u32;
            let physical = length.div_ceil(4096) * 4096;
            let name_ptr = u32::from_be_bytes(pb[18..22].try_into().unwrap());
            let mut name = vec![self.name.len() as u8];
            name.extend(&self.name);
            if name_ptr != 0 {
                memory.validate_write(GuestPtr(name_ptr), name.len())?;
            }
            pb[24..26].copy_from_slice(&reference.to_be_bytes());
            pb[32..36].copy_from_slice(&16u32.to_be_bytes()); // application catalog node
            let flags = 0x2000u16
                | if file.kind == Fork::Resource {
                    0x200
                } else {
                    0
                };
            pb[36..38].copy_from_slice(&flags.to_be_bytes());
            let block = if length == 0 {
                0u16
            } else if file.kind == Fork::Data {
                1
            } else {
                1 + (self.data.len() as u32).div_ceil(4096) as u16
            };
            pb[38..40].copy_from_slice(&block.to_be_bytes());
            for (at, value) in [
                (40, length),
                (44, physical),
                (48, file.mark),
                (54, 4096),
                (58, 2),
            ] {
                pb[at..at + 4].copy_from_slice(&value.to_be_bytes());
            }
            pb[52..54].copy_from_slice(&(-1i16).to_be_bytes());
            // Validate both destinations before writing either one.
            if name_ptr != 0 {
                memory.write(GuestPtr(name_ptr), &name)?;
            }
        }
        memory.write(pointer, &pb)?;
        Ok(error as i16)
    }
}
