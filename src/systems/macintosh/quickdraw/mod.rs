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
//! - `raster` — region-row algebra, shape spans and pattern rectangle
//!   fills shared by the 68K trap path and the PowerPC import path.

pub(crate) mod color;

/// The size of a `PixPat` record, and the offset of `pat1Data`, the bit
/// pattern drawn into a basic GrafPort. A `patType` of 0 makes the record a
/// basic bit pattern. Imaging With QuickDraw (1994), p. 4-58.
pub(crate) const PIXPAT_RECORD_SIZE: u32 = 28;
pub(crate) const PIXPAT_PAT1_DATA_OFFSET: u32 = 20;
pub mod fonts;
pub mod gpu;
pub(crate) mod pixpat;
pub(crate) mod raster;
pub(crate) mod routines;
pub mod text;
