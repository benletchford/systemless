//! Authoritative catalogue schema, compiler and content-addressed asset pipeline.
#![forbid(unsafe_code)]

pub mod assets;
pub mod catalogue;
pub mod community;
pub mod model;
pub mod network;
pub mod r2;
pub mod validate;
pub mod yaml;

pub use catalogue::{build, load, parse_document, Catalogue, Document, Mode};
pub use model::*;

pub mod site;
