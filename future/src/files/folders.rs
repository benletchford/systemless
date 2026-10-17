//! Folder Manager lookup/creation over the shared catalog, IM Toolbox FindFolder.
use super::{Directory, Files};
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindFolder {
    pub volume: i16,
    pub kind: [u8; 4],
    pub create: bool,
    pub found_volume: GuestPtr,
    pub directory: GuestPtr,
}
impl FindFolder {
    pub fn validate(self, m: &Memory) -> Result<()> {
        if self.found_volume.0 == 0 || self.directory.0 == 0 {
            return Err(Error::Cpu("nil FindFolder output".into()));
        }
        m.validate_write(self.found_volume, 2)?;
        m.validate_write(self.directory, 4)
    }
}
impl Files {
    pub(crate) fn system_volume_reference(&self) -> Option<i16> {
        self.system_folder.map(|_| -1)
    }
    fn folder_node(
        &mut self,
        parent: u32,
        name: &[u8],
        create: bool,
        now: u32,
    ) -> std::result::Result<u32, i16> {
        if let Some(id) = self.child(parent, name) {
            return if self.directories.contains_key(&id) {
                Ok(id)
            } else {
                Err(-48)
            };
        }
        if !create {
            return Err(-43);
        }
        if self.files.len() + self.directories.len() >= 1024 || self.next_node == u32::MAX {
            return Err(-33);
        }
        let id = self.next_node;
        self.directories.insert(
            id,
            Directory {
                name: name.to_vec(),
                parent,
                created: now,
            },
        );
        self.next_node += 1;
        self.changed();
        Ok(id)
    }
    /// Host volume initialization, separate from FindFolder (which never creates
    /// a System Folder). Call after restoring a catalog and before running a file app.
    pub fn provision_system_folder(&mut self, m: &Memory) -> Result<i16> {
        if self.name.is_empty() {
            return Ok(-35);
        }
        if self.system_folder.is_some() {
            return Ok(0);
        }
        let now = m.u32(crate::calendar::TIME)?;
        match self.folder_node(2, b"System Folder", true, now) {
            Ok(id) => {
                self.system_folder = Some(id);
                self.changed();
                Ok(0)
            }
            Err(error) => Ok(error),
        }
    }
    pub fn find_folder(&mut self, m: &mut Memory, request: FindFolder) -> Result<i16> {
        request.validate(m)?;
        if self.name.is_empty() {
            return Ok(-35);
        }
        if request.volume != i16::MIN && self.parent(request.volume, 0).is_err() {
            return Ok(-35);
        }
        let (system, name) = match &request.kind {
            b"macs" => (true, b"".as_slice()),
            b"pref" => (true, b"Preferences".as_slice()),
            b"amnu" => (true, b"Apple Menu Items".as_slice()),
            b"ctrl" => (true, b"Control Panels".as_slice()),
            b"extn" => (true, b"Extensions".as_slice()),
            b"font" => (true, b"Fonts".as_slice()),
            b"prnt" => (true, b"PrintMonitor Documents".as_slice()),
            b"strt" => (true, b"Startup Items".as_slice()),
            b"desk" => (false, b"Desktop Folder".as_slice()),
            b"temp" => (false, b"Temporary Items".as_slice()),
            b"trsh" | b"empt" => (false, b"Trash".as_slice()),
            _ => return Ok(-43),
        };
        let parent = if system {
            match self.system_folder {
                Some(id) => id,
                None => return Ok(-43),
            }
        } else {
            2
        };
        let id = if name.is_empty() {
            parent
        } else {
            let now = m.u32(crate::calendar::TIME)?;
            match self.folder_node(parent, name, request.create, now) {
                Ok(id) => id,
                Err(error) => return Ok(error),
            }
        };
        // Both outputs have already been preflighted, including before creation.
        m.write(request.found_volume, &(-1i16).to_be_bytes())?;
        m.write(request.directory, &id.to_be_bytes())?;
        Ok(0)
    }
}
