//! Resource files use the same catalog nodes, forks and access paths as raw I/O.
use super::{File, Files};
use crate::{
    load::resource::ResourceFork,
    memory::{GuestPtr, Memory},
    Result,
};
impl Files {
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
        let used: usize = self
            .files
            .values()
            .map(|f| f.data.len() + f.resource.len())
            .sum();
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
