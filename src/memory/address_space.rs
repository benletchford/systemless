//! Architecture-neutral mapped guest address space.
//!
//! CPU backends have different bus contracts, but guest bytes must have one
//! owner. This type provides that owner while preserving the sparse mappings,
//! read-only regions, and instruction-cache behavior required by native PEF
//! applications.

use m68k::core::memory::{BusFault, BusFaultKind};
use m68k::AddressBus;
use ppc::{PpcMemory, PpcSectionMem, PpcSectionMemSpan};

use super::bus::SharedRamRegion;

#[derive(Debug, Clone)]
struct SharedRegionMapping {
    base: u32,
    region: SharedRamRegion,
}

/// A sparse guest address space that can be executed by either CPU backend.
///
/// The region implementation remains private so loaders and runtime services
/// depend on the architecture-neutral ownership boundary rather than a CPU
/// crate's concrete memory type.
#[derive(Debug, Default)]
pub struct GuestAddressSpace {
    regions: PpcSectionMem,
    shared_regions: Vec<SharedRegionMapping>,
}

impl Clone for GuestAddressSpace {
    fn clone(&self) -> Self {
        Self {
            regions: self.regions.clone(),
            shared_regions: self
                .shared_regions
                .iter()
                .map(|mapping| SharedRegionMapping {
                    base: mapping.base,
                    region: mapping.region.detached_clone(),
                })
                .collect(),
        }
    }
}

impl GuestAddressSpace {
    /// Construct an empty address space.
    pub fn new() -> Self {
        Self::default()
    }

    /// Map a writable region. Newer mappings take precedence over overlaps.
    pub fn add_region(&mut self, base: u32, bytes: Vec<u8>) {
        self.regions.add_region(base, bytes);
    }

    /// Map a read-only region. Newer mappings take precedence over overlaps.
    pub fn add_readonly_region(&mut self, base: u32, bytes: Vec<u8>) {
        self.regions.add_readonly_region(base, bytes);
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
        self.shared_regions
            .push(SharedRegionMapping { base, region });
    }

    /// Return the number of mapped regions.
    pub fn region_count(&self) -> usize {
        self.regions.region_count() + self.shared_regions.len()
    }

    /// Copy a fully mapped range into `dst`.
    pub fn read_bytes_into(&mut self, addr: u32, dst: &mut [u8]) -> Option<()> {
        if !self.shared_overlaps(addr, dst.len()) {
            return self.regions.read_bytes_into(addr, dst);
        }
        for (offset, byte) in dst.iter_mut().enumerate() {
            *byte = self.read_u8(addr.wrapping_add(offset as u32))?;
        }
        Some(())
    }

    /// Copy `src` into a fully mapped, writable range.
    pub fn write_bytes(&mut self, addr: u32, src: &[u8]) -> Option<()> {
        if !self.shared_overlaps(addr, src.len()) {
            return self.regions.write_bytes(addr, src);
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
                ordinary.push((address, self.regions.read_u8(address)?, byte));
            }
        }

        for (committed, &(address, _, byte)) in ordinary.iter().enumerate() {
            if self.regions.write_u8(address, byte).is_none() {
                for &(rollback_address, original, _) in ordinary[..committed].iter().rev() {
                    self.regions
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

    /// Return a cached writable span contained in one mapped region.
    pub fn writable_span(&mut self, addr: u32, len: usize) -> Option<PpcSectionMemSpan> {
        if self.shared_overlaps(addr, len) {
            return None;
        }
        self.regions.writable_span(addr, len)
    }

    /// Read a big-endian word at an offset within a cached span.
    pub fn read_u16_be_in_span(
        &self,
        span: PpcSectionMemSpan,
        relative_offset: usize,
    ) -> Option<u16> {
        self.regions.read_u16_be_in_span(span, relative_offset)
    }

    /// Write a big-endian word at an offset within a cached writable span.
    pub fn write_u16_be_in_span(
        &mut self,
        span: PpcSectionMemSpan,
        relative_offset: usize,
        value: u16,
    ) -> Option<()> {
        self.regions
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
    fn locate_shared(&self, addr: u32) -> Option<(&SharedRamRegion, usize)> {
        self.shared_regions.iter().rev().find_map(|mapping| {
            let offset = usize::try_from(addr.checked_sub(mapping.base)?).ok()?;
            (offset < mapping.region.len()).then_some((&mapping.region, offset))
        })
    }

    fn shared_overlaps(&self, addr: u32, len: usize) -> bool {
        if len == 0 {
            return false;
        }
        const ADDRESS_SPACE_SIZE: u64 = 1u64 << 32;
        let start = u64::from(addr);
        let len = len as u64;
        if len >= ADDRESS_SPACE_SIZE {
            return !self.shared_regions.is_empty();
        }
        let end = start + len;
        self.shared_regions.iter().any(|mapping| {
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
            self.regions.read_u8(addr)
        }
    }

    #[inline]
    fn read_u16_be(&mut self, addr: u32) -> Option<u16> {
        if !self.shared_overlaps(addr, 2) {
            return self.regions.read_u16_be(addr);
        }
        let mut bytes = [0; 2];
        self.read_bytes_into(addr, &mut bytes)?;
        Some(u16::from_be_bytes(bytes))
    }

    #[inline]
    fn read_u32_be(&mut self, addr: u32) -> Option<u32> {
        if !self.shared_overlaps(addr, 4) {
            return self.regions.read_u32_be(addr);
        }
        let mut bytes = [0; 4];
        self.read_bytes_into(addr, &mut bytes)?;
        Some(u32::from_be_bytes(bytes))
    }

    #[inline]
    fn read_u64_be(&mut self, addr: u32) -> Option<u64> {
        if !self.shared_overlaps(addr, 8) {
            return self.regions.read_u64_be(addr);
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
            self.regions.read_instruction_u32_be(addr)
        }
    }

    #[inline]
    fn instruction_cache_token(&mut self, addr: u32) -> Option<u64> {
        if self.shared_overlaps(addr, 4) {
            None
        } else {
            self.regions.instruction_cache_token(addr)
        }
    }

    #[inline]
    fn write_u8(&mut self, addr: u32, value: u8) -> Option<()> {
        if let Some((region, offset)) = self.locate_shared(addr) {
            // SAFETY: `add_shared_region` requires the enclosing runtime to
            // serialize both adapters for the mapping's complete lifetime.
            unsafe { region.write(offset, value) }
        } else {
            self.regions.write_u8(addr, value)
        }
    }

    #[inline]
    fn write_u16_be(&mut self, addr: u32, value: u16) -> Option<()> {
        if !self.shared_overlaps(addr, 2) {
            return self.regions.write_u16_be(addr, value);
        }
        self.write_bytes(addr, &value.to_be_bytes())
    }

    #[inline]
    fn write_u32_be(&mut self, addr: u32, value: u32) -> Option<()> {
        if !self.shared_overlaps(addr, 4) {
            return self.regions.write_u32_be(addr, value);
        }
        self.write_bytes(addr, &value.to_be_bytes())
    }

    #[inline]
    fn write_u64_be(&mut self, addr: u32, value: u64) -> Option<()> {
        if !self.shared_overlaps(addr, 8) {
            return self.regions.write_u64_be(addr, value);
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
}
