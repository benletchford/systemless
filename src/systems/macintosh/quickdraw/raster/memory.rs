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

    /// [`RasterMemory::modify_row`] for a run of 8-bpp pixels.
    #[inline(always)]
    fn modify_pixel_row(
        &mut self,
        addr: u32,
        buf: &mut [u8],
        f: impl FnOnce(&mut [u8]),
    ) -> Option<()> {
        self.modify_row(addr, buf, f)
    }

    /// Reads the byte at `addr`, and stores `f` of it back.
    #[inline(always)]
    fn modify_byte(&mut self, addr: u32, f: impl FnOnce(u8) -> u8) -> Option<()> {
        let mut byte = [0u8; 1];
        self.modify_row(addr, &mut byte, |byte| byte[0] = f(byte[0]))
    }
}

/// 68K: one `write_bytes` per run, keeping `MacMemoryBus`'s Flat-span
/// presentation sync, write-probe routing and read-only code check.
impl RasterMemory for MacMemoryBus {
    #[inline(always)]
    fn write_row(&mut self, addr: u32, bytes: &[u8]) -> Option<()> {
        self.write_bytes(addr, bytes);
        Some(())
    }

    /// Reads with `read_bytes_into`, which an armed access watch records, as
    /// the 68K 1-bpp invert's middle bytes always have.
    #[inline(always)]
    fn modify_row(&mut self, addr: u32, buf: &mut [u8], f: impl FnOnce(&mut [u8])) -> Option<()> {
        self.read_bytes_into(addr, buf);
        f(buf);
        self.write_bytes(addr, buf);
        Some(())
    }

    /// Reads with `read_bytes`, which an armed access watch does not record,
    /// as the 68K 8-bpp boolean-mode row loop always has.
    #[inline(always)]
    fn modify_pixel_row(
        &mut self,
        addr: u32,
        buf: &mut [u8],
        f: impl FnOnce(&mut [u8]),
    ) -> Option<()> {
        buf.copy_from_slice(&self.read_bytes(addr, buf.len()));
        f(buf);
        self.write_bytes(addr, buf);
        Some(())
    }

    /// `read_byte` and `write_byte`, as the 68K 1-bpp edge bytes always have.
    #[inline(always)]
    fn modify_byte(&mut self, addr: u32, f: impl FnOnce(u8) -> u8) -> Option<()> {
        let old = self.read_byte(addr);
        self.write_byte(addr, f(old));
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
