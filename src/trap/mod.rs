//! Trap Dispatcher - modular Mac OS trap handling.
//!
//! The dispatcher routes A-line traps to per-manager handler modules:
//! - `memory` — Memory Manager (NewPtr, NewHandle, BlockMove, etc.)
//! - `event` — Event Manager OS traps (FlushEvents, GetNextEvent, etc.)
//! - `resource` — Resource Manager + File Manager (GetResource, FSRead, etc.)
//! - `quickdraw` — QuickDraw (port, pen, text, shapes, CopyBits, etc.)
//! - `menu` — Menu Manager (NewMenu, DrawMenuBar, etc.)
//! - `window` — Window Manager (NewWindow, GetNewWindow, etc.)
//! - `dialog` — Dialog Manager + Cursor Manager stubs
//! - `toolbox` — Toolbox utilities (Random, TickCount, Sound, etc.)
//! - `shapes` — Shape computation helpers (draw_rect, draw_oval, etc.)
//! - `text_render` — Text rendering helpers (draw_char, draw_string, etc.)
//! - `framebuffer` — Framebuffer helpers + chrome rendering

mod control;
mod dialog;
mod cinepak;
pub mod dispatch;
mod event;
pub(crate) mod extended80;
mod framebuffer;
mod memory;
mod menu;
mod movie_media;
mod pict;
mod qtrle;
mod quickdraw;
mod resource;
mod sane;
mod shapes;
mod smc;
mod sound;
mod text_render;
mod toolbox;
mod types;
mod window;

pub use dispatch::TrapDispatcher;

/// Test helpers for inline trap unit tests within this crate.
/// Gated behind #[cfg(test)] so it does NOT ship in the production library.
#[cfg(test)]
pub(crate) mod test_helpers;
