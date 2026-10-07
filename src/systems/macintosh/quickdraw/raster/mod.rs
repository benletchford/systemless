//! Drawing code shared by the 68K trap path and the PowerPC import path.
//!
//! Everything here works on plain data: region rows as sorted endpoint lists
//! and shape spans as `(left, right)` pairs. Each caller keeps its own ABI
//! decoding, region parser, malformed-region policy and memory access.
//!
//! - [`region_rows`] — interval algebra over one region scanline.
//! - [`spans`] — oval and round-rect scanline spans.

pub(crate) mod region_rows;
pub(crate) mod spans;
