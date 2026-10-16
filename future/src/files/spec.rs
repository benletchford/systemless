//! FSMakeFSSpec: IM Files, Introduction to File Management, p. 1-54.
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
impl Files {
    fn spec_identity(&self, node: u32) -> (u32, Vec<u8>) {
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
