//! Architecture-neutral Dialog Manager records, DITL parsing, and geometry.
//!
//! Inside Macintosh Volume I (1985), pp. I-399--I-434, and
//! Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-1--6-179.
use std::borrow::Cow;

use crate::trap::types::decode_mac_roman;

/// Canonical guest DialogRecord byte size.
pub const DIALOG_RECORD_SIZE: u32 = 256;

/// Canonical DialogRecord guest field offsets.
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
        assert_eq!(
            apply_param_text_str("^9 unknown slot", &str_params),
            "^9 unknown slot"
        );
        assert_eq!(apply_param_text_str("", &str_params), "");
    }
}
