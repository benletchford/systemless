//! Architecture-neutral Menu Manager records and list operations.

use crate::mac_roman::decode_mac_roman;
use crate::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};
use crate::quickdraw::text::{get_glyph, QuickDrawTextStyle};
use std::cell::{RefCell, UnsafeCell};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

/// Largest entry count representable by a menu-list partition byte length.
pub(crate) const MAX_MENU_LIST_ENTRIES: usize = u16::MAX as usize / 6;

/// Size of one guest `MCEntry` in a live menu color information table.
pub(crate) const MENU_COLOR_ENTRY_SIZE: usize = 30;

/// Convert the guest-visible `MenuFlash` count to alternating visible/hidden
/// phases. A count of zero disables blinking; every positive count contributes
/// one unhighlight and one rehighlight phase. Macintosh Toolbox Essentials
/// (1992), p. 3-142; Inside Macintosh Volume I (1985), p. I-366.
pub(crate) fn menu_flash_phase_count(count: u16) -> u32 {
    u32::from(count) * 2
}

pub(crate) const STANDARD_MENU_FLASH_PHASE_DELAY: u8 = 3;

/// Horizontal origin of the first standard menu title's logical hit cell.
pub(crate) const STANDARD_MENU_BAR_FIRST_TITLE_LEFT: i16 = 11;

/// Horizontal distance added after each standard menu title's glyph advance.
pub(crate) const STANDARD_MENU_BAR_TITLE_SPACING: i16 = 13;

/// Horizontal inset from a standard title's logical hit cell to its ink.
pub(crate) const STANDARD_MENU_BAR_TITLE_ORIGIN_INSET: i16 = 7;

/// Reference glyph advance reserved for the standard system-menu mark.
pub(crate) const STANDARD_SYSTEM_MENU_MARK_ADVANCE: i16 = 11;

/// Return whether a menu title is the one-byte system-menu mark.
///
/// The compiled Menu Manager form is `appleMark` (`$14`). The Mac Roman
/// Apple-logo byte (`$F0`) is accepted as the equivalent decoded spelling so
/// host-created records retain the same layout. Macintosh Toolbox Essentials
/// (1992), pp. 3-10 and 3-43--3-44.
pub(crate) fn is_standard_system_menu_title(title: &[u8]) -> bool {
    matches!(title, [0x14] | [0xF0])
}

/// Measure standard menu text in the Roman system font and size.
///
/// Menu titles and ordinary item text use the system font at the system size;
/// the frozen Roman Mac OS 8.1 profiles use font 0 at 12 points. Keeping this
/// byte-oriented also preserves the MENU record's Mac Roman identity across
/// both CPU gateways. Macintosh Toolbox Essentials (1992), pp. 3-10--3-13.
pub(crate) fn standard_menu_text_advance(text: &[u8]) -> i16 {
    let advance = text.iter().fold(0i32, |advance, byte| {
        advance.saturating_add(
            get_glyph(0, 12, char::from(*byte))
                .map(|(glyph, _)| i32::from(glyph.advance))
                .unwrap_or(6),
        )
    });
    i16::try_from(advance).unwrap_or(i16::MAX)
}

/// Measure one standard menu-bar title.
///
/// The system-menu artwork retains the captured 11-pixel logical advance;
/// every text title uses the same system-font measurement as menu items.
pub(crate) fn standard_menu_title_advance(title: &[u8]) -> i16 {
    if is_standard_system_menu_title(title) {
        STANDARD_SYSTEM_MENU_MARK_ADVANCE
    } else {
        standard_menu_text_advance(title)
    }
}

const MENU_COLOR_END_ID: i16 = -99;

/// Callable 68k Pascal-procedure shim used for the standard system MDEF.
///
/// The Menu Manager implements the standard drawing semantics in Rust, but a
/// guest can still dereference and call the resource handle installed in
/// `MenuInfo.menuProc`. The shim recovers the JSR return address, discards the
/// five-parameter 18-byte MDEF frame, and returns to the caller. Inside
/// Macintosh Volume I (1985), pp. I-352 and I-365.
pub(crate) const STANDARD_MENU_DEFINITION_SHIM: [u8; 8] = [
    0x20, 0x5f, // MOVEA.L (SP)+,A0 -- recover JSR return PC.
    0xde, 0xfc, 0x00, 0x12, // ADDA.W #18,SP -- discard MDEF parameters.
    0x4e, 0xd0, // JMP (A0).
];

/// Operation selector passed to a menu definition procedure.
///
/// These values and the five-parameter procedure contract are common to the
/// 68k Pascal ABI and native PowerPC routine descriptors. Macintosh Toolbox
/// Essentials (1992), pp. 3-148--3-151.
#[repr(i16)]
#[allow(dead_code)] // The retained dispatch slices migrate these messages incrementally.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuDefinitionMessage {
    Draw = 0,
    Choose = 1,
    Size = 2,
    PopUp = 3,
}

/// Architecture-neutral values supplied to one MDEF invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuDefinitionCall {
    pub(crate) message: MenuDefinitionMessage,
    pub(crate) menu_handle: u32,
    pub(crate) menu_rect: u32,
    pub(crate) hit_point: u32,
    pub(crate) which_item: u32,
}

impl MenuDefinitionCall {
    /// Native PowerPC passes the same logical arguments in declaration order.
    pub(crate) fn native_arguments(self) -> [u32; 5] {
        [
            self.message as i16 as u16 as u32,
            self.menu_handle,
            self.menu_rect,
            self.hit_point,
            self.which_item,
        ]
    }
}

/// Typed inputs and by-reference storage for one MDEF invocation.
///
/// `menuRect` and `whichItem` are passed by reference by both supported guest
/// ABIs. Keeping their initial bytes beside the logical values makes the
/// architecture adapters responsible only for allocating guest storage and
/// installing the resulting five arguments. Macintosh Toolbox Essentials
/// (1992), pp. 3-148--3-151.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuDefinitionInvocation {
    pub(crate) message: MenuDefinitionMessage,
    pub(crate) menu_handle: u32,
    pub(crate) menu_rect: (i16, i16, i16, i16),
    pub(crate) hit_point: u32,
    pub(crate) which_item: i16,
}

/// Result copied back from the two by-reference MDEF arguments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuDefinitionResult {
    pub(crate) menu_rect: (i16, i16, i16, i16),
    pub(crate) which_item: i16,
}

/// One architecture-neutral step while `GetNewMBar` sizes its menus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuBarBuildStep<Handle> {
    Size(Handle),
    Complete(Handle),
}

/// Retained `GetNewMBar` progress while custom MDEFs receive `mSizeMsg`.
///
/// The Menu Manager creates each menu in MBAR order and returns the completed
/// menu-list Handle only after those records have been sized. The CPU adapters
/// retain only their parked ABI frames. Inside Macintosh Volume I (1985),
/// pp. I-354 and I-365--I-366; Macintosh Toolbox Essentials (1992),
/// pp. 3-43 and 3-148--3-151.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MenuBarBuild<Handle> {
    result_handle: Option<Handle>,
    menu_handles: Vec<Handle>,
    next_menu: usize,
}

impl<Handle: Copy> MenuBarBuild<Handle> {
    pub(crate) fn new(result_handle: Handle, menu_handles: Vec<Handle>) -> Self {
        Self {
            result_handle: Some(result_handle),
            menu_handles,
            next_menu: 0,
        }
    }

    pub(crate) fn next_step(&mut self) -> Option<MenuBarBuildStep<Handle>> {
        if let Some(handle) = self.menu_handles.get(self.next_menu).copied() {
            self.next_menu += 1;
            Some(MenuBarBuildStep::Size(handle))
        } else {
            self.result_handle.take().map(MenuBarBuildStep::Complete)
        }
    }
}

impl MenuDefinitionInvocation {
    pub(crate) fn size(menu_handle: u32) -> Self {
        Self {
            message: MenuDefinitionMessage::Size,
            menu_handle,
            menu_rect: (0, 0, 0, 0),
            hit_point: 0,
            which_item: 0,
        }
    }

    /// Encode the shared Rect followed by the shared INTEGER scratch value.
    pub(crate) fn scratch_bytes(self) -> [u8; 10] {
        let (top, left, bottom, right) = self.menu_rect;
        let mut bytes = [0; 10];
        for (offset, value) in [top, left, bottom, right, self.which_item]
            .into_iter()
            .enumerate()
        {
            bytes[offset * 2..offset * 2 + 2].copy_from_slice(&value.to_be_bytes());
        }
        bytes
    }

    pub(crate) fn call(self, scratch: u32) -> MenuDefinitionCall {
        MenuDefinitionCall {
            message: self.message,
            menu_handle: self.menu_handle,
            menu_rect: scratch,
            hit_point: self.hit_point,
            which_item: scratch + 8,
        }
    }

    pub(crate) fn decode_result(bytes: [u8; 10]) -> MenuDefinitionResult {
        let word = |offset: usize| i16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        MenuDefinitionResult {
            menu_rect: (word(0), word(2), word(4), word(6)),
            which_item: word(8),
        }
    }
}

/// Architecture-neutral continuation for an application-defined MDEF.
///
/// The Menu Manager owns the current rectangle, previous item, and callback
/// ordering. CPU adapters only execute `pending_invocation` and return its
/// two by-reference results. Macintosh Toolbox Essentials (1992),
/// pp. 3-87--3-91 and 3-148--3-151.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuDefinitionTracking {
    menu_handle: u32,
    menu_rect: (i16, i16, i16, i16),
    which_item: i16,
    last_hit_point: Option<u32>,
    pending_invocation: Option<MenuDefinitionInvocation>,
}

impl MenuDefinitionTracking {
    pub(crate) fn begin_draw(menu_handle: u32, menu_rect: (i16, i16, i16, i16)) -> Self {
        Self {
            menu_handle,
            menu_rect,
            which_item: 0,
            last_hit_point: None,
            pending_invocation: Some(MenuDefinitionInvocation {
                message: MenuDefinitionMessage::Draw,
                menu_handle,
                menu_rect,
                hit_point: 0,
                which_item: 0,
            }),
        }
    }

    pub(crate) fn begin_popup(menu_handle: u32, hit_point: u32, which_item: i16) -> Self {
        Self {
            menu_handle,
            menu_rect: (0, 0, 0, 0),
            which_item,
            last_hit_point: None,
            pending_invocation: Some(MenuDefinitionInvocation {
                message: MenuDefinitionMessage::PopUp,
                menu_handle,
                menu_rect: (0, 0, 0, 0),
                hit_point,
                which_item,
            }),
        }
    }

    pub(crate) fn pending_invocation(self) -> Option<MenuDefinitionInvocation> {
        self.pending_invocation
    }

    pub(crate) fn complete_pending(
        &mut self,
        result: MenuDefinitionResult,
    ) -> Option<MenuDefinitionMessage> {
        let completed = self.pending_invocation.take()?;
        self.menu_rect = result.menu_rect;
        self.which_item = result.which_item;
        Some(completed.message)
    }

    pub(crate) fn draw(&mut self) -> Option<MenuDefinitionInvocation> {
        if self.pending_invocation.is_some() {
            return None;
        }
        let invocation = MenuDefinitionInvocation {
            message: MenuDefinitionMessage::Draw,
            menu_handle: self.menu_handle,
            menu_rect: self.menu_rect,
            hit_point: 0,
            which_item: self.which_item,
        };
        self.pending_invocation = Some(invocation);
        Some(invocation)
    }

    /// Request the MDEF to reconcile its highlight with a new global point.
    /// Repeated host frames at the same point do not create duplicate guest
    /// calls; the source contract requires calls when the cursor moves into
    /// or out of an item.
    pub(crate) fn choose(&mut self, hit_point: u32) -> Option<MenuDefinitionInvocation> {
        if self.pending_invocation.is_some() || self.last_hit_point == Some(hit_point) {
            return None;
        }
        self.last_hit_point = Some(hit_point);
        self.queue_choose(hit_point)
    }

    /// Queue one Menu Manager-controlled blink phase. The documented MDEF
    /// contract unhighlights on an outside point and highlights on the saved
    /// selection point; repeated `mChooseMsg` calls produce the blink.
    /// Inside Macintosh Volume I (1985), p. I-366.
    pub(crate) fn flash(&mut self, visible: bool) -> Option<MenuDefinitionInvocation> {
        if self.pending_invocation.is_some() {
            return None;
        }
        let hit_point = if visible {
            self.last_hit_point?
        } else {
            let (top, left, bottom, right) = self.menu_rect;
            let (vertical, horizontal) = if top > i16::MIN {
                (top - 1, left)
            } else if left > i16::MIN {
                (top, left - 1)
            } else if bottom < i16::MAX {
                (bottom, left)
            } else {
                (top, right)
            };
            (u32::from(vertical as u16) << 16) | u32::from(horizontal as u16)
        };
        self.queue_choose(hit_point)
    }

    fn queue_choose(&mut self, hit_point: u32) -> Option<MenuDefinitionInvocation> {
        let invocation = MenuDefinitionInvocation {
            message: MenuDefinitionMessage::Choose,
            menu_handle: self.menu_handle,
            menu_rect: self.menu_rect,
            hit_point,
            which_item: self.which_item,
        };
        self.pending_invocation = Some(invocation);
        Some(invocation)
    }

    pub(crate) fn which_item(self) -> i16 {
        self.which_item
    }

    pub(crate) fn menu_handle(self) -> u32 {
        self.menu_handle
    }

    pub(crate) fn menu_rect(self) -> (i16, i16, i16, i16) {
        self.menu_rect
    }
}

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

/// The standard MDEF pane whose frame and one-pixel shadow are being drawn.
///
/// Pull-down menus attach directly to the menu bar and therefore omit a
/// separate top edge. Hierarchical and pop-up panes are detached rectangles;
/// the latter starts its right shadow one pixel lower. Macintosh Toolbox
/// Essentials (1992), pp. 3-34, 3-120, and 3-122--3-123.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StandardMenuPaneKind {
    PullDown,
    Hierarchical,
    PopUp,
}

impl From<MenuTrackingKind> for StandardMenuPaneKind {
    fn from(kind: MenuTrackingKind) -> Self {
        match kind {
            MenuTrackingKind::MenuBar => Self::PullDown,
            MenuTrackingKind::PopUp => Self::PopUp,
        }
    }
}

/// Architecture-neutral pixel plan for standard menu-pane chrome.
///
/// The plan deliberately contains no framebuffer representation or color.
/// CPU adapters erase the pane and apply the visited frame/shadow pixels to
/// their own surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StandardMenuChrome {
    rect: (i16, i16, i16, i16),
    top_border: bool,
    shadow_top: i16,
}

impl StandardMenuChrome {
    pub(crate) fn new(kind: StandardMenuPaneKind, rect: (i16, i16, i16, i16)) -> Option<Self> {
        let (top, left, bottom, right) = rect;
        if top >= bottom || left >= right {
            return None;
        }
        Some(Self {
            rect,
            top_border: kind != StandardMenuPaneKind::PullDown,
            shadow_top: top.saturating_add(if kind == StandardMenuPaneKind::PopUp {
                3
            } else {
                2
            }),
        })
    }

    pub(crate) fn for_each_frame_pixel(self, mut visit: impl FnMut(i16, i16)) {
        let (top, left, bottom, right) = self.rect;
        for y in top..bottom {
            for x in left..right {
                if x == left
                    || x == right.saturating_sub(1)
                    || y == bottom.saturating_sub(1)
                    || (self.top_border && y == top)
                {
                    visit(x, y);
                }
            }
        }
    }

    pub(crate) fn for_each_shadow_pixel(self, mut visit: impl FnMut(i16, i16)) {
        let (_top, left, bottom, right) = self.rect;
        for y in self.shadow_top..=bottom {
            visit(right, y);
        }
        for x in left.saturating_add(3)..=right {
            visit(x, bottom);
        }
    }
}

/// Direction currently armed by a standard scrolling-menu indicator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuScrollDirection {
    Up,
    Down,
}

/// One regular menu title's live MenuList geometry.
///
/// `left..right` is the half-open logical hit cell stored by the Menu
/// Manager. The standard MBDF draws title ink seven pixels inside that cell
/// and reverses a rectangle extending two pixels left and three pixels right
/// of it. Inside Macintosh Volume I (1985), pp. I-352--I-356; Inside
/// Macintosh Volume V (1986), pp. V-228--V-230 and V-252--V-253.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuBarTitleRegion {
    pub(crate) handle: u32,
    pub(crate) left: i16,
    pub(crate) right: i16,
}

impl MenuBarTitleRegion {
    pub(crate) fn contains_horizontal(self, horizontal: i16) -> bool {
        horizontal >= self.left && horizontal < self.right
    }

    pub(crate) fn title_origin(self) -> i16 {
        self.left
            .saturating_add(STANDARD_MENU_BAR_TITLE_ORIGIN_INSET)
    }

    pub(crate) fn highlighted_rect(self, menu_bar_height: i16) -> (i16, i16, i16, i16) {
        (
            1,
            self.left.saturating_sub(2),
            menu_bar_height.saturating_sub(1),
            self.right.saturating_add(3),
        )
    }
}

/// Center the standard system-font metrics vertically in the live menu bar.
pub(crate) fn standard_menu_bar_title_baseline(
    menu_bar_height: i16,
    ascent: i16,
    descent: i16,
) -> i16 {
    menu_bar_height
        .saturating_sub(ascent)
        .saturating_sub(descent)
        / 2
        + ascent
}

/// Align the replacement system-menu artwork to the standard title baseline.
pub(crate) fn standard_menu_bar_system_mark_top(
    menu_bar_height: i16,
    ascent: i16,
    descent: i16,
) -> i16 {
    standard_menu_bar_title_baseline(menu_bar_height, ascent, descent)
        .saturating_sub(ascent)
        .saturating_add(1)
}

/// Result of one standard scrolling-menu pointer update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuPointerUpdate {
    /// Selectable item exposed to MenuSelect and the retained highlight.
    pub(crate) item: i16,
    /// Raw standard-MDEF row written to MenuDisable for MenuChoice. Disabled
    /// items and dividers remain observable here even though `item` is zero.
    pub(crate) menu_choice_item: i16,
    pub(crate) scrolled: bool,
}

/// Retained pane selected by one standard-MDEF pointer update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuTrackingPane {
    Root,
    Submenu(usize),
}

/// One highlighted standard-menu row that may own a hierarchical child.
/// The adapter resolves the child handle from live guest records before the
/// shared state reconciles the retained pane chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SubmenuRequest<MenuRef> {
    pub(crate) parent: MenuTrackingPane,
    pub(crate) parent_handle: MenuRef,
    pub(crate) parent_item: i16,
    pub(crate) child_depth: usize,
}

/// Opaque authorization to install the child selected by one successful
/// reconciliation. Adapters may inspect the staged identities but cannot
/// manufacture a different request/child pair.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct SubmenuOpenToken<MenuRef> {
    request: SubmenuRequest<MenuRef>,
    child_handle: MenuRef,
}

impl<MenuRef: Copy> SubmenuOpenToken<MenuRef> {
    pub(crate) fn request(&self) -> SubmenuRequest<MenuRef> {
        self.request
    }

    pub(crate) fn child_handle(&self) -> MenuRef {
        self.child_handle
    }
}

/// Result of reconciling one live hierarchical-child lookup with the retained
/// open-menu chain. Presentation adapters restore returned panes in the order
/// supplied before constructing a replacement.
pub(crate) enum SubmenuReconciliation<MenuRef, Pane> {
    Stale,
    Keep,
    Closed {
        panes_deepest_first: Vec<Pane>,
    },
    Open {
        token: SubmenuOpenToken<MenuRef>,
        panes_deepest_first: Vec<Pane>,
    },
}

/// Architecture-neutral state change produced by one standard-menu pointer
/// update. Guest-memory writes, saved-pixel restoration, and drawing remain
/// adapter effects.
pub(crate) struct StandardMenuTrackingUpdate<MenuRef, Pane> {
    pub(crate) pane: MenuTrackingPane,
    pub(crate) menu_handle: MenuRef,
    pub(crate) previous_item: i16,
    pub(crate) pointer: MenuPointerUpdate,
    pub(crate) content_top: i16,
    pub(crate) content_bottom: i16,
    /// Removed descendants in the order their saved pixels must be restored.
    pub(crate) closed_panes_deepest_first: Vec<Pane>,
}

impl<MenuRef: Copy, Pane> StandardMenuTrackingUpdate<MenuRef, Pane> {
    /// Stage hierarchical-child resolution from the row selected by this
    /// pointer update. A zero/disabled row cannot own a child.
    pub(crate) fn submenu_request(&self) -> Option<SubmenuRequest<MenuRef>> {
        (self.pointer.item > 0).then(|| {
            let child_depth = match self.pane {
                MenuTrackingPane::Root => 0,
                MenuTrackingPane::Submenu(depth) => depth.saturating_add(1),
            };
            SubmenuRequest {
                parent: self.pane,
                parent_handle: self.menu_handle,
                parent_item: self.pointer.item,
                child_depth,
            }
        })
    }
}

/// Pack the standard MDEF's live MenuDisable value.
///
/// The high word is the menu ID and the low word is the raw item number under
/// the pointer. The standard MDEF updates this value while a menu is down so
/// MenuChoice can identify a disabled item after MenuSelect returns zero.
/// Inside Macintosh Volume V (1986), pp. V-235 and V-248--V-249; Macintosh
/// Toolbox Essentials (1992), pp. 3-90--3-91 and 3-118--3-119.
pub(crate) fn menu_choice_value(menu_id: i16, item_number: i16) -> u32 {
    (u32::from(menu_id as u16) << 16) | u32::from(item_number as u16)
}

/// Framebuffer identity retained only so the originating presentation adapter
/// can restore its save-under pixels when a tracked pane closes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuTrackingSurface {
    pub(crate) base_addr: u32,
    pub(crate) row_bytes: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) depth: u32,
}

/// Resource bytes retained by the native presentation adapter for one menu
/// item. Resource ownership and framebuffer writes remain adapter work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TrackedMenuIcon {
    CIcon(Vec<u8>),
    Icon { data: Vec<u8>, reduced: bool },
    SmallIcon(Vec<u8>),
}

/// Presentation snapshot retained for one native standard-menu item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrackedMenuItemAppearance {
    pub(crate) height: i16,
    pub(crate) icon_kind: StandardMenuIconKind,
    pub(crate) icon: Option<TrackedMenuIcon>,
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
    /// Application-defined item drawing and hit-testing for this pane.
    pub(crate) definition: Option<MenuDefinitionTracking>,
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
    /// Application-defined item drawing and hit-testing for the root pane.
    pub(crate) definition: Option<MenuDefinitionTracking>,
    pub(crate) flash_remaining: u32,
    pub(crate) flash_delay: u8,
    pub(crate) flash_result: u32,
    pub(crate) saved_width: i16,
    pub(crate) saved_height: i16,
    pub(crate) front_buffer: Surface,
    pub(crate) saved_pixels: Vec<Pixel>,
    pub(crate) item_appearances: Vec<Appearance>,
    pub(crate) submenus: Vec<TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>>,
}

/// The one concrete retained Menu Manager continuation used by both CPU
/// gateways in a process. A missing surface denotes the classic framebuffer
/// adapter; a native surface identifies the framebuffer whose pixels were
/// saved by the PowerPC adapter.
pub(crate) type ProcessMenuTrackingState =
    MenuTrackingState<u32, Option<MenuTrackingSurface>, u16, TrackedMenuItemAppearance>;
pub(crate) type ProcessTrackedMenuPane =
    TrackedMenuPane<u32, Option<MenuTrackingSurface>, u16, TrackedMenuItemAppearance>;

#[cfg(test)]
pub(crate) fn test_process_menu_tracking(menu_handle: u32) -> ProcessMenuTrackingState {
    ProcessMenuTrackingState {
        kind: MenuTrackingKind::MenuBar,
        menu_handle,
        popup_left: 10,
        popup_top: 20,
        content_top: 20,
        scroll_direction: None,
        popup_width: 100,
        popup_height: 40,
        highlighted_item: 1,
        definition: None,
        flash_remaining: 0,
        flash_delay: 0,
        flash_result: 0,
        saved_width: 101,
        saved_height: 41,
        front_buffer: None,
        saved_pixels: Vec::new(),
        item_appearances: Vec::new(),
        submenus: Vec::new(),
    }
}

/// One process-scoped retained Menu Manager owner.
///
/// `MenuSelect` manages the complete mouse-down-through-release interaction,
/// including hierarchical menus, while the menu list stores MenuHandles to
/// the live MenuRecords. Macintosh Toolbox Essentials (1992), pp. 3-95--3-97
/// and 3-114--3-119. Both CPU gateways therefore attach to this same retained
/// continuation instead of owning parallel interaction state.
#[derive(Debug, Default)]
pub(crate) struct SharedMenuTracking(Rc<UnsafeCell<Option<ProcessMenuTrackingState>>>);

impl Clone for SharedMenuTracking {
    fn clone(&self) -> Self {
        // A cloned runtime is a snapshot, not another live CPU adapter.
        Self(Rc::new(UnsafeCell::new(self.state().clone())))
    }
}

impl PartialEq for SharedMenuTracking {
    fn eq(&self, other: &Self) -> bool {
        self.state() == other.state()
    }
}

impl Eq for SharedMenuTracking {}

impl PartialEq<Option<ProcessMenuTrackingState>> for SharedMenuTracking {
    fn eq(&self, other: &Option<ProcessMenuTrackingState>) -> bool {
        self.state() == other
    }
}

impl SharedMenuTracking {
    fn state(&self) -> &Option<ProcessMenuTrackingState> {
        // SAFETY: shared handles can only be created under the serialized
        // ownership contract documented by `shared_handle`.
        unsafe { &*self.0.get() }
    }

    fn state_mut(&mut self) -> &mut Option<ProcessMenuTrackingState> {
        // SAFETY: shared handles can only be created under the serialized
        // ownership contract documented by `shared_handle`.
        unsafe { &mut *self.0.get() }
    }

    /// Attach another CPU adapter without copying the retained continuation.
    ///
    /// # Safety
    ///
    /// Every handle sharing this allocation must remain under one owner that
    /// serializes access. No continuation reference may remain live while
    /// another handle reads or mutates the allocation.
    pub(crate) unsafe fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    /// Attach adapter-local tracking to the process continuation.
    ///
    /// # Safety
    ///
    /// The process owner must serialize all access to every attached handle.
    pub(crate) unsafe fn attach_to(&mut self, process_tracking: &Self) {
        if Rc::ptr_eq(&self.0, &process_tracking.0) {
            return;
        }
        assert!(
            self.is_none() || process_tracking.is_none(),
            "cannot attach two active Menu Manager continuations"
        );
        let pending = self.take();
        // SAFETY: ProcessContext is owned by FixtureRunner, which serializes
        // access to every attached CPU adapter through its mutable borrow.
        self.0 = unsafe { process_tracking.shared_handle() }.0;
        if self.is_none() {
            **self = pending;
        }
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Option<ProcessMenuTrackingState> {
        self.state().clone()
    }
}

impl Deref for SharedMenuTracking {
    type Target = Option<ProcessMenuTrackingState>;

    fn deref(&self) -> &Self::Target {
        self.state()
    }
}

impl DerefMut for SharedMenuTracking {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state_mut()
    }
}

/// One process's host-presented menu command awaiting `MenuSelect`.
///
/// The host injects one ordinary menu-bar click and retains the selected
/// command until the guest reaches `MenuSelect`. Both CPU adapters belong to
/// the same process and must therefore observe and consume the same pending
/// value; only their event and result ABI handling remains architecture-local.
#[derive(Debug, Default)]
pub(crate) struct SharedNativeMenuSelection(Rc<RefCell<Option<(i16, i16)>>>);

impl Clone for SharedNativeMenuSelection {
    fn clone(&self) -> Self {
        Self(Rc::new(RefCell::new(*self.0.borrow())))
    }
}

impl PartialEq for SharedNativeMenuSelection {
    fn eq(&self, other: &Self) -> bool {
        *self.0.borrow() == *other.0.borrow()
    }
}

impl Eq for SharedNativeMenuSelection {}

impl PartialEq<Option<(i16, i16)>> for SharedNativeMenuSelection {
    fn eq(&self, other: &Option<(i16, i16)>) -> bool {
        *self.0.borrow() == *other
    }
}

impl SharedNativeMenuSelection {
    /// Attach another CPU adapter without copying the pending selection.
    #[cfg(test)]
    pub(crate) fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    /// Attach adapter-local input to the process selection slot.
    pub(crate) fn attach_to(&mut self, process_selection: &Self) {
        if Rc::ptr_eq(&self.0, &process_selection.0) {
            return;
        }
        assert!(
            self.is_none() || process_selection.is_none(),
            "cannot attach two pending native menu selections"
        );
        let pending = self.take();
        self.0 = Rc::clone(&process_selection.0);
        if let Some(pending) = pending {
            self.stage(pending);
        }
    }

    pub(crate) fn is_some(&self) -> bool {
        self.0.borrow().is_some()
    }

    pub(crate) fn is_none(&self) -> bool {
        self.0.borrow().is_none()
    }

    pub(crate) fn stage(&mut self, selection: (i16, i16)) -> bool {
        if *self.0.borrow() == Some(selection) {
            return false;
        }
        *self.0.borrow_mut() = Some(selection);
        true
    }

    pub(crate) fn take(&mut self) -> Option<(i16, i16)> {
        self.0.borrow_mut().take()
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Option<(i16, i16)> {
        *self.0.borrow()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuDefinitionPane {
    Root,
    Submenu(usize),
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
    /// Begin the Menu Manager-owned release blink using the live MenuFlash
    /// count. Returns false when blinking is disabled and the adapter should
    /// complete the originating call immediately.
    pub(crate) fn begin_flash(&mut self, count: u16, result: u32) -> bool {
        self.flash_remaining = menu_flash_phase_count(count);
        self.flash_delay = if self.flash_remaining == 0 {
            0
        } else {
            STANDARD_MENU_FLASH_PHASE_DELAY
        };
        self.flash_result = result;
        self.flash_remaining != 0
    }

    pub(crate) fn active_definition_pane(&self) -> Option<MenuDefinitionPane> {
        self.submenus
            .iter()
            .enumerate()
            .rev()
            .find_map(|(depth, submenu)| {
                submenu
                    .definition
                    .is_some()
                    .then_some(MenuDefinitionPane::Submenu(depth))
            })
            .or_else(|| {
                self.definition
                    .is_some()
                    .then_some(MenuDefinitionPane::Root)
            })
    }

    pub(crate) fn active_definition(&self) -> Option<&MenuDefinitionTracking> {
        match self.active_definition_pane()? {
            MenuDefinitionPane::Root => self.definition.as_ref(),
            MenuDefinitionPane::Submenu(depth) => self.submenus.get(depth)?.definition.as_ref(),
        }
    }

    pub(crate) fn active_definition_mut(&mut self) -> Option<&mut MenuDefinitionTracking> {
        match self.active_definition_pane()? {
            MenuDefinitionPane::Root => self.definition.as_mut(),
            MenuDefinitionPane::Submenu(depth) => self.submenus.get_mut(depth)?.definition.as_mut(),
        }
    }

    pub(crate) fn take_active_definition(&mut self) -> Option<MenuDefinitionTracking> {
        match self.active_definition_pane()? {
            MenuDefinitionPane::Root => self.definition.take(),
            MenuDefinitionPane::Submenu(depth) => self.submenus.get_mut(depth)?.definition.take(),
        }
    }

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

    fn close_submenus_deepest_first(
        &mut self,
        depth: usize,
    ) -> Vec<TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>> {
        let mut panes = self.close_submenus_from(depth);
        panes.reverse();
        panes
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

    /// Apply one standard-MDEF pointer slice to the retained hierarchy.
    ///
    /// The deepest open standard submenu under the pointer owns the update;
    /// otherwise the root pane does. This reducer owns row hit-testing,
    /// scrolling state, highlight replacement, and descendant closure. CPU
    /// adapters supply rows decoded from live guest records and perform the
    /// returned presentation effects. Macintosh Toolbox Essentials (1992),
    /// pp. 3-90--3-92 and 3-114--3-119; Inside Macintosh Volume V (1986),
    /// pp. V-250--V-254.
    pub(crate) fn track_standard_pointer(
        &mut self,
        root_rows: &MenuRows,
        submenu_rows: &[Option<MenuRows>],
        point: (i16, i16),
    ) -> Option<
        StandardMenuTrackingUpdate<MenuRef, TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>>,
    > {
        if self.definition.is_some() {
            return None;
        }
        let pane = self
            .submenus
            .iter()
            .enumerate()
            .rev()
            .find_map(|(depth, submenu)| {
                if submenu.definition.is_some() {
                    return None;
                }
                let rows = submenu_rows.get(depth)?.as_ref()?;
                let rect = (
                    submenu.popup_top,
                    submenu.popup_left,
                    submenu.popup_top.saturating_add(submenu.popup_height),
                    submenu.popup_left.saturating_add(submenu.popup_width),
                );
                let (vertical, horizontal) = point;
                let inside = vertical >= rect.0
                    && vertical < rect.2
                    && horizontal >= rect.1
                    && horizontal < rect.3;
                (inside
                    || rows
                        .pointer_scroll_direction(rect, submenu.content_top, point)
                        .is_some())
                .then_some(MenuTrackingPane::Submenu(depth))
            })
            .unwrap_or(MenuTrackingPane::Root);
        let rows = match pane {
            MenuTrackingPane::Root => root_rows,
            MenuTrackingPane::Submenu(depth) => submenu_rows[depth]
                .as_ref()
                .expect("selected submenu rows must remain available"),
        };
        let (menu_handle, previous_item, pointer, content_top) = match pane {
            MenuTrackingPane::Root => {
                let rect = (
                    self.popup_top,
                    self.popup_left,
                    self.popup_top.saturating_add(self.popup_height),
                    self.popup_left.saturating_add(self.popup_width),
                );
                let pointer = rows.track_pointer(
                    rect,
                    &mut self.content_top,
                    &mut self.scroll_direction,
                    point,
                );
                (
                    self.menu_handle,
                    self.highlighted_item,
                    pointer,
                    self.content_top,
                )
            }
            MenuTrackingPane::Submenu(depth) => {
                let submenu = &mut self.submenus[depth];
                let rect = (
                    submenu.popup_top,
                    submenu.popup_left,
                    submenu.popup_top.saturating_add(submenu.popup_height),
                    submenu.popup_left.saturating_add(submenu.popup_width),
                );
                let pointer = rows.track_pointer(
                    rect,
                    &mut submenu.content_top,
                    &mut submenu.scroll_direction,
                    point,
                );
                (
                    submenu.menu_handle,
                    submenu.highlighted_item,
                    pointer,
                    submenu.content_top,
                )
            }
        };
        let parent_depth = match pane {
            MenuTrackingPane::Root => None,
            MenuTrackingPane::Submenu(depth) => Some(depth),
        };
        let mut closed_panes_deepest_first = self
            .update_highlight(parent_depth, pointer.item)
            .expect("the selected retained pane must remain available");
        closed_panes_deepest_first.reverse();

        Some(StandardMenuTrackingUpdate {
            pane,
            menu_handle,
            previous_item,
            pointer,
            content_top,
            content_bottom: content_top.saturating_add(rows.total_height()),
            closed_panes_deepest_first,
        })
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

    fn submenu_request_is_current(&self, request: SubmenuRequest<MenuRef>) -> bool
    where
        MenuRef: PartialEq,
    {
        if request.parent_item <= 0 {
            return false;
        }
        match request.parent {
            MenuTrackingPane::Root => {
                request.child_depth == 0
                    && request.parent_handle == self.menu_handle
                    && request.parent_item == self.highlighted_item
                    && self.definition.is_none()
            }
            MenuTrackingPane::Submenu(depth) => {
                request.child_depth == depth.saturating_add(1)
                    && self.submenus.get(depth).is_some_and(|submenu| {
                        request.parent_handle == submenu.menu_handle
                            && request.parent_item == submenu.highlighted_item
                            && submenu.definition.is_none()
                    })
            }
        }
    }

    /// Reconcile one adapter-resolved live child with the retained hierarchy.
    /// Missing and circular children close the old descendant chain; an exact
    /// match stays open; a changed child closes the old chain before asking
    /// the adapter to build a replacement. Macintosh Toolbox Essentials
    /// (1992), pp. 3-137--3-141.
    pub(crate) fn reconcile_submenu(
        &mut self,
        request: SubmenuRequest<MenuRef>,
        resolved_child: Option<MenuRef>,
    ) -> SubmenuReconciliation<MenuRef, TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>>
    where
        MenuRef: PartialEq,
    {
        if !self.submenu_request_is_current(request) {
            return SubmenuReconciliation::Stale;
        }
        let Some(child_handle) = resolved_child else {
            return SubmenuReconciliation::Closed {
                panes_deepest_first: self.close_submenus_deepest_first(request.child_depth),
            };
        };
        if self.submenu_repeats_ancestor(request.child_depth, child_handle) {
            return SubmenuReconciliation::Closed {
                panes_deepest_first: self.close_submenus_deepest_first(request.child_depth),
            };
        }
        if self
            .submenus
            .get(request.child_depth)
            .is_some_and(|submenu| {
                submenu.menu_handle == child_handle && submenu.parent_item == request.parent_item
            })
        {
            return SubmenuReconciliation::Keep;
        }
        SubmenuReconciliation::Open {
            token: SubmenuOpenToken {
                request,
                child_handle,
            },
            panes_deepest_first: self.close_submenus_deepest_first(request.child_depth),
        }
    }

    /// Commit an adapter-built child only if its staged parent still owns the
    /// same highlighted row and its identity matches the reconciliation.
    /// Returning the pane on failure leaves restoration/disposal to the
    /// adapter that created its presentation snapshot.
    pub(crate) fn install_submenu(
        &mut self,
        token: SubmenuOpenToken<MenuRef>,
        pane: TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>,
    ) -> Result<usize, TrackedMenuPane<MenuRef, Surface, Pixel, Appearance>>
    where
        MenuRef: PartialEq,
    {
        let request = token.request;
        let child_handle = token.child_handle;
        if !self.submenu_request_is_current(request)
            || self.submenus.len() != request.child_depth
            || pane.parent_item != request.parent_item
            || pane.menu_handle != child_handle
            || self.submenu_repeats_ancestor(request.child_depth, child_handle)
        {
            return Err(pane);
        }
        self.submenus.push(pane);
        Ok(request.child_depth)
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
            if let Some(item) = submenu
                .definition
                .as_ref()
                .map(|definition| definition.which_item())
                .filter(|item| *item > 0)
            {
                return Some((submenu.menu_handle, item));
            }
            if submenu.highlighted_item > 0
                && is_terminal_item(submenu.menu_handle, submenu.highlighted_item)
            {
                return Some((submenu.menu_handle, submenu.highlighted_item));
            }
        }
        if let Some(item) = self
            .definition
            .as_ref()
            .map(|definition| definition.which_item())
            .filter(|item| *item > 0)
        {
            return Some((self.menu_handle, item));
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

/// The icon presentation selected by the standard menu definition procedure.
///
/// The item icon byte is a script code, not an icon number, when the command
/// byte is `$1C`. Otherwise a valid color icon takes priority; `$1D` selects a
/// reduced `ICON`, `$1E` selects an `SICN`, and every other command-byte form
/// uses a normal `ICON`. Macintosh Toolbox Essentials (1992), pp. 3-45--3-46
/// and 3-62--3-63.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StandardMenuIconKind {
    None,
    Color { width: i16, height: i16 },
    Normal,
    Reduced,
    Small,
}

impl StandardMenuIconKind {
    pub(crate) fn width(self) -> i16 {
        match self {
            Self::None => 0,
            Self::Color { width, .. } => width.max(16),
            Self::Normal => 32,
            Self::Reduced | Self::Small => 16,
        }
    }

    pub(crate) fn row_height(self, style: QuickDrawTextStyle) -> i16 {
        let (color_icon_height, uses_normal_icon) = match self {
            Self::Color { height, .. } => (Some(height), false),
            Self::Normal => (None, true),
            Self::None | Self::Reduced | Self::Small => (None, false),
        };
        standard_menu_row_height(color_icon_height, uses_normal_icon, style)
    }
}

/// Resolve the standard MDEF icon form after an adapter has decoded an
/// optional valid `cicn` rectangle from its Resource Manager representation.
pub(crate) fn standard_menu_icon_kind(
    icon: u8,
    command: u8,
    color_icon_size: Option<(i16, i16)>,
) -> StandardMenuIconKind {
    if icon == 0 || command == 0x1C {
        return StandardMenuIconKind::None;
    }
    if let Some((width, height)) =
        color_icon_size.filter(|(width, height)| *width > 0 && *height > 0)
    {
        return StandardMenuIconKind::Color { width, height };
    }
    match command {
        0x1D => StandardMenuIconKind::Reduced,
        0x1E => StandardMenuIconKind::Small,
        _ => StandardMenuIconKind::Normal,
    }
}

/// Convert a standard menu icon number into its Resource Manager ID.
/// Script-code items deliberately have no icon resource identity.
pub(crate) fn standard_menu_icon_resource_id(icon: u8, command: u8) -> Option<i16> {
    (icon != 0 && command != 0x1C).then(|| i16::from(icon).saturating_add(256))
}

/// Validated geometry and sampling policy for a standard monochrome menu icon.
///
/// An `ICON` is a 32-by-32 one-bit image. A reduced item OR-compresses that
/// image into a 16-by-16 slot, while an `SICN` item uses the first 16-by-16
/// image in its small-icon list. Macintosh Toolbox Essentials (1992),
/// pp. 3-45--3-46 and 3-62--3-63.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MonochromeMenuIconLayout {
    pub(crate) width: usize,
    pub(crate) height: usize,
    source_size: usize,
    source_row_bytes: usize,
    source_scale: usize,
    length: Option<usize>,
}

impl MonochromeMenuIconLayout {
    pub(crate) fn for_kind(kind: StandardMenuIconKind, length: Option<usize>) -> Option<Self> {
        let (width, source_size, source_row_bytes, source_scale, required_length) = match kind {
            StandardMenuIconKind::Normal => (32, 32, 4, 1, 128),
            StandardMenuIconKind::Reduced => (16, 32, 4, 2, 128),
            StandardMenuIconKind::Small => (16, 16, 2, 1, 32),
            StandardMenuIconKind::None | StandardMenuIconKind::Color { .. } => return None,
        };
        if length.is_some_and(|length| length < required_length) {
            return None;
        }
        Some(Self {
            width,
            height: width,
            source_size,
            source_row_bytes,
            source_scale,
            length,
        })
    }

    /// Sample one destination pixel through an adapter-provided byte reader.
    /// Reduced `ICON` pixels are the OR of the corresponding 2-by-2 source
    /// cell; normal `ICON` and `SICN` pixels map one-to-one.
    pub(crate) fn sample_with(
        self,
        mut read: impl FnMut(usize) -> Option<u8>,
        x: usize,
        y: usize,
    ) -> Option<bool> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let source_x = x.checked_mul(self.source_scale)?;
        let source_y = y.checked_mul(self.source_scale)?;
        let source_x_end = source_x
            .checked_add(self.source_scale)?
            .min(self.source_size);
        let source_y_end = source_y
            .checked_add(self.source_scale)?
            .min(self.source_size);
        for sy in source_y..source_y_end {
            let row = sy.checked_mul(self.source_row_bytes)?;
            for sx in source_x..source_x_end {
                let offset = row.checked_add(sx / 8)?;
                if self.length.is_some_and(|length| offset >= length) {
                    return None;
                }
                if read(offset)? & (0x80 >> (sx & 7)) != 0 {
                    return Some(true);
                }
            }
        }
        Some(false)
    }
}

/// Offsets and geometry decoded from one compiled color-icon resource.
///
/// A `cicn` stores a 50-byte PixMap, two 14-byte BitMaps, a four-byte data
/// Handle placeholder, then mask, monochrome, ColorTable, and pixel payloads.
/// Offsets remain relative to the resource so adapters can use either guest
/// pointers or host slices. Inside Macintosh: Imaging With QuickDraw (1994),
/// pp. 4-105--4-106.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ColorIconLayout {
    pub(crate) width: i16,
    pub(crate) height: i16,
    pub(crate) pixel_row_bytes: usize,
    pub(crate) mask_row_bytes: usize,
    pub(crate) bitmap_row_bytes: usize,
    pub(crate) pixel_size: u16,
    pub(crate) mask_offset: usize,
    pub(crate) bitmap_offset: usize,
    pub(crate) color_table_offset: usize,
    pub(crate) color_table_entries: usize,
    pub(crate) pixel_data_offset: usize,
}

impl ColorIconLayout {
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        Self::decode_with(|offset| bytes.get(offset).copied(), Some(bytes.len()))
    }

    /// Decode through an adapter byte reader. `length` is optional for guest
    /// allocations whose owner cannot report a bound; when supplied, every
    /// derived inline payload must fit inside it.
    pub(crate) fn decode_with(
        mut read: impl FnMut(usize) -> Option<u8>,
        length: Option<usize>,
    ) -> Option<Self> {
        const HEADER_SIZE: usize = 82;
        if length.is_some_and(|length| length < HEADER_SIZE) {
            return None;
        }
        let mut read_u16 = |offset: usize| {
            Some(u16::from_be_bytes([
                read(offset)?,
                read(offset.checked_add(1)?)?,
            ]))
        };
        let top = read_u16(6)? as i16;
        let left = read_u16(8)? as i16;
        let bottom = read_u16(10)? as i16;
        let right = read_u16(12)? as i16;
        let width = right.checked_sub(left)?;
        let height = bottom.checked_sub(top)?;
        if width <= 0 || height <= 0 {
            return None;
        }
        let pixel_row_bytes = usize::from(read_u16(4)? & 0x3FFF);
        let mask_row_bytes = usize::from(read_u16(54)? & 0x3FFF);
        let bitmap_row_bytes = usize::from(read_u16(68)? & 0x3FFF);
        let pixel_size = read_u16(32)?;
        if pixel_row_bytes == 0 || mask_row_bytes == 0 {
            return None;
        }
        let height = usize::try_from(height).ok()?;
        let mask_offset = HEADER_SIZE;
        let bitmap_offset = mask_offset.checked_add(mask_row_bytes.checked_mul(height)?)?;
        let color_table_offset =
            bitmap_offset.checked_add(bitmap_row_bytes.checked_mul(height)?)?;
        let color_table_size_offset = color_table_offset.checked_add(6)?;
        if let Some(length) = length {
            if color_table_size_offset.checked_add(2)? > length {
                return None;
            }
        }
        let color_table_entries = usize::from(read_u16(color_table_size_offset)?).checked_add(1)?;
        let pixel_data_offset = color_table_offset
            .checked_add(8usize.checked_add(color_table_entries.checked_mul(8)?)?)?;
        let end = pixel_data_offset.checked_add(pixel_row_bytes.checked_mul(height)?)?;
        if length.is_some_and(|length| end > length) {
            return None;
        }
        Some(Self {
            width,
            height: i16::try_from(height).ok()?,
            pixel_row_bytes,
            mask_row_bytes,
            bitmap_row_bytes,
            pixel_size,
            mask_offset,
            bitmap_offset,
            color_table_offset,
            color_table_entries,
            pixel_data_offset,
        })
    }

    /// Test one pixel in the icon's transparency mask.
    pub(crate) fn mask_bit_with(
        self,
        mut read: impl FnMut(usize) -> Option<u8>,
        x: usize,
        y: usize,
    ) -> Option<bool> {
        self.bitmap_bit_with(&mut read, self.mask_offset, self.mask_row_bytes, x, y)
    }

    /// Resolve one source pixel to its 48-bit QuickDraw color.
    ///
    /// Indexed PixMaps use their embedded `ColorTable`; direct 16- and
    /// 32-bit PixMaps expand their packed components. A device color table
    /// uses each entry's ordinal as its pixel value, while a PixMap table uses
    /// `ColorSpec.value`. Inside Macintosh: Imaging With QuickDraw (1994),
    /// pp. 4-14--4-17, 4-47--4-48, 4-105--4-106.
    pub(crate) fn rgb_with(
        self,
        mut read: impl FnMut(usize) -> Option<u8>,
        x: usize,
        y: usize,
    ) -> Option<[u16; 3]> {
        let value = self.packed_pixel_with(&mut read, x, y)?;
        match self.pixel_size {
            16 => Some([
                (((value >> 10) & 0x1f) as u16) * 0x842,
                (((value >> 5) & 0x1f) as u16) * 0x842,
                ((value & 0x1f) as u16) * 0x842,
            ]),
            32 => Some([
                (((value >> 16) & 0xff) as u16) * 0x101,
                (((value >> 8) & 0xff) as u16) * 0x101,
                ((value & 0xff) as u16) * 0x101,
            ]),
            1 | 2 | 4 | 8 => {
                let flags = self.read_u16_with(&mut read, self.color_table_offset + 4)?;
                let device_table = flags & 0x8000 != 0;
                for ordinal in 0..self.color_table_entries {
                    let offset = self
                        .color_table_offset
                        .checked_add(8 + ordinal.checked_mul(8)?)?;
                    let entry_value = if device_table {
                        u32::try_from(ordinal).ok()?
                    } else {
                        u32::from(self.read_u16_with(&mut read, offset)?)
                    };
                    if entry_value == value {
                        return Some([
                            self.read_u16_with(&mut read, offset + 2)?,
                            self.read_u16_with(&mut read, offset + 4)?,
                            self.read_u16_with(&mut read, offset + 6)?,
                        ]);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Sample the 1-bit fallback image used on a monochrome destination.
    pub(crate) fn monochrome_bit_with(
        self,
        mut read: impl FnMut(usize) -> Option<u8>,
        x: usize,
        y: usize,
    ) -> Option<bool> {
        if self.bitmap_row_bytes != 0 {
            self.bitmap_bit_with(&mut read, self.bitmap_offset, self.bitmap_row_bytes, x, y)
        } else {
            Some(self.packed_pixel_with(&mut read, x, y)? != 0)
        }
    }

    fn bitmap_bit_with(
        self,
        read: &mut impl FnMut(usize) -> Option<u8>,
        offset: usize,
        row_bytes: usize,
        x: usize,
        y: usize,
    ) -> Option<bool> {
        if x >= usize::try_from(self.width).ok()? || y >= usize::try_from(self.height).ok()? {
            return None;
        }
        let byte = offset
            .checked_add(y.checked_mul(row_bytes)?)?
            .checked_add(x / 8)?;
        Some(read(byte)? & (0x80 >> (x & 7)) != 0)
    }

    fn packed_pixel_with(
        self,
        read: &mut impl FnMut(usize) -> Option<u8>,
        x: usize,
        y: usize,
    ) -> Option<u32> {
        if x >= usize::try_from(self.width).ok()? || y >= usize::try_from(self.height).ok()? {
            return None;
        }
        let row = self
            .pixel_data_offset
            .checked_add(y.checked_mul(self.pixel_row_bytes)?)?;
        match self.pixel_size {
            1 | 2 | 4 | 8 => {
                let bit = x.checked_mul(usize::from(self.pixel_size))?;
                let byte = read(row.checked_add(bit / 8)?)?;
                let shift = 8usize
                    .checked_sub(usize::from(self.pixel_size))?
                    .checked_sub(bit & 7)?;
                Some(u32::from(byte >> shift) & ((1u32 << self.pixel_size) - 1))
            }
            16 => Some(u32::from(
                self.read_u16_with(read, row.checked_add(x.checked_mul(2)?)?)?,
            )),
            32 => {
                let offset = row.checked_add(x.checked_mul(4)?)?;
                Some(u32::from_be_bytes([
                    read(offset)?,
                    read(offset.checked_add(1)?)?,
                    read(offset.checked_add(2)?)?,
                    read(offset.checked_add(3)?)?,
                ]))
            }
            _ => None,
        }
    }

    fn read_u16_with(
        self,
        read: &mut impl FnMut(usize) -> Option<u8>,
        offset: usize,
    ) -> Option<u16> {
        Some(u16::from_be_bytes([
            read(offset)?,
            read(offset.checked_add(1)?)?,
        ]))
    }
}

/// Horizontal inputs resolved by an architecture adapter for one standard
/// menu item. Text and icon resource measurement remain presentation work;
/// the standard MDEF's column policy belongs to the Menu Manager.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StandardMenuItemWidth {
    pub(crate) text: i16,
    pub(crate) icon: i16,
    pub(crate) command: u8,
}

/// Pixel anchors used to draw one standard menu item.
///
/// The standard definition procedure owns these columns, independent of the
/// caller ISA: the mark starts three pixels inside the menu, plain text starts
/// at 15, icons start at 2 (or 18 after a mark), normal icons reserve through
/// column 51, and the command/hierarchy indicators are anchored from the
/// right edge. Macintosh Toolbox Essentials (1992), pp. 3-12--3-13,
/// 3-45--3-46, and 3-148--3-151; exact anchors match the Mac OS 8.1 standard
/// MDEF captures for both supported machine profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StandardMenuItemLayout {
    pub(crate) mark_left: i16,
    pub(crate) icon_left: i16,
    pub(crate) text_left: i16,
    pub(crate) text_baseline: i16,
    pub(crate) separator_y: i16,
    pub(crate) indicator_left: i16,
    pub(crate) command_left: i16,
    pub(crate) indicator_mid_y: i16,
}

pub(crate) fn standard_menu_item_layout(
    menu_bounds: (i16, i16),
    row_bounds: (i16, i16),
    icon: StandardMenuIconKind,
    has_mark: bool,
    font_metrics: (i16, i16),
    attached_pull_down: bool,
) -> StandardMenuItemLayout {
    let (menu_left, menu_right) = menu_bounds;
    let (row_top, row_height) = row_bounds;
    let (font_ascent, font_descent) = font_metrics;
    let icon_left = menu_left.saturating_add(if has_mark { 18 } else { 2 });
    let text_left = match icon {
        StandardMenuIconKind::None => menu_left.saturating_add(15),
        StandardMenuIconKind::Normal => menu_left.saturating_add(51),
        StandardMenuIconKind::Color { .. }
        | StandardMenuIconKind::Reduced
        | StandardMenuIconKind::Small => icon_left.saturating_add(icon.width()),
    };
    StandardMenuItemLayout {
        mark_left: menu_left.saturating_add(3),
        icon_left,
        text_left,
        text_baseline: row_top
            .saturating_add(row_height.saturating_sub(font_ascent.saturating_add(font_descent)) / 2)
            .saturating_add(font_ascent)
            .saturating_sub(i16::from(attached_pull_down)),
        separator_y: row_top.saturating_add(row_height / 2).saturating_sub(1),
        indicator_left: menu_right.saturating_sub(12),
        command_left: menu_right.saturating_sub(25),
        indicator_mid_y: row_top.saturating_add(row_height / 2),
    }
}

/// Visit every pixel in the standard right-pointing hierarchical-menu mark.
///
/// The standard MDEF places this filled triangle in the keyboard-equivalent
/// column. Keeping its raster in the Menu Manager prevents CPU gateways from
/// choosing different indicator shapes. Inside Macintosh Volume V (1986),
/// pp. V-23 and V-236; Macintosh Toolbox Essentials (1992), p. 3-133.
pub(crate) fn for_each_standard_hierarchy_indicator_pixel(
    left: i16,
    mid_y: i16,
    mut visit: impl FnMut(i16, i16),
) {
    for dx in 0..7i16 {
        let half_height = dx.min(6 - dx);
        for dy in -half_height..=half_height {
            visit(left.saturating_add(dx), mid_y.saturating_add(dy));
        }
    }
}

/// Visit every pixel in the standard upward scrolling indicator.
///
/// The supplied edge is the top of the menu rectangle. Inside Macintosh
/// Volume V (1986), pp. V-248--V-249.
pub(crate) fn for_each_standard_scroll_up_indicator_pixel(
    center_x: i16,
    top: i16,
    mut visit: impl FnMut(i16, i16),
) {
    for dy in 0..6i16 {
        for dx in -dy..=dy {
            visit(
                center_x.saturating_add(dx),
                top.saturating_add(4).saturating_add(dy),
            );
        }
    }
}

/// Visit every pixel in the standard downward scrolling indicator.
///
/// The supplied edge is the bottom of the menu rectangle. Inside Macintosh
/// Volume V (1986), pp. V-248--V-249.
pub(crate) fn for_each_standard_scroll_down_indicator_pixel(
    center_x: i16,
    bottom: i16,
    mut visit: impl FnMut(i16, i16),
) {
    for dy in 0..6i16 {
        let half_width = 5i16.saturating_sub(dy);
        for dx in -half_width..=half_width {
            visit(
                center_x.saturating_add(dx),
                bottom.saturating_sub(10).saturating_add(dy),
            );
        }
    }
}

/// Return whether the port-aligned QuickDraw `gray` pattern has an ink bit.
///
/// The predefined pattern is `$AA, $55, ...`; menu separators and monochrome
/// dimming use the same phase in both CPU gateways. Imaging With QuickDraw
/// (1994), p. 2-36 and pp. 3-5--3-6.
pub(crate) fn standard_menu_gray_pattern_is_ink(x: i16, y: i16) -> bool {
    (i32::from(x) + i32::from(y)).rem_euclid(2) == 0
}

/// Visit every black pixel in the standard main-screen corner mask.
///
/// The Window Manager defines the desktop as a 16-by-16-curvature rounded
/// rectangle below the menu bar, leaving these outer-screen corners outside
/// `GrayRgn`. The menu-bar definition preserves that mask when it redraws the
/// strip. Inside Macintosh Volume I (1985), p. I-281; Inside Macintosh Volume
/// V (1986), p. V-120.
pub(crate) fn for_each_standard_menu_bar_corner_pixel(
    screen_width: i16,
    mut visit: impl FnMut(i16, i16),
) {
    const LEFT_CORNER: &[(i16, i16)] = &[
        (0, 0),
        (1, 0),
        (2, 0),
        (3, 0),
        (4, 0),
        (0, 1),
        (1, 1),
        (2, 1),
        (0, 2),
        (1, 2),
        (0, 3),
        (0, 4),
    ];
    for &(x, y) in LEFT_CORNER {
        visit(x, y);
        visit(screen_width.saturating_sub(1).saturating_sub(x), y);
    }
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
    style: QuickDrawTextStyle,
) -> i16 {
    let base_height = if let Some(height) = color_icon_height {
        height.max(16)
    } else if uses_normal_icon {
        34
    } else {
        16
    };
    if style.shadow() {
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

    /// Preserve row geometry while disabling every selection in a disabled
    /// menu. A disabled title may still be pulled down for examination, but
    /// none of its items can be chosen. Macintosh Toolbox Essentials (1992),
    /// pp. 3-6--3-7.
    pub(crate) fn with_menu_enabled(mut self, menu_enabled: bool) -> Self {
        if !menu_enabled {
            for row in &mut self.rows {
                row.selectable = false;
            }
        }
        self
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

    #[cfg(test)]
    pub(crate) fn item_at_offset(&self, offset: i16) -> Option<i16> {
        let item_number = self.item_number_at_offset(offset)?;
        let selectable = menu_item_index(item_number)
            .and_then(|index| self.rows.get(index))
            .is_some_and(|row| row.selectable);
        Some(if selectable { item_number } else { 0 })
    }

    fn item_number_at_offset(&self, offset: i16) -> Option<i16> {
        if offset < 0 {
            return None;
        }
        let mut remaining = offset;
        for (index, row) in self.rows.iter().enumerate() {
            let height = row.height.max(0);
            if remaining < height {
                return i16::try_from(index + 1).ok();
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
        let item_number =
            self.item_number_at_point_with_content_top(rect, insets, first_item_top, point)?;
        if item_number == 0 {
            return Some(0);
        }
        let selectable = menu_item_index(item_number)
            .and_then(|index| self.rows.get(index))
            .is_some_and(|row| row.selectable);
        Some(if selectable { item_number } else { 0 })
    }

    fn item_number_at_point_with_content_top(
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
            self.item_number_at_offset(vertical.saturating_sub(first_item_top))
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

    fn menu_choice_item_at_point(
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
            self.item_number_at_point_with_content_top(rect, (0, 0, 0, 0), content_top, point)
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
            return MenuPointerUpdate {
                item: 0,
                menu_choice_item: 0,
                scrolled,
            };
        }

        *armed_direction = None;
        MenuPointerUpdate {
            item: self.tracking_item_at_point(rect, *content_top, point),
            menu_choice_item: self.menu_choice_item_at_point(rect, *content_top, point),
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

struct DecodedMenuPartitions {
    regular: Vec<(u32, MenuKeyMenu)>,
    hierarchical: Vec<(u32, MenuKeyMenu)>,
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

    /// Return whether a live standard-menu row can become a MenuSelect
    /// result. Disabling the title disables every row, and separators remain
    /// unavailable regardless of their enable bit. Macintosh Toolbox
    /// Essentials (1992), pp. 3-6--3-7 and 3-114--3-119.
    pub(crate) fn item_is_selectable(&self, item_number: i16) -> bool {
        self.enable_flags & 1 != 0
            && menu_item_index(item_number)
                .and_then(|index| self.items.get(index))
                .is_some_and(|item| item.enabled && item.text.as_slice() != b"-")
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

    /// Insert named resources using the standard Menu Manager policy.
    ///
    /// Resource-derived items are sorted alphabetically without reordering
    /// existing items, names beginning with `.` or `%` are hidden, and an
    /// exact name already present in the menu is not inserted again. New
    /// items use the documented enabled, plain, unmarked, iconless defaults.
    /// Macintosh Toolbox Essentials (1992), pp. 3-101--3-104.
    pub(crate) fn insert_resource_names(
        &mut self,
        names: impl IntoIterator<Item = Vec<u8>>,
        after_item: i16,
    ) -> bool {
        let existing = self
            .items
            .iter()
            .map(|item| item.text.clone())
            .collect::<std::collections::HashSet<_>>();
        let mut seen = std::collections::HashSet::new();
        let mut names = names
            .into_iter()
            .filter_map(|mut name| {
                name.truncate(255);
                (!name.is_empty()
                    && !matches!(name.first(), Some(b'.' | b'%'))
                    && !existing.contains(&name)
                    && seen.insert(name.clone()))
                .then_some(name)
            })
            .collect::<Vec<_>>();
        names.sort_by(|left, right| {
            crate::mac_roman::decode_mac_roman(left)
                .to_lowercase()
                .cmp(&crate::mac_roman::decode_mac_roman(right).to_lowercase())
                .then_with(|| left.cmp(right))
        });
        if names.is_empty() {
            return false;
        }

        let insertion = usize::try_from(after_item.max(0))
            .unwrap_or(0)
            .min(self.items.len());
        self.items.splice(
            insertion..insertion,
            names.into_iter().map(|text| MenuItem {
                text,
                icon: 0,
                command: 0,
                mark: 0,
                style: 0,
                enabled: true,
            }),
        );
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

/// Guest-memory write requested while installing a copied menu list.
///
/// The adapter owns allocation mechanics, while the Menu Manager owns whether
/// a missing current list is allocated or an existing current-list Handle is
/// preserved. Macintosh Toolbox Essentials (1992), pp. 3-112--3-113.
pub(crate) enum MenuListInstallRequest<'a> {
    Allocate { bytes: &'a [u8] },
    Replace { handle: u32, bytes: &'a [u8] },
}

/// Successful installation of a copied menu list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuListInstallation {
    pub(crate) handle: u32,
    pub(crate) allocated: bool,
}

/// Copy `source` into the current menu-list allocation.
///
/// `SetMenuBar` copies only the `DynamicMenuList`, not the referenced menu
/// records. Existing current-list Handle identity is preserved; when no
/// current list exists, the adapter must allocate the complete copy before
/// publishing the returned Handle. Macintosh Toolbox Essentials (1992),
/// pp. 3-112--3-113.
pub(crate) fn install_menu_list_copy<E>(
    current_handle: u32,
    source: &MenuList,
    apply: impl FnOnce(MenuListInstallRequest<'_>) -> Result<u32, E>,
) -> Result<MenuListInstallation, E> {
    let bytes = source.encode();
    let allocated = current_handle == 0;
    let request = if allocated {
        MenuListInstallRequest::Allocate { bytes: &bytes }
    } else {
        MenuListInstallRequest::Replace {
            handle: current_handle,
            bytes: &bytes,
        }
    };
    let handle = apply(request)?;
    debug_assert_ne!(handle, 0);
    debug_assert!(allocated || handle == current_handle);
    Ok(MenuListInstallation { handle, allocated })
}

/// Live guest MenuInfo data supplied by a CPU memory adapter when projecting
/// the current menu list for a frontend.
pub(crate) struct MenuSnapshotRecord {
    pub(crate) id: i16,
    pub(crate) title: Vec<u8>,
    pub(crate) items: MenuItems,
}

/// Decode the entries in a compiled `'mctb'` resource.
///
/// The resource starts with a signed entry count followed by complete
/// 30-byte `MCEntry` records. `GetMenu` and `InitMenus` transfer those records
/// into the process's live menu color information table. Inside Macintosh
/// Volume V (1986), pp. V-242--V-244; Macintosh Toolbox Essentials (1992),
/// pp. 3-154--3-156.
pub(crate) fn compiled_menu_color_entries(bytes: &[u8]) -> Vec<u8> {
    let Some(declared_count) = read_u16(bytes, 0).map(|value| value as i16) else {
        return Vec::new();
    };
    if declared_count <= 0 {
        return Vec::new();
    }
    let available_count = bytes.len().saturating_sub(2) / MENU_COLOR_ENTRY_SIZE;
    let entry_count = usize::from(declared_count as u16).min(available_count);
    let mut entries = Vec::with_capacity(entry_count * MENU_COLOR_ENTRY_SIZE);
    for index in 0..entry_count {
        let offset = 2 + index * MENU_COLOR_ENTRY_SIZE;
        let entry = &bytes[offset..offset + MENU_COLOR_ENTRY_SIZE];
        if read_u16(entry, 0).map(|value| value as i16) == Some(MENU_COLOR_END_ID) {
            continue;
        }
        entries.extend_from_slice(entry);
    }
    entries
}

/// Merge complete `MCEntry` records into a live menu color information table.
/// Existing `(menu ID, item)` entries are replaced in place and new identities
/// retain source order at the end of the table. Inside Macintosh Volume V
/// (1986), pp. V-242--V-244.
pub(crate) fn merge_menu_color_entries(current: &[u8], incoming: &[u8]) -> Vec<u8> {
    let mut merged = current.to_vec();
    for entry in incoming.chunks_exact(MENU_COLOR_ENTRY_SIZE) {
        let Some(key) = menu_color_entry_key(entry) else {
            continue;
        };
        let existing = merged
            .chunks_exact(MENU_COLOR_ENTRY_SIZE)
            .position(|candidate| menu_color_entry_key(candidate) == Some(key));
        if let Some(index) = existing {
            let offset = index * MENU_COLOR_ENTRY_SIZE;
            merged[offset..offset + MENU_COLOR_ENTRY_SIZE].copy_from_slice(entry);
        } else {
            merged.extend_from_slice(entry);
        }
    }
    merged
}

/// Keep only live `MCEntry` records accepted by the supplied identity filter.
/// Macintosh Toolbox Essentials (1992), pp. 3-109--3-110 documents the table
/// effects of deleting one menu or clearing the complete menu bar.
pub(crate) fn filter_menu_color_entries(
    current: &[u8],
    mut keep: impl FnMut(i16, i16) -> bool,
) -> Vec<u8> {
    let mut filtered = Vec::with_capacity(current.len());
    for entry in current.chunks_exact(MENU_COLOR_ENTRY_SIZE) {
        if let Some((menu_id, menu_item)) = menu_color_entry_key(entry) {
            if keep(menu_id, menu_item) {
                filtered.extend_from_slice(entry);
            }
        }
    }
    filtered
}

const STANDARD_MENU_BLACK: [u16; 3] = [0; 3];
const STANDARD_MENU_WHITE: [u16; 3] = [u16::MAX; 3];

/// Architecture-neutral view of the live `MenuCInfo` `MCEntry` sequence.
///
/// The CPU adapters own the Handle and supply its current bytes. This view
/// owns the standard MBDF/MDEF fallback chain, so direct guest writes are
/// observed without a second retained color table. Inside Macintosh Volume V
/// (1986), pp. V-231--V-235 and V-249--V-253; Macintosh Toolbox Essentials
/// (1992), pp. 3-152--3-156.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MenuColorTable<'a> {
    bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StandardMenuItemColors {
    pub(crate) mark: [u16; 3],
    pub(crate) name: [u16; 3],
    pub(crate) command: [u16; 3],
    pub(crate) background: [u16; 3],
}

/// Reverse a standard MBDF/MDEF element's foreground and background values.
///
/// Color menu highlighting swaps only the resolved element colors and leaves
/// unrelated pixels unchanged. Inside Macintosh Volume V (1986), pp. V-249
/// and V-252--V-253.
pub(crate) fn standard_menu_highlighted_value<T: Copy + Eq>(
    value: T,
    background: T,
    foreground: T,
) -> T {
    if value == background {
        foreground
    } else if value == foreground {
        background
    } else {
        value
    }
}

impl<'a> MenuColorTable<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Resolve the menu-bar background from `MCEntry(0, 0).RGB4`.
    pub(crate) fn menu_bar_background(self) -> [u16; 3] {
        self.entry_rgb(0, 0, 22).unwrap_or(STANDARD_MENU_WHITE)
    }

    /// Resolve a menu title's foreground from its RGB1, then the menu-bar
    /// entry's RGB1, then standard black.
    pub(crate) fn title_foreground(self, menu_id: i16) -> [u16; 3] {
        self.entry_rgb(menu_id, 0, 4)
            .or_else(|| self.entry_rgb(0, 0, 4))
            .unwrap_or(STANDARD_MENU_BLACK)
    }

    /// Resolve a menu title's background from its RGB2, falling back to the
    /// menu-bar background.
    pub(crate) fn title_background(self, menu_id: i16) -> [u16; 3] {
        self.entry_rgb(menu_id, 0, 10)
            .unwrap_or_else(|| self.menu_bar_background())
    }

    /// Resolve a pulled-down menu's background from its title RGB4, then the
    /// menu-bar entry's RGB2, then standard white.
    pub(crate) fn dropdown_background(self, menu_id: i16) -> [u16; 3] {
        self.entry_rgb(menu_id, 0, 22)
            .or_else(|| self.entry_rgb(0, 0, 10))
            .unwrap_or(STANDARD_MENU_WHITE)
    }

    /// Resolve the four colors consumed while drawing one standard item.
    /// Explicit item RGB1/RGB2/RGB3 values control its mark, name, and
    /// command. Missing components share the title or menu-bar RGB3 default;
    /// RGB4 is the item background and otherwise follows the menu background.
    pub(crate) fn item_colors(self, menu_id: i16, item: i16) -> StandardMenuItemColors {
        let fallback = self
            .entry_rgb(menu_id, 0, 16)
            .or_else(|| self.entry_rgb(0, 0, 16))
            .unwrap_or(STANDARD_MENU_BLACK);
        StandardMenuItemColors {
            mark: self.entry_rgb(menu_id, item, 4).unwrap_or(fallback),
            name: self.entry_rgb(menu_id, item, 10).unwrap_or(fallback),
            command: self.entry_rgb(menu_id, item, 16).unwrap_or(fallback),
            background: self
                .entry_rgb(menu_id, item, 22)
                .unwrap_or_else(|| self.dropdown_background(menu_id)),
        }
    }

    /// RGB midpoint requested through `GetGray` for unavailable content.
    pub(crate) fn dimmed(foreground: [u16; 3], background: [u16; 3]) -> [u16; 3] {
        std::array::from_fn(|channel| {
            ((u32::from(foreground[channel]) + u32::from(background[channel])) / 2) as u16
        })
    }

    fn entry_rgb(self, menu_id: i16, item: i16, offset: usize) -> Option<[u16; 3]> {
        let entry = self
            .bytes
            .chunks_exact(MENU_COLOR_ENTRY_SIZE)
            .find(|entry| menu_color_entry_key(entry) == Some((menu_id, item)))?;
        Some([
            read_u16(entry, offset)?,
            read_u16(entry, offset.checked_add(2)?)?,
            read_u16(entry, offset.checked_add(4)?)?,
        ])
    }
}

fn menu_color_entry_key(entry: &[u8]) -> Option<(i16, i16)> {
    Some((read_u16(entry, 0)? as i16, read_u16(entry, 2)? as i16))
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

    /// Find a title-bearing menu by ID without searching the hierarchical
    /// partition. `HiliteMenu` accepts only a menu that has a menu-bar title;
    /// a submenu ID therefore clears the current title just like an unknown
    /// ID. Macintosh Toolbox Essentials (1992), p. 3-119.
    pub(crate) fn find_regular_handle_by_id(
        &self,
        requested_id: i16,
        mut menu_id: impl FnMut(u32) -> Option<i16>,
    ) -> Option<u32> {
        self.regular_handles()
            .find(|handle| menu_id(*handle) == Some(requested_id))
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
    pub(crate) fn regular_title_regions(&self) -> Vec<MenuBarTitleRegion> {
        self.regular
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let right = self
                    .regular
                    .get(index + 1)
                    .map(|next| next.value)
                    .unwrap_or(self.last_right);
                MenuBarTitleRegion {
                    handle: entry.handle,
                    left: entry.value,
                    right,
                }
            })
            .collect()
    }

    pub(crate) fn regular_title_at_horizontal(&self, horizontal: i16) -> Option<(u32, i16)> {
        self.regular_title_regions()
            .into_iter()
            .find(|region| region.contains_horizontal(horizontal))
            .map(|region| (region.handle, region.left))
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

        let decoded = self.decoded_menu_partitions(&mut decode_menu);
        let regular = &decoded.regular;
        let hierarchical = &decoded.hierarchical;

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
            Self::find_owning_regular_menu(regular, hierarchical, |handle, _menu| {
                handle == menu_handle
            })
            .map(|(handle, _id)| handle)
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

    /// Resolve the regular menu-bar title that owns an installed menu ID.
    ///
    /// `TheMenu` contains the selected submenu's ID, while `DrawMenuBar` and
    /// `FlashMenuBar` keep the regular title that opened its hierarchy
    /// highlighted. The traversal is bounded against malformed circular
    /// hierarchies. Macintosh Toolbox Essentials (1992), pp. 3-115--3-119,
    /// 3-138, and 3-142.
    pub(crate) fn owning_regular_menu(
        &self,
        target_id: i16,
        mut decode_menu: impl FnMut(u32) -> Option<MenuKeyMenu>,
    ) -> Option<(u32, i16)> {
        let decoded = self.decoded_menu_partitions(&mut decode_menu);
        Self::find_owning_regular_menu(&decoded.regular, &decoded.hierarchical, |_handle, menu| {
            menu.id == target_id
        })
    }

    fn decoded_menu_partitions(
        &self,
        decode_menu: &mut impl FnMut(u32) -> Option<MenuKeyMenu>,
    ) -> DecodedMenuPartitions {
        let regular = self
            .regular_handles()
            .filter_map(|handle| decode_menu(handle).map(|menu| (handle, menu)))
            .collect();
        let hierarchical = self
            .hierarchical_handles()
            .filter_map(|handle| decode_menu(handle).map(|menu| (handle, menu)))
            .collect();
        DecodedMenuPartitions {
            regular,
            hierarchical,
        }
    }

    fn find_owning_regular_menu(
        regular: &[(u32, MenuKeyMenu)],
        hierarchical: &[(u32, MenuKeyMenu)],
        mut is_target: impl FnMut(u32, &MenuKeyMenu) -> bool,
    ) -> Option<(u32, i16)> {
        for (root_handle, root) in regular {
            let mut pending = vec![*root_handle];
            let mut visited = Vec::new();
            while let Some(handle) = pending.pop() {
                if visited.contains(&handle) {
                    continue;
                }
                visited.push(handle);
                let menu = regular
                    .iter()
                    .chain(hierarchical.iter())
                    .find(|(candidate, _menu)| *candidate == handle)
                    .map(|(_handle, menu)| menu)?;
                if is_target(handle, menu) {
                    return Some((*root_handle, root.id));
                }
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
    fn cloned_menu_tracking_owner_is_a_detached_runtime_snapshot() {
        let mut live = SharedMenuTracking::default();
        *live = Some(test_process_menu_tracking(0x0012_3456));

        let mut snapshot = live.clone();
        snapshot.as_mut().unwrap().highlighted_item = 4;
        snapshot.as_mut().unwrap().menu_handle = 0x0065_4321;

        assert_eq!(live.as_ref().unwrap().highlighted_item, 1);
        assert_eq!(live.as_ref().unwrap().menu_handle, 0x0012_3456);
        assert_eq!(snapshot.as_ref().unwrap().highlighted_item, 4);
        assert_eq!(snapshot.as_ref().unwrap().menu_handle, 0x0065_4321);
    }

    #[test]
    fn native_menu_selection_distinguishes_snapshots_from_live_cpu_handles() {
        let mut live = SharedNativeMenuSelection::default();
        assert!(live.stage((128, 2)));

        let mut snapshot = live.clone();
        assert_eq!(snapshot.take(), Some((128, 2)));
        assert_eq!(live.snapshot(), Some((128, 2)));

        let mut powerpc = live.shared_handle();
        assert_eq!(powerpc.take(), Some((128, 2)));
        assert!(live.is_none());
    }

    #[test]
    fn menu_flash_count_expands_to_visible_and_hidden_phases() {
        assert_eq!(menu_flash_phase_count(0), 0);
        assert_eq!(menu_flash_phase_count(1), 2);
        assert_eq!(menu_flash_phase_count(3), 6);
        assert_eq!(menu_flash_phase_count(u16::MAX), 131_070);

        let mut tracking = tracking_with_child(1u32, 2);
        assert!(tracking.begin_flash(1, 0x0080_0002));
        assert_eq!(tracking.flash_remaining, 2);
        assert_eq!(tracking.flash_delay, STANDARD_MENU_FLASH_PHASE_DELAY);
        assert_eq!(tracking.flash_result, 0x0080_0002);
        assert!(!tracking.begin_flash(0, 0x0080_0003));
        assert_eq!(tracking.flash_remaining, 0);
        assert_eq!(tracking.flash_delay, 0);
        assert_eq!(tracking.flash_result, 0x0080_0003);
    }

    #[test]
    fn menu_definition_call_preserves_the_shared_five_argument_contract() {
        let invocation = MenuDefinitionInvocation {
            message: MenuDefinitionMessage::Choose,
            menu_handle: 0x1111_2222,
            menu_rect: (-1, 2, 300, 400),
            hit_point: 0x5555_6666,
            which_item: 7,
        };
        let call = invocation.call(0x3333_4444);
        assert_eq!(
            call.native_arguments(),
            [1, 0x1111_2222, 0x3333_4444, 0x5555_6666, 0x3333_444c]
        );
        assert_eq!(
            invocation.scratch_bytes(),
            [0xff, 0xff, 0, 2, 1, 44, 1, 144, 0, 7]
        );
        assert_eq!(
            MenuDefinitionInvocation::size(0x1234).message,
            MenuDefinitionMessage::Size
        );
        assert_eq!(MenuDefinitionMessage::Draw as i16, 0);
        assert_eq!(MenuDefinitionMessage::PopUp as i16, 3);
    }

    #[test]
    fn menu_bar_build_owns_ordered_sizing_and_completion() {
        let mut build = MenuBarBuild::new(0x1111u32, vec![0x2222, 0x3333]);
        assert_eq!(build.next_step(), Some(MenuBarBuildStep::Size(0x2222)));
        assert_eq!(build.next_step(), Some(MenuBarBuildStep::Size(0x3333)));
        assert_eq!(build.next_step(), Some(MenuBarBuildStep::Complete(0x1111)));
        assert_eq!(build.next_step(), None);
    }

    #[test]
    fn custom_menu_definition_tracking_owns_draw_and_choose_order() {
        let rect = (20, 11, 84, 171);
        let mut tracking = MenuDefinitionTracking::begin_draw(0x1111_2222, rect);
        assert_eq!(
            tracking.pending_invocation(),
            Some(MenuDefinitionInvocation {
                message: MenuDefinitionMessage::Draw,
                menu_handle: 0x1111_2222,
                menu_rect: rect,
                hit_point: 0,
                which_item: 0,
            })
        );
        assert_eq!(tracking.choose(0x0030_0040), None);

        let draw_result =
            MenuDefinitionInvocation::decode_result([0, 20, 0, 11, 0, 84, 0, 171, 0, 0]);
        assert_eq!(
            tracking.complete_pending(draw_result),
            Some(MenuDefinitionMessage::Draw)
        );

        let choose = tracking.choose(0x0030_0040).unwrap();
        assert_eq!(choose.message, MenuDefinitionMessage::Choose);
        assert_eq!(choose.menu_rect, rect);
        assert_eq!(choose.which_item, 0);
        assert_eq!(tracking.choose(0x0030_0040), None);

        let choose_result =
            MenuDefinitionInvocation::decode_result([0, 20, 0, 11, 0, 84, 0, 171, 0, 3]);
        assert_eq!(
            tracking.complete_pending(choose_result),
            Some(MenuDefinitionMessage::Choose)
        );
        assert_eq!(tracking.which_item(), 3);
        assert_eq!(tracking.menu_handle(), 0x1111_2222);
        assert_eq!(tracking.choose(0x0030_0040), None);
        let hidden = tracking.flash(false).unwrap();
        assert_eq!(hidden.which_item, 3);
        assert_eq!(hidden.hit_point, 0x0013_000B);
        tracking.complete_pending(MenuDefinitionResult {
            menu_rect: rect,
            which_item: 0,
        });
        let visible = tracking.flash(true).unwrap();
        assert_eq!(visible.which_item, 0);
        assert_eq!(visible.hit_point, 0x0030_0040);
    }

    #[test]
    fn custom_popup_definition_returns_geometry_before_draw() {
        let mut tracking = MenuDefinitionTracking::begin_popup(0x1234, 0x0064_0050, 4);
        assert_eq!(
            tracking.pending_invocation().unwrap().message,
            MenuDefinitionMessage::PopUp
        );
        assert_eq!(
            tracking.pending_invocation().unwrap().hit_point,
            0x0064_0050
        );
        assert_eq!(tracking.pending_invocation().unwrap().which_item, 4);
        let result = MenuDefinitionInvocation::decode_result([0, 40, 0, 30, 0, 120, 0, 150, 0, 2]);
        assert_eq!(
            tracking.complete_pending(result),
            Some(MenuDefinitionMessage::PopUp)
        );
        assert_eq!(tracking.menu_rect(), (40, 30, 120, 150));
        assert_eq!(tracking.which_item(), 2);
        let draw = tracking.draw().unwrap();
        assert_eq!(draw.message, MenuDefinitionMessage::Draw);
        assert_eq!(draw.menu_rect, (40, 30, 120, 150));
        assert_eq!(draw.which_item, 2);
    }

    fn menu_color_entry(menu_id: i16, item: i16, seed: u8) -> [u8; MENU_COLOR_ENTRY_SIZE] {
        let mut entry = [seed; MENU_COLOR_ENTRY_SIZE];
        entry[0..2].copy_from_slice(&menu_id.to_be_bytes());
        entry[2..4].copy_from_slice(&item.to_be_bytes());
        entry
    }

    #[test]
    fn compiled_menu_colors_share_decode_merge_and_filter_semantics() {
        let first = menu_color_entry(128, 0, 0x11);
        let replacement = menu_color_entry(128, 0, 0x22);
        let second = menu_color_entry(128, 2, 0x33);
        let terminator = menu_color_entry(MENU_COLOR_END_ID, 0, 0x44);
        let mut resource = 4i16.to_be_bytes().to_vec();
        resource.extend_from_slice(&replacement);
        resource.extend_from_slice(&terminator);
        resource.extend_from_slice(&second);
        resource.extend_from_slice(&[0; 8]);

        let decoded = compiled_menu_color_entries(&resource);
        assert_eq!(
            decoded,
            [replacement.as_slice(), second.as_slice()].concat()
        );

        let merged = merge_menu_color_entries(&first, &decoded);
        assert_eq!(merged, [replacement.as_slice(), second.as_slice()].concat());
        assert_eq!(
            filter_menu_color_entries(&merged, |menu_id, item| menu_id == 128 && item == 2),
            second
        );
    }

    #[test]
    fn live_menu_colors_share_mbdf_and_mdef_fallbacks() {
        let rgb = |seed: u16| [seed, seed.wrapping_add(1), seed.wrapping_add(2)];
        let colored_entry =
            |menu_id: i16, item: i16, colors: [[u16; 3]; 4]| -> [u8; MENU_COLOR_ENTRY_SIZE] {
                let mut entry = [0; MENU_COLOR_ENTRY_SIZE];
                entry[0..2].copy_from_slice(&menu_id.to_be_bytes());
                entry[2..4].copy_from_slice(&item.to_be_bytes());
                for (offset, color) in [4usize, 10, 16, 22].into_iter().zip(colors) {
                    for (channel, value) in color.into_iter().enumerate() {
                        let channel_offset = offset + channel * 2;
                        entry[channel_offset..channel_offset + 2]
                            .copy_from_slice(&value.to_be_bytes());
                    }
                }
                entry
            };
        let menu_bar = colored_entry(0, 0, [rgb(0x1000), rgb(0x2000), rgb(0x3000), rgb(0x4000)]);
        let title = colored_entry(128, 0, [rgb(0x5000), rgb(0x6000), rgb(0x7000), rgb(0x8000)]);
        let item = colored_entry(128, 2, [rgb(0x9000), rgb(0xa000), rgb(0xb000), rgb(0xc000)]);
        let bytes = [menu_bar.as_slice(), title.as_slice(), item.as_slice()].concat();
        let colors = MenuColorTable::new(&bytes);

        assert_eq!(colors.menu_bar_background(), rgb(0x4000));
        assert_eq!(colors.title_foreground(128), rgb(0x5000));
        assert_eq!(colors.title_background(128), rgb(0x6000));
        assert_eq!(colors.dropdown_background(128), rgb(0x8000));
        assert_eq!(
            colors.item_colors(128, 2),
            StandardMenuItemColors {
                mark: rgb(0x9000),
                name: rgb(0xa000),
                command: rgb(0xb000),
                background: rgb(0xc000),
            }
        );
        assert_eq!(
            colors.item_colors(128, 1),
            StandardMenuItemColors {
                mark: rgb(0x7000),
                name: rgb(0x7000),
                command: rgb(0x7000),
                background: rgb(0x8000),
            }
        );
        assert_eq!(colors.title_foreground(129), rgb(0x1000));
        assert_eq!(colors.title_background(129), rgb(0x4000));
        assert_eq!(colors.dropdown_background(129), rgb(0x2000));
        assert_eq!(
            MenuColorTable::dimmed([0, 0x4000, u16::MAX], [u16::MAX, 0x8000, 0]),
            [0x7fff, 0x6000, 0x7fff],
        );

        let defaults = MenuColorTable::new(&[]);
        assert_eq!(defaults.menu_bar_background(), STANDARD_MENU_WHITE);
        assert_eq!(defaults.title_foreground(128), STANDARD_MENU_BLACK);
        assert_eq!(defaults.title_background(128), STANDARD_MENU_WHITE);
        assert_eq!(defaults.dropdown_background(128), STANDARD_MENU_WHITE);
        assert_eq!(standard_menu_highlighted_value(3, 3, 6), 6);
        assert_eq!(standard_menu_highlighted_value(6, 3, 6), 3);
        assert_eq!(standard_menu_highlighted_value(8, 3, 6), 8);
    }

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
            definition: None,
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
                definition: None,
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
    fn menu_list_installation_allocates_or_preserves_current_handle_atomically() {
        let source = MenuList {
            last_right: 91,
            mb_res_id: 128,
            regular: vec![MenuListEntry {
                handle: 0x1234_5678,
                value: 17,
            }],
            ..MenuList::default()
        };
        let expected = source.encode();

        let allocated = install_menu_list_copy(0, &source, |request| match request {
            MenuListInstallRequest::Allocate { bytes } => {
                assert_eq!(bytes, expected);
                Ok::<u32, ()>(0x1000)
            }
            MenuListInstallRequest::Replace { .. } => panic!("missing list must allocate"),
        })
        .expect("allocate current list");
        assert_eq!(
            allocated,
            MenuListInstallation {
                handle: 0x1000,
                allocated: true,
            }
        );

        let replaced = install_menu_list_copy(0x2000, &source, |request| match request {
            MenuListInstallRequest::Replace { handle, bytes } => {
                assert_eq!(handle, 0x2000);
                assert_eq!(bytes, expected);
                Ok::<u32, ()>(handle)
            }
            MenuListInstallRequest::Allocate { .. } => {
                panic!("existing current-list Handle must be preserved")
            }
        })
        .expect("replace current list");
        assert_eq!(
            replaced,
            MenuListInstallation {
                handle: 0x2000,
                allocated: false,
            }
        );

        let failed = install_menu_list_copy(0, &source, |_request| Err::<u32, _>(-108));
        assert_eq!(failed, Err(-108));
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
                    vec![
                        MenuKeyItem {
                            command: b'H',
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
        assert_eq!(list.owning_regular_menu(20, decode), Some((20, 20)));
        assert_eq!(list.owning_regular_menu(40, decode), Some((10, 10)));
        assert_eq!(list.owning_regular_menu(30, decode), None);
        assert_eq!(
            list.owning_regular_menu(999, decode),
            None,
            "a circular hierarchy must terminate without inventing an owner"
        );
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
    fn standard_menu_text_measurement_is_shared_between_gateways() {
        // The frozen 68040 and 604 profiles both measure "Three" as
        // T6+h8+r6+e8+e8 = 36 pixels in the Roman system font. The standard
        // MDEF then adds its 32-pixel non-indicator columns.
        assert_eq!(standard_menu_text_advance(b"Three"), 36);
        assert_eq!(
            standard_menu_width([StandardMenuItemWidth {
                text: standard_menu_text_advance(b"Three"),
                icon: 0,
                command: 0,
            }]),
            68
        );

        assert!(is_standard_system_menu_title(&[0x14]));
        assert!(is_standard_system_menu_title(&[0xF0]));
        assert!(!is_standard_system_menu_title(b"File"));
        assert_eq!(standard_menu_title_advance(&[0x14]), 11);
        assert_eq!(standard_menu_title_advance(&[0xF0]), 11);
        assert_eq!(
            standard_menu_title_advance(b"File"),
            standard_menu_text_advance(b"File")
        );
    }

    #[test]
    fn standard_menu_item_pixel_anchors_are_shared_between_gateways() {
        let plain = standard_menu_item_layout(
            (11, 111),
            (20, 16),
            StandardMenuIconKind::None,
            false,
            (9, 3),
            true,
        );
        assert_eq!(plain.mark_left, 14);
        assert_eq!(plain.icon_left, 13);
        assert_eq!(plain.text_left, 26);
        assert_eq!(plain.text_baseline, 30);
        assert_eq!(plain.separator_y, 27);
        assert_eq!(plain.indicator_left, 99);
        assert_eq!(plain.command_left, 86);
        assert_eq!(plain.indicator_mid_y, 28);

        let marked_color = standard_menu_item_layout(
            (11, 111),
            (40, 21),
            StandardMenuIconKind::Color {
                width: 24,
                height: 20,
            },
            true,
            (9, 3),
            false,
        );
        assert_eq!(marked_color.icon_left, 29);
        assert_eq!(marked_color.text_left, 53);
        assert_eq!(marked_color.text_baseline, 53);

        let normal = standard_menu_item_layout(
            (11, 111),
            (40, 34),
            StandardMenuIconKind::Normal,
            true,
            (9, 3),
            false,
        );
        assert_eq!(normal.icon_left, 29);
        assert_eq!(normal.text_left, 62);
    }

    #[test]
    fn standard_menu_symbol_rasters_and_gray_phase_are_shared_between_gateways() {
        let mut hierarchy = Vec::new();
        for_each_standard_hierarchy_indicator_pixel(40, 30, |x, y| {
            hierarchy.push((x, y));
        });
        assert_eq!(hierarchy.len(), 25);
        assert_eq!(
            (40..=46)
                .map(|x| hierarchy
                    .iter()
                    .filter(|(pixel_x, _)| *pixel_x == x)
                    .count())
                .collect::<Vec<_>>(),
            vec![1, 3, 5, 7, 5, 3, 1],
        );
        assert!(hierarchy.contains(&(40, 30)));
        assert!(hierarchy.contains(&(43, 27)));
        assert!(hierarchy.contains(&(43, 33)));
        assert!(hierarchy.contains(&(46, 30)));

        let mut scroll_up = Vec::new();
        for_each_standard_scroll_up_indicator_pixel(50, 10, |x, y| scroll_up.push((x, y)));
        assert_eq!(scroll_up.len(), 36);
        assert_eq!(
            (14..=19)
                .map(|y| scroll_up
                    .iter()
                    .filter(|(_, pixel_y)| *pixel_y == y)
                    .count())
                .collect::<Vec<_>>(),
            vec![1, 3, 5, 7, 9, 11],
        );

        let mut scroll_down = Vec::new();
        for_each_standard_scroll_down_indicator_pixel(50, 80, |x, y| {
            scroll_down.push((x, y));
        });
        assert_eq!(scroll_down.len(), 36);
        assert_eq!(
            (70..=75)
                .map(|y| {
                    scroll_down
                        .iter()
                        .filter(|(_, pixel_y)| *pixel_y == y)
                        .count()
                })
                .collect::<Vec<_>>(),
            vec![11, 9, 7, 5, 3, 1],
        );

        assert!(standard_menu_gray_pattern_is_ink(0, 0));
        assert!(!standard_menu_gray_pattern_is_ink(1, 0));
        assert!(!standard_menu_gray_pattern_is_ink(-1, 0));
        assert!(standard_menu_gray_pattern_is_ink(-1, 1));
    }

    #[test]
    fn standard_menu_icon_policy_is_shared_between_gateways() {
        assert_eq!(
            standard_menu_icon_kind(0, 0, Some((24, 20))),
            StandardMenuIconKind::None
        );
        assert_eq!(
            standard_menu_icon_kind(7, 0x1C, Some((24, 20))),
            StandardMenuIconKind::None,
            "the icon byte carries a script code for the $1C form"
        );
        assert_eq!(
            standard_menu_icon_kind(7, 0x1D, Some((24, 20))),
            StandardMenuIconKind::Color {
                width: 24,
                height: 20,
            },
            "a valid cicn takes priority over monochrome selectors"
        );
        assert_eq!(
            standard_menu_icon_kind(7, 0x1D, None),
            StandardMenuIconKind::Reduced
        );
        assert_eq!(
            standard_menu_icon_kind(7, 0x1E, None),
            StandardMenuIconKind::Small
        );
        assert_eq!(
            standard_menu_icon_kind(7, 0, None),
            StandardMenuIconKind::Normal
        );
        assert_eq!(
            standard_menu_icon_kind(7, 0x1B, None),
            StandardMenuIconKind::Normal,
            "the documented otherwise case uses a normal ICON"
        );
        assert_eq!(StandardMenuIconKind::Normal.width(), 32);
        assert_eq!(StandardMenuIconKind::Reduced.width(), 16);
        assert_eq!(
            StandardMenuIconKind::Color {
                width: 12,
                height: 24,
            }
            .width(),
            16
        );
        assert_eq!(
            StandardMenuIconKind::Normal.row_height(QuickDrawTextStyle::plain()),
            34
        );
        assert_eq!(
            StandardMenuIconKind::Reduced.row_height(QuickDrawTextStyle::plain()),
            16
        );
        assert_eq!(
            StandardMenuIconKind::Color {
                width: 16,
                height: 24,
            }
            .row_height(QuickDrawTextStyle::plain()),
            24
        );
        assert_eq!(standard_menu_icon_resource_id(7, 0), Some(263));
        assert_eq!(standard_menu_icon_resource_id(7, 0x1C), None);
    }

    #[test]
    fn monochrome_menu_icon_sampling_is_shared_between_gateways() {
        let mut icon = vec![0; 128];
        icon[0] = 0x40;

        let normal =
            MonochromeMenuIconLayout::for_kind(StandardMenuIconKind::Normal, Some(icon.len()))
                .expect("complete ICON");
        assert_eq!((normal.width, normal.height), (32, 32));
        assert_eq!(
            normal.sample_with(|offset| icon.get(offset).copied(), 1, 0),
            Some(true),
        );
        assert_eq!(
            normal.sample_with(|offset| icon.get(offset).copied(), 0, 0),
            Some(false),
        );

        let reduced =
            MonochromeMenuIconLayout::for_kind(StandardMenuIconKind::Reduced, Some(icon.len()))
                .expect("complete reduced ICON");
        assert_eq!((reduced.width, reduced.height), (16, 16));
        assert_eq!(
            reduced.sample_with(|offset| icon.get(offset).copied(), 0, 0),
            Some(true),
        );
        assert_eq!(
            reduced.sample_with(|offset| icon.get(offset).copied(), 1, 0),
            Some(false),
        );

        let mut small = vec![0; 32];
        small[0] = 0x80;
        let small_layout =
            MonochromeMenuIconLayout::for_kind(StandardMenuIconKind::Small, Some(small.len()))
                .expect("complete SICN image");
        assert_eq!(
            small_layout.sample_with(|offset| small.get(offset).copied(), 0, 0),
            Some(true),
        );
        assert_eq!(
            small_layout.sample_with(|offset| small.get(offset).copied(), 16, 0),
            None,
        );
        assert_eq!(
            MonochromeMenuIconLayout::for_kind(StandardMenuIconKind::Normal, Some(127)),
            None,
        );
        assert_eq!(
            MonochromeMenuIconLayout::for_kind(StandardMenuIconKind::Small, Some(31)),
            None,
        );
    }

    #[test]
    fn compiled_color_icon_layout_is_shared_between_gateways() {
        let mut bytes = vec![0; 104];
        let mut write_u16 = |offset: usize, value: u16| {
            bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        };
        write_u16(4, 2);
        write_u16(10, 1);
        write_u16(12, 16);
        write_u16(32, 1);
        write_u16(54, 2);
        write_u16(68, 2);
        write_u16(92, 0);

        let expected = ColorIconLayout {
            width: 16,
            height: 1,
            pixel_row_bytes: 2,
            mask_row_bytes: 2,
            bitmap_row_bytes: 2,
            pixel_size: 1,
            mask_offset: 82,
            bitmap_offset: 84,
            color_table_offset: 86,
            color_table_entries: 1,
            pixel_data_offset: 102,
        };
        assert_eq!(ColorIconLayout::decode(&bytes), Some(expected));
        assert_eq!(
            ColorIconLayout::decode_with(|offset| bytes.get(offset).copied(), Some(bytes.len())),
            Some(expected),
        );
        assert_eq!(ColorIconLayout::decode(&bytes[..103]), None);
        bytes[4..6].copy_from_slice(&0u16.to_be_bytes());
        assert_eq!(ColorIconLayout::decode(&bytes), None);

        let mut largest_table = vec![0; 86 + 8 + 65_536 * 8 + 2];
        largest_table[4..6].copy_from_slice(&2u16.to_be_bytes());
        largest_table[10..12].copy_from_slice(&1u16.to_be_bytes());
        largest_table[12..14].copy_from_slice(&16u16.to_be_bytes());
        largest_table[32..34].copy_from_slice(&1u16.to_be_bytes());
        largest_table[54..56].copy_from_slice(&2u16.to_be_bytes());
        largest_table[68..70].copy_from_slice(&2u16.to_be_bytes());
        largest_table[92..94].copy_from_slice(&u16::MAX.to_be_bytes());
        assert_eq!(
            ColorIconLayout::decode(&largest_table)
                .expect("the maximum compiled ColorTable remains representable")
                .color_table_entries,
            65_536,
        );
    }

    #[test]
    fn compiled_color_icon_sampling_is_shared_between_gateways() {
        let layout = ColorIconLayout {
            width: 2,
            height: 1,
            pixel_row_bytes: 1,
            mask_row_bytes: 1,
            bitmap_row_bytes: 1,
            pixel_size: 4,
            mask_offset: 0,
            bitmap_offset: 1,
            color_table_offset: 2,
            color_table_entries: 4,
            pixel_data_offset: 42,
        };
        let mut bytes = vec![0; 43];
        bytes[0] = 0x80;
        bytes[1] = 0x80;
        bytes[6..8].copy_from_slice(&0x8000u16.to_be_bytes());
        let entry = 10 + 3 * 8;
        bytes[entry..entry + 2].copy_from_slice(&99u16.to_be_bytes());
        bytes[entry + 2..entry + 4].copy_from_slice(&0x1234u16.to_be_bytes());
        bytes[entry + 4..entry + 6].copy_from_slice(&0x5678u16.to_be_bytes());
        bytes[entry + 6..entry + 8].copy_from_slice(&0x9abcu16.to_be_bytes());
        bytes[42] = 0x30;

        assert_eq!(
            layout.mask_bit_with(|offset| bytes.get(offset).copied(), 0, 0),
            Some(true)
        );
        assert_eq!(
            layout.mask_bit_with(|offset| bytes.get(offset).copied(), 1, 0),
            Some(false)
        );
        assert_eq!(
            layout.rgb_with(|offset| bytes.get(offset).copied(), 0, 0),
            Some([0x1234, 0x5678, 0x9abc]),
            "device tables use the ColorSpec ordinal instead of its value field"
        );
        assert_eq!(
            layout.monochrome_bit_with(|offset| bytes.get(offset).copied(), 0, 0),
            Some(true)
        );

        let direct = ColorIconLayout {
            width: 1,
            height: 1,
            pixel_row_bytes: 2,
            mask_row_bytes: 1,
            bitmap_row_bytes: 0,
            pixel_size: 16,
            mask_offset: 0,
            bitmap_offset: 1,
            color_table_offset: 1,
            color_table_entries: 0,
            pixel_data_offset: 1,
        };
        let direct_bytes = [0x80, 0x03, 0xe0];
        assert_eq!(
            direct.rgb_with(|offset| direct_bytes.get(offset).copied(), 0, 0),
            Some([0, 0xfffe, 0]),
            "direct RGB555 pixels expand to QuickDraw's 16-bit components"
        );
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
    fn resource_name_insertion_filters_sorts_deduplicates_and_uses_defaults() {
        let mut original = vec![0; 15];
        original[10..14].copy_from_slice(&u32::MAX.to_be_bytes());
        original.extend_from_slice(&[0]);
        let mut items = MenuItems::decode(&original).expect("decode empty menu");
        assert!(items.append_specs(b"Existing;Tail"));

        assert!(items.insert_resource_names(
            [
                b"Zulu".to_vec(),
                b".Hidden".to_vec(),
                b"Existing".to_vec(),
                b"alpha".to_vec(),
                b"%Metadata".to_vec(),
                b"Beta".to_vec(),
                b"alpha".to_vec(),
            ],
            1,
        ));
        assert_eq!(
            items
                .items
                .iter()
                .map(|item| item.text.as_slice())
                .collect::<Vec<_>>(),
            [
                b"Existing".as_slice(),
                b"alpha".as_slice(),
                b"Beta".as_slice(),
                b"Zulu".as_slice(),
                b"Tail".as_slice(),
            ]
        );
        for item in &items.items[1..4] {
            assert!(item.enabled);
            assert_eq!(
                (item.icon, item.command, item.mark, item.style),
                (0, 0, 0, 0)
            );
        }
        assert!(!items.insert_resource_names([b"Existing".to_vec()], i16::MAX));
    }

    #[test]
    fn menu_rows_share_offsets_boundaries_and_selectability_across_chrome_insets() {
        let plain = QuickDrawTextStyle::plain();
        let shadow = QuickDrawTextStyle::from_bits(QuickDrawTextStyle::SHADOW_BIT);
        assert_eq!(standard_menu_row_height(None, false, plain), 16);
        assert_eq!(standard_menu_row_height(None, true, plain), 34);
        assert_eq!(standard_menu_row_height(Some(25), true, plain), 25);
        assert_eq!(standard_menu_row_height(None, false, shadow), 21);
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
                menu_choice_item: 15,
                scrolled: false
            }
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (12, 130)),
            MenuPointerUpdate {
                item: 0,
                menu_choice_item: 0,
                scrolled: false
            }
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (12, 130)),
            MenuPointerUpdate {
                item: 0,
                menu_choice_item: 0,
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
                menu_choice_item: 0,
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
                menu_choice_item: 29,
                scrolled: false
            }
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (572, 130)),
            MenuPointerUpdate {
                item: 0,
                menu_choice_item: 0,
                scrolled: false
            }
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (572, 130)),
            MenuPointerUpdate {
                item: 0,
                menu_choice_item: 0,
                scrolled: true
            }
        );
        assert_eq!((content_top, content_top + rows.total_height()), (84, 724));
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (581, 130)),
            MenuPointerUpdate {
                item: 0,
                menu_choice_item: 0,
                scrolled: true
            }
        );
        assert_eq!((content_top, content_top + rows.total_height()), (68, 708));
    }

    #[test]
    fn standard_pointer_update_retains_disabled_row_for_menu_choice() {
        let rows = MenuRows::new([
            MenuRow {
                height: 16,
                selectable: true,
            },
            MenuRow {
                height: 6,
                selectable: false,
            },
            MenuRow {
                height: 16,
                selectable: false,
            },
        ]);
        let rect = (20, 40, 58, 140);
        let mut content_top = rect.0;
        let mut armed = None;

        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (38, 60)),
            MenuPointerUpdate {
                item: 0,
                menu_choice_item: 2,
                scrolled: false,
            },
        );
        assert_eq!(
            rows.track_pointer(rect, &mut content_top, &mut armed, (48, 60)),
            MenuPointerUpdate {
                item: 0,
                menu_choice_item: 3,
                scrolled: false,
            },
        );
        assert_eq!(menu_choice_value(0x0208, 3), 0x0208_0003);

        let disabled_menu_rows = MenuRows::new([MenuRow {
            height: 16,
            selectable: true,
        }])
        .with_menu_enabled(false);
        content_top = rect.0;
        assert_eq!(
            disabled_menu_rows.track_pointer(
                (20, 40, 36, 140),
                &mut content_top,
                &mut armed,
                (28, 60),
            ),
            MenuPointerUpdate {
                item: 0,
                menu_choice_item: 1,
                scrolled: false,
            },
            "a disabled menu stays laid out and observable without selecting its enabled row",
        );
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
            vec![
                MenuBarTitleRegion {
                    handle: 10,
                    left: 11,
                    right: 44,
                },
                MenuBarTitleRegion {
                    handle: 20,
                    left: 44,
                    right: 87,
                },
                MenuBarTitleRegion {
                    handle: 30,
                    left: 87,
                    right: 140,
                },
            ]
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
    fn standard_menu_bar_geometry_is_shared_between_gateways() {
        let region = MenuBarTitleRegion {
            handle: 0x1234,
            left: STANDARD_MENU_BAR_FIRST_TITLE_LEFT,
            right: 45,
        };

        assert!(!region.contains_horizontal(10));
        assert!(region.contains_horizontal(11));
        assert!(region.contains_horizontal(44));
        assert!(!region.contains_horizontal(45));
        assert_eq!(region.title_origin(), 18);
        assert_eq!(region.highlighted_rect(20), (1, 9, 19, 48));
        assert_eq!(standard_menu_bar_title_baseline(20, 11, 2), 14);
        assert_eq!(standard_menu_bar_system_mark_top(20, 11, 2), 4);
    }

    #[test]
    fn standard_menu_bar_corner_mask_is_shared_between_gateways() {
        let mut pixels = Vec::new();
        for_each_standard_menu_bar_corner_pixel(128, |x, y| pixels.push((x, y)));

        assert_eq!(pixels.len(), 24);
        assert_eq!(pixels[0], (0, 0));
        assert_eq!(pixels[1], (127, 0));
        assert!(pixels.contains(&(4, 0)));
        assert!(pixels.contains(&(123, 0)));
        assert!(pixels.contains(&(0, 4)));
        assert!(pixels.contains(&(127, 4)));
        assert!(!pixels.contains(&(5, 0)));
        assert!(!pixels.contains(&(3, 1)));
        assert!(!pixels.contains(&(1, 3)));
    }

    #[test]
    fn standard_menu_chrome_distinguishes_attached_and_detached_panes() {
        let rect = (20, 11, 52, 83);
        let pixels = |kind| {
            let chrome = StandardMenuChrome::new(kind, rect).unwrap();
            let mut frame = Vec::new();
            let mut shadow = Vec::new();
            chrome.for_each_frame_pixel(|x, y| frame.push((x, y)));
            chrome.for_each_shadow_pixel(|x, y| shadow.push((x, y)));
            (frame, shadow)
        };

        let (pull_down_frame, pull_down_shadow) = pixels(StandardMenuPaneKind::PullDown);
        assert!(!pull_down_frame.contains(&(12, 20)));
        assert!(pull_down_frame.contains(&(11, 20)));
        assert!(pull_down_frame.contains(&(82, 20)));
        assert!(pull_down_shadow.contains(&(83, 22)));
        assert!(!pull_down_shadow.contains(&(83, 21)));
        assert!(pull_down_shadow.contains(&(14, 52)));
        assert!(!pull_down_shadow.contains(&(13, 52)));

        let (hierarchy_frame, hierarchy_shadow) = pixels(StandardMenuPaneKind::Hierarchical);
        assert!(hierarchy_frame.contains(&(12, 20)));
        assert!(hierarchy_shadow.contains(&(83, 22)));

        let (popup_frame, popup_shadow) = pixels(StandardMenuPaneKind::PopUp);
        assert!(popup_frame.contains(&(12, 20)));
        assert!(!popup_shadow.contains(&(83, 22)));
        assert!(popup_shadow.contains(&(83, 23)));
    }

    #[test]
    fn standard_menu_chrome_rejects_empty_rectangles() {
        assert_eq!(
            StandardMenuChrome::new(StandardMenuPaneKind::PullDown, (20, 11, 20, 83)),
            None
        );
        assert_eq!(
            StandardMenuChrome::new(StandardMenuPaneKind::PopUp, (20, 11, 52, 11)),
            None
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
    fn both_architecture_reference_types_drop_custom_child_ownership_on_close() {
        let mut classic = tracking_with_child(0usize, 1usize);
        let mut classic_definition = MenuDefinitionTracking::begin_draw(0x1111, (20, 80, 52, 152));
        classic_definition.complete_pending(MenuDefinitionResult {
            menu_rect: (20, 80, 52, 152),
            which_item: 3,
        });
        classic.submenus[0].definition = Some(classic_definition);
        assert_eq!(classic.selection(|_, _| false), Some((1, 3)));
        let closed = classic.close_submenus_from(0);
        assert!(classic.active_definition().is_none());
        assert_eq!(closed[0].definition, Some(classic_definition));

        let mut powerpc = tracking_with_child(0x1000u32, 0x2000u32);
        let mut powerpc_definition = MenuDefinitionTracking::begin_draw(0x2000, (20, 80, 52, 152));
        powerpc_definition.complete_pending(MenuDefinitionResult {
            menu_rect: (20, 80, 52, 152),
            which_item: 3,
        });
        powerpc.submenus[0].definition = Some(powerpc_definition);
        assert_eq!(powerpc.selection(|_, _| false), Some((0x2000, 3)));
        let closed = powerpc.close_submenus_from(0);
        assert!(powerpc.active_definition().is_none());
        assert_eq!(closed[0].definition, Some(powerpc_definition));
    }

    #[test]
    fn both_architecture_reference_types_share_hierarchy_transitions() {
        let mut classic = tracking_with_child(0usize, 1usize);
        let classic_root = SubmenuRequest {
            parent: MenuTrackingPane::Root,
            parent_handle: 0,
            parent_item: 1,
            child_depth: 0,
        };
        assert!(matches!(
            classic.reconcile_submenu(classic_root, Some(1)),
            SubmenuReconciliation::Keep,
        ));
        let classic_child = SubmenuRequest {
            parent: MenuTrackingPane::Submenu(0),
            parent_handle: 1,
            parent_item: 2,
            child_depth: 1,
        };
        assert!(matches!(
            classic.reconcile_submenu(classic_child, Some(0)),
            SubmenuReconciliation::Closed { panes_deepest_first }
                if panes_deepest_first.is_empty()
        ));
        let token = match classic.reconcile_submenu(classic_child, Some(2)) {
            SubmenuReconciliation::Open {
                token,
                panes_deepest_first,
            } => {
                assert!(panes_deepest_first.is_empty());
                token
            }
            _ => panic!("classic replacement child did not stage an open"),
        };
        let child = TrackedMenuPane {
            menu_handle: 2,
            parent_item: 2,
            ..classic.submenus[0].clone()
        };
        assert_eq!(classic.install_submenu(token, child), Ok(1));
        classic.submenus.push(TrackedMenuPane {
            menu_handle: 3,
            parent_item: 2,
            ..classic.submenus[1].clone()
        });
        assert_eq!(
            classic.deepest_submenu_hit(|depth, _| Some(depth as i16 + 1)),
            Some((2, 3))
        );
        match classic.reconcile_submenu(classic_root, None) {
            SubmenuReconciliation::Closed {
                panes_deepest_first,
            } => assert_eq!(
                panes_deepest_first
                    .iter()
                    .map(|pane| pane.menu_handle)
                    .collect::<Vec<_>>(),
                vec![3, 2, 1],
            ),
            _ => panic!("missing classic child did not close the hierarchy"),
        }
        assert!(classic.submenus.is_empty());

        let mut powerpc = tracking_with_child(0x1000u32, 0x2000u32);
        let powerpc_root = SubmenuRequest {
            parent: MenuTrackingPane::Root,
            parent_handle: 0x1000,
            parent_item: 1,
            child_depth: 0,
        };
        assert!(matches!(
            powerpc.reconcile_submenu(powerpc_root, Some(0x2000)),
            SubmenuReconciliation::Keep,
        ));
        let powerpc_child = SubmenuRequest {
            parent: MenuTrackingPane::Submenu(0),
            parent_handle: 0x2000,
            parent_item: 2,
            child_depth: 1,
        };
        assert!(matches!(
            powerpc.reconcile_submenu(powerpc_child, Some(0x1000)),
            SubmenuReconciliation::Closed { panes_deepest_first }
                if panes_deepest_first.is_empty()
        ));
        let token = match powerpc.reconcile_submenu(powerpc_child, Some(0x3000)) {
            SubmenuReconciliation::Open {
                token,
                panes_deepest_first,
            } => {
                assert!(panes_deepest_first.is_empty());
                token
            }
            _ => panic!("PowerPC replacement child did not stage an open"),
        };
        let child = TrackedMenuPane {
            menu_handle: 0x3000,
            parent_item: 2,
            ..powerpc.submenus[0].clone()
        };
        powerpc.submenus[0].highlighted_item = 0;
        assert!(
            powerpc.install_submenu(token, child).is_err(),
            "a stale PowerPC parent accepted a staged child",
        );
        assert_eq!(powerpc.submenus.len(), 1);
    }

    #[test]
    fn both_architecture_reference_types_share_pointer_hierarchy_reduction() {
        let root_rows = MenuRows::new((0..3).map(|_| MenuRow {
            height: 16,
            selectable: true,
        }));
        let child_rows = MenuRows::new((0..3).map(|_| MenuRow {
            height: 16,
            selectable: true,
        }));
        let submenu_rows = [
            Some(child_rows),
            Some(root_rows.clone()),
            Some(root_rows.clone()),
        ];

        let mut classic = tracking_with_child(0usize, 1usize);
        classic.submenus.push(TrackedMenuPane {
            parent_item: 2,
            menu_handle: 2,
            popup_left: 198,
            ..classic.submenus[0].clone()
        });
        classic.submenus.push(TrackedMenuPane {
            parent_item: 1,
            menu_handle: 3,
            popup_left: 297,
            ..classic.submenus[0].clone()
        });
        let classic_update = classic
            .track_standard_pointer(&root_rows, &submenu_rows, (30, 110))
            .expect("classic standard pointer update");

        let mut powerpc = tracking_with_child(0x1000u32, 0x2000u32);
        powerpc.submenus.push(TrackedMenuPane {
            parent_item: 2,
            menu_handle: 0x3000,
            popup_left: 198,
            ..powerpc.submenus[0].clone()
        });
        powerpc.submenus.push(TrackedMenuPane {
            parent_item: 1,
            menu_handle: 0x4000,
            popup_left: 297,
            ..powerpc.submenus[0].clone()
        });
        let powerpc_update = powerpc
            .track_standard_pointer(&root_rows, &submenu_rows, (30, 110))
            .expect("PowerPC standard pointer update");

        assert_eq!(classic_update.pane, MenuTrackingPane::Submenu(0));
        assert_eq!(powerpc_update.pane, MenuTrackingPane::Submenu(0));
        assert_eq!(classic_update.menu_handle, 1);
        assert_eq!(powerpc_update.menu_handle, 0x2000);
        assert_eq!(classic_update.previous_item, powerpc_update.previous_item);
        assert_eq!(classic_update.pointer, powerpc_update.pointer);
        assert_eq!(classic_update.content_top, powerpc_update.content_top);
        assert_eq!(classic_update.content_bottom, powerpc_update.content_bottom);
        assert_eq!(classic_update.pointer.item, 1);
        assert_eq!(classic_update.closed_panes_deepest_first.len(), 2);
        assert_eq!(powerpc_update.closed_panes_deepest_first.len(), 2);
        assert_eq!(
            classic_update
                .closed_panes_deepest_first
                .iter()
                .map(|pane| pane.menu_handle)
                .collect::<Vec<_>>(),
            vec![3, 2],
        );
        assert_eq!(
            powerpc_update
                .closed_panes_deepest_first
                .iter()
                .map(|pane| pane.menu_handle)
                .collect::<Vec<_>>(),
            vec![0x4000, 0x3000],
        );
        assert_eq!(classic.submenus.len(), 1);
        assert_eq!(powerpc.submenus.len(), 1);
        assert_eq!(classic.submenus[0].highlighted_item, 1);
        assert_eq!(powerpc.submenus[0].highlighted_item, 1);
    }

    #[test]
    fn standard_pointer_reducer_leaves_custom_root_definition_ownership_untouched() {
        let rows = MenuRows::new([MenuRow {
            height: 16,
            selectable: true,
        }]);
        let mut tracking = tracking_with_child(0usize, 1usize);
        tracking.definition = Some(MenuDefinitionTracking::begin_draw(
            0x1234,
            tracking.dropdown_rect(),
        ));
        let before = tracking.clone();

        assert!(
            tracking
                .track_standard_pointer(&rows, &[Some(rows.clone())], (28, 40))
                .is_none(),
            "an application-defined root must retain pointer ownership",
        );
        assert_eq!(tracking, before);
    }
}
