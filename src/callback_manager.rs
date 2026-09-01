//! Process-owned Time Manager and Vertical Retrace Manager task records.

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
