//! ResolveAliasFile: IM Macintosh Toolbox Essentials, Finder Interface, p. 7-49.
use super::Files;
mod catalog;
mod record;
use crate::process::Process;
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
use record::Record;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolveAliasFile {
    pub spec: GuestPtr,
    pub chains: bool,
    pub folder: GuestPtr,
    pub aliased: GuestPtr,
}
impl ResolveAliasFile {
    pub fn validate(self, memory: &Memory) -> Result<()> {
        if self.spec.0 == 0 || self.folder.0 == 0 || self.aliased.0 == 0 {
            return Err(Error::Cpu("nil ResolveAliasFile parameter".into()));
        }
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
        let mut current = node;
        let mut seen = std::collections::BTreeSet::new();
        let mut followed = false;
        while let Some(file) = self.file(current) {
            if u16::from_be_bytes(file.finder[8..10].try_into().unwrap()) & 0x8000 == 0 {
                break;
            }
            if followed && !request.chains {
                break;
            }
            if seen.len() >= 64 || !seen.insert(current) {
                return Ok(-50);
            }
            if self
                .open
                .values()
                .any(|v| v.node == current && v.resource_map)
            {
                return Err(Error::Cpu(
                    "alias resolution with an open resource map is unsupported".into(),
                ));
            }
            let bytes = match file.resource {
                Some(v) if !v.is_empty() => v,
                _ => return Ok(-192),
            };
            let fork = match crate::load::resource::ResourceFork::parse(bytes) {
                Ok(v) => v,
                Err(_) => return Ok(-50),
            };
            let value = match fork.get(*b"alis", 0) {
                Some(v) => v,
                None => return Ok(-192),
            };
            let record = match Record::parse(&value.data) {
                Ok(v) => v,
                Err(e) => return Ok(e),
            };
            current = match self.alias_target_node(&record, Some(current)) {
                Ok(v) => v,
                Err(e) => {
                    if e == -43 {
                        match self.alias_missing_spec(&record) {
                            Ok(spec) => memory.write(request.spec, &spec)?,
                            Err(e) => return Ok(e),
                        }
                    }
                    return Ok(e);
                }
            };
            followed = true;
        }
        // All catalog, resource and chain validation precedes publication.
        if followed {
            memory.write(request.spec, &self.alias_spec(current))?;
        }
        memory.write(request.folder, &[u8::from(self.directory_exists(current))])?;
        memory.write(request.aliased, &[u8::from(followed)])?;
        Ok(0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{files::Directory, memory::Permissions};
    #[test]
    fn ordinary_targets_and_missing_alias_resources_preserve_outputs_on_error() {
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
        assert_eq!(
            files.resolve_alias_file(&mut memory, request).unwrap(),
            -192
        );
        assert_eq!(memory.read(GuestPtr(0x1000), 80).unwrap(), before);
        files.files.remove(&regular);
        spec[6] = 5;
        spec[7..12].copy_from_slice(b"Pilot");
        memory.write(request.spec, &spec).unwrap();
        assert_eq!(files.resolve_alias_file(&mut memory, request).unwrap(), -43);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewAlias {
    pub from: GuestPtr,
    pub target: GuestPtr,
    pub destination: GuestPtr,
}
impl NewAlias {
    pub fn validate(self, m: &Memory) -> Result<()> {
        if self.target.0 == 0 || self.destination.0 == 0 {
            return Err(Error::Cpu("nil NewAlias target/output".into()));
        }
        if self.from.0 != 0 {
            m.read(self.from, 70)?;
        }
        m.read(self.target, 70)?;
        m.validate_write(self.destination, 4)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolveAlias {
    pub from: GuestPtr,
    pub alias: u32,
    pub target: GuestPtr,
    pub changed: GuestPtr,
}
impl ResolveAlias {
    pub fn validate(self, m: &Memory) -> Result<()> {
        if self.changed.0 == 0 {
            return Err(Error::Cpu("nil ResolveAlias parameter".into()));
        }
        if self.from.0 != 0 {
            m.read(self.from, 70)?;
        }
        if self.target.0 != 0 {
            m.validate_write(self.target, 70)?;
        }
        m.validate_write(self.changed, 1)
    }
}
pub(crate) fn new_alias(m: &mut Memory, p: &mut Process, request: NewAlias) -> Result<i16> {
    request.validate(m)?;
    // Freeze catalog/FSSpec data before clearing an output that may alias it.
    let bytes = match p.files.alias_record(m, request.from, request.target)? {
        Ok(record) => match record.encode() {
            Ok(b) => b,
            Err(e) => {
                m.write(request.destination, &0u32.to_be_bytes())?;
                return Ok(e);
            }
        },
        Err(e) => {
            m.write(request.destination, &0u32.to_be_bytes())?;
            return Ok(e);
        }
    };
    let h = p.new_handle(m, bytes.len() as u32, false)?;
    if h == 0 {
        m.write(request.destination, &0u32.to_be_bytes())?;
        return Ok(-108);
    }
    let result = (|| {
        let data = GuestPtr(m.u32(GuestPtr(h))?);
        m.write(data, &bytes)?;
        m.write(request.destination, &h.to_be_bytes())
    })();
    if result.is_err() {
        p.dispose_handle(m, h)?;
    }
    result?;
    Ok(0)
}
pub(crate) fn resolve_alias(m: &mut Memory, p: &mut Process, request: ResolveAlias) -> Result<i16> {
    request.validate(m)?;
    if request.alias == 0 || request.target.0 == 0 {
        return Ok(-50);
    }
    let n = p.handle_size(m, request.alias)?;
    if p.mem_error != 0 {
        return Ok(-50);
    }
    let bytes = m
        .read(GuestPtr(m.u32(GuestPtr(request.alias))?), n as usize)?
        .to_vec();
    let record = match Record::parse(&bytes) {
        Ok(r) => r,
        Err(e) => return Ok(e),
    };
    let node = match p.files.alias_target(m, &record, request.from)? {
        Ok(n) => n,
        Err(e) => {
            if e == -43 {
                match p.files.alias_missing_spec(&record) {
                    Ok(spec) => {
                        m.write(request.target, &spec)?;
                        m.write(request.changed, &[0])?;
                    }
                    Err(e) => return Ok(e),
                }
            }
            return Ok(e);
        }
    };
    // Minimal records must never be rewritten by ResolveAlias.
    if record.path.is_none() || record.long(114) == 0 {
        m.write(request.target, &p.files.alias_spec(node))?;
        m.write(request.changed, &[0])?;
        return Ok(0);
    }
    let mut updated = match p.files.alias_record_for_node(m, request.from, node)? {
        Ok(r) => r,
        Err(e) => return Ok(e),
    };
    updated.preserve_private(&record);
    let updated = match updated.encode() {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    let changed = updated != bytes[..usize::from(record.word(4))];
    let spec = p.files.alias_spec(node);
    if changed {
        let mut replacement = updated;
        replacement.extend(&bytes[usize::from(record.word(4))..]);
        p.replace_handles(m, vec![(request.alias, replacement)])?;
        if p.mem_error != 0 {
            return Ok(p.mem_error);
        }
    }
    m.write(request.target, &spec)?;
    m.write(request.changed, &[u8::from(changed)])?;
    Ok(0)
}
