//! Shared Standard File volume-selector state and guest workflow regressions.

#[cfg(test)]
use super::StandardFileSnapshot;

/// Guest-owned volume choice; names retain the mounted volume's identity.
#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct StandardFileVolumeChoice {
    pub ref_num: i16,
    pub root_dir_id: u32,
    pub name: String,
}

/// Package-local popup tracking. CPU adapters own painting, saved pixels and
/// event delivery; this state owns choices, scrolling and acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct StandardFileVolumePopup {
    pub choices: Vec<StandardFileVolumeChoice>,
    pub bounds: (i16, i16, i16, i16),
    pub row_height: i16,
    pub first_visible: usize,
    pub visible_rows: usize,
    pub highlighted: Option<usize>,
    last_pointer: Option<(i16, i16)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StandardFileVolumeResult {
    Pending,
    Cancel,
    Select { ref_num: i16, root_dir_id: u32 },
}

impl StandardFileVolumePopup {
    /// The same guest-font width limit is used by native and GPUI painters.
    pub fn display_name(&self, index: usize) -> Option<String> {
        self.choices.get(index).map(|choice| crate::trap::TrapDispatcher::popup_control_display_title(
            &choice.name, (self.bounds.3 - self.bounds.1 - 12).max(0), 0, 12))
    }

    pub(crate) fn volume_for_directory(volumes: &[crate::process_context::ProcessVfsVolumeRecord],
        directories: &[crate::process_context::ProcessVfsDirectory], mut directory_id: u32) -> StandardFileVolumeChoice {
        let choices = Self::mounted_choices(volumes);
        for _ in 0..=directories.len() {
            if let Some(choice) = choices.iter().find(|choice| choice.root_dir_id == directory_id) {
                return choice.clone();
            }
            let Some(directory) = directories.iter().find(|directory| directory.dir_id == directory_id) else { break; };
            if directory.parent_dir_id == directory_id { break; }
            directory_id = directory.parent_dir_id;
        }
        choices[0].clone()
    }

    pub(crate) fn mounted_choices(volumes: &[crate::process_context::ProcessVfsVolumeRecord])
        -> Vec<StandardFileVolumeChoice> {
        let boot = volumes.iter().find(|volume|
            volume.ref_num == crate::trap::dispatch::BOOT_VOLUME_REF_NUM && volume.root_dir_id >= 2);
        let mut choices = vec![StandardFileVolumeChoice {
            ref_num: crate::trap::dispatch::BOOT_VOLUME_REF_NUM,
            root_dir_id: boot.map_or(2, |volume| volume.root_dir_id),
            name: boot.map_or_else(|| crate::trap::dispatch::BOOT_VOLUME_NAME.into(), |volume| volume.name.clone()),
        }];
        for volume in volumes {
            if volume.root_dir_id < 2 || choices.iter().any(|choice| choice.ref_num == volume.ref_num) { continue; }
            choices.push(StandardFileVolumeChoice {
                ref_num: volume.ref_num, root_dir_id: volume.root_dir_id, name: volume.name.clone(),
            });
        }
        choices
    }

    #[doc(hidden)]
    pub fn new(choices: Vec<StandardFileVolumeChoice>, current_ref: i16,
        anchor: (i16, i16, i16, i16), viewport: (i16, i16, i16, i16), width: i16) -> Option<Self> {
        let row_height = 16;
        let available = i32::from(viewport.2) - i32::from(viewport.0);
        if choices.is_empty() || available < i32::from(row_height)
            || viewport.3 <= viewport.1 || width <= 0 { return None; }
        let visible_rows = choices.len().min((available / i32::from(row_height)) as usize);
        let highlighted = choices.iter().position(|choice| choice.ref_num == current_ref).unwrap_or(0);
        let first_visible = highlighted.saturating_sub(visible_rows - 1);
        let height = visible_rows as i32 * i32::from(row_height);
        let width = i32::from(width).min(i32::from(viewport.3) - i32::from(viewport.1));
        let top = (i32::from(anchor.0) - (highlighted - first_visible) as i32 * i32::from(row_height))
            .clamp(i32::from(viewport.0), i32::from(viewport.2) - height);
        let left = i32::from(anchor.1).clamp(i32::from(viewport.1), i32::from(viewport.3) - width);
        Some(Self { choices, bounds: (top as i16, left as i16, (top + height) as i16, (left + width) as i16),
            row_height, first_visible, visible_rows, highlighted: Some(highlighted), last_pointer: None })
    }

    pub(crate) fn hover(&mut self, v: i16, h: i16) -> bool {
        if self.last_pointer == Some((v, h)) { return false; }
        self.last_pointer = Some((v, h));
        let previous = self.highlighted;
        self.pointer(v, h, false);
        self.highlighted != previous
    }

    pub(crate) fn pointer(&mut self, v: i16, h: i16, released: bool) -> StandardFileVolumeResult {
        let (top, left, bottom, right) = self.bounds;
        self.highlighted = (v >= top && v < bottom && h >= left && h < right)
            .then(|| self.first_visible + ((i32::from(v) - i32::from(top)) / i32::from(self.row_height)) as usize)
            .filter(|index| *index < self.choices.len());
        if released { self.accept() } else { StandardFileVolumeResult::Pending }
    }

    pub(crate) fn key(&mut self, character: u8, code: u8, modifiers: u16) -> StandardFileVolumeResult {
        if crate::dialog_manager::is_dialog_cancel_key(character, code, modifiers) {
            return StandardFileVolumeResult::Cancel;
        }
        if crate::dialog_manager::is_dialog_default_key(character, code) { return self.accept(); }
        let current = self.highlighted.unwrap_or(self.first_visible);
        let next = match (character, code) {
            (0x1e, _) | (_, 0x7e) => current.saturating_sub(1),
            (0x1f, _) | (_, 0x7d) => (current + 1).min(self.choices.len() - 1),
            (_, 0x73) => 0,
            (_, 0x77) => self.choices.len() - 1,
            (_, 0x74) => current.saturating_sub(self.visible_rows),
            (_, 0x79) => (current + self.visible_rows).min(self.choices.len() - 1),
            _ => return StandardFileVolumeResult::Pending,
        };
        self.highlighted = Some(next);
        if next < self.first_visible { self.first_visible = next; }
        if next >= self.first_visible + self.visible_rows { self.first_visible = next + 1 - self.visible_rows; }
        StandardFileVolumeResult::Pending
    }

    fn accept(&self) -> StandardFileVolumeResult {
        self.highlighted.and_then(|index| self.choices.get(index)).map_or(StandardFileVolumeResult::Cancel,
            |choice| StandardFileVolumeResult::Select { ref_num: choice.ref_num, root_dir_id: choice.root_dir_id })
    }
}

#[cfg(test)]
mod volume_popup_tests {
    use super::*;
    fn popup() -> StandardFileVolumePopup {
        let choices = (0..5).map(|index| StandardFileVolumeChoice {
            ref_num: -1 - index, root_dir_id: 2 + index as u32 * 100, name: format!("Disk {index}"),
        }).collect();
        StandardFileVolumePopup::new(choices, -4, (80, 90, 99, 164), (0, 0, 48, 120), 100).unwrap()
    }
    fn volume(ref_num: i16, name: &str, root_dir_id: u32) -> crate::process_context::ProcessVfsVolumeRecord {
        crate::process_context::ProcessVfsVolumeRecord {
            ref_num, name: name.into(), root_dir_id, attributes: 0x0080,
            file_count: 0, allocation_block_count: 0, allocation_block_size: 0,
            clump_size: 0, free_blocks: 0, bitmap_start: 0, allocation_pointer: 0,
            allocation_start: 0, next_catalog_id: 0, created_date: 0, modified_date: 0,
        }
    }
    #[test]
    fn volume_popup_choices_preserve_mounted_names_roots_and_read_only_volumes() {
        let volumes = [volume(-1, "Boot café", 2), volume(-2, "Archive", 102),
            volume(-2, "Duplicate", 202), volume(-3, "Invalid", 0)];
        let choices = StandardFileVolumePopup::mounted_choices(&volumes);
        assert_eq!(choices, vec![StandardFileVolumeChoice { ref_num: -1, root_dir_id: 2, name: "Boot café".into() },
            StandardFileVolumeChoice { ref_num: -2, root_dir_id: 102, name: "Archive".into() }]);
        assert_eq!(StandardFileVolumePopup::mounted_choices(&[]), vec![StandardFileVolumeChoice {
            ref_num: -1, root_dir_id: 2, name: crate::trap::dispatch::BOOT_VOLUME_NAME.into() }]);
    }
    #[test]
    fn volume_popup_directory_ancestry_resolves_volume_identity_without_name_matching() {
        let volumes = [volume(-1, "Renamed Boot", 2), volume(-2, "Archive", 102)];
        let directory = |dir_id, parent_dir_id| crate::process_context::ProcessVfsDirectory {
            dir_id, parent_dir_id, path: "unrelated/path".into(), creator: 0, file_type: 0, finder_flags: 0, dirty: false };
        let directories = [directory(103, 102), directory(104, 103), directory(999, 999)];
        let choice = StandardFileVolumePopup::volume_for_directory(&volumes, &directories, 104);
        assert_eq!((choice.ref_num, choice.root_dir_id, choice.name.as_str()), (-2, 102, "Archive"));
        assert_eq!(StandardFileVolumePopup::volume_for_directory(&volumes, &directories, 999).name, "Renamed Boot");
    }
    #[test]
    fn volume_popup_pointer_release_and_cancellation_preserve_choice_identity() {
        let mut popup = popup();
        assert_eq!(popup.bounds, (0, 20, 48, 120));
        assert_eq!(popup.first_visible, 1);
        assert_eq!(popup.pointer(1, 21, false), StandardFileVolumeResult::Pending);
        assert_eq!(popup.pointer(1, 21, true), StandardFileVolumeResult::Select { ref_num: -2, root_dir_id: 102 });
        assert_eq!(popup.pointer(48, 21, true), StandardFileVolumeResult::Cancel);
        assert_eq!(popup.pointer(1, 120, true), StandardFileVolumeResult::Cancel);
    }
    #[test]
    fn volume_popup_keyboard_scrolls_and_accepts_or_cancels() {
        let mut popup = popup();
        assert_eq!(popup.key(0, 0x77, 0), StandardFileVolumeResult::Pending);
        assert_eq!(popup.first_visible, 2);
        assert_eq!(popup.key(13, 0x24, 0), StandardFileVolumeResult::Select { ref_num: -5, root_dir_id: 402 });
        popup.key(0, 0x73, 0); assert_eq!(popup.first_visible, 0);
        popup.key(0, 0x79, 0); assert_eq!(popup.highlighted, Some(3));
        popup.key(0, 0x74, 0); assert_eq!(popup.highlighted, Some(0));
        assert_eq!(popup.key(b'.', 0, 0x100), StandardFileVolumeResult::Cancel);
        assert_eq!(popup.key(27, 0x35, 0), StandardFileVolumeResult::Cancel);
    }
    #[test]
    fn volume_popup_switches_mounted_guest_directory_on_both_cpus() {
        use crate::systems::macintosh::session::{MacintoshInput, MacintoshSession};
        fn advance(session: &mut MacintoshSession) {
            let tick = session.runner().guest_tick().saturating_add(1);
            session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
        }
        fn wait_panel(session: &mut MacintoshSession, stage: &str, predicate: impl Fn(&StandardFileSnapshot) -> bool) -> StandardFileSnapshot {
            for _ in 0..300 {
                advance(session);
                if let Some(panel) = session.runner_mut().standard_file_snapshot().filter(|panel| predicate(panel)) { return panel; }
            }
            panic!("guest panel did not reach {stage}: {:?}", session.runner().standard_file_snapshot()
                .map(|panel| (panel.directory_id, panel.volume_popup.map(|popup| (popup.highlighted, popup.choices)))));
        }
        for (powerpc, depth) in [(false, 1), (false, 8), (true, 8), (true, 16)] {
            let mut session = MacintoshSession::new(true, Some(depth));
            session.runner_mut().set_prefer_powerpc_executables(powerpc);
            let app = session.load_path(&std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
            session.initialize(&app);
            {
                let dispatcher = session.runner_mut().dispatcher_mut();
                dispatcher.vfs_volumes.with_mut(|volumes| volumes.push(volume(-42, "Archive", 42000)));
                dispatcher.vfs_directories.with_mut(|directories| {
                    for (dir_id, parent_dir_id, path) in [(42000, 1, "Archive"), (42001, 42000, "Archive/Samples")] {
                        assert!(!directories.iter().any(|directory| directory.dir_id == dir_id));
                        directories.push(crate::process_context::ProcessVfsDirectory {
                            dir_id, parent_dir_id, path: path.into(), creator: 0, file_type: 0, finder_flags: 0, dirty: false });
                    }
                });
            }
            for _ in 0..300 {
                advance(&mut session);
                if session.runner_mut().guest_menu_snapshot().menus.iter().any(|menu| menu.id == 129) { break; }
            }
            assert!(session.runner_mut().select_guest_menu_item(129, 12));
            for _ in 0..300 {
                advance(&mut session);
                if session.runner_mut().guest_menu_snapshot().menus.iter().any(|menu| menu.id == 129
                    && menu.items.iter().any(|item| item.number == 12 && item.checked)) { break; }
            }
            for _ in 0..10 { advance(&mut session); }
            session.deliver_input(MacintoshInput::MouseDown { vertical: 266, horizontal: 126 });
            session.deliver_input(MacintoshInput::MouseUp { vertical: 266, horizontal: 126 });
            let opened = wait_panel(&mut session, "Open panel", |_| true);
            let selector = opened.get_layout.as_ref().unwrap().volume;
            session.deliver_input(MacintoshInput::MouseDown { vertical: selector.0 + 5, horizontal: selector.1 + 5 });
            wait_panel(&mut session, "selector open", |panel| panel.volume_popup.is_some());
            session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x77, character: 0 });
            session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x77, character: 0 });
            wait_panel(&mut session, "Archive highlighted", |panel| panel.volume_popup.as_ref().is_some_and(|popup|
                popup.highlighted.and_then(|index| popup.choices.get(index)).is_some_and(|choice| choice.root_dir_id == 42000)));
            session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x24, character: 13 });
            session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x24, character: 13 });
            let switched = wait_panel(&mut session, "Archive directory", |panel| panel.directory_id == 42000 && panel.volume_popup.is_none());
            assert_eq!(switched.directory_label.as_deref(), Some("Archive"));
            assert!(switched.volume_text.as_ref().is_some_and(|(text, _)| text.starts_with("Ar")));
            assert!(switched.entries.as_ref().is_some_and(|entries| entries.iter().any(|entry|
                entry.name == "Samples" && entry.is_directory && entry.directory_id == 42001)));
            assert_eq!((switched.guest_id, switched.generation), (opened.guest_id, opened.generation));
        }
    }
    #[test]
    fn volume_popup_stationary_pointer_preserves_keyboard_navigation() {
        let mut popup = popup();
        assert!(popup.hover(1, 21)); assert_eq!(popup.highlighted, Some(1));
        popup.key(0, 0x7d, 0); assert_eq!(popup.highlighted, Some(2));
        assert!(!popup.hover(1, 21)); assert_eq!(popup.highlighted, Some(2));
        assert!(popup.hover(33, 21)); assert_eq!(popup.highlighted, Some(3));
        assert!(popup.hover(49, 21)); assert_eq!(popup.highlighted, None);
    }
    #[test]
    fn volume_popup_declines_empty_choices_and_unusable_geometry() {
        assert!(StandardFileVolumePopup::new(Vec::new(), -1, (0, 0, 1, 1), (0, 0, 100, 100), 50).is_none());
        assert!(StandardFileVolumePopup::new(popup().choices, -1, (0, 0, 1, 1), (0, 0, 15, 100), 50).is_none());
    }
}
