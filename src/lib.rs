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

mod adb;
pub mod audio;
pub mod binhex;
pub mod callback_manager;
mod control_manager;
pub mod cpu;
pub mod debug_overlay;
pub mod disk_image;
pub mod display;
mod error;
mod event_queue;
pub use event_queue::{
    EventManagerSnapshot, EventProbeResult, EventQueueProbeSnapshot, EventRecordSnapshot,
};
mod execution_kernel;
mod execution_m68k;
pub mod game;
mod guest_call;
mod guest_procedure;
mod list_manager;
pub mod loader;
mod mac_roman;
pub mod machine_profile;
pub mod managers;
pub mod memory;
mod menu_manager;
pub mod menu_model;
mod mixed_mode;
mod process_context;
pub mod quickdraw;
pub mod runner;
/// Deterministic trap-interaction replays. This is internal test
/// scaffolding, not part of the runtime API, so it is gated behind the
/// off-by-default `test-support` feature and is absent from normal builds
/// and docs.
#[cfg(feature = "test-support")]
pub mod scripted_traces;
pub mod sound;
mod text_edit;
mod thread_manager;
pub mod trace;
pub mod trap;
mod ui_art;
pub mod ui_theme;
mod window_manager;

pub use error::{Error, Result};
