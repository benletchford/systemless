//! Frontend-neutral snapshots of the guest Menu Manager state.
//!
//! Frontends may present these menus using native host controls.  A snapshot
//! is deliberately immutable: commands must be routed back through
//! [`crate::runner::FixtureRunner::select_guest_menu_item`], which validates
//! the selection against the current Menu Manager state before waking the
//! guest application.

/// The guest's current inserted menu list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GuestMenuSnapshot {
    pub menus: Vec<GuestMenu>,
}

impl GuestMenuSnapshot {
    /// Validate a host-presented command against the immutable projection of
    /// the live Menu Manager state and return its packed MenuSelect result.
    /// Disabled menus/items, dividers, and submenu-parent rows cannot be
    /// chosen. Macintosh Toolbox Essentials (1992), pp. 3-115--3-119.
    pub(crate) fn selectable_result(&self, menu_id: i16, item_number: i16) -> Option<u32> {
        let menu = self.menus.iter().find(|menu| menu.id == menu_id)?;
        let item = menu.items.iter().find(|item| item.number == item_number)?;
        if !menu.enabled || !item.enabled || item.separator || item.submenu_id.is_some() {
            return None;
        }
        Some((u32::from(menu_id as u16) << 16) | u32::from(item_number as u16))
    }
}

/// One menu in the guest's current menu list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestMenu {
    pub id: i16,
    pub title: String,
    pub enabled: bool,
    /// A hierarchical menu is reached through an item in another menu and
    /// does not itself have a menu-bar title.
    pub hierarchical: bool,
    pub visible_in_menu_bar: bool,
    pub items: Vec<GuestMenuItem>,
}

/// One 1-based Menu Manager item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestMenuItem {
    pub number: i16,
    pub text: String,
    pub enabled: bool,
    pub checked: bool,
    pub key_equivalent: Option<char>,
    pub submenu_id: Option<i16>,
    pub separator: bool,
}

#[cfg(test)]
mod tests {
    use super::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};

    fn snapshot(menu_enabled: bool, item: GuestMenuItem) -> GuestMenuSnapshot {
        GuestMenuSnapshot {
            menus: vec![GuestMenu {
                id: -120,
                title: "File".to_owned(),
                enabled: menu_enabled,
                hierarchical: false,
                visible_in_menu_bar: true,
                items: vec![item],
            }],
        }
    }

    fn item() -> GuestMenuItem {
        GuestMenuItem {
            number: 2,
            text: "Open".to_owned(),
            enabled: true,
            checked: false,
            key_equivalent: Some('o'),
            submenu_id: None,
            separator: false,
        }
    }

    #[test]
    fn native_selection_validation_is_architecture_neutral() {
        assert_eq!(
            snapshot(true, item()).selectable_result(-120, 2),
            Some(0xff88_0002)
        );
        assert_eq!(snapshot(false, item()).selectable_result(-120, 2), None);

        let mut disabled = item();
        disabled.enabled = false;
        assert_eq!(snapshot(true, disabled).selectable_result(-120, 2), None);
        let mut separator = item();
        separator.separator = true;
        assert_eq!(snapshot(true, separator).selectable_result(-120, 2), None);
        let mut parent = item();
        parent.submenu_id = Some(200);
        assert_eq!(snapshot(true, parent).selectable_result(-120, 2), None);
    }
}
