//! The memory the shared raster kernels draw into.
//!
//! The kernels prepare whole rows and hand each covered run to
//! [`RasterMemory::write_row`], so every memory type keeps its own write
//! routing, presentation notes and failure policy.

use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};

pub(crate) trait RasterMemory {
    /// Stores one prepared run of destination bytes at `addr`.
    fn write_row(&mut self, addr: u32, bytes: &[u8]) -> Option<()>;

    /// Reads `buf.len()` destination bytes at `addr` into `buf`, runs `f` on
    /// them, and stores the modified bytes back in one run.
    fn modify_row(&mut self, addr: u32, buf: &mut [u8], f: impl FnOnce(&mut [u8])) -> Option<()>;
}

/// 68K: one `write_bytes` per run, keeping `MacMemoryBus`'s Flat-span
/// presentation sync, write-probe routing and read-only code check.
impl RasterMemory for MacMemoryBus {
    #[inline(always)]
    fn write_row(&mut self, addr: u32, bytes: &[u8]) -> Option<()> {
        self.write_bytes(addr, bytes);
        Some(())
    }

    #[inline(always)]
    fn modify_row(&mut self, addr: u32, buf: &mut [u8], f: impl FnOnce(&mut [u8])) -> Option<()> {
        self.read_bytes_into(addr, buf);
        f(buf);
        self.write_bytes(addr, buf);
        Some(())
    }
}

/// PowerPC: one `write_bytes` per run, so one presentation note per run, as
/// the PowerPC rect fill has always made.
impl RasterMemory for GuestAddressSpace {
    #[inline(always)]
    fn write_row(&mut self, addr: u32, bytes: &[u8]) -> Option<()> {
        self.write_bytes(addr, bytes)
    }

    #[inline(always)]
    fn modify_row(&mut self, addr: u32, buf: &mut [u8], f: impl FnOnce(&mut [u8])) -> Option<()> {
        self.read_bytes_into(addr, buf)?;
        f(buf);
        self.write_bytes(addr, buf)
    }
}
