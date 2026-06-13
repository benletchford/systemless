//! Menu Manager trap handlers.

use crate::cpu::{CpuOps, Register};
use crate::memory::{globals::addr, MacMemoryBus, MemoryBus};
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
}

/// State for MenuSelect mouse tracking across frames.
pub struct MenuTrackingState {
    pub active_menu: usize,
    pub highlighted_item: i16,
    pub saved_pixels: Vec<u8>,
    pub dropdown_rect: (i16, i16, i16, i16),
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

/// Parse a MENU resource from guest memory into a Menu struct.
fn parse_menu_resource(bus: &MacMemoryBus, res_ptr: u32, handle: u32) -> Menu {
    let menu_id = bus.read_word(res_ptr) as i16;
    let enable_flags = bus.read_long(res_ptr + 10);

    let title_len = bus.read_byte(res_ptr + 14) as usize;
    let mut title_bytes = Vec::with_capacity(title_len);
    for i in 0..title_len {
        title_bytes.push(bus.read_byte(res_ptr + 15 + i as u32));
    }
    let title = String::from_utf8_lossy(&title_bytes).into_owned();

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
        let text = String::from_utf8_lossy(&text_bytes).into_owned();
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
        item.text = String::from_utf8_lossy(&text).into_owned();
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
/// Caller is responsible for ensuring the menu record block is large
/// enough for the serialised items. NewMenu currently allocates 256
/// bytes; writes past that bound are skipped for safety.
fn serialise_menu_items_to_memory(bus: &mut MacMemoryBus, menu: &Menu) {
    const MENU_RECORD_SIZE: u32 = 256;
    if menu.handle == 0 {
        return;
    }
    let menu_ptr = bus.read_long(menu.handle);
    if menu_ptr == 0 {
        return;
    }
    let title_len = bus.read_byte(menu_ptr + 14) as u32;
    let mut offset = 15 + title_len;
    for item in &menu.items {
        let bytes = item.text.as_bytes();
        let text_len = bytes.len().min(255) as u32;
        let item_size = 1 + text_len + 4;
        if offset + item_size + 1 > MENU_RECORD_SIZE {
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
    if offset < MENU_RECORD_SIZE {
        bus.write_byte(menu_ptr + offset, 0);
    }
}

/// Live menu-color table entry size in guest memory.
///
/// The compiled `'mctb'` resource entries are 28 bytes, but the in-memory
/// `MCEntry` record adds the trailing reserved word, so live table entries are
/// 30 bytes each.
const MC_ENTRY_SIZE: usize = 30;
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
    pub(crate) fn is_popup_menu_proc_id(proc_id: i16) -> bool {
        (1008..=1023).contains(&proc_id)
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
                let (_, menu_ptr) = self.find_resource_any(*b"MENU", menu_id)?;
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
                return Some(String::from_utf8_lossy(&bytes).into_owned());
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

    fn clone_menu_color_handle(&mut self, bus: &mut MacMemoryBus, handle: u32) -> u32 {
        if handle == 0 {
            return 0;
        }

        let bytes = Self::menu_color_table_bytes(bus, handle);
        self.alloc_handle_with_bytes(bus, &bytes)
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
            // table stored at lowmem $0D50. Systemless still does not
            // auto-load 'mctb' resources here, but it now creates the
            // empty MenuCInfo handle on first use so the AA60..AA65
            // routines can mutate/query a real guest table. Empty
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
                bus.write_long(menu_ptr + 6, 0);
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
                    title = String::from_utf8_lossy(&title_bytes).into_owned();
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
            // GetMenu ($A9BF): Reads MENU resource, copies to allocated block;
            // returns NIL when the MENU resource cannot be read per IM:I I-352.
            (true, 0x1BF) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_id = bus.read_word(sp) as i16;
                let handle = if let Some((_, res_ptr)) = self.find_resource_any(*b"MENU", menu_id) {
                    let menu_ptr = bus.alloc(256);
                    let handle = bus.alloc(4);
                    bus.write_long(handle, menu_ptr);

                    // Copy full MENU resource data to the allocated block.
                    // MENU format: menuID(2), menuWidth(2), menuHeight(2),
                    // menuProc(4), enableFlags(4), title(pstring),
                    // then items: [text(pstring), icon(1), key(1), mark(1), style(1)]...
                    // terminated by a 0-length item string.
                    let res_size = menu_resource_size(bus, res_ptr);
                    for i in 0..res_size.min(256) {
                        bus.write_byte(menu_ptr + i as u32, bus.read_byte(res_ptr + i as u32));
                    }

                    // Keep a non-menu-bar copy so AppendMenu can mutate this
                    // handle before InsertMenu installs it into the menu list.
                    let parsed = parse_menu_resource(bus, menu_ptr, handle);
                    if !self.menus.iter().any(|m| m.handle == handle) {
                        self.menus.push(parsed);
                    }
                    bus.write_word(0x0A60, 0);
                    cpu.write_reg(Register::D0, 0);
                    handle
                } else {
                    bus.write_word(0x0A60, (-192i16) as u16);
                    cpu.write_reg(Register::D0, -192i32 as u32);
                    0
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
                let menu_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);

                if menu_handle == 0 || text_ptr == 0 {
                    return Some(Ok(()));
                }
                let len = bus.read_byte(text_ptr) as usize;
                let mut bytes = Vec::with_capacity(len);
                for i in 0..len {
                    bytes.push(bus.read_byte(text_ptr + 1 + i as u32));
                }
                let parsed = parse_appendmenu_items(&bytes);

                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    for item in parsed {
                        menu.items.push(item);
                    }
                    sync_enable_flags(bus, menu);
                    // Also serialise items into the guest-memory MENU record
                    // so CountMItems / CalcMenuSize (which parse guest memory
                    // to stay compatible with GetMenu-loaded menus) see the
                    // AppendMenu'd items. Per IM:I I-355 menuData layout.
                    let menu_copy = menu.clone();
                    serialise_menu_items_to_memory(bus, &menu_copy);
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
                let _before_id = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);

                if menu_handle != 0 {
                    let menu_ptr = bus.read_long(menu_handle);
                    if menu_ptr != 0 {
                        // Per IM:I I-352, InsertMenu inserts a menu into the
                        // current menu list; it does not create/duplicate one.
                        if let Some(idx) = self.menus.iter().position(|m| m.handle == menu_handle) {
                            self.last_inserted_menu_id = Some(self.menus[idx].id);
                            self.menus[idx].in_menu_bar = true;
                        } else {
                            // Handle wasn't previously tracked (for example a
                            // raw guest MENU handle). Parse any MENU resource
                            // by menu ID, else fall back to title-only memory.
                            let menu_id = bus.read_word(menu_ptr) as i16;
                            if let Some((_, res_ptr)) = self.find_resource_any(*b"MENU", menu_id) {
                                let mut menu = parse_menu_resource(bus, res_ptr, menu_handle);
                                eprintln!(
                                    "[MENU] InsertMenu: ID={} title=\"{}\" items={}",
                                    menu.id,
                                    menu.title,
                                    menu.items.len()
                                );
                                menu.in_menu_bar = true;
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
                                    let title = String::from_utf8_lossy(&title_bytes).into_owned();
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
                                        });
                                    }
                                }
                            }
                        }
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
                self.menus.clear();
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
            (true, 0x1C0) => {
                let sp = cpu.read_reg(Register::A7);
                let mbar_id = bus.read_word(sp) as i16;
                let handle = if let Some((_, mbar_ptr)) = self.find_resource_any(*b"MBAR", mbar_id)
                {
                    let menu_count = bus.read_word(mbar_ptr) as usize;
                    let mut snapshot = Vec::new();

                    for i in 0..menu_count {
                        let menu_id = bus.read_word(mbar_ptr + 2 + (i as u32) * 2) as i16;
                        let Some((_, menu_res_ptr)) = self.find_resource_any(*b"MENU", menu_id)
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
                        snapshot.push(parsed);
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
                let entries = self.named_resources_of_type(res_type);

                let mut touched: Option<Menu> = None;
                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
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
                    serialise_menu_items_to_memory(bus, &m);
                }
                Ok(())
            }

            // DisableItem ($A93A)
            // Disables a menu item so it cannot be chosen.
            // PROCEDURE DisableItem(theMenu: MenuHandle; item: INTEGER);
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-131
            //
            // Regression coverage:
            //   disableitem_disables_menu_item
            //   disableitem_clears_enable_flag_in_guest_memory
            // DisableItem ($A93A): item=0 disables whole menu; item>31
            // is a no-op for individual items per IM:TB 1992 p.3-131.
            (true, 0x13A) => {
                let sp = cpu.read_reg(Register::A7);
                let item = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);

                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
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
            // Regression coverage:
            //   enableitem_enables_menu_item
            //   enableitem_sets_enable_flag_in_guest_memory
            // EnableItem ($A939): item=0 reenables menu title while preserving
            // individually disabled items; item>31 is no-op per IM:TB 1992 p.3-131.
            (true, 0x139) => {
                let sp = cpu.read_reg(Register::A7);
                let item = bus.read_word(sp) as i16;
                let menu_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);

                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
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
                if let Some(ref tracking) = self.menu_tracking {
                    // Re-fire: we're in tracking mode
                    if tracking.flash_remaining > 0 {
                        // Flashing phase: hold each toggle for 3 frames (~50ms),
                        // matching the real Mac's ~3-tick delay per phase.
                        // redraw_chrome handles the visual state based on
                        // whether flash_remaining is even or odd.
                        let result = tracking.flash_result;
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
                            self.restore_dropdown_pixels(
                                bus,
                                saved.dropdown_rect,
                                &saved.saved_pixels,
                            );
                            self.draw_menu_bar_to_fb(bus);
                            bus.write_long(sp + 4, result);
                            cpu.write_reg(Register::A7, sp + 4);
                        }
                        // else: stay on trap, re-fire next frame
                    } else if !self.mouse_button {
                        // Button released — start flash or complete immediately
                        if tracking.highlighted_item > 0 {
                            let menu = &self.menus[tracking.active_menu];
                            let item_idx = tracking.highlighted_item;
                            let result = ((menu.id as u32) << 16) | (item_idx as u32 & 0xFFFF);
                            // Start flashing: 6 toggles = 3 flashes
                            let tracking = self.menu_tracking.as_mut().unwrap();
                            tracking.flash_remaining = 6;
                            tracking.flash_delay = 3;
                            tracking.flash_result = result;
                        } else {
                            // No item selected — return 0 immediately
                            let sp = tracking.stack_ptr;
                            let saved = self.menu_tracking.take().unwrap();
                            self.restore_dropdown_pixels(
                                bus,
                                saved.dropdown_rect,
                                &saved.saved_pixels,
                            );
                            self.draw_menu_bar_to_fb(bus);
                            self.finish_menu_no_hit(bus, cpu, sp, 4);
                        }
                    } else {
                        // Button still held — update highlight
                        let (mv, mh) = self.mouse_pos;
                        let new_item = self.dropdown_item_at_point(mh, mv);

                        // Check if mouse moved to a different menu title
                        let new_menu = self.menu_title_hit_test(mh);
                        let tracking = self.menu_tracking.as_ref().unwrap();
                        let mbar_h =
                            bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
                        if let Some(new_idx) = new_menu {
                            if new_idx != tracking.active_menu && mv < mbar_h {
                                // Switch to different menu
                                let old_saved = self.menu_tracking.take().unwrap();
                                self.restore_dropdown_pixels(
                                    bus,
                                    old_saved.dropdown_rect,
                                    &old_saved.saved_pixels,
                                );
                                let sp = old_saved.stack_ptr;
                                self.open_menu_dropdown(bus, new_idx, sp);
                                // Don't advance PC
                                return Some(Ok(()));
                            }
                        }

                        let old_item = self.menu_tracking.as_ref().unwrap().highlighted_item;
                        if new_item != old_item {
                            // Erase old highlight
                            if old_item > 0 {
                                self.invert_menu_item(bus, old_item);
                            }
                            // Update tracking state
                            self.menu_tracking.as_mut().unwrap().highlighted_item = new_item;
                            // Draw new highlight
                            if new_item > 0 {
                                self.invert_menu_item(bus, new_item);
                            }
                        }
                        // Don't advance PC — stay on the trap
                    }
                } else {
                    // First call: read mouse position and open menu
                    let sp = cpu.read_reg(Register::A7);
                    let _pt_v = bus.read_word(sp) as i16;
                    let _pt_h = bus.read_word(sp + 2) as i16;
                    // Don't pop stack yet — we'll do that when tracking completes

                    let (_, mh) = self.mouse_pos;
                    if let Some(menu_idx) = self.menu_title_hit_test(mh) {
                        // Pop the Point parameter (4 bytes) but keep result space
                        // Stack on entry: SP+0: pt(4), SP+4: result(4)
                        // We store SP so we can write result later
                        self.open_menu_dropdown(bus, menu_idx, sp);
                        // Don't advance PC — re-fire on next iteration
                    } else {
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
                if let Some(ref tracking) = self.menu_tracking {
                    // Re-fire: popup tracking is active
                    if tracking.flash_remaining > 0 {
                        let result = tracking.flash_result;
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
                            self.restore_dropdown_pixels(
                                bus,
                                saved.dropdown_rect,
                                &saved.saved_pixels,
                            );
                            self.restore_visible_dialog_snapshots(bus);
                            // Stack: popUpItem(2) + left(2) + top(2) + menu(4) = 10 bytes
                            bus.write_long(sp + 10, result);
                            cpu.write_reg(Register::A7, sp + 10);
                        }
                    } else if !self.mouse_button {
                        if tracking.highlighted_item > 0 {
                            let menu = &self.menus[tracking.active_menu];
                            let item_idx = tracking.highlighted_item;
                            let result = ((menu.id as u32) << 16) | (item_idx as u32 & 0xFFFF);
                            let tracking = self.menu_tracking.as_mut().unwrap();
                            tracking.flash_remaining = 6;
                            tracking.flash_delay = 3;
                            tracking.flash_result = result;
                        } else {
                            let sp = tracking.stack_ptr;
                            let saved = self.menu_tracking.take().unwrap();
                            self.restore_dropdown_pixels(
                                bus,
                                saved.dropdown_rect,
                                &saved.saved_pixels,
                            );
                            self.restore_visible_dialog_snapshots(bus);
                            self.finish_menu_no_hit(bus, cpu, sp, 10);
                        }
                    } else {
                        // Button held — update highlight
                        let (mv, mh) = self.mouse_pos;
                        let new_item = self.dropdown_item_at_point(mh, mv);
                        let old_item = self.menu_tracking.as_ref().unwrap().highlighted_item;
                        if new_item != old_item {
                            if old_item > 0 {
                                self.invert_menu_item(bus, old_item);
                            }
                            self.menu_tracking.as_mut().unwrap().highlighted_item = new_item;
                            if new_item > 0 {
                                self.invert_menu_item(bus, new_item);
                            }
                        }
                    }
                } else {
                    // First call: read params and open popup dropdown
                    let sp = cpu.read_reg(Register::A7);
                    let _popup_item = bus.read_word(sp) as i16;
                    let left = bus.read_word(sp + 2) as i16;
                    let top = bus.read_word(sp + 4) as i16;
                    let menu_handle = bus.read_long(sp + 6);
                    // Stack: popUpItem(2) + left(2) + top(2) + menu(4) + result(4)
                    // Don't pop yet — store SP for result write later

                    let menu_ptr = bus.read_long(menu_handle);
                    let menu_id = bus.read_word(menu_ptr) as i16;

                    if let Some(menu_idx) = self.menus.iter().position(|m| m.id == menu_id) {
                        let (_, _, screen_width, screen_height, _) = self.get_screen_params();
                        let item_height: i16 = 16;
                        let num_items = self.menus[menu_idx].items.len() as i16;

                        // Compute max item width for dropdown
                        let font_id: i16 = 0;
                        let font_size: i16 = 12;
                        let mut max_width: i16 = 100;
                        for item in &self.menus[menu_idx].items {
                            let w = Self::fb_measure_string(&item.text, font_id, font_size) + 30;
                            max_width = max_width.max(w);
                        }

                        let dd_top = top;
                        let dd_left = left;
                        let dd_bottom = (dd_top + num_items * item_height + 2).min(screen_height);
                        let dd_right = (dd_left + max_width).min(screen_width);
                        let dd_rect = (dd_top, dd_left, dd_bottom, dd_right);

                        self.restore_visible_dialog_snapshots(bus);
                        let saved = self.save_dropdown_pixels(bus, dd_rect);
                        self.draw_menu_dropdown(bus, menu_idx, dd_rect);

                        self.menu_tracking = Some(MenuTrackingState {
                            active_menu: menu_idx,
                            highlighted_item: 0,
                            saved_pixels: saved,
                            dropdown_rect: dd_rect,
                            stack_ptr: sp,
                            flash_remaining: 0,
                            flash_delay: 0,
                            flash_result: 0,
                        });
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
            // Engines-agree subset (witnessed by
            // a938_hilitemenu_strict/):
            //   - A7 unchanged across the call after the 2-byte menuID
            //     argument is consumed (no FUNCTION result slot, no
            //     other stack frame).
            //
            // Engines-divergent (not engines-agree; intentionally not
            // witnessed in the strict bake):
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
            // MenuKey ($A93E): Searches enabled menu-bar menus for matching key
            // equivalent; scan order is right-to-left per IM:I I-355.
            (true, 0x13E) => {
                let sp = cpu.read_reg(Register::A7);
                let ch = (bus.read_word(sp) & 0xFF) as u8;

                // Search enabled items in enabled menus currently in the menu
                // list. IM:I I-355 documents right-to-left menu scan order.
                let mut result: u32 = 0;
                let ch_upper = (ch as char).to_ascii_uppercase() as u8;
                for menu in self.menus.iter().rev() {
                    if !menu.in_menu_bar || !menu.enabled {
                        continue;
                    }
                    for (i, item) in menu.items.iter().enumerate() {
                        if item.enabled
                            && item.key_equiv != 0
                            && (item.key_equiv as char).to_ascii_uppercase() as u8 == ch_upper
                        {
                            result = ((menu.id as u32) << 16) | ((i + 1) as u32);
                            break;
                        }
                    }
                    if result != 0 {
                        break;
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
                            "[MENUKEY]   menu {} \"{}\" in_bar={} enabled={} items={}",
                            menu.id,
                            menu.title,
                            menu.in_menu_bar,
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
                let menu_handle = bus.read_long(sp + 6);
                cpu.write_reg(Register::A7, sp + 10);
                if text_ptr != 0 && item >= 1 {
                    let text_len = bus.read_byte(text_ptr) as usize;
                    let mut text_bytes = Vec::with_capacity(text_len);
                    for i in 0..text_len {
                        text_bytes.push(bus.read_byte(text_ptr + 1 + i as u32));
                    }
                    let text = String::from_utf8_lossy(&text_bytes).into_owned();
                    let mut touched: Option<Menu> = None;
                    if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
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
                        serialise_menu_items_to_memory(bus, &m);
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
                if menu_handle != 0 {
                    let menu_ptr = bus.read_long(menu_handle);
                    if menu_ptr != 0 {
                        bus.free(menu_ptr);
                        self.ptr_to_handle.remove(&menu_ptr);
                    }
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
                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                        mi.icon = icon;
                    }
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
                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                        mi.style = style;
                    }
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
                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                        mi.mark = mark_char;
                    }
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
            // Regression coverage:
            //   calcmenusize_computes_dimensions
            //   calcmenusize_writes_width_and_height_to_menu_record
            // CalcMenuSize ($A948): Computes menuWidth/menuHeight and writes to MENU record per IM:I I-361
            (true, 0x148) => {
                let sp = cpu.read_reg(Register::A7);
                let menu_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);

                if let Some(menu) = self.menus.iter().find(|m| m.handle == menu_handle) {
                    let item_height: i16 = 16;
                    let num_items = menu.items.len() as i16;
                    let menu_height = num_items * item_height + 2;

                    let mut max_width: i16 = 0;
                    for item in &menu.items {
                        let w = Self::fb_measure_string(&item.text, 0, 12);
                        let key_extra = if item.key_equiv != 0 { 30 } else { 0 };
                        let mark_extra = if item.mark != 0 { 14 } else { 0 };
                        let total = w + key_extra + mark_extra + 24;
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
            // Regression coverage:
            //   setmenuflash_sets_flash_count
            //   setmenuflash_writes_to_menuflash_global
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
            // icons go through GetCIcon ($AA1F) → PlotCIcon ($AA1E)
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
            // PlotIcon ($A94B): OR-compress 1bpp / 8bpp pixel writes for shrink + nearest-neighbor for magnify per IM:V V-65 CopyBits scaling; ICON (32×32 mono) only — cicn handled via PlotCIcon $AA1E; NIL handle / NIL rect / NIL master ptr / zero-area / no-port are defensive no-ops; 16/32-bit colour fb silently no-op
            //
            // Pop-8 + no-port proof: a94b_ploticon_strict and
            // a94b_ploticon_noport_strict (BasiliskII-baked, registered
            // in catalogue test harness) witness A7 net-balance across
            // single and 5-call PlotIcon compositions and the
            // current-port-zero defensive no-op path.
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
            // Apple-vs-BasiliskII calling-convention agreement is
            // strict-baked by a94c_flashmenubar_strict/:
            //   - band B1 witnesses A7 unchanged across a single
            //     FlashMenuBar(0) call wrapped in one StackSpace
            //     ($A065) sandwich.
            //   - band B2 witnesses A7 unchanged across a 5-call
            //     FlashMenuBar(0) composition wrapped in one
            //     StackSpace sandwich (5 missed 2-byte pops would
            //     cumulate to 10 bytes A7 drift, which is unambiguous
            //     regardless of StackSpace's rounding).
            //
            // The contract test
            // `flashmenubar_five_call_composition_advances_stack_by_ten_bytes`
            // in the `mod tests` block mirrors B2 surgically.
            //
            // Systemless HLE compromise (engines-divergent side effect):
            // BasiliskII System 7.5.3 ROM Menu Manager inverts the
            // menu bar (or the named menu title) once per call by
            // writing to the WMgrPort frame buffer; "FlashMenuBar(0)
            // twice to blink the menu bar" is the documented idiom.
            // Systemless's host runtime draws the menu bar directly from
            // the Rust menu list and redraws the entire chrome once
            // per frame, so there is no separate invert/wait/invert
            // double-paint loop and the trap is structurally a no-op
            // beyond the 2-byte arg pop. Apps using FlashMenuBar for
            // "operation complete" visual feedback see no flash on
            // Systemless; the documented Tool-bit Pascal PROCEDURE
            // calling convention is engines-agree regardless.
            (true, 0x14C) => {
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 2);
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
                    serialise_menu_items_to_memory(bus, &m);
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
                if let Some(menu) = self.menus.iter_mut().find(|m| m.handle == menu_handle) {
                    let idx = (item - 1) as usize;
                    if idx < menu.items.len() {
                        menu.items.remove(idx);
                        touched = Some(menu.clone());
                    }
                }
                // Keep guest-memory MENU record in sync with the deletion
                // so CountMItems / CalcMenuSize don't still see the item.
                if let Some(m) = touched {
                    serialise_menu_items_to_memory(bus, &m);
                    sync_enable_flags(bus, &m);
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
                    serialise_menu_items_to_memory(bus, &m);
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
            // Apple-vs-BasiliskII alignment on the calling convention:
            // both engines consume the 2-byte mbResID and preserve A7
            // across the call. Witnessed by the strict bake fixture
            //   a808_initprocmenu_strict
            // (single + 5-call composition StackSpace sandwich with
            // mbResID=0).
            //
            // Apple-vs-BasiliskII divergence on the side effect:
            // BasiliskII System 7.5.3 ROM Menu Manager allocates the
            // MenuList if not yet allocated, stores mbResID, and (when
            // the high 13 bits select a non-default MBDF) loads the
            // 'MBDF' resource. Systemless HLE is a true pop-2-and-return
            // stub because the host runtime draws the menu bar
            // directly from the Rust menu list — there is no separate
            // MBDF resource to honour. The visible "MBDF resource
            // gets loaded" path is engines-divergent.
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
            // Apple-vs-BasiliskII alignment on the calling convention:
            // both engines preserve A7 across the call. Witnessed by
            // the strict bake fixture
            //   a81d_invalmenubar_strict
            // (single + 5-call composition StackSpace sandwich).
            //
            // Apple-vs-BasiliskII divergence on the side effect:
            // BasiliskII System 7.5.3 ROM Menu Manager sets the
            // documented menu-bar-invalid flag honored by GetNextEvent.
            // Systemless HLE is a true no-op (`Ok(())`) because the host
            // runtime redraws the entire chrome per frame from the
            // current menu list, so there is no separate "dirty" flag
            // to honor. The visible-side-effect "menu bar gets
            // redrawn" path is engines-divergent and reserved for
            // in-Rust state inspection.
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
                        if let Some(mi) = menu.items.get_mut((item - 1) as usize) {
                            mi.key_equiv = cmd_char;
                        } else {
                            return Some(Ok(()));
                        }
                        menu.clone()
                    } else {
                        return Some(Ok(()));
                    };
                serialise_menu_items_to_memory(bus, &menu_clone);
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
            // AA60..AA65 family can mutate/query real guest state. We still
            // do not auto-load 'mctb' resources here, and the chrome paint
            // path still ignores MC state, but the table itself is now real:
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
            //     writes, so the strict fixture seeds MenuDisable directly
            //     and witnesses the lowmem read path explicitly.

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
            // Engines-divergent absolute behavior (not witnessed): BII
            // mutates the system menu color information table at lowmem
            // MenuCInfo ($0D50). Systemless HLE now mirrors that live-table
            // mutation for exact (menuID, menuItem) matches; the engine-
            // agree subset is still the Pascal PROCEDURE calling
            // convention itself: A7 advances by exactly 4 bytes per
            // call.
            //
            // Strict bake aa60_aa65_menu_color_family_strict B1 witnesses
            // the calling convention via single + 5-call composition
            // StackSpace sandwich combined into one boolean.
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
            // Engines-divergent absolute MCTableHandle (not witnessed):
            // BII may return a non-NIL handle pointing into a system-
            // populated MC table. Systemless now returns a deep copy of the
            // live MenuCInfo table when one exists and NIL when no table
            // has been created yet. The NIL path remains the IM-documented
            // copy-failure return value.
            //
            // Strict bake aa60_aa65_menu_color_family_strict B2 witnesses
            // the Pascal FUNCTION calling convention via single + 5-call
            // composition StackSpace sandwich combined into one boolean.
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
            // Engines-divergent absolute behavior (not witnessed): BII
            // mutates lowmem MenuCInfo ($0D50). Systemless HLE now copies
            // the source table into the live MenuCInfo handle and leaves
            // the source handle alone, preserving the documented
            // "current table is preserved on failure" contract for a NIL
            // source.
            //
            // Strict bake aa60_aa65_menu_color_family_strict B3 witnesses
            // the calling convention with a NIL handle arg (engines-safe
            // per IM:V V-247: NIL source triggers the copy-failure path
            // and the current table is preserved on BII).
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
            // Engines-divergent absolute behavior (not witnessed): BII
            // calls DisposHandle on the caller-supplied handle. Systemless
            // HLE now does the same on the supplied handle while leaving
            // the current MenuCInfo table untouched.
            //
            // Strict bake aa60_aa65_menu_color_family_strict B4 witnesses
            // the calling convention with a NIL handle arg (engines-safe:
            // DisposHandle on NIL is a documented no-op on classic Mac).
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
            // Engines-divergent absolute MCEntryPtr (not witnessed):
            // BII may return a non-NIL pointer when (menuID, menuItem)
            // matches a system-populated entry. Systemless now returns a
            // pointer into the live MenuCInfo table when the exact pair
            // exists, and NIL when it does not.
            //
            // Strict bake aa60_aa65_menu_color_family_strict B5 witnesses
            // the Pascal FUNCTION calling convention via single + 5-call
            // composition StackSpace sandwich combined into one boolean.
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
            // Engines-divergent absolute behavior (not witnessed): BII
            // iterates the caller-supplied array and mutates lowmem
            // MenuCInfo ($0D50). Systemless HLE now updates the live table
            // with exact (menuID, menuItem) matches and appends new
            // entries when needed.
            //
            // Strict bake aa60_aa65_menu_color_family_strict B6 witnesses
            // the calling convention with (numEntries=0, menuCEntries=
            // NIL) args (engines-safe: zero-entry loop is skipped on BII).
            (true, 0x265) => {
                let sp = cpu.read_reg(Register::A7);
                let num_entries = bus.read_word(sp + 4) as i16;
                let entries_ptr = bus.read_long(sp);
                if num_entries > 0 && entries_ptr != 0 {
                    let current_handle = self.ensure_menu_color_table_handle(bus);
                    if current_handle != 0 {
                        let current_bytes = Self::menu_color_table_bytes(bus, current_handle);
                        let mut new_bytes = current_bytes.clone();
                        for index in 0..num_entries as usize {
                            let entry_ptr = entries_ptr + (index as u32 * MC_ENTRY_SIZE as u32);
                            let entry = bus.read_bytes(entry_ptr, MC_ENTRY_SIZE);
                            if let Some((menu_id, menu_item)) = mc_entry_key(&entry) {
                                let mut found_offset = None;
                                for (entry_index, existing) in
                                    new_bytes.chunks_exact(MC_ENTRY_SIZE).enumerate()
                                {
                                    if mc_entry_matches(existing, menu_id, menu_item) {
                                        found_offset = Some(entry_index * MC_ENTRY_SIZE);
                                        break;
                                    }
                                }
                                if let Some(offset) = found_offset {
                                    new_bytes[offset..offset + MC_ENTRY_SIZE]
                                        .copy_from_slice(&entry);
                                } else {
                                    new_bytes.extend_from_slice(&entry);
                                }
                            }
                        }
                        let _ = self.replace_handle_bytes(bus, current_handle, &new_bytes);
                    }
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
            // lowmem global directly to witness the read path.
            //
            // Tool-bit Pascal FUNCTION calling convention: A7 unchanged
            // across the C-level call sequence (caller pre-push of 4-byte
            // result slot + trap-side result-slot write + caller post-pop
            // balance). The strict fixture witnesses both the ABI and the
            // lowmem read behavior.
            //
            // Engines-agree subset (witnessed):
            //   * Pascal FUNCTION calling convention: A7 unchanged
            //     across the C-level call (caller pre-push + trap
            //     result-slot write + caller post-pop balance) —
            //     witnessed both for a single call (aa66_..._strict
            //     B1) and for a 5-call composition (B2).
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

    /// Open a menu dropdown and start tracking.
    fn open_menu_dropdown(&mut self, bus: &mut MacMemoryBus, menu_idx: usize, stack_ptr: u32) {
        let (_screen_base, _row_bytes, screen_width, screen_height, _pixel_size) =
            self.get_screen_params();

        // Compute dropdown rect
        let regions = self.menu_title_regions();
        if menu_idx >= regions.len() || menu_idx >= self.menus.len() {
            return;
        }
        let (title_left, title_right) = regions[menu_idx];
        let menu = &self.menus[menu_idx];

        let item_height: i16 = 16;
        let dropdown_top: i16 = 20; // Below menu bar
        let dropdown_left: i16 = title_left;

        // Compute dropdown width: max of item text widths + padding
        let mut max_width: i16 = 0;
        for item in &menu.items {
            let w = Self::fb_measure_string(&item.text, 0, 12);
            let key_extra = if item.key_equiv != 0 { 30 } else { 0 };
            let mark_extra = if item.mark != 0 { 14 } else { 0 };
            let total = w + key_extra + mark_extra + 24; // padding
            if total > max_width {
                max_width = total;
            }
        }
        // At least as wide as the title
        max_width = max_width.max(title_right - title_left + 20);
        max_width = max_width.max(100); // minimum width

        let dropdown_bottom =
            (dropdown_top + menu.items.len() as i16 * item_height + 2).min(screen_height);
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
            if !menu.in_menu_bar {
                continue;
            }
            let width = Self::fb_measure_string(&menu.title, 0, 12);
            let left = x - 7; // padding before title
            let right = x + width + 7; // padding after title
            regions.push((menu_idx, left, right));
            x += width + 14;
        }
        regions
    }

    /// Determine which item (1-based) is at the given screen point, or 0.
    fn dropdown_item_at_point(&self, mouse_x: i16, mouse_y: i16) -> i16 {
        if let Some(ref tracking) = self.menu_tracking {
            let (top, left, bottom, right) = tracking.dropdown_rect;
            if mouse_x >= left && mouse_x < right && mouse_y >= top && mouse_y < bottom {
                let menu = &self.menus[tracking.active_menu];
                let item_height: i16 = 16;
                let item_idx = (mouse_y - top - 1) / item_height; // 0-based
                if item_idx >= 0 && (item_idx as usize) < menu.items.len() {
                    let item = &menu.items[item_idx as usize];
                    // Don't highlight separators or disabled items
                    if item.text == "-" || !item.enabled {
                        return 0;
                    }
                    return item_idx + 1; // 1-based
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
        let item_height: i16 = 16;

        // Fill white
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

        // Draw border
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

        // Shadow (right edge + bottom edge)
        for y in (top + 2)..=bottom {
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
            left + 2,
            right + 1,
            true,
        );

        // Draw items
        let font_id: i16 = 0;
        let font_size: i16 = 12;
        let metrics = crate::quickdraw::text::get_font_metrics(font_id, font_size);

        for (i, item) in menu.items.iter().enumerate() {
            let item_top = top + 1 + i as i16 * item_height;
            let text_y =
                item_top + (item_height - (metrics.ascent + metrics.descent)) / 2 + metrics.ascent;

            if item.text == "-" {
                // Separator: dotted line across the middle of the item row.
                // Inside Macintosh Volume I, I-359
                let sep_y = item_top + item_height / 2;
                for x in (left + 1)..(right - 1) {
                    if x % 2 == 0 {
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
                continue;
            }

            // Draw mark character if present (0x12 = checkmark, others rendered as-is).
            // Inside Macintosh Volume I, I-358
            let text_left = if item.mark != 0 {
                // Map Mac Roman mark byte to a renderable string.
                // Mac character 0x12 (18) is the standard checkmark in Chicago.
                let mark_str: std::borrow::Cow<str> = if item.mark == 0x12 {
                    "\u{2713}".into() // ✓
                } else {
                    let s = String::from(item.mark as char);
                    s.into()
                };
                Self::fb_draw_string(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left + 4,
                    text_y,
                    &mark_str,
                    font_id,
                    font_size,
                );
                left + 18
            } else {
                left + 18
            };

            // Draw item text
            Self::fb_draw_string(
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
            );

            // Draw key equivalent on right side
            if item.key_equiv != 0 {
                let cmd_str = format!("\u{2318}{}", item.key_equiv as char);
                let cmd_width = Self::fb_measure_string(&cmd_str, font_id, font_size);
                Self::fb_draw_string(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    right - cmd_width - 8,
                    text_y,
                    &cmd_str,
                    font_id,
                    font_size,
                );
            }

            // Gray out disabled items by drawing a white dither pattern over them
            if !item.enabled {
                for y in item_top..(item_top + item_height) {
                    for x in (left + 1)..(right - 1) {
                        if (x + y) % 2 == 0 {
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
        rect: (i16, i16, i16, i16),
        item: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (top, left, _bottom, right) = rect;
        let item_height: i16 = 16;
        let item_top = top + 1 + (item - 1) * item_height;
        let item_bottom = item_top + item_height;
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
                    } else {
                        let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, 255 - b);
                    }
                }
            }
        }
    }

    /// Invert a menu item row in the dropdown (for highlighting).
    pub(super) fn invert_menu_item(&self, bus: &mut MacMemoryBus, item: i16) {
        if let Some(ref tracking) = self.menu_tracking {
            self.invert_dropdown_item_rect(bus, tracking.dropdown_rect, item);
        }
    }

    /// Highlight a menu title in the menu bar (invert it).
    pub(super) fn highlight_menu_title(&self, bus: &mut MacMemoryBus, menu_idx: usize) {
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
        // Invert the title area in the menu bar
        for y in 1i16..19 {
            for x in left..right {
                if x >= 0 && x < screen_width && y >= 0 && y < screen_height {
                    if pixel_size == 1 {
                        let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
                        let bit = 7 - (x as u32 % 8);
                        let addr = screen_base + byte_offset;
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, b ^ (1 << bit));
                    } else {
                        let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                        let b = bus.read_byte(addr);
                        bus.write_byte(addr, 255 - b);
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
    use super::{parse_appendmenu_items, Menu, MenuItem};
    use crate::cpu::{CpuOps, Register};
    use crate::memory::MemoryBus;
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
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
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

    // InvalMenuBar ($A81D) — mirrors B1 of a81d_invalmenubar_strict.
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

        // 5-call composition mirrors the bake's second StackSpace
        // sandwich. Catches per-call drift that a single sandwich
        // might mask under StackSpace's rounding.
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

    // InitProcMenu ($A808) — mirrors B1 of a808_initprocmenu_strict.
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
        // value. Mirrors the strict bake's B1 5-call StackSpace sandwich
        // and catches cumulative pop-size drift (e.g. pop-0 → +10,
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
    fn popupmenuselect_nohit_path_preserves_stack_and_returns_zero() {
        // Inside Macintosh Volume V (1986), p. V-229:
        // PopUpMenuSelect(menu, top, left, popUpItem) returns the
        // selected item. The first call seeds the popup-tracking state;
        // the re-fired no-hit path returns 0 without disturbing the
        // caller stack.
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

        let first = disp.dispatch_menu(true, 0x00B, &mut cpu, &mut bus);
        assert!(first.is_some(), "PopUpMenuSelect should be handled");
        assert!(
            first.unwrap().is_ok(),
            "PopUpMenuSelect should seed tracking"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp,
            "first re-fire should defer stack pop"
        );

        let second = disp.dispatch_menu(true, 0x00B, &mut cpu, &mut bus);
        assert!(second.is_some(), "PopUpMenuSelect should be re-fired");
        assert!(second.unwrap().is_ok(), "PopUpMenuSelect should return");
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

    // Mirrors band B2 of a938_hilitemenu_strict/:
    // five HiliteMenu(0) dispatches in sequence preserve A7 cumulatively.
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

    // Mirrors band B2 of a94c_flashmenubar_strict/:
    // five FlashMenuBar(0) dispatches in sequence preserve A7 cumulatively.
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

    // Mirrors band B2 of aa66_menuchoice_strict/:
    // five MenuChoice() Pascal FUNCTION dispatches in sequence preserve
    // A7 cumulatively. Per IM:MTb 1992 p. 3-118 and MPW Universal Headers
    // Menus.h, MenuChoice is a parameterless Tool-bit Pascal FUNCTION
    // returning LongInt — the caller pre-pushes a 4-byte result slot,
    // the trap writes [SP+0] without modifying A7, and the caller pops
    // the slot. Wrapping each dispatch in a manual pre-push/post-pop
    // pair witnesses that the trap itself leaves A7 unchanged across
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

    // Mirrors the MenuChoice lowmem read path witnessed by
    // aa66_menuchoice_strict/ after seeding
    // lowmem MenuDisable directly. Per IM:MTb 1992 p. 3-118..3-119,
    // MenuChoice returns the packed (menuID, itemNumber) stored in
    // MenuDisable when MenuSelect / MenuKey have tracked a disabled
    // item. This contract test seeds the lowmem word and checks that
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

    // Mirrors band B1..B6 of aa60_aa65_menu_color_family_strict/:
    // per IM:V 1986 pp. V-247..V-248 the Menu Color Manager family is six
    // Tool-bit Pascal routines with the following stack disciplines:
    //   AA60 DelMCEntries  — PROCEDURE pop-4 (2xINTEGER)
    //   AA61 GetMCInfo     — FUNCTION  parameterless + 4-byte result slot
    //   AA62 SetMCInfo     — PROCEDURE pop-4 (1xHandle)
    //   AA63 DispMCInfo    — PROCEDURE pop-4 (1xHandle)
    //   AA64 GetMCEntry    — FUNCTION  2xINTEGER + 4-byte result slot
    //   AA65 SetMCEntries  — PROCEDURE pop-6 (1xINTEGER + 1xPtr)
    // This contract test mirrors the strict bake's 5-call composition by
    // dispatching 5 successive calls of each trap with the appropriate
    // pre-pushed Pascal arg frame and (for FUNCTIONs) result slot,
    // asserting cumulative A7 net-balance across each family.
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
    }

    // IM:I I-355: MenuSelect enters tracking while a menu title is active
    // and does not immediately return a final menuResult.
    #[test]
    fn menuselect_title_hit_enters_tracking_without_immediate_stack_pop() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
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
        // FUNCTION result slot. This test mirrors B2 of the
        // a94b_ploticon_strict catalog test bake: 5 successive
        // PlotIcon calls each with distinct icon Handles and
        // destination Rects net-balance A7.
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
}
