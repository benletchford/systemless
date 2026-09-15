//! Shared event and time polling support for PowerPC import dispatch.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcTickCountPollFingerprint {
    gpr: [u32; 32],
    fpr: [u64; 32],
    cr: u32,
    lr: u32,
    ctr: u32,
    xer: u32,
    fpscr: u32,
    msr: u32,
}

impl PpcTickCountPollFingerprint {
    fn capture(cpu: &PpcCpu) -> Self {
        let mut gpr = cpu.gpr;
        // TickCount returns its value in r3, so that result is expected to
        // differ when the clock changes and is not caller-owned loop state.
        gpr[3] = 0;
        Self {
            gpr,
            fpr: cpu.fpr,
            cr: cpu.cr,
            lr: cpu.lr,
            ctr: cpu.ctr,
            xer: cpu.xer,
            fpscr: cpu.fpscr,
            msr: cpu.msr,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct PpcTickCountIdlePollState {
    pub(super) context: Option<(u32, u32, PpcTickCountPollFingerprint)>,
    pub(super) repeat_count: u32,
}

impl PpcTickCountIdlePollState {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

pub(super) fn dispatch_tick_count_import(
    cpu: &PpcCpu,
    tick_count: u32,
    cycles_until_boundary_or_limit: u64,
    idle_poll: Option<&mut PpcTickCountIdlePollState>,
) -> PpcImportAction {
    // Inside Macintosh: Processes (1993), p. 3-46: TickCount changes when the
    // vertical-retrace clock advances. Repeated reads from one return address
    // within the same tick are therefore equivalent until the next boundary.
    let Some(idle_poll) = idle_poll else {
        return PpcImportAction::Return(tick_count);
    };
    if cpu.lr == 0 {
        idle_poll.reset();
        return PpcImportAction::Return(tick_count);
    }

    let context = (
        cpu.lr,
        tick_count,
        PpcTickCountPollFingerprint::capture(cpu),
    );
    if idle_poll.context != Some(context) {
        idle_poll.context = Some(context);
        idle_poll.repeat_count = 1;
        return PpcImportAction::Return(tick_count);
    }
    idle_poll.repeat_count = idle_poll.repeat_count.saturating_add(1);
    if idle_poll.repeat_count > PPC_TICK_COUNT_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        // The import itself costs one cycle. Charge only the remainder so the
        // execution slice ends exactly at the next tick and the runner can fire
        // its VBL, timer, and sound work before foreground code resumes.
        PpcImportAction::ReturnWithExtraCycles(
            tick_count,
            cycles_until_boundary_or_limit.saturating_sub(1),
        )
    } else {
        PpcImportAction::Return(tick_count)
    }
}

pub(super) fn dispatch_getkeys_import(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    input: PpcInputSnapshot,
    idle_poll_counts: Option<&mut HashMap<u32, u32>>,
) -> PpcImportAction {
    let key_map_ptr = cpu.gpr[3];
    if key_map_ptr != 0 {
        let _ = memory.write_bytes(key_map_ptr, &input.key_map[..PPC_KEY_MAP_SIZE as usize]);
    }
    let Some(idle_poll_counts) = idle_poll_counts else {
        return PpcImportAction::ReturnPreserve;
    };
    if input.key_map.iter().any(|byte| *byte != 0) || cpu.lr == 0 {
        idle_poll_counts.clear();
        return PpcImportAction::ReturnPreserve;
    }
    let count = idle_poll_counts.entry(cpu.lr).or_default();
    *count = count.saturating_add(1);
    if *count > PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        PpcImportAction::ReturnPreserveWithExtraCycles(PPC_GETKEYS_IDLE_POLL_EXTRA_CYCLES)
    } else {
        PpcImportAction::ReturnPreserve
    }
}

pub(super) fn dispatch_button_import(
    cpu: &PpcCpu,
    input: PpcInputSnapshot,
    idle_poll_counts: Option<&mut HashMap<u32, u32>>,
) -> PpcImportAction {
    let result = u32::from(input.mouse_button);
    let Some(idle_poll_counts) = idle_poll_counts else {
        return PpcImportAction::Return(result);
    };
    if input.mouse_button || cpu.lr == 0 {
        idle_poll_counts.clear();
        return PpcImportAction::Return(result);
    }
    let count = idle_poll_counts.entry(cpu.lr).or_default();
    *count = count.saturating_add(1);
    if *count > PPC_BUTTON_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        PpcImportAction::ReturnWithExtraCycles(result, PPC_BUTTON_IDLE_POLL_EXTRA_CYCLES)
    } else {
        PpcImportAction::Return(result)
    }
}

pub(super) fn ppc_still_down_result(
    input: PpcInputSnapshot,
    event_queue: &VecDeque<PpcQueuedEvent>,
) -> bool {
    let has_pending_mouse_event = event_queue.iter().any(|event| matches!(event.what, 1 | 2));
    input.mouse_button && !has_pending_mouse_event
}

pub(super) fn ppc_wait_mouse_up_result(
    input: PpcInputSnapshot,
    event_queue: &mut EventQueue,
) -> bool {
    let still_down = ppc_still_down_result(input, event_queue);
    if !still_down {
        if let Some(index) = event_queue.iter().position(|event| event.what == 2) {
            event_queue.remove(index);
        }
    }
    still_down
}

pub(super) fn dispatch_still_down_import(
    cpu: &PpcCpu,
    input: PpcInputSnapshot,
    event_queue: &VecDeque<PpcQueuedEvent>,
    idle_poll_counts: Option<&mut HashMap<u32, u32>>,
) -> PpcImportAction {
    let result = u32::from(ppc_still_down_result(input, event_queue));
    let Some(idle_poll_counts) = idle_poll_counts else {
        return PpcImportAction::Return(result);
    };
    if result != 0 || cpu.lr == 0 {
        idle_poll_counts.clear();
        return PpcImportAction::Return(result);
    }
    let count = idle_poll_counts.entry(cpu.lr).or_default();
    *count = count.saturating_add(1);
    if *count > PPC_BUTTON_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        PpcImportAction::ReturnWithExtraCycles(result, PPC_BUTTON_IDLE_POLL_EXTRA_CYCLES)
    } else {
        PpcImportAction::Return(result)
    }
}

pub(super) fn dispatch_microseconds_import(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    microseconds: u64,
    idle_poll_counts: Option<&mut HashMap<u32, u32>>,
) -> PpcImportAction {
    // Microseconds
    // Returns the number of microseconds elapsed since system startup.
    // PROCEDURE Microseconds (VAR microTickCount: UnsignedWide);
    // Inside Macintosh: Operating System Utilities (1994), p. 4-49.
    ppc_write_microseconds_value(cpu, memory, microseconds);
    let Some(idle_poll_counts) = idle_poll_counts else {
        return PpcImportAction::ReturnPreserve;
    };
    if cpu.lr == 0 {
        idle_poll_counts.clear();
        return PpcImportAction::ReturnPreserve;
    }
    let count = idle_poll_counts.entry(cpu.lr).or_default();
    *count = count.saturating_add(1);
    if *count > PPC_MICROSECONDS_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        // A repeated caller within one execution slice is polling elapsed
        // time rather than sampling an application event. Charge equivalent
        // guest cycles so the clock remains monotonic without interpreting
        // thousands of iterations of the wait loop.
        PpcImportAction::ReturnPreserveWithExtraCycles(PPC_MICROSECONDS_IDLE_POLL_EXTRA_CYCLES)
    } else {
        PpcImportAction::ReturnPreserve
    }
}

fn ppc_write_microseconds(cpu: &PpcCpu, memory: &mut PpcSectionMem, tick_count: u32) {
    ppc_write_microseconds_value(
        cpu,
        memory,
        u64::from(tick_count).saturating_mul(PPC_MICROSECONDS_PER_TICK),
    );
}

fn ppc_write_microseconds_value(cpu: &PpcCpu, memory: &mut PpcSectionMem, usecs: u64) {
    let microseconds_ptr = cpu.gpr[3];
    if microseconds_ptr == 0 || !ppc_memory_can_write_bytes(memory, microseconds_ptr, 8) {
        return;
    }
    let _ = memory.write_u32_be(microseconds_ptr, (usecs >> 32) as u32);
    let _ = memory.write_u32_be(microseconds_ptr + 4, usecs as u32);
}

fn ppc_seconds_to_date(memory: &mut PpcSectionMem, seconds: u32, date_ptr: u32) {
    if date_ptr == 0 || !ppc_memory_can_write_bytes(memory, date_ptr, 14) {
        return;
    }

    let mut remaining_days = seconds / 86_400;
    let seconds_today = seconds % 86_400;
    let mut year = 1904u16;
    loop {
        let days_this_year = if ppc_is_gregorian_leap_year(year) {
            366
        } else {
            365
        };
        if remaining_days < days_this_year {
            break;
        }
        remaining_days -= days_this_year;
        year += 1;
    }

    let mut month = 1u16;
    loop {
        let days_this_month = ppc_days_in_gregorian_month(year, month);
        if remaining_days < days_this_month {
            break;
        }
        remaining_days -= days_this_month;
        month += 1;
    }

    let days_since_epoch = seconds / 86_400;
    let fields = [
        year,
        month,
        remaining_days as u16 + 1,
        (seconds_today / 3_600) as u16,
        ((seconds_today % 3_600) / 60) as u16,
        (seconds_today % 60) as u16,
        ((days_since_epoch + 5) % 7) as u16 + 1,
    ];

    // Inside Macintosh: Operating System Utilities (1994), pp. 4-23–4-25 and 4-38:
    // SecondsToDate converts seconds since 1904-01-01 into the seven signed,
    // big-endian DateTimeRec fields, with Sunday numbered 1 through Saturday 7.
    for (index, field) in fields.into_iter().enumerate() {
        let _ = memory.write_u16_be(date_ptr + index as u32 * 2, field);
    }
}

fn ppc_is_gregorian_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn ppc_days_in_gregorian_month(year: u16, month: u16) -> u32 {
    match month {
        2 if ppc_is_gregorian_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

pub(super) struct PpcTimeDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) tick_count: u32,
    pub(super) cycles_per_tick: u32,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_time_import(
    context: PpcTimeDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcTimeDispatchContext {
        target,
        cpu,
        memory,
        tick_count,
        cycles_per_tick,
        toolbox_startup,
    } = context;
    match target {
        PpcImportDispatcherTarget::TickCount => Some(PpcImportAction::Return(tick_count)),
        PpcImportDispatcherTarget::GetDateTime => {
            let secs_ptr = cpu.gpr[3];
            if secs_ptr != 0 && ppc_memory_can_write_bytes(memory, secs_ptr, 4) {
                let _ = memory.write_u32_be(secs_ptr, PPC_FIXED_MAC_TIME);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ReadDateTime => {
            let secs_ptr = cpu.gpr[3];
            let result = if secs_ptr != 0 && ppc_memory_can_write_bytes(memory, secs_ptr, 4) {
                let _ = memory.write_u32_be(secs_ptr, PPC_FIXED_MAC_TIME);
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::ReadLocation => {
            // Operating System Utilities (1994), pp. 4-29 and 4-46: an
            // unset 12-byte MachineLocation record reads as all zeroes.
            let location = cpu.gpr[3];
            if location != 0 && ppc_memory_can_write_bytes(memory, location, 12) {
                let _ = memory.write_bytes(location, &[0; 12]);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetTime => {
            ppc_seconds_to_date(memory, PPC_FIXED_MAC_TIME, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::Delay => {
            // Inside Macintosh Volume II (1985), p. II-384: Delay blocks for
            // numTicks vertical-retrace ticks and returns the ending system
            // tick in finalTicks. Yielding keeps the native call parked while
            // the frontend advances the emulated VBL clock.
            let deadline = toolbox_startup
                .delay_deadline
                .get_or_insert_with(|| tick_count.wrapping_add(cpu.gpr[3]));
            let reached = tick_count.wrapping_sub(*deadline) < 0x8000_0000;
            if cpu.gpr[3] == 0 || reached {
                if cpu.gpr[4] != 0 {
                    let _ = memory.write_u32_be(cpu.gpr[4], tick_count);
                }
                toolbox_startup.delay_deadline = None;
                Some(PpcImportAction::ReturnPreserve)
            } else {
                Some(PpcImportAction::Yield(u64::from(cycles_per_tick.max(1))))
            }
        }
        PpcImportDispatcherTarget::GetDblTime => Some(PpcImportAction::Return(
            memory
                .read_u32_be(crate::memory::globals::addr::DOUBLE_TIME)
                .unwrap_or(PPC_DEFAULT_DOUBLE_TIME_TICKS),
        )),
        PpcImportDispatcherTarget::LMGetTime => {
            // Universal Interfaces 3.4 LowMem.h declares LMGetTime as the
            // accessor for the UInt32 Time low-memory global at $020C.
            Some(PpcImportAction::Return(PPC_FIXED_MAC_TIME))
        }
        PpcImportDispatcherTarget::SecondsToDate => {
            ppc_seconds_to_date(memory, cpu.gpr[3], cpu.gpr[4]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::Microseconds => {
            ppc_write_microseconds(cpu, memory, tick_count);
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}

pub(super) struct PpcEventDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) handles: &'a [PpcHandleRecord],
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) current_menu_list: u32,
    pub(super) screen_clut: &'a [[u16; 3]; 256],
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) apple_events: &'a mut PpcAppleEventState,
    pub(super) event_queue: &'a mut EventQueue,
    pub(super) input: PpcInputSnapshot,
    pub(super) tick_count: u32,
}

pub(super) fn dispatch_event_import(
    context: PpcEventDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcEventDispatchContext {
        binding,
        cpu,
        memory,
        handles,
        gworlds,
        current_menu_list,
        screen_clut,
        toolbox_startup,
        apple_events,
        event_queue,
        input,
        tick_count,
    } = context;
    match binding.dispatcher_target {
        PpcImportDispatcherTarget::FlushEvents => {
            toolbox_startup.flush_events_count =
                toolbox_startup.flush_events_count.saturating_add(1);
            toolbox_startup.last_flush_event_mask = cpu.gpr[3] as u16;
            toolbox_startup.last_flush_stop_mask = cpu.gpr[4] as u16;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetEventMask => {
            // Macintosh Toolbox Essentials (1992), pp. 2-99--2-100: this is
            // the current process's OS Event Manager posting state. Universal
            // Interfaces 3.4 LowMem.h exposes the same word directly at
            // SysEvtMask ($0144), so the guest-visible bytes are authoritative.
            let _ = memory.write_u16_be(
                crate::memory::globals::addr::SYS_EVT_MASK,
                cpu.gpr[3] as u16,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetNextEvent(_) | PpcImportDispatcherTarget::GetOSEvent => {
            let event_mask = cpu.gpr[3] as u16;
            let event_ptr = cpu.gpr[4];
            let sleep_ticks = cpu.gpr[5];
            let os_only = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::GetOSEvent
            );
            if !os_only {
                ppc_service_invalid_menu_bar(
                    event_queue,
                    memory,
                    handles,
                    gworlds,
                    current_menu_list,
                    screen_clut,
                    toolbox_startup,
                );
                ppc_enqueue_open_application_event_if_needed(
                    apple_events,
                    event_queue,
                    event_mask,
                    tick_count,
                );
            }
            let (what, message, when, where_v, where_h, modifiers, has_event) =
                ppc_dequeue_event(event_queue, event_mask, input, os_only, tick_count);
            if has_event && what == 8 {
                let pending = if (modifiers & 1) != 0 { 0x0A64 } else { 0x0A68 };
                if memory.read_u32_be(pending) == Some(message) {
                    let _ = memory.write_u32_be(pending, 0);
                }
            }
            if crate::trap::dispatch::trace_input_enabled() {
                eprintln!(
                    "[INPUT] PPC {} mask=${event_mask:04X} event_ptr=${event_ptr:08X} sleep={} -> has_event={} what={} message=${message:08X} where=({}, {}) modifiers=${modifiers:04X}",
                    binding.symbol_name, sleep_ticks, has_event, what, where_v, where_h,
                );
            }
            if event_ptr != 0
                && ppc_write_event_record(
                    memory, event_ptr, what, message, when, where_v, where_h, modifiers,
                )
            {
                ppc_record_event_snapshot(
                    toolbox_startup,
                    what,
                    message,
                    when,
                    where_v,
                    where_h,
                    modifiers,
                );
            }
            if os_only {
                toolbox_startup.event_queue_probe.get_os_event = Some(ppc_event_probe_result(
                    has_event, what, message, when, where_v, where_h, modifiers,
                ));
            }
            let action = PpcImportAction::Return(u32::from(has_event));
            if matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent)
            ) && !has_event
                && sleep_ticks > 0
            {
                Some(ppc_import_action_with_extra_cycles(
                    action,
                    u64::from(sleep_ticks)
                        .saturating_mul(PPC_Q3_IDLE_STATE_ONLY_FRAME_EXTRA_CYCLES),
                ))
            } else {
                Some(action)
            }
        }
        PpcImportDispatcherTarget::EventAvail | PpcImportDispatcherTarget::OSEventAvail => {
            let event_mask = cpu.gpr[3] as u16;
            let event_ptr = cpu.gpr[4];
            let os_only = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::OSEventAvail
            );
            if !os_only {
                ppc_service_invalid_menu_bar(
                    event_queue,
                    memory,
                    handles,
                    gworlds,
                    current_menu_list,
                    screen_clut,
                    toolbox_startup,
                );
                ppc_enqueue_open_application_event_if_needed(
                    apple_events,
                    event_queue,
                    event_mask,
                    tick_count,
                );
            }
            let (what, message, when, where_v, where_h, modifiers, has_event) =
                ppc_peek_event(event_queue, event_mask, input, os_only, tick_count);
            if event_ptr != 0
                && ppc_write_event_record(
                    memory, event_ptr, what, message, when, where_v, where_h, modifiers,
                )
            {
                ppc_record_event_snapshot(
                    toolbox_startup,
                    what,
                    message,
                    when,
                    where_v,
                    where_h,
                    modifiers,
                );
            }
            let snapshot = ppc_event_probe_result(
                has_event, what, message, when, where_v, where_h, modifiers,
            );
            if os_only {
                toolbox_startup.event_queue_probe.os_event_avail = Some(snapshot);
            } else {
                toolbox_startup.event_queue_probe.event_avail = Some(snapshot);
            }
            Some(PpcImportAction::Return(u32::from(has_event)))
        }
        PpcImportDispatcherTarget::PostEvent => {
            let what = cpu.gpr[3] as u16;
            let system_event_mask = memory
                .read_u16_be(crate::memory::globals::addr::SYS_EVT_MASK)
                .unwrap_or(crate::memory::globals::DEFAULT_SYS_EVT_MASK);
            let result =
                if crate::trap::TrapDispatcher::posted_event_is_enabled(system_event_mask, what) {
                    event_queue.push_back(PpcQueuedEvent {
                        what,
                        message: cpu.gpr[4],
                        when: tick_count,
                        where_v: input.mouse_v,
                        where_h: input.mouse_h,
                        modifiers: ppc_current_event_modifiers(input),
                    });
                    PPC_NO_ERR
                } else {
                    PPC_EVT_NOT_ENB
                };
            toolbox_startup.event_queue_probe.post_result = Some(result);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::Button => {
            toolbox_startup.last_button_result = Some(input.mouse_button);
            Some(PpcImportAction::Return(u32::from(input.mouse_button)))
        }
        PpcImportDispatcherTarget::StillDown => {
            toolbox_startup.last_still_down_result = Some(ppc_still_down_result(input, event_queue));
            Some(dispatch_still_down_import(cpu, input, event_queue, None))
        }
        PpcImportDispatcherTarget::WaitMouseUp => {
            let result = ppc_wait_mouse_up_result(input, event_queue);
            toolbox_startup.last_wait_mouse_up_result = Some(result);
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::GetKeys => {
            Some(dispatch_getkeys_import(cpu, memory, input, None))
        }
        _ => None,
    }
}
