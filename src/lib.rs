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
//! [`FixtureRunner`](runner::FixtureRunner) owns the CPU, guest memory, and
//! Toolbox dispatcher. Precise single-instruction work uses
//! [`m68k::CpuCore::step`]. Budgeted execution uses
//! [`m68k::CpuCore::run_batch`], with FastMem for ordinary guest RAM and
//! Cranelift-compiled hot traces on native targets. WebAssembly uses m68k's
//! portable trace executor; the guest-visible CPU and HLE contracts are the
//! same in both modes.
//!
//! The library exposes the full [`m68k::CpuCore`] through
//! [`M68kCpu::core`](cpu::M68kCpu::core) for diagnostics and specialized
//! embedding, while [`cpu::CpuOps`] is the narrower register interface used by
//! Toolbox handlers.
//!
//! # Quick start
//!
//! ```no_run
//! use systemless::runner::{FixtureRunner, FixtureRunnerConfig};
//!
//! // Allocate an 8 MiB guest with guest-controlled menu visibility and
//! // arrow keys left as literal arrow keys.
//! let config = FixtureRunnerConfig::default();
//! let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
//!
//! // Load a Mac executable (StuffIt archive, MacBinary, or raw
//! // resource fork — the loader auto-detects the format).
//! let bytes = std::fs::read("MyGame.sit").unwrap();
//! let _app = systemless::game::load_game(&mut runner, &bytes).unwrap();
//!
//! // Step the guest until it halts or the budget runs out.
//! // The bool is `still_running` — false means the CPU halted.
//! let (steps_taken, still_running) = runner.run_steps(100_000, None);
//! println!("ran {} steps, still_running = {}", steps_taken, still_running);
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
pub use systems::macintosh::audio;
pub use systems::macintosh::binhex;
pub use systems::macintosh::callback_manager;
pub(crate) use systems::macintosh::cfm;
pub(crate) use systems::macintosh::collection_manager;
pub(crate) use systems::macintosh::control_manager;
pub(crate) use systems::macintosh::copy_bits;
pub use systems::macintosh::cpu;
#[cfg(feature = "debug")]
pub use systems::macintosh::debug;
pub use systems::macintosh::debug_overlay;
pub use systems::macintosh::disk_image;
pub use systems::macintosh::display;
pub(crate) use systems::macintosh::event_queue;
pub(crate) use systems::macintosh::execution_kernel;
pub(crate) use systems::macintosh::execution_m68k;
pub(crate) use systems::macintosh::execution_native;
pub use systems::macintosh::game;
pub(crate) use systems::macintosh::guest_call;
pub(crate) use systems::macintosh::guest_procedure;
pub(crate) use systems::macintosh::list_manager;
pub use systems::macintosh::loader;
pub(crate) use systems::macintosh::mac_roman;
pub use systems::macintosh::machine_profile;
pub use systems::macintosh::managers;
pub use systems::macintosh::memory;
pub(crate) use systems::macintosh::menu_manager;
pub use systems::macintosh::menu_model;
pub(crate) use systems::macintosh::mixed_mode;
pub(crate) use systems::macintosh::process_context;
pub(crate) use systems::macintosh::process_manager;
pub use systems::macintosh::quickdraw;
pub use systems::macintosh::runner;
#[cfg(feature = "test-support")]
pub use systems::macintosh::scripted_traces;
pub use systems::macintosh::sound;
pub(crate) use systems::macintosh::text_edit;
pub(crate) use systems::macintosh::thread_manager;
pub use systems::macintosh::trace;
pub use systems::macintosh::trap;
pub(crate) use systems::macintosh::ui_art;
pub use systems::macintosh::ui_theme;
pub(crate) use systems::macintosh::window_manager;

pub use error::{Error, Result};
pub use event_queue::{
    EventManagerSnapshot, EventProbeResult, EventQueueProbeSnapshot, EventRecordSnapshot,
};
