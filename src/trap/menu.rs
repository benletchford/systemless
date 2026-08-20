//! Menu Manager trap handlers.

use crate::cpu::{CpuOps, Register};
use crate::memory::{globals::addr, MacMemoryBus, MemoryBus};
use crate::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};
use crate::trap::types::{decode_mac_roman, encode_mac_roman_lossy};
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
    /// True after InsertMenu; false after NewMenu/DeleteMenu/ClearMenuBar.
    /// GetMHandle only returns menus in the current menu list (per IM:I I-361).
    pub in_menu_bar: bool,
    /// True when InsertMenu was called with beforeID = -1, placing this menu
    /// in the hierarchical/pop-up portion of the current menu list.
    pub hierarchical: bool,
    /// True when this current-menu-list entry contributes a visible menu-bar
    /// title. InsertMenu(menu, -1) installs a submenu/popup without a title.
    /// Macintosh Toolbox Essentials 1992, p. 3-121.
    pub visible_in_menu_bar: bool,
}

/// State for one visible hierarchical submenu while MenuSelect is tracking.
pub struct SubmenuTrackingState {
    pub menu: usize,
    pub parent_item: i16,
    pub highlighted_item: i16,
    pub saved_pixels: Vec<u8>,
    pub dropdown_rect: (i16, i16, i16, i16),
}

/// State for MenuSelect mouse tracking across frames.
pub struct MenuTrackingState {
    pub active_menu: usize,
    pub highlighted_item: i16,
    pub saved_pixels: Vec<u8>,
    pub dropdown_rect: (i16, i16, i16, i16),
    pub submenu: Option<SubmenuTrackingState>,
    pub stack_ptr: u32,
    /// Remaining flash toggles (6 = 3 flashes: off, on, off, on, off, on).
    /// 0 means not flashing.
    pub flash_remaining: u8,
    /// Frames left in the current toggle phase before switching.
    /// Real Mac held each phase ~3 ticks (50ms) ≈ 3 frames at 60fps.
    pub flash_delay: u8,
    /// The result to return after flashing completes.
    pub flash_result: u32,
}

// Macintosh Toolbox Essentials 1992, pp. 3-137 to 3-138: an icon number
// maps to resource ID icon+256; key-equivalent bytes $1D and $1E select
// reduced ICON and SICN menu icons instead of command-key shortcuts.
const MENU_KEY_REDUCED_ICON: u8 = 0x1D;
const MENU_KEY_SMALL_ICON: u8 = 0x1E;
const MENU_ROW_HEIGHT: i16 = 16;
const MENU_TEXT_STYLE_SHADOW: u8 = 0x10;
const MENU_SHADOW_STYLE_ROW_HEIGHT: i16 = MENU_ROW_HEIGHT + 5;
const MENU_NORMAL_ICON_SIZE: i16 = 32;
const MENU_NORMAL_ICON_ROW_HEIGHT: i16 = 34;
const MENU_NORMAL_ICON_TEXT_LEFT_OFFSET: i16 = 51;

#[derive(Clone, Copy, Debug)]
struct MenuCIconLayout {
    width: i16,
    height: i16,
    pm_row_bytes: u32,
    mask_row_bytes: u32,
    bmap_row_bytes: u32,
    pixel_size: u16,
    mask_data_ptr: u32,
    bmap_data_ptr: u32,
    pixel_data_ptr: u32,
}

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

/// Count menu items by parsing the MENU data structure in guest memory.
/// The handle is dereferenced to get the pointer to the menu record.
/// Inside Macintosh Volume I, I-345
fn count_menu_items_from_memory(bus: &MacMemoryBus, menu_handle: u32) -> u16 {
    if menu_handle == 0 {
        return 0;
    }
    let menu_ptr = bus.read_long(menu_handle);
    if menu_ptr == 0 {
        return 0;
    }
    // Skip fixed header (14 bytes) + title Pascal string
    let title_len = bus.read_byte(menu_ptr + 14) as u32;
    let mut offset = 15 + title_len;
    let mut count: u16 = 0;
    loop {
        let item_len = bus.read_byte(menu_ptr + offset) as u32;
        if item_len == 0 {
            break;
        }
        count += 1;
        offset += 1 + item_len + 4; // pstring + icon + key + mark + style
    }
    count
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
fn internal_menu_string_bytes(value: &str) -> Vec<u8> {
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

fn internal_menu_string_to_unicode(value: &str) -> String {
    decode_mac_roman(&internal_menu_string_bytes(value))
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

/// Parse AppendMenu's data Pascal string into a `Vec<MenuItem>`.
/// Per Inside Macintosh Volume I, I-358:
///
/// - Items are separated by `;`.
/// - Within an item, meta-characters modify the item:
///   - `/<char>`  command-key equivalent
///   - `(`        item is disabled (the `(` is consumed, not stored)
///   - `<B/I/U/O/S`  style: Bold/Italic/Underline/Outline/Shadow
///   - `!<char>`  item mark (e.g. checkmark `\u{12}`)
///   - `^<char>`  icon (icon number = char value − 1)
/// - The leftover characters form the item's display text.
fn parse_appendmenu_items(bytes: &[u8]) -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = Vec::new();
    if bytes.is_empty() {
        return items;
    }
    for raw_item in bytes.split(|&b| b == b';') {
        let mut text = Vec::with_capacity(raw_item.len());
        let mut item = MenuItem {
            text: String::new(),
            icon: 0,
            key_equiv: 0,
            mark: 0,
            style: 0,
            enabled: true,
        };
        let mut i = 0;
        while i < raw_item.len() {
            let c = raw_item[i];
            match c {
                b'(' => {
                    item.enabled = false;
                    i += 1;
                }
                b'/' if i + 1 < raw_item.len() => {
                    item.key_equiv = raw_item[i + 1];
                    i += 2;
                }
                b'!' if i + 1 < raw_item.len() => {
                    item.mark = raw_item[i + 1];
                    i += 2;
                }
                b'^' if i + 1 < raw_item.len() => {
                    item.icon = raw_item[i + 1].saturating_sub(1);
                    i += 2;
                }
                b'<' if i + 1 < raw_item.len() => {
                    item.style |= match raw_item[i + 1] {
                        b'B' => 0x01, // bold
                        b'I' => 0x02, // italic
                        b'U' => 0x04, // underline
                        b'O' => 0x08, // outline
                        b'S' => 0x10, // shadow
                        _ => 0,
                    };
                    i += 2;
                }
                _ => {
                    text.push(c);
                    i += 1;
                }
            }
        }
        item.text = macroman_to_string(&text);
        items.push(item);
    }
    items
}

/// Rebuild the enableFlags longword from a Menu's enabled state and write it
/// back to the guest-memory MENU record at offset 10.
/// Inside Macintosh Volume I, I-345: bit 0 = menu enabled; bits 1–31 = items.
///
/// Start from $FFFFFFFF (all bits set, matching NewMenu's seed) and CLEAR
/// bits for disabled items. Real Mac ROM preserves the high bits for
/// non-existent items rather than rebuilding from scratch, so a disable
/// of item 2 yields $FFFFFFFB (not $0000001B).
fn sync_enable_flags(bus: &mut MacMemoryBus, menu: &Menu) {
    if menu.handle == 0 {
        return;
    }
    let menu_ptr = bus.read_long(menu.handle);
    if menu_ptr == 0 {
        return;
    }
    let mut flags: u32 = 0xFFFFFFFF;
    if !menu.enabled {
        flags &= !1u32;
    }
    for (i, item) in menu.items.iter().enumerate() {
        if i >= 31 {
            break;
        }
        if !item.enabled {
            flags &= !(1u32 << (i + 1));
        }
    }
    bus.write_long(menu_ptr + 10, flags);
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

/// Serialise a Menu's items into the guest-memory MENU record.
/// Per IM:I I-355: menuData starts at `menu_ptr + 14` and contains the
/// title (Pascal string) followed by items. Each item is a Pascal
/// string for the item text followed by 4 attribute bytes:
/// icon (1), keyEquiv (1), mark (1), style (1). The items list is
/// terminated by a length-0 byte.
///
/// Without this sync, the `count_menu_items_from_memory` path (used by
/// CountMItems and CalcMenuSize to stay compatible with GetMenu-loaded
/// menus) finds only the NewMenu-seeded terminator and reports 0 items
/// even though AppendMenu populated `self.menus`.
///
fn serialized_menu_record_size(bus: &MacMemoryBus, menu: &Menu) -> u32 {
    let menu_ptr = bus.read_long(menu.handle);
    let title_len = bus.read_byte(menu_ptr + 14) as u32;
    menu.items.iter().fold(16 + title_len, |size, item| {
        size.saturating_add(5 + internal_menu_string_bytes(&item.text).len().min(255) as u32)
    })
}

fn write_menu_items_to_memory(
    bus: &mut MacMemoryBus,
    menu: &Menu,
    menu_ptr: u32,
    menu_record_size: u32,
) {
    let title_len = bus.read_byte(menu_ptr + 14) as u32;
    let mut offset = 15 + title_len;
    for item in &menu.items {
        let encoded = internal_menu_string_bytes(&item.text);
        let bytes = encoded.as_slice();
        let text_len = bytes.len().min(255) as u32;
        let item_size = 1 + text_len + 4;
        if offset + item_size + 1 > menu_record_size {
            break;
        }
        bus.write_byte(menu_ptr + offset, text_len as u8);
        for (i, b) in bytes.iter().take(text_len as usize).enumerate() {
            bus.write_byte(menu_ptr + offset + 1 + i as u32, *b);
        }
        let attr_base = menu_ptr + offset + 1 + text_len;
        bus.write_byte(attr_base, item.icon);
        bus.write_byte(attr_base + 1, item.key_equiv);
        bus.write_byte(attr_base + 2, item.mark);
        bus.write_byte(attr_base + 3, item.style);
        offset += item_size;
    }
    if offset < menu_record_size {
        bus.write_byte(menu_ptr + offset, 0);
    }
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

/// Live menu-color table entry size in guest memory.
///
/// The compiled `'mctb'` resource entries are 28 bytes, but the in-memory
/// `MCEntry` record adds the trailing reserved word, so live table entries are
/// 30 bytes each.
const MC_ENTRY_SIZE: usize = 30;
const MC_RESOURCE_ENTRY_SIZE: usize = 28;
const MC_ALL_ITEMS: i16 = -98;
const MC_LAST_ID_INDIC: i16 = -99;

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
    /// Copy the live, inserted Menu Manager state into a frontend-neutral
    /// representation.  Refresh enable flags first because classic
    /// applications and custom menu procedures are allowed to mutate the
    /// guest-owned MenuInfo records directly.
    pub(crate) fn guest_menu_snapshot(&mut self, bus: &MacMemoryBus) -> GuestMenuSnapshot {
        for menu in &mut self.menus {
            refresh_menu_from_memory(bus, menu);
        }

        GuestMenuSnapshot {
            menus: self
                .menus
                .iter()
                .filter(|menu| menu.in_menu_bar)
                .map(|menu| GuestMenu {
                    id: menu.id,
                    title: internal_menu_string_to_unicode(&menu.title),
                    enabled: menu.enabled,
                    hierarchical: menu.hierarchical,
                    visible_in_menu_bar: menu.visible_in_menu_bar,
                    items: menu
                        .items
                        .iter()
                        .enumerate()
                        .map(|(index, item)| {
                            let submenu_id =
                                Self::is_hierarchical_item(item).then_some(item.mark as i16);
                            let key_equivalent = Self::menu_item_has_command_key(item)
                                .then(|| char::from(item.key_equiv).to_ascii_lowercase());
                            GuestMenuItem {
                                number: index as i16 + 1,
                                text: internal_menu_string_to_unicode(&item.text),
                                enabled: item.enabled,
                                checked: item.mark != 0 && submenu_id.is_none(),
                                key_equivalent,
                                submenu_id,
                                separator: item.text == "-",
                            }
                        })
                        .collect(),
                })
                .collect(),
        }
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
        for menu in &mut self.menus {
            refresh_menu_from_memory(bus, menu);
        }
        let menu = self
            .menus
            .iter()
            .find(|menu| menu.in_menu_bar && menu.id == menu_id)?;
        let item = menu.items.get(item_number.checked_sub(1)? as usize)?;
        if !menu.enabled || !item.enabled || item.text == "-" || Self::is_hierarchical_item(item) {
            return None;
        }
        let (_, left, right) = self.menu_title_regions_with_indices().into_iter().next()?;
        let selection = (menu_id, item_number);
        if self.pending_native_menu_selection != Some(selection) {
            self.pending_native_menu_selection = Some(selection);
            self.pending_native_menu_event = Some(super::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                where_v: 10,
                where_h: left + (right - left) / 2,
                modifiers: self.current_event_modifiers(),
            });
            self.pending_native_menu_event_tick = None;
        }
        Some((10, left + (right - left) / 2))
    }

    /// Consume a staged selection only if it is still valid in the current
    /// guest menu list.  Menu contents can change while AppKit is tracking a
    /// native menu, so validation at enqueue time alone is insufficient.
    fn take_native_menu_selection(&mut self, bus: &MacMemoryBus) -> Option<u32> {
        let (menu_id, item_number) = self.pending_native_menu_selection.take()?;
        self.pending_native_menu_event = None;
        self.pending_native_menu_event_tick = None;
        let menu = self
            .menus
            .iter_mut()
            .find(|menu| menu.in_menu_bar && menu.id == menu_id)?;
        refresh_menu_from_memory(bus, menu);
        let item = menu.items.get(item_number.checked_sub(1)? as usize)?;
        if !menu.enabled || !item.enabled || item.text == "-" || Self::is_hierarchical_item(item) {
            return None;
        }
        Some(((menu_id as u16 as u32) << 16) | item_number as u16 as u32)
    }

    fn menu_tracking_button_down(&self, bus: &MacMemoryBus) -> bool {
        // Menu tracking ends on mouse-up; MBState ($0172) is the documented
        // low-memory mouse button state (0=down, $80=up). Fold it in with
        // dispatcher state so guest callbacks that run during tracking can
        // release or hold the button between trap re-fires. Inside Macintosh
        // Volume II, p. II-371; Macintosh Toolbox Essentials 1992, p. 3-120.
        self.mouse_button || bus.read_byte(addr::MB_STATE) == 0x00
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
            self.mouse_pos
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

        self.ptr_to_handle
            .get(&candidate)
            .copied()
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
        self.record_input_trace_line(format!(
            "A93D action={} start={} live_mouse=({},{}) {} {} highlighted_item={} result={} outcome={}",
            action,
            start,
            self.mouse_pos.0,
            self.mouse_pos.1,
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

    pub(super) fn menu_color_entry_ptr(
        bus: &MacMemoryBus,
        menu_id: i16,
        menu_item: i16,
    ) -> Option<u32> {
        let handle = bus.read_long(addr::MENU_C_INFO);
        if handle == 0 {
            return None;
        }
        let data_ptr = bus.read_long(handle);
        if data_ptr == 0 {
            return None;
        }

        let size = bus.get_alloc_size(data_ptr).unwrap_or(0) as usize;
        let mut offset = 0usize;
        while offset + MC_ENTRY_SIZE <= size {
            let entry_ptr = data_ptr + offset as u32;
            if bus.read_word(entry_ptr) as i16 == menu_id
                && bus.read_word(entry_ptr + 2) as i16 == menu_item
            {
                return Some(entry_ptr);
            }
            offset += MC_ENTRY_SIZE;
        }
        None
    }

    pub(super) fn menu_color_entry_rgb(
        bus: &MacMemoryBus,
        entry_ptr: u32,
        rgb_offset: u32,
    ) -> [u16; 3] {
        [
            bus.read_word(entry_ptr + rgb_offset),
            bus.read_word(entry_ptr + rgb_offset + 2),
            bus.read_word(entry_ptr + rgb_offset + 4),
        ]
    }

    fn menu_color_rgb_pixel_index(bus: &MacMemoryBus, rgb: [u16; 3]) -> Option<u8> {
        Self::fb_pixel_index_for_rgb(bus, rgb)
    }

    pub(super) fn menu_bar_background_pixel_index(
        &self,
        bus: &MacMemoryBus,
        pixel_size: u16,
    ) -> Option<u8> {
        if !matches!(pixel_size, 4 | 8) {
            return None;
        }

        // IM:V 1986 pp. V-232 to V-233 / MTE 1992 table 3-7:
        // a menu bar entry uses RGB4 for the menu bar color. Without
        // a menu bar entry, a menu title entry's RGB2 duplicates the bar
        // color; use the first current menu's title entry because those
        // duplicated values are expected to agree across the menu list.
        let rgb = if let Some(menu_bar) = Self::menu_color_entry_ptr(bus, 0, 0) {
            Self::menu_color_entry_rgb(bus, menu_bar, 22)
        } else if let Some(title) = self
            .menus
            .iter()
            .filter(|menu| menu.visible_in_menu_bar)
            .find_map(|menu| Self::menu_color_entry_ptr(bus, menu.id, 0))
        {
            Self::menu_color_entry_rgb(bus, title, 10)
        } else {
            return None;
        };
        Self::menu_color_rgb_pixel_index(bus, rgb)
    }

    pub(super) fn menu_title_pixel_index(
        bus: &MacMemoryBus,
        menu_id: i16,
        pixel_size: u16,
    ) -> Option<u8> {
        if !matches!(pixel_size, 4 | 8) {
            return None;
        }

        // IM:V 1986 p. V-232: title entries use RGB1 for the title.
        // If absent, the menu bar entry's RGB1 supplies the default title
        // color; without either entry, the standard color is black.
        let rgb = if let Some(title) = Self::menu_color_entry_ptr(bus, menu_id, 0) {
            Self::menu_color_entry_rgb(bus, title, 4)
        } else if let Some(menu_bar) = Self::menu_color_entry_ptr(bus, 0, 0) {
            Self::menu_color_entry_rgb(bus, menu_bar, 4)
        } else {
            return None;
        };
        Self::menu_color_rgb_pixel_index(bus, rgb)
    }

    fn menu_dropdown_background_pixel_index(
        bus: &MacMemoryBus,
        menu_id: i16,
        pixel_size: u16,
    ) -> Option<u8> {
        if pixel_size != 8 {
            return None;
        }

        // IM:V 1986 pp. V-232 to V-233 / MTE 1992 table 3-7:
        // a menu title entry uses RGB4 for the pulled-down menu
        // background; if absent, the menu bar entry uses RGB2.
        let rgb = if let Some(title) = Self::menu_color_entry_ptr(bus, menu_id, 0) {
            Self::menu_color_entry_rgb(bus, title, 22)
        } else if let Some(menu_bar) = Self::menu_color_entry_ptr(bus, 0, 0) {
            Self::menu_color_entry_rgb(bus, menu_bar, 10)
        } else {
            return None;
        };
        Self::menu_color_rgb_pixel_index(bus, rgb)
    }

    fn menu_item_component_pixel_index(
        bus: &MacMemoryBus,
        menu_id: i16,
        menu_item: i16,
        pixel_size: u16,
        item_rgb_offset: u32,
    ) -> Option<u8> {
        if pixel_size != 8 {
            return None;
        }

        // IM:V 1986 p. V-233: item entries supply mark/name/command
        // colors in RGB1/RGB2/RGB3. Missing item entries fall back to
        // the menu title's default item color (RGB3), then the menu bar
        // default item color (RGB3), then standard black.
        let rgb = if let Some(item) = Self::menu_color_entry_ptr(bus, menu_id, menu_item) {
            Self::menu_color_entry_rgb(bus, item, item_rgb_offset)
        } else if let Some(title) = Self::menu_color_entry_ptr(bus, menu_id, 0) {
            Self::menu_color_entry_rgb(bus, title, 16)
        } else if let Some(menu_bar) = Self::menu_color_entry_ptr(bus, 0, 0) {
            Self::menu_color_entry_rgb(bus, menu_bar, 16)
        } else {
            return None;
        };
        Self::menu_color_rgb_pixel_index(bus, rgb)
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
            Some(index) => Self::fb_rgb_for_pixel_index(bus, index).unwrap_or(default),
            None => default,
        };
        let background = resolve(background_index, [0xFFFF; 3]);
        let content = resolve(content_index, [0; 3]);
        Self::fb_gray_pixel_index_between(bus, background, content)
    }

    fn menu_hilite_pixel_indexes(
        &self,
        bus: &MacMemoryBus,
        background_index: Option<u8>,
        pixel_size: u16,
    ) -> Option<(u8, u8)> {
        if pixel_size != 8 {
            return None;
        }

        // HIG 1992 p. 38 and Imaging With QuickDraw 1994 p. 4-42:
        // color hilite mode swaps the background color with HiliteRGB
        // instead of applying an arbitrary indexed-pixel complement.
        let background =
            background_index.or_else(|| Self::menu_color_rgb_pixel_index(bus, [0xFFFF; 3]))?;
        let hilite = Self::menu_color_rgb_pixel_index(
            bus,
            [
                self.hilite_color.0,
                self.hilite_color.1,
                self.hilite_color.2,
            ],
        )?;
        Some((background, hilite))
    }

    fn menu_hilited_pixel_index(pixel: u8, background: u8, hilite: u8) -> u8 {
        if pixel == background {
            hilite
        } else if pixel == hilite {
            background
        } else {
            pixel
        }
    }

    fn menu_plain_hilited_pixel_index(bus: &MacMemoryBus, pixel: u8) -> u8 {
        let white = Self::fb_pixel_index_for_rgb(bus, [0xFFFF; 3]).unwrap_or(0);
        let black = Self::fb_pixel_index_for_rgb(bus, [0; 3]).unwrap_or(255);
        if pixel == white {
            black
        } else if pixel == black {
            white
        } else {
            pixel
        }
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
        self.ptr_to_handle.insert(data_ptr, handle);
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
        self.ptr_to_handle.insert(menu_ptr, handle);

        let menu = if let Some((_, res_ptr)) = self.find_or_load_resource_any(bus, *b"MENU", menu_id) {
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
        let mut new_bytes = current_bytes.clone();
        for entry in entries.chunks_exact(MC_ENTRY_SIZE) {
            let Some((menu_id, menu_item)) = mc_entry_key(entry) else {
                continue;
            };
            let mut found_offset = None;
            for (entry_index, existing) in new_bytes.chunks_exact(MC_ENTRY_SIZE).enumerate() {
                if mc_entry_matches(existing, menu_id, menu_item) {
                    found_offset = Some(entry_index * MC_ENTRY_SIZE);
                    break;
                }
            }
            if let Some(offset) = found_offset {
                new_bytes[offset..offset + MC_ENTRY_SIZE].copy_from_slice(entry);
            } else {
                new_bytes.extend_from_slice(entry);
            }
        }
        let _ = self.replace_handle_bytes(bus, current_handle, &new_bytes);
    }

    fn load_menu_color_resource(&mut self, bus: &mut MacMemoryBus, resource_id: i16) {
        let Some((_, resource_ptr)) = self.find_or_load_resource_any(bus, *b"mctb", resource_id) else {
            return;
        };
        let resource_size = bus.get_alloc_size(resource_ptr).unwrap_or(0) as usize;
        if resource_size < 2 {
            return;
        }

        let declared_count = bus.read_word(resource_ptr) as i16;
        if declared_count <= 0 {
            return;
        }

        let available_entries = (resource_size - 2) / MC_RESOURCE_ENTRY_SIZE;
        let entry_count = (declared_count as usize).min(available_entries);
        let mut entries = Vec::with_capacity(entry_count * MC_ENTRY_SIZE);
        for index in 0..entry_count {
            let entry_ptr = resource_ptr + 2 + (index * MC_RESOURCE_ENTRY_SIZE) as u32;
            let menu_id = bus.read_word(entry_ptr) as i16;
            if menu_id == MC_LAST_ID_INDIC {
                continue;
            }
            entries.extend_from_slice(&bus.read_bytes(entry_ptr, MC_RESOURCE_ENTRY_SIZE));
            entries.extend_from_slice(&0u16.to_be_bytes());
        }

        // IM:V 1986 p. V-234 and MTE 1992 p. 3-156 define compiled
        // 'mctb' resources as a count-prefixed array of 28-byte color
        // entries; the live MCEntry table has the extra reserved word.
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

        let mut filtered = Vec::with_capacity(current_bytes.len());
        for entry in current_bytes.chunks_exact(MC_ENTRY_SIZE) {
            if let Some((menu_id, menu_item)) = mc_entry_key(entry) {
                if keep(menu_id, menu_item) {
                    filtered.extend_from_slice(entry);
                }
            }
        }
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
            self.ptr_to_handle.remove(&old_ptr);
            bus.free(old_ptr);
        }
        bus.write_long(handle, new_ptr);
        if new_ptr != 0 {
            self.ptr_to_handle.insert(new_ptr, handle);
        }
        true
    }

    fn sync_guest_menu_list(&mut self, bus: &mut MacMemoryBus) {
        let regular = self
            .menus
            .iter()
            .filter(|menu| menu.in_menu_bar && !menu.hierarchical)
            .collect::<Vec<_>>();
        let hierarchical = self
            .menus
            .iter()
            .filter(|menu| menu.in_menu_bar && menu.hierarchical)
            .collect::<Vec<_>>();

        let title_lefts = self
            .menu_title_regions_with_indices()
            .into_iter()
            .filter_map(|(index, left, _)| self.menus.get(index).map(|menu| (menu.handle, left)))
            .collect::<std::collections::HashMap<_, _>>();
        let last_right = self
            .menu_title_regions_with_indices()
            .into_iter()
            .last()
            .map(|(_, _, right)| right)
            .unwrap_or(0);

        // System 7's DynamicMenuList contains a six-byte header, one
        // six-byte MenuRec per regular menu, lastHMenu + menuTitleSave,
        // then one six-byte HMenuRec per hierarchical/pop-up menu.
        // Inside Macintosh Volume V (1986), pp. V-228–V-230.
        let mut bytes = Vec::with_capacity(12 + 6 * (regular.len() + hierarchical.len()));
        bytes.extend_from_slice(&((regular.len() * 6) as i16).to_be_bytes());
        bytes.extend_from_slice(&last_right.to_be_bytes());
        bytes.extend_from_slice(&0i16.to_be_bytes());
        for menu in regular {
            bytes.extend_from_slice(&menu.handle.to_be_bytes());
            bytes.extend_from_slice(
                &title_lefts
                    .get(&menu.handle)
                    .copied()
                    .unwrap_or(0)
                    .to_be_bytes(),
            );
        }
        bytes.extend_from_slice(&((hierarchical.len() * 6) as i16).to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        for menu in hierarchical {
            bytes.extend_from_slice(&menu.handle.to_be_bytes());
            bytes.extend_from_slice(&0i16.to_be_bytes());
        }

        let mut handle = bus.read_long(addr::MENU_LIST);
        if handle == 0 {
            handle = bus.alloc(4);
            bus.write_long(addr::MENU_LIST, handle);
        }
        let _ = self.replace_handle_bytes(bus, handle, &bytes);
    }

    fn ensure_menu_record_capacity(
        &mut self,
        bus: &mut MacMemoryBus,
        menu_handle: u32,
        min_size: u32,
    ) -> u32 {
        if menu_handle == 0 {
            return 0;
        }
        let menu_ptr = bus.read_long(menu_handle);
        if menu_ptr == 0 {
            return 0;
        }
        let current_size = bus
            .get_alloc_size(menu_ptr)
            .unwrap_or_else(|| menu_resource_size(bus, menu_ptr) as u32);
        if current_size >= min_size {
            return menu_ptr;
        }

        self.resize_resource_allocation(bus, menu_handle, menu_ptr, min_size)
    }

    fn serialise_menu_items_to_memory(&mut self, bus: &mut MacMemoryBus, menu: &Menu) {
        if menu.handle == 0 || bus.read_long(menu.handle) == 0 {
            return;
        }
        let required_size = serialized_menu_record_size(bus, menu).max(256);
        let menu_ptr = self.ensure_menu_record_capacity(bus, menu.handle, required_size);
        if menu_ptr == 0 {
            return;
        }
        let record_size = bus
            .get_alloc_size(menu_ptr)
            .unwrap_or_else(|| menu_resource_size(bus, menu_ptr) as u32);
        write_menu_items_to_memory(bus, menu, menu_ptr, record_size);
    }

    fn refresh_menus_from_memory(&mut self, bus: &MacMemoryBus) {
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
        Some(match (is_tool, trap_num) {
            // InitMenus ($A930)
            // Per IM:I I-351: "InitMenus initializes the Menu
            // Manager. It allocates space for the menu list (a
            // relocatable block in the heap large enough for the
            // maximum-size menu list), and draws the (empty) menu
            // bar. Call InitMenus once before all other Menu
            // Manager routines. An application should never have
            // to call this procedure more than once; to start
            // afresh with all new menus, use ClearMenuBar."
            // PROCEDURE InitMenus;
            // Inside Macintosh Volume I, I-351
            //
            // No args, no result. Pop 0 bytes.
            //
            // HLE compromise: Systemless's Menu Manager state lives in
            // `self.menus: Vec<Menu>` (initialised empty at
            // TrapDispatcher::new() time) and in the live MenuCInfo
            // table stored at lowmem $0D50. Per IM:V 1986 p. V-234,
            // InitMenus attempts to load 'mctb' resource 0 into that
            // table; repeated calls merge/update entries, matching the
            // SetMCEntries behavior used by the real Menu Manager. Empty
            // menu bar paint is still a no-op for the same reason
            // InitWindows's is: chrome runs once per frame at
            // end-of-tick AND is hidden by default per menu_bar_hidden
            // default-on (dispatch.rs:1580). Per IM:I I-351 the
            // explicit call-once contract is enforceable but not
            // enforced in HLE — repeated calls are idempotent no-ops,
            // which matches the IM-documented "use ClearMenuBar to
            // start afresh" recovery path semantically.
            (true, 0x130) => {
                let _ = self.ensure_menu_color_table_handle(bus);
                self.load_menu_color_resource(bus, 0);
                self.sync_guest_menu_list(bus);
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
                let menu_ptr = bus.alloc(256);
                let handle = bus.alloc(4);
                bus.write_long(handle, menu_ptr);
                self.ptr_to_handle.insert(menu_ptr, handle);
                // MenuInfo layout per IM:I I-355:
                //   +0  menuID
                //   +2  menuWidth
                //   +4  menuHeight
                //   +6  menuProc (handle)
                //   +10 enableFlags
                //   +14 menuData (pstring title followed by items)
                bus.write_word(menu_ptr, menu_id as u16);
                bus.write_word(menu_ptr + 2, 0);
                bus.write_word(menu_ptr + 4, 0);
                let menu_proc = self.menu_def_proc_handle(bus, 0);
                bus.write_long(menu_ptr + 6, menu_proc);
                bus.write_long(menu_ptr + 10, 0xFFFFFFFF); // enable all items
                let mut title = String::new();
                if title_ptr != 0 {
                    let title_len = bus.read_byte(title_ptr) as u32;
                    bus.write_byte(menu_ptr + 14, title_len as u8);
                    let mut title_bytes = Vec::with_capacity(title_len as usize);
                    for i in 0..title_len {
                        let b = bus.read_byte(title_ptr + 1 + i);
                        bus.write_byte(menu_ptr + 15 + i, b);
                        title_bytes.push(b);
                    }
                    // Terminate the menuData area with an empty item string
                    // so AppendMenu's scan knows where to append.
                    bus.write_byte(menu_ptr + 15 + title_len, 0);
                    title = macroman_to_string(&title_bytes);
                }
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
                let handle = match self.find_or_load_resource_any(bus, *b"MENU", menu_id) {
                    Some((refnum, res_ptr)) => {
                        let handle = self.get_or_create_resource_handle_in_file(
                            bus, *b"MENU", menu_id, res_ptr, refnum,
                        );
                        if handle != 0 {
                            let mut menu_ptr = bus.read_long(handle);
                            if menu_ptr == 0 {
                                menu_ptr = res_ptr;
                                bus.write_long(handle, menu_ptr);
                                self.ptr_to_handle.insert(menu_ptr, handle);
                            }
                            if menu_ptr != 0 {
                                if bus.get_alloc_size(menu_ptr).unwrap_or(0) < 256 {
                                    let resized =
                                        self.resize_resource_allocation(bus, handle, menu_ptr, 256);
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
                                        cpu.write_reg(Register::D0, -192i32 as u32);
                                        bus.write_long(sp + 2, 0);
                                        cpu.write_reg(Register::A7, sp + 2);
                                        return Some(Ok(()));
                                    }
                                    bus.write_long(menu_ptr + 6, menu_proc);
                                }
                                let mut parsed = parse_menu_resource(bus, menu_ptr, handle);
                                if let Some(existing) =
                                    self.menus.iter_mut().find(|m| m.handle == handle)
                                {
                                    parsed.in_menu_bar = existing.in_menu_bar;
                                    parsed.hierarchical = existing.hierarchical;
                                    parsed.visible_in_menu_bar = existing.visible_in_menu_bar;
                                    *existing = parsed;
                                } else {
                                    self.menus.push(parsed);
                                }
                            }
                        }
                        self.load_menu_color_resource(bus, menu_id);
                        bus.write_word(0x0A60, 0);
                        cpu.write_reg(Register::D0, 0);
                        handle
                    }
                    None => {
                        bus.write_word(0x0A60, (-192i16) as u16);
                        cpu.write_reg(Register::D0, -192i32 as u32);
                        0
                    }
                };
                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // AppendMenu ($A933)
            // Adds one or more menu items to the end of a menu.
            // PROCEDURE AppendMenu(theMenu: MenuHandle; data: Str255);
            // Inside Macintosh Volume I, I-358:
            //   The data string is a series of items separated by
            //   semicolons. Within each item the following meta-
            //   characters modify the item:
            //     /<char>   command-key equivalent
            //     (         disable item
            //     <B/I/U/O/S  style modifier
            //     !<char>   item mark
            //     ^<char>   icon (icon number = char - 1)
            // AppendMenu ($A933): Parses item text and appends to internal menu
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
                let parsed = parse_appendmenu_items(&bytes);

                if !self.menus.iter().any(|m| m.handle == menu_handle) {
                    let menu_ptr = bus.read_long(menu_handle);
                    if menu_ptr != 0 {
                        self.menus
                            .push(parse_menu_resource(bus, menu_ptr, menu_handle));
                    }
                }
                let menu_copy = if let Some(menu) =
                    self.menus.iter_mut().find(|m| m.handle == menu_handle)
                {
                    refresh_menu_from_memory(bus, menu);
                    for item in parsed {
                        menu.items.push(item);
                    }
                    sync_enable_flags(bus, menu);
                    Some(menu.clone())
                } else {
                    None
                };
                // Also serialise items into the guest-memory MENU record so
                // CountMItems / CalcMenuSize (which parse guest memory to stay
                // compatible with GetMenu-loaded menus) see the AppendMenu'd
                // items. Per IM:I I-355 menuData layout.
                if let Some(menu_copy) = menu_copy {
                    self.serialise_menu_items_to_memory(bus, &menu_copy);
                }
                Ok(())
            }

            // InsertMenu ($A935)
            // Inserts a menu into the menu bar.
            // PROCEDURE InsertMenu(theMenu: MenuHandle; beforeID: INTEGER);
            // Inside Macintosh Volume I, I-352
            // InsertMenu ($A935): Parses MENU resource, adds to internal menu list
            (true, 0x135) => {
                let sp = cpu.read_reg(Register::A7);
                let before_id = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);
                let visible_in_menu_bar = before_id != -1;

                if menu_handle != 0 {
                    let menu_ptr = bus.read_long(menu_handle);
                    if menu_ptr != 0 {
                        // Per IM:I I-352, InsertMenu inserts a menu into the
                        // current menu list; it does not create/duplicate one.
                        if let Some(idx) = self.menus.iter().position(|m| m.handle == menu_handle) {
                            self.last_inserted_menu_id = Some(self.menus[idx].id);
                            self.menus[idx].in_menu_bar = true;
                            self.menus[idx].hierarchical = before_id == -1;
                            self.menus[idx].visible_in_menu_bar = visible_in_menu_bar;
                        } else {
                            // Handle wasn't previously tracked (for example a
                            // raw guest MENU handle). Parse any MENU resource
                            // by menu ID, else fall back to title-only memory.
                            let menu_id = bus.read_word(menu_ptr) as i16;
                            if let Some((_, res_ptr)) = self.find_or_load_resource_any(bus, *b"MENU", menu_id) {
                                let mut menu = parse_menu_resource(bus, res_ptr, menu_handle);
                                eprintln!(
                                    "[MENU] InsertMenu: ID={} title=\"{}\" items={}",
                                    menu.id,
                                    menu.title,
                                    menu.items.len()
                                );
                                menu.in_menu_bar = true;
                                menu.hierarchical = before_id == -1;
                                menu.visible_in_menu_bar = visible_in_menu_bar;
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
                                            in_menu_bar: true,
                                            hierarchical: before_id == -1,
                                            visible_in_menu_bar,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                self.sync_guest_menu_list(bus);

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
                // Initial kiosk mode is only a frontend launch policy. A
                // guest DrawMenuBar call is an explicit request to present its
                // menus, so ownership returns to the guest before rendering.
                self.release_initial_menu_bar_kiosk();
                self.refresh_menus_from_memory(bus);
                self.draw_menu_bar_to_fb(bus);
                Ok(())
            }

            // ClearMenuBar ($A934)
            // Removes all menus from the menu bar.
            // PROCEDURE ClearMenuBar;
            // Inside Macintosh Volume I, I-354
            //
            // Regression coverage:
            //   clearmenubar_empties_current_menu_list
            //   clearmenubar_has_no_parameters_and_preserves_stack_pointer
            // ClearMenuBar ($A934): Clears all menus from self.menus per IM:I I-354
            (true, 0x134) => {
                // IM:V 1986 p. V-244: ClearMenuBar clears both the
                // current menu list and the application's menu color
                // information table.
                self.menus.clear();
                self.clear_menu_color_table_entries(bus);
                self.sync_guest_menu_list(bus);
                Ok(())
            }

            // GetMHandle ($A949)
            // Returns a handle to the menu with the given ID.
            // FUNCTION GetMHandle(menuID: INTEGER): MenuHandle;
            // Inside Macintosh Volume I, I-361
            // GetMHandle ($A949): Returns handle for menus in the current menu list (inserted via InsertMenu); NIL if not found per IM:I I-361
            (true, 0x149) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_id = bus.read_word(sp) as i16;
                let handle = self
                    .menus
                    .iter()
                    .find(|m| m.id == menu_id && m.in_menu_bar)
                    .map(|m| m.handle)
                    .unwrap_or(0);
                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // SetMenuBar ($A93C)
            // Replaces the current menu bar with a copy of the saved
            // menu list previously vended by GetMenuBar ($A93B).
            // PROCEDURE SetMenuBar(menuList: Handle);
            // Inside Macintosh Volume I, I-354
            //
            // Per IM:I I-354 SetMenuBar makes a copy of the input list;
            // it does NOT call DrawMenuBar — callers must do that
            // themselves. Per IM:V V-243 / Macintosh Toolbox Essentials
            // 1992 8502..8511 the input is treated as opaque: we restore
            // the snapshot we saved at GetMenuBar time keyed by the
            // returned handle. Unrecognised handles are no-ops (no
            // DRVR-driven runtime synthesis is in scope here).
            // SetMenuBar ($A93C): Restores `self.menus` from the snapshot saved by GetMenuBar; unrecognised handles are no-ops per IM:I I-354
            (true, 0x13C) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_list = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                if let Some(saved) = self.saved_menu_bars.get(&menu_list).cloned() {
                    self.menus = saved;
                    self.sync_guest_menu_list(bus);
                }
                Ok(())
            }

            // GetNewMBar ($A9C0)
            // Reads an MBAR resource and builds a menu bar from it.
            // FUNCTION GetNewMBar(menuBarID: INTEGER): Handle;
            // Inside Macintosh Volume I, I-354
            // GetNewMBar ($A9C0): Returns NIL when MBAR can't be read;
            // otherwise returns a Handle to a newly built menu list that
            // callers install with SetMenuBar.
            // IM:V 1986 p. V-244: GetNewMBar builds a new menu color
            // information table while restoring the previous MenuList.
            (true, 0x1C0) => {
                let sp = cpu.read_reg(Register::A7);
                let mbar_id = bus.read_word(sp) as i16;
                let handle = if let Some((_, mbar_ptr)) = self.find_or_load_resource_any(bus, *b"MBAR", mbar_id)
                {
                    let menu_count = bus.read_word(mbar_ptr) as usize;
                    let mut snapshot = Vec::new();
                    self.clear_menu_color_table_entries(bus);

                    for i in 0..menu_count {
                        let menu_id = bus.read_word(mbar_ptr + 2 + (i as u32) * 2) as i16;
                        let Some((_, menu_res_ptr)) = self.find_or_load_resource_any(bus, *b"MENU", menu_id)
                        else {
                            continue;
                        };

                        let menu_ptr = bus.alloc(256);
                        let menu_handle = bus.alloc(4);
                        bus.write_long(menu_handle, menu_ptr);
                        let res_size = menu_resource_size(bus, menu_res_ptr);
                        for j in 0..res_size.min(256) {
                            bus.write_byte(
                                menu_ptr + j as u32,
                                bus.read_byte(menu_res_ptr + j as u32),
                            );
                        }

                        let mut parsed = parse_menu_resource(bus, menu_ptr, menu_handle);
                        // This list is not installed until SetMenuBar is called,
                        // but once installed it represents the current menu list.
                        parsed.in_menu_bar = true;
                        parsed.hierarchical = false;
                        parsed.visible_in_menu_bar = true;
                        snapshot.push(parsed);
                        self.load_menu_color_resource(bus, menu_id);
                    }

                    // Match GetMenuBar's handle shape: count word + menu handles.
                    let count = snapshot.len() as u32;
                    let list_block = bus.alloc(2 + 4 * count.max(1));
                    bus.write_word(list_block, count as u16);
                    for (idx, menu) in snapshot.iter().enumerate() {
                        bus.write_long(list_block + 2 + (idx as u32) * 4, menu.handle);
                    }
                    let list_handle = bus.alloc(4);
                    bus.write_long(list_handle, list_block);
                    self.saved_menu_bars.insert(list_handle, snapshot);
                    list_handle
                } else {
                    0
                };

                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // AddResMenu ($A94D)
            // Appends items to a menu from resources of a given type.
            // PROCEDURE AddResMenu(theMenu: MenuHandle; theType: ResType);
            // Inside Macintosh Volume I, I-353; Macintosh Toolbox
            // Essentials 1992, 3-101..3-102 (AppendResMenu — System 7
            // alias for AddResMenu).
            //
            // Per IM:I I-353 / IM:V V-242: walks the resource fork in
            // search order, appending each named resource of `theType`
            // as a menu item. Resources whose name starts with `.` or
            // `%` are skipped (Apple's convention for hidden DA names
            // and font-family marker entries). Items are appended in
            // resource-map order — sorted by ID ascending in our HLE,
            // matching the on-disk map-table layout per IM:I I-118.
            //
            // Stack: SP+0 theType (4), SP+4 theMenu handle (4). Pop 8.
            // AddResMenu ($A94D): Walks the current resource search order, appends named resources of theType (skip names starting with '.' or '%') per IM:I I-353
            (true, 0x14D) => {
                let sp = cpu.read_reg(Register::A7);
                let res_type_word = bus.read_long(sp);
                let menu_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);

                let res_type = res_type_word.to_be_bytes();
                let mut entries = self.named_resources_of_type(res_type);
                if res_type == *b"FONT" {
                    // Systemless's built-in bitmap families are HLE data, not
                    // guest FONT resources. AddResMenu must nevertheless make
                    // those installed families visible to applications that
                    // build a Font menu from the Font Manager resource type.
                    // Inside Macintosh Volume I (1985), pp. I-217, I-353.
                    for &(id, name) in crate::quickdraw::fonts::FONT_NAMES {
                        if id == crate::quickdraw::fonts::FONT_APPLICATION
                            || entries.iter().any(|(_, entry)| entry == name)
                        {
                            continue;
                        }
                        entries.push((id, name.to_string()));
                    }
                    entries.sort_by_key(|(id, _)| *id);
                }

                let mut touched: Option<Menu> = None;
                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    refresh_menu_from_memory(bus, menu);
                    for (_id, name) in entries {
                        if name.is_empty() || name.starts_with('.') || name.starts_with('%') {
                            continue;
                        }
                        // Per IM:I I-358 AddResMenu's "no duplicates"
                        // contract: if an item with this exact text
                        // already exists, skip it. Real Mac does this
                        // case-sensitively.
                        if menu.items.iter().any(|it| it.text == name) {
                            continue;
                        }
                        menu.items.push(MenuItem {
                            text: name,
                            icon: 0,
                            key_equiv: 0,
                            mark: 0,
                            style: 0,
                            enabled: true,
                        });
                    }
                    sync_enable_flags(bus, menu);
                    touched = Some(menu.clone());
                }
                if let Some(m) = touched {
                    self.serialise_menu_items_to_memory(bus, &m);
                }
                Ok(())
            }

            // DisableItem ($A93A)
            // Disables a menu item so it cannot be chosen.
            // PROCEDURE DisableItem(theMenu: MenuHandle; item: INTEGER);
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-131
            //
            // DisableItem ($A93A): item=0 disables whole menu; item>31
            // is a no-op for individual items per IM:TB 1992 p.3-131.
            (true, 0x13A) => {
                let sp = cpu.read_reg(Register::A7);
                let item = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);

                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    refresh_menu_from_memory(bus, menu);
                    if item == 0 {
                        menu.enabled = false;
                    } else if (1..=31).contains(&item) {
                        if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                            mi.enabled = false;
                        }
                    }
                    if std::env::var_os("SYSTEMLESS_TRACE_MENUKEY").is_some() {
                        eprintln!(
                            "[MENUKEY] DisableItem menu={} title=\"{}\" item={} enabled={}",
                            menu.id, menu.title, item, menu.enabled
                        );
                    }
                    sync_enable_flags(bus, menu);
                }
                Ok(())
            }

            // EnableItem ($A939)
            // Enables a menu item so it can be chosen.
            // PROCEDURE EnableItem(theMenu: MenuHandle; item: INTEGER);
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-131
            //
            // EnableItem ($A939): item=0 reenables menu title while preserving
            // individually disabled items; item>31 is no-op per IM:TB 1992 p.3-131.
            (true, 0x139) => {
                let sp = cpu.read_reg(Register::A7);
                let item = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);

                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    refresh_menu_from_memory(bus, menu);
                    if item == 0 {
                        menu.enabled = true;
                    } else if (1..=31).contains(&item) {
                        if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                            mi.enabled = true;
                        }
                    }
                    if std::env::var_os("SYSTEMLESS_TRACE_MENUKEY").is_some() {
                        eprintln!(
                            "[MENUKEY] EnableItem menu={} title=\"{}\" item={} enabled={}",
                            menu.id, menu.title, item, menu.enabled
                        );
                    }
                    sync_enable_flags(bus, menu);
                }
                Ok(())
            }

            // MenuSelect ($A93D)
            // Tracks the mouse in the menu bar and returns the selected item.
            // FUNCTION MenuSelect(startPt: Point): LONGINT;
            // Inside Macintosh Volume I, I-355
            // MenuSelect ($A93D): Full mouse tracking with dropdown, highlighting, flashing
            (true, 0x13D) => {
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
                    // Re-fire: we're in tracking mode
                    if self.menu_tracking.as_ref().unwrap().flash_remaining > 0 {
                        // Flashing phase: hold each toggle for 3 frames (~50ms),
                        // matching the real Mac's ~3-tick delay per phase.
                        // redraw_chrome handles the visual state based on
                        // whether flash_remaining is even or odd.
                        let result = self.menu_tracking.as_ref().unwrap().flash_result;
                        let t = self.menu_tracking.as_mut().unwrap();
                        if t.flash_delay > 0 {
                            t.flash_delay -= 1;
                            return Some(Ok(()));
                        }
                        // Advance to next toggle
                        t.flash_remaining -= 1;
                        t.flash_delay = 3; // frames to hold next phase
                        let new_remaining = t.flash_remaining;

                        if new_remaining == 0 {
                            // Flash complete — finish up
                            let sp = self.menu_tracking.as_ref().unwrap().stack_ptr;
                            let saved = self.menu_tracking.take().unwrap();
                            let active_menu = saved
                                .submenu
                                .as_ref()
                                .map(|submenu| submenu.menu)
                                .unwrap_or(saved.active_menu);
                            let highlighted_item = saved
                                .submenu
                                .as_ref()
                                .map(|submenu| submenu.highlighted_item)
                                .unwrap_or(saved.highlighted_item);
                            self.restore_menu_tracking_pixels(bus, saved);
                            self.draw_menu_bar_to_fb(bus);
                            bus.write_long(sp + 4, result);
                            cpu.write_reg(Register::A7, sp + 4);
                            self.record_menuselect_input_trace(
                                "finish",
                                None,
                                Some(active_menu),
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
                        let result = self.menu_tracking_selection_result();
                        if result != 0 {
                            let (active_menu, item_idx) = self
                                .menu_tracking
                                .as_ref()
                                .and_then(|tracking| {
                                    tracking
                                        .submenu
                                        .as_ref()
                                        .filter(|submenu| submenu.highlighted_item > 0)
                                        .map(|submenu| (submenu.menu, submenu.highlighted_item))
                                        .or_else(|| {
                                            (tracking.highlighted_item > 0).then_some((
                                                tracking.active_menu,
                                                tracking.highlighted_item,
                                            ))
                                        })
                                })
                                .unwrap_or((0, 0));
                            // Start flashing: 6 toggles = 3 flashes
                            let tracking = self.menu_tracking.as_mut().unwrap();
                            tracking.flash_remaining = 6;
                            tracking.flash_delay = 3;
                            tracking.flash_result = result;
                            self.record_menuselect_input_trace(
                                "release",
                                None,
                                Some(active_menu),
                                Some(item_idx),
                                Some(result),
                                "start_flash",
                            );
                        } else {
                            // No item selected — return 0 immediately
                            let (sp, active_menu) = self
                                .menu_tracking
                                .as_ref()
                                .map(|tracking| (tracking.stack_ptr, tracking.active_menu))
                                .unwrap();
                            let saved = self.menu_tracking.take().unwrap();
                            self.restore_menu_tracking_pixels(bus, saved);
                            self.draw_menu_bar_to_fb(bus);
                            self.finish_menu_no_hit(bus, cpu, sp, 4);
                            self.record_menuselect_input_trace(
                                "release",
                                None,
                                Some(active_menu),
                                Some(0),
                                Some(0),
                                "no_selection",
                            );
                        }
                    } else {
                        // Button still held — update highlight
                        let (mv, mh) = self.menu_tracking_mouse_pos(bus);

                        // Check if mouse moved to a different menu title
                        let new_menu = self.menu_title_hit_test(mh);
                        let tracking = self.menu_tracking.as_ref().unwrap();
                        let mbar_h =
                            bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
                        if let Some(new_idx) = new_menu {
                            if new_idx != tracking.active_menu && mv < mbar_h {
                                // Switch to different menu
                                let old_saved = self.menu_tracking.take().unwrap();
                                let sp = old_saved.stack_ptr;
                                self.restore_menu_tracking_pixels(bus, old_saved);
                                self.open_menu_dropdown(bus, new_idx, sp);
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

                        let old_trace = self.menu_tracking.as_ref().map(|tracking| {
                            tracking
                                .submenu
                                .as_ref()
                                .map(|submenu| (submenu.menu, submenu.highlighted_item))
                                .unwrap_or((tracking.active_menu, tracking.highlighted_item))
                        });
                        self.update_menu_tracking_for_point(bus, mh, mv);
                        let new_trace = self.menu_tracking.as_ref().map(|tracking| {
                            tracking
                                .submenu
                                .as_ref()
                                .map(|submenu| (submenu.menu, submenu.highlighted_item))
                                .unwrap_or((tracking.active_menu, tracking.highlighted_item))
                        });
                        if new_trace != old_trace {
                            let (active_menu, highlighted_item) = new_trace.unwrap_or((0, 0));
                            self.record_menuselect_input_trace(
                                "tracking_update",
                                None,
                                Some(active_menu),
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
                    if let Some(menu_idx) = self.menu_title_hit_test(pt_h) {
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
                        self.open_menu_dropdown(bus, menu_idx, sp);
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
                if self.menu_tracking.is_some() {
                    // Re-fire: popup tracking is active
                    if self.menu_tracking.as_ref().unwrap().flash_remaining > 0 {
                        let result = self.menu_tracking.as_ref().unwrap().flash_result;
                        let t = self.menu_tracking.as_mut().unwrap();
                        if t.flash_delay > 0 {
                            t.flash_delay -= 1;
                            return Some(Ok(()));
                        }
                        t.flash_remaining -= 1;
                        t.flash_delay = 3;
                        let new_remaining = t.flash_remaining;

                        if new_remaining == 0 {
                            let sp = self.menu_tracking.as_ref().unwrap().stack_ptr;
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
                        let result = self.menu_tracking_selection_result();
                        if result != 0 {
                            let tracking = self.menu_tracking.as_mut().unwrap();
                            tracking.flash_remaining = 6;
                            tracking.flash_delay = 3;
                            tracking.flash_result = result;
                        } else {
                            let sp = self.menu_tracking.as_ref().unwrap().stack_ptr;
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
                    let popup_item = bus.read_word(sp) as i16;
                    let left = bus.read_word(sp + 2) as i16;
                    let top = bus.read_word(sp + 4) as i16;
                    let menu_handle = bus.read_long(sp + 6);
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
                        .position(|m| m.id == menu_id && m.in_menu_bar)
                    {
                        let (dd_rect, highlighted_item) =
                            self.popup_menu_dropdown_rect(bus, menu_idx, top, left, popup_item);

                        self.restore_visible_dialog_snapshots(bus);
                        let saved = self.save_dropdown_pixels(bus, dd_rect);
                        self.menu_tracking = Some(MenuTrackingState {
                            active_menu: menu_idx,
                            highlighted_item: 0,
                            saved_pixels: saved,
                            dropdown_rect: dd_rect,
                            submenu: None,
                            stack_ptr: sp,
                            flash_remaining: 0,
                            flash_delay: 0,
                            flash_result: 0,
                        });
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
            // Inside Macintosh Volume I (1985), pp. I-355..I-356
            //
            // Per IM:I I-356: "HiliteMenu highlights the title of the
            // given menu, or does nothing if the title is already
            // highlighted. Since only one menu title can be highlighted
            // at a time, it unhighlights any previously highlighted
            // menu title. If menuID is 0 (or isn't the ID of any menu
            // in the menu list), HiliteMenu simply unhighlights
            // whichever menu title is highlighted (if any)."
            //
            // Tool-bit Pascal PROCEDURE calling convention: the caller
            // pushes the 2-byte menuID INTEGER, the trap pops it, no
            // FUNCTION result slot is written. MPW Universal Headers
            // Menus.h declares
            //     EXTERN_API(void) HiliteMenu(MenuID menuID)
            //                                ONEWORDINLINE(0xA938);
            //
            // Stack discipline:
            //   - A7 unchanged across the call after the 2-byte menuID
            //     argument is consumed (no FUNCTION result slot, no
            //     other stack frame).
            //
            // Behaviors intentionally not modeled here:
            //   - The visible menu bar redraw. BasiliskII System 7.5.3
            //     ROM unconditionally stamps a menu bar strip with the
            //     named menu's title highlighted (or unhighlighted when
            //     menuID=0). Systemless's HLE skips the
            //     `draw_menu_bar_to_fb` call when `self.menus.is_empty()`
            //     because the host runtime draws the menu bar directly
            //     from the Rust menu list and would otherwise produce a
            //     spare bottom-border line in the no-menu case (a
            //     visible divergence from BII observed in earlier
            //     iterations).
            //   - The active dropdown / menu-tracking state. Systemless
            //     tracks open dropdowns in `self.menu_tracking` and
            //     restores saved pixels via `restore_dropdown_pixels`
            //     when HiliteMenu is called; BII's TheMenu lowmem
            //     global is structurally different.
            (true, 0x138) => {
                let sp = cpu.read_reg(Register::A7);
                let _menu_id = bus.read_word(sp) as i16;
                cpu.write_reg(Register::A7, sp + 2);

                // If there's still a dropdown open, close it
                if let Some(tracking) = self.menu_tracking.take() {
                    self.restore_dropdown_pixels(
                        bus,
                        tracking.dropdown_rect,
                        &tracking.saved_pixels,
                    );
                }
                // Redraw menu bar without any highlight. Skip when the
                // app has no menus installed -- real ROM doesn't stamp
                // a menu-bar strip in that case, so Systemless shouldn't
                // either (the spare bottom-border line was a visible
                // divergence from BasiliskII for HiliteMenu-without-
                // InsertMenu callers).
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
            // MenuKey ($A93E): Searches enabled menus in the current menu list
            // for a matching key equivalent; scan order is right-to-left per
            // IM:I I-355.
            (true, 0x13E) => {
                let sp = cpu.read_reg(Register::A7);
                let ch = (bus.read_word(sp) & 0xFF) as u8;

                // Search enabled items in enabled menus currently in the menu
                // list. IM:I I-355 documents right-to-left menu scan order.
                let mut result: u32 = 0;
                let mut matched_menu_idx: Option<usize> = None;
                let ch_upper = (ch as char).to_ascii_uppercase() as u8;
                self.refresh_menus_from_memory(bus);
                for (menu_idx, menu) in self.menus.iter().enumerate().rev() {
                    if !menu.in_menu_bar || !menu.enabled {
                        continue;
                    }
                    for (i, item) in menu.items.iter().enumerate() {
                        if item.enabled
                            && Self::menu_item_has_command_key(item)
                            && (item.key_equiv as char).to_ascii_uppercase() as u8 == ch_upper
                        {
                            result = ((menu.id as u32) << 16) | ((i + 1) as u32);
                            matched_menu_idx = Some(menu_idx);
                            break;
                        }
                    }
                    if result != 0 {
                        break;
                    }
                }
                // IM:I I-356 says MenuKey highlights the matching menu title
                // and the app later calls HiliteMenu(0) to clear it. The
                // packed LongInt result remains the guest-visible behavior;
                // title pixels are renderer/theme-owned chrome.
                if let Some(menu_idx) = matched_menu_idx {
                    if self.menus[menu_idx].visible_in_menu_bar {
                        self.highlight_menu_title(bus, menu_idx);
                    }
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
            // CheckItem ($A945): Sets/clears checkmark on internal menu item
            (true, 0x145) => {
                let sp = cpu.read_reg(Register::A7);
                let checked = bus.read_byte(sp) != 0;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);

                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                        mi.mark = if checked { 0x12 } else { 0 }; // 0x12 = checkmark char
                    }
                }
                Ok(())
            }

            // SetItem ($A947)
            // Changes the text of a menu item; does not affect other attributes.
            // PROCEDURE SetItem(theMenu: MenuHandle; item: INTEGER; itemString: Str255);
            // Inside Macintosh Volume I, I-357
            // SetItem ($A947): Updates item text in internal menu cache
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
                    let text = macroman_to_string(&text_bytes);
                    if !self.menus.iter().any(|m| m.handle == menu_handle) {
                        let menu_ptr = bus.read_long(menu_handle);
                        if menu_ptr != 0 {
                            self.menus
                                .push(parse_menu_resource(bus, menu_ptr, menu_handle));
                        }
                    }
                    let mut touched: Option<Menu> = None;
                    if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                        refresh_menu_from_memory(bus, menu);
                        let idx = (item - 1) as usize;
                        if idx < menu.items.len() {
                            menu.items[idx].text = text;
                            touched = Some(menu.clone());
                        }
                    }
                    // Keep guest-memory MENU record in sync. CountMItems /
                    // CalcMenuSize read guest memory via
                    // count_menu_items_from_memory.
                    if let Some(m) = touched {
                        self.serialise_menu_items_to_memory(bus, &m);
                    }
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
                self.sync_guest_menu_list(bus);
                if menu_handle != 0 {
                    let menu_ptr = bus.read_long(menu_handle);
                    if menu_ptr != 0 {
                        bus.free(menu_ptr);
                        self.ptr_to_handle.remove(&menu_ptr);
                    }
                    self.forget_resource_handle_index_for_handle(menu_handle);
                    self.loaded_handles.remove(&menu_handle);
                    self.detached_handles.remove(&menu_handle);
                    self.resource_handle_files.remove(&menu_handle);
                    self.detached_handle_files.remove(&menu_handle);
                    self.handle_state_bits.remove(&menu_handle);
                    bus.free(menu_handle);
                }
                Ok(())
            }

            // DeleteMenu ($A936)
            // Removes a menu from the menu bar.
            // PROCEDURE DeleteMenu(menuID: INTEGER);
            // Inside Macintosh Volume I, I-353
            // DeleteMenu ($A936): Removes menu by ID from internal list
            (true, 0x136) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_id = bus.read_word(sp) as i16;
                cpu.write_reg(Register::A7, sp + 2);
                self.menus.retain(|m| m.id != menu_id);
                self.sync_guest_menu_list(bus);
                // IM:V 1986 p. V-244: DeleteMenu removes all color
                // entries for the deleted menu ID from MenuCInfo.
                self.filter_menu_color_table_entries(bus, |id, _item| id != menu_id);
                Ok(())
            }

            // CountMItems ($A950)
            // Returns the number of items in the specified menu.
            // FUNCTION CountMItems(theMenu: MenuHandle): INTEGER;
            // Inside Macintosh Volume IV, IV-56
            // CountMItems ($A950): Counts items from MENU data in guest memory
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
            // SetItemIcon ($A940): Stores icon byte in menu item per IM:I I-359
            (true, 0x140) => {
                let sp = cpu.read_reg(Register::A7);
                let icon = (bus.read_word(sp) & 0xFF) as u8;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                let touched = if let Some(menu) =
                    self.menus.iter_mut().find(|m| m.handle == menu_handle)
                {
                    refresh_menu_from_memory(bus, menu);
                    if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                        mi.icon = icon;
                        Some(menu.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(menu) = touched {
                    self.serialise_menu_items_to_memory(bus, &menu);
                }
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
            // SetItemStyle ($A942): Stores style byte in menu item per IM:I I-359
            (true, 0x142) => {
                let sp = cpu.read_reg(Register::A7);
                let style = (bus.read_word(sp) & 0xFF) as u8;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                let touched = if let Some(menu) =
                    self.menus.iter_mut().find(|m| m.handle == menu_handle)
                {
                    refresh_menu_from_memory(bus, menu);
                    if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                        mi.style = style;
                        Some(menu.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(menu) = touched {
                    self.serialise_menu_items_to_memory(bus, &menu);
                }
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
            // SetItemMark ($A944): Sets mark on internal menu item
            (true, 0x144) => {
                let sp = cpu.read_reg(Register::A7);
                let mark_char = (bus.read_word(sp) & 0xFF) as u8;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                let touched = if let Some(menu) =
                    self.menus.iter_mut().find(|m| m.handle == menu_handle)
                {
                    refresh_menu_from_memory(bus, menu);
                    if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                        mi.mark = mark_char;
                        Some(menu.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(menu) = touched {
                    self.serialise_menu_items_to_memory(bus, &menu);
                }
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
            //
            // CalcMenuSize ($A948): Computes menuWidth/menuHeight and writes to MENU record per IM:I I-361
            (true, 0x148) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);

                if let Some(menu) = self.menus.iter().find(|m| m.handle == menu_handle) {
                    let menu_height = self.menu_items_height(bus, &menu.items) + 2;

                    let mut max_width: i16 = 0;
                    for item in &menu.items {
                        let w = Self::fb_measure_string(&item.text, 0, 12);
                        let total = w + self.menu_item_width_extra(bus, item) + 24;
                        if total > max_width {
                            max_width = total;
                        }
                    }
                    max_width = max_width.max(100);

                    if menu_handle != 0 {
                        let menu_ptr = bus.read_long(menu_handle);
                        if menu_ptr != 0 {
                            bus.write_word(menu_ptr + 2, max_width as u16);
                            bus.write_word(menu_ptr + 4, menu_height as u16);
                        }
                    }
                }
                Ok(())
            }

            // SetMenuFlash ($A94A)
            // Sets the number of times a selected menu item blinks.
            // PROCEDURE SetMenuFlash(count: INTEGER);
            // Inside Macintosh Volume I, I-361
            //
            // SetMenuFlash ($A94A): Writes count to MenuFlash global ($0A24) per IM:I I-361
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

                let port = self.current_port;
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
            // Visual side effect: on the System 7.5.3 screen,
            // FlashMenuBar(0) inverts the current top menu-bar strip
            // once, so a second call blinks it back.
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
            // Items are sorted alphabetically by name (IM:IV IV-56) via
            // named_resources_of_type; same `.`/`%` skip rules and
            // no-duplicate contract as AddResMenu.
            //
            // Stack: SP+0 afterItem (2), SP+2 theType (4), SP+6 theMenu handle (4). Pop 10.
            // InsertResMenu ($A951): Walks the current resource search order, inserts named resources of theType after `afterItem` (skip names starting with '.' or '%'), sorted alphabetically per IM:IV IV-56
            (true, 0x151) => {
                let sp = cpu.read_reg(Register::A7);
                let after_item = bus.read_word(sp) as i16;
                let res_type_word = bus.read_long(sp + 2);
                let menu_handle = bus.read_long(sp + 6);
                cpu.write_reg(Register::A7, sp + 10);

                let res_type = res_type_word.to_be_bytes();
                let entries = self.named_resources_of_type(res_type);

                let mut touched: Option<Menu> = None;
                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    refresh_menu_from_memory(bus, menu);
                    let base = (after_item.max(0) as usize).min(menu.items.len());
                    let mut offset = 0usize;
                    for (_id, name) in entries {
                        if name.is_empty() || name.starts_with('.') || name.starts_with('%') {
                            continue;
                        }
                        if menu.items.iter().any(|it| it.text == name) {
                            continue;
                        }
                        menu.items.insert(
                            base + offset,
                            MenuItem {
                                text: name,
                                icon: 0,
                                key_equiv: 0,
                                mark: 0,
                                style: 0,
                                enabled: true,
                            },
                        );
                        offset += 1;
                    }
                    sync_enable_flags(bus, menu);
                    touched = Some(menu.clone());
                }
                if let Some(m) = touched {
                    self.serialise_menu_items_to_memory(bus, &m);
                }
                Ok(())
            }

            // DeleteMenuItem ($A952)
            // PROCEDURE DeleteMenuItem(theMenu: MenuHandle; item: INTEGER);
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-127
            // Stack: SP+0: item(2), SP+2: theMenu(4). Pop 6.
            // DeleteMenuItem ($A952): item=0 or item>last is no-op per IM:TB 1992 p.3-127.
            (true, 0x152) => {
                let sp = cpu.read_reg(Register::A7);
                let item = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);
                let mut touched: Option<Menu> = None;
                let mut deleted_color_key: Option<(i16, i16)> = None;
                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    refresh_menu_from_memory(bus, menu);
                    let idx = (item - 1) as usize;
                    if idx < menu.items.len() {
                        menu.items.remove(idx);
                        deleted_color_key = Some((menu.id, item));
                        touched = Some(menu.clone());
                    }
                }
                // Keep guest-memory MENU record in sync with the deletion
                // so CountMItems / CalcMenuSize don't still see the item.
                if let Some(m) = touched {
                    self.serialise_menu_items_to_memory(bus, &m);
                    sync_enable_flags(bus, &m);
                }
                if let Some((menu_id, menu_item)) = deleted_color_key {
                    // IM:V 1986 p. V-244: DelMenuItem removes the
                    // deleted item's color entry from MenuCInfo.
                    self.filter_menu_color_table_entries(bus, |id, item_no| {
                        !(id == menu_id && item_no == menu_item)
                    });
                }
                Ok(())
            }

            // InsertMenuItem ($A826)
            // Inserts one or more menu items after the specified item position.
            // PROCEDURE InsertMenuItem(theMenu: MenuHandle; itemString: Str255; afterItem: INTEGER);
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-126
            // Stack: SP+0: afterItem(2), SP+2: itemString(4), SP+6: theMenu(4). Pop 10.
            // InsertMenuItem accepts the same metacharacter format as AppendMenu.
            // For multiple `itemString` entries, inserted items appear in reverse
            // order relative to the string (MTE 1992, p. 3-126).
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
                let parsed = parse_appendmenu_items(&bytes);

                let mut touched: Option<Menu> = None;
                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    refresh_menu_from_memory(bus, menu);
                    if !parsed.is_empty() {
                        let insert_idx = if after_item <= 0 {
                            0usize
                        } else {
                            (after_item as usize).min(menu.items.len())
                        };
                        // Re-inserting each parsed item at the same insertion
                        // index produces the documented reverse-order result.
                        for item in parsed {
                            menu.items.insert(insert_idx, item);
                        }
                        touched = Some(menu.clone());
                    }
                }
                // Keep guest-memory MENU record in sync so CountMItems
                // reflects the insertion.
                if let Some(m) = touched {
                    sync_enable_flags(bus, &m);
                    self.serialise_menu_items_to_memory(bus, &m);
                }
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
            // Apple-vs-BasiliskII divergence on the side effect:
            // BasiliskII System 7.5.3 ROM Menu Manager sets the
            // documented menu-bar-invalid flag honored by GetNextEvent.
            // Systemless HLE is a true no-op (`Ok(())`) because the host
            // runtime redraws the entire chrome per frame from the
            // current menu list, so there is no separate "dirty" flag
            // to honor. The visible-side-effect "menu bar gets
            // redrawn" path is intentionally not modeled and reserved
            // for in-Rust state inspection.
            //
            // Regression coverage:
            //   src/trap/menu.rs::invalmenubar_procedure_call_preserves_stack_pointer
            (true, 0x01D) => Ok(()),

            // SetItemCmd ($A84F)
            // Sets the keyboard command equivalent of a menu item, or
            // attaches a hierarchical submenu by passing CHR(27) = $1B
            // per IM:V V-244 ("SetItemCmd allows the application to
            // attach a submenu to a menu by passing the character
            // $1B"). The submenu's resID lives in markChar (set via
            // SetItemMark $A944 separately).
            // PROCEDURE SetItemCmd(theMenu: MenuHandle; item: INTEGER;
            //                      cmdChar: CHAR);
            // Inside Macintosh Volume V, V-244
            //
            // Stack: SP+0 cmdChar (2 — Pascal CHAR pushes value in low
            // byte of the word, mirrors SetItemMark $A944 + SetItemIcon
            // $A940 conventions), SP+2 item (2 — 1-based per IM:I I-356),
            // SP+4 theMenu (4). Pop 8 bytes.
            //
            // Mutates `menu.items[item-1].key_equiv` for the matching
            // MenuHandle AND re-serialises the entire MENU data block
            // back to guest memory at offset attr_base+1 of each item
            // per IM:I I-345 menuData layout — so GetItemCmd ($A84E),
            // CountMItems ($A938), CalcMenuSize ($A948), and any other
            // trap that walks the on-disk MENU record sees the updated
            // key_equiv byte. NIL theMenu, item < 1, or item > items.len
            // is a defensive no-op (matches SetItemIcon / SetItemMark /
            // SetItemStyle behaviour for OOB indices). Until this
            // iteration the impl was a 2-line pop-only stub with the
            // trap-doc Notes reading "cmdChar not stored" — apps using
            // SetItemCmd to install ⌘ shortcuts on dynamically-built
            // menus (typical "Customize..." dialog or runtime-localised
            // command keys) saw their installs silently discarded; apps
            // attaching submenus via the IM:V V-244 $1B convention
            // (hierarchical Apple menu / Window menu trees) saw the
            // submenu link silently lost. The previous test
            // `setitemcmd_sets_command_key` was an anti-test asserting
            // only pop=8 (passing against the buggy stub by design).
            //
            // SetItemCmd ($A84F): Stores cmdChar in menu.items[item-1].key_equiv + re-serialises to guest MENU data per IM:I I-345 so GetItemCmd ($A84E) and any guest-memory walker reads back the value; NIL handle / OOB item is defensive no-op; cmdChar=$1B per IM:V V-244 signals submenu attach (submenu ID lives in markChar via SetItemMark)
            (true, 0x04F) => {
                let sp = cpu.read_reg(Register::A7);
                let cmd_char = (bus.read_word(sp) & 0xFF) as u8;
                let item = bus.read_word(sp + 2) as i16;
                let menu_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);
                if menu_handle == 0 || item < 1 {
                    return Some(Ok(()));
                }
                let menu_clone =
                    if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                        refresh_menu_from_memory(bus, menu);
                        if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                            mi.key_equiv = cmd_char;
                        } else {
                            return Some(Ok(()));
                        }
                        menu.clone()
                    } else {
                        return Some(Ok(()));
                    };
                self.serialise_menu_items_to_memory(bus, &menu_clone);
                Ok(())
            }

            // GetMenuBar ($A93B)
            // Snapshots the current menu bar so the caller can later
            // restore it with SetMenuBar.
            // FUNCTION GetMenuBar: Handle;
            // Inside Macintosh Volume I, I-354
            //
            // Per IM:I I-354 the returned handle holds a copy of the
            // current menu list — a length-prefixed sequence of menu
            // handles, NOT the menu records themselves. We mirror that
            // shape in guest memory (count word + N×4-byte menu handle)
            // so an introspecting caller sees a sensible block, AND we
            // also store a Rust-side `Vec<Menu>` snapshot keyed by the
            // returned handle so SetMenuBar can faithfully restore the
            // parsed item state even if the menu was DeleteMenu'd in
            // the meantime (the typical modal-dialog disable+restore
            // pattern).
            //
            // No parameters. Returns Handle in the pre-pushed result
            // slot at SP+0; A7 is unchanged.
            // GetMenuBar ($A93B): Snapshots `self.menus` into `saved_menu_bars` and writes a count+handles block keyed by the returned handle per IM:I I-354
            (true, 0x13B) => {
                let sp = cpu.read_reg(Register::A7);
                self.refresh_menus_from_memory(bus);
                let count = self.menus.len() as u32;
                // Allocate at least 2 bytes even when the menu bar is empty
                // so the handle's master pointer is non-NIL (matches real
                // Mac, where GetMenuBar always returns a valid handle).
                let block = bus.alloc(2 + 4 * count.max(1));
                bus.write_word(block, count as u16);
                for (i, menu) in self.menus.iter().enumerate() {
                    bus.write_long(block + 2 + (i as u32) * 4, menu.handle);
                }
                let handle = bus.alloc(4);
                bus.write_long(handle, block);
                self.saved_menu_bars.insert(handle, self.menus.clone());
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
            //   * MenuChoice ($AA66) — reads lowmem MenuDisable ($0B54)
            //     and writes that LongInt to the result slot.
            //     Per MTb 1992 3-118..3-119 + IM:V V-248, when MenuSelect
            //     or MenuKey return zero the Menu Manager surfaces the
            //     packed (menuID, itemNumber) last tracked into MenuDisable.
            //     Systemless's HLE does not synthesize the MDEF cursor-tracking
            //     writes, so tests seed MenuDisable directly to exercise
            //     the lowmem read path explicitly.

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
            // the current longword and returns it. Systemless's HLE does not
            // synthesize the MDEF cursor-tracking writes, so tests seed the
            // lowmem global directly to exercise the read path.
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
            //   * MenuChoice returns the caller-seeded MenuDisable value.
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
        item.key_equiv == 0x1B && item.mark != 0
    }

    fn menu_item_has_command_key(item: &MenuItem) -> bool {
        item.key_equiv > 0x20
    }

    fn menu_item_uses_reduced_icon(item: &MenuItem) -> bool {
        item.icon != 0 && item.key_equiv == MENU_KEY_REDUCED_ICON
    }

    fn menu_item_uses_small_icon(item: &MenuItem) -> bool {
        item.icon != 0 && item.key_equiv == MENU_KEY_SMALL_ICON
    }

    fn menu_item_uses_normal_icon(item: &MenuItem) -> bool {
        item.icon != 0 && (item.key_equiv == 0 || item.key_equiv > 0x20)
    }

    fn menu_cicn_layout(bus: &MacMemoryBus, icon_ptr: u32) -> Option<MenuCIconLayout> {
        if bus.get_alloc_size(icon_ptr).is_some_and(|size| size < 82) {
            return None;
        }

        // Imaging With QuickDraw 1994 p. 4-106: a compiled 'cicn'
        // resource starts with a 50-byte PixMap, 14-byte mask BitMap,
        // 14-byte 1-bit fallback BitMap, 4-byte iconData handle, then
        // mask bits, fallback bitmap bits, ColorTable, and PixMap data.
        let pm_row_bytes = (bus.read_word(icon_ptr + 4) & 0x3FFF) as u32;
        let pm_top = bus.read_word(icon_ptr + 6) as i16;
        let pm_left = bus.read_word(icon_ptr + 8) as i16;
        let pm_bottom = bus.read_word(icon_ptr + 10) as i16;
        let pm_right = bus.read_word(icon_ptr + 12) as i16;
        let pixel_size = bus.read_word(icon_ptr + 32);
        let mask_row_bytes = (bus.read_word(icon_ptr + 54) & 0x3FFF) as u32;
        let bmap_row_bytes = (bus.read_word(icon_ptr + 68) & 0x3FFF) as u32;

        let width = pm_right - pm_left;
        let height = pm_bottom - pm_top;
        if width <= 0 || height <= 0 || pm_row_bytes == 0 || mask_row_bytes == 0 {
            return None;
        }

        let height_u32 = height as u32;
        let mask_data_size = mask_row_bytes.checked_mul(height_u32)?;
        let bmap_data_size = bmap_row_bytes.checked_mul(height_u32)?;
        let mask_data_ptr = icon_ptr + 82;
        let bmap_data_ptr = mask_data_ptr.checked_add(mask_data_size)?;
        let ctab_ptr = bmap_data_ptr.checked_add(bmap_data_size)?;

        if let Some(resource_size) = bus.get_alloc_size(icon_ptr) {
            let resource_end = icon_ptr.checked_add(resource_size)?;
            if ctab_ptr.checked_add(8)? > resource_end {
                return None;
            }
        }

        let ct_size = bus.read_word(ctab_ptr + 6) as u32;
        let ctab_total_bytes = 8u32.checked_add((ct_size + 1).checked_mul(8)?)?;
        let pixel_data_ptr = ctab_ptr.checked_add(ctab_total_bytes)?;

        if let Some(resource_size) = bus.get_alloc_size(icon_ptr) {
            let resource_end = icon_ptr.checked_add(resource_size)?;
            let pixel_data_size = pm_row_bytes.checked_mul(height_u32)?;
            if pixel_data_ptr.checked_add(pixel_data_size)? > resource_end {
                return None;
            }
        }

        Some(MenuCIconLayout {
            width,
            height,
            pm_row_bytes,
            mask_row_bytes,
            bmap_row_bytes,
            pixel_size,
            mask_data_ptr,
            bmap_data_ptr,
            pixel_data_ptr,
        })
    }

    fn menu_cicn_pixel_index(
        bus: &MacMemoryBus,
        data_ptr: u32,
        row_bytes: u32,
        pixel_size: u16,
        x: u32,
        y: u32,
    ) -> Option<u8> {
        let row_ptr = data_ptr.checked_add(y.checked_mul(row_bytes)?)?;
        match pixel_size {
            1 => {
                let byte = bus.read_byte(row_ptr.checked_add(x / 8)?);
                Some(((byte >> (7 - (x % 8))) & 1) as u8)
            }
            2 => {
                let byte = bus.read_byte(row_ptr.checked_add(x / 4)?);
                Some((byte >> (6 - 2 * (x % 4))) & 0x03)
            }
            4 => {
                let byte = bus.read_byte(row_ptr.checked_add(x / 2)?);
                Some(if x % 2 == 0 {
                    (byte >> 4) & 0x0F
                } else {
                    byte & 0x0F
                })
            }
            8 => Some(bus.read_byte(row_ptr.checked_add(x)?)),
            size if size > 8 && size % 8 == 0 => {
                let bytes_per_pixel = u32::from(size / 8);
                Some(bus.read_byte(row_ptr.checked_add(x.checked_mul(bytes_per_pixel)?)?))
            }
            _ => None,
        }
    }

    fn menu_item_cicn_size(&self, bus: &MacMemoryBus, item: &MenuItem) -> Option<(i16, i16)> {
        let icon_ptr = self.cicn_menu_icon_resource_ptr(item)?;
        let layout = Self::menu_cicn_layout(bus, icon_ptr)?;
        Some((layout.width, layout.height))
    }

    pub(super) fn menu_item_height(&self, bus: &MacMemoryBus, item: &MenuItem) -> i16 {
        // MTE 1992 p. 3-46: 'cicn' has priority over ICON/SICN and
        // enlarges the menu item according to the icon's resource rect.
        // Normal ICON items reserve a 32-by-32 slot; System 7.5.3's
        // standard MDEF uses a 34-pixel row around that slot. Reduced ICON
        // and SICN slots fit the standard 16-by-16 item height.
        let base_height = if let Some((_width, height)) = self.menu_item_cicn_size(bus, item) {
            height.max(MENU_ROW_HEIGHT)
        } else if Self::menu_item_uses_normal_icon(item) {
            MENU_NORMAL_ICON_ROW_HEIGHT
        } else {
            MENU_ROW_HEIGHT
        };
        if (item.style & MENU_TEXT_STYLE_SHADOW) != 0 {
            base_height.max(MENU_SHADOW_STYLE_ROW_HEIGHT)
        } else {
            base_height
        }
    }

    pub(super) fn menu_items_height(&self, bus: &MacMemoryBus, items: &[MenuItem]) -> i16 {
        Self::laid_out_items(items)
            .iter()
            .map(|item| self.menu_item_height(bus, item))
            .sum()
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
        let mut end = items.len();
        while end > 0 && items[end - 1].text == "-" {
            end -= 1;
        }
        &items[..end]
    }

    pub(super) fn menu_item_width_extra(&self, bus: &MacMemoryBus, item: &MenuItem) -> i16 {
        let key_extra = if Self::menu_item_has_command_key(item) {
            30
        } else {
            0
        };
        let is_hierarchical = Self::is_hierarchical_item(item);
        let mark_extra = if item.mark != 0 && !is_hierarchical {
            14
        } else {
            0
        };
        let hierarchy_extra = if is_hierarchical { 20 } else { 0 };
        let icon_extra = if let Some((width, _height)) = self.menu_item_cicn_size(bus, item) {
            width.max(MENU_ROW_HEIGHT)
        } else if Self::menu_item_uses_reduced_icon(item) || Self::menu_item_uses_small_icon(item) {
            MENU_ROW_HEIGHT
        } else if Self::menu_item_uses_normal_icon(item) {
            MENU_NORMAL_ICON_SIZE
        } else {
            0
        };
        key_extra + mark_extra + hierarchy_extra + icon_extra
    }

    fn menu_item_pulldown_padding(item: &MenuItem) -> i16 {
        let has_key = Self::menu_item_has_command_key(item);
        let has_icon = item.icon != 0;
        // IM:I I-358 and MTE 1992 pp. 3-115 to 3-117: the mark sits in the
        // fixed inset left of the item text that every item already
        // reserves, so a marked item is exactly as wide as the same item
        // unmarked — System 7.5.3 sizes Absolute Solitaire's Game menu from
        // "Beleaguered Castle", not from the checked "Klondike (Common)"
        // beside it. `menu_item_width_extra` still reports the mark column
        // for CalcMenuSize's menuWidth, so cancel that allowance here.
        let is_hierarchical = Self::is_hierarchical_item(item);
        let mark_allowance = if item.mark != 0 && !is_hierarchical {
            14
        } else {
            0
        };
        if Self::menu_item_uses_normal_icon(item) {
            6
        } else if has_icon || is_hierarchical {
            14
        } else if has_key {
            20 - mark_allowance
        } else {
            26 - mark_allowance
        }
    }

    fn menu_item_icon_resource_id(item: &MenuItem) -> Option<i16> {
        (item.icon != 0).then_some(i16::from(item.icon) + 256)
    }

    fn menu_item_has_cicn_resource(&self, item: &MenuItem) -> bool {
        self.cicn_menu_icon_resource_ptr(item).is_some()
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
        let Some(icon_resource_id) = Self::menu_item_icon_resource_id(item) else {
            return None;
        };
        self.find_loaded_resource_any(*b"cicn", icon_resource_id)
            .map(|(_, ptr)| ptr)
    }

    fn reduced_menu_icon_resource_ptr(&self, item: &MenuItem) -> Option<u32> {
        if !Self::menu_item_uses_reduced_icon(item) || self.menu_item_has_cicn_resource(item) {
            return None;
        }

        // IM:I I-359 says the item's icon byte is an icon number; MTE
        // 1992 pp. 3-137 to 3-138 specify the Menu Manager adds 256
        // to obtain the ICON resource ID for the reduced-icon case.
        let icon_resource_id = Self::menu_item_icon_resource_id(item)?;
        self.find_loaded_resource_any(*b"ICON", icon_resource_id)
            .map(|(_, ptr)| ptr)
    }

    fn small_menu_icon_resource_ptr(&self, item: &MenuItem) -> Option<u32> {
        if !Self::menu_item_uses_small_icon(item) || self.menu_item_has_cicn_resource(item) {
            return None;
        }

        // MTE 1992 p. 3-46: when the key-equivalent byte is $1E and no
        // cicn is used, the Menu Manager looks for an SICN resource at
        // icon-number+256 and plots it in a 16-by-16 rectangle.
        let icon_resource_id = Self::menu_item_icon_resource_id(item)?;
        self.find_loaded_resource_any(*b"SICN", icon_resource_id)
            .map(|(_, ptr)| ptr)
    }

    fn normal_menu_icon_resource_ptr(&self, item: &MenuItem) -> Option<u32> {
        if !Self::menu_item_uses_normal_icon(item) || self.menu_item_has_cicn_resource(item) {
            return None;
        }

        // MTE 1992 p. 3-46: a normal menu icon uses the icon number plus
        // 256 as an ICON or cicn resource ID; this path implements the
        // monochrome ICON case and leaves cicn-specific drawing open.
        let icon_resource_id = Self::menu_item_icon_resource_id(item)?;
        self.find_loaded_resource_any(*b"ICON", icon_resource_id)
            .map(|(_, ptr)| ptr)
    }

    fn draw_menu_icon_bitmap(
        &self,
        bus: &mut MacMemoryBus,
        icon_ptr: u32,
        top: i16,
        left: i16,
        dst_size: i16,
        pixel_index_override: Option<u8>,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let dst_size = i32::from(dst_size);

        // IM:V 1986 p. V-233 and MTE 1992 p. 3-99: black-and-white
        // menu icons are drawn in the item name color. The caller passes
        // the resolved MenuCInfo name color when an 8bpp table entry exists.
        // Reduced ICONs use a 16x16 slot; normal menu ICONs use 32x32.
        // Shrinks OR-compress source pixels, matching PlotIcon and IM:V
        // V-65 CopyBits scaling behavior. The 32x32 case is a direct copy.
        for dy in 0..dst_size {
            let sy_start = (dy * 32 / dst_size) as u32;
            let sy_end = (((dy + 1) * 32 / dst_size).min(32) as u32).max(sy_start + 1);
            for dx in 0..dst_size {
                let sx_start = (dx * 32 / dst_size) as u32;
                let sx_end = (((dx + 1) * 32 / dst_size).min(32) as u32).max(sx_start + 1);
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
                if any_set {
                    if let Some(pixel_index) = pixel_index_override {
                        Self::fb_set_pixel_index(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            left + dx as i16,
                            top + dy as i16,
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
                            left + dx as i16,
                            top + dy as i16,
                            true,
                        );
                    }
                }
            }
        }
    }

    fn draw_sicn_menu_icon(
        &self,
        bus: &mut MacMemoryBus,
        icon_ptr: u32,
        top: i16,
        left: i16,
        pixel_index_override: Option<u8>,
    ) {
        if bus.get_alloc_size(icon_ptr).is_some_and(|size| size < 32) {
            return;
        }

        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();

        // More Macintosh Toolbox 1993 glossary, "small icon resource":
        // an SICN resource is a list of 16-by-16 black-and-white bitmaps;
        // by convention the first bitmap is image data and the second is
        // a mask. Menu Manager $1E items plot the small icon in a 16x16
        // menu slot per MTE 1992 p. 3-46.
        for dy in 0..MENU_ROW_HEIGHT {
            let row_data = bus.read_word(icon_ptr + u32::from(dy as u16) * 2);
            for dx in 0..MENU_ROW_HEIGHT {
                if (row_data >> (15 - dx)) & 1 != 0 {
                    if let Some(pixel_index) = pixel_index_override {
                        Self::fb_set_pixel_index(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            left + dx,
                            top + dy,
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
                            left + dx,
                            top + dy,
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

        for dy in 0..layout.height {
            for dx in 0..layout.width {
                let sx = dx as u32;
                let sy = dy as u32;
                let mask_byte =
                    bus.read_byte(layout.mask_data_ptr + sy * layout.mask_row_bytes + sx / 8);
                if (mask_byte & (0x80 >> (sx % 8))) == 0 {
                    continue;
                }

                if screen_pixel_size == 8 && layout.pixel_size >= 2 {
                    let Some(pixel_index) = Self::menu_cicn_pixel_index(
                        bus,
                        layout.pixel_data_ptr,
                        layout.pm_row_bytes,
                        layout.pixel_size,
                        sx,
                        sy,
                    ) else {
                        continue;
                    };
                    let dst_x = left + dx;
                    let dst_y = top + dy;
                    if dst_x >= 0 && dst_y >= 0 && dst_x < screen_width && dst_y < screen_height {
                        bus.write_byte(
                            screen_base + (dst_y as u32) * row_bytes + dst_x as u32,
                            pixel_index,
                        );
                    }
                    continue;
                }

                let source_data_ptr = if layout.bmap_row_bytes != 0 {
                    layout.bmap_data_ptr
                } else {
                    layout.pixel_data_ptr
                };
                let source_row_bytes = if layout.bmap_row_bytes != 0 {
                    layout.bmap_row_bytes
                } else {
                    layout.pm_row_bytes
                };
                let source_pixel_size = if layout.bmap_row_bytes != 0 {
                    1
                } else {
                    layout.pixel_size
                };
                let Some(pixel_index) = Self::menu_cicn_pixel_index(
                    bus,
                    source_data_ptr,
                    source_row_bytes,
                    source_pixel_size,
                    sx,
                    sy,
                ) else {
                    continue;
                };
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    screen_pixel_size,
                    screen_width,
                    screen_height,
                    left + dx,
                    top + dy,
                    pixel_index != 0,
                );
            }
        }
    }

    fn popup_menu_dropdown_rect(
        &self,
        bus: &MacMemoryBus,
        menu_idx: usize,
        top: i16,
        left: i16,
        popup_item: i16,
    ) -> ((i16, i16, i16, i16), i16) {
        let (_screen_base, _row_bytes, screen_width, screen_height, _pixel_size) =
            self.get_screen_params();
        let menu = &self.menus[menu_idx];

        let mut width: i16 = 0;
        for item in &menu.items {
            let w = Self::fb_measure_string(&item.text, 0, 12)
                + self.menu_item_width_extra(bus, item)
                + 26;
            width = width.max(w);
        }
        width = width.max(1);
        let height = (self.menu_items_height(bus, &menu.items) + 2).max(1);

        let highlighted_item = if popup_item >= 1 && (popup_item as usize) <= menu.items.len() {
            popup_item
        } else {
            0
        };
        let item_offset = if highlighted_item > 0 {
            menu.items
                .iter()
                .take((highlighted_item - 1) as usize)
                .map(|item| self.menu_item_height(bus, item))
                .sum::<i16>()
        } else {
            0
        };

        // MTE 1992 p. 3-120 and IM:V V-241 define Top/Left as global
        // coordinates used to display the requested PopUpItem at the
        // pop-up box location; the app owns title/mark/control state.
        let desired_top = if highlighted_item > 0 {
            top - 1 - item_offset
        } else {
            top
        };
        // System 7.5.3's standard popup MDEF extends the live popup menu one
        // pixel left of the caller's pop-up box Left coordinate while still
        // aligning the requested item at Top. MTE 1992, p. 3-120.
        let desired_left = left - 1;

        let clamped_left = if screen_width <= 0 {
            desired_left
        } else if width >= screen_width {
            0
        } else {
            desired_left.clamp(0, screen_width - width)
        };
        let clamped_top = if screen_height <= 0 {
            desired_top
        } else if height >= screen_height {
            0
        } else {
            desired_top.clamp(0, screen_height - height)
        };
        let right = if screen_width > 0 {
            (clamped_left + width).min(screen_width)
        } else {
            clamped_left + width
        };
        let bottom = if screen_height > 0 {
            (clamped_top + height).min(screen_height)
        } else {
            clamped_top + height
        };

        ((clamped_top, clamped_left, bottom, right), highlighted_item)
    }

    fn dropdown_width_for_menu(&self, bus: &MacMemoryBus, menu_idx: usize, min_width: i16) -> i16 {
        let Some(menu) = self.menus.get(menu_idx) else {
            return min_width;
        };
        let mut max_width = min_width;
        for item in &menu.items {
            let w = Self::fb_measure_string(&item.text, 0, 12);
            let total =
                w + self.menu_item_width_extra(bus, item) + Self::menu_item_pulldown_padding(item);
            max_width = max_width.max(total);
        }
        max_width
    }

    /// Open a menu dropdown and start tracking.
    fn open_menu_dropdown(&mut self, bus: &mut MacMemoryBus, menu_idx: usize, stack_ptr: u32) {
        // Fault in this menu's icon resources while `self` is still
        // mutable; the draw path below reads them through `&self` only.
        self.preload_menu_item_icon_resources(bus, menu_idx);
        let (_screen_base, _row_bytes, screen_width, screen_height, _pixel_size) =
            self.get_screen_params();

        // Compute dropdown rect
        let region = self
            .menu_title_regions_with_indices()
            .into_iter()
            .find(|(idx, _, _)| *idx == menu_idx);
        if menu_idx >= self.menus.len() {
            return;
        }
        let Some((_idx, title_left, title_right)) = region else {
            return;
        };
        let menu = &self.menus[menu_idx];

        let dropdown_top: i16 = 20; // Below menu bar
                                    // The standard pull-down menu extends slightly to the left of the
                                    // highlighted title frame. This is visible in the System 7.5.3
                                    // MenuSelect reference and follows the Menu Manager's MDEF-owned menu
                                    // rectangle rather than the app-visible title hit region.
                                    // Inside Macintosh Volume I, I-356; Macintosh Toolbox Essentials
                                    // 1992, pp. 3-115 to 3-117.
        let dropdown_left: i16 = title_left - 2;

        let max_width = self.dropdown_width_for_menu(bus, menu_idx, title_right - title_left + 20);

        let dropdown_bottom =
            (dropdown_top + self.menu_items_height(bus, &menu.items) + 1).min(screen_height);
        let dropdown_right = (dropdown_left + max_width).min(screen_width);
        let dropdown_rect = (dropdown_top, dropdown_left, dropdown_bottom, dropdown_right);

        // Save pixels under dropdown
        let saved = self.save_dropdown_pixels(bus, dropdown_rect);

        // Draw dropdown
        self.draw_menu_dropdown(bus, menu_idx, dropdown_rect);

        // Highlight the menu title in the menu bar
        self.highlight_menu_title(bus, menu_idx);

        self.menu_tracking = Some(MenuTrackingState {
            active_menu: menu_idx,
            highlighted_item: 0,
            saved_pixels: saved,
            dropdown_rect,
            submenu: None,
            stack_ptr,
            flash_remaining: 0,
            flash_delay: 0,
            flash_result: 0,
        });
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
        if let Some(submenu) = saved.submenu {
            self.restore_dropdown_pixels(bus, submenu.dropdown_rect, &submenu.saved_pixels);
        }
        self.restore_dropdown_pixels(bus, saved.dropdown_rect, &saved.saved_pixels);
    }

    fn menu_tracking_selection_result(&self) -> u32 {
        let Some(tracking) = self.menu_tracking.as_ref() else {
            return 0;
        };
        if let Some(submenu) = tracking.submenu.as_ref() {
            if submenu.highlighted_item > 0 {
                let menu = &self.menus[submenu.menu];
                return ((menu.id as u32) << 16) | (submenu.highlighted_item as u32 & 0xFFFF);
            }
        }
        if tracking.highlighted_item <= 0 {
            return 0;
        }

        let menu = &self.menus[tracking.active_menu];
        let item_idx = tracking.highlighted_item as usize - 1;
        let Some(item) = menu.items.get(item_idx) else {
            return 0;
        };
        if Self::is_hierarchical_item(item) {
            return 0;
        }
        ((menu.id as u32) << 16) | (tracking.highlighted_item as u32 & 0xFFFF)
    }

    fn submenu_menu_index_for_parent_item(&self, parent_item: i16) -> Option<usize> {
        let tracking = self.menu_tracking.as_ref()?;
        if parent_item <= 0 {
            return None;
        }
        let parent_menu = self.menus.get(tracking.active_menu)?;
        let item = parent_menu.items.get(parent_item as usize - 1)?;
        if !Self::is_hierarchical_item(item) {
            return None;
        }
        let submenu_id = item.mark as i16;
        self.menus.iter().position(|m| m.id == submenu_id)
    }

    fn submenu_rect_for_parent_item(
        &self,
        bus: &MacMemoryBus,
        submenu_idx: usize,
        parent_item: i16,
    ) -> Option<(i16, i16, i16, i16)> {
        let tracking = self.menu_tracking.as_ref()?;
        let (_screen_base, _row_bytes, screen_width, screen_height, _pixel_size) =
            self.get_screen_params();
        let (parent_top, parent_left, _parent_bottom, parent_right) = tracking.dropdown_rect;
        let parent_menu = self.menus.get(tracking.active_menu)?;
        let parent_offset = parent_menu
            .items
            .iter()
            .take((parent_item - 1).max(0) as usize)
            .map(|item| self.menu_item_height(bus, item))
            .sum::<i16>();
        let submenu_top = parent_top + 1 + parent_offset;
        let submenu_height = self.menu_items_height(bus, &self.menus.get(submenu_idx)?.items) + 2;
        let submenu_width = self.dropdown_width_for_menu(bus, submenu_idx, 100);

        let mut submenu_left = parent_right - 1;
        let mut submenu_right = submenu_left + submenu_width;
        if submenu_right > screen_width {
            submenu_right = parent_left + 1;
            submenu_left = (submenu_right - submenu_width).max(0);
        }

        let submenu_bottom = (submenu_top + submenu_height).min(screen_height);
        Some((submenu_top, submenu_left, submenu_bottom, submenu_right))
    }

    fn close_submenu(&mut self, bus: &mut MacMemoryBus) {
        let submenu = self
            .menu_tracking
            .as_mut()
            .and_then(|tracking| tracking.submenu.take());
        if let Some(submenu) = submenu {
            self.restore_dropdown_pixels(bus, submenu.dropdown_rect, &submenu.saved_pixels);
        }
    }

    fn ensure_submenu_for_parent_item(&mut self, bus: &mut MacMemoryBus, parent_item: i16) {
        let Some(submenu_idx) = self.submenu_menu_index_for_parent_item(parent_item) else {
            self.close_submenu(bus);
            return;
        };
        let already_open = self
            .menu_tracking
            .as_ref()
            .and_then(|tracking| tracking.submenu.as_ref())
            .is_some_and(|submenu| {
                submenu.menu == submenu_idx && submenu.parent_item == parent_item
            });
        if already_open {
            return;
        }

        let Some(dropdown_rect) = self.submenu_rect_for_parent_item(bus, submenu_idx, parent_item)
        else {
            self.close_submenu(bus);
            return;
        };
        self.close_submenu(bus);
        let saved_pixels = self.save_dropdown_pixels(bus, dropdown_rect);
        self.draw_menu_dropdown(bus, submenu_idx, dropdown_rect);
        if let Some(tracking) = self.menu_tracking.as_mut() {
            tracking.submenu = Some(SubmenuTrackingState {
                menu: submenu_idx,
                parent_item,
                highlighted_item: 0,
                saved_pixels,
                dropdown_rect,
            });
        }
    }

    fn submenu_item_at_point(&self, mouse_x: i16, mouse_y: i16) -> Option<i16> {
        let tracking = self.menu_tracking.as_ref()?;
        let submenu = tracking.submenu.as_ref()?;
        let (top, left, bottom, right) = submenu.dropdown_rect;
        if mouse_x < left || mouse_x >= right || mouse_y < top || mouse_y >= bottom {
            return None;
        }
        let item_height: i16 = 16;
        let item_idx = (mouse_y - top - 1) / item_height;
        let menu = self.menus.get(submenu.menu)?;
        if item_idx < 0 || item_idx as usize >= menu.items.len() {
            return Some(0);
        }
        let item = &menu.items[item_idx as usize];
        if item.text == "-" || !item.enabled {
            return Some(0);
        }
        Some(item_idx + 1)
    }

    fn update_submenu_highlight(&mut self, bus: &mut MacMemoryBus, new_item: i16) {
        let Some((menu_idx, old_item, rect)) = self
            .menu_tracking
            .as_ref()
            .and_then(|tracking| tracking.submenu.as_ref())
            .map(|submenu| {
                (
                    submenu.menu,
                    submenu.highlighted_item,
                    submenu.dropdown_rect,
                )
            })
        else {
            return;
        };
        if old_item == new_item {
            return;
        }
        let classic_highlight = self.ui_theme_id() == UiThemeId::ClassicSystem7;
        if classic_highlight && old_item > 0 {
            self.invert_dropdown_item_rect(bus, menu_idx, rect, old_item);
        }
        if let Some(submenu) = self
            .menu_tracking
            .as_mut()
            .and_then(|tracking| tracking.submenu.as_mut())
        {
            submenu.highlighted_item = new_item;
        }
        if classic_highlight && new_item > 0 {
            self.invert_dropdown_item_rect(bus, menu_idx, rect, new_item);
        } else if !classic_highlight {
            self.draw_menu_dropdown(bus, menu_idx, rect);
        }
    }

    fn update_parent_menu_highlight(&mut self, bus: &mut MacMemoryBus, new_item: i16) {
        let Some(old_item) = self
            .menu_tracking
            .as_ref()
            .map(|tracking| tracking.highlighted_item)
        else {
            return;
        };

        if old_item != new_item {
            self.close_submenu(bus);
            let classic_highlight = self.ui_theme_id() == UiThemeId::ClassicSystem7;
            if classic_highlight && old_item > 0 {
                self.invert_menu_item(bus, old_item);
            }
            if let Some(tracking) = self.menu_tracking.as_mut() {
                tracking.highlighted_item = new_item;
            }
            if classic_highlight && new_item > 0 {
                self.invert_menu_item(bus, new_item);
            } else if !classic_highlight {
                let Some((active_menu, dropdown_rect)) = self
                    .menu_tracking
                    .as_ref()
                    .map(|tracking| (tracking.active_menu, tracking.dropdown_rect))
                else {
                    return;
                };
                self.draw_menu_dropdown(bus, active_menu, dropdown_rect);
            }
        }

        if new_item > 0 {
            self.ensure_submenu_for_parent_item(bus, new_item);
        } else {
            self.close_submenu(bus);
        }
    }

    fn update_menu_tracking_for_point(
        &mut self,
        bus: &mut MacMemoryBus,
        mouse_x: i16,
        mouse_y: i16,
    ) {
        if let Some(submenu_item) = self.submenu_item_at_point(mouse_x, mouse_y) {
            self.update_submenu_highlight(bus, submenu_item);
            return;
        }
        let new_item = self.dropdown_item_at_point(bus, mouse_x, mouse_y);
        self.update_parent_menu_highlight(bus, new_item);
    }

    /// Determine which menu title the x coordinate falls on.
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
    fn menu_title_regions_with_indices(&self) -> Vec<(usize, i16, i16)> {
        let mut regions = Vec::new();
        let mut x: i16 = 18;
        for (menu_idx, menu) in self.menus.iter().enumerate() {
            if !menu.visible_in_menu_bar {
                continue;
            }
            let width = Self::menu_title_advance(&menu.title);
            let left = x - 7; // padding before title
            let right = x + width + 6; // padding after title
            regions.push((menu_idx, left, right));
            x += width + 13;
        }
        regions
    }

    /// Determine which item (1-based) is at the given screen point, or 0.
    fn dropdown_item_at_point(&self, bus: &MacMemoryBus, mouse_x: i16, mouse_y: i16) -> i16 {
        if let Some(ref tracking) = self.menu_tracking {
            let (top, left, bottom, right) = tracking.dropdown_rect;
            if mouse_x >= left && mouse_x < right && mouse_y >= top && mouse_y < bottom {
                let menu = &self.menus[tracking.active_menu];
                let mut item_top = top + 1;
                for (item_idx, item) in menu.items.iter().enumerate() {
                    let item_bottom = item_top + self.menu_item_height(bus, item);
                    if mouse_y >= item_top && mouse_y < item_bottom {
                        // Don't highlight separators or disabled items
                        if item.text == "-" || !item.enabled {
                            return 0;
                        }
                        return item_idx as i16 + 1; // 1-based
                    }
                    item_top = item_bottom;
                }
            }
        }
        0
    }

    /// Draw the menu dropdown box with items.
    pub(super) fn draw_menu_dropdown(
        &self,
        bus: &mut MacMemoryBus,
        menu_idx: usize,
        rect: (i16, i16, i16, i16),
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (top, left, bottom, right) = rect;
        if menu_idx >= self.menus.len() {
            return;
        }
        let menu = &self.menus[menu_idx];
        let dropdown_bg_index =
            Self::menu_dropdown_background_pixel_index(bus, menu.id, pixel_size);
        let detached_popup = self.menu_tracking.as_ref().is_some_and(|tracking| {
            tracking.active_menu == menu_idx
                && tracking.dropdown_rect == rect
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

            if !attached_pulldown {
                Self::fb_hline(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    top,
                    left,
                    right,
                    true,
                );
            }
            Self::fb_hline(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                bottom - 1,
                left,
                right,
                true,
            );
            for y in top..bottom {
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left,
                    y,
                    true,
                );
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    right - 1,
                    y,
                    true,
                );
            }

            // Shadow (right edge + bottom edge). Detached popup menus start
            // the right-edge shadow one pixel lower than attached pull-downs
            // in the System 7.5.3 MDEF reference. MTE 1992, p. 3-120.
            let shadow_top = if detached_popup { top + 3 } else { top + 2 };
            for y in shadow_top..=bottom {
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    right,
                    y,
                    true,
                );
            }
            Self::fb_hline(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                bottom,
                left + 3,
                right + 1,
                true,
            );
        }

        // Draw items
        let font_id: i16 = 0;
        let font_size: i16 = 12;
        let metrics = crate::quickdraw::text::get_font_metrics(font_id, font_size);
        let highlighted_item = self
            .menu_tracking
            .as_ref()
            .and_then(|tracking| {
                if tracking.active_menu == menu_idx && tracking.dropdown_rect == rect {
                    Some(tracking.highlighted_item)
                } else {
                    tracking
                        .submenu
                        .as_ref()
                        .filter(|submenu| {
                            submenu.menu == menu_idx && submenu.dropdown_rect == rect
                        })
                        .map(|submenu| submenu.highlighted_item)
                }
            })
            .or_else(|| {
                self.control_tracking
                    .as_ref()
                    .filter(|tracking| {
                        tracking.active_menu == menu_idx && tracking.dropdown_rect == rect
                    })
                    .map(|tracking| tracking.highlighted_item)
            })
            .or_else(|| {
                self.dialog_tracking
                    .as_ref()
                    .and_then(|tracking| tracking.active_popup.as_ref())
                    .filter(|popup| {
                        popup.active_menu == menu_idx && popup.dropdown_rect == rect
                    })
                    .map(|popup| popup.highlighted_item)
            })
            .unwrap_or(0);

        let mut item_top = top + 1;
        for (i, item) in Self::laid_out_items(&menu.items).iter().enumerate() {
            let item_no = i as i16 + 1;
            let item_height = self.menu_item_height(bus, item);
            let item_bottom = item_top + item_height;
            let is_separator = item.text == "-";
            let mark_pixel_index =
                Self::menu_item_component_pixel_index(bus, menu.id, item_no, pixel_size, 4);
            let name_pixel_index =
                Self::menu_item_component_pixel_index(bus, menu.id, item_no, pixel_size, 10);
            let command_pixel_index =
                Self::menu_item_component_pixel_index(bus, menu.id, item_no, pixel_size, 16);
            let cicn_icon_ptr = self.cicn_menu_icon_resource_ptr(item);
            let reduced_icon_ptr = self.reduced_menu_icon_resource_ptr(item);
            let small_icon_ptr = self.small_menu_icon_resource_ptr(item);
            let normal_icon_ptr = self.normal_menu_icon_resource_ptr(item);
            let has_app_icon_resource = cicn_icon_ptr.is_some()
                || reduced_icon_ptr.is_some()
                || small_icon_ptr.is_some()
                || normal_icon_ptr.is_some();
            let has_command_key = Self::menu_item_has_command_key(item);
            // MTE 1992, 3-12: standard menu items can carry an icon,
            // mark, command-key equivalent, text style, and dimmed state.
            let provider_row_chrome = self.draw_theme_menu_item_chrome(
                bus,
                item_top,
                left + 1,
                item_bottom,
                right - 1,
                item.enabled,
                highlighted_item == i as i16 + 1,
                is_separator,
                item.icon != 0 && !has_app_icon_resource,
                item.mark != 0,
                has_command_key,
            );
            let text_baseline_adjust = if attached_pulldown { -1 } else { 0 };
            let text_y = item_top
                + (item_height - (metrics.ascent + metrics.descent)) / 2
                + metrics.ascent
                + text_baseline_adjust;
            // IM:I I-358 / MTE 1992 p. 3-131: DisableItem dims an item and
            // takes it out of MenuSelect and MenuKey, and HIG 1992 p. 54 says
            // it stays visible while dimmed. Separator rows are dimmed the
            // same way because IM:I I-353 keeps hyphen items disabled. On a
            // colour screen the definition procedure resolves the dim shade
            // through GetGray (IM:V 1986 p. V-142); where the device has no
            // intermediate shade it knocks the drawn glyphs back with the 50%
            // grey pattern instead.
            let dim_row = !item.enabled || is_separator;
            let dim_index = if dim_row {
                Self::menu_dim_pixel_index(bus, pixel_size, name_pixel_index, dropdown_bg_index)
            } else {
                None
            };
            let dim_with_pattern = dim_row && dim_index.is_none();
            let content_index = |component_index: Option<u8>| {
                if dim_row {
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
                let sep_y = item_top + item_height / 2 - 1;
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
                        // is on. Imaging With QuickDraw 1994 p. 3-9.
                        None => {
                            if (x + sep_y) % 2 == 0 {
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
                item_top = item_bottom;
                continue;
            }

            let is_hierarchical = Self::is_hierarchical_item(item);

            // Draw mark character if present (0x12 = checkmark, others rendered as-is).
            // Inside Macintosh Volume I, I-358
            let mut text_left = left + 15;
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
                        left + 3,
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
                        left + 3,
                        text_y,
                        &mark_str,
                        font_id,
                        font_size,
                    );
                }
            }

            if let Some(icon_ptr) = cicn_icon_ptr {
                let icon_left = if item.mark != 0 { left + 18 } else { left + 2 };
                self.draw_cicn_menu_icon(bus, icon_ptr, item_top, icon_left);
                let icon_width = self
                    .menu_item_cicn_size(bus, item)
                    .map(|(width, _height)| width.max(MENU_ROW_HEIGHT))
                    .unwrap_or(MENU_ROW_HEIGHT);
                text_left = icon_left + icon_width;
            } else if let Some(icon_ptr) = normal_icon_ptr {
                let icon_left = if item.mark != 0 { left + 18 } else { left + 2 };
                self.draw_menu_icon_bitmap(
                    bus,
                    icon_ptr,
                    item_top,
                    icon_left,
                    MENU_NORMAL_ICON_SIZE,
                    name_pixel_index,
                );
                text_left = left + MENU_NORMAL_ICON_TEXT_LEFT_OFFSET;
            } else if let Some(icon_ptr) = reduced_icon_ptr {
                let icon_left = if item.mark != 0 { left + 18 } else { left + 2 };
                self.draw_menu_icon_bitmap(
                    bus,
                    icon_ptr,
                    item_top,
                    icon_left,
                    MENU_ROW_HEIGHT,
                    name_pixel_index,
                );
                text_left = icon_left + MENU_ROW_HEIGHT;
            } else if let Some(icon_ptr) = small_icon_ptr {
                let icon_left = if item.mark != 0 { left + 18 } else { left + 2 };
                self.draw_sicn_menu_icon(bus, icon_ptr, item_top, icon_left, name_pixel_index);
                text_left = icon_left + MENU_ROW_HEIGHT;
            } else if Self::menu_item_uses_normal_icon(item) {
                // MTE 1992 pp. 3-137 to 3-138 says SetItemIcon stores an
                // icon number and the Menu Manager looks up icon+256. If no
                // resource is present, the System 7 standard MDEF still
                // reserves the normal icon column before drawing item text.
                text_left = left + MENU_NORMAL_ICON_TEXT_LEFT_OFFSET;
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
                let tri_mid_y = item_top + item_height / 2;
                for dx in 0..7 {
                    let x = right - 12 + dx;
                    let half_height = dx.min(6 - dx);
                    for dy in -half_height..=half_height {
                        match content_index(None) {
                            Some(pixel_index) => Self::fb_set_pixel_index(
                                bus,
                                screen_base,
                                row_bytes,
                                pixel_size,
                                screen_width,
                                screen_height,
                                x,
                                tri_mid_y + dy,
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
                                tri_mid_y + dy,
                                true,
                            ),
                        }
                    }
                }
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
                let command_left = right - 25;
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
            // pixels. IM:V 1986 p. V-142; Imaging With QuickDraw 1994 p. 3-9.
            if dim_with_pattern && !provider_row_chrome {
                for y in item_top..item_bottom {
                    for x in (left + 1)..(right - 1) {
                        if (x + y) % 2 != 0 {
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

            item_top = item_bottom;
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
    ) -> Vec<u8> {
        let (screen_base, row_bytes, _, screen_h, pixel_size) = self.get_screen_params();
        let (top, left, bottom, right) = rect;
        // Include shadow area (+1 right, +1 bottom)
        let save_bottom = bottom + 1;
        let save_right = right + 1;
        let mut saved = Vec::new();
        let screen_h_i16 = screen_h;
        for y in top..save_bottom {
            if y < 0 || y >= screen_h_i16 {
                continue;
            }
            let row_start = screen_base + (y as u32) * row_bytes;
            let (byte_left, byte_right) = if pixel_size == 1 {
                // 1bpp: pixels packed 8 per byte
                (
                    (left.max(0) as u32) / 8,
                    (save_right.max(0) as u32).div_ceil(8),
                )
            } else {
                // 8bpp: each pixel is one byte
                (left.max(0) as u32, save_right.max(0) as u32)
            };
            let bx_end = byte_right.min(row_bytes);
            for bx in byte_left..bx_end {
                saved.push(bus.read_byte(row_start + bx));
            }
        }
        saved
    }

    /// Restore previously saved framebuffer pixels.
    /// Mirrors save_dropdown_pixels off-screen guard.
    pub(super) fn restore_dropdown_pixels(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        saved: &[u8],
    ) {
        let (screen_base, row_bytes, _, screen_h, pixel_size) = self.get_screen_params();
        let (top, left, bottom, right) = rect;
        let save_bottom = bottom + 1;
        let save_right = right + 1;
        let (byte_left, byte_right) = if pixel_size == 1 {
            (
                (left.max(0) as u32) / 8,
                (save_right.max(0) as u32).div_ceil(8),
            )
        } else {
            (left.max(0) as u32, save_right.max(0) as u32)
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
                    bus.write_byte(row_start + bx, saved[idx]);
                    idx += 1;
                }
            }
        }
    }

    /// Invert a menu item row in the dropdown (for highlighting).
    pub(super) fn invert_dropdown_item_rect(
        &self,
        bus: &mut MacMemoryBus,
        menu_idx: usize,
        rect: (i16, i16, i16, i16),
        item: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (top, left, _bottom, right) = rect;
        if item < 1 || menu_idx >= self.menus.len() {
            return;
        }
        let menu = &self.menus[menu_idx];
        let mut item_top = top + 1;
        for prior in menu.items.iter().take((item - 1) as usize) {
            item_top += self.menu_item_height(bus, prior);
        }
        let Some(target_item) = menu.items.get((item - 1) as usize) else {
            return;
        };
        let item_bottom = item_top + self.menu_item_height(bus, target_item);
        let background_index = Self::menu_dropdown_background_pixel_index(bus, menu.id, pixel_size);
        let hilite_indexes = background_index.and_then(|background| {
            self.menu_hilite_pixel_indexes(bus, Some(background), pixel_size)
        });
        // Invert pixels in the item row (inside the border).
        for y in item_top..item_bottom {
            for x in (left + 1)..(right - 1) {
                if x >= 0 && x < screen_width && y >= 0 && y < screen_height {
                    if pixel_size == 1 {
                        let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
                        let bit = 7 - (x as u32 % 8);
                        let addr = screen_base + byte_offset;
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, b ^ (1 << bit));
                    } else if let Some((background, hilite)) = hilite_indexes {
                        let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, Self::menu_hilited_pixel_index(b, background, hilite));
                    } else {
                        let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, Self::menu_plain_hilited_pixel_index(bus, b));
                    }
                }
            }
        }
    }

    /// Invert a menu item row in the dropdown (for highlighting).
    pub(super) fn invert_menu_item(&self, bus: &mut MacMemoryBus, item: i16) {
        if let Some(ref tracking) = self.menu_tracking {
            self.invert_dropdown_item_rect(bus, tracking.active_menu, tracking.dropdown_rect, item);
        }
    }

    fn invert_menu_bar_rect(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
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
                } else if pixel_size == 8 {
                    let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                    let b = bus.read_byte(addr);
                    bus.write_byte(addr, Self::menu_plain_hilited_pixel_index(bus, b));
                }
            }
        }
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
        if !matches!(pixel_size, 1 | 8) {
            return;
        }

        if menu_id != 0 {
            if let Some((_, left, right)) = self
                .menu_title_regions_with_indices()
                .into_iter()
                .find(|(idx, _, _)| self.menus.get(*idx).is_some_and(|menu| menu.id == menu_id))
            {
                self.invert_menu_bar_rect(bus, 1, left - 2, menu_bar_height - 1, right + 3);
                return;
            }
        }

        self.invert_menu_bar_rect(bus, 0, 0, menu_bar_height, screen_width);
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

        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            if old_item > 0 {
                self.invert_menu_item(bus, old_item);
            }
            if let Some(tracking) = self.menu_tracking.as_mut() {
                tracking.highlighted_item = item;
            }
            if item > 0 {
                self.invert_menu_item(bus, item);
            }
            return;
        }

        let Some((active_menu, dropdown_rect)) = self.menu_tracking.as_mut().map(|tracking| {
            tracking.highlighted_item = item;
            (tracking.active_menu, tracking.dropdown_rect)
        }) else {
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
        let mut target_region: Option<(i16, i16)> = None;
        for (idx, left, right) in self.menu_title_regions_with_indices() {
            if idx == menu_idx {
                target_region = Some((left, right));
                break;
            }
        }
        let Some((left, right)) = target_region else {
            return;
        };
        if self.ui_theme_id() != UiThemeId::ClassicSystem7 {
            let Some(menu) = self
                .menus
                .get(menu_idx)
                .filter(|menu| menu.visible_in_menu_bar)
            else {
                return;
            };
            let menu_bar_height = bus.read_word(addr::MBAR_HEIGHT) as i16;
            if menu_bar_height <= 1 {
                return;
            }

            // HIG 1992 p. 55 says the title remains highlighted while
            // its menu is open. Non-classic themes own that title-state
            // chrome; the compatibility path redraws only the app title text.
            if self.draw_theme_menu_title_chrome(
                bus,
                1,
                left,
                menu_bar_height - 1,
                right,
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
                    left + 7,
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
        let background_index = self.menu_bar_background_pixel_index(bus, pixel_size);
        let hilite_indexes = background_index.and_then(|background| {
            self.menu_hilite_pixel_indexes(bus, Some(background), pixel_size)
        });
        // Invert the title area in the menu bar. The standard MDEF's
        // highlighted title rectangle begins two pixels before the logical
        // hit region, matching the pull-down rectangle captured by the
        // System 7.5.3 MenuSelect reference. Inside Macintosh Volume I, I-356.
        let classic_left = left - 2;
        let classic_right = right + 3;
        for y in 1i16..19 {
            for x in classic_left..classic_right {
                if x >= 0 && x < screen_width && y >= 0 && y < screen_height {
                    if pixel_size == 1 {
                        let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
                        let bit = 7 - (x as u32 % 8);
                        let addr = screen_base + byte_offset;
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, b ^ (1 << bit));
                    } else if let Some((background, hilite)) = hilite_indexes {
                        let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, Self::menu_hilited_pixel_index(b, background, hilite));
                    } else {
                        let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, Self::menu_plain_hilited_pixel_index(bus, b));
                    }
                }
            }
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
mod tests {
    use super::super::test_helpers::{setup, setup_with_port, MockCpu, TEST_SP};
    use super::{
        count_menu_items_from_memory, parse_appendmenu_items, parse_menu_resource, Menu, MenuItem,
        MenuTrackingState, MC_ENTRY_SIZE, MC_RESOURCE_ENTRY_SIZE, MENU_KEY_REDUCED_ICON,
        MENU_KEY_SMALL_ICON, MENU_ROW_HEIGHT,
    };
    use crate::cpu::{CpuOps, Register};
    use crate::memory::{MacMemoryBus, MemoryBus};
    use crate::ui_theme::UiThemeId;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tracing::span::{Attributes, Record};
    use tracing::subscriber::Interest;
    use tracing::{Event, Id, Level, Metadata, Subscriber};

    struct WarnCounter {
        warnings: Arc<AtomicUsize>,
    }

    impl Subscriber for WarnCounter {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn register_callsite(&self, _metadata: &'static Metadata<'static>) -> Interest {
            Interest::always()
        }

        fn new_span(&self, _attrs: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }

        fn record(&self, _span: &Id, _values: &Record<'_>) {}

        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

        fn event(&self, event: &Event<'_>) {
            if *event.metadata().level() == Level::WARN {
                self.warnings.fetch_add(1, Ordering::Relaxed);
            }
        }

        fn enter(&self, _span: &Id) {}

        fn exit(&self, _span: &Id) {}
    }

    fn write_pstring(bus: &mut crate::memory::MacMemoryBus, ptr: u32, s: &str) {
        let bytes = s.as_bytes();
        bus.write_byte(ptr, bytes.len().min(255) as u8);
        for (i, b) in bytes.iter().take(255).enumerate() {
            bus.write_byte(ptr + 1 + i as u32, *b);
        }
    }

    fn screen_pixel_is_set(bus: &MacMemoryBus, base: u32, row_bytes: u32, x: i16, y: i16) -> bool {
        let byte = bus.read_byte(base + (y as u32 * row_bytes) + ((x as u32) / 8));
        byte & (0x80u8 >> ((x as u8) & 7)) != 0
    }

    fn title_region_pixels(
        bus: &MacMemoryBus,
        base: u32,
        row_bytes: u32,
        left: i16,
        right: i16,
    ) -> Vec<bool> {
        let mut pixels = Vec::new();
        for y in 1i16..19 {
            for x in left..right {
                pixels.push(screen_pixel_is_set(bus, base, row_bytes, x, y));
            }
        }
        pixels
    }

    fn changed_pixel_count(before: &[bool], after: &[bool]) -> usize {
        before
            .iter()
            .zip(after.iter())
            .filter(|(lhs, rhs)| lhs != rhs)
            .count()
    }

    fn clear_1bpp_screen(bus: &mut MacMemoryBus, base: u32, row_bytes: u32, height: u32) {
        for offset in 0..(row_bytes * height) {
            bus.write_byte(base + offset, 0);
        }
    }

    fn screen_pixel_index(bus: &MacMemoryBus, base: u32, row_bytes: u32, x: i16, y: i16) -> u8 {
        bus.read_byte(base + (y as u32 * row_bytes) + x as u32)
    }

    fn clear_8bpp_screen(bus: &mut MacMemoryBus, base: u32, row_bytes: u32, height: u32, fill: u8) {
        for offset in 0..(row_bytes * height) {
            bus.write_byte(base + offset, fill);
        }
    }

    fn setup_8bpp_menu_screen(
        disp: &mut super::super::TrapDispatcher,
        bus: &mut MacMemoryBus,
        width: u16,
        height: u16,
    ) -> (u32, u32) {
        let row_bytes = u32::from(width);
        let base = bus.alloc(row_bytes * u32::from(height));
        disp.set_screen_mode_for_test(base, row_bytes, width, height, 8);
        clear_8bpp_screen(bus, base, row_bytes, u32::from(height), 0xEE);
        bus.write_long(crate::memory::globals::addr::SCRN_BASE, base);

        let gdevice_handle = disp.ensure_main_gdevice(bus);
        bus.write_long(0x08A4, gdevice_handle); // MainDevice
        bus.write_long(0x0CC8, gdevice_handle); // TheGDevice
        (base, row_bytes)
    }

    fn menu_icon_source_with_left_stripe() -> [u8; 128] {
        let mut icon = [0u8; 128];
        for row in 0..32 {
            icon[row * 4] = 0x30;
        }
        icon
    }

    fn sicn_source_with_left_stripe() -> [u8; 64] {
        let mut sicn = [0u8; 64];
        for row in 0..16 {
            sicn[row * 2] = 0x30;
            sicn[32 + row * 2] = 0xFF;
            sicn[32 + row * 2 + 1] = 0xFF;
        }
        sicn
    }

    fn sicn_source_with_right_stripe() -> [u8; 64] {
        let mut sicn = [0u8; 64];
        for row in 0..16 {
            sicn[row * 2 + 1] = 0x03;
            sicn[32 + row * 2] = 0xFF;
            sicn[32 + row * 2 + 1] = 0xFF;
        }
        sicn
    }

    fn write_be_word(data: &mut [u8], offset: usize, value: u16) {
        data[offset] = (value >> 8) as u8;
        data[offset + 1] = value as u8;
    }

    fn cicn_source_with_left_stripe(width: u16, height: u16) -> Vec<u8> {
        let row_bytes = u32::from(width).div_ceil(8);
        let mask_size = row_bytes * u32::from(height);
        let bmap_size = row_bytes * u32::from(height);
        let ctab_size = 16u32;
        let pixel_size = row_bytes * u32::from(height);
        let bmap_offset = 82 + mask_size as usize;
        let ctab_offset = bmap_offset + bmap_size as usize;
        let pixel_offset = ctab_offset + ctab_size as usize;
        let mut data = vec![0u8; pixel_offset + pixel_size as usize];

        write_be_word(&mut data, 4, row_bytes as u16);
        write_be_word(&mut data, 10, height);
        write_be_word(&mut data, 12, width);
        write_be_word(&mut data, 32, 1);

        write_be_word(&mut data, 54, row_bytes as u16);
        write_be_word(&mut data, 60, height);
        write_be_word(&mut data, 62, width);

        write_be_word(&mut data, 68, row_bytes as u16);
        write_be_word(&mut data, 74, height);
        write_be_word(&mut data, 76, width);

        write_be_word(&mut data, ctab_offset + 6, 0);

        for row in 0..usize::from(height) {
            let mask_row = 82 + row * row_bytes as usize;
            let bmap_row = bmap_offset + row * row_bytes as usize;
            let pixel_row = pixel_offset + row * row_bytes as usize;
            for col in 0..row_bytes as usize {
                data[mask_row + col] = 0xFF;
            }
            data[bmap_row] = 0x30;
            data[pixel_row] = 0x30;
        }
        data
    }

    fn seed_menu_resource(bus: &mut crate::memory::MacMemoryBus, menu_id: i16, title: &str) -> u32 {
        let menu_res_ptr = bus.alloc(256);
        bus.write_word(menu_res_ptr, menu_id as u16);
        bus.write_word(menu_res_ptr + 2, 0);
        bus.write_word(menu_res_ptr + 4, 0);
        bus.write_long(menu_res_ptr + 6, 0);
        bus.write_long(menu_res_ptr + 10, 0xFFFF_FFFF);
        write_pstring(bus, menu_res_ptr + 14, title);
        bus.write_byte(menu_res_ptr + 15 + title.len() as u32, 0);
        menu_res_ptr
    }

    fn seed_mbar_resource(bus: &mut crate::memory::MacMemoryBus, menu_ids: &[i16]) -> u32 {
        let mbar_ptr = bus.alloc((2 + 2 * menu_ids.len()) as u32);
        bus.write_word(mbar_ptr, menu_ids.len() as u16);
        for (idx, menu_id) in menu_ids.iter().enumerate() {
            bus.write_word(mbar_ptr + 2 + (idx as u32) * 2, *menu_id as u16);
        }
        mbar_ptr
    }

    fn new_menu_with_title(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_id: i16,
        title_ptr: u32,
        title: &str,
    ) -> u32 {
        write_pstring(bus, title_ptr, title);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, title_ptr);
        bus.write_word(TEST_SP + 4, menu_id as u16);
        assert!(
            disp.dispatch_menu(true, 0x131, cpu, bus).unwrap().is_ok(),
            "NewMenu should succeed"
        );
        bus.read_long(cpu.read_reg(Register::A7))
    }

    fn append_menu_data(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
        data_ptr: u32,
        data: &str,
    ) {
        write_pstring(bus, data_ptr, data);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, data_ptr);
        bus.write_long(TEST_SP + 4, menu_handle);
        assert!(
            disp.dispatch_menu(true, 0x133, cpu, bus).unwrap().is_ok(),
            "AppendMenu should succeed"
        );
    }

    fn set_menu_item_style(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
        item: i16,
        style: u8,
    ) {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, style as u16);
        bus.write_word(TEST_SP + 2, item as u16);
        bus.write_long(TEST_SP + 4, menu_handle);
        assert!(
            disp.dispatch_menu(true, 0x142, cpu, bus).unwrap().is_ok(),
            "SetItemStyle should succeed"
        );
    }

    fn calc_menu_size_for_test(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
    ) -> (i16, i16) {
        let menu_ptr = bus.read_long(menu_handle);
        bus.write_word(menu_ptr + 4, 0);
        bus.write_word(menu_ptr + 6, 0);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, menu_handle);
        assert!(
            disp.dispatch_menu(true, 0x148, cpu, bus).unwrap().is_ok(),
            "CalcMenuSize should succeed"
        );
        (
            bus.read_word(menu_ptr + 2) as i16,
            bus.read_word(menu_ptr + 4) as i16,
        )
    }

    fn write_rgb(bus: &mut crate::memory::MacMemoryBus, ptr: u32, rgb: (u16, u16, u16)) {
        bus.write_word(ptr, rgb.0);
        bus.write_word(ptr + 2, rgb.1);
        bus.write_word(ptr + 4, rgb.2);
    }

    fn write_mc_entry(
        bus: &mut crate::memory::MacMemoryBus,
        ptr: u32,
        menu_id: i16,
        item: i16,
        seed: u16,
    ) {
        bus.write_word(ptr, menu_id as u16);
        bus.write_word(ptr + 2, item as u16);
        write_rgb(
            bus,
            ptr + 4,
            (seed, seed.wrapping_add(1), seed.wrapping_add(2)),
        );
        write_rgb(
            bus,
            ptr + 10,
            (
                seed.wrapping_add(3),
                seed.wrapping_add(4),
                seed.wrapping_add(5),
            ),
        );
        write_rgb(
            bus,
            ptr + 16,
            (
                seed.wrapping_add(6),
                seed.wrapping_add(7),
                seed.wrapping_add(8),
            ),
        );
        write_rgb(
            bus,
            ptr + 22,
            (
                seed.wrapping_add(9),
                seed.wrapping_add(10),
                seed.wrapping_add(11),
            ),
        );
        bus.write_word(ptr + 28, 0);
    }

    fn write_mc_entry_colors(
        bus: &mut crate::memory::MacMemoryBus,
        ptr: u32,
        menu_id: i16,
        item: i16,
        rgb1: (u16, u16, u16),
        rgb2: (u16, u16, u16),
        rgb3: (u16, u16, u16),
        rgb4: (u16, u16, u16),
    ) {
        bus.write_word(ptr, menu_id as u16);
        bus.write_word(ptr + 2, item as u16);
        write_rgb(bus, ptr + 4, rgb1);
        write_rgb(bus, ptr + 10, rgb2);
        write_rgb(bus, ptr + 16, rgb3);
        write_rgb(bus, ptr + 22, rgb4);
        bus.write_word(ptr + 28, 0);
    }

    fn write_rgb_bytes(data: &mut [u8], offset: usize, rgb: (u16, u16, u16)) {
        write_be_word(data, offset, rgb.0);
        write_be_word(data, offset + 2, rgb.1);
        write_be_word(data, offset + 4, rgb.2);
    }

    fn compiled_mctb_resource(entries: &[(i16, i16, u16)]) -> Vec<u8> {
        let mut data = vec![0u8; 2 + entries.len() * MC_RESOURCE_ENTRY_SIZE];
        write_be_word(&mut data, 0, entries.len() as u16);
        for (idx, &(menu_id, item, seed)) in entries.iter().enumerate() {
            let base = 2 + idx * MC_RESOURCE_ENTRY_SIZE;
            write_be_word(&mut data, base, menu_id as u16);
            write_be_word(&mut data, base + 2, item as u16);
            write_rgb_bytes(
                &mut data,
                base + 4,
                (seed, seed.wrapping_add(1), seed.wrapping_add(2)),
            );
            write_rgb_bytes(
                &mut data,
                base + 10,
                (
                    seed.wrapping_add(3),
                    seed.wrapping_add(4),
                    seed.wrapping_add(5),
                ),
            );
            write_rgb_bytes(
                &mut data,
                base + 16,
                (
                    seed.wrapping_add(6),
                    seed.wrapping_add(7),
                    seed.wrapping_add(8),
                ),
            );
            write_rgb_bytes(
                &mut data,
                base + 22,
                (
                    seed.wrapping_add(9),
                    seed.wrapping_add(10),
                    seed.wrapping_add(11),
                ),
            );
        }
        data
    }

    fn install_mctb_resource(
        disp: &mut super::super::TrapDispatcher,
        bus: &mut crate::memory::MacMemoryBus,
        resource_id: i16,
        entries: &[(i16, i16, u16)],
    ) {
        let data = compiled_mctb_resource(entries);
        disp.install_test_resource(bus, *b"mctb", resource_id, &data);
    }

    fn set_mc_entries_for_test(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        entries: &[(i16, i16, u16)],
    ) {
        let ptr = bus.alloc((entries.len() * MC_ENTRY_SIZE) as u32);
        for (idx, &(menu_id, item, seed)) in entries.iter().enumerate() {
            write_mc_entry(bus, ptr + (idx * MC_ENTRY_SIZE) as u32, menu_id, item, seed);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, ptr);
        bus.write_word(TEST_SP + 4, entries.len() as u16);
        assert!(
            disp.dispatch_menu(true, 0x265, cpu, bus).unwrap().is_ok(),
            "SetMCEntries should succeed"
        );
    }

    fn get_mc_entry_ptr_for_test(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_id: i16,
        item: i16,
    ) -> u32 {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, item as u16);
        bus.write_word(TEST_SP + 2, menu_id as u16);
        bus.write_long(TEST_SP + 4, 0xDEADBEEF);
        assert!(
            disp.dispatch_menu(true, 0x264, cpu, bus).unwrap().is_ok(),
            "GetMCEntry should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 4,
            "GetMCEntry should pop only the menuID/menuItem arguments"
        );
        bus.read_long(TEST_SP + 4)
    }

    fn insert_menu_item_data(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
        data_ptr: u32,
        data: &str,
        after_item: i16,
    ) {
        write_pstring(bus, data_ptr, data);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, after_item as u16);
        bus.write_long(TEST_SP + 2, data_ptr);
        bus.write_long(TEST_SP + 6, menu_handle);
        assert!(
            disp.dispatch_menu(true, 0x026, cpu, bus).unwrap().is_ok(),
            "InsertMenuItem should succeed"
        );
    }

    fn insert_menu(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
    ) {
        insert_menu_before_id(disp, cpu, bus, menu_handle, 0);
    }

    fn insert_menu_before(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
        before_id: i16,
    ) {
        insert_menu_before_id(disp, cpu, bus, menu_handle, before_id);
    }

    fn insert_menu_before_id(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
        before_id: i16,
    ) {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, before_id as u16);
        bus.write_long(TEST_SP + 2, menu_handle);
        assert!(
            disp.dispatch_menu(true, 0x135, cpu, bus).unwrap().is_ok(),
            "InsertMenu should succeed"
        );
    }

    fn get_mhandle_for_id(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_id: i16,
    ) -> u32 {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, menu_id as u16);
        assert!(
            disp.dispatch_menu(true, 0x149, cpu, bus).unwrap().is_ok(),
            "GetMHandle should succeed"
        );
        bus.read_long(cpu.read_reg(Register::A7))
    }

    fn delete_menu_by_id(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_id: i16,
    ) {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, menu_id as u16);
        assert!(
            disp.dispatch_menu(true, 0x136, cpu, bus).unwrap().is_ok(),
            "DeleteMenu should succeed"
        );
    }

    fn dispose_menu_by_handle(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
    ) {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, menu_handle);
        assert!(
            disp.dispatch_menu(true, 0x132, cpu, bus).unwrap().is_ok(),
            "DisposeMenu should succeed"
        );
    }

    fn menu_key_result(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        key: u8,
    ) -> u32 {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, key as u16);
        assert!(
            disp.dispatch_menu(true, 0x13E, cpu, bus).unwrap().is_ok(),
            "MenuKey should succeed"
        );
        bus.read_long(cpu.read_reg(Register::A7))
    }

    fn menu_key_result_and_stack(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        key: u8,
    ) -> (u32, u32) {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, key as u16);
        bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);
        assert!(
            disp.dispatch_menu(true, 0x13E, cpu, bus).unwrap().is_ok(),
            "MenuKey should succeed"
        );
        (bus.read_long(TEST_SP + 2), cpu.read_reg(Register::A7))
    }

    fn set_item_cmd(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
        item: i16,
        cmd: u8,
    ) {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, cmd as u16);
        bus.write_word(TEST_SP + 2, item as u16);
        bus.write_long(TEST_SP + 4, menu_handle);
        assert!(
            disp.dispatch_menu(true, 0x04F, cpu, bus).unwrap().is_ok(),
            "SetItemCmd should succeed"
        );
    }

    fn set_item_mark(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
        item: i16,
        mark: u8,
    ) {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, mark as u16);
        bus.write_word(TEST_SP + 2, item as u16);
        bus.write_long(TEST_SP + 4, menu_handle);
        assert!(
            disp.dispatch_menu(true, 0x144, cpu, bus).unwrap().is_ok(),
            "SetItemMark should succeed"
        );
    }

    fn get_item_cmd(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
        item: i16,
    ) -> u8 {
        let out_ptr = 0x306700u32;
        bus.write_word(out_ptr, 0xFFFF);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, out_ptr);
        bus.write_word(TEST_SP + 4, item as u16);
        bus.write_long(TEST_SP + 6, menu_handle);
        assert!(
            disp.dispatch_menu(true, 0x04E, cpu, bus).unwrap().is_ok(),
            "GetItemCmd should succeed"
        );
        (bus.read_word(out_ptr) & 0xFF) as u8
    }

    // IM:I I-353: AddResMenu appends named resources of the requested type
    // as enabled plain items, and skips names beginning with '.' or '%'.
    #[test]
    fn addresmenu_appends_named_resources_as_enabled_plain_items_and_skips_hidden_names() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 330, 0x306800, "Apple");

        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306900, "Existing");

        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::new(),
                    named: std::collections::HashMap::from([
                        ((*b"DRVR", ".HiddenDA".to_string()), (101, 0)),
                        ((*b"DRVR", "%MetaDA".to_string()), (102, 0)),
                        ((*b"DRVR", "Calculator".to_string()), (103, 0)),
                        ((*b"DRVR", "Chooser".to_string()), (104, 0)),
                        ((*b"MENU", "NotDriver".to_string()), (105, 0)),
                    ]),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, u32::from_be_bytes(*b"DRVR"));
        bus.write_long(TEST_SP + 4, handle);
        assert!(
            disp.dispatch_menu(true, 0x14D, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "AddResMenu should succeed"
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        let menu = disp
            .menus
            .iter()
            .find(|m| m.handle == handle)
            .expect("menu should remain registered");
        assert_eq!(menu.items[0].text, "Existing");
        assert_eq!(menu.items.len(), 3, "two visible DRVR names should append");
        assert!(
            menu.items.iter().any(|it| it.text == "Calculator"),
            "visible DRVR names should be appended"
        );
        assert!(
            menu.items.iter().any(|it| it.text == "Chooser"),
            "visible DRVR names should be appended"
        );
        assert!(
            !menu.items.iter().any(|it| it.text == ".HiddenDA"),
            "leading '.' names must be skipped"
        );
        assert!(
            !menu.items.iter().any(|it| it.text == "%MetaDA"),
            "leading '%' names must be skipped"
        );
        for item in menu
            .items
            .iter()
            .filter(|it| it.text == "Calculator" || it.text == "Chooser")
        {
            assert!(item.enabled, "new AddResMenu items should be enabled");
            assert_eq!(item.icon, 0, "new AddResMenu items should have no icon");
            assert_eq!(item.mark, 0, "new AddResMenu items should have no mark");
            assert_eq!(item.style, 0, "new AddResMenu items should be plain style");
        }
    }

    #[test]
    fn addresmenu_exposes_builtin_font_families_without_guest_resources() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 331, 0x306A00, "Font");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, u32::from_be_bytes(*b"FONT"));
        bus.write_long(TEST_SP + 4, handle);
        assert!(
            disp.dispatch_menu(true, 0x14D, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "AddResMenu should succeed"
        );

        let menu = disp
            .menus
            .iter()
            .find(|menu| menu.handle == handle)
            .expect("font menu should remain registered");
        for expected in ["Chicago", "Geneva", "Monaco", "New York", "Palatino"] {
            assert!(
                menu.items.iter().any(|item| item.text == expected),
                "built-in family {expected} should appear in the Font menu"
            );
        }
        assert!(
            !menu.items.iter().any(|item| item.text == "Application"),
            "the applFont selector is not a user-facing family"
        );
    }

    // IM:I I-360: SetItemStyle takes one Style value, one item index,
    // and one MenuHandle argument.
    #[test]
    fn setitemstyle_consumes_menu_item_and_style_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 340, 0x306A10, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306A20, "Open");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x0005);
        bus.write_word(TEST_SP + 2, 1);
        bus.write_long(TEST_SP + 4, handle);
        assert!(
            disp.dispatch_menu(true, 0x142, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetItemStyle should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 8,
            "SetItemStyle should consume style, item, and menu arguments"
        );
    }

    // IM:I I-360: SetItemStyle changes the style of the addressed item.
    #[test]
    fn setitemstyle_updates_target_menu_item_style() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 341, 0x306A30, "Edit");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306A40, "Paste");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x0006);
        bus.write_word(TEST_SP + 2, 1);
        bus.write_long(TEST_SP + 4, handle);
        assert!(
            disp.dispatch_menu(true, 0x142, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetItemStyle should succeed"
        );

        let menu = disp
            .menus
            .iter()
            .find(|m| m.handle == handle)
            .expect("menu should exist");
        assert_eq!(
            menu.items[0].style, 0x06,
            "SetItemStyle should store the requested style byte on the target item"
        );
    }

    // IM:I I-360: GetItemStyle takes VAR chStyle, item, and menu arguments.
    #[test]
    fn getitemstyle_consumes_menu_item_and_stylevar_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 342, 0x306A50, "View");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306A60, "Status");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x0003);
        bus.write_word(TEST_SP + 2, 1);
        bus.write_long(TEST_SP + 4, handle);
        assert!(
            disp.dispatch_menu(true, 0x142, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetItemStyle should succeed"
        );

        let out_ptr = 0x306A70u32;
        bus.write_word(out_ptr, 0xFFFF);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, out_ptr);
        bus.write_word(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 6, handle);
        assert!(
            disp.dispatch_menu(true, 0x141, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "GetItemStyle should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 10,
            "GetItemStyle should consume chStyle pointer, item, and menu arguments"
        );
    }

    // IM:I I-360: GetItemStyle returns the current item style in chStyle.
    #[test]
    fn getitemstyle_writes_current_item_style_to_output_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 343, 0x306A80, "Window");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306A90, "Zoom");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x0009);
        bus.write_word(TEST_SP + 2, 1);
        bus.write_long(TEST_SP + 4, handle);
        assert!(
            disp.dispatch_menu(true, 0x142, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetItemStyle should succeed"
        );

        let out_ptr = 0x306AA0u32;
        bus.write_word(out_ptr, 0xFFFF);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, out_ptr);
        bus.write_word(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 6, handle);
        assert!(
            disp.dispatch_menu(true, 0x141, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "GetItemStyle should succeed"
        );
        assert_eq!(
            bus.read_word(out_ptr),
            0x0009,
            "GetItemStyle should write the current style value to the chStyle output pointer"
        );
    }

    // IM:I I-361: SetMenuFlash takes one INTEGER count parameter.
    #[test]
    fn setmenuflash_consumes_count_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 7);
        assert!(
            disp.dispatch_menu(true, 0x14A, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetMenuFlash should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 2,
            "SetMenuFlash should consume one INTEGER argument"
        );
    }

    // IM:I I-361 assembly note: SetMenuFlash stores the count in MenuFlash.
    #[test]
    fn setmenuflash_writes_count_to_menuflash_global() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::MENU_FLASH, 3);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        assert!(
            disp.dispatch_menu(true, 0x14A, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetMenuFlash should succeed"
        );
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::MENU_FLASH),
            1,
            "SetMenuFlash should update the MenuFlash low-memory word"
        );
    }

    // IM:I I-353: InsertResMenu takes afterItem, resource type, and menu
    // handle arguments.
    #[test]
    fn insertresmenu_consumes_menu_type_and_afteritem_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 344, 0x306AB0, "Apple");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"DRVR"));
        bus.write_long(TEST_SP + 6, handle);
        assert!(
            disp.dispatch_menu(true, 0x151, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "InsertResMenu should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 10,
            "InsertResMenu should consume afterItem, type, and menu arguments"
        );
    }

    // IM:I I-353: InsertResMenu follows afterItem placement and skips names
    // beginning with '.' and '%'.
    #[test]
    fn insertresmenu_inserts_visible_resource_names_at_requested_position() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 345, 0x306AC0, "Apple");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x306AD0,
            "Existing;Tail",
        );

        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::new(),
                    named: std::collections::HashMap::from([
                        ((*b"DRVR", ".HiddenDA".to_string()), (101, 0)),
                        ((*b"DRVR", "%MetaDA".to_string()), (102, 0)),
                        ((*b"DRVR", "Alpha".to_string()), (103, 0)),
                        ((*b"DRVR", "Beta".to_string()), (104, 0)),
                    ]),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"DRVR"));
        bus.write_long(TEST_SP + 6, handle);
        assert!(
            disp.dispatch_menu(true, 0x151, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "InsertResMenu should succeed"
        );

        let menu = disp
            .menus
            .iter()
            .find(|m| m.handle == handle)
            .expect("menu should remain registered");
        let names: Vec<&str> = menu.items.iter().map(|it| it.text.as_str()).collect();
        assert_eq!(
            names,
            vec!["Existing", "Alpha", "Beta", "Tail"],
            "InsertResMenu should insert visible names after afterItem position"
        );
        assert!(
            !menu.items.iter().any(|it| it.text == ".HiddenDA"),
            "InsertResMenu should skip names starting with '.'"
        );
        assert!(
            !menu.items.iter().any(|it| it.text == "%MetaDA"),
            "InsertResMenu should skip names starting with '%'"
        );
    }

    // IM:I I-359: SetItemIcon stores the icon number for the specified item.
    #[test]
    fn setitemicon_sets_requested_icon_number_for_target_item() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 331, 0x306A00, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306B00, "Open");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 7);
        bus.write_word(TEST_SP + 2, 1);
        bus.write_long(TEST_SP + 4, handle);
        assert!(
            disp.dispatch_menu(true, 0x140, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetItemIcon should succeed"
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        let menu = disp
            .menus
            .iter()
            .find(|m| m.handle == handle)
            .expect("menu should exist");
        assert_eq!(menu.items[0].icon, 7, "SetItemIcon should store icon byte");

        let icon_ptr = 0x306C00u32;
        bus.write_word(icon_ptr, 0xFFFF);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, icon_ptr);
        bus.write_word(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 6, handle);
        assert!(
            disp.dispatch_menu(true, 0x13F, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "GetItemIcon should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 10,
            "GetItemIcon should pop 10 bytes from stack"
        );
        assert_eq!(bus.read_word(icon_ptr), 7);
    }

    // IM:I I-360: GetItemIcon returns the item's icon number (1..255),
    // or 0 if no icon is associated with that item.
    #[test]
    fn getitemicon_returns_zero_when_item_has_no_icon() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 332, 0x306D80, "Edit");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306D90, "Paste");

        let icon_ptr = 0x306DA0u32;
        bus.write_word(icon_ptr, 0xFFFF);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, icon_ptr);
        bus.write_word(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 6, handle);
        assert!(
            disp.dispatch_menu(true, 0x13F, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "GetItemIcon should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 10,
            "GetItemIcon should pop one pointer, one item index, and one menu handle"
        );
        assert_eq!(
            bus.read_word(icon_ptr),
            0,
            "GetItemIcon should write 0 when no icon is associated with the item"
        );
    }

    // IM:I I-361: CalcMenuSize recalculates menu dimensions and stores them
    // in the menu record's menuWidth/menuHeight fields.
    #[test]
    fn calcmenusize_writes_recalculated_menuwidth_and_menuheight_fields() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 333, 0x306DB0, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306DC0, "Open");

        let menu_ptr = bus.read_long(handle);
        bus.write_word(menu_ptr + 2, 0);
        bus.write_word(menu_ptr + 4, 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        assert!(
            disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "CalcMenuSize should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 4,
            "CalcMenuSize should pop one MenuHandle argument"
        );

        let width = bus.read_word(menu_ptr + 2) as i16;
        let height = bus.read_word(menu_ptr + 4) as i16;
        assert!(
            width > 0 && height > 0,
            "CalcMenuSize should write nonzero width and height into the menu record"
        );
    }

    #[test]
    fn calcmenusize_recomputes_dimensions_after_menu_contents_change() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 334, 0x306DD0, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306DE0, "Open");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        assert!(
            disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "initial CalcMenuSize should succeed"
        );
        let menu_ptr = bus.read_long(handle);
        let width_before = bus.read_word(menu_ptr + 2) as i16;
        let height_before = bus.read_word(menu_ptr + 4) as i16;

        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x306DF0,
            "This is a much longer menu item label",
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        assert!(
            disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "recalc CalcMenuSize should succeed"
        );
        let width_after = bus.read_word(menu_ptr + 2) as i16;
        let height_after = bus.read_word(menu_ptr + 4) as i16;
        assert!(
            width_after > width_before,
            "adding a longer item should increase menuWidth after CalcMenuSize"
        );
        assert!(
            height_after > height_before,
            "adding an item should increase menuHeight after CalcMenuSize"
        );
    }

    fn calcmenusize_results_for_theme(theme_id: UiThemeId) -> (i16, i16, u16, u32, u16) {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 335, 0x306E00, "View");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x306E40,
            "Open/O;!XMarked;(-;Normal Icon;Color Icon;Small Icon",
        );
        disp.menus[0].items[3].icon = 7;
        disp.install_test_resource(
            &mut bus,
            *b"ICON",
            263,
            &menu_icon_source_with_left_stripe(),
        );
        disp.menus[0].items[4].icon = 8;
        disp.install_test_resource(
            &mut bus,
            *b"cicn",
            264,
            &cicn_source_with_left_stripe(24, 20),
        );
        disp.menus[0].items[5].icon = 9;
        disp.menus[0].items[5].key_equiv = MENU_KEY_SMALL_ICON;
        disp.install_test_resource(&mut bus, *b"SICN", 265, &sicn_source_with_left_stripe());

        let menu_ptr = bus.read_long(handle);
        bus.write_word(menu_ptr + 2, 0);
        bus.write_word(menu_ptr + 4, 0);
        bus.write_word(menu_ptr + 6, 0x1357);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        bus.write_word(TEST_SP + 4, 0xCAFE);

        disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        (
            bus.read_word(menu_ptr + 2) as i16,
            bus.read_word(menu_ptr + 4) as i16,
            bus.read_word(menu_ptr + 6),
            cpu.read_reg(Register::A7),
            bus.read_word(TEST_SP + 4),
        )
    }

    #[test]
    fn systemless_theme_does_not_change_calcmenusize_menu_record_fields() {
        // IM:I I-361: CalcMenuSize recalculates a changed menu and stores the
        // horizontal and vertical dimensions in the menu record. Theme chrome
        // must not alter those guest-visible Menu Manager fields or protocol.
        let classic = calcmenusize_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = calcmenusize_results_for_theme(UiThemeId::SystemlessDefault);

        assert!(
            classic.0 >= 100,
            "classic CalcMenuSize should retain the standard minimum menu width"
        );
        assert_eq!(
            classic.1, 120,
            "classic CalcMenuSize should sum standard, normal ICON, cicn, and SICN row heights"
        );
        assert_eq!(
            classic,
            (classic.0, 120, 0x1357, TEST_SP + 4, 0xCAFE),
            "CalcMenuSize should write menuWidth/menuHeight, preserve adjacent fields, and pop one MenuHandle"
        );
        assert_eq!(
            themed, classic,
            "systemless-default must not change CalcMenuSize menu record geometry or stack protocol"
        );
    }

    // IM:I I-193: PinRect returns the point unchanged when it is already in
    // the rectangle; result packs v in high word and h in low word.
    #[test]
    fn pinrect_inside_point_returns_original_coordinates() {
        let (mut disp, mut cpu, mut bus) = setup();
        let rect_ptr = 0x306D00u32;
        bus.write_word(rect_ptr, 10); // top
        bus.write_word(rect_ptr + 2, 20); // left
        bus.write_word(rect_ptr + 4, 40); // bottom
        bus.write_word(rect_ptr + 6, 80); // right

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 15); // v
        bus.write_word(TEST_SP + 2, 25); // h
        bus.write_long(TEST_SP + 4, rect_ptr);
        bus.write_long(TEST_SP + 8, 0xDEADBEEF);
        assert!(
            disp.dispatch_menu(true, 0x14E, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "PinRect should succeed"
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(TEST_SP + 8), ((15u32) << 16) | 25u32);
    }

    // IM:I I-193: PinRect clamps out-of-bounds coordinates to the nearest
    // interior pixel of the rectangle.
    #[test]
    fn pinrect_outside_point_clamps_to_nearest_interior_pixel() {
        let (mut disp, mut cpu, mut bus) = setup();
        let rect_ptr = 0x306E00u32;
        bus.write_word(rect_ptr, 10); // top
        bus.write_word(rect_ptr + 2, 20); // left
        bus.write_word(rect_ptr + 4, 40); // bottom
        bus.write_word(rect_ptr + 6, 80); // right

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 100); // v (below)
        bus.write_word(TEST_SP + 2, (-5i16) as u16); // h (left)
        bus.write_long(TEST_SP + 4, rect_ptr);
        bus.write_long(TEST_SP + 8, 0xDEADBEEF);
        assert!(
            disp.dispatch_menu(true, 0x14E, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "PinRect should succeed"
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(
            bus.read_long(TEST_SP + 8),
            ((39u32) << 16) | 20u32,
            "v clamps to bottom-1 and h clamps to left"
        );
    }

    // IM:I I-475: DeltaPoint subtracts the coordinates of ptB from those
    // of ptA. The low-order horizontal-difference word must not bleed
    // into the high-order vertical-difference word — each word is an
    // independent signed 16-bit subtraction packed into one LONGINT.
    // Guard against any future "fix" that changes the cast such that
    // dh's negative sign-extension corrupts dv.
    #[test]
    fn deltapoint_negative_delta_packs_signed_words_without_sign_bleed() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A7, TEST_SP);
        // ptB.v = 5, ptB.h = 10
        bus.write_word(TEST_SP, 5);
        bus.write_word(TEST_SP + 2, 10);
        // ptA.v = 0, ptA.h = 0
        bus.write_word(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 6, 0);
        bus.write_long(TEST_SP + 8, 0xDEAD_BEEF);
        assert!(
            disp.dispatch_menu(true, 0x14F, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "DeltaPoint should succeed"
        );
        // dv = 0 - 5 = -5 → high word 0xFFFB
        // dh = 0 - 10 = -10 → low word 0xFFF6
        // result = 0xFFFB_FFF6 (no bleed: high word stays 0xFFFB).
        assert_eq!(bus.read_long(TEST_SP + 8), 0xFFFB_FFF6u32);
    }

    // IM:I I-475: DeltaPoint follows the Pascal FUNCTION calling
    // convention with two 4-byte Point arguments and a 4-byte LONGINT
    // result. The trap pops the 8 argument bytes (A7 += 8) and writes
    // the result at the former SP+8. Guard against future stack-frame
    // changes (8-byte pop must not creep up or down).
    #[test]
    fn deltapoint_consumes_eight_arg_bytes_and_writes_function_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A7, TEST_SP);
        // ptB.v = 30, ptB.h = 25
        bus.write_word(TEST_SP, 30);
        bus.write_word(TEST_SP + 2, 25);
        // ptA.v = 50, ptA.h = 40
        bus.write_word(TEST_SP + 4, 50);
        bus.write_word(TEST_SP + 6, 40);
        // Pre-poison the result slot and a sentinel beyond it to guard
        // against any future "fix" that writes past the LONGINT slot.
        bus.write_long(TEST_SP + 8, 0xDEAD_BEEF);
        bus.write_long(TEST_SP + 12, 0xCAFE_BABE);
        assert!(
            disp.dispatch_menu(true, 0x14F, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "DeltaPoint should succeed"
        );
        // A7 advanced by 8: SP+8 is the new top, which holds the result.
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(TEST_SP + 8), ((20u32) << 16) | 15u32);
        // The sentinel past the result slot is preserved (no over-write).
        assert_eq!(bus.read_long(TEST_SP + 12), 0xCAFE_BABEu32);
    }

    // parse_appendmenu_items handles the IM:I I-358 meta-character set
    // across ';'-separated items.
    #[test]
    fn parse_appendmenu_items_splits_semicolons_and_parses_metachars() {
        let raw = b"New/N;Open/O;(-;Quit/Q";
        let items = parse_appendmenu_items(raw);
        assert_eq!(items.len(), 4, "expected 4 items, got {:?}", items);

        assert_eq!(items[0].text, "New");
        assert_eq!(items[0].key_equiv, b'N');
        assert!(items[0].enabled);

        assert_eq!(items[1].text, "Open");
        assert_eq!(items[1].key_equiv, b'O');
        assert!(items[1].enabled);

        // "(-" — '(' disables the item, leaving "-" as the text
        // (the conventional separator-line marker).
        assert_eq!(items[2].text, "-");
        assert!(!items[2].enabled, "'(' must disable the item");

        assert_eq!(items[3].text, "Quit");
        assert_eq!(items[3].key_equiv, b'Q');
    }

    #[test]
    fn parse_appendmenu_items_handles_marks_icons_and_styles() {
        let raw = b"Bold<B;Italic<I;Marked!\x12;IconItem^\x05";
        let items = parse_appendmenu_items(raw);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].text, "Bold");
        assert_eq!(items[0].style, 0x01);
        assert_eq!(items[1].text, "Italic");
        assert_eq!(items[1].style, 0x02);
        assert_eq!(items[2].text, "Marked");
        assert_eq!(items[2].mark, 0x12);
        assert_eq!(items[3].text, "IconItem");
        assert_eq!(items[3].icon, 4); // icon = char - 1 = 5 - 1
    }

    #[test]
    fn parse_appendmenu_items_handles_single_item_no_separator() {
        let raw = b"Solo";
        let items = parse_appendmenu_items(raw);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "Solo");
    }

    #[test]
    fn popup_menu_item_title_prefers_live_menu_items() {
        let (mut disp, _cpu, bus) = setup();
        disp.menus.push(Menu {
            id: 1008,
            title: "Squadies1".to_string(),
            items: vec![
                MenuItem {
                    text: "Brix".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
                MenuItem {
                    text: "Ryan".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
            ],
            enabled: true,
            handle: 0x1234,
            in_menu_bar: false,
            hierarchical: false,
            visible_in_menu_bar: false,
        });

        assert_eq!(
            disp.popup_menu_item_title(&bus, 1008, 2).as_deref(),
            Some("Ryan")
        );
    }

    // 0x130 — InitMenus: initializes the live MenuCInfo handle if needed.
    #[test]
    fn initmenus_procedure_call_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let result = disp.dispatch_menu(true, 0x130, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitMenus should be handled");
        assert!(result.unwrap().is_ok(), "InitMenus should succeed");
        assert_ne!(
            bus.read_long(crate::memory::globals::addr::MENU_C_INFO),
            0,
            "InitMenus should seed the live MenuCInfo handle"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
    }

    // IM:V 1986 p. V-234 and MTE 1992 p. 3-156: InitMenus attempts to load
    // 'mctb' resource 0 into the application's menu color information table.
    #[test]
    fn initmenus_autoloads_mctb_zero_resource_into_menucinfo() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_mctb_resource(&mut disp, &mut bus, 0, &[(0, 0, 0x1100), (601, 0, 0x1200)]);

        let result = disp.dispatch_menu(true, 0x130, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitMenus should be handled");
        assert!(result.unwrap().is_ok(), "InitMenus should succeed");

        let bar_entry = get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 0, 0);
        assert_ne!(
            bar_entry, 0,
            "InitMenus should autoload the mctb=0 menu-bar entry"
        );
        assert_eq!(
            bus.read_word(bar_entry + 4),
            0x1100,
            "compiled mctb RGB1 should be copied into the live MCEntry"
        );
        assert_eq!(
            bus.read_word(bar_entry + 28),
            0,
            "compiled 28-byte mctb entries should gain a zero reserved word"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 601, 0),
            0,
            "InitMenus should autoload all declared mctb=0 entries"
        );
    }

    // 0x131 — NewMenu: pops 6 bytes, writes handle at SP+6.
    #[test]
    fn test_new_menu() {
        let (mut disp, mut cpu, mut bus) = setup();
        let result = disp.dispatch_menu(true, 0x131, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewMenu should be handled");
        assert!(result.unwrap().is_ok(), "NewMenu should succeed");
        let sp = cpu.read_reg(Register::A7);
        assert_eq!(sp, TEST_SP + 6, "NewMenu should pop 6 bytes from stack");
        let handle = bus.read_long(sp);
        assert_ne!(handle, 0, "NewMenu should write a non-zero handle");
    }

    // IM:I I-352 and MTE 1992 p. 3-105: NewMenu creates a menu record
    // with the requested menu ID and title in menuData.
    #[test]
    fn newmenu_sets_menuid_and_copies_title_into_menu_data() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_id = 230i16;
        let title = "Tools";
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B100, title);
        assert_ne!(handle, 0, "NewMenu should return a non-NIL handle");

        let menu_ptr = bus.read_long(handle);
        assert_ne!(menu_ptr, 0, "NewMenu handle should dereference to MenuInfo");
        assert_eq!(
            bus.read_word(menu_ptr) as i16,
            menu_id,
            "menuID field should match NewMenu(menuID)"
        );
        assert_eq!(
            bus.read_byte(menu_ptr + 14),
            title.len() as u8,
            "menuData title length should match input Str255 length"
        );
        assert_eq!(
            bus.read_bytes(menu_ptr + 15, title.len()),
            title.as_bytes().to_vec(),
            "menuData title bytes should match input Str255 bytes"
        );
        assert_eq!(
            bus.read_byte(menu_ptr + 15 + title.len() as u32),
            0,
            "menuData should terminate items with an empty item string"
        );
    }

    #[test]
    fn newmenu_installs_callable_standard_mdef_handle() {
        // Inside Macintosh Volume I, I-352: NewMenu stores a handle to the
        // standard menu definition procedure in MenuInfo.menuProc.
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 232, 0x30B300, "Window");
        let menu_ptr = bus.read_long(handle);
        let mdef_handle = bus.read_long(menu_ptr + 6);

        assert_ne!(
            mdef_handle, 0,
            "NewMenu must not leave the menuProc field NIL"
        );
        let mdef_ptr = bus.read_long(mdef_handle);
        assert_ne!(mdef_ptr, 0, "standard MDEF handle must be loaded");
        assert_eq!(
            bus.read_word(mdef_ptr),
            0x205F,
            "standard MDEF shim should begin by recovering the JSR return address"
        );
    }

    // MTE 1992 p. 3-105: NewMenu does not insert into the current menu list;
    // InsertMenu is required before GetMHandle can find the menu by ID.
    #[test]
    fn newmenu_requires_insertmenu_before_getmhandle_finds_menu() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_id = 231i16;
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B200, "Tools");

        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
            0,
            "GetMHandle should return NIL before InsertMenu adds NewMenu to current list"
        );

        insert_menu(&mut disp, &mut cpu, &mut bus, handle);
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
            handle,
            "GetMHandle should return the NewMenu handle after InsertMenu"
        );
    }

    // IM:V 1986 pp. V-228–V-230: applications may inspect the MenuList
    // low-memory global directly. Keep its DynamicMenuList records in sync
    // with InsertMenu and DeleteMenu, not just the host-side menu model.
    #[test]
    fn insert_and_delete_menu_sync_the_guest_dynamic_menu_list() {
        let (mut disp, mut cpu, mut bus) = setup();
        assert!(
            disp.dispatch_menu(true, 0x130, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "InitMenus should succeed"
        );

        let menu_list = bus.read_long(crate::memory::globals::addr::MENU_LIST);
        assert_ne!(menu_list, 0, "InitMenus should install a MenuList handle");
        let empty_list = bus.read_long(menu_list);
        assert_ne!(empty_list, 0, "MenuList handle should dereference");
        assert_eq!(bus.read_word(empty_list), 0, "lastMenu should start empty");
        assert_eq!(
            bus.read_word(empty_list + 6),
            0,
            "lastHMenu should start empty"
        );

        let menu_id = 231i16;
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B200, "Home");
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);

        let inserted_list = bus.read_long(menu_list);
        assert_eq!(
            bus.read_word(inserted_list),
            6,
            "one regular MenuRec should occupy six bytes"
        );
        assert_eq!(
            bus.read_long(inserted_list + 6),
            handle,
            "MenuRec should expose the inserted menu handle"
        );
        assert_eq!(
            bus.read_word(inserted_list + 12),
            0,
            "lastHMenu should follow the regular MenuRec"
        );

        delete_menu_by_id(&mut disp, &mut cpu, &mut bus, menu_id);
        let deleted_list = bus.read_long(menu_list);
        assert_eq!(
            bus.read_word(deleted_list),
            0,
            "DeleteMenu should remove the regular MenuRec"
        );
        assert_eq!(
            bus.read_word(deleted_list + 6),
            0,
            "lastHMenu should return to the empty-list offset"
        );
    }

    // IM:I I-352: if the MENU resource can't be read, GetMenu returns NIL.
    #[test]
    fn test_get_menu_returns_nil_when_resource_missing() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(0x0A60, 0);
        bus.write_word(TEST_SP, 999); // menu_id = 999 (missing)
        let result = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetMenu should be handled");
        assert!(result.unwrap().is_ok(), "GetMenu should succeed");
        let sp = cpu.read_reg(Register::A7);
        assert_eq!(sp, TEST_SP + 2, "GetMenu should pop 2 bytes from stack");
        assert_eq!(
            bus.read_long(sp),
            0,
            "GetMenu must return NIL when MENU resource is unavailable"
        );
        assert_eq!(
            bus.read_word(0x0A60) as i16,
            -192,
            "GetMenu miss must set ResErr to resNotFound"
        );
    }

    #[test]
    fn getmenu_hit_clears_stale_reserror() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_ptr = seed_menu_resource(&mut bus, 128, "Game");
        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([((*b"MENU", 128), menu_ptr)]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        bus.write_word(0x0A60, (-192i16) as u16);
        bus.write_word(TEST_SP, 128);

        let result = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetMenu should be handled");
        assert!(result.unwrap().is_ok(), "GetMenu should succeed");

        let sp = cpu.read_reg(Register::A7);
        assert_ne!(bus.read_long(sp), 0, "GetMenu hit should return a handle");
        assert_eq!(
            bus.read_word(0x0A60),
            0,
            "GetMenu hit must clear stale resource errors"
        );
    }

    #[test]
    fn getmenu_replaces_standard_mdef_id_placeholder_with_callable_handle() {
        // Inside Macintosh Volume I, I-127 and I-352: an on-disk MENU stores
        // its MDEF resource ID followed by a zero word. GetMenu replaces that
        // four-byte placeholder with the loaded procedure handle.
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_ptr = seed_menu_resource(&mut bus, 128, "Game");
        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([((*b"MENU", 128), menu_ptr)]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        bus.write_word(TEST_SP, 128);

        disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let menu_handle = bus.read_long(cpu.read_reg(Register::A7));
        let live_menu_ptr = bus.read_long(menu_handle);
        let mdef_handle = bus.read_long(live_menu_ptr + 6);
        assert_ne!(
            mdef_handle, 0,
            "GetMenu must replace the standard MDEF ID placeholder"
        );
        let mdef_ptr = bus.read_long(mdef_handle);
        assert_ne!(mdef_ptr, 0, "standard MDEF handle must be loaded");
        assert_eq!(
            bus.read_word(mdef_ptr),
            0x205F,
            "standard MDEF shim should be directly callable by 68k code"
        );
    }

    #[test]
    fn getmenu_resolves_custom_mdef_from_the_resource_chain() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_ptr = seed_menu_resource(&mut bus, 129, "Custom");
        let mdef_ptr = bus.alloc(2);
        bus.write_word(menu_ptr + 6, 256);
        bus.write_word(menu_ptr + 8, 0);
        bus.write_word(mdef_ptr, 0x4E75); // RTS: sufficient callable test body.
        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([
                        ((*b"MENU", 129), menu_ptr),
                        ((*b"MDEF", 256), mdef_ptr),
                    ]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        bus.write_word(TEST_SP, 129);

        disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let menu_handle = bus.read_long(cpu.read_reg(Register::A7));
        let live_menu_ptr = bus.read_long(menu_handle);
        let mdef_handle = bus.read_long(live_menu_ptr + 6);
        assert_ne!(
            mdef_handle,
            u32::from(256u16) << 16,
            "GetMenu must replace the raw ID-plus-zero placeholder"
        );
        assert_eq!(
            bus.read_long(mdef_handle),
            mdef_ptr,
            "custom MDEF handle should dereference to the loaded resource"
        );
    }

    #[test]
    fn getmenu_returns_resource_backed_handle_reused_until_release() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_ptr = seed_menu_resource(&mut bus, 128, "Game");

        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([((*b"MENU", 128), menu_ptr)]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        bus.write_word(TEST_SP, 128);
        let first = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
        assert!(first.is_some(), "GetMenu should be handled");
        assert!(first.unwrap().is_ok(), "GetMenu should succeed");
        let first_handle = bus.read_long(cpu.read_reg(Register::A7));
        assert_ne!(first_handle, 0, "GetMenu should return a handle on hit");
        assert_eq!(
            disp.loaded_handles.get(&first_handle).copied(),
            Some((bus.read_long(first_handle), *b"MENU", 128)),
            "GetMenu must return a Resource Manager-backed MENU handle"
        );
        assert_eq!(
            disp.resource_handle_files.get(&first_handle).copied(),
            Some(0),
            "GetMenu-backed MENU handle should retain its resource-file owner"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 128);
        let second = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
        assert!(second.is_some(), "second GetMenu should be handled");
        assert!(second.unwrap().is_ok(), "second GetMenu should succeed");
        assert_eq!(
            bus.read_long(cpu.read_reg(Register::A7)),
            first_handle,
            "repeated GetMenu should reuse the loaded MENU resource handle"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, first_handle);
        let release = disp.dispatch_resource(true, 0x1A3, &mut cpu, &mut bus);
        assert!(release.is_some(), "ReleaseResource should be handled");
        assert!(release.unwrap().is_ok(), "ReleaseResource should succeed");
        assert_eq!(
            bus.read_long(first_handle),
            0,
            "ReleaseResource should nil the released menu handle"
        );
        assert!(
            !disp.loaded_handles.contains_key(&first_handle),
            "ReleaseResource should invalidate the old MENU handle identity"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 128);
        let third = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
        assert!(third.is_some(), "third GetMenu should be handled");
        assert!(third.unwrap().is_ok(), "third GetMenu should succeed");
        let third_handle = bus.read_long(cpu.read_reg(Register::A7));
        assert_ne!(third_handle, 0, "GetMenu after release should reload");
        assert_ne!(
            third_handle, first_handle,
            "GetMenu after ReleaseResource should allocate a fresh handle"
        );
        assert_eq!(
            disp.resource_handle_files.get(&third_handle).copied(),
            Some(0),
            "reloaded MENU handle should be resource-backed"
        );
    }

    // IM:V 1986 p. V-234 and MTE 1992 p. 3-156: successful GetMenu also
    // attempts to load an 'mctb' resource with the same resource ID.
    #[test]
    fn getmenu_autoloads_matching_mctb_resource_into_menucinfo() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_id = 181i16;
        let menu_ptr = seed_menu_resource(&mut bus, menu_id, "Color");
        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([((*b"MENU", menu_id), menu_ptr)]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        install_mctb_resource(
            &mut disp,
            &mut bus,
            menu_id,
            &[(menu_id, 0, 0x2100), (menu_id, 2, 0x2200)],
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, menu_id as u16);
        let result = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetMenu should be handled");
        assert!(result.unwrap().is_ok(), "GetMenu should succeed");
        assert_ne!(
            bus.read_long(TEST_SP + 2),
            0,
            "GetMenu should return the loaded MENU handle"
        );

        let title_entry = get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 0);
        assert_ne!(
            title_entry, 0,
            "GetMenu should autoload the matching mctb title entry"
        );
        assert_eq!(
            bus.read_word(title_entry + 10),
            0x2103,
            "compiled mctb RGB2 should be converted into the live MCEntry"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 2),
            0,
            "GetMenu should autoload matching mctb item entries"
        );
    }

    // IM:I I-354: GetNewMBar returns NIL when the MBAR resource can't be read.
    #[test]
    fn getnewmbar_missing_resource_returns_nil_and_pops_menuid_word() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 128);
        bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);

        let result = disp.dispatch_menu(true, 0x1C0, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetNewMBar should be handled");
        assert!(result.unwrap().is_ok(), "GetNewMBar should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 2,
            "GetNewMBar should consume one INTEGER argument"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 2),
            0,
            "GetNewMBar must return NIL when MBAR cannot be read"
        );
    }

    // IM:I I-354: GetNewMBar creates and returns a menu list handle, and
    // SetMenuBar installs that list as the current menu list.
    #[test]
    fn getnewmbar_present_resource_returns_handle_and_setmenubar_installs_it() {
        let (mut disp, mut cpu, mut bus) = setup();
        let baseline = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x306000, "Old");
        insert_menu(&mut disp, &mut cpu, &mut bus, baseline);
        assert_ne!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 300), 0);

        let file_menu_ptr = seed_menu_resource(&mut bus, 128, "File");
        let edit_menu_ptr = seed_menu_resource(&mut bus, 129, "Edit");
        let mbar_ptr = seed_mbar_resource(&mut bus, &[128, 129]);
        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([
                        ((*b"MBAR", 900), mbar_ptr),
                        ((*b"MENU", 128), file_menu_ptr),
                        ((*b"MENU", 129), edit_menu_ptr),
                    ]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 900);
        let result = disp.dispatch_menu(true, 0x1C0, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetNewMBar should be handled");
        assert!(result.unwrap().is_ok(), "GetNewMBar should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 2,
            "GetNewMBar should pop menuBarID"
        );
        let mbar_handle = bus.read_long(TEST_SP + 2);
        assert_ne!(mbar_handle, 0, "GetNewMBar should return a non-NIL handle");
        let list_ptr = bus.read_long(mbar_handle);
        assert_ne!(list_ptr, 0, "returned menu-list handle should dereference");
        assert_eq!(
            bus.read_word(list_ptr),
            2,
            "menu-list block should describe both MBAR menu IDs"
        );
        assert_ne!(
            bus.read_long(list_ptr + 2),
            0,
            "first menu handle should be non-NIL"
        );
        assert_ne!(
            bus.read_long(list_ptr + 6),
            0,
            "second menu handle should be non-NIL"
        );

        // IM:I I-354: GetNewMBar only creates the list; SetMenuBar installs it.
        assert_ne!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 300),
            0,
            "GetNewMBar alone should not replace the current menu list"
        );
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 128),
            0,
            "new MBAR menus should not be current until SetMenuBar"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, mbar_handle);
        let set_result = disp.dispatch_menu(true, 0x13C, &mut cpu, &mut bus);
        assert!(set_result.is_some(), "SetMenuBar should be handled");
        assert!(set_result.unwrap().is_ok(), "SetMenuBar should succeed");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);

        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 300),
            0,
            "SetMenuBar should replace current list with the list from GetNewMBar"
        );
        assert_ne!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 128),
            0,
            "SetMenuBar should install File menu from MBAR"
        );
        assert_ne!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 129),
            0,
            "SetMenuBar should install Edit menu from MBAR"
        );
    }

    // IM:V 1986 p. V-244: GetNewMBar clears the current menu color
    // information table, loads the requested menus, and leaves the new
    // MenuCInfo state in place even though the previous MenuList is restored.
    #[test]
    fn getnewmbar_rebuilds_menucinfo_from_menu_mctb_resources() {
        let (mut disp, mut cpu, mut bus) = setup();
        let baseline = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x306080, "Old");
        insert_menu(&mut disp, &mut cpu, &mut bus, baseline);
        set_mc_entries_for_test(&mut disp, &mut cpu, &mut bus, &[(999, 1, 0x3100)]);
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 999, 1),
            0,
            "precondition: old MenuCInfo entry should exist before GetNewMBar"
        );

        let file_menu_ptr = seed_menu_resource(&mut bus, 601, "File");
        let edit_menu_ptr = seed_menu_resource(&mut bus, 602, "Edit");
        let mbar_ptr = seed_mbar_resource(&mut bus, &[601, 602]);
        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([
                        ((*b"MBAR", 901), mbar_ptr),
                        ((*b"MENU", 601), file_menu_ptr),
                        ((*b"MENU", 602), edit_menu_ptr),
                    ]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        install_mctb_resource(&mut disp, &mut bus, 601, &[(601, 0, 0x3200)]);
        install_mctb_resource(&mut disp, &mut bus, 602, &[(602, 2, 0x3300)]);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 901);
        let result = disp.dispatch_menu(true, 0x1C0, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetNewMBar should be handled");
        assert!(result.unwrap().is_ok(), "GetNewMBar should succeed");
        assert_ne!(
            bus.read_long(TEST_SP + 2),
            0,
            "GetNewMBar should return a new menu-list handle"
        );

        assert_ne!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 300),
            0,
            "GetNewMBar should restore the previous current MenuList"
        );
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 601),
            0,
            "GetNewMBar should not install the new MenuList until SetMenuBar"
        );
        assert_eq!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 999, 1),
            0,
            "GetNewMBar should clear the previous MenuCInfo table"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 601, 0),
            0,
            "GetNewMBar should load mctb entries for MBAR menu 601"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 602, 2),
            0,
            "GetNewMBar should load mctb entries for MBAR menu 602"
        );
    }

    // IM:I I-358 + I-352 + I-355:
    // AppendMenu must work before InsertMenu, and MenuKey should only find
    // items once the menu is actually in the current menu list.
    #[test]
    fn getmenu_appendmenu_then_insertmenu_enables_menukey_shortcut() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Seed MENU resource ID 180 with title "File" and no items.
        let menu_res_ptr = bus.alloc(64);
        bus.write_word(menu_res_ptr, 180);
        bus.write_word(menu_res_ptr + 2, 0);
        bus.write_word(menu_res_ptr + 4, 0);
        bus.write_long(menu_res_ptr + 6, 0);
        bus.write_long(menu_res_ptr + 10, 0xFFFF_FFFF);
        write_pstring(&mut bus, menu_res_ptr + 14, "File");
        bus.write_byte(menu_res_ptr + 19, 0); // empty items terminator
        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([((*b"MENU", 180), menu_res_ptr)]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        // GetMenu(180) should return a non-NIL handle.
        bus.write_word(TEST_SP, 180);
        assert!(disp
            .dispatch_menu(true, 0x1BF, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let menu_handle = bus.read_long(cpu.read_reg(Register::A7));
        assert_ne!(menu_handle, 0, "GetMenu should return a handle on hit");

        // Menu isn't in current list yet; MenuKey('O') must return 0.
        assert_eq!(menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'), 0);

        // Append before insertion must persist through InsertMenu.
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            menu_handle,
            0x306000,
            "Open/O",
        );
        insert_menu(&mut disp, &mut cpu, &mut bus, menu_handle);
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'),
            (180u32 << 16) | 1,
            "MenuKey should resolve appended shortcut after InsertMenu"
        );
    }

    // IM:I I-355: MenuKey scans menus from right to left when shortcuts collide.
    #[test]
    fn menukey_duplicate_shortcuts_prefer_rightmost_menu() {
        let (mut disp, mut cpu, mut bus) = setup();
        let left = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 200, 0x306100, "Left");
        append_menu_data(&mut disp, &mut cpu, &mut bus, left, 0x306200, "LeftCmd/X");
        insert_menu(&mut disp, &mut cpu, &mut bus, left);

        let right = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 201, 0x306300, "Right");
        append_menu_data(&mut disp, &mut cpu, &mut bus, right, 0x306400, "RightCmd/X");
        insert_menu(&mut disp, &mut cpu, &mut bus, right);

        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'X'),
            (201u32 << 16) | 1,
            "MenuKey should choose the rightmost menu's matching item"
        );
    }

    // IM:I I-355: only enabled items in the current menu list are eligible.
    #[test]
    fn menukey_ignores_uninserted_menu_items() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 210, 0x306500, "Ghost");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306600, "Hidden/G");
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'G'),
            0,
            "MenuKey should ignore items from menus not yet inserted"
        );
    }

    // IM:I I-352 and I-355: InsertMenu adds a menu to the current menu
    // list, and MenuKey searches that list. A beforeID of -1 omits the title
    // from the menu bar but does not make its command equivalents unavailable.
    #[test]
    fn menukey_searches_installed_command_only_menu_without_drawing_title() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 64;
        let base = bus.alloc(row_bytes * 342);
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        disp.menu_bar_hidden = false;
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let visible = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 230, 0x306800, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, visible, 0x306840, "Open/O");
        insert_menu(&mut disp, &mut cpu, &mut bus, visible);

        let commands =
            new_menu_with_title(&mut disp, &mut cpu, &mut bus, 231, 0x306880, "Commands");
        append_menu_data(&mut disp, &mut cpu, &mut bus, commands, 0x3068C0, "Pause/P");
        insert_menu_before(&mut disp, &mut cpu, &mut bus, commands, -1);

        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(disp.menu_title_regions().len(), 1);
        let menu_bar_before = bus.read_bytes(base, row_bytes as usize * 20);

        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'P'),
            (231u32 << 16) | 1
        );
        assert_eq!(
            bus.read_bytes(base, row_bytes as usize * 20),
            menu_bar_before,
            "a command-only match must not paint a hidden menu title"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, commands);
        disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(menu_key_result(&mut disp, &mut cpu, &mut bus, b'P'), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, commands);
        disp.dispatch_menu(true, 0x139, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, commands);
        disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(menu_key_result(&mut disp, &mut cpu, &mut bus, b'P'), 0);
    }

    // MTE 1992 p. 3-138: GetItemCmd returns 0 if the item has no
    // keyboard equivalent, submenu marker, script-code marker, or icon marker.
    #[test]
    fn getitemcmd_returns_zero_when_no_keyboard_equivalent() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 220, 0x306710, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306720, "Open");

        assert_eq!(
            get_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1),
            0,
            "GetItemCmd should return 0 when keyboard-equivalent field is clear"
        );
    }

    // MTE 1992 pp. 3-138 to 3-139: SetItemCmd writes cmdChar into the
    // item's keyboard-equivalent field.
    #[test]
    fn setitemcmd_sets_keyboard_equivalent_field() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 221, 0x306730, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306740, "Open");

        set_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1, b'O');
        assert_eq!(
            get_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1),
            b'O',
            "GetItemCmd should read back cmdChar written by SetItemCmd"
        );

        insert_menu(&mut disp, &mut cpu, &mut bus, menu);
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'),
            (221u32 << 16) | 1,
            "MenuKey should honor command key installed via SetItemCmd"
        );
    }

    // MTE 1992 p. 3-138: cmdChar=$1B marks the item as hierarchical.
    #[test]
    fn setitemcmd_sets_submenu_marker_for_hierarchical_items() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 222, 0x306750, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306760, "Open");

        set_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1, 0x1B);
        assert_eq!(
            get_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1),
            0x1B,
            "GetItemCmd should return $1B after SetItemCmd installs submenu marker"
        );
    }

    // 0x133 — AppendMenu: pops 8 bytes.
    #[test]
    fn test_append_menu() {
        let (mut disp, mut cpu, mut bus) = setup();
        let result = disp.dispatch_menu(true, 0x133, &mut cpu, &mut bus);
        assert!(result.is_some(), "AppendMenu should be handled");
        assert!(result.unwrap().is_ok(), "AppendMenu should succeed");
        let sp = cpu.read_reg(Register::A7);
        assert_eq!(sp, TEST_SP + 8, "AppendMenu should pop 8 bytes from stack");
    }

    // AppendMenu must serialise parsed items into the MENU record at
    // offset 15+title_len so the guest-memory item count (CountMItems /
    // CalcMenuSize path) matches the Rust-side items list.
    #[test]
    fn test_appendmenu_serialises_items_to_guest_memory() {
        let (mut disp, mut cpu, mut bus) = setup();

        // NewMenu(129, "File") — registers menu + allocates menu record
        let title_ptr = 0x302000u32;
        bus.write_byte(title_ptr, 4);
        bus.write_byte(title_ptr + 1, b'F');
        bus.write_byte(title_ptr + 2, b'i');
        bus.write_byte(title_ptr + 3, b'l');
        bus.write_byte(title_ptr + 4, b'e');
        bus.write_long(TEST_SP, title_ptr);
        bus.write_word(TEST_SP + 4, 129);
        let r = disp.dispatch_menu(true, 0x131, &mut cpu, &mut bus);
        assert!(r.is_some() && r.unwrap().is_ok());
        let sp_after_new = cpu.read_reg(Register::A7);
        let handle = bus.read_long(sp_after_new);
        assert_ne!(handle, 0);

        // AppendMenu(handle, "New/N;Open/O;Quit/Q")
        let data_ptr = 0x302100u32;
        let data = b"New/N;Open/O;Quit/Q";
        bus.write_byte(data_ptr, data.len() as u8);
        for (i, b) in data.iter().enumerate() {
            bus.write_byte(data_ptr + 1 + i as u32, *b);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, data_ptr); // SP+0 text ptr
        bus.write_long(TEST_SP + 4, handle); // SP+4 menu handle
        let r = disp.dispatch_menu(true, 0x133, &mut cpu, &mut bus);
        assert!(r.is_some() && r.unwrap().is_ok());

        // Rust-side self.menus should have 3 items.
        assert_eq!(disp.menus.len(), 1);
        assert_eq!(disp.menus[0].items.len(), 3);

        // CountMItems (0x150) reads from guest memory; must see 3 too.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        let r = disp.dispatch_menu(true, 0x150, &mut cpu, &mut bus);
        assert!(r.is_some() && r.unwrap().is_ok());
        let sp_count = cpu.read_reg(Register::A7);
        let count = bus.read_word(sp_count);
        assert_eq!(count, 3, "CountMItems must see AppendMenu'd items");

        // Guest memory layout sanity: at menu_ptr + 15 + 4 ("File")
        // we expect item 0's pstring = "New" (len 3).
        let menu_ptr = bus.read_long(handle);
        assert_eq!(bus.read_byte(menu_ptr + 19), 3);
        assert_eq!(bus.read_byte(menu_ptr + 20), b'N');
        assert_eq!(bus.read_byte(menu_ptr + 21), b'e');
        assert_eq!(bus.read_byte(menu_ptr + 22), b'w');
    }

    #[test]
    fn long_menu_mutations_preserve_trailing_item_and_refresh_guest_edits() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_id = 447;
        let handle = new_menu_with_title(
            &mut disp,
            &mut cpu,
            &mut bus,
            menu_id,
            0x303800,
            "Long",
        );

        let mut data = vec!["A/A"; 39];
        data.push("Tail/Z");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x303900,
            &data.join(";"),
        );
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);

        let first_long_ptr = bus.read_long(handle);
        assert!(
            bus.get_alloc_size(first_long_ptr).unwrap() > 256,
            "AppendMenu must grow MENU records beyond the legacy 256-byte buffer"
        );
        assert_eq!(count_menu_items_from_memory(&bus, handle), 40);
        assert_eq!(get_item_cmd(&mut disp, &mut cpu, &mut bus, handle, 40), b'Z');

        let replacement = "Expanded first item that forces another complete record resize";
        write_pstring(&mut bus, 0x303A00, replacement);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0x303A00);
        bus.write_word(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 6, handle);
        assert!(disp
            .dispatch_menu(true, 0x147, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        set_item_cmd(&mut disp, &mut cpu, &mut bus, handle, 1, b'X');

        for trap in [0x13A, 0x139] {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(TEST_SP, 1);
            bus.write_long(TEST_SP + 2, handle);
            assert!(disp.dispatch_menu(true, trap, &mut cpu, &mut bus).unwrap().is_ok());
        }
        let (height, width) = calc_menu_size_for_test(&mut disp, &mut cpu, &mut bus, handle);
        assert!(height > 0 && width > 0);

        let resized_ptr = bus.read_long(handle);
        assert!(bus.get_alloc_size(resized_ptr).unwrap() > 256);
        let parsed = parse_menu_resource(&bus, resized_ptr, handle);
        assert_eq!(parsed.items.len(), 40);
        assert_eq!(parsed.items[39].text, "Tail");
        assert_eq!(parsed.items[39].key_equiv, b'Z');

        // MenuInfo belongs to the guest. A direct, same-size edit to the last
        // item must become authoritative before rendering or frontend export.
        let mut tail_ptr = resized_ptr + 15 + bus.read_byte(resized_ptr + 14) as u32;
        for _ in 0..39 {
            tail_ptr += 5 + bus.read_byte(tail_ptr) as u32;
        }
        assert_eq!(bus.read_byte(tail_ptr), 4);
        for (index, byte) in b"Last".iter().enumerate() {
            bus.write_byte(tail_ptr + 1 + index as u32, *byte);
        }
        bus.write_byte(tail_ptr + 6, b'Q');

        cpu.write_reg(Register::A7, TEST_SP);
        assert!(disp
            .dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(disp.menus[0].items[39].text, "Last");
        assert_eq!(disp.menus[0].items[39].key_equiv, b'Q');

        let snapshot = disp.guest_menu_snapshot(&bus);
        assert_eq!(snapshot.menus[0].items[39].text, "Last");
        assert_eq!(snapshot.menus[0].items[39].key_equivalent, Some('q'));
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'Q'),
            ((menu_id as u32) << 16) | 40
        );
    }

    #[test]
    fn resource_backed_long_menu_keeps_its_complete_record_when_mutated() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_id = 448;
        let resource_ptr = bus.alloc(263);
        bus.write_word(resource_ptr, menu_id as u16);
        bus.write_long(resource_ptr + 10, 0xFFFF_FFFF);
        write_pstring(&mut bus, resource_ptr + 14, "Long");
        let mut offset = resource_ptr + 19;
        for _ in 0..39 {
            bus.write_byte(offset, 1);
            bus.write_byte(offset + 1, b'A');
            bus.write_byte(offset + 3, b'A');
            offset += 6;
        }
        bus.write_byte(offset, 4);
        bus.write_bytes(offset + 1, b"Tail");
        bus.write_byte(offset + 6, b'Z');
        bus.write_byte(offset + 9, 0);
        assert_eq!(offset + 10, resource_ptr + 263);

        disp.resources = Some(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([((*b"MENU", menu_id), resource_ptr)]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        bus.write_word(TEST_SP, menu_id as u16);
        assert!(disp
            .dispatch_menu(true, 0x1BF, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let handle = bus.read_long(cpu.read_reg(Register::A7));
        assert_eq!(bus.get_alloc_size(bus.read_long(handle)), Some(263));

        set_item_cmd(&mut disp, &mut cpu, &mut bus, handle, 1, b'X');
        let parsed = parse_menu_resource(&bus, bus.read_long(handle), handle);
        assert_eq!(parsed.items.len(), 40);
        assert_eq!(parsed.items[39].text, "Tail");
        assert_eq!(parsed.items[39].key_equiv, b'Z');
    }

    #[test]
    fn appendmenu_tracks_raw_menu_handle_before_appending_items() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = bus.alloc(4);
        let menu_ptr = bus.alloc(32);
        bus.write_long(handle, menu_ptr);
        bus.write_word(menu_ptr, 4000);
        bus.write_word(menu_ptr + 2, 0);
        bus.write_word(menu_ptr + 4, 0);
        bus.write_long(menu_ptr + 6, 0);
        bus.write_long(menu_ptr + 10, 0xFFFF_FFFF);
        write_pstring(&mut bus, menu_ptr + 14, "Sections");
        bus.write_byte(menu_ptr + 15 + "Sections".len() as u32, 0);

        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x302200,
            "Graphics;Sound;Controls",
        );

        assert_eq!(disp.menus.len(), 1);
        assert_eq!(disp.menus[0].id, 4000);
        assert_eq!(disp.menus[0].items.len(), 3);
        assert_eq!(
            disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
            Some("Graphics")
        );
        assert_eq!(
            bus.get_alloc_size(bus.read_long(handle)),
            Some(256),
            "AppendMenu should grow small raw MENU handles before serialising items"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        let r = disp.dispatch_menu(true, 0x150, &mut cpu, &mut bus);
        assert!(r.is_some() && r.unwrap().is_ok());
        let sp_count = cpu.read_reg(Register::A7);
        assert_eq!(bus.read_word(sp_count), 3);
    }

    #[test]
    fn appendmenu_accepts_a0_menu_handle_when_stack_handle_is_nil() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = bus.alloc(4);
        let menu_ptr = bus.alloc(32);
        let data_ptr = 0x302300;
        bus.write_long(handle, menu_ptr);
        bus.write_word(menu_ptr, 4000);
        bus.write_word(menu_ptr + 2, 0);
        bus.write_word(menu_ptr + 4, 0);
        bus.write_long(menu_ptr + 6, 0);
        bus.write_long(menu_ptr + 10, 0xFFFF_FFFF);
        write_pstring(&mut bus, menu_ptr + 14, "Sections");
        bus.write_byte(menu_ptr + 15 + "Sections".len() as u32, 0);
        write_pstring(&mut bus, data_ptr, "Graphics");

        cpu.write_reg(Register::A0, handle);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, data_ptr);
        bus.write_long(TEST_SP + 4, 0);

        let r = disp.dispatch_menu(true, 0x133, &mut cpu, &mut bus);
        assert!(r.is_some() && r.unwrap().is_ok());

        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 8,
            "AppendMenu should still pop the stack data/handle slots"
        );
        assert_eq!(
            disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
            Some("Graphics")
        );
    }

    #[test]
    fn appendmenu_accepts_a0_menu_ptr_when_stack_handle_is_nil() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 4000, 0x302500, "Sections");
        let menu_ptr = bus.read_long(handle);
        write_pstring(&mut bus, 0x302540, "Graphics");

        cpu.write_reg(Register::A0, menu_ptr);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0x302540);
        bus.write_long(TEST_SP + 4, 0);

        let r = disp.dispatch_menu(true, 0x133, &mut cpu, &mut bus);
        assert!(r.is_some() && r.unwrap().is_ok());

        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 8,
            "AppendMenu should still pop the stack data/handle slots"
        );
        assert_eq!(
            disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
            Some("Graphics")
        );
    }

    #[test]
    fn setitem_accepts_a0_menu_handle_when_stack_handle_is_nil() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 4000, 0x302300, "Sections");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x302340, " ");
        write_pstring(&mut bus, 0x302380, "Graphics");

        cpu.write_reg(Register::A0, handle);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0x302380);
        bus.write_word(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 6, 0);

        let r = disp.dispatch_menu(true, 0x147, &mut cpu, &mut bus);
        assert!(r.is_some() && r.unwrap().is_ok());

        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 10,
            "SetItem should still pop text, item, and menu argument slots"
        );
        assert_eq!(
            disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
            Some("Graphics")
        );
    }

    #[test]
    fn setitem_accepts_a0_menu_ptr_when_stack_handle_is_nil() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 4000, 0x302600, "Sections");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x302640, " ");
        let menu_ptr = bus.read_long(handle);
        write_pstring(&mut bus, 0x302680, "Graphics");

        cpu.write_reg(Register::A0, menu_ptr);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0x302680);
        bus.write_word(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 6, 0);

        let r = disp.dispatch_menu(true, 0x147, &mut cpu, &mut bus);
        assert!(r.is_some() && r.unwrap().is_ok());

        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 10,
            "SetItem should still pop text, item, and menu argument slots"
        );
        assert_eq!(
            disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
            Some("Graphics")
        );
    }

    // InsertMenuItem (0x026) must sync the guest-memory MENU record so
    // CountMItems reflects the inserted item.
    #[test]
    fn test_insertmenuitem_serialises_to_guest_memory() {
        let (mut disp, mut cpu, mut bus) = setup();

        // NewMenu(129, "File")
        let title_ptr = 0x303000u32;
        bus.write_byte(title_ptr, 4);
        for (i, b) in b"File".iter().enumerate() {
            bus.write_byte(title_ptr + 1 + i as u32, *b);
        }
        bus.write_long(TEST_SP, title_ptr);
        bus.write_word(TEST_SP + 4, 129);
        assert!(disp
            .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let handle = bus.read_long(cpu.read_reg(Register::A7));
        assert_ne!(handle, 0);

        // InsertMenuItem(handle, "Copy", afterItem=0)
        let text_ptr = 0x303100u32;
        bus.write_byte(text_ptr, 4);
        for (i, b) in b"Copy".iter().enumerate() {
            bus.write_byte(text_ptr + 1 + i as u32, *b);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0); // afterItem = 0 (insert at head)
        bus.write_long(TEST_SP + 2, text_ptr);
        bus.write_long(TEST_SP + 6, handle);
        assert!(disp
            .dispatch_menu(true, 0x026, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        assert_eq!(disp.menus[0].items.len(), 1);
        assert_eq!(disp.menus[0].items[0].text, "Copy");

        // CountMItems (0x150) must read 1 from guest memory.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        assert!(disp
            .dispatch_menu(true, 0x150, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(bus.read_word(cpu.read_reg(Register::A7)), 1);
    }

    // MTE 1992 p. 3-126: afterItem=0 inserts before the first menu item.
    #[test]
    fn insertmenuitem_afteritem_zero_inserts_before_first_item() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 150, 0x30A100, "Edit");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A110, "B;C");
        insert_menu_item_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A120, "A", 0);

        assert_eq!(disp.menus[0].items[0].text, "A");
        assert_eq!(disp.menus[0].items[1].text, "B");
        assert_eq!(disp.menus[0].items[2].text, "C");
    }

    // MTE 1992 p. 3-126: afterItem=n inserts after item n; values >= last item
    // append at the end.
    #[test]
    fn insertmenuitem_afteritem_index_and_past_end_place_items_per_spec() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 151, 0x30A200, "Edit");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A210, "A;B;C");

        insert_menu_item_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A220, "X", 2);
        insert_menu_item_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A230, "Tail", 99);

        let texts: Vec<String> = disp.menus[0].items.iter().map(|i| i.text.clone()).collect();
        assert_eq!(texts, vec!["A", "B", "X", "C", "Tail"]);
    }

    // MTE 1992 p. 3-126: when itemString contains multiple items separated by
    // ';', InsertMenuItem inserts them in reverse order, while honoring the same
    // metacharacter parsing rules as AppendMenu.
    #[test]
    fn insertmenuitem_multiple_items_reverse_order_and_metacharacters() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_id = 152;
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30A300, "Edit");

        insert_menu_item_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x30A310,
            "Paste/V;Copy/C;Cut/X;(-;Undo/Z",
            0,
        );

        let menu = &disp.menus[0];
        assert_eq!(menu.items.len(), 5);
        assert_eq!(menu.items[0].text, "Undo");
        assert_eq!(menu.items[0].key_equiv, b'Z');
        assert_eq!(menu.items[1].text, "-");
        assert!(!menu.items[1].enabled, "'(' must disable inserted item");
        assert_eq!(menu.items[2].text, "Cut");
        assert_eq!(menu.items[2].key_equiv, b'X');
        assert_eq!(menu.items[3].text, "Copy");
        assert_eq!(menu.items[3].key_equiv, b'C');
        assert_eq!(menu.items[4].text, "Paste");
        assert_eq!(menu.items[4].key_equiv, b'V');

        let menu_ptr = bus.read_long(handle);
        let flags = bus.read_long(menu_ptr + 10);
        assert_eq!(
            flags & (1 << 2),
            0,
            "inserted disabled item must clear item-2 bit"
        );

        insert_menu(&mut disp, &mut cpu, &mut bus, handle);
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'C'),
            ((menu_id as u32) << 16) | 4,
            "Copy command should map to item 4 in reverse-ordered insertion result"
        );
    }

    // SetItem (0x147) must sync the guest-memory MENU record's item text
    // after mutation — for the same reason AppendMenu and InsertMenuItem do.
    #[test]
    fn test_setitem_syncs_guest_memory_text() {
        let (mut disp, mut cpu, mut bus) = setup();

        // NewMenu(129, "File")
        let title_ptr = 0x304000u32;
        bus.write_byte(title_ptr, 4);
        for (i, b) in b"File".iter().enumerate() {
            bus.write_byte(title_ptr + 1 + i as u32, *b);
        }
        bus.write_long(TEST_SP, title_ptr);
        bus.write_word(TEST_SP + 4, 129);
        assert!(disp
            .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let handle = bus.read_long(cpu.read_reg(Register::A7));

        // AppendMenu(handle, "Old")
        let data_ptr = 0x304100u32;
        bus.write_byte(data_ptr, 3);
        for (i, b) in b"Old".iter().enumerate() {
            bus.write_byte(data_ptr + 1 + i as u32, *b);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, data_ptr);
        bus.write_long(TEST_SP + 4, handle);
        assert!(disp
            .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        // SetItem(handle, 1, "New")
        let new_ptr = 0x304200u32;
        bus.write_byte(new_ptr, 3);
        for (i, b) in b"New".iter().enumerate() {
            bus.write_byte(new_ptr + 1 + i as u32, *b);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, new_ptr); // SP+0 text ptr
        bus.write_word(TEST_SP + 4, 1); // SP+4 item index
        bus.write_long(TEST_SP + 6, handle); // SP+6 menu handle
        assert!(disp
            .dispatch_menu(true, 0x147, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        // Rust side updated.
        assert_eq!(disp.menus[0].items[0].text, "New");
        // Guest memory: at menu_ptr + 15 + 4 ("File") we expect the
        // first item's pstring = "New" (len 3).
        let menu_ptr = bus.read_long(handle);
        assert_eq!(bus.read_byte(menu_ptr + 19), 3);
        assert_eq!(bus.read_byte(menu_ptr + 20), b'N');
        assert_eq!(bus.read_byte(menu_ptr + 21), b'e');
        assert_eq!(bus.read_byte(menu_ptr + 22), b'w');
    }

    // DeleteMenuItem must sync the guest-memory MENU record so CountMItems
    // doesn't still see the deleted item via the guest-memory path.
    #[test]
    fn test_deletemenuitem_syncs_guest_memory() {
        let (mut disp, mut cpu, mut bus) = setup();

        // NewMenu(130, "Edit")
        let title_ptr = 0x305000u32;
        bus.write_byte(title_ptr, 4);
        for (i, b) in b"Edit".iter().enumerate() {
            bus.write_byte(title_ptr + 1 + i as u32, *b);
        }
        bus.write_long(TEST_SP, title_ptr);
        bus.write_word(TEST_SP + 4, 130);
        assert!(disp
            .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let handle = bus.read_long(cpu.read_reg(Register::A7));

        // AppendMenu(handle, "A;B;C") — 3 items.
        let data_ptr = 0x305100u32;
        let data = b"A;B;C";
        bus.write_byte(data_ptr, data.len() as u8);
        for (i, b) in data.iter().enumerate() {
            bus.write_byte(data_ptr + 1 + i as u32, *b);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, data_ptr);
        bus.write_long(TEST_SP + 4, handle);
        assert!(disp
            .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(disp.menus[0].items.len(), 3);

        // DeleteMenuItem(handle, 2) — remove "B".
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 2);
        bus.write_long(TEST_SP + 2, handle);
        assert!(disp
            .dispatch_menu(true, 0x152, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        assert_eq!(disp.menus[0].items.len(), 2);
        assert_eq!(disp.menus[0].items[0].text, "A");
        assert_eq!(disp.menus[0].items[1].text, "C");

        // CountMItems (0x150) via guest memory must read 2.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        assert!(disp
            .dispatch_menu(true, 0x150, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(bus.read_word(cpu.read_reg(Register::A7)), 2);
    }

    // IM:V 1986 p. V-244: DelMenuItem also removes the deleted item's
    // color entry from the application's menu color information table.
    #[test]
    fn deletemenuitem_removes_exact_menu_color_item_entry() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_id = 521i16;
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x305180, "Edit");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x3051C0, "A;B;C");
        set_mc_entries_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            &[
                (menu_id, 0, 0x1100),
                (menu_id, 1, 0x1200),
                (menu_id, 2, 0x1300),
                (menu_id, 3, 0x1400),
            ],
        );

        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 2),
            0,
            "precondition: item-2 MenuCInfo entry should exist"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 2);
        bus.write_long(TEST_SP + 2, handle);
        assert!(
            disp.dispatch_menu(true, 0x152, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "DeleteMenuItem should succeed"
        );

        assert_eq!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 2),
            0,
            "DeleteMenuItem should remove the deleted item's MenuCInfo entry"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 0),
            0,
            "DeleteMenuItem should preserve the menu-title MenuCInfo entry"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 1),
            0,
            "DeleteMenuItem should preserve other item entries"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 3),
            0,
            "DeleteMenuItem should not renumber unrelated MenuCInfo entries"
        );
    }

    #[test]
    fn disableitem_item_gt_31_is_noop() {
        let (mut disp, mut cpu, mut bus) = setup();

        // IM:TB Essentials 1992 p.3-131: items with number >31 cannot be
        // individually disabled by DisableItem.
        let title_ptr = 0x305200u32;
        bus.write_byte(title_ptr, 4);
        for (i, b) in b"Long".iter().enumerate() {
            bus.write_byte(title_ptr + 1 + i as u32, *b);
        }
        bus.write_long(TEST_SP, title_ptr);
        bus.write_word(TEST_SP + 4, 140);
        assert!(disp
            .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let handle = bus.read_long(cpu.read_reg(Register::A7));

        let mut items = String::new();
        for i in 1..=32 {
            if i > 1 {
                items.push(';');
            }
            items.push_str(&format!("I{i:02}"));
        }
        let data_ptr = 0x305300u32;
        bus.write_byte(data_ptr, items.len() as u8);
        for (i, b) in items.as_bytes().iter().enumerate() {
            bus.write_byte(data_ptr + 1 + i as u32, *b);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, data_ptr);
        bus.write_long(TEST_SP + 4, handle);
        assert!(disp
            .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        assert!(disp.menus[0].items[31].enabled, "item 32 starts enabled");
        let menu_ptr = bus.read_long(handle);
        let flags_before = bus.read_long(menu_ptr + 10);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 32);
        bus.write_long(TEST_SP + 2, handle);
        assert!(disp
            .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        assert!(
            disp.menus[0].items[31].enabled,
            "DisableItem(item>31) must not change item state"
        );
        assert_eq!(
            bus.read_long(menu_ptr + 10),
            flags_before,
            "DisableItem(item>31) must leave enableFlags unchanged"
        );
    }

    #[test]
    fn disableitem_zero_disables_menu_title_and_menukey_shortcuts() {
        // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-131:
        // DisableItem(menu, 0) disables the whole menu.
        // Inside Macintosh Volume I (1985), p. I-355:
        // MenuKey returns 0 when no enabled item in the current menu list
        // matches the key equivalent.
        let (mut disp, mut cpu, mut bus) = setup();
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 144, 0x305A00, "File");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            menu,
            0x305B00,
            "Open/O;Save/S",
        );
        insert_menu(&mut disp, &mut cpu, &mut bus, menu);

        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'),
            (144u32 << 16) | 1,
            "MenuKey should resolve Open shortcut before whole-menu disable"
        );
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'S'),
            (144u32 << 16) | 2,
            "MenuKey should resolve Save shortcut before whole-menu disable"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, menu);
        assert!(disp
            .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        let menu_state = disp
            .menus
            .iter()
            .find(|m| m.handle == menu)
            .expect("menu should remain tracked after disable");
        assert!(
            !menu_state.enabled,
            "DisableItem(item=0) should disable the menu title"
        );
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'),
            0,
            "MenuKey should return 0 for whole-menu-disabled Open shortcut"
        );
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'S'),
            0,
            "MenuKey should return 0 for whole-menu-disabled Save shortcut"
        );
    }

    #[test]
    fn enableitem_preserves_guest_enableflags_changes_before_mutating_item() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 145, 0x305C00, "File");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            menu,
            0x305D00,
            "Open/O;Save/S",
        );
        insert_menu(&mut disp, &mut cpu, &mut bus, menu);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, menu);
        assert!(disp
            .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        // MenuInfo is guest-owned memory. A menu definition procedure may
        // restore bit 0 directly before asking the Menu Manager to adjust a
        // particular item's bit.
        let menu_ptr = bus.read_long(menu);
        bus.write_long(menu_ptr + 10, bus.read_long(menu_ptr + 10) | 1);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 2);
        bus.write_long(TEST_SP + 2, menu);
        assert!(disp
            .dispatch_menu(true, 0x139, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        let state = disp.menus.iter().find(|m| m.handle == menu).unwrap();
        assert!(
            state.enabled,
            "guest-restored menu bit must survive EnableItem"
        );
        assert!(
            state.items[1].enabled,
            "the requested item should also be enabled"
        );
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'S'),
            (145u32 << 16) | 2,
            "MenuKey must observe the guest-restored menu enable bit"
        );
    }

    #[test]
    fn enableitem_item_gt_31_is_noop() {
        let (mut disp, mut cpu, mut bus) = setup();

        // IM:TB Essentials 1992 p.3-131: items with number >31 cannot be
        // individually enabled by EnableItem.
        let title_ptr = 0x305400u32;
        bus.write_byte(title_ptr, 4);
        for (i, b) in b"Long".iter().enumerate() {
            bus.write_byte(title_ptr + 1 + i as u32, *b);
        }
        bus.write_long(TEST_SP, title_ptr);
        bus.write_word(TEST_SP + 4, 141);
        assert!(disp
            .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let handle = bus.read_long(cpu.read_reg(Register::A7));

        let mut items = String::new();
        for i in 1..=32 {
            if i > 1 {
                items.push(';');
            }
            if i == 32 {
                items.push('(');
            }
            items.push_str(&format!("I{i:02}"));
        }
        let data_ptr = 0x305500u32;
        bus.write_byte(data_ptr, items.len() as u8);
        for (i, b) in items.as_bytes().iter().enumerate() {
            bus.write_byte(data_ptr + 1 + i as u32, *b);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, data_ptr);
        bus.write_long(TEST_SP + 4, handle);
        assert!(disp
            .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        assert!(
            !disp.menus[0].items[31].enabled,
            "item 32 starts disabled from metacharacter input"
        );
        let menu_ptr = bus.read_long(handle);
        let flags_before = bus.read_long(menu_ptr + 10);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 32);
        bus.write_long(TEST_SP + 2, handle);
        assert!(disp
            .dispatch_menu(true, 0x139, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        assert!(
            !disp.menus[0].items[31].enabled,
            "EnableItem(item>31) must not change item state"
        );
        assert_eq!(
            bus.read_long(menu_ptr + 10),
            flags_before,
            "EnableItem(item>31) must leave enableFlags unchanged"
        );
    }

    #[test]
    fn enableitem_zero_reenables_menu_title_but_preserves_preexisting_disabled_items() {
        let (mut disp, mut cpu, mut bus) = setup();

        // IM:TB Essentials 1992 p.3-131: enabling a whole menu with item=0
        // preserves any items that were previously individually disabled.
        let title_ptr = 0x305600u32;
        bus.write_byte(title_ptr, 4);
        for (i, b) in b"Edit".iter().enumerate() {
            bus.write_byte(title_ptr + 1 + i as u32, *b);
        }
        bus.write_long(TEST_SP, title_ptr);
        bus.write_word(TEST_SP + 4, 142);
        assert!(disp
            .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let handle = bus.read_long(cpu.read_reg(Register::A7));

        let data_ptr = 0x305700u32;
        let data = b"Cut/X;Copy/C;Paste/V";
        bus.write_byte(data_ptr, data.len() as u8);
        for (i, b) in data.iter().enumerate() {
            bus.write_byte(data_ptr + 1 + i as u32, *b);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, data_ptr);
        bus.write_long(TEST_SP + 4, handle);
        assert!(disp
            .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);

        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'X'),
            (142u32 << 16) | 1,
            "MenuKey should resolve item 1 before whole-menu disable"
        );
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'C'),
            (142u32 << 16) | 2,
            "MenuKey should resolve item 2 before individual disable"
        );

        // Disable Copy (item 2) individually.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 2);
        bus.write_long(TEST_SP + 2, handle);
        assert!(disp
            .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert!(!disp.menus[0].items[1].enabled);

        // Disable and re-enable whole menu.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, handle);
        assert!(disp
            .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert!(!disp.menus[0].enabled, "item=0 should disable menu title");
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'X'),
            0,
            "MenuKey should return 0 while whole menu is disabled"
        );
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'C'),
            0,
            "MenuKey should return 0 for individually disabled item while whole menu is disabled"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, handle);
        assert!(disp
            .dispatch_menu(true, 0x139, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        assert!(disp.menus[0].enabled, "menu title should be re-enabled");
        assert!(disp.menus[0].items[0].enabled, "item 1 stays enabled");
        assert!(
            !disp.menus[0].items[1].enabled,
            "individually disabled item must remain disabled"
        );
        assert!(disp.menus[0].items[2].enabled, "item 3 stays enabled");
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'X'),
            (142u32 << 16) | 1,
            "MenuKey should resolve non-individually-disabled item after EnableItem(item=0)"
        );
        assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'C'),
            0,
            "MenuKey should keep preexisting individually disabled item unavailable after EnableItem(item=0)"
        );

        let menu_ptr = bus.read_long(handle);
        let flags = bus.read_long(menu_ptr + 10);
        assert_eq!(flags & 1, 1, "menu-enabled bit should be set");
        assert_eq!(flags & (1 << 2), 0, "item 2 bit should remain cleared");
    }

    #[test]
    fn deletemenuitem_zero_or_oob_is_noop() {
        let (mut disp, mut cpu, mut bus) = setup();

        // IM:TB Essentials 1992 p.3-127: item=0 or item>last is a no-op.
        let title_ptr = 0x305800u32;
        bus.write_byte(title_ptr, 4);
        for (i, b) in b"Edit".iter().enumerate() {
            bus.write_byte(title_ptr + 1 + i as u32, *b);
        }
        bus.write_long(TEST_SP, title_ptr);
        bus.write_word(TEST_SP + 4, 143);
        assert!(disp
            .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let handle = bus.read_long(cpu.read_reg(Register::A7));

        let data_ptr = 0x305900u32;
        let data = b"A;B;C";
        bus.write_byte(data_ptr, data.len() as u8);
        for (i, b) in data.iter().enumerate() {
            bus.write_byte(data_ptr + 1 + i as u32, *b);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, data_ptr);
        bus.write_long(TEST_SP + 4, handle);
        assert!(disp
            .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(disp.menus[0].items.len(), 3);

        // item=0 -> no-op
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, handle);
        assert!(disp
            .dispatch_menu(true, 0x152, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(disp.menus[0].items.len(), 3);
        assert_eq!(disp.menus[0].items[0].text, "A");
        assert_eq!(disp.menus[0].items[1].text, "B");
        assert_eq!(disp.menus[0].items[2].text, "C");

        // item>last -> no-op
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 4);
        bus.write_long(TEST_SP + 2, handle);
        assert!(disp
            .dispatch_menu(true, 0x152, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(disp.menus[0].items.len(), 3);
        assert_eq!(disp.menus[0].items[0].text, "A");
        assert_eq!(disp.menus[0].items[1].text, "B");
        assert_eq!(disp.menus[0].items[2].text, "C");

        // Guest-memory count should also remain 3.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        assert!(disp
            .dispatch_menu(true, 0x150, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(bus.read_word(cpu.read_reg(Register::A7)), 3);
    }

    // 0x135 — InsertMenu: pops 6 bytes, adds menu to menus vec.
    #[test]
    fn test_insert_menu() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Allocate a menu record at 0x300000
        let menu_ptr = 0x300000u32;
        // Write menu ID
        bus.write_word(menu_ptr, 1); // menu_id = 1
                                     // Write title "File" as a Pascal string at offset 14 of the menu record
        bus.write_byte(menu_ptr + 14, 4); // length = 4
        bus.write_byte(menu_ptr + 15, b'F');
        bus.write_byte(menu_ptr + 16, b'i');
        bus.write_byte(menu_ptr + 17, b'l');
        bus.write_byte(menu_ptr + 18, b'e');

        // Create a handle pointing to this menu record
        let handle_addr = 0x300100u32;
        bus.write_long(handle_addr, menu_ptr);

        // Push stack: SP+0: before_id(2), SP+2: menu_handle(4)
        bus.write_word(TEST_SP, 0); // before_id = 0
        bus.write_long(TEST_SP + 2, handle_addr); // menu_handle

        let result = disp.dispatch_menu(true, 0x135, &mut cpu, &mut bus);
        assert!(result.is_some(), "InsertMenu should be handled");
        assert!(result.unwrap().is_ok(), "InsertMenu should succeed");
        let sp = cpu.read_reg(Register::A7);
        assert_eq!(sp, TEST_SP + 6, "InsertMenu should pop 6 bytes from stack");
        assert_eq!(disp.menus.len(), 1, "InsertMenu should add one menu");
        assert_eq!(
            disp.menus[0].title, "File",
            "InsertMenu should add 'File' to menus"
        );
    }

    // NewMenu+AppendMenu+InsertMenu must NOT produce duplicate entries in
    // self.menus — InsertMenu only adds a menu the bar tracker doesn't
    // already know about.
    #[test]
    fn test_newmenu_then_insertmenu_does_not_duplicate() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Step 1: NewMenu(129, "File")
        let title_ptr = 0x301000u32;
        bus.write_byte(title_ptr, 4);
        bus.write_byte(title_ptr + 1, b'F');
        bus.write_byte(title_ptr + 2, b'i');
        bus.write_byte(title_ptr + 3, b'l');
        bus.write_byte(title_ptr + 4, b'e');
        bus.write_long(TEST_SP, title_ptr); // SP+0 titlePtr
        bus.write_word(TEST_SP + 4, 129); // SP+4 menuID
        let result = disp.dispatch_menu(true, 0x131, &mut cpu, &mut bus);
        assert!(result.is_some() && result.unwrap().is_ok());
        let sp_after_new = cpu.read_reg(Register::A7);
        let handle = bus.read_long(sp_after_new);
        assert_ne!(handle, 0);
        assert_eq!(
            disp.menus.len(),
            1,
            "NewMenu should register the menu exactly once"
        );

        // Step 2: InsertMenu(handle, 0) — without a MENU resource
        // nothing should change in self.menus count.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0); // before_id
        bus.write_long(TEST_SP + 2, handle);
        let result = disp.dispatch_menu(true, 0x135, &mut cpu, &mut bus);
        assert!(result.is_some() && result.unwrap().is_ok());
        assert_eq!(
            disp.menus.len(),
            1,
            "InsertMenu must not duplicate the entry NewMenu already registered"
        );
        assert_eq!(disp.menus[0].title, "File");
        assert_eq!(disp.menus[0].handle, handle);
        assert_eq!(disp.last_inserted_menu_id, Some(129));
    }

    // InvalMenuBar ($A81D).
    // IM:MTE 1992 p. 3-93: PROCEDURE InvalMenuBar. Parameterless
    // Tool-bit PROCEDURE; A7 unchanged across the call.
    #[test]
    fn invalmenubar_procedure_call_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let result = disp.dispatch_menu(true, 0x01D, &mut cpu, &mut bus);
        assert!(result.is_some(), "InvalMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "InvalMenuBar should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "InvalMenuBar is a parameterless procedure and must preserve A7"
        );

        // 5-call composition catches per-call drift that a single-call
        // check might mask.
        let sp_before_five = cpu.read_reg(Register::A7);
        for _ in 0..5 {
            let result = disp.dispatch_menu(true, 0x01D, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before_five,
            "InvalMenuBar must preserve A7 across a 5-call composition"
        );
    }

    // InitProcMenu ($A808).
    // IM:V V-244: PROCEDURE InitProcMenu(mbResID: INTEGER). Tool-bit
    // Pascal PROCEDURE; caller pushes a 2-byte mbResID, trap pops 2
    // bytes, no FUNCTION result slot, A7 unchanged after the pop.
    #[test]
    fn initprocmenu_consumes_mbresid_and_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Caller-pushed mbResID + a trailing sentinel that the trap
        // must not touch.
        let sp_start = cpu.read_reg(Register::A7);
        let sentinel_addr = sp_start - 4;
        bus.write_long(sentinel_addr, 0xCAFE_BEEF);
        let sp_with_arg = sp_start - 6;
        bus.write_word(sp_with_arg, 0x0000); // mbResID = 0 (Apple-reserved default)
        cpu.write_reg(Register::A7, sp_with_arg);

        let result = disp.dispatch_menu(true, 0x008, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitProcMenu should be handled");
        assert!(result.unwrap().is_ok(), "InitProcMenu should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_with_arg + 2,
            "InitProcMenu must pop exactly 2 bytes (one INTEGER mbResID)"
        );
        assert_eq!(
            bus.read_long(sentinel_addr),
            0xCAFE_BEEF,
            "InitProcMenu must not write a FUNCTION result slot"
        );

        // 5-call composition: push 5 × 2-byte mbResID=0, dispatch 5
        // times in sequence, expect A7 to return to the pre-composition
        // value. Catches cumulative pop-size drift (e.g. pop-0 → +10,
        // pop-4 → −10) that a single-call test might mask.
        let sp_pre_five = cpu.read_reg(Register::A7);
        let sp_after_pushes = sp_pre_five - 10;
        for i in 0..5 {
            bus.write_word(sp_pre_five - 2 * (i + 1) as u32, 0x0000);
        }
        cpu.write_reg(Register::A7, sp_after_pushes);
        for _ in 0..5 {
            let result = disp.dispatch_menu(true, 0x008, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_pre_five,
            "InitProcMenu must pop 2 bytes per call across a 5-call composition"
        );
    }

    // 0x137 — DrawMenuBar: no stack params, calls draw_menu_bar_to_fb.
    #[test]
    fn test_draw_menu_bar() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);
        assert!(result.is_some(), "DrawMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "DrawMenuBar should succeed");
    }

    #[test]
    fn draw_menu_bar_releases_initial_kiosk_policy() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_menu_bar_policy(crate::runner::MenuBarPolicy::InitialKiosk);

        let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);

        assert!(result.is_some(), "DrawMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "DrawMenuBar should succeed");
        assert_eq!(
            disp.menu_bar_policy,
            crate::runner::MenuBarPolicy::GuestControlled
        );
        assert!(!disp.menu_bar_hidden);
    }

    #[test]
    fn draw_menu_bar_preserves_force_hidden_policy() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_menu_bar_policy(crate::runner::MenuBarPolicy::ForceHidden);

        let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);

        assert!(result.is_some(), "DrawMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "DrawMenuBar should succeed");
        assert_eq!(
            disp.menu_bar_policy,
            crate::runner::MenuBarPolicy::ForceHidden
        );
        assert!(disp.menu_bar_hidden);
    }

    #[test]
    fn drawmenubar_uninserted_newmenu_title_is_not_hit_testable() {
        // IM:I I-352 and I-354: NewMenu creates a menu record, but only
        // InsertMenu places that menu in the current menu list that
        // DrawMenuBar renders.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let _ghost = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x302000, "Ghost");

        cpu.write_reg(Register::A7, TEST_SP);
        let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert!(
            disp.menu_title_regions().is_empty(),
            "DrawMenuBar should have no title regions when no menus are inserted"
        );
        assert!(
            disp.menu_title_hit_test(20).is_none(),
            "Uninserted NewMenu title must not be menu-bar hit-testable"
        );
    }

    #[test]
    fn drawmenubar_skips_uninserted_menus_and_tracks_inserted_title_regions() {
        // IM:I I-354 / Macintosh Toolbox Essentials 1992 p.3-113:
        // DrawMenuBar draws according to the current menu list.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let _ghost = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x302000, "Ghost");
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x302100, "File");
        let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x302200, "Edit");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        insert_menu(&mut disp, &mut cpu, &mut bus, edit);

        cpu.write_reg(Register::A7, TEST_SP);
        let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let regions = disp.menu_title_regions();
        assert_eq!(
            regions.len(),
            2,
            "Only inserted menus should receive menu-bar title regions"
        );

        let file_mid = (regions[0].0 + regions[0].1) / 2;
        let file_idx = disp.menu_title_hit_test(file_mid).expect("file hit");
        assert_eq!(disp.menus[file_idx].id, 128);

        let edit_mid = (regions[1].0 + regions[1].1) / 2;
        let edit_idx = disp.menu_title_hit_test(edit_mid).expect("edit hit");
        assert_eq!(disp.menus[edit_idx].id, 129);
    }

    #[test]
    fn drawmenubar_uses_retro_computer_art_for_the_system_mark_without_layout_drift() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 64);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let system = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x302300, "\u{14}");
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x302400, "File");
        insert_menu(&mut disp, &mut cpu, &mut bus, system);
        insert_menu(&mut disp, &mut cpu, &mut bus, file);

        // The mark cell is pinned to the Chicago 12 mark advance the
        // System 7.5.3 menu bar lays out with, not measured from the
        // loaded font's mark glyph — Systemless substitutes its own
        // artwork for that glyph, so measuring it would drift the whole
        // bar whenever the font catalogue changed.
        let system_title_width = super::super::TrapDispatcher::menu_title_advance("\u{14}");
        assert_eq!(system_title_width, 11, "pinned system mark cell width");
        let regions = disp.menu_title_regions();
        assert_eq!(regions[0], (11, 18 + system_title_width + 6));
        assert_eq!(regions[1].0, 18 + system_title_width + 6);

        disp.draw_menu_bar_to_fb(&mut bus);

        const PIXELS: [[u8; 10]; 12] = [
            [0, 1, 1, 1, 1, 1, 1, 1, 1, 0],
            [1, 2, 2, 2, 2, 2, 2, 2, 2, 1],
            [1, 2, 3, 3, 3, 3, 3, 3, 2, 1],
            [1, 2, 3, 4, 3, 3, 3, 3, 2, 1],
            [1, 2, 3, 1, 3, 3, 1, 3, 2, 1],
            [1, 2, 3, 3, 3, 3, 3, 3, 2, 1],
            [1, 2, 3, 1, 3, 3, 1, 3, 2, 1],
            [1, 2, 3, 3, 1, 1, 3, 3, 2, 1],
            [1, 2, 2, 2, 2, 2, 2, 2, 2, 1],
            [0, 1, 1, 1, 1, 1, 1, 1, 1, 0],
            [0, 0, 0, 1, 2, 2, 1, 0, 0, 0],
            [0, 0, 1, 1, 1, 1, 1, 1, 0, 0],
        ];
        const PALETTE: [[u16; 3]; 4] = [
            [0x2222, 0x2222, 0x2222],
            [0xCCCC, 0xBBBB, 0x8888],
            [0x2222, 0x9999, 0xCCCC],
            [0xDDDD, 0xFFFF, 0xFFFF],
        ];
        let palette_indices = PALETTE.map(|rgb| {
            super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, rgb)
                .expect("8bpp test screen should expose a device color table")
        });
        let background =
            super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0xFFFF, 0xFFFF, 0xFFFF])
                .unwrap();

        for (row, pixels) in PIXELS.into_iter().enumerate() {
            for (col, palette_index) in pixels.into_iter().enumerate() {
                let expected = if palette_index == 0 {
                    background
                } else {
                    palette_indices[usize::from(palette_index - 1)]
                };
                assert_eq!(
                    screen_pixel_index(
                        &bus,
                        screen_base,
                        row_bytes,
                        18 + col as i16,
                        3 + row as i16,
                    ),
                    expected,
                    "system menu mark pixel ({col}, {row})"
                );
            }
        }

        assert_eq!(disp.menu_title_regions(), regions);
        let file_midpoint = (regions[1].0 + regions[1].1) / 2;
        assert_eq!(disp.menu_title_hit_test(file_midpoint), Some(1));
    }

    #[test]
    fn drawmenubar_keeps_the_retro_computer_mark_legible_in_monochrome() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 16;
        let screen_base = bus.alloc(row_bytes * 64);
        disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 64, 1);
        clear_1bpp_screen(&mut bus, screen_base, row_bytes, 64);
        bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let system = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x302300, "\u{14}");
        insert_menu(&mut disp, &mut cpu, &mut bus, system);

        disp.draw_menu_bar_to_fb(&mut bus);

        for &(x, y) in &[
            (1, 0),
            (0, 1),
            (9, 1),
            (3, 4),
            (6, 4),
            (3, 6),
            (6, 6),
            (4, 7),
            (5, 7),
            (2, 11),
            (7, 11),
        ] {
            assert!(
                screen_pixel_is_set(&bus, screen_base, row_bytes, 18 + x, 3 + y),
                "dark mark pixel ({x}, {y})"
            );
        }
        for &(x, y) in &[(0, 0), (1, 1), (3, 3), (5, 10)] {
            assert!(
                !screen_pixel_is_set(&bus, screen_base, row_bytes, 18 + x, 3 + y),
                "light mark pixel ({x}, {y})"
            );
        }
    }

    // MTE 1992 p. 3-131 / HIG 1992 p. 54: DisableItem(menu, 0) leaves the
    // title visible but dimmed. On a colour screen the definition procedure
    // greys it through GetGray (IM:V 1986 p. V-142), so every glyph pixel
    // lands on the intermediate shade — System 7.5.3 under BasiliskII draws
    // Sid Meier's Civilization's disabled City title in solid grey, not as
    // stippled black.
    #[test]
    fn drawmenubar_8bpp_dims_a_disabled_title_to_solid_gray() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 64);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 643, 0x303300, "File");
        let city = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 644, 0x303340, "City");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        insert_menu(&mut disp, &mut cpu, &mut bus, city);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, city);
        assert!(
            disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "DisableItem(menu, 0) should disable the whole City title"
        );

        disp.draw_menu_bar_to_fb(&mut bus);

        let black = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0; 3]).unwrap();
        let gray =
            super::super::TrapDispatcher::fb_gray_pixel_index_between(&bus, [0xFFFF; 3], [0, 0, 0])
                .expect("8bpp test screen should express an intermediate shade");
        let regions = disp.menu_title_regions();
        let cell = |(left, right): (i16, i16)| -> Vec<u8> {
            (1..19)
                .flat_map(|y| (left..right).map(move |x| (x, y)))
                .map(|(x, y)| screen_pixel_index(&bus, base, row_bytes, x, y))
                .collect()
        };

        let enabled = cell(regions[0]);
        assert!(
            enabled.iter().any(|&pixel| pixel == black),
            "the enabled File title should draw in full black"
        );

        let dimmed = cell(regions[1]);
        assert!(
            dimmed.iter().any(|&pixel| pixel == gray),
            "the disabled City title should draw in the intermediate grey shade"
        );
        assert!(
            !dimmed.iter().any(|&pixel| pixel == black),
            "no part of a dimmed title should stay full black"
        );
    }

    #[test]
    fn drawmenubar_tracks_full_top_menubar_title_order_and_hits() {
        // The visible Mac menu bar is part of the rendered surface, not
        // only the pull-down menu body. This pins the common top-bar layout
        // shape; exact pixel rendering follows BasiliskII.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        for (id, title, ptr) in [
            (128, "Apple", 0x302300),
            (129, "File", 0x302360),
            (130, "Edit", 0x3023C0),
            (131, "Special", 0x302420),
        ] {
            let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, id, ptr, title);
            insert_menu(&mut disp, &mut cpu, &mut bus, handle);
        }

        cpu.write_reg(Register::A7, TEST_SP);
        let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);
        assert!(result.is_some(), "DrawMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "DrawMenuBar should succeed");

        let regions = disp.menu_title_regions();
        assert_eq!(
            regions.len(),
            4,
            "Apple/File/Edit/Special should all receive title hit regions"
        );
        for pair in regions.windows(2) {
            assert!(
                pair[0].1 <= pair[1].0,
                "top menu-bar title regions should be left-to-right and non-overlapping"
            );
        }

        for ((left, right), expected_id) in regions.iter().copied().zip([128, 129, 130, 131]) {
            let hit_h = (left + right) / 2;
            let menu_idx = disp
                .menu_title_hit_test(hit_h)
                .expect("title midpoint should hit an inserted menu");
            assert_eq!(
                disp.menus[menu_idx].id, expected_id,
                "menu title midpoint should hit the expected top-bar menu"
            );
        }
    }

    #[test]
    fn drawmenubar_systemless_theme_routes_chrome_through_provider() {
        let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
        let classic_row_bytes = 64;
        let classic_base = classic_bus.alloc(classic_row_bytes * 342);
        classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
        classic.menu_bar_hidden = false;
        clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
        classic_bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        classic_cpu.write_reg(Register::A7, TEST_SP);
        classic
            .dispatch_menu(true, 0x137, &mut classic_cpu, &mut classic_bus)
            .unwrap()
            .unwrap();

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        let themed_row_bytes = 64;
        let themed_base = themed_bus.alloc(themed_row_bytes * 342);
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
        themed.menu_bar_hidden = false;
        clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
        themed_bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        themed_cpu.write_reg(Register::A7, TEST_SP);
        themed
            .dispatch_menu(true, 0x137, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();

        assert!(
            !screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 5, 0),
            "classic System 7 menu bar chrome should leave the top edge white"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 5, 0),
            "systemless-default menu bar provider should own the top edge"
        );
    }

    #[test]
    fn drawmenubar_systemless_theme_routes_title_states_through_provider() {
        let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
        let classic_row_bytes = 64;
        let classic_base = classic_bus.alloc(classic_row_bytes * 342);
        classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
        classic.menu_bar_hidden = false;
        clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
        classic_bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let classic_file = new_menu_with_title(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            621,
            0x302E00,
            "File",
        );
        let classic_edit = new_menu_with_title(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            622,
            0x302E40,
            "Edit",
        );
        insert_menu(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            classic_file,
        );
        insert_menu(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            classic_edit,
        );
        classic_cpu.write_reg(Register::A7, TEST_SP);
        classic_bus.write_word(TEST_SP, 0);
        classic_bus.write_long(TEST_SP + 2, classic_edit);
        classic
            .dispatch_menu(true, 0x13A, &mut classic_cpu, &mut classic_bus)
            .unwrap()
            .unwrap();
        classic_cpu.write_reg(Register::A7, TEST_SP);
        classic
            .dispatch_menu(true, 0x137, &mut classic_cpu, &mut classic_bus)
            .unwrap()
            .unwrap();

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        let themed_row_bytes = 64;
        let themed_base = themed_bus.alloc(themed_row_bytes * 342);
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
        themed.menu_bar_hidden = false;
        clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
        themed_bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let themed_file = new_menu_with_title(
            &mut themed,
            &mut themed_cpu,
            &mut themed_bus,
            621,
            0x302E00,
            "File",
        );
        let themed_edit = new_menu_with_title(
            &mut themed,
            &mut themed_cpu,
            &mut themed_bus,
            622,
            0x302E40,
            "Edit",
        );
        insert_menu(&mut themed, &mut themed_cpu, &mut themed_bus, themed_file);
        insert_menu(&mut themed, &mut themed_cpu, &mut themed_bus, themed_edit);
        themed_cpu.write_reg(Register::A7, TEST_SP);
        themed_bus.write_word(TEST_SP, 0);
        themed_bus.write_long(TEST_SP + 2, themed_edit);
        themed
            .dispatch_menu(true, 0x13A, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();
        themed_cpu.write_reg(Register::A7, TEST_SP);
        themed
            .dispatch_menu(true, 0x137, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();

        let classic_regions = classic.menu_title_regions();
        let themed_regions = themed.menu_title_regions();
        assert_eq!(
            themed_regions, classic_regions,
            "systemless-default menu-title chrome must preserve title hit regions"
        );
        let (file_left, _file_right) = themed_regions[0];
        let (edit_left, _edit_right) = themed_regions[1];

        // HIG 1992 p. 54: unavailable menu titles remain visible but dimmed.
        // The provider-owned states add title chrome only; text placement and
        // title geometry stay on the existing Menu Manager path.
        assert!(
            screen_pixel_is_set(
                &themed_bus,
                themed_base,
                themed_row_bytes,
                file_left + 4,
                17
            ),
            "systemless-default should draw provider chrome for an enabled menu title"
        );
        assert!(
            !screen_pixel_is_set(
                &classic_bus,
                classic_base,
                classic_row_bytes,
                file_left + 4,
                17
            ),
            "classic System 7 path should not draw the systemless enabled-title underline"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, edit_left + 2, 3),
            "systemless-default should draw provider chrome for a disabled menu title"
        );
        assert!(
            !screen_pixel_is_set(
                &classic_bus,
                classic_base,
                classic_row_bytes,
                edit_left + 2,
                3
            ),
            "classic System 7 path should not draw the systemless disabled-title frame"
        );
    }

    #[test]
    fn highlight_menu_title_systemless_theme_routes_highlight_state_through_provider() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 64;
        let base = bus.alloc(row_bytes * 342);
        disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        disp.menu_bar_hidden = false;
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 623, 0x302E80, "File");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let regions = disp.menu_title_regions();
        let (left, right) = regions[0];
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, left + 4, 17),
            "precondition: enabled systemless title chrome should draw before highlighting"
        );
        let title_pixel = (2..16)
            .flat_map(|y| ((left + 7)..(right - 7)).map(move |x| (x, y)))
            .find(|(x, y)| screen_pixel_is_set(&bus, base, row_bytes, *x, *y))
            .expect("precondition: menu title text should draw before highlighting");

        disp.highlight_menu_title(&mut bus, 0);

        assert_eq!(
            disp.menu_title_regions(),
            regions,
            "systemless highlighted title chrome must preserve title hit regions"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, left + 1, 5),
            "systemless-default should fill highlighted title chrome through the provider"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, left + 4, 17),
            "provider-highlighted title chrome should redraw, not invert, the enabled-title underline"
        );
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
            "highlighted systemless title should redraw the menu text in the highlighted foreground"
        );
    }

    #[test]
    fn draw_menu_dropdown_systemless_theme_routes_chrome_through_provider() {
        let rect = (20, 20, 56, 120);

        let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
        let classic_row_bytes = 64;
        let classic_base = classic_bus.alloc(classic_row_bytes * 342);
        classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
        let classic_menu = new_menu_with_title(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            610,
            0x302300,
            "File",
        );
        append_menu_data(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            classic_menu,
            0x302340,
            "Open/O;Close/W",
        );
        classic.draw_menu_dropdown(&mut classic_bus, 0, rect);

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        let themed_row_bytes = 64;
        let themed_base = themed_bus.alloc(themed_row_bytes * 342);
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
        let themed_menu = new_menu_with_title(
            &mut themed,
            &mut themed_cpu,
            &mut themed_bus,
            610,
            0x302300,
            "File",
        );
        append_menu_data(
            &mut themed,
            &mut themed_cpu,
            &mut themed_bus,
            themed_menu,
            0x302340,
            "Open/O;Close/W",
        );
        themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

        assert!(
            screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 120, 22),
            "classic dropdown chrome should draw its right-edge drop shadow"
        );
        assert!(
            !screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 120, 22),
            "systemless-default dropdown provider should not draw the classic shadow"
        );
    }

    #[test]
    fn draw_menu_dropdown_systemless_theme_routes_item_states_through_provider() {
        let rect = (20, 20, 86, 140);

        let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
        let classic_row_bytes = 64;
        let classic_base = classic_bus.alloc(classic_row_bytes * 342);
        classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
        let classic_menu = new_menu_with_title(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            611,
            0x302400,
            "File",
        );
        append_menu_data(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            classic_menu,
            0x302440,
            "Open/O;Pick;-;(Dim/D",
        );
        classic.menus[0].items[0].mark = 0x12;
        classic.menus[0].items[1].icon = 7;
        classic.menu_tracking = Some(MenuTrackingState {
            active_menu: 0,
            highlighted_item: 2,
            saved_pixels: Vec::new(),
            dropdown_rect: rect,
            stack_ptr: TEST_SP,
            flash_remaining: 0,
            flash_delay: 0,
            flash_result: 0,
            submenu: None,
        });
        classic.draw_menu_dropdown(&mut classic_bus, 0, rect);

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        let themed_row_bytes = 64;
        let themed_base = themed_bus.alloc(themed_row_bytes * 342);
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
        let themed_menu = new_menu_with_title(
            &mut themed,
            &mut themed_cpu,
            &mut themed_bus,
            611,
            0x302400,
            "File",
        );
        append_menu_data(
            &mut themed,
            &mut themed_cpu,
            &mut themed_bus,
            themed_menu,
            0x302440,
            "Open/O;Pick;-;(Dim/D",
        );
        themed.menus[0].items[0].mark = 0x12;
        themed.menus[0].items[1].icon = 7;
        themed.menu_tracking = Some(MenuTrackingState {
            active_menu: 0,
            highlighted_item: 2,
            saved_pixels: Vec::new(),
            dropdown_rect: rect,
            stack_ptr: TEST_SP,
            flash_remaining: 0,
            flash_delay: 0,
            flash_result: 0,
            submenu: None,
        });
        themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

        assert!(
            !screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 23, 39),
            "classic dropdown rows should not draw the systemless highlight rail"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 23, 39),
            "systemless-default provider should own highlighted menu-item row chrome"
        );
        assert!(
            !screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 36, 43),
            "classic dropdown path should not draw systemless menu-item icon chrome"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 36, 43),
            "systemless-default provider should receive and render menu-item icon state"
        );
        assert!(
            !screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 29, 79),
            "classic separator remains dotted at this odd x-coordinate"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 29, 79),
            "systemless-default provider should own separator menu-item row chrome"
        );
    }

    #[test]
    fn draw_menu_dropdown_systemless_theme_preserves_mark_and_command_indicators() {
        let rect = (20, 20, 56, 140);
        let row_top = rect.0 + 1;
        let row_bottom = row_top + 16;

        let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
        let classic_row_bytes = 64;
        let classic_base = classic_bus.alloc(classic_row_bytes * 342);
        classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
        let classic_menu = new_menu_with_title(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            618,
            0x302B00,
            "File",
        );
        append_menu_data(
            &mut classic,
            &mut classic_cpu,
            &mut classic_bus,
            classic_menu,
            0x302B40,
            "Open/1",
        );
        classic.menus[0].items[0].mark = 0x12;
        classic.draw_menu_dropdown(&mut classic_bus, 0, rect);

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        let themed_row_bytes = 64;
        let themed_base = themed_bus.alloc(themed_row_bytes * 342);
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
        let themed_menu = new_menu_with_title(
            &mut themed,
            &mut themed_cpu,
            &mut themed_bus,
            618,
            0x302B00,
            "File",
        );
        append_menu_data(
            &mut themed,
            &mut themed_cpu,
            &mut themed_bus,
            themed_menu,
            0x302B40,
            "Open/1",
        );
        themed.menus[0].items[0].mark = 0x12;
        themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

        let provider_mark_pixel = |x: i16, y: i16| {
            let provider_left = rect.1 + 1;
            let provider_top = row_top;
            x >= provider_left + 5
                && x < provider_left + 13
                && y >= provider_top + 3
                && y < provider_top + 13
        };
        let mark_pixel = (row_top..row_bottom)
            .flat_map(|y| ((rect.1 + 4)..(rect.1 + 16)).map(move |x| (x, y)))
            .find(|(x, y)| {
                screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, *x, *y)
                    && !provider_mark_pixel(*x, *y)
            })
            .expect("classic mark glyph should have a pixel outside the provider mark indicator");

        let provider_command_pixel = |x: i16, y: i16| {
            let provider_left = rect.1 + 1;
            let provider_top = row_top;
            let provider_width = (rect.3 - 1) - provider_left;
            x >= provider_left + provider_width - 15
                && x < provider_left + provider_width - 13
                && y >= provider_top + 4
                && y < provider_top + 12
        };
        let command_pixel = (row_top..row_bottom)
            .flat_map(|y| ((rect.3 - 28)..(rect.3 - 4)).map(move |x| (x, y)))
            .find(|(x, y)| {
                screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, *x, *y)
                    && !provider_command_pixel(*x, *y)
            })
            .expect(
                "classic command-key glyph should have a pixel outside the provider command marker",
            );
        let command_left = rect.3 - 25;
        let command_symbol_pixel = (row_top..row_bottom)
            .flat_map(|y| (command_left..(command_left + 7)).map(move |x| (x, y)))
            .find(|(x, y)| {
                screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, *x, *y)
            })
            .expect("classic command-key equivalent should draw the Command symbol itself");
        let label_pixel = (row_top..row_bottom)
            .flat_map(|y| ((rect.1 + 15)..(rect.1 + 60)).map(move |x| (x, y)))
            .find(|(x, y)| {
                screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, *x, *y)
            })
            .expect("classic item label should draw");

        assert!(
            screen_pixel_is_set(
                &themed_bus,
                themed_base,
                themed_row_bytes,
                mark_pixel.0,
                mark_pixel.1
            ),
            "systemless-default row chrome should preserve the menu item's mark glyph"
        );
        assert!(
            screen_pixel_is_set(
                &themed_bus,
                themed_base,
                themed_row_bytes,
                command_pixel.0,
                command_pixel.1
            ),
            "systemless-default row chrome should preserve the full command-key equivalent"
        );
        assert!(
            screen_pixel_is_set(
                &themed_bus,
                themed_base,
                themed_row_bytes,
                command_symbol_pixel.0,
                command_symbol_pixel.1
            ),
            "systemless-default row chrome should preserve the Command symbol itself"
        );

        clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
        themed.menu_tracking = Some(MenuTrackingState {
            active_menu: 0,
            highlighted_item: 1,
            saved_pixels: Vec::new(),
            dropdown_rect: rect,
            stack_ptr: TEST_SP,
            flash_remaining: 0,
            flash_delay: 0,
            flash_result: 0,
            submenu: None,
        });
        themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

        assert!(
            screen_pixel_is_set(
                &themed_bus,
                themed_base,
                themed_row_bytes,
                mark_pixel.0,
                mark_pixel.1
            ),
            "highlighted systemless-default row chrome should preserve the menu item's mark glyph"
        );
        assert!(
            screen_pixel_is_set(
                &themed_bus,
                themed_base,
                themed_row_bytes,
                command_pixel.0,
                command_pixel.1
            ),
            "highlighted systemless-default row chrome should preserve the full command-key equivalent"
        );
        assert!(
            screen_pixel_is_set(
                &themed_bus,
                themed_base,
                themed_row_bytes,
                command_symbol_pixel.0,
                command_symbol_pixel.1
            ),
            "highlighted systemless-default row chrome should preserve the Command symbol itself"
        );

        themed.redraw_chrome(&mut themed_bus);

        for (pixel, label) in [
            (mark_pixel, "checkmark"),
            (label_pixel, "item label"),
            (command_symbol_pixel, "Command symbol"),
            (command_pixel, "command key"),
        ] {
            assert!(
                screen_pixel_is_set(
                    &themed_bus,
                    themed_base,
                    themed_row_bytes,
                    pixel.0,
                    pixel.1
                ),
                "final themed chrome composition should preserve the highlighted {label}"
            );
        }

        clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
        themed.menu_tracking = None;
        themed.dialog_tracking = Some(super::super::dispatch::DialogTrackingState {
            active_popup: Some(super::super::dispatch::DialogPopupTrackingState {
                item_no: 1,
                ctrl_handle: 0,
                ctrl_ptr: 0,
                active_menu: 0,
                highlighted_item: 1,
                saved_pixels: Vec::new(),
                dropdown_rect: rect,
            }),
            ..Default::default()
        });
        themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

        assert!(
            screen_pixel_is_set(
                &themed_bus,
                themed_base,
                themed_row_bytes,
                rect.1 + 2,
                row_top + 1
            ),
            "dialog popup tracking should route the highlighted row through themed chrome"
        );

        themed.redraw_chrome(&mut themed_bus);

        for (pixel, label) in [
            (mark_pixel, "popup checkmark"),
            (label_pixel, "popup item label"),
            (command_symbol_pixel, "popup Command symbol"),
            (command_pixel, "popup command key"),
        ] {
            assert!(
                screen_pixel_is_set(
                    &themed_bus,
                    themed_base,
                    themed_row_bytes,
                    pixel.0,
                    pixel.1
                ),
                "final themed popup composition should preserve the highlighted {label}"
            );
        }
    }

    #[test]
    fn draw_menu_dropdown_applies_setitemstyle_pixels_with_classic_style_metrics() {
        let rect = (20, 20, 104, 180);
        let menu_data = "Bold;Italic;Underline;Outline;Shadow;Condense;Extend";
        let style_cases = [
            ("bold", "Bold", 0x01u8),
            ("italic", "Italic", 0x02u8),
            ("underline", "Underline", 0x04u8),
            ("outline", "Outline", 0x08u8),
            ("shadow", "Shadow", 0x10u8),
            ("condense", "Condense", 0x20u8),
            ("extend", "Extend", 0x40u8),
        ];

        let (mut plain, mut plain_cpu, mut plain_bus) = setup_with_port();
        let plain_row_bytes = 64;
        let plain_base = plain_bus.alloc(plain_row_bytes * 342);
        plain.set_screen_mode_for_test(plain_base, plain_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut plain_bus, plain_base, plain_row_bytes, 342);
        let plain_menu = new_menu_with_title(
            &mut plain,
            &mut plain_cpu,
            &mut plain_bus,
            619,
            0x302C00,
            "Style",
        );
        append_menu_data(
            &mut plain,
            &mut plain_cpu,
            &mut plain_bus,
            plain_menu,
            0x302C40,
            menu_data,
        );
        let plain_size =
            calc_menu_size_for_test(&mut plain, &mut plain_cpu, &mut plain_bus, plain_menu);
        plain.draw_menu_dropdown(&mut plain_bus, 0, rect);

        let (mut styled, mut styled_cpu, mut styled_bus) = setup_with_port();
        let styled_row_bytes = 64;
        let styled_base = styled_bus.alloc(styled_row_bytes * 342);
        styled.set_screen_mode_for_test(styled_base, styled_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut styled_bus, styled_base, styled_row_bytes, 342);
        let styled_menu = new_menu_with_title(
            &mut styled,
            &mut styled_cpu,
            &mut styled_bus,
            619,
            0x302C00,
            "Style",
        );
        append_menu_data(
            &mut styled,
            &mut styled_cpu,
            &mut styled_bus,
            styled_menu,
            0x302C40,
            menu_data,
        );
        for (idx, (_label, _text, style)) in style_cases.iter().enumerate() {
            set_menu_item_style(
                &mut styled,
                &mut styled_cpu,
                &mut styled_bus,
                styled_menu,
                idx as i16 + 1,
                *style,
            );
        }
        let styled_size =
            calc_menu_size_for_test(&mut styled, &mut styled_cpu, &mut styled_bus, styled_menu);
        styled.draw_menu_dropdown(&mut styled_bus, 0, rect);

        assert_eq!(
            styled_size.0, plain_size.0,
            "SetItemStyle should not change CalcMenuSize width"
        );
        assert_eq!(
            styled_size.1,
            plain_size.1 + (super::MENU_SHADOW_STYLE_ROW_HEIGHT - super::MENU_ROW_HEIGHT),
            "System 7.5.3's standard MDEF grows the shadow-styled item row"
        );

        let mut row_top = rect.0 + 1;
        for (idx, (label, text, style)) in style_cases.iter().enumerate() {
            let styled_height = styled.menu_item_height(&styled_bus, &styled.menus[0].items[idx]);
            let plain_height = plain.menu_item_height(&plain_bus, &plain.menus[0].items[idx]);
            let expected_height = if (*style & super::MENU_TEXT_STYLE_SHADOW) != 0 {
                plain_height.max(super::MENU_SHADOW_STYLE_ROW_HEIGHT)
            } else {
                plain_height
            };
            assert_eq!(
                styled_height, expected_height,
                "{label} style should use the classic MDEF row height"
            );
            assert_eq!(
                styled.menu_item_width_extra(&styled_bus, &styled.menus[0].items[idx]),
                plain.menu_item_width_extra(&plain_bus, &plain.menus[0].items[idx]),
                "{label} style should not change mark/icon/command geometry"
            );

            let row_bottom = row_top + styled_height;
            let text_left = rect.1 + 18;
            let text_right =
                text_left + super::super::TrapDispatcher::fb_measure_string(text, 0, 12) + 8;
            let styled_only_pixel = ((row_top - 1)..(row_bottom + 2))
                .flat_map(|y| ((text_left - 2)..(text_right + 4)).map(move |x| (x, y)))
                .find(|(x, y)| {
                    screen_pixel_is_set(&styled_bus, styled_base, styled_row_bytes, *x, *y)
                        && !screen_pixel_is_set(&plain_bus, plain_base, plain_row_bytes, *x, *y)
                });
            let plain_only_pixel = ((row_top - 1)..(row_bottom + 2))
                .flat_map(|y| ((text_left - 2)..(text_right + 4)).map(move |x| (x, y)))
                .find(|(x, y)| {
                    screen_pixel_is_set(&plain_bus, plain_base, plain_row_bytes, *x, *y)
                        && !screen_pixel_is_set(&styled_bus, styled_base, styled_row_bytes, *x, *y)
                });
            assert!(
                styled_only_pixel.is_some() || plain_only_pixel.is_some(),
                "{label} SetItemStyle bit should produce a visible menu-text pixel difference"
            );
            row_top = row_bottom;
        }
    }

    // MTE 1992 p. 3-131 / HIG 1992 p. 54: an unavailable item stays visible
    // but dimmed, and MTE 1992 p. 3-30 says a colour screen shows dividers
    // and dimmed content as grey lines and glyphs (a black-and-white screen
    // gets the 50% grey pattern instead). System 7.5.3 under BasiliskII draws
    // Absolute Solitaire's all-disabled Edit menu that way — every row
    // legible in solid grey — so a dimmed row must not collapse to bare
    // background.
    #[test]
    fn draw_menu_dropdown_8bpp_dims_disabled_items_and_dividers_to_solid_gray() {
        let rect = (20, 20, 70, 140);
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 640, 0x303000, "Edit");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            menu,
            0x303040,
            "Undo/Z;-;Copy",
        );
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, menu);
        assert!(
            disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "DisableItem should disable the first item"
        );

        disp.draw_menu_dropdown(&mut bus, 0, rect);

        let background =
            super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0xFFFF; 3]).unwrap();
        let black = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0; 3]).unwrap();
        let gray =
            super::super::TrapDispatcher::fb_gray_pixel_index_between(&bus, [0xFFFF; 3], [0, 0, 0])
                .expect("8bpp test screen should express an intermediate shade");

        let row_pixels = |top: i16| -> Vec<u8> {
            (top..top + MENU_ROW_HEIGHT)
                .flat_map(|y| ((rect.1 + 1)..(rect.3 - 1)).map(move |x| (x, y)))
                .map(|(x, y)| screen_pixel_index(&bus, base, row_bytes, x, y))
                .collect()
        };

        let disabled = row_pixels(rect.0 + 1);
        assert!(
            disabled.iter().any(|&pixel| pixel == gray),
            "the disabled item's text should draw in the intermediate grey shade"
        );
        assert!(
            !disabled.iter().any(|&pixel| pixel == black),
            "no part of a dimmed item should stay full black"
        );

        // The divider is a solid grey line one pixel above the row midpoint.
        let divider_top = rect.0 + 1 + MENU_ROW_HEIGHT;
        let divider_y = divider_top + MENU_ROW_HEIGHT / 2 - 1;
        for x in (rect.1 + 1)..(rect.3 - 1) {
            assert_eq!(
                screen_pixel_index(&bus, base, row_bytes, x, divider_y),
                gray,
                "divider pixel at x={x}"
            );
        }
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, rect.1 + 4, divider_y - 2),
            background,
            "the divider should be a single line, not a filled row"
        );

        let enabled = row_pixels(divider_top + MENU_ROW_HEIGHT);
        assert!(
            enabled.iter().any(|&pixel| pixel == black),
            "an enabled item's text should still draw in full black"
        );
    }

    // MTE 1992 p. 3-30: a black-and-white screen has no intermediate shade,
    // so the definition procedure dims with the 50% grey pattern and draws
    // dividers as dotted lines. Imaging With QuickDraw 1994 p. 3-9 fixes the
    // pattern phase, so the surviving pixels are the ones where x + y is even.
    #[test]
    fn draw_menu_dropdown_1bpp_dims_disabled_items_with_the_gray_pattern() {
        let rect = (20, 16, 60, 120);
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 16u32;
        let base = bus.alloc(row_bytes * 96);
        disp.set_screen_mode_for_test(base, row_bytes, 128, 96, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 96);
        bus.write_long(crate::memory::globals::addr::SCRN_BASE, base);
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 641, 0x303100, "Edit");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x303140, "(Undo;-");

        disp.draw_menu_dropdown(&mut bus, 0, rect);

        let dimmed_row: Vec<(i16, i16)> = ((rect.0 + 1)..(rect.0 + 1 + MENU_ROW_HEIGHT))
            .flat_map(|y| ((rect.1 + 1)..(rect.3 - 1)).map(move |x| (x, y)))
            .collect();
        assert!(
            dimmed_row
                .iter()
                .any(|&(x, y)| screen_pixel_is_set(&bus, base, row_bytes, x, y)),
            "the dimmed item should still leave legible ink on a 1-bit screen"
        );
        assert!(
            dimmed_row
                .iter()
                .filter(|&&(x, y)| screen_pixel_is_set(&bus, base, row_bytes, x, y))
                .all(|&(x, y)| (x + y) % 2 == 0),
            "dimmed ink should survive only where the 50% grey pattern is on"
        );

        // A trailing divider is not laid out at all, so the box is one row
        // tall and its bottom frame sits directly under the dimmed item.
        assert_eq!(
            disp.menu_items_height(&bus, &disp.menus[0].items),
            MENU_ROW_HEIGHT,
            "a trailing divider should not claim a row"
        );
    }

    // Applications author the Apple menu as an About command plus a divider
    // so AppendResMenu has something to append below (MTE 1992 pp. 3-97 to
    // 3-98). With no Apple Menu Items to append, System 7.5.3 under
    // BasiliskII draws Absolute Solitaire's Apple menu exactly one item tall
    // rather than leaving a dangling divider.
    #[test]
    fn menu_layout_gives_a_trailing_divider_no_row() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 642, 0x303200, "\u{14}");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            menu,
            0x303240,
            "About Systemless;-",
        );

        assert_eq!(disp.menus[0].items.len(), 2, "both items stay in the menu");
        assert_eq!(
            disp.menu_items_height(&bus, &disp.menus[0].items),
            MENU_ROW_HEIGHT,
            "only the About command claims a row"
        );

        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x303280, "Note Pad");
        assert_eq!(
            disp.menu_items_height(&bus, &disp.menus[0].items),
            MENU_ROW_HEIGHT * 3,
            "the divider claims its row again once an item follows it"
        );
    }

    #[test]
    fn draw_menu_dropdown_8bpp_uses_menucinfo_background_and_item_name_color() {
        let rect = (20, 20, 38, 110);
        let menu_id = 620;
        let red = (0xFFFF, 0, 0);
        let green = (0, 0xFFFF, 0);
        let blue = (0, 0, 0xFFFF);
        let black = (0, 0, 0);
        let white = (0xFFFF, 0xFFFF, 0xFFFF);

        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 96);
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x302D00, "Color");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302D40, "Open");

        let entries_ptr = bus.alloc((2 * MC_ENTRY_SIZE) as u32);
        write_mc_entry_colors(&mut bus, entries_ptr, menu_id, 0, black, white, green, red);
        write_mc_entry_colors(
            &mut bus,
            entries_ptr + MC_ENTRY_SIZE as u32,
            menu_id,
            1,
            green,
            blue,
            green,
            red,
        );
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, entries_ptr);
        bus.write_word(TEST_SP + 4, 2);
        assert!(
            disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetMCEntries should install title and item color entries"
        );

        disp.draw_menu_dropdown(&mut bus, 0, rect);

        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, 24, 22),
            35,
            "title RGB4 full red should fill the 8bpp dropdown background"
        );

        let text_left = rect.1 + 18;
        let text_top = rect.0 + 1;
        let text_bottom = text_top + 16;
        let text_right = text_left + super::super::TrapDispatcher::fb_measure_string("Open", 0, 12);
        let has_blue_text_pixel = (text_top..text_bottom)
            .flat_map(|y| (text_left..text_right).map(move |x| (x, y)))
            .any(|(x, y)| screen_pixel_index(&bus, base, row_bytes, x, y) == 210);
        assert!(
            has_blue_text_pixel,
            "item RGB2 full blue should draw the menu item name"
        );

        let has_title_default_green_text_pixel = (text_top..text_bottom)
            .flat_map(|y| (text_left..text_right).map(move |x| (x, y)))
            .any(|(x, y)| screen_pixel_index(&bus, base, row_bytes, x, y) == 185);
        assert!(
            !has_title_default_green_text_pixel,
            "item RGB2 should override the title RGB3 default item color"
        );
    }

    #[test]
    fn draw_menu_dropdown_8bpp_uses_item_name_color_for_black_and_white_icons() {
        let rect = (20, 20, 70, 150);
        let menu_id = 621;
        let red = (0xFFFF, 0, 0);
        let green = (0, 0xFFFF, 0);
        let blue = (0, 0, 0xFFFF);
        let black = (0, 0, 0);
        let white = (0xFFFF, 0xFFFF, 0xFFFF);

        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 100);
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x302E00, "Color");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302E40, "Big;Small");
        disp.menus[0].items[0].icon = 7;
        disp.menus[0].items[1].icon = 8;
        disp.menus[0].items[1].key_equiv = MENU_KEY_SMALL_ICON;
        let icon = menu_icon_source_with_left_stripe();
        let sicn = sicn_source_with_left_stripe();
        disp.install_test_resource(&mut bus, *b"ICON", 263, &icon);
        disp.install_test_resource(&mut bus, *b"SICN", 264, &sicn);

        let entries_ptr = bus.alloc((2 * MC_ENTRY_SIZE) as u32);
        write_mc_entry_colors(&mut bus, entries_ptr, menu_id, 0, black, white, green, red);
        write_mc_entry_colors(
            &mut bus,
            entries_ptr + MC_ENTRY_SIZE as u32,
            menu_id,
            1,
            green,
            blue,
            green,
            red,
        );
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, entries_ptr);
        bus.write_word(TEST_SP + 4, 2);
        assert!(
            disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetMCEntries should install title and item color entries"
        );

        disp.draw_menu_dropdown(&mut bus, 0, rect);

        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, rect.1 + 4, rect.0 + 1),
            210,
            "normal black-and-white ICON pixels should use the item RGB2 name color"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, rect.1 + 4, rect.0 + 35),
            185,
            "SICN pixels should fall back to the title RGB3 default item color"
        );
    }

    #[test]
    fn draw_menu_bar_8bpp_uses_menucinfo_bar_and_title_colors() {
        let file_id = 622;
        let edit_id = 623;
        let red = (0xFFFF, 0, 0);
        let green = (0, 0xFFFF, 0);
        let blue = (0, 0, 0xFFFF);
        let white = (0xFFFF, 0xFFFF, 0xFFFF);

        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 64);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, file_id, 0x302F00, "File");
        let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, edit_id, 0x302F40, "Edit");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        insert_menu(&mut disp, &mut cpu, &mut bus, edit);

        let entries_ptr = bus.alloc((2 * MC_ENTRY_SIZE) as u32);
        write_mc_entry_colors(&mut bus, entries_ptr, 0, 0, green, white, green, red);
        write_mc_entry_colors(
            &mut bus,
            entries_ptr + MC_ENTRY_SIZE as u32,
            file_id,
            0,
            blue,
            red,
            green,
            white,
        );
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, entries_ptr);
        bus.write_word(TEST_SP + 4, 2);
        assert!(
            disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetMCEntries should install menu bar and title color entries"
        );

        disp.draw_menu_bar_to_fb(&mut bus);

        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, 5, 5),
            35,
            "menu bar entry RGB4 full red should fill the 8bpp menu bar"
        );

        let regions = disp.menu_title_regions();
        let file_has_blue_title_pixel = (1..19).any(|y| {
            (regions[0].0..regions[0].1)
                .any(|x| screen_pixel_index(&bus, base, row_bytes, x, y) == 210)
        });
        assert!(
            file_has_blue_title_pixel,
            "menu title entry RGB1 full blue should draw that menu title"
        );

        let edit_has_green_title_pixel = (1..19).any(|y| {
            (regions[1].0..regions[1].1)
                .any(|x| screen_pixel_index(&bus, base, row_bytes, x, y) == 185)
        });
        assert!(
            edit_has_green_title_pixel,
            "menu bar entry RGB1 full green should draw titles without their own entry"
        );
    }

    #[test]
    fn draw_menu_dropdown_8bpp_highlight_swaps_background_with_hilite_color() {
        let rect = (20, 20, 38, 110);
        let menu_id = 624;
        let red = (0xFFFF, 0, 0);
        let green = (0, 0xFFFF, 0);
        let blue = (0, 0, 0xFFFF);
        let black = (0, 0, 0);
        let white = (0xFFFF, 0xFFFF, 0xFFFF);

        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.hilite_color = green;
        let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 96);
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x303000, "Color");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x303040, "Open");

        let entries_ptr = bus.alloc((2 * MC_ENTRY_SIZE) as u32);
        write_mc_entry_colors(&mut bus, entries_ptr, menu_id, 0, black, white, green, red);
        write_mc_entry_colors(
            &mut bus,
            entries_ptr + MC_ENTRY_SIZE as u32,
            menu_id,
            1,
            green,
            blue,
            green,
            red,
        );
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, entries_ptr);
        bus.write_word(TEST_SP + 4, 2);
        assert!(
            disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetMCEntries should install dropdown color entries"
        );

        disp.draw_menu_dropdown(&mut bus, 0, rect);

        let bg_x = rect.1 + 70;
        let bg_y = rect.0 + 5;
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
            35,
            "precondition: dropdown background should be the MenuCInfo red"
        );
        let hilite_swap_x = rect.1 + 80;
        let hilite_swap_y = rect.0 + 5;
        bus.write_byte(
            base + (hilite_swap_y as u32) * row_bytes + hilite_swap_x as u32,
            185,
        );

        let text_left = rect.1 + 18;
        let text_top = rect.0 + 1;
        let text_bottom = text_top + 16;
        let text_right = text_left + super::super::TrapDispatcher::fb_measure_string("Open", 0, 12);
        let blue_text_pixel = (text_top..text_bottom)
            .flat_map(|y| (text_left..text_right).map(move |x| (x, y)))
            .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == 210)
            .expect("precondition: item RGB2 blue text should draw");

        disp.invert_dropdown_item_rect(&mut bus, 0, rect, 1);

        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
            185,
            "8bpp menu highlighting should swap the background to HiliteColor"
        );
        assert_ne!(
            screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
            220,
            "8bpp menu highlighting must not use the indexed complement of red"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, hilite_swap_x, hilite_swap_y),
            35,
            "8bpp menu highlighting should swap existing HiliteColor pixels back to the background"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, blue_text_pixel.0, blue_text_pixel.1),
            210,
            "8bpp menu highlighting should leave non-background MenuCInfo text colors unchanged"
        );

        disp.invert_dropdown_item_rect(&mut bus, 0, rect, 1);

        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
            35,
            "highlighting the same row twice should restore the background"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, hilite_swap_x, hilite_swap_y),
            185,
            "highlighting the same row twice should restore existing HiliteColor pixels"
        );
    }

    #[test]
    fn draw_menu_bar_8bpp_highlight_swaps_background_with_hilite_color() {
        let file_id = 625;
        let red = (0xFFFF, 0, 0);
        let green = (0, 0xFFFF, 0);
        let blue = (0, 0, 0xFFFF);
        let white = (0xFFFF, 0xFFFF, 0xFFFF);

        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.hilite_color = green;
        let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 64);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, file_id, 0x303100, "File");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);

        let entries_ptr = bus.alloc(MC_ENTRY_SIZE as u32);
        write_mc_entry_colors(&mut bus, entries_ptr, 0, 0, blue, white, blue, red);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, entries_ptr);
        bus.write_word(TEST_SP + 4, 1);
        assert!(
            disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetMCEntries should install menu bar color entry"
        );

        disp.draw_menu_bar_to_fb(&mut bus);

        let regions = disp.menu_title_regions();
        let bg_x = regions[0].0 + 1;
        let bg_y = 5;
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
            35,
            "precondition: menu title background should be the MenuCInfo red bar"
        );
        let hilite_swap_x = regions[0].1 - 2;
        let hilite_swap_y = 5;
        bus.write_byte(
            base + (hilite_swap_y as u32) * row_bytes + hilite_swap_x as u32,
            185,
        );
        let blue_title_pixel = (1..19)
            .flat_map(|y| (regions[0].0..regions[0].1).map(move |x| (x, y)))
            .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == 210)
            .expect("precondition: menu-bar RGB1 blue title should draw");

        disp.highlight_menu_title(&mut bus, 0);

        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
            185,
            "8bpp menu-title highlighting should swap the bar background to HiliteColor"
        );
        assert_ne!(
            screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
            220,
            "8bpp menu-title highlighting must not use the indexed complement of red"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, hilite_swap_x, hilite_swap_y),
            35,
            "8bpp menu-title highlighting should swap existing HiliteColor pixels back to the bar background"
        );
        assert_eq!(
            screen_pixel_index(
                &bus,
                base,
                row_bytes,
                blue_title_pixel.0,
                blue_title_pixel.1
            ),
            210,
            "8bpp menu-title highlighting should leave non-background title colors unchanged"
        );

        disp.highlight_menu_title(&mut bus, 0);

        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
            35,
            "highlighting the same title twice should restore the bar background"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, hilite_swap_x, hilite_swap_y),
            185,
            "highlighting the same title twice should restore existing HiliteColor pixels"
        );
    }

    #[test]
    fn draw_menu_bar_plain_8bpp_highlight_swaps_logical_black_white() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 64);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 626, 0x303200, "File");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);

        disp.draw_menu_bar_to_fb(&mut bus);

        let regions = disp.menu_title_regions();
        let classic_left = regions[0].0 - 2;
        let right = regions[0].1;
        let white =
            super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0xFFFF; 3]).unwrap();
        let black = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0; 3]).unwrap();
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, classic_left, 1),
            white,
            "precondition: plain menu-title background should be logical white"
        );
        let title_pixel = (1..19)
            .flat_map(|y| (classic_left..right).map(move |x| (x, y)))
            .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == black)
            .expect("precondition: plain title should draw logical black text");

        disp.highlight_menu_title(&mut bus, 0);

        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, classic_left, 1),
            black,
            "plain 8bpp menu-title highlight should invert white background to black"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
            white,
            "plain 8bpp menu-title highlight should invert black title text to white"
        );
    }

    #[test]
    fn open_menu_dropdown_uses_system7_attached_pulldown_chrome() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 627, 0x303300, "File");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            file,
            0x303340,
            "New/N;Open/O;Close/W",
        );
        disp.menus[0].items[1].mark = 0x12;
        disp.menus[0].items[2].style = 0x01;
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        disp.draw_menu_bar_to_fb(&mut bus);

        let regions = disp.menu_title_regions();
        let expected_left = regions[0].0 - 2;
        let expected_width = disp.menus[0]
            .items
            .iter()
            .map(|item| {
                super::super::TrapDispatcher::fb_measure_string(&item.text, 0, 12)
                    + disp.menu_item_width_extra(&bus, item)
                    + super::super::TrapDispatcher::menu_item_pulldown_padding(item)
            })
            .max()
            .unwrap()
            .max(regions[0].1 - regions[0].0 + 20);
        let expected_rect = (
            20,
            expected_left,
            20 + disp.menu_items_height(&bus, &disp.menus[0].items) + 1,
            expected_left + expected_width,
        );
        let white =
            super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0xFFFF; 3]).unwrap();
        let black = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0; 3]).unwrap();

        disp.open_menu_dropdown(&mut bus, 0, TEST_SP);

        assert_eq!(
            disp.menu_tracking.as_ref().unwrap().dropdown_rect,
            expected_rect,
            "attached pull-down display rect should use the System 7 MDEF padding, not CalcMenuSize's stored width"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, expected_left + 1, expected_rect.0),
            white,
            "attached pull-down should not draw a separate top horizontal border"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, expected_left, expected_rect.0),
            black,
            "attached pull-down should still draw its left vertical border"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, expected_rect.3 - 1, expected_rect.0),
            black,
            "attached pull-down should still draw its right vertical border"
        );
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, expected_left, 1),
            black,
            "opening a pull-down should highlight the attached menu title"
        );
    }

    #[test]
    fn open_menu_dropdown_uses_system7_plain_item_padding() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (_base, _row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 180, 120);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let style = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 628, 0x303500, "Style");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            style,
            0x303540,
            "Plain;Underline;-;Wide Underline",
        );
        insert_menu(&mut disp, &mut cpu, &mut bus, style);
        disp.draw_menu_bar_to_fb(&mut bus);

        let regions = disp.menu_title_regions();
        let expected_left = regions[0].0 - 2;
        let expected_width = disp.menus[0]
            .items
            .iter()
            .map(|item| {
                super::super::TrapDispatcher::fb_measure_string(&item.text, 0, 12)
                    + disp.menu_item_width_extra(&bus, item)
                    + super::super::TrapDispatcher::menu_item_pulldown_padding(item)
            })
            .max()
            .unwrap()
            .max(regions[0].1 - regions[0].0 + 20);
        let wide_item_width =
            super::super::TrapDispatcher::fb_measure_string("Wide Underline", 0, 12) + 26;

        disp.open_menu_dropdown(&mut bus, 0, TEST_SP);

        assert_eq!(
            expected_width, wide_item_width,
            "plain no-mark/no-command rows should use the System 7 live pull-down padding"
        );
        assert_eq!(
            disp.menu_tracking.as_ref().unwrap().dropdown_rect,
            (
                20,
                expected_left,
                20 + disp.menu_items_height(&bus, &disp.menus[0].items) + 1,
                expected_left + expected_width,
            ),
            "live pull-down geometry should match the System 7 plain-row width reference"
        );
    }

    #[test]
    fn open_menu_dropdown_uses_system7_icon_column_geometry_without_icon_resources() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let (_base, _row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 240, 220);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 629, 0x303700, "Icons");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            menu,
            0x303740,
            "Folder View/F;Document/D;Checked Icon/C;Disabled Icon/X;(-;No Icon/N",
        );
        for (idx, item) in disp.menus[0].items.iter_mut().take(4).enumerate() {
            item.icon = (idx + 1) as u8;
        }
        disp.menus[0].items[2].mark = 0x12;
        disp.menus[0].items[3].enabled = false;
        insert_menu(&mut disp, &mut cpu, &mut bus, menu);
        disp.draw_menu_bar_to_fb(&mut bus);

        let regions = disp.menu_title_regions();
        let expected_left = regions[0].0 - 2;
        let expected_height = 34 + 34 + 34 + 34 + 16 + 16 + 1;
        let expected_width = disp.menus[0]
            .items
            .iter()
            .map(|item| {
                super::super::TrapDispatcher::fb_measure_string(&item.text, 0, 12)
                    + disp.menu_item_width_extra(&bus, item)
                    + super::super::TrapDispatcher::menu_item_pulldown_padding(item)
            })
            .max()
            .unwrap()
            .max(regions[0].1 - regions[0].0 + 20);

        disp.open_menu_dropdown(&mut bus, 0, TEST_SP);
        let rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect;

        assert_eq!(
            disp.menu_items_height(&bus, &disp.menus[0].items),
            expected_height - 1,
            "icon-column menus should use 34px normal ICON rows and a standard separator row"
        );
        assert_eq!(
            rect,
            (
                20,
                expected_left,
                20 + expected_height,
                expected_left + expected_width,
            ),
            "normal icon-number rows without resources should still reserve System 7 icon-column pull-down geometry"
        );
        assert_eq!(
            super::super::TrapDispatcher::menu_item_pulldown_padding(&disp.menus[0].items[0]),
            6,
            "normal icon-column command rows should use the System 7 live pull-down padding"
        );
        assert_eq!(
            disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 34),
            1,
            "hit testing should include the full first 34px normal ICON row"
        );
        assert_eq!(
            disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 128),
            0,
            "hit testing inside the fourth 34px normal ICON row should respect its disabled state"
        );
        assert_eq!(
            disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 137),
            0,
            "hit testing in the separator row should not select an item"
        );
        assert_eq!(
            disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 152),
            0,
            "hit testing should leave the full separator row unselectable"
        );
        assert_eq!(
            disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 153),
            6,
            "hit testing below the icon-column separator should reach the following item"
        );
    }

    #[test]
    fn draw_menu_dropdown_reduced_icon_resource_draws_app_icon_not_theme_placeholder() {
        let rect = (20, 20, 38, 140);
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 64;
        let base = bus.alloc(row_bytes * 342);
        disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);

        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 612, 0x302500, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302540, "Pick");
        disp.menus[0].items[0].icon = 7;
        disp.menus[0].items[0].key_equiv = MENU_KEY_REDUCED_ICON;
        let icon = menu_icon_source_with_left_stripe();
        disp.install_test_resource(&mut bus, *b"ICON", 263, &icon);

        disp.draw_menu_dropdown(&mut bus, 0, rect);

        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, 23, 22),
            "menu item should draw the app's reduced ICON resource in the icon slot"
        );
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, 36, 28),
            "an explicit ICON resource should suppress systemless icon placeholder chrome"
        );
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, 124, 28),
            "$1D reduced-icon selector should not be rendered as command-key chrome"
        );
    }

    #[test]
    fn sicn_menu_icon_resource_draws_small_icon_without_row_growth_or_theme_placeholder() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 64;
        let base = bus.alloc(row_bytes * 342);
        disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);

        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 615, 0x302800, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302840, "Pick;Next");
        disp.menus[0].items[0].icon = 7;
        disp.menus[0].items[0].key_equiv = MENU_KEY_SMALL_ICON;
        let sicn = sicn_source_with_left_stripe();
        disp.install_test_resource(&mut bus, *b"SICN", 263, &sicn);
        insert_menu(&mut disp, &mut cpu, &mut bus, menu);

        disp.open_menu_dropdown(&mut bus, 0, TEST_SP);
        let rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect;

        assert_eq!(
            rect.2 - rect.0,
            16 + 16 + 1,
            "SICN menu items should keep standard 16px attached pull-down geometry"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 4, rect.0 + 2),
            "SICN resource should draw the app's small icon in the icon slot"
        );
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 16, rect.0 + 8),
            "an explicit SICN resource should suppress systemless icon placeholder chrome"
        );
        // Command-key chrome is right-aligned; sample the SICN item's
        // command zone near the right edge (re-baselined from the old
        // left+8 sample, which landed on the second item's menu text and
        // became a false tripwire after the Jarrah/Chicago 12 glyph
        // redraw — glyph appearance changed, menu logic did not).
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 8, rect.2 - 8),
            "$1E SICN selector should not be rendered as command-key chrome"
        );
    }

    #[test]
    fn cicn_menu_icon_resource_precedes_sicn_and_suppresses_theme_placeholder() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 64;
        let base = bus.alloc(row_bytes * 342);
        disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);

        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 616, 0x302900, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302940, "Pick;Next");
        disp.menus[0].items[0].icon = 7;
        disp.menus[0].items[0].key_equiv = MENU_KEY_SMALL_ICON;
        let cicn = cicn_source_with_left_stripe(24, 20);
        let sicn = sicn_source_with_right_stripe();
        disp.install_test_resource(&mut bus, *b"cicn", 263, &cicn);
        disp.install_test_resource(&mut bus, *b"SICN", 263, &sicn);
        insert_menu(&mut disp, &mut cpu, &mut bus, menu);

        disp.open_menu_dropdown(&mut bus, 0, TEST_SP);
        let rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect;

        assert_eq!(
            rect.2 - rect.0,
            20 + 16 + 1,
            "cicn menu rows should grow to the cicn resource rectangle height in an attached pull-down"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 4, rect.0 + 2),
            "cicn resource should draw before the SICN fallback"
        );
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 17, rect.0 + 2),
            "SICN fallback pixels must not draw when a cicn resource exists"
        );
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 16, rect.0 + 8),
            "an explicit cicn resource should suppress systemless icon placeholder chrome"
        );
        assert_eq!(
            disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 20),
            1,
            "hit testing should include the full cicn-height first row"
        );
        assert_eq!(
            disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 22),
            2,
            "hit testing below the cicn-height row should reach the next item"
        );
    }

    #[test]
    fn calcmenusize_uses_cicn_geometry_instead_of_icon_fallback_geometry() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 617, 0x302A00, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302A40, "Pick;Next");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 7);
        bus.write_word(TEST_SP + 2, 1);
        bus.write_long(TEST_SP + 4, menu);
        assert!(
            disp.dispatch_menu(true, 0x140, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetItemIcon should install a normal icon menu item"
        );

        let cicn = cicn_source_with_left_stripe(24, 20);
        let icon = menu_icon_source_with_left_stripe();
        disp.install_test_resource(&mut bus, *b"cicn", 263, &cicn);
        disp.install_test_resource(&mut bus, *b"ICON", 263, &icon);

        let menu_ptr = bus.read_long(menu);
        bus.write_word(menu_ptr + 4, 0);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, menu);
        assert!(
            disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "CalcMenuSize should succeed"
        );

        assert_eq!(
            bus.read_word(menu_ptr + 4) as i16,
            20 + 16 + 2,
            "CalcMenuSize should use cicn resource height instead of normal ICON's 32px row"
        );
    }

    #[test]
    fn normal_icon_resource_expands_dropdown_geometry_and_hit_testing() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 64;
        let base = bus.alloc(row_bytes * 342);
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);

        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 613, 0x302600, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302640, "Pick;Next");
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 7);
        bus.write_word(TEST_SP + 2, 1);
        bus.write_long(TEST_SP + 4, menu);
        assert!(
            disp.dispatch_menu(true, 0x140, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetItemIcon should install a normal ICON menu item"
        );
        let icon = menu_icon_source_with_left_stripe();
        disp.install_test_resource(&mut bus, *b"ICON", 263, &icon);
        insert_menu(&mut disp, &mut cpu, &mut bus, menu);

        disp.open_menu_dropdown(&mut bus, 0, TEST_SP);
        let rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect;

        assert_eq!(
            rect.2 - rect.0,
            34 + 16 + 1,
            "normal ICON row should enlarge the live attached pull-down height around its 32px icon slot"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 4, rect.0 + 1),
            "normal ICON resource should draw at full 32x32 size in the first row"
        );
        assert_eq!(
            disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 34),
            1,
            "hit testing near the bottom of the 34px normal ICON row should still hit item 1"
        );
        assert_eq!(
            disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 36),
            2,
            "hit testing below the normal ICON row should hit the following item"
        );
    }

    #[test]
    fn calcmenusize_accounts_for_normal_icon_row_height() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 614, 0x302700, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302740, "Pick;Next");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 7);
        bus.write_word(TEST_SP + 2, 1);
        bus.write_long(TEST_SP + 4, menu);
        assert!(
            disp.dispatch_menu(true, 0x140, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetItemIcon should install a normal ICON menu item"
        );

        let menu_ptr = bus.read_long(menu);
        bus.write_word(menu_ptr + 4, 0);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, menu);
        assert!(
            disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "CalcMenuSize should succeed"
        );

        assert_eq!(
            bus.read_word(menu_ptr + 4) as i16,
            34 + 16 + 2,
            "CalcMenuSize should write summed row heights including normal ICON rows"
        );
    }

    #[test]
    fn popupmenuselect_nohit_path_preserves_stack_and_returns_zero() {
        // Inside Macintosh Volume V (1986), p. V-229:
        // PopUpMenuSelect(menu, top, left, popUpItem) returns the
        // selected item. IM:V V-241 and MTE 1992 p. 3-120 require the
        // pop-up menu to be in the MenuList; an uninserted menu returns
        // no selection without disturbing the caller stack.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.menu_bar_hidden = false;
        disp.mouse_button = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x302000, "Pop");

        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0); // popUpItem
        bus.write_word(sp + 2, 0xFC18); // -1000
        bus.write_word(sp + 4, 0xFC18); // -1000
        bus.write_long(sp + 6, menu);
        bus.write_long(sp + 10, 0xDEAD_BEEF); // result placeholder

        let result = disp.dispatch_menu(true, 0x00B, &mut cpu, &mut bus);
        assert!(result.is_some(), "PopUpMenuSelect should be handled");
        assert!(result.unwrap().is_ok(), "PopUpMenuSelect should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 10,
            "PopUpMenuSelect should consume the 10-byte Pascal argument frame"
        );
        assert_eq!(
            bus.read_long(sp + 10),
            0,
            "PopUpMenuSelect no-hit path should return 0"
        );
        assert!(
            disp.menu_tracking.is_none(),
            "uninserted pop-up menus should not seed tracking"
        );
    }

    // IM:I I-354: DeleteMenu removes only the specified menu ID from the
    // current menu list and leaves other inserted menus present.
    #[test]
    fn deletemenu_removes_only_target_menu_id_from_current_list() {
        let (mut disp, mut cpu, mut bus) = setup();
        let left_id = 240i16;
        let right_id = 241i16;
        let left = new_menu_with_title(&mut disp, &mut cpu, &mut bus, left_id, 0x30B300, "Left");
        let right = new_menu_with_title(&mut disp, &mut cpu, &mut bus, right_id, 0x30B400, "Right");
        insert_menu(&mut disp, &mut cpu, &mut bus, left);
        insert_menu(&mut disp, &mut cpu, &mut bus, right);

        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, left_id),
            left
        );
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, right_id),
            right
        );

        delete_menu_by_id(&mut disp, &mut cpu, &mut bus, left_id);

        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, left_id),
            0,
            "Deleted menu ID should no longer be in the current menu list"
        );
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, right_id),
            right,
            "Deleting one menu ID should not remove other inserted menus"
        );
    }

    // IM:V 1986 p. V-244: DeleteMenu removes all color entries for the
    // deleted menu ID from the application's menu color information table.
    #[test]
    fn deletemenu_removes_all_menu_color_entries_for_menu_id() {
        let (mut disp, mut cpu, mut bus) = setup();
        let target_id = 511i16;
        let other_id = 512i16;
        let target =
            new_menu_with_title(&mut disp, &mut cpu, &mut bus, target_id, 0x30B4C0, "Target");
        let other = new_menu_with_title(&mut disp, &mut cpu, &mut bus, other_id, 0x30B500, "Other");
        insert_menu(&mut disp, &mut cpu, &mut bus, target);
        insert_menu(&mut disp, &mut cpu, &mut bus, other);
        set_mc_entries_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            &[
                (target_id, 0, 0x2100),
                (target_id, 1, 0x2200),
                (target_id, 2, 0x2300),
                (other_id, 1, 0x2400),
                (0, 0, 0x2500),
            ],
        );

        delete_menu_by_id(&mut disp, &mut cpu, &mut bus, target_id);

        assert_eq!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, target_id, 0),
            0,
            "DeleteMenu should remove the deleted menu title entry"
        );
        assert_eq!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, target_id, 1),
            0,
            "DeleteMenu should remove item entries for the deleted menu"
        );
        assert_eq!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, target_id, 2),
            0,
            "DeleteMenu should remove every entry for the deleted menu ID"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, other_id, 1),
            0,
            "DeleteMenu should preserve other menus' MenuCInfo entries"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 0, 0),
            0,
            "DeleteMenu should preserve the default MenuCInfo entry"
        );
    }

    // MTE 1992 p. 3-105: DeleteMenu removes from the menu list but does not
    // dispose the menu record memory (DisposeMenu is a separate call).
    #[test]
    fn deletemenu_does_not_dispose_menu_record_memory() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_id = 242i16;
        let title = "Temp";
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B500, title);
        let menu_ptr = bus.read_long(handle);
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);

        delete_menu_by_id(&mut disp, &mut cpu, &mut bus, menu_id);

        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
            0
        );
        assert_eq!(
            bus.read_long(handle),
            menu_ptr,
            "DeleteMenu should not clear or retarget the caller-held MenuHandle"
        );
        assert_eq!(
            bus.read_word(menu_ptr) as i16,
            menu_id,
            "MenuInfo memory should still be readable after DeleteMenu"
        );
        assert_eq!(bus.read_byte(menu_ptr + 14), title.len() as u8);
    }

    // IM:I p. I-352: DisposeMenu releases memory occupied by a menu
    // allocated with NewMenu.
    #[test]
    fn disposemenu_releases_newmenu_menuhandle_and_record_allocations() {
        let (mut disp, mut cpu, mut bus) = setup();
        let menu_id = 244i16;
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B800, "Temp");
        let menu_ptr = bus.read_long(handle);
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);

        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
            handle,
            "precondition: inserted menu should be discoverable by GetMHandle"
        );
        assert!(
            bus.get_alloc_size(handle).is_some(),
            "precondition: NewMenu should allocate a MenuHandle master pointer"
        );
        assert!(
            bus.get_alloc_size(menu_ptr).is_some(),
            "precondition: NewMenu should allocate a menu record block"
        );

        dispose_menu_by_handle(&mut disp, &mut cpu, &mut bus, handle);

        assert_eq!(
            bus.get_alloc_size(handle),
            None,
            "DisposeMenu should free the MenuHandle master-pointer allocation"
        );
        assert_eq!(
            bus.get_alloc_size(menu_ptr),
            None,
            "DisposeMenu should free the menu record allocation"
        );
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
            0,
            "Disposed menu should no longer be returned by GetMHandle"
        );
    }

    // IM:I p. I-352: PROCEDURE DisposeMenu(theMenu: MenuHandle) takes one
    // 4-byte argument.
    #[test]
    fn disposemenu_consumes_menuhandle_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0);

        let result = disp.dispatch_menu(true, 0x132, &mut cpu, &mut bus);
        assert!(result.is_some(), "DisposeMenu should be handled");
        assert!(result.unwrap().is_ok(), "DisposeMenu should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 4,
            "DisposeMenu should pop one MenuHandle argument (4 bytes)"
        );
    }

    // IM:I I-356: HiliteMenu(menuID) takes one INTEGER argument; callers pass
    // 0 to unhighlight menu titles. In HLE this closes active menu tracking.
    #[test]
    fn hilitemenu_pops_menuid_word_and_clears_active_tracking_state() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let menu_id = 243i16;
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B600, "File");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x30B700,
            "Open/O;Close/W",
        );
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);
        disp.open_menu_dropdown(&mut bus, 0, TEST_SP);
        assert!(
            disp.menu_tracking.is_some(),
            "precondition: dropdown tracking should be active"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        let result = disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus);
        assert!(result.is_some(), "HiliteMenu should be handled");
        assert!(result.unwrap().is_ok(), "HiliteMenu should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 2,
            "HiliteMenu should pop one menuID word"
        );
        assert!(
            disp.menu_tracking.is_none(),
            "HiliteMenu(0) should clear active menu tracking/highlight state"
        );
    }

    // Five HiliteMenu(0) dispatches in sequence preserve A7 cumulatively.
    // Per IM:I I-356 and MPW Universal Headers Menus.h, HiliteMenu is a
    // Tool-bit Pascal PROCEDURE that pops 2 bytes per call and writes no
    // result slot, so five calls must advance A7 by exactly 10 bytes
    // (5 × 2 bytes).
    #[test]
    fn hilitemenu_five_call_composition_advances_stack_by_ten_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        // Pre-push five 2-byte menuID=0 arguments.
        for i in 0..5u32 {
            bus.write_word(sp_before.wrapping_sub((i + 1) * 2), 0);
        }
        cpu.write_reg(Register::A7, sp_before.wrapping_sub(10));

        for _ in 0..5 {
            let result = disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus);
            assert!(result.is_some(), "HiliteMenu should be handled");
            assert!(result.unwrap().is_ok(), "HiliteMenu should succeed");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "5 HiliteMenu(0) calls should pop 5×2 = 10 bytes cumulatively"
        );
    }

    // Five FlashMenuBar(0) dispatches in sequence preserve A7 cumulatively.
    // Per IM:I I-361 and MPW Universal Headers Menus.h, FlashMenuBar is a
    // Tool-bit Pascal PROCEDURE that pops 2 bytes per call and writes no
    // result slot, so five calls must advance A7 by exactly 10 bytes
    // (5 × 2 bytes).
    #[test]
    fn flashmenubar_five_call_composition_advances_stack_by_ten_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        for i in 0..5u32 {
            bus.write_word(sp_before.wrapping_sub((i + 1) * 2), 0);
        }
        cpu.write_reg(Register::A7, sp_before.wrapping_sub(10));

        for _ in 0..5 {
            let result = disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus);
            assert!(result.is_some(), "FlashMenuBar should be handled");
            assert!(result.unwrap().is_ok(), "FlashMenuBar should succeed");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "5 FlashMenuBar(0) calls should pop 5×2 = 10 bytes cumulatively"
        );
    }

    #[test]
    fn flashmenubar_zero_inverts_and_restores_top_menu_bar_strip() {
        // IM:I I-361: FlashMenuBar(0) inverts the entire menu bar; calling it
        // again blinks the menu bar back.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 64;
        let base = bus.alloc(row_bytes * 342);
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        disp.menu_bar_hidden = false;
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 701, 0x306000, "File");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let regions = disp.menu_title_regions();
        let title_pixel = (1..19)
            .flat_map(|y| (regions[0].0..regions[0].1).map(move |x| (x, y)))
            .find(|(x, y)| screen_pixel_is_set(&bus, base, row_bytes, *x, *y))
            .expect("precondition: menu title should draw before flashing");
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, 200, 5),
            "precondition: menu-bar background should start white"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, 0, 0),
            "precondition: DrawMenuBar should stamp the top corner mask"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, 200, 5),
            "FlashMenuBar(0) should invert the menu-bar background"
        );
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
            "FlashMenuBar(0) should invert title glyph pixels too"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, 0, 0),
            "FlashMenuBar(0) should preserve the top corner mask"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, 200, 5),
            "a second FlashMenuBar(0) call should restore the background"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
            "a second FlashMenuBar(0) call should restore title glyph pixels"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, 0, 0),
            "a second FlashMenuBar(0) call should keep the top corner mask"
        );
    }

    // Five MenuChoice() Pascal FUNCTION dispatches in sequence preserve
    // A7 cumulatively. Per IM:MTb 1992 p. 3-118 and MPW Universal Headers
    // Menus.h, MenuChoice is a parameterless Tool-bit Pascal FUNCTION
    // returning LongInt — the caller pre-pushes a 4-byte result slot,
    // the trap writes [SP+0] without modifying A7, and the caller pops
    // the slot. Wrapping each dispatch in a manual pre-push/post-pop
    // pair checks that the trap itself leaves A7 unchanged across
    // five successive calls.
    #[test]
    fn menuchoice_pascal_function_preserves_stack_across_five_calls() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        for _ in 0..5 {
            let slot = cpu.read_reg(Register::A7).wrapping_sub(4);
            bus.write_long(slot, 0xDEADBEEF);
            cpu.write_reg(Register::A7, slot);

            let result = disp.dispatch_menu(true, 0x266, &mut cpu, &mut bus);
            assert!(result.is_some(), "MenuChoice should be handled");
            assert!(result.unwrap().is_ok(), "MenuChoice should succeed");

            assert_eq!(
                cpu.read_reg(Register::A7),
                slot,
                "MenuChoice must leave A7 unchanged across the trap dispatch"
            );

            let _result_val = bus.read_long(slot);
            cpu.write_reg(Register::A7, cpu.read_reg(Register::A7).wrapping_add(4));
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "5 MenuChoice() FUNCTION call sequences must net-balance A7"
        );
    }

    // Exercises the MenuChoice lowmem read path by seeding lowmem
    // MenuDisable directly. Per IM:MTb 1992 p. 3-118..3-119,
    // MenuChoice returns the packed (menuID, itemNumber) stored in
    // MenuDisable when MenuSelect / MenuKey have tracked a disabled
    // item. This test seeds the lowmem word and checks that
    // the trap writes the same LongInt into the result slot while
    // still preserving A7 across the Pascal FUNCTION call sequence.
    #[test]
    fn menuchoice_reads_menu_disable_lowmem_value() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let expected = 0x11223344u32;
        let slot = sp_before.wrapping_sub(4);

        bus.write_long(crate::memory::globals::addr::MENU_DISABLE, expected);
        bus.write_long(slot, 0xDEADBEEF);
        cpu.write_reg(Register::A7, slot);

        let result = disp.dispatch_menu(true, 0x266, &mut cpu, &mut bus);
        assert!(result.is_some(), "MenuChoice should be handled");
        assert!(result.unwrap().is_ok(), "MenuChoice should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            slot,
            "MenuChoice must leave A7 unchanged across the trap dispatch"
        );
        assert_eq!(
            bus.read_long(slot),
            expected,
            "MenuChoice should return the seeded MenuDisable value"
        );
        cpu.write_reg(Register::A7, slot.wrapping_add(4));
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "MenuChoice should net-balance the Pascal FUNCTION call sequence"
        );
    }

    // Menu Color Manager family — per IM:V 1986 pp. V-247..V-248 the
    // family is six Tool-bit Pascal routines with the following stack
    // disciplines:
    //   AA60 DelMCEntries  — PROCEDURE pop-4 (2xINTEGER)
    //   AA61 GetMCInfo     — FUNCTION  parameterless + 4-byte result slot
    //   AA62 SetMCInfo     — PROCEDURE pop-4 (1xHandle)
    //   AA63 DispMCInfo    — PROCEDURE pop-4 (1xHandle)
    //   AA64 GetMCEntry    — FUNCTION  2xINTEGER + 4-byte result slot
    //   AA65 SetMCEntries  — PROCEDURE pop-6 (1xINTEGER + 1xPtr)
    // This test dispatches 5 successive calls of each trap with the
    // appropriate pre-pushed Pascal arg frame and (for FUNCTIONs) result
    // slot, asserting cumulative A7 net-balance across each family.
    #[test]
    fn menu_color_family_five_call_compositions_preserve_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();

        // AA60 DelMCEntries — PROCEDURE pop-4 (2xINTEGER each call).
        // 5 calls × 4 bytes = 20 bytes consumed total.
        let sp0 = cpu.read_reg(Register::A7);
        for i in 0..5u32 {
            bus.write_long(sp0.wrapping_sub((i + 1) * 4), 0);
        }
        cpu.write_reg(Register::A7, sp0.wrapping_sub(20));
        for _ in 0..5 {
            disp.dispatch_menu(true, 0x260, &mut cpu, &mut bus)
                .expect("DelMCEntries handled")
                .expect("DelMCEntries Ok");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp0,
            "5 DelMCEntries calls should pop 5×4 = 20 bytes cumulatively"
        );

        // AA61 GetMCInfo — FUNCTION parameterless + 4-byte result slot.
        // 5 calls, each with manual pre-push/post-pop of the slot.
        let sp1 = cpu.read_reg(Register::A7);
        for _ in 0..5 {
            let slot = cpu.read_reg(Register::A7).wrapping_sub(4);
            bus.write_long(slot, 0xDEADBEEF);
            cpu.write_reg(Register::A7, slot);
            disp.dispatch_menu(true, 0x261, &mut cpu, &mut bus)
                .expect("GetMCInfo handled")
                .expect("GetMCInfo Ok");
            assert_eq!(
                cpu.read_reg(Register::A7),
                slot,
                "GetMCInfo must leave A7 unchanged across the trap dispatch"
            );
            cpu.write_reg(Register::A7, cpu.read_reg(Register::A7).wrapping_add(4));
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp1,
            "5 GetMCInfo() FUNCTION call sequences must net-balance A7"
        );

        // AA62 SetMCInfo — PROCEDURE pop-4 (1xHandle). 5 × 4 = 20 bytes.
        let sp2 = cpu.read_reg(Register::A7);
        for i in 0..5u32 {
            bus.write_long(sp2.wrapping_sub((i + 1) * 4), 0);
        }
        cpu.write_reg(Register::A7, sp2.wrapping_sub(20));
        for _ in 0..5 {
            disp.dispatch_menu(true, 0x262, &mut cpu, &mut bus)
                .expect("SetMCInfo handled")
                .expect("SetMCInfo Ok");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp2,
            "5 SetMCInfo calls should pop 5×4 = 20 bytes cumulatively"
        );

        // AA63 DispMCInfo — PROCEDURE pop-4 (1xHandle). 5 × 4 = 20 bytes.
        let sp3 = cpu.read_reg(Register::A7);
        for i in 0..5u32 {
            bus.write_long(sp3.wrapping_sub((i + 1) * 4), 0);
        }
        cpu.write_reg(Register::A7, sp3.wrapping_sub(20));
        for _ in 0..5 {
            disp.dispatch_menu(true, 0x263, &mut cpu, &mut bus)
                .expect("DispMCInfo handled")
                .expect("DispMCInfo Ok");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp3,
            "5 DispMCInfo calls should pop 5×4 = 20 bytes cumulatively"
        );

        // AA64 GetMCEntry — FUNCTION 2xINTEGER + 4-byte result slot.
        // Each call: pre-push 4-byte slot + 4 bytes args, dispatch
        // pops the 4-byte args (leaving slot at SP+0), manually pop slot.
        let sp4 = cpu.read_reg(Register::A7);
        for _ in 0..5 {
            let cur = cpu.read_reg(Register::A7);
            // Pre-push 4-byte slot.
            bus.write_long(cur.wrapping_sub(4), 0xDEADBEEF);
            // Pre-push 4 bytes of args (2xINTEGER both zero).
            bus.write_long(cur.wrapping_sub(8), 0);
            cpu.write_reg(Register::A7, cur.wrapping_sub(8));
            disp.dispatch_menu(true, 0x264, &mut cpu, &mut bus)
                .expect("GetMCEntry handled")
                .expect("GetMCEntry Ok");
            // After trap: args popped (SP advanced by 4); slot now at SP+0.
            assert_eq!(
                cpu.read_reg(Register::A7),
                cur.wrapping_sub(4),
                "GetMCEntry must pop 4 bytes of args, leaving slot at SP+0"
            );
            // Manually pop the result slot.
            cpu.write_reg(Register::A7, cpu.read_reg(Register::A7).wrapping_add(4));
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp4,
            "5 GetMCEntry() FUNCTION call sequences must net-balance A7"
        );

        // AA65 SetMCEntries — PROCEDURE pop-6 (1xINTEGER + 1xPtr).
        // 5 calls × 6 = 30 bytes consumed total.
        let sp5 = cpu.read_reg(Register::A7);
        for i in 0..5u32 {
            // Each frame is 6 bytes; layout is [SP+0: ptr(4), SP+4: int(2)].
            bus.write_long(sp5.wrapping_sub((i + 1) * 6), 0);
            bus.write_word(sp5.wrapping_sub((i + 1) * 6).wrapping_add(4), 0);
        }
        cpu.write_reg(Register::A7, sp5.wrapping_sub(30));
        for _ in 0..5 {
            disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
                .expect("SetMCEntries handled")
                .expect("SetMCEntries Ok");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp5,
            "5 SetMCEntries calls should pop 5×6 = 30 bytes cumulatively"
        );
    }

    // IM:I I-354: ClearMenuBar empties the current menu list.
    #[test]
    fn clearmenubar_empties_current_menu_list() {
        let (mut disp, mut cpu, mut bus) = setup();
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 501, 0x30B640, "File");
        let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 502, 0x30B680, "Edit");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        insert_menu(&mut disp, &mut cpu, &mut bus, edit);
        assert_ne!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 501), 0);
        assert_ne!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 502), 0);

        let result = disp.dispatch_menu(true, 0x134, &mut cpu, &mut bus);
        assert!(result.is_some(), "ClearMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "ClearMenuBar should succeed");
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 501),
            0,
            "ClearMenuBar should remove previously inserted menu ID 501"
        );
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 502),
            0,
            "ClearMenuBar should remove previously inserted menu ID 502"
        );
    }

    // IM:V 1986 p. V-244: ClearMenuBar clears the current menu list and
    // the application's menu color information table.
    #[test]
    fn clearmenubar_clears_menu_color_information_table() {
        let (mut disp, mut cpu, mut bus) = setup();
        let file_id = 501i16;
        let edit_id = 502i16;
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, file_id, 0x30B6C0, "File");
        let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, edit_id, 0x30B700, "Edit");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        insert_menu(&mut disp, &mut cpu, &mut bus, edit);
        set_mc_entries_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            &[
                (0, 0, 0x3100),
                (file_id, 0, 0x3200),
                (file_id, 1, 0x3300),
                (edit_id, 1, 0x3400),
            ],
        );

        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, file_id, 1),
            0,
            "precondition: file item MenuCInfo entry should exist"
        );
        assert_ne!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 0, 0),
            0,
            "precondition: default MenuCInfo entry should exist"
        );

        let result = disp.dispatch_menu(true, 0x134, &mut cpu, &mut bus);
        assert!(result.is_some(), "ClearMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "ClearMenuBar should succeed");

        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, file_id),
            0,
            "ClearMenuBar should remove the File menu from the current list"
        );
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, edit_id),
            0,
            "ClearMenuBar should remove the Edit menu from the current list"
        );
        assert_eq!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, file_id, 0),
            0,
            "ClearMenuBar should clear menu title MenuCInfo entries"
        );
        assert_eq!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, file_id, 1),
            0,
            "ClearMenuBar should clear menu item MenuCInfo entries"
        );
        assert_eq!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, edit_id, 1),
            0,
            "ClearMenuBar should clear every menu's MenuCInfo entries"
        );
        assert_eq!(
            get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 0, 0),
            0,
            "ClearMenuBar should clear the default MenuCInfo entry too"
        );
    }

    // IM:I I-354: ClearMenuBar is a parameterless procedure.
    #[test]
    fn clearmenubar_has_no_parameters_and_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let result = disp.dispatch_menu(true, 0x134, &mut cpu, &mut bus);
        assert!(result.is_some(), "ClearMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "ClearMenuBar should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "ClearMenuBar should not pop stack arguments"
        );
    }

    // 0x149 — GetMHandle: pops 2 bytes, writes handle at new SP.
    #[test]
    fn test_get_mhandle() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 1); // menu_id = 1
        let result = disp.dispatch_menu(true, 0x149, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetMHandle should be handled");
        assert!(result.unwrap().is_ok(), "GetMHandle should succeed");
        let sp = cpu.read_reg(Register::A7);
        assert_eq!(sp, TEST_SP + 2, "GetMHandle should pop 2 bytes from stack");
        let handle = bus.read_long(sp);
        assert_eq!(handle, 0, "GetMHandle should return 0 handle when no menus");
    }

    // IM:I I-354: GetMenuBar returns a Handle to a copy of the current menu
    // list and takes no parameters.
    #[test]
    fn getmenubar_returns_non_nil_counted_menu_list_and_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 510, 0x30B740, "File");
        let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 511, 0x30B780, "Edit");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        insert_menu(&mut disp, &mut cpu, &mut bus, edit);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0xDEAD_BEEF);
        let result = disp.dispatch_menu(true, 0x13B, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "GetMenuBar should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "GetMenuBar should preserve A7 (no parameters)"
        );

        let list_handle = bus.read_long(TEST_SP);
        assert_ne!(list_handle, 0, "GetMenuBar should return a non-NIL handle");
        let list_ptr = bus.read_long(list_handle);
        assert_ne!(list_ptr, 0, "returned menu-list handle should dereference");
        assert_eq!(
            bus.read_word(list_ptr),
            2,
            "menu-list block should start with the current menu count"
        );
        assert_eq!(
            bus.read_long(list_ptr + 2),
            file,
            "first menu handle in snapshot should match current list order"
        );
        assert_eq!(
            bus.read_long(list_ptr + 6),
            edit,
            "second menu handle in snapshot should match current list order"
        );
    }

    // IM:I I-355: SetMenuBar takes one Handle argument and has no function
    // result, so it consumes 4 bytes from the stack.
    #[test]
    fn setmenubar_consumes_menulist_handle_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_long(TEST_SP, 0); // mbar_handle
        let result = disp.dispatch_menu(true, 0x13C, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "SetMenuBar should succeed");
        let sp = cpu.read_reg(Register::A7);
        assert_eq!(sp, TEST_SP + 4, "SetMenuBar should pop 4 bytes from stack");
    }

    // IM:I I-354..I-355: SetMenuBar copies the specified list to the
    // current menu list, enabling restoration of a prior GetMenuBar snapshot.
    #[test]
    fn setmenubar_restores_current_menu_list_from_getmenubar_snapshot() {
        let (mut disp, mut cpu, mut bus) = setup();

        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x30B700, "File");
        let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x30B710, "Edit");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        insert_menu(&mut disp, &mut cpu, &mut bus, edit);

        // Snapshot the current menu list with GetMenuBar.
        cpu.write_reg(Register::A7, TEST_SP);
        assert!(
            disp.dispatch_menu(true, 0x13B, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "GetMenuBar should succeed"
        );
        let saved_menu_list = bus.read_long(TEST_SP);
        assert_ne!(
            saved_menu_list, 0,
            "GetMenuBar should return non-NIL handle"
        );

        // Replace current menu list with a different one.
        assert!(
            disp.dispatch_menu(true, 0x134, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "ClearMenuBar should succeed"
        );
        let tools = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 130, 0x30B720, "Tools");
        insert_menu(&mut disp, &mut cpu, &mut bus, tools);
        assert_eq!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 128), 0);
        assert_eq!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 129), 0);
        assert_ne!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 130), 0);

        // Restore the saved list with SetMenuBar.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, saved_menu_list);
        assert!(
            disp.dispatch_menu(true, 0x13C, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "SetMenuBar should succeed"
        );

        assert_ne!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 128),
            0,
            "SetMenuBar should restore menu ID 128 from saved list"
        );
        assert_ne!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 129),
            0,
            "SetMenuBar should restore menu ID 129 from saved list"
        );
        assert_eq!(
            get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 130),
            0,
            "SetMenuBar should replace current list with the saved snapshot"
        );
    }

    // IM:I I-355 / MTE 3-119: MenuSelect returns 0 when no menu item is
    // chosen; the Point argument is consumed and the result is a LongInt.
    #[test]
    fn menuselect_no_menu_hit_returns_zero_and_pops_startpt() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.enable_input_trace_capture();
        disp.mouse_pos = (40, 120);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 40);
        bus.write_word(TEST_SP + 2, 120);
        bus.write_long(TEST_SP + 4, 0xDEAD_BEEF);

        let result = disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus);
        assert!(result.is_some(), "MenuSelect should be handled");
        assert!(result.unwrap().is_ok(), "MenuSelect should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 4,
            "MenuSelect should pop the Point argument on no-hit return"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 4),
            0,
            "MenuSelect should return 0 when no menu item is selected"
        );
        assert!(
            disp.menu_tracking.is_none(),
            "MenuSelect should not leave tracking state active on no-hit return"
        );
        let trace = disp.input_trace_text();
        assert!(trace.contains("A93D action=start start=(40,120)"));
        assert!(trace.contains("result=$00000000 outcome=no_menu_title"));
    }

    // IM:I I-355: MenuSelect enters tracking while a menu title is active
    // and does not immediately return a final menuResult.
    #[test]
    fn menuselect_title_hit_enters_tracking_without_immediate_stack_pop() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.enable_input_trace_capture();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 520, 0x30B800, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x30B840, "Open/O");
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);

        cpu.write_reg(Register::A7, TEST_SP);
        assert!(
            disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "DrawMenuBar should succeed before MenuSelect tracking"
        );
        let regions = disp.menu_title_regions();
        assert!(
            !regions.is_empty(),
            "menu title regions should be available"
        );
        let title_mid_h = (regions[0].0 + regions[0].1) / 2;
        disp.mouse_pos = (10, title_mid_h);
        disp.mouse_button = true;

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 10);
        bus.write_word(TEST_SP + 2, title_mid_h as u16);
        bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);

        let result = disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus);
        assert!(result.is_some(), "MenuSelect should be handled");
        assert!(result.unwrap().is_ok(), "MenuSelect should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "initial title-hit MenuSelect call should defer stack pop while tracking"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 4),
            0xA5A5_A5A5,
            "MenuSelect should defer writing result while tracking is active"
        );
        assert!(
            disp.menu_tracking.is_some(),
            "MenuSelect should enter tracking mode when menu title is hit"
        );
        let trace = disp.input_trace_text();
        assert!(trace.contains("A93D action=start"));
        assert!(trace.contains("outcome=open_tracking"));
        assert!(trace.contains("A93D action=tracking_entered"));
        assert!(trace.contains("tracking=menu:active"));
    }

    // IM:I I-355: MenuSelect tracks until mouse-up, then returns the
    // selected enabled menu item as menuID in the high word and item number
    // in the low word after the menu flash completes.
    #[test]
    fn menuselect_enabled_item_selection_traces_update_release_and_finish() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.enable_input_trace_capture();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 520, 0x30B900, "File");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x30B940,
            "Open/O;Close/W",
        );
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);

        cpu.write_reg(Register::A7, TEST_SP);
        assert!(
            disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "DrawMenuBar should succeed before MenuSelect tracking"
        );
        let regions = disp.menu_title_regions();
        let title_mid_h = (regions[0].0 + regions[0].1) / 2;

        disp.mouse_pos = (10, title_mid_h);
        disp.mouse_button = true;
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 10);
        bus.write_word(TEST_SP + 2, title_mid_h as u16);
        bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);
        disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(disp.menu_tracking.is_some());

        let (dropdown_top, dropdown_left, _, _) =
            disp.menu_tracking.as_ref().unwrap().dropdown_rect;
        disp.mouse_pos = (dropdown_top + 17, dropdown_left + 8);
        disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            disp.menu_tracking
                .as_ref()
                .map(|tracking| tracking.highlighted_item),
            Some(2)
        );

        disp.mouse_button = false;
        disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            disp.menu_tracking
                .as_ref()
                .map(|tracking| tracking.flash_result),
            Some(0x0208_0002)
        );

        for _ in 0..40 {
            if disp.menu_tracking.is_none() {
                break;
            }
            disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
        }

        assert!(
            disp.menu_tracking.is_none(),
            "MenuSelect should finish after flash phases"
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(TEST_SP + 4), 0x0208_0002);

        let trace = disp.input_trace_text();
        assert!(trace.contains("A93D action=tracking_update"));
        assert!(
            trace.contains("highlighted_item=2 result=pending outcome=enabled_item_highlighted")
        );
        assert!(trace.contains("A93D action=release"));
        assert!(trace.contains("highlighted_item=2 result=$02080002 outcome=start_flash"));
        assert!(trace.contains("A93D action=finish"));
        assert!(trace.contains("tracking=menu:idle dialog:idle control:idle"));
        assert!(trace.contains("highlighted_item=2 result=$02080002 outcome=enabled_item_selected"));
    }

    #[test]
    fn menuselect_release_over_item_selects_without_prior_tracking_refire() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 521, 0x30BA00, "File");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x30BA40,
            "Open/O;Save/S",
        );
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let title = disp.menu_title_regions()[0];
        let title_mid_h = (title.0 + title.1) / 2;

        disp.mouse_pos = (10, title_mid_h);
        disp.mouse_button = true;
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 10);
        bus.write_word(TEST_SP + 2, title_mid_h as u16);
        bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);
        disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let (top, left, _, _) = disp.menu_tracking.as_ref().unwrap().dropdown_rect;
        disp.mouse_pos = (top + 17, left + 8);
        disp.mouse_button = false;
        disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(
            disp.menu_tracking
                .as_ref()
                .map(|tracking| tracking.flash_result),
            Some(0x0209_0002),
            "mouse-up over Save must select it even without a separate held-button refire"
        );
    }

    struct MenuKeyThemeSnapshot {
        lower_result: u32,
        lower_stack_after: u32,
        title_regions: Vec<(i16, i16)>,
        matched_title_changed_pixels: usize,
        other_title_changed_pixels: usize,
        disabled_rightmost_result: u32,
        disabled_rightmost_stack_after: u32,
        all_disabled_result: u32,
        all_disabled_stack_after: u32,
        uninserted_result: u32,
        uninserted_stack_after: u32,
    }

    fn menu_key_results_for_theme(theme_id: UiThemeId) -> MenuKeyThemeSnapshot {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(theme_id);
        let row_bytes = 64;
        let base = bus.alloc(row_bytes * 342);
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        disp.menu_bar_hidden = false;
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 700, 0x30B700, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, file, 0x30B740, "Open/O");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);

        let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 701, 0x30B800, "Edit");
        append_menu_data(&mut disp, &mut cpu, &mut bus, edit, 0x30B840, "Other/O");
        insert_menu(&mut disp, &mut cpu, &mut bus, edit);

        let ghost = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 702, 0x30B900, "Ghost");
        append_menu_data(&mut disp, &mut cpu, &mut bus, ghost, 0x30B940, "Ghost/G");

        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let title_regions = disp.menu_title_regions();
        let file_before = title_region_pixels(
            &bus,
            base,
            row_bytes,
            title_regions[0].0,
            title_regions[0].1,
        );
        let edit_before = title_region_pixels(
            &bus,
            base,
            row_bytes,
            title_regions[1].0,
            title_regions[1].1,
        );

        let (lower_result, lower_stack_after) =
            menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'o');

        let file_after = title_region_pixels(
            &bus,
            base,
            row_bytes,
            title_regions[0].0,
            title_regions[0].1,
        );
        let edit_after = title_region_pixels(
            &bus,
            base,
            row_bytes,
            title_regions[1].0,
            title_regions[1].1,
        );
        let other_title_changed_pixels = changed_pixel_count(&file_before, &file_after);
        let matched_title_changed_pixels = changed_pixel_count(&edit_before, &edit_after);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, edit);
        disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let (disabled_rightmost_result, disabled_rightmost_stack_after) =
            menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'O');

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, file);
        disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let (all_disabled_result, all_disabled_stack_after) =
            menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'O');
        let (uninserted_result, uninserted_stack_after) =
            menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'G');

        MenuKeyThemeSnapshot {
            lower_result,
            lower_stack_after,
            title_regions,
            matched_title_changed_pixels,
            other_title_changed_pixels,
            disabled_rightmost_result,
            disabled_rightmost_stack_after,
            all_disabled_result,
            all_disabled_stack_after,
            uninserted_result,
            uninserted_stack_after,
        }
    }

    #[test]
    fn menukey_does_not_paint_titles_while_menu_bar_is_hidden() {
        for (menu_bar_hidden, fullscreen_locked) in [(true, false), (false, true)] {
            let (mut disp, mut cpu, mut bus) = setup_with_port();
            let row_bytes = 512;
            let height = 342;
            let base = bus.alloc(row_bytes * height);
            disp.set_screen_mode_for_test(base, row_bytes, 512, height as u16, 8);
            clear_1bpp_screen(&mut bus, base, row_bytes, height);
            bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

            let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 703, 0x30BA00, "File");
            append_menu_data(&mut disp, &mut cpu, &mut bus, file, 0x30BA40, "New/N");
            insert_menu(&mut disp, &mut cpu, &mut bus, file);
            assert!(disp.menus[0].visible_in_menu_bar);

            disp.menu_bar_hidden = menu_bar_hidden;
            disp.fullscreen_locked = fullscreen_locked;
            let before = bus.read_bytes(base, row_bytes as usize * 20);

            let (result, stack_after) =
                menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'N');
            assert_eq!(result, 0x02BF_0001);
            assert_eq!(stack_after, TEST_SP + 2);
            assert_eq!(
                bus.read_bytes(base, row_bytes as usize * 20),
                before,
                "MenuKey must not expose a title highlight while menu chrome is hidden"
            );

            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(TEST_SP, 0);
            disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            disp.draw_menu_bar_to_fb(&mut bus);
            assert_eq!(
                bus.read_bytes(base, row_bytes as usize * 20),
                before,
                "clearing and compositing hidden menu chrome must leave the framebuffer unchanged"
            );
        }
    }

    #[test]
    fn systemless_theme_does_not_change_menukey_result_or_highlight_geometry() {
        // IM:I I-356 says MenuKey maps a Command-key character to the
        // same LongInt result as MenuSelect, scans duplicate shortcuts
        // right-to-left/top-to-bottom, ignores disabled/non-current items,
        // folds case via UpperText, and highlights the matching menu title.
        // Theme chrome must not change the result, stack, or title geometry.
        let classic = menu_key_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = menu_key_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.lower_result, 0x02BD_0001);
        assert_eq!(classic.lower_stack_after, TEST_SP + 2);
        assert_eq!(classic.disabled_rightmost_result, 0x02BC_0001);
        assert_eq!(classic.disabled_rightmost_stack_after, TEST_SP + 2);
        assert_eq!(classic.all_disabled_result, 0);
        assert_eq!(classic.all_disabled_stack_after, TEST_SP + 2);
        assert_eq!(classic.uninserted_result, 0);
        assert_eq!(classic.uninserted_stack_after, TEST_SP + 2);
        assert!(
            classic.matched_title_changed_pixels > classic.other_title_changed_pixels,
            "classic MenuKey should primarily change the matched rightmost title"
        );
        assert!(
            classic.matched_title_changed_pixels > 0,
            "classic MenuKey should highlight the matched rightmost title"
        );

        assert_eq!(themed.lower_result, classic.lower_result);
        assert_eq!(themed.lower_stack_after, classic.lower_stack_after);
        assert_eq!(
            themed.disabled_rightmost_result,
            classic.disabled_rightmost_result
        );
        assert_eq!(
            themed.disabled_rightmost_stack_after,
            classic.disabled_rightmost_stack_after
        );
        assert_eq!(themed.all_disabled_result, classic.all_disabled_result);
        assert_eq!(
            themed.all_disabled_stack_after,
            classic.all_disabled_stack_after
        );
        assert_eq!(themed.uninserted_result, classic.uninserted_result);
        assert_eq!(
            themed.uninserted_stack_after,
            classic.uninserted_stack_after
        );
        assert_eq!(
            themed.title_regions, classic.title_regions,
            "systemless-default must not change MenuKey title geometry"
        );
        assert_eq!(
            themed.other_title_changed_pixels, 0,
            "systemless-default should keep non-matched title chrome unchanged"
        );
        assert!(
            themed.matched_title_changed_pixels > 0,
            "systemless-default MenuKey should route the matched title highlight through the provider"
        );
    }

    struct PopUpMenuSelectThemeSnapshot {
        rect: (i16, i16, i16, i16),
        highlighted_item: i16,
        item_at_requested_point: i16,
        first_stack_after: u32,
        result: u32,
        final_stack_after: u32,
        tracking_finished: bool,
        clamped_rect: (i16, i16, i16, i16),
        clamped_highlighted_item: i16,
        uninserted_result: u32,
        uninserted_stack_after: u32,
        uninserted_tracking_finished: bool,
    }

    fn dispatch_popupmenuselect_start(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        menu_handle: u32,
        top: i16,
        left: i16,
        popup_item: i16,
    ) {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, popup_item as u16);
        bus.write_word(TEST_SP + 2, left as u16);
        bus.write_word(TEST_SP + 4, top as u16);
        bus.write_long(TEST_SP + 6, menu_handle);
        bus.write_long(TEST_SP + 10, 0xDEAD_BEEF);
        assert!(
            disp.dispatch_menu(true, 0x00B, cpu, bus).unwrap().is_ok(),
            "PopUpMenuSelect should succeed"
        );
    }

    fn finish_popupmenuselect(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
    ) -> (u32, u32, bool) {
        disp.mouse_button = false;
        bus.write_byte(crate::memory::globals::addr::MB_STATE, 0x80);
        for _ in 0..80 {
            if disp.menu_tracking.is_none() {
                break;
            }
            disp.dispatch_menu(true, 0x00B, cpu, bus).unwrap().unwrap();
        }
        (
            bus.read_long(TEST_SP + 10),
            cpu.read_reg(Register::A7),
            disp.menu_tracking.is_none(),
        )
    }

    fn popupmenuselect_theme_snapshot(theme_id: UiThemeId) -> PopUpMenuSelectThemeSnapshot {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(theme_id);
        let row_bytes = 30;
        let base = bus.alloc(row_bytes * 160);
        disp.set_screen_mode_for_test(base, row_bytes, 240, 160, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 160);
        disp.menu_bar_hidden = false;
        disp.mouse_button = true;

        let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 730, 0x30BB00, "Pop");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            menu,
            0x30BB40,
            "One;Two;Three;Four",
        );
        insert_menu_before_id(&mut disp, &mut cpu, &mut bus, menu, -1);

        dispatch_popupmenuselect_start(&mut disp, &mut cpu, &mut bus, menu, 58, 30, 3);
        let tracking = disp.menu_tracking.as_ref().expect("popup tracking active");
        let rect = tracking.dropdown_rect;
        let highlighted_item = tracking.highlighted_item;
        let first_stack_after = cpu.read_reg(Register::A7);
        let item_at_requested_point = disp.dropdown_item_at_point(&bus, 35, 58);
        // PopUpMenuSelect re-evaluates the live mouse position on release.
        // Keep the synthetic release over the requested third item rather
        // than inheriting setup_with_port's default position at (0, 0).
        disp.mouse_pos = (58, 35);
        let (result, final_stack_after, tracking_finished) =
            finish_popupmenuselect(&mut disp, &mut cpu, &mut bus);

        let (mut clamp_disp, mut clamp_cpu, mut clamp_bus) = setup_with_port();
        clamp_disp.set_ui_theme_id(theme_id);
        let clamp_base = clamp_bus.alloc(row_bytes * 160);
        clamp_disp.set_screen_mode_for_test(clamp_base, row_bytes, 240, 160, 1);
        clear_1bpp_screen(&mut clamp_bus, clamp_base, row_bytes, 160);
        clamp_disp.menu_bar_hidden = false;
        clamp_disp.mouse_button = true;
        let clamp_menu = new_menu_with_title(
            &mut clamp_disp,
            &mut clamp_cpu,
            &mut clamp_bus,
            731,
            0x30BC00,
            "Pop",
        );
        append_menu_data(
            &mut clamp_disp,
            &mut clamp_cpu,
            &mut clamp_bus,
            clamp_menu,
            0x30BC40,
            "One;Two;Three;Four",
        );
        insert_menu_before_id(
            &mut clamp_disp,
            &mut clamp_cpu,
            &mut clamp_bus,
            clamp_menu,
            -1,
        );
        dispatch_popupmenuselect_start(
            &mut clamp_disp,
            &mut clamp_cpu,
            &mut clamp_bus,
            clamp_menu,
            150,
            220,
            4,
        );
        let clamp_tracking = clamp_disp
            .menu_tracking
            .as_ref()
            .expect("clamped popup tracking active");
        let clamped_rect = clamp_tracking.dropdown_rect;
        let clamped_highlighted_item = clamp_tracking.highlighted_item;

        let (mut miss_disp, mut miss_cpu, mut miss_bus) = setup_with_port();
        miss_disp.set_ui_theme_id(theme_id);
        miss_disp.menu_bar_hidden = false;
        miss_disp.mouse_button = false;
        let miss_menu = new_menu_with_title(
            &mut miss_disp,
            &mut miss_cpu,
            &mut miss_bus,
            732,
            0x30BD00,
            "Pop",
        );
        append_menu_data(
            &mut miss_disp,
            &mut miss_cpu,
            &mut miss_bus,
            miss_menu,
            0x30BD40,
            "One;Two",
        );
        dispatch_popupmenuselect_start(
            &mut miss_disp,
            &mut miss_cpu,
            &mut miss_bus,
            miss_menu,
            20,
            20,
            1,
        );

        PopUpMenuSelectThemeSnapshot {
            rect,
            highlighted_item,
            item_at_requested_point,
            first_stack_after,
            result,
            final_stack_after,
            tracking_finished,
            clamped_rect,
            clamped_highlighted_item,
            uninserted_result: miss_bus.read_long(TEST_SP + 10),
            uninserted_stack_after: miss_cpu.read_reg(Register::A7),
            uninserted_tracking_finished: miss_disp.menu_tracking.is_none(),
        }
    }

    #[test]
    fn systemless_theme_does_not_change_popupmenuselect_geometry_and_result() {
        // MTE 1992 p. 3-120 says PopUpMenuSelect displays the requested
        // PopUpItem at Top/Left, tracks until mouse-up, and returns the
        // chosen menu ID/item LongInt. IM:V V-241 adds that the menu must
        // be inserted in the MenuList for the duration of the call.
        let classic = popupmenuselect_theme_snapshot(UiThemeId::ClassicSystem7);
        let themed = popupmenuselect_theme_snapshot(UiThemeId::SystemlessDefault);

        // Width comes from the widest item "Three" measured in the bundled
        // Nimbus Sans Bold 12 face. Its 33px advance plus the standard 26px
        // menu padding makes the box 59px wide. The clamped case pins that box
        // against the 240px screen edge.
        assert_eq!(classic.rect, (25, 29, 91, 88));
        assert_eq!(classic.highlighted_item, 3);
        assert_eq!(classic.item_at_requested_point, 3);
        assert_eq!(classic.first_stack_after, TEST_SP);
        assert_eq!(classic.result, 0x02DA_0003);
        assert_eq!(classic.final_stack_after, TEST_SP + 10);
        assert!(classic.tracking_finished);
        assert_eq!(classic.clamped_rect, (94, 181, 160, 240));
        assert_eq!(classic.clamped_highlighted_item, 4);
        assert_eq!(classic.uninserted_result, 0);
        assert_eq!(classic.uninserted_stack_after, TEST_SP + 10);
        assert!(classic.uninserted_tracking_finished);

        assert_eq!(
            themed.rect, classic.rect,
            "systemless-default must preserve PopUpMenuSelect popup geometry"
        );
        assert_eq!(themed.highlighted_item, classic.highlighted_item);
        assert_eq!(
            themed.item_at_requested_point,
            classic.item_at_requested_point
        );
        assert_eq!(themed.first_stack_after, classic.first_stack_after);
        assert_eq!(themed.result, classic.result);
        assert_eq!(themed.final_stack_after, classic.final_stack_after);
        assert_eq!(themed.tracking_finished, classic.tracking_finished);
        assert_eq!(themed.clamped_rect, classic.clamped_rect);
        assert_eq!(
            themed.clamped_highlighted_item,
            classic.clamped_highlighted_item
        );
        assert_eq!(themed.uninserted_result, classic.uninserted_result);
        assert_eq!(
            themed.uninserted_stack_after,
            classic.uninserted_stack_after
        );
        assert_eq!(
            themed.uninserted_tracking_finished,
            classic.uninserted_tracking_finished
        );
    }

    fn menuselect_enabled_item_result_for_theme(
        theme_id: UiThemeId,
    ) -> (u32, u32, (i16, i16, i16, i16)) {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(theme_id);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 520, 0x30BA00, "File");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            handle,
            0x30BA40,
            "Open/O;Close/W",
        );
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);

        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        disp.mouse_pos = (10, 15);
        disp.mouse_button = true;
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 10);
        bus.write_word(TEST_SP + 2, 15);
        bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);
        disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let dropdown_rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect;
        let (dropdown_top, dropdown_left, _, _) = dropdown_rect;
        disp.mouse_pos = (dropdown_top + 17, dropdown_left + 8);
        disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        disp.mouse_button = false;
        disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        for _ in 0..40 {
            if disp.menu_tracking.is_none() {
                break;
            }
            disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
        }

        (
            bus.read_long(TEST_SP + 4),
            cpu.read_reg(Register::A7),
            dropdown_rect,
        )
    }

    #[test]
    fn systemless_theme_does_not_change_menuselect_result_encoding() {
        // IM:I I-356: MenuSelect tracks until mouse-up and returns a LongInt
        // with menu ID in the high word and item number in the low word.
        // Theme chrome must not change that guest-visible encoding or metrics.
        let classic = menuselect_enabled_item_result_for_theme(UiThemeId::ClassicSystem7);
        let themed = menuselect_enabled_item_result_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.0, 0x0208_0002);
        assert_eq!(classic.1, TEST_SP + 4);
        assert_eq!(
            themed, classic,
            "systemless-default must not change MenuSelect result encoding or dropdown geometry"
        );
    }

    // IM:V V-235..V-237: a hierarchical item has itemCmd=$1B and
    // itemMark equal to the submenu menuID; MenuSelect returns the
    // selected submenu's menuID/item packed as a LongInt.
    #[test]
    fn menuselect_tracks_hierarchical_submenu_item() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x310000, "File");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            file,
            0x310040,
            "New Game;Resume Game;-;Practice Battle;Pause Game;Resign Game;-;Quit",
        );
        let practice =
            new_menu_with_title(&mut disp, &mut cpu, &mut bus, 138, 0x310200, "Practice");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            practice,
            0x310240,
            "1: Cake Walk;(2: One-Gun;(3: Sucker Punch;(4: Airborne",
        );
        set_item_cmd(&mut disp, &mut cpu, &mut bus, file, 4, 0x1B);
        set_item_mark(&mut disp, &mut cpu, &mut bus, file, 4, 138);

        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        insert_menu_before(&mut disp, &mut cpu, &mut bus, practice, -1);
        cpu.write_reg(Register::A7, TEST_SP);
        assert!(
            disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "DrawMenuBar should succeed before MenuSelect tracking"
        );

        let regions = disp.menu_title_regions();
        let file_mid_h = (regions[0].0 + regions[0].1) / 2;
        disp.mouse_pos = (10, file_mid_h);
        disp.mouse_button = true;
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 10);
        bus.write_word(TEST_SP + 2, file_mid_h as u16);
        bus.write_long(TEST_SP + 4, 0xFFFF_FFFF);

        assert!(
            disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "MenuSelect should enter tracking on the File title"
        );
        let parent_rect = disp
            .menu_tracking
            .as_ref()
            .expect("MenuSelect should be tracking")
            .dropdown_rect;
        let parent_item_y = parent_rect.0 + 1 + 3 * 16 + 8;

        disp.mouse_pos = (parent_item_y, parent_rect.1 + 24);
        assert!(
            disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "MenuSelect should track the hierarchical parent item"
        );

        disp.mouse_pos = (parent_item_y, parent_rect.3 + 20);
        assert!(
            disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "MenuSelect should track into the submenu"
        );

        disp.mouse_button = false;
        for _ in 0..40 {
            assert!(
                disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
                    .unwrap()
                    .is_ok(),
                "MenuSelect should finish submenu selection"
            );
            if disp.menu_tracking.is_none() {
                break;
            }
        }

        assert!(
            disp.menu_tracking.is_none(),
            "MenuSelect should finish after flashing the submenu item"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 4),
            (138u32 << 16) | 1,
            "MenuSelect should return the selected submenu menuID/item"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 4,
            "MenuSelect should pop the Point argument after submenu selection"
        );
    }

    // IM:V V-239: InsertMenu(beforeID=-1) places a menu in the hierarchical
    // portion of the current menu list, so later nonhierarchical menu-bar
    // titles must still hit-test by their source menu record, not by compacted
    // title-region index.
    #[test]
    fn menuselect_regular_menu_after_hierarchical_menu_still_opens() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 540, 0x311000, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, file, 0x311040, "Open");
        let submenu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 541, 0x311100, "Practice");
        append_menu_data(
            &mut disp,
            &mut cpu,
            &mut bus,
            submenu,
            0x311140,
            "1: Cake Walk",
        );
        let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 542, 0x311200, "Edit");
        append_menu_data(&mut disp, &mut cpu, &mut bus, edit, 0x311240, "Copy");

        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        insert_menu_before(&mut disp, &mut cpu, &mut bus, submenu, -1);
        insert_menu(&mut disp, &mut cpu, &mut bus, edit);

        cpu.write_reg(Register::A7, TEST_SP);
        assert!(
            disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "DrawMenuBar should succeed before MenuSelect tracking"
        );

        let regions = disp.menu_title_regions();
        assert_eq!(
            regions.len(),
            2,
            "hierarchical menu must not create a menu-bar title"
        );
        let edit_mid_h = (regions[1].0 + regions[1].1) / 2;
        disp.mouse_pos = (10, edit_mid_h);
        disp.mouse_button = true;
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 10);
        bus.write_word(TEST_SP + 2, edit_mid_h as u16);
        bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);

        assert!(
            disp.dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "MenuSelect should enter tracking on the Edit title"
        );

        let edit_idx = disp
            .menus
            .iter()
            .position(|menu| menu.handle == edit)
            .expect("Edit menu should still be tracked");
        assert_eq!(
            disp.menu_tracking
                .as_ref()
                .expect("MenuSelect should be tracking")
                .active_menu,
            edit_idx,
            "MenuSelect should open the regular menu after a hierarchical menu"
        );
    }

    // save_dropdown_pixels / restore_dropdown_pixels must not overflow
    // when the rect has y < 0 or y >= screen_h. Mirrors the off-screen
    // guards in save_dialog_pixels and save_rect_pixels.
    #[test]
    fn save_dropdown_pixels_handles_negative_top_without_overflow() {
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        for i in 0..800u32 * 600 {
            bus.write_byte(screen_base + i, 0x77);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        // Rect straddling the screen top: top=-2 through bottom=4.
        let saved = disp.save_dropdown_pixels(&bus, (-2, 100, 4, 120));
        // save_right = 121; save_bottom = 5. Row width on-screen =
        // (121 - 100) = 21 bytes. Rows y=-2, -1 are skipped; rows
        // y=0..4 contribute 21 bytes each → 5 * 21 = 105 bytes.
        assert_eq!(saved.len(), 5 * 21);
        for &b in &saved {
            assert_eq!(b, 0x77, "on-screen bytes must come from framebuffer");
        }
    }

    #[test]
    fn save_dropdown_pixels_handles_top_beyond_screen_height_without_overflow() {
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        // Rect entirely below screen bottom.
        let saved = disp.save_dropdown_pixels(&bus, (600, 0, 610, 50));
        assert_eq!(
            saved.len(),
            0,
            "dropdown entirely off-screen must produce an empty buffer"
        );
    }

    #[test]
    fn ploticon_pascal_procedure_preserves_stack_across_five_calls() {
        // IM:I 1985 p. I-473 (Toolbox Utilities — Routines That
        // Operate on Icons — PlotIcon): Pascal PROCEDURE PlotIcon
        // (theRect: Rect; theIcon: Handle). Each call pops 8 bytes
        // (theIcon at SP+0 + theRect ptr at SP+4) and writes no
        // FUNCTION result slot. This test dispatches 5 successive
        // PlotIcon calls each with distinct icon Handles and
        // destination Rects and asserts they net-balance A7.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_pre = cpu.read_reg(Register::A7);

        // Five distinct 128-byte icon allocations + handles to defeat
        // any stub that caches the last-seen Handle argument.
        let icons: [u32; 5] = std::array::from_fn(|_| {
            let icon_ptr = bus.alloc(128);
            let icon_handle = bus.alloc(4);
            bus.write_long(icon_handle, icon_ptr);
            icon_handle
        });

        // Five distinct 32x32 destination Rects at non-overlapping
        // positions; Rect record is 8 bytes (4 INTEGERs).
        let rects: [u32; 5] = std::array::from_fn(|i| {
            let rect_ptr = bus.alloc(8);
            bus.write_word(rect_ptr, 2);
            bus.write_word(rect_ptr + 2, (i as u16) * 40 + 2);
            bus.write_word(rect_ptr + 4, 34);
            bus.write_word(rect_ptr + 6, (i as u16) * 40 + 34);
            rect_ptr
        });

        for i in 0..5 {
            let sp = cpu.read_reg(Register::A7);
            cpu.write_reg(Register::A7, sp - 8);
            bus.write_long(sp - 8, icons[i]);
            bus.write_long(sp - 4, rects[i]);
            let result = disp.dispatch_menu(true, 0x14B, &mut cpu, &mut bus);
            assert!(
                result.unwrap().is_ok(),
                "PlotIcon dispatch should succeed (call {})",
                i
            );
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp,
                "PlotIcon should pop 8 bytes per call (call {})",
                i
            );
        }

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_pre,
            "5 successive PlotIcon calls should net-balance A7"
        );
    }

    #[test]
    fn ploticon_current_port_zero_short_circuits_before_touching_memory() {
        // The current-port-zero path is a defensive no-op. It should
        // pop the 8-byte Pascal frame without touching the icon or
        // rect pointers on the stack.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = cpu.read_reg(Register::A7);
        let warnings = Arc::new(AtomicUsize::new(0));
        let subscriber = WarnCounter {
            warnings: warnings.clone(),
        };

        cpu.write_reg(Register::A7, sp - 8);
        bus.write_long(sp - 8, 0x0050_0000);
        bus.write_long(sp - 4, 0x0060_0000);

        let _guard = tracing::subscriber::set_default(subscriber);
        let result = disp.dispatch_menu(true, 0x14B, &mut cpu, &mut bus);
        assert!(result.is_some(), "PlotIcon should be handled");
        assert!(
            result.unwrap().is_ok(),
            "PlotIcon should return cleanly when no current port is set"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp,
            "PlotIcon should still pop the 8-byte argument frame"
        );
        assert_eq!(
            warnings.load(Ordering::Relaxed),
            0,
            "PlotIcon should not touch icon/rect memory when current_port is NIL"
        );
    }

    #[test]
    fn guest_menu_snapshot_exposes_only_the_inserted_menu_list() {
        let (mut disp, _cpu, bus) = setup();
        disp.menus = vec![
            Menu {
                id: 100,
                title: "File".into(),
                items: vec![MenuItem {
                    // The renderer-facing Menu model preserves Mac Roman
                    // bytes as chars; the native snapshot must expose Unicode.
                    text: "New Level\u{C9}".into(),
                    icon: 0,
                    key_equiv: b'N',
                    mark: 0x12,
                    style: 0,
                    enabled: true,
                }],
                enabled: true,
                handle: 0,
                in_menu_bar: true,
                hierarchical: false,
                visible_in_menu_bar: true,
            },
            Menu {
                id: 101,
                title: "Detached".into(),
                items: Vec::new(),
                enabled: true,
                handle: 0,
                in_menu_bar: false,
                hierarchical: false,
                visible_in_menu_bar: false,
            },
        ];

        let snapshot = disp.guest_menu_snapshot(&bus);
        assert_eq!(snapshot.menus.len(), 1);
        assert_eq!(snapshot.menus[0].id, 100);
        assert_eq!(snapshot.menus[0].items[0].number, 1);
        assert_eq!(snapshot.menus[0].items[0].text, "New Level…");
        assert_eq!(snapshot.menus[0].items[0].key_equivalent, Some('n'));
        assert!(snapshot.menus[0].items[0].checked);
    }

    #[test]
    fn native_selection_returns_through_menuselect_pascal_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.menus = vec![Menu {
            id: -120,
            title: "Game".into(),
            items: vec![MenuItem {
                text: "Pause".into(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            }],
            enabled: true,
            handle: 0,
            in_menu_bar: true,
            hierarchical: false,
            visible_in_menu_bar: true,
        }];

        assert!(disp.queue_native_menu_selection(&bus, -120, 1).is_some());
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP + 4, 0xDEAD_BEEF);
        let result = disp
            .dispatch_menu(true, 0x13D, &mut cpu, &mut bus)
            .expect("MenuSelect handled");
        assert!(result.is_ok());
        assert_eq!(bus.read_long(TEST_SP + 4), 0xFF88_0001);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert!(disp.pending_native_menu_selection.is_none());
        assert!(disp.pending_native_menu_event.is_none());
        assert!(disp.pending_native_menu_event_tick.is_none());
    }

    #[test]
    fn native_selection_rejects_disabled_and_hierarchical_parent_items() {
        let (mut disp, _cpu, bus) = setup();
        disp.menus = vec![Menu {
            id: 100,
            title: "File".into(),
            items: vec![
                MenuItem {
                    text: "Disabled".into(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: false,
                },
                MenuItem {
                    text: "Recent".into(),
                    icon: 0,
                    key_equiv: 0x1B,
                    mark: 7,
                    style: 0,
                    enabled: true,
                },
            ],
            enabled: true,
            handle: 0,
            in_menu_bar: true,
            hierarchical: false,
            visible_in_menu_bar: true,
        }];

        assert_eq!(disp.queue_native_menu_selection(&bus, 100, 1), None);
        assert_eq!(disp.queue_native_menu_selection(&bus, 100, 2), None);
        assert_eq!(disp.queue_native_menu_selection(&bus, 999, 1), None);
    }
}
