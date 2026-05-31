//! QuickDraw rendering primitives.
//!
//! Submodules cover the parts of QuickDraw that are not directly
//! mapped to A-line traps in `crate::trap::quickdraw`:
//!
//! - [`fonts`] — built-in DejaVu font baking + heuristic family
//!   lookup for `GetFontName` / `GetFNum`, plus a runtime override
//!   path for embedders that want their own font set.
//! - [`text`] — software glyph rasteriser used by `DrawString`,
//!   `DrawText`, and friends. Reads the active font/style from
//!   the current `GrafPort` and writes pixel coverage directly
//!   into the framebuffer at the current pen location.

pub mod fonts;
pub mod text;
