//! Indexed and direct-color outline presentation. Guest memory and text metrics stay unchanged.
//! Ordinary framebuffer writes replace enlarged pixels in drawing order; supported
//! outline glyphs retain indexed coverage through snapshots and pixel transfers.
//! Frontends consume the presentation at its physical dimensions.
mod compact;
mod controls;
mod resample;
mod samples;
pub use compact::{CompactPresentation, CompactPresentationCache};
use samples::{DetailSamples, TILE_SAMPLES};

use super::page_index::PageIndex;
use super::{MacMemoryBus, MemoryBus};
use crate::quickdraw::fonts::{outline, Glyph};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

const _: () = assert!(TILE_SAMPLES <= u16::BITS as usize, "ink_mask holds one bit per sample");

/// `(offset % row_bytes, offset / row_bytes)` without a divide.
/// `reciprocal` is `floor(2^32 / row_bytes)`, so for any 32-bit `offset` the
/// estimate `offset * reciprocal >> 32` is the true quotient or one less.
#[inline]
fn divide_row(offset: u32, row_bytes: u32, reciprocal: u64) -> (u32, u32) {
    let mut y = ((u64::from(offset) * reciprocal) >> 32) as u32;
    let mut x = offset - y * row_bytes;
    if x >= row_bytes {
        x -= row_bytes;
        y += 1;
    }
    (x, y)
}

/// Shared by both CPU adapters; access is scoped to a single drawing operation.
#[derive(Clone, Default)]
pub(crate) struct PresentationSlot(std::rc::Rc<std::cell::RefCell<Option<Presentation>>>);

/// Page size of the bus's JIT store filter; equal to `PageIndex`'s.
pub(crate) const STORE_FILTER_PAGE_SHIFT: u32 = super::page_index::PAGE_SHIFT;

fn next_store_filter_identity() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
/// Width in pixels of a screen change-tracking tile.
const SCREEN_TILE: u32 = 16;

fn screen_tiles_per_row(width: u32) -> usize {
    width.div_ceil(SCREEN_TILE) as usize
}

fn next_presentation_identity() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A point in a presentation's screen history; see
/// [`MacMemoryBus::screen_rect_unchanged_since`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenMark {
    identity: u64,
    epoch: u64,
}

/// A resolved outline image borrowed from the presentation for one present.
///
/// [`MacMemoryBus::presented_argb_scaled`] copies the resolved image into the
/// caller's buffer so host overlays can be patched into it. Frames where no
/// overlay wrote a pixel skip that copy and present this image in place.
pub struct PresentedOutline<'a> {
    slot: std::cell::Ref<'a, Presentation>,
    scale: u32,
    size: (u32, u32),
}

impl PresentedOutline<'_> {
    /// Image size in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The resolved ARGB image, valid until the presentation is mutated again.
    pub fn pixels(&self) -> std::cell::Ref<'_, [u32]> {
        self.slot.resolved_argb(self.scale)
    }
}

impl std::fmt::Debug for PresentedOutline<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresentedOutline")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for PresentationSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PresentationSlot")
            .field(&self.is_some())
            .finish()
    }
}
impl PresentationSlot {
    pub fn as_ref(&self) -> Option<std::cell::Ref<'_, Presentation>> {
        std::cell::Ref::filter_map(self.0.borrow(), |p| p.as_ref()).ok()
    }
    pub fn as_mut(&self) -> Option<std::cell::RefMut<'_, Presentation>> {
        std::cell::RefMut::filter_map(self.0.borrow_mut(), |p| p.as_mut()).ok()
    }
    pub fn is_some(&self) -> bool {
        self.0.borrow().is_some()
    }
    pub fn is_none(&self) -> bool {
        !self.is_some()
    }
    pub fn set(&self, value: Option<Presentation>) {
        *self.0.borrow_mut() = value;
        super::note_store_filter_event();
    }
    pub fn observes(&self, address: u32, len: usize) -> bool {
        self.as_ref()
            .is_some_and(|p| p.observes_range(address, len))
    }
    pub(crate) fn capture_detail<T>(
        &self,
        pixels: &mut SavedPixels<T>,
        offset: usize,
        address: u32,
        len: usize,
    ) {
        pixels.identity = next_snapshot_identity();
        if let Some(p) = self.as_ref() {
            p.capture_detail_range(pixels, offset, address, len);
        }
    }
    pub(crate) fn restore_detail<T>(
        &self,
        pixels: &SavedPixels<T>,
        offset: usize,
        address: u32,
        len: usize,
    ) {
        self.restore_copy_detail(pixels, offset, address, len, None);
    }
    pub(crate) fn restore_copy_detail<T>(
        &self,
        pixels: &SavedPixels<T>,
        offset: usize,
        address: u32,
        len: usize,
        palette: Option<&[u8; 256]>,
    ) {
        if let Some(mut p) = self.as_mut() {
            for byte in 0..len {
                if let Some(detail) = pixels.detail.get(&(offset + byte)) {
                    if let Some(palette) = palette {
                        let mut detail = detail.clone();
                        let mapped = Arc::make_mut(&mut detail);
                        mapped.value = palette[mapped.value as usize];
                        for index in &mut mapped.indices {
                            *index = palette[*index as usize];
                        }
                        for ink in mapped.ink.values_mut() {
                            ink.foreground = palette[ink.foreground as usize];
                            ink.background.map(&mut |index| palette[index as usize]);
                        }
                        p.put_detail(address + byte as u32, &detail);
                    } else {
                        p.put_detail(address + byte as u32, detail);
                    }
                }
            }
        }
    }
    pub fn write_bytes(&self, address: u32, bytes: &[u8]) {
        if let Some(mut p) = self.as_mut() {
            let end = u64::from(address) + bytes.len() as u64;
            let screen_end = u64::from(p.base) + u64::from(p.row_bytes) * u64::from(p.height);
            if p.glyph.is_none()
                && !p.erasing_text
                && (end <= u64::from(p.base) || u64::from(address) >= screen_end)
            {
                // Ordinary offscreen writes only invalidate existing detail.
                // A native rectangle fill must not visit every background byte.
                // Most such writes are ordinary heap traffic, far from any
                // retained cell.
                if !p.may_have_offscreen_detail(address, end) {
                    return;
                }
                let keys: Vec<_> = p
                    .offscreen
                    .range(address..)
                    .take_while(|(key, _)| u64::from(**key) < end)
                    .map(|(&key, _)| key)
                    .collect();
                if !keys.is_empty() {
                    p.changed(false);
                }
                for key in keys {
                    p.offscreen.remove(&key);
                }
            } else if p.observes_range(address, bytes.len()) {
                for (i, &value) in bytes.iter().enumerate() {
                    p.write(address + i as u32, value);
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct OutlineGlyph {
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub left: i32,
    pub top: i32,
}

// Keep palette indexes through antialiasing so a CLUT change recolors existing
// coverage without rasterizing the guest's one-bit text again.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ink {
    foreground: u8,
    alpha: u32,
    background: IndexedColor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum IndexedColor {
    Solid(u8),
    Mix(Box<IndexedColor>, Box<IndexedColor>, u32),
}

impl IndexedColor {
    /// Whether `map` leaves every index in this colour unchanged.
    fn fixed_under(&self, map: &mut impl FnMut(u8) -> u8) -> bool {
        match self {
            Self::Solid(index) => map(*index) == *index,
            Self::Mix(fg, bg, _) => fg.fixed_under(map) && bg.fixed_under(map),
        }
    }

    fn map(&mut self, map: &mut impl FnMut(u8) -> u8) {
        match self {
            Self::Solid(index) => *index = map(*index),
            Self::Mix(fg, bg, _) => {
                fg.map(map);
                bg.map(map);
            }
        }
    }
    fn over(self, foreground: u8, alpha: u32) -> Self {
        if alpha == 0 {
            self
        } else if alpha == 255 {
            Self::Solid(foreground)
        } else {
            Self::Mix(Box::new(Self::Solid(foreground)), Box::new(self), alpha)
        }
    }
    fn combine(&self, dst: &Self, map: &mut impl FnMut(u8, u8) -> u8) -> Self {
        match (self, dst) {
            (Self::Solid(src), Self::Solid(dst)) => Self::Solid(map(*src, *dst)),
            (Self::Mix(fg, bg, alpha), _) => Self::Mix(
                Box::new(fg.combine(dst, map)),
                Box::new(bg.combine(dst, map)),
                *alpha,
            ),
            (_, Self::Mix(fg, bg, alpha)) => Self::Mix(
                Box::new(self.combine(fg, map)),
                Box::new(self.combine(bg, map)),
                *alpha,
            ),
        }
    }
    fn rgb(&self, palette: &[[u8; 3]; 256]) -> [u8; 3] {
        match self {
            Self::Solid(index) => palette[*index as usize],
            Self::Mix(foreground, background, alpha) => {
                blend(foreground.rgb(palette), background.rgb(palette), *alpha)
            }
        }
    }
}

fn blend(foreground: [u8; 3], background: [u8; 3], alpha: u32) -> [u8; 3] {
    std::array::from_fn(|c| {
        ((u32::from(foreground[c]) * alpha + u32::from(background[c]) * (255 - alpha) + 127) / 255)
            as u8
    })
}

impl Ink {
    fn rgb(&self, palette: &[[u8; 3]; 256]) -> [u8; 3] {
        blend(
            palette[self.foreground as usize],
            self.background.rgb(palette),
            self.alpha,
        )
    }
}

/// An owned pixel snapshot carries the indexed subpixels with the guest bytes.
/// Cloning a snapshot preserves its coverage even after its source is erased.
#[derive(Clone, Debug, Default)]
pub struct SavedPixels<T = u8> {
    values: Vec<T>,
    identity: u64,
    detail: HashMap<usize, Arc<DetailCell>, BuildHasherDefault<SampleOffsetHasher>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DetailCell {
    value: u8,
    indices: Vec<u8>,
    ink: HashMap<usize, Ink, BuildHasherDefault<SampleOffsetHasher>>,
}

/// Evidence for an indexed recoloring performed by guest CPU stores between
/// Toolbox calls. Only a consistent, one-to-one color map can preserve detail;
/// fills, conflicting writes, and unobserved colors retain ordinary invalidation.
struct CpuRecolor {
    map: [u16; 256],
    reverse: [u16; 256],
    last_address: u32,
    valid: bool,
    detail: Vec<(u32, Arc<DetailCell>)>,
}

impl CpuRecolor {
    fn new(address: u32) -> Self {
        Self {
            map: [256; 256],
            reverse: [256; 256],
            last_address: address,
            valid: true,
            detail: Vec::new(),
        }
    }

    fn invalidate(&mut self) {
        self.valid = false;
        self.detail.clear();
    }
}

fn next_snapshot_identity() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
impl<T: PartialEq> PartialEq for SavedPixels<T> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values && self.detail == other.detail
    }
}
impl<T: Eq> Eq for SavedPixels<T> {}

impl<T> From<Vec<T>> for SavedPixels<T> {
    fn from(values: Vec<T>) -> Self {
        Self {
            values,
            identity: next_snapshot_identity(),
            detail: HashMap::default(),
        }
    }
}
impl<T> SavedPixels<T> {
    /// Whether the logical byte at `offset` carries retained subpixel detail.
    pub(crate) fn has_detail_at(&self, offset: usize) -> bool {
        self.detail.contains_key(&offset)
    }

    /// Whether any logical byte in `span` carries retained subpixel detail.
    pub(crate) fn has_detail_in(&self, span: std::ops::Range<usize>) -> bool {
        if self.detail.len() < span.len() {
            self.detail.keys().any(|key| span.contains(key))
        } else {
            span.into_iter().any(|key| self.detail.contains_key(&key))
        }
    }
}
impl<T> std::ops::Deref for SavedPixels<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Vec<T> {
        &self.values
    }
}
impl<T> std::ops::DerefMut for SavedPixels<T> {
    fn deref_mut(&mut self) -> &mut Vec<T> {
        // Raw logical edits cannot retain stale coverage. Region refreshes use
        // replace_range so unrelated parts of a snapshot retain their detail.
        self.detail.clear();
        self.identity = next_snapshot_identity();
        &mut self.values
    }
}
impl<'a, T> IntoIterator for &'a SavedPixels<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
impl<T> SavedPixels<T> {
    pub(crate) fn transform_detail(&mut self, mut map: impl FnMut(usize, u8) -> u8) {
        self.identity = next_snapshot_identity();
        for (&offset, cell) in &mut self.detail {
            let cell = Arc::make_mut(cell);
            cell.value = map(offset, cell.value);
            for value in &mut cell.indices {
                *value = map(offset, *value);
            }
            for ink in cell.ink.values_mut() {
                ink.foreground = map(offset, ink.foreground);
                ink.background.map(&mut |value| map(offset, value));
            }
        }
    }
    pub fn map<U>(self, map: impl FnMut(T) -> U) -> SavedPixels<U> {
        SavedPixels {
            values: self.values.into_iter().map(map).collect(),
            identity: next_snapshot_identity(),
            detail: self.detail,
        }
    }
    pub fn into_vec(self) -> Vec<T> {
        self.values
    }
    pub(crate) fn slice(&self, range: std::ops::Range<usize>) -> Self
    where
        T: Clone,
    {
        Self {
            values: self.values[range.clone()].to_vec(),
            identity: next_snapshot_identity(),
            detail: self
                .detail
                .iter()
                .filter(|(i, _)| range.contains(i))
                .map(|(i, cell)| (i - range.start, cell.clone()))
                .collect(),
        }
    }
    pub(crate) fn replace_range(&mut self, offset: usize, values: &[T])
    where
        T: Clone,
    {
        self.identity = next_snapshot_identity();
        let end = offset + values.len();
        self.detail.retain(|i, _| *i < offset || *i >= end);
        self.values[offset..end].clone_from_slice(values);
    }
}

// These tables use only host-generated offsets into bounded presentation
// surfaces, pixel snapshots, or a cell's subpixel samples. They do not hash guest
// strings or arbitrary resource keys. Mix both the low bucket bits and high tag
// bits without paying SipHash's per-sample cost during painting and restoration.
#[derive(Default)]
struct SampleOffsetHasher(u64);

impl Hasher for SampleOffsetHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_usize(usize::from(byte));
        }
    }

    fn write_usize(&mut self, offset: usize) {
        let mixed = ((offset as u64) ^ self.0).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = mixed ^ (mixed >> 32);
    }
}

// The owned identity prevents an old cache entry from matching a replacement
// surface, including when its geometry and initial revision are identical.
/// An owned token identifying the visible retained image across surface replacement.
#[derive(Clone, Debug)]
pub struct VisibleImageStamp {
    identity: std::rc::Rc<()>,
    revision: u64,
}

impl PartialEq for VisibleImageStamp {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision && std::rc::Rc::ptr_eq(&self.identity, &other.identity)
    }
}

impl Eq for VisibleImageStamp {}

impl VisibleImageStamp {
    fn matches(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResolvedOutputFormat {
    Argb,
    Rgba,
}

enum ResolvedOutput {
    Argb(Vec<u32>),
    Rgba(Vec<u8>),
}

struct ResolvedOutputCache {
    source: VisibleImageStamp,
    size: (u32, u32),
    format: ResolvedOutputFormat,
    pixels: ResolvedOutput,
}

pub(crate) struct Presentation {
    revision: u64,
    /// Distinguishes presentation objects, so a screen mark taken on one is
    /// never compared with another's epochs.
    identity: u64,
    /// Bumped by every change to an on-screen cell: its guest value, text
    /// flag, samples, ink or cached detail.
    screen_epoch: u64,
    /// Per screen row, per `SCREEN_TILE`-pixel column tile: the `screen_epoch`
    /// of the tile's last change. A rectangle whose tiles all predate a mark
    /// holds exactly what it held at that mark.
    tile_epochs: Vec<u64>,
    visible_image: VisibleImageStamp,
    cpu_drawing: bool,
    cpu_recolor: Option<CpuRecolor>,
    cpu_copy: [Option<Arc<DetailCell>>; 4],
    restored_dialog: Option<(u64, (i16, i16, i16, i16), bool, u64)>,
    output_cache: std::cell::RefCell<Option<ResolvedOutputCache>>,
    offscreen: BTreeMap<u32, Arc<DetailCell>>,
    // Conservative bounds: deletion may leave false positives, never false negatives.
    offscreen_bounds: Option<(u32, u32)>,
    /// Page filter over the keys of `offscreen`, with the same conservative
    /// contract as the bounds above. Offscreen detail is scattered through
    /// the same heap that ordinary guest writes walk, so the bounds alone
    /// cannot reject an address landing between two retained cells.
    offscreen_pages: PageIndex,
    /// Pages whose `offscreen_pages` bit went from clear to set since the bus
    /// last read them. The bus's JIT store filter may have proven any such
    /// page plain before; it must stop trusting that proof.
    store_filter_new_pages: Vec<u32>,
    /// Distinguishes presentation objects, so the bus can tell a replacement
    /// (new screen geometry and offscreen set) from the object it filtered.
    store_filter_identity: u64,
    base: u32,
    row_bytes: u32,
    /// `floor(2^32 / row_bytes)`: `position` runs for every guest write, and
    /// a hardware divide showed up in its profile.
    row_reciprocal: u64,
    width: u32,
    height: u32,
    depth: u16,
    direct_palettes: [[[u8; 3]; 256]; 4],
    pub scale: u32,
    palette: [[u8; 3]; 256],
    samples: DetailSamples,
    guest_values: Vec<u16>,
    text_cells: Vec<bool>,
    /// Number of entries set in `text_cells`; maintained on every transition so
    /// `has_visible_outline_detail` does not rescan the whole screen.
    text_cell_count: usize,
    detail_cache: std::cell::RefCell<Vec<Option<Arc<DetailCell>>>>,
    // Keys are cell * TILE_SAMPLES + sample, independent of screen stride.
    ink: HashMap<usize, Ink, BuildHasherDefault<SampleOffsetHasher>>,
    /// Bit `i` of `ink_mask[cell]` is set exactly when `ink` holds
    /// `cell * TILE_SAMPLES + i`. Overwriting text probes the map only for
    /// samples that have ink, instead of once per sample.
    ink_mask: Vec<u16>,
    run_ink: HashSet<usize, BuildHasherDefault<SampleOffsetHasher>>,
    offscreen_run_ink: HashSet<(u32, usize)>,
    in_text_run: bool,
    pub erasing_text: bool,
    glyph: Option<(OutlineGlyph, i16, i16)>,
    pub glyph_count: usize,
}

impl Presentation {
    /// Record a change to the on-screen cell at (`x`, `y`).
    #[inline]
    fn touch_screen(&mut self, x: u32, y: u32) {
        self.screen_epoch += 1;
        let tile = y as usize * screen_tiles_per_row(self.width) + (x / SCREEN_TILE) as usize;
        if let Some(epoch) = self.tile_epochs.get_mut(tile) {
            *epoch = self.screen_epoch;
        }
    }

    fn screen_mark(&self) -> ScreenMark {
        ScreenMark {
            identity: self.identity,
            epoch: self.screen_epoch,
        }
    }

    /// Whether every on-screen cell in the rectangle is exactly as it was at
    /// `mark`. Rows and columns outside the screen hold no cells and are
    /// ignored; a mark from another presentation object is never current.
    fn screen_rect_unchanged_since(
        &self,
        mark: ScreenMark,
        (top, left, width, height): (i16, i16, i16, i16),
    ) -> bool {
        if mark.identity != self.identity {
            return false;
        }
        if width <= 0 || height <= 0 {
            return true;
        }
        let x0 = i32::from(left).max(0) as u32;
        let y0 = i32::from(top).max(0) as u32;
        let x1 = (i32::from(left) + i32::from(width)).clamp(0, self.width as i32) as u32;
        let y1 = (i32::from(top) + i32::from(height)).clamp(0, self.height as i32) as u32;
        if x0 >= x1 || y0 >= y1 {
            return true;
        }
        let per_row = screen_tiles_per_row(self.width);
        let (t0, t1) = ((x0 / SCREEN_TILE) as usize, ((x1 - 1) / SCREEN_TILE) as usize);
        (y0..y1).all(|y| {
            let row = y as usize * per_row;
            self.tile_epochs[row + t0..=row + t1]
                .iter()
                .all(|&epoch| epoch <= mark.epoch)
        })
    }

    fn changed(&mut self, visible: bool) {
        self.revision = self.revision.wrapping_add(1);
        if self.revision == 0 {
            // Even a wrapping counter cannot alias an older cached image.
            self.visible_image.identity = std::rc::Rc::new(());
        }
        if visible {
            self.visible_image.revision = self.revision;
        }
    }

    fn observe_cpu_recolor(&mut self, address: u32, old: u8, value: u8) {
        if self.depth != 8 {
            return;
        }
        if let Some(recolor) = self.cpu_recolor.as_mut() {
            if address <= recolor.last_address {
                recolor.invalidate();
            }
        }
        let recolor = self
            .cpu_recolor
            .get_or_insert_with(|| CpuRecolor::new(address));
        if !recolor.valid {
            return;
        }
        recolor.last_address = address;
        let mapped = recolor.map[old as usize];
        let source = recolor.reverse[value as usize];
        if (mapped != 256 && mapped != u16::from(value))
            || (source != 256 && source != u16::from(old))
        {
            recolor.invalidate();
            return;
        }
        recolor.map[old as usize] = value.into();
        recolor.reverse[value as usize] = old.into();
        if let Some(detail) = self.detail(address) {
            let recolor = self.cpu_recolor.as_mut().unwrap();
            // Bound temporary metadata even for a guest that rewrites a whole
            // text-filled screen without returning to the Toolbox.
            if recolor.detail.len() == 4096 {
                recolor.invalidate();
            } else {
                recolor.detail.push((address, detail));
            }
        }
    }

    fn finish_cpu_recolor(&mut self) {
        let Some(recolor) = self.cpu_recolor.take() else {
            return;
        };
        if !recolor.valid
            || !recolor
                .map
                .iter()
                .enumerate()
                .any(|(old, &new)| new < 256 && old != new as usize)
        {
            return;
        }
        for (address, mut detail) in recolor.detail {
            let cell = Arc::make_mut(&mut detail);
            let mut complete = true;
            let mut map = |index: u8| {
                let mapped = recolor.map[index as usize];
                complete &= mapped < 256;
                mapped as u8
            };
            cell.value = map(cell.value);
            for index in &mut cell.indices {
                *index = map(*index);
            }
            for ink in cell.ink.values_mut() {
                ink.foreground = map(ink.foreground);
                ink.background.map(&mut map);
            }
            if complete {
                self.put_detail(address, &detail);
            }
        }
    }

    fn bytes_per_pixel(&self) -> u32 {
        u32::from(self.depth.max(8) / 8)
    }
    fn logical_width(&self) -> u32 {
        self.width / self.bytes_per_pixel()
    }
    fn palette_at(&self, x: u32) -> &[[u8; 3]; 256] {
        if self.depth == 8 {
            &self.palette
        } else {
            &self.direct_palettes[(x % self.bytes_per_pixel()) as usize]
        }
    }

    fn include_offscreen_address(&mut self, address: u32) {
        self.offscreen_bounds = Some(match self.offscreen_bounds {
            Some((first, last)) => (first.min(address), last.max(address)),
            None => (address, address),
        });
        let fresh = !self
            .offscreen_pages
            .may_overlap(u64::from(address), u64::from(address) + 1);
        self.offscreen_pages
            .mark(u64::from(address), u64::from(address) + 1);
        if fresh {
            self.store_filter_new_pages.push(address >> STORE_FILTER_PAGE_SHIFT);
            super::note_store_filter_event();
        }
    }

    /// Whether any byte of the 4 KiB page at `page_start` may be observed:
    /// it touches the screen, or its offscreen page bit is set. Deliberately
    /// coarser than `observes_range`: the bit, not the exact offscreen map,
    /// is what `include_offscreen_address` reports changes of, so a page is
    /// proven plain only while its bit is clear.
    pub(crate) fn page_may_be_observed(&self, page_start: u32) -> bool {
        let start = u64::from(page_start);
        let end = start + (1u64 << STORE_FILTER_PAGE_SHIFT);
        let screen_end = u64::from(self.base) + u64::from(self.row_bytes) * u64::from(self.height);
        (start < screen_end && end > u64::from(self.base))
            || self.offscreen_pages.may_overlap(start, end)
    }

    pub(crate) fn store_filter_identity(&self) -> u64 {
        self.store_filter_identity
    }

    pub(crate) fn take_store_filter_new_pages(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.store_filter_new_pages)
    }

    /// A glyph capture observes every byte write, wherever it lands.
    pub(crate) fn glyph_active(&self) -> bool {
        self.glyph.is_some()
    }

    #[inline]
    fn may_have_offscreen_detail(&self, address: u32, end: u64) -> bool {
        self.offscreen_bounds
            .is_some_and(|(first, last)| address <= last && end > u64::from(first))
            && self.offscreen_pages.may_overlap(u64::from(address), end)
    }

    /// Only screen bytes and retained offscreen glyphs need write interception.
    /// Ordinary heap/stack ranges retain the bus's bulk memory paths.
    #[inline]
    pub(super) fn observes_range(&self, address: u32, len: usize) -> bool {
        if len == 0 {
            return false;
        }
        let end = u64::from(address).saturating_add(len as u64);
        if end > u64::from(u32::MAX) + 1 {
            return true;
        }
        if u64::from(address)
            < u64::from(self.base) + u64::from(self.row_bytes) * u64::from(self.height)
            && end > u64::from(self.base)
        {
            return true;
        }
        self.offscreen_bounds
            .is_some_and(|(first, last)| address <= last && end > u64::from(first))
            && self.observes_offscreen_range(address, end)
    }

    // Most scalar stores are rejected by the screen/offscreen bounds above.
    // Keep tree traversal and its stack frame out of those inline checks.
    #[inline(never)]
    fn observes_offscreen_range(&self, address: u32, end: u64) -> bool {
        self.offscreen_pages.may_overlap(u64::from(address), end)
            && self
                .offscreen
                .range(address..=(end - 1) as u32)
                .next()
                .is_some()
    }

    fn position(&self, address: u32) -> Option<(u32, u32)> {
        let offset = address.checked_sub(self.base)?;
        let (x, y) = divide_row(offset, self.row_bytes, self.row_reciprocal);
        (x < self.width && y < self.height).then_some((x, y))
    }

    fn resolved_argb(&self, scale: u32) -> std::cell::Ref<'_, [u32]> {
        let size = (self.logical_width() * scale, self.height * scale);
        let cache_matches = self.output_cache.borrow().as_ref().is_some_and(|cache| {
            cache.source == self.visible_image
                && cache.size == size
                && cache.format == ResolvedOutputFormat::Argb
        });
        if !cache_matches {
            let mut cache = self.output_cache.borrow_mut();
            let mut pixels = match cache.take() {
                Some(ResolvedOutputCache {
                    pixels: ResolvedOutput::Argb(pixels),
                    ..
                }) => pixels,
                _ => Vec::new(),
            };
            pixels.clear();
            pixels.reserve((self.logical_width() * self.height * scale * scale) as usize);
            self.render_scaled(scale, |rgb, count| {
                let pixel = 0xff000000
                    | (u32::from(rgb[0]) << 16)
                    | (u32::from(rgb[1]) << 8)
                    | u32::from(rgb[2]);
                pixels.extend(std::iter::repeat_n(pixel, count as usize));
            });
            *cache = Some(ResolvedOutputCache {
                source: self.visible_image.clone(),
                size,
                format: ResolvedOutputFormat::Argb,
                pixels: ResolvedOutput::Argb(pixels),
            });
        }
        std::cell::Ref::map(self.output_cache.borrow(), |cache| {
            match &cache.as_ref().unwrap().pixels {
                ResolvedOutput::Argb(pixels) => pixels.as_slice(),
                ResolvedOutput::Rgba(_) => unreachable!("resolved output cache format mismatch"),
            }
        })
    }

    fn resolved_rgba(&self, scale: u32) -> std::cell::Ref<'_, [u8]> {
        let size = (self.logical_width() * scale, self.height * scale);
        let cache_matches = self.output_cache.borrow().as_ref().is_some_and(|cache| {
            cache.source == self.visible_image
                && cache.size == size
                && cache.format == ResolvedOutputFormat::Rgba
        });
        if !cache_matches {
            let mut cache = self.output_cache.borrow_mut();
            let mut pixels = match cache.take() {
                Some(ResolvedOutputCache {
                    pixels: ResolvedOutput::Rgba(pixels),
                    ..
                }) => pixels,
                _ => Vec::new(),
            };
            pixels.clear();
            pixels.reserve((self.logical_width() * self.height * scale * scale * 4) as usize);
            self.render_scaled(scale, |rgb, count| {
                pixels.extend(
                    std::iter::repeat_n([rgb[0], rgb[1], rgb[2], 255], count as usize).flatten(),
                );
            });
            *cache = Some(ResolvedOutputCache {
                source: self.visible_image.clone(),
                size,
                format: ResolvedOutputFormat::Rgba,
                pixels: ResolvedOutput::Rgba(pixels),
            });
        }
        std::cell::Ref::map(self.output_cache.borrow(), |cache| {
            match &cache.as_ref().unwrap().pixels {
                ResolvedOutput::Rgba(pixels) => pixels.as_slice(),
                ResolvedOutput::Argb(_) => unreachable!("resolved output cache format mismatch"),
            }
        })
    }

    fn render_scaled(&self, scale: u32, mut emit: impl FnMut([u8; 3], u32)) {
        if self.depth == 8 && self.scale % scale == 0 {
            let factor = self.scale / scale;
            let count = factor * factor;
            for y in 0..self.height {
                for dy in 0..scale {
                    for x in 0..self.width {
                        let cell = (y * self.width + x) as usize;
                        if !self.text_cells[cell] {
                            emit(self.palette[self.guest_values[cell] as u8 as usize], scale);
                            continue;
                        }
                        let samples = self.samples.get(cell);
                        for dx in 0..scale {
                            let mut sum = [0u32; 3];
                            let start = ((dy * factor) * self.scale + dx * factor) as usize;
                            for sy in 0..factor as usize {
                                let row = start + sy * self.scale as usize;
                                for rgb in &samples.rgb[row..row + factor as usize] {
                                    for c in 0..3 {
                                        sum[c] += u32::from(rgb[c]);
                                    }
                                }
                            }
                            emit(sum.map(|v| ((v + count / 2) / count) as u8), 1);
                        }
                    }
                }
            }
            return;
        }
        let lanes = self.bytes_per_pixel();
        let width = self.logical_width();
        // Integrate each retained coverage cell directly into the requested
        // integer output scale, without materializing a larger RGB frame.
        for y in 0..self.height {
            for dy in 0..scale {
                for x in 0..width {
                    let first = (y * self.width + x * lanes) as usize;
                    if !self.text_cells[first..first + lanes as usize]
                        .iter()
                        .any(|&v| v)
                    {
                        let mut rgb = [0u8; 3];
                        for lane in 0..lanes {
                            let color = self.palette_at(x * lanes + lane)
                                [self.guest_values[first + lane as usize] as u8 as usize];
                            for c in 0..3 {
                                rgb[c] = rgb[c].saturating_add(color[c]);
                            }
                        }
                        emit(rgb, scale);
                        continue;
                    }
                    for dx in 0..scale {
                        let (left, right, top, bottom, total) = if scale > self.scale {
                            let left = ((dx * 2 + 1) * self.scale / (scale * 2)) * scale;
                            let top = ((dy * 2 + 1) * self.scale / (scale * 2)) * scale;
                            (left, left + scale, top, top + scale, scale * scale)
                        } else {
                            (
                                dx * self.scale,
                                (dx + 1) * self.scale,
                                dy * self.scale,
                                (dy + 1) * self.scale,
                                self.scale * self.scale,
                            )
                        };
                        let mut sum = [0u32; 3];
                        for sy in top / scale..bottom.div_ceil(scale) {
                            let wy = bottom.min((sy + 1) * scale) - top.max(sy * scale);
                            for sx in left / scale..right.div_ceil(scale) {
                                let weight =
                                    wy * (right.min((sx + 1) * scale) - left.max(sx * scale));
                                let mut rgb = [0u8; 3];
                                for lane in 0..lanes {
                                    let bx = x * lanes + lane;
                                    let cell = first + lane as usize;
                                    let color = if self.text_cells[cell] {
                                        self.samples.get(cell).rgb[(sy * self.scale + sx) as usize]
                                    } else {
                                        self.palette_at(bx)[self.guest_values[cell] as u8 as usize]
                                    };
                                    for c in 0..3 {
                                        rgb[c] = rgb[c].saturating_add(color[c]);
                                    }
                                }
                                for c in 0..3 {
                                    sum[c] += u32::from(rgb[c]) * weight;
                                }
                            }
                        }
                        emit(sum.map(|v| ((v + total / 2) / total) as u8), 1);
                    }
                }
            }
        }
    }

    fn render_pixels<T: Copy>(&self, map: impl Fn([u8; 3]) -> T) -> Vec<T> {
        if self.depth == 8 {
            return self.render_indexed_pixels(map);
        }
        let lanes = self.bytes_per_pixel();
        let width = self.logical_width();
        let mut output =
            Vec::with_capacity((width * self.height * self.scale * self.scale) as usize);
        for y in 0..self.height {
            for sy in 0..self.scale {
                for x in 0..width {
                    let first = (y * self.width + x * lanes) as usize;
                    if !self.text_cells[first..first + lanes as usize]
                        .iter()
                        .any(|&text| text)
                    {
                        let mut rgb = [0u8; 3];
                        for lane in 0..lanes {
                            let color = self.direct_palettes[lane as usize]
                                [self.guest_values[first + lane as usize] as u8 as usize];
                            for channel in 0..3 {
                                rgb[channel] = rgb[channel].saturating_add(color[channel]);
                            }
                        }
                        output.extend(std::iter::repeat_n(map(rgb), self.scale as usize));
                        continue;
                    }
                    for sx in 0..self.scale {
                        let mut rgb = [0u8; 3];
                        for lane in 0..lanes {
                            let bx = x * lanes + lane;
                            let cell = (y * self.width + bx) as usize;
                            let color = if self.text_cells[cell] {
                                self.samples.get(cell).rgb[(sy * self.scale + sx) as usize]
                            } else {
                                self.palette_at(bx)[self.guest_values[cell] as u8 as usize]
                            };
                            for c in 0..3 {
                                rgb[c] = rgb[c].saturating_add(color[c]);
                            }
                        }
                        output.push(map(rgb));
                    }
                }
            }
        }
        output
    }

    fn render_indexed_pixels<T: Copy>(&self, map: impl Fn([u8; 3]) -> T) -> Vec<T> {
        let width = (self.width * self.scale) as usize;
        let mut output = Vec::with_capacity(width * (self.height * self.scale) as usize);
        for y in 0..self.height {
            for sy in 0..self.scale {
                for x in 0..self.width {
                    let cell = (y * self.width + x) as usize;
                    if self.text_cells[cell] {
                        let start = (sy * self.scale) as usize;
                        for rgb in &self.samples.get(cell).rgb[start..start + self.scale as usize] {
                            output.push(map([rgb[0], rgb[1], rgb[2]]));
                        }
                    } else {
                        let color = map(self.palette[self.guest_values[cell] as u8 as usize]);
                        output.extend(std::iter::repeat_n(color, self.scale as usize));
                    }
                }
            }
        }
        output
    }

    fn detail(&self, address: u32) -> Option<Arc<DetailCell>> {
        let Some((x, y)) = self.position(address) else {
            return if self.may_have_offscreen_detail(address, u64::from(address) + 1) {
                self.offscreen.get(&address).cloned()
            } else {
                None
            };
        };
        if !self.text_cells[(y * self.width + x) as usize] {
            return None;
        }
        let index = (y * self.width + x) as usize;
        if let Some(cell) = &self.detail_cache.borrow()[index] {
            return Some(cell.clone());
        }
        let mut cell = DetailCell {
            value: self.guest_values[(y * self.width + x) as usize] as u8,
            indices: Vec::new(),
            ink: HashMap::default(),
        };
        let samples = self.samples.get(index);
        for i in 0..(self.scale * self.scale) as usize {
            cell.indices.push(samples.indices[i]);
            if let Some(ink) = self.ink.get(&(index * TILE_SAMPLES + i)) {
                cell.ink.insert(i, ink.clone());
            }
        }
        let cell = Arc::new(cell);
        self.detail_cache.borrow_mut()[index] = Some(cell.clone());
        Some(cell)
    }

    /// Collect the detail cells covering `[address, address + len)` into
    /// `pixels`, keyed by `offset + byte`.
    ///
    /// Equivalent to [`Presentation::detail`] per byte, but shaped for the
    /// range. Screen bytes walk a row at a time, hoisting the address
    /// arithmetic out of the byte loop and reducing the per-byte test to one
    /// `text_cells` bool; bytes outside the screen take one ordered range
    /// query per span, visiting retained glyphs instead of addresses.
    fn capture_detail_range<T>(
        &self,
        pixels: &mut SavedPixels<T>,
        offset: usize,
        address: u32,
        len: usize,
    ) {
        let start = u64::from(address);
        let end = start + len as u64;
        let has_offscreen = self.may_have_offscreen_detail(address, end);

        // Retained glyphs outside the framebuffer, over one address span.
        let offscreen_span = |pixels: &mut SavedPixels<T>, from: u64, to: u64| {
            if !has_offscreen || from >= to {
                return;
            }
            let (Ok(from), Ok(to)) = (u32::try_from(from), u32::try_from(to)) else {
                return;
            };
            for (&key, cell) in self.offscreen.range(from..to) {
                pixels
                    .detail
                    .insert(offset + (key - address) as usize, cell.clone());
            }
        };

        let screen_start = u64::from(self.base);
        let screen_end = screen_start + u64::from(self.row_bytes) * u64::from(self.height);
        offscreen_span(pixels, start, end.min(screen_start));
        offscreen_span(pixels, start.max(screen_end), end);

        let mut cursor = start.max(screen_start);
        let screen_end = end.min(screen_end);
        while cursor < screen_end {
            let row_offset = cursor - screen_start;
            let y = (row_offset / u64::from(self.row_bytes)) as u32;
            let x = (row_offset % u64::from(self.row_bytes)) as u32;
            let row_start = cursor - u64::from(x);
            if x >= self.width {
                // Row padding carries no cell; it can still hold a glyph.
                let next_row = (row_start + u64::from(self.row_bytes)).min(screen_end);
                offscreen_span(pixels, cursor, next_row);
                cursor = next_row;
                continue;
            }
            let run = (u64::from(self.width - x)).min(screen_end - cursor) as usize;
            let cell_base = (y * self.width + x) as usize;
            for index in self.text_cells[cell_base..cell_base + run]
                .iter()
                .enumerate()
                .filter_map(|(index, set)| set.then_some(index))
            {
                let byte = cursor + index as u64;
                if let Some(detail) = self.detail(byte as u32) {
                    pixels
                        .detail
                        .insert(offset + (byte - start) as usize, detail);
                }
            }
            cursor += run as u64;
        }
    }

    /// Prove a screen-row span has no retained outline coverage. Other
    /// layouts use the per-byte path, including padding and row crossings.
    fn plain_screen_row(&self, address: u32, len: usize) -> bool {
        if len == 0 {
            return true;
        }
        let Some(last) = u32::try_from(len - 1)
            .ok()
            .and_then(|n| address.checked_add(n))
        else {
            return false;
        };
        let (Some((x, y)), Some((_, last_y))) = (self.position(address), self.position(last))
        else {
            return false;
        };
        if y != last_y {
            return false;
        }
        let Some(length) = u32::try_from(len).ok() else {
            return false;
        };
        let Some(end_x) = x.checked_add(length) else {
            return false;
        };
        if end_x > self.width {
            return false;
        }
        let start = (y * self.width + x) as usize;
        !self.text_cells[start..start + len].iter().any(|&set| set)
    }

    /// Update ordinary framebuffer bytes without walking presentation state
    /// one cell at a time. The caller has already proved that this is a
    /// plain, single-row span with no source detail to transfer.
    pub(crate) fn can_sync_plain_screen_row(&self, address: u32, len: usize) -> bool {
        !self.cpu_drawing
            && self.cpu_recolor.is_none()
            && self.glyph.is_none()
            && self.plain_screen_row(address, len)
    }

    pub(crate) fn sync_plain_screen_row(&mut self, address: u32, bytes: &[u8]) {
        if !self.can_sync_plain_screen_row(address, bytes.len()) {
            return;
        }
        let Some((x, y)) = self.position(address) else {
            return;
        };
        let start = (y * self.width + x) as usize;
        let changed = bytes
            .iter()
            .enumerate()
            .any(|(offset, &value)| self.guest_values[start + offset] != u16::from(value));
        if !changed {
            return;
        }
        self.changed(true);
        for offset in 0..bytes.len() as u32 {
            self.touch_screen(x + offset, y);
        }
        for (offset, &value) in bytes.iter().enumerate() {
            let cell = start + offset;
            self.detail_cache.get_mut()[cell] = None;
            self.guest_values[cell] = u16::from(value);
        }
    }

    /// Prove that the retained coverage over `[address, address + len)` is
    /// exactly what `pixels` recorded at `[offset, offset + len)`.
    ///
    /// The mirror of [`Presentation::capture_detail_range`] for restoration.
    /// The per-byte proof asks, for every byte, "does the snapshot hold a cell
    /// here, and does the screen agree?", which costs a hash probe, a division
    /// and a call even where neither side has ever held a glyph. Restored
    /// chrome is overwhelmingly background, so this walks the two sparse sides
    /// instead: screen bytes a row at a time, reducing an unchanged background
    /// byte to one `text_cells` bool, and bytes outside the screen by one
    /// ordered range query per span.
    ///
    /// Both sides are read on every call. No additional result or index is
    /// cached, so snapshot mutations need no new invalidation protocol.
    ///
    /// Returns `None` when the snapshot table's capacity exceeds eight times
    /// the span length, bounding rescans of large snapshots for tiny restores.
    /// Counting a table entry is cheaper than a hash probe plus per-byte
    /// coordinates, so equal capacity and span length is too restrictive for
    /// short rows inside a larger saved rectangle. This is a cost heuristic;
    /// the declined case keeps the caller's original proof.
    /// The bound is the table's **capacity**, not its length: walking a hash
    /// table costs a pass over its buckets, and `replace_range` and `clear`
    /// empty a table without returning its capacity, so a snapshot that once
    /// held a screenful of glyphs stays expensive to enumerate after it is
    /// emptied.
    ///
    /// Anything this cannot describe exactly answers `Some(false)` rather than
    /// claiming a proof. That only costs the caller its ordinary per-byte
    /// restore pass, which writes nothing where bytes and coverage agree.
    fn coverage_matches<T: Copy + Into<u16>>(
        &self,
        address: u32,
        len: usize,
        pixels: &SavedPixels<T>,
        offset: usize,
    ) -> Option<bool> {
        if pixels.detail.capacity() > len.saturating_mul(8) {
            return None;
        }
        let start = u64::from(address);
        let end = start + len as u64;

        // Count entries without following each Arc to its retained samples.
        // The destination walk below verifies every covered position against a
        // live, matching entry, so covered <= live entries <= all entries.
        let entries = pixels
            .detail
            .keys()
            .filter(|&&index| index >= offset && index - offset < len)
            .count();

        // Every position the screen still covers must be one of those, and
        // must still hold the same cell. Counting both sides is what proves
        // the snapshot holds nothing where the screen now holds nothing.
        let mut covered = 0usize;
        let has_offscreen = self.may_have_offscreen_detail(address, end);
        let offscreen_span = |from: u64, to: u64, covered: &mut usize| {
            if !has_offscreen || from >= to {
                return true;
            }
            // `to` is exclusive and may be one past the last address, which no
            // `u32` holds; the span's final address always is one. A span this
            // cannot express is reported as unproven, never as proven.
            let (Ok(first), Ok(last)) = (u32::try_from(from), u32::try_from(to - 1)) else {
                return false;
            };
            for (&key, cell) in self.offscreen.range(first..=last) {
                let index = offset + (key - address) as usize;
                let value = pixels[index].into() as u8;
                if pixels
                    .detail
                    .get(&index)
                    .filter(|saved| saved.value == value)
                    != Some(cell)
                {
                    return false;
                }
                *covered += 1;
            }
            true
        };

        let screen_start = u64::from(self.base);
        let screen_limit = screen_start + u64::from(self.row_bytes) * u64::from(self.height);
        if !offscreen_span(start, end.min(screen_start), &mut covered)
            || !offscreen_span(start.max(screen_limit), end, &mut covered)
        {
            return Some(false);
        }

        let mut cursor = start.max(screen_start);
        let screen_end = end.min(screen_limit);
        while cursor < screen_end {
            let row_offset = cursor - screen_start;
            let y = (row_offset / u64::from(self.row_bytes)) as u32;
            let x = (row_offset % u64::from(self.row_bytes)) as u32;
            let row_start = cursor - u64::from(x);
            if x >= self.width {
                // Row padding carries no cell; it can still hold a glyph.
                let next_row = (row_start + u64::from(self.row_bytes)).min(screen_end);
                if !offscreen_span(cursor, next_row, &mut covered) {
                    return Some(false);
                }
                cursor = next_row;
                continue;
            }
            let run = (u64::from(self.width - x)).min(screen_end - cursor) as usize;
            let cell_base = (y * self.width + x) as usize;
            for column in self.text_cells[cell_base..cell_base + run]
                .iter()
                .enumerate()
                .filter_map(|(column, set)| set.then_some(column))
            {
                let index = offset + (cursor + column as u64 - start) as usize;
                let value = pixels[index].into() as u8;
                let Some(cell) = pixels.detail.get(&index).filter(|cell| cell.value == value)
                else {
                    return Some(false);
                };
                if !self.screen_cell_matches(x + column as u32, y, cell) {
                    return Some(false);
                }
                covered += 1;
            }
            cursor += run as u64;
        }
        if covered == entries {
            return Some(true);
        }
        // A map may leave entries whose value no longer matches the saved
        // byte. If the cheap counts differ, retain the original filtered
        // count so these stale entries cannot change the proof's verdict.
        let retained = pixels
            .detail
            .iter()
            .filter(|&(index, cell)| {
                *index >= offset
                    && *index - offset < len
                    && cell.value == pixels[*index].into() as u8
            })
            .count();
        Some(covered == retained)
    }

    fn matches_detail(&self, address: u32, detail: Option<&Arc<DetailCell>>) -> bool {
        let Some((x, y)) = self.position(address) else {
            return self.offscreen.get(&address) == detail;
        };
        match detail {
            None => !self.text_cells[(y * self.width + x) as usize],
            Some(cell) => self.screen_cell_matches(x, y, cell),
        }
    }

    /// Whether the screen cell at `(x, y)` still carries exactly `cell`.
    ///
    /// The retained half of [`Presentation::matches_detail`], for a caller that
    /// already knows the position and so need not derive it from an address.
    fn screen_cell_matches(&self, x: u32, y: u32, cell: &Arc<DetailCell>) -> bool {
        if !self.text_cells[(y * self.width + x) as usize]
            || self.guest_values[(y * self.width + x) as usize] != u16::from(cell.value)
            || cell.indices.len() != (self.scale * self.scale) as usize
        {
            return false;
        }
        if self.detail_cache.borrow()[(y * self.width + x) as usize]
            .as_ref()
            .is_some_and(|cached| Arc::ptr_eq(cached, cell))
        {
            return true;
        }
        let index = (y * self.width + x) as usize;
        let samples = self.samples.get(index);
        for i in 0..(self.scale * self.scale) as usize {
            if samples.indices[i] != cell.indices[i]
                || self.ink.get(&(index * TILE_SAMPLES + i)) != cell.ink.get(&i)
            {
                return false;
            }
        }
        // A redraw can create an equal cell with a new identity. Remember it
        // after comparing once, so idle frames do not repeat every ink lookup.
        self.detail_cache.borrow_mut()[(y * self.width + x) as usize] = Some(cell.clone());
        true
    }

    fn put_detail(&mut self, address: u32, cell: &Arc<DetailCell>) {
        if self.cpu_drawing {
            // A CPU memory copy supplies its own source coverage. It is not
            // evidence for a recoloring of the destination's previous text.
            if let Some(recolor) = self.cpu_recolor.as_mut() {
                recolor.invalidate();
            }
        } else {
            self.finish_cpu_recolor();
        }
        if self.matches_detail(address, Some(cell)) {
            return;
        }
        let position = self.position(address);
        self.changed(position.is_some());
        let Some((x, y)) = position else {
            self.include_offscreen_address(address);
            self.offscreen.insert(address, cell.clone());
            return;
        };
        if cell.indices.len() != (self.scale * self.scale) as usize {
            return;
        }
        self.touch_screen(x, y);
        let index = (y * self.width + x) as usize;
        let palette = if self.depth == 8 {
            &self.palette
        } else {
            &self.direct_palettes[(x % self.bytes_per_pixel()) as usize]
        };
        let samples = self.samples.ensure(index);
        Self::set_text_cell(&mut self.text_cells, &mut self.text_cell_count, index, true);
        self.detail_cache.get_mut()[index] = Some(cell.clone());
        self.guest_values[index] = cell.value.into();
        for i in 0..(self.scale * self.scale) as usize {
            samples.indices[i] = cell.indices[i];
            let offset = index * TILE_SAMPLES + i;
            let bit = 1u16 << i;
            if self.ink_mask[index] & bit != 0 {
                self.ink.remove(&offset);
                self.ink_mask[index] &= !bit;
            }
            samples.rgb[i] = if let Some(ink) = cell.ink.get(&i) {
                self.ink.insert(offset, ink.clone());
                self.ink_mask[index] |= bit;
                ink.rgb(palette)
            } else {
                palette[cell.indices[i] as usize]
            };
        }
    }

    pub fn glyph_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let (g, h, v) = self.glyph.as_ref()?;
        let scale = self.scale as i32;
        Some((
            i32::from(*v) + g.top.div_euclid(scale),
            i32::from(*h) + g.left.div_euclid(scale),
            i32::from(*v) + (g.top + g.height + scale - 1).div_euclid(scale),
            i32::from(*h) + (g.left + g.width + scale - 1).div_euclid(scale),
        ))
    }

    /// Set one coverage cell, keeping `text_cell_count` in step with the
    /// true/false transitions. Repeated assignments must stay no-ops: the erase
    /// path below can revisit the same cell once per covered sample.
    ///
    /// The two coverage fields are passed separately so callers holding a
    /// `samples` borrow can still update coverage without a second whole-`self`
    /// borrow.
    fn set_text_cell(
        text_cells: &mut [bool],
        text_cell_count: &mut usize,
        cell: usize,
        text: bool,
    ) {
        if text_cells[cell] != text {
            text_cells[cell] = text;
            *text_cell_count = if text {
                *text_cell_count + 1
            } else {
                *text_cell_count - 1
            };
        }
    }

    fn prepare_text_cell(&mut self, x: u32, y: u32) {
        self.touch_screen(x, y);
        self.detail_cache.get_mut()[(y * self.width + x) as usize] = None;
        let cell = (y * self.width + x) as usize;
        if !self.text_cells[cell] {
            let index = self.guest_values[cell] as u8;
            let color = self.palette_at(x)[index as usize];
            let samples = self.samples.ensure(cell);
            samples.indices.fill(index);
            samples.rgb.fill(color);
        }
        Self::set_text_cell(&mut self.text_cells, &mut self.text_cell_count, cell, true);
    }

    /// The mask invariant: bits exactly mirror the ink map's keys.
    #[cfg(test)]
    fn ink_mask_matches_ink(&self) -> bool {
        let mut expected = vec![0u16; self.ink_mask.len()];
        for &offset in self.ink.keys() {
            expected[offset / TILE_SAMPLES] |= 1 << (offset % TILE_SAMPLES);
        }
        expected == self.ink_mask
    }

    pub fn write(&mut self, address: u32, value: u8) {
        let Some((x, y)) = self.position(address) else {
            if self.glyph.is_none()
                && !self.may_have_offscreen_detail(address, u64::from(address) + 1)
            {
                return;
            }
            if self.offscreen.contains_key(&address) || self.glyph.is_some() {
                self.changed(false);
            }
            if self.glyph.is_some() {
                if let Some(cell) = self.offscreen.get_mut(&address) {
                    let cell = Arc::make_mut(cell);
                    cell.value = value;
                }
            } else if self.erasing_text
                && self
                    .offscreen_run_ink
                    .iter()
                    .any(|(addr, _)| *addr == address)
            {
                if let Some(cell) = self.offscreen.get_mut(&address) {
                    let cell = Arc::make_mut(cell);
                    cell.value = value;
                    for i in 0..cell.indices.len() {
                        if !self.offscreen_run_ink.contains(&(address, i)) {
                            cell.indices[i] = value;
                            cell.ink.remove(&i);
                        }
                    }
                }
            } else {
                self.offscreen.remove(&address);
            }
            return;
        };
        let cell = (y * self.width + x) as usize;
        if self.cpu_drawing && self.glyph.is_none() && !self.erasing_text {
            self.observe_cpu_recolor(address, self.guest_values[cell] as u8, value);
        } else {
            self.finish_cpu_recolor();
        }
        if self.glyph.is_some() {
            self.detail_cache.get_mut()[cell] = None;
            self.changed(true);
            // The logical mask can extend beyond the native glyph bounds.
            // Such cells still need their current background preserved.
            // (`prepare_text_cell` records the change for screen marks.)
            self.prepare_text_cell(x, y);
            self.guest_values[cell] = u16::from(value);
            return;
        }
        if self.guest_values[cell] == u16::from(value) && !self.text_cells[cell] {
            // Same plain pixel: no cell state changes, so no screen mark
            // moves, and a plain cell holds no cached detail to clear.
            return;
        }
        self.touch_screen(x, y);
        self.changed(true);
        self.guest_values[cell] = u16::from(value);
        if !self.text_cells[cell] {
            // Only text cells ever hold a cached detail cell (`detail` and
            // `put_detail` fill it for text cells, and a cell leaves text
            // only below, after clearing it), so a plain cell has nothing
            // to invalidate.
            // Ordinary game pixels stay indexed until presentation. Expanding
            // each intermediate framebuffer write to scale² RGB samples makes
            // software-rendered animation pay the text cost for every pixel.
            return;
        }
        self.detail_cache.get_mut()[cell] = None;
        Self::set_text_cell(&mut self.text_cells, &mut self.text_cell_count, cell, false);
        let color = self.palette_at(x)[value as usize];
        let samples = self.samples.get_mut(cell);
        for i in 0..(self.scale * self.scale) as usize {
            let offset = cell * TILE_SAMPLES + i;
            // A following character's opaque background must not shave off
            // an outline overhang already painted by this same text run.
            if self.erasing_text && self.run_ink.contains(&offset) {
                Self::set_text_cell(&mut self.text_cells, &mut self.text_cell_count, cell, true);
                continue;
            }
            if !self.run_ink.is_empty() {
                self.run_ink.remove(&offset);
            }
            let bit = 1u16 << i;
            if self.ink_mask[cell] & bit != 0 {
                self.ink.remove(&offset);
                self.ink_mask[cell] &= !bit;
            }
            samples.indices[i] = value;
            samples.rgb[i] = color;
        }
        if !self.text_cells[cell] {
            self.samples.release(cell);
        }
    }

    /// Called for every visible glyph cell, including cells with zero 1x ink.
    /// QuickDraw has already applied both the visibility and clipping regions.
    pub fn glyph_pixel(&mut self, address: u32, x: i16, y: i16, foreground: u8, background: u8) {
        let position = self.position(address);
        self.changed(position.is_some() && self.glyph.is_some());
        let Some((px, py)) = position else {
            if self.glyph.is_none() {
                return;
            }
            self.include_offscreen_address(address);
            let Some((glyph, h, v)) = &self.glyph else {
                return;
            };
            let cell = self.offscreen.entry(address).or_insert_with(|| {
                Arc::new(DetailCell {
                    value: background,
                    indices: vec![background; (self.scale * self.scale) as usize],
                    ink: HashMap::default(),
                })
            });
            let cell = Arc::make_mut(cell);
            for sy in 0..self.scale {
                for sx in 0..self.scale {
                    let gx =
                        (i32::from(x) - i32::from(*h)) * self.scale as i32 + sx as i32 - glyph.left;
                    let gy =
                        (i32::from(y) - i32::from(*v)) * self.scale as i32 + sy as i32 - glyph.top;
                    if gx < 0 || gy < 0 || gx >= glyph.width || gy >= glyph.height {
                        continue;
                    }
                    let alpha = u32::from(glyph.pixels[(gy * glyph.width + gx) as usize]);
                    if alpha == 0 {
                        continue;
                    }
                    let i = (sy * self.scale + sx) as usize;
                    if self.in_text_run {
                        self.offscreen_run_ink.insert((address, i));
                    }
                    if alpha == 255 {
                        cell.indices[i] = foreground;
                        cell.ink.remove(&i);
                    } else {
                        let ink = cell.ink.entry(i).or_insert_with(|| Ink {
                            foreground,
                            alpha: 0,
                            background: IndexedColor::Solid(cell.indices[i]),
                        });
                        if ink.foreground != foreground {
                            let previous = ink.clone();
                            *ink = Ink {
                                foreground,
                                alpha: 0,
                                background: previous
                                    .background
                                    .over(previous.foreground, previous.alpha),
                            };
                        }
                        ink.alpha = ink.alpha.max(alpha);
                    }
                }
            }
            return;
        };
        if self.glyph.is_none() {
            return;
        }
        self.prepare_text_cell(px, py);
        let (glyph, h, v) = self.glyph.as_ref().unwrap();
        let lane = (px % self.bytes_per_pixel()) as usize;
        let palette = if self.depth == 8 {
            &self.palette
        } else {
            &self.direct_palettes[lane]
        };
        let color = palette[foreground as usize];
        let samples = self.samples.get_mut((py * self.width + px) as usize);
        for sy in 0..self.scale {
            for sx in 0..self.scale {
                let gx =
                    (i32::from(x) - i32::from(*h)) * self.scale as i32 + sx as i32 - glyph.left;
                let gy = (i32::from(y) - i32::from(*v)) * self.scale as i32 + sy as i32 - glyph.top;
                if gx < 0 || gy < 0 || gx >= glyph.width || gy >= glyph.height {
                    continue;
                }
                let alpha = u32::from(glyph.pixels[(gy * glyph.width + gx) as usize]);
                if alpha == 0 {
                    continue;
                }
                let offset = (py * self.width + px) as usize * TILE_SAMPLES
                    + (sy * self.scale + sx) as usize;
                if self.in_text_run {
                    self.run_ink.insert(offset);
                }
                let sample = (sy * self.scale + sx) as usize;
                let ink_cell = (py * self.width + px) as usize;
                let bit = 1u16 << sample;
                if alpha == 255 {
                    samples.rgb[sample] = color;
                    if self.ink_mask[ink_cell] & bit != 0 {
                        self.ink.remove(&offset);
                        self.ink_mask[ink_cell] &= !bit;
                    }
                    samples.indices[sample] = foreground;
                    continue;
                }
                self.ink_mask[ink_cell] |= bit;
                // Inside Macintosh I, "Transfer Modes": srcOr forces source
                // ink on and leaves other bits alone; repeated ink is idempotent.
                // Retain coverage rather than
                // repeatedly blending the same ink into its own antialiased edge.
                let ink = self.ink.entry(offset).or_insert_with(|| Ink {
                    foreground,
                    alpha: 0,
                    background: IndexedColor::Solid(samples.indices[sample]),
                });
                if ink.foreground != foreground {
                    let previous = std::mem::replace(
                        ink,
                        Ink {
                            foreground,
                            alpha: 0,
                            background: IndexedColor::Solid(0),
                        },
                    );
                    ink.background = previous
                        .background
                        .over(previous.foreground, previous.alpha);
                }
                ink.alpha = ink.alpha.max(alpha);
                samples.rgb[sample] = ink.rgb(palette);
            }
        }
    }
}

impl MacMemoryBus {
    pub(crate) fn begin_cpu_drawing(&mut self) {
        if let Some(mut p) = self.presentation.as_mut() {
            p.cpu_drawing = true;
        }
    }

    pub(crate) fn end_cpu_drawing(&mut self, finished: bool) {
        if let Some(mut p) = self.presentation.as_mut() {
            p.cpu_drawing = false;
            if finished {
                p.finish_cpu_recolor();
            }
        }
    }

    pub(crate) fn dialog_snapshot_is_current(
        &self,
        saved: &SavedPixels,
        rect: (i16, i16, i16, i16),
        content_only: bool,
    ) -> bool {
        self.presentation.as_ref().is_some_and(|p| {
            p.restored_dialog == Some((saved.identity, rect, content_only, p.revision))
        })
    }
    pub(crate) fn remember_dialog_snapshot(
        &self,
        saved: &SavedPixels,
        rect: (i16, i16, i16, i16),
        content_only: bool,
    ) {
        if let Some(mut p) = self.presentation.as_mut() {
            p.restored_dialog = Some((saved.identity, rect, content_only, p.revision));
        }
    }

    // Only actual drawing invalidates an idle modal filter. Borrowing the store
    // to refresh an unchanged palette or save a snapshot is not a drawing event.
    /// Revision of the retained presentation state for frontend frame caches.
    /// It is a conservative invalidation token: offscreen retained detail may
    /// advance it even when the currently visible image is unchanged.
    pub fn presentation_epoch(&self) -> Option<u64> {
        self.presentation.as_ref().map(|p| p.revision)
    }

    /// Identity and revision of the currently visible retained image.
    /// Offscreen-only drawing deliberately leaves this token unchanged.
    pub fn presentation_visible_epoch(&self) -> Option<VisibleImageStamp> {
        self.presentation.as_ref().map(|p| p.visible_image.clone())
    }

    /// Most CopyBits spans carry no retained text on either side. Such a
    /// span is an ordinary byte copy, so write it in bulk and update the
    /// presentation's guest values once, instead of one presentation write per
    /// pixel. The caller guarantees the source span holds no detail (see
    /// `SavedPixels::has_detail_in`). Returns false, having written nothing,
    /// whenever the per-pixel path could behave differently: an active glyph
    /// capture, observed offscreen detail, a non-plain screen row, or any
    /// diagnostic, probe or protection gate `write_plain_presented_bytes`
    /// refuses.
    pub(crate) fn write_plain_copy_span(
        &mut self,
        address: u32,
        pixels: &SavedPixels,
        offset: usize,
        len: usize,
        palette: Option<&[u8; 256]>,
    ) -> bool {
        debug_assert!(!pixels.has_detail_in(offset..offset + len));
        if len == 0 {
            return false;
        }
        let span = offset..offset + len;
        let Some(destination) = self.range_translates_contiguously(address, len) else {
            return false;
        };
        let plain = self.presentation.as_ref().is_some_and(|p| {
            p.glyph.is_none()
                && (p.can_sync_plain_screen_row(destination, len)
                    || !p.observes_range(destination, len))
        });
        if !plain {
            return false;
        }
        let mut row = pixels.values[span].to_vec();
        if let Some(palette) = palette {
            for pixel in &mut row {
                *pixel = palette[*pixel as usize];
            }
        }
        self.write_plain_presented_bytes(address, &row)
    }

    /// Synchronize a native framebuffer mirror without erasing unchanged coverage.
    pub(crate) fn sync_presented_bytes(&mut self, destination: u32, source: u32, bytes: &[u8]) {
        if destination == source {
            return;
        }
        if self.presentation.is_none() {
            self.write_bytes(destination, bytes);
            return;
        }
        let source_end = u64::from(source) + bytes.len() as u64;
        let destination_address = self.translate_guest_address(destination);
        let plain_row = self.presentation.as_ref().is_some_and(|p| {
            p.can_sync_plain_screen_row(destination_address, bytes.len())
                && (!p.may_have_offscreen_detail(source, source_end)
                    || p.offscreen
                        .range(source..source + bytes.len() as u32)
                        .next()
                        .is_none())
        });
        if plain_row && self.write_plain_presented_bytes(destination, bytes) {
            return;
        }
        // Most mirror rows are unchanged. Compare contiguous RAM once while
        // retaining the routed/traced fallback and changed-byte write behavior.
        if !self
            .untraced_ram_slice(destination, bytes.len())
            .is_some_and(|previous| previous == bytes)
        {
            let previous = self.read_bytes(destination, bytes.len());
            for (i, (&old, &new)) in previous.iter().zip(bytes).enumerate() {
                if old != new {
                    self.write_byte(destination + i as u32, new);
                }
            }
        }
        if let Some(mut p) = self.presentation.as_mut() {
            if p.plain_screen_row(destination, bytes.len())
                && (!p.may_have_offscreen_detail(source, source_end)
                    || p.offscreen
                        .range(source..source + bytes.len() as u32)
                        .next()
                        .is_none())
            {
                return;
            }
            let changes = {
                let mut source_cells = p
                    .offscreen
                    .range(source..source + bytes.len() as u32)
                    .peekable();
                let mut changes = Vec::new();
                for (i, &value) in bytes.iter().enumerate() {
                    let address = source + i as u32;
                    let detail = if source_cells.peek().is_some_and(|(key, _)| **key == address) {
                        source_cells.next().map(|(_, cell)| cell)
                    } else {
                        None
                    };
                    let destination = destination + i as u32;
                    if !p.matches_detail(destination, detail) {
                        changes.push((destination, value, detail.cloned()));
                    }
                }
                changes
            };
            for (address, value, detail) in changes {
                if let Some(detail) = detail {
                    p.put_detail(address, &detail);
                } else {
                    p.write(address, value);
                }
            }
        }
    }

    pub(crate) fn outline_glyph_pixel(&mut self, address: u32, x: i16, y: i16, foreground: u8) {
        if self.presentation.is_none() {
            return;
        }
        let background = self.read_byte(address);
        if let Some(mut p) = self.presentation.as_mut() {
            p.glyph_pixel(address, x, y, foreground, background);
        }
    }

    pub(crate) fn save_pixel_bytes(&self, address: u32, len: usize) -> SavedPixels {
        let mut pixels = SavedPixels::from(self.read_bytes(address, len));
        // The sparse range walk, not a `detail` query per byte: callers save
        // whole menu bars and window frames every frame, and only text cells
        // and retained glyphs carry detail.
        self.presentation.capture_detail(&mut pixels, 0, address, len);
        pixels
    }

    pub(crate) fn transfer_saved_pixel(
        &mut self,
        address: u32,
        source: &SavedPixels,
        offset: usize,
        mut map: impl FnMut(&MacMemoryBus, u8, u8) -> u8,
    ) -> bool {
        let src = source.detail.get(&offset);
        let dst = self.presentation.as_ref().and_then(|p| p.detail(address));
        if src.is_none() && dst.is_none() {
            return false;
        }
        let old = self.read_byte(address);
        let value = map(self, source[offset], old);
        let scale = self.presentation.as_ref().unwrap().scale;
        let mut cell = DetailCell {
            value,
            indices: vec![value; (scale * scale) as usize],
            ink: HashMap::default(),
        };
        let color = |cell: Option<&DetailCell>, i: usize, fallback| {
            cell.map_or(IndexedColor::Solid(fallback), |cell| {
                cell.ink
                    .get(&i)
                    .map_or(IndexedColor::Solid(cell.indices[i]), |ink| {
                        ink.background.clone().over(ink.foreground, ink.alpha)
                    })
            })
        };
        for i in 0..cell.indices.len() {
            let src_color = color(src.map(AsRef::as_ref), i, source[offset]);
            let dst_color = color(dst.as_deref(), i, old);
            let output = if src_color == dst_color {
                let mut same = src_color;
                same.map(&mut |index| map(self, index, index));
                same
            } else {
                src_color.combine(&dst_color, &mut |s, d| map(self, s, d))
            };
            match output {
                IndexedColor::Solid(index) => cell.indices[i] = index,
                background => {
                    cell.ink.insert(
                        i,
                        Ink {
                            foreground: value,
                            alpha: 0,
                            background,
                        },
                    );
                }
            }
        }
        self.write_byte(address, value);
        if let Some(mut p) = self.presentation.as_mut() {
            p.put_detail(address, &Arc::new(cell));
        }
        true
    }

    pub(crate) fn copy_saved_pixel(
        &mut self,
        address: u32,
        pixels: &SavedPixels,
        offset: usize,
        mut map: impl Fn(u8) -> u8,
    ) {
        let value = map(pixels[offset]);
        self.write_byte(address, value);
        if let Some(cell) = pixels.detail.get(&offset) {
            // Most copies map nothing (no colour translation, same value):
            // share the snapshot's cell instead of cloning an identical one.
            let unchanged = cell.value == value
                && cell.indices.iter().all(|&index| map(index) == index)
                && cell.ink.values().all(|ink| {
                    map(ink.foreground) == ink.foreground && ink.background.fixed_under(&mut map)
                });
            let cell = if unchanged {
                cell.clone()
            } else {
                let mut cell = cell.clone();
                let mapped = Arc::make_mut(&mut cell);
                mapped.value = value;
                for index in &mut mapped.indices {
                    *index = map(*index);
                }
                for ink in mapped.ink.values_mut() {
                    ink.foreground = map(ink.foreground);
                    ink.background.map(&mut map);
                }
                cell
            };
            if let Some(mut p) = self.presentation.as_mut() {
                p.put_detail(address, &cell);
            }
        }
    }

    /// The current point in the screen's change history, or `None` without
    /// a presentation (then no screen write is tracked and nothing can be
    /// proven unchanged).
    pub(crate) fn screen_mark(&self) -> Option<ScreenMark> {
        self.presentation.as_ref().map(|p| p.screen_mark())
    }

    /// Whether every on-screen cell of `rect` (top, left, width, height) --
    /// guest byte, text coverage and ink -- is exactly as it was at `mark`.
    pub(crate) fn screen_rect_unchanged_since(
        &self,
        mark: ScreenMark,
        rect: (i16, i16, i16, i16),
    ) -> bool {
        self.presentation
            .as_ref()
            .is_some_and(|p| p.screen_rect_unchanged_since(mark, rect))
    }

    pub(crate) fn begin_cpu_pixel_copy(&mut self, source: u32, bytes: u32) -> bool {
        let addresses: [u32; 4] =
            std::array::from_fn(|i| self.translate_guest_address(source.wrapping_add(i as u32)));
        let Some(mut p) = self.presentation.as_mut() else {
            return false;
        };
        p.cpu_copy = Default::default();
        if (1..=4).contains(&bytes) {
            let contiguous =
                addresses[bytes as usize - 1].checked_sub(addresses[0]) == Some(bytes - 1);
            if contiguous && !p.observes_range(addresses[0], bytes as usize) {
                return false;
            }
            for offset in 0..bytes as usize {
                p.cpu_copy[offset] = p.detail(addresses[offset]);
            }
        }
        p.cpu_copy.iter().any(Option::is_some)
    }

    pub(crate) fn end_cpu_pixel_copy(&mut self, destination: Option<u32>) {
        let Some(mut p) = self.presentation.as_mut() else {
            return;
        };
        let detail = std::mem::take(&mut p.cpu_copy);
        drop(p);
        let Some(destination) = destination else {
            return;
        };
        for (offset, cell) in detail.into_iter().enumerate() {
            let Some(cell) = cell else { continue };
            let address = destination.wrapping_add(offset as u32);
            // The CPU owns the byte writes, including faults and protection.
            // Restore only metadata for bytes that were actually transferred.
            if self.is_guest_address_writable(address, 1) && self.read_byte(address) == cell.value {
                let address = self.translate_guest_address(address);
                if let Some(mut p) = self.presentation.as_mut() {
                    p.put_detail(address, &cell);
                }
            }
        }
    }

    /// Per-byte capture; also the test oracle for the range walk
    /// `save_pixel_bytes` uses.
    pub(crate) fn capture_pixel_detail<T>(
        &self,
        pixels: &mut SavedPixels<T>,
        offset: usize,
        address: u32,
        len: usize,
    ) {
        pixels.identity = next_snapshot_identity();
        if let Some(p) = self.presentation.as_ref() {
            if len == 0 {
                return;
            }
            let end = (u64::from(address) + len as u64).min(u64::from(u32::MAX) + 1);
            for (&addr, cell) in p.offscreen.range(address..=(end - 1) as u32) {
                if p.position(addr).is_some() {
                    continue;
                }
                pixels
                    .detail
                    .insert(offset + (addr - address) as usize, cell.clone());
            }
            let screen_start = u64::from(address).max(u64::from(p.base));
            let screen_end =
                end.min(u64::from(p.base) + u64::from(p.row_bytes) * u64::from(p.height));
            for addr in screen_start..screen_end {
                if let Some(cell) = p.detail(addr as u32) {
                    pixels
                        .detail
                        .insert(offset + (addr - u64::from(address)) as usize, cell);
                }
            }
        }
    }

    pub(crate) fn restore_saved_pixels<T: Copy + Into<u16>>(
        &mut self,
        address: u32,
        pixels: &SavedPixels<T>,
        offset: usize,
        len: usize,
    ) {
        let end = offset.saturating_add(len).min(pixels.len());
        if offset >= end {
            return;
        }
        if self.presentation.is_none() {
            let bytes: Vec<u8> = pixels[offset..end]
                .iter()
                .map(|value| (*value).into() as u8)
                .collect();
            self.write_bytes(address, &bytes);
            return;
        }
        // Unchanged chrome often restores the same entire row every frame.
        // Check its RAM with one route/tracing gate and borrow presentation
        // once. Equal guest bytes alone cannot establish equal outline ink.
        let same_bytes = self
            .untraced_ram_slice(address, end - offset)
            .is_some_and(|bytes| {
                bytes
                    .iter()
                    .zip(&pixels[offset..end])
                    .all(|(&byte, &value)| byte == value.into() as u8)
            });
        if same_bytes
            && self.presentation.as_ref().is_some_and(|p| {
                // Walking the two sparse sides is cheaper than asking about
                // every byte, but it declines ranges it cannot enumerate
                // cheaply; the per-byte proof stays for those.
                p.coverage_matches(address, end - offset, pixels, offset)
                    .unwrap_or_else(|| {
                        (offset..end).all(|i| {
                            let value = pixels[i].into() as u8;
                            let detail = pixels.detail.get(&i).filter(|cell| cell.value == value);
                            p.matches_detail(address + (i - offset) as u32, detail)
                        })
                    })
            })
        {
            return;
        }
        for i in offset..end {
            let dst = address + (i - offset) as u32;
            let value = pixels[i].into() as u8;
            let detail = pixels.detail.get(&i).filter(|cell| cell.value == value);
            if self.read_byte(dst) == value
                && self
                    .presentation
                    .as_ref()
                    .is_some_and(|p| p.matches_detail(dst, detail))
            {
                continue;
            }
            self.write_byte(dst, value);
            if let Some(cell) = detail {
                if let Some(mut p) = self.presentation.as_mut() {
                    p.put_detail(dst, cell);
                }
            }
        }
    }

    /// Apply the same indexed operation to each physical sample, retaining coverage.
    pub(crate) fn map_screen_byte(&mut self, address: u32, mut map: impl Fn(u8) -> u8) {
        let mut cell = self.presentation.as_ref().and_then(|p| p.detail(address));
        let value = map(self.read_byte(address));
        self.write_byte(address, value);
        if let Some(cell) = &mut cell {
            let mapped = Arc::make_mut(cell);
            mapped.value = value;
            for index in &mut mapped.indices {
                *index = map(*index);
            }
            for ink in mapped.ink.values_mut() {
                ink.foreground = map(ink.foreground);
                ink.background.map(&mut map);
            }
            if let Some(mut p) = self.presentation.as_mut() {
                p.put_detail(address, cell);
            }
        }
    }

    /// Invert indexed dialog selection pixels without flattening their outline
    /// coverage. Transform palette indexes, matching the guest's byte inversion.
    pub(crate) fn invert_screen_byte(&mut self, address: u32) {
        self.map_screen_byte(address, |index| !index);
    }

    /// Maintain a 4x outline surface for an indexed screen. Geometry changes
    /// recreate the surface; palette changes recolor the retained coverage.
    pub fn prepare_outline_presentation(
        &mut self,
        screen: (u32, u32, u16, u16, u16),
        palette: [[u8; 3]; 256],
    ) {
        if !matches!(screen.4, 8 | 16 | 32) {
            self.presentation.set(None);
            return;
        }
        let slot = self.presentation.clone();
        if slot.as_ref().is_some_and(|p| {
            (p.base, p.row_bytes, p.logical_width(), p.height, p.depth)
                == (
                    screen.0,
                    screen.1,
                    screen.2 as u32,
                    screen.3 as u32,
                    screen.4,
                )
                && p.scale == 4
        }) {
            let mut guard = slot.as_mut().unwrap();
            let p = &mut *guard;
            if p.depth == 8 && p.palette != palette {
                p.changed(true);
                // Indexed pixels retain their CLUT indexes when the device's
                // colors change (Imaging With QuickDraw, 1994, 4-5–4-6).
                p.palette = palette;
                // Plain image pixels are indexed until output; palette fades
                // only need to recolor the retained native text samples here.
                for (cell, &detail) in p.text_cells.iter().enumerate() {
                    if !detail {
                        continue;
                    }
                    let samples = p.samples.get_mut(cell);
                    for (rgb, &index) in samples.rgb.iter_mut().zip(&samples.indices) {
                        *rgb = palette[index as usize];
                    }
                }
                for (&offset, ink) in &p.ink {
                    p.samples.get_mut(offset / TILE_SAMPLES).rgb[offset % TILE_SAMPLES] =
                        ink.rgb(&palette);
                }
            }
        } else {
            // Drop the slot borrow before replacing its surface.
            self.enable_outline_presentation(screen, palette, 4);
        }
    }

    /// Sampling scale used by cached retained-pixel snapshots.
    pub(crate) fn outline_presentation_scale(&self) -> Option<u32> {
        self.presentation.as_ref().map(|p| p.scale)
    }

    /// Whether an outline presentation surface is available to a frontend.
    pub fn has_outline_presentation(&self) -> bool {
        self.presentation.is_some()
    }

    /// Frames without visible retained text can use the ordinary framebuffer
    /// presenter while continuing to track offscreen text for later copies.
    pub fn has_visible_outline_detail(&self) -> bool {
        let Some(p) = self.presentation.as_ref() else {
            return false;
        };
        debug_assert_eq!(
            p.text_cell_count,
            p.text_cells.iter().filter(|&&text| text).count(),
            "text_cell_count must track text_cells"
        );
        p.text_cell_count != 0
    }

    /// Composite host overlays onto the outline surface. Overlay positions stay
    /// in guest coordinates; only the returned presentation dimensions change.
    pub fn presented_argb(
        &self,
        guest: &[u32],
        with_overlays: &[u32],
    ) -> Option<(u32, u32, Vec<u32>)> {
        let p = self.presentation.as_ref()?;
        if guest.len() != (p.logical_width() * p.height) as usize
            || with_overlays.len() != guest.len()
        {
            return None;
        }
        let width = p.logical_width() * p.scale;
        let height = p.height * p.scale;
        let mut pixels = p.render_pixels(|c| {
            0xff000000 | ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
        });
        for (index, (&before, &after)) in guest.iter().zip(with_overlays).enumerate() {
            if before == after {
                continue;
            }
            let x = index as u32 % p.logical_width();
            let y = index as u32 / p.logical_width();
            for dy in 0..p.scale {
                let start = ((y * p.scale + dy) * width + x * p.scale) as usize;
                pixels[start..start + p.scale as usize].fill(after);
            }
        }
        Some((width, height, pixels))
    }

    /// Render retained coverage into a reusable output buffer at the scale
    /// needed by the drawable, independently of the internal sampling scale.
    pub fn presented_argb_scaled(
        &self,
        guest: &[u32],
        with_overlays: &[u32],
        scale: u32,
        output: &mut Vec<u32>,
    ) -> Option<(u32, u32)> {
        let p = self.presentation.as_ref()?;
        if !(1..=4).contains(&scale)
            || guest.len() != (p.logical_width() * p.height) as usize
            || with_overlays.len() != guest.len()
        {
            return None;
        }
        output.clear();
        output.extend_from_slice(&p.resolved_argb(scale));
        let width = p.logical_width() * scale;
        if !std::ptr::eq(guest.as_ptr(), with_overlays.as_ptr()) {
            for (index, (&before, &after)) in guest.iter().zip(with_overlays).enumerate() {
                if before == after {
                    continue;
                }
                let x = index as u32 % p.logical_width();
                let y = index as u32 / p.logical_width();
                for dy in 0..scale {
                    let start = ((y * scale + dy) * width + x * scale) as usize;
                    output[start..start + scale as usize].fill(after);
                }
            }
        }
        Some((width, p.height * scale))
    }

    /// Borrow the resolved outline image at `scale` for a caller that knows no
    /// overlay changed a pixel this frame.
    ///
    /// [`MacMemoryBus::presented_argb_scaled`] copies the resolved image into
    /// the caller's buffer so that host overlays can be patched in. When the
    /// cursor is hidden and no debug overlay is drawn, that buffer would be a
    /// byte-for-byte copy, so the present path can read the image in place and
    /// skip both the copy and the guest/overlay comparison.
    pub fn presented_argb_cached(&self, scale: u32) -> Option<PresentedOutline<'_>> {
        let slot = self.presentation.as_ref()?;
        if !(1..=4).contains(&scale) {
            return None;
        }
        let size = (slot.logical_width() * scale, slot.height * scale);
        if size.0 == 0 || size.1 == 0 {
            return None;
        }
        Some(PresentedOutline { slot, scale, size })
    }

    /// RGBA version of `presented_argb_scaled`, using the same coverage rules.
    pub fn presented_rgba_scaled(
        &self,
        guest: &[u8],
        with_overlays: &[u8],
        scale: u32,
        output: &mut Vec<u8>,
    ) -> Option<(u32, u32)> {
        let p = self.presentation.as_ref()?;
        if !(1..=4).contains(&scale)
            || guest.len() != (p.logical_width() * p.height * 4) as usize
            || with_overlays.len() != guest.len()
        {
            return None;
        }
        let pixels = p.resolved_rgba(scale);
        output.clear();
        output.extend_from_slice(&pixels);
        let width = p.logical_width() * scale;
        if !std::ptr::eq(guest.as_ptr(), with_overlays.as_ptr()) {
            for (index, (before, after)) in guest
                .chunks_exact(4)
                .zip(with_overlays.chunks_exact(4))
                .enumerate()
            {
                if before == after {
                    continue;
                }
                let x = index as u32 % p.logical_width();
                let y = index as u32 / p.logical_width();
                for dy in 0..scale {
                    let start = ((y * scale + dy) * width + x * scale) as usize * 4;
                    for pixel in output[start..start + scale as usize * 4].chunks_exact_mut(4) {
                        pixel.copy_from_slice(after);
                    }
                }
            }
        }
        Some((width, p.height * scale))
    }

    /// RGBA counterpart of `presented_argb` for browser and image frontends.
    pub fn presented_rgba(
        &self,
        guest: &[u8],
        with_overlays: &[u8],
    ) -> Option<(u32, u32, Vec<u8>)> {
        let p = self.presentation.as_ref()?;
        let logical_width = p.logical_width();
        if guest.len() != (logical_width * p.height * 4) as usize
            || with_overlays.len() != guest.len()
        {
            return None;
        }
        let width = logical_width * p.scale;
        let mut pixels: Vec<u8> = p
            .render_pixels(|c| [c[0], c[1], c[2], 255])
            .into_iter()
            .flatten()
            .collect();
        for (index, (before, after)) in guest
            .chunks_exact(4)
            .zip(with_overlays.chunks_exact(4))
            .enumerate()
        {
            if before == after {
                continue;
            }
            let x = index as u32 % logical_width;
            let y = index as u32 / logical_width;
            for dy in 0..p.scale {
                let start = ((y * p.scale + dy) * width + x * p.scale) as usize * 4;
                for pixel in pixels[start..start + p.scale as usize * 4].chunks_exact_mut(4) {
                    pixel.copy_from_slice(after);
                }
            }
        }
        Some((width, p.height * p.scale, pixels))
    }

    /// Start an indexed outline surface at 2x through 4x resolution.
    /// Frontends normally use `prepare_outline_presentation` to track mode changes.
    pub fn enable_outline_presentation(
        &mut self,
        screen: (u32, u32, u16, u16, u16),
        palette: [[u8; 3]; 256],
        scale: u32,
    ) {
        let (base, row_bytes, width, height, depth) = screen;
        assert!(matches!(depth, 8 | 16 | 32));
        let width = u32::from(width) * u32::from(depth / 8);
        assert!((2..=4).contains(&scale));
        assert!(width > 0 && height > 0 && row_bytes >= u32::from(width));
        assert!(
            u64::from(base) + u64::from(row_bytes) * u64::from(height)
                <= u64::from(self.ram_size())
        );
        let retained = self.presentation.as_mut().and_then(|mut previous| {
            (previous.depth == depth && previous.scale == scale).then(|| {
                (
                    std::mem::take(&mut previous.offscreen),
                    previous.glyph_count,
                )
            })
        });
        let mut presentation = Presentation {
            revision: 0,
            identity: next_presentation_identity(),
            screen_epoch: 0,
            tile_epochs: vec![0; screen_tiles_per_row(width.into()) * usize::from(height)],
            visible_image: VisibleImageStamp {
                identity: std::rc::Rc::new(()),
                revision: 0,
            },
            cpu_drawing: false,
            cpu_recolor: None,
            cpu_copy: Default::default(),
            restored_dialog: None,
            output_cache: std::cell::RefCell::new(None),
            offscreen: BTreeMap::new(),
            offscreen_bounds: None,
            offscreen_pages: PageIndex::default(),
            store_filter_new_pages: Vec::new(),
            store_filter_identity: next_store_filter_identity(),
            base,
            row_bytes,
            row_reciprocal: (1u64 << 32) / u64::from(row_bytes.max(1)),
            width: width.into(),
            height: height.into(),
            depth,
            direct_palettes: std::array::from_fn(|lane| {
                std::array::from_fn(|i| {
                    let byte = i as u8;
                    let expand = |v: u8| (v << 3) | (v >> 2);
                    match (depth, lane) {
                        (16, 0) => [expand((byte >> 2) & 31), (byte & 3) * 66, 0],
                        (16, 1) => [0, ((byte >> 5) & 7) * 8 + (byte >> 7), expand(byte & 31)],
                        (32, 1) => [byte, 0, 0],
                        (32, 2) => [0, byte, 0],
                        (32, 3) => [0, 0, byte],
                        _ => [0; 3],
                    }
                })
            }),
            scale,
            palette,
            samples: DetailSamples::new(width as usize * height as usize),
            guest_values: vec![256; width as usize * height as usize],
            text_cells: vec![false; width as usize * height as usize],
            text_cell_count: 0,
            detail_cache: std::cell::RefCell::new(vec![None; width as usize * height as usize]),
            ink: HashMap::default(),
            ink_mask: vec![0; width as usize * height as usize],
            run_ink: HashSet::default(),
            offscreen_run_ink: HashSet::new(),
            in_text_run: false,
            erasing_text: false,
            glyph: None,
            glyph_count: 0,
        };
        for y in 0..u32::from(height) {
            for x in 0..u32::from(width) {
                let address = base + y * row_bytes + x;
                presentation.write(address, self.read_byte(address));
            }
        }
        if let Some((offscreen, glyph_count)) = retained {
            presentation.offscreen_bounds = offscreen
                .first_key_value()
                .zip(offscreen.last_key_value())
                .map(|((&first, _), (&last, _))| (first, last));
            for &address in offscreen.keys() {
                presentation
                    .offscreen_pages
                    .mark(u64::from(address), u64::from(address) + 1);
            }
            presentation.offscreen = offscreen;
            presentation.glyph_count = glyph_count;
        }
        self.presentation.set(Some(presentation));
    }

    /// Return the real guest's presentation capture and number of outline draws.
    pub fn outline_presentation_rgb(&self) -> Option<(u32, u32, Vec<u8>, usize)> {
        let p = self.presentation.as_ref()?;
        Some((
            p.logical_width() * p.scale,
            p.height * p.scale,
            p.render_pixels(|rgb| rgb).into_iter().flatten().collect(),
            p.glyph_count,
        ))
    }

    pub(crate) fn begin_presentation_text_run(&mut self, opaque: bool) {
        self.presentation.begin_presentation_text_run(opaque);
    }
    pub(crate) fn end_presentation_text_run(&mut self) {
        self.presentation.end_presentation_text_run();
    }
    pub(crate) fn begin_outline_glyph(
        &mut self,
        glyph: &Glyph,
        data: &[u8],
        x: i16,
        y: i16,
        bold: bool,
        italic: Option<i16>,
        underline: Option<(i16, i16)>,
    ) {
        self.presentation
            .begin_outline_glyph(glyph, data, x, y, bold, italic, underline);
    }
    pub(crate) fn style_outline_glyph(
        &mut self,
        style: crate::quickdraw::text::QuickDrawTextStyle,
    ) {
        self.presentation.style_outline_glyph(style);
    }
    pub(crate) fn end_outline_glyph(&mut self) {
        self.presentation.end_outline_glyph();
    }
}
impl PresentationSlot {
    pub(crate) fn begin_presentation_text_run(&mut self, opaque: bool) {
        if let Some(mut p) = self.as_mut() {
            p.run_ink.clear();
            p.offscreen_run_ink.clear();
            p.in_text_run = opaque;
        }
    }

    pub(crate) fn end_presentation_text_run(&mut self) {
        if let Some(mut p) = self.as_mut() {
            p.run_ink.clear();
            p.offscreen_run_ink.clear();
            p.in_text_run = false;
        }
    }

    pub(crate) fn begin_outline_glyph(
        &mut self,
        glyph: &Glyph,
        data: &[u8],
        x: i16,
        y: i16,
        bold: bool,
        italic_descent: Option<i16>,
        underline: Option<(i16, i16)>,
    ) {
        let Some(mut p) = self.as_mut() else {
            return;
        };
        if let Some(mut outline) = outline::presentation_glyph(glyph, data, p.scale) {
            if let Some(descent) = italic_descent {
                // Apply the shared QuickDraw shear on the physical grid rather
                // than enlarging the already sheared one-bit strike.
                let bottom = (i32::from(descent) - 1) * p.scale as i32;
                let shift = |row: i32| (bottom - outline.top - row).max(0) / 2;
                let width = outline.width + shift(0);
                let mut pixels = vec![0; (width * outline.height) as usize];
                for row in 0..outline.height {
                    let src = (row * outline.width) as usize;
                    let dst = (row * width + shift(row)) as usize;
                    pixels[dst..dst + outline.width as usize]
                        .copy_from_slice(&outline.pixels[src..src + outline.width as usize]);
                }
                outline.width = width;
                outline.pixels = pixels;
            }
            if bold && outline.width > 0 {
                let width = outline.width + p.scale as i32;
                let mut pixels = vec![0; (width * outline.height) as usize];
                for y in 0..outline.height {
                    for x in 0..outline.width {
                        let alpha = outline.pixels[(y * outline.width + x) as usize];
                        for shift in 0..=p.scale as i32 {
                            let dst = &mut pixels[(y * width + x + shift) as usize];
                            *dst = (*dst).max(alpha);
                        }
                    }
                }
                outline.width = width;
                outline.pixels = pixels;
            }
            if let Some((advance, thickness)) = underline {
                let scale = p.scale as i32;
                let left = outline.left.min(0);
                let top = outline.top.min(scale);
                let right = (outline.left + outline.width).max(i32::from(advance) * scale);
                let bottom = (outline.top + outline.height).max((1 + i32::from(thickness)) * scale);
                let width = right - left;
                let height = bottom - top;
                let mut pixels = vec![0; (width * height) as usize];
                // Underlines break around descenders. Measure their coverage at
                // the physical resolution, keeping a one-guest-pixel clearance.
                let mut descenders = vec![false; width as usize];
                for row in 0..outline.height {
                    for col in 0..outline.width {
                        let alpha = outline.pixels[(row * outline.width + col) as usize];
                        let px = outline.left + col - left;
                        let py = outline.top + row - top;
                        pixels[(py * width + px) as usize] = alpha;
                        if outline.top + row >= 0 && alpha >= 128 {
                            for nearby in (px - scale).max(0)..=(px + scale).min(width - 1) {
                                descenders[nearby as usize] = true;
                            }
                        }
                    }
                }
                for px in -left..i32::from(advance) * scale - left {
                    if !descenders[px as usize] {
                        for py in scale - top..(1 + i32::from(thickness)) * scale - top {
                            pixels[(py * width + px) as usize] = 255;
                        }
                    }
                }
                outline = OutlineGlyph {
                    pixels,
                    width,
                    height,
                    left,
                    top,
                };
            }
            p.glyph = Some((outline, x, y));
            super::note_store_filter_event();
            p.glyph_count += 1;
        }
    }

    /// Synthesize hollow outline/shadow masks on the physical grid using the
    /// same smear-and-remove rule as the logical QuickDraw renderer.
    pub(crate) fn style_outline_glyph(
        &mut self,
        style: crate::quickdraw::text::QuickDrawTextStyle,
    ) {
        let Some(mut p) = self.as_mut() else {
            return;
        };
        let p = &mut *p;
        let Some((glyph, _, _)) = &mut p.glyph else {
            return;
        };
        let Some(radius) = style.smear_max() else {
            return;
        };
        let scale = p.scale as i32;
        let pad = scale;
        let width = glyph.width + pad + radius * scale;
        let height = glyph.height + pad + radius * scale;
        let mut pixels = vec![0u8; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                let mut alpha = 0;
                for dy in -scale..=radius * scale {
                    for dx in -scale..=radius * scale {
                        let gx = x - pad - dx;
                        let gy = y - pad - dy;
                        if gx >= 0 && gy >= 0 && gx < glyph.width && gy < glyph.height {
                            alpha = alpha.max(glyph.pixels[(gy * glyph.width + gx) as usize]);
                        }
                    }
                }
                let gx = x - pad;
                let gy = y - pad;
                if gx >= 0 && gy >= 0 && gx < glyph.width && gy < glyph.height {
                    alpha = alpha.saturating_sub(glyph.pixels[(gy * glyph.width + gx) as usize]);
                }
                pixels[(y * width + x) as usize] = alpha;
            }
        }
        glyph.left -= pad;
        glyph.top += style.glyph_y_offset() * scale - pad;
        glyph.width = width;
        glyph.height = height;
        glyph.pixels = pixels;
    }

    pub(crate) fn end_outline_glyph(&mut self) {
        if let Some(mut p) = self.as_mut() {
            p.glyph = None;
            super::note_store_filter_event();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn bus() -> MacMemoryBus {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.fill_bytes(0x1000, 64, 255);
        bus.enable_outline_presentation((0x1000, 8, 8, 8, 8), palette, 2);
        bus
    }

    pub(super) fn paint_detail(bus: &mut MacMemoryBus, address: u32) {
        bus.presentation.as_mut().unwrap().glyph = Some((
            OutlineGlyph {
                pixels: vec![64, 255, 0, 128],
                width: 2,
                height: 2,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        bus.outline_glyph_pixel(address, 0, 0, 0);
        bus.write_byte(address, 0);
        bus.end_outline_glyph();
    }

    /// JIT store filter bytes: [0] global, [1 + page] per 4 KiB page.
    fn page_byte(bus: &MacMemoryBus, address: u32) -> u8 {
        bus.store_filter_byte(1 + (address >> STORE_FILTER_PAGE_SHIFT) as usize)
            .unwrap()
    }

    #[test]
    fn store_filter_learns_plain_and_observed_pages() {
        let mut bus = bus();
        assert!(!bus.store_filter_for_batch().is_null());
        assert_eq!(page_byte(&bus, 0x2_0000), 1, "unknown until written");
        bus.write_long(0x2_0000, 7);
        assert_eq!(page_byte(&bus, 0x2_0000), 0, "plain RAM page");
        bus.write_byte(0x1004, 3);
        assert_eq!(page_byte(&bus, 0x1004), 2, "the screen page is observed");
        bus.write_word(0x2_0ffe, 1);
        assert_eq!(bus.store_filter_byte(0), Some(0), "no probe, no glyph");
    }

    #[test]
    fn new_offscreen_detail_withdraws_a_plain_proof() {
        let mut bus = bus();
        bus.store_filter_for_batch();
        bus.write_long(0x3_0000, 1);
        assert_eq!(page_byte(&bus, 0x3_0000), 0);
        paint_detail(&mut bus, 0x3_0010);
        bus.store_filter_for_batch();
        assert_ne!(page_byte(&bus, 0x3_0000), 0, "offscreen detail now lives here");
        // Other proven pages are untouched by an incremental change.
        bus.write_long(0x4_0000, 1);
        paint_detail(&mut bus, 0x3_0020);
        bus.store_filter_for_batch();
        assert_eq!(page_byte(&bus, 0x4_0000), 0);
    }

    #[test]
    fn replacing_the_presentation_resets_every_proof() {
        let mut bus = bus();
        bus.store_filter_for_batch();
        bus.write_long(0x2_0000, 1);
        assert_eq!(page_byte(&bus, 0x2_0000), 0);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.enable_outline_presentation((0x1000, 8, 8, 8, 8), palette, 2);
        bus.store_filter_for_batch();
        assert_eq!(page_byte(&bus, 0x2_0000), 1);
    }

    #[test]
    fn protected_code_resets_proofs_and_is_never_plain() {
        let mut bus = bus();
        bus.store_filter_for_batch();
        bus.write_long(0x5_0000, 1);
        assert_eq!(page_byte(&bus, 0x5_0000), 0);
        bus.protect_readonly_code(0x5_0100, 16);
        bus.store_filter_for_batch();
        assert_eq!(page_byte(&bus, 0x5_0000), 1);
        bus.write_long(0x5_0000, 2);
        assert_eq!(page_byte(&bus, 0x5_0000), 2, "the page holds protected code");
    }

    #[test]
    fn an_active_write_probe_sets_the_global_byte() {
        let mut bus = bus();
        bus.store_filter_for_batch();
        bus.begin_write_probe();
        bus.store_filter_for_batch();
        assert_eq!(bus.store_filter_byte(0), Some(1));
        let suspended = bus.suspend_write_probe().unwrap();
        bus.store_filter_for_batch();
        assert_eq!(bus.store_filter_byte(0), Some(0), "suspended probes record nothing");
        bus.resume_write_probe(suspended);
        bus.store_filter_for_batch();
        assert_eq!(bus.store_filter_byte(0), Some(1));
        bus.finish_write_probe_unchanged();
        bus.store_filter_for_batch();
        assert_eq!(bus.store_filter_byte(0), Some(0));
    }

    /// A screen whose rows may be wider than their visible pixels, so spans
    /// can start in padding, cross a row, or leave the framebuffer entirely.
    fn padded_bus(row_bytes: u16, width: u16, height: u16, scale: u32) -> MacMemoryBus {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.fill_bytes(0x1000, u32::from(row_bytes) * u32::from(height), 255);
        bus.enable_outline_presentation(
            (0x1000, u32::from(row_bytes), width, height, 8),
            palette,
            scale,
        );
        bus
    }

    #[test]
    fn range_capture_agrees_with_the_per_byte_oracle() {
        // Screen text, text in row padding (x >= width), and retained glyphs
        // before, after and between screen rows, captured over every span
        // that starts and ends around them.
        let mut bus = padded_bus(12, 8, 8, 2);
        for address in [0x1000, 0x1003, 0x1009, 0x100b, 0x1025, 0x105f, 0x0ffd, 0x1060, 0x1063] {
            paint_detail(&mut bus, address);
        }
        let points = [0x0ff8u32, 0x0ffd, 0x0ffe, 0x1000, 0x1004, 0x1008, 0x100b, 0x100c, 0x1024, 0x1026, 0x105f, 0x1060, 0x1064];
        let mut compared = 0;
        for &start in &points {
            for &end in &points {
                if end <= start {
                    continue;
                }
                let len = (end - start) as usize;
                let mut oracle = SavedPixels::from(bus.read_bytes(start, len));
                bus.capture_pixel_detail(&mut oracle, 0, start, len);
                let fast = bus.save_pixel_bytes(start, len);
                assert_eq!(fast.values, oracle.values, "{start:#x}+{len}");
                let keys = |p: &SavedPixels| {
                    let mut k: Vec<_> = p.detail.iter().map(|(&i, c)| (i, (**c).clone())).collect();
                    k.sort_by_key(|(i, _)| *i);
                    k
                };
                assert_eq!(keys(&fast), keys(&oracle), "{start:#x}+{len}");
                compared += 1;
            }
        }
        assert!(compared > 50);
    }

    #[test]
    fn ink_mask_tracks_the_ink_map_through_draws_overwrites_and_restores() {
        for scale in [2u32, 4] {
            let mut bus = padded_bus(10, 8, 6, scale);
            let check = |bus: &MacMemoryBus, step: &str| {
                assert!(
                    bus.presentation.as_ref().unwrap().ink_mask_matches_ink(),
                    "scale={scale} {step}"
                );
            };
            // Antialiased glyphs (partial and full alpha) across two rows.
            for address in [0x1000u32, 0x1001, 0x1003, 0x1000 + 10, 0x1000 + 12] {
                paint_detail(&mut bus, address);
                check(&bus, "after glyph");
            }
            let saved = bus.save_pixel_bytes(0x1000, 16);
            // Plain overwrites of text cells, including one to the same value.
            for (address, value) in [(0x1001u32, 7u8), (0x1000 + 10, 9), (0x1003, 0)] {
                bus.write_byte(address, value);
                check(&bus, "after overwrite");
            }
            // Restoring the snapshot puts retained detail back.
            bus.restore_saved_pixels(0x1000, &saved, 0, saved.len());
            check(&bus, "after restore");
            // Redraw over existing ink, then erase everything plainly.
            paint_detail(&mut bus, 0x1000);
            check(&bus, "after redraw");
            for address in 0x1000u32..0x1000 + 20 {
                bus.write_byte(address, 3);
            }
            check(&bus, "after erase");
        }
    }

    #[test]
    fn divide_row_matches_hardware_division() {
        for row_bytes in [1u32, 2, 3, 7, 8, 10, 63, 64, 640, 641, 832, 1024, 1920, 4095, 65535] {
            let reciprocal = (1u64 << 32) / u64::from(row_bytes);
            let offsets = (0u32..5000)
                .chain((0..64).map(|k| u32::MAX - k))
                .chain((1..4096u32).map(|k| k.wrapping_mul(0x9E37_79B9)))
                .chain((0..2000).map(|k| k * row_bytes))
                .chain((1..2000).map(|k| k * row_bytes - 1));
            for offset in offsets {
                assert_eq!(
                    divide_row(offset, row_bytes, reciprocal),
                    (offset % row_bytes, offset / row_bytes),
                    "{offset} / {row_bytes}"
                );
            }
        }
    }

    /// The bulk CopyBits row path must leave RAM, the rendered presentation
    /// and any later snapshot exactly as the per-pixel copy does, whether it
    /// takes the fast path (plain rows, plain offscreen RAM) or declines
    /// (source text detail, rows crossing the padding).
    #[test]
    fn plain_copy_rows_match_the_per_pixel_copy() {
        use crate::copy_bits::CopyBitsMemory;
        let inverted: [u8; 256] = std::array::from_fn(|i| (255 - i) as u8);
        let source = 0x3_0000u32;
        let setup = || {
            let mut bus = padded_bus(10, 8, 6, 2);
            // Unrelated retained text elsewhere on screen and offscreen.
            paint_detail(&mut bus, 0x1000 + 10 * 3 + 1);
            paint_detail(&mut bus, 0x4_0000);
            bus
        };
        // (destination, length, source detail offsets, fast path expected)
        for (destination, len, detail, bulk) in [
            (0x1000u32, 8usize, vec![], true),
            (0x1000 + 10 + 3, 3, vec![], true),
            // Crosses the row padding into the next row: declines.
            (0x1000 + 10 * 2 + 6, 6, vec![], false),
            (0x2_0000, 16, vec![], true),
            (0x1000 + 10 * 4, 8, vec![2usize], false),
            (0x2_0100, 8, vec![0usize, 7], false),
            // Overwrites the retained offscreen glyph: declines.
            (0x4_0000 - 2, 4, vec![], false),
            // The screen row that already holds unrelated text: declines.
            (0x1000 + 10 * 3, 8, vec![], false),
            // Source text onto that row: the plain runs decline too.
            (0x1000 + 10 * 3, 8, vec![4usize], false),
        ] {
            for palette in [None, Some(&inverted)] {
                let mut fast = setup();
                let mut slow = setup();
                for bus in [&mut fast, &mut slow] {
                    for i in 0..len as u32 {
                        bus.write_byte(source + i, (i * 37 + 5) as u8);
                    }
                    for &offset in &detail {
                        paint_detail(bus, source + offset as u32);
                    }
                }
                let pixels = fast.save_pixel_bytes(source, len);
                let context = format!("{destination:#x}+{len} detail={detail:?} palette={}", palette.is_some());
                let mut probe = setup();
                for i in 0..len as u32 {
                    probe.write_byte(source + i, (i * 37 + 5) as u8);
                }
                for &offset in &detail {
                    paint_detail(&mut probe, source + offset as u32);
                }
                assert_eq!(
                    !pixels.has_detail_in(0..len)
                        && probe.write_plain_copy_span(destination, &pixels, 0, len, palette),
                    bulk,
                    "{context}: fast path taken"
                );
                fast.write_copy_pixels(destination, &pixels, 0, len, palette)
                    .expect("writable destination");
                for i in 0..len {
                    slow.copy_saved_pixel(destination + i as u32, &pixels, i, |index| {
                        palette.map_or(index, |table| table[index as usize])
                    });
                }
                assert_eq!(fast.read_bytes(destination, len), slow.read_bytes(destination, len), "{context}: RAM");
                assert_eq!(fast.outline_presentation_rgb(), slow.outline_presentation_rgb(), "{context}: rendered");
                let after_fast = fast.save_pixel_bytes(destination - 2, len + 4);
                let after_slow = slow.save_pixel_bytes(destination - 2, len + 4);
                assert_eq!(after_fast.values, after_slow.values, "{context}: snapshot values");
                let keys = |p: &SavedPixels| {
                    let mut k: Vec<_> = p.detail.iter().map(|(&i, c)| (i, (**c).clone())).collect();
                    k.sort_by_key(|(i, _)| *i);
                    k
                };
                assert_eq!(keys(&after_fast), keys(&after_slow), "{context}: snapshot detail");
            }
        }
    }

    /// The proof `restore_saved_pixels` used before the range walk existed,
    /// written out independently so it can judge the range walk's answer.
    fn per_byte_coverage_proof(
        bus: &MacMemoryBus,
        address: u32,
        len: usize,
        pixels: &SavedPixels,
        offset: usize,
    ) -> bool {
        let p = bus.presentation.as_ref().unwrap();
        (offset..offset + len).all(|i| {
            let value = pixels[i];
            let detail = pixels.detail.get(&i).filter(|cell| cell.value == value);
            p.matches_detail(address + (i - offset) as u32, detail)
        })
    }

    /// Every configuration the range walk accepts must reach the same verdict
    /// as asking about each byte in turn.
    #[test]
    fn coverage_proof_agrees_with_the_per_byte_oracle() {
        // Unpadded, padded, and a row too short to reach its own padding.
        for (row_bytes, width, height, scale) in
            [(8u16, 8u16, 8u16, 2u32), (10, 8, 6, 2), (12, 4, 4, 4)]
        {
            let stride = u32::from(row_bytes);
            let base = 0x1000u32;
            let screen_bytes = stride * u32::from(height);
            for painted in [
                vec![],
                vec![0u32],
                vec![1, 2],
                vec![0, stride, stride + 3],
                vec![stride * 2 + 1],
                // A glyph in the row padding, which is retained off-screen.
                vec![u32::from(width)],
            ] {
                for &(start, len) in &[
                    (0u32, 1usize),
                    (0, u32::from(width) as usize),
                    (1, 3),
                    (u32::from(width) as u32, 2),
                    (stride - 1, 3),
                    (0, (stride * 2) as usize),
                    (stride * u32::from(height) - 2, 4),
                    (0, screen_bytes as usize),
                ] {
                    let address = base + start;
                    // Judge the snapshot as taken, then after each way the
                    // screen can drift from it. The fixture is rebuilt for
                    // every case so one disturbance cannot leak into the next.
                    for disturb in 0..5 {
                        let mut bus = padded_bus(row_bytes, width, height, scale);
                        for &offset in &painted {
                            if offset < screen_bytes {
                                paint_detail(&mut bus, base + offset);
                            }
                        }
                        let saved = bus.save_pixel_bytes(address, len);
                        match disturb {
                            0 => {}
                            // A same-value store erases retained coverage.
                            1 => bus.write_byte(address, bus.read_byte(address)),
                            // A different value changes bytes and coverage.
                            2 => bus.write_byte(address, 7),
                            // New coverage where the snapshot had none.
                            3 => paint_detail(&mut bus, address),
                            // Coverage elsewhere in the same row.
                            _ => {
                                if len > 1 {
                                    paint_detail(&mut bus, address + 1);
                                }
                            }
                        }
                        let expected = per_byte_coverage_proof(&bus, address, len, &saved, 0);
                        let actual = bus
                            .presentation
                            .as_ref()
                            .unwrap()
                            .coverage_matches(address, len, &saved, 0);
                        if let Some(actual) = actual {
                            assert_eq!(
                                actual, expected,
                                "row_bytes={row_bytes} width={width} scale={scale} \
                                 painted={painted:?} start={start} len={len} disturb={disturb}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// The same verdicts through a non-zero snapshot offset, which is how the
    /// dialog and window restores address a snapshot row by row.
    #[test]
    fn coverage_proof_agrees_when_restoring_part_of_a_snapshot() {
        for (offset, len) in [(0usize, 8usize), (3, 5), (8, 4), (10, 8), (20, 10), (0, 30)] {
            for disturb in 0..3 {
                let mut bus = padded_bus(10, 8, 6, 2);
                paint_detail(&mut bus, 0x1000 + 3);
                paint_detail(&mut bus, 0x1000 + 10);
                let saved = bus.save_pixel_bytes(0x1000, 30);
                let address = 0x1000 + offset as u32;
                match disturb {
                    0 => {}
                    1 => bus.write_byte(address, bus.read_byte(address)),
                    _ => paint_detail(&mut bus, address),
                }
                let expected = per_byte_coverage_proof(&bus, address, len, &saved, offset);
                let verdict = {
                    let p = bus.presentation.as_ref().unwrap();
                    p.coverage_matches(address, len, &saved, offset)
                };
                if let Some(actual) = verdict {
                    assert_eq!(
                        actual, expected,
                        "offset={offset} len={len} disturb={disturb}"
                    );
                }
            }
        }
    }

    /// Equal guest bytes do not prove equal retained coverage: a same-value
    /// store erases a glyph, and restoring the snapshot must bring it back.
    #[test]
    fn restore_returns_coverage_erased_by_a_same_value_store() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        let saved = bus.save_pixel_bytes(0x1000, 8);
        assert!(!saved.detail.is_empty(), "the snapshot retained a glyph");
        let before = bus.presentation_visible_epoch();

        let value = bus.read_byte(0x1000);
        bus.write_byte(0x1000, value);
        assert_eq!(bus.read_byte(0x1000), value, "the bytes never changed");
        assert_ne!(
            bus.presentation_visible_epoch(),
            before,
            "the glyph was erased"
        );

        bus.restore_saved_pixels(0x1000, &saved, 0, 8);
        assert_eq!(bus.read_bytes(0x1000, 8), *saved, "bytes restored");
        assert!(
            per_byte_coverage_proof(&bus, 0x1000, 8, &saved, 0),
            "the erased glyph is retained again"
        );
    }

    /// The bounded scan accepts short rows inside larger snapshots but still
    /// declines tiny spans in a comparatively large table.
    #[test]
    fn the_range_walk_declines_by_buckets_and_accepts_a_sparse_table() {
        // Dense: every byte of a short snapshot carries a glyph, so the table
        // has at least as many buckets as the range has bytes.
        {
            let mut dense_bus = bus();
            for offset in 0..8 {
                paint_detail(&mut dense_bus, 0x1000 + offset);
            }
            let dense = dense_bus.save_pixel_bytes(0x1000, 8);
            assert_eq!(dense.detail.len(), 8);
            assert!(
                dense.detail.capacity() > 8,
                "a dense table outgrows its range"
            );
            let p = dense_bus.presentation.as_ref().unwrap();
            assert_eq!(p.coverage_matches(0x1000, 1, &dense, 0), None);
            for len in [4, 8] {
                assert_eq!(
                    p.coverage_matches(0x1000, len, &dense, 0),
                    Some(per_byte_coverage_proof(&dense_bus, 0x1000, len, &dense, 0)),
                );
            }
        }

        // Sparse: one glyph in a 64-byte row, the shape that matters.
        let mut sparse_bus = bus();
        paint_detail(&mut sparse_bus, 0x1000 + 3);
        let sparse = sparse_bus.save_pixel_bytes(0x1000, 64);
        assert_eq!(sparse.detail.len(), 1);
        assert!(
            sparse.detail.capacity() <= 64,
            "a sparse table stays inside its range"
        );
        let expected = per_byte_coverage_proof(&sparse_bus, 0x1000, 64, &sparse, 0);
        let p = sparse_bus.presentation.as_ref().unwrap();
        assert_eq!(
            p.coverage_matches(0x1000, 64, &sparse, 0),
            Some(expected),
            "walks a range it can enumerate, and agrees"
        );
        assert!(expected, "an untouched snapshot still matches");
    }

    /// Emptying a table does not return its buckets, and walking one costs a
    /// pass over those. A snapshot that once held glyphs must keep declining
    /// after `replace_range` removes them, or the bound would not bound
    /// anything.
    #[test]
    fn an_emptied_but_still_large_table_keeps_declining() {
        let mut bus = bus();
        for offset in 0..8 {
            paint_detail(&mut bus, 0x1000 + offset);
        }
        let mut saved = bus.save_pixel_bytes(0x1000, 8);
        let buckets = saved.detail.capacity();
        assert!(buckets >= 8);

        saved.replace_range(0, &[0u8; 8]);
        assert!(saved.detail.is_empty(), "every cell was dropped");
        assert_eq!(saved.detail.capacity(), buckets, "the buckets stayed");

        let p = bus.presentation.as_ref().unwrap();
        for len in 1..=buckets.min(8) {
            let verdict = p.coverage_matches(0x1000, len, &saved, 0);
            if len.saturating_mul(8) < buckets {
                assert_eq!(
                    verdict, None,
                    "len={len} exceeds the bounded table-scan cost"
                );
            }
            // Whatever it answers, it must not contradict the per-byte proof.
            if let Some(verdict) = verdict {
                assert_eq!(
                    verdict,
                    per_byte_coverage_proof(&bus, 0x1000, len, &saved, 0)
                );
            }
        }
    }

    /// A span reaching the last address has an exclusive end no `u32` holds.
    /// The proof must stay exact there rather than read a failed conversion as
    /// success.
    ///
    /// Reaching that conversion needs retained coverage outside the screen, so
    /// the range query actually runs: an empty offscreen table short-circuits
    /// before it. Treating a failed exclusive `u32::try_from(to)` conversion
    /// as success would wrongly prove equality in the missing-coverage case.
    #[test]
    fn a_span_reaching_the_last_address_is_still_proved_exactly() {
        // A cell painted on screen, then retained at the very top address so
        // the offscreen bounds and pages cover it.
        fn bus_with_glyph_at_the_top() -> (MacMemoryBus, Arc<DetailCell>) {
            let mut bus = bus();
            paint_detail(&mut bus, 0x1000);
            let cell = bus
                .presentation
                .as_ref()
                .unwrap()
                .detail(0x1000)
                .expect("the painted glyph is retained");
            {
                let mut p = bus.presentation.as_mut().unwrap();
                p.put_detail(u32::MAX, &cell);
            }
            assert!(
                bus.presentation
                    .as_ref()
                    .unwrap()
                    .may_have_offscreen_detail(u32::MAX, u64::from(u32::MAX) + 1),
                "the range query must be reached, not short-circuited"
            );
            (bus, cell)
        }

        // A snapshot long enough that the capacity guard accepts it, ending
        // exactly at 2^32.
        const LEN: usize = 64;
        let address = u32::MAX - (LEN as u32 - 1);

        for saved_coverage in ["matching", "missing", "changed"] {
            let (bus, cell) = bus_with_glyph_at_the_top();
            let mut saved = SavedPixels::from(vec![cell.value; LEN]);
            match saved_coverage {
                "matching" => {
                    saved.detail.insert(LEN - 1, cell.clone());
                }
                "missing" => {}
                _ => {
                    let mut different = (*cell).clone();
                    different.indices = different.indices.iter().map(|i| i ^ 1).collect();
                    saved.detail.insert(LEN - 1, Arc::new(different));
                }
            }
            assert!(
                saved.detail.capacity() <= LEN,
                "the capacity guard must accept this range"
            );

            let expected = per_byte_coverage_proof(&bus, address, LEN, &saved, 0);
            let p = bus.presentation.as_ref().unwrap();
            assert_eq!(
                p.coverage_matches(address, LEN, &saved, 0),
                Some(expected),
                "coverage={saved_coverage} at a span ending one past {:#x}",
                u32::MAX
            );
        }
    }

    #[test]
    fn key_count_proof_keeps_stale_source_entries_and_empty_ranges_exact() {
        for stale in [vec![], vec![0usize], vec![0, 3], vec![0, 3, 8, 16]] {
            for disturbance in 0..3 {
                let mut bus = bus();
                for offset in [0, 3, 8, 16] {
                    paint_detail(&mut bus, 0x1000 + offset);
                }
                let saved = bus.save_pixel_bytes(0x1000, 64);
                let mut index = 0usize;
                let saved = saved.map(|value| {
                    let changed = stale.contains(&index);
                    index += 1;
                    if changed {
                        value ^ 1
                    } else {
                        value
                    }
                });
                assert_eq!(saved.detail.len(), 4, "map keeps the source entries");
                for &offset in &stale {
                    bus.write_byte(0x1000 + offset as u32, saved[offset]);
                }
                match disturbance {
                    0 => {}
                    1 => bus.write_byte(0x1003, saved[3]),
                    _ => paint_detail(&mut bus, 0x1007),
                }
                // Full snapshot, shifted rows, a range without source keys,
                // and the zero-length case all retain the original verdict.
                for (offset, len) in [(0, 64), (0, 8), (3, 6), (8, 9), (24, 8), (24, 0)] {
                    let address = 0x1000 + offset as u32;
                    let expected = per_byte_coverage_proof(&bus, address, len, &saved, offset);
                    let actual = bus
                        .presentation
                        .as_ref()
                        .unwrap()
                        .coverage_matches(address, len, &saved, offset);
                    if len > 0 {
                        assert_eq!(actual, Some(expected), "stale={stale:?}, disturbance={disturbance}, offset={offset}, len={len}");
                    } else if let Some(actual) = actual {
                        assert_eq!(actual, expected);
                    }
                }
            }
        }
    }

    #[test]
    fn saved_coverage_survives_tile_reuse_and_palette_changes() {
        for scale in [2, 3, 4] {
            let mut bus = bus();
            let screen = (0x1000, 8, 8, 8, 8);
            let palette = std::array::from_fn(|i| [i as u8; 3]);
            bus.enable_outline_presentation(screen, palette, scale);
            paint_detail(&mut bus, 0x1000);
            let saved = bus.save_pixel_bytes(0x1000, 1);
            let guest = [0; 64];
            let expected = bus.presented_argb(&guest, &guest).unwrap();
            let slot = {
                let p = bus.presentation.as_ref().unwrap();
                p.samples.get(0) as *const samples::SampleTile
            };

            // Erase the source, then reuse its storage for different coverage
            // in another row. The saved snapshot must remain independent.
            bus.write_byte(0x1000, 255);
            bus.write_byte(0x1008, 64);
            paint_detail(&mut bus, 0x1008);
            assert_eq!(
                bus.presentation.as_ref().unwrap().samples.get(8) as *const samples::SampleTile,
                slot
            );
            bus.restore_saved_pixels(0x1000, &saved, 0, 1);
            bus.write_byte(0x1008, 255);
            assert_eq!(bus.presented_argb(&guest, &guest).unwrap(), expected);

            if scale == 4 {
                let mut changed = palette;
                changed[0] = [20, 40, 80];
                bus.prepare_outline_presentation(screen, changed);
                assert_ne!(bus.presented_argb(&guest, &guest).unwrap(), expected);
                bus.prepare_outline_presentation(screen, palette);
                assert_eq!(bus.presented_argb(&guest, &guest).unwrap(), expected);
            }
        }
    }

    #[test]
    fn framebuffer_sync_preserves_plain_rows_and_updates_changed_bytes() {
        let mut bus = bus();
        let epoch = bus.presentation_epoch();
        bus.sync_presented_bytes(0x1000, 0x8000, &[255; 8]);
        assert_eq!(bus.presentation_epoch(), epoch);
        bus.sync_presented_bytes(0x1000, 0x8000, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(bus.read_bytes(0x1000, 8), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(bus
            .presentation
            .as_ref()
            .unwrap()
            .plain_screen_row(0x1000, 8));
        assert!(!bus
            .presentation
            .as_ref()
            .unwrap()
            .plain_screen_row(0x1007, 2));
    }

    #[test]
    fn framebuffer_sync_plain_row_bulk_path_translates_addresses_and_revises_once() {
        let mut bus = bus();
        bus.set_addressing_32_bit(false);
        let epoch = bus.presentation_epoch().unwrap();
        bus.sync_presented_bytes(0x0100_1000, 0x8000, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(bus.read_bytes(0x1000, 8), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(bus.presentation_epoch(), Some(epoch + 1));
    }

    #[test]
    fn framebuffer_sync_plain_row_falls_back_while_cpu_drawing() {
        let mut bus = bus();
        bus.begin_cpu_drawing();
        let epoch = bus.presentation_epoch().unwrap();
        bus.sync_presented_bytes(0x1000, 0x8000, &[1, 2, 3, 4, 5, 6, 7, 8]);
        bus.end_cpu_drawing(false);
        assert_eq!(bus.read_bytes(0x1000, 8), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(bus.presentation_epoch(), Some(epoch + 8));
    }

    #[test]
    fn framebuffer_sync_plain_row_finishes_pending_cpu_recolor_before_writing() {
        let mut bus = bus();
        bus.begin_cpu_drawing();
        bus.write_byte(0x1000, 254);
        bus.end_cpu_drawing(false);
        let epoch = bus.presentation_epoch().unwrap();
        bus.sync_presented_bytes(0x1000, 0x8000, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(bus.read_bytes(0x1000, 8), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(bus.presentation_epoch(), Some(epoch + 8));
    }

    #[test]
    fn framebuffer_sync_plain_row_respects_readonly_and_write_probes() {
        let mut readonly_bus = bus();
        readonly_bus.protect_readonly_code(0x1000, 8);
        let epoch = readonly_bus.presentation_epoch();
        readonly_bus.sync_presented_bytes(0x1000, 0x8000, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(readonly_bus.read_bytes(0x1000, 8), [255; 8]);
        assert_eq!(readonly_bus.presentation_epoch(), epoch);

        let mut probe_bus = bus();
        probe_bus.begin_uncapped_write_probe();
        probe_bus.sync_presented_bytes(0x1000, 0x8000, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(probe_bus.read_bytes(0x1000, 8), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(!probe_bus.finish_write_probe_unchanged());
    }

    #[test]
    fn plain_screen_row_rejects_framebuffer_padding() {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.enable_outline_presentation((0x1000, 12, 8, 2, 8), palette, 2);
        assert!(bus
            .presentation
            .as_ref()
            .unwrap()
            .plain_screen_row(0x1000, 8));
        assert!(!bus
            .presentation
            .as_ref()
            .unwrap()
            .plain_screen_row(0x1007, 2));
    }

    #[test]
    fn framebuffer_sync_updates_coverage_even_when_guest_bytes_match() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        let same_bytes = bus.read_bytes(0x1000, 8);
        assert!(bus.presentation.as_ref().unwrap().detail(0x1000).is_some());
        bus.sync_presented_bytes(0x1000, 0x8000, &same_bytes);
        assert!(bus.presentation.as_ref().unwrap().detail(0x1000).is_none());

        paint_detail(&mut bus, 0x8000);
        let source_detail = bus.presentation.as_ref().unwrap().detail(0x8000).unwrap();
        bus.sync_presented_bytes(0x1000, 0x8000, &same_bytes);
        assert_eq!(
            bus.presentation.as_ref().unwrap().detail(0x1000),
            Some(source_detail)
        );
        let epoch = bus.presentation_epoch();
        bus.sync_presented_bytes(0x1000, 0x8000, &same_bytes);
        assert_eq!(bus.presentation_epoch(), epoch);
    }

    #[test]
    fn framebuffer_sync_clears_coverage_across_rows_and_after_source_erasure() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1008);
        let bytes = bus.read_bytes(0x1007, 2);
        bus.sync_presented_bytes(0x1007, 0x8000, &bytes);
        assert!(bus.presentation.as_ref().unwrap().detail(0x1008).is_none());
        paint_detail(&mut bus, 0x8000);
        bus.sync_presented_bytes(0x1000, 0x8000, &[0; 8]);
        bus.write_byte(0x8000, 0); // same byte erases retained source coverage
        bus.sync_presented_bytes(0x1000, 0x8000, &[0; 8]);
        assert!(bus.presentation.as_ref().unwrap().detail(0x1000).is_none());
    }

    #[test]
    fn offscreen_detail_bounds_preserve_insertions_and_partial_ranges() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x8000);
        let detail = bus.presentation.as_ref().unwrap().detail(0x8000).unwrap();
        let mut p = bus.presentation.as_mut().unwrap();
        assert!(!p.observes_range(0x7000, 0x1000));
        assert!(p.observes_range(0x7fff, 2));
        assert!(!p.observes_range(0x8001, 1));
        p.put_detail(0x9000, &detail);
        p.put_detail(0x6000, &detail);
        assert_eq!(p.detail(0x6000), Some(detail.clone()));
        assert!(p.observes_range(0x8fff, 2));
        assert!(!p.observes_range(0x6001, 0x1fff));
        // Bounds can stay conservative after erasure: the map remains the
        // authority for ranges inside them, including a removed endpoint.
        p.write(0x6000, detail.value);
        assert!(p.detail(0x6000).is_none());
        assert!(!p.observes_range(0x6000, 1));
        assert_eq!(p.detail(0x9000), Some(detail.clone()));
        p.put_detail(u32::MAX, &detail);
        assert!(p.observes_range(u32::MAX, 1));
        assert!(!p.observes_range(u32::MAX, 0));
        assert!(p.observes_range(u32::MAX - 1, 3));
        p.write(u32::MAX, detail.value);
        assert!(p.detail(u32::MAX).is_none());
        drop(p);
        // Changing the screen mapping can retain offscreen glyphs; their
        // bounds must survive that transfer into a new presentation surface.
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.enable_outline_presentation((0x2000, 8, 8, 8, 8), palette, 2);
        assert!(bus.presentation.as_ref().unwrap().observes_range(0x9000, 1));
        assert_eq!(
            bus.presentation.as_ref().unwrap().detail(0x9000),
            Some(detail)
        );
        bus.write_byte(0x9000, 0);
        assert!(bus.presentation.as_ref().unwrap().detail(0x9000).is_none());
    }

    #[test]
    fn guest_cpu_recolor_preserves_coverage_across_execution_budgets() {
        use crate::cpu::{M68kCpu, Register, StepResult};

        for budget in [0, 1, 7, 1000] {
            let mut bus = bus();
            paint_detail(&mut bus, 0x1001);
            let mut expected = bus.presentation.as_ref().unwrap().detail(0x1001).unwrap();
            let map = |index| if index == 0 { 0 } else { index ^ 1 };
            let cell = Arc::make_mut(&mut expected);
            for index in &mut cell.indices {
                *index = map(*index);
            }
            for ink in cell.ink.values_mut() {
                ink.foreground = map(ink.foreground);
                ink.background.map(&mut |index| map(index));
            }
            // MOVE.B (A0),D0; TST.B D0; BEQ store; EORI.B #1,D0;
            // store: MOVE.B D0,(A0)+; DBRA D1,loop; SwapMMUMode.
            // Like a direct indexed highlight, this rewrites unchanged black
            // letters as well as changing their background color.
            for (i, word) in [
                0x1010, 0x4a00, 0x6704, 0x0a00, 1, 0x10c0, 0x51c9, 0xfff2, 0xa05d,
            ]
            .into_iter()
            .enumerate()
            {
                bus.write_word(0x200 + i as u32 * 2, word);
            }
            let mut cpu = M68kCpu::new();
            cpu.write_reg(Register::PC, 0x200);
            cpu.write_reg(Register::A0, 0x1000);
            cpu.write_reg(Register::D1, 15);
            for _ in 0..200 {
                let finished = if budget == 0 {
                    matches!(cpu.step(&mut bus), StepResult::Aline(0xa05d))
                } else {
                    matches!(
                        cpu.run_batch(&mut bus, budget, &[]).exit,
                        m68k::BatchExit::AlineTrap { opcode: 0xa05d }
                    )
                };
                if finished {
                    break;
                }
            }
            assert_eq!(cpu.read_reg(Register::PC), 0x212);
            assert_eq!(
                bus.read_bytes(0x1000, 16),
                [vec![254], vec![0], vec![254; 14]].concat()
            );
            assert_eq!(
                bus.presentation.as_ref().unwrap().detail(0x1001),
                Some(expected)
            );
            // A subsequent native erase must remove all retained coverage.
            bus.write_byte(0x1001, 0);
            assert!(bus.presentation.as_ref().unwrap().detail(0x1001).is_none());
        }
    }

    #[test]
    fn cpu_recolor_rejects_erases_conflicts_missing_colors_and_repeated_stores() {
        for writes in [
            vec![(0x1000, 0), (0x1001, 0)], // fill collapses background and ink
            vec![(0x1000, 254), (0x1001, 0), (0x1002, 253)], // conflicting background map
            vec![(0x1001, 1)],              // no evidence for the antialiased background color
            vec![(0x1000, 255), (0x1001, 0)], // ordinary same-value rewrite
            vec![(0x1000, 254), (0x1001, 0), (0x1001, 0)], // overlapping passes
        ] {
            let mut bus = bus();
            paint_detail(&mut bus, 0x1001);
            bus.begin_cpu_drawing();
            for (address, value) in writes {
                bus.write_byte(address, value);
            }
            bus.end_cpu_drawing(true);
            assert!(bus.presentation.as_ref().unwrap().detail(0x1001).is_none());
        }
    }

    #[test]
    fn native_drawing_finishes_pending_cpu_recolor_before_overwriting_it() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1001);
        bus.begin_cpu_drawing();
        bus.write_byte(0x1000, 254);
        bus.write_byte(0x1001, 0);
        bus.end_cpu_drawing(false);
        // The frontend can draw between CPU slices. Its later pixel wins.
        bus.write_byte(0x1001, 42);
        bus.end_cpu_drawing(true);
        assert_eq!(bus.read_byte(0x1001), 42);
        let p = bus.presentation.as_ref().unwrap();
        assert_eq!(p.guest_values[1], 42);
        assert!(p.detail(0x1001).is_none());
    }

    #[test]
    fn cpu_pixel_save_restore_keeps_coverage_and_real_erases_remove_it() {
        for (opcode, bytes) in [(0x12d8, 1), (0x32d8, 2), (0x22d8, 4)] {
            for cpu_type in [m68k::CpuType::M68000, m68k::CpuType::M68020] {
                let mut bus = bus();
                paint_detail(&mut bus, 0x1000);
                let original = bus.presentation.as_ref().unwrap().detail(0x1000).unwrap();
                let mut cpu = m68k::CpuCore::new();
                cpu.set_cpu_type(cpu_type);
                bus.write_word(0x200, opcode);
                bus.write_word(0x202, opcode);
                bus.write_word(0x204, 0x4e71);
                cpu.pc = 0x200;
                cpu.set_a(0, 0x1000);
                cpu.set_a(1, 0x8000);
                assert_eq!(cpu.run_batch(&mut bus, 1, &[]).instructions, 1);
                assert_eq!(
                    bus.presentation.as_ref().unwrap().detail(0x8000),
                    Some(original.clone())
                );

                // A sprite overwrites the screen, then restores its saved background.
                bus.fill_bytes(0x1000, bytes, 255);
                assert!(bus.presentation.as_ref().unwrap().detail(0x1000).is_none());
                cpu.set_a(0, 0x8000);
                cpu.set_a(1, 0x1000);
                assert_eq!(cpu.run_batch(&mut bus, 1, &[]).instructions, 1);
                assert_eq!(
                    bus.presentation.as_ref().unwrap().detail(0x1000),
                    Some(original)
                );

                // An ordinary store of the same logical byte is still an erase.
                let value = bus.read_byte(0x1000);
                bus.write_byte(0x1000, value);
                assert!(bus.presentation.as_ref().unwrap().detail(0x1000).is_none());
            }
        }
    }

    #[test]
    fn hot_cpu_copy_loops_preserve_detail_and_hot_stores_erase_it() {
        use m68k::{AddressBus, CpuCore, CpuType};
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        let original = bus.presentation.as_ref().unwrap().detail(0x1000).unwrap();
        assert!(AddressBus::fast_mem(&mut bus).is_none());
        assert!(AddressBus::tracked_mem(&mut bus).is_some());
        // MOVE.L (A0),(A1); DBRA D0,loop. Repeat enough to compile the copy.
        for (i, word) in [0x2290, 0x51c8, 0xfffc].into_iter().enumerate() {
            MemoryBus::write_word(&mut bus, 0x200 + i as u32 * 2, word);
        }
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(CpuType::M68040);
        cpu.set_a(0, 0x1000);
        cpu.set_a(1, 0x8000);
        cpu.set_d(0, 1023);
        cpu.pc = 0x200;
        assert_eq!(cpu.run_batch(&mut bus, 2048, &[0x206]).instructions, 2048);
        assert_eq!(
            bus.presentation.as_ref().unwrap().detail(0x8000),
            Some(original.clone())
        );
        bus.fill_bytes(0x1000, 4, 255);
        cpu.set_a(0, 0x8000);
        cpu.set_a(1, 0x1000);
        cpu.set_d(0, 1023);
        cpu.pc = 0x200;
        assert_eq!(cpu.run_batch(&mut bus, 2048, &[0x206]).instructions, 2048);
        assert_eq!(
            bus.presentation.as_ref().unwrap().detail(0x1000),
            Some(original)
        );
        // Replacing the recorded copy with a register store must invalidate
        // the old trace and erase coverage even when its logical bytes match.
        MemoryBus::write_word(&mut bus, 0x200, 0x2281); // MOVE.L D1,(A1)
        cpu.set_d(1, MemoryBus::read_long(&bus, 0x1000));
        cpu.set_d(0, 1023);
        cpu.pc = 0x200;
        assert_eq!(cpu.run_batch(&mut bus, 2048, &[0x206]).instructions, 2048);
        assert!(bus.presentation.as_ref().unwrap().detail(0x1000).is_none());
    }

    #[test]
    fn cpu_copy_does_not_resurrect_detail_invalidated_in_the_saved_background() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        bus.begin_cpu_pixel_copy(0x1000, 4);
        let value = bus.read_long(0x1000);
        bus.write_long(0x8000, value);
        bus.end_cpu_pixel_copy(Some(0x8000));
        bus.write_byte(0x8000, bus.read_byte(0x8000));
        bus.begin_cpu_pixel_copy(0x8000, 4);
        bus.write_long(0x1000, value);
        bus.end_cpu_pixel_copy(Some(0x1000));
        assert!(bus.presentation.as_ref().unwrap().detail(0x1000).is_none());
    }

    #[test]
    fn output_scales_match_full_coverage_resampling_for_both_pixel_orders() {
        for (depth, retained) in [8u16, 16, 32]
            .into_iter()
            .flat_map(|depth| (2..=4).map(move |retained| (depth, retained)))
        {
            let mut bus = bus();
            let lanes = u32::from(depth / 8);
            bus.enable_outline_presentation(
                (0x1000, 8 * lanes, 8, 8, depth),
                std::array::from_fn(|i| [i as u8; 3]),
                retained,
            );
            paint_detail(&mut bus, 0x1000);
            let guest = vec![0xff123456; 64];
            let mut overlay = guest.clone();
            overlay[7] = 0xffabcdef;
            let (w, h, full) = bus.presented_argb(&guest, &overlay).unwrap();
            let rgba = |pixels: &[u32]| {
                pixels
                    .iter()
                    .flat_map(|p| {
                        [
                            (*p >> 16) as u8,
                            (*p >> 8) as u8,
                            *p as u8,
                            (*p >> 24) as u8,
                        ]
                    })
                    .collect::<Vec<_>>()
            };
            let rgba_guest = rgba(&guest);
            for scale in 1..=4 {
                let mut expected = Vec::new();
                crate::display::resize_argb_coverage(
                    &full,
                    (w, h),
                    (8 * scale, 8 * scale),
                    &mut expected,
                );
                let mut actual = Vec::new();
                assert_eq!(
                    bus.presented_argb_scaled(&guest, &overlay, scale, &mut actual),
                    Some((8 * scale, 8 * scale))
                );
                assert_eq!(actual, expected, "depth={depth} scale={scale}");
                let mut actual_rgba = Vec::new();
                let rgba_overlay = rgba(&overlay);
                bus.presented_rgba_scaled(&rgba_guest, &rgba_overlay, scale, &mut actual_rgba)
                    .unwrap();
                assert_eq!(actual_rgba, rgba(&expected));

                // The browser passes the same logical buffer when there are
                // no host overlays. Keep that path pixel-identical while it
                // skips the redundant guest/overlay comparison.
                let mut expected_no_overlay = Vec::new();
                bus.presented_argb_scaled(&guest, &guest, scale, &mut expected_no_overlay)
                    .unwrap();
                let mut actual_no_overlay = Vec::new();
                bus.presented_rgba_scaled(&rgba_guest, &rgba_guest, scale, &mut actual_no_overlay)
                    .unwrap();
                assert_eq!(actual_no_overlay, rgba(&expected_no_overlay));
            }
        }
    }

    #[test]
    fn resolved_output_cache_tracks_writes_restores_palette_and_overlay_removal() {
        let mut bus = bus();
        let screen = (0x1000, 8, 8, 8, 8);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.prepare_outline_presentation(screen, palette);
        paint_detail(&mut bus, 0x1000);
        let saved = bus.save_pixel_bytes(0x1000, 64);
        let guest = [0; 64];
        let mut output = Vec::new();
        bus.presented_argb_scaled(&guest, &guest, 2, &mut output)
            .unwrap();
        let expected = output.clone();
        let (cached_ptr, cached_capacity) = {
            let presentation = bus.presentation.as_ref().unwrap();
            let cache = presentation.output_cache.borrow();
            match &cache.as_ref().unwrap().pixels {
                ResolvedOutput::Argb(pixels) => (pixels.as_ptr(), pixels.capacity()),
                ResolvedOutput::Rgba(_) => unreachable!(),
            }
        };
        let mut overlay = guest;
        overlay[0] = 0xffabcdef;
        bus.presented_argb_scaled(&guest, &overlay, 2, &mut output)
            .unwrap();
        assert_ne!(output, expected);
        output.clear();
        bus.presented_argb_scaled(&guest, &guest, 2, &mut output)
            .unwrap();
        assert_eq!(output, expected);
        bus.write_byte(0x1000, bus.read_byte(0x1000));
        bus.presented_argb_scaled(&guest, &guest, 2, &mut output)
            .unwrap();
        assert_ne!(output, expected);
        let (reused_ptr, reused_capacity) = {
            let presentation = bus.presentation.as_ref().unwrap();
            let cache = presentation.output_cache.borrow();
            match &cache.as_ref().unwrap().pixels {
                ResolvedOutput::Argb(pixels) => (pixels.as_ptr(), pixels.capacity()),
                ResolvedOutput::Rgba(_) => unreachable!(),
            }
        };
        assert_eq!(reused_ptr, cached_ptr);
        assert_eq!(reused_capacity, cached_capacity);
        bus.restore_saved_pixels(0x1000, &saved, 0, 64);
        bus.presented_argb_scaled(&guest, &guest, 2, &mut output)
            .unwrap();
        assert_eq!(output, expected);
        let mut changed_palette = palette;
        changed_palette[0] = [255, 0, 0];
        bus.prepare_outline_presentation(screen, changed_palette);
        bus.presented_argb_scaled(&guest, &guest, 2, &mut output)
            .unwrap();
        assert_ne!(output, expected);
    }

    /// The borrowed outline image must be byte-identical to what the copying
    /// presenter produces, in every state a frame can present.
    #[test]
    fn borrowed_outline_matches_copied_presentation() {
        let mut bus = bus();
        let screen = (0x1000, 8, 8, 8, 8);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.prepare_outline_presentation(screen, palette);
        paint_detail(&mut bus, 0x1000);
        let guest = [0; 64];
        for scale in 1..=4 {
            let mut copied = Vec::new();
            bus.presented_argb_scaled(&guest, &guest, scale, &mut copied)
                .unwrap();
            let outline = bus.presented_argb_cached(scale).unwrap();
            assert_eq!(outline.size(), (8 * scale, 8 * scale));
            let pixels = outline.pixels();
            assert_eq!(pixels.len(), copied.len());
            assert_eq!(&pixels[..], &copied[..]);
        }
        assert!(bus.presented_argb_cached(0).is_none());
        assert!(bus.presented_argb_cached(5).is_none());

        // A guest write must be visible through both, and the overlay-aware
        // presenter must still differ once an overlay actually changes a pixel.
        bus.write_byte(0x1000, 7);
        let mut copied = Vec::new();
        bus.presented_argb_scaled(&guest, &guest, 2, &mut copied)
            .unwrap();
        let outline = bus.presented_argb_cached(2).unwrap();
        assert_eq!(&outline.pixels()[..], &copied[..]);
        let cached = outline.pixels().to_vec();
        drop(outline);
        let mut overlay = guest;
        overlay[0] = 0xffabcdef;
        let mut patched = Vec::new();
        bus.presented_argb_scaled(&guest, &overlay, 2, &mut patched)
            .unwrap();
        assert_ne!(patched, cached);
    }

    #[test]
    fn cached_visible_rgba_composes_moving_cursor_and_removes_overlays() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        let token = bus.presentation_visible_epoch();
        let guest = vec![0; 64 * 4];
        let mut output = Vec::new();
        bus.presented_rgba_scaled(&guest, &guest, 2, &mut output)
            .unwrap();
        let clean = output.clone();
        let cursor = crate::display::CursorImage::mono([255; 32], [255; 32], 0, 0);
        let mut previous = clean.clone();
        for position in [(0, 0), (3, 4)] {
            let mut overlay = guest.clone();
            crate::display::render_cursor(&mut overlay, 8, 8, &cursor, position);
            bus.presented_rgba_scaled(&guest, &overlay, 2, &mut output)
                .unwrap();
            assert_ne!(output, previous);
            previous = output.clone();
            assert_eq!(bus.presentation_visible_epoch(), token);
        }
        bus.presented_rgba_scaled(&guest, &guest, 2, &mut output)
            .unwrap();
        assert_eq!(output, clean);
        assert_eq!(bus.presentation_visible_epoch(), token);
    }

    #[test]
    fn visible_epoch_tracks_surface_palette_coverage_and_offscreen_drawing() {
        let mut bus = bus();
        let initial = bus.presentation_visible_epoch();
        paint_detail(&mut bus, 0x2000);
        assert_eq!(bus.presentation_visible_epoch(), initial);
        let saved = bus.save_pixel_bytes(0x2000, 2);
        bus.restore_saved_pixels(0x1000, &saved, 0, 2);
        let retained = bus.presentation_visible_epoch();
        assert_ne!(retained, initial);
        bus.write_byte(0x1000, bus.read_byte(0x1000));
        let erased = bus.presentation_visible_epoch();
        assert_ne!(erased, retained);
        bus.write_byte(0x1000, 77);
        let written = bus.presentation_visible_epoch();
        assert_ne!(written, erased);
        let mut palette = std::array::from_fn(|i| [i as u8; 3]);
        palette[77] = [255, 0, 0];
        let screen = (0x1000, 8, 8, 8, 8);
        bus.prepare_outline_presentation(screen, palette);
        let recolored = bus.presentation_visible_epoch();
        assert_ne!(recolored, written);
        bus.enable_outline_presentation(screen, palette, 4);
        let replaced = bus.presentation_visible_epoch();
        assert_ne!(replaced, recolored);
        bus.enable_outline_presentation(screen, palette, 4);
        assert_ne!(bus.presentation_visible_epoch(), replaced);
    }

    #[test]
    fn resolved_outputs_ignore_offscreen_drawing_and_reject_wrapped_revisions() {
        for format in 0..3 {
            let mut bus = bus();
            // Force a cache at revision zero, then mutate through a wrap to zero.
            bus.presentation.as_mut().unwrap().visible_image.revision = 0;
            let resolve = |bus: &MacMemoryBus| -> Vec<u8> {
                let p = bus.presentation.as_ref().unwrap();
                match format {
                    0 => p.resolved_rgba(2).to_vec(),
                    1 => p
                        .resolved_argb(2)
                        .iter()
                        .flat_map(|p| p.to_le_bytes())
                        .collect(),
                    _ => {
                        let guest = [0; 64];
                        let mut output = Vec::new();
                        bus.presented_argb_resized(&guest, &guest, (13, 11), &mut output)
                            .unwrap();
                        output.iter().flat_map(|p| p.to_le_bytes()).collect()
                    }
                }
            };
            let before = resolve(&bus);
            let token = bus.presentation_visible_epoch();
            paint_detail(&mut bus, 0x2000);
            assert_eq!(bus.presentation_visible_epoch(), token);
            assert_eq!(resolve(&bus), before);
            assert_eq!(
                bus.presentation
                    .as_ref()
                    .unwrap()
                    .output_cache
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .source,
                token.clone().unwrap()
            );
            bus.presentation.as_mut().unwrap().revision = u64::MAX;
            bus.write_byte(0x1000, 77);
            assert_eq!(bus.presentation.as_ref().unwrap().visible_image.revision, 0);
            assert_ne!(bus.presentation_visible_epoch(), token);
            assert_ne!(resolve(&bus), before);
        }
    }

    #[test]
    fn resolved_output_cache_switches_argb_rgba_without_stale_pixels() {
        let mut bus = bus();
        let screen = (0x1000, 8, 8, 8, 8);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.prepare_outline_presentation(screen, palette);
        paint_detail(&mut bus, 0x1000);

        let guest_argb = [0; 64];
        let guest_rgba = [0; 64 * 4];
        let mut argb = Vec::new();
        let mut rgba = Vec::new();
        bus.presented_argb_scaled(&guest_argb, &guest_argb, 2, &mut argb)
            .unwrap();
        bus.presented_rgba_scaled(&guest_rgba, &guest_rgba, 2, &mut rgba)
            .unwrap();
        let expected_rgba = argb
            .iter()
            .flat_map(|pixel| {
                [
                    (*pixel >> 16) as u8,
                    (*pixel >> 8) as u8,
                    *pixel as u8,
                    (*pixel >> 24) as u8,
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(rgba, expected_rgba);

        bus.write_byte(0x1000, bus.read_byte(0x1000));
        bus.presented_rgba_scaled(&guest_rgba, &guest_rgba, 2, &mut rgba)
            .unwrap();
        assert_ne!(rgba, expected_rgba);
    }

    #[test]
    fn unchanged_palette_and_snapshot_preserve_modal_filter_epoch() {
        let mut bus = bus();
        let screen = (0x1000, 8, 8, 8, 8);
        let mut palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.prepare_outline_presentation(screen, palette);
        paint_detail(&mut bus, 0x1000);
        let epoch = bus.presentation_epoch();
        for _ in 0..10 {
            bus.prepare_outline_presentation(screen, palette);
            let saved = bus.save_pixel_bytes(0x1000, 2);
            bus.remember_dialog_snapshot(&saved, (0, 0, 1, 2), false);
            assert_eq!(bus.presentation_epoch(), epoch);
        }
        palette[0] = [200, 0, 0];
        bus.prepare_outline_presentation(screen, palette);
        assert_ne!(bus.presentation_epoch(), epoch);
    }

    #[test]
    fn dialog_snapshot_reuse_observes_drawing_and_snapshot_mutation() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        let saved = bus.save_pixel_bytes(0x1000, 2);
        let rect = (0, 0, 1, 2);
        bus.remember_dialog_snapshot(&saved, rect, false);
        assert!(bus.dialog_snapshot_is_current(&saved.clone(), rect, false));
        assert!(!bus.dialog_snapshot_is_current(&saved, rect, true));
        bus.remember_dialog_snapshot(&saved, rect, true);
        assert!(bus.dialog_snapshot_is_current(&saved, rect, true));
        assert!(!bus.dialog_snapshot_is_current(&saved, rect, false));
        bus.remember_dialog_snapshot(&saved, rect, false);
        assert!(!bus.dialog_snapshot_is_current(&saved, (1, 0, 2, 2), false));
        let _ = bus.save_pixel_bytes(0x1000, 2);
        bus.write_byte(0x1001, 255);
        assert!(bus.dialog_snapshot_is_current(&saved, rect, false));
        let mut changed = saved.clone();
        changed[0] = 1;
        assert!(!bus.dialog_snapshot_is_current(&changed, rect, false));
        // Even a same-byte write over a glyph discards its subpixel coverage.
        bus.write_byte(0x1000, bus.read_byte(0x1000));
        assert!(!bus.dialog_snapshot_is_current(&saved, rect, false));
        bus.restore_saved_pixels(0x1000, &saved, 0, 2);
        bus.remember_dialog_snapshot(&saved, rect, false);
        paint_detail(&mut bus, 0x1000);
        assert!(!bus.dialog_snapshot_is_current(&saved, rect, false));
    }

    #[test]
    fn equal_native_redraw_reuses_the_new_cell_identity() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        let original = bus.save_pixel_bytes(0x1000, 2);
        let replacement = Arc::new((*original.detail[&0]).clone());
        let p = bus.presentation.as_ref().unwrap();
        assert!(p.matches_detail(0x1000, Some(&replacement)));
        assert!(Arc::ptr_eq(
            p.detail_cache.borrow()[0].as_ref().unwrap(),
            &replacement
        ));
    }

    #[test]
    fn bulk_native_erase_discards_only_touched_outline_cells() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x2000);
        paint_detail(&mut bus, 0x2010);
        let before = bus.save_pixel_bytes(0x2010, 2);
        bus.presentation.write_bytes(0x2000, &[255; 16]);
        assert!(bus.save_pixel_bytes(0x2000, 2).detail.is_empty());
        assert_eq!(bus.save_pixel_bytes(0x2010, 2).detail, before.detail);
        let revision = bus.presentation_epoch();
        bus.presentation.write_bytes(0x2000, &[255; 16]);
        assert_eq!(bus.presentation_epoch(), revision);
    }

    #[test]
    fn snapshots_share_detail_until_drawing_changes_the_cell() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        let before = bus.save_pixel_bytes(0x1000, 2);
        let repeated = bus.save_pixel_bytes(0x1000, 2);
        assert!(Arc::ptr_eq(&before.detail[&0], &repeated.detail[&0]));
        bus.restore_saved_pixels(0x1000, &before, 0, 2);
        let restored = bus.save_pixel_bytes(0x1000, 2);
        assert!(Arc::ptr_eq(&before.detail[&0], &restored.detail[&0]));
        bus.write_byte(0x1000, 255);
        assert!(bus.save_pixel_bytes(0x1000, 2).detail.get(&0).is_none());
        paint_detail(&mut bus, 0x1000);
        let repainted = bus.save_pixel_bytes(0x1000, 2);
        assert!(!Arc::ptr_eq(&before.detail[&0], &repainted.detail[&0]));
        assert_eq!(before.detail[&0], repeated.detail[&0]);
    }

    #[test]
    fn shared_row_copy_retains_native_detail_in_indexed_and_direct_formats() {
        use crate::copy_bits::{BytePixmap, RowCopy, RowCopyOutcome};
        for depth in [8u32, 16, 32] {
            let mut bus = bus();
            let lanes = depth / 8;
            bus.enable_outline_presentation(
                (0x1000, 8 * lanes, 8, 8, depth as u16),
                std::array::from_fn(|i| [i as u8; 3]),
                2,
            );
            let mut memory = crate::memory::GuestAddressSpace::new();
            let source = 0x0100_0000;
            let destination = source + 32;
            memory.add_region(source, vec![255; 64]);
            bus.attach_guest_address_space(memory.shared_view());
            paint_detail(&mut bus, source);
            let mut expected = bus.save_pixel_bytes(source, (2 * lanes) as usize);
            let palette: [u8; 256] = std::array::from_fn(|i| 255 - i as u8);
            let translation = (depth == 8).then_some(&palette);
            if let Some(palette) = translation {
                expected.transform_detail(|_, byte| palette[byte as usize]);
            }
            let pixmap = |base| BytePixmap {
                base,
                row_bytes: 2 * lanes,
                depth,
                bounds: [0, 0, 1, 2],
            };
            assert_eq!(
                RowCopy {
                    mode: 0,
                    source: pixmap(source),
                    destination: pixmap(destination),
                    source_rect: [0, 0, 1, 2],
                    destination_rect: [0, 0, 1, 2],
                    clip: [0, 0, 1, 2],
                    palette: translation
                }
                .execute(&mut memory),
                RowCopyOutcome::Completed
            );
            assert_eq!(
                bus.presentation
                    .as_ref()
                    .unwrap()
                    .detail(destination)
                    .as_ref(),
                expected.detail.get(&0)
            );
            memory.write_bytes(destination, &[0]).unwrap();
            assert!(bus
                .presentation
                .as_ref()
                .unwrap()
                .detail(destination)
                .is_none());
        }
    }

    #[test]
    fn ordinary_pixels_expand_at_presentation_and_seed_new_glyphs() {
        let mut bus = bus();
        bus.write_long(0x1000, 0x20202020);
        let before = bus.outline_presentation_rgb().unwrap().2;
        assert_eq!(&before[..24], &[32; 24]);
        paint_detail(&mut bus, 0x1000);
        bus.presentation.as_mut().unwrap().glyph = Some((
            OutlineGlyph {
                pixels: vec![255],
                width: 1,
                height: 1,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        bus.write_byte(0x1001, 0);
        bus.end_outline_glyph();
        let first = bus.outline_presentation_rgb().unwrap().2;
        assert_eq!(
            &first[6..12],
            &[32; 6],
            "logical-only ink must retain its background"
        );
        assert_eq!(
            &first[..3],
            &[24; 3],
            "25% black over the latest image pixel"
        );
        bus.write_word(0x1000, 0x4040);
        let cleared = bus.outline_presentation_rgb().unwrap().2;
        assert_eq!(&cleared[..12], &[64; 12]);
        paint_detail(&mut bus, 0x1000);
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[..3], &[48; 3]);
        // Ordinary writes must not replace neighboring retained glyph samples.
        let glyph = bus.presentation.as_ref().unwrap().detail(0x1000);
        bus.write_byte(0x1001, 99);
        assert_eq!(bus.presentation.as_ref().unwrap().detail(0x1000), glyph);
    }

    #[test]
    fn bulk_memory_paths_invalidate_only_overlapping_offscreen_detail() {
        let mut bus = bus();
        for write in [
            |b: &mut MacMemoryBus| b.write_word(0x2000, 0),
            |b: &mut MacMemoryBus| b.write_long(0x1ffe, 0),
            |b: &mut MacMemoryBus| b.write_bytes(0x1fff, &[0; 4]),
            |b: &mut MacMemoryBus| b.fill_zeros(0x2000, 4),
            |b: &mut MacMemoryBus| b.fill_bytes(0x1fff, 4, 0),
            |b: &mut MacMemoryBus| {
                assert!(b.copy_ram_bytes(0x3000, 0x2000, 4));
            },
        ] {
            paint_detail(&mut bus, 0x2000);
            let p = bus.presentation.as_ref().unwrap();
            assert!(!p.observes_range(0x3000, 0x1000));
            assert!(p.observes_range(0x1fff, 2));
            assert!(!p.observes_range(0x2000, 0));
            drop(p);
            bus.fill_bytes(0x3000, 0x1000, 0);
            assert!(bus.presentation.as_ref().unwrap().detail(0x2000).is_some());
            write(&mut bus);
            assert!(bus.presentation.as_ref().unwrap().detail(0x2000).is_none());
        }
        paint_detail(&mut bus, 0x2000);
        paint_detail(&mut bus, 0x1000);
        let snapshot = bus.save_pixel_bytes(0x0fff, 0x1002);
        assert_eq!(snapshot.detail.len(), 2);
        assert!(snapshot.detail.contains_key(&1));
        assert!(snapshot.detail.contains_key(&0x1001));
    }

    #[test]
    fn screen_snapshot_ignores_detail_from_an_old_offscreen_mapping() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x2000);
        let stale = bus.presentation.as_ref().unwrap().detail(0x2000).unwrap();
        bus.presentation
            .as_mut()
            .unwrap()
            .offscreen
            .insert(0x1000, stale);
        let original = bus.outline_presentation_rgb().unwrap().2;
        let saved = bus.save_pixel_bytes(0x1000, 8);
        assert!(saved.detail.is_empty());
        bus.fill_bytes(0x1000, 8, 42);
        bus.restore_saved_pixels(0x1000, &saved, 0, 8);
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, original);
    }

    #[test]
    fn snapshots_restore_covered_ink_and_clear_ink_absent_from_snapshot() {
        let mut bus = bus();
        let blank = bus.save_pixel_bytes(0x1000, 8);
        paint_detail(&mut bus, 0x1000);
        let expected = bus.outline_presentation_rgb().unwrap().2;
        let saved = bus.save_pixel_bytes(0x1000, 8).clone();
        bus.fill_bytes(0x1000, 8, 42);
        bus.restore_saved_pixels(0x1000, &saved, 0, 8);
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, expected);
        // Restore blank pixels even where their guest byte matches a glyph's
        // empty logical cell: no stale subpixel ink may remain behind.
        bus.restore_saved_pixels(0x1000, &blank, 0, 8);
        assert!(bus.presentation.as_ref().unwrap().ink.is_empty());
        assert!(bus
            .outline_presentation_rgb()
            .unwrap()
            .2
            .iter()
            .all(|&v| v == 255));
    }

    #[test]
    fn snapshot_restore_checks_the_whole_span_and_same_byte_ink_changes() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x101d);
        let saved = bus.save_pixel_bytes(0x1000, 32);
        let expected = bus.outline_presentation_rgb().unwrap().2;
        let epoch = bus.presentation_epoch();
        bus.begin_uncapped_write_probe();
        bus.restore_saved_pixels(0x1003, &saved, 3, usize::MAX);
        assert!(bus.finish_write_probe_ranges().is_empty());
        assert_eq!(bus.presentation_epoch(), epoch);

        // An equal framebuffer byte can have lost its retained outline.
        bus.write_byte(0x101d, bus.read_byte(0x101d));
        assert_ne!(bus.outline_presentation_rgb().unwrap().2, expected);
        bus.restore_saved_pixels(0x1003, &saved, 3, usize::MAX);
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, expected);

        // A long matching prefix must not hide a changed final byte, and a
        // subrange restoration must leave the preceding bytes alone.
        bus.write_byte(0x1002, 17);
        bus.write_byte(0x101f, 42);
        bus.restore_saved_pixels(0x1003, &saved, 3, usize::MAX);
        assert_eq!(bus.read_byte(0x1002), 17);
        assert_eq!(bus.read_bytes(0x1003, 29), saved[3..32]);
        bus.write_byte(0x1002, saved[2]);
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, expected);
    }

    #[test]
    fn offscreen_round_trip_overlap_and_palette_translation_preserve_detail() {
        let mut bus = bus();
        bus.write_byte(0x2000, 255);
        paint_detail(&mut bus, 0x2000);
        assert!(bus.presentation.as_ref().unwrap().detail(0x2000).is_some());
        bus.block_move(0x2000, 0x1000, 1);
        let original = bus.presentation.as_ref().unwrap().detail(0x1000).unwrap();
        assert!(bus.copy_ram_bytes(0x1000, 0x1001, 7));
        assert_eq!(
            bus.presentation.as_ref().unwrap().detail(0x1001),
            Some(original.clone())
        );
        let table = std::array::from_fn(|i| 255 - i as u8);
        assert!(bus.copy_mapped_ram_bytes(0x1001, 0x2001, 1, &table));
        assert!(bus.copy_mapped_ram_bytes(0x2001, 0x1002, 1, &table));
        assert_eq!(
            bus.presentation.as_ref().unwrap().detail(0x1002),
            Some(original)
        );
        bus.write_byte(0x2002, 255);
        bus.presentation.as_mut().unwrap().glyph = Some((
            OutlineGlyph {
                pixels: vec![0; 4],
                width: 2,
                height: 2,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        bus.outline_glyph_pixel(0x2002, 0, 0, 0);
        bus.write_byte(0x2002, 0);
        bus.end_outline_glyph();
        bus.block_move(0x2002, 0x1003, 1);
        assert_eq!(
            &bus.outline_presentation_rgb().unwrap().2[18..24],
            &[255; 6],
            "a logical ink pixel with zero physical coverage must stay clear after copying"
        );
        // Guest overwrites must invalidate offscreen metadata, including equal bytes.
        bus.write_byte(0x2000, 0);
        assert!(bus.presentation.as_ref().unwrap().detail(0x2000).is_none());
    }

    #[test]
    fn indexed_transfers_keep_edges_and_identical_xor_clears_them() {
        let mut bus = bus();
        paint_detail(&mut bus, 0x1000);
        let expected = bus.outline_presentation_rgb().unwrap().2;
        let saved = bus.save_pixel_bytes(0x1000, 1);
        assert!(bus.transfer_saved_pixel(0x1000, &saved, 0, |_, s, d| s | d));
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, expected);
        assert!(bus.transfer_saved_pixel(0x1000, &saved, 0, |_, s, d| s ^ d));
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[..6], &[0; 6]);
        // Transparent source background reveals destination coverage.
        bus.restore_saved_pixels(0x1000, &saved, 0, 1);
        let transparent = vec![255].into();
        assert!(
            bus.transfer_saved_pixel(0x1000, &transparent, 0, |_, s, d| if s == 255 {
                d
            } else {
                s
            })
        );
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, expected);
    }

    #[test]
    fn palette_changes_preserve_coverage_and_distinct_indexes_with_equal_colors() {
        let mut bus = bus();
        let mut palette = [[0; 3]; 256];
        palette[255] = [255; 3];
        let screen = (0x1000, 8, 8, 8, 8);
        bus.prepare_outline_presentation(screen, palette);
        let mut p = bus.presentation.as_mut().unwrap();
        p.glyph = Some((
            OutlineGlyph {
                pixels: vec![128, 255, 0, 0],
                width: 4,
                height: 1,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        p.glyph_pixel(0x1000, 0, 0, 0, 255);
        p.glyph.as_mut().unwrap().0.pixels[1] = 0;
        p.glyph_pixel(0x1000, 0, 0, 2, 255);
        p.glyph = None;
        drop(p);
        // Both foreground indexes were black when drawn. Retaining only RGB
        // cannot distinguish them after the CLUT assigns different colors.
        palette[0] = [255, 0, 0];
        palette[2] = [0, 0, 255];
        palette[255] = [0, 255, 0];
        bus.prepare_outline_presentation(screen, palette);
        let (_, _, rgb, _) = bus.outline_presentation_rgb().unwrap();
        assert_eq!(&rgb[..9], &[64, 63, 128, 255, 0, 0, 0, 255, 0]);
        let original_byte = bus.read_byte(0x1000);
        bus.invert_screen_byte(0x1000);
        assert_eq!(bus.read_byte(0x1000), !original_byte);
        assert_ne!(bus.outline_presentation_rgb().unwrap().2, rgb);
        bus.invert_screen_byte(0x1000);
        assert_eq!(bus.read_byte(0x1000), original_byte);
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, rgb);
        // A same-value guest erase still discards coverage after recoloring.
        bus.write_byte(0x1000, 255);
        palette[255] = [255; 3];
        bus.prepare_outline_presentation(screen, palette);
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[..12], &[255; 12]);
    }

    #[test]
    fn default_surface_preserves_overlay_coordinates_and_invalidates_changed_modes() {
        let mut bus = bus();
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.prepare_outline_presentation((0x1000, 8, 8, 8, 8), palette);
        // The default path replaces an explicitly configured 2x surface.
        let guest = vec![0xffffffff; 64];
        let mut overlay = guest.clone();
        overlay[10] = 0xff123456;
        let (width, height, pixels) = bus.presented_argb(&guest, &overlay).unwrap();
        assert_eq!((width, height), (32, 32));
        assert_eq!(pixels[4 * 32 + 8], 0xff123456);
        assert_eq!(pixels[7 * 32 + 11], 0xff123456);
        assert_eq!(pixels[4 * 32 + 12], 0xffffffff);
        bus.prepare_outline_presentation((0x1000, 8, 8, 8, 1), palette);
        assert!(!bus.has_outline_presentation());
        bus.prepare_outline_presentation((0x1000, 8, 8, 8, 8), palette);
        assert_eq!(bus.outline_presentation_rgb().unwrap().0, 32);
    }

    #[test]
    fn clipped_coverage_blends_over_background_and_later_writes_erase_it() {
        let mut bus = bus();
        let mut p = bus.presentation.as_mut().unwrap();
        p.glyph = Some((
            OutlineGlyph {
                pixels: vec![128; 8],
                width: 4,
                height: 2,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        assert_eq!(p.glyph_bounds(), Some((0, 0, 1, 2)));
        // Only the first logical cell survives the caller's clipping region.
        p.glyph_pixel(0x1000, 0, 0, 0, 255);
        p.glyph_pixel(0x1000, 0, 0, 0, 255); // Repainting must not darken the edge.
        drop(p);
        bus.write_byte(0x1000, 0); // Guest ink must not replace the blended plane.
        bus.end_outline_glyph();
        let (_, _, rgb, _) = bus.outline_presentation_rgb().unwrap();
        assert_eq!(&rgb[0..6], &[127; 6]);
        assert_eq!(&rgb[6..12], &[255; 6]);
        assert_eq!(bus.read_byte(0x1000), 0);
        bus.write_byte(0x1000, 0); // Even a same-value later write invalidates ink.
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[0..6], &[0; 6]);
    }

    #[test]
    fn text_run_overhang_survives_adjacent_erase_but_not_a_new_run() {
        let mut bus = bus();
        bus.begin_presentation_text_run(true);
        let mut p = bus.presentation.as_mut().unwrap();
        p.glyph = Some((
            OutlineGlyph {
                pixels: vec![128; 4],
                width: 2,
                height: 2,
                left: 2,
                top: 0,
            },
            0,
            0,
        ));
        p.glyph_pixel(0x1001, 1, 0, 0, 255);
        drop(p);
        bus.end_outline_glyph();
        bus.presentation.as_mut().unwrap().erasing_text = true;
        bus.write_byte(0x1001, 255);
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[6..12], &[127; 6]);
        assert_eq!(bus.read_byte(0x1001), 255);
        bus.end_presentation_text_run();
        bus.begin_presentation_text_run(true);
        bus.write_byte(0x1001, 255);
        assert_eq!(&bus.outline_presentation_rgb().unwrap().2[6..12], &[255; 6]);
    }

    #[test]
    fn rgba_and_argb_frontends_present_identical_pixels_and_overlay_positions() {
        let mut bus = bus();
        for depth in [8, 16, 32] {
            let lanes = u32::from(depth / 8);
            bus.enable_outline_presentation((0x1000, 8 * lanes, 8, 8, depth), [[255; 3]; 256], 4);
            let guest = vec![0xffffffff; 64];
            let mut overlay = guest.clone();
            overlay[19] = 0xff12ab34;
            let rgba = |pixels: &[u32]| {
                pixels
                    .iter()
                    .flat_map(|p| {
                        let [a, r, g, b] = p.to_be_bytes();
                        [r, g, b, a]
                    })
                    .collect::<Vec<_>>()
            };
            let (w, h, argb) = bus.presented_argb(&guest, &overlay).unwrap();
            let presented = bus.presented_rgba(&rgba(&guest), &rgba(&overlay)).unwrap();
            assert_eq!(presented, (w, h, rgba(&argb)));
            assert_eq!(argb[(2 * 4 * w + 3 * 4) as usize], 0xff12ab34);
        }
    }

    #[test]
    fn direct_color_planes_decode_guest_bytes_and_preserve_owned_detail() {
        for depth in [16, 32] {
            let mut bus = bus();
            let lanes = u32::from(depth / 8);
            bus.enable_outline_presentation((0x1000, 8 * lanes, 8, 8, depth), [[0; 3]; 256], 4);
            let white = if depth == 16 {
                vec![0x7f, 0xff]
            } else {
                vec![0, 255, 255, 255]
            };
            bus.write_bytes(0x1000, &white);
            assert_eq!(&bus.outline_presentation_rgb().unwrap().2[..12], &[255; 12]);
            {
                let mut p = bus.presentation.as_mut().unwrap();
                p.glyph = Some((
                    OutlineGlyph {
                        pixels: vec![128; 16],
                        width: 4,
                        height: 4,
                        left: 0,
                        top: 0,
                    },
                    0,
                    0,
                ));
                for lane in 0..lanes {
                    p.glyph_pixel(0x1000 + lane, 0, 0, 0, white[lane as usize]);
                }
            }
            bus.write_bytes(0x1000, &vec![0; lanes as usize]);
            bus.end_outline_glyph();
            let capture = bus.outline_presentation_rgb().unwrap().2;
            assert!(capture[..12].iter().all(|&v| (126..=128).contains(&v)));
            let saved = bus.save_pixel_bytes(0x1000, lanes as usize);
            bus.write_bytes(0x1000, &vec![0; lanes as usize]);
            assert_eq!(&bus.outline_presentation_rgb().unwrap().2[..12], &[0; 12]);
            bus.restore_saved_pixels(0x1000, &saved, 0, lanes as usize);
            assert_eq!(bus.outline_presentation_rgb().unwrap().2, capture);
        }
    }

    #[test]
    fn four_times_capture_has_four_times_the_linear_resolution() {
        let mut bus = bus();
        bus.enable_outline_presentation((0x1000, 8, 8, 8, 8), [[255; 3]; 256], 4);
        let (w, h, pixels, _) = bus.outline_presentation_rgb().unwrap();
        assert_eq!((w, h, pixels.len()), (32, 32, 32 * 32 * 3));
    }

    #[test]
    fn bulk_writes_and_overlapping_copies_update_both_planes() {
        let mut bus = bus();
        bus.write_word(0x1000, 0x1020);
        bus.write_long(0x1002, 0x30405060);
        bus.write_bytes(0x1006, &[0x70, 0x80]);
        assert!(bus.copy_ram_bytes(0x1000, 0x1001, 7));
        let expected = [0x10, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70];
        assert_eq!(bus.read_bytes(0x1000, 8), expected);
        let rgb = bus.outline_presentation_rgb().unwrap().2;
        for (i, value) in expected.iter().enumerate() {
            assert_eq!(&rgb[i * 6..i * 6 + 6], &[*value; 6]);
        }
        bus.fill_bytes_strided(0x1000, 2, 4, 42);
        bus.fill_zeros(0x1008, 8);
        bus.fill_bytes(0x1010, 8, 99);
        let rgb = bus.outline_presentation_rgb().unwrap().2;
        assert_eq!(&rgb[..6], &[42; 6]);
        assert_eq!(&rgb[16 * 2 * 3..16 * 3 * 3], &[0; 48]);
        assert_eq!(&rgb[16 * 4 * 3..16 * 5 * 3], &[99; 48]);
        assert!(bus.fast_mem_window().is_none());
    }

    #[test]
    fn native_outline_capture_preserves_logical_font_metrics() {
        use crate::quickdraw::{fonts::FONT_GENEVA, text::get_glyph};
        let mut bus = bus();
        let (glyph, data) = get_glyph(FONT_GENEVA, 9, 'a').unwrap();
        let advance = glyph.advance;
        bus.begin_outline_glyph(glyph, data, 0, 7, false, None, None);
        let p = bus.presentation.as_ref().unwrap();
        let native = &p.glyph.as_ref().unwrap().0;
        assert!(native.pixels.iter().any(|&a| a > 0 && a < 255));
        assert!(native.height > i32::from(glyph.height));
        let plain_width = native.width;
        drop(p);
        bus.begin_outline_glyph(glyph, data, 0, 7, false, Some(2), Some((advance as i16, 1)));
        let p = bus.presentation.as_ref().unwrap();
        let styled = &p.glyph.as_ref().unwrap().0;
        assert!(styled.width > plain_width);
        assert!(styled.top + styled.height >= 4);
        assert!(styled.pixels.iter().any(|&a| a > 0 && a < 255));
        assert_eq!(get_glyph(FONT_GENEVA, 9, 'a').unwrap().0.advance, advance);
    }
}
