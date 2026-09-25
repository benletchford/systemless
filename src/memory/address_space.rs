//! Architecture-neutral mapped guest address space.
//!
//! CPU backends have different bus contracts, but guest bytes must have one
//! owner. This type provides that owner while preserving the sparse mappings,
//! read-only regions, and instruction-cache behavior required by native PEF
//! applications.

use super::page_index::PageIndex;
use crate::guest_procedure::GuestIsa;
use m68k::core::memory::{BusFault, BusFaultKind};
use m68k::AddressBus;
use ppc::{PpcMemory, PpcSectionMem, PpcSectionMemSpan, try_allocate_instruction_cache_token};
use std::cell::UnsafeCell;
use std::rc::Rc;

use super::bus::SharedRamRegion;

#[derive(Debug, Clone)]
struct SharedRegionMapping {
    base: u32,
    region: SharedRamRegion,
    writable: bool,
    code_isa: Option<GuestIsa>,
}

#[derive(Debug, Clone, Copy)]
struct OrdinaryRegionMapping {
    base: u32,
    len: usize,
}

/// A span of addresses that provably resolve to one shared mapping.
///
/// `index` indexes `shared_regions`, which only ever grows by appending. The
/// span excludes every later mapping that would shadow part of it, so
/// `shared_mapping_at` names `index` for every address in `start..end`.
#[derive(Debug, Clone, Copy)]
struct SharedRun {
    index: usize,
    start: u64,
    end: u64,
}

/// What the shared-mapping ledger says about a maximal interval of addresses:
/// the mapping backing it, or a proof that no mapping touches it. Both answers
/// let a later access in the same interval skip the ledger.
#[derive(Debug, Clone, Copy)]
enum SharedLookup {
    Owned(SharedRun),
    Gap { start: u64, end: u64 },
}

impl SharedLookup {
    #[inline]
    fn covers(&self, start: u64, end: u64) -> bool {
        let (lookup_start, lookup_end) = self.span();
        lookup_start <= start && end <= lookup_end
    }

    /// The interval this answer describes uniformly.
    #[inline]
    fn span(&self) -> (u64, u64) {
        match self {
            Self::Owned(run) => (run.start, run.end),
            Self::Gap { start, end } => (*start, *end),
        }
    }
}

/// What one ledger lookup proves about a whole range.
#[derive(Debug, Clone, Copy)]
enum RangeSpan {
    /// One shared mapping owns every byte of the range.
    Owned(SharedRun),
    /// The range lies inside a ledger gap: no shared mapping touches it.
    NoMapping,
    /// The range crosses ledger boundaries, so only the walking queries can
    /// classify it.
    Unresolved,
}

/// Entries in each scalar ledger cache.
///
/// Guest code interleaves its stack, heap, and aliased RAM, which sit in
/// different maximal ledger intervals; with one entry, an access in any other
/// interval misses and re-walks the whole mapping ledger before routing. A few
/// entries keep the intervals a running program cycles through resident.
const LOOKUP_SLOTS: usize = 4;
const LOOKUP_REPLACE_MASK: usize = LOOKUP_SLOTS - 1;

/// A small associative cache of ledger answers.
///
/// Entries are intervals from [`shared_lookup_at`] and are validated by
/// [`SharedLookup::covers`] before use, so a hit is always sound. Lookup scans
/// the entries rather than indexing by address: a cached interval covers a
/// whole run, so a streaming access hits the entry that is already resident
/// however far the address has advanced. Every entry is dropped by
/// [`GuestAddressSpaceState::push_shared_mapping`], because an appended mapping
/// can shadow a cached span or fall inside a cached gap.
#[derive(Debug, Default, Clone, Copy)]
struct LookupCache {
    entries: [Option<SharedLookup>; LOOKUP_SLOTS],
    /// Slot the next miss replaces, so misses evict in rotation.
    next: usize,
}

impl LookupCache {
    /// A resident answer whose interval covers `[start, end)`, if any.
    #[inline]
    fn covering(&self, start: u64, end: u64) -> Option<SharedLookup> {
        self.entries
            .iter()
            .flatten()
            .find(|entry| entry.covers(start, end))
            .copied()
    }

    /// The ledger answer covering `[start, end)`, reusing a resident entry when
    /// one already does. Always worth storing back: the answer describes an
    /// interval either way.
    #[inline]
    fn resolve(
        &self,
        state: &GuestAddressSpaceState,
        address: u32,
        start: u64,
        end: u64,
    ) -> SharedLookup {
        self.covering(start, end)
            .unwrap_or_else(|| shared_lookup_at(state, address))
    }

    #[inline]
    fn store(&mut self, lookup: SharedLookup) {
        let (start, end) = lookup.span();
        // A resident entry that already spans this answer makes a new copy
        // redundant, which keeps a scan through one long run from filling every
        // slot with the same interval and evicting the others.
        if self
            .entries
            .iter()
            .flatten()
            .any(|entry| entry.covers(start, end))
        {
            return;
        }
        self.entries[self.next] = Some(lookup);
        self.next = (self.next + 1) & LOOKUP_REPLACE_MASK;
    }

    fn clear(&mut self) {
        self.entries = [None; LOOKUP_SLOTS];
        self.next = 0;
    }
}

/// The backing selected by the process address-space router for an access.
///
/// `Shared` and `SharedReadOnly` identify process mappings whose bytes are
/// owned by another adapter (normally a range of the classic bus RAM).
/// `Sparse` identifies a native PEF/ordinary mapping, while `Flat` is the
/// local classic RAM fallback supplied by the 68K adapter. `Mixed` means that
/// an access spans more than one backing and must use the byte-granular path.
/// Keeping this classification here makes the ownership decision independent
/// of either CPU bus contract while retaining mapping protection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuestMemoryRoute {
    Shared,
    SharedReadOnly,
    Sparse,
    Flat,
    Unmapped,
    Mixed,
}

/// A sparse guest address space that can be executed by either CPU backend.
///
/// The region implementation remains private so loaders and runtime services
/// depend on the architecture-neutral ownership boundary rather than a CPU
/// crate's concrete memory type.
#[derive(Debug, Default)]
struct GuestAddressSpaceState {
    presentation: super::presentation::PresentationSlot,
    regions: PpcSectionMem,
    ordinary_regions: Vec<OrdinaryRegionMapping>,
    shared_regions: Vec<SharedRegionMapping>,
    /// Derived envelope of nonempty shared mappings; extended on every insert
    /// and copied with detached snapshots. It only rejects impossible hits.
    shared_bounds: std::ops::Range<u64>,
    /// Per-page filter over `shared_regions`, maintained alongside the
    /// envelope above. The envelope alone cannot reject an address that falls
    /// between two distant mappings, which is exactly the case on the CPU hot
    /// path: guest code executes from sparse PEF sections that sit inside the
    /// envelope spanned by the runner's RAM aliases.
    shared_pages: PageIndex,
    readonly_allocation_exclusions: Vec<(u32, u32)>,
    /// Recent ledger answers, one cache for instruction fetches and one for
    /// data accesses. Two caches because code and data addresses interleave.
    instruction_lookup: LookupCache,
    data_lookup: LookupCache,
    /// Recent ledger answers for whole ranges, used by the range router and by
    /// the classic adapter's local-alias promotion. Both ask about the same
    /// bulk ranges (a mirror row is routed and then promoted), and a range
    /// spans many scalar intervals, so this cache needs its own entries.
    route_lookup: LookupCache,
    /// Pages we have issued a writable-code token for. A write into one of
    /// them may have rewritten a cached decode, so it retires that page's
    /// tokens; a clear bit proves the write cannot have touched executed code
    /// and costs the write path one word test.
    executed_pages: PageIndex,
    /// Last writable-code token, with the page it answers for. Every
    /// rotation clears it, so a stale token cannot outlive its generation.
    instruction_token_cache: Option<(u32, u64)>,
    /// Tokens issued per code page, retired by a write to that page. Guest
    /// data shares 4 KiB pages with guest code, so retiring per page keeps
    /// data traffic from invalidating every block in the process.
    page_tokens: Option<Box<[CodeTokenSlot]>>,
}

impl GuestAddressSpaceState {
    fn push_shared_mapping(&mut self, mapping: SharedRegionMapping) {
        if mapping.region.len() != 0 {
            let start = u64::from(mapping.base);
            let end = start.saturating_add(mapping.region.len() as u64);
            if self.shared_bounds.is_empty() {
                self.shared_bounds = start..end;
            } else {
                self.shared_bounds.start = self.shared_bounds.start.min(start);
                self.shared_bounds.end = self.shared_bounds.end.max(end);
            }
            self.shared_pages.mark(start, end);
        }
        // A new mapping can shadow a cached span or fall inside a cached gap.
        self.instruction_lookup.clear();
        self.data_lookup.clear();
        self.route_lookup.clear();
        self.instruction_token_cache = None;
        self.shared_regions.push(mapping);
    }

    #[inline]
    fn may_overlap_shared(&self, start: u64, end: u64) -> bool {
        start < self.shared_bounds.end
            && self.shared_bounds.start < end
            && self.shared_pages.may_overlap(start, end)
    }

    /// The token stamped on writable code in the page holding `addr`.
    ///
    /// A page keeps its token until a write retires it, so the interpreter
    /// sees a stable answer while it builds a block. The value itself is
    /// drawn from [`try_allocate_instruction_cache_token`], which cannot
    /// repeat across pages, revisions, address spaces, or immutable mappings.
    #[inline]
    fn writable_code_token(&mut self, addr: u32) -> Option<u64> {
        self.writable_code_token_with(addr, try_allocate_instruction_cache_token)
    }

    fn writable_code_token_with(
        &mut self,
        addr: u32,
        allocate: impl FnOnce() -> Option<u64>,
    ) -> Option<u64> {
        let page = addr >> CODE_PAGE_SHIFT;
        let slots = self.page_tokens.get_or_insert_with(|| {
            vec![CodeTokenSlot::default(); CODE_TOKEN_SLOTS].into_boxed_slice()
        });
        let slot = &mut slots[(page as usize) & CODE_TOKEN_INDEX_MASK];
        if slot.token != 0 && slot.page == page {
            return Some(slot.token);
        }
        let token = allocate()?;
        *slot = CodeTokenSlot { page, token };
        Some(token)
    }

    /// Retire the tokens issued for the pages `start..end` touches. The next
    /// fetch from such a page mints a fresh token, so any block decoded from
    /// its previous contents can never be matched again.
    #[cold]
    fn retire_code_tokens(&mut self, start: u64, end: u64) {
        self.instruction_token_cache = None;
        let Some(slots) = self.page_tokens.as_deref_mut() else {
            return;
        };
        let first = (start >> CODE_PAGE_SHIFT) as usize;
        let last = (((end - 1) >> CODE_PAGE_SHIFT) as usize).min(CODE_PAGE_COUNT - 1);
        if last - first >= CODE_TOKEN_SLOTS {
            // A write that wide reaches every slot anyway.
            slots.fill(CodeTokenSlot::default());
            return;
        }
        for page in first..=last {
            let slot = &mut slots[page & CODE_TOKEN_INDEX_MASK];
            // A slot holding another page is that page's token, not this
            // one's; leaving it alone keeps an unrelated write cheap.
            if slot.page as usize == page {
                *slot = CodeTokenSlot::default();
            }
        }
    }

    /// Retire every instruction-cache token issued for writable guest code.
    fn retire_all_code_tokens(&mut self) {
        self.page_tokens = None;
        self.instruction_token_cache = None;
    }

    /// Report a completed write to the observers of guest memory: the
    /// presentation mirror, and the generation behind writable-code tokens.
    #[inline]
    fn note_write(&mut self, addr: u32, bytes: &[u8]) {
        self.presentation.write_bytes(addr, bytes);
        let start = u64::from(addr);
        let end = start.saturating_add(bytes.len() as u64);
        if self.executed_pages.may_overlap(start, end) {
            self.retire_code_tokens(start, end);
        }
    }

    /// Resolve the shared ledger for a whole range with one lookup, reusing a
    /// resident answer once its interval spans the range.
    ///
    /// A ledger answer describes a maximal uniform interval, so when it covers
    /// the range the range has one meaning: an `Owned` run proves a single
    /// mapping owns every byte, and a `Gap` proves no mapping touches it. Only
    /// a range crossing ledger boundaries falls back to the walking queries.
    #[inline]
    fn resolve_range_span(&mut self, address: u32, start: u64, end: u64) -> RangeSpan {
        // A resident answer is checked first: it is the hit path for the bulk
        // ranges that repeat frame after frame.
        if let Some(lookup) = self.route_lookup.covering(start, end) {
            return match lookup {
                SharedLookup::Owned(run) => RangeSpan::Owned(run),
                SharedLookup::Gap { .. } => RangeSpan::NoMapping,
            };
        }
        // On a miss the envelope and page filter reject a range no mapping can
        // reach before the ledger is walked.
        if !self.may_overlap_shared(start, end) {
            return RangeSpan::NoMapping;
        }
        let lookup = shared_lookup_at(self, address);
        self.route_lookup.store(lookup);
        if !lookup.covers(start, end) {
            return RangeSpan::Unresolved;
        }
        match lookup {
            SharedLookup::Owned(run) => RangeSpan::Owned(run),
            SharedLookup::Gap { .. } => RangeSpan::NoMapping,
        }
    }

    #[inline]
    fn overlaps_shared(&self, start: u64, end: u64) -> bool {
        self.may_overlap_shared(start, end)
            && self.shared_regions.iter().any(|mapping| {
                let mapping_start = u64::from(mapping.base);
                let mapping_end = mapping_start.saturating_add(mapping.region.len() as u64);
                start < mapping_end && mapping_start < end
            })
    }
}

#[derive(Debug, Default)]
pub struct GuestAddressSpace(Rc<UnsafeCell<GuestAddressSpaceState>>);

/// A shared view of one process address space.
///
/// CPU adapters retain this handle while the runner serializes their access.
/// Ordinary [`GuestAddressSpace::clone`] operations remain detached snapshots.
#[derive(Clone, Debug)]
pub(crate) struct SharedGuestAddressSpace(Rc<UnsafeCell<GuestAddressSpaceState>>);

const ADDRESS_SPACE_SIZE: u64 = 1u64 << 32;

#[inline]
fn mapping_end(base: u32, len: usize) -> Option<u64> {
    u64::from(base).checked_add(len as u64)
}

#[inline]
fn range_end(address: u32, len: usize) -> Option<u64> {
    u64::from(address)
        .checked_add(len as u64)
        .filter(|end| *end <= ADDRESS_SPACE_SIZE)
}

#[inline]
fn shared_mapping_at(
    state: &GuestAddressSpaceState,
    address: u32,
) -> Option<(&SharedRegionMapping, usize)> {
    if !state.may_overlap_shared(u64::from(address), u64::from(address) + 1) {
        return None;
    }
    state.shared_regions.iter().rev().find_map(|mapping| {
        let offset = usize::try_from(address.checked_sub(mapping.base)?).ok()?;
        (offset < mapping.region.len()).then_some((mapping, offset))
    })
}

/// Return whether the union of the supplied mappings covers `[start, end)`.
/// The mappings are intentionally scanned without sorting: there are normally
/// only a handful of process mappings, and repeatedly extending the furthest
/// end keeps this query allocation-free on the flat-RAM bulk paths.
#[inline]
fn ranges_cover_shared(state: &GuestAddressSpaceState, start: u64, end: u64) -> bool {
    let mut cursor = start;
    while cursor < end {
        let mut covered_end = cursor;
        for mapping in &state.shared_regions {
            let Some(mapping_end) = mapping_end(mapping.base, mapping.region.len()) else {
                continue;
            };
            let mapping_start = u64::from(mapping.base);
            if mapping_start <= cursor && cursor < mapping_end {
                covered_end = covered_end.max(mapping_end.min(end));
            }
        }
        if covered_end == cursor {
            return false;
        }
        cursor = covered_end;
    }
    true
}

/// The route for a range wholly inside one ledger run.
///
/// A run's span lies inside the single mapping that owns it, so that mapping's
/// writability decides the route outright. [`shared_range_route`] rediscovers
/// the same answer by walking every mapping boundary in the range.
#[inline]
fn shared_run_route(state: &GuestAddressSpaceState, run: SharedRun) -> GuestMemoryRoute {
    let Some(mapping) = state.shared_regions.get(run.index) else {
        return GuestMemoryRoute::Mixed;
    };
    debug_assert_eq!(
        mapping.writable,
        shared_range_route(state, run.start, run.end) == GuestMemoryRoute::Shared,
        "a ledger run must route exactly as the walking classifier"
    );
    if mapping.writable {
        GuestMemoryRoute::Shared
    } else {
        GuestMemoryRoute::SharedReadOnly
    }
}

/// Classify a completely shared range while retaining its write protection.
/// This walks mapping boundaries rather than bytes, so a large writable alias
/// still reaches the classic bus's bulk fast path.
#[inline]
fn shared_range_route(state: &GuestAddressSpaceState, start: u64, end: u64) -> GuestMemoryRoute {
    let mut cursor = start;
    let mut writable = None;
    while cursor < end {
        let address = u32::try_from(cursor).expect("guest range remains in 32-bit address space");
        let Some((mapping, _)) = shared_mapping_at(state, address) else {
            return GuestMemoryRoute::Mixed;
        };
        let Some(mapping_end) = mapping_end(mapping.base, mapping.region.len()) else {
            return GuestMemoryRoute::Mixed;
        };
        let mut segment_end = mapping_end.min(end);
        for newer in &state.shared_regions {
            let newer_start = u64::from(newer.base);
            if newer_start > cursor && newer_start < segment_end {
                segment_end = newer_start;
            }
        }
        if segment_end <= cursor {
            return GuestMemoryRoute::Mixed;
        }
        match writable {
            None => writable = Some(mapping.writable),
            Some(previous) if previous != mapping.writable => {
                return GuestMemoryRoute::Mixed;
            }
            Some(_) => {}
        }
        cursor = segment_end;
    }
    match writable {
        Some(true) => GuestMemoryRoute::Shared,
        Some(false) => GuestMemoryRoute::SharedReadOnly,
        None => GuestMemoryRoute::Mixed,
    }
}

/// Resolve the maximal span around `address` owned by one shared mapping.
///
/// The owner is the last mapping covering `address`. Only mappings appended
/// after it can shadow it, and none of those can contain `address` itself, so
/// each bounds the span below or above.
#[inline]
fn shared_run_at(state: &GuestAddressSpaceState, address: u32) -> Option<SharedRun> {
    if !state.may_overlap_shared(u64::from(address), u64::from(address) + 1) {
        return None;
    }
    let at = u64::from(address);
    let (index, owner) = state
        .shared_regions
        .iter()
        .enumerate()
        .rev()
        .find(|(_, mapping)| {
            address
                .checked_sub(mapping.base)
                .is_some_and(|offset| (offset as usize) < mapping.region.len())
        })?;
    let mut start = u64::from(owner.base);
    let mut end = mapping_end(owner.base, owner.region.len())?;
    for later in &state.shared_regions[index + 1..] {
        let later_start = u64::from(later.base);
        let Some(later_end) = mapping_end(later.base, later.region.len()) else {
            continue;
        };
        if later_start >= later_end {
            continue;
        }
        if later_end <= at {
            start = start.max(later_end);
        } else if later_start > at {
            end = end.min(later_start);
        }
    }
    (start <= at && at < end).then_some(SharedRun { index, start, end })
}

/// The maximal interval around `address` the ledger answers uniformly: the
/// span one mapping owns, or the gap between mappings.
#[inline]
fn shared_lookup_at(state: &GuestAddressSpaceState, address: u32) -> SharedLookup {
    let page_start =
        u64::from(address >> super::page_index::PAGE_SHIFT) << super::page_index::PAGE_SHIFT;
    let page_end = page_start + (1u64 << super::page_index::PAGE_SHIFT);
    if !state.shared_pages.may_overlap(page_start, page_end) {
        return SharedLookup::Gap {
            start: page_start,
            end: page_end,
        };
    }
    if let Some(run) = shared_run_at(state, address) {
        return SharedLookup::Owned(run);
    }
    // No mapping contains `address`, so each lies wholly below or above it
    // and bounds the empty interval on one side.
    let at = u64::from(address);
    let mut start = 0;
    let mut end = ADDRESS_SPACE_SIZE;
    for mapping in &state.shared_regions {
        let mapping_start = u64::from(mapping.base);
        let Some(mapping_end) = mapping_end(mapping.base, mapping.region.len()) else {
            continue;
        };
        if mapping_start >= mapping_end {
            continue;
        }
        if mapping_end <= at {
            start = start.max(mapping_end);
        } else if mapping_start > at {
            end = end.min(mapping_start);
        }
    }
    SharedLookup::Gap { start, end }
}

/// Read the four bytes at `start` out of the mapping `run` names. Equivalent
/// to the routed read, which classifies a wholly shared word as
/// `Shared`/`SharedReadOnly` and returns these same bytes.
#[inline]
fn read_shared_word(
    state: &GuestAddressSpaceState,
    run: SharedRun,
    start: u64,
) -> Option<[u8; 4]> {
    let mapping = &state.shared_regions[run.index];
    let offset = (start - u64::from(mapping.base)) as usize;
    let mut word = [0; 4];
    // SAFETY: see `read_shared_bytes`; the span proves the word lies wholly
    // inside this mapping.
    unsafe { mapping.region.read_into(offset, &mut word) }?;
    Some(word)
}

/// Call `run` for each maximal single-mapping span of `[address, address +
/// len)`, ascending.
///
/// The split mirrors [`shared_range_route`]: a byte is owned by the last
/// mapping covering it, and a mapping starting inside a span truncates it so
/// ownership is re-proved at the new cursor. Returns `None` once a byte is
/// unmapped or `run` rejects a span, so callers that must not partially
/// commit validate in one walk before acting in another.
#[inline]
fn for_each_shared_run(
    state: &GuestAddressSpaceState,
    address: u32,
    len: usize,
    mut run: impl FnMut(&SharedRegionMapping, usize, usize, usize) -> Option<()>,
) -> Option<()> {
    let end = range_end(address, len)?;
    let mut cursor = u64::from(address);
    let mut consumed = 0usize;
    while cursor < end {
        let at = u32::try_from(cursor).ok()?;
        let (mapping, offset) = shared_mapping_at(state, at)?;
        let mapping_end = mapping_end(mapping.base, mapping.region.len())?;
        let mut span_end = mapping_end.min(end);
        for other in &state.shared_regions {
            let other_start = u64::from(other.base);
            if other_start > cursor && other_start < span_end {
                span_end = other_start;
            }
        }
        if span_end <= cursor {
            return None;
        }
        let span_len = usize::try_from(span_end - cursor).ok()?;
        run(mapping, offset, consumed, span_len)?;
        consumed += span_len;
        cursor = span_end;
    }
    Some(())
}

/// Copy a wholly shared-mapped range into `dst`, one bulk copy per mapping
/// span; `None` when the range is not wholly shared. Equivalent to the
/// byte-wise routed read. `dst` may be partially written on `None`, so callers
/// fall back to a path that overwrites all of it.
fn read_shared_bytes(
    state: &GuestAddressSpaceState,
    address: u32,
    dst: &mut [u8],
) -> Option<()> {
    let len = dst.len();
    for_each_shared_run(state, address, len, |mapping, offset, consumed, span| {
        // SAFETY: all shared views are accessed only while their process
        // runner serializes the source allocation.
        unsafe { mapping.region.read_into(offset, &mut dst[consumed..consumed + span]) }
    })
}

#[inline]
fn ranges_cover_ordinary(state: &GuestAddressSpaceState, start: u64, end: u64) -> bool {
    let mut cursor = start;
    while cursor < end {
        let mut covered_end = cursor;
        for mapping in &state.ordinary_regions {
            let Some(mapping_end) = mapping_end(mapping.base, mapping.len) else {
                continue;
            };
            let mapping_start = u64::from(mapping.base);
            if mapping_start <= cursor && cursor < mapping_end {
                covered_end = covered_end.max(mapping_end.min(end));
            }
        }
        if covered_end == cursor {
            return false;
        }
        cursor = covered_end;
    }
    true
}

#[inline]
fn shared_ranges(state: &GuestAddressSpaceState) -> impl Iterator<Item = (u64, u64)> + '_ {
    state.shared_regions.iter().filter_map(|mapping| {
        Some((
            u64::from(mapping.base),
            mapping_end(mapping.base, mapping.region.len())?,
        ))
    })
}

#[inline]
fn ordinary_ranges(state: &GuestAddressSpaceState) -> impl Iterator<Item = (u64, u64)> + '_ {
    state.ordinary_regions.iter().filter_map(|mapping| {
        Some((
            u64::from(mapping.base),
            mapping_end(mapping.base, mapping.len)?,
        ))
    })
}

#[inline]
fn ranges_overlap<I>(start: u64, end: u64, mappings: I) -> bool
where
    I: IntoIterator<Item = (u64, u64)>,
{
    mappings
        .into_iter()
        .any(|(mapping_start, mapping_end)| start < mapping_end && mapping_start < end)
}

/// Select one backing for a single byte. This is the only precedence rule in
/// the memory subsystem: newest explicit shared aliases win, then ordinary
/// sparse mappings, then the optional classic flat-RAM fallback.
#[inline]
fn route_byte_state(
    state: &mut GuestAddressSpaceState,
    address: u32,
    flat_limit: Option<u32>,
) -> GuestMemoryRoute {
    if let Some((mapping, _)) = shared_mapping_at(state, address) {
        return if mapping.writable {
            GuestMemoryRoute::Shared
        } else {
            GuestMemoryRoute::SharedReadOnly
        };
    }
    if PpcMemory::read_u8(&mut state.regions, address).is_some() {
        return GuestMemoryRoute::Sparse;
    }
    if flat_limit.is_some_and(|limit| address < limit) {
        GuestMemoryRoute::Flat
    } else {
        GuestMemoryRoute::Unmapped
    }
}

/// Whether one routed byte accepts writes without changing guest memory.
/// Shared aliases expose their explicit protection bit; ordinary sparse
/// mappings delegate to the PPC region map's writable-span proof; the
/// optional classic fallback is writable whenever the byte lies in RAM.
#[inline]
fn routed_byte_is_writable_state(
    state: &mut GuestAddressSpaceState,
    address: u32,
    flat_limit: Option<u32>,
) -> bool {
    if let Some((mapping, _)) = shared_mapping_at(state, address) {
        return mapping.writable;
    }
    // A sparse byte is authoritative even when it lies below the classic
    // adapter's flat-RAM limit. In particular, a read-only PEF byte must not
    // fall through to that flat fallback merely because the writable-span
    // proof failed. The write-back probe below handles overlapping sparse
    // mappings, for which `writable_span` deliberately declines to return a
    // cached span.
    let Some(original) = PpcMemory::read_u8(&mut state.regions, address) else {
        return flat_limit.is_some_and(|limit| address < limit);
    };
    if state.regions.writable_span(address, 1).is_some() {
        return true;
    }
    // `PpcSectionMem`'s byte write performs the same newest-visible-region
    // protection check for overlapping mappings. Rewriting the original byte
    // leaves guest state unchanged while providing the needed writability
    // proof for callers that are still in their preflight phase.
    PpcMemory::write_u8(&mut state.regions, address, original).is_some()
}

/// Page granularity of the writable-code token generations. It matches the
/// shared-mapping filter so both sides of a write agree on what a page is.
const CODE_PAGE_SHIFT: u32 = super::page_index::PAGE_SHIFT;
const CODE_PAGE_COUNT: usize = super::page_index::PAGE_COUNT;

/// Slots in the direct-mapped token table. Executed code occupies a handful
/// of pages, so collisions are rare and only cost a re-decode.
const CODE_TOKEN_SLOTS: usize = 1024;
const CODE_TOKEN_INDEX_MASK: usize = CODE_TOKEN_SLOTS - 1;

/// A cached writable span, paired with the guest address it starts at.
///
/// [`PpcSectionMemSpan`] is opaque, so a write through one cannot say which
/// guest address it landed on. Carrying the base alongside it lets
/// [`GuestAddressSpace::write_u16_be_in_span`] report the write like any
/// other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuestWritableSpan {
    span: PpcSectionMemSpan,
    base: u32,
}

impl GuestWritableSpan {
    /// The guest address `relative_offset` bytes into the span.
    #[inline]
    fn address_of(self, relative_offset: usize) -> Option<u32> {
        u32::try_from(relative_offset)
            .ok()
            .and_then(|offset| self.base.checked_add(offset))
    }
}

/// One direct-mapped token slot. `token == 0` marks it vacant; the page is
/// stored so a slot holding a different page is re-minted rather than reused.
#[derive(Clone, Copy, Debug, Default)]
struct CodeTokenSlot {
    page: u32,
    token: u64,
}

/// Answer the instruction-cache token for an address served by sparse regions.
///
/// `PpcSectionMem` tokenizes only read-only regions, treating a writable one
/// as possibly self-modifying. Classic Mac OS has no memory protection and
/// CFM publishes fragments into the writable application heap, so that gate
/// refuses every instruction a PowerPC app executes. Issue our own token for
/// writable code and keep the hardware contract instead: code written by the
/// guest is stale until it flushes its instruction cache, and a write to a
/// page we have executed from rotates the generation on its own.
#[inline]
fn sparse_instruction_token(state: &mut GuestAddressSpaceState, addr: u32) -> Option<u64> {
    // The interpreter asks once per block start and once per word while it
    // builds one; a page's answer only changes with a rotation.
    let page = addr >> CODE_PAGE_SHIFT;
    if let Some((cached_page, token)) = state.instruction_token_cache {
        if cached_page == page {
            return Some(token);
        }
    }
    resolve_sparse_instruction_token(state, addr, page)
}

fn resolve_sparse_instruction_token(
    state: &mut GuestAddressSpaceState,
    addr: u32,
    page: u32,
) -> Option<u64> {
    if state.regions.writable_span(addr, 4).is_some() {
        let start = u64::from(addr);
        state.executed_pages.mark(start, start + 4);
        let token = state.writable_code_token(addr)?;
        // Only remember the answer when one writable region covers the whole
        // page. Otherwise the fast path above could hand out a token for an
        // address past that region's end.
        let page_start = page << CODE_PAGE_SHIFT;
        if state
            .regions
            .writable_span(page_start, 1 << CODE_PAGE_SHIFT)
            .is_some()
        {
            state.instruction_token_cache = Some((page, token));
        }
        return Some(token);
    }
    state.regions.instruction_cache_token(addr)
}

/// Select a backing for a contiguous access. Wide ranges avoid a byte loop so
/// a flat-RAM read/write can retain its single-slice fast path. A `Mixed`
/// result deliberately sends the adapter through its byte-granular path,
/// where each byte is resolved by the same `route_byte_state` rule.
#[inline]
fn route_range_state(
    state: &mut GuestAddressSpaceState,
    address: u32,
    len: usize,
    flat_limit: Option<u32>,
) -> GuestMemoryRoute {
    if len == 0 {
        return GuestMemoryRoute::Unmapped;
    }
    let Some(end) = range_end(address, len) else {
        return GuestMemoryRoute::Mixed;
    };
    let start = u64::from(address);

    // One ledger lookup classifies the whole range whenever a resident
    // interval already spans it: a run proves a single mapping owns every
    // byte, a gap proves no mapping touches the range. Both are exact, so the
    // walking overlap queries only run for a range crossing ledger
    // boundaries, which is the rare case on the flat-RAM bulk paths.
    match state.resolve_range_span(address, start, end) {
        RangeSpan::Owned(run) => return shared_run_route(state, run),
        RangeSpan::NoMapping => {}
        RangeSpan::Unresolved => {
            if state.overlaps_shared(start, end) {
                if ranges_cover_shared(state, start, end) {
                    return shared_range_route(state, start, end);
                }
                return GuestMemoryRoute::Mixed;
            }
        }
    }

    // Scalar CPU accesses dominate this path. Let the sparse region map prove
    // a wholly ordinary access directly before consulting the auxiliary
    // mapping ledger; otherwise every pixel/word read would rescan every PEF
    // section merely to rediscover the region that `PpcMemory` must locate
    // immediately afterward. A failed proof still falls through to the
    // ledger so mixed sparse/flat ranges retain byte-wise routing.
    let ordinary_scalar = match len {
        1 => PpcMemory::read_u8(&mut state.regions, address).is_some(),
        2 => PpcMemory::read_u16_be(&mut state.regions, address).is_some(),
        4 => PpcMemory::read_u32_be(&mut state.regions, address).is_some(),
        _ => false,
    };
    if ordinary_scalar {
        return GuestMemoryRoute::Sparse;
    }
    if flat_limit.is_none() && matches!(len, 1 | 2 | 4) {
        // Native scalar semantics are exactly the sparse map's semantics.
        // A failed direct read is therefore unmapped; there is no classic
        // fallback whose overlap would require consulting the range ledger.
        return GuestMemoryRoute::Unmapped;
    }

    let ordinary_overlap = ranges_overlap(start, end, ordinary_ranges(state));
    if ordinary_overlap {
        if ranges_cover_ordinary(state, start, end) {
            return GuestMemoryRoute::Sparse;
        }
        return GuestMemoryRoute::Mixed;
    }

    let Some(flat_limit) = flat_limit else {
        return GuestMemoryRoute::Unmapped;
    };
    let flat_end = u64::from(flat_limit);
    if end <= flat_end {
        GuestMemoryRoute::Flat
    } else if start < flat_end {
        GuestMemoryRoute::Mixed
    } else {
        GuestMemoryRoute::Unmapped
    }
}

/// Whether a range contains an ordinary sparse byte after explicit shared
/// aliases have taken precedence. Bulk classic operations use this to decide
/// whether their flat-RAM slice fast path is valid.
#[inline]
fn sparse_mapping_overlaps_state(state: &GuestAddressSpaceState, address: u32, len: usize) -> bool {
    if len == 0 {
        return false;
    }
    let Some(end) = range_end(address, len) else {
        return true;
    };
    let range_start = u64::from(address);
    for ordinary in &state.ordinary_regions {
        let Some(ordinary_end) = mapping_end(ordinary.base, ordinary.len) else {
            return true;
        };
        let mut cursor = u64::from(ordinary.base).max(range_start);
        let clipped_end = ordinary_end.min(end);
        while cursor < clipped_end {
            let mut shared_end = cursor;
            for shared in &state.shared_regions {
                let Some(mapping_end) = mapping_end(shared.base, shared.region.len()) else {
                    continue;
                };
                let mapping_start = u64::from(shared.base);
                if mapping_start <= cursor && cursor < mapping_end {
                    shared_end = shared_end.max(mapping_end.min(clipped_end));
                }
            }
            if shared_end == cursor {
                return true;
            }
            cursor = shared_end;
        }
    }
    false
}

/// Classify a range against a classic flat-RAM limit when no sparse process
/// view is attached. This keeps the adapter's detached mode on the exact same
/// route vocabulary as the attached mode.
#[inline]
pub(crate) fn flat_memory_route(address: u32, len: usize, flat_limit: u32) -> GuestMemoryRoute {
    if len == 0 {
        return GuestMemoryRoute::Unmapped;
    }
    let Some(end) = range_end(address, len) else {
        return GuestMemoryRoute::Mixed;
    };
    let start = u64::from(address);
    let flat_end = u64::from(flat_limit);
    if end <= flat_end {
        GuestMemoryRoute::Flat
    } else if start < flat_end {
        GuestMemoryRoute::Mixed
    } else {
        GuestMemoryRoute::Unmapped
    }
}

#[inline]
fn read_routed_u8_state(
    state: &mut GuestAddressSpaceState,
    address: u32,
    flat_limit: Option<u32>,
) -> Option<u8> {
    match route_byte_state(state, address, flat_limit) {
        GuestMemoryRoute::Shared | GuestMemoryRoute::SharedReadOnly => {
            let (mapping, offset) = shared_mapping_at(state, address)?;
            // SAFETY: all shared views are accessed only while their process
            // runner serializes the source allocation.
            unsafe { mapping.region.read(offset) }
        }
        GuestMemoryRoute::Sparse => PpcMemory::read_u8(&mut state.regions, address),
        GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped | GuestMemoryRoute::Mixed => None,
    }
}

#[inline]
fn read_routed_u16_state(
    state: &mut GuestAddressSpaceState,
    address: u32,
    flat_limit: Option<u32>,
) -> Option<u16> {
    let end = range_end(address, 2)?;
    if !state.overlaps_shared(u64::from(address), end) {
        return PpcMemory::read_u16_be(&mut state.regions, address);
    }
    let hi = read_routed_u8_state(state, address, flat_limit)?;
    let lo = read_routed_u8_state(state, address.wrapping_add(1), flat_limit)?;
    Some(u16::from_be_bytes([hi, lo]))
}

#[inline]
fn read_routed_u32_state(
    state: &mut GuestAddressSpaceState,
    address: u32,
    flat_limit: Option<u32>,
) -> Option<u32> {
    let end = range_end(address, 4)?;
    if !state.overlaps_shared(u64::from(address), end) {
        return PpcMemory::read_u32_be(&mut state.regions, address);
    }
    let b0 = read_routed_u8_state(state, address, flat_limit)?;
    let b1 = read_routed_u8_state(state, address.wrapping_add(1), flat_limit)?;
    let b2 = read_routed_u8_state(state, address.wrapping_add(2), flat_limit)?;
    let b3 = read_routed_u8_state(state, address.wrapping_add(3), flat_limit)?;
    Some(u32::from_be_bytes([b0, b1, b2, b3]))
}

#[inline]
fn write_routed_u8_state(
    state: &mut GuestAddressSpaceState,
    address: u32,
    value: u8,
    flat_limit: Option<u32>,
) -> Option<()> {
    let result = match route_byte_state(state, address, flat_limit) {
        GuestMemoryRoute::Shared | GuestMemoryRoute::SharedReadOnly => {
            let (mapping, offset) = shared_mapping_at(state, address)?;
            if !mapping.writable {
                return None;
            }
            // SAFETY: see `read_routed_u8_state`.
            unsafe { mapping.region.write(offset, value) }
        }
        GuestMemoryRoute::Sparse => PpcMemory::write_u8(&mut state.regions, address, value),
        GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped | GuestMemoryRoute::Mixed => None,
    };
    if result.is_some() {
        state.note_write(address, &[value]);
    }
    result
}

#[inline]
fn write_routed_u16_state(
    state: &mut GuestAddressSpaceState,
    address: u32,
    value: u16,
    flat_limit: Option<u32>,
) -> Option<()> {
    let end = range_end(address, 2)?;
    if !state.overlaps_shared(u64::from(address), end) {
        PpcMemory::write_u16_be(&mut state.regions, address, value)?;
        state.note_write(address, &value.to_be_bytes());
        return Some(());
    }
    let bytes = value.to_be_bytes();
    for offset in 0..bytes.len() {
        if !routed_byte_is_writable_state(state, address.wrapping_add(offset as u32), flat_limit) {
            return None;
        }
    }
    write_routed_u8_state(state, address, bytes[0], flat_limit)?;
    write_routed_u8_state(state, address.wrapping_add(1), bytes[1], flat_limit)
}

#[inline]
fn write_routed_u32_state(
    state: &mut GuestAddressSpaceState,
    address: u32,
    value: u32,
    flat_limit: Option<u32>,
) -> Option<()> {
    let end = range_end(address, 4)?;
    if !state.overlaps_shared(u64::from(address), end) {
        PpcMemory::write_u32_be(&mut state.regions, address, value)?;
        state.note_write(address, &value.to_be_bytes());
        return Some(());
    }
    let bytes = value.to_be_bytes();
    for offset in 0..bytes.len() {
        if !routed_byte_is_writable_state(state, address.wrapping_add(offset as u32), flat_limit) {
            return None;
        }
    }
    for (offset, byte) in bytes.into_iter().enumerate() {
        write_routed_u8_state(state, address.wrapping_add(offset as u32), byte, flat_limit)?;
    }
    Some(())
}

impl SharedGuestAddressSpace {
    pub(crate) fn set_presentation(&self, slot: super::presentation::PresentationSlot) {
        self.with_state_mut(|state| state.presentation = slot);
    }
    fn new(memory: &GuestAddressSpace) -> Self {
        Self(Rc::clone(&memory.0))
    }

    fn with_state_mut<R>(&self, f: impl FnOnce(&mut GuestAddressSpaceState) -> R) -> R {
        // SAFETY: the process runner serializes all CPU-adapter access. Keeping
        // the reference inside this operation prevents it from escaping the
        // shared handle or remaining live across a later adapter operation.
        unsafe { f(&mut *self.0.get()) }
    }

    fn adapter(&self) -> GuestAddressSpace {
        GuestAddressSpace(Rc::clone(&self.0))
    }

    /// Ask the process-owned router which backing owns an access. The optional
    /// flat limit is supplied by the classic adapter; native PPC views omit it
    /// because their flat aliases are represented by `Shared` mappings.
    #[inline]
    pub(crate) fn route(
        &self,
        address: u32,
        len: usize,
        flat_limit: Option<u32>,
    ) -> GuestMemoryRoute {
        self.with_state_mut(|state| route_range_state(state, address, len, flat_limit))
    }

    #[inline]
    pub(crate) fn route_byte(&self, address: u32, flat_limit: Option<u32>) -> GuestMemoryRoute {
        self.with_state_mut(|state| route_byte_state(state, address, flat_limit))
    }

    /// Whether a complete range belongs to one runtime-owned read-only shared
    /// mapping. Trap Manager uses this as provenance for privileged permanent
    /// come-from links; matching bytes in ordinary guest mappings are never
    /// sufficient.
    #[inline]
    pub(crate) fn is_shared_readonly_range(&self, address: u32, len: usize) -> bool {
        self.route(address, len, None) == GuestMemoryRoute::SharedReadOnly
    }

    /// Prove that a wholly writable shared range is the classic adapter's
    /// own flat allocation at the same offsets. A same-address alias backed
    /// by another bus must remain `Shared`, even when it lies below the local
    /// RAM limit.
    #[inline]
    pub(crate) fn shared_range_is_local_flat(
        &self,
        address: u32,
        len: usize,
        local_ram: &SharedRamRegion,
    ) -> bool {
        self.with_state_mut(|state| {
            if len == 0 {
                return false;
            }
            let Some(end) = range_end(address, len) else {
                return false;
            };
            // One lookup answers the mirror case outright: a ledger run that
            // spans the range names the single mapping that has to be the
            // local alias, where the loop below would rediscover it segment by
            // segment after routing the same range a second time.
            if let RangeSpan::Owned(run) =
                state.resolve_range_span(address, u64::from(address), end)
            {
                return state.shared_regions.get(run.index).is_some_and(|mapping| {
                    mapping.writable
                        && mapping.region.same_backing(local_ram)
                        && mapping.region.backing_offset() == mapping.base as usize
                });
            }
            if route_range_state(state, address, len, None) != GuestMemoryRoute::Shared {
                return false;
            }
            let mut cursor = u64::from(address);
            while cursor < end {
                let guest = u32::try_from(cursor).expect("guest range remains 32-bit");
                let Some((mapping, _)) = shared_mapping_at(state, guest) else {
                    return false;
                };
                if !mapping.region.same_backing(local_ram)
                    || mapping.region.backing_offset() != mapping.base as usize
                {
                    return false;
                }
                let Some(mapping_end) = mapping_end(mapping.base, mapping.region.len()) else {
                    return false;
                };
                let mut segment_end = mapping_end.min(end);
                for newer in &state.shared_regions {
                    let newer_start = u64::from(newer.base);
                    if newer_start > cursor && newer_start < segment_end {
                        segment_end = newer_start;
                    }
                }
                if segment_end <= cursor {
                    return false;
                }
                cursor = segment_end;
            }
            true
        })
    }

    /// Read from a mapped non-flat backing selected by the shared router.
    /// `None` means that the route belongs to the classic adapter's local flat
    /// RAM or is unmapped; the caller must not fall through when the route is
    /// `Shared`/`Sparse` because read-only mappings are still authoritative.
    #[inline]
    pub(crate) fn read_routed_u8(&self, address: u32, flat_limit: Option<u32>) -> Option<u8> {
        self.with_state_mut(|state| read_routed_u8_state(state, address, flat_limit))
    }

    #[inline]
    pub(crate) fn read_routed_u16(&self, address: u32, flat_limit: Option<u32>) -> Option<u16> {
        self.with_state_mut(|state| read_routed_u16_state(state, address, flat_limit))
    }

    #[inline]
    pub(crate) fn read_routed_u32(&self, address: u32, flat_limit: Option<u32>) -> Option<u32> {
        self.with_state_mut(|state| read_routed_u32_state(state, address, flat_limit))
    }

    #[inline]
    pub(crate) fn write_routed_u8(
        &self,
        address: u32,
        value: u8,
        flat_limit: Option<u32>,
    ) -> Option<()> {
        self.with_state_mut(|state| write_routed_u8_state(state, address, value, flat_limit))
    }

    #[inline]
    pub(crate) fn write_routed_u16(
        &self,
        address: u32,
        value: u16,
        flat_limit: Option<u32>,
    ) -> Option<()> {
        self.with_state_mut(|state| write_routed_u16_state(state, address, value, flat_limit))
    }

    #[inline]
    pub(crate) fn write_routed_u32(
        &self,
        address: u32,
        value: u32,
        flat_limit: Option<u32>,
    ) -> Option<()> {
        self.with_state_mut(|state| write_routed_u32_state(state, address, value, flat_limit))
    }

    #[inline]
    pub(crate) fn routed_byte_is_writable(&self, address: u32, flat_limit: Option<u32>) -> bool {
        self.with_state_mut(|state| routed_byte_is_writable_state(state, address, flat_limit))
    }

    /// Whether an address belongs to an ordinary sparse native mapping rather
    /// than a runner-owned shared flat-RAM overlay.
    #[inline]
    #[cfg(test)]
    pub(crate) fn is_ordinary_sparse_mapped(&self, address: u32) -> bool {
        self.route_byte(address, None) == GuestMemoryRoute::Sparse
    }

    pub(crate) fn sparse_mapping_overlaps(&self, address: u32, len: u32) -> bool {
        self.with_state_mut(|state| sparse_mapping_overlaps_state(state, address, len as usize))
    }

    /// Return the end of the highest read-only runtime reservation overlapping
    /// a candidate native heap allocation.
    #[inline]
    pub(crate) fn readonly_allocation_overlap_end(&self, address: u32, len: u32) -> Option<u32> {
        self.adapter().readonly_allocation_overlap_end(address, len)
    }

    /// Write bytes only through the attached guest address space.
    #[inline]
    pub(crate) fn write_bytes(&self, address: u32, bytes: &[u8]) -> Option<()> {
        self.adapter().write_bytes(address, bytes)
    }

    /// Exclusively borrow the retained process address space for one operation.
    ///
    /// The runner serializes this access with both CPU adapters.
    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(&mut GuestAddressSpace) -> R) -> R {
        f(&mut self.adapter())
    }
}

impl Clone for GuestAddressSpace {
    fn clone(&self) -> Self {
        let state = self.state();
        Self(Rc::new(UnsafeCell::new(GuestAddressSpaceState {
            regions: state.regions.clone(),
            presentation: Default::default(),
            ordinary_regions: state.ordinary_regions.clone(),
            shared_bounds: state.shared_bounds.clone(),
            shared_pages: state.shared_pages.clone(),
            shared_regions: state
                .shared_regions
                .iter()
                .map(|mapping| SharedRegionMapping {
                    base: mapping.base,
                    region: mapping.region.detached_clone(),
                    writable: mapping.writable,
                    code_isa: mapping.code_isa,
                })
                .collect(),
            readonly_allocation_exclusions: state.readonly_allocation_exclusions.clone(),
            // Detached regions are fresh allocations; let the clone re-resolve.
            instruction_lookup: LookupCache::default(),
            data_lookup: LookupCache::default(),
            route_lookup: LookupCache::default(),
            executed_pages: state.executed_pages.clone(),
            // A detached clone holds independent copies of the same regions,
            // which then diverge. Inheriting its parent's tokens would let a
            // CPU that has run both reuse blocks decoded from the other's
            // memory, so the clone mints its own from the first fetch.
            page_tokens: None,
            // Detached regions are fresh allocations; let the clone re-resolve.
            instruction_token_cache: None,
        })))
    }
}

impl GuestAddressSpace {
    pub(crate) fn presentation(&self) -> super::presentation::PresentationSlot {
        self.state().presentation.clone()
    }
    fn state(&self) -> &GuestAddressSpaceState {
        // SAFETY: the process runner serializes all CPU-adapter access, and
        // detached clones allocate independent state.
        unsafe { &*self.0.get() }
    }

    fn state_mut(&mut self) -> &mut GuestAddressSpaceState {
        // SAFETY: see `state`.
        unsafe { &mut *self.0.get() }
    }

    #[inline]
    fn route(&self, address: u32, len: usize, flat_limit: Option<u32>) -> GuestMemoryRoute {
        route_range_state(
            // SAFETY: callers use the address-space through one serialized
            // CPU adapter at a time; detached clones own independent state.
            unsafe { &mut *self.0.get() },
            address,
            len,
            flat_limit,
        )
    }

    #[inline]
    fn route_byte(&self, address: u32, flat_limit: Option<u32>) -> GuestMemoryRoute {
        route_byte_state(
            // SAFETY: see `route`.
            unsafe { &mut *self.0.get() },
            address,
            flat_limit,
        )
    }

    /// Construct an empty address space.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a process-lifetime view for another serialized CPU adapter.
    pub(crate) fn shared_view(&self) -> SharedGuestAddressSpace {
        SharedGuestAddressSpace::new(self)
    }

    /// Map a writable region. Newer mappings take precedence over overlaps.
    pub fn add_region(&mut self, base: u32, bytes: Vec<u8>) {
        let state = self.state_mut();
        state.ordinary_regions.push(OrdinaryRegionMapping {
            base,
            len: bytes.len(),
        });
        state.regions.add_region(base, bytes);
        state.retire_all_code_tokens();
    }

    /// Map a read-only region. Newer mappings take precedence over overlaps.
    pub fn add_readonly_region(&mut self, base: u32, bytes: Vec<u8>) {
        let state = self.state_mut();
        state.ordinary_regions.push(OrdinaryRegionMapping {
            base,
            len: bytes.len(),
        });
        state.regions.add_readonly_region(base, bytes);
        state.retire_all_code_tokens();
    }

    /// Publish runtime-generated code with system provenance in owned storage.
    /// Application code remains an ordinary mapping; it cannot acquire this
    /// provenance by presenting matching bytes. Existing mappings and empty or
    /// wrapping ranges are refused before changing the address space.
    pub(crate) fn publish_system_code(
        &mut self,
        isa: GuestIsa,
        base: u32,
        bytes: Vec<u8>,
    ) -> Option<()> {
        let len = u32::try_from(bytes.len()).ok()?;
        if len == 0 || range_end(base, bytes.len()).is_none() || self.mapping_overlaps(base, len) {
            return None;
        }
        let region = SharedRamRegion::from_owned_bytes(bytes);
        // SAFETY: this new allocation has no external accessor. Subsequent
        // views use the existing serialized address-space access contract.
        unsafe { self.add_shared_readonly_region(Some(isa), base, region) };
        Some(())
    }

    /// Return the disjoint holes not occupied by ordinary or shared mappings in
    /// the supplied half-open range.
    pub(crate) fn mapping_holes(&self, start: u32, end: u32) -> Vec<(u32, u32)> {
        if start >= end {
            return Vec::new();
        }

        let state = self.state();
        let mut occupied = ordinary_ranges(state)
            .chain(shared_ranges(state))
            .filter_map(|(base, limit)| {
                let mapping_start = base.max(u64::from(start));
                let mapping_end = limit.min(u64::from(end));
                (mapping_start < mapping_end).then_some((mapping_start, mapping_end))
            })
            .collect::<Vec<_>>();
        occupied.sort_unstable_by_key(|&(mapping_start, _)| mapping_start);

        let mut holes = Vec::new();
        let mut cursor = u64::from(start);
        for (mapping_start, mapping_end) in occupied {
            if cursor < mapping_start {
                holes.push((cursor as u32, mapping_start as u32));
            }
            cursor = cursor.max(mapping_end);
        }
        if cursor < u64::from(end) {
            holes.push((cursor as u32, end));
        }
        holes
    }

    /// Return the disjoint occupied spans of ordinary and shared mappings.
    /// Callers reserve these spans before adding process RAM overlays so that
    /// the classic heap cannot select native PEF or generated system code.
    /// Inside Macintosh: Memory (1992), pp. 2-19--2-21.
    pub(crate) fn mapping_ranges(&self) -> Vec<(u32, u32)> {
        let state = self.state();
        let mut ranges = ordinary_ranges(state)
            .chain(shared_ranges(state))
            .filter_map(|(base, end)| Some((u32::try_from(base).ok()?, u32::try_from(end).ok()?)))
            .collect::<Vec<_>>();
        ranges.sort_unstable_by_key(|&(base, _)| base);

        let mut merged = Vec::new();
        for (base, end) in ranges {
            if let Some((_, merged_end)) = merged.last_mut() {
                if base <= *merged_end {
                    *merged_end = (*merged_end).max(end);
                    continue;
                }
            }
            merged.push((base, end));
        }
        merged
    }

    /// Retire every cached decode of writable guest code.
    ///
    /// The tokens minted by `sparse_instruction_token` stand in for the
    /// instruction cache the guest flushes with `MakeDataExecutable` or
    /// `FlushCodeCache`.
    pub(crate) fn flush_instruction_cache(&mut self) {
        self.state_mut().retire_all_code_tokens();
    }

    /// Overlay a runner-owned RAM range without copying it.
    ///
    /// Shared mappings remain authoritative over ordinary sparse mappings so
    /// both CPU adapters observe the same system-scoped bytes immediately.
    ///
    /// # Safety
    ///
    /// The address space and source bus must remain under one owner that
    /// serializes all access. No source-bus slice or fast-memory window may be
    /// used while this address space mutates the shared allocation.
    pub(crate) unsafe fn add_shared_region(&mut self, base: u32, region: SharedRamRegion) {
        self.state_mut().push_shared_mapping(SharedRegionMapping {
            base,
            region,
            writable: true,
            code_isa: None,
        });
    }

    /// Overlay runner-owned system code without allowing ordinary guest
    /// writes. `code_isa` identifies code and its transition vectors only when
    /// the publishing owner supplies that identity; generic protected data uses
    /// `None`. Trap Manager uses the privileged writer below when it must
    /// update the protected exit of a permanent come-from head.
    ///
    /// # Safety
    ///
    /// The ownership and serialization requirements are the same as for
    /// [`Self::add_shared_region`].
    pub(crate) unsafe fn add_shared_readonly_region(
        &mut self,
        code_isa: Option<GuestIsa>,
        base: u32,
        region: SharedRamRegion,
    ) {
        let state = self.state_mut();
        if let Ok(len) = u32::try_from(region.len()) {
            state
                .readonly_allocation_exclusions
                .retain(|&(excluded_base, excluded_len)| {
                    (excluded_base, excluded_len) != (base, len)
                });
        }
        state.push_shared_mapping(SharedRegionMapping {
            base,
            region,
            writable: false,
            code_isa,
        });
    }

    /// Reserve a range from native allocation before its runner-owned shared
    /// mapping is attached. The exclusion survives detached launch-state
    /// clones and is replaced by the real mapping at runner initialization.
    pub(crate) fn add_readonly_allocation_exclusion(&mut self, base: u32, len: u32) -> Option<()> {
        if len == 0 || u64::from(base) + u64::from(len) > (1u64 << 32) {
            return None;
        }
        let state = self.state_mut();
        if !state.readonly_allocation_exclusions.contains(&(base, len)) {
            state.readonly_allocation_exclusions.push((base, len));
        }
        Some(())
    }

    pub(crate) fn has_readonly_allocation_exclusion(&self, base: u32, len: u32) -> bool {
        self.state()
            .readonly_allocation_exclusions
            .contains(&(base, len))
    }

    /// Whether an ordinary sparse mapping already occupies any byte in the
    /// supplied non-wrapping range. Shared overlays are intentionally ignored.
    pub(crate) fn ordinary_mapping_overlaps(&self, base: u32, len: u32) -> bool {
        if len == 0 || u64::from(base) + u64::from(len) > (1u64 << 32) {
            return false;
        }
        let start = u64::from(base);
        let end = start + u64::from(len);
        self.state().ordinary_regions.iter().any(|mapping| {
            let mapping_start = u64::from(mapping.base);
            let mapping_end = mapping_start.saturating_add(mapping.len as u64);
            start < mapping_end && mapping_start < end
        })
    }

    /// Whether any ordinary or shared backing occupies the requested range.
    /// Allocation exclusions alone are reservations, not published mappings.
    pub(crate) fn mapping_overlaps(&self, base: u32, len: u32) -> bool {
        if len == 0 || u64::from(base) + u64::from(len) > (1u64 << 32) {
            return false;
        }
        self.ordinary_mapping_overlaps(base, len)
            || self.state().shared_regions.iter().any(|mapping| {
                let start = u64::from(mapping.base);
                let end = start + mapping.region.len() as u64;
                u64::from(base) < end && start < u64::from(base) + u64::from(len)
            })
    }

    /// Write a big-endian long through a shared mapping regardless of its
    /// guest write protection. This is intentionally restricted to runtime
    /// services that own the mapped system bytes.
    pub(crate) fn write_shared_system_u32_be(&mut self, address: u32, value: u32) -> Option<()> {
        if self.route(address, 4, None) != GuestMemoryRoute::SharedReadOnly {
            return None;
        }
        for (offset, byte) in value.to_be_bytes().into_iter().enumerate() {
            let (mapping, relative) =
                self.locate_shared_mapping(address.checked_add(offset as u32)?)?;
            // SAFETY: shared mappings can only be installed by the serialized
            // process runner, and this method does not retain a source view.
            unsafe { mapping.region.write(relative, byte)? };
        }
        Some(())
    }

    /// ISA supplied by the owner of the selected system mapping. Protection
    /// alone does not identify an instruction set, and ordinary mappings never
    /// acquire this metadata by presenting matching bytes.
    pub(crate) fn system_code_isa(&self, address: u32) -> Option<GuestIsa> {
        self.locate_shared_mapping(address)
            .and_then(|(mapping, _)| mapping.code_isa)
    }

    /// Return the highest end address among staged allocation exclusions or
    /// live read-only shared mappings that overlap the supplied range.
    pub(crate) fn readonly_allocation_overlap_end(&self, address: u32, len: u32) -> Option<u32> {
        if len == 0 {
            return None;
        }
        let start = u64::from(address);
        let end = start.checked_add(u64::from(len))?;
        let state = self.state();
        let exclusion_ends =
            state
                .readonly_allocation_exclusions
                .iter()
                .filter_map(|&(base, len)| {
                    let mapping_start = u64::from(base);
                    let mapping_end = mapping_start.checked_add(u64::from(len))?;
                    (start < mapping_end && mapping_start < end).then_some(mapping_end)
                });
        let shared_ends = state
            .shared_regions
            .iter()
            .filter(|mapping| !mapping.writable)
            .filter_map(|mapping| {
                let mapping_start = u64::from(mapping.base);
                let mapping_end = mapping_start.checked_add(mapping.region.len() as u64)?;
                (start < mapping_end && mapping_start < end).then_some(mapping_end)
            });
        exclusion_ends
            .chain(shared_ends)
            .max()
            .map(|mapping_end| u32::try_from(mapping_end).unwrap_or(u32::MAX))
    }

    /// Return the total and largest contiguous byte counts remaining in a
    /// half-open range after clipping and unioning staged exclusions and live
    /// read-only shared mappings.
    pub(crate) fn readonly_allocation_available_bytes(&self, start: u32, end: u32) -> (u32, u32) {
        if start >= end {
            return (0, 0);
        }
        let range_start = u64::from(start);
        let range_end = u64::from(end);
        let state = self.state();
        let excluded = state
            .readonly_allocation_exclusions
            .iter()
            .filter_map(|&(base, len)| {
                let mapping_start = u64::from(base).max(range_start);
                let mapping_end = u64::from(base).checked_add(u64::from(len))?.min(range_end);
                (mapping_start < mapping_end).then_some((mapping_start, mapping_end))
            });
        let shared = state
            .shared_regions
            .iter()
            .filter(|mapping| !mapping.writable)
            .filter_map(|mapping| {
                let mapping_start = u64::from(mapping.base).max(range_start);
                let mapping_end = u64::from(mapping.base)
                    .checked_add(mapping.region.len() as u64)?
                    .min(range_end);
                (mapping_start < mapping_end).then_some((mapping_start, mapping_end))
            });
        let mut reserved = excluded.chain(shared).collect::<Vec<_>>();
        reserved.sort_unstable_by_key(|&(mapping_start, _)| mapping_start);

        let mut available_start = range_start;
        let mut total = 0u64;
        let mut largest = 0u64;
        for (mapping_start, mapping_end) in reserved {
            if mapping_start > available_start {
                let available = mapping_start - available_start;
                total += available;
                largest = largest.max(available);
            }
            available_start = available_start.max(mapping_end);
        }
        if available_start < range_end {
            let available = range_end - available_start;
            total += available;
            largest = largest.max(available);
        }

        (total as u32, largest as u32)
    }

    /// Return the number of mapped regions.
    pub fn region_count(&self) -> usize {
        let state = self.state();
        state.regions.region_count() + state.shared_regions.len()
    }

    /// Copy a fully mapped range into `dst`.
    pub fn read_bytes_into(&mut self, addr: u32, dst: &mut [u8]) -> Option<()> {
        if dst.is_empty() {
            return Some(());
        }
        let end = range_end(addr, dst.len())?;
        let state = self.state_mut();
        if !state.overlaps_shared(u64::from(addr), end) {
            return state.regions.read_bytes_into(addr, dst);
        }
        // The hot case: a wholly shared range, copied in bulk per span. This
        // also skips the separate `route_range_state` coverage walk; only a
        // range the walk rejects needs the classification below.
        if read_shared_bytes(state, addr, dst).is_some() {
            return Some(());
        }
        match route_range_state(state, addr, dst.len(), None) {
            GuestMemoryRoute::Sparse => state.regions.read_bytes_into(addr, dst),
            GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly
            | GuestMemoryRoute::Mixed => {
                for (offset, byte) in dst.iter_mut().enumerate() {
                    *byte = self.read_u8(addr.wrapping_add(offset as u32))?;
                }
                Some(())
            }
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    /// Copy `src` into a fully mapped, writable range.
    pub fn write_bytes(&mut self, addr: u32, src: &[u8]) -> Option<()> {
        if src.is_empty() {
            return Some(());
        }
        let end = range_end(addr, src.len())?;
        let state = self.state_mut();
        if !state.overlaps_shared(u64::from(addr), end) {
            state.regions.write_bytes(addr, src)?;
            state.note_write(addr, src);
            return Some(());
        }
        // Wholly shared and writable: preflight every span before copying
        // any, so a read-only span cannot leave a partial store behind.
        // Presentation sees one call, as on the sparse path below.
        let writable_shared = for_each_shared_run(state, addr, src.len(), |mapping, _, _, _| {
            mapping.writable.then_some(())
        })
        .is_some();
        if writable_shared {
            for_each_shared_run(state, addr, src.len(), |mapping, offset, consumed, span| {
                // SAFETY: see `read_shared_bytes`; the preflight walk above
                // proved every span of this range writable.
                unsafe {
                    mapping
                        .region
                        .write_from(offset, &src[consumed..consumed + span])
                }
            })
            .expect("preflighted shared range remains mapped and writable");
            state.note_write(addr, src);
            return Some(());
        }
        match route_range_state(state, addr, src.len(), None) {
            GuestMemoryRoute::Sparse => {
                state.regions.write_bytes(addr, src)?;
                state.note_write(addr, src);
                return Some(());
            }
            GuestMemoryRoute::Shared => {
                // Preflight all bytes so a read-only shared mapping cannot
                // leave a partially committed multi-byte store behind.
                for offset in 0..src.len() {
                    let address = addr.wrapping_add(offset as u32);
                    if self
                        .locate_shared_mapping(address)
                        .is_none_or(|(mapping, _)| !mapping.writable)
                    {
                        return None;
                    }
                }
                for (offset, byte) in src.iter().copied().enumerate() {
                    self.write_u8(addr.wrapping_add(offset as u32), byte)
                        .expect("located shared byte remains mapped");
                }
                return Some(());
            }
            GuestMemoryRoute::SharedReadOnly
            | GuestMemoryRoute::Flat
            | GuestMemoryRoute::Unmapped => return None,
            GuestMemoryRoute::Mixed => {}
        }

        let mut ordinary = Vec::new();
        let mut shared = Vec::new();
        for (offset, byte) in src.iter().copied().enumerate() {
            let address = addr.wrapping_add(offset as u32);
            match self.route_byte(address, None) {
                GuestMemoryRoute::Shared | GuestMemoryRoute::SharedReadOnly => {
                    if self
                        .locate_shared_mapping(address)
                        .is_some_and(|(mapping, _)| !mapping.writable)
                    {
                        return None;
                    }
                    shared.push((address, byte));
                }
                GuestMemoryRoute::Sparse => {
                    ordinary.push((address, self.state_mut().regions.read_u8(address)?, byte));
                }
                GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped | GuestMemoryRoute::Mixed => {
                    return None;
                }
            }
        }

        for (committed, &(address, _, byte)) in ordinary.iter().enumerate() {
            if self.state_mut().regions.write_u8(address, byte).is_none() {
                for &(rollback_address, original, _) in ordinary[..committed].iter().rev() {
                    self.state_mut()
                        .regions
                        .write_u8(rollback_address, original)
                        .expect("a previously writable sparse byte remains writable");
                }
                return None;
            }
        }
        for (address, byte) in shared {
            self.write_u8(address, byte)
                .expect("located shared byte remains mapped");
        }
        Some(())
    }

    /// Verify that a non-wrapping range is mapped and writable without
    /// changing its bytes. The write-back is deliberately byte-identical;
    /// it exercises the same protection and shared-overlay path used by the
    /// eventual commit while retaining atomicity for each chunk.
    pub(crate) fn preflight_writable_range(&mut self, addr: u32, len: u32) -> bool {
        if len == 0 {
            return true;
        }
        if u64::from(addr) + u64::from(len) > (1u64 << 32) {
            return false;
        }
        const PREFLIGHT_CHUNK: u32 = 4096;
        let mut offset = 0;
        while offset < len {
            let chunk_len = (len - offset).min(PREFLIGHT_CHUNK) as usize;
            let address = addr + offset;
            let mut bytes = vec![0; chunk_len];
            if self.read_bytes_into(address, &mut bytes).is_none()
                || self.write_bytes(address, &bytes).is_none()
            {
                return false;
            }
            offset += chunk_len as u32;
        }
        true
    }

    /// Publish discontiguous semantic outputs only after every byte is writable.
    /// The exclusive view remains held and no guest code runs during this commit.
    pub(crate) fn try_write_ranges_atomic(&mut self, writes: &[(u32, &[u8])]) -> bool {
        if writes.iter().any(|(address, bytes)| {
            u32::try_from(bytes.len())
                .ok()
                .is_none_or(|len| !self.preflight_writable_range(*address, len))
        }) {
            return false;
        }
        for (address, bytes) in writes {
            self.write_bytes(*address, bytes)
                .expect("preflighted semantic output remains writable");
        }
        true
    }

    /// Return a cached writable span contained in one mapped region.
    pub fn writable_span(&mut self, addr: u32, len: usize) -> Option<GuestWritableSpan> {
        if self.state().presentation.observes(addr, len) {
            return None;
        }
        if self.route(addr, len, None) != GuestMemoryRoute::Sparse {
            return None;
        }
        let span = self.state_mut().regions.writable_span(addr, len)?;
        Some(GuestWritableSpan { span, base: addr })
    }

    /// Read a big-endian word at an offset within a cached span.
    pub fn read_u16_be_in_span(
        &self,
        span: GuestWritableSpan,
        relative_offset: usize,
    ) -> Option<u16> {
        self.state()
            .regions
            .read_u16_be_in_span(span.span, relative_offset)
    }

    /// Write a big-endian word at an offset within a cached writable span.
    ///
    /// A span bypasses routing, but it may not bypass the observers of guest
    /// memory: a span taken over a page before anything executed from it is
    /// still a way to rewrite code that is executed later. The write is
    /// reported exactly as a routed one, so an executed page is retired here
    /// too.
    pub fn write_u16_be_in_span(
        &mut self,
        span: GuestWritableSpan,
        relative_offset: usize,
        value: u16,
    ) -> Option<()> {
        let addr = span.address_of(relative_offset)?;
        let state = self.state_mut();
        state
            .regions
            .write_u16_be_in_span(span.span, relative_offset, value)?;
        state.note_write(addr, &value.to_be_bytes());
        Some(())
    }

    #[inline]
    fn bus_fault(address: u32) -> BusFault {
        BusFault {
            kind: BusFaultKind::BusError,
            address,
        }
    }

    #[inline]
    fn locate_shared_mapping(&self, addr: u32) -> Option<(&SharedRegionMapping, usize)> {
        shared_mapping_at(self.state(), addr)
    }
}

impl PpcMemory for GuestAddressSpace {
    #[inline]
    fn read_u8(&mut self, addr: u32) -> Option<u8> {
        let state = self.state_mut();
        let start = u64::from(addr);
        if !state.overlaps_shared(start, start + 1) {
            return PpcMemory::read_u8(&mut state.regions, addr);
        }
        read_routed_u8_state(state, addr, None)
    }

    #[inline]
    fn read_u16_be(&mut self, addr: u32) -> Option<u16> {
        let Some(end) = range_end(addr, 2) else {
            return None;
        };
        let state = self.state_mut();
        let start = u64::from(addr);
        let lookup = state.data_lookup.resolve(state, addr, start, end);
        state.data_lookup.store(lookup);
        if lookup.covers(start, end) {
            return match lookup {
                SharedLookup::Gap { .. } => PpcMemory::read_u16_be(&mut state.regions, addr),
                SharedLookup::Owned(run) => {
                    let mapping = &state.shared_regions[run.index];
                    let offset = (start - u64::from(mapping.base)) as usize;
                    let mut bytes = [0; 2];
                    // SAFETY: the cached span proves both bytes have this owner.
                    unsafe { mapping.region.read_into(offset, &mut bytes) }?;
                    Some(u16::from_be_bytes(bytes))
                }
            };
        }
        match route_range_state(state, addr, 2, None) {
            GuestMemoryRoute::Sparse
            | GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly => read_routed_u16_state(state, addr, None),
            GuestMemoryRoute::Mixed => {
                let mut bytes = [0; 2];
                self.read_bytes_into(addr, &mut bytes)?;
                Some(u16::from_be_bytes(bytes))
            }
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    #[inline]
    fn read_u32_be(&mut self, addr: u32) -> Option<u32> {
        let Some(end) = range_end(addr, 4) else {
            return None;
        };
        let state = self.state_mut();
        let start = u64::from(addr);
        let lookup = state.data_lookup.resolve(state, addr, start, end);
        state.data_lookup.store(lookup);
        if lookup.covers(start, end) {
            return match lookup {
                SharedLookup::Gap { .. } => PpcMemory::read_u32_be(&mut state.regions, addr),
                SharedLookup::Owned(run) => {
                    read_shared_word(state, run, start).map(u32::from_be_bytes)
                }
            };
        }
        // No shared mapping overlaps this word, so the sparse region map is the
        // only backing it can have: the native PPC adapter supplies no flat
        // fallback, and a scalar read that the map cannot serve is unmapped.
        // Reading once here replaces the router's proof-read followed by this
        // same read, which would resolve the region map twice per access.
        if !state.overlaps_shared(start, end) {
            return PpcMemory::read_u32_be(&mut state.regions, addr);
        }
        match route_range_state(state, addr, 4, None) {
            GuestMemoryRoute::Sparse
            | GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly => read_routed_u32_state(state, addr, None),
            GuestMemoryRoute::Mixed => {
                let mut bytes = [0; 4];
                self.read_bytes_into(addr, &mut bytes)?;
                Some(u32::from_be_bytes(bytes))
            }
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    #[inline]
    fn read_u64_be(&mut self, addr: u32) -> Option<u64> {
        let Some(end) = range_end(addr, 8) else {
            return None;
        };
        let state = self.state_mut();
        if !state.overlaps_shared(u64::from(addr), end) {
            return state.regions.read_u64_be(addr);
        }
        match route_range_state(state, addr, 8, None) {
            GuestMemoryRoute::Sparse => state.regions.read_u64_be(addr),
            GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly
            | GuestMemoryRoute::Mixed => {
                let mut bytes = [0; 8];
                self.read_bytes_into(addr, &mut bytes)?;
                Some(u64::from_be_bytes(bytes))
            }
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    #[inline]
    fn read_instruction_u32_be(&mut self, addr: u32) -> Option<u32> {
        let Some(end) = range_end(addr, 4) else {
            return None;
        };
        let state = self.state_mut();
        let start = u64::from(addr);
        // Prove where this word lives once per interval, not per fetch. A
        // `Gap` is the `!overlaps_shared` early-out below, without the walk.
        let lookup = state.instruction_lookup.resolve(state, addr, start, end);
        state.instruction_lookup.store(lookup);
        if lookup.covers(start, end) {
            return match lookup {
                SharedLookup::Gap { .. } => state.regions.read_instruction_u32_be(addr),
                SharedLookup::Owned(run) => {
                    read_shared_word(state, run, start).map(u32::from_be_bytes)
                }
            };
        }
        if !state.overlaps_shared(start, end) {
            return state.regions.read_instruction_u32_be(addr);
        }
        match route_range_state(state, addr, 4, None) {
            GuestMemoryRoute::Sparse => state.regions.read_instruction_u32_be(addr),
            GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly
            | GuestMemoryRoute::Mixed => self.read_u32_be(addr),
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    #[inline]
    fn instruction_cache_token(&mut self, addr: u32) -> Option<u64> {
        let Some(end) = range_end(addr, 4) else {
            return None;
        };
        let state = self.state_mut();
        if !state.overlaps_shared(u64::from(addr), end) {
            return sparse_instruction_token(state, addr);
        }
        match route_range_state(state, addr, 4, None) {
            GuestMemoryRoute::Sparse => sparse_instruction_token(state, addr),
            GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly
            | GuestMemoryRoute::Flat
            | GuestMemoryRoute::Unmapped
            | GuestMemoryRoute::Mixed => None,
        }
    }

    #[inline]
    fn write_u8(&mut self, addr: u32, value: u8) -> Option<()> {
        let state = self.state_mut();
        let start = u64::from(addr);
        if !state.overlaps_shared(start, start + 1) {
            PpcMemory::write_u8(&mut state.regions, addr, value)?;
            state.note_write(addr, &[value]);
            return Some(());
        }
        write_routed_u8_state(state, addr, value, None)
    }

    #[inline]
    fn write_u16_be(&mut self, addr: u32, value: u16) -> Option<()> {
        let Some(end) = range_end(addr, 2) else {
            return None;
        };
        let state = self.state_mut();
        let start = u64::from(addr);
        let bytes = value.to_be_bytes();
        let lookup = state.data_lookup.resolve(state, addr, start, end);
        state.data_lookup.store(lookup);
        if lookup.covers(start, end) {
            return match lookup {
                SharedLookup::Gap { .. } => {
                    PpcMemory::write_u16_be(&mut state.regions, addr, value)?;
                    state.note_write(addr, &bytes);
                    Some(())
                }
                SharedLookup::Owned(run) => {
                    let mapping = &state.shared_regions[run.index];
                    if !mapping.writable {
                        return None;
                    }
                    let offset = (start - u64::from(mapping.base)) as usize;
                    // SAFETY: the cached span proves both bytes have this owner.
                    unsafe { mapping.region.write_from(offset, &bytes) }?;
                    state.note_write(addr, &bytes);
                    Some(())
                }
            };
        }
        match route_range_state(state, addr, 2, None) {
            GuestMemoryRoute::Sparse
            | GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly => write_routed_u16_state(state, addr, value, None),
            GuestMemoryRoute::Mixed => self.write_bytes(addr, &value.to_be_bytes()),
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    #[inline]
    fn write_u32_be(&mut self, addr: u32, value: u32) -> Option<()> {
        let Some(end) = range_end(addr, 4) else {
            return None;
        };
        let state = self.state_mut();
        let start = u64::from(addr);
        let bytes = value.to_be_bytes();
        let lookup = state.data_lookup.resolve(state, addr, start, end);
        state.data_lookup.store(lookup);
        if lookup.covers(start, end) {
            return match lookup {
                SharedLookup::Gap { .. } => {
                    PpcMemory::write_u32_be(&mut state.regions, addr, value)?;
                    state.note_write(addr, &bytes);
                    Some(())
                }
                SharedLookup::Owned(run) => {
                    let mapping = &state.shared_regions[run.index];
                    if !mapping.writable {
                        return None;
                    }
                    let offset = (start - u64::from(mapping.base)) as usize;
                    // SAFETY: see `read_shared_bytes`; the span proves the
                    // word lies wholly inside this writable mapping.
                    unsafe { mapping.region.write_from(offset, &bytes) }?;
                    state.note_write(addr, &bytes);
                    Some(())
                }
            };
        }
        // Mirror of the read path above: with no shared overlap the sparse map
        // is the only backing, so one write replaces the router's proof-read
        // followed by this same write. A read-only sparse region fails both,
        // and an unmapped one cannot accept the write either.
        if !state.overlaps_shared(start, end) {
            PpcMemory::write_u32_be(&mut state.regions, addr, value)?;
            state.note_write(addr, &bytes);
            return Some(());
        }
        match route_range_state(state, addr, 4, None) {
            GuestMemoryRoute::Sparse
            | GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly => write_routed_u32_state(state, addr, value, None),
            GuestMemoryRoute::Mixed => self.write_bytes(addr, &value.to_be_bytes()),
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    #[inline]
    fn write_u64_be(&mut self, addr: u32, value: u64) -> Option<()> {
        let Some(end) = range_end(addr, 8) else {
            return None;
        };
        let state = self.state_mut();
        if !state.overlaps_shared(u64::from(addr), end) {
            state.regions.write_u64_be(addr, value)?;
            state.note_write(addr, &value.to_be_bytes());
            return Some(());
        }
        match route_range_state(state, addr, 8, None) {
            GuestMemoryRoute::Sparse => {
                state.regions.write_u64_be(addr, value)?;
                state.note_write(addr, &value.to_be_bytes());
                Some(())
            }
            GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly
            | GuestMemoryRoute::Mixed => self.write_bytes(addr, &value.to_be_bytes()),
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }
}

impl AddressBus for GuestAddressSpace {
    #[inline]
    fn read_byte(&mut self, address: u32) -> u8 {
        self.read_u8(address).unwrap_or(0)
    }

    #[inline]
    fn read_word(&mut self, address: u32) -> u16 {
        self.read_u16_be(address).unwrap_or(0)
    }

    #[inline]
    fn read_long(&mut self, address: u32) -> u32 {
        self.read_u32_be(address).unwrap_or(0)
    }

    #[inline]
    fn write_byte(&mut self, address: u32, value: u8) {
        let _ = self.write_u8(address, value);
    }

    #[inline]
    fn write_word(&mut self, address: u32, value: u16) {
        let _ = self.write_u16_be(address, value);
    }

    #[inline]
    fn write_long(&mut self, address: u32, value: u32) {
        let _ = self.write_u32_be(address, value);
    }

    #[inline]
    fn try_read_byte(&mut self, address: u32) -> Result<u8, BusFault> {
        self.read_u8(address)
            .ok_or_else(|| Self::bus_fault(address))
    }

    #[inline]
    fn try_read_word(&mut self, address: u32) -> Result<u16, BusFault> {
        self.read_u16_be(address)
            .ok_or_else(|| Self::bus_fault(address))
    }

    #[inline]
    fn try_read_long(&mut self, address: u32) -> Result<u32, BusFault> {
        self.read_u32_be(address)
            .ok_or_else(|| Self::bus_fault(address))
    }

    #[inline]
    fn try_write_byte(&mut self, address: u32, value: u8) -> Result<(), BusFault> {
        self.write_u8(address, value)
            .ok_or_else(|| Self::bus_fault(address))
    }

    #[inline]
    fn try_write_word(&mut self, address: u32, value: u16) -> Result<(), BusFault> {
        self.write_u16_be(address, value)
            .ok_or_else(|| Self::bus_fault(address))
    }

    #[inline]
    fn try_write_long(&mut self, address: u32, value: u32) -> Result<(), BusFault> {
        self.write_u32_be(address, value)
            .ok_or_else(|| Self::bus_fault(address))
    }
}

#[cfg(test)]
mod tests {
    use super::{shared_lookup_at, GuestAddressSpace, GuestIsa, SharedLookup};
    use crate::memory::{GuestMemoryRoute, MacMemoryBus, MemoryBus};
    use m68k::{AddressBus, BatchExit, CpuCore, StepResult};
    use ppc::{PpcCpu, PpcMemory, PpcRunResult};

    const M68K_TRACE_HEAD: u32 = 0x1000;
    const M68K_TRACE_WORDS: [u16; 5] = [0x5280, 0x5281, 0x5347, 0x66f8, 0xa000];

    fn install_m68k_trace_loop(bus: &mut MacMemoryBus, head: u32) {
        for (index, word) in M68K_TRACE_WORDS.into_iter().enumerate() {
            MemoryBus::write_word(bus, head + index as u32 * 2, word);
        }
    }

    fn m68k_trace_cpu(head: u32) -> CpuCore {
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(crate::machine_profile::REFERENCE_MACHINE_PROFILE.cpu_type());
        cpu.pc = head;
        cpu.set_d(7, 10_001);
        cpu
    }

    fn warm_m68k_trace_and_step_reference(
        cpu: &mut CpuCore,
        bus: &mut MacMemoryBus,
        reference: &mut CpuCore,
        reference_bus: &mut MacMemoryBus,
    ) {
        let warm = cpu.run_batch(bus, 40_000, &[]);
        assert_eq!(warm.instructions, 40_000);
        assert_eq!(warm.exit, BatchExit::BudgetExhausted);
        for _ in 0..40_000 {
            assert!(matches!(
                reference.step(reference_bus),
                StepResult::Ok { .. }
            ));
        }
        assert_eq!(cpu.pc, reference.pc);
        assert_eq!(cpu.dar, reference.dar);
        assert_eq!(cpu.get_sr(), reference.get_sr());
        assert_eq!((cpu.d(0), cpu.d(1)), (10_000, 10_000));
        assert_eq!((reference.d(0), reference.d(1)), (10_000, 10_000));
    }

    fn assert_m68k_trace_matches_step(
        head: u32,
        cpu: &mut CpuCore,
        bus: &mut MacMemoryBus,
        reference: &mut CpuCore,
        reference_bus: &mut MacMemoryBus,
    ) {
        let expected_d0 = cpu.d(0) + 2;
        let expected_d1 = cpu.d(1) + 4;
        cpu.set_d(7, 2);
        reference.set_d(7, 2);
        let actual = cpu.run_batch(bus, 32, &[]);
        let mut reference_retired = 0;
        let mut reference_opcode = None;
        for _ in 0..32 {
            match reference.step(reference_bus) {
                StepResult::Ok { .. } => reference_retired += 1,
                StepResult::AlineTrap { opcode } => {
                    reference_opcode = Some(opcode);
                    break;
                }
                other => panic!("unexpected stepped exit: {other:?}"),
            }
        }
        let reference_opcode = reference_opcode.expect("stepped execution did not reach A-line");
        assert_eq!(actual.instructions, reference_retired);
        assert_eq!(reference_opcode, 0xa000);
        assert_eq!(actual.exit, BatchExit::AlineTrap { opcode: 0xa000 });
        assert_eq!((cpu.d(0), cpu.d(1)), (expected_d0, expected_d1));
        assert_eq!(cpu.dar, reference.dar);
        assert_eq!(cpu.pc, reference.pc);
        assert_eq!(cpu.pc, head.wrapping_add(10));
        assert_eq!(cpu.ppc, reference.ppc);
        assert_eq!(cpu.ppc, head.wrapping_add(8));
        assert_eq!(cpu.get_sr(), reference.get_sr());
        assert_eq!(cpu.ir, reference.ir);
    }

    struct CountingGuestAddressSpace<'a> {
        memory: &'a mut GuestAddressSpace,
        instruction_reads: usize,
    }

    impl<'a> CountingGuestAddressSpace<'a> {
        fn new(memory: &'a mut GuestAddressSpace) -> Self {
            Self {
                memory,
                instruction_reads: 0,
            }
        }
    }

    impl PpcMemory for CountingGuestAddressSpace<'_> {
        fn read_u8(&mut self, addr: u32) -> Option<u8> {
            PpcMemory::read_u8(self.memory, addr)
        }

        fn write_u8(&mut self, addr: u32, value: u8) -> Option<()> {
            PpcMemory::write_u8(self.memory, addr, value)
        }

        fn read_u16_be(&mut self, addr: u32) -> Option<u16> {
            PpcMemory::read_u16_be(self.memory, addr)
        }

        fn write_u16_be(&mut self, addr: u32, value: u16) -> Option<()> {
            PpcMemory::write_u16_be(self.memory, addr, value)
        }

        fn read_u32_be(&mut self, addr: u32) -> Option<u32> {
            PpcMemory::read_u32_be(self.memory, addr)
        }

        fn write_u32_be(&mut self, addr: u32, value: u32) -> Option<()> {
            PpcMemory::write_u32_be(self.memory, addr, value)
        }

        fn read_u64_be(&mut self, addr: u32) -> Option<u64> {
            PpcMemory::read_u64_be(self.memory, addr)
        }

        fn write_u64_be(&mut self, addr: u32, value: u64) -> Option<()> {
            PpcMemory::write_u64_be(self.memory, addr, value)
        }

        fn read_instruction_u32_be(&mut self, addr: u32) -> Option<u32> {
            self.instruction_reads += 1;
            PpcMemory::read_instruction_u32_be(self.memory, addr)
        }

        fn instruction_cache_token(&mut self, addr: u32) -> Option<u64> {
            PpcMemory::instruction_cache_token(self.memory, addr)
        }
    }

    fn run_cached_add(
        cpu: &mut PpcCpu,
        memory: &mut CountingGuestAddressSpace<'_>,
        pc: u32,
        expected: u32,
    ) {
        cpu.pc = pc;
        cpu.lr = 0;
        cpu.gpr[3] = 0;
        assert_eq!(
            cpu.run_with_imports(memory, 8, 0, 0, 0, |_, _, _| {
                unreachable!("test program has no imports")
            }),
            PpcRunResult::Halted { pc: 0, cycles: 2 }
        );
        assert_eq!(cpu.gpr[3], expected);
    }

    #[test]
    fn powerpc_store_rewrites_code_seen_by_warmed_attached_m68k_trace() {
        const PPC_WRITER: u32 = 0x0100_0000;

        let mut bus = MacMemoryBus::new(64 * 1024);
        install_m68k_trace_loop(&mut bus, M68K_TRACE_HEAD);
        let mut memory = GuestAddressSpace::new();
        let shared_code = bus
            .shared_ram_region(M68K_TRACE_HEAD, M68K_TRACE_WORDS.len() as u32 * 2)
            .unwrap();
        // SAFETY: CPU execution and every access through either adapter are
        // serialized in this test.
        unsafe { memory.add_shared_region(M68K_TRACE_HEAD, shared_code) };
        memory.add_region(
            PPC_WRITER,
            [0xb064_0002u32, 0x4e80_0020]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        bus.attach_guest_address_space(memory.shared_view());
        assert!(bus.fast_mem_window().is_none());

        let mut reference_bus = MacMemoryBus::new(64 * 1024);
        install_m68k_trace_loop(&mut reference_bus, M68K_TRACE_HEAD);
        let mut cpu = m68k_trace_cpu(M68K_TRACE_HEAD);
        let mut reference = m68k_trace_cpu(M68K_TRACE_HEAD);
        warm_m68k_trace_and_step_reference(&mut cpu, &mut bus, &mut reference, &mut reference_bus);

        let mut ppc = PpcCpu::new();
        ppc.pc = PPC_WRITER;
        ppc.lr = 0;
        ppc.gpr[3] = 0x5481;
        ppc.gpr[4] = M68K_TRACE_HEAD;
        assert_eq!(
            ppc.run(&mut memory, 8, 0),
            PpcRunResult::Halted { pc: 0, cycles: 2 }
        );
        assert_eq!(MemoryBus::read_word(&bus, M68K_TRACE_HEAD + 2), 0x5481);
        MemoryBus::write_word(&mut reference_bus, M68K_TRACE_HEAD + 2, 0x5481);

        assert_m68k_trace_matches_step(
            M68K_TRACE_HEAD,
            &mut cpu,
            &mut bus,
            &mut reference,
            &mut reference_bus,
        );
    }

    #[test]
    fn m68k_store_rewrites_writable_powerpc_code_before_same_cpu_rerun() {
        const M68K_WRITER: u32 = 0x1000;
        const PPC_CODE: u32 = 0x0100_0000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(
            PPC_CODE,
            [0x3863_0001u32, 0x4e80_0020]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        // Writable guest code is cacheable: classic Mac OS publishes every
        // fragment into the writable heap.
        let original_token =
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE).expect("token for heap code");

        let mut ppc = PpcCpu::new();
        ppc.pc = PPC_CODE;
        ppc.lr = 0;
        assert_eq!(
            ppc.run(&mut memory, 8, 0),
            PpcRunResult::Halted { pc: 0, cycles: 2 }
        );
        assert_eq!(ppc.gpr[3], 1);

        let mut bus = MacMemoryBus::new(64 * 1024);
        for (index, word) in [0x23fc, 0x3863, 0x0002, 0x0100, 0x0000]
            .into_iter()
            .enumerate()
        {
            MemoryBus::write_word(&mut bus, M68K_WRITER + index as u32 * 2, word);
        }
        bus.attach_guest_address_space(memory.shared_view());
        let mut m68k = CpuCore::new();
        m68k.set_cpu_type(crate::machine_profile::REFERENCE_MACHINE_PROFILE.cpu_type());
        m68k.pc = M68K_WRITER;
        assert!(matches!(m68k.step(&mut bus), StepResult::Ok { .. }));
        assert_eq!(memory.read_u32_be(PPC_CODE), Some(0x3863_0002));
        // The store landed on a page the interpreter has executed, so the
        // token rotates and the cached decode is retired.
        assert_ne!(
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE),
            Some(original_token)
        );

        ppc.pc = PPC_CODE;
        ppc.lr = 0;
        ppc.gpr[3] = 0;
        assert_eq!(
            ppc.run(&mut memory, 8, 0),
            PpcRunResult::Halted { pc: 0, cycles: 2 }
        );
        assert_eq!(ppc.gpr[3], 2);
    }

    #[test]
    fn writable_heap_code_is_decoded_once_across_repeated_runs() {
        const PPC_CODE: u32 = 0x0300_0000;
        const ADD_ONE: u32 = 0x3863_0001;
        const BLR: u32 = 0x4e80_0020;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(
            PPC_CODE,
            [ADD_ONE, BLR]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        let mut cpu = PpcCpu::new();
        let mut counted = CountingGuestAddressSpace::new(&mut memory);

        run_cached_add(&mut cpu, &mut counted, PPC_CODE, 1);
        assert_eq!(counted.instruction_reads, 2);
        // A CFM fragment lives in the writable heap; the block cache has to
        // engage there or every guest instruction pays a fetch.
        run_cached_add(&mut cpu, &mut counted, PPC_CODE, 1);
        assert_eq!(counted.instruction_reads, 2);
    }

    /// Two address spaces holding different programs at the same address must
    /// never answer with the same token. `PpcMemory`'s contract lets the CPU
    /// reuse a decoded block whenever two tokens match, so an alias here would
    /// run one space's instructions against the other's memory.
    #[test]
    fn one_cpu_keeps_two_address_spaces_at_the_same_address_apart() {
        const PPC_CODE: u32 = 0x0300_0000;
        const ADD_ONE: u32 = 0x3863_0001;
        const ADD_TWO: u32 = 0x3863_0002;
        const BLR: u32 = 0x4e80_0020;

        fn space(first: u32) -> GuestAddressSpace {
            let mut memory = GuestAddressSpace::new();
            memory.add_region(
                PPC_CODE,
                [first, BLR].into_iter().flat_map(u32::to_be_bytes).collect(),
            );
            memory
        }

        let mut one = space(ADD_ONE);
        let mut two = space(ADD_TWO);
        // Both spaces are fresh, at the same page, with the same write and
        // mapping history: the strongest case for an accidental alias.
        let first_token = PpcMemory::instruction_cache_token(&mut one, PPC_CODE).unwrap();
        let second_token = PpcMemory::instruction_cache_token(&mut two, PPC_CODE).unwrap();
        assert_ne!(first_token, second_token);

        // The same CPU runs both. The second program must execute its own
        // instruction, not the block decoded from the first space.
        let mut cpu = PpcCpu::new();
        run_cached_add(&mut cpu, &mut CountingGuestAddressSpace::new(&mut one), PPC_CODE, 1);
        run_cached_add(&mut cpu, &mut CountingGuestAddressSpace::new(&mut two), PPC_CODE, 2);
        run_cached_add(&mut cpu, &mut CountingGuestAddressSpace::new(&mut one), PPC_CODE, 1);
    }

    /// A detached clone copies its parent's regions and then diverges, so it
    /// is the same aliasing hazard reached through an in-tree API.
    #[test]
    fn a_detached_clone_does_not_inherit_its_parents_code_tokens() {
        const PPC_CODE: u32 = 0x0300_0000;
        const ADD_ONE: u32 = 0x3863_0001;
        const ADD_TWO: u32 = 0x3863_0002;
        const BLR: u32 = 0x4e80_0020;

        let mut parent = GuestAddressSpace::new();
        parent.add_region(
            PPC_CODE,
            [ADD_ONE, BLR]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        let parent_token = PpcMemory::instruction_cache_token(&mut parent, PPC_CODE).unwrap();

        let mut clone = parent.clone();
        let clone_token = PpcMemory::instruction_cache_token(&mut clone, PPC_CODE).unwrap();
        assert_ne!(parent_token, clone_token);

        // Rewrite the clone's own copy and run both on one CPU.
        PpcMemory::write_u32_be(&mut clone, PPC_CODE, ADD_TWO).unwrap();
        let mut cpu = PpcCpu::new();
        run_cached_add(&mut cpu, &mut CountingGuestAddressSpace::new(&mut parent), PPC_CODE, 1);
        run_cached_add(&mut cpu, &mut CountingGuestAddressSpace::new(&mut clone), PPC_CODE, 2);
    }

    #[test]
    fn exhausted_token_allocation_disables_writable_code_caching() {
        const PPC_CODE: u32 = 0x0300_0000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(PPC_CODE, vec![0; 0x1000]);
        let state = memory.state_mut();

        assert_eq!(state.writable_code_token_with(PPC_CODE, || None), None);
        assert_eq!(
            state.writable_code_token_with(PPC_CODE, || Some(99)),
            Some(99)
        );
        assert_eq!(
            state.writable_code_token_with(PPC_CODE, || {
                panic!("an allocated page must retain its token")
            }),
            Some(99)
        );
    }

    fn assert_guest_store_redecodes_the_following_instruction(base: u32) {
        const STORE_R4_AT_R5_PLUS_4: u32 = 0x9085_0004;
        const ADD_ONE: u32 = 0x3863_0001;
        const ADD_TWO: u32 = 0x3863_0002;
        const BLR: u32 = 0x4e80_0020;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(
            base,
            [STORE_R4_AT_R5_PLUS_4, ADD_ONE, BLR]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        let mut cpu = PpcCpu::new();
        cpu.pc = base;
        cpu.lr = 0;
        cpu.gpr[4] = ADD_TWO;
        cpu.gpr[5] = base;

        assert_eq!(
            cpu.run(&mut memory, 8, 0),
            PpcRunResult::Halted { pc: 0, cycles: 3 }
        );
        assert_eq!(cpu.gpr[3], 2);
    }

    #[test]
    fn guest_store_rewriting_the_next_instruction_revalidates_the_block() {
        assert_guest_store_redecodes_the_following_instruction(0x0300_0000);
    }

    #[test]
    fn guest_store_rewriting_the_next_page_revalidates_the_block() {
        assert_guest_store_redecodes_the_following_instruction(0x0300_0ffc);
    }

    /// A span bypasses routing, so it must not also bypass invalidation --
    /// including when it was taken before anything executed from the page.
    #[test]
    fn a_write_through_a_cached_span_retires_the_code_it_overwrites() {
        const PPC_CODE: u32 = 0x0300_0000;
        const ADD_ONE: u32 = 0x3863_0001;
        const ADD_TWO: u32 = 0x3863_0002;
        const BLR: u32 = 0x4e80_0020;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(
            PPC_CODE,
            [ADD_ONE, BLR]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        // Taken first: the page has not been executed from, so acquisition
        // cannot be what arms the invalidation.
        let span = memory.writable_span(PPC_CODE, 4).expect("writable span");

        let mut cpu = PpcCpu::new();
        run_cached_add(&mut cpu, &mut CountingGuestAddressSpace::new(&mut memory), PPC_CODE, 1);

        // `addi r3, r3, 2` differs from `addi r3, r3, 1` in its low half.
        let before = PpcMemory::instruction_cache_token(&mut memory, PPC_CODE).unwrap();
        memory
            .write_u16_be_in_span(span, 2, (ADD_TWO & 0xffff) as u16)
            .expect("span write");
        let after = PpcMemory::instruction_cache_token(&mut memory, PPC_CODE).unwrap();
        assert_ne!(before, after, "span write left the code token intact");

        run_cached_add(&mut cpu, &mut CountingGuestAddressSpace::new(&mut memory), PPC_CODE, 2);
    }

    #[test]
    fn writes_clear_of_executed_code_leave_the_decode_cache_alone() {
        const PPC_CODE: u32 = 0x0300_0000;
        const DATA: u32 = 0x0300_4000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(PPC_CODE, vec![0; 0x8000]);
        let token =
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE).expect("token for heap code");

        // Far from any page the interpreter has fetched from: ordinary guest
        // data traffic must not retire cached decodes.
        assert_eq!(PpcMemory::write_u32_be(&mut memory, DATA, 0x1234_5678), Some(()));
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE),
            Some(token)
        );

        // The same page is another matter, flush or no flush.
        assert_eq!(
            PpcMemory::write_u32_be(&mut memory, PPC_CODE + 4, 0x6000_0000),
            Some(())
        );
        assert_ne!(
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE),
            Some(token)
        );
    }

    #[test]
    fn token_reuse_stops_at_a_region_that_ends_inside_its_page() {
        const PPC_CODE: u32 = 0x0300_0000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(PPC_CODE, vec![0; 0x100]);
        assert!(PpcMemory::instruction_cache_token(&mut memory, PPC_CODE).is_some());
        // Same page, past the end of the region: answering from a per-page
        // cache here would let a block run off the end of its mapping.
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE + 0x200),
            None
        );
    }

    #[test]
    fn rewriting_one_code_page_leaves_the_other_pages_cached() {
        const PPC_CODE: u32 = 0x0300_0000;
        const SECOND_PAGE: u32 = PPC_CODE + 0x1000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(PPC_CODE, vec![0; 0x2000]);
        let first = PpcMemory::instruction_cache_token(&mut memory, PPC_CODE).expect("first page");
        let second =
            PpcMemory::instruction_cache_token(&mut memory, SECOND_PAGE).expect("second page");
        assert_ne!(first, second);

        assert_eq!(
            PpcMemory::write_u32_be(&mut memory, SECOND_PAGE, 0x6000_0000),
            Some(())
        );
        // Self-modifying code retires its own page, not the whole process.
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE),
            Some(first)
        );
        assert_ne!(
            PpcMemory::instruction_cache_token(&mut memory, SECOND_PAGE),
            Some(second)
        );
    }

    #[test]
    fn guest_instruction_cache_flush_retires_writable_code_tokens() {
        const PPC_CODE: u32 = 0x0300_0000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(PPC_CODE, vec![0; 0x1000]);
        let token =
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE).expect("token for heap code");

        memory.flush_instruction_cache();
        assert_ne!(
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE),
            Some(token)
        );
    }

    #[test]
    fn immutable_powerpc_overlay_invalidates_same_cpu_cache_after_refused_guest_writes() {
        const PPC_CODE: u32 = 0x0100_0000;
        const ADD_ONE: u32 = 0x3863_0001;
        const ADD_TWO: u32 = 0x3863_0002;
        const BLR: u32 = 0x4e80_0020;

        let mut memory = GuestAddressSpace::new();
        memory.add_readonly_region(
            PPC_CODE,
            [ADD_ONE, BLR]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        let original_token = PpcMemory::instruction_cache_token(&mut memory, PPC_CODE).unwrap();
        let mut cpu = PpcCpu::new();

        {
            let mut counted = CountingGuestAddressSpace::new(&mut memory);
            run_cached_add(&mut cpu, &mut counted, PPC_CODE, 1);
            assert_eq!(counted.instruction_reads, 2);
            assert_eq!(
                PpcMemory::write_u32_be(&mut counted, PPC_CODE, ADD_TWO),
                None
            );
        }

        let mut bus = MacMemoryBus::new(64 * 1024);
        bus.attach_guest_address_space(memory.shared_view());
        assert!(!bus.try_write_long(PPC_CODE, ADD_TWO));
        assert_eq!(MemoryBus::read_long(&bus, PPC_CODE), ADD_ONE);

        {
            let mut counted = CountingGuestAddressSpace::new(&mut memory);
            run_cached_add(&mut cpu, &mut counted, PPC_CODE, 1);
            assert_eq!(
                counted.instruction_reads, 0,
                "the unchanged immutable block must come from the warmed cache"
            );
        }

        memory.add_readonly_region(PPC_CODE, ADD_TWO.to_be_bytes().to_vec());
        let overlay_token = PpcMemory::instruction_cache_token(&mut memory, PPC_CODE).unwrap();
        assert_ne!(overlay_token, original_token);
        assert_eq!(MemoryBus::read_long(&bus, PPC_CODE), ADD_TWO);
        {
            let mut counted = CountingGuestAddressSpace::new(&mut memory);
            run_cached_add(&mut cpu, &mut counted, PPC_CODE, 2);
            assert_eq!(counted.instruction_reads, 2);
        }
    }

    #[test]
    fn same_powerpc_cpu_distinguishes_independent_immutable_memory_tokens() {
        const PPC_CODE: u32 = 0x0100_0000;
        const BLR: u32 = 0x4e80_0020;

        let mut first = GuestAddressSpace::new();
        first.add_readonly_region(
            PPC_CODE,
            [0x3863_0001u32, BLR]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        let mut second = GuestAddressSpace::new();
        second.add_readonly_region(
            PPC_CODE,
            [0x3863_0002u32, BLR]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        let first_token = PpcMemory::instruction_cache_token(&mut first, PPC_CODE).unwrap();
        let second_token = PpcMemory::instruction_cache_token(&mut second, PPC_CODE).unwrap();
        assert_ne!(first_token, second_token);

        let mut cpu = PpcCpu::new();
        for (memory, expected) in [(&mut first, 1), (&mut second, 2)] {
            let mut counted = CountingGuestAddressSpace::new(memory);
            run_cached_add(&mut cpu, &mut counted, PPC_CODE, expected);
            assert_eq!(counted.instruction_reads, 2);
        }
        let mut counted = CountingGuestAddressSpace::new(&mut first);
        run_cached_add(&mut cpu, &mut counted, PPC_CODE, 1);
        assert_eq!(counted.instruction_reads, 2);
    }

    #[test]
    fn shared_system_code_refuses_guest_write_then_owner_rewrite_matches_step() {
        const SYSTEM_CODE: u32 = 0x0100_0000;

        fn system_code_memory() -> GuestAddressSpace {
            let mut memory = GuestAddressSpace::new();
            memory
                .publish_system_code(
                    GuestIsa::M68k,
                    SYSTEM_CODE,
                    M68K_TRACE_WORDS
                        .into_iter()
                        .flat_map(u16::to_be_bytes)
                        .collect(),
                )
                .unwrap();
            memory
        }

        let mut memory = system_code_memory();
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, SYSTEM_CODE),
            None
        );
        let mut bus = MacMemoryBus::new(64 * 1024);
        bus.attach_guest_address_space(memory.shared_view());
        assert!(bus.fast_mem_window().is_none());

        let reference_memory = system_code_memory();
        let mut reference_bus = MacMemoryBus::new(64 * 1024);
        reference_bus.attach_guest_address_space(reference_memory.shared_view());
        let mut cpu = m68k_trace_cpu(SYSTEM_CODE);
        let mut reference = m68k_trace_cpu(SYSTEM_CODE);
        warm_m68k_trace_and_step_reference(&mut cpu, &mut bus, &mut reference, &mut reference_bus);

        assert!(!bus.try_write_word(SYSTEM_CODE + 2, 0x5481));
        assert_eq!(MemoryBus::read_word(&bus, SYSTEM_CODE + 2), 0x5281);
        cpu.set_d(7, 1);
        reference.set_d(7, 1);
        let unchanged = cpu.run_batch(&mut bus, 16, &[]);
        assert_eq!(unchanged.instructions, 4);
        assert_eq!(unchanged.exit, BatchExit::AlineTrap { opcode: 0xa000 });
        for _ in 0..4 {
            assert!(matches!(
                reference.step(&mut reference_bus),
                StepResult::Ok { .. }
            ));
        }
        assert!(matches!(
            reference.step(&mut reference_bus),
            StepResult::AlineTrap { opcode: 0xa000 }
        ));
        assert_eq!((cpu.d(0), cpu.d(1)), (10_001, 10_001));
        assert_eq!(cpu.dar, reference.dar);
        assert_eq!(cpu.pc, reference.pc);
        assert_eq!(cpu.ppc, reference.ppc);

        assert_eq!(
            bus.with_foreign_address_space(|memory| {
                memory.write_shared_system_u32_be(SYSTEM_CODE + 2, 0x5481_5347)
            }),
            Some(Some(()))
        );
        assert_eq!(
            reference_bus.with_foreign_address_space(|memory| {
                memory.write_shared_system_u32_be(SYSTEM_CODE + 2, 0x5481_5347)
            }),
            Some(Some(()))
        );
        cpu.pc = SYSTEM_CODE;
        reference.pc = SYSTEM_CODE;

        assert_m68k_trace_matches_step(
            SYSTEM_CODE,
            &mut cpu,
            &mut bus,
            &mut reference,
            &mut reference_bus,
        );
    }

    #[test]
    fn system_code_isa_follows_mapping_owner_and_detached_lifetime() {
        let mut memory = GuestAddressSpace::new();
        memory
            .publish_system_code(GuestIsa::PowerPc, 0x1000, vec![0x60, 0x06, 0x4e, 0xf9])
            .unwrap();
        memory.add_readonly_region(0x2000, vec![0x60, 0x06, 0x4e, 0xf9]);
        let mut bus = MacMemoryBus::new(0x1000);
        // SAFETY: all source-bus and mapped-view access is serialized here.
        unsafe {
            memory.add_shared_readonly_region(None, 0x3000, bus.shared_ram_region(0, 4).unwrap());
            memory.add_shared_readonly_region(
                Some(GuestIsa::M68k),
                0x4000,
                bus.shared_ram_region(4, 4).unwrap(),
            );
        }
        memory.add_region(0x1000, vec![0; 4]);
        assert_eq!(memory.system_code_isa(0x1000), Some(GuestIsa::PowerPc));
        assert_eq!(memory.system_code_isa(0x2000), None);
        assert_eq!(memory.system_code_isa(0x3000), None);
        assert_eq!(memory.system_code_isa(0x4000), Some(GuestIsa::M68k));
        assert_eq!(memory.system_code_isa(0x5000), None);
        let detached = memory.clone();
        // A newer shared writable mapping wins and carries no code identity.
        unsafe { memory.add_shared_region(0x1000, bus.shared_ram_region(8, 4).unwrap()) };
        assert_eq!(memory.system_code_isa(0x1000), None);
        assert_eq!(detached.system_code_isa(0x1000), Some(GuestIsa::PowerPc));
        assert_eq!(detached.system_code_isa(0x3000), None);
        assert_eq!(detached.system_code_isa(0x4000), Some(GuestIsa::M68k));
    }

    /// A later mapping takes ownership of the addresses it overlays, so a
    /// cached span must not survive it.
    #[test]
    fn instruction_fetch_span_cache_follows_later_overlay_mappings() {
        let mut memory = GuestAddressSpace::new();
        let mut bus = MacMemoryBus::new(0x10000);
        for offset in (0..0x40).step_by(4) {
            MemoryBus::write_long(&mut bus, offset, 0x1111_1111);
        }
        // SAFETY: all source and address-space accesses are serialized here.
        unsafe { memory.add_shared_region(0x1000, bus.shared_ram_region(0, 0x40).unwrap()) };

        // Warm the cache on the low word, then fetch across the span.
        assert_eq!(memory.read_instruction_u32_be(0x1000), Some(0x1111_1111));
        assert_eq!(memory.read_instruction_u32_be(0x1020), Some(0x1111_1111));
        assert_eq!(memory.read_instruction_u32_be(0x103c), Some(0x1111_1111));

        // Overlay the middle of that mapping; the overlay now owns 0x1020.
        let mut overlay = MacMemoryBus::new(0x10000);
        MemoryBus::write_long(&mut overlay, 0, 0x2222_2222);
        // SAFETY: as above.
        unsafe { memory.add_shared_region(0x1020, overlay.shared_ram_region(0, 4).unwrap()) };

        assert_eq!(
            memory.read_instruction_u32_be(0x1020),
            Some(0x2222_2222),
            "a later mapping must shadow the cached span"
        );
        // Spans either side of the overlay still resolve to the original.
        assert_eq!(memory.read_instruction_u32_be(0x101c), Some(0x1111_1111));
        assert_eq!(memory.read_instruction_u32_be(0x1024), Some(0x1111_1111));
        // A word straddling the boundary must decline the cached-span path.
        assert_eq!(memory.read_instruction_u32_be(0x101e), Some(0x1111_2222));
    }

    /// A cached `Gap` asserts no mapping touches an interval; publishing one
    /// into it must invalidate that.
    #[test]
    fn cached_gap_is_invalidated_by_a_mapping_published_into_it() {
        let mut memory = GuestAddressSpace::new();
        let mut bus = MacMemoryBus::new(0x10000);
        MemoryBus::write_long(&mut bus, 0, 0x4444_4444);
        // SAFETY: all source and address-space accesses are serialized here.
        unsafe { memory.add_shared_region(0x8000, bus.shared_ram_region(0, 4).unwrap()) };

        // Prove the gap below the mapping, for both the fetch and data slots.
        assert_eq!(memory.read_instruction_u32_be(0x3000), None);
        assert_eq!(memory.read_u32_be(0x3000), None);
        assert_eq!(memory.write_u32_be(0x3000, 0x5555_5555), None);

        // Publish into the middle of that proven-empty interval.
        let mut overlay = MacMemoryBus::new(0x10000);
        MemoryBus::write_long(&mut overlay, 0, 0x6666_6666);
        // SAFETY: as above.
        unsafe { memory.add_shared_region(0x3000, overlay.shared_ram_region(0, 4).unwrap()) };

        assert_eq!(
            memory.read_u32_be(0x3000),
            Some(0x6666_6666),
            "a cached gap must not survive a mapping published into it"
        );
        assert_eq!(memory.read_instruction_u32_be(0x3000), Some(0x6666_6666));
        assert_eq!(memory.write_u32_be(0x3000, 0x7777_7777), Some(()));
        assert_eq!(memory.read_u32_be(0x3000), Some(0x7777_7777));
        // The mapping published first is still reachable and unchanged.
        assert_eq!(memory.read_u32_be(0x8000), Some(0x4444_4444));
    }

    #[test]
    fn alternating_data_spans_are_invalidated_by_later_overlay() {
        let mut memory = GuestAddressSpace::new();
        let mut bus = MacMemoryBus::new(0x10000);
        MemoryBus::write_long(&mut bus, 0, 0x1111_1111);
        MemoryBus::write_long(&mut bus, 4, 0x3333_3333);
        // SAFETY: the source bus and address space are accessed serially here.
        unsafe {
            memory.add_shared_region(0x1000, bus.shared_ram_region(0, 4).unwrap());
            memory.add_shared_region(0x3000, bus.shared_ram_region(4, 4).unwrap());
        }

        for _ in 0..3 {
            assert_eq!(memory.read_u32_be(0x1000), Some(0x1111_1111));
            assert_eq!(memory.read_u32_be(0x3000), Some(0x3333_3333));
        }

        let mut overlay = MacMemoryBus::new(0x10000);
        MemoryBus::write_long(&mut overlay, 0, 0x2222_2222);
        // SAFETY: as above, and the later mapping takes priority at 0x1000.
        unsafe { memory.add_shared_region(0x1000, overlay.shared_ram_region(0, 4).unwrap()) };

        assert_eq!(memory.read_u32_be(0x1000), Some(0x2222_2222));
        assert_eq!(memory.read_u32_be(0x3000), Some(0x3333_3333));
    }

    #[test]
    fn unshared_page_gap_stops_at_shared_page_boundary() {
        let mut memory = GuestAddressSpace::new();
        let mut bus = MacMemoryBus::new(0x10000);
        // SAFETY: the bus and address space are accessed serially here.
        unsafe { memory.add_shared_region(0x2000, bus.shared_ram_region(0, 4).unwrap()) };

        let gap = shared_lookup_at(memory.state(), 0x1ffc);
        assert!(matches!(
            gap,
            SharedLookup::Gap {
                start: 0x1000,
                end: 0x2000
            }
        ));
        assert!(gap.covers(0x1ffc, 0x2000));
        assert!(!gap.covers(0x1ffc, 0x2004));
        assert!(matches!(
            shared_lookup_at(memory.state(), 0x2000),
            SharedLookup::Owned(_)
        ));
    }

    /// A cached span must not wave a write through to a read-only mapping.
    #[test]
    fn cached_data_span_still_refuses_writes_to_readonly_mappings() {
        let mut memory = GuestAddressSpace::new();
        memory
            .publish_system_code(GuestIsa::M68k, 0x5000, vec![0xAB; 8])
            .unwrap();

        assert_eq!(memory.read_u32_be(0x5000), Some(0xABAB_ABAB));
        assert_eq!(memory.write_u32_be(0x5000, 0), None, "read-only mapping");
        assert_eq!(memory.write_u32_be(0x5004, 0), None, "same cached span");
        assert_eq!(
            memory.read_u32_be(0x5000),
            Some(0xABAB_ABAB),
            "a refused write must not have landed"
        );
    }

    #[test]
    fn cached_halfword_accesses_respect_shared_overlay_boundaries() {
        let mut memory = GuestAddressSpace::new();
        let mut base = MacMemoryBus::new(0x10000);
        let mut overlay = MacMemoryBus::new(0x10000);
        MemoryBus::write_long(&mut base, 0, 0x1122_3344);
        MemoryBus::write_word(&mut overlay, 0, 0xaabb);
        // SAFETY: all source and address-space accesses are serialized here.
        unsafe {
            memory.add_shared_region(0x1000, base.shared_ram_region(0, 4).unwrap());
            memory.add_shared_region(0x1002, overlay.shared_ram_region(0, 2).unwrap());
        }

        assert_eq!(memory.read_u16_be(0x1000), Some(0x1122));
        assert_eq!(memory.read_u16_be(0x1001), Some(0x22aa));
        assert_eq!(memory.read_u16_be(0x1002), Some(0xaabb));
        assert_eq!(memory.write_u16_be(0x1000, 0x5566), Some(()));
        assert_eq!(memory.read_u16_be(0x1000), Some(0x5566));
        assert_eq!(memory.write_u16_be(0x1001, 0x7788), Some(()));
        assert_eq!(memory.read_u16_be(0x1001), Some(0x7788));
        assert_eq!(memory.read_u16_be(0x1002), Some(0x88bb));

        let mut readonly = MacMemoryBus::new(0x10000);
        MemoryBus::write_word(&mut readonly, 0, 0xccdd);
        // SAFETY: as above; a later mapping shadows the writable overlay.
        unsafe {
            memory.add_shared_readonly_region(
                None,
                0x1002,
                readonly.shared_ram_region(0, 2).unwrap(),
            );
        }
        assert_eq!(memory.read_u16_be(0x1002), Some(0xccdd));
        assert_eq!(memory.write_u16_be(0x1002, 0), None);
        assert_eq!(memory.write_u16_be(0x1001, 0), None);
        assert_eq!(memory.read_u16_be(0x1001), Some(0x77cc));
    }

    #[test]
    fn instruction_fetch_matches_the_routed_read_across_a_mapping_gap() {
        let mut memory = GuestAddressSpace::new();
        let mut bus = MacMemoryBus::new(0x10000);
        MemoryBus::write_long(&mut bus, 0, 0x3333_3333);
        // SAFETY: all source and address-space accesses are serialized here.
        unsafe { memory.add_shared_region(0x2000, bus.shared_ram_region(0, 4).unwrap()) };

        assert_eq!(memory.read_instruction_u32_be(0x2000), Some(0x3333_3333));
        // Running off the end of the mapping is unmapped, not a stale hit.
        assert_eq!(memory.read_instruction_u32_be(0x2004), None);
        assert_eq!(memory.read_instruction_u32_be(0x2002), None);
        assert_eq!(memory.read_instruction_u32_be(0x2000), Some(0x3333_3333));
    }

    #[test]
    fn shared_mapping_bounds_follow_insertions_and_detached_views() {
        let mut memory = GuestAddressSpace::new();
        let view = memory.shared_view();
        assert_eq!(memory.read_u32_be(0x8000), None);
        memory
            .publish_system_code(GuestIsa::M68k, 0x8000, vec![0x88; 4])
            .unwrap();
        assert_eq!(memory.read_u32_be(0x8000), Some(0x8888_8888));
        assert_eq!(memory.read_u32_be(0x1000), None);
        let mut bus = MacMemoryBus::new(0x10000);
        MemoryBus::write_long(&mut bus, 0, 0x1111_1111);
        let region = bus.shared_ram_region(0, 4).unwrap();
        // SAFETY: all source and address-space accesses are serialized here.
        unsafe { memory.add_shared_region(0x1000, region) };
        assert_eq!(memory.read_u32_be(0x1000), Some(0x1111_1111));
        assert!(!view.is_shared_readonly_range(0x1000, 4));
        assert!(view.is_shared_readonly_range(0x8000, 4));
        assert_eq!(memory.read_u32_be(0x4000), None, "holes remain unmapped");
        let mut detached = memory.clone();
        assert_eq!(detached.read_u32_be(0x1000), Some(0x1111_1111));
        assert_eq!(detached.read_u32_be(0x8000), Some(0x8888_8888));
        memory
            .publish_system_code(GuestIsa::M68k, 0x9000, vec![0x99; 4])
            .unwrap();
        detached
            .publish_system_code(GuestIsa::M68k, 0x0800, vec![0x08; 4])
            .unwrap();
        assert_eq!(memory.read_u32_be(0x9000), Some(0x9999_9999));
        assert_eq!(detached.read_u32_be(0x9000), None);
        assert_eq!(memory.read_u32_be(0x0800), None);
        assert_eq!(detached.read_u32_be(0x0800), Some(0x0808_0808));
    }

    #[test]
    fn owned_system_code_preserves_provenance_and_detached_snapshot_independence() {
        let mut memory = GuestAddressSpace::new();
        memory
            .publish_system_code(
                GuestIsa::M68k,
                0x1000,
                vec![0x60, 0x06, 0x4e, 0xf9, 0, 0, 0x20, 0],
            )
            .unwrap();
        let view = memory.shared_view();
        assert!(view.is_shared_readonly_range(0x1000, 8));
        assert_eq!(memory.write_u32_be(0x1004, 0x3000), None);
        memory.add_region(0x1000, vec![0xff; 8]);
        assert_eq!(memory.read_u32_be(0x1000), Some(0x6006_4ef9));
        assert!(view.is_shared_readonly_range(0x1000, 8));
        let mut detached = memory.clone();
        assert!(detached.shared_view().is_shared_readonly_range(0x1000, 8));
        assert_eq!(
            detached.write_shared_system_u32_be(0x1004, 0x4000),
            Some(())
        );
        assert_eq!(detached.read_u32_be(0x1004), Some(0x4000));
        assert_eq!(memory.read_u32_be(0x1004), Some(0x2000));
        assert_eq!(memory.write_shared_system_u32_be(0x1004, 0x5000), Some(()));
        assert_eq!(detached.read_u32_be(0x1004), Some(0x4000));
        memory.add_readonly_region(0x2000, vec![0x60, 0x06, 0x4e, 0xf9]);
        assert!(!view.is_shared_readonly_range(0x2000, 4));
        assert_eq!(memory.write_shared_system_u32_be(0x2000, 0), None);
    }

    #[test]
    fn system_code_publication_refuses_invalid_or_occupied_ranges_atomically() {
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x1000, vec![0x11; 4]);
        memory.add_readonly_region(0x2000, vec![0x22; 4]);
        memory
            .publish_system_code(GuestIsa::M68k, 0x3000, vec![0x33; 4])
            .unwrap();
        let mut bus = MacMemoryBus::new(0x10000);
        let region = bus.shared_ram_region(0, 4).unwrap();
        // SAFETY: the test serializes both memory owners.
        unsafe { memory.add_shared_region(0x4000, region) };
        for (base, size) in [
            (0, 0),
            (u32::MAX - 1, 4),
            (0x1000, 4),
            (0x1ffe, 4),
            (0x3002, 4),
            (0x3ffe, 4),
        ] {
            let count = memory.region_count();
            assert_eq!(
                memory.publish_system_code(GuestIsa::M68k, base, vec![0x77; size]),
                None
            );
            assert_eq!(memory.region_count(), count);
            assert_eq!(memory.read_u32_be(0x1000), Some(0x1111_1111));
            assert_eq!(memory.read_u32_be(0x2000), Some(0x2222_2222));
            assert_eq!(memory.read_u32_be(0x3000), Some(0x3333_3333));
            assert_eq!(memory.read_u32_be(0x4000), Some(0));
        }
        // The last four bytes are valid; only a range beyond 2^32 wraps.
        memory
            .publish_system_code(GuestIsa::M68k, u32::MAX - 3, vec![0x55; 4])
            .unwrap();
        assert_eq!(memory.read_u32_be(u32::MAX - 3), Some(0x5555_5555));
    }

    #[test]
    fn both_cpu_backends_execute_against_immediately_shared_bytes() {
        const M68K_STORE_PC: u32 = 0x1000;
        const M68K_LOAD_PC: u32 = 0x1020;
        const PPC_PC: u32 = 0x1100;
        const VALUE_ADDR: u32 = 0x2000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x1000, vec![0; 0x1100]);

        // MOVE.L #$DEADBEEF,$00002000
        memory
            .write_bytes(
                M68K_STORE_PC,
                &[0x23, 0xfc, 0xde, 0xad, 0xbe, 0xef, 0x00, 0x00, 0x20, 0x00],
            )
            .unwrap();
        // MOVE.L $00002000,D0
        memory
            .write_bytes(M68K_LOAD_PC, &[0x20, 0x39, 0x00, 0x00, 0x20, 0x00])
            .unwrap();
        // lwz r3,0(r4); addi r3,r3,1; stw r3,0(r4)
        memory
            .write_bytes(
                PPC_PC,
                &[
                    0x80, 0x64, 0x00, 0x00, 0x38, 0x63, 0x00, 0x01, 0x90, 0x64, 0x00, 0x00,
                ],
            )
            .unwrap();

        let mut m68k = CpuCore::new();
        m68k.pc = M68K_STORE_PC;
        assert!(matches!(m68k.step(&mut memory), StepResult::Ok { .. }));
        assert_eq!(memory.read_u32_be(VALUE_ADDR), Some(0xdead_beef));

        let mut ppc = PpcCpu::new();
        ppc.pc = PPC_PC;
        ppc.gpr[4] = VALUE_ADDR;
        assert_eq!(
            ppc.run(&mut memory, 3, 0),
            PpcRunResult::CycleLimit { cycles: 3 }
        );
        assert_eq!(memory.read_u32_be(VALUE_ADDR), Some(0xdead_bef0));

        let mut m68k_reader = CpuCore::new();
        m68k_reader.pc = M68K_LOAD_PC;
        assert!(matches!(
            m68k_reader.step(&mut memory),
            StepResult::Ok { .. }
        ));
        assert_eq!(m68k_reader.d(0), 0xdead_bef0);
    }

    #[test]
    fn selected_68040_preserves_address_error_frame_in_shared_memory() {
        const SSP: u32 = 0x2000;
        const ODD_PC: u32 = 0x1001;
        const HANDLER: u32 = 0x1200;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(0, vec![0; 0x3000]);
        memory.write_u32_be(0, SSP).unwrap();
        memory.write_u32_be(4, 0x1000).unwrap();
        memory.write_u32_be(3 * 4, HANDLER).unwrap();
        memory.write_u16_be(HANDLER, 0x4e73).unwrap(); // RTE

        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(crate::machine_profile::REFERENCE_MACHINE_PROFILE.cpu_type());
        cpu.reset(&mut memory);
        cpu.pc = ODD_PC;
        cpu.set_sr(0x2700);

        assert!(matches!(cpu.step(&mut memory), StepResult::Ok { .. }));
        assert_eq!(cpu.pc, HANDLER);
        assert_eq!(cpu.a(7), SSP - 12, "six-word format-$2 frame");

        let frame = cpu.a(7);
        assert_eq!(memory.read_u32_be(frame + 2), Some(ODD_PC));
        assert_eq!(memory.read_u16_be(frame + 6), Some(0x200c));
        assert_eq!(memory.read_u32_be(frame + 8), Some(ODD_PC & !1));

        assert!(matches!(cpu.step(&mut memory), StepResult::Ok { .. }));
        assert_eq!(cpu.pc, ODD_PC);
        assert_eq!(cpu.a(7), SSP, "RTE consumes the complete frame");
    }

    #[test]
    fn both_bus_contracts_preserve_mapping_faults_and_read_only_regions() {
        let mut memory = GuestAddressSpace::new();
        memory.add_readonly_region(0x1000, vec![0x12, 0x34, 0x56, 0x78]);

        assert_eq!(
            PpcMemory::read_u32_be(&mut memory, 0x1000),
            Some(0x1234_5678)
        );
        assert_eq!(PpcMemory::write_u8(&mut memory, 0x1000, 0xff), None);
        assert!(AddressBus::try_write_byte(&mut memory, 0x1000, 0xff).is_err());
        assert!(AddressBus::try_read_byte(&mut memory, 0x2000).is_err());
        assert_eq!(AddressBus::read_byte(&mut memory, 0x2000), 0);
    }

    #[test]
    fn runner_ram_mapping_is_authoritative_and_clones_as_a_snapshot() {
        const SHARED: u32 = 0x156;

        let mut runner_bus = MacMemoryBus::new(64 * 1024);
        MemoryBus::write_long(&mut runner_bus, SHARED, 0x1122_3344);

        let mut memory = GuestAddressSpace::new();
        memory.add_region(0, vec![0; 64 * 1024]);
        assert!(runner_bus.fast_mem_window().is_some());
        let shared = runner_bus
            .shared_ram_region(SHARED, 4)
            .expect("owned runner RAM");
        assert!(runner_bus.fast_mem_window().is_none());
        // SAFETY: the test accesses the bus and address space sequentially and
        // does not retain a RAM slice or fast-memory window across a mutation.
        unsafe {
            memory.add_shared_region(SHARED, shared);
        }

        assert_eq!(
            PpcMemory::read_u32_be(&mut memory, SHARED),
            Some(0x1122_3344)
        );
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, SHARED),
            None
        );

        PpcMemory::write_u32_be(&mut memory, SHARED, 0x5566_7788).unwrap();
        assert_eq!(MemoryBus::read_long(&runner_bus, SHARED), 0x5566_7788);
        assert_eq!(runner_bus.ram_slice(SHARED, 4), &[0x55, 0x66, 0x77, 0x88]);

        memory
            .write_bytes(SHARED - 1, &[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff])
            .unwrap();
        let mut crossed = [0; 6];
        memory.read_bytes_into(SHARED - 1, &mut crossed).unwrap();
        assert_eq!(crossed, [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        assert_eq!(MemoryBus::read_long(&runner_bus, SHARED), 0xbbcc_ddee);

        memory.add_readonly_region(SHARED + 4, vec![0x7f]);
        assert_eq!(memory.write_bytes(SHARED - 1, &[1, 2, 3, 4, 5, 6]), None);
        assert_eq!(PpcMemory::read_u8(&mut memory, SHARED - 1), Some(0xaa));
        assert_eq!(MemoryBus::read_long(&runner_bus, SHARED), 0xbbcc_ddee);
        assert_eq!(PpcMemory::read_u8(&mut memory, SHARED + 4), Some(0x7f));

        AddressBus::write_long(&mut memory, SHARED, 0x99aa_bbcc);
        assert_eq!(MemoryBus::read_long(&runner_bus, SHARED), 0x99aa_bbcc);

        let mut snapshot = memory.clone();
        PpcMemory::write_u32_be(&mut snapshot, SHARED, 0xddee_ff00).unwrap();
        assert_eq!(MemoryBus::read_long(&runner_bus, SHARED), 0x99aa_bbcc);
        assert_eq!(
            PpcMemory::read_u32_be(&mut memory, SHARED),
            Some(0x99aa_bbcc)
        );
        assert_eq!(
            PpcMemory::read_u32_be(&mut snapshot, SHARED),
            Some(0xddee_ff00)
        );
    }

    #[test]
    fn process_mappings_remain_attached_until_explicitly_replaced() {
        const FLAT: u32 = 0x2000;
        const SPARSE: u32 = 0x0100_0000;
        const READ_ONLY: u32 = SPARSE + 0x100;

        let mut bus = MacMemoryBus::new(64 * 1024);
        MemoryBus::write_long(&mut bus, FLAT, 0x1122_3344);
        let mut memory = GuestAddressSpace::new();
        memory.add_region(SPARSE, vec![0x55, 0x66, 0x77, 0x88, 0, 0, 0, 0, 0, 0, 0, 0]);
        memory.add_readonly_region(READ_ONLY, 0x99aa_bbccu32.to_be_bytes().to_vec());

        let shared = memory.shared_view();
        bus.attach_guest_address_space(shared);
        assert_eq!(MemoryBus::read_long(&bus, FLAT), 0x1122_3344);
        assert_eq!(MemoryBus::read_long(&bus, SPARSE), 0x5566_7788);
        MemoryBus::write_long(&mut bus, SPARSE, 0xdead_beef);
        MemoryBus::write_bytes(&mut bus, SPARSE + 4, &[1, 2, 3, 4]);
        bus.block_move(FLAT, SPARSE + 8, 4);
        bus.block_move(SPARSE + 4, FLAT + 4, 4);
        assert_eq!(
            MemoryBus::read_bytes(&bus, SPARSE + 4, 8),
            [1, 2, 3, 4, 0x11, 0x22, 0x33, 0x44]
        );
        assert_eq!(MemoryBus::read_long(&bus, FLAT + 4), 0x0102_0304);
        MemoryBus::write_long(&mut bus, READ_ONLY, 0);
        bus.detach_guest_address_space();

        assert_eq!(
            PpcMemory::read_u32_be(&mut memory, SPARSE),
            Some(0xdead_beef)
        );
        assert_eq!(
            PpcMemory::read_u32_be(&mut memory, READ_ONLY),
            Some(0x99aa_bbcc)
        );
        let mut sparse_tail = [0; 8];
        memory
            .read_bytes_into(SPARSE + 4, &mut sparse_tail)
            .unwrap();
        assert_eq!(sparse_tail, [1, 2, 3, 4, 0x11, 0x22, 0x33, 0x44]);
        assert_eq!(MemoryBus::read_long(&bus, SPARSE), 0);
    }

    #[test]
    fn shared_process_view_survives_moves_while_clones_remain_detached() {
        const SPARSE: u32 = 0x0100_0000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(SPARSE, 0x1122_3344u32.to_be_bytes().to_vec());
        let mut detached = memory.clone();
        let shared = memory.shared_view();
        let mut moved = memory;

        let mut bus = MacMemoryBus::new(64 * 1024);
        bus.attach_guest_address_space(shared);
        assert_eq!(MemoryBus::read_long(&bus, SPARSE), 0x1122_3344);

        MemoryBus::write_long(&mut bus, SPARSE, 0x5566_7788);
        assert_eq!(
            PpcMemory::read_u32_be(&mut moved, SPARSE),
            Some(0x5566_7788)
        );
        assert_eq!(
            PpcMemory::read_u32_be(&mut detached, SPARSE),
            Some(0x1122_3344)
        );

        PpcMemory::write_u32_be(&mut detached, SPARSE, 0x99aa_bbcc).unwrap();
        assert_eq!(MemoryBus::read_long(&bus, SPARSE), 0x5566_7788);

        drop(moved);
        assert_eq!(MemoryBus::read_long(&bus, SPARSE), 0x5566_7788);
    }

    #[test]
    fn shared_view_distinguishes_ordinary_sparse_mappings_from_overlays() {
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x2000, vec![0; 0x100]);

        let mut shared_bus_ram = MacMemoryBus::new(64 * 1024);
        let shared_region = shared_bus_ram.shared_ram_region(0, 0x1000).unwrap();
        unsafe {
            memory.add_shared_region(0x0000, shared_region);
        }

        let shared = memory.shared_view();
        // Shared overlays are not ordinary sparse mappings.
        assert!(!shared.is_ordinary_sparse_mapped(0x0500));
        // Native heap regions are ordinary sparse mappings.
        assert!(shared.is_ordinary_sparse_mapped(0x2050));
        // Unmapped addresses belong to neither domain.
        assert!(!shared.is_ordinary_sparse_mapped(0x9000));
    }

    #[test]
    fn mapping_holes_preserve_owned_system_code_and_shared_ram() {
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x1200, vec![0; 0x100]);
        memory
            .publish_system_code(GuestIsa::M68k, 0x1400, vec![0x5a; 0x100])
            .unwrap();
        let mut bus = MacMemoryBus::new(0x1000);
        // SAFETY: both adapters are accessed serially in this test.
        unsafe { memory.add_shared_region(0x1600, bus.shared_ram_region(0, 0x100).unwrap()) };
        for view in [&memory, &memory.clone()] {
            assert_eq!(
                view.mapping_ranges(),
                vec![(0x1200, 0x1300), (0x1400, 0x1500), (0x1600, 0x1700)]
            );
            assert_eq!(
                view.mapping_holes(0x1000, 0x1800),
                vec![
                    (0x1000, 0x1200),
                    (0x1300, 0x1400),
                    (0x1500, 0x1600),
                    (0x1700, 0x1800)
                ]
            );
        }
    }

    #[test]
    fn mapping_holes_track_writable_and_readonly_regions() {
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x1200, vec![0; 0x100]);
        memory.add_readonly_region(0x1400, vec![0; 0x200]);
        memory.add_region(0x1500, vec![0; 0x200]);

        assert_eq!(
            memory.mapping_holes(0x1000, 0x1800),
            vec![(0x1000, 0x1200), (0x1300, 0x1400), (0x1700, 0x1800)]
        );
        assert_eq!(
            memory.mapping_ranges(),
            vec![(0x1200, 0x1300), (0x1400, 0x1700)]
        );
        assert!(memory.ordinary_mapping_overlaps(0x1280, 0x100));
        assert!(!memory.ordinary_mapping_overlaps(0x1300, 0x100));

        let detached = memory.clone();
        assert_eq!(
            detached.mapping_holes(0x1000, 0x1800),
            vec![(0x1000, 0x1200), (0x1300, 0x1400), (0x1700, 0x1800)]
        );
        assert_eq!(
            detached.mapping_ranges(),
            vec![(0x1200, 0x1300), (0x1400, 0x1700)]
        );
    }

    /// Opt-in measurement of the scalar routing fast paths.
    ///
    /// Two patterns, because they stress different properties of the ledger
    /// cache. `interleaved` alternates one ordinary sparse access with two
    /// shared RAM aliases, which each fall in a different maximal ledger
    /// interval. `streaming` walks one span in order, where the interval an
    /// entry already holds should keep covering the next word.
    ///
    /// ```sh
    /// cargo test --profile fast --lib scalar_routing_microbench -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "measurement harness; prints scalar routing timings"]
    fn scalar_routing_microbench() {
        use crate::memory::bus::SharedRamRegion;

        const ORDINARY: u32 = 0x2000_0000;
        const SHARED_A: u32 = 0x0004_0000;
        const SHARED_B: u32 = 0x0100_0000;
        const LEN: u32 = 1 << 20;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(ORDINARY, vec![0u8; LEN as usize]);
        // SAFETY: the benchmark owns every allocation and accesses them
        // serially from this thread, and retains no RAM slice or window.
        unsafe {
            memory.add_shared_region(
                SHARED_A,
                SharedRamRegion::from_owned_bytes(vec![0u8; LEN as usize]),
            );
            memory.add_shared_region(
                SHARED_B,
                SharedRamRegion::from_owned_bytes(vec![0u8; LEN as usize]),
            );
        }

        let iterations: u32 = std::env::var("SYSTEMLESS_ROUTE_ITERATIONS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(1_000_000);

        let mut acc = 0u32;
        let start = std::time::Instant::now();
        for i in 0..iterations {
            let offset = (i.wrapping_mul(4)) & (LEN - 4);
            acc = acc.wrapping_add(PpcMemory::read_u32_be(&mut memory, ORDINARY + offset).unwrap());
            PpcMemory::write_u32_be(&mut memory, SHARED_A + offset, acc).unwrap();
            acc = acc.wrapping_add(PpcMemory::read_u32_be(&mut memory, SHARED_B + offset).unwrap());
            std::hint::black_box(PpcMemory::read_instruction_u32_be(
                &mut memory,
                ORDINARY + offset,
            ));
        }
        let elapsed = start.elapsed();
        std::hint::black_box(acc);
        eprintln!(
            "scalar_routing_microbench interleaved: {iterations} rounds in {elapsed:?} ({:?}/round)",
            elapsed / iterations
        );

        let start = std::time::Instant::now();
        for i in 0..iterations {
            let offset = (i.wrapping_mul(4)) & (LEN - 4);
            acc = acc.wrapping_add(PpcMemory::read_u32_be(&mut memory, ORDINARY + offset).unwrap());
        }
        let elapsed = start.elapsed();
        std::hint::black_box(acc);
        eprintln!(
            "scalar_routing_microbench streaming: {iterations} reads in {elapsed:?} ({:?}/read)",
            elapsed / iterations
        );
    }

    /// Opt-in measurement of the bulk range router the classic adapter uses.
    ///
    /// Mirrors one front-buffer mirror row: route a whole row, then ask
    /// whether a wholly-local shared alias owns it (`MacMemoryBus` promotes
    /// that answer to `Flat`). Both steps walked the shared ledger per row.
    ///
    /// ```sh
    /// cargo test --profile fast --lib bulk_range_routing_microbench -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "measurement harness; prints bulk range routing timings"]
    fn bulk_range_routing_microbench() {
        use crate::memory::bus::SharedRamRegion;
        use crate::memory::GuestMemoryRoute;

        const ALIAS: u32 = 0x0004_0000;
        const ORDINARY: u32 = 0x2000_0000;
        const LEN: u32 = 1 << 20;
        const ROW: usize = 640;

        let local = SharedRamRegion::from_owned_bytes(vec![0u8; LEN as usize]);
        let mut memory = GuestAddressSpace::new();
        memory.add_region(ORDINARY, vec![0u8; LEN as usize]);
        // SAFETY: the benchmark owns every allocation and accesses them
        // serially from this thread, and retains no RAM slice or window.
        unsafe {
            memory.add_shared_region(ALIAS, local.clone());
            // Further process mappings, so the ledger walk this measures
            // scales the way a real native process's mapping list does.
            for index in 1..16u32 {
                memory.add_shared_region(
                    0x0100_0000 + index * 0x0008_0000,
                    SharedRamRegion::from_owned_bytes(vec![0u8; 0x1000]),
                );
            }
        }
        let view = memory.shared_view();

        let iterations: u32 = std::env::var("SYSTEMLESS_ROUTE_ITERATIONS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(200_000);

        // The mirror's rows: one local alias promoted to `Flat` per row.
        let mut acc = 0u32;
        let start = std::time::Instant::now();
        for i in 0..iterations {
            let address = ALIAS + ((i.wrapping_mul(ROW as u32)) & (LEN - ROW as u32));
            acc += u32::from(view.route(address, ROW, Some(LEN)) == GuestMemoryRoute::Flat);
            acc += u32::from(view.shared_range_is_local_flat(address, ROW, &local));
        }
        let elapsed = start.elapsed();
        std::hint::black_box(acc);
        eprintln!(
            "bulk_range_routing_microbench mirror: {iterations} rows in {elapsed:?} ({:?}/row)",
            elapsed / iterations
        );

        // Ordinary sparse mappings: the same range size with no alias to
        // promote, which must not regress.
        let mut acc = 0u32;
        let start = std::time::Instant::now();
        for i in 0..iterations {
            let address = ORDINARY + ((i.wrapping_mul(ROW as u32)) & (LEN - ROW as u32));
            acc += u32::from(view.route(address, ROW, Some(LEN)) != GuestMemoryRoute::Unmapped);
        }
        let elapsed = start.elapsed();
        std::hint::black_box(acc);
        eprintln!(
            "bulk_range_routing_microbench ordinary: {iterations} rows in {elapsed:?} ({:?}/row)",
            elapsed / iterations
        );
    }

    /// The addresses used by the ledger-cache agreement tests: both ends of
    /// each backing, the boundaries between backings, and an address outside
    /// every mapping.
    const CACHE_PROBE_ADDRESSES: [u32; 12] = [
        0x0001_fffc, // word just below the shared alias
        0x0002_0000, // first word of the shared alias
        0x0002_0ffc, // last word of the shared alias
        0x0002_1000, // first word of the gap above it
        0x0100_0000, // first word of the ordinary writable region
        0x0100_1000, // first word of the ordinary read-only region
        0x0100_2000, // gap between the ordinary regions
        0x0200_0000, // outside every mapping
        0x0300_0ffc, // last word of the read-only region
        0x0300_1000, // first word past it
        0xffff_fffc, // top of the address space
        0x0000_0000, // bottom of the address space
    ];

    /// Drop the resident ledger answers so the next access resolves from
    /// scratch. Only tests need this; production invalidation happens in
    /// [`GuestAddressSpaceState::push_shared_mapping`].
    fn clear_ledger_caches(memory: &mut GuestAddressSpace) {
        let state = memory.state_mut();
        state.instruction_lookup.clear();
        state.data_lookup.clear();
        state.route_lookup.clear();
    }

    fn mixed_backing_memory() -> GuestAddressSpace {
        use crate::memory::bus::SharedRamRegion;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x0100_0000, vec![0x11; 0x1000]);
        memory.add_readonly_region(0x0100_1000, vec![0x22; 0x1000]);
        memory.add_region(0x0300_0000, vec![0x33; 0x1000]);
        // SAFETY: the test owns every allocation and accesses one address
        // space at a time, retaining no RAM slice or fast-memory window.
        unsafe {
            memory.add_shared_region(
                0x0002_0000,
                SharedRamRegion::from_owned_bytes(vec![0x44; 0x1000]),
            );
        }
        memory
    }

    /// A resident ledger answer must never change what an access resolves to.
    /// Comparing a warm cache against a cleared one across every backing
    /// boundary is the reference check for that.
    #[test]
    fn scalar_accesses_agree_with_a_cold_ledger_cache() {
        let mut memory = mixed_backing_memory();

        // Warm the caches with a different, unrelated address first so the
        // comparison is never accidentally against a freshly cleared state.
        let _ = PpcMemory::read_u32_be(&mut memory, 0x0100_0000);

        for &address in &CACHE_PROBE_ADDRESSES {
            for _ in 0..2 {
                let warm = PpcMemory::read_u32_be(&mut memory, address);
                clear_ledger_caches(&mut memory);
                let cold = PpcMemory::read_u32_be(&mut memory, address);
                assert_eq!(warm, cold, "data read at {address:#010x}");

                let warm = PpcMemory::read_instruction_u32_be(&mut memory, address);
                clear_ledger_caches(&mut memory);
                let cold = PpcMemory::read_instruction_u32_be(&mut memory, address);
                assert_eq!(warm, cold, "instruction read at {address:#010x}");
            }
        }

        // Writes are compared on two copies of the same state: a warm cache and
        // a cold one, with the resulting bytes read back byte-wise.
        for &address in &CACHE_PROBE_ADDRESSES {
            if address >= 0xffff_fffc || address < 0x0000_0004 {
                continue;
            }
            let mut warm = mixed_backing_memory();
            let mut cold = mixed_backing_memory();
            let _ = PpcMemory::read_u32_be(&mut warm, 0x0100_0000);
            clear_ledger_caches(&mut cold);
            let warm_result = PpcMemory::write_u32_be(&mut warm, address, 0xdead_beef);
            let cold_result = PpcMemory::write_u32_be(&mut cold, address, 0xdead_beef);
            assert_eq!(warm_result, cold_result, "write at {address:#010x}");
            for offset in 0..4 {
                assert_eq!(
                    PpcMemory::read_u8(&mut warm, address + offset),
                    PpcMemory::read_u8(&mut cold, address + offset),
                    "byte {offset} of the write at {address:#010x}"
                );
            }
        }
    }

    /// More distinct intervals than the cache holds, revisited in rotation:
    /// every access misses, evicts, and must still resolve correctly.
    #[test]
    fn scalar_accesses_survive_ledger_cache_eviction() {
        use crate::memory::bus::SharedRamRegion;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x4000_0000, vec![0x55; 0x1000]);
        let count = super::LOOKUP_SLOTS * 3;
        for index in 0..count {
            // SAFETY: see `mixed_backing_memory`.
            unsafe {
                memory.add_shared_region(
                    0x0002_0000 + index as u32 * 0x0002_0000,
                    SharedRamRegion::from_owned_bytes(vec![index as u8; 0x1000]),
                );
            }
        }

        let probes: Vec<u32> = (0..count)
            .map(|index| 0x0002_0000 + index as u32 * 0x0002_0000 + 8)
            .chain([0x4000_0008, 0x0002_0008, 0x1000_0000])
            .collect();
        for _ in 0..3 {
            for &address in &probes {
                let warm = PpcMemory::read_u32_be(&mut memory, address);
                clear_ledger_caches(&mut memory);
                let cold = PpcMemory::read_u32_be(&mut memory, address);
                assert_eq!(warm, cold, "read at {address:#010x}");
            }
        }
    }

    /// A new shared mapping takes precedence immediately, so it must also drop
    /// the ledger answers and code tokens that describe the address it covers.
    #[test]
    fn push_shared_mapping_invalidates_ledger_answers_and_tokens() {
        use crate::memory::bus::SharedRamRegion;

        const CODE: u32 = 0x0500_0000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(CODE, vec![0x66; 0x1000]);
        assert_eq!(PpcMemory::read_u32_be(&mut memory, CODE), Some(0x6666_6666));
        assert_eq!(PpcMemory::read_instruction_u32_be(&mut memory, CODE), Some(0x6666_6666));
        let token = PpcMemory::instruction_cache_token(&mut memory, CODE).expect("sparse token");

        // SAFETY: see `mixed_backing_memory`.
        unsafe {
            memory.add_shared_region(CODE, SharedRamRegion::from_owned_bytes(vec![0x77; 0x1000]));
        }

        assert_eq!(
            PpcMemory::read_u32_be(&mut memory, CODE),
            Some(0x7777_7777),
            "the newest shared alias must win on the first access"
        );
        assert_eq!(
            PpcMemory::read_instruction_u32_be(&mut memory, CODE),
            Some(0x7777_7777)
        );
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, CODE),
            None,
            "shared-mapped code has no writable-code token"
        );
        // The retired sparse token must not be handed out again for the page.
        assert_ne!(token, 0);
    }

    /// The scalar fast path must keep reporting writes: a write that lands in
    /// executed code rotates its token even when no shared mapping overlaps.
    #[test]
    fn scalar_write_without_shared_overlap_still_retires_code_tokens() {
        const CODE: u32 = 0x0600_0000;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(CODE, vec![0x60; 0x1000]);
        let first = PpcMemory::instruction_cache_token(&mut memory, CODE).expect("token");
        PpcMemory::write_u32_be(&mut memory, CODE, 0x1122_3344).expect("writable code");
        let second = PpcMemory::instruction_cache_token(&mut memory, CODE).expect("token");
        assert_ne!(first, second, "a write must retire the page's token");

        // A read-only ordinary region cannot accept the write, and the failure
        // must not depend on the ledger cache being warm.
        memory.add_readonly_region(0x0700_0000, vec![0x88; 0x1000]);
        let _ = PpcMemory::read_u32_be(&mut memory, 0x0700_0000);
        assert_eq!(PpcMemory::write_u32_be(&mut memory, 0x0700_0000, 1), None);
        clear_ledger_caches(&mut memory);
        assert_eq!(PpcMemory::write_u32_be(&mut memory, 0x0700_0000, 1), None);
    }

    /// Every whole-range probe with the route it must produce, for the tests
    /// that compare the cached ledger against a cleared one.
    ///
    /// The probes cover: a writable alias and its interior, the alias end (a
    /// partially shared range), a read-only alias, two adjacent writable
    /// aliases (no single ledger run spans both), a covered range followed by a
    /// gap, ordinary writable and read-only regions, and unmapped gaps.
    const BULK_RANGE_PROBES: [(u32, usize, GuestMemoryRoute); 10] = [
        (0x0002_0000, 0x1000, GuestMemoryRoute::Shared),
        (0x0002_0400, 0x0100, GuestMemoryRoute::Shared),
        (0x0002_0f80, 0x0100, GuestMemoryRoute::Mixed),
        (0x0010_0400, 0x0100, GuestMemoryRoute::SharedReadOnly),
        (0x0040_0800, 0x1000, GuestMemoryRoute::Shared),
        (0x0040_1800, 0x1000, GuestMemoryRoute::Mixed),
        (0x0100_0100, 0x0100, GuestMemoryRoute::Sparse),
        (0x0200_0100, 0x0100, GuestMemoryRoute::Sparse),
        (0x0003_0000, 0x0100, GuestMemoryRoute::Unmapped),
        (0x0500_0000, 0x0100, GuestMemoryRoute::Unmapped),
    ];

    /// A writable shared alias, a read-only alias, adjacent writable aliases
    /// that no single ledger run spans, and ordinary mappings.
    fn bulk_backing_memory() -> GuestAddressSpace {
        use crate::memory::bus::SharedRamRegion;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x0100_0000, vec![0x11; 0x1000]);
        memory.add_readonly_region(0x0200_0000, vec![0x22; 0x1000]);
        memory.add_region(0x0300_0000, vec![0x33; 0x1000]);
        // SAFETY: see `mixed_backing_memory`.
        unsafe {
            memory.add_shared_region(
                0x0002_0000,
                SharedRamRegion::from_owned_bytes(vec![0x44; 0x1000]),
            );
            memory.add_shared_readonly_region(
                None,
                0x0010_0000,
                SharedRamRegion::from_owned_bytes(vec![0x55; 0x1000]),
            );
            memory.add_shared_region(
                0x0040_0000,
                SharedRamRegion::from_owned_bytes(vec![0x66; 0x1000]),
            );
            memory.add_shared_region(
                0x0040_1000,
                SharedRamRegion::from_owned_bytes(vec![0x77; 0x1000]),
            );
        }
        memory
    }

    /// The whole-range fast path must agree with a cleared ledger for every
    /// backing it can be asked about, including ranges that cross ledger
    /// boundaries and therefore fall back to the walking classifier.
    #[test]
    fn bulk_range_routes_match_every_backing_and_a_cold_cache() {
        let mut memory = bulk_backing_memory();
        // Warm the cache with an unrelated range first, so a comparison is
        // never accidentally against a freshly cleared state.
        let _ = memory.shared_view().route(0x0100_0100, 0x40, None);

        for &(address, len, expected) in &BULK_RANGE_PROBES {
            for round in 0..2 {
                let warm = memory.shared_view().route(address, len, None);
                assert_eq!(warm, expected, "warm route {address:#010x}+{len:#x}");
                clear_ledger_caches(&mut memory);
                let cold = memory.shared_view().route(address, len, None);
                assert_eq!(cold, expected, "cold route {address:#010x}+{len:#x}");
                assert_eq!(warm, cold, "round {round} {address:#010x}+{len:#x}");
            }
        }
    }

    /// More ledger intervals than the range cache holds, revisited in rotation:
    /// every route misses and evicts, and each must still find the newest alias.
    #[test]
    fn bulk_range_routes_survive_ledger_cache_eviction() {
        use crate::memory::bus::SharedRamRegion;

        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x4000_0000, vec![0x55; 0x1000]);
        let count = super::LOOKUP_SLOTS * 3;
        for index in 0..count {
            // SAFETY: see `mixed_backing_memory`.
            unsafe {
                memory.add_shared_region(
                    0x0002_0000 + index as u32 * 0x0002_0000,
                    SharedRamRegion::from_owned_bytes(vec![index as u8; 0x1000]),
                );
            }
        }

        for round in 0..3 {
            for index in 0..count {
                let address = 0x0002_0000 + index as u32 * 0x0002_0000;
                assert_eq!(
                    memory.shared_view().route(address, 0x800, None),
                    GuestMemoryRoute::Shared,
                    "revisited alias {index} in round {round}"
                );
            }
        }
    }

    /// The classic adapter promotes a wholly-local alias to `Flat`. The cached
    /// ledger run must answer like the walking check for that case.
    #[test]
    fn shared_range_is_local_flat_matches_a_wholly_local_alias() {
        use crate::memory::bus::SharedRamRegion;

        // A region at base 0 has offset 0, so it can be the local alias itself.
        let local = SharedRamRegion::from_owned_bytes(vec![0u8; 0x3000]);
        let other = SharedRamRegion::from_owned_bytes(vec![0u8; 0x3000]);
        let mut memory = GuestAddressSpace::new();
        // SAFETY: see `mixed_backing_memory`.
        unsafe {
            memory.add_shared_region(0x0000_0000, local.clone());
            memory.add_shared_region(0x0010_0000, other.clone());
        }

        assert!(memory
            .shared_view()
            .shared_range_is_local_flat(0x0000_0400, 0x100, &local));
        // A foreign backing is never promoted, and a range crossing the alias
        // end is not wholly shared at all.
        assert!(!memory
            .shared_view()
            .shared_range_is_local_flat(0x0000_0400, 0x100, &other));
        assert!(!memory
            .shared_view()
            .shared_range_is_local_flat(0x0000_2f80, 0x100, &local));
        assert!(!memory
            .shared_view()
            .shared_range_is_local_flat(0x0010_0400, 0x100, &local));

        clear_ledger_caches(&mut memory);
        assert!(memory
            .shared_view()
            .shared_range_is_local_flat(0x0000_0400, 0x100, &local));

        // A read-only alias over the same bytes is authoritative: the cached
        // run must stop the promotion immediately.
        // SAFETY: see `mixed_backing_memory`.
        unsafe {
            memory.add_shared_readonly_region(
                None,
                0x0000_0000,
                SharedRamRegion::from_owned_bytes(vec![0u8; 0x400]),
            );
        }
        assert!(
            !memory
                .shared_view()
                .shared_range_is_local_flat(0x0000_0000, 0x100, &local),
            "a read-only alias must not be promoted"
        );
        // The rest of the alias is still the local writable one.
        assert!(memory
            .shared_view()
            .shared_range_is_local_flat(0x0000_0400, 0x100, &local));
    }

    /// An appended mapping invalidates whole-range answers immediately, for the
    /// router and for the local-alias promotion alike.
    #[test]
    fn push_shared_mapping_invalidates_bulk_range_answers() {
        use crate::memory::bus::SharedRamRegion;

        let local = SharedRamRegion::from_owned_bytes(vec![0x11; 0x1000]);
        let shadow = SharedRamRegion::from_owned_bytes(vec![0x22; 0x0400]);
        let mut memory = GuestAddressSpace::new();
        // SAFETY: see `mixed_backing_memory`.
        unsafe {
            memory.add_shared_region(0x0002_0000, local.clone());
        }
        assert_eq!(
            memory.shared_view().route(0x0002_0000, 0x800, None),
            GuestMemoryRoute::Shared
        );

        // A shorter read-only alias over the same bytes is authoritative from
        // the first access, so the cached run must not still decide the route.
        // SAFETY: see `mixed_backing_memory`.
        unsafe {
            memory.add_shared_readonly_region(None, 0x0002_0000, shadow);
        }
        assert_eq!(
            memory.shared_view().route(0x0002_0000, 0x100, None),
            GuestMemoryRoute::SharedReadOnly
        );
        assert_eq!(
            memory.shared_view().route(0x0002_0000, 0x800, None),
            GuestMemoryRoute::Mixed,
            "a range over the read-only alias and the writable one is mixed"
        );
        clear_ledger_caches(&mut memory);
        assert_eq!(
            memory.shared_view().route(0x0002_0000, 0x100, None),
            GuestMemoryRoute::SharedReadOnly
        );
    }
}
