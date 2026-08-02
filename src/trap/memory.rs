//! Memory Manager trap handlers.

use crate::cpu::{CpuOps, Register};
use crate::machine_profile::REFERENCE_MACHINE_PROFILE;
use crate::memory::{globals::addr, MacMemoryBus, MemoryBus};
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::OnceLock;

static TRACE_MEMORY: OnceLock<bool> = OnceLock::new();
static TRACE_VIDEO_DRIVER: OnceLock<bool> = OnceLock::new();
static TRACE_ENTROPY: OnceLock<bool> = OnceLock::new();
static TRACE_VBL: OnceLock<bool> = OnceLock::new();

/// `VBLQueue`, the 10-byte low-memory `QHdr` for the system VBL queue.
/// Universal Interfaces 3.4 LowMem.h; Processes 1994, p. 4-28.
const VBL_QUEUE_HEADER: u32 = 0x0160;

fn trace_memory_enabled() -> bool {
    *TRACE_MEMORY.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_MEMORY").is_some())
}

fn trace_sound_enabled() -> bool {
    std::env::var_os("SYSTEMLESS_TRACE_SOUND").is_some()
}

fn format_ostype(res_type: [u8; 4]) -> String {
    res_type
        .into_iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '.'
            }
        })
        .collect()
}

fn trace_vbl_enabled() -> bool {
    *TRACE_VBL.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_VBL").is_some())
}

/// Estimate free application heap from ApplLimit + HeapEnd low-mem globals.
/// See `crate::memory::app_heap_free_bytes` for the partition-aware
/// compatibility-floor rule shared with Process Manager reporting.
fn free_heap_estimate(bus: &MacMemoryBus) -> u32 {
    crate::memory::app_heap_free_bytes(bus)
}

fn reconcile_application_zone_limit(bus: &mut MacMemoryBus, old_limit: u32, new_limit: u32) {
    if old_limit == new_limit {
        return;
    }

    let app_zone = bus.read_long(addr::APP_L_ZONE);
    if app_zone == 0 || new_limit <= app_zone {
        return;
    }

    let bk_lim = bus.read_long(app_zone);
    if bk_lim != old_limit {
        return;
    }

    let zcb_free = bus.read_long(app_zone + 12);
    let adjusted_free = if new_limit > old_limit {
        zcb_free.saturating_add(new_limit - old_limit)
    } else {
        zcb_free.saturating_sub(old_limit - new_limit)
    };
    let zone_size = new_limit.saturating_sub(app_zone);

    bus.write_long(app_zone, new_limit); // bkLim
    bus.write_long(app_zone + 12, adjusted_free.min(zone_size)); // zcbFree
}

/// Keep the guest-visible application zone large enough to contain a
/// successful application-heap allocation.
///
/// Systemless eagerly materializes loaded resources in the same backing heap
/// used by Memory Manager traps. That can advance the backing allocator past a
/// SIZE-resource-derived `ApplLimit` before a later `NewPtr` call succeeds.
/// Classic clients validate pointers against the current zone's `[zone,
/// bkLim)` span, so returning a pointer above stale `bkLim` makes a successful
/// Memory Manager allocation appear invalid. Inside Macintosh: Memory (1992),
/// pp. 1-20..1-21 defines `bkLim` as the address immediately beyond the zone.
///
/// The newly exposed span has already been consumed by backing allocations,
/// so this deliberately does not add it to `zcbFree`.
fn include_application_allocation_in_zone(bus: &mut MacMemoryBus, ptr: u32, size: u32) {
    if ptr == 0 {
        return;
    }
    let app_zone = bus.read_long(addr::APP_L_ZONE);
    if app_zone == 0 || ptr < app_zone {
        return;
    }
    let allocation_end = ptr.saturating_add(MacMemoryBus::allocation_bucket_size(size));
    let old_bk_lim = bus.read_long(app_zone);
    if allocation_end <= old_bk_lim {
        return;
    }

    bus.write_long(app_zone, allocation_end); // bkLim

    let old_appl_limit = bus.read_long(addr::APPL_LIMIT);
    if allocation_end > old_appl_limit {
        bus.write_long(addr::APPL_LIMIT, allocation_end);
        if bus.read_long(addr::BUF_PTR) == old_appl_limit {
            bus.write_long(addr::BUF_PTR, allocation_end);
        }
    }

    let heap_end = bus.read_long(addr::HEAP_END);
    if allocation_end > heap_end {
        bus.write_long(addr::HEAP_END, allocation_end);
    }
}

fn init_zone_header(
    bus: &mut MacMemoryBus,
    start: u32,
    limit: u32,
    more_masters_raw: u16,
    grow_zone: u32,
) {
    // Zone record layout per Inside Macintosh Volume II (1985), p. II-22.
    // Only the caller-observable header fields are initialized here.
    let more_masters = i16::from_be_bytes(more_masters_raw.to_be_bytes()).max(0) as u32;
    let free_bytes = limit
        .saturating_sub(start)
        .saturating_sub(72 + (4 * more_masters));
    let first_master_ptr = if more_masters == 0 {
        0
    } else {
        start.wrapping_add(60)
    };

    bus.write_long(start, limit); // bkLim
    bus.write_long(start + 4, 0); // purgePtr
    bus.write_long(start + 8, first_master_ptr); // hFstFree
    bus.write_long(start + 12, free_bytes); // zcbFree
    bus.write_long(start + 16, grow_zone); // gzProc
    bus.write_word(start + 20, more_masters_raw); // moreMast
    bus.write_word(start + 22, 0); // flags
    bus.write_word(start + 24, 0); // cntRel
    bus.write_word(start + 26, 0); // maxRel
    bus.write_word(start + 28, 0); // cntNRel
    bus.write_word(start + 30, 0); // maxNRel
    bus.write_word(start + 32, 0); // cntEmpty
    bus.write_word(start + 34, 0); // cntHandles
    bus.write_long(start + 36, free_bytes); // minCBFree
    bus.write_long(start + 40, 0); // purgeProc
    bus.write_long(start + 44, 0); // sparePtr
    bus.write_long(start + 48, start.wrapping_add(52)); // allocPtr
}

fn scribble_uninitialized_allocation(bus: &mut MacMemoryBus, address: u32, size: u32) {
    if size == 0 {
        return;
    }
    // IM:Memory 1992 / IM:II document that regular NewPtr/NewHandle
    // allocations leave contents undefined; only CLEAR variants are
    // guaranteed to return zero-filled memory.
    for offset in 0..size {
        bus.write_byte(address.wrapping_add(offset), 0xA5);
    }
}

const NO_ERR: u32 = 0;
const BAD_UNIT_ERR: u32 = (-21i32) as u32;
const PARAM_ERR: u32 = (-50i32) as u32;
const MEM_FULL_ERR: u32 = (-108i32) as u32;
const NIL_HANDLE_ERR: u32 = (-109i32) as u32;
const DT_QTYPE: u16 = 7;
const NOT_HELD_ERR: u32 = (-621i32) as u32;
const NOT_LOCKED_ERR: u32 = (-623i32) as u32;
const VM_PAGE_SHIFT: u32 = 12;
const VM_PAGE_SIZE: u32 = 1 << VM_PAGE_SHIFT;

#[inline]
fn return_noerr<C: CpuOps>(cpu: &mut C) -> Result<()> {
    cpu.write_reg(Register::D0, NO_ERR);
    Ok(())
}

#[inline]
fn set_mem_error(bus: &mut MacMemoryBus, result: u32) {
    bus.write_word(addr::MEM_ERR, result as u16);
}

#[inline]
fn write_memory_result<C: CpuOps>(cpu: &mut C, bus: &mut MacMemoryBus, result: u32) {
    set_mem_error(bus, result);
    cpu.write_reg(Register::D0, result);
}

fn vm_page_span(start: u32, count: u32) -> Option<(u32, u32)> {
    if count == 0 {
        return None;
    }
    let page_start = start >> VM_PAGE_SHIFT;
    let end_exclusive = (start as u64).saturating_add(count as u64);
    let page_end_exclusive =
        end_exclusive.saturating_add((VM_PAGE_SIZE - 1) as u64) >> VM_PAGE_SHIFT;
    let page_end_exclusive = page_end_exclusive.min(u32::MAX as u64) as u32;
    if page_end_exclusive <= page_start {
        None
    } else {
        Some((page_start, page_end_exclusive))
    }
}

fn vm_required_physical_entries(start: u32, count: u32) -> u32 {
    vm_page_span(start, count)
        .map(|(page_start, page_end_exclusive)| page_end_exclusive - page_start)
        .unwrap_or(0)
}

fn vm_range_is_logical_ram(bus: &MacMemoryBus, start: u32, count: u32) -> bool {
    let ram_size = bus.ram_size() as u64;
    let start = start as u64;
    // Zero-length ranges are only valid if they still start inside logical RAM.
    if start >= ram_size {
        return false;
    }
    if count == 0 {
        return true;
    }
    let end_exclusive = start.saturating_add(count as u64);
    start < ram_size && end_exclusive <= ram_size
}

fn vm_range_is_fully_tracked(
    page_counts: &HashMap<u32, u16>,
    page_start: u32,
    page_end_exclusive: u32,
) -> bool {
    (page_start..page_end_exclusive).all(|page| page_counts.get(&page).copied().unwrap_or(0) > 0)
}

fn vm_increment_pages(
    page_counts: &mut HashMap<u32, u16>,
    page_start: u32,
    page_end_exclusive: u32,
) {
    for page in page_start..page_end_exclusive {
        let count = page_counts.entry(page).or_insert(0);
        *count = count.saturating_add(1);
    }
}

fn vm_try_decrement_pages(
    page_counts: &mut HashMap<u32, u16>,
    page_start: u32,
    page_end_exclusive: u32,
) -> bool {
    if !vm_range_is_fully_tracked(page_counts, page_start, page_end_exclusive) {
        return false;
    }
    let mut remove_pages = Vec::new();
    for page in page_start..page_end_exclusive {
        if let Some(count) = page_counts.get_mut(&page) {
            if *count <= 1 {
                remove_pages.push(page);
            } else {
                *count -= 1;
            }
        }
    }
    for page in remove_pages {
        page_counts.remove(&page);
    }
    true
}

fn trace_memory_site(trap_site: u32) -> bool {
    trace_memory_enabled() && matches!(trap_site, 0x0007B4FE | 0x0025793E | 0x0007B51E | 0x0007B526)
}

fn trace_video_driver_enabled() -> bool {
    *TRACE_VIDEO_DRIVER.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_VIDEO_DRIVER").is_some())
}

fn trace_entropy_enabled() -> bool {
    *TRACE_ENTROPY.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_ENTROPY").is_some())
}

fn apply_memory_manager_dispatcher_ccr<C: CpuOps>(cpu: &mut C) {
    // Memory Manager assembly-language callers observe the trap dispatcher's
    // TST.W D0 behavior on return. Preserve X, set N/Z from the low word,
    // and clear V/C as TST.W would.
    // Inside Macintosh Volume II, II-14; Memory 1992, 2-14
    let mut ccr = cpu.get_ccr() & 0x10;
    let low_word = cpu.read_reg(Register::D0) as u16;
    if low_word == 0 {
        ccr |= 0x04;
    } else if (low_word & 0x8000) != 0 {
        ccr |= 0x08;
    }
    cpu.set_ccr(ccr);
}

fn memory_manager_trap_updates_dispatcher_ccr(is_tool: bool, trap_num: u16) -> bool {
    matches!(
        (is_tool, trap_num),
        (
            false,
            0x1C | 0x1D
                | 0x1E
                | 0x1F
                | 0x20
                | 0x21
                | 0x22
                | 0x23
                | 0x24
                | 0x25
                | 0x27
                | 0x28
                | 0x29
                | 0x2A
                | 0x2D
                | 0x2E
                | 0x36
                | 0x40
                | 0x4C
                | 0x62
                | 0x63
                | 0x64
                | 0x66
                | 0x69
                | 0x8D
                | 0x88
                | 0xAE
                | 0xA4
        ) | (true, 0x1E1..=0x1E3)
    )
}

impl super::TrapDispatcher {
    fn sync_vbl_links(&mut self, bus: &mut MacMemoryBus) {
        if self.system_vbl_queue_anchor == 0 {
            let anchor = bus.alloc_synthetic(14);
            if anchor != 0 {
                bus.write_word(anchor + 4, 1); // vType
                self.system_vbl_queue_anchor = anchor;
            }
        }

        for task in &self.vbl_tasks {
            let next = self
                .vbl_tasks
                .iter()
                .skip_while(|candidate| candidate.task_ptr != task.task_ptr)
                .skip(1)
                .find(|candidate| candidate.slot == task.slot)
                .map(|candidate| candidate.task_ptr)
                .unwrap_or(0);
            bus.write_long(task.task_ptr, next);
        }

        let mut system_tasks = self
            .vbl_tasks
            .iter()
            .filter(|task| task.slot.is_none())
            .map(|task| task.task_ptr);
        let first_task = system_tasks.next().unwrap_or(0);
        let anchor = self.system_vbl_queue_anchor;
        if anchor != 0 {
            bus.write_long(anchor, first_task);
        }
        let head = if anchor != 0 { anchor } else { first_task };
        let tail = system_tasks.last().unwrap_or_else(|| {
            if first_task != 0 {
                first_task
            } else {
                anchor
            }
        });
        // QHdr layout: qFlags WORD, qHead QElemPtr, qTail QElemPtr.
        bus.write_long(VBL_QUEUE_HEADER + 2, head);
        bus.write_long(VBL_QUEUE_HEADER + 6, tail);
    }

    fn trap_address_table_key(&self, trap_word: u16) -> u16 {
        match self.current_trap_word & 0x0FFF {
            // NGetTrapAddress/NSetTrapAddress NEWTOOL variants.
            // Inside Macintosh: Operating System Utilities 1994, pp. 8-27
            // and 8-30: trapNum may be an A-line instruction or trap number;
            // irrelevant high bits are masked according to the trap type.
            0x647 | 0x746 => 0xA800 | (trap_word & 0x03FF),
            // NEWOS / OS-table variants mask to the low 8-bit OS table.
            0x146 | 0x247 | 0x346 => 0xA000 | (trap_word & 0x00FF),
            _ => {
                let trap_num = trap_word & 0x03FF;
                if matches!(trap_num, 0x000..=0x04F | 0x054 | 0x057) {
                    0xA000 | (trap_num & 0x00FF)
                } else {
                    0xA800 | trap_num
                }
            }
        }
    }

    fn looks_like_callable_proc(bus: &MacMemoryBus, proc_ptr: u32) -> bool {
        if proc_ptr == 0 || proc_ptr > bus.ram_size().saturating_sub(2) {
            return false;
        }
        matches!(bus.read_word(proc_ptr), 0x4E56 | 0x48E7 | 0x4EF9 | 0x4EFA)
    }

    fn get_or_create_defer_user_fn_trampoline(&mut self, bus: &mut MacMemoryBus) -> u32 {
        if self.defer_user_fn_trampoline != 0 {
            return self.defer_user_fn_trampoline;
        }

        let tramp = bus.alloc(24);
        if tramp == 0 {
            return 0;
        }

        // Layout:
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)
        //   +4:  MOVEA.L #imm,A0        ; patched with argument
        //   +10: JSR abs.L             ; patched with userFunction
        //   +16: MOVEM.L (SP)+,D0-D3/A0-A3
        //   +20: MOVEQ #0,D0           ; noErr
        //   +22: RTS
        bus.write_word(tramp, 0x48E7);
        bus.write_word(tramp + 2, 0xF0F0);
        bus.write_word(tramp + 4, 0x207C);
        bus.write_word(tramp + 10, 0x4EB9);
        bus.write_word(tramp + 16, 0x4CDF);
        bus.write_word(tramp + 18, 0x0F0F);
        bus.write_word(tramp + 20, 0x7000);
        bus.write_word(tramp + 22, 0x4E75);
        self.defer_user_fn_trampoline = tramp;
        tramp
    }

    fn install_vbl_task(
        &mut self,
        bus: &mut MacMemoryBus,
        task_ptr: u32,
        slot: Option<i16>,
    ) -> i16 {
        if trace_vbl_enabled() {
            let q_type = bus.read_word(task_ptr + 4) as i16;
            let vbl_addr = bus.read_long(task_ptr + 6);
            let vbl_count = bus.read_word(task_ptr + 10) as i16;
            let vbl_phase = bus.read_word(task_ptr + 12) as i16;
            eprintln!(
                "[VBL] install tick={} task=${:08X} qType={} addr=${:08X} count={} phase={} slot={:?}",
                self.tick_count, task_ptr, q_type, vbl_addr, vbl_count, vbl_phase, slot
            );
        }
        if task_ptr == 0 || bus.read_word(task_ptr + 4) as i16 != 1 {
            return -2; // vTypErr
        }
        // BasiliskII accepts slot = -1 for slot-based VBL install/remove.
        // Preserve the documented slotNumErr for more-negative values.
        if matches!(slot, Some(s) if s < -1) {
            return -360; // slotNumErr
        }

        let vbl_count = bus.read_word(task_ptr + 10) as i16;
        let vbl_phase = bus.read_word(task_ptr + 12) as i16;
        bus.write_word(task_ptr + 10, vbl_count.wrapping_add(vbl_phase) as u16);

        self.vbl_tasks.retain(|task| task.task_ptr != task_ptr);
        self.vbl_tasks
            .push(super::dispatch::VblTask { task_ptr, slot });
        self.sync_vbl_links(bus);
        0
    }

    fn remove_vbl_task(&mut self, bus: &mut MacMemoryBus, task_ptr: u32, slot: Option<i16>) -> i16 {
        if task_ptr == 0 || bus.read_word(task_ptr + 4) as i16 != 1 {
            return -2; // vTypErr
        }
        if matches!(slot, Some(s) if s < -1) {
            return -360; // slotNumErr
        }

        // Real ROM's (Slot)VRemove returns qErr (-1) if the task isn't
        // currently in the VBL queue.
        let was_in_queue = self.vbl_tasks.iter().any(|task| task.task_ptr == task_ptr);
        if !was_in_queue {
            return -1; // qErr
        }
        self.vbl_tasks.retain(|task| task.task_ptr != task_ptr);
        bus.write_long(task_ptr, 0);
        self.sync_vbl_links(bus);
        0
    }

    pub(crate) fn dispatch_memory<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        let result = match (is_tool, trap_num) {
            // ========== Memory Manager ==========
            // NewPtr ($A01E) / NewPtrSys ($A11E) / NewPtrClear ($A31E) / NewPtrSysClear ($A71E)
            // Allocates a nonrelocatable block. CLEAR variants zero the memory.
            // Inside Macintosh Volume II, II-37
            // NewPtr / NewPtrClear / NewPtrSys ($A01E): Allocates via `bus.alloc()`, returns ptr in A0; CLEAR variant ($A31E) zeros memory per IM:II II-37
            (false, 0x1E) => {
                let size = cpu.read_reg(Register::D0);
                // `Size` is a signed Macintosh LONGINT. Reject negative
                // requests before converting them into an unsigned heap
                // allocation; otherwise an error value can wrap the bump
                // pointer and scribble across guest memory.
                if (size as i32) < 0 {
                    cpu.write_reg(Register::A0, 0);
                    write_memory_result(cpu, bus, MEM_FULL_ERR);
                    return Some(Ok(()));
                }
                let ptr = bus.alloc(size);
                if ptr == 0 && size > 0 {
                    cpu.write_reg(Register::A0, 0);
                    write_memory_result(cpu, bus, MEM_FULL_ERR);
                } else {
                    // Trap bit 10 selects the system-heap variants. Keep the
                    // application-zone header synchronized only for ordinary
                    // NewPtr/NewPtrClear allocations.
                    if (self.current_trap_word & 0x0400) == 0 {
                        include_application_allocation_in_zone(bus, ptr, size);
                    }
                    // Check CLEAR bit (bit 9 = 0x0200 in trap word)
                    if (self.current_trap_word & 0x0200) != 0 && size > 0 {
                        bus.fill_zeros(ptr, size);
                    } else {
                        scribble_uninitialized_allocation(bus, ptr, size);
                    }
                    cpu.write_reg(Register::A0, ptr);
                    write_memory_result(cpu, bus, NO_ERR);
                }
                Ok(())
            }

            // NewHandle ($A022)
            // Allocates a new relocatable block and returns a handle to it. CLEAR variants zero the memory.
            // FUNCTION NewHandle (logicalSize: Size): Handle;
            // Inside Macintosh Volume II, II-27
            (false, 0x22) => {
                let size = cpu.read_reg(Register::D0);
                // `Size` is a signed Macintosh LONGINT; negative sizes are
                // invalid and must not be passed to the unsigned allocator.
                if (size as i32) < 0 {
                    cpu.write_reg(Register::A0, 0);
                    write_memory_result(cpu, bus, MEM_FULL_ERR);
                    return Some(Ok(()));
                }
                let ptr = bus.alloc(size);
                if ptr == 0 && size > 0 {
                    cpu.write_reg(Register::A0, 0);
                    write_memory_result(cpu, bus, MEM_FULL_ERR);
                    return Some(Ok(()));
                }
                // Check CLEAR bit (bit 9 = 0x0200 in trap word)
                if (self.current_trap_word & 0x0200) != 0 && size > 0 {
                    bus.fill_zeros(ptr, size);
                } else {
                    scribble_uninitialized_allocation(bus, ptr, size);
                }
                let handle = bus.alloc(4);
                if handle == 0 {
                    if ptr != 0 {
                        bus.free(ptr);
                    }
                    cpu.write_reg(Register::A0, 0);
                    write_memory_result(cpu, bus, MEM_FULL_ERR);
                    return Some(Ok(()));
                }
                bus.write_long(handle, ptr);
                // Track the ptr → handle mapping so RecoverHandle can find
                // this handle later given just its master pointer's data
                // address. Inside Macintosh Volume V, V-579.
                self.ptr_to_handle.insert(ptr, handle);
                cpu.write_reg(Register::A0, handle);
                write_memory_result(cpu, bus, NO_ERR);
                Ok(())
            }

            // NewEmptyHandle ($A166)
            // Allocates a master pointer set to NIL without allocating a data block.
            // FUNCTION NewEmptyHandle: Handle;
            // Inside Macintosh: Memory, 2-33
            // NewEmptyHandle ($A066): Allocates master pointer set to NIL, no data block
            (false, 0x66) => {
                let handle = bus.alloc(4);
                if handle == 0 {
                    cpu.write_reg(Register::A0, 0);
                    write_memory_result(cpu, bus, MEM_FULL_ERR);
                } else {
                    bus.write_long(handle, 0); // master pointer = NIL
                    cpu.write_reg(Register::A0, handle);
                    write_memory_result(cpu, bus, NO_ERR);
                }
                Ok(())
            }

            // DisposePtr ($A01F): A0 = pointer to free
            // DisposePtr ($A01F): Frees pointer via bus.free(), returns noErr
            (false, 0x1F) => {
                let ptr = cpu.read_reg(Register::A0);
                bus.free(ptr);
                write_memory_result(cpu, bus, NO_ERR);
                Ok(())
            }

            // DisposeHandle ($A023)
            // Releases the relocatable block and frees the master pointer for other uses.
            // PROCEDURE DisposeHandle (h: Handle);
            // Inside Macintosh: Memory 1992, 2-34..2-35
            (false, 0x23) => {
                let handle = cpu.read_reg(Register::A0);
                let trap_site = cpu.read_reg(Register::PC).wrapping_sub(2);
                let resource_backing = self.loaded_handles.get(&handle).copied();
                if let Some((_ptr, res_type, res_id)) = resource_backing {
                    if &res_type == b".256" {
                        eprintln!(
                            "[MEM] DisposeHandle .256 id={} handle=${:08X}",
                            res_id, handle
                        );
                    }
                }
                self.detached_handles.remove(&handle);
                self.forget_resource_handle_index_for_handle(handle);
                self.loaded_handles.remove(&handle);
                self.resource_handle_files.remove(&handle);
                self.detached_handle_files.remove(&handle);
                self.handle_state_bits.remove(&handle);
                if handle != 0 {
                    let data_ptr = bus.read_long(handle);
                    if trace_memory_site(trap_site) {
                        eprintln!(
                            "[MEM] DisposeHandle @${:08X} handle=${:08X} ptr=${:08X}",
                            trap_site, handle, data_ptr
                        );
                    }
                    // Do NOT remove data_ptr from ptr_to_handle here. On real
                    // Mac OS, DisposeHandle frees the master pointer but does
                    // not zero it; the freed slot still holds the old data
                    // address. RecoverHandle scans all master pointer slots
                    // (including freed ones), so it returns the stale handle
                    // rather than nil. We replicate this by leaving the stale
                    // entry in ptr_to_handle; NewHandle will overwrite it if
                    // the address is ever reused. (IM:V V-579)
                    if resource_backing.is_none() {
                        bus.free(data_ptr);
                    }
                    bus.free(handle);
                }
                write_memory_result(cpu, bus, NO_ERR);
                Ok(())
            }

            // VInstall ($A033)
            // Installs a VBL task record into the system-based vertical
            // retrace queue. The task's vblAddr routine is executed when
            // its vblCount expires.
            // FUNCTION VInstall (vblTaskPtr: QElemPtr): OSErr;
            // Inside Macintosh: Processes (1994), pp. 4-24..4-25
            //
            // Register convention (IM:Processes 1994 p. 4-24):
            //   On entry: A0 = pointer to the VBL task record.
            //   On exit:  D0 = result code (noErr 0 | vTypErr -2).
            //
            // OS-bit FUNCTION ABI: no Pascal stack frame, no result slot —
            // A0 carries the input, D0 carries the OSErr result. The MPW
            // Universal Headers Retrace.h exposes:
            //   #pragma parameter __D0 VInstall(__A0)
            //   EXTERN_API(OSErr) VInstall(QElemPtr vblTaskPtr)
            //       ONEWORDINLINE(0xA033);
            //
            // VBLTask record layout (IM:Processes 1994 p. 4-7..4-8):
            //   +0   qLink     QElemPtr   set by VInstall
            //   +4   qType     INTEGER    must be ORD(vType) = 1
            //   +6   vblAddr   ProcPtr    interrupt-time routine
            //  +10   vblCount  INTEGER    interrupts until next call
            //  +12   vblPhase  INTEGER    phase shift added on install
            //
            // Result codes (IM:Processes 1994 p. 4-25):
            //   noErr   (0)  — task added to the system VBL queue.
            //   vTypErr (-2) — qType field is not ORD(vType).
            //
            // Regression coverage: src/trap/memory.rs vinstall_consumes_a0_taskptr_...,
            // vinstall_invalid_qtype_returns_vtyperr,
            // vinstall_then_vremove_roundtrip_returns_noerr_on_both_and_qerr_on_second_remove.
            (false, 0x33) => {
                let task_ptr = cpu.read_reg(Register::A0);
                cpu.write_reg(
                    Register::D0,
                    self.install_vbl_task(bus, task_ptr, None) as u16 as u32,
                );
                Ok(())
            }

            // VRemove ($A034)
            // Removes a VBL task record from the system-based vertical
            // retrace queue.
            // FUNCTION VRemove (vblTaskPtr: QElemPtr): OSErr;
            // Inside Macintosh: Processes (1994), pp. 4-25..4-26
            //
            // Register convention (IM:Processes 1994 p. 4-26):
            //   On entry: A0 = pointer to the VBL task record.
            //   On exit:  D0 = result code (noErr 0 | qErr -1 | vTypErr -2).
            //
            // OS-bit FUNCTION ABI: same register-only shape as VInstall.
            // MPW Universal Headers Retrace.h:
            //   #pragma parameter __D0 VRemove(__A0)
            //   EXTERN_API(OSErr) VRemove(QElemPtr vblTaskPtr)
            //       ONEWORDINLINE(0xA034);
            //
            // Result codes (IM:Processes 1994 p. 4-26):
            //   noErr   (0)  — task removed from the queue.
            //   qErr    (-1) — task record isn't in the queue.
            //   vTypErr (-2) — qType field is not ORD(vType).
            //
            // Regression coverage: src/trap/memory.rs vremove_consumes_a0_taskptr_...,
            // vremove_task_not_in_queue_returns_qerr,
            // vinstall_then_vremove_roundtrip_returns_noerr_on_both_and_qerr_on_second_remove.
            (false, 0x34) => {
                let task_ptr = cpu.read_reg(Register::A0);
                cpu.write_reg(
                    Register::D0,
                    self.remove_vbl_task(bus, task_ptr, None) as u16 as u32,
                );
                Ok(())
            }

            // HLock ($A029) / HUnlock ($A02A) / MoveHHi ($A064)
            // Locks or unlocks a relocatable block in its heap zone.
            // PROCEDURE HLock (h: Handle); PROCEDURE HUnlock (h: Handle);
            // Inside Macintosh Volume II, II-34
            (false, 0x29) | (false, 0x2A) | (false, 0x64) => {
                let handle = cpu.read_reg(Register::A0);
                if handle != 0 {
                    if trace_sound_enabled() {
                        if let Some((ptr, res_type, res_id)) =
                            self.loaded_handles.get(&handle).copied()
                        {
                            if res_type == *b"snd " {
                                let trap_site = cpu.read_reg(Register::PC).wrapping_sub(2);
                                let name = match trap_num {
                                    0x29 => "HLock",
                                    0x2A => "HUnlock",
                                    _ => "MoveHHi",
                                };
                                eprintln!(
                                    "[SOUND-MEM] {} @${:08X} handle=${:08X} ptr=${:08X} {} id={}",
                                    name,
                                    trap_site,
                                    handle,
                                    ptr,
                                    format_ostype(res_type),
                                    res_id
                                );
                            }
                        }
                    }
                    match trap_num {
                        0x29 => {
                            let bits = self.handle_state_bits.entry(handle).or_insert(0);
                            *bits |= 0x80; // lock bit
                        }
                        0x2A => {
                            let bits = self.handle_state_bits.entry(handle).or_insert(0);
                            *bits &= !0x80;
                        }
                        _ => {
                            if self.res_purge {
                                let _ = self.write_resource_backing_if_changed(bus, handle);
                            }
                        }
                    }
                }
                write_memory_result(cpu, bus, NO_ERR);
                Ok(())
            }

            // GetHandleSize ($A025)
            // Returns the logical size in bytes of the relocatable block whose handle is h.
            // FUNCTION GetHandleSize (h: Handle): Size;
            // Inside Macintosh Volume II, II-31
            (false, 0x25) => {
                let handle = cpu.read_reg(Register::A0);
                let trap_site = cpu.read_reg(Register::PC).wrapping_sub(2);
                if handle == 0 {
                    write_memory_result(cpu, bus, NIL_HANDLE_ERR);
                } else {
                    let ptr = bus.read_long(handle);
                    let size = self
                        .resource_handle_memory_size(bus, handle, ptr)
                        .or_else(|| bus.get_alloc_size(ptr))
                        .unwrap_or(0);
                    if trace_sound_enabled() {
                        if let Some((_resource_ptr, res_type, res_id)) =
                            self.loaded_handles.get(&handle).copied()
                        {
                            if res_type == *b"snd " {
                                eprintln!(
                                    "[SOUND-MEM] GetHandleSize @${:08X} handle=${:08X} ptr=${:08X} {} id={} size={}",
                                    trap_site,
                                    handle,
                                    ptr,
                                    format_ostype(res_type),
                                    res_id,
                                    size
                                );
                            }
                        }
                    }
                    if trace_memory_site(trap_site) {
                        eprintln!(
                            "[MEM] GetHandleSize @${:08X} handle=${:08X} ptr=${:08X} size={}",
                            trap_site, handle, ptr, size
                        );
                    }
                    set_mem_error(bus, NO_ERR);
                    cpu.write_reg(Register::D0, size);
                }
                Ok(())
            }

            // GetTrapAddress ($A046/$A146/$A346/$A746)
            // Returns the address of a trap handler. Used by apps to check
            // if a trap is implemented (comparing against _Unimplemented).
            // Register-based: D0.W = trap number -> A0 = trap address
            // Inside Macintosh Volume II, II-384
            //
            // Tool traps get a 2-byte trampoline stub (auto-pop variant
            // of the trap word) so games that `JSR (A0)` through the
            // returned address re-enter the trap dispatcher cleanly
            // instead of executing garbage memory at a bare fake-ptr.
            // OS traps fall back to the simple `$00F0xxxx` fake-ptr
            // because they have no auto-pop semantics. See
            // `TrapDispatcher::get_or_create_tool_trap_trampoline`.
            // GetTrapAddress ($A046): Checks native_trap_table; tool traps return a callable trampoline (auto-pop variant), OS traps return $F0xxxx fake-ptr per IM:II II-384
            (false, 0x46) => {
                let trap_word = cpu.read_reg(Register::D0) as u16;
                let trap_variant = self.current_trap_word & 0x0FFF;
                // Check native trap table first (returns address set by SetTrapAddress)
                let trap_table_key = self.trap_address_table_key(trap_word);
                if let Some(&addr) = self.native_trap_table.get(&trap_table_key) {
                    cpu.write_reg(Register::A0, addr);
                } else if trap_variant == 0x746 {
                    // Real GetToolTrapAddress/GetToolBoxTrapAddress
                    // (trap word $A746) passes a bare tool-trap number
                    // in D0. Return a callable tool-trap trampoline so
                    // guests can JSR/JMP through the result.
                    //
                    // DockingDispatch (0x257) is absent on the generic
                    // target, so collapse that probe to _Unimplemented
                    // (0xA89F) just like the other probe forms.
                    let trap_num = trap_word & 0x03FF;
                    let canonical_tool_trap = if trap_num == 0x257 {
                        0xA89F
                    } else {
                        0xA800 | trap_num
                    };
                    let addr = self.get_or_create_tool_trap_trampoline(bus, canonical_tool_trap);
                    cpu.write_reg(Register::A0, addr);
                } else if trap_word == 0xAA57 {
                    // DockingDispatch is PowerBook-only. On the generic
                    // non-PowerBook target we alias the probe to the
                    // existing _Unimplemented trampoline so callers that
                    // compare against GetTrapAddress($A89F) see the trap
                    // as absent rather than as a callable DockingDispatch
                    // entry. IM:II II-384; IM:I I-89.
                    let addr = self.get_or_create_tool_trap_trampoline(bus, 0xA89F);
                    cpu.write_reg(Register::A0, addr);
                } else if trap_word == 0x257 {
                    // NGetTrapAddress() probe for DockingDispatch uses
                    // the low-number tool-trap form. Alias that probe to
                    // the same unimplemented sentinel used for 0x09F so
                    // selector-0 availability checks compare equal.
                    cpu.write_reg(Register::A0, 0xCAFE0000 + 0x09F);
                } else if (trap_word & 0x0800) != 0 {
                    // Tool trap (bit 11 set in the word): hand back a
                    // callable trampoline so JSR-through-fake-ptr works.
                    let addr = self.get_or_create_tool_trap_trampoline(bus, trap_word);
                    cpu.write_reg(Register::A0, addr);
                } else {
                    // OS trap (or bare trap number from NGetTrapAddress
                    // where the caller only intends address comparison):
                    // unique fake address per word so it never matches
                    // _Unimplemented.
                    let fake_addr = 0x00F00000 | (trap_word as u32);
                    cpu.write_reg(Register::A0, fake_addr);
                }
                Ok(())
            }

            // BlockMove ($A02E)
            // Copies a block of bytes from one location to another.
            // A0 = source, A1 = destination, D0 = byte count.
            // Works correctly even when the ranges overlap.
            // Inside Macintosh Volume II, II-44; Memory 1992, 2-59 to 2-60
            // BlockMove / BlockMoveData ($A02E): Copies D0 bytes from A0 to A1 and preserves overlap semantics; BlockMoveData ($A22E) handled identically (no caches in emulator)
            (false, 0x2E) => {
                let src = cpu.read_reg(Register::A0);
                let dst = cpu.read_reg(Register::A1);
                let count = cpu.read_reg(Register::D0);
                let trap_site = cpu.read_reg(Register::PC).wrapping_sub(2);
                if trace_memory_site(trap_site) {
                    let preview_len = count.min(16) as usize;
                    eprintln!(
                        "[MEM] BlockMove @${:08X} src=${:08X} dst=${:08X} count={} bytes={:02X?}",
                        trap_site,
                        src,
                        dst,
                        count,
                        bus.read_bytes(src, preview_len)
                    );
                }
                // Fast path via MacMemoryBus::block_move — uses
                // slice::copy_within for in-RAM copies with overlap
                // handling, falls back to byte-at-a-time for edge
                // cases (watchpoint armed, crosses RAM boundary).
                bus.block_move(src, dst, count);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // Phantom GetToolTrapAddress arm for is_tool=true && idx=0x346.
            //
            // Real GetTrapAddress / GetOSTrapAddress / GetToolTrapAddress
            // (trap words $A046 / $A146 / $A346 / $A746) all have bit
            // 11 = 0, so they reach the (false, 0x46) handler above —
            // *not* this arm. No assigned trap word in the IM trap
            // tables has lower-10-bits 0x346 with bit 11 set, so this
            // handler is unreachable from real guest code.
            //
            // Kept (rather than excised) only because legacy lib tests
            // call dispatch_memory(true, 0x346, ...) directly to
            // exercise the historical $CAFE0xxx fake-ptr emit path —
            // that path's *decoder* in runner.rs::decode_fakeptr_pc is
            // still active diagnostic surface (see
            // decode_fakeptr_pc_recognizes_tool_style_range).
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::phantom_gettooltrapaddress_returns_native_table_entry_and_preserves_stack
            //   src/trap/memory.rs::tests::test_get_tool_trap_address_docking_dispatch_aliases_unimplemented_probe
            //   src/trap/memory.rs::tests::phantom_gettooltrapaddress_creates_stable_trampoline_for_tool_trap_word
            //   src/trap/memory.rs::tests::test_get_tool_trap_address_tool
            // Inside Macintosh Volume II, II-384
            // (phantom GetToolTrapAddress) ($AB46): Unreachable from real guest code; emits $CAFE0xxx fake-ptr only via direct dispatch_memory test calls
            (true, 0x346) => {
                let trap_word = cpu.read_reg(Register::D0) as u16;
                if let Some(&addr) = self.native_trap_table.get(&trap_word) {
                    cpu.write_reg(Register::A0, addr);
                } else if trap_word == 0x257 {
                    // DockingDispatch probe uses the low-number tool-trap
                    // form when callers go through NGetTrapAddress.
                    // Mirror the same fake-ptr sentinel as the Unimplemented
                    // tool trap probe so the two addresses compare equal.
                    cpu.write_reg(Register::A0, 0xCAFE0000 + 0x09F);
                } else if (trap_word & 0x0800) != 0 {
                    let addr = self.get_or_create_tool_trap_trampoline(bus, trap_word);
                    cpu.write_reg(Register::A0, addr);
                } else {
                    let t = trap_word as u32 & 0x3FF;
                    cpu.write_reg(Register::A0, 0xCAFE0000 + t);
                }
                Ok(())
            }

            // SetTrapAddress ($A047/$A647 = (false, 0x47))
            // Installs a native 68K trap handler.
            // D0.W = trap number, A0 = new handler address
            // Inside Macintosh Volume II, II-384
            // SetTrapAddress ($A047): Installs native 68K trap handler via D0=trap word, A0=handler; per IM:II II-384
            (false, 0x47) => {
                let trap_word = cpu.read_reg(Register::D0) as u16;
                let trap_table_key = self.trap_address_table_key(trap_word);
                let handler_addr = cpu.read_reg(Register::A0);
                // Restore-to-default detection: when SetTrapAddress
                // sees an address that *we* handed back from a prior
                // GetTrapAddress (either the legacy $00F0xxxx fake-ptr
                // range OR a tool-trap trampoline allocated by
                // get_or_create_tool_trap_trampoline), the game is
                // saying "put it back the way you found it" and the
                // correct response is to remove our native_trap_table
                // entry so subsequent traps go through the HLE
                // dispatcher chain again.
                //
                // Without the trampoline arm, games that read +
                // restore tool-trap addresses (e.g. Steel Fighters'
                // ExitToShell shim) would store the trampoline
                // address back into native_trap_table, and dispatch
                // would JMP to the trampoline, which re-traps with
                // auto-pop, which re-enters dispatch, which JMPs to
                // the trampoline … infinite loop.
                let is_legacy_fakeptr = (handler_addr & 0xFFFF0000) == 0x00F00000;
                let is_trampoline = self
                    .tool_trap_trampolines
                    .values()
                    .any(|&addr| addr == handler_addr);
                if is_legacy_fakeptr || is_trampoline {
                    self.native_trap_table.remove(&trap_table_key);
                } else {
                    self.native_trap_table.insert(trap_table_key, handler_addr);
                }
                Ok(())
            }

            // StripAddress ($A055)
            // Strips the high byte of a 24-bit address. No-op in 32-bit mode.
            // Inside Macintosh Volume V, V-593
            // StripAddress ($A055): Returns A0 unchanged
            (false, 0x55) => Ok(()),

            // SwapMMUMode ($A05D)
            // Sets the addressing mode to the value in D0 (0=24-bit, 1=32-bit)
            // and returns the previous mode in D0.
            // PROCEDURE SwapMMUMode (VAR mode: Byte);
            // Inside Macintosh Volume V, V-593
            // SwapMMUMode ($A05D): Sets addressing mode (D0=0 for 24-bit, 1 for 32-bit), returns previous mode in D0
            (false, 0x5D) => {
                let new_mode = cpu.read_reg(Register::D0) & 0xFF;
                let old_mode = self.mmu_mode as u32;
                self.mmu_mode = (new_mode & 1) as u8;
                bus.write_byte(addr::MMU32_BIT, self.mmu_mode);
                cpu.write_reg(Register::D0, old_mode);
                Ok(())
            }

            // SlotVInstall ($A06F): A0 = VBLTaskPtr, D0 = slot, D0 = result code
            // Processes 1994, 4-22 to 4-23
            // SlotVInstall ($A06F): Installs slot-VBLTask via install_vbl_task with slot from D0; A0=VBLTaskPtr, D0=OSErr
            (false, 0x6F) => {
                let task_ptr = cpu.read_reg(Register::A0);
                let slot = cpu.read_reg(Register::D0) as u16 as i16;
                cpu.write_reg(
                    Register::D0,
                    self.install_vbl_task(bus, task_ptr, Some(slot)) as u16 as u32,
                );
                Ok(())
            }

            // SlotVRemove ($A070) shares trap space with a compatibility
            // register-based GetNextEvent shim in event.rs. Only claim it when
            // A0 clearly points at a VBLTask record.
            // Processes 1994, 4-23 to 4-24
            // SlotVRemove ($A070): Removes slot-VBLTask via remove_vbl_task; gated to claim only when A0 points at VBLTask record (qType=1) — defers to event.rs GetNextEvent shim otherwise
            (false, 0x70) => {
                let task_ptr = cpu.read_reg(Register::A0);
                if task_ptr == 0 || bus.read_word(task_ptr + 4) as i16 != 1 {
                    return None;
                }
                let slot = cpu.read_reg(Register::D0) as u16 as i16;
                cpu.write_reg(
                    Register::D0,
                    self.remove_vbl_task(bus, task_ptr, Some(slot)) as u16 as u32,
                );
                Ok(())
            }

            // PurgeSpace ($A062)
            // Returns total purgeable space in A0 and contiguous
            // largest free block (after purging) in D0.
            // PROCEDURE PurgeSpace(VAR total: LongInt; VAR contig: LongInt);
            // Inside Macintosh Volume IV, IV-14 (assembly note:
            // A0 = total free if all purgeable blocks were
            // purged; D0 = largest contiguous free block).
            //
            // HLE compromise: Systemless doesn't model heap
            // fragmentation OR purgeable resource blocks (every
            // allocation is permanent until explicitly freed),
            // so "total" and "contiguous" both collapse to the
            // free_heap_estimate value (ApplLimit - HeapEnd,
            // with the same compatibility floor/partition rule
            // used by FreeMem / MaxMem / CompactMem). Apps that probe
            // PurgeSpace before a large allocation to gate "do
            // I have enough room?" see the same 24MB-floor
            // answer those companion traps return — consistent
            // across the heap-introspection family.
            //
            // Previous stub returned a hardcoded 4MB constant
            // which underestimated the available memory and
            // could trip games that gate at "free >= 8 MB" type
            // checks. The free_heap_estimate floor at 24MB
            // satisfies all known minimum-RAM gates while still
            // reading real low-mem state.
            // PurgeSpace ($A062): Per IM:IV IV-14 returns total free space (after purging purgeable blocks) in A0 and largest contiguous free block in D0; Systemless doesn't model fragmentation or purgeable blocks so both registers get the free_heap_estimate value (same helper used by FreeMem / MaxMem / CompactMem). Replaces a prior hardcoded 4MB constant that was below modern minimum-RAM gates.
            (false, 0x62) => {
                let free = free_heap_estimate(bus);
                cpu.write_reg(Register::A0, free); // total purgeable
                cpu.write_reg(Register::D0, free); // largest contiguous
                Ok(())
            }

            // SysEnvirons ($A090)
            // FUNCTION SysEnvirons (versionRequested: INTEGER; VAR theWorld: SysEnvRec): OSErr;
            // Inside Macintosh Volume V, V-6
            // SysEnvirons ($A090): Hardcoded: sysVers=0x0700, machType=9, FPU/Color/68020 all true
            (false, 0x90) => {
                let mut a0 = cpu.read_reg(Register::A0);
                let _version = cpu.read_reg(Register::D0) & 0xFFFF;
                let a5 = cpu.read_reg(Register::A5);

                if a5 == 0 {
                    let saved_a5 = bus.read_long(0x904); // CurrentA5
                    cpu.write_reg(Register::A5, saved_a5);
                    if a0 == 0x30 && saved_a5 != 0 {
                        a0 = saved_a5 + 0x30;
                    }
                }

                if a0 < 0x100 {
                    cpu.write_reg(Register::D0, 0xFFFF_FFCE); // -50 (paramErr)
                    return Some(Ok(()));
                }

                let rec_ptr = a0;
                bus.write_word(rec_ptr, 2); // environsVersion
                bus.write_word(rec_ptr + 2, REFERENCE_MACHINE_PROFILE.gestalt_machine_type);
                bus.write_word(rec_ptr + 4, REFERENCE_MACHINE_PROFILE.system_version_bcd);
                bus.write_word(
                    rec_ptr + 6,
                    REFERENCE_MACHINE_PROFILE.gestalt_processor_type as u16,
                );
                bus.write_byte(rec_ptr + 8, u8::from(REFERENCE_MACHINE_PROFILE.has_fpu()));
                bus.write_byte(rec_ptr + 9, 1); // hasColorQD
                bus.write_word(rec_ptr + 10, 0);
                bus.write_word(rec_ptr + 12, 0);
                bus.write_word(rec_ptr + 14, 0);

                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // FlushCodeCache ($A0BD)
            // Flushes the instruction cache.
            // PROCEDURE FlushCodeCache;
            // Memory 1992, 4-31
            // FlushCodeCache ($A0BD): Memory 1992, 4-31
            (false, 0xBD) => {
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // DebugStr ($ABFF)
            // Enters the system debugger with a Pascal-string message if one
            // is installed; otherwise returns after consuming the argument.
            // Steel Fighters and similar mid-90s titles call this trap
            // many times in tight self-check loops, so the log line is
            // gated behind SYSTEMLESS_TRACE_DEBUGGER_TRAP=1 to avoid
            // drowning stderr when those games run.
            // void DebugStr(ConstStr255Param debuggerMsg);
            // Universal Interfaces 3.4 MacTypes.h; Inside Macintosh:
            // Memory (1992), p. 3-23
            // DebugStr ($ABFF): HLE no-op when no debugger is installed.
            (true, 0x3FF) => {
                let sp = cpu.read_reg(Register::A7);
                let debugger_msg = bus.read_long(sp);
                if std::env::var_os("SYSTEMLESS_TRACE_DEBUGGER_TRAP").is_some() {
                    let trap_site = cpu.read_reg(Register::PC).wrapping_sub(2);
                    let message =
                        String::from_utf8_lossy(&bus.read_pstring(debugger_msg)).into_owned();
                    eprintln!(
                        "[TRAP] DebugStr @${trap_site:08X} (${debugger_msg:08X}, {message:?}) called - no debugger installed, continuing"
                    );
                }
                cpu.write_reg(Register::A7, sp.wrapping_add(4));
                Ok(())
            }

            // SetApplLimit ($A02D)
            // Sets the application heap limit beyond which the heap can't expand.
            // PROCEDURE SetApplLimit (zoneLimit: Ptr);
            // Inside Macintosh: Memory 1992, 2-84..2-85; Inside Macintosh Volume II, II-30
            // SetApplLimit ($A02D): Writes zoneLimit to ApplLimit unless the current
            // heap already extends past zoneLimit, in which case the heap is not cut back.
            // When the application-zone header is still tracking the old limit, keep
            // bkLim/zcbFree in step so a following MaxApplZone leaves FreeMem nonzero.
            (false, 0x2D) => {
                let zone_limit = cpu.read_reg(Register::A0);
                let heap_end = bus.read_long(crate::memory::globals::addr::HEAP_END);
                let appl_limit = bus.read_long(crate::memory::globals::addr::APPL_LIMIT);

                if zone_limit >= heap_end {
                    if zone_limit != appl_limit {
                        bus.write_long(crate::memory::globals::addr::APPL_LIMIT, zone_limit);
                        reconcile_application_zone_limit(bus, appl_limit, zone_limit);
                    }
                } else {
                    // IM:Memory 1992, p. 2-85:
                    // "If the zone already extends beyond the specified limit,
                    // the Memory Manager does not cut it back."
                    bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);
                }
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // MaxApplZone ($A063)
            // PROCEDURE MaxApplZone;
            // Inside Macintosh: Memory 1992, pp. 2-27 and 2-74..2-75;
            // Inside Macintosh Volume II, II-30.
            //
            // Per IM:Memory 1-39: "If you call MaxApplZone at the
            // beginning of your program, the heap immediately extends
            // all the way up to ApplLimit." Systemless models that visible
            // effect by aligning the live heap end with the application
            // limit when the zone can still grow.
            (false, 0x63) => {
                let heap_end = bus.read_long(crate::memory::globals::addr::HEAP_END);
                let appl_limit = bus.read_long(crate::memory::globals::addr::APPL_LIMIT);
                if heap_end != appl_limit {
                    bus.write_long(crate::memory::globals::addr::HEAP_END, appl_limit);
                }
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // MoreMasters ($A036)
            // PROCEDURE MoreMasters;
            // Inside Macintosh: Memory 1992, 1-43 + 2-50.
            //
            // Per IM:Memory 1-43 + 1-993: "The Memory Manager
            // allocates one master pointer block (containing 64
            // master pointers) for your application at launch
            // time, and you can call MoreMasters to request that
            // additional master pointer blocks be allocated."
            // HLE no-op: Systemless doesn't model a fixed-capacity
            // master-pointer table — handles are allocated via
            // bus.alloc(4) on demand, so there's no master-
            // pointer pool to grow. Apps that call MoreMasters
            // multiple times at startup (the canonical
            // "MoreMasters * 4" pattern that pre-allocates 256
            // handles to avoid fragmentation later) see no
            // observable difference — subsequent NewHandle calls
            // succeed regardless of how many times MoreMasters
            // was called.
            // MoreMasters ($A036): PROCEDURE MoreMasters — per IM:Memory 1992 1-43
            // and IM:II II-31 allocates an additional master pointer block
            // (64 master pointers). HLE no-op: Systemless doesn't model a fixed-capacity
            // master-pointer table, so the startup pre-allocation pattern has no
            // observable effect. Return noErr in D0 for assembly-language callers.
            (false, 0x36) => {
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // ReadDateTime ($A039)
            // Reads seconds since midnight January 1, 1904 from the RTC chip.
            // FUNCTION ReadDateTime(VAR time: LongInt): OSErr;
            // Inside Macintosh Volume II, II-378; Operating System Utilities 1994, 4-17
            //
            // Per IM:II II-378: "ReadDateTime copies the current date-time information
            // from the clock chip into low memory." In Systemless the runner owns that
            // synthetic clock: init_app seeds Time ($020C), then advance_guest_tick
            // increments it once per 60 ticks. ReadDateTime returns the current lowmem
            // value so deterministic play runs and SetDateTime remain authoritative.
            (false, 0x39) => {
                let mac_time = bus.read_long(0x020C);
                bus.write_long(0x020C, mac_time); // Time global
                let a0 = cpu.read_reg(Register::A0);
                if a0 != 0 {
                    bus.write_long(a0, mac_time);
                }
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // ========== Heap-introspection family ==========
            //
            // Three register-based OS traps that report free /
            // largest-block / compactable space in the current
            // heap. All three share `free_heap_estimate(bus)`
            // which reads the ApplLimit + HeapEnd low-mem
            // globals. Unconstrained launches keep the historical
            // 24MB compatibility floor; explicit app partitions
            // report the actual partition span.
            //
            // HLE compromise: Systemless does NOT model heap
            // fragmentation — the "free", "max contiguous", and
            // "compactable" answers all collapse to the same
            // ApplLimit-HeapEnd-clamped estimate. Real Mac would
            // diverge: FreeMem returns the SUM of free blocks
            // (potentially fragmented), MaxMem returns the
            // largest single contiguous free block (smaller than
            // FreeMem when the heap is fragmented), CompactMem
            // returns the largest block AFTER compacting purgeable
            // resources. In our flat allocator there's no
            // fragmentation to expose, so the three traps share
            // the estimate. Apps that probe these values to gate
            // large allocations see the clamped 24MB-floor as
            // "plenty of memory available" — which lets games
            // that sanity-check 16MB / 24MB minimums proceed.

            // FreeMem ($A01C) — return total free memory in D0
            // FUNCTION FreeMem: LongInt;
            // Inside Macintosh Volume II, II-39
            // FreeMem ($A01C): Per IM:II II-39 returns total free memory in current heap; Systemless returns free_heap_estimate (Systemless doesn't model fragmentation so FreeMem == MaxMem == CompactMem). The 24MB floor applies only to unconstrained launches; explicit app partitions report their actual free span.
            (false, 0x1C) => {
                cpu.write_reg(Register::D0, free_heap_estimate(bus));
                Ok(())
            }

            // MaxMem ($A01D / $A11D — both alias the same arm via
            // the OS-trap low-byte dispatch) — return max
            // contiguous block in D0, total free in A0
            // FUNCTION MaxMem(VAR grow: Size): Size;
            // Inside Macintosh Volume II, II-39
            // MaxMem ($A01D): Per IM:II II-39 returns largest contiguous free block in D0 and total free in A0 (assembly-language convention); Systemless returns free_heap_estimate in D0 and the same value in A0 (no fragmentation modeled, so largest contiguous == total free). $A11D variant (with grow zone) dispatches to the same arm via the OS-trap low-byte (0x1D) decode.
            (false, 0x1D) => {
                let free = free_heap_estimate(bus);
                cpu.write_reg(Register::D0, free);
                cpu.write_reg(Register::A0, free);
                Ok(())
            }

            // CompactMem ($A04C) — try to compact the heap until
            // a contiguous block of cbNeeded bytes is available;
            // return the largest block actually obtained in D0
            // FUNCTION CompactMem(cbNeeded: Size): Size;
            // Inside Macintosh Volume II, II-40
            // CompactMem ($A04C): Per IM:II II-40 takes cbNeeded in D0 and returns the largest free block (after compacting purgeable resources) in D0; Systemless doesn't model heap compaction so the cbNeeded input is IGNORED and returns free_heap_estimate.
            (false, 0x4C) => {
                cpu.write_reg(Register::D0, free_heap_estimate(bus));
                Ok(())
            }

            // SetHandleSize ($A024)
            // Changes the logical size of the relocatable block whose handle is h.
            // PROCEDURE SetHandleSize (h: Handle; newSize: Size);
            // On entry: A0 = handle, D0 = new size
            // On exit: D0 = result code
            // Inside Macintosh Volume II, II-43; Memory 1992, 2-58
            // SetHandleSize ($A024): Resizes handle: allocs new block, copies data, frees old, updates handle ptr
            (false, 0x24) => {
                let handle = cpu.read_reg(Register::A0);
                let new_size = cpu.read_reg(Register::D0);
                if let Some((ptr, res_type, res_id)) = self.loaded_handles.get(&handle).copied() {
                    if &res_type == b".256" {
                        let old_size = bus.get_alloc_size(ptr).unwrap_or(0);
                        eprintln!(
                            "[MEM] SetHandleSize .256 id={} handle=${:08X} ptr=${:08X} {} -> {}",
                            res_id, handle, ptr, old_size, new_size
                        );
                    }
                }
                if handle == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                    return Some(Ok(()));
                }
                let old_ptr = bus.read_long(handle);
                let old_size = bus.get_alloc_size(old_ptr).unwrap_or(0);
                if self.loaded_handles.contains_key(&handle) {
                    let master_was_nil = old_ptr == 0;
                    let old_ptr = if old_ptr != 0 {
                        old_ptr
                    } else {
                        self.loaded_handles
                            .get(&handle)
                            .map(|(ptr, _, _)| *ptr)
                            .unwrap_or(0)
                    };
                    let new_ptr = self.resize_resource_allocation(bus, handle, old_ptr, new_size);
                    if new_ptr == 0 && new_size > 0 {
                        cpu.write_reg(Register::D0, (-108i32) as u32); // memFullErr
                    } else {
                        if master_was_nil && new_ptr != 0 {
                            bus.write_long(handle, new_ptr);
                            self.ptr_to_handle.insert(new_ptr, handle);
                        }
                        cpu.write_reg(Register::D0, 0); // noErr
                    }
                } else if old_size == new_size
                    || (old_ptr != 0
                        && MacMemoryBus::allocation_bucket_size(new_size)
                            == MacMemoryBus::allocation_bucket_size(old_size))
                {
                    // Same allocation bucket — the block can stay put,
                    // but GetHandleSize and any later grow/move must see
                    // the new logical byte count.
                    if new_size < old_size {
                        bus.fill_zeros(old_ptr.wrapping_add(new_size), old_size - new_size);
                    }
                    bus.set_alloc_size(old_ptr, new_size);
                    cpu.write_reg(Register::D0, 0);
                } else {
                    let new_ptr = bus.alloc(new_size);
                    if new_ptr == 0 && new_size > 0 {
                        cpu.write_reg(Register::D0, (-108i32) as u32); // memFullErr
                    } else {
                        // Copy min(old, new) bytes
                        let copy_len = old_size.min(new_size);
                        let bytes = bus.read_bytes(old_ptr, copy_len as usize);
                        bus.write_bytes(new_ptr, &bytes);
                        bus.free(old_ptr);
                        bus.write_long(handle, new_ptr);
                        // Update the ptr→handle map: the handle's data
                        // pointer just moved.
                        self.ptr_to_handle.remove(&old_ptr);
                        self.ptr_to_handle.insert(new_ptr, handle);
                        cpu.write_reg(Register::D0, 0); // noErr
                    }
                }
                Ok(())
            }

            // ReallocateHandle ($A027)
            // Allocates a new relocatable block for a handle whose master pointer is nil
            // (purged or disposed), updating the master pointer to the new block.
            // PROCEDURE ReallocateHandle (h: Handle; logicalSize: Size);
            // Inside Macintosh: Memory 1992, 2-44
            (false, 0x27) => {
                let handle = cpu.read_reg(Register::A0);
                let size = cpu.read_reg(Register::D0);
                let ptr = bus.alloc(size);
                if ptr == 0 && size > 0 {
                    cpu.write_reg(Register::D0, (-108i32) as u32); // memFullErr
                } else {
                    // Drop the old ptr→handle mapping (if any) and
                    // install the new one.
                    let old_ptr = bus.read_long(handle);
                    if old_ptr != 0 {
                        self.ptr_to_handle.remove(&old_ptr);
                    }
                    bus.write_long(handle, ptr);
                    self.ptr_to_handle.insert(ptr, handle);
                    cpu.write_reg(Register::D0, 0);
                }
                Ok(())
            }

            // HandToHand ($A9E1 = toolbox 0x1E1) - register-based calling convention
            // Duplicates a handle: allocates a new relocatable block of the same
            // size and copies the data.
            // FUNCTION HandToHand (VAR theHndl: Handle): OSErr;
            // On entry: A0 = handle to duplicate
            // On exit:  A0 = new handle (copy), D0 = result code
            // Inside Macintosh Volume II, II-374; Memory 1992, 2-62
            // HandToHand ($A9E1): Duplicates handle: A0=src handle in, A0=new handle out, D0=result
            (true, 0x1E1) => {
                let src_handle = cpu.read_reg(Register::A0);
                if src_handle == 0 {
                    cpu.write_reg(Register::A0, 0);
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                    return Some(Ok(()));
                }
                let src_ptr = bus.read_long(src_handle);
                if src_ptr == 0 {
                    cpu.write_reg(Register::A0, 0);
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                    return Some(Ok(()));
                }
                let size = bus.get_alloc_size(src_ptr).unwrap_or(0);
                let new_ptr = bus.alloc(size);
                if new_ptr == 0 && size > 0 {
                    cpu.write_reg(Register::A0, 0);
                    cpu.write_reg(Register::D0, (-108i32) as u32); // memFullErr
                    return Some(Ok(()));
                }
                let bytes = bus.read_bytes(src_ptr, size as usize);
                bus.write_bytes(new_ptr, &bytes);
                let new_handle = bus.alloc(4);
                bus.write_long(new_handle, new_ptr);
                cpu.write_reg(Register::A0, new_handle);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // PtrToXHand ($A9E2 = toolbox 0x1E2) - register-based calling convention
            // Copies data from a pointer into an existing relocatable block.
            // On entry: A0 = source pointer, A1 = destination handle, D0 = size
            // On exit:  A0 = destination handle, D0 = result code
            // Inside Macintosh Volume III, III-38; Memory 1992, 2-61
            // PtrToXHand ($A9E2): Copies ptr data into existing handle: A0=src ptr, A1=dst handle, D0=size
            (true, 0x1E2) => {
                let src_ptr = cpu.read_reg(Register::A0);
                let dst_handle = cpu.read_reg(Register::A1);
                let size = cpu.read_reg(Register::D0);
                cpu.write_reg(Register::A0, dst_handle);
                if dst_handle == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                    return Some(Ok(()));
                }
                // Resize destination handle to fit the data
                let dst_ptr = bus.read_long(dst_handle);
                let old_size = bus.get_alloc_size(dst_ptr).unwrap_or(0);
                let final_ptr = if (old_size + 3) & !3 != (size + 3) & !3 {
                    let new_ptr = bus.alloc(size);
                    if new_ptr == 0 && size > 0 {
                        cpu.write_reg(Register::D0, (-108i32) as u32); // memFullErr
                        return Some(Ok(()));
                    }
                    if dst_ptr != 0 {
                        bus.free(dst_ptr);
                    }
                    bus.write_long(dst_handle, new_ptr);
                    new_ptr
                } else {
                    // PtrToXHand's contract is about the handle's logical
                    // size, not just its aligned backing bucket. When the
                    // resize stays within the same 4-byte bucket (for
                    // example 3 -> 4 or 4 -> 1), update the recorded size
                    // so subsequent GetHandleSize observes the exact byte
                    // count the caller requested.
                    if dst_ptr != 0 {
                        bus.set_alloc_size(dst_ptr, size);
                    }
                    dst_ptr
                };
                let bytes = bus.read_bytes(src_ptr, size as usize);
                bus.write_bytes(final_ptr, &bytes);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // PtrToHand ($A9E3 = toolbox 0x1E3) - register-based calling convention
            // Copies data from a pointer into a new relocatable block.
            // On entry: A0 = source pointer, D0 = size
            // On exit:  A0 = new handle, D0 = result code
            // Inside Macintosh Volume III, III-38; Memory 1992, 2-60
            // PtrToHand ($A9E3): Copies ptr data into new handle: A0=src ptr, D0=size, A0=new handle out
            (true, 0x1E3) => {
                let src_ptr = cpu.read_reg(Register::A0);
                let size = cpu.read_reg(Register::D0);
                let ptr = bus.alloc(size);
                if ptr == 0 && size > 0 {
                    cpu.write_reg(Register::A0, 0);
                    cpu.write_reg(Register::D0, (-108i32) as u32); // memFullErr
                    return Some(Ok(()));
                }
                let bytes = bus.read_bytes(src_ptr, size as usize);
                bus.write_bytes(ptr, &bytes);
                let handle = bus.alloc(4);
                bus.write_long(handle, ptr);
                cpu.write_reg(Register::A0, handle);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // HandAndHand ($A9E4 = toolbox 0x1E4) - register-based calling convention
            // Concatenates the data from one handle onto the end of another.
            // FUNCTION HandAndHand(hand1, hand2: Handle): OSErr;
            // On entry: A0 = source handle (hand1), A1 = destination handle (hand2)
            // On exit:  A0 = destination handle (hand2), D0 = result code
            // Inside Macintosh Volume II, II-374; Memory 1992, 2-63
            // HandAndHand ($A9E4): Appends source handle data to destination handle: A0=src handle, A1=dst handle, D0=result; per IM:II II-374
            (true, 0x1E4) => {
                let src_handle = cpu.read_reg(Register::A0);
                let dst_handle = cpu.read_reg(Register::A1);
                cpu.write_reg(Register::A0, dst_handle);
                if src_handle == 0 || dst_handle == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                    return Some(Ok(()));
                }
                let src_ptr = bus.read_long(src_handle);
                let dst_ptr = bus.read_long(dst_handle);
                if src_ptr == 0 || dst_ptr == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                    return Some(Ok(()));
                }
                let src_size = bus.get_alloc_size(src_ptr).unwrap_or(0);
                let dst_size = bus.get_alloc_size(dst_ptr).unwrap_or(0);
                let new_size = dst_size + src_size;
                // Allocate new block for combined data
                let new_ptr = bus.alloc(new_size);
                if new_ptr == 0 && new_size > 0 {
                    cpu.write_reg(Register::D0, (-108i32) as u32); // memFullErr
                    return Some(Ok(()));
                }
                // Copy existing dst data, then append src data.
                let dst_bytes = bus.read_bytes(dst_ptr, dst_size as usize);
                bus.write_bytes(new_ptr, &dst_bytes);
                let src_bytes = bus.read_bytes(src_ptr, src_size as usize);
                bus.write_bytes(new_ptr.wrapping_add(dst_size), &src_bytes);
                bus.free(dst_ptr);
                bus.write_long(dst_handle, new_ptr);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // PtrAndHand ($A9EF = toolbox 0x1EF) - register-based calling convention
            // Concatenates data referenced by a pointer onto the end of a handle.
            // FUNCTION PtrAndHand(ptr1: Ptr; hand2: Handle; size: LongInt): OSErr;
            // On entry: A0 = source pointer (ptr1), A1 = destination handle (hand2), D0 = size
            // On exit:  A0 = destination handle (hand2), D0 = result code
            // Inside Macintosh Volume II, II-375; Memory 1992, 2-64
            // PtrAndHand ($A9EF): Appends ptr data to destination handle: A0=src ptr, A1=dst handle, D0=size in/result out; per IM:II II-375
            (true, 0x1EF) => {
                let src_ptr = cpu.read_reg(Register::A0);
                let dst_handle = cpu.read_reg(Register::A1);
                let append_size = cpu.read_reg(Register::D0);
                cpu.write_reg(Register::A0, dst_handle);
                if dst_handle == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                    return Some(Ok(()));
                }
                let dst_ptr = bus.read_long(dst_handle);
                if dst_ptr == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                    return Some(Ok(()));
                }
                let dst_size = bus.get_alloc_size(dst_ptr).unwrap_or(0);
                let new_size = dst_size + append_size;
                let new_ptr = bus.alloc(new_size);
                if new_ptr == 0 && new_size > 0 {
                    cpu.write_reg(Register::D0, (-108i32) as u32); // memFullErr
                    return Some(Ok(()));
                }
                // Copy existing dst data, then append source data.
                let dst_bytes = bus.read_bytes(dst_ptr, dst_size as usize);
                bus.write_bytes(new_ptr, &dst_bytes);
                let src_bytes = bus.read_bytes(src_ptr, append_size as usize);
                bus.write_bytes(new_ptr.wrapping_add(dst_size), &src_bytes);
                bus.free(dst_ptr);
                bus.write_long(dst_handle, new_ptr);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // RecoverHandle ($A128)
            // Searches the master pointer table for the handle pointing to p.
            // FUNCTION RecoverHandle (p: Ptr): Handle;
            // Inside Macintosh Volume V, V-579
            //
            // Classic Mac OS relocatable memory has one extra level of
            // indirection that modern allocators usually do not expose. A
            // `Handle` is not the address of an allocation's bytes; it is the
            // stable address of a four-byte *master pointer*. The Memory Manager
            // may move the relocatable data block and update that master pointer
            // while callers continue to retain the same Handle:
            //
            //     Handle ----> master-pointer slot ----> movable data block
            //
            // RecoverHandle performs the reverse operation. Given the data-block
            // pointer, the original Memory Manager scans its master-pointer slots
            // for one whose four-byte contents equal that pointer. Systemless's
            // `ptr_to_handle` map is an index that avoids repeatedly scanning
            // guest memory, but it must preserve the scan's observable semantics.
            //
            // DisposeHandle releases both the data block and its master-pointer
            // slot without immediately clearing the slot's four bytes. Until the
            // slot is reused, RecoverHandle can therefore still find the disposed
            // Handle by its stale contents. Once NewHandle reuses that same slot,
            // however, it overwrites the four bytes with a different data-block
            // pointer. A real scan can no longer find the old pointer. Keeping the
            // old map entry without checking the slot would alias the old pointer
            // to an unrelated, live Handle; code cleaning up the old allocation
            // could then resize or dispose the new allocation. Validate the slot
            // contents here so the cached reverse lookup behaves like the real
            // master-pointer-table scan.
            (false, 0x28) => {
                let ptr = cpu.read_reg(Register::A0);
                let handle = self
                    .ptr_to_handle
                    .get(&ptr)
                    .copied()
                    .filter(|&handle| bus.read_long(handle) == ptr)
                    .unwrap_or(0);
                if handle == 0 {
                    self.ptr_to_handle.remove(&ptr);
                }
                cpu.write_reg(Register::A0, handle);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // SetPtrSize ($A020)
            // Changes the logical size of a nonrelocatable block.
            // On entry: A0 = pointer, D0 = newSize
            // On exit:  D0 = result code (noErr or memFullErr)
            // Inside Macintosh: Memory 1992, 2-44. Critical contract:
            // "SetPtrSize doesn't move the pointer" — the block stays at
            // its current address whether shrinking or growing.
            //
            // The current allocator is a bump allocator with a free-list;
            // in-place grow beyond the original 4-byte-aligned capacity
            // isn't tractable without a compaction pass. For shrink +
            // in-capacity-grow we update the stored logical size and keep
            // the pointer stable. For grow beyond the aligned capacity we
            // return memFullErr.
            // SetPtrSize ($A020): Resizes nonrelocatable block: allocs new, copies data, frees old, updates A0; per IM:II II-38
            (false, 0x20) => {
                let ptr = cpu.read_reg(Register::A0);
                let new_size = cpu.read_reg(Register::D0);
                if ptr == 0 {
                    write_memory_result(cpu, bus, NIL_HANDLE_ERR);
                    return Some(Ok(()));
                }
                let old_size = bus.get_alloc_size(ptr).unwrap_or(0);
                let old_aligned = MacMemoryBus::allocation_bucket_size(old_size);
                let new_aligned = MacMemoryBus::allocation_bucket_size(new_size);
                if new_aligned <= old_aligned {
                    // Fits in the existing block — just update the
                    // logical size and zero the newly "freed" tail so
                    // the block reads clean for any subsequent
                    // SetPtrSize grow.
                    if new_size < old_size {
                        bus.fill_zeros(ptr.wrapping_add(new_size), old_size - new_size);
                    }
                    bus.set_alloc_size(ptr, new_size);
                    write_memory_result(cpu, bus, NO_ERR);
                    return Some(Ok(()));
                }
                // Can't grow beyond aligned capacity without moving —
                // spec requires memFullErr rather than silently
                // moving the pointer.
                write_memory_result(cpu, bus, MEM_FULL_ERR);
                Ok(())
            }

            // GetPtrSize ($A021)
            // Returns the logical size of the nonrelocatable block pointed to by A0.
            // On exit: D0 = size (or negative error code)
            // Inside Macintosh Volume II, II-39
            // GetPtrSize ($A021): Returns actual allocation size via `get_alloc_size`; per IM:II II-39
            (false, 0x21) => {
                let ptr = cpu.read_reg(Register::A0);
                let size = bus.get_alloc_size(ptr).unwrap_or(0);
                cpu.write_reg(Register::D0, size);
                Ok(())
            }

            // ========== Time Manager ==========

            // InsTime ($A058)
            // Installs a task record into the Time Manager queue.
            // PROCEDURE InsTime(tmTaskPtr: QElemPtr);
            // Processes 1994, 3-18
            // TMTask layout: qLink(+0,4) qType(+4,2) tmAddr(+6,4) tmCount(+10,4)
            (false, 0x58) => {
                let task_ptr = cpu.read_reg(Register::A0);
                let tm_addr = bus.read_long(task_ptr + 6);
                // Remove any existing task for the same record address
                self.timer_tasks.retain(|t| t.task_ptr != task_ptr);
                self.timer_tasks.push(super::dispatch::TimerTask {
                    task_ptr,
                    tm_addr,
                    active: false,
                    fire_at_tick: 0,
                    fire_at_subtick: 0,
                    last_fired_tick: None,
                });
                // InsTime clears the qType high-order bit (task inactive until PrimeTime).
                // Processes 1994, 3-12: "InsTime procedure initially clears this bit."
                let q = bus.read_word(task_ptr + 4);
                bus.write_word(task_ptr + 4, q & 0x7FFF);
                Ok(())
            }

            // RmvTime ($A059)
            // Removes a task from the Time Manager queue.
            // PROCEDURE RmvTime(tmTaskPtr: QElemPtr);
            // Processes 1994, 3-19
            (false, 0x59) => {
                let task_ptr = cpu.read_reg(Register::A0);
                self.timer_tasks.retain(|t| t.task_ptr != task_ptr);
                // RmvTime clears the qType high-order bit (task no longer active).
                // Processes 1994, 3-20: "RmvTime sets the high-order bit of the qType field to 0."
                let q = bus.read_word(task_ptr + 4);
                bus.write_word(task_ptr + 4, q & 0x7FFF);
                Ok(())
            }

            // PrimeTime ($A05A)
            // Activates a Time Manager task after a specified delay.
            // PROCEDURE PrimeTime(tmTaskPtr: QElemPtr; count: LongInt);
            // Positive delay = milliseconds, negative = negated microseconds.
            // Processes 1994, 3-19
            (false, 0x5A) => {
                let task_ptr = cpu.read_reg(Register::A0);
                let delay = cpu.read_reg(Register::D0) as i32;
                let current_ticks = bus.read_long(0x016A);
                const SUBTICKS_PER_TICK: u64 = 1_000_000;
                // Convert delay to 60.15 Hz ticks (VBL rate per Guide to Macintosh Family
                // Hardware, 2nd Ed., p. 6-798: "once every 16.63 ms").
                // We use 60 (not 60.15) to keep integer arithmetic exact for common
                // millisecond values.
                // Positive = milliseconds, negative = negated microseconds.
                let delay_subticks = if delay == 0 {
                    SUBTICKS_PER_TICK // Fire ASAP (next tick)
                } else if delay > 0 {
                    (delay as u64) * 60_000
                } else {
                    let us = (-delay) as u64;
                    (us * 60).max(1)
                };
                let current_subtick = self
                    .timer_current_subtick
                    .max(current_ticks as u64 * SUBTICKS_PER_TICK);
                let fire_at_subtick = current_subtick.saturating_add(delay_subticks);
                let fire_at = fire_at_subtick.div_ceil(SUBTICKS_PER_TICK) as u32;
                if let Some(task) = self.timer_tasks.iter_mut().find(|t| t.task_ptr == task_ptr) {
                    task.active = true;
                    task.fire_at_tick = fire_at;
                    task.fire_at_subtick = fire_at_subtick;
                    // Re-read tmAddr in case it changed between InsTime and PrimeTime
                    task.tm_addr = bus.read_long(task_ptr + 6);
                }
                // PrimeTime sets the qType high-order bit (task now active/primed).
                // Processes 1994, 3-20: "PrimeTime sets the high-order bit of the qType field to 1."
                let q = bus.read_word(task_ptr + 4);
                bus.write_word(task_ptr + 4, q | 0x8000);
                Ok(())
            }

            // Microseconds ($A193)
            // Returns the number of microseconds elapsed since system startup.
            // PROCEDURE Microseconds (VAR microTickCount: UnsignedWide);
            //
            // Calling convention (Inside Macintosh: Operating System
            // Utilities 1994, p. 4-49 + p. 6-805 and the MPW Universal
            // Headers Timer.h FOURWORDINLINE form):
            //
            //   Microseconds(UnsignedWide *buf)
            //     FOURWORDINLINE($A193, $225F, $22C8, $2280);
            //
            // After the inline expansion:
            //   1. Caller pushes `buf` onto A7.
            //   2. _Microseconds ($A193) — the trap returns the 64-bit
            //      microsecond count in registers (D0 = low 32 bits,
            //      A0 = high 32 bits). The trap itself does *not* write
            //      through any caller pointer; A0 on entry is scratch.
            //   3. MOVEA.L (A7)+, A1 ($225F) — pop `buf` into A1.
            //   4. MOVE.L A0, (A1)+ ($22C8) — write `hi` to *(buf+0)
            //      and post-increment A1 to buf+4.
            //   5. MOVE.L D0, (A1) ($2280) — write `lo` to *(buf+4).
            //
            // Systemless mirrors the register-return half of the trap; the
            // FOURWORDINLINE glue (emitted at every caller's call site
            // by MPW C) is responsible for storing the result through
            // the caller-supplied buffer. The HLE deliberately does
            // NOT pre-write through A0 because A0 is uninitialised on
            // entry under the FOURWORDINLINE pattern — speculatively
            // writing through it would corrupt unrelated guest memory.
            //
            // 1 tick = 1/60.15 s ≈ 16,625 µs (VBL fires every 16.63 ms
            // per Guide to Macintosh Family Hardware 2nd Ed., p. 6-798;
            // matches what the MPW Universal Headers FOURWORDINLINE
            // glue then materialises into the caller's UnsignedWide
            // buffer with `hi` at offset 0 and `lo` at offset 4).
            // Microseconds ($A193): returns D0=low 32 bits / A0=high 32 bits; per MPW Universal Headers Timer.h FOURWORDINLINE glue, the caller writes the 64-bit count through its UnsignedWide buffer pointer using these register values
            (false, 0x93) => {
                let ticks = bus.read_long(0x016A);
                let usecs = (ticks as u64) * 16_625;
                cpu.write_reg(Register::D0, usecs as u32);
                cpu.write_reg(Register::A0, (usecs >> 32) as u32);
                if trace_entropy_enabled() {
                    eprintln!(
                        "[ENTROPY] Microseconds pc=${:08X} ticks={} usecs={}",
                        cpu.read_reg(Register::PC),
                        ticks,
                        usecs
                    );
                }
                Ok(())
            }

            // ========== Handle state management ==========

            // HPurge ($A049) - mark handle as purgeable
            // Inside Macintosh Volume II, II-41
            // HPurge ($A049): Sets purgeable bit (0x40) in handle_state_bits per IM:II II-41
            (false, 0x49) => {
                let handle = cpu.read_reg(Register::A0);
                if handle != 0 {
                    let bits = self.handle_state_bits.entry(handle).or_insert(0);
                    *bits |= 0x40;
                }
                Ok(())
            }

            // HNoPurge ($A04A) - mark handle as non-purgeable
            // Inside Macintosh Volume II, II-41
            // HNoPurge ($A04A): Clears purgeable bit (0x40) in handle_state_bits per IM:II II-41
            (false, 0x4A) => {
                let handle = cpu.read_reg(Register::A0);
                if let Some(bits) = self.handle_state_bits.get_mut(&handle) {
                    *bits &= !0x40;
                    if *bits == 0 {
                        self.handle_state_bits.remove(&handle);
                    }
                }
                Ok(())
            }

            // HLock ($A029) / HUnlock ($A02A) already handled above

            // HGetState ($A069)
            // Returns a signed byte containing the flags of the master pointer for the given handle.
            // FUNCTION HGetState (h: Handle): SignedByte;
            // Memory 1992, 2-54
            (false, 0x69) => {
                let handle = cpu.read_reg(Register::A0);
                let mut state = self.handle_state_bits.get(&handle).copied().unwrap_or(0);
                if self.loaded_handles.contains_key(&handle) {
                    state |= 0x20; // resource bit
                }
                cpu.write_reg(Register::D0, state as u32);
                Ok(())
            }

            // HSetState ($A06A) - set handle state flags
            // HSetState ($A06A): Sets lock/purge bits (0xC0 mask) in handle_state_bits
            (false, 0x6A) => {
                let handle = cpu.read_reg(Register::A0);
                if handle != 0 {
                    let bits = (cpu.read_reg(Register::D0) as u8) & 0xC0;
                    if bits == 0 {
                        self.handle_state_bits.remove(&handle);
                    } else {
                        self.handle_state_bits.insert(handle, bits);
                    }
                }
                Ok(())
            }

            // HWPriv ($A198)
            // Hardware privilege trap for processor cache control.
            // Selector in D0: 0=SwapInstructionCache, 1=FlushInstructionCache,
            // 2=SwapDataCache, 3=FlushDataCache, 4-6=ExtCache, 9=FlushCodeCacheRange
            // Memory 1992, 4-29
            // HWPriv ($A198): Cache-control selectors (0/2 swap return previous
            // state and update the simulated enable bit; 1/3/4/5/6 flush no-op;
            // 9 range flush no-op) — Systemless has no physical caches but preserves
            // the documented stateful Boolean surface per Memory 1992, 4-29..4-33
            (false, 0x98) => {
                let selector = cpu.read_reg(Register::D0);
                match selector {
                    0 => {
                        // SwapInstructionCache:
                        // cacheEnable arrives in A0 as a register Boolean,
                        // and the previous state is returned in A0 on exit.
                        // The trap returns the previous state and installs the
                        // requested new state.
                        let requested_enabled = cpu.read_reg(Register::A0) != 0;
                        let previous_enabled = self.instruction_cache_enabled;
                        self.instruction_cache_enabled = requested_enabled;
                        let previous = if previous_enabled { 1 } else { 0 };
                        cpu.write_reg(Register::A0, previous);
                        cpu.write_reg(Register::D0, previous);
                    }
                    2 => {
                        // SwapDataCache: same stateful Boolean contract as
                        // SwapInstructionCache, but for the data cache.
                        let requested_enabled = cpu.read_reg(Register::A0) != 0;
                        let previous_enabled = self.data_cache_enabled;
                        self.data_cache_enabled = requested_enabled;
                        let previous = if previous_enabled { 1 } else { 0 };
                        cpu.write_reg(Register::A0, previous);
                        cpu.write_reg(Register::D0, previous);
                    }
                    1 | 3 | 4 | 5 | 6 => {
                        // Flush/enable/disable caches — no-op in emulation
                        cpu.write_reg(Register::D0, 0);
                    }
                    9 => {
                        // FlushCodeCacheRange: A0=address, A1=count
                        cpu.write_reg(Register::D0, 0); // noErr
                    }
                    _ => {
                        eprintln!("[TRAP] HWPriv: unknown selector {}", selector);
                        cpu.write_reg(Register::D0, 0);
                    }
                }
                Ok(())
            }

            // ========== Device Manager ==========

            // _Control ($A004)
            // Device Manager control call. Register-based: A0 = param block ptr.
            // FUNCTION PBControl (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // Inside Macintosh: Devices 1994, p. 1-16
            // _Control ($A004): Handles cscSetMode (csCode=2) by returning
            // page/base address through the inline csParam VDPgInfo record
            // and cscSetEntries (csCode=3) via VDSetEntryRecord; returns noErr.
            (false, 0x04) => {
                let pb = cpu.read_reg(Register::A0);
                if pb != 0 {
                    let cs_code = bus.read_word(pb + 26) as i16;
                    // cscSetMode (csCode=2): switch mode/page and return base address.
                    // Low-level PBControl stores the VDPgInfo record inline in
                    // CntrlParam.csParam, whose documented layout is short[11].
                    // VDPgInfo layout:
                    //   csMode [word] @ +0
                    //   csData [long] @ +2
                    //   csPage [word] @ +6
                    //   csBaseAddr [long] @ +8
                    // Designing Cards and Drivers 3rd Ed 1992, p. 219, 235
                    // Devices 1994, p. 2-128, 6-68
                    if cs_code == 2 {
                        let vdpg_info = pb + 28;
                        let cs_mode = bus.read_word(vdpg_info) as i16;
                        let cs_data = bus.read_long(vdpg_info + 2);
                        let cs_page = bus.read_word(vdpg_info + 6) as i16;
                        let mut screen_base = self.screen_mode.0;
                        if screen_base == 0 {
                            let gdh = self.ensure_main_gdevice(bus);
                            if gdh != 0 {
                                let gd = bus.read_long(gdh);
                                if gd != 0 {
                                    let pm_handle = bus.read_long(gd + 22); // gdPMap
                                    if pm_handle != 0 {
                                        let pm = bus.read_long(pm_handle);
                                        if pm != 0 {
                                            screen_base = bus.read_long(pm);
                                        }
                                    }
                                }
                            }
                        }

                        if trace_video_driver_enabled() {
                            eprintln!(
                                "[VIDEO] cscSetMode mode={} data=${:08X} page={} screen_mode=({}, {}, {}, {}, {})",
                                cs_mode,
                                cs_data,
                                cs_page,
                                self.screen_mode.0,
                                self.screen_mode.1,
                                self.screen_mode.2,
                                self.screen_mode.3,
                                self.screen_mode.4,
                            );
                        }

                        let vdpg_end = vdpg_info.saturating_add(12);
                        let frame_sp = cpu.read_reg(Register::A7);
                        let frame_a6 = cpu.read_reg(Register::A6);
                        let fits_current_stack_frame = !(frame_sp <= vdpg_info
                            && vdpg_info < frame_a6)
                            || vdpg_end <= frame_a6;

                        if fits_current_stack_frame {
                            // Single-screen HLE currently exposes page 0.
                            bus.write_word(vdpg_info + 6, 0); // csPage
                            bus.write_long(vdpg_info + 8, screen_base); // csBaseAddr
                        } else if trace_video_driver_enabled() {
                            eprintln!(
                                "[VIDEO] cscSetMode output skipped: inline VDPgInfo ${:08X}..${:08X} exceeds stack frame ${:08X}..${:08X}",
                                vdpg_info, vdpg_end, frame_sp, frame_a6
                            );
                        }
                    }

                    // cscSetEntries (csCode=3): update device CLUT via video driver.
                    // csParam[0..3] stores a Ptr to a VDSetEntryRecord on the
                    // caller's stack (see cseries.lib devices.c LowLevelSetEntries).
                    // VDSetEntryRecord layout (as compiled by MPW C for 68k):
                    //   csTable:  Ptr      {+0, pointer to ColorSpec array}
                    //   csStart:  INTEGER  {+4, first entry, or -1 for indexed mode}
                    //   csCount:  INTEGER  {+6, number of entries minus 1}
                    // Designing Cards and Drivers 3rd Ed 1992, p. 245-248
                    // Inside Macintosh: Devices 1994, p. 1-16 (CntrlParam.csParam)
                    if cs_code == 3 {
                        let trap_pc = cpu.read_reg(Register::PC).wrapping_sub(2);
                        let vd_ptr = bus.read_long(pb + 28);
                        let cs_table = bus.read_long(vd_ptr);
                        let cs_start = bus.read_word(vd_ptr + 4) as i16;
                        let cs_count = bus.read_word(vd_ptr + 6) as i16;
                        // Clamp to valid range for 8-bit device (0..255).
                        // csCount can contain stale stack data; treat negative
                        // values as full-CLUT (255) and cap oversized values.
                        let safe_count = if cs_count < 0 { 255 } else { cs_count.min(255) };
                        if trace_video_driver_enabled() {
                            eprintln!(
                                "[VIDEO] cscSetEntries table=${:08X} start={} count={} safe_count={}",
                                cs_table, cs_start, cs_count, safe_count
                            );
                            let preview_entries = (safe_count as u32 + 1).min(4);
                            for i in 0..preview_entries {
                                let entry_addr = cs_table + i * 8;
                                eprintln!(
                                    "[VIDEO]   spec[{}] value={} rgb=({:04X},{:04X},{:04X})",
                                    i,
                                    bus.read_word(entry_addr) as i16,
                                    bus.read_word(entry_addr + 2),
                                    bus.read_word(entry_addr + 4),
                                    bus.read_word(entry_addr + 6),
                                );
                            }
                            for i in [43u32, 100, 150, 185, 220, 245] {
                                if i > safe_count as u32 {
                                    continue;
                                }
                                let entry_addr = cs_table + i * 8;
                                eprintln!(
                                    "[VIDEO]   spec[{}] value={} rgb=({:04X},{:04X},{:04X})",
                                    i,
                                    bus.read_word(entry_addr) as i16,
                                    bus.read_word(entry_addr + 2),
                                    bus.read_word(entry_addr + 4),
                                    bus.read_word(entry_addr + 6),
                                );
                            }
                        }
                        // Apply palette BEFORE logging so screenshots see updated CLUT.
                        self.apply_set_entries(bus, cs_table, cs_start, safe_count);
                        if let Err(err) = self.record_trace_event(
                            bus,
                            trap_pc,
                            "video_set_entries",
                            Self::trace_palette_field_map(bus, cs_table, cs_start, safe_count),
                            true,
                        ) {
                            return Some(Err(err));
                        }
                    }

                    // cscSetGamma (csCode=4): install a per-channel gamma
                    // correction table. csParam[0..3] stores a Ptr to a
                    // GammaTbl in Pascal format (version, type, formula size,
                    // chan count, data count, data width, data bytes).
                    //
                    // Systemless does NOT yet install a runtime gamma — we have
                    // a fixed Mac HiRes linear-interpolated gamma LUT in
                    // `display.rs`. Stub returns noErr.
                    //
                    // Designing Cards and Drivers 3rd Ed 1992, p. 245-248
                    // Inside Macintosh: Devices 1994, p. 6-71
                    if cs_code == 4 && trace_video_driver_enabled() {
                        let gamma_ptr = bus.read_long(pb + 28);
                        let g_version = if gamma_ptr != 0 {
                            bus.read_word(gamma_ptr) as i16
                        } else {
                            -1
                        };
                        let g_type = if gamma_ptr != 0 {
                            bus.read_word(gamma_ptr + 2) as i16
                        } else {
                            -1
                        };
                        let g_formula_size = if gamma_ptr != 0 {
                            bus.read_word(gamma_ptr + 4) as i16
                        } else {
                            -1
                        };
                        let g_chan_cnt = if gamma_ptr != 0 {
                            bus.read_word(gamma_ptr + 6) as i16
                        } else {
                            -1
                        };
                        let g_data_cnt = if gamma_ptr != 0 {
                            bus.read_word(gamma_ptr + 8) as i16
                        } else {
                            -1
                        };
                        let g_data_width = if gamma_ptr != 0 {
                            bus.read_word(gamma_ptr + 10) as i16
                        } else {
                            -1
                        };
                        eprintln!(
                                "[VIDEO] cscSetGamma gamma_ptr=${:08X} version={} type={} formulaSize={} chanCnt={} dataCnt={} dataWidth={}",
                                gamma_ptr,
                                g_version,
                                g_type,
                                g_formula_size,
                                g_chan_cnt,
                                g_data_cnt,
                                g_data_width,
                            );
                        // Preview first 16 data bytes for confirmation
                        if gamma_ptr != 0 && g_data_cnt > 0 && g_data_width == 8 {
                            let data_base = gamma_ptr + 12 + g_formula_size.max(0) as u32;
                            let preview_n = g_data_cnt.min(16) as u32;
                            let mut bytes = Vec::with_capacity(preview_n as usize);
                            for i in 0..preview_n {
                                bytes.push(bus.read_byte(data_base + i));
                            }
                            eprintln!("[VIDEO]   gamma data[0..{}] = {:02X?}", preview_n, bytes);
                        }
                    }
                    // Succeed without applying — keeps Marathon from
                    // thinking its gamma setup failed. Actual gamma
                    // application pending full implementation.
                    bus.write_word(pb + 16, 0); // ioResult = noErr
                }
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PBStatus (_Status, 0xA005)
            // Invokes the Status routine of the driver whose reference number is in
            // the ioRefNum field of the parameter block.
            // FUNCTION PBStatus (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // Inside Macintosh: Devices (1994), pp. 1-79 to 1-80;
            // Inside Macintosh Volume II (1985), p. II-189.
            //
            // Per IM:Devices 1994 p. 1-80 + IM:II 1985 p. II-189 the trap
            // is an OS-bit FUNCTION (bit 11 of the trap word is clear)
            // with a register-only ABI:
            //
            //   Registers on entry:
            //     A0  Pointer to the parameter block (ParmBlkPtr)
            //
            //   Registers on exit:
            //     D0  Result code (OSErr)
            //
            // No Pascal stack argument frame is consumed.
            //
            // Per IM:II 1985 p. II-114 (Device Manager parameter-block
            // dispatcher convention), the OSErr returned by the driver
            // Status routine is mirrored into BOTH D0 AND the ioResult
            // field of the parameter block at offset +16:
            //
            //   ioCompletion @ +12  (NIL for synchronous calls)
            //   ioResult     @ +16  (function result mirrored from D0)
            //   ioNamePtr    @ +18  (unused by PBStatus)
            //   ioVRefNum    @ +22  (unused by PBStatus)
            //   ioRefNum     @ +24  (driver reference number)
            //
            // MPW Universal Headers Devices.h declares:
            //   #pragma parameter __D0 PBStatusSync(__A0)
            //   EXTERN_API(OSErr) PBStatusSync(ParmBlkPtr paramBlock)
            //                                  ONEWORDINLINE(0xA005);
            // plus #define PBStatus(pb,async) macro dispatching to
            // PBStatusSync/PBStatusAsync. The Async ($A405) and Immed
            // ($A205) variants share the same A0/D0 register convention.
            //
            // Systemless HLE compromise: the single-threaded synchronous-I/O
            // HLE has no installed Device Manager drivers and no Status
            // routines to invoke. The trap returns noErr (0) for the
            // zero/refNum-cleared path and collapses any non-zero ioRefNum
            // to badUnitErr (-21), mirroring that into pb.ioResult. That
            // keeps the dispatcher convention (D0 == ioResult) intact while
            // making the bogus-refnum path visibly non-successful.
            //
            // Apple-vs-BasiliskII engine divergence: the absolute OSErr
            // returned for a bogus ioRefNum (e.g. 9999) diverges between
            // engines — Systemless collapses the miss to badUnitErr (-21),
            // while BasiliskII dispatches the real ROM trap and is expected
            // to return badUnitErr (-21) or unitEmptyErr (-22) for a refNum
            // that does not map to an installed driver. Both engines obey
            // the dispatcher convention writing the SAME value to BOTH D0
            // and ioResult, so the observable invariant is "D0 == ioResult
            // AND ioResult != pre-poison sentinel" rather than an absolute
            // OSErr value.
            //
            // Regression coverage:
            //   src/trap/memory.rs::status_writes_ioresult_and_returns_noerr_in_d0
            //   src/trap/memory.rs::status_nil_paramblock_returns_noerr_and_preserves_stack
            //   src/trap/memory.rs::pbstatus_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            (false, 0x05) => {
                let pb = cpu.read_reg(Register::A0);
                if pb != 0 {
                    let io_ref_num = bus.read_word(pb + 24);
                    let result = if io_ref_num == 0 {
                        NO_ERR
                    } else if self.synthetic_drivers.contains_key(&io_ref_num) {
                        NO_ERR
                    } else {
                        BAD_UNIT_ERR
                    };
                    bus.write_word(pb + 16, result as u16); // ioResult mirrors D0
                    cpu.write_reg(Register::D0, result);
                    return Some(Ok(()));
                }
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // InitZone ($A019)
            // PROCEDURE InitZone(pGrowZone: ProcPtr; cMoreMasters: INTEGER;
            //                    limitPtr, startPtr: Ptr);
            // Inside Macintosh: Memory (1992), 2-86..2-87
            //
            // Assembly-language calling convention (IM:Memory 2-87):
            //   On entry: A0 = pointer to InitZone parameter block
            //   On exit:  D0 = result code
            //
            // Systemless does not implement secondary heap zones (single flat
            // allocator), so InitZone is a no-op that returns noErr while
            // preserving A7.
            //
            // Regression coverage:
            //   src/trap/memory.rs::initzone_uses_a0_parameter_block_and_returns_noerr_in_d0
            //   src/trap/memory.rs::initzone_initializes_zone_header_and_makes_startptr_current_zone
            //   src/trap/memory.rs::initzone_uses_register_calling_convention_without_stack_arguments
            // InitZone ($A019): Initializes the observable zone header
            // from the A0 parameter block, makes startPtr current via
            // TheZone, and returns D0=noErr; no stack-argument pop
            // (IM:Memory 1992 2-87)
            (false, 0x19) => {
                use crate::memory::globals::addr;
                let param_block = cpu.read_reg(Register::A0);
                let start = bus.read_long(param_block);
                let limit = bus.read_long(param_block + 4);
                let more_masters = bus.read_word(param_block + 8);
                let grow_zone = bus.read_long(param_block + 10);
                init_zone_header(bus, start, limit, more_masters, grow_zone);
                bus.write_long(addr::THE_ZONE, start);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // SetDateTime ($A03A)
            // Sets the date and time in the clock chip and the Time global.
            // FUNCTION SetDateTime(secs: LongInt): OSErr;
            // Inside Macintosh Volume II (1985), pp. II-378 to II-379 and II-391;
            // Operating System Utilities (1994), pp. 4-36 to 4-37
            //
            // Per IM:II II-378 + II-391 the trap is an OS-bit FUNCTION
            // (bit 11 of the trap word is clear) with a register-only
            // ABI:
            //
            //   Registers on entry:
            //     D0  Seconds since 1904-01-01 00:00:00 (LONGINT)
            //
            //   Registers on exit:
            //     D0  Result code (noErr 0 | clkWrErr | clkRdErr)
            //
            // No Pascal stack argument frame is consumed and no result
            // slot is allocated on the stack — D0 carries both the
            // input and the OSErr result.
            //
            // MPW Universal Headers DateTimeUtils.h declares:
            //   #pragma parameter __D0 SetDateTime(__D0)
            //   EXTERN_API(OSErr) SetDateTime(unsigned long time)
            //                                   ONEWORDINLINE(0xA03A);
            // which maps the single C argument and the FUNCTION result
            // to D0.
            //
            // Per IM:II II-378: "SetDateTime sets the current date and
            // time in the clock chip and also sets the Time global
            // variable." Systemless writes D0 to the Time global ($020C)
            // and returns D0 = noErr.
            //
            // Apple-vs-BasiliskII divergence: BasiliskII's VBL interrupt
            // fires every ~16ms and overwrites $020C with the host
            // clock. Even a sub-microsecond read of $020C immediately
            // after _SetDateTime returns the host clock value, not the
            // value passed in. This Time-global write semantic is
            // pinned via the in-Rust test
            // src/trap/memory.rs::setdatetime_updates_time_global_from_d0_seconds_argument.
            //
            // The register-only OS-bit FUNCTION calling convention plus
            // noErr return on the nominal call path is the observable
            // behavior.
            //
            // Regression coverage:
            //   src/trap/memory.rs::setdatetime_updates_time_global_from_d0_seconds_argument
            //   src/trap/memory.rs::setdatetime_returns_noerr_result_code_in_d0_for_nominal_call
            //   src/trap/memory.rs::setdatetime_returns_noerr_regardless_of_secs_input
            (false, 0x3A) => {
                let secs = cpu.read_reg(Register::D0);
                bus.write_long(0x020C, secs); // Time global ($020C)
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // WriteParam ($A038)
            // Writes the low-memory copy of parameter RAM to the clock chip.
            // FUNCTION WriteParam: OSErr;
            // Inside Macintosh Volume II (1985), pp. II-381 to II-382 and p. II-391
            // Operating System Utilities (1994), pp. 7-12 to 7-13
            //
            // Per IM:II II-381 + IM:OSUtils 1994 p. 7-13 the trap is an
            // OS-bit FUNCTION (bit 11 of the trap word is clear) with a
            // register-only ABI:
            //
            //   Registers on entry:
            //     A0  SysParam     (pointer to low-memory copy of parameter
            //                       RAM at the global $01F8)
            //     D0  MinusOne     (long word $FFFFFFFF — "you have to pass
            //                       the values of these global variables
            //                       for historical reasons")
            //
            //   Registers on exit:
            //     D0  Result code  (noErr 0 | prWrErr -87)
            //
            // No Pascal stack argument frame is consumed and no result slot
            // is allocated on the stack — A0 and D0 carry both inputs, and
            // D0 carries the OSErr result.
            //
            // MPW Universal Headers OSUtils.h declares:
            //     EXTERN_API(OSErr) WriteParam(void);
            // exposed via InterfaceLib only with NO ONEWORDINLINE glue
            // published in the public header. Callers either link through
            // InterfaceLib (which performs the A0/D0 register setup) or use
            // a local `#pragma parameter __D0 kx_WriteParam(__A0,
            // __D0)` inline thunk that emits the trap word directly with
            // the documented register inputs.
            //
            // Documented contract per IM:OSUtils 1994 p. 7-13:
            //   "The WriteParam function writes the modified values in the
            //    system parameters record to parameter RAM. ... The
            //    WriteParam function also attempts to verify the values
            //    written by reading them back in and comparing them to the
            //    values in the low-memory copy."
            //
            // The 20-byte low-memory SysParam record at $01F8 is the SOURCE
            // the trap propagates OUT to the clock chip and the reference
            // it compares against on the verify path; the trap never
            // modifies the low-memory copy.
            //
            // Observable behavior (BasiliskII reference):
            //   - D0 = noErr (0) on nominal exit. BII's emulated clock
            //     chip and SysParam agree on a freshly-booted system, so
            //     the verify path succeeds.
            //   - SysParam bytes at $01F8..$020B byte-for-byte preserved
            //     across the call. The Systemless HLE doesn't touch them;
            //     BII reads them on the write+verify path without writing
            //     back.
            //   - A7 unchanged across the call (register-only ABI; no
            //     Pascal stack frame). Covered by in-Rust test
            //     `writeparam_returns_noerr_in_d0_for_nominal_call`.
            //
            // BII-vs-Systemless divergence: BII propagates the SysParam record
            // to its emulated clock chip and verifies; Systemless returns
            // D0=0 unconditionally without modeling the clock chip (per
            // IM:II II-381 historical note, the low-memory SysParam copy
            // is the canonical store anyway).
            //
            //   src/trap/memory.rs:tests::writeparam_returns_noerr_in_d0_for_nominal_call
            //   src/trap/memory.rs:tests::writeparam_does_not_modify_low_memory_sysparam_copy
            //   src/trap/memory.rs:tests::writeparam_five_call_composition_preserves_stack_across_varying_minusone_register_state
            // WriteParam ($A038): No-op in emulator (no clock chip); returns noErr per IM:II II-381 + IM:OSUtils 1994 p. 7-13
            (false, 0x38) => {
                // In our emulator there is no real clock chip.
                // Just return noErr — the low-memory copy at SysParam ($01F8)
                // is already the canonical store.
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // InitUtil ($A03F)
            // Reads parameter RAM out of the clock chip into the 20-byte
            // SysParam record at low-memory $01F8 and stamps the first
            // byte (SPValid) with $A8 to mark the in-memory copy as
            // authoritative. Per IM:II II-380 + IM:OSUtils 1994 p. 7-9
            // this is the canonical "initialize parameter RAM" entry
            // point invoked once during boot.
            //
            // FUNCTION InitUtil: OSErr;     {trap macro _InitUtil $A03F}
            //
            // Inside Macintosh Volume II (1985), pp. II-375, II-380 to
            // II-381, II-391; Operating System Utilities (1994),
            // pp. 7-8 to 7-10.
            //
            // OS-bit FUNCTION (bit 11 of the trap word is clear) with
            // a register-only ABI per IM:OSUtils 1994 p. 7-9:
            //   Registers on entry:  (none)
            //   Registers on exit:   D0 = OSErr (0 noErr | -88 prInitErr)
            // No Pascal stack frame is consumed and no result slot is
            // allocated on the stack — D0 alone carries the result.
            //
            // MPW Universal Headers OSUtils.h declares InitUtil as
            //   EXTERN_API(OSErr) InitUtil(void)
            // exposed via InterfaceLib only; no ONEWORDINLINE glue is
            // published in the public header. Callers dispatch the trap
            // via an inline thunk such as
            //   #pragma parameter __D0 kx_InitUtil()
            //   pascal long kx_InitUtil(void) = {0xA03F};
            // mapping the FUNCTION result slot to D0.
            //
            // SPValid sentinel: per IM:II II-375 + II-380 the first
            // byte of SysParam is the validity stamp. $A8 means
            // "parameter RAM has been validated by InitUtil since the
            // last reset"; any other value means "invalid" and apps
            // must invoke InitUtil before relying on SysParam contents.
            //
            // BII-vs-Systemless divergence: BII System 7.5.3 ROM reads
            // the host-emulated clock chip and propagates the full
            // 20-byte record into low memory before stamping SPValid.
            // Systemless HLE writes only the SPValid stamp (the rest of
            // SysParam is initialised by other paths or remains zero);
            // both produce the documented post-conditions
            // (D0 = noErr and SPValid = $A8 on a freshly-booted
            // default-configuration system).
            //
            // Regression coverage:
            //   src/trap/memory.rs:initutil_returns_noerr_in_d0_for_nominal_call
            //   src/trap/memory.rs:initutil_sets_spvalid_byte_to_a8_on_success
            //   src/trap/memory.rs:initutil_rewrites_spvalid_when_pre_poisoned_to_invalid
            (false, 0x3F) => {
                // Mark parameter RAM as valid: SPValid ($01F8) = $A8
                bus.write_byte(0x01F8, 0xA8);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // UprString ($A054)
            // Converts lowercase letters in a string to uppercase.
            // PROCEDURE UprString(VAR theString: Str255; diacSens: BOOLEAN);
            // Inside Macintosh Volume II, II-377
            //
            // Assembly calling convention:
            //   On entry: A0 = pointer to first character, D0.W = length
            //   On exit:  A0 = pointer to first character (unchanged)
            //   Bit 9 of trap word: if set, diacSens=FALSE (strip diacriticals)
            //
            // UprString ($A054): Converts lowercase→uppercase in place; bit 9=strip diacriticals; per IM:II II-377
            (false, 0x54) => {
                let ptr = cpu.read_reg(Register::A0);
                let len = cpu.read_reg(Register::D0) & 0xFFFF;
                let strip_marks = (self.current_trap_word & 0x0200) != 0;
                for i in 0..len {
                    let ch = bus.read_byte(ptr + i);
                    let upper = mac_roman_to_upper(ch, strip_marks);
                    bus.write_byte(ptr + i, upper);
                }
                Ok(())
            }

            // LowerText ($A056) / StripText ($A256) / UpperText ($A456) /
            // StripUpperText ($A656)
            // Text conversion utilities selected by trap-word variants.
            // Inside Macintosh Volume VI (1991), 14-62 to 14-63 and
            // Appendix C table C-2 (C-3); Mac Roman accent table per
            // Inside Macintosh Volume IV (1985), IV-235.
            //
            // Per IM:VI 14-62 ("Call shape" line for each variant):
            //   On entry:  A0 = pointer to first character of text buffer
            //              D0.W = byte length of buffer (zero-extended to D0.L)
            //   On exit:   D0 = result code (noErr = 0, resNotFound = -192)
            //
            // Bits 9-10 of the dispatched trap word select the operation
            // per IM:VI Appendix C table C-2 (C-3):
            //   bit 10 = 0, bit 9 = 0 → $A056 LowerText
            //   bit 10 = 0, bit 9 = 1 → $A256 StripText
            //   bit 10 = 1, bit 9 = 0 → $A456 UpperText
            //   bit 10 = 1, bit 9 = 1 → $A656 StripUpperText
            //
            // MPW Universal Headers (`TextUtils.h`) declare each variant
            // as ONEWORDINLINE with `#pragma parameter Foo(__A0, __D0)`:
            //   pascal void LowerText(Ptr textPtr, short len)        = { 0xA056 };
            //   pascal void StripText(Ptr textPtr, short len)        = { 0xA256 };
            //   pascal void UpperText(Ptr textPtr, short len)        = { 0xA456 };
            //   pascal void StripUpperText(Ptr textPtr, short len)   = { 0xA656 };
            // The void return type elides D0 from the C side. Checking the
            // byte output rather than D0 keeps the checks insensitive to
            // the IM:VI 14-63 "checking D0 is unreliable on System 7
            // PowerMacs" caveat.
            //
            // Coverage in this file:
            //   lowertext_family_returns_noerr_in_d0_for_each_trap_word_variant
            //
            // LowerText/UpperText/StripText/StripUpperText ($A056): text
            // conversion utility family selected by trap-word variants
            // ($A056/$A256/$A456/$A656); D0 returns result code (noErr or
            // resNotFound) per IM:VI 14-62 to 14-63.
            (false, 0x56) => {
                let ptr = cpu.read_reg(Register::A0);
                let len = cpu.read_reg(Register::D0) & 0xFFFF;
                let bits = (self.current_trap_word >> 9) & 0x03;
                let do_upper = (bits & 0x02) != 0;
                let do_strip = (bits & 0x01) != 0;
                let do_lower = bits == 0x00;
                for i in 0..len {
                    let ch = bus.read_byte(ptr + i);
                    let result = if do_upper && do_strip {
                        mac_roman_to_upper(ch, true)
                    } else if do_upper {
                        mac_roman_to_upper(ch, false)
                    } else if do_strip {
                        mac_roman_strip_diacriticals(ch)
                    } else if do_lower {
                        mac_roman_to_lower(ch)
                    } else {
                        ch
                    };
                    bus.write_byte(ptr + i, result);
                }
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // RelString ($A050)
            // Compares two strings relationally, returning -1, 0, or 1.
            // FUNCTION RelString(aStr, bStr: Str255; caseSens, diacSens: BOOLEAN): INTEGER;
            // Inside Macintosh Volume IV, IV-234
            //
            // Assembly calling convention:
            //   On entry: A0 = ptr to first string, A1 = ptr to second string
            //             D0 high word = length of first, D0 low word = length of second
            //   On exit:  D0 = -1 (less), 0 (equal), or 1 (greater) as long word
            //   Bit 9: if set, diacSens=FALSE (strip diacriticals)
            //   Bit 10: if set, caseSens=TRUE
            //
            // RelString ($A050): Compares strings relationally: A0/A1=ptrs, D0=lengths; bit 9=strip marks, bit 10=case-sensitive; per IM:IV IV-234
            (false, 0x50) => {
                let ptr_a = cpu.read_reg(Register::A0);
                let ptr_b = cpu.read_reg(Register::A1);
                let d0 = cpu.read_reg(Register::D0);
                let len_a = (d0 >> 16) & 0xFFFF;
                let len_b = d0 & 0xFFFF;
                let strip_marks = (self.current_trap_word & 0x0200) != 0;
                let case_sens = (self.current_trap_word & 0x0400) != 0;

                let min_len = std::cmp::min(len_a, len_b);
                let mut result: i32 = 0;
                for i in 0..min_len {
                    let mut a = bus.read_byte(ptr_a + i);
                    let mut b = bus.read_byte(ptr_b + i);
                    if strip_marks {
                        a = mac_roman_strip_diacriticals(a);
                        b = mac_roman_strip_diacriticals(b);
                    }
                    if !case_sens {
                        a = mac_roman_to_upper(a, false);
                        b = mac_roman_to_upper(b, false);
                    }
                    if a < b {
                        result = -1;
                        break;
                    } else if a > b {
                        result = 1;
                        break;
                    }
                }
                if result == 0 {
                    if len_a < len_b {
                        result = -1;
                    } else if len_a > len_b {
                        result = 1;
                    }
                }
                cpu.write_reg(Register::D0, result as u32);
                Ok(())
            }

            // Translate24To32 ($A091)
            // Translates a 24-bit address to a 32-bit clean address.
            // Inside Macintosh Volume VI, 28-10
            //
            // Assembly calling convention:
            //   On entry: D0 = 24-bit address
            //   On exit:  D0 = 32-bit address
            //
            // BasiliskII's default System 7.5.3 boot runs in 32-bit
            // addressing mode and returns the full D0 input unchanged
            // for tagged values like 0xAB123456. Systemless follows that
            // operational behavior so 32-bit-clean guests see the same
            // identity result.
            //
            // Regression coverage:
            //   src/trap/memory.rs::translate24to32_preserves_full_input_in_32bit_mode
            //   src/trap/memory.rs::translate24to32_uses_d0_register_calling_convention_and_preserves_a7
            // Translate24To32 ($A091): Register-only D0-in/D0-out call;
            // current 32-bit-mode convergence target returns the full
            // D0 input unchanged.
            (false, 0x91) => {
                let addr = cpu.read_reg(Register::D0);
                cpu.write_reg(Register::D0, addr);
                Ok(())
            }

            // ReadXPRam ($A051)
            // PROCEDURE ReadXPRam (count: INTEGER; whichByte: INTEGER; destPtr: Ptr);
            // Reads `count` bytes from extended parameter RAM (the 236-byte
            // region beyond the 20-byte standard SysParam record) starting
            // at byte offset `whichByte` in the 256-byte PRAM record, into
            // the caller-supplied destination buffer at `destPtr`.
            // Inside Macintosh Volume V (1986), p. V-519; Operating System
            // Utilities (1994), pp. 7-1 to 7-14.
            //
            // OS-bit FUNCTION (bit 11 of the trap word is clear) with a
            // register-only ABI per IM:V V-519:
            //
            //     Registers on entry:
            //       D0  packed long: (count << 16) | whichByte_offset
            //       A0  destPtr (caller's destination buffer)
            //
            //     Registers on exit:
            //       D0  Result code (noErr 0 on the nominal path)
            //
            // No Pascal stack frame is consumed and no result slot is
            // allocated on the stack — A0 and D0 carry both inputs and
            // D0 carries the OSErr result. Per IM:OSUtils 1994 p. 7-3
            // WARNING block: applications "should rarely use them
            // [_ReadXPRam / _WriteXPRam]; instead, use the appropriate
            // Toolbox routines to indirectly manipulate values in
            // parameter RAM" (e.g. ReadLocation per IM:OSUtils 1994
            // p. 7-12).
            //
            // The MPW Universal Headers expose the trap word as the
            // `_ReadXPRam = 0xA051` constant in Traps.h only — there is
            // no public EXTERN_API declaration in OSUtils.h or Memory.h.
            // Callers dispatch it via an inline thunk such as
            // `#pragma parameter __D0 kx_ReadXPRam(__D0, __A0)`.
            //
            // BII-vs-Systemless divergence: BasiliskII reads the actual
            // emulated clock-chip XPRAM bytes; the Systemless HLE here
            // zero-fills the destination because no persistent extended
            // PRAM is modeled. Both engines agree on the documented
            // OS-bit register convention + noErr return on the
            // default-configuration nominal-call path.
            //
            // In-Rust contract tests:
            //   readxpram_zero_fills_requested_count_and_returns_noerr
            //   readxpram_uses_d0_count_offset_and_a0_destptr_register_calling_convention
            //   readxpram_five_call_composition_preserves_stack_across_varying_count_offset
            (false, 0x51) => {
                let d0 = cpu.read_reg(Register::D0);
                let count = (d0 >> 16) & 0xFFFF;
                let ptr = cpu.read_reg(Register::A0);
                for i in 0..count {
                    bus.write_byte(ptr + i, 0);
                }
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // WriteXPRam ($A052)
            // PROCEDURE WriteXPRam (count: INTEGER; whichByte: INTEGER; srcPtr: Ptr);
            // Writes `count` bytes from the caller's source buffer at
            // `srcPtr` into extended parameter RAM starting at byte
            // offset `whichByte` in the 256-byte PRAM record.
            // Inside Macintosh Volume V (1986), p. V-519; Operating System
            // Utilities (1994), pp. 7-1 to 7-14.
            //
            // OS-bit FUNCTION with the same register convention as
            // ReadXPRam, except A0 is the SOURCE pointer:
            //
            //     Registers on entry:
            //       D0  packed long: (count << 16) | whichByte_offset
            //       A0  srcPtr (caller's source buffer; READ-ONLY)
            //
            //     Registers on exit:
            //       D0  Result code (noErr 0 on the nominal path)
            //
            // BII-vs-Systemless divergence: BasiliskII writes the source
            // bytes to its emulated clock-chip XPRAM and verifies by
            // readback; the Systemless HLE here is a no-op because no
            // persistent extended PRAM is modeled. Both engines preserve
            // the caller's source buffer (read-only access) and agree on
            // the documented OS-bit register convention + noErr return.
            //
            // In-Rust contract tests:
            //   writexpram_noop_returns_noerr_in_hle_without_persistent_xpram
            //   writexpram_uses_d0_count_offset_and_a0_srcptr_register_calling_convention
            //   writexpram_five_call_composition_preserves_stack_and_source_across_varying_count_offset
            (false, 0x52) => {
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // GetDefaultStartup ($A07D)
            // Reads the default startup device information.
            // PROCEDURE GetDefaultStartup(VAR paramBlock: DefStartRec);
            // Inside Macintosh Volume V, V-529
            //
            // A0 = pointer to 4-byte DefStartRec (sdSlot:1, sdSResource:1, sdExtDev:1, sdFlags:1)
            // In our emulator, return the dispatcher's in-session
            // startup record (initialized to the zero-filled first-device
            // default).
            //
            // Regression coverage:
            //   src/trap/memory.rs::tests::getdefaultstartup_fills_4_byte_defstartrec_through_a0
            // GetDefaultStartup ($A07D): Returns the current in-session
            // DefStartRec bytes through A0; per IM:V V-529
            (false, 0x7D) => {
                let ptr = cpu.read_reg(Register::A0);
                if ptr != 0 {
                    bus.write_long(ptr, self.default_startup_rec);
                }
                Ok(())
            }

            // SetDefaultStartup ($A07E)
            // Sets the default startup device information.
            // PROCEDURE SetDefaultStartup(paramBlock: DefStartRec);
            // Inside Macintosh Volume V, V-529
            //
            // A0 = pointer to DefStartRec. Capture the caller's
            // 4-byte record for subsequent GetDefaultStartup calls.
            //
            // Regression coverage:
            //   src/trap/memory.rs::tests::setdefaultstartup_updates_getdefaultstartup_and_preserves_stack_pointer
            // SetDefaultStartup ($A07E): Stores the caller-provided
            // DefStartRec for later GetDefaultStartup calls; per IM:V V-529
            (false, 0x7E) => {
                let ptr = cpu.read_reg(Register::A0);
                if ptr != 0 {
                    self.default_startup_rec = bus.read_long(ptr);
                }
                Ok(())
            }

            // GetVideoDefault ($A080)
            // Reads the default video device information.
            // Inside Macintosh Volume V, V-354
            //
            // A0 = pointer to DefVideoRec (sdSlot:1, sdSResource:1)
            // Returns the current Start Manager default video bytes.
            //
            // GetVideoDefault ($A080): Writes DefVideoRec.sdSlot and
            // sdSResource at A0; per IM:V V-354.
            (false, 0x80) => {
                let ptr = cpu.read_reg(Register::A0);
                if ptr != 0 {
                    bus.write_word(ptr, self.default_video_rec);
                }
                Ok(())
            }

            // SetVideoDefault ($A081)
            // Sets the default video device.
            // Inside Macintosh Volume V, V-354 to V-355
            // A0 points to DefVideoRec.
            //
            // SetVideoDefault ($A081): Updates in-session Start Manager
            // defaults from DefVideoRec; per IM:V V-354 to V-355.
            (false, 0x81) => {
                let ptr = cpu.read_reg(Register::A0);
                if ptr != 0 {
                    self.default_video_rec = bus.read_word(ptr);
                }
                Ok(())
            }

            // DTInstall ($A082)
            // Installs a deferred task into the deferred task queue.
            // FUNCTION DTInstall(dtEntryPtr: DeferredTaskPtr): OSErr;
            // Inside Macintosh Volume V (1986), p. V-467; and
            // Inside Macintosh: Processes (1994), pp. 6-12 to 6-13.
            //
            // A0 = pointer to DeferredTask record. Returns noErr for a
            // valid qType and vTypErr when qType != ORD(dtQType) = 7.
            // In our emulator, deferred task queue insertion is not modeled.
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::dtinstall_uses_a0_dttaskptr_register_calling_convention
            //   src/trap/memory.rs::tests::dtinstall_valid_record_returns_noerr_in_d0
            //   src/trap/memory.rs::tests::dtinstall_invalid_qtype_returns_vtyperr_and_preserves_stack_pointer
            (false, 0x82) => {
                let task_ptr = cpu.read_reg(Register::A0);
                let result = if task_ptr == 0 || bus.read_word(task_ptr + 4) != DT_QTYPE {
                    (-2i32) as u32 // vTypErr
                } else {
                    0
                };
                cpu.write_reg(Register::D0, result);
                Ok(())
            }

            // GetOSDefault ($A084)
            // Gets the default OS.
            // Inside Macintosh Volume V, V-355
            //
            // A0 = pointer to DefOSRec (sdReserved:1, sdOSType:1).
            // sdReserved returns 0; sdOSType identifies default OS.
            //
            // GetOSDefault ($A084): Writes DefOSRec.sdReserved and
            // sdOSType at A0; per IM:V V-355.
            (false, 0x84) => {
                let ptr = cpu.read_reg(Register::A0);
                if ptr != 0 {
                    bus.write_word(ptr, self.default_os_rec);
                }
                Ok(())
            }

            // SetOSDefault ($A083)
            // Specifies the default startup operating system.
            // PROCEDURE SetOSDefault(paramBlock: DefOSPtr);
            // Inside Macintosh Volume V (1986), p. V-355.
            //
            // Per IM:V V-355 trap-macro summary the trap is an OS-bit
            // PROCEDURE (bit 11 of the trap word is clear) with a
            // register-only ABI:
            //   Registers on entry:
            //     A0  paramBlock  pointer to DefOSRec (2 bytes)
            //   DefOSRec layout:
            //     +0  sdReserved (Byte)  reserved; should be 0 (`→` input)
            //     +1  sdOSType   (Byte)  startup OS identifier (`→` input)
            //   Registers on exit: (none documented)
            //
            // MPW Universal Headers Start.h declares the trap as:
            //   #pragma parameter SetOSDefault(__A0)
            //   EXTERN_API(void) SetOSDefault(DefOSPtr paramBlock)
            //                                                 ONEWORDINLINE(0xA083);
            // i.e. C-level void return with A0 carrying the DefOSRec pointer.
            // No Pascal stack argument frame is consumed and no result slot
            // is allocated; both inputs travel exclusively through A0.
            //
            // Systemless HLE behavior:
            //   - Reads byte +1 (sdOSType) from the caller-supplied DefOSRec.
            //   - Updates self.default_os_rec so subsequent A084 GetOSDefault
            //     observes the new value (IM:V V-355 Apple-canonical
            //     round-trip semantic).
            //   - Does NOT write back to the caller's buffer at A0; the
            //     DefOSRec is a read-only input per the `→` direction arrows
            //     in IM:V V-355.
            //
            // Apple-vs-BasiliskII divergence:
            //   BasiliskII System 7.5 ROM treats $A083 as a no-op: it
            //   accepts the call but does not propagate sdOSType to the
            //   in-session default record. After Set(sdOSType=2) → Get
            //   returns sdOSType=1 (the BII Mac OS boot default).
            //   This behavior is pinned via the in-Rust test
            //   setosdefault_roundtrips_sdostype_and_
            //   getosdefault_reports_reserved_zero.
            //
            //   Both engines match the documented register-only PROCEDURE
            //   calling convention (A0 input, void return, no stack frame)
            //   and the read-only treatment of the caller's DefOSRec
            //   input buffer.
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::setosdefault_roundtrips_sdostype_and_getosdefault_reports_reserved_zero
            //   src/trap/memory.rs::tests::setosdefault_preserves_caller_defosrec_input_bytes_read_only_a0
            (false, 0x83) => {
                let ptr = cpu.read_reg(Register::A0);
                if ptr != 0 {
                    let os_type = bus.read_byte(ptr + 1);
                    self.default_os_rec = os_type as u16;
                }
                Ok(())
            }

            // PowerOff ($A05B)
            // Turns off power. In our emulator, halt.
            // Inside Macintosh Volume V
            // PowerOff ($A05B): Halts emulation; per IM:V
            (false, 0x5B) => Err(Error::Halted),

            // DeferUserFn ($A08F)
            // Defers a user function call until paging is safe.
            // FUNCTION DeferUserFn (userFunction: ProcPtr; argument: UNIV Ptr): OSErr;
            // Inside Macintosh Volume VI (1991), p. 28-30; and
            // Inside Macintosh: Memory (1992), p. 3-33.
            //
            // OS-bit FUNCTION (bit 11 of the trap word is clear). Per
            // IM:Memory 1992 p. 3-33 register convention:
            //     Registers on entry:
            //         A0  Address of function (userFunction ProcPtr)
            //         D0  Argument (UNIV Ptr) — passed in A0 to the
            //             userFunction if it is called immediately
            //     Registers on exit:
            //         D0  Result code (noErr 0 | cannotDeferErr -625)
            // No Pascal stack argument frame is consumed and no result
            // slot is allocated. The MPW Universal Headers Memory.h
            // declaration is `pascal OSErr DeferUserFn(ProcPtr, void *)`
            // exposed via InterfaceLib only; no ONEWORDINLINE glue is
            // published in the public header, so callers either link
            // through InterfaceLib or use a local
            // `#pragma parameter __D0 fn(__A0, __D0)` thunk emitting
            // the trap word directly.
            //
            // Systemless HLE behavior: if userFunction looks like real
            // 68K code, inject a tiny trampoline that copies the
            // argument into A0, calls the function immediately, and
            // returns noErr in D0 after restoring the scratch
            // registers. Non-callable placeholders fall back to a
            // safe no-op noErr. We do not model the VMM deferred
            // queue or the cannotDeferErr path.
            //
            // Regression coverage:
            //   src/trap/memory.rs::tests::deferuserfn_uses_register_calling_convention_without_stack_arguments
            //   src/trap/memory.rs::tests::deferuserfn_callable_pointer_installs_trampoline_and_returns_noerr
            //   src/trap/memory.rs::tests::deferuserfn_two_call_composition_preserves_stack_across_varying_args
            //   src/trap/memory.rs::tests::deferuserfn_five_call_composition_preserves_stack_across_varying_args
            (false, 0x8F) => {
                let user_function = cpu.read_reg(Register::A0);
                let argument = cpu.read_reg(Register::D0);

                if Self::looks_like_callable_proc(bus, user_function) {
                    let trampoline = self.get_or_create_defer_user_fn_trampoline(bus);
                    if trampoline != 0 {
                        bus.write_long(trampoline + 6, argument);
                        bus.write_long(trampoline + 12, user_function);

                        let current_pc = cpu.read_reg(Register::PC);
                        let sp = cpu.read_reg(Register::A7);
                        let new_sp = sp.wrapping_sub(4);
                        bus.write_long(new_sp, current_pc);
                        cpu.write_reg(Register::A7, new_sp);
                        cpu.write_reg(Register::PC, trampoline);
                    }
                }

                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // InternalWait ($A07F)
            // Start Manager timeout selector trap for GetTimeout/SetTimeout.
            // PROCEDURE GetTimeout (VAR count: Integer);
            // PROCEDURE SetTimeout (count: Integer);
            // Inside Macintosh: Operating System Utilities (1994), pp. 9-27 to 9-28;
            // Inside Macintosh Volume V (1986), p. V-356.
            //
            // OS-bit trap (bit 11 of the trap word is clear). Per
            // IM:Operating_System_Utils 1994 p. 9-27 "Trap Macros Requiring
            // Routine Selectors" table:
            //     | Selector | Routine     |
            //     | $0000    | GetTimeout  |
            //     | $0001    | SetTimeout  |
            //
            // Register convention (per IM:OS_Utils 1994 p. 9-27..9-28):
            //     A0 entry  selector ($0000 or $0001)
            //     D0 entry  count word (read for SetTimeout)
            //     D0 exit   count word (written for GetTimeout)
            // No Pascal stack frame is consumed; no FUNCTION result slot is
            // allocated. Startup-drive spin-up timing is not modeled in
            // Systemless — both selectors are accepted as a no-op stub, and
            // the absolute D0 timeout value diverges between BasiliskII
            // (which surfaces a real ROM timeout register) and Systemless
            // (which leaves D0 unchanged). Both share the register-only
            // OS-bit-trap calling convention.
            //
            // MPW Universal Headers expose this trap via OSUtils.h as the
            // `GetTimeout`/`SetTimeout` glue — both compile to inline
            // thunks that load A0 with the selector before invoking $A07F.
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::internalwait_routes_gettimeout_and_settimeout_selector_paths
            //   src/trap/memory.rs::tests::internalwait_stub_preserves_stack_pointer_in_noop_path
            //   src/trap/memory.rs::tests::internalwait_five_call_composition_preserves_stack_across_alternating_selectors
            (false, 0x7F) => Ok(()),

            // PMgrOp ($A085)
            // Power Manager operations.
            // Inside Macintosh Volume V
            //
            // D0 = selector. No-op — we don't have hardware power management.
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::pmgrop_returns_noerr_and_preserves_stack_pointer_in_noop_path
            // PMgrOp ($A085): Returns noErr; no hardware power management; per IM:V
            (false, 0x85) => return_noerr(cpu),

            // IOPInfoAccess ($A086)
            // Access IOP information.
            // Inside Macintosh Volume VI
            // No-op — no IOP hardware.
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::iopinfoaccess_returns_noerr_and_preserves_stack_pointer
            // IOPInfoAccess ($A086): Returns noErr; no IOP hardware; per IM:VI
            (false, 0x86) => return_noerr(cpu),

            // IOPMsgRequest ($A087)
            // Send message to IOP.
            // Inside Macintosh Volume VI
            // No-op.
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::iopmsgrequest_returns_noerr_and_preserves_stack_pointer
            // IOPMsgRequest ($A087): Returns noErr; per IM:VI
            (false, 0x87) => return_noerr(cpu),

            // IOPMoveData ($A088)
            // Move data to/from IOP.
            // Inside Macintosh Volume VI
            // No-op; mirror noErr through the dispatcher CCR path
            // so 68K callers see a clean OSErr return.
            //
            // IOPMoveData ($A088): Returns noErr; per IM:VI
            (false, 0x88) => return_noerr(cpu),

            // EgretDispatch ($A092)
            // Egret processor dispatch.
            // Inside Macintosh Volume VI
            // No-op.
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::egretdispatch_returns_noerr_and_preserves_stack_pointer
            // EgretDispatch ($A092): Returns noErr; no Egret processor; per IM:VI
            (false, 0x92) => return_noerr(cpu),

            // SleepQInstall ($A28A) / SleepQRemove ($A48A)
            // Sleep queue management.
            // Inside Macintosh: Devices (1994), p. 6-33.
            // No-op behavior in HLE — we don't have sleep/wake hardware.
            // Both variants take one SleepQRecPtr Pascal argument on stack.
            //
            // SleepQInstall/SleepQRemove ($A08A): Returns noErr; no sleep/wake hardware; per IM:VI
            (false, 0x8A) => {
                // Variant trap words: $A28A (install), $A48A (remove).
                // Appendix C table C-2 (IM:VI 1991, p. C-3).
                match self.current_trap_word {
                    0xA28A | 0xA48A => {
                        let sp = cpu.read_reg(Register::A7);
                        let _q_rec_ptr = bus.read_long(sp);
                        cpu.write_reg(Register::A7, sp.wrapping_add(4));
                    }
                    _ => {}
                }
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // SCSIAtomic ($A089)
            // SCSI Manager 4.3 dispatch.
            // Inside Macintosh: Devices (1994), pp. 4-49 to 4-50.
            // Systemless models the SCSIGetVirtualIDInfo branch used by
            // SCSIAtomic: the request header must be well formed, the
            // nominal missing-ID lookup returns noErr / scsiExists =
            // false, and malformed headers are rejected before the
            // lookup path with scsiQLinkInvalid / scsiPBLengthError /
            // scsiRequestInvalid.
            //
            // Regression coverage:
            //   src/trap/memory.rs::tests::scsiaction_scsigetvirtualidinfo_missing_virtual_id_clears_exists_and_preserves_stack
            // SCSIAtomic ($A089): selector $0001 (SCSIAction) / SCSIGetVirtualIDInfo virtual-ID lookup, no virtual bus -> noErr + scsiExists false
            (false, 0x89) => {
                const SCSI_PB_Q_LINK_OFFSET: u32 = 0;
                const SCSI_PB_LENGTH_OFFSET: u32 = 6;
                const SCSI_PB_FUNCTION_CODE_OFFSET: u32 = 8;
                const SCSI_PB_RESULT_OFFSET: u32 = 10;
                const SCSI_PB_COMPLETION_OFFSET: u32 = 16;
                const SCSI_PB_LENGTH_MIN: u16 = 40;
                const SCSI_PB_EXISTS_OFFSET: u32 = 38;
                const SCSI_REQUEST_INVALID: i16 = -7870;
                const SCSI_PB_LENGTH_ERROR: i16 = -7872;
                const SCSI_Q_LINK_INVALID: i16 = -7881;
                const SCSI_GET_VIRTUAL_ID_INFO_FUNCTION_CODE: u8 = 0x80;

                let scsi_pb = cpu.read_reg(Register::A0);
                if scsi_pb == 0 {
                    cpu.write_reg(Register::D0, SCSI_REQUEST_INVALID as u32);
                    return Some(Ok(()));
                }
                let q_link = bus.read_long(scsi_pb + SCSI_PB_Q_LINK_OFFSET);
                if q_link != 0 {
                    bus.write_word(scsi_pb + SCSI_PB_RESULT_OFFSET, SCSI_Q_LINK_INVALID as u16);
                    cpu.write_reg(Register::D0, SCSI_Q_LINK_INVALID as u32);
                    return Some(Ok(()));
                }

                let pb_length = bus.read_word(scsi_pb + SCSI_PB_LENGTH_OFFSET);
                if pb_length < SCSI_PB_LENGTH_MIN {
                    bus.write_word(scsi_pb + SCSI_PB_RESULT_OFFSET, SCSI_PB_LENGTH_ERROR as u16);
                    cpu.write_reg(Register::D0, SCSI_PB_LENGTH_ERROR as u32);
                    return Some(Ok(()));
                }

                let completion = bus.read_long(scsi_pb + SCSI_PB_COMPLETION_OFFSET);
                if completion != 0 {
                    bus.write_word(scsi_pb + SCSI_PB_RESULT_OFFSET, SCSI_REQUEST_INVALID as u16);
                    cpu.write_reg(Register::D0, SCSI_REQUEST_INVALID as u32);
                    return Some(Ok(()));
                }

                let function_code = bus.read_byte(scsi_pb + SCSI_PB_FUNCTION_CODE_OFFSET);
                bus.write_word(scsi_pb + SCSI_PB_RESULT_OFFSET, 0);
                if function_code == SCSI_GET_VIRTUAL_ID_INFO_FUNCTION_CODE {
                    bus.write_byte(scsi_pb + SCSI_PB_EXISTS_OFFSET, 0);
                } else {
                    bus.write_word(scsi_pb + SCSI_PB_RESULT_OFFSET, SCSI_REQUEST_INVALID as u16);
                    cpu.write_reg(Register::D0, SCSI_REQUEST_INVALID as u32);
                    return Some(Ok(()));
                }
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PowerDispatch ($A09F)
            // Power management dispatch.
            // Inside Macintosh Volume VI
            // No-op.
            //
            // PowerDispatch ($A09F): Returns noErr; per IM:VI
            (false, 0x9F) => {
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // CommToolboxDispatch ($A08B)
            // Communications Toolbox dispatch.
            // Inside Macintosh Volume VI (1991), Appendix C table C-3 (p. C-4).
            // Selector-dispatch trap (e.g. $0402 AppendDITL, $0403 CountDITL,
            // $0404 ShortenDITL).
            //
            // Systemless HLE implements the Dialog Manager selectors that
            // System 7 exposes through this trap. MPW glue observed in the
            // Marathon 2 preferences path places the return address at SP+0,
            // selector word at SP+4, and selector-specific arguments after it
            // (for AppendDITL: method at SP+6, DITL handle at SP+8, DialogPtr
            // at SP+12). The trap itself does not consume the caller frame.
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::commtoolboxdispatch_countditl_returns_dialog_item_count_and_preserves_stack_pointer
            //   src/trap/memory.rs::tests::commtoolboxdispatch_countditl_preserves_non_d0_registers
            //   src/trap/memory.rs::tests::commtoolboxdispatch_appendditl_overlay_appends_items_and_preserves_stack_pointer
            (false, 0x8B) => {
                fn count_dialog_items(bus: &MacMemoryBus, dialog_ptr: u32) -> u16 {
                    if dialog_ptr == 0 {
                        return 0;
                    }

                    let items_handle = bus.read_long(dialog_ptr + 156);
                    if items_handle != 0 {
                        let ditl_ptr = bus.read_long(items_handle);
                        if ditl_ptr != 0 {
                            return bus.read_word(ditl_ptr).wrapping_add(1);
                        }
                    }

                    0
                }

                let sp = cpu.read_reg(Register::A7);
                let selector_stack = bus.read_word(sp + 4);
                let selector_d0 = cpu.read_reg(Register::D0) as u16;

                match (selector_stack, selector_d0) {
                    (0x0402, _) => {
                        let method = bus.read_word(sp + 6) as i16;
                        let ditl_handle = bus.read_long(sp + 8);
                        let dialog_ptr = bus.read_long(sp + 12);
                        let count =
                            self.append_ditl_to_dialog(bus, dialog_ptr, ditl_handle, method);
                        cpu.write_reg(Register::D0, count as u32);
                    }
                    (_, 0x0402) => {
                        let method = bus.read_word(sp) as i16;
                        let ditl_handle = bus.read_long(sp + 2);
                        let dialog_ptr = bus.read_long(sp + 6);
                        let count =
                            self.append_ditl_to_dialog(bus, dialog_ptr, ditl_handle, method);
                        cpu.write_reg(Register::D0, count as u32);
                    }
                    (0x0403, _) | (_, 0x0403) => {
                        let dialog_ptr = bus.read_long(sp + 6);
                        let mut resolved_ptr = dialog_ptr;
                        let mut count = count_dialog_items(bus, resolved_ptr);
                        if count == 0 {
                            count = self
                                .dialog_items
                                .get(&resolved_ptr)
                                .map(|items| items.len() as u16)
                                .unwrap_or(0);
                        }
                        if count == 0 && self.front_window != 0 && self.front_window != resolved_ptr
                        {
                            resolved_ptr = self.front_window;
                            count = count_dialog_items(bus, resolved_ptr);
                            if count == 0 {
                                count = self
                                    .dialog_items
                                    .get(&resolved_ptr)
                                    .map(|items| items.len() as u16)
                                    .unwrap_or(0);
                            }
                        }

                        cpu.write_reg(Register::D0, count as u32);
                    }
                    (0x0404, _) => {
                        let number_items = bus.read_word(sp + 6);
                        let dialog_ptr = bus.read_long(sp + 8);
                        let count = self.shorten_ditl_in_dialog(bus, dialog_ptr, number_items);
                        cpu.write_reg(Register::D0, count as u32);
                    }
                    (_, 0x0404) => {
                        let number_items = bus.read_word(sp);
                        let dialog_ptr = bus.read_long(sp + 2);
                        let count = self.shorten_ditl_in_dialog(bus, dialog_ptr, number_items);
                        cpu.write_reg(Register::D0, count as u32);
                    }
                    _ => {
                        cpu.write_reg(Register::D0, 0);
                    }
                }

                Ok(())
            }

            // DebugUtil ($A08D)
            // Debug utility selector-dispatch trap.
            // Inside Macintosh Volume VI (1991), pp. 28-30..28-31; and
            // Appendix C table C-3 (p. C-4) selector map:
            //     | Selector | Routine             |
            //     | $0000    | DebuggerGetMax      |
            //     | $0001    | DebuggerEnter       |
            //     | $0002    | DebuggerExit        |
            //     | $0003    | DebuggerPoll        |
            //     | $0004    | GetPageState        |
            //     | $0005    | PageFaultFatal      |
            //     | $0006    | DebuggerLockMemory  |
            //     | $0007    | DebuggerUnlockMemory|
            //     | $0008    | EnterSupervisorMode |
            //
            // Register convention:
            //     D0 entry: selector (and selector-specific arg for some)
            //     D0 exit:  result code or selector-specific return value
            //     A0/A1 entry: selector-specific args (e.g. address)
            // No Pascal stack frame is consumed and no result slot is
            // allocated.
            //
            // MPW Universal Headers (MacMemory.h) expose each selector as
            // a separate TWOWORDINLINE thunk, e.g.
            //   EXTERN_API( long )
            //   DebuggerGetMax(void) TWOWORDINLINE(0x7000, 0xA08D);
            // which inlines "MOVEQ #0,D0" followed by the trap word.
            //
            // Systemless HLE models the safe selector-query path for
            // DebuggerGetMax ($0000): it returns the highest documented
            // selector number without entering the debugger, mutating
            // page state, or changing supervisor mode. The other
            // documented helper selectors are modelled as safe no-ops
            // that still preserve the register-only OS-bit-trap calling
            // convention (stack discipline).
            //
            // Contract coverage:
            //   src/trap/memory.rs::tests::debugutil_debuggergetmax_returns_max_selector_and_preserves_stack_pointer
            //   src/trap/memory.rs::tests::debugutil_other_selectors_preserve_non_d0_registers
            //   src/trap/memory.rs::tests::debugutil_five_call_composition_preserves_stack_across_documented_selectors
            //   src/trap/memory.rs::tests::debugutil_debuggerpoll_five_call_composition_preserves_stack_across_repeated_selector_three_calls
            (false, 0x8D) => match cpu.read_reg(Register::D0) {
                0 => {
                    cpu.write_reg(Register::D0, 8);
                    Ok(())
                }
                1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 => {
                    cpu.write_reg(Register::D0, 0);
                    Ok(())
                }
                _ => {
                    cpu.write_reg(Register::D0, 0);
                    Ok(())
                }
            },

            // NMInstall ($A05E)
            // Installs a notification request.
            // FUNCTION NMInstall(nmReqPtr: NMRecPtr): OSErr;
            // Inside Macintosh Volume VI (1991), p. 24-10
            //
            // A0 = NMRecPtr. Returns noErr — notifications are not displayed
            // in the emulator, but we accept the request silently.
            //
            // Regression coverage:
            //   src/trap/memory.rs::tests::nminstall_returns_noerr_for_nominal_notification_request
            //   src/trap/memory.rs::tests::nminstall_uses_a0_nmrecptr_register_calling_convention
            // NMInstall ($A05E): Returns noErr on nominal requests; per IM:VI 24-10
            (false, 0x5E) => {
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // NMRemove ($A05F)
            // Removes a notification request.
            // FUNCTION NMRemove(nmReqPtr: NMRecPtr): OSErr;
            // Inside Macintosh Volume VI (1991), p. 24-11
            //
            // Regression coverage:
            //   src/trap/memory.rs::tests::nmremove_returns_noerr_for_nominal_notification_request
            //   src/trap/memory.rs::tests::nmremove_uses_a0_nmrecptr_register_calling_convention
            // NMRemove ($A05F): Returns noErr on nominal requests; per IM:VI 24-11
            (false, 0x5F) => {
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // ========== ADB Manager ==========

            // CountADBs ($A077)
            // Returns the number of ADB devices.
            // FUNCTION CountADBs: INTEGER;
            // Inside Macintosh Volume V, V-372
            (false, 0x77) => {
                cpu.write_reg(Register::D0, u32::from(self.adb.device_count()));
                Ok(())
            }

            // GetIndADB ($A078)
            // Returns a device-table entry selected by its one-based index.
            // FUNCTION GetIndADB(VAR info: ADBDataBlock; devTableIndex: INTEGER): ADBAddress;
            // Inside Macintosh Volume V, V-373
            (false, 0x78) => {
                let index = cpu.read_reg(Register::D0) & 0xFF;
                let info_ptr = cpu.read_reg(Register::A0);
                if let Some(entry) = self.adb.device_by_index(index as u8) {
                    if info_ptr != 0 {
                        bus.write_byte(info_ptr, entry.handler_id);
                        bus.write_byte(info_ptr + 1, entry.original_address);
                        bus.write_long(info_ptr + 2, entry.service_routine);
                        bus.write_long(info_ptr + 6, entry.data_area);
                    }
                    cpu.write_reg(Register::D0, u32::from(entry.current_address));
                } else {
                    // IM:Devices 5-43 / IM:V V-373: unknown entry -> negative result.
                    cpu.write_reg(Register::D0, 0xFFFF_FFFF);
                }
                Ok(())
            }

            // GetADBInfo ($A079)
            // Returns ADB device info by address.
            // FUNCTION GetADBInfo(VAR info: ADBDataBlock; adbAddr: ADBAddress): OSErr;
            // Inside Macintosh Volume V, V-373
            (false, 0x79) => {
                let address = (cpu.read_reg(Register::D0) & 0xFF) as u8;
                let info_ptr = cpu.read_reg(Register::A0);
                if let Some(entry) = self.adb.device_by_address(address) {
                    if info_ptr != 0 {
                        bus.write_byte(info_ptr, entry.handler_id);
                        bus.write_byte(info_ptr + 1, entry.original_address);
                        bus.write_long(info_ptr + 2, entry.service_routine);
                        bus.write_long(info_ptr + 6, entry.data_area);
                    }
                    cpu.write_reg(Register::D0, 0); // noErr
                } else {
                    cpu.write_reg(Register::D0, 0xFFFF_FFFF);
                }
                Ok(())
            }

            // SetADBInfo ($A07A)
            // Sets ADB device info.
            // FUNCTION SetADBInfo(VAR info: ADBSetInfoBlock; adbAddr: ADBAddress): OSErr;
            // Inside Macintosh Volume V, V-374
            (false, 0x7A) => {
                let address = (cpu.read_reg(Register::D0) & 0xFF) as u8;
                let info_ptr = cpu.read_reg(Register::A0);
                if info_ptr != 0 {
                    self.adb.set_device_handler(
                        address,
                        bus.read_long(info_ptr),
                        bus.read_long(info_ptr + 4),
                        self.mouse_button,
                    );
                }
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // ADBReInit ($A07B)
            // Reinitializes ADB.
            // PROCEDURE ADBReInit;
            // Inside Macintosh Volume V, V-371
            // No-op.
            //
            // ADBReInit ($A07B): Per IM:V V-371
            (false, 0x7B) => Ok(()),

            // ADBOp ($A07C)
            // Sends a command to an ADB device; Flush clears queued HLE packets.
            // FUNCTION ADBOp(data: Ptr; compRout: ProcPtr; buffer: Ptr; commandNum: INTEGER): OSErr;
            // Inside Macintosh Volume V, V-367 to V-368
            (false, 0x7C) => {
                let command = (cpu.read_reg(Register::D0) & 0xFF) as u8;
                if command & 0x0F == 1 {
                    self.adb.flush(command >> 4);
                }
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // VADBProc ($A0AE)
            // Virtual ADB dispatch.
            // Inside Macintosh: Devices (1994), pp. 5-39 to 5-40
            // (ADBReInit/JADBProc path).
            // No-op — preserves D0/A7; ADB not modeled.
            //
            // Regression coverage:
            //   src/trap/memory.rs::tests::vadbproc_preserves_d0_and_stack_and_updates_ccr
            // VADBProc ($A0AE): Preserves caller D0/A7; per IM:Devices 5-39..5-40
            (false, 0xAE) => Ok(()),

            // ========== Heap Zone Management ==========

            // GetZone ($A11A)
            // Returns the current heap zone.
            // FUNCTION GetZone: THz;
            // Inside Macintosh: Memory 1992, 2-80
            //
            // Returns TheZone low-mem global ($0118) in A0.
            //
            // Regression coverage:
            //   src/trap/memory.rs::getzone_returns_thezone_pointer_and_noerr
            //   src/trap/memory.rs::setzone_writes_thezone_and_getzone_roundtrips
            // GetZone ($A11A): Returns TheZone ($0118) in A0; D0=noErr; per IM:Memory 1992 2-80
            (false, 0x1A) => {
                let the_zone = bus.read_long(0x0118);
                cpu.write_reg(Register::A0, the_zone);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // SetZone ($A01B)
            // Sets the current heap zone.
            // PROCEDURE SetZone(hz: THz);
            // Inside Macintosh: Memory (1992), pp. 2-80 to 2-81
            //
            // Per IM:Memory 1992 p. 2-81 the trap is an OS-bit
            // PROCEDURE (bit 11 of the trap word is clear) with a
            // register-only ABI:
            //
            //   Registers on entry:
            //     A0  hz  pointer to a heap zone (THz)
            //
            //   Registers on exit:
            //     D0  Result code (noErr by Memory Manager dispatcher
            //                      convention)
            //
            // No Pascal stack argument frame is consumed and no result
            // slot is allocated.
            //
            // MPW Universal Headers MacMemory.h declares:
            //   #pragma parameter SetZone(__A0)
            //   EXTERN_API(void) SetZone(THz hz) ONEWORDINLINE(0xA01B);
            //
            // Documented contract per IM:Memory 1992 p. 2-81:
            //   "SetZone makes the zone to which hz points the
            //    current heap zone. ... Assembly-language note:
            //    Assembly-language callers can set TheZone directly."
            //
            // Both engines (BasiliskII System 7.5.3 ROM and Systemless
            // HLE) implement this as a single write of A0 into the
            // low-memory TheZone global at $0118. Subsequent GetZone
            // calls read $0118 and return the value in A0, so the
            // SetZone -> GetZone roundtrip preserves the supplied
            // zone pointer.
            //
            // Observable behavior:
            //   - Low-memory TheZone ($0118) equals the supplied
            //     zone pointer after the call.
            //   - GetZone after SetZone(hz) returns A0=hz.
            //
            // Regression coverage:
            //   src/trap/memory.rs::setzone_writes_thezone_and_getzone_roundtrips
            //   src/trap/memory.rs::setzone_roundtrip_with_saved_original_restores_thezone
            (false, 0x1B) => {
                let zone = cpu.read_reg(Register::A0);
                bus.write_long(0x0118, zone);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // InitApplZone ($A02C)
            // Initializes the application heap zone.
            // PROCEDURE InitApplZone;
            // Inside Macintosh Volume II, II-28
            //
            // In emulation, the heap is managed by the bus allocator.
            // No-op — returns noErr.
            //
            // Regression coverage:
            //   src/trap/memory.rs::initapplzone_returns_noerr_result_code_in_d0
            //   src/trap/memory.rs::initapplzone_reinitializes_application_zone_and_makes_it_current
            //   src/trap/memory.rs::initapplzone_takes_no_arguments_and_preserves_stack_pointer
            // InitApplZone ($A02C): Reinitializes the observable
            // application-zone header, clears the grow-zone pointer,
            // makes ApplZone current, and returns D0=noErr; per IM:II
            // II-28
            (false, 0x2C) => {
                use crate::memory::globals::addr;
                let appl_zone = bus.read_long(addr::APP_L_ZONE);
                let appl_limit = bus.read_long(addr::APPL_LIMIT);
                init_zone_header(bus, appl_zone, appl_limit, 64, 0);
                bus.write_long(addr::THE_ZONE, appl_zone);
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // ========== Memory Manager Utility Traps ==========

            // EmptyHandle ($A02B)
            // Purges the relocatable block and sets master pointer to NIL.
            // PROCEDURE EmptyHandle(h: Handle);
            // Inside Macintosh: Memory, 2-42
            //
            // EmptyHandle ($A02B): Frees data block and sets master pointer to NIL; returns nilHandleErr for NIL handle; per IM:Memory 2-42. Resource-backed handles preserve Resource Manager metadata so LoadResource can revalidate them after purge (MMTB 1993 1-80; Memory 1992 2-52).
            (false, 0x2B) => {
                let handle = cpu.read_reg(Register::A0);
                if handle == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                } else {
                    let master_ptr = bus.read_long(handle);
                    if master_ptr != 0 {
                        self.ptr_to_handle.remove(&master_ptr);
                        if !self.loaded_handles.contains_key(&handle) {
                            bus.free(master_ptr);
                        }
                    }
                    bus.write_long(handle, 0); // set master pointer to NIL
                    cpu.write_reg(Register::D0, 0); // noErr
                }
                Ok(())
            }

            // BlockMoveData ($A22E) — handled by the existing BlockMove ($A02E) arm
            // above. Both map to trap_num=0x2E; in emulation there is no cache
            // flush to skip, so behavior is identical. IM:Memory 2-76.

            // MaxBlock ($A061)
            // Returns the maximum contiguous free space available
            // in the current heap (after a hypothetical compaction
            // and purge of all purgeable blocks).
            // FUNCTION MaxBlock: LongInt;
            // Inside Macintosh: Memory 1992, 2-47
            //
            // Per IM:Memory 1992 2-47: "MaxBlock returns the
            // maximum contiguous space, in bytes, that you could
            // obtain after compacting the current heap. MaxBlock
            // does not actually do the compaction." Real Mac
            // computes this by walking the free-block list and
            // counting the largest contiguous span (smaller than
            // FreeMem when the heap is fragmented).
            //
            // HLE compromise: Systemless doesn't model fragmentation
            // — every allocation is permanent in the bus's flat
            // address space, so "max contiguous" == "total free"
            // == free_heap_estimate value. Routes through the
            // same helper used by FreeMem / MaxMem / CompactMem /
            // PurgeSpace (reads ApplLimit - HeapEnd with the shared
            // compatibility-floor/partition rule). Replaces a prior hardcoded 2MB
            // constant which was BELOW modern minimum-block-size
            // gates — apps that probe MaxBlock to verify "do I
            // have a 4MB+ contiguous block for a sound buffer or
            // GWorld pixmap?" would have failed at the 2MB
            // constant; the 24MB floor passes those gates.
            // MaxBlock ($A061): Per IM:Memory 1992 2-47 returns max contiguous free space (after hypothetical compaction). Systemless doesn't model fragmentation so MaxBlock == FreeMem == MaxMem == CompactMem == PurgeSpace via the shared free_heap_estimate helper. Replaces a prior hardcoded 2MB constant that was below modern 4MB+ contiguous-block gates (sound buffer, GWorld pixmap allocations).
            (false, 0x61) => {
                cpu.write_reg(Register::D0, free_heap_estimate(bus));
                Ok(())
            }

            // StackSpace ($A065)
            // Returns the amount of stack space available.
            // FUNCTION StackSpace: LongInt;
            // Inside Macintosh: Memory, 2-48
            //
            // Returns SP minus ApplLimit (heap top). In emulation, we estimate
            // generously since we don't track the real heap boundary.
            //
            // StackSpace ($A065): Returns SP minus ApplLimit ($0130); per IM:Memory 2-48
            (false, 0x65) => {
                let sp = cpu.read_reg(Register::A7);
                // ApplLimit ($0130) — top of application heap zone
                let appl_limit = bus.read_long(0x0130);
                let space = if appl_limit != 0 && sp > appl_limit {
                    sp - appl_limit
                } else {
                    // If ApplLimit not set, return a reasonable default
                    sp.min(256 * 1024)
                };
                cpu.write_reg(Register::D0, space);
                Ok(())
            }

            // ResrvMem ($A040)
            // Reserves space at the bottom of the heap for a block.
            // PROCEDURE ReserveMem(cbNeeded: Size);
            // Inside Macintosh: Memory, 2-52
            //
            // In emulation, heap compaction isn't needed. This is a no-op that
            // always succeeds — the next NewHandle will allocate wherever it can.
            //
            // ResrvMem ($A040): No-op in emulation; per IM:Memory 2-52
            (false, 0x40) => {
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // PurgeMem ($A04D)
            // Inside Macintosh: Memory (1992), pp. 2-73 to 2-74,
            // p. 2-87 (summary), p. 2-94 (assembly-language summary).
            //
            // PROCEDURE PurgeMem (cbNeeded: Size);
            //   Registers on entry: D0 = Size of block to make room for
            //   Registers on exit:  D0 = Result code (noErr / memFullErr)
            //
            // OS-bit PROCEDURE (trap-word bit 11 clear) with a register-
            // only ABI: no Pascal stack argument frame and no FUNCTION
            // result slot. The MPW Universal Headers MacMemory.h
            // exposes the C-level `PurgeMem(Size cbNeeded)` with
            //   #pragma parameter PurgeMem(__D0)
            //   EXTERN_API(void) PurgeMem(Size cbNeeded) ONEWORDINLINE(0xA04D);
            //
            // Apple-canonical behavior: sequentially purge blocks from
            // the current heap zone until either a contiguous block of
            // at least cbNeeded free bytes exists, or the entire zone
            // has been purged (per IM:Memory 1992 p. 2-73). Only
            // relocatable, unlocked, purgeable blocks are purged. If
            // the zone is exhausted without yielding cbNeeded bytes,
            // memFullErr (-108) is returned in D0. Per IM:Memory 1992
            // p. 2-73 "PurgeMem does not actually attempt to allocate
            // a block of cbNeeded bytes."
            //
            // Why the absolute D0 result differs between BasiliskII and Systemless:
            //   BasiliskII System 7.5.3 ROM manages a real Mac heap
            //   with arbitrary purgeable blocks installed by the boot
            //   process; the absolute D0 result depends on the
            //   boot-time layout. Systemless HLE's host-backed allocator
            //   has no purgeable blocks at all, so PurgeMem is
            //   structurally a no-op that always returns noErr. The
            //   only documented shared post-condition is the
            //   register-only calling convention itself.
            //
            // Systemless HLE compromise: D0 (cbNeeded) is read and
            // discarded; D0 is set to noErr. This matches BasiliskII
            // System 7.5.3 ROM behavior on the calling-convention
            // contract — both engines preserve A7 and consume no
            // Pascal stack frame.
            //
            // Observable behavior:
            //   (1) register-only ABI: A7 preserved across a single
            //       PurgeMem(0) call.
            //   (2) cumulative pop discipline: a 5-call composition
            //       cycling cbNeeded values 0 -> 256 -> 0 -> 1024 -> 0
            //       preserves A7 in aggregate (defeats stubs that
            //       consume a Pascal arg frame on the cbNeeded>0 path
            //       or pop a 4-byte cbNeeded value via Pascal calling
            //       convention).
            //
            // Regression coverage:
            //   src/trap/memory.rs::purgemem_register_only_calling_convention_preserves_stack
            (false, 0x4D) => {
                // D0 = cbNeeded on entry (read and discarded; no
                // purgeable blocks exist in the host-backed allocator).
                // Per IM:Memory 1992 p. 2-73 D0 carries the result code
                // on exit. Return noErr.
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // SetGrowZone ($A04B)
            // Inside Macintosh: Memory (1992), pp. 2-55 to 2-56 and p. 2-93.
            //
            // PROCEDURE SetGrowZone (growZone: ProcPtr);
            //   Registers on entry: A0 = pointer to new grow-zone function
            //                       (NIL removes any existing grow-zone
            //                       function per IM:Memory 1992 p. 2-56)
            //   Registers on exit:  D0 = result code (noErr)
            //
            // OS-bit PROCEDURE (trap-word bit 11 clear) with a register-only
            // ABI: no Pascal stack argument frame and no FUNCTION result
            // slot. The MPW Universal Headers MacMemory.h exposes the
            // C-level `SetGrowZone(GrowZoneUPP growZone)` with
            //   #pragma parameter SetGrowZone(__A0)
            //   EXTERN_API(void) SetGrowZone(GrowZoneUPP growZone) ONEWORDINLINE(0xA04B);
            //
            // Apple-canonical behavior: install the supplied function
            // pointer as the current heap zone's grow-zone function. The
            // Memory Manager calls the grow-zone function only after
            // exhausting all other avenues of satisfying a memory request
            // (compaction, zone growth, purging). A NIL parameter removes
            // any previously installed grow-zone function.
            //
            // Why the visible Zone field is NOT a reliable witness:
            //   Per IM:Memory 1992 p. 2-20 the Zone record's `gzProc` field
            //   description explicitly warns: "Note that in current versions
            //   of system software, this field does not contain a pointer
            //   to the grow-zone function that your application defines."
            //   The system installs an opaque trampoline in `zone.gzProc`
            //   and stashes the caller-supplied pointer in a private slot,
            //   so the directly observable field is system-trampoline-
            //   opaque and differs between BasiliskII and Systemless. The
            //   only documented shared post-condition is the register-only
            //   calling convention itself.
            //
            // Systemless HLE compromise: no real heap-exhaustion path exists
            // (the host-backed allocator never triggers compaction, growth
            // pressure, or purging), so the registered grow-zone function
            // would never be invoked even if it were stored. A0 is read
            // and discarded; D0 is set to noErr. This matches BasiliskII
            // System 7.5.3 ROM behavior on the calling-convention contract
            // — both engines preserve A7, accept any A0 (NIL or non-NIL),
            // and return noErr.
            //
            // Observable behavior:
            //   (1) register-only ABI: A7 preserved across a single
            //       SetGrowZone(NIL) call.
            //   (2) cumulative pop discipline: a 5-call composition
            //       cycling NIL → synthetic ProcPtr → NIL → synthetic
            //       ProcPtr → NIL preserves A7 in aggregate (defeats
            //       stubs that consume a Pascal arg frame on the non-NIL
            //       path).
            //
            // Regression coverage:
            //   src/trap/memory.rs::setgrowzone_register_only_calling_convention_preserves_stack
            (false, 0x4B) => {
                // A0 = pointer to grow-zone function (NIL or non-NIL).
                // Per IM:Memory 1992 p. 2-20 the user-supplied pointer is
                // stashed by the system in a private slot (the visible
                // zone.gzProc field holds an opaque trampoline), so the
                // HLE has nothing observable to write. Return noErr.
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // HSetRBit ($A067)
            //
            // PROCEDURE HSetRBit(h: Handle);
            // Inside Macintosh: Memory (1992), pp. 2-49 to 2-50.
            //
            // IM:Memory 1992 p. 2-49 verbatim: "You can use the HSetRBit
            // procedure to set the resource flag of a relocatable block.
            // The Resource Manager uses this routine extensively, but you
            // should never need to use it. ... The HSetRBit procedure sets
            // the resource flag of the relocatable block to which h is a
            // handle. It does nothing if the flag is already set."
            //
            // Per IM:Memory 1992 p. 2-49 the resource flag is bit 5 of the
            // master pointer's flag byte (also exposed as
            // `kHandleIsResourceMask = 0x20` in MPW MacMemory.h). HGetState
            // ($A069) returns this byte so callers observe the bit
            // transition; the IM warning on p. 2-43 ("do not rely on the
            // structure of master pointers") makes HGetState the canonical
            // read path because the byte layout differs between 24-bit
            // and 32-bit addressing modes.
            //
            // Assembly calling convention (IM:Memory 1992 p. 2-50):
            //   Registers on entry: A0 = handle
            //   Registers on exit:  D0 = result code (OSErr)
            //     noErr        =    0  success
            //     nilHandleErr = -109  NIL master pointer / handle
            //     memWZErr     = -111  free block
            //
            // MPW Universal Headers (MacMemory.h):
            //   #pragma parameter HSetRBit(__A0)
            //   EXTERN_API(void) HSetRBit(Handle h) ONEWORDINLINE(0xA067);
            // The void declaration elides D0 from C callers; a caller
            // that needs to witness D0 (the nilHandleErr return) declares
            // an inline thunk like
            //   #pragma parameter __D0 kx_HSetRBit_D0(__A0)
            //   pascal short kx_HSetRBit_D0(Handle h) = { 0xA067 };
            //
            // Systemless HLE: tracks resource-flag state in the dispatcher's
            // `handle_state_bits` HashMap rather than poking master
            // pointer bytes. This is functionally transparent because
            // applications must use HGetState/HSetState/HSetRBit/HClrRBit
            // for state observation (per the IM warning) — the HashMap
            // is in sync with HSetRBit/HClrRBit writes and HGetState
            // reads from it, so round-trip semantics match BII byte-for-
            // byte on the documented surface.
            //
            // Regression coverage:
            //   src/trap/memory.rs::test_memorydispatch_hsetrbit_sets_resource_flag
            //   src/trap/memory.rs::test_memorydispatch_hsetrbit_nil_handle_returns_error
            //   src/trap/memory.rs::hsetrbit_then_hgetstate_round_trips_resource_bit
            //
            // HSetRBit ($A067): Sets resource flag (0x20) in handle_state_bits; nilHandleErr for NIL; per IM:Memory 1992 2-49..2-50
            (false, 0x67) => {
                let handle = cpu.read_reg(Register::A0);
                if handle == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                } else {
                    let bits = self.handle_state_bits.entry(handle).or_insert(0);
                    *bits |= 0x20; // resource bit
                    cpu.write_reg(Register::D0, 0); // noErr
                }
                Ok(())
            }

            // HClrRBit ($A068)
            //
            // PROCEDURE HClrRBit(h: Handle);
            // Inside Macintosh: Memory (1992), pp. 2-50 to 2-51.
            //
            // IM:Memory 1992 p. 2-50 verbatim: "You can use the HClrRBit
            // procedure to clear the resource flag of a relocatable
            // block. The Resource Manager uses this routine extensively,
            // but you probably won't need to use it. ... The HClrRBit
            // procedure clears the resource flag of a relocatable block.
            // It does nothing if the flag is already cleared."
            //
            // Symmetric with HSetRBit ($A067): clears the same bit-5 of
            // the master pointer's flag byte (0x20 mask). The Resource
            // Manager uses HClrRBit when the resource handle is being
            // detached from its 'rsrc' fork so it no longer needs to be
            // treated as a resource block.
            //
            // Assembly calling convention (IM:Memory 1992 p. 2-51):
            //   Registers on entry: A0 = handle
            //   Registers on exit:  D0 = result code (OSErr)
            //     noErr        =    0  success
            //     nilHandleErr = -109  NIL master pointer / handle
            //     memWZErr     = -111  free block
            //
            // MPW Universal Headers (MacMemory.h):
            //   #pragma parameter HClrRBit(__A0)
            //   EXTERN_API(void) HClrRBit(Handle h) ONEWORDINLINE(0xA068);
            // The void declaration elides D0 from C callers (mirrors
            // HSetRBit); callers use the same inline-thunk pattern with
            //   #pragma parameter __D0 kx_HClrRBit_D0(__A0)
            //   pascal short kx_HClrRBit_D0(Handle h) = { 0xA068 };
            //
            // Systemless HLE: clears 0x20 in `handle_state_bits[handle]` and
            // removes the entry entirely when no flag bits remain. The
            // HashMap mirrors what HGetState ($A069) returns; BasiliskII
            // System 7.5.3 ROM updates the master pointer flag byte
            // directly. Both engines agree on the observable HGetState
            // round-trip behavior — set → 0x20, clear → 0x00.
            //
            // Regression coverage:
            //   src/trap/memory.rs::test_memorydispatch_hclrrbit_clears_resource_flag
            //   src/trap/memory.rs::test_memorydispatch_hclrrbit_nil_handle_returns_error
            //   src/trap/memory.rs::hclrrbit_after_hsetrbit_round_trips_resource_bit_to_zero
            //
            // HClrRBit ($A068): Clears resource flag (0x20) in handle_state_bits; nilHandleErr for NIL; per IM:Memory 1992 2-50..2-51
            (false, 0x68) => {
                let handle = cpu.read_reg(Register::A0);
                if handle == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                } else {
                    if let Some(bits) = self.handle_state_bits.get_mut(&handle) {
                        *bits &= !0x20; // clear resource bit
                        if *bits == 0 {
                            self.handle_state_bits.remove(&handle);
                        }
                    }
                    cpu.write_reg(Register::D0, 0); // noErr
                }
                Ok(())
            }

            // HandleZone ($A126)
            // Returns the heap zone containing a handle's relocatable block.
            // FUNCTION HandleZone(h: Handle): THz;
            // Inside Macintosh: Memory, 2-66
            //
            // In emulation, all allocations live in a single zone.
            // Return TheZone ($0118) for all valid handles.
            // $A126 = OS trap, trap_num = 0x26 (SYS bit ignored).
            //
            // Regression coverage:
            //   src/trap/memory.rs::handlezone_valid_handle_returns_current_zone_pointer
            //   src/trap/memory.rs::handlezone_empty_handle_returns_zone_pointer
            //   src/trap/memory.rs::handlezone_disposed_handle_returns_memwzerr
            // HandleZone ($A126): Returns TheZone ($0118) for live valid/empty handles; noErr for NIL; memWZErr for freed handles; per BasiliskII-observed nil divergence from IM:Memory 1992 2-82..2-83
            (false, 0x26) => {
                let handle = cpu.read_reg(Register::A0);
                if handle == 0 {
                    cpu.write_reg(Register::D0, 0); // noErr
                } else if bus.get_alloc_size(handle) != Some(4) {
                    cpu.write_reg(Register::D0, (-111i32) as u32); // memWZErr
                } else {
                    let the_zone = bus.read_long(0x0118); // TheZone low-mem global
                    cpu.write_reg(Register::A0, the_zone);
                    cpu.write_reg(Register::D0, 0); // noErr
                }
                Ok(())
            }

            // PtrZone ($A148)
            // Returns the heap zone containing a nonrelocatable block.
            // FUNCTION PtrZone(p: Ptr): THz;
            // Inside Macintosh: Memory, 2-65
            //
            // In emulation, all allocations live in a single zone.
            // Return TheZone ($0118) for all valid pointers.
            // $A148 = OS trap, trap_num = 0x48 (SYS bit ignored).
            //
            // Regression coverage:
            //   src/trap/memory.rs::ptrzone_valid_pointer_returns_current_zone_pointer
            //   src/trap/memory.rs::ptrzone_nil_pointer_returns_memwzerr
            // PtrZone ($A148): Returns TheZone ($0118) for valid pointer; memWZErr for NIL; per IM:Memory 1992 2-83
            (false, 0x48) => {
                let ptr = cpu.read_reg(Register::A0);
                if ptr == 0 {
                    cpu.write_reg(Register::D0, (-111i32) as u32); // memWZErr
                } else {
                    let the_zone = bus.read_long(0x0118); // TheZone low-mem global
                    cpu.write_reg(Register::A0, the_zone);
                    cpu.write_reg(Register::D0, 0); // noErr
                }
                Ok(())
            }

            // ========== MemoryDispatch Virtual Memory ==========

            // MemoryDispatch ($A05C)
            // Dispatches virtual memory operations by selector in D0.
            // Inside Macintosh: Memory, 4-6
            //
            // Selectors:
            //   0 = HoldMemory
            //   1 = UnholdMemory
            //   2 = LockMemory
            //   3 = UnlockMemory
            //   4 = LockMemoryContiguous
            //   5 = GetPhysical
            //
            // HLE model:
            // - Track hold/lock by 4 KiB logical pages.
            // - LockMemoryContiguous shares LockMemory semantics (we do not
            //   model physical fragmentation).
            // - UnholdMemory returns notHeldErr for ranges whose pages were
            //   never held; a previously-held range is idempotent after it
            //   has already been released.
            // - BasiliskII returns noErr for HoldMemory/UnholdMemory on
            //   non-logical-RAM ranges; Systemless matches BasiliskII here.
            // - GetPhysical returns identity mappings (logical == physical)
            //   but enforces the documented "range must be locked" contract.
            // - A zero-sized requested_entries query on a non-empty locked
            //   range follows the BasiliskII-observed paramErr/required-count
            //   behavior and leaves the translation table untouched. In
            //   Systemless's flat-RAM model, that required count is one physical
            //   entry. Empty ranges also return paramErr and preserve A0/
            //   table contents.
            // - For selectors 2..5, reject out-of-logical-RAM ranges with
            //   paramErr (-50), matching IM result-code contracts. Hold/
            //   Unhold invalid-range probes follow the BasiliskII noErr
            //   behavior instead.
            //
            // Reference:
            //   Inside Macintosh: Memory (1992), Virtual Memory Manager
            //   Reference, pages 3-25..3-32.
            //
            // Regression coverage:
            //   src/trap/memory.rs::test_memorydispatch_unlockmemory_returns_notlockederr_for_unlocked_range
            //   src/trap/memory.rs::test_memorydispatch_unholdmemory_returns_nothelderr_for_never_held_range
            //   src/trap/memory.rs::test_memorydispatch_holdmemory_round_trip_releases_idempotently
            //   src/trap/memory.rs::test_memorydispatch_holdmemory_invalid_range_returns_noerr_and_preserves_stack
            //   src/trap/memory.rs::test_memorydispatch_unholdmemory_invalid_range_returns_noerr_and_preserves_stack
            //   src/trap/memory.rs::test_memorydispatch_lockmemory_invalid_range_returns_paramerr
            //   src/trap/memory.rs::test_memorydispatch_lockmemorycontiguous_invalid_range_returns_paramerr
            //   src/trap/memory.rs::test_memorydispatch_lockmemorycontiguous_zero_length_invalid_range_returns_paramerr
            //   src/trap/memory.rs::test_memorydispatch_unlockmemory_invalid_range_returns_paramerr
            //   src/trap/memory.rs::test_memorydispatch_unlockmemory_reverses_lockmemorycontiguous_on_page_rounded_range
            //   src/trap/memory.rs::test_memorydispatch_getphysical_requires_locked_range
            //   src/trap/memory.rs::test_memorydispatch_getphysical_invalid_logical_range_returns_paramerr
            //   src/trap/memory.rs::test_memorydispatch_getphysical_null_table_returns_paramerr
            //   src/trap/memory.rs::test_memorydispatch_getphysical_fills_identity_mapping_when_locked
            //   src/trap/memory.rs::test_memorydispatch_getphysical_entrycount_zero_returns_paramerr_and_required_entries
            //   src/trap/memory.rs::test_memorydispatch_getphysical_entrycount_zero_on_empty_range_returns_paramerr_and_preserves_table
            //   src/trap/memory.rs::test_memorydispatch_getphysical_entrycount_zero_on_unlocked_range_returns_notlockederr
            //   src/trap/memory.rs::test_memorydispatch_getphysical_entrycount_zero_on_locked_multpage_range_returns_two_and_preserves_table
            // MemoryDispatch ($A05C): Tracks Hold/Lock page state and implements GetPhysical lock-check + identity mapping; selectors per IM:Memory 1992 pp.3-25..3-32
            (false, 0x5C) => {
                let selector = cpu.read_reg(Register::D0);
                let address = cpu.read_reg(Register::A0);
                let count = cpu.read_reg(Register::A1);
                match selector {
                    0 => {
                        if !vm_range_is_logical_ram(bus, address, count) {
                            cpu.write_reg(Register::D0, NO_ERR);
                            return Some(Ok(()));
                        }
                        // HoldMemory: mark each covered page held.
                        if let Some((page_start, page_end_exclusive)) = vm_page_span(address, count)
                        {
                            vm_increment_pages(
                                &mut self.vm_held_page_counts,
                                page_start,
                                page_end_exclusive,
                            );
                            self.vm_held_page_history
                                .extend(page_start..page_end_exclusive);
                        }
                        cpu.write_reg(Register::D0, NO_ERR);
                    }
                    1 => {
                        if !vm_range_is_logical_ram(bus, address, count) {
                            cpu.write_reg(Register::D0, NO_ERR);
                            return Some(Ok(()));
                        }
                        let result = if let Some((page_start, page_end_exclusive)) =
                            vm_page_span(address, count)
                        {
                            let ever_held = (page_start..page_end_exclusive)
                                .all(|page| self.vm_held_page_history.contains(&page));
                            if ever_held {
                                let _ = vm_try_decrement_pages(
                                    &mut self.vm_held_page_counts,
                                    page_start,
                                    page_end_exclusive,
                                );
                                NO_ERR
                            } else {
                                NOT_HELD_ERR
                            }
                        } else {
                            NO_ERR
                        };
                        cpu.write_reg(Register::D0, result);
                    }
                    2 | 4 => {
                        if !vm_range_is_logical_ram(bus, address, count) {
                            cpu.write_reg(Register::D0, PARAM_ERR);
                            return Some(Ok(()));
                        }
                        // LockMemory / LockMemoryContiguous:
                        // mark each covered page locked.
                        //
                        // We intentionally model selector 4 as selector 2
                        // because Systemless does not model physical-page
                        // relocation/fragmentation; callers still get
                        // lock-state semantics for GetPhysical.
                        if let Some((page_start, page_end_exclusive)) = vm_page_span(address, count)
                        {
                            vm_increment_pages(
                                &mut self.vm_locked_page_counts,
                                page_start,
                                page_end_exclusive,
                            );
                        }
                        cpu.write_reg(Register::D0, NO_ERR);
                    }
                    3 => {
                        if !vm_range_is_logical_ram(bus, address, count) {
                            cpu.write_reg(Register::D0, PARAM_ERR);
                            return Some(Ok(()));
                        }
                        // UnlockMemory: error when any page is not currently locked.
                        let result = if let Some((page_start, page_end_exclusive)) =
                            vm_page_span(address, count)
                        {
                            if vm_try_decrement_pages(
                                &mut self.vm_locked_page_counts,
                                page_start,
                                page_end_exclusive,
                            ) {
                                NO_ERR
                            } else {
                                NOT_LOCKED_ERR
                            }
                        } else {
                            NO_ERR
                        };
                        cpu.write_reg(Register::D0, result);
                    }
                    5 => {
                        // GetPhysical:
                        // A0 = LogicalToPhysicalTable pointer
                        // A1 = physicalEntryCount on entry
                        // A0 = translated entry count on exit
                        let table_ptr = address;
                        let requested_entries = count;
                        if table_ptr == 0 {
                            cpu.write_reg(Register::A0, 0);
                            cpu.write_reg(Register::D0, PARAM_ERR);
                            return Some(Ok(()));
                        }

                        let logical_start = bus.read_long(table_ptr);
                        let logical_count = bus.read_long(table_ptr + 4);
                        if !vm_range_is_logical_ram(bus, logical_start, logical_count) {
                            cpu.write_reg(Register::A0, 0);
                            cpu.write_reg(Register::D0, PARAM_ERR);
                            return Some(Ok(()));
                        }
                        if logical_count == 0 {
                            // BasiliskII leaves A0 pointing at the table on this
                            // empty-range error path.
                            cpu.write_reg(Register::D0, PARAM_ERR);
                            return Some(Ok(()));
                        }

                        let logical_locked = vm_page_span(logical_start, logical_count)
                            .map(|(page_start, page_end_exclusive)| {
                                vm_range_is_fully_tracked(
                                    &self.vm_locked_page_counts,
                                    page_start,
                                    page_end_exclusive,
                                )
                            })
                            .unwrap_or(true);
                        if !logical_locked {
                            cpu.write_reg(Register::A0, 0);
                            cpu.write_reg(Register::D0, NOT_LOCKED_ERR);
                            return Some(Ok(()));
                        }

                        if requested_entries == 0 {
                            // Report the number of physical entries needed to cover the
                            // requested logical span. Systemless's VM model is page-based, so
                            // the count matches the number of tracked pages in the span.
                            let required_entries =
                                vm_required_physical_entries(logical_start, logical_count);
                            cpu.write_reg(Register::A0, required_entries);
                            cpu.write_reg(Register::D0, PARAM_ERR);
                            return Some(Ok(()));
                        }

                        // Identity mapping in HLE: one physical block with the
                        // same start/count as the logical range.
                        // LogicalToPhysicalTable layout:
                        //   +0  logical.address
                        //   +4  logical.count
                        //   +8  physical[0].address
                        //   +12 physical[0].count
                        bus.write_long(table_ptr + 8, logical_start);
                        bus.write_long(table_ptr + 12, logical_count);
                        cpu.write_reg(Register::A0, 1);
                        cpu.write_reg(Register::D0, NO_ERR);
                    }
                    _ => {
                        eprintln!("[TRAP] MemoryDispatch: unknown selector {}", selector);
                        cpu.write_reg(Register::D0, PARAM_ERR);
                    }
                }
                Ok(())
            }

            // HeapDispatch ($A0A4)
            // Heap Manager dispatch for extended heap operations.
            // Inside Macintosh Volume VI, Heap Manager section.
            //
            // No-op in emulation — returns noErr and follows the
            // Memory Manager dispatcher CCR discipline (TST.W D0)
            // so callers that inspect condition codes see Z set for
            // the noErr path.
            //
            (false, 0xA4) => {
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // NewPtrSys ($A51E)
            // Allocates nonrelocatable block in system heap.
            // In emulation, same as NewPtr (single heap).
            // Inside Macintosh: Memory, 2-30
            //
            // Already handled by (false, 0x1E) since SYS bit is ignored.
            // This arm catches the explicit $A51E trap word.

            // SetApplBase ($A057)
            // Sets the application heap's base address to startPtr; per
            // IM:II II-28 + IM:Memory 1992 p. 2-88 the trap is an OS-bit
            // routine (bit 11 clear) with register-only ABI:
            //
            //   Registers on entry: A0 = startPtr (pointer to new heap base)
            //   Registers on exit:  D0 = OSErr (Memory Manager dispatcher
            //                                   convention; 0 = noErr)
            //
            // Pascal signature: PROCEDURE SetApplBase(startPtr: Ptr);
            // MPW Universal Headers MacMemory.h declaration:
            //   #pragma parameter SetApplBase(__A0)
            //   EXTERN_API(void) SetApplBase(void *startPtr)
            //                                ONEWORDINLINE(0xA057);
            //
            // Although the Pascal signature is PROCEDURE, the Memory
            // Manager dispatcher conventionally writes 0 in D0 on
            // success (this is the Memory Manager OS-bit dispatcher
            // convention even for traps declared as PROCEDURE in the
            // public Pascal/C wrapper). Both BII's System 7.5.3 ROM and
            // Systemless HLE return D0 = noErr on the nominal call path.
            //
            // Per IM:II II-28 "The procedure SetApplBase sets the
            // application heap to start at the address startPtr. ...
            // If startPtr is NIL, the procedure sets it to its preset
            // default."
            //
            // Systemless HLE compromise: the bus allocator manages the
            // heap centrally, so SetApplBase is a no-op that writes
            // D0 = 0 (noErr) and returns. The startPtr argument is
            // ignored because Systemless does not model a relocatable
            // application zone.
            //
            // Apple-vs-Systemless divergence on stack discipline:
            //   Per IM:II II-29 the trap has a register-only ABI and
            //   should preserve A7. Systemless HLE preserves A7 byte-for-
            //   byte (only D0 is written). BII's System 7.5.3 ROM does
            //   real heap-validation work during the trap, so a stack-
            //   pointer probe reports sp_pre != sp_post even though the
            //   documented register table guarantees no Pascal stack
            //   frame is consumed. This stack-discipline divergence is
            //   pinned by the in-Rust test setapplbase_uses_register_
            //   calling_convention_without_stack_arguments below.
            //
            // Regression coverage:
            //   src/trap/memory.rs::setapplbase_uses_a0_startptr_and_returns_noerr_in_d0
            //   src/trap/memory.rs::setapplbase_uses_register_calling_convention_without_stack_arguments
            //   src/trap/memory.rs::setapplbase_returns_noerr_regardless_of_startptr_value
            (false, 0x57) => {
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            _ => return None,
        };

        if result.is_ok() && memory_manager_trap_updates_dispatcher_ccr(is_tool, trap_num) {
            apply_memory_manager_dispatcher_ccr(cpu);
        }

        Some(result)
    }
}

/// Convert a Mac Roman character to uppercase per Inside Macintosh IV, IV-235.
/// If `strip_marks` is true, diacritical marks are also stripped.
///
/// Mac Roman uppercase conversion table (from IM:IV case conversion):
///   à(0x88)→À(0xCB), ã(0x8B)→Ã(0xCC), ä(0x8A)→Ä(0x80), å(0x8C)→Å(0x81),
///   æ(0xBE)→Æ(0xAE), ç(0x8D)→Ç(0x82), é(0x8E)→É(0x83), ñ(0x96)→Ñ(0x84),
///   ö(0x9A)→Ö(0x85), õ(0x9B)→Õ(0xCD), ø(0xBF)→Ø(0xAF), œ(0xCF)→Œ(0xCE),
///   ü(0x9F)→Ü(0x86)
fn mac_roman_to_upper(ch: u8, strip_marks: bool) -> u8 {
    if strip_marks {
        // Strip diacriticals first, then uppercase the base letter
        let stripped = mac_roman_strip_diacriticals(ch);
        if stripped.is_ascii_lowercase() {
            return stripped - 32;
        }
        return stripped;
    }
    // Standard ASCII lowercase → uppercase
    if ch.is_ascii_lowercase() {
        return ch - 32;
    }
    // Mac Roman accented lowercase → accented uppercase
    // Only the pairs that exist in Mac Roman per IM:IV IV-235
    match ch {
        0x88 => 0xCB, // à → À
        0x8A => 0x80, // ä → Ä
        0x8B => 0xCC, // ã → Ã
        0x8C => 0x81, // å → Å
        0x8D => 0x82, // ç → Ç
        0x8E => 0x83, // é → É
        0x96 => 0x84, // ñ → Ñ
        0x9A => 0x85, // ö → Ö
        0x9B => 0xCD, // õ → Õ
        0x9F => 0x86, // ü → Ü
        0xBE => 0xAE, // æ → Æ
        0xBF => 0xAF, // ø → Ø
        0xCF => 0xCE, // œ → Œ
        _ => ch,
    }
}

/// Convert a Mac Roman character to lowercase.
fn mac_roman_to_lower(ch: u8) -> u8 {
    if ch.is_ascii_uppercase() {
        return ch + 32;
    }
    // Mac Roman uppercase accented → lowercase accented (reverse of to_upper)
    match ch {
        0x80 => 0x8A, // Ä → ä
        0x81 => 0x8C, // Å → å
        0x82 => 0x8D, // Ç → ç
        0x83 => 0x8E, // É → é
        0x84 => 0x96, // Ñ → ñ
        0x85 => 0x9A, // Ö → ö
        0x86 => 0x9F, // Ü → ü
        0xAE => 0xBE, // Æ → æ
        0xAF => 0xBF, // Ø → ø
        0xCB => 0x88, // À → à
        0xCC => 0x8B, // Ã → ã
        0xCD => 0x9B, // Õ → õ
        0xCE => 0xCF, // Œ → œ
        _ => ch,
    }
}

/// Strip diacritical marks from a Mac Roman character without case conversion.
/// Per IM:IV IV-235 stripping table.
fn mac_roman_strip_diacriticals(ch: u8) -> u8 {
    match ch {
        // Uppercase accented → uppercase base
        0x80 => b'A', // Ä → A
        0x81 => b'A', // Å → A
        0x82 => b'C', // Ç → C
        0x83 => b'E', // É → E
        0x84 => b'N', // Ñ → N
        0x85 => b'O', // Ö → O
        0x86 => b'U', // Ü → U
        0xAE => b'A', // Æ → A
        0xAF => b'O', // Ø → O
        0xCB => b'A', // À → A
        0xCC => b'A', // Ã → A
        0xCD => b'O', // Õ → O
        0xCE => b'O', // Œ → O
        // Lowercase accented → lowercase base
        0x87 => b'a', // á → a
        0x88 => b'a', // à → a
        0x89 => b'a', // â → a
        0x8A => b'a', // ä → a
        0x8B => b'a', // ã → a
        0x8C => b'a', // å → a
        0x8D => b'c', // ç → c
        0x8E => b'e', // é → e
        0x8F => b'e', // è → e
        0x90 => b'e', // ê → e
        0x91 => b'e', // ë → e
        0x92 => b'i', // í → i
        0x93 => b'i', // ì → i
        0x94 => b'i', // î → i
        0x95 => b'i', // ï → i
        0x96 => b'n', // ñ → n
        0x97 => b'o', // ó → o
        0x98 => b'o', // ò → o
        0x99 => b'o', // ô → o
        0x9A => b'o', // ö → o
        0x9B => b'o', // õ → o
        0x9C => b'u', // ú → u
        0x9D => b'u', // ù → u
        0x9E => b'u', // û → u
        0x9F => b'u', // ü → u
        0xBB => b'a', // ª → a
        0xBC => b'o', // º → o
        0xBE => b'a', // æ → a
        0xBF => b'o', // ø → o
        0xCF => b'o', // œ → o
        0xD8 => b'y', // ÿ → y
        _ => ch,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::{setup, TEST_SP};
    use crate::cpu::{CpuOps, Register};
    use crate::memory::globals::addr;
    use crate::memory::MemoryBus;
    use crate::trap::dispatch::{LoadedResources, ResourceFileMap};
    use std::collections::HashMap;

    // ==================== OS Traps (is_tool=false) ====================

    #[test]
    fn test_new_ptr() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 256);
        let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewPtr should be handled");
        assert!(result.unwrap().is_ok(), "NewPtr should succeed");
        let ptr = cpu.read_reg(Register::A0);
        assert!(
            ptr >= 0x200000,
            "NewPtr should return a valid heap pointer, got ${:08X}",
            ptr
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "NewPtr should set D0 to 0 (noErr)"
        );
    }

    #[test]
    fn negative_new_ptr_size_returns_mem_full_without_heap_wrap() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        bus.write_long(addr::J_CRSR_TASK, 0x1234_5678);
        cpu.write_reg(Register::D0, (-108i32) as u32);

        let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);

        assert!(result.is_some() && result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A0), 0);
        assert_eq!(cpu.read_reg(Register::D0), (-108i32) as u32);
        assert_eq!(bus.read_long(addr::J_CRSR_TASK), 0x1234_5678);
    }

    #[test]
    fn negative_new_handle_size_returns_mem_full_without_heap_wrap() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        bus.write_long(addr::J_CRSR_TASK, 0x1234_5678);
        cpu.write_reg(Register::D0, (-108i32) as u32);

        let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);

        assert!(result.is_some() && result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A0), 0);
        assert_eq!(cpu.read_reg(Register::D0), (-108i32) as u32);
        assert_eq!(bus.read_long(addr::J_CRSR_TASK), 0x1234_5678);
    }

    #[test]
    fn new_ptr_extends_stale_application_zone_past_successful_allocation() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let app_zone = 0x0020_0000;
        let old_limit = 0x0020_1000;
        let allocation_start = 0x0020_2000;
        let size = 0x100;
        let original_free = 0x800;

        bus.write_long(addr::APP_L_ZONE, app_zone);
        bus.write_long(addr::THE_ZONE, app_zone);
        bus.write_long(addr::HEAP_END, app_zone + 64);
        bus.write_long(addr::APPL_LIMIT, old_limit);
        bus.write_long(addr::BUF_PTR, old_limit);
        bus.write_long(app_zone, old_limit); // bkLim
        bus.write_long(app_zone + 12, original_free); // zcbFree
        bus.reserve_heap_until(allocation_start);

        // $A11E is the ordinary NewPtr entry point used by MPW clients.
        dispatcher.current_trap_word = 0xA11E;
        cpu.write_reg(Register::D0, size);
        let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewPtr should be handled");
        assert!(result.unwrap().is_ok(), "NewPtr should succeed");

        let ptr = cpu.read_reg(Register::A0);
        let allocation_end = ptr + size;
        assert_eq!(ptr, allocation_start);
        assert_eq!(
            bus.read_long(app_zone),
            allocation_end,
            "bkLim must remain immediately beyond every successful application-zone allocation"
        );
        assert_eq!(bus.read_long(addr::APPL_LIMIT), allocation_end);
        assert_eq!(bus.read_long(addr::BUF_PTR), allocation_end);
        assert_eq!(bus.read_long(addr::HEAP_END), allocation_end);
        assert_eq!(
            bus.read_long(app_zone + 12),
            original_free,
            "already-consumed backing span must not be added to zcbFree"
        );
        assert!(
            ptr >= app_zone && ptr < bus.read_long(app_zone),
            "a successful NewPtr result must pass the classic [zone, bkLim) validity test"
        );
    }

    #[test]
    fn new_ptr_sys_does_not_extend_application_zone() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let app_zone = 0x0020_0000;
        let old_limit = 0x0020_1000;
        let allocation_start = 0x0020_2000;

        bus.write_long(addr::APP_L_ZONE, app_zone);
        bus.write_long(addr::THE_ZONE, app_zone);
        bus.write_long(addr::HEAP_END, old_limit);
        bus.write_long(addr::APPL_LIMIT, old_limit);
        bus.write_long(app_zone, old_limit); // bkLim
        bus.reserve_heap_until(allocation_start);

        dispatcher.current_trap_word = 0xA51E; // NewPtrSys
        cpu.write_reg(Register::D0, 0x100);
        let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
        assert!(result.is_some() && result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A0), allocation_start);
        assert_eq!(
            bus.read_long(app_zone),
            old_limit,
            "system-heap allocations must not widen the application zone"
        );
        assert_eq!(bus.read_long(addr::APPL_LIMIT), old_limit);
        assert_eq!(bus.read_long(addr::HEAP_END), old_limit);
    }

    #[test]
    fn test_new_handle() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 512);
        let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewHandle should be handled");
        assert!(result.unwrap().is_ok(), "NewHandle should succeed");
        let handle = cpu.read_reg(Register::A0);
        assert!(
            handle >= 0x200000,
            "NewHandle should return a valid handle address, got ${:08X}",
            handle
        );
        let ptr = bus.read_long(handle);
        assert!(
            ptr >= 0x200000,
            "NewHandle's handle should point to a valid ptr, got ${:08X}",
            ptr
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "NewHandle should set D0 to 0 (noErr)"
        );
    }

    #[test]
    fn new_handle_clear_clears_stale_memerr_on_success() {
        // Inside Macintosh Volume IV, IV-80: Memory Manager routines store
        // the most recent result code in the low-memory MemErr word.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        bus.write_word(addr::MEM_ERR, 0x1E6C);
        dispatcher.current_trap_word = 0xA322; // NewHandleClear
        cpu.write_reg(Register::D0, 68);

        let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewHandleClear should be handled");
        assert!(result.unwrap().is_ok(), "NewHandleClear should succeed");
        assert_ne!(cpu.read_reg(Register::A0), 0, "allocation should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0, "D0 should report noErr");
        assert_eq!(
            bus.read_word(addr::MEM_ERR),
            0,
            "NewHandleClear must clear stale MemErr on success"
        );
    }

    #[test]
    fn new_allocations_follow_clear_contracts() {
        let (mut dispatcher, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, 64);
        dispatcher.current_trap_word = 0xA01E; // NewPtr
        let ptr_result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
        assert!(ptr_result.is_some() && ptr_result.unwrap().is_ok());
        let ptr = cpu.read_reg(Register::A0);
        assert_ne!(
            bus.read_long(ptr),
            0,
            "regular NewPtr allocations should not be zero-filled"
        );

        cpu.write_reg(Register::D0, 64);
        dispatcher.current_trap_word = 0xA31E; // NewPtrClear
        let clear_ptr_result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
        assert!(clear_ptr_result.is_some() && clear_ptr_result.unwrap().is_ok());
        let clear_ptr = cpu.read_reg(Register::A0);
        assert_eq!(
            bus.read_long(clear_ptr),
            0,
            "NewPtrClear allocations should be zero-filled"
        );

        cpu.write_reg(Register::D0, 64);
        dispatcher.current_trap_word = 0xA022; // NewHandle
        let handle_result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(handle_result.is_some() && handle_result.unwrap().is_ok());
        let handle = cpu.read_reg(Register::A0);
        let data_ptr = bus.read_long(handle);
        assert_ne!(
            bus.read_long(data_ptr),
            0,
            "regular NewHandle allocations should not be zero-filled"
        );

        cpu.write_reg(Register::D0, 64);
        dispatcher.current_trap_word = 0xA322; // NewHandleClear
        let clear_handle_result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(clear_handle_result.is_some() && clear_handle_result.unwrap().is_ok());
        let clear_handle = cpu.read_reg(Register::A0);
        let clear_data_ptr = bus.read_long(clear_handle);
        assert_eq!(
            bus.read_long(clear_data_ptr),
            0,
            "NewHandleClear allocations should be zero-filled"
        );
    }

    #[test]
    fn test_dispose_ptr() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000);
        let result = dispatcher.dispatch_memory(false, 0x1F, &mut cpu, &mut bus);
        assert!(result.is_some(), "DisposePtr should be handled");
        assert!(result.unwrap().is_ok(), "DisposePtr should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DisposePtr should set D0 to 0"
        );
    }

    #[test]
    fn test_dispose_handle() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000);
        let result = dispatcher.dispatch_memory(false, 0x23, &mut cpu, &mut bus);
        assert!(result.is_some(), "DisposeHandle should be handled");
        assert!(result.unwrap().is_ok(), "DisposeHandle should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DisposeHandle should set D0 to 0"
        );
    }

    #[test]
    fn dispose_resource_handle_preserves_resource_backing_for_later_lookup() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(4);
        bus.write_bytes(data_ptr, &[0xDE, 0xAD, 0xBE, 0xEF]);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        dispatcher
            .loaded_handles
            .insert(handle, (data_ptr, *b"RSRC", 7));
        dispatcher.resource_handle_files.insert(handle, 0);
        dispatcher.resources = Some(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"RSRC", 7), data_ptr)]),
                    named: HashMap::new(),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        cpu.write_reg(Register::A0, handle);
        let result = dispatcher.dispatch_memory(false, 0x23, &mut cpu, &mut bus);
        assert!(result.is_some(), "DisposeHandle should be handled");
        assert!(result.unwrap().is_ok(), "DisposeHandle should return");

        assert_eq!(
            bus.get_alloc_size(handle),
            None,
            "DisposeHandle should still release the master-pointer slot"
        );
        assert_eq!(
            bus.read_bytes(data_ptr, 4),
            vec![0xDE, 0xAD, 0xBE, 0xEF],
            "resource backing must remain valid after its handle is disposed"
        );
        assert!(!dispatcher.loaded_handles.contains_key(&handle));
        assert!(!dispatcher.resource_handle_files.contains_key(&handle));
        assert_eq!(
            dispatcher.find_resource_any(*b"RSRC", 7),
            Some((0, data_ptr)),
            "Resource Manager map should still point at live backing data"
        );

        let new_handle = dispatcher.get_or_create_resource_handle(&mut bus, *b"RSRC", 7, data_ptr);
        assert_ne!(new_handle, 0);
        assert_eq!(bus.read_long(new_handle), data_ptr);
    }

    #[test]
    fn test_hlock() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000);
        let result = dispatcher.dispatch_memory(false, 0x29, &mut cpu, &mut bus);
        assert!(result.is_some(), "HLock should be handled");
        assert!(result.unwrap().is_ok(), "HLock should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0, "HLock should set D0 to 0");
    }

    #[test]
    fn test_hunlock() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000);
        let result = dispatcher.dispatch_memory(false, 0x2A, &mut cpu, &mut bus);
        assert!(result.is_some(), "HUnlock should be handled");
        assert!(result.unwrap().is_ok(), "HUnlock should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0, "HUnlock should set D0 to 0");
    }

    #[test]
    fn test_heapdispatch_sets_noerr_and_ccr_like_other_memory_dispatchers() {
        let (mut dispatcher, mut cpu, mut bus) = setup();

        // Seed the CCR with non-zero flags so we can prove the trap
        // rewrites it instead of leaving stale condition codes behind.
        cpu.set_ccr(0x1F);

        let result = dispatcher.dispatch_memory(false, 0xA4, &mut cpu, &mut bus);
        assert!(result.is_some(), "HeapDispatch should be handled");
        assert!(result.unwrap().is_ok(), "HeapDispatch should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HeapDispatch should set D0 to 0"
        );
        assert_eq!(
            cpu.get_ccr() & 0x1F,
            0x14,
            "HeapDispatch should leave X set and Z asserted for the noErr path"
        );
    }

    fn assert_nohw_trap_preserves_d0_and_stack_and_updates_ccr(
        dispatcher: &mut crate::trap::TrapDispatcher,
        cpu: &mut impl CpuOps,
        bus: &mut crate::memory::MacMemoryBus,
        trap_num: u16,
        trap_name: &str,
        d0_seed: u32,
        expected_ccr: u8,
    ) {
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, d0_seed);
        cpu.set_ccr(0x1F);

        let result = dispatcher.dispatch_memory(false, trap_num, cpu, bus);
        assert!(result.is_some(), "{trap_name} should be handled");
        assert!(result.unwrap().is_ok(), "{trap_name} should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            d0_seed,
            "{trap_name} should preserve D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "{trap_name} should not consume a Pascal argument frame"
        );
        assert_eq!(
            cpu.get_ccr() & 0x1F,
            expected_ccr,
            "{trap_name} should mirror the preserved D0 into the dispatcher CCR"
        );
    }

    #[test]
    fn vadbproc_preserves_d0_and_stack_and_updates_ccr() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        assert_nohw_trap_preserves_d0_and_stack_and_updates_ccr(
            &mut dispatcher,
            &mut cpu,
            &mut bus,
            0xAE,
            "VADBProc",
            0x1234_5678,
            0x10,
        );
    }

    #[test]
    fn scsiaction_scsigetvirtualidinfo_missing_virtual_id_clears_exists_and_preserves_stack() {
        let (mut dispatcher, mut cpu, mut bus) = setup();

        const PB_SIZE: u32 = 40;
        const PB_LENGTH_OFFSET: usize = 6;
        const PB_FUNCTION_CODE_OFFSET: usize = 8;
        const PB_RESULT_OFFSET: usize = 10;
        const PB_OLD_CALL_ID_OFFSET: usize = 36;
        const PB_EXISTS_OFFSET: usize = 38;
        const SCSI_GET_VIRTUAL_ID_INFO_FUNCTION_CODE: u8 = 0x80;

        let scsi_pb = bus.alloc(PB_SIZE);
        let sp_before = cpu.read_reg(Register::A7);
        let mut i: usize = 0;

        while i < PB_SIZE as usize {
            bus.write_byte(scsi_pb + i as u32, 0);
            i += 1;
        }
        bus.write_word(scsi_pb + PB_LENGTH_OFFSET as u32, PB_SIZE as u16);
        bus.write_byte(
            scsi_pb + PB_FUNCTION_CODE_OFFSET as u32,
            SCSI_GET_VIRTUAL_ID_INFO_FUNCTION_CODE,
        );
        bus.write_word(scsi_pb + PB_RESULT_OFFSET as u32, 0x3FFF);
        bus.write_word(scsi_pb + PB_OLD_CALL_ID_OFFSET as u32, 0x1234);
        bus.write_byte(scsi_pb + PB_EXISTS_OFFSET as u32, 0xFF);

        cpu.write_reg(Register::A0, scsi_pb);
        cpu.write_reg(Register::D0, 0x0000_0001);

        let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
        assert!(result.is_some(), "SCSIAtomic should be handled");
        assert!(result.unwrap().is_ok(), "SCSIAtomic should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SCSIAtomic should return noErr"
        );
        assert_eq!(
            bus.read_word(scsi_pb + PB_RESULT_OFFSET as u32),
            0,
            "SCSIAction should mirror noErr into scsiResult"
        );
        assert_eq!(
            bus.read_byte(scsi_pb + PB_EXISTS_OFFSET as u32),
            0,
            "SCSIGetVirtualIDInfo should clear scsiExists for a missing virtual ID"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SCSIAction should preserve StackSpace"
        );
    }

    #[test]
    fn scsiaction_scsigetvirtualidinfo_rejects_nonzero_qlink_with_qlink_invalid() {
        // Inside Macintosh: Devices (1994), pp. 4-49 to 4-50 and 4-21:
        // qLink is part of the common SCSIAction header and must be 0.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        const PB_SIZE: u32 = 40;
        const PB_RESULT_OFFSET: usize = 10;
        const PB_EXISTS_OFFSET: usize = 38;

        let scsi_pb = bus.alloc(PB_SIZE);
        let sp_before = cpu.read_reg(Register::A7);

        for i in 0..PB_SIZE as usize {
            bus.write_byte(scsi_pb + i as u32, 0);
        }
        bus.write_long(scsi_pb, 0x0000_0001);
        bus.write_word(scsi_pb + 6, PB_SIZE as u16);
        bus.write_byte(scsi_pb + 8, 0x80);
        bus.write_word(scsi_pb + PB_RESULT_OFFSET as u32, 0x3FFF);
        bus.write_byte(scsi_pb + PB_EXISTS_OFFSET as u32, 0xFF);

        cpu.write_reg(Register::A0, scsi_pb);
        cpu.write_reg(Register::D0, 0x0000_0001);

        let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
        assert!(result.is_some(), "SCSIAtomic should be handled");
        assert!(result.unwrap().is_ok(), "SCSIAtomic should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -7881,
            "SCSIAtomic should return scsiQLinkInvalid for a nonzero qLink"
        );
        assert_eq!(
            bus.read_word(scsi_pb + PB_RESULT_OFFSET as u32) as i16,
            -7881,
            "SCSIAction should mirror scsiQLinkInvalid into scsiResult"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SCSIAction should preserve StackSpace on qLink errors"
        );
        assert_eq!(
            bus.read_byte(scsi_pb + PB_EXISTS_OFFSET as u32),
            0xFF,
            "SCSIGetVirtualIDInfo should leave scsiExists untouched on qLink errors"
        );
    }

    #[test]
    fn scsiaction_scsigetvirtualidinfo_rejects_short_parameter_block_with_length_error() {
        // Inside Macintosh: Devices (1994), pp. 4-49 to 4-50 and 4-21:
        // SCSIAction checks the parameter block length before attempting
        // to use SCSIGetVirtualIDInfo-specific fields.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        const PB_SIZE: u32 = 40;
        const PB_RESULT_OFFSET: usize = 10;
        const PB_EXISTS_OFFSET: usize = 38;

        let scsi_pb = bus.alloc(PB_SIZE);
        let sp_before = cpu.read_reg(Register::A7);

        for i in 0..PB_SIZE as usize {
            bus.write_byte(scsi_pb + i as u32, 0);
        }
        bus.write_word(scsi_pb + 6, 8);
        bus.write_byte(scsi_pb + 8, 0x80);
        bus.write_word(scsi_pb + PB_RESULT_OFFSET as u32, 0x3FFF);
        bus.write_byte(scsi_pb + PB_EXISTS_OFFSET as u32, 0xFF);

        cpu.write_reg(Register::A0, scsi_pb);
        cpu.write_reg(Register::D0, 0x0000_0001);

        let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
        assert!(result.is_some(), "SCSIAtomic should be handled");
        assert!(result.unwrap().is_ok(), "SCSIAtomic should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -7872,
            "SCSIAtomic should return scsiPBLengthError for a short parameter block"
        );
        assert_eq!(
            bus.read_word(scsi_pb + PB_RESULT_OFFSET as u32) as i16,
            -7872,
            "SCSIAction should mirror scsiPBLengthError into scsiResult"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SCSIAction should preserve StackSpace on PB length errors"
        );
        assert_eq!(
            bus.read_byte(scsi_pb + PB_EXISTS_OFFSET as u32),
            0xFF,
            "SCSIGetVirtualIDInfo should leave scsiExists untouched on PB length errors"
        );
    }

    #[test]
    fn scsiaction_scsigetvirtualidinfo_rejects_non_nil_completion_with_request_invalid() {
        // Inside Macintosh: Devices (1994), pp. 4-49 to 4-50 and 4-21:
        // SCSIGetVirtualIDInfo is synchronous, so scsiCompletion must be nil.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        const PB_SIZE: u32 = 40;
        const PB_RESULT_OFFSET: usize = 10;
        const PB_COMPLETION_OFFSET: usize = 16;

        let scsi_pb = bus.alloc(PB_SIZE);

        for i in 0..PB_SIZE as usize {
            bus.write_byte(scsi_pb + i as u32, 0);
        }
        bus.write_word(scsi_pb + 6, PB_SIZE as u16);
        bus.write_byte(scsi_pb + 8, 0x80);
        bus.write_long(scsi_pb + PB_COMPLETION_OFFSET as u32, 0x0000_1234);
        bus.write_word(scsi_pb + PB_RESULT_OFFSET as u32, 0x3FFF);

        cpu.write_reg(Register::A0, scsi_pb);
        cpu.write_reg(Register::D0, 0x0000_0001);

        let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
        assert!(result.is_some(), "SCSIAtomic should be handled");
        assert!(result.unwrap().is_ok(), "SCSIAtomic should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -7870,
            "SCSIAtomic should return scsiRequestInvalid for a non-nil completion"
        );
        assert_eq!(
            bus.read_word(scsi_pb + PB_RESULT_OFFSET as u32) as i16,
            -7870,
            "SCSIAction should mirror scsiRequestInvalid into scsiResult"
        );
    }

    #[test]
    fn scsiaction_scsigetvirtualidinfo_rejects_nil_parameter_block_with_request_invalid() {
        // Inside Macintosh: Devices (1994), pp. 4-38 to 4-39 and p. 4-90:
        // SCSIAction takes a pointer to a SCSI Manager parameter block, so
        // a NIL scsiPB is an invalid request.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        let sp_before = cpu.read_reg(Register::A7);

        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::D0, 0x0000_0001);

        let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
        assert!(result.is_some(), "SCSIAtomic should be handled");
        assert!(result.unwrap().is_ok(), "SCSIAtomic should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -7870,
            "SCSIAtomic should return scsiRequestInvalid for a NIL parameter block"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SCSIAction should preserve StackSpace on a NIL parameter block"
        );
    }

    #[test]
    fn pmgrop_returns_noerr_and_preserves_stack_pointer_in_noop_path() {
        // Inside Macintosh: Devices (1994), Power Manager chapter.
        // PMgrOp is the no-op Power Manager dispatcher on systems with no
        // hardware power management. The HLE path should consume no Pascal
        // arguments, return noErr in D0, and leave A7 untouched.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        cpu.write_reg(Register::D0, 0);
        let result = dispatcher.dispatch_memory(false, 0x85, &mut cpu, &mut bus);
        assert!(result.is_some(), "PMgrOp should be handled");
        assert!(result.unwrap().is_ok(), "PMgrOp should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0, "PMgrOp should return noErr");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "PMgrOp should preserve the caller stack pointer"
        );
    }

    fn assert_no_hardware_trap_returns_noerr_and_preserves_stack(trap_num: u16, trap_name: &str) {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xA5A5_5A5A);

        cpu.write_reg(Register::D0, 0xFFFF_FFFF);
        let result = dispatcher.dispatch_memory(false, trap_num, &mut cpu, &mut bus);
        assert!(result.is_some(), "{trap_name} should be handled");
        assert!(result.unwrap().is_ok(), "{trap_name} should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "{trap_name} should return noErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "{trap_name} should preserve the caller stack pointer"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xA5A5_5A5A,
            "{trap_name} should not consume or rewrite stack arguments"
        );
    }

    #[test]
    fn iopinfoaccess_returns_noerr_and_preserves_stack_pointer() {
        // The generic target has no IOP hardware, so the HLE path is the
        // same no-hardware compatibility return used by IOPMoveData.
        assert_no_hardware_trap_returns_noerr_and_preserves_stack(0x86, "IOPInfoAccess");
    }

    #[test]
    fn iopmsgrequest_returns_noerr_and_preserves_stack_pointer() {
        assert_no_hardware_trap_returns_noerr_and_preserves_stack(0x87, "IOPMsgRequest");
    }

    #[test]
    fn egretdispatch_returns_noerr_and_preserves_stack_pointer() {
        assert_no_hardware_trap_returns_noerr_and_preserves_stack(0x92, "EgretDispatch");
    }

    #[test]
    fn hsetrbit_sets_resource_flag_for_valid_handle() {
        // Inside Macintosh: Memory (1992), pp. 2-49 to 2-50:
        // HSetRBit sets a handle's resource flag and returns noErr.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(16);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);
        let sp_before = cpu.read_reg(Register::A7);

        cpu.write_reg(Register::A0, handle);
        let result = dispatcher.dispatch_memory(false, 0x67, &mut cpu, &mut bus);
        assert!(result.is_some(), "HSetRBit should be handled");
        assert!(result.unwrap().is_ok(), "HSetRBit should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HSetRBit should return noErr for a valid handle"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "HSetRBit uses register calling convention and should preserve A7"
        );
        assert_eq!(
            dispatcher
                .handle_state_bits
                .get(&handle)
                .copied()
                .unwrap_or(0)
                & 0x20,
            0x20,
            "HSetRBit should set the resource bit in tracked handle state"
        );
    }

    #[test]
    fn hsetrbit_nil_handle_returns_nilhandleerr_in_d0() {
        // Inside Macintosh: Memory (1992), pp. 2-49 to 2-50:
        // nilHandleErr (-109) is returned for a NIL master pointer.
        let (mut dispatcher, mut cpu, mut _bus) = setup();
        cpu.write_reg(Register::A0, 0);

        let result = dispatcher.dispatch_memory(false, 0x67, &mut cpu, &mut _bus);
        assert!(result.is_some(), "HSetRBit should be handled");
        assert!(result.unwrap().is_ok(), "HSetRBit should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -109,
            "HSetRBit should return nilHandleErr for a NIL handle"
        );
    }

    #[test]
    fn hclrrbit_clears_resource_flag_for_valid_handle() {
        // Inside Macintosh: Memory (1992), pp. 2-50 to 2-51:
        // HClrRBit clears a handle's resource flag and returns noErr.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(16);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);
        dispatcher.handle_state_bits.insert(handle, 0x20);
        let sp_before = cpu.read_reg(Register::A7);

        cpu.write_reg(Register::A0, handle);
        let result = dispatcher.dispatch_memory(false, 0x68, &mut cpu, &mut bus);
        assert!(result.is_some(), "HClrRBit should be handled");
        assert!(result.unwrap().is_ok(), "HClrRBit should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HClrRBit should return noErr for a valid handle"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "HClrRBit uses register calling convention and should preserve A7"
        );
        assert_eq!(
            dispatcher
                .handle_state_bits
                .get(&handle)
                .copied()
                .unwrap_or(0)
                & 0x20,
            0,
            "HClrRBit should clear the resource bit in tracked handle state"
        );
    }

    #[test]
    fn hclrrbit_nil_handle_returns_nilhandleerr_in_d0() {
        // Inside Macintosh: Memory (1992), pp. 2-50 to 2-51:
        // nilHandleErr (-109) is returned for a NIL master pointer.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0);

        let result = dispatcher.dispatch_memory(false, 0x68, &mut cpu, &mut bus);
        assert!(result.is_some(), "HClrRBit should be handled");
        assert!(result.unwrap().is_ok(), "HClrRBit should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -109,
            "HClrRBit should return nilHandleErr for a NIL handle"
        );
    }

    #[test]
    fn hsetrbit_then_hgetstate_round_trips_resource_bit() {
        // Pins the documented HSetRBit ↔ HGetState round-trip contract
        // (BasiliskII System 7.5.3 ROM). Per IM:Memory 1992 p. 2-43, callers
        // observe handle state ONLY through HGetState ($A069); the master
        // pointer flag byte is not portable across 24/32-bit modes. This
        // test exercises that documented API: after HSetRBit on a freshly
        // allocated handle (whose pre-state is 0x00 per IM:Memory 1992
        // p. 2-27), HGetState must return the byte with bit 5 (0x20) set.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(16);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        // Pre: HGetState reports clear state.
        cpu.write_reg(Register::A0, handle);
        let pre = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
        assert!(pre.is_some() && pre.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0) & 0x20,
            0,
            "fresh NewHandle should leave the resource bit clear per IM:Memory 1992 p. 2-27"
        );

        // HSetRBit on the handle.
        cpu.write_reg(Register::A0, handle);
        let set = dispatcher.dispatch_memory(false, 0x67, &mut cpu, &mut bus);
        assert!(set.is_some() && set.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HSetRBit on a valid handle should return noErr"
        );

        // Post: HGetState reports the resource bit set.
        cpu.write_reg(Register::A0, handle);
        let post = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
        assert!(post.is_some() && post.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0) & 0x20,
            0x20,
            "HGetState should report the resource bit set after HSetRBit"
        );
    }

    #[test]
    fn hclrrbit_after_hsetrbit_round_trips_resource_bit_to_zero() {
        // Symmetric counterpart to hsetrbit_then_hgetstate_round_trips_resource_bit:
        // pins the HClrRBit ↔ HGetState round-trip. After HSetRBit then HClrRBit
        // on the same handle, HGetState must report the resource bit
        // cleared (0x00) — per IM:Memory 1992 p. 2-50 "HClrRBit clears
        // the resource flag of a relocatable block."
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(16);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        cpu.write_reg(Register::A0, handle);
        let set = dispatcher.dispatch_memory(false, 0x67, &mut cpu, &mut bus);
        assert!(set.is_some() && set.unwrap().is_ok());

        cpu.write_reg(Register::A0, handle);
        let mid = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
        assert!(mid.is_some() && mid.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0) & 0x20,
            0x20,
            "precondition: HSetRBit should leave the resource bit set"
        );

        cpu.write_reg(Register::A0, handle);
        let clear = dispatcher.dispatch_memory(false, 0x68, &mut cpu, &mut bus);
        assert!(clear.is_some() && clear.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HClrRBit on a valid handle should return noErr"
        );

        cpu.write_reg(Register::A0, handle);
        let post = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
        assert!(post.is_some() && post.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0) & 0x20,
            0,
            "HGetState should report the resource bit clear after HClrRBit"
        );
    }

    #[test]
    fn readdatetime_returns_current_time_global_without_host_clock_reset() {
        // The runner advances low-memory Time ($020C). ReadDateTime should
        // expose that synthetic clock instead of replacing it with host time.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let secs = 0x1234_5678u32;
        let out_ptr = bus.alloc(4);
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(0x020C, secs);
        bus.write_long(out_ptr, 0);
        cpu.write_reg(Register::A0, out_ptr);

        let result = dispatcher.dispatch_memory(false, 0x39, &mut cpu, &mut bus);

        assert!(result.is_some(), "ReadDateTime should be handled");
        assert!(
            result.unwrap().is_ok(),
            "ReadDateTime should return cleanly"
        );
        assert_eq!(bus.read_long(out_ptr), secs);
        assert_eq!(bus.read_long(0x020C), secs);
        assert_eq!(cpu.read_reg(Register::D0), 0, "ReadDateTime returns noErr");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "ReadDateTime should use register calling convention and preserve A7"
        );
    }

    #[test]
    fn setdatetime_updates_time_global_from_d0_seconds_argument() {
        // Inside Macintosh Volume II (1985), pp. II-378 to II-379:
        // _SetDateTime takes secs in D0 and updates the Time global.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let secs = 0x1234_5678u32;
        bus.write_long(0x020C, 0xDEAD_BEEF);
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, secs);

        let result = dispatcher.dispatch_memory(false, 0x3A, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetDateTime should be handled");
        assert!(result.unwrap().is_ok(), "SetDateTime should return cleanly");
        assert_eq!(
            bus.read_long(0x020C),
            secs,
            "SetDateTime should copy D0 seconds into low-memory Time ($020C)"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SetDateTime should use register calling convention and preserve A7"
        );
    }

    #[test]
    fn setdatetime_returns_noerr_result_code_in_d0_for_nominal_call() {
        // Inside Macintosh Volume II (1985), p. II-391 register summary:
        // _SetDateTime returns result code in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0x0102_0304);

        let result = dispatcher.dispatch_memory(false, 0x3A, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetDateTime should be handled");
        assert!(result.unwrap().is_ok(), "SetDateTime should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetDateTime nominal path should return noErr in D0"
        );
    }

    #[test]
    fn setdatetime_returns_noerr_regardless_of_secs_input() {
        // Inside Macintosh Volume II (1985), pp. II-378..II-379 + II-391:
        // _SetDateTime takes D0=secs and returns D0=OSErr; both engines
        // return noErr on the nominal write path for any LongInt input.
        // Sweeps multiple secs values and checks the noErr return + A7
        // preservation.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let secs_inputs = [
            0u32, // 1904-01-01 epoch
            1,
            0x1234_5678,
            0x7FFF_FFFF, // max signed LongInt
            0xFFFF_FFFF, // sentinel high bits
        ];
        for &secs in &secs_inputs {
            let sp_before = cpu.read_reg(Register::A7);
            cpu.write_reg(Register::D0, secs);
            let result = dispatcher.dispatch_memory(false, 0x3A, &mut cpu, &mut bus);
            assert!(result.is_some(), "SetDateTime should be handled");
            assert!(result.unwrap().is_ok(), "SetDateTime should return cleanly");
            assert_eq!(
                cpu.read_reg(Register::D0) & 0xFF,
                0,
                "SetDateTime should return noErr lowbyte for secs=0x{:08X}",
                secs,
            );
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp_before,
                "SetDateTime should preserve A7 (register-only ABI) for secs=0x{:08X}",
                secs,
            );
        }
    }

    #[test]
    fn writeparam_returns_noerr_in_d0_for_nominal_call() {
        // Inside Macintosh Volume II (1985), pp. II-381 to II-382 and p. II-391:
        // _WriteParam returns result code in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, 0x01F8); // SysParam
        cpu.write_reg(Register::D0, 0xFFFF_FFFF); // MinusOne

        let result = dispatcher.dispatch_memory(false, 0x38, &mut cpu, &mut bus);
        assert!(result.is_some(), "WriteParam should be handled");
        assert!(result.unwrap().is_ok(), "WriteParam should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "WriteParam nominal path should return noErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "WriteParam should use register calling convention and preserve A7"
        );
    }

    #[test]
    fn writeparam_does_not_modify_low_memory_sysparam_copy() {
        // Inside Macintosh Volume II (1985), pp. II-381 to II-382:
        // WriteParam writes the existing low-memory SysParam copy to parameter RAM.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sys_param = 0x01F8u32;
        let mut before = [0u8; 20];
        for (i, slot) in before.iter_mut().enumerate() {
            let value = (i as u8).wrapping_mul(7).wrapping_add(3);
            bus.write_byte(sys_param + i as u32, value);
            *slot = value;
        }
        cpu.write_reg(Register::A0, sys_param);
        cpu.write_reg(Register::D0, 0xFFFF_FFFF);

        let result = dispatcher.dispatch_memory(false, 0x38, &mut cpu, &mut bus);
        assert!(result.is_some(), "WriteParam should be handled");
        assert!(result.unwrap().is_ok(), "WriteParam should return cleanly");
        for (i, expected) in before.iter().enumerate() {
            assert_eq!(
                bus.read_byte(sys_param + i as u32),
                *expected,
                "WriteParam should preserve low-memory SysParam byte {}",
                i
            );
        }
    }

    #[test]
    fn writeparam_five_call_composition_preserves_stack_across_varying_minusone_register_state() {
        // 5 successive _WriteParam dispatches inside one A7-snapshot
        // sandwich with varying D0 entry
        // values (always logical "MinusOne" but seeded with varying high-byte
        // stale state to stress D0 input handling). Per IM:II II-381 +
        // IM:OSUtils 1994 p. 7-13 the OS-bit FUNCTION consumes no Pascal
        // stack arguments; A7 is unchanged across every call.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sys_param = 0x01F8u32;
        let sentinel = 0xBADC_0DE0u32;
        bus.write_long(sys_param + 16, sentinel); // SP+16 sentinel within SysParam record

        let a7_pre = cpu.read_reg(Register::A7);
        let d0_inputs = [
            0xFFFF_FFFFu32,
            0xFFFF_FFFEu32,
            0xFFFF_0000u32,
            0xFFFF_FFFFu32,
            0xFFFF_FF00u32,
        ];
        for d0_in in d0_inputs.iter().copied() {
            cpu.write_reg(Register::A0, sys_param);
            cpu.write_reg(Register::D0, d0_in);
            let result = dispatcher.dispatch_memory(false, 0x38, &mut cpu, &mut bus);
            assert!(result.is_some(), "WriteParam should be handled");
            assert!(result.unwrap().is_ok(), "WriteParam should return cleanly");
            assert_eq!(
                cpu.read_reg(Register::D0),
                0,
                "WriteParam nominal path should return noErr in D0 across varying MinusOne high-byte state"
            );
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            a7_pre,
            "Five WriteParam calls must net zero A7 delta (register-only OS-bit ABI)"
        );
        assert_eq!(
            bus.read_long(sys_param + 16),
            sentinel,
            "SysParam sentinel preserved across all five WriteParam calls"
        );
    }

    #[test]
    fn initutil_returns_noerr_in_d0_for_nominal_call() {
        // Inside Macintosh Volume II (1985), pp. II-380 to II-381 and p. II-391:
        // _InitUtil exits with result code in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0xFFFF_FFFF);

        let result = dispatcher.dispatch_memory(false, 0x3F, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitUtil should be handled");
        assert!(result.unwrap().is_ok(), "InitUtil should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "InitUtil nominal path should return noErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "InitUtil takes no stack arguments and should preserve A7"
        );
    }

    #[test]
    fn initutil_sets_spvalid_byte_to_a8_on_success() {
        // Inside Macintosh Volume II (1985), pp. II-380 to II-381:
        // InitUtil initializes low-memory parameter RAM state from the clock chip.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        bus.write_byte(0x01F8, 0x00);

        let result = dispatcher.dispatch_memory(false, 0x3F, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitUtil should be handled");
        assert!(result.unwrap().is_ok(), "InitUtil should return cleanly");
        assert_eq!(
            bus.read_byte(0x01F8),
            0xA8,
            "InitUtil nominal path should mark SPValid ($01F8) as valid ($A8)"
        );
    }

    #[test]
    fn initutil_rewrites_spvalid_when_pre_poisoned_to_invalid() {
        // Pre-poisons SPValid
        // ($01F8) with $5A (canonical "invalid" per IM:II II-380 — any
        // byte other than $A8 means parameter RAM has not been validated
        // since the last reset), dispatches _InitUtil, and verifies that
        // (a) D0 returns noErr (0), (b) SPValid was overwritten to $A8.
        // Pinning the rewrite-from-invalid path defeats no-op stubs that
        // would leave SPValid at its incoming value.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_byte(0x01F8, 0x5A);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        let result = dispatcher.dispatch_memory(false, 0x3F, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitUtil should be handled");
        assert!(result.unwrap().is_ok(), "InitUtil should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) & 0xFF,
            0,
            "InitUtil must return noErr (0) in D0 lowbyte after rewriting invalid SPValid"
        );
        assert_eq!(
            bus.read_byte(0x01F8),
            0xA8,
            "InitUtil must overwrite a pre-poisoned $5A SPValid with $A8 (valid stamp)"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "InitUtil takes no stack arguments and must preserve A7"
        );
    }

    #[test]
    fn initutil_register_only_calling_convention_preserves_stack_pointer() {
        // Repeated register-only calls should not consume a stack frame
        // or move A7.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        for _ in 0..5 {
            let result = dispatcher.dispatch_memory(false, 0x3F, &mut cpu, &mut bus);
            assert!(result.is_some(), "InitUtil should be handled");
            assert!(result.unwrap().is_ok(), "InitUtil should return cleanly");
            assert_eq!(
                cpu.read_reg(Register::D0) & 0xFF,
                0,
                "InitUtil should return noErr in D0 lowbyte on each call"
            );
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp_before,
                "InitUtil must preserve A7 across repeated register-only calls"
            );
        }
    }

    #[test]
    fn initzone_uses_a0_parameter_block_and_returns_noerr_in_d0() {
        // Inside Macintosh: Memory (1992), pp. 2-86 to 2-87:
        // InitZone takes a parameter-block pointer in A0 and returns
        // the result code in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let param_block = bus.alloc(14);
        bus.write_long(param_block, 0x0030_0000); // startPtr
        bus.write_long(param_block + 4, 0x0038_0000); // limitPtr
        bus.write_word(param_block + 8, 4); // cMoreMasters
        bus.write_long(param_block + 10, 0); // pGrowZone
        cpu.write_reg(Register::A0, param_block);
        cpu.write_reg(Register::D0, 0xFACE_B00C);

        let result = dispatcher.dispatch_memory(false, 0x19, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitZone should be handled");
        assert!(result.unwrap().is_ok(), "InitZone should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "InitZone nominal path should return noErr in D0"
        );
    }

    #[test]
    fn initzone_uses_register_calling_convention_without_stack_arguments() {
        // Inside Macintosh: Memory (1992), pp. 2-86 to 2-87:
        // InitZone uses A0 for its parameter block and does not document
        // a Pascal stack argument frame.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let param_block = bus.alloc(14);
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, param_block);

        let result = dispatcher.dispatch_memory(false, 0x19, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitZone should be handled");
        assert!(result.unwrap().is_ok(), "InitZone should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "InitZone should preserve A7 in its register calling convention"
        );
    }

    #[test]
    fn initzone_initializes_zone_header_and_makes_startptr_current_zone() {
        // Inside Macintosh Volume II (1985), p. II-29:
        // InitZone creates a heap zone at startPtr..limitPtr, initializes
        // its visible header fields, and makes startPtr the current zone.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let param_block = bus.alloc(14);
        let start = 0x0030_0000;
        let limit = 0x0030_0400;
        let more_masters: u16 = 4;
        let grow_zone = 0x0012_3456;
        bus.write_long(param_block, start);
        bus.write_long(param_block + 4, limit);
        bus.write_word(param_block + 8, more_masters);
        bus.write_long(param_block + 10, grow_zone);
        bus.write_long(0x0118, 0x00AA_BBCC);
        cpu.write_reg(Register::A0, param_block);

        let result = dispatcher.dispatch_memory(false, 0x19, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitZone should be handled");
        assert!(result.unwrap().is_ok(), "InitZone should return cleanly");
        assert_eq!(bus.read_long(start), limit, "bkLim should equal limitPtr");
        assert_eq!(
            bus.read_long(start + 12),
            limit - start - (72 + (4 * more_masters as u32)),
            "zcbFree should match the documented initial free-byte formula"
        );
        assert_eq!(
            bus.read_long(start + 16),
            grow_zone,
            "gzProc should reflect pGrowZone from the parameter block"
        );
        assert_eq!(
            bus.read_word(start + 20),
            more_masters,
            "moreMast should reflect cMoreMasters from the parameter block"
        );
        assert_eq!(
            bus.read_long(0x0118),
            start,
            "InitZone should make startPtr the current zone via TheZone"
        );
    }

    #[test]
    fn initapplzone_returns_noerr_result_code_in_d0() {
        // Inside Macintosh: Memory (1992), pp. 2-87 to 2-88:
        // InitApplZone returns a result code in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        let result = dispatcher.dispatch_memory(false, 0x2C, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitApplZone should be handled");
        assert!(
            result.unwrap().is_ok(),
            "InitApplZone should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "InitApplZone nominal path should return noErr in D0"
        );
    }

    #[test]
    fn initapplzone_takes_no_arguments_and_preserves_stack_pointer() {
        // Inside Macintosh Volume II (1985), p. II-28:
        // InitApplZone is a no-argument procedure with D0 result code.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let result = dispatcher.dispatch_memory(false, 0x2C, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitApplZone should be handled");
        assert!(
            result.unwrap().is_ok(),
            "InitApplZone should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "InitApplZone should preserve A7"
        );
    }

    #[test]
    fn initapplzone_reinitializes_application_zone_and_makes_it_current() {
        // Inside Macintosh Volume II (1985), p. II-28:
        // InitApplZone initializes the application heap zone, clears its
        // grow-zone function, and makes ApplZone the current zone.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let appl_zone = 0x0020_0000;
        let appl_limit = 0x0028_0000;
        bus.write_long(0x02AA, appl_zone); // ApplZone
        bus.write_long(0x0130, appl_limit); // ApplLimit
        bus.write_long(0x0118, 0x00AA_BBCC); // TheZone
        bus.write_long(appl_zone + 16, 0xDEAD_BEEF); // gzProc

        let result = dispatcher.dispatch_memory(false, 0x2C, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitApplZone should be handled");
        assert!(
            result.unwrap().is_ok(),
            "InitApplZone should return cleanly"
        );
        assert_eq!(
            bus.read_long(0x0118),
            appl_zone,
            "InitApplZone should make ApplZone the current zone"
        );
        assert_eq!(
            bus.read_long(appl_zone + 16),
            0,
            "InitApplZone should clear the application zone grow-zone pointer"
        );
        assert_eq!(
            bus.read_word(appl_zone + 20),
            64,
            "InitApplZone should restore the application zone moreMast increment"
        );
    }

    #[test]
    fn setapplbase_uses_a0_startptr_and_returns_noerr_in_d0() {
        // Inside Macintosh: Memory (1992), pp. 2-88 to 2-89:
        // SetApplBase takes startPtr in A0 and returns result code in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x0040_0000);
        cpu.write_reg(Register::D0, 0x1234_5678);

        let result = dispatcher.dispatch_memory(false, 0x57, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetApplBase should be handled");
        assert!(result.unwrap().is_ok(), "SetApplBase should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetApplBase nominal path should return noErr in D0"
        );
    }

    #[test]
    fn setapplbase_uses_register_calling_convention_without_stack_arguments() {
        // Inside Macintosh Volume II (1985), p. II-32 summary:
        // SetApplBase receives startPtr in A0 and should not pop stack args.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, 0x0040_0000);

        let result = dispatcher.dispatch_memory(false, 0x57, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetApplBase should be handled");
        assert!(result.unwrap().is_ok(), "SetApplBase should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SetApplBase should preserve A7"
        );
    }

    #[test]
    fn setapplbase_returns_noerr_regardless_of_startptr_value() {
        // Inside Macintosh Volume II (1985), pp. II-28 to II-29:
        // SetApplBase returns noErr on the nominal path. Systemless HLE
        // does not model a relocatable application zone, so the trap
        // is a no-op that writes D0 = noErr regardless of startPtr.
        // Dispatching the trap with startPtr echoing the current
        // ApplZone low-memory pointer, this verifies that any other
        // startPtr value (NIL, sentinel, or arbitrary address) also
        // produces D0 = 0 in the HLE.
        let cases: [u32; 5] = [
            0x0000_0000, // NIL — "use default" per IM:II II-28
            0x0040_0000, // arbitrary 4MB address (matches the existing test)
            0xDEAD_BEEF, // sentinel (untouched by HLE)
            0x0000_0001, // tiny pointer
            0x7FFF_FFFE, // near-max signed pointer
        ];
        for (i, startptr) in cases.iter().enumerate() {
            let (mut dispatcher, mut cpu, mut bus) = setup();
            let sp_before = cpu.read_reg(Register::A7);
            cpu.write_reg(Register::A0, *startptr);
            cpu.write_reg(Register::D0, 0xCAFE_BABE);

            let result = dispatcher.dispatch_memory(false, 0x57, &mut cpu, &mut bus);
            assert!(result.is_some(), "SetApplBase[{i}] should be handled");
            assert!(
                result.unwrap().is_ok(),
                "SetApplBase[{i}] should return cleanly"
            );
            assert_eq!(
                cpu.read_reg(Register::D0),
                0,
                "SetApplBase[{i}] (startPtr=0x{startptr:08X}) should return noErr in D0"
            );
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp_before,
                "SetApplBase[{i}] should preserve A7"
            );
        }
    }

    #[test]
    fn translate24to32_preserves_full_input_in_32bit_mode() {
        // BasiliskII System 7.5.3's default 32-bit-addressing path
        // returns the full input unchanged for tagged values.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0xAB12_3456);

        let result = dispatcher.dispatch_memory(false, 0x91, &mut cpu, &mut bus);
        assert!(result.is_some(), "Translate24To32 should be handled");
        assert!(
            result.unwrap().is_ok(),
            "Translate24To32 should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0xAB12_3456,
            "Translate24To32 should preserve the full tagged input in 32-bit mode"
        );
    }

    #[test]
    fn swapmmumode_updates_mmu32bit_low_memory_flag() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        bus.write_byte(crate::memory::globals::addr::MMU32_BIT, 1);

        cpu.write_reg(Register::D0, 0);
        let result = dispatcher.dispatch_memory(false, 0x5D, &mut cpu, &mut bus);
        assert!(result.is_some(), "SwapMMUMode should be handled");
        assert!(result.unwrap().is_ok(), "SwapMMUMode should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            1,
            "default mode is true32b, so D0 returns the previous mode"
        );
        assert_eq!(
            bus.read_byte(crate::memory::globals::addr::MMU32_BIT),
            0,
            "MMU32Bit should mirror the requested 24-bit mode"
        );

        cpu.write_reg(Register::D0, 1);
        let result = dispatcher.dispatch_memory(false, 0x5D, &mut cpu, &mut bus);
        assert!(result.is_some(), "SwapMMUMode should be handled again");
        assert!(result.unwrap().is_ok(), "SwapMMUMode should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "D0 returns the previous false32b mode"
        );
        assert_eq!(
            bus.read_byte(crate::memory::globals::addr::MMU32_BIT),
            1,
            "MMU32Bit should mirror the requested 32-bit mode"
        );
    }

    #[test]
    fn translate24to32_uses_d0_register_calling_convention_and_preserves_a7() {
        // Inside Macintosh: Memory (1992), p. 4-28:
        // Translate24To32 takes input in D0 and returns output in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0x0012_3456);

        let result = dispatcher.dispatch_memory(false, 0x91, &mut cpu, &mut bus);
        assert!(result.is_some(), "Translate24To32 should be handled");
        assert!(
            result.unwrap().is_ok(),
            "Translate24To32 should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0x0012_3456,
            "Translate24To32 should return the same value for a 32-bit-clean address"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "Translate24To32 should preserve A7 in register calling convention"
        );
    }

    #[test]
    fn vinstall_consumes_a0_taskptr_and_returns_oserr_in_d0() {
        // Inside Macintosh: Processes (1994), pp. 4-24 to 4-25:
        // VInstall takes task pointer in A0 and returns OSErr in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = bus.alloc(14);
        bus.write_long(task_ptr, 0xDEAD_BEEF);
        bus.write_word(task_ptr + 4, 1);
        bus.write_long(task_ptr + 6, 0x1234_5678);
        bus.write_word(task_ptr + 10, 2);
        bus.write_word(task_ptr + 12, 1);
        cpu.write_reg(Register::A0, task_ptr);

        let result = dispatcher.dispatch_memory(false, 0x33, &mut cpu, &mut bus);
        assert!(result.is_some(), "VInstall should be handled");
        assert!(result.unwrap().is_ok(), "VInstall should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "VInstall should return noErr"
        );
        assert_eq!(
            bus.read_long(task_ptr),
            0,
            "single-task queue link should terminate at NIL"
        );
        assert_eq!(
            bus.read_long(super::VBL_QUEUE_HEADER + 2),
            dispatcher.system_vbl_queue_anchor,
            "VInstall should keep the system-owned queue anchor at the head"
        );
        assert_eq!(
            bus.read_long(dispatcher.system_vbl_queue_anchor),
            task_ptr,
            "the system-owned queue anchor should link to the application task"
        );
        assert_eq!(
            bus.read_long(super::VBL_QUEUE_HEADER + 6),
            task_ptr,
            "VInstall should publish the system queue tail in low memory"
        );
    }

    #[test]
    fn vinstall_and_vremove_keep_low_memory_queue_header_and_links_coherent() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let first = bus.alloc(14);
        let second = bus.alloc(14);
        for task_ptr in [first, second] {
            bus.write_word(task_ptr + 4, 1);
            bus.write_long(task_ptr + 6, 0x1234_5678);
            bus.write_word(task_ptr + 10, 2);
            cpu.write_reg(Register::A0, task_ptr);
            dispatcher
                .dispatch_memory(false, 0x33, &mut cpu, &mut bus)
                .expect("VInstall should be handled")
                .expect("VInstall should return");
        }

        let anchor = dispatcher.system_vbl_queue_anchor;
        assert_ne!(anchor, 0);
        assert_eq!(bus.read_long(super::VBL_QUEUE_HEADER + 2), anchor);
        assert_eq!(bus.read_long(super::VBL_QUEUE_HEADER + 6), second);
        assert_eq!(bus.read_long(anchor), first);
        assert_eq!(bus.read_long(first), second);
        assert_eq!(bus.read_long(second), 0);

        cpu.write_reg(Register::A0, first);
        dispatcher
            .dispatch_memory(false, 0x34, &mut cpu, &mut bus)
            .expect("VRemove should be handled")
            .expect("VRemove should return");
        assert_eq!(bus.read_long(super::VBL_QUEUE_HEADER + 2), anchor);
        assert_eq!(bus.read_long(super::VBL_QUEUE_HEADER + 6), second);
        assert_eq!(bus.read_long(anchor), second);
        assert_eq!(bus.read_long(second), 0);
    }

    #[test]
    fn vinstall_invalid_qtype_returns_vtyperr() {
        // Inside Macintosh: Processes (1994), p. 4-25:
        // VInstall returns vTypErr (-2) for invalid qType.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = bus.alloc(14);
        bus.write_word(task_ptr + 4, 0);
        cpu.write_reg(Register::A0, task_ptr);

        let result = dispatcher.dispatch_memory(false, 0x33, &mut cpu, &mut bus);
        assert!(result.is_some(), "VInstall should be handled");
        assert!(result.unwrap().is_ok(), "VInstall should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -2,
            "VInstall should return vTypErr for invalid qType"
        );
    }

    #[test]
    fn vremove_consumes_a0_taskptr_and_returns_oserr_in_d0() {
        // Inside Macintosh: Processes (1994), pp. 4-25 to 4-26:
        // VRemove takes task pointer in A0 and returns OSErr in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = bus.alloc(14);
        bus.write_word(task_ptr + 4, 1);
        bus.write_long(task_ptr + 6, 0x1234_5678);
        bus.write_word(task_ptr + 10, 2);
        bus.write_word(task_ptr + 12, 1);

        cpu.write_reg(Register::A0, task_ptr);
        let install = dispatcher.dispatch_memory(false, 0x33, &mut cpu, &mut bus);
        assert!(install.is_some());
        assert!(install.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);

        cpu.write_reg(Register::A0, task_ptr);
        let remove = dispatcher.dispatch_memory(false, 0x34, &mut cpu, &mut bus);
        assert!(remove.is_some(), "VRemove should be handled");
        assert!(remove.unwrap().is_ok(), "VRemove should return cleanly");
        assert_eq!(cpu.read_reg(Register::D0), 0, "VRemove should return noErr");
        assert_eq!(
            bus.read_long(task_ptr),
            0,
            "removed task should have qLink reset to NIL"
        );
    }

    #[test]
    fn vremove_task_not_in_queue_returns_qerr() {
        // Inside Macintosh: Processes (1994), p. 4-26:
        // VRemove returns qErr (-1) when the task isn't in the queue.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = bus.alloc(14);
        bus.write_word(task_ptr + 4, 1);
        cpu.write_reg(Register::A0, task_ptr);

        let result = dispatcher.dispatch_memory(false, 0x34, &mut cpu, &mut bus);
        assert!(result.is_some(), "VRemove should be handled");
        assert!(result.unwrap().is_ok(), "VRemove should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -1,
            "VRemove should return qErr for non-queued task"
        );
    }

    #[test]
    fn vinstall_then_vremove_roundtrip_returns_noerr_on_both_and_qerr_on_second_remove() {
        // VInstall(valid) → noErr; immediate VRemove → noErr; second VRemove
        // of the same now-empty slot → qErr. Per IM:Processes 1994 p. 4-26
        // a second remove of an already-removed task hits the queue-not-found
        // path and returns qErr=-1.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = bus.alloc(14);
        bus.write_word(task_ptr + 4, 1);
        bus.write_long(task_ptr + 6, 0x0040_1000);
        bus.write_word(task_ptr + 10, 0x7FFF);
        bus.write_word(task_ptr + 12, 0);

        cpu.write_reg(Register::A0, task_ptr);
        let install = dispatcher.dispatch_memory(false, 0x33, &mut cpu, &mut bus);
        assert!(install.is_some());
        assert!(install.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "VInstall should return noErr"
        );

        cpu.write_reg(Register::A0, task_ptr);
        let remove1 = dispatcher.dispatch_memory(false, 0x34, &mut cpu, &mut bus);
        assert!(remove1.is_some());
        assert!(remove1.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "first VRemove should return noErr"
        );

        cpu.write_reg(Register::A0, task_ptr);
        let remove2 = dispatcher.dispatch_memory(false, 0x34, &mut cpu, &mut bus);
        assert!(remove2.is_some());
        assert!(remove2.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -1,
            "second VRemove should return qErr (task no longer in queue)"
        );
    }

    #[test]
    fn slotvinstall_consumes_a0_taskptr_d0_slot_and_returns_oserr_in_d0() {
        // Inside Macintosh: Processes (1994), pp. 4-22 to 4-23:
        // SlotVInstall takes task pointer in A0, slot in D0, and returns OSErr in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = bus.alloc(14);
        bus.write_word(task_ptr + 4, 1);
        bus.write_long(task_ptr + 6, 0x1234_5678);
        bus.write_word(task_ptr + 10, 2);
        bus.write_word(task_ptr + 12, 1);
        cpu.write_reg(Register::A0, task_ptr);
        cpu.write_reg(Register::D0, 3);

        let result = dispatcher.dispatch_memory(false, 0x6F, &mut cpu, &mut bus);
        assert!(result.is_some(), "SlotVInstall should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SlotVInstall should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SlotVInstall should return noErr"
        );
        assert_eq!(
            bus.read_long(task_ptr),
            0,
            "single-task queue link should terminate at NIL"
        );
    }

    #[test]
    fn slotvinstall_negative_one_slot_returns_noerr() {
        // Inside Macintosh: Processes (1994), p. 4-23:
        // BasiliskII accepts slot = -1 for SlotVInstall and returns noErr.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = bus.alloc(14);
        bus.write_word(task_ptr + 4, 1);
        cpu.write_reg(Register::A0, task_ptr);
        cpu.write_reg(Register::D0, (-1i16) as u16 as u32);

        let result = dispatcher.dispatch_memory(false, 0x6F, &mut cpu, &mut bus);
        assert!(result.is_some(), "SlotVInstall should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SlotVInstall should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SlotVInstall should return noErr for slot = -1"
        );
        assert_eq!(
            bus.read_long(task_ptr),
            0,
            "SlotVInstall should still install the task record"
        );
    }

    #[test]
    fn slotvremove_consumes_a0_taskptr_d0_slot_and_returns_oserr_in_d0() {
        // Inside Macintosh: Processes (1994), pp. 4-23 to 4-24:
        // SlotVRemove takes task pointer in A0, slot in D0, and returns OSErr in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = bus.alloc(14);
        bus.write_word(task_ptr + 4, 1);
        bus.write_long(task_ptr + 6, 0x1234_5678);
        bus.write_word(task_ptr + 10, 2);
        bus.write_word(task_ptr + 12, 1);

        cpu.write_reg(Register::A0, task_ptr);
        cpu.write_reg(Register::D0, 2);
        let install = dispatcher.dispatch_memory(false, 0x6F, &mut cpu, &mut bus);
        assert!(install.is_some());
        assert!(install.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);

        cpu.write_reg(Register::A0, task_ptr);
        cpu.write_reg(Register::D0, 2);
        let remove = dispatcher.dispatch_memory(false, 0x70, &mut cpu, &mut bus);
        assert!(remove.is_some(), "SlotVRemove should be handled");
        assert!(remove.unwrap().is_ok(), "SlotVRemove should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SlotVRemove should return noErr"
        );
    }

    #[test]
    fn slotvremove_task_not_in_queue_returns_qerr() {
        // Inside Macintosh: Processes (1994), p. 4-24:
        // SlotVRemove returns qErr (-1) when the task isn't in the queue.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = bus.alloc(14);
        bus.write_word(task_ptr + 4, 1);
        cpu.write_reg(Register::A0, task_ptr);
        cpu.write_reg(Register::D0, 4);

        let result = dispatcher.dispatch_memory(false, 0x70, &mut cpu, &mut bus);
        assert!(result.is_some(), "SlotVRemove should be handled");
        assert!(result.unwrap().is_ok(), "SlotVRemove should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -1,
            "SlotVRemove should return qErr for non-queued task"
        );
    }

    #[test]
    fn test_hpurge() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000);
        let result = dispatcher.dispatch_memory(false, 0x49, &mut cpu, &mut bus);
        assert!(result.is_some(), "HPurge ($A049) should be handled");
        assert!(result.unwrap().is_ok(), "HPurge should succeed");
    }

    #[test]
    fn movehhi_unlocked_handle_returns_noerr_and_preserves_master_pointer() {
        // Inside Macintosh Volume II (1985), p. II-44: MoveHHi moves the
        // relocatable block toward the top of the current heap zone and
        // reports success as noErr.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(32);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);
        cpu.write_reg(Register::A0, handle);

        let result = dispatcher.dispatch_memory(false, 0x64, &mut cpu, &mut bus);
        assert!(result.is_some(), "MoveHHi should be handled");
        assert!(result.unwrap().is_ok(), "MoveHHi should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0, "MoveHHi should return noErr");
        assert_eq!(
            bus.read_long(handle),
            data_ptr,
            "MoveHHi should keep the master pointer valid for the same handle"
        );
    }

    #[test]
    fn test_get_handle_size() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        // First allocate a handle of size 256
        cpu.write_reg(Register::D0, 256);
        let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let handle = cpu.read_reg(Register::A0);
        // Now get its size
        cpu.write_reg(Register::A0, handle);
        let result = dispatcher.dispatch_memory(false, 0x25, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetHandleSize should be handled");
        assert!(result.unwrap().is_ok(), "GetHandleSize should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            256,
            "GetHandleSize should return 256 in D0"
        );
    }

    #[test]
    fn get_handle_size_reports_loaded_resource_handle_logical_size() {
        // GetHandleSize reports a block's LOGICAL size, not the padded
        // physical one, and the Resource Manager allocates a loaded
        // resource's handle at the resource's exact byte count.
        // GetHandleSize ($A025)
        // FUNCTION GetHandleSize (h: Handle): Size;
        // Inside Macintosh Volume II, II-31; Memory 1992, 2-32.
        //
        // Rounding up to a longword hands callers that treat a resource as
        // an array one element too many. SimCity 2000 sizes its CREL
        // relocation table as GetHandleSize/2, and the phantom entry
        // relocated the CODE segment header, wedging its segment loader.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let bytes = vec![0x80; 50];
        let data_ptr = bus.alloc(bytes.len() as u32);
        bus.write_bytes(data_ptr, &bytes);
        dispatcher.remember_resource_backing_data(0, *b"snd ", 417, bytes);
        let handle =
            dispatcher.get_or_create_resource_handle_in_file(&mut bus, *b"snd ", 417, data_ptr, 0);

        cpu.write_reg(Register::A0, handle);
        let result = dispatcher.dispatch_memory(false, 0x25, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetHandleSize should be handled");
        assert!(result.unwrap().is_ok(), "GetHandleSize should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            50,
            "loaded resource handles should expose the resource's exact byte count"
        );
    }

    #[test]
    fn test_get_handle_size_updates_ccr_from_low_word_of_d0() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let ptr = bus.alloc(0x1_0000);
        let handle = bus.alloc(4);
        bus.write_long(handle, ptr);
        cpu.write_reg(Register::A0, handle);
        cpu.ccr = 0x10;

        let result = dispatcher.dispatch_memory(false, 0x25, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetHandleSize should be handled");
        assert!(result.unwrap().is_ok(), "GetHandleSize should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0x1_0000);
        assert_eq!(
            cpu.ccr, 0x14,
            "GetHandleSize should emulate trap-dispatcher TST.W D0 and preserve X"
        );
    }

    #[test]
    fn test_dispose_handle_sets_zero_flag_via_dispatcher_ccr() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let ptr = bus.alloc(8);
        let handle = bus.alloc(4);
        bus.write_long(handle, ptr);
        cpu.write_reg(Register::A0, handle);
        cpu.ccr = 0x18;

        let result = dispatcher.dispatch_memory(false, 0x23, &mut cpu, &mut bus);
        assert!(result.is_some(), "DisposeHandle should be handled");
        assert!(result.unwrap().is_ok(), "DisposeHandle should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            cpu.ccr, 0x14,
            "DisposeHandle should emulate trap-dispatcher TST.W D0 and preserve X"
        );
    }

    #[test]
    fn test_flush_code_cache() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let result = dispatcher.dispatch_memory(false, 0xBD, &mut cpu, &mut bus);
        assert!(result.is_some(), "FlushCodeCache should be handled");
        assert!(
            result.unwrap().is_ok(),
            "FlushCodeCache should succeed (no-op)"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "FlushCodeCache should return noErr in D0"
        );
    }

    #[test]
    fn hwpriv_swapinstructioncache_selector_0000_returns_previous_state_boolean() {
        // Inside Macintosh: Memory (1992), p. 4-29:
        // SwapInstructionCache uses _HWPriv selector $0000 and returns
        // the previous instruction-cache state as a Boolean, while
        // installing the requested new state.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xDEAD_BEEF);
        cpu.write_reg(Register::A0, 0); // request FALSE
        cpu.write_reg(Register::D0, 0); // selector $0000

        let result = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
        assert!(result.is_some(), "HWPriv should be handled");
        assert!(
            result.unwrap().is_ok(),
            "HWPriv selector $0000 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            1,
            "SwapInstructionCache should return previous state Boolean (TRUE) in A0"
        );
        assert!(
            !dispatcher.instruction_cache_enabled,
            "SwapInstructionCache(FALSE) should install the requested disabled state"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "_HWPriv selector calls use register ABI and should not pop stack arguments"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xDEAD_BEEF,
            "stack top should remain untouched for register-dispatched _HWPriv calls"
        );
    }

    #[test]
    fn hwpriv_swapinstructioncache_second_call_returns_false_after_prior_disable() {
        // The second SwapInstructionCache call should observe the state
        // installed by the first one.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0); // disable
        cpu.write_reg(Register::D0, 0);
        let first = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
        assert!(
            first.is_some(),
            "first SwapInstructionCache should be handled"
        );
        assert!(
            first.unwrap().is_ok(),
            "first SwapInstructionCache should return"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            1,
            "first SwapInstructionCache(FALSE) should report previous TRUE state in A0"
        );

        cpu.write_reg(Register::A0, 1); // re-enable
        cpu.write_reg(Register::D0, 0);
        let second = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
        assert!(
            second.is_some(),
            "second SwapInstructionCache should be handled"
        );
        assert!(
            second.unwrap().is_ok(),
            "second SwapInstructionCache should return"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "second SwapInstructionCache(TRUE) should report prior FALSE state in A0"
        );
        assert!(
            dispatcher.instruction_cache_enabled,
            "second SwapInstructionCache(TRUE) should restore the enabled state"
        );
    }

    #[test]
    fn hwpriv_swapdatacache_selector_0002_returns_previous_state_boolean() {
        // Inside Macintosh: Memory (1992), p. 4-30:
        // SwapDataCache uses _HWPriv selector $0002 and returns
        // the previous data-cache state as a Boolean, while installing
        // the requested new state.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xCAFE_F00D);
        cpu.write_reg(Register::A0, 0); // request FALSE
        cpu.write_reg(Register::D0, 2); // selector $0002

        let result = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
        assert!(result.is_some(), "HWPriv should be handled");
        assert!(
            result.unwrap().is_ok(),
            "HWPriv selector $0002 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            1,
            "SwapDataCache should return previous state Boolean (TRUE) in A0"
        );
        assert!(
            !dispatcher.data_cache_enabled,
            "SwapDataCache(FALSE) should install the requested disabled state"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "_HWPriv selector calls use register ABI and should not pop stack arguments"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xCAFE_F00D,
            "stack top should remain untouched for register-dispatched _HWPriv calls"
        );
    }

    #[test]
    fn hwpriv_swapdatacache_second_call_returns_false_after_prior_disable() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0); // disable
        cpu.write_reg(Register::D0, 2);
        let first = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
        assert!(first.is_some(), "first SwapDataCache should be handled");
        assert!(first.unwrap().is_ok(), "first SwapDataCache should return");
        assert_eq!(
            cpu.read_reg(Register::A0),
            1,
            "first SwapDataCache(FALSE) should report previous TRUE state in A0"
        );

        cpu.write_reg(Register::A0, 1); // re-enable
        cpu.write_reg(Register::D0, 2);
        let second = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
        assert!(second.is_some(), "second SwapDataCache should be handled");
        assert!(
            second.unwrap().is_ok(),
            "second SwapDataCache should return"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "second SwapDataCache(TRUE) should report prior FALSE state in A0"
        );
        assert!(
            dispatcher.data_cache_enabled,
            "second SwapDataCache(TRUE) should restore the enabled state"
        );
    }

    #[test]
    fn hwpriv_flushcodecacherange_selector_0009_uses_a0_a1_and_returns_result_code() {
        // Inside Macintosh: Memory (1992), pp. 4-32 to 4-33:
        // FlushCodeCacheRange uses _HWPriv selector $0009 with
        // A0=address, A1=count, and returns result code in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xFACE_C0DE);
        cpu.write_reg(Register::A0, 0x0012_3400);
        cpu.write_reg(Register::A1, 0x0000_0400);
        cpu.write_reg(Register::D0, 9); // selector $0009

        let result = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
        assert!(result.is_some(), "HWPriv should be handled");
        assert!(
            result.unwrap().is_ok(),
            "HWPriv selector $0009 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "FlushCodeCacheRange should return noErr in D0 on nominal HLE path"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0x0012_3400,
            "FlushCodeCacheRange should read address from A0 without clobbering it"
        );
        assert_eq!(
            cpu.read_reg(Register::A1),
            0x0000_0400,
            "FlushCodeCacheRange should read byte count from A1 without clobbering it"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "FlushCodeCacheRange should not consume a Pascal stack argument frame"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xFACE_C0DE,
            "stack top should remain untouched by register ABI call"
        );
    }

    #[test]
    fn flushcodecache_is_parameterless_and_preserves_stack_pointer() {
        // Inside Macintosh: Memory (1992), p. 4-31:
        // FlushCodeCache is PROCEDURE FlushCodeCache; with no arguments.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xBEEF_1357);

        let result = dispatcher.dispatch_memory(false, 0xBD, &mut cpu, &mut bus);
        assert!(result.is_some(), "FlushCodeCache should be handled");
        assert!(
            result.unwrap().is_ok(),
            "FlushCodeCache should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "FlushCodeCache should not pop a stack argument frame"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xBEEF_1357,
            "FlushCodeCache should leave the caller stack untouched"
        );
    }

    #[test]
    fn test_get_trap_address() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0x0044);
        let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetTrapAddress should be handled");
        assert!(result.unwrap().is_ok(), "GetTrapAddress should succeed");
        let addr = cpu.read_reg(Register::A0);
        assert_eq!(
            addr,
            0x00F00000 | 0x0044,
            "GetTrapAddress should return 0x00F00000 | trap_num in A0"
        );
    }

    #[test]
    fn test_get_trap_address_docking_dispatch_aliases_unimplemented_vector() {
        let (mut dispatcher, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, 0xAA57);
        let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "GetTrapAddress should handle DockingDispatch"
        );
        assert!(result.unwrap().is_ok(), "GetTrapAddress should succeed");
        let docking_addr = cpu.read_reg(Register::A0);

        cpu.write_reg(Register::D0, 0xA89F);
        let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "GetTrapAddress should handle Unimplemented"
        );
        assert!(result.unwrap().is_ok(), "GetTrapAddress should succeed");
        let unimplemented_addr = cpu.read_reg(Register::A0);

        assert_eq!(
            docking_addr, unimplemented_addr,
            "DockingDispatch should alias the _Unimplemented trampoline on the generic target"
        );
    }

    #[test]
    fn phantom_gettooltrapaddress_returns_native_table_entry_and_preserves_stack() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0x1357_BEEF);
        dispatcher.native_trap_table.insert(0xA89F, 0x00F0_2468);

        cpu.write_reg(Register::D0, 0xA89F);
        let result = dispatcher.dispatch_memory(true, 0x346, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "phantom GetToolTrapAddress should be handled"
        );
        assert!(
            result.unwrap().is_ok(),
            "phantom GetToolTrapAddress should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0x00F0_2468,
            "native trap-table entries should take precedence"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "phantom GetToolTrapAddress should preserve A7"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0x1357_BEEF,
            "phantom GetToolTrapAddress should leave caller stack memory untouched"
        );
    }

    #[test]
    fn test_get_tool_trap_address_docking_dispatch_aliases_unimplemented_probe() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0x0BAD_F00D);

        cpu.write_reg(Register::D0, 0x257);
        let result = dispatcher.dispatch_memory(true, 0x346, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "GetToolTrapAddress should handle DockingDispatch"
        );
        assert!(result.unwrap().is_ok(), "GetToolTrapAddress should succeed");
        let docking_addr = cpu.read_reg(Register::A0);

        cpu.write_reg(Register::D0, 0x09F);
        let result = dispatcher.dispatch_memory(true, 0x346, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "GetToolTrapAddress should handle Unimplemented"
        );
        assert!(result.unwrap().is_ok(), "GetToolTrapAddress should succeed");
        let unimplemented_addr = cpu.read_reg(Register::A0);

        assert_eq!(
            docking_addr, unimplemented_addr,
            "DockingDispatch should alias the _Unimplemented probe sentinel on the generic target"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "phantom GetToolTrapAddress should preserve A7"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0x0BAD_F00D,
            "phantom GetToolTrapAddress should leave caller stack memory untouched"
        );
    }

    #[test]
    fn phantom_gettooltrapaddress_creates_stable_trampoline_for_tool_trap_word() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xCAFE_BABE);

        cpu.write_reg(Register::D0, 0xA89F);
        let result = dispatcher.dispatch_memory(true, 0x346, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "phantom GetToolTrapAddress should handle tool-trap words"
        );
        assert!(
            result.unwrap().is_ok(),
            "phantom GetToolTrapAddress should succeed"
        );
        let first_addr = cpu.read_reg(Register::A0);
        assert_ne!(
            first_addr, 0,
            "tool-trap trampoline address should be nonzero"
        );
        bus.write_word(first_addr, 0);

        cpu.write_reg(Register::D0, 0xA89F);
        let result = dispatcher.dispatch_memory(true, 0x346, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "phantom GetToolTrapAddress should handle repeated tool-trap words"
        );
        assert!(
            result.unwrap().is_ok(),
            "phantom GetToolTrapAddress should succeed on repeated lookup"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            first_addr,
            "tool-trap trampoline lookup should be stable"
        );
        assert_eq!(
            bus.read_word(first_addr),
            0xAC9F,
            "repeated lookup should restore the ROM-like auto-pop trap word"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "phantom GetToolTrapAddress should preserve A7"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xCAFE_BABE,
            "phantom GetToolTrapAddress should leave caller stack memory untouched"
        );
    }

    #[test]
    fn test_block_move() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let src = 0x300000u32;
        let dst = 0x310000u32;
        let data: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];
        for (i, &b) in data.iter().enumerate() {
            bus.write_byte(src + i as u32, b);
        }
        cpu.write_reg(Register::A0, src);
        cpu.write_reg(Register::A1, dst);
        cpu.write_reg(Register::D0, data.len() as u32);
        let result = dispatcher.dispatch_memory(false, 0x2E, &mut cpu, &mut bus);
        assert!(result.is_some(), "BlockMove should be handled");
        assert!(result.unwrap().is_ok(), "BlockMove should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "BlockMove should set D0 to 0"
        );
        for (i, &b) in data.iter().enumerate() {
            assert_eq!(
                bus.read_byte(dst + i as u32),
                b,
                "BlockMove should copy byte {} correctly (expected 0x{:02X})",
                i,
                b
            );
        }
    }

    #[test]
    fn test_block_move_overlapping_ranges() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let base = 0x300000u32;
        let data: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
        for (i, &b) in data.iter().enumerate() {
            bus.write_byte(base + i as u32, b);
        }

        cpu.write_reg(Register::A0, base);
        cpu.write_reg(Register::A1, base + 2);
        cpu.write_reg(Register::D0, 6);

        let result = dispatcher.dispatch_memory(false, 0x2E, &mut cpu, &mut bus);
        assert!(result.is_some(), "BlockMove should be handled");
        assert!(result.unwrap().is_ok(), "BlockMove should succeed");

        let expected: [u8; 8] = [0, 1, 0, 1, 2, 3, 4, 5];
        for (i, &b) in expected.iter().enumerate() {
            assert_eq!(
                bus.read_byte(base + i as u32),
                b,
                "BlockMove should behave like memmove for overlapping ranges"
            );
        }
    }

    #[test]
    fn test_block_move_negative_count_is_noop() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let src = 0x300000u32;
        let dst = 0x310000u32;
        bus.write_bytes(src, &[0xDE, 0xAD, 0xBE, 0xEF]);
        bus.write_bytes(dst, &[0x11, 0x22, 0x33, 0x44]);

        cpu.write_reg(Register::A0, src);
        cpu.write_reg(Register::A1, dst);
        cpu.write_reg(Register::D0, 0xFFFF_FF81);

        let result = dispatcher.dispatch_memory(false, 0x2E, &mut cpu, &mut bus);
        assert!(result.is_some(), "BlockMove should be handled");
        assert!(result.unwrap().is_ok(), "BlockMove should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_bytes(dst, 4), vec![0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn test_set_trap_address() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        // Set a native trap handler
        cpu.write_reg(Register::D0, 0x01FF);
        cpu.write_reg(Register::A0, 0x00400000); // handler address
        let result = dispatcher.dispatch_memory(false, 0x47, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetTrapAddress should be handled");
        assert!(result.unwrap().is_ok(), "SetTrapAddress should succeed");
        // Now GetTrapAddress should return the installed handler
        cpu.write_reg(Register::D0, 0x01FF);
        let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A0),
            0x00400000,
            "GetTrapAddress should return the handler set by SetTrapAddress"
        );
    }

    #[test]
    fn nsettrapaddress_newtool_bare_trap_number_installs_canonical_tool_handler() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let handler_addr = 0x0040_0000;

        // `_SetTrapAddress newTool` accepts either an A-line instruction
        // or a bare trap number in D0. In either case the Toolbox table
        // key is the canonical A-line word later dispatched by the CPU.
        dispatcher.current_trap_word = 0xA647;
        cpu.write_reg(Register::D0, 0x01F4);
        cpu.write_reg(Register::A0, handler_addr);

        let result = dispatcher.dispatch_memory(false, 0x47, &mut cpu, &mut bus);
        assert!(result.is_some(), "NSetTrapAddress should be handled");
        assert!(result.unwrap().is_ok(), "NSetTrapAddress should succeed");

        assert_eq!(
            dispatcher.native_trap_table.get(&0xA9F4).copied(),
            Some(handler_addr),
            "bare Toolbox trap numbers must install under their canonical A-line word"
        );
        assert!(
            !dispatcher.native_trap_table.contains_key(&0x01F4),
            "the raw bare trap number is not a dispatchable trap-table key"
        );
    }

    #[test]
    fn test_strip_address() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let result = dispatcher.dispatch_memory(false, 0x55, &mut cpu, &mut bus);
        assert!(result.is_some(), "StripAddress should be handled");
        assert!(
            result.unwrap().is_ok(),
            "StripAddress should succeed (no-op)"
        );
    }

    #[test]
    fn test_purge_space() {
        // PurgeSpace returns the free_heap_estimate value (clamped
        // to [24MB, 64MB]) in BOTH A0 and D0. With dispatcher-
        // default HEAP_END/APPL_LIMIT (typically uninitialized
        // = 0), the saturating_sub yields 0 → clamps up to the
        // 24MB floor. Pin both registers receive the same value
        // so the "no fragmentation, total == contiguous" HLE
        // invariant holds.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let result = dispatcher.dispatch_memory(false, 0x62, &mut cpu, &mut bus);
        assert!(result.is_some(), "PurgeSpace should be handled");
        assert!(result.unwrap().is_ok(), "PurgeSpace should succeed");
        let a0 = cpu.read_reg(Register::A0);
        let d0 = cpu.read_reg(Register::D0);
        assert_eq!(a0, 24 * 1024 * 1024,
            "PurgeSpace must return free_heap_estimate floor (24MB) in A0 with default low-mem state");
        assert_eq!(d0, 24 * 1024 * 1024,
            "PurgeSpace must return free_heap_estimate floor (24MB) in D0 with default low-mem state");
        assert_eq!(
            a0, d0,
            "PurgeSpace must return same value in A0 (total) and D0 (contiguous) — \
             no fragmentation in Systemless's flat allocator"
        );
    }

    #[test]
    fn test_sys_environs_0x90() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let buf = 0x300000u32;
        cpu.write_reg(Register::A0, buf);
        cpu.write_reg(Register::D0, 2); // version
        let result = dispatcher.dispatch_memory(false, 0x90, &mut cpu, &mut bus);
        assert!(result.is_some(), "SysEnvirons (0x90) should be handled");
        assert!(result.unwrap().is_ok(), "SysEnvirons (0x90) should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SysEnvirons should set D0 to 0 (noErr)"
        );
        assert_eq!(bus.read_word(buf), 2, "environsVersion should be 2");
        assert_eq!(
            bus.read_word(buf + 2),
            crate::machine_profile::REFERENCE_MACHINE_PROFILE.gestalt_machine_type,
            "machineType should match the reference machine profile"
        );
        assert_eq!(
            bus.read_word(buf + 4),
            crate::machine_profile::REFERENCE_MACHINE_PROFILE.system_version_bcd,
            "systemVersion should match the reference machine profile"
        );
        assert_eq!(bus.read_word(buf + 6), 5, "processor should be 5 (68040)");
        assert_eq!(
            bus.read_byte(buf + 8),
            1,
            "hasFPU should be 1 (68040 has integrated FPU)"
        );
        assert_eq!(bus.read_byte(buf + 9), 1, "hasColorQD should be 1");
    }

    #[test]
    fn test_flush_code_cache_0xbd() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let result = dispatcher.dispatch_memory(false, 0xBD, &mut cpu, &mut bus);
        assert!(result.is_some(), "FlushCodeCache ($A0BD) should be handled");
        assert!(
            result.unwrap().is_ok(),
            "FlushCodeCache should succeed (no-op)"
        );
    }

    #[test]
    fn internalwait_routes_gettimeout_and_settimeout_selector_paths() {
        // Inside Macintosh Volume V (1986), p. V-356 and
        // Inside Macintosh: Operating System Utilities (1994), p. 7-13:
        // GetTimeout/SetTimeout are selector-driven wrappers over _InternalWait.
        for selector in [0u16, 1u16] {
            let (mut dispatcher, mut cpu, mut bus) = setup();
            let sp_before = cpu.read_reg(Register::A7);
            bus.write_word(sp_before, selector);
            bus.write_long(sp_before + 4, 0xACED_C0DE);
            cpu.write_reg(Register::A0, selector as u32);

            let result = dispatcher.dispatch_memory(false, 0x7F, &mut cpu, &mut bus);
            assert!(result.is_some(), "InternalWait should be handled");
            assert!(
                result.unwrap().is_ok(),
                "InternalWait should return cleanly"
            );
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp_before,
                "InternalWait no-op path should not pop selector bytes"
            );
            assert_eq!(
                bus.read_word(sp_before),
                selector,
                "InternalWait should leave caller-provided selector storage intact"
            );
        }
    }

    #[test]
    fn internalwait_stub_preserves_stack_pointer_in_noop_path() {
        // Inside Macintosh Volume V (1986), p. V-356:
        // Start Manager timeout entry points are wrappers over _InternalWait.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xC0DE_CAFE);
        cpu.write_reg(Register::A1, 0x00AB_CDEF);
        cpu.write_reg(Register::D1, 0x1357_9BDF);

        let result = dispatcher.dispatch_memory(false, 0x7F, &mut cpu, &mut bus);
        assert!(result.is_some(), "InternalWait should be handled");
        assert!(
            result.unwrap().is_ok(),
            "InternalWait should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "InternalWait no-op path should preserve stack pointer"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xC0DE_CAFE,
            "InternalWait should not mutate caller stack contents"
        );
        assert_eq!(
            cpu.read_reg(Register::A1),
            0x00AB_CDEF,
            "InternalWait no-op path should preserve A1"
        );
        assert_eq!(
            cpu.read_reg(Register::D1),
            0x1357_9BDF,
            "InternalWait no-op path should preserve D1"
        );
    }

    #[test]
    fn internalwait_five_call_composition_preserves_stack_across_alternating_selectors() {
        // Five successive _InternalWait dispatches with alternating A0
        // selectors ($0000/$0001) and varying D0 count inputs
        // (0/1/15/20/31) preserve A7 in aggregate, with no per-call drift.
        // Per IM:Operating_System_Utils 1994 pp. 9-27 to 9-28 the trap
        // consumes no Pascal stack arguments regardless of selector or D0
        // input.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xDEAD_BEEF);

        let selectors: [u32; 5] = [0x0000, 0x0001, 0x0000, 0x0001, 0x0001];
        let counts: [u32; 5] = [0, 1, 15, 20, 31];
        for (selector, count) in selectors.iter().zip(counts.iter()) {
            cpu.write_reg(Register::A0, *selector);
            cpu.write_reg(Register::D0, *count);
            let result = dispatcher.dispatch_memory(false, 0x7F, &mut cpu, &mut bus);
            assert!(result.is_some(), "InternalWait should be handled");
            assert!(
                result.unwrap().is_ok(),
                "InternalWait should return cleanly"
            );
        }

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "InternalWait five-call composition should preserve A7 in aggregate"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xDEAD_BEEF,
            "InternalWait should not mutate caller stack contents across composition"
        );
    }

    #[test]
    fn dtinstall_uses_a0_dttaskptr_register_calling_convention() {
        // Inside Macintosh Volume V (1986), p. V-467 and
        // Inside Macintosh: Processes (1994), pp. 6-12 to 6-13:
        // _DTInstall uses A0 for dtTaskPtr and D0 for result.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let dt_task_ptr = bus.alloc(24);
        bus.write_long(dt_task_ptr, 0);
        bus.write_word(dt_task_ptr + 4, super::DT_QTYPE);
        bus.write_word(dt_task_ptr + 6, 0);
        bus.write_long(dt_task_ptr + 8, 0x1234_5678);
        bus.write_long(dt_task_ptr + 12, 0);
        bus.write_long(dt_task_ptr + 16, 0);
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xFACE_B00C);
        cpu.write_reg(Register::A0, dt_task_ptr);
        cpu.write_reg(Register::D0, 0xFFFF_FFFE);

        let result = dispatcher.dispatch_memory(false, 0x82, &mut cpu, &mut bus);
        assert!(result.is_some(), "DTInstall should be handled");
        assert!(result.unwrap().is_ok(), "DTInstall should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A0),
            dt_task_ptr,
            "DTInstall should preserve A0 task pointer"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "DTInstall should not consume a Pascal stack frame"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xFACE_B00C,
            "DTInstall should leave caller stack contents untouched"
        );
    }

    #[test]
    fn dtinstall_valid_record_returns_noerr_in_d0() {
        // Inside Macintosh Volume V (1986), p. V-467:
        // DTInstall returns noErr for nominal installs.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let dt_task_ptr = bus.alloc(24);
        bus.write_long(dt_task_ptr, 0);
        bus.write_word(dt_task_ptr + 4, super::DT_QTYPE);
        bus.write_word(dt_task_ptr + 6, 0);
        bus.write_long(dt_task_ptr + 8, 0x1234_5678);
        bus.write_long(dt_task_ptr + 12, 0);
        bus.write_long(dt_task_ptr + 16, 0);
        cpu.write_reg(Register::A0, dt_task_ptr);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        let result = dispatcher.dispatch_memory(false, 0x82, &mut cpu, &mut bus);
        assert!(result.is_some(), "DTInstall should be handled");
        assert!(result.unwrap().is_ok(), "DTInstall should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DTInstall should return noErr in D0 for a valid record"
        );
    }

    #[test]
    fn dtinstall_invalid_qtype_returns_vtyperr_and_preserves_stack_pointer() {
        // Inside Macintosh Volume V (1986), p. V-467; and
        // Inside Macintosh: Processes (1994), pp. 6-12 to 6-13:
        // DTInstall returns vTypErr (-2) when qType is not ORD(dtQType).
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let dt_task_ptr = bus.alloc(24);
        bus.write_long(dt_task_ptr, 0);
        bus.write_word(dt_task_ptr + 4, 0);
        bus.write_word(dt_task_ptr + 6, 0);
        bus.write_long(dt_task_ptr + 8, 0x1234_5678);
        bus.write_long(dt_task_ptr + 12, 0);
        bus.write_long(dt_task_ptr + 16, 0);
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xABCD_0123);
        cpu.write_reg(Register::A0, dt_task_ptr);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        let result = dispatcher.dispatch_memory(false, 0x82, &mut cpu, &mut bus);
        assert!(result.is_some(), "DTInstall should be handled");
        assert!(result.unwrap().is_ok(), "DTInstall should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -2,
            "DTInstall should return vTypErr for invalid qType"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "DTInstall should preserve A7 on invalid qType"
        );
    }

    #[test]
    fn maxapplzone_returns_noerr_and_preserves_appllimit_in_hle() {
        // Inside Macintosh Volume II (1985), p. II-30: MaxApplZone expands
        // the app heap up to ApplLimit and reports noErr on success.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let appl_limit = 0x00A0_0000u32;
        bus.write_long(crate::memory::globals::addr::HEAP_END, appl_limit);
        bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);
        cpu.write_reg(Register::D0, 0xFFFF_FFEE);

        let result = dispatcher.dispatch_memory(false, 0x63, &mut cpu, &mut bus);
        assert!(result.is_some(), "MaxApplZone should be handled");
        assert!(
            result.unwrap().is_ok(),
            "MaxApplZone should succeed on nominal call"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "MaxApplZone should return noErr in D0 for assembly callers"
        );
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
            appl_limit,
            "MaxApplZone should not rewrite ApplLimit itself"
        );
    }

    #[test]
    fn maxapplzone_updates_heapend_to_applimit_when_room_is_available() {
        // Inside Macintosh Volume II, II-30 and Memory 1992, 2-27 / 2-74..2-75:
        // MaxApplZone expands the application heap up to ApplLimit.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let heap_end = 0x00A0_0000u32;
        let appl_limit = heap_end + 0x1000;
        bus.write_long(crate::memory::globals::addr::HEAP_END, heap_end);
        bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);

        let result = dispatcher.dispatch_memory(false, 0x63, &mut cpu, &mut bus);
        assert!(result.is_some(), "MaxApplZone should be handled");
        assert!(
            result.unwrap().is_ok(),
            "MaxApplZone should succeed on nominal call"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "MaxApplZone should return noErr in D0 for assembly callers"
        );
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::HEAP_END),
            appl_limit,
            "MaxApplZone should advance HeapEnd to the current ApplLimit"
        );
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
            appl_limit,
            "MaxApplZone should not rewrite ApplLimit itself"
        );
    }

    #[test]
    fn resrvmem_uses_d0_cbneeded_and_returns_noerr() {
        // Inside Macintosh Volume II (1985), p. II-39: ResrvMem takes
        // cbNeeded in D0 and returns an OSErr in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0x0001_0000);

        let result = dispatcher.dispatch_memory(false, 0x40, &mut cpu, &mut bus);
        assert!(result.is_some(), "ResrvMem should be handled");
        assert!(result.unwrap().is_ok(), "ResrvMem should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "ResrvMem should return noErr in D0 on nominal calls"
        );
    }

    #[test]
    fn moremasters_returns_noerr_and_followup_newhandle_succeeds() {
        // Inside Macintosh Volume II (1985), p. II-31: MoreMasters allocates
        // another master-pointer block and reports noErr on success.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0xFACE_F00D);

        let more_masters = dispatcher.dispatch_memory(false, 0x36, &mut cpu, &mut bus);
        assert!(more_masters.is_some(), "MoreMasters should be handled");
        assert!(
            more_masters.unwrap().is_ok(),
            "MoreMasters should succeed on nominal call"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "MoreMasters should return noErr in D0 for assembly callers"
        );

        cpu.write_reg(Register::D0, 64);
        let new_handle = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(
            new_handle.unwrap().is_ok(),
            "NewHandle should still succeed"
        );
        assert_ne!(
            cpu.read_reg(Register::A0),
            0,
            "MoreMasters no-op path should not block subsequent handle allocation"
        );
    }

    #[test]
    fn sleepqinstall_variant_consumes_sleepqrecptr_argument() {
        // SleepQInstall is PROCEDURE SleepQInstall(qRecPtr: SleepQRecPtr),
        // so it consumes one 4-byte pointer argument from stack.
        // Inside Macintosh: Devices (1994), p. 6-33.
        // Trap word mapping: SleepQInstall = $A28A (IM:VI 1991 Appendix C, p. C-3).
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let q_rec_ptr = bus.alloc(12);
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, q_rec_ptr);
        dispatcher.current_trap_word = 0xA28A;

        let result = dispatcher.dispatch_memory(false, 0x8A, &mut cpu, &mut bus);
        assert!(result.is_some(), "SleepQInstall should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SleepQInstall should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before.wrapping_add(4),
            "SleepQInstall should pop one SleepQRecPtr argument (4 bytes)"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SleepQInstall no-op path should report noErr in D0"
        );
    }

    #[test]
    fn sleepqremove_variant_consumes_sleepqrecptr_argument() {
        // SleepQRemove is PROCEDURE SleepQRemove(qRecPtr: SleepQRecPtr),
        // so it consumes one 4-byte pointer argument from stack.
        // Inside Macintosh: Devices (1994), p. 6-33.
        // Trap word mapping: SleepQRemove = $A48A (IM:VI 1991 Appendix C, p. C-3).
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let q_rec_ptr = bus.alloc(12);
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, q_rec_ptr);
        dispatcher.current_trap_word = 0xA48A;

        let result = dispatcher.dispatch_memory(false, 0x8A, &mut cpu, &mut bus);
        assert!(result.is_some(), "SleepQRemove should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SleepQRemove should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before.wrapping_add(4),
            "SleepQRemove should pop one SleepQRecPtr argument (4 bytes)"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SleepQRemove no-op path should report noErr in D0"
        );
    }

    #[test]
    fn sleepq_family_trap_leaves_stack_untouched() {
        // The family trap word ($A08A) is the shell that dispatches to the
        // SleepQInstall/$A28A and SleepQRemove/$A48A variants.
        // The shell itself should not consume a Pascal argument frame.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0x1357_9BDF);
        dispatcher.current_trap_word = 0xA08A;

        let result = dispatcher.dispatch_memory(false, 0x8A, &mut cpu, &mut bus);
        assert!(result.is_some(), "SleepQ family trap should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SleepQ family trap should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SleepQ family shell should not pop a Pascal frame"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0x1357_9BDF,
            "SleepQ family shell should leave the stack top untouched"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SleepQ family shell should report noErr in D0"
        );
    }

    #[test]
    fn commtoolboxdispatch_countditl_returns_dialog_item_count_and_preserves_stack_pointer() {
        // Inside Macintosh Volume VI (1991), Appendix C table C-3 (p. C-4):
        // _CommToolboxDispatch ($A08B) dispatches CountDITL via selector
        // $0403. The MPW C frame places
        // the 4-byte result slot at SP+0..3, the selector word at SP+4..5,
        // and the DialogPtr argument at SP+6..9.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08B;
        let dialog_ptr = bus.alloc(170);
        let items = vec![
            crate::trap::dispatch::DialogItem {
                item_type: 4,
                rect: (10, 10, 20, 20),
                text: "One".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            crate::trap::dispatch::DialogItem {
                item_type: 8,
                rect: (20, 10, 30, 20),
                text: "Two".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            crate::trap::dispatch::DialogItem {
                item_type: 16,
                rect: (30, 10, 40, 20),
                text: "Three".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ];
        dispatcher.dialog_items.insert(dialog_ptr, items);
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xCAFE_BABE);
        bus.write_word(sp_before + 4, 0x0403);
        bus.write_long(sp_before + 6, dialog_ptr);
        bus.write_word(sp_before + 10, 0x9BDF);

        let result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
        assert!(result.is_some(), "CommToolboxDispatch should be handled");
        assert!(
            result.unwrap().is_ok(),
            "CommToolboxDispatch should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "CountDITL should leave the caller stack pointer unchanged"
        );
        assert_eq!(
            bus.read_word(sp_before + 4),
            0x0403,
            "CountDITL should not overwrite the selector word on the caller stack"
        );
        assert_eq!(
            bus.read_long(sp_before + 6),
            dialog_ptr,
            "CountDITL should not overwrite the dialog pointer slot on the caller stack"
        );
        assert_eq!(
            bus.read_word(sp_before + 10),
            0x9BDF,
            "CountDITL should not overwrite the following stack word"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            3,
            "CountDITL should report the three dialog items we installed"
        );
    }

    #[test]
    fn commtoolboxdispatch_appendditl_overlay_appends_items_and_preserves_stack_pointer() {
        // Inside Macintosh Volume VI (1991), Appendix C table C-3 (p. C-4):
        // selector $0402 dispatches AppendDITL through _CommToolboxDispatch.
        // Marathon 2's preferences glue uses selector at SP+4, overlay method
        // at SP+6, DITL handle at SP+8, and DialogPtr at SP+12.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08B;

        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let old_ditl_ptr = bus.alloc(18);
        let preserved_handle = 0x00AB_CDEF;
        bus.write_long(items_handle, old_ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(old_ditl_ptr, 0);
        bus.write_long(old_ditl_ptr + 2, preserved_handle);
        bus.write_word(old_ditl_ptr + 6, 10);
        bus.write_word(old_ditl_ptr + 8, 20);
        bus.write_word(old_ditl_ptr + 10, 30);
        bus.write_word(old_ditl_ptr + 12, 60);
        bus.write_byte(old_ditl_ptr + 14, 4);
        bus.write_byte(old_ditl_ptr + 15, 2);
        bus.write_bytes(old_ditl_ptr + 16, b"OK");
        dispatcher.dialog_items.insert(
            dialog_ptr,
            vec![crate::trap::dispatch::DialogItem {
                item_type: 4,
                rect: (10, 20, 30, 60),
                text: "OK".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        let append_handle = bus.alloc(4);
        let append_ditl_ptr = bus.alloc(20);
        bus.write_long(append_handle, append_ditl_ptr);
        bus.write_word(append_ditl_ptr, 0);
        bus.write_long(append_ditl_ptr + 2, 0);
        bus.write_word(append_ditl_ptr + 6, 40);
        bus.write_word(append_ditl_ptr + 8, 50);
        bus.write_word(append_ditl_ptr + 10, 55);
        bus.write_word(append_ditl_ptr + 12, 110);
        bus.write_byte(append_ditl_ptr + 14, 8);
        bus.write_byte(append_ditl_ptr + 15, 4);
        bus.write_bytes(append_ditl_ptr + 16, b"Pane");

        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0x1357_2468);
        bus.write_word(sp_before + 4, 0x0402);
        bus.write_word(sp_before + 6, 0); // overlayDITL
        bus.write_long(sp_before + 8, append_handle);
        bus.write_long(sp_before + 12, dialog_ptr);
        bus.write_word(sp_before + 16, 0xBEEF);

        let result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
        assert!(result.is_some(), "AppendDITL should be handled");
        assert!(result.unwrap().is_ok(), "AppendDITL should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "AppendDITL must preserve the caller stack pointer"
        );
        assert_eq!(bus.read_word(sp_before + 4), 0x0402);
        assert_eq!(bus.read_word(sp_before + 16), 0xBEEF);
        assert_eq!(
            cpu.read_reg(Register::D0),
            2,
            "AppendDITL should mirror the new dialog item count in D0"
        );

        let new_ditl_ptr = bus.read_long(items_handle);
        assert_ne!(new_ditl_ptr, old_ditl_ptr);
        assert_eq!(bus.read_word(new_ditl_ptr), 1);
        assert_eq!(
            bus.read_long(new_ditl_ptr + 2),
            preserved_handle,
            "AppendDITL must preserve existing item handles in the copied DITL"
        );
        let appended_handle = bus.read_long(new_ditl_ptr + 18);
        assert_ne!(
            appended_handle, 0,
            "AppendDITL should initialize text storage for appended statText items"
        );
        assert_eq!(
            dispatcher.dialog_items.get(&dialog_ptr).unwrap()[1].text,
            "Pane"
        );
    }

    #[test]
    fn commtoolboxdispatch_countditl_preserves_non_d0_registers() {
        // Inside Macintosh Volume VI (1991), Appendix C table C-3 (p. C-4):
        // CountDITL is one selector behind _CommToolboxDispatch. The MPW
        // C frame places the 4-byte result slot at SP+0..3, selector at
        // SP+4..5, and the DialogPtr argument at SP+6..9.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08B;
        let dialog_ptr = bus.alloc(170);
        dispatcher.dialog_items.insert(
            dialog_ptr,
            vec![crate::trap::dispatch::DialogItem {
                item_type: 4,
                rect: (10, 10, 20, 20),
                text: "One".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xDEAD_BEEF);
        bus.write_word(sp_before + 4, 0x0403);
        bus.write_long(sp_before + 6, dialog_ptr);
        cpu.write_reg(Register::A0, 0x00AB_C000);
        cpu.write_reg(Register::A1, 0x00AB_C100);
        cpu.write_reg(Register::D1, 0x5AA5_0F0F);

        let result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
        assert!(result.is_some(), "CommToolboxDispatch should be handled");
        assert!(
            result.unwrap().is_ok(),
            "CommToolboxDispatch should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0x00AB_C000,
            "CommToolboxDispatch stub should preserve A0"
        );
        assert_eq!(
            cpu.read_reg(Register::A1),
            0x00AB_C100,
            "CountDITL should preserve A1"
        );
        assert_eq!(
            cpu.read_reg(Register::D1),
            0x5AA5_0F0F,
            "CountDITL should preserve D1"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "CountDITL should preserve the caller stack pointer"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            1,
            "CountDITL should mirror the result into D0 for CCR update"
        );
    }

    #[test]
    fn commtoolboxdispatch_selector_0403_returns_expected_counts_for_one_and_three_item_dialogs() {
        // Inside Macintosh Volume VI (1991), Appendix C table C-3 (p. C-4):
        // _CommToolboxDispatch ($A08B) dispatches CountDITL via selector
        // $0403. The MPW frame places the
        // 4-byte result slot at SP+0..3, selector at SP+4..5, and the
        // DialogPtr argument at SP+6..9. This test checks both a one-item
        // dialog and the existing three-item dialog.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08B;

        let one_dialog_ptr = bus.alloc(170);
        dispatcher.dialog_items.insert(
            one_dialog_ptr,
            vec![crate::trap::dispatch::DialogItem {
                item_type: 4,
                rect: (10, 10, 20, 20),
                text: "OK".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        let three_dialog_ptr = bus.alloc(170);
        dispatcher.dialog_items.insert(
            three_dialog_ptr,
            vec![
                crate::trap::dispatch::DialogItem {
                    item_type: 4,
                    rect: (10, 10, 20, 20),
                    text: "One".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                crate::trap::dispatch::DialogItem {
                    item_type: 8,
                    rect: (20, 10, 30, 20),
                    text: "Two".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                crate::trap::dispatch::DialogItem {
                    item_type: 16,
                    rect: (30, 10, 40, 20),
                    text: "Three".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xDEAD_BEEF);
        bus.write_word(sp_before + 4, 0x0403);
        bus.write_long(sp_before + 6, one_dialog_ptr);
        let one_result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
        assert!(
            one_result.is_some(),
            "CommToolboxDispatch should be handled for a one-item dialog"
        );
        assert!(
            one_result.unwrap().is_ok(),
            "CommToolboxDispatch should return cleanly for a one-item dialog"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            1,
            "CommToolboxDispatch should report one item for the one-item dialog"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "CommToolboxDispatch should preserve the caller stack pointer for the one-item dialog"
        );

        cpu.write_reg(Register::A7, sp_before);
        bus.write_long(sp_before, 0xDEAD_BEEF);
        bus.write_word(sp_before + 4, 0x0403);
        bus.write_long(sp_before + 6, three_dialog_ptr);
        let three_result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
        assert!(
            three_result.is_some(),
            "CommToolboxDispatch should be handled for a three-item dialog"
        );
        assert!(
            three_result.unwrap().is_ok(),
            "CommToolboxDispatch should return cleanly for a three-item dialog"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            3,
            "CommToolboxDispatch should report three items for the three-item dialog"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "CommToolboxDispatch should preserve the caller stack pointer for the three-item dialog"
        );
    }

    #[test]
    fn debugutil_debuggergetmax_returns_max_selector_and_preserves_stack_pointer() {
        // Inside Macintosh Volume VI (1991), p. 28-30 and Appendix C table C-3
        // (p. C-4): _DebugUtil is selector-dispatched in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08D;
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0x2468_ACF0);
        cpu.write_reg(Register::D0, 0x0000); // DebuggerGetMax selector

        let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
        assert!(result.is_some(), "DebugUtil should be handled");
        assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "DebugUtil should not consume a stack argument frame"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0x2468_ACF0,
            "stack top should remain untouched for DebugUtil selector dispatch"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            8,
            "DebugUtil selector $0000 should return the highest documented selector"
        );
    }

    #[test]
    fn debugutil_other_selectors_preserve_non_d0_registers() {
        // Inside Macintosh Volume VI (1991), pp. 28-30..28-31 and Appendix C
        // table C-3 (p. C-4): debugger helper routines share _DebugUtil.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08D;
        cpu.write_reg(Register::A0, 0x0012_3000);
        cpu.write_reg(Register::A1, 0x0012_3F00);
        cpu.write_reg(Register::D1, 0x1234_5678);
        cpu.write_reg(Register::D0, 0x0006); // DebuggerLockMemory selector

        let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
        assert!(result.is_some(), "DebugUtil should be handled");
        assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A0),
            0x0012_3000,
            "DebugUtil selector $0006 should preserve A0"
        );
        assert_eq!(
            cpu.read_reg(Register::A1),
            0x0012_3F00,
            "DebugUtil selector $0006 should preserve A1"
        );
        assert_eq!(
            cpu.read_reg(Register::D1),
            0x1234_5678,
            "DebugUtil selector $0006 should preserve D1"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DebugUtil selector $0006 should rewrite only D0 to noErr"
        );
    }

    #[test]
    fn debugutil_debuggerpoll_preserves_stack_sentinel_and_returns_noerr() {
        // Selector $0003 (DebuggerPoll) is part of the documented
        // DebugUtil table in Inside Macintosh Volume VI 1991,
        // Appendix C table C-3 (p. C-4).
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08D;
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0x1357_9BDF);
        cpu.write_reg(Register::D0, 0x0003);
        cpu.write_reg(Register::A0, 0x00FE_DCBA);
        cpu.write_reg(Register::A1, 0x00AB_CDEF);

        let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
        assert!(result.is_some(), "DebugUtil should be handled");
        assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "DebugUtil selector $0003 should not consume a stack frame"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0x1357_9BDF,
            "DebugUtil selector $0003 should leave the stack sentinel untouched"
        );
        assert_eq!(cpu.read_reg(Register::A0), 0x00FE_DCBA);
        assert_eq!(cpu.read_reg(Register::A1), 0x00AB_CDEF);
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DebugUtil selector $0003 should return noErr"
        );
    }

    #[test]
    fn debugutil_debuggerpoll_updates_dispatcher_ccr_for_noerr() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08D;
        cpu.set_ccr(0x10);
        cpu.write_reg(Register::D0, 0x0003);

        let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
        assert!(result.is_some(), "DebugUtil should be handled");
        assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DebugUtil selector $0003 should return noErr"
        );
        assert_eq!(
            cpu.get_ccr(),
            0x14,
            "DebugUtil selector $0003 should mirror noErr into CCR without disturbing X"
        );
    }

    #[test]
    fn debugutil_debuggergetmax_preserves_non_d0_registers() {
        // Selector $0000 should use the same register-only ABI and
        // leave the scratch registers alone while returning 8 in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08D;
        cpu.write_reg(Register::A0, 0x00AA_5500);
        cpu.write_reg(Register::A1, 0x00BB_6600);
        cpu.write_reg(Register::D1, 0x1234_5678);
        cpu.write_reg(Register::D0, 0x0000);

        let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
        assert!(result.is_some(), "DebugUtil should be handled");
        assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
        assert_eq!(cpu.read_reg(Register::A0), 0x00AA_5500);
        assert_eq!(cpu.read_reg(Register::A1), 0x00BB_6600);
        assert_eq!(cpu.read_reg(Register::D1), 0x1234_5678);
        assert_eq!(cpu.read_reg(Register::D0), 8);
    }

    #[test]
    fn debugutil_five_call_composition_preserves_stack_across_documented_selectors() {
        // 5 successive
        // _DebugUtil dispatches with varying D0 selector inputs spanning
        // the documented IM:VI 1991 Appendix C table C-3 (p. C-4) range
        // ($0000 DebuggerGetMax / $0001 DebuggerEnter / $0003 DebuggerPoll
        // / $0005 PageFaultFatal / $0008 EnterSupervisorMode) preserve A7
        // in aggregate AND leave the SP+0 sentinel untouched.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08D;
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xDEAD_BEEF);

        let selectors: [u32; 5] = [0x0000, 0x0001, 0x0003, 0x0005, 0x0008];
        for selector in selectors {
            cpu.write_reg(Register::D0, selector);
            let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
            assert!(result.is_some(), "DebugUtil should be handled");
            assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "5-call DebugUtil composition should preserve A7 in aggregate"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xDEAD_BEEF,
            "SP+0 sentinel should survive 5-call DebugUtil composition"
        );
    }

    #[test]
    fn debugutil_debuggerpoll_five_call_composition_preserves_stack_across_repeated_selector_three_calls(
    ) {
        // Five successive
        // DebuggerPoll calls preserve A7 in aggregate and continue to
        // return noErr across the repeated selector-3 path.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08D;
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xCAFE_BABE);

        for _ in 0..5 {
            cpu.write_reg(Register::D0, 0x0003);
            let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
            assert!(result.is_some(), "DebugUtil should be handled");
            assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
            assert_eq!(
                cpu.read_reg(Register::D0),
                0,
                "DebugUtil selector $0003 should return noErr"
            );
        }

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "five DebuggerPoll calls should preserve A7 in aggregate"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xCAFE_BABE,
            "five DebuggerPoll calls should leave the stack sentinel untouched"
        );
    }

    #[test]
    fn deferuserfn_uses_register_calling_convention_without_stack_arguments() {
        // Inside Macintosh Volume VI (1991), p. 28-30; Inside Macintosh:
        // Memory (1992), p. 3-33: DeferUserFn uses D0(argument)/A0(function)
        // registers and returns result in D0. This test exercises the safe
        // fallback path for a non-callable placeholder pointer.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08F;
        let sp_before = cpu.read_reg(Register::A7);
        let pc_before = 0x00BA_D000;
        bus.write_long(sp_before, 0xBADC_0DE0);
        cpu.write_reg(Register::A0, 0x00C0_FFEE);
        cpu.write_reg(Register::D0, 0x0012_3400);
        cpu.write_reg(Register::PC, pc_before);

        let result = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
        assert!(result.is_some(), "DeferUserFn should be handled");
        assert!(result.unwrap().is_ok(), "DeferUserFn should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "DeferUserFn should not pop a Pascal stack argument frame"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xBADC_0DE0,
            "stack top should remain untouched by DeferUserFn register ABI"
        );
        assert_eq!(
            cpu.read_reg(Register::PC),
            pc_before,
            "non-callable DeferUserFn fallback should not install a trampoline"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DeferUserFn stub should return noErr in D0"
        );
    }

    #[test]
    fn deferuserfn_nominal_stub_returns_noerr() {
        // Inside Macintosh Volume VI (1991), p. 28-30: DeferUserFn returns an
        // OSErr in D0. This is the safe non-callable-pointer fallback.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08F;
        cpu.write_reg(Register::A0, 0x00D0_0000);
        cpu.write_reg(Register::D0, 0x00E0_0000);

        let result = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
        assert!(result.is_some(), "DeferUserFn should be handled");
        assert!(result.unwrap().is_ok(), "DeferUserFn should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DeferUserFn stub should report noErr for nominal calls"
        );
    }

    #[test]
    fn deferuserfn_callable_pointer_installs_trampoline_and_returns_noerr() {
        // Valid callable-proc path: the trap should inject a trampoline,
        // pass the argument in A0, and return noErr in D0 before the
        // trampoline executes.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08F;

        let pc_after_trap = 0x00BA_D100;
        let sp_before = cpu.read_reg(Register::A7);
        let user_fn = bus.alloc(8);
        bus.write_word(user_fn, 0x4E56); // LINK A6,#0 — looks like a real proc
        bus.write_word(user_fn + 2, 0x0000);
        bus.write_word(user_fn + 4, 0x4E75); // RTS

        cpu.write_reg(Register::PC, pc_after_trap);
        cpu.write_reg(Register::A0, user_fn);
        cpu.write_reg(Register::D0, 0x00DE_ADBE);

        let result = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
        assert!(result.is_some(), "DeferUserFn should be handled");
        assert!(result.unwrap().is_ok(), "DeferUserFn should return cleanly");

        let tramp = dispatcher.defer_user_fn_trampoline;
        assert_ne!(tramp, 0, "DeferUserFn should allocate a trampoline");
        assert_eq!(
            cpu.read_reg(Register::PC),
            tramp,
            "trampoline PC should be installed"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before - 4,
            "DeferUserFn should push a return address for the trampoline"
        );
        assert_eq!(
            bus.read_long(sp_before - 4),
            pc_after_trap,
            "trampoline should resume at the post-trap PC"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DeferUserFn should return noErr in D0"
        );
        assert_eq!(
            bus.read_word(tramp),
            0x48E7,
            "trampoline should save scratch registers"
        );
        assert_eq!(
            bus.read_word(tramp + 2),
            0xF0F0,
            "trampoline should save D0-D3/A0-A3"
        );
        assert_eq!(
            bus.read_word(tramp + 4),
            0x207C,
            "trampoline should load the argument into A0"
        );
        assert_eq!(
            bus.read_long(tramp + 6),
            0x00DE_ADBE,
            "trampoline should patch the argument literal"
        );
        assert_eq!(
            bus.read_word(tramp + 10),
            0x4EB9,
            "trampoline should JSR to the user function"
        );
        assert_eq!(
            bus.read_long(tramp + 12),
            user_fn,
            "trampoline should patch the callback address"
        );
        assert_eq!(
            bus.read_word(tramp + 16),
            0x4CDF,
            "trampoline should restore scratch registers"
        );
        assert_eq!(
            bus.read_word(tramp + 18),
            0x0F0F,
            "trampoline should restore D0-D3/A0-A3"
        );
        assert_eq!(
            bus.read_word(tramp + 20),
            0x7000,
            "trampoline should clear D0 to noErr"
        );
        assert_eq!(
            bus.read_word(tramp + 22),
            0x4E75,
            "trampoline should RTS back to the caller"
        );
    }

    #[test]
    fn deferuserfn_five_call_composition_preserves_stack_across_varying_args() {
        // 5 successive _DeferUserFn
        // dispatches with varying (A0=userFunction, D0=argument) inputs
        // span the IM:Memory 1992 p. 3-33 register convention. Per-call
        // pop-discipline errors accumulate; the 5-call composition
        // catches drift even when a single call's discipline is correct.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08F;
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xBADC_0DE0);

        // Five (userFunction, argument) tuples spanning small/large
        // pointer values and zero-vs-nonzero arguments.
        let inputs = [
            (0x0040_0000u32, 0x0000_0000u32),
            (0x00C0_0000u32, 0x0000_0001u32),
            (0x0050_8000u32, 0xDEAD_BEEFu32),
            (0x00B0_C000u32, 0x0000_FFFFu32),
            (0x0080_0000u32, 0xCAFE_BABEu32),
        ];
        for (a0, d0) in inputs.iter().copied() {
            cpu.write_reg(Register::A0, a0);
            cpu.write_reg(Register::D0, d0);
            let r = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
            assert!(r.is_some(), "DeferUserFn should be handled");
            assert!(r.unwrap().is_ok(), "DeferUserFn should return cleanly");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "5-call DeferUserFn composition should preserve A7 in aggregate"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xBADC_0DE0,
            "SP+0 sentinel should survive 5-call DeferUserFn composition"
        );
    }

    #[test]
    fn deferuserfn_three_call_composition_preserves_stack_across_varying_args() {
        // Three successive DeferUserFn dispatches with varying A0/D0
        // inputs should still preserve the Pascal stack discipline.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = 0xA08F;
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xBADC_0DE0);

        let user_fn = bus.alloc(8);
        bus.write_word(user_fn, 0x4E56); // LINK A6,#0 — looks like a real proc
        bus.write_word(user_fn + 2, 0x0000);
        bus.write_word(user_fn + 4, 0x4E75); // RTS

        let inputs = [
            (0x0040_0000u32, 0x0000_0000u32),
            (0x00C0_0000u32, 0x0000_0001u32),
            (0x0050_8000u32, 0xDEAD_BEEFu32),
        ];
        for (a0, d0) in inputs.iter().copied() {
            cpu.write_reg(Register::A0, a0);
            cpu.write_reg(Register::D0, d0);
            let r = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
            assert!(r.is_some(), "DeferUserFn should be handled");
            assert!(r.unwrap().is_ok(), "DeferUserFn should return cleanly");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "3-call DeferUserFn composition should preserve A7 in aggregate"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xBADC_0DE0,
            "SP+0 sentinel should survive 3-call DeferUserFn composition"
        );
    }

    #[test]
    fn nminstall_returns_noerr_for_nominal_notification_request() {
        // Inside Macintosh Volume VI (1991), p. 24-10:
        // NMInstall returns noErr for valid notification requests.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let nm_rec = bus.alloc(32);
        bus.write_word(nm_rec + 4, 8); // qType = ORD(nmType)
        cpu.write_reg(Register::A0, nm_rec);

        let result = dispatcher.dispatch_memory(false, 0x5E, &mut cpu, &mut bus);
        assert!(result.is_some(), "NMInstall should be handled");
        assert!(result.unwrap().is_ok(), "NMInstall should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "NMInstall should return noErr in D0 for nominal input"
        );
    }

    #[test]
    fn nminstall_uses_a0_nmrecptr_register_calling_convention() {
        // Inside Macintosh Volume VI (1991), p. 24-10:
        // NMInstall takes NMRecPtr in A0 and returns OSErr in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let nm_rec = bus.alloc(32);
        bus.write_word(nm_rec + 4, 8); // qType = ORD(nmType)
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xFEED_FACE);
        cpu.write_reg(Register::A0, nm_rec);

        let result = dispatcher.dispatch_memory(false, 0x5E, &mut cpu, &mut bus);
        assert!(result.is_some(), "NMInstall should be handled");
        assert!(result.unwrap().is_ok(), "NMInstall should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "NMInstall should not consume a Pascal stack argument frame"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xFEED_FACE,
            "stack top should remain untouched by register calling convention"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "NMInstall should return noErr"
        );
    }

    #[test]
    fn nmremove_returns_noerr_for_nominal_notification_request() {
        // Inside Macintosh Volume VI (1991), p. 24-11:
        // NMRemove returns noErr for a successful request removal.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let nm_rec = bus.alloc(32);
        bus.write_word(nm_rec + 4, 8); // qType = ORD(nmType)
        cpu.write_reg(Register::A0, nm_rec);

        let result = dispatcher.dispatch_memory(false, 0x5F, &mut cpu, &mut bus);
        assert!(result.is_some(), "NMRemove should be handled");
        assert!(result.unwrap().is_ok(), "NMRemove should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "NMRemove should return noErr in D0 for nominal input"
        );
    }

    #[test]
    fn nmremove_uses_a0_nmrecptr_register_calling_convention() {
        // Inside Macintosh Volume VI (1991), p. 24-11:
        // NMRemove takes NMRecPtr in A0 and returns OSErr in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let nm_rec = bus.alloc(32);
        bus.write_word(nm_rec + 4, 8); // qType = ORD(nmType)
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xC0DE_CAFE);
        cpu.write_reg(Register::A0, nm_rec);

        let result = dispatcher.dispatch_memory(false, 0x5F, &mut cpu, &mut bus);
        assert!(result.is_some(), "NMRemove should be handled");
        assert!(result.unwrap().is_ok(), "NMRemove should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "NMRemove should not consume a Pascal stack argument frame"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xC0DE_CAFE,
            "stack top should remain untouched by register calling convention"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "NMRemove should return noErr"
        );
    }

    #[test]
    fn lowertext_variant_converts_ascii_and_macroman_uppercase_to_lowercase() {
        // Inside Macintosh Volume VI (1991), p. 14-62: LowerText localizes
        // lowercase conversion for len bytes at A0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let input = [b'A', 0x83, b'Z', b'!', 0x84, 0x8E];
        let ptr = bus.alloc(input.len() as u32);
        bus.write_bytes(ptr, &input);

        dispatcher.current_trap_word = 0xA056;
        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 5);
        let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);

        assert!(result.is_some(), "LowerText variant should be handled");
        assert!(result.unwrap().is_ok(), "LowerText should return");
        assert_eq!(
            bus.read_bytes(ptr, input.len()),
            vec![b'a', 0x8E, b'z', b'!', 0x96, 0x8E],
            "LowerText should lowercase only the requested len bytes"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "LowerText should return noErr in D0"
        );
    }

    #[test]
    fn uppertext_variant_converts_ascii_and_macroman_lowercase_to_uppercase() {
        // Inside Macintosh Volume VI (1991), p. 14-63: UpperText localizes
        // uppercase conversion for len bytes at A0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let input = [b'a', 0x8E, b'z', b'?', 0x96, 0x8A];
        let ptr = bus.alloc(input.len() as u32);
        bus.write_bytes(ptr, &input);

        dispatcher.current_trap_word = 0xA456;
        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 5);
        let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);

        assert!(result.is_some(), "UpperText variant should be handled");
        assert!(result.unwrap().is_ok(), "UpperText should return");
        assert_eq!(
            bus.read_bytes(ptr, input.len()),
            vec![b'A', 0x83, b'Z', b'?', 0x84, 0x8A],
            "UpperText should uppercase only the requested len bytes"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "UpperText should return noErr in D0"
        );
    }

    #[test]
    fn striptext_variant_strips_diacriticals_without_case_fold() {
        // Inside Macintosh Volume VI (1991), p. 14-63: StripText removes
        // diacritical marks without forcing uppercase.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let input = [0x8E, 0x83, 0x96, 0x84, b'Q', 0x9A];
        let ptr = bus.alloc(input.len() as u32);
        bus.write_bytes(ptr, &input);

        dispatcher.current_trap_word = 0xA256;
        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 5);
        let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);

        assert!(result.is_some(), "StripText variant should be handled");
        assert!(result.unwrap().is_ok(), "StripText should return");
        assert_eq!(
            bus.read_bytes(ptr, input.len()),
            vec![b'e', b'E', b'n', b'N', b'Q', 0x9A],
            "StripText should strip marks while preserving letter case"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "StripText should return noErr in D0"
        );
    }

    #[test]
    fn stripuppertext_variant_strips_diacriticals_and_uppercases() {
        // Inside Macintosh Volume VI (1991), p. 14-63: StripUpperText strips
        // diacritical marks and uppercases text for len bytes at A0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let input = [0x8E, 0x8A, b'x', 0x96, 0xCF, b'?'];
        let ptr = bus.alloc(input.len() as u32);
        bus.write_bytes(ptr, &input);

        dispatcher.current_trap_word = 0xA656;
        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 5);
        let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);

        assert!(result.is_some(), "StripUpperText variant should be handled");
        assert!(result.unwrap().is_ok(), "StripUpperText should return");
        assert_eq!(
            bus.read_bytes(ptr, input.len()),
            vec![b'E', b'A', b'X', b'N', b'O', b'?'],
            "StripUpperText should strip marks and uppercase requested bytes"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "StripUpperText should return noErr in D0"
        );
    }

    #[test]
    fn lowertext_family_variants_return_noerr_in_d0_for_nominal_calls() {
        // Inside Macintosh Volume VI (1991), p. 14-63: LowerText-family
        // result codes include noErr and resNotFound; nominal calls return noErr.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        for trap_word in [0xA056u16, 0xA256, 0xA456, 0xA656] {
            let ptr = bus.alloc(3);
            bus.write_bytes(ptr, b"Ae?");
            dispatcher.current_trap_word = trap_word;
            cpu.write_reg(Register::A0, ptr);
            cpu.write_reg(Register::D0, 3);
            let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);
            assert!(
                result.is_some(),
                "LowerText-family variant ${trap_word:04X} should be handled"
            );
            assert!(
                result.unwrap().is_ok(),
                "LowerText-family variant ${trap_word:04X} should return"
            );
            assert_eq!(
                cpu.read_reg(Register::D0),
                0,
                "LowerText-family variant ${trap_word:04X} should return noErr in D0"
            );
        }
    }

    #[test]
    fn lowertext_family_returns_noerr_in_d0_for_each_trap_word_variant() {
        // MPW Universal Headers declare the LowerText family as
        // void-returning, so C-side callers cannot sample D0 without
        // inline asm. This test pins D0=0 (noErr) on Systemless for each
        // $A056/$A256/$A456/$A656 dispatch over the input bytes
        // {0x41, 0x61, 0x83, 0x8E}, completing the round-trip contract.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        for trap_word in [0xA056u16, 0xA256, 0xA456, 0xA656] {
            let ptr = bus.alloc(4);
            bus.write_bytes(ptr, &[0x41, 0x61, 0x83, 0x8E]);
            dispatcher.current_trap_word = trap_word;
            cpu.write_reg(Register::A0, ptr);
            cpu.write_reg(Register::D0, 4);
            let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);
            assert!(
                result.is_some(),
                "LowerText-family variant ${trap_word:04X} dispatch must be handled"
            );
            assert!(
                result.unwrap().is_ok(),
                "LowerText-family variant ${trap_word:04X} dispatch must return Ok"
            );
            assert_eq!(
                cpu.read_reg(Register::D0),
                0,
                "LowerText-family variant ${trap_word:04X} must return noErr in D0 per IM:VI 14-63"
            );
        }
    }

    #[test]
    fn lowertext_family_variants_match_documented_output_byte_sequences() {
        // Input: {0x41 'A', 0x61 'a', 0x83 'É', 0x8E 'é'} — 4 bytes.
        //
        // Documented outputs per IM:VI 14-62..14-63 and IM:IV IV-235:
        //   $A056 LowerText      → {0x61, 0x61, 0x8E, 0x8E}  "aa\x8E\x8E"
        //   $A456 UpperText      → {0x41, 0x41, 0x83, 0x83}  "AA\x83\x83"
        //   $A256 StripText      → {0x41, 0x61, 0x45, 0x65}  "AaEe"
        //   $A656 StripUpperText → {0x41, 0x41, 0x45, 0x45}  "AAEE"
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let cases: &[(u16, [u8; 4])] = &[
            (0xA056, [0x61, 0x61, 0x8E, 0x8E]),
            (0xA456, [0x41, 0x41, 0x83, 0x83]),
            (0xA256, [0x41, 0x61, 0x45, 0x65]),
            (0xA656, [0x41, 0x41, 0x45, 0x45]),
        ];
        for &(trap_word, expected) in cases {
            let ptr = bus.alloc(4);
            bus.write_bytes(ptr, &[0x41, 0x61, 0x83, 0x8E]);
            dispatcher.current_trap_word = trap_word;
            cpu.write_reg(Register::A0, ptr);
            cpu.write_reg(Register::D0, 4);
            let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);
            assert!(
                result.is_some(),
                "Variant ${trap_word:04X} dispatch must be handled"
            );
            assert!(
                result.unwrap().is_ok(),
                "Variant ${trap_word:04X} dispatch must return Ok"
            );
            assert_eq!(
                bus.read_bytes(ptr, 4),
                expected.to_vec(),
                "Variant ${trap_word:04X} must produce documented byte sequence"
            );
        }
    }

    #[test]
    fn setvideodefault_roundtrips_defvideorec_through_getvideodefault() {
        // Inside Macintosh Volume V (1986), pp. V-354..V-355:
        // SetVideoDefault consumes DefVideoRec {sdSlot, sdSResource}, and
        // GetVideoDefault returns the stored default video record.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let set_pb = bus.alloc(2);
        bus.write_byte(set_pb, 0x0E);
        bus.write_byte(set_pb + 1, 0x2A);

        cpu.write_reg(Register::A0, set_pb);
        let set_result = dispatcher.dispatch_memory(false, 0x81, &mut cpu, &mut bus);
        assert!(set_result.is_some(), "SetVideoDefault should be handled");
        assert!(
            set_result.unwrap().is_ok(),
            "SetVideoDefault should succeed"
        );

        let get_pb = bus.alloc(2);
        cpu.write_reg(Register::A0, get_pb);
        let get_result = dispatcher.dispatch_memory(false, 0x80, &mut cpu, &mut bus);
        assert!(get_result.is_some(), "GetVideoDefault should be handled");
        assert!(
            get_result.unwrap().is_ok(),
            "GetVideoDefault should succeed"
        );
        assert_eq!(
            bus.read_byte(get_pb),
            0x0E,
            "GetVideoDefault should return sdSlot written by SetVideoDefault"
        );
        assert_eq!(
            bus.read_byte(get_pb + 1),
            0x2A,
            "GetVideoDefault should return sdSResource written by SetVideoDefault"
        );
    }

    #[test]
    fn getvideodefault_writes_two_bytes_and_preserves_following_memory() {
        // Inside Macintosh Volume V (1986), p. V-354: DefVideoRec is two bytes
        // (sdSlot, sdSResource), so GetVideoDefault should write exactly that
        // record at A0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = bus.alloc(4);
        bus.write_byte(pb, 0xAA);
        bus.write_byte(pb + 1, 0xBB);
        bus.write_byte(pb + 2, 0xC3);
        bus.write_byte(pb + 3, 0xD4);

        cpu.write_reg(Register::A0, pb);
        let result = dispatcher.dispatch_memory(false, 0x80, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetVideoDefault should be handled");
        assert!(result.unwrap().is_ok(), "GetVideoDefault should succeed");
        assert_eq!(
            bus.read_byte(pb),
            0,
            "Default sdSlot should be 0 when no explicit video default is set"
        );
        assert_eq!(
            bus.read_byte(pb + 1),
            0,
            "Default sdSResource should be 0 when no explicit video default is set"
        );
        assert_eq!(
            bus.read_byte(pb + 2),
            0xC3,
            "GetVideoDefault must not overwrite bytes beyond DefVideoRec"
        );
        assert_eq!(
            bus.read_byte(pb + 3),
            0xD4,
            "GetVideoDefault must preserve trailing bytes in caller memory"
        );
    }

    #[test]
    fn setosdefault_roundtrips_sdostype_and_getosdefault_reports_reserved_zero() {
        // Inside Macintosh Volume V (1986), p. V-355: SetOSDefault specifies
        // sdOSType; sdReserved is reserved and should be 0 when read back via
        // GetOSDefault.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let set_pb = bus.alloc(2);
        bus.write_byte(set_pb, 0x7F);
        bus.write_byte(set_pb + 1, 0x05);

        cpu.write_reg(Register::A0, set_pb);
        let set_result = dispatcher.dispatch_memory(false, 0x83, &mut cpu, &mut bus);
        assert!(set_result.is_some(), "SetOSDefault should be handled");
        assert!(set_result.unwrap().is_ok(), "SetOSDefault should succeed");

        let get_pb = bus.alloc(2);
        cpu.write_reg(Register::A0, get_pb);
        let get_result = dispatcher.dispatch_memory(false, 0x84, &mut cpu, &mut bus);
        assert!(get_result.is_some(), "GetOSDefault should be handled");
        assert!(get_result.unwrap().is_ok(), "GetOSDefault should succeed");
        assert_eq!(
            bus.read_byte(get_pb),
            0,
            "GetOSDefault should report sdReserved as 0"
        );
        assert_eq!(
            bus.read_byte(get_pb + 1),
            0x05,
            "GetOSDefault should return sdOSType written by SetOSDefault"
        );
    }

    #[test]
    fn setosdefault_preserves_caller_defosrec_input_bytes_read_only_a0() {
        // Inside Macintosh Volume V (1986), p. V-355: SetOSDefault's parameter
        // block direction arrows (`→` in both rows of the trap-macro summary)
        // document sdReserved + sdOSType as INPUTs supplied by the caller; the
        // trap copies them into the in-session default record without writing
        // back to the caller's buffer.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = bus.alloc(4);
        bus.write_byte(pb, 0x00);
        bus.write_byte(pb + 1, 0x02);
        bus.write_byte(pb + 2, 0xCC);
        bus.write_byte(pb + 3, 0xCC);

        cpu.write_reg(Register::A0, pb);
        let result = dispatcher.dispatch_memory(false, 0x83, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetOSDefault should be handled");
        assert!(result.unwrap().is_ok(), "SetOSDefault should succeed");

        assert_eq!(
            bus.read_byte(pb),
            0x00,
            "SetOSDefault must not modify caller's sdReserved input byte"
        );
        assert_eq!(
            bus.read_byte(pb + 1),
            0x02,
            "SetOSDefault must not modify caller's sdOSType input byte"
        );
        assert_eq!(
            bus.read_byte(pb + 2),
            0xCC,
            "SetOSDefault must not clobber memory past DefOSRec at byte +2"
        );
        assert_eq!(
            bus.read_byte(pb + 3),
            0xCC,
            "SetOSDefault must not clobber memory past DefOSRec at byte +3"
        );
    }

    #[test]
    fn getosdefault_writes_two_bytes_and_preserves_following_memory() {
        // Inside Macintosh Volume V (1986), p. V-355: DefOSRec is two bytes
        // (sdReserved, sdOSType), and Macintosh OS is represented by
        // sdOSType=1.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = bus.alloc(4);
        bus.write_byte(pb, 0xAA);
        bus.write_byte(pb + 1, 0xBB);
        bus.write_byte(pb + 2, 0xC3);
        bus.write_byte(pb + 3, 0xD4);

        cpu.write_reg(Register::A0, pb);
        let result = dispatcher.dispatch_memory(false, 0x84, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetOSDefault should be handled");
        assert!(result.unwrap().is_ok(), "GetOSDefault should succeed");
        assert_eq!(
            bus.read_byte(pb),
            0,
            "GetOSDefault should return sdReserved as 0"
        );
        assert_eq!(
            bus.read_byte(pb + 1),
            1,
            "GetOSDefault should default sdOSType to Macintosh OS (1)"
        );
        assert_eq!(
            bus.read_byte(pb + 2),
            0xC3,
            "GetOSDefault must not overwrite bytes beyond DefOSRec"
        );
        assert_eq!(
            bus.read_byte(pb + 3),
            0xD4,
            "GetOSDefault must preserve trailing bytes in caller memory"
        );
    }

    #[test]
    fn test_free_mem() {
        // FreeMem returns `free_heap_estimate` clamped to [24MB, 64MB].
        // setup() leaves HEAP_END/APPL_LIMIT at 0 so the floor (24MB) wins.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let result = dispatcher.dispatch_memory(false, 0x1C, &mut cpu, &mut bus);
        assert!(result.is_some(), "FreeMem should be handled");
        assert!(result.unwrap().is_ok(), "FreeMem should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            24 * 1024 * 1024,
            "FreeMem should return 24MB clamp floor in D0"
        );
    }

    #[test]
    fn free_mem_honors_explicit_application_partition_below_compat_floor() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let heap_end = 0x0020_0000 + 64;
        let appl_limit = 0x0050_0000;
        bus.write_long(crate::memory::globals::addr::MEM_TOP, 32 * 1024 * 1024);
        bus.write_long(crate::memory::globals::addr::HEAP_END, heap_end);
        bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);

        let result = dispatcher.dispatch_memory(false, 0x1C, &mut cpu, &mut bus);

        assert!(result.is_some(), "FreeMem should be handled");
        assert!(result.unwrap().is_ok(), "FreeMem should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            appl_limit - heap_end,
            "FreeMem must report the actual small partition instead of the compatibility floor"
        );
    }

    #[test]
    fn free_mem_uses_zone_zcbfree_after_maxapplzone_extends_heapend() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let app_zone = 0x0020_0000;
        let appl_limit = 0x0220_0000;
        let zcb_free = 0x01FF_DFC0;
        bus.write_long(crate::memory::globals::addr::MEM_TOP, 64 * 1024 * 1024);
        bus.write_long(crate::memory::globals::addr::APP_L_ZONE, app_zone);
        bus.write_long(crate::memory::globals::addr::THE_ZONE, app_zone);
        bus.write_long(crate::memory::globals::addr::HEAP_END, appl_limit);
        bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);
        bus.write_long(app_zone, appl_limit); // bkLim
        bus.write_long(app_zone + 12, zcb_free); // zcbFree

        let result = dispatcher.dispatch_memory(false, 0x1C, &mut cpu, &mut bus);

        assert!(result.is_some(), "FreeMem should be handled");
        assert!(result.unwrap().is_ok(), "FreeMem should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            zcb_free,
            "FreeMem must read the zone header's free-byte count after MaxApplZone"
        );
    }

    #[test]
    fn free_mem_survives_setapplimit_then_maxapplzone_startup_sequence() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let app_zone = 0x0020_0000;
        let heap_end = app_zone + 64;
        let original_limit = 0x006A_E000;
        let lowered_limit = original_limit - 0x4000;
        let original_free = original_limit - app_zone - 64;
        let lowered_free = original_free - 0x4000;
        bus.write_long(crate::memory::globals::addr::MEM_TOP, 64 * 1024 * 1024);
        bus.write_long(crate::memory::globals::addr::APP_L_ZONE, app_zone);
        bus.write_long(crate::memory::globals::addr::THE_ZONE, app_zone);
        bus.write_long(crate::memory::globals::addr::HEAP_END, heap_end);
        bus.write_long(crate::memory::globals::addr::APPL_LIMIT, original_limit);
        bus.write_long(app_zone, original_limit); // bkLim
        bus.write_long(app_zone + 12, original_free); // zcbFree

        cpu.write_reg(Register::A0, lowered_limit);
        let result = dispatcher.dispatch_memory(false, 0x2D, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetApplLimit should be handled");
        assert!(result.unwrap().is_ok(), "SetApplLimit should succeed");
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
            lowered_limit,
            "SetApplLimit should lower ApplLimit before MaxApplZone"
        );
        assert_eq!(
            bus.read_long(app_zone),
            lowered_limit,
            "SetApplLimit should keep application-zone bkLim in sync"
        );
        assert_eq!(
            bus.read_long(app_zone + 12),
            lowered_free,
            "SetApplLimit should reduce zcbFree by the removed zone span"
        );

        let result = dispatcher.dispatch_memory(false, 0x63, &mut cpu, &mut bus);
        assert!(result.is_some(), "MaxApplZone should be handled");
        assert!(result.unwrap().is_ok(), "MaxApplZone should succeed");
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::HEAP_END),
            lowered_limit,
            "MaxApplZone should expand HeapEnd to the lowered ApplLimit"
        );

        let result = dispatcher.dispatch_memory(false, 0x1C, &mut cpu, &mut bus);
        assert!(result.is_some(), "FreeMem should be handled");
        assert!(result.unwrap().is_ok(), "FreeMem should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            lowered_free,
            "FreeMem must not collapse to zero after SetApplLimit followed by MaxApplZone"
        );
    }

    #[test]
    fn test_max_mem() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let result = dispatcher.dispatch_memory(false, 0x1D, &mut cpu, &mut bus);
        assert!(result.is_some(), "MaxMem should be handled");
        assert!(result.unwrap().is_ok(), "MaxMem should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            24 * 1024 * 1024,
            "MaxMem should return 24MB clamp floor in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            24 * 1024 * 1024,
            "MaxMem should return 24MB clamp floor in A0"
        );
    }

    #[test]
    fn test_compact_mem() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 1024); // requested size
        let result = dispatcher.dispatch_memory(false, 0x4C, &mut cpu, &mut bus);
        assert!(result.is_some(), "CompactMem should be handled");
        assert!(result.unwrap().is_ok(), "CompactMem should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            24 * 1024 * 1024,
            "CompactMem should return 24MB clamp floor in D0"
        );
    }

    #[test]
    fn test_set_handle_size() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        // First create a handle of size 128
        cpu.write_reg(Register::D0, 128);
        let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let handle = cpu.read_reg(Register::A0);
        let old_ptr = bus.read_long(handle);
        // Write some data into the block
        bus.write_long(old_ptr, 0xDEADBEEF);
        bus.write_long(old_ptr + 4, 0xCAFEBABE);
        // Resize to 2048
        cpu.write_reg(Register::A0, handle);
        cpu.write_reg(Register::D0, 2048);
        let result = dispatcher.dispatch_memory(false, 0x24, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetHandleSize should be handled");
        assert!(result.unwrap().is_ok(), "SetHandleSize should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetHandleSize should set D0 to 0 (noErr)"
        );
        // Handle should now point to new block with old data preserved
        let new_ptr = bus.read_long(handle);
        assert_eq!(
            bus.read_long(new_ptr),
            0xDEADBEEF,
            "Data should be preserved after resize"
        );
        assert_eq!(
            bus.read_long(new_ptr + 4),
            0xCAFEBABE,
            "Data should be preserved after resize"
        );
        // New block should be tracked with new size
        let new_size = bus.get_alloc_size(new_ptr).unwrap_or(0);
        assert_eq!(new_size, 2048, "New block should be 2048 bytes");
    }

    #[test]
    fn set_handle_size_in_place_growth_updates_logical_size_before_later_move() {
        let (mut dispatcher, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, 1);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let handle = cpu.read_reg(Register::A0);
        let ptr = bus.read_long(handle);
        bus.write_byte(ptr, 0xAA);

        cpu.write_reg(Register::A0, handle);
        cpu.write_reg(Register::D0, 3);
        dispatcher
            .dispatch_memory(false, 0x24, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0, "noErr expected");
        assert_eq!(
            bus.read_long(handle),
            ptr,
            "growth within the same bucket should keep the handle data pointer stable"
        );
        assert_eq!(
            bus.get_alloc_size(ptr),
            Some(3),
            "the logical handle size must be updated even when no move occurs"
        );

        bus.write_byte(ptr + 1, 0xBB);
        bus.write_byte(ptr + 2, 0xCC);

        cpu.write_reg(Register::A0, handle);
        cpu.write_reg(Register::D0, 9);
        dispatcher
            .dispatch_memory(false, 0x24, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0, "noErr expected");
        let moved_ptr = bus.read_long(handle);
        assert_ne!(
            moved_ptr, ptr,
            "growth beyond the current bucket should move the relocatable block"
        );
        assert_eq!(
            bus.read_bytes(moved_ptr, 3),
            vec![0xAA, 0xBB, 0xCC],
            "later moves must copy the full logical size recorded by the in-place grow"
        );
        assert_eq!(bus.get_alloc_size(moved_ptr), Some(9));
    }

    #[test]
    fn test_reallocate_handle() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        // First create a handle via NewHandle
        cpu.write_reg(Register::D0, 128);
        let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok(), "NewHandle (setup) should succeed");
        let handle = cpu.read_reg(Register::A0);
        let old_ptr = bus.read_long(handle);

        // Now reallocate it with a new size
        cpu.write_reg(Register::A0, handle);
        cpu.write_reg(Register::D0, 2048);
        let result = dispatcher.dispatch_memory(false, 0x27, &mut cpu, &mut bus);
        assert!(result.is_some(), "ReallocateHandle should be handled");
        assert!(result.unwrap().is_ok(), "ReallocateHandle should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "ReallocateHandle should set D0 to 0"
        );
        let new_ptr = bus.read_long(handle);
        assert!(
            new_ptr >= 0x200000,
            "ReallocateHandle should update handle to a valid pointer, got ${:08X}",
            new_ptr
        );
        assert_ne!(
            new_ptr, old_ptr,
            "ReallocateHandle should allocate a new block (different from old ptr)"
        );
    }

    #[test]
    fn test_recover_handle() {
        // Per IM:V V-579, RecoverHandle searches the master pointer
        // table and returns the EXISTING handle that owns the given
        // ptr. Allocate a real handle, then verify RecoverHandle on
        // its data ptr returns that same handle.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, 100);
        let _ = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus); // NewHandle
        let original_handle = cpu.read_reg(Register::A0);
        let data_ptr = bus.read_long(original_handle);

        cpu.write_reg(Register::A0, data_ptr);
        let result = dispatcher.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
        assert!(result.is_some(), "RecoverHandle should be handled");
        assert!(result.unwrap().is_ok(), "RecoverHandle should succeed");
        assert_eq!(
            cpu.read_reg(Register::A0),
            original_handle,
            "RecoverHandle must return the existing handle, not a fresh copy"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "RecoverHandle should set D0 to 0"
        );
    }

    #[test]
    fn test_dispose_then_recover_returns_stale_handle() {
        // On real Mac OS, DisposeHandle frees the master pointer slot but
        // does NOT zero it. RecoverHandle scans all slots (including freed
        // ones), so it still finds the stale data address and returns the
        // freed handle rather than nil. IM:V V-579.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 100);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let handle = cpu.read_reg(Register::A0);
        let data_ptr = bus.read_long(handle);
        cpu.write_reg(Register::A0, handle);
        dispatcher
            .dispatch_memory(false, 0x23, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        cpu.write_reg(Register::A0, data_ptr);
        dispatcher
            .dispatch_memory(false, 0x28, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_ne!(
            cpu.read_reg(Register::A0),
            0,
            "RecoverHandle after DisposeHandle must return stale freed handle (not nil), per IM:V V-579 scan-all-slots behavior"
        );
    }

    #[test]
    fn recover_handle_rejects_stale_mapping_after_master_pointer_reuse() {
        // A disposed slot remains discoverable only until the Memory Manager
        // reuses it. Once NewHandle overwrites the slot with a new data pointer,
        // a table scan can no longer recover the old pointer from that slot.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, 100);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let old_handle = cpu.read_reg(Register::A0);
        let old_ptr = bus.read_long(old_handle);

        cpu.write_reg(Register::A0, old_handle);
        dispatcher
            .dispatch_memory(false, 0x23, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        cpu.write_reg(Register::D0, 200);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let new_handle = cpu.read_reg(Register::A0);
        assert_eq!(
            new_handle, old_handle,
            "the freed handle slot should be reused"
        );
        assert_ne!(bus.read_long(new_handle), old_ptr);

        cpu.write_reg(Register::A0, old_ptr);
        dispatcher
            .dispatch_memory(false, 0x28, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "RecoverHandle must not alias an old pointer to the handle now occupying its former slot"
        );
    }

    #[test]
    fn test_get_ptr_size() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        // First allocate a pointer of size 512
        cpu.write_reg(Register::D0, 512);
        let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let ptr = cpu.read_reg(Register::A0);
        // Now get its size
        cpu.write_reg(Register::A0, ptr);
        let result = dispatcher.dispatch_memory(false, 0x21, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetPtrSize should be handled");
        assert!(result.unwrap().is_ok(), "GetPtrSize should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            512,
            "GetPtrSize should return 512 in D0"
        );
    }

    // SetPtrSize must keep the pointer stable per IM:Memory 1992 p.2-44
    // ("SetPtrSize doesn't move the pointer").
    #[test]
    fn test_set_ptr_size_shrink_preserves_pointer() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        // NewPtr(256)
        cpu.write_reg(Register::D0, 256);
        dispatcher
            .dispatch_memory(false, 0x1E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let original_ptr = cpu.read_reg(Register::A0);
        assert_ne!(original_ptr, 0);
        // SetPtrSize(128)
        cpu.write_reg(Register::A0, original_ptr);
        cpu.write_reg(Register::D0, 128);
        dispatcher
            .dispatch_memory(false, 0x20, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr expected");
        assert_eq!(
            bus.read_word(addr::MEM_ERR),
            0,
            "successful SetPtrSize must clear MemErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            original_ptr,
            "SetPtrSize must not move the pointer (IM:Memory 1992 p.2-44)"
        );
        // GetPtrSize should now read 128 at the ORIGINAL pointer.
        cpu.write_reg(Register::A0, original_ptr);
        dispatcher
            .dispatch_memory(false, 0x21, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            cpu.read_reg(Register::D0),
            128,
            "GetPtrSize on the original pointer must reflect the new logical size"
        );
    }

    #[test]
    fn test_set_ptr_size_zero_allocation_can_grow_within_minimum_bucket() {
        let (mut dispatcher, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, 0);
        dispatcher
            .dispatch_memory(false, 0x1E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let ptr = cpu.read_reg(Register::A0);
        assert_ne!(ptr, 0);

        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 1);
        dispatcher
            .dispatch_memory(false, 0x20, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0, "noErr expected");
        assert_eq!(
            cpu.read_reg(Register::A0),
            ptr,
            "SetPtrSize within the minimum allocation bucket should not move the pointer"
        );
        assert_eq!(bus.get_alloc_size(ptr), Some(1));
    }

    // Attempting to grow a Ptr beyond its aligned capacity returns
    // memFullErr rather than silently moving the pointer. This matches
    // IM:Memory 1992's "SetPtrSize doesn't move the pointer" contract.
    #[test]
    fn test_set_ptr_size_grow_beyond_capacity_returns_memfullerr() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 128);
        dispatcher
            .dispatch_memory(false, 0x1E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let ptr = cpu.read_reg(Register::A0);
        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 4096); // way beyond aligned 128
        dispatcher
            .dispatch_memory(false, 0x20, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -108,
            "SetPtrSize beyond aligned capacity must return memFullErr (-108)"
        );
        assert_eq!(
            bus.read_word(addr::MEM_ERR) as i16,
            -108,
            "failed SetPtrSize must publish memFullErr through MemErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            ptr,
            "SetPtrSize on failure must leave A0 untouched"
        );
    }

    #[test]
    fn test_ins_time() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000); // TMTask pointer
        let result = dispatcher.dispatch_memory(false, 0x58, &mut cpu, &mut bus);
        assert!(result.is_some(), "InsTime should be handled");
        assert!(result.unwrap().is_ok(), "InsTime should succeed (no-op)");
    }

    #[test]
    fn test_rmv_time() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000);
        let result = dispatcher.dispatch_memory(false, 0x59, &mut cpu, &mut bus);
        assert!(result.is_some(), "RmvTime should be handled");
        assert!(result.unwrap().is_ok(), "RmvTime should succeed (no-op)");
    }

    #[test]
    fn test_prime_time() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000); // TMTask pointer
        cpu.write_reg(Register::D0, 1000); // delay
        let result = dispatcher.dispatch_memory(false, 0x5A, &mut cpu, &mut bus);
        assert!(result.is_some(), "PrimeTime should be handled");
        assert!(result.unwrap().is_ok(), "PrimeTime should succeed (no-op)");
    }

    #[test]
    fn test_prime_time_rounds_up_to_tick_boundary() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = 0x200000;

        bus.write_long(0x016A, 100);
        cpu.write_reg(Register::A0, task_ptr);
        let result = dispatcher.dispatch_memory(false, 0x58, &mut cpu, &mut bus);
        assert!(result.is_some(), "InsTime should be handled");
        assert!(result.unwrap().is_ok(), "InsTime should succeed");

        cpu.write_reg(Register::A0, task_ptr);
        cpu.write_reg(Register::D0, 33);
        let result = dispatcher.dispatch_memory(false, 0x5A, &mut cpu, &mut bus);
        assert!(result.is_some(), "PrimeTime should be handled");
        assert!(result.unwrap().is_ok(), "PrimeTime should succeed");

        let task = dispatcher
            .timer_tasks
            .iter()
            .find(|task| task.task_ptr == task_ptr)
            .expect("timer task should be queued");
        assert!(task.active, "PrimeTime should activate the task");
        assert_eq!(
            task.fire_at_tick, 102,
            "33 ms should round up to two 60 Hz ticks from TickCount 100"
        );
    }

    #[test]
    fn test_prime_time_preserves_negative_microsecond_deadline() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let task_ptr = 0x200000;

        bus.write_long(0x016A, 100);
        cpu.write_reg(Register::A0, task_ptr);
        dispatcher
            .dispatch_memory(false, 0x58, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        cpu.write_reg(Register::A0, task_ptr);
        cpu.write_reg(Register::D0, (-3333_i32) as u32);
        dispatcher
            .dispatch_memory(false, 0x5A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let task = dispatcher
            .timer_tasks
            .iter()
            .find(|task| task.task_ptr == task_ptr)
            .expect("timer task should be queued");
        assert_eq!(task.fire_at_tick, 101);
        assert_eq!(
            task.fire_at_subtick, 100_199_980,
            "3,333 microseconds should remain below one 60 Hz guest tick"
        );
    }

    #[test]
    fn test_control_set_mode_uses_inline_vdpginfo() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;

        dispatcher.screen_mode.0 = 0x01F80000;
        bus.write_word(pb + 26, 2); // csCode = cscSetMode
        bus.write_word(pb + 28, 1); // csParam.csMode
        bus.write_long(pb + 30, 0x12345678); // csParam.csData
        bus.write_word(pb + 34, 0xFFFF); // csParam.csPage
        bus.write_long(pb + 36, 0xDEADBEEF); // csParam.csBaseAddr

        cpu.write_reg(Register::A0, pb);
        let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
        assert!(result.is_some(), "PBControl should be handled");
        assert!(result.unwrap().is_ok(), "PBControl should succeed");

        assert_eq!(
            bus.read_word(pb + 34),
            0,
            "SetMode should return page 0 through inline csParam VDPgInfo"
        );
        assert_eq!(
            bus.read_long(pb + 36),
            0x01F80000,
            "SetMode should return the framebuffer base through inline csParam VDPgInfo"
        );
    }

    #[test]
    fn test_control_set_mode_does_not_dereference_csparam_as_pointer() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;
        let pointer_shaped_csparam = 0x310000u32;

        dispatcher.screen_mode.0 = 0x01F80000;
        bus.write_word(pb + 26, 2); // csCode = cscSetMode
        bus.write_long(pb + 28, pointer_shaped_csparam);
        bus.write_word(pb + 34, 0xFFFF);
        bus.write_long(pb + 36, 0xDEADBEEF);
        bus.write_word(pointer_shaped_csparam + 6, 0x7777);
        bus.write_long(pointer_shaped_csparam + 8, 0xCAFEBABE);

        cpu.write_reg(Register::A0, pb);
        let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
        assert!(result.is_some(), "PBControl should be handled");
        assert!(result.unwrap().is_ok(), "PBControl should succeed");

        assert_eq!(
            bus.read_word(pointer_shaped_csparam + 6),
            0x7777,
            "SetMode should not treat csParam's first longword as a VDPgInfo pointer"
        );
        assert_eq!(
            bus.read_long(pointer_shaped_csparam + 8),
            0xCAFEBABE,
            "SetMode should not write through a pointer-shaped inline csParam value"
        );
        assert_eq!(
            bus.read_word(pb + 34),
            0,
            "SetMode should still update inline csPage"
        );
        assert_eq!(
            bus.read_long(pb + 36),
            0x01F80000,
            "SetMode should still update inline csBaseAddr"
        );
    }

    #[test]
    fn test_control_set_mode_does_not_write_past_short_stack_frame() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;
        let frame_a6 = pb + 32;

        dispatcher.screen_mode.0 = 0x01F80000;
        bus.write_word(pb + 26, 2); // csCode = cscSetMode
        bus.write_word(pb + 28, 1); // csParam.csMode only
        bus.write_long(frame_a6 + 4, 0x00ABCDEF); // saved return address

        cpu.write_reg(Register::A0, pb);
        cpu.write_reg(Register::A7, pb);
        cpu.write_reg(Register::A6, frame_a6);
        let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
        assert!(result.is_some(), "PBControl should be handled");
        assert!(result.unwrap().is_ok(), "PBControl should succeed");

        assert_eq!(
            bus.read_long(frame_a6 + 4),
            0x00ABCDEF,
            "SetMode should not write csBaseAddr past a short stack parameter block"
        );
    }

    #[test]
    fn test_microseconds() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        // setup() already sets TickCount at $016A to 100
        let ticks = 100u64;
        // 1 tick = 16,625 µs (1/60.15 Hz; Guide to Macintosh Family Hardware 2nd Ed., p. 6-798)
        let expected_usecs = ticks * 16_625;
        let result = dispatcher.dispatch_memory(false, 0x93, &mut cpu, &mut bus);
        assert!(result.is_some(), "Microseconds should be handled");
        assert!(result.unwrap().is_ok(), "Microseconds should succeed");
        // D0 = low 32 bits, A0 = high 32 bits (executor emustubs.cpp convention,
        // retained for Executor-style register-reader callers)
        let d0 = cpu.read_reg(Register::D0) as u64;
        let a0 = cpu.read_reg(Register::A0) as u64;
        let actual_usecs = (a0 << 32) | d0;
        assert_eq!(
            actual_usecs, expected_usecs,
            "Microseconds should return ticks*16625 = {} in D0(lo):A0(hi), got {}",
            expected_usecs, actual_usecs
        );
    }

    #[test]
    fn microseconds_does_not_speculatively_write_through_a0_on_entry() {
        // The MPW Universal Headers `Timer.h` declares Microseconds as
        // FOURWORDINLINE($A193, $225F, $22C8, $2280). The trap itself
        // returns the count in registers (D0 = lo, A0 = hi); the
        // caller-side inline glue ($225F MOVEA.L (A7)+, A1; $22C8
        // MOVE.L A0, (A1)+; $2280 MOVE.L D0, (A1)) then writes the
        // 64-bit result through the caller's UnsignedWide pointer.
        //
        // Under that FOURWORDINLINE pattern, register A0 on entry to
        // the trap is uninitialised scratch — the buffer pointer is
        // still on the stack and won't be moved into A1 until *after*
        // the trap returns. The HLE must NOT speculatively write
        // through whatever value A0 happens to hold, because that
        // would corrupt unrelated guest memory in the common case.
        // Verify the HLE leaves the byte under A0 alone.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let unrelated = 0x322000u32;
        // Pre-fill the "unrelated" memory at the address that A0
        // happens to point to; a buggy HLE that speculatively writes
        // through A0 would clobber this sentinel.
        bus.write_long(unrelated, 0x11223344u32);
        bus.write_long(unrelated + 4, 0x55667788u32);
        cpu.write_reg(Register::A0, unrelated);

        let result = dispatcher.dispatch_memory(false, 0x93, &mut cpu, &mut bus);
        assert!(result.is_some(), "Microseconds should be handled");
        assert!(result.unwrap().is_ok(), "Microseconds should succeed");

        assert_eq!(
            bus.read_long(unrelated),
            0x11223344u32,
            "Microseconds must not write through whatever value A0 held on entry"
        );
        assert_eq!(
            bus.read_long(unrelated + 4),
            0x55667788u32,
            "Microseconds must not write past whatever value A0 held on entry"
        );
    }

    #[test]
    fn readxpram_zero_fills_requested_count_and_returns_noerr() {
        // Inside Macintosh Volume V (1986), p. V-519: ReadXPRam uses D0
        // high-word count and A0 destination pointer.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let dest = 0x322000u32;
        bus.write_bytes(dest, &[0xAA, 0xBB, 0xCC, 0xDD, 0xEE]);
        cpu.write_reg(Register::D0, (4u32 << 16) | 0x0012u32);
        cpu.write_reg(Register::A0, dest);

        let result = dispatcher.dispatch_memory(false, 0x51, &mut cpu, &mut bus);
        assert!(result.is_some(), "ReadXPRam should be handled");
        assert!(result.unwrap().is_ok(), "ReadXPRam should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "ReadXPRam should return noErr"
        );
        assert_eq!(bus.read_bytes(dest, 4), vec![0, 0, 0, 0]);
        assert_eq!(
            bus.read_byte(dest + 4),
            0xEE,
            "ReadXPRam should zero exactly count bytes"
        );
    }

    #[test]
    fn readxpram_uses_d0_count_offset_and_a0_destptr_register_calling_convention() {
        // Inside Macintosh Volume V (1986), p. V-519: ReadXPRam register
        // calling convention does not consume stack arguments.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let dest = 0x322040u32;
        bus.write_bytes(dest, &[0x11, 0x22, 0x33, 0x44]);
        cpu.write_reg(Register::D0, (3u32 << 16) | 0x00FFu32);
        cpu.write_reg(Register::A0, dest);

        let result = dispatcher.dispatch_memory(false, 0x51, &mut cpu, &mut bus);
        assert!(result.is_some(), "ReadXPRam should be handled");
        assert!(result.unwrap().is_ok(), "ReadXPRam should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "ReadXPRam should preserve A7 in register-calling convention"
        );
        assert_eq!(bus.read_bytes(dest, 3), vec![0, 0, 0]);
        assert_eq!(bus.read_byte(dest + 3), 0x44);
    }

    #[test]
    fn readxpram_five_call_composition_preserves_stack_across_varying_count_offset() {
        // 5 successive _ReadXPRam dispatches with varying D0 packed
        // (count << 16) | offset values must each preserve A7. Per
        // IM:V V-519 the register-only ABI takes A0 + D0 inputs and
        // returns OSErr in D0; no Pascal stack frame is consumed.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let dest = 0x322200u32;
        let sp_before = cpu.read_reg(Register::A7);

        let cases: [(u32, u32); 5] = [
            ((1u32 << 16) | 0x0014, dest),     // count=1 offset=20 (start of XPRAM)
            ((4u32 << 16) | 0x0080, dest + 8), // count=4 offset=128
            ((8u32 << 16) | 0x00FF, dest + 16), // count=8 offset=255 (last byte)
            ((2u32 << 16) | 0x0000, dest + 32), // count=2 offset=0 (SysParam start)
            ((16u32 << 16) | 0x004B, dest + 48), // count=16 offset=75
        ];

        for (d0, a0) in cases.iter() {
            cpu.write_reg(Register::D0, *d0);
            cpu.write_reg(Register::A0, *a0);
            let result = dispatcher.dispatch_memory(false, 0x51, &mut cpu, &mut bus);
            assert!(result.is_some(), "ReadXPRam should be handled");
            assert!(result.unwrap().is_ok(), "ReadXPRam should return");
            assert_eq!(
                cpu.read_reg(Register::D0),
                0,
                "ReadXPRam should return noErr regardless of count/offset"
            );
        }

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "5 successive ReadXPRam calls must preserve A7 in aggregate"
        );
    }

    #[test]
    fn writexpram_noop_returns_noerr_in_hle_without_persistent_xpram() {
        // Inside Macintosh Volume V (1986), p. V-519: WriteXPRam uses D0
        // count/offset + A0 source pointer. HLE returns noErr without
        // persistent XPRAM storage.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let src = 0x322080u32;
        bus.write_bytes(src, &[0x9A, 0xBC, 0xDE, 0xF0]);
        cpu.write_reg(Register::D0, (4u32 << 16) | 0x0042u32);
        cpu.write_reg(Register::A0, src);

        let result = dispatcher.dispatch_memory(false, 0x52, &mut cpu, &mut bus);
        assert!(result.is_some(), "WriteXPRam should be handled");
        assert!(result.unwrap().is_ok(), "WriteXPRam should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "WriteXPRam should return noErr"
        );
        assert_eq!(
            bus.read_bytes(src, 4),
            vec![0x9A, 0xBC, 0xDE, 0xF0],
            "WriteXPRam should not mutate caller source bytes"
        );
    }

    #[test]
    fn writexpram_uses_d0_count_offset_and_a0_srcptr_register_calling_convention() {
        // Inside Macintosh Volume V (1986), p. V-519: WriteXPRam register
        // convention should preserve A7.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let src = 0x3220C0u32;
        bus.write_bytes(src, &[0x01, 0x23, 0x45]);
        cpu.write_reg(Register::D0, (3u32 << 16) | 0x0001u32);
        cpu.write_reg(Register::A0, src);

        let result = dispatcher.dispatch_memory(false, 0x52, &mut cpu, &mut bus);
        assert!(result.is_some(), "WriteXPRam should be handled");
        assert!(result.unwrap().is_ok(), "WriteXPRam should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "WriteXPRam should not consume stack arguments"
        );
    }

    #[test]
    fn writexpram_five_call_composition_preserves_stack_and_source_across_varying_count_offset() {
        // 5 successive _WriteXPRam dispatches with varying D0 packed
        // (count << 16) | offset values must each preserve A7 AND
        // leave the caller's source bytes untouched. Per IM:V V-519
        // the register-only ABI treats A0 as a READ-ONLY source
        // pointer (the trap propagates the bytes OUT to the clock
        // chip) — the low-memory source buffer is the authoritative
        // input, never overwritten by the trap.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let src = 0x322300u32;
        let sentinel: [u8; 16] = [
            0x4B, 0x58, 0x44, 0x50, 0x01, 0x02, 0x03, 0x04, 0xAA, 0xBB, 0xCC, 0xDD, 0x10, 0x20,
            0x30, 0x40,
        ];
        bus.write_bytes(src, &sentinel);
        let sp_before = cpu.read_reg(Register::A7);

        let cases: [u32; 5] = [
            (1u32 << 16) | 0x0014,  // count=1 offset=20
            (4u32 << 16) | 0x0080,  // count=4 offset=128
            (8u32 << 16) | 0x00FF,  // count=8 offset=255
            (2u32 << 16) | 0x0000,  // count=2 offset=0
            (16u32 << 16) | 0x004B, // count=16 offset=75
        ];

        for d0 in cases.iter() {
            cpu.write_reg(Register::D0, *d0);
            cpu.write_reg(Register::A0, src);
            let result = dispatcher.dispatch_memory(false, 0x52, &mut cpu, &mut bus);
            assert!(result.is_some(), "WriteXPRam should be handled");
            assert!(result.unwrap().is_ok(), "WriteXPRam should return");
            assert_eq!(
                cpu.read_reg(Register::D0),
                0,
                "WriteXPRam should return noErr regardless of count/offset"
            );
        }

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "5 successive WriteXPRam calls must preserve A7 in aggregate"
        );
        assert_eq!(
            bus.read_bytes(src, 16),
            sentinel.to_vec(),
            "5 successive WriteXPRam calls must leave the caller's source buffer untouched"
        );
    }

    #[test]
    fn getdefaultstartup_fills_4_byte_defstartrec_through_a0() {
        // Inside Macintosh Volume V (1986), p. V-529: GetDefaultStartup
        // writes a DefStartRec through A0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let ptr = 0x322100u32;
        bus.write_long(ptr, 0xDEAD_BEEF);
        cpu.write_reg(Register::A0, ptr);

        let result = dispatcher.dispatch_memory(false, 0x7D, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetDefaultStartup should be handled");
        assert!(result.unwrap().is_ok(), "GetDefaultStartup should return");
        assert_eq!(
            bus.read_long(ptr),
            0,
            "GetDefaultStartup should return the initial in-session DefStartRec in HLE"
        );
    }

    #[test]
    fn getdefaultstartup_uses_a0_parameter_block_without_stack_arguments() {
        // Inside Macintosh Volume V (1986), p. V-529: GetDefaultStartup
        // takes A0 parameter block input.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, 0);

        let result = dispatcher.dispatch_memory(false, 0x7D, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetDefaultStartup should be handled");
        assert!(result.unwrap().is_ok(), "GetDefaultStartup should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "GetDefaultStartup should preserve A7"
        );
    }

    #[test]
    fn setdefaultstartup_updates_getdefaultstartup_and_preserves_stack_pointer() {
        // Inside Macintosh Volume V (1986), p. V-529: SetDefaultStartup
        // consumes a DefStartRec from A0 and GetDefaultStartup should
        // return the same in-session bytes afterward.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let set_ptr = 0x322140u32;
        let get_ptr = 0x322180u32;
        bus.write_long(set_ptr, 0xA1B2_C3D4);
        bus.write_long(get_ptr, 0xDEAD_BEEF);
        cpu.write_reg(Register::A0, set_ptr);
        cpu.write_reg(Register::D0, 0x1357_9BDF);

        let result = dispatcher.dispatch_memory(false, 0x7E, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetDefaultStartup should be handled");
        assert!(result.unwrap().is_ok(), "SetDefaultStartup should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SetDefaultStartup should preserve A7"
        );
        assert_eq!(
            bus.read_long(set_ptr),
            0xA1B2_C3D4,
            "SetDefaultStartup should not mutate the caller DefStartRec"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0x1357_9BDF,
            "SetDefaultStartup should not clobber D0 in the PROCEDURE path"
        );

        cpu.write_reg(Register::A0, get_ptr);
        let result = dispatcher.dispatch_memory(false, 0x7D, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "GetDefaultStartup should be handled after SetDefaultStartup"
        );
        assert!(
            result.unwrap().is_ok(),
            "GetDefaultStartup should return after SetDefaultStartup"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "GetDefaultStartup should preserve A7"
        );
        assert_eq!(
            bus.read_long(get_ptr),
            0xA1B2_C3D4,
            "GetDefaultStartup should return the record stored by SetDefaultStartup"
        );
    }

    #[test]
    fn test_hnopurge() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000);
        let result = dispatcher.dispatch_memory(false, 0x4A, &mut cpu, &mut bus);
        assert!(result.is_some(), "HNoPurge ($A04A) should be handled");
        assert!(result.unwrap().is_ok(), "HNoPurge should succeed (no-op)");
    }

    #[test]
    fn test_hget_state() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000);
        let result = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
        assert!(result.is_some(), "HGetState should be handled");
        assert!(result.unwrap().is_ok(), "HGetState should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HGetState should return 0 in D0"
        );
    }

    #[test]
    fn test_hset_state() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0x200000);
        cpu.write_reg(Register::D0, 0x40); // some state byte
        let result = dispatcher.dispatch_memory(false, 0x6A, &mut cpu, &mut bus);
        assert!(result.is_some(), "HSetState should be handled");
        assert!(result.unwrap().is_ok(), "HSetState should succeed (no-op)");
    }

    #[test]
    fn test_memorydispatch_unlockmemory_returns_notlockederr_for_unlocked_range() {
        // Inside Macintosh: Memory (1992), 3-30: UnlockMemory returns
        // notLockedErr (-623) when the specified range is not locked.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 3); // UnlockMemory selector
        cpu.write_reg(Register::A0, 0x0020_0123);
        cpu.write_reg(Register::A1, 0x180);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(result.is_some(), "MemoryDispatch should handle selector 3");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 3 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-623i32) as u32,
            "UnlockMemory should return notLockedErr (-623) for unlocked pages"
        );
    }

    #[test]
    fn test_memorydispatch_unholdmemory_returns_nothelderr_for_never_held_range() {
        // Inside Macintosh: Memory (1992), 3-25 to 3-27: UnholdMemory
        // returns notHeldErr (-621) when the requested pages were never held.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 1); // UnholdMemory selector
        cpu.write_reg(Register::A0, 0x0020_0456);
        cpu.write_reg(Register::A1, 0x200);
        let sp_pre = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_post = cpu.read_reg(Register::A7);
        assert!(result.is_some(), "MemoryDispatch should handle selector 1");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 1 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-621i32) as u32,
            "UnholdMemory should return notHeldErr (-621) for never-held pages"
        );
        assert_eq!(
            sp_pre, sp_post,
            "UnholdMemory should preserve the caller's stack pointer for never-held pages"
        );
    }

    #[test]
    fn test_memorydispatch_holdmemory_round_trip_releases_idempotently() {
        // Inside Macintosh: Memory (1992), 3-25 to 3-27: HoldMemory
        // rounds the range to whole pages, and UnholdMemory remains
        // noErr on a range that has already been held and released.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let base = bus.alloc(0x3000);
        let hold_start = base + 0x11;
        let hold_count = 0x180u32;

        cpu.write_reg(Register::D0, 0); // HoldMemory selector
        cpu.write_reg(Register::A0, hold_start);
        cpu.write_reg(Register::A1, hold_count);
        let sp_pre = cpu.read_reg(Register::A7);
        let hold_once = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            hold_once.is_some(),
            "MemoryDispatch should handle selector 0"
        );
        assert!(hold_once.unwrap().is_ok(), "HoldMemory should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HoldMemory should return noErr on a live logical-RAM range"
        );

        cpu.write_reg(Register::D0, 0); // HoldMemory selector again
        cpu.write_reg(Register::A0, hold_start);
        cpu.write_reg(Register::A1, hold_count);
        let hold_twice = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            hold_twice.is_some(),
            "MemoryDispatch should handle selector 0"
        );
        assert!(
            hold_twice.unwrap().is_ok(),
            "HoldMemory should return on repeat"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HoldMemory should remain idempotent on the same page span"
        );

        cpu.write_reg(Register::D0, 1); // UnholdMemory selector
        cpu.write_reg(Register::A0, hold_start);
        cpu.write_reg(Register::A1, hold_count);
        let unhold_once = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            unhold_once.is_some(),
            "MemoryDispatch should handle selector 1"
        );
        assert!(unhold_once.unwrap().is_ok(), "UnholdMemory should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "UnholdMemory should return noErr after releasing one hold"
        );

        cpu.write_reg(Register::D0, 1); // UnholdMemory selector again
        cpu.write_reg(Register::A0, hold_start);
        cpu.write_reg(Register::A1, hold_count);
        let unhold_twice = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            unhold_twice.is_some(),
            "MemoryDispatch should handle selector 1"
        );
        assert!(
            unhold_twice.unwrap().is_ok(),
            "UnholdMemory should return on second release"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "UnholdMemory should return noErr while the second hold is still tracked"
        );

        cpu.write_reg(Register::D0, 1); // UnholdMemory selector a third time
        cpu.write_reg(Register::A0, hold_start);
        cpu.write_reg(Register::A1, hold_count);
        let unhold_third = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            unhold_third.is_some(),
            "MemoryDispatch should handle selector 1"
        );
        assert!(unhold_third.unwrap().is_ok(), "UnholdMemory should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "UnholdMemory should remain noErr after the last tracked hold is released"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_pre,
            "HoldMemory/UnholdMemory should preserve the caller's stack pointer"
        );
    }

    #[test]
    fn test_memorydispatch_holdmemory_invalid_range_returns_noerr_and_preserves_stack() {
        // Inside Macintosh: Memory (1992), 3-26: HoldMemory returns
        // noErr on the BasiliskII-observed invalid-range path.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let invalid_start = bus.ram_size() + 0x1000;
        cpu.write_reg(Register::D0, 0); // HoldMemory selector
        cpu.write_reg(Register::A0, invalid_start);
        cpu.write_reg(Register::A1, 0x200);
        let sp_pre = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_post = cpu.read_reg(Register::A7);
        assert!(result.is_some(), "MemoryDispatch should handle selector 0");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 0 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HoldMemory should return noErr for BasiliskII invalid-range behavior"
        );
        assert_eq!(
            sp_pre, sp_post,
            "HoldMemory should preserve the caller's stack pointer for invalid ranges"
        );
    }

    #[test]
    fn test_memorydispatch_unholdmemory_invalid_range_returns_noerr_and_preserves_stack() {
        // Inside Macintosh: Memory (1992), 3-27: UnholdMemory returns
        // noErr on the BasiliskII-observed invalid-range path.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let invalid_start = bus.ram_size() + 0x0800;
        cpu.write_reg(Register::D0, 1); // UnholdMemory selector
        cpu.write_reg(Register::A0, invalid_start);
        cpu.write_reg(Register::A1, 0x300);
        let sp_pre = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_post = cpu.read_reg(Register::A7);
        assert!(result.is_some(), "MemoryDispatch should handle selector 1");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 1 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "UnholdMemory should return noErr for BasiliskII invalid-range behavior"
        );
        assert_eq!(
            sp_pre, sp_post,
            "UnholdMemory should preserve the caller's stack pointer for invalid ranges"
        );
    }

    #[test]
    fn test_memorydispatch_lockmemory_invalid_range_returns_paramerr() {
        // Inside Macintosh: Memory (1992), 3-28 and 3-29: LockMemory and
        // LockMemoryContiguous return paramErr (-50) for invalid ranges.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let invalid_start = bus.ram_size() + 0x0400;
        cpu.write_reg(Register::D0, 2); // LockMemory selector
        cpu.write_reg(Register::A0, invalid_start);
        cpu.write_reg(Register::A1, 0x500);
        let sp_pre = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_post = cpu.read_reg(Register::A7);
        assert!(result.is_some(), "MemoryDispatch should handle selector 2");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 2 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "LockMemory should return paramErr (-50) for non-RAM ranges"
        );
        assert_eq!(
            sp_pre, sp_post,
            "LockMemory should preserve the caller's stack pointer for invalid ranges"
        );
    }

    #[test]
    fn test_memorydispatch_lockmemorycontiguous_invalid_range_returns_paramerr() {
        // Inside Macintosh: Memory (1992), 3-29: LockMemoryContiguous returns
        // paramErr (-50) for an invalid parameter list.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let invalid_start = bus.ram_size() + 0x1400;
        cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
        cpu.write_reg(Register::A0, invalid_start);
        cpu.write_reg(Register::A1, 0x280);
        let sp_pre = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_post = cpu.read_reg(Register::A7);
        assert!(result.is_some(), "MemoryDispatch should handle selector 4");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 4 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "LockMemoryContiguous should return paramErr (-50) for non-RAM ranges"
        );
        assert_eq!(
            sp_pre, sp_post,
            "LockMemoryContiguous should preserve the caller's stack pointer for invalid ranges"
        );
    }

    #[test]
    fn test_memorydispatch_lockmemorycontiguous_zero_length_invalid_range_returns_paramerr() {
        // Inside Macintosh: Memory (1992), 3-29: LockMemoryContiguous returns
        // paramErr (-50) for an invalid parameter list. Zero-length ranges
        // still need a plausible logical-RAM start address.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let invalid_start = bus.ram_size() + 0x2000;
        cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
        cpu.write_reg(Register::A0, invalid_start);
        cpu.write_reg(Register::A1, 0);
        let sp_pre = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(result.is_some(), "MemoryDispatch should handle selector 4");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 4 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "LockMemoryContiguous should return paramErr (-50) for a zero-length range outside logical RAM"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_pre,
            "LockMemoryContiguous should not consume a Pascal stack frame"
        );
    }

    #[test]
    fn test_memorydispatch_unlockmemory_invalid_range_returns_paramerr() {
        // Inside Macintosh: Memory (1992), 3-30: UnlockMemory returns
        // paramErr (-50) for an invalid parameter list.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let invalid_start = bus.ram_size() + 0x2000;
        cpu.write_reg(Register::D0, 3); // UnlockMemory selector
        cpu.write_reg(Register::A0, invalid_start);
        cpu.write_reg(Register::A1, 0x180);
        let sp_pre = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_post = cpu.read_reg(Register::A7);
        assert!(result.is_some(), "MemoryDispatch should handle selector 3");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 3 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "UnlockMemory should return paramErr (-50) for non-RAM ranges"
        );
        assert_eq!(
            sp_pre, sp_post,
            "UnlockMemory should preserve the caller's stack pointer for invalid ranges"
        );
    }

    #[test]
    fn test_memorydispatch_unlockmemory_reverses_lockmemorycontiguous_on_page_rounded_range() {
        // Inside Macintosh: Memory (1992), 3-29 to 3-30: LockMemoryContiguous
        // and UnlockMemory round ranges to page boundaries, and UnlockMemory
        // undoes both LockMemory and LockMemoryContiguous.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let block_start = bus.alloc(0x3000);
        let rounded_page = (block_start + 0x0FFF) & !0x0FFF;
        let lock_start = rounded_page + 0x10;
        let lock_count = 0x20u32;
        let unlock_start = rounded_page + 0x80;
        let unlock_count = 0x30u32;

        cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
        cpu.write_reg(Register::A0, lock_start);
        cpu.write_reg(Register::A1, lock_count);
        let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            lock_result.is_some(),
            "MemoryDispatch should handle selector 4"
        );
        assert!(
            lock_result.unwrap().is_ok(),
            "LockMemoryContiguous should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "LockMemoryContiguous should return noErr"
        );

        cpu.write_reg(Register::D0, 3); // UnlockMemory selector
        cpu.write_reg(Register::A0, unlock_start);
        cpu.write_reg(Register::A1, unlock_count);
        let unlock_once = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            unlock_once.is_some(),
            "MemoryDispatch should handle selector 3"
        );
        assert!(unlock_once.unwrap().is_ok(), "UnlockMemory should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "UnlockMemory should return noErr when undoing LockMemoryContiguous for the rounded page"
        );

        cpu.write_reg(Register::D0, 3); // UnlockMemory selector again
        cpu.write_reg(Register::A0, unlock_start);
        cpu.write_reg(Register::A1, unlock_count);
        let unlock_twice = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            unlock_twice.is_some(),
            "MemoryDispatch should handle selector 3"
        );
        assert!(
            unlock_twice.unwrap().is_ok(),
            "UnlockMemory should return on second attempt"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-623i32) as u32,
            "UnlockMemory should return notLockedErr (-623) after the prior unlock released the rounded page"
        );
    }

    #[test]
    fn test_memorydispatch_getphysical_requires_locked_range() {
        // Inside Macintosh: Memory (1992), 3-32: GetPhysical requires the
        // logical range to be locked and returns notLockedErr (-623) if not.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let table = bus.alloc(16);
        let logical_start = bus.alloc(512);
        let logical_count = 128u32;
        bus.write_long(table, logical_start);
        bus.write_long(table + 4, logical_count);
        cpu.write_reg(Register::D0, 5); // GetPhysical selector
        cpu.write_reg(Register::A0, table);
        cpu.write_reg(Register::A1, 1); // request one entry
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(result.is_some(), "MemoryDispatch should handle selector 5");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 5 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-623i32) as u32,
            "GetPhysical should return notLockedErr (-623) for unlocked ranges"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "GetPhysical should return translated-entry count 0 on lock failure"
        );
    }

    #[test]
    fn test_memorydispatch_getphysical_invalid_logical_range_returns_paramerr() {
        // Inside Macintosh: Memory (1992), 3-32: GetPhysical returns
        // paramErr (-50) when asked to translate non-logical-RAM addresses.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let table = bus.alloc(16);
        let logical_start = bus.ram_size() + 0x4000;
        let logical_count = 256u32;
        bus.write_long(table, logical_start);
        bus.write_long(table + 4, logical_count);
        bus.write_long(table + 8, 0x1111_1111);
        bus.write_long(table + 12, 0x2222_2222);

        cpu.write_reg(Register::D0, 5); // GetPhysical selector
        cpu.write_reg(Register::A0, table);
        cpu.write_reg(Register::A1, 1);
        let sp_before = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_after = cpu.read_reg(Register::A7);
        assert!(result.is_some(), "MemoryDispatch should handle selector 5");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 5 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "GetPhysical should return paramErr (-50) for non-RAM logical ranges"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "GetPhysical should return translated-entry count 0 on paramErr"
        );
        assert_eq!(
            sp_before, sp_after,
            "GetPhysical should preserve the caller's stack pointer on paramErr"
        );
        assert_eq!(
            bus.read_long(table + 8),
            0x1111_1111,
            "GetPhysical should preserve translation entries on paramErr"
        );
        assert_eq!(
            bus.read_long(table + 12),
            0x2222_2222,
            "GetPhysical should preserve translation entries on paramErr"
        );
    }

    #[test]
    fn test_memorydispatch_getphysical_null_table_returns_paramerr() {
        // Inside Macintosh: Memory (1992), 3-32: GetPhysical returns paramErr
        // for an invalid parameter list (NIL translation-table pointer).
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 5); // GetPhysical selector
        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::A1, 1);
        let sp_before = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_after = cpu.read_reg(Register::A7);
        assert!(result.is_some(), "MemoryDispatch should handle selector 5");
        assert!(
            result.unwrap().is_ok(),
            "MemoryDispatch selector 5 should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "GetPhysical should return paramErr (-50) for NIL translation table"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "GetPhysical should return translated-entry count 0 for NIL translation table"
        );
        assert_eq!(
            sp_before, sp_after,
            "GetPhysical should preserve the caller's stack pointer for NIL translation table"
        );
    }

    #[test]
    fn test_memorydispatch_getphysical_fills_identity_mapping_when_locked() {
        // Inside Macintosh: Memory (1992), 3-31 to 3-32: on success,
        // GetPhysical writes translation entries and returns entry count.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let table = bus.alloc(16);
        let logical_start = bus.alloc(1024) + 37; // force non-page-aligned start
        let logical_count = 700u32;
        bus.write_long(table, logical_start);
        bus.write_long(table + 4, logical_count);

        cpu.write_reg(Register::D0, 2); // LockMemory selector
        cpu.write_reg(Register::A0, logical_start);
        cpu.write_reg(Register::A1, logical_count);
        let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            lock_result.is_some(),
            "MemoryDispatch should handle selector 2"
        );
        assert!(lock_result.unwrap().is_ok(), "LockMemory should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "LockMemory should return noErr before GetPhysical"
        );

        cpu.write_reg(Register::D0, 5); // GetPhysical selector
        cpu.write_reg(Register::A0, table);
        cpu.write_reg(Register::A1, 1);
        let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            get_result.is_some(),
            "MemoryDispatch should handle selector 5"
        );
        assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "GetPhysical should return noErr on locked range"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            1,
            "GetPhysical should report one translated physical entry"
        );
        assert_eq!(
            bus.read_long(table + 8),
            logical_start,
            "GetPhysical should emit identity physical start in entry[0]"
        );
        assert_eq!(
            bus.read_long(table + 12),
            logical_count,
            "GetPhysical should emit identity physical count in entry[0]"
        );
        assert_eq!(
            bus.read_long(table),
            logical_start,
            "GetPhysical should leave the logical start unchanged when the range fits in one physical entry"
        );
        assert_eq!(
            bus.read_long(table + 4),
            logical_count,
            "GetPhysical should leave the logical count unchanged when the range fits in one physical entry"
        );
    }

    #[test]
    fn test_memorydispatch_getphysical_entrycount_zero_returns_paramerr_and_required_entries() {
        // BasiliskII returns paramErr (-50) and the required translated-entry
        // count for a locked non-empty range queried with physicalEntryCount=0,
        // while preserving the caller's stack pointer.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let table = bus.alloc(16);
        let logical_start = bus.alloc(4096) + 99;
        let logical_count = 1536u32;
        bus.write_long(table, logical_start);
        bus.write_long(table + 4, logical_count);
        bus.write_long(table + 8, 0x1111_2222);
        bus.write_long(table + 12, 0x3333_4444);

        cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
        cpu.write_reg(Register::A0, logical_start);
        cpu.write_reg(Register::A1, logical_count);
        let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            lock_result.is_some(),
            "MemoryDispatch should handle selector 4"
        );
        assert!(
            lock_result.unwrap().is_ok(),
            "LockMemoryContiguous should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "LockMemoryContiguous should return noErr"
        );

        cpu.write_reg(Register::D0, 5); // GetPhysical selector
        cpu.write_reg(Register::A0, table);
        cpu.write_reg(Register::A1, 0); // ask only for required entry count
        let sp_pre = cpu.read_reg(Register::A7);
        let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_post = cpu.read_reg(Register::A7);
        assert!(
            get_result.is_some(),
            "MemoryDispatch should handle selector 5"
        );
        assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "GetPhysical should return paramErr (-50) for entrycount=0 query"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            1,
            "GetPhysical should report one required translated entry for entrycount=0 query"
        );
        assert_eq!(
            sp_pre, sp_post,
            "GetPhysical should preserve the caller's stack pointer for entrycount=0 query"
        );
        assert_eq!(
            bus.read_long(table + 8),
            0x1111_2222,
            "GetPhysical(entrycount=0) should preserve physical entry address"
        );
        assert_eq!(
            bus.read_long(table + 12),
            0x3333_4444,
            "GetPhysical(entrycount=0) should preserve physical entry count"
        );
    }

    #[test]
    fn test_memorydispatch_getphysical_entrycount_zero_on_locked_multpage_range_returns_two_and_preserves_table(
    ) {
        // Inside Macintosh: Memory (1992), 3-31: a zero-count GetPhysical
        // query reports the number of physical entries required for the
        // entire logical range and leaves the table untouched.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let table = bus.alloc(16);
        let logical_start = bus.alloc(8192) + 99;
        let logical_count = 5000u32;
        assert_eq!(
            super::vm_required_physical_entries(logical_start, logical_count),
            2,
            "zero-count GetPhysical should compute two required physical entries for the locked multi-page witness range"
        );
        bus.write_long(table, logical_start);
        bus.write_long(table + 4, logical_count);
        bus.write_long(table + 8, 0x5555_AAAA);
        bus.write_long(table + 12, 0x7777_BBBB);

        cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
        cpu.write_reg(Register::A0, logical_start);
        cpu.write_reg(Register::A1, logical_count);
        let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            lock_result.is_some(),
            "MemoryDispatch should handle selector 4"
        );
        assert!(
            lock_result.unwrap().is_ok(),
            "LockMemoryContiguous should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "LockMemoryContiguous should return noErr"
        );

        cpu.write_reg(Register::D0, 5); // GetPhysical selector
        cpu.write_reg(Register::A0, table);
        cpu.write_reg(Register::A1, 0); // ask only for required entry count
        let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            get_result.is_some(),
            "MemoryDispatch should handle selector 5"
        );
        assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "GetPhysical should return paramErr (-50) for entrycount=0 query"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            2,
            "GetPhysical should report two required physical entries for this locked multi-page logical range"
        );
        assert_eq!(
            bus.read_long(table + 8),
            0x5555_AAAA,
            "GetPhysical(entrycount=0) should preserve physical entry address"
        );
        assert_eq!(
            bus.read_long(table + 12),
            0x7777_BBBB,
            "GetPhysical(entrycount=0) should preserve physical entry count"
        );
    }

    #[test]
    fn test_memorydispatch_getphysical_entrycount_zero_on_empty_range_returns_paramerr_and_preserves_table(
    ) {
        // BasiliskII returns paramErr for an empty logical range queried
        // with physicalEntryCount=0 and leaves the table/A0 shape intact.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let table = bus.alloc(16);
        bus.write_long(table, 0x0010_0000);
        bus.write_long(table + 4, 0);
        bus.write_long(table + 8, 0xAAAA_BBBB);
        bus.write_long(table + 12, 0xCCCC_DDDD);

        cpu.write_reg(Register::D0, 5); // GetPhysical selector
        cpu.write_reg(Register::A0, table);
        cpu.write_reg(Register::A1, 0); // ask only for required entry count
        let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            get_result.is_some(),
            "MemoryDispatch should handle selector 5"
        );
        assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "GetPhysical should return paramErr (-50) for empty-range entrycount=0 query"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            table,
            "GetPhysical(empty range) should preserve the A0 table pointer on return"
        );
        assert_eq!(
            bus.read_long(table + 8),
            0xAAAA_BBBB,
            "GetPhysical(empty range) should preserve physical entry address"
        );
        assert_eq!(
            bus.read_long(table + 12),
            0xCCCC_DDDD,
            "GetPhysical(empty range) should preserve physical entry count"
        );
    }

    #[test]
    fn test_memorydispatch_getphysical_entrycount_zero_on_unlocked_range_returns_notlockederr() {
        // The logical range must stay locked while GetPhysical reports the
        // required physical-entry count, and the zero-count query still
        // preserves the caller's stack frame.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let table = bus.alloc(16);
        let logical_start = bus.alloc(4096) + 55;
        let logical_count = 1792u32;
        bus.write_long(table, logical_start);
        bus.write_long(table + 4, logical_count);
        bus.write_long(table + 8, 0x4444_5555);
        bus.write_long(table + 12, 0x6666_7777);

        cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
        cpu.write_reg(Register::A0, logical_start);
        cpu.write_reg(Register::A1, logical_count);
        let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            lock_result.is_some(),
            "MemoryDispatch should handle selector 4"
        );
        assert!(
            lock_result.unwrap().is_ok(),
            "LockMemoryContiguous should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "LockMemoryContiguous should return noErr"
        );

        cpu.write_reg(Register::D0, 3); // UnlockMemory selector
        cpu.write_reg(Register::A0, logical_start);
        cpu.write_reg(Register::A1, logical_count);
        let unlock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        assert!(
            unlock_result.is_some(),
            "MemoryDispatch should handle selector 3"
        );
        assert!(unlock_result.unwrap().is_ok(), "UnlockMemory should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "UnlockMemory should return noErr after releasing the range"
        );

        cpu.write_reg(Register::D0, 5); // GetPhysical selector
        cpu.write_reg(Register::A0, table);
        cpu.write_reg(Register::A1, 0); // ask only for required entry count
        let sp_pre = cpu.read_reg(Register::A7);
        let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
        let sp_post = cpu.read_reg(Register::A7);
        assert!(
            get_result.is_some(),
            "MemoryDispatch should handle selector 5"
        );
        assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-623i32) as u32,
            "GetPhysical should return notLockedErr (-623) for unlocked entrycount=0 query"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "GetPhysical should report zero translated entries for unlocked entrycount=0 query"
        );
        assert_eq!(
            sp_pre, sp_post,
            "GetPhysical should preserve the caller's stack pointer for unlocked entrycount=0 query"
        );
        assert_eq!(
            bus.read_long(table + 8),
            0x4444_5555,
            "GetPhysical(entrycount=0) should preserve physical entry address after unlock"
        );
        assert_eq!(
            bus.read_long(table + 12),
            0x6666_7777,
            "GetPhysical(entrycount=0) should preserve physical entry count after unlock"
        );
    }

    #[test]
    fn countadbs_returns_number_of_connected_adb_devices_in_d0() {
        // Inside Macintosh Volume V (1986), p. V-372; Inside Macintosh:
        // Devices (1994), p. 5-42: CountADBs returns the number of ADB
        // devices by counting ADB device-table entries.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let result = dispatcher.dispatch_memory(false, 0x77, &mut cpu, &mut bus);
        assert!(result.is_some(), "CountADBs should be handled");
        assert!(result.unwrap().is_ok(), "CountADBs should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            2,
            "HLE CountADBs should report keyboard+mouse device count"
        );
    }

    #[test]
    fn countadbs_takes_no_arguments_and_reports_no_error_codes() {
        // Inside Macintosh Volume V (1986), p. V-372; Inside Macintosh:
        // Devices (1994), p. 5-42: CountADBs has no arguments and no
        // separate error-code contract.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x77, &mut cpu, &mut bus);
        assert!(result.is_some(), "CountADBs should be handled");
        assert!(result.unwrap().is_ok(), "CountADBs should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "CountADBs should not consume stack parameters"
        );
        assert!(
            (cpu.read_reg(Register::D0) as i32) >= 0,
            "CountADBs should return only a nonnegative device-count result in D0"
        );
    }

    #[test]
    fn getindadb_valid_index_returns_positive_adb_address() {
        // Inside Macintosh Volume V (1986), p. V-373; Inside Macintosh:
        // Devices (1994), p. 5-43: valid entry indexes return the current
        // ADB address as a positive function result.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::D0, 1);
        let first = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
        assert!(first.is_some(), "GetIndADB should be handled");
        assert!(first.unwrap().is_ok(), "GetIndADB(index=1) should return");
        assert!(
            (cpu.read_reg(Register::D0) as i32) > 0,
            "GetIndADB(index=1) should return a positive address"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            2,
            "HLE GetIndADB index 1 should map to keyboard ADB address 2"
        );

        cpu.write_reg(Register::D0, 2);
        let second = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
        assert!(second.is_some(), "GetIndADB should be handled");
        assert!(second.unwrap().is_ok(), "GetIndADB(index=2) should return");
        assert!(
            (cpu.read_reg(Register::D0) as i32) > 0,
            "GetIndADB(index=2) should return a positive address"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            3,
            "HLE GetIndADB index 2 should map to mouse ADB address 3"
        );
    }

    #[test]
    fn getindadb_out_of_range_index_returns_negative_result() {
        // Inside Macintosh: Devices (1994), p. 5-43 (and IM:V V-373):
        // if GetIndADB cannot find the indexed entry, it returns a
        // negative function result.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let info = 0x320000u32;
        for i in 0..10u32 {
            bus.write_byte(info + i, 0xA5);
        }

        cpu.write_reg(Register::A0, info);
        cpu.write_reg(Register::D0, 0);
        let result = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetIndADB should be handled");
        assert!(result.unwrap().is_ok(), "GetIndADB should return");
        assert!(
            (cpu.read_reg(Register::D0) as i32) < 0,
            "Out-of-range GetIndADB index should return a negative result"
        );
    }

    #[test]
    fn getindadb_writes_adbdatablock_for_valid_index() {
        // Inside Macintosh Volume V (1986), p. V-373; Inside Macintosh:
        // Devices (1994), p. 5-43: info receives an ADBDataBlock for the
        // selected entry.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let info = 0x320100u32;
        for i in 0..10u32 {
            bus.write_byte(info + i, 0xCC);
        }

        cpu.write_reg(Register::A0, info);
        cpu.write_reg(Register::D0, 1);
        let result = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetIndADB should be handled");
        assert!(result.unwrap().is_ok(), "GetIndADB should return");
        assert_eq!(
            bus.read_byte(info),
            2,
            "GetIndADB should report the extended-keyboard handler ID"
        );
        assert_eq!(
            bus.read_byte(info + 1),
            2,
            "GetIndADB should write origADBAddr for keyboard entry"
        );
        assert_eq!(
            bus.read_long(info + 2),
            0,
            "GetIndADB should write dbServiceRtPtr field in info block"
        );
        assert_eq!(
            bus.read_long(info + 6),
            0,
            "GetIndADB should write dbDataAreaAddr field in info block"
        );
    }

    #[test]
    fn getindadb_preserves_caller_memory_beyond_adbdatablock() {
        // Per IM:V V-373: the ADBDataBlock is documented as 10 bytes
        // (devType + origADBAddr + dbServiceRtPtr + dbDataAreaAddr).
        // Pin that the HLE write loop does not clobber caller memory
        // beyond the 10-byte window (trailing-sentinel preservation).
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let info = 0x320180u32;
        for i in 0..12u32 {
            bus.write_byte(info + i, 0xFF);
        }

        cpu.write_reg(Register::A0, info);
        cpu.write_reg(Register::D0, 1);
        let result = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetIndADB should be handled");
        assert!(result.unwrap().is_ok(), "GetIndADB should return");
        assert_ne!(
            bus.read_byte(info),
            0xFF,
            "GetIndADB should overwrite the sentinel at offset 0"
        );
        assert_eq!(
            bus.read_byte(info + 10),
            0xFF,
            "GetIndADB must not clobber caller memory at offset 10"
        );
        assert_eq!(
            bus.read_byte(info + 11),
            0xFF,
            "GetIndADB must not clobber caller memory at offset 11"
        );
    }

    #[test]
    fn getadbinfo_returns_noerr_for_nominal_address() {
        // Inside Macintosh Volume V (1986), p. V-369: GetADBInfo returns
        // an OSErr result code in D0; noErr indicates successful completion.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let info = 0x320200u32;
        cpu.write_reg(Register::A0, info);
        cpu.write_reg(Register::D0, 2); // keyboard ADB address

        let result = dispatcher.dispatch_memory(false, 0x79, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetADBInfo should be handled");
        assert!(result.unwrap().is_ok(), "GetADBInfo should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "GetADBInfo nominal path should return noErr in D0"
        );
    }

    #[test]
    fn getadbinfo_writes_adbdatablock_fields_through_a0_parameter_block() {
        // Inside Macintosh Volume V (1986), p. V-369: A0 points to an
        // ADBDataBlock parameter block that GetADBInfo writes on success.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let info = 0x320240u32;
        for i in 0..12u32 {
            bus.write_byte(info + i, 0xCC);
        }

        cpu.write_reg(Register::A0, info);
        cpu.write_reg(Register::D0, 3); // mouse ADB address
        let result = dispatcher.dispatch_memory(false, 0x79, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetADBInfo should be handled");
        assert!(result.unwrap().is_ok(), "GetADBInfo should return");
        assert_eq!(
            bus.read_byte(info),
            1,
            "GetADBInfo should report the standard mouse handler ID"
        );
        assert_eq!(
            bus.read_byte(info + 1),
            3,
            "GetADBInfo should report the mouse's original address"
        );
        assert_eq!(
            bus.read_long(info + 2),
            0,
            "service-routine pointer field should be written"
        );
        assert_eq!(
            bus.read_long(info + 6),
            0,
            "data-area pointer field should be written"
        );
    }

    #[test]
    fn getadbinfo_preserves_stack_pointer_for_nominal_address() {
        // Inside Macintosh Volume V (1986), p. V-369: GetADBInfo uses
        // a register-only OS trap calling convention and should not
        // consume a Pascal argument frame.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let info = 0x320280u32;
        cpu.write_reg(Register::A0, info);
        cpu.write_reg(Register::D0, 2); // keyboard ADB address
        let sp_before = cpu.read_reg(Register::A7);

        let result = dispatcher.dispatch_memory(false, 0x79, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetADBInfo should be handled");
        assert!(result.unwrap().is_ok(), "GetADBInfo should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "GetADBInfo should preserve A7"
        );
    }

    #[test]
    fn setadbinfo_returns_noerr_for_nominal_address() {
        // Inside Macintosh Volume V (1986), p. V-369: SetADBInfo returns
        // an OSErr result code in D0; noErr indicates successful completion.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let info = 0x320280u32;
        bus.write_long(info, 0x00AA_5500);
        bus.write_long(info + 4, 0x00CC_7700);
        cpu.write_reg(Register::A0, info);
        cpu.write_reg(Register::D0, 2);

        let result = dispatcher.dispatch_memory(false, 0x7A, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetADBInfo should be handled");
        assert!(result.unwrap().is_ok(), "SetADBInfo should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetADBInfo nominal path should return noErr in D0"
        );

        cpu.write_reg(Register::A0, info + 16);
        cpu.write_reg(Register::D0, 2);
        let get_result = dispatcher.dispatch_memory(false, 0x79, &mut cpu, &mut bus);
        assert!(get_result.unwrap().is_ok(), "GetADBInfo should return");
        assert_eq!(
            bus.read_long(info + 18),
            0x00AA_5500,
            "GetADBInfo should return the installed service routine"
        );
        assert_eq!(
            bus.read_long(info + 22),
            0x00CC_7700,
            "GetADBInfo should return the installed data area"
        );
    }

    #[test]
    fn setadbinfo_uses_a0_parameter_block_and_d0_address_register_calling_convention() {
        // Inside Macintosh Volume V (1986), p. V-369: SetADBInfo takes
        // A0=ADBSetInfoBlock pointer and D0=ADB address on entry.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::D0, 0x0F);

        let result = dispatcher.dispatch_memory(false, 0x7A, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetADBInfo should be handled");
        assert!(result.unwrap().is_ok(), "SetADBInfo should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SetADBInfo should not consume stack arguments"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetADBInfo should return noErr in D0 on nominal calls"
        );
    }

    #[test]
    fn adbreinit_has_no_parameters_and_preserves_stack_pointer() {
        // Inside Macintosh Volume V (1986), p. V-367: ADBReInit has no
        // parameters.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xDEAD_BEEF);

        let result = dispatcher.dispatch_memory(false, 0x7B, &mut cpu, &mut bus);
        assert!(result.is_some(), "ADBReInit should be handled");
        assert!(result.unwrap().is_ok(), "ADBReInit should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "ADBReInit should preserve A7 because it takes no stack arguments"
        );
    }

    #[test]
    fn adbop_returns_noerr_when_command_queue_accepts_request() {
        // Inside Macintosh Volume V (1986), p. V-368: ADBOp returns
        // noErr when the command is accepted.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = 0x3202C0u32;
        cpu.write_reg(Register::A0, pb);
        cpu.write_reg(Register::D0, 0x08); // command byte

        let result = dispatcher.dispatch_memory(false, 0x7C, &mut cpu, &mut bus);
        assert!(result.is_some(), "ADBOp should be handled");
        assert!(result.unwrap().is_ok(), "ADBOp should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "ADBOp nominal path should return noErr in D0"
        );
    }

    #[test]
    fn adbop_uses_a0_parameter_block_and_d0_commandnum_without_stack_arguments() {
        // Inside Macintosh Volume V (1986), p. V-368: ADBOp uses
        // A0=parameter-block pointer and D0=commandNum on entry.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::D0, 0x0C); // Flush command byte shape

        let result = dispatcher.dispatch_memory(false, 0x7C, &mut cpu, &mut bus);
        assert!(result.is_some(), "ADBOp should be handled");
        assert!(result.unwrap().is_ok(), "ADBOp should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "ADBOp should not consume stack arguments"
        );
    }

    #[test]
    fn adbop_repeated_calls_balance_stack_no_drift() {
        // 8 successive ADBOp dispatches with varied commandNum bytes
        // (Flush + Talk-register-0..2 + Listen-register-0..3) and
        // varied ADB device addresses (1..8) preserve A7 in aggregate.
        // Per IM:V V-368, ADBOp uses only A0+D0 inputs and returns its
        // result in D0; no per-call pop discipline error can
        // accumulate across the composition.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let pb = 0x3202C0u32;
        let commands: [u32; 8] = [
            0x11, // device 1 Flush
            0x2C, // device 2 Talk r0
            0x3D, // device 3 Talk r1
            0x4E, // device 4 Talk r2
            0x58, // device 5 Listen r0
            0x69, // device 6 Listen r1
            0x7A, // device 7 Listen r2
            0x8B, // device 8 Listen r3
        ];
        for command in commands.iter() {
            cpu.write_reg(Register::A0, pb);
            cpu.write_reg(Register::D0, *command);
            let result = dispatcher.dispatch_memory(false, 0x7C, &mut cpu, &mut bus);
            assert!(result.is_some(), "ADBOp should be handled");
            assert!(result.unwrap().is_ok(), "ADBOp should return");
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "8 successive ADBOp calls should preserve A7 in aggregate"
        );
    }

    #[test]
    fn control_writes_ioresult_and_returns_noerr_in_d0() {
        // IM:Devices 1994 p. 1-77: _Control returns the OSErr result in D0
        // and uses CntrlParam.ioResult in the parameter block.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;
        // Write a non-zero value at ioResult (pb+16) to verify it gets cleared
        bus.write_word(pb + 16, 0xFFFF);
        cpu.write_reg(Register::A0, pb);
        let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
        assert!(result.is_some(), "_Control should be handled");
        assert!(result.unwrap().is_ok(), "_Control should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0, "_Control should set D0 to 0");
        assert_eq!(
            bus.read_word(pb + 16),
            0,
            "_Control should set ioResult at pb+16 to 0"
        );
    }

    #[test]
    fn control_nil_paramblock_returns_noerr_and_preserves_stack() {
        // IM:Devices 1994 p. 1-77 (assembly): _Control takes the param block
        // in A0 and only D0 is defined as affected on return.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, 0);
        let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
        assert!(result.is_some(), "_Control should be handled");
        assert!(result.unwrap().is_ok(), "_Control should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "_Control with NIL param block should still return noErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "_Control should not consume stack arguments in the A0-param-block calling convention"
        );
    }

    #[test]
    fn status_writes_ioresult_and_returns_noerr_in_d0() {
        // IM:Devices 1994 p. 1-80: _Status returns result in D0 and reports
        // driver status through CntrlParam (including ioResult).
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;
        // Write a non-zero value at ioResult (pb+16) to verify it gets cleared
        bus.write_word(pb + 16, 0xFFFF);
        cpu.write_reg(Register::A0, pb);
        let result = dispatcher.dispatch_memory(false, 0x05, &mut cpu, &mut bus);
        assert!(result.is_some(), "_Status should be handled");
        assert!(result.unwrap().is_ok(), "_Status should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0, "_Status should set D0 to 0");
        assert_eq!(
            bus.read_word(pb + 16),
            0,
            "_Status should set ioResult at pb+16 to 0"
        );
    }

    #[test]
    fn status_nil_paramblock_returns_noerr_and_preserves_stack() {
        // IM:Devices 1994 p. 1-80 (assembly): _Status uses A0 for the
        // parameter block and returns the result in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, 0);
        let result = dispatcher.dispatch_memory(false, 0x05, &mut cpu, &mut bus);
        assert!(result.is_some(), "_Status should be handled");
        assert!(result.unwrap().is_ok(), "_Status should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "_Status with NIL param block should still return noErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "_Status should not consume stack arguments in the A0-param-block calling convention"
        );
    }

    #[test]
    fn pbstatus_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        // Pre-poisons pb.ioResult at pb+16 with 0x3FFF (neither noErr nor
        // any documented OSErr), dispatches _Status with a clearly-bogus
        // ioRefNum 9999, witnesses that the trap overwrote the sentinel
        // AND that D0 == ioResult (per Device Manager dispatcher
        // convention IM:II 1985 p. II-114) AND that A7 is preserved
        // across the call (register-only OS-bit FUNCTION calling
        // convention per IM:Devices 1994 p. 1-80).
        let (mut dispatcher, mut cpu, mut bus) = setup();

        let pb = 0x300300u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x3FFFu16); // pre-poison ioResult
        bus.write_word(pb + 24, 9999u16); // bogus ioRefNum

        let sp_pre = cpu.read_reg(Register::A7);
        let result = dispatcher.dispatch_memory(false, 0x05, &mut cpu, &mut bus);
        let sp_post = cpu.read_reg(Register::A7);

        assert!(result.is_some(), "_Status should be handled");
        assert!(result.unwrap().is_ok(), "_Status should succeed");

        let d0 = cpu.read_reg(Register::D0) as i16;
        let io_result = bus.read_word(pb + 16) as i16;
        assert_ne!(
            io_result, 0x3FFFi16,
            "ioResult sentinel must be overwritten"
        );
        assert_eq!(d0, io_result, "D0 == ioResult per dispatcher convention");
        assert_eq!(
            d0,
            (-21i16),
            "bogus ioRefNum should collapse to badUnitErr on the HLE path"
        );
        assert_eq!(sp_pre, sp_post, "A7 preserved (register-only ABI)");
    }

    #[test]
    fn setapplimit_updates_appllimit_when_limit_is_not_below_heap_extent() {
        // Inside Macintosh: Memory (1992), pp. 2-84..2-85:
        // SetApplLimit sets the current application heap limit to zoneLimit.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let heap_end = 0x0030_0000u32;
        let appl_limit_before = 0x0038_0000u32;
        let requested_limit = 0x003C_0000u32;
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(0x0114, heap_end); // HEAP_END
        bus.write_long(0x0130, appl_limit_before); // APPL_LIMIT

        cpu.write_reg(Register::A0, requested_limit);
        let result = dispatcher.dispatch_memory(false, 0x2D, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetApplLimit should be handled");
        assert!(result.unwrap().is_ok(), "SetApplLimit should return");
        assert_eq!(
            bus.read_long(0x0130),
            requested_limit,
            "SetApplLimit should store zoneLimit in ApplLimit when zoneLimit is not below current heap extent"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetApplLimit should return noErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SetApplLimit uses A0 parameter passing and should not consume stack arguments"
        );
    }

    #[test]
    fn setapplimit_below_heap_extent_does_not_cut_back_appllimit() {
        // Inside Macintosh: Memory (1992), p. 2-85:
        // if the zone already extends beyond zoneLimit, no cut-back occurs.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let heap_end = 0x0038_0000u32;
        let appl_limit_before = 0x0038_0000u32;
        let requested_limit = 0x0030_0000u32;
        bus.write_long(0x0114, heap_end); // HEAP_END
        bus.write_long(0x0130, appl_limit_before); // APPL_LIMIT

        cpu.write_reg(Register::A0, requested_limit);
        let result = dispatcher.dispatch_memory(false, 0x2D, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetApplLimit should be handled");
        assert!(result.unwrap().is_ok(), "SetApplLimit should return");
        assert_eq!(
            bus.read_long(0x0130),
            appl_limit_before,
            "SetApplLimit must not reduce ApplLimit when heap already extends beyond requested limit"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetApplLimit should still return noErr when lower limit is ignored"
        );
    }

    #[test]
    fn setapplimit_below_heap_extent_leaves_heapend_and_appllimit_unchanged() {
        // Lower-limit branch: a requested limit below HeapEnd must leave
        // both HeapEnd and ApplLimit unchanged while still returning noErr.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let heap_end = 0x0038_0000u32;
        let appl_limit_before = 0x003C_0000u32;
        let requested_limit = heap_end - 0x1000;
        bus.write_long(0x0114, heap_end); // HEAP_END
        bus.write_long(0x0130, appl_limit_before); // APPL_LIMIT

        cpu.write_reg(Register::A0, requested_limit);
        let result = dispatcher.dispatch_memory(false, 0x2D, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetApplLimit should be handled");
        assert!(result.unwrap().is_ok(), "SetApplLimit should return");
        assert_eq!(
            bus.read_long(0x0130),
            appl_limit_before,
            "SetApplLimit must not reduce ApplLimit when the requested limit is below HeapEnd"
        );
        assert_eq!(
            bus.read_long(0x0114),
            heap_end,
            "SetApplLimit should not rewrite HeapEnd"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetApplLimit should return noErr when the lower-limit path is taken"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "SetApplLimit should preserve A7"
        );
    }

    #[test]
    fn setzone_writes_thezone_and_getzone_roundtrips() {
        // Inside Macintosh: Memory (1992), pp. 2-80..2-81:
        // SetZone makes hz current; GetZone returns the current zone.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let new_zone = 0x0BAD_F00Du32;
        bus.write_long(0x0118, 0x00AA_BBCC);

        cpu.write_reg(Register::A0, new_zone);
        let set_result = dispatcher.dispatch_memory(false, 0x1B, &mut cpu, &mut bus);
        assert!(set_result.is_some(), "SetZone should be handled");
        assert!(set_result.unwrap().is_ok(), "SetZone should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetZone should return noErr in D0"
        );
        assert_eq!(
            bus.read_long(0x0118),
            new_zone,
            "SetZone should write TheZone low-memory global ($0118)"
        );

        let get_result = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
        assert!(get_result.is_some(), "GetZone should be handled");
        assert!(get_result.unwrap().is_ok(), "GetZone should return");
        assert_eq!(
            cpu.read_reg(Register::A0),
            new_zone,
            "GetZone should return the zone selected by SetZone"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "GetZone should return noErr in D0"
        );
    }

    #[test]
    fn setzone_roundtrip_with_saved_original_restores_thezone() {
        // Inside Macintosh: Memory (1992), pp. 2-80..2-81:
        // GetZone reads TheZone; SetZone writes A0 to TheZone.
        // Save/test/restore pattern: save the original TheZone via
        // GetZone, switch to a
        // different zone via SetZone, witness both the TheZone write
        // and the GetZone roundtrip on the new value, then restore
        // the original via SetZone and confirm GetZone observes it.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let original_zone = 0x00AA_BBCCu32;
        let new_zone = 0x0BAD_F00Du32;
        bus.write_long(0x0118, original_zone);

        // Save original via GetZone.
        let get_orig = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
        assert!(get_orig.is_some(), "GetZone should be handled");
        assert!(get_orig.unwrap().is_ok(), "GetZone should return");
        let saved_zone = cpu.read_reg(Register::A0);
        assert_eq!(
            saved_zone, original_zone,
            "GetZone should return the original TheZone before any switch"
        );

        // SetZone(new_zone) — writes TheZone.
        cpu.write_reg(Register::A0, new_zone);
        let set_new = dispatcher.dispatch_memory(false, 0x1B, &mut cpu, &mut bus);
        assert!(set_new.is_some(), "SetZone should be handled");
        assert!(set_new.unwrap().is_ok(), "SetZone should return");
        assert_eq!(
            bus.read_long(0x0118),
            new_zone,
            "SetZone should write the new zone pointer into TheZone ($0118)"
        );

        // GetZone after SetZone(new_zone) returns new_zone.
        let get_new = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
        assert!(get_new.is_some(), "GetZone should be handled");
        assert!(get_new.unwrap().is_ok(), "GetZone should return");
        assert_eq!(
            cpu.read_reg(Register::A0),
            new_zone,
            "GetZone should return the zone selected by SetZone (roundtrip)"
        );

        // Restore: SetZone(saved_zone).
        cpu.write_reg(Register::A0, saved_zone);
        let set_back = dispatcher.dispatch_memory(false, 0x1B, &mut cpu, &mut bus);
        assert!(set_back.is_some(), "SetZone should be handled");
        assert!(set_back.unwrap().is_ok(), "SetZone should return");
        assert_eq!(
            bus.read_long(0x0118),
            original_zone,
            "SetZone should restore the original TheZone value"
        );

        // GetZone after restore returns the original.
        let get_restored = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
        assert!(get_restored.is_some(), "GetZone should be handled");
        assert!(get_restored.unwrap().is_ok(), "GetZone should return");
        assert_eq!(
            cpu.read_reg(Register::A0),
            original_zone,
            "GetZone after restore should return the original TheZone"
        );
    }

    #[test]
    fn setgrowzone_register_only_calling_convention_preserves_stack() {
        // Per IM:Memory 1992 p. 2-56 SetGrowZone is an OS-bit PROCEDURE
        // with a register-only ABI (A0 input, D0 result, no Pascal
        // stack frame). The test pins:
        //   - Single SetGrowZone(NIL) preserves A7 and returns D0=noErr
        //   - 5-call composition mixing NIL → synthetic ProcPtr → NIL →
        //     synthetic ProcPtr → NIL preserves A7 cumulatively
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_pre = cpu.read_reg(Register::A7);

        // Single-call: SetGrowZone(NIL)
        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);
        let single = dispatcher.dispatch_memory(false, 0x4B, &mut cpu, &mut bus);
        assert!(single.is_some(), "SetGrowZone should be handled");
        assert!(single.unwrap().is_ok(), "SetGrowZone should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_pre,
            "A7 preserved across single SetGrowZone(NIL) call"
        );
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "SetGrowZone returns noErr in D0"
        );

        // 5-call composition: NIL → synthetic ProcPtr → NIL → ...
        let inputs = [0u32, 0x0040_0000, 0, 0x0040_0040, 0];
        for &a0 in &inputs {
            cpu.write_reg(Register::A0, a0);
            cpu.write_reg(Register::D0, 0xCAFE_F00D);
            let r = dispatcher.dispatch_memory(false, 0x4B, &mut cpu, &mut bus);
            assert!(r.is_some(), "SetGrowZone should be handled");
            assert!(
                r.unwrap().is_ok(),
                "SetGrowZone should succeed for A0=${a0:08X}"
            );
            assert_eq!(
                cpu.read_reg(Register::D0) as i32,
                0,
                "SetGrowZone returns noErr in D0 on each call"
            );
        }

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_pre,
            "A7 preserved across 5-call composition with mixed inputs"
        );
    }

    #[test]
    fn purgemem_register_only_calling_convention_preserves_stack() {
        // Per IM:Memory 1992 p. 2-73 PurgeMem is an OS-bit PROCEDURE
        // with a register-only ABI (D0 = cbNeeded input, D0 = result
        // code output, no Pascal stack frame). The test pins:
        //   - Single PurgeMem(0) preserves A7 and returns D0=noErr
        //   - 5-call composition cycling cbNeeded values
        //     0 -> 256 -> 0 -> 1024 -> 0 preserves A7 cumulatively
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_pre = cpu.read_reg(Register::A7);

        // Single-call: PurgeMem(0)
        cpu.write_reg(Register::D0, 0);
        let single = dispatcher.dispatch_memory(false, 0x4D, &mut cpu, &mut bus);
        assert!(single.is_some(), "PurgeMem should be handled");
        assert!(single.unwrap().is_ok(), "PurgeMem should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_pre,
            "A7 preserved across single PurgeMem(0) call"
        );
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "PurgeMem returns noErr in D0"
        );

        // 5-call composition: cbNeeded = 0 -> 256 -> 0 -> 1024 -> 0
        let inputs = [0u32, 256, 0, 1024, 0];
        for &d0 in &inputs {
            cpu.write_reg(Register::D0, d0);
            let r = dispatcher.dispatch_memory(false, 0x4D, &mut cpu, &mut bus);
            assert!(r.is_some(), "PurgeMem should be handled");
            assert!(
                r.unwrap().is_ok(),
                "PurgeMem should succeed for D0=${d0:08X}"
            );
            assert_eq!(
                cpu.read_reg(Register::D0) as i32,
                0,
                "PurgeMem returns noErr in D0 on each call"
            );
        }

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_pre,
            "A7 preserved across 5-call composition with mixed cbNeeded inputs"
        );
    }

    #[test]
    fn getzone_returns_thezone_pointer_and_noerr() {
        // Inside Macintosh: Memory (1992), p. 2-80:
        // GetZone returns TheZone in A0 and a result code in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let zone = 0x1234_5678u32;
        bus.write_long(0x0118, zone);

        let result = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetZone should be handled");
        assert!(result.unwrap().is_ok(), "GetZone should return");
        assert_eq!(
            cpu.read_reg(Register::A0),
            zone,
            "GetZone should return TheZone in A0"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "GetZone should return noErr in D0"
        );
    }

    #[test]
    fn handlezone_valid_handle_returns_current_zone_pointer() {
        // Inside Macintosh: Memory (1992), pp. 2-82..2-83:
        // HandleZone returns zone pointer for relocatable block handles.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let zone = 0x2001_1000u32;
        bus.write_long(0x0118, zone);

        cpu.write_reg(Register::D0, 4);
        let new_handle = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(new_handle.is_some(), "NewHandle should be handled");
        assert!(new_handle.unwrap().is_ok(), "NewHandle should return");
        let handle = cpu.read_reg(Register::A0);
        assert_ne!(handle, 0, "NewHandle should allocate a non-NIL handle");

        cpu.write_reg(Register::A0, handle);
        let result = dispatcher.dispatch_memory(false, 0x26, &mut cpu, &mut bus);
        assert!(result.is_some(), "HandleZone should be handled");
        assert!(result.unwrap().is_ok(), "HandleZone should return");
        assert_eq!(
            cpu.read_reg(Register::A0),
            zone,
            "HandleZone should return current zone pointer in A0"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HandleZone should return noErr for a valid handle"
        );
    }

    #[test]
    fn handlezone_empty_handle_returns_zone_pointer() {
        // Inside Macintosh: Memory (1992), p. 2-82 important note:
        // empty handles still return a zone pointer (master-pointer zone).
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let zone = 0x2002_2000u32;
        bus.write_long(0x0118, zone);

        let empty_result = dispatcher.dispatch_memory(false, 0x66, &mut cpu, &mut bus);
        assert!(empty_result.is_some(), "NewEmptyHandle should be handled");
        assert!(
            empty_result.unwrap().is_ok(),
            "NewEmptyHandle should return"
        );
        let empty_handle = cpu.read_reg(Register::A0);
        assert_ne!(empty_handle, 0, "NewEmptyHandle should return a handle");
        assert_eq!(
            bus.read_long(empty_handle),
            0,
            "NewEmptyHandle should initialize master pointer to NIL"
        );

        cpu.write_reg(Register::A0, empty_handle);
        let result = dispatcher.dispatch_memory(false, 0x26, &mut cpu, &mut bus);
        assert!(result.is_some(), "HandleZone should be handled");
        assert!(result.unwrap().is_ok(), "HandleZone should return");
        assert_eq!(
            cpu.read_reg(Register::A0),
            zone,
            "HandleZone(empty-handle) should return a zone pointer"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HandleZone(empty-handle) should return noErr"
        );
    }

    #[test]
    fn handlezone_nil_handle_returns_noerr_in_d0() {
        // BasiliskII-observed divergence from Inside Macintosh: Memory
        // (1992), p. 2-83: NIL handles return noErr in D0.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0);
        let result = dispatcher.dispatch_memory(false, 0x26, &mut cpu, &mut bus);
        assert!(result.is_some(), "HandleZone should be handled");
        assert!(result.unwrap().is_ok(), "HandleZone should return");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "HandleZone(NIL) should return noErr (BasiliskII divergence)"
        );
    }

    #[test]
    fn handlezone_disposed_handle_returns_memwzerr() {
        // Inside Macintosh: Memory (1992), p. 2-83:
        // memWZErr (-111) indicates attempt to operate on a free block.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 4);
        let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewHandle should be handled");
        assert!(result.unwrap().is_ok(), "NewHandle should return");
        let handle = cpu.read_reg(Register::A0);
        assert_ne!(handle, 0, "NewHandle should allocate a handle");

        cpu.write_reg(Register::A0, handle);
        let dispose = dispatcher.dispatch_memory(false, 0x23, &mut cpu, &mut bus);
        assert!(dispose.is_some(), "DisposeHandle should be handled");
        assert!(dispose.unwrap().is_ok(), "DisposeHandle should return");
        assert_eq!(
            bus.get_alloc_size(handle),
            None,
            "DisposeHandle should free the master-pointer slot"
        );

        cpu.write_reg(Register::A0, handle);
        let result = dispatcher.dispatch_memory(false, 0x26, &mut cpu, &mut bus);
        assert!(result.is_some(), "HandleZone should be handled");
        assert!(result.unwrap().is_ok(), "HandleZone should return");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -111,
            "HandleZone(disposed handle) should return memWZErr (-111)"
        );
    }

    #[test]
    fn ptrzone_valid_pointer_returns_current_zone_pointer() {
        // Inside Macintosh: Memory (1992), p. 2-83:
        // PtrZone returns the zone containing a nonrelocatable block pointer.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let zone = 0x2003_3000u32;
        bus.write_long(0x0118, zone);

        cpu.write_reg(Register::D0, 8);
        let ptr_result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
        assert!(ptr_result.is_some(), "NewPtr should be handled");
        assert!(ptr_result.unwrap().is_ok(), "NewPtr should return");
        let ptr = cpu.read_reg(Register::A0);
        assert_ne!(ptr, 0, "NewPtr should return a non-NIL pointer");

        cpu.write_reg(Register::A0, ptr);
        let result = dispatcher.dispatch_memory(false, 0x48, &mut cpu, &mut bus);
        assert!(result.is_some(), "PtrZone should be handled");
        assert!(result.unwrap().is_ok(), "PtrZone should return");
        assert_eq!(
            cpu.read_reg(Register::A0),
            zone,
            "PtrZone should return current zone pointer in A0"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "PtrZone should return noErr for a valid pointer"
        );
    }

    #[test]
    fn ptrzone_nil_pointer_returns_memwzerr() {
        // Inside Macintosh: Memory (1992), p. 2-83:
        // PtrZone returns memWZErr for free/invalid pointer blocks.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0);
        let result = dispatcher.dispatch_memory(false, 0x48, &mut cpu, &mut bus);
        assert!(result.is_some(), "PtrZone should be handled");
        assert!(result.unwrap().is_ok(), "PtrZone should return");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -111,
            "PtrZone(NIL) should return memWZErr (-111)"
        );
    }

    // ==================== Toolbox Traps (is_tool=true) ====================

    #[test]
    fn debugstr_consumes_message_pointer_and_preserves_registers() {
        // Universal Interfaces 3.4 MacTypes.h declares
        // DebugStr(ConstStr255Param) as ONEWORDINLINE($ABFF).
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0x0012_3400);
        cpu.write_reg(Register::D0, 0x1234_5678);
        cpu.write_reg(Register::A0, 0x00AA_5500);
        let result = dispatcher.dispatch_memory(true, 0x3FF, &mut cpu, &mut bus);
        assert!(result.is_some(), "DebugStr should be handled");
        assert!(result.unwrap().is_ok(), "DebugStr should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before + 4,
            "_DebugStr should consume its Pascal-string pointer"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0x1234_5678,
            "_DebugStr no-op path should preserve D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0x00AA_5500,
            "_DebugStr no-op path should preserve A0"
        );
    }

    #[test]
    fn repeated_debugstr_calls_consume_one_message_pointer_each() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        for index in 0..5 {
            let sp = cpu.read_reg(Register::A7);
            bus.write_long(sp, 0x0012_3400 + index);
            dispatcher
                .dispatch_memory(true, 0x3FF, &mut cpu, &mut bus)
                .expect("DebugStr should be handled")
                .expect("DebugStr should return");
        }
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 20);
    }

    #[test]
    fn test_get_tool_trap_address_tool() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xAA55_AA55);

        cpu.write_reg(Register::D0, 0x0234);
        let result = dispatcher.dispatch_memory(true, 0x346, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "GetToolTrapAddress (tool) should be handled"
        );
        assert!(
            result.unwrap().is_ok(),
            "GetToolTrapAddress (tool) should succeed"
        );
        let addr = cpu.read_reg(Register::A0);
        assert_eq!(
            addr,
            0xCAFE0000 + 0x0234,
            "GetToolTrapAddress (tool) should return 0xCAFE0000 + (trap & 0x3FF) in A0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "GetToolTrapAddress (tool) should preserve A7"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0xAA55_AA55,
            "GetToolTrapAddress (tool) should leave caller stack memory untouched"
        );
    }

    #[test]
    fn gettooltrapaddress_trap_word_variant_returns_callable_trampoline() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0x1234_5678);

        // Simulate the real trap-instruction variant ($A746) so
        // dispatch_memory(false, 0x46, ..) takes the A746 branch.
        dispatcher.current_trap_word = 0xA746;
        cpu.write_reg(Register::D0, 0x00EC); // CopyBits tool-trap number

        let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "GetToolTrapAddress variant should be handled"
        );
        assert!(
            result.unwrap().is_ok(),
            "GetToolTrapAddress variant should succeed"
        );

        let addr = cpu.read_reg(Register::A0);
        assert_ne!(addr, 0, "tool-trap trampoline address should be nonzero");
        assert!(
            !(0xCAFE0000..=0xCAFE03FF).contains(&addr),
            "A746 variant must return a callable trampoline, not a CAFE fake-ptr"
        );
        assert_eq!(
            bus.read_word(addr),
            0xACEC,
            "trampoline must encode auto-pop canonical trap word for CopyBits"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "GetToolTrapAddress variant should preserve A7"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0x1234_5678,
            "GetToolTrapAddress variant should leave caller stack memory untouched"
        );
    }

    #[test]
    fn test_hand_to_hand() {
        // Inside Macintosh: Memory (1992), p. 2-62:
        // HandToHand returns a new handle to copied data in A0/theHndl.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        // Create a source handle with data
        cpu.write_reg(Register::D0, 8);
        let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let src_handle = cpu.read_reg(Register::A0);
        let src_ptr = bus.read_long(src_handle);
        bus.write_long(src_ptr, 0xDEADBEEF);
        bus.write_long(src_ptr + 4, 0xCAFEBABE);
        // Call HandToHand (0x1E1)
        cpu.write_reg(Register::A0, src_handle);
        let result = dispatcher.dispatch_memory(true, 0x1E1, &mut cpu, &mut bus);
        assert!(result.is_some(), "HandToHand should be handled");
        assert!(result.unwrap().is_ok(), "HandToHand should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HandToHand should return noErr"
        );
        let new_handle = cpu.read_reg(Register::A0);
        assert_ne!(
            new_handle, src_handle,
            "HandToHand should return a different handle"
        );
        let new_ptr = bus.read_long(new_handle);
        assert_ne!(
            new_ptr, src_ptr,
            "HandToHand should allocate new data block"
        );
        assert_eq!(
            bus.read_long(new_ptr),
            0xDEADBEEF,
            "HandToHand should copy data"
        );
        assert_eq!(
            bus.read_long(new_ptr + 4),
            0xCAFEBABE,
            "HandToHand should copy data"
        );
    }

    #[test]
    fn test_hand_to_hand_nil_handle_returns_nilhandleerr() {
        // Inside Macintosh: Memory (1992), p. 2-63:
        // nilHandleErr (-109) for NIL master pointer.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::A0, 0);
        let result = dispatcher.dispatch_memory(true, 0x1E1, &mut cpu, &mut bus);
        assert!(result.is_some(), "HandToHand should be handled");
        assert!(result.unwrap().is_ok(), "HandToHand should return");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -109,
            "HandToHand should return nilHandleErr (-109) for NIL handle"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "HandToHand should leave A0 as NIL on nilHandleErr"
        );
    }

    #[test]
    fn test_ptr_to_hand() {
        // Inside Macintosh: Memory (1992), p. 2-60:
        // PtrToHand returns a newly created handle whose bytes match srcPtr.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let src = 0x300000u32;
        let data: [u8; 6] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        for (i, &b) in data.iter().enumerate() {
            bus.write_byte(src + i as u32, b);
        }
        cpu.write_reg(Register::A0, src);
        cpu.write_reg(Register::D0, data.len() as u32);
        let result = dispatcher.dispatch_memory(true, 0x1E3, &mut cpu, &mut bus);
        assert!(result.is_some(), "PtrToHand should be handled");
        assert!(result.unwrap().is_ok(), "PtrToHand should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "PtrToHand should set D0 to 0 (noErr)"
        );
        let handle = cpu.read_reg(Register::A0);
        assert!(
            handle >= 0x200000,
            "PtrToHand should return a valid handle in A0, got ${:08X}",
            handle
        );
        let ptr = bus.read_long(handle);
        assert!(
            ptr >= 0x200000,
            "PtrToHand handle should point to a valid pointer, got ${:08X}",
            ptr
        );
        for (i, &b) in data.iter().enumerate() {
            assert_eq!(
                bus.read_byte(ptr + i as u32),
                b,
                "PtrToHand should copy byte {} correctly (expected 0x{:02X})",
                i,
                b
            );
        }
    }

    #[test]
    fn test_ptr_to_xhand_copies_bytes_and_returns_destination_handle() {
        // Inside Macintosh: Memory (1992), pp. 2-61..2-62:
        // PtrToXHand copies into an existing handle and returns dstHndl in A0.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        let src = 0x310000u32;
        let src_data: [u8; 4] = [0x41, 0x42, 0x43, 0x44];
        for (i, &b) in src_data.iter().enumerate() {
            bus.write_byte(src + i as u32, b);
        }

        cpu.write_reg(Register::D0, 3);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let dst_handle = cpu.read_reg(Register::A0);

        let dst_ptr_before = bus.read_long(dst_handle);
        for i in 0..3 {
            bus.write_byte(dst_ptr_before + i as u32, 0x7A);
        }

        cpu.write_reg(Register::A0, src);
        cpu.write_reg(Register::A1, dst_handle);
        cpu.write_reg(Register::D0, src_data.len() as u32);
        let result = dispatcher.dispatch_memory(true, 0x1E2, &mut cpu, &mut bus);
        assert!(result.is_some(), "PtrToXHand should be handled");
        assert!(result.unwrap().is_ok(), "PtrToXHand should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "PtrToXHand should return noErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            dst_handle,
            "PtrToXHand should return destination handle in A0"
        );

        let dst_ptr_after = bus.read_long(dst_handle);
        assert_eq!(
            bus.get_alloc_size(dst_ptr_after),
            Some(src_data.len() as u32),
            "PtrToXHand should update the handle's logical size to the requested byte count"
        );
        for (i, &b) in src_data.iter().enumerate() {
            assert_eq!(
                bus.read_byte(dst_ptr_after + i as u32),
                b,
                "PtrToXHand should copy source byte {}",
                i
            );
        }
    }

    #[test]
    fn test_ptr_to_xhand_same_bucket_shrink_updates_logical_size() {
        // Inside Macintosh: Memory (1992), p. 2-61:
        // PtrToXHand makes dstHndl a handle to a copy of size bytes
        // beginning at srcPtr, even when the new logical size stays in
        // the same aligned heap bucket.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let src = 0x320000u32;
        bus.write_byte(src, 0x5A);

        cpu.write_reg(Register::D0, 4);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let dst_handle = cpu.read_reg(Register::A0);
        let dst_ptr_before = bus.read_long(dst_handle);
        for i in 0..4 {
            bus.write_byte(dst_ptr_before + i as u32, 0x71 + i as u8);
        }

        cpu.write_reg(Register::A0, src);
        cpu.write_reg(Register::A1, dst_handle);
        cpu.write_reg(Register::D0, 1);
        let result = dispatcher.dispatch_memory(true, 0x1E2, &mut cpu, &mut bus);
        assert!(result.is_some(), "PtrToXHand should be handled");
        assert!(result.unwrap().is_ok(), "PtrToXHand should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "PtrToXHand should return noErr"
        );

        let dst_ptr_after = bus.read_long(dst_handle);
        assert_eq!(
            bus.get_alloc_size(dst_ptr_after),
            Some(1),
            "PtrToXHand should shrink the handle's logical size inside the same aligned bucket"
        );
        assert_eq!(
            bus.read_byte(dst_ptr_after),
            0x5A,
            "PtrToXHand should preserve copied byte 0 when shrinking the destination"
        );
    }

    #[test]
    fn test_ptr_to_xhand_nil_destination_returns_nilhandleerr() {
        // Inside Macintosh: Memory (1992), p. 2-62:
        // nilHandleErr (-109) for NIL destination handle.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let src = 0x320000u32;
        bus.write_byte(src, 0x55);
        cpu.write_reg(Register::A0, src);
        cpu.write_reg(Register::A1, 0);
        cpu.write_reg(Register::D0, 1);
        let result = dispatcher.dispatch_memory(true, 0x1E2, &mut cpu, &mut bus);
        assert!(result.is_some(), "PtrToXHand should be handled");
        assert!(result.unwrap().is_ok(), "PtrToXHand should return");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -109,
            "PtrToXHand should return nilHandleErr (-109) for NIL destination"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "PtrToXHand should return NIL destination handle in A0"
        );
    }

    #[test]
    fn test_hand_and_hand_appends_source_to_destination_and_returns_destination_handle() {
        // Inside Macintosh: Memory (1992), pp. 2-64..2-65:
        // HandAndHand appends aHndl to bHndl, leaves aHndl unchanged, returns bHndl in A0.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, 2);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let src_handle = cpu.read_reg(Register::A0);
        let src_ptr = bus.read_long(src_handle);
        bus.write_byte(src_ptr, b'A');
        bus.write_byte(src_ptr + 1, b'B');

        cpu.write_reg(Register::D0, 2);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let dst_handle = cpu.read_reg(Register::A0);
        let dst_ptr = bus.read_long(dst_handle);
        bus.write_byte(dst_ptr, b'C');
        bus.write_byte(dst_ptr + 1, b'D');

        cpu.write_reg(Register::A0, src_handle);
        cpu.write_reg(Register::A1, dst_handle);
        let result = dispatcher.dispatch_memory(true, 0x1E4, &mut cpu, &mut bus);
        assert!(result.is_some(), "HandAndHand should be handled");
        assert!(result.unwrap().is_ok(), "HandAndHand should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "HandAndHand should return noErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            dst_handle,
            "HandAndHand should return destination handle in A0"
        );

        let new_dst_ptr = bus.read_long(dst_handle);
        assert_eq!(bus.get_alloc_size(new_dst_ptr), Some(4));
        assert_eq!(bus.read_byte(new_dst_ptr), b'C');
        assert_eq!(bus.read_byte(new_dst_ptr + 1), b'D');
        assert_eq!(bus.read_byte(new_dst_ptr + 2), b'A');
        assert_eq!(bus.read_byte(new_dst_ptr + 3), b'B');

        assert_eq!(
            bus.read_byte(src_ptr),
            b'A',
            "HandAndHand should leave source handle contents unchanged"
        );
        assert_eq!(
            bus.read_byte(src_ptr + 1),
            b'B',
            "HandAndHand should leave source handle contents unchanged"
        );
    }

    #[test]
    fn test_hand_and_hand_nil_source_returns_nilhandleerr() {
        // Inside Macintosh: Memory (1992), p. 2-65:
        // nilHandleErr (-109) for NIL master pointer.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 1);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let dst_handle = cpu.read_reg(Register::A0);

        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::A1, dst_handle);
        let result = dispatcher.dispatch_memory(true, 0x1E4, &mut cpu, &mut bus);
        assert!(result.is_some(), "HandAndHand should be handled");
        assert!(result.unwrap().is_ok(), "HandAndHand should return");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -109,
            "HandAndHand should return nilHandleErr (-109) when source handle is NIL"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            dst_handle,
            "HandAndHand should still return destination handle in A0"
        );
    }

    #[test]
    fn test_ptr_and_hand_appends_pointer_data_and_returns_destination_handle() {
        // Inside Macintosh: Memory (1992), pp. 2-65..2-66:
        // PtrAndHand appends bytes from pntr to hndl and returns hndl in A0.
        let (mut dispatcher, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, 3);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let dst_handle = cpu.read_reg(Register::A0);
        let dst_ptr = bus.read_long(dst_handle);
        bus.write_byte(dst_ptr, b'H');
        bus.write_byte(dst_ptr + 1, b'E');
        bus.write_byte(dst_ptr + 2, b'L');

        let src = 0x330000u32;
        bus.write_byte(src, b'L');
        bus.write_byte(src + 1, b'O');

        cpu.write_reg(Register::A0, src);
        cpu.write_reg(Register::A1, dst_handle);
        cpu.write_reg(Register::D0, 2);
        let result = dispatcher.dispatch_memory(true, 0x1EF, &mut cpu, &mut bus);
        assert!(result.is_some(), "PtrAndHand should be handled");
        assert!(result.unwrap().is_ok(), "PtrAndHand should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "PtrAndHand should return noErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            dst_handle,
            "PtrAndHand should return destination handle in A0"
        );

        let new_dst_ptr = bus.read_long(dst_handle);
        assert_eq!(bus.get_alloc_size(new_dst_ptr), Some(5));
        assert_eq!(bus.read_byte(new_dst_ptr), b'H');
        assert_eq!(bus.read_byte(new_dst_ptr + 1), b'E');
        assert_eq!(bus.read_byte(new_dst_ptr + 2), b'L');
        assert_eq!(bus.read_byte(new_dst_ptr + 3), b'L');
        assert_eq!(bus.read_byte(new_dst_ptr + 4), b'O');
    }

    #[test]
    fn test_ptr_and_hand_nil_destination_returns_nilhandleerr() {
        // Inside Macintosh: Memory (1992), p. 2-66:
        // nilHandleErr (-109) for NIL destination handle.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let src = 0x340000u32;
        bus.write_byte(src, 0xAA);
        cpu.write_reg(Register::A0, src);
        cpu.write_reg(Register::A1, 0);
        cpu.write_reg(Register::D0, 1);
        let result = dispatcher.dispatch_memory(true, 0x1EF, &mut cpu, &mut bus);
        assert!(result.is_some(), "PtrAndHand should be handled");
        assert!(result.unwrap().is_ok(), "PtrAndHand should return");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -109,
            "PtrAndHand should return nilHandleErr (-109) for NIL destination"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            0,
            "PtrAndHand should return NIL destination handle in A0"
        );
    }

    // ==================== Unhandled trap returns None ====================

    #[test]
    fn test_unhandled_trap_returns_none() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let result = dispatcher.dispatch_memory(false, 0xFFFF, &mut cpu, &mut bus);
        assert!(result.is_none(), "Unhandled trap should return None");
    }
}
