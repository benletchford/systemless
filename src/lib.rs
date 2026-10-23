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
pub mod gpu_compute;
pub mod systems;

// Short crate-internal paths for Macintosh modules. The public paths are
// `systemless::systems::macintosh::<module>`.
pub(crate) use systems::macintosh::adb;
pub(crate) use systems::macintosh::audio;
pub(crate) use systems::macintosh::binhex;
pub(crate) use systems::macintosh::callback_manager;
pub(crate) use systems::macintosh::cfm;
pub(crate) use systems::macintosh::collection_manager;
pub(crate) use systems::macintosh::control_manager;
pub(crate) use systems::macintosh::copy_bits;
pub(crate) use systems::macintosh::cpu;
pub(crate) use systems::macintosh::cursor_manager;
pub(crate) use systems::macintosh::date_format;
#[cfg(feature = "debug")]
pub(crate) use systems::macintosh::debug;
pub(crate) use systems::macintosh::debug_overlay;
pub(crate) use systems::macintosh::desk_manager;
pub(crate) use systems::macintosh::dialog_manager;
pub(crate) use systems::macintosh::disk_image;
pub(crate) use systems::macintosh::disk_init_manager;
pub(crate) use systems::macintosh::display;
pub(crate) use systems::macintosh::drag_manager;
pub(crate) use systems::macintosh::event_queue;
pub(crate) use systems::macintosh::execution_kernel;
pub(crate) use systems::macintosh::execution_m68k;
pub(crate) use systems::macintosh::execution_native;
pub(crate) use systems::macintosh::game;
pub(crate) use systems::macintosh::gestalt_manager;
pub(crate) use systems::macintosh::guest_call;
pub(crate) use systems::macintosh::guest_procedure;
pub(crate) use systems::macintosh::list_manager;
pub(crate) use systems::macintosh::loader;
pub(crate) use systems::macintosh::mac_roman;
pub(crate) use systems::macintosh::machine_profile;
pub(crate) use systems::macintosh::managers;
pub(crate) use systems::macintosh::memory;
pub(crate) use systems::macintosh::menu_manager;
pub(crate) use systems::macintosh::menu_model;
pub(crate) use systems::macintosh::mixed_mode;
pub(crate) use systems::macintosh::notification_manager;
pub(crate) use systems::macintosh::process_context;
pub(crate) use systems::macintosh::process_manager;
pub(crate) use systems::macintosh::printing_manager;
pub(crate) use systems::macintosh::queue_utilities;
pub(crate) use systems::macintosh::quickdraw;
pub(crate) use systems::macintosh::runner;
pub(crate) use systems::macintosh::scrap_manager;
pub(crate) use systems::macintosh::sound;
pub(crate) use systems::macintosh::text_edit;
pub(crate) use systems::macintosh::text_utils;
pub(crate) use systems::macintosh::thread_manager;
pub(crate) use systems::macintosh::time_manager;
pub(crate) use systems::macintosh::trace;
pub(crate) use systems::macintosh::trap;
pub(crate) use systems::macintosh::ui_art;
pub(crate) use systems::macintosh::ui_theme;
pub(crate) use systems::macintosh::window_manager;

pub use error::{Error, Result};
pub use event_queue::{
    EventManagerSnapshot, EventProbeResult, EventQueueProbeSnapshot, EventRecordSnapshot,
};
