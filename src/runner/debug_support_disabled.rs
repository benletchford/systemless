use super::*;

impl FixtureRunner {
    pub(super) fn debug_finish_step_if_ready(&mut self) -> bool {
        false
    }

    pub(super) fn debug_stop_at_m68k_breakpoint(&mut self, _address: u32) -> bool {
        false
    }

    pub(super) fn debug_note_m68k_executed_units(&mut self, _units: usize) {}

    pub(super) fn debug_note_terminal(&mut self) {}

    pub(super) fn debug_note_active_context(&mut self) {}

    #[inline]
    pub(super) fn debug_advance_generation(&mut self) {}

    #[inline]
    pub(super) fn debug_step_units_remaining(&self) -> Option<u32> {
        None
    }

    #[inline]
    pub(super) fn debug_breakpoint_rearming(&self) -> bool {
        false
    }

    pub(super) fn debug_m68k_breakpoint_addresses(&self) -> Vec<u32> {
        Vec::new()
    }

    pub(super) fn debug_note_logical_frame_boundary(&mut self) {}

    pub(super) fn debug_has_pending_logical_frame_capture(&self) -> bool {
        false
    }
}
