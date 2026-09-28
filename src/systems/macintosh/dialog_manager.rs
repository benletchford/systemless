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

/// Parse raw DITL resource bytes into a list of parsed dialog item records.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-120--6-121.
pub fn parse_ditl_items(bytes: &[u8]) -> Option<Vec<DialogItemRecord>> {
    let count_minus_one = read_be_i16(bytes, 0)?;
    let count = if count_minus_one < 0 {
        0
    } else {
        usize::try_from(count_minus_one).ok()?.checked_add(1)?
    };

    let mut offset = 2usize;
    let mut items = Vec::with_capacity(count);

    for _ in 0..count {
        let handle = read_be_u32(bytes, offset)?;
        let top = read_be_i16(bytes, offset.checked_add(4)?)?;
        let left = read_be_i16(bytes, offset.checked_add(6)?)?;
        let bottom = read_be_i16(bytes, offset.checked_add(8)?)?;
        let right = read_be_i16(bytes, offset.checked_add(10)?)?;
        let item_type = *bytes.get(offset.checked_add(12)?)?;
        let payload_len = usize::from(*bytes.get(offset.checked_add(13)?)?);
        let payload_start = offset.checked_add(14)?;
        let payload_end = payload_start.checked_add(payload_len)?;
        let payload = bytes.get(payload_start..payload_end)?.to_vec();

        items.push(DialogItemRecord {
            item_offset: offset,
            item_type,
            rect: (top, left, bottom, right),
            handle,
            payload,
        });

        // Advance past data, padded to 2-byte word boundary.
        offset = (payload_end + 1) & !1;
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
}
