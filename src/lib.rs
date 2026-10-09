//! High-Level Emulation (HLE) for classic Macintosh applications.
//!
//! `systemless` runs Mac OS Toolbox apps without a real ROM by intercepting
//! 68k A-line trap instructions (`$A000`–`$AFFF`) and dispatching them
//! to native Rust handlers. QuickDraw, the Window Manager, the Resource
//! Manager, the Sound Manager, SANE, and the rest of the supported Toolbox
//! surface are reimplemented in Rust. The [`m68k`] crate executes guest CPU
//! instructions and models generation-specific architectural state.
//!
//! # Execution model
//!
//! [`MacintoshSession`](systems::macintosh::session::MacintoshSession) is the
//! embedding entry point. Its underlying
//! [`FixtureRunner`](systems::macintosh::runner::FixtureRunner) owns the CPU,
//! guest memory, and Toolbox dispatcher. Precise single-instruction work uses
//! [`m68k::CpuCore::step`]. Budgeted execution uses
//! [`m68k::CpuCore::run_batch`], with FastMem for ordinary guest RAM and
//! Cranelift-compiled hot traces on native targets. WebAssembly uses m68k's
//! portable trace executor; the guest-visible CPU and HLE contracts are the
//! same in both modes.
//!
//! The library exposes the full [`m68k::CpuCore`] through
//! [`M68kCpu::core`](systems::macintosh::cpu::M68kCpu::core) for diagnostics and specialized
//! embedding, while [`CpuOps`](systems::macintosh::cpu::CpuOps) is the narrower register interface used by
//! Toolbox handlers.
//!
//! # Quick start
//!
//! ```no_run
//! use systemless::api::InstructionBudget;
//! use systemless::systems::macintosh::session::MacintoshSession;
//!
//! let mut session = MacintoshSession::new(true, None);
//!
//! // Load a Mac executable (StuffIt archive, MacBinary, or raw
//! // resource fork — the loader auto-detects the format).
//! let bytes = std::fs::read("MyGame.sit").unwrap();
//! let app = session.load_bytes(&bytes).unwrap();
//! session.initialize(&app);
//!
//! // Bound guest work explicitly; the runner retains its guest clock cadence.
//! let result = session.advance(InstructionBudget(100_000));
//! println!("ran {} instructions, running = {}", result.instructions, result.running);
//! ```
//!
//! [`m68k`]: https://crates.io/crates/m68k

#![deny(rustdoc::broken_intra_doc_links)]

pub mod api;
mod error;
mod fast_hash;
pub mod systems;

// Compatibility module paths for existing embedders.
pub(crate) use systems::macintosh::adb;
#[deprecated(note = "use `systemless::systems::macintosh::audio`")]
pub mod audio {
    pub use crate::systems::macintosh::audio::*;
}
#[deprecated(note = "use `systemless::systems::macintosh::binhex`")]
pub mod binhex {
    pub use crate::systems::macintosh::binhex::*;
}
#[deprecated(note = "use `systemless::systems::macintosh::callback_manager`")]
pub mod callback_manager {
    pub use crate::systems::macintosh::callback_manager::*;
}
pub(crate) use systems::macintosh::cfm;
pub(crate) use systems::macintosh::collection_manager;
pub(crate) use systems::macintosh::control_manager;
pub(crate) use systems::macintosh::copy_bits;
#[deprecated(note = "use `systemless::systems::macintosh::cpu`")]
pub mod cpu {
    pub use crate::systems::macintosh::cpu::*;
}
#[cfg(feature = "debug")]
#[deprecated(note = "use `systemless::systems::macintosh::debug`")]
pub mod debug {
    pub use crate::systems::macintosh::debug::*;
}
#[deprecated(note = "use `systemless::systems::macintosh::debug_overlay`")]
pub mod debug_overlay {
    pub use crate::systems::macintosh::debug_overlay::*;
}
pub(crate) use systems::macintosh::cursor_manager;
pub(crate) use systems::macintosh::desk_manager;
pub(crate) use systems::macintosh::dialog_manager;
#[deprecated(note = "use `systemless::systems::macintosh::disk_image`")]
pub mod disk_image {
    pub use crate::systems::macintosh::disk_image::*;
}
#[deprecated(note = "use `systemless::systems::macintosh::display`")]
pub mod display {
    pub use crate::systems::macintosh::display::*;
}
pub(crate) use systems::macintosh::event_queue;
pub(crate) use systems::macintosh::execution_kernel;
pub(crate) use systems::macintosh::execution_m68k;
pub(crate) use systems::macintosh::execution_native;
#[deprecated(note = "use `systemless::systems::macintosh::game`")]
pub mod game {
    pub use crate::systems::macintosh::game::*;
}
pub(crate) use systems::macintosh::guest_call;
pub(crate) use systems::macintosh::guest_procedure;
pub(crate) use systems::macintosh::list_manager;
#[deprecated(note = "use `systemless::systems::macintosh::loader`")]
pub mod loader {
    pub use crate::systems::macintosh::loader::*;
}
pub(crate) use systems::macintosh::mac_roman;
#[deprecated(note = "use `systemless::systems::macintosh::machine_profile`")]
pub mod machine_profile {
    pub use crate::systems::macintosh::machine_profile::*;
}
#[deprecated(note = "use `systemless::systems::macintosh::managers`")]
pub mod managers {
    pub use crate::systems::macintosh::managers::*;
}
#[deprecated(note = "use `systemless::systems::macintosh::memory`")]
pub mod memory {
    pub use crate::systems::macintosh::memory::*;
}
pub(crate) use systems::macintosh::menu_manager;
#[deprecated(note = "use `systemless::systems::macintosh::menu_model`")]
pub mod menu_model {
    pub use crate::systems::macintosh::menu_model::*;
}
pub(crate) use systems::macintosh::mixed_mode;
pub(crate) use systems::macintosh::process_context;
pub(crate) use systems::macintosh::process_manager;
#[deprecated(note = "use `systemless::systems::macintosh::quickdraw`")]
pub mod quickdraw {
    pub use crate::systems::macintosh::quickdraw::*;
}
#[deprecated(note = "use `systemless::systems::macintosh::runner`")]
pub mod runner {
    pub use crate::systems::macintosh::runner::*;
}
pub(crate) use systems::macintosh::scrap_manager;
#[cfg(feature = "test-support")]
#[deprecated(note = "use `systemless::systems::macintosh::scripted_traces`")]
pub mod scripted_traces {
    pub use crate::systems::macintosh::scripted_traces::*;
}
#[deprecated(note = "use `systemless::systems::macintosh::sound`")]
pub mod sound {
    pub use crate::systems::macintosh::sound::*;
}
pub(crate) use systems::macintosh::text_edit;
pub(crate) use systems::macintosh::thread_manager;
pub(crate) use systems::macintosh::time_manager;
#[deprecated(note = "use `systemless::systems::macintosh::trace`")]
pub mod trace {
    pub use crate::systems::macintosh::trace::*;
}
#[deprecated(note = "use `systemless::systems::macintosh::trap`")]
pub mod trap {
    pub use crate::systems::macintosh::trap::*;
}
pub(crate) use systems::macintosh::ui_art;
#[deprecated(note = "use `systemless::systems::macintosh::ui_theme`")]
pub mod ui_theme {
    pub use crate::systems::macintosh::ui_theme::*;
}
pub(crate) use systems::macintosh::window_manager;

pub use error::{Error, Result};
pub use event_queue::{
    EventManagerSnapshot, EventProbeResult, EventQueueProbeSnapshot, EventRecordSnapshot,
};
