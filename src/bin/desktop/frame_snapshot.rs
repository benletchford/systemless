//! Complete presentation data owned independently of the Macintosh runner.

use std::sync::Arc;
use systemless::display::{CursorImage, PackedScreenFrame};
use systemless::memory::CompactPresentation;
#[cfg(target_os = "macos")]
use systemless::trap::dispatch::ScreenCopyBitsRect;

#[derive(Default)]
pub(super) struct GuiFrame {
    pub generation: u64,
    pub sequence: u64,
    pub display_generation: u64,
    pub guest_tick: u32,
    pub screen: PackedScreenFrame,
    pub retained: Option<Arc<CompactPresentation>>,
    pub cursor: Option<CursorImage>,
    pub mouse_position: (i16, i16),
    pub debug_lines: Vec<String>,
    #[cfg(target_os = "macos")]
    pub crop: CropObservations,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
pub(super) struct CropObservations {
    pub dialog_bounds: Option<(i16, i16, i16, i16)>,
    pub framed_rect: Option<ScreenCopyBitsRect>,
    pub manual_rect: Option<ScreenCopyBitsRect>,
    pub declared_rect: Option<ScreenCopyBitsRect>,
    pub copybits_count: u64,
    pub last_copybits_rect: Option<ScreenCopyBitsRect>,
    pub hidden_menu_height: u32,
}

/// Reuse pure expansion for an unchanged owned retained image and drawable.
/// Holding the source Arc prevents allocator address reuse from aliasing a key.
#[derive(Default)]
pub(super) struct RetainedFrameCache {
    source: Option<Arc<CompactPresentation>>,
    size: (u32, u32),
    pixels: Vec<u32>,
}

impl RetainedFrameCache {
    pub fn render(
        &mut self,
        source: &Arc<CompactPresentation>,
        size: (u32, u32),
        output: &mut Vec<u32>,
    ) -> bool {
        if !self.prepare(source, size) {
            return false;
        }
        output.clone_from(&self.pixels);
        true
    }
    pub fn prepare(&mut self, source: &Arc<CompactPresentation>, size: (u32, u32)) -> bool {
        if self.size != size
            || !self
                .source
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, source))
        {
            if !source.render_argb_resized(size, &mut self.pixels) {
                return false;
            }
            self.source = Some(source.clone());
            self.size = size;
        }
        true
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_cache_reuses_owned_images_and_replaces_geometry_and_source() {
        let mut cache = RetainedFrameCache::default();
        let mut image = Arc::new(CompactPresentation {
            width: 1,
            height: 1,
            scale: 1,
            cells: vec![0x123456],
            detail: vec![],
        });
        let mut pixels = Vec::new();
        assert!(cache.render(&image, (2, 3), &mut pixels));
        assert_eq!(pixels, vec![0xff123456; 6]);
        assert!(cache.render(&image, (2, 3), &mut pixels));
        Arc::make_mut(&mut image).cells[0] = 0xaabbcc;
        assert!(cache.render(&image, (3, 2), &mut pixels));
        assert_eq!(pixels, vec![0xffaabbcc; 6]);
        assert!(cache.render(&image, (1, 1), &mut pixels));
        assert_eq!(pixels, [0xffaabbcc]);
        let invalid = Arc::new(CompactPresentation::default());
        assert!(!cache.render(&invalid, (1, 1), &mut pixels));
        assert_eq!(pixels, [0xffaabbcc]);
        assert!(cache.render(&image, (1, 1), &mut pixels));
    }
}
