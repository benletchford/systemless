//! Architecture-neutral Dialog Manager records, DITL parsing, and geometry.
//!
//! Inside Macintosh Volume I (1985), pp. I-399--I-434, and
//! Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-1--6-179.

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
}
