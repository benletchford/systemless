//! Process-owned Time Manager and Vertical Retrace Manager task records.

use std::collections::HashMap;

/// Scheduling metadata shared by every callback gateway in one process.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessCallbackScheduling {
    /// Exact intended wake-up for extended Time Manager records.
    pub(crate) extended_wakeups: HashMap<u32, u64>,
    /// Exact Time Manager clock while callbacks are delivered.
    pub(crate) current_subtick: u64,
    /// Guest address of the dormant system VBL queue element.
    pub(crate) system_vbl_queue_anchor: u32,
    /// Slot number of the primary video monitor.
    pub(crate) primary_vbl_slot: i16,
}

impl ProcessCallbackScheduling {
    /// Returns true if scheduling metadata is at its pristine default state.
    pub(crate) fn is_pristine(&self) -> bool {
        self.extended_wakeups.is_empty()
            && self.current_subtick == 0
            && self.system_vbl_queue_anchor == 0
            && self.primary_vbl_slot == 0
    }

    /// Exact Time Manager clock while callbacks are delivered.
    pub(crate) fn current_subtick(&self) -> u64 {
        self.current_subtick
    }

    /// Set the exact Time Manager clock.
    pub(crate) fn set_current_subtick(&mut self, subtick: u64) {
        self.current_subtick = subtick;
    }

    /// Advance the Time Manager clock if `min_subtick` exceeds the current clock,
    /// returning the new current subtick.
    pub(crate) fn advance_current_subtick_min(&mut self, min_subtick: u64) -> u64 {
        self.current_subtick = self.current_subtick.max(min_subtick);
        self.current_subtick
    }

    /// Get the recorded intended wakeup for an extended Time Manager task record.
    pub(crate) fn extended_wakeup(&self, task_ptr: u32) -> Option<u64> {
        self.extended_wakeups.get(&task_ptr).copied()
    }

    /// Record the intended wakeup for an extended Time Manager task record.
    pub(crate) fn set_extended_wakeup(&mut self, task_ptr: u32, wakeup: u64) {
        self.extended_wakeups.insert(task_ptr, wakeup);
    }

    /// Remove the recorded intended wakeup for an extended Time Manager task record.
    pub(crate) fn remove_extended_wakeup(&mut self, task_ptr: u32) -> Option<u64> {
        self.extended_wakeups.remove(&task_ptr)
    }

    /// Guest address of the dormant system VBL queue element.
    pub(crate) fn system_vbl_queue_anchor(&self) -> u32 {
        self.system_vbl_queue_anchor
    }

    /// Record the guest address of the dormant system VBL queue element.
    pub(crate) fn set_system_vbl_queue_anchor(&mut self, anchor: u32) {
        self.system_vbl_queue_anchor = anchor;
    }

    /// Slot number of the primary video monitor.
    pub(crate) fn primary_vbl_slot(&self) -> i16 {
        self.primary_vbl_slot
    }

    /// Record the slot number of the primary video monitor.
    pub(crate) fn set_primary_vbl_slot(&mut self, slot: i16) {
        self.primary_vbl_slot = slot;
    }
}

/// CPU architecture responsible for delivering an installed callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallbackTaskArchitecture {
    /// Classic 68K callback delivery.
    M68k,
    /// Native PowerPC callback delivery.
    PowerPc,
}

/// An installed Time Manager task shared by every CPU adapter.
///
/// The Time Manager owns one operating-system queue for the current process;
/// the task record remains guest-visible while its exact deadline is private
/// manager state. Inside Macintosh: Processes (1994), pp. 3-6--3-22.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessTimerTask {
    /// Guest address of the `TMTask` record.
    pub task_ptr: u32,
    /// Architecture that installed and delivers this callback.
    pub architecture: CallbackTaskArchitecture,
    /// Whether `InsXTime` installed the extended, drift-free record form.
    pub extended: bool,
    /// Address of the callback procedure from `tmAddr`.
    pub callback: u32,
    /// Whether the task is primed and waiting to fire.
    pub active: bool,
    /// Guest tick at which this task should fire.
    pub fire_at_tick: u32,
    /// Exact deadline in millionths of a 60 Hz guest tick.
    pub fire_at_subtick: u64,
    /// VBL tick in which this task was most recently dispatched.
    pub last_fired_tick: Option<u32>,
}

/// An installed Vertical Retrace Manager task shared by every CPU adapter.
///
/// Inside Macintosh: Processes (1994), pp. 4-6--4-12.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessVblTask {
    /// Guest address of the `VBLTask` record.
    pub task_ptr: u32,
    /// Architecture that installed and delivers this callback.
    pub architecture: CallbackTaskArchitecture,
    /// Optional slot number for slot-based VBL tasks.
    pub slot: Option<i16>,
    /// The task reached a zero count but has not yet received its callback.
    pub pending: bool,
}
