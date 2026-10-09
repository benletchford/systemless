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
    /// The current DynamicMenuList names an application MBDF rather than
    /// the standard menu bar definition function.
    pub custom_bar_definition: bool,
}

impl GuestMenuSnapshot {
    /// A custom MDEF can draw arbitrary pixels and define its own hit regions.
    /// Macintosh Toolbox Essentials (1992), pp. 3-3, 3-87.
    pub fn requires_guest_menu_rendering(&self) -> bool {
        self.custom_bar_definition || self.menus.iter().any(|menu| !menu.standard_definition)
    }

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

    /// Reject an action from a popup built for a disposed or replaced menu.
    pub fn selectable_result_for_guest(
        &self,
        menu_id: i16,
        item_number: i16,
        guest_id: u32,
        generation: u64,
    ) -> Option<u32> {
        let menu = self.menus.iter().find(|menu| menu.id == menu_id)?;
        (menu.guest_id == guest_id && menu.generation == generation)
            .then(|| self.selectable_result(menu_id, item_number))
            .flatten()
    }
}

/// Guest font identity and point size used to lay out and present a menu.
/// `popupUseWFont` must retain this context from its owner GrafPort; ordinary
/// menus use the system font. Inside Macintosh VI (1991), p. 3-18.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuestMenuFont {
    pub family: i16,
    pub size: i16,
}

impl Default for GuestMenuFont {
    fn default() -> Self {
        Self { family: 0, size: 12 }
    }
}

impl GuestMenuFont {
    /// QuickDraw treats size zero as the system size.
    pub fn point_size(self) -> i16 {
        if self.size == 0 {
            12
        } else {
            self.size.max(1)
        }
    }

    pub(crate) fn metrics(self) -> crate::quickdraw::fonts::FontMetrics {
        let (face, numerator, denominator) =
            crate::quickdraw::fonts::get_font_face_scale_ratio(self.family, self.point_size());
        let scale = |value: i16| {
            ((i32::from(value) * numerator + denominator / 2) / denominator)
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
        };
        crate::quickdraw::fonts::FontMetrics {
            ascent: scale(face.metrics.ascent),
            descent: scale(face.metrics.descent),
            leading: scale(face.metrics.leading),
            wid_max: scale(face.metrics.wid_max),
        }
    }

    pub(crate) fn text_advance(self, text: &[u8]) -> i16 {
        let (face, numerator, denominator) =
            crate::quickdraw::fonts::get_font_face_scale_ratio(self.family, self.point_size());
        let advance = text.iter().fold(0i32, |advance, byte| {
            let glyph = crate::quickdraw::text::get_glyph(self.family, face.size, char::from(*byte))
                .map(|(glyph, _)| i32::from(glyph.advance))
                .unwrap_or(6);
            advance.saturating_add(glyph)
        });
        let scaled = advance
            .saturating_mul(numerator)
            .saturating_add(denominator / 2)
            / denominator;
        i16::try_from(scaled).unwrap_or(i16::MAX)
    }
}

/// Read-only geometry of an open standard popup. Pointer input must still pass
/// through the guest tracker; this snapshot never commits a menu selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestPopupSnapshot {
    pub menu: GuestMenu,
    pub font: GuestMenuFont,
    /// Global guest coordinates: top, left, bottom, right.
    pub bounds: (i16, i16, i16, i16),
    /// Global top of the first row, including the guest's scrolling offset.
    pub content_top: i16,
    pub row_heights: Vec<i16>,
    /// One-based item number, or zero when no item is highlighted.
    pub highlighted_item: i16,
}

impl GuestPopupSnapshot {
    /// Standard popup text anchors resolved by the same layout as both guest painters.
    /// Coordinates are relative to the pane; icon-bearing rows need their icon context.
    pub fn text_anchors(&self, row_top: i16, row_height: i16) -> (i16, i16, i16, i16) {
        let metrics = self.font.metrics();
        let layout = crate::menu_manager::standard_menu_item_layout(
            (0, self.bounds.3.saturating_sub(self.bounds.1)),
            (row_top, row_height), crate::menu_manager::StandardMenuIconKind::None,
            false, (metrics.ascent, metrics.descent), false,
        );
        (layout.mark_left, layout.text_left, layout.command_left, layout.text_baseline)
    }

    /// Whether the top and bottom row slots are occupied by scrolling arrows.
    /// Uses the same hidden-content calculation as the standard guest tracker.
    pub fn scroll_indicators(&self) -> (bool, bool) {
        let rows = crate::menu_manager::MenuRows::new(self.row_heights.iter().map(|height| {
            crate::menu_manager::MenuRow { height: *height, selectable: false }
        }));
        rows.scroll_indicators(self.bounds, self.content_top)
    }
}

/// One menu in the guest's current menu list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestMenu {
    /// Live guest MenuHandle; use as presentation identity within this run.
    pub guest_id: u32,
    /// Changes when a disposed MenuHandle address is reused.
    pub generation: u64,
    pub id: i16,
    pub title: String,
    pub enabled: bool,
    /// Whether the live menu uses the standard MDEF. Custom MDEFs own their
    /// drawing and hit testing (Macintosh Toolbox Essentials, pp. 3-3, 3-87).
    pub standard_definition: bool,
    /// A hierarchical menu is reached through an item in another menu and
    /// does not itself have a menu-bar title.
    pub hierarchical: bool,
    pub visible_in_menu_bar: bool,
    pub items: Vec<GuestMenuItem>,
}

/// One 1-based Menu Manager item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestMenuItem {
    /// Raw Menu Manager mark byte; hierarchical marks are submenu IDs.
    pub mark: u8,
    /// Live Menu Manager QuickDraw style bits for the item text.
    pub style: u8,
    pub number: i16,
    pub text: String,
    pub enabled: bool,
    pub checked: bool,
    /// Mac Roman command character with guest case preserved for display.
    /// Matching remains in the guest Menu Manager, independently of presentation.
    pub key_equivalent: Option<char>,
    pub submenu_id: Option<i16>,
    pub separator: bool,
}

#[cfg(test)]
mod tests {
    use super::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};

    #[test]
    fn menu_font_sizes_use_guest_metrics_and_quickdraw_zero_size() {
        let text = b"Deep Field Archive";
        let small = super::GuestMenuFont { family: 3, size: 9 };
        let normal = super::GuestMenuFont { family: 3, size: 12 };
        let large = super::GuestMenuFont { family: 3, size: 24 };
        assert!(small.text_advance(text) < normal.text_advance(text));
        assert!(large.text_advance(text) > normal.text_advance(text));
        let zero = super::GuestMenuFont { family: 3, size: 0 };
        assert_eq!(zero.point_size(), 12);
        assert_eq!(zero.text_advance(text), normal.text_advance(text));
        assert_eq!(small.text_advance(&[]), 0);
        assert_eq!(large.text_advance(&vec![b'W'; 20_000]), i16::MAX);
    }

    fn snapshot(menu_enabled: bool, item: GuestMenuItem) -> GuestMenuSnapshot {
        GuestMenuSnapshot {
            custom_bar_definition: false,
            menus: vec![GuestMenu {
                guest_id: 0x1000,
                generation: 1,
                id: -120,
                title: "File".to_owned(),
                enabled: menu_enabled,
                standard_definition: true,
                hierarchical: false,
                visible_in_menu_bar: true,
                items: vec![item],
            }],
        }
    }

    fn item() -> GuestMenuItem {
        GuestMenuItem {
            mark: 0,
            style: 0,
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

    #[test]
    fn stale_popup_selection_cannot_target_a_reused_menu_id() {
        let mut current = snapshot(true, item());
        assert_eq!(
            current.selectable_result_for_guest(-120, 2, 0x1000, 1),
            Some(0xff88_0002)
        );
        current.menus[0].guest_id = 0x2000;
        assert_eq!(current.selectable_result_for_guest(-120, 2, 0x1000, 1), None);
        current.menus[0].guest_id = 0x1000;
        current.menus[0].generation = 2;
        assert_eq!(current.selectable_result_for_guest(-120, 2, 0x1000, 1), None);
    }

    #[test]
    fn custom_definition_requires_guest_rendering() {
        let mut menus = snapshot(true, item());
        assert!(!menus.requires_guest_menu_rendering());
        menus.menus[0].standard_definition = false;
        assert!(menus.requires_guest_menu_rendering());
        menus.menus[0].standard_definition = true;
        menus.custom_bar_definition = true;
        assert!(menus.requires_guest_menu_rendering());
    }
}

/// Pixels of the standard Menu Manager hierarchy triangle, relative to its
/// left edge and vertical centre. Shared by guest and frontend painters.
pub fn standard_hierarchy_indicator_pixels() -> Vec<(i16, i16)> {
    let mut pixels = Vec::new();
    crate::menu_manager::for_each_standard_hierarchy_indicator_pixel(0, 0,
        |x, y| pixels.push((x, y)));
    pixels
}
