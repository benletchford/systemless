//! PowerPC graphics, GWorld, and color representation records.

pub(crate) use crate::control_manager::ProcessControlRecord as PpcControlRecord;
use crate::menu_manager::MenuTrackingSurface;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcRgbColor {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
}

pub const PPC_RGB_BLACK: PpcRgbColor = PpcRgbColor {
    red: 0,
    green: 0,
    blue: 0,
};

pub const PPC_RGB_WHITE: PpcRgbColor = PpcRgbColor {
    red: 0xffff,
    green: 0xffff,
    blue: 0xffff,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcGWorldRecord {
    /// Host presentation provider; the main screen record selects native Toolbox chrome.
    pub ui_theme: crate::ui_theme::UiThemeId,
    pub port: u32,
    pub pixmap_handle: u32,
    pub pixmap: u32,
    pub base_addr: u32,
    pub gdevice: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub row_bytes: u32,
    /// Compatibility mirror only. The process-owned PixMapHandle registry is
    /// authoritative for LockPixels/GetPixelsState and is synchronized at
    /// native import boundaries. Inside Macintosh: Imaging With QuickDraw
    /// (1994), pp. 6-32--6-38.
    pub pixels_locked: bool,
    /// Compatibility mirror only. The process-owned PixMapHandle registry is
    /// authoritative for purgeability state and is synchronized at native
    /// import boundaries. Inside Macintosh: Imaging With QuickDraw (1994),
    /// pp. 6-34--6-38.
    pub pixels_no_purge: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcGWorldAllocationRecord {
    pub(crate) storage_ptr: u32,
    pub(crate) pixel_ptr: u32,
    pub(crate) origin_base: u32,
    pub(crate) pixel_capacity: u32,
    pub(crate) ctable_handle: u32,
    pub(crate) allocation_end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcFrontBuffer {
    pub base_addr: u32,
    pub row_bytes: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

impl From<PpcFrontBuffer> for MenuTrackingSurface {
    fn from(front: PpcFrontBuffer) -> Self {
        Self {
            base_addr: front.base_addr,
            row_bytes: front.row_bytes,
            width: front.width,
            height: front.height,
            depth: front.depth,
        }
    }
}

impl From<MenuTrackingSurface> for PpcFrontBuffer {
    fn from(surface: MenuTrackingSurface) -> Self {
        Self {
            base_addr: surface.base_addr,
            row_bytes: surface.row_bytes,
            width: surface.width,
            height: surface.height,
            depth: surface.depth,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcQuickDrawSurface {
    pub(crate) front_buffer: PpcFrontBuffer,
    pub(crate) top: i16,
    pub(crate) left: i16,
    pub(crate) ctable_handle: Option<u32>,
}

impl PpcQuickDrawSurface {
    pub(crate) fn local_point(self, (h, v): (i32, i32)) -> (i32, i32) {
        (h - i32::from(self.left), v - i32::from(self.top))
    }

    pub(crate) fn local_rect(
        self,
        (top, left, bottom, right): (i16, i16, i16, i16),
    ) -> (i32, i32, i32, i32) {
        (
            i32::from(top) - i32::from(self.top),
            i32::from(left) - i32::from(self.left),
            i32::from(bottom) - i32::from(self.top),
            i32::from(right) - i32::from(self.left),
        )
    }

    pub(crate) fn local_rect_i16(
        self,
        (top, left, bottom, right): (i16, i16, i16, i16),
    ) -> (i16, i16, i16, i16) {
        (
            top.wrapping_sub(self.top),
            left.wrapping_sub(self.left),
            bottom.wrapping_sub(self.top),
            right.wrapping_sub(self.left),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcAliasRecord {
    pub handle: u32,
    pub target_vref: i16,
    pub target_dir_id: u32,
    pub target_name: Vec<u8>,
}
