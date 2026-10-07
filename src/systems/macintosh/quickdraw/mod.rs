//! QuickDraw rendering primitives.
//!
//! Submodules cover the parts of QuickDraw that are not directly
//! mapped to A-line traps in `crate::trap::quickdraw`:
//!
//! - [`fonts`] — original systemless bitmap fonts + heuristic family
//!   lookup for `GetFontName` / `GetFNum`, plus a runtime override
//!   path for embedders that want their own font set.
//! - [`text`] — software glyph rasteriser used by `DrawString`,
//!   `DrawText`, and friends. Reads the active font/style from
//!   the current `GrafPort` and writes pixel coverage directly
//!   into the framebuffer at the current pen location.
//! - `raster` — region-row algebra and shape spans shared by the
//!   68K trap path and the PowerPC import path.

pub mod fonts;
pub(crate) mod raster;
pub mod text;
