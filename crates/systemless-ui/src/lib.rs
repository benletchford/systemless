//! A modern design system built on egui, independent of the Systemless emulator.
//!
//! Apply [`Theme`] to an [`egui::Context`], compose standard egui widgets with
//! [`components`], and render a dockable workspace with [`workspace::show`].
//! The optional `gpu` feature exposes the matching egui-wgpu renderer; `desktop`
//! exposes eframe for standalone applications. Embedded hosts retain GPU ownership.
#![forbid(unsafe_code)]
pub mod components;
pub mod dialog;
pub mod editor;
pub mod file_picker;
pub mod menu;
pub mod range;
pub mod theme;
pub mod window;
pub mod workspace;
#[cfg(feature = "desktop")]
pub use eframe;
pub use egui;
pub use egui_dock;
pub use egui_extras;
#[cfg(feature = "gpu")]
pub use egui_wgpu;
#[cfg(feature = "gpu")]
pub use egui_wgpu::wgpu;
pub use theme::{Appearance, Density, Palette, Theme};
