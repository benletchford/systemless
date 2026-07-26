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
