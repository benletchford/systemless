//! ResolveAliasFile: IM Macintosh Toolbox Essentials, Finder Interface, p. 7-49.
use super::Files;
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolveAliasFile {
    pub spec: GuestPtr,
    pub chains: bool,
    pub folder: GuestPtr,
    pub aliased: GuestPtr,
}
impl ResolveAliasFile {
    pub fn validate(self, memory: &Memory) -> Result<()> {
        memory.read(self.spec, 70)?;
        memory.validate_write(self.spec, 70)?;
        memory.validate_write(self.folder, 1)?;
        memory.validate_write(self.aliased, 1)
    }
}
impl Files {
    pub fn resolve_alias_file(
        &self,
        memory: &mut Memory,
        request: ResolveAliasFile,
    ) -> Result<i16> {
        request.validate(memory)?;
        let spec = memory.read(request.spec, 70)?;
        let volume = i16::from_be_bytes(spec[..2].try_into().unwrap());
        let directory = u32::from_be_bytes(spec[2..6].try_into().unwrap());
        let length = usize::from(spec[6]);
        if length > 63 {
            return Ok(-50);
        }
        let name = &spec[7..7 + length];
        let selected = self.parent(volume, directory).and_then(|parent| {
            if self.name.is_empty() {
                return Err(-35);
            }
            if parent == 1 && name.eq_ignore_ascii_case(b"Application Volume") {
                Ok(2)
            } else {
                self.path(name, parent)
            }
        });
        let node = match selected {
            Ok(node) => node,
            Err(error) => return Ok(error),
        };
        let folder = self.directory_exists(node);
        if let Some(file) = self.file(node) {
            if u16::from_be_bytes(file.finder[8..10].try_into().unwrap()) & 0x8000 != 0 {
                return Err(Error::Cpu(
                    "Finder alias record resolution is not implemented".into(),
                ));
            }
        }
        // Snapshot and validate before either Boolean write; error returns preserve
        // inputs and outputs. Ordinary targets retain their original FSSpec bytes.
        memory.write(request.folder, &[u8::from(folder)])?;
        memory.write(request.aliased, &[0])?;
        Ok(0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{files::Directory, memory::Permissions};
    #[test]
    fn ordinary_nested_targets_use_live_catalog_and_real_aliases_fail_explicitly() {
        let mut files = Files::application(&[], None);
        files.directories.insert(
            17,
            Directory {
                name: b"Pilots".to_vec(),
                parent: 2,
                created: 0,
            },
        );
        files.next_node = 18;
        let regular = files.create_file(b"Pilot", -1, 17, 0, [0; 16]).unwrap();
        let mut finder = [0; 16];
        finder[8] = 0x80;
        files.create_file(b"Alias", -1, 17, 0, finder).unwrap();
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0x5a; 80], Permissions::RW)
            .unwrap();
        let request = ResolveAliasFile {
            spec: GuestPtr(0x1000),
            chains: true,
            folder: GuestPtr(0x1048),
            aliased: GuestPtr(0x104a),
        };
        for (parent, name, folder) in [
            (2, b"Pilots".as_slice(), true),
            (17, b"Pilot".as_slice(), false),
        ] {
            let mut spec = [0; 70];
            spec[..2].copy_from_slice(&(-1i16).to_be_bytes());
            spec[2..6].copy_from_slice(&u32::to_be_bytes(parent));
            spec[6] = name.len() as u8;
            spec[7..7 + name.len()].copy_from_slice(name);
            memory.write(request.spec, &spec).unwrap();
            assert_eq!(files.resolve_alias_file(&mut memory, request).unwrap(), 0);
            assert_eq!(memory.read(request.folder, 1).unwrap(), [u8::from(folder)]);
            assert_eq!(memory.read(request.spec, 70).unwrap(), spec);
        }
        let mut spec = [0; 70];
        spec[..2].copy_from_slice(&(-1i16).to_be_bytes());
        spec[2..6].copy_from_slice(&17u32.to_be_bytes());
        spec[6] = 5;
        spec[7..12].copy_from_slice(b"Alias");
        memory.write(request.spec, &spec).unwrap();
        let before = memory.read(GuestPtr(0x1000), 80).unwrap().to_vec();
        assert!(files.resolve_alias_file(&mut memory, request).is_err());
        assert_eq!(memory.read(GuestPtr(0x1000), 80).unwrap(), before);
        files.files.remove(&regular);
        spec[6] = 5;
        spec[7..12].copy_from_slice(b"Pilot");
        memory.write(request.spec, &spec).unwrap();
        assert_eq!(files.resolve_alias_file(&mut memory, request).unwrap(), -43);
    }
}
