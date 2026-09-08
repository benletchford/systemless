//! Trap Dispatcher - modular Mac OS trap handling.
//!
//! The dispatcher routes A-line traps to per-manager handler modules:
//! - `memory` — Memory Manager (NewPtr, NewHandle, BlockMove, etc.)
//! - `event` — Event Manager OS traps (FlushEvents, GetNextEvent, etc.)
//! - `resource` — Resource Manager + File Manager (GetResource, FSRead, etc.)
//! - `quickdraw` — QuickDraw (port, pen, text, shapes, CopyBits, etc.)
//! - `menu` — Menu Manager (NewMenu, DrawMenuBar, etc.)
//! - `window` — Window Manager (NewWindow, GetNewWindow, etc.)
//! - `dialog` — Dialog Manager + Cursor Manager
//! - `toolbox` — Toolbox utilities (Random, TickCount, Sound, etc.)
//! - `shapes` — Shape computation helpers (draw_rect, draw_oval, etc.)
//! - `text_render` — Text rendering helpers (draw_char, draw_string, etc.)
//! - `framebuffer` — Framebuffer helpers + chrome rendering

mod cinepak;
mod control;
mod dialog;
pub mod dispatch;
mod event;
pub(crate) mod extended80;
mod framebuffer;
pub(crate) mod gateways;
pub(crate) mod manager;
mod memory;
pub(crate) mod menu;
mod movie_media;
pub(crate) mod pict;
mod qtrle;
mod quickdraw;
mod resource;
mod sane;
mod shapes;
mod smc;
mod sound;
mod text_render;
mod toolbox;
pub(crate) mod types;
mod window;

pub use dispatch::TrapDispatcher;
pub(crate) use memory::mac_roman_to_upper;
pub(crate) use sound::{
    decode_interleaved_stereo_samples, decode_mace3_mono_to_u8, decode_mace6_mono_to_u8,
    parse_aiff_samples,
};

/// Test helpers for inline trap unit tests within this crate.
/// Gated behind #[cfg(test)] so it does NOT ship in the production library.
#[cfg(test)]
pub(crate) mod test_helpers;
