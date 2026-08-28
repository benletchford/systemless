//! Architecture-neutral Menu Manager records and list operations.

use crate::mac_roman::decode_mac_roman;
use crate::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};

/// Largest entry count representable by a menu-list partition byte length.
pub(crate) const MAX_MENU_LIST_ENTRIES: usize = u16::MAX as usize / 6;

const MAX_MENU_ITEMS: usize = 1024;

/// The public Menu Manager operation that owns an active tracking session.
///
/// Guest ABI continuation details deliberately live in the architecture
/// adapter rather than in this manager-owned state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuTrackingKind {
    MenuBar,
    PopUp,
}

/// Direction currently armed by a standard scrolling-menu indicator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuScrollDirection {
    Up,
    Down,
}

/// Result of one standard scrolling-menu pointer update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuPointerUpdate {
    pub(crate) item: i16,
    pub(crate) scrolled: bool,
}

/// Retained state for one visible menu pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrackedMenuPane<MenuRef, Surface, Pixel, Appearance> {
    pub(crate) parent_item: i16,
    pub(crate) menu_handle: MenuRef,
    pub(crate) popup_left: i16,
    pub(crate) popup_top: i16,
    pub(crate) content_top: i16,
    pub(crate) scroll_direction: Option<MenuScrollDirection>,
    pub(crate) popup_width: i16,
    pub(crate) popup_height: i16,
    pub(crate) highlighted_item: i16,
    pub(crate) saved_width: i16,
    pub(crate) saved_height: i16,
    pub(crate) front_buffer: Surface,
    pub(crate) saved_pixels: Vec<Pixel>,
    pub(crate) item_appearances: Vec<Appearance>,
}

/// One retained Menu Manager tracking continuation.
///
/// `Surface` and `Appearance` are presentation snapshots supplied by an
/// architecture adapter. Menu identity, hierarchy, selection, and flashing
/// remain owned by this common state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MenuTrackingState<MenuRef, Surface, Pixel, Appearance> {
    pub(crate) kind: MenuTrackingKind,
    pub(crate) menu_handle: MenuRef,
    pub(crate) popup_left: i16,
    pub(crate) popup_top: i16,
    pub(crate) content_top: i16,
    pub(crate) scroll_direction: Option<MenuScrollDirection>,
    pub(crate) popup_width: i16,
    pub(crate) popup_height: i16,
    pub(crate) highlighted_item: i16,
    pub(crate) flash_remaining: u8,
    pub(crate) flash_delay: u8,
    pub(crate) flash_result: u32,
    pub(crate) saved_width: i16,
    pub(crate) saved_height: i16,
    pub(crate) front_buffer: Surface,
    pub(crate) saved_pixels: Vec<Pixel>,
    pub(crate) item_appearances: Vec<Appearance>,
    pub(crate) submenus: Vec<TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>>,
}

/// Result of reconciling one resolved hierarchical child with the retained
/// open-menu chain. Presentation adapters restore any returned panes before
/// drawing a replacement.
pub(crate) enum SubmenuTransition<Pane> {
    Keep,
    Reject(Vec<Pane>),
    Open(Vec<Pane>),
}

/// Common read-only view used by tracking geometry and presentation code.
pub(crate) trait TrackedMenuPaneView {
    type Surface: Copy;
    type MenuRef: Copy;
    type Pixel;
    type Appearance;

    fn menu_handle(&self) -> Self::MenuRef;
    fn popup_left(&self) -> i16;
    fn popup_top(&self) -> i16;
    fn content_top(&self) -> i16;
    fn popup_width(&self) -> i16;
    fn popup_height(&self) -> i16;
    fn dropdown_rect(&self) -> (i16, i16, i16, i16) {
        (
            self.popup_top(),
            self.popup_left(),
            self.popup_top().saturating_add(self.popup_height()),
            self.popup_left().saturating_add(self.popup_width()),
        )
    }
    fn saved_width(&self) -> i16;
    fn saved_height(&self) -> i16;
    fn front_buffer(&self) -> Self::Surface;
    fn saved_pixels(&self) -> &[Self::Pixel];
    fn item_appearances(&self) -> &[Self::Appearance];
}

macro_rules! impl_tracked_menu_pane_view {
    ($type:ident) => {
        impl<MenuRef: Copy, Surface: Copy, Pixel, Appearance> TrackedMenuPaneView
            for $type<MenuRef, Surface, Pixel, Appearance>
        {
            type Surface = Surface;
            type MenuRef = MenuRef;
            type Pixel = Pixel;
            type Appearance = Appearance;

            fn menu_handle(&self) -> Self::MenuRef {
                self.menu_handle
            }
            fn popup_left(&self) -> i16 {
                self.popup_left
            }
            fn popup_top(&self) -> i16 {
                self.popup_top
            }
            fn content_top(&self) -> i16 {
                self.content_top
            }
            fn popup_width(&self) -> i16 {
                self.popup_width
            }
            fn popup_height(&self) -> i16 {
                self.popup_height
            }
            fn saved_width(&self) -> i16 {
                self.saved_width
            }
            fn saved_height(&self) -> i16 {
                self.saved_height
            }
            fn front_buffer(&self) -> Self::Surface {
                self.front_buffer
            }
            fn saved_pixels(&self) -> &[Self::Pixel] {
                &self.saved_pixels
            }
            fn item_appearances(&self) -> &[Self::Appearance] {
                &self.item_appearances
            }
        }
    };
}

impl_tracked_menu_pane_view!(MenuTrackingState);
impl_tracked_menu_pane_view!(TrackedMenuPane);

impl<MenuRef: Copy, Surface, Pixel, Appearance>
    MenuTrackingState<MenuRef, Surface, Pixel, Appearance>
{
    /// Remove every open submenu from `depth` onward and return the panes so
    /// the presentation adapter can restore their saved pixels deepest-first.
    pub(crate) fn close_submenus_from(
        &mut self,
        depth: usize,
    ) -> Vec<TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>> {
        if depth >= self.submenus.len() {
            Vec::new()
        } else {
            self.submenus.split_off(depth)
        }
    }

    /// Apply one root or submenu highlight transition and return panes that
    /// the presentation adapter must close. A nonzero item may subsequently
    /// open one child; zero always closes the existing child chain.
    pub(crate) fn update_highlight(
        &mut self,
        parent_depth: Option<usize>,
        new_item: i16,
    ) -> Option<Vec<TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>>> {
        let old_item = match parent_depth {
            Some(depth) => self.submenus.get(depth)?.highlighted_item,
            None => self.highlighted_item,
        };
        let child_depth = parent_depth.map_or(0, |depth| depth.saturating_add(1));
        let closed = if old_item != new_item || new_item <= 0 {
            self.close_submenus_from(child_depth)
        } else {
            Vec::new()
        };
        match parent_depth {
            Some(depth) => self.submenus.get_mut(depth)?.highlighted_item = new_item,
            None => self.highlighted_item = new_item,
        }
        Some(closed)
    }

    /// Reject a submenu that would repeat the root or an already-open
    /// ancestor. Circular hierarchical menu definitions are invalid.
    /// Macintosh Toolbox Essentials (1992), p. 3-138.
    pub(crate) fn submenu_repeats_ancestor(&self, child_depth: usize, menu_handle: MenuRef) -> bool
    where
        MenuRef: PartialEq,
    {
        menu_handle == self.menu_handle
            || self
                .submenus
                .iter()
                .take(child_depth)
                .any(|submenu| submenu.menu_handle == menu_handle)
    }

    /// Decide whether a resolved hierarchical child remains open, is
    /// rejected as circular, or replaces the existing child chain.
    pub(crate) fn prepare_submenu(
        &mut self,
        child_depth: usize,
        parent_item: i16,
        menu_handle: MenuRef,
    ) -> SubmenuTransition<TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>>
    where
        MenuRef: PartialEq,
    {
        if self.submenu_repeats_ancestor(child_depth, menu_handle) {
            return SubmenuTransition::Reject(self.close_submenus_from(child_depth));
        }
        if self.submenus.get(child_depth).is_some_and(|submenu| {
            submenu.menu_handle == menu_handle && submenu.parent_item == parent_item
        }) {
            return SubmenuTransition::Keep;
        }
        SubmenuTransition::Open(self.close_submenus_from(child_depth))
    }

    /// Find the deepest open submenu pane hit by the adapter-supplied point
    /// test. Retained hierarchy ordering is manager state; pane geometry and
    /// coordinate conversion remain presentation concerns.
    #[cfg(test)]
    pub(crate) fn deepest_submenu_hit(
        &self,
        mut hit: impl FnMut(usize, &TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>) -> Option<i16>,
    ) -> Option<(usize, i16)> {
        self.submenus
            .iter()
            .enumerate()
            .rev()
            .find_map(|(depth, submenu)| hit(depth, submenu).map(|item| (depth, item)))
    }

    /// Return the deepest highlighted terminal item in the open hierarchy.
    ///
    /// Architecture adapters supply live guest-record validation so this
    /// common continuation never turns a presentation snapshot into menu
    /// authority.
    pub(crate) fn selection(
        &self,
        mut is_terminal_item: impl FnMut(MenuRef, i16) -> bool,
    ) -> Option<(MenuRef, i16)> {
        for submenu in self.submenus.iter().rev() {
            if submenu.highlighted_item > 0
                && is_terminal_item(submenu.menu_handle, submenu.highlighted_item)
            {
                return Some((submenu.menu_handle, submenu.highlighted_item));
            }
        }
        (self.highlighted_item > 0 && is_terminal_item(self.menu_handle, self.highlighted_item))
            .then_some((self.menu_handle, self.highlighted_item))
    }
}

/// One item decoded from a guest `MenuInfo` record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MenuItem {
    pub(crate) text: Vec<u8>,
    pub(crate) icon: u8,
    pub(crate) command: u8,
    pub(crate) mark: u8,
    pub(crate) style: u8,
    pub(crate) enabled: bool,
}

/// Decode the submenu identity stored in a standard MenuInfo item. A command
/// byte of `$1B` makes the mark byte the submenu menu ID; zero is not a usable
/// menu ID. Macintosh Toolbox Essentials (1992), pp. 3-53--3-55.
pub(crate) fn hierarchical_menu_id(command: u8, mark: u8) -> Option<i16> {
    (command == 0x1B && mark != 0).then_some(i16::from(mark))
}

/// One laid-out row in a standard menu pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuRow {
    pub(crate) height: i16,
    pub(crate) selectable: bool,
}

/// Architecture-neutral vertical geometry for a standard menu pane.
///
/// Menu items use 1-based numbers, disabled items remain laid out but cannot
/// be selected, and separators occupy rows without becoming selections.
/// Inside Macintosh Volume I (1985), pp. I-345 and I-355--I-358; Macintosh
/// Toolbox Essentials (1992), pp. 3-95--3-97 and 3-131.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MenuRows {
    rows: Vec<MenuRow>,
}

/// Architecture-neutral result of positioning a standard pop-up menu.
/// `content_top` is the screen coordinate of the uncropped first item and can
/// lie outside the visible rectangle when the standard MDEF scrolls. The
/// one-pixel shadow is presentation state and lies immediately outside the
/// rectangle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PopupMenuLayout {
    pub(crate) top: i16,
    pub(crate) left: i16,
    pub(crate) content_top: i16,
    pub(crate) width: i16,
    pub(crate) height: i16,
    pub(crate) highlighted_item: i16,
}

impl PopupMenuLayout {
    pub(crate) fn rect(self) -> (i16, i16, i16, i16) {
        (
            self.top,
            self.left,
            self.top.saturating_add(self.height),
            self.left.saturating_add(self.width),
        )
    }
}

/// Platinum's standard separator metric on both supported Mac OS 8.1
/// profiles. `GetThemeMenuSeparatorHeight` reports six pixels, and direct
/// `CalcMenuSize` observations report 38 pixels for 16 + separator + 16.
pub(crate) const STANDARD_MENU_SEPARATOR_HEIGHT: i16 = 6;

/// Horizontal inputs resolved by an architecture adapter for one standard
/// menu item. Text and icon resource measurement remain presentation work;
/// the standard MDEF's column policy belongs to the Menu Manager.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StandardMenuItemWidth {
    pub(crate) text: i16,
    pub(crate) icon: i16,
    pub(crate) command: u8,
}

/// Compute `MenuInfo.menuWidth` for the standard Mac OS 8.1 MDEF.
///
/// Every item reserves 32 pixels around its text and leading icon slot.
/// Command-key and hierarchical items reserve one additional 32-pixel
/// indicator column. A hierarchy command reserves that column even when its
/// mark byte does not identify a usable child menu. Direct `CalcMenuSize`
/// observations are identical on the supported 68040 and 604 profiles.
pub(crate) fn standard_menu_width(items: impl IntoIterator<Item = StandardMenuItemWidth>) -> i16 {
    items.into_iter().fold(32i16, |width, item| {
        let indicator = if item.command > 0x20 || item.command == 0x1B {
            32
        } else {
            0
        };
        width.max(
            item.text
                .max(0)
                .saturating_add(item.icon.max(0))
                .saturating_add(32)
                .saturating_add(indicator),
        )
    })
}

/// Limit `MenuInfo.menuHeight` for the standard Mac OS 8.1 MDEF.
///
/// The definition procedure sums complete item rows, then caps the result at
/// the captured maximum. Both supported 800-by-600 profiles
/// report 560 pixels for a 640-pixel, 40-item menu. The general requirement
/// that menu height not exceed the available display is documented by
/// Macintosh Toolbox Essentials (1992), pp. 3-88--3-90; the exact limit is
/// profile evidence rather than an inferred screen-margin formula. The caller
/// supplies the screen height remaining below the menu bar, as required by the
/// same reference.
pub(crate) fn standard_menu_height(rows: &MenuRows, available_screen_height: i16) -> i16 {
    const MACOS_81_STANDARD_MENU_MAX_HEIGHT: i16 = 560;
    rows.total_height()
        .max(0)
        .min(MACOS_81_STANDARD_MENU_MAX_HEIGHT.min(available_screen_height.max(0)))
}

/// Compute the standard MDEF row height from its resolved icon geometry and
/// text style. Macintosh Toolbox Essentials (1992), pp. 3-45--3-46 and
/// 3-133--3-138 defines the icon variants, `cicn` priority, and style inputs;
/// the 16-, 21-, and 34-pixel metrics match the standard System 7.5.3 MDEF.
pub(crate) fn standard_menu_row_height(
    color_icon_height: Option<i16>,
    uses_normal_icon: bool,
    uses_shadow_style: bool,
) -> i16 {
    let base_height = if let Some(height) = color_icon_height {
        height.max(16)
    } else if uses_normal_icon {
        34
    } else {
        16
    };
    if uses_shadow_style {
        base_height.max(21)
    } else {
        base_height
    }
}

/// Return the item prefix laid out by the standard menu definition procedure.
/// A divider separates groups of commands (Macintosh Human Interface
/// Guidelines 1992, p. 63), and the Apple-menu resource pattern deliberately
/// leaves one at the end for appended items (Macintosh Toolbox Essentials
/// 1992, pp. 3-97--3-98). The standard System 7.5.3 MDEF omits that row when
/// no appended group exists.
pub(crate) fn laid_out_menu_item_count<T>(
    items: &[T],
    mut is_separator: impl FnMut(&T) -> bool,
) -> usize {
    let mut end = items.len();
    while end > 0 && is_separator(&items[end - 1]) {
        end -= 1;
    }
    end
}

impl MenuRows {
    pub(crate) fn new(rows: impl IntoIterator<Item = MenuRow>) -> Self {
        Self {
            rows: rows.into_iter().collect(),
        }
    }

    pub(crate) fn total_height(&self) -> i16 {
        self.rows
            .iter()
            .fold(0i16, |height, row| height.saturating_add(row.height.max(0)))
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn height(&self, item_number: i16, fallback: i16) -> i16 {
        menu_item_index(item_number)
            .and_then(|index| self.rows.get(index))
            .map(|row| row.height.max(0))
            .unwrap_or(fallback)
    }

    pub(crate) fn offset(&self, item_number: i16) -> i16 {
        let count = usize::try_from(item_number.saturating_sub(1).max(0)).unwrap_or(0);
        self.rows
            .iter()
            .take(count)
            .fold(0i16, |offset, row| offset.saturating_add(row.height.max(0)))
    }

    pub(crate) fn item_at_offset(&self, offset: i16) -> Option<i16> {
        if offset < 0 {
            return None;
        }
        let mut remaining = offset;
        for (index, row) in self.rows.iter().enumerate() {
            let height = row.height.max(0);
            if remaining < height {
                let item_number = i16::try_from(index + 1).ok()?;
                return Some(if row.selectable { item_number } else { 0 });
            }
            remaining = remaining.saturating_sub(height);
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn item_at_point(
        &self,
        rect: (i16, i16, i16, i16),
        insets: (i16, i16, i16, i16),
        point: (i16, i16),
    ) -> Option<i16> {
        self.item_at_point_with_content_top(rect, insets, rect.0, point)
    }

    pub(crate) fn item_at_point_with_content_top(
        &self,
        rect: (i16, i16, i16, i16),
        insets: (i16, i16, i16, i16),
        first_item_top: i16,
        point: (i16, i16),
    ) -> Option<i16> {
        let (top, left, bottom, right) = rect;
        let (inset_top, inset_left, inset_bottom, inset_right) = insets;
        let (vertical, horizontal) = point;
        let visible_content_top = top.saturating_add(inset_top);
        let content_left = left.saturating_add(inset_left);
        let content_bottom = bottom.saturating_sub(inset_bottom);
        let content_right = right.saturating_sub(inset_right);
        if vertical < top || vertical >= bottom || horizontal < left || horizontal >= right {
            return None;
        }
        if vertical < visible_content_top
            || vertical >= content_bottom
            || horizontal < content_left
            || horizontal >= content_right
        {
            return Some(0);
        }
        Some(
            self.item_at_offset(vertical.saturating_sub(first_item_top))
                .unwrap_or(0),
        )
    }

    /// Return whether the first or last visible row position is occupied by a
    /// standard scrolling indicator. Inside Macintosh Volume V (1986),
    /// pp. V-248--V-249.
    pub(crate) fn scroll_indicators(
        &self,
        rect: (i16, i16, i16, i16),
        content_top: i16,
    ) -> (bool, bool) {
        let content_bottom = content_top.saturating_add(self.total_height());
        (content_top < rect.0, content_bottom > rect.2)
    }

    pub(crate) fn pointer_scroll_direction(
        &self,
        rect: (i16, i16, i16, i16),
        content_top: i16,
        point: (i16, i16),
    ) -> Option<MenuScrollDirection> {
        const INDICATOR_HEIGHT: i16 = 16;

        let (top, left, bottom, right) = rect;
        let (vertical, horizontal) = point;
        let (hidden_above, hidden_below) = self.scroll_indicators(rect, content_top);
        let inside_horizontal = horizontal >= left && horizontal < right;
        if inside_horizontal && hidden_above && vertical < top.saturating_add(INDICATOR_HEIGHT) {
            Some(MenuScrollDirection::Up)
        } else if inside_horizontal
            && hidden_below
            && vertical >= bottom.saturating_sub(INDICATOR_HEIGHT)
        {
            Some(MenuScrollDirection::Down)
        } else {
            None
        }
    }

    pub(crate) fn tracking_item_at_point(
        &self,
        rect: (i16, i16, i16, i16),
        content_top: i16,
        point: (i16, i16),
    ) -> i16 {
        if self
            .pointer_scroll_direction(rect, content_top, point)
            .is_some()
        {
            0
        } else {
            self.item_at_point_with_content_top(rect, (0, 0, 0, 0), content_top, point)
                .unwrap_or(0)
        }
    }

    /// Apply one standard MDEF scrolling-menu pointer update.
    ///
    /// A first call over an indicator arms that direction without selecting
    /// an item. Each subsequent tracking call in the same direction moves the
    /// complete content bounds by one 16-pixel row. Points directly above or
    /// below the pane use the same retained direction. The behavior and
    /// `TopMenuItem`/`AtMenuBottom` deltas are byte-identical in direct Mac OS
    /// 8.1 68040 and 604 MDEF captures. Macintosh Toolbox Essentials (1992),
    /// pp. 3-87--3-92 and 3-151.
    pub(crate) fn track_pointer(
        &self,
        rect: (i16, i16, i16, i16),
        content_top: &mut i16,
        armed_direction: &mut Option<MenuScrollDirection>,
        point: (i16, i16),
    ) -> MenuPointerUpdate {
        const SCROLL_STEP: i16 = 16;

        let (top, _, bottom, _) = rect;
        let direction = self.pointer_scroll_direction(rect, *content_top, point);

        if let Some(direction) = direction {
            let scrolled = *armed_direction == Some(direction);
            if scrolled {
                *content_top = match direction {
                    MenuScrollDirection::Up => content_top.saturating_add(SCROLL_STEP).min(top),
                    MenuScrollDirection::Down => content_top
                        .saturating_sub(SCROLL_STEP)
                        .max(bottom.saturating_sub(self.total_height())),
                };
            }
            *armed_direction = Some(direction);
            return MenuPointerUpdate { item: 0, scrolled };
        }

        *armed_direction = None;
        MenuPointerUpdate {
            item: self.tracking_item_at_point(rect, *content_top, point),
            scrolled: false,
        }
    }
}

const STANDARD_POPUP_SCREEN_MARGIN: i16 = 4;
const STANDARD_POPUP_BOTTOM_RESERVE: i16 = 20;
const STANDARD_POPUP_SCROLL_STEP: i16 = 16;

/// Position the standard Mac OS 8.1 pop-up menu and its uncropped content.
///
/// The standard MDEF's `mPopUpMsg` aligns the requested item's top-left with
/// the caller's `Top`/`Left`, uses exactly the `CalcMenuSize` dimensions, and
/// subtracts only preceding row heights. When the full content exceeds the
/// display, the visible pane is limited to the captured scrolling viewport and
/// aligned to the requested row while `content_top` preserves the uncropped
/// item origin returned through `TopMenuItem`. The behavior is byte-identical
/// on the supported 68040 and 604 Mac OS 8.1 profiles. Macintosh Toolbox
/// Essentials (1992), pp. 3-120 and 3-148--3-151.
pub(crate) fn standard_popup_menu_layout(
    rows: &MenuRows,
    width: i16,
    screen_size: (i16, i16),
    anchor: (i16, i16),
    requested_item: i16,
) -> Option<PopupMenuLayout> {
    let (screen_width, screen_height) = screen_size;
    if rows.len() == 0
        || screen_width <= STANDARD_POPUP_SCREEN_MARGIN.saturating_mul(2)
        || screen_height
            <= STANDARD_POPUP_SCREEN_MARGIN.saturating_add(STANDARD_POPUP_BOTTOM_RESERVE)
    {
        return None;
    }
    let highlighted_item = usize::try_from(requested_item)
        .ok()
        .filter(|item| (1..=rows.len()).contains(item))
        .map_or(0, |_| requested_item);
    let width = width
        .max(1)
        .min(screen_width.saturating_sub(STANDARD_POPUP_SCREEN_MARGIN.saturating_mul(2)));
    let content_height = rows.total_height().max(1);
    let max_viewport_height = screen_height
        .saturating_sub(STANDARD_POPUP_SCREEN_MARGIN)
        .saturating_sub(STANDARD_POPUP_BOTTOM_RESERVE);
    let scrolling = content_height > max_viewport_height;
    let height = content_height.min(max_viewport_height);
    let desired_top = if highlighted_item > 0 {
        anchor.0.saturating_sub(rows.offset(highlighted_item))
    } else {
        anchor.0
    };
    let max_left = screen_width
        .saturating_sub(STANDARD_POPUP_SCREEN_MARGIN)
        .saturating_sub(width);
    let max_top = screen_height.saturating_sub(height).saturating_sub(1);
    let top = if scrolling {
        STANDARD_POPUP_SCREEN_MARGIN.saturating_add(
            anchor
                .0
                .saturating_sub(STANDARD_POPUP_SCREEN_MARGIN)
                .rem_euclid(STANDARD_POPUP_SCROLL_STEP),
        )
    } else {
        desired_top.clamp(0, max_top.max(0))
    };
    Some(PopupMenuLayout {
        top,
        left: anchor.1.clamp(
            STANDARD_POPUP_SCREEN_MARGIN,
            max_left.max(STANDARD_POPUP_SCREEN_MARGIN),
        ),
        content_top: if scrolling { desired_top } else { top },
        width,
        height,
        highlighted_item,
    })
}

/// Keep the standard MBDF's menu frame clear of the screen edges used for its
/// right/bottom shadow and scrolling affordances.
const STANDARD_MENU_RIGHT_MARGIN: i16 = 8;
const STANDARD_MENU_BOTTOM_MARGIN: i16 = 12;
const STANDARD_SUBMENU_RIGHT_OVERLAP: i16 = 4;
const STANDARD_SUBMENU_LEFT_OVERLAP: i16 = 8;
const STANDARD_SUBMENU_MENU_BAR_CLEARANCE: i16 = 7;

/// Position a standard pull-down menu from its live menu-list title origin.
///
/// Mac OS 8.1 places the menu directly below the menu bar, preserves the
/// `CalcMenuSize` dimensions when they fit, and moves an overflowing menu
/// left far enough to retain the standard eight-pixel right margin. The
/// standard MDEF owns scrolling when the full height does not fit; until that
/// viewport is modeled, the returned height is the visible non-scrolling
/// portion.
pub(crate) fn standard_pull_down_menu_layout(
    width: i16,
    height: i16,
    screen_size: (i16, i16),
    title_left: i16,
    menu_bar_height: i16,
) -> Option<PopupMenuLayout> {
    let (screen_width, screen_height) = screen_size;
    let max_right = screen_width.saturating_sub(STANDARD_MENU_RIGHT_MARGIN);
    let max_bottom = screen_height.saturating_sub(STANDARD_MENU_BOTTOM_MARGIN);
    if menu_bar_height < 0 || max_right <= 0 || max_bottom <= menu_bar_height {
        return None;
    }
    let width = width.max(1).min(max_right);
    let height = height
        .max(1)
        .min(max_bottom.saturating_sub(menu_bar_height));
    Some(PopupMenuLayout {
        top: menu_bar_height,
        left: title_left.clamp(0, max_right.saturating_sub(width)),
        content_top: menu_bar_height,
        width,
        height,
        highlighted_item: 0,
    })
}

/// Position a standard hierarchical menu beside its parent item.
///
/// The standard MBDF overlaps a child four pixels into a right-opening parent
/// or eight pixels into a left-opening parent. It aligns to the parent row
/// unless doing so would cover the menu bar or cross the bottom screen margin.
/// Inside Macintosh Volume V (1986), pp. V-250--V-254.
pub(crate) fn standard_submenu_layout(
    parent_rect: (i16, i16, i16, i16),
    parent_row_offset: i16,
    width: i16,
    height: i16,
    screen_size: (i16, i16),
    menu_bar_height: i16,
) -> Option<PopupMenuLayout> {
    let (screen_width, screen_height) = screen_size;
    let (parent_top, parent_left, _parent_bottom, parent_right) = parent_rect;
    let max_right = screen_width.saturating_sub(STANDARD_MENU_RIGHT_MARGIN);
    let min_top = menu_bar_height
        .max(0)
        .saturating_add(STANDARD_SUBMENU_MENU_BAR_CLEARANCE);
    let max_bottom = screen_height.saturating_sub(STANDARD_MENU_BOTTOM_MARGIN);
    if max_right <= 0 || max_bottom <= min_top {
        return None;
    }
    let width = width.max(1).min(max_right);
    let height = height.max(1).min(max_bottom.saturating_sub(min_top));
    let max_left = max_right.saturating_sub(width);
    let right_opening_left = parent_right.saturating_sub(STANDARD_SUBMENU_RIGHT_OVERLAP);
    let left = if right_opening_left >= 0 && right_opening_left.saturating_add(width) <= max_right {
        right_opening_left
    } else {
        parent_left
            .saturating_add(STANDARD_SUBMENU_LEFT_OVERLAP)
            .saturating_sub(width)
            .clamp(0, max_left)
    };
    let max_top = max_bottom.saturating_sub(height);
    Some(PopupMenuLayout {
        top: parent_top
            .saturating_add(parent_row_offset)
            .clamp(min_top, max_top),
        left,
        content_top: parent_top
            .saturating_add(parent_row_offset)
            .clamp(min_top, max_top),
        width,
        height,
        highlighted_item: 0,
    })
}

/// Parse the item-description string accepted by `AppendMenu` and
/// `InsertMenuItem`.
///
/// Semicolon or Return separates items. The `^`, `!`, `<`, `/`, and `(`
/// metacharacters set the icon number, mark, style, command key, and disabled
/// state respectively. Each `<` consumes exactly one style letter, so callers
/// repeat it to combine styles. Icon digits are decoded by subtracting ASCII `0`.
/// Inside Macintosh Volume I (1985), pp. I-347 and I-358; Macintosh Toolbox
/// Essentials (1992), pp. 3-124--3-126.
pub(crate) fn parse_menu_item_specs(data: &[u8]) -> Vec<MenuItem> {
    let mut result = Vec::new();
    for raw_item in data.split(|byte| matches!(*byte, b';' | b'\r')) {
        if raw_item.is_empty() {
            continue;
        }
        let mut item = MenuItem {
            text: Vec::new(),
            icon: 0,
            command: 0,
            mark: 0,
            style: 0,
            enabled: true,
        };
        let mut cursor = 0usize;
        while cursor < raw_item.len() {
            match raw_item[cursor] {
                b'^' if cursor + 1 < raw_item.len() => {
                    item.icon = raw_item[cursor + 1].saturating_sub(b'0');
                    cursor += 2;
                }
                b'/' if cursor + 1 < raw_item.len() => {
                    item.command = raw_item[cursor + 1];
                    cursor += 2;
                }
                b'!' if cursor + 1 < raw_item.len() => {
                    item.mark = raw_item[cursor + 1];
                    cursor += 2;
                }
                b'<' if cursor + 1 < raw_item.len() => {
                    let style = match raw_item[cursor + 1].to_ascii_uppercase() {
                        b'B' => 0x01,
                        b'I' => 0x02,
                        b'U' => 0x04,
                        b'O' => 0x08,
                        b'S' => 0x10,
                        _ => 0,
                    };
                    item.style |= style;
                    cursor += 2;
                }
                b'(' => {
                    item.enabled = false;
                    cursor += 1;
                }
                byte => {
                    if item.text.len() < 255 {
                        item.text.push(byte);
                    }
                    cursor += 1;
                }
            }
        }
        result.push(item);
    }
    result
}

/// The variable item portion of a guest `MenuInfo` record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MenuItems {
    pub(crate) first_item: usize,
    pub(crate) enable_flags: u32,
    pub(crate) items: Vec<MenuItem>,
}

/// Build the empty standard `MenuInfo` record created by `NewMenu`.
///
/// The architecture gateway supplies the standard MDEF handle because that
/// handle belongs to its guest memory adapter. The record layout and initial
/// enabled state are common Menu Manager semantics. Macintosh Toolbox
/// Essentials (1992), pp. 3-95--3-97, 3-105--3-106.
pub(crate) fn new_standard_menu_record(menu_id: i16, menu_proc: u32, title: &[u8]) -> Vec<u8> {
    let title = &title[..title.len().min(255)];
    let mut bytes = Vec::with_capacity(16 + title.len());
    bytes.extend_from_slice(&menu_id.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes());
    bytes.extend_from_slice(&menu_proc.to_be_bytes());
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    bytes.push(title.len() as u8);
    bytes.extend_from_slice(title);
    bytes.push(0);
    bytes
}

/// The MenuInfo fields that participate in `MenuKey` resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MenuKeyMenu {
    pub(crate) id: i16,
    pub(crate) enabled: bool,
    pub(crate) items: Vec<MenuKeyItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuKeyItem {
    pub(crate) command: u8,
    pub(crate) mark: u8,
    pub(crate) enabled: bool,
}

/// A command-key match and the regular menu title that owns its hierarchy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuKeySelection {
    pub(crate) menu_handle: u32,
    pub(crate) owner_handle: Option<u32>,
    pub(crate) menu_id: i16,
    pub(crate) item_number: i16,
}

impl MenuKeySelection {
    pub(crate) fn packed_result(self) -> u32 {
        (u32::from(self.menu_id as u16) << 16) | u32::from(self.item_number as u16)
    }
}

impl MenuItems {
    /// Decode the title-relative item sequence in a guest menu record.
    ///
    /// `MenuInfo.menuData` contains a Pascal title followed by Pascal item
    /// strings and four attribute bytes per item. Bit 0 of `enableFlags`
    /// controls the title and bits 1 through 31 control the first 31 items.
    /// Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 3-95--3-97.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        Self::decode_with(|offset| bytes.get(offset).copied())
    }

    /// Decode a guest menu record through an architecture-neutral byte
    /// reader. CPU gateways can therefore retain their memory adapters while
    /// sharing the exact MenuInfo item traversal and bounds policy.
    pub(crate) fn decode_with(mut read: impl FnMut(usize) -> Option<u8>) -> Option<Self> {
        let title_len = usize::from(read(14)?);
        let first_item = 15usize.checked_add(title_len)?;
        let enable_flags = u32::from_be_bytes([read(10)?, read(11)?, read(12)?, read(13)?]);
        let mut cursor = first_item;
        let mut items = Vec::new();
        while items.len() < MAX_MENU_ITEMS {
            let len = usize::from(read(cursor)?);
            if len == 0 {
                return Some(Self {
                    first_item,
                    enable_flags,
                    items,
                });
            }
            let attributes = cursor.checked_add(1 + len)?;
            let end = attributes.checked_add(4)?;
            let mut text = Vec::with_capacity(len);
            for offset in cursor + 1..attributes {
                text.push(read(offset)?);
            }
            let item_number = items.len() + 1;
            items.push(MenuItem {
                text,
                icon: read(attributes)?,
                command: read(attributes + 1)?,
                mark: read(attributes + 2)?,
                style: read(attributes + 3)?,
                enabled: item_number > 31 || enable_flags & (1u32 << item_number) != 0,
            });
            cursor = end;
        }
        None
    }

    /// Return the number of complete items decoded before the required zero
    /// end marker. CountMItems reports this value from the supplied MenuInfo
    /// record. Inside Macintosh Volume I (1985), p. I-361.
    pub(crate) fn item_count(&self) -> u16 {
        u16::try_from(self.items.len()).unwrap_or(u16::MAX)
    }

    /// Return the submenu ID named by a 1-based standard menu item.
    pub(crate) fn hierarchical_id(&self, item_number: i16) -> Option<i16> {
        let item = menu_item_index(item_number).and_then(|index| self.items.get(index))?;
        hierarchical_menu_id(item.command, item.mark)
    }

    pub(crate) fn item_is_hierarchical(&self, item_number: i16) -> bool {
        self.hierarchical_id(item_number).is_some()
    }

    /// Rebuild a guest record after changing its item sequence.
    pub(crate) fn rebuild(&self, original: &[u8]) -> Option<Vec<u8>> {
        let mut bytes = original.get(..self.first_item)?.to_vec();
        let mut enable_flags = self.enable_flags;
        for (index, item) in self.items.iter().enumerate() {
            let text_len = item.text.len().min(255);
            bytes.push(text_len as u8);
            bytes.extend_from_slice(&item.text[..text_len]);
            bytes.extend_from_slice(&[item.icon, item.command, item.mark, item.style]);
            let item_number = index + 1;
            if item_number <= 31 {
                if item.enabled {
                    enable_flags |= 1u32 << item_number;
                } else {
                    enable_flags &= !(1u32 << item_number);
                }
            }
        }
        bytes.push(0);
        bytes
            .get_mut(10..14)?
            .copy_from_slice(&enable_flags.to_be_bytes());
        Some(bytes)
    }

    pub(crate) fn append_specs(&mut self, data: &[u8]) -> bool {
        let definitions = parse_menu_item_specs(data);
        if definitions.is_empty() {
            return false;
        }
        self.items.extend(definitions);
        true
    }

    pub(crate) fn insert_specs(&mut self, data: &[u8], after_item: i16) -> bool {
        let definitions = parse_menu_item_specs(data);
        if definitions.is_empty() {
            return false;
        }
        let insertion = usize::try_from(after_item.max(0))
            .unwrap_or(0)
            .min(self.items.len());
        // InsertMenuItem inserts every parsed definition at the same
        // location, producing the documented reverse order.
        for definition in definitions {
            self.items.insert(insertion, definition);
        }
        true
    }

    pub(crate) fn delete(&mut self, item_number: i16) -> bool {
        let Some(index) = menu_item_index(item_number) else {
            return false;
        };
        if index >= self.items.len() {
            return false;
        }
        self.items.remove(index);
        true
    }

    pub(crate) fn set_text(&mut self, item_number: i16, text: &[u8]) -> bool {
        let Some(item) = menu_item_index(item_number).and_then(|index| self.items.get_mut(index))
        else {
            return false;
        };
        item.text.clear();
        item.text.extend_from_slice(&text[..text.len().min(255)]);
        true
    }

    pub(crate) fn set_icon(&mut self, item_number: i16, icon: u8) -> bool {
        self.set_attribute(item_number, |item| item.icon = icon)
    }

    pub(crate) fn set_command(&mut self, item_number: i16, command: u8) -> bool {
        self.set_attribute(item_number, |item| item.command = command)
    }

    pub(crate) fn set_mark(&mut self, item_number: i16, mark: u8) -> bool {
        self.set_attribute(item_number, |item| item.mark = mark)
    }

    pub(crate) fn set_style(&mut self, item_number: i16, style: u8) -> bool {
        self.set_attribute(item_number, |item| item.style = style)
    }

    pub(crate) fn set_enabled(&mut self, item_number: i16, enabled: bool) -> bool {
        if item_number == 0 {
            if enabled {
                self.enable_flags |= 1;
            } else {
                self.enable_flags &= !1;
            }
            return true;
        }
        if !(1..=31).contains(&item_number) {
            return false;
        }
        self.set_attribute(item_number, |item| item.enabled = enabled)
    }

    fn set_attribute(&mut self, item_number: i16, update: impl FnOnce(&mut MenuItem)) -> bool {
        let Some(item) = menu_item_index(item_number).and_then(|index| self.items.get_mut(index))
        else {
            return false;
        };
        update(item);
        true
    }
}

fn menu_item_index(item_number: i16) -> Option<usize> {
    item_number
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
}

/// One six-byte entry in a guest `DynamicMenuList` partition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuListEntry {
    pub(crate) handle: u32,
    pub(crate) value: i16,
}

/// The regular and hierarchical partitions of the current guest menu list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MenuList {
    pub(crate) last_right: i16,
    pub(crate) mb_res_id: i16,
    pub(crate) regular: Vec<MenuListEntry>,
    pub(crate) menu_title_save: u32,
    pub(crate) hierarchical: Vec<MenuListEntry>,
}

/// Live guest MenuInfo data supplied by a CPU memory adapter when projecting
/// the current menu list for a frontend.
pub(crate) struct MenuSnapshotRecord {
    pub(crate) id: i16,
    pub(crate) title: Vec<u8>,
    pub(crate) items: MenuItems,
}

/// The ordered menu resource IDs stored in a compiled `'MBAR'` resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MenuBarResource {
    pub(crate) menu_ids: Vec<i16>,
}

impl MenuBarResource {
    /// Decode the declared count followed by that many signed `'MENU'`
    /// resource IDs. Macintosh Toolbox Essentials (1992), pp. 3-111 and
    /// 3-155.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        let count = usize::from(read_u16(bytes, 0)?);
        if count > MAX_MENU_LIST_ENTRIES {
            return None;
        }
        let required = 2usize.checked_add(count.checked_mul(2)?)?;
        if required > bytes.len() {
            return None;
        }
        let mut menu_ids = Vec::with_capacity(count);
        for index in 0..count {
            menu_ids.push(read_u16(bytes, 2 + index * 2)? as i16);
        }
        Some(Self { menu_ids })
    }

    /// Load the ordered menu records named by this menu-bar resource.
    ///
    /// `GetNewMBar` uses `GetMenu` for each ID in the compiled `'MBAR'`
    /// sequence. Resource lookup, failure reporting, and guest handle
    /// allocation remain adapter operations; MBAR traversal and relative
    /// ordering are common Menu Manager behavior. Macintosh Toolbox
    /// Essentials (1992), pp. 3-111--3-112.
    pub(crate) fn load_regular_handles(
        &self,
        mut load_menu: impl FnMut(i16) -> Option<u32>,
    ) -> Vec<u32> {
        self.menu_ids
            .iter()
            .filter_map(|menu_id| load_menu(*menu_id))
            .collect()
    }
}

impl MenuList {
    /// Construct the regular partition created by `GetNewMBar` after its
    /// adapter loads the ordered `'MENU'` resources. Title positions are
    /// calculated separately because text measurement remains an adapter
    /// input. Macintosh Toolbox Essentials (1992), pp. 3-111--3-112.
    pub(crate) fn from_regular_handles(
        mb_res_id: i16,
        handles: impl IntoIterator<Item = u32>,
    ) -> Self {
        Self {
            mb_res_id,
            regular: handles
                .into_iter()
                .take(MAX_MENU_LIST_ENTRIES)
                .map(|handle| MenuListEntry { handle, value: 0 })
                .collect(),
            ..Self::default()
        }
    }

    /// Decode a complete guest `DynamicMenuList` record.
    ///
    /// The record contains a six-byte header, six bytes per regular menu, a
    /// six-byte hierarchical header, and six bytes per hierarchical menu.
    /// Inside Macintosh Volume V (1986), pp. V-228--V-230.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        let regular_bytes = usize::from(read_u16(bytes, 0)?);
        if regular_bytes % 6 != 0 || regular_bytes / 6 > MAX_MENU_LIST_ENTRIES {
            return None;
        }
        let regular_count = regular_bytes / 6;
        let hierarchical_header = 6usize.checked_add(regular_bytes)?;
        let hierarchical_bytes = usize::from(read_u16(bytes, hierarchical_header)?);
        if hierarchical_bytes % 6 != 0 || hierarchical_bytes / 6 > MAX_MENU_LIST_ENTRIES {
            return None;
        }
        let hierarchical_count = hierarchical_bytes / 6;
        let hierarchical_start = hierarchical_header.checked_add(6)?;
        let required = hierarchical_start.checked_add(hierarchical_count.checked_mul(6)?)?;
        if required > bytes.len() {
            return None;
        }

        let mut regular = Vec::with_capacity(regular_count);
        for index in 0..regular_count {
            let offset = 6usize.checked_add(index.checked_mul(6)?)?;
            regular.push(MenuListEntry {
                handle: read_u32(bytes, offset)?,
                value: read_u16(bytes, offset + 4)? as i16,
            });
        }
        let mut hierarchical = Vec::with_capacity(hierarchical_count);
        for index in 0..hierarchical_count {
            let offset = hierarchical_start.checked_add(index.checked_mul(6)?)?;
            hierarchical.push(MenuListEntry {
                handle: read_u32(bytes, offset)?,
                value: read_u16(bytes, offset + 4)? as i16,
            });
        }

        Some(Self {
            last_right: read_u16(bytes, 2)? as i16,
            mb_res_id: read_u16(bytes, 4)? as i16,
            regular,
            menu_title_save: read_u32(bytes, hierarchical_header + 2)?,
            hierarchical,
        })
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let regular_count = self.regular.len().min(MAX_MENU_LIST_ENTRIES);
        let hierarchical_count = self.hierarchical.len().min(MAX_MENU_LIST_ENTRIES);
        let mut bytes = Vec::with_capacity(12 + 6 * (regular_count + hierarchical_count));
        bytes.extend_from_slice(&((regular_count * 6) as u16).to_be_bytes());
        bytes.extend_from_slice(&self.last_right.to_be_bytes());
        bytes.extend_from_slice(&self.mb_res_id.to_be_bytes());
        for entry in self.regular.iter().take(regular_count) {
            bytes.extend_from_slice(&entry.handle.to_be_bytes());
            bytes.extend_from_slice(&entry.value.to_be_bytes());
        }
        bytes.extend_from_slice(&((hierarchical_count * 6) as u16).to_be_bytes());
        bytes.extend_from_slice(&self.menu_title_save.to_be_bytes());
        for entry in self.hierarchical.iter().take(hierarchical_count) {
            bytes.extend_from_slice(&entry.handle.to_be_bytes());
            bytes.extend_from_slice(&entry.value.to_be_bytes());
        }
        bytes
    }

    pub(crate) fn regular_handles(&self) -> impl DoubleEndedIterator<Item = u32> + '_ {
        self.regular.iter().map(|entry| entry.handle)
    }

    /// Project the live current list and its MenuInfo records into the single
    /// immutable representation consumed by host frontends. CPU gateways
    /// provide only guest-memory reads. The regular/hierarchical partition,
    /// item classification, enable state, and Mac Roman conversion remain
    /// architecture neutral. Macintosh Toolbox Essentials (1992), pp.
    /// 3-95--3-97 and 3-112--3-113.
    pub(crate) fn guest_snapshot(
        &self,
        mut read_menu: impl FnMut(u32) -> Option<MenuSnapshotRecord>,
    ) -> GuestMenuSnapshot {
        let menus = self
            .regular_handles()
            .map(|handle| (handle, false))
            .chain(self.hierarchical_handles().map(|handle| (handle, true)))
            .filter_map(|(handle, hierarchical)| {
                let record = read_menu(handle)?;
                let title = if record.title == [0x14] {
                    "Systemless".to_owned()
                } else {
                    decode_mac_roman(&record.title)
                };
                let enabled = record.items.enable_flags & 1 != 0;
                let items = record
                    .items
                    .items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let number = i16::try_from(index + 1).unwrap_or(i16::MAX);
                        let submenu_id = hierarchical_menu_id(item.command, item.mark);
                        GuestMenuItem {
                            number,
                            text: decode_mac_roman(&item.text),
                            enabled: item.enabled,
                            checked: item.mark != 0 && submenu_id.is_none(),
                            key_equivalent: (item.command > 0x20)
                                .then(|| char::from(item.command).to_ascii_lowercase()),
                            submenu_id,
                            separator: item.text == b"-",
                        }
                    })
                    .collect();
                Some(GuestMenu {
                    id: record.id,
                    title,
                    enabled,
                    hierarchical,
                    visible_in_menu_bar: !hierarchical,
                    items,
                })
            })
            .collect();
        GuestMenuSnapshot { menus }
    }

    pub(crate) fn hierarchical_handles(&self) -> impl DoubleEndedIterator<Item = u32> + '_ {
        self.hierarchical.iter().map(|entry| entry.handle)
    }

    pub(crate) fn handles(&self) -> impl DoubleEndedIterator<Item = u32> + '_ {
        self.regular_handles().chain(self.hierarchical_handles())
    }

    pub(crate) fn contains_handle(&self, handle: u32) -> bool {
        self.handles().any(|candidate| candidate == handle)
    }

    /// Insert a menu into the regular or hierarchical partition.
    ///
    /// A regular menu is inserted before the first matching menu ID, or at
    /// the end when `before_id` is zero or absent. `before_id == -1` appends
    /// to the hierarchical partition. An existing handle and a full target
    /// partition are both no-ops. Macintosh Toolbox Essentials (1992),
    /// pp. 3-108--3-109.
    pub(crate) fn insert(
        &mut self,
        handle: u32,
        before_id: i16,
        mut menu_id: impl FnMut(u32) -> Option<i16>,
    ) -> bool {
        if self.contains_handle(handle) {
            return false;
        }
        let target = if before_id == -1 {
            &mut self.hierarchical
        } else {
            &mut self.regular
        };
        if target.len() >= MAX_MENU_LIST_ENTRIES {
            return false;
        }
        let insertion = if before_id <= 0 {
            target.len()
        } else {
            target
                .iter()
                .position(|candidate| menu_id(candidate.handle) == Some(before_id))
                .unwrap_or(target.len())
        };
        target.insert(insertion, MenuListEntry { handle, value: 0 });
        true
    }

    /// Delete the first menu with `menu_id`, searching hierarchical entries
    /// before regular entries so a desk-accessory submenu cannot delete an
    /// application menu with the same ID. Returns the removed handle.
    /// Inside Macintosh Volume V (1986), p. V-244; Macintosh Toolbox
    /// Essentials (1992), p. 3-109.
    pub(crate) fn remove_by_id(
        &mut self,
        requested_id: i16,
        mut menu_id: impl FnMut(u32) -> Option<i16>,
    ) -> Option<u32> {
        if let Some(index) = self
            .hierarchical
            .iter()
            .position(|candidate| menu_id(candidate.handle) == Some(requested_id))
        {
            return Some(self.hierarchical.remove(index).handle);
        }
        let index = self
            .regular
            .iter()
            .position(|candidate| menu_id(candidate.handle) == Some(requested_id))?;
        Some(self.regular.remove(index).handle)
    }

    /// Find a menu by ID, searching hierarchical entries before regular
    /// entries as required when their IDs collide. Inside Macintosh Volume V
    /// (1986), p. V-246.
    pub(crate) fn find_handle_by_id(
        &self,
        requested_id: i16,
        mut menu_id: impl FnMut(u32) -> Option<i16>,
    ) -> Option<u32> {
        self.find_hierarchical_handle_by_id(requested_id, &mut menu_id)
            .or_else(|| {
                self.regular_handles()
                    .find(|handle| menu_id(*handle) == Some(requested_id))
            })
    }

    /// Resolve a submenu only from the current list's hierarchical partition.
    /// `MenuSelect` searches this partition for the ID stored by the parent
    /// item. Macintosh Toolbox Essentials (1992), pp. 3-53--3-55.
    pub(crate) fn find_hierarchical_handle_by_id(
        &self,
        requested_id: i16,
        mut menu_id: impl FnMut(u32) -> Option<i16>,
    ) -> Option<u32> {
        self.hierarchical_handles()
            .find(|handle| menu_id(*handle) == Some(requested_id))
    }

    /// Resolve one parent item through the current list's hierarchical
    /// partition. The parent item and child list are both live guest records;
    /// an adapter supplies only child MenuInfo ID reads.
    pub(crate) fn submenu_handle_for_item(
        &self,
        parent_items: &MenuItems,
        parent_item: i16,
        menu_id: impl FnMut(u32) -> Option<i16>,
    ) -> Option<u32> {
        self.find_hierarchical_handle_by_id(parent_items.hierarchical_id(parent_item)?, menu_id)
    }

    /// Derive each regular menu title's half-open horizontal hit region from
    /// its stored `menuLeft` and the following `menuLeft` or `lastRight`.
    /// Macintosh Toolbox Essentials (1992), pp. 3-96--3-97.
    pub(crate) fn regular_title_regions(&self) -> Vec<(u32, i16, i16)> {
        self.regular
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let right = self
                    .regular
                    .get(index + 1)
                    .map(|next| next.value)
                    .unwrap_or(self.last_right);
                (entry.handle, entry.value, right)
            })
            .collect()
    }

    pub(crate) fn regular_title_at_horizontal(&self, horizontal: i16) -> Option<(u32, i16)> {
        self.regular_title_regions()
            .into_iter()
            .find(|(_handle, left, right)| horizontal >= *left && horizontal < *right)
            .map(|(handle, left, _right)| (handle, left))
    }

    /// Resolve a Command-key equivalent using the Menu Manager's partition
    /// and traversal order. Regular menus are searched right-to-left and
    /// top-to-bottom before the hierarchical portion. A hierarchical match
    /// retains the regular title that owns its installed path.
    /// Inside Macintosh Volume I (1985), p. I-356; Inside Macintosh Volume V
    /// (1986), pp. V-235 and V-245.
    pub(crate) fn menu_key_selection(
        &self,
        key: u8,
        mut decode_menu: impl FnMut(u32) -> Option<MenuKeyMenu>,
    ) -> Option<MenuKeySelection> {
        if key <= 0x20 {
            return None;
        }

        let regular = self
            .regular_handles()
            .filter_map(|handle| decode_menu(handle).map(|menu| (handle, menu)))
            .collect::<Vec<_>>();
        let hierarchical = self
            .hierarchical_handles()
            .filter_map(|handle| decode_menu(handle).map(|menu| (handle, menu)))
            .collect::<Vec<_>>();

        let (menu_handle, menu, item_number, hierarchical_match) = regular
            .iter()
            .rev()
            .filter_map(|(handle, menu)| {
                Self::menu_key_item(menu, key).map(|item| (*handle, menu, item, false))
            })
            .next()
            .or_else(|| {
                hierarchical.iter().find_map(|(handle, menu)| {
                    Self::menu_key_item(menu, key).map(|item| (*handle, menu, item, true))
                })
            })?;

        let owner_handle = if hierarchical_match {
            Self::owning_regular_handle(menu_handle, &regular, &hierarchical)
        } else {
            Some(menu_handle)
        };
        Some(MenuKeySelection {
            menu_handle,
            owner_handle,
            menu_id: menu.id,
            item_number,
        })
    }

    fn menu_key_item(menu: &MenuKeyMenu, key: u8) -> Option<i16> {
        if !menu.enabled {
            return None;
        }
        menu.items.iter().enumerate().find_map(|(index, item)| {
            (item.enabled && item.command > 0x20 && item.command.eq_ignore_ascii_case(&key))
                .then(|| i16::try_from(index + 1).ok())
                .flatten()
        })
    }

    fn owning_regular_handle(
        target_handle: u32,
        regular: &[(u32, MenuKeyMenu)],
        hierarchical: &[(u32, MenuKeyMenu)],
    ) -> Option<u32> {
        for (root_handle, _root) in regular {
            let mut pending = vec![*root_handle];
            let mut visited = Vec::new();
            while let Some(handle) = pending.pop() {
                if handle == target_handle {
                    return Some(*root_handle);
                }
                if visited.contains(&handle) {
                    continue;
                }
                visited.push(handle);
                let menu = regular
                    .iter()
                    .chain(hierarchical.iter())
                    .find(|(candidate, _menu)| *candidate == handle)
                    .map(|(_handle, menu)| menu)?;
                for submenu_id in menu
                    .items
                    .iter()
                    .filter_map(|item| hierarchical_menu_id(item.command, item.mark))
                {
                    if let Some((child_handle, _child)) = hierarchical
                        .iter()
                        .find(|(_handle, child)| child.id == submenu_id)
                    {
                        pending.push(*child_handle);
                    }
                }
            }
        }
        None
    }

    /// Remove every menu while retaining the menu-bar resource identity.
    /// Menu records themselves remain allocated. Macintosh Toolbox
    /// Essentials (1992), p. 3-110.
    pub(crate) fn clear_entries(&mut self) {
        self.last_right = 0;
        self.regular.clear();
        self.menu_title_save = 0;
        self.hierarchical.clear();
    }

    /// Recompute the regular partition's title hit positions. Text
    /// measurement remains a presentation-adapter input; list ordering,
    /// spacing, and `lastRight` mutation are one manager operation.
    pub(crate) fn relayout_regular_titles(
        &mut self,
        first_left: i16,
        spacing: i16,
        mut title_advance: impl FnMut(u32) -> i16,
    ) {
        let mut title_left = first_left;
        for entry in &mut self.regular {
            entry.value = title_left;
            title_left = title_left
                .saturating_add(title_advance(entry.handle).max(0))
                .saturating_add(spacing.max(0));
        }
        self.last_right = if self.regular.is_empty() {
            0
        } else {
            title_left
        };
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_menu_record_uses_the_shared_menu_info_layout() {
        let record = new_standard_menu_record(-128, 0x1234_5678, b"File");

        assert_eq!(&record[0..2], &(-128i16).to_be_bytes());
        assert_eq!(&record[2..6], &[0, 0, 0, 0]);
        assert_eq!(&record[6..10], &0x1234_5678u32.to_be_bytes());
        assert_eq!(&record[10..14], &u32::MAX.to_be_bytes());
        assert_eq!(&record[14..], b"\x04File\0");
        assert_eq!(MenuItems::decode(&record).unwrap().item_count(), 0);
    }

    #[test]
    fn new_menu_record_limits_titles_to_pascal_string_capacity() {
        let record = new_standard_menu_record(128, 0, &[b'A'; 300]);

        assert_eq!(record.len(), 271);
        assert_eq!(record[14], 255);
        assert_eq!(record.last(), Some(&0));
        assert_eq!(MenuItems::decode(&record).unwrap().item_count(), 0);
    }

    fn tracking_with_child<MenuRef: Copy>(
        root: MenuRef,
        child: MenuRef,
    ) -> MenuTrackingState<MenuRef, (), u8, ()> {
        MenuTrackingState {
            kind: MenuTrackingKind::MenuBar,
            menu_handle: root,
            popup_left: 0,
            popup_top: 20,
            content_top: 20,
            scroll_direction: None,
            popup_width: 100,
            popup_height: 40,
            highlighted_item: 1,
            flash_remaining: 0,
            flash_delay: 0,
            flash_result: 0,
            saved_width: 100,
            saved_height: 40,
            front_buffer: (),
            saved_pixels: Vec::new(),
            item_appearances: Vec::new(),
            submenus: vec![TrackedMenuPane {
                parent_item: 1,
                menu_handle: child,
                popup_left: 99,
                popup_top: 22,
                content_top: 22,
                scroll_direction: None,
                popup_width: 100,
                popup_height: 40,
                highlighted_item: 2,
                saved_width: 100,
                saved_height: 40,
                front_buffer: (),
                saved_pixels: Vec::new(),
                item_appearances: Vec::new(),
            }],
        }
    }

    #[test]
    fn menu_list_round_trip_preserves_both_partitions() {
        let expected = MenuList {
            last_right: 91,
            mb_res_id: 128,
            regular: vec![MenuListEntry {
                handle: 0x1234_5678,
                value: 17,
            }],
            menu_title_save: 0x89ab_cdef,
            hierarchical: vec![MenuListEntry {
                handle: 0x8765_4321,
                value: -1,
            }],
        };
        assert_eq!(MenuList::decode(&expected.encode()), Some(expected));
    }

    #[test]
    fn menu_bar_resource_and_list_construction_preserve_order_and_identity() {
        let resource =
            MenuBarResource::decode(&[0, 3, 0, 128, 0xFF, 0xFF, 1, 44]).expect("decode MBAR");
        assert_eq!(resource.menu_ids, vec![128, -1, 300]);
        assert_eq!(MenuBarResource::decode(&[0, 2, 0, 128]), None);

        let handles = resource.load_regular_handles(|menu_id| match menu_id {
            128 => Some(0x1000),
            -1 => None,
            300 => Some(0x3000),
            _ => unreachable!(),
        });
        let list = MenuList::from_regular_handles(900, handles);
        assert_eq!(list.mb_res_id, 900);
        assert_eq!(
            list.regular_handles().collect::<Vec<_>>(),
            vec![0x1000, 0x3000]
        );
        assert!(list.hierarchical.is_empty());
    }

    #[test]
    fn guest_snapshot_projection_is_shared_across_menu_list_partitions() {
        let list = MenuList {
            regular: vec![MenuListEntry {
                handle: 0x1000,
                value: 11,
            }],
            hierarchical: vec![MenuListEntry {
                handle: 0x2000,
                value: 0,
            }],
            ..MenuList::default()
        };
        let snapshot = list.guest_snapshot(|handle| {
            Some(MenuSnapshotRecord {
                id: if handle == 0x1000 { 128 } else { 200 },
                title: if handle == 0x1000 {
                    vec![0x14]
                } else {
                    b"Recent".to_vec()
                },
                items: MenuItems {
                    first_item: 0,
                    enable_flags: if handle == 0x1000 { u32::MAX } else { 0 },
                    items: vec![
                        MenuItem {
                            text: if handle == 0x1000 {
                                b"Recent".to_vec()
                            } else {
                                b"Document".to_vec()
                            },
                            icon: 0,
                            command: if handle == 0x1000 { 0x1b } else { b'D' },
                            mark: if handle == 0x1000 { 200 } else { 0x12 },
                            style: 0,
                            enabled: handle == 0x1000,
                        },
                        MenuItem {
                            text: b"Icon".to_vec(),
                            icon: 1,
                            command: 0x1d,
                            mark: 0,
                            style: 0,
                            enabled: true,
                        },
                    ],
                },
            })
        });

        assert_eq!(snapshot.menus.len(), 2);
        assert_eq!(snapshot.menus[0].title, "Systemless");
        assert!(snapshot.menus[0].visible_in_menu_bar);
        assert_eq!(snapshot.menus[0].items[0].submenu_id, Some(200));
        assert!(!snapshot.menus[0].items[0].checked);
        assert!(snapshot.menus[1].hierarchical);
        assert!(!snapshot.menus[1].visible_in_menu_bar);
        assert!(!snapshot.menus[1].enabled);
        assert_eq!(snapshot.menus[1].items[0].key_equivalent, Some('d'));
        assert!(snapshot.menus[1].items[0].checked);
        assert_eq!(snapshot.menus[1].items[1].key_equivalent, None);
    }

    #[test]
    fn menu_key_search_and_hierarchy_ownership_are_shared() {
        let list = MenuList {
            regular: vec![
                MenuListEntry {
                    handle: 10,
                    value: 11,
                },
                MenuListEntry {
                    handle: 20,
                    value: 44,
                },
            ],
            hierarchical: vec![
                MenuListEntry {
                    handle: 30,
                    value: 0,
                },
                MenuListEntry {
                    handle: 40,
                    value: 0,
                },
            ],
            ..MenuList::default()
        };
        let decode = |handle| {
            let (id, enabled, items) = match handle {
                10 => (
                    10,
                    true,
                    vec![
                        MenuKeyItem {
                            command: b'X',
                            mark: 0,
                            enabled: true,
                        },
                        MenuKeyItem {
                            command: 0x1b,
                            mark: 40,
                            enabled: true,
                        },
                    ],
                ),
                20 => (
                    20,
                    true,
                    vec![MenuKeyItem {
                        command: b'x',
                        mark: 0,
                        enabled: true,
                    }],
                ),
                30 => (
                    30,
                    true,
                    vec![MenuKeyItem {
                        command: b'U',
                        mark: 0,
                        enabled: true,
                    }],
                ),
                40 => (
                    40,
                    true,
                    vec![MenuKeyItem {
                        command: b'H',
                        mark: 0,
                        enabled: true,
                    }],
                ),
                _ => return None,
            };
            Some(MenuKeyMenu { id, enabled, items })
        };

        let regular = list.menu_key_selection(b'X', decode).unwrap();
        assert_eq!(regular.menu_handle, 20, "rightmost regular menu wins");
        assert_eq!(regular.owner_handle, Some(20));
        assert_eq!(regular.packed_result(), (20 << 16) | 1);

        let attached = list.menu_key_selection(b'h', decode).unwrap();
        assert_eq!(attached.menu_handle, 40);
        assert_eq!(attached.owner_handle, Some(10));
        assert_eq!(attached.packed_result(), (40 << 16) | 1);

        let unattached = list.menu_key_selection(b'U', decode).unwrap();
        assert_eq!(unattached.menu_handle, 30);
        assert_eq!(unattached.owner_handle, None);
        assert_eq!(
            list.menu_key_selection(b'X', |handle| {
                let mut menu = decode(handle)?;
                menu.enabled = handle != 20;
                if handle == 10 {
                    menu.items[0].enabled = false;
                }
                Some(menu)
            }),
            None,
            "disabled menus and items cannot provide a command match"
        );
        assert_eq!(list.menu_key_selection(0x1b, decode), None);
    }

    #[test]
    fn menu_items_round_trip_preserves_guest_attributes() {
        let mut bytes = vec![0; 15];
        bytes[10..14].copy_from_slice(&u32::MAX.to_be_bytes());
        bytes[14] = 0;
        bytes.extend_from_slice(&[4, b'O', b'p', b'e', b'n', 3, b'O', 0x12, 1, 0]);
        let decoded = MenuItems::decode(&bytes).expect("decode menu items");
        assert_eq!(decoded.items.len(), 1);
        assert_eq!(decoded.item_count(), 1);
        assert_eq!(
            MenuItems::decode_with(|offset| bytes.get(offset).copied()),
            Some(decoded.clone())
        );
        assert_eq!(decoded.rebuild(&bytes).as_deref(), Some(bytes.as_slice()));
        bytes.pop();
        assert_eq!(MenuItems::decode(&bytes), None);
    }

    #[test]
    fn submenu_resolution_uses_live_parent_items_and_hierarchical_partition() {
        let list = MenuList {
            regular: vec![MenuListEntry {
                handle: 20,
                value: 0,
            }],
            hierarchical: vec![MenuListEntry {
                handle: 30,
                value: 0,
            }],
            ..MenuList::default()
        };
        let parent = MenuItems {
            first_item: 15,
            enable_flags: u32::MAX,
            items: vec![MenuItem {
                text: b"Child".to_vec(),
                icon: 0,
                command: 0x1B,
                mark: 30,
                style: 0,
                enabled: true,
            }],
        };
        let menu_id = |handle| match handle {
            20 | 30 => Some(30),
            _ => None,
        };

        assert_eq!(list.submenu_handle_for_item(&parent, 1, menu_id), Some(30));
        assert!(parent.item_is_hierarchical(1));

        let mut ordinary = parent.clone();
        ordinary.items[0].command = b'C';
        assert_eq!(list.submenu_handle_for_item(&ordinary, 1, menu_id), None);
        assert!(!ordinary.item_is_hierarchical(1));
    }

    #[test]
    fn item_specs_decode_documented_separators_metacharacters_and_icon_digits() {
        let items = parse_menu_item_specs(b"(Everything^2!=<B<I/E\rQuit/Q;Parent<BI");

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, b"Everything");
        assert!(!items[0].enabled);
        assert_eq!(items[0].icon, 2);
        assert_eq!(items[0].mark, b'=');
        assert_eq!(items[0].style, 0x03);
        assert_eq!(items[0].command, b'E');
        assert_eq!(items[1].text, b"Quit");
        assert_eq!(items[1].command, b'Q');
        assert_eq!(items[2].text, b"ParentI");
        assert_eq!(items[2].style, 0x01);
        assert_eq!(hierarchical_menu_id(0x1B, 200), Some(200));
        assert_eq!(hierarchical_menu_id(0x1B, 0), None);
        assert_eq!(hierarchical_menu_id(b'Q', 200), None);
    }

    #[test]
    fn standard_menu_width_matches_both_macos_8_1_profiles() {
        let width = |text, icon, command| StandardMenuItemWidth {
            text,
            icon,
            command,
        };
        assert_eq!(standard_menu_width([width(31, 0, 0)]), 63);
        assert_eq!(standard_menu_width([width(24, 0, 0)]), 56);
        assert_eq!(standard_menu_width([width(43, 0, 0)]), 75);
        assert_eq!(standard_menu_width([width(37, 0, 0x1B)]), 101);
        assert_eq!(standard_menu_width([width(21, 0, b'L')]), 85);
        assert_eq!(standard_menu_width([width(48, 0, 0)]), 80);
        assert_eq!(standard_menu_width([width(10, 16, 0)]), 58);
        assert_eq!(standard_menu_width([]), 32);
    }

    #[test]
    fn shared_item_mutations_rebuild_one_architecture_independent_record() {
        let mut original = vec![0; 15];
        original[10..14].copy_from_slice(&u32::MAX.to_be_bytes());
        original.extend_from_slice(&[0]);

        let mut items = MenuItems::decode(&original).expect("decode empty menu");
        assert!(items.append_specs(b"Open/O;Quit/Q"));
        assert!(items.insert_specs(b"Cut/X;Copy/C", 1));
        assert!(items.set_text(1, b"Open..."));
        assert!(items.set_icon(2, 7));
        assert!(items.set_command(2, b'K'));
        assert!(items.set_mark(2, b'='));
        assert!(items.set_style(2, 0x03));
        assert!(items.set_enabled(3, false));
        assert!(items.delete(4));

        let rebuilt = items.rebuild(&original).expect("rebuild menu");
        let decoded = MenuItems::decode(&rebuilt).expect("decode rebuilt menu");
        assert_eq!(decoded.first_item, items.first_item);
        assert_eq!(decoded.items, items.items);
        assert_eq!(decoded.enable_flags, u32::MAX & !(1 << 3));
        assert_eq!(decoded.items.len(), 3);
        assert_eq!(decoded.items[0].text, b"Open...");
        assert_eq!(decoded.items[1].text, b"Copy");
        assert_eq!(decoded.items[1].icon, 7);
        assert_eq!(decoded.items[1].command, b'K');
        assert_eq!(decoded.items[1].mark, b'=');
        assert_eq!(decoded.items[1].style, 0x03);
        assert!(!decoded.items[2].enabled);
    }

    #[test]
    fn menu_rows_share_offsets_boundaries_and_selectability_across_chrome_insets() {
        assert_eq!(standard_menu_row_height(None, false, false), 16);
        assert_eq!(standard_menu_row_height(None, true, false), 34);
        assert_eq!(standard_menu_row_height(Some(25), true, false), 25);
        assert_eq!(standard_menu_row_height(None, false, true), 21);
        assert_eq!(
            laid_out_menu_item_count(&[false, true, true], |separator| *separator),
            1
        );

        let rows = MenuRows::new([
            MenuRow {
                height: 16,
                selectable: true,
            },
            MenuRow {
                height: 34,
                selectable: false,
            },
            MenuRow {
                height: 21,
                selectable: true,
            },
        ]);

        assert_eq!(rows.total_height(), 71);
        assert_eq!(rows.offset(1), 0);
        assert_eq!(rows.offset(2), 16);
        assert_eq!(rows.offset(3), 50);
        assert_eq!(rows.item_at_offset(15), Some(1));
        assert_eq!(rows.item_at_offset(16), Some(0));
        assert_eq!(rows.item_at_offset(49), Some(0));
        assert_eq!(rows.item_at_offset(50), Some(3));
        assert_eq!(rows.item_at_offset(71), None);

        let rect = (20, 10, 95, 112);
        assert_eq!(rows.item_at_point(rect, (1, 0, 3, 0), (21, 10)), Some(1));
        assert_eq!(rows.item_at_point(rect, (1, 0, 3, 0), (37, 10)), Some(0));
        assert_eq!(rows.item_at_point(rect, (2, 1, 2, 1), (22, 11)), Some(1));
        assert_eq!(rows.item_at_point(rect, (2, 1, 2, 1), (21, 11)), Some(0));
        assert_eq!(rows.item_at_point(rect, (2, 1, 2, 1), (22, 10)), Some(0));
        assert_eq!(rows.item_at_point(rect, (2, 1, 2, 1), (19, 11)), None);
    }

    #[test]
    fn popup_layout_matches_macos_81_standard_mdef_geometry() {
        let plain = MenuRows::new((0..3).map(|_| MenuRow {
            height: 16,
            selectable: true,
        }));
        for (item, top) in [(1, 100), (2, 84), (3, 68)] {
            let layout = standard_popup_menu_layout(&plain, 75, (800, 600), (100, 120), item)
                .expect("plain popup layout");
            assert_eq!(layout.rect(), (top, 120, top + 48, 195));
            assert_eq!(layout.highlighted_item, item);
            assert_eq!(layout.content_top, top);
        }

        let separator = MenuRows::new([
            MenuRow {
                height: 16,
                selectable: true,
            },
            MenuRow {
                height: STANDARD_MENU_SEPARATOR_HEIGHT,
                selectable: false,
            },
            MenuRow {
                height: 16,
                selectable: true,
            },
        ]);
        let layout = standard_popup_menu_layout(&separator, 63, (800, 600), (100, 120), 3)
            .expect("separator popup layout");
        assert_eq!(layout.rect(), (78, 120, 116, 183));
        assert_eq!(layout.highlighted_item, 3);

        let clamped = standard_popup_menu_layout(&plain, 75, (240, 160), (150, 220), 3)
            .expect("clamped popup layout");
        assert_eq!(clamped.rect(), (111, 161, 159, 236));
        assert_eq!(clamped.highlighted_item, 3);
    }

    #[test]
    fn scrolling_popup_layout_matches_macos_81_profile_captures() {
        let rows = MenuRows::new((0..40).map(|_| MenuRow {
            height: 16,
            selectable: true,
        }));
        assert_eq!(standard_menu_height(&rows, 580), 560);
        for (item, anchor_top, expected_rect, content_top) in [
            (1, 100, (4, 120, 580, 155), 100),
            (20, 100, (4, 120, 580, 155), -204),
            (40, 100, (4, 120, 580, 155), -524),
            (1, 10, (10, 120, 586, 155), 10),
            (20, 300, (12, 120, 588, 155), -4),
            (40, 590, (14, 120, 590, 155), -34),
        ] {
            let layout = standard_popup_menu_layout(&rows, 35, (800, 600), (anchor_top, 120), item)
                .expect("scrolling popup layout");
            assert_eq!(layout.rect(), expected_rect);
            assert_eq!(layout.content_top, content_top);
            assert_eq!(layout.highlighted_item, item);
        }
    }

    #[test]
    fn scrolling_pointer_lifecycle_matches_both_macos_81_mdefs() {
        let rows = MenuRows::new((0..40).map(|_| MenuRow {
            height: 16,
            selectable: true,
        }));
        let rect = (4, 120, 580, 155);
        let mut armed = None;
        let mut content_top = -204;

        assert_eq!(rows.scroll_indicators(rect, content_top), (true, false));
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (28, 130)),
            MenuPointerUpdate {
                item: 15,
                scrolled: false
            }
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (12, 130)),
            MenuPointerUpdate {
                item: 0,
                scrolled: false
            }
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (12, 130)),
            MenuPointerUpdate {
                item: 0,
                scrolled: true
            }
        );
        assert_eq!(
            (content_top, content_top + rows.total_height()),
            (-188, 452)
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (3, 130)),
            MenuPointerUpdate {
                item: 0,
                scrolled: true
            }
        );
        assert_eq!(
            (content_top, content_top + rows.total_height()),
            (-172, 468)
        );

        armed = None;
        content_top = 100;
        assert_eq!(rows.scroll_indicators(rect, content_top), (false, true));
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (556, 130)),
            MenuPointerUpdate {
                item: 29,
                scrolled: false
            }
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (572, 130)),
            MenuPointerUpdate {
                item: 0,
                scrolled: false
            }
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (572, 130)),
            MenuPointerUpdate {
                item: 0,
                scrolled: true
            }
        );
        assert_eq!((content_top, content_top + rows.total_height()), (84, 724));
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (581, 130)),
            MenuPointerUpdate {
                item: 0,
                scrolled: true
            }
        );
        assert_eq!((content_top, content_top + rows.total_height()), (68, 708));
    }

    #[test]
    fn pull_down_and_submenu_layout_match_macos_81_standard_mbdf_geometry() {
        let pull_down =
            standard_pull_down_menu_layout(90, 70, (800, 600), 10, 20).expect("ordinary pull-down");
        assert_eq!(pull_down.rect(), (20, 10, 90, 100));

        let right_edge = standard_pull_down_menu_layout(90, 70, (800, 600), 760, 20)
            .expect("right-edge pull-down");
        assert_eq!(right_edge.rect(), (20, 702, 90, 792));

        let root = pull_down.rect();
        assert_eq!(
            standard_submenu_layout(root, 0, 75, 48, (800, 600), 20)
                .expect("first child")
                .rect(),
            (27, 96, 75, 171),
        );
        assert_eq!(
            standard_submenu_layout(root, 38, 75, 48, (800, 600), 20)
                .expect("lower child")
                .rect(),
            (58, 96, 106, 171),
        );

        let right_root = right_edge.rect();
        assert_eq!(
            standard_submenu_layout(right_root, 0, 75, 48, (800, 600), 20)
                .expect("left-opening first child")
                .rect(),
            (27, 635, 75, 710),
        );
        assert_eq!(
            standard_submenu_layout(right_root, 38, 75, 48, (800, 600), 20)
                .expect("left-opening lower child")
                .rect(),
            (58, 635, 106, 710),
        );

        let popup_parent = (100, 120, 170, 210);
        assert_eq!(
            standard_submenu_layout(popup_parent, 0, 75, 48, (800, 600), 20)
                .expect("popup-parent first child")
                .rect(),
            (100, 206, 148, 281),
        );
        assert_eq!(
            standard_submenu_layout(popup_parent, 38, 75, 48, (800, 600), 20)
                .expect("popup-parent lower child")
                .rect(),
            (138, 206, 186, 281),
        );
        assert_eq!(
            standard_submenu_layout((520, 120, 590, 210), 38, 75, 48, (800, 600), 20)
                .expect("bottom-edge child")
                .rect(),
            (540, 206, 588, 281),
        );
    }

    #[test]
    fn menu_list_insert_and_delete_policy_is_shared_between_gateways() {
        let ids = |handle| match handle {
            10 => Some(100),
            20 => Some(200),
            30 => Some(300),
            40 => Some(200),
            _ => None,
        };
        let mut list = MenuList::default();

        assert!(list.insert(10, 0, ids));
        assert!(list.insert(30, 0, ids));
        assert!(list.insert(20, 300, ids));
        assert!(!list.insert(20, 0, ids));
        assert!(list.insert(40, -1, ids));
        assert_eq!(list.regular_handles().collect::<Vec<_>>(), vec![10, 20, 30]);
        assert_eq!(list.hierarchical_handles().collect::<Vec<_>>(), vec![40]);
        assert_eq!(list.find_handle_by_id(200, ids), Some(40));
        assert_eq!(list.find_hierarchical_handle_by_id(200, ids), Some(40));
        list.relayout_regular_titles(11, 13, |handle| match handle {
            10 => 20,
            20 => 30,
            30 => 40,
            _ => 0,
        });
        assert_eq!(
            list.regular
                .iter()
                .map(|entry| entry.value)
                .collect::<Vec<_>>(),
            vec![11, 44, 87]
        );
        assert_eq!(list.last_right, 140);
        assert_eq!(
            list.regular_title_regions(),
            vec![(10, 11, 44), (20, 44, 87), (30, 87, 140)]
        );
        assert_eq!(list.regular_title_at_horizontal(10), None);
        assert_eq!(list.regular_title_at_horizontal(11), Some((10, 11)));
        assert_eq!(list.regular_title_at_horizontal(86), Some((20, 44)));
        assert_eq!(list.regular_title_at_horizontal(139), Some((30, 87)));
        assert_eq!(list.regular_title_at_horizontal(140), None);

        assert_eq!(list.remove_by_id(200, ids), Some(40));
        assert_eq!(list.regular_handles().collect::<Vec<_>>(), vec![10, 20, 30]);
        assert_eq!(list.find_handle_by_id(200, ids), Some(20));
        assert_eq!(list.find_hierarchical_handle_by_id(200, ids), None);
        assert_eq!(list.remove_by_id(200, ids), Some(20));
        assert_eq!(list.remove_by_id(999, ids), None);
        list.mb_res_id = 128;
        list.clear_entries();
        assert_eq!(
            list,
            MenuList {
                mb_res_id: 128,
                ..MenuList::default()
            }
        );
    }

    #[test]
    fn malformed_partition_lengths_are_rejected() {
        let mut bytes = MenuList::default().encode();
        bytes[0..2].copy_from_slice(&5u16.to_be_bytes());
        assert_eq!(MenuList::decode(&bytes), None);
    }

    #[test]
    fn both_architecture_reference_types_select_the_deepest_terminal_item() {
        let classic = tracking_with_child(0usize, 1usize);
        let powerpc = tracking_with_child(0x1000u32, 0x2000u32);

        assert_eq!(classic.selection(|_, _| true), Some((1, 2)));
        assert_eq!(powerpc.selection(|_, _| true), Some((0x2000, 2)));
        assert_eq!(classic.selection(|_, item| item != 2), Some((0, 1)));
        assert_eq!(powerpc.selection(|_, item| item != 2), Some((0x1000, 1)));
    }

    #[test]
    fn both_architecture_reference_types_share_hierarchy_transitions() {
        let mut classic = tracking_with_child(0usize, 1usize);
        assert!(matches!(
            classic.prepare_submenu(0, 1, 1),
            SubmenuTransition::Keep
        ));
        assert!(matches!(
            classic.prepare_submenu(1, 1, 0),
            SubmenuTransition::Reject(closed) if closed.is_empty()
        ));
        assert!(matches!(
            classic.prepare_submenu(1, 2, 2),
            SubmenuTransition::Open(closed) if closed.is_empty()
        ));
        classic.submenus.push(TrackedMenuPane {
            menu_handle: 2,
            parent_item: 2,
            ..classic.submenus[0].clone()
        });
        assert_eq!(
            classic.deepest_submenu_hit(|depth, _| Some(depth as i16 + 1)),
            Some((1, 2))
        );
        let closed = classic.update_highlight(Some(0), 0).unwrap();
        assert_eq!(
            closed
                .iter()
                .map(|pane| pane.menu_handle)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert_eq!(classic.submenus[0].highlighted_item, 0);
        let closed = classic.update_highlight(None, 0).unwrap();
        assert_eq!(
            closed
                .iter()
                .map(|pane| pane.menu_handle)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(classic.highlighted_item, 0);

        let mut powerpc = tracking_with_child(0x1000u32, 0x2000u32);
        assert!(matches!(
            powerpc.prepare_submenu(0, 1, 0x2000),
            SubmenuTransition::Keep
        ));
        assert!(matches!(
            powerpc.prepare_submenu(1, 1, 0x1000),
            SubmenuTransition::Reject(closed) if closed.is_empty()
        ));
        assert!(matches!(
            powerpc.prepare_submenu(1, 2, 0x3000),
            SubmenuTransition::Open(closed) if closed.is_empty()
        ));
        powerpc.submenus.push(TrackedMenuPane {
            menu_handle: 0x3000,
            parent_item: 2,
            ..powerpc.submenus[0].clone()
        });
        assert_eq!(
            powerpc.deepest_submenu_hit(|depth, _| Some(depth as i16 + 1)),
            Some((1, 2))
        );
        let closed = powerpc.update_highlight(Some(0), 0).unwrap();
        assert_eq!(
            closed
                .iter()
                .map(|pane| pane.menu_handle)
                .collect::<Vec<_>>(),
            vec![0x3000]
        );
        assert_eq!(powerpc.submenus[0].highlighted_item, 0);
        let closed = powerpc.update_highlight(None, 0).unwrap();
        assert_eq!(
            closed
                .iter()
                .map(|pane| pane.menu_handle)
                .collect::<Vec<_>>(),
            vec![0x2000]
        );
        assert_eq!(powerpc.highlighted_item, 0);
    }
}
