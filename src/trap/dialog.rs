//! Dialog Manager, Cursor Manager, and misc stub trap handlers.

use crate::memory::SavedPixels;
use super::dispatch::{
    selector_operation_route, DialogItem, DialogPopupDraw, DialogPopupTrackingState,
    DialogTrackingState, PendingDialogPopupMenu, PersistentDialogSnapshot, QueuedEvent,
    RetainedModalDialogClickState, SelectorOperationRoute,
};
use super::types::{decode_mac_roman, encode_mac_roman_lossy, Rect, ShapeOp};
use crate::cpu::{CpuOps, Register};
use crate::display::CursorImage;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::quickdraw::fonts::get_font_face_scaled;
use crate::quickdraw::text::get_font_metrics;
use crate::ui_theme::{ControlKind, UiThemeId};
use crate::Result;
use std::collections::VecDeque;
use std::sync::OnceLock;

static TRACE_DIALOG_PROCS: OnceLock<bool> = OnceLock::new();
static TRACE_DIALOG_FILTER: OnceLock<bool> = OnceLock::new();
static TRACE_TEXTEDIT: OnceLock<bool> = OnceLock::new();
static TRACE_DIALOG_ITEMS: OnceLock<bool> = OnceLock::new();
static TRACE_DIALOG_TEXT_INLINE: OnceLock<bool> = OnceLock::new();
const MANAGER_DIALOG_RECORD_ALIGNMENT: u32 = 256;

const TE_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_te_dispatch_operations.rs");

fn te_dispatch_operation_route(
    trap_word: u16,
    selector: u16,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA83D {
        return None;
    }
    selector_operation_route(TE_DISPATCH_OPERATION_ROUTES, u32::from(selector))
}

struct DialogCIconLayout {
    width: i16,
    height: i16,
    pm_row_bytes: u32,
    mask_row_bytes: u32,
    bmap_row_bytes: u32,
    pixel_size: u16,
    mask_data_ptr: u32,
    bmap_data_ptr: u32,
    ctab_ptr: u32,
    ct_size: u16,
    pixel_data_ptr: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DialogItemTextStyle {
    font: i16,
    face: u8,
    size: i16,
    foreground: Option<[u16; 3]>,
    background: Option<[u16; 3]>,
    mode: i16,
}

#[derive(Clone, Copy)]
struct FullscreenOffscreenDialogRestoreCandidate {
    base: u32,
    row_bytes: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
    ctab_handle: u32,
    score: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TeResolvedStyle {
    font: i16,
    face: i16,
    size: i16,
    color: (u16, u16, u16),
    line_height: i16,
    ascent: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TeStyleRun {
    start: usize,
    style_index: usize,
    style: TeResolvedStyle,
}

fn trace_dialog_procs_enabled() -> bool {
    *TRACE_DIALOG_PROCS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_PROCS").is_some())
}

fn trace_dialog_filter_enabled() -> bool {
    *TRACE_DIALOG_FILTER
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_FILTER").is_some())
}

fn trace_textedit_enabled() -> bool {
    *TRACE_TEXTEDIT.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_TEXTEDIT").is_some())
}

fn trace_dialog_items_enabled() -> bool {
    *TRACE_DIALOG_ITEMS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_ITEMS").is_some())
}

fn trace_dialog_text_inline_enabled() -> bool {
    *TRACE_DIALOG_TEXT_INLINE
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_TEXT").is_some())
}

fn input_trace_event_name(what: u16) -> &'static str {
    match what {
        0 => "nullEvent",
        1 => "mouseDown",
        2 => "mouseUp",
        3 => "keyDown",
        4 => "keyUp",
        5 => "autoKey",
        6 => "updateEvt",
        7 => "diskEvt",
        8 => "activateEvt",
        _ => "unknownEvent",
    }
}

fn input_trace_nonzero(value: u32) -> String {
    if value == 0 {
        "$00000000".to_string()
    } else {
        "$NONZERO".to_string()
    }
}

fn input_trace_event_message(what: u16, message: u32) -> String {
    match what {
        6 | 8 => input_trace_nonzero(message),
        _ => format!("${message:08X}"),
    }
}

fn input_trace_text_bytes(value: &str) -> String {
    if value.is_empty() {
        return "empty".to_string();
    }
    let mut out = String::from("hex:");
    for byte in value.as_bytes() {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

impl super::TrapDispatcher {
    fn record_dialog_input_trace(
        &mut self,
        trap: &str,
        event_ptr: u32,
        what: u16,
        message: u32,
        where_v: i16,
        where_h: i16,
        modifiers: u16,
        active_dialog: Option<u32>,
        result: bool,
        detail: &str,
    ) {
        if !self.input_trace_enabled {
            return;
        }
        let active_dialog = active_dialog
            .map(input_trace_nonzero)
            .unwrap_or_else(|| "none".to_string());
        let mut line = format!(
            "{trap} action=guest_event:{} event_ptr={} delivered=EventRecord{{what={}({}), message={}, where=({where_v},{where_h}), modifiers=${modifiers:04X}}} {} active_dialog={} result={}",
            input_trace_event_name(what),
            input_trace_nonzero(event_ptr),
            input_trace_event_name(what),
            what,
            input_trace_event_message(what, message),
            self.input_trace_state_fields(),
            active_dialog,
            if result { "true" } else { "false" },
        );
        if !detail.is_empty() {
            line.push(' ');
            line.push_str(detail);
        }
        self.record_input_trace_line(line);
    }

    fn record_modal_dialog_input_trace(
        &mut self,
        action: &str,
        dialog_ptr: u32,
        bounds: (i16, i16, i16, i16),
        item_hit: i16,
        item_type: Option<u8>,
        highlighted: Option<bool>,
        result: &str,
        outcome: &str,
    ) {
        if !self.input_trace_enabled {
            return;
        }
        let item_type = item_type
            .map(|value| format!("${value:02X}"))
            .unwrap_or_else(|| "none".to_string());
        let highlighted = highlighted
            .map(|value| if value { "true" } else { "false" }.to_string())
            .unwrap_or_else(|| "none".to_string());
        self.record_input_trace_line(format!(
            "A991 action={} live_mouse=({},{}) {} dialog={} bounds=({},{},{},{}) item_hit={} item_type={} highlighted={} result={} outcome={}",
            action,
            self.input_state.mouse_pos.0,
            self.input_state.mouse_pos.1,
            self.input_trace_state_fields(),
            input_trace_nonzero(dialog_ptr),
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            item_hit,
            item_type,
            highlighted,
            result,
            outcome,
        ));
    }

    fn record_modal_dialog_text_input_trace(
        &mut self,
        action: &str,
        dialog_ptr: u32,
        bounds: (i16, i16, i16, i16),
        edit_item: i16,
        item_type: Option<u8>,
        key_code: u8,
        char_code: u8,
        text_before: &str,
        text_after: &str,
        result: &str,
        outcome: &str,
    ) {
        if !self.input_trace_enabled {
            return;
        }
        let item_type = item_type
            .map(|value| format!("${value:02X}"))
            .unwrap_or_else(|| "none".to_string());
        self.record_input_trace_line(format!(
            "A991 action={} live_mouse=({},{}) {} dialog={} bounds=({},{},{},{}) edit_item={} item_type={} key_code=${:02X} char_code=${:02X} text_before={} text_after={} result={} outcome={}",
            action,
            self.input_state.mouse_pos.0,
            self.input_state.mouse_pos.1,
            self.input_trace_state_fields(),
            input_trace_nonzero(dialog_ptr),
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            edit_item,
            item_type,
            key_code,
            char_code,
            input_trace_text_bytes(text_before),
            input_trace_text_bytes(text_after),
            result,
            outcome,
        ));
    }

    fn record_modal_dialog_filter_input_trace(
        &mut self,
        action: &str,
        dialog_ptr: u32,
        bounds: (i16, i16, i16, i16),
        filter_proc: u32,
        event: Option<&QueuedEvent>,
        item_hit: i16,
        item_type: Option<u8>,
        handled_mouse_down: bool,
        dialog_retained: bool,
        result: &str,
        outcome: &str,
    ) {
        if !self.input_trace_enabled {
            return;
        }
        let item_type = item_type
            .map(|value| format!("${value:02X}"))
            .unwrap_or_else(|| "none".to_string());
        let (event_name, message, where_text, modifiers) = if let Some(event) = event {
            (
                format!("{}({})", input_trace_event_name(event.what), event.what),
                input_trace_event_message(event.what, event.message),
                format!("({},{})", event.where_v, event.where_h),
                format!("${:04X}", event.modifiers),
            )
        } else {
            (
                "none".to_string(),
                "none".to_string(),
                "none".to_string(),
                "none".to_string(),
            )
        };
        self.record_input_trace_line(format!(
            "A991 action={} live_mouse=({},{}) {} dialog={} bounds=({},{},{},{}) filter_proc={} event={} message={} where={} modifiers={} item_hit={} item_type={} handled_mouse_down={} dialog_retained={} result={} outcome={}",
            action,
            self.input_state.mouse_pos.0,
            self.input_state.mouse_pos.1,
            self.input_trace_state_fields(),
            input_trace_nonzero(dialog_ptr),
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            input_trace_nonzero(filter_proc),
            event_name,
            message,
            where_text,
            modifiers,
            item_hit,
            item_type,
            handled_mouse_down,
            dialog_retained,
            result,
            outcome,
        ));
    }

    fn dialog_template_res_err(&self, dialog_id: i16) -> i16 {
        if self.find_loaded_resource_any(*b"DLOG", dialog_id).is_some() {
            0
        } else {
            0
        }
    }

    fn loaded_resource_handle_in_file(
        &self,
        res_type: [u8; 4],
        res_id: i16,
        ptr: u32,
        refnum: u16,
    ) -> Option<u32> {
        self.loaded_handles
            .iter()
            .find_map(|(&handle, &(loaded_ptr, loaded_type, loaded_id))| {
                if loaded_ptr == ptr
                    && loaded_type == res_type
                    && loaded_id == res_id
                    && self.resource_handle_files.get(&handle).copied() == Some(refnum)
                {
                    Some(handle)
                } else {
                    None
                }
            })
    }

    fn set_handle_purgeable(&mut self, handle: u32, purgeable: bool) {
        if handle == 0 {
            return;
        }

        if purgeable {
            self.update_handle_state_bits(handle, |bits| Some(bits.unwrap_or(0) | 0x40));
        } else {
            self.update_handle_state_bits(handle, |bits| {
                let bits = bits.unwrap_or(0) & !0x40;
                (bits != 0).then_some(bits)
            });
        }
    }

    fn prepare_dialog_resource_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        res_type: [u8; 4],
        res_id: i16,
        purgeable: bool,
        load_if_missing: bool,
    ) -> Option<(u32, u32)> {
        let (refnum, ptr) = if load_if_missing {
            self.find_or_load_resource_any(bus, res_type, res_id)?
        } else {
            self.find_loaded_resource_any(res_type, res_id)?
        };
        let handle = if load_if_missing {
            let handle =
                self.get_or_create_resource_handle_in_file(bus, res_type, res_id, ptr, refnum);
            if ptr != 0 && bus.read_long(handle) == 0 {
                bus.write_long(handle, ptr);
                self.track_handle_ptr(ptr, handle);
            }
            handle
        } else {
            self.loaded_resource_handle_in_file(res_type, res_id, ptr, refnum)?
        };

        self.set_handle_purgeable(handle, purgeable);
        Some((handle, ptr))
    }

    fn dialog_item_resource_type(item_type: u8) -> Option<[u8; 4]> {
        match item_type & 0x7F {
            7 => Some(*b"CNTL"),
            32 => Some(*b"ICON"),
            64 => Some(*b"PICT"),
            _ => None,
        }
    }

    fn cascade_dialog_resource_purgeability(
        &mut self,
        bus: &mut MacMemoryBus,
        template_type: [u8; 4],
        template_id: i16,
        purgeable: bool,
        load_if_missing: bool,
    ) {
        let Some((_, template_ptr)) = self.prepare_dialog_resource_handle(
            bus,
            template_type,
            template_id,
            purgeable,
            load_if_missing,
        ) else {
            return;
        };

        let items_id = match template_type {
            // IM:I I-437: DLOG item-list ID is at offset 18.
            [b'D', b'L', b'O', b'G'] if bus.get_alloc_size(template_ptr).unwrap_or(0) >= 20 => {
                bus.read_word(template_ptr + 18) as i16
            }
            // IM:I I-425..I-426: ALRT item-list ID is at offset 8.
            [b'A', b'L', b'R', b'T'] if bus.get_alloc_size(template_ptr).unwrap_or(0) >= 10 => {
                bus.read_word(template_ptr + 8) as i16
            }
            _ => return,
        };

        let Some((_, ditl_ptr)) = self.prepare_dialog_resource_handle(
            bus,
            *b"DITL",
            items_id,
            purgeable,
            load_if_missing,
        ) else {
            return;
        };

        let ditl_len = bus.get_alloc_size(ditl_ptr).unwrap_or(0);
        let items = Self::parse_ditl(bus, ditl_ptr, ditl_len);
        for item in items {
            if let Some(res_type) = Self::dialog_item_resource_type(item.item_type) {
                self.prepare_dialog_resource_handle(
                    bus,
                    res_type,
                    item.resource_id,
                    purgeable,
                    load_if_missing,
                );
            }
        }
    }

    const TE_DEST_RECT_OFFSET: u32 = 0x00;
    const TE_VIEW_RECT_OFFSET: u32 = 0x08;
    const TE_SEL_RECT_OFFSET: u32 = 0x10;
    const TE_LINE_HEIGHT_OFFSET: u32 = 0x18;
    const TE_FONT_ASCENT_OFFSET: u32 = 0x1A;
    const TE_SEL_POINT_OFFSET: u32 = 0x1C;
    const TE_SEL_START_OFFSET: u32 = 0x20;
    const TE_SEL_END_OFFSET: u32 = 0x22;
    const TE_ACTIVE_OFFSET: u32 = 0x24;
    const TE_CARET_TIME_OFFSET: u32 = 0x34;
    const TE_CARET_STATE_OFFSET: u32 = 0x38;
    #[allow(dead_code)]
    const TE_CR_ONLY_OFFSET: u32 = 0x48;
    const TE_JUST_OFFSET: u32 = 0x3A;
    const TE_LENGTH_OFFSET: u32 = 0x3C;
    const TE_HTEXT_OFFSET: u32 = 0x3E;
    const TE_TX_FONT_OFFSET: u32 = 0x4A;
    const TE_TX_FACE_OFFSET: u32 = 0x4C;
    const TE_TX_MODE_OFFSET: u32 = 0x4E;
    const TE_TX_SIZE_OFFSET: u32 = 0x50;
    const TE_IN_PORT_OFFSET: u32 = 0x52;
    const TE_N_LINES_OFFSET: u32 = 0x5E;
    const TE_LINE_STARTS_OFFSET: u32 = 0x60;
    const TE_REC_MIN_SIZE: u32 = 128;
    const TE_CARET_BLINK_TICKS: u32 = 32;
    // TextEdit draws inside the destination rectangle (IM:I I-373 to I-374);
    // BasiliskII/System 7.5.3 `dialog_visual_textedit_smoke` pins the ROM's
    // flush-left glyph origin one pixel in from destRect.left.
    const TE_LINE_LEFT_INSET: i16 = 1;

    const DBOX_FRAME_MARGIN: i16 = 8;
    const EDIT_TEXT_FRAME_OUTSET: i16 = 3;
    const STANDARD_CONTROL_MARK_SIZE: i16 = 12;
    const STANDARD_CONTROL_MARK_LEFT_INSET: i16 = 2;
    const STANDARD_CONTROL_TITLE_GAP: i16 = 4;
    const CLASSIC_RADIO_OUTLINE_12: [&'static str; 12] = [
        "....####....",
        "..##....##..",
        ".#........#.",
        ".#........#.",
        "#..........#",
        "#..........#",
        "#..........#",
        "#..........#",
        ".#........#.",
        ".#........#.",
        "..##....##..",
        "....####....",
    ];
    const CLASSIC_RADIO_DOT_12: [&'static str; 12] = [
        "............",
        "............",
        "............",
        "....####....",
        "...######...",
        "...######...",
        "...######...",
        "...######...",
        "....####....",
        "............",
        "............",
        "............",
    ];

    const TE_STYLE_N_RUNS_OFFSET: u32 = 0x00;
    const TE_STYLE_N_STYLES_OFFSET: u32 = 0x02;
    const TE_STYLE_STYLE_TABLE_OFFSET: u32 = 0x04;
    const TE_STYLE_LH_TABLE_OFFSET: u32 = 0x08;
    const TE_STYLE_NULL_STYLE_OFFSET: u32 = 0x10;
    const TE_STYLE_RUNS_OFFSET: u32 = 0x14;

    const ST_ELEMENT_REFCOUNT_OFFSET: u32 = 0x00;
    const ST_ELEMENT_HEIGHT_OFFSET: u32 = 0x02;
    const ST_ELEMENT_ASCENT_OFFSET: u32 = 0x04;
    const ST_ELEMENT_FONT_OFFSET: u32 = 0x06;
    // Style is a one-byte SET followed by one alignment byte in TextStyle,
    // STElement, and ScrpSTElement records. Inside Macintosh: Text 1993,
    // pp. 2-72, 2-78, and 2-80.
    const ST_ELEMENT_FACE_OFFSET: u32 = 0x08;
    const ST_ELEMENT_SIZE_OFFSET: u32 = 0x0A;
    const ST_ELEMENT_COLOR_OFFSET: u32 = 0x0C;
    const ST_ELEMENT_SIZE: u32 = 0x12;

    const LH_ELEMENT_HEIGHT_OFFSET: u32 = 0x00;
    const LH_ELEMENT_ASCENT_OFFSET: u32 = 0x02;
    const LH_ELEMENT_SIZE: u32 = 0x04;

    const NULL_STYLE_SCRAP_OFFSET: u32 = 0x04;
    const NULL_STYLE_REC_SIZE: u32 = 0x08;

    // Inside Macintosh Volume V, V-274: StScrpRec is a count word
    // followed immediately by ScrpSTElement entries (`scrpStyleTab EQU 2`).
    const SCRAP_N_STYLES_OFFSET: u32 = 0x00;
    const SCRAP_STYLE_TAB_OFFSET: u32 = 0x02;
    const SCRAP_STYLE_START_CHAR_OFFSET: u32 = 0x00;
    const SCRAP_STYLE_HEIGHT_OFFSET: u32 = 0x04;
    const SCRAP_STYLE_ASCENT_OFFSET: u32 = 0x06;
    const SCRAP_STYLE_FONT_OFFSET: u32 = 0x08;
    const SCRAP_STYLE_FACE_OFFSET: u32 = 0x0A;
    const SCRAP_STYLE_SIZE_OFFSET: u32 = 0x0C;
    const SCRAP_STYLE_COLOR_OFFSET: u32 = 0x0E;
    const SCRAP_STYLE_ELEMENT_SIZE: u32 = 0x14;
    const STYLE_SCRAP_REC_SIZE: u32 = Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_ELEMENT_SIZE;

    const TE_FEATURE_AUTO_SCROLL: u16 = 0;
    const TE_FEATURE_TEXT_BUFFERING: u16 = 1;
    const TE_FEATURE_OUTLINE_HILITE: u16 = 2;
    const TE_FEATURE_INLINE_INPUT: u16 = 3;
    const TE_FEATURE_USE_TEXT_SERVICES: u16 = 4;

    const TE_BIT_CLEAR: i16 = 0;
    const TE_BIT_SET: i16 = 1;
    const TE_BIT_TEST: i16 = -1;

    fn dialog_window_proc_id(&self, bus: &MacMemoryBus, dialog_ptr: u32) -> i16 {
        self.window_proc_ids
            .get(&dialog_ptr)
            .copied()
            // Older tests seed pre-side-table dialogs by writing the WDEF
            // procID at +108. Real DialogRecords use +108 as windowKind.
            .unwrap_or_else(|| bus.read_word(dialog_ptr + 108) as i16)
    }

    fn dialog_window_title(bus: &MacMemoryBus, dialog_ptr: u32) -> String {
        if dialog_ptr == 0 {
            return String::new();
        }
        // WindowRecord.titleHandle is a StringHandle at +134; GetWTitle uses
        // the same documented field (IM:I I-284).
        let title_handle = bus.read_long(dialog_ptr + 134);
        if title_handle == 0 {
            return String::new();
        }
        let title_ptr = bus.read_long(title_handle);
        if title_ptr == 0 {
            return String::new();
        }
        decode_mac_roman(&bus.read_pstring(title_ptr))
    }

    fn ensure_text_handle_size(bus: &mut MacMemoryBus, item_handle: u32, size: usize) -> u32 {
        if item_handle == 0 {
            return 0;
        }

        let current_ptr = bus.read_long(item_handle);
        if size == 0 {
            if current_ptr != 0 {
                bus.free(current_ptr);
                bus.write_long(item_handle, 0);
            }
            return 0;
        }

        let required = size as u32;
        let current_size = if current_ptr != 0 {
            bus.get_alloc_size(current_ptr).unwrap_or(0)
        } else {
            0
        };

        if current_ptr != 0 && current_size == required {
            return current_ptr;
        }

        let new_ptr = bus.alloc(required);
        if new_ptr == 0 {
            return current_ptr;
        }

        if current_ptr != 0 {
            bus.free(current_ptr);
        }
        bus.write_long(item_handle, new_ptr);
        new_ptr
    }

    fn text_item_string_from_handle(bus: &MacMemoryBus, item_handle: u32) -> String {
        Self::text_item_string_from_handle_if_present(bus, item_handle).unwrap_or_default()
    }

    fn text_item_string_from_handle_if_present(
        bus: &MacMemoryBus,
        item_handle: u32,
    ) -> Option<String> {
        Self::text_item_bytes_from_handle_if_present(bus, item_handle)
            .map(|bytes| decode_mac_roman(&bytes))
    }

    fn text_item_bytes_from_handle_if_present(
        bus: &MacMemoryBus,
        item_handle: u32,
    ) -> Option<Vec<u8>> {
        if item_handle == 0 {
            return None;
        }

        let data_ptr = bus.read_long(item_handle);
        if data_ptr == 0 {
            return Some(Vec::new());
        }

        let len = bus.get_alloc_size(data_ptr).unwrap_or(0) as usize;
        Some(bus.read_bytes(data_ptr, len))
    }

    fn dialog_item_handle_addr(bus: &MacMemoryBus, dialog_ptr: u32, item_no: i16) -> Option<u32> {
        if item_no <= 0 {
            return None;
        }

        let items_handle = bus.read_long(dialog_ptr + 156);
        if items_handle == 0 {
            return None;
        }
        let ditl_ptr = bus.read_long(items_handle);
        if ditl_ptr == 0 {
            return None;
        }

        let max_index = bus.read_word(ditl_ptr) as i16;
        if item_no > max_index + 1 {
            return None;
        }

        let ditl_len = bus.get_alloc_size(ditl_ptr).unwrap_or(u32::MAX);
        let mut offset = 2u32;
        for current_item in 1..=max_index + 1 {
            let item_handle_addr = ditl_ptr + offset;
            offset += 4; // itmhand
            offset += 8; // itmr
            let item_type = bus.read_byte(ditl_ptr + offset);
            let data_len_byte = bus.read_byte(ditl_ptr + offset + 1);
            offset += 2; // itmtype + itmlen
            let base_type = item_type & 0x7F;
            let remaining = ditl_len.saturating_sub(offset);
            let payload_len = Self::ditl_item_payload_len(base_type, data_len_byte, remaining)?;
            let padded = (payload_len + 1) & !1;
            if current_item == item_no {
                return Some(item_handle_addr);
            }
            offset += padded;
        }

        None
    }

    fn dialog_item_handle(bus: &MacMemoryBus, dialog_ptr: u32, item_no: i16) -> u32 {
        Self::dialog_item_handle_addr(bus, dialog_ptr, item_no)
            .map(|addr| bus.read_long(addr))
            .unwrap_or(0)
    }

    fn set_dialog_item_handle(bus: &mut MacMemoryBus, dialog_ptr: u32, item_no: i16, handle: u32) {
        if let Some(addr) = Self::dialog_item_handle_addr(bus, dialog_ptr, item_no) {
            bus.write_long(addr, handle);
        }
    }

    pub(crate) fn front_dialog_for_user_item_proc(&self, proc_ptr: u32) -> Option<u32> {
        if proc_ptr == 0 {
            return None;
        }
        let front = self.front_window;
        if front == 0 {
            return None;
        }
        let Some(items) = self.dialog_items.get(&front) else {
            return None;
        };
        if items
            .iter()
            .any(|item| (item.item_type & 0x7F) == 0 && item.proc_ptr == proc_ptr)
        {
            Some(front)
        } else {
            None
        }
    }

    pub(crate) fn front_dialog_has_user_item_proc(&self, proc_ptr: u32) -> bool {
        self.front_dialog_for_user_item_proc(proc_ptr).is_some()
    }

    pub(crate) fn redraw_dialog_window_contents(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
    ) {
        if let Some(mut items) = self.dialog_items.get(&dialog_ptr).cloned() {
            Self::refresh_ditl_proc_ptrs(bus, dialog_ptr, &mut items);
            let bounds = Self::dialog_screen_bounds(bus, dialog_ptr);
            let proc_id = self.dialog_window_proc_id(bus, dialog_ptr);
            let (edit_text, edit_item, default_item) =
                Self::dialog_edit_state(bus, dialog_ptr, &items);
            if self.dialogs_drawn_by_app.contains(&dialog_ptr) {
                // ShowWindow may follow a complete application composition
                // into a still-hidden dialog port. Preserve those pixels and
                // repaint only manager-owned controls instead of synthesizing
                // a fresh white shell over the application's scene.
                self.redraw_standard_dialog_items(
                    bus,
                    bounds,
                    &items,
                    default_item,
                    &edit_text,
                    edit_item,
                    dialog_ptr,
                );
            } else {
                self.draw_dialog(
                    bus,
                    bounds,
                    proc_id,
                    "",
                    &items,
                    default_item,
                    &edit_text,
                    edit_item,
                    false,
                    dialog_ptr,
                );
            }
            if Self::dialog_is_game_managed(bounds, &items) {
                self.dialog_visible_snapshots.remove(&dialog_ptr);
            } else {
                let pixels = self.save_dialog_pixels(bus, bounds);
                self.dialog_visible_snapshots
                    .insert(dialog_ptr, PersistentDialogSnapshot { bounds, pixels });
            }
            self.dialog_items.insert(dialog_ptr, items);
        }
    }

    pub(crate) fn queue_modeless_dialog_draw_procs(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
    ) {
        self.queue_modeless_dialog_draw_procs_intersecting(bus, dialog_ptr, None);
    }

    fn update_dialog_window_contents<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        dialog_ptr: u32,
        update_rect: Option<(i16, i16, i16, i16)>,
    ) -> bool {
        let Some(mut items) = self.dialog_items.get(&dialog_ptr).cloned() else {
            return false;
        };
        Self::refresh_ditl_proc_ptrs(bus, dialog_ptr, &mut items);
        let bounds = Self::dialog_screen_bounds(bus, dialog_ptr);
        let update_screen_rect =
            update_rect.and_then(|rect| Self::dialog_update_screen_rect(bounds, rect));
        let proc_id = self.dialog_window_proc_id(bus, dialog_ptr);
        let (edit_text, edit_item, default_item) = Self::dialog_edit_state(bus, dialog_ptr, &items);
        // MTE 1992 p. 6-141: DialogSelect handles update events by calling
        // DrawDialog. MTE 1992 p. 6-143: UpdateDialog uses SetPort before
        // redrawing the update region. Keep both dialog update paths on the
        // dialog port before any standard-item drawing or userItem callbacks.
        self.set_current_port_state(bus, cpu, dialog_ptr, None);
        let Some(update_screen_rect) = update_screen_rect else {
            return false;
        };
        let before_pixels = self.save_dialog_pixels(bus, bounds);
        self.dialog_initial_draw_deferred.remove(&dialog_ptr);
        self.draw_dialog(
            bus,
            bounds,
            proc_id,
            "",
            &items,
            default_item,
            &edit_text,
            edit_item,
            false,
            dialog_ptr,
        );
        // MTE 1992 p. 6-143: UpdateDialog redraws only the supplied update
        // region. Most Dialog Manager HLE drawing helpers write directly to the
        // framebuffer, so clip the full redraw by restoring pre-update pixels
        // outside the update region after standard items have been refreshed.
        self.restore_dialog_pixels_outside_rect(bus, bounds, &before_pixels, update_screen_rect);
        self.dialog_items.insert(dialog_ptr, items);
        // Application-defined userItems are guest-owned. IM:I I-405 says
        // their item handle is a draw procedure; queue only the callbacks
        // whose display rectangles intersect the active update region.
        self.queue_modeless_dialog_draw_procs_intersecting(bus, dialog_ptr, update_rect);
        if !self.modeless_dialog_cdef_draw_queue.contains(&dialog_ptr) {
            self.modeless_dialog_cdef_draw_queue.push_back(dialog_ptr);
        }
        true
    }

    fn dialog_update_screen_rect(
        bounds: (i16, i16, i16, i16),
        rect: (i16, i16, i16, i16),
    ) -> Option<(i16, i16, i16, i16)> {
        let screen_rect = if Self::rects_intersect(rect, bounds) {
            rect
        } else {
            (
                bounds.0.saturating_add(rect.0),
                bounds.1.saturating_add(rect.1),
                bounds.0.saturating_add(rect.2),
                bounds.1.saturating_add(rect.3),
            )
        };
        Self::rect_intersection(
            screen_rect,
            (
                bounds.0 - Self::DBOX_FRAME_MARGIN,
                bounds.1 - Self::DBOX_FRAME_MARGIN,
                bounds.2 + Self::DBOX_FRAME_MARGIN,
                bounds.3 + Self::DBOX_FRAME_MARGIN,
            ),
        )
    }

    fn queue_modeless_dialog_draw_procs_intersecting(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        update_rect: Option<(i16, i16, i16, i16)>,
    ) {
        let Some(mut items) = self.dialog_items.get(&dialog_ptr).cloned() else {
            return;
        };
        Self::refresh_ditl_proc_ptrs(bus, dialog_ptr, &mut items);
        let bounds = Self::dialog_screen_bounds(bus, dialog_ptr);
        let effective_update_rect = update_rect.map(|rect| {
            if Self::rects_intersect(rect, bounds) {
                rect
            } else {
                (
                    bounds.0.saturating_add(rect.0),
                    bounds.1.saturating_add(rect.1),
                    bounds.0.saturating_add(rect.2),
                    bounds.1.saturating_add(rect.3),
                )
            }
        });
        for (i, item) in items.iter().enumerate() {
            let item_rect = Self::dialog_item_screen_rect(bounds, item.rect);
            if (item.item_type & 0x7F) == 0
                && item.proc_ptr != 0
                && Self::rects_intersect(item_rect, bounds)
                && effective_update_rect
                    .map(|rect| Self::rects_intersect(item_rect, rect))
                    .unwrap_or(true)
            {
                self.modeless_dialog_draw_proc_queue.push_back((
                    dialog_ptr,
                    item.proc_ptr,
                    (i + 1) as i16,
                ));
            }
        }
        self.dialog_items.insert(dialog_ptr, items);
    }

    fn allocate_te_handle(bus: &mut MacMemoryBus) -> u32 {
        // TENew returns a handle to a zeroed TERec owned by the caller.
        // Text 1993, 2-85 to 2-86
        let te_ptr = bus.alloc(Self::TE_REC_MIN_SIZE);
        if te_ptr == 0 {
            return 0;
        }
        let handle = bus.alloc(4);
        if handle == 0 {
            return 0;
        }
        bus.write_long(handle, te_ptr);
        handle
    }

    fn allocate_handle_with_data(bus: &mut MacMemoryBus, size: u32) -> u32 {
        let handle = bus.alloc(4);
        if handle == 0 {
            return 0;
        }

        let data_ptr = if size == 0 { 0 } else { bus.alloc(size) };
        bus.write_long(handle, data_ptr);
        handle
    }

    fn ensure_handle_capacity(bus: &mut MacMemoryBus, handle: u32, min_size: u32) -> u32 {
        if handle == 0 {
            return 0;
        }

        let current_ptr = bus.read_long(handle);
        let current_size = if current_ptr != 0 {
            bus.get_alloc_size(current_ptr).unwrap_or(0)
        } else {
            0
        };
        if current_size >= min_size {
            return current_ptr;
        }

        let new_ptr = bus.alloc(min_size);
        if new_ptr == 0 {
            return current_ptr;
        }
        if current_ptr != 0 && current_size != 0 {
            let existing = bus.read_bytes(current_ptr, current_size as usize);
            bus.write_bytes(new_ptr, &existing);
            bus.free(current_ptr);
        }
        bus.write_long(handle, new_ptr);
        new_ptr
    }

    fn te_record_size_for_line_count(line_count: usize) -> u32 {
        let line_entries = line_count.saturating_add(4) as u32;
        Self::TE_REC_MIN_SIZE.max(Self::TE_LINE_STARTS_OFFSET + line_entries * 2)
    }

    fn ensure_te_record_line_capacity(
        bus: &mut MacMemoryBus,
        te_handle: u32,
        line_count: usize,
    ) -> u32 {
        Self::ensure_handle_capacity(
            bus,
            te_handle,
            Self::te_record_size_for_line_count(line_count),
        )
    }

    fn te_read_rect(bus: &MacMemoryBus, addr: u32) -> (i16, i16, i16, i16) {
        (
            bus.read_word(addr) as i16,
            bus.read_word(addr + 2) as i16,
            bus.read_word(addr + 4) as i16,
            bus.read_word(addr + 6) as i16,
        )
    }

    fn te_write_rect_words(bus: &mut MacMemoryBus, addr: u32, rect: (i16, i16, i16, i16)) {
        bus.write_word(addr, rect.0 as u16);
        bus.write_word(addr + 2, rect.1 as u16);
        bus.write_word(addr + 4, rect.2 as u16);
        bus.write_word(addr + 6, rect.3 as u16);
    }

    fn te_rect_is_empty(rect: (i16, i16, i16, i16)) -> bool {
        rect.0 == 0 && rect.1 == 0 && rect.2 == 0 && rect.3 == 0
    }

    fn te_rect_ptr_candidate(bus: &MacMemoryBus, ptr: u32) -> Option<(i16, i16, i16, i16)> {
        if ptr == 0 || (ptr & 1) != 0 || ptr < 0x1000 {
            return None;
        }
        let end = ptr.checked_add(8)?;
        if end > bus.ram_size() {
            return None;
        }

        Some(Self::te_read_rect(bus, ptr))
    }

    /// Decode TENew/TEStyleNew arguments off the Pascal stack, sniffing
    /// either the modern Universal Headers pointer convention (8 bytes,
    /// two `const Rect *` ptrs) or the legacy Inside Macintosh by-value
    /// convention (16 bytes, two Rects pushed by value).
    ///
    /// Pascal calling convention pushes left-to-right, so the first
    /// arg (destRect / destRect_ptr) lands DEEPER on the stack than the
    /// second arg (viewRect / viewRect_ptr). Concretely:
    ///   * Pointer convention (8 bytes, MPW Universal Headers):
    ///       sp+0..3  viewRect_ptr  (last pushed, shallowest)
    ///       sp+4..7  destRect_ptr  (first pushed, deepest)
    ///   * By-value convention (16 bytes, classic IM:I I-373):
    ///       sp+0..7   viewRect bytes
    ///       sp+8..15  destRect bytes
    ///
    /// Returns (destRect, viewRect, stack_pop).
    #[allow(clippy::type_complexity)] // (destRect, viewRect, stack_pop) — local return shape, no aliasing benefit
    fn te_new_rect_args(
        bus: &MacMemoryBus,
        sp: u32,
    ) -> ((i16, i16, i16, i16), (i16, i16, i16, i16), u32) {
        // Pascal first arg (destRect / destRect_ptr) is DEEPEST → sp+4.
        // Pascal second arg (viewRect / viewRect_ptr) is SHALLOWEST → sp+0.
        let view_ptr_word = bus.read_long(sp);
        let dest_ptr_word = bus.read_long(sp + 4);
        let view_via_ptr = Self::te_rect_ptr_candidate(bus, view_ptr_word);
        let dest_via_ptr = Self::te_rect_ptr_candidate(bus, dest_ptr_word);
        if let (Some(dest), Some(view)) = (dest_via_ptr, view_via_ptr) {
            // Modern MPW Universal Headers pointer convention (8 bytes).
            if Self::te_rect_is_empty(view) && !Self::te_rect_is_empty(dest) {
                (dest, dest, 8)
            } else if Self::te_rect_is_empty(dest) && !Self::te_rect_is_empty(view) {
                (view, view, 8)
            } else if Self::te_rect_is_empty(dest) && Self::te_rect_is_empty(view) {
                (
                    Self::te_read_rect(bus, sp + 8),
                    Self::te_read_rect(bus, sp),
                    16,
                )
            } else {
                (dest, view, 8)
            }
        } else {
            // Legacy by-value convention (16 bytes).
            (
                Self::te_read_rect(bus, sp + 8),
                Self::te_read_rect(bus, sp),
                16,
            )
        }
    }

    fn te_record_ptr(bus: &MacMemoryBus, te_handle: u32) -> u32 {
        if te_handle == 0 {
            0
        } else {
            bus.read_long(te_handle)
        }
    }

    fn te_is_styled_record(bus: &MacMemoryBus, te_ptr: u32) -> bool {
        te_ptr != 0 && bus.read_word(te_ptr + Self::TE_TX_SIZE_OFFSET) == 0xFFFF
    }

    fn te_text_handle(bus: &MacMemoryBus, te_handle: u32) -> u32 {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            0
        } else {
            bus.read_long(te_ptr + Self::TE_HTEXT_OFFSET)
        }
    }

    fn te_style_handle(bus: &MacMemoryBus, te_handle: u32) -> u32 {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if Self::te_is_styled_record(bus, te_ptr) {
            bus.read_long(te_ptr + Self::TE_TX_FONT_OFFSET)
        } else {
            0
        }
    }

    fn te_write_style_handle(bus: &mut MacMemoryBus, te_handle: u32, style_handle: u32) {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if Self::te_is_styled_record(bus, te_ptr) {
            bus.write_long(te_ptr + Self::TE_TX_FONT_OFFSET, style_handle);
        }
    }

    fn te_line_starts(bus: &MacMemoryBus, te_handle: u32) -> Vec<usize> {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return vec![0];
        }

        let n_lines = bus.read_word(te_ptr + Self::TE_N_LINES_OFFSET) as usize;
        let mut starts = Vec::with_capacity(n_lines.saturating_add(1));
        for index in 0..=n_lines {
            starts.push(
                bus.read_word(te_ptr + Self::TE_LINE_STARTS_OFFSET + (index as u32 * 2)) as usize,
            );
        }
        if starts.is_empty() {
            starts.push(0);
        }
        starts
    }

    fn te_char_to_line_index(bus: &MacMemoryBus, te_handle: u32, offset: usize) -> usize {
        let starts = Self::te_line_starts(bus, te_handle);
        let n_lines = starts.len().saturating_sub(1);
        if n_lines <= 1 {
            return 0;
        }

        let mut low = 0usize;
        let mut high = n_lines;
        let mut current = (high + low) / 2;
        while low < high && starts[current] != offset {
            if starts[current] < offset {
                low = current + 1;
            } else {
                high = current.saturating_sub(1);
            }
            current = (high + low) / 2;
        }

        if starts[current] > offset || current == n_lines {
            current.saturating_sub(1)
        } else {
            current
        }
    }

    fn font_lookup_size(size: i16) -> i16 {
        // QuickDraw grafPorts initialize txSize to 0, which the Font Manager
        // interprets as the system font size. Preserve that sentinel for font
        // lookup; coercing it to 1 selects the wrong strike/fallback.
        // Inside Macintosh Volume I, I-163 and I-178.
        if size == 0 {
            0
        } else {
            size.max(1)
        }
    }

    fn te_char_to_point(&self, bus: &MacMemoryBus, te_handle: u32, offset: usize) -> (i16, i16) {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return (0, 0);
        }

        let dest_rect = Self::te_read_rect(bus, te_ptr + Self::TE_DEST_RECT_OFFSET);
        let just = bus.read_word(te_ptr + Self::TE_JUST_OFFSET) as i16;
        let text_bytes = Self::te_text_bytes(bus, te_handle);
        let text_len = text_bytes.len();
        let clamped = offset.min(text_len);
        let starts = Self::te_line_starts(bus, te_handle);
        let line_index = Self::te_char_to_line_index(bus, te_handle, clamped);
        let line_start = starts.get(line_index).copied().unwrap_or(0);
        let line_end = starts.get(line_index + 1).copied().unwrap_or(text_len);
        let on_break = clamped != 0
            && clamped == text_len
            && text_bytes.get(clamped - 1).copied() == Some(b'\r');
        let styled_runs = if Self::te_is_styled_record(bus, te_ptr) {
            self.te_style_runs(bus, te_handle, text_len)
        } else {
            Vec::new()
        };
        let uses_styled_runs = !styled_runs.is_empty() && Self::te_is_styled_record(bus, te_ptr);
        let (font, _, size, _, _, _) = self.te_primary_style(bus, te_handle);
        let rect_width = dest_rect.3.saturating_sub(dest_rect.1);
        // Per Inside Macintosh: Text 1993, lines 7320-7323:
        //   teJustLeft   =  0 (flush left — system default for LTR)
        //   teJustCenter =  1 (centered)
        //   teJustRight  = -1 (flush right)
        //   teForceLeft  = -2 (force flush left)
        let left_offset = match just {
            1 | -1 => {
                let line_width = if on_break {
                    rect_width.saturating_sub(1)
                } else {
                    let line_width = if uses_styled_runs {
                        self.te_measure_text_width_styled(
                            &styled_runs,
                            &text_bytes,
                            line_start,
                            line_end,
                        )
                    } else {
                        self.te_measure_text_width(
                            font,
                            Self::font_lookup_size(size),
                            &text_bytes,
                            line_start,
                            line_end,
                        )
                    };
                    rect_width.saturating_sub(line_width).saturating_sub(1)
                };
                if just == 1 {
                    line_width / 2
                } else {
                    line_width
                }
            }
            _ => Self::TE_LINE_LEFT_INSET,
        };
        let x = if on_break {
            dest_rect.1.saturating_add(left_offset)
        } else {
            dest_rect
                .1
                .saturating_add(left_offset)
                .saturating_add(if uses_styled_runs {
                    self.te_measure_text_width_styled(
                        &styled_runs,
                        &text_bytes,
                        line_start,
                        clamped,
                    )
                } else {
                    self.te_measure_text_width(
                        font,
                        Self::font_lookup_size(size),
                        &text_bytes,
                        line_start,
                        clamped,
                    )
                })
        };

        let mut top = dest_rect.0;
        for current_line in 0..line_index {
            top = top.saturating_add(Self::te_height_for_line(bus, te_handle, current_line));
        }
        if on_break {
            top = top.saturating_add(Self::te_height_for_line(bus, te_handle, line_index));
        }

        (top, x)
    }

    /// Inverse of `te_char_to_point`: locate the character offset whose
    /// glyph cell contains `point` (vertical, horizontal). Used by
    /// TEGetOffset ($A83C) per Inside Macintosh Volume V, V-172.
    fn te_point_to_char(&self, bus: &MacMemoryBus, te_handle: u32, point: (i16, i16)) -> i16 {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return 0;
        }

        let text_bytes = Self::te_text_bytes(bus, te_handle);
        let text_len = text_bytes.len();
        let starts = Self::te_line_starts(bus, te_handle);
        let n_lines = starts.len().saturating_sub(1);
        if text_len == 0 || n_lines == 0 {
            return 0;
        }

        let dest_rect = Self::te_read_rect(bus, te_ptr + Self::TE_DEST_RECT_OFFSET);
        let (point_v, point_h) = point;

        if point_v < dest_rect.0 {
            return 0;
        }

        let mut top = dest_rect.0;
        let mut line_index = n_lines.saturating_sub(1);
        let mut found_line = false;
        for current_line in 0..n_lines {
            let height = Self::te_height_for_line(bus, te_handle, current_line);
            let bottom = top.saturating_add(height);
            if point_v < bottom {
                line_index = current_line;
                found_line = true;
                break;
            }
            top = bottom;
        }
        if !found_line {
            return text_len.min(i16::MAX as usize) as i16;
        }

        let line_start = starts[line_index];
        let line_end = starts.get(line_index + 1).copied().unwrap_or(text_len);

        let just = bus.read_word(te_ptr + Self::TE_JUST_OFFSET) as i16;
        let styled_runs = if Self::te_is_styled_record(bus, te_ptr) {
            self.te_style_runs(bus, te_handle, text_len)
        } else {
            Vec::new()
        };
        let uses_styled_runs = !styled_runs.is_empty() && Self::te_is_styled_record(bus, te_ptr);
        let (font, _, size, _, _, _) = self.te_primary_style(bus, te_handle);
        let rect_width = dest_rect.3.saturating_sub(dest_rect.1);
        let left_offset = match just {
            1 | -1 => {
                let measured_width = if uses_styled_runs {
                    self.te_measure_text_width_styled(
                        &styled_runs,
                        &text_bytes,
                        line_start,
                        line_end,
                    )
                } else {
                    self.te_measure_text_width(
                        font,
                        Self::font_lookup_size(size),
                        &text_bytes,
                        line_start,
                        line_end,
                    )
                };
                let line_width = rect_width.saturating_sub(measured_width).saturating_sub(1);
                if just == 1 {
                    line_width / 2
                } else {
                    line_width
                }
            }
            _ => Self::TE_LINE_LEFT_INSET,
        };

        let mut x_cursor = dest_rect.1.saturating_add(left_offset);
        for (offset, byte) in text_bytes[line_start..line_end].iter().enumerate() {
            let index = line_start + offset;
            if matches!(*byte, b'\r' | b'\n') {
                return index.min(i16::MAX as usize) as i16;
            }
            let advance = if uses_styled_runs {
                let style = Self::te_style_at_offset(&styled_runs, index);
                Self::te_styled_char_width(style, *byte)
            } else {
                self.te_char_width(font, Self::font_lookup_size(size), *byte)
            };
            // Hit-test against the glyph midpoint so a click on the right
            // half of a character lands on the next offset, matching the
            // semantics described in Inside Macintosh Volume V, V-172.
            let mid = x_cursor.saturating_add(advance / 2);
            if point_h < mid {
                return index.min(i16::MAX as usize) as i16;
            }
            x_cursor = x_cursor.saturating_add(advance);
        }
        line_end.min(i16::MAX as usize) as i16
    }

    fn te_scroll_contents(
        &mut self,
        cpu: &mut impl CpuOps,
        bus: &mut MacMemoryBus,
        te_handle: u32,
        dh: i16,
        dv: i16,
    ) {
        if dh == 0 && dv == 0 {
            return;
        }

        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return;
        }

        let mut dest_rect = Self::te_read_rect(bus, te_ptr + Self::TE_DEST_RECT_OFFSET);
        dest_rect.0 = dest_rect.0.saturating_add(dv);
        dest_rect.1 = dest_rect.1.saturating_add(dh);
        dest_rect.2 = dest_rect.2.saturating_add(dv);
        dest_rect.3 = dest_rect.3.saturating_add(dh);
        Self::te_write_rect_words(bus, te_ptr + Self::TE_DEST_RECT_OFFSET, dest_rect);
        self.draw_te_contents(cpu, bus, te_handle, true);
        // Refresh rendered_pixels so redraw_chrome restores the scrolled state rather
        // than the pre-scroll snapshot captured at dialog creation.
        // Text 1993, 2-89 (TEScroll/TEPinScroll modify destRect and redraw).
        if let Some(ref tracking) = self.dialog_tracking {
            if !tracking.game_managed && tracking.rendered_pixels_final {
                let bounds = tracking.bounds;
                let new_pixels = self.save_dialog_pixels(bus, bounds);
                self.dialog_tracking.as_mut().unwrap().rendered_pixels = new_pixels;
            }
        }
    }

    fn te_getdelta(sel_start: i16, sel_stop: i16, view_start: i16, view_stop: i16) -> i16 {
        if sel_start < view_start {
            view_start.saturating_sub(sel_start)
        } else if sel_stop > view_stop {
            if sel_stop.saturating_sub(sel_start) > view_stop.saturating_sub(view_start) {
                view_start.saturating_sub(sel_start)
            } else {
                view_stop.saturating_sub(sel_stop)
            }
        } else {
            0
        }
    }

    fn te_height_for_line(bus: &MacMemoryBus, te_handle: u32, line_index: usize) -> i16 {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return 0;
        }

        if Self::te_is_styled_record(bus, te_ptr) {
            let style_handle = Self::te_style_handle(bus, te_handle);
            if style_handle != 0 {
                let style_ptr = bus.read_long(style_handle);
                if style_ptr != 0 {
                    let lh_handle = bus.read_long(style_ptr + Self::TE_STYLE_LH_TABLE_OFFSET);
                    let lh_ptr = if lh_handle != 0 {
                        bus.read_long(lh_handle)
                    } else {
                        0
                    };
                    if lh_ptr != 0 {
                        return bus.read_word(
                            lh_ptr
                                + (line_index as u32 * Self::LH_ELEMENT_SIZE)
                                + Self::LH_ELEMENT_HEIGHT_OFFSET,
                        ) as i16;
                    }
                }
            }
        }

        bus.read_word(te_ptr + Self::TE_LINE_HEIGHT_OFFSET) as i16
    }

    fn te_ascent_for_line(bus: &MacMemoryBus, te_handle: u32, line_index: usize) -> i16 {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return 0;
        }

        if Self::te_is_styled_record(bus, te_ptr) {
            let style_handle = Self::te_style_handle(bus, te_handle);
            if style_handle != 0 {
                let style_ptr = bus.read_long(style_handle);
                if style_ptr != 0 {
                    let lh_handle = bus.read_long(style_ptr + Self::TE_STYLE_LH_TABLE_OFFSET);
                    let lh_ptr = if lh_handle != 0 {
                        bus.read_long(lh_handle)
                    } else {
                        0
                    };
                    if lh_ptr != 0 {
                        return bus.read_word(
                            lh_ptr
                                + (line_index as u32 * Self::LH_ELEMENT_SIZE)
                                + Self::LH_ELEMENT_ASCENT_OFFSET,
                        ) as i16;
                    }
                }
            }
        }

        bus.read_word(te_ptr + Self::TE_FONT_ASCENT_OFFSET) as i16
    }

    fn te_primary_style(
        &self,
        bus: &MacMemoryBus,
        te_handle: u32,
    ) -> (i16, i16, i16, (u16, u16, u16), i16, i16) {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            let size = Self::font_lookup_size(self.tx_size);
            let metrics = get_font_metrics(self.tx_font, size);
            return (
                self.tx_font,
                self.tx_face,
                size,
                self.fg_color,
                metrics.ascent + metrics.descent + metrics.leading,
                metrics.ascent,
            );
        }

        if Self::te_is_styled_record(bus, te_ptr) {
            let style_handle = bus.read_long(te_ptr + Self::TE_TX_FONT_OFFSET);
            let style_ptr = if style_handle != 0 {
                bus.read_long(style_handle)
            } else {
                0
            };
            if style_ptr != 0 {
                let style_table_handle =
                    bus.read_long(style_ptr + Self::TE_STYLE_STYLE_TABLE_OFFSET);
                let style_table_ptr = if style_table_handle != 0 {
                    bus.read_long(style_table_handle)
                } else {
                    0
                };
                if style_table_ptr != 0 {
                    return (
                        bus.read_word(style_table_ptr + Self::ST_ELEMENT_FONT_OFFSET) as i16,
                        i16::from(bus.read_byte(style_table_ptr + Self::ST_ELEMENT_FACE_OFFSET)),
                        bus.read_word(style_table_ptr + Self::ST_ELEMENT_SIZE_OFFSET) as i16,
                        (
                            bus.read_word(style_table_ptr + Self::ST_ELEMENT_COLOR_OFFSET),
                            bus.read_word(style_table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 2),
                            bus.read_word(style_table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 4),
                        ),
                        bus.read_word(style_table_ptr + Self::ST_ELEMENT_HEIGHT_OFFSET) as i16,
                        bus.read_word(style_table_ptr + Self::ST_ELEMENT_ASCENT_OFFSET) as i16,
                    );
                }
            }
        }

        let tx_font = bus.read_word(te_ptr + Self::TE_TX_FONT_OFFSET) as i16;
        let tx_face = i16::from(bus.read_byte(te_ptr + Self::TE_TX_FACE_OFFSET));
        let tx_size =
            Self::font_lookup_size(bus.read_word(te_ptr + Self::TE_TX_SIZE_OFFSET) as i16);
        let metrics = get_font_metrics(tx_font, tx_size);
        (
            tx_font,
            tx_face,
            tx_size,
            self.fg_color,
            metrics.ascent + metrics.descent + metrics.leading,
            metrics.ascent,
        )
    }

    fn te_resolved_style_from_parts(
        font: i16,
        face: i16,
        size: i16,
        color: (u16, u16, u16),
        line_height: i16,
        ascent: i16,
    ) -> TeResolvedStyle {
        let resolved_size = Self::font_lookup_size(size);
        let metrics = get_font_metrics(font, resolved_size);
        let fallback_height = metrics.ascent + metrics.descent + metrics.leading;
        TeResolvedStyle {
            font,
            face,
            size: resolved_size,
            color,
            line_height: if line_height > 0 {
                line_height
            } else {
                fallback_height
            },
            ascent: if ascent > 0 { ascent } else { metrics.ascent },
        }
    }

    fn te_primary_resolved_style(&self, bus: &MacMemoryBus, te_handle: u32) -> TeResolvedStyle {
        let (font, face, size, color, line_height, ascent) = self.te_primary_style(bus, te_handle);
        Self::te_resolved_style_from_parts(font, face, size, color, line_height, ascent)
    }

    fn te_style_from_table_element(bus: &MacMemoryBus, style_ptr: u32) -> TeResolvedStyle {
        Self::te_resolved_style_from_parts(
            bus.read_word(style_ptr + Self::ST_ELEMENT_FONT_OFFSET) as i16,
            i16::from(bus.read_byte(style_ptr + Self::ST_ELEMENT_FACE_OFFSET)),
            bus.read_word(style_ptr + Self::ST_ELEMENT_SIZE_OFFSET) as i16,
            (
                bus.read_word(style_ptr + Self::ST_ELEMENT_COLOR_OFFSET),
                bus.read_word(style_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 2),
                bus.read_word(style_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 4),
            ),
            bus.read_word(style_ptr + Self::ST_ELEMENT_HEIGHT_OFFSET) as i16,
            bus.read_word(style_ptr + Self::ST_ELEMENT_ASCENT_OFFSET) as i16,
        )
    }

    fn te_style_from_scrap_element(bus: &MacMemoryBus, scrap_style_ptr: u32) -> TeResolvedStyle {
        Self::te_resolved_style_from_parts(
            bus.read_word(scrap_style_ptr + Self::SCRAP_STYLE_FONT_OFFSET) as i16,
            i16::from(bus.read_byte(scrap_style_ptr + Self::SCRAP_STYLE_FACE_OFFSET)),
            bus.read_word(scrap_style_ptr + Self::SCRAP_STYLE_SIZE_OFFSET) as i16,
            (
                bus.read_word(scrap_style_ptr + Self::SCRAP_STYLE_COLOR_OFFSET),
                bus.read_word(scrap_style_ptr + Self::SCRAP_STYLE_COLOR_OFFSET + 2),
                bus.read_word(scrap_style_ptr + Self::SCRAP_STYLE_COLOR_OFFSET + 4),
            ),
            bus.read_word(scrap_style_ptr + Self::SCRAP_STYLE_HEIGHT_OFFSET) as i16,
            bus.read_word(scrap_style_ptr + Self::SCRAP_STYLE_ASCENT_OFFSET) as i16,
        )
    }

    fn te_write_style_table_element(
        bus: &mut MacMemoryBus,
        style_ptr: u32,
        style: TeResolvedStyle,
    ) {
        bus.write_word(style_ptr + Self::ST_ELEMENT_REFCOUNT_OFFSET, 1);
        bus.write_word(
            style_ptr + Self::ST_ELEMENT_HEIGHT_OFFSET,
            style.line_height as u16,
        );
        bus.write_word(
            style_ptr + Self::ST_ELEMENT_ASCENT_OFFSET,
            style.ascent as u16,
        );
        bus.write_word(style_ptr + Self::ST_ELEMENT_FONT_OFFSET, style.font as u16);
        bus.write_byte(style_ptr + Self::ST_ELEMENT_FACE_OFFSET, style.face as u8);
        bus.write_word(style_ptr + Self::ST_ELEMENT_SIZE_OFFSET, style.size as u16);
        bus.write_word(style_ptr + Self::ST_ELEMENT_COLOR_OFFSET, style.color.0);
        bus.write_word(style_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 2, style.color.1);
        bus.write_word(style_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 4, style.color.2);
    }

    fn te_set_null_style(
        bus: &mut MacMemoryBus,
        te_handle: u32,
        mode: u16,
        text_style_ptr: u32,
    ) -> bool {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if !Self::te_is_styled_record(bus, te_ptr) || text_style_ptr == 0 {
            return false;
        }
        let style_handle = Self::te_style_handle(bus, te_handle);
        let style_ptr = if style_handle != 0 {
            bus.read_long(style_handle)
        } else {
            0
        };
        let null_style_handle = if style_ptr != 0 {
            bus.read_long(style_ptr + Self::TE_STYLE_NULL_STYLE_OFFSET)
        } else {
            0
        };
        let null_style_ptr = if null_style_handle != 0 {
            bus.read_long(null_style_handle)
        } else {
            0
        };
        let null_scrap_handle = if null_style_ptr != 0 {
            bus.read_long(null_style_ptr + Self::NULL_STYLE_SCRAP_OFFSET)
        } else {
            0
        };
        let scrap_ptr = if null_scrap_handle != 0 {
            bus.read_long(null_scrap_handle)
        } else {
            0
        };
        if scrap_ptr == 0 {
            return false;
        }

        let element = scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET;
        bus.write_word(scrap_ptr + Self::SCRAP_N_STYLES_OFFSET, 1);
        if (mode & 0x0001) != 0 {
            bus.write_word(
                element + Self::SCRAP_STYLE_FONT_OFFSET,
                bus.read_word(text_style_ptr),
            );
        }
        if (mode & 0x0002) != 0 {
            bus.write_byte(
                element + Self::SCRAP_STYLE_FACE_OFFSET,
                bus.read_byte(text_style_ptr + 2),
            );
        }
        if (mode & 0x0004) != 0 {
            bus.write_word(
                element + Self::SCRAP_STYLE_SIZE_OFFSET,
                bus.read_word(text_style_ptr + 4),
            );
        }
        if (mode & 0x0008) != 0 {
            for offset in [0u32, 2, 4] {
                bus.write_word(
                    element + Self::SCRAP_STYLE_COLOR_OFFSET + offset,
                    bus.read_word(text_style_ptr + 6 + offset),
                );
            }
        }
        let font = bus.read_word(element + Self::SCRAP_STYLE_FONT_OFFSET) as i16;
        let size =
            Self::font_lookup_size(bus.read_word(element + Self::SCRAP_STYLE_SIZE_OFFSET) as i16);
        let metrics = get_font_metrics(font, size);
        bus.write_word(
            element + Self::SCRAP_STYLE_HEIGHT_OFFSET,
            (metrics.ascent + metrics.descent + metrics.leading) as u16,
        );
        bus.write_word(
            element + Self::SCRAP_STYLE_ASCENT_OFFSET,
            metrics.ascent as u16,
        );
        true
    }

    fn te_null_style_scrap_handle(bus: &MacMemoryBus, te_handle: u32) -> u32 {
        let style_handle = Self::te_style_handle(bus, te_handle);
        let style_ptr = if style_handle != 0 {
            bus.read_long(style_handle)
        } else {
            0
        };
        let null_style_handle = if style_ptr != 0 {
            bus.read_long(style_ptr + Self::TE_STYLE_NULL_STYLE_OFFSET)
        } else {
            0
        };
        let null_style_ptr = if null_style_handle != 0 {
            bus.read_long(null_style_handle)
        } else {
            0
        };
        if null_style_ptr != 0 {
            bus.read_long(null_style_ptr + Self::NULL_STYLE_SCRAP_OFFSET)
        } else {
            0
        }
    }

    fn te_null_style_resolved_style(
        bus: &MacMemoryBus,
        te_handle: u32,
    ) -> Option<TeResolvedStyle> {
        let scrap_handle = Self::te_null_style_scrap_handle(bus, te_handle);
        let scrap_ptr = bus.read_long(scrap_handle);
        if scrap_ptr == 0 || bus.read_word(scrap_ptr + Self::SCRAP_N_STYLES_OFFSET) == 0 {
            return None;
        }
        Some(Self::te_style_from_scrap_element(
            bus,
            scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET,
        ))
    }

    fn te_style_runs(
        &self,
        bus: &MacMemoryBus,
        te_handle: u32,
        text_len: usize,
    ) -> Vec<TeStyleRun> {
        let fallback = self.te_primary_resolved_style(bus, te_handle);
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if !Self::te_is_styled_record(bus, te_ptr) {
            return vec![TeStyleRun {
                start: 0,
                style_index: 0,
                style: fallback,
            }];
        }

        let style_handle = Self::te_style_handle(bus, te_handle);
        let style_ptr = if style_handle != 0 {
            bus.read_long(style_handle)
        } else {
            0
        };
        if style_ptr == 0 {
            return vec![TeStyleRun {
                start: 0,
                style_index: 0,
                style: fallback,
            }];
        }

        let style_table_handle = bus.read_long(style_ptr + Self::TE_STYLE_STYLE_TABLE_OFFSET);
        let style_table_ptr = if style_table_handle != 0 {
            bus.read_long(style_table_handle)
        } else {
            0
        };
        if style_table_ptr == 0 {
            return vec![TeStyleRun {
                start: 0,
                style_index: 0,
                style: fallback,
            }];
        }

        let n_runs = bus.read_word(style_ptr + Self::TE_STYLE_N_RUNS_OFFSET) as usize;
        let n_styles = bus.read_word(style_ptr + Self::TE_STYLE_N_STYLES_OFFSET) as usize;
        let style_record_size = bus.get_alloc_size(style_ptr).unwrap_or(0);
        let style_table_size = bus.get_alloc_size(style_table_ptr).unwrap_or(0);
        let max_runs = style_record_size
            .saturating_sub(Self::TE_STYLE_RUNS_OFFSET)
            .checked_div(4)
            .unwrap_or(0) as usize;
        let max_styles = style_table_size
            .checked_div(Self::ST_ELEMENT_SIZE)
            .unwrap_or(0) as usize;
        let run_count = n_runs.min(max_runs);
        let style_count = n_styles.min(max_styles);
        if run_count == 0 || style_count == 0 {
            return vec![TeStyleRun {
                start: 0,
                style_index: 0,
                style: fallback,
            }];
        }

        let mut runs = Vec::with_capacity(run_count);
        for run_index in 0..run_count {
            let run_ptr = style_ptr + Self::TE_STYLE_RUNS_OFFSET + (run_index as u32 * 4);
            let style_index_word = bus.read_word(run_ptr + 2);
            if style_index_word == 0xFFFF {
                break;
            }
            let style_index = style_index_word as usize;
            if style_index >= style_count {
                continue;
            }
            let style_element_ptr = style_table_ptr + (style_index as u32 * Self::ST_ELEMENT_SIZE);
            runs.push(TeStyleRun {
                start: (bus.read_word(run_ptr) as usize).min(text_len),
                style_index,
                style: Self::te_style_from_table_element(bus, style_element_ptr),
            });
        }

        if runs.is_empty() {
            return vec![TeStyleRun {
                start: 0,
                style_index: 0,
                style: fallback,
            }];
        }

        runs.sort_by_key(|run| run.start);
        let mut folded: Vec<TeStyleRun> = Vec::with_capacity(runs.len() + 1);
        for run in runs {
            if let Some(last) = folded.last_mut() {
                if last.start == run.start {
                    *last = run;
                    continue;
                }
                if last.style == run.style {
                    continue;
                }
            }
            folded.push(run);
        }

        if folded.first().is_none_or(|run| run.start != 0) {
            folded.insert(
                0,
                TeStyleRun {
                    start: 0,
                    style_index: 0,
                    style: fallback,
                },
            );
        }
        folded
    }

    fn te_style_at_offset(runs: &[TeStyleRun], offset: usize) -> TeResolvedStyle {
        let mut style = runs
            .first()
            .map(|run| run.style)
            .unwrap_or_else(|| Self::te_resolved_style_from_parts(0, 0, 0, (0, 0, 0), 0, 0));
        for run in runs {
            if run.start > offset {
                break;
            }
            style = run.style;
        }
        style
    }

    fn te_measure_text_width_styled(
        &self,
        runs: &[TeStyleRun],
        text_bytes: &[u8],
        start: usize,
        end: usize,
    ) -> i16 {
        let start = start.min(text_bytes.len());
        let end = end.min(text_bytes.len());
        let mut width = 0i16;
        for (offset, &byte) in text_bytes[start..end].iter().enumerate() {
            let style = Self::te_style_at_offset(runs, start + offset);
            width = width.saturating_add(Self::te_styled_char_width(style, byte));
        }
        width
    }

    fn te_wrap_lines_styled(
        &self,
        runs: &[TeStyleRun],
        text_bytes: &[u8],
        box_width: i16,
    ) -> Vec<(usize, usize)> {
        crate::quickdraw::text::wrap_classic_text(text_bytes, box_width, |index, byte| {
            let style = Self::te_style_at_offset(runs, index);
            Self::te_styled_char_width(style, byte)
        })
        .into_iter()
        .map(|line| (line.start, line.next))
        .collect()
    }

    fn te_line_metrics_for_range(runs: &[TeStyleRun], start: usize, end: usize) -> (i16, i16) {
        if runs.is_empty() {
            return (0, 0);
        }

        let mut line_height = 0i16;
        let mut ascent = 0i16;
        if start >= end {
            let style = Self::te_style_at_offset(runs, start);
            return (style.line_height, style.ascent);
        }

        for offset in start..end {
            let style = Self::te_style_at_offset(runs, offset);
            line_height = line_height.max(style.line_height);
            ascent = ascent.max(style.ascent);
        }
        (line_height.max(1), ascent.max(0))
    }

    fn te_update_styled_run_sentinel(bus: &mut MacMemoryBus, te_handle: u32, text_len: usize) {
        let style_handle = Self::te_style_handle(bus, te_handle);
        if style_handle == 0 {
            return;
        }
        let style_ptr = bus.read_long(style_handle);
        if style_ptr == 0 {
            return;
        }
        let n_runs = bus.read_word(style_ptr + Self::TE_STYLE_N_RUNS_OFFSET) as usize;
        let needed = Self::TE_STYLE_RUNS_OFFSET + ((n_runs as u32 + 1) * 4);
        let style_ptr = Self::ensure_handle_capacity(bus, style_handle, needed);
        if style_ptr == 0 {
            return;
        }
        let sentinel = style_ptr + Self::TE_STYLE_RUNS_OFFSET + (n_runs as u32 * 4);
        bus.write_word(
            sentinel,
            text_len.saturating_add(1).min(u16::MAX as usize) as u16,
        );
        bus.write_word(sentinel + 2, 0xFFFF);
    }

    fn te_apply_style_scrap_to_range(
        &mut self,
        bus: &mut MacMemoryBus,
        te_handle: u32,
        style_scrap: u32,
        range_start: usize,
        range_len: usize,
    ) -> bool {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if !Self::te_is_styled_record(bus, te_ptr) || style_scrap == 0 || range_len == 0 {
            return false;
        }

        let text_len = Self::te_text_length(bus, te_handle);
        let range_start = range_start.min(text_len);
        let range_end = range_start.saturating_add(range_len).min(text_len);
        if range_start >= range_end {
            return false;
        }

        let scrap_ptr = bus.read_long(style_scrap);
        if scrap_ptr == 0 {
            return false;
        }
        let scrap_size = bus.get_alloc_size(scrap_ptr).unwrap_or(0);
        if scrap_size < Self::SCRAP_STYLE_TAB_OFFSET {
            return false;
        }
        let style_count = (bus.read_word(scrap_ptr + Self::SCRAP_N_STYLES_OFFSET) as usize).min(
            scrap_size
                .saturating_sub(Self::SCRAP_STYLE_TAB_OFFSET)
                .checked_div(Self::SCRAP_STYLE_ELEMENT_SIZE)
                .unwrap_or(0) as usize,
        );
        if style_count == 0 {
            return false;
        }
        let existing_runs = self.te_style_runs(bus, te_handle, text_len);
        let mut candidates: Vec<(usize, u8, TeResolvedStyle)> =
            Vec::with_capacity(existing_runs.len() + style_count + 2);
        for run in &existing_runs {
            if run.start < range_start || run.start > range_end {
                candidates.push((run.start.min(text_len), 0, run.style));
            }
        }

        for style_index in 0..style_count {
            let scrap_style_ptr = scrap_ptr
                + Self::SCRAP_STYLE_TAB_OFFSET
                + (style_index as u32 * Self::SCRAP_STYLE_ELEMENT_SIZE);
            let local_start =
                bus.read_long(scrap_style_ptr + Self::SCRAP_STYLE_START_CHAR_OFFSET) as usize;
            let run_start = range_start.saturating_add(local_start.min(range_end - range_start));
            candidates.push((
                run_start,
                1,
                Self::te_style_from_scrap_element(bus, scrap_style_ptr),
            ));
        }

        if range_end < text_len {
            candidates.push((
                range_end,
                2,
                Self::te_style_at_offset(&existing_runs, range_end),
            ));
        }
        if !candidates.iter().any(|(start, _, _)| *start == 0) {
            candidates.push((0, 0, Self::te_style_at_offset(&existing_runs, 0)));
        }

        candidates.sort_by_key(|(start, order, _)| (*start, *order));
        let mut merged: Vec<(usize, TeResolvedStyle)> = Vec::with_capacity(candidates.len());
        for (start, _, style) in candidates {
            let start = start.min(text_len);
            if let Some((last_start, last_style)) = merged.last_mut() {
                if *last_start == start {
                    *last_style = style;
                    continue;
                }
                if *last_style == style {
                    continue;
                }
            }
            merged.push((start, style));
        }

        if merged.is_empty() {
            merged.push((0, self.te_primary_resolved_style(bus, te_handle)));
        }
        if merged[0].0 != 0 {
            merged.insert(0, (0, Self::te_style_at_offset(&existing_runs, 0)));
        }

        let run_count = merged.len().min(u16::MAX as usize);
        let style_handle = Self::te_style_handle(bus, te_handle);
        if style_handle == 0 {
            return false;
        }
        let style_ptr = Self::ensure_handle_capacity(
            bus,
            style_handle,
            Self::TE_STYLE_RUNS_OFFSET + ((run_count as u32 + 1) * 4),
        );
        if style_ptr == 0 {
            return false;
        }

        let mut style_table_handle = bus.read_long(style_ptr + Self::TE_STYLE_STYLE_TABLE_OFFSET);
        if style_table_handle == 0 {
            style_table_handle = Self::allocate_handle_with_data(bus, 0);
            bus.write_long(
                style_ptr + Self::TE_STYLE_STYLE_TABLE_OFFSET,
                style_table_handle,
            );
        }
        let style_table_ptr = Self::ensure_handle_capacity(
            bus,
            style_table_handle,
            (run_count as u32) * Self::ST_ELEMENT_SIZE,
        );
        if style_table_ptr == 0 {
            return false;
        }

        bus.write_word(style_ptr + Self::TE_STYLE_N_RUNS_OFFSET, run_count as u16);
        bus.write_word(style_ptr + Self::TE_STYLE_N_STYLES_OFFSET, run_count as u16);
        for (index, (start, style)) in merged.iter().take(run_count).enumerate() {
            let style_element_ptr = style_table_ptr + (index as u32 * Self::ST_ELEMENT_SIZE);
            Self::te_write_style_table_element(bus, style_element_ptr, *style);
            let run_ptr = style_ptr + Self::TE_STYLE_RUNS_OFFSET + (index as u32 * 4);
            bus.write_word(run_ptr, (*start).min(u16::MAX as usize) as u16);
            bus.write_word(run_ptr + 2, index.min(u16::MAX as usize) as u16);
        }
        Self::te_update_styled_run_sentinel(bus, te_handle, text_len);
        true
    }

    fn te_set_style_for_range(
        &mut self,
        bus: &mut MacMemoryBus,
        te_handle: u32,
        range_start: usize,
        range_end: usize,
        mode: u16,
        text_style_ptr: u32,
    ) -> bool {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if !Self::te_is_styled_record(bus, te_ptr) || text_style_ptr == 0 {
            return false;
        }
        let text_len = Self::te_text_length(bus, te_handle);
        let range_start = range_start.min(text_len);
        let range_end = range_end.min(text_len);
        if range_start >= range_end {
            return false;
        }

        let existing_runs = self.te_style_runs(bus, te_handle, text_len);
        let old_range_end_style = Self::te_style_at_offset(&existing_runs, range_end);
        let requested_face = bus.read_byte(text_style_ptr + 2) as i16;
        let toggle_face = mode & 0x0020 != 0 && mode & 0x0002 != 0;
        let remove_toggled_face = toggle_face
            && existing_runs.iter().enumerate().all(|(index, run)| {
                let run_end = existing_runs
                    .get(index + 1)
                    .map(|next| next.start)
                    .unwrap_or(text_len);
                run_end <= range_start
                    || run.start >= range_end
                    || run.style.face & requested_face == requested_face
            });

        let transform = |mut style: TeResolvedStyle| {
            if mode & 0x0001 != 0 {
                style.font = bus.read_word(text_style_ptr) as i16;
            }
            if mode & 0x0002 != 0 {
                style.face = if toggle_face {
                    if remove_toggled_face {
                        style.face & !requested_face
                    } else {
                        style.face | requested_face
                    }
                } else {
                    requested_face
                };
            }
            if mode & 0x0010 != 0 {
                style.size = style
                    .size
                    .saturating_add(bus.read_word(text_style_ptr + 4) as i16)
                    .max(1);
            } else if mode & 0x0004 != 0 {
                style.size = bus.read_word(text_style_ptr + 4) as i16;
            }
            if mode & 0x0008 != 0 {
                style.color = (
                    bus.read_word(text_style_ptr + 6),
                    bus.read_word(text_style_ptr + 8),
                    bus.read_word(text_style_ptr + 10),
                );
            }
            let metrics = get_font_metrics(style.font, Self::font_lookup_size(style.size));
            style.line_height = metrics.ascent + metrics.descent + metrics.leading;
            style.ascent = metrics.ascent;
            style
        };

        let mut merged: Vec<(usize, TeResolvedStyle)> = Vec::with_capacity(existing_runs.len() + 2);
        for run in &existing_runs {
            if run.start < range_start || run.start >= range_end {
                merged.push((run.start, run.style));
            } else {
                merged.push((run.start, transform(run.style)));
            }
        }
        if !existing_runs.iter().any(|run| run.start == range_start) {
            merged.push((
                range_start,
                transform(Self::te_style_at_offset(&existing_runs, range_start)),
            ));
        }
        if range_end < text_len && !existing_runs.iter().any(|run| run.start == range_end) {
            merged.push((range_end, old_range_end_style));
        }
        merged.sort_by_key(|(start, _)| *start);
        let mut folded = Vec::with_capacity(merged.len());
        for (start, style) in merged {
            if let Some((last_start, last_style)) = folded.last_mut() {
                if *last_start == start {
                    *last_style = style;
                    continue;
                }
                if *last_style == style {
                    continue;
                }
            }
            folded.push((start, style));
        }
        let merged = folded;

        let run_count = merged.len().min(u16::MAX as usize);
        let style_handle = Self::te_style_handle(bus, te_handle);
        if style_handle == 0 {
            return false;
        }
        let style_ptr = Self::ensure_handle_capacity(
            bus,
            style_handle,
            Self::TE_STYLE_RUNS_OFFSET + ((run_count as u32 + 1) * 4),
        );
        if style_ptr == 0 {
            return false;
        }
        let style_table_handle = bus.read_long(style_ptr + Self::TE_STYLE_STYLE_TABLE_OFFSET);
        let style_table_ptr = Self::ensure_handle_capacity(
            bus,
            style_table_handle,
            (run_count as u32) * Self::ST_ELEMENT_SIZE,
        );
        if style_table_ptr == 0 {
            return false;
        }
        bus.write_word(style_ptr + Self::TE_STYLE_N_RUNS_OFFSET, run_count as u16);
        bus.write_word(style_ptr + Self::TE_STYLE_N_STYLES_OFFSET, run_count as u16);
        for (index, (start, style)) in merged.iter().take(run_count).enumerate() {
            Self::te_write_style_table_element(
                bus,
                style_table_ptr + (index as u32 * Self::ST_ELEMENT_SIZE),
                *style,
            );
            let run_ptr = style_ptr + Self::TE_STYLE_RUNS_OFFSET + (index as u32 * 4);
            bus.write_word(run_ptr, (*start).min(u16::MAX as usize) as u16);
            bus.write_word(run_ptr + 2, index as u16);
        }
        Self::te_update_styled_run_sentinel(bus, te_handle, text_len);
        true
    }

    #[allow(dead_code)]
    fn te_line_metrics(&self, bus: &MacMemoryBus, te_handle: u32) -> (i16, i16) {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            let metrics = get_font_metrics(self.tx_font, Self::font_lookup_size(self.tx_size));
            return (
                metrics.ascent + metrics.descent + metrics.leading,
                metrics.ascent,
            );
        }

        let line_height = bus.read_word(te_ptr + Self::TE_LINE_HEIGHT_OFFSET) as i16;
        let font_ascent = bus.read_word(te_ptr + Self::TE_FONT_ASCENT_OFFSET) as i16;
        if line_height > 0 && font_ascent >= 0 {
            return (line_height, font_ascent);
        }

        let (_, _, _, _, resolved_line_height, resolved_ascent) =
            self.te_primary_style(bus, te_handle);
        (resolved_line_height, resolved_ascent)
    }

    fn initialize_te_record(
        &mut self,
        bus: &mut MacMemoryBus,
        te_handle: u32,
        dest_rect: (i16, i16, i16, i16),
        view_rect: (i16, i16, i16, i16),
    ) {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return;
        }

        let metrics = get_font_metrics(self.tx_font, Self::font_lookup_size(self.tx_size));
        let line_height = metrics.ascent + metrics.descent + metrics.leading;
        let h_text = bus.alloc(4);
        if h_text != 0 {
            bus.write_long(h_text, 0);
        }

        Self::te_write_rect_words(bus, te_ptr + Self::TE_DEST_RECT_OFFSET, dest_rect);
        Self::te_write_rect_words(bus, te_ptr + Self::TE_VIEW_RECT_OFFSET, view_rect);
        Self::te_write_rect_words(bus, te_ptr + Self::TE_SEL_RECT_OFFSET, dest_rect);
        bus.write_word(te_ptr + Self::TE_LINE_HEIGHT_OFFSET, line_height as u16);
        bus.write_word(te_ptr + Self::TE_FONT_ASCENT_OFFSET, metrics.ascent as u16);
        bus.write_word(te_ptr + Self::TE_SEL_POINT_OFFSET, dest_rect.0 as u16);
        bus.write_word(te_ptr + Self::TE_SEL_POINT_OFFSET + 2, dest_rect.1 as u16);
        bus.write_word(te_ptr + Self::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + Self::TE_SEL_END_OFFSET, 0);
        bus.write_long(te_ptr + Self::TE_CARET_TIME_OFFSET, self.current_tick());
        bus.write_word(te_ptr + Self::TE_CARET_STATE_OFFSET, 0);
        bus.write_word(te_ptr + Self::TE_LENGTH_OFFSET, 0);
        bus.write_long(te_ptr + Self::TE_HTEXT_OFFSET, h_text);
        bus.write_word(te_ptr + Self::TE_TX_FONT_OFFSET, self.tx_font as u16);
        bus.write_byte(te_ptr + Self::TE_TX_FACE_OFFSET, self.tx_face as u8);
        bus.write_word(te_ptr + Self::TE_TX_MODE_OFFSET, self.tx_mode as u16);
        bus.write_word(te_ptr + Self::TE_TX_SIZE_OFFSET, self.tx_size as u16);
        bus.write_long(te_ptr + Self::TE_IN_PORT_OFFSET, *self.current_port);
        self.textedit_states.remove(&te_handle);
    }

    fn initialize_styled_te_record(
        &mut self,
        bus: &mut MacMemoryBus,
        te_handle: u32,
        dest_rect: (i16, i16, i16, i16),
        view_rect: (i16, i16, i16, i16),
    ) {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return;
        }

        let style_size = Self::font_lookup_size(self.tx_size);
        let metrics = get_font_metrics(self.tx_font, style_size);
        let line_height = metrics.ascent + metrics.descent + metrics.leading;

        let h_text = Self::allocate_handle_with_data(bus, 0);
        let style_table = Self::allocate_handle_with_data(bus, Self::ST_ELEMENT_SIZE);
        let lh_table = Self::allocate_handle_with_data(bus, Self::LH_ELEMENT_SIZE);
        let null_scrap = Self::allocate_handle_with_data(bus, Self::STYLE_SCRAP_REC_SIZE);
        let null_style = Self::allocate_handle_with_data(bus, Self::NULL_STYLE_REC_SIZE);
        let style_handle = Self::allocate_handle_with_data(bus, 0x1C);

        let style_table_ptr = if style_table != 0 {
            bus.read_long(style_table)
        } else {
            0
        };
        if style_table_ptr != 0 {
            bus.write_word(style_table_ptr + Self::ST_ELEMENT_REFCOUNT_OFFSET, 1);
            bus.write_word(
                style_table_ptr + Self::ST_ELEMENT_HEIGHT_OFFSET,
                line_height as u16,
            );
            bus.write_word(
                style_table_ptr + Self::ST_ELEMENT_ASCENT_OFFSET,
                metrics.ascent as u16,
            );
            bus.write_word(
                style_table_ptr + Self::ST_ELEMENT_FONT_OFFSET,
                self.tx_font as u16,
            );
            bus.write_byte(
                style_table_ptr + Self::ST_ELEMENT_FACE_OFFSET,
                self.tx_face as u8,
            );
            bus.write_word(
                style_table_ptr + Self::ST_ELEMENT_SIZE_OFFSET,
                style_size as u16,
            );
            bus.write_word(
                style_table_ptr + Self::ST_ELEMENT_COLOR_OFFSET,
                self.fg_color.0,
            );
            bus.write_word(
                style_table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 2,
                self.fg_color.1,
            );
            bus.write_word(
                style_table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 4,
                self.fg_color.2,
            );
        }

        let lh_table_ptr = if lh_table != 0 {
            bus.read_long(lh_table)
        } else {
            0
        };
        if lh_table_ptr != 0 {
            bus.write_word(
                lh_table_ptr + Self::LH_ELEMENT_HEIGHT_OFFSET,
                line_height as u16,
            );
            bus.write_word(
                lh_table_ptr + Self::LH_ELEMENT_ASCENT_OFFSET,
                metrics.ascent as u16,
            );
        }

        let null_scrap_ptr = if null_scrap != 0 {
            bus.read_long(null_scrap)
        } else {
            0
        };
        if null_scrap_ptr != 0 {
            bus.write_word(null_scrap_ptr + Self::SCRAP_N_STYLES_OFFSET, 0);
            bus.write_long(
                null_scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_START_CHAR_OFFSET,
                0,
            );
            bus.write_word(
                null_scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_HEIGHT_OFFSET,
                line_height as u16,
            );
            bus.write_word(
                null_scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_ASCENT_OFFSET,
                metrics.ascent as u16,
            );
            bus.write_word(
                null_scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_FONT_OFFSET,
                self.tx_font as u16,
            );
            bus.write_byte(
                null_scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_FACE_OFFSET,
                self.tx_face as u8,
            );
            bus.write_word(
                null_scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_SIZE_OFFSET,
                style_size as u16,
            );
            bus.write_word(
                null_scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_COLOR_OFFSET,
                self.fg_color.0,
            );
            bus.write_word(
                null_scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_COLOR_OFFSET + 2,
                self.fg_color.1,
            );
            bus.write_word(
                null_scrap_ptr + Self::SCRAP_STYLE_TAB_OFFSET + Self::SCRAP_STYLE_COLOR_OFFSET + 4,
                self.fg_color.2,
            );
        }

        let null_style_ptr = if null_style != 0 {
            bus.read_long(null_style)
        } else {
            0
        };
        if null_style_ptr != 0 {
            bus.write_long(null_style_ptr + Self::NULL_STYLE_SCRAP_OFFSET, null_scrap);
        }

        let style_ptr = if style_handle != 0 {
            bus.read_long(style_handle)
        } else {
            0
        };
        if style_ptr != 0 {
            bus.write_word(style_ptr + Self::TE_STYLE_N_RUNS_OFFSET, 1);
            bus.write_word(style_ptr + Self::TE_STYLE_N_STYLES_OFFSET, 1);
            bus.write_long(style_ptr + Self::TE_STYLE_STYLE_TABLE_OFFSET, style_table);
            bus.write_long(style_ptr + Self::TE_STYLE_LH_TABLE_OFFSET, lh_table);
            bus.write_long(style_ptr + Self::TE_STYLE_NULL_STYLE_OFFSET, null_style);
            bus.write_word(style_ptr + Self::TE_STYLE_RUNS_OFFSET, 0);
            bus.write_word(style_ptr + Self::TE_STYLE_RUNS_OFFSET + 2, 0);
            bus.write_word(style_ptr + Self::TE_STYLE_RUNS_OFFSET + 4, 1);
            bus.write_word(style_ptr + Self::TE_STYLE_RUNS_OFFSET + 6, 0xFFFF);
        }

        Self::te_write_rect_words(bus, te_ptr + Self::TE_DEST_RECT_OFFSET, dest_rect);
        Self::te_write_rect_words(bus, te_ptr + Self::TE_VIEW_RECT_OFFSET, view_rect);
        Self::te_write_rect_words(bus, te_ptr + Self::TE_SEL_RECT_OFFSET, dest_rect);
        bus.write_word(te_ptr + Self::TE_LINE_HEIGHT_OFFSET, 0xFFFF);
        bus.write_word(te_ptr + Self::TE_FONT_ASCENT_OFFSET, 0xFFFF);
        bus.write_word(te_ptr + Self::TE_SEL_POINT_OFFSET, dest_rect.0 as u16);
        bus.write_word(te_ptr + Self::TE_SEL_POINT_OFFSET + 2, dest_rect.1 as u16);
        bus.write_word(te_ptr + Self::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + Self::TE_SEL_END_OFFSET, 0);
        bus.write_long(te_ptr + Self::TE_CARET_TIME_OFFSET, self.current_tick());
        bus.write_word(te_ptr + Self::TE_CARET_STATE_OFFSET, 0);
        bus.write_word(te_ptr + Self::TE_JUST_OFFSET, 0);
        bus.write_word(te_ptr + Self::TE_LENGTH_OFFSET, 0);
        bus.write_long(te_ptr + Self::TE_HTEXT_OFFSET, h_text);
        bus.write_long(te_ptr + Self::TE_TX_FONT_OFFSET, style_handle);
        bus.write_word(te_ptr + Self::TE_TX_MODE_OFFSET, self.tx_mode as u16);
        bus.write_word(te_ptr + Self::TE_TX_SIZE_OFFSET, 0xFFFF);
        bus.write_long(te_ptr + Self::TE_IN_PORT_OFFSET, *self.current_port);
        bus.write_word(te_ptr + Self::TE_N_LINES_OFFSET, 0);
        bus.write_word(te_ptr + Self::TE_LINE_STARTS_OFFSET, 0);
        self.textedit_states.remove(&te_handle);
    }

    fn te_text_length(bus: &MacMemoryBus, te_handle: u32) -> usize {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return 0;
        }

        let stored = bus.read_word(te_ptr + Self::TE_LENGTH_OFFSET) as usize;
        if stored != 0 {
            return stored;
        }

        let h_text = bus.read_long(te_ptr + Self::TE_HTEXT_OFFSET);
        if h_text == 0 {
            return 0;
        }
        let text_ptr = bus.read_long(h_text);
        if text_ptr == 0 {
            0
        } else {
            bus.get_alloc_size(text_ptr).unwrap_or(0) as usize
        }
    }

    fn te_text_bytes(bus: &MacMemoryBus, te_handle: u32) -> Vec<u8> {
        let len = Self::te_text_length(bus, te_handle);
        if len == 0 {
            return Vec::new();
        }

        let h_text = Self::te_text_handle(bus, te_handle);
        if h_text == 0 {
            return Vec::new();
        }
        let text_ptr = bus.read_long(h_text);
        if text_ptr == 0 {
            return Vec::new();
        }
        bus.read_bytes(text_ptr, len)
    }

    fn te_edit_buffer(
        bus: &MacMemoryBus,
        te_handle: u32,
    ) -> Option<crate::text_edit::TextEditBuffer> {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return None;
        }
        Some(crate::text_edit::TextEditBuffer::new(
            Self::te_text_bytes(bus, te_handle),
            bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET) as usize,
            bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) as usize,
        ))
    }

    fn te_commit_edit_buffer(
        &mut self,
        bus: &mut MacMemoryBus,
        te_handle: u32,
        buffer: &crate::text_edit::TextEditBuffer,
    ) {
        self.te_set_text_contents(bus, te_handle, buffer.text());
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return;
        }
        let selection = buffer.selection();
        let start = selection.start.min(u16::MAX as usize) as u16;
        let end = selection.end.min(u16::MAX as usize) as u16;
        bus.write_word(te_ptr + Self::TE_SEL_START_OFFSET, start);
        bus.write_word(te_ptr + Self::TE_SEL_END_OFFSET, end);
    }

    fn te_set_scrap_bytes(bus: &mut MacMemoryBus, selected: &[u8]) {
        use crate::memory::globals::addr;

        let mut scrap_handle = bus.read_long(addr::TE_SCRP_HANDLE);
        if scrap_handle == 0 {
            scrap_handle = Self::allocate_handle_with_data(bus, 0);
            bus.write_long(addr::TE_SCRP_HANDLE, scrap_handle);
        }
        let text_ptr = Self::ensure_text_handle_size(bus, scrap_handle, selected.len());
        if text_ptr != 0 && !selected.is_empty() {
            bus.write_bytes(text_ptr, selected);
        }
        bus.write_word(
            addr::TE_SCRP_LENGTH,
            selected.len().min(u16::MAX as usize) as u16,
        );
    }

    pub(crate) fn te_find_word_bounds(
        &self,
        bus: &MacMemoryBus,
        te_handle: u32,
        current_pos: usize,
    ) -> (u16, u16) {
        let text_bytes = Self::te_text_bytes(bus, te_handle);
        let len = text_bytes.len();
        if len == 0 {
            return (0, 0);
        }

        let pos = current_pos.min(len);
        if pos < len && Self::te_word_break_byte(text_bytes[pos]) {
            return (pos as u16, pos as u16);
        }

        let mut start = pos;
        while start > 0 && !Self::te_word_break_byte(text_bytes[start - 1]) {
            start -= 1;
        }

        let mut end = pos;
        while end < len && !Self::te_word_break_byte(text_bytes[end]) {
            end += 1;
        }

        (start as u16, end as u16)
    }

    #[allow(dead_code)]
    fn te_reset_styled_metadata(
        &mut self,
        bus: &mut MacMemoryBus,
        te_handle: u32,
        text_len: usize,
    ) {
        let style_handle = Self::te_style_handle(bus, te_handle);
        if style_handle == 0 {
            return;
        }

        let (font, face, size, color, line_height, ascent) = self.te_primary_style(bus, te_handle);
        let style_ptr = bus.read_long(style_handle);
        if style_ptr == 0 {
            return;
        }

        let style_table_handle = bus.read_long(style_ptr + Self::TE_STYLE_STYLE_TABLE_OFFSET);
        let style_table_ptr = if style_table_handle != 0 {
            bus.read_long(style_table_handle)
        } else {
            0
        };
        if style_table_ptr != 0 {
            bus.write_word(style_table_ptr + Self::ST_ELEMENT_REFCOUNT_OFFSET, 1);
            bus.write_word(
                style_table_ptr + Self::ST_ELEMENT_HEIGHT_OFFSET,
                line_height as u16,
            );
            bus.write_word(
                style_table_ptr + Self::ST_ELEMENT_ASCENT_OFFSET,
                ascent as u16,
            );
            bus.write_word(style_table_ptr + Self::ST_ELEMENT_FONT_OFFSET, font as u16);
            bus.write_byte(style_table_ptr + Self::ST_ELEMENT_FACE_OFFSET, face as u8);
            bus.write_word(style_table_ptr + Self::ST_ELEMENT_SIZE_OFFSET, size as u16);
            bus.write_word(style_table_ptr + Self::ST_ELEMENT_COLOR_OFFSET, color.0);
            bus.write_word(style_table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 2, color.1);
            bus.write_word(style_table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 4, color.2);
        }

        let lh_handle = bus.read_long(style_ptr + Self::TE_STYLE_LH_TABLE_OFFSET);
        let lh_ptr = if lh_handle != 0 {
            bus.read_long(lh_handle)
        } else {
            0
        };
        if lh_ptr != 0 {
            bus.write_word(lh_ptr + Self::LH_ELEMENT_HEIGHT_OFFSET, line_height as u16);
            bus.write_word(lh_ptr + Self::LH_ELEMENT_ASCENT_OFFSET, ascent as u16);
        }

        bus.write_word(style_ptr + Self::TE_STYLE_N_RUNS_OFFSET, 1);
        bus.write_word(style_ptr + Self::TE_STYLE_N_STYLES_OFFSET, 1);
        bus.write_word(style_ptr + Self::TE_STYLE_RUNS_OFFSET, 0);
        bus.write_word(style_ptr + Self::TE_STYLE_RUNS_OFFSET + 2, 0);
        bus.write_word(
            style_ptr + Self::TE_STYLE_RUNS_OFFSET + 4,
            text_len.saturating_add(1).min(u16::MAX as usize) as u16,
        );
        bus.write_word(style_ptr + Self::TE_STYLE_RUNS_OFFSET + 6, 0xFFFF);
    }

    fn te_set_text_contents(&mut self, bus: &mut MacMemoryBus, te_handle: u32, text: &[u8]) {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return;
        }

        let mut h_text = bus.read_long(te_ptr + Self::TE_HTEXT_OFFSET);
        if h_text == 0 {
            h_text = Self::allocate_handle_with_data(bus, 0);
            bus.write_long(te_ptr + Self::TE_HTEXT_OFFSET, h_text);
        }

        let text_ptr = Self::ensure_text_handle_size(bus, h_text, text.len());
        if !text.is_empty() && text_ptr != 0 {
            bus.write_bytes(text_ptr, text);
        }

        let clamped_len = text.len().min(u16::MAX as usize) as u16;
        bus.write_word(te_ptr + Self::TE_LENGTH_OFFSET, clamped_len);
        bus.write_word(te_ptr + Self::TE_SEL_START_OFFSET, clamped_len);
        bus.write_word(te_ptr + Self::TE_SEL_END_OFFSET, clamped_len);
        self.te_recalculate_layout(bus, te_handle);
    }

    fn textedit_idle(&mut self, cpu: &mut impl CpuOps, bus: &mut MacMemoryBus, te_handle: u32) {
        if trace_textedit_enabled() {
            eprintln!("[TE] TEIdle hTE=${te_handle:08X}");
        }
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return;
        }
        if bus.read_word(te_ptr + Self::TE_ACTIVE_OFFSET) == 0 {
            return;
        }

        let sel_start = bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET);
        let sel_end = bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET);
        if sel_start != sel_end {
            return;
        }

        let last_toggle = bus.read_long(te_ptr + Self::TE_CARET_TIME_OFFSET);
        if self.current_tick().wrapping_sub(last_toggle) < Self::TE_CARET_BLINK_TICKS {
            return;
        }

        // Stamp before painting. Idle-cycle proofs admit TEIdle on the
        // strength of these two guest-RAM writes preceding any draw: the
        // draw path below mutates host-cached port state the write journal
        // cannot see, and it is these stamps that make a proof containing a
        // blink fail on memory (runner.rs, idle_cycle_trap_is_journal_complete).
        let caret_state = bus.read_word(te_ptr + Self::TE_CARET_STATE_OFFSET);
        bus.write_word(
            te_ptr + Self::TE_CARET_STATE_OFFSET,
            if caret_state == 0 { 1 } else { 0 },
        );
        bus.write_long(te_ptr + Self::TE_CARET_TIME_OFFSET, self.current_tick());
        self.draw_te_contents(cpu, bus, te_handle, true);
    }

    fn te_insert_text(&mut self, bus: &mut MacMemoryBus, te_handle: u32, text: &[u8]) {
        let Some(mut buffer) = Self::te_edit_buffer(bus, te_handle) else {
            return;
        };
        buffer.replace_selection(text);
        self.te_commit_edit_buffer(bus, te_handle, &buffer);
    }

    // TextEdit measures each run with its own font, size and face, independent
    // of the caller's current port style. Inside Macintosh: Text (1993), 2-20.
    fn te_styled_char_width(style: TeResolvedStyle, byte: u8) -> i16 {
        let size = Self::font_lookup_size(style.size);
        let (_, scale) = get_font_face_scaled(style.font, size);
        let advance = crate::quickdraw::text::get_glyph(style.font, size, byte as char)
            .map_or(6, |(glyph, _)| i16::from(glyph.advance));
        advance * scale
            + crate::quickdraw::text::QuickDrawTextStyle::from_bits(style.face as u8)
                .advance_extra() as i16
    }

    fn te_char_width(&self, font: i16, size: i16, byte: u8) -> i16 {
        let size = Self::font_lookup_size(size);
        let (_face, scale) = get_font_face_scaled(font, size);
        let ch = byte as char;
        if let Some((glyph, _)) = crate::quickdraw::text::get_glyph(font, size, ch) {
            self.glyph_advance(glyph) * scale
        } else {
            self.missing_glyph_advance() * scale
        }
    }

    fn te_measure_text_width(
        &self,
        font: i16,
        size: i16,
        text_bytes: &[u8],
        start: usize,
        end: usize,
    ) -> i16 {
        let mut width = 0i16;
        for &b in &text_bytes[start..end] {
            width += self.te_char_width(font, size, b);
        }
        width
    }

    fn te_word_break_byte(byte: u8) -> bool {
        byte <= 0x20
    }

    fn te_wrap_lines(
        &self,
        font: i16,
        size: i16,
        text_bytes: &[u8],
        box_width: i16,
    ) -> Vec<(usize, usize)> {
        crate::quickdraw::text::wrap_classic_text(text_bytes, box_width, |_, byte| {
            self.te_char_width(font, size, byte)
        })
        .into_iter()
        .map(|line| (line.start, line.next))
        .collect()
    }

    fn te_recalculate_layout(&mut self, bus: &mut MacMemoryBus, te_handle: u32) {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return;
        }

        let text_bytes = Self::te_text_bytes(bus, te_handle);
        let text_len = text_bytes.len();
        let dest_rect = Self::te_read_rect(bus, te_ptr + Self::TE_DEST_RECT_OFFSET);
        let (font, _, size, _, line_height, font_ascent) = self.te_primary_style(bus, te_handle);
        let styled = Self::te_is_styled_record(bus, te_ptr);
        let style_runs = if styled {
            self.te_style_runs(bus, te_handle, text_len)
        } else {
            Vec::new()
        };
        let uses_styled_runs = styled && !style_runs.is_empty();
        let lines = if text_bytes.is_empty() {
            Vec::new()
        } else if uses_styled_runs {
            self.te_wrap_lines_styled(&style_runs, &text_bytes, (dest_rect.3 - dest_rect.1).max(0))
        } else {
            self.te_wrap_lines(font, size, &text_bytes, (dest_rect.3 - dest_rect.1).max(0))
        };
        let line_count = lines.len();
        let te_ptr = Self::ensure_te_record_line_capacity(bus, te_handle, line_count);
        if te_ptr == 0 {
            return;
        }

        bus.write_word(
            te_ptr + Self::TE_N_LINES_OFFSET,
            line_count.min(u16::MAX as usize) as u16,
        );
        if styled {
            bus.write_word(te_ptr + Self::TE_LINE_HEIGHT_OFFSET, 0xFFFF);
            bus.write_word(te_ptr + Self::TE_FONT_ASCENT_OFFSET, 0xFFFF);
        } else {
            bus.write_word(te_ptr + Self::TE_LINE_HEIGHT_OFFSET, line_height as u16);
            bus.write_word(te_ptr + Self::TE_FONT_ASCENT_OFFSET, font_ascent as u16);
        }

        for index in 0..=line_count.saturating_add(1) {
            bus.write_word(te_ptr + Self::TE_LINE_STARTS_OFFSET + (index as u32 * 2), 0);
        }
        for (index, (start, _)) in lines.iter().enumerate() {
            bus.write_word(
                te_ptr + Self::TE_LINE_STARTS_OFFSET + (index as u32 * 2),
                (*start).min(u16::MAX as usize) as u16,
            );
        }
        bus.write_word(
            te_ptr + Self::TE_LINE_STARTS_OFFSET + (line_count as u32 * 2),
            text_len.min(u16::MAX as usize) as u16,
        );
        bus.write_word(
            te_ptr + Self::TE_LINE_STARTS_OFFSET + ((line_count as u32 + 1) * 2),
            0,
        );

        if styled {
            let style_handle = Self::te_style_handle(bus, te_handle);
            if style_handle != 0 {
                let style_ptr = bus.read_long(style_handle);
                if style_ptr != 0 {
                    let lh_handle = bus.read_long(style_ptr + Self::TE_STYLE_LH_TABLE_OFFSET);
                    if lh_handle != 0 {
                        let lh_ptr = Self::ensure_handle_capacity(
                            bus,
                            lh_handle,
                            ((line_count + 1) as u32) * Self::LH_ELEMENT_SIZE,
                        );
                        if lh_ptr != 0 {
                            for index in 0..=line_count {
                                let (line_height, font_ascent) = if uses_styled_runs
                                    && index < lines.len()
                                {
                                    let (start, end) = lines[index];
                                    Self::te_line_metrics_for_range(&style_runs, start, end)
                                } else if uses_styled_runs {
                                    Self::te_line_metrics_for_range(&style_runs, text_len, text_len)
                                } else {
                                    (line_height, font_ascent)
                                };
                                let element = lh_ptr + (index as u32 * Self::LH_ELEMENT_SIZE);
                                bus.write_word(
                                    element + Self::LH_ELEMENT_HEIGHT_OFFSET,
                                    line_height as u16,
                                );
                                bus.write_word(
                                    element + Self::LH_ELEMENT_ASCENT_OFFSET,
                                    font_ascent as u16,
                                );
                            }
                        }
                    }
                }
            }
        }

        if styled {
            Self::te_update_styled_run_sentinel(bus, te_handle, text_len);
        }
    }

    fn port_has_drawable_bitmap(bus: &MacMemoryBus, port: u32) -> bool {
        if port == 0 {
            return false;
        }

        let port_version = bus.read_word(port + 6);
        let is_color = (port_version & 0xC000) != 0;
        let (base, row_bytes, top, left, bottom, right) = if is_color {
            let pix_map_handle = bus.read_long(port + 2);
            if pix_map_handle == 0 {
                return false;
            }
            let pix_map_ptr = bus.read_long(pix_map_handle);
            if pix_map_ptr == 0 {
                return false;
            }
            (
                Self::offscreen_pixmap_base_ptr(bus, pix_map_ptr),
                bus.read_word(pix_map_ptr + 4) & 0x3FFF,
                bus.read_word(pix_map_ptr + 6) as i16,
                bus.read_word(pix_map_ptr + 8) as i16,
                bus.read_word(pix_map_ptr + 10) as i16,
                bus.read_word(pix_map_ptr + 12) as i16,
            )
        } else {
            (
                bus.read_long(port + 2),
                bus.read_word(port + 6) & 0x3FFF,
                bus.read_word(port + 8) as i16,
                bus.read_word(port + 10) as i16,
                bus.read_word(port + 12) as i16,
                bus.read_word(port + 14) as i16,
            )
        };

        base != 0 && row_bytes != 0 && top < bottom && left < right
    }

    fn draw_te_contents(
        &mut self,
        cpu: &mut impl CpuOps,
        bus: &mut MacMemoryBus,
        te_handle: u32,
        erase_background: bool,
    ) {
        let te_ptr = Self::te_record_ptr(bus, te_handle);
        if te_ptr == 0 {
            return;
        }

        let text_bytes = Self::te_text_bytes(bus, te_handle);
        let dest_rect = Self::te_read_rect(bus, te_ptr + Self::TE_DEST_RECT_OFFSET);
        let view_rect = Self::te_read_rect(bus, te_ptr + Self::TE_VIEW_RECT_OFFSET);
        let te_port = bus.read_long(te_ptr + Self::TE_IN_PORT_OFFSET);
        if !Self::port_has_drawable_bitmap(bus, te_port) {
            return;
        }
        // Theme overlays use screen coordinates; TERec geometry is port-local.
        // Imaging With QuickDraw (1994), pp. 2-9--2-10.
        let (port_top, port_left) = self.port_bounds_top_left(bus, te_port);
        let just = bus.read_word(te_ptr + Self::TE_JUST_OFFSET) as i16;
        let (font, face, size, color, _, _) = self.te_primary_style(bus, te_handle);
        let styled_runs = if Self::te_is_styled_record(bus, te_ptr) {
            self.te_style_runs(bus, te_handle, text_bytes.len())
        } else {
            Vec::new()
        };
        let uses_styled_runs = !styled_runs.is_empty() && Self::te_is_styled_record(bus, te_ptr);
        let old_font = self.tx_font;
        let old_face = self.tx_face;
        let old_size = self.tx_size;
        let old_fg = self.fg_color;
        let old_loc = self.pn_loc;
        let previous_port = *self.current_port;
        let previous_gdevice = *self.current_gdevice;
        let switched_port = te_port != 0 && te_port != previous_port;
        let color_port = te_port != 0 && (bus.read_word(te_port + 6) & 0xC000) == 0xC000;
        let old_port_fg_pixel = color_port.then(|| bus.read_long(te_port + 80));
        let old_resolved_color_fields = self.resolved_port_color_fields.get(&te_port).copied();

        // Always sync A5 globals to te_port before drawing, even if port didn't change.
        // draw_generic_shape reads from A5 globals, and untrack_window can set self.current_port
        // without updating A5 globals, causing a divergence that sends drawing to the wrong port.
        if te_port != 0 {
            self.set_current_port_state(bus, cpu, te_port, None);
        }
        if trace_textedit_enabled() {
            let (vis_top, vis_left, vis_bottom, vis_right) = {
                let vis_handle = bus.read_long(*self.current_port + 24);
                let vis_ptr = if vis_handle != 0 {
                    bus.read_long(vis_handle)
                } else {
                    0
                };
                if vis_ptr != 0 {
                    (
                        bus.read_word(vis_ptr + 2) as i16,
                        bus.read_word(vis_ptr + 4) as i16,
                        bus.read_word(vis_ptr + 6) as i16,
                        bus.read_word(vis_ptr + 8) as i16,
                    )
                } else {
                    (0, 0, 0, 0)
                }
            };
            let (clip_top, clip_left, clip_bottom, clip_right) = {
                let clip_handle = bus.read_long(*self.current_port + 28);
                let clip_ptr = if clip_handle != 0 {
                    bus.read_long(clip_handle)
                } else {
                    0
                };
                if clip_ptr != 0 {
                    (
                        bus.read_word(clip_ptr + 2) as i16,
                        bus.read_word(clip_ptr + 4) as i16,
                        bus.read_word(clip_ptr + 6) as i16,
                        bus.read_word(clip_ptr + 8) as i16,
                    )
                } else {
                    (0, 0, 0, 0)
                }
            };
            eprintln!(
                "[TE] draw_te_contents hTE=${:08X} te_port=${:08X} prev_port=${:08X} current_port=${:08X} switched={} gworld={} dest=({},{},{},{}) view=({},{},{},{}) vis=({},{},{},{}) clip=({},{},{},{})",
                te_handle,
                te_port,
                previous_port,
                *self.current_port,
                switched_port,
                self.gworld_devices.contains_key(&te_port),
                dest_rect.0,
                dest_rect.1,
                dest_rect.2,
                dest_rect.3,
                view_rect.0,
                view_rect.1,
                view_rect.2,
                view_rect.3,
                vis_top,
                vis_left,
                vis_bottom,
                vis_right,
                clip_top,
                clip_left,
                clip_bottom,
                clip_right,
            );
        }

        let checksum_view_rect =
            |bus: &MacMemoryBus, port: u32, rect: (i16, i16, i16, i16)| -> u64 {
                if port == 0 {
                    return 0;
                }
                let port_version = bus.read_word(port + 6);
                let is_color = (port_version & 0xC000) != 0;
                let (pix_base, row_bytes, bounds_top, bounds_left, bounds_bottom, bounds_right) =
                    if is_color {
                        let pix_map_handle = bus.read_long(port + 2);
                        if pix_map_handle == 0 {
                            return 0;
                        }
                        let pix_map_ptr = bus.read_long(pix_map_handle);
                        if pix_map_ptr == 0 {
                            return 0;
                        }
                        (
                            Self::offscreen_pixmap_base_ptr(bus, pix_map_ptr),
                            (bus.read_word(pix_map_ptr + 4) & 0x3FFF) as u32,
                            bus.read_word(pix_map_ptr + 6) as i16,
                            bus.read_word(pix_map_ptr + 8) as i16,
                            bus.read_word(pix_map_ptr + 10) as i16,
                            bus.read_word(pix_map_ptr + 12) as i16,
                        )
                    } else {
                        (
                            bus.read_long(port + 2),
                            (bus.read_word(port + 6) & 0x3FFF) as u32,
                            bus.read_word(port + 8) as i16,
                            bus.read_word(port + 10) as i16,
                            bus.read_word(port + 12) as i16,
                            bus.read_word(port + 14) as i16,
                        )
                    };
                let top = rect.0.max(bounds_top);
                let left = rect.1.max(bounds_left);
                let bottom = rect.2.min(bounds_bottom);
                let right = rect.3.min(bounds_right);
                if top >= bottom || left >= right || pix_base == 0 {
                    return 0;
                }
                let mut sum = 0u64;
                for y in top..bottom {
                    let dy = (y - bounds_top) as u32;
                    for x in left..right {
                        let dx = (x - bounds_left) as u32;
                        sum =
                            sum.wrapping_add(bus.read_byte(pix_base + dy * row_bytes + dx) as u64);
                    }
                }
                sum
            };
        let checksum_before = if trace_textedit_enabled() {
            checksum_view_rect(bus, *self.current_port, view_rect)
        } else {
            0
        };

        self.tx_font = font;
        self.tx_face = face;
        self.tx_size = size;
        self.fg_color = color;

        let clip_top = view_rect.0;
        let clip_left = view_rect.1;
        let clip_bottom = view_rect.2;
        let clip_right = view_rect.3;
        let box_width = (dest_rect.3 - dest_rect.1).max(0);

        if erase_background {
            self.draw_rect(
                cpu,
                bus,
                &Rect {
                    top: clip_top,
                    left: clip_left,
                    bottom: clip_bottom,
                    right: clip_right,
                },
                ShapeOp::Erase,
            );
        }
        let lines = if text_bytes.is_empty() {
            vec![(0usize, 0usize)]
        } else if uses_styled_runs {
            self.te_wrap_lines_styled(&styled_runs, &text_bytes, box_width)
        } else {
            self.te_wrap_lines(font, size, &text_bytes, box_width)
        };

        let mut top = dest_rect.0;
        let mut visible_lines = Vec::new();
        let mut visual_lines = Vec::new();
        for (index, (start, end)) in lines.iter().enumerate() {
            let line_height = Self::te_height_for_line(bus, te_handle, index);
            let line_ascent = Self::te_ascent_for_line(bus, te_handle, index);
            let line_bottom = top.saturating_add(line_height);
            if line_bottom <= clip_top {
                top = line_bottom;
                continue;
            }
            if top >= clip_bottom {
                break;
            }
            visible_lines.push(index);
            let mut trimmed_end = *end;
            while trimmed_end > *start
                && matches!(text_bytes[trimmed_end - 1], b' ' | b'\r' | b'\n')
            {
                trimmed_end -= 1;
            }
            let line_width = if uses_styled_runs {
                self.te_measure_text_width_styled(&styled_runs, &text_bytes, *start, trimmed_end)
            } else {
                self.te_measure_text_width(font, size, &text_bytes, *start, trimmed_end)
            };
            let x = crate::text_edit::aligned_line_left(
                dest_rect.1,
                dest_rect.3,
                line_width,
                just,
                Self::TE_LINE_LEFT_INSET,
            );
            visual_lines.push((*start, *end, top, line_bottom, x));
            self.pn_loc = (top.saturating_add(line_ascent), x);
            for (offset, &byte) in text_bytes[*start..trimmed_end].iter().enumerate() {
                if uses_styled_runs {
                    let style = Self::te_style_at_offset(&styled_runs, *start + offset);
                    self.tx_font = style.font;
                    self.tx_face = style.face;
                    self.tx_size = style.size;
                    self.fg_color = style.color;
                    // A CGrafPort keeps both the logical RGB foreground and a
                    // destination-specific pixel value. Styled TextEdit owns
                    // its run colors, so resolve each run's RGB value before
                    // drawing without leaking that temporary pixel into the
                    // caller's port state.
                    self.resolve_current_port_color_pixels(bus, true, false, false);
                }
                self.draw_char(cpu, bus, byte as char);
            }

            top = line_bottom;
        }

        let te_active = bus.read_word(te_ptr + Self::TE_ACTIVE_OFFSET) != 0;
        let caret_visible = bus.read_word(te_ptr + Self::TE_CARET_STATE_OFFSET) == 0;
        let outline_hilite = self.te_feature_bit(te_handle, Self::TE_FEATURE_OUTLINE_HILITE);
        let sel_start = bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET) as usize;
        let sel_end = bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) as usize;
        let (selection_start, selection_end) = if sel_start <= sel_end {
            (sel_start, sel_end)
        } else {
            (sel_end, sel_start)
        };
        if selection_start == selection_end {
            if te_active && caret_visible {
                if let Some(&(line_start, line_end, line_top, line_bottom, line_x)) = visual_lines
                    .iter()
                    .find(|&&(start, end, _, _, _)| {
                        selection_start >= start && selection_start <= end
                    })
                    .or_else(|| visual_lines.last())
                {
                    let caret_offset = selection_start.min(line_end).max(line_start);
                    let measured_width = if uses_styled_runs {
                        self.te_measure_text_width_styled(
                            &styled_runs,
                            &text_bytes,
                            line_start,
                            caret_offset,
                        )
                    } else {
                        self.te_measure_text_width(
                            font,
                            size,
                            &text_bytes,
                            line_start,
                            caret_offset,
                        )
                    };
                    let mut caret_x = line_x + measured_width;
                    if caret_offset > line_start {
                        caret_x = caret_x.saturating_sub(1);
                    }
                    let caret_width = self.ui_theme().text_theme().caret_width.max(1);
                    // Text (1993), pp. 2-16 and 2-29: viewRect bounds the
                    // visible portion of a TextEdit record. A partial last
                    // line can be shorter than lineHeight. Painting its full
                    // caret leaves pixels outside the area erased next time.
                    if let Some((top, left, bottom, right)) = Self::rect_intersection(
                        (line_top, caret_x, line_bottom, caret_x.saturating_add(caret_width)),
                        view_rect,
                    ) {
                        if !self.draw_theme_caret_clipped(
                            bus,
                            (
                                top.wrapping_sub(port_top),
                                left.wrapping_sub(port_left),
                                bottom.wrapping_sub(port_top),
                                right.wrapping_sub(port_left),
                            ),
                            Some((
                                view_rect.0.wrapping_sub(port_top),
                                view_rect.1.wrapping_sub(port_left),
                                view_rect.2.wrapping_sub(port_top),
                                view_rect.3.wrapping_sub(port_left),
                            )),
                        ) {
                            self.draw_rect(
                                cpu,
                                bus,
                                &Rect { top, left, bottom, right },
                                ShapeOp::Paint,
                            );
                        }
                    }
                }
            }
        } else if te_active || outline_hilite {
            // IM:I I-375/I-385 and Text 1993 p. 2-80: an active edit
            // record displays its selection range by highlighting the
            // selected characters. Classic black-and-white TextEdit uses
            // inverse video; non-classic themes own their selection chrome.
            for &(line_start, line_end, line_top, line_bottom, line_x) in &visual_lines {
                let start = selection_start.max(line_start);
                let end = selection_end.min(line_end);
                if start >= end {
                    continue;
                }
                let mut selection_left = line_x
                    + if uses_styled_runs {
                        self.te_measure_text_width_styled(
                            &styled_runs,
                            &text_bytes,
                            line_start,
                            start,
                        )
                    } else {
                        self.te_measure_text_width(font, size, &text_bytes, line_start, start)
                    };
                let selection_right = line_x
                    + if uses_styled_runs {
                        self.te_measure_text_width_styled(
                            &styled_runs,
                            &text_bytes,
                            line_start,
                            end,
                        )
                    } else {
                        self.te_measure_text_width(font, size, &text_bytes, line_start, end)
                    };
                if start == line_start && matches!(just, 0 | -2) {
                    // BasiliskII/System 7.5.3 `a9d3_teupdate_selection_visual`
                    // shows a selection beginning at a left-justified visual
                    // line includes TextEdit's one-pixel left inset; glyphs
                    // still draw at destRect.left + TE_LINE_LEFT_INSET.
                    selection_left = selection_left.saturating_sub(Self::TE_LINE_LEFT_INSET);
                }
                if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
                    if te_active {
                        self.draw_rect(
                            cpu,
                            bus,
                            &Rect {
                                top: line_top,
                                left: selection_left,
                                bottom: line_bottom,
                                right: selection_right,
                            },
                            ShapeOp::Invert,
                        );
                    }
                } else {
                    // Text 1993, "Outline Highlighting" and TEFeatureFlag
                    // teFOutlineHilite: inactive selection ranges are framed,
                    // while active ranges keep normal highlighted selection chrome.
                    self.draw_theme_text_selection(
                        bus,
                        line_top.wrapping_sub(port_top),
                        selection_left.wrapping_sub(port_left),
                        line_bottom.wrapping_sub(port_top),
                        selection_right.wrapping_sub(port_left),
                        te_active,
                    );
                }
            }
        }

        self.tx_font = old_font;
        self.tx_face = old_face;
        self.tx_size = old_size;
        self.fg_color = old_fg;
        self.pn_loc = old_loc;
        if let Some(pixel) = old_port_fg_pixel {
            bus.write_long(te_port + 80, pixel);
        }
        if let Some(fields) = old_resolved_color_fields {
            self.resolved_port_color_fields.insert(te_port, fields);
        } else {
            self.resolved_port_color_fields.remove(&te_port);
        }
        if trace_textedit_enabled() {
            eprintln!(
                "[TE] draw_te_contents visible_lines hTE=${:08X} {:?}",
                te_handle, visible_lines
            );
            eprintln!(
                "[TE] draw_te_contents checksum hTE=${:08X} before={} after={}",
                te_handle,
                checksum_before,
                checksum_view_rect(bus, *self.current_port, view_rect)
            );
        }

        if switched_port {
            self.set_current_port_state(bus, cpu, previous_port, Some(previous_gdevice));
        }
    }

    fn te_feature_bit(&self, te_handle: u32, feature: u16) -> bool {
        self.textedit_states.feature_bit(te_handle, feature)
    }

    /// Whether auto-scroll is enabled on the TextEdit record at
    /// `te_handle` (set by `TEAutoView` per IM:V V-173). Public
    /// projection over `textedit_states[te].feature_bits` so
    /// integration tests can observe TEAutoView's effect without
    /// access to the private feature-bit constants.
    pub fn te_auto_scroll_enabled(&self, te_handle: u32) -> bool {
        self.te_feature_bit(te_handle, Self::TE_FEATURE_AUTO_SCROLL)
    }

    fn set_te_feature_bit(&mut self, te_handle: u32, feature: u16, enabled: bool) {
        self.textedit_states
            .set_feature_bit(te_handle, feature, enabled);
    }

    fn initialize_dialog_item_handles(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        items: &[DialogItem],
    ) {
        self.initialize_dialog_item_handles_from(bus, dialog_ptr, items, 0);
    }

    fn initialize_dialog_item_handles_from(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        items: &[DialogItem],
        start_index: usize,
    ) {
        // Item lists in memory hold live item handles or userItem ProcPtrs.
        // GetNewDialog reaches here with a copy of the DITL resource; NewDialog
        // initializes the caller-supplied item-list handle.
        // Inside Macintosh Volume I, I-405, I-412; MTE 1992 p. 6-114.
        for (index, item) in items.iter().enumerate().skip(start_index) {
            let item_no = index as i16 + 1;
            let Some(item_handle_addr) = Self::dialog_item_handle_addr(bus, dialog_ptr, item_no)
            else {
                continue;
            };

            let base_type = item.item_type & 0x7F;
            let item_handle = match base_type {
                8 | 16 => {
                    let text_bytes = encode_mac_roman_lossy(&item.text);
                    let handle = bus.alloc(4);
                    let text_ptr = if text_bytes.is_empty() {
                        0
                    } else {
                        let ptr = bus.alloc(text_bytes.len() as u32);
                        bus.write_bytes(ptr, &text_bytes);
                        ptr
                    };
                    bus.write_long(handle, text_ptr);
                    self.dialog_item_handles.insert(handle, (dialog_ptr, index));
                    handle
                }
                32 => self
                    .find_or_load_resource_any(bus, *b"ICON", item.resource_id)
                    .map(|(_, ptr)| {
                        self.get_or_create_resource_handle(bus, *b"ICON", item.resource_id, ptr)
                    })
                    .unwrap_or(0),
                4..=6 => {
                    let existing = bus.read_long(item_handle_addr);
                    if existing != 0 {
                        existing
                    } else {
                        self.create_standard_dialog_control_handle(bus, dialog_ptr, item_no, item)
                    }
                }
                7 => self
                    .find_or_load_resource_any(bus, *b"CNTL", item.resource_id)
                    .map(|(_, cntl_ptr)| {
                        let value = bus.read_word(cntl_ptr + 8) as i16;
                        let vis_word = bus.read_word(cntl_ptr + 10);
                        let max = bus.read_word(cntl_ptr + 12) as i16;
                        let min = bus.read_word(cntl_ptr + 14) as i16;
                        let proc_id = bus.read_word(cntl_ptr + 16) as i16;
                        let ref_con = bus.read_long(cntl_ptr + 18);
                        let title_len = bus.read_byte(cntl_ptr + 22) as usize;
                        let title = bus.read_bytes(cntl_ptr + 23, title_len.min(255));
                        let ctrl_ptr = bus.alloc(296);
                        let handle = bus.alloc(4);
                        bus.write_long(handle, ctrl_ptr);
                        self.initialize_control_record(
                            bus,
                            ctrl_ptr,
                            dialog_ptr,
                            item.rect,
                            &title,
                            vis_word != 0,
                            value,
                            min,
                            max,
                            proc_id,
                            ref_con,
                        );
                        self.control_manager.associate_handle(handle, ctrl_ptr);
                        self.ensure_control_aux_record(bus, handle);
                        let old_head = bus.read_long(dialog_ptr + 140);
                        bus.write_long(ctrl_ptr, old_head);
                        bus.write_long(dialog_ptr + 140, handle);
                        self.dialog_control_handles
                            .insert(handle, (dialog_ptr, item_no));
                        self.dialog_control_values
                            .insert((dialog_ptr, item_no), value);
                        handle
                    })
                    .unwrap_or(0),
                64 => self
                    .find_or_load_resource_any(bus, *b"PICT", item.resource_id)
                    .map(|(_, ptr)| {
                        self.get_or_create_resource_handle(bus, *b"PICT", item.resource_id, ptr)
                    })
                    .unwrap_or(0),
                0 => item.proc_ptr,
                _ => 0,
            };

            bus.write_long(item_handle_addr, item_handle);
        }
    }

    fn create_standard_dialog_control_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        item_no: i16,
        item: &DialogItem,
    ) -> u32 {
        let proc_id = match item.item_type & 0x7F {
            4 => 0, // btnCtrl -> pushButProc
            5 => 1, // chkCtrl -> checkBoxProc
            6 => 2, // radCtrl -> radioButProc
            _ => return 0,
        };
        let value = self
            .dialog_control_values
            .get(&(dialog_ptr, item_no))
            .copied()
            .unwrap_or(0);
        let ctrl_ptr = bus.alloc(296);
        let handle = bus.alloc(4);
        if ctrl_ptr == 0 || handle == 0 {
            return 0;
        }

        bus.write_long(handle, ctrl_ptr);
        let title = encode_mac_roman_lossy(&item.text);
        self.initialize_control_record(
            bus,
            ctrl_ptr,
            dialog_ptr,
            item.rect,
            &title,
            true,
            value,
            0,
            1,
            proc_id,
            0,
        );
        self.control_manager.associate_handle(handle, ctrl_ptr);
        self.ensure_control_aux_record(bus, handle);

        let old_head = bus.read_long(dialog_ptr + 140);
        bus.write_long(ctrl_ptr, old_head);
        bus.write_long(dialog_ptr + 140, handle);
        self.dialog_control_handles
            .insert(handle, (dialog_ptr, item_no));
        self.dialog_control_values
            .insert((dialog_ptr, item_no), value);
        handle
    }

    fn ditl_item_record_len_at(
        bus: &MacMemoryBus,
        ptr: u32,
        data_len: u32,
        record_offset: u32,
    ) -> Option<u32> {
        if record_offset + 14 > data_len {
            return None;
        }

        let item_type = bus.read_byte(ptr + record_offset + 12);
        let data_len_byte = bus.read_byte(ptr + record_offset + 13);
        let base_type = item_type & 0x7F;
        let payload_offset = record_offset + 14;
        let remaining = data_len.saturating_sub(payload_offset);
        let payload_len = Self::ditl_item_payload_len(base_type, data_len_byte, remaining)?;
        let padded = (payload_len + 1) & !1;
        if padded > remaining {
            return None;
        }

        Some(14 + padded)
    }

    fn ditl_used_len(bus: &MacMemoryBus, ptr: u32, data_len: u32) -> u32 {
        if ptr == 0 || data_len < 2 {
            return data_len;
        }

        let max_index = bus.read_word(ptr) as i16;
        let count = if max_index < 0 {
            0
        } else {
            max_index as usize + 1
        };
        let mut offset = 2u32;
        for _ in 0..count {
            let Some(record_len) = Self::ditl_item_record_len_at(bus, ptr, data_len, offset) else {
                break;
            };
            offset = offset.saturating_add(record_len);
        }

        offset.min(data_len)
    }

    fn dialog_local_size(bus: &MacMemoryBus, dialog_ptr: u32) -> (i16, i16) {
        let height = (bus.read_word(dialog_ptr + 20) as i16)
            .saturating_sub(bus.read_word(dialog_ptr + 16) as i16);
        let width = (bus.read_word(dialog_ptr + 22) as i16)
            .saturating_sub(bus.read_word(dialog_ptr + 18) as i16);
        (height, width)
    }

    fn append_ditl_offset(&self, bus: &MacMemoryBus, dialog_ptr: u32, method: i16) -> (i16, i16) {
        match method {
            // overlayDITL: appended rectangles are already dialog-local.
            // Macintosh Toolbox Essentials 1992, pp. 6-108, 6-153.
            0 => (0, 0),
            // appendDITLRight / appendDITLBottom position relative to the
            // dialog's upper-right or lower-left coordinate. The HLE updates
            // item rectangles here; full window resizing is handled by callers
            // that subsequently show/redraw the dialog.
            1 => {
                let (_, width) = Self::dialog_local_size(bus, dialog_ptr);
                (0, width)
            }
            2 => {
                let (height, _) = Self::dialog_local_size(bus, dialog_ptr);
                (height, 0)
            }
            item_method if item_method < 0 => {
                let item_no = item_method.checked_neg().unwrap_or(i16::MAX) as usize;
                self.dialog_items
                    .get(&dialog_ptr)
                    .and_then(|items| item_no.checked_sub(1).and_then(|index| items.get(index)))
                    .map(|item| (item.rect.0, item.rect.1))
                    .unwrap_or((0, 0))
            }
            _ => (0, 0),
        }
    }

    fn offset_dialog_item_rect(item: &mut DialogItem, v_delta: i16, h_delta: i16) {
        item.rect = (
            item.rect.0.saturating_add(v_delta),
            item.rect.1.saturating_add(h_delta),
            item.rect.2.saturating_add(v_delta),
            item.rect.3.saturating_add(h_delta),
        );
    }

    fn erase_retained_dialog_items_after_ditl_shorten(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        removed_items: &[DialogItem],
    ) {
        if removed_items.is_empty() || dialog_ptr == 0 {
            return;
        }
        let bounds = Self::dialog_screen_bounds(bus, dialog_ptr);
        for item in removed_items {
            let (top, left, bottom, right) = Self::dialog_item_screen_rect(bounds, item.rect);
            self.fill_dialog_content_rect(
                bus,
                dialog_ptr,
                bounds,
                (top - 2, left - 2, bottom + 2, right + 2),
            );
        }
        if self.dialog_visible_snapshots.contains_key(&dialog_ptr) {
            let pixels = self.save_dialog_pixels(bus, bounds);
            self.dialog_visible_snapshots
                .insert(dialog_ptr, PersistentDialogSnapshot { bounds, pixels });
        }
        let tracking_bounds = self
            .dialog_tracking
            .as_ref()
            .filter(|tracking| tracking.dialog_ptr == dialog_ptr)
            .map(|tracking| tracking.bounds);
        if let Some(bounds) = tracking_bounds {
            let rendered = self.save_dialog_pixels(bus, bounds);
            if let Some(tracking) = self.dialog_tracking.as_mut() {
                if tracking.dialog_ptr == dialog_ptr {
                    tracking.rendered_pixels = rendered;
                    tracking.rendered_pixels_final = true;
                }
            }
        }
    }

    pub(crate) fn append_ditl_to_dialog(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        ditl_handle: u32,
        method: i16,
    ) -> u16 {
        if dialog_ptr == 0 || ditl_handle == 0 {
            return self
                .dialog_items
                .get(&dialog_ptr)
                .map(|items| items.len() as u16)
                .unwrap_or(0);
        }

        let source_ptr = bus.read_long(ditl_handle);
        let Some(source_alloc_len) = bus.get_alloc_size(source_ptr) else {
            return self
                .dialog_items
                .get(&dialog_ptr)
                .map(|items| items.len() as u16)
                .unwrap_or(0);
        };
        let source_len = Self::ditl_used_len(bus, source_ptr, source_alloc_len);
        if source_len <= 2 {
            return self
                .dialog_items
                .get(&dialog_ptr)
                .map(|items| items.len() as u16)
                .unwrap_or(0);
        }

        let mut appended_items = Self::parse_ditl(bus, source_ptr, source_len);
        if appended_items.is_empty() {
            return self
                .dialog_items
                .get(&dialog_ptr)
                .map(|items| items.len() as u16)
                .unwrap_or(0);
        }

        let mut items_handle = bus.read_long(dialog_ptr + 156);
        if items_handle == 0 {
            items_handle = bus.alloc(4);
            let empty_ditl_ptr = bus.alloc(2);
            bus.write_word(empty_ditl_ptr, 0xFFFF);
            bus.write_long(items_handle, empty_ditl_ptr);
            bus.write_long(dialog_ptr + 156, items_handle);
        }

        let old_ptr = bus.read_long(items_handle);
        let old_alloc_len = bus.get_alloc_size(old_ptr).unwrap_or(0);
        let old_len = if old_ptr != 0 && old_alloc_len >= 2 {
            Self::ditl_used_len(bus, old_ptr, old_alloc_len).max(2)
        } else {
            2
        };
        let raw_existing_items = if old_ptr != 0 && old_alloc_len >= 2 {
            Self::parse_ditl(bus, old_ptr, old_len)
        } else {
            Vec::new()
        };
        let mut all_items = self
            .dialog_items
            .get(&dialog_ptr)
            .cloned()
            .unwrap_or_else(|| raw_existing_items.clone());
        if all_items.len() != raw_existing_items.len() && !raw_existing_items.is_empty() {
            all_items = raw_existing_items;
        }

        let start_index = all_items.len();
        let source_body_len = source_len.saturating_sub(2);
        let new_len = old_len.saturating_add(source_body_len);
        let new_ptr = bus.alloc(new_len);
        if new_ptr == 0 {
            return all_items.len() as u16;
        }

        if old_ptr != 0 && old_alloc_len >= 2 {
            let old_bytes = bus.read_bytes(old_ptr, old_len as usize);
            bus.write_bytes(new_ptr, &old_bytes);
        } else {
            bus.write_word(new_ptr, 0xFFFF);
        }
        let source_body = bus.read_bytes(source_ptr + 2, source_body_len as usize);
        bus.write_bytes(new_ptr + old_len, &source_body);

        let (v_delta, h_delta) = self.append_ditl_offset(bus, dialog_ptr, method);
        let mut source_offset = 2u32;
        let mut dest_offset = old_len;
        for item in &mut appended_items {
            if v_delta != 0 || h_delta != 0 {
                Self::offset_dialog_item_rect(item, v_delta, h_delta);
                bus.write_word(new_ptr + dest_offset + 4, item.rect.0 as u16);
                bus.write_word(new_ptr + dest_offset + 6, item.rect.1 as u16);
                bus.write_word(new_ptr + dest_offset + 8, item.rect.2 as u16);
                bus.write_word(new_ptr + dest_offset + 10, item.rect.3 as u16);
            }

            let Some(record_len) =
                Self::ditl_item_record_len_at(bus, source_ptr, source_len, source_offset)
            else {
                break;
            };
            source_offset = source_offset.saturating_add(record_len);
            dest_offset = dest_offset.saturating_add(record_len);
        }

        all_items.extend(appended_items);
        let new_count = all_items.len();
        let max_index = if new_count == 0 {
            0xFFFF
        } else {
            (new_count as u16).saturating_sub(1)
        };
        bus.write_word(new_ptr, max_index);
        bus.write_long(items_handle, new_ptr);

        self.dialog_items.insert(dialog_ptr, all_items.clone());
        self.initialize_dialog_item_handles_from(bus, dialog_ptr, &all_items, start_index);

        new_count as u16
    }

    pub(crate) fn shorten_ditl_in_dialog(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        number_items: u16,
    ) -> u16 {
        if dialog_ptr == 0 {
            return 0;
        }

        let items_handle = bus.read_long(dialog_ptr + 156);
        let ditl_ptr = if items_handle != 0 {
            bus.read_long(items_handle)
        } else {
            0
        };
        let ditl_len = bus.get_alloc_size(ditl_ptr).unwrap_or(0);
        let raw_items = if ditl_ptr != 0 && ditl_len >= 2 {
            Self::parse_ditl(bus, ditl_ptr, Self::ditl_used_len(bus, ditl_ptr, ditl_len))
        } else {
            Vec::new()
        };
        let mut items = self
            .dialog_items
            .get(&dialog_ptr)
            .cloned()
            .unwrap_or(raw_items);
        let remove_count = (number_items as usize).min(items.len());
        let keep_count = items.len().saturating_sub(remove_count);

        for item_no in (keep_count + 1)..=items.len() {
            let item_no = item_no as i16;
            let handle = Self::dialog_item_handle(bus, dialog_ptr, item_no);
            self.dialog_control_values.remove(&(dialog_ptr, item_no));
            self.dialog_control_handles.remove(&handle);
            self.dialog_item_handles.remove(&handle);
        }

        let removed_items = items[keep_count..].to_vec();
        items.truncate(keep_count);
        if ditl_ptr != 0 && ditl_len >= 2 {
            let max_index = if keep_count == 0 {
                0xFFFF
            } else {
                (keep_count as u16).saturating_sub(1)
            };
            bus.write_word(ditl_ptr, max_index);
        }
        self.dialog_items.insert(dialog_ptr, items);
        self.erase_retained_dialog_items_after_ditl_shorten(bus, dialog_ptr, &removed_items);

        keep_count as u16
    }

    // ========== DLOG / DITL Resource Parsing ==========

    /// Parse a DLOG resource from guest memory.
    /// Returns (bounds, procID, visible, itemsID, title, position).
    /// Inside Macintosh Volume I, I-437
    /// Macintosh Toolbox Essentials 1992, p. 6-148
    fn parse_dlog(
        bus: &MacMemoryBus,
        ptr: u32,
        data_len: u32,
    ) -> ((i16, i16, i16, i16), i16, bool, i16, String, u16) {
        let top = bus.read_word(ptr) as i16;
        let left = bus.read_word(ptr + 2) as i16;
        let bottom = bus.read_word(ptr + 4) as i16;
        let right = bus.read_word(ptr + 6) as i16;
        let proc_id = bus.read_word(ptr + 8) as i16;
        let visible = bus.read_byte(ptr + 10) != 0;
        // +11: filler
        // +12: goAwayFlag (1 byte)
        // +13: filler
        let _ref_con = bus.read_long(ptr + 14);
        let items_id = bus.read_word(ptr + 18) as i16;
        // +20: title as Pascal string
        let title_len = bus.read_byte(ptr + 20) as usize;
        let mut title_bytes = vec![0u8; title_len];
        for (i, byte) in title_bytes.iter_mut().enumerate() {
            *byte = bus.read_byte(ptr + 21 + i as u32);
        }
        let title = decode_mac_roman(&title_bytes);

        // Read positioning constant after the title Pascal string.
        // Macintosh Toolbox Essentials 1992, pp. 4-125 to 4-126
        // The position word follows the title, padded to an even boundary.
        let title_end = 21 + title_len as u32;
        let padded_end = (title_end + 1) & !1;
        let position = if padded_end + 2 <= data_len {
            bus.read_word(ptr + padded_end)
        } else {
            0
        };

        (
            (top, left, bottom, right),
            proc_id,
            visible,
            items_id,
            title,
            position,
        )
    }

    /// Parse an ALRT resource from guest memory.
    /// Returns (bounds, itemsID, stages, position).
    /// Inside Macintosh Volume I, I-425 to I-426
    /// Macintosh Toolbox Essentials 1992, p. 6-150
    fn parse_alrt(
        bus: &MacMemoryBus,
        ptr: u32,
        data_len: u32,
    ) -> ((i16, i16, i16, i16), i16, u16, u16) {
        let bounds = if data_len >= 8 {
            (
                bus.read_word(ptr) as i16,
                bus.read_word(ptr + 2) as i16,
                bus.read_word(ptr + 4) as i16,
                bus.read_word(ptr + 6) as i16,
            )
        } else {
            (0, 0, 0, 0)
        };
        let items_id = if data_len >= 10 {
            bus.read_word(ptr + 8) as i16
        } else {
            0
        };
        let stages = if data_len >= 12 {
            bus.read_word(ptr + 10)
        } else {
            0
        };
        // System 7 compiled ALRT resources append the same positioning
        // constants as DLOG resources after the 12-byte classic template.
        // Macintosh Toolbox Essentials 1992, p. 6-150
        let position = if data_len >= 14 {
            bus.read_word(ptr + 12)
        } else {
            0
        };
        (bounds, items_id, stages, position)
    }

    pub(crate) fn positioned_window_bounds(
        &self,
        bus: &MacMemoryBus,
        mut bounds: (i16, i16, i16, i16),
        position: u16,
        proc_id: i16,
    ) -> (i16, i16, i16, i16) {
        let (_, _, screen_w, screen_h, _) = self.get_screen_params();
        let menu_bar_height = if self.menu_bar_hidden {
            0
        } else {
            bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16
        };
        // The main-screen positioning area is the desktop below the menu bar.
        // Position the complete window structure within it, then translate
        // back to the content bounds consumed by NewWindow. The WIND bounds
        // themselves describe only the content region.
        // Macintosh Toolbox Essentials (1992), pp. 4-29, 4-31, 4-55,
        // and 4-124 to 4-126.
        let main_screen = (menu_bar_height, 0, screen_h, screen_w);
        // Parent-relative placement positions the complete dialog structure,
        // then converts the result back to the content bounds used by NewWindow.
        // Macintosh Toolbox Essentials 1992, pp. 4-125 to 4-126 and 6-62 to 6-63
        let place_structure_at_alert_position =
            |target: (i16, i16, i16, i16)| -> (i16, i16, i16, i16) {
                let structure = self.window_structure_global_rect_for_proc(bus, bounds, proc_id);
                let structure_w = structure.3 - structure.1;
                let structure_h = structure.2 - structure.0;
                let target_w = target.3 - target.1;
                let target_h = target.2 - target.0;
                let new_structure_left = target.1 + (target_w - structure_w) / 2;
                let new_structure_top = target.0 + (target_h - structure_h) / 5;
                let new_left = new_structure_left + (bounds.1 - structure.1);
                let new_top = new_structure_top + (bounds.0 - structure.0);
                (
                    new_top,
                    new_left,
                    new_top + (bounds.2 - bounds.0),
                    new_left + (bounds.3 - bounds.1),
                )
            };
        let center_structure = |target: (i16, i16, i16, i16)| -> (i16, i16, i16, i16) {
            let structure = self.window_structure_global_rect_for_proc(bus, bounds, proc_id);
            let structure_w = structure.3 - structure.1;
            let structure_h = structure.2 - structure.0;
            let target_w = target.3 - target.1;
            let target_h = target.2 - target.0;
            let new_structure_left = target.1 + (target_w - structure_w) / 2;
            let new_structure_top = target.0 + (target_h - structure_h) / 2;
            let new_left = new_structure_left + (bounds.1 - structure.1);
            let new_top = new_structure_top + (bounds.0 - structure.0);
            (
                new_top,
                new_left,
                new_top + (bounds.2 - bounds.0),
                new_left + (bounds.3 - bounds.1),
            )
        };
        match position {
            // alertPositionMainScreen / ParentWindowScreen
            // Macintosh Toolbox Essentials 1992, pp. 4-125 to 4-126
            0x300A | 0x700A => {
                bounds = place_structure_at_alert_position(main_screen);
            }
            // alertPositionParentWindow uses the content rectangle of the
            // window in which the user was last working.
            // Macintosh Toolbox Essentials 1992, pp. 4-125 to 4-126
            0xB00A => {
                let parent = self.front_window_for_trap(bus);
                let target = (parent != 0)
                    .then(|| self.window_content_global_rect(bus, parent))
                    .flatten()
                    .unwrap_or(main_screen);
                bounds = place_structure_at_alert_position(target);
            }
            // centerMainScreen / centerParentWindowScreen. Systemless models a
            // single screen, so both use the main-screen desktop rectangle.
            0x280A | 0x680A => {
                bounds = center_structure(main_screen);
            }
            // centerParentWindow uses the content rectangle of the window in
            // which the user was last working.
            0xA80A => {
                let parent = self.front_window_for_trap(bus);
                let target = (parent != 0)
                    .then(|| self.window_content_global_rect(bus, parent))
                    .flatten()
                    .unwrap_or(main_screen);
                bounds = center_structure(target);
            }
            // Staggering is retained as the existing single-window fallback
            // until the Window Manager tracks per-screen stagger slots.
            0x380A => {
                bounds = center_structure(main_screen);
            }
            _ => {} // noAutoCenter (0x0000) or unknown: use raw bounds
        }
        bounds
    }

    fn alert_stage_default_item(stages: u16, stage_word: u16) -> (u32, u32, Option<i16>) {
        let stage_idx = (stage_word as u32).min(3);
        // Each stage occupies 4 bits, stage 1 in the low nibble.
        let nibble = ((stages as u32) >> (stage_idx * 4)) & 0xF;
        let box_drawn = (nibble & 0x04) != 0;
        let default_item = if (nibble & 0x08) == 0 { 1 } else { 2 };
        (stage_idx, nibble, box_drawn.then_some(default_item))
    }

    fn begin_interactive_alert<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        sp: u32,
        alert_id: i16,
        _filter_proc: u32,
        bounds: (i16, i16, i16, i16),
        items_id: i16,
        position: u16,
        default_item: i16,
    ) -> bool {
        let Some((_, ditl_data)) = self.find_or_load_resource_any(bus, *b"DITL", items_id) else {
            return false;
        };
        let ditl_len = bus.get_alloc_size(ditl_data).unwrap_or(0);
        let items = Self::parse_ditl(bus, ditl_data, ditl_len);

        let items_handle = {
            let handle = bus.alloc(4);
            bus.write_long(handle, ditl_data);
            Self::duplicate_handle_data(bus, handle)
        };
        let bounds = self.positioned_window_bounds(bus, bounds, position, 1);
        let dialog_ptr = self.finish_dialog_creation(
            bus,
            cpu,
            0,
            bounds,
            "",
            true,
            1,
            false,
            alert_id as u32,
            items_handle,
            items.clone(),
            None,
            None,
        );
        if dialog_ptr == 0 {
            return false;
        }
        bus.write_word(dialog_ptr + 168, default_item as u16);

        let saved_pixels = self
            .dialog_saved_pixels
            .get(&dialog_ptr)
            .cloned()
            .unwrap_or_else(|| self.save_dialog_pixels(bus, bounds));
        let rendered_pixels = self.save_dialog_pixels(bus, bounds);
        bus.write_word(sp + 6, 0);
        self.dialog_modal_entered.insert(dialog_ptr);
        self.dialog_tracking = Some(DialogTrackingState {
            dialog_ptr,
            bounds,
            title: String::new(),
            proc_id: 1,
            items,
            default_item,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels,
            // Alert is a Pascal FUNCTION with a 6-byte argument frame
            // (filterProc + alertID) and a 2-byte result slot at SP+6. The
            // shared dialog completion code writes A7 as stack_ptr + 8, so
            // store SP-2 for alerts.
            stack_ptr: sp.wrapping_sub(2),
            item_hit_ptr: sp + 6,
            rendered_pixels,
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });
        true
    }

    fn finish_interactive_alert<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        hit: i16,
    ) {
        let Some(saved) = self.dialog_tracking.take() else {
            return;
        };
        if saved.item_hit_ptr != 0 {
            bus.write_word(saved.item_hit_ptr, hit as u16);
        }
        let stack_after = saved.stack_ptr + 8;
        let dialog_ptr = saved.dialog_ptr;
        self.dialog_saved_pixels
            .insert(dialog_ptr, saved.saved_pixels.clone());
        self.close_dialog_window(bus, cpu, dialog_ptr, true);
        cpu.write_reg(Register::A7, stack_after);
    }

    fn handle_interactive_alert_refire<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) {
        if self.dialog_tracking.is_none() {
            return;
        }

        if self
            .dialog_tracking
            .as_ref()
            .is_some_and(|tracking| tracking.active_button.is_some())
        {
            self.handle_dialog_button_tracking(bus);
            return;
        }

        if self
            .dialog_tracking
            .as_ref()
            .is_some_and(|tracking| tracking.flash_remaining > 0)
        {
            let (remaining, button_draw, finished_hit) = {
                let t = self.dialog_tracking.as_mut().unwrap();
                if t.flash_delay > 0 {
                    t.flash_delay -= 1;
                    return;
                }
                t.flash_remaining -= 1;
                t.flash_delay = 3;
                let remaining = t.flash_remaining;
                let flash_item = t.flash_item;
                let button_draw =
                    if remaining > 0 && flash_item > 0 && (flash_item as usize) <= t.items.len() {
                        let item = &t.items[(flash_item - 1) as usize];
                        Some((
                            Self::dialog_item_screen_rect(t.bounds, item.rect),
                            item.text.clone(),
                            flash_item == t.default_item,
                            remaining % 2 == 0,
                        ))
                    } else {
                        None
                    };
                (remaining, button_draw, flash_item)
            };

            if let Some((screen_rect, title, is_default, highlighted)) = button_draw {
                self.draw_dialog_button_highlight_state(
                    bus,
                    screen_rect,
                    &title,
                    is_default,
                    highlighted,
                );
            }
            if remaining == 0 {
                self.finish_interactive_alert(cpu, bus, finished_hit);
            }
            return;
        }

        let event = if !self.event_queue.is_empty() {
            let mut event = None;
            while let Some(e) = self.event_queue.pop_front() {
                match e.what {
                    1 | 2 | 3 | 6 => {
                        event = Some(e);
                        break;
                    }
                    _ => {}
                }
            }
            event
        } else {
            None
        };

        let Some(event) = event else {
            return;
        };

        match event.what {
            1 => {
                let (dialog_ptr, bounds, items, default_item) = {
                    let tracking = self.dialog_tracking.as_ref().unwrap();
                    (
                        tracking.dialog_ptr,
                        tracking.bounds,
                        tracking.items.clone(),
                        tracking.default_item,
                    )
                };
                let mut hit = self.dialog_item_hit_test(
                    bus,
                    &items,
                    bounds,
                    event.where_v,
                    event.where_h,
                    &self.dialog_popup_original_rects,
                    dialog_ptr,
                );
                if hit <= 0 {
                    hit =
                        Self::dialog_button_hit_test(&items, bounds, event.where_v, event.where_h);
                }
                if hit <= 0 {
                    return;
                }
                let item = &items[(hit - 1) as usize];
                let base_type = item.item_type & 0x7F;
                let is_disabled = (item.item_type & 0x80) != 0;
                if is_disabled {
                    return;
                }
                // Alert returns the number of any enabled item selected by
                // the user. Push buttons use the standard pressed-button
                // tracking affordance; other enabled items terminate the
                // alert directly (Inside Macintosh Volume I, I-418).
                if base_type != 4 {
                    self.finish_interactive_alert(cpu, bus, hit);
                    return;
                }
                let rect = Self::dialog_item_screen_rect(bounds, item.rect);
                let is_default = hit == default_item;
                self.draw_dialog_button_highlight_state(bus, rect, &item.text, is_default, true);
                if self.input_state.mouse_button {
                    let tracking = self.dialog_tracking.as_mut().unwrap();
                    tracking.active_button = Some(super::dispatch::DialogButtonTrackingState {
                        mouse_down: event.clone(),
                        item_no: hit,
                        rect: item.rect,
                        title: item.text.clone(),
                        is_default,
                        highlighted: true,
                    });
                } else {
                    self.start_dialog_button_flash(
                        bus, bounds, hit, item.rect, &item.text, is_default, true,
                    );
                }
            }
            3 => {
                let (default_item, cancel_item) = self
                    .dialog_tracking
                    .as_ref()
                    .map(|tracking| (tracking.default_item, tracking.cancel_item))
                    .unwrap_or((0, 0));
                let key_code = (event.message >> 8) as u16;
                let char_code = (event.message & 0xFF) as u8;
                let hit = if char_code == b'\r' || char_code == 0x03 {
                    default_item
                } else if char_code == 0x1B || key_code == 0x35 {
                    cancel_item
                } else {
                    0
                };
                if hit > 0 {
                    self.finish_interactive_alert(cpu, bus, hit);
                }
            }
            6 => {
                let Some(tracking) = self.dialog_tracking.as_ref() else {
                    return;
                };
                let dialog_ptr = tracking.dialog_ptr;
                let bounds = tracking.bounds;
                let items = tracking.items.clone();
                let default_item = tracking.default_item;
                self.begin_update_window(bus, dialog_ptr);
                self.set_current_port_state(bus, cpu, dialog_ptr, None);
                self.redraw_standard_dialog_items(
                    bus,
                    bounds,
                    &items,
                    default_item,
                    "",
                    0,
                    dialog_ptr,
                );
                let rendered = self.save_dialog_pixels(bus, bounds);
                if let Some(tracking) = self.dialog_tracking.as_mut() {
                    tracking.rendered_pixels = rendered;
                    tracking.rendered_pixels_final = true;
                }
                self.end_update_window(bus, dialog_ptr);
            }
            _ => {}
        }
    }

    /// Parse a DITL resource from guest memory into a list of DialogItems.
    /// Inside Macintosh Volume I, I-439
    fn parse_ditl(bus: &MacMemoryBus, ptr: u32, data_len: u32) -> Vec<DialogItem> {
        if data_len < 2 {
            return Vec::new();
        }
        let max_index = bus.read_word(ptr) as i16; // number of items minus 1
        let count = if max_index < 0 {
            0
        } else {
            max_index as usize + 1
        };
        let mut items = Vec::with_capacity(count);
        let mut offset = 2u32; // skip dlgMaxIndex

        for _ in 0..count {
            if offset + 14 > data_len {
                break;
            }
            // Read 4-byte handle/procPtr field.
            // For userItem types, the game may write a procedure pointer here
            // via SetDItem or direct memory manipulation.
            // Inside Macintosh Volume I, I-427
            let item_handle = bus.read_long(ptr + offset);
            offset += 4;
            // Read display rectangle
            let top = bus.read_word(ptr + offset) as i16;
            let left = bus.read_word(ptr + offset + 2) as i16;
            let bottom = bus.read_word(ptr + offset + 4) as i16;
            let right = bus.read_word(ptr + offset + 6) as i16;
            offset += 8;
            // Read type byte and data length
            let item_type = bus.read_byte(ptr + offset);
            let data_len_byte = bus.read_byte(ptr + offset + 1);
            offset += 2;

            let base_type = item_type & 0x7F; // strip itemDisable bit

            let mut text = String::new();
            let mut resource_id: i16 = 0;
            let remaining = data_len - offset;
            let Some(payload_len) =
                Self::ditl_item_payload_len(base_type, data_len_byte, remaining)
            else {
                break;
            };
            if matches!(base_type, 7 | 32 | 64) {
                resource_id = bus.read_word(ptr + offset) as i16;
            }

            let padded = (payload_len + 1) & !1;
            if padded > remaining {
                break;
            }

            if payload_len > 0 {
                match base_type {
                    // button (4), checkbox (5), radio (6), statText (8),
                    // editText (16): title/text data. IM:I I-427.
                    4 | 5 | 6 | 8 | 16 => {
                        let bytes = bus.read_bytes(ptr + offset, payload_len as usize);
                        text = decode_mac_roman(&bytes);
                    }
                    _ => {}
                }
            }

            // Advance past data, padded to even boundary
            offset += padded;

            // For userItems, itmhand is a ProcPtr, not a relocatable handle.
            // Dialog Manager preserves it in the duplicated DITL and calls it
            // during update/redraw. Some apps replace it later via SetDItem or
            // by writing the duplicated DITL directly.
            // Inside Macintosh Volume I, I-405, I-426 to I-427.
            let proc_ptr = if base_type == 0 { item_handle } else { 0 };

            items.push(DialogItem {
                item_type,
                rect: (top, left, bottom, right),
                text,
                resource_id,
                proc_ptr,
                sel_start: 0,
                sel_end: 0,
            });
        }
        items
    }

    fn ditl_item_payload_len(base_type: u8, data_len_byte: u8, remaining: u32) -> Option<u32> {
        let payload_len = match base_type {
            // The compiled item record stores a length byte for every item
            // type. User items do not interpret their payload, but the Dialog
            // Manager must still skip it to locate the next record.
            // Inside Macintosh Volume I, I-404 to I-405 and I-427.
            0 => u32::from(data_len_byte),
            // Help items use the byte after itmtype as a sized payload.
            // Macintosh Toolbox Essentials 1992, p. 6-154
            1 => u32::from(data_len_byte),
            // icon, picture, resCtrl: 2-byte resource ID. IM:I I-427
            // describes the byte after itmtype as length=2; MTE 1992
            // p. 6-153 documents the same compiled records as a
            // reserved byte plus the two-byte resource ID. Accept both.
            7 | 32 | 64 => {
                if remaining < 2 {
                    return None;
                }
                if data_len_byte >= 2 {
                    u32::from(data_len_byte)
                } else {
                    2
                }
            }
            _ => u32::from(data_len_byte),
        };
        Some(payload_len)
    }

    /// Re-read userItem proc pointers from the DITL data in guest memory.
    /// The game may write proc pointers directly to the DITL handle data
    /// after GetNewDialog returns, bypassing SetDItem.
    /// Inside Macintosh Volume I, I-427
    fn refresh_ditl_proc_ptrs(bus: &MacMemoryBus, dialog_ptr: u32, items: &mut [DialogItem]) {
        // Read the items handle from the DialogRecord (offset 156)
        let items_handle = bus.read_long(dialog_ptr + 156);
        if items_handle == 0 {
            return;
        }
        let ditl_ptr = bus.read_long(items_handle);
        if ditl_ptr == 0 {
            return;
        }

        // Walk the DITL data structure, reading the 4-byte itmhand field
        // for each item. Format: 2-byte count-1, then per item:
        //   4 bytes: itmhand (handle or proc pointer)
        //   8 bytes: itmr (Rect)
        //   1 byte: itmtype
        //   1 byte: itmlen
        //   itmlen bytes: data (padded to even)
        let mut offset = 2u32; // skip count word
        for item in items.iter_mut() {
            let handle = bus.read_long(ditl_ptr + offset);
            offset += 4; // itmhand
            offset += 8; // itmr (Rect)
            let _item_type = bus.read_byte(ditl_ptr + offset);
            let data_len = bus.read_byte(ditl_ptr + offset + 1) as u32;
            offset += 2; // itmtype + itmlen
            let padded = (data_len + 1) & !1;
            offset += padded;

            let base_type = item.item_type & 0x7F;
            if base_type == 0 {
                item.proc_ptr = handle;
            }
        }
    }

    fn duplicate_handle_data(bus: &mut MacMemoryBus, handle: u32) -> u32 {
        if handle == 0 {
            return 0;
        }

        let data_ptr = bus.read_long(handle);
        let new_handle = bus.alloc(4);
        if data_ptr == 0 {
            bus.write_long(new_handle, 0);
            return new_handle;
        }

        let size = bus.get_alloc_size(data_ptr).unwrap_or(0);
        let new_data_ptr = bus.alloc(size);
        for offset in 0..size {
            bus.write_byte(new_data_ptr + offset, bus.read_byte(data_ptr + offset));
        }
        bus.write_long(new_handle, new_data_ptr);
        new_handle
    }

    fn dispose_dialog_handle_storage(&mut self, bus: &mut MacMemoryBus, handle: u32) {
        if handle == 0 {
            return;
        }

        let resource_backing = self.loaded_handles.get(&handle).copied();
        self.forget_resource_handle_index_for_handle(handle);
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.detached_handles.remove(&handle);
            resource_manager.loaded_handles.remove(&handle);
            resource_manager.resource_handle_files.remove(&handle);
            resource_manager.detached_handle_files.remove(&handle);
        });
        self.remove_handle_state_bits(handle);

        let data_ptr = bus.read_long(handle);
        if resource_backing.is_none() {
            bus.free(data_ptr);
        }
        bus.free(handle);
    }

    fn dispose_dialog_te_storage(&mut self, bus: &mut MacMemoryBus, te_handle: u32) {
        if te_handle == 0 {
            return;
        }

        let te_ptr = bus.read_long(te_handle);
        if te_ptr != 0 {
            let h_text = bus.read_long(te_ptr + Self::TE_HTEXT_OFFSET);
            self.dispose_dialog_handle_storage(bus, h_text);

            if Self::te_is_styled_record(bus, te_ptr) {
                let style_handle = bus.read_long(te_ptr + Self::TE_TX_FONT_OFFSET);
                self.dispose_dialog_handle_storage(bus, style_handle);
            }
            bus.free(te_ptr);
        }
        bus.free(te_handle);
        self.textedit_states.remove(&te_handle);
    }

    fn dispose_dialog_control_storage(&mut self, bus: &mut MacMemoryBus, ctrl_handle: u32) {
        if ctrl_handle == 0 {
            return;
        }

        let ctrl_ptr = bus.read_long(ctrl_handle);
        if ctrl_ptr != 0 {
            let owner = bus.read_long(ctrl_ptr + 4);
            if owner != 0 {
                let mut prev_handle = 0u32;
                let mut cur_handle = bus.read_long(owner + 140);
                while cur_handle != 0 {
                    let cur_ptr = bus.read_long(cur_handle);
                    if cur_ptr == 0 {
                        break;
                    }
                    if cur_handle == ctrl_handle {
                        let next = bus.read_long(cur_ptr);
                        if prev_handle == 0 {
                            bus.write_long(owner + 140, next);
                        } else {
                            let prev_ptr = bus.read_long(prev_handle);
                            if prev_ptr != 0 {
                                bus.write_long(prev_ptr, next);
                            }
                        }
                        break;
                    }
                    prev_handle = cur_handle;
                    cur_handle = bus.read_long(cur_ptr);
                }
            }
            self.release_control_aux_record(bus, ctrl_handle);
            self.control_manager.remove_pointer(ctrl_ptr);
            bus.free(ctrl_ptr);
        }
        bus.free(ctrl_handle);
    }

    fn dialog_item_storage_handles_from_ditl(
        bus: &MacMemoryBus,
        dialog_ptr: u32,
    ) -> Vec<(u8, u32)> {
        let items_handle = bus.read_long(dialog_ptr + 156);
        if items_handle == 0 {
            return Vec::new();
        }
        let ditl_ptr = bus.read_long(items_handle);
        if ditl_ptr == 0 {
            return Vec::new();
        }
        let data_len = bus.get_alloc_size(ditl_ptr).unwrap_or(0);
        if data_len < 2 {
            return Vec::new();
        }

        let max_index = bus.read_word(ditl_ptr) as i16;
        if max_index < 0 {
            return Vec::new();
        }

        let mut handles = Vec::new();
        let mut offset = 2u32;
        for _ in 0..=max_index {
            if offset + 14 > data_len {
                break;
            }

            let item_handle = bus.read_long(ditl_ptr + offset);
            offset += 12;
            let item_type = bus.read_byte(ditl_ptr + offset);
            let data_len_byte = bus.read_byte(ditl_ptr + offset + 1);
            offset += 2;

            let base_type = item_type & 0x7F;
            handles.push((base_type, item_handle));

            let payload_len = match base_type {
                0 => 0,
                7 | 32 | 64 if data_len_byte < 2 => 2,
                _ => u32::from(data_len_byte),
            };
            let padded = (payload_len + 1) & !1;
            if padded > data_len.saturating_sub(offset) {
                break;
            }
            offset += padded;
        }

        handles
    }

    fn clear_dialog_scoped_item_state(&mut self, dialog_ptr: u32) {
        self.dialogs_drawn_by_app.remove(&dialog_ptr);
        self.dialog_user_item_port_states.remove(&dialog_ptr);
        self.dialog_items.remove(&dialog_ptr);
        self.dialog_item_handles
            .retain(|_, (dlg, _)| *dlg != dialog_ptr);
        self.dialog_control_handles
            .retain(|_, (dlg, _)| *dlg != dialog_ptr);
        self.dialog_control_values
            .retain(|(dlg, _), _| *dlg != dialog_ptr);
        self.dialog_edit_text_modified_items
            .retain(|(dlg, _)| *dlg != dialog_ptr);
        self.hidden_dialog_item_rects
            .retain(|(dlg, _), _| *dlg != dialog_ptr);
        self.dialog_item_popup_menus
            .retain(|(dlg, _), _| *dlg != dialog_ptr);
        self.dialog_popup_original_rects
            .retain(|(dlg, _), _| *dlg != dialog_ptr);
        self.dialog_popup_candidate_items
            .retain(|(dlg, _)| *dlg != dialog_ptr);
        self.modeless_dialog_draw_proc_queue
            .retain(|(dlg, _, _)| *dlg != dialog_ptr);
        self.modeless_dialog_cdef_draw_queue
            .retain(|dlg| *dlg != dialog_ptr);
        if self
            .pending_dialog_popup_menu
            .is_some_and(|pending| pending.dialog_ptr == dialog_ptr)
        {
            self.pending_dialog_popup_menu = None;
        }
        self.dialog_cancel_items.remove(&dialog_ptr);
    }

    fn dispose_dialog_owned_items(&mut self, bus: &mut MacMemoryBus, dialog_ptr: u32) {
        let can_read_dialog_record =
            self.dialog_items.contains_key(&dialog_ptr) || self.window_list.contains(&dialog_ptr);
        let mut text_handles = Vec::new();
        let mut control_handles = Vec::new();

        if can_read_dialog_record {
            for (base_type, handle) in Self::dialog_item_storage_handles_from_ditl(bus, dialog_ptr)
            {
                match base_type {
                    8 | 16 => text_handles.push(handle),
                    4..=7 => control_handles.push(handle),
                    _ => {}
                }
            }
        }

        text_handles.extend(
            self.dialog_item_handles
                .iter()
                .filter_map(|(&handle, &(dlg, _))| {
                    if dlg == dialog_ptr {
                        Some(handle)
                    } else {
                        None
                    }
                }),
        );
        control_handles.extend(self.dialog_control_handles.iter().filter_map(
            |(&handle, &(dlg, _))| {
                if dlg == dialog_ptr {
                    Some(handle)
                } else {
                    None
                }
            },
        ));

        text_handles.sort_unstable();
        text_handles.dedup();
        control_handles.sort_unstable();
        control_handles.dedup();

        for handle in text_handles {
            self.dispose_dialog_handle_storage(bus, handle);
        }
        for handle in control_handles {
            self.dispose_dialog_control_storage(bus, handle);
        }

        if can_read_dialog_record {
            let text_h = bus.read_long(dialog_ptr + 160);
            self.dispose_dialog_te_storage(bus, text_h);
            bus.write_long(dialog_ptr + 160, 0);
        }

        self.clear_dialog_scoped_item_state(dialog_ptr);
    }

    fn dispose_dialog_record_and_item_list(&mut self, bus: &mut MacMemoryBus, dialog_ptr: u32) {
        if !self.dialog_items.contains_key(&dialog_ptr) && !self.window_list.contains(&dialog_ptr) {
            return;
        }

        let items_handle = bus.read_long(dialog_ptr + 156);
        self.dispose_dialog_handle_storage(bus, items_handle);
        bus.free(dialog_ptr);
    }

    pub(crate) fn dialog_screen_bounds(
        bus: &MacMemoryBus,
        dialog_ptr: u32,
    ) -> (i16, i16, i16, i16) {
        let port_height =
            bus.read_word(dialog_ptr + 20) as i16 - bus.read_word(dialog_ptr + 16) as i16;
        let port_width =
            bus.read_word(dialog_ptr + 22) as i16 - bus.read_word(dialog_ptr + 18) as i16;

        let port_version = bus.read_word(dialog_ptr + 6);
        let is_cgraf = (port_version & 0xC000) == 0xC000;
        let (top, left) = if is_cgraf {
            let pixmap_handle = bus.read_long(dialog_ptr + 2);
            let pixmap_ptr = if pixmap_handle != 0 {
                bus.read_long(pixmap_handle)
            } else {
                0
            };
            if pixmap_ptr != 0 {
                (
                    -(bus.read_word(pixmap_ptr + 6) as i16),
                    -(bus.read_word(pixmap_ptr + 8) as i16),
                )
            } else {
                (0, 0)
            }
        } else {
            (
                -(bus.read_word(dialog_ptr + 8) as i16),
                -(bus.read_word(dialog_ptr + 10) as i16),
            )
        };

        (top, left, top + port_height, left + port_width)
    }

    fn dialog_edit_state(
        bus: &MacMemoryBus,
        dialog_ptr: u32,
        items: &[DialogItem],
    ) -> (String, i16, i16) {
        let default_item = match bus.read_word(dialog_ptr + 168) as i16 {
            value if value > 0 => value,
            _ => 1,
        };

        let edit_field = bus.read_word(dialog_ptr + 164) as i16;
        let stored_edit_item = if edit_field >= 0 { edit_field + 1 } else { 0 };
        let stored_is_valid = stored_edit_item > 0
            && items
                .get((stored_edit_item - 1) as usize)
                .is_some_and(|item| (item.item_type & 0x7F) == 16 && (item.item_type & 0x80) == 0);
        let edit_item = if stored_is_valid {
            stored_edit_item
        } else {
            items
                .iter()
                .position(|item| (item.item_type & 0x7F) == 16 && (item.item_type & 0x80) == 0)
                .map(|idx| (idx + 1) as i16)
                .unwrap_or(0)
        };
        if edit_item <= 0 {
            return (String::new(), 0, default_item);
        }

        let edit_text = Self::text_item_string_from_handle_if_present(
            bus,
            Self::dialog_item_handle(bus, dialog_ptr, edit_item),
        );
        if let Some(edit_text) = edit_text {
            return (edit_text, edit_item, default_item);
        }

        let fallback = items
            .get((edit_item - 1) as usize)
            .map(|item| item.text.clone())
            .unwrap_or_default();
        (fallback, edit_item, default_item)
    }

    fn sync_tracking_active_edit_item(tracking: &mut DialogTrackingState) {
        let edit_item = tracking.edit_item;
        if edit_item <= 0 {
            return;
        }
        if let Some(item) = tracking.items.get_mut((edit_item - 1) as usize) {
            if (item.item_type & 0x7F) == 16 {
                item.text = tracking.edit_text.clone();
            }
        }
    }

    fn set_tracking_active_edit_selection(
        tracking: &mut DialogTrackingState,
        sel_start: usize,
        sel_end: usize,
    ) {
        let edit_item = tracking.edit_item;
        if edit_item <= 0 {
            return;
        }
        if let Some(item) = tracking.items.get_mut((edit_item - 1) as usize) {
            if (item.item_type & 0x7F) == 16 {
                let text_len = encode_mac_roman_lossy(&tracking.edit_text).len();
                item.sel_start = sel_start.min(text_len).min(i16::MAX as usize) as i16;
                item.sel_end = sel_end.min(text_len).min(i16::MAX as usize) as i16;
            }
        }
    }

    fn textedit_key_result(
        existing: &[u8],
        sel_start: usize,
        sel_end: usize,
        key: u8,
    ) -> (Vec<u8>, usize) {
        let text_len = existing.len();
        let s = sel_start.min(text_len);
        let e = sel_end.min(text_len);
        let (s, e) = if s > e { (e, s) } else { (s, e) };

        if key == 0x08 {
            if s != e {
                let mut merged = Vec::with_capacity(text_len - (e - s));
                merged.extend_from_slice(&existing[..s]);
                merged.extend_from_slice(&existing[e..]);
                return (merged, s);
            }
            if s > 0 {
                let mut merged = Vec::with_capacity(text_len - 1);
                merged.extend_from_slice(&existing[..s - 1]);
                merged.extend_from_slice(&existing[s..]);
                return (merged, s - 1);
            }
            return (existing.to_vec(), s);
        }

        let mut merged = Vec::with_capacity(s + 1 + text_len.saturating_sub(e));
        merged.extend_from_slice(&existing[..s]);
        merged.push(key);
        merged.extend_from_slice(&existing[e..]);
        (merged, s + 1)
    }

    fn apply_dialog_select_key_to_edit_item(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        items: &mut [DialogItem],
        edit_item: i16,
        key: u8,
    ) -> bool {
        if edit_item <= 0 {
            return false;
        }
        let Some(item) = items.get_mut((edit_item - 1) as usize) else {
            return false;
        };
        let base_type = item.item_type & 0x7F;
        let is_disabled = (item.item_type & 0x80) != 0;
        if base_type != 16 || is_disabled {
            return false;
        }

        let item_handle = Self::dialog_item_handle(bus, dialog_ptr, edit_item);
        let existing = Self::text_item_bytes_from_handle_if_present(bus, item_handle)
            .unwrap_or_else(|| encode_mac_roman_lossy(&item.text));
        let text_handle = bus.read_long(dialog_ptr + 160);
        let te_ptr = Self::te_record_ptr(bus, text_handle);
        let (sel_start, sel_end) = if te_ptr != 0 {
            (
                bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET) as usize,
                bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) as usize,
            )
        } else {
            (item.sel_start.max(0) as usize, item.sel_end.max(0) as usize)
        };

        let (updated, insertion_point) =
            Self::textedit_key_result(&existing, sel_start, sel_end, key);
        let clamped_insertion = insertion_point.min(u16::MAX as usize) as u16;
        item.text = decode_mac_roman(&updated);
        item.sel_start = clamped_insertion as i16;
        item.sel_end = clamped_insertion as i16;

        if item_handle != 0 {
            let len = updated.len().min(255);
            let data_ptr = Self::ensure_text_handle_size(bus, item_handle, len);
            if data_ptr != 0 && len > 0 {
                bus.write_bytes(data_ptr, &updated[..len]);
            }
        }

        if te_ptr != 0 {
            self.te_set_text_contents(bus, text_handle, &updated);
            bus.write_word(te_ptr + Self::TE_SEL_START_OFFSET, clamped_insertion);
            bus.write_word(te_ptr + Self::TE_SEL_END_OFFSET, clamped_insertion);
        }

        true
    }

    fn activate_dialog_edit_item<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        dialog_ptr: u32,
        items: &[DialogItem],
        edit_item: i16,
    ) -> bool {
        if edit_item <= 0 {
            return false;
        }
        let Some(item) = items.get((edit_item - 1) as usize) else {
            return false;
        };
        if (item.item_type & 0x7F) != 16 || (item.item_type & 0x80) != 0 {
            return false;
        }

        bus.write_word(dialog_ptr + 164, (edit_item - 1) as u16);

        let text_handle = bus.read_long(dialog_ptr + 160);
        let te_ptr = Self::te_record_ptr(bus, text_handle);
        if te_ptr == 0 {
            return true;
        }

        let item_handle = Self::dialog_item_handle(bus, dialog_ptr, edit_item);
        let text = Self::text_item_bytes_from_handle_if_present(bus, item_handle)
            .unwrap_or_else(|| encode_mac_roman_lossy(&item.text));
        self.te_set_text_contents(bus, text_handle, &text);

        let text_len = text.len().min(u16::MAX as usize);
        let sel_start = item.sel_start.max(0) as usize;
        let sel_end = item.sel_end.max(0) as usize;
        bus.write_word(
            te_ptr + Self::TE_SEL_START_OFFSET,
            sel_start.min(text_len) as u16,
        );
        bus.write_word(
            te_ptr + Self::TE_SEL_END_OFFSET,
            sel_end.min(text_len) as u16,
        );
        // MTE 1992 p. 6-139 and IM:I I-417: DialogSelect mouse-down in an
        // enabled editText item displays the insertion point or selection.
        // The Dialog Manager shares one TERecord across editText items, so
        // activating an item must make that record active before later null
        // events call TEIdle for caret blinking.
        bus.write_word(te_ptr + Self::TE_ACTIVE_OFFSET, 1);
        bus.write_long(te_ptr + Self::TE_CARET_TIME_OFFSET, self.current_tick());
        bus.write_word(te_ptr + Self::TE_CARET_STATE_OFFSET, 0);
        self.draw_te_contents(cpu, bus, text_handle, true);
        true
    }

    fn dialog_item_selection_range(item: &DialogItem, text_len: usize) -> Option<(usize, usize)> {
        let start = (item.sel_start.max(0) as usize).min(text_len);
        let end = (item.sel_end.max(0) as usize).min(text_len);
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        (start < end).then_some((start, end))
    }

    fn redraw_dialog_text_item(&mut self, bus: &mut MacMemoryBus, dialog_ptr: u32, item_no: i16) {
        if !self.window_visible(bus, dialog_ptr) {
            return;
        }
        let vis_handle = bus.read_long(dialog_ptr + 24);
        if vis_handle != 0 && Self::region_handle_rect(bus, vis_handle).is_none() {
            return;
        }
        let Some(items) = self.dialog_items.get(&dialog_ptr).cloned() else {
            return;
        };
        let Some(item) = items.get((item_no - 1) as usize) else {
            return;
        };
        let base_type = item.item_type & 0x7F;
        if base_type != 8 && base_type != 16 {
            return;
        }

        let bounds = Self::dialog_screen_bounds(bus, dialog_ptr);
        let (top, left, bottom, right) = bounds;
        let (it, il, ib, ir) = item.rect;
        let abs_top = top + it;
        let abs_left = left + il;
        let abs_bottom = top + ib;
        let abs_right = left + ir;
        if abs_top >= bottom || abs_bottom <= top || abs_left >= right || abs_right <= left {
            return;
        }

        let (edit_text, edit_item, default_item) = Self::dialog_edit_state(bus, dialog_ptr, &items);
        self.redraw_standard_dialog_items(
            bus,
            bounds,
            &items,
            default_item,
            &edit_text,
            edit_item,
            dialog_ptr,
        );

        if self.dialog_visible_snapshots.contains_key(&dialog_ptr) {
            let pixels = self.save_dialog_pixels(bus, bounds);
            self.dialog_visible_snapshots
                .insert(dialog_ptr, PersistentDialogSnapshot { bounds, pixels });
        }
        self.capture_gui_frame(
            bus,
            &format!("set_dialog_item_text_{:08X}_{}", dialog_ptr, item_no),
        );
    }

    fn flush_dialog_edit_item_texts(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        items: &[DialogItem],
        active_edit_item: i16,
        active_edit_text: &str,
    ) {
        for (idx, item) in items.iter().enumerate() {
            if (item.item_type & 0x7F) != 16 {
                continue;
            }
            let item_no = (idx + 1) as i16;
            let text = if item_no == active_edit_item {
                active_edit_text
            } else {
                &item.text
            };
            let bytes = encode_mac_roman_lossy(text);
            let len = bytes.len().min(255);
            let item_handle = Self::dialog_item_handle(bus, dialog_ptr, item_no);
            if item_handle != 0 {
                let data_ptr = Self::ensure_text_handle_size(bus, item_handle, len);
                if data_ptr != 0 && len > 0 {
                    bus.write_bytes(data_ptr, &bytes[..len]);
                }
            }
            if let Some(cached_items) = self.dialog_items.get_mut(&dialog_ptr) {
                if idx < cached_items.len() {
                    cached_items[idx].text = text.to_string();
                    cached_items[idx].sel_start = item.sel_start;
                    cached_items[idx].sel_end = item.sel_end;
                }
            }
        }
    }

    fn copy_dialog_item_color_table_resource(
        &mut self,
        bus: &mut MacMemoryBus,
        table_id: i16,
    ) -> Option<u32> {
        let (_, resource_ptr) = self.find_or_load_resource_any(bus, *b"ictb", table_id)?;
        let byte_size = bus.get_alloc_size(resource_ptr)?;
        if byte_size == 0 {
            return None;
        }
        let table_ptr = bus.alloc(byte_size);
        for offset in 0..byte_size {
            bus.write_byte(table_ptr + offset, bus.read_byte(resource_ptr + offset));
        }
        let table_handle = bus.alloc(4);
        bus.write_long(table_handle, table_ptr);
        Some(table_handle)
    }

    fn dialog_item_text_style(
        &self,
        bus: &MacMemoryBus,
        dialog_ptr: u32,
        item_index: usize,
    ) -> Option<DialogItemTextStyle> {
        let table_handle = self.window_dialog_item_color_table(bus, dialog_ptr);
        if table_handle == 0 {
            return None;
        }
        let table_ptr = bus.read_long(table_handle);
        if table_ptr == 0 {
            return None;
        }
        let table_size = bus.get_alloc_size(table_ptr)?;
        let entry_offset = u32::try_from(item_index).ok()?.checked_mul(4)?;
        if entry_offset.checked_add(4)? > table_size {
            return None;
        }

        let flags = bus.read_word(table_ptr + entry_offset);
        let style_offset = u32::from(bus.read_word(table_ptr + entry_offset + 2));
        if flags == 0 && style_offset == 0 {
            return None;
        }
        if style_offset.checked_add(20)? > table_size {
            return None;
        }

        let style_ptr = table_ptr + style_offset;
        let stored_font = bus.read_word(style_ptr) as i16;
        let stored_face = bus.read_word(style_ptr + 2) as u8;
        let stored_size = bus.read_word(style_ptr + 4) as i16;
        let mut size = self.tx_size;
        if flags & 0x0004 != 0 {
            size = stored_size;
        }
        if flags & 0x0010 != 0 {
            size = size.saturating_add(stored_size);
        }

        // A set doFontName bit makes diFont an offset to a Pascal font name.
        // Font-number lookup remains the safe fallback when that optional
        // name cannot be resolved by the HLE Font Manager.
        let font = if flags & 0x0001 != 0 && flags & 0x8000 == 0 {
            stored_font
        } else {
            self.tx_font
        };
        let face = if flags & 0x0002 != 0 {
            stored_face
        } else {
            self.tx_face as u8
        };
        let foreground = (flags & 0x0008 != 0).then(|| {
            [
                bus.read_word(style_ptr + 6),
                bus.read_word(style_ptr + 8),
                bus.read_word(style_ptr + 10),
            ]
        });
        let background = (flags & 0x2000 != 0).then(|| {
            [
                bus.read_word(style_ptr + 12),
                bus.read_word(style_ptr + 14),
                bus.read_word(style_ptr + 16),
            ]
        });
        let mode = if flags & 0x4000 != 0 {
            bus.read_word(style_ptr + 18) as i16
        } else {
            self.tx_mode
        };
        Some(DialogItemTextStyle {
            font,
            face,
            size,
            foreground,
            background,
            mode,
        })
    }

    fn finish_dialog_creation<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        storage_ptr: u32,
        bounds: (i16, i16, i16, i16),
        title: &str,
        visible: bool,
        proc_id: i16,
        go_away_flag: bool,
        ref_con: u32,
        items_handle: u32,
        items: Vec<DialogItem>,
        dialog_color_table: Option<u32>,
        dialog_item_color_table: Option<u32>,
    ) -> u32 {
        let previous_port = *self.current_port;
        let previous_gdevice = *self.current_gdevice;
        let dlg_ptr = if storage_ptr != 0 {
            storage_ptr
        } else {
            // GetNewDialog/NewDialog allocate storage when dStorage is NIL
            // (IM:I I-424, I-412). System 7's Memory Manager zone layout
            // gives manager-owned records stable low-byte placement that
            // some 68K apps accidentally depend on; keep that shape local
            // to Dialog Manager records instead of changing all heap blocks.
            bus.alloc_aligned(170, MANAGER_DIALOG_RECORD_ALIGNMENT)
        };

        self.window_stack.push((
            self.front_window,
            self.window_bounds,
            self.window_proc_id,
            self.window_title.clone(),
        ));

        let screen_base: u32 = bus.read_long(0x0824);
        self.init_cgraf_window(
            bus,
            cpu,
            dlg_ptr,
            screen_base,
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            title,
            proc_id,
            visible,
            false,
            go_away_flag,
            ref_con,
        );
        self.set_current_port_state(bus, cpu, dlg_ptr, None);

        // GetNewDialog associates a matching DCTab before the first visible
        // shell is drawn. This also updates the AuxWin record returned by
        // GetAuxWin and makes the content color the port background color.
        // Macintosh Toolbox Essentials 1992, pp. 6-120 to 6-121.
        if let Some(color_table) = dialog_color_table {
            self.ensure_window_aux_record(bus, dlg_ptr, color_table);
            self.apply_window_color_table(bus, dlg_ptr, color_table);
        }
        if let Some(item_color_table) = dialog_item_color_table {
            self.set_window_dialog_item_color_table(bus, dlg_ptr, item_color_table);
        }

        // DialogRecord starts with a WindowRecord; +108 is windowKind,
        // not the WDEF procID. Dialog boxes and alerts must use
        // dialogKind=2. The WDEF procID is retained in window_proc_ids.
        // Inside Macintosh Volume I, I-407..I-408, I-273; Volume VI, 11-42.
        bus.write_word(dlg_ptr + 108, 2);
        bus.write_long(dlg_ptr + 156, items_handle);

        // SetDAFont / SetDialogFont affect subsequently created dialog and
        // alert grafPorts; assembly callers may set DlgFont directly.
        // Inside Macintosh Volume I, I-412; MTE 1992 p. 6-104.
        let dialog_font = bus.read_word(crate::memory::globals::addr::DLG_FONT) as i16;
        bus.write_word(dlg_ptr + 68, dialog_font as u16);
        self.tx_font = dialog_font;
        if let Some(state) = self.port_draw_states.get_mut(&dlg_ptr) {
            state.tx_font = dialog_font;
        }

        let text_h = Self::allocate_te_handle(bus);
        bus.write_long(dlg_ptr + 160, text_h);
        bus.write_word(dlg_ptr + 164, 0xFFFF); // editField = -1
        bus.write_word(dlg_ptr + 166, 0); // editOpen
        bus.write_word(dlg_ptr + 168, 1); // aDefItem

        self.initialize_dialog_item_handles(bus, dlg_ptr, &items);
        self.dialog_items.insert(dlg_ptr, items.clone());
        // A dialog's screen-backed GrafPort can receive application control
        // drawing even while its window is hidden. Establish the save-under
        // before returning the DialogPtr so those dialog-owned writes cannot
        // become the background later restored by DisposDialog. Draws through
        // other ports keep this snapshot current until disposal.
        // Inside Macintosh Volume I, I-412, I-425.
        self.ensure_dialog_background_saved(bus, dlg_ptr, bounds);

        if visible {
            // NewDialog/GetNewDialog create and display visible dialogs
            // immediately. userItem contents remain app-owned and are skipped
            // until DrawDialog/ModalDialog/update callbacks can run, but the
            // standard shell, controls, text, and resource items must still be
            // visible. Inside Macintosh Volume I, I-405, I-412, I-421.
            let (edit_text, edit_item, default_item) =
                Self::dialog_edit_state(bus, dlg_ptr, &items);
            self.draw_dialog(
                bus,
                bounds,
                proc_id,
                title,
                &items,
                default_item,
                &edit_text,
                edit_item,
                false,
                dlg_ptr,
            );
            if !Self::dialog_is_game_managed(bounds, &items) {
                let pixels = self.save_dialog_pixels(bus, bounds);
                self.dialog_visible_snapshots
                    .insert(dlg_ptr, PersistentDialogSnapshot { bounds, pixels });
            }
            for item in &items {
                if Self::dialog_item_intersects_bounds(bounds, item) {
                    self.validate_window_rect(bus, dlg_ptr, item.rect);
                }
            }
            self.queue_window_update_event(dlg_ptr);
            self.capture_gui_frame(bus, &format!("get_new_dialog_initial_{:08X}", dlg_ptr));
        } else {
            self.dialog_initial_draw_deferred.remove(&dlg_ptr);
        }

        // NewDialog/GetNewDialog create a window but do not make its GrafPort
        // current; callers explicitly use SetPort when they want to draw in
        // the new dialog. Restore the port used while constructing and
        // initially drawing the dialog before returning to the application.
        self.set_current_port_state(bus, cpu, previous_port, Some(previous_gdevice));

        dlg_ptr
    }

    // ========== Dialog Drawing Helpers ==========

    fn packed_row_byte_bounds(
        left: i16,
        right: i16,
        row_bytes: u32,
        pixel_size: u16,
    ) -> Option<(u32, u32, u32)> {
        if !matches!(pixel_size, 1 | 2 | 4) || right <= 0 {
            return None;
        }
        let pixels_per_byte = 8 / u32::from(pixel_size);
        let byte_left = (left.max(0) as u32) / pixels_per_byte;
        let byte_right = (right.max(0) as u32).div_ceil(pixels_per_byte);
        let byte_end = byte_right.min(row_bytes);
        (byte_left < byte_end).then_some((byte_left, byte_end, pixels_per_byte))
    }

    /// Save framebuffer pixels under a dialog region (including shadow).
    /// Supports indexed 1/2/4/8bpp modes.
    pub(crate) fn save_dialog_pixels(
        &self,
        bus: &MacMemoryBus,
        rect: (i16, i16, i16, i16),
    ) -> SavedPixels {
        let (screen_base, row_bytes, _, screen_h, pixel_size) = self.get_screen_params();
        let (save_top, save_left, save_bottom, save_right) = Self::dialog_saved_pixel_rect(rect);
        // Guard against negative y (top < 5) and y >= screen_h. `y as u32`
        // sign-extends a negative i16 to a huge value that overflows when
        // multiplied by row_bytes. Off-screen rows contribute zeros.
        let row_width = (save_right - save_left) as usize;
        let row_count = (save_bottom - save_top) as usize;
        let mut saved = Vec::new();
        let screen_h_i16 = screen_h;
        for y in save_top..save_bottom {
            let y_on_screen = y >= 0 && y < screen_h_i16;
            if pixel_size == 8 {
                let on_screen_row =
                    y_on_screen && save_left >= 0 && (save_right as u32) <= row_bytes;
                if on_screen_row {
                    // Pre-allocate capacity on first iteration.
                    if saved.capacity() == 0 {
                        saved.reserve(row_count * row_width);
                    }
                    let start = saved.len();
                    saved.resize(start + row_width, 0);
                    let row_addr = screen_base + (y as u32) * row_bytes + (save_left as u32);
                    bus.read_bytes_into(row_addr, &mut saved[start..start + row_width]);
                } else if y_on_screen {
                    for x in save_left..save_right {
                        if x < 0 || (x as u32) >= row_bytes {
                            saved.push(0);
                        } else {
                            let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                            saved.push(bus.read_byte(addr));
                        }
                    }
                } else {
                    // Entire row off-screen — pad with zeros.
                    saved.resize(saved.len() + row_width, 0);
                }
            } else if y_on_screen {
                if let Some((byte_left, byte_end, _)) =
                    Self::packed_row_byte_bounds(save_left, save_right, row_bytes, pixel_size)
                {
                    let len = (byte_end - byte_left) as usize;
                    let start = saved.len();
                    saved.resize(start + len, 0);
                    let row_addr = screen_base + (y as u32) * row_bytes + byte_left;
                    bus.read_bytes_into(row_addr, &mut saved[start..start + len]);
                }
            }
            // Packed off-screen row: silently produces nothing — the
            // byte_left < bx_end check + row_count-based saved.len tracking
            // tolerates short rows.
        }
        let mut saved: SavedPixels = saved.into();
        if pixel_size == 8 {
            let (t, l, b, r) = (save_top, save_left, save_bottom, save_right);
            for y in t.max(0)..b.min(screen_h) {
                let x0 = l.max(0) as u32;
                let len = (r.max(0) as u32).min(row_bytes).saturating_sub(x0) as usize;
                let offset = (y - t) as usize * row_width + (x0 as i32 - i32::from(l)) as usize;
                bus.capture_pixel_detail(
                    &mut saved,
                    offset,
                    screen_base + y as u32 * row_bytes + x0,
                    len,
                );
            }
        }
        saved
    }

    pub(crate) fn ensure_dialog_background_saved(
        &mut self,
        bus: &MacMemoryBus,
        dialog_ptr: u32,
        bounds: (i16, i16, i16, i16),
    ) {
        if dialog_ptr == 0
            || !self.dialog_items.contains_key(&dialog_ptr)
            || self.dialog_saved_pixels.contains_key(&dialog_ptr)
        {
            return;
        }
        let background = self.save_dialog_pixels(bus, bounds);
        self.dialog_saved_pixels.insert(dialog_ptr, background);
    }

    pub(crate) fn ensure_dialog_background_saved_for_screen_port(
        &mut self,
        bus: &MacMemoryBus,
        port: u32,
    ) {
        if port == 0 || !self.dialog_items.contains_key(&port) {
            return;
        }
        let bounds = Self::dialog_screen_bounds(bus, port);
        self.ensure_dialog_background_saved(bus, port, bounds);
    }

    pub(super) fn dialog_saved_pixel_rect(rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
        let (top, left, bottom, right) = rect;
        let margin = Self::DBOX_FRAME_MARGIN;
        (top - margin, left - margin, bottom + margin, right + margin)
    }

    pub(crate) fn refresh_dialog_saved_pixels_after_screen_draw(
        &mut self,
        bus: &MacMemoryBus,
        drawing_port: u32,
        screen_rect: (i16, i16, i16, i16),
    ) {
        if screen_rect.0 >= screen_rect.2 || screen_rect.1 >= screen_rect.3 {
            return;
        }
        let mut dialogs: Vec<(u32, (i16, i16, i16, i16))> = self
            .dialog_visible_snapshots
            .iter()
            .map(|(&dialog_ptr, snapshot)| (dialog_ptr, snapshot.bounds))
            .collect();
        if let Some(tracking) = self.dialog_tracking.as_ref() {
            if !dialogs
                .iter()
                .any(|(dialog_ptr, _)| *dialog_ptr == tracking.dialog_ptr)
            {
                dialogs.push((tracking.dialog_ptr, tracking.bounds));
            }
        }
        for &dialog_ptr in self.dialog_saved_pixels.keys() {
            if !dialogs
                .iter()
                .any(|(candidate, _)| *candidate == dialog_ptr)
                && self.dialog_items.contains_key(&dialog_ptr)
            {
                dialogs.push((dialog_ptr, Self::dialog_screen_bounds(bus, dialog_ptr)));
            }
        }
        for (dialog_ptr, bounds) in dialogs {
            if dialog_ptr == drawing_port
                || self.active_modeless_dialog_draw_proc == Some(dialog_ptr)
            {
                continue;
            }
            self.refresh_single_dialog_saved_pixels_after_screen_draw(
                bus,
                dialog_ptr,
                bounds,
                screen_rect,
            );
        }
    }

    fn refresh_single_dialog_saved_pixels_after_screen_draw(
        &mut self,
        bus: &MacMemoryBus,
        dialog_ptr: u32,
        bounds: (i16, i16, i16, i16),
        screen_rect: (i16, i16, i16, i16),
    ) {
        let screen_params = self.get_screen_params();
        let Some(saved) = self.dialog_saved_pixels.get_mut(&dialog_ptr) else {
            return;
        };
        Self::refresh_saved_pixel_buffer_after_screen_draw(
            bus,
            screen_params,
            bounds,
            screen_rect,
            saved,
        );
    }

    fn refresh_saved_pixel_buffer_after_screen_draw(
        bus: &MacMemoryBus,
        screen_params: (u32, u32, i16, i16, u16),
        bounds: (i16, i16, i16, i16),
        screen_rect: (i16, i16, i16, i16),
        saved: &mut SavedPixels,
    ) {
        let save_rect = Self::dialog_saved_pixel_rect(bounds);
        let Some(intersection) = Self::rect_intersection(save_rect, screen_rect) else {
            return;
        };

        let (screen_base, row_bytes, screen_w, screen_h, pixel_size) = screen_params;
        let (save_top, save_left, save_bottom, save_right) = save_rect;
        let row_width = save_right.saturating_sub(save_left) as usize;
        let row_count = save_bottom.saturating_sub(save_top) as usize;
        if row_width == 0 || row_count == 0 {
            return;
        }

        match pixel_size {
            8 => {
                let expected = row_width.saturating_mul(row_count);
                if saved.len() < expected {
                    return;
                }
                let top = intersection.0.max(0);
                let left = intersection.1.max(0);
                let bottom = intersection.2.min(screen_h);
                let right = intersection.3.min(screen_w).min(row_bytes as i16);
                if top >= bottom || left >= right {
                    return;
                }
                for y in top..bottom {
                    let saved_offset =
                        (y - save_top) as usize * row_width + (left - save_left) as usize;
                    let len = (right - left) as usize;
                    let row_addr = screen_base + (y as u32) * row_bytes + (left as u32);
                    let row = bus.read_bytes(row_addr, len);
                    saved.replace_range(saved_offset, &row);
                    bus.capture_pixel_detail(saved, saved_offset, row_addr, len);
                }
            }
            1 | 2 | 4 => {
                let Some((byte_left, byte_end, pixels_per_byte)) =
                    Self::packed_row_byte_bounds(save_left, save_right, row_bytes, pixel_size)
                else {
                    return;
                };
                let row_len = (byte_end - byte_left) as usize;
                let first_saved_row = save_top.max(0);
                let top = intersection.0.max(0);
                let left = intersection.1.max(0);
                let bottom = intersection.2.min(screen_h);
                let right = intersection
                    .3
                    .min(screen_w)
                    .min((row_bytes.saturating_mul(8)) as i16);
                if top >= bottom || left >= right {
                    return;
                }
                for y in top..bottom {
                    let row_offset = (y - first_saved_row) as usize * row_len;
                    if row_offset + row_len > saved.len() {
                        return;
                    }
                    for x in left..right {
                        let byte_x = (x as u32) / pixels_per_byte;
                        if byte_x < byte_left || byte_x >= byte_end {
                            continue;
                        }
                        let pixel_slot = (x as u32) % pixels_per_byte;
                        let shift = 8 - u32::from(pixel_size) - pixel_slot * u32::from(pixel_size);
                        let mask = ((1u8 << pixel_size) - 1) << shift;
                        let screen_byte =
                            bus.read_byte(screen_base + (y as u32) * row_bytes + byte_x);
                        let saved_idx = row_offset + (byte_x - byte_left) as usize;
                        saved[saved_idx] = (saved[saved_idx] & !mask) | (screen_byte & mask);
                    }
                }
            }
            _ => {}
        }
    }

    /// Restore visible dialog content without replaying the saved-under margin
    /// over its current Window Manager frame. The structure and content regions
    /// have separate owners (Inside Macintosh: Macintosh Toolbox Essentials,
    /// Window Manager, "MyWindow" / "Drawing the Window Frame").
    pub(crate) fn restore_dialog_content_pixels(
        &self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
        saved: &SavedPixels,
    ) {
        // Keep the existing unchanged-snapshot fast path, but distinguish a
        // content-only replay from a later full saved-under restoration.
        if bus.dialog_snapshot_is_current(saved, bounds, true) {
            return;
        }
        let (base, row_bytes, width, height, depth) = self.get_screen_params();
        let save_rect = Self::dialog_saved_pixel_rect(bounds);
        if depth == 8 {
            let Some((top, left, bottom, right)) =
                Self::rect_intersection(bounds, (0, 0, height, width))
            else {
                return;
            };
            let saved_stride = (save_rect.3 - save_rect.1) as usize;
            let len = (right - left) as usize;
            for y in top..bottom {
                let offset = (y - save_rect.0) as usize * saved_stride
                    + (left - save_rect.1) as usize;
                if offset + len <= saved.len() {
                    bus.restore_saved_pixels(
                        base + y as u32 * row_bytes + left as u32,
                        saved,
                        offset,
                        len,
                    );
                }
            }
        } else if let Some((byte_left, byte_end, pixels_per_byte)) =
            Self::packed_row_byte_bounds(save_rect.1, save_rect.3, row_bytes, depth)
        {
            // save_dialog_pixels stores only on-screen rows in packed modes.
            // Mask the first and last bytes: they may also contain frame pixels.
            let saved_stride = (byte_end - byte_left) as usize;
            let pixel_mask = (1u16 << depth) - 1;
            for y in bounds.0.max(0)..bounds.2.min(height) {
                let offset = (y - save_rect.0.max(0)) as usize * saved_stride;
                for bx in byte_left..byte_end {
                    let mut mask = 0u8;
                    for slot in 0..pixels_per_byte {
                        let x = (bx * pixels_per_byte + slot) as i32;
                        if x >= i32::from(bounds.1.max(0)) && x < i32::from(bounds.3.min(width)) {
                            mask |= (pixel_mask << (8 - u32::from(depth) * (slot + 1))) as u8;
                        }
                    }
                    let Some(&value) = saved.get(offset + (bx - byte_left) as usize) else {
                        break;
                    };
                    if mask != 0 {
                        let addr = base + y as u32 * row_bytes + bx;
                        bus.write_byte(addr, (bus.read_byte(addr) & !mask) | (value & mask));
                    }
                }
            }
        }
        bus.remember_dialog_snapshot(saved, bounds, true);
    }

    /// Restore previously saved framebuffer pixels under a dialog.
    pub(crate) fn restore_dialog_pixels(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        saved: &SavedPixels,
    ) {
        if bus.dialog_snapshot_is_current(saved, rect, false) {
            return;
        }
        let (screen_base, row_bytes, _, screen_h, pixel_size) = self.get_screen_params();
        let (save_top, save_left, save_bottom, save_right) = Self::dialog_saved_pixel_rect(rect);
        // Mirror save_dialog_pixels y-bounds guard — saved bytes for off-screen
        // rows are skipped so write position stays aligned with the packed input buffer.
        let row_width = (save_right - save_left) as usize;
        let mut idx = 0;
        let screen_h_i16 = screen_h;
        for y in save_top..save_bottom {
            let y_on_screen = y >= 0 && y < screen_h_i16;
            if pixel_size == 8 {
                let on_screen_row =
                    y_on_screen && save_left >= 0 && (save_right as u32) <= row_bytes;
                if on_screen_row && idx + row_width <= saved.len() {
                    let row_addr = screen_base + (y as u32) * row_bytes + (save_left as u32);
                    bus.restore_saved_pixels(row_addr, saved, idx, row_width);
                    idx += row_width;
                } else if y_on_screen {
                    for x in save_left..save_right {
                        if idx < saved.len() {
                            if x >= 0 && (x as u32) < row_bytes {
                                let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                                bus.restore_saved_pixels(addr, saved, idx, 1);
                            }
                            idx += 1;
                        }
                    }
                } else {
                    // Off-screen row: skip the packed bytes that
                    // save_dialog_pixels padded in.
                    idx += row_width.min(saved.len() - idx);
                }
            } else if y_on_screen {
                if let Some((byte_left, byte_end, _)) =
                    Self::packed_row_byte_bounds(save_left, save_right, row_bytes, pixel_size)
                {
                    let len = (byte_end - byte_left) as usize;
                    if idx + len <= saved.len() {
                        let row_addr = screen_base + (y as u32) * row_bytes + byte_left;
                        bus.restore_saved_pixels(row_addr, saved, idx, len);
                        idx += len;
                    } else {
                        for bx in byte_left..byte_end {
                            if idx < saved.len() {
                                bus.write_byte(
                                    screen_base + (y as u32) * row_bytes + bx,
                                    saved[idx],
                                );
                                idx += 1;
                            }
                        }
                    }
                }
            }
            // Packed off-screen: save produced no bytes for this row,
            // so there's nothing to advance idx over here.
        }
        bus.remember_dialog_snapshot(saved, rect, false);
    }

    fn restore_dialog_pixels_outside_rect(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        saved: &SavedPixels,
        keep_rect: (i16, i16, i16, i16),
    ) {
        let (screen_base, row_bytes, _, screen_h, pixel_size) = self.get_screen_params();
        let (top, left, bottom, right) = rect;
        let margin = Self::DBOX_FRAME_MARGIN;
        let save_top = top - margin;
        let save_left = left - margin;
        let save_bottom = bottom + margin;
        let save_right = right + margin;
        let row_width = (save_right - save_left) as usize;
        let mut idx = 0;
        let screen_h_i16 = screen_h;
        for y in save_top..save_bottom {
            let y_on_screen = y >= 0 && y < screen_h_i16;
            if pixel_size == 8 {
                if y_on_screen {
                    for x in save_left..save_right {
                        if idx < saved.len() {
                            if (y < keep_rect.0
                                || y >= keep_rect.2
                                || x < keep_rect.1
                                || x >= keep_rect.3)
                                && x >= 0
                                && (x as u32) < row_bytes
                            {
                                let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                                bus.restore_saved_pixels(addr, saved, idx, 1);
                            }
                            idx += 1;
                        }
                    }
                } else {
                    idx += row_width.min(saved.len().saturating_sub(idx));
                }
            } else if y_on_screen {
                if let Some((byte_left, byte_end, pixels_per_byte)) =
                    Self::packed_row_byte_bounds(save_left, save_right, row_bytes, pixel_size)
                {
                    for bx in byte_left..byte_end {
                        if idx >= saved.len() {
                            break;
                        }
                        let mut restore_mask = 0u8;
                        for pixel_slot in 0..pixels_per_byte {
                            let x = (bx * pixels_per_byte + pixel_slot) as i16;
                            if x >= save_left
                                && x < save_right
                                && (y < keep_rect.0
                                    || y >= keep_rect.2
                                    || x < keep_rect.1
                                    || x >= keep_rect.3)
                            {
                                let shift =
                                    8 - u32::from(pixel_size) - pixel_slot * u32::from(pixel_size);
                                restore_mask |= ((1u8 << pixel_size) - 1) << shift;
                            }
                        }
                        if restore_mask != 0 {
                            let addr = screen_base + (y as u32) * row_bytes + bx;
                            let current = bus.read_byte(addr);
                            bus.write_byte(
                                addr,
                                (current & !restore_mask) | (saved[idx] & restore_mask),
                            );
                        }
                        idx += 1;
                    }
                }
            }
        }
    }

    /// Save framebuffer pixels for an exact rectangle (no margin).
    /// Guards off-screen y (y < 0 or y >= screen_h) from sign-extend overflow.
    /// Off-screen rows pad 8bpp output with zeros; 1bpp output is short by that row.
    fn save_rect_pixels(&self, bus: &MacMemoryBus, rect: (i16, i16, i16, i16)) -> SavedPixels {
        let (screen_base, row_bytes, _, screen_h, pixel_size) = self.get_screen_params();
        let (top, left, bottom, right) = rect;
        let row_width = (right - left).max(0) as usize;
        let mut saved = Vec::with_capacity((bottom - top).max(0) as usize * row_width);
        let screen_h_i16 = screen_h;
        for y in top..bottom {
            let y_on_screen = y >= 0 && y < screen_h_i16;
            if pixel_size == 8 {
                let on_screen_row = y_on_screen && left >= 0 && (right as u32) <= row_bytes;
                if on_screen_row {
                    let row_addr = screen_base + (y as u32) * row_bytes + (left as u32);
                    saved.extend_from_slice(&bus.read_bytes(row_addr, row_width));
                } else if y_on_screen {
                    for x in left..right {
                        if x < 0 || (x as u32) >= row_bytes {
                            saved.push(0);
                        } else {
                            saved.push(
                                bus.read_byte(screen_base + (y as u32) * row_bytes + (x as u32)),
                            );
                        }
                    }
                } else {
                    saved.resize(saved.len() + row_width, 0);
                }
            } else if y_on_screen {
                if let Some((byte_left, byte_end, _)) =
                    Self::packed_row_byte_bounds(left, right, row_bytes, pixel_size)
                {
                    let len = (byte_end - byte_left) as usize;
                    let row_addr = screen_base + (y as u32) * row_bytes + byte_left;
                    saved.extend_from_slice(&bus.read_bytes(row_addr, len));
                }
            }
        }
        let mut saved: SavedPixels = saved.into();
        if pixel_size == 8 {
            let (t, l, b, r) = (top, left, bottom, right);
            for y in t.max(0)..b.min(screen_h) {
                let x0 = l.max(0) as u32;
                let len = (r.max(0) as u32).min(row_bytes).saturating_sub(x0) as usize;
                let offset = (y - t) as usize * row_width + (x0 as i32 - i32::from(l)) as usize;
                bus.capture_pixel_detail(
                    &mut saved,
                    offset,
                    screen_base + y as u32 * row_bytes + x0,
                    len,
                );
            }
        }
        saved
    }

    fn saved_rect_has_non_background_content(&self, saved: &[u8]) -> bool {
        saved.iter().any(|&byte| byte != 0)
    }

    fn user_item_preserve_rects(
        &self,
        dialog_ptr: u32,
        bounds: (i16, i16, i16, i16),
        items: &[DialogItem],
        skip_popup_user_items: bool,
        skip_disabled_placeholders: bool,
    ) -> Vec<((i16, i16, i16, i16), bool)> {
        items
            .iter()
            .enumerate()
            .filter(|(i, it)| {
                (it.item_type & 0x7F) == 0
                    && (!skip_disabled_placeholders
                        || (it.item_type & 0x80) == 0
                        || it.proc_ptr != 0)
                    && (!skip_popup_user_items
                        || !self
                            .dialog_item_popup_menus
                            .contains_key(&(dialog_ptr, (i + 1) as i16)))
            })
            .map(|(_, it)| {
                let (it_t, it_l, it_b, it_r) = it.rect;
                let rect = (
                    bounds.0 + it_t,
                    bounds.1 + it_l,
                    bounds.0 + it_b,
                    bounds.1 + it_r,
                );
                (rect, it.item_type == 0)
            })
            .collect()
    }

    fn restore_user_item_preserved_pixels(
        &self,
        bus: &mut MacMemoryBus,
        rects: &[((i16, i16, i16, i16), bool)],
        backups: &[SavedPixels],
    ) {
        for ((rect, always_restore), pixels) in rects.iter().zip(backups.iter()) {
            if *always_restore || self.saved_rect_has_non_background_content(pixels) {
                self.restore_rect_pixels(bus, *rect, pixels);
            }
        }
    }

    fn draw_dialog_preserving_user_items(
        &mut self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
        proc_id: i16,
        title: &str,
        items: &[DialogItem],
        default_item: i16,
        edit_text: &str,
        edit_item: i16,
        skip_pictures: bool,
        dialog_ptr: u32,
        preserve_user_items: bool,
        skip_popup_user_items: bool,
        skip_disabled_placeholders: bool,
    ) {
        let user_item_rects = if preserve_user_items {
            self.user_item_preserve_rects(
                dialog_ptr,
                bounds,
                items,
                skip_popup_user_items,
                skip_disabled_placeholders,
            )
        } else {
            Vec::new()
        };
        let user_item_backups: Vec<SavedPixels> = user_item_rects
            .iter()
            .map(|&(r, _)| self.save_rect_pixels(bus, r))
            .collect();

        self.draw_dialog(
            bus,
            bounds,
            proc_id,
            title,
            items,
            default_item,
            edit_text,
            edit_item,
            skip_pictures,
            dialog_ptr,
        );

        self.restore_user_item_preserved_pixels(bus, &user_item_rects, &user_item_backups);
        // Restored user-item backgrounds can overlap manager-owned picture
        // items. DrawDialog must still render those items (MTE 1992, 6-142).
        if !skip_pictures && !user_item_backups.is_empty() {
            for item in items {
                if item.item_type & 0x7F == 64 && item.resource_id != 0 {
                    self.draw_dialog_picture_item(bus, bounds, item, dialog_ptr);
                }
            }
        }
        self.capture_gui_frame(bus, &format!("draw_dialog_preserved_{:08X}", dialog_ptr));
    }

    fn fill_rect_clipped_to_dialog(
        &self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
        rect: (i16, i16, i16, i16),
        value: bool,
    ) {
        let top = rect.0.max(bounds.0);
        let left = rect.1.max(bounds.1);
        let bottom = rect.2.min(bounds.2);
        let right = rect.3.min(bounds.3);
        if top >= bottom || left >= right {
            return;
        }

        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
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
            value,
        );
    }

    fn fill_dialog_content_rect(
        &self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        bounds: (i16, i16, i16, i16),
        rect: (i16, i16, i16, i16),
    ) {
        let top = rect.0.max(bounds.0);
        let left = rect.1.max(bounds.1);
        let bottom = rect.2.min(bounds.2);
        let right = rect.3.min(bounds.3);
        if top >= bottom || left >= right {
            return;
        }

        if let Some((red, green, blue)) = self.window_semantic_color(bus, dialog_ptr, 0) {
            let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
                self.get_screen_params();
            let pixel_index = Self::nearest_palette_index(&self.device_clut, [red, green, blue]);
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
                pixel_index,
            );
        } else {
            self.fill_rect_clipped_to_dialog(bus, bounds, rect, false);
        }
    }

    fn fill_dialog_item_background(
        &self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
        rect: (i16, i16, i16, i16),
        rgb: [u16; 3],
    ) {
        let top = rect.0.max(bounds.0);
        let left = rect.1.max(bounds.1);
        let bottom = rect.2.min(bounds.2);
        let right = rect.3.min(bounds.3);
        if top >= bottom || left >= right {
            return;
        }
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let pixel_index = Self::nearest_palette_index(&self.device_clut, rgb);
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
            pixel_index,
        );
    }

    fn dialog_item_enclosing_local_rect(
        item_type: u8,
        rect: (i16, i16, i16, i16),
    ) -> (i16, i16, i16, i16) {
        match item_type & 0x7F {
            // IM:IV IV-59 notes that Dialog Manager drawing can extend
            // outside an editText item's display rectangle by 3 pixels.
            16 => (rect.0 - 3, rect.1 - 3, rect.2 + 3, rect.3 + 3),
            _ => rect,
        }
    }

    fn erase_dialog_item_enclosing_rect(
        &self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        local_rect: (i16, i16, i16, i16),
    ) {
        let bounds = Self::dialog_screen_bounds(bus, dialog_ptr);
        let screen_rect = (
            bounds.0 + local_rect.0,
            bounds.1 + local_rect.1,
            bounds.0 + local_rect.2,
            bounds.1 + local_rect.3,
        );
        self.fill_dialog_content_rect(bus, dialog_ptr, bounds, screen_rect);
    }

    /// Restore framebuffer pixels to an exact rectangle (no margin).
    ///
    /// Mirrors save_rect_pixels off-screen guards so the idx position stays
    /// aligned with the packed input buffer.
    fn restore_rect_pixels(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        saved: &SavedPixels,
    ) {
        let (screen_base, row_bytes, _, screen_h, pixel_size) = self.get_screen_params();
        let (top, left, bottom, right) = rect;
        let row_width = (right - left).max(0) as usize;
        let mut idx = 0;
        let screen_h_i16 = screen_h;
        for y in top..bottom {
            let y_on_screen = y >= 0 && y < screen_h_i16;
            if pixel_size == 8 {
                let on_screen_row = y_on_screen && left >= 0 && (right as u32) <= row_bytes;
                if on_screen_row && idx + row_width <= saved.len() {
                    let row_addr = screen_base + (y as u32) * row_bytes + (left as u32);
                    bus.restore_saved_pixels(row_addr, saved, idx, row_width);
                    idx += row_width;
                } else if y_on_screen {
                    for x in left..right {
                        if idx < saved.len() {
                            if x >= 0 && (x as u32) < row_bytes {
                                bus.restore_saved_pixels(
                                    screen_base + (y as u32) * row_bytes + (x as u32),
                                    saved,
                                    idx,
                                    1,
                                );
                            }
                            idx += 1;
                        }
                    }
                } else {
                    idx = (idx + row_width).min(saved.len());
                }
            } else if y_on_screen {
                if let Some((byte_left, byte_end, _)) =
                    Self::packed_row_byte_bounds(left, right, row_bytes, pixel_size)
                {
                    let len = (byte_end - byte_left) as usize;
                    if idx + len <= saved.len() {
                        let row_addr = screen_base + (y as u32) * row_bytes + byte_left;
                        bus.restore_saved_pixels(row_addr, saved, idx, len);
                        idx += len;
                    } else {
                        for bx in byte_left..byte_end {
                            if idx < saved.len() {
                                bus.write_byte(
                                    screen_base + (y as u32) * row_bytes + bx,
                                    saved[idx],
                                );
                                idx += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    fn draw_dialog_picture_item(
        &mut self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
        item: &DialogItem,
        dialog_ptr: u32,
    ) {
        let (top, left, bottom, right) = bounds;
        let (_, _, screen_width, screen_height, _) = self.get_screen_params();
        let abs_top = top + item.rect.0;
        let abs_left = left + item.rect.1;
        let abs_bottom = top + item.rect.2;
        let abs_right = left + item.rect.3;
        if let Some((_, pic_ptr)) = self.find_or_load_resource_any(bus, *b"PICT", item.resource_id)
        {
            // Draw PICT into the item's display rectangle.
            // DrawPicture must map against the port's logical
            // ColorTable (or stable color_manager_clut), NOT
            // the transient hardware CLUT state (device_clut),
            // which may be faded down to black mid-transition.
            // Imaging With QuickDraw 1994, p. 7-11, 7-14.
            let dialog_clut = if dialog_ptr != 0 {
                let port_version = bus.read_word(dialog_ptr + 6);
                let is_cgraf_port = (port_version & 0xC000) == 0xC000;
                let ctab_handle = if is_cgraf_port {
                    let pm_handle = bus.read_long(dialog_ptr + 2);
                    if pm_handle != 0 {
                        let pm_ptr = bus.read_long(pm_handle);
                        if pm_ptr != 0 {
                            bus.read_long(pm_ptr + 42)
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                } else {
                    0
                };
                self.read_port_clut(bus, ctab_handle)
            } else {
                *self.color_manager_clut
            };
            let device_ct_seed =
                Self::ctab_seed(bus, self.current_gdevice_ctab_handle(bus)).unwrap_or(0);
            // Picture items draw through the dialog port. Preserve
            // their destination geometry while clipping to content,
            // visRgn and clipRgn (including nonrectangular regions).
            // Imaging With QuickDraw 1994, pp. 2-11–2-12.
            let (bounds_top, bounds_left) = self.port_bounds_top_left(bus, dialog_ptr);
            let mut picture_clip = Self::drawpicture_current_port_pixel_clip(
                bus,
                dialog_ptr,
                bounds_top,
                bounds_left,
                screen_width.max(0) as u16,
                screen_height.max(0) as u16,
            )
            .unwrap_or_else(|| {
                super::pict::DstClip::new(
                    (0, 0, i32::from(screen_height), i32::from(screen_width)),
                    Vec::new(),
                )
            });
            picture_clip.intersect_rect((
                i32::from(top),
                i32::from(left),
                i32::from(bottom),
                i32::from(right),
            ));
            super::pict::draw_picture(
                bus,
                pic_ptr,
                abs_top,
                abs_left,
                abs_bottom,
                abs_right,
                self.screen_mode,
                &dialog_clut,
                device_ct_seed,
                Some(&picture_clip),
            );
        }
    }

    /// Draw a complete dialog box with frame and all items.
    /// When `skip_pictures` is true, icon and picture items are skipped
    /// (used during redraw_chrome to avoid re-parsing PICTs every frame).
    pub(crate) fn draw_dialog(
        &mut self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
        proc_id: i16,
        title: &str,
        items: &[DialogItem],
        default_item: i16,
        edit_text: &str,
        edit_item: i16,
        skip_pictures: bool,
        dialog_ptr: u32,
    ) {
        // DrawDialog renders through the dialog window's port, so QuickDraw
        // clips every item to that port's visRgn. HLE dialog primitives write
        // directly to the framebuffer; bail out when the Window Manager has
        // made the dialog fully occluded rather than letting a background
        // dialog overwrite the windows in front of it.
        if dialog_ptr != 0 {
            let vis_handle = bus.read_long(dialog_ptr + 24);
            if vis_handle != 0 && Self::region_handle_rect(bus, vis_handle).is_none() {
                self.dialog_initial_draw_deferred.remove(&dialog_ptr);
                return;
            }
        }
        self.ensure_dialog_background_saved(bus, dialog_ptr, bounds);
        self.dialog_initial_draw_deferred.remove(&dialog_ptr);

        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (top, left, bottom, right) = bounds;

        // A dialog window's content is erased to its background before the
        // application draws (Window Manager PaintOne; IM:I I-282, I-296).
        // Game-managed dialogs (every in-bounds item is a userItem) still get
        // that default erase on the first manager paint; once the application
        // has painted the dialog itself its pixels are authoritative and must
        // not be clobbered by a synthesized shell.
        let game_managed = Self::dialog_is_game_managed(bounds, items);
        let app_painted = self.dialogs_drawn_by_app.contains(&dialog_ptr);
        let clear_background = !game_managed || !app_painted;
        let content_color = self.window_semantic_color(bus, dialog_ptr, 0);
        if clear_background {
            if let Some((red, green, blue)) = content_color {
                let pixel_index =
                    Self::nearest_palette_index(&self.device_clut, [red, green, blue]);
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
                    pixel_index,
                );
            }
        }
        if !clear_background {
            eprintln!(
                "[DIALOG] Preserving app-painted dialog background at ({},{},{},{}), {} items",
                top,
                left,
                bottom,
                right,
                items.len()
            );
        }
        let themed_frame = match proc_id & 0x0F {
            1 => (
                top - Self::DBOX_FRAME_MARGIN,
                left - Self::DBOX_FRAME_MARGIN,
                bottom + Self::DBOX_FRAME_MARGIN,
                right + Self::DBOX_FRAME_MARGIN,
            ),
            2 => (top - 1, left - 1, bottom + 1, right + 1),
            3 => (top - 1, left - 1, bottom + 3, right + 3),
            _ => (top, left, bottom + 2, right + 2),
        };
        if !self.draw_theme_dialog_frame(
            bus,
            (top, left, bottom, right),
            themed_frame,
            proc_id,
            true,
            clear_background && content_color.is_none(),
        ) {
            if clear_background && content_color.is_none() {
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

            // Draw border
            // Inside Macintosh Volume I, I-299:
            // procID 0 = documentProc (title bar + border)
            // procID 1 = dBoxProc (double border, no title)
            // procID 2 = plainDBox (single border, no title)
            // procID 3 = altDBoxProc (shadow border, no title)
            // procID 4 = noGrowDocProc (title bar + border, no grow)
            match proc_id & 0x0F {
                1 => {
                    self.draw_classic_dbox_frame(bus, top, left, bottom, right);
                }
                2 => {
                    // plainDBox: single-pixel WDEF border outside the content
                    // bounds, matching the Window Manager procID 2 path.
                    // Inside Macintosh Volume I, I-275.
                    self.draw_rect_border(bus, top - 1, left - 1, bottom + 1, right + 1);
                }
                3 => {
                    // altDBoxProc: single border + shadow
                    self.draw_rect_border(bus, top - 1, left - 1, bottom + 1, right + 1);
                    self.draw_shadow(bus, top - 1, left - 1, bottom + 1, right + 1);
                }
                0 | 4 | 5 => {
                    // documentProc/noGrowDocProc use the Window Manager's
                    // document WDEF title-bar chrome. DrawDialog is allowed
                    // on modeless dialogs too (IM:I I-417; MTE 1992 p. 6-142),
                    // so route through the same WDEF frame path used by
                    // visible window creation instead of the modal-box fallback.
                    let saved_bounds = self.window_bounds;
                    let saved_proc_id = self.window_proc_id;
                    let saved_title = self.window_title.clone();
                    let saved_go_away = self.go_away_flag;
                    self.window_bounds = bounds;
                    self.window_proc_id = proc_id;
                    self.window_title = if title.is_empty() {
                        Self::dialog_window_title(bus, dialog_ptr)
                    } else {
                        title.to_string()
                    };
                    self.go_away_flag = dialog_ptr != 0 && bus.read_byte(dialog_ptr + 112) != 0;
                    // DrawDialog has already painted the dialog content. The
                    // full WDEF draw path erases its structure region before
                    // drawing document/movable chrome, so use the chrome-only
                    // path here. This also gives movableDBoxProc its striped
                    // title bar without clearing dialog-manager-owned items.
                    // MTE 1992 p. 6-142; HIG 1992 pp. 185-186.
                    self.draw_window_chrome(bus, true);
                    self.window_bounds = saved_bounds;
                    self.window_proc_id = saved_proc_id;
                    self.window_title = saved_title;
                    self.go_away_flag = saved_go_away;
                }
                _ => {
                    // Default: single border + shadow
                    self.draw_rect_border(bus, top, left, bottom, right);
                    self.draw_shadow(bus, top, left, bottom, right);
                }
            }
        }
        self.draw_dialog_items(
            bus,
            bounds,
            proc_id,
            items,
            default_item,
            edit_text,
            edit_item,
            skip_pictures,
            dialog_ptr,
        );
    }

    /// DrawDialog paints items in the existing window; creating or erasing
    /// that window belongs to the Window Manager (MTE 1992, 6-142).
    fn draw_dialog_items(
        &mut self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
        proc_id: i16,
        items: &[DialogItem],
        default_item: i16,
        edit_text: &str,
        edit_item: i16,
        skip_pictures: bool,
        dialog_ptr: u32,
    ) {
        if dialog_ptr != 0 {
            let vis = bus.read_long(dialog_ptr + 24);
            if vis != 0 && Self::region_handle_rect(bus, vis).is_none() {
                return;
            }
        }
        let (top, left, bottom, right) = bounds;
        // Optional per-iteration trace: gate SYSTEMLESS_TRACE_DIALOG_ITEMS=1
        // logs each item's type + relative rect + computed absolute rect.
        // Use to localize artifact-producing items in modal dialog rendering.
        let trace_items = trace_dialog_items_enabled();
        if trace_items {
            eprintln!(
                "[DLG] draw_dialog bounds=({},{},{},{}) proc_id={} edit_item={} default_item={} items={}",
                top,
                left,
                bottom,
                right,
                proc_id,
                edit_item,
                default_item,
                items.len()
            );
        }

        // aDefItem is authoritative for both Return/Enter semantics and the
        // standard button's default chrome. Drawing that chrome here keeps
        // the initial dialog shell coherent before application userItems run.
        let auto_default_outline = true;

        // Draw each item
        for (i, item) in items.iter().enumerate() {
            let item_num = (i + 1) as i16; // 1-based
            let (it, il, ib, ir) = item.rect;
            // Offset by dialog origin
            let abs_top = top + it;
            let abs_left = left + il;
            let abs_bottom = top + ib;
            let abs_right = left + ir;

            let base_type = item.item_type & 0x7F;
            if trace_items {
                eprintln!(
                    "[DLG]   item #{} type=0x{:02X} (base={}) rel=({},{},{},{}) abs=({},{},{},{}) rsrc={} text={:?}",
                    item_num,
                    item.item_type,
                    base_type,
                    it,
                    il,
                    ib,
                    ir,
                    abs_top,
                    abs_left,
                    abs_bottom,
                    abs_right,
                    item.resource_id,
                    item.text,
                );
            }

            // Generic "fully outside dialog" clip. Inside Macintosh
            // Volume I, I-309: dialog items are drawn through the dialog
            // window's port whose visRgn/clipRgn restrict drawing to the
            // dialog bounds. Real Mac OS QuickDraw clips item draws
            // automatically; Systemless's per-type handlers write to the
            // framebuffer directly with no clip. Items whose rect partially
            // extends beyond bounds are NOT clipped here — the per-type
            // handlers can refine if needed.
            if abs_top >= bottom || abs_bottom <= top || abs_left >= right || abs_right <= left {
                if trace_items {
                    eprintln!("[DLG]     -> skipped (fully outside dialog rect)");
                }
                continue;
            }

            // Buttons, checkboxes, radios, and resource-backed controls are
            // Control Manager controls. Draw them in a second pass below so
            // their z-order matches DrawControls: reverse creation order,
            // with the earliest-created control frontmost (IM:I I-322; MTE
            // 1992 pp. 5-87..5-88).
            if matches!(base_type, 4..=7) {
                continue;
            }

            let enabled = (item.item_type & 0x80) == 0;
            match base_type {
                // Static text (8)
                8 => {
                    let style = self.dialog_item_text_style(bus, dialog_ptr, i);
                    if let Some(rgb) = style.and_then(|value| value.background) {
                        self.fill_dialog_item_background(
                            bus,
                            bounds,
                            (abs_top, abs_left, abs_bottom, abs_right),
                            rgb,
                        );
                    }
                    self.draw_static_text(
                        bus, abs_top, abs_left, abs_bottom, abs_right, &item.text, style,
                    );
                }
                // Edit text (16)
                16 => {
                    let display_text = if item_num == edit_item {
                        edit_text
                    } else {
                        &item.text
                    };
                    // MTE 1992 glossary "disabled item": disabled dialog
                    // items do not report user events. Route that semantic
                    // state to themed field chrome while leaving TextEdit
                    // metrics and existing hit handling unchanged.
                    let selection_range = if item_num == edit_item {
                        Self::dialog_item_selection_range(
                            item,
                            encode_mac_roman_lossy(display_text).len(),
                        )
                    } else {
                        None
                    };
                    self.draw_edit_text_with_cursor(
                        bus,
                        abs_top,
                        abs_left,
                        abs_bottom,
                        abs_right,
                        display_text,
                        selection_range,
                        false,
                        enabled,
                    );
                }
                // Icon (32)
                32 => {
                    if !skip_pictures && item.resource_id != 0 {
                        let drew_cicn = if let Some((_, icon_ptr)) =
                            self.find_or_load_resource_any(bus, *b"cicn", item.resource_id)
                        {
                            self.draw_cicn_icon(
                                bus, abs_top, abs_left, abs_bottom, abs_right, icon_ptr,
                            )
                        } else {
                            false
                        };
                        if !drew_cicn {
                            if let Some((_, icon_ptr)) =
                                self.find_or_load_resource_any(bus, *b"ICON", item.resource_id)
                            {
                                // ICON resource: 32x32 1-bit bitmap = 128 bytes
                                // Inside Macintosh Volume I, I-205
                                self.draw_icon(
                                    bus, abs_top, abs_left, abs_bottom, abs_right, icon_ptr,
                                );
                            } else if let Some(icon_ptr) =
                                self.synthesize_system_icon(bus, item.resource_id)
                            {
                                self.draw_icon(
                                    bus, abs_top, abs_left, abs_bottom, abs_right, icon_ptr,
                                );
                            }
                        }
                    }
                }
                // Picture (64)
                64 => {
                    // Items reaching here overlap the dialog rect (the
                    // fully-outside clip happens above the match).
                    if !skip_pictures && item.resource_id != 0 {
                        self.draw_dialog_picture_item(bus, bounds, item, dialog_ptr);
                    }
                }
                // userItem (0) and unknown: skip
                _ => {}
            }
        }

        // Draw standard controls in reverse DITL/control creation order.
        // NewControl adds each control to the beginning of the window's
        // control list, and DrawControls draws that list in reverse so the
        // earliest-created controls appear frontmost on overlap (IM:I
        // I-319/I-322). Dialog Manager standard DITL controls use the same
        // Control Manager semantics.
        for (i, item) in items.iter().enumerate().rev() {
            let item_num = (i + 1) as i16;
            let (it, il, ib, ir) = item.rect;
            let abs_top = top + it;
            let abs_left = left + il;
            let abs_bottom = top + ib;
            let abs_right = left + ir;
            let base_type = item.item_type & 0x7F;
            if !matches!(base_type, 4..=7) {
                continue;
            }
            if abs_top >= bottom || abs_bottom <= top || abs_left >= right || abs_right <= left {
                continue;
            }
            if !self.dialog_control_visible(bus, dialog_ptr, item_num) {
                continue;
            }

            let enabled = (item.item_type & 0x80) == 0;
            if trace_items {
                if let Some(ctrl_handle) = self.dialog_control_handle_for_item(dialog_ptr, item_num)
                {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    let proc_id_ctrl = self.control_manager.proc_id(ctrl_ptr);
                    eprintln!(
                        "[DLG]     item={} control handle=${:08X} ptr=${:08X} visible={} hilite={} value={} min={} max={} proc_id={}",
                        item_num,
                        ctrl_handle,
                        ctrl_ptr,
                        ctrl_ptr != 0 && bus.read_byte(ctrl_ptr + 16) != 0,
                        if ctrl_ptr == 0 {
                            -1
                        } else {
                            bus.read_byte(ctrl_ptr + 17) as i16
                        },
                        if ctrl_ptr == 0 {
                            0
                        } else {
                            bus.read_word(ctrl_ptr + 18) as i16
                        },
                        if ctrl_ptr == 0 {
                            0
                        } else {
                            bus.read_word(ctrl_ptr + 20) as i16
                        },
                        if ctrl_ptr == 0 {
                            0
                        } else {
                            bus.read_word(ctrl_ptr + 22) as i16
                        },
                        proc_id_ctrl,
                    );
                } else {
                    eprintln!("[DLG]     item={} control handle=<none>", item_num);
                }
            }
            match base_type {
                4 => {
                    self.draw_button_with_enabled(
                        bus,
                        abs_top,
                        abs_left,
                        abs_bottom,
                        abs_right,
                        &item.text,
                        auto_default_outline && item_num == default_item,
                        enabled,
                    );
                }
                5 => {
                    let inactive = self.dialog_control_inactive(bus, dialog_ptr, item_num);
                    let checked = self
                        .dialog_control_values
                        .get(&(dialog_ptr, item_num))
                        .copied()
                        .unwrap_or(0)
                        != 0;
                    self.draw_checkbox_with_enabled_and_inactive(
                        bus, abs_top, abs_left, abs_bottom, abs_right, &item.text, checked,
                        enabled, inactive,
                    );
                }
                6 => {
                    let inactive = self.dialog_control_inactive(bus, dialog_ptr, item_num);
                    let selected = self
                        .dialog_control_values
                        .get(&(dialog_ptr, item_num))
                        .copied()
                        .unwrap_or(0)
                        != 0;
                    self.draw_radio_with_enabled_and_inactive(
                        bus, abs_top, abs_left, abs_bottom, abs_right, &item.text, selected,
                        enabled, inactive,
                    );
                }
                7 => {
                    if let Some(ctrl_handle) =
                        self.dialog_control_handle_for_item(dialog_ptr, item_num)
                    {
                        let ctrl_ptr = bus.read_long(ctrl_handle);
                        let proc_id_ctrl =
                            self.control_manager.proc_id(ctrl_ptr);
                        let value = self
                            .dialog_control_values
                            .get(&(dialog_ptr, item_num))
                            .copied()
                            .unwrap_or_else(|| bus.read_word(ctrl_ptr + 18) as i16);
                        let min = bus.read_word(ctrl_ptr + 20) as i16;
                        let max = bus.read_word(ctrl_ptr + 22) as i16;
                        let hilite = bus.read_byte(ctrl_ptr + 17);
                        let title =
                            decode_mac_roman(&Self::control_title_bytes(bus, ctrl_ptr));

                        match proc_id_ctrl {
                            0 => self.draw_button_with_enabled(
                                bus,
                                abs_top,
                                abs_left,
                                abs_bottom,
                                abs_right,
                                &title,
                                auto_default_outline && item_num == default_item,
                                enabled,
                            ),
                            1 => self.draw_checkbox_with_enabled_and_inactive(
                                bus,
                                abs_top,
                                abs_left,
                                abs_bottom,
                                abs_right,
                                &title,
                                value != 0,
                                enabled,
                                hilite == 255,
                            ),
                            2 => self.draw_radio_with_enabled_and_inactive(
                                bus,
                                abs_top,
                                abs_left,
                                abs_bottom,
                                abs_right,
                                &title,
                                value != 0,
                                enabled,
                                hilite == 255,
                            ),
                            16 => self.draw_scroll_bar(
                                bus, abs_top, abs_left, abs_bottom, abs_right, value, min, max,
                                hilite,
                            ),
                            proc_id if Self::is_popup_menu_proc_id(proc_id) => {
                                let menu_id = self.popup_control_menu_id(bus, ctrl_ptr, min);
                                let selected = value.max(1) as usize;
                                let item_title = self.popup_menu_item_title(bus, menu_id, selected);
                                let title_width = self.popup_control_title_width(ctrl_ptr, max);
                                let (draw_top, draw_left, draw_bottom, draw_right) = self
                                    .popup_control_box_rect(
                                        bus,
                                        abs_top,
                                        abs_left,
                                        abs_bottom,
                                        abs_right,
                                        menu_id,
                                        title_width,
                                        proc_id,
                                    );
                                self.draw_popup_control_label(
                                    bus,
                                    abs_top,
                                    abs_left,
                                    abs_bottom,
                                    draw_left,
                                    &title,
                                    enabled && hilite != 255,
                                );
                                self.draw_popup_control_with_state(
                                    bus,
                                    draw_top,
                                    draw_left,
                                    draw_bottom,
                                    draw_right,
                                    &item_title.unwrap_or_default(),
                                    enabled && hilite != 255,
                                    hilite == 1,
                                );
                            }
                            // For controls with custom/unhandled CDEFs (e.g. proc_id 16000),
                            // render button fallback chrome with the control title so
                            // dialog buttons remain functional and visible.
                            _ => {
                                self.draw_button_with_enabled(
                                    bus,
                                    abs_top,
                                    abs_left,
                                    abs_bottom,
                                    abs_right,
                                    &title,
                                    auto_default_outline && item_num == default_item,
                                    enabled,
                                );
                            }
                        }
                    } else {
                        self.draw_button_with_enabled(
                            bus,
                            abs_top,
                            abs_left,
                            abs_bottom,
                            abs_right,
                            "",
                            auto_default_outline && item_num == default_item,
                            enabled,
                        );
                    }
                }
                _ => {}
            }
        }
        self.capture_gui_frame(bus, &format!("draw_dialog_{:08X}", dialog_ptr));
    }

    fn redraw_standard_dialog_items(
        &self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
        items: &[DialogItem],
        default_item: i16,
        edit_text: &str,
        edit_item: i16,
        dialog_ptr: u32,
    ) {
        let (top, left, bottom, right) = bounds;
        // Apply default chrome after restoring guest-owned pixels and
        // redrawing live standard controls so retained composition cannot
        // drop the aDefItem outline.
        let auto_default_outline = true;
        for (i, item) in items.iter().enumerate() {
            let item_num = (i + 1) as i16;
            let (it, il, ib, ir) = item.rect;
            let abs_top = top + it;
            let abs_left = left + il;
            let abs_bottom = top + ib;
            let abs_right = left + ir;
            if abs_top >= bottom || abs_bottom <= top || abs_left >= right || abs_right <= left {
                continue;
            }

            let base_type = item.item_type & 0x7F;
            if matches!(base_type, 4..=7) && !self.dialog_control_visible(bus, dialog_ptr, item_num)
            {
                continue;
            }
            let enabled = (item.item_type & 0x80) == 0;
            match base_type {
                4 => self.draw_button_with_enabled(
                    bus,
                    abs_top,
                    abs_left,
                    abs_bottom,
                    abs_right,
                    &item.text,
                    auto_default_outline && item_num == default_item,
                    enabled,
                ),
                5 => {
                    let inactive = self.dialog_control_inactive(bus, dialog_ptr, item_num);
                    let checked = self
                        .dialog_control_values
                        .get(&(dialog_ptr, item_num))
                        .copied()
                        .unwrap_or(0)
                        != 0;
                    self.fill_dialog_content_rect(
                        bus,
                        dialog_ptr,
                        bounds,
                        (abs_top, abs_left, abs_bottom, abs_right),
                    );
                    self.draw_checkbox_with_enabled_and_inactive(
                        bus, abs_top, abs_left, abs_bottom, abs_right, &item.text, checked,
                        enabled, inactive,
                    );
                }
                6 => {
                    let inactive = self.dialog_control_inactive(bus, dialog_ptr, item_num);
                    let selected = self
                        .dialog_control_values
                        .get(&(dialog_ptr, item_num))
                        .copied()
                        .unwrap_or(0)
                        != 0;
                    self.fill_dialog_content_rect(
                        bus,
                        dialog_ptr,
                        bounds,
                        (abs_top, abs_left, abs_bottom, abs_right),
                    );
                    self.draw_radio_with_enabled_and_inactive(
                        bus, abs_top, abs_left, abs_bottom, abs_right, &item.text, selected,
                        enabled, inactive,
                    );
                }
                7 => {
                    if let Some(ctrl_handle) =
                        self.dialog_control_handle_for_item(dialog_ptr, item_num)
                    {
                        let ctrl_ptr = bus.read_long(ctrl_handle);
                        let proc_id_ctrl =
                            self.control_manager.proc_id(ctrl_ptr);
                        let value = self
                            .dialog_control_values
                            .get(&(dialog_ptr, item_num))
                            .copied()
                            .unwrap_or_else(|| bus.read_word(ctrl_ptr + 18) as i16);
                        let min = bus.read_word(ctrl_ptr + 20) as i16;
                        let max = bus.read_word(ctrl_ptr + 22) as i16;
                        let hilite = bus.read_byte(ctrl_ptr + 17);
                        let title =
                            decode_mac_roman(&Self::control_title_bytes(bus, ctrl_ptr));
                        match proc_id_ctrl {
                            0 => self.draw_button_with_enabled(
                                bus,
                                abs_top,
                                abs_left,
                                abs_bottom,
                                abs_right,
                                &title,
                                auto_default_outline && item_num == default_item,
                                enabled,
                            ),
                            1 => {
                                self.fill_dialog_content_rect(
                                    bus,
                                    dialog_ptr,
                                    bounds,
                                    (abs_top, abs_left, abs_bottom, abs_right),
                                );
                                self.draw_checkbox_with_enabled_and_inactive(
                                    bus,
                                    abs_top,
                                    abs_left,
                                    abs_bottom,
                                    abs_right,
                                    &title,
                                    value != 0,
                                    enabled,
                                    hilite == 255,
                                )
                            }
                            2 => {
                                self.fill_dialog_content_rect(
                                    bus,
                                    dialog_ptr,
                                    bounds,
                                    (abs_top, abs_left, abs_bottom, abs_right),
                                );
                                self.draw_radio_with_enabled_and_inactive(
                                    bus,
                                    abs_top,
                                    abs_left,
                                    abs_bottom,
                                    abs_right,
                                    &title,
                                    value != 0,
                                    enabled,
                                    hilite == 255,
                                )
                            }
                            16 => self.draw_scroll_bar(
                                bus, abs_top, abs_left, abs_bottom, abs_right, value, min, max,
                                hilite,
                            ),
                            proc_id if Self::is_popup_menu_proc_id(proc_id) => {
                                let menu_id = self.popup_control_menu_id(bus, ctrl_ptr, min);
                                let selected = value.max(1) as usize;
                                let item_title = self.popup_menu_item_title(bus, menu_id, selected);
                                let title_width = self.popup_control_title_width(ctrl_ptr, max);
                                let (draw_top, draw_left, draw_bottom, draw_right) = self
                                    .popup_control_box_rect(
                                        bus,
                                        abs_top,
                                        abs_left,
                                        abs_bottom,
                                        abs_right,
                                        menu_id,
                                        title_width,
                                        proc_id,
                                    );
                                self.draw_popup_control_label(
                                    bus,
                                    abs_top,
                                    abs_left,
                                    abs_bottom,
                                    draw_left,
                                    &title,
                                    enabled && hilite != 255,
                                );
                                self.draw_popup_control_with_state(
                                    bus,
                                    draw_top,
                                    draw_left,
                                    draw_bottom,
                                    draw_right,
                                    &item_title.unwrap_or_default(),
                                    enabled && hilite != 255,
                                    hilite == 1,
                                );
                            }
                            // For controls with custom/unhandled CDEFs (e.g. proc_id 16000),
                            // render button fallback chrome with the control title.
                            _ => {
                                self.draw_button_with_enabled(
                                    bus,
                                    abs_top,
                                    abs_left,
                                    abs_bottom,
                                    abs_right,
                                    &title,
                                    auto_default_outline && item_num == default_item,
                                    enabled,
                                );
                            }
                        }
                    } else {
                        self.draw_button_with_enabled(
                            bus,
                            abs_top,
                            abs_left,
                            abs_bottom,
                            abs_right,
                            "",
                            auto_default_outline && item_num == default_item,
                            enabled,
                        );
                    }
                }
                8 => {
                    let style = self.dialog_item_text_style(bus, dialog_ptr, i);
                    if let Some(rgb) = style.and_then(|value| value.background) {
                        self.fill_dialog_item_background(
                            bus,
                            bounds,
                            (abs_top, abs_left, abs_bottom, abs_right),
                            rgb,
                        );
                    } else {
                        self.fill_dialog_content_rect(
                            bus,
                            dialog_ptr,
                            bounds,
                            (abs_top, abs_left, abs_bottom, abs_right),
                        );
                    }
                    self.draw_static_text(
                        bus, abs_top, abs_left, abs_bottom, abs_right, &item.text, style,
                    )
                }
                16 => {
                    let display_text = if item_num == edit_item {
                        edit_text
                    } else {
                        &item.text
                    };
                    let selection_range = if item_num == edit_item {
                        Self::dialog_item_selection_range(
                            item,
                            encode_mac_roman_lossy(display_text).len(),
                        )
                    } else {
                        None
                    };
                    self.draw_edit_text_with_cursor(
                        bus,
                        abs_top,
                        abs_left,
                        abs_bottom,
                        abs_right,
                        display_text,
                        selection_range,
                        false,
                        enabled,
                    );
                }
                _ => {}
            }
        }
    }

    fn refresh_dialog_tracking_snapshot(&mut self, bus: &mut MacMemoryBus) {
        let Some(tracking) = self.dialog_tracking.as_ref() else {
            return;
        };
        if tracking.game_managed || !tracking.rendered_pixels_final {
            return;
        }
        let bounds = tracking.bounds;
        let items = tracking.items.clone();
        let default_item = tracking.default_item;
        let edit_text = tracking.edit_text.clone();
        let edit_item = tracking.edit_item;
        let dialog_ptr = tracking.dialog_ptr;
        let popup_draws = tracking.popup_draws.clone();
        let rendered_pixels = tracking.rendered_pixels.clone();
        if !rendered_pixels.is_empty() {
            self.restore_dialog_pixels(bus, bounds, &rendered_pixels);
        }
        self.redraw_standard_dialog_items(
            bus,
            bounds,
            &items,
            default_item,
            &edit_text,
            edit_item,
            dialog_ptr,
        );
        self.redraw_dialog_popup_controls(bus, &popup_draws);
        let rendered = self.save_dialog_pixels(bus, bounds);
        if let Some(tracking) = self.dialog_tracking.as_mut() {
            tracking.rendered_pixels = rendered;
        }
    }

    fn persist_visible_dialog_snapshot(
        &mut self,
        bus: &MacMemoryBus,
        tracking: &DialogTrackingState,
    ) {
        let pixels = if tracking.rendered_pixels.is_empty() {
            self.save_dialog_pixels(bus, tracking.bounds)
        } else {
            tracking.rendered_pixels.clone()
        };
        self.dialog_visible_snapshots.insert(
            tracking.dialog_ptr,
            PersistentDialogSnapshot {
                bounds: tracking.bounds,
                pixels,
            },
        );
    }

    pub(crate) fn refresh_visible_dialog_snapshot_for_port(
        &mut self,
        bus: &MacMemoryBus,
        port: u32,
    ) {
        if port == 0 {
            return;
        }
        let Some(bounds) = self
            .dialog_visible_snapshots
            .get(&port)
            .map(|snapshot| snapshot.bounds)
            .or_else(|| {
                if self.dialog_items.contains_key(&port) && self.window_visible(bus, port) {
                    Some(Self::dialog_screen_bounds(bus, port))
                } else {
                    None
                }
            })
        else {
            return;
        };
        let pixels = self.save_dialog_pixels(bus, bounds);
        self.dialog_visible_snapshots
            .insert(port, PersistentDialogSnapshot { bounds, pixels });
    }

    pub(crate) fn refresh_visible_dialog_snapshot_region_for_port(
        &mut self,
        bus: &MacMemoryBus,
        port: u32,
        screen_rect: (i16, i16, i16, i16),
    ) {
        if port == 0 || screen_rect.0 >= screen_rect.2 || screen_rect.1 >= screen_rect.3 {
            return;
        }
        let Some(bounds) = self
            .dialog_visible_snapshots
            .get(&port)
            .map(|snapshot| snapshot.bounds)
            .or_else(|| {
                if self.dialog_items.contains_key(&port) && self.window_visible(bus, port) {
                    Some(Self::dialog_screen_bounds(bus, port))
                } else {
                    None
                }
            })
        else {
            return;
        };

        let screen_params = self.get_screen_params();
        if let Some(snapshot) = self.dialog_visible_snapshots.get_mut(&port) {
            Self::refresh_saved_pixel_buffer_after_screen_draw(
                bus,
                screen_params,
                bounds,
                screen_rect,
                &mut snapshot.pixels,
            );
        } else {
            // The first authoritative draw still needs a complete baseline;
            // later draws can update only the pixels they touched.
            let pixels = self.save_dialog_pixels(bus, bounds);
            self.dialog_visible_snapshots
                .insert(port, PersistentDialogSnapshot { bounds, pixels });
        }
    }

    pub(crate) fn refresh_visible_dialog_snapshot_after_bulk_port_draw(
        &mut self,
        bus: &MacMemoryBus,
        port: u32,
        screen_rect: (i16, i16, i16, i16),
    ) {
        if port == 0 {
            return;
        }
        let known_dialog = self.dialog_items.contains_key(&port);
        let visible_dialog = known_dialog && self.window_visible(bus, port);
        let full_port_draw = known_dialog
            .then(|| Self::dialog_screen_bounds(bus, port))
            .is_some_and(|bounds| {
                screen_rect.0 <= bounds.0
                    && screen_rect.1 <= bounds.1
                    && screen_rect.2 >= bounds.2
                    && screen_rect.3 >= bounds.3
            });
        if !visible_dialog && !full_port_draw && !self.dialog_visible_snapshots.contains_key(&port)
        {
            return;
        }
        if full_port_draw && !self.dialog_modal_entered.contains(&port) {
            // A full-port bulk draw is an application-owned dialog
            // composition, not just a custom item update. ModalDialog
            // must not replace it with a newly synthesized shell. This also
            // applies while the window record is still hidden: a screen-backed
            // destination proves that the completed pixels are authoritative.
            self.dialogs_drawn_by_app.insert(port);
        }
        if full_port_draw && !self.dialog_visible_snapshots.contains_key(&port) {
            let bounds = Self::dialog_screen_bounds(bus, port);
            let pixels = self.save_dialog_pixels(bus, bounds);
            self.dialog_visible_snapshots
                .insert(port, PersistentDialogSnapshot { bounds, pixels });
        }
        // `port` is the port that was just drawn into (callers pass the
        // current graphics port). Reaching here means it is a visible dialog
        // window or already has a retained snapshot, so the drawing landed
        // inside that dialog and is authoritative content — seed or refresh
        // its snapshot unconditionally. Applications often render dialog
        // content directly into the window before entering ModalDialog, which
        // happens before the Dialog Manager has begun modal tracking, so
        // gating this on modal-entry lost that drawing.
        // Inside Macintosh Volume I, I-405 (userItem contents are
        // application-owned and must be preserved across dialog redraws).
        let modal_tracking = self
            .dialog_tracking
            .as_ref()
            .filter(|tracking| tracking.dialog_ptr == port)
            .map(|tracking| (tracking.bounds, tracking.last_filter_event.is_some()));
        self.refresh_visible_dialog_snapshot_for_port(bus, port);
        // ModalDialog's re-fire restores `rendered_pixels` over the dialog on
        // every call, so when the drawing target is the active modal dialog,
        // fold the fresh content into that snapshot too or the next re-fire
        // immediately erases it.
        if let Some((bounds, filter_capture_pending)) = modal_tracking {
            let rendered = self.save_dialog_pixels(bus, bounds);
            if let Some(tracking) = self.dialog_tracking.as_mut() {
                tracking.rendered_pixels = rendered;
                // A filter can perform several QuickDraw operations while it
                // handles one event. A bulk operation such as drawing text is
                // not necessarily its final output, so leave the snapshot
                // stale until ModalDialog resumes and captures the completed
                // callback. Macintosh Toolbox Essentials (1992), pp. 6-135,
                // 6-142.
                tracking.rendered_pixels_final = !filter_capture_pending;
            }
        }
    }

    pub(crate) fn redraw_retained_modal_dialog_click(&self, bus: &mut MacMemoryBus) {
        let Some(click) = self.retained_modal_dialog_click.as_ref() else {
            return;
        };
        if !click.highlighted || click.item_no <= 0 || self.front_window != click.dialog_ptr {
            return;
        }
        let bounds = self
            .dialog_visible_snapshots
            .get(&click.dialog_ptr)
            .map(|snapshot| snapshot.bounds)
            .unwrap_or_else(|| Self::dialog_screen_bounds(bus, click.dialog_ptr));
        let rect = Self::dialog_item_screen_rect(bounds, click.rect);
        self.draw_dialog_button_highlight_state(bus, rect, &click.title, click.is_default, true);
    }

    pub(crate) fn finalize_dialog_draw_procs_if_idle(&mut self, bus: &mut MacMemoryBus) {
        let Some(tracking) = self.dialog_tracking.as_ref() else {
            if let Some(dialog_ptr) = self.active_modeless_dialog_draw_proc.take() {
                self.refresh_visible_dialog_snapshot_for_port(bus, dialog_ptr);
                self.capture_gui_frame(
                    bus,
                    &format!("modeless_dialog_draw_proc_{:08X}", dialog_ptr),
                );
            }
            return;
        };
        if tracking.draw_procs_done || !tracking.draw_proc_queue.is_empty() {
            return;
        }

        let bounds = tracking.bounds;
        let items = tracking.items.clone();
        let default_item = tracking.default_item;
        let edit_text = tracking.edit_text.clone();
        let edit_item = tracking.edit_item;
        let dialog_ptr = tracking.dialog_ptr;
        let popup_draws = tracking.popup_draws.clone();
        let game_managed = tracking.game_managed;

        if !game_managed {
            if self.front_window == dialog_ptr {
                self.blit_window_to_screen(bus);
            }
            self.redraw_standard_dialog_items(
                bus,
                bounds,
                &items,
                default_item,
                &edit_text,
                edit_item,
                dialog_ptr,
            );
        }
        self.redraw_dialog_popup_controls(bus, &popup_draws);
        let rendered = self.save_dialog_pixels(bus, bounds);

        if let Some(tracking) = self.dialog_tracking.as_mut() {
            tracking.rendered_pixels = rendered;
            tracking.rendered_pixels_final = true;
            tracking.draw_procs_done = true;
        }
    }

    /// Draw a 1-pixel black rectangle border.
    pub(crate) fn draw_rect_border(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let screen = (
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
        );
        // Top edge
        Self::fb_hline(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            left,
            right - 1,
            true,
        );
        // Bottom edge
        Self::fb_hline(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            bottom - 1,
            left,
            right - 1,
            true,
        );
        // Left edge
        Self::fb_vline(bus, screen, left, top, bottom, true);
        // Right edge
        Self::fb_vline(bus, screen, right - 1, top, bottom, true);
    }

    fn fill_dialog_rect(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        black: bool,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
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
            black,
        );
    }

    pub(crate) fn draw_classic_dbox_frame(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let margin = Self::DBOX_FRAME_MARGIN;
        // dBoxProc is a Window Manager WDEF variant, not a content-edge
        // dialog item border. IM:I I-273 describes a modal dialog box as a
        // rectangular window with its border inside the structure edge; the
        // boundsRect remains the grafPort content region (IM:I I-282).
        self.erase_structure_frame_around_content(
            bus,
            (top - margin, left - margin, bottom + margin, right + margin),
            (top, left, bottom, right),
        );

        // System 7.5.3's standard WDEF draws an asymmetric black structure
        // edge: one pixel on top/left and a two-pixel shadow edge on
        // bottom/right, with a two-pixel inner band inset from the outer edge.
        self.fill_dialog_rect(bus, top - 8, left - 8, top - 7, right + 8, true);
        self.fill_dialog_rect(bus, top - 8, left - 8, bottom + 8, left - 7, true);
        self.fill_dialog_rect(bus, top - 8, right + 6, bottom + 8, right + 8, true);
        self.fill_dialog_rect(bus, bottom + 6, left - 8, bottom + 8, right + 8, true);

        self.fill_dialog_rect(bus, top - 5, left - 5, top - 4, right + 5, true);
        self.fill_dialog_rect(bus, top - 4, left - 5, top - 3, right + 4, true);
        self.fill_dialog_rect(bus, top - 5, left - 5, bottom + 5, left - 4, true);
        self.fill_dialog_rect(bus, top - 5, left - 4, bottom + 4, left - 3, true);
        self.fill_dialog_rect(bus, top - 5, right + 3, bottom + 4, right + 4, true);
        self.fill_dialog_rect(bus, bottom + 3, left - 5, bottom + 4, right + 4, true);
    }

    /// Draw a 2-pixel shadow on bottom and right edges.
    pub(crate) fn draw_shadow(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        // Right shadow (2px wide)
        for dx in 0..2i16 {
            for y in (top + 2)..=(bottom + dx) {
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    right + dx,
                    y,
                    true,
                );
            }
        }
        // Bottom shadow (2px tall)
        for dy in 0..2i16 {
            Self::fb_hline(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                bottom + dy,
                left + 2,
                right + 2,
                true,
            );
        }
    }

    /// Draw a popup menu control button (procID 1008 / popupMenuProc).
    /// Shows a bordered rectangle with a 1-px drop shadow, a downward-pointing
    /// triangle on the right, and the currently-selected item title.
    /// Macintosh Toolbox Essentials 1992, 3-31
    pub(crate) fn draw_popup_control(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        title: &str,
    ) {
        self.draw_popup_control_with_state(bus, top, left, bottom, right, title, true, false);
    }

    pub(crate) fn popup_control_box_rect(
        &self,
        bus: &MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        menu_id: i16,
        title_width: i16,
        proc_id: i16,
    ) -> (i16, i16, i16, i16) {
        let (_, _, screen_width, _, _) = self.get_screen_params();
        let box_top = top + 1;
        let box_left = left + title_width.max(0);
        let box_bottom = bottom - 2;
        let fixed_width = ((proc_id - 1008) & 0x0001) != 0;
        let box_right = if fixed_width {
            right - 1
        } else {
            let text_width = self.popup_menu_max_item_width(bus, menu_id);
            let auto_width = (text_width + 40).max(80);
            (box_left + auto_width).min(right - 1)
        };
        (
            box_top,
            box_left.min(screen_width),
            box_bottom,
            box_right.min(screen_width),
        )
    }

    fn popup_menu_max_item_width(&self, bus: &MacMemoryBus, menu_id: i16) -> i16 {
        let font_id = 0i16;
        let font_size = 12i16;
        let mut max_width = 0;
        for item_no in 1..=255usize {
            let Some(title) = self.popup_menu_item_title(bus, menu_id, item_no) else {
                break;
            };
            max_width = max_width.max(Self::fb_measure_string(&title, font_id, font_size));
        }
        max_width
    }

    pub(crate) fn draw_popup_control_label(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        popup_left: i16,
        title: &str,
        enabled: bool,
    ) {
        if title.is_empty() || popup_left <= left + 4 {
            return;
        }

        let font_id = 0i16;
        let font_size = 12i16;
        let metrics = get_font_metrics(font_id, font_size);
        let text_width = Self::fb_measure_string(title, font_id, font_size);
        let text_right = popup_left - 6;
        let text_x = (text_right - text_width).max(left);
        let text_y = top + ((bottom - top) + metrics.ascent - metrics.descent) / 2;

        self.draw_control_label_text(
            bus, top, left, bottom, popup_left, text_x, text_y, title, font_id, font_size, !enabled,
        );
    }

    pub(crate) fn draw_popup_control_with_state(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        title: &str,
        enabled: bool,
        pressed: bool,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();

        let font_id = 0i16;
        let font_size = 12i16;
        let text_x = left + 15;
        // The standard arrow occupies the right side of the box. Inside
        // Macintosh Volume VI (1991), p. 3-17 requires popupFixedWidth item
        // text that does not fit this content area to be truncated with
        // ellipses. Keep the same bound for auto-sized controls when their
        // resolved box is clamped to the screen edge.
        let text_right = (right - 19).max(text_x);
        let display_title =
            Self::popup_control_display_title(title, text_right - text_x, font_id, font_size);

        if !self.draw_theme_control_chrome(
            bus,
            ControlKind::PopupButton,
            top,
            left,
            bottom,
            right,
            enabled,
            pressed,
            false,
            false,
        ) {
            // Standard popup menu button appearance.
            // Macintosh Toolbox Essentials 1992, 5-26 to 5-27.
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
                right + 1,
                false,
            );
            if enabled {
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
                    bottom - 2,
                    left,
                    right,
                    true,
                );
                for y in top..(bottom - 1) {
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
                for y in (top + 3)..(bottom - 1) {
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
                    bottom - 1,
                    left + 3,
                    right + 1,
                    true,
                );
            } else {
                for y in (top + 3)..(bottom - 2) {
                    if (y - top) % 2 == 1 {
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
                }
                for x in (left + 1)..=right {
                    if (x - left) % 2 == 1 {
                        Self::fb_set_pixel(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            x,
                            bottom - 2,
                            true,
                        );
                    }
                }
            }

            // Downward-pointing triangle on right side (popup indicator)
            // Macintosh Toolbox Essentials 1992, 5-26
            let tri_x = right - 12;
            if enabled {
                for row in 0..6i16 {
                    Self::fb_hline(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        top + 6 + row,
                        tri_x - 5 + row,
                        tri_x + 6 - row,
                        true,
                    );
                }
            } else {
                for row in [0i16, 2, 4] {
                    let start = tri_x - 4 + row;
                    let end = tri_x + 5 - row;
                    for x in start..end {
                        if (x - start) % 2 == 0 {
                            Self::fb_set_pixel(
                                bus,
                                screen_base,
                                row_bytes,
                                pixel_size,
                                screen_width,
                                screen_height,
                                x,
                                top + 7 + row,
                                true,
                            );
                        }
                    }
                }
            }
        }

        // Selected item text inside the box
        // Macintosh Toolbox Essentials 1992, 5-26
        if !display_title.is_empty() {
            let metrics = get_font_metrics(font_id, font_size);
            let text_y =
                top + (bottom - top - (metrics.ascent + metrics.descent)) / 2 + metrics.ascent - 1;
            if enabled {
                Self::fb_draw_string_clipped(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    text_x,
                    text_y,
                    &display_title,
                    font_id,
                    font_size,
                    (top, text_x, bottom, text_right),
                );
            } else {
                self.draw_control_label_text(
                    bus,
                    top,
                    text_x,
                    bottom,
                    text_right,
                    text_x,
                    text_y,
                    &display_title,
                    font_id,
                    font_size,
                    true,
                );
            }
        }
    }

    fn popup_control_display_title(
        title: &str,
        available_width: i16,
        font_id: i16,
        font_size: i16,
    ) -> String {
        if available_width <= 0 {
            return String::new();
        }
        if Self::fb_measure_string(title, font_id, font_size) <= available_width {
            return title.to_owned();
        }

        let ellipses = "...";
        let ellipses_width = Self::fb_measure_string(ellipses, font_id, font_size);
        if ellipses_width > available_width {
            return String::new();
        }

        let mut prefix = String::new();
        let mut prefix_width = 0;
        for ch in title.chars() {
            let char_width = Self::fb_measure_string(&ch.to_string(), font_id, font_size);
            if prefix_width + char_width + ellipses_width > available_width {
                break;
            }
            prefix.push(ch);
            prefix_width += char_width;
        }
        prefix.push_str(ellipses);
        prefix
    }

    fn redraw_dialog_popup_controls(
        &self,
        bus: &mut MacMemoryBus,
        popup_draws: &[DialogPopupDraw],
    ) {
        for draw in popup_draws {
            let (top, left, bottom, right) = draw.rect;
            if draw.enabled && !draw.pressed {
                self.draw_popup_control(bus, top, left, bottom, right, &draw.title);
                continue;
            }
            self.draw_popup_control_with_state(
                bus,
                top,
                left,
                bottom,
                right,
                &draw.title,
                draw.enabled,
                draw.pressed,
            );
        }
    }

    /// Draw a framed round-rect directly to the framebuffer.
    /// Used for dialog default button outlines where no CpuOps/port is available.
    /// Macintosh Toolbox Essentials 1992, Listing 6-17
    fn fb_frame_round_rect(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        oval_width: i16,
        oval_height: i16,
        pen_size: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let slot = bus.presentation.clone();
        let black = Self::fb_main_screen_pixel_index_for_rgb(bus, [0; 3]).unwrap_or(255);
        let detail = (oval_width == oval_height)
            .then(|| {
                slot.rounded_control_corners(
                    (top.into(), left.into(), bottom.into(), right.into()),
                    oval_width.into(),
                    pen_size.into(),
                    if pixel_size == 8 { 8 } else { 0 },
                    None,
                    black.into(),
                    |x, y, _| {
                        if x < 0
                            || y < 0
                            || x >= i32::from(screen_width)
                            || y >= i32::from(screen_height)
                        {
                            return None;
                        }
                        let address = screen_base + y as u32 * row_bytes + x as u32;
                        Some((address, bus.read_byte(address)))
                    },
                )
            })
            .flatten();
        let r = Rect {
            top,
            left,
            bottom,
            right,
        };
        let outer_spans = Self::compute_rrect_spans(&r, oval_width, oval_height);

        let r_inset = Rect {
            top: top + pen_size,
            left: left + pen_size,
            bottom: bottom - pen_size,
            right: right - pen_size,
        };
        let inner_spans = Self::compute_rrect_spans(
            &r_inset,
            (oval_width - 2 * pen_size).max(0),
            (oval_height - 2 * pen_size).max(0),
        );

        for y in top..bottom {
            let outer_idx = (y - top) as usize;
            if outer_idx >= outer_spans.len() {
                continue;
            }
            let (ol, or) = outer_spans[outer_idx];

            let inner_idx = (y - r_inset.top) as usize;
            let (il, ir) =
                if y >= r_inset.top && y < r_inset.bottom && inner_idx < inner_spans.len() {
                    inner_spans[inner_idx]
                } else {
                    (or, ol) // no inner = draw full outer span
                };

            // Left border segment
            for x in ol..il.min(or) {
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
            // Right border segment
            for x in ir.max(ol)..or {
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
        }
        slot.finish_rounded_control(detail, |address| bus.read_byte(address));
    }

    /// Draw a button with optional default (thick) border.
    /// Inside Macintosh Volume I, I-405
    pub(crate) fn draw_button(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        title: &str,
        is_default: bool,
    ) {
        self.draw_button_state(bus, top, left, bottom, right, title, is_default, true);
    }

    fn draw_button_with_enabled(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        title: &str,
        is_default: bool,
        enabled: bool,
    ) {
        if enabled {
            self.draw_button(bus, top, left, bottom, right, title, is_default);
            return;
        }
        self.draw_button_state(bus, top, left, bottom, right, title, is_default, enabled);
    }

    pub(crate) fn draw_button_state(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        title: &str,
        is_default: bool,
        enabled: bool,
    ) {
        if !self.draw_theme_push_button_chrome(
            bus, top, left, bottom, right, enabled, false, is_default,
        ) {
            let slot = bus.presentation.clone();
            let (base, row_bytes, width, height, pixel_size) = self.get_screen_params();
            let white = Self::fb_main_screen_pixel_index_for_rgb(bus, [0xffff; 3]).unwrap_or(0);
            let black = Self::fb_main_screen_pixel_index_for_rgb(bus, [0; 3]).unwrap_or(255);
            let detail = slot.rounded_control_corners(
                (top.into(), left.into(), bottom.into(), right.into()),
                crate::control_manager::STANDARD_BUTTON_OVAL.into(),
                1,
                if pixel_size == 8 { 8 } else { 0 },
                Some(white.into()),
                black.into(),
                |x, y, _| {
                    if x < 0 || y < 0 || x >= i32::from(width) || y >= i32::from(height) {
                        return None;
                    }
                    let address = base + y as u32 * row_bytes + x as u32;
                    Some((address, bus.read_byte(address)))
                },
            );
            self.fill_classic_button_shape(bus, top, left, bottom, right);
            self.draw_classic_button_outline(bus, top, left, bottom, right);
            slot.finish_rounded_control(detail, |address| bus.read_byte(address));

            // Default button: rounded bold outline (3px thick)
            // Macintosh Toolbox Essentials 1992, Listing 6-17
            // references/executor/src/error/system_error.cpp
            if is_default {
                let hilite_top = top - 4;
                let hilite_left = left - 4;
                let hilite_bottom = bottom + 4;
                let hilite_right = right + 4;
                let hilite_height = hilite_bottom - hilite_top;
                let oval = (hilite_height / 2 - 4).max(4);
                self.fb_frame_round_rect(
                    bus,
                    hilite_top,
                    hilite_left,
                    hilite_bottom,
                    hilite_right,
                    oval,
                    oval,
                    3,
                );
            }
        }

        self.draw_button_label(bus, top, left, bottom, right, title, enabled);
    }

    fn fill_classic_button_shape(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        if bottom - top < 8 || right - left < 8 {
            let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
                self.get_screen_params();
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
            return;
        }

        // Classic push buttons are rounded controls inside the DITL display
        // rectangle. Fill the same rounded shape the CDEF frames so pixels in
        // the rectangular corner cutouts remain whatever the dialog port had
        // underneath.
        if bottom - top >= 24 {
            self.fill_dialog_rect(bus, top, left + 4, top + 1, right - 4, false);
            self.fill_dialog_rect(bus, top + 1, left + 2, top + 2, right - 2, false);
            self.fill_dialog_rect(bus, top + 2, left + 1, top + 4, right - 1, false);
            self.fill_dialog_rect(bus, top + 4, left, bottom - 4, right, false);
            self.fill_dialog_rect(bus, bottom - 4, left + 1, bottom - 2, right - 1, false);
            self.fill_dialog_rect(bus, bottom - 2, left + 2, bottom - 1, right - 2, false);
            self.fill_dialog_rect(bus, bottom - 1, left + 4, bottom, right - 4, false);
            return;
        }

        self.fill_dialog_rect(bus, top, left + 3, top + 1, right - 3, false);
        self.fill_dialog_rect(bus, top + 1, left + 1, top + 3, right - 1, false);
        self.fill_dialog_rect(bus, top + 3, left, bottom - 3, right, false);
        self.fill_dialog_rect(bus, bottom - 3, left + 1, bottom - 1, right - 1, false);
        self.fill_dialog_rect(bus, bottom - 1, left + 3, bottom, right - 3, false);
    }

    fn draw_button_label(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        title: &str,
        enabled: bool,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let font_id = 0i16; // Chicago
        let font_size = 12i16;
        let metrics = get_font_metrics(font_id, font_size);
        let text_w = Self::fb_measure_string(title, font_id, font_size);
        let text_x = left + (right - left - text_w) / 2;
        let text_y = top + (bottom - top - (metrics.ascent + metrics.descent)) / 2 + metrics.ascent;
        if enabled || pixel_size != 8 {
            Self::fb_draw_string(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                text_x,
                text_y,
                title,
                font_id,
                font_size,
            );
        } else {
            let gray_index = self
                .device_clut
                .iter()
                .enumerate()
                .min_by_key(|(_, rgb)| {
                    rgb.iter()
                        .map(|component| (i32::from(*component) - 0xAAAA).unsigned_abs())
                        .sum::<u32>()
                })
                .map(|(index, _)| index as u8)
                .unwrap_or(0);
            Self::fb_draw_string_styled_index(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                text_x,
                text_y,
                title,
                font_id,
                font_size,
                0,
                gray_index,
            );
        }
    }

    fn draw_classic_button_outline(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        if bottom - top < 8 || right - left < 8 {
            self.draw_rect_border(bus, top, left, bottom, right);
            return;
        }

        // Dialog Manager buttons are standard controls whose display
        // rectangle becomes the control's enclosing rectangle (IM:I I-405).
        // The classic System 7 control definition draws the push button as a
        // one-pixel rounded rectangle inside that enclosing rectangle.
        if bottom - top >= 24 {
            self.fill_dialog_rect(bus, top, left + 4, top + 1, right - 4, true);
            self.fill_dialog_rect(bus, top + 1, left + 2, top + 2, left + 4, true);
            self.fill_dialog_rect(bus, top + 1, right - 4, top + 2, right - 2, true);
            self.fill_dialog_rect(bus, top + 2, left + 1, top + 3, left + 2, true);
            self.fill_dialog_rect(bus, top + 2, right - 2, top + 3, right - 1, true);
            self.fill_dialog_rect(bus, top + 3, left + 1, top + 4, left + 2, true);
            self.fill_dialog_rect(bus, top + 3, right - 2, top + 4, right - 1, true);
            self.fill_dialog_rect(bus, top + 4, left, bottom - 4, left + 1, true);
            self.fill_dialog_rect(bus, top + 4, right - 1, bottom - 4, right, true);
            self.fill_dialog_rect(bus, bottom - 4, left + 1, bottom - 3, left + 2, true);
            self.fill_dialog_rect(bus, bottom - 4, right - 2, bottom - 3, right - 1, true);
            self.fill_dialog_rect(bus, bottom - 3, left + 1, bottom - 2, left + 2, true);
            self.fill_dialog_rect(bus, bottom - 3, right - 2, bottom - 2, right - 1, true);
            self.fill_dialog_rect(bus, bottom - 2, left + 2, bottom - 1, left + 4, true);
            self.fill_dialog_rect(bus, bottom - 2, right - 4, bottom - 1, right - 2, true);
            self.fill_dialog_rect(bus, bottom - 1, left + 4, bottom, right - 4, true);
            return;
        }

        self.fill_dialog_rect(bus, top, left + 3, top + 1, right - 3, true);
        self.fill_dialog_rect(bus, top + 1, left + 1, top + 2, left + 3, true);
        self.fill_dialog_rect(bus, top + 1, right - 3, top + 2, right - 1, true);
        self.fill_dialog_rect(bus, top + 2, left + 1, top + 3, left + 2, true);
        self.fill_dialog_rect(bus, top + 2, right - 2, top + 3, right - 1, true);
        self.fill_dialog_rect(bus, top + 3, left, bottom - 3, left + 1, true);
        self.fill_dialog_rect(bus, top + 3, right - 1, bottom - 3, right, true);
        self.fill_dialog_rect(bus, bottom - 3, left + 1, bottom - 2, left + 2, true);
        self.fill_dialog_rect(bus, bottom - 3, right - 2, bottom - 2, right - 1, true);
        self.fill_dialog_rect(bus, bottom - 2, left + 1, bottom - 1, left + 3, true);
        self.fill_dialog_rect(bus, bottom - 2, right - 3, bottom - 1, right - 1, true);
        self.fill_dialog_rect(bus, bottom - 1, left + 3, bottom, right - 3, true);
    }

    fn draw_dialog_button_highlight_state(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        title: &str,
        is_default: bool,
        highlighted: bool,
    ) {
        let (top, left, bottom, right) = rect;
        if self.draw_theme_push_button_chrome(
            bus,
            top,
            left,
            bottom,
            right,
            true,
            highlighted,
            is_default,
        ) {
            self.draw_button_label(bus, top, left, bottom, right, title, true);
            return;
        }
        self.invert_button_rect(bus, top, left, bottom, right);
    }

    fn restore_dialog_button_normal_state(
        &self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        bounds: (i16, i16, i16, i16),
        item_no: i16,
        item: &DialogItem,
        is_default: bool,
    ) {
        if let Some(control_handle) = self.dialog_control_handle_for_item(dialog_ptr, item_no) {
            let control_ptr = bus.read_long(control_handle);
            if control_ptr != 0 {
                bus.write_byte(control_ptr + 17, 0);
            }
        }
        let (top, left, bottom, right) = Self::dialog_item_screen_rect(bounds, item.rect);
        self.draw_button_with_enabled(bus, top, left, bottom, right, &item.text, is_default, true);
    }

    /// Draw a checkbox item.
    pub(crate) fn draw_checkbox(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        _right: i16,
        title: &str,
        checked: bool,
    ) {
        self.draw_checkbox_state(bus, top, left, bottom, _right, title, checked, true, false);
    }

    fn draw_checkbox_with_enabled_and_inactive(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        _right: i16,
        title: &str,
        checked: bool,
        enabled: bool,
        inactive: bool,
    ) {
        if enabled && !inactive {
            self.draw_checkbox(bus, top, left, bottom, _right, title, checked);
            return;
        }
        self.draw_checkbox_state(
            bus, top, left, bottom, _right, title, checked, enabled, inactive,
        );
    }

    fn draw_checkbox_state(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        _right: i16,
        title: &str,
        checked: bool,
        enabled: bool,
        inactive: bool,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let box_size = Self::STANDARD_CONTROL_MARK_SIZE;
        // Standard dialog checkbox/radio controls are drawn by the System
        // CDEF as a 12-pixel mark plus title inside the item rectangle
        // (MTE 1992 pp. 5-4..5-5; HIG 1992 pp. 209..212).
        let height = bottom - top;
        let box_top = top + (height - box_size) / 2;
        let box_left = left + Self::STANDARD_CONTROL_MARK_LEFT_INSET;
        if !self.draw_theme_control_chrome(
            bus,
            ControlKind::Checkbox,
            box_top,
            box_left,
            box_top + box_size,
            box_left + box_size,
            enabled && !inactive,
            false,
            checked,
            false,
        ) {
            // Draw checkbox box
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                box_top,
                box_left,
                box_top + box_size,
                box_left + box_size,
                false,
            );
            self.draw_rect_border(
                bus,
                box_top,
                box_left,
                box_top + box_size,
                box_left + box_size,
            );
            if checked {
                // Draw X inside
                for i in 1..box_size - 1 {
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        box_left + i,
                        box_top + i,
                        true,
                    );
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        box_left + box_size - 1 - i,
                        box_top + i,
                        true,
                    );
                }
            }
        }
        // Draw label text to the right of the box. Checkbox/radio items are
        // controls, and SetDialogFont/SetDAFont does not affect control
        // titles (IM:I I-412; MTE 1992 p. 6-105).
        let font_id = 0i16;
        let font_size = 12i16;
        let metrics = get_font_metrics(font_id, font_size);
        let text_x = box_left + box_size + Self::STANDARD_CONTROL_TITLE_GAP;
        let text_y = top + (height + metrics.ascent - metrics.descent) / 2;
        let label_left = box_left + box_size + 1;
        let dim_disabled_title = !enabled && self.ui_theme_id() != UiThemeId::ClassicSystem7;
        self.draw_standard_control_title_text(
            bus,
            top,
            label_left,
            bottom,
            _right,
            text_x,
            text_y,
            title,
            font_id,
            font_size,
            inactive,
            dim_disabled_title,
        );
    }

    /// Draw a radio button item.
    pub(crate) fn draw_radio(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        _right: i16,
        title: &str,
        selected: bool,
    ) {
        self.draw_radio_state(bus, top, left, bottom, _right, title, selected, true, false);
    }


    fn draw_radio_with_enabled_and_inactive(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        _right: i16,
        title: &str,
        selected: bool,
        enabled: bool,
        inactive: bool,
    ) {
        if enabled && !inactive {
            self.draw_radio(bus, top, left, bottom, _right, title, selected);
            return;
        }
        self.draw_radio_state(
            bus, top, left, bottom, _right, title, selected, enabled, inactive,
        );
    }

    fn draw_radio_state(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        _right: i16,
        title: &str,
        selected: bool,
        enabled: bool,
        inactive: bool,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let mark_size = Self::STANDARD_CONTROL_MARK_SIZE;
        let height = bottom - top;
        let mark_top = top + (height - mark_size) / 2;
        let mark_left = left + Self::STANDARD_CONTROL_MARK_LEFT_INSET;
        if !self.draw_theme_control_chrome(
            bus,
            ControlKind::RadioButton,
            mark_top,
            mark_left,
            mark_top + mark_size,
            mark_left + mark_size,
            enabled && !inactive,
            false,
            selected,
            false,
        ) {
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                mark_top,
                mark_left,
                mark_top + mark_size,
                mark_left + mark_size,
                false,
            );
            // Radio buttons are small circles, with a dot when selected
            // (IM:I I-312; MTE 1992 pp. 5-5..5-6; HIG 1992 pp. 209..210).
            self.draw_control_mask_12(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                mark_left,
                mark_top,
                &Self::CLASSIC_RADIO_OUTLINE_12,
            );
            if selected {
                self.draw_control_mask_12(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    mark_left,
                    mark_top,
                    &Self::CLASSIC_RADIO_DOT_12,
                );
            }
        }
        // Label
        // SetDialogFont/SetDAFont affects statText and editText items but
        // not control titles (IM:I I-412; MTE 1992 p. 6-105).
        let font_id = 0i16;
        let font_size = 12i16;
        let metrics = get_font_metrics(font_id, font_size);
        let text_x = mark_left + mark_size + Self::STANDARD_CONTROL_TITLE_GAP;
        let text_y = top + (height + metrics.ascent - metrics.descent) / 2;
        let label_left = mark_left + mark_size + 1;
        let dim_disabled_title = !enabled && self.ui_theme_id() != UiThemeId::ClassicSystem7;
        self.draw_standard_control_title_text(
            bus,
            top,
            label_left,
            bottom,
            _right,
            text_x,
            text_y,
            title,
            font_id,
            font_size,
            inactive,
            dim_disabled_title,
        );
    }

    fn draw_standard_control_title_text(
        &self,
        bus: &mut MacMemoryBus,
        label_top: i16,
        label_left: i16,
        label_bottom: i16,
        label_right: i16,
        text_x: i16,
        text_y: i16,
        title: &str,
        font_id: i16,
        font_size: i16,
        inactive: bool,
        dim_disabled_title: bool,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        if inactive && pixel_size == 8 {
            // Standard DITL controls are backed by ControlRecords. When a
            // game calls HiliteControl(255), their titles follow inactive
            // Control Manager rendering; itemDisable alone stays visually
            // active in classic dialogs.
            let ink = self.inactive_control_title_index();
            Self::fb_draw_string_styled_index(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                text_x,
                text_y,
                title,
                font_id,
                font_size,
                0,
                ink,
            );
            return;
        }

        Self::fb_draw_string(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            text_x,
            text_y,
            title,
            font_id,
            font_size,
        );
        if inactive || dim_disabled_title {
            self.dim_rect(bus, label_top, label_left, label_bottom, label_right);
        }
    }

    fn draw_control_mask_12(
        &self,
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        left: i16,
        top: i16,
        mask: &[&str; 12],
    ) {
        for (dy, row) in mask.iter().enumerate() {
            for (dx, pixel) in row.as_bytes().iter().enumerate() {
                if *pixel == b'#' {
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

    /// Replace `^0`..`^3` in dialog/alert text with the strings most
    /// recently passed to `ParamText`. Returns a borrowed reference to
    /// the original `text` when no placeholders are present (the common
    /// case — most DITL items don't use ParamText), avoiding an
    /// allocation per draw_static_text call.
    /// Inside Macintosh Volume I, I-422.
    pub(crate) fn apply_param_text<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        if !text.contains('^') {
            return std::borrow::Cow::Borrowed(text);
        }
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '^' {
                if let Some(&next) = chars.peek() {
                    if let Some(idx) = next.to_digit(10) {
                        if (idx as usize) < self.param_text.len() {
                            chars.next();
                            out.push_str(&decode_mac_roman(
                                &self.param_text[idx as usize],
                            ));
                            continue;
                        }
                    }
                }
            }
            out.push(ch);
        }
        std::borrow::Cow::Owned(out)
    }

    fn static_text_bytes(&self, text: &str) -> Vec<u8> {
        encode_mac_roman_lossy(&self.apply_param_text(text))
    }

    fn draw_static_text(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        text: &str,
        style: Option<DialogItemTextStyle>,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        // SetDialogFont/SetDAFont affects statText and editText items but
        // not control titles (IM:I I-412; MTE 1992 p. 6-105).
        let font_id = style.map(|value| value.font).unwrap_or(self.tx_font);
        let raw_font_size = style.map(|value| value.size).unwrap_or(self.tx_size);
        let font_size = Self::font_lookup_size(raw_font_size);
        let face = style.map(|value| value.face).unwrap_or(self.tx_face as u8);
        let foreground = style.and_then(|value| value.foreground);
        let _mode = style.map(|value| value.mode).unwrap_or(self.tx_mode);
        let metrics = get_font_metrics(font_id, font_size);
        let line_height = metrics.ascent + metrics.descent + metrics.leading;
        let max_width = (right - left).saturating_sub(Self::TE_LINE_LEFT_INSET);
        let mut y = top + metrics.ascent;
        if y >= bottom && bottom > top {
            y = bottom - 1;
        }

        // Dialog items are retained as Unicode so HLE-created text and decoded
        // guest strings share one representation. TextEdit and QuickDraw still
        // lay out classic bytes, so encode exactly once at that boundary.
        let text_bytes = self.static_text_bytes(text);
        let lines = self.te_wrap_lines(font_id, raw_font_size, &text_bytes, max_width);

        for (start, end) in lines {
            let mut trimmed_end = end;
            while trimmed_end > start && matches!(text_bytes[trimmed_end - 1], b' ' | b'\r' | b'\n')
            {
                trimmed_end -= 1;
            }
            if trimmed_end > start && y <= bottom {
                // IM:I I-405 to I-406: statText draws like editText text
                // inside the display rectangle, including wrap and clipping,
                // but without the editText frame.
                let line: String = text_bytes[start..trimmed_end]
                    .iter()
                    .map(|&byte| byte as char)
                    .collect();
                if let Some(rgb) = foreground {
                    let pixel_index = Self::nearest_palette_index(&self.device_clut, rgb);
                    Self::fb_draw_string_styled_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        left + Self::TE_LINE_LEFT_INSET,
                        y,
                        &line,
                        font_id,
                        font_size,
                        face,
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
                        left + Self::TE_LINE_LEFT_INSET,
                        y,
                        &line,
                        font_id,
                        font_size,
                        face,
                    );
                }
            }
            y += line_height;
            if y > bottom {
                break;
            }
        }
    }

    /// Draw an editable text field with border and text.
    pub(crate) fn draw_edit_text(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        text: &str,
        selected: bool,
    ) {
        let selection_range =
            selected.then_some((0, encode_mac_roman_lossy(text).len()));
        self.draw_edit_text_with_cursor(
            bus,
            top,
            left,
            bottom,
            right,
            text,
            selection_range,
            !selected,
            true,
        );
    }

    fn draw_edit_text_with_cursor(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        text: &str,
        selection_range: Option<(usize, usize)>,
        show_cursor: bool,
        enabled: bool,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        // IM:I I-414 and HIG 1992 p. 184: the active editText item is a
        // text-entry field with visible focus feedback. The theme provider owns
        // that field chrome only; TextEdit metrics, text drawing, selection,
        // and caret positioning remain classic guest-visible behavior.
        let frame_top = top - Self::EDIT_TEXT_FRAME_OUTSET;
        let frame_left = left - Self::EDIT_TEXT_FRAME_OUTSET;
        let frame_bottom = bottom + Self::EDIT_TEXT_FRAME_OUTSET;
        let frame_right = right + Self::EDIT_TEXT_FRAME_OUTSET;

        if !self.draw_theme_text_field(
            bus,
            frame_top,
            frame_left,
            frame_bottom,
            frame_right,
            enabled,
            selection_range.is_some() || show_cursor,
        ) {
            // White fill
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                frame_top,
                frame_left,
                frame_bottom,
                frame_right,
                false,
            );
            // IM:I I-405: editText's display rectangle becomes the TextEdit
            // view/dest rectangle, and Dialog Manager frames it three pixels
            // outside that display rectangle.
            self.draw_rect_border(bus, frame_top, frame_left, frame_bottom, frame_right);
        }
        let font_id = self.tx_font;
        let font_size = Self::font_lookup_size(self.tx_size);
        let metrics = get_font_metrics(font_id, font_size);
        let text_y = top + metrics.ascent;
        Self::fb_draw_string(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            left + 1,
            text_y,
            text,
            font_id,
            font_size,
        );
        if let Some((selection_start, selection_end)) = selection_range {
            // IM:I I-422 and MTE 1992 p. 6-131: SelIText /
            // SelectDialogItemText displays the selected range by inverting
            // the selected characters. Match TextEdit's left-justified
            // selection rectangle: glyphs draw at destRect.left+1, but a
            // selection beginning at character 0 includes the one-pixel inset.
            if !text.is_empty() {
                let text_bytes = encode_mac_roman_lossy(text);
                let start = selection_start.min(text_bytes.len());
                let end = selection_end.min(text_bytes.len());
                if start < end {
                    let line_height = metrics.ascent + metrics.descent + metrics.leading;
                    let selection_top = top;
                    let selection_bottom = top.saturating_add(line_height).min(bottom);
                    let selection_left = if start == 0 {
                        left
                    } else {
                        left + Self::TE_LINE_LEFT_INSET
                            + self.te_measure_text_width(
                                font_id,
                                self.tx_size,
                                &text_bytes,
                                0,
                                start,
                            )
                    };
                    let selection_right = if end >= text_bytes.len() {
                        right
                    } else {
                        left + Self::TE_LINE_LEFT_INSET
                            + self.te_measure_text_width(
                                font_id,
                                self.tx_size,
                                &text_bytes,
                                0,
                                end,
                            )
                    };
                    if selection_left < selection_right && selection_top < selection_bottom {
                        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
                            self.invert_rect_exact(
                                bus,
                                selection_top,
                                selection_left,
                                selection_bottom,
                                selection_right,
                            );
                        } else {
                            self.draw_theme_text_selection(
                                bus,
                                selection_top,
                                selection_left,
                                selection_bottom,
                                selection_right,
                                true,
                            );
                        }
                    }
                }
            }
        } else if show_cursor {
            // Draw cursor bar at end of text
            let text_width = Self::fb_measure_string(text, font_id, font_size);
            let cursor_x = left + 3 + text_width;
            if cursor_x < right - 1
                && !self.draw_theme_caret(bus, top + 2, cursor_x, bottom - 1, cursor_x + 1)
            {
                for y in (top + 2)..=(bottom - 2) {
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        cursor_x,
                        y,
                        true,
                    );
                }
            }
        }
    }

    fn dialog_cicn_layout(bus: &MacMemoryBus, icon_ptr: u32) -> Option<DialogCIconLayout> {
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

        let ct_size = bus.read_word(ctab_ptr + 6);
        let ctab_total_bytes = 8u32.checked_add((u32::from(ct_size) + 1).checked_mul(8)?)?;
        let pixel_data_ptr = ctab_ptr.checked_add(ctab_total_bytes)?;

        if let Some(resource_size) = bus.get_alloc_size(icon_ptr) {
            let resource_end = icon_ptr.checked_add(resource_size)?;
            let pixel_data_size = pm_row_bytes.checked_mul(height_u32)?;
            if pixel_data_ptr.checked_add(pixel_data_size)? > resource_end {
                return None;
            }
        }

        Some(DialogCIconLayout {
            width,
            height,
            pm_row_bytes,
            mask_row_bytes,
            bmap_row_bytes,
            pixel_size,
            mask_data_ptr,
            bmap_data_ptr,
            ctab_ptr,
            ct_size,
            pixel_data_ptr,
        })
    }

    fn dialog_cicn_rgb_for_index(
        bus: &MacMemoryBus,
        layout: &DialogCIconLayout,
        source_index: u8,
    ) -> Option<[u16; 3]> {
        // ColorSpec.value supplies sparse source indices unless ctFlags marks
        // the table as sequential, in which case the entry ordinal is used.
        let ct_flags = bus.read_word(layout.ctab_ptr + 4);
        for ordinal in 0..=u32::from(layout.ct_size) {
            let entry = layout.ctab_ptr + 8 + ordinal * 8;
            let value = if ct_flags & 0x8000 != 0 {
                ordinal as u16
            } else {
                bus.read_word(entry)
            };
            if value == u16::from(source_index) {
                return Some([
                    bus.read_word(entry + 2),
                    bus.read_word(entry + 4),
                    bus.read_word(entry + 6),
                ]);
            }
        }
        None
    }

    fn dialog_cicn_pixel_index(
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

    fn draw_cicn_icon(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        icon_ptr: u32,
    ) -> bool {
        let Some(layout) = Self::dialog_cicn_layout(bus, icon_ptr) else {
            return false;
        };
        let (screen_base, row_bytes, screen_width, screen_height, screen_pixel_size) =
            self.get_screen_params();
        let dst_w = right - left;
        let dst_h = bottom - top;
        if dst_w <= 0 || dst_h <= 0 {
            return false;
        }
        // Color-table indices belong to the icon PixMap, not the destination
        // device. Resolve each distinct source index through RGB only once.
        let mut palette_translation = [None; 256];

        for dy in 0..dst_h {
            let sy = (dy as i32 * i32::from(layout.height) / i32::from(dst_h)) as u32;
            let dst_y = top + dy;
            for dx in 0..dst_w {
                let sx = (dx as i32 * i32::from(layout.width) / i32::from(dst_w)) as u32;
                let dst_x = left + dx;
                let mask_byte =
                    bus.read_byte(layout.mask_data_ptr + sy * layout.mask_row_bytes + sx / 8);
                if (mask_byte & (0x80 >> (sx % 8))) == 0 {
                    continue;
                }

                if screen_pixel_size == 8 && layout.pixel_size >= 2 {
                    let Some(source_index) = Self::dialog_cicn_pixel_index(
                        bus,
                        layout.pixel_data_ptr,
                        layout.pm_row_bytes,
                        layout.pixel_size,
                        sx,
                        sy,
                    ) else {
                        continue;
                    };
                    let pixel_index = if let Some(mapped) =
                        palette_translation[usize::from(source_index)]
                    {
                        mapped
                    } else {
                        let mapped = Self::dialog_cicn_rgb_for_index(bus, &layout, source_index)
                            .and_then(|rgb| Self::fb_pixel_index_for_rgb(bus, rgb))
                            .unwrap_or(source_index);
                        palette_translation[usize::from(source_index)] = Some(mapped);
                        mapped
                    };
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
                let Some(pixel_index) = Self::dialog_cicn_pixel_index(
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
                    dst_x,
                    dst_y,
                    pixel_index != 0,
                );
            }
        }
        true
    }

    /// Draw a 32x32 1-bit ICON resource, scaled to fit the display rect.
    /// Inside Macintosh Volume I, I-205
    fn draw_icon(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        icon_ptr: u32,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let dst_w = right - left;
        let dst_h = bottom - top;
        // ICON is 32x32 pixels, 1 bit per pixel, 4 bytes per row = 128 bytes
        for row in 0..32i16 {
            let row_data = bus.read_long(icon_ptr + (row as u32) * 4);
            for col in 0..32i16 {
                let bit = (row_data >> (31 - col)) & 1;
                if bit != 0 {
                    // Scale to destination rect
                    let dx = left + col * dst_w / 32;
                    let dy = top + row * dst_h / 32;
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        dx,
                        dy,
                        true,
                    );
                }
            }
        }
    }

    /// Invert the pixels within a button rect (for flash animation).
    pub(crate) fn invert_button_rect(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();

        let y_start = (top as i32 + 1).clamp(0, screen_height as i32);
        let y_end = (bottom as i32 - 1).clamp(0, screen_height as i32);
        let x_start = (left as i32 + 1).clamp(0, screen_width as i32);
        let x_end = (right as i32 - 1).clamp(0, screen_width as i32);
        if y_start >= y_end || x_start >= x_end {
            return;
        }

        for y in y_start..y_end {
            for x in x_start..x_end {
                if pixel_size == 8 {
                    let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                    bus.invert_screen_byte(addr);
                } else {
                    let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
                    let bit = 7 - (x as u32 % 8);
                    let addr = screen_base + byte_offset;
                    let b = bus.read_byte(addr);
                    bus.write_byte(addr, b ^ (1 << bit));
                }
            }
        }
    }

    fn invert_rect_exact(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();

        let y_start = (top as i32).clamp(0, screen_height as i32);
        let y_end = (bottom as i32).clamp(0, screen_height as i32);
        let x_start = (left as i32).clamp(0, screen_width as i32);
        let x_end = (right as i32).clamp(0, screen_width as i32);
        if y_start >= y_end || x_start >= x_end {
            return;
        }

        for y in y_start..y_end {
            for x in x_start..x_end {
                if pixel_size == 8 {
                    let addr = screen_base + (y as u32) * row_bytes + (x as u32);
                    bus.invert_screen_byte(addr);
                } else {
                    let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
                    let bit = 7 - (x as u32 % 8);
                    let addr = screen_base + byte_offset;
                    let b = bus.read_byte(addr);
                    bus.write_byte(addr, b ^ (1 << bit));
                }
            }
        }
    }

    /// Hit-test a point against dialog items. Returns 1-based item number or 0.
    fn dialog_item_hit_test(
        &self,
        bus: &MacMemoryBus,
        items: &[DialogItem],
        bounds: (i16, i16, i16, i16),
        screen_v: i16,
        screen_h: i16,
        popup_original_rects: &std::collections::HashMap<(u32, i16), (i16, i16, i16, i16)>,
        dialog_ptr: u32,
    ) -> i16 {
        let (top, left, _, _) = bounds;
        let local_v = screen_v - top;
        let local_h = screen_h - left;
        for (i, item) in items.iter().enumerate() {
            let item_no = (i + 1) as i16;
            // For popup userItems, use the original DITL rect (before SetDItem
            // narrowed it) so clicks on the full popup area register.
            let mut rect = popup_original_rects
                .get(&(dialog_ptr, item_no))
                .copied()
                .unwrap_or(item.rect);
            let base_type = item.item_type & 0x7F;
            if (4..=7).contains(&base_type) {
                if let Some(ctrl_handle) = self.dialog_control_handle_for_item(dialog_ptr, item_no)
                {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    if ctrl_ptr != 0 && bus.read_byte(ctrl_ptr + 16) != 0 {
                        rect = (
                            bus.read_word(ctrl_ptr + 8) as i16,
                            bus.read_word(ctrl_ptr + 10) as i16,
                            bus.read_word(ctrl_ptr + 12) as i16,
                            bus.read_word(ctrl_ptr + 14) as i16,
                        );
                    }
                }
            }
            let (it, il, ib, ir) = rect;
            if local_v >= it && local_v < ib && local_h >= il && local_h < ir {
                return item_no;
            }
        }
        0
    }

    fn dialog_button_hit_test(
        items: &[DialogItem],
        bounds: (i16, i16, i16, i16),
        screen_v: i16,
        screen_h: i16,
    ) -> i16 {
        for (i, item) in items.iter().enumerate() {
            let base_type = item.item_type & 0x7F;
            let is_disabled = (item.item_type & 0x80) != 0;
            if base_type != 4 || is_disabled {
                continue;
            }
            let (top, left, bottom, right) = Self::dialog_item_screen_rect(bounds, item.rect);
            if screen_v >= top && screen_v < bottom && screen_h >= left && screen_h < right {
                return (i + 1) as i16;
            }
        }
        0
    }

    fn is_plain_modal_user_item(
        &self,
        tracking: &DialogTrackingState,
        item_no: i16,
        item: &DialogItem,
    ) -> bool {
        let base_type = item.item_type & 0x7F;
        let is_disabled = (item.item_type & 0x80) != 0;
        base_type == 0
            && !is_disabled
            && item.proc_ptr == 0
            && !tracking.game_managed
            && !self
                .dialog_item_popup_menus
                .contains_key(&(tracking.dialog_ptr, item_no))
            && !self
                .dialog_popup_candidate_items
                .contains(&(tracking.dialog_ptr, item_no))
    }

    fn dialog_plain_user_item_hit_test(
        &self,
        tracking: &DialogTrackingState,
        screen_v: i16,
        screen_h: i16,
    ) -> i16 {
        for (i, item) in tracking.items.iter().enumerate() {
            let item_no = (i + 1) as i16;
            if !self.is_plain_modal_user_item(tracking, item_no, item) {
                continue;
            }
            let (top, left, bottom, right) =
                Self::dialog_item_screen_rect(tracking.bounds, item.rect);
            if screen_v >= top && screen_v < bottom && screen_h >= left && screen_h < right {
                return item_no;
            }
        }
        0
    }

    pub(crate) fn pending_dialog_plain_user_item_mouse_down(&self) -> bool {
        let Some(tracking) = self.dialog_tracking.as_ref() else {
            return false;
        };
        if tracking.active_user_item.is_some() {
            return true;
        }
        let Some(event) = self
            .event_queue
            .iter()
            .find(|event| matches!(event.what, 1 | 2 | 3 | 4 | 6))
        else {
            return false;
        };
        event.what == 1
            && self.dialog_plain_user_item_hit_test(tracking, event.where_v, event.where_h) > 0
    }

    pub(crate) fn mouse_down_over_dialog_button(&self) -> bool {
        if !self.input_state.mouse_button {
            return false;
        }
        let Some(tracking) = self.dialog_tracking.as_ref() else {
            return false;
        };
        Self::dialog_button_hit_test(
            &tracking.items,
            tracking.bounds,
            self.input_state.mouse_pos.0,
            self.input_state.mouse_pos.1,
        ) > 0
    }

    pub(crate) fn mouse_down_over_dialog_plain_user_item(&self) -> bool {
        if !self.input_state.mouse_button {
            return false;
        }
        let Some(tracking) = self.dialog_tracking.as_ref() else {
            return false;
        };
        tracking.active_user_item.is_some()
            || self.dialog_plain_user_item_hit_test(tracking, self.input_state.mouse_pos.0, self.input_state.mouse_pos.1)
                > 0
    }

    fn read_guest_event_record(bus: &MacMemoryBus, event_ptr: u32) -> (u16, u32, i16, i16, u16) {
        if event_ptr == 0 {
            return (0, 0, 0, 0, 0);
        }
        (
            bus.read_word(event_ptr),
            bus.read_long(event_ptr + 2),
            bus.read_word(event_ptr + 10) as i16,
            bus.read_word(event_ptr + 12) as i16,
            bus.read_word(event_ptr + 14),
        )
    }

    fn front_dialog_ptr(&self) -> Option<u32> {
        let dialog_ptr = self.front_window;
        if dialog_ptr != 0 && self.dialog_items.contains_key(&dialog_ptr) {
            Some(dialog_ptr)
        } else {
            None
        }
    }

    fn front_retained_modal_dialog(
        &self,
        bus: &MacMemoryBus,
    ) -> Option<(u32, (i16, i16, i16, i16))> {
        if self.dialog_tracking.is_some() {
            return None;
        }
        let dialog_ptr = self.front_dialog_ptr()?;
        // This retained-click path is only for dialogs whose event handling
        // has already entered our ModalDialog HLE and returned with the
        // dialog still visible. Dialogs created by GetNewDialog/NewDialog
        // but handled by the application through WaitNextEvent, DialogSelect,
        // or custom Control/TextEdit/Window Manager code must keep receiving
        // their queued events so the app can decide how to respond.
        // Macintosh Toolbox Essentials 1992, pp. 6-136, 6-138..6-141.
        if !self.dialog_modal_entered.contains(&dialog_ptr) {
            return None;
        }
        if !self.window_visible(bus, dialog_ptr) {
            return None;
        }
        let proc_id = self.dialog_window_proc_id(bus, dialog_ptr);
        // Dialog-box WDEFs 1/2/3 are modal-box variants; 5 is the
        // movable modal dialog. noGrowDocProc (4) is modeless and must
        // continue to pass events to the application.
        // Inside Macintosh Volume I, I-273; Macintosh Toolbox Essentials
        // 1992, p. 6-137.
        if !matches!(proc_id, 1 | 2 | 3 | 5) {
            return None;
        }
        Some((dialog_ptr, Self::dialog_screen_bounds(bus, dialog_ptr)))
    }

    fn front_app_owned_modal_dialog(
        &self,
        bus: &MacMemoryBus,
    ) -> Option<(u32, (i16, i16, i16, i16))> {
        if self.dialog_tracking.is_some() || self.retained_modal_dialog_click.is_some() {
            return None;
        }
        let dialog_ptr = self.front_dialog_ptr()?;
        if self.dialog_modal_entered.contains(&dialog_ptr) || !self.window_visible(bus, dialog_ptr)
        {
            return None;
        }
        let proc_id = self.dialog_window_proc_id(bus, dialog_ptr);
        if !matches!(proc_id, 1 | 2 | 3 | 5) {
            return None;
        }
        Some((dialog_ptr, Self::dialog_screen_bounds(bus, dialog_ptr)))
    }

    pub(crate) fn begin_app_owned_modal_dialog_button_tracking(
        &mut self,
        bus: &mut MacMemoryBus,
        event: &QueuedEvent,
    ) {
        if event.what != 1 {
            return;
        }
        let Some((dialog_ptr, bounds)) = self.front_app_owned_modal_dialog(bus) else {
            return;
        };
        if !Self::dialog_contains_screen_point(bounds, event.where_v, event.where_h) {
            return;
        }

        let Some(mut items) = self.dialog_items.get(&dialog_ptr).cloned() else {
            return;
        };
        Self::refresh_ditl_proc_ptrs(bus, dialog_ptr, &mut items);
        let mut hit = self.dialog_item_hit_test(
            bus,
            &items,
            bounds,
            event.where_v,
            event.where_h,
            &self.dialog_popup_original_rects,
            dialog_ptr,
        );
        if hit <= 0 {
            hit = Self::dialog_button_hit_test(&items, bounds, event.where_v, event.where_h);
        }
        self.dialog_items.insert(dialog_ptr, items.clone());
        if hit <= 0 {
            return;
        }

        let item = &items[(hit - 1) as usize];
        let base_type = item.item_type & 0x7F;
        let is_disabled = (item.item_type & 0x80) != 0;
        if base_type != 4 || is_disabled {
            return;
        }

        // Some games create a modal DLOG and run their own WaitNextEvent
        // loop. They still expect the Dialog Manager's standard push-button
        // press/release affordance for modal WDEFs, but the mouseDown must
        // remain deliverable to app code. Track only enabled DITL buttons and
        // finish the press on mouseUp; DialogSelect cancels this state if the
        // app explicitly asks the Dialog Manager to handle the event.
        // Macintosh Toolbox Essentials 1992, pp. 6-136, 6-138..6-141.
        let rect = Self::dialog_item_screen_rect(bounds, item.rect);
        self.invert_button_rect(bus, rect.0, rect.1, rect.2, rect.3);
        self.retained_modal_dialog_click = Some(RetainedModalDialogClickState {
            dialog_ptr,
            item_no: hit,
            rect: item.rect,
            title: item.text.clone(),
            is_default: false,
            highlighted: true,
            delivered_to_app: true,
        });
    }

    fn cancel_app_owned_modal_dialog_button_tracking(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
    ) {
        let should_cancel = self
            .retained_modal_dialog_click
            .as_ref()
            .is_some_and(|click| click.dialog_ptr == dialog_ptr && click.delivered_to_app);
        if !should_cancel {
            return;
        }
        let Some(click) = self.retained_modal_dialog_click.take() else {
            return;
        };
        if click.item_no > 0 && click.highlighted {
            let bounds = Self::dialog_screen_bounds(bus, click.dialog_ptr);
            let rect = Self::dialog_item_screen_rect(bounds, click.rect);
            self.invert_button_rect(bus, rect.0, rect.1, rect.2, rect.3);
        }
    }

    fn dialog_from_window_event(&self, what: u16, message: u32) -> Option<u32> {
        match what {
            6 | 8 if self.dialog_items.contains_key(&message) => Some(message),
            _ => self.front_dialog_ptr(),
        }
    }

    fn dialog_contains_screen_point(bounds: (i16, i16, i16, i16), v: i16, h: i16) -> bool {
        v >= bounds.0 && v < bounds.2 && h >= bounds.1 && h < bounds.3
    }

    fn point_in_screen_rect(v: i16, h: i16, rect: (i16, i16, i16, i16)) -> bool {
        v >= rect.0 && v < rect.2 && h >= rect.1 && h < rect.3
    }

    fn close_dialog_window<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        dialog_ptr: u32,
        dispose_storage: bool,
    ) {
        let retained_modal_is_logical_front = self.dialog_modal_entered.contains(&dialog_ptr)
            && self
                .window_stack
                .last()
                .is_some_and(|(previous, _, _, _)| *previous == self.front_window);
        let was_front = self.front_window == dialog_ptr || retained_modal_is_logical_front;
        let was_visible = self.window_visible(bus, dialog_ptr);
        let windows_behind = self.visible_windows_behind(bus, dialog_ptr);
        let retained_visible_bounds = self
            .dialog_visible_snapshots
            .get(&dialog_ptr)
            .map(|snapshot| snapshot.bounds);
        let dialog_record_bounds = if dialog_ptr != 0 && self.dialog_items.contains_key(&dialog_ptr)
        {
            Self::dialog_screen_bounds(bus, dialog_ptr)
        } else {
            self.window_bounds
        };
        // A movable modal dialog's structure includes its title bar above the
        // DialogRecord content bounds. CloseDialog has CloseWindow semantics,
        // so PaintBehind must expose the entire structure, not only the port;
        // otherwise the window behind can never repaint the former title-bar
        // strip. Macintosh Toolbox Essentials (1992), pp. 4-89, 6-119..6-120.
        let exposed_rect = self
            .window_structure_rect(bus, dialog_ptr)
            .or(retained_visible_bounds)
            .unwrap_or(dialog_record_bounds);
        let initial_draw_deferred = self.dialog_initial_draw_deferred.remove(&dialog_ptr);
        let retained_stale_saved_under = self
            .dialog_saved_pixels
            .get(&dialog_ptr)
            .is_some_and(|saved| self.saved_pixels_are_mostly_logical_white(saved));
        let mut restored_stale_saved_under = false;

        if was_front && was_visible && !initial_draw_deferred {
            if let Some(saved) = self.dialog_saved_pixels.remove(&dialog_ptr) {
                restored_stale_saved_under = self.saved_pixels_are_mostly_logical_white(&saved);
                self.restore_dialog_pixels(bus, dialog_record_bounds, &saved);
            }
        } else {
            self.dialog_saved_pixels.remove(&dialog_ptr);
        }

        self.dialog_visible_snapshots.remove(&dialog_ptr);
        self.dialog_modal_entered.remove(&dialog_ptr);
        self.dialog_cdef_draw_pending_snapshot.remove(&dialog_ptr);
        self.dialog_cdefs_initially_drawn.remove(&dialog_ptr);
        if !was_front && retained_stale_saved_under && self.screen_is_hidden_menu_game_surface(bus)
        {
            let _ = self.restore_dialog_exposure_from_fullscreen_offscreen_port(bus, exposed_rect);
        }
        if self.pending_modal_button_dispose_dialog == Some(dialog_ptr) {
            self.pending_modal_button_dispose_dialog = None;
        }
        if self
            .retained_modal_dialog_click
            .as_ref()
            .is_some_and(|click| click.dialog_ptr == dialog_ptr)
        {
            self.retained_modal_dialog_click = None;
        }

        if was_front && self.pending_modal_dialog_mouse_up {
            // A ModalDialog-owned mouseDown and its matching mouseUp are one
            // press lifecycle (Macintosh Toolbox Essentials 1992, pp. 2-33,
            // 5-100). If the application disposes that dialog before release,
            // discard the already-handled down event so the newly exposed
            // dialog cannot receive the same press.
            if let Some(owned) = self.pending_modal_dialog_mouse_down.as_ref() {
                if let Some(idx) = self.event_queue.iter().rposition(|event| {
                    event.what == owned.what
                        && event.message == owned.message
                        && event.where_v == owned.where_v
                        && event.where_h == owned.where_h
                        && event.modifiers == owned.modifiers
                }) {
                    self.event_queue.remove(idx);
                }
            }
            self.input_state.with_mut(|state| state.mouse_button = false);
            self.adb.note_mouse_state(self.input_state.mouse_pos, false);
            bus.write_byte(0x0172, 0x80);
        }

        self.dispose_dialog_owned_items(bus, dialog_ptr);
        if dispose_storage {
            self.dispose_dialog_record_and_item_list(bus, dialog_ptr);
        }

        self.untrack_window(bus, dialog_ptr);
        if was_visible {
            self.invalidate_exposed_windows(bus, &windows_behind, Some(exposed_rect));
        }

        if was_front {
            if let Some((mut prev_window, mut prev_bounds, prev_proc_id, prev_title)) =
                self.window_stack.pop()
            {
                if prev_window == 0 && self.front_window != 0 {
                    // The saved dialog state can legitimately contain NIL
                    // when the dialog was created before the application
                    // opened its document window. `untrack_window` has
                    // already promoted the first visible remaining window;
                    // use that promotion instead of restoring NIL and
                    // leaving FrontWindow inconsistent with the window list.
                    prev_window = self.front_window;
                    self.set_current_port_state(bus, cpu, prev_window, None);
                    self.sync_cached_front_window_render_state(bus);
                    prev_bounds = self.window_bounds;
                } else {
                    self.set_current_port_state(bus, cpu, prev_window, None);
                    self.front_window = prev_window;
                    self.window_bounds = prev_bounds;
                    self.window_proc_id = prev_proc_id;
                    self.window_title = prev_title;
                }
                if prev_window != 0 {
                    bus.write_byte(prev_window + 111, 0xFF);
                    if self.window_visible(bus, prev_window) {
                        self.blit_window_to_screen(bus);
                        self.blit_large_manual_cport_to_screen(bus);
                    }
                    if !self.event_queue.iter().any(|event| {
                        event.what == 8
                            && event.message == prev_window
                            && (event.modifiers & 1) != 0
                    }) {
                        self.queue_window_activation_event(bus, prev_window, true);
                    }
                    self.draw_single_window_chrome_inline(bus, prev_window, true);
                    if was_visible {
                        let exposed_local = (
                            exposed_rect.0.saturating_sub(prev_bounds.0),
                            exposed_rect.1.saturating_sub(prev_bounds.1),
                            exposed_rect.2.saturating_sub(prev_bounds.0),
                            exposed_rect.3.saturating_sub(prev_bounds.1),
                        );
                        self.invalidate_window_rect(bus, prev_window, exposed_local);
                        self.queue_window_update_event(prev_window);
                    }
                }
                let should_repair_stale_exposure = restored_stale_saved_under
                    && if prev_window != 0 {
                        self.promoted_window_is_fullscreen_game_surface(bus, prev_bounds)
                    } else {
                        self.screen_is_hidden_menu_game_surface(bus)
                    };
                if should_repair_stale_exposure {
                    let _ = self
                        .restore_dialog_exposure_from_fullscreen_offscreen_port(bus, exposed_rect);
                }
            }
        }
    }

    fn dialog_saved_white_like_count(&self, saved: &[u8]) -> usize {
        let (_, _, _, _, pixel_size) = self.get_screen_params();
        match pixel_size {
            8 => saved
                .iter()
                .filter(|&&idx| {
                    let [r, g, b] = self.device_clut[idx as usize];
                    r >= 0xEEEE && g >= 0xEEEE && b >= 0xEEEE
                })
                .count(),
            _ => saved.iter().filter(|&&byte| byte == 0).count(),
        }
    }

    fn saved_pixels_are_mostly_logical_white(&self, saved: &[u8]) -> bool {
        let (_, _, _, _, pixel_size) = self.get_screen_params();
        pixel_size == 8
            && !saved.is_empty()
            && self
                .dialog_saved_white_like_count(saved)
                .saturating_mul(100)
                >= saved.len().saturating_mul(80)
    }

    fn promoted_window_is_fullscreen_game_surface(
        &self,
        bus: &MacMemoryBus,
        bounds: (i16, i16, i16, i16),
    ) -> bool {
        let (_, _, screen_w, screen_h, _) = self.get_screen_params();
        let menu_bar_height = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        bounds.0 <= 0
            && bounds.1 <= 0
            && bounds.2 >= screen_h
            && bounds.3 >= screen_w
            && (self.menu_bar_hidden || self.fullscreen_locked || menu_bar_height == 0)
    }

    pub(super) fn screen_is_hidden_menu_game_surface(&self, bus: &MacMemoryBus) -> bool {
        let menu_bar_height = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        self.menu_bar_hidden || self.fullscreen_locked || menu_bar_height == 0
    }

    fn restore_dialog_exposure_from_fullscreen_offscreen_port(
        &self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
    ) -> bool {
        let (screen_base, screen_row_bytes, screen_w, screen_h, pixel_size) =
            self.get_screen_params();
        if pixel_size != 8 || screen_w == 0 || screen_h == 0 {
            return false;
        }
        let (top, left, bottom, right) = Self::dialog_saved_pixel_rect(bounds);
        let top = top.max(0).min(screen_h);
        let left = left.max(0).min(screen_w);
        let bottom = bottom.max(0).min(screen_h);
        let right = right.max(0).min(screen_w);
        if top >= bottom || left >= right {
            return false;
        }

        let mut ports = Vec::new();
        for port in self
            .cport_ports
            .iter()
            .copied()
            .chain(self.gworld_devices.keys().copied())
        {
            if !ports.contains(&port) {
                ports.push(port);
            }
        }

        let mut best: Option<FullscreenOffscreenDialogRestoreCandidate> = None;
        for port in ports {
            let Some(mut candidate) =
                self.fullscreen_offscreen_dialog_restore_candidate(bus, port, screen_base)
            else {
                continue;
            };
            if candidate.top > 0
                || candidate.left > 0
                || candidate.bottom < screen_h
                || candidate.right < screen_w
            {
                continue;
            }
            let Some(score) = self.score_offscreen_dialog_restore_candidate(
                bus,
                candidate,
                (top, left, bottom, right),
            ) else {
                continue;
            };
            candidate.score = score;
            if best.map(|current| score > current.score).unwrap_or(true) {
                best = Some(candidate);
            }
        }

        let Some(candidate) = best else {
            return false;
        };

        let screen_ctab_handle = Self::gdevice_ctab_handle(bus, self.main_gdevice_handle);
        let src_ctab_seed = Self::ctab_seed(bus, candidate.ctab_handle);
        let dst_ctab_seed = Self::ctab_seed(bus, screen_ctab_handle);
        let src_clut = self.read_port_clut(bus, candidate.ctab_handle);
        let dst_clut = self.read_port_clut(bus, screen_ctab_handle);
        let hardware_palette_active = self.device_clut != self.color_manager_clut;
        let skip_canonical_to_screen = Self::uses_canonical_system_8bpp_clut(&src_clut);
        let translation = if candidate.ctab_handle != screen_ctab_handle
            && matches!(src_ctab_seed, Some(src_seed) if src_seed != 0)
            && src_ctab_seed != dst_ctab_seed
            && !skip_canonical_to_screen
            && !hardware_palette_active
        {
            Some(self.build_palette_translation(
                bus,
                &src_clut,
                &dst_clut,
                screen_ctab_handle,
                8,
                8,
            ))
        } else {
            None
        };

        let width = (right - left) as u32;
        for y in top..bottom {
            let src_y = (y - candidate.top) as u32;
            let src_x = (left - candidate.left) as u32;
            let dst_addr = screen_base + (y as u32) * screen_row_bytes + left as u32;
            let src_addr = candidate.base + src_y * candidate.row_bytes + src_x;
            if let Some(translation) = translation.as_ref() {
                for col in 0..width {
                    let src_idx = bus.read_byte(src_addr + col);
                    bus.write_byte(dst_addr + col, translation[src_idx as usize]);
                }
            } else {
                let row = bus.read_bytes(src_addr, width as usize);
                bus.write_bytes(dst_addr, &row);
            }
        }

        true
    }

    fn fullscreen_offscreen_dialog_restore_candidate(
        &self,
        bus: &MacMemoryBus,
        port: u32,
        screen_base: u32,
    ) -> Option<FullscreenOffscreenDialogRestoreCandidate> {
        if port == 0 || (bus.read_word(port + 6) & 0xC000) != 0xC000 {
            return None;
        }
        let pm_handle = bus.read_long(port + 2);
        if pm_handle == 0 {
            return None;
        }
        let pm_ptr = bus.read_long(pm_handle);
        if pm_ptr == 0 {
            return None;
        }
        let base = Self::offscreen_pixmap_base_ptr(bus, pm_ptr) & 0x3FFF_FFFF;
        let row_bytes = (bus.read_word(pm_ptr + 4) & 0x3FFF) as u32;
        let top = bus.read_word(pm_ptr + 6) as i16;
        let left = bus.read_word(pm_ptr + 8) as i16;
        let bottom = bus.read_word(pm_ptr + 10) as i16;
        let right = bus.read_word(pm_ptr + 12) as i16;
        let pixel_size = bus.read_word(pm_ptr + 32);
        if base == 0
            || base == screen_base
            || pixel_size != 8
            || bottom <= top
            || right <= left
            || row_bytes < (right - left) as u32
        {
            return None;
        }
        Some(FullscreenOffscreenDialogRestoreCandidate {
            base,
            row_bytes,
            top,
            left,
            bottom,
            right,
            ctab_handle: bus.read_long(pm_ptr + 42),
            score: 0,
        })
    }

    fn score_offscreen_dialog_restore_candidate(
        &self,
        bus: &MacMemoryBus,
        candidate: FullscreenOffscreenDialogRestoreCandidate,
        rect: (i16, i16, i16, i16),
    ) -> Option<u32> {
        let (top, left, bottom, right) = rect;
        let width = right - left;
        let height = bottom - top;
        if width <= 0 || height <= 0 {
            return None;
        }
        let step_x = (width as u32 / 32).max(1);
        let step_y = (height as u32 / 24).max(1);
        let mut total = 0u32;
        let mut visible = 0u32;
        let mut white_like = 0u32;
        let mut black_like = 0u32;
        let mut y = top as u32 + step_y / 2;
        while y < bottom as u32 {
            let mut x = left as u32 + step_x / 2;
            while x < right as u32 {
                let src_y = y as i16 - candidate.top;
                let src_x = x as i16 - candidate.left;
                if src_y >= 0 && src_x >= 0 {
                    let idx = bus.read_byte(
                        candidate.base + src_y as u32 * candidate.row_bytes + src_x as u32,
                    ) as usize;
                    let [r, g, b] = self.device_clut[idx];
                    total += 1;
                    if r > 0x1111 || g > 0x1111 || b > 0x1111 {
                        visible += 1;
                    }
                    if r >= 0xEEEE && g >= 0xEEEE && b >= 0xEEEE {
                        white_like += 1;
                    }
                    if r <= 0x1111 && g <= 0x1111 && b <= 0x1111 {
                        black_like += 1;
                    }
                }
                x += step_x;
            }
            y += step_y;
        }
        if total == 0
            || visible < 8
            || white_like.saturating_mul(100) >= total.saturating_mul(70)
            || black_like.saturating_mul(100) >= total.saturating_mul(98)
        {
            return None;
        }
        Some(visible.saturating_mul(2).saturating_sub(white_like))
    }

    fn resolve_dispos_dialog_ptr_after_modal_button_hit(
        &mut self,
        requested_dialog_ptr: u32,
    ) -> u32 {
        let Some(dialog_ptr) = self.pending_modal_button_dispose_dialog.take() else {
            return requested_dialog_ptr;
        };

        if requested_dialog_ptr != 0
            && requested_dialog_ptr != dialog_ptr
            && !self.dialog_items.contains_key(&requested_dialog_ptr)
            && !self.window_list.contains(&requested_dialog_ptr)
            && self.front_window == dialog_ptr
            && self.dialog_modal_entered.contains(&dialog_ptr)
            && self.dialog_items.contains_key(&dialog_ptr)
        {
            eprintln!(
                "[TRAP] DisposDialog(${:08X}) recovered front ModalDialog button target ${:08X}",
                requested_dialog_ptr, dialog_ptr
            );
            dialog_ptr
        } else {
            requested_dialog_ptr
        }
    }

    pub(crate) fn consume_retained_modal_dialog_event<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        event: &QueuedEvent,
    ) -> bool {
        match event.what {
            1 => {
                let Some((dialog_ptr, bounds)) = self.front_retained_modal_dialog(bus) else {
                    return false;
                };

                if !Self::dialog_contains_screen_point(bounds, event.where_v, event.where_h) {
                    // Modal dialogs own the mouse while visible. A real Dialog
                    // Manager click outside the box beeps and does not pass
                    // through to windows behind it.
                    // Inside Macintosh Volume I, I-418; Macintosh Toolbox
                    // Essentials 1992, p. 6-136.
                    self.retained_modal_dialog_click = Some(RetainedModalDialogClickState {
                        dialog_ptr,
                        item_no: 0,
                        rect: (0, 0, 0, 0),
                        title: String::new(),
                        is_default: false,
                        highlighted: false,
                        delivered_to_app: false,
                    });
                    return true;
                }

                let Some(mut items) = self.dialog_items.get(&dialog_ptr).cloned() else {
                    return false;
                };
                Self::refresh_ditl_proc_ptrs(bus, dialog_ptr, &mut items);
                let mut hit = self.dialog_item_hit_test(
                    bus,
                    &items,
                    bounds,
                    event.where_v,
                    event.where_h,
                    &self.dialog_popup_original_rects,
                    dialog_ptr,
                );
                if hit <= 0 {
                    hit =
                        Self::dialog_button_hit_test(&items, bounds, event.where_v, event.where_h);
                }
                self.dialog_items.insert(dialog_ptr, items.clone());

                if hit <= 0 {
                    self.retained_modal_dialog_click = Some(RetainedModalDialogClickState {
                        dialog_ptr,
                        item_no: 0,
                        rect: (0, 0, 0, 0),
                        title: String::new(),
                        is_default: false,
                        highlighted: false,
                        delivered_to_app: false,
                    });
                    return true;
                }

                let item = &items[(hit - 1) as usize];
                let base_type = item.item_type & 0x7F;
                let is_disabled = (item.item_type & 0x80) != 0;
                if base_type == 4 && !is_disabled {
                    let rect = Self::dialog_item_screen_rect(bounds, item.rect);
                    let is_default = hit == bus.read_word(dialog_ptr + 168) as i16;
                    self.draw_dialog_button_highlight_state(
                        bus, rect, &item.text, is_default, true,
                    );
                    self.retained_modal_dialog_click = Some(RetainedModalDialogClickState {
                        dialog_ptr,
                        item_no: hit,
                        rect: item.rect,
                        title: item.text.clone(),
                        is_default,
                        highlighted: true,
                        delivered_to_app: false,
                    });
                } else {
                    self.retained_modal_dialog_click = Some(RetainedModalDialogClickState {
                        dialog_ptr,
                        item_no: 0,
                        rect: (0, 0, 0, 0),
                        title: String::new(),
                        is_default: false,
                        highlighted: false,
                        delivered_to_app: false,
                    });
                }
                true
            }
            2 => {
                let Some(click) = self.retained_modal_dialog_click.take() else {
                    return false;
                };
                if click.item_no > 0 {
                    let bounds = Self::dialog_screen_bounds(bus, click.dialog_ptr);
                    let rect = Self::dialog_item_screen_rect(bounds, click.rect);
                    if click.highlighted {
                        self.draw_dialog_button_highlight_state(
                            bus,
                            rect,
                            &click.title,
                            click.is_default,
                            false,
                        );
                    }
                    if self.front_window == click.dialog_ptr
                        && Self::point_in_screen_rect(event.where_v, event.where_h, rect)
                    {
                        self.close_dialog_window(bus, cpu, click.dialog_ptr, true);
                        self.capture_gui_frame(
                            bus,
                            &format!("retained_modal_dialog_button_{}", click.item_no),
                        );
                    }
                }
                !click.delivered_to_app
            }
            _ => false,
        }
    }

    fn consume_dialog_mouse_up(&mut self) -> bool {
        let next_mouse_event = self
            .event_queue
            .iter()
            .position(|event| matches!(event.what, 1 | 2));
        if let Some(idx) = next_mouse_event.filter(|idx| self.event_queue[*idx].what == 2) {
            self.event_queue.remove(idx);
            true
        } else {
            false
        }
    }

    fn consume_or_retain_dialog_mouse_up(&mut self, mouse_down: &QueuedEvent) {
        self.pending_modal_dialog_mouse_up = !self.consume_dialog_mouse_up();
        self.pending_modal_dialog_mouse_down = self
            .pending_modal_dialog_mouse_up
            .then(|| mouse_down.clone());
    }

    fn dialog_item_screen_rect(
        bounds: (i16, i16, i16, i16),
        rect: (i16, i16, i16, i16),
    ) -> (i16, i16, i16, i16) {
        let (it, il, ib, ir) = rect;
        (bounds.0 + it, bounds.1 + il, bounds.0 + ib, bounds.1 + ir)
    }

    fn rects_intersect(a: (i16, i16, i16, i16), b: (i16, i16, i16, i16)) -> bool {
        a.0 < b.2 && a.2 > b.0 && a.1 < b.3 && a.3 > b.1
    }

    fn dialog_item_intersects_bounds(bounds: (i16, i16, i16, i16), item: &DialogItem) -> bool {
        Self::rects_intersect(Self::dialog_item_screen_rect(bounds, item.rect), bounds)
    }

    fn dialog_is_game_managed(bounds: (i16, i16, i16, i16), items: &[DialogItem]) -> bool {
        let mut has_visible_item = false;
        for item in items {
            if !Self::dialog_item_intersects_bounds(bounds, item) {
                continue;
            }
            has_visible_item = true;
            if (item.item_type & 0x7F) != 0 {
                return false;
            }
        }
        has_visible_item
    }

    fn start_dialog_button_flash(
        &mut self,
        bus: &mut MacMemoryBus,
        bounds: (i16, i16, i16, i16),
        item_no: i16,
        rect: (i16, i16, i16, i16),
        title: &str,
        is_default: bool,
        highlighted: bool,
    ) {
        if !highlighted {
            let screen_rect = Self::dialog_item_screen_rect(bounds, rect);
            self.draw_dialog_button_highlight_state(bus, screen_rect, title, is_default, true);
        }
        if let Some(t) = self.dialog_tracking.as_mut() {
            t.flash_remaining = 6;
            t.flash_delay = 3;
            t.flash_item = item_no;
        }
    }

    fn dialog_control_handle_for_item(&self, dialog_ptr: u32, item_no: i16) -> Option<u32> {
        self.dialog_control_handles
            .iter()
            .find_map(|(&handle, &(dlg, item))| {
                if dlg == dialog_ptr && item == item_no {
                    Some(handle)
                } else {
                    None
                }
            })
    }

    fn dialog_control_inactive(&self, bus: &MacMemoryBus, dialog_ptr: u32, item_no: i16) -> bool {
        self.dialog_control_handle_for_item(dialog_ptr, item_no)
            .map(|handle| bus.read_long(handle))
            .is_some_and(|ctrl_ptr| ctrl_ptr != 0 && bus.read_byte(ctrl_ptr + 17) == 255)
    }

    fn dialog_control_visible(&self, bus: &MacMemoryBus, dialog_ptr: u32, item_no: i16) -> bool {
        let Some(control_handle) = self.dialog_control_handle_for_item(dialog_ptr, item_no) else {
            // Some internal drawing tests and synthesized shells do not
            // materialize ControlRecords. Preserve their DITL-only fallback.
            return true;
        };
        let control_ptr = bus.read_long(control_handle);
        // contrlVis is nonzero for every visible control; HideControl writes
        // zero, and DrawDialog redraws dialog controls through the Control
        // Manager. Inside Macintosh Volume I, I-329 and I-417.
        control_ptr != 0 && bus.read_byte(control_ptr + 16) != 0
    }

    fn begin_dialog_popup_tracking(
        &mut self,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        item_no: i16,
    ) -> bool {
        let Some(ctrl_handle) = self.dialog_control_handle_for_item(dialog_ptr, item_no) else {
            return false;
        };
        let ctrl_ptr = bus.read_long(ctrl_handle);
        if ctrl_ptr == 0 {
            return false;
        }
        let proc_id = self.control_manager.proc_id(ctrl_ptr);
        if !Self::is_popup_menu_proc_id(proc_id) {
            return false;
        }
        let menu_id =
            self.popup_control_menu_id(bus, ctrl_ptr, bus.read_word(ctrl_ptr + 20) as i16);
        let Some(menu_idx) = self.menus.iter().rposition(|menu| menu.id == menu_id) else {
            return false;
        };

        let dropdown_rect = self.popup_control_dropdown_rect(bus, ctrl_ptr, menu_idx);
        let saved_pixels = self.save_dropdown_pixels(bus, dropdown_rect);
        if let Some(tracking) = self.dialog_tracking.as_mut() {
            tracking.active_popup = Some(DialogPopupTrackingState {
                item_no,
                ctrl_handle,
                ctrl_ptr,
                active_menu: menu_idx,
                highlighted_item: 0,
                saved_pixels,
                dropdown_rect,
            });
        } else {
            return false;
        }
        self.draw_menu_dropdown(bus, menu_idx, dropdown_rect);
        true
    }

    fn dialog_popup_item_at_point(&self, bus: &MacMemoryBus, mouse_x: i16, mouse_y: i16) -> i16 {
        let Some(popup) = self
            .dialog_tracking
            .as_ref()
            .and_then(|tracking| tracking.active_popup.as_ref())
        else {
            return 0;
        };
        let (top, left, bottom, right) = popup.dropdown_rect;
        if mouse_x < left || mouse_x >= right || mouse_y < top || mouse_y >= bottom {
            return 0;
        }
        let Some(menu) = self.menus.get(popup.active_menu) else {
            return 0;
        };
        let mut item_top = top + 1;
        for (item_idx, item) in menu.items.iter().enumerate() {
            let item_bottom = item_top + self.menu_item_height(bus, item);
            if mouse_y >= item_top && mouse_y < item_bottom {
                if item.text == "-" || !item.enabled {
                    return 0;
                }
                return item_idx as i16 + 1;
            }
            item_top = item_bottom;
        }
        0
    }

    fn handle_dialog_popup_tracking<C: CpuOps>(&mut self, cpu: &mut C, bus: &mut MacMemoryBus) {
        if self.input_state.mouse_button {
            let (mv, mh) = self.input_state.mouse_pos;
            let new_item = self.dialog_popup_item_at_point(bus, mh, mv);
            let old_item = self
                .dialog_tracking
                .as_ref()
                .and_then(|tracking| tracking.active_popup.as_ref())
                .map(|popup| popup.highlighted_item)
                .unwrap_or(0);
            if new_item != old_item {
                let popup_state = self
                    .dialog_tracking
                    .as_mut()
                    .and_then(|tracking| tracking.active_popup.as_mut())
                    .map(|popup| {
                        popup.highlighted_item = new_item;
                        (popup.active_menu, popup.dropdown_rect)
                    });
                if let Some((active_menu, dropdown_rect)) = popup_state {
                    self.draw_menu_dropdown(bus, active_menu, dropdown_rect);
                }
            }
            return;
        }

        let Some(popup) = self
            .dialog_tracking
            .as_mut()
            .and_then(|tracking| tracking.active_popup.take())
        else {
            return;
        };
        self.restore_dropdown_pixels(bus, popup.dropdown_rect, &popup.saved_pixels);
        self.consume_dialog_mouse_up();

        let selected_item = popup.highlighted_item;
        if selected_item <= 0 {
            return;
        }

        self.write_control_value(bus, popup.ctrl_handle, selected_item);
        self.draw_control(cpu, bus, popup.ctrl_ptr);

        if self.dialog_tracking.is_some() {
            let (dialog_ptr, edit_item, edit_text, items) = {
                let tracking = self.dialog_tracking.as_mut().unwrap();
                Self::sync_tracking_active_edit_item(tracking);
                (
                    tracking.dialog_ptr,
                    tracking.edit_item,
                    tracking.edit_text.clone(),
                    tracking.items.clone(),
                )
            };
            self.flush_dialog_edit_item_texts(bus, dialog_ptr, &items, edit_item, &edit_text);
        }
        if let Some((bounds, game_managed)) = self
            .dialog_tracking
            .as_ref()
            .map(|tracking| (tracking.bounds, tracking.game_managed))
        {
            if !game_managed {
                let rendered = self.save_dialog_pixels(bus, bounds);
                if let Some(tracking) = self.dialog_tracking.as_mut() {
                    tracking.rendered_pixels = rendered;
                    tracking.rendered_pixels_final = true;
                }
            }
        }

        let Some(saved) = self.dialog_tracking.take() else {
            return;
        };
        self.persist_visible_dialog_snapshot(bus, &saved);
        self.dialog_saved_pixels
            .insert(saved.dialog_ptr, saved.saved_pixels);
        if saved.item_hit_ptr != 0 {
            bus.write_word(saved.item_hit_ptr, popup.item_no as u16);
        }
        cpu.write_reg(Register::A7, saved.stack_ptr + 8);
    }

    fn handle_dialog_button_tracking(&mut self, bus: &mut MacMemoryBus) {
        let Some((
            dialog_ptr,
            bounds,
            mouse_down,
            item_no,
            rect,
            title,
            is_default,
            highlighted,
            item_type,
        )) = self.dialog_tracking.as_ref().and_then(|tracking| {
            tracking.active_button.as_ref().map(|button| {
                (
                    tracking.dialog_ptr,
                    tracking.bounds,
                    button.mouse_down.clone(),
                    button.item_no,
                    button.rect,
                    button.title.clone(),
                    button.is_default,
                    button.highlighted,
                    tracking
                        .items
                        .get(button.item_no.saturating_sub(1) as usize)
                        .map(|item| item.item_type)
                        .unwrap_or(4),
                )
            })
        })
        else {
            return;
        };

        let (top, left, bottom, right) = Self::dialog_item_screen_rect(bounds, rect);
        let (mouse_v, mouse_h) = self.input_state.mouse_pos;
        let inside = mouse_v >= top && mouse_v < bottom && mouse_h >= left && mouse_h < right;

        if self.input_state.mouse_button {
            if inside != highlighted {
                self.draw_dialog_button_highlight_state(
                    bus,
                    (top, left, bottom, right),
                    &title,
                    is_default,
                    inside,
                );
                if let Some(button) = self
                    .dialog_tracking
                    .as_mut()
                    .and_then(|tracking| tracking.active_button.as_mut())
                {
                    button.highlighted = inside;
                }
                self.record_modal_dialog_input_trace(
                    "tracking_update",
                    dialog_ptr,
                    bounds,
                    item_no,
                    Some(item_type),
                    Some(inside),
                    "pending",
                    if inside {
                        "button_highlighted"
                    } else {
                        "button_unhighlighted"
                    },
                );
            }
            return;
        }

        self.consume_or_retain_dialog_mouse_up(&mouse_down);
        if let Some(t) = self.dialog_tracking.as_mut() {
            t.active_button = None;
        }

        if inside {
            self.record_modal_dialog_input_trace(
                "release",
                dialog_ptr,
                bounds,
                item_no,
                Some(item_type),
                Some(true),
                "pending",
                "start_flash",
            );
            self.start_dialog_button_flash(
                bus,
                bounds,
                item_no,
                rect,
                &title,
                is_default,
                highlighted,
            );
        } else if highlighted {
            self.draw_dialog_button_highlight_state(
                bus,
                (top, left, bottom, right),
                &title,
                is_default,
                false,
            );
            self.record_modal_dialog_input_trace(
                "release",
                dialog_ptr,
                bounds,
                item_no,
                Some(item_type),
                Some(false),
                "pending",
                "button_no_selection",
            );
        } else {
            self.record_modal_dialog_input_trace(
                "release",
                dialog_ptr,
                bounds,
                item_no,
                Some(item_type),
                Some(false),
                "pending",
                "button_no_selection",
            );
        }
    }

    fn handle_dialog_user_item_tracking<C: CpuOps>(&mut self, cpu: &mut C, bus: &mut MacMemoryBus) {
        let Some((bounds, item_no, rect)) = self.dialog_tracking.as_ref().and_then(|tracking| {
            tracking
                .active_user_item
                .as_ref()
                .map(|user_item| (tracking.bounds, user_item.item_no, user_item.rect))
        }) else {
            return;
        };

        if self.input_state.mouse_button {
            return;
        }

        self.consume_dialog_mouse_up();
        if let Some(t) = self.dialog_tracking.as_mut() {
            t.active_user_item = None;
        }

        let (top, left, bottom, right) = Self::dialog_item_screen_rect(bounds, rect);
        let (mouse_v, mouse_h) = self.input_state.mouse_pos;
        let inside = mouse_v >= top && mouse_v < bottom && mouse_h >= left && mouse_h < right;
        if !inside {
            return;
        }

        let (dlg_ptr, edit_item, edit_text, items) = {
            let tracking = self.dialog_tracking.as_mut().unwrap();
            Self::sync_tracking_active_edit_item(tracking);
            (
                tracking.dialog_ptr,
                tracking.edit_item,
                tracking.edit_text.clone(),
                tracking.items.clone(),
            )
        };
        self.flush_dialog_edit_item_texts(bus, dlg_ptr, &items, edit_item, &edit_text);
        let saved = self.dialog_tracking.take().unwrap();
        self.persist_visible_dialog_snapshot(bus, &saved);
        self.dialog_saved_pixels
            .insert(saved.dialog_ptr, saved.saved_pixels);
        if saved.item_hit_ptr != 0 {
            bus.write_word(saved.item_hit_ptr, item_no as u16);
        }
        cpu.write_reg(Register::A7, saved.stack_ptr + 8);
    }

    pub(crate) fn dispatch_dialog<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        self.read_tick_count(bus);
        Some(match (is_tool, trap_num) {
            // ========== Dialog Manager ==========

            // InitDialogs ($A97B)
            // PROCEDURE InitDialogs(resumeProc: ProcPtr);
            // Inside Macintosh Volume I, I-411.
            //
            // Performs the 3 documented Dialog Manager init steps:
            //   1. Stores resumeProc into the ResumeProc low-mem
            //      global at $0A8C, for later access by the System
            //      Error Handler when a fatal system error occurs
            //      (NIL is the documented default — no resume).
            //   2. Installs the standard sound procedure (we store
            //      NIL into DABeeper at $0A9C since Systemless's HLE
            //      doesn't model menu-bar-blink sound; ErrorSound
            //      ($A98C) can override later).
            //   3. Passes empty strings to ParamText — implemented
            //      as zeroing the 4-handle DAStrings array at
            //      $0AA0 (16 bytes = 4 × Handle, all NIL meaning
            //      "no substitution" so dialog/alert text rendered
            //      with `^0`..`^3` escapes prints the raw escape).
            //
            // Also resets AlertStage at $0A9A to 0 so the first
            // Alert*-family call starts at stage 1 (per IM:I I-417
            // — InitDialogs is the canonical place to ensure
            // alert state is fresh per app launch). ANumber at
            // $0A98 is left untouched since IM:I I-423 only
            // documents it as "the resource ID of the last alert
            // that occurred" with no init contract.
            //
            // Pop = 4 bytes (resumeProc ProcPtr).
            // InitDialogs ($A97B): Per IM:I I-411: stores resumeProc at $0A8C (ResumeProc global), zeros DABeeper at $0A9C ("no sound" default per HLE), zeros AlertStage at $0A9A (so first Alert call starts at stage 1), zeros 16-byte DAStrings array at $0AA0..$0AAF (4 NIL ParamText handles); pops 4 bytes
            (true, 0x17B) => {
                let sp = cpu.read_reg(Register::A7);
                let resume_proc = bus.read_long(sp);
                use crate::memory::globals::addr;
                bus.write_long(addr::RESUME_PROC, resume_proc);
                bus.write_long(addr::DA_BEEPER, 0);
                bus.write_word(addr::ALERT_STAGE, 0);
                // Zero the 4-handle DAStrings array (16 bytes).
                for i in 0..4u32 {
                    bus.write_long(addr::DA_STRINGS + i * 4, 0);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // GetNewDialog ($A97C)
            // Creates a dialog from DLOG and DITL resources.
            // FUNCTION GetNewDialog (dialogID: INTEGER; dStorage: Ptr;
            //     behind: WindowPtr) : DialogPtr;
            // Inside Macintosh Volume I, I-424
            //
            // Stack: SP+0 behind(4), SP+4 dStorage(4), SP+8 dialogID(2),
            //        SP+10 result(4).
            // Honors `behind` at SP+0 via finish_dialog_creation —
            // see NewDialog/NewCDialog and window.rs apply_behind_parameter.
            // GetNewDialog ($A97C): Parses DLOG + DITL resources, creates DialogRecord, stores parsed items for ModalDialog/GetDItem; saves background pixels before drawing so ModalDialog can restore clean background on dismiss
            (true, 0x17C) => {
                let sp = cpu.read_reg(Register::A7);
                let behind = bus.read_long(sp);
                let storage_ptr = bus.read_long(sp + 4);
                let dialog_id = bus.read_word(sp + 8) as i16;
                eprintln!("[TRAP] GetNewDialog({})", dialog_id);

                // Look up DLOG resource
                let dlog_ptr = self
                    .find_or_load_resource_any(bus, *b"DLOG", dialog_id)
                    .map(|(_, ptr)| ptr);

                if let Some(dlog_data) = dlog_ptr {
                    let dlog_len = bus.get_alloc_size(dlog_data).unwrap_or(0);
                    let (raw_bounds, proc_id, visible, items_id, title, position) =
                        Self::parse_dlog(bus, dlog_data, dlog_len);
                    let mut bounds = raw_bounds;

                    // Look up DITL resource
                    let ditl_info = self
                        .find_or_load_resource_any(bus, *b"DITL", items_id)
                        .map(|(_, ptr)| ptr);

                    let Some(ditl_data) = ditl_info else {
                        eprintln!(
                            "[TRAP] GetNewDialog({}): DITL {} not found",
                            dialog_id, items_id
                        );
                        // MTE 1992 p. 6-114: GetNewDialog returns NIL if
                        // either the DLOG or item-list resource cannot be
                        // read. Resource Manager ResError reports
                        // resNotFound (-192) for missing resources.
                        bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                        bus.write_long(sp + 10, 0);
                        cpu.write_reg(Register::A7, sp + 10);
                        return Some(Ok(()));
                    };
                    bus.write_word(0x0A60, 0);
                    let ditl_len = bus.get_alloc_size(ditl_data).unwrap_or(0);
                    let items = Self::parse_ditl(bus, ditl_data, ditl_len);

                    // Apply System 7 DLOG positioning constants.
                    // Macintosh Toolbox Essentials 1992, pp. 4-125 to 4-126.
                    bounds = self.positioned_window_bounds(bus, bounds, position, proc_id);

                    eprintln!(
                        "[TRAP] GetNewDialog({}) bounds=({},{},{},{}) raw_bounds=({},{},{},{}) position=${:04X} len={} procID={} items={} title=\"{}\"",
                        dialog_id, bounds.0, bounds.1, bounds.2, bounds.3,
                        raw_bounds.0, raw_bounds.1, raw_bounds.2, raw_bounds.3,
                        position,
                        dlog_len,
                        proc_id, items.len(), title
                    );
                    for (i, item) in items.iter().enumerate() {
                        let base_type = item.item_type & 0x7F;
                        let type_name = match base_type {
                            4 => "button",
                            5 => "checkbox",
                            6 => "radio",
                            7 => "resCtrl",
                            8 => "statText",
                            16 => "editText",
                            32 => "icon",
                            64 => "picture",
                            0 => "userItem",
                            _ => "unknown",
                        };
                        if base_type == 32 || base_type == 64 {
                            eprintln!(
                                "[TRAP]   item {}: type={}({}) rect=({},{},{},{}) resID={}",
                                i + 1,
                                type_name,
                                item.item_type,
                                item.rect.0,
                                item.rect.1,
                                item.rect.2,
                                item.rect.3,
                                item.resource_id
                            );
                        } else {
                            eprintln!(
                                "[TRAP]   item {}: type={}({}) rect=({},{},{},{}) text=\"{}\"",
                                i + 1,
                                type_name,
                                item.item_type,
                                item.rect.0,
                                item.rect.1,
                                item.rect.2,
                                item.rect.3,
                                item.text
                            );
                        }
                    }

                    let items_handle = if let Some((_, ditl_handle_ptr)) =
                        self.find_or_load_resource_any(bus, *b"DITL", items_id)
                    {
                        let handle = bus.alloc(4);
                        bus.write_long(handle, ditl_handle_ptr);
                        Self::duplicate_handle_data(bus, handle)
                    } else {
                        0
                    };
                    // Matching DCTab and item color table resources are copied
                    // and associated before the initial visible shell is drawn.
                    // The ictb ID follows the DITL ID, not the DLOG ID.
                    // Macintosh Toolbox Essentials 1992, pp. 6-158 to 6-164.
                    let dialog_color_table = self.copy_dialog_color_table_resource(bus, dialog_id);
                    let dialog_item_color_table = dialog_color_table
                        .is_some()
                        .then(|| self.copy_dialog_item_color_table_resource(bus, items_id))
                        .flatten();
                    // Honor the DLOG resource's visible flag per IM:I I-424.
                    let dlg_ptr = self.finish_dialog_creation(
                        bus,
                        cpu,
                        storage_ptr,
                        bounds,
                        &title,
                        visible,
                        proc_id,
                        false,
                        0,
                        items_handle,
                        items,
                        dialog_color_table,
                        dialog_item_color_table,
                    );
                    // Install any 'pltt' resource whose id matches the
                    // dialog id onto the freshly-created window. This
                    // mirrors what GetNewWindow / NewCWindow do in
                    // window.rs and is what the real Window Manager
                    // does: a Window with an associated palette gets
                    // its palette activated when the window is created
                    // and made active. Without this hook, a dialog
                    // whose PICT items rely on an auto-installed
                    // palette (e.g. EV's landing-scene dialog id 1000)
                    // renders through whatever canonical CLUT happens
                    // to be live, producing palette-mismatched colour
                    // noise.  Inside Macintosh Volume VI, 20-12 to
                    // 20-13 (palette association and activation).
                    if dlg_ptr != 0 {
                        let palette = self.copy_palette_resource(bus, dialog_id);
                        if palette != 0 {
                            self.set_window_palette_association(dlg_ptr, palette, -0x2000);
                            self.activate_palette_for_window(bus, dlg_ptr);
                        }
                        self.apply_behind_parameter(bus, dlg_ptr, behind);
                    }
                    bus.write_long(sp + 10, dlg_ptr);
                } else {
                    eprintln!("[TRAP] GetNewDialog({}): DLOG not found -> NIL", dialog_id);
                    // MTE 1992 p. 6-114: GetNewDialog returns NIL if the
                    // DLOG resource cannot be read. Resource Manager ResError
                    // reports resNotFound (-192) for missing resources.
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                    bus.write_long(sp + 10, 0);
                }
                cpu.write_reg(Register::A7, sp + 10);
                let dialog_ptr = bus.read_long(sp + 10);
                self.arm_new_dialog_control_defs(cpu, bus, dialog_ptr);
                Ok(())
            }

            // Alert ($A985) / StopAlert ($A986) / NoteAlert ($A987)
            // / CautionAlert ($A988)
            //
            // FUNCTION Alert(alertID: INTEGER; filterProc: ProcPtr): INTEGER;
            // (and identical signatures for Stop/Note/Caution Alert)
            //
            // The four alert variants differ only in the ICON they
            // display (none / Stop hand / Note speaker / Caution
            // triangle); their dispatch into the ALRT template +
            // ALRT stages logic is identical. Every visible alert enters
            // the normal dialog tracking loop until the user selects an
            // enabled item. This includes one-button informational
            // alerts; Alert is modal and must not silently accept them.
            //
            // ALRT template per IM:I I-422:
            //   +0  boundsRect:   Rect (8 bytes)
            //   +8  itemsID:      INTEGER (DITL resource ID)
            //   +10 stages:       INTEGER (16-bit, 4 nibbles, 1
            //                     per stage 1..4 from low nibble
            //                     to high). Each nibble:
            //                       bit 3:    boldItmNum flag
            //                                 (0 = item 1 bold/
            //                                 default, 1 = item 2
            //                                 bold/default)
            //                       bit 2:    boxDrawn flag
            //                                 (alert is shown if
            //                                 set; suppressed if
            //                                 clear)
            //                       bits 0-1: soundNum (0..3 —
            //                                 0=silent, 1=note,
            //                                 2=caution, 3=stop)
            //
            // The current alert stage lives in the AlertStage
            // low-mem global at $0A9A (1 byte, holds 0..3 mapping
            // to stages 1..4). After each Alert*-family call the
            // byte is incremented and capped at 3, so the fourth
            // and subsequent calls all use the stage-4 nibble.
            // ResetAlrtStage ($A98B) zeros the byte; GetAlrtStage
            // ($A9B7) returns it.
            //
            // HLE compromise: filterProc is NOT invoked (no guest-
            // fn dispatch infrastructure for the ProcPtr argument
            // — same compromise as Pack1 LSearch and other guest-
            // ProcPtr-taking traps).
            // The missing-ALRT path is a defensive probe guard:
            // return -1 but leave AlertStage and ANumber exactly as
            // the caller left them.
            //
            // Inside Macintosh Volume I, I-417..I-422 (Alert
            // family + ALRT template); Macintosh Toolbox Essentials
            // 1992, 6-105..6-119 (System 7 alert dispatch).
            // Alert ($A985): Looks up ALRT, parses stages word at +10 per IM:I I-422, tracks user/script item hits for a visible alert and returns that item, or returns -1 if ALRT is missing / boxDrwn is clear; increments AlertStage capped at 3; filterProc NOT invoked
            // StopAlert ($A986): Same as Alert with Stop icon; identical dispatch path
            // NoteAlert ($A987): Same as Alert with Note icon; identical dispatch path
            // CautionAlert ($A988): Same as Alert with Caution icon; identical dispatch path
            (true, 0x185) | (true, 0x186) | (true, 0x187) | (true, 0x188) => {
                const ALERT_MISSING_RESOURCE_RESULT: i16 = -1;
                let sp = cpu.read_reg(Register::A7);
                if self.dialog_tracking.is_some() {
                    self.handle_interactive_alert_refire(cpu, bus);
                    return Some(Ok(()));
                }
                let filter_proc = bus.read_long(sp);
                let alert_id = bus.read_word(sp + 4) as i16;
                let trap_name = match trap_num {
                    0x185 => "Alert",
                    0x186 => "StopAlert",
                    0x187 => "NoteAlert",
                    0x188 => "CautionAlert",
                    _ => "Alert?",
                };
                let alert_stage_before = bus.read_word(crate::memory::globals::addr::ALERT_STAGE);
                let anumber_before = bus.read_word(crate::memory::globals::addr::ANUMBER);
                let mut alert_trace_stage: Option<(u16, u16, u32, u32)> = None;
                // Look up the ALRT resource and, if present, also pull
                // the referenced DITL's static-text items so the trap
                // log surfaces the alert message before we silently
                // dismiss it. Mid-90s titles (Bumbler, Steel Fighters,
                // etc.) emit StopAlert+ExitToShell on incompatibility;
                // without the message text in the log there's no way
                // to know *which* check failed without RE'ing the
                // binary. Inside Macintosh Volume I, I-422 (ALRT
                // template) and I-426 (DITL).
                let alrt_ptr = self
                    .find_or_load_resource_any(bus, *b"ALRT", alert_id)
                    .map(|(_, ptr)| ptr);
                let result: i16 = if let Some(alrt_data) = alrt_ptr {
                    let alrt_len = bus.get_alloc_size(alrt_data).unwrap_or(0);
                    let (bounds, items_id, stages, position) =
                        Self::parse_alrt(bus, alrt_data, alrt_len);
                    // AlertStage / ACount lives at $0A9A as a
                    // 16-bit WORD (NOT a byte) per IM:I I-423 +
                    // MTb 1992 22620 `#define GetAlertStage()
                    // (* (short*) 0x0A9A)`. On big-endian 68k a
                    // word value 0..3 stores byte 0 at $0A9A and
                    // value at $0A9B, so reading as a byte at
                    // $0A9A would always return 0 — must use
                    // read_word/write_word.
                    let stage_word = bus.read_word(crate::memory::globals::addr::ALERT_STAGE);
                    let (stage_idx, nibble, default_item) =
                        Self::alert_stage_default_item(stages, stage_word);
                    alert_trace_stage = Some((stages, stage_word, stage_idx, nibble));
                    // bit 3 (MSB) of nibble = boldItm per StageList
                    // PACKED RECORD layout (IM:I I-422): boldItm,
                    // boxDrwn, sound[2]. Assembly mask okDismissal=8
                    // and alBit=4 confirm bit masks. MTE 1992 p. 6-106
                    // says Alert returns -1 when boxDrwn is clear.
                    // Increment AlertStage, capped at 3, so the
                    // next call uses the next stage's nibble.
                    let next_stage = ((stage_word as u32) + 1).min(3) as u16;
                    bus.write_word(crate::memory::globals::addr::ALERT_STAGE, next_stage);
                    // ANumber records the resource ID of the last
                    // alert that occurred (IM:I I-423). Apps that
                    // probe ANumber after a sequence of Alert
                    // calls expect this to reflect the most-recent
                    // ID — used by some defensive resume logic.
                    bus.write_word(crate::memory::globals::addr::ANUMBER, alert_id as u16);

                    if let Some(default_item) = default_item {
                        // A filter procedure customizes event handling; it does
                        // not make a visible alert non-modal. Until guest
                        // filter callbacks are supported, preserve the visible
                        // alert and standard button/Return handling instead of
                        // silently choosing the default item.
                        if self.begin_interactive_alert(
                            cpu,
                            bus,
                            sp,
                            alert_id,
                            filter_proc,
                            bounds,
                            items_id,
                            position,
                            default_item,
                        ) {
                            if super::dispatch::trace_dialog_traps_enabled() {
                                eprintln!(
                                    "[TRAP] {} id={} -> interactive dialog (PC=${:08X}) stages=${:04X} alertStage={} stageIdx={} nibble=${:X} defaultItem={}",
                                    trap_name,
                                    alert_id,
                                    cpu.read_reg(Register::PC),
                                    stages,
                                    stage_word,
                                    stage_idx,
                                    nibble,
                                    default_item
                                );
                            }
                            return Some(Ok(()));
                        }
                        default_item
                    } else {
                        ALERT_MISSING_RESOURCE_RESULT
                    }
                } else {
                    bus.write_word(
                        crate::memory::globals::addr::ALERT_STAGE,
                        alert_stage_before,
                    );
                    bus.write_word(crate::memory::globals::addr::ANUMBER, anumber_before);
                    ALERT_MISSING_RESOURCE_RESULT
                };

                if super::dispatch::trace_dialog_traps_enabled() {
                    let mut detail = format!(
                        "[TRAP] {} id={} -> item {} (PC=${:08X})",
                        trap_name,
                        alert_id,
                        result,
                        cpu.read_reg(Register::PC)
                    );
                    if let Some((stages, stage_word, stage_idx, nibble)) = alert_trace_stage {
                        detail.push_str(&format!(
                            " stages=${stages:04X} alertStage={stage_word} stageIdx={stage_idx} nibble=${nibble:X}"
                        ));
                    }
                    if let Some(alrt_data) = alrt_ptr {
                        // ALRT template: bounds(8) + itemsID(2) + stages(2).
                        // Inside Macintosh Volume I, I-422. Some titles
                        // (Bumbler Bee-Luxe in particular) ship ALRT data
                        // whose first 4 bytes don't decode as a Rect — most
                        // likely a non-standard prologue prepended by the
                        // build's PowerPC fragment or resource compiler.
                        // Detect via implausible bounds (top > bottom or
                        // wildly negative coordinates) and suppress the
                        // usual itemsID lookup so the trace reflects that
                        // the resource isn't in the documented format.
                        let alrt_len = bus.get_alloc_size(alrt_data).unwrap_or(0);
                        let ((top, left, bottom, right), items_id, _, _) =
                            Self::parse_alrt(bus, alrt_data, alrt_len);
                        let bounds_plausible = (-32..1024).contains(&top)
                            && (-32..2048).contains(&left)
                            && bottom > top
                            && right > left
                            && (bottom - top) < 1024
                            && (right - left) < 2048;
                        if !bounds_plausible {
                            detail.push_str(&format!(
                                " bounds=({},{},{},{}) — implausible, skipping DITL lookup",
                                top, left, bottom, right
                            ));
                        } else {
                            let ditl_match =
                                self.find_or_load_resource_any(bus, *b"DITL", items_id);
                            detail.push_str(&format!(
                                " bounds=({},{},{},{}) itemsID={} ditl={}",
                                top,
                                left,
                                bottom,
                                right,
                                items_id,
                                if ditl_match.is_some() {
                                    "found"
                                } else {
                                    "missing"
                                }
                            ));
                            if let Some((_, ditl_data)) = ditl_match {
                                let ditl_len = bus.get_alloc_size(ditl_data).unwrap_or(0);
                                let items = Self::parse_ditl(bus, ditl_data, ditl_len);
                                for (i, item) in items.iter().enumerate() {
                                    if !item.text.is_empty() {
                                        detail.push_str(&format!(
                                            "\n[TRAP]   item {} (type=${:02X}): {:?}",
                                            i + 1,
                                            item.item_type,
                                            item.text
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    eprintln!("{}", detail);
                }
                bus.write_word(sp + 6, result as u16);
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // IsDialogEvent ($A97F)
            // Tests whether an event should be handled as part of an active
            // modeless or movable modal dialog.
            // FUNCTION IsDialogEvent(theEvent: EventRecord): Boolean;
            // Inside Macintosh Volume I, I-416; Macintosh Toolbox Essentials 1992, 6-138
            (true, 0x17F) => {
                let sp = cpu.read_reg(Register::A7);
                let event_ptr = bus.read_long(sp);
                let (what, message, where_v, where_h, _modifiers) =
                    Self::read_guest_event_record(bus, event_ptr);
                let target_dialog = self.dialog_from_window_event(what, message);
                let result = if let Some(dialog_ptr) = target_dialog {
                    match what {
                        6 | 8 => message == dialog_ptr,
                        1 => Self::dialog_contains_screen_point(
                            Self::dialog_screen_bounds(bus, dialog_ptr),
                            where_v,
                            where_h,
                        ),
                        _ => true,
                    }
                } else {
                    false
                };
                self.record_dialog_input_trace(
                    "A97F",
                    event_ptr,
                    what,
                    message,
                    where_v,
                    where_h,
                    _modifiers,
                    target_dialog,
                    result,
                    "outcome=is_dialog_event",
                );
                // MPW's Boolean is a one-byte value and Toolbox routines
                // return canonical TRUE as 1. Writing a word-sized -1 here
                // both gives exact-value callers 0xFF and overwrites the
                // adjacent stack byte.
                bus.write_byte(sp + 4, if result { 1 } else { 0 });
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // DialogSelect ($A980)
            // Handles events for modeless and movable modal dialogs.
            // FUNCTION DialogSelect(theEvent: EventRecord; VAR theDialog: DialogPtr; VAR itemHit: INTEGER): BOOLEAN;
            // Inside Macintosh Volume I, I-417; Macintosh Toolbox Essentials 1992, 6-139
            (true, 0x180) => {
                let sp = cpu.read_reg(Register::A7);
                let item_hit_ptr = bus.read_long(sp);
                let dialog_out_ptr = bus.read_long(sp + 4);
                let event_ptr = bus.read_long(sp + 8);
                let (what, message, where_v, where_h, _modifiers) =
                    Self::read_guest_event_record(bus, event_ptr);
                let mut result = false;
                let mut trace_detail = String::from("outcome=no_dialog_target");

                let target_dialog = self.dialog_from_window_event(what, message);
                if let Some(dialog_ptr) = target_dialog {
                    // System 7 leaves the affected dialog in theDialog while
                    // handling update and activate events even though the
                    // Boolean result is FALSE. Macintosh Toolbox Essentials
                    // (1992), pp. 6-139 through 6-141.
                    if matches!(what, 6 | 8) && dialog_out_ptr != 0 {
                        bus.write_long(dialog_out_ptr, dialog_ptr);
                    }
                    let bounds = Self::dialog_screen_bounds(bus, dialog_ptr);
                    trace_detail = format!(
                        "bounds=({},{},{},{}) outcome=no_item",
                        bounds.0, bounds.1, bounds.2, bounds.3
                    );
                    if let Some(mut items) = self.dialog_items.get(&dialog_ptr).cloned() {
                        Self::refresh_ditl_proc_ptrs(bus, dialog_ptr, &mut items);
                        match what {
                            6 => {
                                // MTE 1992 p. 6-141: DialogSelect wraps the
                                // update redraw in BeginUpdate/EndUpdate,
                                // calls DrawDialog, and returns FALSE.
                                let update_rect =
                                    Self::region_handle_rect(bus, bus.read_long(dialog_ptr + 122));
                                self.begin_update_window(bus, dialog_ptr);
                                self.update_dialog_window_contents(
                                    bus,
                                    cpu,
                                    dialog_ptr,
                                    update_rect,
                                );
                                self.end_update_window(bus, dialog_ptr);
                                trace_detail = format!(
                                    "bounds=({},{},{},{}) outcome=update_redraw",
                                    bounds.0, bounds.1, bounds.2, bounds.3
                                );
                            }
                            1 if Self::dialog_contains_screen_point(bounds, where_v, where_h) => {
                                let hit = self.dialog_item_hit_test(
                                    bus,
                                    &items,
                                    bounds,
                                    where_v,
                                    where_h,
                                    &self.dialog_popup_original_rects,
                                    dialog_ptr,
                                );
                                if hit > 0 {
                                    let item = &items[(hit - 1) as usize];
                                    let is_disabled = (item.item_type & 0x80) != 0;
                                    trace_detail = format!(
                                        "bounds=({},{},{},{}) item_hit={} item_type=${:02X} disabled={} outcome={}",
                                        bounds.0,
                                        bounds.1,
                                        bounds.2,
                                        bounds.3,
                                        hit,
                                        item.item_type,
                                        if is_disabled { "true" } else { "false" },
                                        if is_disabled {
                                            "disabled_item"
                                        } else {
                                            "enabled_item"
                                        },
                                    );
                                    if trace_dialog_items_enabled() {
                                        eprintln!(
                                            "[DIALOG-SELECT] mouseDown dialog=${:08X} where=({},{}) bounds=({},{},{},{}) hit={} type=${:02X} disabled={}",
                                            dialog_ptr,
                                            where_v,
                                            where_h,
                                            bounds.0,
                                            bounds.1,
                                            bounds.2,
                                            bounds.3,
                                            hit,
                                            item.item_type,
                                            is_disabled,
                                        );
                                    }
                                    if !is_disabled {
                                        self.cancel_app_owned_modal_dialog_button_tracking(
                                            bus, dialog_ptr,
                                        );
                                        if (item.item_type & 0x7F) == 16 {
                                            // MTE 1992 p. 6-139 / IM:I I-417:
                                            // mouseDown in an enabled editText item makes
                                            // that item the active edit field before
                                            // reporting the item hit. TEClick's pixel-to-
                                            // caret mapping remains the documented HLE
                                            // compromise in the TEClick trap.
                                            self.activate_dialog_edit_item(
                                                bus, cpu, dialog_ptr, &items, hit,
                                            );
                                        }
                                        if dialog_out_ptr != 0 {
                                            bus.write_long(dialog_out_ptr, dialog_ptr);
                                        }
                                        if item_hit_ptr != 0 {
                                            bus.write_word(item_hit_ptr, hit as u16);
                                        }
                                        result = true;
                                    }
                                } else {
                                    trace_detail = format!(
                                        "bounds=({},{},{},{}) item_hit=0 outcome=no_item",
                                        bounds.0, bounds.1, bounds.2, bounds.3
                                    );
                                    if trace_dialog_items_enabled() {
                                        eprintln!(
                                            "[DIALOG-SELECT] mouseDown dialog=${:08X} where=({},{}) bounds=({},{},{},{}) hit=0",
                                            dialog_ptr,
                                            where_v,
                                            where_h,
                                            bounds.0,
                                            bounds.1,
                                            bounds.2,
                                            bounds.3,
                                        );
                                    }
                                }
                            }
                            0 => {
                                let (_edit_text, edit_item, _default_item) =
                                    Self::dialog_edit_state(bus, dialog_ptr, &items);
                                trace_detail = format!(
                                    "bounds=({},{},{},{}) edit_item={} outcome=teidle",
                                    bounds.0, bounds.1, bounds.2, bounds.3, edit_item
                                );
                                if edit_item > 0 {
                                    // MTE 1992 p. 6-139 / IM:I I-417:
                                    // DialogSelect calls TEIdle for null events when an
                                    // editText item is present, letting TextEdit advance
                                    // the insertion-caret blink without changing text or
                                    // selection fields.
                                    let text_handle = bus.read_long(dialog_ptr + 160);
                                    self.textedit_idle(cpu, bus, text_handle);
                                }
                            }
                            1 => {
                                trace_detail = format!(
                                    "bounds=({},{},{},{}) item_hit=0 outcome=outside_dialog",
                                    bounds.0, bounds.1, bounds.2, bounds.3
                                );
                                if trace_dialog_items_enabled() {
                                    eprintln!(
                                        "[DIALOG-SELECT] mouseDown dialog=${:08X} where=({},{}) outside bounds=({},{},{},{})",
                                        dialog_ptr,
                                        where_v,
                                        where_h,
                                        bounds.0,
                                        bounds.1,
                                        bounds.2,
                                        bounds.3,
                                    );
                                }
                            }
                            3 | 5 => {
                                let (_edit_text, edit_item, _default_item) =
                                    Self::dialog_edit_state(bus, dialog_ptr, &items);
                                trace_detail = format!(
                                    "bounds=({},{},{},{}) edit_item={} outcome=no_enabled_edittext",
                                    bounds.0, bounds.1, bounds.2, bounds.3, edit_item
                                );
                                if edit_item > 0 {
                                    // IM:I I-417: keyDown/autoKey dialog handling applies to
                                    // editable text items. If no enabled editText item is
                                    // active, DialogSelect returns FALSE.
                                    if let Some(item) = items.get((edit_item - 1) as usize) {
                                        let item_type = item.item_type;
                                        let base_type = item_type & 0x7F;
                                        let is_disabled = (item_type & 0x80) != 0;
                                        trace_detail = format!(
                                            "bounds=({},{},{},{}) edit_item={} item_type=${:02X} disabled={} outcome={}",
                                            bounds.0,
                                            bounds.1,
                                            bounds.2,
                                            bounds.3,
                                            edit_item,
                                            item_type,
                                            if is_disabled { "true" } else { "false" },
                                            if base_type == 16 && !is_disabled {
                                                "enabled_edittext"
                                            } else {
                                                "no_enabled_edittext"
                                            },
                                        );
                                        if base_type == 16 && !is_disabled {
                                            let char_code = (message & 0xFF) as u8;
                                            // MTE 1992 p. 6-139: DialogSelect uses TextEdit
                                            // to handle key-down and auto-key events in
                                            // editable text items before reporting itemHit.
                                            self.apply_dialog_select_key_to_edit_item(
                                                bus, dialog_ptr, &mut items, edit_item, char_code,
                                            );
                                            if dialog_out_ptr != 0 {
                                                bus.write_long(dialog_out_ptr, dialog_ptr);
                                            }
                                            if item_hit_ptr != 0 {
                                                bus.write_word(item_hit_ptr, edit_item as u16);
                                            }
                                            result = true;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        self.dialog_items.insert(dialog_ptr, items);
                    } else {
                        trace_detail = format!(
                            "bounds=({},{},{},{}) outcome=no_dialog_items",
                            bounds.0, bounds.1, bounds.2, bounds.3
                        );
                        if trace_dialog_items_enabled() {
                            eprintln!(
                                "[DIALOG-SELECT] event what={} dialog=${:08X} has no dialog_items",
                                what, dialog_ptr
                            );
                        }
                    }
                } else if trace_dialog_items_enabled() {
                    eprintln!(
                        "[DIALOG-SELECT] event what={} message=${:08X} where=({},{}) has no dialog target front=${:08X}",
                        what, message, where_v, where_h, self.front_window
                    );
                }
                self.record_dialog_input_trace(
                    "A980",
                    event_ptr,
                    what,
                    message,
                    where_v,
                    where_h,
                    _modifiers,
                    target_dialog,
                    result,
                    &trace_detail,
                );

                // DialogSelect shares IsDialogEvent's one-byte Pascal
                // Boolean ABI: canonical TRUE is 1, not a word-sized -1.
                bus.write_byte(sp + 12, if result { 1 } else { 0 });
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // DrawDialog ($A981)
            // Draws the entire contents of the specified dialog box.
            // PROCEDURE DrawDialog (theDialog: DialogPtr);
            // Inside Macintosh Volume I, I-417; Macintosh Toolbox Essentials 1992, 6-142
            // DrawDialog ($A981): Renders all dialog items with full DITL support
            (true, 0x181) => {
                let sp = cpu.read_reg(Register::A7);
                let dialog_ptr = bus.read_long(sp);
                if let Some(mut items) = self.dialog_items.get(&dialog_ptr).cloned() {
                    Self::refresh_ditl_proc_ptrs(bus, dialog_ptr, &mut items);
                    let bounds = Self::dialog_screen_bounds(bus, dialog_ptr);
                    let proc_id = self.dialog_window_proc_id(bus, dialog_ptr);
                    let (edit_text, edit_item, default_item) =
                        Self::dialog_edit_state(bus, dialog_ptr, &items);
                    self.dialog_initial_draw_deferred.remove(&dialog_ptr);
                    self.draw_dialog_items(
                        bus,
                        bounds,
                        proc_id,
                        &items,
                        default_item,
                        &edit_text,
                        edit_item,
                        false,
                        dialog_ptr,
                    );
                    self.dialog_items.insert(dialog_ptr, items);
                    // Record that the application painted this dialog itself.
                    // ModalDialog must not repaint it on entry, or anything the
                    // app draws into the dialog after this call is erased.
                    // Inside Macintosh Volume I, I-415.
                    self.dialogs_drawn_by_app.insert(dialog_ptr);
                    // MTE 1992 p. 6-142: DrawDialog calls
                    // application-defined item draw procs for userItem
                    // records. Queue them after the HLE redraw so the
                    // runner trampoline executes guest drawing before the
                    // caller resumes.
                    self.queue_modeless_dialog_draw_procs_intersecting(bus, dialog_ptr, None);
                    if !self.modeless_dialog_cdef_draw_queue.contains(&dialog_ptr) {
                        self.modeless_dialog_cdef_draw_queue.push_back(dialog_ptr);
                    }
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // DisposDialog ($A983)
            // Removes a dialog and frees its storage.
            // PROCEDURE DisposDialog (theDialog: DialogPtr);
            // Inside Macintosh Volume I, I-425
            //
            // DisposDialog internally calls CloseWindow, which on real
            // Mac OS invalidates the window's frame and the Window
            // Manager's PaintBehind / CalcVisBehind restores the
            // content beneath via update events to the back windows.
            //
            // In HLE we emulate PaintBehind by blitting the saved-under
            // pixels captured immediately before the dialog first draws to
            // the visible screen. Without this restore, any app that skips
            // ModalDialog and runs its own event loop leaves a dialog-shaped
            // hole over the window behind.
            //
            // Only restore when `was_front` holds: a non-front dialog
            // would require the saved bounds to still be valid, but
            // by the time we'd get here the window stack has already
            // moved on. A visible dialog close separately invalidates every
            // visible window behind its structure; a hidden dialog exposes
            // nothing and therefore does not queue a repaint.
            //
            // Inside Macintosh Volume I, I-283 (CloseWindow), I-425 (DisposDialog)
            //
            // Regression coverage:
            //   src/trap/dialog.rs::tests::disposdialog_restores_saved_background_pixels
            //   src/trap/dialog.rs::tests::disposdialog_non_front_does_not_restore
            // DisposDialog ($A983): Frees dialog, cleans up dialog_items, restores saved background pixels (IM:I I-425 PaintBehind)
            (true, 0x183) => {
                let sp = cpu.read_reg(Register::A7);
                let requested_dialog_ptr = bus.read_long(sp);
                let dialog_ptr =
                    self.resolve_dispos_dialog_ptr_after_modal_button_hit(requested_dialog_ptr);
                eprintln!("[TRAP] DisposDialog(${:08X})", requested_dialog_ptr);
                self.close_dialog_window(bus, cpu, dialog_ptr, true);
                self.capture_gui_frame(bus, &format!("dispos_dialog_{:08X}", dialog_ptr));
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // ParamText ($A98B)
            // Saves the four strings for ^0..^3 substitution into any
            // subsequently drawn dialog or alert static-text item.
            // PROCEDURE ParamText(param0, param1, param2, param3: Str255);
            // Inside Macintosh Volume I, I-422
            // ParamText ($A98B): Saves all 4 Pascal strings for ^0..^3 dialog/alert text substitution
            (true, 0x18B) => {
                let sp = cpu.read_reg(Register::A7);
                let trap_pc = cpu.read_reg(Register::PC).wrapping_sub(2);
                // Stack layout (top-down): SP+0:param3, +4:param2, +8:param1, +12:param0.
                // Per Inside Macintosh Volume I, I-422, passing NIL for any
                // parameter leaves that slot's previous value unchanged —
                // it's a "set this slot, leave others alone" idiom. Clearing
                // on NIL would erase ^N output that an earlier ParamText
                // had legitimately staged.
                let offsets: [u32; 4] = [12, 8, 4, 0];
                for (i, &off) in offsets.iter().enumerate() {
                    let ptr = bus.read_long(sp + off);
                    if ptr == 0 {
                        continue;
                    }
                    let s = bus.read_pstring(ptr);
                    self.param_text.set_slot(i, s.clone());
                    // Write to DAStrings low-memory global ($0AA0 + i*4).
                    // The ROM stores each param string as a StringHandle at
                    // *(StringHandle*)($0AA0 + i*4).
                    // Inside Macintosh Volume I, I-422 (DAStrings global).
                    use crate::memory::globals::addr;
                    let data_len = 1u32 + s.len() as u32;
                    let data_ptr = bus.alloc(data_len);
                    if data_ptr != 0 {
                        bus.write_byte(data_ptr, s.len() as u8);
                        for (j, &b) in s.iter().enumerate() {
                            bus.write_byte(data_ptr + 1 + j as u32, b);
                        }
                        let handle = bus.alloc(4);
                        if handle != 0 {
                            bus.write_long(handle, data_ptr);
                            bus.write_long(addr::DA_STRINGS + i as u32 * 4, handle);
                        }
                    }
                }
                eprintln!(
                    "[TRAP] ParamText pc=${:08X} ^0=\"{}\" ^1=\"{}\" ^2=\"{}\" ^3=\"{}\"",
                    trap_pc,
                    String::from_utf8_lossy(&self.param_text[0]),
                    String::from_utf8_lossy(&self.param_text[1]),
                    String::from_utf8_lossy(&self.param_text[2]),
                    String::from_utf8_lossy(&self.param_text[3]),
                );
                cpu.write_reg(Register::A7, sp + 16);
                Ok(())
            }

            // GetDItem ($A98D)
            // Returns information about a dialog item.
            // PROCEDURE GetDItem (theDialog: DialogPtr; itemNo: INTEGER;
            //     VAR itemType: INTEGER; VAR item: Handle; VAR box: Rect);
            // Inside Macintosh Volume I, I-421
            // GetDItem ($A98D): Returns real item type, handle (with text for statText/editText, proc_ptr for userItem), and rect from dialog_items
            (true, 0x18D) => {
                let sp = cpu.read_reg(Register::A7);
                let box_ptr = bus.read_long(sp);
                let item_handle_ptr = bus.read_long(sp + 4);
                let type_ptr = bus.read_long(sp + 8);
                let item_no = bus.read_word(sp + 12) as i16;
                let dialog_ptr = bus.read_long(sp + 14);

                // Look up real item data
                let found = self.dialog_items.get(&dialog_ptr).and_then(|items| {
                    if item_no > 0 && (item_no as usize) <= items.len() {
                        Some(&items[(item_no - 1) as usize])
                    } else {
                        None
                    }
                });

                if let Some(item) = found {
                    if trace_dialog_items_enabled() && (item.item_type & 0x7F) == 0 {
                        eprintln!(
                            "[DIALOG-ITEM] GetDItem pc=${:08X} dialog=${:08X} item={} type={} proc=${:08X} out_type=${:08X} out_item=${:08X} out_box=${:08X} rect=({},{},{},{})",
                            cpu.read_reg(Register::PC),
                            dialog_ptr,
                            item_no,
                            item.item_type,
                            item.proc_ptr,
                            type_ptr,
                            item_handle_ptr,
                            box_ptr,
                            item.rect.0,
                            item.rect.1,
                            item.rect.2,
                            item.rect.3,
                        );
                    }
                    if type_ptr != 0 {
                        bus.write_word(type_ptr, item.item_type as u16);
                    }
                    if item_handle_ptr != 0 {
                        let current_handle = Self::dialog_item_handle(bus, dialog_ptr, item_no);
                        let base_type = item.item_type & 0x7F;
                        if current_handle != 0 || !(4..=6).contains(&base_type) {
                            bus.write_long(item_handle_ptr, current_handle);
                        } else {
                            // Create a full ControlRecord so draw_control can render it.
                            // ControlRecord layout:
                            //   +0: nextControl (4)
                            //   +4: contrlOwner (4) = window ptr
                            //   +8: contrlRect (8) = top, left, bottom, right
                            //  +16: contrlVis (1) = 255 (visible)
                            //  +17: contrlHilite (1) = 0
                            //  +18: contrlValue (2)
                            //  +20: contrlMin (2)
                            //  +22: contrlMax (2) = 1
                            //  +24: contrlDefProc (4) = procID encoding
                            //  +40: contrlTitle (pascal string)
                            let title = encode_mac_roman_lossy(&item.text);
                            let title_len = title.len().min(255);
                            let ctrl_rec = bus.alloc(42 + title_len as u32);
                            bus.write_long(ctrl_rec, 0); // nextControl
                            bus.write_long(ctrl_rec + 4, dialog_ptr); // contrlOwner
                                                                      // contrlRect: dialog-local coordinates (draw_control gets
                                                                      // screen offset from the owner window's PixMap bounds)
                            bus.write_word(ctrl_rec + 8, item.rect.0 as u16);
                            bus.write_word(ctrl_rec + 10, item.rect.1 as u16);
                            bus.write_word(ctrl_rec + 12, item.rect.2 as u16);
                            bus.write_word(ctrl_rec + 14, item.rect.3 as u16);
                            bus.write_byte(ctrl_rec + 16, 255); // contrlVis = visible
                            bus.write_byte(ctrl_rec + 17, 0); // contrlHilite
                            let value = self
                                .dialog_control_values
                                .get(&(dialog_ptr, item_no))
                                .copied()
                                .unwrap_or(0);
                            bus.write_word(ctrl_rec + 18, value as u16);
                            bus.write_word(ctrl_rec + 20, 0); // contrlMin
                            bus.write_word(ctrl_rec + 22, 1); // contrlMax
                                                              // Write title as pascal string at offset 40
                            bus.write_byte(ctrl_rec + 40, title_len as u8);
                            for (i, &ch) in title.iter().take(title_len).enumerate() {
                                bus.write_byte(ctrl_rec + 41 + i as u32, ch);
                            }
                            // Map DITL item type to Control Manager procID
                            let proc_id: i16 = match base_type {
                                4 => 0, // btnCtrl → pushButProc
                                5 => 1, // chkCtrl → checkBoxProc
                                6 => 2, // radCtrl → radioButProc
                                _ => 0,
                            };
                            self.control_manager.set_proc_id(ctrl_rec, proc_id);
                            let handle = bus.alloc(4);
                            bus.write_long(handle, ctrl_rec);
                            self.control_manager.associate_handle(handle, ctrl_rec);
                            bus.write_long(item_handle_ptr, handle);
                            self.dialog_control_handles
                                .insert(handle, (dialog_ptr, item_no));
                            // Also update the DITL item handle storage
                            Self::set_dialog_item_handle(bus, dialog_ptr, item_no, handle);
                        }
                    }
                    if box_ptr != 0 {
                        bus.write_word(box_ptr, item.rect.0 as u16);
                        bus.write_word(box_ptr + 2, item.rect.1 as u16);
                        bus.write_word(box_ptr + 4, item.rect.2 as u16);
                        bus.write_word(box_ptr + 6, item.rect.3 as u16);
                    }
                    // Some apps use InsertMenu -> GetDItem -> SetDItem to
                    // build a custom popup control backed by a userItem. Do
                    // not promote on GetDItem alone: plain userItem grids also
                    // query their item rectangles and must not inherit a stale
                    // popup-menu association.
                    if let Some(menu_id) = self.last_inserted_menu_id.take() {
                        let enabled_user_item =
                            (item.item_type & 0x7F) == 0 && (item.item_type & 0x80) == 0;
                        if enabled_user_item {
                            if item.proc_ptr != 0 {
                                self.dialog_item_popup_menus
                                    .insert((dialog_ptr, item_no), menu_id);
                                self.dialog_popup_original_rects
                                    .insert((dialog_ptr, item_no), item.rect);
                                self.pending_dialog_popup_menu = None;
                            } else {
                                self.pending_dialog_popup_menu = Some(PendingDialogPopupMenu {
                                    dialog_ptr,
                                    item_no,
                                    menu_id,
                                    rect: item.rect,
                                });
                            }
                        } else {
                            self.pending_dialog_popup_menu = None;
                        }
                    }
                } else {
                    if type_ptr != 0 {
                        bus.write_word(type_ptr, 0);
                    }
                    if item_handle_ptr != 0 {
                        bus.write_long(item_handle_ptr, 0);
                    }
                    if box_ptr != 0 {
                        bus.write_long(box_ptr, 0);
                        bus.write_long(box_ptr + 4, 0);
                    }
                }
                cpu.write_reg(Register::A7, sp + 18);
                Ok(())
            }

            // SetDItem ($A98E)
            // Sets information about a dialog item.
            // PROCEDURE SetDItem (theDialog: DialogPtr; itemNo: INTEGER;
            //     itemType: INTEGER; item: Handle; box: Rect);
            // Inside Macintosh Volume I, I-421
            //
            // Stack layout (box passed as pointer-to-Rect, not inline):
            //   SP+0..3:  &box   (pointer to Rect — MPW compiler passes large
            //                     value params by reference)
            //   SP+4..7:  item   (Handle / ProcPtr)
            //   SP+8..9:  itemType
            //   SP+10..11: itemNo
            //   SP+12..15: theDialog
            // SetDItem ($A98E): Stores item type, rect, and proc_ptr (for userItem); updates both dialog_items and active tracking state
            (true, 0x18E) => {
                let sp = cpu.read_reg(Register::A7);
                let box_ptr = bus.read_long(sp);
                let item_handle = bus.read_long(sp + 4);
                let item_type = bus.read_word(sp + 8) as u8;
                let item_no = bus.read_word(sp + 10) as i16;
                let dialog_ptr = bus.read_long(sp + 12);
                let (box_top, box_left, box_bottom, box_right) = if box_ptr != 0 {
                    (
                        bus.read_word(box_ptr) as i16,
                        bus.read_word(box_ptr + 2) as i16,
                        bus.read_word(box_ptr + 4) as i16,
                        bus.read_word(box_ptr + 6) as i16,
                    )
                } else {
                    (0, 0, 0, 0)
                };

                let base_type = item_type & 0x7F;
                let previous_handle = Self::dialog_item_handle(bus, dialog_ptr, item_no);
                let previous_item = self
                    .dialog_items
                    .get(&dialog_ptr)
                    .and_then(|items| {
                        (item_no > 0)
                            .then(|| items.get((item_no - 1) as usize))
                            .flatten()
                    })
                    .cloned();
                if trace_dialog_items_enabled() {
                    eprintln!(
                        "[DIALOG-ITEM] SetDItem pc=${:08X} sp=${:08X} rawbytes={:02X?} dialog=${:08X} item={} type={} proc=${:08X} rect=({},{},{},{})",
                        cpu.read_reg(Register::PC),
                        sp,
                        (0..24u32).map(|i| bus.read_byte(sp + i)).collect::<Vec<u8>>(),
                        dialog_ptr,
                        item_no,
                        item_type,
                        item_handle,
                        box_top,
                        box_left,
                        box_bottom,
                        box_right,
                    );
                }

                if previous_handle != 0 {
                    self.dialog_item_handles.remove(&previous_handle);
                    self.dialog_control_handles.remove(&previous_handle);
                }
                if let Some(item_handle_addr) =
                    Self::dialog_item_handle_addr(bus, dialog_ptr, item_no)
                {
                    bus.write_long(item_handle_addr, item_handle);
                    bus.write_word(item_handle_addr + 4, box_top as u16);
                    bus.write_word(item_handle_addr + 6, box_left as u16);
                    bus.write_word(item_handle_addr + 8, box_bottom as u16);
                    bus.write_word(item_handle_addr + 10, box_right as u16);
                    bus.write_byte(item_handle_addr + 12, item_type);
                }
                if let Some(items) = self.dialog_items.get_mut(&dialog_ptr) {
                    if item_no > 0 && (item_no as usize) <= items.len() {
                        let item = &mut items[(item_no - 1) as usize];
                        item.item_type = item_type;
                        item.rect = (box_top, box_left, box_bottom, box_right);
                        if base_type == 0 {
                            // userItem: the "item" parameter is a ProcPtr
                            // Inside Macintosh Volume I, I-405
                            item.proc_ptr = item_handle;
                        } else if base_type == 8 || base_type == 16 {
                            item.text = Self::text_item_string_from_handle(bus, item_handle);
                        }
                    }
                }

                if let Some(pending) = self.pending_dialog_popup_menu {
                    if pending.dialog_ptr == dialog_ptr && pending.item_no == item_no {
                        if base_type == 0 && (item_type & 0x80) == 0 && item_handle != 0 {
                            self.dialog_item_popup_menus
                                .insert((dialog_ptr, item_no), pending.menu_id);
                            self.dialog_popup_original_rects
                                .insert((dialog_ptr, item_no), pending.rect);
                        }
                        self.pending_dialog_popup_menu = None;
                    }
                }

                if let Some(previous_item) = previous_item {
                    let key = (dialog_ptr, item_no);
                    let previous_base_type = previous_item.item_type & 0x7F;
                    let previous_enabled_user_item =
                        previous_base_type == 0 && (previous_item.item_type & 0x80) == 0;
                    let current_enabled_user_item = base_type == 0 && (item_type & 0x80) == 0;
                    let old_width = previous_item.rect.3 - previous_item.rect.1;
                    let old_height = previous_item.rect.2 - previous_item.rect.0;
                    let new_width = box_right - box_left;
                    let new_height = box_bottom - box_top;
                    let narrowed_to_popup_indicator = previous_enabled_user_item
                        && current_enabled_user_item
                        && item_handle == 0
                        && old_width >= 40
                        && new_width > 0
                        && new_width <= 24
                        && old_height > 0
                        && new_height > 0
                        && (old_height - new_height).abs() <= 4;
                    if narrowed_to_popup_indicator {
                        self.dialog_popup_original_rects
                            .entry(key)
                            .or_insert(previous_item.rect);
                        self.dialog_popup_candidate_items.insert(key);
                    } else if !current_enabled_user_item {
                        self.dialog_popup_candidate_items.remove(&key);
                        if !self.dialog_item_popup_menus.contains_key(&key) {
                            self.dialog_popup_original_rects.remove(&key);
                        }
                    }
                }

                if base_type == 8 || base_type == 16 {
                    if item_handle != 0 {
                        self.dialog_item_handles
                            .insert(item_handle, (dialog_ptr, (item_no - 1) as usize));
                    }
                } else if (base_type == 4 || base_type == 5 || base_type == 6 || base_type == 7)
                    && item_handle != 0
                {
                    self.dialog_control_handles
                        .insert(item_handle, (dialog_ptr, item_no));
                    let ctrl_ptr = bus.read_long(item_handle);
                    if ctrl_ptr != 0 {
                        bus.write_word(ctrl_ptr + 8, box_top as u16);
                        bus.write_word(ctrl_ptr + 10, box_left as u16);
                        bus.write_word(ctrl_ptr + 12, box_bottom as u16);
                        bus.write_word(ctrl_ptr + 14, box_right as u16);
                        self.dialog_control_values
                            .insert((dialog_ptr, item_no), bus.read_word(ctrl_ptr + 18) as i16);
                    }
                }

                // Also update tracking state if dialog is currently active
                if let Some(ref mut tracking) = self.dialog_tracking {
                    if tracking.dialog_ptr == dialog_ptr
                        && item_no > 0
                        && (item_no as usize) <= tracking.items.len()
                    {
                        let item = &mut tracking.items[(item_no - 1) as usize];
                        item.item_type = item_type;
                        item.rect = (box_top, box_left, box_bottom, box_right);
                        if base_type == 0 {
                            item.proc_ptr = item_handle;
                        } else if base_type == 8 || base_type == 16 {
                            item.text = Self::text_item_string_from_handle(bus, item_handle);
                            if tracking.edit_item == item_no {
                                tracking.edit_text = item.text.clone();
                            }
                        }
                    }
                }

                cpu.write_reg(Register::A7, sp + 16);
                Ok(())
            }

            // ModalDialog ($A991)
            // Handles events in a modal dialog until an enabled item is hit.
            // PROCEDURE ModalDialog (filterProc: ProcPtr; VAR itemHit: INTEGER);
            // Inside Macintosh Volume I, I-415
            // ModalDialog ($A991): Re-fire pattern: draws dialog (including resCtrl/popup controls, type 7), handles button clicks, keyboard input (Return/Escape/text), button flash animation, userItem draw proc callbacks
            (true, 0x191) => {
                // A filter can enter another ModalDialog before returning.
                // Each invocation owns its Pascal arguments and tracking state.
                let call_sp = cpu.read_reg(Register::A7);
                if self
                    .dialog_tracking
                    .as_ref()
                    .is_some_and(|tracking| call_sp < tracking.stack_ptr)
                {
                    self.suspended_modal_dialogs
                        .push(self.dialog_tracking.take().unwrap());
                    if self.dialog_filter_result_addr != 0 {
                        bus.write_word(self.dialog_filter_result_addr, 0);
                    }
                }
                if self.dialog_tracking.is_none()
                    && self
                        .suspended_modal_dialogs
                        .last()
                        .is_some_and(|tracking| tracking.stack_ptr == call_sp)
                {
                    self.dialog_tracking = self.suspended_modal_dialogs.pop();
                }
                // Check if draw procs need to finish before entering event loop.
                if let Some(ref tracking) = self.dialog_tracking {
                    if !tracking.draw_procs_done {
                        return Some(Ok(()));
                    }
                }
                // Re-snapshot rendered_pixels after draw procs or filter proc completes.
                // rendered_pixels_final is cleared when either is injected; the snapshot
                // here captures whatever they drew before redraw_chrome can restore it.
                let cdef_draw_pending_snapshot = self
                    .dialog_tracking
                    .as_ref()
                    .map(|tracking| tracking.dialog_ptr)
                    .is_some_and(|dialog_ptr| {
                        self.dialog_cdef_draw_pending_snapshot.remove(&dialog_ptr)
                    });
                if !cdef_draw_pending_snapshot {
                    if let Some(tracking) = self.dialog_tracking.as_mut() {
                        if let Some(epoch) = tracking.filter_presentation_epoch.take() {
                            if bus.presentation_epoch() == Some(epoch) {
                                tracking.rendered_pixels_final = true;
                            }
                        }
                    }
                }
                if let Some(ref tracking) = self.dialog_tracking {
                    if !tracking.rendered_pixels_final {
                        let bounds = tracking.bounds;
                        let items = tracking.items.clone();
                        let default_item = tracking.default_item;
                        let edit_text = tracking.edit_text.clone();
                        let edit_item = tracking.edit_item;
                        let dialog_ptr = tracking.dialog_ptr;
                        let popup_draws = tracking.popup_draws.clone();
                        if !tracking.game_managed && !cdef_draw_pending_snapshot {
                            if self.front_window == dialog_ptr {
                                self.blit_window_to_screen(bus);
                            }
                            self.redraw_standard_dialog_items(
                                bus,
                                bounds,
                                &items,
                                default_item,
                                &edit_text,
                                edit_item,
                                dialog_ptr,
                            );
                        }
                        self.redraw_dialog_popup_controls(bus, &popup_draws);
                        let rendered = self.save_dialog_pixels(bus, bounds);
                        let t = self.dialog_tracking.as_mut().unwrap();
                        t.rendered_pixels = rendered;
                        t.rendered_pixels_final = true;
                    }
                }

                if let Some(ref tracking) = self.dialog_tracking {
                    if tracking.active_button.is_some() {
                        self.handle_dialog_button_tracking(bus);
                        return Some(Ok(()));
                    }

                    if tracking.active_popup.is_some() {
                        self.handle_dialog_popup_tracking(cpu, bus);
                        return Some(Ok(()));
                    }

                    if tracking.active_user_item.is_some() {
                        self.handle_dialog_user_item_tracking(cpu, bus);
                        return Some(Ok(()));
                    }

                    // Fast path — when nothing can produce an item hit or visible
                    // update on this step (no filter proc, no flash animation, no
                    // pending event, no queued events), return Ok without running
                    // any of the re-fire body. Any of these flags being non-default
                    // routes through the full handler below.
                    if tracking.filter_proc == 0
                        && tracking.flash_remaining == 0
                        && tracking.active_button.is_none()
                        && tracking.active_popup.is_none()
                        && tracking.active_user_item.is_none()
                        && self.event_queue.is_empty()
                    {
                        return Some(Ok(()));
                    }
                    // Re-fire: dialog tracking is active
                    let dialog_ptr = tracking.dialog_ptr;
                    let bounds = tracking.bounds;
                    let item_hit_ptr = tracking.item_hit_ptr;
                    let stack_ptr = tracking.stack_ptr;
                    let filter_proc = tracking.filter_proc;
                    let flash_remaining = tracking.flash_remaining;
                    // Items_clone is built lazily in the mouseDown branch below to
                    // avoid cloning Vec<DialogItem> on every ModalDialog refire.
                    let mut pending_event = None;

                    // For any dialog with a filter proc, check whether the filter
                    // handled the most recent event. Per Inside Macintosh Volume I, I-415:
                    // TRUE means the filter handled the event and set itemHit;
                    // FALSE means ModalDialog should process the event itself.
                    let mut waiting_for_filter_proc_event = false;
                    if filter_proc != 0 {
                        let result_addr = self.dialog_filter_result_addr;
                        let tracking_dialog_ptr = tracking.dialog_ptr;
                        let filter_event = self
                            .dialog_tracking
                            .as_mut()
                            .and_then(|t| t.last_filter_event.take());
                        // The scratch result belongs to the most recently
                        // completed filter callback, not merely to whichever
                        // ModalDialog happens to be active now. A newly opened
                        // dialog can otherwise inherit TRUE from the filter of
                        // the preceding dialog and return before its own filter
                        // has received an event. IM:I I-415 defines the result
                        // only as the response to the event passed to filterProc.
                        let filter_result_valid = filter_event.is_some();
                        let filter_result_word = if result_addr != 0 {
                            bus.read_word(result_addr)
                        } else {
                            0
                        };
                        let filter_returned_true = if filter_result_valid && result_addr != 0 {
                            // Stack-based Pascal BOOLEAN results are encoded
                            // in bit 0 of the high-order byte, not as any
                            // nonzero word. Inside Macintosh Volume I,
                            // "Using Assembly Language", stack-based routines.
                            (filter_result_word & 0x0100) != 0
                        } else {
                            false
                        };

                        let mut hit = 0i16;
                        if filter_returned_true && item_hit_ptr != 0 {
                            hit = bus.read_word(item_hit_ptr) as i16;
                        }
                        if trace_dialog_filter_enabled() {
                            eprintln!(
                                "[DIALOG-FILTER] result dialog=${:08X} result_word=${:04X} returned_true={} item_hit={} item_hit_ptr=${:08X}",
                                tracking_dialog_ptr,
                                filter_result_word,
                                filter_returned_true,
                                hit,
                                item_hit_ptr
                            );
                        }

                        if hit > 0 || filter_returned_true {
                            let handled_mouse_down =
                                filter_event.as_ref().is_some_and(|e| e.what == 1);
                            let (dialog_ptr, edit_item, edit_text, items) = {
                                let tracking = self.dialog_tracking.as_mut().unwrap();
                                Self::sync_tracking_active_edit_item(tracking);
                                (
                                    tracking.dialog_ptr,
                                    tracking.edit_item,
                                    tracking.edit_text.clone(),
                                    tracking.items.clone(),
                                )
                            };
                            let keep_dialog_visible = items
                                .get((hit - 1) as usize)
                                .map(|item| (item.item_type & 0x7F) != 4)
                                .unwrap_or(true);
                            let item_type =
                                items.get((hit - 1) as usize).map(|item| item.item_type);
                            self.flush_dialog_edit_item_texts(
                                bus, dialog_ptr, &items, edit_item, &edit_text,
                            );
                            if hit > 0
                                && item_type.is_some_and(|ty| (ty & 0x7F) == 4 && (ty & 0x80) == 0)
                            {
                                if let Some(item) = items.get((hit - 1) as usize) {
                                    self.restore_dialog_button_normal_state(
                                        bus,
                                        dialog_ptr,
                                        bounds,
                                        hit,
                                        item,
                                        hit == self
                                            .dialog_tracking
                                            .as_ref()
                                            .map(|tracking| tracking.default_item)
                                            .unwrap_or(0),
                                    );
                                }
                            }
                            // Filter handled the event — end tracking and return.
                            let saved = self.dialog_tracking.take().unwrap();
                            let saved_dialog_ptr = saved.dialog_ptr;
                            let saved_bounds = saved.bounds;
                            if keep_dialog_visible {
                                self.persist_visible_dialog_snapshot(bus, &saved);
                            }
                            self.dialog_saved_pixels
                                .insert(saved_dialog_ptr, saved.saved_pixels);
                            if hit > 0
                                && item_type.is_some_and(|ty| (ty & 0x7F) == 4 && (ty & 0x80) == 0)
                            {
                                self.pending_modal_button_dispose_dialog = Some(saved_dialog_ptr);
                            }
                            if handled_mouse_down {
                                self.consume_or_retain_dialog_mouse_up(
                                    filter_event.as_ref().expect("handled mouseDown event"),
                                );
                            }
                            if trace_dialog_filter_enabled() {
                                let actual_hit = bus.read_word(item_hit_ptr) as i16;
                                eprintln!(
                                    "[DIALOG-FILTER] ModalDialog returning: filter hit={} actual_item_hit_ptr=${:08X} actual_hit={} handled_mouseDown={} stack_ptr=${:08X} queue_len={}",
                                    hit, item_hit_ptr, actual_hit, handled_mouse_down, stack_ptr, self.event_queue.len()
                                );
                            }
                            let outcome = if hit > 0 {
                                if keep_dialog_visible {
                                    "filter_item_hit_retained"
                                } else {
                                    "filter_item_hit_dismissed"
                                }
                            } else {
                                "filter_true_zero_hit"
                            };
                            self.record_modal_dialog_filter_input_trace(
                                "filter_result",
                                saved_dialog_ptr,
                                saved_bounds,
                                filter_proc,
                                filter_event.as_ref(),
                                hit,
                                item_type,
                                handled_mouse_down,
                                keep_dialog_visible,
                                "returned",
                                outcome,
                            );
                            cpu.write_reg(Register::A7, stack_ptr + 8);
                            return Some(Ok(()));
                        }
                        pending_event = filter_event;
                        if let Some(ref event) = pending_event {
                            self.record_modal_dialog_filter_input_trace(
                                "filter_result",
                                tracking_dialog_ptr,
                                bounds,
                                filter_proc,
                                Some(event),
                                0,
                                None,
                                false,
                                true,
                                "passed",
                                "filter_declined",
                            );
                        }
                        waiting_for_filter_proc_event = pending_event.is_none();
                    }

                    if flash_remaining > 0 {
                        // Button flash animation
                        let flash_item = self.dialog_tracking.as_ref().unwrap().flash_item;
                        let (remaining, button_draw) = {
                            let t = self.dialog_tracking.as_mut().unwrap();
                            if t.flash_delay > 0 {
                                t.flash_delay -= 1;
                                return Some(Ok(()));
                            }
                            t.flash_remaining -= 1;
                            t.flash_delay = 3;
                            let remaining = t.flash_remaining;
                            let button_draw = if remaining > 0
                                && flash_item > 0
                                && (flash_item as usize) <= t.items.len()
                            {
                                let item = &t.items[(flash_item - 1) as usize];
                                Some((
                                    Self::dialog_item_screen_rect(bounds, item.rect),
                                    item.text.clone(),
                                    flash_item == t.default_item,
                                    remaining % 2 == 0,
                                ))
                            } else {
                                None
                            };
                            (remaining, button_draw)
                        };

                        if let Some((screen_rect, title, is_default, highlighted)) = button_draw {
                            // Toggle button highlight.
                            self.draw_dialog_button_highlight_state(
                                bus,
                                screen_rect,
                                &title,
                                is_default,
                                highlighted,
                            );
                        }

                        if remaining == 0 {
                            // Flash complete — write all editText item handles
                            // back before returning the hit item to the app.
                            let (dialog_ptr, edit_item, edit_text, items) = {
                                let tracking = self.dialog_tracking.as_mut().unwrap();
                                Self::sync_tracking_active_edit_item(tracking);
                                (
                                    tracking.dialog_ptr,
                                    tracking.edit_item,
                                    tracking.edit_text.clone(),
                                    tracking.items.clone(),
                                )
                            };
                            self.flush_dialog_edit_item_texts(
                                bus, dialog_ptr, &items, edit_item, &edit_text,
                            );

                            let mut saved = self.dialog_tracking.take().unwrap();
                            let saved_dialog_ptr = saved.dialog_ptr;
                            let saved_bounds = saved.bounds;
                            let item_type = saved
                                .items
                                .get(flash_item.saturating_sub(1) as usize)
                                .map(|item| item.item_type);
                            if let Some(item) = saved
                                .items
                                .get(flash_item.saturating_sub(1) as usize)
                                .filter(|item| {
                                    (item.item_type & 0x7F) == 4 && (item.item_type & 0x80) == 0
                                })
                            {
                                self.restore_dialog_button_normal_state(
                                    bus,
                                    saved_dialog_ptr,
                                    saved_bounds,
                                    flash_item,
                                    item,
                                    flash_item == saved.default_item,
                                );
                                saved.rendered_pixels = self.save_dialog_pixels(bus, saved_bounds);
                                saved.rendered_pixels_final = true;
                            }
                            self.persist_visible_dialog_snapshot(bus, &saved);
                            self.dialog_saved_pixels
                                .insert(saved_dialog_ptr, saved.saved_pixels);
                            self.pending_modal_button_dispose_dialog = Some(saved_dialog_ptr);

                            if item_hit_ptr != 0 {
                                bus.write_word(item_hit_ptr, flash_item as u16);
                            }
                            self.record_modal_dialog_input_trace(
                                "finish",
                                saved_dialog_ptr,
                                saved_bounds,
                                flash_item,
                                item_type,
                                None,
                                "returned",
                                "button_item_hit",
                            );
                            cpu.write_reg(Register::A7, stack_ptr + 8);
                        }
                        return Some(Ok(()));
                    }

                    // With a non-NIL filterProc, ModalDialog passes each event
                    // to the filter before Dialog Manager default handling.
                    // Leave queued events alone here; runner.rs will inject the
                    // filter callback, and this handler will process the returned
                    // event only if that callback returns FALSE. MTE 1992 6-136.
                    if waiting_for_filter_proc_event {
                        return Some(Ok(()));
                    }

                    let event = if let Some(e) = pending_event {
                        Some(e)
                    } else if !self.event_queue.is_empty() {
                        // Drain events looking for actionable ones
                        let mut event = None;
                        while let Some(e) = self.event_queue.pop_front() {
                            match e.what {
                                1 | 2 | 3 | 6 => {
                                    event = Some(e);
                                    break;
                                }
                                _ => {} // discard other events
                            }
                        }
                        event
                    } else {
                        None
                    };

                    if let Some(mut e) = event {
                        if e.what == 0 && self.input_state.mouse_button {
                            e.what = 1;
                            e.where_v = self.input_state.mouse_pos.0;
                            e.where_h = self.input_state.mouse_pos.1;
                        }
                        match e.what {
                            // updateEvt — re-snapshot rendered_pixels.
                            // Redraw HLE popup controls first, since the game
                            // may have drawn narrow indicator boxes that would
                            // taint the snapshot.
                            6 => {
                                // MTE 1992 pp. 6-135 and 6-141: ModalDialog
                                // handles dialog update events through the same
                                // DialogSelect path that brackets DrawDialog
                                // with BeginUpdate/EndUpdate and makes the
                                // dialog the current graphics port.
                                self.begin_update_window(bus, dialog_ptr);
                                self.set_current_port_state(bus, cpu, dialog_ptr, None);
                                let previous_rendered = self
                                    .dialog_tracking
                                    .as_ref()
                                    .filter(|t| t.rendered_pixels_final)
                                    .map(|t| t.rendered_pixels.clone())
                                    .unwrap_or_default();
                                if !previous_rendered.is_empty() {
                                    self.restore_dialog_pixels(bus, bounds, &previous_rendered);
                                }
                                if let Some(ref t) = self.dialog_tracking {
                                    if !t.game_managed {
                                        self.redraw_standard_dialog_items(
                                            bus,
                                            bounds,
                                            &t.items,
                                            t.default_item,
                                            &t.edit_text,
                                            t.edit_item,
                                            t.dialog_ptr,
                                        );
                                    }
                                    let popup_draws = t.popup_draws.clone();
                                    self.redraw_dialog_popup_controls(bus, &popup_draws);
                                }
                                let rendered = self.save_dialog_pixels(bus, bounds);
                                let t = self.dialog_tracking.as_mut().unwrap();
                                t.rendered_pixels = rendered;
                                t.rendered_pixels_final = true;
                                self.end_update_window(bus, dialog_ptr);
                            }
                            // mouseDown
                            1 => {
                                // Clone items lazily — only when actually processing
                                // a mouseDown. Hot path (no events) skips this entirely.
                                let items_clone: Vec<DialogItem> = self
                                    .dialog_tracking
                                    .as_ref()
                                    .map(|t| t.items.clone())
                                    .unwrap_or_default();
                                let mut hit = self.dialog_item_hit_test(
                                    bus,
                                    &items_clone,
                                    bounds,
                                    e.where_v,
                                    e.where_h,
                                    &self.dialog_popup_original_rects,
                                    dialog_ptr,
                                );
                                if hit <= 0 {
                                    hit = Self::dialog_button_hit_test(
                                        &items_clone,
                                        bounds,
                                        e.where_v,
                                        e.where_h,
                                    );
                                }
                                if hit > 0 {
                                    let item = &items_clone[(hit - 1) as usize];
                                    let base_type = item.item_type & 0x7F;
                                    let is_disabled = (item.item_type & 0x80) != 0;

                                    if !is_disabled {
                                        match base_type {
                                            // Button click: start flash
                                            4 => {
                                                let (abs_top, abs_left, abs_bottom, abs_right) =
                                                    Self::dialog_item_screen_rect(
                                                        bounds, item.rect,
                                                    );
                                                let is_default = hit
                                                    == self
                                                        .dialog_tracking
                                                        .as_ref()
                                                        .map(|tracking| tracking.default_item)
                                                        .unwrap_or(0);
                                                self.draw_dialog_button_highlight_state(
                                                    bus,
                                                    (abs_top, abs_left, abs_bottom, abs_right),
                                                    &item.text,
                                                    is_default,
                                                    true,
                                                );
                                                if self.input_state.mouse_button {
                                                    let t = self.dialog_tracking.as_mut().unwrap();
                                                    t.active_button = Some(
                                                        super::dispatch::DialogButtonTrackingState {
                                                            mouse_down: e.clone(),
                                                            item_no: hit,
                                                            rect: item.rect,
                                                            title: item.text.clone(),
                                                            is_default,
                                                            highlighted: true,
                                                        },
                                                    );
                                                    self.record_modal_dialog_input_trace(
                                                        "mouse_down",
                                                        dialog_ptr,
                                                        bounds,
                                                        hit,
                                                        Some(item.item_type),
                                                        Some(true),
                                                        "pending",
                                                        "button_tracking_started",
                                                    );
                                                } else {
                                                    self.start_dialog_button_flash(
                                                        bus, bounds, hit, item.rect, &item.text,
                                                        is_default, true,
                                                    );
                                                    self.record_modal_dialog_input_trace(
                                                        "mouse_down",
                                                        dialog_ptr,
                                                        bounds,
                                                        hit,
                                                        Some(item.item_type),
                                                        Some(true),
                                                        "pending",
                                                        "start_flash",
                                                    );
                                                }
                                            }
                                            // Checkbox click: return item number immediately
                                            // The dialog stays on screen — the app toggles
                                            // the checkbox value and calls ModalDialog again.
                                            // Inside Macintosh Volume I, I-415
                                            5 => {
                                                let (dlg_ptr, edit_item, edit_text, items) = {
                                                    let tracking =
                                                        self.dialog_tracking.as_mut().unwrap();
                                                    Self::sync_tracking_active_edit_item(tracking);
                                                    (
                                                        tracking.dialog_ptr,
                                                        tracking.edit_item,
                                                        tracking.edit_text.clone(),
                                                        tracking.items.clone(),
                                                    )
                                                };
                                                self.flush_dialog_edit_item_texts(
                                                    bus, dlg_ptr, &items, edit_item, &edit_text,
                                                );
                                                // Preserve saved background pixels for re-entry
                                                let saved = self.dialog_tracking.take().unwrap();
                                                let saved_dialog_ptr = saved.dialog_ptr;
                                                let saved_bounds = saved.bounds;
                                                self.persist_visible_dialog_snapshot(bus, &saved);
                                                self.dialog_saved_pixels
                                                    .insert(saved_dialog_ptr, saved.saved_pixels);
                                                self.consume_or_retain_dialog_mouse_up(&e);
                                                // Don't restore pixels — dialog stays visible
                                                if item_hit_ptr != 0 {
                                                    bus.write_word(item_hit_ptr, hit as u16);
                                                }
                                                self.record_modal_dialog_input_trace(
                                                    "mouse_down",
                                                    saved_dialog_ptr,
                                                    saved_bounds,
                                                    hit,
                                                    Some(item.item_type),
                                                    None,
                                                    "returned",
                                                    "checkbox_item_hit_retained",
                                                );
                                                cpu.write_reg(Register::A7, stack_ptr + 8);
                                            }
                                            // EditText click: set as active
                                            16 => {
                                                let (dlg_ptr, edit_item, edit_text, items) = {
                                                    let tracking =
                                                        self.dialog_tracking.as_mut().unwrap();
                                                    Self::sync_tracking_active_edit_item(tracking);
                                                    tracking.edit_item = hit;
                                                    tracking.edit_text = tracking
                                                        .items
                                                        .get((hit - 1) as usize)
                                                        .map(|item| item.text.clone())
                                                        .unwrap_or_default();
                                                    tracking.edit_text_modified = false;
                                                    (
                                                        tracking.dialog_ptr,
                                                        tracking.edit_item,
                                                        tracking.edit_text.clone(),
                                                        tracking.items.clone(),
                                                    )
                                                };
                                                // IM:I I-415: mouseDown in an enabled editText
                                                // item is TextEdit-handled and ModalDialog
                                                // returns that item. TEClick's pixel-to-caret
                                                // mapping remains the documented HLE compromise,
                                                // but the active editField/TERecord mirror is
                                                // still guest-visible Dialog Manager state.
                                                self.activate_dialog_edit_item(
                                                    bus, cpu, dlg_ptr, &items, edit_item,
                                                );
                                                self.flush_dialog_edit_item_texts(
                                                    bus, dlg_ptr, &items, edit_item, &edit_text,
                                                );
                                                let saved = self.dialog_tracking.take().unwrap();
                                                let saved_dialog_ptr = saved.dialog_ptr;
                                                let saved_bounds = saved.bounds;
                                                self.persist_visible_dialog_snapshot(bus, &saved);
                                                self.dialog_saved_pixels
                                                    .insert(saved_dialog_ptr, saved.saved_pixels);
                                                self.consume_or_retain_dialog_mouse_up(&e);
                                                if item_hit_ptr != 0 {
                                                    bus.write_word(item_hit_ptr, hit as u16);
                                                }
                                                self.record_modal_dialog_input_trace(
                                                    "mouse_down",
                                                    saved_dialog_ptr,
                                                    saved_bounds,
                                                    hit,
                                                    Some(item.item_type),
                                                    None,
                                                    "returned",
                                                    "edittext_item_hit_retained",
                                                );
                                                cpu.write_reg(Register::A7, stack_ptr + 8);
                                            }
                                            // resCtrl popup controls are tracked by
                                            // ModalDialog itself, matching the standard
                                            // Dialog Manager control-item path.
                                            7 if self.begin_dialog_popup_tracking(
                                                bus, dialog_ptr, hit,
                                            ) => {}
                                            // UserItem with popup menu: return item number.
                                            // Dialog stays on screen — the app calls
                                            // PopUpMenuSelect to show the popup dropdown.
                                            // Macintosh Toolbox Essentials 1992, 5-26.
                                            // Original Marathon-style compact popup candidates
                                            // use the same app-owned tracking path after
                                            // InsertMenu/GetDItem/SetDItem narrows the DITL rect.
                                            0 if self
                                                .dialog_item_popup_menus
                                                .contains_key(&(dialog_ptr, hit))
                                                || self
                                                    .dialog_popup_candidate_items
                                                    .contains(&(dialog_ptr, hit)) =>
                                            {
                                                let (dlg_ptr, edit_item, edit_text, items) = {
                                                    let tracking =
                                                        self.dialog_tracking.as_mut().unwrap();
                                                    Self::sync_tracking_active_edit_item(tracking);
                                                    (
                                                        tracking.dialog_ptr,
                                                        tracking.edit_item,
                                                        tracking.edit_text.clone(),
                                                        tracking.items.clone(),
                                                    )
                                                };
                                                self.flush_dialog_edit_item_texts(
                                                    bus, dlg_ptr, &items, edit_item, &edit_text,
                                                );
                                                let saved = self.dialog_tracking.take().unwrap();
                                                self.persist_visible_dialog_snapshot(bus, &saved);
                                                self.dialog_saved_pixels
                                                    .insert(saved.dialog_ptr, saved.saved_pixels);
                                                self.consume_or_retain_dialog_mouse_up(&e);
                                                if item_hit_ptr != 0 {
                                                    bus.write_word(item_hit_ptr, hit as u16);
                                                }
                                                cpu.write_reg(Register::A7, stack_ptr + 8);
                                            }
                                            // Plain userItems in standard dialogs are custom
                                            // hit targets whose content and mouse tracking are
                                            // application-owned (IM:I I-405). Some applications
                                            // implement draggable custom controls (e.g. EV
                                            // Override's Game Speed slider) by running their own
                                            // StillDown()/GetMouse() tracking loop after
                                            // ModalDialog returns the item on the initial press,
                                            // so the hit must be returned while the button is
                                            // still down — holding it until release leaves the
                                            // application's tracking loop with nothing to follow.
                                            // Return the hit immediately on mouse-down; the
                                            // pending mouse-up stays queued so StillDown() and the
                                            // application loop still observe the release.
                                            0 if self.dialog_tracking.as_ref().is_some_and(
                                                |tracking| {
                                                    self.is_plain_modal_user_item(
                                                        tracking, hit, item,
                                                    )
                                                },
                                            ) && self.input_state.mouse_button =>
                                            {
                                                let (dlg_ptr, edit_item, edit_text, items) = {
                                                    let tracking =
                                                        self.dialog_tracking.as_mut().unwrap();
                                                    Self::sync_tracking_active_edit_item(tracking);
                                                    (
                                                        tracking.dialog_ptr,
                                                        tracking.edit_item,
                                                        tracking.edit_text.clone(),
                                                        tracking.items.clone(),
                                                    )
                                                };
                                                self.flush_dialog_edit_item_texts(
                                                    bus, dlg_ptr, &items, edit_item, &edit_text,
                                                );
                                                let saved = self.dialog_tracking.take().unwrap();
                                                self.persist_visible_dialog_snapshot(bus, &saved);
                                                self.dialog_saved_pixels
                                                    .insert(saved.dialog_ptr, saved.saved_pixels);
                                                if item_hit_ptr != 0 {
                                                    bus.write_word(item_hit_ptr, hit as u16);
                                                }
                                                cpu.write_reg(Register::A7, stack_ptr + 8);
                                            }
                                            // Any other enabled item: return item number immediately.
                                            // Inside Macintosh Volume I, I-428
                                            _ => {
                                                let (dlg_ptr, edit_item, edit_text, items) = {
                                                    let tracking =
                                                        self.dialog_tracking.as_mut().unwrap();
                                                    Self::sync_tracking_active_edit_item(tracking);
                                                    (
                                                        tracking.dialog_ptr,
                                                        tracking.edit_item,
                                                        tracking.edit_text.clone(),
                                                        tracking.items.clone(),
                                                    )
                                                };
                                                self.flush_dialog_edit_item_texts(
                                                    bus, dlg_ptr, &items, edit_item, &edit_text,
                                                );
                                                let saved = self.dialog_tracking.take().unwrap();
                                                self.persist_visible_dialog_snapshot(bus, &saved);
                                                self.dialog_saved_pixels
                                                    .insert(saved.dialog_ptr, saved.saved_pixels);
                                                self.consume_or_retain_dialog_mouse_up(&e);
                                                if item_hit_ptr != 0 {
                                                    bus.write_word(item_hit_ptr, hit as u16);
                                                }
                                                cpu.write_reg(Register::A7, stack_ptr + 8);
                                            }
                                        }
                                    }
                                }
                            }
                            // keyDown
                            3 => {
                                let char_code = (e.message & 0xFF) as u8;
                                let key_code = ((e.message >> 8) & 0xFF) as u8;
                                let command_period =
                                    char_code == b'.' && (e.modifiers & 0x0100) != 0;
                                let command_printable =
                                    (e.modifiers & 0x0100) != 0 && matches!(char_code, 0x20..=0x7E);
                                match char_code {
                                    // Return or Enter: trigger default button
                                    0x0D | 0x03 => {
                                        let target =
                                            self.dialog_tracking.as_ref().and_then(|tracking| {
                                                let def = tracking.default_item;
                                                if def <= 0 {
                                                    return None;
                                                }
                                                tracking
                                                    .items
                                                    .get(def.saturating_sub(1) as usize)
                                                    .map(|item| {
                                                        (def, item.rect, item.text.clone(), true)
                                                    })
                                            });
                                        if let Some((def, rect, title, is_default)) = target {
                                            let screen_rect =
                                                Self::dialog_item_screen_rect(bounds, rect);
                                            self.draw_dialog_button_highlight_state(
                                                bus,
                                                screen_rect,
                                                &title,
                                                is_default,
                                                true,
                                            );
                                            let t = self.dialog_tracking.as_mut().unwrap();
                                            t.flash_remaining = 6;
                                            t.flash_delay = 3;
                                            t.flash_item = def;
                                        }
                                    }
                                    // Escape or Command-period: trigger cancel button.
                                    // MTE 1992 p. 6-138 maps Esc and Command-period
                                    // to Cancel before dialog-select style handling.
                                    0x1B | b'.' if char_code == 0x1B || command_period => {
                                        let target =
                                            self.dialog_tracking.as_ref().and_then(|tracking| {
                                                let cancel = tracking.cancel_item;
                                                if cancel <= 0 {
                                                    return None;
                                                }
                                                tracking
                                                    .items
                                                    .get(cancel.saturating_sub(1) as usize)
                                                    .map(|item| {
                                                        (
                                                            cancel,
                                                            item.rect,
                                                            item.text.clone(),
                                                            cancel == tracking.default_item,
                                                        )
                                                    })
                                            });
                                        if let Some((cancel, rect, title, is_default)) = target {
                                            let screen_rect =
                                                Self::dialog_item_screen_rect(bounds, rect);
                                            self.draw_dialog_button_highlight_state(
                                                bus,
                                                screen_rect,
                                                &title,
                                                is_default,
                                                true,
                                            );
                                            let t = self.dialog_tracking.as_mut().unwrap();
                                            t.flash_remaining = 6;
                                            t.flash_delay = 3;
                                            t.flash_item = cancel;
                                        }
                                    }
                                    // Tab: move to the next editText item, wrapping.
                                    0x09 => {
                                        let mut switched = false;
                                        if let Some(tracking) = self.dialog_tracking.as_mut() {
                                            if tracking.edit_item > 0 {
                                                Self::sync_tracking_active_edit_item(tracking);
                                            }
                                            if !tracking.items.is_empty() {
                                                let start = tracking.edit_item.max(0) as usize;
                                                let next =
                                                    (0..tracking.items.len()).find_map(|offset| {
                                                        let idx =
                                                            (start + offset) % tracking.items.len();
                                                        if (tracking.items[idx].item_type & 0x7F)
                                                            == 16
                                                        {
                                                            Some(idx)
                                                        } else {
                                                            None
                                                        }
                                                    });
                                                if let Some(idx) = next {
                                                    tracking.edit_item = (idx + 1) as i16;
                                                    tracking.edit_text =
                                                        tracking.items[idx].text.clone();
                                                    tracking.edit_text_modified = false;
                                                    bus.write_word(dialog_ptr + 164, idx as u16);
                                                    switched = true;
                                                }
                                            }
                                        }
                                        if switched {
                                            self.refresh_dialog_tracking_snapshot(bus);
                                        }
                                    }
                                    // Unhandled Command-key equivalents belong
                                    // to the application and Menu Manager.
                                    // ModalDialog ignores them instead of
                                    // inserting their printable character into
                                    // the active editText item. Command-period
                                    // is handled above as Cancel.
                                    // Inside Macintosh Volume I, I-415, I-428.
                                    _ if command_printable => {}
                                    // Backspace/Delete or printable ASCII.
                                    0x08 | 0x20..=0x7E => {
                                        let mut text_trace = None;
                                        let mut modified_key_to_set = None;
                                        if let Some(tracking) = self.dialog_tracking.as_mut() {
                                            let text_before = tracking.edit_text.clone();
                                            if tracking.edit_item > 0 {
                                                if char_code == 0x08 {
                                                    if !tracking.edit_text_modified {
                                                        // First backspace clears selection
                                                        tracking.edit_text.clear();
                                                        tracking.edit_text_modified = true;
                                                    } else if !tracking.edit_text.is_empty() {
                                                        tracking.edit_text.pop();
                                                    }
                                                } else {
                                                    if !tracking.edit_text_modified {
                                                        // First keypress replaces selection
                                                        tracking.edit_text.clear();
                                                        tracking.edit_text_modified = true;
                                                    }
                                                    tracking.edit_text.push(char_code as char);
                                                }
                                                let cursor =
                                                    encode_mac_roman_lossy(&tracking.edit_text)
                                                        .len();
                                                Self::set_tracking_active_edit_selection(
                                                    tracking, cursor, cursor,
                                                );
                                                Self::sync_tracking_active_edit_item(tracking);
                                                modified_key_to_set =
                                                    Some((tracking.dialog_ptr, tracking.edit_item));

                                                let edit_item = tracking.edit_item;
                                                let item_type = tracking
                                                    .items
                                                    .get((edit_item - 1) as usize)
                                                    .map(|item| item.item_type);
                                                let enabled_edit_text = item_type
                                                    .map(|ty| (ty & 0x7F) == 16 && (ty & 0x80) == 0)
                                                    .unwrap_or(false);
                                                let text_after = tracking.edit_text.clone();
                                                text_trace = Some((
                                                    edit_item,
                                                    item_type,
                                                    text_before,
                                                    text_after,
                                                    enabled_edit_text,
                                                ));
                                            }
                                        }
                                        if let Some(key) = modified_key_to_set {
                                            self.dialog_edit_text_modified_items.insert(key);
                                        }
                                        if let Some((
                                            edit_item,
                                            item_type,
                                            text_before,
                                            text_after,
                                            enabled_edit_text,
                                        )) = text_trace
                                        {
                                            self.refresh_dialog_tracking_snapshot(bus);
                                            let outcome = if enabled_edit_text {
                                                "enabled_edittext_item_hit"
                                            } else {
                                                "edittext_updated"
                                            };
                                            if enabled_edit_text {
                                                let (dlg_ptr, active_edit_item, edit_text, items) = {
                                                    let tracking =
                                                        self.dialog_tracking.as_mut().unwrap();
                                                    Self::sync_tracking_active_edit_item(tracking);
                                                    (
                                                        tracking.dialog_ptr,
                                                        tracking.edit_item,
                                                        tracking.edit_text.clone(),
                                                        tracking.items.clone(),
                                                    )
                                                };
                                                self.flush_dialog_edit_item_texts(
                                                    bus,
                                                    dlg_ptr,
                                                    &items,
                                                    active_edit_item,
                                                    &edit_text,
                                                );
                                                let saved = self.dialog_tracking.take().unwrap();
                                                let saved_dialog_ptr = saved.dialog_ptr;
                                                self.persist_visible_dialog_snapshot(bus, &saved);
                                                self.dialog_saved_pixels
                                                    .insert(saved_dialog_ptr, saved.saved_pixels);
                                                if item_hit_ptr != 0 {
                                                    bus.write_word(
                                                        item_hit_ptr,
                                                        active_edit_item as u16,
                                                    );
                                                }
                                                cpu.write_reg(Register::A7, stack_ptr + 8);
                                                self.record_modal_dialog_text_input_trace(
                                                    "key_down",
                                                    dialog_ptr,
                                                    bounds,
                                                    edit_item,
                                                    item_type,
                                                    key_code,
                                                    char_code,
                                                    &text_before,
                                                    &text_after,
                                                    "returned",
                                                    outcome,
                                                );
                                            } else {
                                                self.record_modal_dialog_text_input_trace(
                                                    "key_down",
                                                    dialog_ptr,
                                                    bounds,
                                                    edit_item,
                                                    item_type,
                                                    key_code,
                                                    char_code,
                                                    &text_before,
                                                    &text_after,
                                                    "pending",
                                                    outcome,
                                                );
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                } else {
                    // First call: initialize dialog tracking
                    let sp = cpu.read_reg(Register::A7);
                    let item_hit_ptr = bus.read_long(sp);
                    let filter_proc = bus.read_long(sp + 4);
                    if item_hit_ptr != 0 {
                        bus.write_word(item_hit_ptr, 0);
                    }

                    // Find the modal dialog's items. Most callers keep the
                    // dialog as the front window, but games can temporarily
                    // select another port/window between ModalDialog returns
                    // and re-entry (for example while tracking a popup menu).
                    // A real modal dialog remains the Dialog Manager target
                    // until DisposDialog, so prefer the retained visible
                    // modal snapshot when the current front window is not a
                    // known dialog.
                    let mut dialog_ptr = self.front_window;
                    if !self.dialog_items.contains_key(&dialog_ptr) {
                        if let Some((&retained_dialog_ptr, snapshot)) =
                            self.dialog_visible_snapshots.iter().next()
                        {
                            dialog_ptr = retained_dialog_ptr;
                            self.front_window = retained_dialog_ptr;
                            self.window_bounds = snapshot.bounds;
                            self.window_proc_id = self
                                .window_proc_ids
                                .get(&retained_dialog_ptr)
                                .copied()
                                .unwrap_or(self.window_proc_id);
                            self.window_title.clear();
                        }
                    }
                    if let Some(mut items) = self.dialog_items.get(&dialog_ptr).cloned() {
                        if trace_dialog_filter_enabled() {
                            eprintln!(
                                "[DIALOG-FILTER] init dialog=${:08X} items={} filter_proc=${:08X} item_hit_ptr=${:08X}",
                                dialog_ptr,
                                items.len(),
                                filter_proc,
                                item_hit_ptr
                            );
                        }
                        // Re-read userItem proc pointers from guest memory.
                        // The game may have written them directly to the DITL
                        // handle data after GetNewDialog returned.
                        Self::refresh_ditl_proc_ptrs(bus, dialog_ptr, &mut items);
                        let record_bounds = Self::dialog_screen_bounds(bus, dialog_ptr);
                        let bounds = if record_bounds.0 < record_bounds.2
                            && record_bounds.1 < record_bounds.3
                        {
                            record_bounds
                        } else {
                            self.window_bounds
                        };
                        let proc_id = self.window_proc_id;
                        let title = self.window_title.clone();

                        let (edit_text, edit_item, default_item) =
                            Self::dialog_edit_state(bus, dialog_ptr, &items);
                        let cancel_item = self
                            .dialog_cancel_items
                            .get(&dialog_ptr)
                            .copied()
                            .unwrap_or(2);
                        let edit_text_modified = edit_item > 0
                            && self
                                .dialog_edit_text_modified_items
                                .contains(&(dialog_ptr, edit_item));

                        // If in-bounds items are all userItems, the game
                        // manages drawing itself; offscreen placeholders do
                        // not make this a standard dialog.
                        let game_managed = Self::dialog_is_game_managed(bounds, &items);

                        if trace_dialog_procs_enabled() {
                            for (i, item) in items.iter().enumerate() {
                                eprintln!(
                                    "[DIALOG-PROC] dialog=${:08X} item={} type={} proc=${:08X} rect=({},{},{},{}) text={:?}",
                                    dialog_ptr, i + 1, item.item_type, item.proc_ptr,
                                    item.rect.0, item.rect.1, item.rect.2, item.rect.3,
                                    item.text,
                                );
                                if (item.item_type & 0x7F) == 0 {
                                    eprintln!(
                                        "[DIALOG-PROC] dialog=${:08X} item={} type={} proc=${:08X}",
                                        dialog_ptr,
                                        i + 1,
                                        item.item_type,
                                        item.proc_ptr,
                                    );
                                }
                            }
                        }

                        // Save pixels under dialog area (background to restore on dismiss).
                        // If we have preserved pixels from a previous non-dismissing return
                        // (e.g., popup click), reuse those instead of capturing the
                        // currently visible dialog as "background."
                        let is_reentry = self.dialog_modal_entered.contains(&dialog_ptr);
                        let preserved_visible_snapshot =
                            self.dialog_visible_snapshots.remove(&dialog_ptr);
                        let reused_retained_visible_snapshot =
                            is_reentry && preserved_visible_snapshot.is_some();
                        let app_painted = self.dialogs_drawn_by_app.contains(&dialog_ptr);
                        let preserved_saved_pixels =
                            self.dialog_saved_pixels.get(&dialog_ptr).cloned();
                        let restored_visible_snapshot =
                            preserved_visible_snapshot.is_some() && (is_reentry || !app_painted);
                        let saved_pixels = preserved_saved_pixels
                            .unwrap_or_else(|| self.save_dialog_pixels(bus, bounds));
                        if let Some(snapshot) = preserved_visible_snapshot {
                            if restored_visible_snapshot {
                                self.restore_dialog_pixels(bus, snapshot.bounds, &snapshot.pixels);
                            }
                        }
                        // ModalDialog gets and handles events; it does not
                        // repaint the dialog. When the application already
                        // called DrawDialog itself, anything it drew into the
                        // dialog afterwards — Civilization's demo notice text
                        // sits in the dialog body, outside any DITL item — is
                        // still on screen, and repainting here would erase it.
                        // Inside Macintosh Volume I, I-415.
                        if !game_managed && !is_reentry && !app_painted {
                            // First entry: draw the dialog chrome and controls.
                            // Before draw_dialog fills the dialog area white, save the
                            // pixel content of every userItem rect when those pixels
                            // come from a visible-dialog snapshot. Games (e.g.
                            // Marathon) often draw custom controls (popup buttons,
                            // sliders) into userItem rects via QuickDraw before
                            // calling ModalDialog. Saved-under background pixels are
                            // only for dismissal restore and must not be treated as
                            // application-owned userItem drawing.
                            // Inside Macintosh Volume I, I-405
                            self.dialog_initial_draw_deferred.remove(&dialog_ptr);
                            self.draw_dialog_preserving_user_items(
                                bus,
                                bounds,
                                proc_id,
                                &title,
                                &items,
                                default_item,
                                &edit_text,
                                edit_item,
                                false,
                                dialog_ptr,
                                restored_visible_snapshot,
                                true,
                                true,
                            );
                        }

                        // HLE-draw popup controls for type-0 userItems that were
                        // associated with MENU resources via the InsertMenu → GetDItem
                        // pattern used by games (e.g. Marathon) to set up popup
                        // controls in dialogs.  We find the checked item (mark=0x12)
                        // in each menu and draw a standard popup button for it.
                        // Inside Macintosh Volume I, I-405 (userItem draw responsibilities)
                        let implicit_popup_candidate_count = items
                            .iter()
                            .enumerate()
                            .filter(|(i, item)| {
                                let item_no = (*i + 1) as i16;
                                (item.item_type & 0x7F) == 0
                                    && !self
                                        .dialog_item_popup_menus
                                        .contains_key(&(dialog_ptr, item_no))
                                    && self
                                        .dialog_popup_candidate_items
                                        .contains(&(dialog_ptr, item_no))
                            })
                            .count();
                        let mut implicit_popup_menu_ids: Vec<i16> = self
                            .menus
                            .iter()
                            .filter(|menu| {
                                menu.in_menu_bar
                                    && !menu.visible_in_menu_bar
                                    && menu.items.iter().any(|item| item.mark == 0x12)
                            })
                            .map(|menu| menu.id)
                            .collect();
                        if implicit_popup_menu_ids.len() < implicit_popup_candidate_count {
                            implicit_popup_menu_ids = self
                                .menus
                                .iter()
                                .filter(|menu| menu.items.iter().any(|item| item.mark == 0x12))
                                .map(|menu| menu.id)
                                .collect();
                        }
                        let mut implicit_popup_menu_index = 0usize;
                        let popup_draws: Vec<DialogPopupDraw> = items
                            .iter()
                            .enumerate()
                            .filter_map(|(i, item)| {
                                let item_no = (i + 1) as i16;
                                if (item.item_type & 0x7F) != 0 {
                                    return None;
                                }
                                let key = (dialog_ptr, item_no);
                                let (menu_id, original_rect) = if let Some(&menu_id) =
                                    self.dialog_item_popup_menus.get(&key)
                                {
                                    let rect = self
                                        .dialog_popup_original_rects
                                        .get(&key)
                                        .copied()
                                        .unwrap_or(item.rect);
                                    (menu_id, rect)
                                } else if self.dialog_popup_candidate_items.contains(&key) {
                                    let menu_id =
                                        *implicit_popup_menu_ids.get(implicit_popup_menu_index)?;
                                    implicit_popup_menu_index += 1;
                                    let rect = self
                                        .dialog_popup_original_rects
                                        .get(&key)
                                        .copied()
                                        .unwrap_or(item.rect);
                                    (menu_id, rect)
                                } else {
                                    return None;
                                };
                                let checked_text = self
                                    .menus
                                    .iter()
                                    .find(|m| m.id == menu_id)
                                    .and_then(|m| {
                                        m.items
                                            .iter()
                                            .find(|mi| mi.mark == 0x12)
                                            .map(|mi| mi.text.clone())
                                    })
                                    .unwrap_or_default();
                                // Use the original DITL rect (before SetDItem narrowed it)
                                let (it_t, it_l, it_b, it_r) = original_rect;
                                Some(DialogPopupDraw {
                                    rect: (
                                        bounds.0 + it_t,
                                        bounds.1 + it_l,
                                        bounds.0 + it_b,
                                        bounds.1 + it_r,
                                    ),
                                    title: checked_text,
                                    enabled: (item.item_type & 0x80) == 0,
                                    pressed: false,
                                })
                            })
                            .collect();
                        self.redraw_dialog_popup_controls(bus, &popup_draws);

                        // Snapshot the fully rendered dialog (including PICTs) so
                        // redraw_chrome can restore it without re-parsing pictures.
                        // If there are userItem draw procs, this will be re-snapshotted
                        // after they execute.
                        let rendered_pixels = self.save_dialog_pixels(bus, bounds);

                        // Collect userItem draw procs to call. ShowWindow can
                        // create a visible snapshot before the first
                        // ModalDialog entry, and that first entry still needs
                        // the initial userItem update pass. Only skip the
                        // queue when the same retained modal dialog is
                        // re-entered after a non-dismissing return: that
                        // restored visible snapshot already contains the
                        // completed userItem output, and a real ModalDialog
                        // re-entry does not manufacture a fresh update pass
                        // just because the app called it again.
                        // Inside Macintosh Volume I, I-405 and I-415.
                        let mut draw_proc_queue = VecDeque::new();
                        if !reused_retained_visible_snapshot {
                            for (i, item) in items.iter().enumerate() {
                                let base_type = item.item_type & 0x7F;
                                if base_type == 0
                                    && item.proc_ptr != 0
                                    && Self::dialog_item_intersects_bounds(bounds, item)
                                {
                                    draw_proc_queue.push_back((item.proc_ptr, (i + 1) as i16));
                                }
                            }
                        }
                        let has_draw_procs = !draw_proc_queue.is_empty();

                        self.dialog_modal_entered.insert(dialog_ptr);
                        self.dialog_tracking = Some(super::dispatch::DialogTrackingState {
                            dialog_ptr,
                            bounds,
                            title,
                            proc_id,
                            items,
                            default_item,
                            cancel_item,
                            edit_text,
                            edit_item,
                            saved_pixels,
                            stack_ptr: sp,
                            item_hit_ptr,
                            rendered_pixels,
                            flash_remaining: 0,
                            flash_delay: 0,
                            flash_item: 0,
                            edit_text_modified,
                            draw_proc_queue,
                            draw_procs_done: !has_draw_procs,
                            rendered_pixels_final: !has_draw_procs,
                            filter_presentation_epoch: None,
                            filter_proc,
                            game_managed,
                            last_filter_event: None,
                            popup_draws,
                            active_popup: None,
                            active_button: None,
                            active_user_item: None,
                        });
                        // ModalDialog is a re-fire trap. Run application CDEF
                        // draws after the HLE shell becomes visible, then
                        // resume at the trap instruction so the next pass can
                        // snapshot the guest-owned pixels before event
                        // handling continues.
                        let after_trap_pc = cpu.read_reg(Register::PC);
                        cpu.write_reg(Register::PC, after_trap_pc.wrapping_sub(2));
                        if self.arm_dialog_control_def_draws(cpu, bus, dialog_ptr) {
                            self.dialog_cdef_draw_pending_snapshot.insert(dialog_ptr);
                            if let Some(tracking) = self.dialog_tracking.as_mut() {
                                tracking.rendered_pixels_final = false;
                            }
                        } else {
                            cpu.write_reg(Register::PC, after_trap_pc);
                        }
                        self.record_modal_dialog_input_trace(
                            "start",
                            dialog_ptr,
                            bounds,
                            0,
                            None,
                            None,
                            "pending",
                            "open_modal_tracking",
                        );
                        // Don't pop stack or advance PC — re-fire pattern
                    } else {
                        // No items found — fall back to returning item 1
                        eprintln!("[TRAP] ModalDialog: no items found, returning 1");
                        if item_hit_ptr != 0 {
                            bus.write_word(item_hit_ptr, 1);
                        }
                        cpu.write_reg(Register::A7, sp + 8);
                    }
                }
                Ok(())
            }

            // ========== TextEdit Manager ==========
            // TEInit ($A9CC)
            // Initializes TextEdit's internal globals.
            // PROCEDURE TEInit;
            // Inside Macintosh Volume I, I-376 ("TEInit
            // initializes TextEdit by allocating a handle for
            // the TextEdit scrap. The scrap is initially empty.
            // Call this procedure once and only once at the
            // beginning of your program."). Also note from IM:I
            // I-376: "You should call TEInit even if your
            // application doesn't use TextEdit, so that desk
            // accessories and dialog and alert boxes will work
            // correctly."
            //
            // Per IM:I I-389 the scrap globals are TEScrpHandle
            // ($0AB4, 4-byte Handle to the empty/cut/copied
            // text block) and TEScrpLength ($0AB0, 2-byte
            // INTEGER byte count). TEInit must:
            //   1. Allocate a zero-length relocatable block
            //      and store its handle at TEScrpHandle.
            //   2. Set TEScrpLength to 0 (empty scrap).
            //
            // Idempotent: per IM the routine is documented as
            // "call once and only once" but defensive impls
            // check for an existing handle and skip the
            // re-allocation to avoid leaking the prior one. We
            // do the same — apps that violate the IM contract
            // and call TEInit twice get a stable handle (no
            // double-free).
            // TEInit ($A9CC): Per IM:I I-376 allocates a zero-length scrap handle and stores it at TEScrpHandle ($0AB4); zeros TEScrpLength ($0AB0). Idempotent — repeated calls reuse the existing handle to avoid leaking. TECopy / TECut / TEPaste subsequently resize the underlying block as needed. No args, no result.
            (true, 0x1CC) => {
                use crate::memory::globals::addr;
                // Idempotency: skip re-allocation if a prior
                // TEInit (or first-touch by TECopy / TECut /
                // TEPaste) already populated the handle.
                let existing = bus.read_long(addr::TE_SCRP_HANDLE);
                if existing == 0 {
                    // Allocate a handle whose master ptr is
                    // NIL (== empty scrap). Subsequent
                    // TECopy / TECut grow the underlying
                    // block via ensure_text_handle_size which
                    // tolerates the NIL master ptr by lazy-
                    // allocating on first non-empty write.
                    // This matches the existing
                    // TECopy / TECut first-touch pattern that
                    // calls allocate_handle_with_data(bus, 0).
                    let handle = Self::allocate_handle_with_data(bus, 0);
                    bus.write_long(addr::TE_SCRP_HANDLE, handle);
                }
                bus.write_word(addr::TE_SCRP_LENGTH, 0);
                Ok(())
            }

            // TEPinScroll ($A812)
            // Scrolls the text within the view rectangle by the
            // requested (dh, dv); stops scrolling when the last line
            // of text is scrolled into view.
            // PROCEDURE TEPinScroll(dh: INTEGER; dv: INTEGER; hTE: TEHandle);
            // Inside Macintosh: Text 1993, p. 2-91.
            //
            // IM:Text 1993 p. 2-91 verbatim:
            //   "The TEPinScroll procedure scrolls the text within the
            //    view rectangle of the specified edit record by the
            //    designated number of pixels. Scrolling stops when the
            //    last line of text is scrolled into view. ... The
            //    destination rectangle is offset by the amount
            //    scrolled. ... When the edit record is longer than the
            //    text it contains, TEPinScroll displays up to the last
            //    line of text inclusive, and not beyond it."
            //
            // Sign convention (IM:Text 1993 p. 2-91):
            //   dh > 0: text moves right → destRect.left/right += dh
            //   dh < 0: text moves left  → destRect.left/right += dh
            //   dv > 0: text moves down  → destRect.top/bottom += dv
            //   dv < 0: text moves up    → destRect.top/bottom += dv
            //
            // Pascal stack frame (args push left-to-right, first
            // source arg deepest):
            //   sp+0  hTE: TEHandle           (4) — last arg, shallowest
            //   sp+4  dv:  INTEGER            (2) — middle arg
            //   sp+6  dh:  INTEGER            (2) — first arg, deepest
            // Total pop = 8 bytes; no function-result slot.
            //
            // MPW Universal Headers TextEdit.h:
            //   EXTERN_API(void) TEPinScroll(short dh, short dv,
            //                                TEHandle hTE)
            //                                  ONEWORDINLINE(0xA812);
            //
            // Pin semantics: the dv arm clamps so dest_rect.top stays
            // within [view_top - (text_bottom - view_bottom), view_top]
            // — i.e. far enough that the last line of text remains
            // visible at the bottom of the view. The dh arm applies a
            // symmetric horizontal clamp. For in-range scrolls the
            // call behaves exactly like TEScroll ($A9DD) per the IM
            // "offset by the amount scrolled" guarantee.
            //
            // Regression coverage:
            //   dialog::tests::te_pin_scroll_reads_handle_from_stack_top
            //   dialog::tests::tepinscroll_in_range_negative_dv_offsets_destrect_top_and_bottom_exactly_by_dv
            //   dialog::tests::tepinscroll_pascal_lr_stack_layout_reads_dh_dv_and_hte_from_correct_offsets
            //   dialog::tests::tepinscroll_clamps_overscroll_when_last_line_is_already_visible
            // TEPinScroll ($A812): Offsets `destRect` by the requested delta and pops 8 bytes
            (true, 0x012) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let mut dv = bus.read_word(sp + 4) as i16;
                let mut dh = bus.read_word(sp + 6) as i16;
                cpu.write_reg(Register::A7, sp + 8);

                let te_ptr = Self::te_record_ptr(bus, te_handle);
                if te_ptr != 0 {
                    let view_rect = Self::te_read_rect(bus, te_ptr + Self::TE_VIEW_RECT_OFFSET);
                    let dest_rect = Self::te_read_rect(bus, te_ptr + Self::TE_DEST_RECT_OFFSET);

                    if dv > 0 {
                        let max_down = view_rect.0.saturating_sub(dest_rect.0);
                        if dv > max_down {
                            dv = max_down;
                        }
                    } else if dv < 0 {
                        // Scrolling up (dv < 0) is bounded by the
                        // distance between the text's bottom and
                        // the view's bottom — pinning behaviour
                        // stops once the last line is visible per
                        // Text 1993, 2-91. max_up = view_bottom -
                        // text_bottom: if text already fits (value
                        // ≥ 0) there's nothing to scroll up to, so
                        // dv clamps to 0. Otherwise max_up < 0
                        // gives the amount of up-scroll still available; clamp dv
                        // upward to max_up so it can't exceed that.
                        let text_len = Self::te_text_length(bus, te_handle);
                        let (end_top, _) = self.te_char_to_point(bus, te_handle, text_len);
                        let end_line = Self::te_char_to_line_index(bus, te_handle, text_len);
                        let text_bottom = end_top
                            .saturating_add(Self::te_height_for_line(bus, te_handle, end_line));
                        let max_up = view_rect.2.saturating_sub(text_bottom);
                        dv = if max_up >= 0 {
                            0
                        } else if dv < max_up {
                            max_up
                        } else {
                            dv
                        };
                    }

                    if dh > 0 {
                        let max_right = view_rect.1.saturating_sub(dest_rect.1);
                        dh = if max_right > 0 { dh.min(max_right) } else { 0 };
                    } else if dh < 0 {
                        let max_left = view_rect.1.saturating_sub(dest_rect.1);
                        dh = if max_left < 0 { dh.max(max_left) } else { 0 };
                    }

                    if trace_textedit_enabled() {
                        let adjusted = (
                            dest_rect.0.saturating_add(dv),
                            dest_rect.1.saturating_add(dh),
                            dest_rect.2.saturating_add(dv),
                            dest_rect.3.saturating_add(dh),
                        );
                        eprintln!(
                            "[TE] TEPinScroll hTE=${:08X} dh={} dv={} dest=({},{},{},{})",
                            te_handle, dh, dv, adjusted.0, adjusted.1, adjusted.2, adjusted.3
                        );
                    }
                    self.te_scroll_contents(cpu, bus, te_handle, dh, dv);
                }
                Ok(())
            }

            // TEAutoView ($A813)
            // Enables or disables automatic scrolling for an edit record.
            // PROCEDURE TEAutoView(fAuto: Boolean; hTE: TEHandle);
            // Text 1993, 2-92
            //
            // hTE is the LAST parameter (Pascal left-to-right push), so it
            // sits at SP+0 above the BOOLEAN at SP+4.
            // TEAutoView ($A813): Tracks the auto-scroll feature bit per TEHandle
            (true, 0x013) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                // Pascal BOOLEAN in high byte (MPW C convention).
                let enabled = bus.read_byte(sp + 4) != 0;
                self.set_te_feature_bit(te_handle, Self::TE_FEATURE_AUTO_SCROLL, enabled);
                if trace_textedit_enabled() {
                    eprintln!("[TE] TEAutoView hTE=${:08X} enabled={}", te_handle, enabled);
                }
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // TESelView ($A811)
            // PROCEDURE TESelView(hTE: TEHandle);
            // Inside Macintosh: Text (1993), p. 2-92.
            //
            // Per IM:Text 1993 p. 2-92 verbatim: "Once automatic scrolling
            // has been enabled by a call to the TEAutoView procedure or
            // through the TEFeatureFlag function, the TESelView procedure
            // ensures that the selection range is visible and scrolls it
            // into the view rectangle if necessary. ... The top left part
            // of the selection range is scrolled into view. ... If
            // automatic scrolling is disabled, TESelView has no effect."
            //
            // MPW Universal Headers TextEdit.h:
            //   EXTERN_API(void) TESelView(TEHandle hTE)
            //                              ONEWORDINLINE(0xA811);
            //
            // Pascal stack frame:
            //   sp+0  hTE: TEHandle  (4)
            // Total pop = 4 bytes. No function result.
            //
            // Algorithm (matches Apple's documented contract):
            //   1. If TE_FEATURE_AUTO_SCROLL is OFF on this hTE → no-op.
            //   2. Read viewRect, destRect, and the current selection
            //      range from the TERec.
            //   3. Resolve sel_start and sel_end character offsets to
            //      pixel coordinates (top_left of selection range and
            //      bottom_right via line-height lookup).
            //   4. Compute dh, dv via te_getdelta — the per-axis shift
            //      that brings the selection rectangle inside viewRect
            //      (zero if the selection is already inside).
            //   5. Call te_scroll_contents which adds (dh, dv) to
            //      destRect.{top,left,bottom,right} and redraws.
            //
            // BasiliskII-vs-Apple divergence note:
            //   BasiliskII System 7.5.3 ROM does NOT scroll destRect when
            //   auto-scroll is enabled and the selection lies below viewRect
            //   — pre and post destRect coincide at (0,0,200,30). Apple's
            //   IM:Text 1993 p. 2-92 says this case must scroll. Systemless
            //   implements the Apple-canonical semantic; the divergent rule
            //   is pinned by the assertion-bearing tests in this module.
            (true, 0x011) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                if self.te_feature_bit(te_handle, Self::TE_FEATURE_AUTO_SCROLL) {
                    let te_ptr = Self::te_record_ptr(bus, te_handle);
                    if te_ptr != 0 {
                        let view_rect = Self::te_read_rect(bus, te_ptr + Self::TE_VIEW_RECT_OFFSET);
                        let dest_rect = Self::te_read_rect(bus, te_ptr + Self::TE_DEST_RECT_OFFSET);
                        let sel_start = bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET) as usize;
                        let sel_end = bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) as usize;
                        let (start_top, start_left) =
                            self.te_char_to_point(bus, te_handle, sel_start);
                        let (stop_top, stop_left) = self.te_char_to_point(bus, te_handle, sel_end);
                        let stop_line = Self::te_char_to_line_index(bus, te_handle, sel_end);
                        let stop_bottom = stop_top
                            .saturating_add(Self::te_height_for_line(bus, te_handle, stop_line));
                        let dv =
                            Self::te_getdelta(start_top, stop_bottom, view_rect.0, view_rect.2);
                        let dh = Self::te_getdelta(start_left, stop_left, view_rect.1, view_rect.3);
                        if trace_textedit_enabled() {
                            let adjusted = (
                                dest_rect.0.saturating_add(dv),
                                dest_rect.1.saturating_add(dh),
                                dest_rect.2.saturating_add(dv),
                                dest_rect.3.saturating_add(dh),
                            );
                            eprintln!(
                                "[TE] TESelView hTE=${:08X} view=({},{},{},{}) dest=({},{},{},{}) adjusted=({},{},{},{})",
                                te_handle,
                                view_rect.0,
                                view_rect.1,
                                view_rect.2,
                                view_rect.3,
                                dest_rect.0,
                                dest_rect.1,
                                dest_rect.2,
                                dest_rect.3,
                                adjusted.0,
                                adjusted.1,
                                adjusted.2,
                                adjusted.3
                            );
                        }
                        self.te_scroll_contents(cpu, bus, te_handle, dh, dv);
                    }
                }
                Ok(())
            }

            // ========== Cursor Manager ==========

            // InitCursor ($A850) - Toolbox version
            // Resets to standard arrow cursor
            // InitCursor ($A850): Sets arrow cursor, resets cursor level to 0,
            // makes visible (IM:I I-167).
            (true, 0x050) => {
                self.cursor_state.init();
                Ok(())
            }

            // SetCursor ($A851)
            // PROCEDURE SetCursor(crsr: Cursor);
            // Cursor record: data[32] + mask[32] + hotSpot.v(2) + hotSpot.h(2) = 68 bytes
            // SetCursor ($A851): Reads 68-byte cursor record (16×16 data + mask + hotspot).
            // Per IM:I I-167, if the cursor is hidden it stays hidden and only
            // changes appearance when uncovered by matching ShowCursor calls.
            (true, 0x051) => {
                let sp = cpu.read_reg(Register::A7);
                let crsr_ptr = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);

                // Read cursor bitmap (16x16 = 32 bytes)
                let mut data = [0u8; 32];
                for (i, byte) in data.iter_mut().enumerate() {
                    *byte = bus.read_byte(crsr_ptr + i as u32);
                }
                // Read cursor mask (16x16 = 32 bytes)
                let mut mask = [0u8; 32];
                for (i, byte) in mask.iter_mut().enumerate() {
                    *byte = bus.read_byte(crsr_ptr + 32 + i as u32);
                }
                // Read hotspot
                let hot_v = bus.read_word(crsr_ptr + 64) as i16;
                let hot_h = bus.read_word(crsr_ptr + 66) as i16;

                self.cursor_state
                    .install(CursorImage::mono(data, mask, hot_v, hot_h));
                Ok(())
            }

            // HideCursor ($A852)
            // HideCursor ($A852): Decrements cursor level and hides while level < 0
            // per IM:I I-168.
            (true, 0x052) => {
                self.cursor_state.hide();
                Ok(())
            }

            // ShowCursor ($A853)
            // ShowCursor ($A853): Increments cursor level toward 0; extra calls
            // at level 0 are no-op (IM:I I-168).
            (true, 0x053) => {
                self.cursor_state.show();
                Ok(())
            }

            // ObscureCursor ($A856)
            // PROCEDURE ObscureCursor;
            // Inside Macintosh Volume I, I-168
            // Imaging With QuickDraw 1994, p. 8-29
            //
            // MPW Universal Headers (Quickdraw.h):
            //
            //   EXTERN_API(void) ObscureCursor(void) ONEWORDINLINE(0xA856);
            //
            // Pascal PROCEDURE with no arguments and no result slot:
            // caller pushes 0 bytes; trap pops 0 bytes; SP unchanged.
            //
            // Per IM:I I-168: "ObscureCursor hides the cursor until
            // the next time the mouse is moved. It's normally
            // called when the user begins to type. Unlike
            // HideCursor, it has no effect on the cursor level and
            // must not be balanced by a call to ShowCursor."
            //
            // HLE compromise: Systemless synthesizes mouse-move events
            // every frame from the scripted event source (or
            // every interactive frame from systemless). Honouring
            // the "hide until next mouse move" semantic would keep
            // the cursor PERMANENTLY hidden because each
            // synthesized mouse-move arrives before any "is the
            // mouse stationary?" check can materialise the cursor
            // (every frame produces both the obscure-trigger and
            // the un-obscure-trigger simultaneously). Treating it
            // as a no-op preserves cursor visibility — HideCursor
            // ($A852) / ShowCursor ($A853) still operate the
            // level-counter hide/show stack for explicit pairs in
            // apps that need them. Per IM:I I-168 explicit
            // "must not be balanced by a call to ShowCursor"
            // means apps universally call ObscureCursor without a
            // matching ShowCursor — so the no-op contract leaves
            // them in the same observable state (cursor visible,
            // level unchanged) regardless of dispatch.
            //
            // The Apple-canonical "hides until mouse move" and
            // "must not be balanced by ShowCursor" rules are pinned
            // in-Rust via `obscure_cursor_noop_preserves_cursor_level_visibility_and_stack`.
            // BII and Systemless HLE diverge on the LowMem CrsrVis
            // side-effect (BII System 7.5.3 ROM writes CrsrVis;
            // Systemless HLE keeps cursor state internal).
            //
            // ObscureCursor ($A856): No args / no result per IM:I I-168 MPW C declaration ObscureCursor(void) ONEWORDINLINE(0xA856) — HLE no-op; SP unchanged across calls.
            (true, 0x056) => Ok(()),

            // GetCursor ($A9B9)
            // FUNCTION GetCursor(cursorID: INTEGER): CursHandle;
            // Inside Macintosh Volume I, I-474
            //
            // "GetCursor returns a handle to the cursor having the
            // given resource ID, reading it from the resource file if
            // necessary. It calls the Resource Manager function
            // GetResource('CURS', cursorID). If the resource can't be
            // read, GetCursor returns NIL." — IM:I I-474.
            //
            // HLE compromise: Systemless doesn't load the System file's
            // resource fork, so the four standard system cursor IDs
            // documented at IM:I I-475..I-477 are synthesized here
            // via [`Self::synthesize_system_cursor`] (cached for
            // handle stability — apps cache the GetCursor result at
            // boot and pass it to SetCursor every frame). Any other
            // ID falls through to the IM-correct NIL miss path.
            //
            // The previous Stub allocated a fresh 68-byte zero-filled
            // block on every miss and returned a handle to it —
            // strictly worse than NIL since callers that defensively
            // check `if handle = NIL` got a non-NIL pointer to an
            // empty/white cursor and SetCursor'd a blank cursor onto
            // the screen. Same fallback issue as the GetIcon ($A9BB)
            // 128-byte uninitialised-heap stub closed by the
            // family-level resource fallback audit.
            //
            // Pop = 2 (cursorID INTEGER), result CursHandle at new SP+0.
            // GetCursor ($A9B9): Per IM:I I-474 calls GetResource('CURS', cursorID); on hit returns stable handle via get_or_create_resource_handle; on miss synthesizes built-in cursor 1..4 (iBeam/cross/plus/watch per IM:I I-475..I-477) via cached synthesize_system_cursor; otherwise returns NIL. Pops 2 bytes (cursorID), 4-byte CursHandle result at new SP+0.
            (true, 0x1B9) => {
                let sp = cpu.read_reg(Register::A7);
                let cursor_id = bus.read_word(sp) as i16;

                let handle = if let Some((refnum, ptr)) =
                    self.find_or_load_resource_any(bus, *b"CURS", cursor_id)
                {
                    self.get_or_create_resource_handle_in_file(
                        bus, *b"CURS", cursor_id, ptr, refnum,
                    )
                } else if let Some(ptr) = self.synthesize_system_cursor(bus, cursor_id) {
                    // Built-in cursor synthesised + cached. Use the
                    // resource-handle helper so subsequent GetCursor
                    // calls for the same ID return the same handle.
                    self.get_or_create_resource_handle(bus, *b"CURS", cursor_id, ptr)
                } else {
                    0
                };

                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // GetPattern ($A9B8)
            // FUNCTION GetPattern(patID: INTEGER): PatHandle;
            // Inside Macintosh Volume I, I-473
            //
            // "GetPattern returns a handle to the pattern having the
            // given resource ID, reading it from the resource file if
            // necessary. It calls the Resource Manager function
            // GetResource('PAT ', patID). If the resource can't be
            // read, GetPattern returns NIL." — IM:I I-473.
            //
            // The previous Stub allocated a fresh 8-byte all-0xFF
            // (white) pattern on every miss and returned a handle to
            // it — strictly worse than NIL since callers that
            // defensively check `if handle = NIL then use_default
            // else FillRect(rect, handle^^)` got a non-NIL handle and
            // proceeded to FillRect with white instead of taking the
            // recovery branch. Same fallback issue as the GetIcon
            // ($A9BB) and GetCursor ($A9B9) fallbacks closed in this
            // family's audit pass.
            //
            // Pop = 2 (patID INTEGER), result PatHandle at new SP+0.
            // GetPattern ($A9B8): Per IM:I I-473 calls GetResource('PAT ', patID); on hit returns stable handle via get_or_create_resource_handle; on miss returns NIL (previously a fresh all-0xFF white pattern, which made callers branching on `handle = NIL` take the wrong path). Pops 2 bytes (patID), 4-byte PatHandle result at new SP+0.
            (true, 0x1B8) => {
                let sp = cpu.read_reg(Register::A7);
                let pat_id = bus.read_word(sp) as i16;

                let handle = if let Some((refnum, ptr)) =
                    self.find_or_load_resource_any(bus, *b"PAT ", pat_id)
                {
                    self.get_or_create_resource_handle_in_file(bus, *b"PAT ", pat_id, ptr, refnum)
                } else {
                    0
                };

                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // GetIcon ($A9BB)
            // Returns a handle to the icon stored in the 'ICON' resource
            // with the given ID. Equivalent to GetResource('ICON', iconID).
            // The resource is a 128-byte black-and-white bitmap (32x32
            // pixels at 1 bit each).
            // FUNCTION GetIcon(iconID: INTEGER): Handle;
            // Inside Macintosh Volume I, I-473
            //
            // Mirrors GetPicture ($A9BC) exactly: look up the
            // resource via the dispatcher's resource search chain;
            // on hit, materialise (or reuse) a stable handle that
            // points at the loaded resource bytes; on miss, return
            // NIL per IM:I I-473 ("If the resource can't be read,
            // GetIcon returns NIL").
            //
            // The previous Stub allocated a fresh 128-byte block of
            // UNINITIALISED memory and returned a handle to it on
            // every call — strictly worse than NIL since callers
            // pass that handle to PlotIcon ($A94B) which CopyBits
            // the random bytes onto the framebuffer. Apps with a
            // missing 'ICON' that defensively check `if handle =
            // NIL` would crash on the dereference path; apps that
            // trust the result blindly would render a junk icon.
            // The proper Partial impl returns NIL on miss so both
            // branches behave correctly.
            //
            // Pop = 2 (iconID INTEGER), result Handle at new SP+0.
            // GetIcon ($A9BB): Per IM:I I-473 calls GetResource('ICON', iconID); returns handle to the loaded resource via get_or_create_resource_handle (stable handle reused across calls), or NIL if the ICON resource is missing. Pops 2 bytes (iconID), 4-byte Handle result at new SP+0. Mirrors GetPicture ($A9BC).
            (true, 0x1BB) => {
                let sp = cpu.read_reg(Register::A7);
                let icon_id = bus.read_word(sp) as i16;

                let handle = if let Some((refnum, ptr)) =
                    self.find_or_load_resource_any(bus, *b"ICON", icon_id)
                {
                    let h = self
                        .get_or_create_resource_handle_in_file(bus, *b"ICON", icon_id, ptr, refnum);
                    eprintln!(
                        "[TRAP] GetIcon({}) -> handle=${:08X} ptr=${:08X}",
                        icon_id, h, ptr
                    );
                    h
                } else if let Some(ptr) = self.synthesize_system_icon(bus, icon_id) {
                    let h = self.get_or_create_resource_handle(bus, *b"ICON", icon_id, ptr);
                    eprintln!(
                        "[TRAP] GetIcon({}) -> system handle=${:08X} ptr=${:08X}",
                        icon_id, h, ptr
                    );
                    h
                } else {
                    eprintln!("[TRAP] GetIcon({}) -> NIL (not found)", icon_id);
                    0
                };

                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // GetPicture ($A9BC)
            // Returns a handle to the picture stored in the 'PICT' resource
            // with the given ID. Equivalent to GetResource('PICT', picID).
            // FUNCTION GetPicture(picID: INTEGER): PicHandle;
            // Inside Macintosh Volume I, I-475
            // GetPicture ($A9BC): Loads PICT resource via GetResource, returns handle
            (true, 0x1BC) => {
                let sp = cpu.read_reg(Register::A7);
                let pic_id = bus.read_word(sp) as i16;

                let handle = if let Some((refnum, ptr)) =
                    self.find_or_load_resource_any(bus, *b"PICT", pic_id)
                {
                    let h = self
                        .get_or_create_resource_handle_in_file(bus, *b"PICT", pic_id, ptr, refnum);
                    eprintln!(
                        "[TRAP] GetPicture({}) -> handle=${:08X} ptr=${:08X}",
                        pic_id, h, ptr
                    );
                    h
                } else {
                    eprintln!("[TRAP] GetPicture({}) -> NIL (not found)", pic_id);
                    0
                };

                // GetPicture is a thin GetResource('PICT', picID) wrapper;
                // preserve GetResource's ResErr contract for both hits and
                // misses so a stale error from an earlier resource lookup
                // cannot invalidate a returned PicHandle.
                bus.write_word(0x0A60, 0);
                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // GetString ($A9BA)
            // Returns a handle to the 'STR ' resource with the given ID.
            // FUNCTION GetString (stringID: INTEGER): StringHandle;
            // Text 1993, 5-49; Inside Macintosh Volume I, I-468
            // GetString ($A9BA): Returns the loaded `'STR '` resource handle or NIL when missing
            (true, 0x1BA) => {
                let sp = cpu.read_reg(Register::A7);
                let string_id = bus.read_word(sp) as i16;
                let handle = if let Some((refnum, ptr)) =
                    self.find_or_load_resource_any(bus, *b"STR ", string_id)
                {
                    self.get_or_create_resource_handle_in_file(
                        bus, *b"STR ", string_id, ptr, refnum,
                    )
                } else if let Some(ptr) = self.synthesize_system_str(bus, string_id) {
                    self.get_or_create_resource_handle(bus, *b"STR ", string_id, ptr)
                } else {
                    0
                };
                bus.write_long(sp + 2, handle);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // ========== TextEdit Manager Stubs ==========

            // TENew ($A9D2)
            // FUNCTION TENew(destRect, viewRect: Rect): TEHandle;
            // Inside Macintosh Volume I (1985), p. I-373..I-374.
            // Text 1993, 2-85..2-86.
            //
            // IM:I I-374: TENew "creates and initializes the necessary
            // data structures, allocates an edit record, returns a
            // handle to it, and sets that handle's selection range,
            // view rectangle, destination rectangle, and other fields."
            //
            // Fresh TERec state per IM:I I-373:
            //   destRect, viewRect = caller-supplied
            //   selStart = selEnd = 0
            //   teLength = 0
            //   hText = handle to empty char buffer (non-NIL)
            //   txFont, txFace, txMode, txSize copied from current grafPort
            //   inPort = current grafPort
            //
            // Calling-convention duality. Classic Inside Macintosh
            // declares TENew with Pascal by-value Rect parameters
            // (16 bytes on the stack). MPW Universal Headers
            // (TextEdit.h) modernise it to pointer parameters:
            //   EXTERN_API(TEHandle) TENew(const Rect *destRect,
            //                              const Rect *viewRect)
            //                                  ONEWORDINLINE(0xA9D2);
            // BasiliskII System 7.5.3 ROM accepts the pointer-arg
            // convention. Systemless's HLE
            // sniffs which convention the caller used by inspecting
            // whether the first two long words on the stack are valid
            // guest pointers and pops either 8 bytes (pointer convention)
            // or 16 bytes (by-value convention) accordingly.
            //
            // Regression coverage (this file):
            //   tenew_pointer_arg_convention_initializes_destrect_viewrect_and_returns_non_nil_handle
            //   tenew_fresh_terec_has_zero_telength_and_empty_selection_per_im_i_373
            //   tenew_function_protocol_consumes_two_pointer_args_and_writes_4_byte_result
            //
            // TENew ($A9D2): Allocates and initializes a basic monostyled TERec plus empty `hText` handle; supports both pointer-arg and by-value-rect conventions per te_new_rect_args sniffing
            (true, 0x1D2) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = Self::allocate_te_handle(bus);
                let (dest_rect, view_rect, stack_pop) = Self::te_new_rect_args(bus, sp);
                self.initialize_te_record(bus, handle, dest_rect, view_rect);
                self.textedit_states.register(handle);
                bus.write_long(sp + stack_pop, handle);
                cpu.write_reg(Register::A7, sp + stack_pop);
                Ok(())
            }

            // TEStyleNew ($A83E)
            // Creates a multistyled edit record in the current port.
            // FUNCTION TEStyleNew(destRect: Rect; viewRect: Rect): TEHandle;
            // Inside Macintosh: Text (1993), p. 2-78.
            //
            // IM:Text 1993 p. 2-78 verbatim:
            //   "The TEStyleNew function creates a multistyled edit
            //    record and allocates a handle to it... TEStyleNew
            //    sets the txSize, lineHeight, and fontAscent fields
            //    of the edit record to -1, allocates a style record,
            //    and stores a handle to the style record in the
            //    txFont and txFace fields. The TEStyleNew function
            //    creates and initializes a null scrap that is used
            //    by TextEdit routines throughout the life of the
            //    edit record."
            //
            // MPW Universal Headers (TextEdit.h):
            //   EXTERN_API(TEHandle)
            //   TEStyleNew(const Rect *destRect,
            //              const Rect *viewRect)   ONEWORDINLINE(0xA83E);
            //
            // Calling convention: identical to TENew. Pascal pushes
            // left-to-right, so destRect (first arg) lands deepest
            // and viewRect (second arg) lands shallowest:
            //   sp+0..3   viewRect_ptr  (last pushed)
            //   sp+4..7   destRect_ptr  (first pushed)
            // Both pointer (8-byte) and by-value (16-byte) forms are
            // accepted via te_new_rect_args sniffing.
            //
            // Styled-record signature, per IM:Text 1993 p. 2-78
            // (initialize_styled_te_record at dialog.rs:843..):
            //   txSize     = -1   sentinel at offset 0x50
            //   lineHeight = -1   sentinel at offset 0x18
            //   fontAscent = -1   sentinel at offset 0x1A
            //   txFont/txFace (4-byte overlay at offset 0x4A) holds
            //                     the TEStyleHandle.
            //
            // Regression coverage (this file):
            //   testylenew_returns_styled_handle_and_initializes_sentinel_fields
            //   testylenew_pointer_arg_convention_initializes_destrect_viewrect_and_styled_sentinels
            //   testylenew_function_protocol_consumes_two_pointer_args_and_writes_4_byte_result
            //
            // TEStyleNew ($A83E): Allocates a TEHandle and initializes a multistyled record (destRect + viewRect + txSize/lineHeight/fontAscent=-1 sentinels + non-NIL TEStyleHandle); style runs and null scrap allocated per Text 1993, 2-78
            (true, 0x03E) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = Self::allocate_te_handle(bus);
                let (dest_rect, view_rect, stack_pop) = Self::te_new_rect_args(bus, sp);
                self.initialize_styled_te_record(bus, handle, dest_rect, view_rect);
                self.textedit_states.register(handle);
                bus.write_long(sp + stack_pop, handle);
                cpu.write_reg(Register::A7, sp + stack_pop);
                Ok(())
            }

            // TEGetOffset ($A83C)
            // FUNCTION TEGetOffset(pt: Point; hTE: TEHandle): INTEGER;
            // Inside Macintosh Volume V (1986), p. V-172.
            //
            // IM:V V-172 verbatim: "TEGetOffset returns the character
            // position closest to the point pt. The point pt is in
            // local coordinates relative to the destination rectangle.
            // If pt is above the first line, TEGetOffset returns the
            // character offset of the start of the first line. If pt
            // is below the last line, TEGetOffset returns the
            // character offset of the end of the text."
            //
            // MPW Universal Headers (TextEdit.h):
            //   EXTERN_API(short)
            //   TEGetOffset(Point pt, TEHandle hTE) ONEWORDINLINE(0xA83C);
            //
            // Calling convention. `EXTERN_API` expands to `extern
            // pascal` on the 68k target, so the Pascal LR push order
            // applies: pt (the first arg) is pushed FIRST and lands
            // DEEPEST on the stack; hTE (the last arg) is pushed LAST
            // and lands SHALLOWEST. Point is a 4-byte record with
            // pt.v at the lower address and pt.h at the higher
            // address. Pascal FUNCTION pre-allocates the 2-byte
            // INTEGER result slot just above the args. Stack layout
            // at trap entry:
            //   sp+0..3   hTE      (4 bytes, last pushed)
            //   sp+4..5   pt.v     (2 bytes, first half of Point)
            //   sp+6..7   pt.h     (2 bytes, second half of Point)
            //   sp+8..9   function result slot (2 bytes)
            //
            // Pre-fix (commit ca6a0ebf — A9D2 te_new_rect_args
            // Pascal-LR fix only covered TENew + TEStyleNew sharing
            // the te_new_rect_args helper): this arm read te_handle
            // from sp+2 and pt.v/pt.h from sp+6/sp+8, off-by-2 versus
            // the canonical Pascal LR layout. That off-by-2 read placed
            // garbage in te_handle so the te_point_to_char helper bailed
            // via the NIL TERec branch and returned 0 instead of the
            // expected teLength=5. Fixed by reading args at the canonical
            // sp+0, sp+4, sp+6 offsets.
            //
            // Regression coverage (this file):
            //   tegetoffset_point_above_destrect_returns_zero
            //   tegetoffset_point_below_last_line_returns_telength
            //   tegetoffset_function_protocol_consumes_point_and_tehandle_args_writes_integer_result
            //
            // TEGetOffset ($A83C): Maps a point back to a character offset using the line-starts / per-line heights and primary-run advance widths per IM:V V-172. Pascal LR push order — sp+0 hTE (last pushed), sp+4 pt.v, sp+6 pt.h, sp+8 INTEGER result slot.
            (true, 0x03C) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let point_v = bus.read_word(sp + 4) as i16;
                let point_h = bus.read_word(sp + 6) as i16;
                let offset = self.te_point_to_char(bus, te_handle, (point_v, point_h));
                bus.write_word(sp + 8, offset as u16);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // TEFindWord ($A0FE)
            // Register-based TextEdit hook:
            //   currentPos in D0.W, caller in D2.W, pTE in A3.L, hTE in A4.L.
            //   wordStart returns in D0.W and wordEnd in D1.W.
            // Inside Macintosh: Text (1993), pp. 2-60..2-61.
            (false, 0x0FE) => {
                let current_pos = (cpu.read_reg(Register::D0) as u16) as usize;
                let _caller = cpu.read_reg(Register::D2);
                let _p_te = cpu.read_reg(Register::A3);
                let h_te = cpu.read_reg(Register::A4);
                let (word_start, word_end) = self.te_find_word_bounds(bus, h_te, current_pos);
                cpu.write_reg(Register::D0, u32::from(word_start));
                cpu.write_reg(Register::D1, u32::from(word_end));
                Ok(())
            }

            // TEDispatch ($A83D)
            // Dispatches styled TextEdit routines selected by a word on the stack.
            // FUNCTION/PROCEDURE TEDispatch(...); selector is the first stack word.
            // Inside Macintosh Volume VI, 15-42 to 15-43.
            (true, 0x03D) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp);
                let operation = te_dispatch_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                if trace_textedit_enabled() {
                    eprintln!("[TE] TEDispatch selector=${:04X} sp=${:08X}", selector, sp);
                }
                match selector {
                    0x0000 => {
                        // TEStylePaste ($A83D, selector $0000)
                        // PROCEDURE TEStylePaste(hTE: TEHandle);
                        cpu.write_reg(Register::A7, sp + 6);
                    }
                    0x0001 => {
                        // TESetStyle ($A83D, selector $0001)
                        // Sets the current selection's style in a styled edit record.
                        // PROCEDURE TESetStyle(mode: INTEGER; newStyle: TextStyle; redraw: BOOLEAN; hTE: TEHandle);
                        // Inside Macintosh Volume VI, 15-32
                        let te_handle = bus.read_long(sp + 2);
                        // Pascal BOOLEAN in high byte (MPW C convention).
                        let redraw = bus.read_byte(sp + 6) != 0;
                        let style_ptr = bus.read_long(sp + 8);
                        let mode = bus.read_word(sp + 12);
                        if trace_textedit_enabled() {
                            eprintln!(
                                "[TE] TESetStyle hTE=${:08X} mode=${:04X} redraw={} style_ptr=${:08X}",
                                te_handle, mode, redraw, style_ptr
                            );
                            if style_ptr != 0 {
                                let te_ptr = Self::te_record_ptr(bus, te_handle);
                                eprintln!(
                                    "[TE] TESetStyle values sel={}..{} font={} face=${:04X} size={} color=(${:04X},${:04X},${:04X})",
                                    if te_ptr != 0 { bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET) } else { 0 },
                                    if te_ptr != 0 { bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) } else { 0 },
                                    bus.read_word(style_ptr) as i16,
                                    bus.read_byte(style_ptr + 2),
                                    bus.read_word(style_ptr + 4) as i16,
                                    bus.read_word(style_ptr + 6),
                                    bus.read_word(style_ptr + 8),
                                    bus.read_word(style_ptr + 10),
                                );
                            }
                        }
                        if style_ptr != 0 {
                            let te_ptr = Self::te_record_ptr(bus, te_handle);
                            if te_ptr != 0 {
                                let insertion_point = bus
                                    .read_word(te_ptr + Self::TE_SEL_START_OFFSET)
                                    == bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET);
                                if insertion_point && Self::te_is_styled_record(bus, te_ptr) {
                                    Self::te_set_null_style(bus, te_handle, mode, style_ptr);
                                } else if Self::te_is_styled_record(bus, te_ptr) {
                                    let mut selection_start =
                                        bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET) as usize;
                                    let mut selection_end =
                                        bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) as usize;
                                    if selection_end < selection_start {
                                        std::mem::swap(&mut selection_start, &mut selection_end);
                                    }
                                    if self.te_set_style_for_range(
                                        bus,
                                        te_handle,
                                        selection_start,
                                        selection_end,
                                        mode,
                                        style_ptr,
                                    ) && redraw
                                    {
                                        self.te_recalculate_layout(bus, te_handle);
                                        self.draw_te_contents(cpu, bus, te_handle, true);
                                    }
                                } else {
                                    let style_handle = Self::te_style_handle(bus, te_handle);
                                    if style_handle != 0 {
                                        let style_ptr_record = bus.read_long(style_handle);
                                        if style_ptr_record != 0 {
                                            let table_handle = bus.read_long(
                                                style_ptr_record
                                                    + Self::TE_STYLE_STYLE_TABLE_OFFSET,
                                            );
                                            let table_ptr = if table_handle != 0 {
                                                bus.read_long(table_handle)
                                            } else {
                                                0
                                            };
                                            if table_ptr != 0 {
                                                if (mode & 0x0001) != 0 {
                                                    bus.write_word(
                                                        table_ptr + Self::ST_ELEMENT_FONT_OFFSET,
                                                        bus.read_word(style_ptr),
                                                    );
                                                }
                                                if (mode & 0x0002) != 0 {
                                                    bus.write_byte(
                                                        table_ptr + Self::ST_ELEMENT_FACE_OFFSET,
                                                        bus.read_byte(style_ptr + 2),
                                                    );
                                                }
                                                if (mode & 0x0004) != 0 {
                                                    bus.write_word(
                                                        table_ptr + Self::ST_ELEMENT_SIZE_OFFSET,
                                                        bus.read_word(style_ptr + 4),
                                                    );
                                                }
                                                if (mode & 0x0008) != 0 {
                                                    bus.write_word(
                                                        table_ptr + Self::ST_ELEMENT_COLOR_OFFSET,
                                                        bus.read_word(style_ptr + 6),
                                                    );
                                                    bus.write_word(
                                                        table_ptr
                                                            + Self::ST_ELEMENT_COLOR_OFFSET
                                                            + 2,
                                                        bus.read_word(style_ptr + 8),
                                                    );
                                                    bus.write_word(
                                                        table_ptr
                                                            + Self::ST_ELEMENT_COLOR_OFFSET
                                                            + 4,
                                                        bus.read_word(style_ptr + 10),
                                                    );
                                                }

                                                let resolved_font = bus.read_word(
                                                    table_ptr + Self::ST_ELEMENT_FONT_OFFSET,
                                                )
                                                    as i16;
                                                let resolved_size = bus.read_word(
                                                    table_ptr + Self::ST_ELEMENT_SIZE_OFFSET,
                                                )
                                                    as i16;
                                                let metrics = get_font_metrics(
                                                    resolved_font,
                                                    Self::font_lookup_size(resolved_size),
                                                );
                                                let line_height = metrics.ascent
                                                    + metrics.descent
                                                    + metrics.leading;
                                                bus.write_word(
                                                    table_ptr + Self::ST_ELEMENT_HEIGHT_OFFSET,
                                                    line_height as u16,
                                                );
                                                bus.write_word(
                                                    table_ptr + Self::ST_ELEMENT_ASCENT_OFFSET,
                                                    metrics.ascent as u16,
                                                );

                                                let lh_handle = bus.read_long(
                                                    style_ptr_record
                                                        + Self::TE_STYLE_LH_TABLE_OFFSET,
                                                );
                                                let lh_ptr = if lh_handle != 0 {
                                                    bus.read_long(lh_handle)
                                                } else {
                                                    0
                                                };
                                                if lh_ptr != 0 {
                                                    bus.write_word(
                                                        lh_ptr + Self::LH_ELEMENT_HEIGHT_OFFSET,
                                                        line_height as u16,
                                                    );
                                                    bus.write_word(
                                                        lh_ptr + Self::LH_ELEMENT_ASCENT_OFFSET,
                                                        metrics.ascent as u16,
                                                    );
                                                }
                                            }
                                        }
                                    } else {
                                        if (mode & 0x0001) != 0 {
                                            bus.write_word(
                                                te_ptr + Self::TE_TX_FONT_OFFSET,
                                                bus.read_word(style_ptr),
                                            );
                                        }
                                        if (mode & 0x0002) != 0 {
                                            bus.write_byte(
                                                te_ptr + Self::TE_TX_FACE_OFFSET,
                                                bus.read_byte(style_ptr + 2),
                                            );
                                        }
                                        if (mode & 0x0004) != 0 {
                                            bus.write_word(
                                                te_ptr + Self::TE_TX_SIZE_OFFSET,
                                                bus.read_word(style_ptr + 4),
                                            );
                                        }

                                        let resolved_font =
                                            bus.read_word(te_ptr + Self::TE_TX_FONT_OFFSET) as i16;
                                        let resolved_size =
                                            bus.read_word(te_ptr + Self::TE_TX_SIZE_OFFSET) as i16;
                                        let metrics = get_font_metrics(
                                            resolved_font,
                                            Self::font_lookup_size(resolved_size),
                                        );
                                        bus.write_word(
                                            te_ptr + Self::TE_LINE_HEIGHT_OFFSET,
                                            (metrics.ascent + metrics.descent + metrics.leading)
                                                as u16,
                                        );
                                        bus.write_word(
                                            te_ptr + Self::TE_FONT_ASCENT_OFFSET,
                                            metrics.ascent as u16,
                                        );
                                    }
                                }
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 14);
                    }
                    0x0002 => {
                        // TEReplaceStyle ($A83D, selector $0002)
                        // PROCEDURE TEReplaceStyle(mode: INTEGER;
                        //     oldStyle, newStyle: TextStyle;
                        //     redraw: BOOLEAN; hTE: TEHandle);
                        // Inside Macintosh Volume V, V-271..V-272.
                        // MPW C glue passes both TextStyle records by
                        // pointer (not by value), giving an 18-byte arg
                        // frame: selector(2) + hTE(4) + redraw(2) +
                        // newStyle ptr(4) + oldStyle ptr(4) + mode(2).
                        let te_handle = bus.read_long(sp + 2);
                        let _redraw = bus.read_byte(sp + 6) != 0;
                        let new_style_ptr = bus.read_long(sp + 8);
                        let old_style_ptr = bus.read_long(sp + 12);
                        let mode = bus.read_word(sp + 16);
                        if old_style_ptr != 0 && new_style_ptr != 0 {
                            let style_handle = Self::te_style_handle(bus, te_handle);
                            if style_handle != 0 {
                                let style_ptr_record = bus.read_long(style_handle);
                                if style_ptr_record != 0 {
                                    let table_handle = bus.read_long(
                                        style_ptr_record + Self::TE_STYLE_STYLE_TABLE_OFFSET,
                                    );
                                    let table_ptr = if table_handle != 0 {
                                        bus.read_long(table_handle)
                                    } else {
                                        0
                                    };
                                    if table_ptr != 0 {
                                        // Per IM:V V-270, replace only when the
                                        // existing style's selected attributes
                                        // match oldStyle exactly. With Systemless's
                                        // single-element style table this collapses
                                        // to one comparison.
                                        let mut matches_old = true;
                                        if (mode & 0x0001) != 0
                                            && bus
                                                .read_word(table_ptr + Self::ST_ELEMENT_FONT_OFFSET)
                                                != bus.read_word(old_style_ptr)
                                        {
                                            matches_old = false;
                                        }
                                        if (mode & 0x0002) != 0
                                            && bus
                                                .read_byte(table_ptr + Self::ST_ELEMENT_FACE_OFFSET)
                                                != bus.read_byte(old_style_ptr + 2)
                                        {
                                            matches_old = false;
                                        }
                                        if (mode & 0x0004) != 0
                                            && bus
                                                .read_word(table_ptr + Self::ST_ELEMENT_SIZE_OFFSET)
                                                != bus.read_word(old_style_ptr + 4)
                                        {
                                            matches_old = false;
                                        }
                                        if (mode & 0x0008) != 0
                                            && (bus.read_word(
                                                table_ptr + Self::ST_ELEMENT_COLOR_OFFSET,
                                            ) != bus.read_word(old_style_ptr + 6)
                                                || bus.read_word(
                                                    table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 2,
                                                ) != bus.read_word(old_style_ptr + 8)
                                                || bus.read_word(
                                                    table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 4,
                                                ) != bus.read_word(old_style_ptr + 10))
                                        {
                                            matches_old = false;
                                        }
                                        if matches_old {
                                            if (mode & 0x0001) != 0 {
                                                bus.write_word(
                                                    table_ptr + Self::ST_ELEMENT_FONT_OFFSET,
                                                    bus.read_word(new_style_ptr),
                                                );
                                            }
                                            if (mode & 0x0002) != 0 {
                                                bus.write_byte(
                                                    table_ptr + Self::ST_ELEMENT_FACE_OFFSET,
                                                    bus.read_byte(new_style_ptr + 2),
                                                );
                                            }
                                            if (mode & 0x0004) != 0 {
                                                bus.write_word(
                                                    table_ptr + Self::ST_ELEMENT_SIZE_OFFSET,
                                                    bus.read_word(new_style_ptr + 4),
                                                );
                                            }
                                            if (mode & 0x0008) != 0 {
                                                bus.write_word(
                                                    table_ptr + Self::ST_ELEMENT_COLOR_OFFSET,
                                                    bus.read_word(new_style_ptr + 6),
                                                );
                                                bus.write_word(
                                                    table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 2,
                                                    bus.read_word(new_style_ptr + 8),
                                                );
                                                bus.write_word(
                                                    table_ptr + Self::ST_ELEMENT_COLOR_OFFSET + 4,
                                                    bus.read_word(new_style_ptr + 10),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let _ = mode;
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    0x0003 => {
                        // TEGetStyle ($A83D, selector $0003)
                        // PROCEDURE TEGetStyle(sel: INTEGER; VAR attrs: TextStyle;
                        //     VAR lineHeight: INTEGER; VAR fontAscent: INTEGER; hTE: TEHandle);
                        let te_handle = bus.read_long(sp + 2);
                        let font_ascent_ptr = bus.read_long(sp + 6);
                        let line_height_ptr = bus.read_long(sp + 10);
                        let attrs_ptr = bus.read_long(sp + 14);
                        let _sel = bus.read_word(sp + 18) as i16;
                        let (font, face, size, color, line_height, font_ascent) =
                            self.te_primary_style(bus, te_handle);
                        if attrs_ptr != 0 {
                            bus.write_word(attrs_ptr, font as u16);
                            bus.write_word(attrs_ptr + 2, face as u16);
                            bus.write_word(attrs_ptr + 4, size as u16);
                            bus.write_word(attrs_ptr + 6, color.0);
                            bus.write_word(attrs_ptr + 8, color.1);
                            bus.write_word(attrs_ptr + 10, color.2);
                        }
                        if line_height_ptr != 0 {
                            bus.write_word(line_height_ptr, line_height as u16);
                        }
                        if font_ascent_ptr != 0 {
                            bus.write_word(font_ascent_ptr, font_ascent as u16);
                        }
                        cpu.write_reg(Register::A7, sp + 20);
                    }
                    0x0004 => {
                        // TEGetStyleHandle ($A83D, selector $0004)
                        // FUNCTION TEGetStyleHandle(hTE: TEHandle): TEStyleHandle;
                        let te_handle = bus.read_long(sp + 2);
                        bus.write_long(sp + 6, Self::te_style_handle(bus, te_handle));
                        cpu.write_reg(Register::A7, sp + 6);
                    }
                    0x0005 => {
                        // TESetStyleHandle ($A83D, selector $0005)
                        // PROCEDURE TESetStyleHandle(theHandle: TEStyleHandle; hTE: TEHandle);
                        let te_handle = bus.read_long(sp + 2);
                        let style_handle = bus.read_long(sp + 6);
                        Self::te_write_style_handle(bus, te_handle, style_handle);
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    0x0006 => {
                        // TEGetStyleScrapHandle ($A83D, selector $0006)
                        // FUNCTION TEGetStyleScrapHandle(hTE: TEHandle): STScrpHandle;
                        let te_handle = bus.read_long(sp + 2);
                        let style_handle = Self::te_style_handle(bus, te_handle);
                        let mut result = 0;
                        if style_handle != 0 {
                            let style_ptr = bus.read_long(style_handle);
                            if style_ptr != 0 {
                                let null_style_handle =
                                    bus.read_long(style_ptr + Self::TE_STYLE_NULL_STYLE_OFFSET);
                                if null_style_handle != 0 {
                                    let null_style_ptr = bus.read_long(null_style_handle);
                                    if null_style_ptr != 0 {
                                        let scrap_handle = bus.read_long(
                                            null_style_ptr + Self::NULL_STYLE_SCRAP_OFFSET,
                                        );
                                        result = Self::duplicate_handle_data(bus, scrap_handle);
                                    }
                                }
                            }
                        }
                        bus.write_long(sp + 6, result);
                        cpu.write_reg(Register::A7, sp + 6);
                    }
                    0x0007 => {
                        // TEStyleInsert ($A83D, selector $0007)
                        // Inserts styled text before the selection and redraws it as necessary.
                        // PROCEDURE TEStyleInsert(text: Ptr; length: LONGINT; hST: StScrpHandle; hTE: TEHandle);
                        // Inside Macintosh: Text (1993), pp. 2-102 to 2-103.
                        let te_handle = bus.read_long(sp + 2);
                        let style_scrap = bus.read_long(sp + 6);
                        let length = bus.read_long(sp + 10) as usize;
                        let text_ptr = bus.read_long(sp + 14);
                        if text_ptr != 0 && length != 0 {
                            let text = bus.read_bytes(text_ptr, length);
                            let insert_start = {
                                let te_ptr = Self::te_record_ptr(bus, te_handle);
                                let text_len = Self::te_text_length(bus, te_handle);
                                if te_ptr != 0 {
                                    let mut sel_start =
                                        bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET) as usize;
                                    let mut sel_end =
                                        bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) as usize;
                                    sel_start = sel_start.min(text_len);
                                    sel_end = sel_end.min(text_len);
                                    if sel_end < sel_start {
                                        std::mem::swap(&mut sel_start, &mut sel_end);
                                    }
                                    sel_start
                                } else {
                                    0
                                }
                            };
                            let preview_len = text.len().min(64);
                            if trace_textedit_enabled() {
                                eprintln!(
                                    "[TE] TEStyleInsert hTE=${:08X} hST=${:08X} len={} text_ptr=${:08X} preview={:?}",
                                    te_handle,
                                    style_scrap,
                                    length,
                                    text_ptr,
                                    String::from_utf8_lossy(&text[..preview_len])
                                );
                            }
                            self.te_insert_text(bus, te_handle, &text);
                            // With NIL hST, TEStyleInsert follows TEInsert;
                            // insertion-point attributes previously stored by
                            // TESetStyle come from the null scrap. Inside
                            // Macintosh Volume V, V-274; Inside Macintosh:
                            // Text 1993, pp. 2-61 and 2-82.
                            let effective_style_scrap = if style_scrap != 0 {
                                style_scrap
                            } else {
                                Self::te_null_style_scrap_handle(bus, te_handle)
                            };
                            if self.te_apply_style_scrap_to_range(
                                bus,
                                te_handle,
                                effective_style_scrap,
                                insert_start,
                                text.len(),
                            ) {
                                self.te_recalculate_layout(bus, te_handle);
                            }
                            self.draw_te_contents(cpu, bus, te_handle, true);
                        }
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    0x0008 => {
                        // TEGetPoint ($A83D, selector $0008)
                        // Returns the point for a character offset within the edit record.
                        // FUNCTION TEGetPoint(offset: INTEGER; hTE: TEHandle): Point;
                        // Inside Macintosh Volume VI, 15-31
                        let te_handle = bus.read_long(sp + 2);
                        let offset = bus.read_word(sp + 6) as i16;
                        let te_ptr = Self::te_record_ptr(bus, te_handle);
                        let result_addr = sp + 8;
                        if te_ptr != 0 {
                            let text_len = Self::te_text_length(bus, te_handle);
                            let clamped = i32::from(offset).clamp(0, text_len as i32) as usize;
                            let line_index = Self::te_char_to_line_index(bus, te_handle, clamped);
                            let line_start = Self::te_line_starts(bus, te_handle)
                                .get(line_index)
                                .copied()
                                .unwrap_or(0);
                            let (top, x) = self.te_char_to_point(bus, te_handle, clamped);
                            let y = top.saturating_add(Self::te_ascent_for_line(
                                bus, te_handle, line_index,
                            ));
                            if trace_textedit_enabled() {
                                let starts = Self::te_line_starts(bus, te_handle);
                                let pc = cpu.read_reg(Register::PC);
                                eprintln!(
                                    "[TE] TEGetPoint hTE=${:08X} offset={} line={} start={} point_offset={} point=({}, {}) pc=${:08X} next=[{:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X}]",
                                    te_handle,
                                    clamped,
                                    line_index,
                                    line_start,
                                    clamped,
                                    y,
                                    x,
                                    pc,
                                    bus.read_word(pc),
                                    bus.read_word(pc + 2),
                                    bus.read_word(pc + 4),
                                    bus.read_word(pc + 6),
                                    bus.read_word(pc + 8),
                                    bus.read_word(pc + 10),
                                    bus.read_word(pc + 12),
                                    bus.read_word(pc + 14),
                                    bus.read_word(pc + 16),
                                    bus.read_word(pc + 18),
                                    bus.read_word(pc + 20),
                                    bus.read_word(pc + 22),
                                    bus.read_word(pc + 24),
                                    bus.read_word(pc + 26),
                                    bus.read_word(pc + 28),
                                    bus.read_word(pc + 30),
                                    bus.read_word(pc + 32),
                                    bus.read_word(pc + 34),
                                    bus.read_word(pc + 36),
                                    bus.read_word(pc + 38)
                                );
                                eprintln!(
                                    "[TE] TEGetPoint layout nLines={} teLength={} lineStarts={:?}",
                                    starts.len().saturating_sub(1),
                                    text_len,
                                    starts
                                );
                                eprintln!(
                                    "[TE] TEGetPoint helper@35614=[{:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X}]",
                                    bus.read_word(0x0003_5614),
                                    bus.read_word(0x0003_5616),
                                    bus.read_word(0x0003_5618),
                                    bus.read_word(0x0003_561A),
                                    bus.read_word(0x0003_561C),
                                    bus.read_word(0x0003_561E),
                                    bus.read_word(0x0003_5620),
                                    bus.read_word(0x0003_5622),
                                    bus.read_word(0x0003_5624),
                                    bus.read_word(0x0003_5626),
                                    bus.read_word(0x0003_5628),
                                    bus.read_word(0x0003_562A)
                                );
                                let table_base = cpu.read_reg(Register::A5).wrapping_sub(0x37B4);
                                let rect_table = bus.read_long(table_base);
                                let rect_ptr = rect_table.wrapping_add(21 * 8);
                                let a6 = cpu.read_reg(Register::A6);
                                let ret = bus.read_long(a6 + 4);
                                eprintln!(
                                    "[TE] TEGetPoint rect21 table=${:08X} rect_ptr=${:08X} rect=({},{},{},{}) a4=${:08X} d5={} d6={} d7={} a6=${:08X} ret=${:08X}",
                                    rect_table,
                                    rect_ptr,
                                    bus.read_word(rect_ptr) as i16,
                                    bus.read_word(rect_ptr + 2) as i16,
                                    bus.read_word(rect_ptr + 4) as i16,
                                    bus.read_word(rect_ptr + 6) as i16,
                                    cpu.read_reg(Register::A4),
                                    cpu.read_reg(Register::D5) as i32,
                                    cpu.read_reg(Register::D6) as i32,
                                    cpu.read_reg(Register::D7) as i32,
                                    a6,
                                    ret
                                );
                                eprintln!(
                                    "[TE] TEGetPoint caller@{:08X}=[{:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X}]",
                                    ret,
                                    bus.read_word(ret),
                                    bus.read_word(ret + 2),
                                    bus.read_word(ret + 4),
                                    bus.read_word(ret + 6),
                                    bus.read_word(ret + 8),
                                    bus.read_word(ret + 10),
                                    bus.read_word(ret + 12),
                                    bus.read_word(ret + 14),
                                    bus.read_word(ret + 16),
                                    bus.read_word(ret + 18),
                                    bus.read_word(ret + 20),
                                    bus.read_word(ret + 22)
                                );
                                let caller_start = ret.wrapping_sub(0x20);
                                eprintln!(
                                    "[TE] TEGetPoint caller_pre@{:08X}=[{:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X}]",
                                    caller_start,
                                    bus.read_word(caller_start),
                                    bus.read_word(caller_start + 2),
                                    bus.read_word(caller_start + 4),
                                    bus.read_word(caller_start + 6),
                                    bus.read_word(caller_start + 8),
                                    bus.read_word(caller_start + 10),
                                    bus.read_word(caller_start + 12),
                                    bus.read_word(caller_start + 14),
                                    bus.read_word(caller_start + 16),
                                    bus.read_word(caller_start + 18),
                                    bus.read_word(caller_start + 20),
                                    bus.read_word(caller_start + 22),
                                    bus.read_word(caller_start + 24),
                                    bus.read_word(caller_start + 26),
                                    bus.read_word(caller_start + 28),
                                    bus.read_word(caller_start + 30)
                                );
                                eprintln!(
                                    "[TE] TEGetPoint caller_block@00035F7C=[{:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X}]",
                                    bus.read_word(0x0003_5F7C),
                                    bus.read_word(0x0003_5F7E),
                                    bus.read_word(0x0003_5F80),
                                    bus.read_word(0x0003_5F82),
                                    bus.read_word(0x0003_5F84),
                                    bus.read_word(0x0003_5F86),
                                    bus.read_word(0x0003_5F88),
                                    bus.read_word(0x0003_5F8A),
                                    bus.read_word(0x0003_5F8C),
                                    bus.read_word(0x0003_5F8E),
                                    bus.read_word(0x0003_5F90),
                                    bus.read_word(0x0003_5F92),
                                    bus.read_word(0x0003_5F94),
                                    bus.read_word(0x0003_5F96),
                                    bus.read_word(0x0003_5F98),
                                    bus.read_word(0x0003_5F9A),
                                    bus.read_word(0x0003_5F9C),
                                    bus.read_word(0x0003_5F9E),
                                    bus.read_word(0x0003_5FA0),
                                    bus.read_word(0x0003_5FA2),
                                    bus.read_word(0x0003_5FA4),
                                    bus.read_word(0x0003_5FA6),
                                    bus.read_word(0x0003_5FA8),
                                    bus.read_word(0x0003_5FAA),
                                    bus.read_word(0x0003_5FAC),
                                    bus.read_word(0x0003_5FAE),
                                    bus.read_word(0x0003_5FB0),
                                    bus.read_word(0x0003_5FB2),
                                    bus.read_word(0x0003_5FB4),
                                    bus.read_word(0x0003_5FB6),
                                    bus.read_word(0x0003_5FB8),
                                    bus.read_word(0x0003_5FBA)
                                );
                                eprintln!(
                                    "[TE] TEGetPoint branch_c6@00035FC6=[{:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X}] branch_e2@00035FE2=[{:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X}]",
                                    bus.read_word(0x0003_5FC6),
                                    bus.read_word(0x0003_5FC8),
                                    bus.read_word(0x0003_5FCA),
                                    bus.read_word(0x0003_5FCC),
                                    bus.read_word(0x0003_5FCE),
                                    bus.read_word(0x0003_5FD0),
                                    bus.read_word(0x0003_5FD2),
                                    bus.read_word(0x0003_5FD4),
                                    bus.read_word(0x0003_5FE2),
                                    bus.read_word(0x0003_5FE4),
                                    bus.read_word(0x0003_5FE6),
                                    bus.read_word(0x0003_5FE8),
                                    bus.read_word(0x0003_5FEA),
                                    bus.read_word(0x0003_5FEC),
                                    bus.read_word(0x0003_5FEE),
                                    bus.read_word(0x0003_5FF0)
                                );
                                eprintln!(
                                    "[TE] TEGetPoint helper_fn@00036B9A=[{:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X} {:04X}]",
                                    bus.read_word(0x0003_6B9A),
                                    bus.read_word(0x0003_6B9C),
                                    bus.read_word(0x0003_6B9E),
                                    bus.read_word(0x0003_6BA0),
                                    bus.read_word(0x0003_6BA2),
                                    bus.read_word(0x0003_6BA4),
                                    bus.read_word(0x0003_6BA6),
                                    bus.read_word(0x0003_6BA8),
                                    bus.read_word(0x0003_6BAA),
                                    bus.read_word(0x0003_6BAC),
                                    bus.read_word(0x0003_6BAE),
                                    bus.read_word(0x0003_6BB0),
                                    bus.read_word(0x0003_6BB2),
                                    bus.read_word(0x0003_6BB4),
                                    bus.read_word(0x0003_6BB6),
                                    bus.read_word(0x0003_6BB8),
                                    bus.read_word(0x0003_6BBA),
                                    bus.read_word(0x0003_6BBC),
                                    bus.read_word(0x0003_6BBE),
                                    bus.read_word(0x0003_6BC0),
                                    bus.read_word(0x0003_6BC2),
                                    bus.read_word(0x0003_6BC4),
                                    bus.read_word(0x0003_6BC6),
                                    bus.read_word(0x0003_6BC8)
                                );
                            }
                            bus.write_word(result_addr, y as u16);
                            bus.write_word(result_addr + 2, x as u16);
                        } else {
                            bus.write_word(result_addr, 0);
                            bus.write_word(result_addr + 2, 0);
                        }
                        cpu.write_reg(Register::A7, result_addr);
                    }
                    0x0009 => {
                        // TEGetHeight ($A83D, selector $0009)
                        // Returns the total height of the requested line range.
                        // FUNCTION TEGetHeight(endLine: LONGINT; startLine: LONGINT; hTE: TEHandle): LONGINT;
                        // Text 1993, 2-90
                        let te_handle = bus.read_long(sp + 2);
                        let mut start_line = bus.read_long(sp + 6) as i32;
                        let mut end_line = bus.read_long(sp + 10) as i32;
                        let te_ptr = Self::te_record_ptr(bus, te_handle);
                        let n_lines = if te_ptr != 0 {
                            bus.read_word(te_ptr + Self::TE_N_LINES_OFFSET) as i32
                        } else {
                            0
                        };
                        if start_line > 0 {
                            start_line -= 1;
                        } else {
                            start_line = 0;
                        }
                        end_line = end_line.min(n_lines);
                        if end_line < 0 {
                            end_line = 0;
                        } else if end_line > 0 {
                            end_line -= 1;
                        }
                        if start_line > end_line {
                            std::mem::swap(&mut start_line, &mut end_line);
                        }

                        let text_bytes = Self::te_text_bytes(bus, te_handle);
                        let line_starts = Self::te_line_starts(bus, te_handle);
                        if !text_bytes.is_empty() {
                            while end_line >= start_line {
                                let current = end_line as usize;
                                let Some(&line_start) = line_starts.get(current) else {
                                    break;
                                };
                                let line_end = line_starts
                                    .get(current + 1)
                                    .copied()
                                    .unwrap_or(text_bytes.len());
                                let blank_trailing_line = current + 1 == line_starts.len() - 1
                                    && line_start < line_end
                                    && text_bytes[line_start..line_end]
                                        .iter()
                                        .all(|&b| matches!(b, b'\r' | b'\n'));
                                if blank_trailing_line {
                                    end_line -= 1;
                                } else {
                                    break;
                                }
                            }
                        }

                        let mut total_height = 0i32;
                        if end_line >= start_line {
                            for current_line in
                                start_line.max(0) as usize..=end_line.max(0) as usize
                            {
                                total_height += i32::from(Self::te_height_for_line(
                                    bus,
                                    te_handle,
                                    current_line,
                                ));
                            }
                        }
                        if trace_textedit_enabled() {
                            eprintln!(
                                "[TE] TEGetHeight hTE=${:08X} start_line={} end_line={} result={}",
                                te_handle, start_line, end_line, total_height
                            );
                        }
                        bus.write_long(sp + 14, total_height as u32);
                        cpu.write_reg(Register::A7, sp + 14);
                    }
                    0x000A => {
                        // TEContinuousStyle ($A83D, selector $000A)
                        // Returns the common style across the current selection.
                        // FUNCTION TEContinuousStyle(VAR mode: INTEGER; VAR aStyle: TextStyle; hTE: TEHandle): BOOLEAN;
                        // Inside Macintosh Volume VI, 15-34 to 15-35
                        let te_handle = bus.read_long(sp + 2);
                        let style_ptr = bus.read_long(sp + 6);
                        let mode_ptr = bus.read_long(sp + 10);
                        let result_addr = sp + 14;
                        let requested_mode = if mode_ptr != 0 {
                            bus.read_word(mode_ptr)
                        } else {
                            0
                        };
                        let text_len = Self::te_text_length(bus, te_handle);
                        let te_ptr = Self::te_record_ptr(bus, te_handle);
                        let mut selection_start = if te_ptr != 0 {
                            bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET) as usize
                        } else {
                            0
                        };
                        let mut selection_end = if te_ptr != 0 {
                            bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) as usize
                        } else {
                            0
                        };
                        if selection_end < selection_start {
                            std::mem::swap(&mut selection_start, &mut selection_end);
                        }
                        selection_start = selection_start.min(text_len);
                        selection_end = selection_end.min(text_len);
                        let runs = self.te_style_runs(bus, te_handle, text_len);
                        let first = if selection_start == selection_end {
                            Self::te_null_style_resolved_style(bus, te_handle).unwrap_or_else(|| {
                                Self::te_style_at_offset(
                                    &runs,
                                    selection_start
                                        .saturating_sub(1)
                                        .min(text_len.saturating_sub(1)),
                                )
                            })
                        } else {
                            Self::te_style_at_offset(&runs, selection_start)
                        };
                        let mut common_mode = requested_mode;
                        let mut common_face = first.face;
                        let mut all_faces_equal = true;
                        for offset in selection_start..selection_end {
                            let style = Self::te_style_at_offset(&runs, offset);
                            if requested_mode & 0x0001 != 0 && style.font != first.font {
                                common_mode &= !0x0001;
                            }
                            if requested_mode & 0x0002 != 0 {
                                if style.face != first.face {
                                    all_faces_equal = false;
                                }
                                common_face &= style.face;
                            }
                            if requested_mode & 0x0004 != 0 && style.size != first.size {
                                common_mode &= !0x0004;
                            }
                            if requested_mode & 0x0008 != 0 && style.color != first.color {
                                common_mode &= !0x0008;
                            }
                        }
                        if requested_mode & 0x0002 != 0 && !all_faces_equal && common_face == 0 {
                            common_mode &= !0x0002;
                        }
                        if mode_ptr != 0 {
                            bus.write_word(mode_ptr, common_mode);
                        }
                        if style_ptr != 0 {
                            if requested_mode & 0x0001 != 0 {
                                bus.write_word(style_ptr, first.font as u16);
                            }
                            if requested_mode & 0x0002 != 0 {
                                bus.write_byte(style_ptr + 2, common_face as u8);
                            }
                            if requested_mode & 0x0004 != 0 {
                                bus.write_word(style_ptr + 4, first.size as u16);
                            }
                            if requested_mode & 0x0008 != 0 {
                                bus.write_word(style_ptr + 6, first.color.0);
                                bus.write_word(style_ptr + 8, first.color.1);
                                bus.write_word(style_ptr + 10, first.color.2);
                            }
                        }
                        bus.write_word(
                            result_addr,
                            if common_mode == requested_mode {
                                0xFFFF
                            } else {
                                0
                            },
                        );
                        cpu.write_reg(Register::A7, result_addr);
                    }
                    0x000B => {
                        // TEUseStyleScrap ($A83D, selector $000B)
                        // Sets style data for the specified text range from a style scrap handle.
                        // PROCEDURE TEUseStyleScrap(rangeStart: LONGINT; rangeEnd: LONGINT;
                        //     newStyles: StScrpHandle; redraw: BOOLEAN; hTE: TEHandle);
                        // Inside Macintosh Volume VI, 15-35 to 15-36
                        let te_handle = bus.read_long(sp + 2);
                        // Pascal BOOLEAN in high byte (MPW C convention).
                        let redraw = bus.read_byte(sp + 6) != 0;
                        let new_styles = bus.read_long(sp + 8);
                        let range_end = bus.read_long(sp + 12) as usize;
                        let range_start = bus.read_long(sp + 16) as usize;
                        let (range_start, range_end) = if range_end < range_start {
                            (range_end, range_start)
                        } else {
                            (range_start, range_end)
                        };
                        if self.te_apply_style_scrap_to_range(
                            bus,
                            te_handle,
                            new_styles,
                            range_start,
                            range_end.saturating_sub(range_start),
                        ) {
                            self.te_recalculate_layout(bus, te_handle);
                            if redraw {
                                self.draw_te_contents(cpu, bus, te_handle, true);
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 20);
                    }
                    0x000C => {
                        // TECustomHook ($A83D, selector $000C)
                        // Reads or replaces one of TextEdit's internal hook procedures.
                        // PROCEDURE TECustomHook(which: TEIntHook; VAR addr: ProcPtr; hTE: TEHandle);
                        // Inside Macintosh Volume VI, 15-25 to 15-26.
                        // Per IM:VI 15-26, addr is a VAR ProcPtr — on
                        // return it must hold the previous hook. Systemless
                        // does not invoke registered hooks (no guest-fn
                        // dispatch infrastructure), so we report "no
                        // previous hook" by writing 0 into *addr.
                        let addr_ptr = bus.read_long(sp + 6);
                        if addr_ptr != 0 {
                            bus.write_long(addr_ptr, 0);
                        }
                        cpu.write_reg(Register::A7, sp + 12);
                    }
                    0x000D => {
                        // TENumStyles ($A83D, selector $000D)
                        // Returns the number of style changes contained in the
                        // given range, counting one for the start of the range.
                        // FUNCTION TENumStyles(rangeStart: LONGINT; rangeEnd: LONGINT;
                        //                      hTE: TEHandle): LONGINT;
                        // Inside Macintosh Volume VI, 15-36.
                        // Per IM:VI 15-36, an unstyled record always
                        // returns 1. With Systemless's single-run styled
                        // record the in-range transitions count is also
                        // zero, so the +1 for the start-of-range gives 1.
                        let te_handle = bus.read_long(sp + 2);
                        let style_handle = Self::te_style_handle(bus, te_handle);
                        let mut count: u32 = 1;
                        if style_handle != 0 {
                            let style_ptr_record = bus.read_long(style_handle);
                            if style_ptr_record != 0 {
                                let n_runs = bus
                                    .read_word(style_ptr_record + Self::TE_STYLE_N_RUNS_OFFSET)
                                    as u32;
                                count = n_runs.max(1);
                            }
                        }
                        bus.write_long(sp + 14, count);
                        cpu.write_reg(Register::A7, sp + 14);
                    }
                    0x000E => {
                        // TEFeatureFlag ($A83D, selector $000E)
                        // Tests or changes a TextEdit feature bit and returns the previous state.
                        // FUNCTION TEFeatureFlag(feature: INTEGER; action: INTEGER; hTE: TEHandle): INTEGER;
                        // Inside Macintosh Volume VI, 15-22 to 15-24
                        let te_handle = bus.read_long(sp + 2);
                        let action = bus.read_word(sp + 6) as i16;
                        let feature = bus.read_word(sp + 8);
                        let was_set = self.te_feature_bit(te_handle, feature);
                        if matches!(
                            feature,
                            Self::TE_FEATURE_AUTO_SCROLL
                                | Self::TE_FEATURE_TEXT_BUFFERING
                                | Self::TE_FEATURE_OUTLINE_HILITE
                                | Self::TE_FEATURE_INLINE_INPUT
                                | Self::TE_FEATURE_USE_TEXT_SERVICES
                        ) {
                            match action {
                                Self::TE_BIT_CLEAR => {
                                    self.set_te_feature_bit(te_handle, feature, false)
                                }
                                Self::TE_BIT_SET => {
                                    self.set_te_feature_bit(te_handle, feature, true)
                                }
                                Self::TE_BIT_TEST => {}
                                _ => {}
                            }
                        }
                        bus.write_word(sp + 10, if was_set { 1 } else { 0 });
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    _ => return None,
                }
                Ok(())
            }

            // TEDispose ($A9CD)
            // Disposes of the edit record and releases memory used by
            // the text and record structures.
            // PROCEDURE TEDispose(hTE: TEHandle);
            // Inside Macintosh Volume I, I-383 to I-384; Text 1993, 2-79
            //
            // Regression coverage:
            //   dialog::tests::tedispose_releases_te_record_text_handle_and_pops_arg
            // TEDispose ($A9CD): Frees TERec, hText, and style handles per IM:I I-383..I-384
            (true, 0x1CD) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                if te_handle != 0 {
                    let te_ptr = bus.read_long(te_handle);
                    if te_ptr != 0 {
                        // Free the text handle and its data
                        let h_text = bus.read_long(te_ptr + Self::TE_HTEXT_OFFSET);
                        if h_text != 0 {
                            let text_ptr = bus.read_long(h_text);
                            if text_ptr != 0 {
                                bus.free(text_ptr);
                            }
                            bus.free(h_text);
                        }
                        // For styled records, free the style handle
                        if Self::te_is_styled_record(bus, te_ptr) {
                            let style_handle = bus.read_long(te_ptr + Self::TE_TX_FONT_OFFSET);
                            if style_handle != 0 {
                                let style_ptr = bus.read_long(style_handle);
                                if style_ptr != 0 {
                                    bus.free(style_ptr);
                                }
                                bus.free(style_handle);
                            }
                        }
                        bus.free(te_ptr);
                    }
                    bus.free(te_handle);
                    self.textedit_states.remove(&te_handle);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TESetText ($A9CF)
            // PROCEDURE TESetText(text: Ptr; length: LONGINT; hTE: TEHandle);
            // Inside Macintosh Volume I (1985), p. I-378.
            //
            // IM:I I-378 verbatim: "TESetText sets the current text
            // contents of the edit record specified by hTE. The text
            // parameter points to the text, and the length parameter
            // contains the number of bytes."
            //
            // MPW Universal Headers `TextEdit.h` declares:
            //   EXTERN_API(void) TESetText(const void *text, long length,
            //                              TEHandle hTE) ONEWORDINLINE(0xA9CF);
            //
            // Pascal calling convention pushes args left-to-right (first
            // arg deepest). At trap entry the stack layout is therefore:
            //   sp+0  TEHandle hTE          (last pushed, shallowest)
            //   sp+4  LONGINT  length       (middle, 4 bytes)
            //   sp+8  Ptr      text         (first pushed, deepest)
            // The trap pops 12 bytes (no result slot — PROCEDURE).
            //
            // Empty-input semantics: per IM:I I-378 passing zero length
            // yields an empty edit record. Systemless additionally defends
            // against NIL text pointer (clears regardless of length);
            // the real Mac ROM (BasiliskII System 7.5.3) does NOT safely
            // handle NIL source pointers.
            //
            // Contract tests in this file:
            //   tesettext_copies_bytes_updates_length_and_pops_arguments
            //   tesettext_nil_or_zero_length_input_clears_text
            //   tesettext_replaces_prior_contents_via_sequential_call
            //   tesettext_balances_stackspace_with_pascal_protocol
            (true, 0x1CF) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let length = bus.read_long(sp + 4) as usize;
                let text_ptr = bus.read_long(sp + 8);
                if text_ptr != 0 && length != 0 {
                    let text = bus.read_bytes(text_ptr, length);
                    self.te_set_text_contents(bus, te_handle, &text);
                } else {
                    self.te_set_text_contents(bus, te_handle, &[]);
                }
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // TEGetText ($A9CB)
            // FUNCTION TEGetText(hTE: TEHandle): CharsHandle;
            // Inside Macintosh Volume I (1985), p. I-384.
            //
            // IM:I I-384 verbatim: "TEGetText returns a handle to the text
            // of the specified edit record. The result is the same as the
            // handle in the hText field of the edit record, but has the
            // CharsHandle data type, which is defined as:
            //
            //   TYPE CharsHandle = ^CharsPtr;
            //        CharsPtr    = ^Chars;
            //        Chars       = PACKED ARRAY[0..32000] OF CHAR;
            //
            // You can get the length of the text from the teLength field
            // of the edit record."
            //
            // Pascal FUNCTION calling convention: the caller pre-allocates
            // a 4-byte CharsHandle result slot at SP+4, pushes a 4-byte
            // TEHandle argument at SP+0. The trap reads the TEHandle,
            // looks up TERec.hText (offset TE_HTEXT_OFFSET = +18 per
            // IM:I I-379), writes that handle to the result slot, and
            // pops the 4-byte argument. The C wrapper then pops the
            // 4-byte result slot — net externally-observed SP delta is
            // zero.
            //
            // MPW Universal Headers `TextEdit.h` declares:
            //   EXTERN_API(CharsHandle) TEGetText(TEHandle hTE)
            //       ONEWORDINLINE(0xA9CB);
            (true, 0x1CB) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                bus.write_long(sp + 4, Self::te_text_handle(bus, te_handle));
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TETextBox ($A9CE)
            //
            // PROCEDURE TETextBox(text: Ptr; length: LONGINT;
            //                     box: Rect; align: INTEGER);
            //
            // Inside Macintosh: Text (1993), p. 2-88; alignment
            // constants on p. 2-87.
            //
            // MPW Universal Headers TextEdit.h:
            //   EXTERN_API(void) TETextBox(const void *text,
            //                              long          length,
            //                              const Rect *  box,
            //                              short         just)
            //                                       ONEWORDINLINE(0xA9CE);
            //
            // IM:Text 1993 p. 2-88: "TETextBox erases the specified
            //   rectangle and then draws the text into it... TETextBox
            //   creates a transient edit record, draws the text wrapped
            //   to fit the rectangle, and disposes of the record."
            // IM:Text 1993 p. 2-87 alignment constants:
            //   teJustLeft  =  0  (flush left — system default)
            //   teJustCenter =  1  (centered)
            //   teJustRight  = -1  (flush right)
            //   teForceLeft  = -2  (force flush left for right-to-left)
            //
            // Calling convention (MPW C canonical, the only one MPW
            // emits because the Universal Headers declaration uses
            // `const Rect *`):
            //   sp+ 0  short        just     (last pushed, shallowest)
            //   sp+ 2  Rect *       box      (4-byte pointer)
            //   sp+ 6  long         length   (4 bytes)
            //   sp+10  const void * text     (4 bytes, deepest)
            //   pop = 14 bytes; no result slot (PROCEDURE).
            //
            // (The Pascal canonical signature inlines the 8-byte Rect
            // by value for an 18-byte frame, but MPW C never produces
            // that form. Systemless's HLE only supports the MPW C
            // canonical layout; the in-Rust contract tests exercise
            // this layout exclusively.)
            //
            // Documented behaviors (BII System 7.5.3 ROM and Systemless
            // HLE produce the same boolean predicates):
            //   1. Erases the destination box before drawing (probe a
            //      pre-blackened pixel far from any glyph is WHITE after
            //      the call) — IM:Text 1993 p. 2-88.
            //   2. Word-wraps when text exceeds rect width (40x60 box
            //      with "A B C D E F G" has black pixels at row 14+;
            //      800x16 box with same text has none) — IM:Text 1993
            //      p. 2-88 line layout behavior.
            //   3. Alignment parameter controls horizontal origin
            //      (teJustLeft / teJustCenter / teJustRight produce
            //      strictly increasing leftmost-black-pixel columns)
            //      — IM:Text 1993 p. 2-87.
            //   4. Pascal PROCEDURE pops 14 bytes, no result slot
            //      (StackSpace bookends equal).
            (true, 0x1CE) => {
                let sp = cpu.read_reg(Register::A7);
                let align = bus.read_word(sp) as i16;
                let box_ptr = bus.read_long(sp + 2);
                let box_top = bus.read_word(box_ptr) as i16;
                let box_left = bus.read_word(box_ptr + 2) as i16;
                let box_bottom = bus.read_word(box_ptr + 4) as i16;
                let box_right = bus.read_word(box_ptr + 6) as i16;
                let length = bus.read_long(sp + 6) as usize;
                let text_ptr = bus.read_long(sp + 10);
                cpu.write_reg(Register::A7, sp + 14);

                if trace_dialog_text_inline_enabled() {
                    let mut stack_bytes = [0u8; 24];
                    for (i, byte) in stack_bytes.iter_mut().enumerate() {
                        *byte = bus.read_byte(sp + i as u32);
                    }
                    let preview_len = length.min(64);
                    let preview = if text_ptr != 0 {
                        bus.read_bytes(text_ptr, preview_len)
                    } else {
                        Vec::new()
                    };
                    eprintln!(
                        "[DIALOG-TEXT] TETextBox params current_port=${:08X} sp=${:08X} stack={:02X?} text_ptr=${:08X} len={} box=({},{}..{},{} ) align={} preview={:02X?}",
                        *self.current_port,
                        sp,
                        stack_bytes,
                        text_ptr,
                        length,
                        box_top,
                        box_left,
                        box_bottom,
                        box_right,
                        align,
                        preview,
                    );
                }

                if box_right > box_left && box_bottom > box_top {
                    // TETextBox creates a transient edit record and clears the
                    // destination box before drawing the wrapped text. The erase
                    // happens even when length is zero.
                    // Text 1993, 2-88; Executor textedit/teDisplay.cpp C_TETextBox
                    self.draw_rect(
                        cpu,
                        bus,
                        &Rect {
                            top: box_top,
                            left: box_left,
                            bottom: box_bottom,
                            right: box_right,
                        },
                        ShapeOp::Erase,
                    );

                    if text_ptr != 0 && length > 0 {
                        // Read the text bytes from guest memory
                        let text_bytes = bus.read_bytes(text_ptr, length);

                        if trace_dialog_text_inline_enabled() {
                            let preview_len = text_bytes.len().min(160);
                            let first_char =
                                text_bytes.first().copied().map(char::from).unwrap_or('\0');
                            let first_glyph = crate::quickdraw::text::get_glyph(
                                self.tx_font,
                                self.tx_size,
                                first_char,
                            )
                            .is_some();
                            eprintln!(
                            "[DIALOG-TEXT] TETextBox current_port=${:08X} box=({},{}..{},{} ) align={} len={} txFont={} txFace=${:04X} txMode={} txSize={} firstChar={:?} firstGlyph={} text=\"{}\"",
                            *self.current_port,
                            box_top,
                            box_left,
                            box_bottom,
                            box_right,
                            align,
                            length,
                            self.tx_font,
                            self.tx_face,
                            self.tx_mode,
                            self.tx_size,
                            first_char,
                            first_glyph,
                            String::from_utf8_lossy(&text_bytes[..preview_len]),
                        );
                        }

                        // Capture font params to avoid borrowing self in closures
                        let font_id = self.tx_font;
                        let font_size = self.tx_size;
                        let advance_extra = self.advance_extra();
                        let missing_advance = self.missing_glyph_advance();

                        let metrics = crate::quickdraw::text::get_font_metrics(font_id, font_size);
                        let line_height = metrics.ascent + metrics.descent + metrics.leading;
                        let box_width = box_right - box_left;

                        // Measure a run of bytes (no &self borrow needed)
                        let measure = |start: usize, end: usize| -> i16 {
                            let mut w = 0i16;
                            for &b in &text_bytes[start..end] {
                                let ch = b as char;
                                if let Some((g, _)) =
                                    crate::quickdraw::text::get_glyph(font_id, font_size, ch)
                                {
                                    w += g.advance as i16 + advance_extra;
                                } else {
                                    w += missing_advance;
                                }
                            }
                            w
                        };

                        let lines = crate::quickdraw::text::wrap_classic_text(
                            &text_bytes,
                            box_width,
                            |_, byte| {
                                crate::quickdraw::text::get_glyph(
                                    font_id,
                                    font_size,
                                    byte as char,
                                )
                                .map_or(missing_advance, |(glyph, _)| {
                                    glyph.advance as i16 + advance_extra
                                })
                            },
                        );

                        // Draw each line
                        let mut y = box_top + metrics.ascent;
                        for line in &lines {
                            if y + metrics.descent > box_bottom {
                                break;
                            }
                            let lw = measure(line.start, line.visible_end);
                            // Per Inside Macintosh: Text 1993, lines 7320-7323:
                            //   teJustLeft   =  0 (flush left — system default)
                            //   teJustCenter =  1 (centered)
                            //   teJustRight  = -1 (flush right)
                            //   teForceLeft  = -2 (force flush left)
                            let x = crate::text_edit::aligned_line_left(
                                box_left,
                                box_right,
                                lw,
                                align,
                                Self::TE_LINE_LEFT_INSET,
                            );
                            self.pn_loc = (y, x);
                            for &byte in &text_bytes[line.start..line.visible_end] {
                                self.draw_char(cpu, bus, byte as char);
                            }
                            y += line_height;
                        }
                    }
                }
                self.refresh_visible_dialog_snapshot_for_port(bus, *self.current_port);
                Ok(())
            }

            // TECalText ($A9D0)
            // PROCEDURE TECalText(hTE: TEHandle);
            // TECalText ($A9D0): Syncs TE_LENGTH and recalculates TE line layout per IM:I I-390
            (true, 0x1D0) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let te_ptr = Self::te_record_ptr(bus, te_handle);
                if te_ptr != 0 {
                    let text_len =
                        Self::te_text_length(bus, te_handle).min(u16::MAX as usize) as u16;
                    bus.write_word(te_ptr + Self::TE_LENGTH_OFFSET, text_len);
                    self.te_recalculate_layout(bus, te_handle);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TESetSelect ($A9D1)
            // PROCEDURE TESetSelect(selStart: LONGINT; selEnd: LONGINT; hTE: TEHandle);
            // Inside Macintosh Volume I, I-385
            // TESetSelect ($A9D1): Clamps selEnd to teLength per IM:I I-385; selStart/selEnd range 0..32767 stored as u16
            (true, 0x1D1) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let te_ptr = Self::te_record_ptr(bus, te_handle);
                if te_ptr != 0 {
                    let te_length = u32::from(bus.read_word(te_ptr + Self::TE_LENGTH_OFFSET));
                    // IM:I I-385: "SelEnd and selStart can range from 0 to 32767.
                    // If selEnd is anywhere beyond the last character of the text,
                    // the position just past the last character is used."
                    let sel_end = bus
                        .read_long(sp + 4)
                        .min(u32::from(u16::MAX))
                        .min(te_length);
                    let sel_start = bus.read_long(sp + 8).min(u32::from(u16::MAX));
                    bus.write_word(te_ptr + Self::TE_SEL_START_OFFSET, sel_start as u16);
                    bus.write_word(te_ptr + Self::TE_SEL_END_OFFSET, sel_end as u16);
                }
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // TEUpdate ($A9D3)
            // Redraws text within the update rectangle.
            // PROCEDURE TEUpdate (rUpdate: Rect; hTE: TEHandle);
            // Inside Macintosh: Text (1993), p. 2-90; MPW TextEdit.h:
            // pascal void TEUpdate(const Rect *rUpdate, TEHandle hTE).
            // The rectangle is passed by address, followed by hTE. Always
            // consume two pointers, including for stack-local or empty Rects.
            (true, 0x1D3) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let te_ptr = Self::te_record_ptr(bus, te_handle);
                if te_ptr != 0 {
                    if trace_textedit_enabled() {
                        let dest_rect = Self::te_read_rect(bus, te_ptr + Self::TE_DEST_RECT_OFFSET);
                        let view_rect = Self::te_read_rect(bus, te_ptr + Self::TE_VIEW_RECT_OFFSET);
                        eprintln!(
                            "[TE] TEUpdate hTE=${:08X} dest=({},{},{},{}) view=({},{},{},{})",
                            te_handle,
                            dest_rect.0,
                            dest_rect.1,
                            dest_rect.2,
                            dest_rect.3,
                            view_rect.0,
                            view_rect.1,
                            view_rect.2,
                            view_rect.3
                        );
                    }
                    self.draw_te_contents(cpu, bus, te_handle, false);
                    let te_port = bus.read_long(te_ptr + Self::TE_IN_PORT_OFFSET);
                    self.refresh_visible_dialog_snapshot_for_port(bus, te_port);
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // TEClick (0xA9D4)
            // Positions or extends the selection and owns the mouse until release.
            // PROCEDURE TEClick (pt: Point; extend: BOOLEAN; hTE: TEHandle);
            // Inside Macintosh: Text (1993), p. 2-85.
            (true, 0x1D4) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let te_ptr = Self::te_record_ptr(bus, te_handle);
                if te_ptr == 0 {
                    self.textedit_states.clear_click_tracking();
                    cpu.write_reg(Register::A7, sp + 10);
                    return Some(Ok(()));
                }
                let previous_selection = (
                    bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET),
                    bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET),
                );
                let tracking = self.textedit_states.take_click_tracking();
                let point = if tracking.is_some() {
                    let port = bus.read_long(te_ptr + Self::TE_IN_PORT_OFFSET);
                    let (top, left) = self.port_bounds_top_left(bus, port);
                    let (v, h) = self.window_tracking_mouse_pos(bus);
                    (v.wrapping_add(top), h.wrapping_add(left))
                } else {
                    (bus.read_word(sp + 6) as i16, bus.read_word(sp + 8) as i16)
                };
                let offset = self.te_point_to_char(bus, te_handle, point).max(0) as usize;
                let anchor = tracking.map_or_else(
                    || {
                        if bus.read_byte(sp + 4) != 0 {
                            let start = bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET) as usize;
                            let end = bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) as usize;
                            if offset < start {
                                end
                            } else {
                                start
                            }
                        } else {
                            offset
                        }
                    },
                    |tracking| tracking.anchor,
                );
                let length = bus.read_word(te_ptr + Self::TE_LENGTH_OFFSET) as usize;
                let anchor = anchor.min(length);
                bus.write_word(
                    te_ptr + Self::TE_SEL_START_OFFSET,
                    anchor.min(offset) as u16,
                );
                bus.write_word(te_ptr + Self::TE_SEL_END_OFFSET, anchor.max(offset) as u16);
                bus.write_word(te_ptr + Self::TE_SEL_POINT_OFFSET, point.0 as u16);
                bus.write_word(te_ptr + Self::TE_SEL_POINT_OFFSET + 2, point.1 as u16);
                bus.write_long(te_ptr + Self::TE_CARET_TIME_OFFSET, self.current_tick());
                if previous_selection != (anchor.min(offset) as u16, anchor.max(offset) as u16) {
                    self.draw_te_contents(cpu, bus, te_handle, true);
                }
                if self.window_tracking_button_down(bus) {
                    self.textedit_states.retain_click_tracking(
                        crate::text_edit::TextEditClickTracking {
                            handle: te_handle,
                            anchor,
                            native: false,
                            last_point: point,
                        },
                    );
                } else {
                    if let Some(index) = self.event_queue.iter().position(|event| event.what == 2) {
                        self.event_queue.remove(index);
                    }
                    cpu.write_reg(Register::A7, sp + 10);
                }
                Ok(())
            }


            // TECopy ($A9D5)
            // PROCEDURE TECopy(hTE: TEHandle);
            // Copies the currently-selected bytes into the TextEdit
            // scrap (TEScrpLength low-mem global at $0AB0 (word) /
            // TEScrpHandle at $0AB4 (long)). Empty selection leaves
            // the scrap untouched per IM:I I-386.
            // Inside Macintosh Volume I, I-386
            // TECopy ($A9D5): Writes selected bytes into TextEdit scrap low-mem globals (TEScrpLength $0AB0, TEScrpHandle $0AB4) per IM:I I-386. Empty selection leaves scrap untouched.
            (true, 0x1D5) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                if let Some(buffer) = Self::te_edit_buffer(bus, te_handle) {
                    if !buffer.selected_text().is_empty() {
                        Self::te_set_scrap_bytes(bus, buffer.selected_text());
                    }
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TECut ($A9D6)
            // PROCEDURE TECut(hTE: TEHandle);
            // Removes the currently-selected text from the edit record
            // and places a copy in the TextEdit scrap (TEScrpLength at
            // $0AB0 / TEScrpHandle at $0AB4). Any previous scrap
            // contents are replaced. Insertion-point selection is a
            // no-op per IM:I I-391.
            // TECut ($A9D6): Removes selected text from TERec and writes a copy into the TextEdit scrap low-mem globals per IM:I I-391. Empty selection is a no-op.
            (true, 0x1D6) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                if let Some(mut buffer) = Self::te_edit_buffer(bus, te_handle) {
                    if !buffer.selected_text().is_empty() {
                        Self::te_set_scrap_bytes(bus, buffer.selected_text());
                        buffer.delete_selection();
                        self.te_commit_edit_buffer(bus, te_handle, &buffer);
                    }
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TEDelete ($A9D7)
            // Removes the currently selected text from the edit record and
            // redraws. If selStart == selEnd (insertion point), does nothing.
            // Does NOT transfer text to the scrap (unlike TECut).
            // PROCEDURE TEDelete(hTE: TEHandle);
            // Inside Macintosh Volume I, I-387; Text 1993, 2-93
            //
            // Regression coverage:
            //   dialog::tests::tedelete_removes_selection_without_touching_scrap_and_pops_arg
            //   dialog::tests::tedelete_insertion_point_selection_is_noop_and_pops_arg
            // TEDelete ($A9D7): Removes selected text, collapses selection; no-op at insertion point. IM:I I-387
            (true, 0x1D7) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                if trace_textedit_enabled() {
                    eprintln!("[TE] TEDelete hTE=${:08X}", te_handle);
                }
                if let Some(mut buffer) = Self::te_edit_buffer(bus, te_handle) {
                    if buffer.delete_selection() {
                        self.te_commit_edit_buffer(bus, te_handle, &buffer);
                    }
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TEActivate ($A9D8)
            // Activates the edit record, highlighting the selection range or
            // displaying a blinking caret at the insertion point.
            // PROCEDURE TEActivate(hTE: TEHandle);
            // Inside Macintosh Volume I, I-385; Text 1993, 2-80
            //
            // Regression coverage:
            //   teactivate_and_tedeactivate_repaint_empty_insertion_caret
            // TEActivate ($A9D8): Sets TERec.active flag per IM:I I-385
            (true, 0x1D8) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                if trace_textedit_enabled() {
                    eprintln!("[TE] TEActivate hTE=${:08X}", te_handle);
                }
                let te_ptr = Self::te_record_ptr(bus, te_handle);
                if te_ptr != 0 {
                    bus.write_word(te_ptr + Self::TE_ACTIVE_OFFSET, 1);
                    bus.write_long(te_ptr + Self::TE_CARET_TIME_OFFSET, self.current_tick());
                    bus.write_word(te_ptr + Self::TE_CARET_STATE_OFFSET, 0);
                    self.draw_te_contents(cpu, bus, te_handle, true);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TEDeactivate ($A9D9)
            // Deactivates the edit record, unhighlighting the selection range
            // or removing the caret. Does not affect selStart/selEnd.
            // PROCEDURE TEDeactivate(hTE: TEHandle);
            // Inside Macintosh Volume I, I-385; Text 1993, 2-80
            //
            // Regression coverage:
            //   teactivate_and_tedeactivate_repaint_empty_insertion_caret
            // TEDeactivate ($A9D9): Clears TERec.active flag per IM:I I-385
            (true, 0x1D9) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                if trace_textedit_enabled() {
                    eprintln!("[TE] TEDeactivate hTE=${:08X}", te_handle);
                }
                let te_ptr = Self::te_record_ptr(bus, te_handle);
                if te_ptr != 0 {
                    bus.write_word(te_ptr + Self::TE_ACTIVE_OFFSET, 0);
                    self.draw_te_contents(cpu, bus, te_handle, true);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TEIdle ($A9DA)
            // PROCEDURE TEIdle(hTE: TEHandle);
            // Inside Macintosh Volume I (1985), p. I-374; Inside
            //   Macintosh: Text (1993), p. 2-84.
            // MPW Universal Headers TextEdit.h:
            //   EXTERN_API(void) TEIdle(TEHandle hTE) ONEWORDINLINE(0xA9DA);
            //
            // Pascal stack frame:
            //   sp+0  hTE: TEHandle  (4)
            // Total pop = 4 bytes; no function-result slot. Net
            // externally-observed SP delta is zero (StackSpace
            // bookends are equal across one call).
            //
            // IM:I I-374 quote: "Call TEIdle from your event loop...
            // TEIdle causes the caret of the edit record to blink at
            // the rate specified in caretTime."
            //
            // IM:Text 1993 p. 2-84 quote: "TEIdle blinks the caret if
            // the destination rectangle contains the caret position;
            // if the specified edit record is inactive (such as when
            // the window is inactive), TEIdle has no effect."
            //
            // HLE model: TextEdit's TERec layout includes the private
            // caretTime and caretState fields before just/teLength. Systemless
            // treats caretTime as the last toggle tick and caretState == 0 as
            // visible. On an active insertion point, TEIdle waits the
            // documented initial 32-tick interval, toggles caretState, and
            // redraws the edit record. Non-insertion selections do not blink.
            //
            // Crucial no-mutation contract: TEIdle MUST NOT mutate TERec.active,
            // TERec.selStart, TERec.selEnd, TERec.teLength, or any
            // other selection / text fields. Selection updates are
            // the exclusive responsibility of TESetSelect /
            // TEClick / TEKey / TEDelete; activation toggles are
            // handled by TEActivate / TEDeactivate.
            //
            // NIL hTE is a defensive no-op.
            //
            // Regression coverage:
            //   dialog::tests::teidle_consumes_tehandle_argument
            //   dialog::tests::teidle_toggles_visible_insertion_caret_after_blink_interval
            //   dialog::tests::teidle_preserves_active_flag_and_selection_offsets
            //   dialog::tests::teidle_repeated_calls_balance_stack_and_preserve_terec_state
            (true, 0x1DA) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                self.textedit_idle(cpu, bus, te_handle);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TEPaste ($A9DB)
            // PROCEDURE TEPaste(hTE: TEHandle);
            // Replaces the current selection with the contents of the
            // TextEdit scrap (TEScrpLength at $0AB0 word, TEScrpHandle
            // at $0AB4 long). Empty scrap leaves the TERec unchanged
            // per IM:I I-387.
            // Inside Macintosh Volume I, I-387
            // TEPaste ($A9DB): Replaces current selection with TextEdit scrap contents (TEScrpLength / TEScrpHandle low-mem globals) per IM:I I-387. Empty scrap is a no-op.
            (true, 0x1DB) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                if trace_textedit_enabled() {
                    eprintln!("[TE] TEPaste hTE=${:08X}", te_handle);
                }
                use crate::memory::globals::addr;
                let scrap_len = bus.read_word(addr::TE_SCRP_LENGTH) as usize;
                let scrap_handle = bus.read_long(addr::TE_SCRP_HANDLE);
                if scrap_handle != 0 && scrap_len > 0 {
                    let scrap_ptr = bus.read_long(scrap_handle);
                    if scrap_ptr != 0 {
                        let scrap = bus.read_bytes(scrap_ptr, scrap_len);
                        self.te_insert_text(bus, te_handle, &scrap);
                    }
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // TEKey ($A9DC)
            // Replaces the selection range with the input character and positions
            // the insertion point just past it. Backspace ($08) deletes the
            // selection or the character before the insertion point.
            // PROCEDURE TEKey(key: CHAR; hTE: TEHandle);
            // Inside Macintosh Volume I, I-385; Inside Macintosh: Text
            // (1993), pp. 2-81 to 2-82. In addition to editing the TERec,
            // TEKey "redraws the text as necessary" before it returns.
            //
            // TEKey ($A9DC): Inserts character at insertion point, replaces selection; backspace deletes. IM:I I-385
            (true, 0x1DC) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let key = bus.read_word(sp + 4) as u8;
                if trace_textedit_enabled() {
                    eprintln!(
                        "[TE] TEKey hTE=${:08X} key=${:02X} {:?}",
                        te_handle,
                        key,
                        char::from(key).escape_default().to_string()
                    );
                }
                if let Some(mut buffer) = Self::te_edit_buffer(bus, te_handle) {
                    buffer.apply_key(key);
                    self.te_commit_edit_buffer(bus, te_handle, &buffer);

                    // TEKey is a drawing operation as well as an editing
                    // operation. Applications do not need to follow every
                    // key with TEUpdate; the TextEdit Manager redraws the
                    // affected text and insertion point itself.
                    self.draw_te_contents(cpu, bus, te_handle, true);
                }
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // TEScroll ($A9DD)
            // PROCEDURE TEScroll(dh: INTEGER; dv: INTEGER; hTE: TEHandle);
            // TEScroll ($A9DD): Scrolls destRect by (dh, dv) and redraws per IM:I I-389 / Text 1993 2-89
            (true, 0x1DD) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let dv = bus.read_word(sp + 4) as i16;
                let dh = bus.read_word(sp + 6) as i16;
                if trace_textedit_enabled() {
                    eprintln!("[TE] TEScroll hTE=${:08X} dh={} dv={}", te_handle, dh, dv);
                }
                self.te_scroll_contents(cpu, bus, te_handle, dh, dv);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // TEInsert ($A9DE)
            // PROCEDURE TEInsert(text: Ptr; length: LONGINT; hTE: TEHandle);
            // Inside Macintosh Volume I, I-387; Inside Macintosh: Text 1993,
            // 2-94. TEInsert splices the supplied bytes into hText at the
            // selStart offset (inserting JUST BEFORE the selection range, not
            // replacing it). The selection range is preserved logically —
            // selStart and selEnd both shift forward by `length` so the
            // selection continues to span the same original characters.
            // TEInsert does not touch the scrap.
            (true, 0x1DE) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let length = bus.read_long(sp + 4) as usize;
                let text_ptr = bus.read_long(sp + 8);
                if text_ptr != 0 && length != 0 {
                    let te_ptr = Self::te_record_ptr(bus, te_handle);
                    if te_ptr != 0 {
                        let existing = Self::te_text_bytes(bus, te_handle);
                        let sel_start = (bus.read_word(te_ptr + Self::TE_SEL_START_OFFSET)
                            as usize)
                            .min(existing.len());
                        let sel_end = (bus.read_word(te_ptr + Self::TE_SEL_END_OFFSET) as usize)
                            .min(existing.len());
                        let text = bus.read_bytes(text_ptr, length);
                        let mut merged = Vec::with_capacity(existing.len() + text.len());
                        merged.extend_from_slice(&existing[..sel_start]);
                        merged.extend_from_slice(&text);
                        merged.extend_from_slice(&existing[sel_start..]);
                        self.te_set_text_contents(bus, te_handle, &merged);
                        let shifted_start = (sel_start + text.len()).min(u16::MAX as usize) as u16;
                        let shifted_end = (sel_end + text.len()).min(u16::MAX as usize) as u16;
                        bus.write_word(te_ptr + Self::TE_SEL_START_OFFSET, shifted_start);
                        bus.write_word(te_ptr + Self::TE_SEL_END_OFFSET, shifted_end);
                    }
                }
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // TESetAlignment ($A9DF)
            // Sets the alignment (justification) of the text in the edit record.
            // PROCEDURE TESetAlignment(just: INTEGER; hTE: TEHandle);
            // Inside Macintosh Volume I, I-387; Text 1993, 2-87
            //
            // TESetAlignment ($A9DF): Writes just field to TERec per IM:I I-387
            (true, 0x1DF) => {
                let sp = cpu.read_reg(Register::A7);
                let te_handle = bus.read_long(sp);
                let just = bus.read_word(sp + 4) as i16;
                let te_ptr = Self::te_record_ptr(bus, te_handle);
                if te_ptr != 0 {
                    bus.write_word(te_ptr + Self::TE_JUST_OFFSET, just as u16);
                }
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // SelectDialogItemText ($A97E)
            // PROCEDURE SelectDialogItemText(theDialog: DialogPtr;
            //     itemNo: INTEGER; strtSel: INTEGER; endSel: INTEGER);
            // Selects and highlights text in an editable text dialog item.
            // Inside Macintosh Volume I, I-414; Macintosh Toolbox Essentials
            // 1992, 6-131..6-132
            //
            // Pascal stack frame:
            //   SP+0  endSel: INTEGER     (2) — last arg, shallowest
            //   SP+2  strtSel: INTEGER    (2)
            //   SP+4  itemNo: INTEGER     (2)
            //   SP+6  theDialog: DialogPtr (4) — first arg, deepest
            // Total pop = 10 bytes.
            //
            // Behaviour per IM:I I-414:
            //   - "Selects the text from character strtSel through
            //     character endSel-1 in the editable text item with
            //     item number itemNo of the specified dialog box."
            //   - Special case: "If you set strtSel = 0 and
            //     endSel = -1, the entire text is selected" — we
            //     normalize this to (0, text.len()) at dispatch.
            //   - If itemNo is not an editText item (type 16), the
            //     procedure is a no-op (real Mac just ignores it).
            //   - The named item becomes the dialog's active edit
            //     field (DialogRecord.editField at offset 164).
            //
            // Systemless stores the normalized range in DialogItem and mirrors
            // it into DialogRecord.textH's TERecord when present, then redraws
            // the visible editText item so the selection highlight is
            // immediate like SelIText / SelectDialogItemText.
            (true, 0x17E) => {
                let sp = cpu.read_reg(Register::A7);
                let end_sel = bus.read_word(sp) as i16;
                let start_sel = bus.read_word(sp + 2) as i16;
                let item_no = bus.read_word(sp + 4) as i16;
                let dialog_ptr = bus.read_long(sp + 6);
                let mut redraw_item = None;
                if dialog_ptr != 0 && item_no > 0 {
                    if let Some(items) = self.dialog_items.get_mut(&dialog_ptr) {
                        if let Some(item) = items.get_mut((item_no - 1) as usize) {
                            if item.item_type & 0x7F == 16 {
                                // IM:I I-414 special case: (0, -1)
                                // means "select all" — normalize
                                // to (0, text.len()).
                                let text_len = encode_mac_roman_lossy(&item.text)
                                    .len()
                                    .min(i16::MAX as usize)
                                    as i16;
                                let (s, e) = if start_sel == 0 && end_sel == -1 {
                                    (0, text_len)
                                } else {
                                    // Clamp to text bounds.
                                    let s = start_sel.max(0).min(text_len);
                                    let e = end_sel.max(0).min(text_len);
                                    // Swap if reversed (defensive
                                    // — real Mac also normalizes).
                                    if s <= e {
                                        (s, e)
                                    } else {
                                        (e, s)
                                    }
                                };
                                item.sel_start = s;
                                item.sel_end = e;
                                bus.write_word(dialog_ptr + 164, (item_no - 1) as u16);
                                // Mirror selStart/selEnd into the TERecord so
                                // callers that read (**textH).selStart via the
                                // canonical DialogRecord layout can verify the
                                // selection (IM:I I-382, I-414).
                                // textH is a TEHandle at dialog_ptr+160
                                // (IM:I I-411 DialogRecord layout).
                                let text_h_handle = bus.read_long(dialog_ptr + 160);
                                if text_h_handle != 0 {
                                    let te_ptr = bus.read_long(text_h_handle);
                                    if te_ptr != 0 {
                                        bus.write_word(
                                            te_ptr + Self::TE_SEL_START_OFFSET,
                                            s as u16,
                                        );
                                        bus.write_word(te_ptr + Self::TE_SEL_END_OFFSET, e as u16);
                                    }
                                }
                                redraw_item = Some((dialog_ptr, item_no));
                            }
                        }
                    }
                }
                if let Some((dialog_ptr, item_no)) = redraw_item {
                    self.redraw_dialog_text_item(bus, dialog_ptr, item_no);
                }
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // NewCDialog ($AA4B)
            // Color-aware variant of NewDialog. Identical parameters and
            // semantics; internally uses a CGrafPort instead of GrafPort
            // so the dialog supports color drawing. For our HLE this is
            // the same code path as NewDialog.
            // FUNCTION NewCDialog(dStorage: Ptr; boundsRect: Rect;
            //     title: Str255; visible: BOOLEAN; procID: INTEGER;
            //     behind: WindowPtr; goAwayFlag: BOOLEAN;
            //     refCon: LongInt; items: Handle): CDialogPtr;
            // Inside Macintosh Volume V, V-243
            //
            // NewDialog ($A97D)
            // Creates a dialog from a caller-supplied item list handle.
            // FUNCTION NewDialog(dStorage: Ptr; boundsRect: Rect; title: Str255;
            //     visible: BOOLEAN; procID: INTEGER; behind: WindowPtr;
            //     goAwayFlag: BOOLEAN; refCon: LONGINT; items: Handle): DialogPtr;
            // Inside Macintosh Volume I, I-412
            // NewDialog ($A97D): Allocates DialogRecord, processes DITL items, pops 32 bytes
            // NewCDialog ($AA4B): Color-aware NewDialog; same code path, CGrafPort variant per IM:V V-243
            (true, 0x17D) | (true, 0x24B) => {
                let sp = cpu.read_reg(Register::A7);
                // 68K Pascal stack:
                //   SP+0:  items(4)
                //   SP+4:  refCon(4)
                //   SP+8:  goAwayFlag(2)
                //   SP+10: behind(4)
                //   SP+14: procID(2)
                //   SP+16: visible(2)
                //   SP+18: title(4)
                //   SP+22: boundsRect(4)
                //   SP+26: dStorage(4)
                //   SP+30: result(4)
                let items_handle = bus.read_long(sp);
                let ref_con = bus.read_long(sp + 4);
                // Pascal BOOLEAN as the HIGH byte of its 2-byte stack slot
                // (MPW C convention).
                let go_away_flag = bus.read_byte(sp + 8) != 0;
                let behind = bus.read_long(sp + 10);
                let proc_id = bus.read_word(sp + 14) as i16;
                let visible = bus.read_byte(sp + 16) != 0;
                let title_ptr = bus.read_long(sp + 18);
                let bounds_rect_ptr = bus.read_long(sp + 22);
                let storage_ptr = bus.read_long(sp + 26);

                let bounds = if bounds_rect_ptr != 0 {
                    (
                        bus.read_word(bounds_rect_ptr) as i16,
                        bus.read_word(bounds_rect_ptr + 2) as i16,
                        bus.read_word(bounds_rect_ptr + 4) as i16,
                        bus.read_word(bounds_rect_ptr + 6) as i16,
                    )
                } else {
                    (0, 0, 0, 0)
                };
                let title = if title_ptr != 0 {
                    decode_mac_roman(&bus.read_pstring(title_ptr))
                } else {
                    String::new()
                };

                let items_ptr = if items_handle != 0 {
                    bus.read_long(items_handle)
                } else {
                    0
                };
                let items_len = if items_ptr != 0 {
                    bus.get_alloc_size(items_ptr).unwrap_or(0)
                } else {
                    0
                };
                let items = if items_ptr != 0 && items_len != 0 {
                    Self::parse_ditl(bus, items_ptr, items_len)
                } else {
                    Vec::new()
                };

                let dlg_ptr = self.finish_dialog_creation(
                    bus,
                    cpu,
                    storage_ptr,
                    bounds,
                    &title,
                    visible,
                    proc_id,
                    go_away_flag,
                    ref_con,
                    items_handle,
                    items,
                    None,
                    None,
                );
                // Honor Pascal `behind` param at SP+10 per IM:I I-412.
                self.apply_behind_parameter(bus, dlg_ptr, behind);
                bus.write_long(sp + 30, dlg_ptr);
                cpu.write_reg(Register::A7, sp + 30);
                Ok(())
            }

            // CloseDialog ($A982)
            // PROCEDURE CloseDialog(theDialog: DialogPtr);
            // Inside Macintosh Volume I, I-413
            // CloseDialog ($A982): Removes the dialog from the window list
            // and restores previous window state (bounds, procID, title)
            // when the closed dialog was frontmost. IM:I I-413 explicitly
            // states "deletes the dialog window from the window list" —
            // routed through untrack_window so FrontWindow sees the
            // updated list, matching DisposDialog's pattern.
            (true, 0x182) => {
                let sp = cpu.read_reg(Register::A7);
                let dialog_ptr = bus.read_long(sp);
                self.close_dialog_window(bus, cpu, dialog_ptr, false);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // UpdtDialog ($A978)
            // Redraws the dialog items in the specified update region.
            // PROCEDURE UpdtDialog(theDialog: DialogPtr; updateRgn: RgnHandle);
            // Inside Macintosh Volume I, I-415; Macintosh Toolbox Essentials 1992, 6-142..6-143
            (true, 0x178) => {
                let sp = cpu.read_reg(Register::A7);
                let update_rgn = bus.read_long(sp);
                let dialog_ptr = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);
                if update_rgn == 0 {
                    return Some(Ok(()));
                }
                let update_rect = Self::region_handle_rect(bus, update_rgn);
                self.update_dialog_window_contents(bus, cpu, dialog_ptr, update_rect);
                Ok(())
            }

            // CouldDialog ($A979) / FreeDialog ($A97A)
            // PROCEDURE CouldDialog(dialogID: INTEGER);
            // PROCEDURE FreeDialog (dialogID: INTEGER);
            // Inside Macintosh Volume I, I-415
            //
            // Per IM:I I-415, CouldDialog makes the DLOG, its DITL, and
            // resource-backed DITL items unpurgeable, loading them first if
            // needed; FreeDialog makes the same already-loaded resources
            // purgeable again. Systemless models that visible handle-state
            // axis for DLOG/DITL plus CNTL/ICON/PICT DITL resources. WDEF
            // cascade is intentionally omitted: HLE dialog window procs are
            // not loaded or executed as guest WDEF resources.
            // CouldDialog ($A979): Loads and HNoPurge-equivalent marks DLOG/DITL/resource-backed items; writes ResErr noErr on both hit and miss in BasiliskII.
            // FreeDialog ($A97A): HPurge-equivalent marks the already-loaded DLOG/DITL/resource-backed items; writes ResErr noErr on both hit and miss in BasiliskII.
            (true, 0x179) | (true, 0x17A) => {
                let sp = cpu.read_reg(Register::A7);
                let dialog_id = bus.read_word(sp) as i16;
                let load_if_missing = trap_num == 0x179;
                let purgeable = trap_num == 0x17A;
                self.cascade_dialog_resource_purgeability(
                    bus,
                    *b"DLOG",
                    dialog_id,
                    purgeable,
                    load_if_missing,
                );
                let res_err = self.dialog_template_res_err(dialog_id);
                bus.write_word(0x0A60, res_err as u16);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // HideDialogItem ($A827)
            // Erases the item's enclosing rectangle, then moves the
            // item's display rect offscreen so it isn't drawn or hit-tested.
            // The original rect is saved so ShowDialogItem can restore it.
            // PROCEDURE HideDialogItem(theDialog: DialogPtr; itemNo: INTEGER);
            // Inside Macintosh Volume IV, IV-59
            // Stack: SP+0=itemNo(2), SP+2=theDialog(4). Pop 6.
            // HideDialogItem ($A827): If itemRect.left < 8192, calls EraseRect on
            // the item's enclosing rectangle, adds that rectangle to the update
            // region, and offsets itemRect.left/right by +16384 so the item
            // becomes offscreen; already-hidden items are unchanged
            // (Inside Macintosh Volume IV, IV-59; MTE 1992, 6-123).
            // Pops 6 bytes per IM:IV IV-59.
            (true, 0x027) => {
                let sp = cpu.read_reg(Register::A7);
                let item_no = bus.read_word(sp) as i16;
                let dialog_ptr = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);
                let mut updated_control_rect = None;
                let mut redraw_local_rect = None;
                if dialog_ptr != 0 && item_no > 0 {
                    let key = (dialog_ptr, item_no);
                    if let Some(items) = self.dialog_items.get_mut(&dialog_ptr) {
                        let idx = (item_no as usize).wrapping_sub(1);
                        if idx < items.len() {
                            let rect = items[idx].rect;
                            // MTE 1992, 6-123: already-hidden items (left > 8192) are a no-op.
                            if rect.1 < 8192 {
                                let enclosing = Self::dialog_item_enclosing_local_rect(
                                    items[idx].item_type,
                                    rect,
                                );
                                self.hidden_dialog_item_rects.entry(key).or_insert(rect);
                                items[idx].rect.1 = rect.1.wrapping_add(16384);
                                items[idx].rect.3 = rect.3.wrapping_add(16384);
                                updated_control_rect = Some(items[idx].rect);
                                redraw_local_rect = Some(enclosing);
                            }
                        }
                    }
                    if let Some(rect) = redraw_local_rect {
                        self.erase_dialog_item_enclosing_rect(bus, dialog_ptr, rect);
                        self.invalidate_window_rect(bus, dialog_ptr, rect);
                    }
                    if let Some(rect) = updated_control_rect {
                        if let Some(ctrl_handle) =
                            self.dialog_control_handle_for_item(dialog_ptr, item_no)
                        {
                            let ctrl_ptr = bus.read_long(ctrl_handle);
                            if ctrl_ptr != 0 {
                                bus.write_word(ctrl_ptr + 8, rect.0 as u16);
                                bus.write_word(ctrl_ptr + 10, rect.1 as u16);
                                bus.write_word(ctrl_ptr + 12, rect.2 as u16);
                                bus.write_word(ctrl_ptr + 14, rect.3 as u16);
                            }
                        }
                        if let Some(tracking) = self.dialog_tracking.as_mut() {
                            if tracking.dialog_ptr == dialog_ptr {
                                let idx = (item_no as usize).wrapping_sub(1);
                                if idx < tracking.items.len() {
                                    tracking.items[idx].rect = rect;
                                }
                            }
                        }
                    }
                }
                Ok(())
            }

            // ShowDialogItem ($A828)
            // Restores the rect saved by HideDialogItem and invalidates
            // the new rect so the dialog redraws the item.
            // PROCEDURE ShowDialogItem(theDialog: DialogPtr; itemNo: INTEGER);
            // Inside Macintosh Volume IV, IV-59
            // ShowDialogItem ($A828): If itemRect.left > 8192, restores visibility by
            // subtracting 16384 from itemRect.left/right; already-visible items are unchanged
            // (MTE 1992, 6-124). Pops 6 bytes per IM:IV IV-59.
            (true, 0x028) => {
                let sp = cpu.read_reg(Register::A7);
                let item_no = bus.read_word(sp) as i16;
                let dialog_ptr = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);
                let mut updated_control_rect = None;
                let mut redraw_local_rect = None;
                if dialog_ptr != 0 && item_no > 0 {
                    let key = (dialog_ptr, item_no);
                    if let Some(items) = self.dialog_items.get_mut(&dialog_ptr) {
                        let idx = (item_no as usize).wrapping_sub(1);
                        if idx < items.len() {
                            let rect = items[idx].rect;
                            // MTE 1992, 6-124: already-visible items (left < 8192) are a no-op.
                            if rect.1 > 8192 {
                                let restored_rect = if let Some(orig_rect) =
                                    self.hidden_dialog_item_rects.remove(&key)
                                {
                                    orig_rect
                                } else {
                                    (
                                        rect.0,
                                        rect.1.wrapping_sub(16384),
                                        rect.2,
                                        rect.3.wrapping_sub(16384),
                                    )
                                };
                                let enclosing = Self::dialog_item_enclosing_local_rect(
                                    items[idx].item_type,
                                    restored_rect,
                                );
                                items[idx].rect = restored_rect;
                                updated_control_rect = Some(restored_rect);
                                redraw_local_rect = Some(enclosing);
                            }
                        }
                    }
                    if let Some(rect) = redraw_local_rect {
                        self.invalidate_window_rect(bus, dialog_ptr, rect);
                    }
                    if let Some(rect) = updated_control_rect {
                        if let Some(ctrl_handle) =
                            self.dialog_control_handle_for_item(dialog_ptr, item_no)
                        {
                            let ctrl_ptr = bus.read_long(ctrl_handle);
                            if ctrl_ptr != 0 {
                                bus.write_word(ctrl_ptr + 8, rect.0 as u16);
                                bus.write_word(ctrl_ptr + 10, rect.1 as u16);
                                bus.write_word(ctrl_ptr + 12, rect.2 as u16);
                                bus.write_word(ctrl_ptr + 14, rect.3 as u16);
                            }
                        }
                        if let Some(tracking) = self.dialog_tracking.as_mut() {
                            if tracking.dialog_ptr == dialog_ptr {
                                let idx = (item_no as usize).wrapping_sub(1);
                                if idx < tracking.items.len() {
                                    tracking.items[idx].rect = rect;
                                }
                            }
                        }
                    }
                }
                Ok(())
            }

            // FindDItem ($A984)
            // Returns the 0-indexed item number of the item containing thePt
            // (in coordinates local to the dialog box), or –1 if no item
            // contains the point. Disabled items are returned per IM:IV-60;
            // hidden items naturally fail the rect-contains check because
            // HideDialogItem moves their rect to (16384, 16384, 16385, 16385).
            // Overlapping items resolve to the first matching item in DITL
            // order (Macintosh Toolbox Essentials 1992, 6-125).
            // FUNCTION FindDItem(theDialog: DialogPtr; thePt: Point): INTEGER;
            // Inside Macintosh Volume IV, IV-60; Macintosh Toolbox Essentials 1992, 6-125
            //
            // Stack layout (Pascal — args pushed left-to-right, result slot
            // pre-pushed by caller):
            //   SP+0..3:  thePt (Point — v at +0..1, h at +2..3)
            //   SP+4..7:  theDialog (DialogPtr)
            //   SP+8..9:  result slot (INTEGER)
            //
            // FindDItem ($A984): Walks dialog_items in order; first item whose
            // local rect contains thePt wins; returns 0-indexed item number or
            // -1 per Macintosh Toolbox Essentials 1992, p. 6-125.
            (true, 0x184) => {
                let sp = cpu.read_reg(Register::A7);
                let pt_v = bus.read_word(sp) as i16;
                let pt_h = bus.read_word(sp + 2) as i16;
                let dialog_ptr = bus.read_long(sp + 4);
                let result: i16 = if dialog_ptr == 0 {
                    -1
                } else if let Some(items) = self.dialog_items.get(&dialog_ptr) {
                    let mut hit: i16 = -1;
                    for (idx, item) in items.iter().enumerate() {
                        let (top, left, bottom, right) = item.rect;
                        if pt_v >= top && pt_v < bottom && pt_h >= left && pt_h < right {
                            hit = idx as i16;
                            break;
                        }
                    }
                    hit
                } else {
                    -1
                };
                bus.write_word(sp + 8, result as u16);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // CouldAlert ($A989)
            // PROCEDURE CouldAlert(alertID: INTEGER);
            // Inside Macintosh Volume I, I-420
            //
            // FreeAlert ($A98A)
            // PROCEDURE FreeAlert(alertID: INTEGER);
            // Inside Macintosh Volume I, I-420
            //
            // Companion of CouldDialog/FreeDialog ($A979/$A97A) for ALRT
            // templates. Per IM:I I-420, CouldAlert loads and makes the
            // ALRT, DITL, and resource-backed DITL items unpurgeable;
            // FreeAlert marks the already-loaded equivalents purgeable.
            // BasiliskII treats missing alert IDs as harmless no-ops and
            // leaves ResErr at noErr.
            (true, 0x189) | (true, 0x18A) => {
                let sp = cpu.read_reg(Register::A7);
                let alert_id = bus.read_word(sp) as i16;
                let load_if_missing = trap_num == 0x189;
                let purgeable = trap_num == 0x18A;
                self.cascade_dialog_resource_purgeability(
                    bus,
                    *b"ALRT",
                    alert_id,
                    purgeable,
                    load_if_missing,
                );
                bus.write_word(0x0A60, 0);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // ErrorSound ($A98C)
            // Sets the error-sound procedure for alerts.
            // PROCEDURE ErrorSound(soundProc: ProcPtr);
            // Inside Macintosh Volume I, I-411
            //
            // Per IM:I I-411: "The address of the sound procedure
            // being used is stored in the global variable DABeeper."
            // ErrorSound replaces the Dialog Manager's standard
            // sound procedure (installed by InitDialogs $A97B) with
            // the caller's `soundProc`. NIL is documented as "no
            // sound (or menu bar blinking) at all" — same encoding
            // as a NIL InitDialogs default.
            //
            // HLE compromise: Systemless doesn't invoke DABeeper from
            // any Alert family path (no menu-bar-blink emulation),
            // so this trap's only observable side effect is the
            // DABeeper global itself. Apps that probe DABeeper
            // before/after ErrorSound to detect "did the previous
            // app's sound proc get installed?" still see the
            // correct value. Future iterations that wire up
            // sound-proc invocation from Alert dispatch will pick
            // up the stored ProcPtr automatically.
            //
            // Pop = 4 bytes (soundProc ProcPtr).
            // ErrorSound ($A98C): Per IM:I I-411 stores soundProc at $0A9C (DABeeper global); NIL is the documented "no sound + no menu-bar-blink" default. Mirrors InitDialogs's "install standard sound procedure" step. HLE does not invoke DABeeper from Alert dispatch (no menu-bar-blink emulation), so this is state-only.
            (true, 0x18C) => {
                let sp = cpu.read_reg(Register::A7);
                let sound_proc = bus.read_long(sp);
                bus.write_long(crate::memory::globals::addr::DA_BEEPER, sound_proc);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetDAFont ($A98D variant — actually uses DlgFont low-mem global)
            // Sets the font for subsequently created dialogs and alerts.
            // PROCEDURE SetDAFont(fontNum: INTEGER); [Not in ROM]
            // Inside Macintosh Volume I, I-412
            // Assembly-language: sets DlgFont ($0AFA) directly.
            //
            // Note: $A98D is GetDItem; SetDAFont is not a ROM trap but a
            // glue routine that writes DlgFont. We handle it via the
            // dedicated trap word $A97F (Pack13 slot repurposed).
            // If apps call it directly, they just write DlgFont.

            // SetDialogItemText ($A98F)
            // Sets the text of a dialog item (statText or editText).
            // PROCEDURE SetDialogItemText(item: Handle; text: Str255);
            // Inside Macintosh Volume I, I-422; Macintosh Toolbox Essentials 1992, 6-131
            (true, 0x18F) => {
                let sp = cpu.read_reg(Register::A7);
                let pc = cpu.read_reg(Register::PC);
                let text_str_ptr = bus.read_long(sp);
                let item_handle = bus.read_long(sp + 4);
                let mut redraw_text_item = None;
                if text_str_ptr != 0 {
                    let bytes = bus.read_pstring(text_str_ptr);
                    let len = bytes.len();
                    let text = decode_mac_roman(&bytes);

                    if item_handle != 0 {
                        let data_ptr = Self::ensure_text_handle_size(bus, item_handle, len);
                        if data_ptr != 0 {
                            bus.write_bytes(data_ptr, &bytes);
                        }
                    }

                    // Also update the item in dialog_items so ModalDialog picks it up
                    if let Some((dlg_ptr, idx)) =
                        self.dialog_item_handles.get(&item_handle).copied()
                    {
                        let item_no = (idx + 1) as i16;
                        if trace_dialog_items_enabled() {
                            eprintln!(
                                "[DIALOG-ITEM] SetDialogItemText pc=${:08X} dialog=${:08X} item={} handle=${:08X} text={:?}",
                                pc,
                                dlg_ptr,
                                item_no,
                                item_handle,
                                text
                            );
                        }
                        if let Some(items) = self.dialog_items.get_mut(&dlg_ptr) {
                            if idx < items.len() {
                                items[idx].text = text.clone();
                            }
                        }
                        let mut refresh_tracking = false;
                        if let Some(tracking) = self.dialog_tracking.as_mut() {
                            if tracking.dialog_ptr == dlg_ptr && idx < tracking.items.len() {
                                tracking.items[idx].text = text.clone();
                                if tracking.edit_item == item_no {
                                    tracking.edit_text = text;
                                    tracking.edit_text_modified = false;
                                }
                                refresh_tracking = !tracking.game_managed;
                            }
                        }
                        if refresh_tracking {
                            self.refresh_dialog_tracking_snapshot(bus);
                        } else {
                            redraw_text_item = Some((dlg_ptr, item_no));
                        }
                    } else if trace_dialog_items_enabled() {
                        eprintln!(
                            "[DIALOG-ITEM] SetDialogItemText pc=${:08X} dialog=<unknown> handle=${:08X} text={:?}",
                            pc,
                            item_handle,
                            text
                        );
                    }
                }

                if let Some((dlg_ptr, item_no)) = redraw_text_item {
                    self.redraw_dialog_text_item(bus, dlg_ptr, item_no);
                }

                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GetDialogItemText ($A990)
            // Returns the text of a dialog item (statText or editText).
            // PROCEDURE GetDialogItemText(item: Handle; VAR text: Str255);
            // Inside Macintosh Volume I, I-422
            (true, 0x190) => {
                let sp = cpu.read_reg(Register::A7);
                let text_ptr = bus.read_long(sp);
                let item_handle = bus.read_long(sp + 4);

                if text_ptr != 0 {
                    // If dialog tracking is active, return the current edit text
                    let mut wrote = false;
                    if let Some(ref tracking) = self.dialog_tracking {
                        let current_edit_handle =
                            self.dialog_item_handles.get(&item_handle).copied().filter(
                                |(dlg_ptr, idx)| {
                                    *dlg_ptr == tracking.dialog_ptr
                                        && (*idx as i16 + 1) == tracking.edit_item
                                },
                            );
                        if current_edit_handle.is_some() {
                            let bytes = encode_mac_roman_lossy(&tracking.edit_text);
                            let len = bytes.len().min(255);
                            bus.write_byte(text_ptr, len as u8);
                            for (i, byte) in bytes.iter().take(len).enumerate() {
                                bus.write_byte(text_ptr + 1 + i as u32, *byte);
                            }
                            wrote = true;
                        }
                    }
                    if !wrote {
                        // Text item handles store raw bytes, not a Pascal-length byte.
                        // Inside Macintosh Volume I, I-422; Executor dialManip.cpp
                        if item_handle != 0 {
                            let master = bus.read_long(item_handle);
                            if master != 0 {
                                let len = bus.get_alloc_size(master).unwrap_or(0).min(255) as usize;
                                bus.write_byte(text_ptr, len as u8);
                                for i in 0..len {
                                    bus.write_byte(
                                        text_ptr + 1 + i as u32,
                                        bus.read_byte(master + i as u32),
                                    );
                                }
                                wrote = true;
                            }
                        }
                        if !wrote {
                            bus.write_byte(text_ptr, 0);
                        }
                    }
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // DialogDispatch ($AA68)
            // Selector-based dispatch for extended Dialog Manager routines.
            // Selector is in D0 (not on stack) per the MPW THREEWORDINLINE
            // sequence: MOVE.W #selector, D0 / _DialogDispatch. The low
            // byte is the routine number; the high byte encodes param-
            // bytes / 2 which the dispatcher uses to pop args.
            // Macintosh Toolbox Essentials 1992, 6-162
            // DialogDispatch ($AA68): Selector-based per MTb 1992 6-162..6-167: $03 GetStdFilterProc returns a guest-callable standard-filter shim ProcPtr so apps using the common `GetStdFilterProc(&p); ModalDialog(p, &itemHit);` pattern get a safe non-NIL proc instead of Systemless's old NIL compromise; the shim itself declines events (FALSE) so Systemless's ModalDialog default Return/Escape handling still runs. $04 SetDialogDefaultItem writes newItem to DialogRecord.aDefItem at offset 168 + mirrors into DialogTrackingState.default_item if dialog is being tracked; $05 SetDialogCancelItem stores newItem in DialogTrackingState.cancel_item (no canonical DialogRecord field — System 7 addition); $06 SetDialogTracksCursor is a no-op noErr (HLE has no interactive cursor tracking); $0C NewFeaturesDialog allocates an Appearance-flavored DialogRecord via NewCDialog delegation. All return noErr; pop = 2 (selector encoded in D0) + arg_words*2 from the (arg_words << 8) | routine D0 encoding.
            (true, 0x268) => {
                let sp = cpu.read_reg(Register::A7);
                let d0 = cpu.read_reg(Register::D0) as u16;
                let param_bytes = (((d0 >> 8) & 0xFF) as u32) * 2;
                let routine = d0 & 0xFF;
                match routine {
                    // NewFeaturesDialog (selector $0C, param_bytes=34)
                    // FUNCTION NewFeaturesDialog(inStorage: Ptr;
                    //     inBoundsRect: Rect; inTitle: ConstStr255Param;
                    //     inIsVisible: Boolean; inProcID: SInt16;
                    //     inBehind: WindowPtr; inGoAwayFlag: Boolean;
                    //     inRefCon: SInt32; inItems: Handle;
                    //     inFeatures: UInt32): DialogPtr;
                    // Mac Toolbox: Appearance Manager (Apple, 1997).
                    //
                    // Same shape as NewCDialog with one extra LongInt
                    // for Appearance feature flags. Mid-90s titles
                    // (e.g. Meteor Storm) call this unconditionally —
                    // even when Gestalt('appr') reports no features —
                    // to allocate their main dialog. Delegate to the
                    // existing NewCDialog path so the dialog is created
                    // without the Appearance theming, then drop the
                    // unused inFeatures word.
                    0x0C => {
                        // Stack (low to high), 34 bytes of params + 4 result:
                        //   SP+0:  inFeatures(4)
                        //   SP+4:  inItems(4)
                        //   SP+8:  inRefCon(4)
                        //   SP+12: inGoAwayFlag(2)
                        //   SP+14: inBehind(4)
                        //   SP+18: inProcID(2)
                        //   SP+20: inIsVisible(2)
                        //   SP+22: inTitle(4)
                        //   SP+26: inBoundsRect(4)
                        //   SP+30: inStorage(4)
                        //   SP+34: result DialogPtr(4)
                        let _features = bus.read_long(sp);
                        let items_handle = bus.read_long(sp + 4);
                        let ref_con = bus.read_long(sp + 8);
                        let go_away_flag = bus.read_byte(sp + 12) != 0;
                        let behind = bus.read_long(sp + 14);
                        let proc_id = bus.read_word(sp + 18) as i16;
                        let visible = bus.read_byte(sp + 20) != 0;
                        let title_ptr = bus.read_long(sp + 22);
                        let bounds_rect_ptr = bus.read_long(sp + 26);
                        let storage_ptr = bus.read_long(sp + 30);

                        let bounds = if bounds_rect_ptr != 0 {
                            (
                                bus.read_word(bounds_rect_ptr) as i16,
                                bus.read_word(bounds_rect_ptr + 2) as i16,
                                bus.read_word(bounds_rect_ptr + 4) as i16,
                                bus.read_word(bounds_rect_ptr + 6) as i16,
                            )
                        } else {
                            (0, 0, 0, 0)
                        };
                        let title = if title_ptr != 0 {
                            decode_mac_roman(&bus.read_pstring(title_ptr))
                        } else {
                            String::new()
                        };
                        let items_ptr = if items_handle != 0 {
                            bus.read_long(items_handle)
                        } else {
                            0
                        };
                        let items_len = if items_ptr != 0 {
                            bus.get_alloc_size(items_ptr).unwrap_or(0)
                        } else {
                            0
                        };
                        let items = if items_ptr != 0 && items_len != 0 {
                            Self::parse_ditl(bus, items_ptr, items_len)
                        } else {
                            Vec::new()
                        };

                        let dlg_ptr = self.finish_dialog_creation(
                            bus,
                            cpu,
                            storage_ptr,
                            bounds,
                            &title,
                            visible,
                            proc_id,
                            go_away_flag,
                            ref_con,
                            items_handle,
                            items,
                            None,
                            None,
                        );
                        self.apply_behind_parameter(bus, dlg_ptr, behind);
                        bus.write_long(sp + param_bytes, dlg_ptr);
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }
                    // GetStdFilterProc (selector 3, param_bytes=4)
                    // FUNCTION GetStdFilterProc(VAR theProc: ProcPtr): OSErr;
                    // Macintosh Toolbox Essentials 1992, 6-163
                    //
                    // Returns a ProcPtr to the system's standard
                    // event filter for modal dialogs. The public
                    // Dialogs.h / Dialogs.p declarations expose
                    // GetStdFilterProc as returning a ModalFilterUPP
                    // through a VAR/out pointer. MTb 1992 documents
                    // that NIL filterProc passed to ModalDialog uses
                    // the standard filter, and BasiliskII returns a
                    // non-NIL callable proc here.
                    //
                    // HLE compromise: return a tiny guest-resident
                    // Pascal FUNCTION shim that always returns FALSE:
                    //
                    //   JMP    shimBody ; recognizable callback entry for
                    //                    runner-side filter-proc firing
                    //   MOVEQ  #0,D0    ; Boolean result = FALSE in D0 too
                    //   CLR.W  16(SP)   ; Boolean result = FALSE
                    //   RTD    #12      ; pop dialog/event/itemHit args
                    //
                    // This makes the proc SAFE and callable for guest
                    // code (better than the older NIL placeholder) while
                    // still preserving Systemless's own ModalDialog default
                    // handling for Return/Escape when apps pass the proc
                    // straight back to ModalDialog.
                    //
                    // NIL VAR ptr (the caller passed NULL for theProc)
                    // is a defensive no-op — the impl skips the write
                    // rather than dereffing NIL.
                    0x03 => {
                        let proc_ptr = bus.read_long(sp);
                        let shim = if self.dialog_std_filter_proc != 0 {
                            self.dialog_std_filter_proc
                        } else {
                            let shim = bus.alloc(16);
                            bus.write_word(shim, 0x4EF9); // JMP abs.L shimBody
                            bus.write_long(shim + 2, shim + 6);
                            // Pascal FUNCTION ModalFilterProc(dialog, event, itemHit): Boolean
                            // entry stack layout:
                            //   +0  return address
                            //   +4  itemHit ptr
                            //   +8  EventRecord ptr
                            //   +12 DialogPtr
                            //   +16 Boolean result slot
                            bus.write_word(shim + 6, 0x7000); // MOVEQ #0, D0
                            bus.write_word(shim + 8, 0x426F); // CLR.W 16(SP)
                            bus.write_word(shim + 10, 0x0010);
                            bus.write_word(shim + 12, 0x4E74); // RTD #12
                            bus.write_word(shim + 14, 0x000C);
                            self.dialog_std_filter_proc = shim;
                            shim
                        };
                        if proc_ptr != 0 {
                            bus.write_long(proc_ptr, shim);
                        }
                        // pop params; leave result in place
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }
                    // SetDialogDefaultItem (selector 4, param_bytes=6)
                    // FUNCTION SetDialogDefaultItem(theDialog: DialogPtr;
                    //     newItem: INTEGER): OSErr;
                    // Macintosh Toolbox Essentials 1992, 6-164
                    //
                    // Stack: SP+0=newItem(2), SP+2=theDialog(4),
                    //        SP+6=result OSErr slot (caller pre-pushed).
                    //
                    // Marks `newItem` as the dialog's default
                    // button — pressed when user hits Return per
                    // IM:MTb 6-164. Stored in DialogRecord.aDefItem
                    // at offset 168 so subsequent ModalDialog
                    // redraws can outline the default button. If
                    // dialog tracking is active for this dialog,
                    // mirror the value into DialogTrackingState
                    // .default_item so the active redraw path
                    // sees it without re-reading guest memory.
                    0x04 => {
                        let new_item = bus.read_word(sp) as i16;
                        let dialog_ptr = bus.read_long(sp + 2);
                        if dialog_ptr != 0 {
                            // DialogRecord.aDefItem is at offset 168
                            // per the existing GetNewDialog impl
                            // (dialog.rs:1961 writes "aDefItem" here).
                            bus.write_word(dialog_ptr + 168, new_item as u16);
                            // Mirror into active tracking state if
                            // this dialog is currently being tracked.
                            if let Some(tracking) = self.dialog_tracking.as_mut() {
                                if tracking.dialog_ptr == dialog_ptr {
                                    tracking.default_item = new_item;
                                }
                            }
                        }
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }
                    // SetDialogCancelItem (selector 5, param_bytes=6)
                    // FUNCTION SetDialogCancelItem(theDialog: DialogPtr;
                    //     newItem: INTEGER): OSErr;
                    // Macintosh Toolbox Essentials 1992, 6-165
                    //
                    // System 7 addition (no canonical DialogRecord
                    // field — Apple added cancel-item tracking to
                    // the Dialog Manager AFTER the original
                    // DialogRecord layout was finalized). Stored in
                    // a host-side per-dialog map and mirrored into
                    // active tracking state when ModalDialog is
                    // already running. NIL theDialog is a defensive
                    // no-op.
                    0x05 => {
                        let new_item = bus.read_word(sp) as i16;
                        let dialog_ptr = bus.read_long(sp + 2);
                        if dialog_ptr != 0 {
                            self.dialog_cancel_items.insert(dialog_ptr, new_item);
                            if let Some(tracking) = self.dialog_tracking.as_mut() {
                                if tracking.dialog_ptr == dialog_ptr {
                                    tracking.cancel_item = new_item;
                                }
                            }
                        }
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }
                    // SetDialogTracksCursor (selector 6, param_bytes=6)
                    // FUNCTION SetDialogTracksCursor(theDialog: DialogPtr;
                    //     tracks: BOOLEAN): OSErr;
                    // Macintosh Toolbox Essentials 1992, 6-166
                    //
                    // System 7 addition that controls whether the
                    // Dialog Manager auto-changes the cursor (to
                    // an I-beam) when the mouse is over an
                    // editText item. HLE compromise: Systemless
                    // doesn't model interactive cursor tracking
                    // (no continuous mouse-poll stream from the
                    // scripted event source), so the trap is a
                    // no-op noErr. Apps that defensively call this
                    // at dialog setup time get noErr and proceed.
                    0x06 => {
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }
                    _ => {
                        eprintln!(
                            "[TRAP] DialogDispatch unknown routine=${:02X} d0=${:04X}",
                            routine, d0
                        );
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }
                }
                Ok(())
            }

            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests;
