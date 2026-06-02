//! Memory bus implementation
//!
//! Provides Big-Endian memory access compatible with 68k Mac architecture.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;

use super::globals::LowMemGlobals;

// Release-mode tracer for writes to a guest address range. Use to
// localize the source of unexpected pixel writes in the framebuffer
// or to any other narrow guest memory range. Format:
//   `SYSTEMLESS_TRACE_FB_WRITE_RANGE=START_HEX:END_HEX`
// Both inclusive. Each write to an address in [start, end] logs the
// guest PC + address + value to stderr. Cheap when unset (one atomic
// load + branch on the hot path).
static FB_WRITE_TRACE_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();

#[inline]
fn fb_write_trace_range() -> Option<(u32, u32)> {
    *FB_WRITE_TRACE_RANGE.get_or_init(|| {
        std::env::var("SYSTEMLESS_TRACE_FB_WRITE_RANGE")
            .ok()
            .and_then(|s| {
                let mut parts = s.split(':');
                let start_str = parts.next()?.trim_start_matches("0x");
                let end_str = parts.next()?.trim_start_matches("0x");
                let start = u32::from_str_radix(start_str, 16).ok()?;
                let end = u32::from_str_radix(end_str, 16).ok()?;
                Some((start, end))
            })
    })
}

/// `true` when SYSTEMLESS_TRACE_FB_WRITE_RANGE is set; the runner uses this
/// to decide whether to mirror guest PC into [`CURRENT_PC`] in release.
#[inline]
pub fn fb_write_trace_active() -> bool {
    fb_write_trace_range().is_some()
}

#[inline]
fn maybe_log_fb_write(address: u32, value: u8) {
    if let Some((start, end)) = fb_write_trace_range() {
        if address >= start && address <= end {
            let pc = CURRENT_PC.with(|p| *p.borrow());
            // When PC=0 (host code, not guest M68K), include a Rust
            // backtrace so we can identify the host call site. Set
            // RUST_BACKTRACE=1 to enable; otherwise just the PC line.
            eprintln!(
                "[FB-WRITE] PC=${:08X} addr=${:08X}=${:02X}",
                pc, address, value
            );
            if pc == 0 && std::env::var_os("RUST_BACKTRACE").is_some() {
                let bt = std::backtrace::Backtrace::force_capture();
                eprintln!("[FB-WRITE-BT]\n{}", bt);
            }
        }
    }
}

/// Set `SYSTEMLESS_TRACE_FB_WRITE_DISASM=1` (or `=N` for N>1) alongside
/// `SYSTEMLESS_TRACE_FB_WRITE_RANGE` to dump the 8 instruction bytes
/// (PC..PC+8) and m68k mnemonic following each tracked write. The
/// numeric value extends the dump to cover N consecutive instructions
/// after the first — useful for spotting the surrounding blit-loop
/// branch back instead of just the one trapping/writing instruction.
/// Lets us identify the 68k blit loop responsible for a stuck-pixel
/// divergence without a full debug-build watchpoint.
static FB_WRITE_DISASM_COUNT: OnceLock<usize> = OnceLock::new();

#[inline]
fn fb_write_disasm_count() -> usize {
    *FB_WRITE_DISASM_COUNT.get_or_init(|| {
        std::env::var("SYSTEMLESS_TRACE_FB_WRITE_DISASM")
            .ok()
            .and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    return Some(1);
                }
                // Accept "1", "8", etc. — anything non-numeric falls
                // back to "set, count = 1" so existing
                // `SYSTEMLESS_TRACE_FB_WRITE_DISASM=1` invocations keep
                // working unchanged.
                trimmed.parse::<usize>().ok().or(Some(1))
            })
            .unwrap_or(0)
    })
}

#[inline]
fn fb_write_disasm_enabled() -> bool {
    fb_write_disasm_count() > 0
}

// Release-mode tracer for reads from a guest address range.
// Mirrors the FB write tracer. Format:
//   `SYSTEMLESS_TRACE_MEM_READ_RANGE=START_HEX:END_HEX`
// Both inclusive. Each byte read whose address falls in [start, end]
// logs the guest PC + address + value to stderr. Cheap when unset
// (one atomic load + None branch on the hot path).
static MEM_READ_TRACE_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();

#[inline]
fn mem_read_trace_range() -> Option<(u32, u32)> {
    *MEM_READ_TRACE_RANGE.get_or_init(|| {
        std::env::var("SYSTEMLESS_TRACE_MEM_READ_RANGE")
            .ok()
            .and_then(|s| {
                let mut parts = s.split(':');
                let start_str = parts.next()?.trim_start_matches("0x");
                let end_str = parts.next()?.trim_start_matches("0x");
                let start = u32::from_str_radix(start_str, 16).ok()?;
                let end = u32::from_str_radix(end_str, 16).ok()?;
                Some((start, end))
            })
    })
}

#[inline]
fn maybe_log_mem_read(address: u32, width: u8, value: u32) {
    if let Some((start, end)) = mem_read_trace_range() {
        if address >= start && address <= end {
            let pc = CURRENT_PC.with(|p| *p.borrow());
            eprintln!(
                "[MEM-READ] PC=${:08X} addr=${:08X} width={} value=${:0width$X}",
                pc,
                address,
                width,
                value,
                width = (width as usize) * 2
            );
        }
    }
}

// ============================================================================
// DEBUG WATCHPOINT INFRASTRUCTURE
// ============================================================================

/// Global step counter for debugging (incremented by runner)
pub static STEP_COUNTER: AtomicU32 = AtomicU32::new(0);
pub static WATCHPOINT_ARMED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Address to watch for writes (set from test harness)
    pub static WATCH_ADDRESS: RefCell<Option<u32>> = const { RefCell::new(None) };
    /// Current PC for debugging context (updated by runner before each step)
    pub static CURRENT_PC: RefCell<u32> = const { RefCell::new(0) };
    /// Current A0 for watchpoint diagnostics.
    pub static CURRENT_A0: RefCell<u32> = const { RefCell::new(0) };
    /// Current A1 for watchpoint diagnostics.
    pub static CURRENT_A1: RefCell<u32> = const { RefCell::new(0) };
    /// Current A6 for watchpoint diagnostics.
    pub static CURRENT_A6: RefCell<u32> = const { RefCell::new(0) };
    /// Current A7 for watchpoint diagnostics.
    pub static CURRENT_A7: RefCell<u32> = const { RefCell::new(0) };
}

/// Set the address to watch for writes
pub fn arm_watchpoint(addr: u32) {
    WATCH_ADDRESS.with(|wa| {
        *wa.borrow_mut() = Some(addr);
    });
    WATCHPOINT_ARMED.store(true, Ordering::Relaxed);
    eprintln!("[WATCHPOINT] Armed on address ${:08X}", addr);
}

/// Clear the watchpoint
pub fn disarm_watchpoint() {
    WATCH_ADDRESS.with(|wa| {
        *wa.borrow_mut() = None;
    });
    WATCHPOINT_ARMED.store(false, Ordering::Relaxed);
}

pub fn watchpoint_armed() -> bool {
    WATCHPOINT_ARMED.load(Ordering::Relaxed)
}

/// Update current PC for debug context
pub fn set_current_pc(pc: u32) {
    CURRENT_PC.with(|p| {
        *p.borrow_mut() = pc;
    });
}

pub fn set_watch_registers(a0: u32, a1: u32, a6: u32, a7: u32) {
    CURRENT_A0.with(|r| {
        *r.borrow_mut() = a0;
    });
    CURRENT_A1.with(|r| {
        *r.borrow_mut() = a1;
    });
    CURRENT_A6.with(|r| {
        *r.borrow_mut() = a6;
    });
    CURRENT_A7.with(|r| {
        *r.borrow_mut() = a7;
    });
}

/// Get current step count
pub fn get_step() -> u32 {
    STEP_COUNTER.load(Ordering::Relaxed)
}

/// Increment step counter
pub fn increment_step() {
    STEP_COUNTER.fetch_add(1, Ordering::Relaxed);
}

/// Memory bus trait for Big-Endian 68k memory access
pub trait MemoryBus {
    /// Read a byte from memory
    fn read_byte(&self, address: u32) -> u8;

    /// Read a 16-bit word from memory (Big-Endian)
    fn read_word(&self, address: u32) -> u16;

    /// Read a 32-bit long from memory (Big-Endian)
    fn read_long(&self, address: u32) -> u32;

    /// Write a byte to memory
    fn write_byte(&mut self, address: u32, value: u8);

    /// Write a 16-bit word to memory (Big-Endian)
    fn write_word(&mut self, address: u32, value: u16);

    /// Write a 32-bit long to memory (Big-Endian)
    fn write_long(&mut self, address: u32, value: u32);

    /// Get the total RAM size
    fn ram_size(&self) -> u32;

    /// Read a Pascal string (length-prefixed) from memory. Delegates
    /// to [`Self::read_bytes`] for the data so the underlying slice
    /// fast path on [`MacMemoryBus`] applies.
    fn read_pstring(&self, address: u32) -> Vec<u8> {
        let len = self.read_byte(address) as usize;
        self.read_bytes(address.wrapping_add(1), len)
    }

    /// Write a Pascal string (length-prefixed) to memory. Clamps to
    /// the Pascal-string max of 255 bytes (the length byte's range)
    /// and routes the data through [`Self::write_bytes`] so the
    /// slice fast path on [`MacMemoryBus`] applies.
    fn write_pstring(&mut self, address: u32, data: &[u8]) {
        let n = data.len().min(255);
        self.write_byte(address, n as u8);
        self.write_bytes(address.wrapping_add(1), &data[..n]);
    }

    /// Copy bytes from memory into a freshly allocated buffer.
    /// Default impl delegates to [`Self::read_bytes_into`] so any
    /// fast-path override (like `MacMemoryBus`'s slice copy) is
    /// shared between both helpers.
    fn read_bytes(&self, address: u32, len: usize) -> Vec<u8> {
        let mut result = vec![0u8; len];
        self.read_bytes_into(address, &mut result);
        result
    }

    /// Zero-alloc bulk read into a pre-allocated slice. Mirrors the
    /// `write_bytes` fast path: callers that pull many short rows can
    /// pre-allocate the output `Vec` once and read row-by-row without
    /// N intermediate `Vec` allocations. Default impl falls back to
    /// per-byte read; `MacMemoryBus` overrides with a `slice_at +
    /// copy_from_slice` fast path.
    fn read_bytes_into(&self, address: u32, dst: &mut [u8]) {
        for (i, byte) in dst.iter_mut().enumerate() {
            *byte = self.read_byte(address.wrapping_add(i as u32));
        }
    }

    /// Copy bytes from a buffer into memory
    fn write_bytes(&mut self, address: u32, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.write_byte(address.wrapping_add(i as u32), byte);
        }
    }

    /// Zero-fill a region of memory. Default impl is a byte-by-byte
    /// loop; the [`MacMemoryBus`] override uses a single slice fill on
    /// the underlying RAM. Used by Memory Manager `_NewPtrClear` /
    /// `_NewHandleClear` allocators to avoid an intermediate
    /// `vec![0u8; size]` allocation.
    fn fill_zeros(&mut self, address: u32, len: u32) {
        for i in 0..len {
            self.write_byte(address.wrapping_add(i), 0);
        }
    }
}

/// Mac memory bus with RAM, ROM, and low-memory globals
pub struct MacMemoryBus {
    ram: RamStorage,
    ram_size: u32,
    globals: LowMemGlobals,
    /// Heap allocation pointer (grows upward from 0x200000)
    heap_ptr: u32,
    /// Upper limit for heap allocations (screen buffer start)
    heap_limit: u32,
    /// Free list: maps aligned_size → stack of recycled addresses
    free_blocks: HashMap<u32, Vec<u32>>,
    /// Tracks the aligned size of each allocation (address → aligned_size)
    alloc_sizes: HashMap<u32, u32>,
    /// For best-fit allocations, the bucket capacity the block came from
    /// (always >= `alloc_sizes[addr]`). On free, the block returns to this
    /// bucket so its full capacity is recovered. Absent for blocks
    /// produced by the bump path or the exact-size fast path.
    alloc_bucket_sizes: HashMap<u32, u32>,
}

/// RAM storage - either owned vector or borrowed slice
enum RamStorage {
    Owned(Vec<u8>),
    /// Borrowed raw pointer + length (used for wrapping r68k memory)
    /// Safety: The lifetime is managed externally
    External(*mut u8, usize),
}

impl RamStorage {
    #[inline]
    fn get(&self, index: usize) -> u8 {
        match self {
            RamStorage::Owned(v) => v.get(index).copied().unwrap_or(0),
            RamStorage::External(ptr, len) => {
                if index < *len {
                    unsafe { *ptr.add(index) }
                } else {
                    0
                }
            }
        }
    }

    /// Borrow `len` bytes starting at `index` if the range lies
    /// entirely within RAM. Returns `None` when the read would straddle
    /// the RAM boundary or fall fully outside; callers fall back to the
    /// byte-at-a-time path. Used by `read_word` / `read_long` to avoid
    /// per-byte bounds checks on the hot M68K instruction-fetch path.
    #[inline]
    fn slice_at(&self, index: usize, len: usize) -> Option<&[u8]> {
        match self {
            RamStorage::Owned(v) => v.get(index..index + len),
            RamStorage::External(ptr, total_len) => {
                if index
                    .checked_add(len)
                    .map(|end| end <= *total_len)
                    .unwrap_or(false)
                {
                    Some(unsafe { std::slice::from_raw_parts(ptr.add(index), len) })
                } else {
                    None
                }
            }
        }
    }

    /// Mutable counterpart of `slice_at`. Used by `write_word` /
    /// `write_long` to do one bounds check + direct slice write
    /// instead of 2-4 `write_byte` calls each with its own bounds
    /// check. Only used in release builds; debug builds fall back to
    /// byte-at-a-time so watchpoints still fire per address.
    #[inline]
    fn slice_at_mut(&mut self, index: usize, len: usize) -> Option<&mut [u8]> {
        match self {
            RamStorage::Owned(v) => v.get_mut(index..index + len),
            RamStorage::External(ptr, total_len) => {
                if index
                    .checked_add(len)
                    .map(|end| end <= *total_len)
                    .unwrap_or(false)
                {
                    Some(unsafe { std::slice::from_raw_parts_mut(ptr.add(index), len) })
                } else {
                    None
                }
            }
        }
    }

    fn set(&mut self, index: usize, value: u8) {
        match self {
            RamStorage::Owned(v) => {
                if index < v.len() {
                    v[index] = value;
                }
            }
            RamStorage::External(ptr, len) => {
                if index < *len {
                    unsafe {
                        *ptr.add(index) = value;
                    }
                }
            }
        }
    }
}

impl MacMemoryBus {
    pub(crate) fn allocation_bucket_size(size: u32) -> u32 {
        ((size + 3) & !3).max(4)
    }

    /// `BlockMove` fast path. Copies `count` bytes from `src` to
    /// `dst`, handling overlap correctly. When both ranges are fully
    /// inside RAM and no watchpoint is armed, uses `slice::copy_within`
    /// — one bounds check, memmove-grade throughput. Falls back to
    /// byte-at-a-time (preserving the overlap-handling order from
    /// Inside Macintosh II-44) when the fast path doesn't apply.
    pub fn block_move(&mut self, src: u32, dst: u32, count: u32) {
        if count == 0 {
            return;
        }
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed) && fb_write_trace_range().is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none();
        let count_usize = count as usize;
        let src_end = (src as u64).saturating_add(count as u64);
        let dst_end = (dst as u64).saturating_add(count as u64);
        if fast && src_end <= self.ram_size as u64 && dst_end <= self.ram_size as u64 {
            let ram_size_usize = self.ram_size as usize;
            if let Some(ram) = self.ram.slice_at_mut(0, ram_size_usize) {
                let src_range = (src as usize)..(src as usize + count_usize);
                ram.copy_within(src_range, dst as usize);
                return;
            }
        }
        // Fallback with explicit overlap handling (IM II-44).
        if dst > src && dst < src.saturating_add(count) {
            for i in (0..count).rev() {
                let b = self.read_byte(src.wrapping_add(i));
                self.write_byte(dst.wrapping_add(i), b);
            }
        } else {
            for i in 0..count {
                let b = self.read_byte(src.wrapping_add(i));
                self.write_byte(dst.wrapping_add(i), b);
            }
        }
    }

    /// Create a new memory bus with the given RAM size
    pub fn new(ram_size: usize) -> Self {
        // Screen buffer is at the top of RAM; heap must not grow into it.
        let screen_buffer_start: u32 = if ram_size >= 0x100000 {
            (ram_size as u32) - 0x80000
        } else if ram_size >= 0x20000 {
            (ram_size as u32) - 0x10000
        } else {
            ram_size as u32
        };
        let mut bus = Self {
            ram: RamStorage::Owned(vec![0; ram_size]),
            ram_size: ram_size as u32,
            globals: LowMemGlobals::new(),
            heap_ptr: 0x200000, // Start heap at 2MB
            heap_limit: screen_buffer_start,
            free_blocks: HashMap::new(),
            alloc_sizes: HashMap::new(),
            alloc_bucket_sizes: HashMap::new(),
        };
        bus.write_word(super::globals::addr::ROM85, 0x7FFF);

        // Set up ScrnBase at $0824 to point to screen memory.
        // Default to 800x600 8bpp color mode. The framebuffer is placed at
        // the top of RAM minus 512KB (0x80000), which fits 800*600 = 480,000 bytes.
        // For small RAM sizes (unit tests), fall back to a safe address.
        let screen_base: u32 = if ram_size >= 0x100000 {
            (ram_size as u32) - 0x80000
        } else if ram_size >= 0x20000 {
            (ram_size as u32) - 0x10000
        } else {
            0 // Fallback for unit tests with small RAM
        };
        let screen_row_bytes: u16 = 800;
        let screen_width: u16 = 800;
        let screen_height: u16 = 600;

        // ScrnBase ($0824) - pointer to screen buffer
        bus.write_long(0x0824, screen_base);

        // screenBits BitMap structure at $083C (14 bytes)
        // BitMap: baseAddr(4) + rowBytes(2) + bounds(8) = 14 bytes
        // Stored at $083C to avoid conflicting with mouse globals at $0828-$0833.
        // Reference: Executor docs/globals.cpp — $0828 is MTemp, $082C is MouseLocation
        use super::globals::addr;
        bus.write_long(addr::SCREEN_BITS, screen_base); // baseAddr
        bus.write_word(addr::SCREEN_BITS + 4, screen_row_bytes); // rowBytes
        bus.write_word(addr::SCREEN_BITS + 6, 0); // bounds.top
        bus.write_word(addr::SCREEN_BITS + 8, 0); // bounds.left
        bus.write_word(addr::SCREEN_BITS + 10, screen_height); // bounds.bottom
        bus.write_word(addr::SCREEN_BITS + 12, screen_width); // bounds.right

        bus
    }

    /// Create a memory bus wrapping an external RAM slice
    ///
    /// # Safety
    /// The RAM slice must remain valid for the lifetime of this bus.
    #[allow(dead_code)]
    pub unsafe fn wrap_external(ram_ptr: *mut u8, ram_size: usize, globals: LowMemGlobals) -> Self {
        let screen_buffer_start: u32 = if ram_size >= 0x100000 {
            (ram_size as u32) - 0x80000
        } else if ram_size >= 0x20000 {
            (ram_size as u32) - 0x10000
        } else {
            ram_size as u32
        };
        Self {
            ram: RamStorage::External(ram_ptr, ram_size),
            ram_size: ram_size as u32,
            globals,
            heap_ptr: 0x200000,
            heap_limit: screen_buffer_start,
            free_blocks: HashMap::new(),
            alloc_sizes: HashMap::new(),
            alloc_bucket_sizes: HashMap::new(),
        }
    }

    /// Allocate memory from heap.
    /// Reuses freed blocks via best-fit (smallest free block >= request),
    /// otherwise bump-allocates. Returns 0 on OOM; callers must set
    /// memFullErr.
    /// Reserve space at the start of the heap without returning it.
    /// Used to protect zone headers from being overwritten by alloc().
    pub fn reserve_heap(&mut self, size: u32) {
        let aligned = (size + 3) & !3;
        self.heap_ptr = self.heap_ptr.max(0x200000) + aligned;
    }

    pub fn alloc(&mut self, size: u32) -> u32 {
        let aligned = Self::allocation_bucket_size(size); // 4-byte align, unique zero-size blocks

        // Fast path: exact-size bucket.
        if let Some(blocks) = self.free_blocks.get_mut(&aligned) {
            if let Some(addr) = blocks.pop() {
                self.fill_zeros(addr, aligned);
                self.alloc_sizes.insert(addr, size);
                return addr;
            }
        }

        // Best-fit fallback: find the smallest free bucket whose size
        // is >= the request. Without this, repeated alloc/free cycles
        // with mixed sizes (typical for resource loaders that load
        // large variable-size assets) fragment the heap into per-size
        // buckets that never recycle for differently-sized requests,
        // even when several megabytes of capacity sit idle. Resource-
        // heavy games (Bonkheads, Marathon) hit this limit fast.
        let best = self
            .free_blocks
            .iter()
            .filter(|(&k, v)| k > aligned && !v.is_empty())
            .map(|(&k, _)| k)
            .min();
        if let Some(bucket) = best {
            if let Some(blocks) = self.free_blocks.get_mut(&bucket) {
                if let Some(addr) = blocks.pop() {
                    // Zero only the requested span; the remainder is
                    // wasted (no splitting). Splitting would mean tracking
                    // adjacency for coalescing — out of scope here.
                    self.fill_zeros(addr, aligned);
                    // Record the *requested* size, not the bucket size,
                    // so GetPtrSize/GetHandleSize return the user-visible
                    // size. The full bucket capacity is recovered on free.
                    self.alloc_sizes.insert(addr, size);
                    self.alloc_bucket_sizes.insert(addr, bucket);
                    return addr;
                }
            }
        }

        // Bump allocate
        let ptr = self.heap_ptr;
        let new_ptr = ptr + aligned;

        if new_ptr >= self.heap_limit {
            eprintln!(
                "[ALLOC] Out of memory: requesting {} bytes, heap at ${:08X}, limit ${:08X}",
                size, ptr, self.heap_limit
            );
            return 0; // Return NULL; callers must check and set memFullErr
        }

        self.heap_ptr = new_ptr;
        self.alloc_sizes.insert(ptr, size);
        ptr
    }

    /// Return the allocated size for a given address, or None if unknown.
    pub fn get_alloc_size(&self, addr: u32) -> Option<u32> {
        self.alloc_sizes.get(&addr).copied()
    }

    /// Update the logical size of an existing allocation. Used by
    /// SetPtrSize / SetHandleSize for in-place resize. Caller is
    /// responsible for ensuring the new size fits within the original
    /// 4-byte-aligned capacity — see trap/memory.rs SetPtrSize.
    ///
    /// No-op for unknown addresses.
    pub fn set_alloc_size(&mut self, addr: u32, new_size: u32) {
        if self.alloc_sizes.contains_key(&addr) {
            self.alloc_sizes.insert(addr, new_size);
        }
    }

    /// Return a previously allocated block to the free list for reuse.
    /// Does nothing for null pointers or unknown addresses.
    pub fn free(&mut self, addr: u32) {
        if addr == 0 {
            return;
        }
        if let Some(size) = self.alloc_sizes.remove(&addr) {
            // For best-fit-recycled blocks, the bucket capacity exceeds
            // the user-visible size; return to the original bucket so
            // the full capacity stays available for the next alloc.
            let bucket = self
                .alloc_bucket_sizes
                .remove(&addr)
                .unwrap_or_else(|| Self::allocation_bucket_size(size));
            self.free_blocks.entry(bucket).or_default().push(addr);
        }
    }

    /// Return a read-only slice of contiguous RAM.
    /// Useful for bulk reads (e.g. framebuffer rendering) without per-byte
    /// method-call overhead.
    pub fn ram_slice(&self, start: u32, len: u32) -> &[u8] {
        let s = start as usize;
        let e = s + len as usize;
        match &self.ram {
            RamStorage::Owned(v) => &v[s..e],
            RamStorage::External(ptr, max_len) => {
                assert!(e <= *max_len);
                unsafe { std::slice::from_raw_parts(ptr.add(s), len as usize) }
            }
        }
    }

    /// Load data into memory at the given address
    pub fn load(&mut self, address: u32, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            let addr = address.wrapping_add(i as u32);
            if addr < self.ram_size {
                self.ram.set(addr as usize, byte);
            }
        }
    }

    /// Get reference to low-memory globals
    pub fn globals(&self) -> &LowMemGlobals {
        &self.globals
    }

    /// Get mutable reference to low-memory globals
    pub fn globals_mut(&mut self) -> &mut LowMemGlobals {
        &mut self.globals
    }

    /// Get RAM size
    pub fn ram_size(&self) -> u32 {
        self.ram_size
    }

    /// Dump stack contents around the given SP for debugging
    pub fn dump_stack(&self, sp: u32, label: &str) {
        eprintln!("[STACK DUMP] {} (SP=${:08X})", label, sp);
        let start = sp.saturating_sub(32) & !3; // Align to 4 bytes
        let end = sp.saturating_add(32);

        for addr in (start..end).step_by(4) {
            let val = self.read_long(addr);
            let marker = if addr == sp { " <--- SP" } else { "" };
            eprintln!("  ${:08X}: ${:08X}{}", addr, val, marker);
        }
    }
}

impl MemoryBus for MacMemoryBus {
    #[inline]
    fn read_byte(&self, address: u32) -> u8 {
        let v = if address < self.ram_size {
            self.ram.get(address as usize)
        } else {
            tracing::warn!("Read from unmapped address ${:08X}", address);
            0
        };
        maybe_log_mem_read(address, 1, v as u32);
        v
    }

    /// Big-endian 16-bit read.
    ///
    /// Fast path uses one bounds check + direct slice index instead of
    /// two `read_byte` calls (each with its own bounds check + branch
    /// on the `RamStorage` variant). This is on the M68K instruction-
    /// fetch hot path, so per-call overhead dominates. Falls back to
    /// the byte-by-byte path when the read straddles `self.ram_size`.
    #[inline]
    fn read_word(&self, address: u32) -> u16 {
        let v = if (address as u64) + 2 <= (self.ram_size as u64) {
            if let Some(slice) = self.ram.slice_at(address as usize, 2) {
                ((slice[0] as u16) << 8) | (slice[1] as u16)
            } else {
                // Fallback: cross-boundary or out-of-bounds
                let hi = self.read_byte(address) as u16;
                let lo = self.read_byte(address.wrapping_add(1)) as u16;
                (hi << 8) | lo
            }
        } else {
            let hi = self.read_byte(address) as u16;
            let lo = self.read_byte(address.wrapping_add(1)) as u16;
            (hi << 8) | lo
        };
        maybe_log_mem_read(address, 2, v as u32);
        v
    }

    /// Big-endian 32-bit read.
    ///
    /// Same optimisation as `read_word` — one bounds check + direct
    /// slice index when the 4 bytes lie wholly within `self.ram_size`.
    #[inline]
    fn read_long(&self, address: u32) -> u32 {
        let v = if (address as u64) + 4 <= (self.ram_size as u64) {
            if let Some(slice) = self.ram.slice_at(address as usize, 4) {
                ((slice[0] as u32) << 24)
                    | ((slice[1] as u32) << 16)
                    | ((slice[2] as u32) << 8)
                    | (slice[3] as u32)
            } else {
                let hi = self.read_word(address) as u32;
                let lo = self.read_word(address.wrapping_add(2)) as u32;
                (hi << 16) | lo
            }
        } else {
            let hi = self.read_word(address) as u32;
            let lo = self.read_word(address.wrapping_add(2)) as u32;
            (hi << 16) | lo
        };
        maybe_log_mem_read(address, 4, v);
        v
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        // Optional release-mode FB-write tracer. Cheap when unset (one
        // atomic load + None branch).
        maybe_log_fb_write(address, value);
        // Companion disassembly window: when both FB_WRITE_RANGE and
        // FB_WRITE_DISASM are set, dump the 8 instruction bytes at PC
        // alongside an m68k-disassembled mnemonic for each write that
        // falls in the watched range. Lets release-build pixel-
        // divergence investigations identify the 68k blit loop
        // responsible without a debug build.
        if let Some((start, end)) = fb_write_trace_range() {
            if address >= start && address <= end && fb_write_disasm_enabled() {
                let pc = CURRENT_PC.with(|p| *p.borrow());
                if pc != 0 && (pc as u64 + 8) <= self.ram_size as u64 {
                    let read = |off: u32| self.ram.get((pc + off) as usize);
                    let opcode_word = ((read(0) as u16) << 8) | read(1) as u16;
                    let (mnemonic, _size) =
                        m68k::dasm::disassemble(pc, opcode_word, m68k::CpuType::M68000);
                    let _size = _size.clamp(2, 10);
                    // Annotate A-line traps with their canonical trap
                    // entry. The opcode word's bits 10/11 carry trap
                    // dispatch flags (auto-pop, etc.) — masking to the
                    // canonical 10-bit trap index and re-OR'ing $A800
                    // recovers the trap name a human reader recognises.
                    // Without this annotation a Mac-aware investigator
                    // sees `DC.W $ACEC` and may not recognise it as
                    // CopyBits with the auto-pop bit set (canonical
                    // form: $A8EC). $A000-$A7FF are OS traps; $A800-
                    // $AFFF are toolbox traps with bit 10 = auto-pop
                    // (Inside Macintosh Volume I, I-220).
                    let trap_annotation = if (opcode_word & 0xF000) == 0xA000 {
                        let canonical = if (opcode_word & 0x0800) != 0 {
                            // Toolbox trap: 10-bit index, re-OR $A800.
                            0xA800u16 | (opcode_word & 0x03FF)
                        } else {
                            // OS trap: 8-bit index, re-OR $A000.
                            0xA000u16 | (opcode_word & 0x00FF)
                        };
                        let auto_pop = (opcode_word & 0x0800) != 0 && (opcode_word & 0x0400) != 0;
                        if canonical == opcode_word {
                            String::new()
                        } else if auto_pop {
                            format!(" (canonical=${:04X}, auto-pop)", canonical)
                        } else {
                            format!(" (canonical=${:04X})", canonical)
                        }
                    } else {
                        String::new()
                    };
                    eprintln!(
                        "[FB-WRITE-DISASM] PC=${:08X} bytes=[{:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}] {}{}",
                        pc,
                        read(0), read(1), read(2), read(3),
                        read(4), read(5), read(6), read(7),
                        mnemonic,
                        trap_annotation,
                    );
                    // Optional multi-instruction context: when
                    // SYSTEMLESS_TRACE_FB_WRITE_DISASM=N for N>1, walk
                    // forward N-1 more instructions after the first
                    // and dump each. Useful for spotting the loop
                    // structure around a write site (e.g. Bcc back to
                    // a label) instead of just the trapping op alone.
                    let extra = fb_write_disasm_count().saturating_sub(1);
                    if extra > 0 {
                        let mut cur = pc.wrapping_add(_size);
                        for _ in 0..extra {
                            if (cur as u64 + 2) > self.ram_size as u64 {
                                break;
                            }
                            let op = ((self.ram.get(cur as usize) as u16) << 8)
                                | self.ram.get(cur as usize + 1) as u16;
                            let (m, sz) = m68k::dasm::disassemble(cur, op, m68k::CpuType::M68000);
                            // Same A-line annotation as above.
                            let ann = if (op & 0xF000) == 0xA000 {
                                let canonical = if (op & 0x0800) != 0 {
                                    0xA800u16 | (op & 0x03FF)
                                } else {
                                    0xA000u16 | (op & 0x00FF)
                                };
                                let auto_pop = (op & 0x0800) != 0 && (op & 0x0400) != 0;
                                if canonical == op {
                                    String::new()
                                } else if auto_pop {
                                    format!(" (canonical=${:04X}, auto-pop)", canonical)
                                } else {
                                    format!(" (canonical=${:04X})", canonical)
                                }
                            } else {
                                String::new()
                            };
                            eprintln!(
                                "[FB-WRITE-DISASM]   +{:08X}                                           {}{}",
                                cur, m, ann
                            );
                            cur = cur.wrapping_add(sz.clamp(2, 10));
                        }
                    }
                }
            }
        }

        // WATCHPOINT CHECK: Only in debug builds (thread-local access is very
        // expensive in WASM and this runs on every byte write).
        #[cfg(debug_assertions)]
        if WATCHPOINT_ARMED.load(Ordering::Relaxed) {
            WATCH_ADDRESS.with(|wa| {
                if let Some(watch_addr) = *wa.borrow() {
                    // Watchpoint fires on writes of any value (including
                    // zero — e.g. MBarHeight=0 switches to fullscreen).
                    if address >= watch_addr && address < watch_addr + 4 {
                        let step = STEP_COUNTER.load(Ordering::Relaxed);
                        let pc = CURRENT_PC.with(|p| *p.borrow());
                        let a0 = CURRENT_A0.with(|r| *r.borrow());
                        let a1 = CURRENT_A1.with(|r| *r.borrow());
                        let a6 = CURRENT_A6.with(|r| *r.borrow());
                        let a7 = CURRENT_A7.with(|r| *r.borrow());
                        // Read opcode and surrounding words at PC for disassembly
                        let rw = |off: usize| -> u16 {
                            let a = pc as usize + off;
                            if a + 1 < self.ram_size as usize {
                                ((self.ram.get(a) as u16) << 8) | self.ram.get(a + 1) as u16
                            } else {
                                0
                            }
                        };
                        let op0 = rw(0);
                        let op1 = rw(2);
                        let op2 = rw(4);
                        eprintln!(
                            "WATCHPOINT at Step {} PC=${:08X} [{:04X} {:04X} {:04X}] A0=${:08X} A1=${:08X} A6=${:08X} A7=${:08X} Write ${:08X}=${:02X}",
                            step, pc, op0, op1, op2, a0, a1, a6, a7, address, value
                        );
                    }
                }
            });
        }

        if address < self.ram_size {
            self.ram.set(address as usize, value);
        } else {
            tracing::warn!(
                "Write to unmapped address ${:08X} = ${:02X}",
                address,
                value
            );
        }
    }

    /// Big-endian 16-bit write.
    ///
    /// Fast-path slice write (one bounds check + direct write) instead
    /// of two `write_byte` calls. Falls back to byte-at-a-time when
    /// (a) the write straddles `ram_size`, (b) a debug watchpoint is
    /// armed, or (c) the FB-write tracer is enabled — any of those
    /// needs per-byte dispatch through `write_byte`.
    #[inline]
    fn write_word(&mut self, address: u32, value: u16) {
        // Fast path: watchpoint disarmed + tracer disabled + write fully in-bounds.
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed) && fb_write_trace_range().is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none();
        if fast && (address as u64) + 2 <= (self.ram_size as u64) {
            if let Some(slice) = self.ram.slice_at_mut(address as usize, 2) {
                slice[0] = (value >> 8) as u8;
                slice[1] = value as u8;
                return;
            }
        }
        self.write_byte(address, (value >> 8) as u8);
        self.write_byte(address.wrapping_add(1), value as u8);
    }

    /// Big-endian 32-bit write.
    ///
    /// Same fast-path optimisation as `write_word`.
    #[inline]
    fn write_long(&mut self, address: u32, value: u32) {
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed) && fb_write_trace_range().is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none();
        if fast && (address as u64) + 4 <= (self.ram_size as u64) {
            if let Some(slice) = self.ram.slice_at_mut(address as usize, 4) {
                slice[0] = (value >> 24) as u8;
                slice[1] = (value >> 16) as u8;
                slice[2] = (value >> 8) as u8;
                slice[3] = value as u8;
                return;
            }
        }
        self.write_word(address, (value >> 16) as u16);
        self.write_word(address.wrapping_add(2), value as u16);
    }

    /// Bulk read fast path — one `slice_at` instead of `len` byte
    /// reads (each with its own bounds check + `RamStorage` dispatch).
    /// Used by `BlockMove`, resource-fork loads, and any other caller
    /// that pulls more than a few bytes at once.
    #[inline]
    fn read_bytes(&self, address: u32, len: usize) -> Vec<u8> {
        let end = (address as u64).saturating_add(len as u64);
        if end <= self.ram_size as u64 {
            if let Some(slice) = self.ram.slice_at(address as usize, len) {
                return slice.to_vec();
            }
        }
        let mut result = Vec::with_capacity(len);
        for i in 0..len {
            result.push(self.read_byte(address.wrapping_add(i as u32)));
        }
        result
    }

    /// Zero-alloc bulk read fast path — `slice_at + copy_from_slice`
    /// directly into the caller's buffer. Lets per-row readers pre-
    /// allocate one `Vec` and write row-by-row instead of allocating +
    /// copying twice per row.
    #[inline]
    fn read_bytes_into(&self, address: u32, dst: &mut [u8]) {
        let len = dst.len();
        let end = (address as u64).saturating_add(len as u64);
        if end <= self.ram_size as u64 {
            if let Some(slice) = self.ram.slice_at(address as usize, len) {
                dst.copy_from_slice(slice);
                return;
            }
        }
        for (i, byte) in dst.iter_mut().enumerate() {
            *byte = self.read_byte(address.wrapping_add(i as u32));
        }
    }

    /// Bulk write fast path — one `slice_at_mut + copy_from_slice`
    /// instead of per-byte writes. Watchpoint-armed debug builds keep
    /// the byte-at-a-time fallback so per-address watchpoints still
    /// trigger; same for the FB-write tracer.
    #[inline]
    fn write_bytes(&mut self, address: u32, data: &[u8]) {
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed) && fb_write_trace_range().is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none();
        let end = (address as u64).saturating_add(data.len() as u64);
        if fast && end <= self.ram_size as u64 {
            if let Some(slice) = self.ram.slice_at_mut(address as usize, data.len()) {
                slice.copy_from_slice(data);
                return;
            }
        }
        for (i, &byte) in data.iter().enumerate() {
            self.write_byte(address.wrapping_add(i as u32), byte);
        }
    }

    #[inline]
    fn fill_zeros(&mut self, address: u32, len: u32) {
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed) && fb_write_trace_range().is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none();
        let end = (address as u64).saturating_add(len as u64);
        if fast && end <= self.ram_size as u64 {
            if let Some(slice) = self.ram.slice_at_mut(address as usize, len as usize) {
                slice.fill(0);
                return;
            }
        }
        for i in 0..len {
            self.write_byte(address.wrapping_add(i), 0);
        }
    }

    fn ram_size(&self) -> u32 {
        self.ram_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_big_endian_word() {
        let mut bus = MacMemoryBus::new(1024);
        bus.write_word(0x100, 0x1234);
        assert_eq!(bus.read_byte(0x100), 0x12); // High byte first
        assert_eq!(bus.read_byte(0x101), 0x34);
        assert_eq!(bus.read_word(0x100), 0x1234);
    }

    #[test]
    fn test_big_endian_long() {
        let mut bus = MacMemoryBus::new(1024);
        bus.write_long(0x100, 0x12345678);
        assert_eq!(bus.read_byte(0x100), 0x12);
        assert_eq!(bus.read_byte(0x101), 0x34);
        assert_eq!(bus.read_byte(0x102), 0x56);
        assert_eq!(bus.read_byte(0x103), 0x78);
        assert_eq!(bus.read_long(0x100), 0x12345678);
    }

    #[test]
    fn test_pascal_string() {
        let mut bus = MacMemoryBus::new(1024);
        bus.write_pstring(0x100, b"Hello");
        assert_eq!(bus.read_byte(0x100), 5); // Length byte
        assert_eq!(bus.read_pstring(0x100), b"Hello".to_vec());
    }

    #[test]
    fn zero_size_allocations_get_unique_slots() {
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);

        let zero = bus.alloc(0);
        let next = bus.alloc(4);

        assert_ne!(zero, 0);
        assert_ne!(
            zero, next,
            "zero-size allocations must not alias the following allocation"
        );
        assert_eq!(
            bus.get_alloc_size(zero),
            Some(0),
            "the logical allocation size should remain zero"
        );
        assert_eq!(bus.get_alloc_size(next), Some(4));

        bus.free(zero);
        let reused = bus.alloc(1);
        assert_eq!(
            reused, zero,
            "the minimum bucket for a freed zero-size allocation should be reusable"
        );
    }

    /// Pascal strings are length-byte + up-to-255-byte data; passing a
    /// longer source must clamp to 255 (the length byte's max value),
    /// never wrap or truncate via the unchecked `len as u8` cast.
    /// Prevents a class of guest-corrupting silent overflows.
    /// Symmetric byte-isomorphism gate for `write_bytes`: the
    /// MacMemoryBus override copies via `slice.copy_from_slice` for
    /// the on-RAM case, falling back to per-byte writes when the
    /// destination range straddles `ram_size`. Pre-stamp a sentinel
    /// outside the write window to guarantee neither path overruns.
    #[test]
    fn write_bytes_fast_path_matches_byte_loop() {
        let mut bus = MacMemoryBus::new(64 * 1024);
        // Sentinel outside [0x1000, 0x1000+1000)
        bus.write_byte(0x0FFF, 0xCC);
        bus.write_byte(0x13E8, 0xCC);

        let payload: Vec<u8> = (0..1000).map(|i| ((i * 37) & 0xFF) as u8).collect();
        bus.write_bytes(0x1000, &payload);

        // Round-trip: read_bytes must return the same payload.
        assert_eq!(bus.read_bytes(0x1000, 1000), payload);
        // Sentinels untouched.
        assert_eq!(
            bus.read_byte(0x0FFF),
            0xCC,
            "byte before write_bytes window"
        );
        assert_eq!(bus.read_byte(0x13E8), 0xCC, "byte after write_bytes window");
    }

    #[test]
    fn read_pstring_handles_zero_and_max_lengths() {
        let mut bus = MacMemoryBus::new(8 * 1024);

        // length-0 → empty Vec
        bus.write_byte(0x100, 0);
        assert_eq!(bus.read_pstring(0x100), Vec::<u8>::new());

        // length-255 round-trips intact (Pascal max)
        bus.write_pstring(0x200, &vec![0x77u8; 255]);
        assert_eq!(bus.read_pstring(0x200), vec![0x77u8; 255]);
    }

    #[test]
    fn write_pstring_clamps_to_255_bytes() {
        let mut bus = MacMemoryBus::new(8 * 1024);
        let huge = vec![0x33u8; 1000];
        bus.write_pstring(0x100, &huge);
        assert_eq!(bus.read_byte(0x100), 255);
        assert_eq!(bus.read_pstring(0x100).len(), 255);
        // The 256th byte (one past the clamped data) must NOT be 0x33.
        // The clamp must not have walked past byte 254 of the source.
        assert_eq!(
            bus.read_byte(0x100 + 256),
            0,
            "byte after the clamped 255-byte payload must be untouched"
        );
    }

    /// Byte-isomorphism gate for the `read_bytes_into` fast path. The
    /// `MacMemoryBus` override routes through `slice_at +
    /// dst.copy_from_slice` for the on-RAM case; the default trait
    /// impl falls back to per-byte `read_byte`. A regression to either
    /// path that returns wrong bytes (off-by-one stride, wrong
    /// fallback condition, missed length check) would silently corrupt
    /// callers.
    #[test]
    fn read_bytes_into_matches_read_bytes() {
        let mut bus = MacMemoryBus::new(64 * 1024);
        // Stamp a deterministic per-byte pattern so off-by-one
        // stride bugs surface as shifted output (uniform fill would
        // hide them).
        for i in 0..1024u32 {
            bus.write_byte(0x1000 + i, ((i.wrapping_mul(13)) & 0xFF) as u8);
        }

        // Compare on the fast path (fully on-RAM, address aligned).
        let baseline = bus.read_bytes(0x1000, 619);
        let mut into = vec![0u8; 619];
        bus.read_bytes_into(0x1000, &mut into);
        assert_eq!(
            baseline, into,
            "read_bytes_into fast path must return identical bytes to read_bytes"
        );

        // Compare on the boundary fallback (read straddles ram_size).
        // ram_size = 64 KB; reading from 0xFFF0 (the last 16 bytes) is
        // entirely on-RAM, but reading from 0xFFF0 with len 32 straddles.
        let baseline_straddle = bus.read_bytes(0xFFF0, 32);
        let mut into_straddle = vec![0u8; 32];
        bus.read_bytes_into(0xFFF0, &mut into_straddle);
        assert_eq!(
            baseline_straddle, into_straddle,
            "read_bytes_into must match read_bytes even on the boundary fallback"
        );

        // Empty slice is a no-op.
        let mut empty: [u8; 0] = [];
        bus.read_bytes_into(0x1234, &mut empty);
        // No assertion needed — just verifying no panic.
    }

    /// Pin the contract for `fill_zeros`: writes `len` zero bytes
    /// starting at `address`. Verifies both the on-RAM fast path
    /// (uses `slice.fill(0)`) and the straddle / out-of-range
    /// fallback. Used by NewPtrClear / NewHandleClear allocators on
    /// the hot path so a regression here would touch every game.
    #[test]
    fn fill_zeros_clears_target_bytes_only() {
        let mut bus = MacMemoryBus::new(64 * 1024);
        // Stamp a non-zero pattern around the target window.
        for i in 0..1024u32 {
            bus.write_byte(0x1000 + i, 0xAA);
        }

        // Fast path: zero a 100-byte window in the middle.
        bus.fill_zeros(0x1100, 100);
        for i in 0..0x100u32 {
            assert_eq!(bus.read_byte(0x1000 + i), 0xAA, "before window untouched");
        }
        for i in 0..100u32 {
            assert_eq!(bus.read_byte(0x1100 + i), 0, "fill_zeros target zero");
        }
        for i in 0..100u32 {
            assert_eq!(bus.read_byte(0x1164 + i), 0xAA, "after window untouched");
        }

        // Zero-length is a no-op.
        bus.fill_zeros(0x1000, 0);
        assert_eq!(bus.read_byte(0x1000), 0xAA);

        // Boundary: an end-of-RAM straddle takes the byte-by-byte
        // fallback, not the slice fast path. Verify both the in-RAM
        // tail and the suffix that wraps past ram_size still write
        // zeros consistently.
        for i in 0u32..16 {
            bus.write_byte(0xFFF0 + i, 0xCC);
        }
        bus.fill_zeros(0xFFF0, 32); // ram_size = 64 KB → 0x10000
        for i in 0u32..16 {
            assert_eq!(
                bus.read_byte(0xFFF0 + i),
                0,
                "in-RAM tail of straddling fill_zeros"
            );
        }
    }
}
