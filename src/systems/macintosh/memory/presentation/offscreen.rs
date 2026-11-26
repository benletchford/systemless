//! Retained text detail for guest memory outside the framebuffer.
//!
//! Offscreen pixmaps (GWorlds, save-behind buffers) hold the same per-pixel
//! detail cells as the screen, but at arbitrary addresses. Cells are stored
//! by content in fixed 256-byte address chunks, each keeping its cells'
//! values and sample indices contiguously, a text bitmask and sparse ink.
//! A pixmap row is a contiguous address span, so erasing, capturing and
//! copying text rows walk chunks and bitmasks instead of one heap object per
//! pixel, and drawing a glyph pixel rewrites array slots instead of
//! allocating a cell.
//!
//! The API keeps the shape of the address-keyed map it replaced. A cell is
//! handed out as an `Arc<DetailCell>` built on first request and cached
//! until the cell changes, so snapshots of unchanged text still share one
//! immutable cell, and a cell stored from a snapshot is returned as that
//! same `Arc`.

use super::ink::{CellInk, ComplexInk, InkView, InkViewMut};
use super::{DetailCell, IndexedColor, Ink, SampleOffsetHasher, TILE_SAMPLES};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasherDefault;
use std::sync::Arc;

const CHUNK_SHIFT: u32 = 8;
const CHUNK_BYTES: usize = 1 << CHUNK_SHIFT;
const MASK_WORDS: usize = CHUNK_BYTES / 64;

type CellInkMap = HashMap<usize, Ink, BuildHasherDefault<SampleOffsetHasher>>;

/// A chunk emptied of cells is kept for reuse: text drawn, erased and
/// drawn again at the same addresses (a menu's save-behind buffer, a text
/// crawl) would otherwise free and re-zero it every time.
struct Chunk {
    /// Bit `slot` is set when the slot holds a cell.
    present: [u64; MASK_WORDS],
    /// Bit `slot` is set when the slot may hold ink. It is set whenever ink
    /// is added and cleared only with the slot, so an unset bit proves the
    /// slot has none and lets clearing and lookups skip the ink list.
    inked: [u64; MASK_WORDS],
    values: [u8; CHUNK_BYTES],
    /// Sample count of each cell (`scale²` of the presentation that made it).
    lens: [u8; CHUNK_BYTES],
    /// `TILE_SAMPLES` indices per slot, of which the first `lens[slot]` count.
    indices: Box<[u8; CHUNK_BYTES * TILE_SAMPLES]>,
    /// Partial-coverage ink per slot, each list sorted by sample. Empty
    /// until the chunk first holds ink; a cleared slot keeps its capacity.
    /// Ink blocks for the slots that have ink (bit set in `inked`), found
    /// through `ink_at`; blocks of cleared slots are reused. A chunk pays
    /// only for its inked slots, not a block per slot.
    inks: Vec<CellInk>,
    ink_at: [u8; CHUNK_BYTES],
    free_inks: Vec<u8>,
    /// Ink that does not pack (a blended background), by (slot, sample).
    complex: ComplexInk,
    /// Cells already handed out as `Arc`s and unchanged since.
    shared: RefCell<Vec<Option<Arc<DetailCell>>>>,
    /// The text run that last drew each slot's cell (0: none).
    runs: [u32; CHUNK_BYTES],
    /// The run each slot's cell belonged to when a guest store after that
    /// run was damaged wrote its byte again unchanged (0: none). The mark
    /// counts only while `runs` still names the same run.
    rewritten: [u32; CHUNK_BYTES],
    /// The `next_generation` value of the chunk's last change.
    generation: u64,
    count: usize,
}

impl Chunk {
    fn new() -> Self {
        Self {
            present: [0; MASK_WORDS],
            inked: [0; MASK_WORDS],
            values: [0; CHUNK_BYTES],
            lens: [0; CHUNK_BYTES],
            indices: Box::new([0; CHUNK_BYTES * TILE_SAMPLES]),
            inks: Vec::new(),
            ink_at: [0; CHUNK_BYTES],
            free_inks: Vec::new(),
            complex: ComplexInk::new(),
            shared: RefCell::new(Vec::new()),
            runs: [0; CHUNK_BYTES],
            rewritten: [0; CHUNK_BYTES],
            generation: next_generation(),
            count: 0,
        }
    }

    fn has(&self, slot: usize) -> bool {
        self.present[slot / 64] & (1 << (slot % 64)) != 0
    }

    /// Whether `slot`'s cell, of `run`, has had its byte written again since
    /// `run` was damaged.
    fn rewritten(&self, slot: usize, run: u32) -> bool {
        self.rewritten[slot] == run
    }

    /// Give an empty `slot` a cell of `len` samples of `background` with no
    /// ink, reporting whether it was empty.
    #[inline]
    fn insert_blank(&mut self, slot: usize, background: u8, len: usize) -> bool {
        if self.has(slot) {
            return false;
        }
        assert!(len <= TILE_SAMPLES, "cell samples exceed a tile");
        self.present[slot / 64] |= 1 << (slot % 64);
        self.count += 1;
        self.values[slot] = background;
        self.lens[slot] = len as u8;
        self.indices[slot * TILE_SAMPLES..slot * TILE_SAMPLES + len].fill(background);
        self.forget_shared(slot);
        true
    }

    /// Whether `slot`'s cell shows only its own byte: no sample or ink of
    /// another colour, so presenting it looks like the plain pixel.
    fn is_blank(&self, slot: usize) -> bool {
        let value = self.values[slot];
        let start = slot * TILE_SAMPLES;
        self.indices[start..start + usize::from(self.lens[slot])]
            .iter()
            .all(|&index| index == value)
            && (!self.may_have_ink(slot)
                || self.slot_ink(slot).iter().all(|(_, ink)| {
                    ink.foreground == value
                        && matches!(ink.background, super::IndexedColor::Solid(background) if background == value)
                }))
    }

    /// Call `visit(colour, own)` for the cell's byte (`own`) and for each
    /// other colour its samples or ink show (repeats included).
    fn visit_colours(&self, slot: usize, mut visit: impl FnMut(u8, bool)) {
        let value = self.values[slot];
        visit(value, true);
        let start = slot * TILE_SAMPLES;
        for &index in &self.indices[start..start + usize::from(self.lens[slot])] {
            if index != value {
                visit(index, false);
            }
        }
        if self.may_have_ink(slot) {
            for (_, ink) in self.slot_ink(slot).iter() {
                if ink.foreground != value {
                    visit(ink.foreground, false);
                }
                if let super::IndexedColor::Solid(background) = ink.background {
                    if background != value {
                        visit(background, false);
                    }
                }
            }
        }
    }

    fn may_have_ink(&self, slot: usize) -> bool {
        self.inked[slot / 64] & (1 << (slot % 64)) != 0
    }

    /// The slot's ink, sorted by sample.
    fn slot_ink(&self, slot: usize) -> InkView<'_> {
        static EMPTY: CellInk = CellInk::EMPTY;
        if self.may_have_ink(slot) {
            InkView::new(&self.inks[usize::from(self.ink_at[slot])], &self.complex, slot as u32)
        } else {
            InkView::new(&EMPTY, &self.complex, slot as u32)
        }
    }

    /// The slot's ink for changes, marking the slot as inked.
    fn slot_ink_mut(&mut self, slot: usize) -> InkViewMut<'_> {
        self.generation = next_generation();
        if !self.may_have_ink(slot) {
            let at = match self.free_inks.pop() {
                Some(at) => at,
                None => {
                    self.inks.push(CellInk::default());
                    (self.inks.len() - 1) as u8
                }
            };
            self.ink_at[slot] = at;
            self.inked[slot / 64] |= 1 << (slot % 64);
        }
        let at = usize::from(self.ink_at[slot]);
        InkViewMut::new(&mut self.inks[at], &mut self.complex, slot as u32)
    }

    /// Clear an inked slot's ink and give its block back.
    fn release_ink(&mut self, slot: usize) {
        let at = self.ink_at[slot];
        InkViewMut::new(&mut self.inks[usize::from(at)], &mut self.complex, slot as u32).clear();
        self.free_inks.push(at);
        self.inked[slot / 64] &= !(1 << (slot % 64));
    }

    fn forget_shared(&mut self, slot: usize) {
        self.generation = next_generation();
        if let Some(cached) = self.shared.get_mut().get_mut(slot) {
            *cached = None;
        }
    }

    fn cell(&self, slot: usize) -> Arc<DetailCell> {
        if let Some(Some(cell)) = self.shared.borrow().get(slot) {
            return cell.clone();
        }
        let len = usize::from(self.lens[slot]);
        let mut ink = CellInkMap::default();
        for (sample, value) in self.slot_ink(slot).iter() {
            ink.insert(sample, value);
        }
        let cell = Arc::new(DetailCell {
            value: self.values[slot],
            indices: self.indices[slot * TILE_SAMPLES..slot * TILE_SAMPLES + len].to_vec(),
            ink,
            dimmed: false,
        });
        let mut shared = self.shared.borrow_mut();
        if shared.is_empty() {
            shared.resize(CHUNK_BYTES, None);
        }
        shared[slot] = Some(cell.clone());
        cell
    }

    fn matches(&self, slot: usize, cell: &DetailCell) -> bool {
        let len = usize::from(self.lens[slot]);
        if self.values[slot] != cell.value
            || len != cell.indices.len()
            || self.indices[slot * TILE_SAMPLES..slot * TILE_SAMPLES + len] != cell.indices[..]
        {
            return false;
        }
        let held = self.slot_ink(slot);
        held.len() == cell.ink.len()
            && held.iter().all(|(sample, ink)| cell.ink.get(&sample) == Some(&ink))
    }

    /// Store `cell` in `slot`, which must not hold one. `shared` is the
    /// `Arc` to hand out for it, if the caller has one.
    fn store(&mut self, slot: usize, cell: &DetailCell, shared: Option<&Arc<DetailCell>>) {
        self.generation = next_generation();
        assert!(cell.indices.len() <= TILE_SAMPLES, "cell samples exceed a tile");
        self.present[slot / 64] |= 1 << (slot % 64);
        self.count += 1;
        self.values[slot] = cell.value;
        self.lens[slot] = cell.indices.len() as u8;
        self.indices[slot * TILE_SAMPLES..slot * TILE_SAMPLES + cell.indices.len()]
            .copy_from_slice(&cell.indices);
        if !cell.ink.is_empty() {
            let mut held = self.slot_ink_mut(slot);
            held.clear();
            for (&sample, ink) in &cell.ink {
                held.set(sample, ink.clone());
            }
        }
        match shared {
            Some(cell) => {
                let shared = self.shared.get_mut();
                if shared.is_empty() {
                    shared.resize(CHUNK_BYTES, None);
                }
                shared[slot] = Some(cell.clone());
            }
            None => self.forget_shared(slot),
        }
    }

    fn clear(&mut self, slot: usize) {
        self.present[slot / 64] &= !(1 << (slot % 64));
        self.rewritten[slot] = 0;
        self.count -= 1;
        if self.may_have_ink(slot) {
            self.release_ink(slot);
        }
        self.forget_shared(slot);
    }

    /// Clear every occupied slot in `[first, end)`, returning how many.
    fn clear_span(&mut self, first: usize, end: usize) -> usize {
        let mut cleared = 0;
        for word in first / 64..end.div_ceil(64) {
            let low = if word == first / 64 { first % 64 } else { 0 };
            let high = if word == (end - 1) / 64 { (end - 1) % 64 + 1 } else { 64 };
            let mask = (u64::MAX >> (64 - (high - low))) << low;
            let hit = self.present[word] & mask;
            if hit == 0 {
                continue;
            }
            cleared += hit.count_ones() as usize;
            self.present[word] &= !hit;
            let mut bits = hit;
            while bits != 0 {
                self.rewritten[word * 64 + bits.trailing_zeros() as usize] = 0;
                bits &= bits - 1;
            }
            let mut inked = self.inked[word] & hit;
            while inked != 0 {
                let slot = word * 64 + inked.trailing_zeros() as usize;
                self.release_ink(slot);
                inked &= inked - 1;
            }
            if let Some(shared) = self.shared.get_mut().get_mut(word * 64..word * 64 + 64) {
                let mut bits = hit;
                while bits != 0 {
                    shared[bits.trailing_zeros() as usize] = None;
                    bits &= bits - 1;
                }
            }
        }
        if cleared > 0 {
            self.generation = next_generation();
        }
        self.count -= cleared;
        cleared
    }
}

/// Move an ink out, leaving a placeholder that allocates nothing.
pub(super) fn take_ink(ink: &mut Ink) -> Ink {
    std::mem::replace(
        ink,
        Ink { foreground: 0, alpha: 0, background: super::IndexedColor::Solid(0) },
    )
}

/// Read access to one offscreen cell's contents, without building an `Arc`.
pub(super) struct OffscreenCellRef<'a> {
    chunk: &'a Chunk,
    slot: usize,
}

impl OffscreenCellRef<'_> {
    /// The cell's sample indices.
    pub(super) fn indices(&self) -> &[u8] {
        let start = self.slot * TILE_SAMPLES;
        &self.chunk.indices[start..start + usize::from(self.chunk.lens[self.slot])]
    }

    /// The cell's ink.
    pub(super) fn ink(&self) -> InkView<'_> {
        self.chunk.slot_ink(self.slot)
    }

    /// The cell's ink as `(sample, ink)` in sample order.
    #[cfg(test)]
    pub(super) fn inks(&self) -> impl Iterator<Item = (usize, Ink)> + '_ {
        self.chunk.slot_ink(self.slot).iter()
    }
}

/// Mutable access to one offscreen cell. Any change forgets the cell's
/// shared `Arc`, so later reads see the new contents.
pub(super) struct OffscreenCellMut<'a> {
    chunk: &'a mut Chunk,
    slot: usize,
}

impl OffscreenCellMut<'_> {
    pub(super) fn len(&self) -> usize {
        usize::from(self.chunk.lens[self.slot])
    }

    pub(super) fn set_value(&mut self, value: u8) {
        self.chunk.forget_shared(self.slot);
        self.chunk.values[self.slot] = value;
    }

    #[cfg(test)]
    pub(super) fn index(&self, sample: usize) -> u8 {
        assert!(sample < self.len());
        self.chunk.indices[self.slot * TILE_SAMPLES + sample]
    }

    pub(super) fn set_index(&mut self, sample: usize, index: u8) {
        assert!(sample < self.len());
        self.chunk.forget_shared(self.slot);
        self.chunk.indices[self.slot * TILE_SAMPLES + sample] = index;
    }

    /// Paint glyph coverage `alphas` (one per sample; 0 leaves a sample,
    /// 255 sets it to `foreground`, anything else inks it) exactly as one
    /// `set_index`/`remove_ink` or `update_ink` per covered sample would.
    pub(super) fn paint_glyph(&mut self, alphas: &[u8], foreground: u8) {
        let base = self.slot * TILE_SAMPLES;
        let (mut full, mut partial) = (0u16, 0u16);
        for (i, &alpha) in alphas.iter().enumerate() {
            match alpha {
                0 => {}
                255 => full |= 1 << i,
                _ => partial |= 1 << i,
            }
        }
        if full | partial == 0 {
            return;
        }
        assert!(alphas.len() <= self.len());
        self.chunk.forget_shared(self.slot);
        let mut bits = full;
        while bits != 0 {
            let i = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            self.chunk.indices[base + i] = foreground;
        }
        if !self.chunk.may_have_ink(self.slot) {
            // A cell without ink (text drawn onto an erased buffer): each
            // partly covered sample's ink is the foreground at its coverage
            // over the sample's current index, exactly what `update` would
            // build from nothing.
            if partial != 0 {
                let indices = &self.chunk.indices[base..base + TILE_SAMPLES];
                let block = CellInk::painted(partial, foreground, alphas, indices);
                self.chunk.slot_ink_mut(self.slot).assign(&block);
            }
            return;
        }
        let ink_to_clear = self.chunk.may_have_ink(self.slot) && full != 0;
        if !ink_to_clear && partial == 0 {
            return;
        }
        let indices = &self.chunk.indices[base..base + TILE_SAMPLES];
        let backgrounds: [u8; TILE_SAMPLES] = std::array::from_fn(|i| indices[i]);
        let mut held = if partial != 0 || self.chunk.may_have_ink(self.slot) {
            self.chunk.slot_ink_mut(self.slot)
        } else {
            return;
        };
        let mut bits = full & held.mask();
        while bits != 0 {
            let i = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            held.remove(i);
        }
        let mut bits = partial;
        while bits != 0 {
            let i = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            let alpha = u32::from(alphas[i]);
            held.update(
                i,
                || Ink { foreground, alpha: 0, background: IndexedColor::Solid(backgrounds[i]) },
                |ink| {
                    if ink.foreground != foreground {
                        let previous = ink.clone();
                        *ink = Ink {
                            foreground,
                            alpha: 0,
                            background: previous.background.over(previous.foreground, previous.alpha),
                        };
                    }
                    ink.alpha = ink.alpha.max(alpha);
                },
            );
        }
    }

    pub(super) fn remove_ink(&mut self, sample: usize) {
        if !self.chunk.may_have_ink(self.slot) {
            return;
        }
        let removed = {
            let mut held = self.chunk.slot_ink_mut(self.slot);
            let had = held.mask() & (1 << sample) != 0;
            if had {
                held.remove(sample);
            }
            had
        };
        if removed {
            self.chunk.forget_shared(self.slot);
        }
    }

    /// Apply `change` to the ink of `sample`, starting from `ink()` when it
    /// has none.
    #[cfg(test)]
    pub(super) fn update_ink(&mut self, sample: usize, ink: impl FnOnce() -> Ink, change: impl FnOnce(&mut Ink)) {
        self.chunk.forget_shared(self.slot);
        self.chunk.slot_ink_mut(self.slot).update(sample, ink, change);
    }
}

#[derive(Default)]
pub(super) struct OffscreenDetail {
    chunks: BTreeMap<u32, Box<Chunk>>,
    count: usize,
    /// The text run now drawing (0: none). Cells it creates or changes are
    /// tagged with it, so that they can later be dropped together.
    run: u32,
    /// The last run number handed out (`begin_run`). It lives with the
    /// cells, so a presentation that takes them over keeps counting.
    last_run: u32,
    /// The first and last address the current run has reached so far, kept
    /// here while it draws and moved into `run_bounds` when it changes, so
    /// that drawing a cell costs no map lookup.
    reach: Option<(u32, u32)>,
    /// The first and last address of each tagged run's cells. Entries of
    /// runs whose cells are all gone linger until the map reaches
    /// `run_bounds_rebuild_at.max(MAX_RUNS)`, when it is rebuilt from the
    /// cells' tags.
    run_bounds: HashMap<u32, (u32, u32)>,
    /// The runs guest stores have changed pixels of.
    damage: HashMap<u32, RunDamage>,
    /// Next `run_bounds.len()` threshold that triggers a rebuild. Doubles the
    /// surviving live count when live runs stay above `MAX_RUNS` so rebuilds
    /// remain amortized O(1) per run.
    run_bounds_rebuild_at: usize,
    #[cfg(test)]
    run_bounds_rebuilds: usize,
    /// One bit per 64 KiB page of the address space: set once a chunk is
    /// made in the page and never cleared, so a clear bit proves the page
    /// holds no cell. Most copies read and write pages that never held
    /// detail; this lets them skip the chunk map entirely.
    pages: Vec<u64>,
    /// While set, the cells that run damage removes, by address (see
    /// `keep_run_removals`).
    kept: Option<BTreeMap<u32, Arc<DetailCell>>>,
}

/// What guest stores have changed of a run.
#[derive(Clone, Copy)]
struct RunDamage {
    /// First and last changed bytes, and the outside edges of the stores
    /// that reached them. Addresses between separate stores can remain live.
    span: (u32, u32),
    store_edges: (u32, u32),
    /// Two of its remaining cells that show it still holds text (see
    /// `text_witness`), checked again before scanning for others.
    witness: (u32, u32),
}

fn split(address: u32) -> (u32, usize) {
    (address >> CHUNK_SHIFT, (address as usize) & (CHUNK_BYTES - 1))
}

/// A process-wide counter for chunk changes: a chunk whose `generation` is
/// at most a value read earlier has not changed since.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_generation() -> u64 {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// The latest generation handed out.
pub(super) fn current_generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

const PAGE_SHIFT: u32 = 16;
const MAX_RUNS: usize = 1 << 16;
const PAGE_WORDS: usize = 1 << (32 - PAGE_SHIFT - 6);

impl OffscreenDetail {
    /// Note that the chunk `key` (an address shifted by `CHUNK_SHIFT`)
    /// exists, before it is made.
    fn mark_page(&mut self, key: u32) {
        let page = (key >> (PAGE_SHIFT - CHUNK_SHIFT)) as usize;
        if self.pages.is_empty() {
            self.pages = vec![0; PAGE_WORDS];
        }
        self.pages[page / 64] |= 1 << (page % 64);
    }

    /// Whether `[from, last]` may hold a cell: false only when no chunk was
    /// ever made in any of its pages.
    fn may_hold_cells(&self, from: u32, last: u32) -> bool {
        if self.pages.is_empty() {
            return false;
        }
        (from >> PAGE_SHIFT..=last >> PAGE_SHIFT)
            .any(|page| self.pages[page as usize / 64] & (1 << (page % 64)) != 0)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.count
    }

    /// Tag the cells created or changed from now on with text run `run`
    /// (0: none), returning the previous run.
    pub(super) fn set_run(&mut self, run: u32) -> u32 {
        self.settle_run();
        std::mem::replace(&mut self.run, run)
    }

    /// Start a new run, returning the one to restore when it ends.
    pub(super) fn begin_run(&mut self) -> u32 {
        self.last_run = self.last_run.checked_add(1).unwrap_or(1);
        self.set_run(self.last_run)
    }

    /// Tag the cell at `address`, if any, with the current run: a run that
    /// redraws a cell exactly as it was still owns it.
    pub(super) fn retag(&mut self, address: u32) {
        let run = self.run;
        if run == 0 {
            return;
        }
        let (key, slot) = split(address);
        let Some(chunk) = self.chunks.get_mut(&key).filter(|chunk| chunk.has(slot)) else {
            return;
        };
        chunk.runs[slot] = run;
        self.note_run(address, address);
    }

    /// Record that the current run reaches `[from, last]`.
    #[inline]
    fn note_run(&mut self, from: u32, last: u32) {
        if self.run != 0 {
            self.reach = Some(self.reach.map_or((from, last), |(lo, hi)| (lo.min(from), hi.max(last))));
        }
    }

    /// Move the current run's reach into `run_bounds`.
    fn settle_run(&mut self) {
        let Some((from, last)) = self.reach.take() else {
            return;
        };
        let threshold = self.run_bounds_rebuild_at.max(MAX_RUNS);
        if self.run_bounds.len() >= threshold && !self.run_bounds.contains_key(&self.run) {
            self.rebuild_run_bounds();
            self.run_bounds_rebuild_at = self.run_bounds.len().saturating_mul(2).max(MAX_RUNS);
        }
        let bounds = self.run_bounds.entry(self.run).or_insert((from, last));
        *bounds = (bounds.0.min(from), bounds.1.max(last));
    }

    /// Recompute `run_bounds` from the tags of the cells that remain, which
    /// drops the entries of runs that have no cells left.
    fn rebuild_run_bounds(&mut self) {
        #[cfg(test)]
        {
            self.run_bounds_rebuilds += 1;
        }
        self.run_bounds.clear();
        for (&key, chunk) in &self.chunks {
            for slot in 0..CHUNK_BYTES {
                let run = chunk.runs[slot];
                if run != 0 && chunk.has(slot) {
                    let address = (key << CHUNK_SHIFT) | slot as u32;
                    let bounds = self.run_bounds.entry(run).or_insert((address, address));
                    *bounds = (bounds.0.min(address), bounds.1.max(address));
                }
            }
        }
        let run_bounds = &self.run_bounds;
        self.damage.retain(|run, _| run_bounds.contains_key(run));
    }

    /// Whether a cell in `[from, from + len)` may have changed since
    /// `generation` (a `current_generation` value).
    pub(super) fn changed_since(&self, from: u32, len: usize, generation: u64) -> bool {
        let Some(last) = len
            .checked_sub(1)
            .and_then(|n| from.checked_add(n as u32))
        else {
            return len != 0;
        };
        self.chunks
            .range(from >> CHUNK_SHIFT..=last >> CHUNK_SHIFT)
            .any(|(_, chunk)| chunk.generation > generation)
    }

    /// Remove the current run's cells that show only their own byte. Drawing
    /// keeps such cells while the run lasts (a later glyph's stores may land
    /// on them); afterwards they describe nothing, and every copy and sync
    /// would still carry them.
    pub(super) fn drop_blank_run_cells(&mut self) -> usize {
        let (run, Some((from, last))) = (self.run, self.reach) else {
            return 0;
        };
        let mut removed = 0;
        for (_, chunk) in self.chunks.range_mut(from >> CHUNK_SHIFT..=last >> CHUNK_SHIFT) {
            for slot in 0..CHUNK_BYTES {
                if chunk.runs[slot] == run && chunk.has(slot) && chunk.is_blank(slot) {
                    chunk.clear(slot);
                    removed += 1;
                }
            }
        }
        self.count -= removed;
        removed
    }

    /// Remove `run`'s cells in `[from, to)`.
    fn remove_run_cells(&mut self, run: u32, from: u32, to: u32) {
        if from >= to {
            return;
        }
        let mut removed = 0;
        for (&key, chunk) in self
            .chunks
            .range_mut(from >> CHUNK_SHIFT..=(to - 1) >> CHUNK_SHIFT)
        {
            let base = key << CHUNK_SHIFT;
            let first = from.saturating_sub(base).min(CHUNK_BYTES as u32) as usize;
            let end = (to - base).min(CHUNK_BYTES as u32) as usize;
            for slot in first..end {
                if chunk.runs[slot] == run && chunk.has(slot) {
                    if let Some(kept) = self.kept.as_mut() {
                        kept.insert(base + slot as u32, chunk.cell(slot));
                    }
                    chunk.clear(slot);
                    removed += 1;
                }
            }
        }
        self.count -= removed;
    }

    /// Whether `address` holds a cell of `run`.
    fn holds_run_cell(&self, address: u32, run: u32) -> bool {
        let (key, slot) = split(address);
        self.chunks
            .get(&key)
            .is_some_and(|chunk| chunk.has(slot) && chunk.runs[slot] == run)
    }

    /// Whether `address` holds a cell of `run` that no store has written
    /// over since `run` was damaged.
    fn holds_untouched_run_cell(&self, address: u32, run: u32) -> bool {
        let (key, slot) = split(address);
        self.chunks.get(&key).is_some_and(|chunk| {
            chunk.has(slot) && chunk.runs[slot] == run && !chunk.rewritten(slot, run)
        })
    }

    /// Two remaining cells of `run` showing it still holds text: the first's
    /// byte is a colour the second shows only in its samples. Text over a
    /// background leaves cells whose byte is the ink with background
    /// samples, and cells whose byte is the background with ink samples.
    /// Erasing it to the background changes every ink byte; filling it with
    /// the ink changes every background byte. Either way the cells left all
    /// show the same side, and would draw only the old glyphs' edges.
    /// Only cells no store has written over since the run was damaged
    /// count: a new picture drawn over the text in both colours keeps ink
    /// bytes where it is the ink colour and background bytes where it is
    /// the background colour, which together look like text, but it wrote
    /// every one of them.
    /// The scan starts from the end of the run farther from `changed`, where
    /// a store sweeping across the run reaches last.
    fn text_witness(&mut self, run: u32, changed: u32) -> Option<(u32, u32)> {
        self.settle_run();
        let &(from, last) = self.run_bounds.get(&run)?;
        let backward = changed.saturating_sub(from) <= last.saturating_sub(changed);
        let mut own_at = [None::<u32>; 256];
        let mut other_at = [None::<u32>; 256];
        let mut visit = |address: u32, chunk: &Chunk, slot: usize| {
            if chunk.runs[slot] != run || chunk.rewritten(slot, run) {
                return None;
            }
            let mut witness = None;
            chunk.visit_colours(slot, |colour, own| {
                let (mine, theirs) = if own {
                    (&mut own_at, &other_at)
                } else {
                    (&mut other_at, &own_at)
                };
                if witness.is_none() {
                    if let Some(held) = theirs[usize::from(colour)].filter(|&held| held != address)
                    {
                        witness = Some(if own {
                            (address, held)
                        } else {
                            (held, address)
                        });
                    }
                }
                mine[usize::from(colour)].get_or_insert(address);
            });
            witness
        };
        let chunks = self.chunks.range(from >> CHUNK_SHIFT..=last >> CHUNK_SHIFT);
        if backward {
            for (&key, chunk) in chunks.rev() {
                for word in (0..MASK_WORDS).rev() {
                    let mut bits = chunk.present[word];
                    while bits != 0 {
                        let bit = 63 - bits.leading_zeros() as usize;
                        bits &= !(1 << bit);
                        let slot = word * 64 + bit;
                        if let Some(witness) =
                            visit((key << CHUNK_SHIFT) | slot as u32, chunk, slot)
                        {
                            return Some(witness);
                        }
                    }
                }
            }
        } else {
            for (&key, chunk) in chunks {
                for word in 0..MASK_WORDS {
                    let mut bits = chunk.present[word];
                    while bits != 0 {
                        let bit = bits.trailing_zeros() as usize;
                        bits &= bits - 1;
                        let slot = word * 64 + bit;
                        if let Some(witness) =
                            visit((key << CHUNK_SHIFT) | slot as u32, chunk, slot)
                        {
                            return Some(witness);
                        }
                    }
                }
            }
        }
        None
    }

    /// Remove every remaining cell of each run in `runs`, returning how many.
    fn remove_runs(&mut self, runs: &[u32]) -> usize {
        self.settle_run();
        let mut removed = 0;
        for &run in runs {
            let Some((from, last)) = self.run_bounds.remove(&run) else {
                continue;
            };
            for (&key, chunk) in self
                .chunks
                .range_mut(from >> CHUNK_SHIFT..=last >> CHUNK_SHIFT)
            {
                for slot in 0..CHUNK_BYTES {
                    if chunk.runs[slot] == run && chunk.has(slot) {
                        if let Some(kept) = self.kept.as_mut() {
                            kept.insert((key << CHUNK_SHIFT) + slot as u32, chunk.cell(slot));
                        }
                        chunk.clear(slot);
                        removed += 1;
                    }
                }
            }
        }
        self.count -= removed;
        removed
    }

    /// Start or stop keeping the cells that run damage removes. A guest
    /// recoloring text in place changes the pixels of a run before it has
    /// rewritten all of them, and the cells it has yet to reach must still
    /// be found when it does. Stopping forgets what was kept, returning the
    /// addresses it held.
    pub(super) fn keep_run_removals(&mut self, keep: bool) -> Vec<u32> {
        match (keep, self.kept.is_some()) {
            (true, false) => self.kept = Some(BTreeMap::new()),
            (false, true) => return self.kept.take().unwrap().into_keys().collect(),
            _ => {}
        }
        Vec::new()
    }

    /// Whether a cell in `[from, to)` is kept.
    pub(super) fn keeps_any_in(&self, from: u32, to: u64) -> bool {
        let to = to.min(1 << 32);
        u64::from(from) < to
            && self
                .kept
                .as_ref()
                .is_some_and(|kept| kept.range(from..=(to - 1) as u32).next().is_some())
    }

    /// Take the kept cells in `[from, to)` that no cell has replaced since.
    pub(super) fn take_kept(&mut self, from: u32, to: u64) -> Vec<(u32, Arc<DetailCell>)> {
        let Some(kept) = self.kept.as_mut().filter(|kept| !kept.is_empty()) else {
            return Vec::new();
        };
        let to = to.min(1 << 32);
        if u64::from(from) >= to {
            return Vec::new();
        }
        let addresses: Vec<u32> = kept
            .range(from..=(to - 1) as u32)
            .map(|(&a, _)| a)
            .collect();
        let mut taken = Vec::with_capacity(addresses.len());
        for address in addresses {
            let cell = kept.remove(&address).unwrap();
            let (key, slot) = split(address);
            if !self.chunks.get(&key).is_some_and(|chunk| chunk.has(slot)) {
                taken.push((address, cell));
            }
        }
        taken
    }

    pub(super) fn contains(&self, address: u32) -> bool {
        let (chunk, slot) = split(address);
        self.chunks.get(&chunk).is_some_and(|chunk| chunk.has(slot))
    }

    pub(super) fn get(&self, address: u32) -> Option<Arc<DetailCell>> {
        let (chunk, slot) = split(address);
        let chunk = self.chunks.get(&chunk)?;
        chunk.has(slot).then(|| chunk.cell(slot))
    }

    /// Whether `address` holds exactly `cell` (or, for `None`, nothing),
    /// compared by contents as the replaced map compared its `Arc`s.
    pub(super) fn matches(&self, address: u32, cell: Option<&DetailCell>) -> bool {
        let (chunk, slot) = split(address);
        let held = self.chunks.get(&chunk).filter(|chunk| chunk.has(slot));
        match (held, cell) {
            (None, None) => true,
            (Some(chunk), Some(cell)) => chunk.matches(slot, cell),
            _ => false,
        }
    }

    /// Store `cell` at `address`, replacing any cell there, and hand out this
    /// same `Arc` for it until it changes.
    pub(super) fn insert(&mut self, address: u32, cell: &Arc<DetailCell>) {
        let (chunk, slot) = split(address);
        self.mark_page(chunk);
        let run = self.run;
        let chunk = self.chunks.entry(chunk).or_insert_with(|| Box::new(Chunk::new()));
        if chunk.has(slot) {
            chunk.clear(slot);
        } else {
            self.count += 1;
        }
        chunk.store(slot, cell, Some(cell));
        chunk.runs[slot] = run;
        self.note_run(address, address);
    }

    /// Remove the cell at `address`, reporting whether there was one.
    pub(super) fn remove(&mut self, address: u32) -> bool {
        let (key, slot) = split(address);
        let Some(chunk) = self.chunks.get_mut(&key) else {
            return false;
        };
        if !chunk.has(slot) {
            return false;
        }
        chunk.clear(slot);
        self.count -= 1;
        true
    }

    /// Remove the cells a store of `bytes` at `from` changes, reporting whether
    /// any were removed. A cell records the pixel byte it describes; storing
    /// that same byte leaves the pixel, and so its detail, as it was. Guest
    /// blitters that read, mask and write back every destination byte store
    /// unchanged values over text they are not drawing on.
    pub(super) fn remove_changed(&mut self, from: u32, bytes: &[u8]) -> bool {
        let Some(last) = from.checked_add(bytes.len().saturating_sub(1) as u32) else {
            return self.remove_range(from, u64::from(from) + bytes.len() as u64);
        };
        if bytes.is_empty() || !self.may_hold_cells(from, last) {
            return false;
        }
        let mut removed = false;
        let mut rewritten_witnesses = Vec::new();
        // The first and last address changed of each run.
        let mut runs: Vec<(u32, (u32, u32))> = Vec::new();
        for (offset, &byte) in bytes.iter().enumerate() {
            let address = from + offset as u32;
            let (key, slot) = split(address);
            let Some(chunk) = self.chunks.get_mut(&key) else {
                continue;
            };
            if !chunk.has(slot) {
                continue;
            }
            if chunk.values[slot] == byte {
                // The byte stays, and so does its detail. But once its run
                // is damaged, a store over it is part of what is drawing over
                // the run, and the cell no longer shows the run holds text.
                let run = chunk.runs[slot];
                if run != 0 && !self.damage.is_empty() {
                    let Some(damage) = self.damage.get(&run) else { continue };
                    chunk.rewritten[slot] = run;
                    if (damage.witness.0 == address || damage.witness.1 == address)
                        && !rewritten_witnesses.contains(&run)
                    {
                        rewritten_witnesses.push(run);
                    }
                }
                continue;
            }
            {
                let run = chunk.runs[slot];
                chunk.clear(slot);
                self.count -= 1;
                removed = true;
                if run != 0 {
                    match runs.iter_mut().find(|(held, _)| *held == run) {
                        Some((_, span)) => span.1 = address,
                        None => runs.push((run, (address, address))),
                    }
                }
            }
        }
        // A changed byte on either side of a cell can expose a stale glyph
        // edge, but two independent stores say nothing about the gap between
        // them. Invalidate only within actual writes. Remember the outer
        // tails of the boundary stores until a later change brackets them.
        let store_end = last;
        let mut ended = Vec::new();
        for (run, (first, last)) in runs {
            let damage = self.damage.get(&run).copied();
            let (old_first, old_last) = damage.map_or((first, last), |d| d.span);
            let span = (first.min(old_first), last.max(old_last));
            let (mut left, mut right) = (from, store_end);
            if let Some(old) = damage {
                if first < old_first {
                    self.remove_run_cells(run, first.max(old.store_edges.0), old_first);
                }
                left = left.min(old.store_edges.0);
                if last > old_last {
                    self.remove_run_cells(
                        run,
                        old_last.saturating_add(1),
                        last.min(old.store_edges.1).saturating_add(1),
                    );
                }
                right = right.max(old.store_edges.1);
            }
            self.remove_run_cells(
                run,
                from.max(span.0),
                store_end.min(span.1).saturating_add(1),
            );
            let witness = damage
                .map(|damage| damage.witness)
                .filter(|&(a, b)| {
                    self.holds_untouched_run_cell(a, run) && self.holds_untouched_run_cell(b, run)
                })
                .or_else(|| self.text_witness(run, first));
            match witness {
                Some(witness) => {
                    self.damage.insert(run, RunDamage {
                        span,
                        store_edges: (left, right),
                        witness,
                    });
                }
                None => {
                    self.damage.remove(&run);
                    ended.push(run);
                }
            }
        }
        // A store can invalidate the last text witness without changing a byte.
        for run in rewritten_witnesses {
            let Some(damage) = self.damage.get(&run) else { continue };
            let (a, b) = damage.witness;
            if self.holds_untouched_run_cell(a, run) && self.holds_untouched_run_cell(b, run) {
                continue;
            }
            if let Some(witness) = self.text_witness(run, from) {
                self.damage.get_mut(&run).unwrap().witness = witness;
            } else {
                self.damage.remove(&run);
                ended.push(run);
            }
        }
        if !ended.is_empty() {
            removed |= self.remove_runs(&ended) != 0;
        }
        removed
    }

    /// Remove every cell in `[from, to)`, reporting whether any existed.
    /// `to` may lie past the last address; the span ends there.
    pub(super) fn remove_range(&mut self, from: u32, to: u64) -> bool {
        let to = to.min(1 << 32);
        if u64::from(from) >= to {
            return false;
        }
        let last = (to - 1) as u32;
        let mut removed = false;
        if !self.may_hold_cells(from, last) {
            return removed;
        }
        let first_chunk = from >> CHUNK_SHIFT;
        let last_chunk = last >> CHUNK_SHIFT;
        if first_chunk == last_chunk {
            let Some(chunk) = self.chunks.get_mut(&first_chunk) else {
                return false;
            };
            let first = (from as usize) & (CHUNK_BYTES - 1);
            let end = ((last as usize) & (CHUNK_BYTES - 1)) + 1;
            let cleared = chunk.clear_span(first, end);
            self.count -= cleared;
            return cleared != 0;
        }
        for (&key, chunk) in self.chunks.range_mut(first_chunk..=last_chunk) {
            let base = key << CHUNK_SHIFT;
            let first = from.saturating_sub(base).min(CHUNK_BYTES as u32) as usize;
            let end = (u64::from(last) - u64::from(base) + 1).min(CHUNK_BYTES as u64) as usize;
            let cleared = chunk.clear_span(first, end);
            self.count -= cleared;
            removed |= cleared != 0;
        }
        removed
    }

    /// Whether every cell in `[from, to)` has `len` samples.
    pub(super) fn all_cells_have_len(&self, from: u32, to: u64, len: usize) -> bool {
        let mut all = true;
        self.visit(from, to, |_, chunk, slot| {
            all = usize::from(chunk.lens[slot]) == len;
            all
        });
        all
    }

    /// Whether any cell lies in `[from, to)`.
    pub(super) fn any_in(&self, from: u32, to: u64) -> bool {
        let mut any = false;
        self.visit(from, to, |_, _, _| {
            any = true;
            false
        });
        any
    }

    /// The cells in `[from, to)` in address order.
    pub(super) fn range(&self, from: u32, to: u64) -> Vec<(u32, Arc<DetailCell>)> {
        let mut cells = Vec::new();
        self.visit(from, to, |address, chunk, slot| {
            cells.push((address, chunk.cell(slot)));
            true
        });
        cells
    }

    /// Visit the occupied slots in `[from, to)` in address order until
    /// `visit` returns false.
    fn visit(&self, from: u32, to: u64, mut visit: impl FnMut(u32, &Chunk, usize) -> bool) {
        let to = to.min(1 << 32);
        if u64::from(from) >= to {
            return;
        }
        let last = (to - 1) as u32;
        if !self.may_hold_cells(from, last) {
            return;
        }
        for (&key, chunk) in self.chunks.range(from >> CHUNK_SHIFT..=last >> CHUNK_SHIFT) {
            let base = key << CHUNK_SHIFT;
            let first = from.saturating_sub(base).min(CHUNK_BYTES as u32) as usize;
            let end = (u64::from(last) - u64::from(base) + 1).min(CHUNK_BYTES as u64) as usize;
            for word in first / 64..end.div_ceil(64) {
                let mut bits = chunk.present[word];
                while bits != 0 {
                    let slot = word * 64 + bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    if slot < first || slot >= end {
                        continue;
                    }
                    if !visit(base + slot as u32, chunk, slot) {
                        return;
                    }
                }
            }
        }
    }

    /// Visit each cell in `[from, from + len)` in address order, with its
    /// offset from `from`.
    pub(super) fn visit_cells(&self, from: u32, len: usize, mut visit: impl FnMut(usize, OffscreenCellRef<'_>)) {
        self.visit(from, u64::from(from) + len as u64, |address, chunk, slot| {
            visit((address - from) as usize, OffscreenCellRef { chunk, slot });
            true
        });
    }

    /// Make `address` hold the cell `value` / `indices` / `ink` (ink sorted
    /// by sample), unless it already holds exactly that cell, and report
    /// whether it changed. The ink is moved out of `ink`.
    pub(super) fn store_parts(&mut self, address: u32, value: u8, indices: &[u8], ink: &mut [(u8, Ink)]) -> bool {
        assert!(indices.len() <= TILE_SAMPLES, "cell samples exceed a tile");
        let (key, slot) = split(address);
        self.mark_page(key);
        let chunk = self.chunks.entry(key).or_insert_with(|| Box::new(Chunk::new()));
        if chunk.has(slot) {
            let start = slot * TILE_SAMPLES;
            if chunk.values[slot] == value
                && usize::from(chunk.lens[slot]) == indices.len()
                && chunk.indices[start..start + indices.len()] == *indices
                && chunk.slot_ink(slot).eq_list(ink)
            {
                if self.run != 0 {
                    chunk.runs[slot] = self.run;
                    self.note_run(address, address);
                }
                return false;
            }
            chunk.clear(slot);
            self.count -= 1;
        }
        chunk.present[slot / 64] |= 1 << (slot % 64);
        chunk.count += 1;
        self.count += 1;
        chunk.values[slot] = value;
        chunk.lens[slot] = indices.len() as u8;
        let start = slot * TILE_SAMPLES;
        chunk.indices[start..start + indices.len()].copy_from_slice(indices);
        if !ink.is_empty() {
            let mut held = chunk.slot_ink_mut(slot);
            held.clear();
            for (sample, ink) in ink.iter_mut() {
                held.set(usize::from(*sample), take_ink(ink));
            }
        }
        chunk.forget_shared(slot);
        chunk.runs[slot] = self.run;
        self.note_run(address, address);
        true
    }

    /// `store_parts` with the ink as a packed block (no blended ink).
    pub(super) fn store_block(&mut self, address: u32, value: u8, indices: &[u8], ink: &CellInk) -> bool {
        assert!(indices.len() <= TILE_SAMPLES, "cell samples exceed a tile");
        let (key, slot) = split(address);
        self.mark_page(key);
        let chunk = self.chunks.entry(key).or_insert_with(|| Box::new(Chunk::new()));
        if chunk.has(slot) {
            let start = slot * TILE_SAMPLES;
            if chunk.values[slot] == value
                && usize::from(chunk.lens[slot]) == indices.len()
                && chunk.indices[start..start + indices.len()] == *indices
                && chunk.slot_ink(slot).block().same_as(ink)
            {
                if self.run != 0 {
                    chunk.runs[slot] = self.run;
                    self.note_run(address, address);
                }
                return false;
            }
            chunk.clear(slot);
            self.count -= 1;
        }
        chunk.present[slot / 64] |= 1 << (slot % 64);
        chunk.count += 1;
        self.count += 1;
        chunk.values[slot] = value;
        chunk.lens[slot] = indices.len() as u8;
        let start = slot * TILE_SAMPLES;
        chunk.indices[start..start + indices.len()].copy_from_slice(indices);
        if !ink.is_empty() {
            chunk.slot_ink_mut(slot).assign(ink);
        }
        chunk.forget_shared(slot);
        chunk.runs[slot] = self.run;
        self.note_run(address, address);
        true
    }

    /// Every occupied address, in order.
    pub(super) fn addresses(&self) -> Vec<u32> {
        let mut addresses = Vec::with_capacity(self.count);
        self.visit(0, 1 << 32, |address, _, _| {
            addresses.push(address);
            true
        });
        addresses
    }

    /// The cell at `address`; one changed while a run draws joins the run.
    pub(super) fn cell_mut(&mut self, address: u32) -> Option<OffscreenCellMut<'_>> {
        let run = self.run;
        if run != 0 && self.contains(address) {
            self.note_run(address, address);
        }
        let (chunk, slot) = split(address);
        let chunk = self.chunks.get_mut(&chunk)?;
        if !chunk.has(slot) {
            return None;
        }
        if run != 0 {
            chunk.runs[slot] = run;
        }
        Some(OffscreenCellMut { chunk, slot })
    }

    /// The cell at `address`, first creating one of `len` samples of
    /// `background` with no ink when there is none.
    pub(super) fn cell_mut_or_insert(
        &mut self,
        address: u32,
        background: u8,
        len: usize,
    ) -> OffscreenCellMut<'_> {
        let (key, slot) = split(address);
        self.mark_page(key);
        let run = self.run;
        self.note_run(address, address);
        let chunk = self.chunks.entry(key).or_insert_with(|| Box::new(Chunk::new()));
        if chunk.insert_blank(slot, background, len) {
            self.count += 1;
            chunk.runs[slot] = run;
        } else if run != 0 {
            chunk.runs[slot] = run;
        }
        OffscreenCellMut { chunk, slot }
    }

    /// `cell_mut_or_insert` for each of the consecutive addresses from
    /// `address`, one per byte of `backgrounds`, that `select` picks: a cell
    /// for which `select` returns `None` is neither inserted nor visited; the
    /// others are inserted when absent and visited with their offset and what
    /// `select` returned. Each chunk is looked up once. The span must not wrap.
    pub(super) fn selected_cells_mut_or_insert<T>(
        &mut self,
        address: u32,
        backgrounds: &[u8],
        len: usize,
        mut select: impl FnMut(usize) -> Option<T>,
        mut visit: impl FnMut(usize, T, &mut OffscreenCellMut<'_>),
    ) {
        assert!(
            u64::from(address) + backgrounds.len() as u64 <= 1 << 32,
            "offscreen span wraps"
        );
        let run = self.run;
        let mut reached: Option<(usize, usize)> = None;
        let mut offset = 0;
        while offset < backgrounds.len() {
            let (key, first) = split(address + offset as u32);
            let span = (CHUNK_BYTES - first).min(backgrounds.len() - offset);
            // The chunk is found (or made) at the span's first selected cell.
            let mut pending =
                (0..span).find_map(|i| select(offset + i).map(|selected| (i, selected)));
            if let Some((skipped, _)) = pending {
                self.mark_page(key);
                let chunk = self
                    .chunks
                    .entry(key)
                    .or_insert_with(|| Box::new(Chunk::new()));
                for i in skipped..span {
                    let selected = match pending.take() {
                        Some((_, selected)) => selected,
                        None => match select(offset + i) {
                            Some(selected) => selected,
                            None => continue,
                        },
                    };
                    let slot = first + i;
                    if chunk.insert_blank(slot, backgrounds[offset + i], len) {
                        self.count += 1;
                        chunk.runs[slot] = run;
                    } else if run != 0 {
                        chunk.runs[slot] = run;
                    }
                    let at = offset + i;
                    reached = Some(reached.map_or((at, at), |(lo, hi)| (lo.min(at), hi.max(at))));
                    visit(at, selected, &mut OffscreenCellMut { chunk, slot });
                }
            }
            offset += span;
        }
        if let Some((lo, hi)) = reached {
            self.note_run(address + lo as u32, address + hi as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::IndexedColor;
    use super::*;

    fn cell(value: u8, indices: &[u8], ink: &[(usize, u8)]) -> Arc<DetailCell> {
        Arc::new(DetailCell {
            value,
            indices: indices.to_vec(),
            ink: ink
                .iter()
                .map(|&(sample, foreground)| {
                    (
                        sample,
                        Ink {
                            foreground,
                            alpha: 90,
                            background: IndexedColor::Solid(value),
                        },
                    )
                })
                .collect(),
            dimmed: false,
        })
    }

    #[test]
    fn short_invalidations_preserve_neighbors_snapshots_and_reused_chunks() {
        let mut store = OffscreenDetail::default();
        let held = cell(7, &[1, 2, 3, 4], &[(1, 9)]);
        for address in [0x10fe, 0x10ff, 0x1100, u32::MAX] {
            store.insert(address, &held);
        }
        let snapshot = store.get(0x10ff).unwrap();
        assert!(store.remove_range(0x10ff, 0x1100));
        assert!(!store.remove_range(0x10ff, 0x1100));
        assert_eq!(store.get(0x10fe), Some(held.clone()));
        assert_eq!(store.get(0x1100), Some(held.clone()));
        assert_eq!(snapshot, held);
        store.insert(0x10ff, &held);
        assert!(store.remove_range(0x10fe, 0x1101));
        assert!(!store.contains(0x10fe));
        assert!(!store.contains(0x10ff));
        assert!(!store.contains(0x1100));
        assert!(store.remove_range(u32::MAX, u64::from(u32::MAX) + 2));
        assert!(!store.contains(u32::MAX));
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn a_changed_pixel_drops_its_whole_text_run() {
        let mut store = OffscreenDetail::default();
        // Run 1 spans two chunks; run 2 and an untagged cell sit between and
        // beside its cells.
        store.set_run(1);
        for address in [0x10f0, 0x10f1, 0x1205] {
            store.insert(address, &cell(5, &[5, 9, 9, 5], &[]));
        }
        store.set_run(2);
        store.insert(0x10f2, &cell(5, &[5, 7, 7, 5], &[]));
        store.set_run(0);
        store.insert(0x10f3, &cell(5, &[5, 3, 3, 5], &[]));

        // Storing every cell's own byte back keeps everything.
        assert!(!store.remove_changed(0x10f0, &[5, 5, 5, 5]));
        assert_eq!(store.len(), 5);

        // One changed pixel of run 1 drops all three of its cells, though
        // the other two keep their bytes.
        assert!(store.remove_changed(0x10f0, &[6]));
        assert!(!store.contains(0x10f1));
        assert!(!store.contains(0x1205));
        assert!(store.contains(0x10f2));
        assert!(store.contains(0x10f3));

        // An untagged cell goes alone, as before.
        assert!(store.remove_changed(0x10f3, &[6]));
        assert!(store.contains(0x10f2));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn a_run_partly_drawn_over_keeps_the_rows_that_still_show_it() {
        // Ink 9 over background 5, in three rows 0x100 apart: in each, a cell
        // whose byte is the ink, with background samples, and one whose byte
        // is the background, with ink samples.
        let fresh = || {
            let mut store = OffscreenDetail::default();
            store.set_run(1);
            for row in [0x4000, 0x4100, 0x4200] {
                store.insert(row, &cell(9, &[9, 9, 9, 5], &[]));
                store.insert(row + 1, &cell(5, &[5, 9, 5, 5], &[]));
            }
            store.set_run(0);
            store
        };
        // Something drawn over the first row takes only that row.
        let mut store = fresh();
        assert!(store.remove_changed(0x4000, &[7, 7]));
        assert_eq!(store.len(), 4);
        // A separate write to the last row leaves the untouched middle intact.
        assert!(store.remove_changed(0x4201, &[7]));
        assert!(store.holds_run_cell(0x4100, 1));
        assert!(store.holds_run_cell(0x4101, 1));
        // A blitter sweeping upward has the same guarantee.
        let mut store = fresh();
        assert!(store.remove_changed(0x4200, &[7, 7]));
        assert!(store.remove_changed(0x4001, &[7]));
        assert!(store.holds_run_cell(0x4100, 1));
        assert!(store.holds_run_cell(0x4101, 1));
        // Erasing to the background changes only the ink bytes; the edges
        // left would show nothing but the old glyphs' outline, so they go.
        let mut store = fresh();
        for row in [0x4000, 0x4100, 0x4200] {
            assert!(store.remove_changed(row, &[5, 5]));
        }
        assert_eq!(store.len(), 0);
        // Filling with the ink changes only the edge bytes, and likewise.
        let mut store = fresh();
        for row in [0x4000, 0x4100, 0x4200] {
            assert!(store.remove_changed(row, &[9, 9]));
        }
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn a_picture_drawn_over_the_whole_run_leaves_no_glyph_edges() {
        // Ink 9 over background 5 in three rows, as in the test above.
        let mut store = OffscreenDetail::default();
        store.set_run(1);
        for row in [0x4000, 0x4100, 0x4200] {
            store.insert(row, &cell(9, &[9, 9, 9, 5], &[]));
            store.insert(row + 1, &cell(5, &[5, 9, 5, 5], &[]));
        }
        store.set_run(0);
        // A new picture over all of it happens to match some bytes: the ink
        // byte on the first and last rows, the background byte on the
        // middle one. The cells left would still hold both kinds, but they
        // describe glyphs that are gone.
        store.remove_changed(0x4000, &[9, 7]);
        store.remove_changed(0x4100, &[7, 5]);
        store.remove_changed(0x4200, &[9, 7]);
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn a_picture_stored_byte_by_byte_over_the_run_leaves_no_glyph_edges() {
        // Ink 9 over background 5 in three rows, as in the tests above.
        let mut store = OffscreenDetail::default();
        store.set_run(1);
        for row in [0x4000, 0x4100, 0x4200] {
            store.insert(row, &cell(9, &[9, 9, 9, 5], &[]));
            store.insert(row + 1, &cell(5, &[5, 9, 5, 5], &[]));
        }
        store.set_run(0);
        // Guest code draws a new picture over all of it one byte at a time:
        // the ink colour over the first and last rows, the background colour
        // over the middle one. Each store reaches only its own byte, so the
        // cells whose bytes it matches stay: ink bytes on the outer rows, a
        // background byte in the middle. Between them they still show both
        // colours, but they are the new picture's, not the text's.
        for (row, colour) in [(0x4000, 9), (0x4100, 5), (0x4200, 9)] {
            for address in [row, row + 1] {
                store.remove_changed(address, &[colour]);
            }
        }
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn unchanged_tail_stores_finish_replacing_a_damaged_run() {
        let mut store = OffscreenDetail::default();
        store.set_run(1);
        store.insert(0x4000, &cell(9, &[9, 9, 9, 5], &[]));
        store.insert(0x4100, &cell(9, &[9, 9, 9, 5], &[]));
        store.insert(0x4200, &cell(5, &[5, 9, 5, 5], &[]));
        store.set_run(0);
        assert!(store.remove_changed(0x4000, &[7]));
        store.remove_changed(0x4100, &[9]);
        store.remove_changed(0x4200, &[5]);
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn stores_of_unchanged_bytes_before_any_change_keep_a_run_partly_drawn_over() {
        // A blitter that writes back every byte of the text unchanged, then
        // something drawn over its first row: the other rows still show it.
        let mut store = OffscreenDetail::default();
        store.set_run(1);
        for row in [0x4000, 0x4100, 0x4200] {
            store.insert(row, &cell(9, &[9, 9, 9, 5], &[]));
            store.insert(row + 1, &cell(5, &[5, 9, 5, 5], &[]));
        }
        store.set_run(0);
        for row in [0x4000, 0x4100, 0x4200] {
            assert!(!store.remove_changed(row, &[9, 5]));
        }
        assert!(store.remove_changed(0x4000, &[7, 7]));
        assert_eq!(store.len(), 4);
    }

    /// Cost of guest code erasing a band of retained text one byte at a
    /// time. Run with `cargo test --release --lib erase_bench -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore = "measurement"]
    fn erase_bench_byte_stores_over_a_text_band() {
        const ROW_BYTES: u32 = 656;
        const WIDTH: u32 = 640;
        const ROWS: u32 = 12;
        const BASE: u32 = 0x0100_0000;
        // Ink 9 cells with background samples beside background 5 cells
        // with ink samples, both with partial-coverage ink.
        let ink = cell(9, &[&[9; 12][..], &[5; 4]].concat(), &[(12, 9)]);
        let edge = cell(5, &[&[5; 12][..], &[9; 4]].concat(), &[(0, 9)]);
        // One run across the band, or one run per 16-pixel word, erased
        // from the top or from the bottom.
        for (run_width, upward) in [(WIDTH, false), (WIDTH, true), (16, false), (16, true)] {
            let mut timings = Vec::new();
            for _ in 0..5 {
                let mut store = OffscreenDetail::default();
                for left in (0..WIDTH).step_by(run_width as usize) {
                    store.begin_run();
                    for row in 0..ROWS {
                        for x in left..left + run_width {
                            let address = BASE + row * ROW_BYTES + x;
                            let cell = if (x + row) % 2 == 0 { &ink } else { &edge };
                            store.insert(address, cell);
                        }
                    }
                }
                store.set_run(0);
                let stores = ROWS * ROW_BYTES;
                let start = std::time::Instant::now();
                for offset in 0..stores {
                    let offset = if upward { stores - 1 - offset } else { offset };
                    store.remove_changed(BASE + offset, &[5]);
                }
                let elapsed = start.elapsed();
                assert_eq!(store.len(), 0);
                timings.push(elapsed);
            }
            timings.sort();
            let median = timings[timings.len() / 2];
            let stores = ROWS * ROW_BYTES;
            eprintln!(
                "erase_bench run_width={run_width} upward={upward} stores={stores} median_total={:.3} ms per_store={:.3} us",
                median.as_secs_f64() * 1e3,
                median.as_secs_f64() * 1e6 / f64::from(stores),
            );
        }
    }

    #[test]
    fn a_cell_redrawn_by_a_later_run_leaves_the_earlier_one() {
        let mut store = OffscreenDetail::default();
        store.set_run(1);
        store.insert(0x2000, &cell(5, &[5, 9, 9, 5], &[]));
        store.insert(0x2001, &cell(5, &[5, 9, 9, 5], &[]));
        store.set_run(2);
        store.insert(0x2001, &cell(5, &[5, 7, 7, 5], &[]));
        store.set_run(0);
        // Run 1 ends; the cell run 2 drew over it stays.
        assert!(store.remove_changed(0x2000, &[6]));
        assert!(store.contains(0x2001));
    }

    #[test]
    fn a_run_keeps_its_bounds_when_the_run_table_fills_up() {
        let mut store = OffscreenDetail::default();
        store.begin_run();
        store.insert(0x3000, &cell(5, &[5, 9, 9, 5], &[]));
        store.insert(0x3400, &cell(5, &[5, 9, 9, 5], &[]));
        // Enough later runs, each taking over one other cell, to fill the
        // table with runs that no longer own anything.
        for _ in 0..MAX_RUNS + 1 {
            store.begin_run();
            store.insert(0x5000, &cell(5, &[5, 7, 7, 5], &[]));
        }
        store.set_run(0);
        assert!(store.remove_changed(0x3000, &[6]));
        assert!(!store.contains(0x3400), "the first run lost its bounds");
        assert!(store.contains(0x5000));
    }

    #[test]
    fn a_run_that_redraws_a_cell_unchanged_owns_it() {
        let mut store = OffscreenDetail::default();
        let mut ink = [];
        store.begin_run();
        store.store_parts(0x2000, 5, &[5, 9, 9, 5], &mut ink);
        store.begin_run();
        assert!(!store.store_parts(0x2000, 5, &[5, 9, 9, 5], &mut ink));
        store.store_parts(0x2001, 5, &[5, 7, 7, 5], &mut ink);
        store.set_run(0);
        // Ending the second run takes the redrawn cell with it.
        assert!(store.remove_changed(0x2001, &[6]));
        assert!(!store.contains(0x2000));
    }

    /// Deterministic xorshift: the model test needs varied but repeatable ops.
    fn next(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    /// A page that never held a chunk answers every query without one,
    /// while a query reaching across a page boundary still finds, and
    /// removes, the cells on the populated side.
    #[test]
    fn untouched_pages_hold_no_cells_and_boundary_queries_still_see_them() {
        let mut store = OffscreenDetail::default();
        let page = 1u32 << PAGE_SHIFT;
        assert!(store.range(0x0003_0000, 0x0003_0100).is_empty(), "an empty store");
        let held = cell(7, &[1, 2, 3, 4], &[]);
        store.insert(5 * page + 2, &held);
        store.insert(5 * page - 1, &held);
        // Pages 4 (only its last byte) and 5 hold cells; 3 and 6 never did.
        assert!(store.range(3 * page, u64::from(4 * page)).is_empty());
        assert!(!store.any_in(6 * page, u64::from(7 * page)));
        assert!(!store.remove_range(6 * page, u64::from(7 * page)));
        let across: Vec<u32> = store
            .range(5 * page - 4, u64::from(5 * page + 4))
            .into_iter()
            .map(|(address, _)| address)
            .collect();
        assert_eq!(across, [5 * page - 1, 5 * page + 2]);
        let mut seen = Vec::new();
        store.visit_cells(5 * page - 4, 8, |offset, _| seen.push(offset));
        assert_eq!(seen, [3, 6]);
        assert!(store.all_cells_have_len(5 * page - 4, u64::from(5 * page + 4), 4));
        assert!(store.remove_range(5 * page - 4, u64::from(5 * page + 4)));
        assert!(store.range(4 * page, u64::from(6 * page)).is_empty());
        // The last page of the address space, reached by a range ending at 2^32.
        store.insert(u32::MAX, &held);
        assert_eq!(store.range(u32::MAX - 8, 1 << 32).len(), 1);
    }

    /// Every operation agrees with the address-keyed map it replaced,
    /// including across chunk boundaries, removals that empty a chunk,
    /// in-place edits and ranges reaching the last address.
    #[test]
    fn chunk_store_agrees_with_an_ordered_map() {
        let mut store = OffscreenDetail::default();
        let mut model: BTreeMap<u32, DetailCell> = BTreeMap::new();
        let mut rng = 0x9E37_79B9_7F4A_7C15u64;
        let bases = [0x0003_0000u32, 0x0003_00F0, 0xFFFF_FF00, 0x0010_0000];
        for step in 0..20_000 {
            let r = next(&mut rng);
            let base = bases[(r % bases.len() as u64) as usize];
            let address = base.wrapping_add((r >> 8) as u32 % 600);
            match (r >> 20) % 8 {
                0 | 1 => {
                    let len = [4usize, 9, 16][((r >> 24) % 3) as usize];
                    let indices: Vec<u8> = (0..len).map(|i| (r >> (i % 8)) as u8).collect();
                    let ink: Vec<(usize, u8)> = (0..len)
                        .filter(|i| (r >> (32 + i)) & 3 == 0)
                        .map(|i| (i, (r >> 40) as u8))
                        .collect();
                    let new = cell((r >> 48) as u8, &indices, &ink);
                    store.insert(address, &new);
                    model.insert(address, (*new).clone());
                }
                2 => {
                    assert_eq!(store.remove(address), model.remove(&address).is_some(), "step {step}");
                }
                3 => {
                    let to = u64::from(address) + (r >> 40) % 700;
                    let expected: Vec<u32> = model
                        .range(address..)
                        .take_while(|(&key, _)| u64::from(key) < to)
                        .map(|(&key, _)| key)
                        .collect();
                    for key in &expected {
                        model.remove(key);
                    }
                    assert_eq!(store.remove_range(address, to), !expected.is_empty(), "step {step}");
                }
                4 => {
                    let value = (r >> 30) as u8;
                    if let Some(mut held) = store.cell_mut(address) {
                        held.set_value(value);
                        let sample = (r >> 36) as usize % held.len();
                        held.set_index(sample, value ^ 0x55);
                        held.remove_ink(sample);
                        let model_cell = model.get_mut(&address).unwrap();
                        model_cell.value = value;
                        model_cell.indices[sample] = value ^ 0x55;
                        model_cell.ink.remove(&sample);
                    } else {
                        assert!(!model.contains_key(&address), "step {step}");
                    }
                }
                5 => {
                    let mut held = store.cell_mut_or_insert(address, 7, 16);
                    let sample = (r >> 36) as usize % held.len();
                    let foreground = (r >> 44) as u8;
                    held.update_ink(
                        sample,
                        || Ink { foreground, alpha: 0, background: IndexedColor::Solid(3) },
                        |ink| ink.alpha = 200,
                    );
                    let model_cell = model.entry(address).or_insert_with(|| DetailCell {
                        value: 7,
                        indices: vec![7; 16],
                        ink: HashMap::default(),
                        dimmed: false,
                    });
                    model_cell
                        .ink
                        .entry(sample)
                        .or_insert_with(|| Ink {
                            foreground,
                            alpha: 0,
                            background: IndexedColor::Solid(3),
                        })
                        .alpha = 200;
                }
                6 => {
                    let to = u64::from(address) + (r >> 40) % 700;
                    let expected: Vec<(u32, DetailCell)> = model
                        .range(address..)
                        .take_while(|(&key, _)| u64::from(key) < to)
                        .map(|(&key, cell)| (key, cell.clone()))
                        .collect();
                    let got: Vec<(u32, DetailCell)> = store
                        .range(address, to)
                        .into_iter()
                        .map(|(key, cell)| (key, (*cell).clone()))
                        .collect();
                    assert_eq!(got, expected, "step {step}");
                    assert_eq!(store.any_in(address, to), !expected.is_empty(), "step {step}");
                    for len in [4usize, 16] {
                        assert_eq!(
                            store.all_cells_have_len(address, to, len),
                            expected.iter().all(|(_, cell)| cell.indices.len() == len),
                            "step {step}"
                        );
                    }
                }
                _ => {
                    let held = model.get(&address);
                    assert_eq!(store.get(address).map(|cell| (*cell).clone()).as_ref(), held, "step {step}");
                    assert!(store.matches(address, held), "step {step}");
                    assert_eq!(store.contains(address), held.is_some(), "step {step}");
                    let mut viewed = None;
                    store.visit_cells(address, 1, |offset, view| {
                        assert_eq!(offset, 0);
                        let ink: Vec<(usize, Ink)> = view.inks().collect();
                        viewed = Some((view.indices().to_vec(), ink));
                    });
                    let expected = held.map(|cell| {
                        let mut ink: Vec<(usize, Ink)> = cell.ink.iter().map(|(&k, v)| (k, v.clone())).collect();
                        ink.sort_by_key(|&(sample, _)| sample);
                        (cell.indices.clone(), ink)
                    });
                    assert_eq!(viewed, expected, "step {step}");
                    // Storing a cell's own parts back changes nothing; a
                    // different value replaces it.
                    if let Some((indices, ink)) = expected {
                        let mut parts: Vec<(u8, Ink)> = ink.iter().map(|(k, v)| (*k as u8, v.clone())).collect();
                        let value = held.unwrap().value;
                        assert!(!store.store_parts(address, value, &indices, &mut parts), "step {step}");
                        let mut parts: Vec<(u8, Ink)> = ink.iter().map(|(k, v)| (*k as u8, v.clone())).collect();
                        assert!(store.store_parts(address, value ^ 1, &indices, &mut parts), "step {step}");
                        model.get_mut(&address).unwrap().value ^= 1;
                    }
                }
            }
            assert_eq!(store.len(), model.len(), "step {step}");
        }
        assert_eq!(store.addresses(), model.keys().copied().collect::<Vec<_>>());
    }

    /// Unchanged cells are handed out as one shared `Arc`, and a stored
    /// `Arc` is handed back; any edit ends the sharing.
    /// The per-sample painting `paint_glyph` replaces.
    fn paint_per_sample(cell: &mut OffscreenCellMut<'_>, alphas: &[u8], foreground: u8) {
        for (i, &alpha) in alphas.iter().enumerate() {
            let alpha = u32::from(alpha);
            if alpha == 0 {
                continue;
            }
            if alpha == 255 {
                cell.set_index(i, foreground);
                cell.remove_ink(i);
            } else {
                let background = cell.index(i);
                cell.update_ink(
                    i,
                    || Ink { foreground, alpha: 0, background: IndexedColor::Solid(background) },
                    |ink| {
                        if ink.foreground != foreground {
                            let previous = ink.clone();
                            *ink = Ink {
                                foreground,
                                alpha: 0,
                                background: previous.background.over(previous.foreground, previous.alpha),
                            };
                        }
                        ink.alpha = ink.alpha.max(alpha);
                    },
                );
            }
        }
    }

    #[test]
    fn glyph_cell_paint_matches_per_sample_painting() {
        let mut painted = OffscreenDetail::default();
        let mut reference = OffscreenDetail::default();
        let mut rng = 0x2545_F491_4F6C_DD1Du64;
        for step in 0..20_000 {
            let r = next(&mut rng);
            let address = 0x4_0000 + (r % 40) as u32;
            let len = [4usize, 9, 16][((r >> 8) % 3) as usize];
            let foreground = [3u8, 7, 200][((r >> 12) % 3) as usize];
            let alphas: Vec<u8> = (0..len)
                .map(|i| [0u8, 0, 255, 128, 40, 255, 1, 254][((r >> (16 + i * 3)) & 7) as usize])
                .collect();
            let background = (r >> 60) as u8;
            if (r >> 58) & 3 == 0 {
                painted.remove(address);
                reference.remove(address);
                continue;
            }
            let n = painted.cell_mut_or_insert(address, background, len).len().min(alphas.len());
            painted.cell_mut(address).unwrap().paint_glyph(&alphas[..n], foreground);
            let mut cell = reference.cell_mut_or_insert(address, background, len);
            let n = alphas.len().min(cell.len());
            paint_per_sample(&mut cell, &alphas[..n], foreground);
            assert_eq!(
                painted.get(address).map(|cell| (*cell).clone()),
                reference.get(address).map(|cell| (*cell).clone()),
                "step {step}"
            );
        }
    }

    #[test]
    fn unchanged_cells_keep_their_identity() {
        let mut store = OffscreenDetail::default();
        let stored = cell(1, &[1, 2, 3, 4], &[(2, 9)]);
        store.insert(0x4_0000, &stored);
        assert!(Arc::ptr_eq(&store.get(0x4_0000).unwrap(), &stored));
        store.cell_mut_or_insert(0x4_0001, 5, 4);
        let built = store.get(0x4_0001).unwrap();
        assert!(Arc::ptr_eq(&store.get(0x4_0001).unwrap(), &built));
        store.cell_mut(0x4_0000).unwrap().set_value(8);
        let changed = store.get(0x4_0000).unwrap();
        assert!(!Arc::ptr_eq(&changed, &stored));
        assert_eq!(changed.value, 8);
        assert_eq!(changed.ink, stored.ink);
    }

    #[test]
    fn run_bounds_rebuilds_are_amortized_above_max_runs() {
        let mut store = OffscreenDetail::default();
        let total = MAX_RUNS + 1_024;
        for i in 0..total {
            store.begin_run();
            let address = 0x10_0000 + i as u32;
            store.cell_mut_or_insert(address, (i & 0xff) as u8, 4);
        }
        store.settle_run();
        // Only the initial threshold crossing at MAX_RUNS triggers a rebuild;
        // doubling the threshold avoids rebuilding on each subsequent live run.
        assert_eq!(store.run_bounds_rebuilds, 1);
        assert_eq!(store.run_bounds.len(), total);
        assert_eq!(store.count, total);

        // Removing the first and last live runs still finds their cells.
        assert_eq!(store.remove_runs(&[1, total as u32]), 2);
        assert!(store.get(0x10_0000).is_none());
        assert!(store.get(0x10_0000 + (total as u32 - 1)).is_none());
        assert!(store.get(0x10_0001).is_some());
    }
}
