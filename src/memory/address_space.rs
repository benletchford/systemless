//! Architecture-neutral mapped guest address space.
//!
//! CPU backends have different bus contracts, but guest bytes must have one
//! owner. This type provides that owner while preserving the sparse mappings,
//! read-only regions, and instruction-cache behavior required by native PEF
//! applications.

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
}

#[derive(Debug, Clone, Copy)]
struct OrdinaryRegionMapping {
    base: u32,
    len: usize,
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
    readonly_allocation_exclusions: Vec<(u32, u32)>,
}

#[derive(Debug, Default)]
pub struct GuestAddressSpace(Rc<UnsafeCell<GuestAddressSpaceState>>);

/// A shared view of one process address space.
///
/// CPU adapters retain this handle while the runner serializes their access.
/// Ordinary [`GuestAddressSpace::clone`] operations remain detached snapshots.
#[derive(Clone, Debug)]
pub(crate) struct SharedGuestAddressSpace(Rc<UnsafeCell<GuestAddressSpaceState>>);

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

    fn shared_overlaps(&self, address: u32, len: u32) -> bool {
        if len == 0 {
            return false;
        }
        let start = u64::from(address);
        let end = start + u64::from(len);
        self.state_mut().shared_regions.iter().any(|mapping| {
            let mapping_start = u64::from(mapping.base);
            let mapping_end = mapping_start.saturating_add(mapping.region.len() as u64);
            start < mapping_end && mapping_start < end
        })
    }

    #[inline]
    pub(crate) fn read_u8(&self, address: u32) -> Option<u8> {
        if self.shared_overlaps(address, 1) {
            return None;
        }
        PpcMemory::read_u8(&mut self.state_mut().regions, address)
    }

    #[inline]
    pub(crate) fn read_u16_be(&self, address: u32) -> Option<u16> {
        if self.shared_overlaps(address, 2) {
            return None;
        }
        PpcMemory::read_u16_be(&mut self.state_mut().regions, address)
    }

    #[inline]
    pub(crate) fn read_u32_be(&self, address: u32) -> Option<u32> {
        if self.shared_overlaps(address, 4) {
            return None;
        }
        PpcMemory::read_u32_be(&mut self.state_mut().regions, address)
    }

    #[inline]
    pub(crate) fn write_u8(&self, address: u32, value: u8) -> Option<()> {
        if self.shared_overlaps(address, 1) {
            return None;
        }
        PpcMemory::write_u8(&mut self.state_mut().regions, address, value)
    }

    #[inline]
    pub(crate) fn write_u16_be(&self, address: u32, value: u16) -> Option<()> {
        if self.shared_overlaps(address, 2) {
            return None;
        }
        PpcMemory::write_u16_be(&mut self.state_mut().regions, address, value)
    }

    #[inline]
    pub(crate) fn write_u32_be(&self, address: u32, value: u32) -> Option<()> {
        if self.shared_overlaps(address, 4) {
            return None;
        }
        PpcMemory::write_u32_be(&mut self.state_mut().regions, address, value)
    }

    /// Whether an address belongs to an ordinary sparse native mapping rather
    /// than a runner-owned shared flat-RAM overlay.
    #[inline]
    pub(crate) fn is_ordinary_sparse_mapped(&self, address: u32) -> bool {
        !self.shared_overlaps(address, 1) && self.state_mut().regions.read_u8(address).is_some()
    }

    pub(crate) fn ordinary_mapping_overlaps(&self, address: u32, len: u32) -> bool {
        if len == 0 {
            return false;
        }
        let range_start = u64::from(address);
        let range_end = range_start + u64::from(len);
        let state = self.state_mut();
        state.ordinary_regions.iter().any(|ordinary| {
            let mut cursor = u64::from(ordinary.base).max(range_start);
            let ordinary_end = u64::from(ordinary.base)
                .saturating_add(ordinary.len as u64)
                .min(range_end);
            while cursor < ordinary_end {
                let covered_end = state
                    .shared_regions
                    .iter()
                    .filter_map(|mapping| {
                        let mapping_start = u64::from(mapping.base);
                        let mapping_end =
                            mapping_start.saturating_add(mapping.region.len() as u64);
                        (mapping_start <= cursor && cursor < mapping_end).then_some(mapping_end)
                    })
                    .max();
                let Some(covered_end) = covered_end else {
                    return true;
                };
                cursor = covered_end;
            }
            false
        })
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
            shared_regions: state
                .shared_regions
                .iter()
                .map(|mapping| SharedRegionMapping {
                    base: mapping.base,
                    region: mapping.region.detached_clone(),
                    writable: mapping.writable,
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

    /// Return the disjoint holes not occupied by ordinary sparse mappings in
    /// the supplied half-open range.
    pub(crate) fn ordinary_mapping_holes(&self, start: u32, end: u32) -> Vec<(u32, u32)> {
        if start >= end {
            return Vec::new();
        }

        let mut occupied = self
            .state()
            .ordinary_regions
            .iter()
            .filter_map(|mapping| {
                let mapping_start = u64::from(mapping.base).max(u64::from(start));
                let mapping_end = u64::from(mapping.base)
                    .saturating_add(mapping.len as u64)
                    .min(u64::from(end));
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

    /// Return the disjoint occupied spans of ordinary sparse mappings.
    ///
    /// Shared flat-RAM overlays are intentionally not subtracted: callers
    /// use these spans to keep process allocators away from the native PEF
    /// layout before those overlays are installed. Inside Macintosh: Memory
    /// (1992), pp. 2-19--2-21.
    pub(crate) fn ordinary_mapping_ranges(&self) -> Vec<(u32, u32)> {
        let mut ranges = self
            .state()
            .ordinary_regions
            .iter()
            .filter_map(|mapping| {
                let end = u64::from(mapping.base).checked_add(mapping.len as u64)?;
                (u64::from(mapping.base) < end)
                    .then_some((mapping.base, u32::try_from(end).ok()?))
            })
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
        self.state_mut().shared_regions.push(SharedRegionMapping {
            base,
            region,
            writable: true,
        });
    }

    /// Overlay runner-owned system code without allowing ordinary guest
    /// writes. Trap Manager uses the privileged writer below when it must
    /// update the protected exit of a permanent come-from head.
    ///
    /// # Safety
    ///
    /// The ownership and serialization requirements are the same as for
    /// [`Self::add_shared_region`].
    pub(crate) unsafe fn add_shared_readonly_region(&mut self, base: u32, region: SharedRamRegion) {
        let state = self.state_mut();
        if let Ok(len) = u32::try_from(region.len()) {
            state
                .readonly_allocation_exclusions
                .retain(|&(excluded_base, excluded_len)| {
                    (excluded_base, excluded_len) != (base, len)
                });
        }
        state.shared_regions.push(SharedRegionMapping {
            base,
            region,
            writable: false,
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

    /// Write a big-endian long through a shared mapping regardless of its
    /// guest write protection. This is intentionally restricted to runtime
    /// services that own the mapped system bytes.
    pub(crate) fn write_shared_system_u32_be(&mut self, address: u32, value: u32) -> Option<()> {
        for (offset, byte) in value.to_be_bytes().into_iter().enumerate() {
            let (mapping, relative) =
                self.locate_shared_mapping(address.checked_add(offset as u32)?)?;
            // SAFETY: shared mappings can only be installed by the serialized
            // process runner, and this method does not retain a source view.
            unsafe { mapping.region.write(relative, byte)? };
        }
        Some(())
    }

    pub(crate) fn is_shared_readonly_address(&self, address: u32) -> bool {
        self.locate_shared_mapping(address)
            .is_some_and(|(mapping, _)| !mapping.writable)
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
        if !self.shared_overlaps(addr, dst.len()) {
            return self.state_mut().regions.read_bytes_into(addr, dst);
        }
        for (offset, byte) in dst.iter_mut().enumerate() {
            *byte = self.read_u8(addr.wrapping_add(offset as u32))?;
        }
        Some(())
    }

    /// Copy `src` into a fully mapped, writable range.
    pub fn write_bytes(&mut self, addr: u32, src: &[u8]) -> Option<()> {
        if !self.shared_overlaps(addr, src.len()) {
            return self.state_mut().regions.write_bytes(addr, src);
        }
        if (0..src.len()).any(|offset| {
            self.locate_shared_mapping(addr.wrapping_add(offset as u32))
                .is_some_and(|(mapping, _)| !mapping.writable)
        }) {
            return None;
        }
        if (0..src.len()).all(|offset| {
            self.locate_shared(addr.wrapping_add(offset as u32))
                .is_some()
        }) {
            for (offset, byte) in src.iter().copied().enumerate() {
                self.write_u8(addr.wrapping_add(offset as u32), byte)
                    .expect("located shared byte remains mapped");
            }
            return Some(());
        }

        let mut ordinary = Vec::new();
        let mut shared = Vec::new();
        for (offset, byte) in src.iter().copied().enumerate() {
            let address = addr.wrapping_add(offset as u32);
            if self.locate_shared(address).is_some() {
                shared.push((address, byte));
            } else {
                ordinary.push((address, self.state_mut().regions.read_u8(address)?, byte));
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

    /// Return a cached writable span contained in one mapped region.
    pub fn writable_span(&mut self, addr: u32, len: usize) -> Option<PpcSectionMemSpan> {
        if self.shared_overlaps(addr, len) {
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
        self.state().shared_regions.iter().rev().find_map(|mapping| {
            let offset = usize::try_from(addr.checked_sub(mapping.base)?).ok()?;
            (offset < mapping.region.len()).then_some((mapping, offset))
        })
    }

    fn locate_shared(&self, addr: u32) -> Option<(&SharedRamRegion, usize)> {
        self.locate_shared_mapping(addr)
            .map(|(mapping, offset)| (&mapping.region, offset))
    }

    fn shared_overlaps(&self, addr: u32, len: usize) -> bool {
        if len == 0 {
            return false;
        }
        const ADDRESS_SPACE_SIZE: u64 = 1u64 << 32;
        let start = u64::from(addr);
        let len = len as u64;
        if len >= ADDRESS_SPACE_SIZE {
            return !self.state().shared_regions.is_empty();
        }
        let end = start + len;
        self.state().shared_regions.iter().any(|mapping| {
            let mapping_start = u64::from(mapping.base);
            let mapping_end = mapping_start.saturating_add(mapping.region.len() as u64);
            if end <= ADDRESS_SPACE_SIZE {
                start < mapping_end && mapping_start < end
            } else {
                start < mapping_end || mapping_start < end - ADDRESS_SPACE_SIZE
            }
        })
    }
}

impl PpcMemory for GuestAddressSpace {
    #[inline]
    fn read_u8(&mut self, addr: u32) -> Option<u8> {
        if let Some((region, offset)) = self.locate_shared(addr) {
            // SAFETY: `add_shared_region` requires the enclosing runtime to
            // serialize both adapters for the mapping's complete lifetime.
            unsafe { region.read(offset) }
        } else {
            self.state_mut().regions.read_u8(addr)
        }
    }

    #[inline]
    fn read_u16_be(&mut self, addr: u32) -> Option<u16> {
        if !self.shared_overlaps(addr, 2) {
            return self.state_mut().regions.read_u16_be(addr);
        }
        let mut bytes = [0; 2];
        self.read_bytes_into(addr, &mut bytes)?;
        Some(u16::from_be_bytes(bytes))
    }

    #[inline]
    fn read_u32_be(&mut self, addr: u32) -> Option<u32> {
        if !self.shared_overlaps(addr, 4) {
            return self.state_mut().regions.read_u32_be(addr);
        }
        let mut bytes = [0; 4];
        self.read_bytes_into(addr, &mut bytes)?;
        Some(u32::from_be_bytes(bytes))
    }

    #[inline]
    fn read_u64_be(&mut self, addr: u32) -> Option<u64> {
        if !self.shared_overlaps(addr, 8) {
            return self.state_mut().regions.read_u64_be(addr);
        }
        let mut bytes = [0; 8];
        self.read_bytes_into(addr, &mut bytes)?;
        Some(u64::from_be_bytes(bytes))
    }

    #[inline]
    fn read_instruction_u32_be(&mut self, addr: u32) -> Option<u32> {
        if self.shared_overlaps(addr, 4) {
            self.read_u32_be(addr)
        } else {
            self.state_mut().regions.read_instruction_u32_be(addr)
        }
    }

    #[inline]
    fn instruction_cache_token(&mut self, addr: u32) -> Option<u64> {
        if self.shared_overlaps(addr, 4) {
            None
        } else {
            self.state_mut().regions.instruction_cache_token(addr)
        }
    }

    #[inline]
    fn write_u8(&mut self, addr: u32, value: u8) -> Option<()> {
        if let Some((mapping, offset)) = self.locate_shared_mapping(addr) {
            if !mapping.writable {
                return None;
            }
            // SAFETY: `add_shared_region` requires the enclosing runtime to
            // serialize both adapters for the mapping's complete lifetime.
            unsafe { mapping.region.write(offset, value) }
        } else {
            self.state_mut().regions.write_u8(addr, value)
        }
    }

    #[inline]
    fn write_u16_be(&mut self, addr: u32, value: u16) -> Option<()> {
        if !self.shared_overlaps(addr, 2) {
            return self.state_mut().regions.write_u16_be(addr, value);
        }
        self.write_bytes(addr, &value.to_be_bytes())
    }

    #[inline]
    fn write_u32_be(&mut self, addr: u32, value: u32) -> Option<()> {
        if !self.shared_overlaps(addr, 4) {
            return self.state_mut().regions.write_u32_be(addr, value);
        }
        self.write_bytes(addr, &value.to_be_bytes())
    }

    #[inline]
    fn write_u64_be(&mut self, addr: u32, value: u64) -> Option<()> {
        if !self.shared_overlaps(addr, 8) {
            return self.state_mut().regions.write_u64_be(addr, value);
        }
        self.write_bytes(addr, &value.to_be_bytes())
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
    use super::GuestAddressSpace;
    use crate::memory::{MacMemoryBus, MemoryBus};
    use m68k::{AddressBus, CpuCore, StepResult};
    use ppc::{PpcCpu, PpcMemory, PpcRunResult};

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
    fn ordinary_mapping_holes_track_writable_and_readonly_regions() {
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x1200, vec![0; 0x100]);
        memory.add_readonly_region(0x1400, vec![0; 0x200]);
        memory.add_region(0x1500, vec![0; 0x200]);

        assert_eq!(
            memory.ordinary_mapping_holes(0x1000, 0x1800),
            vec![(0x1000, 0x1200), (0x1300, 0x1400), (0x1700, 0x1800)]
        );
        assert_eq!(
            memory.ordinary_mapping_ranges(),
            vec![(0x1200, 0x1300), (0x1400, 0x1700)]
        );
        assert!(memory.ordinary_mapping_overlaps(0x1280, 0x100));
        assert!(!memory.ordinary_mapping_overlaps(0x1300, 0x100));

        let detached = memory.clone();
        assert_eq!(
            detached.ordinary_mapping_holes(0x1000, 0x1800),
            vec![(0x1000, 0x1200), (0x1300, 0x1400), (0x1700, 0x1800)]
        );
        assert_eq!(
            detached.ordinary_mapping_ranges(),
            vec![(0x1200, 0x1300), (0x1400, 0x1700)]
        );
    }
}
