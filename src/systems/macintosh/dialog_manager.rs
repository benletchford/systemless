//! Architecture-neutral Dialog Manager records, DITL parsing, and geometry.
//!
//! Inside Macintosh Volume I (1985), pp. I-399--I-434, and
//! Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-1--6-179.
use std::borrow::Cow;

use crate::trap::types::decode_mac_roman;

/// Canonical guest DialogRecord byte size.
pub const DIALOG_RECORD_SIZE: u32 = 256;

/// Canonical DialogRecord guest field offsets.
pub const DIALOG_WINDOW_KIND_OFFSET: u32 = 108;
pub const DIALOG_GO_AWAY_FLAG_OFFSET: u32 = 112;
pub const DIALOG_ITEMS_OFFSET: u32 = 156;
pub const DIALOG_TEXT_HANDLE_OFFSET: u32 = 160;
pub const DIALOG_EDIT_FIELD_OFFSET: u32 = 164;
pub const DIALOG_EDIT_OPEN_OFFSET: u32 = 166;
pub const DIALOG_DEFAULT_ITEM_OFFSET: u32 = 168;
pub const DIALOG_RESOURCE_ID_OFFSET: u32 = 170;

/// Host-private Dialog Manager state offsets following the documented DialogRecord.
pub const DIALOG_CANCEL_ITEM_OFFSET: u32 = 172;
pub const DIALOG_ALERT_HIT_OFFSET: u32 = 174;
pub const DIALOG_STANDARD_ALERT_OUTPUT_OFFSET: u32 = 176;
pub const DIALOG_STANDARD_ALERT_STACK_OFFSET: u32 = 180;

/// Canonical Dialog window kind. Inside Macintosh Volume I, p. I-273.
pub const DIALOG_WINDOW_KIND: u16 = 2;

/// Canonical DialogDispatch ($AA68) routine selectors.
/// Macintosh Toolbox Essentials (1992), pp. 6-162--6-167.
#[allow(dead_code)]
pub const DIALOG_DISPATCH_NEW_COLOR_DIALOG: u16 = 0x0000;
pub const DIALOG_DISPATCH_GET_STD_FILTER_PROC: u16 = 0x0003;
pub const DIALOG_DISPATCH_SET_DIALOG_DEFAULT_ITEM: u16 = 0x0004;
pub const DIALOG_DISPATCH_SET_DIALOG_CANCEL_ITEM: u16 = 0x0005;
pub const DIALOG_DISPATCH_SET_DIALOG_TRACKS_CURSOR: u16 = 0x0006;
pub const DIALOG_DISPATCH_NEW_FEATURES_DIALOG: u16 = 0x000C;

/// Standard default button outline thickness in pixels.
/// Macintosh Toolbox Essentials (1992), Listing 6-17.
pub const DEFAULT_BUTTON_OUTLINE_THICKNESS: i16 = 3;

/// Standard default button outline outset padding in pixels.
/// Macintosh Toolbox Essentials (1992), Listing 6-17.
pub const DEFAULT_BUTTON_OUTLINE_INSET: i16 = 4;

/// Standard margin around dialog content for a modal dialog box (`dBoxProc`).
/// Inside Macintosh Volume I, p. I-273.
pub const DIALOG_DBOX_FRAME_MARGIN: i16 = 8;

/// Standard outset around an active edit-text field for focus/border frame rendering.
/// Inside Macintosh Volume I, p. I-414; Macintosh Human Interface Guidelines (1992), p. 184.
pub const EDIT_TEXT_FRAME_OUTSET: i16 = 3;

/// Standard flush-left text glyph inset from destination rectangle leading edge.
/// Inside Macintosh Volume I, pp. I-373--I-374.
pub const DIALOG_TEXT_LEFT_INSET: i16 = 1;

/// Canonical AppendDITL placement methods.
/// Macintosh Toolbox Essentials (1992), pp. 6-108, 6-153.
pub const APPEND_DITL_OVERLAY: i16 = 0;
pub const APPEND_DITL_RIGHT: i16 = 1;
pub const APPEND_DITL_BOTTOM: i16 = 2;

/// Item disable bit in the DITL item type byte.
pub const DIALOG_ITEM_DISABLED_FLAG: u8 = 0x80;

/// Canonical Dialog item type codes. Inside Macintosh Volume I, p. I-427.
pub const DIALOG_ITEM_USER_ITEM: u8 = 0;
pub const DIALOG_ITEM_BUTTON: u8 = 4;
pub const DIALOG_ITEM_CHECKBOX: u8 = 5;
pub const DIALOG_ITEM_RADIO: u8 = 6;
pub const DIALOG_ITEM_RESOURCE_CONTROL: u8 = 7;
pub const DIALOG_ITEM_STATIC_TEXT: u8 = 8;
pub const DIALOG_ITEM_EDIT_TEXT: u8 = 16;
pub const DIALOG_ITEM_ICON: u8 = 32;
pub const DIALOG_ITEM_PICTURE: u8 = 64;

/// Canonical Alert types. Inside Macintosh Volume I, p. I-417, and Appearance.h.
pub const ALERT_TYPE_STOP: u16 = 0;
pub const ALERT_TYPE_NOTE: u16 = 1;
pub const ALERT_TYPE_CAUTION: u16 = 2;
#[allow(dead_code)]
pub const ALERT_TYPE_PLAIN: u16 = 3;

/// Canonical Standard Alert button IDs. Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 6-110.
#[allow(dead_code)]
pub const ALERT_BUTTON_OK: i16 = 1;
#[allow(dead_code)]
pub const ALERT_BUTTON_CANCEL: i16 = 2;
#[allow(dead_code)]
pub const ALERT_BUTTON_OTHER: i16 = 3;
#[allow(dead_code)]
pub const ALERT_BUTTON_HELP: i16 = 4;

/// Strongly typed Dialog Manager item kind.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogItemKind {
    UserItem,
    Button,
    Checkbox,
    RadioButton,
    ResourceControl,
    StaticText,
    EditText,
    Icon,
    Picture,
    Unknown(u8),
}

#[allow(dead_code)]
impl DialogItemKind {
    /// Decode the base item kind, masking out the disabled flag bit (0x80).
    pub const fn from_raw_type(raw: u8) -> Self {
        match raw & 0x7F {
            DIALOG_ITEM_USER_ITEM => Self::UserItem,
            DIALOG_ITEM_BUTTON => Self::Button,
            DIALOG_ITEM_CHECKBOX => Self::Checkbox,
            DIALOG_ITEM_RADIO => Self::RadioButton,
            DIALOG_ITEM_RESOURCE_CONTROL => Self::ResourceControl,
            DIALOG_ITEM_STATIC_TEXT => Self::StaticText,
            DIALOG_ITEM_EDIT_TEXT => Self::EditText,
            DIALOG_ITEM_ICON => Self::Icon,
            DIALOG_ITEM_PICTURE => Self::Picture,
            other => Self::Unknown(other),
        }
    }

    /// Return the raw integer code for this item kind.
    pub const fn raw_base_type(self) -> u8 {
        match self {
            Self::UserItem => DIALOG_ITEM_USER_ITEM,
            Self::Button => DIALOG_ITEM_BUTTON,
            Self::Checkbox => DIALOG_ITEM_CHECKBOX,
            Self::RadioButton => DIALOG_ITEM_RADIO,
            Self::ResourceControl => DIALOG_ITEM_RESOURCE_CONTROL,
            Self::StaticText => DIALOG_ITEM_STATIC_TEXT,
            Self::EditText => DIALOG_ITEM_EDIT_TEXT,
            Self::Icon => DIALOG_ITEM_ICON,
            Self::Picture => DIALOG_ITEM_PICTURE,
            Self::Unknown(raw) => raw,
        }
    }

    /// Whether this item represents a clickable button or standard control.
    pub const fn is_control(self) -> bool {
        matches!(
            self,
            Self::Button | Self::Checkbox | Self::RadioButton | Self::ResourceControl
        )
    }

    /// Whether this item contains text payload (static or editable).
    pub const fn is_text(self) -> bool {
        matches!(self, Self::StaticText | Self::EditText)
    }
}

/// Parsed Dialog Item List (DITL) entry.
///
/// Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-120--6-121.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogItemRecord {
    /// Byte offset of the item entry within the source DITL data.
    pub item_offset: usize,
    /// Raw item type byte (including `DIALOG_ITEM_DISABLED_FLAG`).
    pub item_type: u8,
    /// Display rectangle in dialog-local coordinates (top, left, bottom, right).
    pub rect: (i16, i16, i16, i16),
    /// Handle or procedure pointer from the item header (0 in resource data).
    pub handle: u32,
    /// Raw item payload bytes (Pascal string, resource ID, or application data).
    pub payload: Vec<u8>,
}

#[allow(dead_code)]
impl DialogItemRecord {
    /// Whether the item is enabled for user interaction.
    pub fn is_enabled(&self) -> bool {
        self.item_type & DIALOG_ITEM_DISABLED_FLAG == 0
    }

    /// Strongly typed item kind.
    pub fn kind(&self) -> DialogItemKind {
        DialogItemKind::from_raw_type(self.item_type)
    }

    /// Whether the item represents a button, checkbox, or radio control.
    pub fn is_control(&self) -> bool {
        is_dialog_item_control(self.item_type)
    }

    /// Whether the item represents static or editable text.
    pub fn is_text(&self) -> bool {
        is_dialog_item_text(self.item_type)
    }

    /// Decoded Mac Roman text if the payload represents a Pascal string.
    pub fn text(&self) -> String {
        decode_mac_roman(&self.payload)
    }

    /// 16-bit resource ID if the item references a resource (ICON, PICT, CNTL).
    pub fn resource_id(&self) -> Option<i16> {
        if self.payload.len() >= 2 {
            Some(i16::from_be_bytes([self.payload[0], self.payload[1]]))
        } else {
            None
        }
    }

    /// Whether the item rectangle is moved off-screen via `HideDialogItem`.
    pub fn is_hidden(&self) -> bool {
        is_dialog_item_rect_hidden(self.rect)
    }

    /// Offset the item display rectangle by `(v_delta, h_delta)`.
    pub fn offset_rect(&mut self, v_delta: i16, h_delta: i16) {
        self.rect = offset_rect(self.rect, v_delta, h_delta);
    }

    /// Calculate the enclosing rectangle for invalidation and redrawing.
    pub fn enclosing_rect(&self) -> (i16, i16, i16, i16) {
        dialog_item_enclosing_rect(self.item_type, self.rect)
    }
}

#[inline]
fn read_be_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(i16::from_be_bytes([slice[0], slice[1]]))
}

#[inline]
fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Calculate the payload length for a DITL item.
///
/// Inside Macintosh Volume I, pp. I-404--I-405, I-427;
/// Macintosh Toolbox Essentials (1992), p. 6-153.
pub fn ditl_item_payload_len(base_type: u8, data_len_byte: u8, remaining: usize) -> Option<usize> {
    match base_type {
        // icon (32), picture (64), resCtrl (7): 2-byte resource ID.
        // IM:I I-427 describes the byte after itmtype as length=2; MTE 1992
        // p. 6-153 documents the same compiled records as a reserved byte plus
        // the two-byte resource ID. Accept both conventions.
        DIALOG_ITEM_RESOURCE_CONTROL | DIALOG_ITEM_ICON | DIALOG_ITEM_PICTURE => {
            if remaining < 2 {
                return None;
            }
            if data_len_byte >= 2 {
                Some(usize::from(data_len_byte))
            } else {
                Some(2)
            }
        }
        _ => Some(usize::from(data_len_byte)),
    }
}

/// Calculate the total byte length of a compiled DITL item record at `record_offset`,
/// including its 14-byte header and padded payload.
pub fn ditl_item_record_len_at(bytes: &[u8], record_offset: usize) -> Option<usize> {
    if record_offset.checked_add(14)? > bytes.len() {
        return None;
    }
    let item_type = *bytes.get(record_offset.checked_add(12)?)?;
    let data_len_byte = *bytes.get(record_offset.checked_add(13)?)?;
    let base_type = item_type & !DIALOG_ITEM_DISABLED_FLAG;
    let payload_offset = record_offset.checked_add(14)?;
    let remaining = bytes.len().saturating_sub(payload_offset);
    let payload_len = ditl_item_payload_len(base_type, data_len_byte, remaining)?;
    let padded = (payload_len + 1) & !1;
    if padded > remaining {
        return None;
    }
    Some(14 + padded)
}

/// Calculate the total used bytes of compiled DITL data up to the last item.
pub fn ditl_total_used_len(bytes: &[u8]) -> usize {
    if bytes.len() < 2 {
        return bytes.len();
    }
    let Some(count_minus_one) = read_be_i16(bytes, 0) else {
        return bytes.len();
    };
    let count = if count_minus_one < 0 {
        0
    } else {
        match usize::try_from(count_minus_one)
            .ok()
            .and_then(|c| c.checked_add(1))
        {
            Some(c) => c,
            None => return bytes.len(),
        }
    };
    let mut offset = 2usize;
    for _ in 0..count {
        let Some(record_len) = ditl_item_record_len_at(bytes, offset) else {
            break;
        };
        offset = offset.saturating_add(record_len);
    }
    offset
}

/// Parse raw DITL resource bytes into a list of parsed dialog item records.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-120--6-121.
pub fn parse_ditl_items(bytes: &[u8]) -> Option<Vec<DialogItemRecord>> {
    let count_minus_one = read_be_i16(bytes, 0)?;
    let count = if count_minus_one < 0 {
        0
    } else {
        match usize::try_from(count_minus_one)
            .ok()
            .and_then(|c| c.checked_add(1))
        {
            Some(c) => c,
            None => return None,
        }
    };

    let mut offset = 2usize;
    let mut items = Vec::with_capacity(count);

    for _ in 0..count {
        if offset + 14 > bytes.len() {
            break;
        }
        let Some(handle) = read_be_u32(bytes, offset) else {
            break;
        };
        let Some(top) = read_be_i16(bytes, offset + 4) else {
            break;
        };
        let Some(left) = read_be_i16(bytes, offset + 6) else {
            break;
        };
        let Some(bottom) = read_be_i16(bytes, offset + 8) else {
            break;
        };
        let Some(right) = read_be_i16(bytes, offset + 10) else {
            break;
        };
        let item_type = bytes[offset + 12];
        let data_len_byte = bytes[offset + 13];
        let base_type = item_type & !DIALOG_ITEM_DISABLED_FLAG;
        let payload_start = offset + 14;
        let remaining = bytes.len().saturating_sub(payload_start);
        let Some(payload_len) = ditl_item_payload_len(base_type, data_len_byte, remaining) else {
            break;
        };
        let padded = (payload_len + 1) & !1;
        if padded > remaining {
            break;
        }
        let payload = bytes[payload_start..payload_start + payload_len].to_vec();

        items.push(DialogItemRecord {
            item_offset: offset,
            item_type,
            rect: (top, left, bottom, right),
            handle,
            payload,
        });

        offset = payload_start + padded;
    }

    Some(items)
}

/// Adjust dialog window bounds according to System 7 AutoPositioning flags.
pub fn position_dialog_bounds(
    bounds: (i16, i16, i16, i16),
    position: u16,
    screen_width: i32,
    screen_height: i32,
) -> (i16, i16, i16, i16) {
    let centered = matches!(
        position,
        0x280a | 0x300a | 0x380a | 0xa80a | 0xb00a | 0xb80a
    );
    if !centered {
        return bounds;
    }

    let height = i32::from(bounds.2) - i32::from(bounds.0);
    let width = i32::from(bounds.3) - i32::from(bounds.1);
    let top = (screen_height - height).max(0) / 2;
    let left = (screen_width - width).max(0) / 2;

    (
        top.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        left.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        (top + height).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        (left + width).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
    )
}

/// Standard coordinate offset applied to move a dialog item off-screen when hidden.
///
/// Inside Macintosh Volume IV, p. IV-59;
/// Macintosh Toolbox Essentials (1992), pp. 6-123--6-124.
pub const DIALOG_ITEM_HIDDEN_OFFSET: i16 = 16384;

/// Horizontal threshold coordinate distinguishing on-screen dialog items from hidden ones.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-123--6-124.
pub const DIALOG_ITEM_HIDDEN_THRESHOLD: i16 = 8192;

/// Offset a rectangle by `(v_delta, h_delta)` using saturating arithmetic.
pub fn offset_rect(rect: (i16, i16, i16, i16), v_delta: i16, h_delta: i16) -> (i16, i16, i16, i16) {
    (
        rect.0.saturating_add(v_delta),
        rect.1.saturating_add(h_delta),
        rect.2.saturating_add(v_delta),
        rect.3.saturating_add(h_delta),
    )
}

/// Convert a dialog-local rectangle into global/screen coordinates based on dialog bounds.
pub fn dialog_rect_to_global(
    bounds: (i16, i16, i16, i16),
    rect: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    (
        bounds.0.saturating_add(rect.0),
        bounds.1.saturating_add(rect.1),
        bounds.0.saturating_add(rect.2),
        bounds.1.saturating_add(rect.3),
    )
}

/// Convert a global/screen rectangle into dialog-local coordinates based on dialog bounds.
#[allow(dead_code)]
pub fn dialog_rect_to_local(
    bounds: (i16, i16, i16, i16),
    rect: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    (
        rect.0.saturating_sub(bounds.0),
        rect.1.saturating_sub(bounds.1),
        rect.2.saturating_sub(bounds.0),
        rect.3.saturating_sub(bounds.1),
    )
}

/// Whether a rectangle contains a point `(v, h)` (with half-open interval `top <= v < bottom` and `left <= h < right`).
pub fn rect_contains_point(rect: (i16, i16, i16, i16), v: i16, h: i16) -> bool {
    v >= rect.0 && v < rect.2 && h >= rect.1 && h < rect.3
}

/// Inset a rectangle by `(v_delta, h_delta)` using saturating arithmetic.
#[allow(dead_code)]
pub fn inset_rect(rect: (i16, i16, i16, i16), v_delta: i16, h_delta: i16) -> (i16, i16, i16, i16) {
    (
        rect.0.saturating_add(v_delta),
        rect.1.saturating_add(h_delta),
        rect.2.saturating_sub(v_delta),
        rect.3.saturating_sub(h_delta),
    )
}

/// Outset a rectangle by `(v_delta, h_delta)` using saturating arithmetic.
pub fn outset_rect(rect: (i16, i16, i16, i16), v_delta: i16, h_delta: i16) -> (i16, i16, i16, i16) {
    (
        rect.0.saturating_sub(v_delta),
        rect.1.saturating_sub(h_delta),
        rect.2.saturating_add(v_delta),
        rect.3.saturating_add(h_delta),
    )
}

/// Calculate the outer boundary rectangle for a modal dialog box (`dBoxProc`)
/// structure frame including its standard 8-pixel margin.
///
/// Inside Macintosh Volume I, p. I-273.
pub fn dialog_dbox_frame_rect(content_bounds: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    outset_rect(
        content_bounds,
        DIALOG_DBOX_FRAME_MARGIN,
        DIALOG_DBOX_FRAME_MARGIN,
    )
}

/// Calculate the outer border frame rectangle for an edit-text dialog item.
///
/// Inside Macintosh Volume I, p. I-414;
/// Macintosh Human Interface Guidelines (1992), p. 184.
pub fn edit_text_frame_rect(item_rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    outset_rect(item_rect, EDIT_TEXT_FRAME_OUTSET, EDIT_TEXT_FRAME_OUTSET)
}

/// Calculate the text rendering rectangle for a static or editable text dialog item,
/// applying the standard 1-pixel flush-left glyph inset.
///
/// Inside Macintosh Volume I, pp. I-373--I-374.
pub fn dialog_text_rect(item_rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    (
        item_rect.0,
        item_rect.1.saturating_add(DIALOG_TEXT_LEFT_INSET),
        item_rect.2,
        item_rect.3,
    )
}

/// Extract the base dialog item type code, stripping the disabled bit flag (`0x80`).
pub const fn dialog_item_base_type(raw_type: u8) -> u8 {
    raw_type & !DIALOG_ITEM_DISABLED_FLAG
}

/// Resolve the target dialog pointer for a Toolbox event.
///
/// Inside Macintosh Volume I, pp. I-416--I-417:
/// Update and activate events name their target window in `message`; if that
/// window is a dialog box, the event routes to it (and if not, no dialog is targeted).
/// All other events are routed to the frontmost active dialog.
pub fn dialog_target_for_event<F>(
    what: u16,
    message: u32,
    is_dialog: F,
    front_dialog: Option<u32>,
) -> Option<u32>
where
    F: FnOnce(u32) -> bool,
{
    match what {
        EVENT_UPDATE | EVENT_ACTIVATE => {
            if is_dialog(message) {
                Some(message)
            } else {
                None
            }
        }
        _ => front_dialog,
    }
}

/// Determine whether a dialog item rectangle is hidden off-screen.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-123--6-124.
pub fn is_dialog_item_rect_hidden(rect: (i16, i16, i16, i16)) -> bool {
    rect.1 > DIALOG_ITEM_HIDDEN_THRESHOLD
}

/// Calculate the hidden offscreen rectangle for a dialog item (+16384 on left and right).
pub fn hide_dialog_item_rect(rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    (
        rect.0,
        rect.1.wrapping_add(DIALOG_ITEM_HIDDEN_OFFSET),
        rect.2,
        rect.3.wrapping_add(DIALOG_ITEM_HIDDEN_OFFSET),
    )
}

/// Calculate the restored onscreen rectangle for a hidden dialog item (-16384 on left and right).
pub fn show_dialog_item_rect(rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    (
        rect.0,
        rect.1.wrapping_sub(DIALOG_ITEM_HIDDEN_OFFSET),
        rect.2,
        rect.3.wrapping_sub(DIALOG_ITEM_HIDDEN_OFFSET),
    )
}

/// Calculate the enclosing rectangle for a dialog item for erasure and invalidation.
///
/// Inside Macintosh Volume IV, p. IV-59 notes that Dialog Manager drawing can extend
/// outside an editText item's display rectangle by 3 pixels.
pub fn dialog_item_enclosing_rect(
    item_type: u8,
    rect: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    let base_type = item_type & !DIALOG_ITEM_DISABLED_FLAG;
    match base_type {
        DIALOG_ITEM_EDIT_TEXT => (
            rect.0.saturating_sub(3),
            rect.1.saturating_sub(3),
            rect.2.saturating_add(3),
            rect.3.saturating_add(3),
        ),
        _ => rect,
    }
}

/// Find the 0-based index of the first item whose rectangle contains the dialog-local point.
///
/// Inside Macintosh Volume IV, p. IV-60;
/// Macintosh Toolbox Essentials (1992), p. 6-125.
pub fn find_dialog_item_at_local_point<'a>(
    rects: impl IntoIterator<Item = &'a (i16, i16, i16, i16)>,
    pt_v: i16,
    pt_h: i16,
) -> Option<usize> {
    for (idx, &rect) in rects.into_iter().enumerate() {
        if rect_contains_point(rect, pt_v, pt_h) {
            return Some(idx);
        }
    }
    None
}

/// Offset the bounding rectangles of compiled DITL items directly within the raw binary buffer.
pub fn offset_ditl_bytes(
    bytes: &mut [u8],
    item_offsets: impl IntoIterator<Item = usize>,
    dv: i16,
    dh: i16,
) {
    if dv == 0 && dh == 0 {
        return;
    }
    for item_offset in item_offsets {
        for (coordinate_offset, delta) in [(4usize, dv), (6, dh), (8, dv), (10, dh)] {
            let Some(start) = item_offset.checked_add(coordinate_offset) else {
                continue;
            };
            let Some(pair) = bytes.get(start..start.saturating_add(2)) else {
                continue;
            };
            let value = i16::from_be_bytes([pair[0], pair[1]]).saturating_add(delta);
            if let Some(destination) = bytes.get_mut(start..start.saturating_add(2)) {
                destination.copy_from_slice(&value.to_be_bytes());
            }
        }
    }
}

/// Parsed representation of a Macintosh dialog template (`DLOG` resource).
///
/// Inside Macintosh Volume I, pp. I-437--I-438;
/// Macintosh Toolbox Essentials (1992), pp. 6-113--6-114, 6-148.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogTemplate {
    pub bounds: (i16, i16, i16, i16),
    pub proc_id: i16,
    pub visible: bool,
    pub go_away: bool,
    pub ref_con: u32,
    pub items_id: i16,
    pub title: Vec<u8>,
    pub position: u16,
}

impl DialogTemplate {
    /// Decode the Pascal title bytes as a Mac Roman string.
    pub fn title_string(&self) -> String {
        decode_mac_roman(&self.title)
    }
}

/// Parse a dialog template (`DLOG` resource) from compiled binary bytes.
///
/// Inside Macintosh Volume I, pp. I-437--I-438;
/// Macintosh Toolbox Essentials (1992), pp. 6-113--6-114, 6-148.
pub fn parse_dialog_template(bytes: &[u8]) -> Option<DialogTemplate> {
    if bytes.len() < 20 {
        return None;
    }
    let bounds = (
        i16::from_be_bytes(bytes.get(0..2)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(2..4)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(4..6)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(6..8)?.try_into().ok()?),
    );
    let proc_id = i16::from_be_bytes(bytes.get(8..10)?.try_into().ok()?);
    let visible = *bytes.get(10)? != 0;
    let go_away = bytes.get(12).copied().unwrap_or(0) != 0;
    let ref_con = bytes
        .get(14..18)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_be_bytes)
        .unwrap_or(0);
    let items_id = i16::from_be_bytes(bytes.get(18..20)?.try_into().ok()?);
    let title_len = usize::from(bytes.get(20).copied().unwrap_or(0));
    let title_start = 21.min(bytes.len());
    let title_end = 21usize.saturating_add(title_len).min(bytes.len());
    let title = bytes.get(title_start..title_end).unwrap_or(&[]).to_vec();
    let nominal_title_end = 21usize.saturating_add(title_len);
    let position_offset = (nominal_title_end + 1) & !1;
    let position = bytes
        .get(position_offset..position_offset.saturating_add(2))
        .and_then(|value| value.try_into().ok())
        .map(u16::from_be_bytes)
        .unwrap_or(0);

    Some(DialogTemplate {
        bounds,
        proc_id,
        visible,
        go_away,
        ref_con,
        items_id,
        title,
        position,
    })
}

/// Parsed representation of a Macintosh alert template (`ALRT` resource).
///
/// Inside Macintosh Volume I, pp. I-425--I-426;
/// Macintosh Toolbox Essentials (1992), p. 6-150.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlertTemplate {
    pub bounds: (i16, i16, i16, i16),
    pub items_id: i16,
    pub stages: u16,
    pub position: u16,
}

/// Parse an alert template (`ALRT` resource) from compiled binary bytes.
///
/// Inside Macintosh Volume I, pp. I-425--I-426;
/// Macintosh Toolbox Essentials (1992), p. 6-150.
pub fn parse_alert_template(bytes: &[u8]) -> Option<AlertTemplate> {
    if bytes.len() < 8 {
        return None;
    }
    let bounds = (
        i16::from_be_bytes(bytes.get(0..2)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(2..4)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(4..6)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(6..8)?.try_into().ok()?),
    );
    let items_id = bytes
        .get(8..10)
        .and_then(|slice| slice.try_into().ok())
        .map(i16::from_be_bytes)
        .unwrap_or(0);
    let stages = bytes
        .get(10..12)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_be_bytes)
        .unwrap_or(0);
    let position = bytes
        .get(12..14)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_be_bytes)
        .unwrap_or(0);

    Some(AlertTemplate {
        bounds,
        items_id,
        stages,
        position,
    })
}

/// Information about an alert stage extracted from an `ALRT` resource's stage word.
///
/// Inside Macintosh Volume I, pp. I-422--I-424;
/// Macintosh Toolbox Essentials (1992), pp. 6-106, 6-150.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlertStageInfo {
    /// Zero-based stage index (0..=3), clamped from guest stage counter.
    pub stage_index: u16,
    /// The 4-bit nibble for this stage.
    pub stage_nibble: u8,
    /// Whether the alert window is drawn (bit 2 / 0x04 of the nibble).
    pub box_drawn: bool,
    /// The default button item index: 1 (OK) if bit 3 is 0, or 2 (Cancel) if bit 3 is 1.
    pub default_item: i16,
    /// Sound number (0..=3) from bits 0..=1.
    pub sound_number: u8,
}

/// Decode alert stage parameters for the specified stage counter (0..=3).
pub fn alert_stage_info(stages: u16, stage_counter: u16) -> AlertStageInfo {
    let stage_index = stage_counter.min(3);
    let stage_nibble = ((stages >> (stage_index * 4)) & 0x0F) as u8;
    let box_drawn = (stage_nibble & 0x04) != 0;
    let default_item = if (stage_nibble & 0x08) == 0 { 1 } else { 2 };
    let sound_number = stage_nibble & 0x03;
    AlertStageInfo {
        stage_index,
        stage_nibble,
        box_drawn,
        default_item,
        sound_number,
    }
}

/// Replace `^0`..`^3` placeholders in dialog text bytes with the corresponding ParamText slot contents.
///
/// Returns `Cow::Borrowed` when no `^` character is present, avoiding allocations.
/// Inside Macintosh Volume I, p. I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-129--6-130.
pub fn apply_param_text<'a>(text: &'a [u8], slots: &[impl AsRef<[u8]>]) -> Cow<'a, [u8]> {
    if !text.contains(&b'^') {
        return Cow::Borrowed(text);
    }
    let mut expanded = Vec::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        if text[offset] == b'^' {
            if let Some(index) = text
                .get(offset + 1)
                .copied()
                .filter(u8::is_ascii_digit)
                .map(|b| usize::from(b - b'0'))
                .filter(|&idx| idx < slots.len())
            {
                expanded.extend_from_slice(slots[index].as_ref());
                offset += 2;
                continue;
            }
        }
        expanded.push(text[offset]);
        offset += 1;
    }
    Cow::Owned(expanded)
}

/// String-oriented wrapper for `apply_param_text` operating on Mac Roman text strings.
pub fn apply_param_text_str<'a, S: AsRef<[u8]>>(text: &'a str, slots: &[S]) -> Cow<'a, str> {
    if !text.contains('^') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '^' {
            if let Some(&next) = chars.peek() {
                if let Some(digit) = next.to_digit(10) {
                    let idx = digit as usize;
                    if idx < slots.len() {
                        chars.next();
                        out.push_str(&decode_mac_roman(slots[idx].as_ref()));
                        continue;
                    }
                }
            }
        }
        out.push(ch);
    }
    Cow::Owned(out)
}

/// Canonical Macintosh key codes for modal dialog navigation.
pub const KEY_RETURN: u8 = 0x24;
pub const KEY_NUMPAD_ENTER: u8 = 0x4C;
pub const KEY_ESCAPE: u8 = 0x35;
pub const KEY_TAB: u8 = 0x30;
pub const KEY_PERIOD: u8 = 0x2F;

/// Canonical Macintosh ASCII character codes for modal dialog navigation.
pub const CHAR_RETURN: u8 = 0x0D;
pub const CHAR_ENTER: u8 = 0x03;
pub const CHAR_ESCAPE: u8 = 0x1B;
pub const CHAR_TAB: u8 = 0x09;
pub const CHAR_PERIOD: u8 = b'.';

/// Event modifier bit for Command key (`cmdKey`).
pub const MODIFIER_CMD_KEY: u16 = 0x0100;

/// Event `what` codes for Toolbox events.
///
/// Inside Macintosh Volume I (1985), pp. I-249--I-250;
/// Macintosh Toolbox Essentials (1992), pp. 2-83--2-84.
pub const EVENT_NULL: u16 = 0;
pub const EVENT_MOUSE_DOWN: u16 = 1;
#[allow(dead_code)]
pub const EVENT_MOUSE_UP: u16 = 2;
pub const EVENT_KEY_DOWN: u16 = 3;
#[allow(dead_code)]
pub const EVENT_KEY_UP: u16 = 4;
pub const EVENT_AUTO_KEY: u16 = 5;
pub const EVENT_UPDATE: u16 = 6;
#[allow(dead_code)]
pub const EVENT_DISK: u16 = 7;
pub const EVENT_ACTIVATE: u16 = 8;

/// Decision produced by standard modal dialog keyboard filtering.
///
/// Inside Macintosh Volume I (1985), pp. I-415--I-416;
/// Macintosh Toolbox Essentials (1992), pp. 6-86--6-90, 6-138.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogFilterDecision {
    /// Return or Enter key activated: triggers the dialog's default button.
    TriggerDefaultButton,
    /// Escape or Command-Period key activated: triggers the dialog's cancel button.
    TriggerCancelButton,
    /// Tab key activated: advances focus to the next `editText` item.
    AdvanceEditTextFocus,
    /// Key event not handled by standard dialog filter.
    Unhandled,
}

/// Determine whether the specified character and key code correspond to the default button activation key (Return or Enter).
pub fn is_dialog_default_key(char_code: u8, key_code: u8) -> bool {
    matches!(char_code, CHAR_RETURN | CHAR_ENTER)
        || matches!(key_code, KEY_RETURN | KEY_NUMPAD_ENTER)
}

/// Determine whether the specified character, key code, and modifier word correspond to the cancel button activation key (Escape or Command-Period).
pub fn is_dialog_cancel_key(char_code: u8, key_code: u8, modifiers: u16) -> bool {
    char_code == CHAR_ESCAPE
        || key_code == KEY_ESCAPE
        || ((modifiers & MODIFIER_CMD_KEY) != 0
            && (char_code == CHAR_PERIOD || key_code == KEY_PERIOD))
}

/// Determine whether the specified character and key code correspond to the Tab key.
pub fn is_dialog_tab_key(char_code: u8, key_code: u8) -> bool {
    char_code == CHAR_TAB || key_code == KEY_TAB
}

/// Evaluate a keyboard event for standard modal dialog filtering.
///
/// Inside Macintosh Volume I, pp. I-415--I-416;
/// Macintosh Toolbox Essentials (1992), pp. 6-86--6-90, 6-138.
pub fn evaluate_modal_dialog_key(
    event_what: u16,
    message: u32,
    modifiers: u16,
) -> DialogFilterDecision {
    if !matches!(event_what, EVENT_KEY_DOWN | EVENT_AUTO_KEY) {
        return DialogFilterDecision::Unhandled;
    }
    let char_code = (message & 0xFF) as u8;
    let key_code = ((message >> 8) & 0xFF) as u8;

    if is_dialog_default_key(char_code, key_code) {
        DialogFilterDecision::TriggerDefaultButton
    } else if is_dialog_cancel_key(char_code, key_code, modifiers) {
        DialogFilterDecision::TriggerCancelButton
    } else if is_dialog_tab_key(char_code, key_code) {
        DialogFilterDecision::AdvanceEditTextFocus
    } else {
        DialogFilterDecision::Unhandled
    }
}

/// Find the next `editText` item in item-list order, wrapping cyclically.
///
/// Takes an iterator of item types (raw `u8`) and the current 1-indexed
/// edit item number (0 if no edit item is active). Returns the 1-indexed item
/// number of the next `editText` item, or `None` if the dialog has no `editText` items.
///
/// Inside Macintosh Volume I, p. I-416;
/// Macintosh Toolbox Essentials (1992), p. 6-88.
pub fn find_next_edit_text_item(
    item_types: impl IntoIterator<Item = u8>,
    current_edit_item_1_indexed: usize,
) -> Option<usize> {
    let types: Vec<u8> = item_types.into_iter().collect();
    if types.is_empty() {
        return None;
    }
    let start = current_edit_item_1_indexed;
    for offset in 0..types.len() {
        let idx = (start + offset) % types.len();
        let base_type = types[idx] & !DIALOG_ITEM_DISABLED_FLAG;
        if base_type == DIALOG_ITEM_EDIT_TEXT {
            return Some(idx + 1);
        }
    }
    None
}

/// Helper to find the next `editText` item directly from a slice of `DialogItemRecord`s.
#[allow(dead_code)]
pub fn find_next_edit_text_in_items(
    items: &[DialogItemRecord],
    current_edit_item_1_indexed: usize,
) -> Option<usize> {
    find_next_edit_text_item(
        items.iter().map(|item| item.item_type),
        current_edit_item_1_indexed,
    )
}

/// Clamps a text length to 255 bytes (Pascal string maximum payload).
pub fn clamp_dialog_item_text_len(len: usize) -> u8 {
    len.min(255) as u8
}

/// Clamps a byte slice to at most 255 bytes (Pascal string maximum payload).
pub fn clamp_dialog_item_text_bytes(text_bytes: &[u8]) -> &[u8] {
    let len = text_bytes.len().min(255);
    &text_bytes[..len]
}

/// Encodes raw text bytes into Pascal string format: `(length_byte, clamped_payload)`.
pub fn encode_dialog_item_pstring(text_bytes: &[u8]) -> (u8, &[u8]) {
    let clamped = clamp_dialog_item_text_bytes(text_bytes);
    (clamped.len() as u8, clamped)
}

/// Decodes a Pascal string slice (where the first byte is length) into payload bytes.
#[allow(dead_code)]
pub fn decode_dialog_item_pstring(bytes: &[u8]) -> Option<&[u8]> {
    let &len = bytes.first()?;
    let end = 1 + len as usize;
    if bytes.len() >= end {
        Some(&bytes[1..end])
    } else {
        None
    }
}

/// Retrieves a reference to an item from a 1-indexed slice (returns `None` if `item_number == 0` or out of bounds).
pub fn get_item_at_1_indexed<T>(items: &[T], item_number_1_indexed: usize) -> Option<&T> {
    item_number_1_indexed
        .checked_sub(1)
        .and_then(|idx| items.get(idx))
}

/// Retrieves a mutable reference to an item from a 1-indexed slice (returns `None` if `item_number == 0` or out of bounds).
#[allow(dead_code)]
pub fn get_item_at_1_indexed_mut<T>(
    items: &mut [T],
    item_number_1_indexed: usize,
) -> Option<&mut T> {
    item_number_1_indexed
        .checked_sub(1)
        .and_then(|idx| items.get_mut(idx))
}

/// Returns true if the raw item type represents a button, checkbox, radio button, or resource control.
pub fn is_dialog_item_control(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_control()
}

/// Returns true if the raw item type represents static text or edit text.
pub fn is_dialog_item_text(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_text()
}

/// Returns true if the raw item type represents static or editable text requiring handle disposal.
pub fn is_dialog_item_disposable_text(raw_type: u8) -> bool {
    is_dialog_item_text(raw_type)
}

/// Returns true if the raw item type represents a button, checkbox, radio, or resource control requiring control record disposal.
pub fn is_dialog_item_disposable_control(raw_type: u8) -> bool {
    is_dialog_item_control(raw_type)
}

/// Maps a dialog item type (with or without disabled bit) to its standard Control Manager procID.
///
/// Inside Macintosh Volume I, pp. I-410, I-421:
/// - Button (4) -> 0 (`pushButProc`)
/// - Checkbox (5) -> 1 (`checkBoxProc`)
/// - Radio (6) -> 2 (`radioButProc`)
pub fn dialog_item_control_proc_id(item_type: u8) -> Option<i16> {
    match item_type & !DIALOG_ITEM_DISABLED_FLAG {
        DIALOG_ITEM_BUTTON => Some(0),
        DIALOG_ITEM_CHECKBOX => Some(1),
        DIALOG_ITEM_RADIO => Some(2),
        _ => None,
    }
}

/// Maps an alert type (Stop, Note, Caution, Plain) to standard icon resource ID (Stop=0, Note=1, Caution=2, Plain=None).
#[allow(dead_code)]
pub fn alert_icon_id(alert_type: u16) -> Option<i16> {
    match alert_type {
        ALERT_TYPE_STOP => Some(0),
        ALERT_TYPE_NOTE => Some(1),
        ALERT_TYPE_CAUTION => Some(2),
        _ => None,
    }
}

/// Computes the next AlertStage counter capped at 3, per Inside Macintosh Volume I, p. I-423.
pub fn next_alert_stage(current_stage: u16) -> u16 {
    ((current_stage as u32) + 1).min(3) as u16
}

/// Normalizes an edit text item selection range `(start_sel, end_sel)` against a given text length.
///
/// Inside Macintosh Volume I, p. I-414, and Macintosh Toolbox Essentials (1992), p. 6-132:
/// - Special case: `(0, -1)` or `(0, 32767)` indicates "select all" (`0..text_len`).
/// - Clamps start and end bounds to `0..=text_len`.
/// - Normalizes reversed bounds (`start > end` -> swapped to `end..start`).
pub fn normalize_dialog_item_selection(
    start_sel: i16,
    end_sel: i16,
    text_len: usize,
) -> (u16, u16) {
    let text_len = text_len.min(i16::MAX as usize) as i16;
    let (s, e) = if start_sel == 0 && (end_sel == -1 || end_sel == i16::MAX) {
        (0, text_len)
    } else {
        let s = start_sel.clamp(0, text_len);
        let e = end_sel.clamp(0, text_len);
        if s <= e {
            (s, e)
        } else {
            (e, s)
        }
    };
    (s as u16, e as u16)
}

/// Computes the outer bounding rectangle and corner oval radius for drawing the standard 3px bold
/// default button ring around a push button rectangle.
///
/// Macintosh Toolbox Essentials (1992), Listing 6-17:
/// Outer rect is outset by 4 pixels in all dimensions; corner oval diameter is `max(4, height / 2 - 4)`.
pub fn default_button_outline_geometry(
    button_rect: (i16, i16, i16, i16),
) -> ((i16, i16, i16, i16), i16) {
    let outer = (
        button_rect.0.saturating_sub(DEFAULT_BUTTON_OUTLINE_INSET),
        button_rect.1.saturating_sub(DEFAULT_BUTTON_OUTLINE_INSET),
        button_rect.2.saturating_add(DEFAULT_BUTTON_OUTLINE_INSET),
        button_rect.3.saturating_add(DEFAULT_BUTTON_OUTLINE_INSET),
    );
    let height = outer.2.saturating_sub(outer.0);
    let oval = (height / 2 - 4).max(4);
    (outer, oval)
}

/// Returns true if the title matches "Cancel" (case-insensitive ASCII), which identifies
/// the standard Cancel button in dialog boxes per Macintosh Toolbox Essentials (1992), p. 6-51.
pub fn is_dialog_cancel_button_title(title: &[u8]) -> bool {
    title.eq_ignore_ascii_case(b"cancel")
}

/// Returns true if the string title matches "Cancel" (case-insensitive ASCII).
#[allow(dead_code)]
pub fn is_dialog_cancel_button_title_str(title: &str) -> bool {
    title.eq_ignore_ascii_case("cancel")
}

/// Finds the 1-indexed item number of an enabled PushButton with title "Cancel" (case-insensitive).
pub fn find_dialog_cancel_item_index<I, T>(items: I) -> Option<u16>
where
    I: IntoIterator<Item = (u8, T)>,
    T: AsRef<[u8]>,
{
    items
        .into_iter()
        .enumerate()
        .find_map(|(idx, (raw_type, title))| {
            let is_enabled_button = (raw_type & DIALOG_ITEM_DISABLED_FLAG == 0)
                && (raw_type & !DIALOG_ITEM_DISABLED_FLAG == DIALOG_ITEM_BUTTON);
            if is_enabled_button && is_dialog_cancel_button_title(title.as_ref()) {
                u16::try_from(idx + 1).ok()
            } else {
                None
            }
        })
}

/// Computes the `(dv, dh)` translation offset for items being appended to a dialog via `AppendDITL`.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-108, 6-153:
/// - `overlayDITL` (0): (0, 0)
/// - `appendDITLRight` (1): (0, dialog_width)
/// - `appendDITLBottom` (2): (dialog_height, 0)
/// - negative `item_no` (< 0): offset relative to the upper-left of item `-method` (1-indexed).
pub fn append_ditl_offset_delta<F>(
    method: i16,
    dialog_height: i16,
    dialog_width: i16,
    item_origin_lookup: F,
) -> (i16, i16)
where
    F: FnOnce(usize) -> Option<(i16, i16)>,
{
    match method {
        APPEND_DITL_OVERLAY => (0, 0),
        APPEND_DITL_RIGHT => (0, dialog_width),
        APPEND_DITL_BOTTOM => (dialog_height, 0),
        relative_item if relative_item < 0 => {
            let item_no = usize::from(relative_item.unsigned_abs());
            item_origin_lookup(item_no).unwrap_or((0, 0))
        }
        _ => (0, 0),
    }
}

/// Calculates the number of retained items and the new DITL count-minus-one header word
/// after removing `remove_count` items from a dialog with `current_count` items.
///
/// Macintosh Toolbox Essentials (1992), p. 6-154:
/// - If `remove_count >= current_count`, all items are removed (0 retained, header word is -1 / 0xFFFF).
/// - Otherwise, `retained = current_count - remove_count`, and header word is `(retained - 1) as i16`.
pub fn shorten_ditl_counts(current_count: usize, remove_count: usize) -> (usize, i16) {
    let remove = remove_count.min(current_count);
    let retained = current_count.saturating_sub(remove);
    let count_minus_one = if retained == 0 {
        -1
    } else {
        retained.saturating_sub(1).min(i16::MAX as usize) as i16
    };
    (retained, count_minus_one)
}

/// Returns true if two rectangles intersect.
pub fn rects_intersect(a: (i16, i16, i16, i16), b: (i16, i16, i16, i16)) -> bool {
    a.0 < b.2 && a.2 > b.0 && a.1 < b.3 && a.3 > b.1
}

/// Returns true if a dialog item's local rectangle intersects the dialog global bounds.
pub fn dialog_item_intersects_bounds(
    bounds: (i16, i16, i16, i16),
    item_rect: (i16, i16, i16, i16),
) -> bool {
    rects_intersect(dialog_rect_to_global(bounds, item_rect), bounds)
}

/// Checks whether an in-bounds dialog is game-managed (all items intersecting bounds are user items).
pub fn is_dialog_game_managed<I>(bounds: (i16, i16, i16, i16), items: I) -> bool
where
    I: IntoIterator<Item = (u8, (i16, i16, i16, i16))>,
{
    let mut has_visible_item = false;
    for (item_type, item_rect) in items {
        if !dialog_item_intersects_bounds(bounds, item_rect) {
            continue;
        }
        has_visible_item = true;
        if (item_type & !DIALOG_ITEM_DISABLED_FLAG) != DIALOG_ITEM_USER_ITEM {
            return false;
        }
    }
    has_visible_item
}

/// Tests whether an event should be handled as part of an active modeless or movable modal dialog.
///
/// Inside Macintosh Volume I (1985), p. I-416;
/// Macintosh Toolbox Essentials (1992), p. 6-138:
/// - If there is no target dialog corresponding to the event, returns `false`.
/// - For `updateEvt` (6) and `activateEvt` (8), returns `true` if `message == dialog_ptr`.
/// - For `mouseDown` (1), returns `true` if the mouse point `(where_v, where_h)` falls within the dialog window bounds.
/// - For all other event types directed to the dialog, returns `true`.
pub fn is_dialog_event(
    what: u16,
    message: u32,
    where_v: i16,
    where_h: i16,
    target_dialog: Option<u32>,
    dialog_bounds: Option<(i16, i16, i16, i16)>,
) -> bool {
    let Some(dialog_ptr) = target_dialog else {
        return false;
    };
    match what {
        EVENT_UPDATE | EVENT_ACTIVATE => message == dialog_ptr,
        EVENT_MOUSE_DOWN => {
            dialog_bounds.is_some_and(|bounds| rect_contains_point(bounds, where_v, where_h))
        }
        _ => true,
    }
}

/// Validates whether a character is an editable character accepted by dialog edit fields.
///
/// Macintosh Toolbox Essentials (1992), p. 6-139:
/// Accepts backspace (0x08) and printable ASCII / Mac Roman characters (0x20..=0x7E).
pub fn is_dialog_edit_text_character(character: u8) -> bool {
    matches!(character, 0x08 | 0x20..=0x7E)
}

/// Calculates the updated text bytes and new insertion caret offset after processing a key press
/// (such as backspace or printable character) against an existing text buffer and selection range.
///
/// Macintosh Toolbox Essentials (1992), pp. 2-83--2-88, 6-139.
pub fn textedit_key_result(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_kind_decoding_and_properties() {
        assert_eq!(DialogItemKind::from_raw_type(4), DialogItemKind::Button);
        assert_eq!(
            DialogItemKind::from_raw_type(4 | DIALOG_ITEM_DISABLED_FLAG),
            DialogItemKind::Button
        );
        assert!(DialogItemKind::Button.is_control());
        assert!(!DialogItemKind::Button.is_text());

        assert_eq!(DialogItemKind::from_raw_type(8), DialogItemKind::StaticText);
        assert!(DialogItemKind::StaticText.is_text());
        assert!(!DialogItemKind::StaticText.is_control());

        assert_eq!(DialogItemKind::from_raw_type(16), DialogItemKind::EditText);
        assert_eq!(DialogItemKind::from_raw_type(32), DialogItemKind::Icon);
        assert_eq!(DialogItemKind::from_raw_type(64), DialogItemKind::Picture);
        assert_eq!(DialogItemKind::from_raw_type(0), DialogItemKind::UserItem);
        assert_eq!(
            DialogItemKind::from_raw_type(99),
            DialogItemKind::Unknown(99)
        );
    }

    #[test]
    fn parse_ditl_items_round_trip() {
        let mut data = Vec::new();
        // Item count minus 1: 1 item (0x0000)
        data.extend_from_slice(&0i16.to_be_bytes());
        // Handle: 0
        data.extend_from_slice(&0u32.to_be_bytes());
        // Rect: (10, 20, 30, 40)
        data.extend_from_slice(&10i16.to_be_bytes());
        data.extend_from_slice(&20i16.to_be_bytes());
        data.extend_from_slice(&30i16.to_be_bytes());
        data.extend_from_slice(&40i16.to_be_bytes());
        // Type: Button (4), Length: 2, Payload: "OK"
        data.push(DIALOG_ITEM_BUTTON);
        data.push(2);
        data.extend_from_slice(b"OK");

        let items = parse_ditl_items(&data).expect("valid DITL");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].rect, (10, 20, 30, 40));
        assert_eq!(items[0].kind(), DialogItemKind::Button);
        assert!(items[0].is_enabled());
        assert_eq!(items[0].text(), "OK");
    }

    #[test]
    fn position_dialog_centering() {
        let bounds = (0, 0, 100, 200);
        // Centered position flag 0x280a
        let centered = position_dialog_bounds(bounds, 0x280a, 640, 480);
        assert_eq!(centered, (190, 220, 290, 420));

        // Non-centered flag leaves bounds unchanged
        let uncentered = position_dialog_bounds(bounds, 0x0000, 640, 480);
        assert_eq!(uncentered, bounds);
    }

    #[test]
    fn parse_ditl_items_empty_and_truncated() {
        let mut empty_data = Vec::new();
        empty_data.extend_from_slice(&(-1i16).to_be_bytes());
        let empty_items = parse_ditl_items(&empty_data).expect("valid empty DITL");
        assert!(empty_items.is_empty());
        assert_eq!(ditl_total_used_len(&empty_data), 2);

        // Truncated item: count 2, first valid button, second item truncated
        let mut data = Vec::new();
        data.extend_from_slice(&1i16.to_be_bytes()); // count-1 = 1 (2 items)
                                                     // Item 1: Button "OK"
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&[0, 1, 0, 2, 0, 11, 0, 42]);
        data.push(DIALOG_ITEM_BUTTON);
        data.push(2);
        data.extend_from_slice(b"OK");
        // Item 2: StatText declaring 6 bytes, but only 3 provided
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&[0, 12, 0, 2, 0, 22, 0, 72]);
        data.push(DIALOG_ITEM_STATIC_TEXT);
        data.push(6);
        data.extend_from_slice(b"Bad");

        let items = parse_ditl_items(&data).expect("parses valid items before truncated one");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text(), "OK");
    }

    #[test]
    fn parse_ditl_items_resource_ids_and_record_lengths() {
        let mut data = Vec::new();
        data.extend_from_slice(&1i16.to_be_bytes()); // 2 items
                                                     // Item 1: resCtrl (7) with reserved byte 0 followed by 128
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&[0, 1, 0, 2, 0, 11, 0, 42]);
        data.push(DIALOG_ITEM_RESOURCE_CONTROL);
        data.push(0); // reserved byte per MTE
        data.extend_from_slice(&128i16.to_be_bytes());

        // Item 2: icon (32) with length byte 2 followed by -42
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&[0, 12, 0, 2, 0, 28, 0, 34]);
        data.push(DIALOG_ITEM_ICON);
        data.push(2); // length byte 2 per IM:I
        data.extend_from_slice(&(-42i16).to_be_bytes());

        let len1 = ditl_item_record_len_at(&data, 2).expect("record 1 len");
        assert_eq!(len1, 16);
        let len2 = ditl_item_record_len_at(&data, 2 + len1).expect("record 2 len");
        assert_eq!(len2, 16);
        assert_eq!(ditl_total_used_len(&data), 2 + len1 + len2);

        let items = parse_ditl_items(&data).expect("valid items");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind(), DialogItemKind::ResourceControl);
        assert_eq!(items[0].resource_id(), Some(128));
        assert_eq!(items[1].kind(), DialogItemKind::Icon);
        assert_eq!(items[1].resource_id(), Some(-42));
    }

    #[test]
    fn dialog_item_rect_geometry_and_visibility() {
        let initial_rect = (10, 20, 30, 80);
        let offset = offset_rect(initial_rect, 5, -10);
        assert_eq!(offset, (15, 10, 35, 70));

        let bounds = (100, 200, 300, 500);
        let global = dialog_rect_to_global(bounds, initial_rect);
        assert_eq!(global, (110, 220, 130, 280));
        let local = dialog_rect_to_local(bounds, global);
        assert_eq!(local, initial_rect);

        assert!(!is_dialog_item_rect_hidden(initial_rect));
        let hidden = hide_dialog_item_rect(initial_rect);
        assert_eq!(hidden, (10, 16404, 30, 16464));
        assert!(is_dialog_item_rect_hidden(hidden));
        let shown = show_dialog_item_rect(hidden);
        assert_eq!(shown, initial_rect);

        // Enclosing rect: standard controls match exactly; editText extends 3px on all sides
        assert_eq!(
            dialog_item_enclosing_rect(DIALOG_ITEM_BUTTON, initial_rect),
            initial_rect
        );
        assert_eq!(
            dialog_item_enclosing_rect(DIALOG_ITEM_EDIT_TEXT, initial_rect),
            (7, 17, 33, 83)
        );

        // DialogItemRecord helpers
        let mut record = DialogItemRecord {
            item_offset: 2,
            item_type: DIALOG_ITEM_EDIT_TEXT,
            rect: initial_rect,
            handle: 0,
            payload: vec![],
        };
        assert!(!record.is_hidden());
        assert_eq!(record.enclosing_rect(), (7, 17, 33, 83));
        record.offset_rect(10, 20);
        assert_eq!(record.rect, (20, 40, 40, 100));
    }

    #[test]
    fn dialog_item_point_hit_testing_and_ditl_byte_offsetting() {
        let rect1 = (10, 20, 30, 50);
        let rect2 = (30, 20, 50, 50);
        let rects = [rect1, rect2];

        // rect_contains_point is half-open: [top, bottom) and [left, right)
        assert!(rect_contains_point(rect1, 10, 20));
        assert!(rect_contains_point(rect1, 29, 49));
        assert!(!rect_contains_point(rect1, 30, 20));
        assert!(!rect_contains_point(rect1, 10, 50));

        // find_dialog_item_at_local_point
        assert_eq!(find_dialog_item_at_local_point(&rects, 15, 25), Some(0));
        assert_eq!(find_dialog_item_at_local_point(&rects, 35, 25), Some(1));
        assert_eq!(find_dialog_item_at_local_point(&rects, 5, 5), None);

        // offset_ditl_bytes
        let mut data = Vec::new();
        data.extend_from_slice(&0i16.to_be_bytes()); // 1 item
        data.extend_from_slice(&0u32.to_be_bytes()); // handle at 2
        data.extend_from_slice(&10i16.to_be_bytes()); // top at 6
        data.extend_from_slice(&20i16.to_be_bytes()); // left at 8
        data.extend_from_slice(&30i16.to_be_bytes()); // bottom at 10
        data.extend_from_slice(&40i16.to_be_bytes()); // right at 12
        data.push(DIALOG_ITEM_BUTTON);
        data.push(0);

        offset_ditl_bytes(&mut data, [2], 5, 10);
        assert_eq!(i16::from_be_bytes([data[6], data[7]]), 15);
        assert_eq!(i16::from_be_bytes([data[8], data[9]]), 30);
        assert_eq!(i16::from_be_bytes([data[10], data[11]]), 35);
        assert_eq!(i16::from_be_bytes([data[12], data[13]]), 50);
    }

    #[test]
    fn dialog_template_parsing_and_positioning() {
        // Classic 22-byte DLOG (empty title, no position word)
        let mut dlog22 = Vec::new();
        dlog22.extend_from_slice(&228i16.to_be_bytes()); // top
        dlog22.extend_from_slice(&198i16.to_be_bytes()); // left
        dlog22.extend_from_slice(&372i16.to_be_bytes()); // bottom
        dlog22.extend_from_slice(&455i16.to_be_bytes()); // right
        dlog22.extend_from_slice(&2i16.to_be_bytes()); // proc_id
        dlog22.push(1); // visible
        dlog22.push(0); // filler
        dlog22.push(0); // go_away
        dlog22.push(0); // filler
        dlog22.extend_from_slice(&0u32.to_be_bytes()); // ref_con
        dlog22.extend_from_slice(&1013i16.to_be_bytes()); // items_id
        dlog22.push(0); // empty title length
        dlog22.push(0); // padding

        let template = parse_dialog_template(&dlog22).expect("valid 22-byte dlog");
        assert_eq!(template.bounds, (228, 198, 372, 455));
        assert_eq!(template.proc_id, 2);
        assert!(template.visible);
        assert!(!template.go_away);
        assert_eq!(template.ref_con, 0);
        assert_eq!(template.items_id, 1013);
        assert_eq!(template.title, b"");
        assert_eq!(template.title_string(), "");
        assert_eq!(template.position, 0);

        // 24-byte DLOG with position word 0x280A
        let mut dlog24 = dlog22.clone();
        dlog24.extend_from_slice(&0x280Au16.to_be_bytes());
        let template = parse_dialog_template(&dlog24).expect("valid 24-byte dlog");
        assert_eq!(template.position, 0x280A);

        // DLOG with odd title ("Odd" -> 3 bytes, padded to 24, position at 24..26)
        let mut dlog_odd = Vec::new();
        dlog_odd.extend_from_slice(&10i16.to_be_bytes());
        dlog_odd.extend_from_slice(&20i16.to_be_bytes());
        dlog_odd.extend_from_slice(&110i16.to_be_bytes());
        dlog_odd.extend_from_slice(&220i16.to_be_bytes());
        dlog_odd.extend_from_slice(&1i16.to_be_bytes());
        dlog_odd.push(1); // visible
        dlog_odd.push(0);
        dlog_odd.push(1); // go_away
        dlog_odd.push(0);
        dlog_odd.extend_from_slice(&42u32.to_be_bytes());
        dlog_odd.extend_from_slice(&701i16.to_be_bytes());
        dlog_odd.push(3); // title len
        dlog_odd.extend_from_slice(b"Odd");
        // title_end = 21 + 3 = 24 (even boundary), so position is at offset 24..26
        dlog_odd.extend_from_slice(&0x300Au16.to_be_bytes());
        let template = parse_dialog_template(&dlog_odd).expect("valid odd title dlog");
        assert_eq!(template.title_string(), "Odd");
        assert_eq!(template.items_id, 701);
        assert_eq!(template.position, 0x300A);
        assert!(template.go_away);

        // DLOG with even title ("Even" -> 4 bytes, padded to 26, position at 26..28)
        let mut dlog_even = Vec::new();
        dlog_even.extend_from_slice(&11i16.to_be_bytes());
        dlog_even.extend_from_slice(&21i16.to_be_bytes());
        dlog_even.extend_from_slice(&111i16.to_be_bytes());
        dlog_even.extend_from_slice(&221i16.to_be_bytes());
        dlog_even.extend_from_slice(&1i16.to_be_bytes());
        dlog_even.push(1);
        dlog_even.push(0);
        dlog_even.push(0);
        dlog_even.push(0);
        dlog_even.extend_from_slice(&0u32.to_be_bytes());
        dlog_even.extend_from_slice(&702i16.to_be_bytes());
        dlog_even.push(4); // title len
        dlog_even.extend_from_slice(b"Even");
        dlog_even.push(0); // alignment pad
        dlog_even.extend_from_slice(&0x700Au16.to_be_bytes());
        let template = parse_dialog_template(&dlog_even).expect("valid even title dlog");
        assert_eq!(template.title_string(), "Even");
        assert_eq!(template.items_id, 702);
        assert_eq!(template.position, 0x700A);

        // Truncated DLOG (< 20 bytes) returns None
        assert!(parse_dialog_template(&[0u8; 19]).is_none());
    }

    #[test]
    fn alert_template_parsing_and_stages() {
        // Classic 12-byte ALRT
        let mut alrt12 = Vec::new();
        alrt12.extend_from_slice(&10i16.to_be_bytes());
        alrt12.extend_from_slice(&20i16.to_be_bytes());
        alrt12.extend_from_slice(&90i16.to_be_bytes());
        alrt12.extend_from_slice(&220i16.to_be_bytes());
        alrt12.extend_from_slice(&123i16.to_be_bytes()); // items_id
        alrt12.extend_from_slice(&0x0008u16.to_be_bytes()); // stages

        let alert = parse_alert_template(&alrt12).expect("valid 12-byte alrt");
        assert_eq!(alert.bounds, (10, 20, 90, 220));
        assert_eq!(alert.items_id, 123);
        assert_eq!(alert.stages, 0x0008);
        assert_eq!(alert.position, 0);

        // 14-byte System 7 ALRT with position word 0xB00A
        let mut alrt14 = Vec::new();
        alrt14.extend_from_slice(&0i16.to_be_bytes());
        alrt14.extend_from_slice(&0i16.to_be_bytes());
        alrt14.extend_from_slice(&80i16.to_be_bytes());
        alrt14.extend_from_slice(&200i16.to_be_bytes());
        alrt14.extend_from_slice(&(-321i16).to_be_bytes()); // items_id
        alrt14.extend_from_slice(&0xF721u16.to_be_bytes()); // stages
        alrt14.extend_from_slice(&0xB00Au16.to_be_bytes()); // position

        let alert = parse_alert_template(&alrt14).expect("valid 14-byte alrt");
        assert_eq!(alert.items_id, -321);
        assert_eq!(alert.stages, 0xF721);
        assert_eq!(alert.position, 0xB00A);

        // Alert stages evaluation: 0xF721
        // Stage 0 (counter 0): nibble 1 (0b0001) -> box_drawn: false, default_item: 1, sound: 1
        let s0 = alert_stage_info(0xF721, 0);
        assert_eq!(s0.stage_index, 0);
        assert_eq!(s0.stage_nibble, 1);
        assert!(!s0.box_drawn);
        assert_eq!(s0.default_item, 1);
        assert_eq!(s0.sound_number, 1);

        // Stage 1 (counter 1): nibble 2 (0b0010) -> box_drawn: false, default_item: 1, sound: 2
        let s1 = alert_stage_info(0xF721, 1);
        assert_eq!(s1.stage_index, 1);
        assert_eq!(s1.stage_nibble, 2);
        assert!(!s1.box_drawn);
        assert_eq!(s1.default_item, 1);
        assert_eq!(s1.sound_number, 2);

        // Stage 2 (counter 2): nibble 7 (0b0111) -> box_drawn: true, default_item: 1, sound: 3
        let s2 = alert_stage_info(0xF721, 2);
        assert_eq!(s2.stage_index, 2);
        assert_eq!(s2.stage_nibble, 7);
        assert!(s2.box_drawn);
        assert_eq!(s2.default_item, 1);
        assert_eq!(s2.sound_number, 3);

        // Stage 3 (counter 3): nibble 0xF (0b1111) -> box_drawn: true, default_item: 2 (boldItm set), sound: 3
        let s3 = alert_stage_info(0xF721, 3);
        assert_eq!(s3.stage_index, 3);
        assert_eq!(s3.stage_nibble, 0xF);
        assert!(s3.box_drawn);
        assert_eq!(s3.default_item, 2);
        assert_eq!(s3.sound_number, 3);

        // Counter > 3 clamps to stage 3
        let s_clamped = alert_stage_info(0xF721, 10);
        assert_eq!(s_clamped, s3);

        // Truncated ALRT (< 8 bytes) returns None
        assert!(parse_alert_template(&[0u8; 7]).is_none());
    }

    #[test]
    fn param_text_byte_and_str_substitution() {
        let params: [Vec<u8>; 4] = [
            b"first".to_vec(),
            Vec::new(),
            b"third".to_vec(),
            b"fourth".to_vec(),
        ];

        // Byte substitution
        assert_eq!(
            apply_param_text(b"^0/^1/^2/^3", &params).as_ref(),
            b"first//third/fourth"
        );
        assert_eq!(
            apply_param_text(b"^^0 ^9 trailing^", &params).as_ref(),
            b"^first ^9 trailing^"
        );
        assert!(matches!(
            apply_param_text(b"plain", &params),
            Cow::Borrowed(_)
        ));

        // String substitution
        let str_params: [Vec<u8>; 4] = [
            b"MS UserKey".to_vec(),
            b"42".to_vec(),
            Vec::new(),
            Vec::new(),
        ];

        assert_eq!(
            apply_param_text_str("Unable to open the \"^0\" file.", &str_params),
            "Unable to open the \"MS UserKey\" file."
        );
        assert_eq!(apply_param_text_str("count: ^1", &str_params), "count: 42");
        assert_eq!(
            apply_param_text_str("plain text without placeholders", &str_params),
            "plain text without placeholders"
        );
        assert_eq!(
            apply_param_text_str("^0 ^1 ^2 ^3", &str_params),
            "MS UserKey 42  "
        );
        assert_eq!(
            apply_param_text_str("^A literal caret", &str_params),
            "^A literal caret"
        );
        assert_eq!(apply_param_text_str("trailing^", &str_params), "trailing^");
        assert_eq!(apply_param_text_str("^^0", &str_params), "^MS UserKey");
        assert_eq!(apply_param_text_str("", &str_params), "");
    }

    #[test]
    fn modal_dialog_keyboard_navigation_and_filter_evaluation() {
        // Return key: char 0x0D, keycode 0x24
        assert!(is_dialog_default_key(CHAR_RETURN, 0));
        assert!(is_dialog_default_key(0, KEY_RETURN));
        // Enter key: char 0x03, keycode 0x4C
        assert!(is_dialog_default_key(CHAR_ENTER, 0));
        assert!(is_dialog_default_key(0, KEY_NUMPAD_ENTER));
        assert!(!is_dialog_default_key(b'a', 0));

        // Escape: char 0x1B, keycode 0x35
        assert!(is_dialog_cancel_key(CHAR_ESCAPE, 0, 0));
        assert!(is_dialog_cancel_key(0, KEY_ESCAPE, 0));
        // Command-period: char '.' with MODIFIER_CMD_KEY
        assert!(is_dialog_cancel_key(CHAR_PERIOD, 0, MODIFIER_CMD_KEY));
        assert!(is_dialog_cancel_key(0, KEY_PERIOD, MODIFIER_CMD_KEY));
        // Period without Cmd is not cancel
        assert!(!is_dialog_cancel_key(CHAR_PERIOD, 0, 0));

        // Tab: char 0x09, keycode 0x30
        assert!(is_dialog_tab_key(CHAR_TAB, 0));
        assert!(is_dialog_tab_key(0, KEY_TAB));
        assert!(!is_dialog_tab_key(b' ', 0));

        // evaluate_modal_dialog_key
        let return_msg = (u32::from(KEY_RETURN) << 8) | u32::from(CHAR_RETURN);
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, return_msg, 0),
            DialogFilterDecision::TriggerDefaultButton
        );
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_AUTO_KEY, return_msg, 0),
            DialogFilterDecision::TriggerDefaultButton
        );

        let esc_msg = (u32::from(KEY_ESCAPE) << 8) | u32::from(CHAR_ESCAPE);
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, esc_msg, 0),
            DialogFilterDecision::TriggerCancelButton
        );

        let cmd_period_msg = (u32::from(KEY_PERIOD) << 8) | u32::from(CHAR_PERIOD);
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, cmd_period_msg, MODIFIER_CMD_KEY),
            DialogFilterDecision::TriggerCancelButton
        );
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, cmd_period_msg, 0),
            DialogFilterDecision::Unhandled
        );

        let tab_msg = (u32::from(KEY_TAB) << 8) | u32::from(CHAR_TAB);
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, tab_msg, 0),
            DialogFilterDecision::AdvanceEditTextFocus
        );

        // Non-keyboard events (e.g. mouseDown = 1) are Unhandled
        assert_eq!(
            evaluate_modal_dialog_key(1, return_msg, 0),
            DialogFilterDecision::Unhandled
        );
    }

    #[test]
    fn find_next_edit_text_item_cycling() {
        assert_eq!(find_next_edit_text_item([], 0), None);
        assert_eq!(
            find_next_edit_text_item([DIALOG_ITEM_BUTTON, DIALOG_ITEM_STATIC_TEXT], 0),
            None
        );

        // Single editText at index 1 (item 2)
        let single = [
            DIALOG_ITEM_BUTTON,
            DIALOG_ITEM_EDIT_TEXT,
            DIALOG_ITEM_STATIC_TEXT,
        ];
        assert_eq!(find_next_edit_text_item(single, 0), Some(2));
        assert_eq!(find_next_edit_text_item(single, 1), Some(2));
        assert_eq!(find_next_edit_text_item(single, 2), Some(2));
        assert_eq!(find_next_edit_text_item(single, 3), Some(2));

        // Multiple editText items at item 2 and item 4
        let multiple = [
            DIALOG_ITEM_BUTTON,      // 1
            DIALOG_ITEM_EDIT_TEXT,   // 2
            DIALOG_ITEM_STATIC_TEXT, // 3
            DIALOG_ITEM_EDIT_TEXT,   // 4
        ];
        assert_eq!(find_next_edit_text_item(multiple, 0), Some(2));
        assert_eq!(find_next_edit_text_item(multiple, 1), Some(2));
        assert_eq!(find_next_edit_text_item(multiple, 2), Some(4));
        assert_eq!(find_next_edit_text_item(multiple, 3), Some(4));
        assert_eq!(find_next_edit_text_item(multiple, 4), Some(2)); // wraps!

        // Handles disabled flag (0x80)
        let disabled_edit = [
            DIALOG_ITEM_BUTTON,
            DIALOG_ITEM_EDIT_TEXT | DIALOG_ITEM_DISABLED_FLAG,
        ];
        assert_eq!(find_next_edit_text_item(disabled_edit, 0), Some(2));

        // find_next_edit_text_in_items helper
        let item_records = [
            DialogItemRecord {
                item_offset: 0,
                item_type: DIALOG_ITEM_BUTTON,
                rect: (0, 0, 0, 0),
                handle: 0,
                payload: vec![],
            },
            DialogItemRecord {
                item_offset: 0,
                item_type: DIALOG_ITEM_EDIT_TEXT,
                rect: (0, 0, 0, 0),
                handle: 0,
                payload: vec![],
            },
        ];
        assert_eq!(find_next_edit_text_in_items(&item_records, 0), Some(2));
        assert_eq!(find_next_edit_text_in_items(&item_records, 2), Some(2));
    }

    #[test]
    fn dialog_item_text_and_query_operations() {
        // Clamping length and bytes
        assert_eq!(clamp_dialog_item_text_len(10), 10);
        assert_eq!(clamp_dialog_item_text_len(300), 255);

        let short_bytes = b"Hello world";
        assert_eq!(clamp_dialog_item_text_bytes(short_bytes), short_bytes);
        let long_bytes = vec![b'A'; 300];
        let clamped = clamp_dialog_item_text_bytes(&long_bytes);
        assert_eq!(clamped.len(), 255);
        assert_eq!(clamped, &long_bytes[..255]);

        // Pascal string encode & decode
        let (len, text) = encode_dialog_item_pstring(b"Test");
        assert_eq!(len, 4);
        assert_eq!(text, b"Test");

        let mut pstr = vec![4];
        pstr.extend_from_slice(b"Test");
        assert_eq!(decode_dialog_item_pstring(&pstr), Some(&b"Test"[..]));
        assert_eq!(decode_dialog_item_pstring(&[]), None);
        assert_eq!(decode_dialog_item_pstring(&[10, 1, 2]), None); // truncated

        // 1-indexed lookups
        let mut items = vec!["First", "Second", "Third"];
        assert_eq!(get_item_at_1_indexed(&items, 0), None);
        assert_eq!(get_item_at_1_indexed(&items, 1), Some(&"First"));
        assert_eq!(get_item_at_1_indexed(&items, 2), Some(&"Second"));
        assert_eq!(get_item_at_1_indexed(&items, 3), Some(&"Third"));
        assert_eq!(get_item_at_1_indexed(&items, 4), None);

        *get_item_at_1_indexed_mut(&mut items, 2).unwrap() = "Modified";
        assert_eq!(items[1], "Modified");
        assert_eq!(get_item_at_1_indexed_mut(&mut items, 0), None);
        assert_eq!(get_item_at_1_indexed_mut(&mut items, 99), None);

        // Control and text predicates
        assert!(is_dialog_item_control(DIALOG_ITEM_BUTTON));
        assert!(is_dialog_item_control(
            DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(is_dialog_item_control(DIALOG_ITEM_CHECKBOX));
        assert!(is_dialog_item_control(DIALOG_ITEM_RADIO));
        assert!(is_dialog_item_control(DIALOG_ITEM_RESOURCE_CONTROL));
        assert!(!is_dialog_item_control(DIALOG_ITEM_STATIC_TEXT));
        assert!(!is_dialog_item_control(DIALOG_ITEM_EDIT_TEXT));
        assert!(!is_dialog_item_control(DIALOG_ITEM_ICON));

        assert!(is_dialog_item_text(DIALOG_ITEM_STATIC_TEXT));
        assert!(is_dialog_item_text(DIALOG_ITEM_EDIT_TEXT));
        assert!(is_dialog_item_text(
            DIALOG_ITEM_EDIT_TEXT | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(!is_dialog_item_text(DIALOG_ITEM_BUTTON));
        assert!(!is_dialog_item_text(DIALOG_ITEM_ICON));

        // Control procIDs
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_BUTTON), Some(0));
        assert_eq!(
            dialog_item_control_proc_id(DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG),
            Some(0)
        );
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_CHECKBOX), Some(1));
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_RADIO), Some(2));
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_STATIC_TEXT), None);
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_ICON), None);
    }

    #[test]
    fn dialog_selection_normalization_and_alert_helpers() {
        // Select all special cases: (0, -1) and (0, 32767)
        assert_eq!(normalize_dialog_item_selection(0, -1, 10), (0, 10));
        assert_eq!(normalize_dialog_item_selection(0, i16::MAX, 10), (0, 10));
        assert_eq!(normalize_dialog_item_selection(0, -1, 0), (0, 0));

        // Normal in-bounds range
        assert_eq!(normalize_dialog_item_selection(2, 5, 10), (2, 5));
        assert_eq!(normalize_dialog_item_selection(0, 0, 10), (0, 0));
        assert_eq!(normalize_dialog_item_selection(10, 10, 10), (10, 10));

        // Clamping out-of-bounds
        assert_eq!(normalize_dialog_item_selection(-5, 20, 10), (0, 10));
        assert_eq!(normalize_dialog_item_selection(15, 20, 10), (10, 10));

        // Reversed bounds swapping
        assert_eq!(normalize_dialog_item_selection(7, 3, 10), (3, 7));
        assert_eq!(normalize_dialog_item_selection(12, -2, 10), (0, 10));

        // Alert icon IDs
        assert_eq!(alert_icon_id(ALERT_TYPE_STOP), Some(0));
        assert_eq!(alert_icon_id(ALERT_TYPE_NOTE), Some(1));
        assert_eq!(alert_icon_id(ALERT_TYPE_CAUTION), Some(2));
        assert_eq!(alert_icon_id(ALERT_TYPE_PLAIN), None);
        assert_eq!(alert_icon_id(999), None);

        // Standard alert buttons
        assert_eq!(ALERT_BUTTON_OK, 1);
        assert_eq!(ALERT_BUTTON_CANCEL, 2);
        assert_eq!(ALERT_BUTTON_OTHER, 3);
        assert_eq!(ALERT_BUTTON_HELP, 4);

        // Next alert stage capped at 3
        assert_eq!(next_alert_stage(0), 1);
        assert_eq!(next_alert_stage(1), 2);
        assert_eq!(next_alert_stage(2), 3);
        assert_eq!(next_alert_stage(3), 3);
        assert_eq!(next_alert_stage(10), 3);

        // Disposal classification predicates
        assert!(is_dialog_item_disposable_text(DIALOG_ITEM_STATIC_TEXT));
        assert!(is_dialog_item_disposable_text(DIALOG_ITEM_EDIT_TEXT));
        assert!(is_dialog_item_disposable_text(
            DIALOG_ITEM_EDIT_TEXT | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(!is_dialog_item_disposable_text(DIALOG_ITEM_BUTTON));
        assert!(!is_dialog_item_disposable_text(DIALOG_ITEM_ICON));

        assert!(is_dialog_item_disposable_control(DIALOG_ITEM_BUTTON));
        assert!(is_dialog_item_disposable_control(DIALOG_ITEM_CHECKBOX));
        assert!(is_dialog_item_disposable_control(DIALOG_ITEM_RADIO));
        assert!(is_dialog_item_disposable_control(
            DIALOG_ITEM_RESOURCE_CONTROL
        ));
        assert!(is_dialog_item_disposable_control(
            DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(!is_dialog_item_disposable_control(DIALOG_ITEM_STATIC_TEXT));
        assert!(!is_dialog_item_disposable_control(DIALOG_ITEM_ICON));
    }

    #[test]
    fn dialog_dispatch_button_geometry_and_cancel_detection() {
        // DialogRecord chrome and window kind
        assert_eq!(DIALOG_WINDOW_KIND_OFFSET, 108);
        assert_eq!(DIALOG_WINDOW_KIND, 2);
        assert_eq!(DIALOG_GO_AWAY_FLAG_OFFSET, 112);

        // DialogDispatch selectors
        assert_eq!(DIALOG_DISPATCH_NEW_COLOR_DIALOG, 0x0000);
        assert_eq!(DIALOG_DISPATCH_GET_STD_FILTER_PROC, 0x0003);
        assert_eq!(DIALOG_DISPATCH_SET_DIALOG_DEFAULT_ITEM, 0x0004);
        assert_eq!(DIALOG_DISPATCH_SET_DIALOG_CANCEL_ITEM, 0x0005);
        assert_eq!(DIALOG_DISPATCH_SET_DIALOG_TRACKS_CURSOR, 0x0006);
        assert_eq!(DIALOG_DISPATCH_NEW_FEATURES_DIALOG, 0x000C);

        // Default button outline geometry
        assert_eq!(DEFAULT_BUTTON_OUTLINE_THICKNESS, 3);
        assert_eq!(DEFAULT_BUTTON_OUTLINE_INSET, 4);

        let button_rect = (50, 100, 70, 160);
        let (outer, oval) = default_button_outline_geometry(button_rect);
        assert_eq!(outer, (46, 96, 74, 164));
        // height = 74 - 46 = 28; oval = max(4, 28/2 - 4) = 10
        assert_eq!(oval, 10);

        // Cancel button title predicates
        assert!(is_dialog_cancel_button_title(b"Cancel"));
        assert!(is_dialog_cancel_button_title(b"cancel"));
        assert!(is_dialog_cancel_button_title(b"CANCEL"));
        assert!(!is_dialog_cancel_button_title(b"OK"));
        assert!(!is_dialog_cancel_button_title(b""));

        assert!(is_dialog_cancel_button_title_str("Cancel"));
        assert!(is_dialog_cancel_button_title_str("cancel"));
        assert!(!is_dialog_cancel_button_title_str("Dismiss"));

        // Cancel item search across items
        let items: Vec<(u8, &str)> = vec![
            (DIALOG_ITEM_BUTTON, "OK"),
            (DIALOG_ITEM_BUTTON, "Cancel"),
            (DIALOG_ITEM_STATIC_TEXT, "Cancel"),
        ];
        assert_eq!(
            find_dialog_cancel_item_index(items.iter().copied()),
            Some(2)
        );

        // Disabled cancel button is ignored
        let items_disabled: Vec<(u8, &str)> = vec![
            (DIALOG_ITEM_BUTTON, "OK"),
            (DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG, "Cancel"),
        ];
        assert_eq!(
            find_dialog_cancel_item_index(items_disabled.iter().copied()),
            None
        );

        // No cancel button present
        let items_no_cancel: Vec<(u8, &str)> =
            vec![(DIALOG_ITEM_BUTTON, "OK"), (DIALOG_ITEM_BUTTON, "Help")];
        assert_eq!(
            find_dialog_cancel_item_index(items_no_cancel.iter().copied()),
            None
        );
    }

    #[test]
    fn append_and_shorten_ditl_operations_and_game_managed_check() {
        // AppendDITL constants
        assert_eq!(APPEND_DITL_OVERLAY, 0);
        assert_eq!(APPEND_DITL_RIGHT, 1);
        assert_eq!(APPEND_DITL_BOTTOM, 2);

        // append_ditl_offset_delta tests
        let height = 200;
        let width = 300;
        let item_origins = [(20, 30), (50, 60)];

        assert_eq!(
            append_ditl_offset_delta(APPEND_DITL_OVERLAY, height, width, |_| None),
            (0, 0)
        );
        assert_eq!(
            append_ditl_offset_delta(APPEND_DITL_RIGHT, height, width, |_| None),
            (0, 300)
        );
        assert_eq!(
            append_ditl_offset_delta(APPEND_DITL_BOTTOM, height, width, |_| None),
            (200, 0)
        );
        assert_eq!(
            append_ditl_offset_delta(-1, height, width, |idx| item_origins.get(idx - 1).copied()),
            (20, 30)
        );
        assert_eq!(
            append_ditl_offset_delta(-2, height, width, |idx| item_origins.get(idx - 1).copied()),
            (50, 60)
        );
        assert_eq!(
            append_ditl_offset_delta(-3, height, width, |idx| item_origins.get(idx - 1).copied()),
            (0, 0)
        );

        // shorten_ditl_counts tests
        assert_eq!(shorten_ditl_counts(10, 3), (7, 6));
        assert_eq!(shorten_ditl_counts(5, 5), (0, -1));
        assert_eq!(shorten_ditl_counts(5, 10), (0, -1));
        assert_eq!(shorten_ditl_counts(0, 2), (0, -1));

        // rects_intersect tests
        let r1 = (10, 10, 50, 50);
        let r2 = (20, 20, 60, 60);
        let r3 = (60, 60, 100, 100);
        let r4 = (50, 50, 100, 100);
        assert!(rects_intersect(r1, r2));
        assert!(!rects_intersect(r1, r3));
        assert!(!rects_intersect(r1, r4)); // Touching edge only

        // dialog_item_intersects_bounds tests
        let bounds = (100, 100, 300, 400);
        assert!(dialog_item_intersects_bounds(bounds, (10, 10, 50, 50)));
        assert!(!dialog_item_intersects_bounds(bounds, (300, 10, 350, 50)));

        // is_dialog_game_managed tests
        let all_user = vec![
            (DIALOG_ITEM_USER_ITEM, (10, 10, 50, 50)),
            (DIALOG_ITEM_USER_ITEM, (60, 60, 100, 100)),
        ];
        assert!(is_dialog_game_managed(bounds, all_user));

        let with_button = vec![
            (DIALOG_ITEM_USER_ITEM, (10, 10, 50, 50)),
            (DIALOG_ITEM_BUTTON, (60, 60, 100, 100)),
        ];
        assert!(!is_dialog_game_managed(bounds, with_button));

        let button_offscreen = vec![
            (DIALOG_ITEM_USER_ITEM, (10, 10, 50, 50)),
            (DIALOG_ITEM_BUTTON, (300, 300, 350, 350)),
        ];
        assert!(is_dialog_game_managed(bounds, button_offscreen));

        let empty: Vec<(u8, (i16, i16, i16, i16))> = vec![];
        assert!(!is_dialog_game_managed(bounds, empty));
    }

    #[test]
    fn dialog_event_classification_and_textedit_key_processing() {
        let dialog = 0x2000;
        let bounds = (50, 50, 200, 300);

        // is_dialog_event tests
        // None target dialog -> always false
        assert!(!is_dialog_event(
            EVENT_UPDATE,
            dialog,
            0,
            0,
            None,
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_MOUSE_DOWN,
            0,
            100,
            100,
            None,
            Some(bounds)
        ));

        // updateEvt (6) & activateEvt (8) -> match message == dialog
        assert!(is_dialog_event(
            EVENT_UPDATE,
            dialog,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_UPDATE,
            0x9999,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(is_dialog_event(
            EVENT_ACTIVATE,
            dialog,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_ACTIVATE,
            0x1111,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));

        // mouseDown (1) -> within bounds
        assert!(is_dialog_event(
            EVENT_MOUSE_DOWN,
            0,
            100,
            100,
            Some(dialog),
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_MOUSE_DOWN,
            0,
            10,
            10,
            Some(dialog),
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_MOUSE_DOWN,
            0,
            100,
            100,
            Some(dialog),
            None
        ));

        // other events (keyDown, autoKey, null, etc.) -> true if target dialog present
        assert!(is_dialog_event(
            EVENT_KEY_DOWN,
            b'a' as u32,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(is_dialog_event(
            EVENT_AUTO_KEY,
            b'a' as u32,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(is_dialog_event(
            EVENT_NULL,
            0,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));

        // is_dialog_edit_text_character tests
        assert!(is_dialog_edit_text_character(0x08)); // backspace
        assert!(is_dialog_edit_text_character(b' ')); // space (0x20)
        assert!(is_dialog_edit_text_character(b'A'));
        assert!(is_dialog_edit_text_character(b'~')); // 0x7E
        assert!(!is_dialog_edit_text_character(0x00)); // null
        assert!(!is_dialog_edit_text_character(0x0D)); // CR
        assert!(!is_dialog_edit_text_character(0x1B)); // ESC
        assert!(!is_dialog_edit_text_character(0x7F)); // DEL
        assert!(!is_dialog_edit_text_character(0x80));

        // textedit_key_result tests
        let initial = b"Hello World";
        // Insertion at caret (index 5)
        let (res, caret) = textedit_key_result(initial, 5, 5, b',');
        assert_eq!(res, b"Hello, World");
        assert_eq!(caret, 6);

        // Insertion replacing range [5, 11) (" World")
        let (res, caret) = textedit_key_result(initial, 5, 11, b'!');
        assert_eq!(res, b"Hello!");
        assert_eq!(caret, 6);

        // Insertion with reversed endpoints [11, 5)
        let (res, caret) = textedit_key_result(initial, 11, 5, b'!');
        assert_eq!(res, b"Hello!");
        assert_eq!(caret, 6);

        // Backspace at caret > 0
        let (res, caret) = textedit_key_result(initial, 5, 5, 0x08);
        assert_eq!(res, b"Hell World");
        assert_eq!(caret, 4);

        // Backspace deleting selection range [0, 5) ("Hello")
        let (res, caret) = textedit_key_result(initial, 0, 5, 0x08);
        assert_eq!(res, b" World");
        assert_eq!(caret, 0);

        // Backspace at caret 0 (noop)
        let (res, caret) = textedit_key_result(initial, 0, 0, 0x08);
        assert_eq!(res, b"Hello World");
        assert_eq!(caret, 0);
    }

    #[test]
    fn dialog_target_routing_and_frame_geometry() {
        // Target resolution for events
        let front = Some(0x1000);
        let dialog_window = 0x2000;
        let other_window = 0x3000;
        let is_dialog = |ptr| ptr == dialog_window;

        // Update events route to message only if it is a dialog
        assert_eq!(
            dialog_target_for_event(EVENT_UPDATE, dialog_window, is_dialog, front),
            Some(dialog_window)
        );
        assert_eq!(
            dialog_target_for_event(EVENT_UPDATE, other_window, is_dialog, front),
            None
        );

        // Activate events route to message only if it is a dialog
        assert_eq!(
            dialog_target_for_event(EVENT_ACTIVATE, dialog_window, is_dialog, front),
            Some(dialog_window)
        );
        assert_eq!(
            dialog_target_for_event(EVENT_ACTIVATE, other_window, is_dialog, front),
            None
        );

        // Mouse, key, and other events route to front dialog regardless of message
        assert_eq!(
            dialog_target_for_event(EVENT_MOUSE_DOWN, other_window, is_dialog, front),
            front
        );
        assert_eq!(
            dialog_target_for_event(EVENT_KEY_DOWN, other_window, is_dialog, front),
            front
        );
        assert_eq!(
            dialog_target_for_event(EVENT_AUTO_KEY, other_window, is_dialog, front),
            front
        );
        assert_eq!(
            dialog_target_for_event(EVENT_NULL, other_window, is_dialog, front),
            front
        );

        // Inset and outset math
        let base = (10, 20, 100, 200);
        assert_eq!(inset_rect(base, 2, 5), (12, 25, 98, 195));
        assert_eq!(outset_rect(base, 3, 4), (7, 16, 103, 204));

        // Modal dialog dBoxProc frame rect: 8px margin
        assert_eq!(
            dialog_dbox_frame_rect((50, 60, 150, 260)),
            (42, 52, 158, 268)
        );

        // Edit text frame rect: 3px outset
        assert_eq!(edit_text_frame_rect((10, 20, 30, 80)), (7, 17, 33, 83));

        // Dialog text rect: 1px left inset
        assert_eq!(dialog_text_rect((10, 20, 30, 80)), (10, 21, 30, 80));

        // Dialog item base type stripping 0x80
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_USER_ITEM),
            DIALOG_ITEM_USER_ITEM
        );
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_BUTTON),
            DIALOG_ITEM_BUTTON
        );
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG),
            DIALOG_ITEM_BUTTON
        );
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_EDIT_TEXT | DIALOG_ITEM_DISABLED_FLAG),
            DIALOG_ITEM_EDIT_TEXT
        );
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_PICTURE | DIALOG_ITEM_DISABLED_FLAG),
            DIALOG_ITEM_PICTURE
        );
    }
}
