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

pub(crate) fn ppc_flush_events(
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    which_mask: u16,
    stop_mask: u16,
) {
    let mut stopped = false;
    event_queue.retain(|event| {
        if stopped || !ppc_is_low_level_event(event.what) {
            return true;
        }
        if ppc_event_matches_mask(stop_mask, event.what) {
            stopped = true;
            return true;
        }
        !ppc_event_matches_mask(which_mask, event.what)
    });
}

pub(crate) fn ppc_suppress_window_updates(
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    windows_without_updates: &std::collections::HashSet<u32>,
) {
    event_queue.retain(|event| {
        event.what != 6 || !windows_without_updates.contains(&event.message)
    });
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
        let index = matching
            .min_by_key(|(index, event)| (ppc_toolbox_event_priority(event.what), *index))
            .map(|(index, _)| index)?;
        Some(crate::event_queue::autokey_before_its_keyup(
            event_queue,
            index,
            |what| ppc_event_matches_mask(event_mask, what),
        ))
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
    (
        0,
        0,
        tick_count,
        input.mouse_v,
        input.mouse_h,
        ppc_current_event_modifiers(input),
        false,
    )
}

pub(crate) fn ppc_dequeue_event(
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    event_mask: u16,
    input: PpcInputSnapshot,
    os_only: bool,
    tick_count: u32,
) -> (u16, u32, u32, i16, i16, u16, bool) {
    if let Some(index) = ppc_matching_event_index(event_queue, event_mask, os_only) {
        let event =
            crate::event_queue::remove_event_and_retire_earlier_autokeys(event_queue, index)
                .unwrap();
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
    (
        0,
        0,
        tick_count,
        input.mouse_v,
        input.mouse_h,
        ppc_current_event_modifiers(input),
        false,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn event(what: u16, message: u32) -> PpcQueuedEvent {
        PpcQueuedEvent {
            what,
            message,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        }
    }

    #[test]
    fn elapsed_autokey_is_selected_before_the_keyup_that_follows_it() {
        // An autoKey posted before the release precedes that key's keyUp;
        // another key's autoKey keeps its lower priority.
        let queue: VecDeque<_> = [event(5, 0x7C1D), event(4, 0x7C1D)].into_iter().collect();
        assert_eq!(ppc_matching_event_index(&queue, 0xFFFF, false), Some(0));
        let queue: VecDeque<_> = [event(5, 0x7B1C), event(4, 0x7C1D)].into_iter().collect();
        assert_eq!(ppc_matching_event_index(&queue, 0xFFFF, false), Some(1));
        // A mask without autoKey still gets the keyUp.
        assert_eq!(ppc_matching_event_index(&queue, 0x0010, false), Some(1));
        let queue: VecDeque<_> = [event(5, 0x7C1D), event(4, 0x7C1D)].into_iter().collect();
        assert_eq!(ppc_matching_event_index(&queue, 0x0010, false), Some(1));
    }

    #[test]
    fn keyup_only_dequeue_retires_earlier_autokeys_for_that_key_only() {
        let input = PpcInputSnapshot::default();
        let mut queue: VecDeque<_> = [
            event(5, 0x7C1D),
            event(5, 0x7B1C),
            event(4, 0x7C1D),
            event(5, 0x7C1D),
        ]
        .into_iter()
        .collect();

        // Peeking with a keyUp-only mask leaves earlier repeats intact.
        let peeked = ppc_peek_event(&queue, 0x0010, input, false, 10);
        assert_eq!((peeked.0, peeked.1, peeked.6), (4, 0x7C1D, true));
        assert_eq!(queue.len(), 4);

        // Consuming keyUp retires the earlier repeat for 0x7C, while preserving
        // the other key's repeat and any later repeat queued after the release.
        let first = ppc_dequeue_event(&mut queue, 0x0010, input, false, 10);
        assert_eq!((first.0, first.1, first.6), (4, 0x7C1D, true));
        let second = ppc_dequeue_event(&mut queue, 0xFFFF, input, false, 10);
        assert_eq!((second.0, second.1, second.6), (5, 0x7B1C, true));
        let third = ppc_dequeue_event(&mut queue, 0xFFFF, input, false, 10);
        assert_eq!((third.0, third.1, third.6), (5, 0x7C1D, true));
        let fourth = ppc_dequeue_event(&mut queue, 0xFFFF, input, false, 10);
        assert!(!fourth.6);

        // The two-event changing-mask sequence [autoKey, keyUp] with keyUp-only
        // then everyEvent never delivers the stale repeat after the release.
        let mut pair: VecDeque<_> = [event(5, 0x7C1D), event(4, 0x7C1D)].into_iter().collect();
        let release = ppc_dequeue_event(&mut pair, 0x0010, input, false, 10);
        assert_eq!((release.0, release.1, release.6), (4, 0x7C1D, true));
        let follow_up = ppc_dequeue_event(&mut pair, 0xFFFF, input, false, 10);
        assert!(!follow_up.6);
    }
}
