//! Menu Manager trap handlers.

use crate::memory::SavedPixels;
use crate::cpu::{CpuOps, Register};
use crate::guest_call::{GuestCallTarget, MenuBarBuildResume, MenuBarCallOrigin, MenuTrackingCall, MenuTrackingOrigin};
use crate::guest_procedure::{resolve_guest_procedure, GuestIsa};
use crate::memory::{globals::addr, MacMemoryBus, MemoryBus};
use crate::menu_manager::{
    compiled_menu_color_entries, filter_menu_color_entries,
    for_each_standard_hierarchy_indicator_pixel, for_each_standard_scroll_down_indicator_pixel,
    for_each_standard_scroll_up_indicator_pixel, hierarchical_menu_id, install_menu_list_copy,
    laid_out_menu_item_count, menu_choice_value,
    merge_menu_color_entries as shared_merge_menu_color_entries, new_standard_menu_record,
    standard_menu_gray_pattern_is_ink, standard_menu_height as shared_standard_menu_height,
    standard_menu_highlighted_value, standard_menu_icon_kind, standard_menu_icon_resource_id,
    standard_menu_item_layout, standard_menu_text_advance as shared_standard_menu_text_advance,
    standard_menu_width as shared_standard_menu_width, standard_popup_menu_layout,
    standard_pull_down_menu_layout, standard_submenu_layout,
    ColorIconLayout as SharedColorIconLayout, MenuBarBuild as SharedMenuBarBuild,
    MenuBarResource as SharedMenuBarResource,
    MenuBarTitleRegion as SharedMenuBarTitleRegion, MenuColorTable,
    MenuDefinitionInvocation as SharedMenuDefinitionInvocation,
    MenuDefinitionPane as SharedMenuDefinitionPane,
    MenuDefinitionTracking as SharedMenuDefinitionTracking, MenuItems as SharedMenuItems,
    MenuKeyItem as SharedMenuKeyItem, MenuKeyMenu as SharedMenuKeyMenu, MenuList as SharedMenuList,
    MenuListInstallRequest, MenuRow as SharedMenuRow, MenuRows as SharedMenuRows,
    MenuSnapshotRecord as SharedMenuSnapshotRecord, MenuFlashStep, MenuTrackingKind, MenuTrackingPane,
    MonochromeMenuIconLayout as SharedMonochromeMenuIconLayout, ProcessMenuTrackingState,
    MenuTrackingRequest, PopupMenuRequest, ProcessTrackedMenuPane, StandardMenuChrome, StandardMenuIconKind, StandardMenuItemWidth,
    StandardMenuPaneKind, SubmenuReconciliation, SubmenuRequest, TrackedMenuPaneView,
    MAX_MENU_LIST_ENTRIES, MENU_COLOR_ENTRY_SIZE, STANDARD_MENU_BAR_FIRST_TITLE_LEFT,
    STANDARD_MENU_BAR_TITLE_SPACING, STANDARD_MENU_DEFINITION_SHIM, STANDARD_MENU_SEPARATOR_HEIGHT,
};
#[cfg(test)]
use crate::menu_manager::{parse_menu_item_specs, standard_menu_row_height};
use crate::menu_model::GuestMenuSnapshot;
use crate::quickdraw::text::QuickDrawTextStyle;
use crate::trap::types::encode_mac_roman_lossy;
use crate::ui_theme::UiThemeId;
use crate::Result;

/// A single menu item.
#[derive(Clone, Debug)]
pub struct MenuItem {
    pub text: String,
    pub icon: u8,
    pub key_equiv: u8,
    pub mark: u8,
    pub style: u8,
    pub enabled: bool,
}

/// A parsed menu with its items.
#[derive(Clone, Debug)]
pub struct Menu {
    pub id: i16,
    pub title: String,
    pub items: Vec<MenuItem>,
    pub enabled: bool,
    pub handle: u32,
    /// Presentation-cache membership derived from the guest MenuList.
    pub in_menu_bar: bool,
    /// Derived membership in the guest list's hierarchical/pop-up partition.
    pub hierarchical: bool,
    /// Derived regular-partition membership that contributes a visible title.
    /// Macintosh Toolbox Essentials 1992, p. 3-121.
    pub visible_in_menu_bar: bool,
}

pub(crate) type MenuTrackingState = ProcessMenuTrackingState;
pub(crate) type SubmenuTrackingState = ProcessTrackedMenuPane;

pub(crate) fn tracked_menu_state(
    kind: MenuTrackingKind,
    menu_handle: u32,
    rect: (i16, i16, i16, i16),
    saved_pixels: SavedPixels,
) -> MenuTrackingState {
    tracked_menu_state_with_content_top(kind, menu_handle, rect, rect.0, saved_pixels)
}

fn tracked_menu_state_with_content_top(
    kind: MenuTrackingKind,
    menu_handle: u32,
    rect: (i16, i16, i16, i16),
    content_top: i16,
    saved_pixels: SavedPixels,
) -> MenuTrackingState {
    let (popup_top, popup_left, popup_bottom, popup_right) = rect;
    MenuTrackingState {
        kind,
        menu_handle,
        popup_left,
        popup_top,
        content_top,
        scroll_direction: None,
        popup_width: popup_right.saturating_sub(popup_left),
        popup_height: popup_bottom.saturating_sub(popup_top),
        highlighted_item: 0,
        definition: None,
        flash_remaining: 0,
        flash_tick: None,
        flash_deadline: 0,
        flash_result: 0,
        saved_width: popup_right.saturating_sub(popup_left),
        saved_height: popup_bottom.saturating_sub(popup_top),
        front_buffer: None,
        saved_pixels: saved_pixels.map(u16::from),
        item_appearances: Vec::new(),
        submenus: Vec::new(),
    }
}

pub(crate) fn tracked_submenu_state(
    menu_handle: u32,
    parent_item: i16,
    rect: (i16, i16, i16, i16),
    saved_pixels: SavedPixels,
) -> SubmenuTrackingState {
    let (popup_top, popup_left, popup_bottom, popup_right) = rect;
    SubmenuTrackingState {
        parent_item,
        menu_handle,
        popup_left,
        popup_top,
        content_top: popup_top,
        scroll_direction: None,
        popup_width: popup_right.saturating_sub(popup_left),
        popup_height: popup_bottom.saturating_sub(popup_top),
        highlighted_item: 0,
        definition: None,
        saved_width: popup_right.saturating_sub(popup_left),
        saved_height: popup_bottom.saturating_sub(popup_top),
        front_buffer: None,
        saved_pixels: saved_pixels.map(u16::from),
        item_appearances: Vec::new(),
    }
}

#[cfg(test)]
pub(crate) fn test_tracked_menu_state(
    menu_handle: u32,
    rect: (i16, i16, i16, i16),
    highlighted_item: i16,
) -> MenuTrackingState {
    let mut state = tracked_menu_state(
        MenuTrackingKind::MenuBar,
        menu_handle,
        rect,
        Vec::new().into(),
    );
    state.highlighted_item = highlighted_item;
    state
}

// Macintosh Toolbox Essentials 1992, pp. 3-137 to 3-138: an icon number
// maps to resource ID icon+256; key-equivalent bytes $1D and $1E select
// reduced ICON and SICN menu icons instead of command-key shortcuts.
#[cfg(test)]
const MENU_KEY_REDUCED_ICON: u8 = 0x1D;
#[cfg(test)]
const MENU_KEY_SMALL_ICON: u8 = 0x1E;
const MENU_ROW_HEIGHT: i16 = 16;


/// Compute the size of a MENU resource in guest memory by scanning through it.
/// MENU format: menuID(2), menuWidth(2), menuHeight(2), menuProc(4), enableFlags(4),
/// title(pstring), then items: [text(pstring), icon(1), key(1), mark(1), style(1)]...
/// terminated by a 0-length item string.
/// Inside Macintosh Volume I, I-345
fn menu_resource_size(bus: &MacMemoryBus, ptr: u32) -> usize {
    // Fixed header: 14 bytes
    let mut offset = 14usize;
    // Title Pascal string
    let title_len = bus.read_byte(ptr + offset as u32) as usize;
    offset += 1 + title_len;
    // Items
    loop {
        let item_len = bus.read_byte(ptr + offset as u32) as usize;
        offset += 1;
        if item_len == 0 {
            break;
        }
        offset += item_len; // item text
        offset += 4; // icon, key, mark, style
    }
    offset
}

/// Read a per-item attribute byte from the MENU data in guest memory.
/// `item` is 1-based. `field_offset` selects which byte after the item
/// text: 0 = icon, 1 = key equivalent, 2 = mark character, 3 = style.
/// Inside Macintosh Volume I, I-345
fn get_menu_item_field(bus: &MacMemoryBus, menu_ptr: u32, item: i16, field_offset: u32) -> u8 {
    if item < 1 {
        return 0;
    }
    // Skip fixed header (14 bytes) + title Pascal string
    let title_len = bus.read_byte(menu_ptr + 14) as u32;
    let mut offset = 15 + title_len;
    let mut idx: i16 = 0;
    loop {
        let item_len = bus.read_byte(menu_ptr + offset) as u32;
        if item_len == 0 {
            break;
        }
        idx += 1;
        if idx == item {
            // item text starts at offset+1, attributes start at offset+1+item_len
            return bus.read_byte(menu_ptr + offset + 1 + item_len + field_offset);
        }
        offset += 1 + item_len + 4; // pstring + 4 attribute bytes
    }
    0
}

/// Count the complete items in the live MENU record through the shared
/// MenuInfo decoder. Inside Macintosh Volume I (1985), pp. I-345 and I-361.
fn count_menu_items_from_memory(bus: &MacMemoryBus, menu_handle: u32) -> u16 {
    menu_items_from_memory(bus, menu_handle)
        .map(|items| items.item_count())
        .unwrap_or(0)
}

/// Decode a guest menu string payload as Mac Roman text.
///
/// Menu titles and item text are Mac Roman byte strings (IM:I I-247), and
/// the glyph lookup treats a `char` in 0x80..=0xFF as the Mac Roman code
/// for that byte. Decoding is therefore a per-byte cast: interpreting the
/// payload as UTF-8 would fold every accent, ellipsis, bullet and symbol
/// byte into U+FFFD, which has no glyph, and drop it from the menu.
fn macroman_to_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| byte as char).collect()
}

/// Recover the guest Mac Roman bytes stored in the Menu Manager's internal
/// byte-preserving string representation.
pub(super) fn internal_menu_string_bytes(value: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(value.len());
    for ch in value.chars() {
        if (ch as u32) <= u8::MAX as u32 {
            bytes.push(ch as u8);
        } else {
            bytes.extend(encode_mac_roman_lossy(&ch.to_string()));
        }
    }
    bytes
}

/// Parse a MENU resource from guest memory into a Menu struct.
fn parse_menu_resource(bus: &MacMemoryBus, res_ptr: u32, handle: u32) -> Menu {
    let menu_id = bus.read_word(res_ptr) as i16;
    let enable_flags = bus.read_long(res_ptr + 10);

    let title_len = bus.read_byte(res_ptr + 14) as usize;
    let mut title_bytes = Vec::with_capacity(title_len);
    for i in 0..title_len {
        title_bytes.push(bus.read_byte(res_ptr + 15 + i as u32));
    }
    let title = macroman_to_string(&title_bytes);

    // Items start after the title Pascal string
    let mut offset = res_ptr + 15 + title_len as u32;
    let mut items = Vec::new();

    loop {
        let item_len = bus.read_byte(offset) as usize;
        if item_len == 0 {
            break;
        }
        let mut text_bytes = Vec::with_capacity(item_len);
        for i in 0..item_len {
            text_bytes.push(bus.read_byte(offset + 1 + i as u32));
        }
        let text = macroman_to_string(&text_bytes);
        offset += 1 + item_len as u32;

        let icon = bus.read_byte(offset);
        let key_equiv = bus.read_byte(offset + 1);
        let mark = bus.read_byte(offset + 2);
        let style = bus.read_byte(offset + 3);
        offset += 4;

        let item_index = items.len() + 1; // 1-based
        let enabled = if item_index <= 31 {
            (enable_flags & (1 << item_index)) != 0
        } else {
            true
        };

        items.push(MenuItem {
            text,
            icon,
            key_equiv,
            mark,
            style,
            enabled,
        });
    }

    Menu {
        id: menu_id,
        title,
        items,
        enabled: (enable_flags & 1) != 0,
        handle,
        in_menu_bar: false,
        hierarchical: false,
        visible_in_menu_bar: false,
    }
}

fn menu_key_menu_from_memory(bus: &MacMemoryBus, menu_handle: u32) -> Option<SharedMenuKeyMenu> {
    let menu_ptr = bus.read_long(menu_handle);
    if menu_ptr == 0 {
        return None;
    }
    let record_size = bus
        .get_alloc_size(menu_ptr)
        .unwrap_or_else(|| menu_resource_size(bus, menu_ptr) as u32);
    let menu = SharedMenuItems::decode(&bus.read_bytes(menu_ptr, record_size as usize))?;
    Some(SharedMenuKeyMenu {
        id: bus.read_word(menu_ptr) as i16,
        enabled: menu.enable_flags & 1 != 0,
        items: menu
            .items
            .into_iter()
            .map(|item| SharedMenuKeyItem {
                command: item.command,
                mark: item.mark,
                enabled: item.enabled,
            })
            .collect(),
    })
}

fn menu_items_from_memory(bus: &MacMemoryBus, menu_handle: u32) -> Option<SharedMenuItems> {
    let menu_ptr = bus.read_long(menu_handle);
    if menu_ptr == 0 {
        return None;
    }
    let record_size = bus
        .get_alloc_size(menu_ptr)
        .unwrap_or_else(|| menu_resource_size(bus, menu_ptr) as u32);
    SharedMenuItems::decode(&bus.read_bytes(menu_ptr, record_size as usize))
}

#[cfg(test)]
fn parse_appendmenu_items(bytes: &[u8]) -> Vec<MenuItem> {
    parse_menu_item_specs(bytes)
        .into_iter()
        .map(|item| MenuItem {
            text: macroman_to_string(&item.text),
            icon: item.icon,
            key_equiv: item.command,
            mark: item.mark,
            style: item.style,
            enabled: item.enabled,
        })
        .collect()
}

/// Refresh cached menu contents from the guest-owned MenuInfo record.
/// Applications and menu definition procedures can inspect and change this
/// record directly, so rendering or writing a cached copy back without first
/// observing it loses guest-visible item text and attributes.
fn refresh_menu_from_memory(bus: &MacMemoryBus, menu: &mut Menu) {
    if menu.handle == 0 {
        return;
    }
    let menu_ptr = bus.read_long(menu.handle);
    if menu_ptr == 0 {
        return;
    }
    let mut parsed = parse_menu_resource(bus, menu_ptr, menu.handle);
    // MenuInfo has enable bits only for items 1 through 31. Preserve the
    // cache-only state of later items because there is no guest field from
    // which it can be reconstructed.
    for (parsed_item, cached_item) in parsed
        .items
        .iter_mut()
        .skip(31)
        .zip(menu.items.iter().skip(31))
    {
        parsed_item.enabled = cached_item.enabled;
    }
    menu.id = parsed.id;
    menu.title = parsed.title;
    menu.items = parsed.items;
    menu.enabled = parsed.enabled;
}

fn looks_like_menu_ptr(bus: &MacMemoryBus, menu_ptr: u32) -> bool {
    if menu_ptr == 0 || menu_ptr + 15 >= bus.ram_size() {
        return false;
    }
    let title_len = bus.read_byte(menu_ptr + 14) as u32;
    if title_len > 63 || menu_ptr + 15 + title_len >= bus.ram_size() {
        return false;
    }

    true
}

fn looks_like_menu_handle(bus: &MacMemoryBus, handle: u32) -> bool {
    if handle == 0 {
        return false;
    }
    looks_like_menu_ptr(bus, bus.read_long(handle))
}

fn menu_list_from_memory(bus: &MacMemoryBus, menu_list_handle: u32) -> Option<SharedMenuList> {
    let list = bus.read_long(menu_list_handle);
    if list == 0 {
        return None;
    }
    let regular_bytes = usize::from(bus.read_word(list));
    if regular_bytes % 6 != 0 || regular_bytes / 6 > MAX_MENU_LIST_ENTRIES {
        return None;
    }
    let hierarchical_header = list.checked_add(6 + u32::try_from(regular_bytes).ok()?)?;
    let hierarchical_bytes = usize::from(bus.read_word(hierarchical_header));
    if hierarchical_bytes % 6 != 0 || hierarchical_bytes / 6 > MAX_MENU_LIST_ENTRIES {
        return None;
    }
    let byte_count = 12usize
        .checked_add(regular_bytes)?
        .checked_add(hierarchical_bytes)?;
    SharedMenuList::decode(&bus.read_bytes(list, byte_count))
}

/// Menu-color table entry size in compiled resources and guest memory.
///
/// Apple's `MenuCRsrc` stores an array of complete `MCEntry` records, including
/// the trailing reserved word.
const MC_ENTRY_SIZE: usize = MENU_COLOR_ENTRY_SIZE;
const MC_ALL_ITEMS: i16 = -98;

fn mc_entry_key(bytes: &[u8]) -> Option<(i16, i16)> {
    if bytes.len() < 4 {
        return None;
    }
    Some((
        i16::from_be_bytes([bytes[0], bytes[1]]),
        i16::from_be_bytes([bytes[2], bytes[3]]),
    ))
}

fn mc_entry_matches(bytes: &[u8], menu_id: i16, menu_item: i16) -> bool {
    mc_entry_key(bytes) == Some((menu_id, menu_item))
}

impl super::TrapDispatcher {
    fn menu_uses_standard_definition(&self, bus: &MacMemoryBus, menu_ptr: u32) -> bool {
        let handle = bus.read_long(menu_ptr + 6);
        self.loaded_handles
            .get(&handle)
            .is_some_and(|(_, resource_type, resource_id)| {
                *resource_type == *b"MDEF" && *resource_id == 0
            })
            || bus.read_bytes(bus.read_long(handle), STANDARD_MENU_DEFINITION_SHIM.len())
                == STANDARD_MENU_DEFINITION_SHIM
    }

    fn arm_menu_definition<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        invocation: SharedMenuDefinitionInvocation,
    ) -> bool {
        self.arm_menu_definition_to(cpu, bus, invocation, cpu.read_reg(Register::PC))
    }

    fn arm_menu_definition_to<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        invocation: SharedMenuDefinitionInvocation,
        return_pc: u32,
    ) -> bool {
        let menu_handle = invocation.menu_handle;
        let menu_ptr = bus.read_long(menu_handle);
        if menu_ptr == 0 || self.menu_uses_standard_definition(bus, menu_ptr) {
            return false;
        }
        let proc_ptr = bus.read_long(bus.read_long(menu_ptr + 6));
        let Some(procedure) =
            resolve_guest_procedure(bus, proc_ptr, 0, None, GuestIsa::M68k, GuestIsa::M68k)
        else {
            return false;
        };
        let proc_addr = match (procedure.isa, procedure.representation) {
            (GuestIsa::M68k, _) => procedure.entry,
            (
                GuestIsa::PowerPc,
                crate::guest_procedure::GuestProcedureRepresentation::RoutineDescriptor {
                    descriptor,
                    ..
                },
            ) if procedure.proc_info == SharedMenuDefinitionInvocation::PASCAL_PROC_INFO => {
                descriptor
            }
            _ => return false,
        };
        if !bus.is_guest_address_mapped(proc_addr, 2) {
            return false;
        }

        use crate::execution_m68k::M68kMenuDefinitionFrame;
        let final_sp = cpu.read_reg(Register::A7);
        let Some(entry) = final_sp.checked_sub(M68kMenuDefinitionFrame::RESERVATION) else {
            return false;
        };
        let scratch = entry + 60;
        let Some(mut frame) =
            M68kMenuDefinitionFrame::new(invocation.call(scratch), proc_addr, final_sp, return_pc == cpu.read_reg(Register::PC))
        else {
            return false;
        };
        let floor = frame.entry - M68kMenuDefinitionFrame::STACK_PREFIX;
        if !bus.is_guest_address_writable(floor, (final_sp - floor) as usize) {
            return false;
        }
        let menu_build = self.guest_calls.menu_bar_build();
        let tracking_root = self.menu_tracking.entry_id();
        let return_pc = if menu_build.is_some() {
            frame.trap_return(0xa9c0)
        } else if let Some(call) = tracking_root.and(self.menu_tracking.context().call) {
            frame.trap_return(if call.popup_request().is_some() {
                0xa80b
            } else {
                0xa93d
            })
        } else {
            return_pc
        };
        let completion = crate::menu_manager::MenuDefinitionCompletion::pending();
        if return_pc != cpu.read_reg(Register::PC) {
            if !self.guest_calls.begin_m68k_with_operation(
                GuestCallTarget { isa: GuestIsa::M68k, entry: proc_addr, rtoc: 0 },
                return_pc, final_sp, Some(frame.entry),
                Some(crate::guest_call::ManagerContinuation::Menu(
                    crate::guest_call::MenuManagerContinuation::Definition(
                        crate::menu_manager::MenuDefinitionOperation {
                            scratch, completion: completion.clone(),
                        },
                    ),
                )),
            ) { return false; }
            if let Some(id) = tracking_root {
                self.menu_tracking
                    .bind_completion(id, invocation, completion.clone());
            }
            if invocation.message == crate::menu_manager::MenuDefinitionMessage::Size {
                if let Some(id) = menu_build {
                    self.guest_calls.bind_menu_bar_build_completion(id, invocation.menu_handle, completion);
                }
            }
        }
        for (offset, byte) in frame
            .image
            .into_iter()
            .chain(invocation.scratch_bytes())
            .enumerate()
        {
            bus.write_byte(frame.entry + offset as u32, byte);
        }
        bus.write_long(frame.entry - 4, return_pc);
        cpu.write_reg(Register::A7, frame.entry - 4);
        cpu.write_reg(Register::PC, frame.entry);
        true
    }

    fn retire_menu_definition<C: CpuOps>(&self, cpu: &mut C, bus: &MacMemoryBus) -> bool {
        crate::execution_m68k::complete_classic_manager_return(&self.guest_calls, cpu, bus)
    }

    fn complete_pending_menu_definition<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &MacMemoryBus,
    ) -> Option<crate::menu_manager::MenuDefinitionMessage> {
        self.retire_menu_definition(cpu, bus);
        self.with_active_menu_definition_mut(|definition| definition.complete_callback())?
            .ok()
            .flatten()
    }

    fn arm_pending_menu_definition<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        return_pc: u32,
    ) -> bool {
        let invocation = self
            .active_menu_definition()
            .and_then(SharedMenuDefinitionTracking::pending_invocation);
        invocation
            .is_some_and(|invocation| self.arm_menu_definition_to(cpu, bus, invocation, return_pc))
    }

    fn active_menu_definition(&self) -> Option<&SharedMenuDefinitionTracking> {
        self.menu_tracking
            .as_ref()
            .and_then(MenuTrackingState::active_definition)
            .or(self.menu_tracking.context().definition.as_ref())
    }

    fn with_active_menu_definition_mut<R>(
        &self,
        update: impl FnOnce(&mut SharedMenuDefinitionTracking) -> R,
    ) -> Option<R> {
        if self
            .menu_tracking
            .as_ref()
            .and_then(MenuTrackingState::active_definition)
            .is_some()
        {
            return self
                .menu_tracking
                .with_tracking_mut(|tracking| tracking.active_definition_mut().map(update))
                .flatten();
        }
        self.menu_tracking
            .with_existing_context_mut(|context| context.definition.as_mut().map(update))
            .flatten()
    }

    fn clear_active_menu_definition(&mut self) {
        if self
            .menu_tracking
            .with_tracking_mut(|tracking| tracking.take_active_definition().is_some())
            .unwrap_or(false)
        {
            return;
        }
        self.menu_tracking
            .with_existing_context_mut(|context| context.definition = None);
    }

    pub(crate) fn preserve_menu_callback_port(&mut self, bus: &MacMemoryBus) {
        if self.menu_tracking.context().classic_port.is_none() {
            let snapshot = self.capture_current_port_state(bus);
            self.menu_tracking
                .with_context_mut(|context| context.classic_port = Some(snapshot));
        }
    }

    fn prepare_menu_definition_port<C: CpuOps>(&mut self, cpu: &mut C, bus: &mut MacMemoryBus) {
        if self.menu_tracking.context().classic_port.is_none() {
            let snapshot = self.capture_current_port_state(bus);
            self.menu_tracking
                .with_context_mut(|context| context.classic_port = Some(snapshot));
        }
        let port = self.ensure_color_window_manager_port(bus);
        self.set_current_port_state(bus, cpu, port, None);
    }

    fn restore_menu_definition_port<C: CpuOps>(&mut self, cpu: &mut C, bus: &mut MacMemoryBus) {
        if let Some(snapshot) = self
            .menu_tracking
            .with_existing_context_mut(|context| context.classic_port.take())
            .flatten()
        {
            self.restore_current_port_state(bus, cpu, &snapshot);
        }
    }

    fn abort_custom_menu_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        result_offset: u32,
    ) {
        self.clear_active_menu_definition();
        let kind = self.menu_tracking.as_ref().map(|tracking| tracking.kind);
        if let Some(saved) = self.menu_tracking.take() {
            self.restore_menu_tracking_pixels(bus, saved);
        }
        if kind == Some(MenuTrackingKind::MenuBar) {
            bus.write_word(addr::THE_MENU, 0);
            self.draw_menu_bar_to_fb(bus);
        } else {
            self.restore_visible_dialog_snapshots(bus);
        }
        self.restore_menu_definition_port(cpu, bus);
        self.finish_menu_no_hit(bus, cpu, self.menu_tracking.context().classic_stack(), result_offset);
    }

    fn finish_custom_menu_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        result_offset: u32,
        result: u32,
    ) {
        self.clear_active_menu_definition();
        let kind = self.menu_tracking.as_ref().map(|tracking| tracking.kind);
        if let Some(saved) = self.menu_tracking.take() {
            self.restore_menu_tracking_pixels(bus, saved);
        }
        if kind == Some(MenuTrackingKind::MenuBar) {
            bus.write_word(addr::THE_MENU, (result >> 16) as u16);
            self.draw_menu_bar_to_fb(bus);
        } else {
            self.restore_visible_dialog_snapshots(bus);
        }
        self.restore_menu_definition_port(cpu, bus);
        let sp = self.menu_tracking.context().classic_stack();
        bus.write_long(sp + result_offset, result);
        cpu.write_reg(Register::A7, sp + result_offset);
    }

    /// Copy the live, inserted Menu Manager state into the shared
    /// frontend-neutral representation. Classic applications and custom menu
    /// procedures can mutate guest-owned MenuInfo records directly, so this
    /// reads those records instead of the adapter's presentation cache.
    pub(crate) fn guest_menu_snapshot(&mut self, bus: &MacMemoryBus) -> GuestMenuSnapshot {
        self.current_menu_list(bus)
            .unwrap_or_default()
            .guest_snapshot(|menu_handle| {
                let menu = bus.read_long(menu_handle);
                if menu == 0 {
                    return None;
                }
                let title_len = usize::from(bus.read_byte(menu + 14));
                Some(SharedMenuSnapshotRecord {
                    id: bus.read_word(menu) as i16,
                    title: bus.read_bytes(menu + 15, title_len),
                    items: menu_items_from_memory(bus, menu_handle)?,
                })
            })
    }

    /// Validate and stage a host-native selection, returning a point inside
    /// the first visible title.  The frontend injects that point as a normal
    /// mouse click so the application follows its ordinary event loop.
    pub(crate) fn queue_native_menu_selection(
        &mut self,
        bus: &MacMemoryBus,
        menu_id: i16,
        item_number: i16,
    ) -> Option<(i16, i16)> {
        self.guest_menu_snapshot(bus)
            .selectable_result(menu_id, item_number)?;
        let region = self
            .current_menu_list(bus)?
            .regular_title_regions()
            .into_iter()
            .next()?;
        let selection = (menu_id, item_number);
        if self.pending_native_menu_selection.stage(selection) {
            let tick = self.current_tick();
            self.pending_native_menu_event = Some(super::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: tick,
                where_v: 10,
                where_h: region.left + (region.right - region.left) / 2,
                modifiers: self.current_event_modifiers(),
            });
            self.pending_native_menu_event_tick = None;
        }
        Some((10, region.left + (region.right - region.left) / 2))
    }

    /// Consume a staged selection only if it is still valid in the current
    /// guest menu list.  Menu contents can change while AppKit is tracking a
    /// native menu, so validation at enqueue time alone is insufficient.
    fn take_native_menu_selection(&mut self, bus: &MacMemoryBus) -> Option<u32> {
        let (menu_id, item_number) = self.pending_native_menu_selection.take()?;
        self.pending_native_menu_event = None;
        self.pending_native_menu_event_tick = None;
        self.guest_menu_snapshot(bus)
            .selectable_result(menu_id, item_number)
    }

    fn menu_tracking_button_down(&self, bus: &MacMemoryBus) -> bool {
        // Menu tracking ends on mouse-up; MBState ($0172) is the documented
        // low-memory mouse button state (0=down, $80=up). Fold it in with
        // dispatcher state so guest callbacks that run during tracking can
        // release or hold the button between trap re-fires. Inside Macintosh
        // Volume II, p. II-371; Macintosh Toolbox Essentials 1992, p. 3-120.
        self.input_state.mouse_button_pressed() || bus.read_byte(addr::MB_STATE) == 0x00
    }

    fn menu_tracking_mouse_pos(&self, bus: &MacMemoryBus) -> (i16, i16) {
        // Mouse ($0830) mirrors the current low-memory mouse Point used by
        // code that polls classic globals during tracking. Guest callbacks can
        // move this point between trap re-fires without going through the host
        // event dispatcher. Inside Macintosh Volume II, p. II-371; Volume III,
        // p. III-446.
        let v = bus.read_word(addr::MOUSE_LOC2) as i16;
        let h = bus.read_word(addr::MOUSE_LOC2 + 2) as i16;
        if v != 0 || h != 0 {
            (v, h)
        } else {
            self.input_state.mouse_position()
        }
    }

    fn menu_trace_menu_fields(&self, menu_idx: Option<usize>) -> String {
        match menu_idx.and_then(|idx| self.menus.get(idx).map(|menu| (idx, menu))) {
            Some((idx, menu)) => format!(
                "menu_index={} menu_id={} menu_title={:?}",
                idx, menu.id, menu.title
            ),
            None => "menu_index=none menu_id=none menu_title=none".to_string(),
        }
    }

    fn resolve_menu_handle_candidate(&self, bus: &MacMemoryBus, candidate: u32) -> Option<u32> {
        if looks_like_menu_handle(bus, candidate) {
            return Some(candidate);
        }

        if !looks_like_menu_ptr(bus, candidate) {
            return None;
        }

        self.handle_for_ptr(candidate)
            .or_else(|| {
                self.menus
                    .iter()
                    .find(|menu| bus.read_long(menu.handle) == candidate)
                    .map(|menu| menu.handle)
            })
            .filter(|handle| looks_like_menu_handle(bus, *handle))
    }

    fn record_menuselect_input_trace(
        &mut self,
        action: &str,
        start_pt: Option<(i16, i16)>,
        menu_idx: Option<usize>,
        highlighted_item: Option<i16>,
        result: Option<u32>,
        outcome: &str,
    ) {
        if !self.input_trace_enabled {
            return;
        }
        let start = start_pt
            .map(|(v, h)| format!("({v},{h})"))
            .unwrap_or_else(|| "none".to_string());
        let highlighted = highlighted_item
            .map(|item| item.to_string())
            .unwrap_or_else(|| "none".to_string());
        let result = result
            .map(|value| format!("${value:08X}"))
            .unwrap_or_else(|| "pending".to_string());
        // IM:I I-355 documents MenuSelect as a mouse-tracking call that
        // returns menu ID in the high word and item number in the low word.
        let (mouse_v, mouse_h) = self.input_state.mouse_position();
        self.record_input_trace_line(format!(
            "A93D action={} start={} live_mouse=({},{}) {} {} highlighted_item={} result={} outcome={}",
            action,
            start,
            mouse_v,
            mouse_h,
            self.input_trace_state_fields(),
            self.menu_trace_menu_fields(menu_idx),
            highlighted,
            result,
            outcome,
        ));
    }

    pub(crate) fn is_popup_menu_proc_id(proc_id: i16) -> bool {
        (1008..=1023).contains(&proc_id)
    }

    fn menu_def_proc_handle(&mut self, bus: &mut MacMemoryBus, mdef_id: i16) -> u32 {
        if let Some((refnum, ptr)) = self.find_or_load_resource_any(bus, *b"MDEF", mdef_id) {
            return self.get_or_create_resource_handle_in_file(bus, *b"MDEF", mdef_id, ptr, refnum);
        }

        self.synthesize_system_mdef(bus, mdef_id)
            .map(|ptr| self.get_or_create_resource_handle_in_file(bus, *b"MDEF", mdef_id, ptr, 0))
            .unwrap_or(0)
    }

    /// Materialize one resource-backed menu exactly as `GetMenu` does.
    /// Macintosh Toolbox Essentials (1992), pp. 3-106--3-107, 3-111--3-112.
    fn load_menu_resource(&mut self, bus: &mut MacMemoryBus, menu_id: i16) -> u32 {
        let Some((refnum, res_ptr)) = self.find_or_load_resource_any(bus, *b"MENU", menu_id) else {
            bus.write_word(0x0A60, (-192i16) as u16);
            return 0;
        };

        let handle =
            self.get_or_create_resource_handle_in_file(bus, *b"MENU", menu_id, res_ptr, refnum);
        if handle == 0 {
            bus.write_word(0x0A60, (-192i16) as u16);
            return 0;
        }

        let mut menu_ptr = bus.read_long(handle);
        if menu_ptr == 0 {
            menu_ptr = res_ptr;
            bus.write_long(handle, menu_ptr);
            self.track_handle_ptr(menu_ptr, handle);
        }
        if menu_ptr != 0 {
            if bus.get_alloc_size(menu_ptr).unwrap_or(0) < 256 {
                let resized = self.resize_resource_allocation(bus, handle, menu_ptr, 256);
                if resized != 0 {
                    menu_ptr = resized;
                }
            }
            let menu_proc_placeholder = bus.read_long(menu_ptr + 6);
            if !self.loaded_handles.contains_key(&menu_proc_placeholder) {
                let mdef_id = (menu_proc_placeholder >> 16) as u16 as i16;
                let menu_proc = self.menu_def_proc_handle(bus, mdef_id);
                if menu_proc == 0 {
                    bus.write_word(0x0A60, (-192i16) as u16);
                    return 0;
                }
                bus.write_long(menu_ptr + 6, menu_proc);
            }
            let mut parsed = parse_menu_resource(bus, menu_ptr, handle);
            if let Some(existing) = self.menus.iter_mut().find(|menu| menu.handle == handle) {
                parsed.in_menu_bar = existing.in_menu_bar;
                parsed.hierarchical = existing.hierarchical;
                parsed.visible_in_menu_bar = existing.visible_in_menu_bar;
                *existing = parsed;
            } else {
                self.menus.push(parsed);
            }
        }

        self.load_menu_color_resource(bus, menu_id);
        bus.write_word(0x0A60, 0);
        handle
    }

    fn calculate_standard_menu_size(&self, bus: &mut MacMemoryBus, menu_handle: u32) {
        let Some(menu) = self.menus.iter().find(|menu| menu.handle == menu_handle) else {
            return;
        };
        let menu_ptr = bus.read_long(menu_handle);
        if menu_ptr == 0 {
            return;
        }
        let (_base, _row_bytes, _screen_width, screen_height, _depth) = self.get_screen_params();
        let menu_height = shared_standard_menu_height(
            &self.menu_rows(bus, &menu.items),
            screen_height.saturating_sub(bus.read_word(addr::MBAR_HEIGHT) as i16),
        );
        let menu_width = self.standard_menu_width(bus, &menu.items);
        bus.write_word(menu_ptr + 2, menu_width as u16);
        bus.write_word(menu_ptr + 4, menu_height as u16);
    }

    pub(crate) fn resume_completed_menu_bar_build<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> bool {
        if self
            .guest_calls
            .ready_menu_bar_build(GuestIsa::M68k)
            .is_none()
        {
            return false;
        }
        self.continue_menu_bar_build(cpu, bus, false);
        true
    }

    fn continue_menu_bar_build<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        preserve_auto_pop: bool,
    ) {
        loop {
            let menu_handle = match self.guest_calls.advance_menu_bar_build(GuestIsa::M68k) {
                Some(MenuBarBuildResume::Size(handle)) => handle,
                Some(MenuBarBuildResume::Complete {
                    result,
                    origin:
                        MenuBarCallOrigin::M68k {
                            stack_pointer,
                            return_address,
                        },
                }) => {
                    bus.write_long(stack_pointer + 2, result);
                    cpu.write_reg(Register::A7, stack_pointer + 2);
                    cpu.write_reg(Register::PC, return_address);
                    if preserve_auto_pop {
                        self.preserve_auto_pop_pc_once = true;
                    }
                    return;
                }
                _ => return,
            };
            if self.arm_menu_definition_to(
                cpu,
                bus,
                SharedMenuDefinitionInvocation::size(menu_handle),
                cpu.read_reg(Register::PC).wrapping_sub(2),
            ) {
                if preserve_auto_pop {
                    self.preserve_auto_pop_pc_once = true;
                }
                return;
            }
            self.calculate_standard_menu_size(bus, menu_handle);
        }
    }

    pub(crate) fn popup_menu_item_title(
        &self,
        bus: &MacMemoryBus,
        menu_id: i16,
        selected: usize,
    ) -> Option<String> {
        if selected == 0 {
            return None;
        }

        self.menus
            .iter()
            .rev()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.items.get(selected - 1))
            .map(|item| item.text.clone())
            .or_else(|| {
                let (_, menu_ptr) = self.find_loaded_resource_any(*b"MENU", menu_id)?;
                Self::popup_menu_item_title_from_resource(bus, menu_ptr, selected)
            })
    }

    fn popup_menu_item_title_from_resource(
        bus: &MacMemoryBus,
        menu_ptr: u32,
        selected: usize,
    ) -> Option<String> {
        let title_len = bus.read_byte(menu_ptr + 14) as u32;
        let mut offset = menu_ptr + 15 + title_len;
        let mut nth = 0usize;
        loop {
            let item_len = bus.read_byte(offset) as usize;
            if item_len == 0 {
                break;
            }
            nth += 1;
            if nth == selected {
                let bytes = bus.read_bytes(offset + 1, item_len);
                return Some(macroman_to_string(&bytes));
            }
            offset += 1 + item_len as u32 + 4;
        }
        None
    }

    fn ensure_menu_color_table_handle(&mut self, bus: &mut MacMemoryBus) -> u32 {
        let current = bus.read_long(addr::MENU_C_INFO);
        if current != 0 {
            return current;
        }

        let handle = self.alloc_handle_with_bytes(bus, &[]);
        if handle != 0 {
            bus.write_long(addr::MENU_C_INFO, handle);
        }
        handle
    }

    fn menu_color_table_bytes(bus: &MacMemoryBus, handle: u32) -> Vec<u8> {
        if handle == 0 {
            return Vec::new();
        }

        let data_ptr = bus.read_long(handle);
        if data_ptr == 0 {
            return Vec::new();
        }

        let size = bus.get_alloc_size(data_ptr).unwrap_or(0);
        if size == 0 {
            Vec::new()
        } else {
            bus.read_bytes(data_ptr, size as usize)
        }
    }

    pub(super) fn live_menu_color_table_bytes(bus: &MacMemoryBus) -> Vec<u8> {
        Self::menu_color_table_bytes(bus, bus.read_long(addr::MENU_C_INFO))
    }

    fn menu_color_rgb_pixel_index(bus: &MacMemoryBus, rgb: [u16; 3]) -> Option<u8> {
        // Menu chrome is drawn directly into the main framebuffer even when
        // an application has selected an offscreen GDevice.
        Self::fb_main_screen_pixel_index_for_rgb(bus, rgb)
    }

    pub(super) fn menu_standard_pixel_index(
        bus: &MacMemoryBus,
        pixel_size: u16,
        black: bool,
    ) -> Option<u8> {
        if !matches!(pixel_size, 2 | 4 | 8) {
            return None;
        }
        Self::menu_color_rgb_pixel_index(bus, if black { [0; 3] } else { [0xFFFF; 3] })
    }

    fn menu_set_standard_pixel(
        bus: &mut MacMemoryBus,
        screen: (u32, u32, u16, i16, i16),
        x: i16,
        y: i16,
        black: bool,
    ) {
        let (screen_base, row_bytes, pixel_size, screen_width, screen_height) = screen;
        if let Some(pixel_index) = Self::menu_standard_pixel_index(bus, pixel_size, black) {
            Self::fb_set_pixel_index(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                pixel_index,
            );
        } else {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                black,
            );
        }
    }

    pub(super) fn menu_bar_background_pixel_index(
        &self,
        bus: &MacMemoryBus,
        pixel_size: u16,
    ) -> Option<u8> {
        if !matches!(pixel_size, 2 | 4 | 8) {
            return None;
        }

        let bytes = Self::live_menu_color_table_bytes(bus);
        Self::menu_color_rgb_pixel_index(bus, MenuColorTable::new(&bytes).menu_bar_background())
    }

    pub(super) fn menu_title_pixel_index(
        bus: &MacMemoryBus,
        menu_id: i16,
        pixel_size: u16,
    ) -> Option<u8> {
        if !matches!(pixel_size, 2 | 4 | 8) {
            return None;
        }

        let bytes = Self::live_menu_color_table_bytes(bus);
        Self::menu_color_rgb_pixel_index(bus, MenuColorTable::new(&bytes).title_foreground(menu_id))
    }

    pub(super) fn menu_title_background_pixel_index(
        &self,
        bus: &MacMemoryBus,
        menu_id: i16,
        pixel_size: u16,
    ) -> Option<u8> {
        if !matches!(pixel_size, 2 | 4 | 8) {
            return None;
        }

        let bytes = Self::live_menu_color_table_bytes(bus);
        Self::menu_color_rgb_pixel_index(bus, MenuColorTable::new(&bytes).title_background(menu_id))
    }

    fn menu_dropdown_background_pixel_index(
        bus: &MacMemoryBus,
        menu_id: i16,
        pixel_size: u16,
    ) -> Option<u8> {
        if !matches!(pixel_size, 2 | 4 | 8) {
            return None;
        }

        let bytes = Self::live_menu_color_table_bytes(bus);
        Self::menu_color_rgb_pixel_index(
            bus,
            MenuColorTable::new(&bytes).dropdown_background(menu_id),
        )
    }

    /// Pixel value the standard definition procedures use to dim
    /// unavailable menu content on a colour screen.
    ///
    /// MTE 1992 p. 3-131 and HIG 1992 p. 54 say unavailable titles and
    /// items stay visible but dimmed. The System 7 definition procedures
    /// do that with `GetGray` (IM:V 1986 p. V-142), which resolves the
    /// shade halfway between the content colour and the menu background
    /// against the device colour table — so a colour screen shows solid
    /// grey glyphs rather than stippled black ones. `None` means the
    /// device cannot express that shade (notably 1-bit screens), where
    /// the definition procedures apply the 50% grey pattern instead.
    pub(super) fn menu_dim_pixel_index(
        bus: &MacMemoryBus,
        pixel_size: u16,
        content_index: Option<u8>,
        background_index: Option<u8>,
    ) -> Option<u8> {
        if pixel_size == 1 {
            return None;
        }
        let resolve = |index: Option<u8>, default: [u16; 3]| match index {
            Some(index) => Self::fb_main_screen_rgb_for_pixel_index(bus, index).unwrap_or(default),
            None => default,
        };
        let background = resolve(background_index, [0xFFFF; 3]);
        let content = resolve(content_index, [0; 3]);
        let midpoint = MenuColorTable::dimmed(content, background);
        let gray = Self::fb_main_screen_pixel_index_for_rgb(bus, midpoint)?;
        let content_pixel = Self::fb_main_screen_pixel_index_for_rgb(bus, content);
        let background_pixel = Self::fb_main_screen_pixel_index_for_rgb(bus, background);
        if Some(gray) == content_pixel || Some(gray) == background_pixel {
            return None;
        }
        Some(gray)
    }

    fn menu_hilite_pixel_indexes(
        bus: &MacMemoryBus,
        background_index: Option<u8>,
        foreground_index: Option<u8>,
        pixel_size: u16,
    ) -> Option<(u8, u8)> {
        if !matches!(pixel_size, 2 | 4 | 8) {
            return None;
        }

        // IM:V 1986 pp. V-249 and V-252 to V-253: the standard MDEF and
        // MBDF highlight color menus by reversing their element foreground
        // and background colors, not by complementing indices or applying
        // the general QuickDraw HiliteRGB transfer mode.
        let background =
            background_index.or_else(|| Self::menu_color_rgb_pixel_index(bus, [0xFFFF; 3]))?;
        let foreground =
            foreground_index.or_else(|| Self::menu_color_rgb_pixel_index(bus, [0; 3]))?;
        Some((background, foreground))
    }

    fn menu_hilited_pixel_index(pixel: u8, background: u8, foreground: u8) -> u8 {
        standard_menu_highlighted_value(pixel, background, foreground)
    }

    fn hilite_packed_menu_pixel(
        bus: &mut MacMemoryBus,
        screen: (u32, u32, i16, i16),
        pixel_size: u16,
        x: i16,
        y: i16,
        hilite_indexes: (u8, u8),
    ) {
        let (screen_base, row_bytes, screen_width, screen_height) = screen;
        if x < 0 || x >= screen_width || y < 0 || y >= screen_height {
            return;
        }

        let Some(pixel) = Self::fb_get_pixel_index(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            x,
            y,
        ) else {
            return;
        };
        let highlighted = Self::menu_hilited_pixel_index(pixel, hilite_indexes.0, hilite_indexes.1);
        Self::fb_set_pixel_index(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            x,
            y,
            highlighted,
        );
    }

    fn alloc_handle_with_bytes(&mut self, bus: &mut MacMemoryBus, bytes: &[u8]) -> u32 {
        let handle = bus.alloc(4);
        if handle == 0 {
            return 0;
        }

        if bytes.is_empty() {
            bus.write_long(handle, 0);
            return handle;
        }

        let data_ptr = bus.alloc(bytes.len() as u32);
        if data_ptr == 0 {
            bus.free(handle);
            return 0;
        }

        bus.write_bytes(data_ptr, bytes);
        bus.write_long(handle, data_ptr);
        self.track_handle_ptr(data_ptr, handle);
        handle
    }

    pub(crate) fn create_popup_menu_handle(&mut self, bus: &mut MacMemoryBus, menu_id: i16) -> u32 {
        if menu_id <= 0 {
            return 0;
        }

        if let Some(menu) = self
            .menus
            .iter()
            .rev()
            .find(|menu| menu.id == menu_id && menu.handle != 0)
        {
            return menu.handle;
        }

        let menu_ptr = bus.alloc(256);
        let handle = bus.alloc(4);
        if menu_ptr == 0 || handle == 0 {
            return 0;
        }
        bus.write_long(handle, menu_ptr);
        self.track_handle_ptr(menu_ptr, handle);

        let menu =
            if let Some((_, res_ptr)) = self.find_or_load_resource_any(bus, *b"MENU", menu_id) {
                let res_size = menu_resource_size(bus, res_ptr);
                for i in 0..res_size.min(256) {
                    bus.write_byte(menu_ptr + i as u32, bus.read_byte(res_ptr + i as u32));
                }
                parse_menu_resource(bus, menu_ptr, handle)
            } else {
                bus.write_word(menu_ptr, menu_id as u16);
                bus.write_word(menu_ptr + 2, 0);
                bus.write_word(menu_ptr + 4, 0);
                bus.write_long(menu_ptr + 6, 0);
                bus.write_long(menu_ptr + 10, 0xFFFF_FFFF);
                bus.write_byte(menu_ptr + 14, 0);
                bus.write_byte(menu_ptr + 15, 0);
                Menu {
                    id: menu_id,
                    title: String::new(),
                    items: Vec::new(),
                    enabled: true,
                    handle,
                    in_menu_bar: false,
                    hierarchical: false,
                    visible_in_menu_bar: false,
                }
            };

        self.menus.push(menu);
        self.load_menu_color_resource(bus, menu_id);
        handle
    }

    fn clone_menu_color_handle(&mut self, bus: &mut MacMemoryBus, handle: u32) -> u32 {
        if handle == 0 {
            return 0;
        }

        let bytes = Self::menu_color_table_bytes(bus, handle);
        self.alloc_handle_with_bytes(bus, &bytes)
    }

    fn merge_menu_color_entries(&mut self, bus: &mut MacMemoryBus, entries: &[u8]) {
        if entries.is_empty() {
            return;
        }

        let current_handle = self.ensure_menu_color_table_handle(bus);
        if current_handle == 0 {
            return;
        }

        let current_bytes = Self::menu_color_table_bytes(bus, current_handle);
        let new_bytes = shared_merge_menu_color_entries(&current_bytes, entries);
        let _ = self.replace_handle_bytes(bus, current_handle, &new_bytes);
    }

    fn load_menu_color_resource(&mut self, bus: &mut MacMemoryBus, resource_id: i16) {
        let Some((_, resource_ptr)) = self.find_or_load_resource_any(bus, *b"mctb", resource_id)
        else {
            return;
        };
        let resource_size = bus.get_alloc_size(resource_ptr).unwrap_or(0) as usize;
        let entries = compiled_menu_color_entries(&bus.read_bytes(resource_ptr, resource_size));
        self.merge_menu_color_entries(bus, &entries);
    }

    fn filter_menu_color_table_entries<F>(&mut self, bus: &mut MacMemoryBus, mut keep: F)
    where
        F: FnMut(i16, i16) -> bool,
    {
        let current_handle = bus.read_long(addr::MENU_C_INFO);
        if current_handle == 0 {
            return;
        }

        let current_bytes = Self::menu_color_table_bytes(bus, current_handle);
        if current_bytes.is_empty() {
            return;
        }

        let filtered = filter_menu_color_entries(&current_bytes, |menu_id, menu_item| {
            keep(menu_id, menu_item)
        });
        let _ = self.replace_handle_bytes(bus, current_handle, &filtered);
    }

    fn clear_menu_color_table_entries(&mut self, bus: &mut MacMemoryBus) {
        let current_handle = bus.read_long(addr::MENU_C_INFO);
        if current_handle != 0 {
            let _ = self.replace_handle_bytes(bus, current_handle, &[]);
        }
    }

    fn replace_handle_bytes(&mut self, bus: &mut MacMemoryBus, handle: u32, bytes: &[u8]) -> bool {
        if handle == 0 {
            return false;
        }

        let old_ptr = bus.read_long(handle);
        let new_ptr = if bytes.is_empty() {
            0
        } else {
            let new_ptr = bus.alloc(bytes.len() as u32);
            if new_ptr == 0 {
                return false;
            }
            bus.write_bytes(new_ptr, bytes);
            new_ptr
        };

        if old_ptr != 0 && old_ptr != new_ptr {
            self.untrack_handle_ptr(old_ptr);
            bus.free(old_ptr);
        }
        bus.write_long(handle, new_ptr);
        if new_ptr != 0 {
            self.track_handle_ptr(new_ptr, handle);
        }
        true
    }

    fn current_menu_list(&self, bus: &MacMemoryBus) -> Option<SharedMenuList> {
        menu_list_from_memory(bus, bus.read_long(addr::MENU_LIST))
    }

    /// Resolve a canonical guest MenuHandle into this gateway's derived
    /// presentation cache. Retained Menu Manager state uses handles, as the
    /// current menu list does; cache positions are never stable menu
    /// identities. Macintosh Toolbox Essentials (1992), pp. 3-95--3-97.
    pub(super) fn menu_index_for_handle(&self, menu_handle: u32) -> Option<usize> {
        self.menus
            .iter()
            .position(|menu| menu.handle == menu_handle)
    }

    fn menu_handle_for_index(&self, menu_index: usize) -> Option<u32> {
        self.menus.get(menu_index).map(|menu| menu.handle)
    }

    pub(super) fn current_menu_bar_highlight_index(&self, bus: &MacMemoryBus) -> Option<usize> {
        let selected_menu_id = bus.read_word(addr::THE_MENU) as i16;
        if selected_menu_id == 0 {
            return None;
        }
        let (owner_handle, _owner_id) = self
            .current_menu_list(bus)?
            .owning_regular_menu(selected_menu_id, |handle| {
                menu_key_menu_from_memory(bus, handle)
            })?;
        self.menus
            .iter()
            .position(|menu| menu.handle == owner_handle)
    }

    fn replace_current_menu_list(
        &mut self,
        bus: &mut MacMemoryBus,
        menu_list: &SharedMenuList,
    ) -> bool {
        let current_handle = bus.read_long(addr::MENU_LIST);
        let installation =
            install_menu_list_copy(current_handle, menu_list, |request| match request {
                MenuListInstallRequest::Allocate { bytes } => {
                    let handle = self.alloc_handle_with_bytes(bus, bytes);
                    (handle != 0).then_some(handle).ok_or(())
                }
                MenuListInstallRequest::Replace { handle, bytes } => self
                    .replace_handle_bytes(bus, handle, bytes)
                    .then_some(handle)
                    .ok_or(()),
            });
        let Ok(installation) = installation else {
            return false;
        };
        if installation.allocated {
            bus.write_long(addr::MENU_LIST, installation.handle);
        }
        true
    }

    fn relayout_menu_list(&self, bus: &MacMemoryBus, menu_list: &mut SharedMenuList) {
        menu_list.relayout_regular_titles(
            STANDARD_MENU_BAR_FIRST_TITLE_LEFT,
            STANDARD_MENU_BAR_TITLE_SPACING,
            |handle| {
                let menu_ptr = bus.read_long(handle);
                (menu_ptr != 0)
                    .then(|| {
                        let title_len = bus.read_byte(menu_ptr + 14) as usize;
                        macroman_to_string(&bus.read_bytes(menu_ptr + 15, title_len))
                    })
                    .map(|title| Self::menu_title_advance(&title))
                    .unwrap_or(0)
            },
        );
    }

    fn mutate_menu_items(
        &mut self,
        bus: &mut MacMemoryBus,
        menu_handle: u32,
        mutation: impl FnOnce(&mut crate::menu_manager::MenuItems) -> bool,
    ) -> bool {
        let menu_ptr = bus.read_long(menu_handle);
        if menu_ptr == 0 {
            return false;
        }
        let record_size = bus
            .get_alloc_size(menu_ptr)
            .unwrap_or_else(|| menu_resource_size(bus, menu_ptr) as u32);
        let original = bus.read_bytes(menu_ptr, record_size as usize);
        let Some(mut items) = crate::menu_manager::MenuItems::decode(&original) else {
            return false;
        };
        if !mutation(&mut items) {
            return false;
        }
        let Some(bytes) = items.rebuild(&original) else {
            return false;
        };
        // A serialized 68k callback can be attached to a native process whose
        // allocator capacity is not recorded by MacMemoryBus. Non-growing
        // rebuilds still fit the decoded record and must preserve its handle.
        if record_size as usize >= bytes.len() {
            bus.write_bytes(menu_ptr, &bytes);
        } else if bus.is_foreign_ordinary_sparse_address(menu_handle)
            && bus.is_foreign_ordinary_sparse_address(menu_ptr)
        {
            // Inside Macintosh: Memory (1992), pp. 2-40--2-41.
            // A native relocatable block must remain in its process heap. The
            // attached process Memory Manager updates its master pointer and
            // allocation record before this 68K trap returns.
            if !self.replace_process_native_handle_bytes(
                bus,
                menu_handle,
                menu_ptr,
                &bytes,
            ) {
                return false;
            }
        } else {
            let mut allocation = bytes;
            allocation.resize(allocation.len().max(256), 0);
            if !self.replace_handle_bytes(bus, menu_handle, &allocation) {
                return false;
            }
        }
        if let Some(menu) = self
            .menus
            .iter_mut()
            .find(|menu| menu.handle == menu_handle)
        {
            refresh_menu_from_memory(bus, menu);
        } else {
            let menu_ptr = bus.read_long(menu_handle);
            if menu_ptr != 0 {
                self.menus
                    .push(parse_menu_resource(bus, menu_ptr, menu_handle));
            }
        }
        true
    }

    /// Collect and materialize the resource names consumed by AppendResMenu
    /// and InsertResMenu.
    ///
    /// A `FONT` request gives `FOND` names precedence before legacy `FONT`
    /// names. Both routines restore automatic resource loading before they
    /// return; filtering, sorting, duplicate suppression, insertion, and item
    /// defaults remain shared `MenuItems` policy. Macintosh Toolbox
    /// Essentials (1992), pp. 3-101--3-104.
    fn resource_menu_names(
        &mut self,
        bus: &mut MacMemoryBus,
        requested_type: [u8; 4],
    ) -> Vec<Vec<u8>> {
        self.policy.set_res_load(true);
        bus.write_word(crate::memory::globals::addr::RES_LOAD, 0x0100);
        let resource_types = if requested_type == *b"FONT" {
            [Some(*b"FOND"), Some(*b"FONT")]
        } else {
            [Some(requested_type), None]
        };
        let mut names = Vec::new();
        for resource_type in resource_types.into_iter().flatten() {
            for (refnum, id, name, ptr) in self.named_resource_records_of_type(resource_type) {
                let _ =
                    self.get_or_create_resource_handle_in_file(bus, resource_type, id, ptr, refnum);
                names.push(encode_mac_roman_lossy(&name));
            }
        }
        if requested_type == *b"FONT" {
            for &(id, name) in crate::quickdraw::fonts::FONT_NAMES {
                if id != crate::quickdraw::fonts::FONT_APPLICATION {
                    names.push(encode_mac_roman_lossy(name));
                }
            }
        }
        names
    }

    pub(crate) fn refresh_menus_from_memory(&mut self, bus: &MacMemoryBus) {
        let Some(menu_list) = self.current_menu_list(bus) else {
            return;
        };
        self.apply_menu_list_membership(bus, &menu_list);
        for menu in &mut self.menus {
            refresh_menu_from_memory(bus, menu);
        }
    }

    /// Perform the one deferred DrawMenuBar requested through InvalMenuBar.
    /// The Toolbox Event Manager calls this while scanning for update work;
    /// repeated invalidations coalesce in the shared event state. Macintosh
    /// Toolbox Essentials (1992), pp. 3-93 and 3-114.
    pub(crate) fn service_invalid_menu_bar(&mut self, bus: &mut MacMemoryBus) -> bool {
        if !self.event_queue.take_menu_bar_invalidation() {
            return false;
        }
        self.release_initial_menu_bar_kiosk();
        self.refresh_menus_from_memory(bus);
        self.draw_menu_bar_to_fb(bus);
        true
    }

    /// Reconcile the 68k presentation cache with the authoritative guest
    /// MenuList without replacing cached item presentation state for handles
    /// already known to the adapter.
    fn apply_menu_list_membership(&mut self, bus: &MacMemoryBus, menu_list: &SharedMenuList) {
        let mut previous = std::mem::take(&mut self.menus);
        let mut current = Vec::with_capacity(previous.len());
        for (handle, hierarchical) in menu_list
            .regular_handles()
            .map(|handle| (handle, false))
            .chain(
                menu_list
                    .hierarchical_handles()
                    .map(|handle| (handle, true)),
            )
        {
            let mut menu =
                if let Some(index) = previous.iter().position(|menu| menu.handle == handle) {
                    previous.remove(index)
                } else {
                    let menu_ptr = bus.read_long(handle);
                    if menu_ptr == 0 {
                        continue;
                    }
                    parse_menu_resource(bus, menu_ptr, handle)
                };
            menu.in_menu_bar = true;
            menu.hierarchical = hierarchical;
            menu.visible_in_menu_bar = !hierarchical;
            current.push(menu);
        }
        for mut menu in previous {
            menu.in_menu_bar = false;
            menu.hierarchical = false;
            menu.visible_in_menu_bar = false;
            current.push(menu);
        }
        self.menus = current;
    }

    fn refresh_menu_cache_from_list(&mut self, bus: &MacMemoryBus, menu_list: &SharedMenuList) {
        self.apply_menu_list_membership(bus, menu_list);
        for menu in &mut self.menus {
            refresh_menu_from_memory(bus, menu);
        }
    }

    pub(crate) fn dispatch_menu<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        self.retire_menu_definition(cpu, bus);
        let _menu_root = (is_tool && matches!(trap_num, 0x13d | 0x00b)).then(|| {
            let sp = cpu.read_reg(Register::A7);
            let request = if trap_num == 0x00b {
                MenuTrackingRequest::PopUp(PopupMenuRequest {
                    menu_handle: bus.read_long(sp + 6),
                    anchor: (bus.read_word(sp + 4) as i16, bus.read_word(sp + 2) as i16),
                    requested_item: bus.read_word(sp) as i16,
                })
            } else {
                MenuTrackingRequest::MenuSelect {
                    initial_point: bus.read_long(sp),
                }
            };
            self.menu_tracking.enter_new_call(MenuTrackingCall {
                request,
                origin: MenuTrackingOrigin::M68k {
                    stack_pointer: sp,
                    return_address: self
                        .current_trap_caller
                        .unwrap_or_else(|| cpu.read_reg(Register::PC)),
                },
            })
        });
        let result = self.dispatch_menu_body(is_tool, trap_num, cpu, bus);
        if _menu_root.is_some()
            && self.current_trap_caller.is_some()
            && self.menu_tracking.context().call.is_some()
            && (self.menu_tracking.is_some() || self.active_menu_definition().is_some())
        {
            self.preserve_auto_pop_pc_once = true;
        }
        result
    }

    #[cfg(test)]
    fn step_menu_fixture<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        // One fixture step models a presentation tick, independently of
        // the amount of guest code exercised by the ABI call below.
        bus.write_long(
            crate::memory::globals::addr::TICKS,
            bus.read_long(crate::memory::globals::addr::TICKS)
                .wrapping_add(1),
        );
        let tracking_call = self.menu_tracking.context().call.filter(|call| {
            call.origin.isa() == GuestIsa::M68k
                && is_tool
                && match call.request {
                    MenuTrackingRequest::MenuSelect { .. } => trap_num == 0x13d,
                    MenuTrackingRequest::PopUp(_) => trap_num == 0x00b,
                }
        });
        if let Some(call) = tracking_call {
            // These adapter fixtures write the MDEF result directly instead
            // of executing its instruction stream. Retire that exact installed
            // frame before stepping the retained operation.
            let MenuTrackingOrigin::M68k { stack_pointer, .. } = call.origin else {
                unreachable!()
            };
            let frame_sp =
                stack_pointer - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION;
            if self.guest_calls.depth() > 0 && cpu.read_reg(Register::A7) == frame_sp {
                cpu.write_reg(Register::PC, frame_sp + 54);
                self.retire_menu_definition(cpu, bus);
            }
            if self.resume_menu_tracking(cpu, bus).is_some() {
                return Some(Ok(()));
            }
        }
        self.dispatch_menu(is_tool, trap_num, cpu, bus)
    }

    pub(crate) fn has_ready_menu_tracking(&self) -> bool {
        self.menu_tracking.ready_call(GuestIsa::M68k).is_some()
    }

    pub(crate) fn resume_menu_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<u16> {
        let (call, _scope) = self.menu_tracking.resume_call(GuestIsa::M68k)?;
        let MenuTrackingOrigin::M68k {
            stack_pointer,
            return_address,
        } = call.origin
        else {
            unreachable!()
        };
        let trap_num = match call.request {
            MenuTrackingRequest::MenuSelect { .. } => 0x13d,
            MenuTrackingRequest::PopUp(_) => 0x00b,
        };
        cpu.write_reg(Register::A7, stack_pointer);
        cpu.write_reg(Register::PC, return_address);
        self.dispatch_menu_body(true, trap_num, cpu, bus)?.ok()?;
        if self.menu_tracking.is_none() && self.active_menu_definition().is_none() {
            self.restore_menu_definition_port(cpu, bus);
        }
        Some(0xa800 | trap_num)
    }

    fn dispatch_menu_body<C: CpuOps>(
        &mut self, is_tool: bool, trap_num: u16, cpu: &mut C, bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        self.read_tick_count(bus);
        Some(match (is_tool, trap_num) {
            // InitMenus ($A930)
            // Initializes an empty current menu list and menu color table.
            // PROCEDURE InitMenus;
            // Macintosh Toolbox Essentials (1992), pp. 3-103--3-104
            (true, 0x130) => {
                let _ = self.ensure_menu_color_table_handle(bus);
                self.load_menu_color_resource(bus, 0);
                let menu_list = SharedMenuList::default();
                if self.replace_current_menu_list(bus, &menu_list) {
                    self.apply_menu_list_membership(bus, &menu_list);
                }
                Ok(())
            }

            // NewMenu ($A931)
            // Creates a new empty menu.
            // FUNCTION NewMenu(menuID: INTEGER; menuTitle: Str255): MenuHandle;
            // Inside Macintosh Volume I, I-352
            // Stack: SP+0 titlePtr (4), SP+4 menuID (2), SP+6 result (4). Pop 6.
            // NewMenu ($A931): Allocates menu handle
            (true, 0x131) => {
                let sp = cpu.read_reg(Register::A7);
                let title_ptr = bus.read_long(sp);
                let menu_id = bus.read_word(sp + 4) as i16;
                let menu_proc = self.menu_def_proc_handle(bus, 0);
                let title_bytes = if title_ptr == 0 {
                    Vec::new()
                } else {
                    let title_len = usize::from(bus.read_byte(title_ptr));
                    bus.read_bytes(title_ptr + 1, title_len)
                };
                let menu_record = new_standard_menu_record(menu_id, menu_proc, &title_bytes);
                let menu_ptr = bus.alloc(menu_record.len().max(256) as u32);
                let handle = bus.alloc(4);
                bus.write_long(handle, menu_ptr);
                self.track_handle_ptr(menu_ptr, handle);
                bus.write_bytes(menu_ptr, &menu_record);
                let title = macroman_to_string(&title_bytes);
                // Track the menu in self.menus immediately so AppendMenu
                // (which often runs BEFORE InsertMenu in typical Mac app
                // boot code) can find the menu by handle.
                if !self.menus.iter().any(|m| m.handle == handle) {
                    self.menus.push(Menu {
                        id: menu_id,
                        title,
                        items: Vec::new(),
                        enabled: true,
                        handle,
                        in_menu_bar: false,
                        hierarchical: false,
                        visible_in_menu_bar: false,
                    });
                }
                bus.write_long(sp + 6, handle);
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // GetMenu ($A9BF)
            // Reads a menu from a MENU resource and returns a MenuHandle.
            // FUNCTION GetMenu(resourceID: INTEGER): MenuHandle;
            // Inside Macintosh Volume I, I-352
            // GetMenu ($A9BF): Reads MENU resource via the Resource Manager;
            // returns NIL when the MENU resource cannot be read per IM:I I-352.
            // IM:V 1986 p. V-234: after loading a MENU resource, GetMenu
            // also attempts to load an 'mctb' resource with the same ID.
            (true, 0x1BF) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_id = bus.read_word(sp) as i16;
                let handle = self.load_menu_resource(bus, menu_id);
                cpu.write_reg(Register::D0, bus.read_word(0x0A60) as i16 as i32 as u32);
                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                if handle != 0 {
                    // A custom definition owns the initial dimensions of a
                    // newly created MenuRecord. Macintosh Toolbox Essentials
                    // (1992), pp. 3-148--3-151.
                    if self.arm_menu_definition(
                        cpu,
                        bus,
                        SharedMenuDefinitionInvocation::size(handle),
                    ) {
                        return Some(Ok(()));
                    }
                    self.calculate_standard_menu_size(bus, handle);
                }
                Ok(())
            }

            // AppendMenu ($A933)
            // Adds one or more menu items to the end of a menu.
            // PROCEDURE AppendMenu(theMenu: MenuHandle; data: Str255);
            // Inside Macintosh Volume I, I-358
            (true, 0x133) => {
                let sp = cpu.read_reg(Register::A7);
                let text_ptr = bus.read_long(sp);
                let mut menu_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);

                if menu_handle == 0 {
                    let a0_handle = cpu.read_reg(Register::A0);
                    if let Some(resolved) = self.resolve_menu_handle_candidate(bus, a0_handle) {
                        menu_handle = resolved;
                    }
                }
                if menu_handle == 0 || text_ptr == 0 {
                    return Some(Ok(()));
                }
                let len = bus.read_byte(text_ptr) as usize;
                let mut bytes = Vec::with_capacity(len);
                for i in 0..len {
                    bytes.push(bus.read_byte(text_ptr + 1 + i as u32));
                }
                self.mutate_menu_items(bus, menu_handle, |items| items.append_specs(&bytes));
                Ok(())
            }

            // InsertMenu ($A935)
            // Inserts an existing menu into the current menu list.
            // PROCEDURE InsertMenu(theMenu: MenuHandle; beforeID: INTEGER);
            // Macintosh Toolbox Essentials (1992), p. 3-108
            (true, 0x135) => {
                let sp = cpu.read_reg(Register::A7);
                let before_id = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);

                if menu_handle != 0 {
                    let menu_ptr = bus.read_long(menu_handle);
                    if menu_ptr != 0 {
                        // Per IM:I I-352, InsertMenu inserts a menu into the
                        // current menu list; it does not create/duplicate one.
                        if let Some(idx) = self.menus.iter().position(|m| m.handle == menu_handle) {
                            self.last_inserted_menu_id = Some(self.menus[idx].id);
                        } else {
                            // Handle wasn't previously tracked (for example a
                            // raw guest MENU handle). Parse any MENU resource
                            // by menu ID, else fall back to title-only memory.
                            let menu_id = bus.read_word(menu_ptr) as i16;
                            if let Some((_, res_ptr)) =
                                self.find_or_load_resource_any(bus, *b"MENU", menu_id)
                            {
                                let mut menu = parse_menu_resource(bus, res_ptr, menu_handle);
                                eprintln!(
                                    "[MENU] InsertMenu: ID={} title=\"{}\" items={}",
                                    menu.id,
                                    menu.title,
                                    menu.items.len()
                                );
                                menu.in_menu_bar = false;
                                menu.hierarchical = false;
                                menu.visible_in_menu_bar = false;
                                self.last_inserted_menu_id = Some(menu.id);
                                self.menus.push(menu);
                            } else {
                                // Fallback: read title from the menu record in memory
                                let title_len = bus.read_byte(menu_ptr + 14) as usize;
                                if title_len > 0 && title_len < 64 {
                                    let mut title_bytes = Vec::with_capacity(title_len);
                                    for i in 0..title_len {
                                        title_bytes.push(bus.read_byte(menu_ptr + 15 + i as u32));
                                    }
                                    let title = macroman_to_string(&title_bytes);
                                    if !title.is_empty() {
                                        eprintln!(
                                            "[MENU] InsertMenu: title=\"{}\" (no resource)",
                                            title
                                        );
                                        self.last_inserted_menu_id = Some(menu_id);
                                        self.menus.push(Menu {
                                            id: menu_id,
                                            title,
                                            items: Vec::new(),
                                            enabled: true,
                                            handle: menu_handle,
                                            in_menu_bar: false,
                                            hierarchical: false,
                                            visible_in_menu_bar: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                let mut menu_list = self.current_menu_list(bus).unwrap_or_default();
                if self.menus.iter().any(|menu| menu.handle == menu_handle)
                    && menu_list.insert(menu_handle, before_id, |candidate| {
                        let menu_ptr = bus.read_long(candidate);
                        (menu_ptr != 0).then(|| bus.read_word(menu_ptr) as i16)
                    })
                {
                    self.relayout_menu_list(bus, &mut menu_list);
                    if self.replace_current_menu_list(bus, &menu_list) {
                        self.apply_menu_list_membership(bus, &menu_list);
                    }
                }

                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // DrawMenuBar ($A937)
            // Draws the menu bar.
            // PROCEDURE DrawMenuBar;
            // Inside Macintosh Volume I, I-354
            // DrawMenuBar ($A937): Renders menu-bar titles for menus currently
            // in the menu list (InsertMenu-installed) per IM:I I-352/I-354.
            (true, 0x137) => {
                // An explicit draw satisfies any earlier deferred request.
                self.event_queue.take_menu_bar_invalidation();
                // Initial kiosk mode is only a frontend launch policy. A
                // guest DrawMenuBar call is an explicit request to present its
                // menus, so ownership returns to the guest before rendering.
                self.release_initial_menu_bar_kiosk();
                self.refresh_menus_from_memory(bus);
                self.draw_menu_bar_to_fb(bus);
                Ok(())
            }

            // ClearMenuBar ($A934)
            // Removes every menu from the current menu list.
            // PROCEDURE ClearMenuBar;
            // Macintosh Toolbox Essentials (1992), p. 3-110
            (true, 0x134) => {
                // IM:V 1986 p. V-244: ClearMenuBar clears both the
                // current menu list and the application's menu color
                // information table.
                let mut menu_list = self.current_menu_list(bus).unwrap_or_default();
                menu_list.clear_entries();
                if self.replace_current_menu_list(bus, &menu_list) {
                    self.apply_menu_list_membership(bus, &menu_list);
                }
                self.clear_menu_color_table_entries(bus);
                Ok(())
            }

            // GetMHandle ($A949)
            // Returns a current-list menu handle, checking submenus first.
            // FUNCTION GetMHandle(menuID: INTEGER): MenuHandle;
            // Inside Macintosh Volume V (1986), p. V-246
            (true, 0x149) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_id = bus.read_word(sp) as i16;
                let handle = menu_list_from_memory(bus, bus.read_long(addr::MENU_LIST))
                    .and_then(|menu_list| {
                        menu_list.find_handle_by_id(menu_id, |handle| {
                            let menu_ptr = bus.read_long(handle);
                            (menu_ptr != 0).then(|| bus.read_word(menu_ptr) as i16)
                        })
                    })
                    .unwrap_or(0);
                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // SetMenuBar ($A93C)
            // Replaces the current menu list with a copy of the supplied list.
            // PROCEDURE SetMenuBar(menuList: Handle);
            // Inside Macintosh Volume I, I-354
            (true, 0x13C) => {
                let sp = cpu.read_reg(Register::A7);
                let source_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                let Some(menu_list) = menu_list_from_memory(bus, source_handle) else {
                    return Some(Ok(()));
                };
                if self.replace_current_menu_list(bus, &menu_list) {
                    self.refresh_menu_cache_from_list(bus, &menu_list);
                }
                Ok(())
            }

            // GetNewMBar ($A9C0)
            // Reads an MBAR resource and builds a menu bar from it.
            // FUNCTION GetNewMBar(menuBarID: INTEGER): Handle;
            // Inside Macintosh Volume I, I-354
            (true, 0x1C0) => {
                let sp = cpu.read_reg(Register::A7);
                let mbar_id = bus.read_word(sp) as i16;
                let handle = if let Some((_, mbar_ptr)) =
                    self.find_or_load_resource_any(bus, *b"MBAR", mbar_id)
                {
                    let mbar_size = bus.get_alloc_size(mbar_ptr).unwrap_or(0) as usize;
                    let Some(mbar) =
                        SharedMenuBarResource::decode(&bus.read_bytes(mbar_ptr, mbar_size))
                    else {
                        bus.write_long(sp + 2, 0);
                        cpu.write_reg(Register::A7, sp + 2);
                        return Some(Ok(()));
                    };
                    self.clear_menu_color_table_entries(bus);
                    let menu_handles = mbar.load_regular_handles(|menu_id| {
                        let handle = self.load_menu_resource(bus, menu_id);
                        (handle != 0).then_some(handle)
                    });
                    let mut menu_list =
                        SharedMenuList::from_regular_handles(mbar_id, menu_handles.iter().copied());
                    self.relayout_menu_list(bus, &mut menu_list);
                    let list_bytes = menu_list.encode();
                    let list_block = bus.alloc(list_bytes.len() as u32);
                    bus.write_bytes(list_block, &list_bytes);
                    let list_handle = bus.alloc(4);
                    bus.write_long(list_handle, list_block);
                    if self.guest_calls.begin_menu_bar_build(
                        SharedMenuBarBuild::new(list_handle, menu_handles),
                        MenuBarCallOrigin::M68k { stack_pointer: sp, return_address: self.current_trap_caller.unwrap_or_else(|| cpu.read_reg(Register::PC)) },
                    ).is_none() {
                        bus.write_long(sp + 2, 0);
                        cpu.write_reg(Register::A7, sp + 2);
                        return Some(Ok(()));
                    }
                    list_handle
                } else {
                    0
                };

                if handle != 0 {
                    self.continue_menu_bar_build(cpu, bus, self.current_trap_caller.is_some());
                } else {
                    bus.write_long(sp + 2, handle);
                    cpu.write_reg(Register::A7, sp + 2);
                }
                Ok(())
            }

            // AddResMenu ($A94D)
            // Appends items to a menu from resources of a given type.
            // PROCEDURE AddResMenu(theMenu: MenuHandle; theType: ResType);
            // Inside Macintosh Volume I, I-353; Macintosh Toolbox
            // Essentials 1992, 3-101..3-102 (AppendResMenu — System 7
            // alias for AddResMenu).
            //
            // Stack: SP+0 theType (4), SP+4 theMenu handle (4). Pop 8.
            (true, 0x14D) => {
                let sp = cpu.read_reg(Register::A7);
                let res_type_word = bus.read_long(sp);
                let menu_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);

                let res_type = res_type_word.to_be_bytes();
                let names = self.resource_menu_names(bus, res_type);
                self.mutate_menu_items(bus, menu_handle, |items| {
                    items.insert_resource_names(names, i16::MAX)
                });
                Ok(())
            }

            // DisableItem ($A93A)
            // Disables a menu item so it cannot be chosen.
            // PROCEDURE DisableItem(theMenu: MenuHandle; item: INTEGER);
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-131
            (true, 0x13A) => {
                let sp = cpu.read_reg(Register::A7);
                let item = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);

                self.mutate_menu_items(bus, menu_handle, |items| items.set_enabled(item, false));
                if let Some(menu) = self.menus.iter().find(|m| m.handle == menu_handle) {
                    if std::env::var_os("SYSTEMLESS_TRACE_MENUKEY").is_some() {
                        eprintln!(
                            "[MENUKEY] DisableItem menu={} title=\"{}\" item={} enabled={}",
                            menu.id, menu.title, item, menu.enabled
                        );
                    }
                }
                Ok(())
            }

            // EnableItem ($A939)
            // Enables a menu item so it can be chosen.
            // PROCEDURE EnableItem(theMenu: MenuHandle; item: INTEGER);
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-131
            (true, 0x139) => {
                let sp = cpu.read_reg(Register::A7);
                let item = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);

                self.mutate_menu_items(bus, menu_handle, |items| items.set_enabled(item, true));
                if let Some(menu) = self.menus.iter().find(|m| m.handle == menu_handle) {
                    if std::env::var_os("SYSTEMLESS_TRACE_MENUKEY").is_some() {
                        eprintln!(
                            "[MENUKEY] EnableItem menu={} title=\"{}\" item={} enabled={}",
                            menu.id, menu.title, item, menu.enabled
                        );
                    }
                }
                Ok(())
            }

            // MenuSelect ($A93D)
            // Tracks the mouse in the menu bar and returns the selected item.
            // FUNCTION MenuSelect(startPt: Point): LONGINT;
            // Inside Macintosh Volume I, I-355
            // MenuSelect ($A93D): Full mouse tracking with dropdown, highlighting, flashing
            (true, 0x13D) => {
                if self.menu_tracking.context().caller_isa() == Some(GuestIsa::PowerPc)
                {
                    // A native MenuSelect owns this process continuation. A
                    // nested 68k call must not consume its presentation state
                    // or origin ABI frame.
                    let sp = cpu.read_reg(Register::A7);
                    self.finish_menu_no_hit(bus, cpu, sp, 4);
                    return Some(Ok(()));
                }
                if self.menu_tracking.is_none() && self.pending_native_menu_selection.is_some() {
                    let sp = cpu.read_reg(Register::A7);
                    let result = self.take_native_menu_selection(bus).unwrap_or(0);
                    // The synthetic mouse-up belongs to the native selection;
                    // native AppKit tracking has already completed it.
                    if let Some(index) = self.event_queue.iter().position(|event| event.what == 2) {
                        self.event_queue.remove(index);
                    }
                    bus.write_long(sp + 4, result);
                    cpu.write_reg(Register::A7, sp + 4);
                    self.record_menuselect_input_trace(
                        "native",
                        None,
                        None,
                        Some((result & 0xFFFF) as i16),
                        Some(result),
                        if result == 0 {
                            "native_selection_invalidated"
                        } else {
                            "native_selection"
                        },
                    );
                    return Some(Ok(()));
                }
                if self.menu_tracking.is_some() {
                    if self.active_menu_definition().is_some() {
                        let completed = self.complete_pending_menu_definition(cpu, bus);
                        if completed == Some(crate::menu_manager::MenuDefinitionMessage::Choose) {
                            let active_submenu = self.menu_tracking.as_ref().and_then(|tracking| {
                                match tracking.active_definition_pane() {
                                    Some(SharedMenuDefinitionPane::Submenu(depth)) => tracking
                                        .submenus
                                        .get(depth)
                                        .map(|submenu| submenu.dropdown_rect()),
                                    _ => None,
                                }
                            });
                            if let Some((top, left, bottom, right)) = active_submenu {
                                let (mv, mh) = self.menu_tracking_mouse_pos(bus);
                                if mv < top || mv >= bottom || mh < left || mh >= right {
                                    self.update_menu_tracking_for_point(bus, mh, mv);
                                    if self.active_menu_definition().is_none() {
                                        self.restore_menu_definition_port(cpu, bus);
                                        return Some(Ok(()));
                                    }
                                    if self.active_menu_definition().is_some_and(|definition| {
                                        definition.pending_invocation().is_some()
                                    }) {
                                        if !self.arm_pending_menu_definition(
                                            cpu,
                                            bus,
                                            cpu.read_reg(Register::PC).wrapping_sub(2),
                                        ) {
                                            self.abort_custom_menu_tracking(cpu, bus, 4);
                                        }
                                        return Some(Ok(()));
                                    }
                                }
                            }
                        }
                        if self
                            .menu_tracking
                            .as_ref()
                            .is_some_and(|tracking| tracking.is_flashing())
                        {
                            let tick = bus.read_long(crate::memory::globals::addr::TICKS);
                            match self
                                .menu_tracking
                                .with_tracking_mut(|tracking| tracking.advance_flash_at(tick))
                                .unwrap()
                            {
                                MenuFlashStep::Wait | MenuFlashStep::Inactive => {
                                    return Some(Ok(()))
                                }
                                MenuFlashStep::Complete(result) => {
                                    self.finish_custom_menu_tracking(cpu, bus, 4, result);
                                    return Some(Ok(()));
                                }
                                MenuFlashStep::Highlight(_) => {}
                            }
                            if !self.arm_pending_menu_definition(
                                cpu,
                                bus,
                                cpu.read_reg(Register::PC).wrapping_sub(2),
                            ) {
                                self.abort_custom_menu_tracking(cpu, bus, 4);
                            }
                            return Some(Ok(()));
                        }
                        let (mv, mh) = self.menu_tracking_mouse_pos(bus);
                        if self.menu_tracking_button_down(bus) {
                            let mbar_h = bus.read_word(addr::MBAR_HEIGHT) as i16;
                            if mv < mbar_h {
                                let new_menu = self.current_menu_title_hit_test(bus, mh);
                                let active_menu = self.menu_tracking.as_ref().unwrap().menu_handle;
                                if let Some(new_idx) = new_menu {
                                    if self.menu_handle_for_index(new_idx) != Some(active_menu) {
                                    let old_saved = self.menu_tracking.take().unwrap();
                                    self.clear_active_menu_definition();
                                    self.restore_menu_tracking_pixels(bus, old_saved);
                                    if self.open_menu_dropdown(bus, new_idx) {
                                        self.prepare_menu_definition_port(cpu, bus);
                                        if !self.arm_pending_menu_definition(
                                            cpu,
                                            bus,
                                            cpu.read_reg(Register::PC).wrapping_sub(2),
                                        ) {
                                            self.abort_custom_menu_tracking(cpu, bus, 4);
                                        }
                                    } else {
                                        self.restore_menu_definition_port(cpu, bus);
                                    }
                                        return Some(Ok(()));
                                    }
                                }
                            }
                        }

                        let hit_point = (u32::from(mv as u16) << 16) | u32::from(mh as u16);
                        let choose_armed = self
                            .with_active_menu_definition_mut(|tracking| tracking.choose(hit_point))
                            .flatten()
                            .is_some();
                        if choose_armed {
                            if !self.arm_pending_menu_definition(
                                cpu,
                                bus,
                                cpu.read_reg(Register::PC).wrapping_sub(2),
                            ) {
                                self.abort_custom_menu_tracking(cpu, bus, 4);
                            }
                            return Some(Ok(()));
                        }

                        if !self.menu_tracking_button_down(bus) {
                            let definition = self.active_menu_definition().unwrap().clone();
                            let item = definition.which_item();
                            let menu_handle = definition.menu_handle();
                            let menu_id = bus.read_long(menu_handle);
                            let result = if item > 0 && menu_id != 0 {
                                (u32::from(bus.read_word(menu_id)) << 16) | u32::from(item as u16)
                            } else {
                                0
                            };
                            if result == 0 {
                                self.finish_custom_menu_tracking(cpu, bus, 4, 0);
                            } else {
                                let tick = bus.read_long(crate::memory::globals::addr::TICKS);
                                let flashes =
                                    bus.read_word(crate::memory::globals::addr::MENU_FLASH);
                                let flash_enabled = self
                                    .menu_tracking
                                    .with_tracking_mut(|tracking| {
                                        tracking.set_flash_tick(tick);
                                        tracking.begin_flash(flashes, result)
                                    })
                                    .unwrap();
                                if !flash_enabled {
                                    self.finish_custom_menu_tracking(cpu, bus, 4, result);
                                }
                            }
                        }
                        return Some(Ok(()));
                    }
                    // Re-fire: we're in tracking mode
                    if self.menu_tracking.as_ref().unwrap().is_flashing() {
                        let tick = bus.read_long(crate::memory::globals::addr::TICKS);
                        if let MenuFlashStep::Complete(result) = self
                            .menu_tracking
                            .with_tracking_mut(|tracking| tracking.advance_flash_at(tick))
                            .unwrap()
                        {
                            // Flash complete — finish up
                            let sp = self.menu_tracking.context().classic_stack();
                            let saved = self.menu_tracking.take().unwrap();
                            let active_menu = saved
                                .submenus
                                .last()
                                .map(|submenu| submenu.menu_handle)
                                .unwrap_or(saved.menu_handle);
                            let highlighted_item = saved
                                .submenus
                                .last()
                                .map(|submenu| submenu.highlighted_item)
                                .unwrap_or(saved.highlighted_item);
                            self.restore_menu_tracking_pixels(bus, saved);
                            bus.write_word(addr::THE_MENU, (result >> 16) as u16);
                            self.draw_menu_bar_to_fb(bus);
                            bus.write_long(sp + 4, result);
                            cpu.write_reg(Register::A7, sp + 4);
                            self.record_menuselect_input_trace(
                                "finish",
                                None,
                                self.menu_index_for_handle(active_menu),
                                Some(highlighted_item),
                                Some(result),
                                "enabled_item_selected",
                            );
                        }
                        // else: stay on trap, re-fire next frame
                    } else if !self.menu_tracking_button_down(bus) {
                        // Button released — start flash or complete immediately
                        // Mouse-up may be the first event whose coordinates are
                        // inside a menu item. Update the highlight from that
                        // final point before deriving the selection result.
                        // Inside Macintosh Volume I, I-355: MenuSelect tracks
                        // the pointer until mouse-up and returns that item.
                        let (mv, mh) = self.menu_tracking_mouse_pos(bus);
                        self.update_menu_tracking_for_point(bus, mh, mv);
                        if self
                            .active_menu_definition()
                            .is_some_and(|definition| definition.pending_invocation().is_some())
                        {
                            self.prepare_menu_definition_port(cpu, bus);
                            if !self.arm_pending_menu_definition(
                                cpu,
                                bus,
                                cpu.read_reg(Register::PC).wrapping_sub(2),
                            ) {
                                self.abort_custom_menu_tracking(cpu, bus, 4);
                            }
                            return Some(Ok(()));
                        }
                        let result = self.menu_tracking_selection_result(bus);
                        if result != 0 {
                            let (active_menu, item_idx) = self
                                .menu_tracking
                                .as_ref()
                                .and_then(|tracking| {
                                    tracking
                                        .submenus
                                        .last()
                                        .filter(|submenu| submenu.highlighted_item > 0)
                                        .map(|submenu| {
                                            (submenu.menu_handle, submenu.highlighted_item)
                                        })
                                        .or_else(|| {
                                            (tracking.highlighted_item > 0).then_some((
                                                tracking.menu_handle,
                                                tracking.highlighted_item,
                                            ))
                                        })
                                })
                                .unwrap_or((0, 0));
                            // MenuFlash stores the caller-selected blink count;
                            // each blink has one hidden and one visible phase.
                            let tick = bus.read_long(crate::memory::globals::addr::TICKS);
                            let flashes = bus.read_word(crate::memory::globals::addr::MENU_FLASH);
                            let flash_enabled = self
                                .menu_tracking
                                .with_tracking_mut(|tracking| {
                                    tracking.set_flash_tick(tick);
                                    tracking.begin_flash(flashes, result)
                                })
                                .unwrap();
                            self.record_menuselect_input_trace(
                                "release",
                                None,
                                self.menu_index_for_handle(active_menu),
                                Some(item_idx),
                                Some(result),
                                if !flash_enabled {
                                    "blink_disabled"
                                } else {
                                    "start_flash"
                                },
                            );
                            if !flash_enabled {
                                self.finish_custom_menu_tracking(cpu, bus, 4, result);
                            }
                        } else {
                            // No item selected — return 0 immediately
                            let (sp, active_menu) = self
                                .menu_tracking
                                .as_ref()
                                .map(|tracking| {
                                    (self.menu_tracking.context().classic_stack(), tracking.menu_handle)
                                })
                                .unwrap();
                            let saved = self.menu_tracking.take().unwrap();
                            self.restore_menu_tracking_pixels(bus, saved);
                            bus.write_word(addr::THE_MENU, 0);
                            self.draw_menu_bar_to_fb(bus);
                            self.finish_menu_no_hit(bus, cpu, sp, 4);
                            self.record_menuselect_input_trace(
                                "release",
                                None,
                                self.menu_index_for_handle(active_menu),
                                Some(0),
                                Some(0),
                                "no_selection",
                            );
                        }
                    } else {
                        // Button still held — update highlight
                        let (mv, mh) = self.menu_tracking_mouse_pos(bus);

                        // Check if mouse moved to a different menu title
                        let mbar_h =
                            bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
                        if mv < mbar_h {
                            let new_menu = self.current_menu_title_hit_test(bus, mh);
                            let tracking = self.menu_tracking.as_ref().unwrap();
                            if let Some(new_idx) = new_menu {
                                if self.menu_handle_for_index(new_idx) != Some(tracking.menu_handle) {
                                    // Switch to different menu
                                    let old_saved = self.menu_tracking.take().unwrap();
                                self.restore_menu_tracking_pixels(bus, old_saved);
                                if self.open_menu_dropdown(bus, new_idx) {
                                    self.prepare_menu_definition_port(cpu, bus);
                                    if !self.arm_pending_menu_definition(
                                        cpu,
                                        bus,
                                        cpu.read_reg(Register::PC).wrapping_sub(2),
                                    ) {
                                        self.abort_custom_menu_tracking(cpu, bus, 4);
                                    }
                                } else {
                                    self.restore_menu_definition_port(cpu, bus);
                                }
                                self.record_menuselect_input_trace(
                                    "tracking_switch",
                                    None,
                                    Some(new_idx),
                                    Some(0),
                                    None,
                                    "menu_title_changed",
                                );
                                // Don't advance PC
                                return Some(Ok(()));
                            }
                        }
                    }

                    let old_trace = self.menu_tracking.as_ref().map(|tracking| {
                            tracking
                                .submenus
                                .last()
                                .map(|submenu| (submenu.menu_handle, submenu.highlighted_item))
                                .unwrap_or((tracking.menu_handle, tracking.highlighted_item))
                        });
                        self.update_menu_tracking_for_point(bus, mh, mv);
                        if self
                            .active_menu_definition()
                            .is_some_and(|definition| definition.pending_invocation().is_some())
                        {
                            self.prepare_menu_definition_port(cpu, bus);
                            if !self.arm_pending_menu_definition(
                                cpu,
                                bus,
                                cpu.read_reg(Register::PC).wrapping_sub(2),
                            ) {
                                self.abort_custom_menu_tracking(cpu, bus, 4);
                            }
                            return Some(Ok(()));
                        }
                        let new_trace = self.menu_tracking.as_ref().map(|tracking| {
                            tracking
                                .submenus
                                .last()
                                .map(|submenu| (submenu.menu_handle, submenu.highlighted_item))
                                .unwrap_or((tracking.menu_handle, tracking.highlighted_item))
                        });
                        if new_trace != old_trace {
                            let (active_menu, highlighted_item) = new_trace.unwrap_or((0, 0));
                            self.record_menuselect_input_trace(
                                "tracking_update",
                                None,
                                self.menu_index_for_handle(active_menu),
                                Some(highlighted_item),
                                None,
                                if highlighted_item > 0 {
                                    "enabled_item_highlighted"
                                } else {
                                    "no_enabled_item"
                                },
                            );
                        }
                        // Don't advance PC — stay on the trap
                    }
                } else {
                    // First call: read mouse position and open menu
                    let sp = cpu.read_reg(Register::A7);
                    let pt_v = bus.read_word(sp) as i16;
                    let pt_h = bus.read_word(sp + 2) as i16;
                    self.refresh_menus_from_memory(bus);
                    // Don't pop stack yet — we'll do that when tracking completes

                    // MenuSelect startPt is the global mouse-down point
                    // supplied by the application; the Menu Manager uses it
                    // to choose the initial menu before it owns the tracking
                    // loop. Inside Macintosh Volume I, I-355.
                    if let Some(menu_idx) = self.current_menu_title_hit_test(bus, pt_h) {
                        self.record_menuselect_input_trace(
                            "start",
                            Some((pt_v, pt_h)),
                            Some(menu_idx),
                            Some(0),
                            None,
                            "open_tracking",
                        );
                        // Pop the Point parameter (4 bytes) but keep result space
                        // Stack on entry: SP+0: pt(4), SP+4: result(4)
                        // We store SP so we can write result later
                        if self.open_menu_dropdown(bus, menu_idx) {
                            self.prepare_menu_definition_port(cpu, bus);
                            if !self.arm_pending_menu_definition(
                                cpu,
                                bus,
                                cpu.read_reg(Register::PC).wrapping_sub(2),
                            ) {
                                self.abort_custom_menu_tracking(cpu, bus, 4);
                            }
                        }
                        self.record_menuselect_input_trace(
                            "tracking_entered",
                            Some((pt_v, pt_h)),
                            Some(menu_idx),
                            Some(0),
                            None,
                            "menu_title_active",
                        );
                        // Don't advance PC — re-fire on next iteration
                    } else {
                        self.record_menuselect_input_trace(
                            "start",
                            Some((pt_v, pt_h)),
                            None,
                            None,
                            Some(0),
                            "no_menu_title",
                        );
                        // Click not on any menu title — return 0
                        self.finish_menu_no_hit(bus, cpu, sp, 4);
                    }
                }
                Ok(())
            }

            // PopUpMenuSelect ($A80B)
            // Displays a pop-up menu, tracks the mouse, and returns the selected item.
            // FUNCTION PopUpMenuSelect(menu: MenuHandle; top, left, popUpItem: INTEGER): LONGINT;
            // Macintosh Toolbox Essentials 1992, 3-120
            // PopUpMenuSelect ($A80B): Full re-fire tracking with dropdown display, item highlighting, flash animation; uses MenuTrackingState
            (true, 0x00B) => {
                if self.menu_tracking.context().caller_isa() == Some(GuestIsa::PowerPc)
                {
                    // Preserve an outer native retained call and return from
                    // this nested Pascal call through its own result slot.
                    let sp = cpu.read_reg(Register::A7);
                    self.finish_menu_no_hit(bus, cpu, sp, 10);
                    return Some(Ok(()));
                }
                if self.menu_tracking.is_some() {
                    if self.active_menu_definition().is_some() {
                        let completed = self.complete_pending_menu_definition(cpu, bus);
                        if completed == Some(crate::menu_manager::MenuDefinitionMessage::PopUp) {
                            let rect = self.active_menu_definition().unwrap().menu_rect();
                            if rect.2 <= rect.0 || rect.3 <= rect.1 {
                                self.abort_custom_menu_tracking(cpu, bus, 10);
                                return Some(Ok(()));
                            }
                            let menu_handle = self.menu_tracking.as_ref().unwrap().menu_handle;
                            let Some(menu_idx) = self.menu_index_for_handle(menu_handle) else {
                                self.abort_custom_menu_tracking(cpu, bus, 10);
                                return Some(Ok(()));
                            };
                            self.restore_visible_dialog_snapshots(bus);
                            let saved = self.save_dropdown_pixels(bus, rect);
                            let mut tracking = tracked_menu_state(
                                MenuTrackingKind::PopUp,
                                menu_handle,
                                rect,
                                saved,
                            );
                            tracking.definition = self
                                .menu_tracking
                                .with_context_mut(|context| context.definition.take());
                            self.menu_tracking.set(Some(tracking));
                            self.draw_menu_dropdown_chrome(bus, menu_idx, rect);
                            self.with_active_menu_definition_mut(|definition| definition.draw())
                                .unwrap();
                            if !self.arm_pending_menu_definition(
                                cpu,
                                bus,
                                cpu.read_reg(Register::PC).wrapping_sub(2),
                            ) {
                                self.abort_custom_menu_tracking(cpu, bus, 10);
                            }
                            return Some(Ok(()));
                        }

                        if self
                            .menu_tracking
                            .as_ref()
                            .is_some_and(|tracking| tracking.is_flashing())
                        {
                            let tick = bus.read_long(crate::memory::globals::addr::TICKS);
                            match self
                                .menu_tracking
                                .with_tracking_mut(|tracking| tracking.advance_flash_at(tick))
                                .unwrap()
                            {
                                MenuFlashStep::Wait | MenuFlashStep::Inactive => {
                                    return Some(Ok(()))
                                }
                                MenuFlashStep::Complete(result) => {
                                    self.finish_custom_menu_tracking(cpu, bus, 10, result);
                                    return Some(Ok(()));
                                }
                                MenuFlashStep::Highlight(_) => {}
                            }
                            if !self.arm_pending_menu_definition(
                                cpu,
                                bus,
                                cpu.read_reg(Register::PC).wrapping_sub(2),
                            ) {
                                self.abort_custom_menu_tracking(cpu, bus, 10);
                            }
                            return Some(Ok(()));
                        }

                        let (mv, mh) = self.menu_tracking_mouse_pos(bus);
                        let hit_point = (u32::from(mv as u16) << 16) | u32::from(mh as u16);
                        let choose_armed = self
                            .with_active_menu_definition_mut(|tracking| tracking.choose(hit_point))
                            .flatten()
                            .is_some();
                        if choose_armed {
                            if !self.arm_pending_menu_definition(
                                cpu,
                                bus,
                                cpu.read_reg(Register::PC).wrapping_sub(2),
                            ) {
                                self.abort_custom_menu_tracking(cpu, bus, 10);
                            }
                            return Some(Ok(()));
                        }
                        if !self.menu_tracking_button_down(bus) {
                            let definition = self.active_menu_definition().unwrap().clone();
                            let item = definition.which_item();
                            let menu_ptr = bus.read_long(definition.menu_handle());
                            let result = if item > 0 && menu_ptr != 0 {
                                (u32::from(bus.read_word(menu_ptr)) << 16) | u32::from(item as u16)
                            } else {
                                0
                            };
                            if result == 0 {
                                self.finish_custom_menu_tracking(cpu, bus, 10, 0);
                            } else {
                                let tick = bus.read_long(crate::memory::globals::addr::TICKS);
                                let flashes =
                                    bus.read_word(crate::memory::globals::addr::MENU_FLASH);
                                let flash_enabled = self
                                    .menu_tracking
                                    .with_tracking_mut(|tracking| {
                                        tracking.set_flash_tick(tick);
                                        tracking.begin_flash(flashes, result)
                                    })
                                    .unwrap();
                                if !flash_enabled {
                                    self.finish_custom_menu_tracking(cpu, bus, 10, result);
                                }
                            }
                        }
                        return Some(Ok(()));
                    }
                    // Re-fire: popup tracking is active
                    if self.menu_tracking.as_ref().unwrap().is_flashing() {
                        let tick = bus.read_long(crate::memory::globals::addr::TICKS);
                        if let MenuFlashStep::Complete(result) = self
                            .menu_tracking
                            .with_tracking_mut(|tracking| tracking.advance_flash_at(tick))
                            .unwrap()
                        {
                            let sp = self.menu_tracking.context().classic_stack();
                            let saved = self.menu_tracking.take().unwrap();
                            self.restore_menu_tracking_pixels(bus, saved);
                            self.restore_visible_dialog_snapshots(bus);
                            // Stack: popUpItem(2) + left(2) + top(2) + menu(4) = 10 bytes
                            bus.write_long(sp + 10, result);
                            cpu.write_reg(Register::A7, sp + 10);
                        }
                    } else if !self.menu_tracking_button_down(bus) {
                        // As with MenuSelect, the release point itself is a
                        // valid final item hit even without an earlier
                        // held-button tracking refire. IM:I I-355.
                        let (mv, mh) = self.menu_tracking_mouse_pos(bus);
                        self.update_menu_tracking_for_point(bus, mh, mv);
                        let result = self.menu_tracking_selection_result(bus);
                        if result != 0 {
                            let tick = bus.read_long(crate::memory::globals::addr::TICKS);
                            let flashes = bus.read_word(crate::memory::globals::addr::MENU_FLASH);
                            let flash_enabled = self
                                .menu_tracking
                                .with_tracking_mut(|tracking| {
                                    tracking.set_flash_tick(tick);
                                    tracking.begin_flash(flashes, result)
                                })
                                .unwrap();
                            if !flash_enabled {
                                self.finish_custom_menu_tracking(cpu, bus, 10, result);
                            }
                        } else {
                            let sp = self.menu_tracking.context().classic_stack();
                            let saved = self.menu_tracking.take().unwrap();
                            self.restore_menu_tracking_pixels(bus, saved);
                            self.restore_visible_dialog_snapshots(bus);
                            self.finish_menu_no_hit(bus, cpu, sp, 10);
                        }
                    } else {
                        // Button held — update highlight
                        let (mv, mh) = self.menu_tracking_mouse_pos(bus);
                        self.update_menu_tracking_for_point(bus, mh, mv);
                    }
                } else {
                    // First call: read params and open popup dropdown
                    let sp = cpu.read_reg(Register::A7);
                    // If the mouse button is not down when PopUpMenuSelect is called,
                    // return 0 immediately without tracking or drawing dropdown per IM:Toolbox Essentials 3-120.
                    if !self.menu_tracking_button_down(bus) {
                        self.finish_menu_no_hit(bus, cpu, sp, 10);
                        return Some(Ok(()));
                    }
                    let request = PopupMenuRequest {
                        menu_handle: bus.read_long(sp + 6),
                        anchor: (bus.read_word(sp + 4) as i16, bus.read_word(sp + 2) as i16),
                        requested_item: bus.read_word(sp) as i16,
                    };
                    let menu_handle = request.menu_handle;
                    // Stack: popUpItem(2) + left(2) + top(2) + menu(4) + result(4)
                    // Don't pop yet — store SP for result write later

                    let menu_ptr = bus.read_long(menu_handle);
                    if menu_ptr == 0 {
                        self.finish_menu_no_hit(bus, cpu, sp, 10);
                        return Some(Ok(()));
                    }
                    let menu_id = bus.read_word(menu_ptr) as i16;

                    if let Some(menu_idx) = self
                        .menus
                        .iter()
                        .position(|m| m.handle == menu_handle || m.id == menu_id)
                    {
                        if !self.menu_uses_standard_definition(bus, menu_ptr) {
                            self.menu_tracking.set(Some(tracked_menu_state(
                                MenuTrackingKind::PopUp,
                                menu_handle,
                                (0, 0, 0, 0),
                                Default::default(),
                            )));
                            self.menu_tracking.with_context_mut(|context| {
                                context.definition = Some(request.begin_definition());
                            });
                            self.prepare_menu_definition_port(cpu, bus);
                            if !self.arm_pending_menu_definition(
                                cpu,
                                bus,
                                cpu.read_reg(Register::PC).wrapping_sub(2),
                            ) {
                                self.abort_custom_menu_tracking(cpu, bus, 10);
                            }
                            return Some(Ok(()));
                        }
                        // Popup menus bypass `open_menu_dropdown`, so fault
                        // their item-icon resources in here before sizing and
                        // drawing. Macintosh Toolbox Essentials (1992),
                        // pp. 3-46 and 3-137 to 3-138: the standard MDEF
                        // resolves item icon number + 256, with `cicn`
                        // taking precedence over the monochrome families.
                        self.preload_menu_item_icon_resources(bus, menu_idx);
                        let Some((dd_rect, highlighted_item, content_top)) =
                            self.popup_menu_dropdown_rect(bus, menu_idx, request)
                        else {
                            self.finish_menu_no_hit(bus, cpu, sp, 10);
                            return Some(Ok(()));
                        };

                        self.restore_visible_dialog_snapshots(bus);
                        let saved = self.save_dropdown_pixels(bus, dd_rect);
                        self.menu_tracking.set(Some(tracked_menu_state_with_content_top(
                            MenuTrackingKind::PopUp,
                            menu_handle,
                            dd_rect,
                            content_top,
                            saved,
                        )));
                        let rows = self.menu_rows(bus, &self.menus[menu_idx].items);
                        Self::write_menu_scrolling_globals(bus, &rows, content_top);
                        self.draw_menu_dropdown(bus, menu_idx, dd_rect);
                        if highlighted_item > 0 {
                            self.set_menu_tracking_highlight(bus, highlighted_item);
                        }
                    } else {
                        // Menu not found — return 0
                        self.finish_menu_no_hit(bus, cpu, sp, 10);
                    }
                }
                Ok(())
            }

            // HiliteMenu ($A938)
            // Highlights or unhighlights a menu title in the menu bar.
            // PROCEDURE HiliteMenu(menuID: INTEGER);
            // Macintosh Toolbox Essentials (1992), pp. 3-119 and 3-153.
            (true, 0x138) => {
                let sp = cpu.read_reg(Register::A7);
                let requested_menu_id = bus.read_word(sp) as i16;
                cpu.write_reg(Register::A7, sp + 2);

                // If there's still a dropdown open, close it
                if let Some(tracking) = self.menu_tracking.take() {
                    self.restore_menu_tracking_pixels(bus, tracking);
                }
                let target_menu_id = self
                    .current_menu_list(bus)
                    .and_then(|menu_list| {
                        menu_list.find_regular_handle_by_id(requested_menu_id, |handle| {
                            let menu = bus.read_long(handle);
                            (menu != 0).then(|| bus.read_word(menu) as i16)
                        })
                    })
                    .map_or(0, |_| requested_menu_id);
                bus.write_word(addr::THE_MENU, target_menu_id as u16);
                // Skip drawing when the app has no installed menus; the low
                // memory state is still cleared above, but no spare menu-bar
                // border is stamped into the framebuffer.
                if !self.menus.is_empty() {
                    self.draw_menu_bar_to_fb(bus);
                }
                Ok(())
            }

            // MenuKey ($A93E)
            // Determines which menu item corresponds to a given keyboard equivalent.
            // FUNCTION MenuKey(ch: CHAR): LONGINT;
            // Inside Macintosh Volume I, I-355
            // Stack: SP+0: ch (2 bytes), SP+2: result (4 bytes).
            // Callee pops 2 bytes (ch), leaves LONGINT at SP.
            // MenuKey ($A93E): Searches regular menus right-to-left, then the
            // hierarchical portion. IM:I I-356; IM:V V-245.
            (true, 0x13E) => {
                let sp = cpu.read_reg(Register::A7);
                let ch = (bus.read_word(sp) & 0xFF) as u8;

                let selection = self.current_menu_list(bus).and_then(|menu_list| {
                    menu_list.menu_key_selection(ch, |menu_handle| {
                        menu_key_menu_from_memory(bus, menu_handle)
                    })
                });
                let result = selection.map_or(0, |selection| selection.packed_result());
                // The selected submenu ID remains in TheMenu while the
                // shared hierarchy resolver redraws its owning regular title.
                // A miss clears any prior command-key highlight, matching the
                // native PowerPC gateway.
                bus.write_word(addr::THE_MENU, (result >> 16) as u16);
                if !self.menus.is_empty() {
                    self.draw_menu_bar_to_fb(bus);
                }

                if std::env::var_os("SYSTEMLESS_TRACE_MENUKEY").is_some() {
                    eprintln!(
                        "[MENUKEY] MenuKey ch=${:02X} '{}' -> ${:08X}",
                        ch,
                        if ch.is_ascii_graphic() {
                            ch as char
                        } else {
                            '.'
                        },
                        result
                    );
                    for menu in &self.menus {
                        eprintln!(
                            "[MENUKEY]   menu {} \"{}\" in_bar={} visible={} enabled={} items={}",
                            menu.id,
                            menu.title,
                            menu.in_menu_bar,
                            menu.visible_in_menu_bar,
                            menu.enabled,
                            menu.items.len()
                        );
                    }
                }

                bus.write_long(sp + 2, result);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // CheckItem ($A945)
            // Sets or clears the check mark for a menu item.
            // PROCEDURE CheckItem(theMenu: MenuHandle; item: INTEGER; checked: BOOLEAN);
            // Inside Macintosh Volume I, I-358
            //
            // Pascal BOOLEAN: TRUE = $0100 (byte 1 in high byte of word).
            // The low byte (SP+1) holds stale stack bytes so only SP+0
            // carries the value.
            (true, 0x145) => {
                let sp = cpu.read_reg(Register::A7);
                let checked = bus.read_byte(sp) != 0;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);

                self.mutate_menu_items(bus, menu_handle, |items| {
                    items.set_mark(item, if checked { 0x12 } else { 0 })
                });
                Ok(())
            }

            // SetItem ($A947)
            // Changes the text of a menu item; does not affect other attributes.
            // PROCEDURE SetItem(theMenu: MenuHandle; item: INTEGER; itemString: Str255);
            // Inside Macintosh Volume I, I-357
            (true, 0x147) => {
                let sp = cpu.read_reg(Register::A7);
                let text_ptr = bus.read_long(sp);
                let item = bus.read_word(sp + 4) as i16;
                let mut menu_handle = bus.read_long(sp + 6);
                cpu.write_reg(Register::A7, sp + 10);
                if menu_handle == 0 {
                    let a0_handle = cpu.read_reg(Register::A0);
                    if let Some(resolved) = self.resolve_menu_handle_candidate(bus, a0_handle) {
                        menu_handle = resolved;
                    }
                }
                if text_ptr != 0 && item >= 1 {
                    let text_len = bus.read_byte(text_ptr) as usize;
                    let mut text_bytes = Vec::with_capacity(text_len);
                    for i in 0..text_len {
                        text_bytes.push(bus.read_byte(text_ptr + 1 + i as u32));
                    }
                    self.mutate_menu_items(bus, menu_handle, |items| {
                        items.set_text(item, &text_bytes)
                    });
                }
                Ok(())
            }

            // DisposeMenu ($A932)
            // Disposes of a menu and releases its memory.
            // PROCEDURE DisposeMenu(theMenu: MenuHandle);
            // Inside Macintosh Volume I, I-352
            // DisposeMenu ($A932): Releases NewMenu-allocated menu memory and
            // consumes one MenuHandle argument from the stack.
            (true, 0x132) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                self.menus.retain(|m| m.handle != menu_handle);
                if menu_handle != 0 {
                    let menu_ptr = bus.read_long(menu_handle);
                    if menu_ptr != 0 {
                        bus.free(menu_ptr);
                        self.untrack_handle_ptr(menu_ptr);
                    }
                    self.forget_resource_handle_index_for_handle(menu_handle);
                    self.with_resource_manager_mut(|resource_manager| {
                        resource_manager.loaded_handles.remove(&menu_handle);
                        resource_manager.detached_handles.remove(&menu_handle);
                        resource_manager.resource_handle_files.remove(&menu_handle);
                        resource_manager.detached_handle_files.remove(&menu_handle);
                    });
                    self.remove_handle_state_bits(menu_handle);
                    bus.free(menu_handle);
                }
                Ok(())
            }

            // DeleteMenu ($A936)
            // Removes the menu with the given ID from the current menu list.
            // PROCEDURE DeleteMenu(menuID: INTEGER);
            // Macintosh Toolbox Essentials (1992), p. 3-109
            (true, 0x136) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_id = bus.read_word(sp) as i16;
                cpu.write_reg(Register::A7, sp + 2);
                let mut menu_list = self.current_menu_list(bus).unwrap_or_default();
                if menu_list
                    .remove_by_id(menu_id, |candidate| {
                        let menu_ptr = bus.read_long(candidate);
                        (menu_ptr != 0).then(|| bus.read_word(menu_ptr) as i16)
                    })
                    .is_some()
                {
                    self.relayout_menu_list(bus, &mut menu_list);
                    if self.replace_current_menu_list(bus, &menu_list) {
                        self.apply_menu_list_membership(bus, &menu_list);
                    }
                }
                // IM:V 1986 p. V-244: DeleteMenu removes all color
                // entries for the deleted menu ID from MenuCInfo.
                self.filter_menu_color_table_entries(bus, |id, _item| id != menu_id);
                Ok(())
            }

            // CountMItems ($A950)
            // Returns the number of items in the specified menu.
            // FUNCTION CountMItems(theMenu: MenuHandle): INTEGER;
            // Macintosh Toolbox Essentials (1992), p. 3-141
            (true, 0x150) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_handle = bus.read_long(sp);
                // Parse the MENU data structure directly from guest memory
                // rather than looking up self.menus. This handles menus loaded
                // via GetMenu that aren't inserted into the menu bar.
                let count = count_menu_items_from_memory(bus, menu_handle);
                bus.write_word(sp + 4, count);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // GetItemCmd ($A84E)
            // Returns the keyboard equivalent of a menu item.
            // PROCEDURE GetItemCmd(theMenu: MenuHandle; item: INTEGER; VAR cmdChar: CHAR);
            // Inside Macintosh Volume V, V-235
            // GetItemCmd ($A84E): Reads key equivalent from MENU data in guest memory
            (true, 0x04E) => {
                let sp = cpu.read_reg(Register::A7);
                let cmd_ptr = bus.read_long(sp);
                let item = bus.read_word(sp + 4) as i16;
                let menu_handle = bus.read_long(sp + 6);
                if cmd_ptr != 0 {
                    let cmd_char = if menu_handle != 0 {
                        let menu_ptr = bus.read_long(menu_handle);
                        if menu_ptr != 0 {
                            get_menu_item_field(bus, menu_ptr, item, 1)
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    bus.write_word(cmd_ptr, cmd_char as u16);
                }
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // GetItemIcon ($A93F)
            // Returns the icon number of a menu item.
            // PROCEDURE GetItemIcon(theMenu: MenuHandle; item: INTEGER; VAR icon: Byte);
            // Inside Macintosh Volume I, I-359
            //
            // Writes the byte zero-extended into a 16-bit word — MPW's
            // Universal Headers typedef the out-ptr as `short *`, so callers
            // with `short icon; GetItemIcon(..., &icon)` expect the 2-byte
            // slot to contain the zero-extended byte value.
            // GetItemIcon ($A93F): Reads icon byte from menu item per IM:I I-359
            (true, 0x13F) => {
                let sp = cpu.read_reg(Register::A7);
                let icon_ptr = bus.read_long(sp);
                let item = bus.read_word(sp + 4) as i16;
                let menu_handle = bus.read_long(sp + 6);
                if icon_ptr != 0 {
                    let icon = self
                        .menus
                        .iter()
                        .find(|m| m.handle == menu_handle)
                        .and_then(|m| m.items.get((item - 1) as usize))
                        .map(|mi| mi.icon)
                        .unwrap_or(0);
                    bus.write_word(icon_ptr, icon as u16);
                }
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // SetItemIcon ($A940)
            // Sets the icon number of a menu item.
            // PROCEDURE SetItemIcon(theMenu: MenuHandle; item: INTEGER; icon: Byte);
            // Inside Macintosh Volume I, I-359
            (true, 0x140) => {
                let sp = cpu.read_reg(Register::A7);
                let icon = (bus.read_word(sp) & 0xFF) as u8;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                self.mutate_menu_items(bus, menu_handle, |items| items.set_icon(item, icon));
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GetItemStyle ($A941)
            // Returns the character style of a menu item.
            // PROCEDURE GetItemStyle(theMenu: MenuHandle; item: INTEGER; VAR chStyle: Style);
            // Inside Macintosh Volume I, I-359
            //
            // Zero-extend to 16-bit word like GetItemIcon to match MPW
            // `Style *` (= `short *` in the headers) caller convention.
            // GetItemStyle ($A941): Reads style byte from menu item per IM:I I-359
            (true, 0x141) => {
                let sp = cpu.read_reg(Register::A7);
                let style_ptr = bus.read_long(sp);
                let item = bus.read_word(sp + 4) as i16;
                let menu_handle = bus.read_long(sp + 6);
                if style_ptr != 0 {
                    let style = self
                        .menus
                        .iter()
                        .find(|m| m.handle == menu_handle)
                        .and_then(|m| m.items.get((item - 1) as usize))
                        .map(|mi| mi.style)
                        .unwrap_or(0);
                    bus.write_word(style_ptr, style as u16);
                }
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // SetItemStyle ($A942)
            // Sets the character style of a menu item.
            // PROCEDURE SetItemStyle(theMenu: MenuHandle; item: INTEGER; chStyle: Style);
            // Inside Macintosh Volume I, I-359
            (true, 0x142) => {
                let sp = cpu.read_reg(Register::A7);
                let style = (bus.read_word(sp) & 0xFF) as u8;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                self.mutate_menu_items(bus, menu_handle, |items| items.set_style(item, style));
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GetItemMark ($A943)
            // Returns the mark character of a menu item.
            // PROCEDURE GetItemMark(theMenu: MenuHandle; item: INTEGER; VAR markChar: CHAR);
            // Inside Macintosh Volume I, I-358
            // GetItemMark ($A943): Returns mark from internal menu item
            (true, 0x143) => {
                let sp = cpu.read_reg(Register::A7);
                let mark_ptr = bus.read_long(sp);
                let item = bus.read_word(sp + 4) as i16;
                let menu_handle = bus.read_long(sp + 6);
                if mark_ptr != 0 {
                    let mark = self
                        .menus
                        .iter()
                        .find(|m| m.handle == menu_handle)
                        .and_then(|m| m.items.get((item - 1) as usize))
                        .map(|mi| mi.mark)
                        .unwrap_or(0);
                    bus.write_word(mark_ptr, mark as u16);
                }
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // SetItemMark ($A944)
            // Sets the mark character of a menu item.
            // PROCEDURE SetItemMark(theMenu: MenuHandle; item: INTEGER; markChar: CHAR);
            // Inside Macintosh Volume I, I-358
            (true, 0x144) => {
                let sp = cpu.read_reg(Register::A7);
                let mark_char = (bus.read_word(sp) & 0xFF) as u8;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                self.mutate_menu_items(bus, menu_handle, |items| items.set_mark(item, mark_char));
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GetItem (0xA946)
            // Returns the text of the specified menu item.
            // PROCEDURE GetItem(theMenu: MenuHandle; item: INTEGER; VAR itemString: Str255);
            // Inside Macintosh Volume I, I-357
            // GetItem ($A946): Returns item text from internal menu per IM:I I-357
            (true, 0x146) => {
                let sp = cpu.read_reg(Register::A7);
                let str_ptr = bus.read_long(sp);
                let item = bus.read_word(sp + 4) as i16;
                let menu_handle = bus.read_long(sp + 6);
                if str_ptr != 0 {
                    if let Some(text) = self
                        .menus
                        .iter()
                        .find(|m| m.handle == menu_handle)
                        .and_then(|m| m.items.get((item - 1) as usize))
                        .map(|mi| mi.text.clone())
                    {
                        bus.write_pstring(str_ptr, text.as_bytes());
                    } else {
                        bus.write_byte(str_ptr, 0);
                    }
                }
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // CalcMenuSize ($A948)
            // Recalculates the horizontal and vertical dimensions of a menu
            // and stores them in the menuWidth and menuHeight fields.
            // PROCEDURE CalcMenuSize(theMenu: MenuHandle);
            // Inside Macintosh Volume I, I-361
            (true, 0x148) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);

                let menu_ptr = bus.read_long(menu_handle);
                if menu_ptr != 0
                    && self.arm_menu_definition(
                        cpu,
                        bus,
                        SharedMenuDefinitionInvocation::size(menu_handle),
                    )
                {
                    return Some(Ok(()));
                }

                self.calculate_standard_menu_size(bus, menu_handle);
                Ok(())
            }

            // SetMenuFlash ($A94A)
            // Sets the number of times a selected menu item blinks.
            // PROCEDURE SetMenuFlash(count: INTEGER);
            // Inside Macintosh Volume I, I-361
            (true, 0x14A) => {
                let sp = cpu.read_reg(Register::A7);
                let count = bus.read_word(sp);
                bus.write_word(crate::memory::globals::addr::MENU_FLASH, count);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // PlotIcon ($A94B)
            // Draws the 32x32 1bpp ICON resource referred to by
            // `theIcon` into `theRect` of the current port via CopyBits
            // with srcCopy mode per IM:I I-473 ("PlotIcon draws the
            // icon whose handle is theIcon in the rectangle theRect,
            // which is in the local coordinates of the current
            // grafPort. It calls the QuickDraw procedure CopyBits and
            // uses the srcCopy transfer mode."). When the destination
            // rect is not 32×32 the icon is scaled per IM:V V-65
            // CopyBits scaling rules: OR-compress on shrink (every src
            // pixel that maps to a dst pixel contributes a logical OR
            // — preserves visual mass), nearest-neighbor sample on
            // magnify with `sx = (dx*2*32 + 32) / (dst_w*2)` rounding
            // (matches real-Mac 1.5× / 2× scaling for ICON family).
            // PROCEDURE PlotIcon(theRect: Rect; theIcon: Handle);
            // Inside Macintosh Volume I, I-473
            // Inside Macintosh Volume V, V-65 (CopyBits scaling)
            //
            // Stack: SP+0 theIcon (4-byte Handle), SP+4 theRect (4-byte
            // Rect-by-pointer per IM:I-91 "a Rect is an 8-byte record,
            // so push a pointer to it"). Pop 8 bytes. NIL theIcon /
            // NIL rect / NIL master ptr / 0-area dst are defensive
            // no-ops; current_port == 0 is a defensive no-op and is
            // checked before any icon/rect dereference (no current
            // grafPort means nothing to draw to).
            //
            // HLE compromise (Partial vs Complete): only handles 1bpp
            // ICON (32×32 monochrome) resources; cicn / 'cicn' colour
            // icons go through GetCIcon ($AA1E) -> PlotCIcon ($AA1F)
            // separately. Pixel-format dispatch handles dst.pixel_size
            // 1 (b/w fb OR-paint) + 8 (8-bit fb white-paint = 0xFF);
            // 16/32-bit colour fb is silently no-op (matches the
            // IM:V V-65 "no colour" assumption for legacy ICON paths).
            // Status note: this was tagged Stub since the
            // Menu Manager promotion sweep but the body has been
            // substantive bitmap drawing for many iterations — same
            // status issue as UpdtControl / Draw1Control / GetItemCmd
            // / GetItemIcon / GetItemStyle / GetItemMark all of which
            // were silently mislabeled and surfaced via prior audit
            // passes.
            // PlotIcon ($A94B): OR-compress 1bpp / 8bpp pixel writes for shrink + nearest-neighbor for magnify per IM:V V-65 CopyBits scaling; ICON (32×32 mono) only — cicn handled via PlotCIcon $AA1F; NIL handle / NIL rect / NIL master ptr / zero-area / no-port are defensive no-ops; 16/32-bit colour fb silently no-op
            //
            // Stack discipline: A7 pops the 8-byte argument frame and is
            // net-balanced across both single and repeated PlotIcon
            // compositions, including the current-port-zero defensive
            // no-op path.
            (true, 0x14B) => {
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 8);

                let port = *self.current_port;
                if port == 0 {
                    return Some(Ok(()));
                }
                let icon_handle = bus.read_long(sp);
                let rect_ptr = bus.read_long(sp + 4);
                if icon_handle == 0 || rect_ptr == 0 {
                    return Some(Ok(()));
                }
                let icon_ptr = bus.read_long(icon_handle);
                if icon_ptr == 0 {
                    return Some(Ok(()));
                }

                let top = bus.read_word(rect_ptr) as i16;
                let left = bus.read_word(rect_ptr + 2) as i16;
                let bottom = bus.read_word(rect_ptr + 4) as i16;
                let right = bus.read_word(rect_ptr + 6) as i16;
                let dst_w = (right - left) as i32;
                let dst_h = (bottom - top) as i32;
                if dst_w <= 0 || dst_h <= 0 {
                    return Some(Ok(()));
                }
                let dst = self.resolve_copy_bitmap(bus, port.wrapping_add(2));

                // OR-compress the icon per IM:V V-65: when dst is smaller
                // than the 32×32 source, each dst pixel must OR-merge ALL
                // src pixels that map to it. For MAGNIFY cases (dst >= 32),
                // use center-of-pixel nearest-neighbor sampling — `sx =
                // (dx*2*32 + 32) / (dst_w*2)` matches real-Mac rounding for
                // non-integer scale ratios like 1.5× (32→48).
                // Inside Macintosh Volume V, V-65 (CopyBits scaling).
                let magnify_x = dst_w >= 32;
                let magnify_y = dst_h >= 32;
                for dy in 0..dst_h {
                    let (sy_start, sy_end) = if magnify_y {
                        let sy = ((dy * 2 * 32 + 32) / (dst_h * 2)).min(31) as u32;
                        (sy, sy + 1)
                    } else {
                        let a = (dy * 32 / dst_h) as u32;
                        let b = (((dy + 1) * 32 / dst_h).min(32) as u32).max(a + 1);
                        (a, b)
                    };
                    let py = top as i32 + dy;
                    if py < dst.bounds_top as i32 || py >= dst.bounds_bottom as i32 {
                        continue;
                    }
                    let row_off = (py - dst.bounds_top as i32) as u32;
                    for dx in 0..dst_w {
                        let (sx_start, sx_end) = if magnify_x {
                            let sx = ((dx * 2 * 32 + 32) / (dst_w * 2)).min(31) as u32;
                            (sx, sx + 1)
                        } else {
                            let a = (dx * 32 / dst_w) as u32;
                            let b = (((dx + 1) * 32 / dst_w).min(32) as u32).max(a + 1);
                            (a, b)
                        };
                        // OR-scan the src range.
                        let mut any_set = false;
                        'or_scan: for sy in sy_start..sy_end {
                            let row_data = bus.read_long(icon_ptr + sy * 4);
                            for sx in sx_start..sx_end {
                                if (row_data >> (31 - sx)) & 1 != 0 {
                                    any_set = true;
                                    break 'or_scan;
                                }
                            }
                        }
                        if !any_set {
                            continue;
                        }
                        let px = left as i32 + dx;
                        if px < dst.bounds_left as i32 || px >= dst.bounds_right as i32 {
                            continue;
                        }
                        let col = (px - dst.bounds_left as i32) as u32;
                        match dst.pixel_size {
                            1 => {
                                let addr = dst.base + row_off * dst.row_bytes + (col / 8);
                                let bit = 7 - (col % 8);
                                let byte = bus.read_byte(addr);
                                bus.write_byte(addr, byte | (1 << bit));
                            }
                            8 => {
                                let addr = dst.base + row_off * dst.row_bytes + col;
                                bus.write_byte(addr, 255);
                            }
                            _ => {}
                        }
                    }
                }
                Ok(())
            }

            // FlashMenuBar ($A94C)
            // PROCEDURE FlashMenuBar (menuID: INTEGER);
            // Inside Macintosh Volume I (1985), p. I-361.
            //
            // IM:I I-361: "If menuID is 0 (or isn't the ID of any menu
            // in the menu list), FlashMenuBar inverts the entire menu
            // bar; otherwise, it inverts the title of the given menu.
            // You can call FlashMenuBar(0) twice to blink the menu
            // bar."
            //
            // MPW Universal Headers Menus.h declares:
            //     EXTERN_API(void) FlashMenuBar(MenuID menuID)
            //                                  ONEWORDINLINE(0xA94C);
            //
            // The trap is a Tool-bit Pascal PROCEDURE (bit 11 set):
            // caller pushes the 2-byte menuID INTEGER, trap pops it,
            // no FUNCTION result slot is written. A7 is unchanged
            // across the call after the 2-byte argument is consumed.
            //
            // Calling-convention behavior (Apple headers and BasiliskII
            // agree):
            //   - A7 is unchanged across a single FlashMenuBar(0) call.
            //   - A7 is unchanged across a 5-call FlashMenuBar(0)
            //     composition (5 missed 2-byte pops would cumulate to
            //     10 bytes of A7 drift, which is unambiguous).
            //
            // Regression coverage:
            //   flashmenubar_five_call_composition_advances_stack_by_ten_bytes
            //   covers the 5-call composition.
            //
            // Visual side effect: on a monochrome System 7.5.3 screen the
            // strip bits are inverted. On indexed-color screens StandardMBDF
            // reverses the MCEntry(0,0) RGB4/RGB1 background/title pair, or
            // standard white/black when that entry is absent. A second call
            // blinks either representation back.
            (true, 0x14C) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_id = bus.read_word(sp) as i16;
                cpu.write_reg(Register::A7, sp + 2);
                self.flash_menu_bar(bus, menu_id);
                Ok(())
            }

            // AppendResMenu / AddResMenu ($A94D already handled above)

            // PinRect ($A94E)
            // Pins a point inside a rectangle.
            // FUNCTION PinRect(theRect: Rect; thePt: Point): LONGINT;
            // Inside Macintosh Volume I, I-193
            // PinRect ($A94E): Pins point inside rectangle
            (true, 0x14E) => {
                let sp = cpu.read_reg(Register::A7);
                let pt_v = bus.read_word(sp) as i16;
                let pt_h = bus.read_word(sp + 2) as i16;
                let rect_ptr = bus.read_long(sp + 4);
                let top = bus.read_word(rect_ptr) as i16;
                let left = bus.read_word(rect_ptr + 2) as i16;
                let bottom = bus.read_word(rect_ptr + 4) as i16;
                let right = bus.read_word(rect_ptr + 6) as i16;
                let pinned_v = pt_v.max(top).min(bottom - 1);
                let pinned_h = pt_h.max(left).min(right - 1);
                let result = ((pinned_v as u32) << 16) | (pinned_h as u16 as u32);
                bus.write_long(sp + 8, result);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // DeltaPoint ($A94F)
            // FUNCTION DeltaPoint(ptA, ptB: Point): LONGINT;
            // Inside Macintosh Volume I (1985), p. I-475 (Toolbox
            // Utilities — Miscellaneous Utilities); Inside Macintosh
            // Volume V (1986), V-258; Imaging With QuickDraw (1994),
            // pp. 2-53, 2-78..2-79.
            //
            // Subtracts the coordinates of ptB from those of ptA and
            // returns the result as a LONGINT. The high-order 16-bit
            // word is the vertical difference (ptA.v - ptB.v); the
            // low-order 16-bit word is the horizontal difference
            // (ptA.h - ptB.h). Each word is an independent signed
            // 16-bit arithmetic — the low word must not sign-extend
            // into the high word.
            //
            // Parameter order matters: DeltaPoint(A, B) returns the
            // negation of DeltaPoint(B, A). The companion procedure
            // SubPt(srcPt, VAR dstPt) computes the same difference
            // but stores the result through a VAR parameter; its
            // parameter order is reversed from DeltaPoint's.
            //
            // Stack frame (Pascal FUNCTION, 8 bytes arg + 4 bytes
            // result; Pascal pushes args right-to-left so ptB is
            // pushed last and ends up at the lower SP):
            //   SP+0  ptB.v   INTEGER (Point.v at struct offset 0)
            //   SP+2  ptB.h   INTEGER (Point.h at struct offset 2)
            //   SP+4  ptA.v   INTEGER
            //   SP+6  ptA.h   INTEGER
            //   SP+8  result  LONGINT (caller-allocated result slot)
            //
            // The trap pops the 8 argument bytes (A7 += 8) and writes
            // the 4-byte LONGINT into the result slot at the former
            // SP+8. The net externally-observed SP delta across the
            // full call site (caller pre-allocates result + push args
            // + trap + caller pops result) is zero.
            (true, 0x14F) => {
                let sp = cpu.read_reg(Register::A7);
                let pt_b_v = bus.read_word(sp) as i16;
                let pt_b_h = bus.read_word(sp + 2) as i16;
                let pt_a_v = bus.read_word(sp + 4) as i16;
                let pt_a_h = bus.read_word(sp + 6) as i16;
                let dv = pt_a_v.wrapping_sub(pt_b_v);
                let dh = pt_a_h.wrapping_sub(pt_b_h);
                let result = ((dv as u16 as u32) << 16) | (dh as u16 as u32);
                bus.write_long(sp + 8, result);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // InsertResMenu ($A951)
            // Inserts items from resources of a given type into a menu
            // at a specified position.
            // PROCEDURE InsertResMenu(theMenu: MenuHandle; theType: ResType; afterItem: INTEGER);
            // Inside Macintosh Volume IV, IV-56; IM:I I-353 (companion
            // to AddResMenu); Macintosh Toolbox Essentials 1992,
            // 3-103..3-104 (InsertResMenu — same filter rules as
            // AppendResMenu).
            //
            // afterItem semantics per IM:IV IV-56:
            //   0          — insert before the first item
            //   N (>= 1)   — insert after item N
            //   >= count   — append (degrades to AddResMenu)
            //
            // Stack: SP+0 afterItem (2), SP+2 theType (4), SP+6 theMenu handle (4). Pop 10.
            (true, 0x151) => {
                let sp = cpu.read_reg(Register::A7);
                let after_item = bus.read_word(sp) as i16;
                let res_type_word = bus.read_long(sp + 2);
                let menu_handle = bus.read_long(sp + 6);
                cpu.write_reg(Register::A7, sp + 10);

                let res_type = res_type_word.to_be_bytes();
                let names = self.resource_menu_names(bus, res_type);
                self.mutate_menu_items(bus, menu_handle, |items| {
                    items.insert_resource_names(names, after_item)
                });
                Ok(())
            }

            // DeleteMenuItem ($A952)
            // Deletes the specified item from a menu.
            // PROCEDURE DeleteMenuItem(theMenu: MenuHandle; item: INTEGER);
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-127
            (true, 0x152) => {
                let sp = cpu.read_reg(Register::A7);
                let item = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);
                let menu_id = bus.read_word(bus.read_long(menu_handle)) as i16;
                if self.mutate_menu_items(bus, menu_handle, |items| items.delete(item)) {
                    // IM:V 1986 p. V-244: DelMenuItem removes the
                    // deleted item's color entry from MenuCInfo.
                    self.filter_menu_color_table_entries(bus, |id, item_no| {
                        !(id == menu_id && item_no == item)
                    });
                }
                Ok(())
            }

            // InsertMenuItem ($A826)
            // Inserts one or more menu items after the specified item position.
            // PROCEDURE InsertMenuItem(theMenu: MenuHandle; itemString: Str255; afterItem: INTEGER);
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-126
            (true, 0x026) => {
                let sp = cpu.read_reg(Register::A7);
                let after_item = bus.read_word(sp) as i16;
                let text_ptr = bus.read_long(sp + 2);
                let menu_handle = bus.read_long(sp + 6);
                cpu.write_reg(Register::A7, sp + 10);

                if menu_handle == 0 || text_ptr == 0 {
                    return Some(Ok(()));
                }
                let len = bus.read_byte(text_ptr) as usize;
                let mut bytes = Vec::with_capacity(len);
                for i in 0..len {
                    bytes.push(bus.read_byte(text_ptr + 1 + i as u32));
                }
                self.mutate_menu_items(bus, menu_handle, |items| {
                    items.insert_specs(&bytes, after_item)
                });
                Ok(())
            }

            // InitProcMenu ($A808)
            // PROCEDURE InitProcMenu(mbResID: INTEGER);
            // Inside Macintosh Volume V (1986), p. V-244
            // (Menu Manager — New Menu Manager Routines — InitProcMenu).
            //
            // Per IM:V V-244, InitProcMenu installs a custom menu bar
            // definition procedure ('MBDF'). It allocates a new
            // MenuList if one has not already been allocated, and
            // stores mbResID into the MenuList's mbResID field. The
            // low 3 bits of mbResID are the mbVariant code; the high
            // 13 bits index the 'MBDF' resource to load. Apple
            // reserves mbResID values $000-$100 for its own use. MPW
            // Universal Headers Menus.h declares:
            //
            //   EXTERN_API(void) InitProcMenu(short resID) ONEWORDINLINE(0xA808);
            //
            // The Tool-bit Pascal PROCEDURE ABI is therefore: caller
            // pushes the 2-byte INTEGER mbResID, trap pops 2 bytes,
            // no FUNCTION result slot is written, A7 unchanged across
            // the call after the argument is consumed.
            //
            // Calling-convention behavior (Apple headers and BasiliskII
            // agree): both consume the 2-byte mbResID and preserve A7
            // across the call.
            //
            // Apple-vs-BasiliskII divergence on the side effect:
            // BasiliskII System 7.5.3 ROM Menu Manager allocates the
            // MenuList if not yet allocated, stores mbResID, and (when
            // the high 13 bits select a non-default MBDF) loads the
            // 'MBDF' resource. Systemless HLE is a true pop-2-and-return
            // stub because the host runtime draws the menu bar
            // directly from the Rust menu list — there is no separate
            // MBDF resource to honour. The visible "MBDF resource
            // gets loaded" path is intentionally not modeled.
            (true, 0x008) => {
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // InvalMenuBar ($A81D)
            // PROCEDURE InvalMenuBar;
            // Inside Macintosh: Macintosh Toolbox Essentials (1992),
            // p. 3-93 (Menu Manager — Drawing the Menu Bar —
            // InvalMenuBar).
            //
            // Per IM:MTE 1992 p. 3-93 InvalMenuBar marks the menu bar
            // as needing redraw at the next event-loop pass. When the
            // Event Manager scans update regions it also checks the
            // menu-bar-invalid flag and, if set, calls DrawMenuBar to
            // refresh the chrome. MPW Universal Headers Menus.h
            // declares:
            //
            //   EXTERN_API(void) InvalMenuBar(void) ONEWORDINLINE(0xA81D);
            //
            // The Tool-bit PROCEDURE ABI is therefore: no Pascal stack
            // argument frame, no FUNCTION result slot, A7 unchanged
            // across the call.
            //
            // Calling-convention behavior (Apple headers and BasiliskII
            // agree): both preserve A7 across the call.
            //
            // Regression coverage:
            //   src/trap/menu.rs::invalmenubar_procedure_call_preserves_stack_pointer
            (true, 0x01D) => {
                self.event_queue.invalidate_menu_bar();
                Ok(())
            }

            // SetItemCmd ($A84F)
            // Sets the command-key byte for a menu item.
            // PROCEDURE SetItemCmd(theMenu: MenuHandle; item: INTEGER; cmdChar: CHAR);
            // Inside Macintosh Volume V (1986), p. V-244
            (true, 0x04F) => {
                let sp = cpu.read_reg(Register::A7);
                let cmd_char = (bus.read_word(sp) & 0xFF) as u8;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);
                if menu_handle == 0 || item < 1 {
                    return Some(Ok(()));
                }
                self.mutate_menu_items(bus, menu_handle, |items| items.set_command(item, cmd_char));
                Ok(())
            }

            // GetMenuBar ($A93B)
            // Snapshots the current menu bar so the caller can later
            // restore it with SetMenuBar.
            // FUNCTION GetMenuBar: Handle;
            // Inside Macintosh Volume I, I-354
            //
            // The returned handle owns a byte-for-byte `DynamicMenuList`
            // copy. Menu records remain referenced by their existing handles.
            // Inside Macintosh Volume V (1986), pp. V-228--V-230.
            //
            // No parameters. Returns Handle in the pre-pushed result
            // slot at SP+0; A7 is unchanged.
            (true, 0x13B) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_list =
                    menu_list_from_memory(bus, bus.read_long(addr::MENU_LIST)).unwrap_or_default();
                let bytes = menu_list.encode();
                let block = bus.alloc(bytes.len() as u32);
                bus.write_bytes(block, &bytes);
                let handle = bus.alloc(4);
                bus.write_long(handle, block);
                bus.write_long(sp, handle);
                Ok(())
            }

            // ========== Menu Color Manager + MenuChoice ($AA60..$AA66) ==========
            //
            // Family rationale (HLE compromise documented once for the whole
            // family). Per IM:V V-241..V-248 the Menu Color Manager is the
            // System II-era mechanism for colorizing individual menu items
            // via 'mctb' resources or programmatically-built MCEntry arrays.
            // The mechanism cooperates with the standard menu definition
            // procedure ('MDEF' 0): the MDEF reads MenuCInfo ($0D50) when
            // drawing each item to pick title-bar / item / mark / chevron
            // colors per (menuID, menuItem) match. Apple deprecated this
            // mechanism in System 7.5 in favor of the Theme Manager — the
            // 'mctb' resource is treated as compatibility-only on later
            // systems (Macintosh Toolbox Essentials 1992 lists the seven
            // routines as classic-only API).
            //
            // Systemless now keeps a live MenuCInfo table in low memory so the
            // AA60..AA65 family can mutate/query real guest state. InitMenus,
            // GetMenu, and GetNewMBar now auto-load 'mctb' resources into
            // that table; the standard 8bpp dropdown paint path consumes MC
            // entries for pulled-down menu background and text-component
            // colors:
            //   * MenuCInfo at $0D50 (handle to the MC table — IM:V V-571)
            //     is created on InitMenus and then updated by the family.
            //   * MenuDisable at $0B54 (last-tracked menu/item — IM:V V-571)
            //     remains the separate MenuChoice state used by AA66.
            //
            // The seven-trap surface now behaves as a live table API:
            //   * DelMCEntries / SetMCInfo / DispMCInfo / SetMCEntries mutate
            //     or dispose the current MC table and preserve the documented
            //     Pascal stack discipline.
            //   * GetMCInfo / GetMCEntry return deep copies / live pointers
            //     into that table when one exists, and NIL when it does not.
            //   * MenuChoice ($AA66) reads lowmem MenuDisable ($0B54), which
            //     the shared standard-MDEF tracker updates while a menu is
            //     down, and writes that LongInt to the result slot.

            // DelMCEntries ($AA60)
            // PROCEDURE DelMCEntries(menuID: INTEGER; menuItem: INTEGER);
            // Inside Macintosh Volume V, V-248
            //
            // Per IM:V 1986 p. V-248, DelMCEntries deletes entries from the
            // menu color information table based on the given menuID and
            // menuItem. If the entry is not found, no entry is removed. If
            // menuItem is mctAllItems (-98), then all items for the given
            // menuID are removed. Modern MPW Universal Headers Menus.h
            // declares the trap as DeleteMCEntries with
            //   `#define DelMCEntries(menuID, menuItem)
            //    DeleteMCEntries(menuID, menuItem)`
            // aliasing.
            //
            // Tool-bit Pascal PROCEDURE ABI: caller pre-pushes 2 INTEGERs
            // (4 bytes total) left-to-right; trap pops 4 bytes; no FUNCTION
            // result slot. Stack layout at trap entry:
            //   SP+0: menuItem(2)
            //   SP+2: menuID(2)
            //
            // Absolute behavior (BasiliskII source): BII mutates the
            // system menu color information table at lowmem MenuCInfo
            // ($0D50). Systemless HLE now mirrors that live-table
            // mutation for exact (menuID, menuItem) matches; the shared
            // behavior is the Pascal PROCEDURE calling convention
            // itself: A7 advances by exactly 4 bytes per call.
            (true, 0x260) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_item = bus.read_word(sp) as i16;
                let menu_id = bus.read_word(sp + 2) as i16;
                let current_handle = bus.read_long(addr::MENU_C_INFO);
                if current_handle != 0 {
                    let mut kept = Vec::new();
                    let current_ptr = bus.read_long(current_handle);
                    if current_ptr != 0 {
                        let current_size = bus.get_alloc_size(current_ptr).unwrap_or(0) as usize;
                        let current_bytes = if current_size == 0 {
                            Vec::new()
                        } else {
                            bus.read_bytes(current_ptr, current_size)
                        };
                        if !current_bytes.is_empty() {
                            kept.reserve(current_bytes.len());
                            for entry in current_bytes.chunks_exact(MC_ENTRY_SIZE) {
                                let remove = if menu_item == MC_ALL_ITEMS {
                                    mc_entry_key(entry)
                                        .is_some_and(|(id, item)| id == menu_id && item != 0)
                                } else {
                                    mc_entry_matches(entry, menu_id, menu_item)
                                };
                                if !remove {
                                    kept.extend_from_slice(entry);
                                }
                            }
                            let _ = self.replace_handle_bytes(bus, current_handle, &kept);
                        }
                    }
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // GetMCInfo ($AA61)
            // FUNCTION GetMCInfo: MCTableHandle;
            // Inside Macintosh Volume V, V-247
            //
            // Per IM:V 1986 p. V-247, GetMCInfo creates a copy of the
            // current menu color information table and returns a handle
            // to the copy. If the copy fails, a NIL handle is returned.
            // MPW Universal Headers Menus.h declares it as
            //   `EXTERN_API(MCTableHandle) GetMCInfo(void)
            //    ONEWORDINLINE(0xAA61)`.
            //
            // Tool-bit Pascal FUNCTION ABI: parameterless; caller pre-
            // pushes a 4-byte MCTableHandle result slot; trap writes the
            // handle at [SP+0] without modifying A7; caller pops the slot
            // post-trap. Net A7 effect per C-level call sequence is zero.
            //
            // Absolute MCTableHandle behavior (BasiliskII source):
            // BII may return a non-NIL handle pointing into a system-
            // populated MC table. Systemless now returns a deep copy of the
            // live MenuCInfo table when one exists and NIL when no table
            // has been created yet. The NIL path remains the IM-documented
            // copy-failure return value.
            (true, 0x261) => {
                let sp = cpu.read_reg(Register::A7);
                let current_handle = bus.read_long(addr::MENU_C_INFO);
                let copy_handle = self.clone_menu_color_handle(bus, current_handle);
                bus.write_long(sp, copy_handle);
                Ok(())
            }

            // SetMCInfo ($AA62)
            // PROCEDURE SetMCInfo(menuCTbl: MCTableHandle);
            // Inside Macintosh Volume V, V-247
            //
            // Per IM:V 1986 p. V-247, SetMCInfo copies the given menu
            // color information table to the current menu color
            // information table after first disposing of the current
            // table. If the copy fails, MemErr contains the error code
            // and the current table is preserved. MPW Universal Headers
            // Menus.h declares it as
            //   `EXTERN_API(void) SetMCInfo(MCTableHandle menuCTbl)
            //    ONEWORDINLINE(0xAA62)`.
            //
            // Tool-bit Pascal PROCEDURE ABI: caller pre-pushes a 4-byte
            // MCTableHandle; trap pops 4 bytes; no FUNCTION result slot.
            // Stack layout at trap entry: SP+0: menuCTbl(4).
            //
            // Absolute behavior (BasiliskII source): BII mutates lowmem
            // MenuCInfo ($0D50). Systemless HLE now copies
            // the source table into the live MenuCInfo handle and leaves
            // the source handle alone, preserving the documented
            // "current table is preserved on failure" contract for a NIL
            // source. Per IM:V V-247, a NIL source triggers the
            // copy-failure path and the current table is preserved.
            (true, 0x262) => {
                let sp = cpu.read_reg(Register::A7);
                let source_handle = bus.read_long(sp);
                if source_handle != 0 {
                    let current_handle = self.ensure_menu_color_table_handle(bus);
                    if current_handle != 0 {
                        let copy_handle = self.clone_menu_color_handle(bus, source_handle);
                        if copy_handle != 0 {
                            let old_ptr = bus.read_long(current_handle);
                            if old_ptr != 0 {
                                bus.free(old_ptr);
                            }
                            bus.free(current_handle);
                            bus.write_long(addr::MENU_C_INFO, copy_handle);
                        }
                    }
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // DispMCInfo ($AA63)
            // PROCEDURE DispMCInfo(menuCTbl: MCTableHandle);
            // Inside Macintosh Volume V, V-248
            //
            // Per IM:V 1986 p. V-248, DispMCInfo disposes of the given
            // menu color information table. Modern MPW Universal Headers
            // Menus.h declares the trap as DisposeMCInfo with
            //   `#define DispMCInfo(menuCTbl) DisposeMCInfo(menuCTbl)`
            // aliasing.
            //
            // Tool-bit Pascal PROCEDURE ABI: caller pre-pushes a 4-byte
            // MCTableHandle; trap pops 4 bytes; no FUNCTION result slot.
            // Stack layout at trap entry: SP+0: menuCTbl(4).
            //
            // Absolute behavior (BasiliskII source): BII calls
            // DisposHandle on the caller-supplied handle. Systemless
            // HLE now does the same on the supplied handle while leaving
            // the current MenuCInfo table untouched. With a NIL handle
            // argument this is a no-op: DisposHandle on NIL is a
            // documented no-op on classic Mac.
            (true, 0x263) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                if handle != 0 {
                    let data_ptr = bus.read_long(handle);
                    if data_ptr != 0 {
                        bus.free(data_ptr);
                    }
                    bus.free(handle);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // GetMCEntry ($AA64)
            // FUNCTION GetMCEntry(menuID: INTEGER; menuItem: INTEGER): MCEntryPtr;
            // Inside Macintosh Volume V, V-248
            //
            // Per IM:V 1986 p. V-248, GetMCEntry finds the entry of the
            // specified menuID and menuItem in the menu color information
            // table and returns a pointer into the table. If the entry
            // is not found, a NIL pointer is returned. MPW Universal
            // Headers Menus.h declares it as
            //   `EXTERN_API(MCEntryPtr) GetMCEntry(MenuID menuID,
            //    short menuItem) ONEWORDINLINE(0xAA64)`.
            //
            // Tool-bit Pascal FUNCTION ABI: caller pre-pushes 4-byte
            // MCEntryPtr result slot + 4 bytes of args (2xINTEGER); trap
            // pops the 2xINTEGER args and writes MCEntryPtr at [SP+0]
            // (which is the result slot once the args are popped);
            // caller pops the 4-byte result slot post-trap. Net A7
            // effect per call sequence is zero. Stack layout at trap
            // entry:
            //   SP+0: menuItem(2), SP+2: menuID(2), SP+4: result(4)
            //
            // Absolute MCEntryPtr behavior (BasiliskII source):
            // BII may return a non-NIL pointer when (menuID, menuItem)
            // matches a system-populated entry. Systemless now returns a
            // pointer into the live MenuCInfo table when the exact pair
            // exists, and NIL when it does not.
            (true, 0x264) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_item = bus.read_word(sp) as i16;
                let menu_id = bus.read_word(sp + 2) as i16;
                let current_handle = bus.read_long(addr::MENU_C_INFO);
                let mut result = 0;
                if current_handle != 0 {
                    let current_ptr = bus.read_long(current_handle);
                    if current_ptr != 0 {
                        let current_size = bus.get_alloc_size(current_ptr).unwrap_or(0) as usize;
                        let mut offset = 0usize;
                        while offset + MC_ENTRY_SIZE <= current_size {
                            let entry_ptr = current_ptr + offset as u32;
                            if bus.read_word(entry_ptr) as i16 == menu_id
                                && bus.read_word(entry_ptr + 2) as i16 == menu_item
                            {
                                result = entry_ptr;
                                break;
                            }
                            offset += MC_ENTRY_SIZE;
                        }
                    }
                }
                bus.write_long(sp + 4, result);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetMCEntries ($AA65)
            // PROCEDURE SetMCEntries(numEntries: INTEGER; menuCEntries: MCTablePtr);
            // Inside Macintosh Volume V, V-248
            //
            // Per IM:V 1986 p. V-248, SetMCEntries takes a pointer to an
            // array of color information records and adds or updates the
            // entries in the menu color information table based on the
            // (menuID, menuItem) match. MPW Universal Headers Menus.h
            // declares it as
            //   `EXTERN_API(void) SetMCEntries(short numEntries,
            //    MCTablePtr menuCEntries) ONEWORDINLINE(0xAA65)`.
            //
            // Tool-bit Pascal PROCEDURE ABI: caller pre-pushes 1 INTEGER
            // (2 bytes) + 1 MCTablePtr (4 bytes) = 6 bytes; trap pops 6
            // bytes; no FUNCTION result slot. Stack layout at trap entry:
            //   SP+0: menuCEntries(4), SP+4: numEntries(2)
            //
            // Absolute behavior (BasiliskII source): BII iterates the
            // caller-supplied array and mutates lowmem
            // MenuCInfo ($0D50). Systemless HLE now updates the live table
            // with exact (menuID, menuItem) matches and appends new
            // entries when needed. With (numEntries=0, menuCEntries=NIL)
            // the zero-entry loop is skipped.
            (true, 0x265) => {
                let sp = cpu.read_reg(Register::A7);
                let num_entries = bus.read_word(sp + 4) as i16;
                let entries_ptr = bus.read_long(sp);
                if num_entries > 0 && entries_ptr != 0 {
                    let mut entries = Vec::with_capacity(num_entries as usize * MC_ENTRY_SIZE);
                    for index in 0..num_entries as usize {
                        let entry_ptr = entries_ptr + (index as u32 * MC_ENTRY_SIZE as u32);
                        entries.extend_from_slice(&bus.read_bytes(entry_ptr, MC_ENTRY_SIZE));
                    }
                    self.merge_menu_color_entries(bus, &entries);
                }
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // MenuChoice ($AA66)
            // FUNCTION MenuChoice: LongInt;
            // Inside Macintosh: Macintosh Toolbox Essentials (1992),
            //   p. 3-118 (Menu Manager — MenuChoice).
            //
            // MPW Universal Headers Menus.h:
            //   EXTERN_API(long) MenuChoice(void) ONEWORDINLINE(0xAA66);
            //
            // Tool-bit Pascal FUNCTION (bit 11 set) with no arguments
            // and a 4-byte LongInt function result. Caller pre-pushes
            // a 4-byte result slot, the trap writes the LongInt to
            // [SP+0] (without modifying A7), and the caller pops the
            // slot after the trap returns. The C-level call sequence
            // is net A7-zero.
            //
            // Per IM:MTb 1992 p. 3-118..3-119 MenuChoice surfaces the
            // menu ID + item number of the last disabled menu item the
            // user attempted to choose via MenuSelect or MenuKey: the
            // high-order word is the menu ID and the low-order word is
            // the item number. The Menu Manager stores that packed result
            // in lowmem global MenuDisable ($0B54); the trap simply reads
            // the current longword and returns it.
            //
            // Tool-bit Pascal FUNCTION calling convention: A7 unchanged
            // across the C-level call sequence (caller pre-push of 4-byte
            // result slot + trap-side result-slot write + caller post-pop
            // balance).
            //
            // Behavior:
            //   * Pascal FUNCTION calling convention: A7 unchanged
            //     across the C-level call (caller pre-push + trap
            //     result-slot write + caller post-pop balance), for
            //     both a single call and a repeated composition.
            //   * MenuChoice returns the live MenuDisable value unchanged.
            (true, 0x266) => {
                let sp = cpu.read_reg(Register::A7);
                let value = bus.read_long(addr::MENU_DISABLE);
                bus.write_long(sp, value);
                Ok(())
            }

            _ => return None,
        })
    }

    fn is_hierarchical_item(item: &MenuItem) -> bool {
        hierarchical_menu_id(item.key_equiv, item.mark).is_some()
    }

    fn menu_item_has_command_key(item: &MenuItem) -> bool {
        item.key_equiv > 0x20
    }

    fn menu_cicn_layout(bus: &MacMemoryBus, icon_ptr: u32) -> Option<SharedColorIconLayout> {
        SharedColorIconLayout::decode_with(
            |offset| {
                let offset = u32::try_from(offset).ok()?;
                Some(bus.read_byte(icon_ptr.checked_add(offset)?))
            },
            bus.get_alloc_size(icon_ptr).map(|size| size as usize),
        )
    }

    fn menu_item_cicn_size(&self, bus: &MacMemoryBus, item: &MenuItem) -> Option<(i16, i16)> {
        let icon_ptr = self.cicn_menu_icon_resource_ptr(item)?;
        let layout = Self::menu_cicn_layout(bus, icon_ptr)?;
        Some((layout.width, layout.height))
    }

    fn menu_item_icon_kind(&self, bus: &MacMemoryBus, item: &MenuItem) -> StandardMenuIconKind {
        standard_menu_icon_kind(
            item.icon,
            item.key_equiv,
            self.menu_item_cicn_size(bus, item),
        )
    }

    pub(super) fn menu_item_height(&self, bus: &MacMemoryBus, item: &MenuItem) -> i16 {
        if item.text == "-" {
            return STANDARD_MENU_SEPARATOR_HEIGHT;
        }
        // MTE 1992 p. 3-46: 'cicn' has priority over ICON/SICN and
        // enlarges the menu item according to the icon's resource rect.
        // Normal ICON items reserve a 32-by-32 slot; System 7.5.3's
        // standard MDEF uses a 34-pixel row around that slot. Reduced ICON
        // and SICN slots fit the standard 16-by-16 item height.
        self.menu_item_icon_kind(bus, item)
            .row_height(QuickDrawTextStyle::from_bits(item.style))
    }

    pub(super) fn menu_items_height(&self, bus: &MacMemoryBus, items: &[MenuItem]) -> i16 {
        self.menu_rows(bus, items).total_height()
    }

    pub(super) fn menu_rows(&self, bus: &MacMemoryBus, items: &[MenuItem]) -> SharedMenuRows {
        SharedMenuRows::new(
            Self::laid_out_items(items)
                .iter()
                .map(|item| SharedMenuRow {
                    height: self.menu_item_height(bus, item),
                    selectable: item.enabled && item.text != "-",
                }),
        )
    }

    /// Combine retained standard-MDEF row geometry with live MenuInfo
    /// selectability for one tracking slice. Guest code can mutate enable
    /// flags and item bytes while MenuSelect owns the interaction; missing or
    /// malformed live rows therefore remain laid out but unavailable.
    /// Macintosh Toolbox Essentials (1992), pp. 3-90--3-92 and 3-114--3-119.
    fn menu_tracking_rows(&self, bus: &MacMemoryBus, menu: &Menu) -> SharedMenuRows {
        let geometry = self.menu_rows(bus, &menu.items);
        let live = menu_items_from_memory(bus, menu.handle);
        let menu_enabled = live
            .as_ref()
            .is_some_and(|items| items.enable_flags & 1 != 0);
        SharedMenuRows::new((0..geometry.len()).map(|index| {
            let item_number = i16::try_from(index + 1).unwrap_or(i16::MAX);
            SharedMenuRow {
                height: geometry.height(item_number, 0),
                selectable: live
                    .as_ref()
                    .is_some_and(|items| items.item_is_selectable(item_number)),
            }
        }))
        .with_menu_enabled(menu_enabled)
    }

    /// The items a menu actually lays out.
    ///
    /// A divider only means anything between groups of items (HIG 1992
    /// p. 63), and the standard definition procedure gives a trailing one
    /// no row. Applications author the Apple menu as an About command plus
    /// a divider in their `'MENU'` resource so `AppendResMenu` has
    /// something to append below (MTE 1992 pp. 3-97 to 3-98); when the
    /// Apple Menu Items list comes back empty, System 7.5.3 under
    /// BasiliskII draws that menu exactly one item tall rather than
    /// leaving a dangling line.
    fn laid_out_items(items: &[MenuItem]) -> &[MenuItem] {
        let end = laid_out_menu_item_count(items, |item| item.text == "-");
        &items[..end]
    }

    fn menu_item_icon_width(&self, bus: &MacMemoryBus, item: &MenuItem) -> i16 {
        self.menu_item_icon_kind(bus, item).width()
    }

    pub(super) fn standard_menu_width(&self, bus: &MacMemoryBus, items: &[MenuItem]) -> i16 {
        shared_standard_menu_width(Self::laid_out_items(items).iter().map(|item| {
            StandardMenuItemWidth {
                text: shared_standard_menu_text_advance(&internal_menu_string_bytes(&item.text)),
                icon: self.menu_item_icon_width(bus, item),
                command: item.key_equiv,
            }
        }))
    }

    fn menu_item_icon_resource_id(item: &MenuItem) -> Option<i16> {
        standard_menu_icon_resource_id(item.icon, item.key_equiv)
    }

    /// Materialize a menu's item-icon resources (cicn/ICON/SICN at
    /// icon-number + 256, MTE 1992 pp. 3-46, 3-137..3-138) before the
    /// dropdown draws. The draw path reads icons through `&self` helpers
    /// deep in borrow chains that cannot load on demand, so lazily-seeded
    /// icon resources (IM-style empty handles) are faulted in here at the
    /// one mutable moment the flow guarantees -- without this, an
    /// application whose first icon touch happens at draw time shows
    /// empty slots for icons that are present in the fork.
    fn preload_menu_item_icon_resources(&mut self, bus: &mut MacMemoryBus, menu_idx: usize) {
        let icon_ids: Vec<i16> = self
            .menus
            .get(menu_idx)
            .map(|menu| {
                menu.items
                    .iter()
                    .filter_map(Self::menu_item_icon_resource_id)
                    .collect()
            })
            .unwrap_or_default();
        for icon_resource_id in icon_ids {
            for res_type in [*b"cicn", *b"ICON", *b"SICN"] {
                let _ = self.find_or_load_resource_any(bus, res_type, icon_resource_id);
            }
        }
    }

    fn cicn_menu_icon_resource_ptr(&self, item: &MenuItem) -> Option<u32> {
        let icon_resource_id = Self::menu_item_icon_resource_id(item)?;
        self.find_loaded_resource_any(*b"cicn", icon_resource_id)
            .map(|(_, ptr)| ptr)
    }

    fn reduced_menu_icon_resource_ptr(&self, bus: &MacMemoryBus, item: &MenuItem) -> Option<u32> {
        if self.menu_item_icon_kind(bus, item) != StandardMenuIconKind::Reduced {
            return None;
        }

        // IM:I I-359 says the item's icon byte is an icon number; MTE
        // 1992 pp. 3-137 to 3-138 specify the Menu Manager adds 256
        // to obtain the ICON resource ID for the reduced-icon case.
        let icon_resource_id = Self::menu_item_icon_resource_id(item)?;
        self.find_loaded_resource_any(*b"ICON", icon_resource_id)
            .map(|(_, ptr)| ptr)
    }

    fn small_menu_icon_resource_ptr(&self, bus: &MacMemoryBus, item: &MenuItem) -> Option<u32> {
        if self.menu_item_icon_kind(bus, item) != StandardMenuIconKind::Small {
            return None;
        }

        // MTE 1992 p. 3-46: when the key-equivalent byte is $1E and no
        // cicn is used, the Menu Manager looks for an SICN resource at
        // icon-number+256 and plots it in a 16-by-16 rectangle.
        let icon_resource_id = Self::menu_item_icon_resource_id(item)?;
        self.find_loaded_resource_any(*b"SICN", icon_resource_id)
            .map(|(_, ptr)| ptr)
    }

    fn normal_menu_icon_resource_ptr(&self, bus: &MacMemoryBus, item: &MenuItem) -> Option<u32> {
        if self.menu_item_icon_kind(bus, item) != StandardMenuIconKind::Normal {
            return None;
        }

        // MTE 1992 p. 3-46: a normal menu icon uses the icon number plus
        // 256 as an ICON or cicn resource ID; this path implements the
        // monochrome ICON case and leaves cicn-specific drawing open.
        let icon_resource_id = Self::menu_item_icon_resource_id(item)?;
        self.find_loaded_resource_any(*b"ICON", icon_resource_id)
            .map(|(_, ptr)| ptr)
    }

    fn draw_monochrome_menu_icon(
        &self,
        bus: &mut MacMemoryBus,
        icon_ptr: u32,
        top: i16,
        left: i16,
        kind: StandardMenuIconKind,
        pixel_index_override: Option<u8>,
    ) {
        let Some(layout) = SharedMonochromeMenuIconLayout::for_kind(
            kind,
            bus.get_alloc_size(icon_ptr).map(|size| size as usize),
        ) else {
            return;
        };
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();

        // IM:V 1986 p. V-233 and MTE 1992 p. 3-99: black-and-white
        // menu icons are drawn in the item name color. The caller passes
        // the resolved MenuCInfo name color when an 8bpp table entry exists.
        // The architecture-neutral sampler owns ICON reduction and SICN
        // first-image selection; this adapter owns only guest reads and
        // framebuffer writes.
        for dy in 0..layout.height {
            for dx in 0..layout.width {
                let set = layout
                    .sample_with(
                        |offset| {
                            let offset = u32::try_from(offset).ok()?;
                            Some(bus.read_byte(icon_ptr.checked_add(offset)?))
                        },
                        dx,
                        dy,
                    )
                    .unwrap_or(false);
                if set {
                    if let Some(pixel_index) = pixel_index_override {
                        Self::fb_set_pixel_index(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            left + i16::try_from(dx).unwrap_or(i16::MAX),
                            top + i16::try_from(dy).unwrap_or(i16::MAX),
                            pixel_index,
                        );
                    } else {
                        Self::fb_set_pixel(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            left + i16::try_from(dx).unwrap_or(i16::MAX),
                            top + i16::try_from(dy).unwrap_or(i16::MAX),
                            true,
                        );
                    }
                }
            }
        }
    }

    fn draw_cicn_menu_icon(&self, bus: &mut MacMemoryBus, icon_ptr: u32, top: i16, left: i16) {
        let Some(layout) = Self::menu_cicn_layout(bus, icon_ptr) else {
            return;
        };
        let (screen_base, row_bytes, screen_width, screen_height, screen_pixel_size) =
            self.get_screen_params();
        let screen = (
            screen_base,
            row_bytes,
            screen_pixel_size,
            screen_width,
            screen_height,
        );

        for dy in 0..layout.height {
            for dx in 0..layout.width {
                let (Ok(sx), Ok(sy)) = (usize::try_from(dx), usize::try_from(dy)) else {
                    continue;
                };
                let read = |offset: usize| {
                    let offset = u32::try_from(offset).ok()?;
                    Some(bus.read_byte(icon_ptr.checked_add(offset)?))
                };
                if !layout.mask_bit_with(read, sx, sy).unwrap_or(false) {
                    continue;
                }

                if matches!(screen_pixel_size, 2 | 4 | 8) {
                    let read = |offset: usize| {
                        let offset = u32::try_from(offset).ok()?;
                        Some(bus.read_byte(icon_ptr.checked_add(offset)?))
                    };
                    let Some(rgb) = layout.rgb_with(read, sx, sy) else {
                        continue;
                    };
                    // PlotCIcon remaps the source RGB color to the active
                    // screen device. More Macintosh Toolbox (1993), pp. 5-25
                    // to 5-26; Imaging With QuickDraw (1994), p. 4-106.
                    let destination_index = Self::fb_main_screen_pixel_index_for_rgb(bus, rgb)
                        .unwrap_or_else(|| {
                            super::pict::closest_clut_index(
                                rgb[0],
                                rgb[1],
                                rgb[2],
                                &self.device_clut,
                            )
                        });
                    let dst_x = left + dx;
                    let dst_y = top + dy;
                    Self::fb_set_pixel_index(
                        bus,
                        screen_base,
                        row_bytes,
                        screen_pixel_size,
                        screen_width,
                        screen_height,
                        dst_x,
                        dst_y,
                        destination_index,
                    );
                    continue;
                }

                let read = |offset: usize| {
                    let offset = u32::try_from(offset).ok()?;
                    Some(bus.read_byte(icon_ptr.checked_add(offset)?))
                };
                let Some(set) = layout.monochrome_bit_with(read, sx, sy) else {
                    continue;
                };
                Self::menu_set_standard_pixel(bus, screen, left + dx, top + dy, set);
            }
        }
    }

    fn popup_menu_dropdown_rect(
        &self,
        bus: &MacMemoryBus,
        menu_idx: usize,
        request: PopupMenuRequest,
    ) -> Option<((i16, i16, i16, i16), i16, i16)> {
        let (_screen_base, _row_bytes, screen_width, screen_height, _pixel_size) =
            self.get_screen_params();
        let menu = &self.menus[menu_idx];

        let width = self.standard_menu_width(bus, &menu.items);
        let rows = self.menu_rows(bus, &menu.items);
        let layout = standard_popup_menu_layout(
            &rows,
            width,
            (screen_width, screen_height),
            request.anchor,
            request.requested_item,
        )?;
        Some((layout.rect(), layout.highlighted_item, layout.content_top))
    }

    fn dropdown_width_for_menu(&self, bus: &MacMemoryBus, menu_idx: usize, min_width: i16) -> i16 {
        let Some(menu) = self.menus.get(menu_idx) else {
            return min_width;
        };
        min_width.max(self.standard_menu_width(bus, &menu.items))
    }

    /// Open a menu dropdown and start tracking.
    fn open_menu_dropdown(
        &mut self,
        bus: &mut MacMemoryBus,
        menu_idx: usize,
    ) -> bool {
        // Fault in this menu's icon resources while `self` is still
        // mutable; the draw path below reads them through `&self` only.
        self.preload_menu_item_icon_resources(bus, menu_idx);
        let (_screen_base, _row_bytes, screen_width, screen_height, _pixel_size) =
            self.get_screen_params();

        // Compute dropdown rect
        let region = self
            .current_menu_title_regions_with_indices(bus)
            .into_iter()
            .find(|(idx, _region)| *idx == menu_idx);
        if menu_idx >= self.menus.len() {
            return false;
        }
        let Some((_idx, title_region)) = region else {
            return false;
        };
        let menu = &self.menus[menu_idx];
        let menu_id = menu.id;
        let menu_ptr = bus.read_long(menu.handle);
        let custom_definition = menu_ptr != 0 && !self.menu_uses_standard_definition(bus, menu_ptr);

        let max_width = if custom_definition {
            bus.read_word(menu_ptr + 2) as i16
        } else {
            self.dropdown_width_for_menu(bus, menu_idx, title_region.right - title_region.left + 20)
        };
        let content_height = if custom_definition {
            bus.read_word(menu_ptr + 4) as i16
        } else {
            self.menu_items_height(bus, &menu.items)
        };
        let menu_bar_height = bus.read_word(addr::MBAR_HEIGHT) as i16;
        let Some(layout) = standard_pull_down_menu_layout(
            max_width,
            content_height,
            (screen_width, screen_height),
            title_region.left,
            menu_bar_height,
        ) else {
            return false;
        };
        let dropdown_rect = layout.rect();

        // TheMenu owns the retained title identity. Redrawing before the
        // pane opens restores any prior title and highlights this one.
        bus.write_word(addr::THE_MENU, menu_id as u16);
        self.draw_menu_bar_to_fb(bus);

        // Save pixels under dropdown
        let saved = self.save_dropdown_pixels(bus, dropdown_rect);

        if custom_definition {
            self.draw_menu_dropdown_chrome(bus, menu_idx, dropdown_rect);
        } else {
            self.draw_menu_dropdown(bus, menu_idx, dropdown_rect);
        }

        let mut tracking =
            tracked_menu_state(MenuTrackingKind::MenuBar, menu.handle, dropdown_rect, saved);
        tracking.definition = custom_definition
            .then(|| SharedMenuDefinitionTracking::begin_draw(menu.handle, dropdown_rect));
        self.menu_tracking.set(Some(tracking));
        if !custom_definition {
            let rows = self.menu_rows(bus, &self.menus[menu_idx].items);
            Self::write_menu_scrolling_globals(bus, &rows, dropdown_rect.0);
        }
        custom_definition
    }

    /// Finish a Pascal LONGINT menu call on the immediate no-hit path.
    fn finish_menu_no_hit(
        &self,
        bus: &mut MacMemoryBus,
        cpu: &mut dyn CpuOps,
        stack_ptr: u32,
        result_offset: u32,
    ) {
        bus.write_long(stack_ptr + result_offset, 0);
        cpu.write_reg(Register::A7, stack_ptr + result_offset);
    }

    fn restore_menu_tracking_pixels(&self, bus: &mut MacMemoryBus, saved: MenuTrackingState) {
        let root_rect = saved.dropdown_rect();
        for submenu in saved.submenus.into_iter().rev() {
            self.restore_dropdown_pixels(bus, submenu.dropdown_rect(), &submenu.saved_pixels);
        }
        self.restore_dropdown_pixels(bus, root_rect, &saved.saved_pixels);
    }

    fn menu_tracking_selection_result(&self, bus: &MacMemoryBus) -> u32 {
        let Some(tracking) = self.menu_tracking.as_ref() else {
            return 0;
        };
        tracking
            .selection(|menu_handle, item_number| {
                menu_items_from_memory(bus, menu_handle).is_some_and(|items| {
                    items.item_is_selectable(item_number)
                        && !items.item_is_hierarchical(item_number)
                })
            })
            .and_then(|(menu_handle, item_number)| {
                let menu_ptr = bus.read_long(menu_handle);
                (menu_ptr != 0).then(|| {
                    (u32::from(bus.read_word(menu_ptr)) << 16) | (u32::from(item_number as u16))
                })
            })
            .unwrap_or(0)
    }

    fn submenu_menu_index_for_parent_item(
        &self,
        bus: &MacMemoryBus,
        parent_menu_idx: usize,
        parent_item: i16,
    ) -> Option<usize> {
        if parent_item <= 0 {
            return None;
        }
        let parent_menu_handle = self.menus.get(parent_menu_idx)?.handle;
        let parent_items = menu_items_from_memory(bus, parent_menu_handle)?;
        let submenu_handle = self.current_menu_list(bus)?.submenu_handle_for_item(
            &parent_items,
            parent_item,
            |handle| {
                let menu_ptr = bus.read_long(handle);
                (menu_ptr != 0).then(|| bus.read_word(menu_ptr) as i16)
            },
        )?;
        self.menus
            .iter()
            .position(|menu| menu.handle == submenu_handle)
    }

    fn submenu_rect_for_parent_item(
        &self,
        bus: &MacMemoryBus,
        parent_menu_idx: usize,
        parent_rect: (i16, i16, i16, i16),
        submenu_idx: usize,
        parent_item: i16,
    ) -> Option<(i16, i16, i16, i16)> {
        let (_screen_base, _row_bytes, screen_width, screen_height, _pixel_size) =
            self.get_screen_params();
        let parent_menu = self.menus.get(parent_menu_idx)?;
        let parent_offset = self.menu_rows(bus, &parent_menu.items).offset(parent_item);
        let submenu = self.menus.get(submenu_idx)?;
        let submenu_ptr = bus.read_long(submenu.handle);
        let custom_definition =
            submenu_ptr != 0 && !self.menu_uses_standard_definition(bus, submenu_ptr);
        let (submenu_width, submenu_height) = if custom_definition {
            (
                bus.read_word(submenu_ptr + 2) as i16,
                bus.read_word(submenu_ptr + 4) as i16,
            )
        } else {
            (
                self.dropdown_width_for_menu(bus, submenu_idx, 1),
                self.menu_items_height(bus, &submenu.items),
            )
        };
        standard_submenu_layout(
            parent_rect,
            parent_offset,
            submenu_width,
            submenu_height,
            (screen_width, screen_height),
            bus.read_word(addr::MBAR_HEIGHT) as i16,
        )
        .map(|layout| layout.rect())
    }

    fn ensure_submenu_for_request(&mut self, bus: &mut MacMemoryBus, request: SubmenuRequest<u32>) {
        let Some(parent_menu_idx) = self.menu_index_for_handle(request.parent_handle) else {
            return;
        };
        let Some(parent_menu) = self.menus.get(parent_menu_idx) else {
            return;
        };
        let open_submenus_at_depth = self
            .menu_tracking
            .as_ref()
            .map_or(0, |tracking| tracking.submenus.len());
        let item_idx = (request.parent_item - 1) as usize;
        let item_has_hierarchical_id = parent_menu
            .items
            .get(item_idx)
            .and_then(|item| crate::menu_manager::hierarchical_menu_id(item.key_equiv, item.mark))
            .is_some();

        if !item_has_hierarchical_id && open_submenus_at_depth <= request.child_depth {
            return;
        }
        if item_has_hierarchical_id
            && self
                .menu_tracking
                .as_ref()
                .and_then(|t| t.submenus.get(request.child_depth))
                .is_some_and(|s| s.parent_item == request.parent_item)
        {
            return;
        }

        let resolved_child = if item_has_hierarchical_id {
            self.submenu_menu_index_for_parent_item(bus, parent_menu_idx, request.parent_item)
                .and_then(|submenu_idx| self.menus.get(submenu_idx).map(|menu| menu.handle))
        } else {
            None
        };
        let reconciliation = self
            .menu_tracking
            .with_tracking_mut(|tracking| tracking.reconcile_submenu(request, resolved_child));
        let (token, closed) = match reconciliation {
            Some(SubmenuReconciliation::Stale | SubmenuReconciliation::Keep) | None => return,
            Some(SubmenuReconciliation::Closed {
                panes_deepest_first,
            }) => {
                for submenu in panes_deepest_first {
                    self.restore_dropdown_pixels(
                        bus,
                        submenu.dropdown_rect(),
                        &submenu.saved_pixels,
                    );
                }
                return;
            }
            Some(SubmenuReconciliation::Open {
                token,
                panes_deepest_first,
            }) => (token, panes_deepest_first),
        };
        for submenu in closed {
            self.restore_dropdown_pixels(bus, submenu.dropdown_rect(), &submenu.saved_pixels);
        }
        let request = token.request();
        let submenu_handle = token.child_handle();

        let Some(parent_rect) =
            self.menu_tracking
                .as_ref()
                .and_then(|tracking| match request.parent {
                    MenuTrackingPane::Root => Some(tracking.dropdown_rect()),
                    MenuTrackingPane::Submenu(depth) => tracking
                        .submenus
                        .get(depth)
                        .map(|submenu| submenu.dropdown_rect()),
                })
        else {
            return;
        };
        let Some(parent_menu_idx) = self.menu_index_for_handle(request.parent_handle) else {
            return;
        };
        let Some(submenu_idx) = self.menu_index_for_handle(submenu_handle) else {
            return;
        };
        let Some(dropdown_rect) = self.submenu_rect_for_parent_item(
            bus,
            parent_menu_idx,
            parent_rect,
            submenu_idx,
            request.parent_item,
        ) else {
            return;
        };
        let saved_pixels = self.save_dropdown_pixels(bus, dropdown_rect);
        let submenu_ptr = bus.read_long(submenu_handle);
        let custom_definition =
            submenu_ptr != 0 && !self.menu_uses_standard_definition(bus, submenu_ptr);
        let mut submenu = tracked_submenu_state(
            submenu_handle,
            request.parent_item,
            dropdown_rect,
            saved_pixels,
        );
        submenu.definition = custom_definition
            .then(|| SharedMenuDefinitionTracking::begin_draw(submenu_handle, dropdown_rect));
        let installed = self
            .menu_tracking
            .with_tracking_mut(|tracking| tracking.install_submenu(token, submenu).is_ok())
            .unwrap_or(false);
        if !installed {
            return;
        }
        if custom_definition {
            self.draw_menu_dropdown_chrome(bus, submenu_idx, dropdown_rect);
        } else {
            self.draw_menu_dropdown(bus, submenu_idx, dropdown_rect);
        }
    }

    fn write_standard_menu_choice(
        &self,
        bus: &mut MacMemoryBus,
        menu_handle: u32,
        item_number: i16,
    ) {
        let menu_ptr = bus.read_long(menu_handle);
        if menu_ptr == 0 {
            return;
        }
        let menu_id = bus.read_word(menu_ptr) as i16;
        bus.write_long(addr::MENU_DISABLE, menu_choice_value(menu_id, item_number));
    }

    fn update_menu_tracking_for_point(
        &mut self,
        bus: &mut MacMemoryBus,
        mouse_x: i16,
        mouse_y: i16,
    ) {
        let point = (mouse_y, mouse_x);
        let Some(menu_handle) = self
            .menu_tracking
            .as_ref()
            .map(|tracking| tracking.menu_handle)
        else {
            return;
        };
        let Some(menu_idx) = self.menu_index_for_handle(menu_handle) else {
            return;
        };
        let Some(menu) = self.menus.get(menu_idx) else {
            return;
        };
        let root_rows = self.menu_tracking_rows(bus, menu);
        let submenu_rows = self
            .menu_tracking
            .as_ref()
            .map(|tracking| {
                tracking
                    .submenus
                    .iter()
                    .map(|submenu| {
                        let menu_idx = self.menu_index_for_handle(submenu.menu_handle)?;
                        let menu = self.menus.get(menu_idx)?;
                        Some(self.menu_tracking_rows(bus, menu))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let update = self
            .menu_tracking
            .with_tracking_mut(|tracking| {
                tracking.track_standard_pointer(&root_rows, &submenu_rows, point)
            })
            .flatten();
        let Some(update) = update else {
            return;
        };
        let submenu_request = update.submenu_request();
        self.write_standard_menu_choice(bus, update.menu_handle, update.pointer.menu_choice_item);
        for submenu in update.closed_panes_deepest_first {
            self.restore_dropdown_pixels(bus, submenu.dropdown_rect(), &submenu.saved_pixels);
        }
        let pane = match update.pane {
            MenuTrackingPane::Root => self
                .menu_tracking
                .as_ref()
                .map(|tracking| (menu_idx, tracking.dropdown_rect())),
            MenuTrackingPane::Submenu(depth) => self
                .menu_tracking
                .as_ref()
                .and_then(|tracking| tracking.submenus.get(depth))
                .and_then(|submenu| {
                    self.menu_index_for_handle(submenu.menu_handle)
                        .map(|menu_idx| (menu_idx, submenu.dropdown_rect()))
                }),
        };
        if update.previous_item != update.pointer.item {
            if let Some((menu_idx, rect)) = pane {
                self.draw_menu_dropdown(bus, menu_idx, rect);
            }
        }
        if let Some(request) = submenu_request {
            self.ensure_submenu_for_request(bus, request);
        }
        if update.pointer.scrolled {
            if let Some((menu_idx, rect)) = pane {
                self.draw_menu_dropdown(bus, menu_idx, rect);
            }
        }
        Self::write_menu_scrolling_bounds(bus, update.content_top, update.content_bottom);
    }

    fn write_menu_scrolling_bounds(bus: &mut MacMemoryBus, content_top: i16, content_bottom: i16) {
        bus.write_word(addr::TOP_MENU_ITEM, content_top as u16);
        bus.write_word(addr::AT_MENU_BOTTOM, content_bottom as u16);
    }

    fn write_menu_scrolling_globals(
        bus: &mut MacMemoryBus,
        rows: &SharedMenuRows,
        content_top: i16,
    ) {
        Self::write_menu_scrolling_bounds(
            bus,
            content_top,
            content_top.saturating_add(rows.total_height()),
        );
    }

    /// Read the live MenuList title geometry and map guest handles to this
    /// adapter's derived presentation-cache indices.
    pub(crate) fn current_menu_title_regions_with_indices(
        &self,
        bus: &MacMemoryBus,
    ) -> Vec<(usize, SharedMenuBarTitleRegion)> {
        self.current_menu_list(bus)
            .into_iter()
            .flat_map(|menu_list| menu_list.regular_title_regions())
            .filter_map(|region| {
                self.menus
                    .iter()
                    .position(|menu| menu.handle == region.handle)
                    .map(|index| (index, region))
            })
            .collect()
    }

    fn current_menu_title_hit_test(&self, bus: &MacMemoryBus, mouse_x: i16) -> Option<usize> {
        self.current_menu_title_regions_with_indices(bus)
            .into_iter()
            .find(|(_menu_idx, region)| region.contains_horizontal(mouse_x))
            .map(|(menu_idx, _region)| menu_idx)
    }

    /// Determine which menu title the x coordinate falls on.
    #[cfg(test)]
    pub(crate) fn menu_title_hit_test(&self, mouse_x: i16) -> Option<usize> {
        for (menu_idx, left, right) in self.menu_title_regions_with_indices() {
            if mouse_x >= left && mouse_x < right {
                return Some(menu_idx);
            }
        }
        None
    }

    /// Compute the (left, right) x-coordinate regions for each menu title.
    /// Regions are derived from the current menu list only (menus that have
    /// been inserted via InsertMenu). IM:I I-352 / I-354.
    #[cfg(test)]
    fn menu_title_regions(&self) -> Vec<(i16, i16)> {
        self.menu_title_regions_with_indices()
            .into_iter()
            .map(|(_, left, right)| (left, right))
            .collect()
    }

    /// Compute menu title regions and keep the source `self.menus` index for
    /// each region so hit testing/highlighting can address the underlying
    /// inserted menu record directly.
    #[cfg(test)]
    fn menu_title_regions_with_indices(&self) -> Vec<(usize, i16, i16)> {
        let mut regions = Vec::new();
        let mut left = STANDARD_MENU_BAR_FIRST_TITLE_LEFT;
        for (menu_idx, menu) in self.menus.iter().enumerate() {
            if !menu.visible_in_menu_bar {
                continue;
            }
            let width = Self::menu_title_advance(&menu.title);
            let right = left
                .saturating_add(width)
                .saturating_add(STANDARD_MENU_BAR_TITLE_SPACING);
            regions.push((menu_idx, left, right));
            left = right;
        }
        regions
    }

    /// Determine which item (1-based) is at the given screen point, or 0.
    #[cfg(test)]
    fn dropdown_item_at_point(&self, bus: &MacMemoryBus, mouse_x: i16, mouse_y: i16) -> i16 {
        if let Some(tracking) = self.menu_tracking.as_ref() {
            let Some(menu_idx) = self.menu_index_for_handle(tracking.menu_handle) else {
                return 0;
            };
            let menu = &self.menus[menu_idx];
            return self.menu_rows(bus, &menu.items).tracking_item_at_point(
                tracking.dropdown_rect(),
                tracking.content_top,
                (mouse_y, mouse_x),
            );
        }
        0
    }

    /// Draw the Menu Manager-owned structure and erase a menu's contents.
    ///
    /// The menu bar definition function performs this pass before a custom
    /// MDEF receives `mDrawMsg`; the MDEF draws only the menu items. Macintosh
    /// Toolbox Essentials (1992), pp. 3-90 and 3-148--3-150.
    fn draw_menu_dropdown_chrome(
        &self,
        bus: &mut MacMemoryBus,
        menu_idx: usize,
        rect: (i16, i16, i16, i16),
    ) -> bool {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (top, left, bottom, right) = rect;
        if menu_idx >= self.menus.len() {
            return false;
        }
        let menu = &self.menus[menu_idx];
        let dropdown_bg_index =
            Self::menu_dropdown_background_pixel_index(bus, menu.id, pixel_size);
        let detached_popup = self.menu_tracking.as_ref().is_some_and(|tracking| {
            tracking.menu_handle == menu.handle
                && tracking.dropdown_rect() == rect
                && !menu.visible_in_menu_bar
        });
        let attached_pulldown =
            !detached_popup && menu.in_menu_bar && top == bus.read_word(addr::MBAR_HEIGHT) as i16;

        if !self.draw_theme_menu_dropdown_chrome(bus, top, left, bottom, right) {
            // Standard pull-down menu chrome is white, framed, and carries
            // the classic one-pixel drop shadow. Macintosh Toolbox
            // Essentials 1992, glossary "menu" and "menu bar".
            if let Some(bg_index) = dropdown_bg_index {
                Self::fb_fill_rect_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    top,
                    left,
                    bottom,
                    right,
                    bg_index,
                );
            } else {
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    top,
                    left,
                    bottom,
                    right,
                    false,
                );
            }

            let kind = if attached_pulldown {
                StandardMenuPaneKind::PullDown
            } else if detached_popup {
                StandardMenuPaneKind::PopUp
            } else {
                StandardMenuPaneKind::Hierarchical
            };
            let black_pixel = Self::menu_standard_pixel_index(bus, pixel_size, true);
            if let Some(chrome) = StandardMenuChrome::new(kind, rect) {
                chrome.for_each_frame_pixel(|x, y| {
                    if let Some(pixel_index) = black_pixel {
                        Self::fb_set_pixel_index(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            x,
                            y,
                            pixel_index,
                        );
                    } else {
                        Self::fb_set_pixel(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            x,
                            y,
                            true,
                        );
                    }
                });
                chrome.for_each_shadow_pixel(|x, y| {
                    if let Some(pixel_index) = black_pixel {
                        Self::fb_set_pixel_index(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            x,
                            y,
                            pixel_index,
                        );
                    } else {
                        Self::fb_set_pixel(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            x,
                            y,
                            true,
                        );
                    }
                });
            }
        }

        attached_pulldown
    }

    /// Draw the menu dropdown box with standard MDEF items.
    pub(super) fn draw_menu_dropdown(
        &self,
        bus: &mut MacMemoryBus,
        menu_idx: usize,
        rect: (i16, i16, i16, i16),
    ) {
        let attached_pulldown = self.draw_menu_dropdown_chrome(bus, menu_idx, rect);
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (top, left, bottom, right) = rect;
        if menu_idx >= self.menus.len() {
            return;
        }
        let menu = &self.menus[menu_idx];
        let standard_black = Self::menu_standard_pixel_index(bus, pixel_size, true);
        let standard_white = Self::menu_standard_pixel_index(bus, pixel_size, false);
        let color_table_bytes = Self::live_menu_color_table_bytes(bus);
        let menu_color_table = MenuColorTable::new(&color_table_bytes);
        let dropdown_bg_index = if matches!(pixel_size, 2 | 4 | 8) {
            if color_table_bytes.is_empty() {
                standard_white
            } else {
                Self::menu_color_rgb_pixel_index(bus, menu_color_table.dropdown_background(menu.id))
            }
        } else {
            None
        };

        // Draw items
        let font_id: i16 = 0;
        let font_size: i16 = 12;
        let metrics = crate::quickdraw::text::get_font_metrics(font_id, font_size);
        let (highlighted_item, content_top) = self
            .menu_tracking
            .as_ref()
            .and_then(|tracking| {
                if tracking.menu_handle == menu.handle && tracking.dropdown_rect() == rect {
                    Some((tracking.highlighted_item, tracking.content_top))
                } else {
                    tracking
                        .submenus
                        .iter()
                        .find(|submenu| {
                            submenu.menu_handle == menu.handle && submenu.dropdown_rect() == rect
                        })
                        .map(|submenu| (submenu.highlighted_item, submenu.content_top))
                }
            })
            .or_else(|| {
                self.control_tracking
                    .as_ref()
                    .filter(|tracking| {
                        tracking.active_menu == menu_idx && tracking.dropdown_rect == rect
                    })
                    .map(|tracking| (tracking.highlighted_item, tracking.popup_content_top))
            })
            .or_else(|| {
                self.dialog_tracking
                    .as_ref()
                    .and_then(|tracking| tracking.active_popup.as_ref())
                    .filter(|popup| popup.active_menu == menu_idx && popup.dropdown_rect == rect)
                    .map(|popup| (popup.highlighted_item, top))
            })
            .unwrap_or((0, top));
        let rows = self.menu_rows(bus, &menu.items);
        let (scroll_up, scroll_down) = rows.scroll_indicators(rect, content_top);
        let visible_item_top = top.saturating_add(if scroll_up { MENU_ROW_HEIGHT } else { 0 });
        let visible_item_bottom =
            bottom.saturating_sub(if scroll_down { MENU_ROW_HEIGHT } else { 0 });

        let mut item_top = content_top;
        for (i, item) in Self::laid_out_items(&menu.items).iter().enumerate() {
            let item_no = i as i16 + 1;
            let item_height = self.menu_item_height(bus, item);
            let item_bottom = item_top.saturating_add(item_height);
            if item_bottom <= top {
                item_top = item_bottom;
                continue;
            }
            if item_top >= bottom {
                break;
            }
            if item_top < visible_item_top || item_bottom > visible_item_bottom {
                item_top = item_bottom;
                continue;
            }
            let is_separator = item.text == "-";
            let row_enabled = menu.enabled && item.enabled;
            let (mark_pixel_index, name_pixel_index, command_pixel_index, item_bg_pixel_index) =
                if matches!(pixel_size, 2 | 4 | 8) {
                    if color_table_bytes.is_empty() {
                        (standard_black, standard_black, standard_black, standard_white)
                    } else {
                        let colors = menu_color_table.item_colors(menu.id, item_no);
                        (
                            Self::menu_color_rgb_pixel_index(bus, colors.mark),
                            Self::menu_color_rgb_pixel_index(bus, colors.name),
                            Self::menu_color_rgb_pixel_index(bus, colors.command),
                            Self::menu_color_rgb_pixel_index(bus, colors.background),
                        )
                    }
                } else {
                    (None, None, None, None)
                };
            let classic_selected = self.ui_theme_id() == UiThemeId::ClassicSystem7
                && highlighted_item == item_no
                && row_enabled
                && !is_separator;
            let selected_mono = classic_selected && pixel_size == 1;
            let selected_colors =
                (classic_selected && matches!(pixel_size, 2 | 4 | 8)).then(|| {
                    // IM:V 1986 pp. V-233 and V-249: the standard color MDEF
                    // redraws a selected row with its item/name color (RGB2, or
                    // the default item color) as the background and its menu
                    // background (RGB4) as the foreground for every component.
                    let selected_background = name_pixel_index
                        .or(standard_black)
                        .unwrap_or(255);
                    let selected_foreground = item_bg_pixel_index
                        .or(standard_white)
                        .unwrap_or(0);
                    (selected_background, selected_foreground)
                });
            if let Some((selected_background, _)) = selected_colors {
                Self::fb_fill_rect_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    item_top,
                    left + 1,
                    item_bottom,
                    right - 1,
                    selected_background,
                );
            }
            let icon_kind = self.menu_item_icon_kind(bus, item);
            let cicn_icon_ptr = matches!(icon_kind, StandardMenuIconKind::Color { .. })
                .then(|| self.cicn_menu_icon_resource_ptr(item))
                .flatten();
            let reduced_icon_ptr = self.reduced_menu_icon_resource_ptr(bus, item);
            let small_icon_ptr = self.small_menu_icon_resource_ptr(bus, item);
            let normal_icon_ptr = self.normal_menu_icon_resource_ptr(bus, item);
            let has_app_icon_resource = cicn_icon_ptr.is_some()
                || reduced_icon_ptr.is_some()
                || small_icon_ptr.is_some()
                || normal_icon_ptr.is_some();
            let has_command_key = Self::menu_item_has_command_key(item);
            // MTE 1992, 3-12: standard menu items can carry an icon,
            // mark, command-key equivalent, text style, and dimmed state.
            // Keep provider row paint inside the Menu Manager-owned frame so
            // the first and final rows cannot erase its horizontal borders
            // (Macintosh Toolbox Essentials 1992, pp. 3-90 and 3-148--3-150).
            let provider_row_chrome = self.draw_theme_menu_item_chrome(
                bus,
                item_top.max(top.saturating_add(1)),
                left + 1,
                item_bottom.min(bottom.saturating_sub(1)),
                right - 1,
                row_enabled,
                highlighted_item == i as i16 + 1,
                is_separator,
                item.icon != 0 && !has_app_icon_resource,
                item.mark != 0,
                has_command_key,
            );
            let provider_selected_foreground = (provider_row_chrome
                && highlighted_item == item_no
                && row_enabled
                && !is_separator)
                .then(|| self.theme_pixel_index(bus, self.ui_theme().palette().window_background));
            let layout = standard_menu_item_layout(
                (left, right),
                (item_top, item_height),
                icon_kind,
                item.mark != 0,
                (metrics.ascent, metrics.descent),
                attached_pulldown,
            );
            let text_y = layout.text_baseline;
            // MTE 1992 pp. 3-6--3-7 and p. 3-131: disabling a whole menu or
            // one item dims its rows and removes them from MenuSelect and
            // MenuKey. HIG 1992 p. 54 says unavailable items stay visible.
            // Separator rows are dimmed the same way because IM:I I-353 keeps
            // hyphen items disabled. On a colour screen the definition
            // procedure resolves the dim shade through GetGray (IM:V 1986
            // p. V-142); where the device has no intermediate shade it knocks
            // the drawn glyphs back with the 50% grey pattern instead.
            let dim_row = !row_enabled || is_separator;
            let dim_index = if dim_row {
                Self::menu_dim_pixel_index(bus, pixel_size, name_pixel_index, dropdown_bg_index)
            } else {
                None
            };
            let dim_with_pattern = dim_row && dim_index.is_none();
            let content_index = |component_index: Option<u8>| {
                if let Some(selected_foreground) = provider_selected_foreground {
                    Some(selected_foreground)
                } else if let Some((_, selected_foreground)) = selected_colors {
                    Some(selected_foreground)
                } else if dim_row {
                    dim_index.or(component_index)
                } else {
                    component_index
                }
            };

            if is_separator {
                if provider_row_chrome {
                    item_top = item_bottom;
                    continue;
                }
                // Separator: a dividing line across the item row, one pixel
                // above the row's midpoint in the System 7.5.3 standard MDEF.
                // Inside Macintosh Volume I, I-359
                let sep_y = layout.separator_y;
                for x in (left + 1)..(right - 1) {
                    match dim_index {
                        Some(pixel_index) => Self::fb_set_pixel_index(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            x,
                            sep_y,
                            pixel_index,
                        ),
                        // 50% grey pattern: set pixels where the pattern bit
                        // is on. Imaging With QuickDraw 1994 pp. 3-5--3-6.
                        None => {
                            if standard_menu_gray_pattern_is_ink(x, sep_y) {
                                if let Some(pixel_index) = name_pixel_index {
                                    Self::fb_set_pixel_index(
                                        bus,
                                        screen_base,
                                        row_bytes,
                                        pixel_size,
                                        screen_width,
                                        screen_height,
                                        x,
                                        sep_y,
                                        pixel_index,
                                    );
                                } else {
                                    Self::fb_set_pixel(
                                        bus,
                                        screen_base,
                                        row_bytes,
                                        pixel_size,
                                        screen_width,
                                        screen_height,
                                        x,
                                        sep_y,
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }
                item_top = item_bottom;
                continue;
            }

            let is_hierarchical = Self::is_hierarchical_item(item);

            // Draw mark character if present (0x12 = checkmark, others rendered as-is).
            // Inside Macintosh Volume I, I-358
            let text_left = layout.text_left;
            if item.mark != 0 && !is_hierarchical {
                // Map Mac Roman mark byte to a renderable string.
                // Mac character 0x12 (18) is the standard checkmark in Chicago.
                let mark_str: std::borrow::Cow<str> = if item.mark == 0x12 {
                    "\u{2713}".into() // ✓
                } else {
                    let s = String::from(item.mark as char);
                    s.into()
                };
                if let Some(pixel_index) = content_index(mark_pixel_index) {
                    Self::fb_draw_string_styled_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        layout.mark_left,
                        text_y,
                        &mark_str,
                        font_id,
                        font_size,
                        0,
                        pixel_index,
                    );
                } else {
                    Self::fb_draw_string(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        layout.mark_left,
                        text_y,
                        &mark_str,
                        font_id,
                        font_size,
                    );
                }
            }

            if let Some(icon_ptr) = cicn_icon_ptr {
                self.draw_cicn_menu_icon(bus, icon_ptr, item_top, layout.icon_left);
            } else if let Some(icon_ptr) = normal_icon_ptr {
                self.draw_monochrome_menu_icon(
                    bus,
                    icon_ptr,
                    item_top,
                    layout.icon_left,
                    StandardMenuIconKind::Normal,
                    content_index(name_pixel_index),
                );
            } else if let Some(icon_ptr) = reduced_icon_ptr {
                self.draw_monochrome_menu_icon(
                    bus,
                    icon_ptr,
                    item_top,
                    layout.icon_left,
                    StandardMenuIconKind::Reduced,
                    content_index(name_pixel_index),
                );
            } else if let Some(icon_ptr) = small_icon_ptr {
                self.draw_monochrome_menu_icon(
                    bus,
                    icon_ptr,
                    item_top,
                    layout.icon_left,
                    StandardMenuIconKind::Small,
                    content_index(name_pixel_index),
                );
            }

            // MTE 1992 pp. 3-60 and 3-133 to 3-134: `SetItemStyle`
            // changes a menu item's font style. Keep placement and
            // measurement on the existing classic metrics path, and let
            // the framebuffer text renderer apply only the visible style
            // pixels for the app-owned item text.
            if let Some(pixel_index) = content_index(name_pixel_index) {
                Self::fb_draw_string_styled_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    text_left,
                    text_y,
                    &item.text,
                    font_id,
                    font_size,
                    item.style,
                    pixel_index,
                );
            } else {
                Self::fb_draw_string_styled(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    text_left,
                    text_y,
                    &item.text,
                    font_id,
                    font_size,
                    item.style,
                );
            }

            if is_hierarchical {
                // IM:V V-23 / V-236: hierarchical items show a right-pointing
                // indicator; their mark byte is the submenu ID, not a checkmark.
                let indicator_color = content_index(command_pixel_index);
                for_each_standard_hierarchy_indicator_pixel(
                    layout.indicator_left,
                    layout.indicator_mid_y,
                    |x, y| match indicator_color {
                        Some(pixel_index) => Self::fb_set_pixel_index(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            x,
                            y,
                            pixel_index,
                        ),
                        None => Self::fb_set_pixel(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            x,
                            y,
                            true,
                        ),
                    },
                );
            }

            // MTE 1992 pp. 3-12 and 3-16 define marks and Command-key
            // equivalents as application-owned menu item characteristics.
            // Theme providers draw row chrome without replacing those
            // semantic indicators.
            if has_command_key {
                let cmd_str = format!("\u{2318}{}", item.key_equiv as char);
                // The standard MDEF places single-character command-key
                // equivalents in a fixed right-side column instead of
                // right-aligning each glyph pair by measured width. This
                // keeps N/O/W equivalents aligned in the System 7.5.3
                // MenuSelect reference. MTE 1992 pp. 3-115 to 3-117.
                let command_left = layout.command_left;
                if let Some(pixel_index) = content_index(command_pixel_index) {
                    Self::fb_draw_string_styled_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        command_left,
                        text_y,
                        &cmd_str,
                        font_id,
                        font_size,
                        0,
                        pixel_index,
                    );
                } else {
                    Self::fb_draw_string_styled(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        command_left,
                        text_y,
                        &cmd_str,
                        font_id,
                        font_size,
                        0,
                    );
                }
            }

            // Devices with no intermediate shade dim by knocking the drawn
            // glyphs back with the 50% grey pattern, the standard MDEF's
            // fallback when GetGray reports it cannot grey the content.
            // The pattern is `$AA $55 …` aligned to the port origin, so its
            // bits are on where x + y is even and the glyph keeps only those
            // pixels. IM:V 1986 p. V-142; Imaging With QuickDraw 1994
            // pp. 3-5--3-6.
            if dim_with_pattern && !provider_row_chrome {
                for y in item_top..item_bottom {
                    for x in (left + 1)..(right - 1) {
                        if !standard_menu_gray_pattern_is_ink(x, y) {
                            if let Some(bg_index) = dropdown_bg_index {
                                Self::fb_set_pixel_index(
                                    bus,
                                    screen_base,
                                    row_bytes,
                                    pixel_size,
                                    screen_width,
                                    screen_height,
                                    x,
                                    y,
                                    bg_index,
                                );
                            } else {
                                Self::fb_set_pixel(
                                    bus,
                                    screen_base,
                                    row_bytes,
                                    pixel_size,
                                    screen_width,
                                    screen_height,
                                    x,
                                    y,
                                    false,
                                );
                            }
                        }
                    }
                }
            }

            if selected_mono {
                // The monochrome standard MDEF highlights by inverting the
                // fully rendered row. Doing this after icons and text keeps
                // cicn bitmap fallbacks and every other 1-bit component
                // byte-for-byte identical to the classic XOR path.
                for y in item_top..item_bottom {
                    for x in (left + 1)..(right - 1) {
                        if x >= 0 && x < screen_width && y >= 0 && y < screen_height {
                            let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
                            let bit = 7 - (x as u32 % 8);
                            let addr = screen_base + byte_offset;
                            bus.write_byte(addr, bus.read_byte(addr) ^ (1 << bit));
                        }
                    }
                }
            }

            item_top = item_bottom;
        }

        // The standard MDEF replaces the first or last visible item position
        // with a triangular scrolling indicator when content exists beyond
        // that edge. Inside Macintosh Volume V (1986), pp. V-248--V-249.
        let center_x = left.saturating_add(right.saturating_sub(left) / 2);
        if scroll_up {
            for_each_standard_scroll_up_indicator_pixel(center_x, top, |x, y| {
                if let Some(pixel_index) = standard_black {
                    Self::fb_set_pixel_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        pixel_index,
                    );
                } else {
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        true,
                    );
                }
            });
        }
        if scroll_down {
            for_each_standard_scroll_down_indicator_pixel(center_x, bottom, |x, y| {
                if let Some(pixel_index) = standard_black {
                    Self::fb_set_pixel_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        pixel_index,
                    );
                } else {
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        true,
                    );
                }
            });
        }
    }

    /// Save the framebuffer pixels under a rectangle (including the 1px shadow).
    ///
    /// Guards off-screen y (y < 0 or y >= screen_h) from the same
    /// sign-extend multiply-overflow hazard guarded by save_dialog_pixels
    /// and save_rect_pixels.
    pub(super) fn save_dropdown_pixels(
        &self,
        bus: &MacMemoryBus,
        rect: (i16, i16, i16, i16),
    ) -> SavedPixels {
        let (screen_base, row_bytes, _, screen_h, pixel_size) = self.get_screen_params();
        let (top, left, bottom, right) = rect;
        // Include shadow area (+1 right, +1 bottom)
        let save_bottom = bottom + 1;
        let save_right = right + 1;
        let mut saved: SavedPixels = Default::default();
        let screen_h_i16 = screen_h;
        for y in top..save_bottom {
            if y < 0 || y >= screen_h_i16 {
                continue;
            }
            let row_start = screen_base + (y as u32) * row_bytes;
            let (byte_left, byte_right) = match pixel_size {
                bits @ (1 | 2 | 4) => {
                    // Packed indexed pixels occupy each byte from its high
                    // field downward. Save whole boundary bytes so rectangles
                    // with non-byte-aligned edges round-trip their neighbours.
                    // Imaging With QuickDraw (1994), pp. 4-10--4-11.
                    let pixels_per_byte = 8 / u32::from(bits);
                    (
                        (left.max(0) as u32) / pixels_per_byte,
                        (save_right.max(0) as u32).div_ceil(pixels_per_byte),
                    )
                }
                _ => {
                    // 8bpp: each pixel is one byte
                    (left.max(0) as u32, save_right.max(0) as u32)
                }
            };
            let bx_end = byte_right.min(row_bytes);
            for bx in byte_left..bx_end {
                saved.push(bus.read_byte(row_start + bx));
            }
        }
        if pixel_size == 8 {
            let mut offset = 0;
            for y in top.max(0)..save_bottom.min(screen_h) {
                let left = left.max(0) as u32;
                let len = (save_right.max(0) as u32)
                    .min(row_bytes)
                    .saturating_sub(left) as usize;
                bus.capture_pixel_detail(
                    &mut saved,
                    offset,
                    screen_base + y as u32 * row_bytes + left,
                    len,
                );
                offset += len;
            }
        }
        saved
    }

    /// Restore previously saved framebuffer pixels.
    /// Mirrors save_dropdown_pixels off-screen guard.
    pub(super) fn restore_dropdown_pixels<Pixel: Copy + Into<u16>>(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        saved: &SavedPixels<Pixel>,
    ) {
        let (screen_base, row_bytes, _, screen_h, pixel_size) = self.get_screen_params();
        let (top, left, bottom, right) = rect;
        let save_bottom = bottom + 1;
        let save_right = right + 1;
        let (byte_left, byte_right) = match pixel_size {
            bits @ (1 | 2 | 4) => {
                let pixels_per_byte = 8 / u32::from(bits);
                (
                    (left.max(0) as u32) / pixels_per_byte,
                    (save_right.max(0) as u32).div_ceil(pixels_per_byte),
                )
            }
            _ => (left.max(0) as u32, save_right.max(0) as u32),
        };
        let bx_end = byte_right.min(row_bytes);
        let bytes_per_row = bx_end.saturating_sub(byte_left);
        let mut idx = 0;
        let screen_h_i16 = screen_h;
        for y in top..save_bottom {
            if y < 0 || y >= screen_h_i16 {
                continue;
            }
            let row_start = screen_base + (y as u32) * row_bytes;
            for bx in byte_left..(byte_left + bytes_per_row) {
                if idx < saved.len() {
                    bus.restore_saved_pixels(row_start + bx, saved, idx, 1);
                    idx += 1;
                }
            }
        }
    }

    fn invert_menu_bar_rect(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        hilite_indexes: Option<(u8, u8)>,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let top = top.max(0).min(screen_height);
        let left = left.max(0).min(screen_width);
        let bottom = bottom.max(0).min(screen_height);
        let right = right.max(0).min(screen_width);
        if top >= bottom || left >= right {
            return;
        }

        for y in top..bottom {
            for x in left..right {
                if pixel_size == 1 {
                    let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
                    let bit = 7 - (x as u32 % 8);
                    let addr = screen_base + byte_offset;
                    let b = bus.read_byte(addr);
                    bus.write_byte(addr, b ^ (1 << bit));
                } else if matches!(pixel_size, 2 | 4) {
                    let indexes = hilite_indexes.unwrap_or((0, 15));
                    Self::hilite_packed_menu_pixel(
                        bus,
                        (screen_base, row_bytes, screen_width, screen_height),
                        pixel_size,
                        x,
                        y,
                        (
                            indexes.0 & ((1u16 << pixel_size) - 1) as u8,
                            indexes.1 & ((1u16 << pixel_size) - 1) as u8,
                        ),
                    );
                } else if pixel_size == 8 {
                    let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                    let (background, foreground) = hilite_indexes.unwrap_or((0, 255));
                    bus.map_screen_byte(addr, |index| {
                        Self::menu_hilited_pixel_index(index, background, foreground)
                    });
                }
            }
        }
    }

    fn invert_menu_title_index(&self, bus: &mut MacMemoryBus, menu_idx: usize) -> bool {
        let Some((_idx, region)) = self
            .current_menu_title_regions_with_indices(bus)
            .into_iter()
            .find(|(idx, _region)| *idx == menu_idx)
        else {
            return false;
        };
        let Some(menu) = self.menus.get(menu_idx) else {
            return false;
        };
        let (_screen_base, _row_bytes, _screen_width, _screen_height, pixel_size) =
            self.get_screen_params();
        let hilite_indexes = Self::menu_hilite_pixel_indexes(
            bus,
            self.menu_title_background_pixel_index(bus, menu.id, pixel_size),
            Self::menu_title_pixel_index(bus, menu.id, pixel_size),
            pixel_size,
        );
        let menu_bar_height = bus.read_word(addr::MBAR_HEIGHT) as i16;
        let (top, left, bottom, right) = region.highlighted_rect(menu_bar_height);
        self.invert_menu_bar_rect(bus, top, left, bottom, right, hilite_indexes);
        self.redraw_color_system_menu_mark_title(bus, menu_idx, region.title_origin());
        true
    }

    fn flash_menu_bar(&self, bus: &mut MacMemoryBus, menu_id: i16) {
        if self.fullscreen_locked || self.menu_bar_hidden {
            return;
        }
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let menu_bar_height = bus.read_word(addr::MBAR_HEIGHT) as i16;
        if menu_bar_height <= 0 {
            return;
        }
        if !matches!(pixel_size, 1 | 2 | 4 | 8) {
            return;
        }

        if menu_id != 0 {
            if let Some((idx, _region)) = self
                .current_menu_title_regions_with_indices(bus)
                .into_iter()
                .find(|(idx, _region)| self.menus.get(*idx).is_some_and(|menu| menu.id == menu_id))
            {
                let current = self.current_menu_bar_highlight_index(bus);
                if let Some(previous_idx) = current.filter(|previous_idx| *previous_idx != idx) {
                    self.invert_menu_title_index(bus, previous_idx);
                }
                self.invert_menu_title_index(bus, idx);
                let target_menu_id = if current == Some(idx) { 0 } else { menu_id };
                bus.write_word(addr::THE_MENU, target_menu_id as u16);
                return;
            }
        }

        let hilite_indexes = Self::menu_hilite_pixel_indexes(
            bus,
            self.menu_bar_background_pixel_index(bus, pixel_size),
            Self::menu_title_pixel_index(bus, 0, pixel_size),
            pixel_size,
        );
        self.invert_menu_bar_rect(bus, 0, 0, menu_bar_height, screen_width, hilite_indexes);
        // DrawMenuBar stamps the classic top screen-corner mask (IM:I
        // I-354). System 7.5.3 FlashMenuBar(0) preserves that black mask
        // while inverting the menu-bar strip; dialog_visual_flash_menubar_smoke
        // pins the exact pixels.
        Self::fb_draw_menu_bar_rounded_corners(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
        );
    }

    fn set_menu_tracking_highlight(&mut self, bus: &mut MacMemoryBus, item: i16) {
        let Some(old_item) = self
            .menu_tracking
            .as_ref()
            .map(|tracking| tracking.highlighted_item)
        else {
            return;
        };
        if old_item == item {
            return;
        }

        let Some((active_menu_handle, dropdown_rect)) =
            self.menu_tracking.with_tracking_mut(|tracking| {
                tracking.highlighted_item = item;
                (tracking.menu_handle, tracking.dropdown_rect())
            })
        else {
            return;
        };
        let Some(active_menu) = self.menu_index_for_handle(active_menu_handle) else {
            return;
        };
        self.draw_menu_dropdown(bus, active_menu, dropdown_rect);
    }

    /// Highlight a menu title in the menu bar.
    pub(super) fn highlight_menu_title(&self, bus: &mut MacMemoryBus, menu_idx: usize) {
        // MenuKey still resolves keyboard commands while menu chrome is
        // suppressed, but its transient title highlight must remain hidden.
        // This matches the effective-visibility guards used by DrawMenuBar
        // and FlashMenuBar.
        if self.fullscreen_locked || self.menu_bar_hidden {
            return;
        }
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let mut target_region = None;
        for (idx, region) in self.current_menu_title_regions_with_indices(bus) {
            if idx == menu_idx {
                target_region = Some(region);
                break;
            }
        }
        let Some(region) = target_region else {
            return;
        };
        let menu_bar_height = bus.read_word(addr::MBAR_HEIGHT) as i16;
        if menu_bar_height <= 1 {
            return;
        }
        if self.ui_theme_id() != UiThemeId::ClassicSystem7 {
            let Some(menu) = self
                .menus
                .get(menu_idx)
                .filter(|menu| menu.visible_in_menu_bar)
            else {
                return;
            };
            // HIG 1992 p. 55 says the title remains highlighted while
            // its menu is open. Non-classic themes own that title-state
            // chrome; the compatibility path redraws only the app title text.
            if self.draw_theme_menu_title_chrome(
                bus,
                1,
                region.left,
                menu_bar_height - 1,
                region.right,
                menu.enabled,
                true,
            ) {
                let font_id: i16 = 0;
                let font_size: i16 = 12;
                let metrics = crate::quickdraw::text::get_font_metrics(font_id, font_size);
                let text_height = metrics.ascent + metrics.descent;
                let text_y = (menu_bar_height - text_height) / 2 + metrics.ascent;
                Self::fb_draw_string_styled_ink(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    region.title_origin(),
                    text_y,
                    &menu.title,
                    font_id,
                    font_size,
                    0,
                    false,
                );
            }
            return;
        }
        let Some(menu) = self
            .menus
            .get(menu_idx)
            .filter(|menu| menu.visible_in_menu_bar)
        else {
            return;
        };
        let hilite_indexes = Self::menu_hilite_pixel_indexes(
            bus,
            self.menu_title_background_pixel_index(bus, menu.id, pixel_size),
            Self::menu_title_pixel_index(bus, menu.id, pixel_size),
            pixel_size,
        );
        // Invert the title area in the menu bar. The standard MBDF's
        // highlighted title rectangle begins two pixels before the logical
        // hit region, matching the pull-down rectangle captured by the
        // System 7.5.3 MenuSelect reference. Inside Macintosh Volume I, I-356.
        let (classic_top, classic_left, classic_bottom, classic_right) =
            region.highlighted_rect(menu_bar_height);
        for y in classic_top..classic_bottom {
            for x in classic_left..classic_right {
                if x >= 0 && x < screen_width && y >= 0 && y < screen_height {
                    if pixel_size == 1 {
                        let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
                        let bit = 7 - (x as u32 % 8);
                        let addr = screen_base + byte_offset;
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, b ^ (1 << bit));
                    } else if matches!(pixel_size, 2 | 4) {
                        Self::hilite_packed_menu_pixel(
                            bus,
                            (screen_base, row_bytes, screen_width, screen_height),
                            pixel_size,
                            x,
                            y,
                            hilite_indexes.unwrap_or((0, ((1u16 << pixel_size) - 1) as u8)),
                        );
                    } else if let Some((background, foreground)) = hilite_indexes {
                        let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                        bus.map_screen_byte(addr, |index| {
                            Self::menu_hilited_pixel_index(index, background, foreground)
                        });
                    } else {
                        let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                        bus.map_screen_byte(addr, |index| {
                            Self::menu_hilited_pixel_index(index, 0, 255)
                        });
                    }
                }
            }
        }
        // StandardMBDF reverses a title's foreground/background pair and
        // then replots the title. Preserve the color system-mark artwork
        // itself: only its transparent cell background participates in the
        // reversal. Replotting is intentionally limited to indexed color;
        // the monochrome mark remains ordinary reversed one-bit title ink.
        self.redraw_color_system_menu_mark_title(bus, menu_idx, region.title_origin());
    }

    fn redraw_color_system_menu_mark_title(
        &self,
        bus: &mut MacMemoryBus,
        menu_idx: usize,
        title_x: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let Some(menu_enabled) = self
            .menus
            .get(menu_idx)
            .filter(|menu| Self::is_system_menu_mark_title(&menu.title))
            .map(|menu| menu.enabled)
        else {
            return;
        };
        if !matches!(pixel_size, 2 | 4 | 8) {
            return;
        }
        self.fb_draw_retro_computer_menu_mark(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            title_x,
        );
        if !menu_enabled {
            let menu_bar_height = bus.read_word(addr::MBAR_HEIGHT) as i16;
            // Read the already-reversed cell background rather than assuming
            // which half of the toggle this call represents. This keeps a
            // disabled mark's checkerboard dimming involutive across both
            // HiliteMenu and nonzero FlashMenuBar calls.
            let background_x = title_x - 9;
            let background_index = Self::fb_get_pixel_index(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                background_x,
                1,
            )
            .unwrap_or(0);
            self.fb_apply_menu_title_dim_pattern(
                bus,
                (
                    1,
                    title_x,
                    menu_bar_height - 1,
                    title_x.saturating_add(Self::menu_title_advance("\u{14}")),
                ),
                Some(background_index),
                true,
            );
        }
    }

    /// Measure a string's width in pixels without drawing it.
    pub(crate) fn fb_measure_string(s: &str, font_id: i16, font_size: i16) -> i16 {
        let mut width: i16 = 0;
        for ch in s.chars() {
            if let Some((glyph, _)) = crate::quickdraw::text::get_glyph(font_id, font_size, ch) {
                width += glyph.advance as i16;
            } else {
                width += 6;
            }
        }
        width
    }
}

#[cfg(test)]
mod tests;
