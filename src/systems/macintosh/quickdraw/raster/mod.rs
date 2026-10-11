//! Drawing code shared by the 68K trap path and the PowerPC import path.
//!
//! Everything here works on plain data: region rows as sorted endpoint lists,
//! shape spans as `(left, right)` pairs, and pixel images, clips and inks the
//! caller has already resolved. Each caller keeps its own ABI decoding,
//! region parser, malformed-region policy, colour resolution and memory
//! access.
//!
//! - [`region_rows`] — interval algebra over one region scanline.
//! - [`spans`] — oval and round-rect scanline spans.
//! - [`paint`] — rectangle fills with a bit pattern, clipped by rows.
//! - [`transfer`] — row kernels that expand a pattern into pixels.
//! - [`memory`] — the row writes the kernels draw through.

pub(crate) mod memory;
pub(crate) mod paint;
pub(crate) mod region_rows;
pub(crate) mod spans;
pub(crate) mod transfer;
