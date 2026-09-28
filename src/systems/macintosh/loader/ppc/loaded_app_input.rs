//! PowerPC loaded application input snapshot, event queue, and cursor state
//! methods on [`PpcLoadedApp`].

use super::*;
use crate::memory::globals::addr;

impl PpcLoadedApp {
    pub fn set_input_snapshot(&mut self, input: PpcInputSnapshot) {
        self.mirror_input_low_memory(input);
        self.input = input;
        self.process_input.set_key_map_snapshot(input.key_map);
        self.process_input
            .set_mouse_state((input.mouse_v, input.mouse_h), input.mouse_button);
    }

    pub(crate) fn mirror_input_low_memory(&mut self, input: PpcInputSnapshot) {
        // Native mouse and keyboard drivers mirror the current device state
        // into low memory. PowerPC applications may poll these globals
        // directly instead of calling Button, GetMouse, or GetKeys.
        let _ = self
            .memory
            .write_u8(addr::MB_STATE, if input.mouse_button { 0x00 } else { 0x80 });
        let _ = self.memory.write_bytes(addr::KEY_MAP_LM, &input.key_map);
        for point_addr in [addr::M_TEMP, addr::MOUSE_LOC, addr::MOUSE_LOC2] {
            let _ = self.memory.write_u16_be(point_addr, input.mouse_v as u16);
            let _ = self
                .memory
                .write_u16_be(point_addr + 2, input.mouse_h as u16);
        }
    }

    pub(crate) fn current_input_snapshot(&self) -> PpcInputSnapshot {
        let key_map = self.process_input.key_map_snapshot();
        let ((mouse_v, mouse_h), mouse_button) = self.process_input.mouse_state_snapshot();
        PpcInputSnapshot {
            key_map,
            mouse_button,
            mouse_v,
            mouse_h,
        }
    }

    pub fn set_event_queue<I>(&mut self, events: I)
    where
        I: IntoIterator<Item = PpcQueuedEvent>,
    {
        self.event_queue.clear();
        self.event_queue.extend(events);
    }

    #[cfg(test)]
    pub(crate) fn event_queue(&self) -> &SharedProcessEventQueue {
        &self.event_queue
    }

    pub fn cursor_level(&self) -> i16 {
        self.cursor_state.level()
    }

    pub fn cursor_data(&self) -> Option<([u8; 32], [u8; 32], i16, i16)> {
        self.cursor_state.mono_parts()
    }
}
