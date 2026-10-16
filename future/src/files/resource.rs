//! Resource files use the same catalog nodes, forks and access paths as raw I/O.
use super::{File, Files};
use crate::{
    load::resource::ResourceFork,
    memory::{GuestPtr, Memory},
    Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateSpecResource {
    pub spec: GuestPtr,
    pub creator: [u8; 4],
    pub kind: [u8; 4],
    pub script: i16,
}
impl CreateSpecResource {
    pub fn validate(self, memory: &Memory) -> Result<()> {
        if self.spec.0 == 0 {
            return Err(crate::Error::Cpu("nil resource FSSpec".into()));
        }
        memory.read(self.spec, 70)?;
        Ok(())
    }
}
impl Files {
    pub(super) fn space_used(&self) -> usize {
        self.files
            .values()
            .map(|f| f.data.len() + f.resource.len())
            .sum::<usize>()
            + self
                .resource_growth
                .iter()
                .map(|(node, size)| {
                    size.saturating_sub(
                        self.node_fork(*node, super::Fork::Resource)
                            .map_or(0, |b| b.len()),
                    )
                })
                .sum::<usize>()
    }
    pub(crate) fn reserve_resource_growth(
        &mut self,
        node: u32,
        size: usize,
    ) -> std::result::Result<(), i16> {
        let old = self
            .node_fork(node, super::Fork::Resource)
            .ok_or(-43i16)?
            .len();
        let previous = self
            .resource_growth
            .get(&node)
            .copied()
            .unwrap_or(old)
            .saturating_sub(old);
        let projected = self.space_used() - previous;
        if projected
            .checked_add(size.saturating_sub(old))
            .is_none_or(|n| n > super::io::LIMIT)
        {
            return Err(-34);
        }
        self.resource_growth.insert(node, size);
        Ok(())
    }
    pub(crate) fn replace_resource_file(
        &mut self,
        reference: i16,
        bytes: Vec<u8>,
        modified: u32,
    ) -> Result<i16> {
        self.replace_resource_file_reserved(reference, bytes, modified, 0)
    }
    /// Retain space promised to other live resource-map changes when a single
    /// payload is written before the map is committed.
    pub(crate) fn replace_resource_file_reserved(
        &mut self,
        reference: i16,
        bytes: Vec<u8>,
        modified: u32,
        pending_size: usize,
    ) -> Result<i16> {
        self.validate_resource_access(reference)?;
        let access = self.open[&reference];
        if !access.writable {
            return Ok(-54);
        }
        let Some(file) = self.files.get(&access.node) else {
            return Ok(-45);
        };
        let old = file.resource.len();
        let reserved = self
            .resource_growth
            .get(&access.node)
            .copied()
            .unwrap_or(old)
            .saturating_sub(old);
        let base = self.space_used() - old - reserved;
        if base
            .checked_add(bytes.len().max(pending_size))
            .is_none_or(|n| n > super::io::LIMIT)
        {
            return Ok(-34);
        }
        let file = self.files.get_mut(&access.node).unwrap();
        file.resource = bytes;
        file.modified = modified;
        self.resource_growth.remove(&access.node);
        if pending_size > file.resource.len() {
            self.resource_growth.insert(access.node, pending_size);
        }
        self.changed();
        Ok(0)
    }
    /// Create a real empty resource map, retaining existing data and catalog identity.
    /// Explicit scripts retain the seven-bit FXInfo script code; -1 means system.
    pub fn create_spec_resource(
        &mut self,
        memory: &Memory,
        request: CreateSpecResource,
    ) -> Result<i16> {
        request.validate(memory)?;
        if !(-1..=127).contains(&request.script) {
            return Ok(-50);
        }
        let spec = memory.read(request.spec, 70)?;
        let length = usize::from(spec[6]);
        if length > 63 {
            return Ok(-50);
        }
        let name = &spec[7..7 + length];
        if name.is_empty() || name.len() > 31 || name.contains(&0) || name.contains(&b':') {
            return Ok(-37);
        }
        let volume = i16::from_be_bytes(spec[..2].try_into().unwrap());
        let directory = u32::from_be_bytes(spec[2..6].try_into().unwrap());
        let parent = match self.parent(volume, directory) {
            Ok(p) => p,
            Err(e) => return Ok(e),
        };
        if self.name.is_empty() {
            return Ok(-35);
        }
        if !self.directory_exists(parent) {
            return Ok(-120);
        }
        let target = self.child(parent, name);
        if let Some(node) = target {
            let Some(file) = self.file(node) else {
                return Ok(-48);
            };
            if file.locked {
                return Ok(-45);
            }
            if file.resource.is_some_and(|r| !r.is_empty()) {
                return Ok(-48);
            }
        }
        let bytes = ResourceFork::default().encode()?;
        let used = self.space_used();
        if used > super::io::LIMIT - bytes.len() {
            return Ok(-34);
        }
        let mut finder = [0; 16];
        finder[..4].copy_from_slice(&request.kind);
        finder[4..8].copy_from_slice(&request.creator);
        let node = match target {
            Some(node) => node,
            None => {
                match self.create_file(name, volume, parent, memory.u32(GuestPtr(0x20c))?, finder) {
                    Ok(n) => n,
                    Err(e) => return Ok(e),
                }
            }
        };
        let file = self.files.get_mut(&node).unwrap();
        file.finder[..8].copy_from_slice(&finder[..8]);
        // IM Toolbox Essentials, FXInfo: clear high bit selects system script;
        // set high bit selects the explicit script in the remaining seven bits.
        file.extended_finder[8] = if request.script == -1 {
            0
        } else {
            0x80 | request.script as u8
        };
        file.resource = bytes;
        self.changed();
        Ok(0)
    }
    pub(crate) fn resource_spec_target(
        &self,
        memory: &Memory,
        spec: GuestPtr,
    ) -> Result<std::result::Result<u32, i16>> {
        if spec.0 == 0 {
            return Err(crate::Error::Cpu("nil resource FSSpec".into()));
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
        let volume = i16::from_be_bytes(bytes[..2].try_into().unwrap());
        let directory = u32::from_be_bytes(bytes[2..6].try_into().unwrap());
        Ok(self
            .parent(volume, directory)
            .and_then(|parent| self.path(name, parent))
            .and_then(|node| {
                self.node_fork(node, super::Fork::Resource)
                    .map(|_| node)
                    .ok_or(-43)
            }))
    }
    pub(crate) fn protect_resource_access(&mut self, reference: i16) {
        self.open.get_mut(&reference).unwrap().resource_map = true;
    }
    pub(crate) fn resource_target(
        &self,
        memory: &Memory,
        name: GuestPtr,
        volume: i16,
    ) -> Result<std::result::Result<u32, i16>> {
        let name = Self::name_input(memory, name)?;
        if name.is_empty() {
            return Ok(Err(-37));
        }
        Ok(self
            .classic_parent(volume)
            .and_then(|p| self.path(&name, p))
            .and_then(|node| {
                self.node_fork(node, super::Fork::Resource)
                    .map(|_| node)
                    .ok_or(-43)
            }))
    }
    pub(crate) fn resource_bytes(&self, node: u32) -> Option<&[u8]> {
        self.node_fork(node, super::Fork::Resource)
    }
    pub(crate) fn open_resource_access(
        &mut self,
        node: u32,
        permission: u8,
    ) -> std::result::Result<(i16, bool), i16> {
        let reference = self.open_access(node, super::Fork::Resource, permission)?;
        let entry = self.open.get_mut(&reference).unwrap();
        entry.resource_map = true;
        Ok((reference, entry.writable))
    }
    pub(crate) fn validate_resource_access(&self, reference: i16) -> Result<()> {
        if !self.open.get(&reference).is_some_and(|f| f.resource_map) {
            return Err(crate::Error::Load(
                "resource map lost its file access path".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn close_resource_access(&mut self, reference: i16) -> Result<()> {
        if self.open.remove(&reference).is_none() {
            return Err(crate::Error::Load(
                "resource map lost its file access path".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn create_file(
        &mut self,
        name: &[u8],
        volume: i16,
        parent: u32,
        created: u32,
        finder: [u8; 16],
    ) -> std::result::Result<u32, i16> {
        let split = name.iter().rposition(|&c| c == b':');
        let leaf = split.map_or(name, |i| &name[i + 1..]);
        let parent = self.parent(volume, parent)?;
        if self.name.is_empty() {
            return Err(-43);
        }
        if leaf.is_empty() || leaf.len() > 31 || leaf.contains(&0) {
            return Err(-37);
        }
        let parent = match split {
            Some(0) => parent,
            Some(i) => self.path(&name[..=i], parent)?,
            None => parent,
        };
        if !self.directory_exists(parent) {
            return Err(-120);
        }
        if self.child(parent, leaf).is_some() {
            return Err(-48);
        }
        if self.files.len() + self.directories.len() >= 1024 {
            return Err(-33);
        }
        let node = self.next_node;
        let next = node.checked_add(1).ok_or(-33i16)?;
        self.files.insert(
            node,
            File {
                name: leaf.to_vec(),
                parent,
                data: Vec::new(),
                resource: Vec::new(),
                created,
                modified: created,
                finder,
                extended_finder: [0; 16],
            },
        );
        self.next_node = next;
        self.changed();
        Ok(node)
    }
    pub fn create_resource_file(&mut self, memory: &Memory, name: GuestPtr) -> Result<i16> {
        let name = Self::name_input(memory, name)?;
        if name.is_empty() {
            return Ok(-37);
        }
        let bytes = ResourceFork::default().encode()?;
        let target = match self.path(&name, 2) {
            Ok(node) => {
                let Some(file) = self.file(node) else {
                    return Ok(-48);
                };
                if file.locked {
                    return Ok(-45);
                }
                if file.resource.is_some_and(|r| !r.is_empty()) {
                    return Ok(-48);
                }
                Some(node)
            }
            Err(-43) => None,
            Err(error) => return Ok(error),
        };
        let used = self.space_used();
        if used > super::io::LIMIT - bytes.len() {
            return Ok(-34);
        }
        let node = match target {
            Some(node) => node,
            None => match self.create_file(&name, 0, 2, memory.u32(GuestPtr(0x20c))?, [0; 16]) {
                Ok(node) => node,
                Err(error) => return Ok(error),
            },
        };
        self.files.get_mut(&node).unwrap().resource = bytes;
        self.changed();
        Ok(0)
    }
}
#[cfg(test)]
mod create_spec_tests {
    use super::*;
    use crate::memory::Permissions;
    fn setup() -> (Files, Memory, CreateSpecResource) {
        let f = Files::application(&[], None);
        let mut m = Memory::default();
        m.map(GuestPtr(0), vec![0; 4096], Permissions::RW).unwrap();
        m.write(GuestPtr(0x20c), &1234u32.to_be_bytes()).unwrap();
        let mut spec = [0; 70];
        spec[..2].copy_from_slice(&(-1i16).to_be_bytes());
        spec[2..6].copy_from_slice(&2u32.to_be_bytes());
        spec[6] = 5;
        spec[7..12].copy_from_slice(b"Prefs");
        m.write(GuestPtr(0x100), &spec).unwrap();
        (
            f,
            m,
            CreateSpecResource {
                spec: GuestPtr(0x100),
                creator: *b"TEST",
                kind: *b"pref",
                script: 0,
            },
        )
    }
    #[test]
    fn existing_data_identity_dates_and_finder_fields_survive_resource_initialization_and_duplicate(
    ) {
        let (mut f, m, r) = setup();
        let node = f.create_file(b"Prefs", -1, 2, 17, [0x5a; 16]).unwrap();
        f.files.get_mut(&node).unwrap().data = b"keep data".to_vec();
        assert_eq!(f.create_spec_resource(&m, r).unwrap(), 0);
        let file = &f.files[&node];
        assert_eq!(file.data, b"keep data");
        assert_eq!(file.created, 17);
        assert_eq!(&file.finder[..8], b"prefTEST");
        assert_eq!(&file.finder[8..], &[0x5a; 8]);
        assert_eq!(
            ResourceFork::parse(&file.resource)
                .unwrap()
                .entries()
                .count(),
            0
        );
        let image = f.export_volume().unwrap();
        assert_eq!(
            f.create_spec_resource(
                &m,
                CreateSpecResource {
                    creator: *b"NOPE",
                    ..r
                }
            )
            .unwrap(),
            -48
        );
        assert_eq!(f.export_volume().unwrap(), image);
        let mut restored = Files::application(&[], None);
        restored.restore_volume(&image).unwrap();
        assert_eq!(restored.files[&node].data, b"keep data");
        assert_eq!(restored.files[&node].finder, f.files[&node].finder);
        assert_eq!(restored.files[&node].resource, f.files[&node].resource);
    }
    #[test]
    fn filename_scripts_are_retained_in_extended_finder_bytes_and_catalog_queries() {
        for script in [-1, 0, 1, 2, 127] {
            let (mut f, mut m, r) = setup();
            let node = f.create_file(b"Prefs", -1, 2, 17, [0x5a; 16]).unwrap();
            f.files.get_mut(&node).unwrap().extended_finder = [0xa5; 16];
            assert_eq!(
                f.create_spec_resource(&m, CreateSpecResource { script, ..r })
                    .unwrap(),
                0
            );
            let mut expected = [0xa5; 16];
            expected[8] = if script == -1 { 0 } else { 0x80 | script as u8 };
            assert_eq!(f.files[&node].extended_finder, expected);
            m.write(GuestPtr(0x400), b"\x05Prefs").unwrap();
            let mut pb = [0; 108];
            pb[18..22].copy_from_slice(&0x400u32.to_be_bytes());
            pb[22..24].copy_from_slice(&(-1i16).to_be_bytes());
            pb[48..52].copy_from_slice(&2u32.to_be_bytes());
            m.write(GuestPtr(0x300), &pb).unwrap();
            assert_eq!(f.catalog_info(&mut m, GuestPtr(0x300)).unwrap(), 0);
            assert_eq!(m.read(GuestPtr(0x350), 16).unwrap(), expected);
            let image = f.export_volume().unwrap();
            assert_eq!(u32::from_be_bytes(image[8..12].try_into().unwrap()), 2);
            let mut fresh = Files::application(&[], None);
            fresh.restore_volume(&image).unwrap();
            assert_eq!(fresh.files[&node].extended_finder, expected);
            assert_eq!(fresh.export_volume().unwrap(), image);
        }
        for script in [-32768, -3, -2, 128, 32767] {
            let (mut f, m, r) = setup();
            let before = f.export_volume().unwrap();
            assert_eq!(
                f.create_spec_resource(&m, CreateSpecResource { script, ..r })
                    .unwrap(),
                -50
            );
            assert_eq!(f.export_volume().unwrap(), before);
        }
    }
    #[test]
    fn new_files_have_current_dates_and_failures_do_not_publish_partial_catalog_changes() {
        let (mut f, mut m, r) = setup();
        assert_eq!(f.create_spec_resource(&m, r).unwrap(), 0);
        let node = f.child(2, b"Prefs").unwrap();
        assert_eq!(
            (f.files[&node].created, f.files[&node].modified),
            (1234, 1234)
        );
        let image = f.export_volume().unwrap();
        assert_eq!(
            f.create_spec_resource(&m, CreateSpecResource { script: 128, ..r })
                .unwrap(),
            -50
        );
        assert_eq!(f.export_volume().unwrap(), image);
        for (volume, parent, name, error) in [
            (7i16, 2u32, b"New".as_slice(), -35),
            (-1, 999, b"New".as_slice(), -120),
            (-1, 2, b"".as_slice(), -37),
        ] {
            let mut spec = [0; 70];
            spec[..2].copy_from_slice(&volume.to_be_bytes());
            spec[2..6].copy_from_slice(&parent.to_be_bytes());
            spec[6] = name.len() as u8;
            spec[7..7 + name.len()].copy_from_slice(name);
            m.write(r.spec, &spec).unwrap();
            assert_eq!(f.create_spec_resource(&m, r).unwrap(), error);
            assert_eq!(f.export_volume().unwrap(), image);
        }
        let revision = f.revision();
        assert!(f
            .create_spec_resource(
                &m,
                CreateSpecResource {
                    spec: GuestPtr(0xff0),
                    ..r
                }
            )
            .is_err());
        assert_eq!(f.revision(), revision);
    }
}
