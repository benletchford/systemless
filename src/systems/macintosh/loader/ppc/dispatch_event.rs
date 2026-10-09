//! Shared event and time polling support for PowerPC import dispatch.

use super::*;

// EventQueueRef is an opaque Carbon handle. The HLE has one application event
// queue, so this stable non-null token identifies that queue to guest calls.
pub(super) const PPC_MAIN_EVENT_QUEUE_REF: u32 = 1;
pub(super) const PPC_MAIN_EVENT_LOOP_REF: u32 = 2;
pub(super) const PPC_APPLICATION_EVENT_TARGET_REF: u32 = 3;
pub(super) const PPC_EVENT_DISPATCHER_TARGET_REF: u32 = 4;
const PPC_EVENT_LOOP_TIMED_OUT_ERR: i16 = -9875;
const PPC_EVENT_NOT_HANDLED_ERR: i16 = -9874;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcCarbonEventHandlerRecord {
    pub(super) handler_ref: u32,
    pub(super) target: u32,
    pub(super) callback: PpcCallbackTarget,
    pub(super) user_data: u32,
    pub(super) event_types: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcCarbonEventRecord {
    pub(super) event_ref: u32,
    pub(super) event_class: u32,
    pub(super) event_kind: u32,
    pub(super) time_bits: u64,
    pub(super) attributes: u32,
    pub(super) reference_count: u32,
    pub(super) parameters: Vec<PpcCarbonEventParameterRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcCarbonEventParameterRecord {
    pub(super) name: u32,
    pub(super) type_code: u32,
    pub(super) data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PpcCarbonEventDispatchOrigin {
    Send,
    CallNext,
    ApplicationLoop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcCarbonEventDispatchRecord {
    pub(super) origin: PpcCarbonEventDispatchOrigin,
    pub(super) import_pc: u32,
    pub(super) return_pc: u32,
    pub(super) restore_rtoc: u32,
    pub(super) event_ref: u32,
    pub(super) handlers: Vec<PpcCarbonEventHandlerRecord>,
    pub(super) next_index: usize,
    pub(super) active_call_ref: u32,
    pub(super) delegated: bool,
}

fn ppc_call_next_carbon_event_handler(
    cpu: &mut PpcCpu,
    dispatch: &mut PpcCarbonEventDispatchRecord,
    next_call_ref: &mut u32,
) -> Option<PpcImportAction> {
    let handler = dispatch.handlers.get(dispatch.next_index)?;
    let call_ref = *next_call_ref;
    *next_call_ref = call_ref.checked_add(4)?;
    dispatch.next_index += 1;
    dispatch.active_call_ref = call_ref;
    dispatch.delegated = false;
    cpu.gpr[3] = call_ref;
    cpu.gpr[4] = dispatch.event_ref;
    cpu.gpr[5] = handler.user_data;
    Some(PpcImportAction::CallNative {
        entry: handler.callback.entry,
        rtoc: handler.callback.rtoc,
        return_pc: dispatch.import_pc,
        final_pc: dispatch.import_pc,
        restore_rtoc: dispatch.restore_rtoc,
        return_gpr3: PpcNativeReturnGpr3::Preserve,
    })
}

fn ppc_carbon_matching_handlers(
    toolbox_startup: &PpcToolboxStartupState,
    event_ref: u32,
    target: u32,
) -> Option<Vec<PpcCarbonEventHandlerRecord>> {
    let event = toolbox_startup
        .carbon_events
        .iter()
        .find(|event| event.event_ref == event_ref)?;
    let event_type = (event.event_class, event.event_kind);
    let mut handlers = Vec::new();
    if target == PPC_EVENT_DISPATCHER_TARGET_REF {
        handlers.extend(
            toolbox_startup
                .carbon_event_handlers
                .iter()
                .rev()
                .filter(|handler| {
                    handler.target == PPC_EVENT_DISPATCHER_TARGET_REF
                        && handler.event_types.contains(&event_type)
                })
                .cloned(),
        );
    }
    handlers.extend(
        toolbox_startup
            .carbon_event_handlers
            .iter()
            .rev()
            .filter(|handler| {
                handler.target == PPC_APPLICATION_EVENT_TARGET_REF
                    && handler.event_types.contains(&event_type)
            })
            .cloned(),
    );
    Some(handlers)
}

fn ppc_resume_carbon_event_dispatch(
    cpu: &mut PpcCpu,
    toolbox_startup: &mut PpcToolboxStartupState,
    origin: PpcCarbonEventDispatchOrigin,
) -> Option<PpcImportAction> {
    let dispatch = toolbox_startup.carbon_event_dispatch_stack.last_mut()?;
    if dispatch.origin != origin || dispatch.import_pc != cpu.pc || cpu.lr != cpu.pc {
        return None;
    }
    let status = cpu.gpr[3];
    if status == ppc_i16_result(PPC_EVENT_NOT_HANDLED_ERR) && !dispatch.delegated {
        if let Some(action) = ppc_call_next_carbon_event_handler(
            cpu,
            dispatch,
            &mut toolbox_startup.next_carbon_event_call_ref,
        ) {
            return Some(action);
        }
    }
    let dispatch = toolbox_startup.carbon_event_dispatch_stack.pop().unwrap();
    cpu.lr = dispatch.return_pc;
    Some(PpcImportAction::Return(status))
}

fn ppc_release_carbon_event(toolbox_startup: &mut PpcToolboxStartupState, event_ref: u32) {
    if let Some(index) = toolbox_startup
        .carbon_events
        .iter()
        .position(|event| event.event_ref == event_ref)
    {
        let event = &mut toolbox_startup.carbon_events[index];
        event.reference_count -= 1;
        if event.reference_count == 0 {
            toolbox_startup.carbon_events.remove(index);
        }
    }
}

// Carbon Event Manager Programming Guide (2005), "Installing Timers": a
// timer belongs to an event loop, fires only while that loop is running, and
// repeats after its callback unless its interval is zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcEventLoopTimerRecord {
    pub(super) timer_ref: u32,
    pub(super) callback: u32,
    pub(super) user_data: u32,
    pub(super) next_fire_tick: Option<u32>,
    pub(super) interval_ticks: u32,
}

fn ppc_event_timer_interval_ticks(seconds: f64) -> Option<Option<u32>> {
    if seconds == -1.0 {
        return Some(None); // kEventDurationForever
    }
    if !seconds.is_finite() || seconds < 0.0 || seconds > f64::from(u32::MAX) / 60.0 {
        return None;
    }
    Some(Some((seconds * 60.0).ceil() as u32))
}

pub(super) fn ppc_tick_is_due(current: u32, deadline: u32) -> bool {
    current.wrapping_sub(deadline) < 0x8000_0000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcEventPollOperation {
    GetNextEvent,
    WaitNextEvent,
}

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
        crate::time_manager::evaluate_microseconds(tick_count),
    );
}

fn ppc_write_microseconds_value(cpu: &PpcCpu, memory: &mut PpcSectionMem, usecs: u64) {
    let microseconds_ptr = cpu.gpr[3];
    if microseconds_ptr == 0 || !ppc_memory_can_write_bytes(memory, microseconds_ptr, 8) {
        return;
    }
    let _ = memory.write_u64_be(microseconds_ptr, usecs);
}

fn ppc_seconds_to_date(memory: &mut PpcSectionMem, seconds: u32, date_ptr: u32) {
    if date_ptr == 0 || !ppc_memory_can_write_bytes(memory, date_ptr, crate::time_manager::DATE_TIME_REC_SIZE as u32) {
        return;
    }

    let rec = crate::time_manager::evaluate_seconds_to_date(seconds);
    let fields = [
        rec.year,
        rec.month,
        rec.day,
        rec.hour,
        rec.minute,
        rec.second,
        rec.day_of_week,
    ];

    // Inside Macintosh: Operating System Utilities (1994), pp. 4-23–4-25 and 4-38:
    // SecondsToDate converts seconds since 1904-01-01 into the seven signed,
    // big-endian DateTimeRec fields, with Sunday numbered 1 through Saturday 7.
    for (index, field) in fields.into_iter().enumerate() {
        let _ = memory.write_u16_be(date_ptr + index as u32 * 2, field);
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

pub(super) fn dispatch_time_import(context: PpcTimeDispatchContext<'_>) -> Option<PpcImportAction> {
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
            let params = crate::time_manager::evaluate_delay_parameters(cpu.gpr[3], cpu.gpr[4], tick_count);
            let deadline = toolbox_startup
                .delay_deadline
                .get_or_insert_with(|| params.deadline());
            let reached = params.is_reached(*deadline);
            if params.is_immediate() || reached {
                if params.final_ticks_ptr != 0 {
                    let _ = memory.write_u32_be(params.final_ticks_ptr, tick_count);
                }
                toolbox_startup.delay_deadline = None;
                Some(PpcImportAction::ReturnPreserve)
            } else {
                Some(PpcImportAction::Yield(u64::from(cycles_per_tick.max(1))))
            }
        }
        // GetCaretTime: Toolbox Essentials (1992), p. 2-113.
        // FUNCTION GetCaretTime: LongInt; reads the live CaretTime global.
        PpcImportDispatcherTarget::GetCaretTime => Some(PpcImportAction::Return(
            memory.read_u32_be(crate::memory::globals::addr::CARET_TIME)
                .unwrap_or(crate::memory::globals::DEFAULT_CARET_TIME_TICKS),
        )),
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
    pub(super) current_gworld: u32,
    pub(super) current_menu_list: u32,
    pub(super) screen_clut: &'a [[u16; 3]; 256],
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) apple_events: &'a mut PpcAppleEventState,
    pub(super) event_queue: &'a mut EventQueue,
    pub(super) input: PpcInputSnapshot,
    pub(super) tick_count: u32,
    pub(super) process_mode: u32,
    pub(super) window_list: &'a SharedProcessWindowList,
    pub(super) dialog_callback_active: bool,
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
        current_gworld,
        current_menu_list,
        screen_clut,
        toolbox_startup,
        apple_events,
        event_queue,
        input,
        tick_count,
        process_mode,
        window_list,
        dialog_callback_active,
    } = context;
    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetMainEventLoop => {
            // CarbonEvents.h: each application has one main EventLoopRef.
            Some(PpcImportAction::Return(PPC_MAIN_EVENT_LOOP_REF))
        }
        PpcImportDispatcherTarget::GetApplicationEventTarget => {
            Some(PpcImportAction::Return(PPC_APPLICATION_EVENT_TARGET_REF))
        }
        PpcImportDispatcherTarget::GetEventDispatcherTarget => {
            Some(PpcImportAction::Return(PPC_EVENT_DISPATCHER_TARGET_REF))
        }
        PpcImportDispatcherTarget::InstallEventLoopTimer => {
            // CarbonEvents.h (QuickTime 6.0.2): OSStatus InstallEventLoopTimer(
            // EventLoopRef, EventTimerInterval, EventTimerInterval,
            // EventLoopTimerUPP, void *, EventLoopTimerRef *). The two double
            // arguments consume PPC integer parameter slots r4-r7, so the
            // trailing pointers arrive in r8-r10.
            let loop_ref = cpu.gpr[3];
            let first_fire = f64::from_bits(cpu.fpr[1]);
            let interval = f64::from_bits(cpu.fpr[2]);
            let callback = cpu.gpr[8];
            let user_data = cpu.gpr[9];
            let out_ref = cpu.gpr[10];
            let Some(interval_ticks) = ppc_event_timer_interval_ticks(interval) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            let interval_ticks = interval_ticks.unwrap_or(0);
            let Some(first_fire_ticks) = ppc_event_timer_interval_ticks(first_fire) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            if loop_ref != PPC_MAIN_EVENT_LOOP_REF
                || callback == 0
                || (out_ref != 0 && !ppc_memory_can_write_bytes(memory, out_ref, 4))
            {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            let timer_ref = toolbox_startup.next_event_loop_timer_ref;
            toolbox_startup.next_event_loop_timer_ref = timer_ref.wrapping_add(4).max(0x100);
            toolbox_startup
                .event_loop_timers
                .push(PpcEventLoopTimerRecord {
                    timer_ref,
                    callback,
                    user_data,
                    next_fire_tick: first_fire_ticks.map(|ticks| tick_count.wrapping_add(ticks)),
                    interval_ticks,
                });
            if out_ref != 0 {
                let _ = memory.write_u32_be(out_ref, timer_ref);
            }
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::RemoveEventLoopTimer => {
            // CarbonEvents.h: removal invalidates the opaque timer reference.
            let timer_ref = cpu.gpr[3];
            let Some(index) = toolbox_startup
                .event_loop_timers
                .iter()
                .position(|timer| timer.timer_ref == timer_ref)
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            toolbox_startup.event_loop_timers.remove(index);
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::InstallEventHandler => {
            // CarbonEvents.h: an EventTypeSpec is two UInt32 fields. Copy the
            // caller's list because it may be stack storage that disappears
            // before the handler receives an event.
            let target = cpu.gpr[3];
            let callback_ptr = cpu.gpr[4];
            let count = cpu.gpr[5];
            let type_list = cpu.gpr[6];
            let user_data = cpu.gpr[7];
            let out_ref = cpu.gpr[8];
            if !matches!(
                target,
                PPC_APPLICATION_EVENT_TARGET_REF | PPC_EVENT_DISPATCHER_TARGET_REF
            ) || count > 4096
                || (count != 0 && type_list == 0)
                || (out_ref != 0 && !ppc_memory_can_write_bytes(memory, out_ref, 4))
            {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            let Some(callback) =
                ppc_resolve_callback_target(memory, callback_ptr, cpu.gpr[2], None)
                    .filter(|callback| memory.read_u32_be(callback.entry).is_some())
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            let mut event_types = Vec::with_capacity(count as usize);
            for index in 0..count {
                let Some(address) = type_list.checked_add(index * 8) else {
                    return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
                };
                let Some(kind_address) = address.checked_add(4) else {
                    return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
                };
                let (Some(event_class), Some(event_kind)) = (
                    memory.read_u32_be(address),
                    memory.read_u32_be(kind_address),
                ) else {
                    return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
                };
                event_types.push((event_class, event_kind));
            }
            let handler_ref = toolbox_startup.next_carbon_event_handler_ref;
            let Some(next_ref) = handler_ref.checked_add(4) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            toolbox_startup.next_carbon_event_handler_ref = next_ref;
            toolbox_startup
                .carbon_event_handlers
                .push(PpcCarbonEventHandlerRecord {
                    handler_ref,
                    target,
                    callback,
                    user_data,
                    event_types,
                });
            if out_ref != 0 {
                let _ = memory.write_u32_be(out_ref, handler_ref);
            }
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::RemoveEventHandler => {
            let Some(index) = toolbox_startup
                .carbon_event_handlers
                .iter()
                .position(|handler| handler.handler_ref == cpu.gpr[3])
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            toolbox_startup.carbon_event_handlers.remove(index);
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::CreateEvent => {
            // The EventTime double occupies f1 and PPC argument slots r6-r7.
            let when = f64::from_bits(cpu.fpr[1]);
            let out_ref = cpu.gpr[9];
            if !when.is_finite()
                || (when < 0.0 && when != -1.0)
                || out_ref == 0
                || !ppc_memory_can_write_bytes(memory, out_ref, 4)
            {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            let event_ref = toolbox_startup.next_carbon_event_ref;
            let Some(next_ref) = event_ref.checked_add(4) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            toolbox_startup.next_carbon_event_ref = next_ref;
            toolbox_startup.carbon_events.push(PpcCarbonEventRecord {
                event_ref,
                event_class: cpu.gpr[4],
                event_kind: cpu.gpr[5],
                time_bits: if when == 0.0 {
                    (f64::from(tick_count) / 60.0).to_bits()
                } else {
                    when.to_bits()
                },
                attributes: cpu.gpr[8],
                reference_count: 1,
                parameters: Vec::new(),
            });
            let _ = memory.write_u32_be(out_ref, event_ref);
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::ReleaseEvent => {
            ppc_release_carbon_event(toolbox_startup, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RetainEvent => {
            let event_ref = cpu.gpr[3];
            if let Some(event) = toolbox_startup
                .carbon_events
                .iter_mut()
                .find(|event| event.event_ref == event_ref)
            {
                if let Some(reference_count) = event.reference_count.checked_add(1) {
                    event.reference_count = reference_count;
                    Some(PpcImportAction::Return(event_ref))
                } else {
                    Some(PpcImportAction::Return(0))
                }
            } else {
                Some(PpcImportAction::Return(0))
            }
        }
        PpcImportDispatcherTarget::GetEventClass => Some(PpcImportAction::Return(
            toolbox_startup
                .carbon_events
                .iter()
                .find(|event| event.event_ref == cpu.gpr[3])
                .map_or(0, |event| event.event_class),
        )),
        PpcImportDispatcherTarget::GetEventKind => Some(PpcImportAction::Return(
            toolbox_startup
                .carbon_events
                .iter()
                .find(|event| event.event_ref == cpu.gpr[3])
                .map_or(0, |event| event.event_kind),
        )),
        PpcImportDispatcherTarget::GetEventTime => {
            cpu.fpr[1] = toolbox_startup
                .carbon_events
                .iter()
                .find(|event| event.event_ref == cpu.gpr[3])
                .map_or(0.0f64.to_bits(), |event| event.time_bits);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetEventParameter => {
            // CarbonEventsCore.h, SetEventParameter: the event owns a copy of
            // the caller's bytes, and a second write replaces the named value.
            let (event_ref, name, type_code, size, data_ptr) =
                (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7]);
            if size != 0 && (data_ptr == 0 || !ppc_memory_can_read_bytes(memory, data_ptr, size)) {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            let Some(event) = toolbox_startup
                .carbon_events
                .iter_mut()
                .find(|event| event.event_ref == event_ref)
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            let data = if size == 0 {
                Vec::new()
            } else {
                let Some(data) = ppc_memory_read_bytes(memory, data_ptr, size) else {
                    return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
                };
                data
            };
            if let Some(parameter) = event
                .parameters
                .iter_mut()
                .find(|parameter| parameter.name == name)
            {
                parameter.type_code = type_code;
                parameter.data = data;
            } else {
                event.parameters.push(PpcCarbonEventParameterRecord {
                    name,
                    type_code,
                    data,
                });
            }
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::GetEventParameter => {
            // CarbonEventsCore.h, GetEventParameter: NULL data with a zero
            // buffer size requests metadata only; typeWildCard is '****'.
            let Some(event) = toolbox_startup
                .carbon_events
                .iter()
                .find(|event| event.event_ref == cpu.gpr[3])
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            let Some(parameter) = event
                .parameters
                .iter()
                .find(|parameter| parameter.name == cpu.gpr[4])
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(-9870)));
            };
            let (desired_type, actual_type_ptr, buffer_size, actual_size_ptr, data_ptr) =
                (cpu.gpr[5], cpu.gpr[6], cpu.gpr[7], cpu.gpr[8], cpu.gpr[9]);
            let data_size = parameter.data.len() as u32;
            if (desired_type != u32::from_be_bytes(*b"****") && desired_type != parameter.type_code)
                || (actual_type_ptr != 0 && !ppc_memory_can_write_bytes(memory, actual_type_ptr, 4))
                || (actual_size_ptr != 0 && !ppc_memory_can_write_bytes(memory, actual_size_ptr, 4))
                || (data_ptr == 0 && buffer_size != 0)
                || (data_ptr != 0
                    && (buffer_size < data_size
                        || !ppc_memory_can_write_bytes(memory, data_ptr, data_size)))
            {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            if actual_type_ptr != 0 {
                let _ = memory.write_u32_be(actual_type_ptr, parameter.type_code);
            }
            if actual_size_ptr != 0 {
                let _ = memory.write_u32_be(actual_size_ptr, data_size);
            }
            if data_ptr != 0 {
                let _ = memory.write_bytes(data_ptr, &parameter.data);
            }
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::PostEventToQueue => {
            // CarbonEventsCore.h, PostEventToQueue: the queue retains an
            // event, refuses duplicate posts, and orders by EventPriority.
            let queue_ref = cpu.gpr[3];
            let event_ref = cpu.gpr[4];
            let priority = cpu.gpr[5] as i16;
            if queue_ref != PPC_MAIN_EVENT_QUEUE_REF || !(0..=2).contains(&priority) {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            if toolbox_startup
                .carbon_event_queue
                .iter()
                .any(|(queued_ref, _)| *queued_ref == event_ref)
            {
                return Some(PpcImportAction::Return(ppc_i16_result(-9860)));
            }
            let Some(event) = toolbox_startup
                .carbon_events
                .iter_mut()
                .find(|event| event.event_ref == event_ref)
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            let Some(reference_count) = event.reference_count.checked_add(1) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            event.reference_count = reference_count;
            let index = toolbox_startup
                .carbon_event_queue
                .iter()
                .position(|(_, queued_priority)| *queued_priority < priority)
                .unwrap_or(toolbox_startup.carbon_event_queue.len());
            toolbox_startup
                .carbon_event_queue
                .insert(index, (event_ref, priority));
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::ReceiveNextEvent => {
            // CarbonEventsCore.h, ReceiveNextEvent: the double timeout uses
            // f1 and PPC integer slots r5-r6; pull and result use r7-r8.
            // Pull transfers the queue's retained reference to the caller.
            let count = cpu.gpr[3];
            let type_list = cpu.gpr[4];
            let timeout = f64::from_bits(cpu.fpr[1]);
            let pull = cpu.gpr[7] != 0;
            let out_event = cpu.gpr[8];
            if count > 4096
                || !timeout.is_finite()
                || (timeout < 0.0 && timeout != -1.0)
                || out_event == 0
                || !ppc_memory_can_write_bytes(memory, out_event, 4)
            {
                toolbox_startup.receive_next_event_deadline = None;
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            let mut event_types = Vec::with_capacity(count as usize);
            if type_list != 0 {
                for index in 0..count {
                    let Some(address) = index
                        .checked_mul(8)
                        .and_then(|offset| type_list.checked_add(offset))
                    else {
                        toolbox_startup.receive_next_event_deadline = None;
                        return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
                    };
                    let (Some(class), Some(kind)) = (
                        memory.read_u32_be(address),
                        address
                            .checked_add(4)
                            .and_then(|kind_address| memory.read_u32_be(kind_address)),
                    ) else {
                        toolbox_startup.receive_next_event_deadline = None;
                        return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
                    };
                    event_types.push((class, kind));
                }
            }
            let index = toolbox_startup
                .carbon_event_queue
                .iter()
                .position(|(event_ref, _)| {
                    toolbox_startup
                        .carbon_events
                        .iter()
                        .find(|event| event.event_ref == *event_ref)
                        .is_some_and(|event| {
                            event_types.is_empty()
                                || event_types.contains(&(event.event_class, event.event_kind))
                        })
                });
            if let Some(index) = index {
                let event_ref = toolbox_startup.carbon_event_queue[index].0;
                if pull {
                    toolbox_startup.carbon_event_queue.remove(index);
                }
                toolbox_startup.receive_next_event_deadline = None;
                let _ = memory.write_u32_be(out_event, event_ref);
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)));
            }
            if timeout == 0.0 {
                toolbox_startup.receive_next_event_deadline = None;
                return Some(PpcImportAction::Return(ppc_i16_result(
                    PPC_EVENT_LOOP_TIMED_OUT_ERR,
                )));
            }
            if timeout > 0.0 {
                let remaining = (timeout * 60.0).ceil().max(1.0) as u64;
                let (last_tick, remaining) = match toolbox_startup.receive_next_event_deadline {
                    Some((caller, last_tick, remaining)) if caller == cpu.lr => {
                        let elapsed = u64::from(tick_count.wrapping_sub(last_tick));
                        if elapsed >= remaining {
                            toolbox_startup.receive_next_event_deadline = None;
                            return Some(PpcImportAction::Return(ppc_i16_result(
                                PPC_EVENT_LOOP_TIMED_OUT_ERR,
                            )));
                        }
                        (tick_count, remaining - elapsed)
                    }
                    _ => (tick_count, remaining),
                };
                toolbox_startup.receive_next_event_deadline = Some((cpu.lr, last_tick, remaining));
            } else {
                toolbox_startup.receive_next_event_deadline = None;
            }
            toolbox_startup.event_loop_poll_until_tick = Some(tick_count.wrapping_add(1));
            Some(PpcImportAction::Yield(u64::MAX))
        }
        PpcImportDispatcherTarget::SendEventToEventTarget => {
            if let Some(action) = ppc_resume_carbon_event_dispatch(
                cpu,
                toolbox_startup,
                PpcCarbonEventDispatchOrigin::Send,
            ) {
                return Some(action);
            }
            let event_ref = cpu.gpr[3];
            let target = cpu.gpr[4];
            if !matches!(
                target,
                PPC_APPLICATION_EVENT_TARGET_REF | PPC_EVENT_DISPATCHER_TARGET_REF
            ) {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            // Carbon Event Manager Programming Guide (2005), pp. 10-13:
            // handlers form a last-installed-first stack; the dispatcher
            // propagates an unhandled event to the application target.
            let Some(handlers) = ppc_carbon_matching_handlers(toolbox_startup, event_ref, target)
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            if handlers.is_empty() {
                return Some(PpcImportAction::Return(ppc_i16_result(
                    PPC_EVENT_NOT_HANDLED_ERR,
                )));
            }
            let mut dispatch = PpcCarbonEventDispatchRecord {
                origin: PpcCarbonEventDispatchOrigin::Send,
                import_pc: cpu.pc,
                return_pc: cpu.lr,
                restore_rtoc: cpu.gpr[2],
                event_ref,
                handlers,
                next_index: 0,
                active_call_ref: 0,
                delegated: false,
            };
            let Some(action) = ppc_call_next_carbon_event_handler(
                cpu,
                &mut dispatch,
                &mut toolbox_startup.next_carbon_event_call_ref,
            ) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            toolbox_startup.carbon_event_dispatch_stack.push(dispatch);
            Some(action)
        }
        PpcImportDispatcherTarget::CallNextEventHandler => {
            if let Some(action) = ppc_resume_carbon_event_dispatch(
                cpu,
                toolbox_startup,
                PpcCarbonEventDispatchOrigin::CallNext,
            ) {
                return Some(action);
            }
            let call_ref = cpu.gpr[3];
            let event_ref = cpu.gpr[4];
            let Some(parent) = toolbox_startup
                .carbon_event_dispatch_stack
                .last_mut()
                .filter(|dispatch| {
                    dispatch.active_call_ref == call_ref && dispatch.event_ref == event_ref
                })
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            let handlers = parent.handlers[parent.next_index..].to_vec();
            parent.delegated = true;
            if handlers.is_empty() {
                return Some(PpcImportAction::Return(ppc_i16_result(
                    PPC_EVENT_NOT_HANDLED_ERR,
                )));
            }
            let mut dispatch = PpcCarbonEventDispatchRecord {
                origin: PpcCarbonEventDispatchOrigin::CallNext,
                import_pc: cpu.pc,
                return_pc: cpu.lr,
                restore_rtoc: cpu.gpr[2],
                event_ref,
                handlers,
                next_index: 0,
                active_call_ref: 0,
                delegated: false,
            };
            let Some(action) = ppc_call_next_carbon_event_handler(
                cpu,
                &mut dispatch,
                &mut toolbox_startup.next_carbon_event_call_ref,
            ) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            toolbox_startup.carbon_event_dispatch_stack.push(dispatch);
            Some(action)
        }
        PpcImportDispatcherTarget::RunApplicationEventLoop => {
            // CarbonEvents.h (QuickTime 6.0.2): this is a void routine. The
            // loop owns each pulled queue reference until dispatch finishes.
            if let Some(dispatch) = toolbox_startup.carbon_event_dispatch_stack.last_mut() {
                if dispatch.origin == PpcCarbonEventDispatchOrigin::ApplicationLoop
                    && dispatch.import_pc == cpu.pc
                    && cpu.lr == cpu.pc
                {
                    if cpu.gpr[3] == ppc_i16_result(PPC_EVENT_NOT_HANDLED_ERR)
                        && !dispatch.delegated
                    {
                        if let Some(action) = ppc_call_next_carbon_event_handler(
                            cpu,
                            dispatch,
                            &mut toolbox_startup.next_carbon_event_call_ref,
                        ) {
                            return Some(action);
                        }
                    }
                    let dispatch = toolbox_startup.carbon_event_dispatch_stack.pop().unwrap();
                    ppc_release_carbon_event(toolbox_startup, dispatch.event_ref);
                    cpu.lr = dispatch.return_pc;
                }
            }
            let return_pc = match toolbox_startup.application_event_loop_context {
                Some((import_pc, return_pc)) if import_pc == cpu.pc => return_pc,
                Some(_) => return Some(PpcImportAction::ReturnPreserve),
                None => {
                    toolbox_startup.application_event_loop_context = Some((cpu.pc, cpu.lr));
                    cpu.lr
                }
            };
            if toolbox_startup.application_event_loop_quit_requested {
                toolbox_startup.application_event_loop_quit_requested = false;
                toolbox_startup.application_event_loop_context = None;
                cpu.lr = return_pc;
                return Some(PpcImportAction::ReturnPreserve);
            }
            toolbox_startup.event_loop_poll_until_tick = Some(tick_count.wrapping_add(1));
            while let Some((event_ref, _)) = toolbox_startup.carbon_event_queue.pop_front() {
                let handlers = ppc_carbon_matching_handlers(
                    toolbox_startup,
                    event_ref,
                    PPC_EVENT_DISPATCHER_TARGET_REF,
                )
                .unwrap_or_default();
                if handlers.is_empty() {
                    ppc_release_carbon_event(toolbox_startup, event_ref);
                    continue;
                }
                let mut dispatch = PpcCarbonEventDispatchRecord {
                    origin: PpcCarbonEventDispatchOrigin::ApplicationLoop,
                    import_pc: cpu.pc,
                    return_pc,
                    restore_rtoc: cpu.gpr[2],
                    event_ref,
                    handlers,
                    next_index: 0,
                    active_call_ref: 0,
                    delegated: false,
                };
                let Some(action) = ppc_call_next_carbon_event_handler(
                    cpu,
                    &mut dispatch,
                    &mut toolbox_startup.next_carbon_event_call_ref,
                ) else {
                    ppc_release_carbon_event(toolbox_startup, event_ref);
                    return Some(PpcImportAction::ReturnPreserve);
                };
                toolbox_startup.carbon_event_dispatch_stack.push(dispatch);
                return Some(action);
            }
            Some(PpcImportAction::Yield(u64::MAX))
        }
        PpcImportDispatcherTarget::QuitApplicationEventLoop => {
            toolbox_startup.application_event_loop_quit_requested = true;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InstallStandardEventHandler => {
            // CarbonEventsCore.h, InstallStandardEventHandler (CarbonLib
            // 1.1): before Mac OS X 10.5 only window targets have an
            // installable standard handler. Other targets have no effect.
            let target = cpu.gpr[3];
            if matches!(
                target,
                PPC_APPLICATION_EVENT_TARGET_REF | PPC_EVENT_DISPATCHER_TARGET_REF
            ) {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
            } else {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            }
        }
        PpcImportDispatcherTarget::GetCurrentEventTime => {
            // CarbonEventsCore.h, GetCurrentEventTime: EventTime is seconds
            // since startup. The classic tick clock advances at 60 Hz.
            cpu.fpr[1] = (f64::from(tick_count) / 60.0).to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetMainEventQueue => {
            // Carbon Event Manager Programming Guide (2005), "Posting Events":
            // GetMainEventQueue returns the main application's EventQueueRef.
            Some(PpcImportAction::Return(PPC_MAIN_EVENT_QUEUE_REF))
        }
        PpcImportDispatcherTarget::FlushEventQueue => {
            // Carbon Event Manager: FlushEventQueue(inQueue) removes all
            // pending events from the selected queue. This HLE has one main
            // queue shared with classic Event Manager calls.
            if cpu.gpr[3] != PPC_MAIN_EVENT_QUEUE_REF {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            while let Some((event_ref, _)) = toolbox_startup.carbon_event_queue.pop_front() {
                ppc_release_carbon_event(toolbox_startup, event_ref);
            }
            event_queue.clear();
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::FlushEvents => {
            // FlushEvents removes matching low-level events before the first
            // event selected by stopMask; non-low-level events remain queued.
            // PROCEDURE FlushEvents(whichMask: Integer; stopMask: Integer);
            // Macintosh Toolbox Essentials (1992), pp. 2-93–2-94.
            toolbox_startup.flush_events_count =
                toolbox_startup.flush_events_count.saturating_add(1);
            toolbox_startup.last_flush_event_mask = cpu.gpr[3] as u16;
            toolbox_startup.last_flush_stop_mask = cpu.gpr[4] as u16;
            ppc_flush_events(
                event_queue,
                toolbox_startup.last_flush_event_mask,
                toolbox_startup.last_flush_stop_mask,
            );
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
            // Classic polling functions run the Carbon event loop too. Keep
            // it active through a WaitNextEvent sleep, but not through later
            // application work after a nonblocking poll.
            let os_only = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::GetOSEvent
            );
            if !os_only {
                ppc_service_process_activation(memory, gworlds, window_list,
                    toolbox_startup, event_queue, process_mode, tick_count,
                    matches!(binding.dispatcher_target,
                        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent)),
                    dialog_callback_active);

                let wait_ticks = if matches!(
                    binding.dispatcher_target,
                    PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent)
                ) {
                    sleep_ticks
                } else {
                    0
                };
                toolbox_startup.event_loop_poll_until_tick =
                    Some(tick_count.wrapping_add(wait_ticks.max(1)));
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
            ppc_suppress_window_updates(event_queue, &toolbox_startup.windows_without_updates);
            let (what, message, when, where_v, where_h, modifiers, has_event) =
                ppc_poll_process_event(event_queue, event_mask, input, os_only, tick_count, true);
            if has_event && what == 8 {
                let pending = if (modifiers & 1) != 0 { 0x0A64 } else { 0x0A68 };
                if memory.read_u32_be(pending) == Some(message) {
                    let _ = memory.write_u32_be(pending, 0);
                }
            }
            if crate::trap::dispatch::trace_input_enabled()
                || (has_event && crate::trap::dispatch::trace_delivered_events_enabled())
            {
                eprintln!(
                    "[INPUT] PPC {} lr=${:08X} mask=${event_mask:04X} event_ptr=${event_ptr:08X} sleep={} -> has_event={} what={} message=${message:08X} where=({}, {}) modifiers=${modifiers:04X}",
                    binding.symbol_name, cpu.lr, sleep_ticks, has_event, what, where_v, where_h,
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
                ppc_service_process_activation(memory, gworlds, window_list,
                    toolbox_startup, event_queue, process_mode, tick_count, true,
                    dialog_callback_active);

                toolbox_startup.event_loop_poll_until_tick = Some(tick_count.wrapping_add(1));
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
            ppc_suppress_window_updates(event_queue, &toolbox_startup.windows_without_updates);
            let (what, message, when, where_v, where_h, modifiers, has_event) =
                ppc_poll_process_event(event_queue, event_mask, input, os_only, tick_count, false);
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
            let snapshot =
                ppc_event_probe_result(has_event, what, message, when, where_v, where_h, modifiers);
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
            toolbox_startup.last_still_down_result =
                Some(ppc_still_down_result(input, event_queue));
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
        PpcImportDispatcherTarget::GetMouse => {
            let point_ptr = cpu.gpr[3];
            if point_ptr != 0 && ppc_memory_can_write_bytes(memory, point_ptr, 4) {
                let _ = memory.write_u16_be(point_ptr, input.mouse_v as u16);
                let _ = memory.write_u16_be(point_ptr + 2, input.mouse_h as u16);
                // GetMouse reports the position in the current graphics port's
                // local coordinate system. EventRecord.where remains global.
                // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 2-25.
                let _ = ppc_transform_port_point(memory, current_gworld, point_ptr, false);
            }
            if crate::trap::dispatch::trace_input_enabled() {
                let local_v = memory
                    .read_u16_be(point_ptr)
                    .unwrap_or(input.mouse_v as u16) as i16;
                let local_h = memory
                    .read_u16_be(point_ptr.saturating_add(2))
                    .unwrap_or(input.mouse_h as u16) as i16;
                eprintln!(
                    "[INPUT] PPC GetMouse ptr=${point_ptr:08X} -> ({}, {})",
                    local_v, local_h
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
