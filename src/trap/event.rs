//! Event Manager trap handlers (OS traps only).

use super::dispatch::trace_input_enabled;
use crate::cpu::{CpuOps, Register};
use crate::event_queue::{EventProbeResult, EventRecordSnapshot};
use crate::memory::globals::addr;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::Result;

impl super::TrapDispatcher {
    const HIGH_LEVEL_EVENT_MASK: u16 = 0x0400;
    pub(crate) const K_HIGH_LEVEL_EVENT: u16 = 23;
    const K_CORE_EVENT_CLASS: u32 = 0x61657674; // 'aevt'
    const K_AE_OPEN_APPLICATION: u32 = 0x6F617070; // 'oapp'
    const AUTO_KEY_EVENT: u16 = 5;
    const OS_EVENT: u16 = 15;
    const MOUSE_MOVED_MESSAGE: u32 = 0xFA00_0000;
    const QHDR_HEAD_OFFSET: u32 = 2;
    const QHDR_TAIL_OFFSET: u32 = 6;
    const QELEM_LINK_OFFSET: u32 = 0;
    const QERR: u32 = (-1i32) as u32;
    const EVT_NOT_ENB: u32 = 1;
    const EVQEL_QTYPE: u16 = 4; // ORD(evType)
    const EVQEL_WHAT_OFFSET: u32 = 6;
    const EVQEL_MESSAGE_OFFSET: u32 = 8;
    const EVQEL_WHEN_OFFSET: u32 = 12;
    const EVQEL_WHERE_V_OFFSET: u32 = 16;
    const EVQEL_WHERE_H_OFFSET: u32 = 18;
    const EVQEL_MODIFIERS_OFFSET: u32 = 20;
    const AUX_DCE_SIZE: u32 = 52;
    const DCE_DRIVER_OFFSET: u32 = 0;
    const DCE_FLAGS_OFFSET: u32 = 4;
    const DCE_REF_NUM_OFFSET: u32 = 24;
    const D_OPENED_MASK: u16 = 0x0020;
    const D_RAM_BASED_MASK: u16 = 0x0040;
    const BAD_UNIT_ERR: u32 = (-21i32) as u32;
    const D_REMOVE_ERR: u32 = (-25i32) as u32;
    const MEM_FULL_ERR: u32 = (-108i32) as u32;

    /// Resolve a driver reference number to its unit-table entry address.
    /// Inside Macintosh: Devices (1994), pp. 1-8--1-9 defines the unit
    /// number as the one's complement of the reference number and exposes
    /// the table through UTableBase and UnitNtryCnt.
    fn driver_unit_table_slot(bus: &MacMemoryBus, ref_num: u16) -> Option<u32> {
        let unit = !ref_num;
        let count = bus.read_word(addr::UNIT_NTRY_CNT);
        let table = bus.read_long(addr::U_TABLE_BASE);
        (unit < count && table != 0).then(|| table + u32::from(unit) * 4)
    }

    fn install_driver_dce(&mut self, bus: &mut MacMemoryBus, ref_num: u16) -> u32 {
        let Some(slot) = Self::driver_unit_table_slot(bus, ref_num) else {
            return Self::BAD_UNIT_ERR;
        };

        let mut handle = bus.read_long(slot);
        let mut dce = if handle == 0 {
            0
        } else {
            bus.read_long(handle)
        };
        if dce == 0 {
            dce = bus.alloc(Self::AUX_DCE_SIZE);
            if dce == 0 {
                return Self::MEM_FULL_ERR;
            }
            handle = bus.alloc(4);
            if handle == 0 {
                bus.free(dce);
                return Self::MEM_FULL_ERR;
            }
            bus.write_long(handle, dce);
            self.track_handle_ptr(dce, handle);
        }

        // Devices 1994, pp. 1-83--1-85: both install forms clear the full
        // AuxDCE, set only dRAMBased and dCtlRefNum, and place its handle in
        // the selected unit-table entry. In particular dCtlDriver remains
        // clear; the caller may populate it after installation.
        bus.fill_zeros(dce, Self::AUX_DCE_SIZE);
        bus.write_word(dce + Self::DCE_FLAGS_OFFSET, Self::D_RAM_BASED_MASK);
        bus.write_word(dce + Self::DCE_REF_NUM_OFFSET, ref_num);
        bus.write_long(slot, handle);
        0
    }

    fn remove_driver_dce<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        ref_num: u16,
    ) -> u32 {
        let Some(slot) = Self::driver_unit_table_slot(bus, ref_num) else {
            return Self::BAD_UNIT_ERR;
        };
        let handle = bus.read_long(slot);
        if handle == 0 {
            return 0;
        }
        let dce = bus.read_long(handle);
        if dce == 0 {
            bus.write_long(slot, 0);
            bus.free(handle);
            return 0;
        }
        let flags = bus.read_word(dce + Self::DCE_FLAGS_OFFSET);
        if flags & Self::D_OPENED_MASK != 0 {
            return Self::D_REMOVE_ERR;
        }

        // Devices 1994, pp. 1-85--1-86: release a RAM-based driver resource
        // before disposing its DCE handle. Reuse the Resource Manager path so
        // residency and resource-map bookkeeping stay coherent.
        let driver = bus.read_long(dce + Self::DCE_DRIVER_OFFSET);
        if flags & Self::D_RAM_BASED_MASK != 0 && driver != 0 {
            let saved_sp = cpu.read_reg(Register::A7);
            let call_sp = saved_sp.wrapping_sub(4);
            bus.write_long(call_sp, driver);
            cpu.write_reg(Register::A7, call_sp);
            let _ = self.dispatch_resource(true, 0x1A3, cpu, bus);
            cpu.write_reg(Register::A7, saved_sp);
        }

        // DisposeHandle preserves stale RecoverHandle discovery until a
        // master-pointer slot is reused; mirror that policy by retaining the
        // ptr_to_handle entry while returning both blocks to the allocator.
        bus.free(dce);
        bus.free(handle);
        bus.write_long(slot, 0);
        0
    }

    pub(crate) fn event_matches_mask(event_mask: u16, what: u16) -> bool {
        match what {
            Self::K_HIGH_LEVEL_EVENT => (event_mask & Self::HIGH_LEVEL_EVENT_MASK) != 0,
            0..=15 => {
                let bit = 1u16 << what;
                (event_mask & bit) != 0
            }
            _ => false,
        }
    }

    fn toolbox_event_priority(what: u16) -> u8 {
        // Macintosh Toolbox Essentials (1992), pp. 2-18--2-19: the Event
        // Manager selects by event-class priority, preserving FIFO order
        // among mouse, key, and disk events.
        match what {
            8 => 0,
            1..=4 | 7 => 1,
            5 => 2,
            6 => 3,
            15 => 4,
            Self::K_HIGH_LEVEL_EVENT => 5,
            _ => 6,
        }
    }

    fn matching_toolbox_event_index(&self, event_mask: u16) -> Option<usize> {
        self.event_queue
            .iter()
            .enumerate()
            .filter(|(_, event)| {
                event.what != 6 && Self::event_matches_mask(event_mask, event.what)
            })
            .min_by_key(|(index, event)| (Self::toolbox_event_priority(event.what), *index))
            .map(|(index, _)| index)
    }

    fn preferred_marked_update_event(
        &mut self,
        bus: &MacMemoryBus,
        event_mask: u16,
    ) -> Option<(super::dispatch::QueuedEvent, bool)> {
        if !Self::event_matches_mask(event_mask, 6) {
            return None;
        }

        let known_windows = &self.window_list;
        self.flushed_update_events
            .retain(|event| event.what == 6 && known_windows.contains(&event.message));

        for window in self.window_list.windows() {
            if !self.window_visible(bus, window) || !self.window_has_pending_update(bus, window) {
                continue;
            }
            // CheckUpdate consumes picture-backed window updates internally,
            // so EventAvail must not advertise a marker that GetNextEvent
            // will redraw instead of returning. Macintosh Toolbox Essentials
            // (1992), pp. 4-115--4-116.
            const WINDOW_PIC_OFFSET: u32 = 148;
            let picture_handle = bus.read_long(window + WINDOW_PIC_OFFSET);
            if picture_handle != 0 && bus.read_long(picture_handle) != 0 {
                continue;
            }
            if let Some(event) = self
                .event_queue
                .iter()
                .find(|event| event.what == 6 && event.message == window)
            {
                return Some((event.clone(), false));
            }
            if let Some(event) = self
                .flushed_update_events
                .iter()
                .find(|event| event.what == 6 && event.message == window)
            {
                return Some((event.clone(), true));
            }
        }

        // Preserve explicitly posted update events that do not name a window
        // known to this Window Manager. Known-window markers were considered
        // above and must not bypass visibility, dirtiness, picture handling,
        // or front-to-back order through this fallback.
        self.event_queue
            .iter()
            .find(|event| event.what == 6 && !self.window_list.contains(&event.message))
            .map(|event| (event, false))
    }

    fn dequeue_preferred_marked_update_event(
        &mut self,
        bus: &MacMemoryBus,
        event_mask: u16,
    ) -> Option<super::dispatch::QueuedEvent> {
        let (event, recovered) = self.preferred_marked_update_event(bus, event_mask)?;
        let index = if recovered {
            self.flushed_update_events
                .iter()
                .position(|candidate| candidate.what == 6 && candidate.message == event.message)?
        } else {
            self.event_queue
                .iter()
                .position(|candidate| candidate.what == 6 && candidate.message == event.message)?
        };
        if recovered {
            self.flushed_update_events.remove(index)
        } else {
            self.event_queue.remove(index)
        }
    }

    pub(crate) fn mouse_moved_event_for_region(
        &self,
        bus: &MacMemoryBus,
        event_mask: u16,
        mouse_rgn: u32,
    ) -> Option<super::dispatch::QueuedEvent> {
        // WaitNextEvent's mouseRgn is the region inside which the Event
        // Manager does not generate mouse-moved operating-system events.
        // NIL or empty regions suppress them. Macintosh Toolbox Essentials
        // 1992, pp. 2-22..2-23 and 2-62..2-63; Region record layout:
        // Inside Macintosh Volume I, I-141.
        if !Self::event_matches_mask(event_mask, Self::OS_EVENT) || mouse_rgn == 0 {
            return None;
        }

        if Self::region_bbox(bus, mouse_rgn).is_none()
            || Self::region_contains_point(
                bus,
                mouse_rgn,
                self.input_state.mouse_position().0,
                self.input_state.mouse_position().1,
            )
        {
            return None;
        }

        Some(super::dispatch::QueuedEvent {
            what: Self::OS_EVENT,
            message: Self::MOUSE_MOVED_MESSAGE,
            when: self.current_tick(),
            where_v: self.input_state.mouse_position().0,
            where_h: self.input_state.mouse_position().1,
            modifiers: self.current_event_modifiers(),
        })
    }

    pub(crate) fn posted_event_is_enabled(system_event_mask: u16, what: u16) -> bool {
        // The system event mask (SysEvtMask) gates OS-level events.
        // Per IM:II-67 table 2-2, bits 0..15 correspond to specific
        // event types (mDown, keyDown, activate, etc.); bit 10 is
        // highLevelEventMask for what=23 (kHighLevelEvent / app1Evt).
        // Application-defined events beyond what=23 (app2/3/4Evt =
        // 24/25/26 per IM:II-66) and Sound Manager / file-system
        // notification events are NOT gated by SysEvtMask — always postable.
        match what {
            Self::K_HIGH_LEVEL_EVENT => (system_event_mask & Self::HIGH_LEVEL_EVENT_MASK) != 0,
            0..=15 => {
                let bit = 1u16 << what;
                (system_event_mask & bit) != 0
            }
            _ => true,
        }
    }

    fn make_posted_event(&self, what: u16, message: u32) -> super::dispatch::QueuedEvent {
        super::dispatch::QueuedEvent {
            what,
            message,
            when: self.current_tick(),
            where_v: self.input_state.mouse_position().0,
            where_h: self.input_state.mouse_position().1,
            modifiers: self.current_event_modifiers(),
        }
    }

    fn alloc_evqel_snapshot(
        &self,
        bus: &mut MacMemoryBus,
        event: &super::dispatch::QueuedEvent,
    ) -> u32 {
        let ptr = bus.alloc(22);
        if ptr == 0 {
            return 0;
        }

        bus.write_long(ptr + Self::QELEM_LINK_OFFSET, 0);
        bus.write_word(ptr + 4, Self::EVQEL_QTYPE);
        bus.write_word(ptr + Self::EVQEL_WHAT_OFFSET, event.what);
        bus.write_long(ptr + Self::EVQEL_MESSAGE_OFFSET, event.message);
        bus.write_long(ptr + Self::EVQEL_WHEN_OFFSET, event.when);
        bus.write_word(ptr + Self::EVQEL_WHERE_V_OFFSET, event.where_v as u16);
        bus.write_word(ptr + Self::EVQEL_WHERE_H_OFFSET, event.where_h as u16);
        bus.write_word(ptr + Self::EVQEL_MODIFIERS_OFFSET, event.modifiers);
        ptr
    }

    fn post_os_event(&mut self, bus: &mut MacMemoryBus, what: u16, message: u32) -> (u32, u32) {
        if !Self::posted_event_is_enabled(
            bus.read_word(crate::memory::globals::addr::SYS_EVT_MASK),
            what,
        ) {
            return (Self::EVT_NOT_ENB, 0);
        }

        let event = self.make_posted_event(what, message);
        let qel_ptr = if (self.current_trap_word & 0x0100) != 0 {
            self.alloc_evqel_snapshot(bus, &event)
        } else {
            0
        };
        self.event_queue.push_back(event);
        (0, qel_ptr)
    }

    fn flush_events_with_masks(&mut self, event_mask: u16, stop_mask: u16) -> u32 {
        let mut result = 0;
        let mut stopped = false;
        let mut remaining = std::collections::VecDeque::with_capacity(self.event_queue.len());

        while let Some(event) = self.event_queue.pop_front() {
            if !stopped && stop_mask != 0 && Self::event_matches_mask(stop_mask, event.what) {
                result = event.what as u32;
                stopped = true;
                remaining.push_back(event);
                continue;
            }

            if !stopped && Self::event_matches_mask(event_mask, event.what) {
                if event.what == 6 {
                    self.remember_flushed_update_event(&event);
                }
                continue;
            }

            remaining.push_back(event);
        }

        self.event_queue.replace_events(remaining);
        result
    }

    fn remember_flushed_update_event(&mut self, event: &super::dispatch::QueuedEvent) {
        if event.what != 6 || event.message == 0 {
            return;
        }
        if self
            .flushed_update_events
            .iter()
            .any(|queued| queued.what == 6 && queued.message == event.message)
        {
            return;
        }
        if std::env::var_os("SYSTEMLESS_TRACE_INVAL").is_some() {
            eprintln!(
                "[INVAL] remember_flushed_update_event window=${:08X} tick={}",
                event.message,
                self.current_tick()
            );
        }
        self.flushed_update_events.push_back(event.clone());
    }

    fn enqueue_open_application_event_if_needed(&mut self, event_mask: u16) {
        // The Finder sends required launch Apple events only to applications
        // whose 'SIZE' resource declares isHighLevelEventAware. Applications
        // without that resource or flag default to false.
        // Macintosh Toolbox Essentials 1992, pp. 2-30 to 2-32 and 5-90.
        if (event_mask & Self::HIGH_LEVEL_EVENT_MASK) == 0
            || !self.apple_event_launch_state.claim_open_application_event()
        {
            return;
        }

        let tick = self.current_tick();
        self.event_queue.push_front(super::dispatch::QueuedEvent {
            what: Self::K_HIGH_LEVEL_EVENT,
            message: Self::K_CORE_EVENT_CLASS,
            when: tick,
            where_v: (Self::K_AE_OPEN_APPLICATION >> 16) as i16,
            where_h: (Self::K_AE_OPEN_APPLICATION & 0xFFFF) as i16,
            modifiers: 0,
        });
    }

    fn tick_has_reached(now: u32, due: u32) -> bool {
        now.wrapping_sub(due) < 0x8000_0000
    }

    pub(crate) fn post_auto_key_if_due(&mut self, system_event_mask: u16) {
        // Auto-key is a low-level event posted by the Operating System Event
        // Manager once the threshold/rate elapses; it is not synthesized only
        // when an application happens to poll. Macintosh Toolbox Essentials,
        // pp. 2-29 and 2-38.
        if !Self::posted_event_is_enabled(system_event_mask, Self::AUTO_KEY_EVENT) {
            return;
        }
        let Some(repeat) = self.input_state.key_repeat() else {
            return;
        };
        if !Self::key_generates_auto_key(repeat.key_code()) || !self.key_is_down(repeat.key_code())
        {
            self.input_state.clear_key_repeat();
            return;
        }
        if !Self::tick_has_reached(self.current_tick(), repeat.next_tick()) {
            return;
        }

        let tick = self.current_tick();
        self.input_state
            .advance_key_repeat(tick.wrapping_add(Self::AUTO_KEY_RATE_TICKS));

        let message = repeat.message();
        let modifiers = self.current_event_modifiers();
        self.event_queue.push_back(super::dispatch::QueuedEvent {
            what: Self::AUTO_KEY_EVENT,
            message,
            when: tick,
            where_v: self.input_state.mouse_position().0,
            where_h: self.input_state.mouse_position().1,
            modifiers,
        });
    }

    fn enqueue_auto_key_if_due(&mut self, system_event_mask: u16, event_mask: u16) {
        if Self::event_matches_mask(event_mask, Self::AUTO_KEY_EVENT) {
            self.post_auto_key_if_due(system_event_mask);
        }
    }

    fn peek_pending_native_menu_event(
        &self,
        event_mask: u16,
    ) -> Option<super::dispatch::QueuedEvent> {
        if self.pending_native_menu_event_tick == Some(self.current_tick()) {
            return None;
        }
        self.pending_native_menu_event
            .as_ref()
            .filter(|event| Self::event_matches_mask(event_mask, event.what))
            .cloned()
    }

    fn pending_native_menu_wins_fifo_tie(&self) -> bool {
        // Before the retained click has been presented, matching low-level
        // events already in the OS queue predate it. Once presented, the
        // retained click keeps its place while it is rate-limited between
        // presentations. Macintosh Toolbox Essentials (1992), pp. 2-18--2-19.
        self.pending_native_menu_event_tick.is_some()
    }

    fn dequeue_pending_native_menu_event(
        &mut self,
        event_mask: u16,
    ) -> Option<super::dispatch::QueuedEvent> {
        let event = self.peek_pending_native_menu_event(event_mask)?;
        self.pending_native_menu_event_tick = Some(self.current_tick());
        if trace_input_enabled() || super::dispatch::trace_delivered_events_enabled() {
            eprintln!(
                "[INPUT] present latched native-menu mouseDown where=({}, {}) mask=${:04X} tick={}",
                event.where_v,
                event.where_h,
                event_mask,
                self.current_tick()
            );
        }
        Some(event)
    }

    pub(crate) fn has_pending_native_menu_event(&self) -> bool {
        self.pending_native_menu_event.is_some()
    }

    pub(crate) fn peek_toolbox_event(
        &mut self,
        bus: &MacMemoryBus,
        event_mask: u16,
    ) -> Option<super::dispatch::QueuedEvent> {
        self.enqueue_open_application_event_if_needed(event_mask);
        self.enqueue_auto_key_if_due(
            bus.read_word(crate::memory::globals::addr::SYS_EVT_MASK),
            event_mask,
        );
        let pending_menu = self.peek_pending_native_menu_event(event_mask);
        let queued = self
            .matching_toolbox_event_index(event_mask)
            .and_then(|index| self.event_queue.get(index));
        let update = self
            .preferred_marked_update_event(bus, event_mask)
            .map(|(event, _)| event);
        let queued_or_menu = match (pending_menu, queued) {
            (Some(pending), Some(queued)) => Some(
                if Self::toolbox_event_priority(pending.what)
                    < Self::toolbox_event_priority(queued.what)
                    || (Self::toolbox_event_priority(pending.what)
                        == Self::toolbox_event_priority(queued.what)
                        && self.pending_native_menu_wins_fifo_tie())
                {
                    pending
                } else {
                    queued
                },
            ),
            (pending, queued) => pending.or(queued),
        };
        match (queued_or_menu, update) {
            (Some(queued), Some(update)) => Some(
                if Self::toolbox_event_priority(queued.what)
                    <= Self::toolbox_event_priority(update.what)
                {
                    queued
                } else {
                    update
                },
            ),
            (queued, update) => queued.or(update),
        }
    }

    fn peek_event(
        &mut self,
        bus: &MacMemoryBus,
        event_mask: u16,
    ) -> Option<super::dispatch::QueuedEvent> {
        self.enqueue_auto_key_if_due(
            bus.read_word(crate::memory::globals::addr::SYS_EVT_MASK),
            event_mask,
        );
        let pending = self.peek_pending_native_menu_event(event_mask);
        let queued = self.event_queue.iter().find(|event| {
            Self::is_low_level_os_event(event.what)
                && Self::event_matches_mask(event_mask, event.what)
        });
        match (pending, queued) {
            (Some(_), Some(queued)) if !self.pending_native_menu_wins_fifo_tie() => Some(queued),
            (pending, queued) => pending.or(queued),
        }
    }

    pub(crate) fn dequeue_toolbox_event<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        event_mask: u16,
    ) -> (u16, u32, u32, i16, i16, u16, bool) {
        self.enqueue_open_application_event_if_needed(event_mask);
        self.enqueue_auto_key_if_due(
            bus.read_word(crate::memory::globals::addr::SYS_EVT_MASK),
            event_mask,
        );
        if Self::event_matches_mask(event_mask, 6) {
            self.service_window_picture_updates(cpu, bus);
        }
        let pending_menu = self.peek_pending_native_menu_event(event_mask);
        let first_idx = self.matching_toolbox_event_index(event_mask);
        let preferred_update = self
            .preferred_marked_update_event(bus, event_mask)
            .map(|(event, _)| event);
        let pending_has_priority = pending_menu.as_ref().is_some_and(|pending| {
            let precedes_queued = first_idx
                .and_then(|index| self.event_queue.get(index))
                .is_none_or(|queued| {
                    Self::toolbox_event_priority(pending.what)
                        < Self::toolbox_event_priority(queued.what)
                        || (Self::toolbox_event_priority(pending.what)
                            == Self::toolbox_event_priority(queued.what)
                            && self.pending_native_menu_wins_fifo_tie())
                });
            let precedes_update = preferred_update.as_ref().is_none_or(|update| {
                Self::toolbox_event_priority(pending.what)
                    <= Self::toolbox_event_priority(update.what)
            });
            precedes_queued && precedes_update
        });
        if pending_has_priority {
            let event = self.dequeue_pending_native_menu_event(event_mask).unwrap();
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
        let update_has_priority = preferred_update.as_ref().is_some_and(|update| {
            first_idx
                .and_then(|index| self.event_queue.get(index))
                .is_none_or(|queued| {
                    Self::toolbox_event_priority(update.what)
                        < Self::toolbox_event_priority(queued.what)
                })
        });
        if update_has_priority {
            let event = self
                .dequeue_preferred_marked_update_event(bus, event_mask)
                .expect("selected update must remain available");
            if trace_input_enabled() || super::dispatch::trace_delivered_events_enabled() {
                eprintln!(
                    "[INPUT] dequeue update what={} message=${:08X} mask=${:04X}",
                    event.what, event.message, event_mask
                );
            }
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
        if let Some(first_idx) = first_idx {
            let idx = first_idx;
            let event = self.event_queue.get(idx).expect("event queue index");
            if self.consume_retained_modal_dialog_event(cpu, bus, &event) {
                self.event_queue.remove(idx);
                self.acknowledge_window_activation_event(bus, &event);
                if trace_input_enabled() || super::dispatch::trace_delivered_events_enabled() {
                    eprintln!(
                        "[INPUT] consumed retained-modal what={} where=({}, {}) mask=${:04X}",
                        event.what, event.where_v, event.where_h, event_mask
                    );
                }
                return (
                    0,
                    0,
                    self.current_tick(),
                    self.input_state.mouse_position().0,
                    self.input_state.mouse_position().1,
                    self.current_event_modifiers(),
                    false,
                );
            }
            let event = self.event_queue.remove(idx).unwrap();
            self.acknowledge_window_activation_event(bus, &event);
            if event.what == Self::K_HIGH_LEVEL_EVENT
                && event.message == Self::K_CORE_EVENT_CLASS
                && ((event.where_v as u16 as u32) << 16 | event.where_h as u16 as u32)
                    == Self::K_AE_OPEN_APPLICATION
            {
                self.apple_event_launch_state
                    .note_open_application_event_delivered();
            }
            if trace_input_enabled() || super::dispatch::trace_delivered_events_enabled() {
                eprintln!(
                    "[INPUT] dequeue what={} message=${:08X} where=({}, {}) mask=${:04X}",
                    event.what, event.message, event.where_v, event.where_h, event_mask
                );
            }
            if event.what == 2 {
                self.input_state.set_mouse_button_pressed(false);
            }
            self.begin_app_owned_modal_dialog_button_tracking(bus, &event);
            if matches!(event.what, 3 | 4 | 5) {
                self.debug_key_event_delivery_count =
                    self.debug_key_event_delivery_count.saturating_add(1);
                self.debug_last_key_event_message = event.message;
            }
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

        if let Some(event) = self.dequeue_preferred_marked_update_event(bus, event_mask) {
            if trace_input_enabled() || super::dispatch::trace_delivered_events_enabled() {
                eprintln!(
                    "[INPUT] dequeue update what={} message=${:08X} mask=${:04X}",
                    event.what, event.message, event_mask
                );
            }
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

        // Update events flow exclusively through `event_queue` (pushed by
        // `queue_window_update_event` on InvalRect/InvalRgn/ShowWindow,
        // cleared by BeginUpdate). FlushEvents can remove such a queued
        // update while the Window Manager update region remains dirty, so
        // those dropped entries are recoverable once through
        // `flushed_update_events`. We intentionally do not synthesize from
        // arbitrary dirty update regions here: an unacknowledged dirty region
        // would otherwise stream updateEvts indefinitely.
        (
            0,
            0,
            self.current_tick(),
            self.input_state.mouse_position().0,
            self.input_state.mouse_position().1,
            self.current_event_modifiers(),
            false,
        )
    }

    /// Write an EventRecord to guest memory and update low-memory mouse globals.
    pub(crate) fn write_event_record(
        &mut self,
        bus: &mut MacMemoryBus,
        event_ptr: u32,
        what: u16,
        message: u32,
        when: u32,
        where_v: i16,
        where_h: i16,
        modifiers: u16,
    ) {
        self.debug_last_event_record = Some(EventRecordSnapshot {
            what,
            message,
            when,
            where_v,
            where_h,
            modifiers,
        });
        if what == 8 {
            self.debug_activation_event_seen = true;
        }
        if what == 6 {
            self.debug_update_event_seen = true;
        }
        // Pack the 16-byte EventRecord into one big-endian buffer and issue
        // a single bus.write_bytes call (faster than 6 word/long writes for
        // hot paths like WaitNextEvent).
        let rec: [u8; 16] = [
            (what >> 8) as u8,
            what as u8,
            (message >> 24) as u8,
            (message >> 16) as u8,
            (message >> 8) as u8,
            message as u8,
            (when >> 24) as u8,
            (when >> 16) as u8,
            (when >> 8) as u8,
            when as u8,
            ((where_v as u16) >> 8) as u8,
            (where_v as u16) as u8,
            ((where_h as u16) >> 8) as u8,
            (where_h as u16) as u8,
            (modifiers >> 8) as u8,
            modifiers as u8,
        ];
        bus.write_bytes(event_ptr, &rec);

        // Update low-memory mouse globals
        // Reference: Executor docs/globals.cpp — MTemp=$0828, MouseLocation=$082C, MouseLocation2=$0830
        let mb_state: u8 = if self.input_state.mouse_button_pressed() {
            0x00
        } else {
            0x80
        };
        bus.write_byte(0x0172, mb_state);
        // MTemp, MouseLocation, MouseLocation2 are 12 contiguous bytes at $0828
        // (3 × Point = 3 × (i16 v, i16 h)). Single packed write.
        let (mouse_v, mouse_h) = self.input_state.mouse_position();
        let v = mouse_v as u16;
        let h = mouse_h as u16;
        let mouse_globals: [u8; 12] = [
            (v >> 8) as u8,
            v as u8,
            (h >> 8) as u8,
            h as u8,
            (v >> 8) as u8,
            v as u8,
            (h >> 8) as u8,
            h as u8,
            (v >> 8) as u8,
            v as u8,
            (h >> 8) as u8,
            h as u8,
        ];
        bus.write_bytes(0x0828, &mouse_globals);
    }

    /// Dequeue one event matching the event mask, or return a null event.
    /// Returns (what, message, where_v, where_h, modifiers, has_event).
    pub(crate) fn dequeue_event(
        &mut self,
        bus: &MacMemoryBus,
        event_mask: u16,
    ) -> (u16, u32, u32, i16, i16, u16, bool) {
        self.enqueue_auto_key_if_due(
            bus.read_word(crate::memory::globals::addr::SYS_EVT_MASK),
            event_mask,
        );
        let queued_idx = self.event_queue.iter().position(|event| {
            Self::is_low_level_os_event(event.what)
                && Self::event_matches_mask(event_mask, event.what)
        });
        if queued_idx.is_none() || self.pending_native_menu_wins_fifo_tie() {
            if let Some(event) = self.dequeue_pending_native_menu_event(event_mask) {
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
        }
        if let Some(idx) = queued_idx {
            let ev = self.event_queue.remove(idx).unwrap();
            if trace_input_enabled() || super::dispatch::trace_delivered_events_enabled() {
                eprintln!(
                    "[INPUT] dequeue what={} message=${:08X} where=({}, {}) mask=${:04X}",
                    ev.what, ev.message, ev.where_v, ev.where_h, event_mask
                );
            }
            // mouseUp dequeues leave the hardware button released.
            // push_mouse_up() already updates the physical state immediately,
            // but keeping this assignment is harmless and mirrors event delivery.
            if ev.what == 2 {
                self.input_state.set_mouse_button_pressed(false);
            }

            (
                ev.what,
                ev.message,
                ev.when,
                ev.where_v,
                ev.where_h,
                ev.modifiers,
                true,
            )
        } else {
            (
                0,
                0,
                self.current_tick(),
                self.input_state.mouse_position().0,
                self.input_state.mouse_position().1,
                self.current_event_modifiers(),
                false,
            )
        }
    }

    fn is_low_level_os_event(what: u16) -> bool {
        matches!(what, 1..=5 | 7)
    }

    fn enqueue_qelem(&self, bus: &mut MacMemoryBus, q_entry: u32, q_header: u32) {
        let head = bus.read_long(q_header + Self::QHDR_HEAD_OFFSET);
        let tail = bus.read_long(q_header + Self::QHDR_TAIL_OFFSET);

        bus.write_long(q_entry + Self::QELEM_LINK_OFFSET, 0);

        if head == 0 {
            bus.write_long(q_header + Self::QHDR_HEAD_OFFSET, q_entry);
            bus.write_long(q_header + Self::QHDR_TAIL_OFFSET, q_entry);
            return;
        }

        let current_tail = if tail != 0 {
            tail
        } else {
            let mut cursor = head;
            loop {
                let next = bus.read_long(cursor + Self::QELEM_LINK_OFFSET);
                if next == 0 {
                    break cursor;
                }
                cursor = next;
            }
        };

        bus.write_long(current_tail + Self::QELEM_LINK_OFFSET, q_entry);
        bus.write_long(q_header + Self::QHDR_TAIL_OFFSET, q_entry);
    }

    fn dequeue_qelem(&self, bus: &mut MacMemoryBus, q_entry: u32, q_header: u32) -> u32 {
        let mut prev = 0;
        let mut current = bus.read_long(q_header + Self::QHDR_HEAD_OFFSET);

        while current != 0 {
            let next = bus.read_long(current + Self::QELEM_LINK_OFFSET);
            if current == q_entry {
                if prev == 0 {
                    bus.write_long(q_header + Self::QHDR_HEAD_OFFSET, next);
                } else {
                    bus.write_long(prev + Self::QELEM_LINK_OFFSET, next);
                }

                if bus.read_long(q_header + Self::QHDR_TAIL_OFFSET) == current {
                    bus.write_long(q_header + Self::QHDR_TAIL_OFFSET, prev);
                }
                return 0;
            }

            prev = current;
            current = next;
        }

        Self::QERR
    }

    pub(crate) fn dispatch_event<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        self.read_tick_count(bus);
        Some(match (is_tool, trap_num) {
            // ========== OS Event Traps ==========

            // FlushEvents ($A032)
            // Removes events matching eventMask, stopping before the first stopMask match.
            // PROCEDURE FlushEvents(eventMask, stopMask: INTEGER);
            // D0.low = eventMask, D0.high = stopMask; Inside Macintosh Volume II, II-69
            (false, 0x32) => {
                let masks = cpu.read_reg(Register::D0);
                let event_mask = masks as u16;
                let stop_mask = (masks >> 16) as u16;
                let queue_len_before = self.event_queue.len();
                let result = self.flush_events_with_masks(event_mask, stop_mask);
                if trace_input_enabled() {
                    eprintln!(
                        "[INPUT] FlushEvents event_mask=${:04X} stop_mask=${:04X} -> result={} queue_len {}->{}",
                        event_mask,
                        stop_mask,
                        result,
                        queue_len_before,
                        self.event_queue.len()
                    );
                }
                cpu.write_reg(Register::D0, result);
                Ok(())
            }

            // AttachVBL ($A071)
            // Changes the slot number of the primary video monitor.
            // FUNCTION AttachVBL (theSlot: Integer): OSErr; register-only: D0=theSlot -> D0=OSErr, A7 preserved
            // Inside Macintosh: Processes (1994), p. 4-26 (slotNumErr -360)
            (false, 0x71) => {
                let slot = cpu.read_reg(Register::D0) as u16 as i16;
                if !(0..=15).contains(&slot) {
                    cpu.write_reg(Register::D0, (-360i32) as u32);
                    return Some(Ok(()));
                }
                self.callback_scheduling.set_primary_vbl_slot(slot);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PostEvent ($A02F) / PPostEvent ($A12F)
            // Posts an OS event to the event queue with current time, mouse, and modifier state.
            // FUNCTION PostEvent(eventCode: INTEGER; eventMsg: LONGINT): OSErr;
            // A0: eventCode (word); D0: eventMsg (long); Inside Macintosh Volume II, II-69
            (false, 0x2F) => {
                let event_code = cpu.read_reg(Register::A0) as u16;
                let event_msg = cpu.read_reg(Register::D0);
                let (result, qel_ptr) = self.post_os_event(bus, event_code, event_msg);
                self.debug_event_queue_probe.post_result = Some(result as i16);
                cpu.write_reg(Register::D0, result);
                if (self.current_trap_word & 0x0100) != 0 {
                    cpu.write_reg(Register::A0, qel_ptr);
                }
                Ok(())
            }

            // InitEvents ($A06D)
            // Internal Event Manager initialization. No-op stub.
            // Inside Macintosh Volume II
            // InitEvents ($A06D): Internal Event Manager init; returns Ok with no side effects
            (false, 0x6D) => {
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // Enqueue ($A96F)
            // Inside Macintosh: Operating System Utilities (1994),
            // pp. 6-15 to 6-16 (originally IM:II 1985, p. II-374).
            //
            // PROCEDURE Enqueue(qElement: QElemPtr; qHeader: QHdrPtr);
            //   Registers on entry: A0 = qElement, A1 = qHeader.
            //   Registers on exit:  A1 = qHeader (preserved).
            //   No function-result code.
            //
            // Tool-bit PROCEDURE (trap-word bit 11 set; the dispatcher
            // normalises the 12-bit selector to 0x16F). Register-only
            // ABI: no Pascal stack argument frame and no Pascal
            // function-result slot since Enqueue is declared `void`.
            // The MPW Universal Headers OSUtils.h exposes the C-level
            // `Enqueue(QElemPtr qElement, QHdrPtr qHeader) -> void` with
            // `#pragma parameter Enqueue(__A0, __A1)` +
            // `ONEWORDINLINE(0xA96F)`.
            //
            // QHdr layout (IM:OSUtils 1994 p. 6-13):
            //   +0  qFlags  (Integer, 2 bytes)  queue-specific flags
            //   +2  qHead   (QElemPtr, 4 bytes) pointer to first element
            //   +6  qTail   (QElemPtr, 4 bytes) pointer to last element
            //
            // QElem layout (IM:OSUtils 1994 p. 6-14):
            //   +0  qLink   (QElemPtr, 4 bytes) pointer to next element
            //   +4  qType   (QTypes / Integer, 2 bytes) queue type tag
            //   ... per-type union body
            //
            // Apple-canonical behavior: write NIL into the new element's
            // qLink to terminate it, then either (a) if the queue is
            // empty, set qHead = qTail = element; (b) if the queue is
            // non-empty, append by writing element into the prior tail's
            // qLink and updating qTail. qFlags is never mutated. The
            // procedure is callable at interrupt time and disables
            // interrupts briefly while the queue header is updated.
            //
            // Behavior:
            //   (1) Empty-queue insert sets qHead == qTail == element
            //       AND element.qLink == NIL.
            //   (2) Non-empty-queue append: prior-tail.qLink points at
            //       new element, qTail becomes new element, new
            //       element.qLink == NIL, qHead unchanged, qFlags
            //       unchanged.
            //
            // Contract-test coverage in this file (mod tests):
            //   enqueue_on_empty_queue_sets_qhead_qtail_and_terminal_link
            //   enqueue_appends_after_existing_tail_and_preserves_qflags
            //   enqueue_dequeue_dispatcher_convention_preserves_register_only_abi
            (true, 0x16F) => {
                let q_entry = cpu.read_reg(Register::A0);
                let q_header = cpu.read_reg(Register::A1);
                self.enqueue_qelem(bus, q_entry, q_header);
                Ok(())
            }

            // Dequeue ($A96E)
            // Inside Macintosh: Operating System Utilities (1994),
            // pp. 6-16 to 6-17 (originally IM:II 1985, p. II-374).
            //
            // FUNCTION Dequeue(qElement: QElemPtr; qHeader: QHdrPtr): OSErr;
            //   Registers on entry: A0 = qElement, A1 = qHeader.
            //   Registers on exit:  A1 = qHeader (preserved); D0 =
            //                       result code (noErr 0 | qErr -1).
            //
            // Tool-bit FUNCTION (trap-word bit 11 set; the dispatcher
            // normalises the 12-bit selector to 0x16E). Register-only
            // ABI: no Pascal stack argument frame and no Pascal
            // function-result slot — D0 carries the OSErr result. The
            // MPW Universal Headers OSUtils.h exposes the C-level
            // `Dequeue(QElemPtr qElement, QHdrPtr qHeader) -> OSErr`
            // with `#pragma parameter __D0 Dequeue(__A0, __A1)` +
            // `ONEWORDINLINE(0xA96E)`.
            //
            // Apple-canonical behavior: walk the singly-linked qHead
            // chain looking for qElement. If found: unlink it (repair
            // the predecessor's qLink, repair qTail if removing the
            // current tail, repair qHead if removing the current head)
            // and return D0 = noErr (0). If not found: return D0 =
            // qErr (-1) with the queue structure unchanged. The
            // function is callable at interrupt time and disables
            // interrupts during the walk.
            //
            // Behavior:
            //   (1) Present-entry removal: D0 == 0 AND qHead/qTail
            //       repaired.
            //   (2) Missing-entry: D0 == qErr (-1) AND qHead/qTail
            //       unchanged.
            //
            // Contract-test coverage in this file (mod tests):
            //   dequeue_present_entry_unlinks_element_and_returns_noerr
            //   dequeue_missing_entry_returns_qerr_and_preserves_queue
            //   enqueue_dequeue_dispatcher_convention_preserves_register_only_abi
            (true, 0x16E) => {
                let q_entry = cpu.read_reg(Register::A0);
                let q_header = cpu.read_reg(Register::A1);
                cpu.write_reg(Register::D0, self.dequeue_qelem(bus, q_entry, q_header));
                Ok(())
            }

            // ========== Device Manager / interrupt registration ==========
            //
            // Register-based OS traps that install / remove device drivers
            // and interrupt handlers. DriverInstall and DriverRemove model
            // their guest-visible DCE/unit-table state. Interrupt handlers
            // remain registrations without synthetic hardware sources:
            //   - VBL queue scheduling (no VBLTask record chain;
            //     vertical-blank events are synthesized by the
            //     wall-clock-paced event loop, not by guest VBL
            //     tasks)
            //   - Slot Manager interrupt vectors (no NuBus slot
            //     emulation; Systemless's framebuffer is direct, not
            //     via a slot device)
            //
            // The "noErr return" is the IM-canonical "I have
            // installed/removed your handler successfully" answer.
            // Apps that defensively check OSErr after these calls
            // proceed to use whatever driver/handler they thought
            // got registered — but since Systemless never INVOKES the
            // registered handler (no real interrupt source), the
            // handler is dead-coded, harmlessly. Apps that depend
            // on the handler firing (e.g. a VBL task that polls
            // hardware) will have their gameplay tied to the
            // wall-clock-paced 60 Hz event loop instead of the
            // emulated VBL — same effective rate, different
            // dispatch path.
            //
            // Inside Macintosh Volume II, II-244 (DrvrInstall /
            // DrvrRemove); Volume III, III-21 (RDrvrInstall —
            // ROM driver install variant); Volume V, V-575 +
            // V-577 (DoVBLTask / SIntInstall / SIntRemove).

            // DrvrInstall ($A03D)
            // Inside Macintosh: Devices (1994), pp. 1-83 to 1-84
            // (originally IM Volume II 1985, p. II-244).
            //
            // FUNCTION DriverInstall(drvrPtr: Ptr; refNum: INTEGER): OSErr;
            //   Registers on entry: A0 = drvrPtr, D0 = refNum (driver
            //                       reference number).
            //   Registers on exit:  D0 = result code (noErr 0 |
            //                       badUnitErr -21).
            //
            // OS-bit FUNCTION (trap-word bit 11 clear) with register-only
            // ABI: no Pascal stack argument frame, no FUNCTION result
            // slot — both inputs and the OSErr result travel in
            // registers. The MPW Universal Headers Devices.h exposes
            // the C-level `DriverInstall(DRVRHeaderPtr drvrPtr,
            // short refNum)` with `#pragma parameter __D0
            // DriverInstall(__A0, __D0)` + `ONEWORDINLINE(0xA03D)`.
            //
            // Apple-canonical behavior: allocate a DCE in the system
            // heap, install a handle to it in the unit table at the
            // refNum'd slot, copy refNum into dCtlRefNum, set the
            // dRAMBased flag, and clear all other fields. Per the
            // IM:Devices 1994 p. 1-83 "does not load the driver
            // resource into memory, copy the flags from the driver
            // header, or open the driver" disclaimer, the install does
            // NOT execute the driver's open routine.
            //
            // Behavior:
            //   (1) noErr after installing or reinitializing the selected
            //       52-byte AuxDCE and its unit-table handle.
            //   (2) badUnitErr when the one's-complement refNum lies outside
            //       UnitNtryCnt.
            //   (3) bit 10 ($A43D) selects DriverInstallReserveMem. Its
            //       preallocation ReserveMem has no separate placement effect
            //       in Systemless's flat heap, but retains the same DCE state.
            //   (4) register-only ABI: A7 preserved across the call.
            (false, 0x3D) => {
                let ref_num = cpu.read_reg(Register::D0) as u16;
                let result = self.install_driver_dce(bus, ref_num);
                cpu.write_reg(Register::D0, result);
                Ok(())
            }

            // DrvrRemove ($A03E)
            // Inside Macintosh: Devices (1994), pp. 1-85 to 1-86
            // (originally IM Volume II 1985, p. II-244).
            //
            // FUNCTION DriverRemove(refNum: INTEGER): OSErr;
            //   Registers on entry: D0 = refNum (driver reference number).
            //   Registers on exit:  D0 = result code (noErr 0 |
            //                       dRemoveErr -25 if driver is open).
            //
            // OS-bit FUNCTION with single-register-input ABI: only D0
            // is consumed and rewritten. The MPW Universal Headers
            // Devices.h exposes the C-level `DrvrRemove(short refNum)`
            // with `#pragma parameter __D0 DrvrRemove(__D0)` +
            // `ONEWORDINLINE(0xA03E)`. The header comment notes that
            // DrvrRemove has been renamed to DriverRemove on
            // InterfaceLib 7.1+, but the trap word is unchanged and
            // the calling convention is preserved.
            //
            // Apple-canonical behavior: locate the unit-table entry
            // for the refNum'd slot, call DisposeHandle on the DCE,
            // NIL the unit-table slot, and (if the driver was loaded
            // via Resource Manager with dRAMBased set) call
            // ReleaseResource on the driver resource. The driver must
            // be closed (per IM:Devices 1994 p. 1-85).
            //
            // Behavior:
            //   (1) noErr after disposing a closed DCE and clearing its slot;
            //       an already-empty valid slot is also a successful no-op.
            //   (2) dRemoveErr with no mutation while dOpened is set.
            //   (3) badUnitErr for a refNum outside UnitNtryCnt.
            //   (4) register-only ABI: A7 preserved across the call.
            (false, 0x3E) => {
                let ref_num = cpu.read_reg(Register::D0) as u16;
                let result = self.remove_driver_dce(cpu, bus, ref_num);
                cpu.write_reg(Register::D0, result);
                Ok(())
            }

            // RDrvrInstall ($A04F)
            // Inside Macintosh Volume III, III-21
            // ROM driver install variant — same shape as DrvrInstall
            // but for ROM-resident DRVR resources. HLE no-op.
            // RDrvrInstall ($A04F): Returns noErr in D0; per IM:III III-21 ROM-driver install variant of DrvrInstall; HLE no-op (no Mac ROM, no ROM DRVR chain)
            (false, 0x4F) => {
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // AttachVBL ($A071)
            // Attach VBL task to a slot. Already handled at (false, 0x71) above.
            // Inside Macintosh Volume V, V-575

            // DoVBLTask ($A072)
            // Decrements vblCount and executes tasks in a slot-based vertical retrace queue.
            // FUNCTION DoVBLTask (theSlot: Integer): OSErr; register-only: D0=theSlot -> D0=OSErr, A7 preserved
            // Inside Macintosh: Processes (1994), p. 4-27 (slotNumErr -360)
            (false, 0x72) => {
                let slot = cpu.read_reg(Register::D0) as u16 as i16;
                if !(0..=15).contains(&slot) {
                    cpu.write_reg(Register::D0, (-360i32) as u32);
                    return Some(Ok(()));
                }
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // SIntInstall ($A075)
            // Inside Macintosh Volume V, V-577
            // PROCEDURE SIntInstall(intRec: SInt32; slot: INTEGER): OSErr; (register-based)
            // SIntInstall ($A075): Returns noErr in D0; per IM:V V-577 installs a slot-interrupt handler in NuBus slot N; HLE has no NuBus slot emulation (framebuffer is direct, not via a slot card) so the handler is never invoked
            (false, 0x75) => {
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // SIntRemove ($A076)
            // Inside Macintosh Volume V, V-577
            // PROCEDURE SIntRemove(intRec: SInt32; slot: INTEGER): OSErr; (register-based)
            // SIntRemove ($A076): Returns noErr in D0; per IM:V V-577 removes a slot-interrupt handler; HLE no-op (no slot-interrupt vector to clear)
            (false, 0x76) => {
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // OSEventAvail ($A030)
            // Peeks at the OS event queue without dequeueing.
            // FUNCTION OSEventAvail(mask: INTEGER; VAR theEvent: EventRecord): BOOLEAN;
            // D0=0 if event present (TRUE), D0=$FFFF if null (FALSE); Macintosh Toolbox Essentials 1992, p. 2-98
            (false, 0x30) => {
                let event_mask = cpu.read_reg(Register::D0) as u16;
                let event_ptr = cpu.read_reg(Register::A0);
                if let Some(event) = self.peek_event(bus, event_mask) {
                    self.write_event_record(
                        bus,
                        event_ptr,
                        event.what,
                        event.message,
                        event.when,
                        event.where_v,
                        event.where_h,
                        event.modifiers,
                    );
                    self.debug_event_queue_probe.os_event_avail = Some(EventProbeResult {
                        available: true,
                        record: EventRecordSnapshot {
                            what: event.what,
                            message: event.message,
                            when: event.when,
                            where_v: event.where_v,
                            where_h: event.where_h,
                            modifiers: event.modifiers,
                        },
                    });
                    // D0=0 means event found (TRUE); D0=$FFFF means null (FALSE).
                    // TB Essentials 1992, p. 2-98; confirmed by MPW disassembly (ADDQ+BEQ pattern).
                    cpu.write_reg(Register::D0, 0);
                } else {
                    self.write_event_record(
                        bus,
                        event_ptr,
                        0,
                        0,
                        self.current_tick(),
                        self.input_state.mouse_position().0,
                        self.input_state.mouse_position().1,
                        self.current_event_modifiers(),
                    );
                    self.debug_event_queue_probe.os_event_avail = Some(EventProbeResult {
                        available: false,
                        record: EventRecordSnapshot {
                            what: 0,
                            message: 0,
                            when: self.current_tick(),
                            where_v: self.input_state.mouse_position().0,
                            where_h: self.input_state.mouse_position().1,
                            modifiers: self.current_event_modifiers(),
                        },
                    });
                    cpu.write_reg(Register::D0, 0xFFFF);
                }
                Ok(())
            }

            // GetOSEvent ($A031)
            // Dequeues the next matching event from the OS event queue.
            // FUNCTION GetOSEvent(mask: INTEGER; VAR theEvent: EventRecord): BOOLEAN;
            // D0=0 if event found (TRUE), D0=$FFFF if null event (FALSE); Macintosh Toolbox Essentials 1992, p. 2-97
            (false, 0x31) => {
                let event_mask = cpu.read_reg(Register::D0) as u16;
                let event_ptr = cpu.read_reg(Register::A0);
                let (what, message, when, where_v, where_h, modifiers, has_event) =
                    self.dequeue_event(bus, event_mask);
                self.write_event_record(
                    bus, event_ptr, what, message, when, where_v, where_h, modifiers,
                );
                self.debug_event_queue_probe.get_os_event = Some(EventProbeResult {
                    available: has_event,
                    record: EventRecordSnapshot {
                        what,
                        message,
                        when,
                        where_v,
                        where_h,
                        modifiers,
                    },
                });
                // D0=0 means event found (TRUE); D0=$FFFF means null (FALSE).
                // TB Essentials 1992, p. 2-97; confirmed by MPW disassembly (ADDQ+BEQ pattern).
                cpu.write_reg(Register::D0, if has_event { 0 } else { 0xFFFF });
                Ok(())
            }

            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests;
