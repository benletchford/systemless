//! PowerPC loaded application tick timing and clock cycle timing methods on [`PpcLoadedApp`].

use super::*;
use crate::memory::globals::addr;

impl PpcLoadedApp {
    pub fn set_tick_count(&mut self, tick_count: u32) {
        // This explicit setter is a guest-memory operation for detached
        // loader/test setup. Production execution reads `$016A` directly;
        // no adapter scalar is projected back into guest memory implicitly.
        self.tick_state.set_tick(tick_count);
        let _ = self.memory.write_u32_be(addr::TICKS, tick_count);
        self.callback_scheduling
            .advance_current_subtick_min(u64::from(tick_count) * 1_000_000);
    }

    pub(crate) fn current_tick(&mut self) -> u32 {
        let guest_ticks = self
            .memory
            .read_u32_be(addr::TICKS)
            .unwrap_or_else(|| self.tick_state.current_tick());
        self.tick_state.read_tick_count(guest_ticks);
        guest_ticks
    }

    pub(crate) fn publish_tick(&mut self, candidate: u32) -> u32 {
        // A guest store may have happened during the native slice. Import it
        // before applying the host's next VBL candidate. Only the current
        // value or its single next host VBL are valid candidates: a caller's
        // pre-slice value may be arbitrarily stale after a direct guest store,
        // and SharedProcessTickState's wrapping "newer" comparison alone
        // cannot distinguish that stale value from legitimate forward time.
        let guest_ticks = self.current_tick();
        let tick = if candidate == guest_ticks || candidate == guest_ticks.wrapping_add(1) {
            self.tick_state.publish_tick(candidate)
        } else {
            guest_ticks
        };
        let _ = self.memory.write_u32_be(addr::TICKS, tick);
        tick
    }

    /// Apply one tick from a host cycle epoch that has already been charged by
    /// the runner. Unlike [`Self::publish_tick`], this is an explicit replay
    /// operation: the runner owns the trusted baseline and elapsed-VBL count,
    /// so intermediate ticks must be visible to callbacks even though the
    /// guest bytes were advanced to the epoch's final tick before replay.
    pub(crate) fn publish_host_epoch_tick(&mut self, tick: u32) -> u32 {
        self.tick_state.set_tick(tick);
        let _ = self.memory.write_u32_be(addr::TICKS, tick);
        tick
    }

    pub fn set_clock_cycle_timing(&mut self, cycles_per_tick: u32, cycle_phase: u32) {
        self.clock_cycles_per_tick = cycles_per_tick.max(1);
        self.clock_cycle_phase = cycle_phase.min(self.clock_cycles_per_tick.saturating_sub(1));
    }
}
