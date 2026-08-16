//! Memory-bus abstraction for the PowerPC interpreter, plus the
//! [`PpcSectionMem`] multi-region implementation used by the PEF
//! loader.
//!
//! Mac PowerPC always runs big-endian (the MSR LE bit is left
//! clear), so the byte-order is hardcoded — there's no little-
//! endian variant. Implementors return `None` for unmapped
//! addresses; the dispatcher converts that into a `MemoryFault`
//! step result rather than panicking.

/// Memory-bus abstraction for the PowerPC interpreter.
///
/// Provides byte-granular reads and writes plus default
/// big-endian convenience methods at u16 / u32 granularity.
/// Mac PowerPC always runs big-endian (the MSR LE bit is left
/// clear), so the byte-order is hardcoded — there's no little-
/// endian variant.
///
/// Implementors return `None` when the access falls on an unmapped
/// address. The dispatcher converts that into a `MemoryFault`
/// step result rather than panicking, so a guest that wanders into
/// unmapped memory surfaces cleanly to the host.
pub trait PpcMemory {
    /// Read one byte from `addr`, or `None` if unmapped.
    fn read_u8(&mut self, addr: u32) -> Option<u8>;
    /// Write one byte to `addr`, or `None` if unmapped / read-only.
    fn write_u8(&mut self, addr: u32, value: u8) -> Option<()>;

    /// Read a big-endian 16-bit value. Default impl falls through
    /// to two byte reads.
    fn read_u16_be(&mut self, addr: u32) -> Option<u16> {
        let b0 = self.read_u8(addr)?;
        let b1 = self.read_u8(addr.wrapping_add(1))?;
        Some(u16::from_be_bytes([b0, b1]))
    }

    /// Write a big-endian 16-bit value. Default impl falls through
    /// to two byte writes.
    fn write_u16_be(&mut self, addr: u32, value: u16) -> Option<()> {
        let bytes = value.to_be_bytes();
        self.write_u8(addr, bytes[0])?;
        self.write_u8(addr.wrapping_add(1), bytes[1])?;
        Some(())
    }

    /// Read a big-endian 64-bit value. Default impl falls through
    /// to two `u32` reads.
    fn read_u64_be(&mut self, addr: u32) -> Option<u64> {
        let hi = self.read_u32_be(addr)?;
        let lo = self.read_u32_be(addr.wrapping_add(4))?;
        Some((u64::from(hi) << 32) | u64::from(lo))
    }

    /// Write a big-endian 64-bit value. Default impl falls through
    /// to two `u32` writes.
    fn write_u64_be(&mut self, addr: u32, value: u64) -> Option<()> {
        self.write_u32_be(addr, (value >> 32) as u32)?;
        self.write_u32_be(addr.wrapping_add(4), value as u32)?;
        Some(())
    }

    /// Read a big-endian 32-bit value. Default impl falls through
    /// to four byte reads. Implementors with native 32-bit access
    /// may override for speed.
    fn read_u32_be(&mut self, addr: u32) -> Option<u32> {
        let b0 = self.read_u8(addr)?;
        let b1 = self.read_u8(addr.wrapping_add(1))?;
        let b2 = self.read_u8(addr.wrapping_add(2))?;
        let b3 = self.read_u8(addr.wrapping_add(3))?;
        Some(u32::from_be_bytes([b0, b1, b2, b3]))
    }

    /// Fetch one big-endian instruction word. Memory implementations with
    /// immutable executable regions may override this separately from data
    /// reads so repeated instruction fetches can be cached safely.
    #[inline]
    fn read_instruction_u32_be(&mut self, addr: u32) -> Option<u32> {
        self.read_u32_be(addr)
    }

    /// Write a big-endian 32-bit value. Default impl falls through
    /// to four byte writes.
    fn write_u32_be(&mut self, addr: u32, value: u32) -> Option<()> {
        let bytes = value.to_be_bytes();
        self.write_u8(addr, bytes[0])?;
        self.write_u8(addr.wrapping_add(1), bytes[1])?;
        self.write_u8(addr.wrapping_add(2), bytes[2])?;
        self.write_u8(addr.wrapping_add(3), bytes[3])?;
        Some(())
    }
}

/// A degenerate `PpcMemory` that fails every access. Used by
/// `step_instruction` for unit tests of non-memory instructions —
/// any spurious memory access surfaces immediately as a
/// `MemoryFault` rather than silently reading zeros.
///
/// `pub(super)` so the dispatcher in `mod.rs` can construct one
/// to back the no-memory `step_instruction` convenience.
pub(super) struct NullMemory;

impl PpcMemory for NullMemory {
    fn read_u8(&mut self, _addr: u32) -> Option<u8> {
        None
    }
    fn write_u8(&mut self, _addr: u32, _value: u8) -> Option<()> {
        None
    }
}

/// Multi-region memory bus for hosting an instantiated PEF
/// container. Each region maps a contiguous range of guest
/// addresses to a backing byte buffer. Accesses outside any
/// mapped region return `None`, surfacing as `MemoryFault` /
/// `FetchFault` from the run loop.
///
/// Designed for the loader-to-interpreter handoff: the host
/// instantiates each PEF section at the address the host chose
/// (typically the section's `default_address`, or a host-picked
/// base when relocations remap the fragment), and the bus
/// dispatches reads/writes to the appropriate region.
///
/// Regions are read-write by default. Read-only sections (PEF
/// code sections, `section_kind == 0`) may be flagged as such by
/// the host via [`Self::add_readonly_region`]; writes to read-only
/// regions return `None` and surface as a `MemoryFault` rather
/// than silently mutating the shared backing store.
#[derive(Debug, Clone)]
pub struct PpcSectionMem {
    regions: Vec<PpcMemRegion>,
    page_cache: [Option<(u32, usize)>; PPC_SECTION_MEM_PAGE_CACHE_ENTRIES],
    overlap_span_cache: [Option<(u32, u32, usize)>; PPC_SECTION_MEM_OVERLAP_SPAN_CACHE_ENTRIES],
    region_cache: [Option<usize>; PPC_SECTION_MEM_REGION_CACHE_ENTRIES],
    instruction_cache: Box<[Option<(u32, u32)>]>,
    has_overlapping_regions: bool,
}

/// Cached span inside a single mapped [`PpcSectionMem`] region.
///
/// The span is valid until regions are added to the memory bus. It lets hot
/// host-side code repeatedly access a known mapped range without paying a
/// region lookup per scalar read/write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcSectionMemSpan {
    index: usize,
    offset: usize,
    len: usize,
}

const PPC_SECTION_MEM_PAGE_CACHE_ENTRIES: usize = 256;
const PPC_SECTION_MEM_OVERLAP_SPAN_CACHE_ENTRIES: usize = 256;
const PPC_SECTION_MEM_REGION_CACHE_ENTRIES: usize = 4;
const PPC_SECTION_MEM_INSTRUCTION_CACHE_ENTRIES: usize = 4_096;
const PPC_SECTION_MEM_PAGE_SHIFT: u32 = 12;
const PPC_SECTION_MEM_PAGE_CACHE_INDEX_MASK: usize = PPC_SECTION_MEM_PAGE_CACHE_ENTRIES - 1;
const PPC_SECTION_MEM_OVERLAP_SPAN_CACHE_INDEX_MASK: usize =
    PPC_SECTION_MEM_OVERLAP_SPAN_CACHE_ENTRIES - 1;
const PPC_SECTION_MEM_INSTRUCTION_CACHE_INDEX_MASK: usize =
    PPC_SECTION_MEM_INSTRUCTION_CACHE_ENTRIES - 1;

#[derive(Debug, Clone)]
struct PpcMemRegion {
    base: u32,
    bytes: Vec<u8>,
    writable: bool,
}

impl Default for PpcSectionMem {
    fn default() -> Self {
        Self {
            regions: Vec::new(),
            page_cache: [None; PPC_SECTION_MEM_PAGE_CACHE_ENTRIES],
            overlap_span_cache: [None; PPC_SECTION_MEM_OVERLAP_SPAN_CACHE_ENTRIES],
            region_cache: [None; PPC_SECTION_MEM_REGION_CACHE_ENTRIES],
            instruction_cache: vec![None; PPC_SECTION_MEM_INSTRUCTION_CACHE_ENTRIES]
                .into_boxed_slice(),
            has_overlapping_regions: false,
        }
    }
}

impl PpcSectionMem {
    /// Construct an empty memory bus. The host fills it via
    /// [`Self::add_region`] / [`Self::add_readonly_region`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Map `bytes` into the guest at `[base, base + bytes.len())`
    /// as read-write storage. The host must guarantee the new
    /// range does not overlap any existing region unless it
    /// intentionally wants to overlay an earlier writable mapping.
    /// When ranges overlap, the most recently added region wins.
    pub fn add_region(&mut self, base: u32, bytes: Vec<u8>) {
        self.has_overlapping_regions |= self.overlaps_existing_region(base, bytes.len());
        self.regions.push(PpcMemRegion {
            base,
            bytes,
            writable: true,
        });
        self.clear_region_cache();
    }

    /// Map `bytes` into the guest at `[base, base + bytes.len())`
    /// as read-only storage. Stores hitting this range surface
    /// as a `MemoryFault`.
    pub fn add_readonly_region(&mut self, base: u32, bytes: Vec<u8>) {
        self.has_overlapping_regions |= self.overlaps_existing_region(base, bytes.len());
        self.regions.push(PpcMemRegion {
            base,
            bytes,
            writable: false,
        });
        self.clear_region_cache();
    }

    /// Number of regions currently mapped.
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    fn overlaps_existing_region(&self, base: u32, len: usize) -> bool {
        let Ok(len) = u32::try_from(len) else {
            return true;
        };
        let Some(end) = base.checked_add(len) else {
            return true;
        };
        self.regions.iter().any(|region| {
            let Ok(region_len) = u32::try_from(region.bytes.len()) else {
                return true;
            };
            let Some(region_end) = region.base.checked_add(region_len) else {
                return true;
            };
            base < region_end && region.base < end
        })
    }

    /// Copy a mapped byte range into `dst`.
    ///
    /// Returns `None` when any byte in the requested range is unmapped.
    /// When the full range falls inside one region this uses a single
    /// slice copy; otherwise it falls back to byte reads so callers can
    /// still copy across adjacent mapped regions.
    pub fn read_bytes_into(&mut self, addr: u32, dst: &mut [u8]) -> Option<()> {
        if dst.is_empty() {
            return Some(());
        }
        if !self.has_overlapping_regions {
            if let Some((i, off)) = self.locate_cached(addr) {
                let region = &self.regions[i];
                if off
                    .checked_add(dst.len())
                    .is_some_and(|end| end <= region.bytes.len())
                {
                    dst.copy_from_slice(&region.bytes[off..off + dst.len()]);
                    return Some(());
                }
            }
        }

        for (offset, byte) in dst.iter_mut().enumerate() {
            *byte = self.read_u8(addr.wrapping_add(offset as u32))?;
        }
        Some(())
    }

    /// Copy `src` into a mapped writable byte range.
    ///
    /// Returns `None` when any byte in the requested range is unmapped
    /// or read-only. No bytes are written unless the whole range can be
    /// written.
    pub fn write_bytes(&mut self, addr: u32, src: &[u8]) -> Option<()> {
        if src.is_empty() {
            return Some(());
        }
        if !self.has_overlapping_regions {
            if let Some((i, off)) = self.locate_cached(addr) {
                let region = &mut self.regions[i];
                if !region.writable {
                    return None;
                }
                if off
                    .checked_add(src.len())
                    .is_some_and(|end| end <= region.bytes.len())
                {
                    region.bytes[off..off + src.len()].copy_from_slice(src);
                    return Some(());
                }
            }
        }

        let mut positions = Vec::with_capacity(src.len());
        for offset in 0..src.len() {
            let addr = addr.wrapping_add(offset as u32);
            let (i, off) = self.locate_cached(addr)?;
            if !self.regions[i].writable {
                return None;
            }
            positions.push((i, off));
        }
        for ((i, off), byte) in positions.into_iter().zip(src.iter().copied()) {
            self.regions[i].bytes[off] = byte;
        }
        Some(())
    }

    /// Return a cached writable span for a range contained in one mapped region.
    pub fn writable_span(&mut self, addr: u32, len: usize) -> Option<PpcSectionMemSpan> {
        let (index, offset) = self.locate_writable_same_region(addr, len)?;
        Some(PpcSectionMemSpan { index, offset, len })
    }

    /// Read a big-endian u16 at `relative_offset` inside `span`.
    #[inline]
    pub fn read_u16_be_in_span(
        &self,
        span: PpcSectionMemSpan,
        relative_offset: usize,
    ) -> Option<u16> {
        let region = self.regions.get(span.index)?;
        if relative_offset.checked_add(2)? > span.len {
            return None;
        }
        let offset = span.offset.checked_add(relative_offset)?;
        if offset.checked_add(2)? > region.bytes.len() {
            return None;
        }
        Some(u16::from_be_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
        ]))
    }

    /// Write a big-endian u16 at `relative_offset` inside a writable `span`.
    #[inline]
    pub fn write_u16_be_in_span(
        &mut self,
        span: PpcSectionMemSpan,
        relative_offset: usize,
        value: u16,
    ) -> Option<()> {
        let region = self.regions.get_mut(span.index)?;
        if !region.writable || relative_offset.checked_add(2)? > span.len {
            return None;
        }
        let offset = span.offset.checked_add(relative_offset)?;
        if offset.checked_add(2)? > region.bytes.len() {
            return None;
        }
        let bytes = value.to_be_bytes();
        region.bytes[offset] = bytes[0];
        region.bytes[offset + 1] = bytes[1];
        Some(())
    }

    #[inline]
    fn read_same_region_u16(&mut self, addr: u32) -> Option<u16> {
        if self.has_overlapping_regions {
            return None;
        }
        let (i, off) = self.locate_cached(addr)?;
        let bytes = &self.regions[i].bytes;
        if bytes.len().saturating_sub(off) < 2 {
            return None;
        }
        // SAFETY: the length check above proves `off..off + 2` is in-bounds.
        let ptr = unsafe { bytes.as_ptr().add(off) };
        Some(u16::from_be_bytes(unsafe { [*ptr, *ptr.add(1)] }))
    }

    #[inline]
    fn read_same_region_u32(&mut self, addr: u32) -> Option<u32> {
        if self.has_overlapping_regions {
            return None;
        }
        let (i, off) = self.locate_cached(addr)?;
        let bytes = &self.regions[i].bytes;
        if bytes.len().saturating_sub(off) < 4 {
            return None;
        }
        // SAFETY: the length check above proves `off..off + 4` is in-bounds.
        let ptr = unsafe { bytes.as_ptr().add(off) };
        let b0 = unsafe { u32::from(*ptr) };
        let b1 = unsafe { u32::from(*ptr.add(1)) };
        let b2 = unsafe { u32::from(*ptr.add(2)) };
        let b3 = unsafe { u32::from(*ptr.add(3)) };
        Some((b0 << 24) | (b1 << 16) | (b2 << 8) | b3)
    }

    #[inline]
    fn read_same_region_u64(&mut self, addr: u32) -> Option<u64> {
        if self.has_overlapping_regions {
            return None;
        }
        let (i, off) = self.locate_cached(addr)?;
        let bytes = &self.regions[i].bytes;
        if bytes.len().saturating_sub(off) < 8 {
            return None;
        }
        // SAFETY: the length check above proves `off..off + 8` is in-bounds.
        let ptr = unsafe { bytes.as_ptr().add(off) };
        Some(u64::from_be_bytes(unsafe {
            [
                *ptr,
                *ptr.add(1),
                *ptr.add(2),
                *ptr.add(3),
                *ptr.add(4),
                *ptr.add(5),
                *ptr.add(6),
                *ptr.add(7),
            ]
        }))
    }

    #[inline]
    fn locate_writable_same_region(&mut self, addr: u32, len: usize) -> Option<(usize, usize)> {
        if self.has_overlapping_regions {
            return None;
        }
        let (i, off) = self.locate_cached(addr)?;
        if !self.regions[i].writable {
            return None;
        }
        if self.regions[i].bytes.len().saturating_sub(off) < len {
            return None;
        }
        Some((i, off))
    }

    #[inline]
    fn write_same_region_u16(&mut self, addr: u32, value: u16) -> Option<()> {
        let (i, off) = self.locate_writable_same_region(addr, 2)?;
        let bytes = value.to_be_bytes();
        // SAFETY: `locate_writable_same_region` proved `off..off + 2` is in-bounds.
        let dst = unsafe { self.regions[i].bytes.as_mut_ptr().add(off) };
        unsafe {
            *dst = bytes[0];
            *dst.add(1) = bytes[1];
        }
        Some(())
    }

    #[inline]
    fn write_same_region_u32(&mut self, addr: u32, value: u32) -> Option<()> {
        if self.has_overlapping_regions {
            return None;
        }
        let (i, off) = self.locate_cached(addr)?;
        let region = &mut self.regions[i];
        if !region.writable {
            return None;
        }
        if region.bytes.len().saturating_sub(off) < 4 {
            return None;
        }
        let bytes = value.to_be_bytes();
        // SAFETY: the length check above proves `off..off + 4` is in-bounds.
        let dst = unsafe { region.bytes.as_mut_ptr().add(off) };
        unsafe {
            *dst = bytes[0];
            *dst.add(1) = bytes[1];
            *dst.add(2) = bytes[2];
            *dst.add(3) = bytes[3];
        }
        Some(())
    }

    #[inline]
    fn write_same_region_u64(&mut self, addr: u32, value: u64) -> Option<()> {
        let (i, off) = self.locate_writable_same_region(addr, 8)?;
        let bytes = value.to_be_bytes();
        // SAFETY: `locate_writable_same_region` proved `off..off + 8` is in-bounds.
        let dst = unsafe { self.regions[i].bytes.as_mut_ptr().add(off) };
        unsafe {
            *dst = bytes[0];
            *dst.add(1) = bytes[1];
            *dst.add(2) = bytes[2];
            *dst.add(3) = bytes[3];
            *dst.add(4) = bytes[4];
            *dst.add(5) = bytes[5];
            *dst.add(6) = bytes[6];
            *dst.add(7) = bytes[7];
        }
        Some(())
    }

    /// Find the region covering `addr` and the byte offset within
    /// it, if any. Internal helper for the read/write impls.
    ///
    /// `checked_sub` is mapped to `continue` (skip this region)
    /// rather than propagated out — `addr < region.base` simply
    /// means "this region is above the requested address; try
    /// the next one." Using `?` here would exit the whole
    /// search on the first higher-based region, masking valid
    /// matches further down the list.
    #[inline]
    fn locate(&self, addr: u32) -> Option<(usize, usize)> {
        let mut i = self.regions.len();
        while i > 0 {
            i -= 1;
            let region = &self.regions[i];
            let Some(off) = addr.checked_sub(region.base) else {
                continue;
            };
            if (off as usize) < region.bytes.len() {
                return Some((i, off as usize));
            }
        }
        None
    }

    #[inline]
    fn clear_region_cache(&mut self) {
        self.page_cache = [None; PPC_SECTION_MEM_PAGE_CACHE_ENTRIES];
        self.overlap_span_cache = [None; PPC_SECTION_MEM_OVERLAP_SPAN_CACHE_ENTRIES];
        self.region_cache = [None; PPC_SECTION_MEM_REGION_CACHE_ENTRIES];
        self.instruction_cache.fill(None);
    }

    #[inline]
    fn locate_cached(&mut self, addr: u32) -> Option<(usize, usize)> {
        if self.has_overlapping_regions {
            let page_key = addr >> PPC_SECTION_MEM_PAGE_SHIFT;
            let slot = (page_key as usize) & PPC_SECTION_MEM_OVERLAP_SPAN_CACHE_INDEX_MASK;
            if let Some((start, end, index)) = self.overlap_span_cache[slot] {
                if addr >= start && addr < end && index < self.regions.len() {
                    return Some((index, (addr - self.regions[index].base) as usize));
                }
            }
            let located = self.locate(addr);
            if let Some((index, _)) = located {
                self.overlap_span_cache[slot] = self.visible_region_span(index, addr);
            }
            return located;
        }
        let page_key = addr >> PPC_SECTION_MEM_PAGE_SHIFT;
        let page_slot = (page_key as usize) & PPC_SECTION_MEM_PAGE_CACHE_INDEX_MASK;
        if let Some((cached_page_key, index)) = self.page_cache[page_slot] {
            if cached_page_key == page_key && index < self.regions.len() {
                let region = &self.regions[index];
                if addr >= region.base {
                    let off = (addr - region.base) as usize;
                    if off < region.bytes.len() {
                        return Some((index, off));
                    }
                }
            }
        }

        let mut slot = 0;
        while slot < PPC_SECTION_MEM_REGION_CACHE_ENTRIES {
            if let Some(index) = self.region_cache[slot] {
                if index < self.regions.len() {
                    let region = &self.regions[index];
                    if addr >= region.base {
                        let off = (addr - region.base) as usize;
                        if off < region.bytes.len() {
                            self.page_cache[page_slot] = Some((page_key, index));
                            if slot > 0 {
                                let value = self.region_cache[slot];
                                let mut i = slot;
                                while i > 0 {
                                    self.region_cache[i] = self.region_cache[i - 1];
                                    i -= 1;
                                }
                                self.region_cache[0] = value;
                            }
                            return Some((index, off));
                        }
                    }
                }
            }
            slot += 1;
        }
        let located = self.locate(addr);
        if let Some((index, _)) = located {
            self.page_cache[page_slot] = Some((page_key, index));
            let mut slot = PPC_SECTION_MEM_REGION_CACHE_ENTRIES - 1;
            while slot > 0 {
                self.region_cache[slot] = self.region_cache[slot - 1];
                slot -= 1;
            }
            self.region_cache[0] = Some(index);
        }
        located
    }

    fn visible_region_span(&self, index: usize, addr: u32) -> Option<(u32, u32, usize)> {
        let region = self.regions.get(index)?;
        let region_len = u32::try_from(region.bytes.len()).ok()?;
        let mut start = region.base;
        let mut end = region.base.checked_add(region_len)?;
        for newer in self.regions.get(index + 1..)? {
            let Ok(newer_len) = u32::try_from(newer.bytes.len()) else {
                return None;
            };
            let newer_end = newer.base.checked_add(newer_len)?;
            if newer_end <= addr {
                start = start.max(newer_end);
            } else if newer.base > addr {
                end = end.min(newer.base);
            } else {
                return None;
            }
        }
        (start <= addr && addr < end).then_some((start, end, index))
    }
}

impl PpcMemory for PpcSectionMem {
    fn read_u8(&mut self, addr: u32) -> Option<u8> {
        let (i, off) = self.locate_cached(addr)?;
        Some(self.regions[i].bytes[off])
    }

    fn read_u16_be(&mut self, addr: u32) -> Option<u16> {
        if let Some(value) = self.read_same_region_u16(addr) {
            return Some(value);
        }
        let b0 = self.read_u8(addr)?;
        let b1 = self.read_u8(addr.wrapping_add(1))?;
        Some(u16::from_be_bytes([b0, b1]))
    }

    fn read_u32_be(&mut self, addr: u32) -> Option<u32> {
        if let Some(value) = self.read_same_region_u32(addr) {
            return Some(value);
        }
        let b0 = self.read_u8(addr)?;
        let b1 = self.read_u8(addr.wrapping_add(1))?;
        let b2 = self.read_u8(addr.wrapping_add(2))?;
        let b3 = self.read_u8(addr.wrapping_add(3))?;
        Some(u32::from_be_bytes([b0, b1, b2, b3]))
    }

    #[inline]
    fn read_instruction_u32_be(&mut self, addr: u32) -> Option<u32> {
        let slot = ((addr >> 2) as usize) & PPC_SECTION_MEM_INSTRUCTION_CACHE_INDEX_MASK;
        if let Some((cached_addr, word)) = self.instruction_cache[slot] {
            if cached_addr == addr {
                return Some(word);
            }
        }

        let (region_index, offset) = self.locate_cached(addr)?;
        let region = &self.regions[region_index];
        let crosses_visible_region = self.has_overlapping_regions
            && match addr.checked_add(3) {
                Some(instruction_last) => !matches!(
                    self.visible_region_span(region_index, addr),
                    Some((_, end, _)) if end > instruction_last
                ),
                None => true,
            };
        if region.writable
            || region.bytes.len().saturating_sub(offset) < 4
            || crosses_visible_region
        {
            return self.read_u32_be(addr);
        }
        let word = u32::from_be_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
            region.bytes[offset + 2],
            region.bytes[offset + 3],
        ]);
        self.instruction_cache[slot] = Some((addr, word));
        Some(word)
    }

    fn read_u64_be(&mut self, addr: u32) -> Option<u64> {
        if let Some(value) = self.read_same_region_u64(addr) {
            return Some(value);
        }
        let hi = self.read_u32_be(addr)?;
        let lo = self.read_u32_be(addr.wrapping_add(4))?;
        Some((u64::from(hi) << 32) | u64::from(lo))
    }

    fn write_u8(&mut self, addr: u32, value: u8) -> Option<()> {
        let (i, off) = self.locate_cached(addr)?;
        if !self.regions[i].writable {
            return None;
        }
        self.regions[i].bytes[off] = value;
        Some(())
    }

    fn write_u16_be(&mut self, addr: u32, value: u16) -> Option<()> {
        if self.write_same_region_u16(addr, value).is_some() {
            return Some(());
        }
        let bytes = value.to_be_bytes();
        self.write_u8(addr, bytes[0])?;
        self.write_u8(addr.wrapping_add(1), bytes[1])?;
        Some(())
    }

    fn write_u32_be(&mut self, addr: u32, value: u32) -> Option<()> {
        if self.write_same_region_u32(addr, value).is_some() {
            return Some(());
        }
        let bytes = value.to_be_bytes();
        self.write_u8(addr, bytes[0])?;
        self.write_u8(addr.wrapping_add(1), bytes[1])?;
        self.write_u8(addr.wrapping_add(2), bytes[2])?;
        self.write_u8(addr.wrapping_add(3), bytes[3])?;
        Some(())
    }

    fn write_u64_be(&mut self, addr: u32, value: u64) -> Option<()> {
        if self.write_same_region_u64(addr, value).is_some() {
            return Some(());
        }
        self.write_u32_be(addr, (value >> 32) as u32)?;
        self.write_u32_be(addr.wrapping_add(4), value as u32)?;
        Some(())
    }
}
