//! Indexed and direct-color outline presentation. Guest memory and text metrics stay unchanged.
//! Ordinary framebuffer writes replace enlarged pixels in drawing order; supported
//! outline glyphs retain indexed coverage through snapshots and pixel transfers.
//! Frontends consume the presentation at its physical dimensions.
mod compact;
mod controls;
mod ink;
mod offscreen;
mod resample;
mod samples;
pub use compact::{CompactPresentation, CompactPresentationCache};
use offscreen::OffscreenDetail;
use samples::{DetailSamples, TILE_SAMPLES};

use super::page_index::PageIndex;
use super::{MacMemoryBus, MemoryBus};
use crate::quickdraw::fonts::{outline, Glyph};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

const _: () = assert!(
    TILE_SAMPLES <= u16::BITS as usize,
    "ink_mask holds one bit per sample"
);

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
/// Shortest plain run `paste_plain_screen_run` compares and stores in bulk.
const PLAIN_RUN_MIN: usize = 8;

fn screen_tiles_per_row(width: u32) -> usize {
    width.div_ceil(SCREEN_TILE) as usize
}

/// Offset of the first cell a plain store must touch: one whose value differs
/// or that holds text. Scans a chunk at a time with a branch-free test.
fn first_plain_store(held: &[u16], text: &[bool], values: &[u8]) -> Option<usize> {
    const CHUNK: usize = 16;
    let needs = |((&held, &text), &value): ((&u16, &bool), &u8)| text | (held != u16::from(value));
    let chunks = held
        .chunks(CHUNK)
        .zip(text.chunks(CHUNK))
        .zip(values.chunks(CHUNK));
    for (index, ((held, text), values)) in chunks.enumerate() {
        let cells = || held.iter().zip(text).zip(values);
        if cells().fold(false, |any, cell| any | needs(cell)) {
            return cells().position(needs).map(|offset| index * CHUNK + offset);
        }
    }
    None
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
    /// The presentation half of a span copy (see
    /// `MacMemoryBus::copy_detail_spans`): copy `spans` (source,
    /// destination, length), whose source bytes are `pixels` laid end to
    /// end, through `palette`, exactly as a per-pixel copy would. `store`
    /// writes a destination span's bytes to guest memory; the caller has
    /// proved every destination writable and that no diagnostic observes
    /// individual stores. Declines, changing nothing, unless every span
    /// qualifies for `can_copy_detail_row`.
    pub(crate) fn copy_detail_spans(
        &self,
        spans: &[(u32, u32, usize)],
        pixels: &[u8],
        palette: Option<&[u8; 256]>,
        mut store: impl FnMut(u32, &[u8]),
    ) -> bool {
        let total: usize = spans.iter().map(|&(_, _, len)| len).sum();
        if spans.is_empty() || total != pixels.len() || spans.iter().any(|&(_, _, len)| len == 0) {
            return false;
        }
        let Some(mut p) = self.as_mut() else {
            return false;
        };
        if !spans
            .iter()
            .all(|&(source, destination, len)| p.can_copy_detail_row(source, destination, len))
        {
            return false;
        }
        let mut copied = std::mem::take(&mut p.copied);
        p.capture_copied_detail(
            spans.iter().map(|&(source, _, len)| (source, len)),
            palette,
            &mut copied,
        );
        p.paste_copied_spans(
            spans
                .iter()
                .map(|&(_, destination, len)| (destination, len)),
            pixels,
            palette,
            &mut copied,
            &mut store,
        );
        p.copied = copied;
        true
    }

    /// `copy_detail_spans` from a saved snapshot instead of live memory:
    /// copy the snapshot's `spans` (destination, offset in `saved`, length)
    /// exactly as `copy_saved_pixel` through `palette` would per pixel, in
    /// order.
    pub(crate) fn copy_saved_spans(
        &self,
        spans: &[(u32, usize, usize)],
        saved: &SavedPixels,
        palette: Option<&[u8; 256]>,
        mut store: impl FnMut(u32, &[u8]),
    ) -> bool {
        if spans.is_empty()
            || spans
                .iter()
                .any(|&(_, offset, len)| len == 0 || offset + len > saved.len())
        {
            return false;
        }
        let Some(mut p) = self.as_mut() else {
            return false;
        };
        if !spans
            .iter()
            .all(|&(destination, _, len)| p.can_paste_copied_row(destination, len))
        {
            return false;
        }
        let mut copied = std::mem::take(&mut p.copied);
        copied.capture_saved(
            spans.iter().map(|&(_, offset, len)| (offset, len)),
            saved,
            palette,
        );
        let mut first = 0;
        let fits = spans.iter().all(|&(destination, _, len)| {
            let from = copied
                .cells
                .partition_point(|cell| (cell.offset as usize) < first);
            let to = copied
                .cells
                .partition_point(|cell| (cell.offset as usize) < first + len);
            first += len;
            p.copied_cells_fit(destination, len, &copied.cells[from..to])
        });
        if fits {
            let mut values = Vec::with_capacity(first);
            for &(_, offset, len) in spans {
                values.extend_from_slice(&saved[offset..offset + len]);
            }
            p.paste_copied_spans(
                spans
                    .iter()
                    .map(|&(destination, _, len)| (destination, len)),
                &values,
                palette,
                &mut copied,
                &mut store,
            );
        }
        copied.cells.clear();
        copied.inks.clear();
        p.copied = copied;
        fits
    }

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
                if p.offscreen.remove_range(address, end) {
                    p.changed(false);
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
    pub pixels: std::sync::Arc<[u8]>,
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

/// A snapshot cell copied with `map` applied to its value, indices and ink.
/// Most copies map nothing (no colour translation, same value): those share
/// the snapshot's cell instead of cloning an identical one.
fn mapped_detail_cell(
    cell: &Arc<DetailCell>,
    value: u8,
    map: &mut impl FnMut(u8) -> u8,
) -> Arc<DetailCell> {
    let unchanged = cell.value == value
        && cell.indices.iter().all(|&index| map(index) == index)
        && cell.ink.values().all(|ink| {
            map(ink.foreground) == ink.foreground && ink.background.fixed_under(&mut *map)
        });
    if unchanged {
        return cell.clone();
    }
    let mut cell = cell.clone();
    let mapped = Arc::make_mut(&mut cell);
    mapped.value = value;
    for index in &mut mapped.indices {
        *index = map(*index);
    }
    for ink in mapped.ink.values_mut() {
        ink.foreground = map(ink.foreground);
        ink.background.map(&mut *map);
    }
    cell
}

/// Mapped cells for one CopyBits colour table, keyed by source content and
/// mapped value. A mapped cell is a pure function of those, so sharing it is
/// exact. Keys are owned copies rather than the snapshot's `Arc`s, so the
/// cache never makes a live offscreen cell look shared (which would make the
/// next glyph drawn into it clone the cell).
#[derive(Default)]
pub(crate) struct CopyMapCache {
    table: Option<Box<[u8; 256]>>,
    cells: HashMap<u64, Vec<(DetailCell, u8, Arc<DetailCell>)>>,
    len: usize,
}

/// Entries kept before the cache starts over.
const COPY_MAP_CACHE_LIMIT: usize = 4096;

impl CopyMapCache {
    fn mapped(&mut self, cell: &Arc<DetailCell>, value: u8, table: &[u8; 256]) -> Arc<DetailCell> {
        if self.table.as_deref() != Some(table) {
            self.table = Some(Box::new(*table));
            self.cells.clear();
            self.len = 0;
        }
        let key = detail_fingerprint(cell, value);
        if let Some(entry) = self.cells.get(&key).and_then(|bucket| {
            bucket
                .iter()
                .find(|(source, mapped_value, _)| *mapped_value == value && source == &**cell)
        }) {
            return entry.2.clone();
        }
        let mapped = mapped_detail_cell(cell, value, &mut |index| table[index as usize]);
        if self.len >= COPY_MAP_CACHE_LIMIT {
            self.cells.clear();
            self.len = 0;
        }
        self.cells
            .entry(key)
            .or_default()
            .push(((**cell).clone(), value, mapped.clone()));
        self.len += 1;
        mapped
    }
}

/// A content hash of a cell and the value it is copied as.
fn detail_fingerprint(cell: &DetailCell, value: u8) -> u64 {
    fn mix(hash: u64, word: u64) -> u64 {
        (hash ^ word)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .rotate_left(29)
    }
    fn color(hash: u64, indexed: &IndexedColor) -> u64 {
        match indexed {
            IndexedColor::Solid(index) => mix(hash, u64::from(*index)),
            IndexedColor::Mix(a, b, alpha) => {
                color(color(mix(hash, 0x100 | u64::from(*alpha)), a), b)
            }
        }
    }
    let mut hash = mix(u64::from(value), u64::from(cell.value));
    for (i, &index) in cell.indices.iter().enumerate() {
        hash = mix(hash, u64::from(index));
        if let Some(ink) = cell.ink.get(&i) {
            hash = mix(
                hash,
                (i as u64) << 40 | u64::from(ink.foreground) << 32 | u64::from(ink.alpha),
            );
            hash = color(hash, &ink.background);
        }
    }
    hash
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
        // A narrow screen draw may refresh one byte from hundreds of rows of
        // a dialog snapshot. HashMap::retain scans its entire capacity each
        // time, including buckets left allocated after earlier text changed.
        // Remove only touched keys when the range is smaller than that scan.
        if values.len() < self.detail.capacity() {
            for key in offset..end {
                self.detail.remove(&key);
            }
        } else {
            self.detail.retain(|i, _| *i < offset || *i >= end);
        }
        self.values[offset..end].clone_from_slice(values);
    }
}

// These tables use only host-generated offsets into bounded presentation
// surfaces, pixel snapshots, or a cell's subpixel samples. They do not hash guest
// strings or arbitrary resource keys. Mix both the low bucket bits and high tag
// bits without paying SipHash's per-sample cost during painting and restoration.
#[derive(Default)]
struct SampleOffsetHasher(u64);

/// Where a row span keeps its retained detail (see `detail_row_side`).
#[derive(Clone, Copy)]
enum DetailRowSide {
    /// One screen row, from this cell.
    Screen(usize),
    Offscreen,
}

/// One source cell of a row copy, already mapped through the copy's table.
pub(crate) struct CopiedCell {
    /// Byte offset from the start of the copy's first source row.
    offset: u32,
    len: u8,
    indices: [u8; TILE_SAMPLES],
    /// How many of `CopiedDetail::inks`, in order, belong to this cell:
    /// only a cell with blended (side-table) ink lists its ink there.
    inks: u8,
    /// The cell's ink, mapped, when none of it is blended.
    block: ink::CellInk,
}

/// The retained detail a row copy carries, reused between copies.
#[derive(Default)]
pub(crate) struct CopiedDetail {
    cells: Vec<CopiedCell>,
    inks: Vec<(u8, Ink)>,
}

impl CopiedDetail {
    /// Take the cells of `spans` (offset, length) of a saved snapshot, laid
    /// end to end, mapped through `table`, as `capture_copied_detail` takes
    /// them from live memory.
    fn capture_saved<T>(
        &mut self,
        spans: impl Iterator<Item = (usize, usize)>,
        saved: &SavedPixels<T>,
        table: Option<&[u8; 256]>,
    ) {
        self.cells.clear();
        self.inks.clear();
        let map = |index: u8| table.map_or(index, |table| table[index as usize]);
        let mut keys: Vec<usize> = saved.detail.keys().copied().collect();
        keys.sort_unstable();
        let mut base = 0;
        for (offset, len) in spans {
            let first = keys.partition_point(|&key| key < offset);
            for &key in keys[first..].iter().take_while(|&&key| key < offset + len) {
                let cell = &saved.detail[&key];
                let mut indices = [0u8; TILE_SAMPLES];
                for (index, &source) in indices.iter_mut().zip(&cell.indices) {
                    *index = map(source);
                }
                let first_ink = self.inks.len();
                let mut ink: Vec<(usize, &Ink)> = cell
                    .ink
                    .iter()
                    .filter(|(&sample, _)| sample < cell.indices.len())
                    .map(|(&sample, ink)| (sample, ink))
                    .collect();
                ink.sort_unstable_by_key(|&(sample, _)| sample);
                for (sample, ink) in ink {
                    let mut ink = ink.clone();
                    ink.foreground = map(ink.foreground);
                    ink.background.map(&mut |index| map(index));
                    self.inks.push((sample as u8, ink));
                }
                self.cells.push(CopiedCell {
                    offset: (base + key - offset) as u32,
                    len: cell.indices.len().min(TILE_SAMPLES) as u8,
                    indices,
                    inks: (self.inks.len() - first_ink) as u8,
                    block: ink::CellInk::default(),
                });
            }
            base += len;
        }
    }
}

/// Offscreen `(address, sample)` pairs an opaque text run has inked.
type OffscreenRunInk = HashSet<(u32, usize), BuildHasherDefault<SampleOffsetHasher>>;

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
    offscreen: OffscreenDetail,
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
    /// Bit `i` of `ink_mask[cell]` is set exactly when the cell's tile holds
    /// ink for sample `i` (screen ink lives with the tile, in `samples`).
    /// Overwriting text visits the ink list only for cells that have ink.
    ink_mask: Vec<u16>,
    run_ink: HashSet<usize, BuildHasherDefault<SampleOffsetHasher>>,
    offscreen_run_ink: OffscreenRunInk,
    in_text_run: bool,
    pub erasing_text: bool,
    glyph: Option<(OutlineGlyph, i16, i16)>,
    pub glyph_count: usize,
    /// The address of a pending store whose presentation update the
    /// following `put_detail` supersedes (see `detail_supersedes_store`);
    /// its `write` does nothing.
    superseded_write: Option<u32>,
    /// Scratch for row copies (`MacMemoryBus::copy_detail_rows`).
    copied: CopiedDetail,
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

    /// `touch_screen` for every cell from `x_first` to `x_last` of row `y`,
    /// with one epoch for the span.
    fn touch_screen_span(&mut self, x_first: u32, x_last: u32, y: u32) {
        self.screen_epoch += 1;
        let row = y as usize * screen_tiles_per_row(self.width);
        let tiles = row + (x_first / SCREEN_TILE) as usize..=row + (x_last / SCREEN_TILE) as usize;
        if let Some(epochs) = self.tile_epochs.get_mut(tiles) {
            epochs.fill(self.screen_epoch);
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
        let (t0, t1) = (
            (x0 / SCREEN_TILE) as usize,
            ((x1 - 1) / SCREEN_TILE) as usize,
        );
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
            self.store_filter_new_pages
                .push(address >> STORE_FILTER_PAGE_SHIFT);
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
            && self.offscreen.any_in(address, end)
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
                self.offscreen.get(address)
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
        let len = (self.scale * self.scale) as usize;
        cell.indices.extend_from_slice(&samples.indices[..len]);
        for (sample, ink) in self.samples.ink(index).iter() {
            if sample < len {
                cell.ink.insert(sample, ink);
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
            for (key, cell) in self.offscreen.range(from, u64::from(to)) {
                pixels
                    .detail
                    .insert(offset + (key - address) as usize, cell);
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
            for (key, cell) in self.offscreen.range(first, u64::from(last) + 1) {
                let index = offset + (key - address) as usize;
                let value = pixels[index].into() as u8;
                if pixels
                    .detail
                    .get(&index)
                    .filter(|saved| saved.value == value)
                    != Some(&cell)
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
            return self.offscreen.matches(address, detail.map(|cell| &**cell));
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
        // A cached cell is exactly this cell's current state (`detail`
        // returns it as such), so comparing with it settles the question
        // either way. Snapshots taken each frame hold equal cells with new
        // identities, which would otherwise miss the pointer check and
        // compare every sample against the shared ink map.
        let cached = match self.detail_cache.borrow()[(y * self.width + x) as usize].as_ref() {
            Some(cached) if Arc::ptr_eq(cached, cell) => return true,
            Some(cached) => Some(**cached == **cell),
            None => None,
        };
        match cached {
            Some(false) => return false,
            Some(true) => {
                self.detail_cache.borrow_mut()[(y * self.width + x) as usize] = Some(cell.clone());
                return true;
            }
            None => {}
        }
        let index = (y * self.width + x) as usize;
        let samples = self.samples.get(index);
        let ink = self.samples.ink(index);
        for i in 0..(self.scale * self.scale) as usize {
            if samples.indices[i] != cell.indices[i] || ink.get(i).as_ref() != cell.ink.get(&i) {
                return false;
            }
        }
        // A redraw can create an equal cell with a new identity. Remember it
        // after comparing once, so idle frames do not repeat every ink lookup.
        self.detail_cache.borrow_mut()[(y * self.width + x) as usize] = Some(cell.clone());
        true
    }

    /// Whether a store of `cell.value` at `address`, followed by
    /// `put_detail(address, cell)`, may skip the store's presentation update.
    /// Outside text drawing, glyph capture and CPU recolor tracking, that
    /// update only clears the destination's text, and `put_detail` then
    /// replaces every part of the cell anyway: its value, text flag, samples,
    /// ink and cached detail (or the whole offscreen entry).
    fn detail_supersedes_store(&self, address: u32, cell: &Arc<DetailCell>) -> bool {
        !self.cpu_drawing
            && self.glyph.is_none()
            && !self.erasing_text
            && self.run_ink.is_empty()
            && (self.position(address).is_none()
                || cell.indices.len() == (self.scale * self.scale) as usize)
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
            self.offscreen.insert(address, cell);
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
        let (samples, mut ink) = self.samples.ensure_with_ink(index);
        Self::set_text_cell(&mut self.text_cells, &mut self.text_cell_count, index, true);
        self.detail_cache.get_mut()[index] = Some(cell.clone());
        self.guest_values[index] = cell.value.into();
        let len = (self.scale * self.scale) as usize;
        ink.clear();
        let mut mask = 0u16;
        for i in 0..len {
            samples.indices[i] = cell.indices[i];
            samples.rgb[i] = if let Some(held) = cell.ink.get(&i) {
                ink.set(i, held.clone());
                mask |= 1 << i;
                held.rgb(palette)
            } else {
                palette[cell.indices[i] as usize]
            };
        }
        self.ink_mask[index] = mask;
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
        for (cell, mask) in expected.iter_mut().enumerate() {
            *mask = self.samples.ink(cell).mask();
        }
        expected == self.ink_mask
    }

    /// The offscreen half of `write`, out of line: every screen byte a game
    /// draws passes through `write`, and few stores reach retained offscreen
    /// text.
    #[inline(never)]
    fn write_offscreen(&mut self, address: u32, value: u8) {
        if self.offscreen.contains(address) || self.glyph.is_some() {
            self.changed(false);
        }
        if self.glyph.is_some() {
            if let Some(mut cell) = self.offscreen.cell_mut(address) {
                cell.set_value(value);
            }
        } else if self.erasing_text
            && self
                .offscreen_run_ink
                .iter()
                .any(|(addr, _)| *addr == address)
        {
            if let Some(mut cell) = self.offscreen.cell_mut(address) {
                cell.set_value(value);
                for i in 0..cell.len() {
                    if !self.offscreen_run_ink.contains(&(address, i)) {
                        cell.set_index(i, value);
                        cell.remove_ink(i);
                    }
                }
            }
        } else {
            self.offscreen.remove(address);
        }
    }

    /// Invalidate the detail under a plain store of `len` bytes at
    /// `address`, exactly as `len` calls to `write` would, when the span lies
    /// wholly offscreen and no glyph or text erase is in progress, and
    /// report whether it did. Plain offscreen stores only drop cells, so a
    /// span erase drops them in one pass.
    pub(super) fn write_offscreen_span(&mut self, address: u32, len: usize) -> bool {
        let end = u64::from(address) + len as u64;
        let screen_end = u64::from(self.base) + u64::from(self.row_bytes) * u64::from(self.height);
        if len == 0
            || self.glyph.is_some()
            || self.erasing_text
            || !(end <= u64::from(self.base) || u64::from(address) >= screen_end)
        {
            return false;
        }
        // The first `write` consumes a superseded store, and skips its byte
        // if it lies in the span.
        let skipped = self
            .superseded_write
            .take()
            .filter(|&skip| skip >= address && u64::from(skip) < end);
        if !self.may_have_offscreen_detail(address, end) {
            return true;
        }
        let removed = match skipped {
            Some(skip) => {
                let before = self.offscreen.remove_range(address, u64::from(skip));
                self.offscreen.remove_range(skip + 1, end) | before
            }
            None => self.offscreen.remove_range(address, end),
        };
        if removed {
            self.changed(false);
        }
        true
    }

    pub fn write(&mut self, address: u32, value: u8) {
        if self.superseded_write.take() == Some(address) {
            return;
        }
        let Some((x, y)) = self.position(address) else {
            if self.glyph.is_none()
                && !self.may_have_offscreen_detail(address, u64::from(address) + 1)
            {
                return;
            }
            self.write_offscreen(address, value);
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
        self.clear_text_cell(cell, x, value);
    }

    /// Replace the text at screen cell `cell` (column `x`) with plain
    /// `value`: the text half of `write`, shared with
    /// `sync_screen_row_over_text`.
    fn clear_text_cell(&mut self, cell: usize, x: u32, value: u8) {
        self.detail_cache.get_mut()[cell] = None;
        Self::set_text_cell(&mut self.text_cells, &mut self.text_cell_count, cell, false);
        let color = self.palette_at(x)[value as usize];
        let (samples, mut ink) = self.samples.get_mut_with_ink(cell);
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
                ink.remove(i);
                self.ink_mask[cell] &= !bit;
            }
            samples.indices[i] = value;
            samples.rgb[i] = color;
        }
        if !self.text_cells[cell] {
            self.samples.release(cell);
        }
    }

    /// Whether `sync_screen_row_over_text` may stand in for one `write` per
    /// byte of a single screen-row span: no glyph capture, CPU drawing or
    /// recolor tracking, text erasing or text run in progress, each of which
    /// gives `write` per-byte behaviour.
    pub(crate) fn can_sync_screen_row_over_text(&self, address: u32, len: usize) -> bool {
        !self.cpu_drawing
            && self.cpu_recolor.is_none()
            && self.glyph.is_none()
            && !self.erasing_text
            && self.run_ink.is_empty()
            && self.screen_row_span(address, len).is_some()
    }

    /// The first cell of a span within one screen row, if it is one.
    fn screen_row_span(&self, address: u32, len: usize) -> Option<usize> {
        let last = u32::try_from(len.checked_sub(1)?)
            .ok()
            .and_then(|n| address.checked_add(n))?;
        let ((x, y), (_, last_y)) = (self.position(address)?, self.position(last)?);
        let end_x = x.checked_add(u32::try_from(len).ok()?)?;
        (y == last_y && end_x <= self.width).then_some((y * self.width + x) as usize)
    }

    /// Apply stored `bytes` at `address` exactly as one `write` per byte
    /// would, for a span `can_sync_screen_row_over_text` admits: unchanged
    /// plain cells stay put, changed plain cells take their value, and text
    /// cells are cleared to plain.
    pub(crate) fn sync_screen_row_over_text(&mut self, address: u32, bytes: &[u8]) {
        let Some((x, y)) = self.position(address) else {
            return;
        };
        let start = (y * self.width + x) as usize;
        for (i, &value) in bytes.iter().enumerate() {
            let cell = start + i;
            let cell_x = x + i as u32;
            if self.guest_values[cell] == u16::from(value) && !self.text_cells[cell] {
                continue;
            }
            self.touch_screen(cell_x, y);
            self.changed(true);
            self.guest_values[cell] = u16::from(value);
            if self.text_cells[cell] {
                self.clear_text_cell(cell, cell_x, value);
            }
        }
    }

    /// Where a span of `len` bytes at `address` keeps its retained detail,
    /// for row copies: within one screen row, or wholly off the screen.
    fn detail_row_side(&self, address: u32, len: usize) -> Option<DetailRowSide> {
        if let Some(first) = self.screen_row_span(address, len) {
            return Some(DetailRowSide::Screen(first));
        }
        let end = u64::from(address) + len as u64;
        let screen_end = u64::from(self.base) + u64::from(self.row_bytes) * u64::from(self.height);
        (end <= u64::from(self.base) || u64::from(address) >= screen_end)
            .then_some(DetailRowSide::Offscreen)
    }

    /// Whether `capture_copied_detail` and `paste_copied_row` may stand in
    /// for copying the `len` bytes at `source` to `destination` a pixel at a
    /// time: each side is one screen-row span or lies wholly off the screen,
    /// a screen destination receives only full-size cells, and no glyph
    /// capture, CPU drawing or recolor tracking, text erasing or text run is
    /// in progress (each gives stores per-byte behaviour).
    pub(crate) fn can_copy_detail_row(&self, source: u32, destination: u32, len: usize) -> bool {
        if !self.can_paste_copied_row(destination, len) {
            return false;
        }
        let Some(source_side) = self.detail_row_side(source, len) else {
            return false;
        };
        match (self.detail_row_side(destination, len), source_side) {
            (Some(DetailRowSide::Screen(_)), DetailRowSide::Offscreen) => {
                let samples = (self.scale * self.scale) as usize;
                self.offscreen
                    .all_cells_have_len(source, u64::from(source) + len as u64, samples)
            }
            (Some(_), _) => true,
            (None, _) => false,
        }
    }

    /// Whether `paste_copied_row` may stand in for storing `len` copied
    /// bytes at `destination` a pixel at a time: the span is one screen-row
    /// span or lies wholly off the screen, and no glyph capture, CPU drawing
    /// or recolor tracking, text erasing or text run is in progress. A
    /// screen destination also needs full-size cells.
    fn can_paste_copied_row(&self, destination: u32, len: usize) -> bool {
        !self.cpu_drawing
            && self.cpu_recolor.is_none()
            && self.glyph.is_none()
            && !self.erasing_text
            && self.run_ink.is_empty()
            && self.detail_row_side(destination, len).is_some()
    }

    /// Whether every copied cell for a span of `len` bytes to `destination`
    /// fits it: a screen destination takes only full-size cells.
    pub(crate) fn copied_cells_fit(
        &self,
        destination: u32,
        len: usize,
        cells: &[CopiedCell],
    ) -> bool {
        let samples = self.scale * self.scale;
        !matches!(
            self.detail_row_side(destination, len),
            Some(DetailRowSide::Screen(_))
        ) || cells.iter().all(|cell| u32::from(cell.len) == samples)
    }

    /// Snapshot the retained detail of a row copy's source spans (address,
    /// length), laid end to end, into `copied`, mapped through `table` as the copy maps
    /// the bytes. Taking every row first keeps overlapping copies (a scroll)
    /// reading the source as it was.
    pub(crate) fn capture_copied_detail(
        &self,
        sources: impl Iterator<Item = (u32, usize)>,
        table: Option<&[u8; 256]>,
        copied: &mut CopiedDetail,
    ) {
        copied.cells.clear();
        copied.inks.clear();
        let map = |index: u8| table.map_or(index, |table| table[index as usize]);
        let push =
            |copied: &mut CopiedDetail, offset: usize, indices: &[u8], ink: ink::InkView<'_>| {
                let mut mapped = [0u8; TILE_SAMPLES];
                match table {
                    Some(table) => {
                        for (index, &source) in mapped.iter_mut().zip(indices) {
                            *index = table[source as usize];
                        }
                    }
                    None => mapped[..indices.len()].copy_from_slice(indices),
                }
                let block = ink.block().within(indices.len());
                let first = copied.inks.len();
                let block = if block.has_complex() {
                    // Blended ink keeps its full form, in the list.
                    for (sample, mut ink) in ink.iter() {
                        if sample < indices.len() {
                            ink.foreground = map(ink.foreground);
                            ink.background.map(&mut |index| map(index));
                            copied.inks.push((sample as u8, ink));
                        }
                    }
                    ink::CellInk::default()
                } else {
                    block.mapped(table)
                };
                copied.cells.push(CopiedCell {
                    offset: offset as u32,
                    len: indices.len() as u8,
                    indices: mapped,
                    inks: (copied.inks.len() - first) as u8,
                    block,
                });
            };
        let samples = (self.scale * self.scale) as usize;
        let mut base = 0;
        for (source, row_len) in sources {
            match self.detail_row_side(source, row_len) {
                Some(DetailRowSide::Screen(first)) => {
                    for i in 0..row_len {
                        let cell = first + i;
                        if self.text_cells[cell] {
                            push(
                                copied,
                                base + i,
                                &self.samples.get(cell).indices[..samples],
                                self.samples.ink(cell),
                            );
                        }
                    }
                }
                Some(DetailRowSide::Offscreen) => {
                    self.offscreen.visit_cells(source, row_len, |i, cell| {
                        push(copied, base + i, cell.indices(), cell.ink());
                    });
                }
                None => {}
            }
            base += row_len;
        }
    }

    /// Finish one destination row of a row copy exactly as the per-pixel
    /// copy would, after its bytes `values` are stored: a byte with a copied
    /// cell (`cells`, offsets counted from `first`, their ink in order in
    /// `inks`) takes that cell as `put_detail` would, leaving an identical
    /// destination untouched; any other byte is a plain store, as `write`
    /// would make it. The cells' ink is moved out of `inks`.
    /// Store and paste each destination span of a copy in order: its bytes
    /// (`pixels` laid end to end, mapped through `palette`) go to guest
    /// memory through `store`, then its cells from `copied` are pasted.
    /// Leaves `copied` empty.
    fn paste_copied_spans(
        &mut self,
        spans: impl Iterator<Item = (u32, usize)>,
        pixels: &[u8],
        palette: Option<&[u8; 256]>,
        copied: &mut CopiedDetail,
        store: &mut impl FnMut(u32, &[u8]),
    ) {
        let mut row = Vec::new();
        let (mut first, mut next_cell, mut next_ink) = (0, 0, 0);
        for (destination, len) in spans {
            row.clear();
            row.extend_from_slice(&pixels[first..first + len]);
            if let Some(palette) = palette {
                for pixel in &mut row {
                    *pixel = palette[*pixel as usize];
                }
            }
            store(destination, &row);
            let end = (first + len) as u32;
            let cell_end =
                next_cell + copied.cells[next_cell..].partition_point(|cell| cell.offset < end);
            let ink_end = next_ink
                + copied.cells[next_cell..cell_end]
                    .iter()
                    .map(|cell| usize::from(cell.inks))
                    .sum::<usize>();
            self.paste_copied_row(
                destination,
                &row,
                first,
                &copied.cells[next_cell..cell_end],
                &mut copied.inks[next_ink..ink_end],
            );
            (first, next_cell, next_ink) = (first + len, cell_end, ink_end);
        }
        copied.cells.clear();
        copied.inks.clear();
    }

    fn paste_copied_row(
        &mut self,
        destination: u32,
        values: &[u8],
        first: usize,
        cells: &[CopiedCell],
        inks: &mut [(u8, Ink)],
    ) {
        match self.detail_row_side(destination, values.len()) {
            Some(DetailRowSide::Screen(_)) => {
                self.paste_screen_row(destination, values, first, cells, inks)
            }
            Some(DetailRowSide::Offscreen) => {
                self.paste_offscreen_row(destination, values, first, cells, inks)
            }
            None => {}
        }
    }

    fn paste_screen_row(
        &mut self,
        destination: u32,
        values: &[u8],
        first: usize,
        cells: &[CopiedCell],
        inks: &mut [(u8, Ink)],
    ) {
        let Some((x0, y)) = self.position(destination) else {
            return;
        };
        let samples = (self.scale * self.scale) as usize;
        let mut cells = cells.iter().peekable();
        let mut inks = inks;
        let mut i = 0;
        while i < values.len() {
            // The bytes up to the next copied cell carry no text: store them
            // a run at a time.
            let run_end = cells.peek().map_or(values.len(), |copied| {
                (copied.offset as usize)
                    .saturating_sub(first)
                    .clamp(i, values.len())
            });
            if i < run_end {
                self.paste_plain_screen_run(x0 + i as u32, y, &values[i..run_end]);
                i = run_end;
                continue;
            }
            let value = values[i];
            let x = x0 + i as u32;
            let cell = (y * self.width + x) as usize;
            let Some(copied) = cells.next_if(|copied| copied.offset as usize == first + i) else {
                self.paste_plain_screen_run(x, y, &values[i..=i]);
                i += 1;
                continue;
            };
            i += 1;
            let (ink, rest) = std::mem::take(&mut inks).split_at_mut(usize::from(copied.inks));
            inks = rest;
            debug_assert_eq!(usize::from(copied.len), samples);
            // Unblended ink travels as a packed block; blended ink as a list.
            let listed = !ink.is_empty();
            let mut mask = copied.block.mask();
            for (sample, _) in ink.iter() {
                mask |= 1 << sample;
            }
            // Unchanged when the value, indices and ink already match, as
            // `put_detail`'s comparison would find.
            if self.text_cells[cell]
                && self.guest_values[cell] == u16::from(value)
                && self.ink_mask[cell] == mask
                && self.samples.get(cell).indices[..samples] == copied.indices[..samples]
                && if listed {
                    self.samples.ink(cell).eq_list(ink)
                } else {
                    self.samples.ink(cell).block().same_as(&copied.block)
                }
            {
                continue;
            }
            self.changed(true);
            self.touch_screen(x, y);
            let palette = if self.depth == 8 {
                &self.palette
            } else {
                &self.direct_palettes[(x % self.bytes_per_pixel()) as usize]
            };
            let (tile, mut held) = self.samples.ensure_with_ink(cell);
            Self::set_text_cell(&mut self.text_cells, &mut self.text_cell_count, cell, true);
            self.detail_cache.get_mut()[cell] = None;
            self.guest_values[cell] = u16::from(value);
            tile.indices[..samples].copy_from_slice(&copied.indices[..samples]);
            for (rgb, &index) in tile.rgb[..samples]
                .iter_mut()
                .zip(&copied.indices[..samples])
            {
                *rgb = palette[index as usize];
            }
            if listed {
                held.clear();
                for (sample, ink) in ink.iter_mut() {
                    tile.rgb[usize::from(*sample)] = ink.rgb(palette);
                    held.set(usize::from(*sample), offscreen::take_ink(ink));
                }
            } else {
                held.assign(&copied.block);
                for sample in copied.block.samples() {
                    tile.rgb[sample] = copied.block.rgb(sample, palette);
                }
            }
            self.ink_mask[cell] = mask;
        }
    }

    /// Store plain bytes onto one screen row exactly as a store per byte
    /// would: an unchanged plain cell stays put, a changed one takes its value,
    /// and a text cell is cleared to plain. One pass finds the first byte with
    /// work to do, so a run over an unchanged destination ends there. From that
    /// byte on, a long run without text cells, the usual case for sprites and
    /// chrome, is recorded with one change and one touch of the tiles between
    /// its first and last changed byte (a tile in between counts as changed,
    /// which only makes an unchanged-since check more conservative).
    fn paste_plain_screen_run(&mut self, x0: u32, y: u32, values: &[u8]) {
        let start = (y * self.width + x0) as usize;
        let end = start + values.len();
        // Short runs (the gaps between glyph cells in a line of text) cost
        // less a byte at a time than the run setup below.
        let first = if values.len() < PLAIN_RUN_MIN {
            0
        } else {
            let held = &self.guest_values[start..end];
            let Some(first) = first_plain_store(held, &self.text_cells[start..end], values) else {
                return;
            };
            first
        };
        let values = &values[first..];
        let (x0, start) = (x0 + first as u32, start + first);
        if values.len() < PLAIN_RUN_MIN || self.text_cells[start..end].iter().any(|&text| text) {
            for (i, &value) in values.iter().enumerate() {
                let cell = start + i;
                if self.guest_values[cell] == u16::from(value) && !self.text_cells[cell] {
                    continue;
                }
                let x = x0 + i as u32;
                self.touch_screen(x, y);
                self.changed(true);
                self.guest_values[cell] = u16::from(value);
                if self.text_cells[cell] {
                    self.clear_text_cell(cell, x, value);
                }
            }
            return;
        }
        // No text from here on, and the first byte differs.
        let held = &self.guest_values[start..end];
        let differs = |(&held, &value): (&u16, &u8)| held != u16::from(value);
        let last = values.len() - 1 - held.iter().zip(values).rev().position(differs).unwrap_or(0);
        self.changed(true);
        self.touch_screen_span(x0, x0 + last as u32, y);
        for (held, &value) in self.guest_values[start..=start + last]
            .iter_mut()
            .zip(&values[..=last])
        {
            *held = u16::from(value);
        }
    }

    fn paste_offscreen_row(
        &mut self,
        destination: u32,
        values: &[u8],
        first: usize,
        cells: &[CopiedCell],
        inks: &mut [(u8, Ink)],
    ) {
        let end = u64::from(destination) + values.len() as u64;
        let mut changed = false;
        // Plain bytes only drop the cells under them, a run at a time.
        let mut plain_from = destination;
        let mut inks = inks;
        for copied in cells {
            let address = destination + (copied.offset as usize - first) as u32;
            if plain_from < address
                && self.may_have_offscreen_detail(plain_from, u64::from(address))
            {
                changed |= self.offscreen.remove_range(plain_from, u64::from(address));
            }
            plain_from = address + 1;
            let (ink, rest) = std::mem::take(&mut inks).split_at_mut(usize::from(copied.inks));
            inks = rest;
            let value = values[copied.offset as usize - first];
            let indices = &copied.indices[..usize::from(copied.len)];
            let stored = if ink.is_empty() {
                self.offscreen
                    .store_block(address, value, indices, &copied.block)
            } else {
                self.offscreen.store_parts(address, value, indices, ink)
            };
            if stored {
                changed = true;
                self.include_offscreen_address(address);
            }
        }
        if u64::from(plain_from) < end && self.may_have_offscreen_detail(plain_from, end) {
            changed |= self.offscreen.remove_range(plain_from, end);
        }
        if changed {
            self.changed(false);
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
            let mut cell = self.offscreen.cell_mut_or_insert(
                address,
                background,
                (self.scale * self.scale) as usize,
            );
            paint_offscreen_glyph_cell(
                &mut cell,
                (glyph, *h, *v),
                self.scale,
                (x, y, foreground),
                (self.in_text_run, &mut self.offscreen_run_ink, address),
            );
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
        let (samples, mut cell_ink) = self
            .samples
            .get_mut_with_ink((py * self.width + px) as usize);
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
                        cell_ink.remove(sample);
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
                let background = samples.indices[sample];
                let mut rgb = [0; 3];
                cell_ink.update(
                    sample,
                    || Ink {
                        foreground,
                        alpha: 0,
                        background: IndexedColor::Solid(background),
                    },
                    |ink| {
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
                        rgb = ink.rgb(palette);
                    },
                );
                samples.rgb[sample] = rgb;
            }
        }
    }

    /// `glyph_pixel` for a run of consecutive guest pixels of `lanes` bytes
    /// each, starting at `address` (pixel `x0`, row `y`): byte `i` is lane
    /// `i % lanes` of pixel `x0 + i / lanes`, with foreground byte
    /// `foreground[i % lanes]` over `backgrounds[i]`. An offscreen run marks
    /// its pages once and looks each chunk up once; a run touching the
    /// screen takes the per-pixel path.
    pub(crate) fn glyph_span(
        &mut self,
        address: u32,
        (x0, y): (i16, i16),
        lanes: usize,
        foreground: &[u8],
        backgrounds: &[u8],
        stored: &[u8],
    ) {
        let end = u64::from(address) + backgrounds.len() as u64;
        let screen_end = u64::from(self.base) + u64::from(self.row_bytes) * u64::from(self.height);
        let offscreen = end <= u64::from(self.base) || u64::from(address) >= screen_end;
        if backgrounds.is_empty() || self.glyph.is_none() || !offscreen || end > 1 << 32 {
            for (i, &background) in backgrounds.iter().enumerate() {
                let x = x0.wrapping_add((i / lanes) as i16);
                self.glyph_pixel(address + i as u32, x, y, foreground[i % lanes], background);
            }
            return;
        }
        for _ in backgrounds {
            self.changed(false);
        }
        self.include_offscreen_span(address, backgrounds.len());
        let Some((glyph, h, v)) = &self.glyph else {
            return;
        };
        let scale = self.scale;
        let samples = (scale * scale) as usize;
        let in_text_run = self.in_text_run;
        let run_ink = &mut self.offscreen_run_ink;
        // A pixel the glyph leaves uncovered keeps a blank cell only where
        // the bitmap glyph may store to it: the cell then holds the
        // background the store must not show. Elsewhere a blank cell would
        // show exactly the unchanged byte, so none is made.
        let rows = GlyphRowSamples::new((glyph, *h, *v), scale, y);
        self.offscreen.selected_cells_mut_or_insert(
            address,
            backgrounds,
            samples,
            |i| {
                let x = x0.wrapping_add((i / lanes) as i16);
                let coverage = rows.cell(x);
                (coverage.is_some() || stored[i / lanes] != 0).then_some(coverage)
            },
            |i, coverage, cell| {
                if let Some(alphas) = coverage {
                    paint_offscreen_glyph_coverage(
                        cell,
                        &alphas[..samples],
                        foreground[i % lanes],
                        (in_text_run, run_ink, address + i as u32),
                    );
                }
            },
        );
    }

    /// `include_offscreen_address` for every byte of `[address, address +
    /// len)`, which must not wrap: once per page, at the span's first byte in
    /// it, which is where the per-byte calls would first reach the page.
    fn include_offscreen_span(&mut self, address: u32, len: usize) {
        let last = address + (len as u32 - 1);
        let mut at = address;
        loop {
            self.include_offscreen_address(at);
            let next_page = (u64::from(at) >> STORE_FILTER_PAGE_SHIFT) + 1;
            let next = next_page << STORE_FILTER_PAGE_SHIFT;
            if next > u64::from(last) {
                break;
            }
            at = next as u32;
        }
        self.include_offscreen_address(last);
    }
}

/// Paint the glyph's coverage for guest pixel (`x`, `y`) into one offscreen
/// cell: the offscreen half of `Presentation::glyph_pixel`, shared with the
/// span form. Opaque runs record each inked sample in `run_ink`.
fn paint_offscreen_glyph_cell(
    cell: &mut offscreen::OffscreenCellMut<'_>,
    (glyph, h, v): (&OutlineGlyph, i16, i16),
    scale: u32,
    (x, y, foreground): (i16, i16, u8),
    (in_text_run, run_ink, address): (bool, &mut OffscreenRunInk, u32),
) {
    if let Some(alphas) = offscreen_glyph_cell_coverage((glyph, h, v), scale, (x, y)) {
        paint_offscreen_glyph_coverage(
            cell,
            &alphas[..(scale * scale) as usize],
            foreground,
            (in_text_run, run_ink, address),
        );
    }
}

/// The glyph's coverage of guest pixel (`x`, `y`), one sample per entry, or
/// `None` when the glyph leaves the pixel uncovered.
fn offscreen_glyph_cell_coverage(
    glyph: (&OutlineGlyph, i16, i16),
    scale: u32,
    (x, y): (i16, i16),
) -> Option<[u8; TILE_SAMPLES]> {
    GlyphRowSamples::new(glyph, scale, y).cell(x)
}

/// The glyph's sample rows under guest row `y`, found once for a run of
/// pixels along the row; `cell` reads each pixel's coverage from them.
struct GlyphRowSamples<'a> {
    glyph: &'a OutlineGlyph,
    h: i16,
    scale: usize,
    /// Each sample row's glyph row, empty where the glyph has none.
    rows: [&'a [u8]; 4],
    any_row: bool,
}

impl<'a> GlyphRowSamples<'a> {
    fn new((glyph, h, v): (&'a OutlineGlyph, i16, i16), scale: u32, y: i16) -> Self {
        let scale = scale as usize;
        assert!(scale * scale <= TILE_SAMPLES, "cell samples exceed a tile");
        let gy0 = (i32::from(y) - i32::from(v)) * scale as i32 - glyph.top;
        let mut rows: [&[u8]; 4] = [&[]; 4];
        for (sy, row) in rows[..scale].iter_mut().enumerate() {
            let gy = gy0 + sy as i32;
            if gy >= 0 && gy < glyph.height {
                *row = &glyph.pixels[(gy * glyph.width) as usize..][..glyph.width as usize];
            }
        }
        let any_row = rows.iter().any(|row| !row.is_empty());
        Self {
            glyph,
            h,
            scale,
            rows,
            any_row,
        }
    }

    /// The coverage of pixel `x`, one sample per entry, or `None` when the
    /// glyph leaves it uncovered.
    fn cell(&self, x: i16) -> Option<[u8; TILE_SAMPLES]> {
        let scale = self.scale;
        let width = self.glyph.width;
        let gx0 = (i32::from(x) - i32::from(self.h)) * scale as i32 - self.glyph.left;
        if !self.any_row || gx0 + scale as i32 <= 0 || gx0 >= width {
            return None;
        }
        let mut alphas = [0u8; TILE_SAMPLES];
        let mut any = 0u8;
        let rows = self.rows[..scale].iter().enumerate();
        if gx0 >= 0 && gx0 + scale as i32 <= width {
            // Wholly inside the glyph's columns: one slice per sample row.
            let gx0 = gx0 as usize;
            for (sy, row) in rows.filter(|(_, row)| !row.is_empty()) {
                let samples = &row[gx0..gx0 + scale];
                alphas[sy * scale..][..scale].copy_from_slice(samples);
                any |= samples.iter().fold(0, |any, &alpha| any | alpha);
            }
        } else {
            for (sy, row) in rows.filter(|(_, row)| !row.is_empty()) {
                for sx in 0..scale {
                    let gx = gx0 + sx as i32;
                    if gx >= 0 && gx < width {
                        alphas[sy * scale + sx] = row[gx as usize];
                        any |= row[gx as usize];
                    }
                }
            }
        }
        (any != 0).then_some(alphas)
    }
}

/// Paint `alphas`, a covered pixel's samples, into its offscreen cell.
fn paint_offscreen_glyph_coverage(
    cell: &mut offscreen::OffscreenCellMut<'_>,
    alphas: &[u8],
    foreground: u8,
    (in_text_run, run_ink, address): (bool, &mut OffscreenRunInk, u32),
) {
    if in_text_run {
        for (i, &alpha) in alphas.iter().enumerate() {
            if alpha != 0 {
                run_ink.insert((address, i));
            }
        }
    }
    cell.paint_glyph(alphas, foreground);
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
    /// Store `pixels` (laid end to end) through `palette` at each span
    /// (destination, length) and paste the spans' cells from `copied`, then
    /// hand the scratch back to the presentation.
    /// `copy_detail_spans` from a saved snapshot instead of live memory:
    /// copy the snapshot's `spans` (destination, offset in `saved`, length)
    /// exactly as `copy_saved_pixel` through `palette` would per pixel, in
    /// order. Declines (returning false, having changed nothing) unless
    /// every destination qualifies under open gates and identity translation.
    pub(crate) fn copy_saved_spans(
        &mut self,
        spans: &[(u32, usize, usize)],
        saved: &SavedPixels,
        palette: Option<&[u8; 256]>,
    ) -> bool {
        if spans.is_empty() || !self.presented_bytes_gates_open() {
            return false;
        }
        let eligible = spans.iter().all(|&(destination, _, len)| {
            self.range_translates_contiguously(destination, len) == Some(destination)
                && self.presented_bytes_writable(destination, len)
        });
        if !eligible {
            return false;
        }
        self.with_presentation_and_ram(|presentation, store| {
            presentation.copy_saved_spans(spans, saved, palette, |destination, row| {
                store(destination, row)
            })
        })
    }

    /// Copy `rows` (source, destination), `row_len` bytes each, whose source
    /// bytes are `pixels`: `copy_detail_spans` with equal spans.
    pub(crate) fn copy_detail_rows(
        &mut self,
        rows: &[(u32, u32)],
        pixels: &[u8],
        row_len: usize,
        palette: Option<&[u8; 256]>,
    ) -> bool {
        if row_len == 0 {
            return false;
        }
        let spans: Vec<(u32, u32, usize)> = rows
            .iter()
            .map(|&(source, destination)| (source, destination, row_len))
            .collect();
        self.copy_detail_spans(&spans, pixels, palette)
    }

    /// Copy `spans` (source, destination, length), whose source bytes are
    /// `pixels` laid end to end, exactly as capturing every source span's
    /// detail and then copying each pixel through `palette` would. Declines
    /// (returning false, having changed nothing) unless every span qualifies
    /// for `can_copy_detail_row` under open gates and identity translation.
    pub(crate) fn copy_detail_spans(
        &mut self,
        spans: &[(u32, u32, usize)],
        pixels: &[u8],
        palette: Option<&[u8; 256]>,
    ) -> bool {
        if spans.is_empty() || !self.presented_bytes_gates_open() {
            return false;
        }
        let eligible = spans.iter().all(|&(source, destination, len)| {
            self.range_translates_contiguously(source, len) == Some(source)
                && self.range_translates_contiguously(destination, len) == Some(destination)
                && self.presented_bytes_writable(destination, len)
        });
        if !eligible {
            return false;
        }
        self.with_presentation_and_ram(|presentation, store| {
            presentation.copy_detail_spans(spans, pixels, palette, |destination, row| {
                store(destination, row)
            })
        })
    }

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
        // Plain bytes landing on a screen row that holds text (text
        // scrolling away) clear it cell by cell, still in one pass.
        let over_text = !plain
            && self
                .presentation
                .as_ref()
                .is_some_and(|p| p.can_sync_screen_row_over_text(destination, len));
        if !plain && !over_text {
            return false;
        }
        let mut row = pixels.values[span].to_vec();
        if let Some(palette) = palette {
            for pixel in &mut row {
                *pixel = palette[*pixel as usize];
            }
        }
        if over_text {
            return self.write_presented_bytes_over_text(address, &row);
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
                    || !p.offscreen.any_in(source, source_end))
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
                    || !p.offscreen.any_in(source, source_end))
            {
                return;
            }
            let changes = {
                let mut source_cells = p.offscreen.range(source, source_end).into_iter().peekable();
                let mut changes = Vec::new();
                for (i, &value) in bytes.iter().enumerate() {
                    let address = source + i as u32;
                    let detail = if source_cells.peek().is_some_and(|(key, _)| *key == address) {
                        source_cells.next().map(|(_, cell)| cell)
                    } else {
                        None
                    };
                    let destination = destination + i as u32;
                    if !p.matches_detail(destination, detail.as_ref()) {
                        changes.push((destination, value, detail));
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

    /// `outline_glyph_pixel` for `count` consecutive pixels of `lanes` bytes
    /// from `address` (pixel `x0` of row `y`), each lane taking its byte of
    /// the big-endian `foreground`. The backgrounds are read up front, as
    /// the per-pixel calls would read them before drawing the run. `stored`
    /// holds each pixel's bitmap-glyph coverage: zero where the drawing will
    /// not store to the pixel.
    pub(crate) fn outline_glyph_span(
        &mut self,
        address: u32,
        (x0, y): (i16, i16),
        count: usize,
        lanes: usize,
        foreground: u32,
        stored: &[u8],
    ) {
        if self.presentation.is_none() || count == 0 {
            return;
        }
        // A glyph's run fits on the stack; read longer runs into the heap.
        let len = count * lanes;
        let mut stack = [0u8; 256];
        let heap;
        let backgrounds: &[u8] = if len <= stack.len() {
            self.read_bytes_into(address, &mut stack[..len]);
            &stack[..len]
        } else {
            heap = self.read_bytes(address, len);
            &heap
        };
        let foreground: [u8; 4] = std::array::from_fn(|lane| {
            (foreground >> ((lanes.saturating_sub(1 + lane)) * 8)) as u8
        });
        if let Some(mut p) = self.presentation.as_mut() {
            p.glyph_span(
                address,
                (x0, y),
                lanes,
                &foreground[..lanes],
                backgrounds,
                stored,
            );
        }
    }

    pub(crate) fn save_pixel_bytes(&self, address: u32, len: usize) -> SavedPixels {
        let mut pixels = SavedPixels::from(self.read_bytes(address, len));
        // The sparse range walk, not a `detail` query per byte: callers save
        // whole menu bars and window frames every frame, and only text cells
        // and retained glyphs carry detail.
        self.presentation
            .capture_detail(&mut pixels, 0, address, len);
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
        let Some(cell) = pixels.detail.get(&offset) else {
            self.write_byte(address, value);
            return;
        };
        let cell = mapped_detail_cell(cell, value, &mut map);
        self.store_detail_pixel(address, value, &cell);
    }

    /// `copy_saved_pixel` through a CopyBits colour table. Text copied
    /// through the same table repeats the same source cells (a scrolling
    /// crawl redraws its glyphs every frame), so their mapped cells come
    /// from `copy_map_cache` instead of a fresh clone per pixel.
    pub(crate) fn copy_saved_pixel_through(
        &mut self,
        address: u32,
        pixels: &SavedPixels,
        offset: usize,
        table: Option<&[u8; 256]>,
    ) {
        let mut map = |index: u8| table.map_or(index, |table| table[index as usize]);
        let value = map(pixels[offset]);
        let Some(cell) = pixels.detail.get(&offset) else {
            self.write_byte(address, value);
            return;
        };
        let cell = match table {
            Some(table) => self.copy_map_cache.mapped(cell, value, table),
            None => mapped_detail_cell(cell, value, &mut map),
        };
        self.store_detail_pixel(address, value, &cell);
    }

    /// Store a copied text byte and its cell.
    fn store_detail_pixel(&mut self, address: u32, value: u8, cell: &Arc<DetailCell>) {
        // `put_detail` replaces the destination's whole cell, so the store's
        // own presentation update (clearing any text there first) is wasted.
        // When the cell is already in place (a HUD redrawn every frame),
        // `put_detail` then leaves the presentation untouched as well.
        let superseded = self.presentation.as_mut().is_some_and(|mut p| {
            let superseded = p.detail_supersedes_store(address, cell);
            if superseded {
                p.superseded_write = Some(address);
            }
            superseded
        });
        self.write_byte(address, value);
        if let Some(mut p) = self.presentation.as_mut() {
            if superseded {
                p.superseded_write = None;
            }
            p.put_detail(address, cell);
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

    /// Whether `bytes` already sit at `address` as plain pixels: the same
    /// guest bytes, with no retained text over any of them (a screen span
    /// within one row, or any span when nothing is presented). Storing them
    /// again would change nothing, so a restore may skip the span. False
    /// while read tracing needs to see the bytes.
    pub(crate) fn span_holds_plain_bytes(&self, address: u32, bytes: &[u8]) -> bool {
        self.untraced_ram_slice(address, bytes.len())
            .is_some_and(|held| held == bytes)
            && self
                .presentation
                .as_ref()
                .is_none_or(|p| p.plain_screen_row(address, bytes.len()))
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
        // The range walk visits only screen cells holding text and the
        // offscreen cells in the span (row padding included), which is
        // exactly what a `detail` query per byte would find.
        self.presentation
            .capture_detail(pixels, offset, address, len);
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
        let plain_at = |i: usize| {
            let value = pixels[i].into() as u8;
            pixels
                .detail
                .get(&i)
                .filter(|cell| cell.value == value)
                .is_none()
        };
        let mut next = offset;
        for i in offset..end {
            if i < next {
                continue;
            }
            let dst = address + (i - offset) as u32;
            let value = pixels[i].into() as u8;
            // A run of bytes without text is a plain store, which the bulk
            // store applies a row span at a time; storing a byte's own value
            // over a plain cell changes nothing, as the check below skips.
            if plain_at(i) {
                next = (i..end).find(|&j| !plain_at(j)).unwrap_or(end);
                if next - i > 1 {
                    let run: Vec<u8> = pixels[i..next]
                        .iter()
                        .map(|value| (*value).into() as u8)
                        .collect();
                    self.write_bytes(dst, &run);
                    continue;
                }
            }
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
                p.samples.for_each_ink_mut(|tile, sample, ink| {
                    tile.rgb[sample] = ink.rgb(&palette);
                });
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
            offscreen: OffscreenDetail::default(),
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
            ink_mask: vec![0; width as usize * height as usize],
            run_ink: HashSet::default(),
            offscreen_run_ink: OffscreenRunInk::default(),
            in_text_run: false,
            erasing_text: false,
            glyph: None,
            glyph_count: 0,
            superseded_write: None,
            copied: CopiedDetail::default(),
        };
        for y in 0..u32::from(height) {
            for x in 0..u32::from(width) {
                let address = base + y * row_bytes + x;
                presentation.write(address, self.read_byte(address));
            }
        }
        if let Some((offscreen, glyph_count)) = retained {
            let addresses = offscreen.addresses();
            presentation.offscreen_bounds = addresses
                .first()
                .zip(addresses.last())
                .map(|(&first, &last)| (first, last));
            for &address in &addresses {
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
                outline.pixels = pixels.into();
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
                outline.pixels = pixels.into();
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
                    pixels: pixels.into(),
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
        glyph.pixels = pixels.into();
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
                pixels: vec![64, 255, 0, 128].into(),
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

    #[test]
    fn plain_bytes_check_sees_text_under_matching_bytes() {
        let mut bus = bus();
        assert!(bus.span_holds_plain_bytes(0x1000, &[255; 8]));
        assert!(!bus.span_holds_plain_bytes(0x1000, &[255, 255, 255, 255, 255, 255, 255, 0]));
        paint_detail(&mut bus, 0x1002);
        let row = bus.read_bytes(0x1000, 8);
        assert!(
            !bus.span_holds_plain_bytes(0x1000, &row),
            "equal bytes over retained text are not plain"
        );
        assert!(bus.span_holds_plain_bytes(0x1003, &row[3..]));
    }

    #[test]
    fn glyph_spans_match_the_per_pixel_calls() {
        let glyph = OutlineGlyph {
            pixels: (0..40 * 6)
                .map(|i| [0, 255, 64, 128, 0, 200, 255][i % 7])
                .collect(),
            width: 40,
            height: 6,
            left: -1,
            top: 0,
        };
        let setup = |start: u32, opaque: bool| {
            let mut bus = MacMemoryBus::new(1024 * 1024);
            let palette = std::array::from_fn(|i| [i as u8; 3]);
            bus.enable_outline_presentation((0x1000, 8, 8, 8, 8), palette, 4);
            for i in 0..96u32 {
                bus.write_byte(start + i, (i * 7) as u8);
            }
            let mut p = bus.presentation.as_mut().unwrap();
            p.glyph = Some((glyph.clone(), 0, 0));
            p.in_text_run = opaque;
            drop(p);
            bus
        };
        let snapshot = |bus: &MacMemoryBus| {
            let p = bus.presentation.as_ref().unwrap();
            let cells: Vec<(u32, DetailCell)> = p
                .offscreen
                .addresses()
                .into_iter()
                .map(|address| (address, (*p.offscreen.get(address).unwrap()).clone()))
                .collect();
            (
                cells,
                p.revision,
                p.offscreen_bounds,
                p.store_filter_new_pages.clone(),
                p.offscreen_run_ink.clone(),
            )
        };
        for lanes in [1usize, 2, 4] {
            for opaque in [false, true] {
                // Within a chunk, across a 256-byte chunk and across a page.
                for start in [0x4_0010u32, 0x4_00F8, 0x4_0FF0] {
                    let mut per_pixel = setup(start, opaque);
                    let mut span = setup(start, opaque);
                    let count = 20;
                    for (y, foreground) in
                        [(1i16, 0x0A0B_0C0Du32), (1, 0x1112_1314), (2, 0x0A0B_0C0D)]
                    {
                        for i in 0..count * lanes {
                            let lane = i % lanes;
                            per_pixel.outline_glyph_pixel(
                                start + i as u32,
                                3 + (i / lanes) as i16,
                                y,
                                (foreground >> ((lanes - 1 - lane) * 8)) as u8,
                            );
                        }
                        span.outline_glyph_span(
                            start,
                            (3, y),
                            count,
                            lanes,
                            foreground,
                            &[255; 20],
                        );
                        assert_eq!(
                            snapshot(&span),
                            snapshot(&per_pixel),
                            "lanes {lanes} opaque {opaque} start {start:#x} y {y}"
                        );
                    }
                }
            }
        }
    }

    /// A glyph span makes no cell for a pixel the glyph leaves uncovered and
    /// the bitmap glyph does not store to, where the per-pixel path makes a
    /// blank one: once the bitmap glyph's stores land and the rows reach the
    /// screen, both look the same.
    #[test]
    fn glyph_spans_skip_blank_cells_that_nothing_stores_to() {
        // Coverage in glyph pixels 0, 3 and 6 of each row; the rest blank.
        let glyph = OutlineGlyph {
            pixels: (0..32 * 8)
                .map(|i| if (i % 32 / 4) % 3 == 0 { 200 } else { 0 })
                .collect(),
            width: 32,
            height: 8,
            left: 0,
            top: 0,
        };
        let start = 0x4_0010u32;
        // Bitmap stores at a covered pixel (0) and two uncovered ones (1, 4).
        let stored = [255u8, 255, 0, 0, 255, 0, 0, 0];
        let foreground = 0x2A;
        for opaque in [false, true] {
            let setup = || {
                let mut bus = MacMemoryBus::new(1024 * 1024);
                let palette = std::array::from_fn(|i| [i as u8, (i * 3) as u8, 255 - i as u8]);
                bus.enable_outline_presentation((0x1000, 8, 8, 8, 8), palette, 4);
                for i in 0..16u32 {
                    bus.write_byte(start + i, 0x10 + i as u8);
                }
                let mut p = bus.presentation.as_mut().unwrap();
                p.glyph = Some((glyph.clone(), 0, 0));
                p.in_text_run = opaque;
                drop(p);
                bus
            };
            let mut per_pixel = setup();
            let mut span = setup();
            for y in 0..2i16 {
                let row = start + y as u32 * 8;
                for x in 0..8u32 {
                    per_pixel.outline_glyph_pixel(row + x, x as i16, y, foreground);
                }
                span.outline_glyph_span(row, (0, y), 8, 1, u32::from(foreground), &stored);
                for bus in [&mut per_pixel, &mut span] {
                    for (x, &alpha) in stored.iter().enumerate() {
                        if alpha != 0 {
                            bus.write_byte(row + x as u32, foreground);
                        }
                    }
                }
            }
            let cells = |bus: &MacMemoryBus| {
                bus.presentation
                    .as_ref()
                    .unwrap()
                    .offscreen
                    .addresses()
                    .len()
            };
            assert!(
                cells(&span) < cells(&per_pixel),
                "opaque {opaque}: blank cells skipped"
            );
            for bus in [&mut per_pixel, &mut span] {
                bus.end_outline_glyph();
                let pixels = bus.read_bytes(start, 16);
                assert!(bus.copy_detail_rows(
                    &[(start, 0x1000), (start + 8, 0x1008)],
                    &pixels,
                    8,
                    None
                ));
            }
            assert_eq!(
                span.read_bytes(0x1000, 16),
                per_pixel.read_bytes(0x1000, 16),
                "opaque {opaque}: RAM"
            );
            assert_eq!(
                span.outline_presentation_rgb(),
                per_pixel.outline_presentation_rgb(),
                "opaque {opaque}: rendered"
            );
        }
    }

    /// A row's sampler reads every pixel's coverage exactly as sampling the
    /// glyph one sample at a time does, at every scale, over and around
    /// glyphs whose edges fall inside cells.
    #[test]
    fn glyph_row_samples_match_per_sample_coverage() {
        let per_sample =
            |glyph: &OutlineGlyph, (h, v): (i16, i16), scale: i32, (x, y): (i16, i16)| {
                let gx0 = (i32::from(x) - i32::from(h)) * scale - glyph.left;
                let gy0 = (i32::from(y) - i32::from(v)) * scale - glyph.top;
                let mut alphas = [0u8; TILE_SAMPLES];
                let mut any = false;
                for sy in 0..scale {
                    for sx in 0..scale {
                        let (gx, gy) = (gx0 + sx, gy0 + sy);
                        if gx >= 0 && gx < glyph.width && gy >= 0 && gy < glyph.height {
                            let alpha = glyph.pixels[(gy * glyph.width + gx) as usize];
                            alphas[(sy * scale + sx) as usize] = alpha;
                            any |= alpha != 0;
                        }
                    }
                }
                any.then_some(alphas)
            };
        for scale in 1..=4i32 {
            for (width, height, left, top) in [(9, 7, -3, 2), (13, 11, 1, -5), (4, 4, 0, 0)] {
                let glyph = OutlineGlyph {
                    pixels: (0..width * height)
                        .map(|i| [0, 255, 0, 90, 0, 0, 17][(i * 5 % 7) as usize])
                        .collect(),
                    width,
                    height,
                    left,
                    top,
                };
                let (h, v) = (2i16, -1i16);
                for y in -8..8i16 {
                    let rows = GlyphRowSamples::new((&glyph, h, v), scale as u32, y);
                    for x in -8..12i16 {
                        assert_eq!(
                            rows.cell(x),
                            per_sample(&glyph, (h, v), scale, (x, y)),
                            "scale {scale} glyph {width}x{height} at ({left},{top}) pixel ({x},{y})"
                        );
                    }
                }
            }
        }
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
        assert_ne!(
            page_byte(&bus, 0x3_0000),
            0,
            "offscreen detail now lives here"
        );
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
        assert_eq!(
            page_byte(&bus, 0x5_0000),
            2,
            "the page holds protected code"
        );
    }

    #[test]
    fn an_overlapping_copy_is_the_same_with_a_write_probe_armed() {
        // A destination above an overlapping source, away from the screen.
        let copy = |probe: bool| {
            let mut bus = bus();
            for i in 0..16u32 {
                bus.write_byte(0x8_0000 + i, i as u8 + 1);
            }
            if probe {
                bus.begin_write_probe();
            }
            assert!(bus.copy_ram_bytes(0x8_0000, 0x8_0004, 12));
            if probe {
                bus.cancel_write_probe();
            }
            bus.read_bytes(0x8_0000, 16)
        };
        let plain = copy(false);
        assert_eq!(plain, [1, 2, 3, 4, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(copy(true), plain);
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
        assert_eq!(
            bus.store_filter_byte(0),
            Some(0),
            "suspended probes record nothing"
        );
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
        for address in [
            0x1000, 0x1003, 0x1009, 0x100b, 0x1025, 0x105f, 0x0ffd, 0x1060, 0x1063,
        ] {
            paint_detail(&mut bus, address);
        }
        let points = [
            0x0ff8u32, 0x0ffd, 0x0ffe, 0x1000, 0x1004, 0x1008, 0x100b, 0x100c, 0x1024, 0x1026,
            0x105f, 0x1060, 0x1064,
        ];
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
        for row_bytes in [
            1u32, 2, 3, 7, 8, 10, 63, 64, 640, 641, 832, 1024, 1920, 4095, 65535,
        ] {
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
            // The screen row that already holds unrelated text: cleared
            // cell by cell in one pass.
            (0x1000 + 10 * 3, 8, vec![], true),
            // Source text onto that row: not a plain span.
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
                let context = format!(
                    "{destination:#x}+{len} detail={detail:?} palette={}",
                    palette.is_some()
                );
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
                assert_eq!(
                    fast.read_bytes(destination, len),
                    slow.read_bytes(destination, len),
                    "{context}: RAM"
                );
                assert_eq!(
                    fast.outline_presentation_rgb(),
                    slow.outline_presentation_rgb(),
                    "{context}: rendered"
                );
                let after_fast = fast.save_pixel_bytes(destination - 2, len + 4);
                let after_slow = slow.save_pixel_bytes(destination - 2, len + 4);
                assert_eq!(
                    after_fast.values, after_slow.values,
                    "{context}: snapshot values"
                );
                let keys = |p: &SavedPixels| {
                    let mut k: Vec<_> = p.detail.iter().map(|(&i, c)| (i, (**c).clone())).collect();
                    k.sort_by_key(|(i, _)| *i);
                    k
                };
                assert_eq!(
                    keys(&after_fast),
                    keys(&after_slow),
                    "{context}: snapshot detail"
                );
            }
        }
    }

    /// Copying text skips the store's own presentation update, which only
    /// clears the destination before `put_detail` replaces the cell. The
    /// result must equal doing both, whatever the destination held: the same
    /// text (a HUD redrawn every frame), other text, or plain pixels.
    #[test]
    fn copying_text_matches_the_full_store_and_detail() {
        let source = 0x3_0000u32;
        let inverted: [u8; 256] = std::array::from_fn(|i| (255 - i) as u8);
        for destination in [0x1000u32 + 10 * 2 + 1, 0x4_0000] {
            for prior in ["same text", "other text", "plain"] {
                for palette in [None, Some(&inverted)] {
                    let map = |index: u8| {
                        palette.map_or(index, |table: &[u8; 256]| table[index as usize])
                    };
                    let setup = || {
                        let mut bus = padded_bus(10, 8, 6, 2);
                        paint_detail(&mut bus, source);
                        paint_detail(&mut bus, source + 1);
                        bus.write_byte(source + 2, 9);
                        let pixels = bus.save_pixel_bytes(source, 3);
                        for i in 0..3 {
                            let address = destination + i as u32;
                            match prior {
                                "same text" => bus.copy_saved_pixel(address, &pixels, i, map),
                                "other text" => paint_detail(&mut bus, address),
                                _ => bus.write_byte(address, 3),
                            }
                        }
                        bus
                    };
                    let mut fast = setup();
                    let mut full = setup();
                    let pixels = fast.save_pixel_bytes(source, 3);
                    assert!(pixels.has_detail_at(0) && pixels.has_detail_in(0..2));
                    let revision = |bus: &MacMemoryBus| bus.presentation.as_ref().unwrap().revision;
                    let (fast_before, full_before) = (revision(&fast), revision(&full));
                    for i in 0..3 {
                        let address = destination + i as u32;
                        fast.copy_saved_pixel(address, &pixels, i, map);
                        // The full store, as copies worked before: the byte,
                        // then the mapped cell.
                        full.write_byte(address, map(pixels[i]));
                        if let Some(cell) = pixels.detail.get(&i) {
                            let mut cell = cell.clone();
                            let mapped = Arc::make_mut(&mut cell);
                            mapped.value = map(mapped.value);
                            for index in &mut mapped.indices {
                                *index = map(*index);
                            }
                            for ink in mapped.ink.values_mut() {
                                ink.foreground = map(ink.foreground);
                                ink.background.map(&mut |index| map(index));
                            }
                            full.presentation
                                .as_mut()
                                .unwrap()
                                .put_detail(address, &cell);
                        }
                    }
                    let context = format!("{destination:#x} {prior} palette={}", palette.is_some());
                    if prior == "same text" {
                        assert_eq!(
                            revision(&fast),
                            fast_before,
                            "{context}: presentation untouched"
                        );
                        assert_ne!(
                            revision(&full),
                            full_before,
                            "{context}: full store rebuilt"
                        );
                    }
                    assert_eq!(
                        fast.read_bytes(destination, 3),
                        full.read_bytes(destination, 3),
                        "{context}: RAM"
                    );
                    assert_eq!(
                        fast.outline_presentation_rgb(),
                        full.outline_presentation_rgb(),
                        "{context}: rendered"
                    );
                    let after_fast = fast.save_pixel_bytes(destination - 1, 5);
                    let after_full = full.save_pixel_bytes(destination - 1, 5);
                    assert_eq!(after_fast, after_full, "{context}: snapshot");
                    assert!(
                        after_fast.has_detail_at(1) && after_fast.has_detail_at(2),
                        "{context}: text kept"
                    );
                    // A later plain store must still clear the copied text.
                    fast.write_byte(destination, 7);
                    full.write_byte(destination, 7);
                    assert_eq!(
                        fast.save_pixel_bytes(destination, 1),
                        full.save_pixel_bytes(destination, 1)
                    );
                    assert!(
                        !fast.save_pixel_bytes(destination, 1).has_detail_at(0),
                        "{context}: cleared"
                    );
                    assert_eq!(
                        fast.outline_presentation_rgb(),
                        full.outline_presentation_rgb(),
                        "{context}: cleared render"
                    );
                }
            }
        }
    }

    /// CopyBits through a colour table shares one mapped cell among equal
    /// source cells, starts over when the table changes, and always leaves
    /// the same result as mapping each pixel afresh.
    #[test]
    fn copying_text_through_a_table_shares_mapped_cells() {
        use crate::copy_bits::CopyBitsMemory;
        let inverted: [u8; 256] = std::array::from_fn(|i| (255 - i) as u8);
        let rotated: [u8; 256] = std::array::from_fn(|i| (i as u8).wrapping_add(1));
        let source = 0x3_0000u32;
        let setup = || {
            let mut bus = padded_bus(10, 8, 6, 2);
            // Two equal glyph cells and a plain byte between them.
            paint_detail(&mut bus, source);
            bus.write_byte(source + 1, 9);
            paint_detail(&mut bus, source + 2);
            bus
        };
        let mut fast = setup();
        let mut slow = setup();
        let pixels = fast.save_pixel_bytes(source, 3);
        assert_eq!(
            **pixels.detail.get(&0).unwrap(),
            **pixels.detail.get(&2).unwrap()
        );
        for (row, table) in [(1u32, &inverted), (2, &inverted), (3, &rotated)] {
            let destination = 0x1000 + 10 * row + 1;
            fast.write_copy_pixels(destination, &pixels, 0, 3, Some(table))
                .expect("writable");
            for i in 0..3 {
                slow.copy_saved_pixel(destination + i as u32, &pixels, i, |index| {
                    table[index as usize]
                });
            }
            let context = format!("row {row}");
            assert_eq!(
                fast.read_bytes(destination, 3),
                slow.read_bytes(destination, 3),
                "{context}: RAM"
            );
            assert_eq!(
                fast.outline_presentation_rgb(),
                slow.outline_presentation_rgb(),
                "{context}: rendered"
            );
            assert_eq!(
                fast.save_pixel_bytes(destination, 3),
                slow.save_pixel_bytes(destination, 3),
                "{context}: snapshot"
            );
            // Equal source cells share one entry; a new table starts over.
            assert_eq!(fast.copy_map_cache.len, 1, "{context}: cache entries");
        }
        let mapped = |bus: &MacMemoryBus, address| {
            bus.save_pixel_bytes(address, 1)
                .detail
                .get(&0)
                .cloned()
                .unwrap()
        };
        assert!(Arc::ptr_eq(
            &mapped(&fast, 0x1000 + 10 + 1),
            &mapped(&fast, 0x1000 + 10 + 3)
        ));
    }

    /// Copying rows of offscreen text to the screen straight from the chunk
    /// store leaves exactly what capturing each source row and copying it a
    /// pixel at a time leaves, whatever the destination held and with or
    /// without a colour table.
    #[test]
    fn plain_row_paste_marks_only_changed_rows() {
        let mut bus = bus();
        let source = 0x8000;
        bus.write_bytes(source, &[1, 2, 3, 4, 5, 6, 7, 8]);
        let pixels = bus.read_bytes(source, 8);
        let before = bus.screen_mark().expect("presented screen");
        assert!(bus.copy_detail_rows(&[(source, 0x1000 + 8)], &pixels, 8, None));
        assert_eq!(bus.read_bytes(0x1000 + 8, 8), pixels);
        assert!(
            !bus.screen_rect_unchanged_since(before, (1, 0, 8, 1)),
            "the pasted row reports a change"
        );
        assert!(
            bus.screen_rect_unchanged_since(before, (0, 0, 8, 1)),
            "the row above does not"
        );
        let again = bus.screen_mark().expect("presented screen");
        assert!(bus.copy_detail_rows(&[(source, 0x1000 + 8)], &pixels, 8, None));
        assert!(
            bus.screen_rect_unchanged_since(again, (1, 0, 8, 1)),
            "pasting the same bytes changes nothing"
        );
    }

    #[test]
    fn long_plain_runs_copy_to_the_screen_like_the_per_pixel_copy() {
        use crate::copy_bits::{BytePixmap, CopyBitsMemory, RowCopy, RowCopyOutcome};
        let inverted: [u8; 256] = std::array::from_fn(|i| (255 - i) as u8);
        let source = 0x3_0000u32;
        let (row_len, rows) = (32usize, 2u32);
        let destination = |row: u32| 0x1000 + (1 + row) * 40 + 4;
        // Source text at columns 10 and 25 splits each row into plain runs of
        // 10, 14 and 6 bytes: two take the bulk path, one the byte path.
        for prior in ["plain", "text in a long run", "same bytes", "same bytes, then text"] {
            for palette in [None, Some(&inverted)] {
                let setup = || {
                    let mut bus = padded_bus(40, 40, 4, 2);
                    for row in 0..rows {
                        for i in 0..row_len as u32 {
                            bus.write_byte(source + row * 40 + i, (row * 11 + i * 5) as u8);
                        }
                        paint_detail(&mut bus, source + row * 40 + 10);
                        paint_detail(&mut bus, source + row * 40 + 25);
                    }
                    match prior {
                        "text in a long run" => paint_detail(&mut bus, destination(0) + 17),
                        "same bytes" | "same bytes, then text" => {
                            if prior == "same bytes, then text" {
                                // The text cell below keeps the byte the copy
                                // stores, so only its text marks it for work.
                                let zero = palette.map_or(0, |_| 255);
                                bus.write_byte(source + 20, zero);
                            }
                            for row in 0..rows {
                                let pixels = bus.read_bytes(source + row * 40, row_len);
                                let mapped: Vec<u8> = pixels
                                    .iter()
                                    .map(|&v| palette.map_or(v, |table| table[v as usize]))
                                    .collect();
                                bus.write_bytes(destination(row), &mapped);
                            }
                            if prior == "same bytes, then text" {
                                // Text partway along the 14-byte run, after
                                // bytes that already match.
                                paint_detail(&mut bus, destination(0) + 20);
                            }
                        }
                        _ => {}
                    }
                    bus
                };
                let mut direct = setup();
                let mut oracle = setup();
                let context = format!("{prior} palette={}", palette.is_some());
                let copy = RowCopy {
                    mode: 0,
                    source: BytePixmap {
                        base: source,
                        row_bytes: 40,
                        depth: 8,
                        bounds: [0, 0, 3, 40],
                    },
                    destination: BytePixmap {
                        base: 0x1000,
                        row_bytes: 40,
                        depth: 8,
                        bounds: [0, 0, 4, 40],
                    },
                    source_rect: [0, 0, 2, 32],
                    destination_rect: [1, 4, 3, 36],
                    clip: [0, 0, 4, 40],
                    palette,
                };
                assert_eq!(
                    copy.execute(&mut direct),
                    RowCopyOutcome::Completed,
                    "{context}"
                );
                let mut pixels = vec![0u8; row_len * rows as usize];
                for row in 0..rows as usize {
                    oracle
                        .read_copy_row(
                            source + row as u32 * 40,
                            &mut pixels[row * row_len..][..row_len],
                        )
                        .unwrap();
                }
                let mut pixels: SavedPixels = pixels.into();
                for row in 0..rows as usize {
                    oracle.capture_copy_detail(
                        source + row as u32 * 40,
                        &mut pixels,
                        row * row_len,
                        row_len,
                    );
                }
                for row in 0..rows {
                    oracle
                        .write_copy_pixels(
                            destination(row),
                            &pixels,
                            row as usize * row_len,
                            row_len,
                            palette,
                        )
                        .expect("writable");
                }
                assert_eq!(
                    direct.read_bytes(0x1000, 160),
                    oracle.read_bytes(0x1000, 160),
                    "{context}: RAM"
                );
                assert_eq!(
                    direct.outline_presentation_rgb(),
                    oracle.outline_presentation_rgb(),
                    "{context}: rendered"
                );
                for row in 0..4 {
                    assert_eq!(
                        direct.save_pixel_bytes(0x1000 + row * 40, 40),
                        oracle.save_pixel_bytes(0x1000 + row * 40, 40),
                        "{context}: snapshot row {row}"
                    );
                }
            }
        }
    }

    #[test]
    fn offscreen_rows_copy_to_the_screen_like_the_per_pixel_copy() {
        use crate::copy_bits::{BytePixmap, CopyBitsMemory, RowCopy, RowCopyOutcome};
        let inverted: [u8; 256] = std::array::from_fn(|i| (255 - i) as u8);
        let source = 0x3_0000u32;
        let (row_len, rows) = (6usize, 2u32);
        let destination = |row: u32| 0x1000 + (1 + row) * 10 + 1;
        for prior in ["plain", "other text", "same text"] {
            for palette in [None, Some(&inverted)] {
                let setup = || {
                    let mut bus = padded_bus(10, 8, 6, 2);
                    for row in 0..rows {
                        for i in 0..row_len as u32 {
                            bus.write_byte(source + row * 10 + i, (row * 7 + i * 3) as u8);
                        }
                        // Text in some columns of each source row.
                        paint_detail(&mut bus, source + row * 10 + 1);
                        paint_detail(&mut bus, source + row * 10 + 4 - row);
                    }
                    match prior {
                        "other text" => paint_detail(&mut bus, destination(0) + 2),
                        "same text" => {
                            let pixels = bus.save_pixel_bytes(source, row_len);
                            bus.write_copy_pixels(destination(0), &pixels, 0, row_len, palette)
                                .expect("writable");
                        }
                        _ => bus.write_byte(destination(1) + 3, 77),
                    }
                    bus
                };
                let mut direct = setup();
                let mut oracle = setup();
                let context = format!("{prior} palette={}", palette.is_some());
                assert!(
                    (0..rows).all(
                        |row| direct.presentation.as_ref().unwrap().can_copy_detail_row(
                            source + row * 10,
                            destination(row),
                            row_len,
                        )
                    ),
                    "{context}: the direct path applies"
                );
                let copy = RowCopy {
                    mode: 0,
                    source: BytePixmap {
                        base: source,
                        row_bytes: 10,
                        depth: 8,
                        bounds: [0, 0, 3, 8],
                    },
                    destination: BytePixmap {
                        base: 0x1000,
                        row_bytes: 10,
                        depth: 8,
                        bounds: [0, 0, 6, 8],
                    },
                    source_rect: [0, 0, 2, 6],
                    destination_rect: [1, 1, 3, 7],
                    clip: [0, 0, 6, 8],
                    palette,
                };
                assert_eq!(
                    copy.execute(&mut direct),
                    RowCopyOutcome::Completed,
                    "{context}"
                );
                // The per-pixel sequence `execute` used before the direct path.
                let mut pixels = vec![0u8; row_len * rows as usize];
                for row in 0..rows as usize {
                    oracle
                        .read_copy_row(
                            source + row as u32 * 10,
                            &mut pixels[row * row_len..][..row_len],
                        )
                        .unwrap();
                }
                let mut pixels: SavedPixels = pixels.into();
                for row in 0..rows as usize {
                    oracle.capture_copy_detail(
                        source + row as u32 * 10,
                        &mut pixels,
                        row * row_len,
                        row_len,
                    );
                }
                for row in 0..rows {
                    oracle
                        .write_copy_pixels(
                            destination(row),
                            &pixels,
                            row as usize * row_len,
                            row_len,
                            palette,
                        )
                        .expect("writable");
                }
                assert_eq!(
                    direct.read_bytes(0x1000, 60),
                    oracle.read_bytes(0x1000, 60),
                    "{context}: RAM"
                );
                assert_eq!(
                    direct.outline_presentation_rgb(),
                    oracle.outline_presentation_rgb(),
                    "{context}: rendered"
                );
                for row in 0..6 {
                    assert_eq!(
                        direct.save_pixel_bytes(0x1000 + row * 10, 10),
                        oracle.save_pixel_bytes(0x1000 + row * 10, 10),
                        "{context}: snapshot row {row}"
                    );
                }
                // A later plain store still clears copied text on both.
                direct.write_byte(destination(0) + 1, 5);
                oracle.write_byte(destination(0) + 1, 5);
                assert_eq!(
                    direct.outline_presentation_rgb(),
                    oracle.outline_presentation_rgb(),
                    "{context}: after store"
                );
            }
        }
    }

    #[test]
    fn detail_rows_copy_between_screen_and_offscreen_like_the_per_pixel_copy() {
        use crate::copy_bits::CopyBitsMemory;
        let inverted: [u8; 256] = std::array::from_fn(|i| (255 - i) as u8);
        let (row_len, rows) = (6usize, 3u32);
        let screen = |row: u32, x: u32| 0x1000 + row * 10 + x;
        let off = |row: u32, x: u32| 0x3_0000 + row * 10 + x;
        // (name, source row address, destination row address)
        type Row = fn(u32) -> u32;
        let cases: [(&str, Row, Row); 6] = [
            (
                "offscreen to screen",
                |r| 0x3_0000 + r * 10,
                |r| 0x1000 + (2 + r) * 10 + 1,
            ),
            (
                "screen down (overlap)",
                |r| 0x1000 + r * 10 + 1,
                |r| 0x1000 + (1 + r) * 10 + 2,
            ),
            (
                "screen up (overlap)",
                |r| 0x1000 + (2 + r) * 10,
                |r| 0x1000 + (1 + r) * 10,
            ),
            ("screen left", |r| 0x1000 + r * 10 + 2, |r| 0x1000 + r * 10),
            (
                "offscreen to offscreen (overlap)",
                |r| 0x3_0000 + r * 10,
                |r| 0x3_0000 + (1 + r) * 10 + 1,
            ),
            (
                "screen to offscreen",
                |r| 0x1000 + r * 10 + 1,
                |r| 0x3_0100 + r * 10,
            ),
        ];
        for (name, source, destination) in cases {
            for prior in ["plain", "text"] {
                for palette in [None, Some(&inverted)] {
                    let setup = || {
                        let mut bus = padded_bus(10, 8, 6, 2);
                        for row in 0..6 {
                            for x in 0..10 {
                                bus.write_byte(off(row, x), (row * 7 + x * 3) as u8);
                                bus.write_byte(0x3_0100 + row * 10 + x, (row + x) as u8);
                            }
                            for x in 0..8 {
                                bus.write_byte(screen(row, x), (row * 5 + x) as u8);
                            }
                        }
                        for row in 0..rows {
                            paint_detail(&mut bus, source(row) + 1);
                            paint_detail(&mut bus, source(row) + 4 - row.min(2));
                        }
                        // Text over text of another colour: blended ink.
                        bus.presentation.as_mut().unwrap().glyph = Some((
                            OutlineGlyph {
                                pixels: vec![64, 255, 0, 128].into(),
                                width: 2,
                                height: 2,
                                left: 0,
                                top: 0,
                            },
                            0,
                            0,
                        ));
                        bus.outline_glyph_pixel(source(0) + 1, 0, 0, 9);
                        bus.end_outline_glyph();
                        if prior == "text" {
                            paint_detail(&mut bus, destination(0) + 2);
                            paint_detail(&mut bus, destination(2) + 5);
                            paint_detail(&mut bus, off(5, 3));
                        }
                        bus
                    };
                    let context = format!("{name}, prior {prior}, palette {}", palette.is_some());
                    let mut direct = setup();
                    let mut oracle = setup();
                    let pairs: Vec<(u32, u32)> = (0..rows)
                        .map(|row| (source(row), destination(row)))
                        .collect();
                    let mut pixels = vec![0u8; row_len * rows as usize];
                    for (row, &(from, _)) in pairs.iter().enumerate() {
                        oracle
                            .read_copy_row(from, &mut pixels[row * row_len..][..row_len])
                            .unwrap();
                    }
                    assert!(
                        direct.copy_detail_rows(&pairs, &pixels, row_len, palette),
                        "{context}: applies"
                    );
                    // The per-pixel sequence `RowCopy::execute` falls back to.
                    let mut saved: SavedPixels = pixels.into();
                    for (row, &(from, _)) in pairs.iter().enumerate() {
                        oracle.capture_copy_detail(from, &mut saved, row * row_len, row_len);
                    }
                    for (row, &(_, to)) in pairs.iter().enumerate() {
                        oracle
                            .write_copy_pixels(to, &saved, row * row_len, row_len, palette)
                            .expect("writable");
                    }
                    for base in [0x1000u32, 0x3_0000, 0x3_0100] {
                        assert_eq!(
                            direct.read_bytes(base, 60),
                            oracle.read_bytes(base, 60),
                            "{context}: RAM {base:#x}"
                        );
                        for row in 0..6 {
                            assert_eq!(
                                direct.save_pixel_bytes(base + row * 10, 10),
                                oracle.save_pixel_bytes(base + row * 10, 10),
                                "{context}: detail {base:#x} row {row}"
                            );
                        }
                    }
                    assert_eq!(
                        direct.outline_presentation_rgb(),
                        oracle.outline_presentation_rgb(),
                        "{context}: rendered"
                    );
                    assert!(
                        direct.presentation.as_ref().unwrap().ink_mask_matches_ink(),
                        "{context}: ink mask"
                    );
                    // Later stores still clear copied text on both.
                    for to in [destination(0) + 1, destination(1) + 1] {
                        direct.write_byte(to, 5);
                        oracle.write_byte(to, 5);
                    }
                    assert_eq!(
                        direct.outline_presentation_rgb(),
                        oracle.outline_presentation_rgb(),
                        "{context}: after store"
                    );
                    assert_eq!(
                        direct.save_pixel_bytes(destination(0), 6),
                        oracle.save_pixel_bytes(destination(0), 6),
                        "{context}: after store detail"
                    );
                }
            }
        }
    }

    #[test]
    fn bulk_stores_over_text_match_byte_stores() {
        let setup = || {
            let mut bus = padded_bus(10, 8, 6, 2);
            for row in 0..6u32 {
                for x in 0..10 {
                    bus.write_byte(0x3_0000 + row * 10 + x, (row + x) as u8);
                }
            }
            for address in [
                0x1000 + 12,
                0x1000 + 15,
                0x1000 + 31,
                0x3_0000 + 2,
                0x3_0000 + 13,
            ] {
                paint_detail(&mut bus, address);
            }
            bus
        };
        // (store kind, address, length): screen rows with and without text,
        // a span past a row's visible width, offscreen spans with text.
        let spans = [
            (0x1000u32 + 10, 8usize),
            (0x1000 + 40, 8),
            (0x1000 + 14, 6),
            (0x1000 + 30, 3),
            (0x3_0000, 20),
            (0x3_0000 + 12, 3),
        ];
        for kind in ["write_bytes", "fill_bytes", "fill_zeros"] {
            for (address, len) in spans {
                let mut bulk = setup();
                let mut bytes = setup();
                let data: Vec<u8> = match kind {
                    "write_bytes" => (0..len).map(|i| (i * 11 + 3) as u8).collect(),
                    "fill_bytes" => vec![9; len],
                    _ => vec![0; len],
                };
                match kind {
                    "write_bytes" => bulk.write_bytes(address, &data),
                    "fill_bytes" => bulk.fill_bytes(address, len as u32, 9),
                    _ => bulk.fill_zeros(address, len as u32),
                }
                for (i, &value) in data.iter().enumerate() {
                    bytes.write_byte(address + i as u32, value);
                }
                let context = format!("{kind} at {address:#x} len {len}");
                for base in [0x1000u32, 0x3_0000] {
                    assert_eq!(
                        bulk.read_bytes(base, 60),
                        bytes.read_bytes(base, 60),
                        "{context}: RAM"
                    );
                    for row in 0..6 {
                        assert_eq!(
                            bulk.save_pixel_bytes(base + row * 10, 10),
                            bytes.save_pixel_bytes(base + row * 10, 10),
                            "{context}: detail {base:#x} row {row}"
                        );
                    }
                }
                assert_eq!(
                    bulk.outline_presentation_rgb(),
                    bytes.outline_presentation_rgb(),
                    "{context}: rendered"
                );
                assert!(
                    bulk.presentation.as_ref().unwrap().ink_mask_matches_ink(),
                    "{context}: ink mask"
                );
            }
        }
    }

    #[test]
    fn span_restores_match_byte_restores() {
        let setup = || {
            let mut bus = padded_bus(10, 8, 6, 2);
            for row in 0..6u32 {
                for x in 0..10 {
                    bus.write_byte(0x3_0000 + row * 10 + x, (row * 3 + x) as u8);
                }
            }
            for address in [0x1000 + 11, 0x1000 + 14, 0x3_0000 + 3, 0x3_0000 + 4] {
                paint_detail(&mut bus, address);
            }
            bus
        };
        for (address, len) in [(0x1000u32 + 10, 8usize), (0x3_0000, 10)] {
            let saver = setup();
            let saved = saver.save_pixel_bytes(address, len);
            // Change the span: new plain bytes, new text, cleared text.
            let disturb = |bus: &mut MacMemoryBus| {
                bus.write_bytes(address, &[7, 7, 7]);
                paint_detail(bus, address + 5);
                bus.write_byte(address + 4, 1);
            };
            let mut span = setup();
            let mut bytes = setup();
            disturb(&mut span);
            disturb(&mut bytes);
            span.restore_saved_pixels(address, &saved, 0, len);
            for i in 0..len {
                bytes.restore_saved_pixels(address + i as u32, &saved, i, 1);
            }
            let context = format!("at {address:#x}");
            assert_eq!(
                span.read_bytes(address, len),
                bytes.read_bytes(address, len),
                "{context}: RAM"
            );
            assert_eq!(
                span.save_pixel_bytes(address, len),
                bytes.save_pixel_bytes(address, len),
                "{context}: detail"
            );
            assert_eq!(
                span.save_pixel_bytes(address, len),
                saved,
                "{context}: restored"
            );
            assert_eq!(
                span.outline_presentation_rgb(),
                bytes.outline_presentation_rgb(),
                "{context}: rendered"
            );
        }
    }

    #[test]
    fn observed_block_moves_match_byte_copies() {
        let inverted: [u8; 256] = std::array::from_fn(|i| (255 - i) as u8);
        let setup = || {
            let mut bus = padded_bus(10, 8, 6, 2);
            for row in 0..6u32 {
                for x in 0..10 {
                    bus.write_byte(0x3_0000 + row * 10 + x, (row * 3 + x) as u8);
                }
            }
            for address in [
                0x1000 + 11,
                0x1000 + 13,
                0x3_0000 + 2,
                0x3_0000 + 5,
                0x3_0000 + 12,
            ] {
                paint_detail(&mut bus, address);
            }
            bus
        };
        // (source, destination, length): same screen row overlapping, an
        // offscreen overlap, offscreen to a screen row, and a span over two
        // screen rows (the byte copy).
        for (src, dst, len) in [
            (0x1000u32 + 10, 0x1000 + 12, 6u32),
            (0x3_0000, 0x3_0003, 12),
            (0x3_0001, 0x1000 + 31, 6),
            (0x1000 + 5, 0x1000 + 25, 12),
        ] {
            for map in [None, Some(&inverted)] {
                let mut moved = setup();
                let mut bytes = setup();
                match map {
                    None => assert!(moved.copy_ram_bytes(src, dst, len)),
                    Some(map) => assert!(moved.copy_mapped_ram_bytes(src, dst, len, map)),
                }
                let pixels = bytes.save_pixel_bytes(src, len as usize);
                for offset in 0..len {
                    bytes.copy_saved_pixel(dst + offset, &pixels, offset as usize, |index| {
                        map.map_or(index, |map| map[index as usize])
                    });
                }
                let context = format!("{src:#x} to {dst:#x} len {len} mapped {}", map.is_some());
                for base in [0x1000u32, 0x3_0000] {
                    assert_eq!(
                        moved.read_bytes(base, 60),
                        bytes.read_bytes(base, 60),
                        "{context}: RAM"
                    );
                    for row in 0..6 {
                        assert_eq!(
                            moved.save_pixel_bytes(base + row * 10, 10),
                            bytes.save_pixel_bytes(base + row * 10, 10),
                            "{context}: detail {base:#x} row {row}"
                        );
                    }
                }
                assert_eq!(
                    moved.outline_presentation_rgb(),
                    bytes.outline_presentation_rgb(),
                    "{context}: rendered"
                );
            }
        }
    }

    #[test]
    fn powerpc_rows_copy_like_the_per_pixel_copy() {
        use crate::copy_bits::CopyBitsMemory;
        use crate::memory::GuestAddressSpace;
        let inverted: [u8; 256] = std::array::from_fn(|i| (255 - i) as u8);
        let (row_len, rows) = (6usize, 3u32);
        // (source row, destination row): offscreen to screen, and a screen
        // scroll that overlaps itself.
        type Row = fn(u32) -> u32;
        let cases: [(&str, Row, Row); 2] = [
            (
                "offscreen to screen",
                |r| 0x3_0000 + r * 10,
                |r| 0x1000 + (2 + r) * 10 + 1,
            ),
            (
                "screen up (overlap)",
                |r| 0x1000 + (1 + r) * 10,
                |r| 0x1000 + r * 10,
            ),
        ];
        for (name, source, destination) in cases {
            for palette in [None, Some(&inverted)] {
                let setup = || {
                    let mut bus = padded_bus(10, 8, 6, 2);
                    for row in 0..rows {
                        paint_detail(&mut bus, source(row) + 1);
                        paint_detail(&mut bus, source(row) + 4 - row);
                    }
                    paint_detail(&mut bus, destination(0) + 3);
                    // The PowerPC memory holds the bytes the text was drawn on,
                    // as it would had the program drawn it there.
                    let mut memory = GuestAddressSpace::new();
                    memory.add_region(0x1000, bus.read_bytes(0x1000, 60));
                    memory.add_region(0x3_0000, bus.read_bytes(0x3_0000, 60));
                    memory
                        .shared_view()
                        .set_presentation(bus.presentation.clone());
                    (bus, memory)
                };
                let context = format!("{name}, palette {}", palette.is_some());
                let (direct_bus, mut direct) = setup();
                let (oracle_bus, mut oracle) = setup();
                let pairs: Vec<(u32, u32)> = (0..rows)
                    .map(|row| (source(row), destination(row)))
                    .collect();
                let mut pixels = vec![0u8; row_len * rows as usize];
                for (row, &(from, _)) in pairs.iter().enumerate() {
                    direct
                        .read_copy_row(from, &mut pixels[row * row_len..][..row_len])
                        .unwrap();
                }
                assert!(
                    direct.copy_rows_with_detail(&pairs, &pixels, row_len, palette),
                    "{context}: applies"
                );
                // The PowerPC fallback: capture every row, then write each.
                let mut saved: SavedPixels = pixels.clone().into();
                for (row, &(from, _)) in pairs.iter().enumerate() {
                    oracle.capture_copy_detail(from, &mut saved, row * row_len, row_len);
                }
                for (row, &(_, to)) in pairs.iter().enumerate() {
                    oracle
                        .write_copy_pixels(to, &saved, row * row_len, row_len, palette)
                        .expect("writable");
                }
                for base in [0x1000u32, 0x3_0000] {
                    let (mut a, mut b) = (vec![0u8; 60], vec![0u8; 60]);
                    direct.read_bytes_into(base, &mut a).unwrap();
                    oracle.read_bytes_into(base, &mut b).unwrap();
                    assert_eq!(a, b, "{context}: memory {base:#x}");
                    for row in 0..6 {
                        assert_eq!(
                            direct_bus.save_pixel_bytes(base + row * 10, 10).detail,
                            oracle_bus.save_pixel_bytes(base + row * 10, 10).detail,
                            "{context}: detail {base:#x} row {row}"
                        );
                    }
                }
                assert_eq!(
                    direct_bus.outline_presentation_rgb(),
                    oracle_bus.outline_presentation_rgb(),
                    "{context}: rendered"
                );
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

    #[test]
    fn snapshot_range_replacement_preserves_detail_outside_narrow_and_wide_draws() {
        let mut bus = bus();
        for offset in 0..16u32 {
            paint_detail(&mut bus, 0x1000 + offset);
        }
        paint_detail(&mut bus, 0x1000 + 127);
        let mut saved = bus.save_pixel_bytes(0x1000, 128);
        let capacity = saved.detail.capacity();
        assert!(capacity > 1 && capacity < 127);

        saved.replace_range(3, &[0x55]);
        assert_eq!(saved[3], 0x55);
        assert!(!saved.has_detail_at(3));
        for key in [0, 2, 4, 15, 127] {
            assert!(saved.has_detail_at(key), "narrow draw lost detail at {key}");
        }

        saved.replace_range(0, &vec![0x77; capacity]);
        assert!(saved.detail.keys().all(|&key| key >= capacity));
        assert!(saved.has_detail_at(127), "wide draw lost outside detail");
        assert_eq!(saved[0], 0x77);
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
                pixels: vec![255].into(),
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
            .insert(0x1000, &stale);
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
        assert!(bus
            .presentation
            .as_ref()
            .unwrap()
            .ink_mask
            .iter()
            .all(|&mask| mask == 0));
        assert!(bus.presentation.as_ref().unwrap().ink_mask_matches_ink());
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
                pixels: vec![0; 4].into(),
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
                pixels: vec![128, 255, 0, 0].into(),
                width: 4,
                height: 1,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        p.glyph_pixel(0x1000, 0, 0, 0, 255);
        let glyph = &mut p.glyph.as_mut().unwrap().0;
        let mut pixels = glyph.pixels.to_vec();
        pixels[1] = 0;
        glyph.pixels = pixels.into();
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
                pixels: vec![128; 8].into(),
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
                pixels: vec![128; 4].into(),
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
                        pixels: vec![128; 16].into(),
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
