//! Shared FSSpec construction and deletion. IM Files, pp. 1-54 and 2-120.
//! FSpDelete: https://dev.os9.ca/techpubs/mac/Files/Files-193.html
use super::Files;
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MakeSpec {
    pub volume: i16,
    pub directory: u32,
    pub name: GuestPtr,
    pub destination: GuestPtr,
}
impl MakeSpec {
    pub fn validate(self, memory: &Memory) -> Result<()> {
        if self.destination.0 == 0 {
            return Err(Error::Cpu("nil FSSpec destination".into()));
        }
        Files::name_input(memory, self.name)?;
        memory.validate_write(self.destination, 70)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetSpecInfo {
    pub spec: GuestPtr,
    pub destination: GuestPtr,
}
impl GetSpecInfo {
    pub fn validate(self, memory: &Memory) -> Result<()> {
        if self.spec.0 == 0 || self.destination.0 == 0 {
            return Err(Error::Cpu("nil FSpGetFInfo pointer".into()));
        }
        memory.read(self.spec, 70)?;
        memory.validate_write(self.destination, 16)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetSpecInfo {
    pub spec: GuestPtr,
    pub source: GuestPtr,
}
impl SetSpecInfo {
    pub fn validate(self, memory: &Memory) -> Result<()> {
        if self.spec.0 == 0 || self.source.0 == 0 {
            return Err(Error::Cpu("nil FSpSetFInfo pointer".into()));
        }
        memory.read(self.spec, 70)?;
        memory.read(self.source, 16)?;
        Ok(())
    }
}
impl Files {
    /// IM Files, pp. 2-160–2-161: replace only the original sixteen Finder bytes.
    pub fn set_spec_info(&mut self, memory: &Memory, request: SetSpecInfo) -> Result<i16> {
        request.validate(memory)?;
        let node = match self.spec_target(memory, request.spec)? {
            Ok(node) => node,
            Err(error) => return Ok(error),
        };
        let finder: [u8; 16] = memory.read(request.source, 16)?.try_into().unwrap();
        if self.directory_exists(node) {
            return Err(Error::Cpu(
                "directory Finder information writes are unsupported".into(),
            ));
        }
        if node == 16 {
            return Ok(-45);
        }
        let file = self
            .files
            .get_mut(&node)
            .ok_or_else(|| Error::Cpu("resolved Finder info target disappeared".into()))?;
        if file.finder != finder {
            file.finder = finder;
            self.changed();
        }
        Ok(0)
    }
    pub fn get_spec_info(&self, memory: &mut Memory, request: GetSpecInfo) -> Result<i16> {
        request.validate(memory)?;
        let node = match self.spec_target(memory, request.spec)? {
            Ok(node) => node,
            Err(error) => return Ok(error),
        };
        let finder = self
            .finder_info(node)
            .ok_or_else(|| Error::Cpu("resolved Finder info target disappeared".into()))?;
        memory.write(request.destination, &finder)?;
        Ok(0)
    }
    pub fn delete_spec(&mut self, memory: &Memory, spec: GuestPtr) -> Result<i16> {
        let error = match self.spec_target(memory, spec)? {
            Ok(node) => self.delete_node(node),
            Err(error) => error,
        };
        if error == 0 {
            self.changed();
        }
        Ok(error)
    }
    pub(super) fn spec_target(
        &self,
        memory: &Memory,
        spec: GuestPtr,
    ) -> Result<std::result::Result<u32, i16>> {
        if spec.0 == 0 {
            return Err(Error::Cpu("nil catalog FSSpec".into()));
        }
        let bytes = memory.read(spec, 70)?;
        let length = usize::from(bytes[6]);
        if length > 63 {
            return Ok(Err(-50));
        }
        let name = &bytes[7..7 + length];
        if name.is_empty() || name.len() > 31 || name.contains(&0) || name.contains(&b':') {
            return Ok(Err(-37));
        }
        if self.name.is_empty() {
            return Ok(Err(-35));
        }
        let volume = i16::from_be_bytes(bytes[..2].try_into().unwrap());
        let directory = u32::from_be_bytes(bytes[2..6].try_into().unwrap());
        let node = if directory == 1 && name.eq_ignore_ascii_case(b"Application Volume") {
            self.parent(volume, 2).map(|_| 2)
        } else {
            self.parent(volume, directory)
                .and_then(|parent| self.path(name, parent))
        };
        Ok(node)
    }
    pub(super) fn delete_node(&mut self, node: u32) -> i16 {
        if node == 16 {
            return -45;
        }
        if node == 2
            || self.system_folder == Some(node)
            || node == self.default_directory()
            || self.open.values().any(|f| f.node == node)
            || self.directories.contains_key(&node)
                && (!self.children(node, false).is_empty()
                    || self
                        .working
                        .values()
                        .any(|&(directory, _)| directory == node))
        {
            return -47;
        }
        if self.files.remove(&node).is_none() {
            self.directories.remove(&node);
        }
        self.resource_growth.remove(&node);
        0
    }
    pub(super) fn spec_identity(&self, node: u32) -> (u32, Vec<u8>) {
        if node == 2 {
            (1, b"Application Volume".to_vec())
        } else if let Some(directory) = self.directories.get(&node) {
            (directory.parent, directory.name.clone())
        } else {
            let file = self.file(node).expect("resolved catalog node");
            (file.parent, file.name.to_vec())
        }
    }
    fn make_spec_parts(
        &self,
        volume: i16,
        directory: u32,
        name: &[u8],
    ) -> std::result::Result<(u32, Vec<u8>, bool), i16> {
        if self.name.is_empty() {
            return Err(-35);
        }
        let full = !name.starts_with(b":") && name.contains(&b':');
        let parent = if full {
            2
        } else {
            self.parent(volume, directory)?
        };
        if parent == 1 && name.eq_ignore_ascii_case(b"Application Volume") {
            return Ok((1, b"Application Volume".to_vec(), true));
        }
        match self.path(name, parent) {
            Ok(node) => {
                let (parent, name) = self.spec_identity(node);
                Ok((parent, name, true))
            }
            Err(-43) => {
                let split = name.iter().rposition(|&c| c == b':');
                let leaf = split.map_or(name, |at| &name[at + 1..]);
                if leaf.is_empty() || leaf.len() > 31 || leaf.contains(&0) {
                    return Err(-37);
                }
                let parent = match split {
                    Some(at) => self.path(&name[..=at], parent)?,
                    None => parent,
                };
                if !self.directory_exists(parent) {
                    return Err(-120);
                }
                Ok((parent, leaf.to_vec(), false))
            }
            Err(error) => Err(error),
        }
    }
    pub fn make_spec(&self, memory: &mut Memory, request: MakeSpec) -> Result<i16> {
        request.validate(memory)?;
        let name = Self::name_input(memory, request.name)?;
        let mut bytes = [0; 70];
        let error = match self.make_spec_parts(request.volume, request.directory, &name) {
            Ok((parent, name, exists)) => {
                bytes[..2].copy_from_slice(&(-1i16).to_be_bytes());
                bytes[2..6].copy_from_slice(&parent.to_be_bytes());
                bytes[6] = name.len() as u8;
                bytes[7..7 + name.len()].copy_from_slice(&name);
                if exists {
                    0
                } else {
                    -43
                }
            }
            Err(error) => error,
        };
        memory.write(request.destination, &bytes)?;
        Ok(error)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{files::Directory, memory::Permissions};
    #[test]
    fn finder_info_reads_exact_catalog_bytes_without_mutation_and_preserves_failed_outputs() {
        let mut files = Files::application(&[], None);
        let finder = *b"TYPECREA01234567";
        files.create_file(b"Test", -1, 2, 0, finder).unwrap();
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0x5a; 256], Permissions::RW)
            .unwrap();
        let request = GetSpecInfo {
            spec: GuestPtr(0x1000),
            destination: GuestPtr(0x1081),
        };
        let mut spec = [0; 70];
        spec[..2].copy_from_slice(&(-1i16).to_be_bytes());
        spec[2..6].copy_from_slice(&2u32.to_be_bytes());
        spec[6] = 4;
        spec[7..11].copy_from_slice(b"tEsT");
        memory.write(request.spec, &spec).unwrap();
        let revision = files.revision;
        assert_eq!(files.get_spec_info(&mut memory, request).unwrap(), 0);
        assert_eq!(memory.read(request.destination, 16).unwrap(), finder);
        assert_eq!(memory.read(GuestPtr(0x1080), 1).unwrap(), [0x5a]);
        assert_eq!(memory.read(GuestPtr(0x1091), 1).unwrap(), [0x5a]);
        assert_eq!(files.revision, revision);
        for (volume, parent, name, error) in [
            (-1i16, 2u32, b"Gone".as_slice(), -43),
            (7, 2, b"Test".as_slice(), -35),
            (-1, 999, b"Test".as_slice(), -120),
            (-1, 2, b"Bad:".as_slice(), -37),
        ] {
            spec[..2].copy_from_slice(&volume.to_be_bytes());
            spec[2..6].copy_from_slice(&parent.to_be_bytes());
            spec[6] = name.len() as u8;
            spec[7..7 + name.len()].copy_from_slice(name);
            memory.write(request.spec, &spec).unwrap();
            memory.write(request.destination, &[0x5a; 16]).unwrap();
            assert_eq!(files.get_spec_info(&mut memory, request).unwrap(), error);
            assert_eq!(memory.read(request.destination, 16).unwrap(), [0x5a; 16]);
            assert_eq!(files.revision, revision);
        }
        assert!(files
            .get_spec_info(
                &mut memory,
                GetSpecInfo {
                    spec: GuestPtr(0),
                    ..request
                }
            )
            .is_err());
        assert!(files
            .get_spec_info(
                &mut memory,
                GetSpecInfo {
                    destination: GuestPtr(0x10f1),
                    ..request
                }
            )
            .is_err());
        spec[..2].copy_from_slice(&(-1i16).to_be_bytes());
        spec[2..6].copy_from_slice(&1u32.to_be_bytes());
        spec[6] = 18;
        spec[7..25].copy_from_slice(b"Application Volume");
        memory.write(request.spec, &spec).unwrap();
        assert_eq!(files.get_spec_info(&mut memory, request).unwrap(), 0);
        assert_eq!(memory.read(request.destination, 16).unwrap(), [0; 16]);
    }
    #[test]
    fn deletion_removes_both_forks_and_reservations_but_preserves_busy_objects() {
        let mut files = Files::application(&[], None);
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0; 70], Permissions::RW)
            .unwrap();
        let spec = GuestPtr(0x1000);
        let mut bytes = [0; 70];
        bytes[..2].copy_from_slice(&(-1i16).to_be_bytes());
        bytes[2..6].copy_from_slice(&2u32.to_be_bytes());
        bytes[6] = 4;
        bytes[7..11].copy_from_slice(b"Test");
        memory.write(spec, &bytes).unwrap();
        files.create_file(b"Test", -1, 2, 0, [0; 16]).unwrap();
        let node = files.path(b"Test", 2).unwrap();
        files.files.get_mut(&node).unwrap().data = vec![1, 2];
        files.files.get_mut(&node).unwrap().resource = vec![3, 4];
        files.resource_growth.insert(node, 100);
        let reference = files
            .open_access(node, super::super::Fork::Resource, 3)
            .unwrap();
        assert_eq!(files.delete_spec(&memory, spec).unwrap(), -47);
        assert_eq!(files.files[&node].data, [1, 2]);
        files.open.remove(&reference);
        assert_eq!(files.delete_spec(&memory, spec).unwrap(), 0);
        assert!(!files.files.contains_key(&node));
        assert!(!files.resource_growth.contains_key(&node));
        assert_eq!(files.delete_spec(&memory, spec).unwrap(), -43);
        assert_eq!(memory.read(spec, 70).unwrap(), bytes);
        files.directories.insert(
            node,
            Directory {
                name: b"Test".to_vec(),
                parent: 2,
                created: 0,
            },
        );
        let wd = files.open_working_directory(-1, node, 42).unwrap();
        assert_eq!(files.delete_spec(&memory, spec).unwrap(), -47);
        files.close_working_directory(wd).unwrap();
        files.create_file(b"Child", -1, node, 0, [0; 16]).unwrap();
        assert_eq!(files.delete_spec(&memory, spec).unwrap(), -47);
        let child = files.path(b"Child", node).unwrap();
        assert_eq!(files.delete_node(child), 0);
        assert_eq!(files.delete_spec(&memory, spec).unwrap(), 0);
        assert!(files.delete_spec(&memory, GuestPtr(0)).is_err());
        assert!(files.delete_spec(&memory, GuestPtr(0x1040)).is_err());
    }
    #[test]
    fn canonical_paths_directory_identity_missing_leaf_hints_and_working_references_share_catalog()
    {
        let mut files = Files::application(&[], None);
        files.directories.insert(
            17,
            Directory {
                name: b"Pilots".to_vec(),
                parent: 2,
                created: 0,
            },
        );
        files.directories.insert(
            18,
            Directory {
                name: b"Nested".to_vec(),
                parent: 17,
                created: 0,
            },
        );
        files.next_node = 19;
        files.create_file(b"Pilot", -1, 17, 0, [0; 16]).unwrap();
        let wd = files.open_working_directory(-1, 17, 42).unwrap();
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
            .unwrap();
        for (volume, directory, name, error, parent, leaf) in [
            (
                -1,
                2,
                b":pilots:pilot".as_slice(),
                0,
                17,
                b"Pilot".as_slice(),
            ),
            (
                7,
                999,
                b"Application Volume:Pilots:Pilot".as_slice(),
                0,
                17,
                b"Pilot".as_slice(),
            ),
            (-1, 18, b"::Pilot".as_slice(), 0, 17, b"Pilot".as_slice()),
            (wd, 0, b"Pilot".as_slice(), 0, 17, b"Pilot".as_slice()),
            (
                wd,
                2,
                b"Application".as_slice(),
                0,
                2,
                b"Application".as_slice(),
            ),
            (-1, 17, b"".as_slice(), 0, 2, b"Pilots".as_slice()),
            (
                -1,
                2,
                b"".as_slice(),
                0,
                1,
                b"Application Volume".as_slice(),
            ),
            (
                -1,
                2,
                b":Pilots:Nested:".as_slice(),
                0,
                17,
                b"Nested".as_slice(),
            ),
            (
                -1,
                2,
                b":Pilots:New Pilot".as_slice(),
                -43,
                17,
                b"New Pilot".as_slice(),
            ),
            (-1, 2, b":Missing:New".as_slice(), -120, 0, b"".as_slice()),
            (7, 2, b"Pilot".as_slice(), -35, 0, b"".as_slice()),
            (
                -1,
                2,
                b"Other Volume:Pilot".as_slice(),
                -35,
                0,
                b"".as_slice(),
            ),
            (
                -1,
                2,
                b"abcdefghijklmnopqrstuvwxyz123456".as_slice(),
                -37,
                0,
                b"".as_slice(),
            ),
        ] {
            let mut input = vec![name.len() as u8];
            input.extend(name);
            memory.write(GuestPtr(0x1000), &input).unwrap();
            memory.write(GuestPtr(0x1100), &[0x5a; 72]).unwrap();
            let request = MakeSpec {
                volume,
                directory,
                name: GuestPtr(0x1000),
                destination: GuestPtr(0x1101),
            };
            assert_eq!(files.make_spec(&mut memory, request).unwrap(), error);
            let bytes = memory.read(request.destination, 70).unwrap();
            if error == 0 || error == -43 {
                assert_eq!(i16::from_be_bytes(bytes[..2].try_into().unwrap()), -1);
                assert_eq!(u32::from_be_bytes(bytes[2..6].try_into().unwrap()), parent);
                assert_eq!(bytes[6] as usize, leaf.len());
                assert_eq!(&bytes[7..7 + leaf.len()], leaf);
            } else {
                assert_eq!(bytes, [0; 70]);
            }
            assert_eq!(memory.read(GuestPtr(0x1100), 1).unwrap(), [0x5a]);
            assert_eq!(memory.read(GuestPtr(0x1147), 1).unwrap(), [0x5a]);
        }
        let (parent, name, exists) = files.make_spec_parts(-1, 2, b":Pilots:New Pilot").unwrap();
        assert!(!exists);
        files.create_file(&name, -1, parent, 0, [0; 16]).unwrap();
        assert!(
            files
                .make_spec_parts(-1, 2, b":Pilots:New Pilot")
                .unwrap()
                .2
        );
    }
}
