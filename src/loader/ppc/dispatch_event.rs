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
