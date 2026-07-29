//! High-Level Emulation (HLE) for classic Macintosh applications.
//!
//! `systemless` runs Mac OS Toolbox apps without a real ROM by intercepting
//! 68k A-line trap instructions (`$A000`–`$AFFF`) and dispatching them
//! to native Rust handlers — the QuickDraw, Window Manager, Resource
//! Manager, Sound Manager, SANE, and the rest of the Toolbox are
//! reimplemented natively. The 68k CPU itself is interpreted by the
//! [`m68k`] crate.
//!
//! See the crate's `README.md` for the architecture overview.
//!
//! # Quick start
//!
//! ```no_run
//! use systemless::runner::{FixtureRunner, FixtureRunnerConfig};
//!
//! // Allocate an 8 MiB guest, default config (kiosk mode — Mac menu
//! // bar suppressed; arrow keys not remapped to numpad).
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

mod adb;
pub mod audio;
pub mod binhex;
pub mod cpu;
pub mod debug_overlay;
pub mod disk_image;
pub mod display;
mod error;
pub mod game;
pub mod loader;
pub mod machine_profile;
pub mod managers;
pub mod memory;
pub mod menu_model;
pub mod quickdraw;
pub mod runner;
/// Deterministic trap-interaction replays. This is internal test
/// scaffolding, not part of the runtime API, so it is gated behind the
/// off-by-default `test-support` feature and is absent from normal builds
/// and docs.
#[cfg(feature = "test-support")]
pub mod scripted_traces;
pub mod sound;
pub mod trace;
pub mod trap;
mod ui_art;
pub mod ui_theme;

pub use error::{Error, Result};
