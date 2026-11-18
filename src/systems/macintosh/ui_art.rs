//! Original pixel artwork for Systemless-owned UI chrome.

pub(crate) const RETRO_COMPUTER_MENU_MARK_PIXELS: [[u8; 10]; 12] = [
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

pub(crate) const RETRO_COMPUTER_MENU_MARK_PALETTE: [[u16; 3]; 4] = [
    [0x2222, 0x2222, 0x2222],
    [0xCCCC, 0xBBBB, 0x8888],
    [0x2222, 0x9999, 0xCCCC],
    [0xDDDD, 0xFFFF, 0xFFFF],
];

/// Small monochrome folder glyph drawn before directory names in the
/// Standard File list. Each row is 12 pixels wide, most significant bit
/// leftmost.
pub(crate) const STANDARD_FILE_FOLDER_ICON_ROWS: [u16; 11] =
    [0x000, 0x3c0, 0x420, 0xffe, 0x801, 0x801, 0x801, 0x801, 0x801, 0xfff, 0x000];

/// Small monochrome document glyph drawn before file names in the Standard
/// File list. Same layout as [`STANDARD_FILE_FOLDER_ICON_ROWS`].
pub(crate) const STANDARD_FILE_DOCUMENT_ICON_ROWS: [u16; 11] =
    [0x7e0, 0x420, 0x430, 0x428, 0x427, 0x401, 0x401, 0x401, 0x401, 0x7ff, 0x000];

/// Width in pixels of the Standard File list glyphs.
pub(crate) const STANDARD_FILE_ICON_WIDTH: i16 = 12;
