//! Architecture-neutral Cursor and Icon Manager operations and parameter evaluation.
//!
//! Inside Macintosh Volume I (1985), chapter 6 "QuickDraw", pp. I-167--I-168;
//! Inside Macintosh Volume I (1985), chapter 16 "The Resource Manager", pp. I-474--I-477;
//! Inside Macintosh Volume V (1986), chapter 4 "Color QuickDraw", pp. V-75--V-80;
//! Inside Macintosh: Imaging With QuickDraw (1994), chapter 8 "Cursor Utilities", pp. 8-3--8-36.
//!
//! The Cursor and Icon Utilities handle standard monochrome cursors (16×16 bitmaps),
//! color cursors (`CCrsr`), color icons (`CIcon`), and cursor visibility tracking.
//! The canonical evaluation functions below formalize the ABI contracts across 68k traps
//! ($A850 InitCursor, $A851 SetCursor, $A852 HideCursor, $A853 ShowCursor, $A855 ShieldCursor,
//! $A856 ObscureCursor, $A9B9 GetCursor, $AA1A GetCCursor, $AA1B SetCCursor, $AA1E GetCIcon,
//! $AA1F PlotCIcon, $AA25 DisposeCIcon, $AA26 DisposeCCursor) and PowerPC CFM imports.

/// Size of a classic Macintosh `Cursor` record in bytes (Inside Macintosh I-167, Imaging With QuickDraw 8-19).
/// Contains 16×16 data (32 bytes), 16×16 mask (32 bytes), and Point hotSpot (4 bytes).
pub const CURSOR_RECORD_SIZE: usize = 68;

/// Byte offset of the 16×16 data bitmap in a `Cursor` record.
pub const CURSOR_DATA_OFFSET: usize = 0;

/// Byte offset of the 16×16 mask bitmap in a `Cursor` record.
pub const CURSOR_MASK_OFFSET: usize = 32;

/// Byte offset of the vertical coordinate of the hotSpot in a `Cursor` record.
pub const CURSOR_HOT_V_OFFSET: usize = 64;

/// Byte offset of the horizontal coordinate of the hotSpot in a `Cursor` record.
pub const CURSOR_HOT_H_OFFSET: usize = 66;

/// Size of each 16×16 1-bit cursor bitmap (data or mask) in bytes.
pub const CURSOR_BITMAP_SIZE: usize = 32;

/// Standard cursor width in pixels.
pub const CURSOR_WIDTH: i16 = 16;

/// Standard cursor height in pixels.
pub const CURSOR_HEIGHT: i16 = 16;

/// Standard resource ID for the I-beam text editing cursor (Inside Macintosh I-475).
#[allow(dead_code)]
pub const IBEAM_CURSOR_ID: i16 = 1;

/// Standard resource ID for the crosshair cursor (Inside Macintosh I-476).
#[allow(dead_code)]
pub const CROSS_CURSOR_ID: i16 = 2;

/// Standard resource ID for the plus-sign cursor (Inside Macintosh I-476).
#[allow(dead_code)]
pub const PLUS_CURSOR_ID: i16 = 3;

/// Standard resource ID for the wristwatch wait cursor (Inside Macintosh I-477).
#[allow(dead_code)]
pub const WATCH_CURSOR_ID: i16 = 4;

/// Evaluates invocation of `InitCursor`.
///
/// Per IM:I I-167: `PROCEDURE InitCursor;`
/// Sets the standard arrow cursor and makes it visible (resets cursor level to 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitCursorAction;

/// Evaluates canonical `InitCursor` invocation.
#[inline]
pub fn evaluate_init_cursor() -> InitCursorAction {
    InitCursorAction
}

/// Evaluates invocation of `HideCursor`.
///
/// Per IM:I I-168: `PROCEDURE HideCursor;`
/// Decrements the cursor level by 1, hiding the cursor when level < 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HideCursorAction;

/// Evaluates canonical `HideCursor` invocation.
#[inline]
pub fn evaluate_hide_cursor() -> HideCursorAction {
    HideCursorAction
}

/// Evaluates invocation of `ShowCursor`.
///
/// Per IM:I I-168: `PROCEDURE ShowCursor;`
/// Increments the cursor level toward 0; unhides cursor when level reaches 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowCursorAction;

/// Evaluates canonical `ShowCursor` invocation.
#[inline]
pub fn evaluate_show_cursor() -> ShowCursorAction {
    ShowCursorAction
}

/// Evaluates invocation of `ObscureCursor`.
///
/// Per IM:I I-168: `PROCEDURE ObscureCursor;`
/// Hides the cursor temporarily until mouse moves without changing the cursor level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObscureCursorAction;

/// Evaluates canonical `ObscureCursor` invocation.
#[inline]
pub fn evaluate_obscure_cursor() -> ObscureCursorAction {
    ObscureCursorAction
}

/// Evaluates canonical parameters for `SetCursor`.
///
/// Per IM:I I-167: `PROCEDURE SetCursor(crsr: Cursor);`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetCursorParameters {
    pub cursor_ptr: u32,
}

/// Evaluates canonical parameters for `SetCursor`.
#[inline]
pub fn evaluate_set_cursor_parameters(cursor_ptr: u32) -> SetCursorParameters {
    SetCursorParameters { cursor_ptr }
}

/// Evaluates canonical parameters for `ShieldCursor`.
///
/// Per IM:I I-474, Imaging With QuickDraw 8-29:
/// `PROCEDURE ShieldCursor(shieldRect: Rect; offsetPt: Point);`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShieldCursorParameters {
    pub rect_ptr: u32,
    pub offset_v: i16,
    pub offset_h: i16,
}

/// Evaluates canonical parameters for `ShieldCursor`.
#[inline]
pub fn evaluate_shield_cursor_parameters(
    rect_ptr: u32,
    offset_v: i16,
    offset_h: i16,
) -> ShieldCursorParameters {
    ShieldCursorParameters {
        rect_ptr,
        offset_v,
        offset_h,
    }
}

/// Evaluates whether a cursor's screen bounding box overlaps a shielded rectangle.
///
/// `shield_rect` is in (top, left, bottom, right) order.
/// `offset` is (offset_v, offset_h).
/// `mouse_pos` is (mouse_v, mouse_h).
/// `hot_spot` is (hot_v, hot_h).
#[inline]
pub fn evaluate_cursor_shield_overlap(
    shield_rect: (i16, i16, i16, i16),
    offset: (i16, i16),
    mouse_pos: (i16, i16),
    hot_spot: (i16, i16),
) -> bool {
    let shield_top = shield_rect.0.wrapping_add(offset.0);
    let shield_left = shield_rect.1.wrapping_add(offset.1);
    let shield_bottom = shield_rect.2.wrapping_add(offset.0);
    let shield_right = shield_rect.3.wrapping_add(offset.1);

    let cursor_top = mouse_pos.0.saturating_sub(hot_spot.0);
    let cursor_left = mouse_pos.1.saturating_sub(hot_spot.1);
    let cursor_bottom = cursor_top.saturating_add(CURSOR_HEIGHT);
    let cursor_right = cursor_left.saturating_add(CURSOR_WIDTH);

    cursor_top < shield_bottom
        && cursor_bottom > shield_top
        && cursor_left < shield_right
        && cursor_right > shield_left
}

/// Evaluates canonical parameters for `GetCursor`.
///
/// Per IM:I I-474: `FUNCTION GetCursor(cursorID: INTEGER): CursHandle;`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetCursorParameters {
    pub cursor_id: i16,
}

/// Evaluates canonical parameters for `GetCursor`.
#[inline]
pub fn evaluate_get_cursor_parameters(cursor_id: i16) -> GetCursorParameters {
    GetCursorParameters { cursor_id }
}

/// Evaluates canonical parameters for `GetCCursor`.
///
/// Per IM:V V-75: `FUNCTION GetCCursor(cursorID: INTEGER): CCrsrHandle;`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetCCursorParameters {
    pub cursor_id: i16,
}

/// Evaluates canonical parameters for `GetCCursor`.
#[inline]
pub fn evaluate_get_ccursor_parameters(cursor_id: i16) -> GetCCursorParameters {
    GetCCursorParameters { cursor_id }
}

/// Evaluates canonical parameters for `SetCCursor`.
///
/// Per IM:V V-76: `PROCEDURE SetCCursor(cCrsr: CCrsrHandle);`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetCCursorParameters {
    pub cursor_handle: u32,
}

/// Evaluates canonical parameters for `SetCCursor`.
#[inline]
pub fn evaluate_set_ccursor_parameters(cursor_handle: u32) -> SetCCursorParameters {
    SetCCursorParameters { cursor_handle }
}

/// Evaluates canonical parameters for `DisposeCCursor` (`DisposCCursor`).
///
/// Per IM:V V-77: `PROCEDURE DisposCCursor(cCrsr: CCrsrHandle);`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisposeCCursorParameters {
    pub cursor_handle: u32,
}

/// Evaluates canonical parameters for `DisposeCCursor`.
#[inline]
pub fn evaluate_dispose_ccursor_parameters(cursor_handle: u32) -> DisposeCCursorParameters {
    DisposeCCursorParameters { cursor_handle }
}

/// Evaluates canonical parameters for `GetCIcon`.
///
/// Per IM:V V-78: `FUNCTION GetCIcon(iconID: INTEGER): CIconHandle;`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetCIconParameters {
    pub icon_id: i16,
}

/// Evaluates canonical parameters for `GetCIcon`.
#[inline]
pub fn evaluate_get_cicon_parameters(icon_id: i16) -> GetCIconParameters {
    GetCIconParameters { icon_id }
}

/// Evaluates canonical parameters for `PlotCIcon`.
///
/// Per IM:V V-79: `PROCEDURE PlotCIcon(theRect: Rect; theIcon: CIconHandle);`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlotCIconParameters {
    pub rect_ptr: u32,
    pub cicon_handle: u32,
}

/// Evaluates canonical parameters for `PlotCIcon`.
#[inline]
pub fn evaluate_plot_cicon_parameters(rect_ptr: u32, cicon_handle: u32) -> PlotCIconParameters {
    PlotCIconParameters {
        rect_ptr,
        cicon_handle,
    }
}

/// Evaluates canonical parameters for `DisposeCIcon` (`DisposCIcon`).
///
/// Per IM:V V-79: `PROCEDURE DisposCIcon(theIcon: CIconHandle);`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisposeCIconParameters {
    pub cicon_handle: u32,
}

/// Evaluates canonical parameters for `DisposeCIcon`.
#[inline]
pub fn evaluate_dispose_cicon_parameters(cicon_handle: u32) -> DisposeCIconParameters {
    DisposeCIconParameters { cicon_handle }
}

/// Evaluates canonical parameters for Carbon `GetQDGlobalsArrow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetQDGlobalsArrowParameters {
    pub destination_ptr: u32,
}

/// Evaluates canonical parameters for `GetQDGlobalsArrow`.
#[inline]
pub fn evaluate_get_qd_globals_arrow_parameters(
    destination_ptr: u32,
) -> GetQDGlobalsArrowParameters {
    GetQDGlobalsArrowParameters { destination_ptr }
}

/// Evaluates canonical parameters for `CrsrDevNextDevice`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrsrDevNextDeviceParameters {
    pub device_ptr: u32,
}

/// Evaluates canonical parameters for `CrsrDevNextDevice`.
#[inline]
pub fn evaluate_crsr_dev_next_device_parameters(device_ptr: u32) -> CrsrDevNextDeviceParameters {
    CrsrDevNextDeviceParameters { device_ptr }
}

/// Evaluates canonical parameters for `CrsrDevMoveTo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrsrDevMoveToParameters {
    pub device_ptr: u32,
    pub x: i32,
    pub y: i32,
}

/// Evaluates canonical parameters for `CrsrDevMoveTo`.
#[inline]
pub fn evaluate_crsr_dev_move_to_parameters(
    device_ptr: u32,
    x: i32,
    y: i32,
) -> CrsrDevMoveToParameters {
    CrsrDevMoveToParameters { device_ptr, x, y }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_and_icon_manager_evaluation() {
        assert_eq!(CURSOR_RECORD_SIZE, 68);
        assert_eq!(CURSOR_DATA_OFFSET, 0);
        assert_eq!(CURSOR_MASK_OFFSET, 32);
        assert_eq!(CURSOR_HOT_V_OFFSET, 64);
        assert_eq!(CURSOR_HOT_H_OFFSET, 66);
        assert_eq!(CURSOR_BITMAP_SIZE, 32);
        assert_eq!(CURSOR_WIDTH, 16);
        assert_eq!(CURSOR_HEIGHT, 16);

        assert_eq!(IBEAM_CURSOR_ID, 1);
        assert_eq!(CROSS_CURSOR_ID, 2);
        assert_eq!(PLUS_CURSOR_ID, 3);
        assert_eq!(WATCH_CURSOR_ID, 4);

        assert_eq!(evaluate_init_cursor(), InitCursorAction);
        assert_eq!(evaluate_hide_cursor(), HideCursorAction);
        assert_eq!(evaluate_show_cursor(), ShowCursorAction);
        assert_eq!(evaluate_obscure_cursor(), ObscureCursorAction);

        let set_cursor = evaluate_set_cursor_parameters(0x1234_5678);
        assert_eq!(set_cursor.cursor_ptr, 0x1234_5678);

        let shield_params = evaluate_shield_cursor_parameters(0x2000, 10, 20);
        assert_eq!(shield_params.rect_ptr, 0x2000);
        assert_eq!(shield_params.offset_v, 10);
        assert_eq!(shield_params.offset_h, 20);

        // Overlap testing
        // Shield rect: (50, 50, 100, 100), offset (0, 0), mouse (60, 60), hotSpot (0, 0) -> overlaps
        assert!(evaluate_cursor_shield_overlap(
            (50, 50, 100, 100),
            (0, 0),
            (60, 60),
            (0, 0)
        ));
        // Mouse outside rect (e.g. 200, 200) -> does not overlap
        assert!(!evaluate_cursor_shield_overlap(
            (50, 50, 100, 100),
            (0, 0),
            (200, 200),
            (0, 0)
        ));
        // Overlap with offset: shield (0, 0, 50, 50) + offset (100, 100) -> effective shield (100, 100, 150, 150)
        assert!(evaluate_cursor_shield_overlap(
            (0, 0, 50, 50),
            (100, 100),
            (120, 120),
            (0, 0)
        ));

        let get_cursor = evaluate_get_cursor_parameters(4);
        assert_eq!(get_cursor.cursor_id, 4);

        let get_ccursor = evaluate_get_ccursor_parameters(128);
        assert_eq!(get_ccursor.cursor_id, 128);

        let set_ccursor = evaluate_set_ccursor_parameters(0x8000_1000);
        assert_eq!(set_ccursor.cursor_handle, 0x8000_1000);

        let dispose_ccursor = evaluate_dispose_ccursor_parameters(0x8000_1000);
        assert_eq!(dispose_ccursor.cursor_handle, 0x8000_1000);

        let get_cicon = evaluate_get_cicon_parameters(256);
        assert_eq!(get_cicon.icon_id, 256);

        let plot_cicon = evaluate_plot_cicon_parameters(0x3000, 0x8000_2000);
        assert_eq!(plot_cicon.rect_ptr, 0x3000);
        assert_eq!(plot_cicon.cicon_handle, 0x8000_2000);

        let dispose_cicon = evaluate_dispose_cicon_parameters(0x8000_2000);
        assert_eq!(dispose_cicon.cicon_handle, 0x8000_2000);

        let qd_arrow = evaluate_get_qd_globals_arrow_parameters(0x4000);
        assert_eq!(qd_arrow.destination_ptr, 0x4000);

        let next_dev = evaluate_crsr_dev_next_device_parameters(0x5000);
        assert_eq!(next_dev.device_ptr, 0x5000);

        let move_to = evaluate_crsr_dev_move_to_parameters(0x5000, 100, 200);
        assert_eq!(move_to.device_ptr, 0x5000);
        assert_eq!(move_to.x, 100);
        assert_eq!(move_to.y, 200);
    }
}
