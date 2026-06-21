//! Event Manager trap handlers (OS traps only).

use super::dispatch::trace_input_enabled;
use crate::cpu::{CpuOps, Register};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::Result;

impl super::TrapDispatcher {
    const HIGH_LEVEL_EVENT_MASK: u16 = 0x0400;
    const K_HIGH_LEVEL_EVENT: u16 = 23;
    const K_CORE_EVENT_CLASS: u32 = 0x61657674; // 'aevt'
    const K_AE_OPEN_APPLICATION: u32 = 0x6F617070; // 'oapp'
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
            || Self::region_contains_point(bus, mouse_rgn, self.mouse_pos.0, self.mouse_pos.1)
        {
            return None;
        }

        Some(super::dispatch::QueuedEvent {
            what: Self::OS_EVENT,
            message: Self::MOUSE_MOVED_MESSAGE,
            where_v: self.mouse_pos.0,
            where_h: self.mouse_pos.1,
            modifiers: self.current_event_modifiers(),
        })
    }

    fn posted_event_is_enabled(&self, what: u16) -> bool {
        // The system event mask (SysEvtMask) gates OS-level events.
        // Per IM:II-67 table 2-2, bits 0..15 correspond to specific
        // event types (mDown, keyDown, activate, etc.); bit 10 is
        // highLevelEventMask for what=23 (kHighLevelEvent / app1Evt).
        // Application-defined events beyond what=23 (app2/3/4Evt =
        // 24/25/26 per IM:II-66) and Sound Manager / file-system
        // notification events are NOT gated by SysEvtMask — always postable.
        match what {
            Self::K_HIGH_LEVEL_EVENT => (self.system_event_mask & Self::HIGH_LEVEL_EVENT_MASK) != 0,
            0..=15 => {
                let bit = 1u16 << what;
                (self.system_event_mask & bit) != 0
            }
            _ => true,
        }
    }

    fn make_posted_event(&self, what: u16, message: u32) -> super::dispatch::QueuedEvent {
        super::dispatch::QueuedEvent {
            what,
            message,
            where_v: self.mouse_pos.0,
            where_h: self.mouse_pos.1,
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
        bus.write_long(ptr + Self::EVQEL_WHEN_OFFSET, self.tick_count);
        bus.write_word(ptr + Self::EVQEL_WHERE_V_OFFSET, event.where_v as u16);
        bus.write_word(ptr + Self::EVQEL_WHERE_H_OFFSET, event.where_h as u16);
        bus.write_word(ptr + Self::EVQEL_MODIFIERS_OFFSET, event.modifiers);
        ptr
    }

    fn post_os_event(&mut self, bus: &mut MacMemoryBus, what: u16, message: u32) -> (u32, u32) {
        if !self.posted_event_is_enabled(what) {
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
                continue;
            }

            remaining.push_back(event);
        }

        self.event_queue = remaining;
        result
    }

    fn enqueue_open_application_event_if_needed(&mut self, event_mask: u16) {
        if self.sent_open_app_event || (event_mask & Self::HIGH_LEVEL_EVENT_MASK) == 0 {
            return;
        }

        self.sent_open_app_event = true;
        self.event_queue.push_front(super::dispatch::QueuedEvent {
            what: Self::K_HIGH_LEVEL_EVENT,
            message: Self::K_CORE_EVENT_CLASS,
            where_v: (Self::K_AE_OPEN_APPLICATION >> 16) as i16,
            where_h: (Self::K_AE_OPEN_APPLICATION & 0xFFFF) as i16,
            modifiers: 0,
        });
    }

    pub(crate) fn peek_toolbox_event(
        &mut self,
        bus: &MacMemoryBus,
        event_mask: u16,
    ) -> Option<super::dispatch::QueuedEvent> {
        self.enqueue_open_application_event_if_needed(event_mask);
        self.event_queue
            .iter()
            .find(|event| Self::event_matches_mask(event_mask, event.what))
            .cloned()
            .or_else(|| self.pending_update_event(bus, event_mask))
    }

    fn peek_event(&self, event_mask: u16) -> Option<super::dispatch::QueuedEvent> {
        self.event_queue
            .iter()
            .find(|event| Self::event_matches_mask(event_mask, event.what))
            .cloned()
    }

    pub(crate) fn dequeue_toolbox_event<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        event_mask: u16,
    ) -> (u16, u32, i16, i16, u16, bool) {
        self.enqueue_open_application_event_if_needed(event_mask);
        if let Some(idx) = self
            .event_queue
            .iter()
            .position(|event| Self::event_matches_mask(event_mask, event.what))
        {
            let event = self.event_queue[idx].clone();
            if self.consume_retained_modal_dialog_event(cpu, bus, &event) {
                self.event_queue.remove(idx);
                if trace_input_enabled() || super::dispatch::trace_delivered_events_enabled() {
                    eprintln!(
                        "[INPUT] consumed retained-modal what={} where=({}, {}) mask=${:04X}",
                        event.what, event.where_v, event.where_h, event_mask
                    );
                }
                return (
                    0,
                    0,
                    self.mouse_pos.0,
                    self.mouse_pos.1,
                    self.current_event_modifiers(),
                    false,
                );
            }
            let event = self.event_queue.remove(idx).unwrap();
            if trace_input_enabled() || super::dispatch::trace_delivered_events_enabled() {
                eprintln!(
                    "[INPUT] dequeue what={} message=${:08X} where=({}, {}) mask=${:04X}",
                    event.what, event.message, event.where_v, event.where_h, event_mask
                );
            }
            if event.what == 2 {
                self.mouse_button = false;
            }
            self.begin_app_owned_modal_dialog_button_tracking(bus, &event);
            if matches!(event.what, 3 | 4) {
                self.debug_key_event_delivery_count =
                    self.debug_key_event_delivery_count.saturating_add(1);
                self.debug_last_key_event_message = event.message;
            }
            return (
                event.what,
                event.message,
                event.where_v,
                event.where_h,
                event.modifiers,
                true,
            );
        }

        // Update events flow exclusively through `event_queue` (pushed by
        // `queue_window_update_event` on InvalRect/InvalRgn/ShowWindow,
        // cleared by BeginUpdate). Previously we also synthesized update
        // events from any window whose update region was non-empty — that
        // re-fired the same event forever for any window the app didn't
        // BeginUpdate-acknowledge. POD MARS Master shows MARS (visible=0
        // in WIND 256) and then ShowWindows it before pushing the
        // Registration dialog on top; POD's modal-dialog event loop only
        // ever calls BeginUpdate on the dialog, so the MARS update region
        // stayed non-empty and the synthetic path streamed ~10K updateEvts
        // for MARS into the dialog loop, locking POD at <1 MIPS while the
        // scripted harness timed out. With the synthetic path removed, the
        // queued MARS updateEvt is delivered once, POD's loop ignores it,
        // and the loop runs at full speed.
        (
            0,
            0,
            self.mouse_pos.0,
            self.mouse_pos.1,
            self.current_event_modifiers(),
            false,
        )
    }

    /// Write an EventRecord to guest memory and update low-memory mouse globals.
    pub(crate) fn write_event_record(
        &self,
        bus: &mut MacMemoryBus,
        event_ptr: u32,
        what: u16,
        message: u32,
        where_v: i16,
        where_h: i16,
        modifiers: u16,
    ) {
        // Pack the 16-byte EventRecord into one big-endian buffer and issue
        // a single bus.write_bytes call (faster than 6 word/long writes for
        // hot paths like WaitNextEvent).
        let when = self.tick_count;
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
        let mb_state: u8 = if self.mouse_button { 0x00 } else { 0x80 };
        bus.write_byte(0x0172, mb_state);
        // MTemp, MouseLocation, MouseLocation2 are 12 contiguous bytes at $0828
        // (3 × Point = 3 × (i16 v, i16 h)). Single packed write.
        let v = self.mouse_pos.0 as u16;
        let h = self.mouse_pos.1 as u16;
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
    pub(crate) fn dequeue_event(&mut self, event_mask: u16) -> (u16, u32, i16, i16, u16, bool) {
        if let Some(idx) = self
            .event_queue
            .iter()
            .position(|e| Self::event_matches_mask(event_mask, e.what))
        {
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
                self.mouse_button = false;
            }

            (
                ev.what,
                ev.message,
                ev.where_v,
                ev.where_h,
                ev.modifiers,
                true,
            )
        } else {
            (
                0,
                0,
                self.mouse_pos.0,
                self.mouse_pos.1,
                self.current_event_modifiers(),
                false,
            )
        }
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
                let result = self.flush_events_with_masks(event_mask, stop_mask);
                cpu.write_reg(Register::D0, result);
                Ok(())
            }

            // OS trap 0x70 — officially SlotVRemove ($A070), but some apps/glue
            // may emit this as a register-based GetNextEvent variant.
            // The real Toolbox GetNextEvent is at (true, 0x170) in toolbox.rs.
            // We handle it as register-based event dispatch for compatibility.
            // Register-based: D0=event mask, A0=event record ptr; D0=result on exit.
            // SlotVRemove ($A070): Officially SlotVRemove; also handles register-based event dispatch for compatibility
            (false, 0x70) => {
                let event_mask = cpu.read_reg(Register::D0) as u16;
                let event_ptr = cpu.read_reg(Register::A0);
                // tick_count is maintained by the runner via advance_guest_tick()

                let (what, message, where_v, where_h, modifiers, has_event) =
                    self.dequeue_toolbox_event(cpu, bus, event_mask);
                self.write_event_record(bus, event_ptr, what, message, where_v, where_h, modifiers);
                // Mac convention: OS trap boolean is $0000 (FALSE) or $FFFF (TRUE).
                cpu.write_reg(Register::D0, if has_event { 0xFFFF } else { 0 });
                Ok(())
            }

            // AttachVBL ($A071)
            // Inside Macintosh: Processes 1994, p. 4-26.
            // FUNCTION AttachVBL (theSlot: Integer): OSErr;
            // The trap uses a register-only ABI: D0 carries the slot
            // number on entry and the OSErr result on exit.
            (false, 0x71) => {
                let slot = cpu.read_reg(Register::D0) as u16 as i16;
                if !(0..=15).contains(&slot) {
                    cpu.write_reg(Register::D0, (-337i32) as u32);
                    return Some(Ok(()));
                }
                self.primary_vbl_slot = slot;
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
            // Engines-agree on:
            //   (1) Empty-queue insert sets qHead == qTail == element
            //       AND element.qLink == NIL (witnessed by strict bake
            //       a96e_a96f_dequeue_enqueue_strict
            //       band B1).
            //   (2) Non-empty-queue append: prior-tail.qLink points at
            //       new element, qTail becomes new element, new
            //       element.qLink == NIL, qHead unchanged, qFlags
            //       unchanged (witnessed by band B2).
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
            // Engines-agree on:
            //   (1) Present-entry removal: D0 == 0 AND qHead/qTail
            //       repaired (witnessed by strict bake
            //       a96e_a96f_dequeue_enqueue_strict
            //       band B3).
            //   (2) Missing-entry: D0 == qErr (-1) AND qHead/qTail
            //       unchanged (witnessed by band B4).
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

            // ========== Device Manager / Interrupt no-op family ==========
            //
            // Six register-based OS traps that install / remove
            // device drivers and slot-interrupt handlers. All
            // collapse to "write noErr to D0" no-ops in Systemless's
            // HLE because Systemless does NOT model:
            //   - Classic Device Manager DRVR resource chaining
            //     (no DCE / DCtl table, no driver dispatch table)
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
            // Systemless HLE compromise: no DCE chain, no unit table, no
            // dispatch table. Returns D0=0 (noErr) unconditionally;
            // the "installed" driver is never invoked because no
            // synthetic interrupt source ever dispatches into a DRVR
            // resource. Apps that defensively check OSErr proceed
            // safely; apps that depend on the driver firing (rare for
            // RAM-installed user drivers in modern System 7.5+ era
            // apps) lose nothing observable since the driver routines
            // were never going to run anyway.
            //
            // Engines-agree on:
            //   (1) noErr return for the nominal-install path on an
            //       empty unit-table slot (witnessed by strict bake
            //       a03d_a03e_drvr_install_remove_strict
            //       band B1).
            //   (2) register-only ABI: A7 preserved across the call
            //       (witnessed by band B2 via StackSpace sandwich).
            (false, 0x3D) => {
                cpu.write_reg(Register::D0, 0);
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
            // Systemless HLE compromise: no DCE chain to dispose. Returns
            // D0=0 (noErr) unconditionally.
            //
            // Engines-agree on:
            //   (1) noErr return when called on a just-installed
            //       (closed) slot (witnessed by band B3).
            //   (2) register-only ABI: A7 preserved across the call
            //       (witnessed by band B4).
            (false, 0x3E) => {
                cpu.write_reg(Register::D0, 0);
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
            // FUNCTION DoVBLTask (theSlot: Integer): OSErr;
            // Inside Macintosh: Processes (1994), p. 4-27.
            //
            // Per the IM:Processes 1994 p. 4-27 register table, DoVBLTask
            // is a register-only OS-bit FUNCTION:
            //   Registers on entry: D0 = slot number
            //   Registers on exit:  D0 = result code (OSErr)
            // No Pascal stack frame is consumed — A7 is unchanged across
            // the call.
            //
            // Result codes (IM:Processes 1994, p. 4-27):
            //   noErr       0     No error
            //   slotNumErr -337   Invalid slot number
            //
            // MPW Universal Headers Retrace.h declares:
            //   #pragma parameter __D0 DoVBLTask(__D0)
            //   EXTERN_API( OSErr )
            //   DoVBLTask(short theSlot)        ONEWORDINLINE(0xA072);
            //
            // Behavior per IM:Processes 1994 p. 4-27: DoVBLTask decrements
            // the vblCount field of each task in the vertical retrace
            // queue corresponding to the theSlot parameter (except for
            // tasks whose vblCount field already contains 0), and
            // executes a task if decrementing results in a value of 0.
            // If theSlot designates the slot of the primary video device,
            // the cursor position is also updated. Invalid slots return
            // slotNumErr (-337).
            //
            // Systemless HLE compromise: the host runtime has no slot VBL
            // queue (vertical-blank events are synthesised from the
            // wall-clock-paced event loop) so valid slots remain a
            // no-op/noErr path, but the documented invalid-slot error
            // path is preserved.
            //
            // Engines-agree subset: the documented register-only OS-bit
            // FUNCTION calling convention itself (A7 unchanged, no Pascal
            // stack frame consumed) plus the invalid-slot error path.
            // Witnessed by a072_dovbltask_strict with
            // a single-call StackSpace ($A065) sandwich, a 5-call
            // composition cycling slot inputs 0/1/0/2/0, and an
            // out-of-range slot=16 slotNumErr witness.
            (false, 0x72) => {
                let slot = cpu.read_reg(Register::D0) as u16 as i16;
                if !(0..=15).contains(&slot) {
                    cpu.write_reg(Register::D0, (-337i32) as u32);
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
                if let Some(event) = self.peek_event(event_mask) {
                    self.write_event_record(
                        bus,
                        event_ptr,
                        event.what,
                        event.message,
                        event.where_v,
                        event.where_h,
                        event.modifiers,
                    );
                    // D0=0 means event found (TRUE); D0=$FFFF means null (FALSE).
                    // TB Essentials 1992, p. 2-98; confirmed by MPW disassembly (ADDQ+BEQ pattern).
                    cpu.write_reg(Register::D0, 0);
                } else {
                    self.write_event_record(
                        bus,
                        event_ptr,
                        0,
                        0,
                        self.mouse_pos.0,
                        self.mouse_pos.1,
                        self.current_event_modifiers(),
                    );
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
                let (what, message, where_v, where_h, modifiers, has_event) =
                    self.dequeue_event(event_mask);
                self.write_event_record(bus, event_ptr, what, message, where_v, where_h, modifiers);
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
mod tests {
    use super::super::dispatch::QueuedEvent;
    use super::super::test_helpers::setup;
    use crate::cpu::{CpuOps, Register};
    use crate::memory::MemoryBus;

    // ---- FlushEvents ($A032) ----

    #[test]
    fn flush_events_clears_queue_and_sets_d0_zero() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Push a couple of events into the queue
        disp.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });
        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 42,
            where_v: 30,
            where_h: 40,
            modifiers: 0,
        });
        assert_eq!(disp.event_queue.len(), 2);

        // D0 low-order word = eventMask, high-order word = stopMask (IM:II II-69)
        cpu.write_reg(Register::D0, 0xFFFFu32);

        let result = disp.dispatch_event(false, 0x32, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert!(disp.event_queue.is_empty());
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn initevents_returns_noerr_and_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let stack_ptr = 0x00F0_3000;
        cpu.write_reg(Register::D0, 0xFFFF_FFEC);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_event(false, 0x6D, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitEvents should be handled");
        assert!(result.unwrap().is_ok(), "InitEvents should return normally");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "InitEvents should return noErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "InitEvents should preserve A7"
        );
    }

    // ---- Device/Interrupt no-op family ($A03D/$A03E/$A072/$A075/$A076) ----

    #[test]
    fn drvrinstall_uses_a0_driverptr_d0_refnum_and_returns_oserr_in_d0() {
        // Inside Macintosh: Devices (1994), pp. 1-83 to 1-84:
        // _DrvrInstall uses A0=drvrPtr, D0=refNum, and returns OSErr in D0.
        let (mut disp, mut cpu, mut bus) = setup();
        let driver_ptr = 0x320000;
        let stack_ptr = 0x00F0_1000;
        cpu.write_reg(Register::A0, driver_ptr);
        cpu.write_reg(Register::D0, 0xFFFF_FFEC);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_event(false, 0x3D, &mut cpu, &mut bus);
        assert!(result.is_some(), "DrvrInstall should be handled");
        assert!(
            result.unwrap().is_ok(),
            "DrvrInstall should return normally"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DrvrInstall should return noErr in D0 for nominal calls"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            driver_ptr,
            "DrvrInstall should not rewrite the A0 driver pointer"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "DrvrInstall is register-based and should preserve A7"
        );
    }

    #[test]
    fn drvrremove_uses_d0_refnum_and_returns_oserr_in_d0() {
        // Inside Macintosh: Devices (1994), pp. 1-85 to 1-86:
        // _DrvrRemove uses D0=refNum and returns OSErr in D0.
        let (mut disp, mut cpu, mut bus) = setup();
        let stack_ptr = 0x00F0_2000;
        cpu.write_reg(Register::D0, 0xFFFF_FFEC);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_event(false, 0x3E, &mut cpu, &mut bus);
        assert!(result.is_some(), "DrvrRemove should be handled");
        assert!(result.unwrap().is_ok(), "DrvrRemove should return normally");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DrvrRemove should return noErr in D0 for nominal calls"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "DrvrRemove is register-based and should preserve A7"
        );
    }

    #[test]
    fn drvrinstall_drvrremove_install_then_remove_composition_balances_stack() {
        // Mirrors the strict bake a03d_a03e_drvr_install_remove_strict
        // band B0 composite: dispatch _DrvrInstall + _DrvrRemove in
        // sequence against the same refNum=-50 slot, with a poisoned
        // sentinel above SP to verify neither trap walks past the
        // caller's stack window. Per IM:Devices 1994 pp. 1-83..1-86
        // both traps are register-only OS-bit FUNCTIONs with no Pascal
        // stack frame.
        let (mut disp, mut cpu, mut bus) = setup();
        let driver_ptr: u32 = 0x320200;
        let stack_ptr: u32 = 0x200000;
        let sentinel_addr = stack_ptr;
        let sentinel: u32 = 0xBADC_0DE0;
        let ref_num: u32 = 0xFFFF_FFCE; // -50 sign-extended to 32 bits

        bus.write_long(sentinel_addr, sentinel);

        cpu.write_reg(Register::A7, stack_ptr);

        // DrvrInstall
        cpu.write_reg(Register::A0, driver_ptr);
        cpu.write_reg(Register::D0, ref_num);
        let r1 = disp.dispatch_event(false, 0x3D, &mut cpu, &mut bus);
        assert!(r1.is_some());
        assert!(r1.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DrvrInstall composition: noErr"
        );

        // DrvrRemove against the same refNum
        cpu.write_reg(Register::D0, ref_num);
        let r2 = disp.dispatch_event(false, 0x3E, &mut cpu, &mut bus);
        assert!(r2.is_some());
        assert!(r2.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DrvrRemove composition: noErr"
        );

        // A7 unchanged after both calls.
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "Install + Remove must leave A7 untouched"
        );

        // Caller's stack memory at SP+0 untouched (no spurious writes).
        assert_eq!(
            bus.read_long(sentinel_addr),
            sentinel,
            "Install + Remove must not clobber caller memory above SP"
        );
    }

    #[test]
    fn dovbltask_uses_d0_slot_and_returns_oserr_in_d0() {
        // Inside Macintosh: Processes (1994), p. 4-27:
        // DoVBLTask uses D0=slot and returns OSErr in D0.
        let (mut disp, mut cpu, mut bus) = setup();
        let stack_ptr = 0x00F0_3000;
        cpu.write_reg(Register::D0, 0x0000_0009);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_event(false, 0x72, &mut cpu, &mut bus);
        assert!(result.is_some(), "DoVBLTask should be handled");
        assert!(result.unwrap().is_ok(), "DoVBLTask should return normally");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DoVBLTask should return noErr in D0 for nominal calls"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "DoVBLTask is register-based and should preserve A7"
        );
    }

    #[test]
    fn dovbltask_register_only_calling_convention_preserves_stack_across_mixed_slots() {
        // Mirrors B2 of a072_dovbltask_strict:
        // a 5-call composition cycling slot inputs 0 → 1 → 0 → 2 → 0
        // must leave A7 unchanged across each dispatch (register-only
        // OS-bit FUNCTION calling convention per IM:Processes 1994
        // p. 4-27 — no Pascal stack frame consumed).
        let (mut disp, mut cpu, mut bus) = setup();
        let stack_ptr = 0x00F0_3000;
        cpu.write_reg(Register::A7, stack_ptr);

        for slot in [0i32, 1, 0, 2, 0] {
            cpu.write_reg(Register::D0, slot as u32);
            let result = disp.dispatch_event(false, 0x72, &mut cpu, &mut bus);
            assert!(result.is_some(), "DoVBLTask should be handled");
            assert!(result.unwrap().is_ok(), "DoVBLTask should return normally");
            assert_eq!(
                cpu.read_reg(Register::A7),
                stack_ptr,
                "DoVBLTask must preserve A7 across dispatch with slot={slot}"
            );
        }
    }

    #[test]
    fn dovbltask_invalid_slot_returns_slotnumerr_without_consuming_stack() {
        // Inside Macintosh: Processes (1994), p. 4-27:
        // DoVBLTask returns slotNumErr for invalid slot numbers.
        let (mut disp, mut cpu, mut bus) = setup();
        let stack_ptr = 0x00F0_5000;
        cpu.write_reg(Register::D0, 16);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_event(false, 0x72, &mut cpu, &mut bus);
        assert!(result.is_some(), "DoVBLTask should be handled");
        assert!(result.unwrap().is_ok(), "DoVBLTask should return normally");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -337,
            "DoVBLTask should return slotNumErr for an invalid slot"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "DoVBLTask should preserve A7 on the invalid-slot path"
        );
    }

    #[test]
    fn attachvbl_uses_d0_slot_and_updates_primary_vbl_slot() {
        // Inside Macintosh: Processes (1994), p. 4-26:
        // AttachVBL takes a slot number in D0, returns OSErr in D0,
        // and does not consume a Pascal stack frame.
        let (mut disp, mut cpu, mut bus) = setup();
        let stack_ptr = 0x00F0_6000;
        cpu.write_reg(Register::D0, 10);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_event(false, 0x71, &mut cpu, &mut bus);
        assert!(result.is_some(), "AttachVBL should be handled");
        assert!(result.unwrap().is_ok(), "AttachVBL should return normally");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "AttachVBL should return noErr in D0 for a valid slot"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "AttachVBL is register-based and should preserve A7"
        );
        assert_eq!(
            disp.primary_vbl_slot, 10,
            "AttachVBL should record the newly selected primary slot"
        );
    }

    #[test]
    fn attachvbl_invalid_slot_returns_slotnumerr_without_changing_primary_slot() {
        // Invalid slots should reject cleanly and leave the recorded
        // primary slot unchanged.
        let (mut disp, mut cpu, mut bus) = setup();
        let stack_ptr = 0x00F0_7000;
        disp.primary_vbl_slot = 7;
        cpu.write_reg(Register::D0, 16);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_event(false, 0x71, &mut cpu, &mut bus);
        assert!(result.is_some(), "AttachVBL should be handled");
        assert!(result.unwrap().is_ok(), "AttachVBL should return normally");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -337,
            "AttachVBL should return slotNumErr for an invalid slot"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "AttachVBL should preserve A7 on an invalid-slot path"
        );
        assert_eq!(
            disp.primary_vbl_slot, 7,
            "AttachVBL should not mutate the recorded primary slot on error"
        );
    }

    #[test]
    fn sintinstall_uses_a0_qelemptr_d0_slot_and_returns_oserr_in_d0() {
        // Inside Macintosh: Devices (1994), pp. 2-70 to 2-71:
        // _SIntInstall uses A0=slot-queue element pointer, D0=slot number, and returns OSErr in D0.
        let (mut disp, mut cpu, mut bus) = setup();
        let queue_elem_ptr = 0x320100;
        let stack_ptr = 0x00F0_4000;
        bus.write_long(queue_elem_ptr, 0xA5A5_5A5A);
        bus.write_long(queue_elem_ptr + 4, 0x1122_3344);
        cpu.write_reg(Register::A0, queue_elem_ptr);
        cpu.write_reg(Register::D0, 0x0000_0009);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_event(false, 0x75, &mut cpu, &mut bus);
        assert!(result.is_some(), "SIntInstall should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SIntInstall should return normally"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SIntInstall should return noErr in D0 for nominal calls"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            queue_elem_ptr,
            "SIntInstall should not rewrite the queue-element pointer"
        );
        assert_eq!(
            bus.read_long(queue_elem_ptr),
            0xA5A5_5A5A,
            "SIntInstall no-op path should not mutate queue-element memory"
        );
        assert_eq!(
            bus.read_long(queue_elem_ptr + 4),
            0x1122_3344,
            "SIntInstall no-op path should preserve queue-element fields"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "SIntInstall is register-based and should preserve A7"
        );
    }

    #[test]
    fn sintremove_uses_a0_qelemptr_d0_slot_and_returns_oserr_in_d0() {
        // Inside Macintosh: Devices (1994), p. 2-71:
        // _SIntRemove uses A0=slot-queue element pointer, D0=slot number, and returns OSErr in D0.
        let (mut disp, mut cpu, mut bus) = setup();
        let queue_elem_ptr = 0x320200;
        let stack_ptr = 0x00F0_5000;
        bus.write_long(queue_elem_ptr, 0x55AA_33CC);
        bus.write_long(queue_elem_ptr + 4, 0xDEAD_BEEF);
        cpu.write_reg(Register::A0, queue_elem_ptr);
        cpu.write_reg(Register::D0, 0x0000_000A);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_event(false, 0x76, &mut cpu, &mut bus);
        assert!(result.is_some(), "SIntRemove should be handled");
        assert!(result.unwrap().is_ok(), "SIntRemove should return normally");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SIntRemove should return noErr in D0 for nominal calls"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            queue_elem_ptr,
            "SIntRemove should not rewrite the queue-element pointer"
        );
        assert_eq!(
            bus.read_long(queue_elem_ptr),
            0x55AA_33CC,
            "SIntRemove no-op path should not mutate queue-element memory"
        );
        assert_eq!(
            bus.read_long(queue_elem_ptr + 4),
            0xDEAD_BEEF,
            "SIntRemove no-op path should preserve queue-element fields"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "SIntRemove is register-based and should preserve A7"
        );
    }

    // ---- GetNextEvent ($A070, OS) ----

    const EVENT_PTR: u32 = 0x300000;
    const QHDR_FLAGS_OFFSET: u32 = 0;
    const QHDR_HEAD_OFFSET: u32 = 2;
    const QHDR_TAIL_OFFSET: u32 = 6;
    const QELEM_LINK_OFFSET: u32 = 0;

    /// Helper to read back an EventRecord from guest memory.
    fn read_event_record(
        bus: &crate::memory::MacMemoryBus,
        ptr: u32,
    ) -> (u16, u32, u32, u16, u16, u16) {
        let what = bus.read_word(ptr);
        let message = bus.read_long(ptr + 2);
        let when = bus.read_long(ptr + 6);
        let where_v = bus.read_word(ptr + 10);
        let where_h = bus.read_word(ptr + 12);
        let modifiers = bus.read_word(ptr + 14);
        (what, message, when, where_v, where_h, modifiers)
    }

    // ---- Enqueue ($A96F) / Dequeue ($A96E) ----

    #[test]
    fn enqueue_on_empty_queue_sets_qhead_qtail_and_terminal_link() {
        // Inside Macintosh: Operating System Utilities (1994), pp. 6-15..6-16:
        // Enqueue adds qElement to the end of qHeader and updates the queue header.
        let (mut disp, mut cpu, mut bus) = setup();
        let q_header = 0x310000;
        let q_entry = 0x310100;

        bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0xA5A5);
        bus.write_long(q_header + QHDR_HEAD_OFFSET, 0);
        bus.write_long(q_header + QHDR_TAIL_OFFSET, 0);
        bus.write_long(q_entry + QELEM_LINK_OFFSET, 0xDEAD_BEEF);

        cpu.write_reg(Register::A0, q_entry);
        cpu.write_reg(Register::A1, q_header);
        let result = disp.dispatch_event(true, 0x16F, &mut cpu, &mut bus);
        assert!(result.is_some(), "Enqueue should be handled");
        assert!(result.unwrap().is_ok(), "Enqueue should succeed");
        assert_eq!(
            bus.read_long(q_header + QHDR_HEAD_OFFSET),
            q_entry,
            "Enqueue should set qHead to inserted entry for an empty queue"
        );
        assert_eq!(
            bus.read_long(q_header + QHDR_TAIL_OFFSET),
            q_entry,
            "Enqueue should set qTail to inserted entry for an empty queue"
        );
        assert_eq!(
            bus.read_long(q_entry + QELEM_LINK_OFFSET),
            0,
            "Enqueue should terminate the inserted entry with qLink=NIL"
        );
        assert_eq!(
            bus.read_word(q_header + QHDR_FLAGS_OFFSET),
            0xA5A5,
            "Enqueue should not modify qFlags"
        );
    }

    #[test]
    fn enqueue_appends_after_existing_tail_and_preserves_qflags() {
        // Inside Macintosh: Operating System Utilities (1994), pp. 6-13..6-16:
        // QHdr stores qFlags/qHead/qTail and Enqueue appends at queue end.
        let (mut disp, mut cpu, mut bus) = setup();
        let q_header = 0x310200;
        let first_entry = 0x310300;
        let second_entry = 0x310400;

        bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0x55AA);
        bus.write_long(q_header + QHDR_HEAD_OFFSET, first_entry);
        bus.write_long(q_header + QHDR_TAIL_OFFSET, first_entry);
        bus.write_long(first_entry + QELEM_LINK_OFFSET, 0);
        bus.write_long(second_entry + QELEM_LINK_OFFSET, 0x1111_2222);

        cpu.write_reg(Register::A0, second_entry);
        cpu.write_reg(Register::A1, q_header);
        let result = disp.dispatch_event(true, 0x16F, &mut cpu, &mut bus);
        assert!(result.is_some(), "Enqueue should be handled");
        assert!(result.unwrap().is_ok(), "Enqueue should succeed");
        assert_eq!(
            bus.read_long(q_header + QHDR_HEAD_OFFSET),
            first_entry,
            "Enqueue should preserve qHead when appending"
        );
        assert_eq!(
            bus.read_long(q_header + QHDR_TAIL_OFFSET),
            second_entry,
            "Enqueue should move qTail to the appended entry"
        );
        assert_eq!(
            bus.read_long(first_entry + QELEM_LINK_OFFSET),
            second_entry,
            "Enqueue should link the previous tail to the appended entry"
        );
        assert_eq!(
            bus.read_long(second_entry + QELEM_LINK_OFFSET),
            0,
            "Enqueue should terminate the appended entry with qLink=NIL"
        );
        assert_eq!(
            bus.read_word(q_header + QHDR_FLAGS_OFFSET),
            0x55AA,
            "Enqueue should not modify qFlags"
        );
    }

    #[test]
    fn dequeue_present_entry_unlinks_element_and_returns_noerr() {
        // Inside Macintosh: Operating System Utilities (1994), p. 6-16:
        // Dequeue removes a found element, adjusts the queue, and returns noErr.
        let (mut disp, mut cpu, mut bus) = setup();
        let q_header = 0x310500;
        let first_entry = 0x310600;
        let second_entry = 0x310700;

        bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0x0F0F);
        bus.write_long(q_header + QHDR_HEAD_OFFSET, first_entry);
        bus.write_long(q_header + QHDR_TAIL_OFFSET, second_entry);
        bus.write_long(first_entry + QELEM_LINK_OFFSET, second_entry);
        bus.write_long(second_entry + QELEM_LINK_OFFSET, 0);

        cpu.write_reg(Register::A0, first_entry);
        cpu.write_reg(Register::A1, q_header);
        let result = disp.dispatch_event(true, 0x16E, &mut cpu, &mut bus);
        assert!(result.is_some(), "Dequeue should be handled");
        assert!(
            result.unwrap().is_ok(),
            "Dequeue should return from dispatch"
        );
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "Dequeue should return noErr when entry is present"
        );
        assert_eq!(
            bus.read_long(q_header + QHDR_HEAD_OFFSET),
            second_entry,
            "Dequeue should promote next entry to qHead when removing head"
        );
        assert_eq!(
            bus.read_long(q_header + QHDR_TAIL_OFFSET),
            second_entry,
            "Dequeue should update qTail when head removal leaves one entry"
        );
        assert_eq!(
            bus.read_word(q_header + QHDR_FLAGS_OFFSET),
            0x0F0F,
            "Dequeue should not modify qFlags"
        );
    }

    #[test]
    fn dequeue_missing_entry_returns_qerr_and_preserves_queue() {
        // Inside Macintosh: Operating System Utilities (1994), p. 6-16:
        // Dequeue returns qErr (-1) when the entry is not in the queue.
        let (mut disp, mut cpu, mut bus) = setup();
        let q_header = 0x310800;
        let first_entry = 0x310900;
        let second_entry = 0x310A00;
        let missing_entry = 0x310B00;

        bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0xAAAA);
        bus.write_long(q_header + QHDR_HEAD_OFFSET, first_entry);
        bus.write_long(q_header + QHDR_TAIL_OFFSET, second_entry);
        bus.write_long(first_entry + QELEM_LINK_OFFSET, second_entry);
        bus.write_long(second_entry + QELEM_LINK_OFFSET, 0);

        cpu.write_reg(Register::A0, missing_entry);
        cpu.write_reg(Register::A1, q_header);
        let result = disp.dispatch_event(true, 0x16E, &mut cpu, &mut bus);
        assert!(result.is_some(), "Dequeue should be handled");
        assert!(
            result.unwrap().is_ok(),
            "Dequeue should return from dispatch"
        );
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            -1,
            "Dequeue should return qErr for missing entry"
        );
        assert_eq!(
            bus.read_long(q_header + QHDR_HEAD_OFFSET),
            first_entry,
            "Missing-entry Dequeue should preserve qHead"
        );
        assert_eq!(
            bus.read_long(q_header + QHDR_TAIL_OFFSET),
            second_entry,
            "Missing-entry Dequeue should preserve qTail"
        );
        assert_eq!(
            bus.read_long(first_entry + QELEM_LINK_OFFSET),
            second_entry,
            "Missing-entry Dequeue should preserve existing queue links"
        );
        assert_eq!(
            bus.read_long(second_entry + QELEM_LINK_OFFSET),
            0,
            "Missing-entry Dequeue should preserve existing terminal qLink"
        );
        assert_eq!(
            bus.read_word(q_header + QHDR_FLAGS_OFFSET),
            0xAAAA,
            "Missing-entry Dequeue should preserve qFlags"
        );
    }

    #[test]
    fn enqueue_dequeue_dispatcher_convention_preserves_register_only_abi() {
        // Mirrors the strict bake
        // a96e_a96f_dequeue_enqueue_strict bands
        // B1+B2+B3+B4 as a single in-Rust sequence pinning the
        // Tool-bit Enqueue PROCEDURE / Dequeue FUNCTION register
        // calling convention (A0=qElement, A1=qHeader; D0=OSErr for
        // Dequeue). Per IM:OSUtils 1994 pp. 6-13..6-17 + IM:II 1985
        // p. II-374.
        let (mut disp, mut cpu, mut bus) = setup();
        let q_header = 0x320000;
        let elem_a = 0x320100;
        let elem_b = 0x320200;
        let elem_c = 0x320300;

        bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0x5A5A);
        bus.write_long(q_header + QHDR_HEAD_OFFSET, 0);
        bus.write_long(q_header + QHDR_TAIL_OFFSET, 0);
        bus.write_long(elem_a + QELEM_LINK_OFFSET, 0xDEAD_BEEF);
        bus.write_long(elem_b + QELEM_LINK_OFFSET, 0xCAFE_F00D);
        bus.write_long(elem_c + QELEM_LINK_OFFSET, 0xBAAD_F00D);

        // B1: Enqueue elem_a onto empty queue.
        cpu.write_reg(Register::A0, elem_a);
        cpu.write_reg(Register::A1, q_header);
        assert!(disp
            .dispatch_event(true, 0x16F, &mut cpu, &mut bus)
            .is_some());
        assert_eq!(bus.read_long(q_header + QHDR_HEAD_OFFSET), elem_a);
        assert_eq!(bus.read_long(q_header + QHDR_TAIL_OFFSET), elem_a);
        assert_eq!(bus.read_long(elem_a + QELEM_LINK_OFFSET), 0);

        // B2: Enqueue elem_b — appends after elem_a.
        cpu.write_reg(Register::A0, elem_b);
        cpu.write_reg(Register::A1, q_header);
        assert!(disp
            .dispatch_event(true, 0x16F, &mut cpu, &mut bus)
            .is_some());
        assert_eq!(bus.read_long(q_header + QHDR_HEAD_OFFSET), elem_a);
        assert_eq!(bus.read_long(q_header + QHDR_TAIL_OFFSET), elem_b);
        assert_eq!(bus.read_long(elem_a + QELEM_LINK_OFFSET), elem_b);
        assert_eq!(bus.read_long(elem_b + QELEM_LINK_OFFSET), 0);
        assert_eq!(bus.read_word(q_header + QHDR_FLAGS_OFFSET), 0x5A5A);

        // B3: Dequeue elem_a — present, returns noErr in D0.
        cpu.write_reg(Register::D0, 0x3FFF_3FFF); // poison D0
        cpu.write_reg(Register::A0, elem_a);
        cpu.write_reg(Register::A1, q_header);
        assert!(disp
            .dispatch_event(true, 0x16E, &mut cpu, &mut bus)
            .is_some());
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_long(q_header + QHDR_HEAD_OFFSET), elem_b);
        assert_eq!(bus.read_long(q_header + QHDR_TAIL_OFFSET), elem_b);

        // B4: Dequeue elem_c — not in queue, returns qErr -1 in D0.
        cpu.write_reg(Register::D0, 0x3FFF_3FFF); // poison D0
        cpu.write_reg(Register::A0, elem_c);
        cpu.write_reg(Register::A1, q_header);
        assert!(disp
            .dispatch_event(true, 0x16E, &mut cpu, &mut bus)
            .is_some());
        assert_eq!(cpu.read_reg(Register::D0) as i32, -1);
        assert_eq!(bus.read_long(q_header + QHDR_HEAD_OFFSET), elem_b);
        assert_eq!(bus.read_long(q_header + QHDR_TAIL_OFFSET), elem_b);
        assert_eq!(bus.read_word(q_header + QHDR_FLAGS_OFFSET), 0x5A5A);
    }

    #[test]
    fn get_next_event_empty_queue_returns_null_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.sent_open_app_event = true; // suppress synthetic oapp event

        // D0 = event mask (all events), A0 = event record pointer
        cpu.write_reg(Register::D0, 0xFFFF);
        cpu.write_reg(Register::A0, EVENT_PTR);

        let result = disp.dispatch_event(false, 0x70, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // D0 should be 0 (no event)
        assert_eq!(cpu.read_reg(Register::D0), 0);

        // Event record should have what=0 (null event)
        let (what, _message, _when, _where_v, _where_h, _modifiers) =
            read_event_record(&bus, EVENT_PTR);
        assert_eq!(what, 0);
    }

    #[test]
    fn get_next_event_matching_event_returns_it() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.sent_open_app_event = true; // suppress synthetic oapp event

        // Push a mouseDown event (what=1)
        disp.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0,
            where_v: 100,
            where_h: 200,
            modifiers: 0,
        });

        // D0 = 0xFFFF (all events), A0 = event record pointer
        cpu.write_reg(Register::D0, 0xFFFF);
        cpu.write_reg(Register::A0, EVENT_PTR);

        let result = disp.dispatch_event(false, 0x70, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // D0 should be $FFFF (TRUE = event found, Mac OS convention)
        assert_eq!(cpu.read_reg(Register::D0), 0xFFFF);

        let (what, _message, when, where_v, where_h, _modifiers) =
            read_event_record(&bus, EVENT_PTR);
        assert_eq!(what, 1); // mouseDown
        assert_eq!(where_v, 100);
        assert_eq!(where_h, 200);
        // when should be the tick count read from $016A (set to 100 by setup)
        assert_eq!(when, 100);

        // Queue should now be empty
        assert!(disp.event_queue.is_empty());
    }

    #[test]
    fn get_next_event_non_matching_mask_returns_no_event() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Push a mouseDown event (what=1, bit 1)
        disp.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0,
            where_v: 50,
            where_h: 60,
            modifiers: 0,
        });

        // D0 = 0x0004 (only keyDown, bit 2), A0 = event record pointer
        cpu.write_reg(Register::D0, 0x0004);
        cpu.write_reg(Register::A0, EVENT_PTR);

        let result = disp.dispatch_event(false, 0x70, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // D0 should be 0 (no matching event)
        assert_eq!(cpu.read_reg(Register::D0), 0);

        // Event should still be in the queue (not consumed)
        assert_eq!(disp.event_queue.len(), 1);
    }

    #[test]
    fn get_next_event_mouse_up_clears_mouse_button() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Simulate that mouse button is currently held
        disp.mouse_button = true;

        // Push a mouseUp event (what=2)
        disp.event_queue.push_back(QueuedEvent {
            what: 2,
            message: 0,
            where_v: 75,
            where_h: 150,
            modifiers: 0,
        });

        // D0 = mask with bit 2 set (mouseUp), A0 = event record pointer
        cpu.write_reg(Register::D0, 0x0004); // bit 2 = mouseUp
        cpu.write_reg(Register::A0, EVENT_PTR);

        assert!(disp.mouse_button);

        let result = disp.dispatch_event(false, 0x70, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // D0 should be $FFFF (TRUE = event found, Mac OS convention)
        assert_eq!(cpu.read_reg(Register::D0), 0xFFFF);

        // mouse_button should now be false (dequeue_event clears it on mouseUp)
        assert!(!disp.mouse_button);

        let (what, _message, _when, where_v, where_h, _modifiers) =
            read_event_record(&bus, EVENT_PTR);
        assert_eq!(what, 2);
        assert_eq!(where_v, 75);
        assert_eq!(where_h, 150);
    }

    // ---- EventAvail ($A071, OS) ----

    #[test]
    fn event_avail_returns_d0_zero() {
        let (mut disp, mut cpu, mut bus) = setup();

        let result = disp.dispatch_event(false, 0x71, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn rdrvrinstall_returns_d0_zero_and_preserves_stack_pointer() {
        // Inside Macintosh Volume III (1986), p. III-21: RDrvrInstall is the
        // ROM-driver install variant of DrvrInstall and returns noErr.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_pre = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, 0x1234_5678);
        cpu.write_reg(Register::D0, 7);

        let result = disp.dispatch_event(false, 0x4F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp_pre);
    }

    // ---- PostEvent ($A02F) ----

    #[test]
    fn post_event_returns_d0_zero() {
        let (mut disp, mut cpu, mut bus) = setup();

        let result = disp.dispatch_event(false, 0x2F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // ---- OSEventAvail ($A030) ----

    #[test]
    fn os_event_avail_returns_d0_ffff_when_empty() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0xFFFF);
        cpu.write_reg(Register::A0, EVENT_PTR);

        let result = disp.dispatch_event(false, 0x30, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        // D0=$FFFF = null event (FALSE) per TB Essentials 1992, p. 2-98
        assert_eq!(cpu.read_reg(Register::D0), 0xFFFF);

        let (what, _message, _when, _where_v, _where_h, modifiers) =
            read_event_record(&bus, EVENT_PTR);
        assert_eq!(what, 0);
        assert_eq!(modifiers & 0x0080, 0x0080);
    }

    // ---- GetOSEvent ($A031) ----

    #[test]
    fn get_os_event_returns_d0_ffff_when_empty() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0xFFFF);
        cpu.write_reg(Register::A0, EVENT_PTR);

        let result = disp.dispatch_event(false, 0x31, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        // D0=$FFFF = null event (FALSE) per TB Essentials 1992, p. 2-97
        assert_eq!(cpu.read_reg(Register::D0), 0xFFFF);

        let (what, _message, _when, _where_v, _where_h, modifiers) =
            read_event_record(&bus, EVENT_PTR);
        assert_eq!(what, 0);
        assert_eq!(modifiers & 0x0080, 0x0080);
    }

    #[test]
    fn get_os_event_mouse_up_reports_button_up_modifier() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.push_mouse_down(75, 150);
        let _ = disp.dequeue_event(0xFFFF);
        disp.push_mouse_up(75, 150);

        cpu.write_reg(Register::D0, 0x0004);
        cpu.write_reg(Register::A0, EVENT_PTR);

        let result = disp.dispatch_event(false, 0x31, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        // D0=0 = event found (TRUE) per TB Essentials 1992, p. 2-97
        assert_eq!(cpu.read_reg(Register::D0), 0);

        let (what, _message, _when, where_v, where_h, modifiers) =
            read_event_record(&bus, EVENT_PTR);
        assert_eq!(what, 2);
        assert_eq!(where_v, 75);
        assert_eq!(where_h, 150);
        assert_eq!(modifiers & 0x0080, 0x0080);
    }
}
