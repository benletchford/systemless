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
