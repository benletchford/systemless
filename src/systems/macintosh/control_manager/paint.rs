//! Original guest pixels beneath a completed standard CDEF drawing.
//!
//! Capture both sides of the actual draw, rather than guessing a panel colour
//! from its corners. Consumers must validate surface/identity and the current
//! completed raster before replacing control ink. This supports patterned and
//! application-painted backgrounds without making their pixels host-owned.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControlPaintFormat {
    Rgba,
    /// Palette index in byte zero; the remaining bytes are canonical padding.
    /// Resolve against the actual display CLUT only when presenting a snapshot.
    Indexed8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ControlPaintIdentity {
    pub owner: u32,
    pub generation: u64,
    pub surface: u32,
    pub bounds: (i16, i16, i16, i16),
    pub depth: u16,
    pub palette: u64,
    pub format: ControlPaintFormat,
    pub recipe: [u8; 268],
}

/// Exact paint-relevant guest fields, excluding unused title storage. This also
/// catches direct application writes which bypass Control Manager setters.
pub(crate) fn control_recipe(pointer: u32, mut read: impl FnMut(u32) -> Option<u8>) -> Option<[u8; 268]> {
    let mut recipe = [0; 268];
    for (index, byte) in recipe[..12].iter_mut().enumerate() {
        *byte = read(pointer.checked_add(16 + index as u32)?)?;
    }
    let length = read(pointer.checked_add(40)?)?;
    recipe[12] = length;
    for index in 0..usize::from(length) {
        recipe[13 + index] = read(pointer.checked_add(41 + index as u32)?)?;
    }
    Some(recipe)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ControlPaint {
    identity: ControlPaintIdentity,
    backdrop: Vec<u8>,
    completed: Vec<u8>,
}

impl ControlPaint {
    /// Before/after are complete row-major four-byte pixels from the same guest
    /// surface in the declared format. Capture must preserve guest clipping.
    pub(crate) fn capture(
        identity: ControlPaintIdentity,
        before: Vec<u8>,
        completed: Vec<u8>,
        previous: Option<&Self>,
    ) -> Option<Self> {
        let (top, left, bottom, right) = identity.bounds;
        let width = usize::try_from(i32::from(right) - i32::from(left)).ok()?;
        let height = usize::try_from(i32::from(bottom) - i32::from(top)).ok()?;
        let length = width.checked_mul(height)?.checked_mul(4)?;
        if length == 0 || before.len() != length || completed.len() != length {
            return None;
        }
        let mut backdrop = before;
        if let Some(previous) = previous {
            let mut previous_surface = previous.identity;
            let mut surface = identity;
            previous_surface.recipe = [0; 268];
            surface.recipe = [0; 268];
            if previous_surface == surface && backdrop == previous.completed {
                backdrop = previous.backdrop.clone();
            } else if !Self::uniform(&backdrop) {
                // A partly changed raster cannot distinguish new application
                // paint from old CDEF ink. Do not recover pixels by colour alone.
                return None;
            }
        }
        Some(Self { identity, backdrop, completed })
    }

    fn uniform(pixels: &[u8]) -> bool {
        pixels.chunks_exact(4).next().is_some_and(|first|
            pixels.chunks_exact(4).all(|pixel| pixel == first))
    }

    /// A changed raster is insufficient ownership evidence. Keep guest pixels
    /// until a subsequent genuine CDEF draw establishes another complete record.
    pub(crate) fn backdrop(&self, identity: ControlPaintIdentity, current: &[u8]) -> Option<&[u8]> {
        (self.identity == identity && self.completed == current).then_some(self.backdrop.as_slice())
    }
}

/// Clones of a process control record share completed draw evidence. Disposal
/// drops the record's slot; a reused guest handle receives a fresh generation.
#[derive(Clone, Debug, Default)]
pub(crate) struct ControlPaintSlot(std::sync::Arc<std::sync::Mutex<ControlPaintState>>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct ControlPaintState {
    drawing: Option<ControlPaint>,
    fresh: bool,
    presentation_stale: bool,
}
impl Default for ControlPaintState {
    fn default() -> Self { Self { drawing: None, fresh: true, presentation_stale: false } }
}

impl PartialEq for ControlPaintSlot {
    fn eq(&self, other: &Self) -> bool {
        if std::sync::Arc::ptr_eq(&self.0, &other.0) { return true; }
        // Never hold both locks: control snapshots can be compared by workers.
        let left = self.0.lock().ok().map(|slot| slot.clone());
        let right = other.0.lock().ok().map(|slot| slot.clone());
        left == right
    }
}
impl Eq for ControlPaintSlot {}

impl ControlPaintSlot {
    pub(crate) fn has_drawing(&self) -> bool {
        self.0.lock().ok().is_some_and(|slot| slot.drawing.is_some() && !slot.presentation_stale)
    }

    /// Preserve the old backdrop for redraw recovery, but decline presentation
    /// until a genuine CDEF draw establishes the updated font recipe.
    pub(crate) fn invalidate_presentation(&self) {
        if let Ok(mut slot) = self.0.lock() { slot.presentation_stale = true; }
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut slot) = self.0.lock() { slot.drawing = None; slot.fresh = false; }
    }

    pub(crate) fn record(&self, identity: ControlPaintIdentity, before: Vec<u8>, completed: Vec<u8>) {
        let Ok(mut slot) = self.0.lock() else { return; };
        let eligible = slot.fresh || slot.drawing.is_some() || ControlPaint::uniform(&before);
        slot.drawing = if eligible {
            ControlPaint::capture(identity, before, completed, slot.drawing.as_ref())
        } else { None };
        slot.fresh = false;
        slot.presentation_stale = false;
    }

    pub(crate) fn backdrop(&self, identity: ControlPaintIdentity, current: &[u8]) -> Option<Vec<u8>> {
        let slot = self.0.lock().ok()?;
        if slot.presentation_stale { return None; }
        slot.drawing.as_ref()?.backdrop(identity, current).map(<[u8]>::to_vec)
    }

    pub(crate) fn rgba_backdrop(&self, identity: ControlPaintIdentity, current: &[u8],
        display_palette: &[u32; 256]) -> Option<std::sync::Arc<[u8]>> {
        let mut pixels = self.backdrop(identity, current)?;
        if identity.format == ControlPaintFormat::Indexed8 {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.copy_from_slice(&display_palette[usize::from(pixel[0])].to_le_bytes());
            }
        }
        Some(pixels.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> ControlPaintIdentity {
        ControlPaintIdentity { owner: 10, generation: 1, surface: 20,
            bounds: (0, 0, 1, 3), depth: 8, palette: 0, format: ControlPaintFormat::Rgba, recipe: [0; 268] }
    }
    fn pixels(colours: [[u8; 3]; 3]) -> Vec<u8> {
        colours.into_iter().flat_map(|[r, g, b]| [r, g, b, 255]).collect()
    }

    #[test]
    fn recipe_tracks_guest_title_and_state_without_unused_title_padding() {
        let mut memory = vec![0u8; 300];
        memory[40] = 3; memory[41..44].copy_from_slice(b"ABC");
        let original = control_recipe(0, |address| memory.get(address as usize).copied()).unwrap();
        memory[44] = 99;
        assert_eq!(control_recipe(0, |address| memory.get(address as usize).copied()), Some(original));
        memory[18] = 1;
        assert_ne!(control_recipe(0, |address| memory.get(address as usize).copied()), Some(original));
        memory[18] = 0; memory[42] = b'Z';
        assert_ne!(control_recipe(0, |address| memory.get(address as usize).copied()), Some(original));
        assert!(control_recipe(u32::MAX - 10, |_| Some(0)).is_none());
    }

    #[test]
    fn changed_recipe_declines_presentation_but_recovers_backdrop_after_redraw() {
        let slot = ControlPaintSlot::default();
        let original = identity();
        let before = pixels([[238, 238, 238]; 3]);
        let completed = pixels([[0, 0, 0], [238, 238, 238], [0, 0, 0]]);
        slot.record(original, before.clone(), completed.clone());
        let mut changed = original; changed.recipe[1] = 255;
        assert!(slot.backdrop(changed, &completed).is_none());
        slot.record(changed, completed.clone(), completed.clone());
        assert_eq!(slot.backdrop(changed, &completed), Some(before));
    }

    #[test]
    fn retains_pattern_for_repeated_draws_and_declines_ambiguous_partial_repaint() {
        let original = pixels([[238; 3], [0, 0, 255], [238; 3]]);
        let first = pixels([[0; 3], [0, 0, 255], [238; 3]]);
        let held = ControlPaint::capture(identity(), original.clone(), first.clone(), None).unwrap();
        let repeated = ControlPaint::capture(identity(), first.clone(), first.clone(), Some(&held)).unwrap();
        assert_eq!(repeated.backdrop(identity(), &first), Some(original.as_slice()));
        let partial = pixels([[0; 3], [0, 0, 255], [255, 0, 0]]);
        assert!(ControlPaint::capture(identity(), partial, first.clone(), Some(&held)).is_none());
        let grey = pixels([[238; 3]; 3]);
        let cleared = ControlPaint::capture(identity(), grey.clone(), first.clone(), Some(&held)).unwrap();
        assert_eq!(cleared.backdrop(identity(), &first), Some(grey.as_slice()));
    }

    #[test]
    fn changed_font_declines_presentation_until_redraw_without_losing_backdrop() {
        let slot = ControlPaintSlot::default();
        let original = pixels([[238; 3]; 3]);
        let completed = pixels([[0; 3]; 3]);
        slot.record(identity(), original.clone(), completed.clone());
        slot.invalidate_presentation();
        assert!(!slot.has_drawing());
        assert!(slot.backdrop(identity(), &completed).is_none());
        let redrawn = pixels([[0; 3], [238; 3], [0; 3]]);
        slot.record(identity(), completed, redrawn.clone());
        assert!(slot.has_drawing());
        assert_eq!(slot.backdrop(identity(), &redrawn), Some(original));
    }

    #[test]
    fn indexed_backdrop_uses_current_display_palette_without_guest_redraw() {
        let identity = ControlPaintIdentity { format: ControlPaintFormat::Indexed8, ..identity() };
        let slot = ControlPaintSlot::default();
        let before = vec![1, 0, 0, 255].repeat(3);
        let mut after = before.clone();
        after[..4].copy_from_slice(&[0, 0, 0, 255]);
        slot.record(identity, before, after.clone());
        let mut palette = [0u32; 256];
        palette[1] = u32::from_le_bytes([31, 63, 95, 255]);
        assert_eq!(slot.rgba_backdrop(identity, &after, &palette).unwrap().as_ref(),
            [31, 63, 95, 255].repeat(3).as_slice());
        palette[1] = u32::from_le_bytes([9, 7, 5, 255]);
        assert_eq!(slot.rgba_backdrop(identity, &after, &palette).unwrap().as_ref(),
            [9, 7, 5, 255].repeat(3).as_slice());
    }

    #[test]
    fn cloned_records_share_draws_and_incomplete_draws_invalidate_evidence() {
        let slot = ControlPaintSlot::default();
        let cloned = slot.clone();
        let original = pixels([[238; 3]; 3]);
        let completed = pixels([[0; 3]; 3]);
        slot.record(identity(), original.clone(), completed.clone());
        assert_eq!(cloned.backdrop(identity(), &completed), Some(original));
        cloned.record(identity(), vec![], completed.clone());
        assert!(slot.backdrop(identity(), &completed).is_none());
        let fresh = ControlPaintSlot::default();
        assert!(fresh.backdrop(identity(), &completed).is_none());
    }

    #[test]
    fn rejects_stale_identity_surface_bounds_depth_and_partial_rasters() {
        let original = pixels([[238; 3]; 3]);
        let completed = pixels([[0; 3]; 3]);
        let held = ControlPaint::capture(identity(), original.clone(), completed.clone(), None).unwrap();
        for changed in [
            ControlPaintIdentity { owner: 11, ..identity() },
            ControlPaintIdentity { generation: 2, ..identity() },
            ControlPaintIdentity { surface: 21, ..identity() },
            ControlPaintIdentity { bounds: (1, 0, 2, 3), ..identity() },
            ControlPaintIdentity { depth: 16, ..identity() },
            ControlPaintIdentity { palette: 1, ..identity() },
            ControlPaintIdentity { format: ControlPaintFormat::Indexed8, ..identity() },
        ] {
            assert!(held.backdrop(changed, &completed).is_none());
            let fresh = ControlPaint::capture(changed, completed.clone(), completed.clone(), Some(&held)).unwrap();
            assert_eq!(fresh.backdrop(changed, &completed), Some(completed.as_slice()));
        }
        assert!(ControlPaint::capture(identity(), original[..8].to_vec(), completed, None).is_none());
    }
}
