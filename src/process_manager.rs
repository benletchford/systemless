//! Architecture-neutral process identity shared by guest adapters.

use crate::process_context::{
    ProcessVfsDirectory, ProcessVfsFileRecord, ProcessVfsMetadata, ProcessVfsResourceFileRecord,
};
use std::collections::HashMap;

/// Finder metadata for the application represented by the current process.
///
/// Process Manager adapters encode the path's basename for their guest ABI,
/// but application selection and catalogue identity belong to the process.
/// Inside Macintosh: Processes (1994), pp. 2-23--2-24.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessApplicationMetadata {
    pub(crate) path: String,
    pub(crate) parent_dir_id: u32,
    pub(crate) file_type: u32,
    pub(crate) creator: u32,
}

pub(crate) fn resolve_process_application_metadata(
    directories: &[ProcessVfsDirectory],
    files: &[ProcessVfsFileRecord],
    resource_files: &[ProcessVfsResourceFileRecord],
    classic_metadata: Option<&HashMap<String, ProcessVfsMetadata>>,
    launched_app_path: Option<&str>,
) -> ProcessApplicationMetadata {
    const ROOT_DIR_ID: u32 = 2;
    let appl_type = u32::from_be_bytes(*b"APPL");

    let parent_dir_id_for_path = |path: &str| {
        let parent_path = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        directories
            .iter()
            .find(|directory| directory.path.eq_ignore_ascii_case(parent_path))
            .map_or(ROOT_DIR_ID, |directory| directory.dir_id)
    };
    let from_file = |path: &str, file_type: u32, creator: u32, parent_dir_id| {
        ProcessApplicationMetadata {
            path: path.to_string(),
            parent_dir_id,
            file_type,
            creator,
        }
    };

    if let Some(path) = launched_app_path {
        if let Some((catalogue_path, metadata)) = classic_metadata.and_then(|metadata| {
            metadata
                .iter()
                .find(|(catalogue_path, _)| catalogue_path.eq_ignore_ascii_case(path))
        }) {
            return from_file(
                catalogue_path,
                metadata.file_type,
                metadata.creator,
                metadata.parent_dir_id,
            );
        }
        if let Some(file) = files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(path) && file.file_type == appl_type)
        {
            return from_file(
                &file.path,
                file.file_type,
                file.creator,
                parent_dir_id_for_path(&file.path),
            );
        }
        if let Some(file) = resource_files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(path) && file.file_type == appl_type)
        {
            return from_file(
                &file.path,
                file.file_type,
                file.creator,
                parent_dir_id_for_path(&file.path),
            );
        }
    }

    if let Some(file) = files.iter().find(|file| file.file_type == appl_type) {
        return from_file(
            &file.path,
            file.file_type,
            file.creator,
            parent_dir_id_for_path(&file.path),
        );
    }
    if let Some(file) = resource_files
        .iter()
        .find(|file| file.file_type == appl_type)
    {
        return from_file(
            &file.path,
            file.file_type,
            file.creator,
            parent_dir_id_for_path(&file.path),
        );
    }

    ProcessApplicationMetadata {
        path: "Application".to_string(),
        parent_dir_id: ROOT_DIR_ID,
        file_type: appl_type,
        creator: u32::from_be_bytes(*b"????"),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessSerialNumber {
    pub(crate) high: u32,
    pub(crate) low: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SingleProcessEnumeration {
    Current(ProcessSerialNumber),
    End,
    Invalid,
}

impl ProcessSerialNumber {
    pub(crate) const NONE: Self = Self::new(0, 0);
    pub(crate) const CURRENT: Self = Self::new(0, 2);

    pub(crate) const fn new(high: u32, low: u32) -> Self {
        Self { high, low }
    }

    pub(crate) const fn is_current(self) -> bool {
        self.high == Self::CURRENT.high && self.low == Self::CURRENT.low
    }

    pub(crate) const fn next_single_process(self) -> SingleProcessEnumeration {
        if self.high == Self::NONE.high && self.low == Self::NONE.low {
            SingleProcessEnumeration::Current(Self::CURRENT)
        } else if self.is_current() {
            SingleProcessEnumeration::End
        } else {
            SingleProcessEnumeration::Invalid
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_identity_is_canonical() {
        assert_eq!(ProcessSerialNumber::CURRENT, ProcessSerialNumber::new(0, 2));
        assert!(ProcessSerialNumber::CURRENT.is_current());
        assert!(!ProcessSerialNumber::NONE.is_current());
        assert!(!ProcessSerialNumber::new(0, 3).is_current());
    }

    #[test]
    fn single_process_enumeration_classifies_every_cursor_state() {
        assert_eq!(
            ProcessSerialNumber::NONE.next_single_process(),
            SingleProcessEnumeration::Current(ProcessSerialNumber::CURRENT)
        );
        assert_eq!(
            ProcessSerialNumber::CURRENT.next_single_process(),
            SingleProcessEnumeration::End
        );
        assert_eq!(
            ProcessSerialNumber::new(0, 3).next_single_process(),
            SingleProcessEnumeration::Invalid
        );
    }

    #[test]
    fn application_metadata_prefers_the_launched_application() {
        let directories = vec![ProcessVfsDirectory {
            dir_id: 100,
            parent_dir_id: 2,
            path: "Games".to_string(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        }];
        let files = vec![
            ProcessVfsFileRecord {
                path: "Installer".to_string(),
                data: Vec::new().into(),
                creator: u32::from_be_bytes(*b"INST"),
                file_type: u32::from_be_bytes(*b"APPL"),
                finder_flags: 0,
                dirty: false,
            },
            ProcessVfsFileRecord {
                path: "Games/Current App".to_string(),
                data: Vec::new().into(),
                creator: u32::from_be_bytes(*b"GAME"),
                file_type: u32::from_be_bytes(*b"APPL"),
                finder_flags: 0,
                dirty: false,
            },
        ];

        let metadata = resolve_process_application_metadata(
            &directories,
            &files,
            &[],
            None,
            Some("games/current app"),
        );

        assert_eq!(metadata.path, "Games/Current App");
        assert_eq!(metadata.parent_dir_id, 100);
        assert_eq!(metadata.file_type, u32::from_be_bytes(*b"APPL"));
        assert_eq!(metadata.creator, u32::from_be_bytes(*b"GAME"));
    }

    #[test]
    fn application_metadata_falls_back_to_resource_fork_then_default() {
        let resource_files = vec![ProcessVfsResourceFileRecord {
            path: "Resource App".to_string(),
            creator: u32::from_be_bytes(*b"RSRC"),
            file_type: u32::from_be_bytes(*b"APPL"),
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        }];

        let resource_metadata = resolve_process_application_metadata(
            &[],
            &[],
            &resource_files,
            None,
            None,
        );
        assert_eq!(resource_metadata.path, "Resource App");
        assert_eq!(resource_metadata.parent_dir_id, 2);
        assert_eq!(resource_metadata.creator, u32::from_be_bytes(*b"RSRC"));

        let default_metadata =
            resolve_process_application_metadata(&[], &[], &[], None, None);
        assert_eq!(default_metadata.path, "Application");
        assert_eq!(default_metadata.parent_dir_id, 2);
        assert_eq!(default_metadata.file_type, u32::from_be_bytes(*b"APPL"));
        assert_eq!(default_metadata.creator, u32::from_be_bytes(*b"????"));
    }
}
