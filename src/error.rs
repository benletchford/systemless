//! Error types surfaced by the public API.

use thiserror::Error;

/// `Result<T, Error>` shorthand for any systemless operation.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error returned by [`crate::runner::FixtureRunner`] and
/// the supporting traps. Variants:
///
/// * [`Error::UnimplementedTrap`] — the dispatcher reached an A-line
///   trap word with no matching handler. The trap word is included
///   for diagnosis. Adding a `(is_tool, trap_num) =>` arm in the
///   appropriate `src/trap/*.rs` file is how you fix this.
///
/// * [`Error::TrapTableInitialization`] — guest memory cannot hold a complete
///   classic trap topology; initialization leaves it unchanged.
///
/// * [`Error::TrapTableLookup`] — a live trap table entry cannot be resolved.
///
/// * [`Error::Halted`] — the guest application called `ExitToShell`
///   (or otherwise reached a halt state). Not a failure per se;
///   callers that loop on `run_steps` use `still_running == false`
///   from the tuple instead of catching this.
///
/// * [`Error::Timeout`] — execution exceeded a caller-supplied
///   instruction budget without halting. The carried `usize` is the
///   instruction-count cap that was hit.
///
/// * [`Error::Trace`] — recording-side error from the cross-runtime
///   parity trace sink (filesystem write, JSON serialisation, etc).
///   Surfaces only when a trace sink is installed via
///   [`FixtureRunner::set_trace_sink`](crate::runner::FixtureRunner::set_trace_sink).
#[derive(Debug, Error)]
pub enum Error {
    /// The dispatcher reached an A-line trap word with no matching
    /// handler. The trap word is included for diagnosis.
    #[error("Unimplemented trap ${0:04X}")]
    UnimplementedTrap(u16),

    /// The supplied guest memory cannot hold a complete classic trap topology.
    #[error("Trap tables cannot be initialized in the supplied guest memory")]
    TrapTableInitialization,

    /// A live trap table entry cannot be resolved through its protected chain.
    #[error("Cannot resolve trap table entry for ${0:04X}")]
    TrapTableLookup(u16),

    /// The guest application reached a halt state (typically via
    /// `ExitToShell`).
    #[error("Application halted")]
    Halted,

    /// Execution exceeded the caller-supplied instruction budget
    /// without halting. The carried value is the instruction count
    /// at the timeout.
    #[error("Execution timeout after {0} instructions")]
    Timeout(usize),

    /// Cross-runtime parity trace sink returned an error (filesystem
    /// write, JSON serialisation, etc).
    #[error("Trace error: {0}")]
    Trace(String),
}
