//! Memory Manager trap handlers.

use super::dispatch::{
    raw_trap_route, selector_operation_route, OsRoutineVariant, SelectorOperationRoute,
};
use super::manager::{TrapManager, TrapManagerSetError, TrapTableKind};
use crate::callback_manager::CallbackTaskArchitecture;
use crate::cpu::{CpuOps, Register};
use crate::machine_profile::{REFERENCE_M68K_EXECUTION_CAPABILITIES, REFERENCE_MACHINE_PROFILE};
use crate::memory::{globals::addr, MacMemoryBus, MemoryBus};
use crate::process_context::{
    ProcessHandleHeap, ProcessNewHandleBackend, ProcessNewHandleRequest, ProcessNewHandleResult,
};
use crate::{Error, Result};
use std::sync::OnceLock;

static TRACE_MEMORY: OnceLock<bool> = OnceLock::new();
static TRACE_VIDEO_DRIVER: OnceLock<bool> = OnceLock::new();
static TRACE_ENTROPY: OnceLock<bool> = OnceLock::new();
static TRACE_VBL: OnceLock<bool> = OnceLock::new();

const MEMORY_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_memory_dispatch_operations.rs");
const MEMORY_DISPATCH_A0_RESULT_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_memory_dispatch_a0_result_operations.rs");
const HWPRIV_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_hwpriv_operations.rs");
const IDLE_STATE_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_idle_state_operations.rs");
const SERIAL_POWER_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_serial_power_operations.rs");
const SCSI_ATOMIC_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_scsi_atomic_operations.rs");
const DEBUG_UTIL_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_debug_util_operations.rs");

fn memory_dispatch_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    let routes = match trap_word {
        0xA05C => MEMORY_DISPATCH_OPERATION_ROUTES,
        0xA15C => MEMORY_DISPATCH_A0_RESULT_OPERATION_ROUTES,
        _ => return None,
    };
    selector_operation_route(routes, selector)
}

fn hwpriv_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if !matches!(trap_word, 0xA098 | 0xA198) {
        return None;
    }
    selector_operation_route(HWPRIV_OPERATION_ROUTES, selector)
}

fn power_control_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    let routes = match trap_word {
        0xA485 => IDLE_STATE_OPERATION_ROUTES,
        0xA685 => SERIAL_POWER_OPERATION_ROUTES,
        _ => return None,
    };
    selector_operation_route(routes, selector)
}

fn scsi_atomic_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA089 {
        return None;
    }
    selector_operation_route(SCSI_ATOMIC_OPERATION_ROUTES, selector)
}

fn debug_util_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA08D {
        return None;
    }
    selector_operation_route(DEBUG_UTIL_OPERATION_ROUTES, selector)
}

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
    // guaranteed to return zero-filled memory. One bulk fill: the bus keeps
    // its per-byte path whenever a tracer, watchpoint or write probe needs
    // to observe every byte.
    bus.fill_bytes(address, size, 0xA5);
}

const NO_ERR: u32 = 0;
const CONTROL_ERR: u32 = (-17i32) as u32;
const BAD_UNIT_ERR: u32 = (-21i32) as u32;
const PARAM_ERR: u32 = (-50i32) as u32;
const MEM_FULL_ERR: u32 = (-108i32) as u32;
const NIL_HANDLE_ERR: u32 = (-109i32) as u32;
#[cfg(test)]
const MEM_WZ_ERR: u32 = (-111i32) as u32;
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

/// Translate a Trap Manager table-write failure at the 68K ABI edge.
///
/// Trap Manager owns the come-from-chain validation and reports structural
/// failures as a typed error. The classic 68K setter has no result register;
/// its system-error path publishes the failure through DSErrCode and halts
/// the HLE invocation. Inside Macintosh: Operating System Utilities (1994),
/// pp. 8-29--8-31.
fn handle_trap_manager_set_error(bus: &mut MacMemoryBus, error: TrapManagerSetError) -> Result<()> {
    let system_error = match error {
        TrapManagerSetError::InvalidComeFromHead
        | TrapManagerSetError::UnreadableTable
        | TrapManagerSetError::MalformedComeFromChain
        | TrapManagerSetError::WriteRejected => 12,
    };
    bus.write_word(addr::DS_ERR_CODE, system_error);
    Err(Error::Halted)
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
    page_counts: &crate::fast_hash::FastHashMap<u32, u16>,
    page_start: u32,
    page_end_exclusive: u32,
) -> bool {
    (page_start..page_end_exclusive).all(|page| page_counts.get(&page).copied().unwrap_or(0) > 0)
}

fn vm_increment_pages(
    page_counts: &mut crate::fast_hash::FastHashMap<u32, u16>,
    page_start: u32,
    page_end_exclusive: u32,
) {
    for page in page_start..page_end_exclusive {
        let count = page_counts.entry(page).or_insert(0);
        *count = count.saturating_add(1);
    }
}

fn vm_try_decrement_pages(
    page_counts: &mut crate::fast_hash::FastHashMap<u32, u16>,
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
    fn sync_time_task_links(&self, bus: &mut MacMemoryBus) {
        for (index, task) in self.timer_tasks.iter().enumerate() {
            let next = self
                .timer_tasks
                .get(index.saturating_add(1))
                .map(|next| next.task_ptr)
                .unwrap_or(0);
            bus.write_long(task.task_ptr, next);
        }
    }

    /// Install a video-device gamma table from a `VDGammaRecord`.
    ///
    /// Universal Interfaces 3.4 `Video.h` defines `VDGammaRecord.csGTable`
    /// as a `GammaTbl` pointer and the six-word variable-length table header.
    /// BasiliskII `video.cpp::set_gamma_table` supplies the validation and
    /// channel-layout oracle used here.
    fn install_device_gamma(&mut self, bus: &MacMemoryBus, vd_gamma_ptr: u32) -> u32 {
        if vd_gamma_ptr == 0 || vd_gamma_ptr.saturating_add(4) > bus.ram_size() {
            return PARAM_ERR;
        }
        if bus
            .get_alloc_size(vd_gamma_ptr)
            .is_some_and(|size| size < 4)
        {
            return PARAM_ERR;
        }

        let gamma_ptr = bus.read_long(vd_gamma_ptr);
        if gamma_ptr == 0 {
            self.display_gamma
                .install(crate::display::linear_display_gamma());
            return NO_ERR;
        }

        let Some(header_end) = gamma_ptr.checked_add(12) else {
            return PARAM_ERR;
        };
        if header_end > bus.ram_size() {
            return PARAM_ERR;
        }

        let version = bus.read_word(gamma_ptr);
        let gamma_type = bus.read_word(gamma_ptr + 2);
        let formula_size = u32::from(bus.read_word(gamma_ptr + 4));
        let channel_count = u32::from(bus.read_word(gamma_ptr + 6));
        let data_count = u32::from(bus.read_word(gamma_ptr + 8));
        let data_width = u32::from(bus.read_word(gamma_ptr + 10));

        if version != 0
            || gamma_type != 0
            || !matches!(channel_count, 1 | 3)
            || data_width > 8
            || data_count != (1u32 << data_width)
        {
            return PARAM_ERR;
        }

        let Some(data_base) = header_end.checked_add(formula_size) else {
            return PARAM_ERR;
        };
        let Some(data_len) = channel_count.checked_mul(data_count) else {
            return PARAM_ERR;
        };
        let Some(table_end) = data_base.checked_add(data_len) else {
            return PARAM_ERR;
        };
        if table_end > bus.ram_size() {
            return PARAM_ERR;
        }
        if bus
            .get_alloc_size(gamma_ptr)
            .is_some_and(|size| table_end - gamma_ptr > size)
        {
            return PARAM_ERR;
        }

        let shift = 8 - data_width;
        let mut installed = [[0u8; 256]; 3];
        for (channel, output) in installed.iter_mut().enumerate() {
            let source_channel = if channel_count == 1 {
                0
            } else {
                channel as u32
            };
            let source_base = data_base + source_channel * data_count;
            for (input, value) in output.iter_mut().enumerate() {
                let source_index = (input as u32) >> shift;
                *value = bus.read_byte(source_base + source_index);
            }
        }
        self.display_gamma.install(installed);
        NO_ERR
    }

    fn sync_vbl_links(&mut self, bus: &mut MacMemoryBus) {
        if self.callback_scheduling.system_vbl_queue_anchor() == 0 {
            let anchor = bus.alloc_synthetic(14);
            if anchor != 0 {
                bus.write_word(anchor + 4, 1); // vType
                self.callback_scheduling
                    .set_system_vbl_queue_anchor(anchor);
            }
        }

        for task in self.vbl_tasks.iter() {
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
        let anchor = self.callback_scheduling.system_vbl_queue_anchor();
        if anchor != 0 {
            bus.write_long(anchor, first_task);
        }
        let head = if anchor != 0 { anchor } else { first_task };
        let tail =
            system_tasks.last().unwrap_or_else(
                || {
                    if first_task != 0 {
                        first_task
                    } else {
                        anchor
                    }
                },
            );
        // QHdr layout: qFlags WORD, qHead QElemPtr, qTail QElemPtr.
        bus.write_long(VBL_QUEUE_HEADER + 2, head);
        bus.write_long(VBL_QUEUE_HEADER + 6, tail);
    }

    fn trap_address_table_key(&self, trap_word: u16) -> u16 {
        let kind = match raw_trap_route(self.current_trap_word).os_routine_variant {
            // `_GetTrapAddress newTool` / `_SetTrapAddress newTool`.
            // Inside Macintosh: Operating System Utilities (1994),
            // pp. 8-27--8-31: trapNum may be an A-line instruction or trap
            // number; irrelevant high bits are masked to the selected table.
            OsRoutineVariant::TrapAddressNewTool => TrapTableKind::Toolbox,
            // The newOS forms mask to the low 8-bit Operating System table.
            OsRoutineVariant::TrapAddressNewOs => TrapTableKind::OperatingSystem,
            // Legacy GetTrapAddress ($A146) and SetTrapAddress ($A047)
            // ignore the high-order bits and infer the table from the trap
            // number: $00-$4F, $54, and $57 are OS traps; all others are
            // Toolbox traps. Inside Macintosh: Operating System Utilities
            // 1994, pp. 8-32 to 8-33.
            OsRoutineVariant::TrapAddressLegacy | OsRoutineVariant::Unclassified => {
                debug_assert!(matches!(
                    raw_trap_route(self.current_trap_word).os_routine_variant,
                    OsRoutineVariant::TrapAddressLegacy | OsRoutineVariant::Unclassified
                ));
                TrapTableKind::Legacy
            }
            variant => {
                unreachable!("non-Trap Manager route reached trap-address handler: {variant:?}")
            }
        };
        TrapManager::canonical_trap_word(trap_word, kind)
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

    fn remove_notification_request(&mut self, bus: &mut MacMemoryBus, nm_rec: u32) -> i16 {
        let Some(index) = self
            .notification_requests
            .iter()
            .position(|&request| request == nm_rec)
        else {
            return -1; // qErr
        };
        self.notification_requests.remove(index);
        bus.write_long(nm_rec, 0);
        if index > 0 {
            let previous = self.notification_requests[index - 1];
            let next = self
                .notification_requests
                .get(index)
                .copied()
                .unwrap_or(0);
            bus.write_long(previous, next);
        }
        0
    }

    fn arm_notification_response<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        nm_rec: u32,
        response: u32,
    ) {
        if !Self::looks_like_callable_proc(bus, response) {
            return;
        }

        let trampoline = bus.alloc(28);
        if trampoline == 0 {
            return;
        }
        let return_slot = cpu.read_reg(Register::A7).wrapping_sub(4);
        let saved_regs_sp = return_slot.wrapping_sub(32);

        // MyResponse(nmReqPtr) is a Pascal procedure. Reset A7 after the JSR
        // so either RTS or RTD #4 response procedures return safely.
        bus.write_word(trampoline, 0x48E7); // MOVEM.L D0-D3/A0-A3,-(SP)
        bus.write_word(trampoline + 2, 0xF0F0);
        bus.write_word(trampoline + 4, 0x2F3C); // MOVE.L #nmReqPtr,-(SP)
        bus.write_long(trampoline + 6, nm_rec);
        bus.write_word(trampoline + 10, 0x4EB9); // JSR abs.L
        bus.write_long(trampoline + 12, response);
        bus.write_word(trampoline + 16, 0x2E7C); // MOVEA.L #savedRegsSP,A7
        bus.write_long(trampoline + 18, saved_regs_sp);
        bus.write_word(trampoline + 22, 0x4CDF); // MOVEM.L (SP)+,D0-D3/A0-A3
        bus.write_word(trampoline + 24, 0x0F0F);
        bus.write_word(trampoline + 26, 0x4E75); // RTS

        bus.write_long(return_slot, cpu.read_reg(Register::PC));
        cpu.write_reg(Register::A7, return_slot);
        cpu.write_reg(Register::PC, trampoline);
    }

    fn install_notification_request<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        nm_rec: u32,
    ) -> i16 {
        if nm_rec == 0 || bus.read_word(nm_rec + 4) != 8 {
            return -299; // nmTypErr
        }
        if self.notification_requests.contains(&nm_rec) {
            return 0;
        }

        if let Some(&tail) = self.notification_requests.last() {
            bus.write_long(tail, nm_rec);
        }
        bus.write_long(nm_rec, 0);
        self.notification_requests.push(nm_rec);

        match bus.read_long(nm_rec + 28) {
            u32::MAX => {
                self.remove_notification_request(bus, nm_rec);
            }
            0 => {}
            response => self.arm_notification_response(cpu, bus, nm_rec, response),
        }
        0
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
                self.current_tick(), task_ptr, q_type, vbl_addr, vbl_count, vbl_phase, slot
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

        self.vbl_tasks.with_mut(|vbl_tasks| {
            vbl_tasks.retain(|task| task.task_ptr != task_ptr);
            vbl_tasks.push(super::dispatch::VblTask {
                task_ptr,
                architecture: CallbackTaskArchitecture::M68k,
                slot,
                pending: false,
            });
        });
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
        self.vbl_tasks
            .with_mut(|vbl_tasks| vbl_tasks.retain(|task| task.task_ptr != task_ptr));
        bus.write_long(task_ptr, 0);
        self.sync_vbl_links(bus);
        0
    }

    pub(super) fn new_process_classic_ptr(&mut self, bus: &mut MacMemoryBus, size: u32) -> u32 {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.new_classic_ptr(bus, size)
    }

    fn new_process_classic_ptr_below(
        &mut self,
        bus: &mut MacMemoryBus,
        size: u32,
        upper_bound: u32,
    ) -> u32 {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.new_classic_ptr_below(bus, size, upper_bound)
    }

    pub(super) fn dispose_process_ptr(&mut self, bus: &mut MacMemoryBus, ptr: u32) {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.dispose_process_ptr(bus, ptr);
    }

    pub(super) fn new_process_classic_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        size: u32,
    ) -> std::result::Result<(u32, u32), u32> {
        let Some(request) =
            ProcessNewHandleRequest::from_unsigned(size, false, ProcessHandleHeap::Current)
        else {
            return Err(MEM_FULL_ERR);
        };
        let result = self.new_process_handle(bus, request);
        if result.error == 0 {
            Ok((result.handle, result.data_ptr))
        } else {
            Err(result.error as i32 as u32)
        }
    }

    /// Decode one classic Memory Manager heap request through the process
    /// service. The trap remains responsible only for attaching its bus and
    /// later publishing the neutral result through the 68K ABI.
    pub(super) fn new_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        request: ProcessNewHandleRequest,
    ) -> ProcessNewHandleResult {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.new_handle(request, ProcessNewHandleBackend::Classic(bus))
    }

    pub(super) fn new_empty_process_classic_handle(
        &mut self,
        bus: &mut MacMemoryBus,
    ) -> std::result::Result<u32, u32> {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager
            .new_empty_classic_handle(bus)
            .map_err(|error| error as i32 as u32)
    }

    pub(super) fn dispose_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        dispose_classic_data: bool,
    ) -> std::result::Result<(), u32> {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager
            .dispose_process_handle(bus, handle, dispose_classic_data)
            .map(|_| ())
            .map_err(|error| error as i32 as u32)
    }

    fn process_ptr_size(&self, bus: &mut MacMemoryBus, ptr: u32) -> Option<u32> {
        let memory_manager = self.process_memory_manager();
        memory_manager.borrow_mut().attach_classic_memory_bus(bus);
        let memory_manager = memory_manager.borrow();
        memory_manager.process_ptr_size(bus, ptr)
    }

    fn set_process_ptr_size(
        &mut self,
        bus: &mut MacMemoryBus,
        ptr: u32,
        new_size: u32,
    ) -> u32 {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.set_process_ptr_size(bus, ptr, new_size) as i32 as u32
    }

    fn process_handle_size(
        &self,
        bus: &mut MacMemoryBus,
        handle: u32,
    ) -> Option<u32> {
        let memory_manager = self.process_memory_manager();
        memory_manager.borrow_mut().attach_classic_memory_bus(bus);
        let memory_manager = memory_manager.borrow();
        memory_manager.process_handle_size(bus, handle)
    }

    fn set_process_handle_size(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        new_size: u32,
    ) -> u32 {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.set_process_handle_size(bus, handle, new_size) as i32 as u32
    }

    fn reallocate_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        size: u32,
    ) -> std::result::Result<(u32, u32), u32> {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager
            .reallocate_process_handle(bus, handle, size)
            .map_err(|error| error as i32 as u32)
    }

    fn empty_process_handle(&mut self, bus: &mut MacMemoryBus, handle: u32) -> u32 {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.empty_process_handle(bus, handle) as i32 as u32
    }

    fn copy_bytes_to_new_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        bytes: &[u8],
    ) -> std::result::Result<u32, u32> {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager
            .copy_bytes_to_new_classic_handle(bus, bytes)
            .map(|(handle, _)| handle)
            .map_err(|error| error as i32 as u32)
    }

    fn copy_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
    ) -> std::result::Result<u32, u32> {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager
            .copy_process_handle(bus, handle)
            .map(|(handle, _)| handle)
            .map_err(|error| error as i32 as u32)
    }

    fn replace_process_handle_bytes(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        bytes: &[u8],
    ) -> u32 {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.replace_process_handle_bytes(bus, handle, bytes) as i32 as u32
    }

    fn append_bytes_to_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        bytes: &[u8],
    ) -> u32 {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.append_bytes_to_process_handle(bus, handle, bytes) as i32 as u32
    }

    fn append_process_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        source: u32,
        destination: u32,
    ) -> u32 {
        let memory_manager = self.process_memory_manager();
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.attach_classic_memory_bus(bus);
        memory_manager.append_process_handle(bus, source, destination) as i32 as u32
    }

    pub(crate) fn dispatch_memory<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        if !is_tool && matches!(trap_num, 0x46 | 0x47) {
            if let Err(error) = self.initialize_trap_tables(bus) {
                return Some(Err(error));
            }
        }
        self.read_tick_count(bus);
        let result = match (is_tool, trap_num) {
            // ========== Memory Manager ==========
            // NewPtr ($A11E) / NewPtrSys ($A51E) / NewPtrClear ($A31E) / NewPtrSysClear ($A71E)
            // Allocates a nonrelocatable block. CLEAR variants zero the memory.
            // Inside Macintosh: Memory (1992), pp. 2-36--2-38.
            // Allocates through the process Memory Manager and returns the pointer in A0;
            // CLEAR variants zero memory and SYS variants do not extend the
            // application-zone header.
            (false, 0x1E) => {
                let size = cpu.read_reg(Register::D0);
                let variant = raw_trap_route(self.current_trap_word).os_routine_variant;
                // `Size` is a signed Macintosh LONGINT. Reject negative
                // requests before converting them into an unsigned heap
                // allocation; otherwise an error value can wrap the bump
                // pointer and scribble across guest memory.
                if (size as i32) < 0 {
                    cpu.write_reg(Register::A0, 0);
                    write_memory_result(cpu, bus, MEM_FULL_ERR);
                    return Some(Ok(()));
                }
                let ptr = if matches!(
                    variant,
                    OsRoutineVariant::CurrentHeap | OsRoutineVariant::CurrentHeapClear
                ) {
                    // The application heap shares its partition with the
                    // downward-growing 68K stack. A request that reaches the
                    // live A7 must fail with `memFullErr`; returning it would
                    // let the documented undefined contents overwrite active
                    // frames. Inside Macintosh: Memory (1992), pp. 1-7,
                    // 2-36--2-37.
                    self.new_process_classic_ptr_below(bus, size, cpu.read_reg(Register::A7))
                } else {
                    self.new_process_classic_ptr(bus, size)
                };
                if ptr == 0 && size > 0 {
                    cpu.write_reg(Register::A0, 0);
                    write_memory_result(cpu, bus, MEM_FULL_ERR);
                } else {
                    // Trap bit 10 selects the system-heap variants. Keep the
                    // application-zone header synchronized only for ordinary
                    // NewPtr/NewPtrClear allocations.
                    if matches!(
                        variant,
                        OsRoutineVariant::CurrentHeap | OsRoutineVariant::CurrentHeapClear
                    ) {
                        include_application_allocation_in_zone(bus, ptr, size);
                    }
                    if matches!(
                        variant,
                        OsRoutineVariant::CurrentHeapClear | OsRoutineVariant::SystemHeapClear
                    ) && size > 0
                    {
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
            // Inside Macintosh: Memory (1992), pp. 2-29--2-32.
            (false, 0x22) => {
                let size = cpu.read_reg(Register::D0);
                let variant = raw_trap_route(self.current_trap_word).os_routine_variant;
                let heap = match variant {
                    OsRoutineVariant::SystemHeap | OsRoutineVariant::SystemHeapClear => {
                        ProcessHandleHeap::System
                    }
                    _ => ProcessHandleHeap::Current,
                };
                let clear = matches!(
                    variant,
                    OsRoutineVariant::CurrentHeapClear | OsRoutineVariant::SystemHeapClear
                );
                let result = self.new_process_handle(
                    bus,
                    ProcessNewHandleRequest::new(size as i32, clear, heap),
                );
                if result.succeeded() {
                    // The clear contract is handled by the process service.
                    // For the ordinary 68K edge, retain its existing A5
                    // marker as a deterministic compatibility aid; the
                    // neutral operation deliberately leaves non-clear bytes
                    // undefined and this marker is not cross-ABI policy.
                    if !clear {
                        scribble_uninitialized_allocation(bus, result.data_ptr, size);
                    }
                    cpu.write_reg(Register::A0, result.handle);
                    write_memory_result(cpu, bus, result.error as i32 as u32);
                } else {
                    cpu.write_reg(Register::A0, 0);
                    let error = if result.error == 0 {
                        MEM_FULL_ERR
                    } else {
                        result.error as i32 as u32
                    };
                    write_memory_result(cpu, bus, error);
                }
                Ok(())
            }

            // NewEmptyHandle ($A166)
            // Allocates a master pointer set to NIL without allocating a data block.
            // FUNCTION NewEmptyHandle: Handle;
            // Inside Macintosh: Memory (1992), p. 2-33.
            // NewEmptyHandle ($A066): Allocates master pointer set to NIL, no data block
            (false, 0x66) => {
                match self.new_empty_process_classic_handle(bus) {
                    Ok(handle) => {
                        cpu.write_reg(Register::A0, handle);
                        write_memory_result(cpu, bus, NO_ERR);
                    }
                    Err(error) => {
                        cpu.write_reg(Register::A0, 0);
                        write_memory_result(cpu, bus, error);
                    }
                }
                Ok(())
            }

            // DisposePtr ($A01F)
            // Releases a nonrelocatable block for reuse.
            // PROCEDURE DisposePtr (p: Ptr);
            // Inside Macintosh: Memory (1992), pp. 2-38--2-39.
            (false, 0x1F) => {
                let ptr = cpu.read_reg(Register::A0);
                self.dispose_process_ptr(bus, ptr);
                write_memory_result(cpu, bus, NO_ERR);
                Ok(())
            }

            // DisposeHandle ($A023)
            // Releases the relocatable block and frees the master pointer for other uses.
            // PROCEDURE DisposeHandle (h: Handle);
            // Inside Macintosh: Memory (1992), pp. 2-34--2-35.
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
                if handle != 0 {
                    let data_ptr = bus.read_long(handle);
                    if trace_memory_site(trap_site) {
                        eprintln!(
                            "[MEM] DisposeHandle @${:08X} handle=${:08X} ptr=${:08X}",
                            trap_site, handle, data_ptr
                        );
                    }
                    // Classic disposal retains the stale reverse index until
                    // the freed master-pointer slot is reused, matching the
                    // RecoverHandle scan described in IM:V V-579.
                    let disposal = self.dispose_process_handle(
                        bus,
                        handle,
                        resource_backing.is_none(),
                    );
                    if let Err(error) = disposal {
                        write_memory_result(cpu, bus, error);
                        return Some(Ok(()));
                    }
                }
                self.forget_resource_residency_for_handle(handle);
                self.forget_resource_handle_index_for_handle(handle);
                self.with_resource_manager_mut(|resource_manager| {
                    resource_manager.detached_handles.remove(&handle);
                    resource_manager.loaded_handles.remove(&handle);
                    resource_manager.resource_handle_files.remove(&handle);
                    resource_manager.detached_handle_files.remove(&handle);
                });
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

            // HLock ($A029), HUnlock ($A02A), and MoveHHi ($A064)
            // Changes whether or where a relocatable block can move.
            // PROCEDURE HLock/HUnlock/MoveHHi (h: Handle);
            // Inside Macintosh: Memory (1992), pp. 2-45--2-46, 2-56.
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
                            self.process_memory_manager()
                                .borrow_mut()
                                .lock_process_handle(handle, false);
                        }
                        0x2A => {
                            self.process_memory_manager()
                                .borrow_mut()
                                .unlock_process_handle(handle);
                        }
                        _ => {
                            if self.policy.res_purge() {
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
            // Inside Macintosh: Memory (1992), pp. 2-39--2-40.
            (false, 0x25) => {
                let handle = cpu.read_reg(Register::A0);
                let trap_site = cpu.read_reg(Register::PC).wrapping_sub(2);
                if handle == 0 {
                    write_memory_result(cpu, bus, NIL_HANDLE_ERR);
                } else {
                    let ptr = bus.read_long(handle);
                    let size = self.process_handle_size(bus, handle).unwrap_or(0);
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

            // GetTrapAddress ($A146), GetOSTrapAddress ($A346), and
            // GetToolTrapAddress ($A746): Return the handler address in A0
            // for the D0.W trap word/number. The legacy form infers its table
            // from the trap number; the newer forms select it explicitly.
            // Inside Macintosh: Operating System Utilities 1994, pp. 8-26
            // to 8-28 and 8-32 to 8-33.
            // Both tables return stable callable project-authored gateways.
            // Toolbox gateways use the auto-pop variant so the Pascal
            // argument frame begins below the JSR return address. OS gateways
            // use the canonical register-based trap followed by RTS. See the
            // two `get_or_create_*_trap_trampoline` helpers.
            (false, 0x46) => {
                let trap_word = cpu.read_reg(Register::D0) as u16;
                let trap_table_key = self.trap_address_table_key(trap_word);
                // Match the native getter: a malformed logical chain does
                // not turn into a reconstructed default system gateway.
                cpu.write_reg(
                    Register::A0,
                    self.trap_table_address(bus, trap_table_key).unwrap_or(0),
                );
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

            // SetTrapAddress ($A047/$A647 = (false, 0x47))
            // Installs a native 68K trap handler.
            // D0.W = trap number, A0 = new handler address
            // Inside Macintosh Volume II, II-384
            // SetTrapAddress ($A047): Installs native 68K trap handler via D0=trap word, A0=handler; per IM:II II-384
            (false, 0x47) => {
                let trap_word = cpu.read_reg(Register::D0) as u16;
                let trap_table_key = self.trap_address_table_key(trap_word);
                let handler_addr = cpu.read_reg(Register::A0);
                // The shared service validates protected heads before
                // updating the selected guest table cell or chain link.
                match self.install_trap_address(bus, trap_table_key, handler_addr) {
                    Ok(()) => Ok(()),
                    Err(error) => handle_trap_manager_set_error(bus, error),
                }
            }

            // StripAddress ($A055)
            // Strips the high byte of a 24-bit address. No-op in 32-bit mode.
            // Inside Macintosh Volume V, V-593
            // StripAddress ($A055): Removes the flag byte in 24-bit mode.
            (false, 0x55) => {
                if !bus.addressing_32_bit() {
                    cpu.write_reg(Register::A0, cpu.read_reg(Register::A0) & 0x00FF_FFFF);
                }
                Ok(())
            }

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
                bus.set_addressing_32_bit(self.mmu_mode != 0);
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

            // SlotVRemove ($A070)
            // Removes a slot-based vertical retrace task from its queue.
            // FUNCTION SlotVRemove (vblTaskPtr: QElemPtr; theSlot: Integer): OSErr;
            // Inside Macintosh: Processes (1994), pp. 4-23--4-24
            (false, 0x70) => {
                let task_ptr = cpu.read_reg(Register::A0);
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
                    REFERENCE_M68K_EXECUTION_CAPABILITIES.processor_type as u16,
                );
                bus.write_byte(
                    rec_ptr + 8,
                    u8::from(REFERENCE_M68K_EXECUTION_CAPABILITIES.fpu_type != 0),
                );
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
                // Keep the process-owned application limit in step with the
                // guest global so a nested PowerPC callback observes the same
                // boundary immediately. The native allocator ceiling remains
                // independent. Inside Macintosh: Memory (1992), pp. 2-83--2-85.
                self.process_memory_manager()
                    .borrow_mut()
                    .set_application_heap_limit(
                        bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
                    );
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
            // FreeMem when the heap is fragmented) plus the
            // application-zone growth allowance in A0, CompactMem
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
            // MaxMem ($A01D): Per IM:II II-39 and Memory 1992 2-74, the
            // largest contiguous block is returned in D0 and the number of
            // bytes by which the application zone can grow is returned in
            // A0. The launcher exposes the current application-zone extent
            // through HeapEnd; MaxApplZone advances it to ApplLimit. The zone
            // header's bkLim may already cover directly loaded resources, so
            // it cannot represent the remaining launch-time growth allowance.
            // $A11D dispatches here via the OS-trap low-byte (0x1D) decode.
            (false, 0x1D) => {
                let free = free_heap_estimate(bus);
                cpu.write_reg(Register::D0, free);
                let heap_end = bus.read_long(addr::HEAP_END);
                let appl_limit = bus.read_long(addr::APPL_LIMIT);
                let grow = appl_limit.saturating_sub(heap_end);
                cpu.write_reg(Register::A0, grow);
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
                            self.track_handle_ptr(new_ptr, handle);
                        }
                        cpu.write_reg(Register::D0, 0); // noErr
                    }
                } else {
                    let result = self.set_process_handle_size(bus, handle, new_size);
                    cpu.write_reg(Register::D0, result);
                }
                Ok(())
            }

            // ReallocateHandle ($A027) / ReallocateHandleSys ($A427)
            // Replaces any existing relocatable block and updates the master
            // pointer. The new block is unlocked, unpurgeable, and has
            // undefined contents. Systemless has one flat guest heap, so the
            // current- and system-heap forms share allocation behavior while
            // retaining their exact raw-word identity in RawTrapRoute.
            // PROCEDURE ReallocateHandle (h: Handle; logicalSize: Size);
            // Inside Macintosh: Memory (1992), pp. 2-52--2-53; Universal
            // Interfaces 3.4 MacMemory.h lines 1184--1202.
            (false, 0x27) => {
                let handle = cpu.read_reg(Register::A0);
                let size = cpu.read_reg(Register::D0);
                let indexed_old_ptr = self
                    .loaded_handles
                    .get(&handle)
                    .map(|entry| entry.0)
                    .unwrap_or_else(|| bus.read_long(handle));
                match self.reallocate_process_handle(bus, handle, size) {
                    Ok((_old_ptr, new_ptr)) => {
                        self.with_resource_manager_mut(|resource_manager| {
                            let resource_key = resource_manager
                                .loaded_handles
                                .get(&handle)
                                .and_then(|&(_, res_type, res_id)| {
                                    resource_manager
                                        .resource_handle_files
                                        .get(&handle)
                                        .copied()
                                        .map(|refnum| (refnum, res_type, res_id))
                                });
                            if let Some(entry) = resource_manager.loaded_handles.get_mut(&handle) {
                                entry.0 = new_ptr;
                            }
                            if let Some(resources) = resource_manager.resources.as_mut() {
                                // NIL is shared by every unloaded resource; only a
                                // resource-owned handle may change its own map entry.
                                if indexed_old_ptr == 0 {
                                    if let Some((refnum, res_type, res_id)) = resource_key {
                                        if let Some(file) = resources.files.get_mut(&refnum) {
                                            if file.loaded.get(&(res_type, res_id)).copied()
                                                == Some(0)
                                            {
                                                file.loaded.insert((res_type, res_id), new_ptr);
                                            }
                                            for ((named_type, _), (named_id, ptr)) in &mut file.named
                                            {
                                                if *named_type == res_type
                                                    && *named_id == res_id
                                                    && *ptr == 0
                                                {
                                                    *ptr = new_ptr;
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    for file in resources.files.values_mut() {
                                        for loaded_ptr in file.loaded.values_mut() {
                                            if *loaded_ptr == indexed_old_ptr {
                                                *loaded_ptr = new_ptr;
                                            }
                                        }
                                        for (_id, named_ptr) in file.named.values_mut() {
                                            if *named_ptr == indexed_old_ptr {
                                                *named_ptr = new_ptr;
                                            }
                                        }
                                    }
                                }
                            }
                        });
                        write_memory_result(cpu, bus, NO_ERR);
                    }
                    Err(error) => write_memory_result(cpu, bus, error),
                }
                Ok(())
            }

            // HandToHand ($A9E1)
            // Copies a relocatable block into a new unlocked, unpurgeable handle
            // in the source block's heap zone.
            // FUNCTION HandToHand (VAR theHndl: Handle): OSErr;
            // Inside Macintosh: Memory (1992), pp. 2-62--2-63.
            (true, 0x1E1) => {
                let src_handle = cpu.read_reg(Register::A0);
                match self.copy_process_handle(bus, src_handle) {
                    Ok(handle) => {
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, NO_ERR);
                    }
                    Err(error) => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, error);
                    }
                }
                Ok(())
            }

            // PtrToXHand ($A9E2)
            // Makes an existing handle refer to a copy of the requested bytes.
            // FUNCTION PtrToXHand (srcPtr: Ptr; dstHndl: Handle; size: LongInt): OSErr;
            // Inside Macintosh: Memory (1992), pp. 2-61--2-62.
            (true, 0x1E2) => {
                let src_ptr = cpu.read_reg(Register::A0);
                let dst_handle = cpu.read_reg(Register::A1);
                let size = cpu.read_reg(Register::D0);
                cpu.write_reg(Register::A0, dst_handle);
                if (size as i32) < 0 {
                    cpu.write_reg(Register::D0, MEM_FULL_ERR);
                    return Some(Ok(()));
                }
                let bytes = bus.read_bytes(src_ptr, size as usize);
                let result = self.replace_process_handle_bytes(bus, dst_handle, &bytes);
                cpu.write_reg(Register::D0, result);
                Ok(())
            }

            // PtrToHand ($A9E3)
            // Copies bytes referenced by a pointer into a new relocatable block.
            // FUNCTION PtrToHand (srcPtr: Ptr; VAR dstHndl: Handle; size: LongInt): OSErr;
            // Inside Macintosh: Memory (1992), pp. 2-60--2-61.
            (true, 0x1E3) => {
                let src_ptr = cpu.read_reg(Register::A0);
                let size = cpu.read_reg(Register::D0);
                if (size as i32) < 0 {
                    cpu.write_reg(Register::A0, 0);
                    cpu.write_reg(Register::D0, MEM_FULL_ERR);
                    return Some(Ok(()));
                }
                let bytes = bus.read_bytes(src_ptr, size as usize);
                match self.copy_bytes_to_new_process_handle(bus, &bytes) {
                    Ok(handle) => {
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, NO_ERR);
                    }
                    Err(error) => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, error);
                    }
                }
                Ok(())
            }

            // HandAndHand ($A9E4)
            // Appends one relocatable block to another without changing the source.
            // FUNCTION HandAndHand (aHndl, bHndl: Handle): OSErr;
            // Inside Macintosh: Memory (1992), pp. 2-64--2-65.
            (true, 0x1E4) => {
                let src_handle = cpu.read_reg(Register::A0);
                let dst_handle = cpu.read_reg(Register::A1);
                cpu.write_reg(Register::A0, dst_handle);
                let result = self.append_process_handle(bus, src_handle, dst_handle);
                cpu.write_reg(Register::D0, result);
                Ok(())
            }

            // PtrAndHand ($A9EF)
            // Appends bytes referenced by a pointer to a relocatable block.
            // FUNCTION PtrAndHand (pntr: Ptr; hndl: Handle; size: LongInt): OSErr;
            // Inside Macintosh: Memory (1992), pp. 2-65--2-66.
            (true, 0x1EF) => {
                let src_ptr = cpu.read_reg(Register::A0);
                let dst_handle = cpu.read_reg(Register::A1);
                let append_size = cpu.read_reg(Register::D0);
                cpu.write_reg(Register::A0, dst_handle);
                if (append_size as i32) < 0 {
                    cpu.write_reg(Register::D0, MEM_FULL_ERR);
                    return Some(Ok(()));
                }
                let bytes = bus.read_bytes(src_ptr, append_size as usize);
                let result = self.append_bytes_to_process_handle(bus, dst_handle, &bytes);
                cpu.write_reg(Register::D0, result);
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
                let memory_manager = self.process_memory_manager();
                let handle = memory_manager
                    .borrow()
                    .recover_handle_from_master_pointer(ptr, |handle| {
                        Some(bus.read_long(handle))
                    })
                    .unwrap_or(0);
                cpu.write_reg(Register::A0, handle);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // SetPtrSize ($A020)
            // Changes the logical size of a nonrelocatable block.
            // On entry: A0 = pointer, D0 = newSize
            // On exit:  D0 = result code (noErr or memFullErr)
            // Inside Macintosh: Memory (1992), pp. 2-42--2-43. Critical contract:
            // "SetPtrSize doesn't move the pointer" — the block stays at
            // its current address whether shrinking or growing.
            //
            // The current allocator is a bump allocator with a free-list;
            // in-place grow beyond the original 4-byte-aligned capacity
            // isn't tractable without a compaction pass. For shrink +
            // in-capacity-grow we update the stored logical size and keep
            // the pointer stable. For grow beyond the aligned capacity we
            // return memFullErr.
            (false, 0x20) => {
                let ptr = cpu.read_reg(Register::A0);
                let new_size = cpu.read_reg(Register::D0);
                let result = self.set_process_ptr_size(bus, ptr, new_size);
                write_memory_result(cpu, bus, result);
                Ok(())
            }

            // GetPtrSize ($A021)
            // Returns the logical size of the nonrelocatable block pointed to by A0.
            // On exit: D0 = size (or negative error code)
            // Inside Macintosh: Memory (1992), pp. 2-41--2-42.
            (false, 0x21) => {
                let ptr = cpu.read_reg(Register::A0);
                let size = self.process_ptr_size(bus, ptr).unwrap_or(0);
                cpu.write_reg(Register::D0, size);
                Ok(())
            }

            // ========== Time Manager ==========

            // InsTime / InsXTime ($A058/$A458)
            // Installs an original or extended task record into the Time Manager queue.
            // PROCEDURE InsTime (tmTaskPtr: QElemPtr);
            // PROCEDURE InsXTime (tmTaskPtr: QElemPtr);
            // Inside Macintosh: Processes (1994), pp. 3-18--3-20
            // TMTask layout: qLink(+0,4) qType(+4,2) tmAddr(+6,4) tmCount(+10,4)
            // Extended fields: tmWakeUp(+14,4) tmReserved(+18,4)
            (false, 0x58) => {
                let task_ptr = cpu.read_reg(Register::A0);
                let tm_addr = bus.read_long(task_ptr + 6);
                let extended = matches!(
                    raw_trap_route(self.current_trap_word).os_routine_variant,
                    OsRoutineVariant::TimeTaskExtended
                );
                if !extended || bus.read_long(task_ptr + 14) == 0 {
                    self.callback_scheduling.remove_extended_wakeup(task_ptr);
                }
                // Remove any existing task for the same record address
                self.timer_tasks.with_mut(|timer_tasks| {
                    timer_tasks.retain(|task| task.task_ptr != task_ptr);
                    timer_tasks.push(super::dispatch::TimerTask {
                        task_ptr,
                        architecture: CallbackTaskArchitecture::M68k,
                        extended,
                        callback: tm_addr,
                        active: false,
                        fire_at_tick: 0,
                        fire_at_subtick: 0,
                        last_fired_tick: None,
                    });
                });
                self.sync_time_task_links(bus);
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
                let current_subtick = self
                    .callback_scheduling
                    .current_subtick()
                    .max(bus.read_long(0x016A) as u64 * 1_000_000);
                let remaining_subticks = self
                    .timer_tasks
                    .iter()
                    .find(|task| task.task_ptr == task_ptr && task.active)
                    .map(|task| task.fire_at_subtick.saturating_sub(current_subtick))
                    .unwrap_or(0);
                self.timer_tasks
                    .with_mut(|timer_tasks| timer_tasks.retain(|task| task.task_ptr != task_ptr));
                self.sync_time_task_links(bus);

                // The revised and extended Time Managers return unused time
                // through tmCount. Prefer negated microseconds for maximum
                // accuracy, falling back to positive milliseconds only when
                // the microsecond magnitude cannot fit in a signed LongInt.
                // Inside Macintosh: Processes (1994), pp. 3-14 and 3-21.
                let remaining_count = if remaining_subticks == 0 {
                    0
                } else {
                    let remaining_us = remaining_subticks.div_ceil(60);
                    if remaining_us <= i32::MAX as u64 {
                        -(remaining_us as i32)
                    } else {
                        remaining_us.div_ceil(1_000).min(i32::MAX as u64) as i32
                    }
                };
                bus.write_long(task_ptr + 10, remaining_count as u32);
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
                let requested_delay_subticks = if delay == 0 {
                    0
                } else if delay > 0 {
                    (delay as u64) * 60_000
                } else {
                    let us = (-delay) as u64;
                    (us * 60).max(1)
                };
                let current_subtick = self
                    .callback_scheduling
                    .current_subtick()
                    .max(current_ticks as u64 * SUBTICKS_PER_TICK);
                let task_kind = self
                    .timer_tasks
                    .iter()
                    .find(|task| task.task_ptr == task_ptr)
                    .map(|task| task.extended);
                let fire_at_subtick = if task_kind == Some(true) {
                    // Processes 1994, pp. 3-8--3-9: an extended task whose
                    // tmWakeUp is nonzero schedules relative to its preceding
                    // intended expiry, eliminating callback/interrupt drift.
                    // A target already in the past has an actual delay of 0,
                    // while the intended (past) target remains the next base.
                    let prior_wakeup = if bus.read_long(task_ptr + 14) == 0 {
                        None
                    } else {
                        self.callback_scheduling.extended_wakeup(task_ptr)
                    };
                    let intended_wakeup = prior_wakeup
                        .unwrap_or(current_subtick)
                        .saturating_add(requested_delay_subticks);
                    self.callback_scheduling
                        .set_extended_wakeup(task_ptr, intended_wakeup);
                    // tmWakeUp is explicitly an opaque internal format. Keep
                    // it nonzero so guest code can preserve or reset it, while
                    // the exact deadline remains in manager-owned state.
                    let opaque_wakeup = ((intended_wakeup / 60) as u32).max(1);
                    bus.write_long(task_ptr + 14, opaque_wakeup);
                    intended_wakeup.max(current_subtick)
                } else {
                    // Preserve the established next-tick scheduling boundary
                    // for an original task's zero-delay "as soon as interrupts
                    // are enabled" request.
                    let delay_subticks = if delay == 0 {
                        SUBTICKS_PER_TICK
                    } else {
                        requested_delay_subticks
                    };
                    current_subtick.saturating_add(delay_subticks)
                };
                let fire_at = fire_at_subtick.div_ceil(SUBTICKS_PER_TICK) as u32;
                self.timer_tasks.with_mut(|timer_tasks| {
                    if let Some(task) = timer_tasks.iter_mut().find(|t| t.task_ptr == task_ptr) {
                        task.active = true;
                        task.fire_at_tick = fire_at;
                        task.fire_at_subtick = fire_at_subtick;
                        // Re-read tmAddr in case it changed between InsTime and PrimeTime
                        task.callback = bus.read_long(task_ptr + 6);
                    }
                });
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

            // HPurge ($A049)
            // Makes a relocatable block purgeable.
            // PROCEDURE HPurge (h: Handle);
            // Inside Macintosh: Memory (1992), pp. 2-46--2-48.
            (false, 0x49) => {
                let handle = cpu.read_reg(Register::A0);
                self.process_memory_manager()
                    .borrow_mut()
                    .set_process_handle_purgeable(handle, true);
                Ok(())
            }

            // HNoPurge ($A04A)
            // Makes a relocatable block unpurgeable.
            // PROCEDURE HNoPurge (h: Handle);
            // Inside Macintosh: Memory (1992), p. 2-48.
            (false, 0x4A) => {
                let handle = cpu.read_reg(Register::A0);
                self.process_memory_manager()
                    .borrow_mut()
                    .set_process_handle_purgeable(handle, false);
                Ok(())
            }

            // HGetState ($A069)
            // Returns the properties of a relocatable block.
            // FUNCTION HGetState (h: Handle): SignedByte;
            // Inside Macintosh: Memory (1992), pp. 2-48--2-49.
            (false, 0x69) => {
                let handle = cpu.read_reg(Register::A0);
                let mut state = self.handle_state_bits(handle).unwrap_or(0);
                if self.loaded_handles.contains_key(&handle) {
                    state |= 0x20; // resource bit
                }
                cpu.write_reg(Register::D0, state as u32);
                Ok(())
            }

            // HSetState ($A06A)
            // Restores the properties of a relocatable block.
            // PROCEDURE HSetState (h: Handle; flags: SignedByte);
            // Inside Macintosh: Memory (1992), p. 2-49.
            (false, 0x6A) => {
                let handle = cpu.read_reg(Register::A0);
                self.process_memory_manager()
                    .borrow_mut()
                    .restore_process_handle_state(handle, cpu.read_reg(Register::D0) as u8);
                Ok(())
            }

            // HWPriv ($A198; cache-flush glue also uses $A098)
            // Controls processor caches through a selector in D0.
            // Register ABI: D0 = selector; FlushCodeCacheRange uses A0/A1 and returns OSErr in D0.
            // Inside Macintosh: Memory (1992), pp. 4-29 to 4-33.
            (false, 0x98) => {
                let selector = cpu.read_reg(Register::D0);
                let operation = hwpriv_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
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
            // _Control ($A004): Handles cscSetMode (csCode=2) by switching
            // depth and returning the page/base address through the VDPgInfoPtr
            // and cscSetEntries (csCode=3) via VDSetEntryRecord. Unsupported
            // requests return controlErr (IM:Devices 1994, pp. 1-35--1-36, 1-77).
            (false, 0x04) => {
                let pb = cpu.read_reg(Register::A0);
                let mut control_result = NO_ERR;
                if pb != 0 {
                    let cs_code = bus.read_word(pb + 26) as i16;
                    if !matches!(cs_code, 2 | 3 | 4) {
                        // Do not advertise an output we did not produce. For
                        // example, a drive-icon request (21) expects an icon
                        // pointer in csParam; noErr with untouched output makes
                        // callers copy arbitrary memory instead of receiving an error.
                        control_result = CONTROL_ERR;
                    }
                    // cscSetMode (csCode=2): switch mode/page and return base address.
                    // The video driver's csParam contains a VDPgInfoPtr.
                    // VDPgInfo layout:
                    //   csMode [word] @ +0
                    //   csData [long] @ +2
                    //   csPage [word] @ +6
                    //   csBaseAddr [long] @ +8
                    // Designing Cards and Drivers 3rd Ed 1992, p. 219, 235
                    // Devices 1994, p. 2-128, 6-68
                    if cs_code == 2 {
                        let vdpg_info = bus.read_long(pb + 28);
                        let cs_mode = if vdpg_info != 0 {
                            bus.read_word(vdpg_info)
                        } else {
                            0
                        };
                        let cs_data = if vdpg_info != 0 {
                            bus.read_long(vdpg_info + 2)
                        } else {
                            0
                        };
                        let cs_page = if vdpg_info != 0 {
                            bus.read_word(vdpg_info + 6)
                        } else {
                            0
                        };

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

                        if vdpg_info == 0 || cs_page != 0 {
                            control_result = CONTROL_ERR;
                        } else if let Some(depth) = crate::display::classic_pixel_size(cs_mode) {
                            let width = self.screen_mode.2;
                            let height = self.screen_mode.3;
                            if self.do_setdepth_with_geometry(cpu, bus, depth, width, height) {
                                // Single-screen HLE exposes page 0.
                                bus.write_word(vdpg_info + 6, 0); // csPage
                                bus.write_long(vdpg_info + 8, self.screen_mode.0);
                            } else {
                                control_result = (-108i16) as u32;
                            }
                        } else {
                            control_result = CONTROL_ERR;
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
                        // A direct driver palette contains presentation-ready
                        // values. Keep explicit guest gamma authoritative, but
                        // otherwise stop applying the compatibility transfer
                        // used by the emulated high-level Color Manager path.
                        self.display_gamma
                            .set_implicit(crate::display::linear_display_gamma());
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

                    // cscSetGamma (csCode=4): csParam points to a
                    // VDGammaRecord whose first longword is the GammaTbl Ptr.
                    // Universal Interfaces 3.4 Video.h (`VDGammaRecord`,
                    // `GammaTbl`); BasiliskII video.cpp `set_gamma_table`.
                    if cs_code == 4 {
                        let vd_gamma_ptr = bus.read_long(pb + 28);
                        control_result = self.install_device_gamma(bus, vd_gamma_ptr);
                        if trace_video_driver_enabled() {
                            let gamma_ptr = if vd_gamma_ptr != 0 {
                                bus.read_long(vd_gamma_ptr)
                            } else {
                                0
                            };
                            eprintln!(
                                "[VIDEO] cscSetGamma record=${:08X} table=${:08X} result={}",
                                vd_gamma_ptr, gamma_ptr, control_result as i32,
                            );
                        }
                    }
                    bus.write_word(pb + 16, control_result as u16);
                }
                cpu.write_reg(Register::D0, control_result);
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
            // The modeled main video device has refNum 0. Its GetMode,
            // GetPages, and GetGray status calls fill the VDPgInfo record
            // pointed to by csParam. Other zero-refNum requests retain the legacy noErr
            // result; unknown nonzero refNums return badUnitErr (-21).
            // Designing Cards and Drivers for the Macintosh II and SE
            // (1987), pp. 9-17 and 9-27: status csCode 2 returns csMode,
            // csPage, and csBaseAddr; csCode 4 returns the number of pages
            // through csPage; csCode 6 returns 0 for colors or 1 for gray
            // tones through csMode. The modeled screen has one display page.
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
                    let cs_code = bus.read_word(pb + 26);
                    let vdpg_info = bus.read_long(pb + 28);
                    let result = if io_ref_num == 0 {
                        match cs_code {
                            2 => {
                                if vdpg_info != 0 {
                                    if let Some(mode) =
                                        crate::display::classic_depth_mode(self.screen_mode.4)
                                    {
                                        bus.write_word(vdpg_info, mode);
                                        bus.write_word(vdpg_info + 6, 0);
                                        bus.write_long(vdpg_info + 8, self.screen_mode.0);
                                    }
                                }
                            }
                            4 if vdpg_info != 0 => bus.write_word(vdpg_info + 6, 1),
                            6 => {
                                if vdpg_info != 0 {
                                    let gdh = self.ensure_main_gdevice(bus);
                                    let gd = bus.read_long(gdh);
                                    let is_color = gd != 0 && bus.read_word(gd + 20) & 1 != 0;
                                    bus.write_word(vdpg_info, u16::from(!is_color));
                                }
                            }
                            _ => {}
                        }
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

            // UpperString / UprString ($A054/$A254)
            // Converts lowercase Roman characters to uppercase in place.
            // PROCEDURE UpperString(VAR theString: Str255; diacSens: Boolean);
            // Inside Macintosh: Text (1993), pp. 5-64--5-65.
            (false, 0x54) => {
                let ptr = cpu.read_reg(Register::A0);
                let len = cpu.read_reg(Register::D0) & 0xFFFF;
                let strip_marks = match raw_trap_route(self.current_trap_word).os_routine_variant {
                    OsRoutineVariant::UpperStringPreserveMarks => false,
                    OsRoutineVariant::UpperStringStripMarks => true,
                    _ => unreachable!("UprString trap word must have a classified variant"),
                };
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
                let variant = raw_trap_route(self.current_trap_word).os_routine_variant;
                for i in 0..len {
                    let ch = bus.read_byte(ptr + i);
                    let result = match variant {
                        OsRoutineVariant::LowerText => mac_roman_to_lower(ch),
                        OsRoutineVariant::StripText => mac_roman_strip_diacriticals(ch),
                        OsRoutineVariant::UpperText => mac_roman_to_upper(ch, false),
                        OsRoutineVariant::StripUpperText => mac_roman_to_upper(ch, true),
                        _ => ch,
                    };
                    bus.write_byte(ptr + i, result);
                }
                cpu.write_reg(Register::D0, 0); // noErr
                Ok(())
            }

            // RelString compares two Roman strings relationally. MARKS (bit 9)
            // makes comparison diacritic-sensitive; CASE (bit 10) makes it
            // case-sensitive. Inside Macintosh: Text (1993), pp. 5-60--5-61.
            //
            // Assembly calling convention:
            //   On entry: A0 = ptr to first string, A1 = ptr to second string
            //             D0 high word = length of first, D0 low word = length of second
            //   On exit:  D0 = -1 (less), 0 (equal), or 1 (greater) as long word
            (false, 0x50) => {
                let ptr_a = cpu.read_reg(Register::A0);
                let ptr_b = cpu.read_reg(Register::A1);
                let d0 = cpu.read_reg(Register::D0);
                let len_a = (d0 >> 16) & 0xFFFF;
                let len_b = d0 & 0xFFFF;
                let (case_sensitive, diacritic_sensitive) = raw_trap_route(self.current_trap_word)
                    .os_routine_variant
                    .text_comparison_sensitivity()
                    .expect("RelString trap word must have a classified comparison variant");

                let min_len = std::cmp::min(len_a, len_b);
                let mut result: i32 = 0;
                for i in 0..min_len {
                    let mut a = bus.read_byte(ptr_a + i);
                    let mut b = bus.read_byte(ptr_b + i);
                    if !diacritic_sensitive {
                        a = mac_roman_strip_diacriticals(a);
                        b = mac_roman_strip_diacriticals(b);
                    }
                    if !case_sensitive {
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
                bus.fill_zeros(ptr, count);
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
            // Queue a valid task for one-shot delivery after an interrupt.
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
                    self.enqueue_deferred_task(bus, task_ptr);
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

            // IdleUpdate ($A285), IdleState ($A485), SerialPower ($A685)
            // Resets the activity timer or controls idle and serial-port power through D0.
            // FUNCTION IdleUpdate: LongInt;
            // PROCEDURE EnableIdle; PROCEDURE DisableIdle;
            // FUNCTION GetCPUSpeed: LongInt;
            // PROCEDURE AOn; PROCEDURE AOnIgnoreModem; PROCEDURE BOn;
            // PROCEDURE AOff; PROCEDURE BOff;
            // Inside Macintosh: Devices (1994), pp. 6-29--6-30, 6-33--6-35.
            // Universal Interfaces 3.4 Power.h lines 650--701 and 733--791.
            (false, 0x85) => {
                let raw_selector = cpu.read_reg(Register::D0);
                let operation = power_control_operation_route(self.current_trap_word, raw_selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                match raw_trap_route(self.current_trap_word).os_routine_variant {
                    OsRoutineVariant::PowerIdleUpdate => {
                        self.power_idle_last_update_tick = self.current_tick();
                        cpu.write_reg(Register::D0, self.current_tick());
                    }
                    OsRoutineVariant::PowerIdleState => {
                        let selector = cpu.read_reg(Register::D0) as i32;
                        if selector < 0 {
                            cpu.write_reg(
                                Register::D0,
                                crate::machine_profile::REFERENCE_MACHINE_PROFILE.realtime_cpu_mhz
                                    as u32,
                            );
                        } else if selector == 0 {
                            self.power_idle_disable_count =
                                self.power_idle_disable_count.saturating_sub(1);
                        } else {
                            self.power_idle_disable_count =
                                self.power_idle_disable_count.saturating_add(1);
                        }
                    }
                    OsRoutineVariant::PowerSerial => match cpu.read_reg(Register::D0) as u8 {
                        0x04 | 0x05 => self.serial_port_a_powered = true,
                        0x00 => self.serial_port_b_powered = true,
                        0x84 => self.serial_port_a_powered = false,
                        0x80 => self.serial_port_b_powered = false,
                        _ => {}
                    },
                    _ => cpu.write_reg(Register::D0, 0),
                }
                Ok(())
            }

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
            // Adds or removes an A0-supplied SleepQRec from the ordered sleep
            // queue; the manager maintains its first-longword link.
            // PROCEDURE SleepQInstall(qRecPtr: SleepQRecPtr);
            // PROCEDURE SleepQRemove(qRecPtr: SleepQRecPtr);
            // Inside Macintosh: Devices (1994), pp. 6-18, 6-26, and 6-33.
            // Universal Interfaces 3.4 Power.h lines 447--461 and 705--731.
            (false, 0x8A) => {
                let q_rec_ptr = cpu.read_reg(Register::A0);
                match raw_trap_route(self.current_trap_word).os_routine_variant {
                    OsRoutineVariant::SleepQueueInstall
                        if q_rec_ptr != 0 && !self.sleep_queue.contains(&q_rec_ptr) =>
                    {
                        if let Some(&tail) = self.sleep_queue.last() {
                            bus.write_long(tail, q_rec_ptr);
                        }
                        bus.write_long(q_rec_ptr, 0);
                        self.sleep_queue.push(q_rec_ptr);
                    }
                    OsRoutineVariant::SleepQueueRemove => {
                        if let Some(index) = self
                            .sleep_queue
                            .iter()
                            .position(|&entry| entry == q_rec_ptr)
                        {
                            let next = self.sleep_queue.get(index + 1).copied().unwrap_or(0);
                            if index != 0 {
                                let previous = self.sleep_queue[index - 1];
                                bus.write_long(previous, next);
                            }
                            self.sleep_queue.remove(index);
                        }
                    }
                    _ => {}
                }
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // SCSIAtomic ($A089)
            // Dispatches SCSI Manager 4.3 client and XPT operations selected in D0.
            // FUNCTION SCSIAction (scsiPB: SCSI_PBPtr): OSErr;
            // Inside Macintosh: Devices (1994), pp. 4-38 to 4-39 and 4-54 to 4-58.
            (false, 0x89) => {
                let raw_selector = cpu.read_reg(Register::D0);
                let operation = scsi_atomic_operation_route(self.current_trap_word, raw_selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);

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
            // Dispatches Communications Toolbox routines by selector.
            // SELECTOR(selector: INTEGER; parameters: selector-specific);
            // Inside Macintosh Volume VI (1991), Appendix C, p. C-4.
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
                    // CountDITL ($A08B/$0403)
                    // Returns the number of current items in the specified dialog box.
                    // FUNCTION CountDITL (theDialog: DialogPtr): Integer;
                    // Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-128 to 6-129.
                    (0x0403, _) | (_, 0x0403) => {
                        let dialog_ptr = bus.read_long(sp + 6);
                        let mut count = count_dialog_items(bus, dialog_ptr);
                        if count == 0 {
                            count = self
                                .dialog_items
                                .get(&dialog_ptr)
                                .map(|items| items.len() as u16)
                                .unwrap_or(0);
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
            // Dispatches virtual-memory debugger support routines selected in D0.
            // FUNCTION DebuggerGetMax: LongInt;
            // Inside Macintosh: Memory (1992), pp. 3-34 to 3-40.
            (false, 0x8D) => {
                let raw_selector = cpu.read_reg(Register::D0);
                let operation = debug_util_operation_route(self.current_trap_word, raw_selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);

                match raw_selector {
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
                }
            }

            // NMInstall ($A05E)
            // Adds a notification request to the notification queue.
            // FUNCTION NMInstall(nmReqPtr: NMRecPtr): OSErr;
            // Inside Macintosh Volume VI (1991), pp. 24-6 to 24-10.
            (false, 0x5E) => {
                let nm_rec = cpu.read_reg(Register::A0);
                let result = self.install_notification_request(cpu, bus, nm_rec);
                cpu.write_reg(Register::D0, result as u16 as u32);
                Ok(())
            }

            // NMRemove ($A05F)
            // Removes a notification request from the notification queue.
            // FUNCTION NMRemove(nmReqPtr: NMRecPtr): OSErr;
            // Inside Macintosh Volume VI (1991), pp. 24-10 to 24-11.
            (false, 0x5F) => {
                let nm_rec = cpu.read_reg(Register::A0);
                let result = if nm_rec == 0 || bus.read_word(nm_rec + 4) != 8 {
                    -299 // nmTypErr
                } else {
                    self.remove_notification_request(bus, nm_rec)
                };
                cpu.write_reg(Register::D0, result as u16 as u32);
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
                        self.input_state.mouse_button_pressed(),
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
            // Releases an unlocked relocatable block while retaining its
            // master pointer, whose value becomes NIL.
            // PROCEDURE EmptyHandle (h: Handle);
            // Inside Macintosh: Memory (1992), pp. 2-51--2-52.
            (false, 0x2B) => {
                let handle = cpu.read_reg(Register::A0);
                let resource_backed = self.loaded_handles.contains_key(&handle);
                let result = self.empty_process_handle(bus, handle);
                if result == NO_ERR && resource_backed {
                    self.empty_resource_handle_residency(handle);
                }
                write_memory_result(cpu, bus, result);
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
            // Sets the resource flag of a relocatable block.
            // PROCEDURE HSetRBit (h: Handle);
            // Inside Macintosh: Memory (1992), pp. 2-49--2-50.
            (false, 0x67) => {
                let handle = cpu.read_reg(Register::A0);
                if handle == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                } else {
                    self.process_memory_manager()
                        .borrow_mut()
                        .set_process_handle_resource(handle, true);
                    cpu.write_reg(Register::D0, 0); // noErr
                }
                Ok(())
            }

            // HClrRBit ($A068)
            // Clears the resource flag of a relocatable block.
            // PROCEDURE HClrRBit (h: Handle);
            // Inside Macintosh: Memory (1992), pp. 2-50--2-51.
            (false, 0x68) => {
                let handle = cpu.read_reg(Register::A0);
                if handle == 0 {
                    cpu.write_reg(Register::D0, (-109i32) as u32); // nilHandleErr
                } else {
                    self.process_memory_manager()
                        .borrow_mut()
                        .set_process_handle_resource(handle, false);
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
                } else {
                    let memory_manager = self.process_memory_manager();
                    let valid = {
                        let mut memory_manager = memory_manager.borrow_mut();
                        memory_manager.attach_classic_memory_bus(bus);
                        memory_manager.native_allocation(handle).is_some()
                            || memory_manager.classic_allocation_size(handle) == Some(4)
                    };
                    if !valid {
                        cpu.write_reg(Register::D0, (-111i32) as u32); // memWZErr
                    } else {
                        let the_zone = bus.read_long(0x0118); // TheZone low-mem global
                        cpu.write_reg(Register::A0, the_zone);
                        cpu.write_reg(Register::D0, 0); // noErr
                    }
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
                    let memory_manager = self.process_memory_manager();
                    let valid = {
                        let mut memory_manager = memory_manager.borrow_mut();
                        memory_manager.attach_classic_memory_bus(bus);
                        memory_manager.process_ptr_size(bus, ptr).is_some()
                    };
                    if !valid {
                        cpu.write_reg(Register::D0, (-111i32) as u32); // memWZErr
                    } else {
                        let the_zone = bus.read_long(0x0118); // TheZone low-mem global
                        cpu.write_reg(Register::A0, the_zone);
                        cpu.write_reg(Register::D0, 0); // noErr
                    }
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
            // MemoryDispatch ($A05C) / MemoryDispatchA0Result ($A15C)
            // Tracks virtual-memory page state and reports physical mappings.
            // D0: selector/result; A0: address/table/result; A1: count.
            // Inside Macintosh: Memory (1992), pp. 3-25 to 3-32.
            (false, 0x5C) => {
                let selector = cpu.read_reg(Register::D0);
                let operation = memory_dispatch_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
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
pub(crate) fn mac_roman_to_upper(ch: u8, strip_marks: bool) -> u8 {
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
pub(crate) fn mac_roman_strip_diacriticals(ch: u8) -> u8 {
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
mod tests;
