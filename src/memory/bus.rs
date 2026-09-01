//! Memory bus implementation
//!
//! Provides Big-Endian memory access compatible with 68k Mac architecture.

use std::cell::{RefCell, UnsafeCell};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

use super::globals::LowMemGlobals;
use super::{GuestAddressSpace, SharedGuestAddressSpace};

const LEGACY_SOUND_BUFFER_WORDS: u32 = 370;
const LEGACY_SOUND_BUFFER_BYTES: u32 = LEGACY_SOUND_BUFFER_WORDS * 2;
const SYNTHETIC_RESERVE_BYTES: u32 = 64 * 1024;
// System 7.5.3 on the Quadra 650 leaves exception vector 0 pointing to
// $40810000. A BasiliskII oracle capture of that ROM establishes the word at
// offset 6 as $0372. Keep the shadow deliberately narrow: bytes outside this
// witnessed range retain the bus's existing unmapped-zero behavior.
const BOOT_ROM_SHADOW_BASE: u32 = 0x4081_0006;
const BOOT_ROM_SHADOW: [u8; 2] = 0x0372u16.to_be_bytes();
// Release-mode tracer for writes to a guest address range. Use to
// localize the source of unexpected pixel writes in the framebuffer
// or to any other narrow guest memory range. Format:
//   `SYSTEMLESS_TRACE_FB_WRITE_RANGE=START_HEX:END_HEX`
// Both inclusive. Each write to an address in [start, end] logs the
// guest PC + address + value to stderr. Cheap when unset (one atomic
// load + branch on the hot path).
#[cfg(not(target_arch = "wasm32"))]
static FB_WRITE_TRACE_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
static ALLOC_TRACE_MIN: OnceLock<Option<u32>> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
#[inline]
fn fb_write_trace_range() -> Option<(u32, u32)> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
#[inline]
fn alloc_trace_min() -> Option<u32> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
#[inline]
fn alloc_trace_min() -> Option<u32> {
    *ALLOC_TRACE_MIN.get_or_init(|| {
        std::env::var("SYSTEMLESS_TRACE_ALLOC_MIN")
            .ok()
            .and_then(|value| {
                let value = value.trim();
                let parsed = if let Some(hex) = value
                    .strip_prefix("0x")
                    .or_else(|| value.strip_prefix("0X"))
                {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    value.parse().ok()
                }?;
                Some(parsed)
            })
    })
}

#[inline]
fn trace_alloc_event(event: &str, addr: u32, size: u32, bucket: u32) {
    if let Some(min) = alloc_trace_min() {
        if size >= min || bucket >= min {
            eprintln!(
                "[ALLOC] {} addr=${:08X} size={} bucket={}",
                event, addr, size, bucket
            );
        }
    }
}

/// `true` when SYSTEMLESS_TRACE_FB_WRITE_RANGE is set; the runner uses this
/// to decide whether to mirror guest PC into [`CURRENT_PC`] in release.
#[inline]
pub fn fb_write_trace_active() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
static FB_WRITE_DISASM_COUNT: OnceLock<usize> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
#[inline]
fn fb_write_disasm_count() -> usize {
    0
}

#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
static MEM_READ_TRACE_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
static MEM_WRITE_TRACE_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[inline]
fn mem_write_trace_range() -> Option<(u32, u32)> {
    *MEM_WRITE_TRACE_RANGE.get_or_init(|| {
        std::env::var("SYSTEMLESS_TRACE_MEM_WRITE_RANGE")
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

#[cfg(not(target_arch = "wasm32"))]
pub fn mem_read_trace_active() -> bool {
    mem_read_trace_range().is_some()
}

#[cfg(target_arch = "wasm32")]
pub fn mem_read_trace_active() -> bool {
    false
}

#[cfg(not(target_arch = "wasm32"))]
pub fn mem_write_trace_active() -> bool {
    mem_write_trace_range().is_some()
}

#[cfg(target_arch = "wasm32")]
pub fn mem_write_trace_active() -> bool {
    false
}

#[inline]
fn maybe_log_mem_read(address: u32, width: u8, value: u32) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (address, width, value);
    }
    #[cfg(not(target_arch = "wasm32"))]
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

#[inline]
fn maybe_log_mem_write(address: u32, width: u8, value: u32) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (address, width, value);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((start, end)) = mem_write_trace_range() {
        if address >= start && address <= end {
            let pc = CURRENT_PC.with(|p| *p.borrow());
            eprintln!(
                "[MEM-WRITE] PC=${:08X} addr=${:08X} width={} value=${:0width$X}",
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

    /// Highest address available to the application heap, globals, and stack.
    /// Implementations may reserve RAM above this boundary for emulated
    /// hardware and host-owned callback code.
    fn application_memory_limit(&self) -> u32 {
        self.ram_size()
    }

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

    /// Fill a region of memory with a repeated byte.
    fn fill_bytes(&mut self, address: u32, len: u32, value: u8) {
        for i in 0..len {
            self.write_byte(address.wrapping_add(i), value);
        }
    }

    /// Write `value` to `count` bytes spaced `stride` apart starting at
    /// `address` — one column of an 8-bit framebuffer. Default impl is a
    /// byte-by-byte loop; the [`MacMemoryBus`] override takes one bounds
    /// check for the whole span.
    fn fill_bytes_strided(&mut self, address: u32, stride: u32, count: u32, value: u8) {
        for i in 0..count {
            self.write_byte(address.wrapping_add(i.wrapping_mul(stride)), value);
        }
    }
}

#[derive(Debug)]
struct ClassicHeapAllocatorState {
    /// Heap allocation pointer (grows upward from 0x200000).
    heap_ptr: u32,
    /// Free list: maps aligned_size to recycled addresses.
    free_blocks: HashMap<u32, Vec<u32>>,
    /// Logical allocation sizes keyed by guest address.
    alloc_sizes: HashMap<u32, u32>,
    /// Direct-loaded application image spans that heap allocations must skip.
    reserved_heap_ranges: Vec<(u32, u32)>,
    /// Capacity retained when a best-fit recycled block exceeds its request.
    alloc_bucket_sizes: HashMap<u32, u32>,
}

impl Default for ClassicHeapAllocatorState {
    fn default() -> Self {
        Self {
            heap_ptr: 0x20_0000,
            free_blocks: HashMap::new(),
            alloc_sizes: HashMap::new(),
            reserved_heap_ranges: Vec::new(),
            alloc_bucket_sizes: HashMap::new(),
        }
    }
}

/// Stable process-level ownership for the classic Memory Manager heap.
///
/// A `MacMemoryBus` retains only an adapter reference after its allocator is
/// adopted by `ProcessMemoryManager`. Detached buses receive independent
/// instances. Inside Macintosh: Memory (1992), pp. 2-19--2-21, 2-35--2-44.
#[derive(Debug, Clone, Default)]
pub(crate) struct SharedClassicHeapAllocator(Rc<RefCell<ClassicHeapAllocatorState>>);

impl SharedClassicHeapAllocator {
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    fn is_pristine(&self) -> bool {
        let state = self.0.borrow();
        state.heap_ptr == 0x20_0000
            && state.free_blocks.is_empty()
            && state.alloc_sizes.is_empty()
            && state.reserved_heap_ranges.is_empty()
            && state.alloc_bucket_sizes.is_empty()
    }

    pub(crate) fn allocation_size(&self, address: u32) -> Option<u32> {
        self.0.borrow().alloc_sizes.get(&address).copied()
    }

    fn allocation_bucket_size(size: u32) -> u32 {
        ((size + 3) & !3).max(4)
    }

    fn can_reuse_bucket_for_request(bucket: u32, requested: u32) -> bool {
        let max_bucket = if requested <= 1024 {
            4096
        } else {
            requested.saturating_mul(2).saturating_add(4096)
        };
        bucket <= max_bucket
    }

    fn reserve_until(&self, end_addr: u32) {
        let aligned = (end_addr + 3) & !3;
        let mut state = self.0.borrow_mut();
        state.heap_ptr = state.heap_ptr.max(aligned);
    }

    fn reserve_range(&self, start_addr: u32, end_addr: u32) {
        let start = start_addr & !3;
        let end = (end_addr.saturating_add(3)) & !3;
        if start >= end {
            return;
        }
        let mut state = self.0.borrow_mut();
        state.reserved_heap_ranges.push((start, end));
        state.reserved_heap_ranges.sort_unstable();
    }

    fn bump_allocation_address(
        state: &ClassicHeapAllocatorState,
        size: u32,
        alignment: u32,
    ) -> Option<(u32, u32)> {
        let mut ptr = (state.heap_ptr + alignment - 1) & !(alignment - 1);
        loop {
            let new_ptr = ptr.checked_add(size)?;
            let overlap = state
                .reserved_heap_ranges
                .iter()
                .find(|&&(start, end)| ptr < end && new_ptr > start);
            if let Some(&(_, end)) = overlap {
                ptr = (end + alignment - 1) & !(alignment - 1);
                continue;
            }
            return Some((ptr, new_ptr));
        }
    }

    fn allocate(&self, size: u32, alignment: u32, heap_limit: u32) -> u32 {
        let alignment = if alignment > 4 && alignment.is_power_of_two() {
            alignment
        } else {
            4
        };
        let aligned_size = Self::allocation_bucket_size(size);
        let mut state = self.0.borrow_mut();

        if let Some(blocks) = state.free_blocks.get_mut(&aligned_size) {
            let index = if alignment == 4 {
                blocks.len().checked_sub(1)
            } else {
                blocks.iter().position(|addr| addr % alignment == 0)
            };
            if let Some(index) = index {
                let addr = blocks.swap_remove(index);
                state.alloc_sizes.insert(addr, size);
                let event = if alignment == 4 {
                    "reuse-exact"
                } else {
                    "reuse-exact-aligned"
                };
                trace_alloc_event(event, addr, size, aligned_size);
                return addr;
            }
        }

        let best = state
            .free_blocks
            .iter()
            .filter(|(&bucket, blocks)| {
                bucket > aligned_size
                    && !blocks.is_empty()
                    && Self::can_reuse_bucket_for_request(bucket, aligned_size)
                    && (alignment == 4 || blocks.iter().any(|addr| addr % alignment == 0))
            })
            .map(|(&bucket, _)| bucket)
            .min();
        if let Some(bucket) = best {
            let blocks = state
                .free_blocks
                .get_mut(&bucket)
                .expect("free bucket exists");
            let index = if alignment == 4 {
                blocks.len() - 1
            } else {
                blocks
                    .iter()
                    .position(|addr| addr % alignment == 0)
                    .expect("aligned free block exists")
            };
            let addr = blocks.swap_remove(index);
            state.alloc_sizes.insert(addr, size);
            state.alloc_bucket_sizes.insert(addr, bucket);
            let event = if alignment == 4 {
                "reuse-best"
            } else {
                "reuse-best-aligned"
            };
            trace_alloc_event(event, addr, size, bucket);
            return addr;
        }

        let Some((ptr, new_ptr)) = Self::bump_allocation_address(&state, aligned_size, alignment)
        else {
            return 0;
        };
        if new_ptr >= heap_limit {
            eprintln!(
                "[ALLOC] Out of memory: requesting {} bytes aligned to {}, heap at ${:08X}, limit ${:08X}",
                size, alignment, ptr, heap_limit
            );
            return 0;
        }
        state.heap_ptr = new_ptr;
        state.alloc_sizes.insert(ptr, size);
        let event = if alignment == 4 {
            "bump"
        } else {
            "bump-aligned"
        };
        trace_alloc_event(event, ptr, size, aligned_size);
        ptr
    }

    fn heap_bump_ptr(&self) -> u32 {
        self.0.borrow().heap_ptr
    }

    fn set_allocation_size(&self, address: u32, size: u32) {
        let mut state = self.0.borrow_mut();
        if state.alloc_sizes.contains_key(&address) {
            state.alloc_sizes.insert(address, size);
        }
    }

    fn free(&self, address: u32) {
        if address == 0 {
            return;
        }
        let mut state = self.0.borrow_mut();
        if let Some(size) = state.alloc_sizes.remove(&address) {
            let bucket = state
                .alloc_bucket_sizes
                .remove(&address)
                .unwrap_or_else(|| Self::allocation_bucket_size(size));
            state.free_blocks.entry(bucket).or_default().push(address);
            trace_alloc_event("free", address, size, bucket);
        }
    }
}

/// Flat guest RAM with low-memory globals, a process heap adapter, and diagnostics.
pub struct MacMemoryBus {
    ram: RamStorage,
    ram_size: u32,
    /// Whether guest addresses use all 32 bits. In 24-bit mode the upper
    /// byte is metadata and every memory access wraps through the low 24 bits.
    addressing_32_bit: bool,
    globals: LowMemGlobals,
    /// Classic heap state, process-owned after runner attachment.
    heap_allocator: SharedClassicHeapAllocator,
    /// Systemless-owned executable/data stubs grow downward outside the guest heap.
    synthetic_ptr: u32,
    /// Lower bound of the fixed reservation for future synthetic allocations.
    synthetic_floor: u32,
    /// Guest writes cannot modify synthesized ROM-like instruction stubs.
    readonly_code_ranges: Vec<(u32, u32)>,
    /// Half-open bounding box over `readonly_code_ranges`, maintained on
    /// insert. Every guest write consults the protection list, but the
    /// ranges are synthetic-region trampolines clustered well away from the
    /// heap, stack and framebuffer that real writes target -- so this lets
    /// the overwhelmingly common case reject in two comparisons instead of
    /// scanning the list.
    readonly_code_span: Option<(u32, u32)>,
    /// Original byte values for a short, explicitly requested execution
    /// probe. While present, fast-memory and bulk-write paths are disabled so
    /// every guest and HLE write passes through `write_byte`. Comparing the
    /// journal with final RAM proves whether a candidate execution cycle left
    /// guest memory unchanged, while still allowing temporary stack writes
    /// that are restored before the cycle closes.
    write_probe_original: Option<WriteProbeJournal>,
    /// The journal's allocation between probes. Probes start more than a
    /// million times in a long SimCity 2000 session; reusing one map keeps
    /// the table's capacity instead of regrowing it from empty each time.
    write_probe_spare: WriteProbeJournal,
    /// An out-of-range write makes the probe unverifiable even though the
    /// normal bus retains its legacy warn-and-ignore behavior.
    write_probe_invalid: bool,
    /// The journal outgrew [`WRITE_PROBE_MAX_ENTRIES`] and was dropped: the
    /// probed cycle was doing work, not waiting. Sticky until the runner
    /// takes it, so the verdict survives the journal's disappearance.
    write_probe_overflowed: bool,
    /// Armed journal has no entry cap (parked-repaint byte-identity check:
    /// no guest code runs, so unbounded growth is impossible and overflow
    /// must not void the check).
    write_probe_uncapped: bool,
    /// Temporary sparse-memory view used only while a parked native process
    /// runs its emulated 68k context. The runner installs and removes this
    /// view around each serialized execution interval.
    foreign_address_space: Option<SharedGuestAddressSpace>,
}

/// An exact-state probe journals the words an idle cycle writes: a spilled
/// register or two, an event record, a saved keymap -- tens of bytes, a few
/// hundred at most. A journal past this many distinct words is not
/// watching a wait; it is watching real work, and while it stays open every
/// guest store pays a hash insert and fastmem is suspended for the whole
/// core. Past the cap the probe is voided on the spot so normal fast paths
/// resume immediately, and the runner backs off from that site for the rest
/// of the tick.
pub(crate) const WRITE_PROBE_MAX_ENTRIES: usize = 4096;

/// Stable shared ownership of the runner's flat RAM allocation.
///
/// `FixtureRunner` serializes access to its 68k bus and native application
/// behind `&mut self`. A mapped [`SharedRamRegion`] can therefore expose the
/// same allocation to the PowerPC address-space adapter without concurrent
/// access or a dangling pointer when the runner is moved.
#[derive(Clone)]
struct SharedRam(Rc<UnsafeCell<Box<[u8]>>>);

impl SharedRam {
    #[inline]
    fn len(&self) -> usize {
        // SAFETY: the boxed allocation is fixed for every shared handle.
        unsafe { (&*self.0.get()).len() }
    }

    #[inline]
    fn as_ptr(&self) -> *const u8 {
        // SAFETY: the boxed allocation is fixed for every shared handle.
        unsafe { (&*self.0.get()).as_ptr() }
    }

    #[inline]
    fn as_mut_ptr(&self) -> *mut u8 {
        // SAFETY: dereferencing this pointer remains subject to the serialized
        // access contract on `SharedRamRegion`.
        unsafe { (&mut *self.0.get()).as_mut_ptr() }
    }
}

/// A stable subrange of runner RAM mapped into another CPU bus.
#[derive(Clone)]
pub(crate) struct SharedRamRegion {
    ram: SharedRam,
    offset: usize,
    len: usize,
}

impl std::fmt::Debug for SharedRamRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedRamRegion")
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl SharedRamRegion {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Read a byte from the shared subrange.
    ///
    /// # Safety
    ///
    /// The caller must serialize access with the source [`MacMemoryBus`] and
    /// must not retain a source RAM slice or fast-memory window.
    #[inline]
    pub(crate) unsafe fn read(&self, offset: usize) -> Option<u8> {
        (offset < self.len).then(|| {
            // SAFETY: the caller upholds serialization and the offset was
            // checked against the shared subrange.
            unsafe { *self.ram.as_ptr().add(self.offset + offset) }
        })
    }

    /// Write a byte into the shared subrange.
    ///
    /// # Safety
    ///
    /// The caller must serialize access with the source [`MacMemoryBus`] and
    /// must not retain a source RAM slice or fast-memory window.
    #[inline]
    pub(crate) unsafe fn write(&self, offset: usize, value: u8) -> Option<()> {
        if offset >= self.len {
            return None;
        }
        // SAFETY: the caller upholds serialization and the offset was checked
        // against the shared subrange.
        unsafe {
            *self.ram.as_mut_ptr().add(self.offset + offset) = value;
        }
        Some(())
    }

    pub(crate) fn snapshot(&self) -> Vec<u8> {
        (0..self.len)
            .map(|offset| {
                // SAFETY: snapshotting occurs while the address-space clone
                // has exclusive access to its runtime-owned mapping.
                unsafe { self.read(offset).expect("bounded shared RAM read") }
            })
            .collect()
    }

    pub(crate) fn detached_clone(&self) -> Self {
        let bytes = self.snapshot();
        let ram = SharedRam(Rc::new(UnsafeCell::new(bytes.into_boxed_slice())));
        Self {
            len: ram.len(),
            ram,
            offset: 0,
        }
    }
}

/// Hasher for the write-probe journal, which is keyed by RAM address.
/// The journal takes one insert per byte written while an idle-cycle probe
/// is armed, so the default SipHash was most of a probe's cost. A
/// multiplicative mix of the 32-bit key, folded so the high address bits
/// reach the low hash bits, is enough for hashbrown, which draws its tag
/// from the top bits and its bucket from the low ones.
#[derive(Default, Clone, Copy)]
struct AddressHasher(u64);

impl Hasher for AddressHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    #[inline]
    fn write_u32(&mut self, address: u32) {
        let mixed = (u64::from(address) ^ self.0).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = mixed ^ (mixed >> 32);
    }
}

/// Original contents of the aligned 32-bit words a probe's writes touched,
/// keyed by the word's address. One insert per word instead of one per
/// byte; the bytes of a word a write did not touch cannot change while a
/// probe is armed (every writer goes through the bus and fastmem is
/// withdrawn), so comparing them at the end is harmless.
type WriteProbeJournal = HashMap<u32, u32, BuildHasherDefault<AddressHasher>>;

/// An armed write journal temporarily detached from the bus by
/// [`MacMemoryBus::suspend_write_probe`]; hand it back with
/// [`MacMemoryBus::resume_write_probe`].
pub(crate) struct SuspendedWriteProbe(WriteProbeJournal);

/// RAM storage - either a stable owned allocation or borrowed slice.
enum RamStorage {
    Owned(Vec<u8>),
    Shared(SharedRam),
    /// Borrowed raw pointer + length (used for wrapping r68k memory)
    /// Safety: The lifetime is managed externally
    External(*mut u8, usize),
}

impl RamStorage {
    #[inline]
    fn get(&self, index: usize) -> u8 {
        match self {
            RamStorage::Owned(v) => v.get(index).copied().unwrap_or(0),
            RamStorage::Shared(v) => {
                if index < v.len() {
                    unsafe { *v.as_ptr().add(index) }
                } else {
                    0
                }
            }
            RamStorage::External(ptr, len) => {
                if index < *len {
                    unsafe { *ptr.add(index) }
                } else {
                    0
                }
            }
        }
    }

    #[inline]
    fn get_in_bounds(&self, index: usize) -> u8 {
        // Callers have already checked the access against `ram_size`;
        // avoid repeating slice bounds checks on the instruction hot path.
        match self {
            RamStorage::Owned(v) => unsafe { *v.as_ptr().add(index) },
            RamStorage::Shared(v) => unsafe { *v.as_ptr().add(index) },
            RamStorage::External(ptr, _) => unsafe { *ptr.add(index) },
        }
    }

    #[inline]
    fn read_word_in_bounds(&self, index: usize) -> u16 {
        match self {
            RamStorage::Owned(v) => unsafe {
                let ptr = v.as_ptr().add(index);
                u16::from_be_bytes([*ptr, *ptr.add(1)])
            },
            RamStorage::Shared(v) => unsafe {
                let ptr = v.as_ptr().add(index);
                u16::from_be_bytes([*ptr, *ptr.add(1)])
            },
            RamStorage::External(ptr, _) => unsafe {
                let ptr = ptr.add(index);
                u16::from_be_bytes([*ptr, *ptr.add(1)])
            },
        }
    }

    #[inline]
    fn read_long_in_bounds(&self, index: usize) -> u32 {
        match self {
            RamStorage::Owned(v) => unsafe {
                let ptr = v.as_ptr().add(index);
                u32::from_be_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)])
            },
            RamStorage::Shared(v) => unsafe {
                let ptr = v.as_ptr().add(index);
                u32::from_be_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)])
            },
            RamStorage::External(ptr, _) => unsafe {
                let ptr = ptr.add(index);
                u32::from_be_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)])
            },
        }
    }

    #[inline]
    fn set_in_bounds(&mut self, index: usize, value: u8) {
        match self {
            RamStorage::Owned(v) => unsafe {
                *v.as_mut_ptr().add(index) = value;
            },
            RamStorage::Shared(v) => unsafe { *v.as_mut_ptr().add(index) = value },
            RamStorage::External(ptr, _) => unsafe {
                *ptr.add(index) = value;
            },
        }
    }

    #[inline]
    fn write_word_in_bounds(&mut self, index: usize, value: u16) {
        let bytes = value.to_be_bytes();
        match self {
            RamStorage::Owned(v) => unsafe {
                let ptr = v.as_mut_ptr().add(index);
                *ptr = bytes[0];
                *ptr.add(1) = bytes[1];
            },
            RamStorage::Shared(v) => unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), v.as_mut_ptr().add(index), 2);
            },
            RamStorage::External(ptr, _) => unsafe {
                let ptr = ptr.add(index);
                *ptr = bytes[0];
                *ptr.add(1) = bytes[1];
            },
        }
    }

    #[inline]
    fn write_long_in_bounds(&mut self, index: usize, value: u32) {
        let bytes = value.to_be_bytes();
        match self {
            RamStorage::Owned(v) => unsafe {
                let ptr = v.as_mut_ptr().add(index);
                *ptr = bytes[0];
                *ptr.add(1) = bytes[1];
                *ptr.add(2) = bytes[2];
                *ptr.add(3) = bytes[3];
            },
            RamStorage::Shared(v) => unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), v.as_mut_ptr().add(index), 4);
            },
            RamStorage::External(ptr, _) => unsafe {
                let ptr = ptr.add(index);
                *ptr = bytes[0];
                *ptr.add(1) = bytes[1];
                *ptr.add(2) = bytes[2];
                *ptr.add(3) = bytes[3];
            },
        }
    }

    #[inline]
    fn write_bytes_in_bounds(&mut self, index: usize, data: &[u8]) {
        match self {
            RamStorage::Owned(v) => unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), v.as_mut_ptr().add(index), data.len());
            },
            RamStorage::Shared(v) => unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), v.as_mut_ptr().add(index), data.len());
            },
            RamStorage::External(ptr, _) => unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.add(index), data.len());
            },
        }
    }

    #[inline]
    fn copy_bytes_in_bounds(&mut self, src_index: usize, dst_index: usize, len: usize) {
        match self {
            RamStorage::Owned(v) => unsafe {
                std::ptr::copy(
                    v.as_ptr().add(src_index),
                    v.as_mut_ptr().add(dst_index),
                    len,
                );
            },
            RamStorage::Shared(v) => unsafe {
                std::ptr::copy(
                    v.as_ptr().add(src_index),
                    v.as_mut_ptr().add(dst_index),
                    len,
                );
            },
            RamStorage::External(ptr, _) => unsafe {
                std::ptr::copy(ptr.add(src_index), ptr.add(dst_index), len);
            },
        }
    }

    #[inline]
    fn copy_mapped_bytes_in_bounds(
        &mut self,
        src_index: usize,
        dst_index: usize,
        len: usize,
        map: &[u8; 256],
    ) {
        match self {
            RamStorage::Owned(v) => unsafe {
                let src = v.as_ptr().add(src_index);
                let dst = v.as_mut_ptr().add(dst_index);
                for offset in 0..len {
                    *dst.add(offset) = map[*src.add(offset) as usize];
                }
            },
            RamStorage::Shared(v) => unsafe {
                let src = v.as_ptr().add(src_index);
                let dst = v.as_mut_ptr().add(dst_index);
                for offset in 0..len {
                    *dst.add(offset) = map[*src.add(offset) as usize];
                }
            },
            RamStorage::External(ptr, _) => unsafe {
                let src = ptr.add(src_index);
                let dst = ptr.add(dst_index);
                for offset in 0..len {
                    *dst.add(offset) = map[*src.add(offset) as usize];
                }
            },
        }
    }

    #[inline]
    fn fill_zeros_in_bounds(&mut self, index: usize, len: usize) {
        match self {
            RamStorage::Owned(v) => unsafe {
                std::ptr::write_bytes(v.as_mut_ptr().add(index), 0, len);
            },
            RamStorage::Shared(v) => unsafe {
                std::ptr::write_bytes(v.as_mut_ptr().add(index), 0, len);
            },
            RamStorage::External(ptr, _) => unsafe {
                std::ptr::write_bytes(ptr.add(index), 0, len);
            },
        }
    }

    #[inline]
    fn fill_bytes_in_bounds(&mut self, index: usize, len: usize, value: u8) {
        match self {
            RamStorage::Owned(v) => unsafe {
                std::ptr::write_bytes(v.as_mut_ptr().add(index), value, len);
            },
            RamStorage::Shared(v) => unsafe {
                std::ptr::write_bytes(v.as_mut_ptr().add(index), value, len);
            },
            RamStorage::External(ptr, _) => unsafe {
                std::ptr::write_bytes(ptr.add(index), value, len);
            },
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
            RamStorage::Shared(v) => {
                let end = index.checked_add(len)?;
                (end <= v.len())
                    .then(|| unsafe { std::slice::from_raw_parts(v.as_ptr().add(index), len) })
            }
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
            RamStorage::Shared(v) => {
                let end = index.checked_add(len)?;
                (end <= v.len()).then(|| unsafe {
                    std::slice::from_raw_parts_mut(v.as_mut_ptr().add(index), len)
                })
            }
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
            RamStorage::Shared(v) => {
                if index < v.len() {
                    unsafe {
                        *v.as_mut_ptr().add(index) = value;
                    }
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
    #[inline]
    fn boot_rom_shadow_byte(address: u32) -> Option<u8> {
        let offset = address.checked_sub(BOOT_ROM_SHADOW_BASE)? as usize;
        BOOT_ROM_SHADOW.get(offset).copied()
    }

    pub(crate) fn allocation_bucket_size(size: u32) -> u32 {
        SharedClassicHeapAllocator::allocation_bucket_size(size)
    }

    pub(crate) fn shared_classic_heap_allocator(&self) -> SharedClassicHeapAllocator {
        self.heap_allocator.clone()
    }

    pub(crate) fn attach_classic_heap_allocator(
        &mut self,
        allocator: SharedClassicHeapAllocator,
    ) {
        if self.heap_allocator.ptr_eq(&allocator) {
            return;
        }
        assert!(
            self.heap_allocator.is_pristine(),
            "cannot discard active classic heap state while attaching a process allocator"
        );
        self.heap_allocator = allocator;
    }

    fn legacy_sound_base_address(ram_size: usize, screen_base: u32, screen_bytes: u32) -> u32 {
        let ram_size = ram_size as u32;
        let preferred = screen_base.saturating_add(screen_bytes);
        if preferred >= 0x1000 && preferred + LEGACY_SOUND_BUFFER_BYTES <= ram_size {
            preferred
        } else if ram_size >= 0x1000 + LEGACY_SOUND_BUFFER_BYTES {
            ram_size - LEGACY_SOUND_BUFFER_BYTES
        } else {
            0
        }
    }

    fn init_legacy_sound_buffer(&mut self, sound_base: u32) {
        if sound_base == 0 {
            return;
        }
        for word in 0..LEGACY_SOUND_BUFFER_WORDS {
            let addr = sound_base + word * 2;
            self.write_byte(addr, 0x80);
            self.write_byte(addr + 1, 0x00);
        }
    }

    /// `BlockMove` fast path. Copies `count` bytes from `src` to
    /// `dst`, handling overlap correctly. When both ranges are fully
    /// inside RAM and no watchpoint is armed, uses `slice::copy_within`
    /// — one bounds check, memmove-grade throughput. Falls back to
    /// byte-at-a-time (preserving the overlap-handling order from
    /// Inside Macintosh II-44) when the fast path doesn't apply.
    pub fn block_move(&mut self, src: u32, dst: u32, count: u32) {
        // `Size` is a signed Macintosh LONGINT. A negative byte count is
        // invalid; treating its bit pattern as an unsigned length would let
        // a malformed guest call overwrite the entire emulated address
        // space. The ROM routine leaves the destination untouched for this
        // case, while the trap wrapper reports noErr in D0.
        if (count as i32) <= 0 {
            return;
        }
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed)
            && self.foreign_address_space.is_none()
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        #[cfg(not(debug_assertions))]
        let fast = self.foreign_address_space.is_none()
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        let count_usize = count as usize;
        let translated_src = self.range_translates_contiguously(src, count_usize);
        let translated_dst = self.range_translates_contiguously(dst, count_usize);
        let src_for_overlap = self.translate_guest_address(src);
        let dst_for_overlap = self.translate_guest_address(dst);
        let src = translated_src.unwrap_or(src);
        let dst = translated_dst.unwrap_or(dst);
        let src_end = (src as u64).saturating_add(count as u64);
        let dst_end = (dst as u64).saturating_add(count as u64);
        if (fast || self.only_write_probe_blocks_fast_path())
            && translated_src.is_some()
            && translated_dst.is_some()
            && !self.readonly_code_overlaps(dst, count)
            && src_end <= self.ram_size as u64
            && dst_end <= self.ram_size as u64
        {
            if !fast {
                self.record_write_probe_range(dst, count);
            }
            let ram_size_usize = self.ram_size as usize;
            if let Some(ram) = self.ram.slice_at_mut(0, ram_size_usize) {
                let src_range = (src as usize)..(src as usize + count_usize);
                ram.copy_within(src_range, dst as usize);
                return;
            }
        }
        // Fallback with explicit overlap handling (IM II-44).
        if dst_for_overlap > src_for_overlap
            && dst_for_overlap < src_for_overlap.saturating_add(count)
        {
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
        let synthetic_floor = screen_buffer_start.saturating_sub(SYNTHETIC_RESERVE_BYTES);
        let mut bus = Self {
            ram: RamStorage::Owned(vec![0; ram_size]),
            ram_size: ram_size as u32,
            addressing_32_bit: true,
            globals: LowMemGlobals::new(),
            heap_allocator: SharedClassicHeapAllocator::default(),
            synthetic_ptr: screen_buffer_start,
            synthetic_floor,
            readonly_code_ranges: Vec::new(),
            readonly_code_span: None,
            write_probe_original: None,
            write_probe_spare: WriteProbeJournal::default(),
            write_probe_invalid: false,
            write_probe_overflowed: false,
            write_probe_uncapped: false,
            foreign_address_space: None,
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
        let screen_row_bytes: u16 = 816;
        let screen_width: u16 = 800;
        let screen_height: u16 = 600;

        // ScrnBase ($0824) - pointer to screen buffer
        bus.write_long(0x0824, screen_base);
        bus.write_word(super::globals::addr::SCREEN_ROW, screen_row_bytes);

        // SoundBase ($0266) - pointer to the 370-word main sound buffer
        // used by the original free-form synthesizer. Keep it in the
        // display/sound hardware reservation just past the active 800x600
        // framebuffer: still outside the heap, but below Systemless's
        // top-of-RAM stack window. Direct Sound Driver clients such as
        // Crystal Quest clear this buffer themselves via
        // `MOVEA.L (SoundBase).W,A0`; if NIL, those writes land in low
        // memory and corrupt Ticks ($016A). Inside Macintosh Volume III,
        // III-21 and III-425; Volume IV, IV-247.
        use super::globals::addr;
        let sound_base = Self::legacy_sound_base_address(
            ram_size,
            screen_base,
            screen_row_bytes as u32 * screen_height as u32,
        );
        bus.write_long(addr::SOUND_BASE, sound_base);
        bus.init_legacy_sound_buffer(sound_base);

        // screenBits BitMap structure at $083C (14 bytes)
        // BitMap: baseAddr(4) + rowBytes(2) + bounds(8) = 14 bytes
        // Stored at $083C to avoid conflicting with mouse globals at $0828-$0833.
        // Reference: Executor docs/globals.cpp — $0828 is MTemp, $082C is MouseLocation
        bus.write_long(addr::SCREEN_BITS, screen_base); // baseAddr
        bus.write_word(addr::SCREEN_BITS + 4, screen_row_bytes); // rowBytes
        bus.write_word(addr::SCREEN_BITS + 6, 0); // bounds.top
        bus.write_word(addr::SCREEN_BITS + 8, 0); // bounds.left
        bus.write_word(addr::SCREEN_BITS + 10, screen_height); // bounds.bottom
        bus.write_word(addr::SCREEN_BITS + 12, screen_width); // bounds.right

        bus
    }

    pub(crate) fn configure_screen_depth(&mut self, depth: u16) {
        debug_assert!(matches!(depth, 1 | 2 | 4 | 8));
        let profile = crate::machine_profile::reference_machine_profile();
        let visible_row_bytes = (u32::from(profile.screen_width) * u32::from(depth)).div_ceil(8);
        let row_bytes = (visible_row_bytes / 16 + 1) * 16;
        self.write_word(super::globals::addr::SCREEN_ROW, row_bytes as u16);
        self.write_word(super::globals::addr::SCREEN_BITS + 4, row_bytes as u16);
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
        let synthetic_floor = screen_buffer_start.saturating_sub(SYNTHETIC_RESERVE_BYTES);
        Self {
            ram: RamStorage::External(ram_ptr, ram_size),
            ram_size: ram_size as u32,
            addressing_32_bit: true,
            globals,
            heap_allocator: SharedClassicHeapAllocator::default(),
            synthetic_ptr: screen_buffer_start,
            synthetic_floor,
            readonly_code_ranges: Vec::new(),
            readonly_code_span: None,
            write_probe_original: None,
            write_probe_spare: WriteProbeJournal::default(),
            write_probe_invalid: false,
            write_probe_overflowed: false,
            write_probe_uncapped: false,
            foreign_address_space: None,
        }
    }

    /// Attach one parked process's sparse mappings for a serialized 68k run.
    ///
    /// # Safety
    ///
    /// `memory` must remain alive and unmoved until
    /// [`Self::detach_guest_address_space`] is called, and no other adapter
    /// may access it during that interval.
    pub(crate) unsafe fn attach_guest_address_space(&mut self, memory: SharedGuestAddressSpace) {
        debug_assert!(self.foreign_address_space.is_none());
        self.foreign_address_space = Some(memory);
    }

    pub(crate) fn detach_guest_address_space(&mut self) {
        self.foreign_address_space = None;
    }

    /// Whether an address is mapped into a foreign guest address space's
    /// ordinary sparse regions (and not in a shared flat-RAM overlay).
    #[inline]
    pub(crate) fn is_foreign_ordinary_sparse_address(&self, address: u32) -> bool {
        let Some(foreign) = self.foreign_address_space else {
            return false;
        };
        // SAFETY: see `foreign_read_u8`.
        unsafe { foreign.is_ordinary_sparse_mapped(address) }
    }

    /// Return the end of a read-only process mapping that overlaps a proposed
    /// native heap allocation while the owning PowerPC process is parked.
    pub(crate) fn foreign_readonly_allocation_overlap_end(
        &self,
        address: u32,
        len: u32,
    ) -> Option<u32> {
        let foreign = self.foreign_address_space?;
        // SAFETY: see `foreign_read_u8`.
        unsafe { foreign.readonly_allocation_overlap_end(address, len) }
    }

    /// Write bytes exclusively through the parked process address space.
    pub(crate) fn write_foreign_bytes(&mut self, address: u32, bytes: &[u8]) -> Option<()> {
        let foreign = self.foreign_address_space?;
        // SAFETY: see `foreign_read_u8`.
        unsafe { foreign.write_bytes(address, bytes) }
    }

    /// Exclusively operate on the parked process address space.
    ///
    /// The attachment contract serializes the native adapter while the 68K
    /// bus is active, and the mutable bus borrow prevents overlapping access
    /// through this adapter.
    pub(crate) fn with_foreign_address_space<R>(
        &mut self,
        f: impl FnOnce(&mut GuestAddressSpace) -> R,
    ) -> Option<R> {
        let foreign = self.foreign_address_space?;
        // SAFETY: `attach_guest_address_space` requires an exclusive,
        // serialized interval and `self` is mutably borrowed for the closure.
        unsafe { foreign.with_mut(f) }
    }

    #[inline]
    fn foreign_read_u8(&self, address: u32) -> Option<u8> {
        let memory = self.foreign_address_space?;
        // SAFETY: attachment requires the runner to serialize the parked
        // native process with this bus.
        unsafe { memory.read_u8(address) }
    }

    #[inline]
    fn foreign_read_u16(&self, address: u32) -> Option<u16> {
        let memory = self.foreign_address_space?;
        // SAFETY: see `foreign_read_u8`.
        unsafe { memory.read_u16_be(address) }
    }

    #[inline]
    fn foreign_read_u32(&self, address: u32) -> Option<u32> {
        let memory = self.foreign_address_space?;
        // SAFETY: see `foreign_read_u8`.
        unsafe { memory.read_u32_be(address) }
    }

    #[inline]
    fn foreign_write_u8_if_mapped(&mut self, address: u32, value: u8) -> bool {
        let Some(memory) = self.foreign_address_space else {
            return false;
        };
        // A readable but non-writable mapping is still authoritative: ignore
        // the store instead of falling through into same-address flat RAM.
        // SAFETY: see `attach_guest_address_space`.
        unsafe {
            if memory.read_u8(address).is_none() {
                return false;
            }
            let _ = memory.write_u8(address, value);
        }
        true
    }

    #[inline]
    fn foreign_write_u16_if_mapped(&mut self, address: u32, value: u16) -> bool {
        let Some(memory) = self.foreign_address_space else {
            return false;
        };
        // SAFETY: see `attach_guest_address_space`.
        unsafe {
            if memory.read_u16_be(address).is_none() {
                return false;
            }
            let _ = memory.write_u16_be(address, value);
        }
        true
    }

    #[inline]
    fn foreign_write_u32_if_mapped(&mut self, address: u32, value: u32) -> bool {
        let Some(memory) = self.foreign_address_space else {
            return false;
        };
        // SAFETY: see `attach_guest_address_space`.
        unsafe {
            if memory.read_u32_be(address).is_none() {
                return false;
            }
            let _ = memory.write_u32_be(address, value);
        }
        true
    }

    /// Begin journaling original RAM bytes for an exact-state execution
    /// probe. Calling this again discards the previous incomplete probe.
    pub(crate) fn begin_write_probe(&mut self) {
        let mut journal = std::mem::take(&mut self.write_probe_spare);
        journal.clear();
        self.write_probe_original = Some(journal);
        self.write_probe_invalid = false;
        self.write_probe_overflowed = false;
        self.write_probe_uncapped = false;
    }

    /// Begin a write probe with no entry cap. Only for host-owned drawing
    /// performed while the guest is parked (no guest code runs, so the
    /// journal is bounded by what the drawing touches): the byte-identity
    /// answer from [`Self::finish_write_probe_unchanged`] must never be
    /// voided by the overflow guard sized for guest wait cycles.
    pub(crate) fn begin_uncapped_write_probe(&mut self) {
        self.begin_write_probe();
        self.write_probe_uncapped = true;
    }

    /// Discard an incomplete write probe and restore normal fast-memory use.
    pub(crate) fn cancel_write_probe(&mut self) {
        self.park_write_probe_journal();
        self.write_probe_invalid = false;
        self.write_probe_overflowed = false;
        self.write_probe_uncapped = false;
    }

    /// Detach the armed journal so writes made meanwhile -- host-owned
    /// drawing the guest never performed -- are neither recorded nor able
    /// to overflow it. The bus's fast paths come back while it is detached;
    /// `resume_write_probe` re-arms the very same journal. `None` when no
    /// journal is armed.
    pub(crate) fn suspend_write_probe(&mut self) -> Option<SuspendedWriteProbe> {
        self.write_probe_original.take().map(SuspendedWriteProbe)
    }

    pub(crate) fn resume_write_probe(&mut self, suspended: SuspendedWriteProbe) {
        self.write_probe_original = Some(suspended.0);
    }

    /// Report -- and clear -- whether the most recent probe's journal
    /// overflowed [`WRITE_PROBE_MAX_ENTRIES`] and was dropped.
    pub(crate) fn take_write_probe_overflow(&mut self) -> bool {
        std::mem::take(&mut self.write_probe_overflowed)
    }

    /// Finish a write probe and report whether guest RAM is byte-for-byte
    /// identical at every address written during the probe.
    pub(crate) fn finish_write_probe_unchanged(&mut self) -> bool {
        let Some(original) = self.write_probe_original.take() else {
            return false;
        };
        let unchanged = !self.write_probe_invalid
            && original
                .iter()
                .all(|(&word, &value)| self.ram.read_long_in_bounds(word as usize) == value);
        self.write_probe_spare = original;
        self.write_probe_invalid = false;
        self.write_probe_overflowed = false;
        self.write_probe_uncapped = false;
        unchanged
    }

    /// Close the journal, keeping its allocation for the next probe.
    fn park_write_probe_journal(&mut self) {
        if let Some(journal) = self.write_probe_original.take() {
            self.write_probe_spare = journal;
        }
    }

    /// True when an armed write probe is the only thing keeping a
    /// multi-byte write off its fast path: no framebuffer-write tracer and
    /// (in debug builds) no watchpoint, both of which need to observe each
    /// byte through `write_byte`. The probe only needs the words journaled
    /// before the write, which the caller does.
    #[inline]
    fn only_write_probe_blocks_fast_path(&self) -> bool {
        #[cfg(debug_assertions)]
        if WATCHPOINT_ARMED.load(Ordering::Relaxed) {
            return false;
        }
        self.write_probe_original.is_some() && fb_write_trace_range().is_none()
    }

    /// Journal the aligned words covering `len` bytes at the translated
    /// `address`. Must run before the write.
    #[inline]
    fn record_write_probe_range(&mut self, address: u32, len: u32) {
        if self.write_probe_original.is_none() || len == 0 {
            return;
        }
        let end = u64::from(address) + u64::from(len);
        if end > u64::from(self.ram_size) {
            self.write_probe_invalid = true;
            return;
        }
        let last = ((end - 1) as u32) & !3;
        let mut word = address & !3;
        loop {
            if u64::from(word) + 4 > u64::from(self.ram_size) {
                self.write_probe_invalid = true;
                return;
            }
            let original = self.ram.read_long_in_bounds(word as usize);
            let journal = self
                .write_probe_original
                .as_mut()
                .expect("write probe checked above");
            journal.entry(word).or_insert(original);
            if !self.write_probe_uncapped && journal.len() > WRITE_PROBE_MAX_ENTRIES {
                // Too much written for a wait cycle: void the probe now so
                // the fast paths (and fastmem) come back for the work in
                // progress.
                self.park_write_probe_journal();
                self.write_probe_overflowed = true;
                return;
            }
            if word == last {
                return;
            }
            word += 4;
        }
    }

    /// Allocate memory from heap.
    /// Reuses freed blocks via best-fit (smallest free block >= request),
    /// otherwise bump-allocates. Returns 0 on OOM; callers must set
    /// memFullErr.
    /// Reserve space at the start of the heap without returning it.
    /// Used to protect zone headers from being overwritten by alloc().
    /// Idempotent so callers can reserve before resources are loaded and
    /// later write the zone header during application initialization.
    pub fn reserve_heap(&mut self, size: u32) {
        let aligned = (size + 3) & !3;
        self.reserve_heap_until(0x200000 + aligned);
    }

    /// Advance the heap bump pointer past an absolute guest address.
    ///
    /// Loader-owned regions are written directly rather than allocated through
    /// the Memory Manager shim, so the runner uses this before materializing
    /// Toolbox heap objects that must not overlap the loaded application image.
    pub fn reserve_heap_until(&mut self, end_addr: u32) {
        self.heap_allocator.reserve_until(end_addr);
    }

    /// Prevent heap allocations from overlapping a direct-loaded guest range
    /// while preserving usable partition space before it.
    pub(crate) fn reserve_heap_range(&mut self, start_addr: u32, end_addr: u32) {
        self.heap_allocator.reserve_range(start_addr, end_addr);
    }

    pub fn alloc(&mut self, size: u32) -> u32 {
        self.heap_allocator.allocate(size, 4, self.synthetic_floor)
    }

    /// Allocate Systemless-owned memory without consuming or perturbing the
    /// guest Memory Manager heap. Synthetic callbacks and queue anchors live
    /// here because classic applications can depend on consecutive NewPtr
    /// results when building or relocating executable stubs.
    pub(crate) fn alloc_synthetic(&mut self, size: u32) -> u32 {
        let aligned = Self::allocation_bucket_size(size);
        let Some(ptr) = self.synthetic_ptr.checked_sub(aligned) else {
            return 0;
        };
        if ptr < self.synthetic_floor {
            eprintln!(
                "[ALLOC] Out of synthetic memory: requesting {} bytes, floor at ${:08X}, synthetic at ${:08X}",
                size, self.synthetic_floor, self.synthetic_ptr
            );
            return 0;
        }
        self.synthetic_ptr = ptr;
        self.fill_bytes(ptr, aligned, 0);
        ptr
    }

    pub(crate) fn protect_readonly_code(&mut self, address: u32, len: u32) {
        if len != 0 {
            let end = address.saturating_add(len);
            self.readonly_code_ranges.push((address, end));
            self.readonly_code_span = Some(match self.readonly_code_span {
                Some((lo, hi)) => (lo.min(address), hi.max(end)),
                None => (address, end),
            });
        }
    }

    pub(crate) fn write_readonly_code_word(&mut self, address: u32, value: u16) {
        if (address as u64) + 2 <= self.ram_size as u64 {
            self.ram.write_word_in_bounds(address as usize, value);
        }
    }

    fn readonly_code_overlaps(&self, address: u32, len: u32) -> bool {
        // Reject outside the bounding box first: this runs on every guest
        // write, and protected stubs occupy a small clustered region that
        // ordinary writes never touch.
        let Some((span_start, span_end)) = self.readonly_code_span else {
            return false;
        };
        let end = (address as u64).saturating_add(len as u64);
        if end <= span_start as u64 || address as u64 >= span_end as u64 {
            return false;
        }
        self.readonly_code_ranges
            .iter()
            .any(|&(start, stop)| (start as u64) < end && (stop as u64) > address as u64)
    }

    /// Allocate memory from the heap with a stronger start-address alignment.
    ///
    /// This keeps the same user-visible size and free-list behavior as
    /// [`Self::alloc`], but lets Toolbox managers request stable record
    /// placement without making every heap allocation pay that cost.
    pub fn alloc_aligned(&mut self, size: u32, alignment: u32) -> u32 {
        self.heap_allocator
            .allocate(size, alignment, self.synthetic_floor)
    }

    /// Return the allocated size for a given address, or None if unknown.
    pub fn get_alloc_size(&self, addr: u32) -> Option<u32> {
        self.heap_allocator.allocation_size(addr)
    }

    pub(crate) fn heap_bump_ptr(&self) -> u32 {
        self.heap_allocator.heap_bump_ptr()
    }

    /// Update the logical size of an existing allocation. Used by
    /// SetPtrSize / SetHandleSize for in-place resize. Caller is
    /// responsible for ensuring the new size fits within the original
    /// 4-byte-aligned capacity — see trap/memory.rs SetPtrSize.
    ///
    /// No-op for unknown addresses.
    pub fn set_alloc_size(&mut self, addr: u32, new_size: u32) {
        self.heap_allocator.set_allocation_size(addr, new_size);
    }

    /// Return a previously allocated block to the free list for reuse.
    /// Does nothing for null pointers or unknown addresses.
    pub fn free(&mut self, addr: u32) {
        self.heap_allocator.free(addr);
    }

    /// Return a read-only slice of contiguous RAM.
    /// Useful for bulk reads (e.g. framebuffer rendering) without per-byte
    /// method-call overhead.
    pub fn ram_slice(&self, start: u32, len: u32) -> &[u8] {
        let s = start as usize;
        let e = s + len as usize;
        match &self.ram {
            RamStorage::Owned(v) => {
                assert!(e <= v.len());
                &v[s..e]
            }
            RamStorage::Shared(v) => {
                assert!(e <= v.len());
                unsafe { std::slice::from_raw_parts(v.as_ptr().add(s), len as usize) }
            }
            RamStorage::External(ptr, max_len) => {
                assert!(e <= *max_len);
                unsafe { std::slice::from_raw_parts(ptr.add(s), len as usize) }
            }
        }
    }

    /// Copy a RAM range to another RAM range with one bounds/tracing gate.
    /// Falls back to byte writes when debug watchpoints or framebuffer-write
    /// tracing are active so diagnostics still observe each destination byte.
    #[inline]
    pub fn copy_ram_bytes(&mut self, src: u32, dst: u32, len: u32) -> bool {
        if self.foreign_address_space.is_some() {
            self.block_move(src, dst, len);
            return true;
        }
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed)
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none() && self.write_probe_original.is_none();
        let translated_src = self.range_translates_contiguously(src, len as usize);
        let translated_dst = self.range_translates_contiguously(dst, len as usize);
        let src = translated_src.unwrap_or(src);
        let dst = translated_dst.unwrap_or(dst);
        let src_end = (src as u64).saturating_add(len as u64);
        let dst_end = (dst as u64).saturating_add(len as u64);
        if translated_src.is_none()
            || translated_dst.is_none()
            || src_end > self.ram_size as u64
            || dst_end > self.ram_size as u64
        {
            return false;
        }
        if fast {
            self.ram
                .copy_bytes_in_bounds(src as usize, dst as usize, len as usize);
            return true;
        }
        if self.only_write_probe_blocks_fast_path() && !self.readonly_code_overlaps(dst, len) {
            self.record_write_probe_range(dst, len);
            self.ram
                .copy_bytes_in_bounds(src as usize, dst as usize, len as usize);
            return true;
        }
        for offset in 0..len {
            let byte = self.read_byte(src.wrapping_add(offset));
            self.write_byte(dst.wrapping_add(offset), byte);
        }
        true
    }

    /// Copy a RAM range through an 8-bit lookup table into another RAM range.
    /// Used by indexed blitters that need source-palette to destination-palette
    /// translation without allocating a scratch row.
    #[inline]
    pub fn copy_mapped_ram_bytes(&mut self, src: u32, dst: u32, len: u32, map: &[u8; 256]) -> bool {
        if self.foreign_address_space.is_some() {
            if dst > src && dst < src.saturating_add(len) {
                for offset in (0..len).rev() {
                    let value = map[self.read_byte(src.wrapping_add(offset)) as usize];
                    self.write_byte(dst.wrapping_add(offset), value);
                }
            } else {
                for offset in 0..len {
                    let value = map[self.read_byte(src.wrapping_add(offset)) as usize];
                    self.write_byte(dst.wrapping_add(offset), value);
                }
            }
            return true;
        }
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed)
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none() && self.write_probe_original.is_none();
        let translated_src = self.range_translates_contiguously(src, len as usize);
        let translated_dst = self.range_translates_contiguously(dst, len as usize);
        let src = translated_src.unwrap_or(src);
        let dst = translated_dst.unwrap_or(dst);
        let src_end = (src as u64).saturating_add(len as u64);
        let dst_end = (dst as u64).saturating_add(len as u64);
        if translated_src.is_none()
            || translated_dst.is_none()
            || src_end > self.ram_size as u64
            || dst_end > self.ram_size as u64
        {
            return false;
        }
        if fast {
            self.ram
                .copy_mapped_bytes_in_bounds(src as usize, dst as usize, len as usize, map);
            return true;
        }
        if self.only_write_probe_blocks_fast_path() && !self.readonly_code_overlaps(dst, len) {
            self.record_write_probe_range(dst, len);
            self.ram
                .copy_mapped_bytes_in_bounds(src as usize, dst as usize, len as usize, map);
            return true;
        }
        for offset in 0..len {
            let byte = map[self.read_byte(src.wrapping_add(offset)) as usize];
            self.write_byte(dst.wrapping_add(offset), byte);
        }
        true
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

    /// Return a stable shared view over an owned RAM subrange.
    ///
    /// The runner uses this to map selected system-scoped bytes into another
    /// CPU adapter without copying them. Externally wrapped RAM cannot safely
    /// extend its caller-provided lifetime and therefore returns `None`.
    pub(crate) fn shared_ram_region(&mut self, address: u32, len: u32) -> Option<SharedRamRegion> {
        let end = address.checked_add(len)?;
        if end > self.ram_size {
            return None;
        }
        let ram = match &mut self.ram {
            RamStorage::Owned(bytes) => {
                let shared = SharedRam(Rc::new(UnsafeCell::new(
                    std::mem::take(bytes).into_boxed_slice(),
                )));
                self.ram = RamStorage::Shared(shared.clone());
                shared
            }
            RamStorage::Shared(ram) => ram.clone(),
            RamStorage::External(_, _) => return None,
        };
        Some(SharedRamRegion {
            ram,
            offset: address as usize,
            len: len as usize,
        })
    }

    /// Return the complete system-owned synthetic reservation. CPU adapters
    /// may expose this as read-only code while retaining visibility of stubs
    /// allocated later in the process lifetime.
    pub(crate) fn shared_synthetic_reservation(&mut self) -> Option<(u32, SharedRamRegion)> {
        let (base, len) = self.synthetic_reservation_range()?;
        self.shared_ram_region(base, len)
            .map(|region| (base, region))
    }

    /// Return the runner-owned synthetic reservation without creating a
    /// shared-memory view. Native loaders use this range as an allocation
    /// exclusion before the runner attaches the live mapping at initialization.
    pub(crate) fn synthetic_reservation_range(&self) -> Option<(u32, u32)> {
        self.synthetic_floor
            .checked_add(SYNTHETIC_RESERVE_BYTES)
            .filter(|end| *end <= self.ram_size)
            .map(|_| (self.synthetic_floor, SYNTHETIC_RESERVE_BYTES))
    }

    /// Select the guest MMU address width. The default is 32-bit addressing.
    pub fn set_addressing_32_bit(&mut self, enabled: bool) {
        self.addressing_32_bit = enabled;
    }

    /// Whether guest memory accesses currently use all 32 address bits.
    pub fn addressing_32_bit(&self) -> bool {
        self.addressing_32_bit
    }

    #[inline]
    pub fn translate_guest_address(&self, address: u32) -> u32 {
        if self.addressing_32_bit {
            address
        } else {
            address & 0x00FF_FFFF
        }
    }

    #[inline]
    fn range_translates_contiguously(&self, address: u32, len: usize) -> Option<u32> {
        let translated = self.translate_guest_address(address);
        let address_space_end = if self.addressing_32_bit {
            u64::from(u32::MAX) + 1
        } else {
            0x0100_0000
        };
        ((translated as u64).saturating_add(len as u64) <= address_space_end).then_some(translated)
    }

    /// Raw window over guest RAM for the m68k fastmem path, or `None`
    /// while any per-access diagnostic (framebuffer-write tracer, memory
    /// read/write tracer, watchpoint) needs to observe individual bus
    /// accesses — fastmem reads/writes bypass those hooks entirely.
    pub(crate) fn fast_mem_window(&mut self) -> Option<(*mut u8, u32)> {
        if !self.addressing_32_bit
            || self.foreign_address_space.is_some()
            || fb_write_trace_range().is_some()
            || mem_read_trace_active()
            || mem_write_trace_active()
            || watchpoint_armed()
            || self.write_probe_original.is_some()
        {
            return None;
        }
        let ptr = match &mut self.ram {
            RamStorage::Owned(v) => v.as_mut_ptr(),
            RamStorage::Shared(_) => return None,
            RamStorage::External(ptr, _) => *ptr,
        };
        Some((ptr, self.ram_size))
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
        let address = self.translate_guest_address(address);
        let v = if let Some(value) = self.foreign_read_u8(address) {
            value
        } else if address < self.ram_size {
            self.ram.get_in_bounds(address as usize)
        } else if let Some(value) = Self::boot_rom_shadow_byte(address) {
            value
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
        let foreign_address = self.translate_guest_address(address);
        if let Some(value) = self.foreign_read_u16(foreign_address) {
            maybe_log_mem_read(address, 2, value as u32);
            return value;
        }
        if self.foreign_address_space.is_some() {
            let value = (u16::from(self.read_byte(address)) << 8)
                | u16::from(self.read_byte(address.wrapping_add(1)));
            maybe_log_mem_read(address, 2, value as u32);
            return value;
        }
        let translated = self.range_translates_contiguously(address, 2);
        let v = if translated.is_some_and(|address| (address as u64) + 2 <= self.ram_size as u64) {
            let address = translated.unwrap();
            self.ram.read_word_in_bounds(address as usize)
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
        let foreign_address = self.translate_guest_address(address);
        if let Some(value) = self.foreign_read_u32(foreign_address) {
            maybe_log_mem_read(address, 4, value);
            return value;
        }
        if self.foreign_address_space.is_some() {
            let value = (u32::from(self.read_word(address)) << 16)
                | u32::from(self.read_word(address.wrapping_add(2)));
            maybe_log_mem_read(address, 4, value);
            return value;
        }
        let translated = self.range_translates_contiguously(address, 4);
        let v = if translated.is_some_and(|address| (address as u64) + 4 <= self.ram_size as u64) {
            let address = translated.unwrap();
            self.ram.read_long_in_bounds(address as usize)
        } else {
            let hi = self.read_word(address) as u32;
            let lo = self.read_word(address.wrapping_add(2)) as u32;
            (hi << 16) | lo
        };
        maybe_log_mem_read(address, 4, v);
        v
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        let address = self.translate_guest_address(address);
        if self.foreign_write_u8_if_mapped(address, value) {
            maybe_log_mem_write(address, 1, value as u32);
            return;
        }
        if self.readonly_code_overlaps(address, 1) {
            return;
        }
        self.record_write_probe_range(address, 1);
        maybe_log_mem_write(address, 1, value as u32);

        // Optional release-mode FB-write tracer. Cheap when unset (one
        // atomic load + None branch). The range is read once and shared
        // with the disassembly companion below rather than fetched twice.
        let fb_trace = fb_write_trace_range();
        if fb_trace.is_some() {
            maybe_log_fb_write(address, value);
        }
        // Companion disassembly window: when both FB_WRITE_RANGE and
        // FB_WRITE_DISASM are set, dump the 8 instruction bytes at PC
        // alongside an m68k-disassembled mnemonic for each write that
        // falls in the watched range. Lets release-build pixel-
        // divergence investigations identify the 68k blit loop
        // responsible without a debug build.
        if let Some((start, end)) = fb_trace {
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
            self.ram.set_in_bounds(address as usize, value);
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
        let foreign_address = self.translate_guest_address(address);
        if self.foreign_write_u16_if_mapped(foreign_address, value) {
            maybe_log_mem_write(address, 2, value as u32);
            return;
        }
        let translated = self.range_translates_contiguously(address, 2);
        let protected_address = translated.unwrap_or(address);
        if self.readonly_code_overlaps(protected_address, 2) {
            return;
        }
        maybe_log_mem_write(address, 2, value as u32);

        // Fast path: watchpoint disarmed + tracer disabled + write fully in-bounds.
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed)
            && self.foreign_address_space.is_none()
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        #[cfg(not(debug_assertions))]
        let fast = self.foreign_address_space.is_none()
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        if let Some(address) =
            translated.filter(|&address| (address as u64) + 2 <= self.ram_size as u64)
        {
            if fast {
                self.ram.write_word_in_bounds(address as usize, value);
                return;
            }
            if self.only_write_probe_blocks_fast_path() {
                self.record_write_probe_range(address, 2);
                self.ram.write_word_in_bounds(address as usize, value);
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
        let foreign_address = self.translate_guest_address(address);
        if self.foreign_write_u32_if_mapped(foreign_address, value) {
            maybe_log_mem_write(address, 4, value);
            return;
        }
        let translated = self.range_translates_contiguously(address, 4);
        let protected_address = translated.unwrap_or(address);
        if self.readonly_code_overlaps(protected_address, 4) {
            return;
        }
        maybe_log_mem_write(address, 4, value);

        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed)
            && self.foreign_address_space.is_none()
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        #[cfg(not(debug_assertions))]
        let fast = self.foreign_address_space.is_none()
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        if let Some(address) =
            translated.filter(|&address| (address as u64) + 4 <= self.ram_size as u64)
        {
            if fast {
                self.ram.write_long_in_bounds(address as usize, value);
                return;
            }
            if self.only_write_probe_blocks_fast_path() {
                self.record_write_probe_range(address, 4);
                self.ram.write_long_in_bounds(address as usize, value);
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
        if self.foreign_address_space.is_some() {
            return (0..len)
                .map(|offset| self.read_byte(address.wrapping_add(offset as u32)))
                .collect();
        }
        let translated = self.range_translates_contiguously(address, len);
        let address = translated.unwrap_or(address);
        let end = (address as u64).saturating_add(len as u64);
        if translated.is_some() && end <= self.ram_size as u64 {
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
        if self.foreign_address_space.is_some() {
            for (offset, byte) in dst.iter_mut().enumerate() {
                *byte = self.read_byte(address.wrapping_add(offset as u32));
            }
            return;
        }
        let len = dst.len();
        let translated = self.range_translates_contiguously(address, len);
        let address = translated.unwrap_or(address);
        let end = (address as u64).saturating_add(len as u64);
        if translated.is_some() && end <= self.ram_size as u64 {
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
        if self.foreign_address_space.is_some() {
            for (offset, byte) in data.iter().copied().enumerate() {
                self.write_byte(address.wrapping_add(offset as u32), byte);
            }
            return;
        }
        let translated = self.range_translates_contiguously(address, data.len());
        let protected_address = translated.unwrap_or(address);
        if self.readonly_code_overlaps(protected_address, data.len() as u32) {
            return;
        }
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed)
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none() && self.write_probe_original.is_none();
        let address = translated.unwrap_or(address);
        let end = (address as u64).saturating_add(data.len() as u64);
        if translated.is_some() && end <= self.ram_size as u64 {
            if fast {
                self.ram.write_bytes_in_bounds(address as usize, data);
                return;
            }
            if self.only_write_probe_blocks_fast_path() {
                self.record_write_probe_range(address, data.len() as u32);
                self.ram.write_bytes_in_bounds(address as usize, data);
                return;
            }
        }
        for (i, &byte) in data.iter().enumerate() {
            self.write_byte(address.wrapping_add(i as u32), byte);
        }
    }

    #[inline]
    fn fill_zeros(&mut self, address: u32, len: u32) {
        if self.foreign_address_space.is_some() {
            for offset in 0..len {
                self.write_byte(address.wrapping_add(offset), 0);
            }
            return;
        }
        let translated = self.range_translates_contiguously(address, len as usize);
        let protected_address = translated.unwrap_or(address);
        if self.readonly_code_overlaps(protected_address, len) {
            return;
        }
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed)
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none() && self.write_probe_original.is_none();
        let address = translated.unwrap_or(address);
        let end = (address as u64).saturating_add(len as u64);
        if translated.is_some() && end <= self.ram_size as u64 {
            if fast {
                self.ram
                    .fill_zeros_in_bounds(address as usize, len as usize);
                return;
            }
            if self.only_write_probe_blocks_fast_path() {
                self.record_write_probe_range(address, len);
                self.ram
                    .fill_zeros_in_bounds(address as usize, len as usize);
                return;
            }
        }
        for i in 0..len {
            self.write_byte(address.wrapping_add(i), 0);
        }
    }

    /// Strided fill fast path: one translation and bounds check for the
    /// span from the first to the last byte written, then a stride loop
    /// over the RAM slice. Falls back to byte writes — which skip exactly
    /// the protected bytes and journal each write — when the span touches
    /// read-only code, a write probe is armed, or a tracer is active.
    #[inline]
    fn fill_bytes_strided(&mut self, address: u32, stride: u32, count: u32, value: u8) {
        if count == 0 {
            return;
        }
        if self.foreign_address_space.is_some() {
            for offset in 0..count {
                self.write_byte(address.wrapping_add(offset.wrapping_mul(stride)), value);
            }
            return;
        }
        let span = u64::from(stride) * u64::from(count - 1) + 1;
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed)
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none() && self.write_probe_original.is_none();
        if fast && span <= u64::from(u32::MAX) {
            if let Some(start) = self.range_translates_contiguously(address, span as usize) {
                let end = u64::from(start) + span;
                if end <= u64::from(self.ram_size)
                    && !self.readonly_code_overlaps(start, span as u32)
                {
                    if let Some(slice) = self.ram.slice_at_mut(start as usize, span as usize) {
                        let stride = stride as usize;
                        for i in 0..count as usize {
                            slice[i * stride] = value;
                        }
                        return;
                    }
                }
            }
        }
        for i in 0..count {
            self.write_byte(address.wrapping_add(i.wrapping_mul(stride)), value);
        }
    }

    #[inline]
    fn fill_bytes(&mut self, address: u32, len: u32, value: u8) {
        if self.foreign_address_space.is_some() {
            for offset in 0..len {
                self.write_byte(address.wrapping_add(offset), value);
            }
            return;
        }
        let translated = self.range_translates_contiguously(address, len as usize);
        let protected_address = translated.unwrap_or(address);
        if self.readonly_code_overlaps(protected_address, len) {
            return;
        }
        #[cfg(debug_assertions)]
        let fast = !WATCHPOINT_ARMED.load(Ordering::Relaxed)
            && fb_write_trace_range().is_none()
            && self.write_probe_original.is_none();
        #[cfg(not(debug_assertions))]
        let fast = fb_write_trace_range().is_none() && self.write_probe_original.is_none();
        let address = translated.unwrap_or(address);
        let end = (address as u64).saturating_add(len as u64);
        if translated.is_some() && end <= self.ram_size as u64 {
            if fast {
                self.ram
                    .fill_bytes_in_bounds(address as usize, len as usize, value);
                return;
            }
            if self.only_write_probe_blocks_fast_path() {
                self.record_write_probe_range(address, len);
                self.ram
                    .fill_bytes_in_bounds(address as usize, len as usize, value);
                return;
            }
        }
        for i in 0..len {
            self.write_byte(address.wrapping_add(i), value);
        }
    }

    fn ram_size(&self) -> u32 {
        self.ram_size
    }

    fn application_memory_limit(&self) -> u32 {
        self.synthetic_floor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_four_bit_mode_translates_scalar_and_bulk_accesses() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        bus.set_addressing_32_bit(false);

        bus.write_byte(0x0301_0000, 0x12);
        bus.write_word(0x0401_0002, 0x3456);
        bus.write_long(0xA501_0004, 0x789A_BCDE);
        bus.write_bytes(0x7F01_0008, &[1, 2, 3, 4]);

        assert_eq!(bus.read_byte(0x0001_0000), 0x12);
        assert_eq!(bus.read_word(0x0001_0002), 0x3456);
        assert_eq!(bus.read_long(0x0001_0004), 0x789A_BCDE);
        assert_eq!(bus.read_bytes(0x0001_0008, 4), [1, 2, 3, 4]);
        assert_eq!(bus.read_long(0xEE01_0004), 0x789A_BCDE);
    }

    #[test]
    fn twenty_four_bit_accesses_wrap_at_the_address_space_boundary() {
        let mut bus = MacMemoryBus::new(0x0100_0000);
        bus.set_addressing_32_bit(false);
        bus.write_byte(0x00FF_FFFF, 0x12);
        bus.write_byte(0, 0x34);

        assert_eq!(bus.read_word(0xABFF_FFFF), 0x1234);
        assert_eq!(bus.read_bytes(0xCDFF_FFFF, 2), [0x12, 0x34]);

        bus.write_word(0xEFFF_FFFF, 0x5678);
        assert_eq!(bus.read_byte(0x00FF_FFFF), 0x56);
        assert_eq!(bus.read_byte(0), 0x78);
    }

    #[test]
    fn thirty_two_bit_mode_preserves_tagged_addresses() {
        let mut bus = MacMemoryBus::new(2 * 1024 * 1024);
        bus.write_byte(0x0001_0000, 0x5A);

        assert_eq!(bus.read_byte(0x0301_0000), 0);
        assert_eq!(bus.read_byte(0x0001_0000), 0x5A);
        assert!(bus.fast_mem_window().is_some());

        bus.set_addressing_32_bit(false);
        assert_eq!(bus.read_byte(0x0301_0000), 0x5A);
        assert!(bus.fast_mem_window().is_none());
    }

    #[test]
    fn new_bus_publishes_default_screen_row_bytes() {
        let bus = MacMemoryBus::new(1024);
        assert_eq!(bus.read_word(crate::memory::globals::addr::SCREEN_ROW), 816);
    }

    #[test]
    fn boot_rom_shadow_exposes_witnessed_vector_zero_word() {
        let bus = MacMemoryBus::new(1024);

        assert_eq!(bus.read_byte(0x4081_0006), 0x03);
        assert_eq!(bus.read_byte(0x4081_0007), 0x72);
        assert_eq!(bus.read_word(0x4081_0006), 0x0372);
        assert_eq!(bus.read_byte(0x4081_0005), 0);
        assert_eq!(bus.read_byte(0x4081_0008), 0);
    }

    #[test]
    fn boot_rom_shadow_ignores_writes() {
        let mut bus = MacMemoryBus::new(1024);

        bus.write_word(0x4081_0006, 0xA55A);

        assert_eq!(bus.read_word(0x4081_0006), 0x0372);
    }

    #[test]
    fn write_probe_accepts_temporary_writes_that_restore_original_bytes() {
        let mut bus = MacMemoryBus::new(1024);
        bus.write_long(0x100, 0x1122_3344);

        bus.begin_write_probe();
        assert!(bus.fast_mem_window().is_none());
        bus.write_word(0x100, 0xAABB);
        bus.write_byte(0x102, 0xCC);
        bus.write_long(0x100, 0x1122_3344);

        assert!(bus.finish_write_probe_unchanged());
        assert!(bus.fast_mem_window().is_some());
    }

    #[test]
    fn write_probe_rejects_a_changed_final_byte() {
        let mut bus = MacMemoryBus::new(1024);
        bus.write_long(0x100, 0x1122_3344);

        bus.begin_write_probe();
        bus.write_byte(0x102, 0xCC);

        assert!(!bus.finish_write_probe_unchanged());
    }

    #[test]
    fn armed_probe_journals_every_write_size_and_bulk_path() {
        let mut bus = MacMemoryBus::new(4096);
        bus.fill_bytes(0x100, 0x200, 0x11);
        // A source span the copies read from, distinct from what they overwrite.
        for (i, address) in (0x1F0u32..0x200).enumerate() {
            bus.write_byte(address, 0x80 + i as u8);
        }
        type Case = (&'static str, Box<dyn Fn(&mut MacMemoryBus)>);
        let cases: Vec<Case> = vec![
            ("byte", Box::new(|b| b.write_byte(0x103, 0x22))),
            ("word", Box::new(|b| b.write_word(0x106, 0x2233))),
            ("long", Box::new(|b| b.write_long(0x10A, 0x2233_4455))),
            (
                "write_bytes",
                Box::new(|b| b.write_bytes(0x121, &[1, 2, 3, 4, 5])),
            ),
            ("fill_bytes", Box::new(|b| b.fill_bytes(0x141, 7, 0x33))),
            ("fill_zeros", Box::new(|b| b.fill_zeros(0x161, 3))),
            ("block_move", Box::new(|b| b.block_move(0x1F0, 0x181, 9))),
            (
                "copy_ram_bytes",
                Box::new(|b| {
                    assert!(b.copy_ram_bytes(0x1F0, 0x1A1, 6));
                }),
            ),
            (
                "copy_mapped",
                Box::new(|b| {
                    let mut map = [0u8; 256];
                    map[0x11] = 0x44;
                    assert!(b.copy_mapped_ram_bytes(0x100, 0x1C1, 6, &map));
                }),
            ),
        ];
        for (name, write) in &cases {
            let before = bus.read_bytes(0x100, 0x200);
            bus.begin_write_probe();
            write(&mut bus);
            assert!(!bus.finish_write_probe_unchanged(), "{name}: change seen");
            bus.write_bytes(0x100, &before);
            bus.begin_write_probe();
            write(&mut bus);
            bus.write_bytes(0x100, &before);
            assert!(
                bus.finish_write_probe_unchanged(),
                "{name}: restore accepted"
            );
        }
    }

    #[test]
    fn armed_probe_fast_paths_match_the_byte_path() {
        let mut fast = MacMemoryBus::new(4096);
        let mut slow = MacMemoryBus::new(4096);
        for bus in [&mut fast, &mut slow] {
            bus.fill_bytes(0x100, 0x300, 0x11);
            for (i, address) in (0x200u32..0x210).enumerate() {
                bus.write_byte(address, i as u8);
            }
            bus.protect_readonly_code(0x342, 1);
            bus.begin_write_probe();
        }
        // Multi-byte and bulk writes on the armed-probe fast paths...
        fast.write_word(0x120, 0xCAFE);
        fast.write_long(0x124, 0xDEAD_BEEF);
        fast.write_bytes(0x131, &[9, 8, 7]);
        fast.fill_bytes(0x141, 5, 0x55);
        fast.fill_zeros(0x151, 2);
        fast.block_move(0x200, 0x204, 8); // overlapping, forward
        fast.block_move(0x204, 0x201, 8); // overlapping, backward
        assert!(fast.copy_ram_bytes(0x200, 0x220, 16));
        let mut map = [0u8; 256];
        for (i, entry) in map.iter_mut().enumerate() {
            *entry = (i as u8).wrapping_mul(3);
        }
        assert!(fast.copy_mapped_ram_bytes(0x200, 0x240, 16, &map));
        fast.write_long(0x340, 0x0102_0304); // crosses read-only: skipped whole
                                             // ...versus the byte-at-a-time reference on the other bus.
        for (address, byte) in [
            (0x120u32, 0xCAu8),
            (0x121, 0xFE),
            (0x124, 0xDE),
            (0x125, 0xAD),
            (0x126, 0xBE),
            (0x127, 0xEF),
            (0x131, 9),
            (0x132, 8),
            (0x133, 7),
        ] {
            slow.write_byte(address, byte);
        }
        for address in 0x141..0x146 {
            slow.write_byte(address, 0x55);
        }
        for address in 0x151..0x153 {
            slow.write_byte(address, 0);
        }
        let snapshot: Vec<u8> = (0..8).map(|i| slow.read_byte(0x200 + i)).collect();
        for (i, byte) in snapshot.iter().enumerate() {
            slow.write_byte(0x204 + i as u32, *byte);
        }
        let snapshot: Vec<u8> = (0..8).map(|i| slow.read_byte(0x204 + i)).collect();
        for (i, byte) in snapshot.iter().enumerate() {
            slow.write_byte(0x201 + i as u32, *byte);
        }
        for i in 0..16u32 {
            let byte = slow.read_byte(0x200 + i);
            slow.write_byte(0x220 + i, byte);
            slow.write_byte(0x240 + i, map[byte as usize]);
        }
        assert_eq!(fast.read_bytes(0x100, 0x300), slow.read_bytes(0x100, 0x300));
        assert_eq!(fast.read_long(0x340), 0x1111_1111, "read-only untouched");
        assert!(!fast.finish_write_probe_unchanged());
        assert!(!slow.finish_write_probe_unchanged());
    }

    #[test]
    fn write_probe_journal_is_reused_without_stale_entries() {
        let mut bus = MacMemoryBus::new(1024);
        bus.write_long(0x100, 0x1122_3344);

        // First probe records $102 as changed; its journal is kept as the
        // spare afterwards.
        bus.begin_write_probe();
        bus.write_byte(0x102, 0xCC);
        assert!(!bus.finish_write_probe_unchanged());

        // The reused journal must start empty: a probe that writes nothing
        // is unchanged even though $102 now differs from its old original.
        bus.begin_write_probe();
        assert!(bus.finish_write_probe_unchanged());

        // And a cancelled probe hands the journal back too.
        bus.begin_write_probe();
        bus.write_byte(0x103, 0xDD);
        bus.cancel_write_probe();
        bus.begin_write_probe();
        bus.write_byte(0x103, 0xDD);
        assert!(bus.finish_write_probe_unchanged());
    }

    #[test]
    fn fill_bytes_strided_writes_only_the_column() {
        let mut bus = MacMemoryBus::new(4096);
        bus.fill_bytes(0x100, 0x100, 0x11);
        bus.fill_bytes_strided(0x105, 16, 8, 0xEE);
        for i in 0..0x100u32 {
            let expected = if (5..5 + 8 * 16).contains(&i) && (i - 5) % 16 == 0 {
                0xEE
            } else {
                0x11
            };
            assert_eq!(bus.read_byte(0x100 + i), expected, "byte {i:#x}");
        }
        bus.fill_bytes_strided(0x1F0, 16, 0, 0xAA);
        assert_eq!(bus.read_byte(0x1F0), 0x11, "count 0 writes nothing");
    }

    #[test]
    fn fill_bytes_strided_skips_read_only_code_like_byte_writes() {
        let mut bus = MacMemoryBus::new(4096);
        bus.fill_bytes(0x100, 0x100, 0x11);
        bus.protect_readonly_code(0x125, 1); // inside the span, on the column
        bus.protect_readonly_code(0x131, 1); // inside the span, off the column
        bus.fill_bytes_strided(0x105, 16, 8, 0xEE);
        assert_eq!(bus.read_byte(0x125), 0x11, "protected column byte kept");
        assert_eq!(bus.read_byte(0x115), 0xEE);
        assert_eq!(bus.read_byte(0x135), 0xEE, "bytes past it still written");
        assert_eq!(bus.read_byte(0x131), 0x11);
    }

    #[test]
    fn fill_bytes_strided_is_journaled_by_a_write_probe() {
        let mut bus = MacMemoryBus::new(4096);
        bus.fill_bytes(0x100, 0x100, 0x11);
        bus.begin_write_probe();
        bus.fill_bytes_strided(0x105, 16, 4, 0xEE);
        assert!(!bus.finish_write_probe_unchanged(), "changed bytes seen");
        bus.begin_write_probe();
        bus.fill_bytes_strided(0x105, 16, 4, 0xEE);
        assert!(bus.finish_write_probe_unchanged(), "same bytes: unchanged");
    }

    #[test]
    fn suspended_write_probe_ignores_writes_and_rearms_intact() {
        // A host-sized burst (more units than WRITE_PROBE_MAX_ENTRIES)
        // overflows an armed journal; made while the journal is suspended it
        // is neither recorded nor able to overflow, and the re-armed journal
        // still catches the next write.
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let base = 0x0008_0000u32;
        let burst = (WRITE_PROBE_MAX_ENTRIES as u32 + 64) * 4;

        bus.begin_write_probe();
        bus.fill_bytes(base, burst, 0xAA);
        assert!(bus.take_write_probe_overflow());
        bus.cancel_write_probe();
        bus.fill_bytes(base, burst, 0x00);

        bus.begin_write_probe();
        let suspended = bus.suspend_write_probe().expect("journal armed");
        assert!(bus.suspend_write_probe().is_none());
        bus.fill_bytes(base, burst, 0xAA);
        bus.resume_write_probe(suspended);
        assert!(!bus.take_write_probe_overflow());
        bus.write_byte(base + 8, 0x01);
        assert!(!bus.finish_write_probe_unchanged());

        bus.begin_write_probe();
        let suspended = bus.suspend_write_probe().expect("journal armed");
        bus.fill_bytes(base, burst, 0x55);
        bus.resume_write_probe(suspended);
        assert!(bus.finish_write_probe_unchanged());
        assert!(bus.suspend_write_probe().is_none());
    }

    #[test]
    fn write_probe_overflow_voids_the_journal_and_restores_fast_paths() {
        let mut bus = MacMemoryBus::new(64 * 1024);

        bus.begin_write_probe();
        assert!(bus.fast_mem_window().is_none());
        // Restore-perfect writes to as many distinct addresses as the cap
        // admits keep the probe alive and verifiable...
        for word in 0..WRITE_PROBE_MAX_ENTRIES as u32 {
            bus.write_byte(0x1000 + word * 4, 0);
        }
        assert!(bus.fast_mem_window().is_none());
        assert!(!bus.take_write_probe_overflow());
        // ...one more distinct address is more than a wait cycle writes:
        // the journal is dropped on the spot, the fast paths (and fastmem)
        // return, and the probe can no longer verify.
        bus.write_byte(0x1000 + WRITE_PROBE_MAX_ENTRIES as u32 * 4, 0);
        assert!(bus.fast_mem_window().is_some());
        assert!(!bus.finish_write_probe_unchanged());
        assert!(bus.take_write_probe_overflow());
        assert!(
            !bus.take_write_probe_overflow(),
            "the overflow verdict is consumed once"
        );

        // Rewriting one address many times is one journal entry, not many.
        bus.begin_write_probe();
        for _ in 0..(4 * WRITE_PROBE_MAX_ENTRIES) {
            bus.write_long(0x2000, 0x1234_5678);
        }
        assert!(bus.fast_mem_window().is_none());
        bus.write_long(0x2000, 0);
        assert!(bus.finish_write_probe_unchanged());
        assert!(!bus.take_write_probe_overflow());
    }

    #[test]
    fn write_probe_observes_bulk_and_copy_fast_paths() {
        let mut bus = MacMemoryBus::new(1024);
        bus.write_bytes(0x100, &[1, 2, 3, 4]);
        bus.write_bytes(0x200, &[5, 6, 7, 8]);

        bus.begin_write_probe();
        bus.write_bytes(0x100, &[9, 2, 3, 4]);
        assert!(bus.copy_ram_bytes(0x200, 0x204, 4));

        assert!(!bus.finish_write_probe_unchanged());
    }

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

    #[test]
    fn synthetic_allocations_do_not_perturb_guest_heap_addresses() {
        let mut bus = MacMemoryBus::new(8 * 1024 * 1024);

        let first_guest = bus.alloc(64);
        let synthetic = bus.alloc_synthetic(22);
        let second_guest = bus.alloc(64);

        assert_eq!(second_guest, first_guest + 64);
        assert!(synthetic > second_guest);
        assert_eq!(bus.read_bytes(synthetic, 24), vec![0; 24]);
        assert_eq!(bus.get_alloc_size(synthetic), None);
    }

    #[test]
    fn synthetic_reservation_has_a_stable_guest_memory_boundary() {
        let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
        let application_limit = bus.application_memory_limit();
        assert_eq!(
            bus.synthetic_reservation_range(),
            Some((application_limit, SYNTHETIC_RESERVE_BYTES))
        );

        bus.reserve_heap_until(application_limit - 4);
        assert_eq!(
            bus.alloc(4),
            0,
            "guest allocations must stop at the reservation"
        );

        let whole_reservation = bus.alloc_synthetic(SYNTHETIC_RESERVE_BYTES);
        assert_eq!(whole_reservation, application_limit);
        assert_eq!(
            bus.alloc_synthetic(4),
            0,
            "synthetic allocations must not escape their reservation"
        );

        let tiny_bus = MacMemoryBus::new((SYNTHETIC_RESERVE_BYTES / 2) as usize);
        assert_eq!(tiny_bus.synthetic_reservation_range(), None);
    }

    #[test]
    fn tiny_allocations_do_not_consume_large_free_blocks() {
        let mut bus = MacMemoryBus::new(8 * 1024 * 1024);

        let large = bus.alloc(175_414);
        assert_ne!(large, 0);
        bus.free(large);

        let tiny = bus.alloc(4);
        assert_ne!(
            tiny, large,
            "tiny allocations should not consume large resource-sized free blocks"
        );

        let large_again = bus.alloc(175_414);
        assert_eq!(
            large_again, large,
            "the original large block should remain available for a matching request"
        );
    }

    #[test]
    fn alloc_aligned_skips_to_requested_boundary() {
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);

        let skew = bus.alloc(5);
        assert_eq!(skew, 0x200000);

        let aligned = bus.alloc_aligned(170, 256);
        assert_eq!(
            aligned & 0xFF,
            0,
            "aligned allocation should start on the requested boundary"
        );
        assert_eq!(
            bus.get_alloc_size(aligned),
            Some(170),
            "logical size remains the caller-requested size"
        );

        let next = bus.alloc(4);
        assert_eq!(
            next,
            aligned + MacMemoryBus::allocation_bucket_size(170),
            "only the leading alignment gap is skipped"
        );
    }

    #[test]
    fn alloc_aligned_reuses_aligned_free_blocks() {
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);

        let aligned = bus.alloc_aligned(170, 256);
        let skewed = bus.alloc(170);
        assert_eq!(aligned & 0xFF, 0);
        assert_ne!(skewed & 0xFF, 0);

        bus.free(skewed);
        bus.free(aligned);

        let reused = bus.alloc_aligned(170, 256);
        assert_eq!(
            reused, aligned,
            "aligned allocation should prefer an aligned free block over a skewed one"
        );
    }

    #[test]
    fn reserve_heap_is_idempotent_start_of_heap_guard() {
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);

        bus.reserve_heap(64);
        let first = bus.alloc(12);
        bus.reserve_heap(64);
        let second = bus.alloc(12);

        assert_eq!(
            first,
            0x200000 + 64,
            "first allocation after zone-header reservation must skip the header"
        );
        assert_eq!(
            second,
            first + 12,
            "re-reserving the same zone-header range must not create a second gap"
        );
    }

    #[test]
    fn reserved_heap_range_preserves_space_before_direct_loaded_image() {
        let mut bus = MacMemoryBus::new(16 * 1024 * 1024);
        let image_start = 0x0080_0000;
        let image_end = 0x0090_0000;
        bus.reserve_heap(64);
        bus.reserve_heap_range(image_start, image_end);

        let lower_start = bus.alloc(image_start - (0x0020_0000 + 64));
        assert_eq!(lower_start, 0x0020_0000 + 64);
        let above_image = bus.alloc(4);

        assert_eq!(above_image, image_end);
        assert_eq!(bus.get_alloc_size(above_image), Some(4));
    }

    #[test]
    fn new_initializes_legacy_sound_base_buffer() {
        let bus = MacMemoryBus::new(4 * 1024 * 1024);
        let sound_base = bus.read_long(crate::memory::globals::addr::SOUND_BASE);

        assert_eq!(
            sound_base, 0x003F_7880,
            "SoundBase should sit just past the active framebuffer in the reserved hardware-buffer area"
        );
        assert!(
            sound_base + LEGACY_SOUND_BUFFER_BYTES <= bus.ram_size(),
            "the full 370-word sound buffer must be inside RAM"
        );
        assert_eq!(
            bus.read_byte(sound_base),
            0x80,
            "legacy sound high bytes should start at neutral amplitude"
        );
        assert_eq!(
            bus.read_byte(sound_base + 1),
            0,
            "legacy sound low bytes overlap disk-speed data and should start clear"
        );
        assert_eq!(
            bus.read_byte(sound_base + LEGACY_SOUND_BUFFER_BYTES - 2),
            0x80,
            "last legacy sound high byte"
        );
        assert_eq!(
            bus.read_byte(sound_base + LEGACY_SOUND_BUFFER_BYTES - 1),
            0,
            "last legacy sound low byte"
        );
    }

    #[test]
    fn writes_through_legacy_sound_base_do_not_corrupt_ticks() {
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let sound_base = bus.read_long(crate::memory::globals::addr::SOUND_BASE);
        bus.write_long(crate::memory::globals::addr::TICKS, 1234);

        // Mirrors the classic free-form Sound Driver pattern Crystal Quest
        // uses: write the high byte of each 370-word sample slot directly
        // through the SoundBase low-memory pointer.
        for offset in (0..LEGACY_SOUND_BUFFER_BYTES).step_by(2) {
            bus.write_byte(sound_base + offset, 0x80);
        }

        assert_eq!(
            bus.read_long(crate::memory::globals::addr::TICKS),
            1234,
            "SoundBase must never point at low memory; direct sound-buffer clears must not wrap Ticks"
        );
        assert_eq!(bus.read_byte(sound_base), 0x80);
        assert_eq!(
            bus.read_byte(sound_base + LEGACY_SOUND_BUFFER_BYTES - 2),
            0x80
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
    fn readonly_code_protection_survives_the_bounding_box_fast_path() {
        // Every guest write consults the protection list through a bounding
        // box, so verify the fast reject cannot let a protected write
        // through and cannot block an ordinary one -- including with
        // several disjoint ranges, where the box spans the gap between
        // them.
        let mut bus = MacMemoryBus::new(64 * 1024);
        bus.write_byte(0x2000, 0x11);
        bus.write_byte(0x4000, 0x22);
        bus.write_byte(0x3000, 0x33); // inside the future span, unprotected
        bus.protect_readonly_code(0x2000, 2);
        bus.protect_readonly_code(0x4000, 2);

        // Protected: writes are dropped at every width.
        bus.write_byte(0x2000, 0xFF);
        bus.write_word(0x4000, 0xFFFF);
        bus.write_long(0x2000, 0xFFFF_FFFF);
        assert_eq!(bus.read_byte(0x2000), 0x11, "protected byte is unchanged");
        assert_eq!(bus.read_byte(0x4000), 0x22, "protected byte is unchanged");

        // Inside the bounding box but between the ranges: still writable.
        bus.write_byte(0x3000, 0x44);
        assert_eq!(bus.read_byte(0x3000), 0x44, "the gap stays writable");

        // Outside the bounding box entirely: the fast-reject path.
        bus.write_byte(0x0100, 0x55);
        bus.write_byte(0x8000, 0x66);
        assert_eq!(bus.read_byte(0x0100), 0x55, "below the span is writable");
        assert_eq!(bus.read_byte(0x8000), 0x66, "above the span is writable");
    }

    #[test]
    fn copy_ram_bytes_handles_overlap_and_bounds() {
        let mut bus = MacMemoryBus::new(64 * 1024);
        for i in 0..16u32 {
            bus.write_byte(0x1000 + i, i as u8);
        }

        assert!(bus.copy_ram_bytes(0x1000, 0x1004, 8));
        assert_eq!(
            bus.read_bytes(0x1000, 12),
            vec![0, 1, 2, 3, 0, 1, 2, 3, 4, 5, 6, 7],
            "RAM copy should match memmove semantics for overlapping ranges"
        );

        bus.write_byte(0x0FFF, 0xAA);
        assert!(!bus.copy_ram_bytes(0x0FFF, 0xFFFF, 2));
        assert_eq!(
            bus.read_byte(0x0FFF),
            0xAA,
            "out-of-bounds copy should report failure before writing"
        );
    }

    #[test]
    fn copy_mapped_ram_bytes_applies_lookup_table() {
        let mut bus = MacMemoryBus::new(64 * 1024);
        bus.write_bytes(0x2000, &[1, 2, 3, 4]);
        bus.write_bytes(0x3000, &[0xEE; 4]);
        let mut map = [0u8; 256];
        for (index, slot) in map.iter_mut().enumerate() {
            *slot = 255u8.wrapping_sub(index as u8);
        }

        assert!(bus.copy_mapped_ram_bytes(0x2000, 0x3000, 4, &map));
        assert_eq!(bus.read_bytes(0x3000, 4), vec![254, 253, 252, 251]);
        assert!(!bus.copy_mapped_ram_bytes(0x2000, 0xFFFF, 2, &map));
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

    #[test]
    fn bus_detects_foreign_ordinary_sparse_addresses_only_when_attached() {
        use crate::memory::GuestAddressSpace;

        let mut bus = MacMemoryBus::new(64 * 1024);
        assert!(!bus.is_foreign_ordinary_sparse_address(0x2000));

        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x2000, vec![0; 0x100]);

        let shared_region = bus.shared_ram_region(0, 0x1000).unwrap();
        unsafe {
            memory.add_shared_region(0x0000, shared_region);
        }

        let shared = unsafe { memory.shared_view() };
        unsafe {
            bus.attach_guest_address_space(shared);
        }

        assert!(!bus.is_foreign_ordinary_sparse_address(0x0500));
        assert!(bus.is_foreign_ordinary_sparse_address(0x2050));
        assert!(!bus.is_foreign_ordinary_sparse_address(0x9000));

        bus.detach_guest_address_space();
        assert!(!bus.is_foreign_ordinary_sparse_address(0x2050));
    }
}
