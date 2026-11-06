/// Last RGBA image delivered to the renderer. A guest can redraw the same
/// pixels between host frames, making its conservative dirty token advance
/// even though the final image is unchanged.
#[derive(Default)]
pub(crate) struct RgbaFrameCache {
    size: Option<(u32, u32)>,
    pixels: Vec<u8>,
    cursor_css: String,
}

impl RgbaFrameCache {
    /// The host applies a native cursor update with a visual packet, so a
    /// changed CSS cursor must accompany a frame even when pixels match.
    pub(crate) fn update_cursor(&mut self, cursor_css: &str) -> bool {
        if self.cursor_css == cursor_css {
            return false;
        }
        self.cursor_css.clear();
        self.cursor_css.push_str(cursor_css);
        true
    }

    /// Keep forced snapshots and changed images; suppress only a byte-exact
    /// repeat of the same geometry. Update the cache before returning so a
    /// later frame compares against what the renderer was last sent.
    pub(crate) fn should_send(&mut self, size: (u32, u32), pixels: &[u8], force: bool) -> bool {
        if !force && self.size == Some(size) && self.pixels == pixels {
            return false;
        }
        self.size = Some(size);
        self.pixels.clear();
        self.pixels.extend_from_slice(pixels);
        true
    }

    /// A different packet format or GPU frame replaces the renderer image.
    pub(crate) fn invalidate(&mut self) {
        self.size = None;
    }
}

#[cfg(test)]
mod tests {
    use super::RgbaFrameCache;

    #[test]
    fn sends_real_changes_and_forced_snapshots() {
        let mut cache = RgbaFrameCache::default();
        assert!(cache.should_send((1, 1), &[1, 2, 3, 255], false));
        assert!(!cache.should_send((1, 1), &[1, 2, 3, 255], false));
        assert!(cache.should_send((1, 1), &[1, 2, 4, 255], false));
        assert!(cache.should_send((1, 1), &[1, 2, 4, 255], true));
        assert!(cache.should_send((2, 1), &[1, 2, 4, 255], false));
    }

    #[test]
    fn sends_again_after_another_presentation_format() {
        let mut cache = RgbaFrameCache::default();
        assert!(cache.should_send((1, 1), &[1, 2, 3, 255], false));
        cache.invalidate();
        assert!(cache.should_send((1, 1), &[1, 2, 3, 255], false));
    }

    #[test]
    fn changed_native_cursor_requires_a_packet_with_unchanged_pixels() {
        let mut cache = RgbaFrameCache::default();
        let pixels = [1, 2, 3, 255];
        assert!(cache.update_cursor("default"));
        assert!(cache.should_send((1, 1), &pixels, false));
        assert!(!cache.update_cursor("default"));
        assert!(!cache.should_send((1, 1), &pixels, false));
        let cursor_changed = cache.update_cursor("pointer");
        assert!(cursor_changed);
        assert!(cache.should_send((1, 1), &pixels, cursor_changed));
    }
}
