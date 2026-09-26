//! PowerPC Event Manager queue inspection, priority matching, snapshot recording,
//! and window event queuing routines.

use super::*;
use crate::event_queue::{EventProbeResult, EventRecordSnapshot};
use std::collections::VecDeque;

pub(crate) fn ppc_write_event_record(
    memory: &mut PpcSectionMem,
    event_ptr: u32,
    what: u16,
    message: u32,
    when: u32,
    where_v: i16,
    where_h: i16,
    modifiers: u16,
) -> bool {
    if event_ptr == 0 || !ppc_memory_can_write_bytes(memory, event_ptr, 16) {
        return false;
    }
    memory.write_u16_be(event_ptr, what).is_some()
        && memory.write_u32_be(event_ptr + 2, message).is_some()
        && memory.write_u32_be(event_ptr + 6, when).is_some()
        && memory
            .write_u16_be(event_ptr + 10, where_v as u16)
            .is_some()
        && memory
            .write_u16_be(event_ptr + 12, where_h as u16)
            .is_some()
        && memory.write_u16_be(event_ptr + 14, modifiers).is_some()
}

pub(crate) fn ppc_record_event_snapshot(
    startup: &mut PpcToolboxStartupState,
    what: u16,
    message: u32,
    when: u32,
    where_v: i16,
    where_h: i16,
    modifiers: u16,
) {
    startup.last_event_record = Some(EventRecordSnapshot {
        what,
        message,
        when,
        where_v,
        where_h,
        modifiers,
    });
    if what == 8 {
        startup.activation_event_seen = true;
    }
    if what == 6 {
        startup.update_event_seen = true;
    }
}

pub(crate) fn ppc_event_probe_result(
    available: bool,
    what: u16,
    message: u32,
    when: u32,
    where_v: i16,
    where_h: i16,
    modifiers: u16,
) -> EventProbeResult {
    EventProbeResult {
        available,
        record: EventRecordSnapshot {
            what,
            message,
            when,
            where_v,
            where_h,
            modifiers,
        },
    }
}

pub(crate) fn ppc_event_matches_mask(event_mask: u16, what: u16) -> bool {
    match what {
        23 => (event_mask & 0x0400) != 0,
        0..=15 => (event_mask & (1u16 << what)) != 0,
        _ => false,
    }
}

pub(crate) fn ppc_current_event_modifiers(input: PpcInputSnapshot) -> u16 {
    // EventRecord modifiers use the same classic bit assignments on both
    // adapters. Read the shared logical key map rather than the host key
    // callback so PostEvent observes the state visible to Button/GetKeys.
    const BTN_STATE: u16 = 0x0080;
    const CMD_KEY: u16 = 0x0100;
    const SHIFT_KEY: u16 = 0x0200;
    const ALPHA_LOCK: u16 = 0x0400;
    const OPTION_KEY: u16 = 0x0800;
    const CONTROL_KEY: u16 = 0x1000;

    let mut modifiers = 0;
    if !input.mouse_button {
        modifiers |= BTN_STATE;
    }
    if crate::trap::dispatch::key_map_key_is_down(&input.key_map, 0x37) {
        modifiers |= CMD_KEY;
    }
    if crate::trap::dispatch::key_map_key_is_down(&input.key_map, 0x38)
        || crate::trap::dispatch::key_map_key_is_down(&input.key_map, 0x3C)
    {
        modifiers |= SHIFT_KEY;
    }
    if crate::trap::dispatch::key_map_key_is_down(&input.key_map, 0x39) {
        modifiers |= ALPHA_LOCK;
    }
    if crate::trap::dispatch::key_map_key_is_down(&input.key_map, 0x3A)
        || crate::trap::dispatch::key_map_key_is_down(&input.key_map, 0x3D)
    {
        modifiers |= OPTION_KEY;
    }
    if crate::trap::dispatch::key_map_key_is_down(&input.key_map, 0x3B)
        || crate::trap::dispatch::key_map_key_is_down(&input.key_map, 0x3E)
    {
        modifiers |= CONTROL_KEY;
    }
    modifiers
}

pub(crate) fn ppc_is_low_level_event(what: u16) -> bool {
    matches!(what, 1..=5 | 7)
}

pub(crate) fn ppc_toolbox_event_priority(what: u16) -> u8 {
    // Macintosh Toolbox Essentials (1992), pp. 2-18--2-19: the Event
    // Manager selects by event-class priority, preserving FIFO order among
    // mouse, key, and disk events rather than using one combined FIFO.
    match what {
        8 => 0,
        1..=4 | 7 => 1,
        5 => 2,
        6 => 3,
        15 => 4,
        23 => 5,
        _ => 6,
    }
}

pub(crate) fn ppc_matching_event_index(
    event_queue: &VecDeque<PpcQueuedEvent>,
    event_mask: u16,
    os_only: bool,
) -> Option<usize> {
    let matching = event_queue.iter().enumerate().filter(|(_, event)| {
        (!os_only || ppc_is_low_level_event(event.what))
            && ppc_event_matches_mask(event_mask, event.what)
    });
    if os_only {
        matching.map(|(index, _)| index).next()
    } else {
        matching
            .min_by_key(|(index, event)| (ppc_toolbox_event_priority(event.what), *index))
            .map(|(index, _)| index)
    }
}

pub(crate) fn ppc_peek_event(
    event_queue: &VecDeque<PpcQueuedEvent>,
    event_mask: u16,
    input: PpcInputSnapshot,
    os_only: bool,
    tick_count: u32,
) -> (u16, u32, u32, i16, i16, u16, bool) {
    if let Some(event) = ppc_matching_event_index(event_queue, event_mask, os_only)
        .and_then(|index| event_queue.get(index))
    {
        return (
            event.what,
            event.message,
            event.when,
            event.where_v,
            event.where_h,
            event.modifiers,
            true,
        );
    }
    (0, 0, tick_count, input.mouse_v, input.mouse_h, 0, false)
}

pub(crate) fn ppc_dequeue_event(
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    event_mask: u16,
    input: PpcInputSnapshot,
    os_only: bool,
    tick_count: u32,
) -> (u16, u32, u32, i16, i16, u16, bool) {
    if let Some(index) = ppc_matching_event_index(event_queue, event_mask, os_only) {
        let event = event_queue.remove(index).unwrap();
        return (
            event.what,
            event.message,
            event.when,
            event.where_v,
            event.where_h,
            event.modifiers,
            true,
        );
    }
    (0, 0, tick_count, input.mouse_v, input.mouse_h, 0, false)
}

pub(crate) fn ppc_enqueue_window_update_event(
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    window: u32,
    when: u32,
    input: PpcInputSnapshot,
) {
    if window == 0
        || event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == window)
    {
        return;
    }
    // Macintosh Toolbox Essentials (1992), pp. 4-65--4-67: showing a
    // previously invisible window makes its content region invalid and the
    // Window Manager posts an updateEvt for the application to redraw it.
    event_queue.push_back(PpcQueuedEvent {
        what: 6,
        message: window,
        when,
        where_v: input.mouse_v,
        where_h: input.mouse_h,
        modifiers: 0,
    });
}

pub(crate) fn ppc_enqueue_window_activation_event(
    memory: &mut PpcSectionMem,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    window: u32,
    activating: bool,
    when: u32,
) {
    if window == 0 {
        return;
    }
    let active_flag = u16::from(activating);
    event_queue.retain(|event| event.what != 8 || (event.modifiers & 1) != active_flag);
    let _ = memory.write_u32_be(if activating { 0x0A64 } else { 0x0A68 }, window);
    event_queue.push_back(PpcQueuedEvent {
        what: 8,
        message: window,
        when,
        where_v: 0,
        where_h: 0,
        modifiers: active_flag,
    });
}

pub(crate) fn ppc_enqueue_window_activation_transition(
    memory: &mut PpcSectionMem,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    previous_front: Option<u32>,
    next_front: Option<u32>,
    when: u32,
) {
    if previous_front == next_front {
        return;
    }
    if let Some(previous) = previous_front {
        ppc_enqueue_window_activation_event(memory, event_queue, previous, false, when);
    }
    if let Some(next) = next_front {
        ppc_enqueue_window_activation_event(memory, event_queue, next, true, when);
    }
}
