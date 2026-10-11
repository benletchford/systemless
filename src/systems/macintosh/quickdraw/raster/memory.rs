//! The memory the shared raster kernels draw into.
//!
//! The kernels prepare whole rows and hand each covered run to
//! [`RasterMemory::write_row`], so every memory type keeps its own write
//! routing, presentation notes and failure policy.

use crate::memory::GuestAddressSpace;

pub(crate) trait RasterMemory {
    /// Stores one prepared run of destination bytes at `addr`.
    fn write_row(&mut self, addr: u32, bytes: &[u8]) -> Option<()>;
}

/// PowerPC: one `write_bytes` per run, so one presentation note per run, as
/// the PowerPC rect fill has always made.
impl RasterMemory for GuestAddressSpace {
    #[inline(always)]
    fn write_row(&mut self, addr: u32, bytes: &[u8]) -> Option<()> {
        self.write_bytes(addr, bytes)
    }
}
