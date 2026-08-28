//! Architecture-neutral mapped guest address space.
//!
//! CPU backends have different bus contracts, but guest bytes must have one
//! owner. This type provides that owner while preserving the sparse mappings,
//! read-only regions, and instruction-cache behavior required by native PEF
//! applications.

use m68k::core::memory::{BusFault, BusFaultKind};
use m68k::AddressBus;
use ppc::{PpcMemory, PpcSectionMem, PpcSectionMemSpan};

/// A sparse guest address space that can be executed by either CPU backend.
///
/// The region implementation remains private so loaders and runtime services
/// depend on the architecture-neutral ownership boundary rather than a CPU
/// crate's concrete memory type.
#[derive(Debug, Clone, Default)]
pub struct GuestAddressSpace {
    regions: PpcSectionMem,
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

    /// Return the number of mapped regions.
    pub fn region_count(&self) -> usize {
        self.regions.region_count()
    }

    /// Copy a fully mapped range into `dst`.
    pub fn read_bytes_into(&mut self, addr: u32, dst: &mut [u8]) -> Option<()> {
        self.regions.read_bytes_into(addr, dst)
    }

    /// Copy `src` into a fully mapped, writable range.
    pub fn write_bytes(&mut self, addr: u32, src: &[u8]) -> Option<()> {
        self.regions.write_bytes(addr, src)
    }

    /// Return a cached writable span contained in one mapped region.
    pub fn writable_span(&mut self, addr: u32, len: usize) -> Option<PpcSectionMemSpan> {
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
}

impl PpcMemory for GuestAddressSpace {
    #[inline]
    fn read_u8(&mut self, addr: u32) -> Option<u8> {
        self.regions.read_u8(addr)
    }

    #[inline]
    fn read_u16_be(&mut self, addr: u32) -> Option<u16> {
        self.regions.read_u16_be(addr)
    }

    #[inline]
    fn read_u32_be(&mut self, addr: u32) -> Option<u32> {
        self.regions.read_u32_be(addr)
    }

    #[inline]
    fn read_u64_be(&mut self, addr: u32) -> Option<u64> {
        self.regions.read_u64_be(addr)
    }

    #[inline]
    fn read_instruction_u32_be(&mut self, addr: u32) -> Option<u32> {
        self.regions.read_instruction_u32_be(addr)
    }

    #[inline]
    fn instruction_cache_token(&mut self, addr: u32) -> Option<u64> {
        self.regions.instruction_cache_token(addr)
    }

    #[inline]
    fn write_u8(&mut self, addr: u32, value: u8) -> Option<()> {
        self.regions.write_u8(addr, value)
    }

    #[inline]
    fn write_u16_be(&mut self, addr: u32, value: u16) -> Option<()> {
        self.regions.write_u16_be(addr, value)
    }

    #[inline]
    fn write_u32_be(&mut self, addr: u32, value: u32) -> Option<()> {
        self.regions.write_u32_be(addr, value)
    }

    #[inline]
    fn write_u64_be(&mut self, addr: u32, value: u64) -> Option<()> {
        self.regions.write_u64_be(addr, value)
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
}
