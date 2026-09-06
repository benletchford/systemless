//! Architecture-neutral mapped guest address space.
//!
//! CPU backends have different bus contracts, but guest bytes must have one
//! owner. This type provides that owner while preserving the sparse mappings,
//! read-only regions, and instruction-cache behavior required by native PEF
//! applications.

use crate::guest_procedure::GuestIsa;
use m68k::core::memory::{BusFault, BusFaultKind};
use m68k::AddressBus;
use ppc::{PpcMemory, PpcSectionMem, PpcSectionMemSpan};
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
    regions: PpcSectionMem,
    ordinary_regions: Vec<OrdinaryRegionMapping>,
    shared_regions: Vec<SharedRegionMapping>,
    /// Derived envelope of nonempty shared mappings; extended on every insert
    /// and copied with detached snapshots. It only rejects impossible hits.
    shared_bounds: std::ops::Range<u64>,
    readonly_allocation_exclusions: Vec<(u32, u32)>,
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
        }
        self.shared_regions.push(mapping);
    }

    #[inline]
    fn may_overlap_shared(&self, start: u64, end: u64) -> bool {
        start < self.shared_bounds.end && self.shared_bounds.start < end
    }

    #[inline]
    fn overlaps_shared(&self, start: u64, end: u64) -> bool {
        self.may_overlap_shared(start, end) && ranges_overlap(start, end, shared_ranges(self))
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

/// Classify a completely shared range while retaining its write protection.
/// This walks mapping boundaries rather than bytes, so a large writable alias
/// still reaches the classic bus's bulk fast path.
#[inline]
fn shared_range_route(
    state: &GuestAddressSpaceState,
    start: u64,
    end: u64,
) -> GuestMemoryRoute {
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
    state
        .shared_regions
        .iter()
        .filter_map(|mapping| Some((u64::from(mapping.base), mapping_end(mapping.base, mapping.region.len())?)))
}

#[inline]
fn ordinary_ranges(state: &GuestAddressSpaceState) -> impl Iterator<Item = (u64, u64)> + '_ {
    state
        .ordinary_regions
        .iter()
        .filter_map(|mapping| Some((u64::from(mapping.base), mapping_end(mapping.base, mapping.len)?)))
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

    let shared_overlap = state.overlaps_shared(start, end);
    if shared_overlap {
        if ranges_cover_shared(state, start, end) {
            return shared_range_route(state, start, end);
        }
        return GuestMemoryRoute::Mixed;
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
fn sparse_mapping_overlaps_state(
    state: &GuestAddressSpaceState,
    address: u32,
    len: usize,
) -> bool {
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
    match route_byte_state(state, address, flat_limit) {
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
    }
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
        return PpcMemory::write_u16_be(&mut state.regions, address, value);
    }
    let bytes = value.to_be_bytes();
    for offset in 0..bytes.len() {
        if !routed_byte_is_writable_state(
            state,
            address.wrapping_add(offset as u32),
            flat_limit,
        ) {
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
        return PpcMemory::write_u32_be(&mut state.regions, address, value);
    }
    let bytes = value.to_be_bytes();
    for offset in 0..bytes.len() {
        if !routed_byte_is_writable_state(
            state,
            address.wrapping_add(offset as u32),
            flat_limit,
        ) {
            return None;
        }
    }
    for (offset, byte) in bytes.into_iter().enumerate() {
        write_routed_u8_state(state, address.wrapping_add(offset as u32), byte, flat_limit)?;
    }
    Some(())
}

impl SharedGuestAddressSpace {
    fn new(memory: &GuestAddressSpace) -> Self {
        Self(Rc::clone(&memory.0))
    }

    fn state_mut(&self) -> &mut GuestAddressSpaceState {
        // SAFETY: the process runner serializes all CPU-adapter access.
        unsafe { &mut *self.0.get() }
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
        route_range_state(self.state_mut(), address, len, flat_limit)
    }

    #[inline]
    pub(crate) fn route_byte(
        &self,
        address: u32,
        flat_limit: Option<u32>,
    ) -> GuestMemoryRoute {
        route_byte_state(self.state_mut(), address, flat_limit)
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
        if len == 0 || route_range_state(self.state_mut(), address, len, None)
            != GuestMemoryRoute::Shared
        {
            return false;
        }
        let Some(end) = range_end(address, len) else {
            return false;
        };
        let mut cursor = u64::from(address);
        while cursor < end {
            let guest = u32::try_from(cursor).expect("guest range remains 32-bit");
            let state = self.state_mut();
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
    }

    /// Read from a mapped non-flat backing selected by the shared router.
    /// `None` means that the route belongs to the classic adapter's local flat
    /// RAM or is unmapped; the caller must not fall through when the route is
    /// `Shared`/`Sparse` because read-only mappings are still authoritative.
    #[inline]
    pub(crate) fn read_routed_u8(
        &self,
        address: u32,
        flat_limit: Option<u32>,
    ) -> Option<u8> {
        read_routed_u8_state(self.state_mut(), address, flat_limit)
    }

    #[inline]
    pub(crate) fn read_routed_u16(
        &self,
        address: u32,
        flat_limit: Option<u32>,
    ) -> Option<u16> {
        read_routed_u16_state(self.state_mut(), address, flat_limit)
    }

    #[inline]
    pub(crate) fn read_routed_u32(
        &self,
        address: u32,
        flat_limit: Option<u32>,
    ) -> Option<u32> {
        read_routed_u32_state(self.state_mut(), address, flat_limit)
    }

    #[inline]
    pub(crate) fn write_routed_u8(
        &self,
        address: u32,
        value: u8,
        flat_limit: Option<u32>,
    ) -> Option<()> {
        write_routed_u8_state(self.state_mut(), address, value, flat_limit)
    }

    #[inline]
    pub(crate) fn write_routed_u16(
        &self,
        address: u32,
        value: u16,
        flat_limit: Option<u32>,
    ) -> Option<()> {
        write_routed_u16_state(self.state_mut(), address, value, flat_limit)
    }

    #[inline]
    pub(crate) fn write_routed_u32(
        &self,
        address: u32,
        value: u32,
        flat_limit: Option<u32>,
    ) -> Option<()> {
        write_routed_u32_state(self.state_mut(), address, value, flat_limit)
    }

    #[inline]
    pub(crate) fn routed_byte_is_writable(
        &self,
        address: u32,
        flat_limit: Option<u32>,
    ) -> bool {
        routed_byte_is_writable_state(self.state_mut(), address, flat_limit)
    }

    /// Whether an address belongs to an ordinary sparse native mapping rather
    /// than a runner-owned shared flat-RAM overlay.
    #[inline]
    #[cfg(test)]
    pub(crate) fn is_ordinary_sparse_mapped(&self, address: u32) -> bool {
        self.route_byte(address, None) == GuestMemoryRoute::Sparse
    }

    pub(crate) fn sparse_mapping_overlaps(&self, address: u32, len: u32) -> bool {
        sparse_mapping_overlaps_state(self.state_mut(), address, len as usize)
    }

    /// Return the end of the highest read-only runtime reservation overlapping
    /// a candidate native heap allocation.
    #[inline]
    pub(crate) fn readonly_allocation_overlap_end(
        &self,
        address: u32,
        len: u32,
    ) -> Option<u32> {
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
    pub(crate) fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut GuestAddressSpace) -> R,
    ) -> R {
        f(&mut self.adapter())
    }
}

impl Clone for GuestAddressSpace {
    fn clone(&self) -> Self {
        let state = self.state();
        Self(Rc::new(UnsafeCell::new(GuestAddressSpaceState {
            regions: state.regions.clone(),
            ordinary_regions: state.ordinary_regions.clone(),
            shared_bounds: state.shared_bounds.clone(),
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
        })))
    }
}

impl GuestAddressSpace {
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
    }

    /// Map a read-only region. Newer mappings take precedence over overlaps.
    pub fn add_readonly_region(&mut self, base: u32, bytes: Vec<u8>) {
        let state = self.state_mut();
        state.ordinary_regions.push(OrdinaryRegionMapping {
            base,
            len: bytes.len(),
        });
        state.regions.add_readonly_region(base, bytes);
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
        if !state
            .readonly_allocation_exclusions
            .contains(&(base, len))
        {
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
        let exclusion_ends = state
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
    pub(crate) fn readonly_allocation_available_bytes(
        &self,
        start: u32,
        end: u32,
    ) -> (u32, u32) {
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
                let mapping_end = u64::from(base)
                    .checked_add(u64::from(len))?
                    .min(range_end);
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
        let mut reserved = excluded
            .chain(shared)
            .collect::<Vec<_>>();
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
        match self.route(addr, dst.len(), None) {
            GuestMemoryRoute::Sparse => self.state_mut().regions.read_bytes_into(addr, dst),
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
        match self.route(addr, src.len(), None) {
            GuestMemoryRoute::Sparse => return self.state_mut().regions.write_bytes(addr, src),
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
                    ordinary.push((
                        address,
                        self.state_mut().regions.read_u8(address)?,
                        byte,
                    ));
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
    pub fn writable_span(&mut self, addr: u32, len: usize) -> Option<PpcSectionMemSpan> {
        if self.route(addr, len, None) != GuestMemoryRoute::Sparse {
            return None;
        }
        self.state_mut().regions.writable_span(addr, len)
    }

    /// Read a big-endian word at an offset within a cached span.
    pub fn read_u16_be_in_span(
        &self,
        span: PpcSectionMemSpan,
        relative_offset: usize,
    ) -> Option<u16> {
        self.state()
            .regions
            .read_u16_be_in_span(span, relative_offset)
    }

    /// Write a big-endian word at an offset within a cached writable span.
    pub fn write_u16_be_in_span(
        &mut self,
        span: PpcSectionMemSpan,
        relative_offset: usize,
        value: u16,
    ) -> Option<()> {
        self.state_mut()
            .regions
            .write_u16_be_in_span(span, relative_offset, value)
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
        read_routed_u8_state(self.state_mut(), addr, None)
    }

    #[inline]
    fn read_u16_be(&mut self, addr: u32) -> Option<u16> {
        match self.route(addr, 2, None) {
            GuestMemoryRoute::Sparse
            | GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly => {
                read_routed_u16_state(self.state_mut(), addr, None)
            }
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
        match self.route(addr, 4, None) {
            GuestMemoryRoute::Sparse
            | GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly => {
                read_routed_u32_state(self.state_mut(), addr, None)
            }
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
        match self.route(addr, 8, None) {
            GuestMemoryRoute::Sparse => self.state_mut().regions.read_u64_be(addr),
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
        match self.route(addr, 4, None) {
            GuestMemoryRoute::Sparse => {
                self.state_mut().regions.read_instruction_u32_be(addr)
            }
            GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly
            | GuestMemoryRoute::Mixed => {
                self.read_u32_be(addr)
            }
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    #[inline]
    fn instruction_cache_token(&mut self, addr: u32) -> Option<u64> {
        match self.route(addr, 4, None) {
            GuestMemoryRoute::Sparse => self.state_mut().regions.instruction_cache_token(addr),
            GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly
            | GuestMemoryRoute::Flat
            | GuestMemoryRoute::Unmapped
            | GuestMemoryRoute::Mixed => None,
        }
    }

    #[inline]
    fn write_u8(&mut self, addr: u32, value: u8) -> Option<()> {
        write_routed_u8_state(self.state_mut(), addr, value, None)
    }

    #[inline]
    fn write_u16_be(&mut self, addr: u32, value: u16) -> Option<()> {
        match self.route(addr, 2, None) {
            GuestMemoryRoute::Sparse
            | GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly => {
                write_routed_u16_state(self.state_mut(), addr, value, None)
            }
            GuestMemoryRoute::Mixed => self.write_bytes(addr, &value.to_be_bytes()),
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    #[inline]
    fn write_u32_be(&mut self, addr: u32, value: u32) -> Option<()> {
        match self.route(addr, 4, None) {
            GuestMemoryRoute::Sparse
            | GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly => {
                write_routed_u32_state(self.state_mut(), addr, value, None)
            }
            GuestMemoryRoute::Mixed => self.write_bytes(addr, &value.to_be_bytes()),
            GuestMemoryRoute::Flat | GuestMemoryRoute::Unmapped => None,
        }
    }

    #[inline]
    fn write_u64_be(&mut self, addr: u32, value: u64) -> Option<()> {
        match self.route(addr, 8, None) {
            GuestMemoryRoute::Sparse => self.state_mut().regions.write_u64_be(addr, value),
            GuestMemoryRoute::Shared
            | GuestMemoryRoute::SharedReadOnly
            | GuestMemoryRoute::Mixed => {
                self.write_bytes(addr, &value.to_be_bytes())
            }
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
    use super::{GuestAddressSpace, GuestIsa};
    use crate::memory::{MacMemoryBus, MemoryBus};
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
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE),
            None
        );

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
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, PPC_CODE),
            None
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
        assert_eq!(PpcMemory::read_u32_be(&mut moved, SPARSE), Some(0x5566_7788));
        assert_eq!(PpcMemory::read_u32_be(&mut detached, SPARSE), Some(0x1122_3344));

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
}
