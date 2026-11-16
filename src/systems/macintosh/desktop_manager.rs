//! Desktop database records, independent of the guest CPU ABI.

/// Universal Interfaces 3.4.1 MacErrors.h: replacement icon size differs.
const AFP_ICON_TYPE_ERROR: i16 = -5030;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DesktopIcon {
    pub(crate) creator: u32,
    pub(crate) file_type: u32,
    pub(crate) icon_type: u8,
    pub(crate) data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DesktopApplication {
    pub(crate) creator: u32,
    pub(crate) name: String,
    pub(crate) parent_dir_id: u32,
    pub(crate) creation_date: u32,
}

/// One volume's in-memory icon definitions. The volume owner is responsible
/// for its lifetime, persistence, and access checks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DesktopDatabase {
    icons: Vec<DesktopIcon>,
    applications: Vec<DesktopApplication>,
}

impl DesktopDatabase {
    pub(crate) fn merge_missing(&mut self, other: Self) {
        for icon in other.icons {
            if self
                .icon(icon.creator, icon.file_type, icon.icon_type)
                .is_none()
            {
                self.icons.push(icon);
            }
        }
        for application in other.applications {
            if !self.applications.iter().any(|existing| {
                existing.creator == application.creator
                    && existing.parent_dir_id == application.parent_dir_id
                    && existing.name.eq_ignore_ascii_case(&application.name)
            }) {
                self.applications.push(application);
            }
        }
    }

    pub(crate) fn remap_parent_directories(
        &mut self,
        remapped: &std::collections::HashMap<u32, u32>,
    ) {
        for application in &mut self.applications {
            if let Some(&new_id) = remapped.get(&application.parent_dir_id) {
                application.parent_dir_id = new_id;
            }
        }
    }

    /// Register or refresh an application entry. Re-registering an entry
    /// makes it the most recently added entry.
    pub(crate) fn add_application(&mut self, application: DesktopApplication) {
        if let Some(index) = self.applications.iter().position(|existing| {
            existing.creator == application.creator
                && existing.parent_dir_id == application.parent_dir_id
                && existing.name.eq_ignore_ascii_case(&application.name)
        }) {
            self.applications.remove(index);
        }
        self.applications.push(application);
    }

    /// Apple Technical Q&A TB19: index 0 selects the last database update,
    /// even if another application has a later creation date.
    pub(crate) fn application(&self, creator: u32, index: usize) -> Option<&DesktopApplication> {
        let matches = self
            .applications
            .iter()
            .filter(|entry| entry.creator == creator);
        if index == 0 {
            matches.last()
        } else {
            matches.skip(index - 1).next()
        }
    }

    /// More Macintosh Toolbox, PBDTAddIcon: replacing a matching entry with
    /// differently sized data fails and leaves the existing entry intact.
    pub(crate) fn add_icon(&mut self, icon: DesktopIcon) -> Result<(), i16> {
        if let Some(existing) = self.icons.iter_mut().find(|existing| {
            existing.creator == icon.creator
                && existing.file_type == icon.file_type
                && existing.icon_type == icon.icon_type
        }) {
            if existing.data.len() != icon.data.len() {
                return Err(AFP_ICON_TYPE_ERROR);
            }
            *existing = icon;
        } else {
            self.icons.push(icon);
        }
        Ok(())
    }

    pub(crate) fn icon(&self, creator: u32, file_type: u32, icon_type: u8) -> Option<&DesktopIcon> {
        self.icons.iter().find(|icon| {
            icon.creator == creator && icon.file_type == file_type && icon.icon_type == icon_type
        })
    }

    /// PBDTGetIconInfo enumerates one creator's definitions from index 1.
    pub(crate) fn icon_info(&self, creator: u32, index: usize) -> Option<&DesktopIcon> {
        let index = index.checked_sub(1)?;
        self.icons
            .iter()
            .filter(|icon| icon.creator == creator)
            .nth(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(creator: u32, file_type: u32, icon_type: u8, data: &[u8]) -> DesktopIcon {
        DesktopIcon {
            creator,
            file_type,
            icon_type,
            data: data.to_vec(),
        }
    }

    #[test]
    fn icon_replacement_checks_size_and_keeps_other_definitions() {
        let mut db = DesktopDatabase::default();
        db.add_icon(icon(1, 2, 3, &[1, 2])).unwrap();
        db.add_icon(icon(1, 2, 4, &[9])).unwrap();
        assert_eq!(db.add_icon(icon(1, 2, 3, &[7])), Err(AFP_ICON_TYPE_ERROR));
        assert_eq!(db.icon(1, 2, 3).unwrap().data, [1, 2]);
        db.add_icon(icon(1, 2, 3, &[3, 4])).unwrap();
        assert_eq!(db.icon(1, 2, 3).unwrap().data, [3, 4]);
        assert_eq!(db.icon(1, 2, 4).unwrap().data, [9]);
    }

    #[test]
    fn icon_enumeration_is_one_based_and_scoped_to_creator_and_volume() {
        let mut first = DesktopDatabase::default();
        let mut second = DesktopDatabase::default();
        first.add_icon(icon(1, 2, 3, &[1])).unwrap();
        first.add_icon(icon(9, 2, 3, &[2])).unwrap();
        first.add_icon(icon(1, 5, 6, &[3])).unwrap();
        second.add_icon(icon(1, 2, 3, &[4])).unwrap();
        assert!(first.icon_info(1, 0).is_none());
        assert_eq!(first.icon_info(1, 1).unwrap().data, [1]);
        assert_eq!(first.icon_info(1, 2).unwrap().data, [3]);
        assert!(first.icon_info(1, 3).is_none());
        assert_eq!(second.icon_info(1, 1).unwrap().data, [4]);
    }

    #[test]
    fn application_lookup_selects_last_update_and_enumerates_copies() {
        let mut db = DesktopDatabase::default();
        let app = |creator, name: &str, parent_dir_id, creation_date| DesktopApplication {
            creator,
            name: name.to_owned(),
            parent_dir_id,
            creation_date,
        };
        db.add_application(app(1, "Old", 2, 10));
        db.add_application(app(2, "Other", 2, 50));
        db.add_application(app(1, "New", 2, 20));
        db.add_application(app(1, "Tie", 3, 20));
        assert_eq!(db.application(1, 0).unwrap().name, "Tie");
        assert_eq!(db.application(1, 1).unwrap().name, "Old");
        assert_eq!(db.application(1, 2).unwrap().name, "New");
        assert_eq!(db.application(1, 3).unwrap().name, "Tie");
        assert!(db.application(1, 4).is_none());
        db.add_application(app(1, "new", 2, 20));
        assert_eq!(db.application(1, 0).unwrap().name, "new");
        db.add_application(app(1, "old", 2, 10));
        assert_eq!(db.application(1, 0).unwrap().name, "old");
        assert!(db.application(1, 4).is_none());
        assert_eq!(db.application(2, 0).unwrap().name, "Other");
    }
}
